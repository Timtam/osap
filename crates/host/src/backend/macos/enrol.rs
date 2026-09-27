//! Which requests the application makes for Screen Recording and Input Monitoring, in which
//! order, and what each answer is called in the log — decided here, without a Mac.
//!
//! Its own file for the reason `keys.rs` and `budget.rs` have one: nothing in it touches macOS,
//! so `backend/mod.rs` borrows it by `#[path]` under `cfg(test)` and its rules run on the machine
//! they are written on. The calls themselves are in `perm.rs` (the order) and `capture.rs` (the
//! two capture requests).
//!
//! **Why there is more than one request.** An application appears in a privacy list only once it
//! has asked, and `CGRequestScreenCaptureAccess` is the documented way to ask for Screen
//! Recording — but on macOS 12.7.6 it was measured to raise no dialog and add no entry, which
//! left a blind tester adding the application by hand. What TCC treats as a request is a capture
//! itself, so a second, different request exists: ScreenCaptureKit's list of what can be
//! captured (the call Apple's own sample makes first, and whose first run it says prompts), and,
//! where ScreenCaptureKit is missing or silent, or on macOS 12, one point through the older
//! capture functions. Input Monitoring has the same pair: `CGRequestListenEventAccess`, and
//! IOKit's `IOHIDRequestAccess` for listening.
//!
//! **Why they are a ladder rather than a burst.** Every request can put a system dialog on the
//! screen, and a dialog raised on top of another — or a settings pane opened on top of one —
//! leaves somebody who cannot see the screen hearing one of them and lost in the other. So each
//! rung is climbed at most once per process, one per press of the permission's button, and the
//! pane is opened only by a press that asks nothing: a second press is the user saying "no
//! dialog came". The automatic request (at start, or when Accessibility is granted) climbs only
//! the first rung — except on macOS 12, where the first rung is known not to be enough.

/// `SCStreamErrorUserDeclined`, ScreenCaptureKit's answer while the permission is missing.
pub(crate) const SCK_USER_DECLINED: isize = -3801;

/// What ScreenCaptureKit said when it was asked for the displays and windows it could capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SckAnswer {
    /// No `SCShareableContent` on this macOS — older than 12.3 — so nothing was asked.
    Absent,
    /// Asked, and no answer came within the wait.
    TimedOut { ms: u128 },
    /// The content came back, which ScreenCaptureKit normally does only for a process allowed
    /// to capture.
    Content { displays: usize, windows: usize },
    /// An error came back instead.
    Refused { domain: String, code: isize, text: String },
}

/// What the one-point capture through the older functions came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyAnswer {
    /// Neither `CGWindowListCreateImage` nor `CGDisplayCreateImageForRect` is on this macOS.
    Absent,
    /// Called, and an image came back.
    Image { via: &'static str },
    /// Called, and nothing came back.
    NoImage { via: &'static str },
}

/// How far this process's requests for one permission have got. Climbed one rung at a time,
/// never down, at most once per rung per process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub(crate) enum Rung {
    /// Nothing asked yet.
    Nothing = 0,
    /// The documented request has been made.
    Documented = 1,
    /// The second, different request has been made as well: nothing is left to ask.
    All = 2,
}

impl Rung {
    /// Back from the number an atomic keeps it as. Anything past the top is the top.
    pub(crate) fn from_u8(n: u8) -> Rung {
        match n {
            0 => Rung::Nothing,
            1 => Rung::Documented,
            _ => Rung::All,
        }
    }
}

/// What a Screen Recording request makes now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenStep {
    /// `CGRequestScreenCaptureAccess`, and with `captures_too` the capture requests after it
    /// (each after a pause).
    Documented { captures_too: bool },
    /// The capture requests alone: ScreenCaptureKit's list, and the older functions where
    /// [`legacy_reason`] says.
    Captures,
    /// Nothing. For the button: everything has been asked in this process, so the pane opens.
    /// For the automatic request: something has been asked already.
    Nothing,
}

/// The next Screen Recording request, from how far this process has got, who is asking (the
/// button, or the automatic request at start or when Accessibility is granted), and the macOS
/// major version.
///
/// macOS 12 gets both kinds at once, from either: the documented request alone was measured
/// there to raise no dialog and add no entry. Everywhere else the automatic request makes only
/// the documented one — the one that is known to work there, and the one that cannot stack a
/// second dialog on its own — and the capture requests wait for the button, pressed again
/// because no dialog came.
pub(crate) fn screen_step(done: Rung, button: bool, macos_major: u64) -> ScreenStep {
    match (done, button) {
        (Rung::Nothing, _) => ScreenStep::Documented { captures_too: macos_major < 13 },
        (Rung::Documented, true) => ScreenStep::Captures,
        _ => ScreenStep::Nothing,
    }
}

/// The rung a Screen Recording step leaves this process on.
pub(crate) fn screen_after(step: ScreenStep, done: Rung) -> Rung {
    match step {
        ScreenStep::Documented { captures_too: true } | ScreenStep::Captures => Rung::All,
        ScreenStep::Documented { captures_too: false } => Rung::Documented,
        ScreenStep::Nothing => done,
    }
}

/// Why the older capture functions are asked as well, or `None` when they are not.
///
/// Asked where ScreenCaptureKit could not be — it is not there, or it did not answer — and on
/// macOS 12, the version where the documented request was measured to add nothing, whatever
/// ScreenCaptureKit said. Not asked when ScreenCaptureKit handed the content over: there is
/// nothing left to request. Not asked on macOS 13 or later once ScreenCaptureKit has answered:
/// its question was a capture request already, and macOS 15 added alerts of its own for the
/// older functions (macOS 14 for `CGDisplayStream`, which is not used here), so asking them too
/// could put a second dialog about the same thing on the screen of somebody who cannot see which
/// one is which.
pub(crate) fn legacy_reason(sck: &SckAnswer, macos_major: u64) -> Option<&'static str> {
    match sck {
        SckAnswer::Absent => Some("ScreenCaptureKit is not on this macOS"),
        SckAnswer::TimedOut { .. } => Some("ScreenCaptureKit did not answer"),
        SckAnswer::Content { .. } => None,
        SckAnswer::Refused { .. } if macos_major < 13 => Some(
            "on macOS 12 the plain request was measured to add nothing to the list, so both \
             kinds of capture request are made",
        ),
        SckAnswer::Refused { .. } => None,
    }
}

/// Input Monitoring as `IOHIDCheckAccess` reads it for listening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Listen {
    Granted,
    /// Refused in a dialog or switched off: the entry is in the list.
    Denied,
    /// Never asked — or the question could not be put.
    Unknown,
}

impl Listen {
    /// From `IOHIDAccessType`: 0 granted, 1 denied, 2 unknown; `None` when it could not be asked.
    pub(crate) fn from_access(code: Option<u32>) -> Listen {
        match code {
            Some(0) => Listen::Granted,
            Some(1) => Listen::Denied,
            _ => Listen::Unknown,
        }
    }

    pub(crate) fn word(self) -> &'static str {
        match self {
            Listen::Granted => "granted",
            Listen::Denied => "denied",
            Listen::Unknown => "unknown",
        }
    }
}

/// What a press of Input Monitoring's button makes now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListenStep {
    /// Accessibility is missing. Nothing is asked: the key tap is the kind Accessibility
    /// governs, and its dialog may be on screen. The pane opens and the page says so.
    AccessibilityFirst,
    /// Granted already: the pane opens.
    Granted,
    /// Denied: the entry is in the list, switched off, and switching it on is what is left. The
    /// pane opens; nothing is asked.
    InListOff,
    /// `CGRequestListenEventAccess`.
    Documented,
    /// `IOHIDRequestAccess` for listening, the second way.
    Hid,
    /// Both asked in this process and still unknown: the pane opens, for the + button.
    Exhausted,
}

/// The next Input Monitoring request. Unlike Screen Recording's, its state can be read, so
/// "the entry is there" is known rather than guessed: `Denied` means it is.
pub(crate) fn listen_step(trusted: bool, state: Listen, done: Rung) -> ListenStep {
    if !trusted {
        return ListenStep::AccessibilityFirst;
    }
    match (state, done) {
        (Listen::Granted, _) => ListenStep::Granted,
        (Listen::Denied, _) => ListenStep::InListOff,
        (Listen::Unknown, Rung::Nothing) => ListenStep::Documented,
        (Listen::Unknown, Rung::Documented) => ListenStep::Hid,
        (Listen::Unknown, Rung::All) => ListenStep::Exhausted,
    }
}

/// The rung an Input Monitoring step leaves this process on.
pub(crate) fn listen_after(step: ListenStep, done: Rung) -> Rung {
    match step {
        ListenStep::Documented => Rung::Documented,
        ListenStep::Hid => Rung::All,
        _ => done,
    }
}

/// What the state before and after an Input Monitoring request says, for the end of its line.
/// Unknown before and denied after is the one visible sign that an entry now exists.
pub(crate) fn listen_change(before: Listen, after: Listen) -> &'static str {
    match (before, after) {
        (Listen::Unknown, Listen::Denied) => {
            " — unknown to denied: the application is in the Input Monitoring list now, switched \
             off, and a dialog may be on screen"
        }
        (_, Listen::Granted) => " — granted",
        (_, Listen::Unknown) => {
            " — still unknown: either a dialog is on screen and not answered yet, or macOS \
             registered nothing; if no dialog came, the button asks a second way"
        }
        _ => "",
    }
}

/// The log line for ScreenCaptureKit's answer. Every line starts `screen recording:`, the prefix
/// a tester's log is searched by.
pub(crate) fn sck_line(sck: &SckAnswer) -> String {
    match sck {
        SckAnswer::Absent => "screen recording: ScreenCaptureKit is not on this macOS (it arrived in \
                              12.3), so it was not asked"
            .to_string(),
        SckAnswer::TimedOut { ms } => format!(
            "screen recording: asked ScreenCaptureKit for what can be captured, and it did not \
             answer within {ms} ms; not waited for further"
        ),
        SckAnswer::Content { displays, windows } => format!(
            "screen recording: asked ScreenCaptureKit for what can be captured, and it handed over \
             {displays} display(s) and {windows} window(s) — which it normally does only for a \
             process allowed to capture, so the 'NOT granted' above may be the answer the system \
             gave this process when it started, kept until the application is started again"
        ),
        SckAnswer::Refused { domain, code, text } if *code == SCK_USER_DECLINED => format!(
            "screen recording: asked ScreenCaptureKit for what can be captured, and it answered \
             'declined' ({domain} {code}: \"{text}\") — its word for not permitted, given whether \
             or not a dialog is on screen. The asking is what counts: it is a capture request of \
             its own."
        ),
        SckAnswer::Refused { domain, code, text } => format!(
            "screen recording: asked ScreenCaptureKit for what can be captured, and it answered \
             {domain} {code}: \"{text}\""
        ),
    }
}

/// The log line for the one-point capture, with the reason it was made.
pub(crate) fn legacy_line(answer: LegacyAnswer, why: &str) -> String {
    match answer {
        LegacyAnswer::Absent => format!(
            "screen recording: {why}, and the older capture functions are gone too, so there was \
             nothing further to ask"
        ),
        LegacyAnswer::Image { via } => format!(
            "screen recording: also captured one point through {via} ({why}). An image came back, \
             as it does without the permission too (a picture of the desktop), so this line says \
             the request was made, not how it was answered."
        ),
        LegacyAnswer::NoImage { via } => format!(
            "screen recording: also asked {via} for one point ({why}); it returned no image"
        ),
    }
}

/// The last Screen Recording line of a request, or of a press that asked nothing: what is left
/// to do if the application is not in the list, or is in it and still not granted.
///
/// `more_to_ask`: the button has a second kind of request left in this process, so pressing it
/// again comes before adding the entry by hand. `pane` names the settings as this macOS calls
/// them, `bundle_id` goes into the reset command, and `monterey_note` is said on the version
/// where the documented request was measured to add nothing.
pub(crate) fn fallback_line(more_to_ask: bool, pane: &str, bundle_id: &str, monterey_note: bool) -> String {
    let first = if more_to_ask {
        "if a dialog came up, its button that opens the settings leads to the list. If none did, \
         or the application is not in the list, press 'Open the Screen Recording settings' on the \
         Permissions page: it asks a second way. Failing that"
    } else {
        "if this application is not in the list"
    };
    let monterey = if monterey_note {
        " On macOS 12 the documented request was measured to add nothing — that is macOS \
         behaviour, not a fault here."
    } else {
        ""
    };
    format!(
        "screen recording: {first}: open {pane} > Screen Recording, press the + button under the \
         list, choose AutomationPlatform.app, switch it on, then quit and reopen the application. \
         If it IS in the list and switched on, and this run started after that, the entry belongs \
         to an earlier build (compare `signature cdhash` between two logs): quit, run `tccutil \
         reset ScreenCapture {bundle_id}` in Terminal, and open the application again.{monterey}"
    )
}

/// Who started this process, in the terms the permission dialogs care about.
///
/// launchd is the parent of anything opened with `open` or from the Finder, and then macOS asks
/// about this application itself. Anything else started it — a shell, usually — and then macOS
/// counts permission requests against the application RESPONSIBLE for it, which for a program
/// started in a terminal is that terminal: the dialog and the list entry can name the terminal,
/// and a grant given to this application does not reach this process.
pub(crate) fn launch_line(ppid: i32, parent: Option<&str>) -> String {
    if ppid == 1 {
        return "launchd — opened with `open` or from the Finder, so the permission dialogs and \
                lists are about this application itself"
            .to_string();
    }
    let who = parent.filter(|p| !p.is_empty()).unwrap_or("another process");
    format!(
        "{who} (pid {ppid}), not launchd — started from it rather than opened with `open` or from \
         the Finder. macOS counts permission requests against the application responsible for a \
         program, which for one started in a terminal is that terminal, so the dialogs and the \
         list entries may name it instead of this application. Quit and open the .app with \
         `open` or from the Finder."
    )
}

/// Whether the launch was one the dialogs are about this application for.
pub(crate) fn launched_by_launchd(ppid: i32) -> bool {
    ppid == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declined() -> SckAnswer {
        SckAnswer::Refused {
            domain: "com.apple.ScreenCaptureKit.SCStreamErrorDomain".into(),
            code: SCK_USER_DECLINED,
            text: "The user declined TCCs for application, window, display capture".into(),
        }
    }

    #[test]
    fn the_automatic_request_is_the_documented_one_alone_except_on_macos_12() {
        for major in [13, 14, 15, 26] {
            let step = screen_step(Rung::Nothing, false, major);
            assert_eq!(step, ScreenStep::Documented { captures_too: false }, "on {major}");
            assert_eq!(screen_after(step, Rung::Nothing), Rung::Documented);
        }
        let step = screen_step(Rung::Nothing, false, 12);
        assert_eq!(step, ScreenStep::Documented { captures_too: true });
        assert_eq!(screen_after(step, Rung::Nothing), Rung::All);
    }

    #[test]
    fn the_automatic_request_never_asks_twice() {
        for major in [12, 13, 15, 26] {
            for done in [Rung::Documented, Rung::All] {
                assert_eq!(screen_step(done, false, major), ScreenStep::Nothing, "{done:?} on {major}");
            }
        }
    }

    #[test]
    fn each_press_climbs_one_rung_and_only_the_last_opens_the_pane() {
        // The newer versions: documented, then the captures, then the pane.
        let mut done = Rung::Nothing;
        let mut steps = Vec::new();
        for _ in 0..4 {
            let step = screen_step(done, true, 15);
            done = screen_after(step, done);
            steps.push(step);
        }
        assert_eq!(
            steps,
            [
                ScreenStep::Documented { captures_too: false },
                ScreenStep::Captures,
                ScreenStep::Nothing,
                ScreenStep::Nothing
            ]
        );
        // After the automatic request, the first press is the second way of asking.
        assert_eq!(screen_step(Rung::Documented, true, 14), ScreenStep::Captures);
        // macOS 12 asks everything with the first press, so the second opens the pane.
        let first = screen_step(Rung::Nothing, true, 12);
        assert_eq!(first, ScreenStep::Documented { captures_too: true });
        assert_eq!(screen_step(screen_after(first, Rung::Nothing), true, 12), ScreenStep::Nothing);
    }

    #[test]
    fn a_rung_survives_its_trip_through_an_atomic() {
        for r in [Rung::Nothing, Rung::Documented, Rung::All] {
            assert_eq!(Rung::from_u8(r as u8), r);
        }
        assert_eq!(Rung::from_u8(200), Rung::All);
    }

    #[test]
    fn the_older_functions_stand_in_where_screencapturekit_could_not_ask() {
        for major in [11, 12, 13, 14, 15, 26] {
            assert!(legacy_reason(&SckAnswer::Absent, major).is_some(), "absent on {major}");
            assert!(legacy_reason(&SckAnswer::TimedOut { ms: 5000 }, major).is_some(), "silent on {major}");
        }
    }

    #[test]
    fn macos_12_gets_both_capture_requests_and_newer_ones_only_one() {
        assert!(legacy_reason(&declined(), 12).is_some());
        for major in [13, 14, 15, 26] {
            assert_eq!(legacy_reason(&declined(), major), None, "a second dialog on {major}");
        }
        let other = SckAnswer::Refused { domain: "x".into(), code: -3802, text: "y".into() };
        assert!(legacy_reason(&other, 12).is_some());
        assert_eq!(legacy_reason(&other, 15), None);
    }

    #[test]
    fn content_handed_over_asks_nothing_more() {
        for major in [12, 13, 15, 26] {
            assert_eq!(legacy_reason(&SckAnswer::Content { displays: 1, windows: 9 }, major), None);
        }
    }

    #[test]
    fn input_monitoring_is_not_asked_without_accessibility() {
        for state in [Listen::Granted, Listen::Denied, Listen::Unknown] {
            for done in [Rung::Nothing, Rung::Documented, Rung::All] {
                assert_eq!(listen_step(false, state, done), ListenStep::AccessibilityFirst);
            }
        }
    }

    #[test]
    fn input_monitoring_asks_only_while_it_reads_unknown() {
        for done in [Rung::Nothing, Rung::Documented, Rung::All] {
            assert_eq!(listen_step(true, Listen::Granted, done), ListenStep::Granted);
            assert_eq!(listen_step(true, Listen::Denied, done), ListenStep::InListOff);
        }
        let mut done = Rung::Nothing;
        let mut steps = Vec::new();
        for _ in 0..4 {
            let step = listen_step(true, Listen::Unknown, done);
            done = listen_after(step, done);
            steps.push(step);
        }
        assert_eq!(
            steps,
            [ListenStep::Documented, ListenStep::Hid, ListenStep::Exhausted, ListenStep::Exhausted]
        );
        assert_eq!(listen_after(ListenStep::InListOff, Rung::Documented), Rung::Documented);
    }

    #[test]
    fn input_monitoring_reads_the_iokit_codes() {
        assert_eq!(Listen::from_access(Some(0)), Listen::Granted);
        assert_eq!(Listen::from_access(Some(1)), Listen::Denied);
        assert_eq!(Listen::from_access(Some(2)), Listen::Unknown);
        assert_eq!(Listen::from_access(None), Listen::Unknown);
        // The words the log's before-and-after line is searched by.
        let words = [Listen::Granted, Listen::Denied, Listen::Unknown].map(Listen::word);
        assert_eq!(words, ["granted", "denied", "unknown"]);
        assert!(listen_change(Listen::Unknown, Listen::Denied).contains("in the Input Monitoring list"));
        assert!(listen_change(Listen::Unknown, Listen::Unknown).contains("still unknown"));
        assert_eq!(listen_change(Listen::Denied, Listen::Denied), "");
    }

    #[test]
    fn every_line_is_found_by_the_prefix_a_log_is_searched_by() {
        let answers = [
            SckAnswer::Absent,
            SckAnswer::TimedOut { ms: 5001 },
            SckAnswer::Content { displays: 2, windows: 14 },
            declined(),
            SckAnswer::Refused { domain: "d".into(), code: 7, text: "t".into() },
        ];
        for a in &answers {
            assert!(sck_line(a).starts_with("screen recording: "), "{a:?}");
        }
        for l in [
            LegacyAnswer::Absent,
            LegacyAnswer::Image { via: "CGWindowListCreateImage" },
            LegacyAnswer::NoImage { via: "CGDisplayCreateImageForRect" },
        ] {
            let line = legacy_line(l, "because");
            assert!(line.starts_with("screen recording: "), "{l:?}");
            assert!(line.contains("because"), "the reason is lost: {line}");
        }
        for more in [false, true] {
            assert!(fallback_line(more, "P", "b", false).starts_with("screen recording: "));
        }
    }

    #[test]
    fn the_lines_carry_what_the_system_answered() {
        let line = sck_line(&declined());
        assert!(line.contains("-3801") && line.contains("declined TCCs"), "{line}");
        assert!(sck_line(&SckAnswer::TimedOut { ms: 5001 }).contains("5001 ms"));
        let line = sck_line(&SckAnswer::Content { displays: 2, windows: 14 });
        assert!(line.contains("2 display(s)") && line.contains("14 window(s)"), "{line}");
        assert!(legacy_line(LegacyAnswer::Image { via: "CGWindowListCreateImage" }, "w")
            .contains("CGWindowListCreateImage"));
    }

    #[test]
    fn the_fallback_names_the_second_request_the_manual_way_and_the_stale_entry() {
        let pane = "System Settings > Privacy & Security";
        let line = fallback_line(true, pane, "com.automationplatform.app", false);
        assert!(line.contains("asks a second way"), "{line}");
        assert!(line.contains(pane) && line.contains("+ button"), "{line}");
        assert!(line.contains("tccutil reset ScreenCapture com.automationplatform.app"), "{line}");
        let line = fallback_line(false, pane, "x.y", true);
        assert!(!line.contains("second way"), "{line}");
        assert!(line.contains("tccutil reset ScreenCapture x.y"), "{line}");
        assert!(line.contains("macOS 12"), "{line}");
    }

    #[test]
    fn a_launch_from_a_terminal_is_named_and_one_from_launchd_is_not_a_warning() {
        assert!(launched_by_launchd(1));
        assert!(!launched_by_launchd(412));
        assert!(launch_line(1, None).starts_with("launchd"));
        let line = launch_line(412, Some("zsh"));
        assert!(line.starts_with("zsh (pid 412)") && line.contains("terminal"), "{line}");
        assert!(launch_line(412, Some("")).starts_with("another process (pid 412)"));
        assert!(launch_line(412, None).starts_with("another process (pid 412)"));
    }
}
