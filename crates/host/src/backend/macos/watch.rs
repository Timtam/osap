//! Noticing that the user moved somewhere else.
//!
//! Three things count as "somewhere else", and the second is the one the whole embedded
//! overlay system rests on: the frontmost application changed, the keyboard focus moved
//! WITHIN an application (a plugin window inside a DAW that is already frontmost raises no
//! application-level event at all), or the frontmost window's title changed.

/// Idempotent.
pub fn start() -> Result<(), String> {
    Ok(())
}

/// Is a system menu open right now?
///
/// Called from inside the key path and from a 150 ms timer, so it has to be microseconds.
/// Whatever answers it, it must not be an accessibility traversal.
pub fn native_menu_open() -> bool {
    false
}
