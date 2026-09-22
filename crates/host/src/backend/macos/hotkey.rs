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

/// One registration, and what it was made from.
struct Registration {
    /// What `UnregisterEventHotKey` wants back — or null while the hotkey is **parked**: it
    /// could not follow a layout change to its letter's new key, or no key of the layout typed
    /// its letter when it was registered, so nothing is registered with Carbon for it, and
    /// [`reregister_letters`] tries it again at every later change.
    reference: EventHotKeyRef,
    /// The spec, virtual key and mask it came from, so that a hotkey on a letter can be
    /// registered again on another key when the keyboard layout changes.
    spec: String,
    vk: u32,
    mask: u8,
    /// The key code it is registered on now; for a parked one, the key it was last on, and
    /// `None` for one that has never been held.
    code: Option<u16>,
}

thread_local! {
    /// Host id to its registration. Kept because Carbon gives us no way to ask "what is
    /// registered for id N" — lose the reference and the hotkey stays claimed for the life of
    /// the process.
    static REGISTERED: RefCell<HashMap<i32, Registration>> = RefCell::new(HashMap::new());
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
    // Control and Option, which are the Win and Alt roles.
    const VOICEOVER_PAIR: u8 = crate::backend::MAC_VOICEOVER_LAYER;
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
    let spec = crate::backend::describe_key_for(
        crate::backend::KeyOs::Macos,
        vk,
        mask,
        crate::backend::KeyStyle::Short,
    );
    crate::logging::line(
        "macos",
        &format!(
            "the {how} key '{spec}' sits on Control-Option, which is VoiceOver's modifier, and \
             VoiceOver is running: with the default VoiceOver modifier this chord is taken by \
             VoiceOver before any application sees it, and pressing it plays VoiceOver's \
             error sound. It is accepted here and never arrives. A chord without both Control \
             and Option — Command-Shift-<key>, which is `Ctrl+Shift+<key>` in a spec, is \
             measured to arrive — is the fix; VoiceOver Utility > General > modifier set to \
             Caps Lock is the workaround"
        ),
    );
}

pub fn register(id: i32, spec: &str) -> Result<(), String> {
    // The shared parser, not a second one: what a module writes in its manifest has to mean
    // the same thing on both platforms, and a spec that parses here but not on Windows is a
    // module that works on one machine and mystifies the user on the other.
    let (vk, mask) = crate::backend::parse_key_spec(spec)
        .map_err(|e| format!("could not parse the hotkey spec '{spec}': {e}"))?;

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

    // Said here, at registration, because this is the last place the chord is a chord and
    // not a reference. See `warn_if_voiceover_owns` for why it is worth a line at all.
    warn_if_voiceover_owns(vk, mask, "registered");

    install_handler()?;

    // Re-registering an id replaces what was there, which is the documented behaviour on
    // Windows and something the host relies on. Carbon would otherwise leave the old
    // registration in place and its reference unreachable, so the combination would stay
    // claimed until the process exited.
    unregister(id);

    // A letter no key of the current layout types is parked, not refused: the letters are the
    // layout's, which the user can switch at any moment, so the hotkey is kept and registered
    // by `reregister_letters` as soon as a layout that types the letter is selected — as a
    // hotkey is that could not follow a switch. Refused, it went back to the host as an error,
    // which reached the user as the "Binding unavailable" dialog blaming another application,
    // and nothing tried it again after a switch.
    if (0x41..=0x5A).contains(&vk) && keys::vk_to_keycode_for(vk, mask).is_none() {
        REGISTERED.with(|m| {
            m.borrow_mut().insert(
                id,
                Registration {
                    reference: core::ptr::null_mut(),
                    spec: spec.to_string(),
                    vk,
                    mask,
                    code: None,
                },
            )
        });
        crate::logging::line(
            "macos",
            &format!(
                "hotkey '{spec}' is not held now: {}. It is registered as soon as a layout \
                 that types it is selected",
                keys::why_no_keycode(vk)
            ),
        );
        return Ok(());
    }

    register_on_key(id, spec, vk, mask)
}

/// The registration itself, on the key the letters in force give `vk` — shared by `register`
/// and by [`reregister_letters`], which moves a letter's hotkey when the layout changes.
fn register_on_key(id: i32, spec: &str, vk: u32, mask: u8) -> Result<(), String> {
    // A letter is the key that types it under the current layout (`keys.rs`, `layout.rs`) — with
    // Command held when the combination holds Command — so for a letter the miss means no key
    // of that layout types it.
    let code = keys::vk_to_keycode_for(vk, mask).ok_or_else(|| {
        format!("'{spec}' (virtual key {vk:#04X}): {}", keys::why_no_keycode(vk))
    })?;
    let modifiers = keys::mask_to_carbon(mask);

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

    REGISTERED.with(|m| {
        m.borrow_mut().insert(
            id,
            Registration { reference, spec: spec.to_string(), vk, mask, code: Some(code) },
        )
    });
    crate::logging::trace("macos", || {
        format!("hotkey {id} registered: '{spec}' is key code {code:#04X} + mods {modifiers:#06X}")
    });
    Ok(())
}

/// Moves every hotkey on a letter to the key that types that letter now — called by the pump
/// after `layout.rs` put a changed layout in force. A Carbon hotkey is a key code, so without
/// this a hotkey registered on the German Z would stay on the key where US has Y after a
/// switch to a US layout, and the other way round.
///
/// All of them are released first, then all registered again: two of our own hotkeys can swap
/// keys (Cmd+Y and Cmd+Z between US and German), and registering the first while the second
/// still held its new key would be refused. A hotkey that cannot follow — another application
/// holds the combination on its new key, or no key types the letter now — is said in the log
/// and **parked**: kept here without a Carbon registration, and tried again at every later
/// layout change, so switching back to the old layout brings it back. The host still counts it
/// as held meanwhile, and does not claim it again on its own, so without the parking a hotkey
/// a module registers once would be lost for the session; one its module registers again (an
/// overlay does on its next focus move) is registered afresh then. A hotkey parked by
/// [`register`], because no key typed its letter then, is tried here the same way.
pub fn reregister_letters() {
    let moved: Vec<(i32, String, u32, u8, Option<u16>, bool)> = REGISTERED.with(|m| {
        m.borrow()
            .iter()
            .filter(|(_, r)| {
                r.reference.is_null()
                    || ((0x41..=0x5A).contains(&r.vk)
                        && keys::vk_to_keycode_for(r.vk, r.mask) != r.code)
            })
            .map(|(id, r)| (*id, r.spec.clone(), r.vk, r.mask, r.code, r.reference.is_null()))
            .collect()
    });
    for (id, ..) in &moved {
        unregister(*id);
    }
    for (id, spec, vk, mask, old, was_parked) in moved {
        match register_on_key(id, &spec, vk, mask) {
            Ok(()) => {
                let now = keys::vk_to_keycode_for(vk, mask).unwrap_or(0);
                let how = match (was_parked, old) {
                    (_, None) => format!("held for the first time, on key code {now:#04x}"),
                    (true, Some(old)) => {
                        format!("held again, last on key code {old:#04x}, now on {now:#04x}")
                    }
                    (false, Some(old)) => {
                        format!("moved from key code {old:#04x}, now on {now:#04x}")
                    }
                };
                crate::logging::line(
                    "macos",
                    &format!("hotkey '{spec}' follows the keyboard layout: {how}"),
                )
            }
            Err(e) => {
                REGISTERED.with(|m| {
                    m.borrow_mut().insert(
                        id,
                        Registration {
                            reference: core::ptr::null_mut(),
                            spec: spec.clone(),
                            vk,
                            mask,
                            code: old,
                        },
                    )
                });
                // Said once, when it is parked; a retry that fails again adds only what
                // `register_on_key` says itself about a refusal.
                if !was_parked {
                    crate::logging::line(
                        "macos",
                        &format!(
                            "hotkey '{spec}' could not follow the keyboard layout and is not held \
                             now; it is tried again at every layout change: {e}"
                        ),
                    );
                }
            }
        }
    }
}

pub fn unregister(id: i32) {
    let Some(Registration { reference, .. }) = REGISTERED.with(|m| m.borrow_mut().remove(&id))
    else {
        return;
    };
    if reference.is_null() {
        // Parked: nothing is registered with Carbon to release.
        crate::logging::trace("macos", || format!("hotkey {id} unregistered while parked"));
        return;
    }
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
