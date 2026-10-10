//! The Vision half of `automation-platform ocr-bench`. The pictures, the statistics and every
//! line it prints are the pure half, `crate::ocr::bench`, whose header says what it is for.
//!
//! A child of `ocr.rs`, so that what it times is that file's own code rather than a copy:
//! `recognize_noted` for a whole read, `new_request` and `perform` for one pass,
//! `content_margin`, `Plan::content` and `render` for the picture a pass is handed. Every variant
//! starts from the production request and changes one thing on it ([`request_for`]); every
//! strategy is a shape of the production ladder (`ocr/ladder.rs`). Nothing in the running
//! application calls this.
//!
//! What it does, in this order, each line written to the file the moment it is known, so that a
//! pass that kills the process keeps everything before it; each section ends with its duration:
//!
//! 1. **The machine**: model, processor, cores, memory, macOS, thermal state, low power mode,
//!    load, power source, VoiceOver; the text-recognition revisions this macOS has and the one a
//!    new request uses; the compute devices Core ML has (`MLAllComputeDevices`, looked up by
//!    name: a macOS before 14 does not have it, and importing it would stop the application from
//!    starting there) and the ones Vision offers text recognition, per stage.
//! 2. **First passes**, each in a process of its own (this executable again, `--child`): the
//!    application's warm-up over a line of words, the one it made until 2026-10 over six bars,
//!    one over a small field, or none, on a thread of its own, then the first real pass on another
//!    thread, which is what the recognise thread's first read is in the application, and the pass
//!    after it; the fast level's first pass with no warm-up; the first passes in a second language
//!    after the warm-up in Vision's default; and the neural recogniser's first recognitions — after
//!    its session alone, after its warm-up, and after its warm-up beside Vision's. One process
//!    first that is not counted, so that none of the counted ones meets a cold file cache, and the
//!    order turned each round.
//! 3. **Probes**: each variant that calls what no Mac has run for this application (a request
//!    revision, a compute device, the neural recogniser) runs one pass in a process of its own
//!    first. One that dies there, or that is refused, is left out of everything below; the neural
//!    recogniser is made in this process only after its probe went through.
//! 4. **The pipeline**: every picture through `recognize_noted` as `host.ocr.recognize` reads it
//!    — no language, the ladder's budget counted from a pretend capture — under each strategy, every
//!    row once a round in an order that changes from round to round, the few pictures of the plan
//!    read for speed and the rest twice, for whether they are read right. Each row says what a read
//!    cost, the Vision passes and the neural recogniser's runs it made, who answered, and what was
//!    not Vision.
//! 5. **The engine**: one pass of each variant over a few pictures, every cell once a round in an
//!    order that changes from round to round, the first sample of each cell kept apart. Then the
//!    closing lines: the cheapest strategy that reads right and saves time.
//! 6. **Threads**: the first pass on fresh threads once the process is warm, one thread alone
//!    against two at once, what switching between variants cost the engine, Vision beside the
//!    neural recogniser at two priorities, and four blocks of sustained load.
//! 7. **Idle**: one pass after 2, 10 and 30 seconds with nothing to do, on the thread that read
//!    before and on a fresh one; and the fast level and the neural recogniser after the same.
//!
//! Every Objective-C object is made on the thread that uses it (none of them is `Send`), and each
//! pass runs inside an autorelease pool of its own: neither the main thread of a command-line
//! process nor a thread started here has a run loop to drain one. Every thread that times a pass
//! asks for user-initiated quality of service first, as the application's recognise thread does.

use std::ffi::CStr;
use std::io::Read as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Barrier;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::sel;
use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_core_ml::MLComputeDeviceProtocol;
use objc2_foundation::{NSArray, NSProcessInfo, NSString};
use objc2_vision::VNRecognizeTextRequest;

use image::RgbImage;

use super::{
    cgimage_to_rgba, content_margin, intel_mac, new_request, paddle_crop, perform, picture_from_png, qos_word,
    recognize_noted, render, run_vision, supported_revisions, thread_qos, upscale_toward, warm_up_page, Ladder, Plan,
    ACCURATE, FAST, LADDER_BUDGET, REVISION_OVERRIDE, TARGET_CONTENT_PX,
};
use crate::backend::paddle_ocr::{self, Polled, Qos, IN_FLIGHT};
use crate::ocr::bench::{
    self as pure, Cell, Child, ChildResult, Device, Fixture, Grid, Idle, IdleEngine, Options, Out, PaddleChild,
    PaddleFirst, PaddleInput, PaddleStart, Pipeline, Probe, ReadNote, Sample, Stats, Strategy, Tweak, Warmup,
    STRATEGIES, VARIANTS,
};
use crate::ocr::cost::{Capture, Stage};
use crate::ocr::ladder::{Answer, Shape};
use crate::ocr::paddle_pre::Tighten;
use crate::ocr::policy::{SMALL_H, SMALL_W};

type Devices = Vec<Retained<ProtocolObject<dyn MLComputeDeviceProtocol>>>;

/// `automation-platform ocr-bench …`: the exit code.
pub fn run(args: &[String]) -> i32 {
    let opts = match pure::parse(args) {
        Ok(o) => o,
        Err(e) => {
            println!("{e}\n\n{}", pure::USAGE);
            return 2;
        }
    };
    if opts.help {
        println!("{}", pure::USAGE);
        return 0;
    }
    // The neural recogniser alone, as Windows measures it, and whether it loads at all.
    if opts.paddle_probe {
        return paddle_ocr::probe();
    }
    if opts.paddle {
        return paddle_ocr::bench_rows(&opts, None, out_path(&opts));
    }
    match opts.child {
        Some(Child::FirstPass(warmup)) => objc2::rc::autoreleasepool(|_| child(warmup)),
        Some(Child::FastFirst) => objc2::rc::autoreleasepool(|_| fast_first_child()),
        Some(Child::SecondLanguage) => objc2::rc::autoreleasepool(|_| second_language_child()),
        Some(Child::PaddleFirst(start)) => objc2::rc::autoreleasepool(|_| paddle_first_child(start)),
        Some(Child::Probe(v)) => objc2::rc::autoreleasepool(|_| probe_child(v)),
        None => objc2::rc::autoreleasepool(|_| bench(&opts)),
    }
}

// ── Quality of service ───────────────────────────────────────────────────────────────────────

const QOS_USER_INITIATED: u32 = 0x19;

/// A class a measuring thread was left with instead of user-initiated; `u32::MAX` while none was.
static QOS_MISSED: AtomicU32 = AtomicU32::new(u32::MAX);

/// This thread's quality of service, in words.
fn qos_here() -> String {
    qos_word(thread_qos())
}

/// What a thread that times a pass does first: ask for user-initiated, as the application's
/// recognise thread does (`thread_init`). One that is left with another class is remembered, and
/// the conditions at the end say so.
fn measuring_thread() {
    // SAFETY: a plain call about the calling thread.
    let rc = unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0) };
    let got = thread_qos();
    if rc != 0 || got != QOS_USER_INITIATED {
        QOS_MISSED.store(got, Ordering::Relaxed);
    }
}

// ── The pictures ─────────────────────────────────────────────────────────────────────────────

/// An image and the bitmap behind it.
struct Handed {
    image: CFRetained<CGImage>,
    // The bitmap `render` drew into: the image shares it copy-on-write, so it is dropped after the
    // image (fields drop in order), as `recognize_captured` keeps it.
    _buf: Option<Vec<u8>>,
}

/// A picture decoded, and what the production preprocessing hands Vision for it.
struct Picture {
    fixture: &'static Fixture,
    // Before `native`: fields drop in order, and for a large picture `prod` is `native`'s own
    // image, whose pixels live in `native`'s bitmap.
    prod: Handed,
    native: Handed,
    /// The blank guard answers it: no pass is ever made over it, so it is read only by the
    /// pipeline — whose reads the guard answers, as it answers the application's — and `prod` is
    /// the picture itself, which nothing hands Vision.
    blank: bool,
}

/// The PNG decoded by CoreGraphics, at the size its entry says, and drawn once into a bitmap of
/// its own, as the application's warm-up draws its picture (`picture_from_png`): a capture
/// reaches the pipeline as pixels, while an image backed by PNG data may be decoded again
/// whenever it is read.
fn decode(f: &'static Fixture) -> Result<Handed, String> {
    let (image, buf) = picture_from_png(f.png, f.px())?;
    Ok(Handed { image, _buf: Some(buf) })
}

/// What a pass over `native` is handed when the ink is enlarged toward `target` pixels: for a small
/// region the content crop as `recognize_captured` makes it (`content_margin`, `Plan::content`,
/// `render`), for a larger one the picture itself.
fn hand(native: &Handed, f: &Fixture, target: usize) -> Result<Handed, String> {
    if f.w_pt > SMALL_W || f.h_pt > SMALL_H {
        if target != TARGET_CONTENT_PX {
            return Err("a region this large is handed to Vision as it is, never enlarged".to_string());
        }
        return Ok(Handed { image: native.image.clone(), _buf: None });
    }
    let (rgba, w, h) =
        cgimage_to_rgba(&native.image).ok_or_else(|| "its pixels could not be read back".to_string())?;
    let mut plan = Plan::content(&rgba, w, h, content_margin(f.scale as f64));
    if plan.blank {
        return Err("the blank guard finds nothing to read in it".to_string());
    }
    if target != TARGET_CONTENT_PX {
        plan.up = upscale_toward(target, plan.ink_h);
    }
    let (image, buf) = render(&native.image, &plan).ok_or_else(|| "it could not be rendered".to_string())?;
    Ok(Handed { image, _buf: Some(buf) })
}

fn picture(f: &'static Fixture) -> Result<Picture, String> {
    let native = decode(f)?;
    if f.small() && blank(&native, f) {
        let prod = Handed { image: native.image.clone(), _buf: None };
        return Ok(Picture { fixture: f, prod, native, blank: true });
    }
    let prod = hand(&native, f, TARGET_CONTENT_PX)?;
    Ok(Picture { fixture: f, prod, native, blank: false })
}

/// Whether the blank guard answers a small picture: its content crop finds nothing in it, as in a
/// read (`Plan::content`).
fn blank(native: &Handed, f: &Fixture) -> bool {
    cgimage_to_rgba(&native.image)
        .is_some_and(|(rgba, w, h)| Plan::content(&rgba, w, h, content_margin(f.scale as f64)).blank)
}

/// What the neural recogniser is handed of a small picture, as top-down RGBA: the whole region,
/// or the content crop a read makes (`paddle_crop`). `Err` for a large picture, which production
/// never hands it, and for one the blank guard answers.
fn paddle_rgba(p: &Picture, input: PaddleInput) -> Result<(u32, u32, Vec<u8>), String> {
    let f = p.fixture;
    if !f.small() {
        return Err("production hands the neural recogniser small regions only".to_string());
    }
    let (rgba, w, h) =
        cgimage_to_rgba(&p.native.image).ok_or_else(|| "its pixels could not be read back".to_string())?;
    match input {
        PaddleInput::Raw => Ok((w as u32, h as u32, rgba)),
        PaddleInput::Crop => {
            let plan = Plan::content(&rgba, w, h, content_margin(f.scale as f64));
            if plan.blank {
                return Err("the blank guard finds nothing to read in it".to_string());
            }
            Ok(paddle_crop(&rgba, w, &plan))
        }
    }
}

/// [`paddle_rgba`] as the RGB the recogniser's preprocessing takes.
fn paddle_rgb(p: &Picture, input: PaddleInput) -> Result<RgbImage, String> {
    let (w, h, rgba) = paddle_rgba(p, input)?;
    let rgba = image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| "its pixels do not make a picture".to_string())?;
    Ok(RgbImage::from_fn(w, h, |x, y| {
        let q = rgba.get_pixel(x, y).0;
        image::Rgb([q[0], q[1], q[2]])
    }))
}

/// The crop in points the recogniser is given for a picture of this scale.
fn crop_for(f: &Fixture) -> Tighten {
    Tighten::for_scale(f.scale as f64)
}

/// One recognition of `p`'s content crop on the recogniser's own thread, as a read asks it, at
/// `qos`, waited for: what it read, and its milliseconds there (`PaddleRead::ms`) and from the ask
/// to the answer. `None` when it could not be asked or read nothing.
fn paddle_on_its_thread(p: &Picture, qos: Qos) -> Option<(paddle_ocr::PaddleRead, f64)> {
    let (w, h, rgba) = paddle_rgba(p, PaddleInput::Crop).ok()?;
    let t = Instant::now();
    let read = paddle_ocr::ask_with(w, h, &rgba, crop_for(p.fixture), qos)?.wait_read()?;
    Some((read, ms_since(t)))
}

/// The process's peak resident memory so far, in megabytes (`getrusage`; macOS gives bytes).
fn peak_memory_mb() -> Option<f64> {
    // SAFETY: a zeroed struct of the size `getrusage` fills.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: a valid pointer to it, about this process.
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    (rc == 0).then(|| usage.ru_maxrss as f64 / (1024.0 * 1024.0))
}

// ── One pass ─────────────────────────────────────────────────────────────────────────────────

fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// One pass over `image`, timed as `run_vision` spends it: making and configuring the request,
/// the handler, `performRequests` and reading the results out. `Err` when the request cannot be
/// made here; the second value is Vision's reason when it refused the pass.
fn timed(
    image: &CGImage,
    make: impl FnOnce() -> Result<Retained<VNRecognizeTextRequest>, String>,
) -> Result<(Sample, Option<String>), String> {
    objc2::rc::autoreleasepool(|_| {
        let t = Instant::now();
        let request = make()?;
        let r = perform(image, &request, &|_| (0, 0, 1, 1));
        let ms = ms_since(t);
        Ok(match r {
            Ok((text, _, _)) => (Sample { ms, text: Some(text), passes: 1, skipped: false }, None),
            Err(why) => (Sample { ms, text: None, passes: 1, skipped: false }, Some(why)),
        })
    })
}

/// The milliseconds of one pass of today's request, made for it as the application makes one per
/// pass.
fn prod_pass(image: &CGImage) -> f64 {
    match timed(image, || Ok(new_request(None, ACCURATE))) {
        Ok((s, _)) => s.ms,
        Err(_) => f64::NAN,
    }
}

/// The production request with the one change `tweak` names, and the stages it pinned.
fn request_for(tweak: Tweak) -> Result<(Retained<VNRecognizeTextRequest>, Vec<String>), String> {
    let request = match tweak {
        Tweak::Fast => new_request(None, FAST),
        Tweak::Lang(code) => new_request(Some(code), ACCURATE),
        _ => new_request(None, ACCURATE),
    };
    let mut pinned = Vec::new();
    match tweak {
        Tweak::Revision(r) => {
            // SAFETY: a property of a request made on this thread; the caller has checked that
            // this macOS lists the revision.
            unsafe { request.setRevision(r) };
            // SAFETY: as above.
            let kept = unsafe { request.revision() };
            if kept != r {
                return Err(format!("the request kept revision {kept}"));
            }
        }
        Tweak::MinTextHeight(h) => request.setMinimumTextHeight(h),
        Tweak::Device(d) => pinned = pin(&request, d)?,
        Tweak::None
        | Tweak::Control
        | Tweak::Fast
        | Tweak::Lang(_)
        | Tweak::Reuse
        | Tweak::TargetPx(_)
        | Tweak::Paddle(_) => {}
    }
    Ok((request, pinned))
}

/// The compute devices Vision offers this request, per stage (macOS 14 and later).
fn stage_devices(request: &VNRecognizeTextRequest) -> Result<Vec<(Retained<NSString>, Devices)>, String> {
    // On this objc2 a selector the running macOS lacks aborts the process rather than failing,
    // so it is asked first. Both calls this file makes arrived together, in macOS 14.
    if !request.respondsToSelector(sel!(supportedComputeStageDevicesAndReturnError:))
        || !request.respondsToSelector(sel!(setComputeDevice:forComputeStage:))
    {
        return Err("Vision names its compute devices from macOS 14 on".to_string());
    }
    // SAFETY: an instance method of a request made on this thread, present (asked above).
    let stages = unsafe { request.supportedComputeStageDevicesAndReturnError() }
        .map_err(|e| format!("Vision did not list them: {}", e.localizedDescription()))?;
    let (keys, values) = stages.to_vecs();
    Ok(keys.into_iter().zip(values).map(|(k, v)| (k, v.to_vec())).collect())
}

fn class_name(d: &ProtocolObject<dyn MLComputeDeviceProtocol>) -> String {
    let any: &AnyObject = d.as_ref();
    any.class().name().to_string_lossy().into_owned()
}

/// Which kind of device this is: by Core ML's three public classes where the runtime has them,
/// so that a private subclass still counts, and by the class's name otherwise.
fn kind(d: &ProtocolObject<dyn MLComputeDeviceProtocol>) -> Option<Device> {
    let classes = [
        (c"MLNeuralEngineComputeDevice", Device::NeuralEngine),
        (c"MLGPUComputeDevice", Device::Gpu),
        (c"MLCPUComputeDevice", Device::Cpu),
    ];
    for (name, k) in classes {
        if AnyClass::get(name).is_some_and(|c| d.isKindOfClass(c)) {
            return Some(k);
        }
    }
    Device::of_class(&class_name(d))
}

/// "CPU", "GPU", "Neural Engine", or the class name of a kind this does not know.
fn device_word(d: &ProtocolObject<dyn MLComputeDeviceProtocol>) -> String {
    kind(d).map(|k| k.word().to_string()).unwrap_or_else(|| class_name(d))
}

/// Pins every stage of `request` that offers a device of kind `want` to it, and names the stages.
fn pin(request: &VNRecognizeTextRequest, want: Device) -> Result<Vec<String>, String> {
    let mut pinned = Vec::new();
    for (stage, devices) in stage_devices(request)? {
        if let Some(d) = devices.iter().find(|d| kind(d) == Some(want)) {
            // SAFETY: a device Vision itself listed for this stage of this request.
            unsafe { request.setComputeDevice_forComputeStage(Some(d), &stage) };
            pinned.push(stage.to_string());
        }
    }
    if pinned.is_empty() {
        Err(format!("Vision offers text recognition no {} here", want.word()))
    } else {
        Ok(pinned)
    }
}

// ── 1. The machine ───────────────────────────────────────────────────────────────────────────

fn sysctl_u64(name: &CStr) -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: a NUL-terminated name and a buffer of the length given.
    let rc = unsafe {
        libc::sysctlbyname(name.as_ptr(), (&mut value as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0)
    };
    (rc == 0 && len == std::mem::size_of::<u64>()).then_some(value)
}

fn machine(out: &mut Out) {
    use super::super::perm::{sysctl_i32, sysctl_string};
    let s = |name: &str| sysctl_string(name).unwrap_or_else(|| "?".to_string());
    let n = |name: &str| sysctl_i32(name).map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
    let translated = match sysctl_i32("sysctl.proc_translated") {
        Some(1) => ", translated by Rosetta",
        _ => "",
    };
    let levels = match (sysctl_i32("hw.perflevel0.physicalcpu"), sysctl_i32("hw.perflevel1.physicalcpu")) {
        (Some(p), Some(e)) => format!(" ({p} performance, {e} efficiency)"),
        _ => String::new(),
    };
    let memory = sysctl_u64(c"hw.memsize").map(|b| format!(", {} GB", b >> 30)).unwrap_or_default();
    out.say(&format!(
        "machine: {}, \"{}\", the {} slice{translated}, {} cores{levels} and {} threads{memory}",
        s("hw.model"),
        s("machdep.cpu.brand_string"),
        std::env::consts::ARCH,
        n("hw.physicalcpu"),
        n("hw.logicalcpu"),
    ));
    out.say(&format!("system: macOS {}", NSProcessInfo::processInfo().operatingSystemVersionString()));
    out.say(&format!(
        "quality of service: every thread that times a pass asks for user-initiated, as the application's \
         recognise thread does; this thread has {}",
        qos_here()
    ));
}

/// The first line of `pmset -g batt`: "Now drawing from 'AC Power'" or 'Battery Power'.
fn power_source() -> String {
    Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "?".to_string())
}

/// Whether VoiceOver runs: it speaks, and on a two-core laptop that is load of its own.
fn voiceover() -> &'static str {
    match Command::new("/usr/bin/pgrep")
        .args(["-x", "VoiceOver"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(s) if s.success() => "running",
        Ok(_) => "not running",
        Err(_) => "?",
    }
}

fn conditions(out: &mut Out, when: &str) {
    let info = NSProcessInfo::processInfo();
    let thermal = match info.thermalState().0 {
        0 => "nominal",
        1 => "fair",
        2 => "serious",
        3 => "critical",
        _ => "unknown",
    };
    let low_power = if info.isLowPowerModeEnabled() { "on" } else { "off" };
    let mut load = [0f64; 3];
    // SAFETY: a buffer of three doubles, as asked for.
    let got = unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
    let load = if got >= 1 { format!("{:.2}", load[0]) } else { "?".to_string() };
    let qos = match QOS_MISSED.load(Ordering::Relaxed) {
        u32::MAX => "every measuring thread so far got user-initiated".to_string(),
        other => format!("a measuring thread was left at {} instead of user-initiated", qos_word(other)),
    };
    out.say(&format!(
        "conditions {when}: thermal state {thermal}, low power mode {low_power}, load average {load} (1 min), \
         power: {}, VoiceOver {}, quality of service: {qos}",
        power_source(),
        voiceover()
    ));
}

/// Core ML's own list of compute devices, looked up by name: `MLAllComputeDevices` exists from
/// macOS 14, and an import of it would stop the application from starting on 12 and 13.
fn all_compute_devices() -> Option<Vec<String>> {
    type AllDevices = unsafe extern "C-unwind" fn() -> *mut NSArray<ProtocolObject<dyn MLComputeDeviceProtocol>>;
    // SAFETY: RTLD_DEFAULT with a NUL-terminated name; dlsym only reads.
    let mut p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"MLAllComputeDevices".as_ptr()) };
    if p.is_null() {
        // Core ML is loaded with Vision, which uses it; this covers a macOS where it is not yet.
        // SAFETY: a NUL-terminated path; the handle is kept for the life of the process.
        let h = unsafe { libc::dlopen(c"/System/Library/Frameworks/CoreML.framework/CoreML".as_ptr(), libc::RTLD_LAZY) };
        if !h.is_null() {
            // SAFETY: as above.
            p = unsafe { libc::dlsym(h, c"MLAllComputeDevices".as_ptr()) };
        }
    }
    if p.is_null() {
        return None;
    }
    // SAFETY: the address of `MLAllComputeDevices`, whose C signature is the type above.
    let all = unsafe { std::mem::transmute::<*mut libc::c_void, AllDevices>(p) };
    // SAFETY: not a Create or Copy function, so the array comes back autoreleased; it is retained
    // here, inside the caller's pool.
    let list = unsafe { Retained::retain_autoreleased(all()) }?;
    Some(list.to_vec().iter().map(|d| device_word(d)).collect())
}

/// The vision lines, each written before the next call is made, so that a call that kills the
/// process is the one after the last line; returns the revisions this macOS lists.
fn vision(out: &mut Out) -> Vec<usize> {
    let revisions = supported_revisions();
    // SAFETY: a property of a request made on this thread.
    let default = unsafe { new_request(None, ACCURATE).revision() };
    let langs = super::languages();
    out.say(&format!(
        "vision: text recognition revisions {} on this macOS; a new request uses revision {default}; \
         the accurate level reads {} languages, the fast level {}",
        if revisions.is_empty() {
            "(none listed)".to_string()
        } else {
            revisions.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(", ")
        },
        langs.available.len(),
        langs.fast.len()
    ));
    match all_compute_devices() {
        Some(d) => out.say(&format!("compute devices, Core ML: {}", d.join(", "))),
        None => out.say("compute devices, Core ML: this macOS has no MLAllComputeDevices (it arrived in 14)"),
    }
    match stage_devices(&new_request(None, ACCURATE)) {
        Ok(stages) => out.say(&format!(
            "compute devices, Vision's text recognition: {}",
            stages
                .iter()
                .map(|(stage, d)| format!(
                    "{stage}: {}",
                    d.iter().map(|d| device_word(d)).collect::<Vec<_>>().join(", ")
                ))
                .collect::<Vec<_>>()
                .join("; ")
        )),
        Err(why) => out.say(&format!("compute devices, Vision's text recognition: {why}")),
    }
    revisions
}

// ── Children ─────────────────────────────────────────────────────────────────────────────────

/// This executable again, as `ocr-bench --child <role>`: its standard output when it ended
/// normally, or what became of it — a death after it had printed its answer said as one at exit
/// ([`pure::child_ended`]), and so is a hang there, which the limit ends
/// ([`pure::child_timed_out`]).
fn run_child(exe: &Path, role: Child) -> Result<String, String> {
    const LIMIT_S: u64 = 120;
    let mut child = Command::new(exe)
        .arg("ocr-bench")
        .arg("--child")
        .arg(role.arg())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("could not be started: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(LIMIT_S);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // What it printed before it was ended: the pipe holds it, and is at its end now.
                let mut text = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut text);
                }
                return Err(pure::child_timed_out(LIMIT_S, &text, pure::child_read(role, &text)));
            }
            Err(e) => return Err(format!("could not be waited for: {e}")),
        }
    };
    let mut text = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut text);
    }
    pure::child_ended(status.signal(), status.code(), &text, pure::child_read(role, &text))
}

/// A warm-up on a thread of its own, waited for: one accurate pass with no language, over the
/// application's picture (`warm_up_page`, a line of printed words), over the six bars it read
/// until 2026-10, or over a small field through the content crop a read makes. Its time, and
/// whether it read anything. The thread asks for no quality of service, as the application's does
/// not.
fn warm_up_on_own_thread(kind: Warmup) -> (f64, bool) {
    std::thread::spawn(move || {
        objc2::rc::autoreleasepool(|_| {
            let t = Instant::now();
            // Each picture with the bitmap behind it, both kept until the pass is done (`render`).
            let page = match kind {
                Warmup::Word => warm_up_page().ok().map(|(img, buf, _)| (img, buf)),
                Warmup::Bars => bars_page(),
                Warmup::Field | Warmup::Nothing => None,
            };
            let field = match kind {
                Warmup::Field => pure::fixture(pure::WARM_UP_FIELD).and_then(|f| picture(f).ok()),
                _ => None,
            };
            let image: Option<&CGImage> = match (&field, &page) {
                (Some(pic), _) => Some(&pic.prod.image),
                (None, Some((img, _))) => Some(img),
                (None, None) => None,
            };
            let read = image.and_then(|img| run_vision(img, None, ACCURATE, Stage::WarmUp, &|_| (0, 0, 1, 1)));
            drop(field);
            drop(page);
            (ms_since(t), read.is_some_and(|(text, _, _)| !text.trim().is_empty()))
        })
    })
    .join()
    .unwrap_or((f64::NAN, false))
}

/// What the application's warm-up read until 2026-10: six dark bars on a light ground, 240x64 px,
/// so that the text detector would pass something to the recogniser. Kept here for the first-pass
/// comparison only — whether bars warm the recogniser at all is one of its questions.
fn bars_page() -> Option<(CFRetained<CGImage>, Vec<u8>)> {
    let (w, h) = (240usize, 64usize);
    let bytes_per_row = w * 4;
    let mut buf = vec![0u8; bytes_per_row * h];
    let space = CGColorSpace::new_device_rgb()?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: as in `render` — the buffer is sized to the geometry and outlives the context.
    let ctx = unsafe {
        CGBitmapContextCreate(buf.as_mut_ptr() as *mut core::ffi::c_void, w, h, 8, bytes_per_row, Some(&space), info)
    }?;
    CGContext::set_rgb_fill_color(Some(&ctx), 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(Some(&ctx), CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as CGFloat, h as CGFloat)));
    CGContext::set_rgb_fill_color(Some(&ctx), 0.05, 0.05, 0.05, 1.0);
    for i in 0..6 {
        CGContext::fill_rect(
            Some(&ctx),
            CGRect::new(CGPoint::new(24.0 + i as CGFloat * 32.0, 16.0), CGSize::new(8.0, 32.0)),
        );
    }
    CGContext::flush(Some(&ctx));
    let image = CGBitmapContextCreateImage(Some(&ctx))?;
    drop(ctx);
    Some((image, buf))
}

/// The probe picture, for a child.
fn probe_picture() -> Option<Picture> {
    pure::fixture(pure::PROBE).and_then(|f| picture(f).ok())
}

/// A first-pass child: the warm-up asked for, then the first two real passes on this thread, each
/// with its request made inside the clock, as the application's first read makes one.
fn child(warmup: Warmup) -> i32 {
    measuring_thread();
    let Some(pic) = probe_picture() else {
        println!("OCR BENCH CHILD: failed: the picture could not be prepared");
        return 1;
    };
    let warm = (warmup != Warmup::Nothing).then(|| warm_up_on_own_thread(warmup));
    let first = prod_pass(&pic.prod.image);
    let second = prod_pass(&pic.prod.image);
    println!("{}", pure::child_line(&ChildResult { warmup: warm, first, second }));
    0
}

/// The fast level's first two passes in a process that has made no pass of any kind.
fn fast_first_child() -> i32 {
    measuring_thread();
    let Some(pic) = probe_picture() else {
        println!("OCR BENCH CHILD: failed: the picture could not be prepared");
        return 1;
    };
    let fast = |image: &CGImage| timed(image, || Ok(new_request(None, FAST))).map_or(f64::NAN, |(s, _)| s.ms);
    let first = fast(&pic.prod.image);
    let second = fast(&pic.prod.image);
    println!("{}", pure::child_line(&ChildResult { warmup: None, first, second }));
    0
}

/// The application's warm-up in Vision's default language, then the first two passes in
/// [`pure::SECOND_LANGUAGE`]: whether a first pass in another language costs a first pass again.
fn second_language_child() -> i32 {
    measuring_thread();
    let Some(pic) = probe_picture() else {
        println!("OCR BENCH CHILD: failed: the picture could not be prepared");
        return 1;
    };
    let warm = warm_up_on_own_thread(Warmup::Word);
    let lang = |image: &CGImage| {
        timed(image, || Ok(new_request(Some(pure::SECOND_LANGUAGE), ACCURATE))).map_or(f64::NAN, |(s, _)| s.ms)
    };
    let first = lang(&pic.prod.image);
    let second = lang(&pic.prod.image);
    println!("{}", pure::child_line(&ChildResult { warmup: Some(warm), first, second }));
    0
}

/// The application's neural-recogniser warm-up (`paddle_ocr::warm_now`: the session made, one run
/// over a dummy) on a thread of its own, waited for: its milliseconds.
fn paddle_warm_up_on_own_thread(start: Option<&Barrier>) -> f64 {
    std::thread::scope(|s| {
        s.spawn(|| {
            if let Some(b) = start {
                b.wait();
            }
            let t = Instant::now();
            paddle_ocr::warm_now();
            ms_since(t)
        })
        .join()
        .unwrap_or(f64::NAN)
    })
}

/// A neural recogniser's first-pass child: its start, then its first two recognitions of
/// `lone-1@2x`'s content crop on its own thread, as a read asks them, and the process's peak
/// memory before and after.
fn paddle_first_child(start: PaddleStart) -> i32 {
    measuring_thread();
    let no = |why: String| {
        println!("{}", pure::paddle_child_line(&PaddleFirst::No(why)));
        0
    };
    let (Some(lone), Some(probe)) = (pure::fixture("lone-1@2x").and_then(|f| picture(f).ok()), probe_picture()) else {
        return no("the pictures could not be prepared".to_string());
    };
    let mem_before = peak_memory_mb();
    let (begun, vision, vision_first) = match start {
        PaddleStart::Nothing => match paddle_ocr::init_timed() {
            Ok(ms) => (ms, None, None),
            Err(why) => return no(format!("the neural recogniser is not available: {why}")),
        },
        PaddleStart::Warm => (paddle_warm_up_on_own_thread(None), None, None),
        PaddleStart::WithVision => {
            let both = Barrier::new(2);
            let (paddle, vision) = std::thread::scope(|s| {
                let paddle = s.spawn(|| paddle_warm_up_on_own_thread(Some(&both)));
                let vision = s.spawn(|| {
                    both.wait();
                    warm_up_on_own_thread(Warmup::Word).0
                });
                (paddle.join().unwrap_or(f64::NAN), vision.join().unwrap_or(f64::NAN))
            });
            (paddle, Some(vision), Some(prod_pass(&probe.prod.image)))
        }
    };
    if let Err(why) = paddle_ocr::init_timed() {
        return no(format!("the neural recogniser is not available: {why}"));
    }
    let (Some((first, first_ms)), Some((_, second_ms))) =
        (paddle_on_its_thread(&lone, Qos::Interactive), paddle_on_its_thread(&lone, Qos::Interactive))
    else {
        return no("the recogniser read nothing in lone-1@2x".to_string());
    };
    let right = pure::accepted(&first.text, lone.fixture.accept);
    println!(
        "{}",
        pure::paddle_child_line(&PaddleFirst::Ran(PaddleChild {
            start: begun,
            vision,
            first: first_ms,
            second: second_ms,
            vision_first,
            mem_before,
            mem_after: peak_memory_mb(),
            right,
        }))
    );
    0
}

/// A probe child: one pass of variant `v` over the probe picture — for the neural recogniser, the
/// recogniser made and run once over its content crop.
fn probe_child(v: usize) -> i32 {
    measuring_thread();
    let Some(pic) = probe_picture() else {
        println!("OCR BENCH CHILD: failed: the picture could not be prepared");
        return 1;
    };
    let probe = match VARIANTS[v].tweak {
        Tweak::Paddle(input) => match paddle_ocr::init_timed() {
            Err(why) => Probe::No(format!("the neural recogniser is not available: {why}")),
            Ok(_) => match paddle_rgb(&pic, input).map(|rgb| paddle_ocr::recognize_timed(&rgb, crop_for(pic.fixture))) {
                Ok(Some(_)) => Probe::Ran { pinned: Vec::new() },
                Ok(None) => Probe::No("the session was made but could not run the model".to_string()),
                Err(why) => Probe::No(why),
            },
        },
        tweak => match request_for(tweak) {
            Err(why) => Probe::No(why),
            Ok((request, pinned)) => match perform(&pic.prod.image, &request, &|_| (0, 0, 1, 1)) {
                Ok(_) => Probe::Ran { pinned },
                Err(why) => Probe::No(format!("Vision refused it: {why}")),
            },
        },
    };
    println!("{}", pure::probe_line(&probe));
    0
}

// ── 2. First passes, one process each ────────────────────────────────────────────────────────

fn first_pass_text(r: &ChildResult) -> String {
    let warm = match r.warmup {
        Some((ms, read)) => {
            format!("warm-up {} ms, {}", pure::ms(ms), if read { "it read text" } else { "it read nothing" })
        }
        None => "no warm-up".to_string(),
    };
    format!("{warm}; first real pass {} ms, the next {} ms", pure::ms(r.first), pure::ms(r.second))
}

fn paddle_first_text(r: &PaddleFirst) -> String {
    match r {
        PaddleFirst::No(why) => format!("not measured: {why}"),
        PaddleFirst::Ran(c) => {
            let vision = match (c.vision, c.vision_first) {
                (Some(w), Some(f)) => format!(
                    "; Vision's word warm-up beside it {} ms, Vision's first real pass after both {} ms",
                    pure::ms(w),
                    pure::ms(f)
                ),
                _ => String::new(),
            };
            let mem = match (c.mem_before, c.mem_after) {
                (Some(a), Some(b)) => format!("; peak memory {a:.0} MB before, {b:.0} MB after"),
                _ => String::new(),
            };
            format!(
                "start {} ms{vision}; first recognition of lone-1@2x {} ms ({}), the next {} ms{mem}",
                pure::ms(c.start),
                pure::ms(c.first),
                if c.right { "read right" } else { "NOT read right" },
                pure::ms(c.second)
            )
        }
    }
}

struct FirstPasses {
    /// The process that is not counted, which meets the cold file cache.
    uncounted: Option<ChildResult>,
    /// The Vision kinds' results — the warm-ups, the fast level, the second language — by role.
    by_kind: Vec<(Child, Vec<ChildResult>)>,
    paddle: Vec<(PaddleStart, Vec<PaddleFirst>)>,
}

fn first_passes(out: &mut Out, plan: &pure::Plan, exe: &Path) -> FirstPasses {
    let roles = pure::first_roles(0);
    let mut got = FirstPasses {
        uncounted: None,
        by_kind: roles.iter().filter(|c| !matches!(c, Child::PaddleFirst(_))).map(|c| (*c, Vec::new())).collect(),
        paddle: PaddleStart::ALL.iter().map(|p| (*p, Vec::new())).collect(),
    };
    out.say(&format!(
        "first passes: each in a process of its own — a warm-up on a thread of its own (over a line of words, six \
         bars, the small field {}, or none), then the first real pass of {} on another thread and the pass after it, \
         each with its request made inside the clock; the fast level's first passes with no warm-up; the first passes \
         in {} after the word warm-up; and the neural recogniser's first two recognitions of lone-1@2x after its \
         session alone, after its warm-up, and after its warm-up beside Vision's. One process with the word warm-up \
         first, not counted, so that the counted ones find the file cache warm; then {} round(s), one process per \
         kind, the order turned by one each round",
        pure::WARM_UP_FIELD,
        pure::PROBE,
        pure::SECOND_LANGUAGE,
        plan.child_rounds
    ));
    match run_child(exe, Child::FirstPass(Warmup::Word)).and_then(|t| {
        pure::parse_child(&t).ok_or_else(|| format!("printed no result: {}", t.trim()))
    }) {
        Ok(r) => {
            out.say(&format!("first passes | not counted | warm-up word | {}", first_pass_text(&r)));
            got.uncounted = Some(r);
        }
        Err(why) => out.say(&format!("first passes | not counted | warm-up word | the process {why}")),
    }
    for round in 0..plan.child_rounds {
        let order = pure::first_roles(round);
        out.say(&format!(
            "first passes | round {} | {}",
            round + 1,
            order.iter().map(|c| c.word()).collect::<Vec<_>>().join(", then ")
        ));
        for role in order {
            let output = run_child(exe, role);
            if let Child::PaddleFirst(start) = role {
                let result = output
                    .and_then(|t| pure::parse_paddle_child(&t).ok_or_else(|| format!("printed no result: {}", t.trim())));
                match result {
                    Ok(r) => {
                        out.say(&format!("first passes | {} | {}", role.word(), paddle_first_text(&r)));
                        if let Some((_, list)) = got.paddle.iter_mut().find(|(p, _)| *p == start) {
                            list.push(r);
                        }
                    }
                    Err(why) => out.say(&format!("first passes | {} | not measured: the process {why}", role.word())),
                }
                continue;
            }
            match output.and_then(|t| pure::parse_child(&t).ok_or_else(|| format!("printed no result: {}", t.trim()))) {
                Ok(r) => {
                    out.say(&format!("first passes | {} | {}", role.word(), first_pass_text(&r)));
                    if let Some((_, list)) = got.by_kind.iter_mut().find(|(c, _)| *c == role) {
                        list.push(r);
                    }
                }
                Err(why) => out.say(&format!("first passes | {} | not measured: the process {why}", role.word())),
            }
        }
    }
    got
}

// ── 3. Probes ────────────────────────────────────────────────────────────────────────────────

/// What became of each probed variant, by its index in `VARIANTS`: `Err` when its process did not
/// come back with an answer. A revision this macOS does not list is not tried.
type Probes = Vec<(usize, Result<Probe, String>)>;

fn probes(out: &mut Out, exe: &Path, revisions: &[usize]) -> Probes {
    out.say(
        "probes: each variant that calls what no Mac has run for this application yet (a request revision, a \
         compute device, the neural recogniser) runs one pass in a process of its own first; one that dies there, \
         or that is refused, is left out of everything below",
    );
    let mut got = Vec::new();
    for (v, variant) in VARIANTS.iter().enumerate() {
        if !variant.tweak.probed() {
            continue;
        }
        if let Tweak::Revision(r) = variant.tweak {
            if !revisions.contains(&r) {
                out.say(&format!("probe | {} | not tried: this macOS has no revision {r}", variant.name));
                continue;
            }
        }
        let result = run_child(exe, Child::Probe(v))
            .and_then(|t| pure::parse_probe(&t).ok_or_else(|| format!("printed no result: {}", t.trim())));
        match &result {
            Ok(Probe::Ran { pinned }) if pinned.is_empty() => {
                out.say(&format!("probe | {} | one pass went through", variant.name))
            }
            Ok(Probe::Ran { pinned }) => out.say(&format!(
                "probe | {} | one pass went through, the request pinned at {}",
                variant.name,
                pinned.join(", ")
            )),
            Ok(Probe::No(why)) => out.say(&format!("probe | {} | not run: {why}", variant.name)),
            Err(why) => out.say(&format!("probe | {} | the process that tried it {why}", variant.name)),
        }
        got.push((v, result));
    }
    got
}

/// Whether variant `v` went through its probe; `Err` says why not.
fn probed_ok(probes: &Probes, v: usize) -> Result<(), String> {
    match probes.iter().find(|(i, _)| *i == v).map(|(_, r)| r) {
        Some(Ok(Probe::Ran { .. })) => Ok(()),
        Some(Ok(Probe::No(why))) => Err(why.clone()),
        Some(Err(why)) => Err(format!("the process that tried it {why}")),
        None => Err("it was not tried in a process of its own".to_string()),
    }
}

// ── 4. The pipeline ──────────────────────────────────────────────────────────────────────────

/// One read of a picture through `recognize_noted` under strategy `s`, with `shape` its shape on
/// this Mac (`Strategy::shape_here`), as `host.ocr.recognize` reads a region once its capture is
/// in hand: no language, the ladder's budget counted from `capture` before the read, where the
/// application's starts before its capture. The time reported is the read's alone. Whatever the
/// neural recogniser still runs for the read — a region a Vision pass answered first is left to
/// finish unread, as on Windows — is waited out afterwards, outside the clock, so that each read
/// is timed alone and its runs are all counted.
fn read_through_pipeline(p: &Picture, s: &Strategy, shape: Shape, capture: Duration) -> (Sample, ReadNote) {
    objc2::rc::autoreleasepool(|_| {
        let f = p.fixture;
        // The ladder a read climbs on the recognise thread with nothing waiting behind it, in a
        // language the fast model reads, without the pass made only for the counts.
        let ladder = Ladder { fast_ok: true, preempt: None, counts: false, shape };
        let revision = s.revision_all.map(RevisionOverride::set);
        let runs = paddle_ocr::runs_started();
        let t = Instant::now();
        let started = t.checked_sub(capture).unwrap_or(t);
        let r = recognize_noted(
            &p.native.image,
            f.scale as f64,
            0,
            0,
            f.w_pt,
            f.h_pt,
            None,
            started,
            Capture::Took(capture.as_secs_f64() * 1000.0),
            false,
            &ladder,
        );
        let ms = ms_since(t);
        drop(revision);
        IN_FLIGHT.wait_idle(Duration::from_secs(2));
        let paddle_runs = paddle_ocr::runs_started() - runs;
        match r {
            Ok((text, note)) => (
                Sample { ms, text: Some(text.text), passes: note.passes.len() as u64, skipped: text.skipped },
                ReadNote {
                    passes: note.passes.len(),
                    vision_ms: note.passes.iter().map(|p| p.ms).sum(),
                    paddle_runs,
                    answer: note.answer,
                    agreed: note.agreed,
                    today: note.today,
                },
            ),
            Err(_) => (
                Sample { ms, text: None, passes: 0, skipped: false },
                ReadNote { passes: 0, vision_ms: 0.0, paddle_runs, answer: Answer::Nobody, agreed: None, today: false },
            ),
        }
    })
}

/// What a strategy needs that this run cannot give it — a revision this macOS lacks or whose
/// probe did not go through, the neural recogniser where it did not load — or `None`.
fn strategy_gate(s: &Strategy, revisions: &[usize], probes: &Probes, paddle: &Result<f64, String>) -> Option<String> {
    if let Some(r) = s.revision() {
        if !revisions.contains(&r) {
            return Some(format!("this macOS has no revision {r}"));
        }
        if let Some(v) = pure::variant(&format!("rev{r}")) {
            if let Err(why) = probed_ok(probes, v) {
                return Some(format!("revision {r} did not go through its probe: {why}"));
            }
        }
    }
    if s.paddle() {
        if let Err(why) = paddle {
            return Some(format!("the neural recogniser is not available: {why}"));
        }
    }
    None
}

/// Every strategy over every picture, interleaved: each round visits every row in an order that
/// changes from round to round, as the engine's cells, so that the load drifting over the minutes
/// of the section falls on every strategy alike. Each row is written as soon as it is done. With
/// it, the line saying which reading `prod` is on this Mac, for the summary page.
#[allow(clippy::too_many_arguments)]
fn pipeline(
    out: &mut Out,
    pictures: &[Picture],
    plan: &pure::Plan,
    capture: Duration,
    revisions: &[usize],
    probes: &Probes,
    paddle: &Result<f64, String>,
) -> (Pipeline, String) {
    let mut g = Pipeline::new(STRATEGIES.iter().collect(), pictures.iter().map(|p| p.fixture).collect(), plan);
    // The shape each strategy reads with here: prod's is the application's on this Mac.
    let (ready, intel) = (paddle.is_ok() && paddle_ocr::ready(), intel_mac());
    let shapes: Vec<Shape> = STRATEGIES.iter().map(|s| s.shape_here(ready, intel)).collect();
    let reads_as = pure::prod_reads_as(ready, intel);
    out.say(&reads_as);
    for (i, s) in STRATEGIES.iter().enumerate() {
        out.say(&format!("strategy {} ({}): {}", s.name, s.role.word(), s.what));
        if let Some(why) = strategy_gate(s, revisions, probes, paddle) {
            out.say(&format!("pipeline | {} | not measured: {why}", s.name));
            g.skip(i, &why);
        }
    }
    // `prod` is the first strategy (`STRATEGIES`).
    if let Some(why) = g.without_old_where_prod_is_old(shapes[0]) {
        out.say(&format!("pipeline | old | not measured: {why}"));
    }
    let speed: Vec<String> = g.rows[0].iter().filter(|r| r.speed).map(|r| r.picture.label()).collect();
    out.say(&format!(
        "pipeline: every picture as host.ocr.recognize reads it once the capture is in hand, under each of {} \
         strategies; {} read {} times after a first read kept apart, for a verdict on its speed against prod ({}), \
         {}; the ladder's {} ms budget counted from {} ms before each read for the capture it does not make \
         (--capture-ms); a large picture under every strategy but rev2 is prod's; every row once a round in an \
         order that changes from round to round; this thread at {}",
        STRATEGIES.len(),
        speed.len(),
        plan.samples,
        speed.join(", "),
        if plan.every_picture {
            "every other picture read twice, for whether it reads it right"
        } else {
            "no other picture (a quick run)"
        },
        LADDER_BUDGET.as_millis(),
        capture.as_millis(),
        qos_here()
    ));
    let (ns, np) = (g.strategies.len(), g.pictures.len());
    let mut printed = vec![vec![false; np]; ns];
    for round in 0..=plan.samples {
        let order = pure::order(g.count(), round);
        for k in order.cells {
            let (s, p) = g.at(k);
            if !g.rows[s][p].wants_more(plan) {
                continue;
            }
            let (sample, note) = read_through_pipeline(&pictures[p], g.strategies[s], shapes[s], capture);
            g.rows[s][p].push(sample, note);
            if !g.rows[s][p].wants_more(plan) {
                printed[s][p] = true;
                out.say(&g.rows[s][p].line());
            }
        }
    }
    for (rows, done) in g.rows.iter().zip(&printed) {
        for (row, done) in rows.iter().zip(done) {
            if !done && row.cell.skipped.is_none() {
                out.say(&row.line());
            }
        }
    }
    for s in 0..ns {
        for p in 0..np {
            if let Some(line) = g.verdict_line(s, p) {
                out.say(&line);
            }
        }
    }
    let (begun, ran, cancelled) = paddle_ocr::job_counts();
    if begun + cancelled > 0 {
        out.say(&format!(
            "pipeline | the neural recogniser's thread began {begun} regions, ran {ran} through its model to the end, \
             and gave up on {cancelled} that were no longer wanted (the content crop of a region a Vision pass had \
             answered first)"
        ));
    }
    (g, reads_as)
}

/// While it lives, every request made on this thread asks for this revision (`new_request`).
struct RevisionOverride;

impl RevisionOverride {
    fn set(r: usize) -> RevisionOverride {
        REVISION_OVERRIDE.with(|c| c.set(Some(r)));
        RevisionOverride
    }
}

impl Drop for RevisionOverride {
    fn drop(&mut self) {
        REVISION_OVERRIDE.with(|c| c.set(None));
    }
}

// ── 5. The engine ────────────────────────────────────────────────────────────────────────────

fn engine(
    out: &mut Out,
    pictures: &[Picture],
    plan: &pure::Plan,
    revisions: &[usize],
    probes: &Probes,
    paddle: &Result<f64, String>,
) -> Grid {
    let chosen: Vec<&Picture> = plan
        .engine_pictures
        .iter()
        .filter_map(|label| pictures.iter().find(|p| p.fixture.label() == *label && !p.blank))
        .collect();
    let mut grid = Grid::new(VARIANTS.iter().collect(), chosen.iter().map(|p| p.fixture).collect());
    let (nv, np) = (grid.variants.len(), grid.pictures.len());
    out.say(&format!(
        "engine: {nv} variants over {np} pictures ({}), {} passes a cell after a first one kept apart, every cell \
         once a round in an order that changes from round to round; a pass is making and configuring the request, \
         the handler, performRequests and reading the results out, as run_vision spends it (reuse: the one kept \
         request; the neural recogniser: its preparation and its model, on this thread); this thread at {}",
        grid.pictures.iter().map(|p| p.label()).collect::<Vec<_>>().join(", "),
        plan.samples,
        qos_here()
    ));
    // What a target variant and the neural recogniser are handed, per picture, made once; and
    // which cells have been printed.
    let mut alt: Vec<Vec<Option<Handed>>> = Vec::new();
    let mut neural: Vec<Vec<Option<RgbImage>>> = Vec::new();
    let mut printed = vec![vec![false; np]; nv];
    for (v, variant) in VARIANTS.iter().enumerate() {
        let why = match variant.tweak {
            Tweak::Revision(r) if !revisions.contains(&r) => Some(format!("this macOS has no revision {r}")),
            Tweak::Paddle(_) => probed_ok(probes, v).err().or_else(|| {
                paddle.as_ref().err().map(|e| format!("the neural recogniser is not available in this process: {e}"))
            }),
            t if t.probed() => probed_ok(probes, v).err(),
            _ => None,
        };
        let mut handed = Vec::new();
        let mut rgb = Vec::new();
        for (p, pic) in chosen.iter().enumerate() {
            if let Some(why) = &why {
                grid.cells[v][p] = Cell::skip(why.clone());
            }
            if let Tweak::TargetPx(t) = variant.tweak {
                match hand(&pic.native, pic.fixture, t) {
                    Ok(h) => handed.push(Some(h)),
                    Err(why) => {
                        grid.cells[v][p] = Cell::skip(why);
                        handed.push(None);
                    }
                }
            } else {
                handed.push(None);
            }
            match variant.tweak {
                Tweak::Paddle(input) if why.is_none() => match paddle_rgb(pic, input) {
                    Ok(image) => rgb.push(Some(image)),
                    Err(why) => {
                        grid.cells[v][p] = Cell::skip(why);
                        rgb.push(None);
                    }
                },
                _ => rgb.push(None),
            }
            if grid.cells[v][p].skipped.is_some() {
                printed[v][p] = true;
                out.say(&grid.line(v, p));
            }
        }
        alt.push(handed);
        neural.push(rgb);
    }
    // The `reuse` variant's request: one for the whole section, on this thread.
    let reused = new_request(None, ACCURATE);
    for round in 0..=plan.samples {
        let order = pure::order(grid.count(), round);
        out.say(&format!(
            "engine | round {} of {} | the {} cells from cell {} in steps of {} (cell = variant x pictures + picture, \
             both counted from 0 in the order above)",
            round + 1,
            plan.samples + 1,
            grid.count(),
            order.start,
            order.step
        ));
        for k in order.cells {
            let (v, p) = grid.at(k);
            if !grid.cells[v][p].wants_more(plan) {
                continue;
            }
            if grid.cells[v][p].first.is_none() {
                out.say(&format!("engine | running {} over {}", grid.variants[v].name, grid.pictures[p].label()));
            }
            let tweak = grid.variants[v].tweak;
            let image: &CGImage = match alt[v][p].as_ref() {
                Some(h) => &h.image,
                None => &chosen[p].prod.image,
            };
            let measured = match tweak {
                Tweak::Paddle(_) => match neural[v][p].as_ref() {
                    Some(rgb) => match paddle_ocr::recognize_timed(rgb, crop_for(chosen[p].fixture)) {
                        Some(s) => {
                            let ms = s.pre_ms + s.run_ms;
                            Ok((Sample { ms, text: Some(s.text), passes: 0, skipped: false }, None))
                        }
                        None => Err("the session could not run it".to_string()),
                    },
                    None => Err("nothing to hand the neural recogniser".to_string()),
                },
                Tweak::Reuse => timed(image, || Ok(reused.clone())),
                _ => timed(image, || request_for(tweak).map(|(r, _)| r)),
            };
            let cell = &mut grid.cells[v][p];
            match measured {
                Ok((sample, err)) => {
                    if cell.error.is_none() {
                        cell.error = err;
                    }
                    cell.push(sample);
                }
                Err(why) => cell.skipped = Some(why),
            }
            if !grid.cells[v][p].wants_more(plan) && !printed[v][p] {
                printed[v][p] = true;
                out.say(&grid.line(v, p));
            }
        }
    }
    for (v, row) in printed.iter().enumerate() {
        for (p, done) in row.iter().enumerate() {
            if !done {
                out.say(&grid.line(v, p));
            }
        }
    }
    out.say(&format!("rule: {}", pure::RULE));
    for v in 0..nv {
        for p in 0..np {
            if let Some(line) = grid.verdict_line(v, p) {
                out.say(&line);
            }
        }
    }
    if let Some(line) = grid.control_line() {
        out.say(&line);
    }
    grid
}

// ── 6. Threads ───────────────────────────────────────────────────────────────────────────────

/// One pass of today's request on a thread started for it, the thread's first.
fn pass_on_fresh_thread(pic: &Picture) -> f64 {
    std::thread::scope(|s| {
        s.spawn(|| {
            measuring_thread();
            prod_pass(&pic.prod.image)
        })
        .join()
    })
    .unwrap_or(f64::NAN)
}

/// `k` passes on this thread, and `k` passes on each of two threads at once. The per-pass times
/// and the wall-clock seconds of each.
fn pair(pic: &Picture, k: usize) -> ((Vec<f64>, f64), (Vec<f64>, f64)) {
    let image: &CGImage = &pic.prod.image;
    let t = Instant::now();
    let alone: Vec<f64> = (0..k).map(|_| prod_pass(image)).collect();
    let alone_wall = t.elapsed().as_secs_f64();
    let barrier = Barrier::new(2);
    let one = || {
        measuring_thread();
        // A first pass on the new thread, untimed: the question here is contention, not the
        // first pass on a thread, which `pass_on_fresh_thread` measures.
        let _ = prod_pass(image);
        barrier.wait();
        let t = Instant::now();
        let v: Vec<f64> = (0..k).map(|_| prod_pass(image)).collect();
        (v, t.elapsed().as_secs_f64())
    };
    let (a, b) = std::thread::scope(|s| {
        let a = s.spawn(one);
        let b = s.spawn(one);
        (a.join().unwrap_or_default(), b.join().unwrap_or_default())
    });
    let wall = a.1.max(b.1);
    let mut both = a.0;
    both.extend(b.0);
    ((alone, alone_wall), (both, wall))
}

/// One pass of the fast level over `image`, its request made inside the clock.
fn fast_pass(image: &CGImage) -> f64 {
    match timed(image, || Ok(new_request(None, FAST))) {
        Ok((s, _)) => s.ms,
        Err(_) => f64::NAN,
    }
}

/// Vision beside the neural recogniser: whether either slows the other. Over the probe picture,
/// `k` rounds, each one of every kind in an order turned by one each round — a Vision pass alone,
/// a recognition alone (asked on its thread, as a read asks it, and waited for), and a Vision pass
/// with a recognition of the same region asked on its thread the moment before, at user-initiated
/// and at utility quality of service — for the accurate level and for the fast one. Each beside
/// against itself alone by the engine's rule. The timing rows for the table.
fn beside_paddle(out: &mut Out, pic: &Picture, k: usize) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let Ok((w, h, rgba)) = paddle_rgba(pic, PaddleInput::Crop) else {
        out.say("threads | vision beside the neural recogniser | not measured: the probe picture has no content crop");
        return rows;
    };
    let crop = crop_for(pic.fixture);
    // A recognition on the recogniser's thread: its own milliseconds there.
    let alone = || {
        paddle_ocr::ask_with(w, h, &rgba, crop, Qos::Interactive).and_then(|a| a.wait_read()).map_or(f64::NAN, |r| r.ms)
    };
    for (level, pass) in [("accurate", prod_pass as fn(&CGImage) -> f64), ("fast", fast_pass as fn(&CGImage) -> f64)] {
        let image: &CGImage = &pic.prod.image;
        let (mut v_alone, mut p_alone) = (Vec::new(), Vec::new());
        let mut v_beside = [Vec::new(), Vec::new()];
        let mut p_beside = [Vec::new(), Vec::new()];
        let mut running_at_start = [0usize, 0];
        for round in 0..k {
            for kind in (0..4).map(|i| (i + round) % 4) {
                match kind {
                    0 => v_alone.push(pass(image)),
                    1 => p_alone.push(alone()),
                    q => {
                        let qos = if q == 2 { Qos::Interactive } else { Qos::Utility };
                        let i = q - 2;
                        let asked = paddle_ocr::ask_with(w, h, &rgba, crop, qos);
                        // Whether the recogniser had begun by the time the Vision pass did.
                        running_at_start[i] += usize::from(paddle_ocr::running());
                        v_beside[i].push(pass(image));
                        p_beside[i].push(asked.and_then(|a| a.wait_read()).map_or(f64::NAN, |r| r.ms));
                    }
                }
            }
        }
        let med = |v: &[f64]| median(v).map(pure::ms).unwrap_or_else(|| "?".into());
        out.say(&format!(
            "threads | {level} beside the neural recogniser | {} | Vision alone: median {} ms; the recogniser alone: \
             median {} ms",
            pure::PROBE,
            med(&v_alone),
            med(&p_alone)
        ));
        for (i, qos) in ["user-initiated", "utility"].iter().enumerate() {
            out.say(&format!(
                "threads | {level} beside the neural recogniser at {qos} | Vision {}; the recogniser {}; it had begun \
                 when the Vision pass began {} of {k} times",
                pure::against_alone(&v_alone, &v_beside[i]),
                pure::against_alone(&p_alone, &p_beside[i]),
                running_at_start[i]
            ));
            rows.push((format!("{level} pass beside the neural recogniser at {qos}, median"), med(&v_beside[i])));
            rows.push((format!("the neural recogniser beside the {level} pass, at {qos}, median"), med(&p_beside[i])));
        }
        rows.push((format!("{level} pass alone in that section, median"), med(&v_alone)));
        rows.push(("the neural recogniser alone in that section, median".to_string(), med(&p_alone)));
    }
    IN_FLIGHT.wait_idle(Duration::from_secs(2));
    rows
}

/// Sustained load: four blocks of `secs` seconds of back-to-back accurate passes over the probe
/// picture, alternately alone and with the neural recogniser asked about the same region before
/// each pass, at this thread's own class (`Qos::Reader`), as a read of the application asks it,
/// and its answer looked for after it, never waited for. Passes a second and the median per
/// block. The timing rows for the table.
fn sustained(out: &mut Out, pic: &Picture, secs: u64) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let neural = paddle_rgba(pic, PaddleInput::Crop).ok().filter(|_| paddle_ocr::ready());
    let image: &CGImage = &pic.prod.image;
    for block in 0..4 {
        let beside = block % 2 == 1;
        if beside && neural.is_none() {
            out.say(&format!(
                "sustained | block {} of 4, beside the neural recogniser | not measured: it is not available",
                block + 1
            ));
            continue;
        }
        let began = Instant::now();
        let mut passes = Vec::new();
        let (mut asked_n, mut answered_n) = (0usize, 0usize);
        while began.elapsed() < Duration::from_secs(secs) {
            let asked = neural
                .as_ref()
                .filter(|_| beside)
                .and_then(|(w, h, rgba)| paddle_ocr::ask_with(*w, *h, rgba, crop_for(pic.fixture), Qos::Reader));
            asked_n += usize::from(asked.is_some());
            passes.push(prod_pass(image));
            if let Some(a) = asked {
                answered_n += usize::from(a.try_wait() != Polled::NotYet);
            }
        }
        let wall = began.elapsed().as_secs_f64();
        let rate = passes.len() as f64 / wall.max(f64::MIN_POSITIVE);
        let med = median(&passes).map(pure::ms).unwrap_or_else(|| "?".into());
        let what = if beside { "beside the neural recogniser" } else { "Vision alone" };
        out.say(&format!(
            "sustained | block {} of 4, {what} | {} passes in {wall:.1} s, {rate:.1} a second, median {med} ms{}",
            block + 1,
            passes.len(),
            if beside {
                format!("; the recogniser had answered by the end of the pass {answered_n} of {asked_n} times")
            } else {
                String::new()
            }
        ));
        rows.push((
            format!("sustained load, block {} ({what}), median a pass; passes a second", block + 1),
            format!("{med}; {rate:.1}"),
        ));
    }
    IN_FLIGHT.wait_idle(Duration::from_secs(2));
    rows
}

// ── 7. Idle ──────────────────────────────────────────────────────────────────────────────────

/// Three warm passes of each engine, then one pass after each pause, each written as it comes (a
/// long-idle run takes over half an hour). Right before each pause one pass of the engine it is
/// for, not timed, on the thread that read before: the pauses of the three engines are taken in
/// turn, and without it the fast level, say, would have sat idle through the accurate pauses
/// before its own as well. A pause whose engine is not there — the neural recogniser where it did
/// not load — is not waited.
fn idle(out: &mut Out, pic: &Picture, plan: &pure::Plan) -> (Vec<f64>, Vec<(Idle, f64)>) {
    let image: &CGImage = &pic.prod.image;
    let paddle = || paddle_on_its_thread(pic, Qos::Interactive).map_or(f64::NAN, |(r, _)| r.ms);
    let warm: Vec<f64> = (0..3).map(|_| prod_pass(image)).collect();
    out.say(&format!("idle | {} | accurate, warm right before: {} ms", pure::PROBE, pure::ms_list(&warm)));
    let fast: Vec<f64> = (0..3).map(|_| fast_pass(image)).collect();
    out.say(&format!("idle | {} | fast, warm right before: {} ms", pure::PROBE, pure::ms_list(&fast)));
    let neural = paddle_ocr::ready();
    if neural {
        let p: Vec<f64> = (0..3).map(|_| paddle()).collect();
        out.say(&format!("idle | {} | the neural recogniser, warm right before: {} ms", pure::PROBE, pure::ms_list(&p)));
    }
    let mut after = Vec::new();
    for i in &plan.idle {
        if i.engine == IdleEngine::Paddle && !neural {
            out.say(&format!(
                "idle | {} | after {} s, {}: not measured, the neural recogniser is not available",
                pure::PROBE,
                i.secs,
                i.engine.word()
            ));
            continue;
        }
        let _ = match i.engine {
            IdleEngine::Accurate => prod_pass(image),
            IdleEngine::Fast => fast_pass(image),
            IdleEngine::Paddle => paddle(),
        };
        std::thread::sleep(Duration::from_secs(i.secs));
        let ms = match (i.engine, i.fresh) {
            (IdleEngine::Accurate, true) => pass_on_fresh_thread(pic),
            (IdleEngine::Accurate, false) => prod_pass(image),
            (IdleEngine::Fast, _) => fast_pass(image),
            (IdleEngine::Paddle, _) => paddle(),
        };
        out.say(&format!(
            "idle | {} | after {} s, {} on {}: {} ms",
            pure::PROBE,
            i.secs,
            i.engine.word(),
            i.thread(),
            pure::ms(ms)
        ));
        after.push((*i, ms));
    }
    (warm, after)
}

// ── The run ──────────────────────────────────────────────────────────────────────────────────

fn out_path(opts: &Options) -> std::path::PathBuf {
    if let Some(p) = &opts.out {
        return p.clone();
    }
    let base = crate::portable::base_dir();
    let dir = if crate::portable::is_writable(base) {
        base.to_path_buf()
    } else {
        let alt = crate::portable::fallback_dir();
        let _ = std::fs::create_dir_all(&alt);
        alt
    };
    pure::free_name(&dir)
}

fn median(v: &[f64]) -> Option<f64> {
    Stats::of(v).map(|s| s.median)
}

/// The run's summary page, appended to section by section; after the first failure it is left
/// alone, and the failure is said once.
struct Summary<'a> {
    path: Option<&'a Path>,
    failed: bool,
}

impl Summary<'_> {
    fn add(&mut self, out: &mut Out, markdown: &str) {
        let Some(path) = self.path else { return };
        if self.failed {
            return;
        }
        if let Err(e) = pure::append_summary(path, markdown) {
            self.failed = true;
            out.say(&format!("summary: {e}"));
        }
    }
}

fn bench(opts: &Options) -> i32 {
    let started = Instant::now();
    measuring_thread();
    let plan = pure::Plan::for_mode(opts.quick, opts.long_idle);
    let capture = Duration::from_millis(opts.capture_ms);
    let mut out = Out::open(out_path(opts), opts.quiet);
    if let Some(p) = &out.path {
        out.loud(&format!("writing to {}", p.display()));
    }
    out.say(&format!(
        "start: automation-platform ocr-bench, {}, {} mode, {} passes a cell after a first one kept apart, at most \
         {} s a cell once it has {}, {} s of idle",
        crate::build_info::log_line(env!("CARGO_PKG_VERSION")),
        if opts.quick { "quick" } else { "full" },
        plan.samples,
        plan.cell_cap.as_secs(),
        plan.min_samples,
        plan.idle_secs()
    ));
    let section = Instant::now();
    machine(&mut out);
    conditions(&mut out, "at the start");
    let revisions = vision(&mut out);
    for v in VARIANTS {
        out.say(&format!("variant {}: {}", v.name, v.what));
    }

    let mut pictures = Vec::new();
    for f in pure::all_pictures(opts) {
        match picture(f) {
            Ok(p) => {
                if p.blank {
                    out.say(&format!(
                        "picture {}: the blank guard answers it, so only the pipeline reads it, as a read would",
                        f.label()
                    ));
                }
                pictures.push(p);
            }
            Err(why) => out.say(&format!("picture {}: not usable: {why}", f.label())),
        }
    }
    let Some(probe) = pictures.iter().position(|p| p.fixture.label() == pure::PROBE) else {
        out.loud(&format!("stopped: the picture {} could not be prepared, so nothing can be compared", pure::PROBE));
        return 1;
    };
    out.say(&section_took("machine", section));

    let mut summary = Summary { path: opts.summary.as_deref(), failed: false };
    summary.add(
        &mut out,
        &format!(
            "### Text recognition on {} ({}, the {} slice, {} run)\n\n",
            super::super::perm::sysctl_string("hw.model").unwrap_or_else(|| "?".into()),
            NSProcessInfo::processInfo().operatingSystemVersionString(),
            std::env::consts::ARCH,
            if opts.quick { "quick" } else { "full" }
        ),
    );

    let (children, probed) = match std::env::current_exe() {
        Ok(exe) => {
            let section = Instant::now();
            let children = first_passes(&mut out, &plan, &exe);
            out.say(&section_took("first passes", section));
            let section = Instant::now();
            let probed = probes(&mut out, &exe, &revisions);
            out.say(&section_took("probes", section));
            (Some(children), probed)
        }
        Err(e) => {
            out.say(&format!("first passes and probes: not run, this executable cannot find itself: {e}"));
            (None, Vec::new())
        }
    };

    // The neural recogniser in this process, once its probe went through in one of its own.
    let section = Instant::now();
    let paddle: Result<f64, String> = match pure::variant("paddle-crop").map(|v| probed_ok(&probed, v)) {
        Some(Ok(())) => paddle_ocr::init_timed(),
        Some(Err(why)) => Err(why),
        None => Err("no probe for it".to_string()),
    };
    match &paddle {
        Ok(ms) => out.say(&format!(
            "paddle | the neural recogniser in this process: the session was made in {} ms: {}",
            pure::ms(*ms),
            paddle_ocr::engine_words()
        )),
        Err(why) => out.say(&format!("paddle | the neural recogniser is not measured in this run: {why}")),
    }
    out.say(&section_took("the neural recogniser", section));

    // This process's own first passes are not measured (the children did that); two untimed
    // passes make the sections below start warm, and one recognition the neural recogniser.
    for _ in 0..2 {
        let _ = prod_pass(&pictures[probe].prod.image);
    }
    if paddle.is_ok() {
        let _ = paddle_on_its_thread(&pictures[probe], Qos::Interactive);
    }

    let section = Instant::now();
    let (lines, reads_as) = pipeline(&mut out, &pictures, &plan, capture, &revisions, &probed, &paddle);
    let pipeline_table = lines.table();
    let strategies_table = lines.summary_table();
    out.say(
        "table: pipeline, the median ms of a whole read without the capture, as host.ocr.recognize makes it, one \
         column a strategy; \"faster\" or \"slower\" than prod by the rule where the picture was read for speed",
    );
    out.raw(&pipeline_table);
    out.say("table: what each strategy came to over every picture");
    out.raw(&strategies_table);
    // The application's reading on its own line, whatever the verdicts: what CI warns by. And the
    // ladder it read with before, beside it.
    let came_to: Vec<String> = [lines.prod_line(), lines.old_line()].into_iter().flatten().collect();
    for line in &came_to {
        out.say(line);
    }
    // The summary page says per Mac which reading prod was, and both lines, before the tables.
    summary.add(
        &mut out,
        &format!(
            "{reads_as}\n\n{}\n\nPipeline: the median ms of a whole read without the capture, one column a strategy; \
             \"faster\" or \"slower\" than prod by the rule where the picture was read for speed.\n\n{pipeline_table}\n\
             What each strategy came to.\n\n{strategies_table}\n",
            came_to.join("\n\n")
        ),
    );
    out.say(&section_took("pipeline", section));

    let section = Instant::now();
    let grid = engine(&mut out, &pictures, &plan, &revisions, &probed, &paddle);
    let engine_table = grid.table();
    out.say(
        "table: engine, median ms of one pass (the fastest to the slowest), against prod, and the verdict where \
         there is one; \"wrong\" = not every read was right",
    );
    out.raw(&engine_table);
    summary.add(
        &mut out,
        &format!(
            "Engine: median ms of one pass (the fastest to the slowest), against prod, and \"faster\" or \
             \"slower\" where the rule finds a clear difference. \"wrong\" = not every read was right.\n\n\
             {engine_table}\n"
        ),
    );
    out.say(&section_took("engine", section));

    // The closing lines: the pipeline's strategies, weighed by the rule, unless the engine's
    // control says this run is too noisy for any verdict.
    out.say(&format!("closing rule: {}", pure::CLOSING_RULE));
    let closing = pure::closing_lines(&lines.outcomes(), grid.control_loud());
    for line in &closing {
        out.say(line);
    }
    let listed = closing.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n");
    summary.add(&mut out, &format!("{listed}\n\n"));

    let section = Instant::now();
    let pic = &pictures[probe];
    out.say(&format!("threads: one pass of prod over {} at a time; this thread at {}", pure::PROBE, qos_here()));
    let fresh: Vec<f64> = (0..plan.fresh_threads).map(|_| pass_on_fresh_thread(pic)).collect();
    out.say(&format!(
        "threads | {} | first pass on a fresh thread, the process warm: {} ms",
        pure::PROBE,
        pure::ms_list(&fresh)
    ));
    let ((alone, alone_wall), (both, both_wall)) = pair(pic, plan.pair_passes);
    let rate = |n: usize, secs: f64| if secs > 0.0 { n as f64 / secs } else { f64::NAN };
    let (alone_med, both_med) = (median(&alone), median(&both));
    out.say(&format!(
        "threads | {} | one thread: median {} ms a pass, {:.1} passes a second; two threads at once: median {} ms a \
         pass, {:.1} passes a second together",
        pure::PROBE,
        alone_med.map(pure::ms).unwrap_or_else(|| "?".into()),
        rate(alone.len(), alone_wall),
        both_med.map(pure::ms).unwrap_or_else(|| "?".into()),
        rate(both.len(), both_wall)
    ));
    let interleaved = grid
        .variants
        .iter()
        .position(|v| v.tweak == Tweak::None)
        .zip(grid.pictures.iter().position(|p| p.label() == pure::PROBE))
        .and_then(|(v, p)| grid.cells[v][p].stats())
        .map(|s| s.median);
    out.say(&pure::switching_line(interleaved, alone_med));
    let mut beside_rows = Vec::new();
    match &paddle {
        Ok(_) => beside_rows = beside_paddle(&mut out, pic, plan.pair_passes),
        Err(why) => out.say(&format!("threads | vision beside the neural recogniser | not measured: {why}")),
    }
    out.say(&format!(
        "sustained: four blocks of {} s of back-to-back accurate passes over {}, alternately Vision alone and with the \
         neural recogniser asked about the same region before each pass, at this thread's own priority, as a read \
         asks it",
        plan.sustained_secs,
        pure::PROBE
    ));
    let sustained_rows = sustained(&mut out, pic, plan.sustained_secs);
    out.say(&section_took("threads", section));

    let section = Instant::now();
    out.say(&format!(
        "idle: one pass after each pause, in this order: {}; this thread at {}",
        plan.idle
            .iter()
            .map(|i| format!("{} s, {} on {}", i.secs, i.engine.word(), i.thread()))
            .collect::<Vec<_>>()
            .join("; "),
        qos_here()
    ));
    let (warm, after) = idle(&mut out, pic, &plan);
    out.say(&section_took("idle", section));
    conditions(&mut out, "at the end");

    let mut timing: Vec<(String, String)> = Vec::new();
    if let Some(children) = &children {
        if let Some(r) = &children.uncounted {
            timing.push((
                "first real pass in the process not counted (word warm-up, cold file cache)".into(),
                pure::ms(r.first),
            ));
        }
        for (role, results) in &children.by_kind {
            let label = match role {
                Child::FirstPass(Warmup::Bars) => "after the former warm-up over bars".to_string(),
                Child::FirstPass(Warmup::Word) => "after the application's warm-up (a line of words)".to_string(),
                Child::FirstPass(Warmup::Field) => format!("after a warm-up over the field {}", pure::WARM_UP_FIELD),
                Child::FirstPass(Warmup::Nothing) => "with no warm-up".to_string(),
                Child::FastFirst => "of the fast level, with no warm-up".to_string(),
                Child::SecondLanguage => format!("in {}, after the word warm-up in Vision's default", pure::SECOND_LANGUAGE),
                other => other.word(),
            };
            let warmups: Vec<f64> = results.iter().filter_map(|r| r.warmup.map(|w| w.0)).collect();
            if !warmups.is_empty() && !matches!(role, Child::SecondLanguage) {
                timing.push((format!("the warm-up itself, {}", role.word()), pure::ms_list(&warmups)));
            }
            let firsts: Vec<f64> = results.iter().map(|r| r.first).collect();
            let seconds: Vec<f64> = results.iter().map(|r| r.second).collect();
            timing.push((format!("first real pass in a process, {label}"), pure::ms_list(&firsts)));
            timing.push((format!("the pass after it, {label}"), pure::ms_list(&seconds)));
        }
        for (start, results) in &children.paddle {
            let ran: Vec<&PaddleChild> =
                results.iter().filter_map(|r| if let PaddleFirst::Ran(c) = r { Some(c) } else { None }).collect();
            if ran.is_empty() {
                continue;
            }
            let list = |f: &dyn Fn(&PaddleChild) -> Option<f64>| {
                pure::ms_list(&ran.iter().filter_map(|c| f(c)).collect::<Vec<_>>())
            };
            let what = match start {
                PaddleStart::Nothing => "the neural recogniser's session made",
                PaddleStart::Warm => "the neural recogniser's warm-up",
                PaddleStart::WithVision => "the neural recogniser's warm-up beside Vision's",
            };
            timing.push((what.to_string(), list(&|c| Some(c.start))));
            if *start == PaddleStart::WithVision {
                timing.push(("Vision's word warm-up beside the neural recogniser's".into(), list(&|c| c.vision)));
                timing.push(("Vision's first real pass after both warm-ups".into(), list(&|c| c.vision_first)));
            }
            timing.push((format!("first recognition after {what}"), list(&|c| Some(c.first))));
            timing.push((format!("the recognition after it ({what})"), list(&|c| Some(c.second))));
            let added = list(&|c| Some(c.mem_after? - c.mem_before?));
            timing.push((format!("peak memory the recogniser added, MB ({what})"), added));
        }
    }
    timing.push(("first pass on a fresh thread, the process warm".into(), pure::ms_list(&fresh)));
    timing.push(("one thread, median a pass".into(), alone_med.map(pure::ms).unwrap_or_else(|| "?".into())));
    timing.push(("two threads at once, median a pass".into(), both_med.map(pure::ms).unwrap_or_else(|| "?".into())));
    timing.extend(beside_rows);
    timing.extend(sustained_rows);
    timing.push(("warm accurate pass right before the idle passes".into(), pure::ms_list(&warm)));
    let mut pauses: Vec<Idle> = Vec::new();
    for (i, _) in &after {
        if !pauses.contains(i) {
            pauses.push(*i);
        }
    }
    pauses.sort_by_key(|i| (i.engine as u8, i.secs, i.fresh));
    for i in pauses {
        let v: Vec<f64> = after.iter().filter(|(j, _)| *j == i).map(|(_, ms)| *ms).collect();
        timing.push((format!("{} after {} s idle, on {}", i.engine.word(), i.secs, i.thread()), pure::ms_list(&v)));
    }
    let timing_table = pure::timing_table(&timing);
    out.say("table: first passes, threads and idle (in the order taken where there are several)");
    out.raw(&timing_table);
    summary.add(&mut out, &format!("First passes, threads and idle (ms).\n\n{timing_table}\n"));

    let wrote = out.path.as_ref().map(|p| format!("; wrote {}", p.display())).unwrap_or_default();
    out.loud(&format!("done in {} s{wrote}", started.elapsed().as_secs()));
    0
}

/// The line that ends a section: how long it took.
fn section_took(name: &str, since: Instant) -> String {
    format!("{name} | section took {:.1} s", since.elapsed().as_secs_f64())
}
