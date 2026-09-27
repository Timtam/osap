//! Keeping App Nap away while somebody depends on how fast this application answers.
//!
//! This is an accessory application (`LSUIElement`: no Dock icon, no window most of the time),
//! and macOS naps such an application once it has been in the background for a while: its
//! timers are coalesced and its main thread runs at background priority. The main thread is
//! the one that carries the event tap (`tap.rs`) and the pump — so a napped application would
//! hand captured keys over late, and a tap that answers too late is switched off by the system
//! (the watchdog switches it back on; the keys in between reach the application).
//!
//! So an activity is held — `UserInitiatedAllowingIdleSystemSleep`, the request macOS offers
//! against App Nap — while any of the reasons below holds, with `LatencyCritical` as well for
//! the first two:
//!
//! - **keys are captured** (`tap::set_captured_keys` with a non-empty set). The host has no
//!   notion of an overlay; an overlay that is active is one that has captured its keys, so this
//!   is the "an overlay is active" case;
//! - **a game controller's buttons or axes are listened to** (`gamepad/apple.rs`), which held
//!   an activity of its own before this file existed;
//! - **Screen Recording's own request waits for Accessibility** (`perm::pump`), at a first
//!   setup. The pump looks at Accessibility once a second so that the request follows the grant
//!   within about a second, and the person granting it in System Settings — with this
//!   application's window covered, the case App Nap is for — is waiting for that next dialog.
//!   Released the moment the request is made, from the pump or from the Permissions page's
//!   button; held for the whole session when Accessibility is never granted, which is why this
//!   reason alone is not latency-critical (`activity_reasons.rs` says more).
//!
//! Which activity the reasons call for, and what the log says, is decided in
//! `activity_reasons.rs`, which is pure and tested on Windows; this file begins and ends the
//! activities it decides on.
//!
//! `AllowingIdleSystemSleep`, not plain `UserInitiated`, which would also keep the Mac awake: an
//! overlay being active is no reason to stop the machine sleeping when its owner walks away, and
//! a blind user who leaves the application running for days must not find it has kept the Mac
//! up all night. So `pmset -g assertions` should list nothing of this application's, and
//! Activity Monitor's Energy tab should read "App Nap: No" and "Preventing Sleep: No" while
//! the activity is held.
//!
//! The Info.plist (`package-macos.sh`) deliberately has no `NSAppSleepDisabled`: that would keep
//! App Nap away for the whole session, including the hours in which nothing is captured and the
//! application only waits for a window to come to the front — time a laptop's battery pays for.
//! Whether a registered hotkey alone, a window trigger waiting, or the event tap with nothing
//! captured suffers from a nap is not known; the tap's own "reached the event tap late" line is
//! what would show it (TODO.md, "macOS over days of uptime").
//!
//! Main thread only: every caller is on the main thread — the event tap's captured set, the
//! game controller's listeners and the pump, and the Permissions page's button, which runs in
//! the GUI's click handler — and the token lives in a thread-local.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};

use super::activity_reasons::{change, words, Hold};
pub(crate) use super::activity_reasons::{GAMEPAD, KEYS, SETUP};

thread_local! {
    /// The reasons that hold the activity now.
    static WANTED: Cell<u8> = const { Cell::new(0) };
    /// The activity held, if any; always the one `Hold::for_reasons(WANTED)` calls for.
    static TOKEN: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(None) };
    /// The reasons whose hold has been written to the log in full; later holds for them are
    /// traced, since an overlay captures and releases its keys at every activation.
    static SAID: Cell<u8> = const { Cell::new(0) };
}

/// Sets or clears one reason (`KEYS`, `GAMEPAD`, `SETUP`) and begins, changes or ends the
/// activity to match. Returns whether an activity is held afterwards.
pub(crate) fn want(reason: u8, on: bool) -> bool {
    let c = change(WANTED.with(Cell::get), SAID.with(Cell::get), reason, on);
    WANTED.with(|w| w.set(c.reasons));
    SAID.with(|s| s.set(c.said));
    if c.swap {
        // The new activity is begun before the old one is ended, so that App Nap has no moment
        // in between when one replaces the other.
        let begun = begin(c.hold);
        let old = TOKEN.with(|t| std::mem::replace(&mut *t.borrow_mut(), begun));
        if let Some(token) = old {
            // SAFETY: a token `beginActivity` returned, ended once: it has just left the slot.
            unsafe { NSProcessInfo::processInfo().endActivity(&token) };
        }
    }
    for (line, full) in c.lines {
        if full {
            crate::logging::line("macos", &line);
        } else {
            crate::logging::trace("macos", move || line);
        }
    }
    c.hold != Hold::None
}

/// What the activity is held for now, in words ("nothing" when it is not held): for the game
/// controller's `activity` status, which says why the activity is still held once its own
/// listeners are gone.
pub(crate) fn held_for() -> String {
    words(WANTED.with(Cell::get))
}

/// Begins the activity `hold` calls for; `None` for `Hold::None`.
fn begin(hold: Hold) -> Option<Retained<ProtocolObject<dyn NSObjectProtocol>>> {
    let options = match hold {
        Hold::None => return None,
        Hold::Plain => NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
        Hold::LatencyCritical => {
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep | NSActivityOptions::LatencyCritical
        }
    };
    let why = NSString::from_str(
        "Answering captured keys and game controllers for an accessibility overlay, and the \
         permissions it is being set up with",
    );
    Some(NSProcessInfo::processInfo().beginActivityWithOptions_reason(options, &why))
}
