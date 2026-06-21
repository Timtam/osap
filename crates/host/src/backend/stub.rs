//! Fallback backend for platforms without a real implementation yet.
//! Window queries return empty; hotkey registration returns an error.

use super::{Backend, CapturedImage, HostEvents, WinInfo};

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

    fn register_hotkey(&self, _id: i32, spec: &str) -> Result<(), String> {
        Err(format!(
            "hotkeys are not implemented on this platform yet (spec '{spec}')"
        ))
    }

    fn watch_foreground(&self) -> Result<(), String> {
        Ok(())
    }

    fn run_event_loop(&self, _events: &mut dyn HostEvents) -> Result<(), String> {
        Ok(())
    }
}
