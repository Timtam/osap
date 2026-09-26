//! Fallback backend for platforms without a real implementation yet.
//! Window queries return empty; hotkey registration returns an error.

use super::{
    Backend, CaptureFn, CaptureSource, CapturedImage, ControlInfo, DumpNode, HostEvents,
    MouseButton, OcrText, WinInfo, CAPTURE_FAILED,
};

pub struct StubBackend;

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

    fn screen_size(&self) -> (i32, i32) {
        (0, 0)
    }

    // The source is accepted and ignored here, as on macOS: there is only one way of reading
    // nothing.
    fn pixel(&self, _x: i32, _y: i32, _src: CaptureSource) -> Result<(u8, u8, u8), String> {
        Ok((0, 0, 0))
    }

    fn capture(&self, _x: i32, _y: i32, _w: i32, _h: i32, _src: CaptureSource) -> Result<CapturedImage, String> {
        Err(CAPTURE_FAILED.to_string())
    }

    fn capture_fn(&self) -> CaptureFn {
        |regions, _| regions.iter().map(|_| Err(CAPTURE_FAILED.to_string())).collect()
    }

    // Nothing to keep. `pixels` is the trait's, which fails through `capture` above.
    fn frame(&self, _r: crate::ocr::types::Rect, _src: CaptureSource) -> Result<super::frame::Frame, String> {
        Err(CAPTURE_FAILED.to_string())
    }

    fn ocr(
        &self,
        _x: i32,
        _y: i32,
        _w: i32,
        _h: i32,
        _lang: Option<&str>,
        _src: CaptureSource,
    ) -> Result<OcrText, String> {
        Err("OCR is not implemented on this platform yet".to_string())
    }

    fn cursor_pos(&self) -> (i32, i32) {
        (0, 0)
    }
    fn mouse_move(&self, _x: i32, _y: i32) {}
    fn mouse_click(&self, _x: i32, _y: i32, _button: MouseButton) {}
    fn mouse_drag(&self, _x1: i32, _y1: i32, _x2: i32, _y2: i32, _button: MouseButton) {}
    fn mouse_down(&self, _x: i32, _y: i32, _button: MouseButton) {}

    fn mouse_up(&self, _x: i32, _y: i32, _button: MouseButton) {}

    fn mouse_scroll(&self, _x: i32, _y: i32, _delta: i32) {}
    fn key_post(&self, _hwnd: isize, _key: &str) -> Result<(), String> {
        Ok(())
    }

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
    fn element_find(&self, _hwnd: isize, _name: &str, _control_type: i32) -> bool {
        false
    }
    fn element_find_any(&self, _hwnd: isize, _names: &[String], _types: &[i32]) -> Option<usize> {
        None
    }
    fn element_locate(&self, _hwnd: isize, _name: &str, _control_type: i32) -> Option<(i32, i32)> {
        None
    }
    fn element_locate_via(
        &self,
        _hwnd: isize,
        _via_name: &str,
        _via_type: i32,
        _name: &str,
        _control_type: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn element_plugin_locate(
        &self,
        _hwnd: isize,
        _container_name: &str,
        _name: &str,
        _control_type: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn element_dump(&self, _hwnd: isize) -> Vec<DumpNode> {
        Vec::new()
    }
    fn element_raw_dump(&self, _hwnd: isize) -> Vec<DumpNode> {
        Vec::new()
    }

    fn element_state_probe(
        &self,
        _hwnd: isize,
        _container_name: &str,
        _name: &str,
        _control_type: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn element_class_nav_point(
        &self,
        _hwnd: isize,
        _class_substr: &str,
        _ctype: i32,
        _child: i32,
        _sibling: i32,
    ) -> Option<(i32, i32)> {
        None
    }
    fn element_focus_step(&self, _hwnd: isize, _direction: i32) -> Option<(String, i32, i32, i32)> {
        None
    }
    fn set_captured_keys(&self, _keys: &[(u32, u8)]) {}
    fn set_key_scope(&self, _to_foreground: bool) {}
    fn set_menu_open(&self, _open: bool) {}
    fn modifiers_down(&self) -> bool {
        false
    }
    fn native_menu_open(&self) -> bool {
        false
    }
    fn watch_keys(&self) -> Result<(), String> {
        Ok(())
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
