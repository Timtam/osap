//! Fallback backend for platforms without a real implementation yet.
//! Window queries return empty; hotkey registration returns an error.

use super::{Backend, HostEvents, WinInfo};

pub struct StubBackend;

impl Backend for StubBackend {
    fn enumerate_windows(&self) -> Vec<WinInfo> {
        Vec::new()
    }

    fn active_window(&self) -> Option<WinInfo> {
        None
    }

    fn register_hotkey(&self, _id: i32, spec: &str) -> Result<(), String> {
        Err(format!(
            "hotkeys are not implemented on this platform yet (spec '{spec}')"
        ))
    }

    fn run_event_loop(&self, _events: &mut dyn HostEvents) -> Result<(), String> {
        Ok(())
    }
}
