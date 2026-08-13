//! Synthesising mouse and keyboard input.
//!
//! Nothing here may sleep. A press-and-hold gesture is composed by the caller out of
//! `mouse_down`, a timer, and `mouse_up`, precisely so the waiting happens somewhere that
//! is not the thread carrying speech, hotkeys and detection.
//!
//! Every coordinate below is a global point with the origin at the top-left of the primary
//! display, which is already the space `CGEvent` locations are expressed in. So unlike
//! capture, and unlike anything read out of AppKit, nothing in this file flips an axis or
//! divides by a scale factor; a Retina display changes none of these numbers.
//!
//! Two facts about macOS shape the rest of it. Modifiers travel as *flags on the event*
//! rather than as key presses of their own, so `key_send` sets them instead of pressing
//! them — see there for why that is the safer of the two. And a pointer movement made while
//! a button is held is a different event type from one made with nothing held:
//! `LeftMouseDragged`, not `MouseMoved`. Every move here therefore asks the session which
//! buttons are down before it decides what to post. Getting that second one wrong fails
//! quietly in the worst way — the pointer arrives exactly where it was asked to, and the
//! control that was tracking the drag simply never learns that it moved.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventType, CGMouseButton, CGPreflightPostEventAccess, CGScrollEventUnit,
};

use super::keys;
use crate::backend::MouseButton;

/// The session state every question about "what is held right now" is asked against.
///
/// `HIDSystemState` is the rawer of the two and reports the physical keyboard and mouse;
/// `CombinedSessionState` reports what the user's login session sees, which is the physical
/// state plus anything already synthesised into it. The second is the right one here:
/// callers ask these questions to decide what their *next* synthesised event should look
/// like, and a modifier this process posted a moment ago is every bit as real to the
/// receiving application as one the user is holding. Neither is the per-application cached
/// state that `GetKeyState` was on Windows and that cost a day of debugging there.
const SESSION: CGEventSourceStateID = CGEventSourceStateID::CombinedSessionState;

thread_local! {
    /// One event source for the whole session.
    ///
    /// Every constructor here accepts `None` for the source and still produces a usable
    /// event, so this is an optimisation rather than a requirement — but `mouse_click` is
    /// the hottest input path in the system and a source carries the timing and coalescing
    /// state the window server uses to tell a stream of events apart from a burst of
    /// unrelated ones. `HIDSystemState` is the source that makes a synthesised event look
    /// like it came from the hardware, which is what the plugins this platform drives are
    /// written to react to.
    static SOURCE: RefCell<Option<CFRetained<CGEventSource>>> = RefCell::new(None);
}

/// The shared event source, created on first use.
///
/// Returns a clone rather than lending out of the `RefCell`, because the borrow would
/// otherwise be alive across a `CGEventPost` and a second borrow would panic rather than
/// fail — and a panic in the backend takes the process down with it.
fn source() -> Option<CFRetained<CGEventSource>> {
    SOURCE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
            if slot.is_none() {
                crate::logging::line(
                    "macos",
                    "CGEventSourceCreate(HIDSystemState) returned NULL; continuing with a null \
                     source, which still posts events but loses their shared timing state",
                );
            }
        }
        slot.as_ref().cloned()
    })
}

static POST_ACCESS_LOGGED: AtomicBool = AtomicBool::new(false);

/// Records once, in the log, whether this process may synthesise input at all.
///
/// `CGEventPost` returns nothing — not a status, not an error — so from in here a refused
/// event and a delivered one are the same call. The only thing that can be asked is whether
/// the permission is present, and a tester whose clicks do nothing needs that one sentence
/// in the log more than anything else this file could record. Asked lazily, on the first
/// event actually posted, so the answer in the log is the state at the moment it mattered
/// rather than at startup.
fn note_post_access_once() {
    if POST_ACCESS_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    if CGPreflightPostEventAccess() {
        crate::logging::line("macos", "input: post-event access granted");
    } else {
        crate::logging::line(
            "macos",
            "input: post-event access DENIED — every synthesised click and keystroke from here \
             on will be discarded by the window server without a word. Grant this application \
             Accessibility under System Settings > Privacy & Security and restart it.",
        );
    }
}

/// Posts one event into the session, at the same place hardware enters it.
///
/// `HIDEventTap` rather than `SessionEventTap` so the event passes every tap the system has,
/// including our own — which is exactly the behaviour Windows has with `SendInput`, and
/// exactly the reason `key_post` exists.
fn post(event: &CGEvent) {
    note_post_access_once();
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
}

// Taken by reference throughout, because `MouseButton` is neither `Copy` nor `Clone` and
// every one of these wants to look at the same value more than once.
fn cg_button(button: &MouseButton) -> CGMouseButton {
    match button {
        MouseButton::Left => CGMouseButton::Left,
        MouseButton::Right => CGMouseButton::Right,
        MouseButton::Middle => CGMouseButton::Center,
    }
}

/// The button's name for the log, since the shared enum carries no `Debug`.
fn button_name(button: &MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// The (down, up, dragged) event types for a button. Middle is one of the numbered "other"
/// buttons on this platform rather than a type of its own.
fn button_events(button: &MouseButton) -> (CGEventType, CGEventType, CGEventType) {
    match button {
        MouseButton::Left => (
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGEventType::LeftMouseDragged,
        ),
        MouseButton::Right => (
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGEventType::RightMouseDragged,
        ),
        MouseButton::Middle => (
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
            CGEventType::OtherMouseDragged,
        ),
    }
}

/// Which button the session has down right now, if any.
///
/// Three cheap state reads, and they decide whether a move is a move or a drag. A composed
/// press-and-hold-and-glide — `mouse_down`, timer, `mouse_move`, timer, `mouse_up` — is the
/// gesture the down/up split was created for, and on this platform its middle leg has to be
/// a dragged event or the flyout being glided over never sees it.
fn held_button() -> Option<MouseButton> {
    if CGEventSource::button_state(SESSION, CGMouseButton::Left) {
        Some(MouseButton::Left)
    } else if CGEventSource::button_state(SESSION, CGMouseButton::Right) {
        Some(MouseButton::Right)
    } else if CGEventSource::button_state(SESSION, CGMouseButton::Center) {
        Some(MouseButton::Middle)
    } else {
        None
    }
}

/// Builds and posts one mouse event. `clicks` sets the click-state field, which is what
/// distinguishes a press from the tail of a double-click; a plain click wants 1.
fn post_mouse(etype: CGEventType, x: i32, y: i32, button: CGMouseButton, clicks: Option<i64>) {
    let point = CGPoint::new(x as f64, y as f64);
    let src = source();
    match CGEvent::new_mouse_event(src.as_deref(), etype, point, button) {
        Some(event) => {
            if let Some(n) = clicks {
                CGEvent::set_integer_value_field(
                    Some(&event),
                    CGEventField::MouseEventClickState,
                    n,
                );
            }
            post(&event);
        }
        None => crate::logging::line(
            "macos",
            &format!(
                "CGEventCreateMouseEvent(type {}) returned NULL at {x},{y}; that mouse event was \
                 not sent",
                etype.0
            ),
        ),
    }
}

/// Moves the pointer to `(x, y)` and posts the movement, choosing the drag form of the event
/// when a button is held.
fn glide_to(x: i32, y: i32) {
    match held_button() {
        Some(b) => {
            let (_, _, dragged) = button_events(&b);
            post_mouse(dragged, x, y, cg_button(&b), Some(1));
        }
        None => post_mouse(CGEventType::MouseMoved, x, y, CGMouseButton::Left, None),
    }
}

pub fn cursor_pos() -> (i32, i32) {
    // An event created with no type is Quartz's way of snapshotting the current input state,
    // and its location is the pointer in the global, top-left, point-based space everything
    // else here uses. `NSEvent::mouseLocation` answers the same question but bottom-left and
    // only on the main thread, so it would need flipping against a screen height that this
    // module has no business knowing.
    match CGEvent::new(None) {
        Some(event) => {
            let p = CGEvent::location(Some(&event));
            (p.x.round() as i32, p.y.round() as i32)
        }
        None => {
            crate::logging::line(
                "macos",
                "CGEventCreate returned NULL; reporting the cursor at 0,0, which is a guess",
            );
            (0, 0)
        }
    }
}

pub fn mouse_move(x: i32, y: i32) {
    crate::logging::trace("macos", || format!("mouse_move {x},{y}"));
    glide_to(x, y);
}

pub fn mouse_click(x: i32, y: i32, button: MouseButton) {
    crate::logging::trace("macos", || {
        format!("mouse_click {x},{y} {}", button_name(&button))
    });
    let (down, up, _) = button_events(&button);
    let cg = cg_button(&button);
    // The move first, even though the press carries its own location and would warp the
    // pointer by itself. Custom-drawn plugin UIs — which is nearly everything this platform
    // touches — hit-test on movement and highlight on hover, and a press that arrives at a
    // position they were never told about lands on whatever they still believe is under the
    // pointer. This leg is not asked whether a button is held: a click is by definition not
    // part of a held gesture, and this is the hottest input path in the system.
    post_mouse(CGEventType::MouseMoved, x, y, CGMouseButton::Left, None);
    post_mouse(down, x, y, cg, Some(1));
    post_mouse(up, x, y, cg, Some(1));
}

pub fn mouse_drag(x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton) {
    crate::logging::trace("macos", || {
        format!("mouse_drag {x1},{y1} -> {x2},{y2} {}", button_name(&button))
    });
    let (down, up, dragged) = button_events(&button);
    let cg = cg_button(&button);
    glide_to(x1, y1);
    post_mouse(down, x1, y1, cg, Some(1));
    // No dwell between the press and the move, matching Windows. The single intermediate
    // drag event is what a scrollbar needs; a control that only reveals itself after a
    // sustained press is served by `mouse_down`/`mouse_up` and a timer instead, which is
    // the whole reason those two exist separately.
    post_mouse(dragged, x2, y2, cg, Some(1));
    post_mouse(up, x2, y2, cg, Some(1));
}

pub fn mouse_down(x: i32, y: i32, button: MouseButton) {
    crate::logging::trace("macos", || {
        format!("mouse_down {x},{y} {}", button_name(&button))
    });
    let (down, _, _) = button_events(&button);
    glide_to(x, y);
    post_mouse(down, x, y, cg_button(&button), Some(1));
}

pub fn mouse_up(x: i32, y: i32, button: MouseButton) {
    crate::logging::trace("macos", || {
        format!("mouse_up {x},{y} {}", button_name(&button))
    });
    let (_, up, _) = button_events(&button);
    // Moving before releasing is what makes "press here, glide there, let go" work when the
    // caller composes it out of down, a timer and up: `glide_to` sees our own button still
    // held and posts a drag, so the release lands on a control that has been following the
    // pointer all the way rather than on one that never heard it leave.
    glide_to(x, y);
    post_mouse(up, x, y, cg_button(&button), Some(1));
}

pub fn mouse_scroll(x: i32, y: i32, amount: i32) {
    crate::logging::trace("macos", || format!("mouse_scroll {x},{y} by {amount}"));
    glide_to(x, y);
    let src = source();
    // `amount` is in notches, and here that is the number itself. Windows multiplies by 120
    // because `WHEEL_DELTA` is a raw resolution unit with no name of its own — a mouse
    // reports 120 per detent so that finer wheels can report less. Quartz names the unit
    // instead: asked in `Line` units, one is one line, and one detent of an ordinary wheel
    // is exactly what macOS itself reports as one line. So there is nothing to multiply by,
    // and inventing a factor here would only make a notch mean different things on the two
    // platforms.
    //
    // Sign follows Windows too: positive is a wheel turned away from the user. Whether the
    // user's "natural scrolling" preference then inverts what the content does is a question
    // only a Mac can answer, and it is the first thing to check if scrolling goes the wrong
    // way — the flip is applied to hardware events below the point this one is injected.
    match CGEvent::new_scroll_wheel_event2(src.as_deref(), CGScrollEventUnit::Line, 1, amount, 0, 0)
    {
        Some(event) => {
            // A scroll event has no position of its own until it is given one, and it is
            // delivered to whatever sits under that position rather than under the pointer.
            // Setting it explicitly means the target does not depend on the move posted just
            // above having been processed first.
            CGEvent::set_location(Some(&event), CGPoint::new(x as f64, y as f64));
            post(&event);
        }
        None => crate::logging::line(
            "macos",
            &format!("CGEventCreateScrollWheelEvent2 returned NULL; no scroll sent at {x},{y}"),
        ),
    }
}

static KEY_POST_ROUTE_LOGGED: AtomicBool = AtomicBool::new(false);

/// Deliver a key to one window without it entering the input queue. A Win32 idea; see the
/// trait for why it exists and what the honest macOS answer is.
///
/// The Win32 version posts `WM_KEYDOWN`/`WM_KEYUP` to a window, which no other part of the
/// system observes. macOS has no per-window message queue to post into, so the nearest
/// equivalent is one step coarser: `CGEventPostToPid` hands the event to a *process*, and
/// it is delivered without travelling past the session-wide taps — which is the property
/// that matters, because the alternative is that our own key capture sees the key we just
/// sent and fires the handler that sent it, forever.
///
/// Coarser is good enough for the caller that exists. Melodyne is a single-window
/// application being stepped through its sub-tools by repeated function-key presses, and the
/// window addressed is its own front window. Where it is not good enough, the precedent the
/// project already set is to fall back to a click rather than to invent a mechanism: the
/// `WM_COMMAND` menu driver was deleted for exactly that reason. `AXUIElementPostKeyboardEvent`
/// would be the per-element route and is not one — it is deprecated, undocumented as to which
/// element it actually targets, and reported not to work at all on recent systems.
pub fn key_post(hwnd: isize, key: &str) -> Result<(), String> {
    let vk = crate::backend::key_to_vk(key).ok_or_else(|| format!("unknown key '{key}'"))?;
    let code = keys::vk_to_keycode(vk)
        .ok_or_else(|| format!("key '{key}' (VK {vk:#04x}) has no macOS keycode"))?;
    let entry = super::handles::get(hwnd).ok_or_else(|| {
        format!("key_post: {hwnd} is not a handle this backend issued, so there is no process to post '{key}' to")
    })?;
    if !KEY_POST_ROUTE_LOGGED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "key_post: routing keys with CGEventPostToPid (first call: '{key}' to pid {}). \
                 This is the macOS stand-in for PostMessage and is delivered per process, not \
                 per window. If a posted key ever re-triggers the overlay's own hotkey, or \
                 lands in a different window of the same application, this is the line to \
                 suspect.",
                entry.pid
            ),
        );
    }
    let src = source();
    for is_down in [true, false] {
        match CGEvent::new_keyboard_event(src.as_deref(), code, is_down) {
            Some(event) => {
                // Cleared rather than inherited. A source built on the hardware state hands
                // the new event whatever modifiers are physically held, and `key_post` is
                // called from inside a hotkey callback — that is, while the combination that
                // triggered it is still down. Melodyne wants a bare F-key, not Alt+F-key.
                CGEvent::set_flags(Some(&event), CGEventFlags::empty());
                note_post_access_once();
                CGEvent::post_to_pid(entry.pid as libc::pid_t, Some(&event));
            }
            None => {
                return Err(format!(
                    "CGEventCreateKeyboardEvent returned NULL for '{key}' (keycode {code})"
                ))
            }
        }
    }
    crate::logging::trace("macos", || {
        format!("key_post '{key}' (keycode {code}) to pid {}", entry.pid)
    });
    Ok(())
}

pub fn key_send(combo: &str) -> Result<(), String> {
    let parts: Vec<&str> = combo
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let (key_part, mod_parts) = parts
        .split_last()
        .ok_or_else(|| "empty key combo".to_string())?;

    // The modifiers become flags on the key event rather than key events of their own, and
    // that is a deliberate divergence from the Windows implementation's press-key-release
    // ordering. Three reasons, in order of weight.
    //
    // It cannot leave a modifier stuck. A synthesised Command press that never gets its
    // release — because the release was dropped, or the process died between the two — locks
    // the whole session into a state a blind user cannot see, cannot diagnose and cannot
    // clear without sighted help. A flag lives on one event and is gone when the event is.
    //
    // It is what Quartz documents. A modifier key posted as an ordinary key event does not
    // reliably become a modifier for the events that follow it; applications read the flags
    // off each event, so the flags have to be set even when the modifier key was pressed as
    // well. Setting them and skipping the presses drops the half that was never load-bearing.
    //
    // And it isolates the send from what the user is holding. `CGEventSetFlags` replaces the
    // whole set, so the Alt that is still down from the hotkey which triggered this call does
    // not turn a sent Escape into Alt+Escape. That is strictly better than the Windows
    // behaviour, which does inherit it.
    //
    // The cost, if a tester finds an application that ignores flag-only modifiers: the fix is
    // to bracket the pair below with `FlagsChanged` events carrying the accumulating and then
    // receding flag set. Nothing else in this file would change.
    let mut flags = CGEventFlags::empty();
    for m in mod_parts {
        flags |= match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => CGEventFlags::MaskControl,
            "alt" | "option" => CGEventFlags::MaskAlternate,
            "shift" => CGEventFlags::MaskShift,
            // By position, not by name: what a module wrote as the Windows key is the key in
            // the same place on this keyboard, which is Command.
            "win" | "super" | "cmd" | "command" | "meta" => CGEventFlags::MaskCommand,
            other => return Err(format!("unknown modifier '{other}'")),
        };
    }
    let vk =
        crate::backend::key_to_vk(key_part).ok_or_else(|| format!("unknown key '{key_part}'"))?;
    let code = keys::vk_to_keycode(vk)
        .ok_or_else(|| format!("key '{key_part}' (VK {vk:#04x}) has no macOS keycode"))?;

    let src = source();
    for is_down in [true, false] {
        match CGEvent::new_keyboard_event(src.as_deref(), code, is_down) {
            Some(event) => {
                // On the release as well as the press. An application that tracks modifiers
                // by watching the flags on every event it receives reads a release whose
                // flags disagree with its press as a modifier that was let go early, and some
                // of them then behave as though it were still held.
                CGEvent::set_flags(Some(&event), flags);
                post(&event);
            }
            None => {
                return Err(format!(
                    "CGEventCreateKeyboardEvent returned NULL for '{combo}' (keycode {code})"
                ))
            }
        }
    }
    crate::logging::trace("macos", || {
        format!("key_send '{combo}' -> keycode {code} flags {:#x}", flags.0)
    });
    Ok(())
}

pub fn type_text(text: &str) {
    // The keyboard layout is bypassed entirely, exactly as `KEYEVENTF_UNICODE` does on
    // Windows: the events carry keycode 0 and a Unicode payload that overrides it, so what
    // arrives is the character asked for rather than whatever the user's current layout puts
    // on some key. A German layout, a Dvorak layout and a dead key in the middle of the
    // string all stop mattering.
    let src = source();
    let mut sent = 0usize;
    for ch in text.chars() {
        // One event pair per CHARACTER, not per UTF-16 code unit. Windows sends a unit at a
        // time and gets away with it; here a lone surrogate half in a payload of its own is
        // not text, so anything outside the basic plane would arrive as two pieces of
        // nothing. Encoding per character keeps a surrogate pair together in one payload.
        let mut buf = [0u16; 2];
        let units: &[u16] = ch.encode_utf16(&mut buf);
        for is_down in [true, false] {
            match CGEvent::new_keyboard_event(src.as_deref(), 0, is_down) {
                Some(event) => {
                    // Cleared, for the same reason as in `key_post`: a Command still held
                    // from the hotkey that started this would turn every typed character into
                    // a menu shortcut.
                    CGEvent::set_flags(Some(&event), CGEventFlags::empty());
                    // SAFETY: `units` is a slice of one or two initialised `u16`s that
                    // outlives the call, and its length is passed as the count. `UniCharCount`
                    // is not exported by the bindings, so its underlying `c_ulong` is written
                    // out here.
                    unsafe {
                        CGEvent::keyboard_set_unicode_string(
                            Some(&event),
                            units.len() as core::ffi::c_ulong,
                            units.as_ptr(),
                        );
                    }
                    post(&event);
                }
                None => {
                    crate::logging::line(
                        "macos",
                        &format!(
                            "CGEventCreateKeyboardEvent returned NULL while typing; {sent} of {} \
                             characters were sent",
                            text.chars().count()
                        ),
                    );
                    return;
                }
            }
        }
        sent += 1;
    }
    crate::logging::trace("macos", || format!("type_text sent {sent} character(s)"));
}

/// Are any of Command / Option / Control / Shift held right now? Read from the hardware
/// state, not from a per-application cache.
///
/// The caller is a hotkey handler deciding whether to defer: it is running while its own
/// combination is still down, and anything it synthesises next would arrive wearing those
/// modifiers. Caps Lock is excluded deliberately — it is a latch rather than something being
/// held, it is in these flags as `MaskAlphaShift`, and Windows does not count it either.
pub fn modifiers_down() -> bool {
    let flags = CGEventSource::flags_state(SESSION);
    flags.intersects(
        CGEventFlags::MaskCommand
            | CGEventFlags::MaskAlternate
            | CGEventFlags::MaskControl
            | CGEventFlags::MaskShift,
    )
}
