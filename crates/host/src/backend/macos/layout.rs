//! The keyboard layout the user types in, read once and asked two questions.
//!
//! **Which key types a letter.** A macOS key code is a position, and the fixed table in
//! `keys.rs` names US positions, while a letter virtual key on Windows follows the layout's
//! labels. Without this, `"Cmd+Z"` on a German Mac is the key labelled Y —
//! `host.input.send("Cmd+Z")` redoes instead of undoing — and a French keyboard moves A, Q, W,
//! Z and M. So the layout in force is asked, key by key, which letter it types (`UCKeyTranslate`
//! over the layout's 'uchr' data), and the answer goes into `keys.rs`, which Carbon
//! registration, `key_send`, `key_post` and the event tap all translate through. Asked twice:
//! with no modifier held, and with Command held, for the layouts that type other letters then
//! ("Dvorak – QWERTY ⌘" types QWERTY) — a combination that holds Command is looked up in the
//! second table.
//!
//! **What Option with a key types** ([`character`]), for `host.keys.check`'s `composes`: Option
//! composes characters on a Mac, so an overlay that claims Option+E takes the acute dead key
//! away from every text field while it holds it, and on a German layout Option+L is the @. The
//! answer is informational — the key still fires — and it is for the layout selected at the
//! moment of asking.
//!
//! **Which layout.** For the letters, the selected keyboard layout
//! (`TISCopyCurrentKeyboardLayoutInputSource`), unless it types fewer letters than the
//! ASCII-capable one the system offers (`TISCopyCurrentASCIICapableKeyboardLayoutInputSource`)
//! — a Russian or Greek layout types no Latin letters, and shortcuts are then typed on that
//! other one; the choice itself is `keys::pick`, which runs, and is tested, anywhere. For what
//! a key types, the selected layout itself, and the ASCII-capable one only when the selected
//! source has no layout data of its own (an input method whose source carries none).
//!
//! **One read, one cache.** Both questions are answered from the same read ([`refresh`]): the
//! input source whose layout data a key is translated through is kept, retained, in
//! [`TYPING`], and the letter tables are put in force in `keys.rs`. Read when the backend is
//! created, and again after the system posts `kTISNotifySelectedKeyboardInputSourceChanged`;
//! then the hotkeys whose letter moved are registered again on their new key
//! (`hotkey::reregister_letters`). The notification's callback only raises a flag, and the pump
//! takes the change in on its turn ([`take_change`]). `character` reads the layout again at
//! every call rather than answering from the cache: the notification arrives asynchronously,
//! so a question asked right after a switch could otherwise be answered for the layout before
//! it, and without a distributed notification centre it never arrives at all. `check` asks
//! rarely — the overlay runtime asks with `layout = false` — so the read costs nothing that
//! matters. Whichever read moved a letter, the pump is told and re-registers
//! ([`LETTERS_MOVED`]).
//!
//! **Main thread only.** The Text Input Source Services assert that on current macOS, and the
//! callback's thread is not something this code controls. Every entry point checks: off the
//! main thread `start` says so and leaves the letters on their US positions, `pump` leaves the
//! change for the main thread, and `character` answers nothing. A `start` that could not read
//! is made up for by the first `character` on the main thread.
//!
//! The declarations are in `ffi.rs`, with the rest of Carbon: no crate in the objc2 family has
//! them (`objc2-carbon` 0.3 is empty and skips HIToolbox, `objc2-core-services` does not
//! generate CarbonCore). Everything CoreFoundation — an input source's lifetime, the layout's
//! bytes, the observer — goes through `objc2-core-foundation`, like every other macOS file.
//!
//! Unverified on a Mac: TODO.md lists what the next session has to try.

use core::cell::RefCell;
use core::ffi::c_void;
use core::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2::MainThreadMarker;
use objc2_core_foundation::{
    CFData, CFDictionary, CFNotificationCenter, CFNotificationSuspensionBehavior, CFRetained,
    CFString, CFType,
};

use super::ffi::{
    kTISNotifySelectedKeyboardInputSourceChanged, kTISPropertyInputSourceID,
    kTISPropertyUnicodeKeyLayoutData, kUCKeyActionDown, kUCKeyTranslateNoDeadKeysMask, noErr,
    LMGetKbdType, OptionBits, TISCopyCurrentASCIICapableKeyboardLayoutInputSource,
    TISCopyCurrentKeyboardLayoutInputSource, TISGetInputSourceProperty, TISInputSourceRef,
    UCKeyTranslate, UCKeyboardLayout, UniChar, UniCharCount,
};
use super::keys::{self, Letters};

/// `start` has run on the main thread.
static STARTED: AtomicBool = AtomicBool::new(false);

/// The system said the selected input source changed, and nobody on the main thread has read
/// it yet.
static CHANGED: AtomicBool = AtomicBool::new(false);

/// A read moved a letter, and the pump has not re-registered the hotkeys on letters yet. Set by
/// every read ([`read_now`]): the pump's own, `character`'s, and the one at start. That last one
/// moves nothing registered when the backend reads at creation, before any hotkey exists; it
/// does when `start` first ran late, from `character`, after hotkeys were registered on the US
/// positions.
static LETTERS_MOVED: AtomicBool = AtomicBool::new(false);

/// What the observer is registered as. The distributed centre wants a non-null observer to
/// file the registration under; the address of this is ours alone and lives forever.
static OBSERVER: u8 = 0;

thread_local! {
    /// The input source a key is translated through for [`character`]: the selected layout,
    /// or the ASCII-capable one when the selected source has no layout data. Retained, so its
    /// layout data stays valid until the next read replaces it. A thread-local on purpose:
    /// only the main thread ever reads the layout, so only the main thread's copy is ever
    /// filled, and another thread asking finds nothing rather than a source it may not use.
    static TYPING: RefCell<Option<CFRetained<CFType>>> = const { RefCell::new(None) };
}

/// Reads the layout and starts listening for changes to it. Idempotent; main thread only —
/// anywhere else it says so and leaves the letters on their US positions.
pub fn start() {
    if MainThreadMarker::new().is_none() {
        crate::logging::line(
            "macos",
            "keyboard layout: not read, because this is not the main thread; letters stay on \
             their US positions",
        );
        return;
    }
    begin();
}

/// `start` on the main thread: `true` when this call read the layout, `false` when it had been
/// read already. Main thread only (the callers check).
fn begin() -> bool {
    if STARTED.swap(true, Ordering::SeqCst) {
        return false;
    }
    subscribe();
    read_now("at start", true);
    true
}

/// Called by the pump on every turn: `true` when a read since the last turn moved a letter — a
/// change the system announced, taken in here, or a read `character` or a late `start` took —
/// so that the hotkeys on letters are registered again. A check of the thread and two atomic
/// operations when nothing happened.
pub fn pump() -> bool {
    // The pump runs on the main thread; if it ever does not, the change waits for a caller that
    // does.
    if MainThreadMarker::new().is_none() {
        return false;
    }
    take_change();
    LETTERS_MOVED.swap(false, Ordering::Relaxed)
}

/// Reads the layout again if the system said it changed since the last read. Main thread only
/// (the caller checks).
fn take_change() {
    if CHANGED.swap(false, Ordering::Relaxed) {
        read_now("the selected input source changed", false);
    }
}

/// [`refresh`], remembering for the pump when it moved a letter. The one way the layout is
/// read — at start, on the pump's turn and for [`character`] — so that no read that moved a
/// letter leaves the hotkeys on letters behind. Main thread only (the callers check).
fn read_now(why: &str, always_say: bool) {
    if refresh(why, always_say) {
        LETTERS_MOVED.store(true, Ordering::Relaxed);
    }
}

/// What the current layout types for `vk` with `mask` held, and whether it is a dead key —
/// `host.keys.check`'s `composes`, asked only for Option with or without Shift
/// (`backend::layout_question`). `None` when it types nothing, only a control character, or
/// the layout cannot be read, and off the main thread. The key is the one a hotkey on `vk`
/// would be registered on, so on a German layout Option+Z is asked about the key labelled Z.
pub fn character(vk: u32, mask: u8) -> Option<(String, bool)> {
    // The caller is a Lua binding, which runs on the thread that pumps; anything else would
    // break the Text Input Source Services' rule, so it gets no answer.
    MainThreadMarker::new()?;
    // The layout selected now, not the last one a notification announced (see the module
    // comment). A `start` that runs only now has just read it; otherwise it is read here, which
    // takes in a change the pump has not reached yet as well.
    if !begin() {
        CHANGED.store(false, Ordering::Relaxed);
        read_now("read for host.keys.check", false);
    }
    let code = keys::vk_to_keycode_for(vk, mask)?;
    let state = keys::translate_modifier_state(mask);
    TYPING.with(|typing| {
        let typing = typing.borrow();
        let layout = layout_data(typing.as_ref()?)?.byte_ptr() as *const UCKeyboardLayout;
        if layout.is_null() {
            return None;
        }
        // SAFETY: takes nothing, returns a byte.
        let kbd_type = unsafe { LMGetKbdType() } as u32;
        // `layout` points into data the cached source owns, and the source is held by the
        // borrow for the whole of this closure.
        composed(layout, kbd_type, code, state)
    })
}

/// What one input source types: its letters with no modifier held, with Command held, and its
/// id for the log — and the source itself, kept for [`TYPING`].
struct Source {
    plain: Letters,
    command: Letters,
    id: String,
    owned: CFRetained<CFType>,
}

/// Rebuilds the letters from the layouts the system offers now and puts them in force, and
/// keeps the source a key is translated through. `true` when a letter moved. Main thread only
/// (the callers check).
///
/// Said in the log at start, and after that only when a letter moved: it is the one line that
/// explains a hotkey answering to a different key after the user switched layouts — the switch
/// itself is otherwise silent — but a switch that moves nothing (between two layouts with the
/// same letters, or the per-document input source following every application switch) would
/// otherwise write a line each time and explain nothing.
fn refresh(why: &str, always_say: bool) -> bool {
    // SAFETY: both functions take nothing and return a +1 reference or NULL; `read` takes it.
    let current = read(unsafe { TISCopyCurrentKeyboardLayoutInputSource() });
    // Asked only when the selected layout lacks letters — a second copy, translation and
    // release for nothing in the common case. A selected source without layout data counts as
    // lacking all of them, so for that one the ASCII-capable source is what a key is
    // translated through as well.
    let ascii = match &current {
        Some(s) if s.plain.count() == 26 && s.command.count() == 26 => None,
        _ => read(unsafe { TISCopyCurrentASCIICapableKeyboardLayoutInputSource() }),
    };
    let describe = |r: &Option<Source>| match r {
        Some(s) if s.command == s.plain => format!("{} ({} letters)", s.id, s.plain.count()),
        Some(s) => format!(
            "{} ({} letters, {} with Command held)",
            s.id,
            s.plain.count(),
            s.command.count()
        ),
        None => "unreadable".to_string(),
    };
    let sources = match &ascii {
        None => format!("layout {}", describe(&current)),
        Some(_) => format!(
            "layout {}, ASCII-capable layout {}",
            describe(&current),
            describe(&ascii)
        ),
    };
    let letters = keys::pick(
        current.as_ref().map(|s| s.plain),
        ascii.as_ref().map(|s| s.plain),
    );
    let command = keys::pick(
        current.as_ref().map(|s| s.command),
        ascii.as_ref().map(|s| s.command),
    );
    // Replaced whether or not a letter moved: two layouts with the same letters can compose
    // different characters with Option (US and US International).
    let typing = current.as_ref().or(ascii.as_ref()).map(|s| s.owned.clone());
    TYPING.with(|t| *t.borrow_mut() = typing);
    let before = keys::set_letters(letters, command);
    let changed = before != (letters, command);
    if always_say || changed {
        let mut placement = placement_of(&letters);
        if command != letters {
            placement.push_str("; with Command held: ");
            placement.push_str(&placement_of(&command));
        }
        crate::logging::line(
            "macos",
            &format!("keyboard letters {why}: {sources}; {placement}"),
        );
    }
    changed
}

/// The letters that are not on their US position, for the log.
fn placement_of(letters: &Letters) -> String {
    let moved = letters.moved();
    if moved.is_empty() {
        return "every letter is on its US position".to_string();
    }
    moved
        .iter()
        .map(|(c, code)| match code {
            Some(code) => format!("{c}={code:#04x}"),
            None => format!("{c}=no key"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `source` in the form the Text Input Source functions take it.
fn source_ref(source: &CFType) -> TISInputSourceRef {
    source as *const CFType as TISInputSourceRef
}

/// The 'uchr' layout data of `source`, borrowed from it; `None` for a source that has none.
fn layout_data(source: &CFType) -> Option<&CFData> {
    // SAFETY: `source` is a live input source, the key is the framework's own constant, and the
    // property for that key is documented as a `CFDataRef` owned by the source (the Get rule) —
    // which the signature ties the borrow to.
    unsafe {
        (TISGetInputSourceProperty(source_ref(source), kTISPropertyUnicodeKeyLayoutData)
            as *const CFData)
            .as_ref()
    }
}

/// The letters one input source types, with no modifier and with Command held, its id for the
/// log, and the source. Takes the +1 reference over: released when the returned `Source` drops,
/// or at once for NULL and for a source without 'uchr' data, both of which are `None`.
fn read(source: TISInputSourceRef) -> Option<Source> {
    // SAFETY: the caller hands over a Create-rule reference, and `from_raw` takes it, so it is
    // released exactly once.
    let owned: CFRetained<CFType> =
        unsafe { CFRetained::from_raw(NonNull::new(source as *mut CFType)?) };
    // SAFETY: a Get-rule reference, valid while `owned` is, and only read here.
    let id = unsafe {
        let p = TISGetInputSourceProperty(source_ref(&owned), kTISPropertyInputSourceID)
            as *const CFString;
        p.as_ref()
            .map_or_else(|| "(no id)".to_string(), |s| s.to_string())
    };
    let Some(data) = layout_data(&owned) else {
        crate::logging::trace("macos", || {
            format!("keyboard layout {id} has no key layout data")
        });
        return None;
    };
    let layout = data.byte_ptr() as *const UCKeyboardLayout;
    if layout.is_null() {
        return None;
    }
    // SAFETY: takes nothing, returns a byte.
    let kbd_type = unsafe { LMGetKbdType() } as u32;
    // Command is the Ctrl role.
    let command_held = keys::translate_modifier_state(crate::backend::MASK_CTRL);
    let plain = keys::letters_from(|code| letter(layout, kbd_type, code, 0));
    let command = keys::letters_from(|code| letter(layout, kbd_type, code, command_held));
    Some(Source {
        plain,
        command,
        id,
        owned,
    })
}

/// What one `UCKeyTranslate` call typed, and the dead-key state it left behind.
struct Typed {
    units: [UniChar; 8],
    len: usize,
    dead_key_state: u32,
}

impl Typed {
    fn text(&self) -> String {
        String::from_utf16_lossy(&self.units[..self.len])
    }

    /// The character, when exactly one UTF-16 unit was typed.
    fn single(&self) -> Option<char> {
        (self.len == 1)
            .then(|| char::from_u32(self.units[0] as u32))
            .flatten()
    }
}

/// The key `code` under `layout` with the modifiers `modifier_state` held (in `UCKeyTranslate`'s
/// form, [`keys::translate_modifier_state`]). `None` when the call failed. The one call of it:
/// both questions go through here.
fn translate(
    layout: *const UCKeyboardLayout,
    kbd_type: u32,
    code: u16,
    modifier_state: u32,
    options: OptionBits,
) -> Option<Typed> {
    let mut out = Typed {
        units: [0; 8],
        len: 0,
        dead_key_state: 0,
    };
    let mut len: UniCharCount = 0;
    // SAFETY: `layout` points into CFData the caller holds; every out-pointer is a stack slot
    // of the width the header gives, and the buffer's length is passed with it.
    let status = unsafe {
        UCKeyTranslate(
            layout,
            code,
            kUCKeyActionDown,
            modifier_state,
            kbd_type,
            options,
            &mut out.dead_key_state,
            out.units.len() as UniCharCount,
            &mut len,
            out.units.as_mut_ptr(),
        )
    };
    if status != noErr {
        return None;
    }
    out.len = (len as usize).min(out.units.len());
    Some(out)
}

/// The letter question: what the key types, if it is one character. Dead keys off, so a dead
/// key answers with its own character (the German `^` key types `^`) and never waits.
fn letter(
    layout: *const UCKeyboardLayout,
    kbd_type: u32,
    code: u16,
    modifier_state: u32,
) -> Option<char> {
    translate(
        layout,
        kbd_type,
        code,
        modifier_state,
        kUCKeyTranslateNoDeadKeysMask,
    )?
    .single()
}

/// The `composes` question: what the key types, and whether it is a dead key. Asked with dead
/// keys on first; nothing typed and a dead-key state left behind is a dead key, and asked again
/// with dead keys off it names the accent it stands for. Control characters count as nothing.
fn composed(
    layout: *const UCKeyboardLayout,
    kbd_type: u32,
    code: u16,
    modifier_state: u32,
) -> Option<(String, bool)> {
    let first = translate(layout, kbd_type, code, modifier_state, 0)?;
    let answer = if first.len > 0 {
        Some((first.text(), false))
    } else if first.dead_key_state != 0 {
        translate(
            layout,
            kbd_type,
            code,
            modifier_state,
            kUCKeyTranslateNoDeadKeysMask,
        )
        .filter(|t| t.len > 0)
        .map(|t| (t.text(), true))
    } else {
        None
    };
    answer.filter(|(t, _)| !t.chars().all(char::is_control))
}

/// Asks the distributed notification centre for the input-source change, delivered even while
/// this application is in the background — which a tray application almost always is.
fn subscribe() {
    let Some(centre) = CFNotificationCenter::distributed_center() else {
        crate::logging::line(
            "macos",
            "keyboard layout: no distributed notification centre, so a layout switched while \
             running is noticed only when host.keys.check asks the layout, or at the next start",
        );
        return;
    };
    // SAFETY: the name is a constant the framework owns for the life of the process; the
    // observer is the address of a static; the callback matches `CFNotificationCallback`.
    unsafe {
        let name = kTISNotifySelectedKeyboardInputSourceChanged.as_ref();
        centre.add_observer(
            &OBSERVER as *const u8 as *const c_void,
            Some(on_changed),
            name,
            core::ptr::null(),
            CFNotificationSuspensionBehavior::DeliverImmediately,
        );
    }
}

/// The notification's callback: raises the flag and returns. See the module comment for why
/// nothing else happens here.
unsafe extern "C-unwind" fn on_changed(
    _centre: *mut CFNotificationCenter,
    _observer: *mut c_void,
    _name: *const CFString,
    _object: *const c_void,
    _info: *const CFDictionary,
) {
    CHANGED.store(true, Ordering::Relaxed);
}
