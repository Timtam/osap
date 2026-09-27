//! How soon a screen reader that turned us down is tried again — one schedule for both
//! platforms.
//!
//! Written for prism's searcher on Windows (`prism.rs`, where the reasons for each number are
//! measured and written down) and used by the VoiceOver path on macOS as well (`vo_park.rs`), so
//! a user who restarts their screen reader gets it back on the same terms on either machine: at
//! the same pace, and with the same end to the fast pace. Pure, so its tests run on every
//! platform.

use std::time::Duration;

/// The fast pace: every three seconds.
pub(super) const RETRY_SOON: Duration = Duration::from_secs(3);
/// The pace for a reader that runs and has refused for longer than [`RUNNING_FAST_FOR`].
pub(super) const RETRY_CAPPED: Duration = Duration::from_secs(10);
/// The pace for a search that has found none running for longer than [`RETRY_SOON_FOR`].
pub(super) const RETRY_LATER: Duration = Duration::from_secs(30);
/// How long the fast pace lasts while no reader runs.
pub(super) const RETRY_SOON_FOR: Duration = Duration::from_secs(60);
/// How long the fast pace lasts while a reader runs and refuses (the maintainer's decision,
/// 2026-09-25: the fast pace has an end).
pub(super) const RUNNING_FAST_FOR: Duration = Duration::from_secs(300);

/// When to look again after a look that opened nothing. For a reader that is `running`,
/// `since` is how long it has been seen running and refusing: [`RETRY_SOON`] for the first
/// [`RUNNING_FAST_FOR`], [`RETRY_CAPPED`] after that. Otherwise `since` is how long the search
/// has been going: [`RETRY_SOON`] for the first [`RETRY_SOON_FOR`], [`RETRY_LATER`] after that.
pub(super) fn next_look(since: Duration, running: bool) -> Duration {
    match (running, since) {
        (true, s) if s < RUNNING_FAST_FOR => RETRY_SOON,
        (true, _) => RETRY_CAPPED,
        (false, s) if s < RETRY_SOON_FOR => RETRY_SOON,
        (false, _) => RETRY_LATER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    /// The pace: 3 s for five minutes and 10 s after while a reader runs, 3 s for a minute and
    /// 30 s after while none does.
    #[test]
    fn the_schedule() {
        for s in [0, 3, 59, 60, 61, 90, 299] {
            assert_eq!(next_look(secs(s), true), RETRY_SOON, "running, {s} s in");
        }
        for s in [300, 301, 3600] {
            assert_eq!(next_look(secs(s), true), RETRY_CAPPED, "running, {s} s in");
        }
        for s in [0, 3, 57, 59] {
            assert_eq!(next_look(secs(s), false), RETRY_SOON, "none running, {s} s in");
        }
        for s in [60, 61, 90, 3600] {
            assert_eq!(next_look(secs(s), false), RETRY_LATER, "none running, {s} s in");
        }
    }
}
