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
//! saying which pane to open and that the application has to be restarted afterwards.

use std::ffi::CString;
use std::sync::Once;

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
    CGRequestScreenCaptureAccess,
};
use objc2_foundation::{MainThreadMarker, NSProcessInfo, NSString};

/// The one fact about macOS permissions that costs everybody an hour the first time, and
/// which a blind tester has no way of discovering: the switch takes effect for the *next*
/// process, not this one. Repeated verbatim next to every permission that is missing,
/// because a tester greps for the permission name, not for a note further up the file.
const RESTART_NOTE: &str =
    "then quit this application completely and open it again — macOS only hands a newly \
     granted permission to a process that started after the grant";

/// Asks for Accessibility once, with the system prompt.
pub fn request_accessibility_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // The prompting form is the only way an application can raise the Accessibility
        // dialog at all; there is no API that grants it. It also answers the question, so
        // this doubles as the first check. Note that it returns the state as it was *at
        // process start*: a user who grants it in response to this very prompt still gets
        // `false` here, which is exactly why the restart note has to be logged either way.
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
        crate::logging::line("macos", &format!("accessibility: {RESTART_NOTE}"));
    });
}

/// Asks for Screen Recording once, with the system prompt.
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
/// `CGRequestScreenCaptureAccess` is the one that both prompts and enrols. It answers with
/// the state as it was at process start, so a user who grants it in response to this very
/// prompt still sees `false` — which is why the restart note is logged either way.
pub fn request_screen_recording_once() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if CGPreflightScreenCaptureAccess() {
            crate::logging::line("macos", "screen recording: granted");
            return;
        }
        if CGRequestScreenCaptureAccess() {
            crate::logging::line("macos", "screen recording: granted after asking");
            return;
        }
        crate::logging::line(
            "macos",
            "screen recording: NOT granted. Asking is also what puts an application into \
             that list at all, so it is done even when the answer is already known.",
        );
        crate::logging::line(
            "macos",
            "screen recording: without it a capture does not fail. It comes back as a \
             picture of the wallpaper, so every image search and every OCR read reads empty \
             with nothing anywhere to explain why.",
        );
        if prompt_is_unreliable() {
            // The honest instruction for THIS machine. Waiting for a dialog that is not
            // going to appear is worse than being told to go and add it by hand, and
            // somebody who cannot see the screen has no way to tell "no prompt yet" from
            // "no prompt ever".
            for msg in [
                "screen recording: on this version of macOS the request often raises no \
                 dialog and adds nothing to the list. That is a known macOS behaviour and \
                 not a fault in this application.",
                "screen recording: add it by hand instead. Open the pane named on the next \
                 line, unlock the padlock, press the + button, and choose \
                 AutomationPlatform.app",
            ] {
                crate::logging::line("macos", msg);
            }
        }
        crate::logging::line(
            "macos",
            &format!(
                "screen recording: {} > Screen Recording (Screen & System Audio Recording from \
                 macOS 15), {RESTART_NOTE}",
                privacy_pane()
            ),
        );
    });
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

/// Whether this macOS is old enough that the screen-recording prompt cannot be relied on.
///
/// Measured rather than assumed. On 12.7.6 the request raised no dialog and added nothing to
/// the list, and the tester got past it by adding the application by hand with the + button;
/// the same shape of problem was reported independently on Monterey for a different
/// permission entirely, which points at the OS version rather than at anything this
/// application does.
fn prompt_is_unreliable() -> bool {
    NSProcessInfo::processInfo().operatingSystemVersion().majorVersion < 13
}

use crate::backend::{Grant, Permission};

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
            anchor: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
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
                      If this application is not in that list yet, grant Accessibility first: \
                      it appears in the Screen Recording list only afterwards. From macOS 15 \
                      on, System Settings calls that list Screen & System Audio Recording.",
            anchor: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            can_ask: true,
            blocking: true,
        },
        Permission {
            name: "Input Monitoring",
            state: match input_monitoring() {
                Some(0) => Grant::Granted,
                Some(ACCESS_DENIED) => Grant::Missing,
                _ => Grant::Unknown,
            },
            without: "Keys an overlay has claimed reach the plugin instead of the overlay. It \
                      usually follows the Accessibility grant without being asked for \
                      separately, so it is only worth opening if it is still missing after that.",
            anchor: "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            can_ask: false,
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
                      it.",
            anchor: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
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

/// Asks for the one permission whose dialog we can raise from a button.
///
/// Accessibility and Screen Recording have system prompts; Input Monitoring has none we can
/// trigger, and Automation is asked for by ticking the setting that needs it. Anything else
/// returns false and the caller falls back to opening the pane.
pub fn ask_for(name: &str) -> bool {
    match name {
        "Accessibility" => {
            request_accessibility_once();
            true
        }
        "Screen Recording" => {
            request_screen_recording_once();
            true
        }
        _ => false,
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
    std::thread::spawn(move || {
        let after = ask_tcc(true);
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
            format!("{} > Accessibility, {RESTART_NOTE}", privacy_pane()),
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

    // Reported, not asked for. Observed across three sessions on one machine: it read
    // `unknown` before Accessibility was granted, `granted` immediately afterwards, and
    // denied again once a rebuild invalidated the Accessibility grant — tracking it exactly,
    // without the user ever opening that pane. A process trusted for Accessibility is
    // allowed to listen to events, so this usually needs no grant of its own. It is listed
    // because it can be switched off independently, and because if it ever disagrees with
    // Accessibility that disagreement is the finding.
    match input_monitoring() {
        Some(ACCESS_GRANTED) => push(
            "input monitoring",
            "granted (it normally follows the Accessibility grant rather than needing one of its own)"
                .into(),
        ),
        Some(ACCESS_DENIED) => {
            push(
                "input monitoring",
                "NOT granted — the event tap cannot see or swallow keys. Usually this resolves itself when Accessibility is granted; only if it does not is the pane below worth opening."
                    .into(),
            );
            push("input monitoring symptom", INPUT_MONITORING_SYMPTOM.into());
            push(
                "input monitoring fix",
                format!("{} > Input Monitoring, {RESTART_NOTE}", privacy_pane()),
            );
        }
        other => {
            // `kIOHIDAccessTypeUnknown`, or no `IOHIDCheckAccess` to ask at all. Either way
            // the honest answer is that we do not know, and the symptom is the useful part.
            push(
                "input monitoring",
                match other {
                    Some(code) => format!("unknown (IOHIDCheckAccess returned {code})"),
                    None => "unknown (IOHIDCheckAccess not available on this system)".to_string(),
                },
            );
            push("input monitoring symptom", INPUT_MONITORING_SYMPTOM.into());
        }
    }

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

    // macOS records every permission above against the *bundle*, not the executable path.
    // A build run as a bare binary out of `target/` is a different identity from the same
    // build inside an .app, so grants do not carry across and the tester sees a permission
    // they know they granted reported as missing. This is the line that explains it.
    let me = NSRunningApplication::currentApplication();
    let bundle_id = me.bundleIdentifier().map(|s| s.to_string());
    let bundle_path = me.bundleURL().and_then(|u| u.path()).map(|s| s.to_string());
    // RUNNING FROM A RANDOMISED READ-ONLY COPY, which looks like nothing at all.
    //
    // macOS "translocates" a quarantined application launched from the Finder: it mounts a
    // copy of the .app ALONE at a random path under /private/var/folders, so the folder the
    // application thinks it is in contains no `modules` directory, no settings and no log.
    // Everything then behaves correctly and uselessly — the manager starts with nothing
    // loaded, because that is right for somebody installing their first module, and the log
    // goes to Application Support because the app folder is not writable. To a tester it
    // reads as "every overlay is broken", and it is the likeliest way a remote session is
    // lost to something that is not a bug. The path is the only evidence, so here it is.
    if bundle_path.as_deref().is_some_and(|p| p.contains("/AppTranslocation/")) {
        push(
            "translocated",
            "YES — macOS is running a read-only copy of the .app from a random folder, so \
             the modules beside the original are not there and none will load. Quit, run \
             `xattr -dr com.apple.quarantine` on the .app (see README.txt beside it), and \
             open it again."
                .into(),
        );
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

/// `kIOHIDRequestTypeListenEvent` — asking about *reading* input, which is what the event
/// tap does. The posting side (`kIOHIDRequestTypePostEvent`, 1) is gated by Accessibility.
const HID_REQUEST_LISTEN: u32 = 0;
/// `kIOHIDAccessTypeGranted` / `kIOHIDAccessTypeDenied`; 2 is the enum's own "unknown".
const ACCESS_GRANTED: u32 = 0;
const ACCESS_DENIED: u32 = 1;

/// Said whenever Input Monitoring is not confirmed granted. The distinction matters: the
/// hotkeys keep working without it, so a tester will report "the shortcut works but the
/// overlay's own keys leak through", which reads like a bug in the key handling.
const INPUT_MONITORING_SYMPTOM: &str =
    "registered hotkeys still fire (Carbon needs no permission), but captured keys reach \
     the application underneath instead of the overlay";

/// Asks IOKit whether this process may listen to input events, if it can be asked at all.
///
/// `IOHIDCheckAccess` is the only public way to put the question, and no crate in the objc2
/// family binds it. Declaring it in an `extern` block would be the obvious move and is the
/// wrong one: a Windows `cargo check` never links, so a mistyped symbol would surface as a
/// link failure of the whole application on the tester's first build, taking every other
/// permission report down with it. Looking it up by name costs one `dlopen` in a function
/// that runs once, and degrades to an honest "unknown".
fn input_monitoring() -> Option<u32> {
    let path = CString::new("/System/Library/Frameworks/IOKit.framework/IOKit").ok()?;
    let name = CString::new("IOHIDCheckAccess").ok()?;
    // The handle is deliberately never closed: the resolved function pointer stays valid
    // only while the image is loaded, and IOKit is loaded for the life of the process
    // anyway, so closing buys nothing and unloading under a live pointer is a crash.
    let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_LAZY) };
    if handle.is_null() {
        crate::logging::line("macos", "IOKit.framework did not open; Input Monitoring unknown");
        return None;
    }
    let sym = unsafe { libc::dlsym(handle, name.as_ptr()) };
    if sym.is_null() {
        crate::logging::line(
            "macos",
            "IOHIDCheckAccess not found in IOKit (pre-10.15?); Input Monitoring unknown",
        );
        return None;
    }
    // SAFETY: IOHIDCheckAccess is `IOHIDAccessType IOHIDCheckAccess(IOHIDRequestType)`, and
    // both of those are `CF_ENUM(uint32_t)`, so this is a u32 -> u32 C call.
    let f: unsafe extern "C" fn(u32) -> u32 = unsafe { std::mem::transmute(sym) };
    let access = unsafe { f(HID_REQUEST_LISTEN) };
    crate::logging::trace("macos", || format!("IOHIDCheckAccess(listen) -> {access}"));
    Some(access)
}

/// A string-valued sysctl, or `None` if it is not there.
fn sysctl_string(name: &str) -> Option<String> {
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
fn sysctl_i32(name: &str) -> Option<i32> {
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
