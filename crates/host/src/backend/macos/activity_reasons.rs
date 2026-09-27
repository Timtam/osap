//! The App Nap activity's bookkeeping (`activity.rs`): which reasons hold it, which options it
//! is begun with, and what the log says when a reason comes or goes.
//!
//! **Latency-critical only for the reasons that need it.** Captured keys and a controller's
//! buttons need an answer within milliseconds — a key the event tap hands back late is one the
//! system may switch the tap off for — so they get `UserInitiatedAllowingIdleSystemSleep |
//! LatencyCritical`. Screen Recording's own request waiting for Accessibility only needs App Nap
//! kept away, so that its look once a second is not stretched; and it waits for as long as
//! Accessibility is not granted, which after an ad-hoc rebuild (which loses the grant) can be a
//! whole session. So on its own it gets `UserInitiatedAllowingIdleSystemSleep`, which keeps App
//! Nap away without asking for the most accurate timers. When the reasons change which of the
//! two is called for, `activity.rs` begins the new activity before it ends the old one.
//!
//! **A line for every reason the first time it holds the activity** — whether the activity was
//! already held for another one or not. An overlay captures and releases its keys at every
//! activation, so the later holds for a reason are traced; and a reason that joins an activity
//! already held still gets its line, so that neither "keys are captured" nor the setup wait is
//! missing from a log because the other came first.
//!
//! Pure, so that it is decided and tested without a Mac (`backend/mod.rs` borrows this file for
//! its tests on Windows).

/// Keys are captured.
pub(crate) const KEYS: u8 = 1;
/// A game controller's buttons or axes are listened to.
pub(crate) const GAMEPAD: u8 = 2;
/// Screen Recording's own request waits for Accessibility to be granted (`perm::pump`).
pub(crate) const SETUP: u8 = 4;

/// The reasons that need an answer within milliseconds, and so `LatencyCritical` too.
const LATENCY_CRITICAL: u8 = KEYS | GAMEPAD;

/// The activity a set of reasons calls for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Hold {
    /// No activity: App Nap may come.
    None,
    /// `UserInitiatedAllowingIdleSystemSleep`: App Nap kept away.
    Plain,
    /// `UserInitiatedAllowingIdleSystemSleep | LatencyCritical`.
    LatencyCritical,
}

impl Hold {
    pub(crate) fn for_reasons(reasons: u8) -> Hold {
        if reasons & LATENCY_CRITICAL != 0 {
            Hold::LatencyCritical
        } else if reasons != 0 {
            Hold::Plain
        } else {
            Hold::None
        }
    }
}

/// What one call to `activity::want` does.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Change {
    /// The reasons after the call.
    pub(crate) reasons: u8,
    /// The activity they call for.
    pub(crate) hold: Hold,
    /// Whether that differs from the activity held before the call: begin `hold` (unless it is
    /// `None`), then end the activity held before (if there was one).
    pub(crate) swap: bool,
    /// The reasons that have had their line written in full, after the call.
    pub(crate) said: u8,
    /// What the log gets: each line, and whether it is written in full (`true`) or traced.
    pub(crate) lines: Vec<(String, bool)>,
}

/// Sets (`on`) or clears one `reason`, given the reasons held before and the reasons whose line
/// has been written in full.
pub(crate) fn change(before: u8, said: u8, reason: u8, on: bool) -> Change {
    let reasons = if on { before | reason } else { before & !reason };
    let hold = Hold::for_reasons(reasons);
    let swap = hold != Hold::for_reasons(before);
    let added = reasons & !before;
    let mut lines = Vec::new();
    if added != 0 {
        let what = match hold {
            Hold::LatencyCritical => "a latency-critical activity",
            _ => "an activity (not latency-critical)",
        };
        let others = reasons & !added;
        let already = if others != 0 { format!(" (already held while {})", words(others)) } else { String::new() };
        lines.push((
            format!(
                "App Nap: holding {what} while {}{already} — Activity Monitor's Energy tab should \
                 read App Nap: No, and Preventing Sleep: No",
                words(added)
            ),
            said & added != added,
        ));
    } else if swap {
        lines.push((
            match hold {
                Hold::None => {
                    "App Nap: the activity is ended — nothing is captured, listened to or waited for"
                        .to_string()
                }
                _ => format!("App Nap: the activity is no longer latency-critical — only while {}", words(reasons)),
            },
            false,
        ));
    }
    Change { reasons, hold, swap, said: said | added, lines }
}

/// The reasons in words, joined with commas and a final "and"; "nothing" for none.
pub(crate) fn words(reasons: u8) -> String {
    let each: Vec<&str> = [
        (KEYS, "keys are captured"),
        (GAMEPAD, "a game controller's buttons or axes are listened to"),
        (SETUP, "Screen Recording's own request waits for Accessibility to be granted"),
    ]
    .into_iter()
    .filter(|&(bit, _)| reasons & bit != 0)
    .map(|(_, w)| w)
    .collect();
    match each.as_slice() {
        [] => "nothing".to_string(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `steps` from nothing held and nothing said, and returns every change.
    fn run(steps: &[(u8, bool)]) -> Vec<Change> {
        let (mut reasons, mut said) = (0, 0);
        steps
            .iter()
            .map(|&(reason, on)| {
                let c = change(reasons, said, reason, on);
                (reasons, said) = (c.reasons, c.said);
                c
            })
            .collect()
    }

    #[test]
    fn only_keys_and_controllers_are_latency_critical() {
        assert_eq!(Hold::for_reasons(0), Hold::None);
        assert_eq!(Hold::for_reasons(SETUP), Hold::Plain);
        for r in [KEYS, GAMEPAD, KEYS | SETUP, GAMEPAD | SETUP, KEYS | GAMEPAD | SETUP] {
            assert_eq!(Hold::for_reasons(r), Hold::LatencyCritical, "{r}");
        }
    }

    /// A first setup on its own: a plain activity, said once, and ended by the request.
    #[test]
    fn the_setup_wait_alone_holds_a_plain_activity() {
        let c = run(&[(SETUP, true), (SETUP, true), (SETUP, false)]);
        assert!(c[0].swap && c[0].hold == Hold::Plain);
        assert_eq!(c[0].lines.len(), 1);
        let (line, full) = &c[0].lines[0];
        assert!(*full);
        assert!(line.starts_with(
            "App Nap: holding an activity (not latency-critical) while Screen Recording's own \
             request waits for Accessibility to be granted — "
        ), "{line}");
        // The pump asks again every second while it waits: nothing changes and nothing is said.
        assert_eq!(c[1], Change { reasons: SETUP, hold: Hold::Plain, swap: false, said: SETUP, lines: vec![] });
        assert!(c[2].swap && c[2].hold == Hold::None);
        assert_eq!(c[2].lines, vec![(
            "App Nap: the activity is ended — nothing is captured, listened to or waited for".to_string(),
            false
        )]);
    }

    /// Keys captured at load, before the first tick finds Accessibility missing (a module that
    /// captures at load on a Mac without it): the setup wait joins an activity already held and
    /// still gets its line in full.
    #[test]
    fn a_reason_that_joins_a_held_activity_is_still_said() {
        let c = run(&[(KEYS, true), (SETUP, true)]);
        assert!(c[0].swap && c[0].hold == Hold::LatencyCritical);
        assert!(c[0].lines[0].1 && c[0].lines[0].0.starts_with(
            "App Nap: holding a latency-critical activity while keys are captured — "
        ), "{:?}", c[0].lines);
        assert!(!c[1].swap, "already latency-critical");
        assert_eq!(c[1].lines.len(), 1);
        let (line, full) = &c[1].lines[0];
        assert!(*full, "the setup wait's first hold is written");
        assert!(line.contains(
            "while Screen Recording's own request waits for Accessibility to be granted (already \
             held while keys are captured) — "
        ), "{line}");
    }

    /// A pad listened to, or the setup wait, before the first key is captured: the first
    /// capture still writes its line in full, and the activity becomes latency-critical.
    #[test]
    fn the_first_capture_is_said_whatever_held_the_activity_before() {
        for first in [GAMEPAD, SETUP] {
            let c = run(&[(first, true), (KEYS, true)]);
            assert_eq!(c[1].swap, first == SETUP, "plain to latency-critical only after the setup wait");
            let (line, full) = &c[1].lines[0];
            assert!(*full, "{first}");
            assert!(line.starts_with("App Nap: holding a latency-critical activity while keys are captured ("), "{line}");
        }
    }

    /// Every activation captures and releases keys again: after the first, only traces — and
    /// a release that leaves only the setup wait goes back to a plain activity.
    #[test]
    fn later_holds_are_traced_and_a_release_lowers_the_activity() {
        let c = run(&[(SETUP, true), (KEYS, true), (KEYS, false), (KEYS, true), (KEYS, false), (SETUP, false)]);
        assert!(c[1].lines[0].1, "the first capture is written");
        assert!(c[2].swap && c[2].hold == Hold::Plain);
        assert_eq!(c[2].lines, vec![(
            "App Nap: the activity is no longer latency-critical — only while Screen Recording's \
             own request waits for Accessibility to be granted"
                .to_string(),
            false
        )]);
        assert!(c[3].swap && !c[3].lines[0].1, "a second capture is traced");
        assert!(c[5].swap && c[5].hold == Hold::None);
        for step in &c[2..] {
            assert!(step.lines.iter().all(|(_, full)| !full), "{:?}", step.lines);
        }
    }

    /// Releasing one of two latency-critical reasons changes nothing and says nothing.
    #[test]
    fn a_release_that_keeps_the_same_activity_is_silent() {
        let c = run(&[(KEYS, true), (GAMEPAD, true), (KEYS, false)]);
        assert!(!c[1].swap && !c[2].swap);
        assert!(c[2].lines.is_empty());
        assert_eq!(c[2].hold, Hold::LatencyCritical);
        // Clearing a reason that was never set is no change at all.
        assert_eq!(change(0, 0, SETUP, false), Change { reasons: 0, hold: Hold::None, swap: false, said: 0, lines: vec![] });
    }

    #[test]
    fn reasons_read_as_a_list() {
        assert_eq!(words(0), "nothing");
        assert_eq!(words(KEYS), "keys are captured");
        assert_eq!(
            words(KEYS | GAMEPAD | SETUP),
            "keys are captured, a game controller's buttons or axes are listened to and Screen \
             Recording's own request waits for Accessibility to be granted"
        );
    }
}
