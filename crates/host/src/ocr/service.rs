//! The two threads behind `host.ocr.read`: one that photographs, one that recognises.
//!
//! **The picture is taken at the call.** A read is handed to the capture thread the moment a
//! module asks, and that thread does nothing but capture, so a picture is never queued behind a
//! recognition — which on macOS can be a quarter of a second, and a help bubble a gamepad
//! press raised can be gone by then. Recognition runs on the second thread, one job at a time,
//! next to a game that wants the rest of the machine.
//!
//! **Read, then act, stays safe.** A module that reads a field and then clicks it expects the
//! read to see the field before the click. The click is synchronous and the read is not, so
//! `host.input.*` and `host.window.focus` wait — up to `policy::BARRIER` — until the calling
//! module's pending pictures are taken ([`Service::barrier`]), and those pictures go before
//! every other waiting one meanwhile.
//!
//! **A hang is a region that does not come back**, not a long job: the recognisers report each
//! region they answer, and new reads are refused only when none has been answered for
//! `policy::HANG`. The same per-region loop is where a closing application stops a job.
//!
//! **One capture thread**, named `screen-capture`, because the snapshot feature that comes
//! next shares it: a snapshot is the same act as the first half of a read.
//!
//! What stays as it was: the Windows neural recogniser still starts beside `Windows.Media.Ocr`
//! for every small region, inside the recognise stage exactly as `recognize` runs it, and its
//! answer is used only when the system engine reads nothing; its poison-tolerant lock and the
//! count of its runs that the exit waits for are `paddle_ocr.rs`'s — the count now closes when
//! that wait begins, because a job of this service can still be starting regions then. The
//! warm-ups (the neural model's, Vision's) keep their own threads, and the recogniser never
//! waits for them.
//!
//! Main-thread code talks to this through [`Service`]; the exit, through [`ShutdownHandle`],
//! which `run` takes out before the manager is built and calls after it is gone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::backend::{CaptureSource, OcrText, OcrThread, OcrWorker, Recognise, NO_RECOGNISER};
use crate::logging;

use super::lang::{self, LangReq, Languages};
use super::pipeline;
use super::policy::{CAPTURED_BUDGET, HANG, LANG_REREAD};
use super::sched::{Owner, Scheduler, Started, Submitted, Ticket, TicketId};
use super::types::{EngineLine, EngineOut, Priority, Reading, Rect, WordBox};

/// The two limits the threads enforce, as numbers the tests can shrink.
#[derive(Clone, Copy, Debug)]
struct Limits {
    /// No region answered for this long is a hang (`policy::HANG`).
    hang: Duration,
    /// Captured-but-unrecognised bytes above which background captures wait
    /// (`policy::CAPTURED_BUDGET`).
    budget: usize,
}

impl Limits {
    const POLICY: Limits = Limits { hang: HANG, budget: CAPTURED_BUDGET };
}

/// What a region is answered when the recognition panicked inside the application.
const RECOGNISER_FAILED: &str = "the text recogniser failed inside the application; the log has the details";

/// What one read asks for. Two reads that ask for the same thing before the first is
/// photographed share one capture and one recognition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spec {
    pub regions: Vec<Rect>,
    pub lang: LangReq,
    pub source: CaptureSource,
}

/// A finished job, handed to the event loop: one reading per region, for every ticket.
pub struct Done {
    pub tickets: Vec<Ticket>,
    pub readings: Vec<Reading>,
    /// The language request that could not be met, when that is why the readings failed —
    /// the event loop logs it once per module and tag.
    pub lang_error: Option<(LangReq, String)>,
    pub timings: Timings,
}

/// Where a job's time went, in milliseconds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// From the call until its picture was being taken.
    pub before_capture: u64,
    pub capture: u64,
    /// From the picture until recognition began.
    pub before_recognition: u64,
    pub recognition: u64,
}

impl Timings {
    pub fn total(&self) -> u64 {
        self.before_capture + self.capture + self.before_recognition + self.recognition
    }
}

/// The pixels a job holds between its two stages: the platform's, or why there are none.
type Pixels<S> = Result<S, String>;

struct State<S> {
    sched: Scheduler<Spec, Pixels<S>>,
    /// While a recognition runs: when it began, or answered its last region, whichever is later.
    /// `None` when none is running.
    answered_at: Option<Instant>,
    /// Said once per hang, and once when it ends.
    hang_said: bool,
}

impl<S> State<S> {
    /// The running recognition answered a region (`still_running`), or finished: the hang clock
    /// starts again, or stops, and a hang that was reported is reported over.
    fn answered(&mut self, now: Instant, still_running: bool) {
        let since = if still_running { self.answered_at.replace(now) } else { self.answered_at.take() };
        if self.hang_said {
            self.hang_said = false;
            logging::line(
                "ocr",
                &format!(
                    "the text recogniser answered again after {} ms without an answer",
                    since.map_or(0, |s| now.saturating_duration_since(s).as_millis())
                ),
            );
        }
    }
}

struct Inner<S> {
    limits: Limits,
    state: Mutex<State<S>>,
    /// The capture thread waits here for a job to photograph.
    capture_cv: Condvar,
    /// The recognise thread waits here for a job to recognise.
    recognise_cv: Condvar,
    /// The event loop's input barrier waits here for pictures to be taken.
    barrier_cv: Condvar,
    stop: AtomicBool,
    /// Interactive work is waiting: a background recognition skips its optional passes.
    preempt: AtomicBool,
    langs: Mutex<LangState>,
    langs_cv: Condvar,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Default)]
struct LangState {
    known: Option<Languages>,
    read_at: Option<Instant>,
    /// Asked to read the list again (a language did not resolve).
    again: bool,
}

/// Locks, whether or not a thread panicked while holding the lock: every state behind these
/// locks is valid after any single change to it, and a queue that stops answering for good
/// would silence every read of the session.
fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `host.ocr.read`'s two threads, as the event loop holds them.
pub struct Service<S: Send + 'static> {
    inner: Arc<Inner<S>>,
    done: Receiver<Done>,
    worker: OcrWorker<S>,
}

/// Stops the two threads at exit. Taken out of the manager before it is built, because the
/// manager is dropped inside `run`'s closure and nothing may rely on that drop.
#[derive(Clone)]
pub struct ShutdownHandle {
    stop: Arc<dyn Fn(Duration) + Send + Sync>,
}

impl ShutdownHandle {
    /// Asks both threads to stop and waits for them, for at most `bound`. A thread still
    /// inside a recognition then is logged and left to the process exit.
    pub fn shutdown(&self, bound: Duration) {
        (self.stop)(bound)
    }
}

impl<S: Send + 'static> Service<S> {
    /// Starts the capture and recognise threads over `worker`.
    pub fn spawn(worker: OcrWorker<S>) -> (Service<S>, ShutdownHandle) {
        Self::spawn_with(worker, Limits::POLICY)
    }

    fn spawn_with(worker: OcrWorker<S>, limits: Limits) -> (Service<S>, ShutdownHandle) {
        let inner = Arc::new(Inner {
            limits,
            state: Mutex::new(State { sched: Scheduler::new(), answered_at: None, hang_said: false }),
            capture_cv: Condvar::new(),
            recognise_cv: Condvar::new(),
            barrier_cv: Condvar::new(),
            stop: AtomicBool::new(false),
            preempt: AtomicBool::new(false),
            langs: Mutex::new(LangState::default()),
            langs_cv: Condvar::new(),
            threads: Mutex::new(Vec::new()),
        });
        let (tx, rx) = std::sync::mpsc::channel::<Done>();
        let mut threads = Vec::new();
        let c = inner.clone();
        match std::thread::Builder::new()
            .name("screen-capture".into())
            .spawn(move || capture_loop(c, worker))
        {
            Ok(h) => threads.push(h),
            Err(e) => logging::line("ocr", &format!("the screen-capture thread could not start: {e}")),
        }
        let r = inner.clone();
        match std::thread::Builder::new()
            .name("ocr-recognise".into())
            .spawn(move || recognise_loop(r, worker, tx))
        {
            Ok(h) => threads.push(h),
            Err(e) => logging::line("ocr", &format!("the ocr-recognise thread could not start: {e}")),
        }
        *locked(&inner.threads) = threads;
        let stopper = inner.clone();
        let handle = ShutdownHandle { stop: Arc::new(move |bound| shutdown(&stopper, bound)) };
        (Service { inner, done: rx, worker }, handle)
    }

    /// Queues a read. The capture thread is woken at once: the picture is taken now, not when
    /// the recogniser gets round to it.
    pub fn submit(&self, spec: Spec, ticket: Ticket) -> Submitted {
        let now = Instant::now();
        let mut st = locked(&self.inner.state);
        // A recognition that has answered no region for `HANG` refuses new work at once, with
        // the reason, rather than letting the queue fill behind it.
        if let Some(since) = st.answered_at {
            let for_s = now.saturating_duration_since(since);
            if for_s >= self.inner.limits.hang {
                if !st.hang_said {
                    st.hang_said = true;
                    logging::line(
                        "ocr",
                        &format!(
                            "the text recogniser has not answered a region for {} s; reads fail at \
                             once until it does",
                            for_s.as_secs()
                        ),
                    );
                }
                return Submitted {
                    refused: Some(format!(
                        "the text recogniser has not answered a region for {} s",
                        for_s.as_secs()
                    )),
                    ..Submitted::default()
                };
            }
        }
        let out = st.sched.submit(spec, ticket, now);
        self.inner.preempt.store(st.sched.interactive_waiting(), Ordering::Relaxed);
        drop(st);
        self.inner.capture_cv.notify_one();
        // A superseded or evicted job may have freed pictures from the budget, or a job to
        // recognise may have gone; the recogniser re-checks either way.
        self.inner.recognise_cv.notify_one();
        out
    }

    /// Finished jobs since the last call.
    pub fn drain(&self) -> Vec<Done> {
        self.done.try_iter().collect()
    }

    /// Makes `owner`'s waiting reads with `key` stale, as a newer read with that key does — for
    /// a newer read the host answers without queueing it. Returns the stale tickets.
    pub fn supersede(&self, owner: Owner, key: &str) -> Vec<TicketId> {
        let mut st = locked(&self.inner.state);
        let stale = st.sched.supersede(owner, key);
        self.inner.preempt.store(st.sched.interactive_waiting(), Ordering::Relaxed);
        drop(st);
        self.inner.barrier_cv.notify_all();
        // Dropped jobs may have freed pictures from the budget a background capture waits on,
        // or taken away the job the recogniser was about to start.
        self.inner.capture_cv.notify_one();
        self.inner.recognise_cv.notify_one();
        stale
    }

    /// Takes every ticket of module `idx` off its job; jobs nobody else waits for are dropped.
    pub fn cancel_owner(&self, idx: usize) -> Vec<TicketId> {
        let mut st = locked(&self.inner.state);
        let gone = st.sched.cancel_owner(idx);
        self.inner.preempt.store(st.sched.interactive_waiting(), Ordering::Relaxed);
        drop(st);
        self.inner.barrier_cv.notify_all();
        // Dropped jobs may have freed pictures from the budget a background capture waits on.
        self.inner.capture_cv.notify_one();
        gone
    }

    /// Waits until module `idx` has no picture left to be taken, for at most `bound`. True when
    /// it has none; false when the wait ran out. Its pictures go first meanwhile: the module is
    /// about to act, and a click that lands before the read's picture is what this is for.
    pub fn barrier(&self, idx: usize, bound: Duration) -> bool {
        let deadline = Instant::now() + bound;
        let mut st = locked(&self.inner.state);
        if st.sched.barrier_clear(idx) {
            return true;
        }
        if st.sched.expedite(idx) {
            // Woken now, it takes the lock the moment the wait below releases it.
            self.inner.capture_cv.notify_one();
        }
        loop {
            if st.sched.barrier_clear(idx) {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            st = self
                .inner
                .barrier_cv
                .wait_timeout(st, deadline - now)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    /// The languages the recognise thread published, waiting for them at most `bound` — they
    /// are its first act, so only the first moments after start ever wait.
    pub fn languages(&self, bound: Duration) -> Option<Languages> {
        if !self.worker.present {
            return Some(Languages::default());
        }
        let deadline = Instant::now() + bound;
        let mut l = locked(&self.inner.langs);
        loop {
            if let Some(known) = &l.known {
                return Some(known.clone());
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            l = self
                .inner
                .langs_cv
                .wait_timeout(l, deadline - now)
                .map(|(g, _)| g)
                .unwrap_or_else(|e| e.into_inner().0);
        }
    }

    /// A language did not resolve: the list is read again, at most once per `LANG_REREAD`, so a
    /// language pack installed while the application runs is picked up without a restart.
    pub fn reread_languages(&self) {
        let mut l = locked(&self.inner.langs);
        if l.read_at.is_some_and(|t| t.elapsed() >= LANG_REREAD) {
            l.again = true;
            drop(l);
            // Under the queue's lock, which the recognise thread checks the flag under: a wake
            // between its check and its wait would otherwise be lost until the next read.
            let _st = locked(&self.inner.state);
            self.inner.recognise_cv.notify_one();
        }
    }
}

fn shutdown<S>(inner: &Inner<S>, bound: Duration) {
    inner.stop.store(true, Ordering::SeqCst);
    {
        // Under the lock, so a thread between its check and its wait cannot miss the wake.
        let _st = locked(&inner.state);
        inner.capture_cv.notify_all();
        inner.recognise_cv.notify_all();
        inner.barrier_cv.notify_all();
    }
    let started = Instant::now();
    let deadline = started + bound;
    let threads = std::mem::take(&mut *locked(&inner.threads));
    for h in threads {
        let name = h.thread().name().unwrap_or("ocr").to_string();
        while !h.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if h.is_finished() {
            let _ = h.join();
        } else {
            logging::line(
                "ocr",
                &format!(
                    "the {name} thread was still working {} ms into the exit and is left to it",
                    started.elapsed().as_millis()
                ),
            );
        }
    }
}

// ── The capture thread ──────────────────────────────────────────────────────────────────────

fn capture_loop<S: Send + 'static>(inner: Arc<Inner<S>>, worker: OcrWorker<S>) {
    (worker.init_thread)(OcrThread::Capture);
    loop {
        let (id, spec) = {
            let mut st = locked(&inner.state);
            loop {
                if inner.stop.load(Ordering::SeqCst) {
                    return;
                }
                let over = st.sched.captured_bytes() > inner.limits.budget;
                if let Some(next) = st.sched.take_capture(over, Instant::now()) {
                    break next;
                }
                st = inner.capture_cv.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        let regions: Vec<(i32, i32, i32, i32)> = spec.regions.iter().map(Rect::tuple).collect();
        let (pixels, bytes) = if worker.present {
            match logging::contain(|| (worker.capture)(&regions, spec.source)) {
                Ok((shot, bytes)) => (Ok(shot), bytes),
                Err(report) => {
                    contained("a screen capture", &report);
                    (Err("the screen capture failed inside the application; the log has the details".to_string()), 0)
                }
            }
        } else {
            (Err(NO_RECOGNISER.to_string()), 0)
        };
        {
            let mut st = locked(&inner.state);
            // False when nobody waits any more (superseded while it was taken): the pixels go.
            let _kept = st.sched.captured(id, pixels, bytes, Instant::now());
            inner.preempt.store(st.sched.interactive_waiting(), Ordering::Relaxed);
        }
        inner.recognise_cv.notify_one();
        inner.barrier_cv.notify_all();
    }
}

// ── The recognise thread ────────────────────────────────────────────────────────────────────

enum Next<S> {
    Job(Started<Spec, Pixels<S>>),
    ReadLanguages,
}

fn read_languages<S>(inner: &Inner<S>, worker: &OcrWorker<S>) -> Languages {
    let langs = if worker.present {
        match logging::contain(worker.languages) {
            Ok(l) => l,
            Err(report) => {
                contained("listing the recognition languages", &report);
                Languages::default()
            }
        }
    } else {
        Languages::default()
    };
    let mut l = locked(&inner.langs);
    let first = l.known.is_none();
    if first || l.known.as_ref() != Some(&langs) {
        logging::line(
            "ocr",
            &format!(
                "recognition languages{}: {}; the user's: {}; a read without `lang` uses {}",
                if first { "" } else { " (read again)" },
                if langs.available.is_empty() { "none".to_string() } else { langs.available.join(", ") },
                if langs.preferred.is_empty() { "none".to_string() } else { langs.preferred.join(", ") },
                lang::default_tag(&langs).unwrap_or_else(|| "nothing".to_string())
            ),
        );
    }
    l.known = Some(langs.clone());
    l.read_at = Some(Instant::now());
    l.again = false;
    drop(l);
    inner.langs_cv.notify_all();
    langs
}

fn recognise_loop<S: Send + 'static>(inner: Arc<Inner<S>>, worker: OcrWorker<S>, done: Sender<Done>) {
    (worker.init_thread)(OcrThread::Recognise);
    // First, before any job: a module asking for the languages in its `activate` must not find
    // the list missing because a recognition got there first.
    let mut langs = read_languages(&inner, &worker);
    loop {
        let next = {
            let mut st = locked(&inner.state);
            loop {
                if inner.stop.load(Ordering::SeqCst) {
                    return;
                }
                if locked(&inner.langs).again {
                    break Next::ReadLanguages;
                }
                let now = Instant::now();
                if let Some(job) = st.sched.next_recognise(now) {
                    st.answered_at = Some(now);
                    inner.preempt.store(st.sched.interactive_waiting(), Ordering::Relaxed);
                    break Next::Job(job);
                }
                st = inner.recognise_cv.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        let job = match next {
            Next::ReadLanguages => {
                langs = read_languages(&inner, &worker);
                continue;
            }
            Next::Job(job) => job,
        };
        // Its pixels left the budget: a background capture may go ahead.
        inner.capture_cv.notify_one();

        let began = Instant::now();
        let preempt = (job.prio == Priority::Background).then_some(&inner.preempt);
        // Each region answered restarts the hang clock.
        let answered = || locked(&inner.state).answered(Instant::now(), true);
        // The whole job, not only the platform's recogniser: a panic in the language matching or
        // in the host's own normalisation would otherwise end this thread for the session, and
        // every read after it would wait for an answer that never comes.
        let (readings, lang_error) =
            match logging::contain(|| recognise_job(&worker, &job, &langs, preempt, &inner.stop, &answered)) {
                Ok(r) => r,
                Err(report) => {
                    contained("a recognition", &report);
                    (job.spec.regions.iter().map(|r| Reading::failed(*r, RECOGNISER_FAILED)).collect(), None)
                }
            };
        let finished = Instant::now();

        let tickets = {
            let mut st = locked(&inner.state);
            st.answered(finished, false);
            let t = st.sched.finished(job.id);
            inner.preempt.store(st.sched.interactive_waiting(), Ordering::Relaxed);
            t
        };
        if tickets.is_empty() {
            continue; // everybody who asked went away meanwhile
        }
        let ms = |a: Instant, b: Instant| b.saturating_duration_since(a).as_millis() as u64;
        let timings = Timings {
            before_capture: ms(job.asked, job.capture_started),
            capture: ms(job.capture_started, job.captured_at),
            before_recognition: ms(job.captured_at, began),
            recognition: ms(began, finished),
        };
        if done.send(Done { tickets, readings, lang_error, timings }).is_err() {
            return; // the event loop is gone
        }
    }
}

/// One job's readings, whatever happens: every region is answered. `stop` is the service's —
/// set, the regions not started yet are answered without being read — and `answered` is told
/// after each region.
fn recognise_job<S>(
    worker: &OcrWorker<S>,
    job: &Started<Spec, Pixels<S>>,
    langs: &Languages,
    preempt: Option<&AtomicBool>,
    stop: &AtomicBool,
    answered: &dyn Fn(),
) -> (Vec<Reading>, Option<(LangReq, String)>) {
    let regions = &job.spec.regions;
    let all_failed =
        |why: &str| regions.iter().map(|r| Reading::failed(*r, why)).collect::<Vec<_>>();
    if !worker.present {
        return (all_failed(NO_RECOGNISER), None);
    }
    let pixels = match &job.pixels {
        Ok(p) => p,
        Err(why) => return (all_failed(why), None),
    };
    let tag = match lang::resolve(&job.spec.lang, langs) {
        Ok(t) => t,
        Err(why) => return (all_failed(&why), Some((job.spec.lang.clone(), why))),
    };
    let tuples: Vec<(i32, i32, i32, i32)> = regions.iter().map(Rect::tuple).collect();
    let ctx = Recognise {
        lang: Some(&tag),
        fast_ok: lang::fast_reads(&tag, langs),
        preempt,
        stop: Some(stop),
        region_done: Some(answered),
    };
    let answers = match logging::contain(|| (worker.recognise)(pixels, &tuples, &ctx)) {
        Ok(a) => a,
        Err(report) => {
            // Nothing to rebuild: the recognisers keep no state of their own on this thread, and
            // the neural one's lock survives a panic (paddle_ocr.rs). The next job starts clean.
            contained("a recognition", &report);
            return (all_failed(RECOGNISER_FAILED), None);
        }
    };
    let mut answers = answers.into_iter();
    let readings = regions
        .iter()
        .map(|r| {
            let answer = answers.next();
            if r.is_empty() {
                let mut reading = Reading::failed(*r, "empty region: x2 must be greater than x1 and y2 greater than y1");
                reading.lang = tag.clone();
                return reading;
            }
            let out = match answer {
                Some(a) => engine_out(a),
                None => EngineOut::Failed("the recogniser gave no answer for this region".to_string()),
            };
            pipeline::normalise(*r, out, &tag)
        })
        .collect();
    (readings, None)
}

/// A backend answer as the pipeline takes it.
fn engine_out(answer: Result<OcrText, String>) -> EngineOut {
    let t = match answer {
        Err(e) => return EngineOut::Failed(e),
        Ok(t) => t,
    };
    if t.skipped {
        return EngineOut::Blank;
    }
    if let Some((x, y, w, h)) = t.fallback {
        return EngineOut::Fallback { text: t.text, content: Rect::new(x, y, w, h) };
    }
    let boxed = |w: &crate::backend::OcrWord| WordBox { text: w.text.clone(), x: w.x, y: w.y, w: w.w, h: w.h };
    let lines: Vec<EngineLine> = if !t.lines.is_empty() {
        t.lines
            .iter()
            .map(|l| EngineLine { text: l.text.clone(), words: l.words.iter().map(boxed).collect() })
            .collect()
    } else if !t.text.trim().is_empty() {
        // An engine that gave no lines: its text as one line, with whatever words it located.
        vec![EngineLine { text: t.text.clone(), words: t.words.iter().map(boxed).collect() }]
    } else {
        Vec::new()
    };
    EngineOut::Lines(lines)
}

/// Panics this session on the two threads, so the log gets the 1st, 2nd, 4th, 8th…
static CONTAINED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn contained(what: &str, report: &str) {
    let n = CONTAINED.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_power_of_two() {
        logging::line(
            "ocr",
            &format!(
                "{what} panicked on an OCR thread; the read was answered as failed and the thread \
                 carries on ({n} so far this session): {report}"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{OcrLine, OcrWord};
    use crate::ocr::sched::Owner;
    use crate::ocr::types::Status;
    use std::sync::atomic::AtomicU32;

    /// The fake's pixels: the regions it was asked to photograph.
    type Fake = Vec<(i32, i32, i32, i32)>;

    static CAPTURES: AtomicU32 = AtomicU32::new(0);

    fn fake_langs() -> Languages {
        Languages {
            available: vec!["en-US".into(), "de-DE".into()],
            fast: vec!["en-US".into()],
            preferred: vec!["de-AT".into()],
        }
    }

    /// The regions photographed in the barrier-order test, in the order they were taken.
    static ORDER: Mutex<Vec<i32>> = Mutex::new(Vec::new());

    /// Photographs at once, except a region at x = 555, which takes 200 ms; panics at x = 667;
    /// notes the order of x = 556, 557 and 5001.
    fn fake_capture(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> (Fake, usize) {
        CAPTURES.fetch_add(1, Ordering::SeqCst);
        if regions.iter().any(|r| r.0 == 667) {
            panic!("{}inside the fake capture", crate::EXPECTED_PANIC);
        }
        if regions.iter().any(|r| r.0 == 555) {
            std::thread::sleep(Duration::from_millis(200));
        }
        for r in regions.iter().filter(|r| matches!(r.0, 556 | 557 | 5001)) {
            locked(&ORDER).push(r.0);
        }
        (regions.to_vec(), 16)
    }

    fn line_of(text: String, word_x: i32) -> OcrText {
        let word = OcrWord { text: text.clone(), x: word_x, y: 0, w: 10, h: 10 };
        OcrText {
            text: text.clone(),
            words: vec![word.clone()],
            lines: vec![OcrLine { text, words: vec![word] }],
            fallback: None,
            skipped: false,
        }
    }

    /// Reads "x,y lang" for every region, one at a time through `Recognise::each` as the platform
    /// recognisers do. Slowly for x = 999 (60 ms); for y = 77, 100 ms; for y = 78, 400 ms; for
    /// y = 81, 50 ms. Panics at x = 666. At y = 79 it says whether it may be preempted; at y = 80
    /// it answers a word no screen has (x = i32::MAX), which the host's own arithmetic cannot
    /// place.
    fn fake_recognise(shot: &Fake, _: &[(i32, i32, i32, i32)], ctx: &Recognise) -> Vec<Result<OcrText, String>> {
        ctx.each(shot.iter(), |&(x, y, _, _)| {
            if x == 666 {
                panic!("{}inside the fake recogniser", crate::EXPECTED_PANIC);
            }
            let pause = match (x, y) {
                (999, _) => 60,
                (_, 77) => 100,
                (_, 78) => 400,
                (_, 81) => 50,
                _ => 0,
            };
            std::thread::sleep(Duration::from_millis(pause));
            if x == 7 {
                return Ok(OcrText {
                    text: "7".into(),
                    words: vec![],
                    lines: vec![],
                    fallback: Some((1, 1, 5, 8)),
                    skipped: false,
                });
            }
            match y {
                79 => Ok(line_of(format!("preempt={}", ctx.preempt.is_some()), 0)),
                80 => Ok(line_of("far".into(), i32::MAX)),
                _ => Ok(line_of(format!("{x},{y} {}", ctx.lang.unwrap_or("?")), 0)),
            }
        })
    }

    fn worker() -> OcrWorker<Fake> {
        OcrWorker {
            present: true,
            init_thread: |_| {},
            capture: fake_capture,
            recognise: fake_recognise,
            languages: fake_langs,
        }
    }

    const A: Owner = Owner { idx: 1, gen: 5 };

    fn spec(x: i32) -> Spec {
        Spec { regions: vec![Rect::new(x, 2, 30, 10)], lang: LangReq::Default, source: CaptureSource::Standard }
    }

    fn ticket(id: TicketId, key: Option<&str>) -> Ticket {
        Ticket { id, owner: A, key: key.map(str::to_string), prio: Priority::Background }
    }

    /// Everything delivered until `n` tickets have been answered or the time runs out.
    fn collect(s: &Service<Fake>, n: usize) -> Vec<Done> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut out: Vec<Done> = Vec::new();
        while out.iter().map(|d| d.tickets.len()).sum::<usize>() < n && Instant::now() < deadline {
            out.extend(s.drain());
            std::thread::sleep(Duration::from_millis(2));
        }
        out
    }

    #[test]
    fn a_read_is_captured_recognised_and_delivered_exactly_once() {
        let (s, stop) = Service::spawn(worker());
        let out = s.submit(spec(10), ticket(1, None));
        assert!(out.refused.is_none());
        let done = collect(&s, 1);
        assert_eq!(done.len(), 1);
        let r = &done[0].readings[0];
        assert_eq!(r.status, Status::Text);
        assert_eq!(r.text, "10,2 de-DE", "the user's language, resolved to the engine's tag");
        assert_eq!(r.lang, "de-DE");
        assert_eq!(r.words[0].x, 10, "absolute");
        std::thread::sleep(Duration::from_millis(30));
        assert!(s.drain().is_empty(), "exactly once");
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn a_panic_is_answered_as_failed_and_the_thread_lives_on() {
        crate::quiet_expected_panics();
        let (s, stop) = Service::spawn(worker());
        s.submit(spec(666), ticket(1, None));
        let first = collect(&s, 1);
        assert_eq!(first[0].readings[0].status, Status::Failed);
        s.submit(spec(20), ticket(2, None));
        let second = collect(&s, 1);
        assert_eq!(second[0].readings[0].status, Status::Text, "the recogniser still answers");
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn an_unavailable_language_fails_with_what_is_there() {
        let (s, stop) = Service::spawn(worker());
        let mut sp = spec(10);
        sp.lang = LangReq::Tags(vec!["ja".into()]);
        s.submit(sp, ticket(1, None));
        let done = collect(&s, 1);
        let r = &done[0].readings[0];
        assert_eq!(r.status, Status::Failed);
        assert!(r.error.as_deref().unwrap().starts_with("language: ja is not available here (available: en-US, de-DE)"));
        assert!(done[0].lang_error.is_some());
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn the_fallback_and_an_empty_region_come_back_in_the_readings_shape() {
        let (s, stop) = Service::spawn(worker());
        let sp = Spec {
            regions: vec![Rect::new(7, 0, 20, 10), Rect::new(50, 50, 0, 10)],
            lang: LangReq::Tags(vec!["en".into()]),
            source: CaptureSource::Standard,
        };
        s.submit(sp, ticket(1, None));
        let done = collect(&s, 1);
        let r = &done[0].readings;
        assert_eq!(r[0].text, "7");
        assert!(r[0].words[0].approx);
        assert_eq!((r[0].words[0].x, r[0].words[0].y), (8, 1));
        assert_eq!(r[0].lang, "en-US");
        assert_eq!(r[1].status, Status::Failed);
        assert!(r[1].error.as_deref().unwrap().starts_with("empty region"));
        stop.shutdown(Duration::from_secs(2));
    }

    /// The languages are published before any job runs, and a caller waits only briefly.
    #[test]
    fn the_language_list_is_the_first_thing_published() {
        let (s, stop) = Service::spawn(worker());
        let l = s.languages(Duration::from_secs(5)).expect("published");
        assert_eq!(l, fake_langs());
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn the_barrier_returns_once_the_picture_is_taken() {
        let (s, stop) = Service::spawn(worker());
        assert!(s.barrier(A.idx, Duration::ZERO), "nothing asked, nothing to wait for");
        s.submit(spec(999), ticket(1, None));
        let t = Instant::now();
        assert!(s.barrier(A.idx, Duration::from_secs(5)));
        assert!(t.elapsed() < Duration::from_secs(5));
        // The recognition (60 ms) is still running: the barrier did not wait for it.
        assert!(s.drain().is_empty());
        collect(&s, 1);
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn the_barrier_gives_up_at_its_bound() {
        let (s, stop) = Service::spawn(worker());
        s.submit(spec(555), ticket(1, None));
        let t = Instant::now();
        assert!(!s.barrier(A.idx, Duration::from_millis(50)), "the picture takes 200 ms");
        let waited = t.elapsed();
        assert!(waited >= Duration::from_millis(50) && waited < Duration::from_millis(190), "{waited:?}");
        collect(&s, 1);
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn a_disabled_modules_tickets_are_dropped_and_nothing_is_delivered() {
        let (s, stop) = Service::spawn(worker());
        // A slow job holds the recogniser so the next one is still queued when it is cancelled.
        s.submit(spec(999), Ticket { id: 1, owner: Owner { idx: 9, gen: 1 }, key: None, prio: Priority::Background });
        s.submit(spec(30), ticket(2, None));
        let gone = s.cancel_owner(A.idx);
        let done = collect(&s, 1);
        let answered: Vec<TicketId> = done.iter().flat_map(|d| d.tickets.iter().map(|t| t.id)).collect();
        if gone.contains(&2) {
            assert_eq!(answered, vec![1]);
        }
        std::thread::sleep(Duration::from_millis(100));
        assert!(s.drain().iter().all(|d| d.tickets.iter().all(|t| t.owner != A)));
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn a_keyed_poll_slower_than_the_recogniser_keeps_delivering() {
        let (s, stop) = Service::spawn(worker());
        let mut stale = 0;
        for i in 0..8u64 {
            let out = s.submit(spec(999), ticket(i + 1, Some("menu")));
            stale += out.stale.len();
            std::thread::sleep(Duration::from_millis(45));
        }
        let done = collect(&s, 8 - stale);
        let delivered: usize = done.iter().map(|d| d.tickets.len()).sum();
        assert_eq!(delivered + stale, 8, "every read is either delivered or stale");
        assert!(delivered >= 4, "delivered {delivered}, stale {stale}");
        assert!(done.iter().all(|d| d.readings[0].status == Status::Text));
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn shutdown_joins_both_threads_within_its_bound() {
        let (s, stop) = Service::spawn(worker());
        s.submit(spec(10), ticket(1, None));
        let t = Instant::now();
        stop.shutdown(Duration::from_secs(2));
        assert!(t.elapsed() < Duration::from_secs(2));
        assert!(locked(&s.inner.threads).is_empty());
    }

    fn spec_at(x: i32, y: i32) -> Spec {
        Spec { regions: vec![Rect::new(x, y, 30, 10)], lang: LangReq::Default, source: CaptureSource::Standard }
    }

    fn of(owner: Owner, id: TicketId, prio: Priority) -> Ticket {
        Ticket { id, owner, key: None, prio }
    }

    /// Five regions at 100 ms each against a hang bound of 150 ms: a long job that keeps
    /// answering is not a hang. One region of 400 ms is, and the refusal says so; the first read
    /// after it is taken again.
    #[test]
    fn a_hang_is_a_region_that_does_not_come_back_not_a_long_job() {
        let (s, stop) = Service::spawn_with(worker(), Limits { hang: Duration::from_millis(150), ..Limits::POLICY });
        let other = Owner { idx: 3, gen: 1 };
        let regions = (0..5).map(|i| Rect::new(100 + i * 40, 77, 30, 10)).collect();
        s.submit(Spec { regions, lang: LangReq::Default, source: CaptureSource::Standard }, ticket(1, None));
        std::thread::sleep(Duration::from_millis(330));
        let out = s.submit(spec_at(40, 2), of(other, 2, Priority::Interactive));
        assert!(out.refused.is_none(), "330 ms into a job whose regions answer every 100 ms: {:?}", out.refused);
        collect(&s, 2);

        s.submit(spec_at(10, 78), ticket(3, None));
        std::thread::sleep(Duration::from_millis(250));
        let out = s.submit(spec_at(41, 2), of(other, 4, Priority::Interactive));
        let why = out.refused.expect("250 ms without a region answered, against a bound of 150");
        assert!(why.starts_with("the text recogniser has not answered a region for"), "{why}");
        collect(&s, 1);
        assert!(s.submit(spec_at(42, 2), of(other, 5, Priority::Interactive)).refused.is_none(), "over");
        collect(&s, 1);
        stop.shutdown(Duration::from_secs(2));
    }

    /// Pictures of 16 bytes against a budget of 10: while one waits for the recogniser, the next
    /// background capture waits too — here for the 60 ms recognition in front of it.
    #[test]
    fn background_captures_wait_while_the_picture_budget_is_exceeded() {
        let (s, stop) = Service::spawn_with(worker(), Limits { budget: 10, ..Limits::POLICY });
        s.submit(spec(999), ticket(1, None));
        s.submit(spec(31), ticket(2, None));
        s.submit(spec(32), ticket(3, None));
        let done = collect(&s, 3);
        let waited = |x: i32| {
            done.iter().find(|d| d.readings[0].rect.x == x).map(|d| d.timings.before_capture).unwrap()
        };
        assert!(waited(32) >= 30, "the third picture waited {} ms", waited(32));
        stop.shutdown(Duration::from_secs(2));
    }

    /// Only a background recognition is handed the preempt flag; an interactive one runs every
    /// pass it has.
    #[test]
    fn only_a_background_read_may_skip_its_last_passes() {
        let (s, stop) = Service::spawn(worker());
        s.submit(spec_at(10, 79), Ticket { prio: Priority::Interactive, ..ticket(1, None) });
        assert_eq!(collect(&s, 1)[0].readings[0].text, "preempt=false");
        s.submit(spec_at(11, 79), ticket(2, None));
        assert_eq!(collect(&s, 1)[0].readings[0].text, "preempt=true");
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn a_capture_that_panics_is_answered_as_failed_and_the_thread_lives_on() {
        crate::quiet_expected_panics();
        let (s, stop) = Service::spawn(worker());
        s.submit(spec(667), ticket(1, None));
        let first = collect(&s, 1);
        assert_eq!(first[0].readings[0].status, Status::Failed);
        assert!(first[0].readings[0].error.as_deref().unwrap().starts_with("the screen capture failed inside"));
        s.submit(spec(21), ticket(2, None));
        assert_eq!(collect(&s, 1)[0].readings[0].status, Status::Text, "the capture thread still answers");
        stop.shutdown(Duration::from_secs(2));
    }

    /// A panic outside the platform's recogniser — here the host's own arithmetic on a word no
    /// screen has, which overflows in a debug build — is answered, and the thread lives on.
    #[test]
    fn a_panic_in_the_hosts_own_share_of_a_recognition_is_contained() {
        crate::quiet_expected_panics();
        let (s, stop) = Service::spawn(worker());
        s.submit(spec_at(10, 80), ticket(1, None));
        let first = collect(&s, 1);
        assert_eq!(first.len(), 1, "answered");
        if cfg!(debug_assertions) {
            assert_eq!(first[0].readings[0].error.as_deref(), Some(RECOGNISER_FAILED));
        }
        s.submit(spec(22), ticket(2, None));
        assert_eq!(collect(&s, 1)[0].readings[0].status, Status::Text, "the recognise thread still answers");
        stop.shutdown(Duration::from_secs(2));
    }

    /// Twenty regions at 50 ms each: the exit comes after the second, and the job ends with the
    /// region it is on — the rest are answered without being read, and no recognition starts
    /// after the exit.
    #[test]
    fn the_exit_stops_a_job_between_regions() {
        let (s, stop) = Service::spawn(worker());
        let regions = (0..20).map(|i| Rect::new(100 + i * 40, 81, 30, 10)).collect();
        s.submit(Spec { regions, lang: LangReq::Default, source: CaptureSource::Standard }, ticket(1, None));
        std::thread::sleep(Duration::from_millis(120));
        let t = Instant::now();
        stop.shutdown(Duration::from_secs(2));
        let took = t.elapsed();
        assert!(took < Duration::from_millis(400), "the exit waited {took:?} for a job it could have stopped");
        let done = s.drain();
        let r = &done[0].readings;
        assert!(r.iter().filter(|r| r.status == Status::Text).count() < 8);
        assert_eq!(r.last().unwrap().error.as_deref(), Some(crate::backend::CLOSING));
    }

    /// A module about to act has its picture taken before reads other modules asked for
    /// earlier — interactive ones included.
    #[test]
    fn the_barrier_puts_its_modules_picture_first() {
        let (s, stop) = Service::spawn(worker());
        let b = Owner { idx: 2, gen: 1 };
        s.submit(spec(555), of(b, 1, Priority::Interactive)); // holds the capture thread 200 ms
        std::thread::sleep(Duration::from_millis(20));
        s.submit(spec(556), of(b, 2, Priority::Interactive));
        s.submit(spec(557), of(b, 3, Priority::Interactive));
        s.submit(spec(5001), ticket(4, None));
        assert!(s.barrier(A.idx, Duration::from_secs(2)));
        collect(&s, 4);
        let order = locked(&ORDER).clone();
        assert_eq!(order.first(), Some(&5001), "{order:?}");
        stop.shutdown(Duration::from_secs(2));
    }

    #[test]
    fn a_platform_without_a_recogniser_fails_every_read_and_lists_nothing() {
        let mut w = worker();
        w.present = false;
        let (s, stop) = Service::spawn(w);
        assert_eq!(s.languages(Duration::ZERO), Some(Languages::default()));
        s.submit(spec(10), ticket(1, None));
        let done = collect(&s, 1);
        assert_eq!(done[0].readings[0].error.as_deref(), Some(NO_RECOGNISER));
        stop.shutdown(Duration::from_secs(2));
    }
}
