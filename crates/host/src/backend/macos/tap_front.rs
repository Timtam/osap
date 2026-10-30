//! Whether the application in front is still the one the event tap was told of: the question the
//! tap asks the window server for each key it would take for a module whose captures are pinned to
//! a window.
//!
//! Its own file for the reason `tap_handoff.rs` has one: a comparison of two numbers, with nothing
//! macOS about it, so `backend/mod.rs` borrows it by `#[path]` under `cfg(test)` and its rules run
//! on the machine they are written on.
//!
//! **Why the tap asks.** The tap answers on a thread of its own (`tap.rs`), and decides every key
//! by what the main thread last told it: the window in front, the captures, the scopes. While the
//! main thread is busy — a walk of a plug-in's tree, an application that does not answer — that
//! can be seconds old. A user who switches to another application meanwhile would have every key a
//! capture holds taken for the window they left, and handed to its overlay, which lets go of its
//! keys as the activation is heard: the keys would reach neither. So for each key it would take for
//! a pinned capture the tap asks which application is in front as the window server knows it now,
//! and lets the key through to that application when it is not the one the main thread was in when
//! it last told the tap its window. No timer decides it: the answer is the window server's, at the
//! key.
//!
//! The applications are compared as the Process Manager names them (a `ProcessSerialNumber`, two
//! 32-bit halves, packed into one word here): the one call the tap makes per key answers in that
//! form, and the main thread turns the application it last told the tap of into it once per change
//! of application, not per key.

use std::time::Duration;

/// What the comparison says about a key the tap would take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontCheck {
    /// The application in front is the one the tap was told of: decided as the tap decides it.
    Same,
    /// Another application is in front, which the main thread has not told the tap of yet: the key
    /// goes to it.
    Other,
    /// One side is not known — the window server did not say, or the main thread had no answer for
    /// the window it told the tap of — so the key is decided as before the tap asked, by the window
    /// the tap was told.
    Unknown,
}

/// A process serial number as one word: the high half above the low one. 0 is no process
/// (`kNoProcess`), which is never an answer here.
pub(crate) const fn psn_word(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

/// The two halves of a [`psn_word`].
pub(crate) const fn psn_halves(word: u64) -> (u32, u32) {
    ((word >> 32) as u32, word as u32)
}

/// The comparison: `told` is the application the main thread named with the window it told the tap
/// of, `now` the window server's answer at the key.
pub(crate) fn front_check(told: Option<u64>, now: Option<u64>) -> FrontCheck {
    match (told, now) {
        (Some(t), Some(n)) if t != 0 && n != 0 => {
            if t == n {
                FrontCheck::Same
            } else {
                FrontCheck::Other
            }
        }
        _ => FrontCheck::Unknown,
    }
}

/// The application told with window `window`, for a key decided by window `front`: `None` when the
/// two differ — the main thread told the tap of another window between the tap's two reads — or no
/// window was told, so that the comparison is never made against an application the decision was
/// not made with.
pub(crate) fn told_for(told: (isize, Option<u64>), front: isize) -> Option<u64> {
    let (window, psn) = told;
    if window == 0 || window != front {
        return None;
    }
    psn
}

/// An answer this slow, [`TOO_SLOW_IN_A_ROW`] times with no quicker one between, switches the tap's
/// question off until the Mac wakes, the screen is unlocked or this session becomes active again,
/// said once: it means the window server's answer waits on the tap itself, or the Mac is badly
/// overloaded, and every key a capture takes would wait as long. The keys are then decided as
/// before the tap asked.
pub(crate) const TOO_SLOW: Duration = Duration::from_millis(250);

/// How many answers of [`TOO_SLOW`] or more in a row switch the question off ([`slow_run`]). One
/// alone is a window server busy for a moment — just after a wake, during a change of displays —
/// and costs the one key that waited for it; switched off by it, the question would be gone until
/// the next wake, for a moment long over.
pub(crate) const TOO_SLOW_IN_A_ROW: u32 = 2;

/// The run of answers of [`TOO_SLOW`] or more after one that took `took`, from `run` before it,
/// and whether that run switches the question off: a quicker answer ends the run.
pub(crate) fn slow_run(run: u32, took: Duration) -> (u32, bool) {
    if took < TOO_SLOW {
        return (0, false);
    }
    let run = run.saturating_add(1);
    (run, run >= TOO_SLOW_IN_A_ROW)
}

/// An answer this slow is written down (at most every five seconds, the worst since the last
/// line): how much each key the tap would take pays for the question, measured where it is asked.
pub(crate) const SLOW: Duration = Duration::from_millis(1);

#[cfg(test)]
mod tests {
    use super::*;

    const REAPER: u64 = psn_word(0, 0x1_2345);
    const VOICEOVER: u64 = psn_word(0, 0x1_2399);

    /// The same application keeps the decision, another takes the key, and an unknown side changes
    /// nothing.
    #[test]
    fn another_application_in_front_takes_the_key_and_an_unknown_one_changes_nothing() {
        assert_eq!(front_check(Some(REAPER), Some(REAPER)), FrontCheck::Same);
        assert_eq!(front_check(Some(REAPER), Some(VOICEOVER)), FrontCheck::Other);
        assert_eq!(front_check(None, Some(VOICEOVER)), FrontCheck::Unknown);
        assert_eq!(front_check(Some(REAPER), None), FrontCheck::Unknown);
        assert_eq!(front_check(Some(0), Some(VOICEOVER)), FrontCheck::Unknown, "no process is no answer");
        assert_eq!(front_check(Some(REAPER), Some(0)), FrontCheck::Unknown);
    }

    /// The application is compared only with the window the key was decided by.
    #[test]
    fn the_application_told_counts_only_for_its_own_window() {
        assert_eq!(told_for((7, Some(REAPER)), 7), Some(REAPER));
        assert_eq!(told_for((7, Some(REAPER)), 8), None, "told of another window between the reads");
        assert_eq!(told_for((0, Some(REAPER)), 0), None, "no window told");
        assert_eq!(told_for((7, None), 7), None);
    }

    /// Both halves come back, the high one above the low one.
    #[test]
    fn a_process_serial_number_packs_into_one_word() {
        for (high, low) in [(0, 1), (0, 0x1_2345), (1, 0), (u32::MAX, u32::MAX)] {
            assert_eq!(psn_halves(psn_word(high, low)), (high, low));
        }
        assert_ne!(psn_word(1, 0), psn_word(0, 1));
        assert!(SLOW < TOO_SLOW);
    }

    /// One slow answer is a moment of a busy window server and switches nothing off; two in a row
    /// do, and a quicker answer between starts the run again.
    #[test]
    fn only_slow_answers_in_a_row_switch_the_question_off() {
        let slow = TOO_SLOW;
        let quick = TOO_SLOW - Duration::from_millis(1);
        assert_eq!(slow_run(0, quick), (0, false));
        assert_eq!(slow_run(0, slow), (1, false), "one alone");
        assert_eq!(slow_run(1, slow), (2, true), "two in a row");
        assert_eq!(slow_run(1, quick), (0, false), "a quicker one between");
        assert_eq!(slow_run(0, slow), (1, false), "and the run starts again");
        assert_eq!(slow_run(u32::MAX, slow), (u32::MAX, true));
    }
}
