//! The guard that keeps text recognition and screen captures off the event loop.
//!
//! The rule is the maintainer's: no text recognition may burden the event loop, which carries
//! every hotkey, every captured key, speech, every module callback and, on a Mac, the keyboard's
//! event tap — and after it no `host.screen` call either. `host.ocr.recognize` recognises on
//! threads of its own, with a callback or in a handler waiting for those threads (task.rs), and
//! the `host.screen` calls that read the screen wait in a handler while the `screen-capture` thread
//! or the image worker reads it; every one of them raises where it cannot wait. No way onto the
//! loop is left for either.
//!
//! So every function that photographs for a read or recognises asks, first thing, whether it
//! runs on the event loop (`off_loop`): the platform's `OcrWorker` functions that serve a read,
//! Windows' `recognize_image`, the neural recogniser's `ask`, and the place a Vision request is
//! performed on a Mac. And every function that captures the screen for a `host.screen` call asks
//! the same of the screen (`off_loop_screen`): the `OcrWorker` functions that take a snapshot
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

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

/// The work a guard keeps off the loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Text recognition, and the captures made for it (`off_loop`).
    Text,
    /// A screen capture for a `host.screen` call (`off_loop_screen`).
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
}

/// Violations of each kind on the loop, this session (release builds; a debug build panics at the
/// first).
static VIOLATIONS: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

/// Marks the calling thread as the event loop's, for the rest of its life.
pub fn mark_loop_thread() {
    LOOP.with(|l| l.set(true));
}

/// Says that `what` — a capture for a read, a recognition — is about to run, and that it must not
/// run on the event loop. Nothing happens anywhere else.
///
/// On the loop: a panic in a debug build, and in a release build a log line for the first
/// violation of the session and for every power of two after it, with the count.
pub fn off_loop(what: &str) {
    guard(Kind::Text, what);
}

/// Says that `what` — a capture of the screen for a `host.screen` call: a capture thread's snapshot
/// round or points, the image worker's capture or first-read comparison — is about to run, and that
/// it must not run on the event loop. Nothing happens anywhere else.
///
/// On the loop: a panic in a debug build, and in a release build a log line for the first violation
/// of the session and for every power of two after it, with the count — counted apart from the
/// recognitions'.
pub fn off_loop_screen(what: &str) {
    guard(Kind::Screen, what);
}

/// The guards: `what`, work of `kind`, about to run.
fn guard(kind: Kind, what: &str) {
    if !LOOP.with(Cell::get) {
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
fn violations(kind: Kind) -> u64 {
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
    }

    /// A recognition on the loop panics in a debug build.
    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "[loop] text recognition on the event loop: a stray recognition — this is a bug")]
    fn a_recognition_on_the_loop_panics_in_a_debug_build() {
        mark_loop_thread();
        off_loop("a stray recognition");
    }

    /// A screen capture on the loop panics in a debug build.
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

    /// And a capture likewise, counted apart from the recognitions.
    #[test]
    #[cfg(not(debug_assertions))]
    fn a_capture_on_the_loop_is_counted_in_a_release_build() {
        let t = std::thread::spawn(|| {
            mark_loop_thread();
            let before = violations(Kind::Screen);
            off_loop_screen("a stray capture");
            violations(Kind::Screen) - before
        });
        assert!(t.join().unwrap() >= 1, "a stray capture was not counted");
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
    /// places it captures for a `host.screen` call — a snapshot round's pictures, a handler's points
    /// and the image worker's captures — with the screen guard. macOS CI runs the tests of a release
    /// build only, where the guards count instead of panicking, and Windows never compiles these
    /// files — so this reads their source, on every platform.
    #[test]
    fn every_macos_recognition_and_capture_entry_begins_with_its_guard() {
        const OCR: &str = include_str!("backend/macos/ocr.rs");
        const CAPTURE: &str = include_str!("backend/macos/capture.rs");
        const BACKEND: &str = include_str!("backend/macos/mod.rs");
        let text = "crate::loop_guard::off_loop(";
        let screen = "crate::loop_guard::off_loop_screen(";
        for (file, src, f, guard) in [
            ("ocr.rs", OCR, "pub fn shot_of(", text),
            ("ocr.rs", OCR, "pub fn capture_for_read(", text),
            ("ocr.rs", OCR, "pub fn recognise_shot(", text),
            ("ocr.rs", OCR, "pub fn warm_up_recognise(", text),
            ("ocr.rs", OCR, "fn run_vision(", text),
            ("ocr.rs", OCR, "pub fn frames_for_round(", screen),
            ("capture.rs", CAPTURE, "pub fn points(", screen),
            ("mod.rs", BACKEND, "fn capture_many(", screen),
        ] {
            let first = first_line(src, file, f);
            assert!(first.starts_with(guard), "{file}: {f} begins with {first:?}");
        }
        // The image worker's routines are those, and a Mac makes no first-read comparison.
        assert!(BACKEND.contains("    fn capture_fn(&self) -> CaptureFn {\n        capture_many\n    }"));
        assert!(BACKEND.contains("compare: super::no_comparison,") && !BACKEND.contains("fn compare_fn("));
        // No read of the screen on the calling thread is left in the backend: a call that cannot
        // wait raises.
        for gone in ["fn pixel(&self, ", "fn pixels(&self, ", "fn capture(&self, ", "fn frame(&self, ", "pub fn pixel("] {
            assert!(!BACKEND.contains(gone) && !CAPTURE.contains(gone), "{gone}");
        }
    }

    /// Windows' captures for a `host.screen` call — the capture thread's snapshot rounds and points,
    /// the image worker's captures and first-read comparisons — begin with the screen guard, and no
    /// read of the screen on the calling thread is left in the backend. The guard's place, read
    /// from the source; backend/windows.rs holds that each of them panics there before it captures.
    #[test]
    fn every_windows_capture_entry_begins_with_its_screen_guard() {
        const WINDOWS: &str = include_str!("backend/windows.rs");
        let screen = "crate::loop_guard::off_loop_screen(";
        for f in ["fn frames_on_capture_thread(", "fn points_on_capture_thread(", "fn capture_on_worker(", "fn compare_on_worker("] {
            let first = first_line(WINDOWS, "windows.rs", f);
            assert!(first.starts_with(screen), "windows.rs: {f} begins with {first:?}");
        }
        for gone in ["fn pixel(&self, ", "fn pixels(&self, ", "fn capture(&self, ", "fn frame(&self, ", "Caller::Pump"] {
            assert!(!WINDOWS.contains(gone), "{gone}");
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
        let line = format!("[loop] {}", violation_line(Kind::Screen, "the image worker's capture", 3));
        assert_eq!(
            line,
            "[loop] screen capture on the event loop: the image worker's capture — this is a bug; please report it (3 so far this session)"
        );
    }
}
