//! The Vision half of `automation-platform ocr-bench`. The pictures, the statistics and every
//! line it prints are the pure half, `crate::ocr::bench`, whose header says what it is for.
//!
//! A child of `ocr.rs`, so that what it times is that file's own code rather than a copy:
//! `recognize_captured` for a whole read, `new_request` and `perform` for one pass,
//! `content_margin`, `Plan::content` and `render` for the picture a pass is handed. Every variant
//! starts from the production request and changes one thing on it ([`request_for`]). Nothing in
//! the running application calls this.
//!
//! What it does, in this order, each line written to the file the moment it is known, so that a
//! pass that kills the process keeps everything before it:
//!
//! 1. **The machine**: model, processor, cores, memory, macOS, thermal state, low power mode,
//!    load, power source, VoiceOver; the text-recognition revisions this macOS has and the one a
//!    new request uses; the compute devices Core ML has (`MLAllComputeDevices`, looked up by
//!    name: a macOS before 14 does not have it, and importing it would stop the application from
//!    starting there) and the ones Vision offers text recognition, per stage.
//! 2. **First passes**, each in a process of its own (this executable again, `--child`): the
//!    application's warm-up over its bars, one over a line of words, or none, on a thread of its
//!    own; then the first real pass on another thread, which is what the event loop's first read
//!    is in the application, and the pass after it. One process first that is not counted, so
//!    that none of the counted ones meets a cold file cache, and the order turned each round.
//! 3. **Probes**: each variant that calls what no Mac has run for this application (a request
//!    revision, a compute device) runs one pass in a process of its own first. One that dies
//!    there, or that Vision refuses, is left out of everything below.
//! 4. **The pipeline**: every picture through `recognize_captured` as `host.ocr.recognize` reads
//!    it — no language, every rung of the ladder, the ladder's budget counted from a pretend
//!    capture — with the Vision passes each read made; then again with every request asking for
//!    revision 2, when this macOS has it.
//! 5. **The engine**: one pass of each variant over a few pictures, every cell once a round in an
//!    order that changes from round to round, the first sample of each cell kept apart.
//! 6. **Threads**: the first pass on fresh threads once the process is warm, one thread alone
//!    against two at once, and what switching between variants cost the engine.
//! 7. **Idle**: one pass after 2, 10 and 30 seconds with nothing to do, on the thread that read
//!    before and on a fresh one.
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
use objc2::{msg_send, sel, ClassType};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{CGColorRenderingIntent, CGDataProvider, CGImage};
use objc2_core_ml::MLComputeDeviceProtocol;
use objc2_foundation::{NSArray, NSIndexSet, NSProcessInfo, NSString};
use objc2_vision::VNRecognizeTextRequest;

use super::{
    cgimage_to_rgba, content_margin, new_request, passes_on_this_thread, perform, recognize_captured,
    render, run_vision, synthetic_page, upscale_toward, Ladder, Plan, ACCURATE, FAST, LADDER_BUDGET,
    REVISION_OVERRIDE, TARGET_CONTENT_PX,
};
use crate::ocr::bench::{
    self as pure, Cell, Child, ChildResult, Device, Fixture, Grid, Idle, Options, Out, PipelineRow,
    Probe, Sample, Stats, Tweak, Warmup, FIXTURES, VARIANTS,
};
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
    match opts.child {
        Some(Child::FirstPass(warmup)) => objc2::rc::autoreleasepool(|_| child(warmup)),
        Some(Child::Probe(v)) => objc2::rc::autoreleasepool(|_| probe_child(v)),
        None => objc2::rc::autoreleasepool(|_| bench(&opts)),
    }
}

// ── Quality of service ───────────────────────────────────────────────────────────────────────

extern "C" {
    /// `<sys/qos.h>`, macOS 10.10 and later: the calling thread's quality-of-service class.
    /// Declared here as a plain number: the libc crate binds the class as a Rust enum, into which
    /// a value it does not list could not be read back soundly.
    fn qos_class_self() -> u32;
}

const QOS_USER_INITIATED: u32 = 0x19;

/// A class a measuring thread was left with instead of user-initiated; `u32::MAX` while none was.
static QOS_MISSED: AtomicU32 = AtomicU32::new(u32::MAX);

fn qos_word(class: u32) -> String {
    match class {
        0x21 => "user-interactive".to_string(),
        0x19 => "user-initiated".to_string(),
        0x15 => "default".to_string(),
        0x11 => "utility".to_string(),
        0x09 => "background".to_string(),
        0x00 => "unspecified".to_string(),
        other => format!("class 0x{other:x}"),
    }
}

/// This thread's quality of service, in words.
fn qos_here() -> String {
    // SAFETY: a plain question about the calling thread.
    qos_word(unsafe { qos_class_self() })
}

/// What a thread that times a pass does first: ask for user-initiated, as the application's
/// recognise thread does (`thread_init`). One that is left with another class is remembered, and
/// the conditions at the end say so.
fn measuring_thread() {
    // SAFETY: plain calls about the calling thread.
    let (rc, got) = unsafe {
        (
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0),
            qos_class_self(),
        )
    };
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
}

/// The PNG decoded by CoreGraphics, at the size its entry says, and drawn once into a bitmap of
/// its own: a capture reaches the pipeline as pixels, while an image backed by PNG data may be
/// decoded again whenever it is read.
fn decode(f: &'static Fixture) -> Result<Handed, String> {
    let data = CFData::from_static_bytes(f.png);
    let provider = CGDataProvider::with_cf_data(Some(&data))
        .ok_or_else(|| "CoreGraphics made no data provider for it".to_string())?;
    // SAFETY: no decode array (null is documented as "none") and a live provider.
    let png = unsafe {
        CGImage::with_png_data_provider(
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| "CoreGraphics did not decode it".to_string())?;
    let got = (CGImage::width(Some(&png)), CGImage::height(Some(&png)));
    if got != f.px() {
        return Err(format!("it decoded at {}x{} px, not {}x{}", got.0, got.1, f.px().0, f.px().1));
    }
    let (image, buf) = render(&png, &Plan::identity(got.0, got.1))
        .ok_or_else(|| "it could not be drawn into a bitmap".to_string())?;
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
    let prod = hand(&native, f, TARGET_CONTENT_PX)?;
    Ok(Picture { fixture: f, prod, native })
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
        Tweak::None | Tweak::Control | Tweak::Fast | Tweak::Lang(_) | Tweak::Reuse | Tweak::TargetPx(_) => {}
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
         recognise thread does (its event loop's own reads run at user-interactive); this thread has {}",
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

/// The revisions this macOS lists for text recognition.
fn supported_revisions() -> Vec<usize> {
    // Asked of VNRecognizeTextRequest's class: `supportedRevisions` is a class method, and the
    // binding's `VNRequest::supportedRevisions()` would ask the base class.
    // SAFETY: `+supportedRevisions` is on every request class from macOS 10.13 and returns an
    // index set, or nil, which `Option` takes.
    let set: Option<Retained<NSIndexSet>> =
        unsafe { msg_send![VNRecognizeTextRequest::class(), supportedRevisions] };
    set.map(|set| (1..=16).filter(|r| set.containsIndex(*r)).collect()).unwrap_or_default()
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
/// normally, or what became of it.
fn run_child(exe: &Path, role: Child) -> Result<String, String> {
    let mut child = Command::new(exe)
        .arg("ocr-bench")
        .arg("--child")
        .arg(role.arg())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("could not be started: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("did not finish within 120 s".to_string());
            }
            Err(e) => return Err(format!("could not be waited for: {e}")),
        }
    };
    let mut text = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut text);
    }
    if status.success() {
        return Ok(text);
    }
    let printed = if text.trim().is_empty() { String::new() } else { format!(" after printing: {}", text.trim()) };
    Err(match (status.signal(), status.code()) {
        (Some(sig), _) => format!("died of signal {sig}{printed}"),
        (None, Some(code)) => format!("ended with status {code}{printed}"),
        (None, None) => format!("ended without a status{printed}"),
    })
}

/// The application's warm-up (`warm_up_in_pool`) on a thread of its own, waited for: one accurate
/// pass with no language, over its bars or over a line of words. Its time, and whether it read
/// anything. The thread asks for no quality of service, as the application's does not.
fn warm_up_on_own_thread(words: bool) -> (f64, bool) {
    std::thread::spawn(move || {
        objc2::rc::autoreleasepool(|_| {
            let line = if words { pure::fixture("line@1x").and_then(|f| decode(f).ok()) } else { None };
            let t = Instant::now();
            let read = match (words, line) {
                (true, Some(img)) => run_vision(&img.image, None, ACCURATE, &|_| (0, 0, 1, 1)),
                (true, None) => None,
                (false, _) => synthetic_page().and_then(|(img, buf)| {
                    let r = run_vision(&img, None, ACCURATE, &|_| (0, 0, 1, 1));
                    drop(buf);
                    r
                }),
            };
            (ms_since(t), read.is_some_and(|(text, _, _)| !text.trim().is_empty()))
        })
    })
    .join()
    .unwrap_or((f64::NAN, false))
}

/// A first-pass child: the warm-up asked for, then the first two real passes on this thread, each
/// with its request made inside the clock, as the application's first read makes one.
fn child(warmup: Warmup) -> i32 {
    measuring_thread();
    let Some(pic) = pure::fixture(pure::PROBE).and_then(|f| picture(f).ok()) else {
        println!("OCR BENCH CHILD: failed: the picture could not be prepared");
        return 1;
    };
    let warm = match warmup {
        Warmup::Bars => Some(warm_up_on_own_thread(false)),
        Warmup::Word => Some(warm_up_on_own_thread(true)),
        Warmup::Nothing => None,
    };
    let first = prod_pass(&pic.prod.image);
    let second = prod_pass(&pic.prod.image);
    println!("{}", pure::child_line(&ChildResult { warmup: warm, first, second }));
    0
}

/// A probe child: one pass of variant `v` over the probe picture.
fn probe_child(v: usize) -> i32 {
    measuring_thread();
    let Some(pic) = pure::fixture(pure::PROBE).and_then(|f| picture(f).ok()) else {
        println!("OCR BENCH CHILD: failed: the picture could not be prepared");
        return 1;
    };
    let probe = match request_for(VARIANTS[v].tweak) {
        Err(why) => Probe::No(why),
        Ok((request, pinned)) => match perform(&pic.prod.image, &request, &|_| (0, 0, 1, 1)) {
            Ok(_) => Probe::Ran { pinned },
            Err(why) => Probe::No(format!("Vision refused it: {why}")),
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

struct FirstPasses {
    /// The process that is not counted, which meets the cold file cache.
    uncounted: Option<ChildResult>,
    by_kind: Vec<(Warmup, Vec<ChildResult>)>,
}

fn first_passes(out: &mut Out, plan: &pure::Plan, exe: &Path) -> FirstPasses {
    let mut got = FirstPasses {
        uncounted: None,
        by_kind: Warmup::ALL.iter().map(|w| (*w, Vec::new())).collect(),
    };
    out.say(&format!(
        "first passes: each in a process of its own — a warm-up on a thread of its own, then the first real pass of \
         {} on another thread and the pass after it, each with its request made inside the clock. One process with \
         the word warm-up first, not counted, so that the counted ones find the file cache warm; then {} round(s), \
         one process per warm-up, the order turned by one each round",
        pure::PROBE,
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
        let order = pure::warmup_order(round);
        out.say(&format!(
            "first passes | round {} | {}",
            round + 1,
            order.iter().map(|w| w.word()).collect::<Vec<_>>().join(", then ")
        ));
        for warmup in order {
            let result = run_child(exe, Child::FirstPass(warmup))
                .and_then(|t| pure::parse_child(&t).ok_or_else(|| format!("printed no result: {}", t.trim())));
            match result {
                Ok(r) => {
                    out.say(&format!("first passes | warm-up {} | {}", warmup.word(), first_pass_text(&r)));
                    if let Some((_, list)) = got.by_kind.iter_mut().find(|(w, _)| *w == warmup) {
                        list.push(r);
                    }
                }
                Err(why) => out.say(&format!("first passes | warm-up {} | not measured: the process {why}", warmup.word())),
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
         compute device) runs one pass in a process of its own first; one that dies there, or that Vision \
         refuses, is left out of everything below",
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

/// One read of a picture through `recognize_captured`, exactly as `host.ocr.recognize` reads a
/// region once its capture is in hand: no language, every rung of the ladder. The ladder's
/// budget starts `capture` before the read, where the application's starts before its capture;
/// the time reported is the read's alone.
fn read_through_pipeline(p: &Picture, capture: Duration) -> Sample {
    objc2::rc::autoreleasepool(|_| {
        let f = p.fixture;
        let before = passes_on_this_thread();
        let t = Instant::now();
        let started = t.checked_sub(capture).unwrap_or(t);
        let r = recognize_captured(
            &p.native.image,
            f.scale as f64,
            0,
            0,
            f.w_pt,
            f.h_pt,
            None,
            started,
            false,
            &Ladder::FULL,
        );
        let ms = ms_since(t);
        let passes = passes_on_this_thread() - before;
        match r {
            Ok(t) => Sample { ms, text: Some(t.text), passes, skipped: t.skipped },
            Err(_) => Sample { ms, text: None, passes, skipped: false },
        }
    })
}

/// Every picture through the pipeline, each row written as soon as it is done.
fn pipeline(
    out: &mut Out,
    pictures: &[Picture],
    plan: &pure::Plan,
    request: &'static str,
    capture: Duration,
) -> Vec<PipelineRow> {
    let mut rows: Vec<PipelineRow> =
        pictures.iter().map(|p| PipelineRow { request, picture: p.fixture, cell: Cell::default() }).collect();
    for _ in 0..=plan.samples {
        for (row, p) in rows.iter_mut().zip(pictures) {
            if row.cell.wants_more(plan) {
                row.cell.push(read_through_pipeline(p, capture));
                if !row.cell.wants_more(plan) {
                    out.say(&row.line());
                }
            }
        }
    }
    rows
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

fn engine(out: &mut Out, pictures: &[Picture], plan: &pure::Plan, revisions: &[usize], probes: &Probes) -> Grid {
    let chosen: Vec<&Picture> = plan
        .engine_pictures
        .iter()
        .filter_map(|label| pictures.iter().find(|p| p.fixture.label() == *label))
        .collect();
    let mut grid = Grid::new(VARIANTS.iter().collect(), chosen.iter().map(|p| p.fixture).collect());
    let (nv, np) = (grid.variants.len(), grid.pictures.len());
    out.say(&format!(
        "engine: {nv} variants over {np} pictures ({}), {} passes a cell after a first one kept apart, every cell \
         once a round in an order that changes from round to round; a pass is making and configuring the request, \
         the handler, performRequests and reading the results out, as run_vision spends it (reuse: the one kept \
         request); this thread at {}",
        grid.pictures.iter().map(|p| p.label()).collect::<Vec<_>>().join(", "),
        plan.samples,
        qos_here()
    ));
    // What a target variant is handed, per picture, made once; and which cells have been printed.
    let mut alt: Vec<Vec<Option<Handed>>> = Vec::new();
    let mut printed = vec![vec![false; np]; nv];
    for (v, variant) in VARIANTS.iter().enumerate() {
        let why = match variant.tweak {
            Tweak::Revision(r) if !revisions.contains(&r) => Some(format!("this macOS has no revision {r}")),
            t if t.probed() => probed_ok(probes, v).err(),
            _ => None,
        };
        let mut handed = Vec::new();
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
            if grid.cells[v][p].skipped.is_some() {
                printed[v][p] = true;
                out.say(&grid.line(v, p));
            }
        }
        alt.push(handed);
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

// ── 7. Idle ──────────────────────────────────────────────────────────────────────────────────

/// Three warm passes, then one pass after each pause, each written as it comes (a long-idle run
/// takes over half an hour).
fn idle(out: &mut Out, pic: &Picture, plan: &pure::Plan) -> (Vec<f64>, Vec<(Idle, f64)>) {
    let warm: Vec<f64> = (0..3).map(|_| prod_pass(&pic.prod.image)).collect();
    out.say(&format!("idle | {} | warm right before: {} ms", pure::PROBE, pure::ms_list(&warm)));
    let mut after = Vec::new();
    for i in &plan.idle {
        std::thread::sleep(Duration::from_secs(i.secs));
        let ms = if i.fresh { pass_on_fresh_thread(pic) } else { prod_pass(&pic.prod.image) };
        out.say(&format!("idle | {} | after {} s, on {}: {} ms", pure::PROBE, i.secs, i.thread(), pure::ms(ms)));
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
    machine(&mut out);
    conditions(&mut out, "at the start");
    let revisions = vision(&mut out);
    for v in VARIANTS {
        out.say(&format!("variant {}: {}", v.name, v.what));
    }

    let mut pictures = Vec::new();
    for f in FIXTURES {
        match picture(f) {
            Ok(p) => pictures.push(p),
            Err(why) => out.say(&format!("picture {}: not usable: {why}", f.label())),
        }
    }
    let Some(probe) = pictures.iter().position(|p| p.fixture.label() == pure::PROBE) else {
        out.loud(&format!("stopped: the picture {} could not be prepared, so nothing can be compared", pure::PROBE));
        return 1;
    };

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
            let children = first_passes(&mut out, &plan, &exe);
            let probed = probes(&mut out, &exe, &revisions);
            (Some(children), probed)
        }
        Err(e) => {
            out.say(&format!("first passes and probes: not run, this executable cannot find itself: {e}"));
            (None, Vec::new())
        }
    };

    // This process's own first passes are not measured (the children did that); two untimed
    // passes make the sections below start warm.
    for _ in 0..2 {
        let _ = prod_pass(&pictures[probe].prod.image);
    }

    out.say(&format!(
        "pipeline: every picture as host.ocr.recognize reads it once the capture is in hand, the ladder's {} ms budget \
         counted from {} ms before each read for the capture it does not make (--capture-ms); this thread at {}",
        LADDER_BUDGET.as_millis(),
        opts.capture_ms,
        qos_here()
    ));
    let mut rows = pipeline(&mut out, &pictures, &plan, "prod", capture);
    let rev2 = pure::variant("rev2").expect("rev2 is a variant");
    let rev2_ok = if revisions.contains(&2) {
        probed_ok(&probed, rev2)
    } else {
        Err("this macOS has no revision 2".to_string())
    };
    match rev2_ok {
        Ok(()) => {
            out.say("pipeline: the same again with every request of every rung asking for revision 2");
            let _rev2 = RevisionOverride::set(2);
            rows.extend(pipeline(&mut out, &pictures, &plan, "rev2", capture));
        }
        Err(why) => out.say(&format!("pipeline | rev2 | not measured: {why}")),
    }
    let pipeline_table = pure::pipeline_table(&rows);
    out.say("table: pipeline, a whole read without the capture, as host.ocr.recognize makes it");
    out.raw(&pipeline_table);
    summary.add(&mut out, &format!("Pipeline: a whole read without the capture.\n\n{pipeline_table}\n"));

    let grid = engine(&mut out, &pictures, &plan, &revisions, &probed);
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

    out.say(&format!(
        "idle: one pass after each pause, in this order: {}; this thread at {}",
        plan.idle.iter().map(|i| format!("{} s on {}", i.secs, i.thread())).collect::<Vec<_>>().join(", "),
        qos_here()
    ));
    let (warm, after) = idle(&mut out, pic, &plan);
    conditions(&mut out, "at the end");

    let mut timing: Vec<(String, String)> = Vec::new();
    if let Some(children) = &children {
        if let Some(r) = &children.uncounted {
            timing.push((
                "first real pass in the process not counted (word warm-up, cold file cache)".into(),
                pure::ms(r.first),
            ));
        }
        for (warmup, results) in &children.by_kind {
            let label = match warmup {
                Warmup::Bars => "after the application's warm-up (bars)",
                Warmup::Word => "after a warm-up over a line of words",
                Warmup::Nothing => "with no warm-up",
            };
            let warmups: Vec<f64> = results.iter().filter_map(|r| r.warmup.map(|w| w.0)).collect();
            if !warmups.is_empty() {
                timing.push((format!("the warm-up itself, {}", warmup.word()), pure::ms_list(&warmups)));
            }
            let firsts: Vec<f64> = results.iter().map(|r| r.first).collect();
            let seconds: Vec<f64> = results.iter().map(|r| r.second).collect();
            timing.push((format!("first real pass in a process, {label}"), pure::ms_list(&firsts)));
            timing.push((format!("the pass after it, {label}"), pure::ms_list(&seconds)));
        }
    }
    timing.push(("first pass on a fresh thread, the process warm".into(), pure::ms_list(&fresh)));
    timing.push(("one thread, median a pass".into(), alone_med.map(pure::ms).unwrap_or_else(|| "?".into())));
    timing.push(("two threads at once, median a pass".into(), both_med.map(pure::ms).unwrap_or_else(|| "?".into())));
    timing.push(("warm pass right before the idle passes".into(), pure::ms_list(&warm)));
    let mut pauses: Vec<Idle> = Vec::new();
    for (i, _) in &after {
        if !pauses.contains(i) {
            pauses.push(*i);
        }
    }
    pauses.sort_by_key(|i| (i.secs, i.fresh));
    for i in pauses {
        let v: Vec<f64> = after.iter().filter(|(j, _)| *j == i).map(|(_, ms)| *ms).collect();
        timing.push((format!("pass after {} s idle, on {}", i.secs, i.thread()), pure::ms_list(&v)));
    }
    let timing_table = pure::timing_table(&timing);
    out.say("table: first passes, threads and idle (in the order taken where there are several)");
    out.raw(&timing_table);
    summary.add(&mut out, &format!("First passes, threads and idle (ms).\n\n{timing_table}\n"));

    let wrote = out.path.as_ref().map(|p| format!("; wrote {}", p.display())).unwrap_or_default();
    out.loud(&format!("done in {} s{wrote}", started.elapsed().as_secs()));
    0
}
