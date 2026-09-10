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
//!
//! The table is the only thing in this file, in both directions, deliberately: two
//! hand-written maps drift, and a drift here is a hotkey that registers on a key nobody
//! pressed. The tests at the bottom check the round trip over every entry.
//!
//! Nothing here touches an OS API, or any other part of this backend, and that is on
//! purpose too: the only thing it borrows is the mask constants from `backend/mod.rs`,
//! which are plain integers, so `backend/mod.rs` can pull this file into an ordinary test
//! build on Windows — which is where the tests below actually get run.

/// Win32 virtual-key code and the macOS virtual keycode for the same physical key.
///
/// The macOS codes are from HIToolbox's `Events.h`. They are positions on a US keyboard
/// rather than characters, and they are **scattered**: the letters are not in alphabetical
/// order, digits 5 and 6 are swapped (0x17 / 0x16), and the function keys start at F1 =
/// 0x7A and go down. Nothing here can be computed; it is a table because it has to be.
const TABLE: &[(u32, u16)] = &[
    // Letters, VK_A..VK_Z.
    (0x41, 0x00), // A
    (0x42, 0x0B), // B
    (0x43, 0x08), // C
    (0x44, 0x02), // D
    (0x45, 0x0E), // E
    (0x46, 0x03), // F
    (0x47, 0x05), // G
    (0x48, 0x04), // H
    (0x49, 0x22), // I
    (0x4A, 0x26), // J
    (0x4B, 0x28), // K
    (0x4C, 0x25), // L
    (0x4D, 0x2E), // M
    (0x4E, 0x2D), // N
    (0x4F, 0x1F), // O
    (0x50, 0x23), // P
    (0x51, 0x0C), // Q
    (0x52, 0x0F), // R
    (0x53, 0x01), // S
    (0x54, 0x11), // T
    (0x55, 0x20), // U
    (0x56, 0x09), // V
    (0x57, 0x0D), // W
    (0x58, 0x07), // X
    (0x59, 0x10), // Y
    (0x5A, 0x06), // Z
    // Digits along the top row, VK_0..VK_9.
    (0x30, 0x1D), // 0
    (0x31, 0x12), // 1
    (0x32, 0x13), // 2
    (0x33, 0x14), // 3
    (0x34, 0x15), // 4
    (0x35, 0x17), // 5 — yes, above 6; the header really is out of order here
    (0x36, 0x16), // 6
    (0x37, 0x1A), // 7
    (0x38, 0x1C), // 8
    (0x39, 0x19), // 9
    // Function keys. `key_to_vk` allows F1..F24 by arithmetic (`0x70 + n - 1`); macOS has
    // codes for F1..F20 only, and they are in no order at all. F21..F24 have no macOS
    // equivalent and are absent on purpose — a module asking for one gets a `None` and a
    // log line, not a keycode belonging to something else.
    (0x70, 0x7A), // F1
    (0x71, 0x78), // F2
    (0x72, 0x63), // F3
    (0x73, 0x76), // F4
    (0x74, 0x60), // F5
    (0x75, 0x61), // F6
    (0x76, 0x62), // F7
    (0x77, 0x64), // F8
    (0x78, 0x65), // F9
    (0x79, 0x6D), // F10
    (0x7A, 0x67), // F11
    (0x7B, 0x6F), // F12
    (0x7C, 0x69), // F13
    (0x7D, 0x6B), // F14
    (0x7E, 0x71), // F15
    (0x7F, 0x6A), // F16
    (0x80, 0x40), // F17
    (0x81, 0x4F), // F18
    (0x82, 0x50), // F19
    (0x83, 0x5A), // F20
    // Editing and navigation. Two of these are asymmetric and bite: Apple's "Delete" is
    // Windows' Backspace, and what Windows calls Delete is Apple's Forward Delete. Insert
    // has no key on a Mac keyboard at all; Help sits in the same slot on the same block of
    // six, which is where a module aiming at that block expects it.
    (0x08, 0x33), // VK_BACK       -> kVK_Delete
    (0x09, 0x30), // VK_TAB        -> kVK_Tab
    (0x0D, 0x24), // VK_RETURN     -> kVK_Return
    (0x1B, 0x35), // VK_ESCAPE     -> kVK_Escape
    (0x20, 0x31), // VK_SPACE      -> kVK_Space
    (0x21, 0x74), // VK_PRIOR      -> kVK_PageUp
    (0x22, 0x79), // VK_NEXT       -> kVK_PageDown
    (0x23, 0x77), // VK_END        -> kVK_End
    (0x24, 0x73), // VK_HOME       -> kVK_Home
    (0x25, 0x7B), // VK_LEFT       -> kVK_LeftArrow
    (0x26, 0x7E), // VK_UP         -> kVK_UpArrow
    (0x27, 0x7C), // VK_RIGHT      -> kVK_RightArrow
    (0x28, 0x7D), // VK_DOWN       -> kVK_DownArrow
    (0x2D, 0x72), // VK_INSERT     -> kVK_Help
    (0x2E, 0x75), // VK_DELETE     -> kVK_ForwardDelete
    // The modifiers as keys in their own right. `key_spec` produces exactly these four for
    // a "<modifier> tap" spec, and the generic VK is what the host stores, so the generic
    // VK is what the left-hand key maps to; the right-hand keys come back to the same
    // generic VK through `REVERSE_ONLY` below.
    (0x10, 0x38), // VK_SHIFT      -> kVK_Shift
    (0x11, 0x3B), // VK_CONTROL    -> kVK_Control
    (0x12, 0x3A), // VK_MENU       -> kVK_Option
    (0x5B, 0x37), // VK_LWIN       -> kVK_Command
    // Punctuation. `key_to_vk` cannot produce any of these, so nothing can bind them today;
    // they are here for the event tap, which sees whatever the user presses and has to name
    // it. **US layout on both sides** — the OEM virtual keys and the ANSI keycodes are both
    // physical positions, and on a German keyboard neither means what its comment says.
    (0xBA, 0x29), // VK_OEM_1      ;
    (0xBB, 0x18), // VK_OEM_PLUS   =
    (0xBC, 0x2B), // VK_OEM_COMMA  ,
    (0xBD, 0x1B), // VK_OEM_MINUS  -
    (0xBE, 0x2F), // VK_OEM_PERIOD .
    (0xBF, 0x2C), // VK_OEM_2      /
    (0xC0, 0x32), // VK_OEM_3      `
    (0xDB, 0x21), // VK_OEM_4      [
    (0xDC, 0x2A), // VK_OEM_5      \
    (0xDD, 0x1E), // VK_OEM_6      ]
    (0xDE, 0x27), // VK_OEM_7      '
    // The keypad, for the tap's benefit as well.
    (0x60, 0x52), // VK_NUMPAD0
    (0x61, 0x53), // VK_NUMPAD1
    (0x62, 0x54), // VK_NUMPAD2
    (0x63, 0x55), // VK_NUMPAD3
    (0x64, 0x56), // VK_NUMPAD4
    (0x65, 0x57), // VK_NUMPAD5
    (0x66, 0x58), // VK_NUMPAD6
    (0x67, 0x59), // VK_NUMPAD7
    (0x68, 0x5B), // VK_NUMPAD8
    (0x69, 0x5C), // VK_NUMPAD9
    (0x6A, 0x43), // VK_MULTIPLY
    (0x6B, 0x45), // VK_ADD
    (0x6D, 0x4E), // VK_SUBTRACT
    (0x6E, 0x41), // VK_DECIMAL
    (0x6F, 0x4B), // VK_DIVIDE
];

/// Keys the tap can see that have no virtual key of their own on the way out.
///
/// Read only by [`keycode_to_vk`]. Windows reports left and right modifiers as the one
/// generic virtual key — the hook folds `VK_LMENU`/`VK_RMENU` into `VK_MENU` so that a
/// capture of "Alt" answers to either — and the host's captured-key table is written in
/// that vocabulary, so the right-hand keys have to arrive as the same code here. The
/// keypad's Enter is the same story: Windows calls it `VK_RETURN` with the extended flag,
/// which the host never looks at.
const REVERSE_ONLY: &[(u16, u32)] = &[
    (0x3C, 0x10), // kVK_RightShift   -> VK_SHIFT
    (0x3E, 0x11), // kVK_RightControl -> VK_CONTROL
    (0x3D, 0x12), // kVK_RightOption  -> VK_MENU
    (0x36, 0x5B), // kVK_RightCommand -> VK_LWIN
    (0x4C, 0x0D), // kVK_ANSI_KeypadEnter -> VK_RETURN
];

/// Carbon's modifier bits, from HIToolbox `Events.h`.
///
/// They live here rather than with the other Carbon declarations in [`super::ffi`] because
/// they are key vocabulary, and keeping them here is what lets this file stay free of the
/// `extern` block and be compiled — and tested — on any platform.
///
/// These are **not** `NSEventModifierFlags`, which the same four modifiers also have and
/// which are entirely different numbers (`NSEventModifierFlagCommand` is `1 << 20`). Mixing
/// the two registers a hotkey that never fires and reports no error at all.
const CMD_KEY: u32 = 1 << 8; // cmdKey     0x0100
const SHIFT_KEY: u32 = 1 << 9; // shiftKey   0x0200
const OPTION_KEY: u32 = 1 << 11; // optionKey  0x0800
const CONTROL_KEY: u32 = 1 << 12; // controlKey 0x1000

/// Win32 virtual-key code to the macOS virtual keycode a `CGEvent` needs.
///
/// `None` for a key macOS has no code for — F21 and upwards are the only ones the host can
/// currently ask for. The caller logs it; the table cannot, because it has no business
/// knowing whether the miss came from a module's hotkey or from a synthetic keystroke.
pub fn vk_to_keycode(vk: u32) -> Option<u16> {
    // A scan of a hundred-odd entries, in a burst of tens on a window switch: microseconds
    // in total. A lazily built map would cost more to maintain than it saves.
    TABLE.iter().find(|(v, _)| *v == vk).map(|(_, code)| *code)
}

/// The reverse, for reporting a key the tap saw.
// The tap is the only caller and it is being written alongside this file; the attribute
// goes away with the first call from `tap.rs`.
#[allow(dead_code)]
pub fn keycode_to_vk(code: u16) -> Option<u32> {
    TABLE
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(vk, _)| *vk)
        .or_else(|| REVERSE_ONLY.iter().find(|(c, _)| *c == code).map(|(_, vk)| *vk))
}

/// The host's modifier mask (`MASK_SHIFT` and friends) as Carbon's modifier bits.
///
/// `MASK_TAP` has no meaning here and is ignored: a bare modifier tap is not a combination
/// the OS can register, it is something the event tap recognises. `hotkey::register`
/// refuses such a spec outright rather than registering a modifier-less hotkey on whatever
/// key happened to be left.
pub fn mask_to_carbon(mask: u8) -> u32 {
    let mut out = 0u32;
    if mask & crate::backend::MASK_SHIFT != 0 {
        out |= SHIFT_KEY;
    }
    if mask & crate::backend::MASK_CTRL != 0 {
        out |= CONTROL_KEY;
    }
    if mask & crate::backend::MASK_ALT != 0 {
        out |= OPTION_KEY;
    }
    if mask & crate::backend::MASK_WIN != 0 {
        out |= CMD_KEY;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{key_spec, key_to_vk, MASK_ALT, MASK_CTRL, MASK_SHIFT, MASK_WIN};

    /// Every entry survives the trip out and back. This is the property the rest of the
    /// backend depends on: the tap reports what the host asked to capture, or the key is
    /// swallowed and never announced.
    #[test]
    fn vk_round_trips_through_the_keycode() {
        for &(vk, code) in TABLE {
            assert_eq!(vk_to_keycode(vk), Some(code), "vk {vk:#04X} lost its keycode");
            assert_eq!(keycode_to_vk(code), Some(vk), "keycode {code:#04X} came back as another");
        }
    }

    /// A duplicate on either side would make one of the two keys unreachable, and the
    /// linear scans above would hide which one by silently answering with the first.
    #[test]
    fn the_table_is_a_bijection() {
        let mut vks: Vec<u32> = TABLE.iter().map(|(v, _)| *v).collect();
        let before = vks.len();
        vks.sort_unstable();
        vks.dedup();
        assert_eq!(vks.len(), before, "the same virtual key appears twice");

        let mut codes: Vec<u16> = TABLE.iter().map(|(_, c)| *c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), before, "the same macOS keycode appears twice");

        for (code, _) in REVERSE_ONLY {
            assert!(
                !TABLE.iter().any(|(_, c)| c == code),
                "keycode {code:#04X} is in both tables; the reverse lookup would never reach it"
            );
        }
    }

    /// The table has to cover everything the shared parser can produce, or a module's
    /// hotkey fails on macOS for no reason a user could see. F21..F24 are the documented
    /// exception: macOS has no keycode for them.
    #[test]
    fn every_key_the_shared_parser_accepts_is_mapped() {
        let mut names: Vec<String> = Vec::new();
        for c in 'a'..='z' {
            names.push(c.to_string());
        }
        for c in '0'..='9' {
            names.push(c.to_string());
        }
        for n in 1..=20 {
            names.push(format!("F{n}"));
        }
        for n in [
            "space", "enter", "return", "esc", "escape", "tab", "backspace", "delete", "del", "up",
            "down", "left", "right", "home", "end", "pageup", "pagedown",
        ] {
            names.push(n.to_string());
        }

        for name in &names {
            let vk = key_to_vk(name).unwrap_or_else(|| panic!("key_to_vk lost '{name}'"));
            assert!(
                vk_to_keycode(vk).is_some(),
                "'{name}' parses to vk {vk:#04X}, which has no macOS keycode"
            );
        }

        for n in 21..=24 {
            let vk = key_to_vk(&format!("F{n}")).unwrap();
            assert_eq!(vk_to_keycode(vk), None, "F{n} should have no macOS keycode");
        }
    }

    /// The four modifiers a "<modifier> tap" spec resolves to are keys as well, and the tap
    /// has to be able to name them when it sees one pressed on its own.
    #[test]
    fn bare_modifier_taps_have_keycodes() {
        for spec in ["Alt tap", "Ctrl tap", "Shift tap", "Cmd tap"] {
            let (vk, _mask) = key_spec(spec).unwrap_or_else(|| panic!("key_spec lost '{spec}'"));
            assert!(vk_to_keycode(vk).is_some(), "'{spec}' has no macOS keycode");
        }
    }

    #[test]
    fn modifiers_map_by_position() {
        assert_eq!(mask_to_carbon(MASK_SHIFT), 0x0200);
        assert_eq!(mask_to_carbon(MASK_CTRL), 0x1000);
        assert_eq!(mask_to_carbon(MASK_ALT), 0x0800);
        assert_eq!(mask_to_carbon(MASK_WIN), 0x0100);
        assert_eq!(mask_to_carbon(MASK_CTRL | MASK_SHIFT | MASK_ALT | MASK_WIN), 0x1B00);
        assert_eq!(mask_to_carbon(0), 0);
    }

    /// The reload key the host registers for itself on a Mac, spelled out. If this one stops
    /// translating, the tester loses the only way to reload a module without a mouse.
    ///
    /// Command-Shift, not the four-modifier chord Windows uses: that one carries
    /// Control-Option, VoiceOver's modifier, and the third Mac session measured it as
    /// registered and never delivered. The old spelling still has to translate — a module
    /// may spell a chord that way — it just must not be the host's own key any more.
    #[test]
    fn the_reload_hotkey_translates() {
        let (vk, mask) = key_spec("Cmd+Shift+F5").unwrap();
        assert_eq!(vk_to_keycode(vk), Some(0x60));
        assert_eq!(mask_to_carbon(mask), 0x0300);
        assert_eq!(
            mask & (MASK_CTRL | MASK_ALT),
            0,
            "the Mac reload key must stay off VoiceOver's modifier"
        );
        let (vk, mask) = key_spec("Ctrl+Shift+Win+Alt+F5").unwrap();
        assert_eq!(vk_to_keycode(vk), Some(0x60));
        assert_eq!(mask_to_carbon(mask), 0x1B00);
    }
}
