//! `host.element`'s questions, and the thread that asks them.
//!
//! **Off the event loop.** A question to an accessibility tree is synchronous IPC into another
//! program, answered on that program's own thread when it gets to it — and a DAW building a file
//! dialog does not get to it for a second or two (Kontakt's Load and Save in REAPER, TODO.md). Asked
//! from the event loop, it held every key, every timer and every message to this application's own
//! windows for that long, a screen reader's questions about them included: each of NVDA's freeze
//! recoveries of 2026-10-10 over a second lined up with one of ours. So on a platform that has one
//! ([`ElementRunner`], Windows) every question is asked on a thread of the host's own,
//! `accessibility`, started at the first question. It has no window, and on Windows it lives in
//! the multithreaded COM apartment, as Microsoft recommends for a UI Automation client; the
//! automation object and its timeouts are its own (`backend/uia.rs`).
//!
//! **One question at a time, and none twice.** The thread asks one question at a time, so no
//! program ever has more than one of ours to answer. The question the event loop waits for goes
//! first, then those asked from what a person did, then a poll's ([`Lane`]) — but never a question
//! before one that acts on the same program and was put before it (`press`, `focusStep`,
//! `focusWithin`): what it finds would be from before that act. Such a question goes as early as the
//! most urgent one that waits behind it. A question the same as one that waits — the same call with
//! the same arguments, whichever module asked — is not put again: its askers share the one answer,
//! unless something acted on the screen between the two (`host.inputEpoch` turned over, or a question
//! that acts on the same program waits between them). One the same as the question being asked right
//! now shares that answer as well, unless the input epoch turned over since that question was put:
//! what it finds may be from before. A question that acts is never shared.
//!
//! **Not known is not "no".** A question the program did not answer — UI Automation gave up on it
//! after `UIA_TIMEOUT_MS`, the program runs with higher rights, it went away — is answered "not
//! known", with the reason ([`ElementFailure`]): a module is told `nil` and why, and nothing here
//! turns it into "not there". And the questions to the same program that waited behind one it did
//! not answer in time are answered "not known" too, without being put to it ([`Queue::finish`]): a
//! program that does not answer is not sent the next question while it is still busy with the last
//! — what sent full-tree searches into REAPER one after another, 5.4 s in all, on 2026-10-10. A
//! question asked after that answer is put to it again. No clock decides any of this: a question
//! is answered, or it waited behind one that was not. What the host tells an asker without putting
//! the question to the program — not asked behind one not answered, replaced by a newer key, asked
//! inside a wait, the thread not started, a window of this application — is no answer of the
//! program's, and the epoch's store never keeps it ([`Due::program`], [`Waited::program`]).
//!
//! **This application's own windows** are not asked about on the thread: their accessibility is
//! answered on the event loop, partly by what is *posted* to it (a COM call into its apartment),
//! which the event loop's wait for an answer does not run ([`Signal`]) — the thread would wait for
//! the event loop and the event loop for the thread. Every question about one is answered
//! [`OWN_WINDOW`].
//!
//! **Who waits.** A module that hands over a callback: nobody. The answer comes on a later turn of
//! the event loop, as a handler of the module, through its mailbox, in the lane it was asked from
//! ([`Elements::take_due`], `lib.rs`), and not at all when the module was disabled, reloaded or
//! stopped meanwhile — and a question of such a module that was not asked yet is not asked at all
//! ([`Elements::drop_owner`]): a press of a module that is gone is not made. A module that does
//! not: the event loop waits for the answer, as it waited for the call it made itself before — but
//! in a wait that answers what other threads and programs *send* to this thread's windows
//! ([`Signal`]), so a screen reader asking one of them what it holds (`WM_GETOBJECT`, and the
//! messages Windows' own proxies send to read a control) is answered while the event loop waits.
//! What that lets run, and what it does not, is set out at [`Signal`]. Until the thread takes the
//! question, the wait is the questions' put before it, often another module's: the guard's wall
//! clock for the callback stands meanwhile (`vm_guard`, `Guard::stop_wall`), and the log line for a
//! long wait says how much of it that was.
//!
//! **Where there is no such thread** (macOS, the stub backend) a question is asked on the event
//! loop as it always was ([`Backend::element_ask`]), and a callback still gets its answer on a later
//! turn, never inside the call.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::backend::{Backend, ElementAnswer, ElementAsk, ElementFailure, ElementOutcome, ElementRunner};
use crate::logging;
use crate::ocr::sched::Owner;
use crate::ocr::types::{current_priority, Priority};
use crate::vm_guard::Guard;

/// The thread's name.
const THREAD: &str = "accessibility";

/// A question that took at least this long on the thread gets a `[uia]` line: what it asked, of
/// which window and program, how long, and what came of it. The event loop's own line for a wait
/// that long is element_lua.rs's `blocked_line`, at the same mark.
pub(crate) const SLOW: Duration = Duration::from_millis(50);

/// What a module is told for a question the thread did not ask because it waited behind one the
/// same program did not answer in time; `{}` is the reason that one got.
fn not_asked(why: &str) -> String {
    format!("{why} to an earlier question, so this one was not asked")
}

/// What a module is told for a question replaced, before it was asked, by a newer one of the same
/// module with the same `opts.key`.
pub(crate) const REPLACED: &str = "a newer question with the same key was asked before this one";

/// What a question asked inside another one's wait is told: the event loop is already waiting.
pub(crate) const NESTED: &str = "another accessibility question is being waited for";

/// What a question about a window of this application itself is told (see the module's docs).
pub(crate) const OWN_WINDOW: &str = "the window is this application's own, which is not asked about";

/// Which questions the thread asks first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Lane {
    /// The event loop waits for it.
    Waited = 0,
    /// Asked from what a person did: a key, a hotkey, a window or focus change, or what one of
    /// those asked for (`ocr/types.rs`, `Priority::Interactive`).
    Interactive = 1,
    /// A poll's, or asked while a module loads.
    Background = 2,
}

impl Lane {
    fn of(p: Priority) -> Lane {
        match p {
            Priority::Interactive => Lane::Interactive,
            Priority::Background => Lane::Background,
        }
    }
}

/// Where one asker's answer goes.
enum To {
    /// The event loop, waiting for it ([`Elements::ask`]).
    Wait(Arc<Back>),
    /// A module's callback, by its token on the event loop's side; with the module VM that asked
    /// and the `opts.key` it was asked under, which a newer question of that VM with that key
    /// replaces while it waits.
    Call { token: u64, owner: Owner, key: Option<String> },
}

/// A question waiting to be asked, with everyone who waits for its answer.
struct Job {
    seq: u64,
    ask: ElementAsk,
    pid: u32,
    lane: Lane,
    input_epoch: u64,
    to: Vec<To>,
}

/// The question being asked now.
struct Running {
    ask: ElementAsk,
    pid: u32,
    input_epoch: u64,
    to: Vec<To>,
}

/// The thread's work, behind one lock: what waits, what is being asked, and whether to stop.
#[derive(Default)]
struct Queue {
    jobs: Vec<Job>,
    running: Option<Running>,
    seq: u64,
    closed: bool,
}

/// How [`Queue::submit`] put a question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Put {
    /// As a question of its own, to be asked.
    New,
    /// Beside an identical question that waits: one answer for both.
    Waiting,
    /// Beside the identical question being asked now: that one's answer.
    Running,
}

impl Queue {
    /// Puts `ask` — about a window of process `pid`, asked in `lane` while the input epoch was
    /// `input_epoch` — in, for `to`: beside an identical question that waits, put in the same input
    /// epoch with no question that acts on the same program waiting after it; else beside the one
    /// being asked, put in the same input epoch; else as a question of its own. A question that acts
    /// is always one of its own.
    fn submit(&mut self, ask: ElementAsk, pid: u32, lane: Lane, input_epoch: u64, to: To) -> Put {
        if !ask.acts() {
            let same = |j: &Job| j.ask == ask && j.input_epoch == input_epoch && !self.acts_after(j.pid, j.seq);
            if let Some(i) = self.jobs.iter().position(same) {
                let job = &mut self.jobs[i];
                job.to.push(to);
                job.lane = job.lane.min(lane);
                return Put::Waiting;
            }
            if let Some(r) = self.running.as_mut().filter(|r| r.ask == ask && r.input_epoch == input_epoch) {
                r.to.push(to);
                return Put::Running;
            }
        }
        self.seq += 1;
        self.jobs.push(Job { seq: self.seq, ask, pid, lane, input_epoch, to: vec![to] });
        Put::New
    }

    /// Whether a question that acts on program `pid` waits, put after the one put as `seq`.
    fn acts_after(&self, pid: u32, seq: u64) -> bool {
        self.jobs.iter().any(|b| b.pid == pid && b.seq > seq && b.ask.acts())
    }

    /// The next question to ask — the first of the most urgent lane, a question that acts counting
    /// as urgent as the most urgent question of its program put after it — now the one being asked:
    /// what it is and whose program answers it. That one rule keeps every question after an act of
    /// its program put before it: the act is at least as urgent as each of them, and was put first.
    /// The event loop's waits in it are told it is taken ([`Back::taken`]).
    fn next(&mut self) -> Option<(ElementAsk, u32)> {
        let jobs = &self.jobs;
        let urgency = |j: &Job| {
            if !j.ask.acts() {
                return j.lane;
            }
            jobs.iter().filter(|w| w.pid == j.pid && w.seq > j.seq).map(|w| w.lane).fold(j.lane, |a, b| a.min(b))
        };
        let i = jobs.iter().enumerate().min_by_key(|(_, j)| (urgency(j), j.seq)).map(|(i, _)| i)?;
        let job = self.jobs.remove(i);
        for t in &job.to {
            if let To::Wait(back) = t {
                back.taken();
            }
        }
        let picked = (job.ask.clone(), job.pid);
        self.running = Some(Running { ask: job.ask, pid: job.pid, input_epoch: job.input_epoch, to: job.to });
        Some(picked)
    }

    /// The question being asked is answered, `unanswered` when its program did not answer in time:
    /// who waits for it, and — then — the questions to the same program that waited behind it,
    /// taken out, to be told so without being asked.
    fn finish(&mut self, unanswered: bool) -> (Vec<To>, Vec<Job>) {
        let Some(r) = self.running.take() else { return (Vec::new(), Vec::new()) };
        let behind = if unanswered && r.pid != 0 {
            let (same, rest): (Vec<Job>, Vec<Job>) = std::mem::take(&mut self.jobs).into_iter().partition(|j| j.pid == r.pid);
            self.jobs = rest;
            same
        } else {
            Vec::new()
        };
        (r.to, behind)
    }

    /// Takes the callbacks `which` picks — by the module VM that asked and its key — out of the
    /// questions not asked yet, and a question nobody waits for any more goes with them: their
    /// tokens. The question being asked is answered as it is.
    fn take_calls(&mut self, which: impl Fn(Owner, Option<&str>) -> bool) -> Vec<u64> {
        let mut taken = Vec::new();
        for job in &mut self.jobs {
            job.to.retain(|t| match t {
                To::Call { token, owner, key } if which(*owner, key.as_deref()) => {
                    taken.push(*token);
                    false
                }
                _ => true,
            });
        }
        self.jobs.retain(|j| !j.to.is_empty());
        taken
    }
}

/// The work and the bell the thread waits on.
#[derive(Default)]
struct Inner {
    queue: Mutex<Queue>,
    work: Condvar,
}

/// The queue, whatever a panic elsewhere left in it: every step under the lock is a plain move of
/// data, and the thread must go on answering.
fn lock(m: &Mutex<Queue>) -> MutexGuard<'_, Queue> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The event loop's way of waiting for an answer: the answer once it is there, and whether it is
/// the program's ([`Waited::program`]); the [`Signal`] the thread gives when it is; and when the
/// thread took the question — the end of the wait behind the questions put before it, when the
/// guard's wall clock goes on.
struct Back {
    answer: Mutex<Option<(ElementOutcome, bool)>>,
    signal: Signal,
    taken: Mutex<Option<Instant>>,
    guard: Option<Arc<Guard>>,
}

impl Back {
    fn new(signal: Signal, guard: Option<Arc<Guard>>) -> Back {
        Back { answer: Mutex::new(None), signal, taken: Mutex::new(None), guard }
    }

    fn give(&self, outcome: ElementOutcome, program: bool) {
        *self.answer.lock().unwrap_or_else(PoisonError::into_inner) = Some((outcome, program));
        self.signal.set();
    }

    fn take(&self) -> Option<(ElementOutcome, bool)> {
        self.answer.lock().unwrap_or_else(PoisonError::into_inner).take()
    }

    fn answered(&self) -> bool {
        self.answer.lock().unwrap_or_else(PoisonError::into_inner).is_some()
    }

    /// The thread takes the question: noted, and the callback's wall clock goes on.
    fn taken(&self) {
        *self.taken.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        if let Some(g) = &self.guard {
            g.go_on();
        }
    }

    fn taken_at(&self) -> Option<Instant> {
        *self.taken.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// An answer on its way to a callback: the callback's token, the outcome, and whether the program
/// gave it ([`Due::program`]).
type Done = (u64, ElementOutcome, bool);

/// Hands `outcome` — the program's, or the host's without asking it — to everyone in `to`: a
/// waiting event loop through its [`Back`], a callback through the channel the event loop drains.
fn answer(to: Vec<To>, outcome: &ElementOutcome, program: bool, done: &Sender<Done>) {
    for t in to {
        match t {
            To::Wait(back) => back.give(outcome.clone(), program),
            To::Call { token, .. } => {
                let _ = done.send((token, outcome.clone(), program));
            }
        }
    }
}

/// The `[uia]` line for a question that took [`SLOW`] or longer on the thread.
fn slow_line(ask: &ElementAsk, pid: u32, took: Duration, outcome: &ElementOutcome) -> String {
    let came = match outcome {
        Ok(ElementAnswer::Nodes(n, Some(why))) => format!("{} element(s), and then {why}", n.len()),
        Ok(a) if a.is_nothing() => "nothing found".to_string(),
        Ok(_) => "answered".to_string(),
        Err(f) => format!("not known: {}", f.why),
    };
    format!(
        "{}({}) of window {} (process {pid}) took {} ms on the accessibility thread: {came}",
        ask.call_name(),
        ask.detail(),
        ask.hwnd(),
        took.as_millis()
    )
}

/// The thread: asks what waits, one question at a time, until the host lets it go. Each question
/// runs inside `logging::contain`, so one that panics is answered "not known" with the panic's
/// report, which is in the log, and the thread goes on.
fn serve(inner: Arc<Inner>, runner: ElementRunner, done: Sender<Done>) {
    (runner.init)();
    loop {
        let (ask, pid) = {
            let mut q = lock(&inner.queue);
            loop {
                if q.closed {
                    return;
                }
                if let Some(next) = q.next() {
                    break next;
                }
                q = inner.work.wait(q).unwrap_or_else(PoisonError::into_inner);
            }
        };
        let t = Instant::now();
        let ran = logging::contain(|| (runner.run)(&ask));
        // A panic's report is the host's, not the program's answer.
        let program = ran.is_ok();
        let outcome = ran.unwrap_or_else(|p| {
            Err(ElementFailure { why: format!("the host's accessibility thread failed: {p}"), unanswered: false })
        });
        let took = t.elapsed();
        if took >= SLOW {
            logging::line("uia", &slow_line(&ask, pid, took, &outcome));
        }
        let unanswered = matches!(&outcome, Err(f) if f.unanswered);
        let (to, behind) = lock(&inner.queue).finish(unanswered);
        answer(to, &outcome, program, &done);
        if !behind.is_empty() {
            let why = outcome.as_ref().err().map(|f| f.why.clone()).unwrap_or_default();
            logging::line(
                "uia",
                &format!(
                    "{} question(s) to process {pid} that waited behind it are answered \"not known\" without being asked",
                    behind.len()
                ),
            );
            let told: ElementOutcome = Err(ElementFailure { why: not_asked(&why), unanswered: true });
            for job in behind {
                answer(job.to, &told, false, &done);
            }
        }
    }
}

/// A callback's question, kept on the event loop until its answer comes.
pub(crate) struct Pending {
    /// The module, its VM and its callback.
    pub answer: crate::acting::Answer,
    /// The lane of the dispatch that asked, which the answer runs in.
    pub prio: Priority,
    pub ask: ElementAsk,
    /// The epoch and the input epoch it was asked in: an answer of the program's that comes while
    /// both still stand is kept for the epoch, as one the event loop waited for is.
    pub asked_in: (u64, u64),
    /// The module VM that asked: a question of a VM disabled, reloaded or stopped is dropped
    /// ([`Elements::drop_owner`]).
    pub owner: Owner,
    /// Its `opts.key`: `host.element.pending(key)` is true from the call until the answer runs or is
    /// dropped ([`Elements::pending_with`]).
    pub key: Option<String>,
}

impl Pending {
    /// What `host.element.pending` counts it under: its VM and its key, if it has one.
    pub(crate) fn out(&self) -> Option<(Owner, String)> {
        self.key.clone().map(|k| (self.owner, k))
    }
}

/// An answer that came, for its callback.
pub(crate) struct Due {
    pub pending: Pending,
    pub outcome: ElementOutcome,
    /// The program gave it — the thread's runner, or the backend on the event loop, answered it —
    /// rather than the host without putting the question to it (not asked behind one not answered,
    /// replaced, the thread not started, a window of this application, a panic) or the epoch's
    /// store at the call. Only the program's answer is kept for the epoch.
    pub program: bool,
}

/// What the event loop's wait for an answer came to ([`Elements::ask`]).
pub(crate) struct Waited {
    pub outcome: ElementOutcome,
    /// The program gave it ([`Due::program`]).
    pub program: bool,
    /// How much of the wait went by before the thread took the question: behind questions put
    /// before it — or all of it, when it was answered without being put to the program.
    pub behind: Duration,
}

/// How far the thread got: not started yet, running, or not startable, with why.
enum Started {
    No,
    Yes,
    Failed(String),
}

/// The questions on their way and the answers coming back. Event loop only, like the rest of
/// `Shared`; the thread holds only its end of the queue and of the channel.
pub(crate) struct Elements {
    inner: Arc<Inner>,
    started: RefCell<Started>,
    next: Cell<u64>,
    pending: RefCell<HashMap<u64, Pending>>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    /// The event loop is waiting for an answer ([`Elements::ask`]): a question asked inside that
    /// wait is refused, not waited for in a second wait.
    waiting: Cell<bool>,
    /// By module VM and key, how many questions asked with a callback are not answered to it yet,
    /// nor dropped: waiting here, on their way back, or in the module's mailbox
    /// (`host.element.pending`). Counted down as the answer's event runs or is dropped
    /// ([`Elements::settle`]), or as the question is dropped with its module.
    out: RefCell<HashMap<(Owner, String), u32>>,
}

impl Elements {
    pub(crate) fn new() -> Elements {
        let (done_tx, done_rx) = channel();
        Elements {
            inner: Arc::new(Inner::default()),
            started: RefCell::new(Started::No),
            next: Cell::new(0),
            pending: RefCell::new(HashMap::new()),
            done_tx,
            done_rx,
            waiting: Cell::new(false),
            out: RefCell::new(HashMap::new()),
        }
    }

    /// Asks `ask` and waits for the answer: on the thread, where the platform has one, else on the
    /// event loop itself (`backend.element_ask`). `input_epoch` is `host.inputEpoch()` now; `guard`,
    /// the guard whose wall clock stands while the wait is behind questions put before this one.
    pub(crate) fn ask(&self, backend: &dyn Backend, ask: ElementAsk, input_epoch: u64, guard: Option<&Arc<Guard>>) -> Waited {
        match backend.element_runner() {
            Some(runner) => self.ask_on(runner, ask, input_epoch, guard),
            None => Waited { outcome: backend.element_ask(&ask), program: true, behind: Duration::ZERO },
        }
    }

    /// [`Elements::ask`] on the thread of `runner`.
    ///
    /// The event loop waits in [`Signal::wait`]. A question asked while it already waits — from
    /// code that something sent to one of this thread's windows ran — is refused at once with
    /// [`NESTED`] rather than waited for in a second wait inside the first: no such code asks one
    /// today (see [`Signal`]), and two waits inside each other would answer the outer one only once
    /// the inner one has. Until the thread takes the question, the guard's wall clock stands — but
    /// not for a question that shares the one being asked: that wait is for its own answer.
    fn ask_on(&self, runner: ElementRunner, ask: ElementAsk, input_epoch: u64, guard: Option<&Arc<Guard>>) -> Waited {
        let refused =
            |why: String| Waited { outcome: Err(ElementFailure { why, unanswered: false }), program: false, behind: Duration::ZERO };
        if self.waiting.get() {
            return refused(NESTED.to_string());
        }
        if let Err(why) = self.start(runner) {
            return refused(why);
        }
        let pid = (runner.process_of)(ask.hwnd());
        if pid == std::process::id() {
            return refused(OWN_WINDOW.to_string());
        }
        let Some(signal) = Signal::new() else {
            return refused("the event the host waits on for the answer could not be made".to_string());
        };
        let back = Arc::new(Back::new(signal, guard.cloned()));
        let asked = Instant::now();
        let put = {
            let mut q = lock(&self.inner.queue);
            let put = q.submit(ask, pid, Lane::Waited, input_epoch, To::Wait(back.clone()));
            // Under the lock: the thread cannot take the question before the clock stands.
            if put != Put::Running {
                if let Some(g) = guard {
                    g.stop_wall();
                }
            }
            put
        };
        self.inner.work.notify_one();
        self.waiting.set(true);
        back.signal.wait(&|| back.answered());
        self.waiting.set(false);
        // Taken or answered without being put to the program: the clock goes on either way.
        if let Some(g) = guard {
            g.go_on();
        }
        let behind = match (put, back.taken_at()) {
            (Put::Running, _) => Duration::ZERO,
            (_, Some(at)) => at.saturating_duration_since(asked),
            (_, None) => asked.elapsed(),
        };
        let (outcome, program) = back
            .take()
            .unwrap_or_else(|| (Err(ElementFailure { why: "the answer was lost".to_string(), unanswered: false }), false));
        Waited { outcome, program, behind }
    }

    /// Asks `ask` on the thread of `runner` for a callback, `pending`: its answer is handed out by
    /// [`Elements::take_due`] on a later turn. A platform without the thread asks on the event loop
    /// and hands the answer over with [`Elements::answer_now`] (element_lua.rs).
    pub(crate) fn ask_for(&self, runner: ElementRunner, ask: ElementAsk, input_epoch: u64, pending: Pending) {
        let (owner, key) = (pending.owner, pending.key.clone());
        let token = self.keep(pending);
        let refuse = |why: String| {
            let _ = self.done_tx.send((token, Err(ElementFailure { why, unanswered: false }), false));
        };
        if let Err(why) = self.start(runner) {
            return refuse(why);
        }
        let pid = (runner.process_of)(ask.hwnd());
        if pid == std::process::id() {
            return refuse(OWN_WINDOW.to_string());
        }
        let lane = Lane::of(current_priority());
        if lock(&self.inner.queue).submit(ask, pid, lane, input_epoch, To::Call { token, owner, key }) == Put::New {
            self.inner.work.notify_one();
        }
    }

    /// A newer question of module VM `owner` with `key` was asked: the callbacks of that VM and key
    /// that wait for a question not asked yet are told [`REPLACED`] on the next turn, and a question
    /// nobody waits for any more is not asked. The one being asked is answered as it is.
    pub(crate) fn replace(&self, owner: Owner, key: &str) {
        let replaced = lock(&self.inner.queue).take_calls(|o, k| o == owner && k == Some(key));
        for token in replaced {
            let _ = self.done_tx.send((token, Err(ElementFailure { why: REPLACED.to_string(), unanswered: false }), false));
        }
    }

    /// An answer known at the call — the epoch's, or the event loop's own on a platform without the
    /// thread, kept for the epoch there already — for `pending`: handed out on the next turn, as one
    /// from the thread is, so a callback is never called inside the call that handed it over.
    pub(crate) fn answer_now(&self, pending: Pending, outcome: ElementOutcome) {
        let token = self.keep(pending);
        let _ = self.done_tx.send((token, outcome, false));
    }

    /// The next token, with `pending` kept under it and counted for `host.element.pending`.
    fn keep(&self, pending: Pending) -> u64 {
        let token = self.next.get() + 1;
        self.next.set(token);
        if let Some(out) = pending.out() {
            *self.out.borrow_mut().entry(out).or_insert(0) += 1;
        }
        self.pending.borrow_mut().insert(token, pending);
        token
    }

    /// A question counted under `out` ([`Pending::out`]) is done with: its answer's event runs, or
    /// is dropped.
    pub(crate) fn settle(&self, out: Option<(Owner, String)>) {
        let Some(out) = out else { return };
        let mut counts = self.out.borrow_mut();
        if let Some(n) = counts.get_mut(&out) {
            *n -= 1;
            if *n == 0 {
                counts.remove(&out);
            }
        }
    }

    /// `host.element.pending(key)`: whether a question of module VM `owner` asked with `key` and a
    /// callback is not answered to it yet, nor dropped.
    pub(crate) fn pending_with(&self, owner: Owner, key: &str) -> bool {
        self.out.borrow().contains_key(&(owner, key.to_string()))
    }

    /// Module `idx` was disabled, reloaded or removed, or the guard stopped it: its questions with
    /// callbacks are dropped. Those not asked yet are not asked — a press or a focus step of a module
    /// that is gone is not made — and the answer to one being asked, or on its way, is not handed
    /// out. What already waits in its mailbox goes with the mailbox (`mailbox::drop_owner`).
    pub(crate) fn drop_owner(&self, idx: usize) {
        self.drop_where(|o| o.idx == idx);
    }

    /// A hot-load rolled back to `n` modules: the questions of every module from `n` on are dropped,
    /// as [`Elements::drop_owner`] drops one's.
    pub(crate) fn drop_from(&self, n: usize) {
        self.drop_where(|o| o.idx >= n);
    }

    fn drop_where(&self, which: impl Fn(Owner) -> bool) {
        lock(&self.inner.queue).take_calls(|o, _| which(o));
        let gone: Vec<Pending> = {
            let mut pending = self.pending.borrow_mut();
            let tokens: Vec<u64> = pending.iter().filter(|(_, p)| which(p.owner)).map(|(t, _)| *t).collect();
            tokens.into_iter().filter_map(|t| pending.remove(&t)).collect()
        };
        for p in gone {
            self.settle(p.out());
        }
    }

    /// The thread, started at the first question. One that cannot be started says so once and
    /// answers every question with why.
    fn start(&self, runner: ElementRunner) -> Result<(), String> {
        let mut started = self.started.borrow_mut();
        match &*started {
            Started::Yes => return Ok(()),
            Started::Failed(why) => return Err(why.clone()),
            Started::No => {}
        }
        let (inner, done) = (self.inner.clone(), self.done_tx.clone());
        match std::thread::Builder::new().name(THREAD.to_string()).spawn(move || serve(inner, runner, done)) {
            Ok(_) => {
                *started = Started::Yes;
                Ok(())
            }
            Err(e) => {
                let why = format!("the host could not start its accessibility thread: {e}");
                logging::line("uia", &why);
                *started = Started::Failed(why.clone());
                Err(why)
            }
        }
    }

    /// The answers that came since the last turn for callbacks, in the order they came. An answer
    /// for a question dropped with its module is let go here.
    pub(crate) fn take_due(&self) -> Vec<Due> {
        let mut out = Vec::new();
        while let Ok((token, outcome, program)) = self.done_rx.try_recv() {
            if let Some(pending) = self.pending.borrow_mut().remove(&token) {
                out.push(Due { pending, outcome, program });
            }
        }
        out
    }
}

impl Drop for Elements {
    /// The thread ends with the host's `Shared`, after the question it is asking, if any.
    fn drop(&mut self) {
        lock(&self.inner.queue).closed = true;
        self.inner.work.notify_all();
    }
}

/// The event loop's wait for one answer from the thread.
///
/// **Windows.** A manual-reset event the thread sets, waited on with `MsgWaitForMultipleObjectsEx`
/// for it or for a message *sent* to this thread (`QS_SENDMESSAGE`); a sent message is answered
/// with `PeekMessageW(PM_QS_SENDMESSAGE | PM_NOREMOVE)`, which calls the window procedure of every
/// message sent to this thread and takes nothing out of its queue. So while the event loop waits:
///
/// - **Answered:** what another thread or program sends to one of our windows and waits for — a
///   screen reader's `WM_GETOBJECT` and the messages the system's accessibility proxies send to
///   read a control (`WM_GETTEXT` and the like), the activation messages Windows sends as the
///   foreground moves (`WM_ACTIVATE`, `WM_NCACTIVATE`, `WM_ACTIVATEAPP`). These run in wxWidgets'
///   window procedures and in the system's; none of them calls back into a module, and none of the
///   host's handlers runs on one (`gui.rs` binds clicks, keys, menus, the timer and closing — all
///   posted, or sent only by this thread itself). The out-of-context WinEvent callbacks of this
///   thread may run as well: they only queue the event and post a wake-up (`win_event_proc`).
/// - **Not run:** anything posted — the timer that runs the pump, a hotkey's `WM_HOTKEY`, a key or
///   a click into our windows, a COM call into this single-threaded apartment, WebView2's events —
///   which wait in the queue as they waited while the event loop asked the question itself. That
///   is why a window of this application is never asked about on the thread ([`OWN_WINDOW`]): the
///   COM calls its accessibility makes into this apartment would wait for the end of the wait.
/// - **Refused:** a question asked from inside the wait ([`NESTED`]); there is no code that asks
///   one, and a second wait inside the first would hold the first's answer until it ended.
///
/// Should the wait itself fail, the event is waited on without answering anything.
///
/// **Elsewhere** (only the tests of other platforms: a Mac asks on the event loop, no wait) a mutex
/// and a condition variable.
#[cfg(windows)]
struct Signal(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: an event handle may be set and waited on from any thread of the process.
#[cfg(windows)]
unsafe impl Send for Signal {}
// SAFETY: as above; the handle is only ever passed to the system.
#[cfg(windows)]
unsafe impl Sync for Signal {}

#[cfg(windows)]
impl Signal {
    fn new() -> Option<Signal> {
        // SAFETY: an unnamed manual-reset event, not set, with default security.
        let h = unsafe { windows_sys::Win32::System::Threading::CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        (!h.is_null()).then_some(Signal(h))
    }

    fn set(&self) {
        // SAFETY: our own event, alive for as long as `self` is.
        unsafe { windows_sys::Win32::System::Threading::SetEvent(self.0) };
    }

    /// Waits until `done` says yes, answering what is sent to this thread meanwhile (see above).
    fn wait(&self, done: &dyn Fn() -> bool) {
        use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            MsgWaitForMultipleObjectsEx, PeekMessageW, MSG, PM_NOREMOVE, PM_QS_SENDMESSAGE, QS_SENDMESSAGE,
        };
        let answer_sent = || {
            // SAFETY: a plain query into a zeroed MSG of our own; with PM_QS_SENDMESSAGE only, it
            // calls the window procedures of the messages sent to this thread and removes nothing.
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE | PM_QS_SENDMESSAGE) };
        };
        // What was sent before the wait began is answered first: the wait wakes only for what comes.
        answer_sent();
        while !done() {
            // SAFETY: one handle of our own, alive for the call.
            let woke = unsafe { MsgWaitForMultipleObjectsEx(1, &self.0, INFINITE, QS_SENDMESSAGE, 0) };
            if woke == WAIT_OBJECT_0 + 1 {
                answer_sent();
            } else if woke == WAIT_FAILED {
                // SAFETY: as above; the plain wait, answering nothing, until the thread sets it.
                unsafe { WaitForSingleObject(self.0, INFINITE) };
            }
        }
    }
}

#[cfg(windows)]
impl Drop for Signal {
    fn drop(&mut self) {
        // SAFETY: our own handle, closed once.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(windows))]
struct Signal {
    set: Mutex<bool>,
    bell: Condvar,
}

#[cfg(not(windows))]
impl Signal {
    fn new() -> Option<Signal> {
        Some(Signal { set: Mutex::new(false), bell: Condvar::new() })
    }

    fn set(&self) {
        *self.set.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.bell.notify_all();
    }

    fn wait(&self, done: &dyn Fn() -> bool) {
        let mut set = self.set.lock().unwrap_or_else(PoisonError::into_inner);
        while !*set && !done() {
            set = self.bell.wait(set).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const VM: Owner = Owner { idx: 3, gen: 1 };

    fn find(hwnd: isize, name: &str) -> ElementAsk {
        ElementAsk::Find { hwnd, name: name.to_string(), ctype: 50033 }
    }

    fn press(hwnd: isize, name: &str) -> ElementAsk {
        ElementAsk::Press { hwnd, name: name.to_string(), ctype: 50000 }
    }

    fn call(token: u64) -> To {
        To::Call { token, owner: VM, key: None }
    }

    fn keyed(token: u64, module: usize, key: &str) -> To {
        To::Call { token, owner: Owner { idx: module, gen: 1 }, key: Some(key.to_string()) }
    }

    fn tokens(to: &[To]) -> Vec<u64> {
        to.iter().filter_map(|t| if let To::Call { token, .. } = t { Some(*token) } else { None }).collect()
    }

    fn wait() -> To {
        To::Wait(Arc::new(Back::new(Signal::new().unwrap(), None)))
    }

    /// A callback's question of module VM `owner`, with `key`, for the tests of `Elements`.
    fn pending(lua: &mlua::Lua, owner: Owner, key: Option<&str>, ask: ElementAsk) -> Pending {
        Pending {
            answer: crate::acting::Answer { idx: owner.idx, lua: lua.clone(), cb: lua.create_function(|_, ()| Ok(())).unwrap(), what: "element.find" },
            prio: Priority::Background,
            ask,
            asked_in: (0, 0),
            owner,
            key: key.map(str::to_string),
        }
    }

    /// The same question, whoever asks it, is asked once while it waits; another one is a question
    /// of its own. The one the event loop waits for goes first, then a person's, then a poll's, each
    /// lane in the order asked — and a question shared with a more urgent asker moves up with it.
    #[test]
    fn the_same_question_waits_once_and_the_urgent_go_first() {
        let mut q = Queue::default();
        assert_eq!(q.submit(find(1, "Kontakt 8"), 10, Lane::Background, 0, call(1)), Put::New);
        assert_eq!(q.submit(find(1, "Kontakt 8"), 10, Lane::Background, 0, call(2)), Put::Waiting);
        assert_eq!(q.submit(find(2, "Kontakt 8"), 20, Lane::Background, 0, call(3)), Put::New, "another window");
        assert_eq!(q.submit(find(1, "Kontakt 7"), 10, Lane::Interactive, 0, call(4)), Put::New, "another name");
        assert_eq!(q.submit(find(3, "x"), 30, Lane::Interactive, 0, call(5)), Put::New);
        assert_eq!(q.jobs.len(), 4);
        assert_eq!(q.next(), Some((find(1, "Kontakt 7"), 10)), "a person's before the polls'");
        assert_eq!(tokens(&q.finish(false).0), vec![4]);
        // A poll's question the event loop now waits for too moves up with it.
        assert_eq!(q.submit(find(2, "Kontakt 8"), 20, Lane::Waited, 0, wait()), Put::Waiting);
        assert_eq!(q.next(), Some((find(2, "Kontakt 8"), 20)));
        let (to, _) = q.finish(false);
        assert_eq!((to.len(), tokens(&to)), (2, vec![3]));
        assert_eq!(q.next(), Some((find(3, "x"), 30)), "then the person's");
        q.finish(false);
        assert_eq!(q.next(), Some((find(1, "Kontakt 8"), 10)));
        assert_eq!(tokens(&q.finish(false).0), vec![1, 2], "both askers, one answer");
        assert_eq!(q.next(), None);
    }

    /// The question being asked is shared only while nothing acted on the screen since it was put;
    /// a question that acts is never shared, waiting or being asked; and one waiting is shared only
    /// with a question put in the same input epoch.
    #[test]
    fn the_question_being_asked_is_shared_only_with_nothing_acted_since_and_never_one_that_acts() {
        let mut q = Queue::default();
        q.submit(find(1, "VIEW"), 10, Lane::Background, 7, call(1));
        q.next();
        assert_eq!(q.submit(find(1, "VIEW"), 10, Lane::Background, 7, call(2)), Put::Running, "the same input epoch");
        assert_eq!(q.submit(find(1, "VIEW"), 10, Lane::Background, 8, call(3)), Put::New, "something acted since");
        assert_eq!(q.submit(find(1, "VIEW"), 10, Lane::Background, 9, call(9)), Put::New, "and again: not the waiting one either");
        assert_eq!(tokens(&q.finish(false).0), vec![1, 2]);
        let press = press(1, "Bypass");
        assert_eq!(q.submit(press.clone(), 10, Lane::Interactive, 9, call(4)), Put::New);
        assert_eq!(q.submit(press.clone(), 10, Lane::Interactive, 9, call(5)), Put::New, "two presses are two actions");
        let step = ElementAsk::FocusStep { hwnd: 1, direction: 1 };
        q.submit(step.clone(), 10, Lane::Interactive, 9, call(6));
        // `find(1, "VIEW")` at epochs 8 and 9 still waits, beside the two presses and the step.
        assert_eq!(q.jobs.len(), 5);
        assert_eq!(q.next(), Some((press.clone(), 10)), "a person's first");
        assert_eq!(q.submit(press, 10, Lane::Interactive, 9, call(7)), Put::New, "not even the one being made");
        assert_eq!(q.submit(step, 10, Lane::Interactive, 9, call(8)), Put::New);
    }

    /// No question is asked before one that acts on the same program and was put before it, so
    /// nothing asked after a press finds what was there before it — not even one the event loop
    /// waits for — and the act goes as early as the most urgent question behind it. A question put
    /// after an act does not share one put before it. Another program's questions go their own way.
    #[test]
    fn a_question_asked_after_an_act_is_asked_after_it() {
        let mut q = Queue::default();
        q.submit(find(5, "poll"), 10, Lane::Background, 0, call(1));
        q.submit(find(1, "Menu"), 10, Lane::Background, 0, call(2));
        q.submit(find(9, "other"), 20, Lane::Interactive, 0, call(4));
        q.submit(press(1, "Snapshot"), 10, Lane::Interactive, 1, call(3));
        assert_eq!(q.submit(find(1, "Menu"), 10, Lane::Waited, 1, wait()), Put::New, "not the one from before the press");
        q.submit(find(9, "other 2"), 20, Lane::Interactive, 1, call(5));
        // The menu question before the press is a poll's; the press goes before it, as urgent as the
        // wait behind it — before another program's person's question put before the press, too.
        assert_eq!(q.next(), Some((press(1, "Snapshot"), 10)));
        q.finish(false);
        assert_eq!(q.next(), Some((find(1, "Menu"), 10)), "the event loop's, now after the press");
        assert_eq!(q.finish(false).0.len(), 1);
        assert_eq!(q.next(), Some((find(9, "other"), 20)));
        q.finish(false);
        assert_eq!(q.next(), Some((find(9, "other 2"), 20)));
        q.finish(false);
        assert_eq!(q.next(), Some((find(5, "poll"), 10)), "same lane, put first");
        q.finish(false);
        assert_eq!(q.next(), Some((find(1, "Menu"), 10)), "the poll's from before the press, asked after it all the same");
        assert_eq!(tokens(&q.finish(false).0), vec![2]);
        // A focus step turns no epoch over: a question after it still does not share one from
        // before it.
        q.submit(find(1, "y"), 10, Lane::Background, 4, call(8));
        q.submit(ElementAsk::FocusStep { hwnd: 1, direction: 1 }, 10, Lane::Interactive, 4, call(9));
        assert_eq!(q.submit(find(1, "y"), 10, Lane::Background, 4, call(10)), Put::New, "a focus step between them");
        assert_eq!(q.submit(find(1, "y"), 10, Lane::Background, 4, call(11)), Put::Waiting, "two after it share");
        while q.next().is_some() {
            q.finish(false);
        }
        // Two acts in a row: the second after the first, a question after both after both.
        q.submit(press(1, "a"), 10, Lane::Background, 2, call(6));
        q.submit(press(1, "b"), 10, Lane::Background, 3, call(7));
        q.submit(find(1, "x"), 10, Lane::Waited, 3, wait());
        assert_eq!(q.next(), Some((press(1, "a"), 10)));
        q.finish(false);
        assert_eq!(q.next(), Some((press(1, "b"), 10)));
        q.finish(false);
        assert_eq!(q.next(), Some((find(1, "x"), 10)));
    }

    /// A program that did not answer in time is not sent the questions that waited behind it: they
    /// are taken out, to be told so, and only the same program's; a question asked after is put to
    /// it again. One it answered — even "no" — or refused otherwise leaves the others waiting.
    #[test]
    fn the_questions_behind_one_a_program_did_not_answer_are_not_put_to_it() {
        let mut q = Queue::default();
        q.submit(find(1, "Komplete Kontrol"), 10, Lane::Background, 0, call(1));
        q.next();
        q.submit(find(1, "Kontakt 8"), 10, Lane::Background, 0, call(2));
        q.submit(find(2, "PlogueXMLGUI"), 20, Lane::Background, 0, call(3));
        q.submit(find(4, "Kontakt 7"), 10, Lane::Interactive, 0, call(4));
        let (to, behind) = q.finish(true);
        assert_eq!(tokens(&to), vec![1]);
        let mut gone: Vec<u64> = behind.iter().flat_map(|j| tokens(&j.to)).collect();
        gone.sort();
        assert_eq!(gone, vec![2, 4], "every question to process 10, another window of it included");
        assert_eq!(q.next(), Some((find(2, "PlogueXMLGUI"), 20)), "another program's is asked");
        let (_, behind) = q.finish(true);
        assert!(behind.is_empty());
        q.submit(find(1, "Kontakt 8"), 10, Lane::Background, 0, call(5));
        assert_eq!(q.next(), Some((find(1, "Kontakt 8"), 10)), "asked again after");
        q.submit(find(1, "Kontakt 7"), 10, Lane::Background, 0, call(6));
        let (_, behind) = q.finish(false);
        assert!(behind.is_empty() && q.jobs.len() == 1, "answered: the others wait");
        q.next();
        q.finish(false);
        // A window that is gone has no program: nothing waits behind it.
        q.submit(find(9, "x"), 0, Lane::Background, 0, call(7));
        q.submit(find(9, "y"), 0, Lane::Background, 0, call(8));
        assert_eq!(q.next(), Some((find(9, "x"), 0)));
        let (_, behind) = q.finish(true);
        assert!(behind.is_empty() && q.jobs.len() == 1);
    }

    /// A newer question of the same module VM with the same key takes out the ones that wait to be
    /// asked — not another module's, not another key's, not the one being asked — and a question
    /// nobody waits for any more is not asked.
    #[test]
    fn a_newer_question_with_the_same_key_takes_out_the_waiting_ones() {
        let mut q = Queue::default();
        q.submit(find(1, "a"), 10, Lane::Background, 0, keyed(1, 3, "menu"));
        q.next();
        q.submit(find(1, "b"), 10, Lane::Background, 0, keyed(2, 3, "menu"));
        q.submit(find(1, "c"), 10, Lane::Background, 0, keyed(3, 4, "menu"));
        q.submit(find(1, "d"), 10, Lane::Background, 0, keyed(4, 3, "other"));
        q.submit(find(1, "e"), 10, Lane::Background, 0, call(5));
        q.submit(find(1, "e"), 10, Lane::Background, 0, keyed(6, 3, "menu"));
        let menu = |o: Owner, k: Option<&str>| o == Owner { idx: 3, gen: 1 } && k == Some("menu");
        assert_eq!(q.take_calls(menu), vec![2, 6]);
        let waiting: Vec<String> = q.jobs.iter().map(|j| format!("{:?}", tokens(&j.to))).collect();
        assert_eq!(waiting, vec!["[3]", "[4]", "[5]"], "b goes, e stays for its other asker");
        assert_eq!(tokens(&q.finish(false).0), vec![1], "the one being asked is answered");
    }

    /// The thread end to end, with a runner of the test's own: questions with callbacks come back
    /// through `take_due`, the same question twice is asked once, a question the event loop waits
    /// for is answered to the wait, and the questions behind one a program did not answer are told
    /// so without being asked. Only what the runner answered is the program's. Pure threads:
    /// nothing here touches a window.
    #[test]
    fn the_thread_answers_callbacks_and_waits_and_asks_nothing_twice() {
        static ASKED: AtomicU32 = AtomicU32::new(0);
        static GATE: Mutex<bool> = Mutex::new(false);
        static OPEN: Condvar = Condvar::new();
        fn run(ask: &ElementAsk) -> ElementOutcome {
            ASKED.fetch_add(1, Ordering::SeqCst);
            if matches!(ask, ElementAsk::Find { name, .. } if name == "boom") {
                panic!("the test's runner fails");
            }
            // Held until the test opens the gate, so that what it asks meanwhile waits.
            let mut open = GATE.lock().unwrap();
            while !*open {
                open = OPEN.wait(open).unwrap();
            }
            match ask {
                ElementAsk::Find { name, .. } if name == "late" => {
                    Err(ElementFailure { why: "the program did not answer within 1000 ms".into(), unanswered: true })
                }
                ElementAsk::Find { name, .. } => Ok(ElementAnswer::Found(name == "yes")),
                _ => ask.nothing(),
            }
        }
        let runner = ElementRunner { init: || {}, run, process_of: |hwnd| hwnd as u32 };
        let lua = mlua::Lua::new();
        let p = |idx| pending(&lua, Owner { idx, gen: 1 }, None, find(1, "yes"));
        let e = Elements::new();
        e.ask_for(runner, find(1, "late"), 0, p(1));
        // Wait until the thread is inside the first question, then ask behind it.
        while ASKED.load(Ordering::SeqCst) == 0 {
            std::thread::yield_now();
        }
        e.ask_for(runner, find(1, "yes"), 0, p(2));
        e.ask_for(runner, find(2, "yes"), 0, p(3));
        e.ask_for(runner, find(2, "yes"), 0, p(4));
        *GATE.lock().unwrap() = true;
        OPEN.notify_all();
        let mut due = Vec::new();
        let until = Instant::now() + Duration::from_secs(10);
        while due.len() < 4 && Instant::now() < until {
            due.extend(e.take_due());
            std::thread::yield_now();
        }
        let mut got: Vec<(usize, ElementOutcome, bool)> = due.into_iter().map(|d| (d.pending.answer.idx, d.outcome, d.program)).collect();
        got.sort_by_key(|g| g.0);
        assert_eq!(got.len(), 4);
        let late = Err(ElementFailure { why: "the program did not answer within 1000 ms".into(), unanswered: true });
        assert_eq!((&got[0].1, got[0].2), (&late, true), "the program's own not known");
        assert_eq!(
            (&got[1].1, got[1].2),
            (
                &Err(ElementFailure {
                    why: "the program did not answer within 1000 ms to an earlier question, so this one was not asked".into(),
                    unanswered: true
                }),
                false
            ),
            "the same program's question, behind it, not put to it"
        );
        assert_eq!((&got[2].1, &got[3].1), (&Ok(ElementAnswer::Found(true)), &Ok(ElementAnswer::Found(true))));
        assert!(got[2].2 && got[3].2);
        assert_eq!(ASKED.load(Ordering::SeqCst), 2, "the late one and the other program's, once");
        // The event loop's wait is answered by the thread.
        let w = e.ask_on(runner, find(3, "no"), 0, None);
        assert_eq!((w.outcome, w.program), (Ok(ElementAnswer::Found(false)), true));
        assert_eq!(ASKED.load(Ordering::SeqCst), 3);
        // A runner that panics: not known, the thread goes on, and it is no answer of the program's.
        let w = e.ask_on(runner, find(3, "boom"), 0, None);
        assert!(!w.program && matches!(&w.outcome, Err(f) if f.why.starts_with("the host's accessibility thread failed")), "{:?}", w.outcome);
        assert_eq!(e.ask_on(runner, find(3, "yes"), 0, None).outcome, Ok(ElementAnswer::Found(true)));
        assert_eq!(ASKED.load(Ordering::SeqCst), 5);
        // A window of this application is not asked about, with a callback or without.
        let own = std::process::id() as isize;
        let w = e.ask_on(runner, find(own, "x"), 0, None);
        assert_eq!((w.outcome, w.program), (Err(ElementFailure { why: OWN_WINDOW.into(), unanswered: false }), false));
        e.ask_for(runner, find(own, "x"), 0, p(5));
        let due = e.take_due();
        assert_eq!((due.len(), &due[0].outcome, due[0].program), (1, &Err(ElementFailure { why: OWN_WINDOW.into(), unanswered: false }), false));
        assert_eq!(ASKED.load(Ordering::SeqCst), 5, "neither was put to the thread");
        assert!(e.pending.borrow().is_empty(), "every callback answered once");
    }

    /// The event loop's wait behind the question the thread is asking stops the guard's wall clock
    /// for the callback, and the thread starts it again as it takes the waited question — before
    /// asking it; the wait says how much of it went by behind. Pure threads.
    #[test]
    fn a_wait_behind_another_question_stops_the_guards_wall_clock_until_its_own_is_taken() {
        use std::sync::atomic::AtomicBool;
        use std::sync::OnceLock;
        static GUARD: OnceLock<Arc<Guard>> = OnceLock::new();
        static IN_FIRST: AtomicBool = AtomicBool::new(false);
        static GATE: Mutex<bool> = Mutex::new(false);
        static OPEN: Condvar = Condvar::new();
        static STOOD_AT_OWN: Mutex<Option<bool>> = Mutex::new(None);
        fn run(ask: &ElementAsk) -> ElementOutcome {
            match ask {
                ElementAsk::Find { name, .. } if name == "first" => {
                    IN_FIRST.store(true, Ordering::SeqCst);
                    let mut open = GATE.lock().unwrap();
                    while !*open {
                        open = OPEN.wait(open).unwrap();
                    }
                }
                _ => *STOOD_AT_OWN.lock().unwrap() = Some(GUARD.get().unwrap().wall_stands()),
            }
            Ok(ElementAnswer::Found(true))
        }
        let guard = GUARD.get_or_init(Guard::new).clone();
        let runner = ElementRunner { init: || {}, run, process_of: |hwnd| hwnd as u32 };
        let lua = mlua::Lua::new();
        let e = Elements::new();
        e.ask_for(runner, find(1, "first"), 0, pending(&lua, VM, None, find(1, "first")));
        while !IN_FIRST.load(Ordering::SeqCst) {
            std::thread::yield_now();
        }
        let g = guard.clone();
        let opener = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            let mut stood = false;
            while !stood && Instant::now() < until {
                stood = g.wall_stands();
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_millis(10));
            *GATE.lock().unwrap() = true;
            OPEN.notify_all();
            stood
        });
        let w = e.ask_on(runner, find(2, "own"), 0, Some(&guard));
        assert!(opener.join().unwrap(), "the wall clock stood while the wait was behind the first question");
        assert_eq!(*STOOD_AT_OWN.lock().unwrap(), Some(false), "and ran again as the thread took the waited question");
        assert!(!guard.wall_stands());
        assert!(w.program && w.behind >= Duration::from_millis(10), "{:?}", w.behind);
    }

    /// A question asked while the event loop already waits is refused, not waited for again, and is
    /// no answer of the program's; a platform without the thread asks on the event loop, and that is
    /// the program's answer.
    #[test]
    fn a_question_inside_the_wait_is_refused_and_without_a_thread_it_is_asked_here() {
        let runner = ElementRunner { init: || {}, run: |_| Ok(ElementAnswer::Found(true)), process_of: |_| 1 };
        let e = Elements::new();
        e.waiting.set(true);
        let w = e.ask_on(runner, find(1, "x"), 0, None);
        assert_eq!((w.outcome, w.program), (Err(ElementFailure { why: NESTED.into(), unanswered: false }), false));
        assert!(matches!(&*e.started.borrow(), Started::No), "nothing was started for it");
        e.waiting.set(false);
        let stub = crate::backend::stub::StubBackend::default();
        let w = e.ask(&stub, find(1, "x"), 0, None);
        assert_eq!((w.outcome, w.program, w.behind), (Ok(ElementAnswer::Found(false)), true, Duration::ZERO));
    }

    /// `host.element.pending(key)`: a callback's question with a key counts from the call until its
    /// answer's event runs or is dropped, per module VM and key — two count twice; a replaced one
    /// still counts until its answer runs; one without a key never counts.
    #[test]
    fn a_keyed_question_is_pending_until_its_answer_runs() {
        let lua = mlua::Lua::new();
        let e = Elements::new();
        let other = Owner { idx: 4, gen: 1 };
        // Answered from the store at the call: out until its event runs.
        e.answer_now(pending(&lua, VM, Some("menu"), find(1, "x")), Ok(ElementAnswer::Found(true)));
        e.answer_now(pending(&lua, VM, Some("menu"), find(1, "x")), Ok(ElementAnswer::Found(true)));
        e.answer_now(pending(&lua, VM, None, find(1, "x")), Ok(ElementAnswer::Found(true)));
        assert!(e.pending_with(VM, "menu") && !e.pending_with(other, "menu") && !e.pending_with(VM, "other"));
        let due = e.take_due();
        assert_eq!(due.len(), 3);
        assert!(due.iter().all(|d| !d.program), "the store's answer is not kept again");
        assert!(e.pending_with(VM, "menu"), "in the mailbox: still out");
        e.settle(due[0].pending.out());
        assert!(e.pending_with(VM, "menu"), "one of two ran");
        e.settle(due[1].pending.out());
        e.settle(due[2].pending.out());
        assert!(!e.pending_with(VM, "menu"));
        // Replaced before it was asked: still out until its answer runs, and it says so. (No thread:
        // the question stays in the queue for the newer one to take out.)
        let token = e.keep(pending(&lua, VM, Some("menu"), find(1, "y")));
        lock(&e.inner.queue).submit(find(1, "y"), 1, Lane::Background, 0, To::Call { token, owner: VM, key: Some("menu".into()) });
        e.replace(VM, "menu");
        assert!(lock(&e.inner.queue).jobs.is_empty(), "not asked");
        let due = e.take_due();
        assert_eq!((due.len(), &due[0].outcome, due[0].program), (1, &Err(ElementFailure { why: REPLACED.into(), unanswered: false }), false));
        assert!(e.pending_with(VM, "menu"));
        e.settle(due[0].pending.out());
        assert!(!e.pending_with(VM, "menu"));
    }

    /// A module disabled, reloaded or stopped: its questions not asked yet are taken out of the
    /// queue — a press of it is not made — another module's stay, nothing of its is handed out, and
    /// `pending` is false for its keys. A rollback drops every module from its index on.
    #[test]
    fn a_dropped_modules_questions_are_not_asked() {
        let lua = mlua::Lua::new();
        let e = Elements::new();
        let other = Owner { idx: 4, gen: 1 };
        let mut tokens_of = Vec::new();
        for (owner, ask, key) in [(VM, press(1, "Bypass"), Some("p")), (other, find(1, "x"), None), (VM, find(1, "x"), None)] {
            let token = e.keep(pending(&lua, owner, key, ask.clone()));
            lock(&e.inner.queue).submit(ask, 1, Lane::Background, 0, To::Call { token, owner, key: key.map(str::to_string) });
            tokens_of.push(token);
        }
        // An answer of the module's on its way back already.
        e.answer_now(pending(&lua, VM, Some("p"), find(2, "y")), Ok(ElementAnswer::Found(true)));
        assert!(e.pending_with(VM, "p"));
        e.drop_owner(VM.idx);
        let left: Vec<(ElementAsk, Vec<u64>)> = lock(&e.inner.queue).jobs.iter().map(|j| (j.ask.clone(), tokens(&j.to))).collect();
        assert_eq!(left, vec![(find(1, "x"), vec![tokens_of[1]])], "the press is not made; the shared question stays for the other");
        assert!(!e.pending_with(VM, "p"));
        assert!(e.take_due().is_empty(), "nothing of the module's is handed out");
        e.drop_from(4);
        assert!(lock(&e.inner.queue).jobs.is_empty() && e.pending.borrow().is_empty());
    }

    /// The wait answers what is sent to this thread while it waits: another thread's `SendMessage`
    /// to a window of the waiting thread returns before the answer is given — a screen reader's
    /// `WM_GETOBJECT` is that — and the wait ends once it is. A message-only window of the test's
    /// own: never shown, not in the desktop's window list — but a window, which the maintainer's
    /// rule for what runs unasked keeps out of the default run.
    #[cfg(windows)]
    #[test]
    #[ignore = "makes a message-only window: run on request, `cargo test -p host -- --ignored the_wait_answers`"]
    fn the_wait_answers_what_is_sent_to_this_threads_windows() {
        use std::sync::atomic::AtomicIsize;
        use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SendMessageW, HWND_MESSAGE, WM_APP, WNDCLASSW,
        };
        static ANSWERED: AtomicU32 = AtomicU32::new(0);
        unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
            if msg == WM_APP + 7 {
                ANSWERED.fetch_add(1, Ordering::SeqCst);
                return 42;
            }
            DefWindowProcW(hwnd, msg, w, l)
        }
        let class: Vec<u16> = "element-wait-test\0".encode_utf16().collect();
        // SAFETY: this module's own handle.
        let module = unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) };
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: module, lpszClassName: class.as_ptr(), ..unsafe { std::mem::zeroed() } };
        // SAFETY: a class of this test's own and a message-only window of it — never shown, not in
        // the desktop's window list — destroyed below.
        let hwnd = unsafe {
            RegisterClassW(&wc);
            CreateWindowExW(0, class.as_ptr(), std::ptr::null(), 0, 0, 0, 0, 0, HWND_MESSAGE, std::ptr::null_mut(), module, std::ptr::null())
        };
        assert!(!hwnd.is_null());
        static WINDOW: AtomicIsize = AtomicIsize::new(0);
        WINDOW.store(hwnd as isize, Ordering::SeqCst);
        let signal = Arc::new(Signal::new().unwrap());
        let s = signal.clone();
        let sender = std::thread::spawn(move || {
            // SAFETY: a message to the test's window, which its thread answers in the wait.
            let r = unsafe { SendMessageW(WINDOW.load(Ordering::SeqCst) as HWND, WM_APP + 7, 0, 0) };
            s.set();
            r
        });
        let done = || ANSWERED.load(Ordering::SeqCst) > 0;
        signal.wait(&done);
        assert_eq!(sender.join().unwrap(), 42, "the sent message was answered inside the wait");
        assert_eq!(ANSWERED.load(Ordering::SeqCst), 1);
        // SAFETY: the window made above.
        unsafe { DestroyWindow(hwnd) };
    }

    /// The `[uia]` line for a slow question says what, of which window and program, how long, and
    /// what came of it.
    #[test]
    fn a_slow_question_says_what_it_asked_and_what_came_of_it() {
        let any = ElementAsk::FindAny { hwnd: 4242, names: vec!["Kontakt 8".into(), "Kontakt 7".into()], types: vec![50032, 50033] };
        let late = Err(ElementFailure { why: "the program did not answer within 1000 ms".into(), unanswered: true });
        assert_eq!(
            slow_line(&any, 77, Duration::from_millis(1003), &late),
            "findAny(Kontakt 8/Kontakt 7) of window 4242 (process 77) took 1003 ms on the accessibility thread: \
             not known: the program did not answer within 1000 ms"
        );
        assert!(slow_line(&any, 77, Duration::from_millis(60), &Ok(ElementAnswer::Index(Some(1)))).ends_with(": answered"));
        assert!(slow_line(&any, 77, Duration::from_millis(60), &Ok(ElementAnswer::Index(None))).ends_with(": nothing found"));
        let dump = ElementAsk::RawDump { hwnd: 9 };
        assert_eq!(
            slow_line(&dump, 1, Duration::from_millis(5000), &Ok(ElementAnswer::Nodes(Vec::new(), Some("the program is gone".into())))),
            "rawDump() of window 9 (process 1) took 5000 ms on the accessibility thread: 0 element(s), and then the program is gone"
        );
    }
}
