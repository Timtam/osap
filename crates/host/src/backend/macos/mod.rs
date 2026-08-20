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

use super::{
    Backend, CapturedImage, ControlInfo, DumpNode, HostEvents, MouseButton, OcrText, WinInfo,
};

pub(super) mod app;
mod ax;
mod capture;
mod ffi;
mod handles;
mod hotkey;
mod input;
mod keys;
mod ocr;
mod perm;
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

    fn window_controls(&self, hwnd: isize) -> Vec<ControlInfo> {
        ax::window_controls(hwnd)
    }

    fn window_focus_chain(&self) -> Vec<ControlInfo> {
        ax::window_focus_chain()
    }

    fn uia_find(&self, hwnd: isize, name: &str, control_type: i32) -> bool {
        ax::find(hwnd, name, control_type)
    }

    fn uia_find_any(&self, hwnd: isize, names: &[String], types: &[i32]) -> Option<usize> {
        ax::find_any(hwnd, names, types)
    }

    fn uia_locate(&self, hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
        ax::locate(hwnd, name, control_type)
    }

    fn uia_locate_via(
        &self,
        hwnd: isize,
        via_name: &str,
        via_type: i32,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        ax::locate_via(hwnd, via_name, via_type, name, control_type)
    }

    fn uia_plugin_locate(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        ax::plugin_locate(hwnd, container_name, name, control_type)
    }

    fn uia_dump(&self, hwnd: isize) -> Vec<DumpNode> {
        ax::dump(hwnd)
    }

    fn uia_raw_dump(&self, hwnd: isize) -> Vec<DumpNode> {
        // No two views of the tree on this platform: AX has one. Both dumps answer the same
        // question, and the calibrator calls this one.
        ax::dump(hwnd)
    }

    fn uia_state_probe(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        ax::state_probe(hwnd, container_name, name, control_type)
    }

    fn uia_class_nav_point(
        &self,
        hwnd: isize,
        class_substr: &str,
        ctype: i32,
        child: i32,
        sibling: i32,
    ) -> Option<(i32, i32)> {
        ax::class_nav_point(hwnd, class_substr, ctype, child, sibling)
    }

    fn uia_focus_step(&self, hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
        ax::focus_step(hwnd, direction)
    }

    fn screen_size(&self) -> (i32, i32) {
        capture::screen_size()
    }

    fn pixel(&self, x: i32, y: i32) -> (u8, u8, u8) {
        capture::pixel(x, y)
    }

    fn capture(&self, x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage> {
        capture::capture_region(x, y, w, h)
    }

    fn capture_fn(&self) -> fn(i32, i32, i32, i32) -> Option<CapturedImage> {
        capture::capture_region
    }

    fn ocr(&self, x: i32, y: i32, w: i32, h: i32, lang: Option<&str>) -> Result<OcrText, String> {
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

    fn mouse_scroll(&self, x: i32, y: i32, amount: i32) {
        input::mouse_scroll(x, y, amount);
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
        tap::set_key_scope(if to_foreground { ax::foreground_window_id() } else { 0 });
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
        queue::run_event_loop(events)
    }

    fn pump_pending(&self, events: &mut dyn HostEvents) {
        // Belt and braces. The tap re-enables itself from a run-loop observer, but the
        // moment it most needs to is the moment this thread was too busy to answer — so it
        // is also asked here, where being busy has just finished. Rate-limited inside.
        tap::health_check();
        queue::drain(events);
    }
}

/// The backend, as `platform()` hands it out.
pub fn new() -> Rc<dyn Backend> {
    Rc::new(MacBackend::new())
}
