//! Translating between the platform's key currency and the one macOS speaks.
//!
//! `key_spec` in `backend/mod.rs` is shared code and produces **Win32 virtual-key codes**
//! on every platform, because that is what every module and every hotkey string in the
//! system already means. Nothing about that changes here: the codes are translated at this
//! edge and translated back before an event is reported, so the host's captured-key table,
//! its conflict detection and the specs a module author writes all stay in one vocabulary.
//!
//! Modifiers map by POSITION, not by name — Ctrl to Control, Alt to Option, Win/Cmd to
//! Command — so a ported overlay keeps its finger on the same physical key.

/// Win32 virtual-key code to the macOS virtual keycode a `CGEvent` needs.
pub fn vk_to_keycode(_vk: u32) -> Option<u16> {
    None
}

/// The reverse, for reporting a key the tap saw.
pub fn keycode_to_vk(_code: u16) -> Option<u32> {
    None
}

/// The host's modifier mask (`MASK_SHIFT` and friends) as Carbon's modifier bits.
pub fn mask_to_carbon(_mask: u8) -> u32 {
    0
}
