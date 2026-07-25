//! Fallback backend for platforms without a real implementation yet.
//! Window queries return empty; hotkey registration returns an error.

use super::{Backend, CapturedImage, ControlInfo, HostEvents, MouseButton, OcrText, WinInfo};

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

    fn capture_fn(&self) -> fn(i32, i32, i32, i32) -> Option<CapturedImage> {
        |_, _, _, _| None
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

    fn window_controls(&self, _hwnd: isize) -> Vec<ControlInfo> {
        Vec::new()
    }
    fn window_focus_chain(&self) -> Vec<ControlInfo> {
        Vec::new()
    }
    fn uia_find(&self, _hwnd: isize, _name: &str, _control_type: i32) -> bool {
        false
    }
    fn uia_locate(&self, _hwnd: isize, _name: &str, _control_type: i32) -> Option<(i32, i32)> {
        None
    }
    fn uia_locate_via(
        &self,
        _hwnd: isize,
        _via_name: &str,
        _via_type: i32,
        _name: &str,
        _control_type: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn uia_plugin_locate(
        &self,
        _hwnd: isize,
        _container_name: &str,
        _name: &str,
        _control_type: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn uia_dump(&self, _hwnd: isize) -> Vec<(i32, String, String, i32)> {
        Vec::new()
    }
    fn uia_raw_dump(&self, _hwnd: isize) -> Vec<(i32, String, String, i32)> {
        Vec::new()
    }
    fn uia_class_nav_point(
        &self,
        _hwnd: isize,
        _class_substr: &str,
        _ctype: i32,
        _child: i32,
        _sibling: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn uia_focus_step(&self, _hwnd: isize, _direction: i32) -> Option<(String, i32, i32, i32)> {
        None
    }
    fn set_captured_keys(&self, _keys: &[(u32, u8)]) {}
    fn set_key_scope(&self, _to_foreground: bool) {}
    fn set_menu_open(&self, _open: bool) {}
    fn watch_keys(&self) -> Result<(), String> {
        Ok(())
    }

    fn run_event_loop(&self, _events: &mut dyn HostEvents) -> Result<(), String> {
        Ok(())
    }

    fn pump_pending(&self, _events: &mut dyn HostEvents) {}
}
