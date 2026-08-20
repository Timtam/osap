//! This process as an application: bringing it to the front, and whether it has a Dock icon.
//!
//! Here rather than in `gui.rs` for a reason that has already cost a session: `macos-check`
//! borrows `backend/mod.rs` by path and **not** `gui.rs`, so anything written in `gui.rs`
//! behind `#[cfg(target_os = "macos")]` is compiled by exactly one machine in the world, the
//! CI Mac, and only after a push. Code in here is type-checked from Windows.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
    NSRunningApplication,
};

/// Bring this process to the front.
///
/// Ordering a window in does not do this for an agent application — it comes forward behind
/// whatever is in front, which for somebody who cannot see the screen is the same as not
/// appearing at all.
///
/// **Not `NSApplication::activate()`**, which is macOS 14 and later. On the machine this is
/// written for — 12.7.6 — that selector does not exist, and an unrecognised selector is not
/// an error, it is a crash. `cargo check` cannot see it either: objc2 binds the whole
/// framework with no availability gating, so the compiler agrees with a call that will kill
/// the process. `NSRunningApplication` has answered this since 10.6.
pub(crate) fn activate_self() {
    // `ActivateIgnoringOtherApps` is deprecated on 14+, where it is simply ignored, and is
    // what does the work on 12. Both flags together are the form that behaves on each.
    #[allow(deprecated)]
    let opts = NSApplicationActivationOptions::ActivateAllWindows
        | NSApplicationActivationOptions::ActivateIgnoringOtherApps;
    NSRunningApplication::currentApplication().activateWithOptions(opts);
}

/// Regular means a Dock icon, a menu bar, and an entry in the application switcher.
/// Accessory means none of the three.
///
/// They are one bit, and that is the whole difficulty: **there is no way to be in the
/// application switcher without a Dock icon.** The tester's report — Command-Tab out of the
/// manager window and it is gone — is exactly what `LSUIElement` asks for, so the only thing
/// that answers it literally is becoming a regular application while the window is open.
///
/// Which is why it is a setting rather than a decision. Promoting an accessory application
/// has a reported failure — the menu bar can stay dead until the user switches away and
/// back — that nobody here can reproduce, and shipping a possibly-dead menu bar as the
/// default would trade one unreachable thing for another.
///
/// Returns what AppKit said rather than assuming, and logs it, because this is the kind of
/// call that quietly does nothing on a machine we cannot look at.
pub(crate) fn set_regular(regular: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        crate::logging::line("macos", "activation policy not changed: not on the main thread");
        return false;
    };
    let policy = if regular {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    let accepted = NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
    crate::logging::line(
        "macos",
        &format!(
            "activation policy set to {} — accepted: {accepted}",
            if regular { "regular (Dock icon, in the app switcher)" } else { "accessory" }
        ),
    );
    accepted
}
