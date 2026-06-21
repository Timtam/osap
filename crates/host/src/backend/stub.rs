//! Fallback backend for platforms without a real implementation yet.
//! Window queries return empty; hotkey registration returns an error.

use super::{Backend, CapturedImage, HostEvents, MouseButton, OcrText, WinInfo};

pub struct StubBackend;

impl Backend for StubBackend {
    fn enumerate_windows(&self) -> Vec<WinInfo> {
        Vec::new()
    }

    fn active_window(&self) -> Option<WinInfo> {
        None
    }

    fn screen_size(&self) -> (i32, i32) {
        (0, 0)
    }

    fn pixel(&self, _x: i32, _y: i32) -> (u8, u8, u8) {
        (0, 0, 0)
    }

    fn capture(&self, _x: i32, _y: i32, _w: i32, _h: i32) -> Option<CapturedImage> {
        None
    }

    fn ocr(
        &self,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _lang: Option<&str>,
    ) -> Result<OcrText, String> {
        Err("OCR is not implemented on this platform yet".to_string())
    }

    fn cursor_pos(&self) -> (i32, i32) {
        (0, 0)
    }
    fn mouse_move(&self, _x: i32, _y: i32) {}
    fn mouse_click(&self, _x: i32, _y: i32, _button: MouseButton) {}
    fn mouse_drag(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _button: MouseButton) {}
    fn mouse_scroll(&self, _x: i32, _y: i32, _amount: i32) {}
    fn key_send(&self, _combo: &str) -> Result<(), String> {
        Err("input is not implemented on this platform yet".to_string())
    }
    fn type_text(&self, _text: &str) {}

    fn register_hotkey(&self, _id: i32, spec: &str) -> Result<(), String> {
        Err(format!(
            "hotkeys are not implemented on this platform yet (spec '{spec}')"
        ))
    }

    fn unregister_hotkey(&self, _id: i32) {}

    fn watch_foreground(&self) -> Result<(), String> {
        Ok(())
    }

    fn set_captured_keys(&self, _vks: &[u32]) {}
    fn watch_keys(&self) -> Result<(), String> {
        Ok(())
    }

    fn run_event_loop(&self, _events: &mut dyn HostEvents) -> Result<(), String> {
        Ok(())
    }
}
