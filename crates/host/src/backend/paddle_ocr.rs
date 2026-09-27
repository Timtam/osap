//! Secondary OCR engine: a PaddleOCR recognition model run in-process via ONNX
//! Runtime (the `ort` crate), recognition-only. Windows.Media.Ocr is fast but
//! blind to isolated single glyphs (a lone "1"); this neural recognizer reads
//! them. Used only as a fallback when the system OCR returns nothing on a small
//! region — so the fast path stays the OS engine and this pays its cost rarely.
//!
//! The recognition model and its dictionary are embedded in the binary (via
//! `include_bytes!`), so the app stays fully self-contained and portable.
//!
//! **One recogniser thread, asked in parallel with the system engine.** `recognize_image`
//! (`windows.rs`) hands every small region to this recogniser the moment it has the pixels —
//! before `Windows.Media.Ocr` starts on it — and waits for the answer only when
//! `Windows.Media.Ocr` read nothing. That rule is unchanged, and deliberate: the two run side by
//! side on every small region, so a miss costs the slower of the two rather than their sum. What
//! changed is who runs the recognition: it used to be a new thread per region, left running when
//! `Windows.Media.Ocr` had answered or the region was blank. At Melodyne's ten regions a second
//! that was half a core of answers nobody read, and at seventy blank regions a second two and a
//! half cores, with as many threads as regions arrived faster than the one session drained them.
//! Now one long-lived thread ([`ask`]) takes the regions in order, and a region whose answer is
//! no longer wanted is cancelled when its [`Asked`] is dropped — skipped if it has not started,
//! stopped before its preprocessing or before the session if it has. The session was already
//! one, behind a lock, with one intra-op thread, so recognitions were one at a time before too;
//! only the preprocessing of regions nobody waits for no longer runs.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
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
/// `recognize_image` (`windows.rs`) asks the recogniser thread for one of these for every small
/// region, beside `Windows.Media.Ocr`, and when `Windows.Media.Ocr` answers it does not wait
/// for it: that is the point of running them side by side, so that a miss costs the slower of
/// the two rather than their sum. The recogniser can therefore still be inside ONNX Runtime when
/// the application exits, and a thread in there while `ort`'s static cleanup runs is the fault
/// the warm-up join in `run` exists for: an access violation at exit. This counts them — from
/// the moment one is asked for until it is done, skipped or cancelled — so that the exit can
/// wait for them the way it waits for the warm-up — see `settle`. The recogniser thread itself,
/// waiting for work, is in no call of ONNX Runtime's and is not counted.
///
/// And once that wait has begun it is CLOSED: `start` refuses, so nothing enters ONNX Runtime
/// after the exit stopped waiting. The recognise thread of `host.ocr.read` can still be working
/// through a job of many regions then — the exit waits for it only a second — and without this
/// each small region it reached would start a recognition nobody waits for.
pub(super) struct InFlight {
    running: AtomicUsize,
    closed: AtomicBool,
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
        InFlight { running: AtomicUsize::new(0), closed: AtomicBool::new(false) }
    }

    /// Counts one recognition, from now until the returned guard is dropped; `None`, counting
    /// nothing, once [`close`](Self::close) was called — the caller then does not start it.
    ///
    /// Taken by the caller BEFORE it hands the region over and moved into the job: counted only
    /// once the recogniser had started it, a recognition asked for a moment before the exit
    /// would not be counted yet, and the exit would not wait for it.
    ///
    /// Counted first and checked second, both sequentially consistent, against `close`, which
    /// sets the flag first and reads the count second: whichever order the two threads meet
    /// in, either this sees the flag and backs out, or the exit sees this counted and waits.
    pub(super) fn start(&self) -> Option<Running<'_>> {
        self.running.fetch_add(1, Ordering::SeqCst);
        if self.closed.load(Ordering::SeqCst) {
            self.running.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Running { of: self })
    }

    /// No recognition starts from now on (see the type).
    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    pub(super) fn count(&self) -> usize {
        self.running.load(Ordering::SeqCst)
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
/// Called by `run` at exit, beside the warm-up join; see `InFlight`. Closes the count first, so
/// no recognition starts after this has looked. Returns at once when nothing is running.
/// Otherwise it logs how long it waited, or, when the bound is hit, that it gave up — and then
/// leaves what is still running to the process exit, which is the state of things before this
/// wait existed, now with a line in the log when it happens.
pub fn settle() {
    let started = Instant::now();
    IN_FLIGHT.close();
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

/// A recognition asked of the recogniser thread ([`ask`]). Dropping it cancels the recognition:
/// skipped if the thread has not reached it, stopped at the next point it looks if it has.
pub(super) struct Asked {
    cancel: Arc<AtomicBool>,
    answer: Receiver<Option<String>>,
}

impl Asked {
    /// Waits for the answer: the text, or `None` when the recogniser found nothing, is not
    /// available, or its thread has gone. As long as the recognition takes — tens of
    /// milliseconds warm — and no longer: the thread answers every job it takes.
    pub(super) fn wait(self) -> Option<String> {
        self.answer.recv().ok().flatten()
    }
}

impl Drop for Asked {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

/// A region for the recogniser thread, counted in [`IN_FLIGHT`] until it is dropped.
struct Job {
    cap: CapturedImage,
    cancel: Arc<AtomicBool>,
    reply: SyncSender<Option<String>>,
    _running: Running<'static>,
}

/// Regions waiting for the recogniser: a caller waits for its own region or cancels it before
/// it hands over the next, so a handful at most — the cap only guards against a recogniser
/// that has stopped answering.
const QUEUE_MAX: usize = 32;

static JOBS: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
static JOB_READY: Condvar = Condvar::new();
/// Whether the recogniser thread was started; decided once, on the first region.
static RECOGNISER: OnceLock<bool> = OnceLock::new();
/// Whether the recogniser thread is still there to answer. Set before it starts, cleared — under
/// the [`JOBS`] lock, with the queue emptied — if it ever ends; [`ask`] reads it under the same
/// lock, so no region is queued for a thread that has gone. A queued region holds its caller's
/// only way to be answered: left in the queue of a thread that has gone, its caller's `wait`
/// would never return, and a synchronous `host.ocr.recognize` waits on the event loop, which
/// carries every captured key.
static ALIVE: AtomicBool = AtomicBool::new(false);

/// Hands a small region to the recogniser thread, which starts on it as soon as it is free —
/// before the caller runs `Windows.Media.Ocr` on the same region. `None`, and nothing started,
/// once the exit's wait has begun (see [`InFlight::start`]), when the thread could not be
/// started or has gone ([`ALIVE`]), or when [`QUEUE_MAX`] regions already wait (a recogniser
/// that stopped answering; said once).
pub(super) fn ask(cap: &CapturedImage) -> Option<Asked> {
    let running = IN_FLIGHT.start()?;
    if !*RECOGNISER.get_or_init(start_recogniser) {
        return None;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let (reply, answer) = sync_channel(1);
    let job = Job { cap: CapturedImage { w: cap.w, h: cap.h, rgba: cap.rgba.clone() }, cancel: cancel.clone(), reply, _running: running };
    let admitted = admit(&mut lock_even_if_poisoned(&JOBS), &ALIVE, job);
    match admitted {
        Ok(()) => {}
        // The thread has gone (said when it went): WinRT reads alone.
        Err(Refused::Gone) => return None,
        Err(Refused::Full) => {
            static SAID: AtomicBool = AtomicBool::new(false);
            if !SAID.swap(true, Ordering::Relaxed) {
                crate::logging::line(
                    "ocr",
                    &format!(
                        "{QUEUE_MAX} small regions wait for the neural recogniser, which has stopped \
                         keeping up or answering; regions are read by Windows.Media.Ocr alone until \
                         it catches up (said once)"
                    ),
                );
            }
            return None;
        }
    }
    JOB_READY.notify_one();
    Some(Asked { cancel, answer })
}

/// Why [`admit`] did not queue a region.
#[derive(Debug, PartialEq, Eq)]
enum Refused {
    /// The recogniser thread has gone.
    Gone,
    /// [`QUEUE_MAX`] regions already wait.
    Full,
}

/// Queues `job` — asked with the queue's lock held — unless the recogniser thread has gone
/// (`alive` false) or the queue is full; the cancelled ones leave first. A refused job is
/// dropped, and its caller is not handed a wait.
fn admit(jobs: &mut VecDeque<Job>, alive: &AtomicBool, job: Job) -> Result<(), Refused> {
    if !alive.load(Ordering::SeqCst) {
        return Err(Refused::Gone);
    }
    prune_cancelled(jobs);
    if jobs.len() >= QUEUE_MAX {
        return Err(Refused::Full);
    }
    jobs.push_back(job);
    Ok(())
}

/// The recogniser thread has gone: `alive` cleared and the queue emptied, both under the queue's
/// lock, so [`admit`] queues nothing after. Returns the jobs that waited, for the caller to drop
/// outside the lock — each one dropped answers its caller's `wait` with `None`.
fn orphan_all(jobs: &Mutex<VecDeque<Job>>, alive: &AtomicBool) -> Vec<Job> {
    let mut jobs = lock_even_if_poisoned(jobs);
    alive.store(false, Ordering::SeqCst);
    jobs.drain(..).collect()
}

/// What was cancelled while it waited goes now, uncounted with it.
fn prune_cancelled(jobs: &mut VecDeque<Job>) {
    jobs.retain(|j| !j.cancel.load(Ordering::Acquire));
}

fn start_recogniser() -> bool {
    ALIVE.store(true, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("paddle-ocr".to_string()).spawn(serve);
    if let Err(e) = &spawned {
        ALIVE.store(false, Ordering::SeqCst);
        crate::logging::line(
            "ocr",
            &format!("could not start the neural recogniser's thread ({e}); small regions are read by Windows.Media.Ocr alone"),
        );
    }
    spawned.is_ok()
}

/// Marks the recogniser gone when its thread ends, however it ends, and answers everything still
/// queued with nothing: each job dropped drops its reply channel, and its caller's `wait` returns
/// `None`. See [`ALIVE`].
struct Gone;

impl Drop for Gone {
    fn drop(&mut self) {
        let orphans = orphan_all(&JOBS, &ALIVE);
        let n = orphans.len();
        drop(orphans);
        crate::logging::line(
            "ocr",
            &format!(
                "the neural recogniser's thread ended; {n} waiting region(s) were answered with \
                 nothing, and small regions are read by Windows.Media.Ocr alone from now on"
            ),
        );
    }
}

/// The recogniser thread: one region at a time, in the order asked, for as long as the process
/// runs. A panic in one recognition answers that region `None` and the thread carries on; a
/// panic anywhere else in its loop (the wait, the log) drops only the job in hand, whose caller
/// is answered `None`, and the loop starts again. Should the thread end all the same, [`Gone`]
/// answers what is left.
fn serve() {
    let _gone = Gone;
    loop {
        if std::panic::catch_unwind(serve_jobs).is_ok() {
            return; // `serve_jobs` never returns; kept for the type
        }
        static OUTER: AtomicUsize = AtomicUsize::new(0);
        let n = OUTER.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_power_of_two() {
            crate::logging::line(
                "ocr",
                &format!("the neural recogniser's loop panicked outside a recognition ({n} so far); it carries on"),
            );
        }
    }
}

/// The loop of [`serve`], one job at a time.
fn serve_jobs() {
    loop {
        let job = {
            let mut jobs = lock_even_if_poisoned(&JOBS);
            loop {
                if let Some(job) = jobs.pop_front() {
                    break job;
                }
                jobs = JOB_READY.wait(jobs).unwrap_or_else(|p| p.into_inner());
            }
        };
        if job.cancel.load(Ordering::Acquire) {
            continue; // dropped here, uncounted
        }
        let answer = match crate::logging::contain(|| recognize_unless(&job.cap, &job.cancel)) {
            Ok(answer) => answer,
            Err(report) => {
                // The 1st, 2nd, 4th … — one region that trips something would trip it again.
                static PANICS: AtomicUsize = AtomicUsize::new(0);
                let n = PANICS.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_power_of_two() {
                    crate::logging::line(
                        "ocr",
                        &format!(
                            "the neural recogniser panicked on a region ({n} so far this session); \
                             it was answered with nothing and the recogniser carries on: {report}"
                        ),
                    );
                }
                None
            }
        };
        let _ = job.reply.send(answer);
    }
}

/// Recognizes text in a captured region via the neural recognizer, or `None` when the engine
/// is unavailable, finds nothing, or `cancel` was set — looked at before the preprocessing,
/// before waiting for the session, and once the session is in hand.
fn recognize_unless(cap: &CapturedImage, cancel: &AtomicBool) -> Option<String> {
    let cancelled = || cancel.load(Ordering::Acquire);
    if cancelled() {
        return None;
    }
    let eng = engine()?;

    let rgba = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone())?;
    let rgb = image::RgbImage::from_fn(cap.w, cap.h, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        image::Rgb([p[0], p[1], p[2]])
    });

    let tight = tighten(&rgb);
    let input = preprocess(&tight);
    let tensor = Tensor::from_array(input).ok()?;

    if cancelled() {
        return None;
    }
    let mut session = lock_even_if_poisoned(&eng.session);
    if cancelled() {
        return None;
    }
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

        let running = c.start().expect("open");
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

    /// Once the exit's wait has begun nothing new is counted or started, and what was already
    /// running is still waited for.
    #[test]
    fn closed_refuses_new_recognitions_and_still_counts_the_running_ones() {
        let c = InFlight::new();
        let running = c.start().expect("open");
        c.close();
        assert!(c.start().is_none(), "no recognition starts after the exit looked");
        assert_eq!(c.count(), 1, "the refused one left no count behind");
        assert!(!c.wait_idle(Duration::ZERO));
        drop(running);
        assert!(c.wait_idle(Duration::ZERO));
        assert!(c.start().is_none(), "closed stays closed");
    }

    /// A recognition that never finishes costs the exit its bound and no more.
    #[test]
    fn the_wait_gives_up_at_its_bound() {
        let c = InFlight::new();
        let _stuck = c.start().expect("open");
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
                let _running = c.start().expect("open");
                panic!("{EXPECTED_PANIC}inside the recogniser");
            })
            .join()
        });
        assert!(joined.is_err());
        assert_eq!(c.count(), 0);
    }

    /// A region whose answer is no longer wanted — WinRT read it, or it was blank — is cancelled
    /// by dropping its handle, and leaves the queue uncounted; a cancelled one never reaches the
    /// engine at all.
    #[test]
    fn a_cancelled_region_leaves_the_queue_uncounted_and_is_never_recognised() {
        static C: InFlight = InFlight::new();
        let mut jobs = VecDeque::new();
        let mut asked = Vec::new();
        for _ in 0..3 {
            let cancel = Arc::new(AtomicBool::new(false));
            let (reply, answer) = sync_channel(1);
            let cap = CapturedImage { w: 1, h: 1, rgba: vec![0; 4] };
            jobs.push_back(Job { cap, cancel: cancel.clone(), reply, _running: C.start().expect("open") });
            asked.push(Asked { cancel, answer });
        }
        assert_eq!(C.count(), 3);
        drop(asked.remove(0)); // the first: WinRT answered
        prune_cancelled(&mut jobs);
        assert_eq!((jobs.len(), C.count()), (2, 2));
        // A job answered: its caller hears it, and it is uncounted once dropped.
        let job = jobs.pop_front().unwrap();
        job.reply.send(Some("7".into())).unwrap();
        drop(job);
        assert_eq!(asked.remove(0).wait(), Some("7".to_string()));
        assert_eq!(C.count(), 1);
        // A recogniser gone without answering: the wait ends with nothing rather than hanging.
        drop(jobs.pop_front());
        assert_eq!(asked.remove(0).wait(), None);
        assert_eq!(C.count(), 0);
        // Cancelled before it starts: no engine is even opened for it.
        let cap = CapturedImage { w: 4, h: 4, rgba: vec![0; 64] };
        assert_eq!(recognize_unless(&cap, &AtomicBool::new(true)), None);
    }

    /// The recogniser thread gone: everything it left in the queue is answered with nothing, and
    /// nothing is queued for it after — a synchronous recognition waiting on the event loop
    /// would otherwise wait for ever, and every captured key with it.
    #[test]
    fn a_recogniser_that_has_gone_answers_its_queue_and_takes_no_more() {
        static C: InFlight = InFlight::new();
        let jobs: Mutex<VecDeque<Job>> = Mutex::new(VecDeque::new());
        let alive = AtomicBool::new(true);
        let job = || {
            let cancel = Arc::new(AtomicBool::new(false));
            let (reply, answer) = sync_channel(1);
            let cap = CapturedImage { w: 1, h: 1, rgba: vec![0; 4] };
            (Job { cap, cancel: cancel.clone(), reply, _running: C.start().expect("open") }, Asked { cancel, answer })
        };
        let mut waiting = Vec::new();
        for _ in 0..3 {
            let (j, a) = job();
            assert_eq!(admit(&mut jobs.lock().unwrap(), &alive, j), Ok(()));
            waiting.push(a);
        }
        // Waiters on other threads, as the event loop and the OCR thread would be.
        let waits: Vec<_> = waiting.into_iter().map(|a| std::thread::spawn(move || a.wait())).collect();
        let orphans = orphan_all(&jobs, &alive);
        assert_eq!(orphans.len(), 3);
        drop(orphans);
        for w in waits {
            assert_eq!(w.join().unwrap(), None, "answered with nothing, not left waiting");
        }
        assert_eq!(C.count(), 0);
        let (j, a) = job();
        assert_eq!(admit(&mut jobs.lock().unwrap(), &alive, j), Err(Refused::Gone));
        assert_eq!(a.wait(), None, "the refused job was dropped, so even a wait returns");
        assert!(jobs.lock().unwrap().is_empty());
        // A full queue refuses too.
        let alive = AtomicBool::new(true);
        let mut q = VecDeque::new();
        let mut held = Vec::new();
        for _ in 0..QUEUE_MAX {
            let (j, a) = job();
            assert_eq!(admit(&mut q, &alive, j), Ok(()));
            held.push(a);
        }
        let (j, _a) = job();
        assert_eq!(admit(&mut q, &alive, j), Err(Refused::Full));
        // Cancelling one makes room.
        drop(held.remove(0));
        let (j, _b) = job();
        assert_eq!(admit(&mut q, &alive, j), Ok(()));
    }

    /// The thread's loop is kept going by an outer catch, and its end is what answers the queue:
    /// checked where it is written, since the real thread holds the real model.
    #[test]
    fn the_recogniser_thread_answers_its_queue_however_it_ends() {
        const SRC: &str = include_str!("paddle_ocr.rs");
        let serve = &SRC[SRC.find("fn serve() {").unwrap()..SRC.find("fn serve_jobs() {").unwrap()];
        assert!(serve.contains("let _gone = Gone;"), "the guard that answers the queue");
        assert!(serve.contains("std::panic::catch_unwind(serve_jobs)"), "the loop starts again after a panic");
        let ask = &SRC[SRC.find("pub(super) fn ask(").unwrap()..SRC.find("enum Refused").unwrap()];
        assert!(ask.contains("admit(&mut lock_even_if_poisoned(&JOBS), &ALIVE, job)"));
        let gone = &SRC[SRC.find("impl Drop for Gone").unwrap()..SRC.find("fn serve() {").unwrap()];
        assert!(gone.contains("orphan_all(&JOBS, &ALIVE)"));
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
