//! Reading text off the screen with Vision.
//!
//! The regions this is asked about are tiny and the text in them is small — a parameter
//! read-out of a few characters, sometimes a single digit. Word boxes come back in
//! capture-region coordinates; the host adds the region origin itself.
//!
//! Two things about the Windows implementation have to be understood before this one makes
//! sense, because they are the reason it looks the way it does.
//!
//! The first is that Windows does not hand the captured region to its recogniser as it
//! found it. For anything up to 400x200 it crops to the content's bounding box and upscales
//! that crop until the glyphs are around 64 px tall, because a recogniser trained on
//! photographs of documents is bad at eight-pixel UI text and good at the same text
//! enlarged. The same is true of Vision, so the same preprocessing happens here.
//!
//! The second is that Windows runs a *second*, independent recogniser for small regions and
//! uses its answer only when the first comes back empty — that split is why Melodyne's note
//! field is readable at all, and `modules/melodyne/src/main.luau` documents at length what
//! happened when a change made the primary non-empty and the fallback stopped firing. On a Mac
//! that second engine (`backend/paddle_ocr.rs`) is loaded where the package carries ONNX Runtime,
//! and from the moment it is ready a read follows Windows' rule with Vision's accurate ladder in
//! the first recogniser's place (`ocr/merge.rs` writes the rule down, `Rungs::merged` climbs it):
//! the recogniser is handed the content crop beside the ladder, and its text answers where the
//! ladder read nothing. On an Intel Mac the fast level reads first, checked by it. Until then, and
//! where it is not there at all, Vision carries the small-text case alone, and everything it is
//! given is chosen for that either way: the capture is taken at full backing resolution
//! rather than the point-sized one the rest of
//! the platform uses (`capture::capture_backing`, the same path with the downsample left
//! off), recognition is `.accurate`, language correction is off (these are
//! values, not words — correction is exactly what turns "+36 Ct" into a dictionary word),
//! and the minimum text height is dropped to zero. When the tightened pass still comes back
//! empty, one further pass runs over the untightened capture, on the same "only when empty"
//! rule as the Windows fallback and for the same reason: a content crop that went wrong
//! (a border, a caret, a stray highlight) shrinks the effective upscale, and by then there
//! is nothing left to lose.
//!
//! Nothing here returns `Err`. The binding turns an `Err` into a thrown Lua error inside the
//! module's timer callback, so a screen that could not be captured would take the callback
//! down rather than simply reading as nothing; every failure is logged and answered with
//! empty text instead. That makes the log the only evidence a remote tester can send, which
//! is why there is so much of it.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, AllocAnyThread, ClassType};
use objc2_core_foundation::{CFData, CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorRenderingIntent, CGColorSpace,
    CGContext, CGDataProvider, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGInterpolationQuality, CGPreflightScreenCaptureAccess,
};
use objc2_foundation::{NSArray, NSDictionary, NSIndexSet, NSLocale, NSRange, NSString};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};

use crate::backend::paddle_ocr::{self, Polled, Qos};
use crate::backend::{CaptureSource, OcrLine, OcrText, OcrThread, OcrWord, Recognise};
use crate::ocr::cost::{self, Answered, Capture, Check, CheckWait, PaddleWait, Pass, Stage, Waited};
use crate::ocr::ladder::{Answer, Level, PaddleUse, Path, Shape};
use crate::ocr::merge::{self, Pick};
use crate::ocr::paddle_pre::{self, Tighten};
use crate::ocr::plan::{self, Plan as CapturePlan};
use crate::ocr::policy::{SLOW_JOB, SLOW_LOG_EVERY};
use crate::ocr::shadow;
use crate::ocr::types::Rect;

/// `automation-platform ocr-bench`: Vision measured on fixed pictures. A child of this file so
/// that it runs this file's own pipeline, request and pass rather than a copy; nothing in the
/// application calls it.
pub(crate) mod bench;

/// Which of the ladder's last rungs one recognition may climb, and the ladder's shape.
///
/// The legacy calls climb all of them, as they always have. A `host.ocr.read` skips the fast
/// model for a language that model does not read, and a background read skips both rungs while
/// an interactive one is waiting behind it — they are the rungs that cost the most on a read that
/// will never resolve.
struct Ladder<'a> {
    fast_ok: bool,
    preempt: Option<&'a AtomicBool>,
    /// Climbed on the event loop — the legacy calls — rather than on `host.ocr.read`'s recognise
    /// thread. Only what the log says about a ladder given up depends on it.
    on_event_loop: bool,
    /// Which level reads first, what the neural recogniser does, whether the rest of the ladder
    /// runs, which revisions the first passes ask for (`ocr/ladder.rs`). Every read the
    /// application makes passes the production shape ([`Ladder::shape_now`]); `ocr-bench` passes
    /// one per strategy.
    shape: Shape,
}

impl Ladder<'static> {
    const FULL: Ladder<'static> = Ladder { fast_ok: true, preempt: None, on_event_loop: true, shape: Shape::TODAY };
}

impl Ladder<'_> {
    fn preempted(&self) -> bool {
        self.preempt.is_some_and(|p| p.load(Ordering::Relaxed))
    }

    /// The shape every read of the application climbs: today's until the neural recogniser is
    /// ready, Windows' rule over the accurate ladder from then on, on an Intel Mac with the fast
    /// level checked first (`Shape::production`).
    fn shape_now() -> Shape {
        Shape::production(paddle_ocr::ready(), intel_mac())
    }
}

/// Whether this is an Intel Mac (`ladder::intel_mac`): the x86_64 slice, not translated by Rosetta.
/// Asked of the system once.
pub(crate) fn intel_mac() -> bool {
    static INTEL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *INTEL.get_or_init(|| {
        crate::ocr::ladder::intel_mac(cfg!(target_arch = "x86_64"), super::perm::sysctl_i32("sysctl.proc_translated"))
    })
}

/// Background border added around the upscaled content, in pixels of the processed image.
///
/// Same figure as the Windows path. A recogniser expects text to sit in a page, not to run
/// off the edge of one, and a few characters cropped hard to their own ink read badly.
const OCR_PAD: usize = 24;

/// How tall the content is aimed at in the processed image.
///
/// 64 is the figure the Windows path settled on, and it leaves room for the crop being a little
/// loose. Apple names no height in pixels: the one figure Vision documents is the relative
/// `minimumTextHeight`, a fraction of the image's height — one thirty-second unless it is set, and
/// it is set to 0 here (`new_request`). This comment used to quote Apple guidance of "around
/// 32 px"; three searches of Apple's documentation and sessions (2026-10-01) found no such source.
const TARGET_CONTENT_PX: usize = 64;

/// How long the whole ladder may take before the last rungs are abandoned.
///
/// The ladder exists for the read that nearly works; the cost falls entirely on the read
/// that never will. Every rung is a Vision pass, they run on the thread that also carries
/// the keyboard event tap, and macOS switches a tap off when its thread stops answering —
/// which is exactly how the tester lost a Shift+Tab into a plugin that then moved its own
/// focus. Better a field that says nothing promptly than a keystroke that goes missing.
///
/// It bounds a read's waits for the neural recogniser as well (`ocr/merge.rs`): what is left of
/// it is how long a read may still wait, and once it is spent a read takes the recogniser's answer
/// only if it is already there. So every wait of a read for the recogniser ends by the budget's
/// end, where the fast rung it replaces could start right before it.
const LADDER_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

/// Above this size, in points, the region is passed through untouched.
///
/// Identical to the Windows thresholds on purpose: whether a region gets the small-text
/// treatment is observable from Lua (it changes which of two adjacent read-outs is legible),
/// so the two platforms have to draw the line in the same place — one constant, in
/// `ocr/policy.rs`, for both.
use crate::ocr::policy::{SMALL_H, SMALL_W};

pub fn recognize(x: i32, y: i32, w: i32, h: i32, lang: Option<&str>) -> Result<OcrText, String> {
    // Vision produces a good deal of temporary Objective-C on every pass — an observation
    // and a candidate string per line, an array per call — and one module asks for this
    // sixteen times a second. The result is plain Rust data, so the pool can close over
    // everything the recognition made rather than leaving it for whenever the run loop next
    // drains its own.
    objc2::rc::autoreleasepool(|_| recognize_inner(x, y, w, h, lang))
}

fn recognize_inner(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    lang: Option<&str>,
) -> Result<OcrText, String> {
    let started = Instant::now();
    // A zero-sized region is not a failure: `region::loose_corners` reads a reversed rectangle
    // as zero wide or high, so a module with its geometry momentarily wrong gets here rather
    // than being stopped earlier.
    if w <= 0 || h <= 0 {
        crate::logging::trace("macos", || {
            format!("ocr: nothing to read, region is {w}x{h} at {x},{y}")
        });
        return Ok(empty());
    }

    let debug = crate::appcfg::ocr_debug();

    // Everything below works in capture pixels and converts to points only at the very end,
    // through this one factor. On a Retina display it is 2.0, and the whole reason the
    // capture is taken at backing resolution is that halving it first would throw away the
    // detail that makes eight-point text readable — so the division belongs on the answer,
    // not on the input.
    let Some((native, scale)) = super::capture::capture_backing(x, y, w, h) else {
        report_capture_failure(x, y, w, h);
        return Ok(empty());
    };
    let capture = Capture::Took(started.elapsed().as_secs_f64() * 1000.0);
    let ladder = Ladder { shape: Ladder::shape_now(), ..Ladder::FULL };
    recognize_captured(&native, scale, x, y, w, h, lang, started, capture, debug, &ladder)
}

/// The slack the content crop leaves round the ink, in capture pixels. A physical distance, not
/// a pixel count: three pixels of slack round the ink at 1x is one and a half at 2x, and the
/// crop would start clipping antialias fringes off Retina glyphs. One function, because
/// `ocr-bench` (`bench.rs`) crops its pictures exactly as a read does.
fn content_margin(scale: f64) -> usize {
    (3.0 * scale).round().max(1.0) as usize
}

/// Everything after the capture, so that a caller who already has the pixels — one bounding-box
/// capture cut into several regions — runs exactly the same pipeline as a single read. `capture`
/// says where the pixels came from, for the slow-read line.
#[allow(clippy::too_many_arguments)]
fn recognize_captured(
    native: &CGImage,
    scale: f64,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    lang: Option<&str>,
    started: Instant,
    capture: Capture,
    debug: bool,
    ladder: &Ladder,
) -> Result<OcrText, String> {
    recognize_noted(native, scale, x, y, w, h, lang, started, capture, debug, ladder).map(|(text, _)| text)
}

/// What a read did besides its answer: its Vision passes, who answered, and whether the fast
/// level and the neural recogniser read the same. For `ocr-bench`'s pipeline, which counts them
/// per strategy; the application drops it.
struct Note {
    passes: Vec<Pass>,
    answer: Answer,
    /// A fast first pass beside the neural recogniser: whether the two read the same, when either
    /// read anything.
    agreed: Option<bool>,
    /// The ladder's shape wanted the neural recogniser, but today's ladder read the region: it was
    /// not ready, or could not be asked, or — for `ocr-bench`'s strategies but the application's —
    /// the region was not its to read, its ink more than one line, or too wide (`paddle_pre::fits`).
    today: bool,
}

impl Note {
    /// A read that made no pass: the blank guard answered, or the pixels were not there.
    fn none() -> Note {
        Note { passes: Vec::new(), answer: Answer::Nobody, agreed: None, today: false }
    }
}

/// [`recognize_captured`], and what the read did ([`Note`]).
#[allow(clippy::too_many_arguments)]
fn recognize_noted(
    native: &CGImage,
    scale: f64,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    lang: Option<&str>,
    started: Instant,
    capture: Capture,
    debug: bool,
    ladder: &Ladder,
) -> Result<(OcrText, Note), String> {
    let nw = CGImage::width(Some(native));
    let nh = CGImage::height(Some(native));
    // How long Vision has been idle, taken before this read's own passes, and the journal they
    // are written into. A read that returns early — nothing in it, pixels that could not be read
    // back — made no pass, and says nothing about its cost.
    let idle = idle_now();
    journal_open();

    let small = w <= SMALL_W && h <= SMALL_H;
    let (result, climbed, merged) = if !small {
        // A whole window, or something like it. Multi-word layout and speed matter more
        // than the last per-glyph pixel, and cropping to content would be meaningless.
        let plan = Plan::identity(nw, nh);
        if debug {
            // Vision is handed the capture itself here, so the raw image and the processed
            // one are the same file.
            if let Some((rgba, dw, dh)) = cgimage_to_rgba(native) {
                debug_dump("ocr-debug.bmp", &rgba, dw, dh);
            }
        }
        (
            run_vision(native, lang, ACCURATE, Stage::AsCaptured, &|bb| map_box(&plan, scale, bb)),
            Climbed::default(),
            None,
        )
    } else {
        let Some((rgba, px_w, px_h)) = cgimage_to_rgba(native) else {
            warn_once(
                "ocr-convert",
                "ocr: could not read the captured pixels back out of CoreGraphics",
            );
            return Ok((empty(), Note::none()));
        };
        if debug {
            // One file per region and content, named for both, rather than one file every read
            // overwrites: a session's pictures of the same field then sit side by side, and a
            // poll's sixteen reads a second of an unchanged field write one. Bounded per region
            // and per session (`ocr/shadow.rs`, `Pictures`).
            keep_picture(&RAW_PICTURES, "debug-raw", (x, y, w, h), &rgba, px_w, px_h, shadow::pixels_hash(&rgba));
        }
        let plan = Plan::content(&rgba, px_w, px_h, content_margin(scale));

        // NOTHING TO READ IS AN ANSWER, and it is the one answer a recogniser cannot give.
        //
        // Asked about a blank rectangle, a recogniser does not return nothing — it returns
        // whatever its network makes of noise, and a module cannot tell that from a reading.
        // Windows has refused this since an empty search box and five clipped tile captions
        // all came back as the same invented string from six different places. So the question
        // is not asked: an empty region reads as empty, which is the truth about it.
        if plan.blank {
            // At LINE level, not trace, and the reason is an instrument rather than this
            // function. The probe has a section that picks provably flat rectangles, reads
            // them, and reports "0 word(s) <- nothing invented" as evidence that the
            // recogniser invents nothing on a blank. It is evidence of no such thing: a
            // provably flat rectangle is exactly what this branch refuses to send to Vision,
            // so the harder that section works to prove the region uniform, the more certainly
            // Vision was never asked. With this at trace level — off by default — a remote log
            // showed no sign the guard had fired, and the section read as a pass.
            //
            // The line turned out not to be enough on its own. The probe prints its verdict
            // from the result it is handed and cannot read this log, so the next session
            // showed the guard's line and the probe's "nothing invented" side by side, twice.
            // The result carries the fact as `skipped` now, and that is the field an
            // instrument has to look at before it calls an empty answer a measurement.
            //
            // Rare enough to afford a line: it fires only for a region with no ink in it.
            crate::logging::line(
                "macos",
                &format!(
                    "ocr: {px_w}x{px_h} px has nothing in it — returning empty WITHOUT asking \
                     the recogniser, so an empty result here says nothing about what Vision \
                     would have done"
                ),
            );
            return Ok((OcrText { skipped: true, ..empty() }, Note::none()));
        }

        crate::logging::trace("macos", || {
            let (pw, ph) = plan.out_size();
            format!(
                "ocr: cropped {}x{} px at {},{} of {px_w}x{px_h}, upscaled {}x, framed to {pw}x{ph}, {} line(s) of ink",
                plan.cw, plan.ch, plan.x0, plan.y0, plan.up, plan.lines
            )
        });
        // Windows' rule (`ocr/merge.rs`): the neural recogniser handed the content crop now, the
        // moment it is known and before the first pass, so that it reads beside the ladder rather
        // than after it; its answer is waited for only where Vision reads nothing, and on an Intel
        // Mac as the check of the fast level's first pass.
        let mut neural = ladder.shape.paddle.merges().then(|| Neural::ask(&rgba, px_w, &plan, scale));
        let rungs = Rungs {
            native,
            plan: &plan,
            rgba: &rgba,
            px_w,
            px_h,
            scale,
            lang,
            debug,
            ladder,
            started,
            w,
            h,
        };
        let (result, climbed) = rungs.climb(neural.as_mut());
        (result, climbed, neural.map(|n| (n, rgba, px_w, px_h)))
    };

    let Climbed { paddle, crop, agreed, today, compared } = climbed;
    let by_paddle = paddle.is_some();
    let (text, words, lines, fallback) = match paddle {
        // The neural recogniser's text, which locates nothing: no words and no lines, and the
        // content crop it read as the box, as Windows' fallback answers.
        Some(t) => (t, Vec::new(), Vec::new(), crop),
        None => {
            let (t, w, l) = result.unwrap_or_else(|| (String::new(), Vec::new(), Vec::new()));
            (t, w, l, None)
        }
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let passes = journal_take();
    // The last pass that could answer: the fast pass made only for the counts, right after the
    // first, never does.
    let last = passes.iter().rev().find(|p| p.stage != Stage::FastCompared);
    let answer = if by_paddle { Answer::Paddle } else { Answer::of_last_pass(last.map(|p| (p.stage, p.words))) };
    let answered = Answered {
        path: Path::of(answer),
        check: merged.as_ref().and_then(|(n, ..)| n.check),
        waited: merged.as_ref().and_then(|(n, ..)| n.waited),
    };
    // The read's own line, as Windows writes its own (`windows.rs`, `recognize_image`): at line
    // level while "Save the images OCR was given" is on — so a tester's round with the pictures
    // has who answered every read — and at trace level otherwise.
    let line = || {
        format!(
            "ocr {w}x{h} pt at {x},{y} -> {nw}x{nh} px (scale {scale:.2}), {:.1} ms, {} word(s), answered by {}{}: '{}'",
            ms,
            words.len(),
            answered.path.word(),
            answered.waits_text(),
            text.replace('\n', " ")
        )
    };
    if debug {
        crate::logging::line("macos", &line());
    } else {
        crate::logging::trace("macos", line);
    }
    note_cost(ms, w, h, idle, passes.iter().any(|p| p.beside), answered);
    note_slow(ms, (x, y, w, h), capture, &passes, answered);
    if let Some((neural, rgba, px_w, px_h)) = merged {
        // Vision's ladder read nothing where the recogniser answered alone.
        let ladder_text = if by_paddle { "" } else { text.as_str() };
        neural.count(answered.path, ladder_text, &passes, compared, (x, y, w, h), debug.then_some((rgba.as_slice(), px_w, px_h)));
    }
    let note = Note { passes, answer, agreed, today };
    Ok((OcrText { text, words, lines, fallback, skipped: false }, note))
}

/// The neural recogniser's part in one small read, wherever it has loaded (`PaddleUse::merges`):
/// asked once, when the read has its content crop — before the first pass, so that it reads beside
/// the ladder — at the reading thread's own quality of service (`Qos::Reader`); its answer kept
/// once it has come, since a channel hands it over once; and the read counted with it at the end
/// (`ocr/shadow.rs`). Dropped with the read, which cancels a recognition nobody took the answer of,
/// as Windows drops its handle once `Windows.Media.Ocr` has answered.
///
/// What it costs a read: copying the crop and queueing it, on the reading thread — microseconds —
/// and on the recogniser's own thread `paddle-ocr` one recognition: 4 to 25 ms a small field on CI's
/// Macs, Intel and Apple-silicon virtual machines alike, against 3 to 36 ms on the Windows machine
/// (`ocr-bench`, 2026-10-03). A read waits for it only where it needs the answer, and never past
/// the ladder's budget ([`Neural::poll`]).
struct Neural {
    /// `None` when it was not asked: the exit had begun, its queue was full or its thread had gone.
    asked: Option<paddle_ocr::Asked>,
    /// What it answered, once it has — its text or nothing, or that it never will.
    got: Option<Polled>,
    /// Whether the region is one the fast level may be checked on (`paddle_pre::fits`).
    fits: bool,
    /// An Intel Mac's check of its fast pass: what the read waited for it, and how the check ended.
    check: Option<CheckWait>,
    /// Windows' wait, where Vision's ladder read nothing: how long, and how it ended. `None` where
    /// the read did not wait so — the ladder read something, or the check had the answer already.
    waited: Option<PaddleWait>,
}

impl Neural {
    /// Hands the content crop to the recogniser — the crop `Plan::content` found, a well's inside
    /// included, with the crop rule in points (`Tighten::for_scale`), as `ocr-bench`'s
    /// `paddle-crop` measures it.
    fn ask(rgba: &[u8], px_w: usize, plan: &Plan, scale: f64) -> Neural {
        say_reader_class();
        let (cw, ch, pixels) = paddle_crop(rgba, px_w, plan);
        Neural {
            asked: paddle_ocr::ask_with(cw, ch, &pixels, Tighten::for_scale(scale), Qos::Reader),
            got: None,
            fits: paddle_pre::fits(plan.ink_w, plan.ink_h, plan.lines),
            check: None,
            waited: None,
        }
    }

    fn asked(&self) -> bool {
        self.asked.is_some()
    }

    /// Its text, waited for at most `bound` (`Asked::wait_for`), and what this wait was: `None`
    /// when it read nothing, could not read the region, or has not answered by then — the
    /// recognition goes on, and a later call can still find its answer. An answer already had
    /// (`got`) is taken without a wait, and then the wait is `None`, as it is when it was not asked.
    fn poll(&mut self, bound: Duration) -> (Option<String>, Option<PaddleWait>) {
        let mut wait = None;
        if self.got.is_none() {
            if let Some(asked) = &self.asked {
                let began = Instant::now();
                let polled = asked.wait_for(bound);
                let ended = match &polled {
                    Polled::Answered(Some(_)) => Waited::Answered,
                    Polled::NotYet => Waited::NotYet,
                    Polled::Answered(None) | Polled::Failed => Waited::Nothing,
                };
                wait = Some(PaddleWait { ms: began.elapsed().as_secs_f64() * 1000.0, ended });
                if polled != Polled::NotYet {
                    self.got = Some(polled);
                }
            }
        }
        let text = match &self.got {
            Some(Polled::Answered(Some(read))) => Some(read.text.clone()),
            _ => None,
        };
        (text, wait)
    }

    /// Windows' wait, where Vision's ladder read nothing: its text within `bound`, the wait kept
    /// for the read's lines ([`Neural::waited`]).
    fn wait(&mut self, bound: Duration) -> Option<String> {
        let (text, wait) = self.poll(bound);
        self.waited = wait;
        text
    }

    /// Keeps how an Intel Mac's check went, for the read's lines: its `wait`, and whether the fast
    /// level and the recogniser `agreed` (`merge::check`).
    fn checked(&mut self, wait: Option<PaddleWait>, agreed: Option<bool>) {
        let Some(wait) = wait else { return };
        let ended = match (wait.ended, agreed) {
            (Waited::NotYet, _) => Check::NotYet,
            (Waited::Nothing, _) => Check::Nothing,
            (Waited::Answered, Some(true)) => Check::Same,
            (Waited::Answered, _) => Check::Otherwise,
        };
        self.check = Some(CheckWait { ms: wait.ms, ended });
    }

    /// Counts the read in the process's tally, once it has answered by `path`, Vision's ladder having
    /// read `ladder` with `passes`: the recogniser's answer as the read had it, or if it had none
    /// yet, as it is now — not waiting for it, and cancelling it when it is not there — beside the
    /// fast level's reading (`fast`: an Intel Mac's check, or the pass made only for these counts).
    /// Every `shadow::LINE_EVERY`th read the tally's line; a trace line for a read worth a look, and
    /// with `picture` (the switch "Save the images OCR was given") its picture, named for its case —
    /// `ocr-shadow-paddle-alone-…` for every one the recogniser answered alone. The same pixels are
    /// often the read's `ocr-debug-raw` file as well: kept twice on purpose, since only this name
    /// says the case when trace logging is off, as it is in a tester's round, and the lists' bytes
    /// are bounded (`shadow::MAX_BYTES`). A read whose crop could not be rendered made no pass and
    /// is not counted.
    fn count(
        self,
        path: Path,
        ladder: &str,
        passes: &[Pass],
        fast: Option<String>,
        region: (i32, i32, i32, i32),
        picture: Option<(&[u8], usize, usize)>,
    ) {
        let first = passes.iter().find(|p| p.stage == Stage::Tight).map(|p| p.words);
        // An Intel Mac's agreed answer makes no accurate pass; any other read made one, or none.
        if first.is_none() && path != Path::Agreed {
            return;
        }
        let polled = match (&self.got, &self.asked) {
            (Some(got), _) => Some(got.clone()),
            (None, Some(asked)) => Some(asked.try_wait()),
            (None, None) => None,
        };
        let paddle = match polled {
            None => shadow::Paddle::NotAsked,
            Some(Polled::NotYet) => shadow::Paddle::NotDone,
            Some(Polled::Failed) => shadow::Paddle::Failed,
            Some(Polled::Answered(None)) => shadow::Paddle::Nothing,
            Some(Polled::Answered(Some(r))) => shadow::Paddle::Text { text: r.text, score: r.score, ms: r.ms },
        };
        // Dropped here: a recognition that has not answered is cancelled, as nobody waits for it.
        drop(self.asked);
        let reading = shadow::Reading {
            path,
            first: first.flatten(),
            ladder: ladder.to_string(),
            whole: passes.iter().any(|p| p.stage == Stage::Whole),
            fast,
            paddle,
            fits: self.fits,
            passes: passes.len(),
            passes_beside: passes.iter().filter(|p| p.paddle_beside).count(),
        };
        let (case, due) = {
            let mut tally = SHADOW.lock().unwrap_or_else(|e| e.into_inner());
            let case = tally.add(&reading);
            (case, tally.due())
        };
        if let Some(line) = due {
            crate::logging::line("macos", &line);
        }
        let Some(slug) = case.slug() else { return };
        crate::logging::trace("macos", || shadow::trace_line(&reading, case, region).unwrap_or_default());
        if let Some((rgba, pw, ph)) = picture {
            keep_picture(&SHADOW_PICTURES, &format!("shadow-{slug}"), region, rgba, pw, ph, shadow::pixels_hash(rgba));
        }
    }
}

/// The shadow's counts since the application started (`ocr/shadow.rs`).
static SHADOW: Mutex<shadow::Tally> = Mutex::new(shadow::Tally::new());

/// The pictures of small regions the switch "Save the images OCR was given" has written this
/// session, per region and content: what every read was given, and what the shadow found worth a
/// look. Two lists, so that one never takes the other's room; 64 MiB a session at most between
/// them (`shadow::MAX_BYTES`).
static RAW_PICTURES: Mutex<shadow::Pictures> = Mutex::new(shadow::Pictures::new());
static SHADOW_PICTURES: Mutex<shadow::Pictures> = Mutex::new(shadow::Pictures::new());

/// The shadow's line once more at exit (`backend::ocr_exit_report`), with whatever was counted after
/// the last one: written when a read was counted, or when the recogniser loaded and none was.
pub fn shadow_report() {
    let tally = SHADOW.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if tally.reads > 0 || paddle_ocr::ready() {
        crate::logging::line("macos", &tally.line());
    }
}

/// Writes one picture of a small region, `w` by `h` RGBA pixels whose hash is `hash`
/// (`shadow::pixels_hash`), as `ocr-<kind>-<region>-<hash>.bmp` beside the application — when
/// `kept` has not written this content of this region yet this session, and the region's eight,
/// the session's 64 regions or the list's 32 MiB are not used up (`shadow::Pictures`); a line,
/// once, when any is. Only while the switch is on, which is when the caller asks.
#[allow(clippy::too_many_arguments)]
fn keep_picture(
    kept: &Mutex<shadow::Pictures>,
    kind: &str,
    region: (i32, i32, i32, i32),
    rgba: &[u8],
    w: usize,
    h: usize,
    hash: u32,
) {
    // The file `debug_dump` writes: its 54-byte header and four bytes a pixel.
    let bytes = 54 + w * h * 4;
    let decided = kept.lock().unwrap_or_else(|e| e.into_inner()).admit(kind, region, hash, bytes);
    let (x, y, rw, rh) = region;
    match decided {
        shadow::Picture::Write(name) => debug_dump(&name, rgba, w, h),
        shadow::Picture::Full { say: true } => crate::logging::line(
            "macos",
            &format!(
                "ocr: {} pictures of the {rw}x{rh} pt region at {x},{y} were written this session ({kind}); no \
                 more of it",
                shadow::PER_REGION
            ),
        ),
        shadow::Picture::Regions { say: true } => crate::logging::line(
            "macos",
            &format!(
                "ocr: pictures of {} regions were written this session ({kind}); none of another region",
                shadow::MAX_REGIONS
            ),
        ),
        shadow::Picture::Bytes { say: true } => crate::logging::line(
            "macos",
            &format!(
                "ocr: {} MiB of pictures were written this session ({kind}); no more",
                shadow::MAX_BYTES >> 20
            ),
        ),
        shadow::Picture::Seen
        | shadow::Picture::Full { .. }
        | shadow::Picture::Regions { .. }
        | shadow::Picture::Bytes { .. } => {}
    }
}

/// What a small region's rungs came to besides Vision's answer.
#[derive(Default)]
struct Climbed {
    /// The neural recogniser's text, when it is the answer.
    paddle: Option<String>,
    /// Where it read, in points of the region: the content crop.
    crop: Option<(i32, i32, i32, i32)>,
    agreed: Option<bool>,
    today: bool,
    /// The fast level's pass over the crop where the application made one — an Intel Mac's check
    /// (`Rungs::merged`), or the pass made only for the counts once the answer was picked
    /// (`Rungs::counted`): its text, empty for nothing; `None` where none was made or Vision
    /// refused it. For `ocr/shadow.rs`.
    compared: Option<String>,
}

/// What the rungs of one small region share: the capture, its content crop and pixels, and the
/// read's ladder.
struct Rungs<'a, 'l> {
    native: &'a CGImage,
    plan: &'a Plan,
    rgba: &'a [u8],
    px_w: usize,
    px_h: usize,
    scale: f64,
    lang: Option<&'a str>,
    debug: bool,
    ladder: &'a Ladder<'l>,
    started: Instant,
    /// The region in points, for the lines a ladder given up writes.
    w: i32,
    h: i32,
}

/// Whether a pass read any text.
fn has_text(read: &Option<VisionRead>) -> bool {
    read.as_ref().is_some_and(|(t, _, _)| !t.trim().is_empty())
}

impl Rungs<'_, '_> {
    /// The rungs the ladder's shape asks for, over the content crop rendered once. Today's shape
    /// is [`Rungs::accurate`] with the fast rung last: an accurate pass over the crop, the whole
    /// region when that read nothing and the plan cropped, then the enlarged crop and the fast
    /// model within the budget. The application's, wherever the neural recogniser has loaded, is
    /// [`Rungs::merged`], with `neural` asked already. A crop that could not be rendered reads as
    /// nothing, and no rung runs.
    fn climb(&self, neural: Option<&mut Neural>) -> (Option<VisionRead>, Climbed) {
        let Some((tight, buf)) = render(self.native, self.plan) else {
            return (None, Climbed::default());
        };
        if self.debug {
            let (pw, ph) = self.plan.out_size();
            debug_dump("ocr-debug.bmp", &buf, pw, ph);
        }
        let shape = self.ladder.shape;
        let out = if shape.paddle.merges() {
            self.merged(&tight, neural)
        } else if shape.paddle.reads() {
            self.with_paddle(&tight)
        } else if shape.first == Level::Fast {
            // The fast level first, its text taken unchecked; today's ladder when it read
            // nothing. `ocr-bench`'s `fast>acc`, for comparison only.
            let fast = run_vision(&tight, self.lang, FAST, Stage::FastFirst, &|bb| map_box(self.plan, self.scale, bb));
            if has_text(&fast) {
                (fast, Climbed::default())
            } else {
                (self.accurate(&tight, true), Climbed::default())
            }
        } else {
            (self.accurate(&tight, true), Climbed::default())
        };
        // `buf` is the bitmap context's backing store and the image created from it is a
        // copy-on-write of that memory. Dropping it before Vision has read the image would be a
        // use-after-free that only shows up on the machine nobody here owns: so after every pass
        // over it.
        drop(buf);
        out
    }

    /// The accurate ladder from its first rung, over the rendered content crop `tight`; the fast
    /// rung at its end only when `fast_last`.
    fn accurate(&self, tight: &CGImage, fast_last: bool) -> Option<VisionRead> {
        let first = self.rung_one(tight);
        self.after_rung_one(first, tight, fast_last)
    }

    /// What is left of the ladder's budget, which bounds a read's waits for the neural recogniser
    /// (`merge::left`): the budget counts every pass of the read so far, the check's included, and
    /// the waits.
    fn left(&self) -> Duration {
        merge::left(LADDER_BUDGET, self.started.elapsed())
    }

    /// Windows' rule over Vision's accurate ladder (`ocr/merge.rs`), the application's way wherever
    /// the neural recogniser has loaded; `neural` was asked when the read had its content crop.
    ///
    /// 1. Not asked after all — the exit has begun, its queue is full, its thread has gone — and
    ///    Vision reads alone: today's ladder, the fast rung included, as Windows' system recogniser
    ///    reads alone then.
    /// 2. An Intel Mac's check (`PaddleUse::FastMerge`), for a region whose ink is one line no wider
    ///    than twelve times its height and a language the fast level reads: one fast pass over the
    ///    tight crop, then the recogniser's answer, waited for no longer than that pass took and
    ///    never past the budget (`merge::check_wait`). Where the two read the same, spaces
    ///    included, that is the answer: the fast level's words, with the recogniser's spacing
    ///    (`merge::check`). Its time counts against the budget, so that a check that fails leaves
    ///    the enlarged rung less of it.
    /// 3. Otherwise Vision's accurate ladder — the tight crop, the whole region, the enlarged crop
    ///    within the budget — without the fast rung that ends today's: Windows has no such rung,
    ///    and here the recogniser is the ladder's last resort. It read "o" for sforzando's "0" on CI's
    ///    macOS 26 Mac, and text on a level meter, where the recogniser reads the digit
    ///    (`ocr-bench`, 2026-10-03).
    /// 4. When Vision refused the first accurate pass, the read answers nothing, as a refusal always
    ///    has here, and the recogniser's answer is not used: Windows' read fails then.
    /// 5. When the ladder read anything, that is the answer, and the recognition is cancelled with
    ///    the read; when it read nothing, the recogniser's text, waited for within what is left of
    ///    the budget — its text locates nothing, so the content crop is its box (`merge::pick`). A
    ///    read from a poll that made way for a key press's takes an answer already there, or none.
    /// 6. On the recognise thread of a Mac without the check, one pass of the fast level only for
    ///    the counts, once the answer is picked ([`Rungs::counted`]).
    fn merged(&self, tight: &CGImage, neural: Option<&mut Neural>) -> (Option<VisionRead>, Climbed) {
        let (plan, scale, ladder) = (self.plan, self.scale, self.ladder);
        let Some(neural) = neural.filter(|n| n.asked()) else {
            return (self.accurate(tight, true), Climbed { today: true, ..Climbed::default() });
        };
        let mut climbed = Climbed::default();
        if ladder.shape.paddle == PaddleUse::FastMerge && ladder.fast_ok && neural.fits {
            let began = Instant::now();
            let fast = run_vision(tight, self.lang, FAST, Stage::FastFirst, &|bb| map_box(plan, scale, bb));
            // A poll's read that has made way for a key press's waits for this too: a check that
            // fails costs the accurate ladder, far more than the wait.
            let (paddle, wait) = neural.poll(merge::check_wait(self.left(), began.elapsed()));
            let check = merge::check(fast.as_ref().map(|(_, _, lines)| lines.as_slice()), paddle.as_deref());
            neural.checked(wait, check.agreed);
            climbed.agreed = check.agreed;
            climbed.compared = fast.map(|(t, _, _)| t);
            if let Some(lines) = check.answer {
                return (Some(vision_read_of(lines)), climbed);
            }
        }
        let first = self.rung_one(tight);
        // Refused, not read: no reading of nothing, and nothing for the recogniser to answer.
        let refused = first.is_none();
        let read = self.after_rung_one(first, tight, false);
        let system = (!refused).then(|| read.as_ref().map_or("", |(t, _, _)| t.as_str()));
        // Waited for only where the ladder read nothing, as Windows waits, within what is left of
        // the budget — and not at all by a poll's read that a key press's waits behind, which
        // skipped the last rungs for it as well.
        let bound = if ladder.preempted() { Duration::ZERO } else { self.left() };
        let paddle = if merge::waits(system) { neural.wait(bound) } else { None };
        if let Pick::Paddle(text) = merge::pick(system, paddle.as_deref()) {
            climbed.paddle = Some(text.to_string());
            climbed.crop = Some(crop_in_points(plan, scale));
        }
        if self.counts_fast() {
            climbed.compared = self.counted(tight);
        }
        (read, climbed)
    }

    /// Whether this read makes a pass of the fast level only for the neural recogniser's counts
    /// (`ocr/shadow.rs`): on a Mac that does not check its fast level (`PaddleUse::Merge`), on
    /// `host.ocr.read`'s recognise thread only — never on the event loop, which carries the keyboard
    /// tap — for a language that level reads, for a region it could be checked on (one line of ink,
    /// not too wide), and not while an interactive read waits behind this one. What an
    /// Apple-silicon Mac's check would read, measured before it has one.
    fn counts_fast(&self) -> bool {
        let ladder = self.ladder;
        ladder.shape.paddle == PaddleUse::Merge
            && !ladder.on_event_loop
            && ladder.fast_ok
            && !ladder.preempted()
            && paddle_pre::fits(self.plan.ink_w, self.plan.ink_h, self.plan.lines)
    }

    /// The pass of the fast level over the rendered crop made only for the counts, once the read's
    /// answer is picked, compared and never used: its text, empty for nothing, `None` where Vision
    /// refused it. Made after the answer, so that neither the answer nor the ladder's budget nor
    /// the wait for the recogniser depends on it; the read returns that much later. It costs the
    /// fast level's pass — over a small field 11 to 20 ms on the CI's Intel Mac, 5 to 12 ms on its
    /// arm64 ones, warm (`ocr-bench`, 2026-10-02).
    fn counted(&self, tight: &CGImage) -> Option<String> {
        run_vision(tight, self.lang, FAST, Stage::FastCompared, &|bb| map_box(self.plan, self.scale, bb)).map(|(t, _, _)| t)
    }

    /// The first accurate pass over the content crop, in the revision the shape names.
    fn rung_one(&self, tight: &CGImage) -> Option<VisionRead> {
        let (plan, scale) = (self.plan, self.scale);
        with_revision(self.ladder.shape.rung1_revision, || {
            run_vision(tight, self.lang, ACCURATE, Stage::Tight, &|bb| map_box(plan, scale, bb))
        })
    }

    /// The rungs after the first accurate pass, which answered `first`.
    fn after_rung_one(&self, first: Option<VisionRead>, tight: &CGImage, fast_last: bool) -> Option<VisionRead> {
        let (plan, scale, ladder, started) = (self.plan, self.scale, self.ladder, self.started);
        let (w, h) = (self.w, self.h);
        let again = ladder.shape.rev2_second;
        match first {
            // Only when the tightened pass found nothing, and only when it actually cropped
            // — if it already fell back to the whole region there is no second input to try
            // and the retry would just pay for the same answer twice. (Revision 2 over the same
            // crop, `ocr-bench`'s `rev3>rev2`, is another model rather than another input.)
            Some((ref t, _, _)) if t.trim().is_empty() && (plan.cropped || again) => {
                let second = if again {
                    crate::logging::trace("macos", || {
                        "ocr: tightened pass read nothing, trying revision 2 over the same crop".to_string()
                    });
                    with_revision(Some(2), || {
                        run_vision(tight, self.lang, ACCURATE, Stage::TightAgain, &|bb| map_box(plan, scale, bb))
                    })
                } else {
                    crate::logging::trace("macos", || {
                        "ocr: tightened pass read nothing, trying the whole region".to_string()
                    });
                    let whole = Plan::whole_with_ink(self.rgba, self.px_w, self.px_h, plan.ink_h);
                    render(self.native, &whole).and_then(|(img, buf)| {
                        if self.debug {
                            let (pw, ph) = whole.out_size();
                            debug_dump("ocr-debug-retry.bmp", &buf, pw, ph);
                        }
                        let r = run_vision(&img, self.lang, ACCURATE, Stage::Whole, &|bb| map_box(&whole, scale, bb));
                        drop(buf);
                        r
                    })
                };
                let exhausted = second.as_ref().is_none_or(|(t, _, _)| t.trim().is_empty());
                let spent = started.elapsed();
                if exhausted && spent < LADDER_BUDGET && !ladder.preempted() {
                    bigger_then_faster(
                        self.native, plan, self.rgba, self.px_w, self.px_h, scale, self.lang, self.debug, ladder,
                        fast_last,
                    )
                } else if exhausted && ladder.preempted() {
                    // Not a failure: a read from a poll made way for one somebody is waiting
                    // for, and the poll asks again on its next tick. Traced, because a poll
                    // would otherwise say it every tick.
                    crate::logging::trace("macos", || {
                        format!(
                            "ocr: skipped the last rungs for a {w}x{h} pt region after {} ms: an \
                             interactive read is waiting",
                            started.elapsed().as_millis()
                        )
                    });
                    second
                } else if exhausted && !ladder.on_event_loop {
                    // Out of budget on `host.ocr.read`'s recognise thread: nothing waits on
                    // the event loop here, but every read queued behind this one does.
                    crate::logging::line(
                        "macos",
                        &format!(
                            "ocr: gave up on a {w}x{h} pt region after {} ms rather than keep the \
                             reads behind it waiting",
                            started.elapsed().as_millis()
                        ),
                    );
                    second
                } else if exhausted {
                    // Out of budget with nothing to show. Said out loud rather than traced,
                    // because this is the shape of a real failure the tester met: a read
                    // that returns nothing is also the most expensive read there is, and
                    // this thread carries the keyboard. Four passes over a field that will
                    // never resolve is how a keystroke goes missing.
                    crate::logging::line(
                        "macos",
                        &format!(
                            "ocr: gave up on a {w}x{h} pt region after {} ms rather than \
                             keep the event loop waiting",
                            started.elapsed().as_millis()
                        ),
                    );
                    second
                } else {
                    second
                }
            }
            other => other,
        }
    }

    /// The shapes of `ocr-bench`'s other strategies that read the neural recogniser's answer, as
    /// they were measured before the application's was chosen ([`Rungs::merged`]). The recogniser
    /// is asked about the content crop when the region is its to read — one line of ink, not too
    /// wide — and it is ready; any other region is read by today's ladder.
    fn with_paddle(&self, tight: &CGImage) -> (Option<VisionRead>, Climbed) {
        let shape = self.ladder.shape;
        let plan = self.plan;
        let today = || (self.accurate(tight, true), Climbed { today: true, ..Climbed::default() });
        if !paddle_pre::fits(plan.ink_w, plan.ink_h, plan.lines) || !paddle_ocr::ready() {
            return today();
        }
        let qos = if shape.paddle.urgent() { Qos::Interactive } else { Qos::Utility };
        let (cw, ch, pixels) = paddle_crop(self.rgba, self.px_w, plan);
        let Some(asked) = paddle_ocr::ask_with(cw, ch, &pixels, Tighten::for_scale(self.scale), qos) else {
            return today();
        };
        let mut climbed = Climbed { crop: Some(crop_in_points(plan, self.scale)), ..Climbed::default() };
        match shape.paddle {
            PaddleUse::Alone => {
                climbed.paddle = asked.wait_read().map(|r| r.text);
                (None, climbed)
            }
            PaddleUse::Fallback => {
                // Windows' rule after the first accurate pass alone (the application's waits for
                // the accurate ladder): that pass answers when it read anything, and the
                // recogniser, dropped, is cancelled — as it is when the pass was refused, which is
                // no reading; when it read nothing, the recogniser's text.
                let first = self.rung_one(tight);
                if first.is_none() || has_text(&first) {
                    drop(asked);
                    return (first, climbed);
                }
                if let Some(read) = asked.wait_read() {
                    climbed.paddle = Some(read.text);
                    return (first, climbed);
                }
                if shape.rest {
                    (self.after_rung_one(first, tight, true), climbed)
                } else {
                    (first, climbed)
                }
            }
            PaddleUse::Agree | PaddleUse::AgreeStrict | PaddleUse::AgreeChecked => {
                let fast = run_vision(tight, self.lang, FAST, Stage::FastFirst, &|bb| map_box(plan, self.scale, bb));
                // Waited for whatever fast read: it is the check on fast.
                let p = asked.wait_read().map(|r| r.text);
                // Judged as the application's check judges, spaces included, so that these differ
                // from `prod` on an Intel Mac only in what follows a check that failed.
                let check = merge::check(fast.as_ref().map(|(_, _, lines)| lines.as_slice()), p.as_deref());
                climbed.agreed = check.agreed;
                // Two models read the same: fast's text and words, with the recogniser's spacing.
                if let Some(lines) = check.answer {
                    return (Some(vision_read_of(lines)), climbed);
                }
                let f = fast.as_ref().map(|(t, _, _)| t.trim().to_string()).filter(|t| !t.is_empty());
                match (f, p) {
                    // They differ, or the recogniser read nothing: the accurate ladder, without
                    // the fast rung at its end — an unchecked fast reading is what this avoids.
                    // When it reads nothing too, the answer is nothing.
                    (Some(_), _) => (self.accurate(tight, false), climbed),
                    // Fast read nothing: the reading rules as they stand, which never speak the
                    // recogniser alone — the accurate ladder, without the fast rung at its end, an
                    // unchecked reading of the level that has just read nothing; nothing when it
                    // reads nothing.
                    (None, _) if shape.paddle == PaddleUse::AgreeChecked => (self.accurate(tight, false), climbed),
                    // Fast read nothing and the recogniser something: an accurate pass first, and
                    // the recogniser's text only when that reads nothing too.
                    (None, Some(p)) if shape.paddle == PaddleUse::AgreeStrict => {
                        let first = self.rung_one(tight);
                        if !has_text(&first) {
                            climbed.paddle = Some(p);
                        }
                        (first, climbed)
                    }
                    (None, Some(p)) => {
                        climbed.paddle = Some(p);
                        (fast, climbed)
                    }
                    // Neither read anything: today's ladder.
                    (None, None) => (self.accurate(tight, true), climbed),
                }
            }
            // Not climbed here: `climb` sends the application's own to `merged`, and `Off` reads
            // nothing of the recogniser's.
            PaddleUse::Off | PaddleUse::Merge | PaddleUse::FastMerge => (self.accurate(tight, true), climbed),
        }
    }
}

/// The content crop `Plan::content` found — a well's inside included, with its margin — as
/// top-down RGBA: what the neural recogniser is handed on a Mac (`ocr-bench`'s `paddle-crop`). A
/// Mac's region holds more surroundings than a Windows one, and the recogniser takes its
/// background from the four corners.
fn paddle_crop(rgba: &[u8], px_w: usize, plan: &Plan) -> (u32, u32, Vec<u8>) {
    let mut out = Vec::with_capacity(plan.cw * plan.ch * 4);
    for y in plan.y0..plan.y0 + plan.ch {
        let at = (y * px_w + plan.x0) * 4;
        if let Some(row) = rgba.get(at..at + plan.cw * 4) {
            out.extend_from_slice(row);
        }
    }
    let rows = (out.len() / (plan.cw * 4).max(1)) as u32;
    (plan.cw as u32, rows, out)
}

/// The content crop in points of the region, as an answer's fallback box.
fn crop_in_points(plan: &Plan, scale: f64) -> (i32, i32, i32, i32) {
    let pt = |px: usize| (px as f64 / scale).round() as i32;
    (pt(plan.x0), pt(plan.y0), pt(plan.cw).max(1), pt(plan.ch).max(1))
}

/// `f` with every request made on this thread asking for revision `r`, and the override as it was
/// after; `None` leaves everything as it is.
fn with_revision<R>(r: Option<usize>, f: impl FnOnce() -> R) -> R {
    let Some(r) = r else { return f() };
    struct Restore(Option<usize>);
    impl Drop for Restore {
        fn drop(&mut self) {
            REVISION_OVERRIDE.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(REVISION_OVERRIDE.with(|c| c.replace(Some(r))));
    f()
}

/// Several regions, one capture.
///
/// The promise is not speed — it is SIMULTANEITY. Two values a module has to compare with each
/// other must come from the same instant, and reading them one after another is how a note name
/// and its cent offset come to disagree.
///
/// Falls back to one capture each wherever the shortcut cannot be trusted: fewer than two
/// regions, a degenerate one, a capture that failed, or a capture that came back a different
/// size than asked for — which means it was clipped at a screen edge, and every offset computed
/// from it would point somewhere else.
pub fn recognize_regions(
    regions: &[(i32, i32, i32, i32)],
    lang: Option<&str>,
) -> Vec<Result<OcrText, String>> {
    let one_each = || -> Vec<Result<OcrText, String>> {
        regions.iter().map(|(x, y, w, h)| recognize(*x, *y, *w, *h, lang)).collect()
    };
    if regions.len() < 2 || regions.iter().any(|(_, _, w, h)| *w <= 0 || *h <= 0) {
        return one_each();
    }
    // No box that fits the coordinate range (two regions two billion points apart): one
    // capture each, as for a degenerate region.
    let Some((x0, y0, bw, bh)) = crate::region::bounding_box(regions) else {
        return one_each();
    };

    objc2::rc::autoreleasepool(|_| {
        let Some((big, scale)) = super::capture::capture_backing(x0, y0, bw, bh) else {
            report_capture_failure(x0, y0, bw, bh);
            return one_each();
        };
        let (gw, gh) = (CGImage::width(Some(&big)), CGImage::height(Some(&big)));
        let (want_w, want_h) =
            (((bw as f64) * scale).round() as usize, ((bh as f64) * scale).round() as usize);
        if gw != want_w || gh != want_h {
            crate::logging::trace("macos", || {
                format!(
                    "ocr: the {bw}x{bh} pt enclosing capture came back {gw}x{gh} px, not \
                     {want_w}x{want_h} — reading each region on its own instead"
                )
            });
            return one_each();
        }

        regions
            .iter()
            .map(|(x, y, w, h)| {
                let started = Instant::now();
                let debug = crate::appcfg::ocr_debug();
                // A pass-through plan: crop only, no upscale and no border, so `render`'s
                // clipping gives exactly this region's pixels out of the shared capture.
                let cut = Plan {
                    x0: (((x - x0) as f64) * scale).round() as usize,
                    y0: (((y - y0) as f64) * scale).round() as usize,
                    cw: ((*w as f64) * scale).round() as usize,
                    ch: ((*h as f64) * scale).round() as usize,
                    up: 1,
                    pad: 0,
                    ink_h: 1,
                    ink_w: 1,
                    lines: 0,
                    bg: [0.0; 3],
                    cropped: false,
                    blank: false,
                };
                match render(&big, &cut) {
                    Some((img, buf)) => {
                        let ladder = Ladder { shape: Ladder::shape_now(), ..Ladder::FULL };
                        let r = recognize_captured(
                            &img, scale, *x, *y, *w, *h, lang, started, Capture::Shared, debug,
                            &ladder,
                        );
                        // `buf` backs the image copy-on-write; it has to outlive every read
                        // of it, which on a machine nobody here owns is not a thing to leave
                        // to the optimiser.
                        drop(buf);
                        r
                    }
                    None => Err("could not cut this region out of the shared capture".to_string()),
                }
            })
            .collect()
    })
}

// ── host.ocr.read's two threads ─────────────────────────────────────────────────────────────

/// The pixels the capture stage took for one read: one backing-resolution capture of the
/// regions' bounding box, or one per region when the box would be wasteful or came back
/// clipped. `CGImage` is `Send + Sync` in these bindings, so this crosses to the recognise
/// thread as it is.
pub enum Shot {
    Shared { big: CFRetained<CGImage>, scale: f64, origin: (i32, i32) },
    /// Per region, in order: its capture and scale, or `None` when it could not be taken.
    Each(Vec<Option<(CFRetained<CGImage>, f64)>>),
    /// Nothing to read, and why: a snapshot that kept no backing image.
    Failed(String),
}

impl Default for Shot {
    fn default() -> Shot {
        Shot::Each(Vec::new())
    }
}

/// What the recognise thread does first: ask for the quality of service a person waiting for
/// an answer gets. Designed rather than measured — whether macOS throttles this thread when the
/// application is in the background is on the list in TODO.md.
pub fn thread_init(role: OcrThread) {
    if role == OcrThread::Recognise {
        let rc = set_thread_qos(libc::qos_class_t::QOS_CLASS_USER_INITIATED);
        if rc != 0 {
            crate::logging::line(
                "macos",
                &format!("ocr: the recognise thread kept its quality of service ({rc})"),
            );
        }
    }
}

/// Asks for `class` as the calling thread's quality of service: what the recognise thread does
/// first, and what the neural recogniser's thread does before each region (`paddle_ocr::Qos`).
/// `pthread_set_qos_class_self_np`'s answer, 0 when it took.
pub(crate) fn set_thread_qos(class: libc::qos_class_t) -> libc::c_int {
    // SAFETY: a plain call about the calling thread.
    unsafe { libc::pthread_set_qos_class_self_np(class, 0) }
}

extern "C" {
    /// `<sys/qos.h>`, macOS 10.10 and later: the calling thread's quality-of-service class.
    /// Declared here as a plain number, for this file and `ocr-bench`: the libc crate binds the
    /// class as a Rust enum, into which a value it does not list could not be read back soundly.
    #[link_name = "qos_class_self"]
    fn qos_class_self_raw() -> u32;
}

/// The calling thread's quality-of-service class, as `<sys/qos.h>` numbers it: what a read that
/// may wait for the neural recogniser has it run at (`paddle_ocr::Qos::Reader`).
pub(crate) fn thread_qos() -> u32 {
    // SAFETY: a plain question about the calling thread.
    unsafe { qos_class_self_raw() }
}

/// A quality-of-service class in words, as the log and `ocr-bench` say it.
pub(crate) fn qos_word(class: u32) -> String {
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

/// Says once a thread at which quality of service the neural recogniser runs the regions this
/// thread asks it for (`paddle_ocr::Qos::Reader`), and the thread's own class where the two differ:
/// what the thread reports depends on how the application was started, and the event loop's had
/// not been seen on a real Mac when this was written.
fn say_reader_class() {
    thread_local! {
        static SAID: Cell<bool> = const { Cell::new(false) };
    }
    if SAID.with(|s| s.replace(true)) {
        return;
    }
    let own = thread_qos();
    let class = paddle_ocr::class_for(Qos::Reader, || own);
    let mut line = format!(
        "ocr: the neural recogniser runs the regions {} asks for at {}",
        this_thread(),
        qos_word(class)
    );
    if own != class {
        line.push_str(&format!(", the thread's own class being {}", qos_word(own)));
    }
    crate::logging::line("macos", &line);
}

/// A snapshot round's captures (`OcrWorker::frames`), on the capture thread: one
/// `capture::frame` per rectangle, each at the display's own resolution and kept beside its
/// point-sized pixels. The source is ignored here, as everywhere on this platform; `poll` skips
/// the flat-picture watch for a change wait's rounds.
pub fn frames_for_round(
    regions: &[(i32, i32, i32, i32)],
    _src: CaptureSource,
    poll: bool,
) -> Vec<Result<crate::backend::frame::Frame, String>> {
    crate::loop_guard::off_loop("a snapshot round");
    regions.iter().map(|&(x, y, w, h)| super::capture::frame(x, y, w, h, poll)).collect()
}

/// A read of a snapshot (`OcrWorker::shot_of`): the backing image the snapshot kept, as the
/// capture stage's shared capture — so the read is as sharp as a live one. The regions arrive cut
/// to the part the image covers (`Frame::ocr_rect`); a region the image does not hold after all
/// fails in its own slot when it is cut out (`render`). A snapshot without a backing image — none
/// is made without one on this platform — answers every region with the reason.
pub fn shot_of(frame: &crate::backend::frame::Frame, _regions: &[(i32, i32, i32, i32)]) -> Shot {
    crate::loop_guard::off_loop("a snapshot's pixels for a text read");
    match &frame.native {
        Some(n) => Shot::Shared { big: n.image.clone(), scale: n.scale, origin: (n.on.x, n.on.y) },
        None => Shot::Failed("this snapshot kept no picture at the display's resolution to read text from".to_string()),
    }
}

/// The capture stage of `host.ocr.read`. The source is ignored here, as everywhere on this
/// platform.
pub fn capture_for_read(regions: &[(i32, i32, i32, i32)], _src: CaptureSource) -> (Shot, usize) {
    crate::loop_guard::off_loop("a capture for a text read");
    objc2::rc::autoreleasepool(|_| {
        let rects: Vec<Rect> = regions.iter().copied().map(Rect::from_tuple).collect();
        let bytes_of = |img: &CGImage| CGImage::width(Some(img)) * CGImage::height(Some(img)) * 4;
        if let CapturePlan::BoundingBox(b) = plan::capture_plan(&rects) {
            if let Some((big, scale)) = super::capture::capture_backing(b.x, b.y, b.w, b.h) {
                let (gw, gh) = (CGImage::width(Some(&big)), CGImage::height(Some(&big)));
                let want = (((b.w as f64) * scale).round() as usize, ((b.h as f64) * scale).round() as usize);
                if (gw, gh) == want {
                    let bytes = bytes_of(&big);
                    return (Shot::Shared { big, scale, origin: (b.x, b.y) }, bytes);
                }
            } else {
                report_capture_failure(b.x, b.y, b.w, b.h);
            }
        }
        let each: Vec<Option<(CFRetained<CGImage>, f64)>> = regions
            .iter()
            .map(|&(x, y, w, h)| {
                let got = super::capture::capture_backing(x, y, w, h);
                if got.is_none() && w > 0 && h > 0 {
                    report_capture_failure(x, y, w, h);
                }
                got
            })
            .collect();
        let bytes = each.iter().flatten().map(|(img, _)| bytes_of(img)).sum();
        (Shot::Each(each), bytes)
    })
}

/// The recognise stage: each region through exactly the pipeline `recognize` runs, the ladder
/// and the blank guard included, with the rungs `ctx` allows. One region at a time through
/// `Recognise::each`, so a closing application starts no further one and the hang guard hears
/// of every region answered.
pub fn recognise_shot(
    shot: &Shot,
    regions: &[(i32, i32, i32, i32)],
    ctx: &Recognise,
) -> Vec<Result<OcrText, String>> {
    crate::loop_guard::off_loop("the recognise stage of a text read");
    let ladder =
        Ladder { fast_ok: ctx.fast_ok, preempt: ctx.preempt, on_event_loop: false, shape: Ladder::shape_now() };
    let debug = crate::appcfg::ocr_debug();
    ctx.each(regions.iter().enumerate(), |(i, &(x, y, w, h))| {
        objc2::rc::autoreleasepool(|_| {
            let started = Instant::now();
            match shot {
                Shot::Shared { big, scale, origin } => {
                    let cut = Plan {
                        x0: (((x - origin.0) as f64) * scale).round() as usize,
                        y0: (((y - origin.1) as f64) * scale).round() as usize,
                        cw: ((w as f64) * scale).round() as usize,
                        ch: ((h as f64) * scale).round() as usize,
                        up: 1,
                        pad: 0,
                        ink_h: 1,
                        ink_w: 1,
                        lines: 0,
                        bg: [0.0; 3],
                        cropped: false,
                        blank: false,
                    };
                    match render(big, &cut) {
                        Some((img, buf)) => {
                            let r = recognize_captured(
                                &img, *scale, x, y, w, h, ctx.lang, started, Capture::Apart, debug,
                                &ladder,
                            );
                            // `buf` backs the image copy-on-write; see `recognize_regions`.
                            drop(buf);
                            r
                        }
                        None => Err("could not cut this region out of the shared capture".to_string()),
                    }
                }
                Shot::Failed(why) => Err(why.clone()),
                Shot::Each(each) => match each.get(i).and_then(|c| c.as_ref()) {
                    Some((native, scale)) => recognize_captured(
                        native, *scale, x, y, w, h, ctx.lang, started, Capture::Apart, debug, &ladder,
                    ),
                    None => Err(
                        "screen capture failed — if every read fails, grant this application \
                         Screen Recording and restart it"
                            .to_string(),
                    ),
                },
            }
        })
    })
}

/// What Vision reads at each level on this macOS, and the user's languages. Asked on the
/// recognise thread, once, and again when a language did not resolve.
pub fn languages() -> crate::ocr::lang::Languages {
    objc2::rc::autoreleasepool(|_| {
        let supported = |level: VNRequestTextRecognitionLevel| -> Vec<String> {
            let request = VNRecognizeTextRequest::new();
            request.setRecognitionLevel(level);
            // SAFETY: an instance method of a request made on this thread (macOS 12 and later,
            // which is the oldest this application runs on).
            match unsafe { request.supportedRecognitionLanguagesAndReturnError() } {
                Ok(list) => list.iter().map(|s| s.to_string()).collect(),
                Err(e) => {
                    warn_once(
                        "ocr-languages",
                        &format!("ocr: Vision did not list its languages — {}", e.localizedDescription()),
                    );
                    Vec::new()
                }
            }
        };
        let available = supported(ACCURATE);
        let fast = supported(FAST);
        let preferred = NSLocale::preferredLanguages().iter().map(|s| s.to_string()).collect();
        crate::ocr::lang::Languages { available, fast, preferred }
    })
}

fn empty() -> OcrText {
    OcrText {
        text: String::new(),
        words: Vec::new(),
        lines: Vec::new(),
        fallback: None,
        // Not the blank guard's answer. Every caller of this is a fault of its own — a
        // zero-sized region, a capture that failed, pixels that could not be read back —
        // and each says so in the log; the guard overrides this at its one site.
        skipped: false,
    }
}

/// Makes Vision's first pass now, on a thread of its own, so that the first real recognition
/// does not.
///
/// The first pass in a process costs what no later one does — the warm-up over six bars that this
/// was until 2026-10 took 0.2–0.33 s on a Mac mini M1, 0.3–0.85 s on the CI's virtual Macs and
/// 1.7–1.8 s on an Intel MacBook Air (2020); over a line of words it has not been timed on a Mac
/// yet — and the legacy calls run synchronously on the pump thread, which is also the thread
/// carrying speech, timers and the overlay's own polling. Paying it there means a frozen interface
/// and a late announcement at exactly the moment a user first asked to read something. `docs/
/// macos-port.md` names this as one of the three failures the port is shaped to avoid. Whether a
/// first pass costs that much again on every other thread is open (TODO.md); the recognise thread
/// makes one of its own after this one (`warm_up_recognise`), and the two lines tell.
///
/// Nothing crosses the thread boundary: every Objective-C object is made on the thread that
/// uses it, because none of them are `Send`, and the result is thrown away. Vision's request
/// handler is documented as usable from any thread and needs no run loop, which is what makes
/// this legal at all.
#[allow(dead_code)] // Called from `MacBackend::new`; see this file's entry in docs/macos-port.md.
pub fn warm_up() {
    // The turn is taken before the thread exists, so that the recognise thread, started just
    // after, cannot find it free; it ends when the pass does, or when the thread unwinds.
    let turn = FIRST_WARM_UP.take();
    // And the neural recogniser's warm-up waits for this one to end in the same way.
    let warmed = VISION_WARM.opener();
    // Named, so that its line, and the first-pass lines after it, say whose pass was whose.
    let spawned = std::thread::Builder::new().name("ocr-warm-up".to_string()).spawn(move || {
        let _turn = turn;
        let _warmed = warmed;
        warm_up_here(None, None);
    });
    // A thread that could not start drops its closure, and the turn and the opener with it.
    if let Err(e) = spawned {
        crate::logging::line(
            "macos",
            &format!("ocr: the warm-up thread could not be started ({e}); the first recognition makes Vision's first pass instead, which will be slow"),
        );
    }
}

/// The two warm-ups' turns (`cost::Gate`): the one on a thread of its own first, then the
/// recognise thread's.
static FIRST_WARM_UP: cost::Gate = cost::Gate::new();

/// Opened when Vision's warm-up on a thread of its own has ended — its pass done, its thread
/// unwound, or never started — for the neural recogniser's warm-up, which waits for it
/// (`backend::warmup_ocr`): two engines warming at once compete on the few cores of an Intel Mac.
/// Stopped by the exit (`backend::stop_ocr_warmup`), so that a Vision warm-up that never ends
/// cannot hold it.
pub(crate) static VISION_WARM: cost::Latch = cost::Latch::new();

/// The recognise thread's warm-up (`OcrWorker::warm_up`), once it has read the languages: one pass
/// in `lang`, the language its reads are made in when they name none, so that the first
/// `host.ocr.read` pays no first pass of any kind — made after the warm-up on a thread of its own
/// has ended, not beside it. Its line then times a first pass on another thread of a warm
/// process, and the first line a first pass in the process: whether a first pass is a cost per
/// process or per thread, the question the logs of September 2026 left open. The wait is on the
/// hang clock (`ocr/service.rs`): should the first warm-up never end, reads are refused with the
/// reason after `policy::HANG`, as behind any recognition that does not answer.
pub fn warm_up_recognise(lang: Option<&str>) {
    crate::loop_guard::off_loop("the recogniser's warm-up");
    let waited = FIRST_WARM_UP.wait();
    warm_up_here(lang, Some(waited.as_secs_f64() * 1000.0));
}

/// The warm-up's pass, on the calling thread: one accurate pass over a line of printed words, in
/// `lang` (Vision's default for none), and a line saying how long it took and whether Vision read
/// the words — so the warm-up is a self-test as well. `waited`: how long the calling thread waited
/// for its turn, for the line. On an Intel Mac one pass of the fast level after it, with a line of
/// its own (`cost::fast_warm_up_line`).
pub fn warm_up_here(lang: Option<&str>, waited: Option<f64>) {
    // A thread that is not the main one has no autorelease pool of its own and no run loop to
    // drain one, so everything Vision autoreleases here would simply stay.
    objc2::rc::autoreleasepool(|_| warm_up_in_pool(lang, waited))
}

/// The picture the warm-up reads: `ocr-bench`'s `line@1x` (crate::ocr::bench::FIXTURES), a line
/// of nine printed words, drawn by tools/ocr-fixtures/make.py and carried in the executable. One
/// picture for both, so that the benchmark's warm-up over words is this one.
///
/// Words rather than the six dark bars it read until 2026-10: a pass over bars found no text in
/// any Mac's log, so whether the recogniser — the second of Vision's two models, the one the
/// first real read would otherwise wait for — ran at all was unproven. And a picture whose text
/// is known turns the warm-up into a test: a report of macOS 27 has Vision's accurate model
/// answering nothing, without an error, and with this the first line of a session says so.
///
/// Not a screen capture on purpose: warming must not depend on Screen Recording having been
/// granted, and it must not read the user's screen before anything has asked it to.
const WARM_UP_PICTURE: &str = "line@1x";

/// The warm-up's picture, drawn into a bitmap, and the text it says.
fn warm_up_page() -> Result<(CFRetained<CGImage>, Vec<u8>, &'static str), String> {
    let f = crate::ocr::bench::fixture(WARM_UP_PICTURE)
        .ok_or_else(|| format!("the executable carries no picture {WARM_UP_PICTURE}"))?;
    let (image, buf) = picture_from_png(f.png, f.px())?;
    Ok((image, buf, f.accept.first().copied().unwrap_or_default()))
}

fn warm_up_in_pool(lang: Option<&str>, waited: Option<f64>) {
    let started = Instant::now();
    let (image, buf, expected) = match warm_up_page() {
        Ok(page) => page,
        Err(why) => {
            crate::logging::line(
                "macos",
                &format!("ocr: could not draw the warm-up's test line ({why}); the first recognition on this thread makes Vision's first pass instead, which will be slow"),
            );
            return;
        }
    };
    // Its boxes placed in the picture, as a read's are in its region: the words then come back in
    // reading order (`reading_order`), whatever order Vision observed the line's pieces in, and
    // are compared with the line as printed.
    let plan = Plan::identity(CGImage::width(Some(&image)), CGImage::height(Some(&image)));
    journal_open();
    let read = run_vision(&image, lang, ACCURATE, Stage::WarmUp, &|bb| map_box(&plan, 1.0, bb));
    let beside = journal_take().iter().any(|p| p.beside);
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let text = read.as_ref().map(|(t, _, _)| t.as_str());
    crate::logging::line("macos", &cost::warm_up_line(ms, &this_thread(), text, expected, beside, waited));
    // On an Intel Mac a read's first pass is the fast level's, checked by the neural recogniser
    // (`Rungs::merged`), and that pass would otherwise be its level's first on the thread: one more
    // over the same line, in Vision's default language, which the fast level reads wherever it
    // reads any. Its own line, so that the accurate pass's stays the self-test it was.
    if intel_mac() {
        let started = Instant::now();
        let fast = run_vision(&image, None, FAST, Stage::WarmUp, &|bb| map_box(&plan, 1.0, bb));
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        crate::logging::line("macos", &cost::fast_warm_up_line(ms, &this_thread(), fast.is_none()));
    }
    drop(buf);
}

/// A PNG the executable carries, decoded by CoreGraphics at the size it must have and drawn once
/// into a bitmap of its own, so that it reaches Vision as a capture does — as pixels — rather than
/// as an image backed by PNG data that may be decoded again whenever it is read. The bitmap comes
/// back beside the image, which shares it copy-on-write; see `render`.
fn picture_from_png(png: &'static [u8], size: (usize, usize)) -> Result<(CFRetained<CGImage>, Vec<u8>), String> {
    let data = CFData::from_static_bytes(png);
    let provider = CGDataProvider::with_cf_data(Some(&data))
        .ok_or_else(|| "CoreGraphics made no data provider for it".to_string())?;
    // SAFETY: no decode array (null is documented as "none") and a live provider.
    let decoded = unsafe {
        CGImage::with_png_data_provider(
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| "CoreGraphics did not decode it".to_string())?;
    let got = (CGImage::width(Some(&decoded)), CGImage::height(Some(&decoded)));
    if got != size {
        return Err(format!("it decoded at {}x{} px, not {}x{}", got.0, got.1, size.0, size.1));
    }
    render(&decoded, &Plan::identity(got.0, got.1)).ok_or_else(|| "it could not be drawn into a bitmap".to_string())
}

/// The revisions of text recognition this macOS lists, asked of `VNRecognizeTextRequest`'s class:
/// `supportedRevisions` is a class method, and the binding's `VNRequest::supportedRevisions()`
/// would ask the base class.
fn supported_revisions() -> Vec<usize> {
    // SAFETY: `+supportedRevisions` is on every request class from macOS 10.13 and returns an
    // index set, or nil, which `Option` takes.
    let set: Option<Retained<NSIndexSet>> =
        unsafe { msg_send![VNRecognizeTextRequest::class(), supportedRevisions] };
    set.map(|set| (1..=16).filter(|r| set.containsIndex(*r)).collect()).unwrap_or_default()
}

/// The `[env]` line `vision`: the revision a new text request of this application actually
/// carries — the request every pass makes, before anything is asked of it — and the revisions this
/// macOS lists. The request's revision is left at Vision's default (`new_request`), and Apple
/// documents that default as the newest for the SDK the application was built with, not for the
/// running system, so the log says which it is. A request object and a class method, on the
/// event loop at start: no recognition, no model.
pub fn vision_report() -> String {
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: a property of a request made on this thread.
        let carried = unsafe { new_request(None, ACCURATE).revision() };
        let listed = supported_revisions();
        let listed = if listed.is_empty() {
            "lists none".to_string()
        } else {
            format!("lists revisions {}", listed.iter().map(|r| r.to_string()).collect::<Vec<_>>().join(", "))
        };
        format!("a new text request carries revision {carried}; this macOS {listed}")
    })
}

/// Says, once, why nothing could be captured.
///
/// Missing Screen Recording is the likely cause and it is invisible from inside the process:
/// the permission is not something an application can grant itself, and without it macOS
/// does not return an error — it returns a picture of the wallpaper. A tester who is blind
/// cannot see that the overlay is reading an empty desktop, so it has to be said in words.
fn report_capture_failure(x: i32, y: i32, w: i32, h: i32) {
    if screen_capture_permitted() {
        warn_once(
            "ocr-capture",
            &format!("ocr: the screen region {w}x{h} at {x},{y} could not be captured, although Screen Recording is granted"),
        );
    }
}

/// Whether this application may read the screen — asked of the system once, then remembered.
///
/// Asked once because the callers are polls: a region that reads as blank sixteen times a
/// second must not become sixteen TCC queries a second on the thread that also owns the
/// keyboard tap. Remembering it is safe in the direction that matters, since a Screen
/// Recording grant made while the application is running does not take effect until it is
/// restarted anyway. Logs the missing case itself, because every caller wants to say the
/// same sentence.
fn screen_capture_permitted() -> bool {
    thread_local! {
        static ALLOWED: RefCell<Option<bool>> = const { RefCell::new(None) };
    }
    ALLOWED.with(|cell| {
        let mut cell = cell.borrow_mut();
        if let Some(known) = *cell {
            return known;
        }
        let allowed = CGPreflightScreenCaptureAccess();
        *cell = Some(allowed);
        if !allowed {
            crate::logging::line(
                "macos",
                "ocr: nothing can be read off the screen — grant this application Screen Recording (Screen & System Audio Recording from macOS 15) in System Settings, Privacy & Security, then restart it",
            );
        } else {
            crate::logging::trace("macos", || {
                "ocr: Screen Recording is granted".to_string()
            });
        }
        allowed
    })
}

// ── What a recognition cost, in the log ────────────────────────────────────────────────────
//
// The wording and the arithmetic are `crate::ocr::cost`'s, tested on every platform; what is
// here is the state they are worked out from. The logs of September 2026 could not say whether
// the expensive first read was the first in the process, the first on its thread or the first
// after a pause, how many passes a field took, or whether two passes ran at once — so every
// pass is timed and journalled with its rung, the passes running in the process are counted, and
// the end of each pass is remembered for the process and for its thread.

/// The Vision passes running in the process now (`run_vision`).
static RUNNING: cost::Running = cost::Running::new();

/// When the last Vision pass in the process ended, a warm-up's included.
static LAST_PASS: Mutex<Option<Instant>> = Mutex::new(None);

/// Whether a recognition — a read, not a warm-up — has run in the process.
static RECOGNISED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// When this thread's last Vision pass ended, a warm-up's included.
    static THREAD_LAST_PASS: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Whether a recognition has run on this thread.
    static RECOGNISED_HERE: Cell<bool> = const { Cell::new(false) };
    /// The slowest recognition on this thread so far, for `note_cost`.
    static SLOWEST: Cell<Option<f64>> = const { Cell::new(None) };
    /// The passes of the recognition under way on this thread, while one is (`journal_open`).
    static JOURNAL: RefCell<Option<Vec<Pass>>> = const { RefCell::new(None) };
    /// When a slow read's line was last said on this thread, per region.
    static SLOW_SAID: RefCell<HashMap<(i32, i32, i32, i32), Instant>> = RefCell::new(HashMap::new());
}

/// The calling thread, as the cost lines name it.
fn this_thread() -> String {
    cost::thread_word(std::thread::current().name())
}

/// How long ago the last Vision pass ended, in the process and on this thread, and whether one of
/// another thread is running — now, before a read makes its own. The firsts are `note_cost`'s.
fn idle_now() -> cost::Before {
    let now = Instant::now();
    let since = |t: Option<Instant>| t.map(|t| now.saturating_duration_since(t));
    let process = *LAST_PASS.lock().unwrap_or_else(|e| e.into_inner());
    cost::Before {
        since_process: since(process),
        since_thread: since(THREAD_LAST_PASS.with(Cell::get)),
        busy_at_start: RUNNING.busy(),
        ..cost::Before::default()
    }
}

/// Starts writing down this thread's passes, forgetting any a read that ended early left.
fn journal_open() {
    JOURNAL.with(|j| *j.borrow_mut() = Some(Vec::new()));
}

/// The passes written down since `journal_open`, and the writing stopped.
fn journal_take() -> Vec<Pass> {
    JOURNAL.with(|j| j.borrow_mut().take()).unwrap_or_default()
}

/// Logs the cost of a recognition where a user can act on it: the first one in the process and
/// the first on each thread, each with how long Vision had been idle before it (`idle`, taken when
/// the read began) and whether another pass ran beside its own (`beside`), and after those a new
/// slowest on the thread that is clearly worse than the slowest before it (`cost::cost_line` has
/// the rule). Each says who answered the read and what it waited for the neural recogniser
/// (`answered`).
///
/// The first ones are called out because they are not representative: the first pass costs what
/// no later one does. Whether that is a cost per process, per thread or after every pause is what
/// the two firsts and the pause beside them can tell apart; the line used to call the first on
/// every thread "Vision's model load", and one on the recognise thread minutes after the warm-up
/// was read as a model load for it.
fn note_cost(ms: f64, w: i32, h: i32, idle: cost::Before, beside: bool, answered: Answered) {
    let before = cost::Before {
        first_in_process: !RECOGNISED.swap(true, Ordering::Relaxed),
        first_on_thread: !RECOGNISED_HERE.with(|c| c.replace(true)),
        ..idle
    };
    let slowest = SLOWEST.with(|s| {
        let before = s.get();
        if before.is_none_or(|p| ms > p) {
            s.set(Some(ms));
        }
        before
    });
    if let Some(line) = cost::cost_line(ms, w, h, this_thread, &before, slowest, beside, answered) {
        crate::logging::line("macos", &line);
    }
}

/// A slow read — `SLOW_JOB` or more, the threshold `host.ocr.read`'s own line has — part by part:
/// who answered it, its capture, each Vision pass with its rung, its time, its words and whether
/// another pass ran beside it, and its wait for the neural recogniser. At most one line per region
/// every `SLOW_LOG_EVERY` on each thread, the limit
/// that line has per module (this file knows no modules): a poll of three read-outs on a slow Mac
/// is then three lines every ten seconds, not three a second, and two fields of one size each have
/// their own. The time is from the start of this recognition: a `host.ocr.read`'s picture was
/// taken before it (`Capture::Apart`), so there the threshold is the recognition's alone.
fn note_slow(ms: f64, (x, y, w, h): (i32, i32, i32, i32), capture: Capture, passes: &[Pass], answered: Answered) {
    if ms < SLOW_JOB.as_secs_f64() * 1000.0 {
        return;
    }
    let due = SLOW_SAID.with(|s| cost::due(&mut s.borrow_mut(), (x, y, w, h), Instant::now(), SLOW_LOG_EVERY));
    if due {
        crate::logging::line("macos", &cost::slow_line((w, h), ms, &this_thread(), capture, passes, answered));
    }
}

/// A pass counted among those running (`RUNNING`) until it is left — or dropped, should the pass
/// unwind: a count left standing would have every later pass say it had company.
struct InVision(Option<cost::Entered>);

impl InVision {
    fn enter() -> InVision {
        InVision(Some(RUNNING.enter()))
    }

    /// Whether another pass ran beside this one.
    fn leave(mut self) -> bool {
        self.0.take().is_some_and(|e| RUNNING.leave(e))
    }
}

impl Drop for InVision {
    fn drop(&mut self) {
        if let Some(e) = self.0.take() {
            RUNNING.leave(e);
        }
    }
}

/// One recognition pass over an image Vision is given as-is.
///
/// `map` turns a Vision rectangle into the caller's coordinates; it is the only thing that
/// knows about the preprocessing, so this function stays honest about what it was handed.
/// `None` means the pass failed and has already said so; an empty string means it ran and
/// found nothing, which is an ordinary answer here.
const ACCURATE: VNRequestTextRecognitionLevel = VNRequestTextRecognitionLevel::Accurate;
const FAST: VNRequestTextRecognitionLevel = VNRequestTextRecognitionLevel::Fast;

/// The last two passes, tried only where the answer would otherwise be nothing at all.
///
/// A single small glyph is the case both system recognisers give up on — Windows'
/// outright, which is why that platform carries a second engine, and Vision's
/// conditionally. Two things are still untried at the point this runs, and both are free of
/// any new dependency:
///
/// **Bigger.** The ladder above already upscales, but only to the target; doubling it puts a
/// lone digit well past the target rather than near it, and widens the quiet space around
/// it, which some recognisers need in order to see a glyph as a glyph at all.
///
/// **Faster.** `Fast` is a different model — character-level rather than the accurate path's
/// language-aware one — and a value with no linguistic context is exactly where that trade
/// runs the right way. It goes LAST on purpose. A character model reading an unvalidated
/// number aloud to somebody who cannot check it is worse than saying nothing, so it is only
/// ever asked once everything better has already declined.
#[allow(clippy::too_many_arguments)]
fn bigger_then_faster(
    native: &CGImage,
    tight: &Plan,
    _rgba: &[u8],
    _px_w: usize,
    _px_h: usize,
    scale: f64,
    lang: Option<&str>,
    debug: bool,
    ladder: &Ladder,
    fast_last: bool,
) -> Option<VisionRead> {
    let big = Plan {
        x0: tight.x0,
        y0: tight.y0,
        cw: tight.cw,
        ch: tight.ch,
        up: (tight.up * 2).min(10),
        pad: OCR_PAD * 2,
        ink_h: tight.ink_h,
        ink_w: tight.ink_w,
        lines: tight.lines,
        bg: tight.bg,
        cropped: tight.cropped,
        // Reached only from a plan that was not blank, so this cannot be true here.
        blank: false,
    };
    let (img, buf) = render(native, &big)?;
    if debug {
        let (pw, ph) = big.out_size();
        debug_dump("ocr-debug-big.bmp", &buf, pw, ph);
    }
    // Both passes share one blit: rendering is the expensive part, and the only thing that
    // differs between them is which model reads it.
    let mut out = run_vision(&img, lang, ACCURATE, Stage::Enlarged, &|bb| map_box(&big, scale, bb));
    // The fast model only for a language it reads, and not while an interactive read waits
    // behind a background one — nor at the end of a ladder that asked it first and found the
    // neural recogniser reading otherwise (`fast_last` false, `ocr-bench`'s strategies only).
    if out.as_ref().is_none_or(|(t, _, _)| t.trim().is_empty())
        && ladder.fast_ok
        && fast_last
        && !ladder.preempted()
    {
        crate::logging::trace("macos", || {
            format!("ocr: {}x enlarged accurate pass read nothing, trying the fast model", big.up)
        });
        out = run_vision(&img, lang, FAST, Stage::Fast, &|bb| map_box(&big, scale, bb));
        if let Some((t, _, _)) = out.as_ref().filter(|(t, _, _)| !t.trim().is_empty()) {
            // Named, because it is the one answer in this file that did not come from the
            // recogniser we trust most, and a reader of the log should know which read it.
            crate::logging::line("macos", &format!("ocr: only the fast model read this: '{t}'"));
        }
    }
    drop(buf);
    out
}

/// One pass's answer: the text (lines joined with "\n"), every word in reading order, and the
/// lines themselves, which `host.ocr.read` groups into rows.
type VisionRead = (String, Vec<OcrWord>, Vec<OcrLine>);

/// A pass's answer made of its lines, as [`perform`] makes one: their texts joined with "\n", and
/// every line's words, in order. For an answer whose lines were changed after the pass
/// (`merge::respaced`).
fn vision_read_of(lines: Vec<OcrLine>) -> VisionRead {
    let text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    let words = lines.iter().flat_map(|l| l.words.iter().cloned()).collect();
    (text, words, lines)
}

/// One pass, made and timed the way the cost lines count it: the request made and configured,
/// the handler, the pass and its answer read out, as one time. `stage` is the rung of the ladder
/// it is, for the slow-read line. Every pass in the application comes through here.
fn run_vision(
    image: &CGImage,
    lang: Option<&str>,
    level: VNRequestTextRecognitionLevel,
    stage: Stage,
    map: &dyn Fn(CGRect) -> (i32, i32, i32, i32),
) -> Option<VisionRead> {
    crate::loop_guard::off_loop("a Vision pass");
    let inside = InVision::enter();
    // Whether the neural recogniser ran at some moment of this pass: running at either end, or a
    // run of it begun in between. Three atomic reads.
    let paddle_runs = paddle_ocr::runs_started();
    let paddle_then = paddle_ocr::running();
    let began = Instant::now();
    let request = new_request(lang, level);
    let out = match perform(image, &request, map) {
        Ok(read) => Some(read),
        Err(why) => {
            warn_once("ocr-perform", &format!("ocr: Vision refused the request — {why}"));
            None
        }
    };
    let ended = Instant::now();
    let beside = inside.leave();
    let paddle_beside = paddle_then || paddle_ocr::running() || paddle_ocr::runs_started() != paddle_runs;
    let pass = Pass {
        stage,
        ms: ended.saturating_duration_since(began).as_secs_f64() * 1000.0,
        words: out.as_ref().map(|(_, words, _)| words.len()),
        beside,
        paddle_beside,
    };
    *LAST_PASS.lock().unwrap_or_else(|e| e.into_inner()) = Some(ended);
    THREAD_LAST_PASS.with(|t| t.set(Some(ended)));
    JOURNAL.with(|j| {
        if let Some(passes) = j.borrow_mut().as_mut() {
            passes.push(pass);
        }
    });
    out
}

/// The request every pass makes, configured. Apart from `perform` so that `ocr-bench`
/// (`bench.rs`) can start from exactly this one and change a single thing on it.
fn new_request(
    lang: Option<&str>,
    level: VNRequestTextRecognitionLevel,
) -> Retained<VNRecognizeTextRequest> {
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(level);
    // Language correction is a dictionary pass over the result, and every string this
    // platform reads is a value: a note name, a cent offset, a lone digit. Correction is
    // what turns those into words that were never on the screen.
    request.setUsesLanguageCorrection(false);
    // Relative to the image height, default one thirty-second. The preprocessing already
    // makes the text a large fraction of the image, so this only matters on the pass-through
    // path — where it is the difference between reading a small label in a big window and
    // not seeing it at all.
    request.setMinimumTextHeight(0.0);
    if let Some(code) = lang {
        let langs = [NSString::from_str(code)];
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&langs));
    }
    // The request's revision is deliberately left alone. Naming revision 3 explicitly would
    // fail the request outright on macOS 12, where it does not exist. Apple documents the
    // default as the newest revision for the SDK the application was built with, not for the
    // running system; which one a request actually carries on this Mac is what the `[env]` line
    // `vision` says (`vision_report`).
    //
    // `setCustomWords` is likewise skipped: Vision only consults it during the
    // language-correction stage, which is switched off above, so it would be a no-op that
    // reads like a precaution.
    if let Some(r) = REVISION_OVERRIDE.with(std::cell::Cell::get) {
        // SAFETY: a property of a request made on this thread. Only `ocr-bench` sets the
        // override, and only to a revision this macOS lists and a probe process has run.
        unsafe { request.setRevision(r) };
    }
    request
}

thread_local! {
    // A revision every request made on this thread asks for: `ocr-bench`'s strategies that read
    // through the ladder under revision 2, all of it (`rev2`) or one rung (`with_revision`).
    // Nothing in the application sets it, so every request it makes keeps the default revision,
    // as above. (The bench counts a read's passes by the journal, `Note::passes`.)
    static REVISION_OVERRIDE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// One pass of `request` over `image`, and its answer; `Err` carries Vision's reason for
/// refusing it, which `run_vision` logs.
fn perform(
    image: &CGImage,
    request: &VNRecognizeTextRequest,
    map: &dyn Fn(CGRect) -> (i32, i32, i32, i32),
) -> Result<VisionRead, String> {
    let options: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::new();
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            image,
            &options,
        )
    };
    // Synchronous: it returns when the requests have finished. No completion handler, no
    // queue, no run loop — which is what makes it usable from the pump thread at all.
    let base: &VNRequest = request;
    let requests: Retained<NSArray<VNRequest>> = NSArray::from_slice(&[base]);
    if let Err(e) = handler.performRequests_error(&requests) {
        return Err(e.localizedDescription().to_string());
    }

    // No results at all is the ordinary "there was no text here" answer, not a failure.
    let mut lines: Vec<Line> = Vec::new();
    for observation in request.results().into_iter().flatten() {
        let candidates = observation.topCandidates(1);
        let Some(top) = candidates.firstObject() else {
            continue;
        };
        let line = top.string().to_string();
        if line.is_empty() {
            continue;
        }
        // SAFETY: reading a property off an observation Vision just produced.
        let line_box = unsafe { observation.boundingBox() };
        let (lx, ly, _, lh) = map(line_box);

        let mut words = Vec::new();
        for (start, len, text) in split_words(&line) {
            let range = NSRange {
                location: start,
                length: len,
            };
            // SAFETY: the range indexes the very string this candidate returned.
            let word_box = match unsafe { top.boundingBoxForRange_error(range) } {
                Ok(rect) => unsafe { rect.boundingBox() },
                Err(_) => {
                    // Documented as approximate and for UI purposes; when it declines
                    // entirely, the line's own box at least puts the word on the right line
                    // rather than at the origin.
                    crate::logging::trace("macos", || {
                        format!("ocr: no box for '{text}' within '{line}', using the line's")
                    });
                    line_box
                }
            };
            let (wx, wy, ww, wh) = map(word_box);
            words.push(OcrWord {
                text,
                x: wx,
                y: wy,
                w: ww,
                h: wh,
            });
        }
        lines.push(Line {
            x: lx,
            y: ly,
            h: lh,
            text: line,
            words,
        });
    }

    let ordered = reading_order(lines);
    let text = ordered
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let out_lines =
        ordered.iter().map(|l| OcrLine { text: l.text.clone(), words: l.words.clone() }).collect();
    let words = ordered.into_iter().flat_map(|l| l.words).collect();
    Ok((text, words, out_lines))
}

/// One observation: a run of text Vision read, and where it put it.
struct Line {
    x: i32,
    y: i32,
    h: i32,
    text: String,
    words: Vec<OcrWord>,
}

/// Puts the observations into reading order.
///
/// Vision promises no order at all, and the host hands the joined text straight to a module
/// that compares it against what it read last tick — an order that wobbles between ticks
/// reads as the value having changed.
///
/// Sorting by y and then x is not enough, because Vision returns one observation per *run*
/// of text: a row with a gap in it, which is exactly how two read-outs sit side by side in
/// Melodyne's tool strip, arrives as two observations whose tops differ by a pixel or two.
/// Whichever happened to sit higher would come first. So runs are gathered into rows first,
/// by a tolerance of half a line height, and only then ordered left to right within the row.
fn reading_order(mut lines: Vec<Line>) -> Vec<Line> {
    lines.sort_by_key(|l| (l.y, l.x));
    let mut rows = Vec::with_capacity(lines.len());
    let mut row = 0i32;
    let mut top: Option<(i32, i32)> = None;
    for line in &lines {
        match top {
            Some((row_y, row_h)) if line.y - row_y <= row_h.max(line.h) / 2 => {}
            _ => {
                row += 1;
                top = Some((line.y, line.h));
            }
        }
        rows.push(row);
    }
    let mut ranked: Vec<(i32, i32, Line)> = lines
        .into_iter()
        .zip(rows)
        .map(|(line, row)| (row, line.x, line))
        .collect();
    ranked.sort_by_key(|(row, x, _)| (*row, *x));
    ranked.into_iter().map(|(_, _, line)| line).collect()
}

/// Splits a recognised line into words, with each word's offset and length in UTF-16 code
/// units.
///
/// `boundingBoxForRange:` indexes an `NSString`, and an `NSRange` counts UTF-16 code units —
/// not bytes and not characters. For "+36 Ct" all three agree, which is exactly why getting
/// it wrong here would survive every test anyone thought to run.
fn split_words(line: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut start = 0usize;
    let mut current = String::new();
    for ch in line.chars() {
        let units = ch.len_utf16();
        if ch.is_whitespace() {
            if !current.is_empty() {
                out.push((start, offset - start, std::mem::take(&mut current)));
            }
            offset += units;
            start = offset;
        } else {
            if current.is_empty() {
                start = offset;
            }
            current.push(ch);
            offset += units;
        }
    }
    if !current.is_empty() {
        out.push((start, offset - start, current));
    }
    out
}

/// What was done to the capture before Vision saw it, and therefore what has to be undone to
/// its answers.
struct Plan {
    /// Content-crop origin within the capture, in capture pixels.
    x0: usize,
    y0: usize,
    /// Crop size in capture pixels.
    cw: usize,
    ch: usize,
    /// Integer upscale applied to the crop.
    up: usize,
    /// Background border around the upscaled crop, in processed pixels.
    pad: usize,
    /// How tall the ink was, before any margin. Carried so a later rung can upscale for the
    /// glyph rather than for the rectangle it was found in.
    ink_h: usize,
    /// How wide the ink was, before any margin: with `ink_h` and `lines`, whether the region is
    /// the neural recogniser's to read (`paddle_pre::fits`).
    ink_w: usize,
    /// The lines of ink in the crop: runs of pixel rows with ink in them, apart by at least the
    /// margin (`paddle_pre::ink_lines`). 0 where nothing was measured — a capture handed on as
    /// it is, a cut, a flat region.
    lines: usize,
    /// Fill colour for that border, 0..1 per channel.
    bg: [f64; 3],
    /// Whether the crop actually narrowed anything — the retry hinges on this.
    cropped: bool,
    /// Nothing here to read: the region is one flat colour, or a panel with nothing on it.
    ///
    /// Not the same as "the recogniser found nothing", and the difference is the whole point.
    /// A recogniser asked about a blank rectangle does not answer "nothing" — it answers
    /// whatever its network makes of noise, and the caller has no way to tell that from a
    /// reading. So the question is not asked.
    blank: bool,
}

impl Plan {
    /// The capture untouched: what a large region gets, and the case where every mapping
    /// below reduces to dividing by the backing scale.
    fn identity(nw: usize, nh: usize) -> Plan {
        Plan {
            x0: 0,
            y0: 0,
            cw: nw,
            ch: nh,
            up: 1,
            pad: 0,
            ink_h: nh,
            ink_w: nw,
            lines: 0,
            bg: [0.0; 3],
            cropped: false,
            blank: false,
        }
    }

    /// The whole capture, upscaled and framed but not cropped.
    fn whole(rgba: &[u8], nw: usize, nh: usize) -> Plan {
        Plan::whole_with_ink(rgba, nw, nh, nh)
    }

    /// The whole region, but upscaled for ink of a known height rather than for the region's.
    ///
    /// The retry rung reaches here having already had a tightened plan in hand, so it knows
    /// how tall the ink actually was — and handing that over is the difference between a
    /// second attempt and a weaker one. Measured on the two fields this was chased through:
    /// a 22-point region got `64/22 = 2` where its 20-point neighbour got 3, so the safety
    /// net was thinnest exactly where it was needed. `nh` remains the answer for the only
    /// other caller, the flat-colour fallback, where no ink was found to measure.
    fn whole_with_ink(rgba: &[u8], nw: usize, nh: usize, ink_h: usize) -> Plan {
        Plan {
            x0: 0,
            y0: 0,
            cw: nw,
            ch: nh,
            up: upscale_for(ink_h),
            pad: OCR_PAD,
            ink_h,
            ink_w: nw,
            lines: 0,
            bg: corner_background(rgba, nw, nh),
            cropped: false,
            blank: false,
        }
    }

    /// Cropped to the ink and upscaled so the ink is large.
    ///
    /// Upscaling the whole padded region instead leaves a tiny digit tiny, because the
    /// factor is then decided by the region's height rather than the glyph's — which is the
    /// single change that made small read-outs legible on the Windows side.
    fn content(rgba: &[u8], nw: usize, nh: usize, margin: usize) -> Plan {
        let bg = corner_background(rgba, nw, nh);
        if nw < 3 || nh < 3 {
            return Plan::whole(rgba, nw, nh);
        }
        // Distance from the background colour, in the same 0..255 space and against the same
        // threshold the Windows path uses, so the two platforms crop the same pixels. Squared
        // on both sides: this walks every pixel of the capture on the thread that owns the
        // keyboard tap, and a square root per pixel buys nothing an inequality needs.
        let threshold_sq = 55.0f64 * 55.0;
        let (bgr, bgg, bgb) = (bg[0] * 255.0, bg[1] * 255.0, bg[2] * 255.0);
        let (mut x0, mut y0, mut x1, mut y1) = (nw, nh, 0usize, 0usize);
        let mut found = false;
        // Which rows hold ink, for the lines of it (`lines`).
        let mut rows = vec![false; rgba.len() / (nw * 4)];
        // Row by row through the slice rather than by index, so the bounds check happens once
        // per row instead of three times per pixel.
        for ((y, row), row_ink) in rgba.chunks_exact(nw * 4).enumerate().zip(rows.iter_mut()) {
            for (x, px) in row.chunks_exact(4).enumerate() {
                let dr = px[0] as f64 - bgr;
                let dg = px[1] as f64 - bgg;
                let db = px[2] as f64 - bgb;
                if dr * dr + dg * dg + db * db > threshold_sq {
                    found = true;
                    *row_ink = true;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if !found {
            // A region that is one flat colour is either genuinely blank — a read-out with
            // nothing in it — or a capture that saw nothing. The second case is exactly what
            // a missing Screen Recording grant looks like from in here, and it is worth one
            // question to the system to tell them apart.
            crate::logging::trace("macos", || {
                format!("ocr: {nw}x{nh} px capture is one flat colour, nothing to crop to")
            });
            screen_capture_permitted();
            let mut w = Plan::whole(rgba, nw, nh);
            w.blank = true;
            return w;
        }

        // A well, not a word.
        //
        // The regions these overlays author are slices of a plugin's chrome, and a value is
        // very often painted inside a sunken box of its own — sforzando draws both of the
        // fields this was chased through as pure black wells set into grey. The crop above
        // then finds the WELL, because the well is what differs from the region's corners,
        // and everything downstream treats a 15-row box as though it were a 15-row glyph:
        // the upscale comes out far too small, and the recogniser is handed a black
        // rectangle floating in a grey field instead of a digit.
        //
        // So if what was found is itself one flat colour at its own corners, and that colour
        // is far from the outer background, measure again inside it against that colour. Only
        // a strictly shorter result is taken, and the background becomes the well's own, so
        // the padding frames the glyph in the colour it was drawn on rather than in chrome
        // from outside the box. A region that really is just text is untouched: its corners
        // will not agree, and nothing changes.
        let mut bg = bg;
        let mut lines = paddle_pre::ink_lines(&rows[y0..=y1], margin);
        let panel = ink_inside_panel(rgba, nw, x0, y0, x1, y1, bg);
        if matches!(panel, Panel::Empty) {
            // A value field with no value in it. The crop found the WELL, so `cropped` would
            // be true and the retry ladder would open — ending at the character model, over a
            // box that has nothing in it. That rung has no linguistic validation and no way to
            // answer "nothing", so what it returns is whatever it makes of an empty rectangle.
            crate::logging::trace("macos", || {
                format!(
                    "ocr: the crop is a {}x{} panel with nothing on it — not recognising it",
                    x1 - x0 + 1,
                    y1 - y0 + 1
                )
            });
            let mut w = Plan::whole(rgba, nw, nh);
            w.blank = true;
            return w;
        }
        if let Panel::Ink(ix0, iy0, ix1, iy1, inner_bg, inner_rows) = panel {
            if iy1 - iy0 < y1 - y0 {
                crate::logging::trace("macos", || {
                    format!(
                        "ocr: the crop was a filled panel {}x{}; the ink inside it is {}x{}",
                        x1 - x0 + 1,
                        y1 - y0 + 1,
                        ix1 - ix0 + 1,
                        iy1 - iy0 + 1
                    )
                });
                bg = inner_bg;
                lines = paddle_pre::ink_lines(&inner_rows[iy0 - y0..=iy1 - y0], margin);
                x0 = ix0;
                y0 = iy0;
                x1 = ix1;
                y1 = iy1;
            }
        }

        // The height of the INK, before the margin is added to it. This is what the upscale
        // has to be computed from, and computing it from the padded crop instead was a real
        // defect with a measurable cost: for sforzando's polyphony field on a non-Retina
        // Mac — a 40x20 px capture whose digit is about 11 px tall — the margin makes the
        // crop 17 px, so the factor came out as 64/17 = 3 and the glyph reached Vision at
        // roughly 33 px. This file's own header promises about 64 (`TARGET_CONTENT_PX`), so
        // every lone digit was being handed over at half the height the preprocessing aims for.
        // From the ink it is 64/11 = 5, and the glyph arrives at 55.
        let ink_h = y1 - y0 + 1;
        let ink_w = x1 - x0 + 1;
        let x0 = x0.saturating_sub(margin);
        let y0 = y0.saturating_sub(margin);
        let x1 = (x1 + margin).min(nw - 1);
        let y1 = (y1 + margin).min(nh - 1);
        let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
        Plan {
            x0,
            y0,
            cw,
            ch,
            up: upscale_for(ink_h),
            pad: OCR_PAD,
            ink_h,
            ink_w,
            lines,
            bg,
            cropped: cw < nw || ch < nh,
            blank: false,
        }
    }

    /// Size of the image Vision is handed, in pixels.
    fn out_size(&self) -> (usize, usize) {
        (self.cw * self.up + self.pad * 2, self.ch * self.up + self.pad * 2)
    }
}

/// Is this crop a filled panel with something drawn on it, and if so where is that something?
///
/// Answers by asking the crop's own four corners. If they agree with each other and differ
/// from the colour the region as a whole was measured against, the crop is a box rather than
/// a word — and the interesting content is whatever inside it differs from the box.
///
/// Three answers, and the third is the one that used to be lost. `NotAPanel` is a real glyph
/// cluster, whose corners are background on some sides and ink on others. `Ink` is a box with
/// something drawn on it. `Empty` is a box with NOTHING drawn on it — a value field with no
/// value in it — which used to be reported as `None` alongside the first, and so escalated
/// through the whole retry ladder instead of stopping.
#[allow(clippy::too_many_arguments)]
fn ink_inside_panel(
    rgba: &[u8],
    nw: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    outer_bg: [f64; 3],
) -> Panel {
    if x1 <= x0 + 2 || y1 <= y0 + 2 {
        return Panel::NotAPanel;
    }
    let at = |x: usize, y: usize| -> [f64; 3] {
        let i = (y * nw + x) * 4;
        [rgba[i] as f64, rgba[i + 1] as f64, rgba[i + 2] as f64]
    };
    let corners = [at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)];
    let far = |a: [f64; 3], b: [f64; 3]| {
        let (dr, dg, db) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
        dr * dr + dg * dg + db * db
    };
    // The corners have to agree closely — a panel is one colour — and the panel has to be
    // clearly distinct from what surrounds it, or there is no panel, only noise.
    const SAME_SQ: f64 = 12.0 * 12.0;
    const DIFFERENT_SQ: f64 = 55.0 * 55.0;
    for c in &corners[1..] {
        if far(corners[0], *c) > SAME_SQ {
            return Panel::NotAPanel;
        }
    }
    let outer = [outer_bg[0] * 255.0, outer_bg[1] * 255.0, outer_bg[2] * 255.0];
    if far(corners[0], outer) <= DIFFERENT_SQ {
        return Panel::NotAPanel;
    }

    let panel = corners[0];
    let (mut ix0, mut iy0, mut ix1, mut iy1) = (x1, y1, x0, y0);
    let mut found = false;
    let mut rows = vec![false; y1 - y0 + 1];
    for y in y0..=y1 {
        for x in x0..=x1 {
            if far(at(x, y), panel) > DIFFERENT_SQ {
                found = true;
                rows[y - y0] = true;
                ix0 = ix0.min(x);
                iy0 = iy0.min(y);
                ix1 = ix1.max(x);
                iy1 = iy1.max(y);
            }
        }
    }
    if !found {
        // A panel, and nothing on it.
        return Panel::Empty;
    }
    Panel::Ink(ix0, iy0, ix1, iy1, [panel[0] / 255.0, panel[1] / 255.0, panel[2] / 255.0], rows)
}

/// What the crop turned out to be. See `ink_inside_panel`.
enum Panel {
    NotAPanel,
    /// A box with nothing drawn on it — a value field with no value in it.
    Empty,
    /// A box, and the bounds of what is drawn on it, plus the box's own colour, and which of the
    /// box's rows (from its top) hold any of it.
    Ink(usize, usize, usize, usize, [f64; 3], Vec<bool>),
}

/// The integer upscale that brings content of this height to roughly the target.
///
/// Integer division gives 1 as soon as the content is already tall enough, which is the
/// right answer: the upscale exists to rescue small glyphs, and doubling text that Vision
/// can read as it stands only costs time. The ceiling stops a one-pixel-tall artefact from
/// asking for a sixty-fold blit.
///
/// **Pass it the height of the ink, not of the crop.** The crop carries a margin on both
/// sides, and giving that away turns a factor of five into a factor of three — which for a
/// single small glyph is the difference between near the 64 px aimed for and about half of it.
fn upscale_for(content_px: usize) -> usize {
    upscale_toward(TARGET_CONTENT_PX, content_px)
}

/// [`upscale_for`] toward another target than [`TARGET_CONTENT_PX`]: the one rule, with the
/// target a parameter for `ocr-bench`'s `target-48` variant.
fn upscale_toward(target_px: usize, content_px: usize) -> usize {
    (target_px / content_px.max(1)).clamp(1, 10)
}

/// Average of the four corners, 0..1 per channel — the region's background.
///
/// The regions in question are fixed slices of a plugin's chrome with padding around the
/// text, so the corners are background by construction. Taking all four rather than one
/// survives a gradient.
fn corner_background(rgba: &[u8], nw: usize, nh: usize) -> [f64; 3] {
    if nw == 0 || nh == 0 || rgba.len() < nw * nh * 4 {
        return [0.0; 3];
    }
    let at = |x: usize, y: usize| {
        let p = (y * nw + x) * 4;
        [rgba[p] as f64, rgba[p + 1] as f64, rgba[p + 2] as f64]
    };
    let corners = [at(0, 0), at(nw - 1, 0), at(0, nh - 1), at(nw - 1, nh - 1)];
    let mut out = [0.0f64; 3];
    for c in 0..3 {
        out[c] = corners.iter().map(|p| p[c]).sum::<f64>() / 4.0 / 255.0;
    }
    out
}

/// Turns one of Vision's normalised rectangles into points relative to the requested region.
///
/// Three coordinate systems meet here and each one is a chance to be silently wrong.
/// Vision's box is normalised to the *processed* image, and its origin is the box's
/// **bottom** edge with y growing **upward** — so the flip is `1 - origin.y - height`, not
/// `1 - origin.y`. That lands in processed pixels; undoing the border and the upscale and
/// adding the crop origin lands in capture pixels; dividing by the backing scale lands in
/// points, which is the only space allowed to cross the trait boundary.
fn map_box(plan: &Plan, scale: f64, bb: CGRect) -> (i32, i32, i32, i32) {
    let (pw, ph) = plan.out_size();
    let (pw, ph) = (pw as f64, ph as f64);
    let px = bb.origin.x * pw;
    let py = (1.0 - bb.origin.y - bb.size.height) * ph;
    let pwidth = bb.size.width * pw;
    let pheight = bb.size.height * ph;

    let up = plan.up as f64;
    let pad = plan.pad as f64;
    let cx = (px - pad) / up + plan.x0 as f64;
    let cy = (py - pad) / up + plan.y0 as f64;

    (
        (cx / scale).round() as i32,
        (cy / scale).round() as i32,
        // A word narrower than a point still has to have a width, or a caller measuring it
        // divides by zero.
        (pwidth / up / scale).round().max(1.0) as i32,
        (pheight / up / scale).round().max(1.0) as i32,
    )
}

/// Renders the plan: crops, upscales and frames the capture into a fresh image for Vision.
///
/// The crop is done by drawing the whole capture offset and letting the context clip, rather
/// than by `CGImageCreateWithImageInRect`, whose rectangle is documented in the image's own
/// coordinate space — a convention this port has no way to test. Clipping is unambiguous.
///
/// Returns the image together with the buffer behind it: the image created from a bitmap
/// context shares that memory copy-on-write, so the caller has to keep it alive.
fn render(source: &CGImage, plan: &Plan) -> Option<(CFRetained<CGImage>, Vec<u8>)> {
    let (pw, ph) = plan.out_size();
    if pw == 0 || ph == 0 {
        return None;
    }
    let sw = CGImage::width(Some(source)) as f64;
    let sh = CGImage::height(Some(source)) as f64;
    let bytes_per_row = pw * 4;
    let mut buf = vec![0u8; bytes_per_row * ph];
    let space = CGColorSpace::new_device_rgb()?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: the buffer is sized to exactly the geometry described here, and it outlives the
    // context — the context is released below, the buffer is handed to the caller.
    let ctx = unsafe {
        CGBitmapContextCreate(
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            pw,
            ph,
            8,
            bytes_per_row,
            Some(&space),
            info,
        )
    }?;

    CGContext::set_rgb_fill_color(Some(&ctx), plan.bg[0], plan.bg[1], plan.bg[2], 1.0);
    CGContext::fill_rect(
        Some(&ctx),
        CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(pw as CGFloat, ph as CGFloat),
        ),
    );
    CGContext::set_interpolation_quality(Some(&ctx), CGInterpolationQuality::High);

    // A bitmap context has its origin at the BOTTOM left while its memory starts at the top
    // row, so placing the crop at (pad, pad) in the picture means computing where the
    // image's bottom edge has to sit for its top edge to land there. Written out: memory row
    // r is at context y = ph - r; the crop's top row must land at r = pad; the source's own
    // top row is y0 crop-rows above that.
    let up = plan.up as f64;
    let pad = plan.pad as f64;
    let dest = CGRect::new(
        CGPoint::new(
            pad - plan.x0 as f64 * up,
            ph as f64 - pad + plan.y0 as f64 * up - sh * up,
        ),
        CGSize::new(sw * up, sh * up),
    );
    CGContext::draw_image(Some(&ctx), dest, Some(source));
    CGContext::flush(Some(&ctx));

    let image = CGBitmapContextCreateImage(Some(&ctx))?;
    drop(ctx);
    Some((image, buf))
}

/// The capture as tightly packed, top-down RGBA.
///
/// Rendering into a context we describe ourselves rather than reading the capture's own
/// bytes: a screen `CGImage` on Apple silicon comes back premultiplied-first in
/// little-endian order — BGRA in memory — with a row stride that need not be the width, and
/// deriving that permutation at runtime is a guess this port cannot check. Here the layout
/// is stated rather than discovered, and one extra blit of a region this size costs nothing
/// next to the capture itself.
fn cgimage_to_rgba(image: &CGImage) -> Option<(Vec<u8>, usize, usize)> {
    let w = CGImage::width(Some(image));
    let h = CGImage::height(Some(image));
    if w == 0 || h == 0 {
        return None;
    }
    let bytes_per_row = w * 4;
    let mut buf = vec![0u8; bytes_per_row * h];
    let space = CGColorSpace::new_device_rgb()?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: the buffer outlives the context, which is dropped before the buffer is read.
    let ctx = unsafe {
        CGBitmapContextCreate(
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            w,
            h,
            8,
            bytes_per_row,
            Some(&space),
            info,
        )
    }?;
    CGContext::draw_image(
        Some(&ctx),
        CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(w as CGFloat, h as CGFloat),
        ),
        Some(image),
    );
    CGContext::flush(Some(&ctx));
    drop(ctx);
    Some((buf, w, h))
}

/// Says a thing to the log the first time, and only to the trace thereafter.
///
/// Everything that can fail here is polled: a missing permission would otherwise write the
/// same line sixteen times a second and bury the rest of the session.
fn warn_once(key: &'static str, msg: &str) {
    thread_local! {
        static SAID: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
    }
    if SAID.with(|s| s.borrow_mut().insert(key)) {
        crate::logging::line("macos", msg);
    } else {
        crate::logging::trace("macos", || msg.to_string());
    }
}

/// Writes what Vision was given beside the application, where its log is, while the switch "Save
/// the images OCR was given" is on (`appcfg::ocr_debug`, Application settings; the old
/// `AUTOMATION_PLATFORM_OCR_DEBUG` still forces it) — the same switch as on Windows. The
/// processed pictures keep one name each and are overwritten by the next read; a small region's
/// raw capture and the shadow's pictures are named per region and content (`keep_picture`).
///
/// The one question a log cannot answer is "what did the recogniser actually see", and it is
/// the question that matters when a region reads as empty on a machine none of us has. BMP
/// rather than PNG because it needs no encoder: the whole point is that this file must not
/// drag a dependency into the check crate to earn its place.
fn debug_dump(name: &str, rgba: &[u8], w: usize, h: usize) {
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return;
    }
    let stride = w * 4;
    let image_size = (stride * h) as u32;
    let mut out = Vec::with_capacity(54 + image_size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(54 + image_size).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes()); // BITMAPINFOHEADER
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes()); // positive: rows bottom-up
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&image_size.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for y in (0..h).rev() {
        for x in 0..w {
            let p = y * stride + x * 4;
            out.extend_from_slice(&[rgba[p + 2], rgba[p + 1], rgba[p], 255]);
        }
    }
    let path = crate::portable::base_dir().join(name);
    match std::fs::write(&path, out) {
        Ok(()) => crate::logging::line("macos", &format!("ocr: wrote {}", path.display())),
        Err(e) => crate::logging::line(
            "macos",
            &format!("ocr: could not write {}: {e}", path.display()),
        ),
    }
}
