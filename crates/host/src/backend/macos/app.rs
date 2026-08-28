//! This process as an application: bringing it to the front, and whether it has a Dock icon.
//!
//! Here rather than in `gui.rs` for a reason that has already cost a session: `macos-check`
//! borrows `backend/mod.rs` by path and **not** `gui.rs`, so anything written in `gui.rs`
//! behind `#[cfg(target_os = "macos")]` is compiled by exactly one machine in the world, the
//! CI Mac, and only after a push. Code in here is type-checked from Windows.

use std::sync::atomic::{AtomicI32, Ordering};

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
    NSRunningApplication, NSWorkspace,
};

/// Whoever was in front before wxWidgets took it away from them.
static LAUNCH_FRONT: AtomicI32 = AtomicI32::new(0);

/// The activation options that mean "come forward now, properly".
///
/// `ActivateIgnoringOtherApps` is deprecated on macOS 14 and later, where it is simply
/// ignored, and is what does the work on 12. Both together behave on each.
fn activation_options() -> NSApplicationActivationOptions {
    #[allow(deprecated)]
    {
        NSApplicationActivationOptions::ActivateAllWindows
            | NSApplicationActivationOptions::ActivateIgnoringOtherApps
    }
}

/// Remembers who is frontmost, to be called BEFORE the wxWidgets loop starts.
///
/// wxWidgets activates this process at launch, and it does so **because** the application is
/// an agent: `applicationDidFinishLaunching:` sees an accessory activation policy and calls
/// `activateWithOptions` itself, with the comment that otherwise it would get no events
/// (wxWidgets src/osx/cocoa/utils.mm:99-131). That runs inside `[NSApp run]`, before our own
/// init closure, so there is no hook early enough to prevent it — only late enough to undo
/// it.
///
/// Undoing it matters more here than the cosmetics suggest. The tester's VoiceOver cursor
/// was dumped into an application with no windows, and VoiceOver said exactly that; he then
/// had to navigate back before he could test anything at all. An accessibility tool that
/// takes the screen reader away from whatever the user was doing, merely by being started,
/// has picked the user's pocket.
pub(crate) fn note_frontmost_before_gui() {
    let me = std::process::id() as i32;
    let front = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
        .unwrap_or(0);
    if front <= 0 || front == me {
        return;
    }
    LAUNCH_FRONT.store(front, Ordering::Relaxed);
    crate::logging::line("macos", &format!("frontmost before we start: pid {front}"));
}

/// Hands the front back, to be called as the FIRST thing inside the GUI init closure.
///
/// Logs who is frontmost at that moment as well as the result, because those two facts
/// together are what say whether the diagnosis above is right: if the machine already names
/// the user's own application here, wxWidgets' activation had not completed and the cause is
/// something else.
pub(crate) fn restore_frontmost_after_gui_start() {
    let pid = LAUNCH_FRONT.swap(0, Ordering::Relaxed);
    if pid <= 0 {
        return;
    }
    let now = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
        .unwrap_or(0);
    let handed = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .is_some_and(|app| app.activateWithOptions(activation_options()));
    crate::logging::line(
        "macos",
        &format!(
            "the front was pid {now} when the window system handed over; giving it back to \
             pid {pid}: {handed}"
        ),
    );
}

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
    NSRunningApplication::currentApplication().activateWithOptions(activation_options());
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
pub(crate) fn set_regular(regular: bool, why: &str) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        crate::logging::line("macos", "activation policy not changed: not on the main thread");
        return false;
    };
    let policy = if regular {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    let named = if regular { "regular (Dock icon, in the app switcher)" } else { "accessory" };
    let app = NSApplication::sharedApplication(mtm);
    // Asked before told, because AppKit answers a request for the policy it is already in
    // with `false`, and a log line reading "REFUSED" for "there was nothing to do" is the
    // kind of diagnostic that sends somebody looking in the wrong place — as it did.
    if app.activationPolicy() == policy {
        crate::logging::line("macos", &format!("activation policy is already {named} ({why})"));
        return true;
    }
    let accepted = app.setActivationPolicy(policy);
    crate::logging::line(
        "macos",
        &format!("activation policy set to {named} for {why} — accepted: {accepted}"),
    );
    accepted
}
