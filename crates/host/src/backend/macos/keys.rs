//! Translating between the platform's key currency and the one macOS speaks.
//!
//! `key_spec` in `backend/mod.rs` is shared code and produces **Win32 virtual-key codes**
//! on every platform, because that is what every module and every hotkey string in the
//! system already means. Nothing about that changes here: the codes are translated at this
//! edge and translated back before an event is reported, so the host's captured-key table,
//! its conflict detection and the specs a module author writes all stay in one vocabulary.
//!
//! **Modifiers map by role**, the Qt way (decided 2026-09-22; see `KeyOs` in `backend/mod.rs`):
//! the host's `MASK_CTRL` is Command here, `MASK_ALT` Option, `MASK_SHIFT` Shift and `MASK_WIN`
//! Control, so `"Ctrl+C"` copies on both platforms. [`MODIFIER_KEYS`] is that table, the one
//! place a role becomes a Mac key and back — for Carbon's hotkey bits, the event flags `key_send`
//! sets and the event tap reads, the layout's modifier state, and the modifier keys themselves.
//!
//! **Letters map by the keyboard layout**, the way Windows maps them: `"Z"` is the key that
//! types z, which on a German keyboard is the one where a US keyboard has Y. Windows gets that
//! for free, because its letter virtual keys follow the layout's labels; a macOS key code is a
//! position, so the letters are looked up in the current layout ([`Letters`], filled by
//! `layout.rs` through `UCKeyTranslate`) and the fixed table below is only their fallback.
//! Two tables, because a layout can type other letters while Command is held — "Dvorak –
//! QWERTY ⌘" types QWERTY then, so that shortcuts stay where a QWERTY user's hands expect
//! them — and a combination that holds Command is looked up in the Command table
//! ([`letters_for`]). For every other layout the two are the same.
//! Everything else stays positional: digits, function keys, the named keys and punctuation.
//! A digit keeps its position because the layouts that move digits move them to Shift (the
//! French top row types `&` unshifted and `1` shifted, and Windows' French layout still calls
//! that key `VK_1`), and punctuation because nothing in the spec grammar can name it.
//!
//! The table is the only fixed map in this file, in both directions, deliberately: two
//! hand-written maps drift, and a drift here is a hotkey that registers on a key nobody
//! pressed. The tests at the bottom check the round trip over every entry, and over every
//! layout they describe.
//!
//! Nothing here touches an OS API, or any other part of this backend, and that is on
//! purpose too: the only thing it borrows is the mask constants from `backend/mod.rs`,
//! which are plain integers, so `backend/mod.rs` can pull this file into an ordinary test
//! build on Windows — which is where the tests below actually get run. The layout itself is
//! read in `layout.rs`, which hands this file a closure and never an OS type.

use std::sync::atomic::{AtomicU16, Ordering};

/// Win32 virtual-key code and the macOS virtual keycode for the same physical key — except the
/// four modifiers, whose virtual keys are roles and map to the key each role is here
/// ([`MODIFIER_KEYS`]).
///
/// The macOS codes are from HIToolbox's `Events.h`. They are positions on a US keyboard
/// rather than characters, and they are **scattered**: the letters are not in alphabetical
/// order, digits 5 and 6 are swapped (0x17 / 0x16), and the function keys start at F1 =
/// 0x7A and go down. Nothing here can be computed; it is a table because it has to be.
const TABLE: &[(u32, u16)] = &[
    // Letters, VK_A..VK_Z, on their US positions: what a letter falls back to when no layout
    // has been read, or when the layout types it nowhere. The letters in force are `CURRENT`.
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
    // generic VK through `REVERSE_ONLY` below. By ROLE, as in `MODIFIER_KEYS` (a test holds
    // the two together): the Ctrl role's VK is the Command key, the Win role's the Control key.
    (0x10, 0x38), // VK_SHIFT      -> kVK_Shift
    (0x11, 0x37), // VK_CONTROL    -> kVK_Command (the Ctrl role)
    (0x12, 0x3A), // VK_MENU       -> kVK_Option
    (0x5B, 0x3B), // VK_LWIN       -> kVK_Control (the Win role)
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
    (0x36, 0x11), // kVK_RightCommand -> VK_CONTROL (the Ctrl role)
    (0x3D, 0x12), // kVK_RightOption  -> VK_MENU
    (0x3E, 0x5B), // kVK_RightControl -> VK_LWIN (the Win role)
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

/// Quartz's event flags for the four modifiers, `kCGEventFlagMask*` from `CGEventTypes.h`: the
/// numbers `CGEventFlags` wraps, written out so that this file stays free of the bindings. The
/// event tap asserts at compile time that they are the bindings' own.
pub const CG_FLAG_SHIFT: u64 = 0x0002_0000;
pub const CG_FLAG_CONTROL: u64 = 0x0004_0000;
pub const CG_FLAG_ALTERNATE: u64 = 0x0008_0000;
pub const CG_FLAG_COMMAND: u64 = 0x0010_0000;

/// One of the host's modifier roles, as the Mac key it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacModifier {
    /// The role: `MASK_CTRL`, `MASK_ALT`, `MASK_SHIFT` or `MASK_WIN`.
    pub mask: u8,
    /// The virtual key the host knows the role by as a key of its own — what a tap spec names.
    pub vk: u32,
    /// The key's `kVK_*` codes, left-hand key first.
    pub keycodes: [u16; 2],
    /// Carbon's modifier bit, for `RegisterEventHotKey`.
    pub carbon: u32,
    /// Quartz's event flag, for `CGEventSetFlags` and for reading an event.
    pub cg_flag: u64,
    /// What the key is called.
    pub name: &'static str,
}

/// The role table: which Mac key each of the host's modifier roles is. The Qt convention —
/// `Qt::ControlModifier` is Command on macOS and `Qt::MetaModifier` Control — so a spec's Ctrl
/// is Command here, its Win is Control, and Alt and Shift are Option and Shift.
pub const MODIFIER_KEYS: [MacModifier; 4] = [
    MacModifier {
        mask: crate::backend::MASK_CTRL,
        vk: 0x11,
        keycodes: [0x37, 0x36],
        carbon: CMD_KEY,
        cg_flag: CG_FLAG_COMMAND,
        name: "Command",
    },
    MacModifier {
        mask: crate::backend::MASK_ALT,
        vk: 0x12,
        keycodes: [0x3A, 0x3D],
        carbon: OPTION_KEY,
        cg_flag: CG_FLAG_ALTERNATE,
        name: "Option",
    },
    MacModifier {
        mask: crate::backend::MASK_SHIFT,
        vk: 0x10,
        keycodes: [0x38, 0x3C],
        carbon: SHIFT_KEY,
        cg_flag: CG_FLAG_SHIFT,
        name: "Shift",
    },
    MacModifier {
        mask: crate::backend::MASK_WIN,
        vk: 0x5B,
        keycodes: [0x3B, 0x3E],
        carbon: CONTROL_KEY,
        cg_flag: CG_FLAG_CONTROL,
        name: "Control",
    },
];

/// "No key types this letter."
const NO_KEY: u16 = u16::MAX;

/// The US positions of A to Z: the letters of [`TABLE`], in alphabetical order (a test checks
/// the two agree). What every letter falls back to, and what they all are until a layout has
/// been read.
const US_LETTERS: [u16; 26] = [
    0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, // A..M
    0x2D, 0x1F, 0x23, 0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06, // N..Z
];

/// The keys a layout is asked about: every key of the typing block that can carry a letter.
///
/// The letter positions first, in alphabetical order, so a letter found on its own US position
/// is found there first; then the punctuation positions (French puts M where US has `;`); the
/// ISO key beside the left Shift (`kVK_ISO_Section`); the digit row last. Not the keypad, the
/// function keys or the named keys — no layout types a letter on those, and asking would
/// only open the door to a keypad key answering for one.
const CANDIDATES: [u16; 48] = [
    // US letter positions, A..Z.
    0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F,
    0x23, 0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
    // Punctuation positions, as in TABLE.
    0x29, 0x18, 0x2B, 0x1B, 0x2F, 0x2C, 0x32, 0x21, 0x2A, 0x1E, 0x27,
    // kVK_ISO_Section.
    0x0A,
    // The digit row, 0..9.
    0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19,
];

/// Which key types each letter A to Z under one keyboard layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Letters {
    codes: [u16; 26],
}

impl Letters {
    /// A US keyboard: every letter on its own position.
    pub const US: Letters = Letters { codes: US_LETTERS };

    const EMPTY: Letters = Letters { codes: [NO_KEY; 26] };

    /// The key code that types `letter` (b'A'..=b'Z', either case), if any does.
    pub fn code(&self, letter: u8) -> Option<u16> {
        let i = letter.to_ascii_uppercase().checked_sub(b'A').filter(|i| *i < 26)? as usize;
        Some(self.codes[i]).filter(|c| *c != NO_KEY)
    }

    /// The letter a key code types, as its virtual key (VK_A..VK_Z).
    fn letter_at(&self, code: u16) -> Option<u32> {
        self.codes.iter().position(|c| *c == code).map(|i| 0x41 + i as u32)
    }

    /// How many letters some key types.
    pub fn count(&self) -> usize {
        self.codes.iter().filter(|c| **c != NO_KEY).count()
    }

    /// Every letter no key types moved to its US position — unless that position types another
    /// letter here, in which case it stays without a key: two letters on one key would make the
    /// reverse lookup answer with one of them for a press of the other.
    pub fn completed(mut self) -> Letters {
        for i in 0..26 {
            if self.codes[i] != NO_KEY {
                continue;
            }
            let us = US_LETTERS[i];
            if !self.codes.contains(&us) {
                self.codes[i] = us;
            }
        }
        self
    }

    /// The letters that are not on their US position, as (letter, key code), for the log.
    pub fn moved(&self) -> Vec<(char, Option<u16>)> {
        (0..26)
            .filter(|&i| self.codes[i] != US_LETTERS[i])
            .map(|i| ((b'A' + i as u8) as char, Some(self.codes[i]).filter(|c| *c != NO_KEY)))
            .collect()
    }
}

/// The letters of one layout, from what each candidate key types in it — with no modifier
/// held, or with Command held for the Command table; the caller's `translate` decides which.
///
/// `translate` is the layout, as a function: what the key with this code types, or `None`.
/// Only a single ASCII letter counts; anything else — punctuation, a digit, a letter with an
/// accent, a Cyrillic one — is not a letter the spec grammar can name. A letter two keys type
/// goes to its US position when that is one of them (it is the key the user's hands already
/// expect), otherwise to the first key asked.
pub fn letters_from(mut translate: impl FnMut(u16) -> Option<char>) -> Letters {
    let mut out = Letters::EMPTY;
    for &code in &CANDIDATES {
        let Some(c) = translate(code) else { continue };
        if !c.is_ascii_alphabetic() {
            continue;
        }
        let i = (c.to_ascii_uppercase() as u8 - b'A') as usize;
        if out.codes[i] == NO_KEY || code == US_LETTERS[i] {
            out.codes[i] = code;
        }
    }
    out
}

/// Which of the two layouts the system offers supplies the letters: the current keyboard
/// layout, unless the ASCII-capable one types more of them — a Russian or Greek layout types
/// none, and the system's ASCII-capable layout is then the one a shortcut is typed on. Letters
/// neither supplies fall back to their US position ([`Letters::completed`]); with neither
/// layout readable, the result is the US keyboard.
pub fn pick(current: Option<Letters>, ascii_capable: Option<Letters>) -> Letters {
    let best = match (current, ascii_capable) {
        (Some(c), Some(a)) if a.count() > c.count() => a,
        (Some(c), _) => c,
        (None, Some(a)) => a,
        (None, None) => return Letters::US,
    };
    best.completed()
}

/// One table of letters in force, one atomic per letter so the event tap can read them without
/// a lock. Written by `layout.rs` on the main thread; every reader is on the main thread as
/// well, so a half-written table is never seen in practice, and would cost one keystroke if it
/// were.
type LiveLetters = [AtomicU16; 26];

const fn us_live() -> LiveLetters {
    const fn at(i: usize) -> AtomicU16 {
        AtomicU16::new(US_LETTERS[i])
    }
    [
        at(0), at(1), at(2), at(3), at(4), at(5), at(6), at(7), at(8), at(9), at(10), at(11),
        at(12), at(13), at(14), at(15), at(16), at(17), at(18), at(19), at(20), at(21), at(22),
        at(23), at(24), at(25),
    ]
}

/// The letters in force with no modifier held — and with Control, Option or Shift, which no
/// layout's shortcut behaviour depends on the way it can on Command.
static CURRENT: LiveLetters = us_live();
/// The letters in force with Command held.
static CURRENT_CMD: LiveLetters = us_live();

fn load(live: &LiveLetters) -> Letters {
    let mut out = Letters::EMPTY;
    for (i, slot) in live.iter().enumerate() {
        out.codes[i] = slot.load(Ordering::Relaxed);
    }
    out
}

fn swap(live: &LiveLetters, letters: Letters) -> Letters {
    let mut before = Letters::EMPTY;
    for (i, slot) in live.iter().enumerate() {
        before.codes[i] = slot.swap(letters.codes[i], Ordering::Relaxed);
    }
    before
}

/// Puts `letters` (no modifier held) and `command` (Command held) in force for every
/// translation below. Returns what was in force before, in the same order.
// Only `layout.rs` writes them, and no test does: the tests share these tables with each other.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn set_letters(letters: Letters, command: Letters) -> (Letters, Letters) {
    (swap(&CURRENT, letters), swap(&CURRENT_CMD, command))
}

/// The letters in force with no modifier held.
pub fn current_letters() -> Letters {
    load(&CURRENT)
}

/// The table a combination holding `mask` is looked up in: the Command one when it holds
/// Command (`MASK_CTRL`, the Ctrl role, which is Command here), the plain one otherwise.
pub fn letters_for<'a>(plain: &'a Letters, command: &'a Letters, mask: u8) -> &'a Letters {
    if mask & crate::backend::MASK_CTRL != 0 {
        command
    } else {
        plain
    }
}

/// The letters in force for a combination holding `mask` — see [`letters_for`].
pub fn current_letters_for(mask: u8) -> Letters {
    let (plain, command) = (load(&CURRENT), load(&CURRENT_CMD));
    *letters_for(&plain, &command, mask)
}

/// Win32 virtual-key code to the macOS virtual keycode a `CGEvent` needs, under the letters
/// in force with no modifier held — see [`vk_to_keycode_in`].
pub fn vk_to_keycode(vk: u32) -> Option<u16> {
    vk_to_keycode_in(&current_letters(), vk)
}

/// The same for a key pressed with `mask` held: a letter comes from the Command table when
/// `mask` holds Command.
pub fn vk_to_keycode_for(vk: u32, mask: u8) -> Option<u16> {
    vk_to_keycode_in(&current_letters_for(mask), vk)
}

/// Win32 virtual-key code to the macOS virtual keycode, under `letters`.
///
/// `None` for a key macOS has no code for — F21 and upwards are the only ones the host can
/// currently ask for — and for a letter no key of the layout types. The caller logs it; the
/// table cannot, because it has no business knowing whether the miss came from a module's
/// hotkey or from a synthetic keystroke.
pub fn vk_to_keycode_in(letters: &Letters, vk: u32) -> Option<u16> {
    if (0x41..=0x5A).contains(&vk) {
        return letters.code(vk as u8);
    }
    // A scan of a hundred-odd entries, in a burst of tens on a window switch: microseconds
    // in total. A lazily built map would cost more to maintain than it saves.
    TABLE.iter().find(|(v, _)| *v == vk).map(|(_, code)| *code)
}

/// Why [`vk_to_keycode_in`] found no key for `vk`, for an error or a log line: a letter is
/// missing from the layout, anything else from macOS. One wording for the hotkey, `key_send`
/// and `key_post`, so that a letter the layout does not type is never reported as a key macOS
/// lacks.
// Its callers are macOS-only; the wording is tested anywhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn why_no_keycode(vk: u32) -> &'static str {
    if (0x41..=0x5A).contains(&vk) {
        "no key of the current keyboard layout types this letter"
    } else {
        "macOS has no key code for it"
    }
}

/// The reverse, for reporting a key the tap saw, under the letters in force with no modifier
/// held.
pub fn keycode_to_vk(code: u16) -> Option<u32> {
    keycode_to_vk_in(&current_letters(), code)
}

/// The same for a key the tap saw with `mask` held: with Command, the Command table names it.
pub fn keycode_to_vk_for(code: u16, mask: u8) -> Option<u32> {
    keycode_to_vk_in(&current_letters_for(mask), code)
}

/// The reverse under `letters`. A key that types a letter is that letter, wherever it sits —
/// on a French keyboard the key where US has `;` is M. A US letter position that types no
/// letter in this layout names nothing (on the same keyboard, the key where US has M types a
/// comma, which no spec can name). Everything else is positional, as in [`TABLE`].
pub fn keycode_to_vk_in(letters: &Letters, code: u16) -> Option<u32> {
    if let Some(vk) = letters.letter_at(code) {
        return Some(vk);
    }
    if US_LETTERS.contains(&code) {
        return None;
    }
    TABLE
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(vk, _)| *vk)
        .or_else(|| REVERSE_ONLY.iter().find(|(c, _)| *c == code).map(|(_, vk)| *vk))
}

/// The host's modifier mask (`MASK_SHIFT` and friends, which are roles) as Carbon's modifier
/// bits, through [`MODIFIER_KEYS`]: `MASK_CTRL` is `cmdKey`, `MASK_WIN` is `controlKey`.
///
/// `MASK_TAP` has no meaning here and is ignored: a bare modifier tap is not a combination
/// the OS can register, it is something the event tap recognises. `hotkey::register`
/// refuses such a spec outright rather than registering a modifier-less hotkey on whatever
/// key happened to be left.
pub fn mask_to_carbon(mask: u8) -> u32 {
    MODIFIER_KEYS.iter().filter(|m| mask & m.mask != 0).fold(0, |out, m| out | m.carbon)
}

/// The host's modifier mask as Quartz event flags, for the events `key_send` posts: the Ctrl
/// role is `kCGEventFlagMaskCommand`, the Win role `kCGEventFlagMaskControl`.
// Its callers are macOS-only; the table is tested anywhere.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn mask_to_cg_flags(mask: u8) -> u64 {
    MODIFIER_KEYS.iter().filter(|m| mask & m.mask != 0).fold(0, |out, m| out | m.cg_flag)
}

/// The reverse, for a key the event tap saw: the roles held, out of an event's own flags.
///
/// Only these four flags: an arrow key on macOS also carries `MaskSecondaryFn` and
/// `MaskNumericPad`, and Caps Lock `MaskAlphaShift`, none of which is a role — comparing whole
/// flag sets for equality would mean no arrow key ever matched a capture of "Down".
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn mask_of_cg_flags(flags: u64) -> u8 {
    MODIFIER_KEYS.iter().filter(|m| flags & m.cg_flag != 0).fold(0, |out, m| out | m.mask)
}

/// The host's modifier mask as `UCKeyTranslate`'s modifier state: Carbon's modifier bits shifted
/// down by eight and cut to a byte (`UnicodeUtilities.h`: "(EventRecord.modifiers >> 8) &
/// 0xFF"), so Command held (`MASK_CTRL`) is 0x01 and Option 0x08. `layout.rs` asks the layout with it for both
/// of its questions — which key types a letter with Command held, and what Option with a key
/// composes — and it lives here, beside [`mask_to_carbon`], so that the arithmetic is tested
/// anywhere.
pub fn translate_modifier_state(mask: u8) -> u32 {
    (mask_to_carbon(mask) >> 8) & 0xFF
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
        for spec in ["Alt tap", "Ctrl tap", "Shift tap", "Cmd tap", "Win tap", "Meta tap"] {
            let (vk, _mask) = key_spec(spec).unwrap_or_else(|| panic!("key_spec lost '{spec}'"));
            assert!(vk_to_keycode(vk).is_some(), "'{spec}' has no macOS keycode");
        }
    }

    /// The role table is the Qt convention: Ctrl is Command, Win is Control. Its names are the
    /// ones `describe` says on a Mac, each role is in it once, and its virtual keys are the ones a
    /// tap spec of that role parses to.
    #[test]
    fn the_role_table_is_the_qt_convention() {
        use crate::backend::{role_words, KeyOs};
        let by_role = |mask| MODIFIER_KEYS.iter().find(|m| m.mask == mask).unwrap();
        assert_eq!(by_role(MASK_CTRL).name, "Command");
        assert_eq!(by_role(MASK_CTRL).keycodes, [0x37, 0x36]);
        assert_eq!(by_role(MASK_WIN).name, "Control");
        assert_eq!(by_role(MASK_WIN).keycodes, [0x3B, 0x3E]);
        assert_eq!(by_role(MASK_ALT).name, "Option");
        assert_eq!(by_role(MASK_ALT).keycodes, [0x3A, 0x3D]);
        assert_eq!(by_role(MASK_SHIFT).name, "Shift");
        assert_eq!(by_role(MASK_SHIFT).keycodes, [0x38, 0x3C]);
        let mut masks: Vec<u8> = MODIFIER_KEYS.iter().map(|m| m.mask).collect();
        masks.sort_unstable();
        assert_eq!(masks, vec![MASK_SHIFT, MASK_CTRL, MASK_ALT, MASK_WIN]);
        for w in role_words(KeyOs::Macos) {
            assert_eq!(by_role(w.role).name, w.spoken, "describe and the backend name one key");
            let tap = format!("{} tap", w.spec);
            assert_eq!(key_spec(&tap), Some((by_role(w.role).vk, crate::backend::MASK_TAP)), "{tap}");
        }
    }

    /// The key table's modifier keys are the role table's: a tap of the Ctrl role is a press of
    /// either Command key, and the tap reports either Command key as that role's key.
    #[test]
    fn the_modifier_keys_follow_the_roles() {
        for m in MODIFIER_KEYS {
            assert_eq!(vk_to_keycode(m.vk), Some(m.keycodes[0]), "{}", m.name);
            for code in m.keycodes {
                assert_eq!(keycode_to_vk(code), Some(m.vk), "{} key {code:#04X}", m.name);
            }
        }
    }

    #[test]
    fn modifiers_map_by_role() {
        assert_eq!(mask_to_carbon(MASK_SHIFT), 0x0200);
        assert_eq!(mask_to_carbon(MASK_CTRL), 0x0100, "the Ctrl role is cmdKey");
        assert_eq!(mask_to_carbon(MASK_ALT), 0x0800);
        assert_eq!(mask_to_carbon(MASK_WIN), 0x1000, "the Win role is controlKey");
        assert_eq!(mask_to_carbon(MASK_CTRL | MASK_SHIFT | MASK_ALT | MASK_WIN), 0x1B00);
        assert_eq!(mask_to_carbon(0), 0);
        assert_eq!(mask_to_carbon(crate::backend::MASK_TAP), 0);
    }

    /// The event flags `key_send` sets and the event tap reads, both ways: `"Ctrl+C"` is sent
    /// with Command's flag, and an event with Command's flag is the Ctrl role.
    #[test]
    fn event_flags_become_roles_and_back() {
        assert_eq!(mask_to_cg_flags(MASK_CTRL), CG_FLAG_COMMAND);
        assert_eq!(mask_to_cg_flags(MASK_WIN), CG_FLAG_CONTROL);
        assert_eq!(mask_to_cg_flags(MASK_ALT), CG_FLAG_ALTERNATE);
        assert_eq!(mask_to_cg_flags(MASK_SHIFT), CG_FLAG_SHIFT);
        let (_, copy) = key_spec("Ctrl+C").unwrap();
        assert_eq!(mask_to_cg_flags(copy), CG_FLAG_COMMAND, "Ctrl+C copies on a Mac");
        assert_eq!(mask_of_cg_flags(CG_FLAG_COMMAND), MASK_CTRL);
        assert_eq!(mask_of_cg_flags(CG_FLAG_CONTROL | CG_FLAG_ALTERNATE), MASK_WIN | MASK_ALT);
        for mask in 0..16u8 {
            assert_eq!(mask_of_cg_flags(mask_to_cg_flags(mask)), mask, "mask {mask}");
        }
        // Caps Lock (AlphaShift), the numeric pad and the Fn flag an arrow key carries are no role.
        let noise = 0x0001_0000 | 0x0020_0000 | 0x0080_0000;
        assert_eq!(mask_of_cg_flags(noise), 0);
        assert_eq!(mask_of_cg_flags(noise | CG_FLAG_SHIFT), MASK_SHIFT);
    }

    /// The table a capture callback gets on a Mac, from the flags of the event the tap saw:
    /// `ctrl` is Command held and `win` Control held.
    #[test]
    fn the_capture_table_on_a_mac() {
        use crate::backend::capture_mods_fields;
        let table = |flags| capture_mods_fields(mask_of_cg_flags(flags));
        assert_eq!(
            table(CG_FLAG_COMMAND),
            [("shift", false), ("ctrl", true), ("alt", false), ("win", false)]
        );
        assert_eq!(
            table(CG_FLAG_CONTROL | CG_FLAG_SHIFT),
            [("shift", true), ("ctrl", false), ("alt", false), ("win", true)]
        );
        assert_eq!(
            table(CG_FLAG_ALTERNATE | CG_FLAG_COMMAND),
            [("shift", false), ("ctrl", true), ("alt", true), ("win", false)]
        );
    }

    /// The modifier state `layout.rs` hands `UCKeyTranslate`: Command alone (the Ctrl role) is
    /// the 0x01 the Command letter table is read with, Option the 0x08 a `composes` answer is
    /// asked with, Control (the Win role) 0x10.
    #[test]
    fn the_layout_is_asked_with_carbons_bits_shifted_down() {
        assert_eq!(translate_modifier_state(MASK_CTRL), 0x01);
        assert_eq!(translate_modifier_state(MASK_SHIFT), 0x02);
        assert_eq!(translate_modifier_state(MASK_ALT), 0x08);
        assert_eq!(translate_modifier_state(MASK_ALT | MASK_SHIFT), 0x0A);
        assert_eq!(translate_modifier_state(MASK_WIN), 0x10);
        assert_eq!(translate_modifier_state(0), 0);
        assert_eq!(translate_modifier_state(crate::backend::MASK_TAP), 0);
    }

    /// The reload key the host registers for itself on a Mac, spelled out. If this one stops
    /// translating, the tester loses the only way to reload a module without a mouse.
    ///
    /// `"Cmd+Shift+F5"`, Command-Shift, not the four-modifier chord Windows uses: that one
    /// carries Control-Option (Win+Alt), VoiceOver's modifier, and the third Mac session measured
    /// it as registered and never delivered. The Windows spelling still has to translate — a
    /// module may spell a chord that way — it just must not be the host's own key on a Mac.
    #[test]
    fn the_reload_hotkey_translates() {
        let (vk, mask) = key_spec("Cmd+Shift+F5").unwrap();
        assert_eq!(vk_to_keycode(vk), Some(0x60));
        assert_eq!(mask_to_carbon(mask), 0x0300);
        assert_ne!(
            mask & crate::backend::MAC_VOICEOVER_LAYER,
            crate::backend::MAC_VOICEOVER_LAYER,
            "the Mac reload key must stay off VoiceOver's modifier"
        );
        assert_eq!(key_spec("Ctrl+Shift+F5"), Some((vk, mask)), "Cmd is the Ctrl role");
        let (vk, mask) = key_spec("Ctrl+Shift+Win+Alt+F5").unwrap();
        assert_eq!(vk_to_keycode(vk), Some(0x60));
        assert_eq!(mask_to_carbon(mask), 0x1B00);
    }

    /// `host.keys.check`'s "no-keycode" is decided without this table (it has to answer for a
    /// Mac on any machine), so the two must agree on every key the parser accepts — under the
    /// US letters, the table's own. Under another layout a letter can have no key; that is the
    /// layout's, not the platform's, and `hotkey::register` parks such a hotkey rather than
    /// refusing it, so "no-keycode" stays structural (see the next test).
    #[test]
    fn the_no_keycode_rule_agrees_with_the_table() {
        let mut names: Vec<String> = ('a'..='z').map(String::from).collect();
        names.extend(('0'..='9').map(String::from));
        names.extend((1..=24).map(|n| format!("F{n}")));
        for n in [
            "space", "return", "escape", "tab", "backspace", "delete", "up", "down", "left",
            "right", "home", "end", "pageup", "pagedown",
        ] {
            names.push(n.to_string());
        }
        for name in &names {
            let vk = key_to_vk(name).unwrap();
            assert_eq!(
                crate::backend::has_mac_keycode(vk),
                vk_to_keycode_in(&Letters::US, vk).is_some(),
                "'{name}' (vk {vk:#04X})"
            );
        }
    }

    /// A letter the layout does not type is said to be the layout's doing, and never refused as
    /// a key macOS lacks: `check` and `register` let it through, and the hotkey is parked.
    #[test]
    fn a_letter_without_a_key_is_the_layouts_not_the_platforms() {
        // Q's position types A and no key types Q (as in the next section's missing-letter test).
        let l = pick(Some(layout(&[(0x0C, 'a'), (0x00, '@')])), None);
        let q = key_to_vk("q").unwrap();
        assert_eq!(vk_to_keycode_in(&l, q), None);
        assert!(crate::backend::has_mac_keycode(q), "a letter is never 'no-keycode'");
        assert_eq!(why_no_keycode(q), "no key of the current keyboard layout types this letter");
        let f21 = key_to_vk("F21").unwrap();
        assert!(!crate::backend::has_mac_keycode(f21));
        assert_eq!(why_no_keycode(f21), "macOS has no key code for it");
    }

    // --- Letters by layout -------------------------------------------------------------

    const VK_A: u32 = 0x41;
    const VK_M: u32 = 0x4D;
    const VK_Q: u32 = 0x51;
    const VK_W: u32 = 0x57;
    const VK_Y: u32 = 0x59;
    const VK_Z: u32 = 0x5A;

    /// What a US keyboard types on each candidate key, with no modifier held.
    fn us_types(code: u16) -> Option<char> {
        if let Some(i) = US_LETTERS.iter().position(|c| *c == code) {
            return Some((b'a' + i as u8) as char);
        }
        let digits = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];
        if let Some(d) = digits.iter().position(|c| *c == code) {
            return char::from_digit(d as u32, 10);
        }
        match code {
            0x29 => Some(';'),
            0x18 => Some('='),
            0x2B => Some(','),
            0x1B => Some('-'),
            0x2F => Some('.'),
            0x2C => Some('/'),
            0x32 => Some('`'),
            0x21 => Some('['),
            0x2A => Some('\\'),
            0x1E => Some(']'),
            0x27 => Some('\''),
            0x0A => Some('§'),
            _ => None,
        }
    }

    /// A layout: the US keyboard with some keys retyped.
    fn layout(changes: &'static [(u16, char)]) -> Letters {
        letters_from(move |code| {
            changes.iter().find(|(k, _)| *k == code).map(|(_, c)| *c).or_else(|| us_types(code))
        })
    }

    /// macOS "German": Y and Z swapped, umlauts and ß where US has punctuation.
    const GERMAN: &[(u16, char)] = &[
        (0x10, 'z'),
        (0x06, 'y'),
        (0x29, 'ö'),
        (0x27, 'ä'),
        (0x21, 'ü'),
        (0x1B, 'ß'),
        (0x0A, '^'),
    ];

    /// macOS "French": A and Q swapped, Z and W swapped, M where US has `;`, and the digit row
    /// typing symbols unshifted.
    const FRENCH: &[(u16, char)] = &[
        (0x00, 'q'),
        (0x0C, 'a'),
        (0x0D, 'z'),
        (0x06, 'w'),
        (0x29, 'm'),
        (0x2E, ','),
        (0x2B, ';'),
        (0x2F, ':'),
        (0x2C, '='),
        (0x27, 'ù'),
        (0x12, '&'),
        (0x13, 'é'),
        (0x14, '"'),
        (0x15, '\''),
        (0x17, '('),
        (0x16, '§'),
        (0x1A, 'è'),
        (0x1C, '!'),
        (0x19, 'ç'),
        (0x1D, 'à'),
    ];

    /// Dvorak: nearly every letter somewhere else, four US letter positions typing punctuation.
    const DVORAK: &[(u16, char)] = &[
        (0x0C, '\''),
        (0x0D, ','),
        (0x0E, '.'),
        (0x0F, 'p'),
        (0x11, 'y'),
        (0x10, 'f'),
        (0x20, 'g'),
        (0x22, 'c'),
        (0x1F, 'r'),
        (0x23, 'l'),
        (0x21, '/'),
        (0x1E, '='),
        (0x00, 'a'),
        (0x01, 'o'),
        (0x02, 'e'),
        (0x03, 'u'),
        (0x05, 'i'),
        (0x04, 'd'),
        (0x26, 'h'),
        (0x28, 't'),
        (0x25, 'n'),
        (0x29, 's'),
        (0x27, '-'),
        (0x06, ';'),
        (0x07, 'q'),
        (0x08, 'j'),
        (0x09, 'k'),
        (0x0B, 'x'),
        (0x2D, 'b'),
        (0x2E, 'm'),
        (0x2B, 'w'),
        (0x2F, 'v'),
        (0x2C, 'z'),
    ];

    #[test]
    fn the_fallback_positions_are_the_tables_letters() {
        for (i, code) in US_LETTERS.iter().enumerate() {
            let vk = 0x41 + i as u32;
            let in_table = TABLE.iter().find(|(v, _)| *v == vk).map(|(_, c)| *c);
            assert_eq!(in_table, Some(*code), "letter {}", (b'A' + i as u8) as char);
        }
        for code in CANDIDATES {
            assert!(
                code == 0x0A || TABLE.iter().any(|(_, c)| *c == code),
                "candidate {code:#04X} is not a key the table knows"
            );
        }
        let mut sorted = CANDIDATES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), CANDIDATES.len(), "a key is asked about twice");
    }

    #[test]
    fn the_letters_in_force_start_as_the_us_keyboard() {
        // Nothing in the tests writes them; a Mac writes them once a layout has been read.
        assert_eq!(current_letters(), Letters::US);
        assert_eq!(layout(&[]), Letters::US);
    }

    #[test]
    fn german_letters_follow_the_labels() {
        let de = pick(Some(layout(GERMAN)), None);
        assert_eq!(de.count(), 26);
        // The issue this exists for: Cmd+Z is the key labelled Z, where a US keyboard has Y.
        assert_eq!(vk_to_keycode_in(&de, VK_Z), Some(0x10));
        assert_eq!(vk_to_keycode_in(&de, VK_Y), Some(0x06));
        assert_eq!(keycode_to_vk_in(&de, 0x10), Some(VK_Z));
        assert_eq!(keycode_to_vk_in(&de, 0x06), Some(VK_Y));
        assert_eq!(de.moved(), vec![('Y', Some(0x06)), ('Z', Some(0x10))]);
        // Everything that is not a letter stays where it was.
        assert_eq!(vk_to_keycode_in(&de, 0x31), Some(0x12), "digit 1");
        assert_eq!(keycode_to_vk_in(&de, 0x29), Some(0xBA), "the key where US has ;");
        assert_eq!(vk_to_keycode_in(&de, 0x74), Some(0x60), "F5");
    }

    #[test]
    fn french_letters_follow_the_labels_and_punctuation_stays_put() {
        let fr = pick(Some(layout(FRENCH)), None);
        assert_eq!(fr.count(), 26);
        assert_eq!(vk_to_keycode_in(&fr, VK_A), Some(0x0C));
        assert_eq!(vk_to_keycode_in(&fr, VK_Q), Some(0x00));
        assert_eq!(vk_to_keycode_in(&fr, VK_Z), Some(0x0D));
        assert_eq!(vk_to_keycode_in(&fr, VK_W), Some(0x06));
        assert_eq!(vk_to_keycode_in(&fr, VK_M), Some(0x29));
        // The key where US has ; types m here, so the tap names it M ...
        assert_eq!(keycode_to_vk_in(&fr, 0x29), Some(VK_M));
        // ... and the key where US has M types a comma, which no spec can name.
        assert_eq!(keycode_to_vk_in(&fr, 0x2E), None);
        // Punctuation is positional in the other direction too.
        assert_eq!(vk_to_keycode_in(&fr, 0xBA), Some(0x29));
        // Digits keep their positions although the row types symbols unshifted.
        for (d, code) in [(0x30, 0x1D), (0x31, 0x12), (0x35, 0x17), (0x39, 0x19)] {
            assert_eq!(vk_to_keycode_in(&fr, d), Some(code));
            assert_eq!(keycode_to_vk_in(&fr, code), Some(d));
        }
    }

    #[test]
    fn every_layout_is_a_bijection_on_its_letters() {
        let layouts = [("us", &[][..]), ("german", GERMAN), ("french", FRENCH), ("dvorak", DVORAK)];
        for (name, changes) in layouts {
            let l = pick(Some(layout(changes)), None);
            assert_eq!(l.count(), 26, "{name}");
            let mut codes: Vec<u16> = (b'A'..=b'Z').filter_map(|c| l.code(c)).collect();
            codes.sort_unstable();
            codes.dedup();
            assert_eq!(codes.len(), 26, "{name}: two letters share a key");
            for vk in 0x41..=0x5A {
                let code = vk_to_keycode_in(&l, vk).unwrap();
                assert_eq!(keycode_to_vk_in(&l, code), Some(vk), "{name}: letter {vk:#04X}");
            }
        }
    }

    #[test]
    fn dvorak_positions_that_type_punctuation_name_nothing() {
        let dv = pick(Some(layout(DVORAK)), None);
        for code in [0x0C, 0x0D, 0x0E, 0x06] {
            assert_eq!(keycode_to_vk_in(&dv, code), None, "{code:#04X}");
        }
        assert_eq!(vk_to_keycode_in(&dv, VK_Z), Some(0x2C));
        assert_eq!(keycode_to_vk_in(&dv, 0x29), Some(0x53), "S where US has ;");
    }

    #[test]
    fn every_key_the_shared_parser_accepts_is_mapped_in_every_layout() {
        let mut names: Vec<String> = ('a'..='z').chain('0'..='9').map(String::from).collect();
        names.extend((1..=20).map(|n| format!("F{n}")));
        for (name, changes) in [("german", GERMAN), ("french", FRENCH), ("dvorak", DVORAK)] {
            let l = pick(Some(layout(changes)), None);
            for key in &names {
                let vk = key_to_vk(key).unwrap();
                assert!(vk_to_keycode_in(&l, vk).is_some(), "{name}: '{key}' has no key code");
            }
        }
    }

    #[test]
    fn a_layout_without_latin_letters_gives_way_to_the_ascii_capable_one() {
        // Russian: every letter key types Cyrillic.
        let ru = letters_from(|code| {
            let i = US_LETTERS.iter().position(|c| *c == code)?;
            char::from_u32(0x430 + i as u32)
        });
        assert_eq!(ru.count(), 0);
        let de = layout(GERMAN);
        assert_eq!(pick(Some(ru), Some(de)), de.completed());
        // And with only that one to go on, US positions.
        assert_eq!(pick(Some(ru), None), Letters::US);
        assert_eq!(pick(None, None), Letters::US);
        assert_eq!(pick(None, Some(de)), de.completed());
    }

    #[test]
    fn a_tie_keeps_the_current_layout() {
        let de = layout(GERMAN);
        let fr = layout(FRENCH);
        assert_eq!(pick(Some(de), Some(fr)), de.completed());
    }

    #[test]
    fn a_letter_two_keys_type_goes_to_its_us_position() {
        // A key where US has ; that also types a — the A key keeps it.
        let l = layout(&[(0x29, 'a')]);
        assert_eq!(l.code(b'A'), Some(0x00));
        // Unless its US position types something else: then the first key asked wins.
        let l = layout(&[(0x00, '@'), (0x29, 'a'), (0x27, 'a')]);
        assert_eq!(l.code(b'A'), Some(0x29));
    }

    #[test]
    fn a_missing_letter_falls_back_only_to_a_free_position() {
        // No key types q, and the Q position types a symbol: Q goes back to its position.
        let l = pick(Some(layout(&[(0x0C, '@')])), None);
        assert_eq!(vk_to_keycode_in(&l, VK_Q), Some(0x0C));
        // No key types q, and the Q position types a: Q has no key, rather than two letters
        // sharing one.
        let l = pick(Some(layout(&[(0x0C, 'a'), (0x00, '@')])), None);
        assert_eq!(vk_to_keycode_in(&l, VK_A), Some(0x0C));
        assert_eq!(vk_to_keycode_in(&l, VK_Q), None);
        assert_eq!(keycode_to_vk_in(&l, 0x0C), Some(VK_A));
        assert_eq!(l.moved(), vec![('A', Some(0x0C)), ('Q', None)]);
    }

    /// "Dvorak – QWERTY ⌘": Dvorak while typing, QWERTY while Command is held. A Command
    /// combination — the Ctrl role, `"Ctrl+Z"` or `"Cmd+Z"` — is looked up in the Command
    /// table, so it stays where a QWERTY user's hands and every application's shortcut expect
    /// it; without Command, Dvorak's letters, Control (`"Meta+Z"`) included.
    #[test]
    fn a_layout_that_types_qwerty_with_command_keeps_command_letters_there() {
        let plain = pick(Some(layout(DVORAK)), None);
        let command = pick(Some(layout(&[])), None);
        let (cmd_vk, cmd_mask) = key_spec("Ctrl+Z").unwrap();
        assert_eq!(key_spec("Cmd+Z"), Some((cmd_vk, cmd_mask)));
        let (ctl_vk, ctl_mask) = key_spec("Meta+Z").unwrap();
        let for_cmd = letters_for(&plain, &command, cmd_mask);
        assert_eq!(vk_to_keycode_in(for_cmd, cmd_vk), Some(0x06), "the QWERTY Z key");
        assert_eq!(keycode_to_vk_in(for_cmd, 0x06), Some(VK_Z));
        let for_ctl = letters_for(&plain, &command, ctl_mask);
        assert_eq!(vk_to_keycode_in(for_ctl, ctl_vk), Some(0x2C), "Dvorak's Z key");
        assert_eq!(keycode_to_vk_in(for_ctl, 0x06), None, "types ; in Dvorak");
        // Command with anything else is still Command.
        let (_, all) = key_spec("Ctrl+Alt+Shift+Win+Z").unwrap();
        assert!(std::ptr::eq(letters_for(&plain, &command, all), &command));
        assert!(std::ptr::eq(letters_for(&plain, &command, 0), &plain));
        assert!(std::ptr::eq(letters_for(&plain, &command, MASK_WIN | MASK_ALT), &plain));
        // With nothing read, both tables in force are the US keyboard, whatever is held.
        assert_eq!(vk_to_keycode_for(VK_Z, MASK_CTRL), Some(0x06));
        assert_eq!(vk_to_keycode_for(VK_Z, MASK_WIN), Some(0x06));
        assert_eq!(keycode_to_vk_for(0x06, MASK_CTRL), Some(VK_Z));
        assert_eq!(current_letters_for(MASK_CTRL | MASK_SHIFT), Letters::US);
    }

    #[test]
    fn uppercase_output_counts_as_the_letter() {
        let l = letters_from(|code| us_types(code).map(|c| c.to_ascii_uppercase()));
        assert_eq!(l, Letters::US);
    }
}
