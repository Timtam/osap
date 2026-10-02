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
//! take turns ([`Gate`]), so that their two lines time two first passes apart.
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
    /// The warm-up's own pass over its test line.
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
/// Nothing is formatted, the thread's name included, unless a line is due: this runs after every
/// recognition, on the event loop too.
pub fn cost_line(
    ms: f64,
    w: i32,
    h: i32,
    thread: impl FnOnce() -> String,
    before: &Before,
    slowest_before: Option<f64>,
    beside: bool,
) -> Option<String> {
    let slowest = matches!(slowest_before, Some(p) if ms >= 50.0 && ms > p * 1.25);
    if !(before.first_in_process || before.first_on_thread || slowest) {
        return None;
    }
    let thread = thread();
    let pause = pause_text(before);
    let beside = if beside { "; another Vision pass ran beside it" } else { "" };
    Some(if before.first_in_process {
        format!(
            "ocr: the first recognition in this process, a {w}x{h} pt region on {thread}, took {ms:.0} ms; {pause}{beside}"
        )
    } else if before.first_on_thread {
        format!("ocr: the first recognition on {thread}, a {w}x{h} pt region, took {ms:.0} ms; {pause}{beside}")
    } else {
        format!("ocr: reading a {w}x{h} pt region took {ms:.0} ms on {thread}, the slowest there so far; {pause}{beside}")
    })
}

/// The line a slow read is worth: its time, then its capture, each Vision pass with its rung, its
/// milliseconds, its words and whether another pass ran beside it, and what the rest took — the
/// crop, the enlargement, the pixels read back.
pub fn slow_line(w: i32, h: i32, ms: f64, thread: &str, capture: Capture, passes: &[Pass]) -> String {
    let mut parts = Vec::with_capacity(passes.len() + 2);
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
        parts.push(format!("{} {:.0} ms, {words}{beside}", p.stage.word(), p.ms));
    }
    parts.push(format!("the rest {:.0} ms", (ms - counted).max(0.0)));
    format!("ocr: a slow read, a {w}x{h} pt region on {thread} in {ms:.0} ms: {}", parts.join("; "))
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
        Pass { stage, ms, words, beside }
    }

    #[test]
    fn every_stage_has_its_own_word() {
        let all = [Stage::AsCaptured, Stage::Tight, Stage::Whole, Stage::Enlarged, Stage::Fast, Stage::WarmUp];
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

    /// The first in the process and the first on a thread are two statements now: the old line
    /// said "Vision's model load included" for the first on every thread, and a 2.4 s read on the
    /// recognise thread, minutes after the warm-up, was taken for a model load.
    #[test]
    fn firsts_are_told_apart_and_the_slowest_is_said_only_when_clearly_worse() {
        let first = Before {
            first_in_process: true,
            first_on_thread: true,
            since_process: Some(Duration::from_secs(703)),
            since_thread: None,
            busy_at_start: false,
        };
        let line = cost_line(1538.4, 123, 23, loop_word, &first, None, false).unwrap();
        assert_eq!(
            line,
            "ocr: the first recognition in this process, a 123x23 pt region on the event loop, took 1538 ms; \
             the last Vision pass in this process ended 703.0 s before it, none ran on this thread"
        );
        assert!(!line.contains("model load"));

        let thread_first = Before { first_in_process: false, ..first };
        assert_eq!(
            cost_line(2412.6, 174, 72, || "thread ocr-recognise".to_string(), &thread_first, None, false).unwrap(),
            "ocr: the first recognition on thread ocr-recognise, a 174x72 pt region, took 2413 ms; \
             the last Vision pass in this process ended 703.0 s before it, none ran on this thread"
        );
        // Begun while another thread's pass ran, and joined by one: said, both.
        let busy = Before { busy_at_start: true, ..thread_first };
        assert_eq!(
            cost_line(2412.6, 174, 72, || "thread ocr-recognise".to_string(), &busy, None, true).unwrap(),
            "ocr: the first recognition on thread ocr-recognise, a 174x72 pt region, took 2413 ms; \
             another Vision pass in this process was running when it began, none ran on this thread; \
             another Vision pass ran beside it"
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
        assert_eq!(cost_line(200.0, 40, 20, unasked, &later, Some(180.0), false), None, "not 1.25 times worse");
        assert_eq!(cost_line(45.0, 40, 20, unasked, &later, Some(10.0), true), None, "under 50 ms");
        assert_eq!(
            cost_line(300.0, 40, 20, loop_word, &later, Some(200.0), false).unwrap(),
            "ocr: reading a 40x20 pt region took 300 ms on the event loop, the slowest there so far; \
             the last Vision pass in this process ended 1.5 s before it, the last on this thread 1.5 s before"
        );
    }

    #[test]
    fn a_slow_read_is_said_part_by_part() {
        let passes = [pass(Stage::Tight, 180.4, Some(0), false), pass(Stage::Whole, 210.0, Some(1), true)];
        assert_eq!(
            slow_line(123, 23, 448.2, "the event loop", Capture::Took(49.0), &passes),
            "ocr: a slow read, a 123x23 pt region on the event loop in 448 ms: capture 49 ms; \
             tight crop 180 ms, 0 words; whole region 210 ms, 1 word, another pass beside it; the rest 9 ms"
        );
        assert_eq!(
            slow_line(174, 72, 357.0, "thread ocr-recognise", Capture::Apart, &[pass(Stage::Tight, 350.0, Some(3), false)]),
            "ocr: a slow read, a 174x72 pt region on thread ocr-recognise in 357 ms: picture taken apart; \
             tight crop 350 ms, 3 words; the rest 7 ms"
        );
        assert_eq!(
            slow_line(40, 20, 120.0, "the event loop", Capture::Shared, &[pass(Stage::Fast, 130.0, None, false)]),
            "ocr: a slow read, a 40x20 pt region on the event loop in 120 ms: captured with the call's other \
             regions; fast model 130 ms, refused; the rest 0 ms",
            "rounding never makes the rest negative"
        );
        assert!(slow_line(40, 20, 150.0, "the event loop", Capture::Took(150.0), &[]).contains("capture 150 ms; no Vision pass"));
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
