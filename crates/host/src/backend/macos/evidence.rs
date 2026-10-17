//! Whether Screen Recording is granted, and whether Input Monitoring is needed, judged by what
//! this run has seen rather than by the system's own checks alone — and the words the Permissions
//! page, the start announcement and the log say for each verdict — decided here, without a Mac.
//!
//! Its own file for the reason `enrol.rs` has one: nothing in it touches macOS, so `backend/mod.rs`
//! borrows it by `#[path]` under `cfg(test)` and its rules run where they are written. What it is
//! handed is gathered in `perm.rs` (the system's checks and the window list), `capture.rs` (what
//! ScreenCaptureKit last answered) and `tap.rs` (what the key capture has seen).
//!
//! **Why not `CGPreflightScreenCaptureAccess` alone.** It answered "not granted" for the whole of
//! the first session of the Avenger test (macOS 26.6.1, 2026-10-14), in which every capture held
//! Logic and the plug-in: OCR read the plug-in's header, and the overlay identified it by that read.
//! On macOS 12.7.6 it said the same while a capture held a plug-in's real window, and on macOS 14.5
//! it read "granted" only in the launch after the grant. A capture asked of ScreenCaptureKit is
//! answered from the permission as it stands — a picture, or `SCStreamErrorUserDeclined` (-3801)
//! without it, as the same session's request was answered before the grant — so its last answer
//! counts first, a refusal only until a grant made since shows. The system's check is believed
//! next, where it says granted. Before anything has been captured, and after a refusal, the
//! window list: macOS gives an application the NAMES of other applications' windows only with
//! Screen Recording (Apple's developer technical support on the developer forums; the reference
//! does not say so, and a macOS 27 beta was reported to hand out owner names without it), and
//! asking for the list puts nothing on screen. After a refusal they count only when they were
//! hidden as it came ([`after_refusal`]): names shown now then say that something changed since.
//!
//! **Why Input Monitoring is optional.** The same session's log read it "denied" while the event
//! tap captured the overlay's keys with Accessibility alone (macOS 26.6). It is counted as needed
//! only when captured keys are seen not to reach the overlay: macOS refused the tap with
//! Accessibility granted, or a shortcut of this application's — Carbon's, which needs no
//! permission — arrived while the installed tap had seen no key at all, though every key passes
//! the tap's place before it becomes a shortcut.

/// What ScreenCaptureKit last answered this process, as evidence about Screen Recording. The last
/// answer stands, whichever kind of request it answered: a permission switched off while the
/// application runs is refused from then on. A refusal keeps what the window names said as it
/// came ([`after_refusal`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Seen {
    /// Nothing asked yet, or nothing answered that says anything: a timeout, another error.
    Nothing = 0,
    /// A capture came back with a picture.
    Picture = 1,
    /// The list of what can be captured was handed over.
    Content = 2,
    /// Refused as not permitted (`SCStreamErrorUserDeclined`) while other applications' window
    /// names were shown, or there was no window to tell by: the refusal stands until
    /// ScreenCaptureKit answers otherwise.
    Declined = 3,
    /// Refused as not permitted while other applications' windows carried no names, as without
    /// Screen Recording: names shown later say it was granted since.
    DeclinedNamesHidden = 4,
}

impl Seen {
    /// Back from the number an atomic keeps it as. An unknown number is nothing seen.
    pub(crate) fn from_u8(n: u8) -> Seen {
        match n {
            1 => Seen::Picture,
            2 => Seen::Content,
            3 => Seen::Declined,
            4 => Seen::DeclinedNamesHidden,
            _ => Seen::Nothing,
        }
    }
}

/// Other applications' ordinary windows on screen, by whether their names can be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Names {
    /// At least one carries its name: macOS shows them only with Screen Recording.
    Shown,
    /// There are some, and none carries a name, as without Screen Recording.
    Hidden,
    /// There is none to tell by, or the list could not be had.
    NoWindows,
}

/// One entry of the window server's on-screen list, as far as [`names`] needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listed {
    pub pid: i64,
    /// `kCGWindowLayer`: 0 is an ordinary window. The menu bar, the Dock, a menu, a panel floating
    /// over everything are other layers, and the system's own processes own most of them.
    pub layer: i64,
    /// Whether the entry carries `kCGWindowName` at all — empty or not.
    pub named: bool,
}

/// What the window list says about Screen Recording: only ordinary windows of other processes
/// count. Chromium's check for the same question, on macOS 10.15, counted the Dock's layer as
/// well; leaving it out can only find fewer names, so it can only read "granted" less often.
pub(crate) fn names(windows: &[Listed], own_pid: i64) -> Names {
    let mut any = false;
    for w in windows.iter().filter(|w| w.pid != own_pid && w.layer == 0) {
        if w.named {
            return Names::Shown;
        }
        any = true;
    }
    if any {
        Names::Hidden
    } else {
        Names::NoWindows
    }
}

/// What a refusal leaves as evidence, after `before`: the window names are read as it comes
/// (`names`), so that names read later overrule it only when they were hidden then. Names shown at
/// a refusal say nothing about a grant since: the window server may keep its first answer, as the
/// system's check does. Read at the first refusal after another answer, or none; the refusals
/// after it keep what it found — they come with every capture while the permission is missing,
/// and a look at the window list for each would make every poll of a region dearer.
pub(crate) fn after_refusal(before: Seen, names: impl FnOnce() -> Names) -> Seen {
    if matches!(before, Seen::Declined | Seen::DeclinedNamesHidden) {
        return before;
    }
    match names() {
        Names::Hidden => Seen::DeclinedNamesHidden,
        Names::Shown | Names::NoWindows => Seen::Declined,
    }
}

/// What decided Screen Recording's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenBasis {
    /// Granted: a capture through ScreenCaptureKit came back with a picture.
    Picture,
    /// Granted: ScreenCaptureKit handed over what can be captured.
    Content,
    /// Missing: ScreenCaptureKit refused as not permitted.
    Declined,
    /// Granted: the system's own check says so, and nothing has said otherwise.
    Preflight,
    /// Granted: other applications' window names can be read, while the system's check says no.
    Names,
    /// Granted: other applications' window names can be read now, after ScreenCaptureKit refused
    /// a capture while they were hidden — the permission was granted since.
    NamesSinceRefusal,
    /// Missing: the system's check says so, and the window list says `names` (never `Shown`).
    PreflightNo { names: Names },
}

/// Screen Recording as judged, with what decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScreenVerdict {
    pub granted: bool,
    pub basis: ScreenBasis,
}

/// Judges Screen Recording: ScreenCaptureKit's last answer first, then the system's check when it
/// says granted, then the window names; `names` is asked only when it is needed, since it costs a
/// look at the window list.
///
/// A refusal that came while the names were hidden is the one answer the window names can
/// overrule: they are read now, and the refusal was given to a capture before. A grant made after
/// it — in System Settings, while the application runs — is then seen before the next capture is
/// made, which may be long after: no module captures while no overlay is in front. A refusal that
/// came while they were shown stands: they were no sign of the permission then, so they are none
/// now. A picture is not overruled by names that are hidden: windows without a title carry none
/// either, and the next capture is refused anyway if the permission was taken back.
pub(crate) fn screen_verdict(preflight: bool, seen: Seen, names: impl FnOnce() -> Names) -> ScreenVerdict {
    let (granted, basis) = match seen {
        Seen::Picture => (true, ScreenBasis::Picture),
        Seen::Content => (true, ScreenBasis::Content),
        Seen::Declined => (false, ScreenBasis::Declined),
        Seen::DeclinedNamesHidden => match names() {
            Names::Shown => (true, ScreenBasis::NamesSinceRefusal),
            _ => (false, ScreenBasis::Declined),
        },
        Seen::Nothing if preflight => (true, ScreenBasis::Preflight),
        Seen::Nothing => match names() {
            Names::Shown => (true, ScreenBasis::Names),
            other => (false, ScreenBasis::PreflightNo { names: other }),
        },
    };
    ScreenVerdict { granted, basis }
}

impl ScreenVerdict {
    /// The sentence for the Permissions page, and for the start announcement while it is missing.
    pub(crate) fn judged(self) -> &'static str {
        match self.basis {
            ScreenBasis::Picture => {
                "Judged by a capture: the screen came back in a picture, which macOS gives only to \
                 an application allowed to record it."
            }
            ScreenBasis::Content => {
                "Judged by macOS handing over the list of what can be captured, which it does only \
                 for an application allowed to record the screen."
            }
            ScreenBasis::Declined => "Judged by a capture: macOS refused it as not permitted.",
            ScreenBasis::Preflight => "Judged by the system's own check.",
            ScreenBasis::Names => {
                "Judged by the names of other applications' windows, which macOS shows only to an \
                 application allowed to record the screen. The system's own check says not granted; \
                 it has been seen saying so while every capture worked."
            }
            ScreenBasis::NamesSinceRefusal => {
                "Judged by the names of other applications' windows, which macOS shows only to an \
                 application allowed to record the screen: they were hidden when the last capture \
                 was refused, so it was granted since."
            }
            ScreenBasis::PreflightNo { names: Names::Hidden } => {
                "Judged by the system's own check, and the windows of other applications carry no \
                 names, as they do without it."
            }
            ScreenBasis::PreflightNo { .. } => {
                "Judged by the system's own check alone: no capture has answered the question yet, \
                 and no other application has a window on screen to tell by."
            }
        }
    }

    /// The same, for a log line: after `granted, judged by ` or `NOT granted, judged by `.
    pub(crate) fn short(self) -> &'static str {
        match self.basis {
            ScreenBasis::Picture => "a capture: ScreenCaptureKit returned a picture",
            ScreenBasis::Content => "ScreenCaptureKit handing over what can be captured",
            ScreenBasis::Declined => "a capture: ScreenCaptureKit refused it as not permitted (-3801)",
            ScreenBasis::Preflight => "CGPreflightScreenCaptureAccess",
            ScreenBasis::Names => {
                "other applications' window names, which macOS shows only with it \
                 (CGPreflightScreenCaptureAccess answers not granted)"
            }
            ScreenBasis::NamesSinceRefusal => {
                "other applications' window names, which macOS shows only with it, read after \
                 ScreenCaptureKit had refused a capture (-3801) while they were hidden"
            }
            ScreenBasis::PreflightNo { names: Names::Hidden } => {
                "CGPreflightScreenCaptureAccess, and other applications' windows carry no names"
            }
            ScreenBasis::PreflightNo { .. } => {
                "CGPreflightScreenCaptureAccess alone: no capture has answered the question yet, and \
                 no other application's window on screen to tell by"
            }
        }
    }
}

/// What the key capture has shown in this run about keys reaching an overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Keys {
    /// Nothing yet: no tap, or no key has reached it and nothing has shown that one should have.
    NotYet,
    /// The tap sees the keyboard; no overlay has taken a key yet.
    Seen,
    /// The tap took a key for an overlay: captured keys reach it.
    Reached,
    /// A shortcut arrived while the installed tap had seen no key at all.
    Deaf,
    /// macOS refused the tap while Accessibility was granted.
    Refused,
}

/// Whether Input Monitoring counts as needed: only while it is not granted and captured keys are
/// seen not to reach the overlay.
pub(crate) fn listen_needed(granted: bool, keys: Keys) -> bool {
    !granted && matches!(keys, Keys::Deaf | Keys::Refused)
}

impl Keys {
    /// The sentence for the Permissions page, with Input Monitoring `granted` or not.
    pub(crate) fn judged(self, granted: bool) -> &'static str {
        match self {
            // Granted, and the keys still do not arrive: as far as anything here can tell, the
            // grant has not reached this process, and a new one is what is left to try.
            Keys::Deaf if granted => {
                "A shortcut of this application arrived while the key capture had seen no key at \
                 all, although Input Monitoring is granted: if it was granted while the \
                 application ran, quit the application and open it again."
            }
            Keys::Refused if granted => {
                "macOS refused the key capture although Accessibility and Input Monitoring are \
                 granted: if either was granted while the application ran, quit the application \
                 and open it again."
            }
            Keys::NotYet => {
                "No overlay has taken a key in this run yet, so nothing has shown whether it is \
                 needed."
            }
            Keys::Seen => "The key capture sees the keyboard in this run; no overlay has taken a key yet.",
            Keys::Reached => "Keys an overlay took have reached it in this run.",
            Keys::Deaf => {
                "A shortcut of this application arrived while the key capture had seen no key at \
                 all: keys an overlay takes do not reach it, so Input Monitoring is needed now."
            }
            Keys::Refused => {
                "macOS refused the key capture although Accessibility is granted, so Input \
                 Monitoring is needed now."
            }
        }
    }

    /// The same, for a log line.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Keys::NotYet => "no key taken for an overlay yet",
            Keys::Seen => "the tap sees keys, none taken for an overlay yet",
            Keys::Reached => "keys taken for an overlay reach it",
            Keys::Deaf => "a shortcut arrived while the tap had seen no key: captured keys do not arrive",
            Keys::Refused => "the tap was refused with Accessibility granted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: i64 = 4242;

    fn w(pid: i64, layer: i64, named: bool) -> Listed {
        Listed { pid, layer, named }
    }

    #[test]
    fn only_another_application_s_ordinary_window_tells() {
        assert_eq!(names(&[], OWN), Names::NoWindows);
        // This application's own windows carry their names whatever is granted.
        assert_eq!(names(&[w(OWN, 0, true)], OWN), Names::NoWindows);
        // The menu bar, the Dock and floating panels are the system's, on other layers.
        assert_eq!(names(&[w(90, 24, true), w(91, 20, true), w(92, 3, true)], OWN), Names::NoWindows);
        assert_eq!(names(&[w(OWN, 0, true), w(500, 0, false), w(501, 0, false)], OWN), Names::Hidden);
        assert_eq!(names(&[w(500, 0, false), w(501, 0, true)], OWN), Names::Shown);
    }

    #[test]
    fn screencapturekit_s_last_answer_comes_first() {
        let never = || -> Names { panic!("the window list was asked although a capture had answered") };
        for preflight in [false, true] {
            let v = screen_verdict(preflight, Seen::Picture, never);
            assert_eq!(v, ScreenVerdict { granted: true, basis: ScreenBasis::Picture });
            let v = screen_verdict(preflight, Seen::Content, never);
            assert_eq!(v, ScreenVerdict { granted: true, basis: ScreenBasis::Content });
            // Switched off while the application runs: the system's check may still say granted,
            // and so may the window names, were the window server to keep its first answer as
            // that check does. Shown at the refusal, they are not asked again.
            let v = screen_verdict(preflight, Seen::Declined, never);
            assert_eq!(v, ScreenVerdict { granted: false, basis: ScreenBasis::Declined });
            for names in [Names::Hidden, Names::NoWindows] {
                let v = screen_verdict(preflight, Seen::DeclinedNamesHidden, || names);
                assert_eq!(v, ScreenVerdict { granted: false, basis: ScreenBasis::Declined });
            }
        }
    }

    #[test]
    fn a_grant_after_a_refusal_is_seen_before_the_next_capture() {
        // The first session: refused at the request (-3801), granted in System Settings a few
        // minutes later, with no overlay in front to capture anything in between.
        let refused = after_refusal(Seen::Nothing, || Names::Hidden);
        assert_eq!(refused, Seen::DeclinedNamesHidden);
        for preflight in [false, true] {
            let v = screen_verdict(preflight, refused, || Names::Shown);
            assert_eq!(v, ScreenVerdict { granted: true, basis: ScreenBasis::NamesSinceRefusal });
        }
    }

    #[test]
    fn names_shown_at_the_refusal_mean_the_refusal_stands() {
        // Granted at launch, so the names are shown; switched off while the application runs, and
        // the next capture refused while they still are.
        for before in [Seen::Nothing, Seen::Picture, Seen::Content] {
            assert_eq!(after_refusal(before, || Names::Shown), Seen::Declined, "{before:?}");
            assert_eq!(after_refusal(before, || Names::NoWindows), Seen::Declined, "{before:?}");
        }
        let never = || -> Names { panic!("the window list was asked although the refusal stands") };
        for preflight in [false, true] {
            let v = screen_verdict(preflight, after_refusal(Seen::Picture, || Names::Shown), never);
            assert_eq!(v, ScreenVerdict { granted: false, basis: ScreenBasis::Declined });
        }
        // The refusals after the first keep what it found, and read nothing.
        assert_eq!(after_refusal(Seen::Declined, never), Seen::Declined);
        assert_eq!(after_refusal(Seen::DeclinedNamesHidden, never), Seen::DeclinedNamesHidden);
    }

    #[test]
    fn the_first_session_of_the_avenger_test_reads_granted() {
        // The check said no throughout while ScreenCaptureKit's captures held Logic.
        let v = screen_verdict(false, Seen::Picture, || Names::Hidden);
        assert!(v.granted);
        // And before its first capture, the window names decide.
        let v = screen_verdict(false, Seen::Nothing, || Names::Shown);
        assert_eq!(v, ScreenVerdict { granted: true, basis: ScreenBasis::Names });
    }

    #[test]
    fn the_system_s_check_decides_what_nothing_else_can() {
        let never = || -> Names { panic!("the window list was asked although the check said granted") };
        assert_eq!(
            screen_verdict(true, Seen::Nothing, never),
            ScreenVerdict { granted: true, basis: ScreenBasis::Preflight }
        );
        for names in [Names::Hidden, Names::NoWindows] {
            assert_eq!(
                screen_verdict(false, Seen::Nothing, || names),
                ScreenVerdict { granted: false, basis: ScreenBasis::PreflightNo { names } }
            );
        }
    }

    #[test]
    fn every_verdict_says_what_decided_it() {
        let all = [
            ScreenBasis::Picture,
            ScreenBasis::Content,
            ScreenBasis::Declined,
            ScreenBasis::Preflight,
            ScreenBasis::Names,
            ScreenBasis::NamesSinceRefusal,
            ScreenBasis::PreflightNo { names: Names::Hidden },
            ScreenBasis::PreflightNo { names: Names::NoWindows },
        ];
        for basis in all {
            let v = ScreenVerdict { granted: true, basis };
            assert!(v.judged().starts_with("Judged by "), "{basis:?}");
            assert!(v.judged().ends_with('.'), "{basis:?}");
            assert!(!v.short().is_empty(), "{basis:?}");
        }
        let names = ScreenVerdict { granted: true, basis: ScreenBasis::Names };
        assert!(names.judged().contains("window") && names.short().contains("CGPreflightScreenCaptureAccess"));
        let declined = ScreenVerdict { granted: false, basis: ScreenBasis::Declined };
        assert!(declined.short().contains("-3801"));
    }

    #[test]
    fn seen_survives_its_trip_through_an_atomic() {
        for s in [Seen::Nothing, Seen::Picture, Seen::Content, Seen::Declined, Seen::DeclinedNamesHidden] {
            assert_eq!(Seen::from_u8(s as u8), s);
        }
        assert_eq!(Seen::from_u8(200), Seen::Nothing);
    }

    #[test]
    fn input_monitoring_is_needed_only_when_keys_are_seen_not_to_arrive() {
        // The tester's first session: denied, and the tap took the overlay's keys.
        for keys in [Keys::NotYet, Keys::Seen, Keys::Reached] {
            assert!(!listen_needed(false, keys), "{keys:?}");
        }
        for keys in [Keys::Deaf, Keys::Refused] {
            assert!(listen_needed(false, keys), "{keys:?}");
            assert!(!listen_needed(true, keys), "granted already: {keys:?}");
            assert!(keys.judged(false).contains("needed now"), "{keys:?}");
            // Granted already: not needed, and what is left to try.
            assert!(!keys.judged(true).contains("needed"), "{keys:?}");
            assert!(keys.judged(true).contains("open it again"), "{keys:?}");
        }
        for keys in [Keys::NotYet, Keys::Seen, Keys::Reached, Keys::Deaf, Keys::Refused] {
            for granted in [false, true] {
                assert!(keys.judged(granted).ends_with('.') && !keys.word().is_empty(), "{keys:?}");
            }
        }
    }
}
