//! Global shortcuts, through Carbon.
//!
//! Deliberately not an event tap. A registered hotkey needs no Input Monitoring permission
//! and cannot be silently disabled, which matters because these arrive in bursts — every
//! visible control of an overlay claims one, so a single window switch registers and
//! unregisters tens of them.

pub fn register(_id: i32, spec: &str) -> Result<(), String> {
    Err(format!("hotkeys are not implemented on macOS yet (spec '{spec}')"))
}

pub fn unregister(_id: i32) {}
