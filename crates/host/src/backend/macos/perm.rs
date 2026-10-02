//! Permissions, and saying out loud which ones are missing.
//!
//! Three switches decide whether this application can do anything at all, none of them
//! grantable programmatically, and two of them fail in ways that look like a bug rather
//! than a permission: without Accessibility every element read comes back empty, and
//! without Screen Recording a capture does not fail — it returns a picture of the
//! wallpaper. For a blind tester on a machine nobody here owns, a silent wrong answer is
//! the worst possible outcome, so everything here ends up in the log by name.
//!
//! The startup block is written for one reader: someone on a Mac we cannot see, who will
//! send us a log file and nothing else. Every value is therefore one short greppable line
//! rather than a sentence, and every state that is *wrong* is followed by a `… fix` line
//! saying which pane to open and whether the application has to be restarted afterwards
//! (Screen Recording yes, Accessibility no — both measured).

use std::cell::Cell;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};

use objc2_app_kit::{NSRunningApplication, NSScreen, NSWorkspace};
use objc2_core_services::{
    errAEEventNotPermitted, errAEEventWouldRequireUserConsent, typeWildCard,
    AEDeterminePermissionToAutomateTarget,
};
use objc2_foundation::NSAppleEventDescriptor;
use objc2_application_services::{
    kAXTrustedCheckOptionPrompt, AXIsProcessTrusted, AXIsProcessTrustedWithOptions,
};
use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGError,
    CGGetActiveDisplayList, CGMainDisplayID, CGPreflightScreenCaptureAccess,
    CGRequestListenEventAccess, CGRequestScreenCaptureAccess,
};
use objc2_foundation::{MainThreadMarker, NSProcessInfo, NSString};
use objc2_io_kit::{IOHIDAccessType, IOHIDCheckAccess, IOHIDRequestAccess, IOHIDRequestType};

use super::enrol::{Listen, ListenStep, Rung, ScreenStep};

/// The one fact about macOS permissions that costs everybody an hour the first time, and
/// which a blind tester has no way of discovering: the switch takes effect for the *next*
/// process, not this one. Repeated verbatim next to every permission that is missing,
/// because a tester greps for the permission name, not for a note further up the file.
///
/// Screen Recording only, now. It was attached to all three, and the fifth session measured
/// Accessibility otherwise: observation began in the running process within seconds of the
/// switch (focus notifications from System Settings, then Finder), no restart involved —
/// while Screen Recording, still "not granted" after the grant, read "granted" only in the
/// next launch. Telling the tester to quit after every grant cost him a launch each time.
const RESTART_NOTE: &str =
    "then quit this application completely and open it again — macOS only hands this \
     permission to a process that started after the grant";

/// Accessibility, measured on macOS 14.5: it reaches the running process.
const ACCESSIBILITY_NOTE: &str =
    "it takes effect in the running application within a few seconds — no need to quit; \
     'Re-check now' on the Permissions page confirms it";

/// Input Monitoring has not been measured either way — whether a grant reaches the running
/// process, or whether this application needs it at all next to Accessibility (TODO.md) — so
/// the restart is the fallback, not the rule.
const INPUT_MONITORING_NOTE: &str =
    "and if it still reads as missing after 'Re-check now', quit this application completely \
     and open it again";

/// The settings panes, by the anchors System Settings (and System Preferences before it) opens
/// them at — what the Permissions page's buttons open once nothing is left to ask.
const ACCESSIBILITY_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
const SCREEN_RECORDING_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
const INPUT_MONITORING_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent";
const AUTOMATION_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation";

/// How far this process's Screen Recording requests have got — an `enrol::Rung`, as a number.
/// Climbed by the automatic request and by the Permissions page's button, both on the main
/// thread; atomic so that nothing has to prove it.
static SCREEN_RECORDING_RUNG: AtomicU8 = AtomicU8::new(0);
/// The same for Input Monitoring, which only its button climbs.
static INPUT_MONITORING_RUNG: AtomicU8 = AtomicU8::new(0);
/// Screen Recording is to be asked for automatically, on the first tick at which Accessibility
/// is granted — see [`pump`].
static SCREEN_RECORDING_WAITING: AtomicBool = AtomicBool::new(false);
/// Accessibility was granted already when the application started, so the automatic request is
/// the one "at start" in the log rather than "now that Accessibility is granted".
static TRUSTED_AT_START: AtomicBool = AtomicBool::new(false);

/// How often [`pump`] looks at Accessibility while Screen Recording waits for it. A look is one
/// `AXIsProcessTrusted`, and a second is well inside the time it takes to switch a setting on.
const ACCESSIBILITY_LOOK_EVERY: Duration = Duration::from_secs(1);

/// The pause before a capture request that follows another request made in the same breath —
/// on macOS 12, where the first request makes all of them (`enrol::screen_step`), and before the
/// older functions wherever they follow ScreenCaptureKit: long enough for a dialog the request
/// before raised to be on screen, with its entry in the list, before the next arrives. Whether
/// macOS shows two anyway is unmeasured (TODO.md). Nothing waits on it but the request's own
/// thread.
const BETWEEN_REQUESTS: Duration = Duration::from_secs(1);

/// How long the request's thread waits for ScreenCaptureKit's list of what can be captured. It
/// answers in milliseconds as a rule; the bound is for a system where it does not.
const SHAREABLE_CONTENT_WAIT: Duration = Duration::from_secs(5);

thread_local! {
    // When `pump` last looked at Accessibility. Main thread only, like `pump`.
    static LAST_ACCESSIBILITY_LOOK: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// What started a Screen Recording request, for the log and for how far it may go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Trigger {
    /// The application's start, with Accessibility already granted — asked on the first tick,
    /// once the application is up.
    Start,
    /// Accessibility granted while the application ran — see [`pump`].
    AccessibilityGranted,
    /// The Screen Recording button on the Permissions page, which climbs past the documented
    /// request when pressed again (`enrol::screen_step`).
    Button,
}

impl Trigger {
    fn words(self) -> &'static str {
        match self {
            Trigger::Start => "at start, once the application was up",
            Trigger::AccessibilityGranted => "now that Accessibility is granted",
            Trigger::Button => "from the Permissions page",
        }
    }
}

// What the Permissions page says after a press that asked. The pane is not opened then — see
// `backend::Asked` — so these say where the dialog's own button leads, and what a press that
// finds no dialog does next.
const SAY_SCREEN_FIRST: &str = "macOS has been asked for Screen Recording. If it shows a dialog, \
     its button that opens the settings leads to the list. If no dialog came up, press this \
     button again: it asks a second way.";
const SAY_SCREEN_LAST: &str = "macOS has been asked for Screen Recording in every way this \
     application knows. If it shows a dialog, its button that opens the settings leads to the \
     list. If none came up, press this button again to open the list, and add the application \
     there with the plus button.";
const SAY_LISTEN_FIRST: &str = "macOS has been asked for Input Monitoring. If it shows a dialog, \
     its button that opens the settings leads to the list. If no dialog came up, press this \
     button again: it asks a second way.";
const SAY_LISTEN_LAST: &str = "macOS has been asked for Input Monitoring a second way. If it \
     shows a dialog, its button that opens the settings leads to the list. If none came up, \
     press this button again to open the list, and add the application there with the plus \
     button.";
const SAY_ACCESSIBILITY_FIRST: &str = "Accessibility is not granted yet. Grant it first: the key \
     capture needs it before anything else. Nothing was asked for Input Monitoring.";

/// Asks for Accessibility once, with the system prompt.
pub fn request_accessibility_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // The prompting form is the only way an application can raise the Accessibility
        // dialog at all; there is no API that grants it. It also answers the question, so
        // this doubles as the first check. A user who grants it in response to this very
        // prompt still gets `false` here — the answer is read before they could act — so what
        // to do next is logged either way. No restart, though: the grant reaches this process
        // (see ACCESSIBILITY_NOTE).
        let trusted = is_trusted(true);
        if trusted {
            crate::logging::line("macos", "accessibility: granted");
            return;
        }
        for msg in [
            "accessibility: NOT granted — window reads, control reads and key capture will \
             all return nothing until it is",
            "accessibility: a system dialog may be open now; the button on it that opens the \
             settings leads to the right pane",
        ] {
            crate::logging::line("macos", msg);
        }
        crate::logging::line(
            "macos",
            &format!(
                "accessibility: otherwise open {} > Accessibility and switch this \
                 application on in the list",
                privacy_pane()
            ),
        );
        crate::logging::line("macos", &format!("accessibility: {ACCESSIBILITY_NOTE}"));
    });
}

/// Arranges for Screen Recording to be asked for, once, at start. The asking itself is done by
/// [`pump`], on the first tick at which Accessibility is granted.
///
/// Separate from the accessibility request and not optional, because of a macOS detail that
/// cost a tester their first session: **an application does not appear in the Screen
/// Recording list until it has asked.** `CGPreflightScreenCaptureAccess` is a question, not
/// a request — it reports the current state, raises no dialog, and enrols nothing. So an
/// application that only ever preflights is invisible in that pane, and the user is left
/// looking for a switch that is not there, with no way to grant a permission the
/// application will then complain about not having. Measured on macOS 12.7.6:
/// "Could grant accessibility permission, but it doesn't make itself available for screen
/// recording yet."
///
/// **Not here, and not while the Accessibility dialog may be up.** This runs while the backend
/// is built, before the GUI exists: a dialog raised now comes up before AppKit, and under the
/// application's own startup window and its spoken "cannot work yet" — the state both earlier
/// field observations were made in. And [`request_accessibility_once`] runs just before this and
/// raises a dialog whenever Accessibility is missing; a second system dialog raised at the same
/// instant lands on top of it, for somebody who hears one of them and cannot see that there are
/// two — and on macOS 14.5 the application appeared in the Screen Recording list only after
/// Accessibility had been granted. So [`pump`] asks: its first look is the first tick, when the
/// window and its announcement are out, and it asks then if Accessibility is granted, or the
/// moment it is. The Screen Recording button on the Permissions page asks whenever it is
/// pressed. Accessibility is the one to grant first in any case — nothing works without it.
pub fn request_screen_recording_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if CGPreflightScreenCaptureAccess() {
            crate::logging::line("macos", "screen recording: granted");
            return;
        }
        let trusted = is_trusted(false);
        TRUSTED_AT_START.store(trusted, Ordering::SeqCst);
        SCREEN_RECORDING_WAITING.store(true, Ordering::SeqCst);
        crate::logging::line(
            "macos",
            if trusted {
                "screen recording: NOT granted; asked for as soon as the application is up, not \
                 before — a dialog raised now would come up under its startup window"
            } else {
                "screen recording: NOT granted, and not asked for yet: the Accessibility dialog \
                 may be on screen, and a second dialog on top of it is one too many. It is asked \
                 for as soon as Accessibility is granted, or from the Screen Recording button on \
                 the Permissions page."
            },
        );
    });
}

/// Asks for Screen Recording once Accessibility is granted, if it is waiting for that.
///
/// Called from the pump, on the main thread, every tick; it looks at Accessibility at most once
/// a second and only while a request is waiting, so otherwise it costs one atomic load. The first
/// look is the first tick: in the GUI that is after the window and its announcement are out, and
/// after AppKit is up.
///
/// While it waits, App Nap is kept away (`activity::SETUP`): the grant is made in System
/// Settings, with this application's window covered, which is when macOS naps an accessory
/// application and stretches its timers — and "once a second" would become whenever the pump
/// next turns. Released by [`ask_for_screen_recording`], whoever makes the request — and so held
/// for the whole session when Accessibility is never granted, which is why this reason on its
/// own is an activity without `LatencyCritical` (`activity_reasons.rs`).
pub fn pump() {
    if !SCREEN_RECORDING_WAITING.load(Ordering::Relaxed) {
        return;
    }
    let now = Instant::now();
    let due = LAST_ACCESSIBILITY_LOOK.with(|last| match last.get() {
        Some(t) if now.duration_since(t) < ACCESSIBILITY_LOOK_EVERY => false,
        _ => {
            last.set(Some(now));
            true
        }
    });
    if !due {
        return;
    }
    // The quiet check, not `is_trusted`: that one writes a trace line per call, and this one is
    // made every second for as long as the grant takes.
    if !unsafe { AXIsProcessTrusted() } {
        // Idempotent: one thread-local read once the activity is held.
        super::activity::want(super::activity::SETUP, true);
        return;
    }
    SCREEN_RECORDING_WAITING.store(false, Ordering::SeqCst);
    let trigger = if TRUSTED_AT_START.load(Ordering::SeqCst) {
        Trigger::Start
    } else {
        Trigger::AccessibilityGranted
    };
    ask_for_screen_recording(trigger);
}

/// Makes the next Screen Recording request this process has left, and says what the page does
/// next (the automatic request ignores that).
///
/// The requests, in order, each at most once per process — which of them a call makes is
/// `enrol::screen_step`'s decision:
///
/// 1. `CGRequestScreenCaptureAccess`, the documented one, here on the calling thread because it
///    returns at once — the dialog is drawn by another process. It answers with the state as it
///    was at process start, so a user who grants it in response to this very prompt still sees
///    `false` — which is why the restart note is logged either way. The automatic request makes
///    this one alone, except on macOS 12.
/// 2. ScreenCaptureKit's list of what can be captured, on a thread of its own with a bounded
///    wait — a capture request as far as TCC is concerned, which the documented one on macOS
///    12.7.6 evidently was not — and one point through the older capture functions where
///    ScreenCaptureKit is missing or did not answer, and on macOS 12 whatever it answered (see
///    `enrol::legacy_reason`). Made by the button's next press, or straight after the first
///    request on macOS 12.
///
/// Every step writes what it asked and what came back. Nothing here can say whether an entry
/// was added — no public call answers that — so the log ends with what is left to do.
fn ask_for_screen_recording(trigger: Trigger) -> Asked {
    let button = trigger == Trigger::Button;
    // A request from anywhere settles the automatic one, which is made at most once as well, and
    // lets go of the App Nap activity [`pump`] held while it waited. Both callers are on the
    // main thread, which is the one the activity is kept on.
    SCREEN_RECORDING_WAITING.store(false, Ordering::SeqCst);
    super::activity::want(super::activity::SETUP, false);
    if CGPreflightScreenCaptureAccess() {
        if button {
            crate::logging::line("macos", "screen recording: granted; only the pane is opened");
        }
        return Asked::PANE;
    }
    let major = macos_major();
    let (pane, bundle, monterey) = (privacy_pane(), bundle_id(), prompt_is_unreliable());
    let done = Rung::from_u8(SCREEN_RECORDING_RUNG.load(Ordering::SeqCst));
    let step = super::enrol::screen_step(done, button, major);
    if step == ScreenStep::Nothing {
        if button {
            crate::logging::line(
                "macos",
                "screen recording: every request this application makes has been made in this \
                 run, so only the pane is opened",
            );
            crate::logging::line(
                "macos",
                &super::enrol::fallback_line(false, pane, &bundle, monterey),
            );
        }
        return Asked::PANE;
    }
    SCREEN_RECORDING_RUNG.store(super::enrol::screen_after(step, done) as u8, Ordering::SeqCst);
    if done == Rung::Nothing {
        // Which application the dialog and the list entry will be about. Said only when it is
        // not this one, which is the case worth a line.
        let ppid = unsafe { libc::getppid() };
        if !super::enrol::launched_by_launchd(ppid) {
            crate::logging::line(
                "macos",
                &format!(
                    "screen recording: started by {}",
                    super::enrol::launch_line(ppid, process_name(ppid).as_deref())
                ),
            );
        }
    }
    let pause_first = match step {
        ScreenStep::Documented { captures_too } => {
            let granted = CGRequestScreenCaptureAccess();
            crate::logging::line(
                "macos",
                &format!(
                    "screen recording: asked with CGRequestScreenCaptureAccess ({}); it answered {}",
                    trigger.words(),
                    if granted { "granted" } else { "not granted" }
                ),
            );
            if granted {
                crate::logging::line("macos", "screen recording: granted after asking");
                return Asked::PANE;
            }
            crate::logging::line(
                "macos",
                "screen recording: NOT granted. Asking is also what puts an application into that \
                 list at all, so it is done even when the answer is already known.",
            );
            crate::logging::line(
                "macos",
                "screen recording: without it a capture does not fail. It comes back as a picture \
                 of the wallpaper, so every image search and every OCR read reads empty with \
                 nothing anywhere to explain why.",
            );
            crate::logging::line(
                "macos",
                &format!(
                    "screen recording: {pane} > Screen Recording (Screen & System Audio Recording \
                     from macOS 15), {RESTART_NOTE}"
                ),
            );
            if !captures_too {
                crate::logging::line(
                    "macos",
                    &super::enrol::fallback_line(true, pane, &bundle, monterey),
                );
                return Asked { open_pane: false, say: Some(SAY_SCREEN_FIRST) };
            }
            true
        }
        ScreenStep::Captures => {
            crate::logging::line(
                "macos",
                &format!(
                    "screen recording: asking a second way ({}): a capture request",
                    trigger.words()
                ),
            );
            false
        }
        ScreenStep::Nothing => unreachable!("returned above"),
    };
    // Core Graphics' connection to the window server, made on this thread before ScreenCaptureKit
    // is asked from another: headless there is no AppKit to have made it, and a framework that
    // finds none can abort the process (a ScreenCaptureKit call in a command-line tool,
    // developer forums thread 743615). Cheap, and a no-op once the connection exists.
    let _ = CGMainDisplayID();
    let fallback = super::enrol::fallback_line(false, pane, &bundle, monterey);
    let spawned = std::thread::Builder::new()
        .name("screen-recording-request".into())
        .spawn(move || capture_requests(major, pause_first, fallback));
    if let Err(e) = spawned {
        crate::logging::line(
            "macos",
            &format!(
                "screen recording: the thread for the capture requests could not be started ({e}); \
                 they were not made"
            ),
        );
        crate::logging::line("macos", &super::enrol::fallback_line(false, pane, &bundle, monterey));
        // Nothing was asked by this press unless the documented request went out with it.
        if step == ScreenStep::Captures {
            return Asked::PANE;
        }
    }
    Asked { open_pane: false, say: Some(SAY_SCREEN_LAST) }
}

/// The capture requests, on the request's own thread: nothing here may hold the thread that
/// carries the event tap, and the wait for ScreenCaptureKit is bounded. `pause_first` when a
/// request was made the moment before; `fallback` is the line the log ends with, whatever came
/// back — ScreenCaptureKit handing the content over is a strong sign, not a proof.
fn capture_requests(macos_major: u64, pause_first: bool, fallback: String) {
    objc2::rc::autoreleasepool(|_| {
        if pause_first {
            std::thread::sleep(BETWEEN_REQUESTS);
        }
        let sck = super::capture::ask_for_shareable_content(SHAREABLE_CONTENT_WAIT);
        crate::logging::line("macos", &super::enrol::sck_line(&sck));
        if let Some(why) = super::enrol::legacy_reason(&sck, macos_major) {
            // ScreenCaptureKit answers in milliseconds even while a dialog it raised is still on
            // screen, so the next request would otherwise arrive on top of it.
            std::thread::sleep(BETWEEN_REQUESTS);
            let answer = super::capture::legacy_one_point();
            crate::logging::line("macos", &super::enrol::legacy_line(answer, why));
        }
        crate::logging::line("macos", &fallback);
    });
}

/// The macOS major version: 12 for Monterey, 26 for Tahoe.
fn macos_major() -> u64 {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion as u64
}

/// The bundle identifier the privacy lists know this application by, for the `tccutil` command
/// the log names. The packaged one where there is no bundle to ask.
fn bundle_id() -> String {
    NSRunningApplication::currentApplication()
        .bundleIdentifier()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "com.automationplatform.app".to_string())
}

/// The name of a process, as `ps` shows it, if it can be read.
fn process_name(pid: i32) -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer and its length match; proc_name writes at most that many bytes and
    // answers how many it wrote.
    let n = unsafe { libc::proc_name(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    if n <= 0 {
        return None;
    }
    // Up to the terminator, whether or not the count included it.
    let name = buf[..(n as usize).min(buf.len())].split(|&b| b == 0).next().unwrap_or(&[]);
    (!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned())
}

/// How this bundle is signed, as `codesign` sees it.
///
/// Asked of the tool rather than of an API because the answer only has to be readable, not
/// programmable, and `codesign` is the same thing a person would run to check. It writes to
/// stderr, which is why that is what gets parsed. Failure is silent by design: an
/// unsignable or unsigned bundle is a legitimate state and the absence of these lines says
/// so as clearly as a message would.
///
/// Two fields matter and they answer different questions. `authority` is WHO signed it — an
/// ad-hoc signature has none, a local certificate names itself — and it changing means the
/// signing arrangement changed. `cdhash` is the content, and for an ad-hoc signature it is
/// the whole identity, so it changing between two runs is exactly why permissions granted
/// before the rebuild are not honoured after it.
fn signature_of(bundle_path: &str) -> Vec<(String, String)> {
    let Ok(out) = std::process::Command::new("/usr/bin/codesign")
        // verbose=4, not 2. At 2 there is no CDHash line at all, and an ad-hoc signature
        // prints "Signature=adhoc" with no Authority — which the first version of this
        // function read as "not signed", and then said so in a tester's log about a bundle
        // that was signed perfectly well, just not the way we wanted.
        .args(["-dv", "--verbose=4", bundle_path])
        .output()
    else {
        return Vec::new();
    };
    // codesign reports on stderr even when it succeeds.
    let text = String::from_utf8_lossy(&out.stderr);
    let mut authority: Option<String> = None;
    let mut cdhash: Option<String> = None;
    let mut ident: Option<String> = None;
    let mut adhoc = false;
    let mut unsigned = false;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Authority=") {
            // The first Authority line is the leaf; the rest are the chain above it.
            authority.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("CDHash=") {
            cdhash.get_or_insert_with(|| v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Identifier=") {
            ident.get_or_insert_with(|| v.trim().to_string());
        } else if line.trim() == "Signature=adhoc" {
            adhoc = true;
        } else if line.contains("not signed at all") {
            unsigned = true;
        }
    }
    let mut out_pairs = Vec::new();
    let verdict = if let Some(a) = &authority {
        format!("signed by {a} — permissions survive a rebuild")
    } else if adhoc {
        "ad-hoc (no certificate). The identity is derived from the binary, so it changes \
         on every rebuild and macOS forgets every permission each time. \
         ./macos-signing-identity.sh fixes that."
            .to_string()
    } else if unsigned {
        "none — not signed at all, so no permission will be remembered".to_string()
    } else {
        "could not be determined".to_string()
    };
    out_pairs.push(("signature".to_string(), verdict));
    if let Some(i) = ident {
        out_pairs.push(("signature identifier".to_string(), i));
    }
    if let Some(h) = cdhash {
        // The number to compare between two logs. If it differs and permissions stopped
        // working, that is the whole explanation.
        out_pairs.push(("signature cdhash".to_string(), h));
    }
    out_pairs
}

/// What the privacy settings are actually CALLED on the machine this is running on.
///
/// Ventura renamed System Preferences to System Settings and turned "Security & Privacy"
/// round into "Privacy & Security". Telling a Monterey user to open a pane named after a
/// version they do not have sends them looking for something that is not there — and this
/// text is read aloud to somebody who cannot scan the window for the nearest match.
pub(super) fn privacy_pane() -> &'static str {
    if NSProcessInfo::processInfo().operatingSystemVersion().majorVersion >= 13 {
        "System Settings > Privacy & Security"
    } else {
        "System Preferences > Security & Privacy > Privacy tab (unlock the padlock first)"
    }
}

/// Whether this macOS is old enough that the documented screen-recording request cannot be
/// relied on.
///
/// Measured rather than assumed. On 12.7.6 the request raised no dialog and added nothing to
/// the list, and the tester got past it by adding the application by hand with the + button;
/// the same shape of problem was reported independently on Monterey for a different
/// permission entirely, which points at the OS version rather than at anything this
/// application does. It is why the capture requests after it make both kinds of request on
/// that version (`enrol::legacy_reason`), and why the log's hand-made way in says so there.
fn prompt_is_unreliable() -> bool {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion < 13
}

use crate::backend::{Asked, Grant, Permission};

/// The four permissions, as they stand right now.
///
/// Written for the permissions window, which exists because the log was the only place any of
/// this appeared — and a log is what a tester reads AFTERWARDS, when the session has already
/// been spent. macOS never says which permission is missing; it lets the application behave as
/// though it were broken, and three of the four fail without any error at all. Somebody
/// setting this up on a new machine needs to be told what is done and what is not, in the
/// application, before they start.
///
/// Nothing here prompts. Every check is the quiet form, so the list can be built whenever the
/// window is opened without putting a dialog on screen behind a screen reader's back.
pub fn permissions() -> Vec<Permission> {
    vec![
        Permission {
            name: "Accessibility",
            state: if is_trusted(false) { Grant::Granted } else { Grant::Missing },
            without: "Nothing can be read or clicked. Every window and control comes back \
                      empty, and no overlay ever activates.",
            anchor: ACCESSIBILITY_PANE,
            can_ask: true,
            blocking: true,
        },
        Permission {
            name: "Screen Recording",
            state: if CGPreflightScreenCaptureAccess() {
                Grant::Granted
            } else {
                Grant::Missing
            },
            without: "Captures do not fail. They come back as a picture of the desktop with \
                      every other application removed, so image search finds nothing and OCR \
                      reads nothing, for ever, without an error. This is the dangerous one. \
                      This application asks for it as soon as Accessibility is granted, and \
                      asking is what normally puts it into that list. The button below asks \
                      too, one way per press, and opens the list once nothing is left to ask; \
                      there, the + button adds AutomationPlatform.app by hand. From macOS 15 \
                      on, System Settings calls that list Screen & System Audio Recording.",
            anchor: SCREEN_RECORDING_PANE,
            can_ask: true,
            blocking: true,
        },
        Permission {
            name: "Input Monitoring",
            state: match listen_state() {
                Listen::Granted => Grant::Granted,
                Listen::Denied => Grant::Missing,
                Listen::Unknown => Grant::Unknown,
            },
            without: "If it is needed and missing, keys an overlay has claimed reach the plugin \
                      instead of the overlay. Whether this application needs it next to \
                      Accessibility is not known yet, so it is not asked for at start. If \
                      claimed keys do reach the plugin while Accessibility is granted, the \
                      button below asks macOS for it, one way per press, and opens the list \
                      once nothing is left to ask.",
            anchor: INPUT_MONITORING_PANE,
            can_ask: true,
            blocking: true,
        },
        Permission {
            name: "Automation, for VoiceOver",
            state: match voiceover_automation() {
                Automation::Granted => Grant::Granted,
                Automation::Refused => Grant::Missing,
                // Never asked, or the question could not be put. Both are "not known yet"
                // rather than "refused", and the difference decides whether ticking the
                // setting will raise a dialog or do nothing visible.
                Automation::NotAsked | Automation::Unknown => Grant::Unknown,
            },
            without: "Only needed for \"Speak through VoiceOver\". Without it that setting \
                      looks on and the overlay goes on speaking in its own voice — macOS \
                      refuses the Apple Event silently. Ticking the setting is what asks for \
                      it. VoiceOver's own AppleScript setting, below, is needed as well.",
            anchor: AUTOMATION_PANE,
            can_ask: true,
            blocking: false,
        },
    ]
}

/// Opens a settings pane, and says whether macOS took it.
///
/// The return value is not decoration. These URLs are the documented way in, but they are also
/// the sort of thing Apple has moved before — System Preferences became System Settings in
/// Ventura and the anchors survived, which is evidence rather than a promise. A button that
/// silently does nothing is the worst outcome for somebody who cannot see whether a window
/// opened, so a refusal is logged and the caller says so.
pub fn open_pane(anchor: &str) -> bool {
    let url = objc2_foundation::NSURL::URLWithString(&NSString::from_str(anchor));
    let Some(url) = url else {
        crate::logging::line("macos", &format!("settings pane: '{anchor}' is not a URL"));
        return false;
    };
    let opened = NSWorkspace::sharedWorkspace().openURL(&url);
    crate::logging::line(
        "macos",
        &format!(
            "settings pane: {} for {anchor}",
            if opened { "opened" } else { "REFUSED by macOS" }
        ),
    );
    opened
}

/// Asks for a permission from its button on the Permissions page, and says what the page does
/// next — see [`Asked`].
///
/// Screen Recording and Input Monitoring make their next request, one per press, and leave the
/// pane shut while its dialog may be up; a press with nothing left to ask opens the pane.
/// Accessibility is asked with its prompt at start, which enrols it at once, so its button opens
/// the pane; Automation is asked for by ticking the setting that needs it.
pub fn ask_for(name: &str) -> Asked {
    match name {
        "Accessibility" => {
            request_accessibility_once();
            Asked::PANE
        }
        "Screen Recording" => ask_for_screen_recording(Trigger::Button),
        "Input Monitoring" => ask_for_input_monitoring(),
        _ => Asked::PANE,
    }
}

/// Asks for Input Monitoring from its button, one way per press — `enrol::listen_step` decides
/// which, from Accessibility, the state `IOHIDCheckAccess` reads, and how far this run has got.
///
/// **Only from the button, never at start.** Whether this application needs it next to
/// Accessibility has not been measured (TODO.md): the key tap is created active, the kind
/// Accessibility governs, and captured keys were seen suppressed on a Mac this application had
/// never asked for Input Monitoring. The button is for the case where keys an overlay has claimed
/// still reach the application underneath.
///
/// The first way is `CGRequestListenEventAccess` (macOS 10.15, "potentially prompting" in its
/// header); the second, `IOHIDRequestAccess` for listening, which another project measured
/// raising a dialog on macOS 26.6 where the first raised none. Each on a thread of its own,
/// because neither header says whether the call waits for the user's answer, and the thread
/// carrying the event tap must not wait for anybody's. The state before and after goes to the
/// log: unknown to denied is the one visible sign that an entry now exists.
fn ask_for_input_monitoring() -> Asked {
    let trusted = is_trusted(false);
    let before = listen_state();
    let done = Rung::from_u8(INPUT_MONITORING_RUNG.load(Ordering::SeqCst));
    let step = super::enrol::listen_step(trusted, before, done);
    let pane_only = |why: &str| {
        crate::logging::line("macos", &format!("input monitoring: {why}; only the pane is opened"));
    };
    match step {
        ListenStep::AccessibilityFirst => {
            pane_only(
                "not asked for — Accessibility is not granted yet, the key tap needs that first, \
                 and its dialog may be on screen",
            );
            return Asked { open_pane: true, say: Some(SAY_ACCESSIBILITY_FIRST) };
        }
        ListenStep::Granted => {
            pane_only("granted");
            return Asked::PANE;
        }
        ListenStep::InListOff => {
            pane_only(
                "denied — the application is in the list, switched off, and switching it on is \
                 what is left, so nothing is asked",
            );
            return Asked::PANE;
        }
        ListenStep::Exhausted => {
            pane_only(&format!(
                "both requests have been made in this run and it still reads unknown. If this \
                 application is not in the list, press + under it and choose \
                 AutomationPlatform.app, then switch it on — {INPUT_MONITORING_NOTE}"
            ));
            return Asked::PANE;
        }
        ListenStep::Documented | ListenStep::Hid => {}
    }
    INPUT_MONITORING_RUNG.store(super::enrol::listen_after(step, done) as u8, Ordering::SeqCst);
    let spawned = std::thread::Builder::new()
        .name("input-monitoring-request".into())
        .spawn(move || {
            let (via, granted) = if step == ListenStep::Documented {
                ("CGRequestListenEventAccess", CGRequestListenEventAccess())
            } else {
                (
                    "IOHIDRequestAccess (listen)",
                    IOHIDRequestAccess(IOHIDRequestType::ListenEvent),
                )
            };
            let after = listen_state();
            crate::logging::line(
                "macos",
                &format!(
                    "input monitoring: asked with {via}; it answered {}. IOHIDCheckAccess (listen) \
                     read {} before and {} after{}",
                    if granted { "granted" } else { "not granted" },
                    before.word(),
                    after.word(),
                    super::enrol::listen_change(before, after)
                ),
            );
        });
    match spawned {
        Ok(_) => Asked {
            open_pane: false,
            say: Some(if step == ListenStep::Documented { SAY_LISTEN_FIRST } else { SAY_LISTEN_LAST }),
        },
        Err(e) => {
            pane_only(&format!("the request could not be started ({e})"));
            Asked::PANE
        }
    }
}

/// The startup environment block: displays and their scale, the three permissions, the
/// macOS version, whether VoiceOver is running.
///
/// Asked exactly once, from `Backend::environment`, so it may take its time. It must not
/// prompt for anything — the one prompt this application shows has already been shown by
/// [`request_accessibility_once`], and a second dialog during startup would be read out on
/// top of the speech engine announcing itself.
/// Whether this application may send Apple Events to VoiceOver.
///
/// The fourth permission, and the one nothing else in this file covers. Accessibility lets
/// us READ other applications; Screen Recording lets us capture them; Input Monitoring lets
/// us see keys; **Automation** is what lets us tell an application to do something — and
/// every line the VoiceOver transport says is an Apple Event. It lives in its own pane, is
/// refused by default, and its refusal is quiet: TCC declines the event, the transport's
/// health flag goes false, and the overlay falls back to its own voice. So the symptom is
/// not silence — it is the overlay talking in the wrong voice, which is easy to mistake for
/// the setting simply not having taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Automation {
    Granted,
    /// Asked and declined, or switched off in System Settings since.
    Refused,
    /// Nobody has been asked yet. The ordinary state of a fresh install, and its own word
    /// rather than "unknown", because the two mean different things to whoever reads the log.
    NotAsked,
    /// The question could not be put, or was answered with something this code does not know.
    Unknown,
}

impl Automation {
    /// The value column of the startup block: short, greppable, no pane name.
    ///
    /// The pane goes in a separate `… fix` row, which is this file's own convention — and
    /// which also keeps the version-correct name out of a `&'static str`.
    fn as_str(self) -> &'static str {
        match self {
            Automation::Granted => "granted",
            Automation::Refused => "REFUSED",
            Automation::NotAsked => "not asked yet (asked when the setting is switched on)",
            Automation::Unknown => "could not be determined",
        }
    }
}

/// `procNotFound`, the documented answer when the target application is not running.
///
/// Spelled out here because it lives in CarbonCore rather than in the AppleEvents module
/// this file already imports, and pulling in a framework for one integer is not worth it.
const PROC_NOT_FOUND: i32 = -600;

/// Puts the permission question to TCC. With `prompt`, this is also what raises the dialog.
///
/// One function for both, because the system provides one:
/// `AEDeterminePermissionToAutomateTarget` takes an `askUserIfNeeded` flag, so the check and
/// the request differ by an argument rather than by mechanism. Worth preferring over the
/// common trick of sending a harmless real event to raise the dialog — a fake event is a
/// real event to whoever receives it.
///
/// **Every status Apple documents is handled by name**, because the interesting one is the
/// ordinary one: with `prompt` false and nobody yet asked, the answer is
/// `errAEEventWouldRequireUserConsent`, which is the state of every fresh install. An
/// earlier draft let that fall into the catch-all and wrote "the system answered -1744,
/// which this code does not recognise" into the startup block of every first run.
///
/// Builds its own descriptor rather than taking one, so it can be called from a worker
/// thread — see `request_voiceover_automation`.
fn ask_tcc(prompt: bool) -> Automation {
    let target = NSAppleEventDescriptor::descriptorWithBundleIdentifier(&NSString::from_str(
        "com.apple.VoiceOver",
    ));
    // SAFETY: `aeDesc` borrows the descriptor's own storage, which outlives the call, and the
    // call does not keep it. The wildcards ask about any event of any class, which the
    // documentation names as the way to ask "may I send anything at all".
    let status = unsafe {
        let desc = target.aeDesc();
        if desc.is_null() {
            return Automation::Unknown;
        }
        AEDeterminePermissionToAutomateTarget(desc, typeWildCard, typeWildCard, prompt)
    };
    match status {
        0 => Automation::Granted,
        s if s == errAEEventNotPermitted => Automation::Refused,
        s if s == errAEEventWouldRequireUserConsent => Automation::NotAsked,
        // VoiceOver quit between the check above and this call. Not worth a line: the next
        // switch-on asks again, and the startup block already says VoiceOver is not running.
        PROC_NOT_FOUND => Automation::Unknown,
        other => {
            crate::logging::line(
                "macos",
                &format!(
                    "VoiceOver automation: the system answered {other}, which is none of the \
                     statuses this call documents — treating it as unknown"
                ),
            );
            Automation::Unknown
        }
    }
}

/// The permission as it stands, without putting anything on screen.
///
/// Safe on the main thread: the warning below is about the PROMPTING call, which can sit
/// there as long as the user takes to read a dialog.
fn voiceover_automation() -> Automation {
    if !voiceover_running() {
        return Automation::Unknown;
    }
    ask_tcc(false)
}

/// Requests made by [`request_voiceover_automation`] whose prompting call has not returned yet —
/// a count, because ticking the setting off and on again quickly makes a second one.
static AUTOMATION_ASKING: AtomicUsize = AtomicUsize::new(0);

/// Whether the Automation question for VoiceOver is on screen because the setting was ticked: a
/// request is out and has not come back. The system's own answer cannot say — asked without
/// asking the user, it gives -1744 both while the question is up and before it was ever put.
pub fn voiceover_automation_asking() -> bool {
    AUTOMATION_ASKING.load(Ordering::SeqCst) > 0
}

/// Asks the USER for the permission, and logs what came back.
///
/// Called when "Speak through VoiceOver" is switched on, and only then. It raises a system
/// dialog, and a dialog nobody asked for is worse than a missing permission: the user would
/// have to work out what it wants and why it appeared now. The moment they switch the
/// setting on is the moment the question is obviously about what they just did.
///
/// **On a thread of its own, because Apple says so** — the header for this call reads "Do
/// not call this function on your main thread because it may take arbitrarily long to
/// return if the user needs to be prompted for consent". That is not a style note here: the
/// main thread is the one carrying the CGEvent tap, and a stall of about a second is what
/// gets the tap switched off. A modal dialog on it would take the keyboard away for as long
/// as the dialog is up, which on this platform means for as long as a blind user needs to
/// find and answer it. The call is documented thread-safe since 10.14.
///
/// Not `Once`, unlike the two permissions above: the user can switch this off and on again,
/// and the second time is exactly when they are trying to fix the thing this asks about.
/// Whether macOS shows the dialog a second time after a refusal is its decision, not ours —
/// it does not, in general, which is why the log names the pane instead.
///
/// While the request is out, [`voiceover_automation_asking`] says so: the speech path does not
/// talk over the dialog (`speech/vo_script.rs`, which needs to tell "on screen" from "not asked
/// yet", both of which the system answers -1744).
pub fn request_voiceover_automation() {
    if !voiceover_running() {
        crate::logging::line(
            "macos",
            "VoiceOver automation not requested: VoiceOver is not running, so there is \
             nothing to ask about yet. Switch the setting on again with VoiceOver up.",
        );
        return;
    }
    let before = voiceover_automation();
    if before == Automation::Granted {
        crate::logging::line("macos", "VoiceOver automation: already granted");
        return;
    }
    crate::logging::line(
        "macos",
        &format!(
            "VoiceOver automation: {} — asking, on a worker thread so the dialog cannot hold \
             the event tap",
            before.as_str()
        ),
    );
    // Counted up before the thread starts, so the speech path's next pass already sees it; down
    // once the call has returned, which it does when the dialog has been answered.
    AUTOMATION_ASKING.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        let after = ask_tcc(true);
        AUTOMATION_ASKING.fetch_sub(1, Ordering::SeqCst);
        crate::logging::line(
            "macos",
            &format!(
                "VoiceOver automation: {} after asking{}",
                after.as_str(),
                if after == Automation::Granted {
                    String::new()
                } else {
                    format!(" — {} > Automation, then tick this application's VoiceOver entry", privacy_pane())
                }
            ),
        );
    });
}

pub fn environment_report() -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |k: &str, v: String| out.push((k.to_string(), v));

    // --- machine ---------------------------------------------------------------------
    let info = NSProcessInfo::processInfo();
    let v = info.operatingSystemVersion();
    push(
        "macos",
        format!(
            "{}.{}.{} ({})",
            v.majorVersion,
            v.minorVersion,
            v.patchVersion,
            info.operatingSystemVersionString()
        ),
    );

    // Three different answers to "which architecture", and they can legitimately differ.
    // `ARCH` is what this binary was built for; `hw.machine` is what the process thinks it
    // is running on, which reads `x86_64` under Rosetta even on Apple silicon; `hw.model`
    // is the only one that names the actual machine.
    push(
        "arch",
        format!(
            "{} binary / {} hw.machine / {} hw.model",
            std::env::consts::ARCH,
            sysctl_string("hw.machine").unwrap_or_else(|| "?".into()),
            sysctl_string("hw.model").unwrap_or_else(|| "?".into()),
        ),
    );
    push(
        "rosetta",
        match sysctl_i32("sysctl.proc_translated") {
            Some(1) => "YES — an Intel binary translated on Apple silicon; Vision, timing \
                        and permission identity all differ from a native build"
                .to_string(),
            Some(_) => "no".to_string(),
            // The sysctl does not exist at all on an Intel Mac, which is itself an answer.
            None => "no (sysctl.proc_translated absent)".to_string(),
        },
    );

    // --- displays --------------------------------------------------------------------
    // The whole port assumes one point of AX geometry equals one pixel of a capture, and
    // on a Retina display that assumption is off by a factor of two in a way that produces
    // no error, only clicks in the wrong half of the screen. So the scale goes in the log
    // of the very first session, derived two independent ways, in the coordinate space the
    // backend actually uses (CoreGraphics: points, top-left origin, primary at 0,0).
    let main_id = CGMainDisplayID();
    let mut cg_primary_scale = None;
    let displays = active_displays();
    if displays.is_empty() {
        push("display", "none reported by CoreGraphics".into());
    }
    for (i, id) in displays.into_iter().enumerate() {
        let bounds = CGDisplayBounds(id);
        let mode = CGDisplayCopyDisplayMode(id);
        let (pt_w, pt_h, px_w, px_h) = match mode.as_deref() {
            Some(m) => (
                CGDisplayMode::width(Some(m)),
                CGDisplayMode::height(Some(m)),
                CGDisplayMode::pixel_width(Some(m)),
                CGDisplayMode::pixel_height(Some(m)),
            ),
            None => (0, 0, 0, 0),
        };
        let scale = if pt_w > 0 { px_w as f64 / pt_w as f64 } else { 0.0 };
        if id == main_id {
            cg_primary_scale = Some(scale);
        }
        push(
            &format!("display {i}"),
            format!(
                "{}x{} pt at ({},{}) / mode {pt_w}x{pt_h} pt {px_w}x{px_h} px / scale {scale:.2} \
                 [cgid {id}]{}",
                bounds.size.width,
                bounds.size.height,
                bounds.origin.x,
                bounds.origin.y,
                if id == main_id { " PRIMARY" } else { "" },
            ),
        );
    }

    // AppKit's own answer, which is the one Cocoa drawing believes. Disagreement with the
    // mode-derived figure above would mean the display is running a scaled mode, and that
    // is worth knowing before blaming the capture code.
    push("backing scale", backing_scale(cg_primary_scale));

    // --- permissions -----------------------------------------------------------------
    // Deliberately the non-prompting check: this runs during startup, after the speech
    // engine has come up, and a dialog here would talk over it.
    if is_trusted(false) {
        push("accessibility", "granted".into());
    } else {
        push(
            "accessibility",
            "NOT granted — every window, control and key-capture call comes back empty".into(),
        );
        push(
            "accessibility fix",
            format!("{} > Accessibility; {ACCESSIBILITY_NOTE}", privacy_pane()),
        );
    }

    // The dangerous one. Absence is not an error anywhere in the capture path: the picture
    // comes back, it just shows the desktop wallpaper with every other application's window
    // missing, so image search reports "no match" and OCR reports "no text" forever.
    if CGPreflightScreenCaptureAccess() {
        push("screen recording", "granted".into());
    } else {
        push(
            "screen recording",
            "reported as NOT granted — if that is right, a capture comes back as the \
             desktop with other applications' windows removed, so every image search and \
             every OCR read fails silently. This check has been caught answering 'no' for \
             an application that could capture perfectly well, so read the capture lines \
             further down before acting on it."
                .into(),
        );
        push(
            "screen recording fix",
            format!(
                "{} > Screen Recording (Screen & System Audio Recording from macOS 15), \
                 {RESTART_NOTE}",
                privacy_pane()
            ),
        );
    }

    // Input Monitoring's own state: `IOHIDCheckAccess` for listening. Reported, not asked for:
    // whether this application needs it next to Accessibility has not been measured (TODO.md),
    // and `unknown` is simply the state of an application that has never asked. It is listed
    // because it is the switch to try if keys an overlay has claimed reach the application
    // underneath while Accessibility is granted.
    match listen_state() {
        Listen::Granted => push("input monitoring", "granted".into()),
        Listen::Denied => {
            push(
                "input monitoring",
                "NOT granted — refused in a dialog, or switched off in the list. If keys an \
                 overlay has claimed reach the application underneath, this is the switch to try."
                    .into(),
            );
            push("input monitoring symptom", INPUT_MONITORING_SYMPTOM.into());
            push(
                "input monitoring fix",
                format!(
                    "{} > Input Monitoring, switch this application on, {INPUT_MONITORING_NOTE}",
                    privacy_pane()
                ),
            );
        }
        Listen::Unknown => {
            push(
                "input monitoring",
                "unknown — macOS has not been asked (the Permissions page's button asks)".into(),
            );
            push("input monitoring symptom", INPUT_MONITORING_SYMPTOM.into());
        }
    }
    // The posting side of the same check, and what builds before 2026-09-27 logged as `input
    // monitoring` — their request number was this one's. It tracked Accessibility exactly across
    // three sessions (unknown before the grant, granted right after, denied again after a
    // rebuild), which is where "Input Monitoring follows Accessibility" came from. Kept as a line
    // of its own so that old logs and new ones can be compared.
    push(
        "hid post events",
        format!(
            "{} (IOHIDCheckAccess for posting events, the Accessibility side — what earlier logs \
             called input monitoring)",
            Listen::from_access(Some(IOHIDCheckAccess(IOHIDRequestType::PostEvent).0)).word()
        ),
    );

    // --- who we are, and who else is listening ---------------------------------------
    push(
        "voiceover",
        if voiceover_running() {
            "running".into()
        } else {
            "not running (speech goes to the built-in engine)".into()
        },
    );
    // Asked without prompting: a session header must not put a dialog on screen.
    let automation = voiceover_automation();
    push("voiceover automation", automation.as_str().into());
    if automation == Automation::Refused {
        // Only when it is actually wrong. "Not asked yet" is the ordinary state and needs no
        // instruction — switching the setting on is what asks.
        push(
            "voiceover automation fix",
            format!(
                "{} > Automation, then tick this application's VoiceOver entry. No restart \
                 needed — the transport picks it up on the next line it says.",
                privacy_pane()
            ),
        );
    }
    // VoiceOver's own permission, next to the system's: "Allow VoiceOver to be controlled with
    // AppleScript". Unticked, VoiceOver drops every line handed to it without a word, and no
    // system call reports it — so it is read here from the file VoiceOver Utility writes and the
    // preference it sets (`speech/vo_script.rs`), asking VoiceOver nothing, and from those alone.
    // VoiceOver's own answer, asked on the speech thread when the setting is on, follows in the
    // `[speech]` lines.
    push("voiceover applescript", crate::speech::voiceover_applescript_env());

    // macOS records every permission above against the *bundle*, not the executable path.
    // A build run as a bare binary out of `target/` is a different identity from the same
    // build inside an .app, so grants do not carry across and the tester sees a permission
    // they know they granted reported as missing. This is the line that explains it.
    let me = NSRunningApplication::currentApplication();
    let bundle_id = me.bundleIdentifier().map(|s| s.to_string());
    let bundle_path = me.bundleURL().and_then(|u| u.path()).map(|s| s.to_string());
    // RUNNING FROM A RANDOMISED READ-ONLY COPY, which on its own looks like nothing at all.
    //
    // macOS "translocates" a quarantined application launched from the Finder: it mounts a
    // copy of the .app ALONE at a random path under /private/var/folders, so the folder around
    // the copy contains no `modules` directory, no settings and no log. `portable::base_dir`
    // asks the Security framework for the original and uses the folder around that instead,
    // before anything reads it; this line says whether it had to, and what it found. Said
    // here as well as in the log's header because this block is what a tester copies out.
    if let Some(t) = crate::portable::translocation().report() {
        push("translocated", t);
    }
    match (&bundle_id, &bundle_path) {
        (Some(id), Some(path)) => push("bundle", format!("{id} at {path}")),
        (Some(id), None) => push("bundle", format!("{id} (no bundle path)")),
        (None, _) => {
            push("bundle", "none — running as a bare binary, not from an .app".into());
            push(
                "bundle note",
                "permissions are recorded against the bundle identity, so grants given to \
                 the packaged application do not apply to this process and vice versa"
                    .into(),
            );
        }
    }
    // Who macOS thinks we are, in the terms it records permissions against.
    //
    // This is the line that settles an argument the log could not otherwise settle: a
    // permission the user has clearly granted, against an application that clearly still
    // cannot use it. TCC stores a requirement describing the signed identity, so a build
    // signed differently from the one that was granted is a different application to it —
    // while still appearing, ticked, in the settings list. Two runs of this line say
    // immediately whether the identity moved.
    if let Some(path) = bundle_path.as_ref() {
        for (k, v) in signature_of(path) {
            push(&k, v);
        }
    }
    push(
        "executable",
        me.executableURL()
            .and_then(|u| u.path())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "?".into()),
    );
    // Whom macOS asks about. Started from a terminal rather than opened, the permissions above
    // can be the terminal's (its "responsible" application) and the lists can name the terminal
    // instead — a reason for "it is not in the list" that the rest of this block cannot see.
    let ppid = unsafe { libc::getppid() };
    push(
        "launched by",
        super::enrol::launch_line(ppid, process_name(ppid).as_deref()),
    );

    out
}

/// Is this process trusted for Accessibility, optionally raising the system prompt.
///
/// Two entry points rather than one with a flag, because the plain check is the one that
/// runs during startup and it must be impossible for it to put a dialog on screen: the
/// options form with the prompt set to false does the same job, but the difference between
/// prompting and not would then be one boolean deep in a call chain.
///
/// For the prompting form, the option dictionary has to be handed over as the *opaque*
/// `CFDictionary` — a typed one does not coerce — and `kAXTrustedCheckOptionPrompt` is one
/// of the very few `kAX*` statics that survives into these bindings at all.
fn is_trusted(prompt: bool) -> bool {
    let trusted = if prompt {
        let key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
        let value: &CFBoolean = CFBoolean::new(true);
        let opts: CFRetained<CFDictionary<CFString, CFBoolean>> =
            CFDictionary::from_slices(&[key], &[value]);
        unsafe { AXIsProcessTrustedWithOptions(Some(opts.as_opaque())) }
    } else {
        unsafe { AXIsProcessTrusted() }
    };
    crate::logging::trace("macos", || format!("AX trusted (prompt={prompt}) -> {trusted}"));
    trusted
}

/// Ids of the displays currently drawable, in the order CoreGraphics reports them.
fn active_displays() -> Vec<CGDirectDisplayID> {
    // Sixteen is past any real setup; the call fills what fits and reports what it would
    // have needed, so a bigger rig still gets sixteen lines instead of none.
    let mut ids = [0 as CGDirectDisplayID; 16];
    let mut count: u32 = 0;
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    if err != CGError::Success {
        crate::logging::line(
            "macos",
            &format!("CGGetActiveDisplayList failed ({err:?}); falling back to the main display"),
        );
        return vec![CGMainDisplayID()];
    }
    let count = (count as usize).min(ids.len());
    ids[..count].to_vec()
}

/// AppKit's `backingScaleFactor` for the display at the origin, as a cross-check on the
/// figure derived from the display mode.
fn backing_scale(cg_primary: Option<f64>) -> String {
    // `NSScreen` is main-thread-only and says so through the marker type. The environment
    // report is called from the host's startup, which is that thread — but proving it costs
    // nothing and the alternative to proving it is undefined behaviour.
    let Some(mtm) = MainThreadMarker::new() else {
        crate::logging::line(
            "macos",
            "environment report ran off the main thread; NSScreen not asked",
        );
        return "not asked (report did not run on the main thread)".into();
    };
    let screens = NSScreen::screens(mtm);
    // The screen whose frame origin is (0,0) is the primary one in AppKit's bottom-left
    // space just as it is in CoreGraphics' top-left space — the two origins coincide there
    // and only there, which is what makes it the right screen to compare against.
    let mut primary = None;
    let mut all: Vec<String> = Vec::new();
    for s in screens.iter() {
        let f = s.frame();
        let scale = s.backingScaleFactor();
        all.push(format!("{scale:.2}"));
        if f.origin.x == 0.0 && f.origin.y == 0.0 {
            primary = Some(scale);
        }
    }
    let all = all.join(", ");
    match (primary, cg_primary) {
        (Some(ns), Some(cg)) if (ns - cg).abs() > 0.01 => {
            crate::logging::line(
                "macos",
                &format!(
                    "backing scale disagreement: NSScreen {ns:.2} vs display mode {cg:.2} — \
                     the primary display is probably in a scaled mode, and capture \
                     downsampling should be trusting NSScreen"
                ),
            );
            format!("MISMATCH: NSScreen {ns:.2} vs display mode {cg:.2} (all screens: {all})")
        }
        (Some(ns), _) => format!("{ns:.2} on the primary display (all screens: {all})"),
        (None, _) => format!("no screen at the origin (all screens: {all})"),
    }
}

/// Whether VoiceOver is up.
///
/// By bundle id rather than by walking every running application: this is a single lookup
/// against the workspace's index instead of a list scan, and the id is stable across every
/// macOS version we care about.
pub(super) fn voiceover_running() -> bool {
    let id = NSString::from_str("com.apple.VoiceOver");
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&id).is_empty()
}

// `enrol::Listen::from_access` reads IOKit's access codes as plain numbers, so that it can be
// tested where it is written; these hold those numbers to the binding's own.
const _: () = assert!(IOHIDAccessType::Granted.0 == 0);
const _: () = assert!(IOHIDAccessType::Denied.0 == 1);
const _: () = assert!(IOHIDAccessType::Unknown.0 == 2);

/// Said whenever Input Monitoring is not confirmed granted. The distinction matters: the
/// hotkeys keep working without it, so a tester will report "the shortcut works but the
/// overlay's own keys leak through", which reads like a bug in the key handling.
const INPUT_MONITORING_SYMPTOM: &str =
    "if it is needed and missing, registered hotkeys still fire (Carbon needs no permission), but \
     captured keys reach the application underneath instead of the overlay";

/// Input Monitoring's own state: whether this process may LISTEN to input events, as
/// `IOHIDCheckAccess` answers for `kIOHIDRequestTypeListenEvent`.
///
/// Through `objc2-io-kit`'s binding and its named request types. It used to be looked up by
/// name and called with a hand-written 0 — which is `kIOHIDRequestTypePostEvent`, the posting
/// side, governed by Accessibility — so for every build before 2026-09-27 the log's `input
/// monitoring` line was Accessibility read a second way. Both calls exist from macOS 10.15.
fn listen_state() -> Listen {
    let access = IOHIDCheckAccess(IOHIDRequestType::ListenEvent);
    crate::logging::trace("macos", || format!("IOHIDCheckAccess(listen) -> {}", access.0));
    Listen::from_access(Some(access.0))
}

/// A string-valued sysctl, or `None` if it is not there.
pub(super) fn sysctl_string(name: &str) -> Option<String> {
    let key = CString::new(name).ok()?;
    let mut len: usize = 0;
    // First call sizes the buffer; a failure here is the normal answer for "no such name".
    let rc = unsafe {
        libc::sysctlbyname(key.as_ptr(), std::ptr::null_mut(), &mut len, std::ptr::null_mut(), 0)
    };
    if rc != 0 || len == 0 || len > 512 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let rc = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    // These come back NUL-terminated, and the length includes the terminator.
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..end]).into_owned())
}

/// An integer-valued sysctl, or `None` if it is not there.
pub(super) fn sysctl_i32(name: &str) -> Option<i32> {
    let key = CString::new(name).ok()?;
    let mut value: i32 = 0;
    let mut len = std::mem::size_of::<i32>();
    let rc = unsafe {
        libc::sysctlbyname(
            key.as_ptr(),
            (&mut value as *mut i32).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(value)
}
