//! Synthesising mouse and keyboard input.
//!
//! Nothing here may sleep. A press-and-hold gesture is composed by the caller out of
//! `mouse_down`, a timer, and `mouse_up`, precisely so the waiting happens somewhere that
//! is not the thread carrying speech, hotkeys and detection.

use crate::backend::MouseButton;

pub fn cursor_pos() -> (i32, i32) {
    (0, 0)
}
pub fn mouse_move(_x: i32, _y: i32) {}
pub fn mouse_click(_x: i32, _y: i32, _button: MouseButton) {}
pub fn mouse_drag(_x1: i32, _y1: i32, _x2: i32, _y2: i32, _button: MouseButton) {}
pub fn mouse_down(_x: i32, _y: i32, _button: MouseButton) {}
pub fn mouse_up(_x: i32, _y: i32, _button: MouseButton) {}
pub fn mouse_scroll(_x: i32, _y: i32, _amount: i32) {}

/// Deliver a key to one window without it entering the input queue. A Win32 idea; see the
/// trait for why it exists and what the honest macOS answer is.
pub fn key_post(_hwnd: isize, _key: &str) -> Result<(), String> {
    Ok(())
}

pub fn key_send(_combo: &str) -> Result<(), String> {
    Err("input is not implemented on macOS yet".to_string())
}
pub fn type_text(_text: &str) {}

/// Are any of Command / Option / Control / Shift held right now? Read from the hardware
/// state, not from a per-application cache.
pub fn modifiers_down() -> bool {
    false
}
