//! The guard that keeps text recognition and screen captures off the event loop.
//!
//! The rule is the maintainer's: no text recognition may burden the event loop, which carries
//! every hotkey, every captured key, speech, every module callback and, on a Mac, the keyboard's
//! event tap — and after it no `host.screen` call either. `host.ocr.recognize` recognises on
//! threads of its own, with a callback or in a handler waiting for those threads (task.rs), and
//! raises where it cannot wait: no way onto the loop is left for it. The `host.screen` calls that
//! read the screen wait in a handler while the `screen-capture` thread or the image worker reads
//! it, and only where they cannot wait do they capture on the loop, for that one call.
//!
//! So every function that photographs for a read or recognises asks, first thing, whether it
//! runs on the event loop (`off_loop`): the platform's `OcrWorker` functions that serve a read,
//! Windows' `recognize_image`, the neural recogniser's `ask`, and the place a Vision request is
//! performed on a Mac. And every function that captures the screen for a `host.screen` call asks
//! the same of the screen: the backend's four reads of the screen on the calling thread
//! (`Backend::pixel`, `pixels`, `capture`, `frame`), which a call that could not wait makes on the
//! loop (`off_loop_screen`), and the ones only a thread of the host's makes, which no such call
//! lets be there (`off_loop_screen_always`) — the `OcrWorker` functions that take a snapshot
//! round's pictures or a handler's points, and the image worker's capture and first-read
//! comparison (`Backend::capture_fn`, `compare_fn`). On the loop, that is a bug in the host: a
//! panic in a debug build, so every test that brings a recogniser or a capture onto the loop
//! fails, and in a release build one log line — `[loop] text recognition on the event loop: …` or
//! `[loop] screen capture on the event loop: …` — which CI looks for in the capture probe's log.
//!
//! The loop is a thread, marked once by `Manager::new` on the thread that goes on to run it (the
//! window's tick and the headless loop are the same thread). A thread nobody marked — the OCR
//! threads, the image worker, `ocr-bench`, every test that did not mark itself — is never the
//! loop.
//!
//! A blocking call — a host call that runs on the loop where it cannot wait (`task::legacy`) —
//! holds `legacy(kind)` while it runs, and only that call: the exception is scoped to the call and
//! to its kind of work, not to the module or the thread. Recognition has no such call any more,
//! and a scope of another kind never lets it be. A `host.screen` call where it cannot wait holds
//! one of `Kind::Screen`, inside which the backend's reads on the calling thread may be on the
//! loop; outside every such scope a capture there is the bug the screen guard names, and a capture
//! thread's or the image worker's is that bug inside one too.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

/// The work a guard keeps off the loop, and the work a blocking call's scope lets be there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Text recognition, and the captures made for it (`off_loop`).
    Text,
    /// A screen capture for a `host.screen` call (`off_loop_screen`), which a call that could not
    /// wait takes on the loop for that one call (`task::legacy`): `snapshot`, `pixel`, `pixels`,
    /// `profile`, the cells calls, `template{ capture }`, the searches, the shots and a snapshot's
    /// `crop` (which copies from its picture and captures nothing), where they cannot wait —
    /// outside every handler, in a function the host calls and waits for, at a place Luau cannot
    /// suspend, or in a coroutine the module made, even inside a handler.
    Screen,
}

impl Kind {
    fn index(self) -> usize {
        match self {
            Kind::Text => 0,
            Kind::Screen => 1,
        }
    }

    /// What the violation line says ran on the loop.
    fn work(self) -> &'static str {
        match self {
            Kind::Text => "text recognition",
            Kind::Screen => "screen capture",
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

/// Violations of each kind on the loop, this session (release builds; a debug build panics at the
/// first).
static VIOLATIONS: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

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
    guard(Kind::Text, what, true);
}

/// Says that `what` — a capture of the screen for a `host.screen` call — is about to run, and that
/// it must not run on the event loop but inside a blocking call of [`Kind::Screen`]: a call that
/// could not wait, whose capture is on the loop for that one call. Nothing happens anywhere else.
///
/// On the loop outside such a call: a panic in a debug build, and in a release build a log line for
/// the first violation of the session and for every power of two after it, with the count — counted
/// apart from the recognitions'.
pub fn off_loop_screen(what: &str) {
    guard(Kind::Screen, what, true);
}

/// Says that `what` — a capture of the screen that only a thread of the host's makes: a capture
/// thread's snapshot round or points, the image worker's capture or first-read comparison — is
/// about to run, and that it must never run on the event loop, not even inside a call that could
/// not wait, whose own reads go through the backend ([`off_loop_screen`]). Nothing happens anywhere
/// else.
///
/// On the loop: the screen guard's panic, line and count.
pub fn off_loop_screen_always(what: &str) {
    guard(Kind::Screen, what, false);
}

/// The guards: `what`, work of `kind`, about to run — let be inside a blocking call of `kind` when
/// it is `excusable`.
fn guard(kind: Kind, what: &str, excusable: bool) {
    if !LOOP.with(Cell::get) || (excusable && excused(kind)) {
        return;
    }
    let n = VIOLATIONS[kind.index()].fetch_add(1, Ordering::Relaxed) + 1;
    let line = violation_line(kind, what, n);
    if cfg!(debug_assertions) {
        panic!("[loop] {line}");
    }
    if n.is_power_of_two() {
        crate::logging::line("loop", &line);
    }
}

/// The words of the violation line, without its `[loop]` scope.
fn violation_line(kind: Kind, what: &str, n: u64) -> String {
    format!("{} on the event loop: {what} — this is a bug; please report it ({n} so far this session)", kind.work())
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

/// How many violations of `kind` a release build has counted this session.
#[cfg(all(test, not(debug_assertions)))]
pub fn violations(kind: Kind) -> u64 {
    VIOLATIONS[kind.index()].load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A thread nobody marked is not the loop: the OCR threads, the image worker, the bench, a test.
    #[test]
    fn an_unmarked_thread_may_recognise_and_capture() {
        off_loop("a test's recognition");
        off_loop_screen("a test's capture");
        off_loop_screen_always("a test's worker capture");
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

    /// A capture on the loop is let be inside a screen call's scope, and only there: before it,
    /// after it, and inside a scope of the other kind it is the bug the screen guard names.
    #[test]
    #[cfg(debug_assertions)]
    fn a_capture_on_the_loop_is_let_be_only_inside_a_screen_calls_scope() {
        crate::quiet_expected_panics();
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            let fired = |what: &'static str| std::panic::catch_unwind(|| off_loop_screen(what)).is_err();
            let before = fired("before the call");
            let inside = {
                let _call = legacy(Kind::Screen);
                let _nested = legacy(Kind::Screen);
                fired("inside the call")
            };
            let after = fired("after the call");
            let text = {
                let _other = legacy(Kind::Text);
                fired("inside a scope of the other kind")
            };
            [before, inside, after, text]
        });
        assert_eq!(t.join().unwrap(), [true, false, true, true], "before, inside, after, inside a text scope");
    }

    /// A capture only a thread of the host's makes is the bug on the loop wherever it is made
    /// there, inside a screen call's scope too: the scope lets the call's own reads be, not the
    /// capture thread's or the image worker's.
    #[test]
    #[cfg(debug_assertions)]
    fn a_worker_capture_on_the_loop_is_let_be_nowhere() {
        crate::quiet_expected_panics();
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            let fired = |what: &'static str| std::panic::catch_unwind(|| off_loop_screen_always(what)).is_err();
            let outside = fired("outside every call");
            let inside = {
                let _call = legacy(Kind::Screen);
                fired("inside a screen call")
            };
            [outside, inside]
        });
        assert_eq!(t.join().unwrap(), [true, true], "outside a call, inside a screen call");
    }

    /// A recognition on the loop panics in a debug build.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "[loop] text recognition on the event loop: a stray recognition — this is a bug")]
    fn a_recognition_on_the_loop_panics_in_a_debug_build() {
        mark_loop_thread();
        off_loop("a stray recognition");
    }

    /// A screen capture on the loop outside a call that could not wait panics in a debug build.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "[loop] screen capture on the event loop: a stray capture — this is a bug")]
    fn a_capture_on_the_loop_panics_in_a_debug_build() {
        mark_loop_thread();
        off_loop_screen("a stray capture");
    }

    /// In a release build it is counted and logged instead.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_recognition_on_the_loop_is_counted_in_a_release_build() {
        let before = violations(Kind::Text);
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            off_loop("a stray recognition");
        });
        t.join().unwrap();
        assert!(violations(Kind::Text) > before);
    }

    /// And a capture likewise, counted apart from the recognitions, and not inside a screen call.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_capture_on_the_loop_is_counted_in_a_release_build() {
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            let before = violations(Kind::Screen);
            {
                let _call = legacy(Kind::Screen);
                off_loop_screen("a capture of a call that could not wait");
            }
            let inside = violations(Kind::Screen) - before;
            off_loop_screen("a stray capture");
            (inside, violations(Kind::Screen) - before)
        });
        let (inside, outside) = t.join().unwrap();
        assert_eq!(inside, 0, "a capture inside a screen call's scope was counted");
        assert!(outside >= 1, "a stray capture was not counted");
    }

    /// And a worker's capture on the loop is counted inside a screen call's scope too.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_worker_capture_on_the_loop_is_counted_inside_a_screen_call_too() {
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            let before = violations(Kind::Screen);
            let _call = legacy(Kind::Screen);
            off_loop_screen_always("a worker capture inside a screen call");
            violations(Kind::Screen) - before
        });
        assert!(t.join().unwrap() >= 1, "a worker capture inside a screen call's scope was not counted");
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
        off_loop_screen("after the mark");
    }

    /// The first non-empty line of the body of `f`, the one item of `src` that starts with it.
    fn first_line<'a>(src: &'a str, file: &str, f: &str) -> &'a str {
        assert_eq!(src.matches(f).count(), 1, "{f}: not exactly one in {file}");
        let at = src.find(f).unwrap();
        let open = src[at..].find("{\n").expect("a body") + at + 2;
        src[open..].lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
    }

    /// The places a Mac photographs for a read or recognises begin with the text guard, and the
    /// places it captures for a `host.screen` call with a screen guard: the backend's four reads
    /// on the calling thread with the one a call that could not wait lets be, and a snapshot
    /// round's pictures, a handler's points and the image worker's captures with the one nothing
    /// lets be. macOS CI runs the tests of a release build only, where the guards count instead of
    /// panicking, and Windows never compiles these files — so this reads their source, on every
    /// platform.
    #[test]
    fn every_macos_recognition_and_capture_entry_begins_with_its_guard() {
        const OCR: &str = include_str!("backend/macos/ocr.rs");
        const CAPTURE: &str = include_str!("backend/macos/capture.rs");
        const BACKEND: &str = include_str!("backend/macos/mod.rs");
        let text = "crate::loop_guard::off_loop(";
        let screen = "crate::loop_guard::off_loop_screen(";
        let always = "crate::loop_guard::off_loop_screen_always(";
        for (file, src, f, guard) in [
            ("ocr.rs", OCR, "pub fn shot_of(", text),
            ("ocr.rs", OCR, "pub fn capture_for_read(", text),
            ("ocr.rs", OCR, "pub fn recognise_shot(", text),
            ("ocr.rs", OCR, "pub fn warm_up_recognise(", text),
            ("ocr.rs", OCR, "fn run_vision(", text),
            ("ocr.rs", OCR, "pub fn frames_for_round(", always),
            ("capture.rs", CAPTURE, "pub fn points(", always),
            ("mod.rs", BACKEND, "fn capture_many(", always),
            ("mod.rs", BACKEND, "fn pixel(&self, ", screen),
            ("mod.rs", BACKEND, "fn pixels(&self, ", screen),
            ("mod.rs", BACKEND, "fn capture(&self, ", screen),
            ("mod.rs", BACKEND, "fn frame(&self, ", screen),
        ] {
            let first = first_line(src, file, f);
            assert!(first.starts_with(guard), "{file}: {f} begins with {first:?}");
        }
        // The image worker's routines are those, and a Mac makes no first-read comparison.
        assert!(BACKEND.contains("    fn capture_fn(&self) -> CaptureFn {\n        capture_many\n    }"));
        assert!(BACKEND.contains("compare: super::no_comparison,") && !BACKEND.contains("fn compare_fn("));
    }

    /// Windows' four reads of the screen on the calling thread begin with the screen guard a call
    /// that could not wait lets be, and the captures only a thread of the host's makes — the
    /// capture thread's snapshot rounds and points, the image worker's captures and first-read
    /// comparisons — with the one nothing lets be. The guard's place, read from the source;
    /// backend/windows.rs holds that each of them panics there before it captures.
    #[test]
    fn every_windows_capture_entry_begins_with_its_screen_guard() {
        const WINDOWS: &str = include_str!("backend/windows.rs");
        let screen = "crate::loop_guard::off_loop_screen(";
        let always = "crate::loop_guard::off_loop_screen_always(";
        for (f, guard) in [
            ("fn pixel(&self, ", screen),
            ("fn pixels(&self, ", screen),
            ("fn capture(&self, ", screen),
            ("fn frame(&self, ", screen),
            ("fn frames_on_capture_thread(", always),
            ("fn points_on_capture_thread(", always),
            ("fn capture_on_worker(", always),
            ("fn compare_on_worker(", always),
        ] {
            let first = first_line(WINDOWS, "windows.rs", f);
            assert!(first.starts_with(guard), "windows.rs: {f} begins with {first:?}");
        }
        // The image worker's routines and the capture thread's comparison are those.
        assert!(WINDOWS.contains("    fn capture_fn(&self) -> CaptureFn {\n        capture_on_worker\n    }"));
        assert!(WINDOWS.contains("    fn compare_fn(&self) -> CompareFn {\n        compare_on_worker\n    }"));
        assert!(WINDOWS.contains("            compare: compare_on_worker,\n"));
    }

    #[test]
    fn the_lines_are_what_ci_looks_for() {
        let line = format!("[loop] {}", violation_line(Kind::Text, "Windows.Media.Ocr", 1));
        assert!(line.starts_with("[loop] text recognition on the event loop: Windows.Media.Ocr"), "{line}");
        let line = format!("[loop] {}", violation_line(Kind::Screen, "a region (Backend::capture)", 3));
        assert_eq!(
            line,
            "[loop] screen capture on the event loop: a region (Backend::capture) — this is a bug; please report it (3 so far this session)"
        );
    }
}
