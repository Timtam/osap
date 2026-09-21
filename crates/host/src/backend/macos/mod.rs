//! The macOS backend.
//!
//! Written without a Mac to run it on — see `docs/macos-port.md` for what that means and
//! what has been done about it. Two rules hold everywhere in this module and are worth
//! reading before anything else:
//!
//! **Points, never pixels.** Every coordinate that crosses the [`Backend`] boundary is in
//! points, origin top-left of the primary display. That is the space `AXPosition` reports
//! and the space `CGEvent` clicks land in. Capture is the one place they differ — a Retina
//! display hands back twice the pixels — and [`capture`] downsamples so that one pixel of a
//! `CapturedImage` is one point. Getting this wrong does not crash: it clicks half a screen
//! away, on Retina machines only, and the user gets no signal at all.
//!
//! **Cocoa's other origin never escapes.** `NSScreen`/`NSWindow` frames and Vision's
//! normalised boxes are bottom-left. They are flipped where they are read, never later.
//!
//! Everything here runs on the main thread with exactly one exception, the capture function
//! handed to the image worker ([`Backend::capture_fn`]), which is a bare `fn` for that
//! reason. OS callbacks — the event tap, the Carbon hotkey handler, the accessibility
//! observers — arrive on the main run loop, which wxWidgets already runs, and they only
//! ever push onto a queue and return. Dispatching from inside them is what gets an event
//! tap disabled for being slow.

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{
    Backend, CaptureFn, CaptureSource, CapturedImage, ControlInfo, DumpNode, HostEvents,
    MouseButton, OcrText, WinInfo,
};

/// Whether a scroll has been sent yet in this run — so the first one is always recorded and
/// the rest are not. A Mac has never sent one, so the first is worth a line on its own.
static SCROLL_SEEN: AtomicBool = AtomicBool::new(false);

pub(super) mod app;
mod ax;
mod budget;
mod capture;
mod ffi;
mod front_memory;
mod handles;
mod hotkey;
mod input;
mod key_age;
mod keys;
mod ocr;
pub(crate) mod perm;
mod queue;
mod tap;
mod watch;

pub struct MacBackend;

impl MacBackend {
    pub fn new() -> Self {
        // Asking for trust here, once, is the whole permission onboarding: the prompting
        // form of the check is the only way an application can raise the Accessibility
        // dialog at all, and everything this platform does needs it.
        perm::request_accessibility_once();
        // And screen recording, which has to be ASKED for rather than checked: an
        // application that never asks is never listed in that settings pane, so the user
        // cannot grant it even when they want to.
        perm::request_screen_recording_once();
        // Vision loads its model on the first request — half a second to two seconds — and
        // `ocr` runs on the thread that carries the keyboard. Warming it on a background
        // thread now is the difference between a first read-out that is merely slow and one
        // that stalls the pump long enough for the system to switch off the event tap.
        ocr::warm_up();
        MacBackend
    }
}

impl Backend for MacBackend {
    fn environment(&self) -> Vec<(String, String)> {
        perm::environment_report()
    }

    fn enumerate_windows(&self) -> Vec<WinInfo> {
        ax::enumerate_windows()
    }

    fn active_window(&self) -> Option<WinInfo> {
        ax::active_window()
    }

    fn focus_window(&self, id: isize) -> bool {
        ax::focus_window(id)
    }

    fn window_controls(&self, hwnd: isize) -> Vec<ControlInfo> {
        ax::window_controls(hwnd)
    }

    fn window_focus_chain(&self) -> Vec<ControlInfo> {
        ax::window_focus_chain()
    }

    fn element_find(&self, hwnd: isize, name: &str, control_type: i32) -> bool {
        ax::find(hwnd, name, control_type)
    }

    fn element_find_any(&self, hwnd: isize, names: &[String], types: &[i32]) -> Option<usize> {
        ax::find_any(hwnd, names, types)
    }

    fn element_locate(&self, hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
        ax::locate(hwnd, name, control_type)
    }

    fn element_locate_via(
        &self,
        hwnd: isize,
        via_name: &str,
        via_type: i32,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        ax::locate_via(hwnd, via_name, via_type, name, control_type)
    }

    fn element_plugin_locate(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        ax::plugin_locate(hwnd, container_name, name, control_type)
    }

    fn element_dump(&self, hwnd: isize) -> Vec<DumpNode> {
        ax::dump(hwnd)
    }

    fn element_raw_dump(&self, hwnd: isize) -> Vec<DumpNode> {
        // No two views of the tree on this platform: AX has one. Both dumps answer the same
        // question, and the calibrator calls this one.
        ax::dump(hwnd)
    }

    fn element_state_probe(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        ax::state_probe(hwnd, container_name, name, control_type)
    }

    fn element_class_nav_point(
        &self,
        hwnd: isize,
        class_substr: &str,
        ctype: i32,
        child: i32,
        sibling: i32,
    ) -> Option<(i32, i32)> {
        ax::class_nav_point(hwnd, class_substr, ctype, child, sibling)
    }

    fn element_focus_step(&self, hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
        ax::focus_step(hwnd, direction)
    }

    fn element_focus_within(&self, hwnd: isize, x: i32, y: i32, w: i32, h: i32) -> Option<(String, i32)> {
        ax::focus_within(hwnd, x, y, w, h)
    }

    fn screen_size(&self) -> (i32, i32) {
        capture::screen_size()
    }

    // The capture source is accepted and ignored on this platform. `[screen] capture =
    // "duplication"` names a Windows mechanism; here every read keeps going through
    // ScreenCaptureKit or CoreGraphics exactly as it did before the key existed, and the host
    // resolves such a module to the standard source before it ever gets this far.
    fn pixel(&self, x: i32, y: i32, _src: CaptureSource) -> Option<(u8, u8, u8)> {
        Some(capture::pixel(x, y))
    }

    fn capture(&self, x: i32, y: i32, w: i32, h: i32, _src: CaptureSource) -> Option<CapturedImage> {
        capture::capture_region(x, y, w, h)
    }

    fn capture_fn(&self) -> CaptureFn {
        capture_many
    }

    fn ocr(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        lang: Option<&str>,
        _src: CaptureSource,
    ) -> Result<OcrText, String> {
        ocr::recognize(x, y, w, h, lang)
    }

    fn cursor_pos(&self) -> (i32, i32) {
        input::cursor_pos()
    }

    fn mouse_move(&self, x: i32, y: i32) {
        input::mouse_move(x, y);
    }

    fn mouse_click(&self, x: i32, y: i32, button: MouseButton) {
        input::mouse_click(x, y, button);
    }

    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton) {
        input::mouse_drag(x1, y1, x2, y2, button);
    }

    fn mouse_down(&self, x: i32, y: i32, button: MouseButton) {
        input::mouse_down(x, y, button);
    }

    fn mouse_up(&self, x: i32, y: i32, button: MouseButton) {
        input::mouse_up(x, y, button);
    }

    fn mouse_scroll(&self, x: i32, y: i32, delta: i32) {
        // The trait counts what Windows counts — 120 units to a notch — and Quartz counts
        // lines, so this is where the two meet. Kept on the Windows unit because that is the
        // finer of the two: a fraction of a notch is expressible there and not here, and a
        // caller asking for less than a line gets the smallest whole line rather than nothing
        // at all. That is a real difference between the platforms and not a rounding detail —
        // a control that needs a half-step will not get one on a Mac until this asks Quartz in
        // pixel units instead, which is a change nobody can test from here.
        let lines = (delta as f64 / 120.0).round() as i32;
        let lines = if lines != 0 {
            lines
        } else if delta > 0 {
            1
        } else if delta < 0 {
            -1
        } else {
            0
        };

        // Say what was ASKED FOR and what was SENT, on the two occasions where the difference
        // between them is the whole question.
        //
        // The first scroll of a run is logged whatever it is, because until a Mac has run one
        // nobody knows this path is reached at all. After that only requests that are not a
        // whole notch are — those are rare, deliberate, and exactly the case nothing here can
        // predict: a control finer than a line either follows a rounded-up whole line or
        // ignores it and reports the same value back, and only a real one can say which.
        // Whole-notch scrolling is left silent, because a list being scrolled writes a line
        // per keypress and a flooded log is worse than none.
        let remainder = delta % 120;
        if remainder != 0 || !SCROLL_SEEN.swap(true, Ordering::Relaxed) {
            crate::logging::line(
                "macos",
                &format!(
                    "scroll at {x},{y}: asked {} notch(es) ({delta} units), sent {lines} Quartz line(s){}",
                    delta as f64 / 120.0,
                    if remainder != 0 {
                        " — a fraction of a notch cannot be expressed in lines, so this was rounded"
                    } else {
                        ""
                    }
                ),
            );
        }

        input::mouse_scroll(x, y, lines);
    }

    fn ocr_regions(
        &self,
        regions: &[(i32, i32, i32, i32)],
        lang: Option<&str>,
        _src: CaptureSource,
    ) -> Vec<Result<OcrText, String>> {
        ocr::recognize_regions(regions, lang)
    }

    fn key_post(&self, hwnd: isize, key: &str) -> Result<(), String> {
        input::key_post(hwnd, key)
    }

    fn key_send(&self, combo: &str) -> Result<(), String> {
        input::key_send(combo)
    }

    fn type_text(&self, text: &str) {
        input::type_text(text);
    }

    fn register_hotkey(&self, id: i32, spec: &str) -> Result<(), String> {
        hotkey::register(id, spec)
    }

    fn unregister_hotkey(&self, id: i32) {
        hotkey::unregister(id);
    }

    fn watch_foreground(&self) -> Result<(), String> {
        watch::start()
    }

    fn set_captured_keys(&self, keys: &[(u32, u8)]) {
        tap::set_captured_keys(keys);
    }

    fn set_key_scope(&self, to_foreground: bool) {
        // Snapshot semantics, exactly as on Windows: `true` freezes the window that is
        // foreground AT THIS MOMENT. Resolving it lazily at press time would always match
        // and a menu opened by a control could never be navigated.
        // A window that could not be resolved pins nothing, which is what 0 already meant
        // here and is the permissive answer: scope 0 is global, so the overlay's keys are
        // claimed everywhere rather than nowhere. Said out loud, because it is a scope the
        // caller did not ask for.
        let window = if to_foreground {
            match ax::foreground_window_id() {
                Some(w) => w,
                None => {
                    crate::logging::line(
                        "macos",
                        "key scope: the frontmost application did not say which window is in \
                         front, so the scope stays global for now",
                    );
                    0
                }
            }
        } else {
            0
        };
        tap::set_key_scope(window);
    }

    fn set_menu_open(&self, open: bool) {
        tap::set_menu_open(open);
    }

    fn modifiers_down(&self) -> bool {
        input::modifiers_down()
    }

    fn native_menu_open(&self) -> bool {
        watch::native_menu_open()
    }

    fn watch_keys(&self) -> Result<(), String> {
        tap::install()
    }

    fn run_event_loop(&self, events: &mut dyn HostEvents) -> Result<(), String> {
        // The same cadence the GUI tick would drain at, through the same pump, so headless
        // stays a fair test of the rest — see `queue::run_once`.
        crate::logging::line("macos", "headless: running the CoreFoundation run loop");
        loop {
            queue::run_once();
            self.pump_pending(events);
            events.on_tick();
        }
    }

    fn enumerate_windows_of(&self, pids: &[u32]) -> Vec<WinInfo> {
        ax::enumerate_windows_of(pids)
    }

    fn running_apps(&self) -> Vec<crate::backend::AppInfo> {
        ax::running_apps()
    }

    fn windows_of(&self, pid: u32) -> Vec<crate::backend::WindowSpot> {
        ax::windows_of(pid)
    }

    fn window_owns_point(&self, hwnd: isize, x: i32, y: i32) -> Option<bool> {
        ax::window_owns_point(hwnd, x, y)
    }

    fn take_menu_pass_through(&self) -> Vec<(u32, u8)> {
        tap::take_menu_pass_through()
    }

    fn pump_pending(&self, events: &mut dyn HostEvents) {
        // Belt and braces. The tap re-enables itself from a run-loop observer, but the
        // moment it most needs to is the moment this thread was too busy to answer — so it
        // is also asked here, where being busy has just finished. Rate-limited inside.
        tap::health_check();
        // A busy application owed another subscription attempt gets it here, on the clock,
        // rather than only when the user next switches applications — see watch.rs.
        watch::retry_refused();
        queue::drain(events);
    }
}

/// The image worker's capture routine: each region captured on its own, in order, exactly as
/// the worker did one at a time before it was handed several. The source is ignored — see
/// `pixel` above.
fn capture_many(regions: &[(i32, i32, i32, i32)], _src: CaptureSource) -> Vec<Option<CapturedImage>> {
    regions.iter().map(|&(x, y, w, h)| capture::capture_region(x, y, w, h)).collect()
}

/// The backend, as `platform()` hands it out.
pub fn new() -> Rc<dyn Backend> {
    Rc::new(MacBackend::new())
}
