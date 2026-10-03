//! What a text recognition cost, as the log says it: the pure half of the macOS recogniser's cost
//! lines (`backend/macos/ocr.rs`) and of the processor line in `[env]`.
//!
//! The logs of September 2026 left four questions open that an ordinary session ought to answer
//! by itself: whether the expensive first read is the first in the process, the first on its
//! thread, or simply the first after a pause; how many Vision passes a field costs; whether two
//! passes ran at once; and which processor the Mac has. So a recognition says which first it was
//! and how long Vision had been idle before it, a slow one names every pass with the rung of the
//! ladder it was and whether another pass ran beside it, the warm-up says whether it read its test
//! line, and `[env]` names the processor. Nothing here changes what is read. And the warm-ups
//! take turns ([`Gate`]), so that their two lines time two first passes apart; the neural
//! recogniser's waits for Vision's to end ([`Latch`]).
//!
//! Std only, so the wording and the arithmetic are tested on every platform; `crates/macos-check`
//! borrows it.

// Everywhere but on a Mac only the processor line is used; the rest is compiled there for its
// tests.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::ladder::Path;

/// The rung of the retry ladder a Vision pass was, or the warm-up's pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// A region larger than 400x200 points, handed to Vision as captured.
    AsCaptured,
    /// The content crop, enlarged: the first pass of a small region.
    Tight,
    /// The whole region, enlarged for the ink, after a tight pass that read nothing.
    Whole,
    /// The crop at twice the enlargement, the accurate model again.
    Enlarged,
    /// The same enlarged picture, the fast model.
    Fast,
    /// The fast model over the content crop, before any accurate pass: an Intel Mac's, checked by
    /// the neural recogniser (`ocr/merge.rs`), and `ocr-bench`'s strategies that read fast first.
    FastFirst,
    /// The content crop again under request revision 2, after a first pass that read nothing,
    /// instead of the whole region: `ocr-bench`'s `rev3>rev2`.
    TightAgain,
    /// The fast model over the content crop once the read's answer is picked, on `host.ocr.read`'s
    /// recognise thread of a Mac that does not check its fast level, for the neural recogniser's
    /// counts (`ocr/shadow.rs`): its reading is compared and never answers.
    FastCompared,
    /// The warm-up's own pass over its test line — the accurate one, and on an Intel Mac the fast
    /// one after it.
    WarmUp,
}

impl Stage {
    pub fn word(self) -> &'static str {
        match self {
            Stage::AsCaptured => "as captured",
            Stage::Tight => "tight crop",
            Stage::Whole => "whole region",
            Stage::Enlarged => "enlarged",
            Stage::Fast => "fast model",
            Stage::FastFirst => "fast model, first",
            Stage::TightAgain => "tight crop, revision 2",
            Stage::FastCompared => "fast model, only compared",
            Stage::WarmUp => "warm-up",
        }
    }
}

/// One Vision pass, timed as `run_vision` spends it: making the request, the handler, the pass
/// and reading its answer out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass {
    pub stage: Stage,
    pub ms: f64,
    /// The words it read; `None` when Vision refused the request.
    pub words: Option<usize>,
    /// Another Vision pass ran in this process at some moment of this one ([`Running`]).
    pub beside: bool,
    /// The neural recogniser ran at some moment of this pass: it was running when the pass began or
    /// ended, or a run of it began in between. On a Mac only, where it reads beside Vision
    /// (`ocr/merge.rs`); always false elsewhere.
    pub paddle_beside: bool,
}

/// How a read's wait for the neural recogniser ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waited {
    /// It answered with text.
    Answered,
    /// It answered nothing, or could not read the region.
    Nothing,
    /// It had not answered when the wait's bound — what was left of the ladder's budget — was up.
    NotYet,
}

/// What a read waited for the neural recogniser, all told, and how its last wait ended: for the
/// read's lines. A read that never waited for it has none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaddleWait {
    pub ms: f64,
    pub ended: Waited,
}

impl PaddleWait {
    /// How it ended, in Windows' words where it has them (`windows.rs`, `recognize_image`), and in
    /// the same form for the case only a Mac has: a wait with a bound.
    pub fn which(&self) -> &'static str {
        match self.ended {
            Waited::Answered => "which answered",
            Waited::Nothing => "which had nothing",
            Waited::NotYet => "which had not answered by then",
        }
    }
}

/// The wait for the neural recogniser as Windows' own line says it, ` + waited 1.2ms for paddle
/// (which answered)`, for a Mac's line of a read; nothing for a read that did not wait.
pub fn waited_text(w: Option<PaddleWait>) -> String {
    w.map(|w| format!(" + waited {:.1}ms for paddle ({})", w.ms, w.which())).unwrap_or_default()
}

/// How an Intel Mac's check of its fast pass by the neural recogniser ended (`ocr/merge.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// It read what the fast level read, its spaces included: the fast level's answer stands.
    Same,
    /// It read text, and not that.
    Otherwise,
    /// It read nothing, or could not read the region.
    Nothing,
    /// It had not answered within the check's wait.
    NotYet,
}

/// What an Intel Mac's read waited for the neural recogniser's check, and how the check ended:
/// apart from [`PaddleWait`], which is Windows' wait where Vision read nothing, so that neither
/// says the other's outcome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckWait {
    pub ms: f64,
    pub ended: Check,
}

impl CheckWait {
    /// How it ended, in the form of Windows' words for the other wait.
    pub fn which(&self) -> &'static str {
        match self.ended {
            Check::Same => "which read the same",
            Check::Otherwise => "which read otherwise",
            Check::Nothing => "which had nothing",
            Check::NotYet => "which had not answered by then",
        }
    }
}

/// The check's wait, ` + waited 9.8ms for paddle's check (which read the same)`; nothing for a read
/// that made no check.
pub fn check_text(c: Option<CheckWait>) -> String {
    c.map(|c| format!(" + waited {:.1}ms for paddle's check ({})", c.ms, c.which())).unwrap_or_default()
}

/// Who answered a read and what it waited for the neural recogniser — an Intel Mac's check, and
/// the wait where Vision's ladder read nothing — for the cost lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Answered {
    pub path: Path,
    pub check: Option<CheckWait>,
    pub waited: Option<PaddleWait>,
}

impl Answered {
    /// What the read waited for the neural recogniser, the check first: ` + waited …` for each.
    pub fn waits_text(&self) -> String {
        format!("{}{}", check_text(self.check), waited_text(self.waited))
    }

    /// `, answered by Vision` and the waits, as the cost lines say it after a read's time.
    fn text(&self) -> String {
        format!(", answered by {}{}", self.path.word(), self.waits_text())
    }
}

/// Where the picture of a slow read came from, for its line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Capture {
    /// Taken for this read, on this thread, in this many milliseconds: `host.ocr.recognize`.
    Took(f64),
    /// One capture for all the regions of the call, taken before this region's clock started:
    /// `host.ocr.recognizeMany`.
    Shared,
    /// Taken apart from this thread before the recognition began — on the capture thread, or the
    /// pixels of a snapshot the module holds: `host.ocr.read`, whose `[ocr]` line gives the
    /// capture's time when the whole read was slow. Its slow-read threshold is then the
    /// recognition's alone.
    Apart,
}

/// The Vision passes running in the process, so that a pass can say whether another ran beside
/// it at any moment of it — not only whether one was running when it began. One for the process,
/// in `ocr.rs`; a test makes its own.
pub struct Running {
    now: AtomicUsize,
    started: AtomicU64,
}

/// A pass under way, as [`Running::enter`] left it.
#[derive(Debug)]
pub struct Entered {
    seq: u64,
    joined: bool,
}

impl Running {
    pub const fn new() -> Running {
        Running { now: AtomicUsize::new(0), started: AtomicU64::new(0) }
    }

    /// A pass begins.
    pub fn enter(&self) -> Entered {
        // Counted as begun before it counts as running: a pass ending at that very moment then
        // finds it in one count or the other, never in neither. The price is that a pass which
        // ends just as another begins may say it had company it only brushed.
        let seq = self.started.fetch_add(1, Ordering::SeqCst) + 1;
        let joined = self.now.fetch_add(1, Ordering::SeqCst) > 0;
        Entered { seq, joined }
    }

    /// The pass ends: whether another ran beside it — one was running when it began, or one began
    /// since.
    pub fn leave(&self, e: Entered) -> bool {
        self.now.fetch_sub(1, Ordering::SeqCst);
        e.joined || self.started.load(Ordering::SeqCst) != e.seq
    }

    /// Whether a pass is running in the process now: asked when a recognition begins, before its
    /// own passes, so that its line can say Vision was not idle then.
    pub fn busy(&self) -> bool {
        self.now.load(Ordering::SeqCst) > 0
    }
}

impl Default for Running {
    fn default() -> Running {
        Running::new()
    }
}

/// What came before a recognition: whether it is the first real one in the process and on its
/// thread (a warm-up is not one), how long ago the last Vision pass of any kind — a warm-up's
/// included — ended in the process and on its thread, and whether a pass was running in the
/// process at that moment (on another thread: Vision was not idle at all). Taken when the read
/// began.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Before {
    pub first_in_process: bool,
    pub first_on_thread: bool,
    pub since_process: Option<Duration>,
    pub since_thread: Option<Duration>,
    pub busy_at_start: bool,
}

/// The thread a line names: the event loop by its role, any other by its name.
pub fn thread_word(name: Option<&str>) -> String {
    match name {
        // The application's event loop is its main thread, on a Mac by AppKit's rule.
        Some("main") => "the event loop".to_string(),
        Some(n) => format!("thread {n}"),
        None => "an unnamed thread".to_string(),
    }
}

fn secs(d: Duration) -> String {
    format!("{:.1} s", d.as_secs_f64())
}

/// How long Vision had been idle before a recognition, in the process and on its thread — or that
/// it was not idle: a pass of another thread was running when the recognition began.
pub fn pause_text(b: &Before) -> String {
    let thread = match b.since_thread {
        None => "none ran on this thread".to_string(),
        Some(t) => format!("the last on this thread {} before", secs(t)),
    };
    match (b.busy_at_start, b.since_process) {
        (true, _) => format!("another Vision pass in this process was running when it began, {thread}"),
        (false, None) => "no Vision pass ran before it in this process".to_string(),
        (false, Some(p)) => format!("the last Vision pass in this process ended {} before it, {thread}", secs(p)),
    }
}

/// The line a recognition's cost is worth, or `None`: the first in the process, the first on its
/// thread — one line, the first kind, for a recognition that is both — and after those a new
/// slowest on that thread from 50 ms on that is clearly worse — 1.25 times — than the slowest
/// before it. 50 ms is the host's own threshold for "this held the event loop up"; a poll that
/// reads sixteen times a second would otherwise bury the log it is meant to be evidence in.
/// `beside`: another Vision pass ran in the process at some moment of this recognition's passes.
/// `answered`: who answered it, and what it waited for the neural recogniser. Nothing is formatted,
/// the thread's name included, unless a line is due: this runs after every recognition, on the
/// event loop too.
#[allow(clippy::too_many_arguments)]
pub fn cost_line(
    ms: f64,
    w: i32,
    h: i32,
    thread: impl FnOnce() -> String,
    before: &Before,
    slowest_before: Option<f64>,
    beside: bool,
    answered: Answered,
) -> Option<String> {
    let slowest = matches!(slowest_before, Some(p) if ms >= 50.0 && ms > p * 1.25);
    if !(before.first_in_process || before.first_on_thread || slowest) {
        return None;
    }
    let thread = thread();
    let pause = pause_text(before);
    let beside = if beside { "; another Vision pass ran beside it" } else { "" };
    let by = answered.text();
    Some(if before.first_in_process {
        format!(
            "ocr: the first recognition in this process, a {w}x{h} pt region on {thread}, took {ms:.0} ms{by}; \
             {pause}{beside}"
        )
    } else if before.first_on_thread {
        format!("ocr: the first recognition on {thread}, a {w}x{h} pt region, took {ms:.0} ms{by}; {pause}{beside}")
    } else {
        format!(
            "ocr: reading a {w}x{h} pt region took {ms:.0} ms on {thread}{by}, the slowest there so far; {pause}{beside}"
        )
    })
}

/// The line a slow read is worth: its time and who answered it, then its capture, each Vision pass
/// with its rung, its milliseconds, its words, whether another pass ran beside it and whether the
/// neural recogniser did, the read's wait for the neural recogniser, and what the rest took — the
/// crop, the enlargement, the pixels read back.
pub fn slow_line(
    (w, h): (i32, i32),
    ms: f64,
    thread: &str,
    capture: Capture,
    passes: &[Pass],
    answered: Answered,
) -> String {
    let mut parts = Vec::with_capacity(passes.len() + 3);
    let mut counted = 0.0;
    match capture {
        Capture::Took(c) => {
            parts.push(format!("capture {c:.0} ms"));
            counted += c;
        }
        Capture::Shared => parts.push("captured with the call's other regions".to_string()),
        Capture::Apart => parts.push("picture taken apart".to_string()),
    }
    if passes.is_empty() {
        parts.push("no Vision pass".to_string());
    }
    for p in passes {
        counted += p.ms;
        let words = match p.words {
            None => "refused".to_string(),
            Some(1) => "1 word".to_string(),
            Some(n) => format!("{n} words"),
        };
        let beside = if p.beside { ", another pass beside it" } else { "" };
        let paddle = if p.paddle_beside { ", the neural recogniser beside it" } else { "" };
        parts.push(format!("{} {:.0} ms, {words}{beside}{paddle}", p.stage.word(), p.ms));
    }
    if let Some(check) = answered.check {
        counted += check.ms;
        parts.push(format!("the wait for paddle's check {:.0} ms, {}", check.ms, check.which()));
    }
    if let Some(wait) = answered.waited {
        counted += wait.ms;
        parts.push(format!("the wait for paddle {:.0} ms, {}", wait.ms, wait.which()));
    }
    parts.push(format!("the rest {:.0} ms", (ms - counted).max(0.0)));
    format!(
        "ocr: a slow read, a {w}x{h} pt region on {thread} in {ms:.0} ms, answered by {}: {}",
        answered.path.word(),
        parts.join("; ")
    )
}

/// Whether a line for `key` is due: none was said for it within the last `every`. Records it
/// when it is, and forgets every key said longer ago than that, so the map only ever holds what
/// was said recently.
pub fn due<K: Eq + Hash>(said: &mut HashMap<K, Instant>, key: K, now: Instant, every: Duration) -> bool {
    said.retain(|_, t| now.saturating_duration_since(*t) < every);
    if said.contains_key(&key) {
        return false;
    }
    said.insert(key, now);
    true
}

/// Text with its white space collapsed to single spaces and trimmed, for comparing a reading
/// with what a picture says.
fn collapsed(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How many of `expected`'s words `read` has, each counted once however often it is there: a
/// letter misread is one word wrong, not the line.
fn words_right(read: &str, expected: &str) -> usize {
    let mut left: Vec<&str> = read.split_whitespace().collect();
    expected
        .split_whitespace()
        .filter(|w| match left.iter().position(|r| r == w) {
            Some(i) => {
                left.swap_remove(i);
                true
            }
            None => false,
        })
        .count()
}

/// The warm-up's line, which is also a self-test: its time and thread, and whether Vision read the
/// words of its test line — all of them right, some of them, or none at all. `read` is `None` when
/// Vision refused the request (`run_vision` has said why). `waited`: the milliseconds this warm-up
/// waited for the one before it ([`Gate`]), said when there were any.
///
/// Reading nothing is the case it exists for. A report of macOS 27 has Vision's accurate model
/// returning nothing, without an error, on every request; with this line the first one says so at
/// the start of the session, instead of every read in it coming back empty without a word. Only
/// that is said as an alarm: a word misread in a line of small print is the recogniser at work,
/// and the line says how many it read right.
pub fn warm_up_line(ms: f64, thread: &str, read: Option<&str>, expected: &str, beside: bool, waited: Option<f64>) -> String {
    let mut tail = String::new();
    if let Some(w) = waited.filter(|w| *w >= 1.0) {
        tail.push_str(&format!("; it waited {w:.0} ms for the warm-up before it"));
    }
    if beside {
        tail.push_str("; another Vision pass ran beside it");
    }
    let want = collapsed(expected);
    match read.map(collapsed) {
        None => format!("ocr: Vision did not warm up on {thread}: the request failed after {ms:.0} ms{tail}"),
        Some(t) if t == want => format!("ocr: Vision warmed up in {ms:.0} ms on {thread} and read its test line right{tail}"),
        Some(t) if t.is_empty() => format!(
            "ocr: Vision warmed up in {ms:.0} ms on {thread} but read NOTHING in its test line, a line of \
             printed words — its recogniser answers empty on this Mac, and the reads of this session will \
             most likely come back empty too{tail}"
        ),
        Some(t) => format!(
            "ocr: Vision warmed up in {ms:.0} ms on {thread} and read {} of its test line's {} words right: \
             \"{t}\" where it says \"{want}\"{tail}",
            words_right(&t, &want),
            want.split_whitespace().count()
        ),
    }
}

/// The line of an Intel Mac's warm-up of Vision's fast level, one pass over the same test line
/// right after the accurate one, on the same thread (`backend/macos/ocr.rs`): its time — what an
/// Intel Mac's first pass of a read, the fast level's checked by the neural recogniser, would
/// otherwise pay on top — or that Vision refused it (`refused`). What it read is not judged: the
/// accurate pass's line is the self-test.
pub fn fast_warm_up_line(ms: f64, thread: &str, refused: bool) -> String {
    if refused {
        format!("ocr: Vision's fast level did not warm up on {thread}: the request failed after {ms:.0} ms")
    } else {
        format!("ocr: Vision's fast level warmed up in {ms:.0} ms on {thread}, for an Intel Mac's first pass of a read")
    }
}

/// The warm-ups' turns: the recognise thread's pass waits until the one on a thread of its own has
/// ended (`backend/macos/ocr.rs`). Side by side, as they ran at first, both lines said "another
/// Vision pass ran beside it" and neither timed a first pass by itself; one after the other, the
/// first times the first pass in the process and the second a first pass on another thread of a
/// process already warm — whether a first pass is a cost per process or per thread, the question
/// the September logs left open, from the log of an ordinary session. Not a timer: the second
/// waits for the first to say it has ended, and a first that was never started is not waited for.
pub struct Gate {
    taken: Mutex<bool>,
    ended: Condvar,
}

/// The first warm-up's turn, ended when this is dropped — after its pass, or when its thread
/// unwinds or could not be started.
pub struct Turn<'a>(&'a Gate);

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        *self.0.taken.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.0.ended.notify_all();
    }
}

impl Gate {
    pub const fn new() -> Gate {
        Gate { taken: Mutex::new(false), ended: Condvar::new() }
    }

    /// The first warm-up takes its turn, before its thread is started; the turn ends when the
    /// returned guard is dropped.
    pub fn take(&self) -> Turn<'_> {
        *self.taken.lock().unwrap_or_else(|e| e.into_inner()) = true;
        Turn(self)
    }

    /// Waits until no warm-up has the turn — at once when none took it — and says how long it
    /// waited.
    pub fn wait(&self) -> Duration {
        let began = Instant::now();
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        while *taken {
            taken = self.ended.wait(taken).unwrap_or_else(|e| e.into_inner());
        }
        began.elapsed()
    }
}

impl Default for Gate {
    fn default() -> Gate {
        Gate::new()
    }
}

/// A signal that happens once, and a wait for it that the exit can call off.
///
/// On a Mac the neural recogniser warms up after Vision's warm-up has ended rather than beside it
/// (`backend::warmup_ocr`): two engines warming at once on the two to four cores of an Intel Mac
/// slow each other down, as two Vision passes at once did (2.3 times each on the CI's Intel Mac).
/// Vision's warm-up holds an [`Opener`], which opens the latch when its pass ends, when its thread
/// unwinds, and when the thread could not be started at all; the neural recogniser's warm-up waits
/// with [`wait_or_stopped`](Latch::wait_or_stopped). And the exit calls [`stop`](Latch::stop)
/// before it joins that warm-up, so that a Vision warm-up which never ends — macOS 27 has been
/// reported to hang in its first request — cannot hold the exit. Not a timer: it opens when
/// something has ended, or is stopped when the application quits.
pub struct Latch {
    /// Whether it was opened, and whether it was stopped.
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

/// Opens its latch when dropped.
pub struct Opener<'a>(&'a Latch);

impl Drop for Opener<'_> {
    fn drop(&mut self) {
        self.0.open();
    }
}

impl Latch {
    pub const fn new() -> Latch {
        Latch { state: Mutex::new((false, false)), changed: Condvar::new() }
    }

    /// Opened: every wait returns true from now on, unless it was stopped.
    pub fn open(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).0 = true;
        self.changed.notify_all();
    }

    /// Stopped: every wait returns false from now on, opened or not — a stop is the exit, and
    /// whatever waited for the signal is no longer wanted.
    pub fn stop(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
        self.changed.notify_all();
    }

    /// Opens the latch when the returned guard is dropped.
    pub fn opener(&self) -> Opener<'_> {
        Opener(self)
    }

    /// Waits until the latch is opened or stopped, at once when it already is: true when it was
    /// opened and not stopped.
    pub fn wait_or_stopped(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while !state.0 && !state.1 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        !state.1
    }
}

impl Default for Latch {
    fn default() -> Latch {
        Latch::new()
    }
}

/// The `[env]` line `cpu`: the processor's name, its cores and threads, and on a Mac with two kinds
/// of core how many of each. Any part a platform could not tell is `?`. On both platforms: the
/// processor is half of what a recognition costs, and a log that does not name it cannot be held
/// against another.
pub fn cpu_text(brand: Option<&str>, cores: Option<u32>, threads: Option<u32>, levels: Option<(u32, u32)>) -> String {
    let n = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_else(|| "?".to_string());
    let brand = brand.map(str::trim).filter(|b| !b.is_empty()).unwrap_or("?");
    let levels = match levels {
        Some((p, e)) => format!(" ({p} performance, {e} efficiency)"),
        None => String::new(),
    };
    format!("\"{brand}\", {} cores{levels}, {} threads", n(cores), n(threads))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(stage: Stage, ms: f64, words: Option<usize>, beside: bool) -> Pass {
        Pass { stage, ms, words, beside, paddle_beside: false }
    }

    #[test]
    fn every_stage_has_its_own_word() {
        let all = [
            Stage::AsCaptured,
            Stage::Tight,
            Stage::Whole,
            Stage::Enlarged,
            Stage::Fast,
            Stage::FastFirst,
            Stage::TightAgain,
            Stage::FastCompared,
            Stage::WarmUp,
        ];
        let words: std::collections::HashSet<&str> = all.iter().map(|s| s.word()).collect();
        assert_eq!(words.len(), all.len());
        assert_eq!(Stage::Tight.word(), "tight crop");
        assert_eq!(Stage::Whole.word(), "whole region");
    }

    /// One pass alone has no company; two that overlap both say so, whichever began first and
    /// whichever ends first; one after another, neither.
    #[test]
    fn a_pass_knows_whether_another_ran_beside_it() {
        let r = Running::new();
        let a = r.enter();
        assert!(!r.leave(a), "alone");
        let b = r.enter();
        assert!(!r.leave(b), "after another, not beside it");

        // Nested: b begins and ends inside a.
        let a = r.enter();
        let b = r.enter();
        assert!(r.leave(b), "b began while a ran");
        assert!(r.leave(a), "b began while a ran, and a was told");

        // Crossed: a ends first.
        let a = r.enter();
        let b = r.enter();
        assert!(r.leave(a));
        assert!(r.leave(b));

        // And the count is back to nothing: the next pass alone is alone.
        let c = r.enter();
        assert!(r.busy(), "busy while a pass runs");
        assert!(!r.leave(c));
        assert!(!r.busy(), "and idle after it");
    }

    /// Two real threads: each pass holds the other's start inside it, so both are told.
    #[test]
    fn passes_on_two_threads_at_once_are_both_told() {
        let r = std::sync::Arc::new(Running::new());
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let run = |r: std::sync::Arc<Running>, b: std::sync::Arc<std::sync::Barrier>| {
            std::thread::spawn(move || {
                let e = r.enter();
                b.wait(); // both are inside now
                b.wait();
                r.leave(e)
            })
        };
        let one = run(r.clone(), barrier.clone());
        let two = run(r.clone(), barrier.clone());
        assert!(one.join().unwrap() && two.join().unwrap());
    }

    #[test]
    fn the_threads_are_named_by_role_or_name() {
        assert_eq!(thread_word(Some("main")), "the event loop");
        assert_eq!(thread_word(Some("ocr-recognise")), "thread ocr-recognise");
        assert_eq!(thread_word(None), "an unnamed thread");
    }

    #[test]
    fn the_pause_is_said_in_seconds_for_the_process_and_the_thread() {
        let mut b = Before::default();
        assert_eq!(pause_text(&b), "no Vision pass ran before it in this process");
        b.since_process = Some(Duration::from_millis(703_240));
        assert_eq!(
            pause_text(&b),
            "the last Vision pass in this process ended 703.2 s before it, none ran on this thread"
        );
        b.since_thread = Some(Duration::from_millis(64_000));
        assert_eq!(
            pause_text(&b),
            "the last Vision pass in this process ended 703.2 s before it, the last on this thread 64.0 s before"
        );
        // A pass of another thread was running when it began: Vision was not idle, whatever the
        // last pass to END says.
        b.busy_at_start = true;
        assert_eq!(
            pause_text(&b),
            "another Vision pass in this process was running when it began, the last on this thread 64.0 s before"
        );
        b.since_thread = None;
        assert_eq!(pause_text(&b), "another Vision pass in this process was running when it began, none ran on this thread");
    }

    fn loop_word() -> String {
        "the event loop".to_string()
    }

    fn by(path: Path) -> Answered {
        Answered { path, check: None, waited: None }
    }

    /// The first in the process and the first on a thread are two statements now: the old line
    /// said "Vision's model load included" for the first on every thread, and a 2.4 s read on the
    /// recognise thread, minutes after the warm-up, was taken for a model load. Each says who
    /// answered.
    #[test]
    fn firsts_are_told_apart_and_the_slowest_is_said_only_when_clearly_worse() {
        let first = Before {
            first_in_process: true,
            first_on_thread: true,
            since_process: Some(Duration::from_secs(703)),
            since_thread: None,
            busy_at_start: false,
        };
        let line = cost_line(1538.4, 123, 23, loop_word, &first, None, false, by(Path::Vision)).unwrap();
        assert_eq!(
            line,
            "ocr: the first recognition in this process, a 123x23 pt region on the event loop, took 1538 ms, answered \
             by Vision; the last Vision pass in this process ended 703.0 s before it, none ran on this thread"
        );
        assert!(!line.contains("model load"));

        let thread_first = Before { first_in_process: false, ..first };
        assert_eq!(
            cost_line(2412.6, 174, 72, || "thread ocr-recognise".to_string(), &thread_first, None, false, by(Path::Nothing))
                .unwrap(),
            "ocr: the first recognition on thread ocr-recognise, a 174x72 pt region, took 2413 ms, answered by nobody; \
             the last Vision pass in this process ended 703.0 s before it, none ran on this thread"
        );
        // Begun while another thread's pass ran, and joined by one: said, both.
        let busy = Before { busy_at_start: true, ..thread_first };
        let paddle = Answered { waited: Some(PaddleWait { ms: 2.04, ended: Waited::Answered }), ..by(Path::Paddle) };
        assert_eq!(
            cost_line(2412.6, 174, 72, || "thread ocr-recognise".to_string(), &busy, None, true, paddle).unwrap(),
            "ocr: the first recognition on thread ocr-recognise, a 174x72 pt region, took 2413 ms, answered by Paddle \
             alone + waited 2.0ms for paddle (which answered); another Vision pass in this process was running when it \
             began, none ran on this thread; another Vision pass ran beside it"
        );

        let later = Before {
            first_in_process: false,
            first_on_thread: false,
            since_process: Some(Duration::from_millis(1500)),
            since_thread: Some(Duration::from_millis(1500)),
            busy_at_start: false,
        };
        // No line due: not even the thread's name is asked for.
        let unasked = || -> String { panic!("the thread's name formatted for no line") };
        assert_eq!(cost_line(200.0, 40, 20, unasked, &later, Some(180.0), false, by(Path::Vision)), None, "not 1.25 times worse");
        assert_eq!(cost_line(45.0, 40, 20, unasked, &later, Some(10.0), true, by(Path::Vision)), None, "under 50 ms");
        assert_eq!(
            cost_line(300.0, 40, 20, loop_word, &later, Some(200.0), false, by(Path::Agreed)).unwrap(),
            "ocr: reading a 40x20 pt region took 300 ms on the event loop, answered by the fast level and Paddle \
             agreeing, the slowest there so far; the last Vision pass in this process ended 1.5 s before it, the last \
             on this thread 1.5 s before"
        );
    }

    /// The wait for the neural recogniser in Windows' words, and the bounded wait's own end.
    #[test]
    fn the_wait_for_the_recogniser_is_said_as_windows_says_it() {
        assert_eq!(waited_text(None), "");
        let w = |ms: f64, ended: Waited| waited_text(Some(PaddleWait { ms, ended }));
        assert_eq!(w(1.23, Waited::Answered), " + waited 1.2ms for paddle (which answered)");
        assert_eq!(w(0.0, Waited::Nothing), " + waited 0.0ms for paddle (which had nothing)");
        assert_eq!(w(14.0, Waited::NotYet), " + waited 14.0ms for paddle (which had not answered by then)");
        // The two Windows says, word for word (`windows.rs`, `recognize_image`).
        const WINDOWS: &str = include_str!("../backend/windows.rs");
        assert!(WINDOWS.contains("\" + waited {wait_ms:.1}ms for paddle ({})\""), "Windows' fragment");
        assert!(WINDOWS.contains("\"which answered\"") && WINDOWS.contains("\"which had nothing\""));
    }

    /// An Intel Mac's check has its own fragment, before Windows' wait: a check that read
    /// otherwise is never said as "which answered", and one that timed out is still said when the
    /// wait after the ladder found the answer.
    #[test]
    fn the_checks_wait_is_said_apart_from_windows_wait() {
        assert_eq!(check_text(None), "");
        let c = |ms: f64, ended: Check| Some(CheckWait { ms, ended });
        assert_eq!(check_text(c(9.84, Check::Same)), " + waited 9.8ms for paddle's check (which read the same)");
        assert_eq!(check_text(c(9.8, Check::Otherwise)), " + waited 9.8ms for paddle's check (which read otherwise)");
        assert_eq!(check_text(c(3.0, Check::Nothing)), " + waited 3.0ms for paddle's check (which had nothing)");
        assert_eq!(check_text(c(14.0, Check::NotYet)), " + waited 14.0ms for paddle's check (which had not answered by then)");
        let late = Answered {
            path: Path::Paddle,
            check: c(14.0, Check::NotYet),
            waited: Some(PaddleWait { ms: 0.2, ended: Waited::Answered }),
        };
        assert_eq!(
            late.waits_text(),
            " + waited 14.0ms for paddle's check (which had not answered by then) + waited 0.2ms for paddle (which answered)"
        );
        let differed = Answered { path: Path::Vision, check: c(9.8, Check::Otherwise), waited: None };
        assert_eq!(differed.waits_text(), " + waited 9.8ms for paddle's check (which read otherwise)");
        assert!(!differed.waits_text().contains("which answered"));
    }

    #[test]
    fn a_slow_read_is_said_part_by_part() {
        let passes = [pass(Stage::Tight, 180.4, Some(0), false), pass(Stage::Whole, 210.0, Some(1), true)];
        assert_eq!(
            slow_line((123, 23), 448.2, "the event loop", Capture::Took(49.0), &passes, by(Path::Vision)),
            "ocr: a slow read, a 123x23 pt region on the event loop in 448 ms, answered by Vision: capture 49 ms; \
             tight crop 180 ms, 0 words; whole region 210 ms, 1 word, another pass beside it; the rest 9 ms"
        );
        assert_eq!(
            slow_line(
                (174, 72),
                357.0,
                "thread ocr-recognise",
                Capture::Apart,
                &[pass(Stage::Tight, 350.0, Some(3), false)],
                by(Path::Vision)
            ),
            "ocr: a slow read, a 174x72 pt region on thread ocr-recognise in 357 ms, answered by Vision: picture taken \
             apart; tight crop 350 ms, 3 words; the rest 7 ms"
        );
        assert_eq!(
            slow_line((40, 20), 120.0, "the event loop", Capture::Shared, &[pass(Stage::Fast, 130.0, None, false)], by(Path::Nothing)),
            "ocr: a slow read, a 40x20 pt region on the event loop in 120 ms, answered by nobody: captured with the \
             call's other regions; fast model 130 ms, refused; the rest 0 ms",
            "rounding never makes the rest negative"
        );
        assert!(slow_line((40, 20), 150.0, "the event loop", Capture::Took(150.0), &[], by(Path::Nothing))
            .contains("capture 150 ms; no Vision pass"));
        // The neural recogniser beside a pass, and the fast pass made only for its counts named as
        // what it is.
        let counted = [
            Pass { paddle_beside: true, ..pass(Stage::Tight, 290.0, Some(1), false) },
            pass(Stage::FastCompared, 14.0, Some(1), false),
        ];
        assert_eq!(
            slow_line((40, 20), 330.0, "thread ocr-recognise", Capture::Apart, &counted, by(Path::Vision)),
            "ocr: a slow read, a 40x20 pt region on thread ocr-recognise in 330 ms, answered by Vision: picture taken \
             apart; tight crop 290 ms, 1 word, the neural recogniser beside it; fast model, only compared 14 ms, 1 \
             word; the rest 26 ms"
        );
        // An Intel Mac's check that failed, the accurate ladder, and the recogniser's answer, waited
        // for at its end: each wait is a part of its own.
        let checked = [
            pass(Stage::FastFirst, 12.0, Some(1), false),
            Pass { paddle_beside: true, ..pass(Stage::Tight, 290.0, Some(0), false) },
            pass(Stage::Whole, 300.0, Some(0), false),
        ];
        let paddle = Answered {
            path: Path::Paddle,
            check: Some(CheckWait { ms: 12.0, ended: Check::NotYet }),
            waited: Some(PaddleWait { ms: 3.2, ended: Waited::Answered }),
        };
        assert_eq!(
            slow_line((20, 20), 624.0, "the event loop", Capture::Took(4.0), &checked, paddle),
            "ocr: a slow read, a 20x20 pt region on the event loop in 624 ms, answered by Paddle alone: capture 4 ms; \
             fast model, first 12 ms, 1 word; tight crop 290 ms, 0 words, the neural recogniser beside it; whole \
             region 300 ms, 0 words; the wait for paddle's check 12 ms, which had not answered by then; the wait for \
             paddle 3 ms, which answered; the rest 3 ms"
        );
    }

    /// One line per key per interval; another key is its own; an old key is forgotten.
    #[test]
    fn a_line_is_due_once_per_key_and_interval() {
        let every = Duration::from_secs(10);
        let t0 = Instant::now();
        let mut said = HashMap::new();
        assert!(due(&mut said, (40, 20), t0, every));
        assert!(!due(&mut said, (40, 20), t0 + Duration::from_secs(9), every));
        assert!(due(&mut said, (123, 23), t0 + Duration::from_secs(9), every), "another key is its own");
        assert!(due(&mut said, (40, 20), t0 + Duration::from_secs(10), every), "due again after the interval");
        assert!(!due(&mut said, (40, 20), t0 + Duration::from_secs(11), every));
        let _ = due(&mut said, (1, 1), t0 + Duration::from_secs(60), every);
        assert_eq!(said.len(), 1, "everything older than the interval was forgotten");
    }

    #[test]
    fn the_warm_up_says_whether_it_read_its_test_line() {
        let want = "Instrument Polyphony Pitchbend";
        assert_eq!(
            warm_up_line(1673.2, "thread ocr-warm-up", Some("Instrument  Polyphony\nPitchbend"), want, false, None),
            "ocr: Vision warmed up in 1673 ms on thread ocr-warm-up and read its test line right"
        );
        let nothing = warm_up_line(210.0, "thread ocr-recognise", Some(" "), want, true, None);
        assert!(nothing.starts_with("ocr: Vision warmed up in 210 ms on thread ocr-recognise but read NOTHING"), "{nothing}");
        assert!(nothing.ends_with("; another Vision pass ran beside it"), "{nothing}");
        // A letter misread is one word of three, not an alarm.
        let one_off = warm_up_line(300.0, "thread ocr-warm-up", Some("lnstrument Polyphony Pitchbend"), want, false, None);
        assert_eq!(
            one_off,
            "ocr: Vision warmed up in 300 ms on thread ocr-warm-up and read 2 of its test line's 3 words right: \
             \"lnstrument Polyphony Pitchbend\" where it says \"Instrument Polyphony Pitchbend\""
        );
        assert!(!one_off.contains("NOTHING"));
        // Words in another order are the words; a word twice counts once.
        assert!(warm_up_line(1.0, "t", Some("Pitchbend Instrument"), want, false, None).contains("read 2 of its"));
        assert!(warm_up_line(1.0, "t", Some("Pitchbend Pitchbend"), want, false, None).contains("read 1 of its"));
        assert_eq!(
            warm_up_line(5.0, "thread ocr-warm-up", None, want, false, None),
            "ocr: Vision did not warm up on thread ocr-warm-up: the request failed after 5 ms"
        );
        // The wait for the warm-up before it, when there was one.
        assert_eq!(
            warm_up_line(160.0, "thread ocr-recognise", Some(want), want, false, Some(1650.4)),
            "ocr: Vision warmed up in 160 ms on thread ocr-recognise and read its test line right; it waited \
             1650 ms for the warm-up before it"
        );
        assert!(!warm_up_line(160.0, "t", Some(want), want, false, Some(0.2)).contains("waited"), "no wait to speak of");
        // An Intel Mac's fast level, warmed after it.
        assert_eq!(
            fast_warm_up_line(63.4, "thread ocr-warm-up", false),
            "ocr: Vision's fast level warmed up in 63 ms on thread ocr-warm-up, for an Intel Mac's first pass of a read"
        );
        assert_eq!(
            fast_warm_up_line(2.0, "thread ocr-recognise", true),
            "ocr: Vision's fast level did not warm up on thread ocr-recognise: the request failed after 2 ms"
        );
    }

    /// The second warm-up waits for the first to end, however it ends — its pass done, or its
    /// thread unwound — and not at all when there was no first.
    #[test]
    fn the_second_warm_up_waits_for_the_first_to_end() {
        let gate = Gate::new();
        assert!(gate.wait() < Duration::from_millis(50), "no first warm-up: no wait");

        static GATE: Gate = Gate::new();
        let turn = GATE.take();
        let first = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            drop(turn);
        });
        let waited = GATE.wait();
        assert!(waited >= Duration::from_millis(60), "{waited:?}");
        first.join().unwrap();

        crate::quiet_expected_panics();
        let turn = GATE.take();
        let fell = std::thread::spawn(move || {
            let _turn = turn;
            panic!("{}inside the first warm-up", crate::EXPECTED_PANIC);
        });
        assert!(fell.join().is_err());
        assert!(GATE.wait() < Duration::from_millis(50), "a first warm-up that unwound ended its turn");
    }

    /// Open: a wait returns true, at once when it was opened before; stopped: false, opened or
    /// not; and a waiter on another thread is woken by either.
    #[test]
    fn the_latch_opens_or_is_stopped() {
        let open = Latch::new();
        open.open();
        assert!(open.wait_or_stopped(), "opened before the wait: at once, and true");
        assert!(open.wait_or_stopped(), "and again");

        let stopped = Latch::new();
        stopped.stop();
        assert!(!stopped.wait_or_stopped());
        stopped.open();
        assert!(!stopped.wait_or_stopped(), "a stop is the exit: opening after it changes nothing");
        let late = Latch::new();
        late.open();
        late.stop();
        assert!(!late.wait_or_stopped(), "nor does opening before it");

        static WAITED: Latch = Latch::new();
        let waiter = std::thread::spawn(|| WAITED.wait_or_stopped());
        std::thread::sleep(Duration::from_millis(30));
        drop(WAITED.opener());
        assert!(waiter.join().unwrap(), "an opener dropped opens it");

        static CALLED_OFF: Latch = Latch::new();
        let waiter = std::thread::spawn(|| CALLED_OFF.wait_or_stopped());
        std::thread::sleep(Duration::from_millis(30));
        CALLED_OFF.stop();
        assert!(!waiter.join().unwrap(), "the exit calls the wait off");

        // An opener held by a thread that unwinds opens it too.
        crate::quiet_expected_panics();
        static UNWOUND: Latch = Latch::new();
        let opener = UNWOUND.opener();
        let fell = std::thread::spawn(move || {
            let _opener = opener;
            panic!("{}inside Vision's warm-up", crate::EXPECTED_PANIC);
        });
        assert!(fell.join().is_err());
        assert!(UNWOUND.wait_or_stopped());
    }

    #[test]
    fn the_processor_line_says_what_it_knows() {
        assert_eq!(
            cpu_text(Some("Intel(R) Core(TM) i5-1030NG7 CPU @ 1.10GHz "), Some(4), Some(8), None),
            "\"Intel(R) Core(TM) i5-1030NG7 CPU @ 1.10GHz\", 4 cores, 8 threads"
        );
        assert_eq!(cpu_text(Some("Apple M1"), Some(8), Some(8), Some((4, 4))), "\"Apple M1\", 8 cores (4 performance, 4 efficiency), 8 threads");
        assert_eq!(cpu_text(None, None, Some(12), None), "\"?\", ? cores, 12 threads");
        assert_eq!(cpu_text(Some("  "), Some(6), None, None), "\"?\", 6 cores, ? threads");
    }
}
