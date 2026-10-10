//! Nothing of its own — it borrows two files out of `host` and asks the compiler whether
//! they are true for macOS. See this crate's Cargo.toml for why that cannot be done by
//! checking `host` itself.
//!
//! The borrowed modules are declared at the CRATE ROOT on purpose: the backend writes to
//! `crate::logging`, and that path has to resolve to the same thing here as it does in
//! `host`, or the code would compile in one crate and not the other.
#![cfg(target_os = "macos")]

#[path = "../../host/src/appcfg.rs"]
pub mod appcfg;

#[path = "../../host/src/portable.rs"]
pub mod portable;

#[path = "../../host/src/logging.rs"]
pub mod logging;

/// Borrowed because `logging` writes the build into its header.
#[path = "../../host/src/build_info.rs"]
pub mod build_info;

/// The guard that keeps text recognition off the event loop: the backend's capture and
/// recognition functions ask it first, so it has to exist at `crate::loop_guard` here as it does
/// in `host`.
#[path = "../../host/src/loop_guard.rs"]
pub mod loop_guard;

/// The event loop's processor time, read from the guard's watchdog thread: its macOS half,
/// `thread_info(THREAD_BASIC_INFO)` on the port of `pthread_mach_thread_np`, has no other way to
/// meet a compiler before it meets a Mac.
#[path = "../../host/src/thread_cpu.rs"]
pub mod thread_cpu;

/// Names what the guard calls in `thread_cpu.rs`, so a signature that changes fails here and not
/// only on the Mac: an unreferenced item only warns.
pub fn thread_cpu_calls() {
    let _: fn() -> Option<thread_cpu::LoopThread> = thread_cpu::LoopThread::current;
    let _: fn(&thread_cpu::LoopThread) -> Option<std::time::Duration> = thread_cpu::LoopThread::cpu;
}

/// The single-instance guard's names, socket and decision. Kept free of wxdragon for this:
/// the lock itself is created in `gui.rs`, and reaches this file only through its `Lock`
/// trait. The macOS half — the uid, the per-user temporary folder, the Unix-domain socket and
/// its peer check — has no other way to meet a compiler before it meets a Mac.
#[path = "../../host/src/instance.rs"]
pub mod instance;

#[path = "../../host/src/backend/mod.rs"]
pub mod backend;

/// What the operating system does to the whole session — sleep, lock, display changes — the
/// `[system]` line for each and the host's plan: the event type and the queue the macOS
/// backend's `system.rs` pushes to and delivers from, and named by `HostEvents::on_system`, so
/// it has to exist at `crate::system_events` here as it does in `host`. Pure apart from the
/// queue; `--tests` builds its tests too.
#[path = "../../host/src/system_events.rs"]
pub mod system_events;

/// `host.screen.cells` and the window-relative Region form: pure code, with no macOS half of
/// its own, borrowed so that nothing in them can compile on Windows alone — the winnow parser,
/// the hex exchange and the capture type they read are checked for this target too, and
/// `--tests` builds their unit tests here as well.
#[path = "../../host/src/region.rs"]
pub mod region;
#[path = "../../host/src/cells.rs"]
pub mod cells;

/// `host.screen.profile`'s reduction, pure, borrowed for the reason `cells.rs` is: it reads the
/// capture type and a snapshot's `Area`, both of which the macOS backend defines the other half of.
#[path = "../../host/src/profile.rs"]
pub mod profile;

/// Names what `lib.rs` calls in `system_events.rs`, and the speech path's own hook, for the
/// reason `cells_calls` exists. (The macOS backend's `system.rs` calls `push` and `take`, and
/// is compiled here with the backend.)
#[allow(clippy::type_complexity)]
pub fn system_calls() {
    let _: fn(
        &mut system_events::Since,
        &[system_events::Stamped],
        std::time::Instant,
        std::time::SystemTime,
    ) -> system_events::Plan = system_events::Since::plan;
    let _: fn(&mut system_events::Since, std::time::Instant) -> Vec<String> = system_events::Since::due;
    let _: fn(&speech::Speech, system_events::SystemEvent) = speech::Speech::on_system;
}

/// Names what `lib.rs`, `region_lua.rs` and `ocr/lua.rs` call in the cells and region files, for
/// the reason `checked` names the backend: an unreferenced item only warns, and a signature the
/// host no longer matches would otherwise reach the macOS CI job first. Written as typed bindings
/// in a body, because these items are crate-private and a public signature may not show them.
/// (`region_lua.rs` itself reads Luau values and is not borrowed: macos-check has no Luau.)
#[allow(clippy::type_complexity)]
pub fn cells_calls() {
    let _: fn(&str) -> Result<cells::Predicate, cells::PredError> = cells::Predicate::parse;
    let _: fn(u32, u32, cells::Predicate) -> Result<cells::CellSpec, String> = cells::CellSpec::new;
    let _: fn(&cells::CellSpec) -> usize = cells::CellSpec::cells;
    let _: fn(&backend::CapturedImage, i32, i32, &cells::CellSpec) -> Result<Vec<u8>, String> = cells::of_capture;
    let _: fn(&cells::Matcher, &backend::CapturedImage, i32, i32) -> Result<(Vec<u8>, cells::Ranked), String> =
        cells::Matcher::answer;
    let _: fn(&backend::CapturedImage, cells::Sub, &cells::CellSpec) -> Result<Vec<u8>, String> = cells::of_sub;
    let _: fn(&cells::Matcher, &backend::CapturedImage, cells::Sub) -> Result<(Vec<u8>, cells::Ranked), String> =
        cells::Matcher::answer_sub;
    let _ = |spec: cells::CellSpec| cells::Matcher { spec, states: Vec::new(), item_of: Vec::new() };
    let _: fn(&[u8], usize) -> Result<Vec<u8>, cells::HexError> = cells::from_hex;
    let _: fn(&[u8]) -> String = cells::to_hex;
    let _: fn(&[Option<String>]) -> Vec<usize> = cells::items_of;
    let _: fn(f64, f64, f64, f64) -> Result<region::Fraction, String> = region::Fraction::new;
    let _: fn(region::Client, &region::Fraction) -> Result<region::ScreenRect, region::Unresolved> = region::resolve;
    let _: fn(&str, i64, i64, i64, i64) -> Result<region::ScreenRect, String> = region::corners;
    let _: fn(&region::Region) -> Result<region::ScreenRect, region::Unresolved> = region::Region::resolve;
    let _: fn(region::Client, f64, f64) -> Result<(i32, i32), region::Unresolved> = region::resolve_point;
    let _: fn(i32, i32, i32, i32) -> region::ScreenRect = region::loose_corners;
    let _ = region::Region::Rect(region::ScreenRect { x: 0, y: 0, w: 1, h: 1 });
    let _ = |c: region::Client, f: region::Fraction| region::Region::Window(c, f);
    let _ = region::Client { x: 0, y: 0, w: 0, h: 0 };
    let _ = (cells::MAX_SIDE, cells::MAX_STATES, cells::MAX_REGION_PIXELS);
}

/// Names what `snapshot.rs`, `image_search.rs` and `lib.rs` call in a snapshot's frame and the
/// profile reduction, for the reason `cells_calls` exists: the frame is where the macOS backend's
/// `NativeImage` meets the host's pure geometry, and a signature the host no longer matches
/// would otherwise reach the macOS CI job first.
#[allow(clippy::type_complexity)]
pub fn frame_calls() {
    use backend::frame::{self, Area, Frame, FrameVia, NativeImage};
    use ocr::types::Rect;
    let _: fn(Rect, backend::CapturedImage, std::time::Instant, FrameVia, Option<NativeImage>) -> Option<Frame> =
        Frame::from_image;
    let _: fn(&Frame, Rect) -> bool = Frame::contains;
    let _: fn(&Frame, i32, i32) -> bool = Frame::contains_point;
    let _: fn(&Frame, Rect) -> Option<Rect> = Frame::clip;
    let _: fn(&Frame, Rect) -> Option<Area> = Frame::area;
    let _: fn(&Frame, i32, i32) -> Option<(u8, u8, u8)> = Frame::sample;
    let _: fn(&Frame, Rect) -> Option<backend::CapturedImage> = Frame::crop_image;
    let _: fn(&Frame, Rect) -> Option<Frame> = Frame::crop;
    let _: fn(&Frame, Rect) -> Option<Frame> = Frame::pixels_only;
    let _: fn(&Frame) -> usize = Frame::bytes;
    let _: fn(&Frame) -> Rect = Frame::ocr_rect;
    let _: fn(&NativeImage) -> usize = NativeImage::bytes;
    let _: fn(&NativeImage, Rect) -> Option<NativeImage> = NativeImage::crop;
    let _: fn(&NativeImage) -> Rect = NativeImage::covers;
    let _: fn(FrameVia) -> &'static str = FrameVia::word;
    let _: fn(&[(i32, i32)]) -> Option<Rect> = frame::bbox;
    let _: fn(&[(i32, i32)]) -> Vec<Rect> = frame::plan_points;
    let _: fn(&Rect, &Rect) -> Option<Rect> = Rect::intersect;
    let _: fn(&Rect, &Rect) -> bool = Rect::contains;
    let _: fn(&backend::CapturedImage, Area, bool, bool, u8) -> Option<(Option<profile::Axis>, Option<profile::Axis>)> =
        profile::profile;
}

/// Names what `ocr/service.rs` and `snapshot.rs` call in the change wait and the snapshot lane,
/// for the reason `frame_calls` exists: those two files are not borrowed, and a signature they no
/// longer match would otherwise reach the macOS CI job first.
#[allow(clippy::type_complexity)]
pub fn snapshot_lane_calls() {
    use backend::frame::Frame;
    use ocr::change::{ChangeSpec, Wait};
    use ocr::sched::{Cand, Scheduler};
    use ocr::snap_queue::{self, Pick, Round, RoundDone, SnapDone, SnapLane, SnapReq};
    use ocr::types::Rect;
    use std::sync::Arc;
    use std::time::Instant;
    let _: fn(Rect, &[Rect], ChangeSpec, Option<Arc<Frame>>, Instant) -> Wait = Wait::new;
    let _: fn(fn(i32, i32) -> u32) -> SnapLane = SnapLane::new;
    let _: fn(&mut SnapLane, SnapReq) = SnapLane::submit;
    let _: fn(&SnapLane, Instant) -> Option<Cand> = SnapLane::candidate;
    let _: fn(&mut SnapLane, Instant) -> Option<Round> = SnapLane::take;
    let _: fn(&SnapLane) -> Option<Instant> = SnapLane::next_wake;
    let _: fn(&mut SnapLane, Instant) -> Vec<SnapDone> = SnapLane::sweep_cancelled;
    let _: fn(&SnapLane, usize) -> bool = SnapLane::picture_due;
    let _: fn(&mut SnapLane, RoundDone, Instant) -> Vec<SnapDone> = SnapLane::finish;
    let _: fn(Round, Vec<Result<Frame, String>>, Instant, u64) -> RoundDone = Round::run;
    let _: fn(&[Rect], i64) -> (Vec<Rect>, Vec<usize>) = snap_queue::group;
    let _: fn(Round, &str, Instant) -> RoundDone = Round::fail;
    let _: fn(&Round) -> Vec<(i32, i32, i32, i32)> = Round::tuples;
    let _: fn(&mut Round) -> Vec<backend::CompareReq> = Round::take_compares;
    // The first-read comparison a request carries, which `capture_source.rs` hands out and the
    // capture thread and the image worker make.
    let _: fn(bool) -> backend::CompareSlot = backend::CompareSlot::new;
    let _: fn(&Arc<backend::CompareSlot>, &str, (i32, i32, i32, i32)) -> Option<backend::CompareReq> =
        backend::CompareSlot::take;
    let _: fn(&backend::CompareReq, backend::CompareFn) = backend::CompareReq::make;
    let _: fn(&backend::CompareReq, bool) = backend::CompareReq::settle;
    let _: fn(Option<Cand>, Option<Cand>) -> Option<Pick> = snap_queue::choose;
    let _: fn(&Scheduler<u8, u8>, bool, Instant) -> Option<Cand> = Scheduler::peek_capture;
    let _: fn(&Scheduler<u8, u8>, usize) -> bool = Scheduler::picture_due;
    let _: fn(&mut Scheduler<u8, u8>, u8, ocr::sched::Ticket, u8, usize, Instant) -> ocr::sched::Submitted =
        Scheduler::submit_captured;
}

/// The pure OCR files — types, languages, the capture plan, the queue and the shape a module is
/// handed — which the macOS backend names. See the file.
pub mod ocr;

/// The two helpers `host`'s lib.rs gives its tests for a panic raised on purpose, which the
/// borrowed `ocr/snap_queue.rs` tests use to show a round that panics is still answered. Here
/// the panic is simply printed: `--tests` only has to build them.
#[cfg(test)]
pub(crate) const EXPECTED_PANIC: &str = "expected by a test: ";
#[cfg(test)]
pub(crate) fn quiet_expected_panics() {}

/// The VoiceOver speech path, borrowed for the same reason — it talks to an application
/// that does not exist on this machine, through a binary that does not either.
/// The WHOLE speech module now, not just the two macOS files inside it.
///
/// It could not be borrowed before: it pulled in `tts`, which does not build for this
/// target from here. Removing `tts` in favour of `avspeech.rs` took that away — so the part
/// that was never checked at home, the WIRING (which transport a line goes to, what
/// `engines()` answers, what `use` accepts), is checked now. That wiring is where the
/// mistakes are; the two leaf files were only ever the easy half.
#[path = "../../host/src/speech/mod.rs"]
pub mod speech;

/// Names the backend so the check cannot pass by leaving it out of the build: without a
/// use, an unreferenced `mod` still compiles, but a typo'd `platform()` would not be caught.
pub fn checked() -> std::rc::Rc<dyn backend::Backend> {
    backend::platform()
}

/// Names the two application-level calls for the same reason `checked` names the backend.
///
/// They are called from `gui.rs`, which this crate cannot borrow (wxdragon), so without
/// this the re-export is unreferenced here — and an unreferenced re-export warns instead of
/// proving that the path resolves and the signatures are what the caller expects.
/// The permissions page's three calls, named for the same reason as `app_calls` below.
///
/// They live in `gui.rs`, which this crate cannot borrow (wxdragon), so without naming them
/// here the compiler would only tell us they are unused — and a signature that no longer
/// matches its caller would reach the macOS CI job as a link error, or a tester as a page
/// that does not build.
pub fn permission_calls() -> (
    fn() -> Vec<backend::Permission>,
    fn(&str) -> bool,
    fn(&str) -> backend::Asked,
) {
    (backend::permissions, backend::open_pane, backend::ask_for)
}

/// The Permissions page's line about VoiceOver's AppleScript box, named for the same reason: it
/// is read by `gui.rs`. (The `[env]` line's half is called by the borrowed `perm.rs`, and the
/// notice of VoiceOver Utility quitting by the borrowed `system.rs`.)
pub fn applescript_calls() {
    let _: fn() -> Option<String> = speech::voiceover_applescript_page;
    let _: fn() -> String = speech::voiceover_applescript_env;
    let _: fn(Option<&str>, Option<&str>) = speech::application_quit;
}

/// `automation-platform ocr-bench`, named for the reason `app_calls` names its calls: `lib.rs`
/// calls it, and this crate cannot borrow `lib.rs`. Without a use the re-export only warns.
pub fn ocr_bench_calls() -> fn(&[String]) -> i32 {
    backend::ocr_bench
}

/// The neural text recogniser's start and end, which `lib.rs` calls around a session: its warm-up
/// (on a Mac, after Vision's), the exit's call that stops that warm-up waiting, the shadow's
/// counts at exit, the exit's wait for recognitions still running, ONNX Runtime's release before
/// `exit()`, and the end without `exit()`'s teardown when it could not be released. Named for the
/// reason `ocr_bench_calls` is.
#[allow(clippy::type_complexity)]
pub fn ocr_lifecycle_calls() -> (fn() -> Option<std::thread::JoinHandle<()>>, fn(), fn(), fn(), fn() -> bool, fn(i32) -> !) {
    (
        backend::warmup_ocr,
        backend::stop_ocr_warmup,
        backend::ocr_exit_report,
        backend::settle_ocr,
        backend::release_ocr,
        backend::end_now,
    )
}

pub fn app_calls() -> (fn(), fn(bool, &str) -> bool, fn(), fn(), fn()) {
    (
        backend::activate_self,
        backend::set_regular,
        backend::note_frontmost_before_gui,
        backend::restore_frontmost_after_gui_start,
        // Called from the settings switch in `gui.rs`, which this crate cannot borrow, so
        // naming it here is the only thing that proves the path and the signature.
        backend::request_voiceover_automation,
    )
}
