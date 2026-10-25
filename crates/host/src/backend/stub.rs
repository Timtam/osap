//! Fallback backend for platforms without a real implementation yet.
//! Window queries return empty; hotkey registration returns an error.
//!
//! Also the backend of the tests that run module VMs against the host's key bindings: it keeps
//! the captures and the owners it is handed, as a hook would, so a test can decide a key by them
//! with [`super::capture_decision`], and it answers `resolve_key_scope` with the window a test
//! put in front. A key passed on to the program in front (`pass_on_key`) is recorded, and sent
//! nowhere: refused, as `key_send` is, unless a test lets it through.

use std::cell::{Cell, RefCell};

use super::{
    Backend, CaptureFn, Captured, ControlInfo, ElementAsk, ElementOutcome,
    HostEvents, KeyRefusal, KeyboardNow, MouseButton, OwnerKeys, Stroke, WinInfo, capture_failed,
};
use crate::errors::{common, hotkey, HostError};

#[derive(Default)]
pub struct StubBackend {
    /// The window in front, as far as this backend knows: 0, unless a test put one there.
    front: Cell<isize>,
    /// The captures and the owners as the host last handed them over.
    keys: RefCell<(Vec<Captured>, Vec<OwnerKeys>)>,
    /// The keys a test holds down, and the modifier roles, for `keyboard_now`.
    held: RefCell<(Vec<u32>, u8)>,
    /// Whether a test holds a screen reader's key, for `keyboard_now`.
    reader: Cell<bool>,
    /// The count of key-downs let through to the program, as a test moves it on, for
    /// `keyboard_now`.
    let_through: Cell<u32>,
    /// How many of them end a count of quick repeats: all but the ones `host.input.send` sent.
    taps_through: Cell<u32>,
    /// Every key passed on, as `(vk, mask, strokes)`, in order.
    passed: RefCell<Vec<(u32, u8, Vec<Stroke>)>>,
    /// The physical key each of them was sent on, in the same order.
    passed_phys: RefCell<Vec<Option<u16>>>,
    /// Whether a key passed on, or sent with `host.input.send`, is taken: no by default, as there
    /// is no input to send it to.
    sends: Cell<bool>,
    /// What `watch_keys` answers when a test refuses the hook: none by default.
    hook_refused: RefCell<Option<HostError>>,
    /// The double-click time a test set, for `double_click_time`: the default's, 500 ms, until then.
    double_click: Cell<Option<std::time::Duration>>,
    /// When a test says the pump took the queued presses, for `input_taken_at`: never, until then.
    input_taken: Cell<Option<std::time::Instant>>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl StubBackend {
    /// Puts window `w` in front, for `resolve_key_scope`.
    pub(crate) fn set_front(&self, w: isize) {
        self.front.set(w);
    }

    /// The captures and the owners as the host last handed them over — what a hook would decide
    /// a key by.
    pub(crate) fn keys(&self) -> (Vec<Captured>, Vec<OwnerKeys>) {
        self.keys.borrow().clone()
    }

    /// Holds the keys `vks` and the modifier roles `mods` down, for `keyboard_now`.
    pub(crate) fn hold(&self, vks: &[u32], mods: u8) {
        *self.held.borrow_mut() = (vks.to_vec(), mods);
    }

    /// Holds a screen reader's key down (`true`), or lets it go, for `keyboard_now`.
    pub(crate) fn hold_reader(&self, held: bool) {
        self.reader.set(held);
    }

    /// The count of key-downs let through to the program now, for `keyboard_now`: what a hook
    /// would have counted by the time a key is taken (`Pressed::seq`), and as a key typed later
    /// moves it on.
    pub(crate) fn let_through(&self) -> u32 {
        self.let_through.get()
    }

    /// A key-down typed and let through to the program: both counts move on by one.
    pub(crate) fn type_through(&self) {
        self.let_through.set(self.let_through.get().wrapping_add(1));
        self.taps_through.set(self.taps_through.get().wrapping_add(1));
    }

    /// How many of the key-downs let through end a count of quick repeats (`Pressed::taps_seq`).
    pub(crate) fn taps_through(&self) -> u32 {
        self.taps_through.get()
    }

    /// A key-down `host.input.send` sent, let through to the program: the hook or the tap knows it
    /// by its mark, so it moves the count of keys let through on and not the one that ends a count
    /// of quick repeats.
    pub(crate) fn send_through(&self) {
        self.let_through.set(self.let_through.get().wrapping_add(1));
    }

    /// The physical keys the keys passed on since the last call were sent on, in order.
    pub(crate) fn take_passed_phys(&self) -> Vec<Option<u16>> {
        std::mem::take(&mut *self.passed_phys.borrow_mut())
    }

    /// Takes a key passed on or sent (`true`), or refuses it, as there is no input to send it to.
    pub(crate) fn set_sends(&self, sends: bool) {
        self.sends.set(sends);
    }

    /// The keys passed on since the last call, as `(vk, mask, strokes)`, in order.
    pub(crate) fn take_passed(&self) -> Vec<(u32, u8, Vec<Stroke>)> {
        std::mem::take(&mut *self.passed.borrow_mut())
    }

    /// Refuses the keyboard hook with `why` from now on, as a system that will not install it does,
    /// or installs it again (`None`).
    pub(crate) fn refuse_hook(&self, why: Option<HostError>) {
        *self.hook_refused.borrow_mut() = why;
    }

    /// Sets the user's double-click time, as the user changes it in the system's settings.
    pub(crate) fn set_double_click(&self, d: std::time::Duration) {
        self.double_click.set(Some(d));
    }

    /// The pump took the presses queued until `at`, as a backend's pump notes it.
    pub(crate) fn set_input_taken(&self, at: std::time::Instant) {
        self.input_taken.set(Some(at));
    }
}

impl Backend for StubBackend {
    fn environment(&self) -> Vec<(String, String)> {
        vec![(
            "backend".to_string(),
            format!("none — {} is not supported", std::env::consts::OS),
        )]
    }

    fn enumerate_windows(&self) -> Vec<WinInfo> {
        Vec::new()
    }

    fn active_window(&self) -> Option<WinInfo> {
        None
    }

    // The one platform with no windows has no foreground either.
    fn foreground_window(&self) -> Option<super::Foreground> {
        None
    }

    fn screen_size(&self) -> (i32, i32) {
        (0, 0)
    }

    // The source is accepted and ignored here, as on macOS: there is only one way of reading
    // nothing.
    fn capture_fn(&self) -> CaptureFn {
        |regions, _| regions.iter().map(|_| Err(capture_failed())).collect()
    }

    fn cursor_pos(&self) -> (i32, i32) {
        (0, 0)
    }
    // No input here: every pointer call answers that it was made, as `post` does, and makes nothing.
    fn mouse_move(&self, _x: i32, _y: i32) -> Result<(), HostError> {
        Ok(())
    }
    fn mouse_click(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), HostError> {
        Ok(())
    }
    // A double-click too: handed to the host's thread as on Windows and a Mac, and made there as
    // nothing.
    fn mouse_double_click(&self, _x: i32, _y: i32, _button: MouseButton, _interval: std::time::Duration) -> Result<super::ActJob, HostError> {
        Ok(Box::new(|| Ok(())))
    }
    fn mouse_drag(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _button: MouseButton) -> Result<(), HostError> {
        Ok(())
    }
    fn mouse_down(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), HostError> {
        Ok(())
    }

    fn mouse_up(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), HostError> {
        Ok(())
    }

    fn mouse_scroll(&self, _x: i32, _y: i32, _delta: i32) -> Result<(), HostError> {
        Ok(())
    }
    // No window to post to: answered as `key_send` is, as a way of acting this platform does not
    // have — a key name that is not one still raises, as everywhere.
    fn key_post(&self, _hwnd: isize, key: &str) -> Result<(), KeyRefusal> {
        super::key_to_vk(key).ok_or_else(|| format!("unknown key '{key}'"))?;
        Err(KeyRefusal::Answer(HostError::of(&common::NOT_YET)))
    }

    // No input to send it to: answered, as a way of acting this platform does not have — unless a
    // test takes it, and then it reaches the program as the hook lets a key `host.input.send` sent
    // go on (`send_through`).
    fn key_send(&self, combo: &str) -> super::KeySend {
        if !self.sends.get() {
            return super::KeySend::all_or_nothing(Err(KeyRefusal::Answer(HostError::of(&common::NOT_YET))));
        }
        if let Err(why) = super::parse_key_spec(combo) {
            return super::KeySend::all_or_nothing(Err(KeyRefusal::Mistake(why)));
        }
        self.send_through();
        super::KeySend::all_or_nothing(Ok(()))
    }
    fn type_text(&self, _text: &str) -> Result<(), HostError> {
        Ok(())
    }

    // No hotkey to hold: answered, as a way of being called this platform does not have.
    // A spec that does not parse raises, as on Windows and a Mac: a mistake in the call, whatever
    // the platform. Until 2026-10-24 it was answered as every other spec here.
    fn register_hotkey(&self, _id: i32, spec: &str) -> Result<(), KeyRefusal> {
        super::parse_key_spec(spec).map_err(|e| format!("could not parse the hotkey spec '{spec}': {e}"))?;
        Err(KeyRefusal::Answer(HostError::with(
            &hotkey::STUB,
            format!("hotkeys are not implemented on this platform yet (spec '{spec}')"),
        )))
    }

    fn unregister_hotkey(&self, _id: i32) {}

    fn watch_foreground(&self) -> Result<(), String> {
        Ok(())
    }

    fn window_controls(&self, _hwnd: isize) -> Vec<ControlInfo> {
        Vec::new()
    }
    fn window_focus_chain(&self) -> Vec<ControlInfo> {
        Vec::new()
    }
    // No tree to ask: every question finds nothing, and a press is not supported.
    fn element_ask(&self, ask: &ElementAsk) -> ElementOutcome {
        ask.nothing()
    }
    fn set_captured_keys(&self, keys: &[Captured]) {
        self.keys.borrow_mut().0 = keys.to_vec();
    }
    fn set_key_owners(&self, owners: &[OwnerKeys]) {
        self.keys.borrow_mut().1 = owners.to_vec();
    }
    fn resolve_key_scope(&self) -> isize {
        self.front.get()
    }
    fn key_front(&self) -> isize {
        self.front.get()
    }
    fn keyboard_now(&self, vk: u32, _mask: u8, _phys: Option<u16>) -> KeyboardNow {
        let held = self.held.borrow();
        KeyboardNow {
            front: Some(self.front.get()),
            key_held: held.0.contains(&vk),
            mods: held.1,
            reader_mod: self.reader.get(),
            let_through: self.let_through.get(),
        }
    }
    fn pass_on_key(&self, vk: u32, mask: u8, phys: Option<u16>, strokes: &[Stroke]) -> Result<(), String> {
        self.passed.borrow_mut().push((vk, mask, strokes.to_vec()));
        self.passed_phys.borrow_mut().push(phys);
        if self.sends.get() {
            Ok(())
        } else {
            Err("input is not implemented on this platform yet".to_string())
        }
    }
    fn modifiers_down(&self) -> bool {
        false
    }
    fn double_click_time(&self) -> std::time::Duration {
        self.double_click.get().unwrap_or(super::DEFAULT_DOUBLE_CLICK)
    }
    // No presses to queue: only what a test says.
    fn input_taken_at(&self) -> Option<std::time::Instant> {
        self.input_taken.get()
    }
    fn native_menu_open(&self) -> bool {
        false
    }
    fn watch_keys(&self) -> Result<(), HostError> {
        self.hook_refused.borrow().clone().map_or(Ok(()), Err)
    }

    fn run_event_loop(&self, _events: &mut dyn HostEvents) -> Result<(), String> {
        Ok(())
    }

    fn pump_pending(&self, events: &mut dyn HostEvents) {
        // Never anything to drain here — the stub has no pad source and `ensure_started`
        // refuses — but called anyway, so the pump has the same shape on every platform.
        super::gamepad::drain_into(events);
    }
}
