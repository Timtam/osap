//! Secondary OCR engine: a PaddleOCR recognition model run in-process via ONNX
//! Runtime (the `ort` crate), recognition-only. Windows.Media.Ocr is fast but
//! blind to isolated single glyphs (a lone "1"); this neural recognizer reads
//! them. Used only as a fallback when the system OCR returns nothing on a small
//! region — so the fast path stays the OS engine and this pays its cost rarely.
//!
//! The recognition model and its dictionary are embedded in the binary (via
//! `include_bytes!`), so the app stays fully self-contained and portable.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use super::CapturedImage;

struct Engine {
    session: Mutex<Session>,
    /// Recognition alphabet: class index `i` (1-based, after the CTC blank at 0)
    /// maps to `dict[i - 1]`.
    dict: Vec<String>,
}

static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();

fn engine() -> Option<&'static Engine> {
    ENGINE.get_or_init(|| init().ok()).as_ref()
}

/// Takes a lock whether or not an earlier holder panicked while holding it.
///
/// For the session lock that is the right answer, and `lock().ok()?` was the wrong one: one
/// panic under it — in `ort`'s wrapper, or in the decode that reads the outputs while the
/// session is still borrowed — poisoned the mutex for good, and every later recognition
/// answered `None` without a word, which is to say the lone-digit fallback was switched off for
/// the rest of the session. A panic cannot leave the session half-changed: the only thing done
/// to it under the lock is `run`, and ONNX Runtime's `Run` does not change the session — its
/// documentation allows several threads to call it on one session at once. (The mutex is
/// there because `ort`'s `run` takes `&mut self`, not because the session needs it.)
fn lock_even_if_poisoned<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Recognitions that are, or are about to be, inside ONNX Runtime.
///
/// `recognize_image` (`windows.rs`) runs one of these on a thread of its own for every small
/// region, beside `Windows.Media.Ocr`, and when `Windows.Media.Ocr` answers it does not wait
/// for it: that is the point of running them side by side, so that a miss costs the slower of
/// the two rather than their sum. Such a thread can therefore still be inside ONNX Runtime when
/// the application exits, and a thread in there while `ort`'s static cleanup runs is the fault
/// the warm-up join in `run` exists for: an access violation at exit. This counts them, so
/// that the exit can wait for them the way it waits for the warm-up — see `settle`.
pub(super) struct InFlight {
    running: AtomicUsize,
}

/// The recognitions of this process. A struct rather than a bare counter so the tests can
/// have counters of their own and run beside each other.
pub(super) static IN_FLIGHT: InFlight = InFlight::new();

/// One recognition counted in `InFlight`. Ends when it is dropped, unwinding included, so a
/// recognition that panics does not keep the exit waiting for it.
pub(super) struct Running<'a> {
    of: &'a InFlight,
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.of.running.fetch_sub(1, Ordering::AcqRel);
    }
}

impl InFlight {
    pub(super) const fn new() -> Self {
        InFlight { running: AtomicUsize::new(0) }
    }

    /// Counts one recognition, from now until the returned guard is dropped.
    ///
    /// Taken by the caller BEFORE it spawns the thread and moved into it: counted only once
    /// the thread had started, a recognition spawned a moment before the exit would not be
    /// counted yet, and the exit would not wait for it.
    pub(super) fn start(&self) -> Running<'_> {
        self.running.fetch_add(1, Ordering::AcqRel);
        Running { of: self }
    }

    pub(super) fn count(&self) -> usize {
        self.running.load(Ordering::Acquire)
    }

    /// Waits until nothing is counted, for at most `bound`. True when nothing is.
    ///
    /// Polled, in steps of a few milliseconds: this runs once, at exit, and a recognition takes
    /// tens of milliseconds, so a condition variable would buy nothing a person could notice.
    pub(super) fn wait_idle(&self, bound: Duration) -> bool {
        const STEP: Duration = Duration::from_millis(5);
        let deadline = Instant::now() + bound;
        loop {
            if self.count() == 0 {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            std::thread::sleep((deadline - now).min(STEP));
        }
    }
}

/// How long the exit waits for recognitions still running: many times the ~15 ms a warm
/// recognition of a small region takes, and short enough that an exit that has to give up is
/// still an exit. The same half second the desktop duplication thread is given at exit.
const SETTLE_BOUND: Duration = Duration::from_millis(500);

/// Waits, for at most half a second, until no recognition is inside ONNX Runtime any more.
/// Called by `run` at exit, beside the warm-up join; see `InFlight`. Returns at once when
/// nothing is running. Otherwise it logs how long it waited, or, when the bound is hit, that it
/// gave up — and then leaves what is still running to the process exit, which is the state of
/// things before this wait existed, now with a line in the log when it happens.
pub fn settle() {
    let started = Instant::now();
    let at_exit = IN_FLIGHT.count();
    if at_exit == 0 {
        return;
    }
    // Logged either way once there was something to wait for — once per exit, and the only
    // record of whether this wait ever matters in practice.
    if IN_FLIGHT.wait_idle(SETTLE_BOUND) {
        crate::logging::line(
            "ocr",
            &format!(
                "waited {} ms at exit for {at_exit} neural OCR recognition(s) to finish",
                started.elapsed().as_millis()
            ),
        );
    } else {
        crate::logging::line(
            "ocr",
            &format!(
                "{} neural OCR recognition(s) still running after {} ms at exit; leaving them to \
                 the process exit, which can fault inside ONNX Runtime",
                IN_FLIGHT.count(),
                started.elapsed().as_millis()
            ),
        );
    }
}

/// Recognition model + dictionary, embedded so the app is self-contained.
const MODEL: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocr-rec.onnx"));
const DICT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocr-dict.txt"));

fn init() -> anyhow::Result<Engine> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(1)?
        .commit_from_memory(MODEL)?;
    let mut dict: Vec<String> = DICT.lines().map(|l| l.to_string()).collect();
    dict.push(" ".to_string()); // PaddleOCR appends a space char (use_space_char)
    Ok(Engine {
        session: Mutex::new(session),
        dict,
    })
}

/// Recognizes text in a captured region via the neural recognizer, or `None`
/// when the engine is unavailable or finds nothing.
pub fn recognize(cap: &CapturedImage) -> Option<String> {
    let eng = engine()?;

    let rgba = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone())?;
    let rgb = image::RgbImage::from_fn(cap.w, cap.h, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        image::Rgb([p[0], p[1], p[2]])
    });

    let tight = tighten(&rgb);
    let input = preprocess(&tight);
    let tensor = Tensor::from_array(input).ok()?;

    let mut session = lock_even_if_poisoned(&eng.session);
    let outputs = session.run(ort::inputs![tensor]).ok()?;
    let logits = outputs[0].try_extract_array::<f32>().ok()?;
    let text = decode(&logits, &eng.dict);

    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Loads the model and warms the inference graph on a background thread, so the
/// first real OCR doesn't pay the (one-time, hundreds-of-ms) init cost on the
/// hot path. No-op if the model files aren't present. Returns the thread handle:
/// the caller MUST join it before the process exits, or a fast exit can tear the
/// process down while this thread is still inside ONNX Runtime init, racing ort's
/// static cleanup (an access violation — the "headless segfault").
pub fn warmup() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        let Some(eng) = engine() else { return };
        let dummy = image::RgbImage::from_pixel(40, 16, image::Rgb([20, 20, 20]));
        let input = preprocess(&tighten(&dummy));
        if let Ok(tensor) = Tensor::from_array(input) {
            let _ = lock_even_if_poisoned(&eng.session).run(ort::inputs![tensor]);
        }
    })
}

/// Crops to the content bounding box (pixels far from the corner background) and
/// upscales so tiny glyphs become large — a cheap "poor man's detection" that is
/// what lets the recognizer read small, isolated digits.
fn tighten(img: &image::RgbImage) -> image::RgbImage {
    use image::imageops::{resize, FilterType};
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return img.clone();
    }
    let at = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let cs = [at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
    let bg = [
        (cs[0][0] + cs[1][0] + cs[2][0] + cs[3][0]) / 4.0,
        (cs[0][1] + cs[1][1] + cs[2][1] + cs[3][1]) / 4.0,
        (cs[0][2] + cs[1][2] + cs[2][2] + cs[3][2]) / 4.0,
    ];
    let thr = 55.0f32;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    let mut found = false;
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            let d = ((p[0] as f32 - bg[0]).powi(2)
                + (p[1] as f32 - bg[1]).powi(2)
                + (p[2] as f32 - bg[2]).powi(2))
            .sqrt();
            if d > thr {
                found = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !found {
        return img.clone();
    }
    let m = 3i32;
    let x0 = (x0 as i32 - m).max(0) as u32;
    let y0 = (y0 as i32 - m).max(0) as u32;
    let x1 = (x1 as i32 + m).min(w as i32 - 1) as u32;
    let y1 = (y1 as i32 + m).min(h as i32 - 1) as u32;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let cropped = image::imageops::crop_imm(img, x0, y0, cw, ch).to_image();
    let scale = (48.0 / ch as f32).max(2.0);
    let nw = ((cw as f32 * scale).round() as u32).max(1);
    let nh = ((ch as f32 * scale).round() as u32).max(1);
    resize(&cropped, nw, nh, FilterType::Lanczos3)
}

/// Resizes to height 48 (preserving aspect, width capped at 320) and normalizes
/// to [-1, 1] in NCHW RGB — the PP-OCR mobile recognition input format.
fn preprocess(img: &image::RgbImage) -> ndarray::Array4<f32> {
    let (w0, h0) = img.dimensions();
    let target_h = 48u32;
    let ratio = w0 as f32 / h0.max(1) as f32;
    let target_w = ((target_h as f32 * ratio).ceil() as u32).clamp(1, 320);
    let resized = image::imageops::resize(img, target_w, target_h, image::imageops::FilterType::Triangle);

    let mut arr = ndarray::Array4::<f32>::zeros((1, 3, target_h as usize, target_w as usize));
    for y in 0..target_h {
        for x in 0..target_w {
            let p = resized.get_pixel(x, y).0;
            for c in 0..3 {
                arr[[0, c, y as usize, x as usize]] = (p[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
    }
    arr
}

#[cfg(test)]
mod exit_safety_tests {
    use super::*;
    use crate::{quiet_expected_panics, EXPECTED_PANIC};

    /// Counted before the thread exists, uncounted when it is done, and the wait returns as
    /// soon as it is — not at its bound.
    #[test]
    fn the_exit_waits_for_a_recognition_and_no_longer() {
        let c = InFlight::new();
        assert!(c.wait_idle(Duration::ZERO), "nothing running: no wait at all");

        let running = c.start();
        assert_eq!(c.count(), 1, "counted before the thread is even spawned");
        let waited = std::thread::scope(|s| {
            s.spawn(move || {
                let _running = running;
                std::thread::sleep(Duration::from_millis(30));
            });
            let t = Instant::now();
            assert!(c.wait_idle(Duration::from_secs(10)));
            t.elapsed()
        });
        assert_eq!(c.count(), 0);
        assert!(waited < Duration::from_secs(5), "returned at the end of the run: {waited:?}");
    }

    /// A recognition that never finishes costs the exit its bound and no more.
    #[test]
    fn the_wait_gives_up_at_its_bound() {
        let c = InFlight::new();
        let _stuck = c.start();
        let t = Instant::now();
        assert!(!c.wait_idle(Duration::from_millis(40)));
        assert!(t.elapsed() >= Duration::from_millis(40));
        assert_eq!(c.count(), 1);
    }

    /// A recognition that panics is uncounted all the same, or every exit after it would sit
    /// out the whole bound.
    #[test]
    fn a_recognition_that_panics_is_uncounted() {
        quiet_expected_panics();
        let c = InFlight::new();
        let joined = std::thread::scope(|s| {
            s.spawn(|| {
                let _running = c.start();
                panic!("{EXPECTED_PANIC}inside the recogniser");
            })
            .join()
        });
        assert!(joined.is_err());
        assert_eq!(c.count(), 0);
    }

    /// One panic under the session lock no longer switches the recogniser off for good.
    #[test]
    fn a_panic_under_the_lock_does_not_disable_it() {
        quiet_expected_panics();
        let m = Mutex::new(41);
        let joined = std::thread::scope(|s| {
            s.spawn(|| {
                let _session = m.lock().unwrap();
                panic!("{EXPECTED_PANIC}under the session lock");
            })
            .join()
        });
        assert!(joined.is_err());
        assert!(m.is_poisoned());
        assert!(m.lock().is_err(), "`lock().ok()?` would answer None from here on");
        *lock_even_if_poisoned(&m) += 1;
        assert_eq!(*lock_even_if_poisoned(&m), 42);
    }
}

/// CTC greedy decode: argmax per timestep over the class axis, drop the blank
/// (index 0) and collapse repeats, mapping class `i` to `dict[i - 1]`.
fn decode(logits: &ndarray::ArrayViewD<f32>, dict: &[String]) -> String {
    let shape = logits.shape();
    if shape.len() != 3 {
        return String::new();
    }
    let (t, c) = (shape[1], shape[2]);
    let mut out = String::new();
    let mut prev = usize::MAX;
    for ti in 0..t {
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for ci in 0..c {
            let v = logits[[0, ti, ci]];
            if v > best_v {
                best_v = v;
                best = ci;
            }
        }
        if best != 0 && best != prev {
            if let Some(s) = dict.get(best - 1) {
                out.push_str(s);
            }
        }
        prev = best;
    }
    out
}
