//! The guard that keeps text recognition off the event loop.
//!
//! The rule is the maintainer's: no text recognition may burden the event loop, which carries
//! every hotkey, every captured key, speech, every module callback and, on a Mac, the keyboard's
//! event tap. `host.ocr.recognize` recognises on threads of its own, with a callback or in a
//! handler waiting for those threads (task.rs), and raises where it cannot wait: no way onto the
//! loop is left for it.
//!
//! So every function that photographs for a read or recognises asks, first thing, whether it
//! runs on the event loop (`off_loop`): the platform's `OcrWorker` functions, Windows'
//! `recognize_image`, the neural recogniser's `ask`, and the place a Vision request is performed
//! on a Mac. On the loop, that is a bug in the host: a panic in a debug build, so every test that
//! brings a recogniser onto the loop fails, and in a release build one log line —
//! `[loop] text recognition on the event loop: …` — which CI looks for in the capture probe's log.
//!
//! The loop is a thread, marked once by `Manager::new` on the thread that goes on to run it (the
//! window's tick and the headless loop are the same thread). A thread nobody marked — the OCR
//! threads, `ocr-bench`, every test that did not mark itself — is never the loop.
//!
//! A blocking call — a host call that runs on the loop where it cannot wait (`task::legacy`) —
//! holds `legacy(kind)` while it runs, and only that call: the exception is scoped to the call and
//! to its kind of work, not to the module or the thread. Recognition has no such call any more,
//! and a scope of another kind never lets it be.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

/// The work a guard keeps off the loop, and the work a blocking call's scope lets be there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Text recognition, and the captures made for it (`off_loop`).
    Text,
    /// A screen capture of a `host.screen` call. No call opens a scope of it yet: see
    /// `task::legacy`.
    #[cfg_attr(not(test), allow(dead_code))]
    Screen,
}

impl Kind {
    fn index(self) -> usize {
        match self {
            Kind::Text => 0,
            Kind::Screen => 1,
        }
    }
}

thread_local! {
    /// Whether this thread runs the event loop.
    static LOOP: Cell<bool> = const { Cell::new(false) };
    /// How many blocking calls of each kind are running on this thread, one inside the other at
    /// most when a module's own coroutine called inside one: while any of a kind is, that kind of
    /// work here is that call's.
    static LEGACY: Cell<[u32; 2]> = const { Cell::new([0; 2]) };
}

/// Recognitions on the loop, this session (release builds; a debug build panics at the first).
static VIOLATIONS: AtomicU64 = AtomicU64::new(0);

/// Marks the calling thread as the event loop's, for the rest of its life.
pub fn mark_loop_thread() {
    LOOP.with(|l| l.set(true));
}

/// Whether a blocking call of `kind` runs on this thread now.
fn excused(kind: Kind) -> bool {
    LEGACY.with(Cell::get)[kind.index()] > 0
}

/// Says that `what` — a capture for a read, a recognition — is about to run, and that it must not
/// run on the event loop. Nothing happens anywhere else, or inside a blocking call of
/// [`Kind::Text`], which nothing opens.
///
/// On the loop: a panic in a debug build, and in a release build a log line for the first
/// violation of the session and for every power of two after it, with the count.
pub fn off_loop(what: &str) {
    if !LOOP.with(Cell::get) || excused(Kind::Text) {
        return;
    }
    let n = VIOLATIONS.fetch_add(1, Ordering::Relaxed) + 1;
    let line = violation_line(what, n);
    if cfg!(debug_assertions) {
        panic!("[loop] {line}");
    }
    if n.is_power_of_two() {
        crate::logging::line("loop", &line);
    }
}

/// The words of the violation line, without its `[loop]` scope.
fn violation_line(what: &str, n: u64) -> String {
    format!("text recognition on the event loop: {what} — this is a bug; please report it ({n} so far this session)")
}

/// The scope of one blocking call of a kind: a host call where it could not wait, which does that
/// kind of work on the loop. Ends when dropped, unwinding included.
#[must_use = "the exception ends when this is dropped"]
pub struct Legacy {
    kind: Kind,
}

/// Opens a blocking call's scope of `kind` on the calling thread (see [`Legacy`]).
pub fn legacy(kind: Kind) -> Legacy {
    LEGACY.with(|l| {
        let mut n = l.get();
        n[kind.index()] += 1;
        l.set(n);
    });
    Legacy { kind }
}

impl Drop for Legacy {
    fn drop(&mut self) {
        LEGACY.with(|l| {
            let mut n = l.get();
            n[self.kind.index()] = n[self.kind.index()].saturating_sub(1);
            l.set(n);
        });
    }
}

/// A test's mark of its own thread as the loop's, taken off again when it is dropped: a test
/// that stands where the event loop stands (task_tests.rs) holds one for its whole length, and
/// hands no mark on to a test that runs after it on the same thread.
#[cfg(test)]
#[must_use = "the mark is taken off when this is dropped"]
pub(crate) struct TestLoop {
    _private: (),
}

/// Marks the calling thread as the loop's until the [`TestLoop`] is dropped.
#[cfg(test)]
pub(crate) fn mark_for_a_test() -> TestLoop {
    mark_loop_thread();
    TestLoop { _private: () }
}

#[cfg(test)]
impl Drop for TestLoop {
    fn drop(&mut self) {
        LOOP.with(|l| l.set(false));
    }
}

/// How many violations a release build has counted this session.
#[cfg(all(test, not(debug_assertions)))]
pub fn violations() -> u64 {
    VIOLATIONS.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A thread nobody marked is not the loop: the OCR threads, the bench, a test.
    #[test]
    fn an_unmarked_thread_may_recognise() {
        off_loop("a test's recognition");
    }

    /// A blocking call's scope lasts as long as the call, nested ones included, and covers its own
    /// kind of work only: inside the screen calls' scopes, a recognition on the loop is still the
    /// bug it is everywhere else on it.
    #[test]
    fn a_screen_calls_scope_lasts_while_it_runs_and_never_lets_a_recognition_be() {
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            let mut fired = Vec::new();
            {
                let _outer = legacy(Kind::Screen);
                {
                    let _inner = legacy(Kind::Screen);
                    assert!(excused(Kind::Screen) && !excused(Kind::Text));
                    fired.push(std::panic::catch_unwind(|| off_loop("inside two screen calls")).is_err());
                }
                assert!(excused(Kind::Screen), "the outer call still runs");
            }
            assert!(!excused(Kind::Screen), "outside every screen call");
            fired.push(std::panic::catch_unwind(|| off_loop("after both")).is_err());
            fired
        });
        let fired = t.join().unwrap();
        assert_eq!(fired, [cfg!(debug_assertions); 2], "the text guard fired inside the screen call and after it");
    }

    /// A recognition on the loop panics in a debug build.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "[loop] text recognition on the event loop: a stray recognition — this is a bug")]
    fn a_recognition_on_the_loop_panics_in_a_debug_build() {
        mark_loop_thread();
        off_loop("a stray recognition");
    }

    /// In a release build it is counted and logged instead.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_recognition_on_the_loop_is_counted_in_a_release_build() {
        let before = violations();
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            off_loop("a stray recognition");
        });
        t.join().unwrap();
        assert!(violations() > before);
    }

    /// A test's mark ends with it: the thread is not the loop's afterwards.
    #[test]
    fn a_tests_mark_is_taken_off_when_it_ends() {
        {
            let _mark = mark_for_a_test();
            assert!(LOOP.with(Cell::get));
        }
        assert!(!LOOP.with(Cell::get));
        off_loop("after the mark");
    }

    /// The six places a Mac photographs for a read or recognises begin with the guard. macOS CI
    /// runs the tests of a release build only, where the guard counts instead of panicking, and
    /// Windows never compiles the file — so this reads its source, on every platform.
    #[test]
    fn every_macos_recognition_entry_begins_with_the_guard() {
        const MAC: &str = include_str!("backend/macos/ocr.rs");
        for f in [
            "pub fn frames_for_round(",
            "pub fn shot_of(",
            "pub fn capture_for_read(",
            "pub fn recognise_shot(",
            "pub fn warm_up_recognise(",
            "fn run_vision(",
        ] {
            assert_eq!(MAC.matches(f).count(), 1, "{f}: not exactly one in backend/macos/ocr.rs");
            let at = MAC.find(f).unwrap();
            let open = MAC[at..].find("{\n").expect("a body") + at + 2;
            let first = MAC[open..].lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
            assert!(first.starts_with("crate::loop_guard::off_loop("), "{f} begins with {first:?}");
        }
    }

    #[test]
    fn the_line_is_what_ci_looks_for() {
        let line = format!("[loop] {}", violation_line("Windows.Media.Ocr", 1));
        assert!(line.starts_with("[loop] text recognition on the event loop: Windows.Media.Ocr"), "{line}");
    }
}
