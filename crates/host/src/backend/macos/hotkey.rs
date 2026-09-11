//! Global shortcuts, through Carbon.
//!
//! Deliberately not an event tap. A registered hotkey needs no Input Monitoring permission
//! and cannot be silently disabled, which matters because these arrive in bursts — every
//! visible control of an overlay claims one, so a single window switch registers and
//! unregisters tens of them.
//!
//! The shape is one handler for the whole process and N hotkeys registered against it.
//! Carbon will happily install a handler per hotkey, but every one of them then fires for
//! every hotkey and each has to demultiplex anyway, so this way is both cheaper and the
//! documented arrangement.
//!
//! The handler itself does exactly one thing — push the id onto [`super::queue`] — for the
//! same reason the event tap does: it runs on the thread that carries speech, detection and
//! the pump, and anything it does is time the next keystroke waits for.

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::ffi::{
    eventNotHandledErr, fourcc, kEventClassKeyboard, kEventHotKeyNoOptions, kEventHotKeyPressed,
    kEventParamDirectObject, noErr, typeEventHotKeyID, ByteCount, EventHandlerCallRef,
    EventHandlerRef, EventHotKeyID, EventHotKeyRef, EventRef, EventTypeSpec,
    GetApplicationEventTarget, GetEventParameter, InstallEventHandler, OSStatus, OSType,
    RegisterEventHotKey, UnregisterEventHotKey,
};
use super::{keys, queue};

/// Our tag on every hotkey we own. The OS does not validate it — it is a caller-chosen
/// four-character code — but the handler is installed on the *application* event target,
/// which anything linked into this process can also register against, so checking it is
/// what keeps somebody else's hotkey from arriving as one of our ids.
const SIGNATURE: OSType = fourcc(b"aphk");

thread_local! {
    /// Host id to the reference `UnregisterEventHotKey` wants back. Kept because Carbon
    /// gives us no way to ask "what is registered for id N" — lose the reference and the
    /// hotkey stays claimed for the life of the process.
    static REGISTERED: RefCell<HashMap<i32, EventHotKeyRef>> = RefCell::new(HashMap::new());
    static HANDLER_INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Chords already explained as VoiceOver's, by (vk, mask). Per-control hotkeys are
    /// released and registered again on every focus move and tab switch, so a per-call line
    /// would print tens of times per window switch for one chord.
    static VOICEOVER_WARNED: RefCell<std::collections::HashSet<(u32, u8)>> =
        RefCell::new(std::collections::HashSet::new());
}

/// Says once, per chord, when a key sits on Control-Option while VoiceOver is running.
///
/// A chord VoiceOver owns leaves no trace anywhere else: Carbon accepts it, the tap never
/// sees it, no press ever arrives, and the only symptom is a sound the log cannot hear.
/// Control-Option is VoiceOver's modifier on any Mac that keeps the default setting (Caps
/// Lock is the alternative, and some long-time users choose it — which is why the same
/// chord worked for two sessions on one tester's Mac and never once on another). Not an
/// error: the registration is legal, the setting is the user's, and a chord that reaches
/// nobody is still better explained than refused. `how` names the mechanism — registered
/// through Carbon, or captured by the event tap — because the two are explained at
/// different sites and a reader should not have to guess which one produced the line.
pub(super) fn warn_if_voiceover_owns(vk: u32, mask: u8, how: &str) {
    const VOICEOVER_PAIR: u8 = crate::backend::MASK_CTRL | crate::backend::MASK_ALT;
    if mask & VOICEOVER_PAIR != VOICEOVER_PAIR {
        return;
    }
    if VOICEOVER_WARNED.with(|w| w.borrow().contains(&(vk, mask))) {
        return;
    }
    // Asked after the cheap checks and remembered only when it answered yes, so a chord
    // registered before VoiceOver starts is still explained once it is running.
    if !super::perm::voiceover_running() {
        return;
    }
    VOICEOVER_WARNED.with(|w| w.borrow_mut().insert((vk, mask)));
    let key = crate::backend::vk_name(vk).unwrap_or_else(|| format!("vk {vk:#04x}"));
    let mut spec = String::new();
    for (bit, name) in [
        (crate::backend::MASK_CTRL, "Ctrl"),
        (crate::backend::MASK_ALT, "Alt"),
        (crate::backend::MASK_SHIFT, "Shift"),
        (crate::backend::MASK_WIN, "Cmd"),
    ] {
        if mask & bit != 0 {
            spec.push_str(name);
            spec.push('+');
        }
    }
    spec.push_str(&key);
    crate::logging::line(
        "macos",
        &format!(
            "the {how} key '{spec}' sits on Control-Option, which is VoiceOver's modifier, and \
             VoiceOver is running: with the default VoiceOver modifier this chord is taken by \
             VoiceOver before any application sees it, and pressing it plays VoiceOver's \
             error sound. It is accepted here and never arrives. A chord without both Control \
             and Option — Command-Shift-<key> is measured to arrive — is the fix; VoiceOver \
             Utility > General > modifier set to Caps Lock is the workaround"
        ),
    );
}

pub fn register(id: i32, spec: &str) -> Result<(), String> {
    // The shared parser, not a second one: what a module writes in its manifest has to mean
    // the same thing on both platforms, and a spec that parses here but not on Windows is a
    // module that works on one machine and mystifies the user on the other.
    let (vk, mask) = crate::backend::key_spec(spec)
        .ok_or_else(|| format!("could not parse the hotkey spec '{spec}'"))?;

    // A bare modifier tap is not something any OS can register: there is no key to hang it
    // on, and the whole point of it is that the modifier keeps working as a modifier. The
    // event tap recognises those. Refusing here rather than registering something plausible
    // is the difference between a message the user can act on and a shortcut that fires on
    // the wrong key.
    if mask & crate::backend::MASK_TAP != 0 {
        return Err(format!(
            "'{spec}' is a modifier tap, which cannot be a global hotkey — capture it with \
             host.keys instead"
        ));
    }

    let code = keys::vk_to_keycode(vk).ok_or_else(|| {
        format!("'{spec}' resolves to virtual key {vk:#04X}, which macOS has no key code for")
    })?;
    let modifiers = keys::mask_to_carbon(mask);

    // Said here, at registration, because this is the last place the chord is a chord and
    // not a reference. See `warn_if_voiceover_owns` for why it is worth a line at all.
    warn_if_voiceover_owns(vk, mask, "registered");

    install_handler()?;

    // Re-registering an id replaces what was there, which is the documented behaviour on
    // Windows and something the host relies on. Carbon would otherwise leave the old
    // registration in place and its reference unreachable, so the combination would stay
    // claimed until the process exited.
    unregister(id);

    let hot_key_id = EventHotKeyID { signature: SIGNATURE, id: id as u32 };
    let mut reference: EventHotKeyRef = core::ptr::null_mut();
    // Auto-repeat: there is no `MOD_NOREPEAT` to pass, and none is needed. Carbon sends
    // kEventHotKeyPressed once when the combination goes down and kEventHotKeyReleased when
    // it comes up; the key repeats the system generates go to whatever has focus and never
    // become a second hotkey event. We register for the press only, so a held-down shortcut
    // fires exactly once. (Unverified on hardware — worth one deliberate press-and-hold in
    // the first session on a Mac.)
    let status = unsafe {
        RegisterEventHotKey(
            code as u32,
            modifiers,
            hot_key_id,
            GetApplicationEventTarget(),
            kEventHotKeyNoOptions,
            &mut reference,
        )
    };
    if status != noErr || reference.is_null() {
        // The host turns this into a dialog the user reads, so it has to say what happened
        // and what it means, not just fail.
        let msg = format!(
            "macOS refused the shortcut '{spec}' (RegisterEventHotKey returned {status}); \
             another application probably already uses it system-wide"
        );
        crate::logging::line("macos", &msg);
        return Err(msg);
    }

    REGISTERED.with(|m| m.borrow_mut().insert(id, reference));
    crate::logging::trace("macos", || {
        format!("hotkey {id} registered: '{spec}' is key code {code:#04X} + mods {modifiers:#06X}")
    });
    Ok(())
}

pub fn unregister(id: i32) {
    let Some(reference) = REGISTERED.with(|m| m.borrow_mut().remove(&id)) else {
        return;
    };
    let status = unsafe { UnregisterEventHotKey(reference) };
    if status != noErr {
        // Not fatal and not worth failing a caller over — the host has no error path here —
        // but it does mean the combination is still claimed, which is exactly the shape of
        // "the shortcut stopped working after switching windows a few times".
        crate::logging::line(
            "macos",
            &format!("UnregisterEventHotKey returned {status} for hotkey {id}"),
        );
        return;
    }
    crate::logging::trace("macos", || format!("hotkey {id} unregistered"));
}

/// Installs the one process-wide handler, on first use.
///
/// Deliberately not done in `MacBackend::new`: registration and delivery both have to
/// happen on the thread that runs the pump, and doing it here means the handler is
/// installed by the same call that first needs it, on whichever thread that is.
fn install_handler() -> Result<(), String> {
    if HANDLER_INSTALLED.with(Cell::get) {
        return Ok(());
    }

    let spec = EventTypeSpec { eventClass: kEventClassKeyboard, eventKind: kEventHotKeyPressed };
    let mut handler: EventHandlerRef = core::ptr::null_mut();
    // No user data: the handler needs nothing but the id it reads out of the event, and a
    // pointer into Rust state that outlives nothing in particular is how a handler that
    // fires during teardown turns into a crash.
    let status = unsafe {
        InstallEventHandler(
            GetApplicationEventTarget(),
            Some(hotkey_handler),
            1,
            &spec,
            core::ptr::null_mut(),
            &mut handler,
        )
    };
    if status != noErr {
        let msg = format!(
            "could not install the Carbon hotkey handler (InstallEventHandler returned \
             {status}); no global shortcut will work in this session"
        );
        crate::logging::line("macos", &msg);
        return Err(msg);
    }

    // The handler is never removed. It costs nothing while nothing is registered, and the
    // alternative — tearing it down when the last hotkey goes away — would reinstall it
    // dozens of times per window switch, which is the one call shape this module has to
    // stay cheap under.
    HANDLER_INSTALLED.with(|f| f.set(true));
    crate::logging::line("macos", "Carbon hotkey handler installed");
    Ok(())
}

/// Runs on the main thread, from the Carbon event dispatcher.
///
/// Note what it does not do: no lookup of what the id means, no dispatch into Lua, no
/// speech. Returning `noErr` consumes the event, which is right — the combination was
/// registered by us and nothing else should see it.
///
/// The queue it pushes onto is thread-local, and this assumes the dispatcher calls back on
/// the thread that registered — which is what the Carbon documentation says and what the
/// main run loop does. If a shortcut ever turns out to fire and do nothing, that assumption
/// is the first thing to suspect: the id would be sitting in a queue nobody drains.
unsafe extern "C" fn hotkey_handler(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user_data: *mut c_void,
) -> OSStatus {
    let mut hot_key_id = EventHotKeyID::default();
    let status = GetEventParameter(
        event,
        kEventParamDirectObject,
        typeEventHotKeyID,
        core::ptr::null_mut(),
        core::mem::size_of::<EventHotKeyID>() as ByteCount,
        core::ptr::null_mut(),
        &mut hot_key_id as *mut EventHotKeyID as *mut c_void,
    );
    if status != noErr {
        return eventNotHandledErr;
    }
    if hot_key_id.signature != SIGNATURE {
        // Somebody else's hotkey on the same target. Pass it on rather than swallowing it.
        return eventNotHandledErr;
    }
    // The id went out as a `u32` and comes back as one; host ids are small and positive
    // (allocated by a counter that starts at 1), so the round trip is exact.
    queue::push_hotkey(hot_key_id.id as i32);
    noErr
}
