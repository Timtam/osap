//! The declarations macOS does not hand us through a crate.
//!
//! Two things live here, and both are unverifiable from a Windows machine: `cargo check`
//! type-checks an `extern` block but never links it, so every signature below has to be
//! right by construction rather than by experiment. They are transcribed from Apple's
//! headers and cross-checked against the type shims in objc2's own bindings.
//!
//! - **Carbon hotkeys.** `RegisterEventHotKey` is the proven way to hold a global shortcut
//!   on macOS: it needs no permission and cannot be silently switched off the way an event
//!   tap can. No crate in the objc2 family exposes it.
//! - **`_AXUIElementGetWindow`.** The one function that pairs an accessibility element with
//!   the window id capture needs. It is private, it is what every window manager on the
//!   platform uses, and the fallback when it is missing is matching by geometry.
//!
//! Every declaration cites the header it came from, because comparing this file against
//! `/System/Library/Frameworks/Carbon.framework/…/Headers/` is the only review that can
//! catch a mistake here — a wrong scalar width does not fail to compile, it corrupts the
//! call frame at run time on a machine none of us has.
//!
//! The Carbon *modifier* bits are deliberately not here; they are in [`super::keys`] with
//! the rest of the key vocabulary, so that module stays a table of plain integers with no
//! platform dependency and its round-trip tests can run in ordinary CI.

// Apple's identifiers, verbatim, against Rust's naming conventions. Kept as they are spelt
// in the headers on purpose: the only available review of this file is someone reading it
// next to CarbonEvents.h, and a renamed constant cannot be grepped for over there.
#![allow(non_upper_case_globals, non_snake_case)]

use core::ffi::{c_ulong, c_void};

use objc2_application_services::{AXError, AXUIElement};
use objc2_core_graphics::CGWindowID;

// --- Scalars (MacTypes.h) -------------------------------------------------------------

/// `SInt32`. Signed on purpose: Carbon status codes are negative (`eventNotHandledErr` is
/// −9874), and objc2's own MacTypes shim declares `OSStatus` unsigned — ABI-identical, but
/// every comparison and every log line would then read as a huge positive number.
pub type OSStatus = i32;

/// `FourCharCode`: four ASCII bytes packed big-endian. See [`fourcc`].
pub type OSType = u32;

/// `UInt32` of flag bits.
pub type OptionBits = u32;

/// `unsigned long`, and therefore **64 bits** on aarch64 — not `UInt32`, however much the
/// name suggests a count. This is the width that would break silently: `InstallEventHandler`
/// takes one by value, and passing 32 bits leaves the top half of the register as whatever
/// happened to be there, so the handler is installed for a garbage number of event types.
pub type ItemCount = c_ulong;

/// `unsigned long` as well, same reasoning; `GetEventParameter` takes two of them.
pub type ByteCount = c_ulong;

/// `MacTypes.h`. Success.
pub const noErr: OSStatus = 0;

/// `CarbonEventsCore.h`. What a handler returns to say "not mine, pass it on".
pub const eventNotHandledErr: OSStatus = -9874;

// --- Opaque references (CarbonEventsCore.h, CarbonEvents.h) ---------------------------
//
// Each is an opaque struct pointer in C. The zero-sized `_private` field is the usual Rust
// spelling for "you may hold this pointer and never dereference it".

#[repr(C)]
pub struct OpaqueEventRef {
    _private: [u8; 0],
}
#[repr(C)]
pub struct OpaqueEventTargetRef {
    _private: [u8; 0],
}
#[repr(C)]
pub struct OpaqueEventHandlerRef {
    _private: [u8; 0],
}
#[repr(C)]
pub struct OpaqueEventHandlerCallRef {
    _private: [u8; 0],
}
#[repr(C)]
pub struct OpaqueEventHotKeyRef {
    _private: [u8; 0],
}

pub type EventRef = *mut OpaqueEventRef;
pub type EventTargetRef = *mut OpaqueEventTargetRef;
pub type EventHandlerRef = *mut OpaqueEventHandlerRef;
pub type EventHandlerCallRef = *mut OpaqueEventHandlerCallRef;
pub type EventHotKeyRef = *mut OpaqueEventHotKeyRef;

pub type EventParamName = OSType;
pub type EventParamType = OSType;

/// `CarbonEventsCore.h`: `struct EventTypeSpec { OSType eventClass; UInt32 eventKind; }`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct EventTypeSpec {
    pub eventClass: OSType,
    pub eventKind: u32,
}

/// `CarbonEvents.h`: `struct EventHotKeyID { OSType signature; UInt32 id; }`.
///
/// Passed to `RegisterEventHotKey` **by value** — eight bytes, one register pair under
/// AAPCS64. Passing a pointer to it instead compiles perfectly and registers rubbish.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventHotKeyID {
    pub signature: OSType,
    pub id: u32,
}

/// `CarbonEventsCore.h`:
/// `typedef OSStatus (*EventHandlerProcPtr)(EventHandlerCallRef, EventRef, void*)`.
pub type EventHandlerProcPtr =
    unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

/// `typedef EventHandlerProcPtr EventHandlerUPP` — on OS X the UPP is the bare function
/// pointer, with none of the Classic-era glue, so the `Option` here is only C's nullability.
pub type EventHandlerUPP = Option<EventHandlerProcPtr>;

// --- Constants (CarbonEvents.h) --------------------------------------------------------

/// A `FourCharCode` from its four ASCII bytes, most significant first.
pub const fn fourcc(s: &[u8; 4]) -> OSType {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
}

/// `'keyb'` — hotkey events are keyboard-class events.
pub const kEventClassKeyboard: OSType = fourcc(b"keyb");

/// The only kind we register for. `kEventHotKeyReleased` (6) exists and is deliberately
/// left alone: the host's hotkey callback is a press, and asking for releases as well would
/// deliver two events per shortcut.
pub const kEventHotKeyPressed: u32 = 5;

/// `'hkid'` — the parameter type an `EventHotKeyID` is fetched as.
pub const typeEventHotKeyID: EventParamType = fourcc(b"hkid");

/// `'----'` — the event's subject, which for a hotkey event is its `EventHotKeyID`.
///
/// Four hyphens, `0x2D2D2D2D`. It is the same code Apple Events uses for `keyDirectObject`
/// and Carbon reuses it verbatim; `CarbonEvents.h` spells it `kEventParamDirectObject =
/// '----'`. A transcription of `'ee--'` reached this file from the API survey and survived
/// a compile, because a wrong four-character code is still a valid `u32` — the handler
/// would simply never have found the parameter, `GetEventParameter` would have returned an
/// error, and every registered hotkey in the application would have done nothing at all,
/// silently, with the registration itself reporting success.
pub const kEventParamDirectObject: EventParamName = fourcc(b"----");

/// No options, which is what every use of this call has passed since it existed. The
/// header's other value, `kEventHotKeyExclusive` (1 << 0), claims some form of exclusivity
/// over the combination; its exact effect is not documented anywhere readable from here, and
/// a shortcut another application already owns is a case the host handles by telling the
/// user, so there is nothing to be gained by asking for it.
pub const kEventHotKeyNoOptions: OptionBits = 0;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    /// `CarbonEvents.h`. `outRef` receives the reference `UnregisterEventHotKey` needs.
    pub fn RegisterEventHotKey(
        inHotKeyCode: u32,
        inHotKeyModifiers: u32,
        inHotKeyID: EventHotKeyID,
        inTarget: EventTargetRef,
        inOptions: OptionBits,
        outRef: *mut EventHotKeyRef,
    ) -> OSStatus;

    /// `CarbonEvents.h`.
    pub fn UnregisterEventHotKey(inHotKey: EventHotKeyRef) -> OSStatus;

    /// `CarbonEventsCore.h`. The target both the handler and the hotkeys are installed on.
    pub fn GetApplicationEventTarget() -> EventTargetRef;

    /// `CarbonEventsCore.h`. Note this and not `InstallApplicationEventHandler`, which is a
    /// macro over it and has no symbol to link against.
    pub fn InstallEventHandler(
        inTarget: EventTargetRef,
        inHandler: EventHandlerUPP,
        inNumTypes: ItemCount,
        inList: *const EventTypeSpec,
        inUserData: *mut c_void,
        outRef: *mut EventHandlerRef,
    ) -> OSStatus;

    /// `CarbonEventsCore.h`. `outActualType` and `outActualSize` are documented as
    /// nullable, and we pass null for both: we asked for a fixed type of a known size.
    pub fn GetEventParameter(
        inEvent: EventRef,
        inName: EventParamName,
        inDesiredType: EventParamType,
        outActualType: *mut EventParamType,
        inBufferSize: ByteCount,
        outActualSize: *mut ByteCount,
        outData: *mut c_void,
    ) -> OSStatus;
}

// --- The private accessibility-to-window pairing ---------------------------------------

/// `AXError _AXUIElementGetWindow(AXUIElementRef, CGWindowID *out)`.
///
/// There is no header for this one; the signature is the one every window manager on the
/// platform uses.
type AXUIElementGetWindowFn =
    unsafe extern "C" fn(*const AXUIElement, *mut CGWindowID) -> AXError;

/// Resolved once, at the first call. Fn pointers are `Send + Sync`, so unlike the `CFString`
/// constants elsewhere in this backend a plain `OnceLock` is fine here.
static AX_GET_WINDOW: std::sync::OnceLock<Option<AXUIElementGetWindowFn>> =
    std::sync::OnceLock::new();

/// The `CGWindowID` behind an accessibility element, if the private function is there.
///
/// Looked up with `dlsym` rather than declared in the `extern` block above, and that is not
/// a stylistic choice: a private symbol named in an `extern` block is resolved by dyld at
/// **process launch**, so a macOS that has dropped it would give the tester an application
/// that does not start and writes no log at all — the one failure we could not diagnose
/// remotely. This way a missing symbol is a log line and a `None`, and the caller falls back
/// to matching the window by geometry.
///
/// Called once per window that has to be paired, never in a loop over a tree.
// Nothing calls this yet: pairing lives in `ax.rs`, which is being written alongside this
// file. The attribute goes away with the first caller.
#[allow(dead_code)]
pub fn ax_window_id(element: &AXUIElement) -> Option<CGWindowID> {
    let f = (*AX_GET_WINDOW.get_or_init(resolve_ax_get_window))?;
    let mut id: CGWindowID = 0;
    // Safe to call: the element is a live `AXUIElement` for as long as the borrow lasts,
    // and the out-parameter is a stack slot of exactly the width the function writes.
    let err = unsafe { f(element as *const AXUIElement, &mut id) };
    if err != AXError::Success {
        crate::logging::trace("macos", || {
            format!("_AXUIElementGetWindow refused this element: AXError {}", err.0)
        });
        return None;
    }
    // Zero is not a window id; treat it as "did not answer" rather than handing capture a
    // number that will quietly capture nothing.
    if id == 0 {
        crate::logging::trace("macos", || {
            "_AXUIElementGetWindow returned window id 0".to_string()
        });
        return None;
    }
    Some(id)
}

fn resolve_ax_get_window() -> Option<AXUIElementGetWindowFn> {
    // The C name has the leading underscore; the second one that the linker adds is dlsym's
    // business, not ours, so the string is spelt exactly as the function is declared.
    let sym = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"_AXUIElementGetWindow".as_ptr()) };
    if sym.is_null() {
        crate::logging::line(
            "macos",
            "_AXUIElementGetWindow is not present in this macOS; windows will be paired with \
             their accessibility elements by geometry instead, which is slower and can pick \
             the wrong one when two windows overlap exactly",
        );
        return None;
    }
    crate::logging::line("macos", "_AXUIElementGetWindow resolved; window pairing is direct");
    // The cast is the whole point of the exercise: `dlsym` returns a data pointer and we
    // know, from the declaration above, what shape of function is behind it.
    Some(unsafe { std::mem::transmute::<*mut c_void, AXUIElementGetWindowFn>(sym) })
}
