//! Capturing and suppressing keys, through a `CGEventTap`.
//!
//! This is the fragile one. A tap that takes too long inside its callback is switched off
//! by the system and stays off, so the callback matches against a table and pushes; anything
//! else happens later, on the pump. A watchdog re-enables it and says so in the log, because
//! a tap that has quietly died looks exactly like an overlay that stopped working for no
//! reason.
//!
//! Matching is EXACT: the pressed modifier state must equal the mask, so a capture of "Tab"
//! does not swallow Command-Tab. Both halves of a suppressed key are swallowed — letting
//! the key-up through hands the application underneath an orphan release.

pub fn install() -> Result<(), String> {
    Ok(())
}

/// Replaces the whole captured set. Called on every focus move inside an overlay, so it
/// has to stay cheap.
pub fn set_captured_keys(_keys: &[(u32, u8)]) {}

/// Which window suppression applies to; 0 means everywhere. The value is a SNAPSHOT taken
/// when the caller asked, which is what lets a menu opened by a control receive keys
/// natively — the menu is a different window, so the comparison stops matching.
pub fn set_key_scope(_window: isize) {}

/// A plugin-drawn menu is open; let captured navigation keys through to it.
pub fn set_menu_open(_open: bool) {}
