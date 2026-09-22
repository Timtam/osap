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
//! happened when a change made the primary non-empty and the fallback stopped firing. That
//! second engine is a Windows-only dependency and does not exist here. Vision therefore has
//! to carry the small-text case alone, and everything it is given is chosen for that: the
//! capture is taken at full backing resolution rather than the point-sized one the rest of
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

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImage,
    CGImageAlphaInfo, CGImageByteOrderInfo, CGInterpolationQuality, CGPreflightScreenCaptureAccess,
};
use objc2_foundation::{NSArray, NSDictionary, NSLocale, NSRange, NSString};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};

use crate::backend::{CaptureSource, OcrLine, OcrText, OcrThread, OcrWord, Recognise};
use crate::ocr::plan::{self, Plan as CapturePlan};
use crate::ocr::types::Rect;

/// Which of the ladder's last rungs one recognition may climb.
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
}

impl Ladder<'static> {
    const FULL: Ladder<'static> = Ladder { fast_ok: true, preempt: None, on_event_loop: true };
}

impl Ladder<'_> {
    fn preempted(&self) -> bool {
        self.preempt.is_some_and(|p| p.load(Ordering::Relaxed))
    }
}

/// Background border added around the upscaled content, in pixels of the processed image.
///
/// Same figure as the Windows path. A recogniser expects text to sit in a page, not to run
/// off the edge of one, and a few characters cropped hard to their own ink read badly.
const OCR_PAD: usize = 24;

/// How tall the content is aimed at in the processed image.
///
/// Apple's own guidance for the accurate recognition level puts the comfortable floor at
/// around 32 px of text height; 64 is the figure the Windows path settled on and it leaves
/// room for the crop being a little loose.
const TARGET_CONTENT_PX: usize = 64;

/// How long the whole ladder may take before the last rungs are abandoned.
///
/// The ladder exists for the read that nearly works; the cost falls entirely on the read
/// that never will. Every rung is a Vision pass, they run on the thread that also carries
/// the keyboard event tap, and macOS switches a tap off when its thread stops answering —
/// which is exactly how the tester lost a Shift+Tab into a plugin that then moved its own
/// focus. Better a field that says nothing promptly than a keystroke that goes missing.
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
    // A zero-sized region is not a failure: `read_region` clamps a reversed rectangle to
    // zero (`lib.rs`, `(x2 - x1).max(0)`), so a module with its geometry momentarily wrong
    // gets here rather than being stopped earlier.
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
    recognize_captured(&native, scale, x, y, w, h, lang, started, debug, &Ladder::FULL)
}

/// Everything after the capture, so that a caller who already has the pixels — one bounding-box
/// capture cut into several regions — runs exactly the same pipeline as a single read.
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
    debug: bool,
    ladder: &Ladder,
) -> Result<OcrText, String> {
    let nw = CGImage::width(Some(native));
    let nh = CGImage::height(Some(&native));

    let small = w <= SMALL_W && h <= SMALL_H;
    let result = if !small {
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
        run_vision(native, lang, ACCURATE, &|bb| map_box(&plan, scale, bb))
    } else {
        let Some((rgba, px_w, px_h)) = cgimage_to_rgba(native) else {
            warn_once(
                "ocr-convert",
                "ocr: could not read the captured pixels back out of CoreGraphics",
            );
            return Ok(empty());
        };
        if debug {
            debug_dump("ocr-debug-raw.bmp", &rgba, px_w, px_h);
        }
        // The margin is a physical distance, not a pixel count: three pixels of slack round
        // the ink at 1x is one and a half at 2x, and the crop would start clipping antialias
        // fringes off Retina glyphs.
        let margin = (3.0 * scale).round().max(1.0) as usize;
        let plan = Plan::content(&rgba, px_w, px_h, margin);

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
            return Ok(OcrText { skipped: true, ..empty() });
        }

        crate::logging::trace("macos", || {
            let (pw, ph) = plan.out_size();
            format!(
                "ocr: cropped {}x{} px at {},{} of {px_w}x{px_h}, upscaled {}x, framed to {pw}x{ph}",
                plan.cw, plan.ch, plan.x0, plan.y0, plan.up
            )
        });
        let out = render(native, &plan).and_then(|(img, buf)| {
            if debug {
                let (pw, ph) = plan.out_size();
                debug_dump("ocr-debug.bmp", &buf, pw, ph);
            }
            let r = run_vision(&img, lang, ACCURATE, &|bb| map_box(&plan, scale, bb));
            // `buf` is the bitmap context's backing store and the image created from it is
            // a copy-on-write of that memory. Dropping it before Vision has read the image
            // would be a use-after-free that only shows up on the machine nobody here owns.
            drop(buf);
            r
        });

        match out {
            // Only when the tightened pass found nothing, and only when it actually cropped
            // — if it already fell back to the whole region there is no second input to try
            // and the retry would just pay for the same answer twice.
            Some((ref t, _, _)) if t.trim().is_empty() && plan.cropped => {
                crate::logging::trace("macos", || {
                    "ocr: tightened pass read nothing, trying the whole region".to_string()
                });
                let whole = Plan::whole_with_ink(&rgba, px_w, px_h, plan.ink_h);
                let second = render(native, &whole).and_then(|(img, buf)| {
                    if debug {
                        let (pw, ph) = whole.out_size();
                        debug_dump("ocr-debug-retry.bmp", &buf, pw, ph);
                    }
                    let r = run_vision(&img, lang, ACCURATE, &|bb| map_box(&whole, scale, bb));
                    drop(buf);
                    r
                });
                let exhausted = second.as_ref().is_none_or(|(t, _, _)| t.trim().is_empty());
                if exhausted && started.elapsed() < LADDER_BUDGET && !ladder.preempted() {
                    bigger_then_faster(native, &plan, &rgba, px_w, px_h, scale, lang, debug, ladder)
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
    };

    let (text, words, lines) = result.unwrap_or_else(|| (String::new(), Vec::new(), Vec::new()));
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    crate::logging::trace("macos", || {
        format!(
            "ocr {w}x{h} pt at {x},{y} -> {nw}x{nh} px (scale {scale:.2}), {:.1} ms, {} word(s): '{}'",
            ms,
            words.len(),
            text.replace('\n', " ")
        )
    });
    note_cost(ms, w, h);
    Ok(OcrText { text, words, lines, fallback: None, skipped: false })
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
    let x0 = regions.iter().map(|r| r.0).min().unwrap_or(0);
    let y0 = regions.iter().map(|r| r.1).min().unwrap_or(0);
    let x1 = regions.iter().map(|r| r.0 + r.2).max().unwrap_or(0);
    let y1 = regions.iter().map(|r| r.1 + r.3).max().unwrap_or(0);
    let (bw, bh) = (x1 - x0, y1 - y0);

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
                    bg: [0.0; 3],
                    cropped: false,
                    blank: false,
                };
                match render(&big, &cut) {
                    Some((img, buf)) => {
                        let r = recognize_captured(
                            &img, scale, *x, *y, *w, *h, lang, started, debug, &Ladder::FULL,
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
        // SAFETY: a plain call about the calling thread; the answer is only logged.
        let rc = unsafe {
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INITIATED, 0)
        };
        if rc != 0 {
            crate::logging::line(
                "macos",
                &format!("ocr: the recognise thread kept its quality of service ({rc})"),
            );
        }
    }
}

/// The capture stage of `host.ocr.read`. The source is ignored here, as everywhere on this
/// platform.
pub fn capture_for_read(regions: &[(i32, i32, i32, i32)], _src: CaptureSource) -> (Shot, usize) {
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
    let ladder = Ladder { fast_ok: ctx.fast_ok, preempt: ctx.preempt, on_event_loop: false };
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
                        bg: [0.0; 3],
                        cropped: false,
                        blank: false,
                    };
                    match render(big, &cut) {
                        Some((img, buf)) => {
                            let r = recognize_captured(
                                &img, *scale, x, y, w, h, ctx.lang, started, debug, &ladder,
                            );
                            // `buf` backs the image copy-on-write; see `recognize_regions`.
                            drop(buf);
                            r
                        }
                        None => Err("could not cut this region out of the shared capture".to_string()),
                    }
                }
                Shot::Each(each) => match each.get(i).and_then(|c| c.as_ref()) {
                    Some((native, scale)) => recognize_captured(
                        native, *scale, x, y, w, h, ctx.lang, started, debug, &ladder,
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

/// Makes Vision load its recognition model now, on a thread of its own, so that the first
/// real recognition does not.
///
/// The model load is a one-off of anything from half a second to two seconds, and `ocr` runs
/// synchronously on the pump thread — which is also the thread carrying speech, timers and
/// the overlay's own polling. Paying it there means a frozen interface and a late
/// announcement at exactly the moment a user first asked to read something. `docs/macos-
/// port.md` names this as one of the three failures the port is shaped to avoid.
///
/// Nothing crosses the thread boundary: every Objective-C object is made on the thread that
/// uses it, because none of them are `Send`, and the result is thrown away. Vision's request
/// handler is documented as usable from any thread and needs no run loop, which is what makes
/// this legal at all.
#[allow(dead_code)] // Called from `MacBackend::new`; see this file's entry in docs/macos-port.md.
pub fn warm_up() {
    // A thread that is not the main one has no autorelease pool of its own and no run loop to
    // drain one, so everything Vision autoreleases here would simply stay.
    std::thread::spawn(|| objc2::rc::autoreleasepool(|_| warm_up_in_pool()));
}

fn warm_up_in_pool() {
    let started = Instant::now();
    let Some((image, buf)) = synthetic_page() else {
        crate::logging::line(
            "macos",
            "ocr: could not build a warm-up image; Vision will load its model on the first real recognition instead, which will be slow",
        );
        return;
    };
    let read = run_vision(&image, None, ACCURATE, &|_| (0, 0, 1, 1));
    drop(buf);
    crate::logging::line(
        "macos",
        &format!(
            "ocr: Vision warmed up in {:.0} ms{}",
            started.elapsed().as_secs_f64() * 1000.0,
            match read {
                Some((text, _, _)) if !text.trim().is_empty() => " and read its test page",
                // The bars are not letters, so finding nothing in them is the expected
                // outcome; the model is loaded either way, which is the whole point.
                Some(_) => "",
                None => ", or rather did not: the request failed",
            }
        ),
    );
}

/// A small image with dark bars on a light ground, for the warm-up.
///
/// Not a screen capture on purpose: warming must not depend on Screen Recording having been
/// granted, and it must not read the user's screen before anything has asked it to. The bars
/// are there so the text *detector* passes something to the *recogniser* — it is the second
/// of those two models that the first real call would otherwise wait for.
fn synthetic_page() -> Option<(CFRetained<CGImage>, Vec<u8>)> {
    let (w, h) = (240usize, 64usize);
    let bytes_per_row = w * 4;
    let mut buf = vec![0u8; bytes_per_row * h];
    let space = CGColorSpace::new_device_rgb()?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: as in `render` — the buffer is sized to the geometry and outlives the context.
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
    CGContext::set_rgb_fill_color(Some(&ctx), 1.0, 1.0, 1.0, 1.0);
    CGContext::fill_rect(
        Some(&ctx),
        CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(w as CGFloat, h as CGFloat),
        ),
    );
    CGContext::set_rgb_fill_color(Some(&ctx), 0.05, 0.05, 0.05, 1.0);
    for i in 0..6 {
        CGContext::fill_rect(
            Some(&ctx),
            CGRect::new(
                CGPoint::new(24.0 + i as CGFloat * 32.0, 16.0),
                CGSize::new(8.0, 32.0),
            ),
        );
    }
    CGContext::flush(Some(&ctx));
    let image = CGBitmapContextCreateImage(Some(&ctx))?;
    drop(ctx);
    Some((image, buf))
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

/// Logs the cost of a recognition where a user can act on it.
///
/// The first one is called out separately because it is not representative: Vision loads its
/// model on first use, and that one-off can be an order of magnitude above the steady state.
/// After that, 50 ms is the host's own threshold for "this blocked the pump" — one module
/// polls this every 120 ms and issues two calls a tick, measured at 19-31 ms each on
/// Windows, so a materially larger figure here is the number that decides whether the macOS
/// port can keep that module at all.
///
/// Only a new worst time is written, and only if it is clearly worse than the last one
/// reported. A slow recogniser is called sixteen times a second by that same module, and a
/// line per call would bury the log it is meant to be evidence in.
fn note_cost(ms: f64, w: i32, h: i32) {
    thread_local! {
        static WORST: RefCell<Option<f64>> = const { RefCell::new(None) };
    }
    let previous = WORST.with(|worst| {
        let mut worst = worst.borrow_mut();
        let previous = *worst;
        if previous.is_none_or(|p| ms > p) {
            *worst = Some(ms);
        }
        previous
    });
    match previous {
        None => crate::logging::line(
            "macos",
            &format!("ocr: first recognition of a {w}x{h} pt region took {ms:.0} ms, Vision's model load included"),
        ),
        Some(p) if ms >= 50.0 && ms > p * 1.25 => crate::logging::line(
            "macos",
            &format!("ocr: reading a {w}x{h} pt region took {ms:.0} ms, the slowest so far"),
        ),
        _ => {}
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
/// lone digit well clear of the floor rather than near it, and widens the quiet space around
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
) -> Option<VisionRead> {
    let big = Plan {
        x0: tight.x0,
        y0: tight.y0,
        cw: tight.cw,
        ch: tight.ch,
        up: (tight.up * 2).min(10),
        pad: OCR_PAD * 2,
        ink_h: tight.ink_h,
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
    let mut out = run_vision(&img, lang, ACCURATE, &|bb| map_box(&big, scale, bb));
    // The fast model only for a language it reads, and not while an interactive read waits
    // behind a background one.
    if out.as_ref().is_none_or(|(t, _, _)| t.trim().is_empty())
        && ladder.fast_ok
        && !ladder.preempted()
    {
        crate::logging::trace("macos", || {
            format!("ocr: {}x enlarged accurate pass read nothing, trying the fast model", big.up)
        });
        out = run_vision(&img, lang, FAST, &|bb| map_box(&big, scale, bb));
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

fn run_vision(
    image: &CGImage,
    lang: Option<&str>,
    level: VNRequestTextRecognitionLevel,
    map: &dyn Fn(CGRect) -> (i32, i32, i32, i32),
) -> Option<VisionRead> {
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
    // fail the request outright on macOS 12, where it does not exist, and the default is
    // already the newest revision the running system supports.
    //
    // `setCustomWords` is likewise skipped: Vision only consults it during the
    // language-correction stage, which is switched off above, so it would be a no-op that
    // reads like a precaution.

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
    let requests: Retained<NSArray<VNRequest>> =
        NSArray::from_slice(&[request.as_ref() as &VNRequest]);
    if let Err(e) = handler.performRequests_error(&requests) {
        warn_once(
            "ocr-perform",
            &format!("ocr: Vision refused the request — {}", e.localizedDescription()),
        );
        return None;
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
    Some((text, words, out_lines))
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
        // Row by row through the slice rather than by index, so the bounds check happens once
        // per row instead of three times per pixel.
        for (y, row) in rgba.chunks_exact(nw * 4).enumerate() {
            for (x, px) in row.chunks_exact(4).enumerate() {
                let dr = px[0] as f64 - bgr;
                let dg = px[1] as f64 - bgg;
                let db = px[2] as f64 - bgb;
                if dr * dr + dg * dg + db * db > threshold_sq {
                    found = true;
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
        if let Panel::Ink(ix0, iy0, ix1, iy1, inner_bg) = panel {
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
        // roughly 33 px. This file's own header promises about 64, and Apple's floor for
        // reliable recognition is around 32. Every lone digit was being handed over sitting
        // exactly on that floor. From the ink it is 64/11 = 5, and the glyph arrives at 55.
        let ink_h = y1 - y0 + 1;
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
    for y in y0..=y1 {
        for x in x0..=x1 {
            if far(at(x, y), panel) > DIFFERENT_SQ {
                found = true;
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
    Panel::Ink(ix0, iy0, ix1, iy1, [panel[0] / 255.0, panel[1] / 255.0, panel[2] / 255.0])
}

/// What the crop turned out to be. See `ink_inside_panel`.
enum Panel {
    NotAPanel,
    /// A box with nothing drawn on it — a value field with no value in it.
    Empty,
    /// A box, and the bounds of what is drawn on it, plus the box's own colour.
    Ink(usize, usize, usize, usize, [f64; 3]),
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
/// single small glyph is the difference between comfortably above Apple's recognition floor
/// and sitting on it.
fn upscale_for(content_px: usize) -> usize {
    (TARGET_CONTENT_PX / content_px.max(1)).clamp(1, 10)
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

/// Writes what Vision was given next to the executable, when `AUTOMATION_PLATFORM_OCR_DEBUG`
/// is set — the same switch as on Windows.
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
