//! When the VoiceOver path is tried again after VoiceOver would not take a line, and what a
//! refusal costs while it is.
//!
//! It used to be parked for the rest of the session after the first failure of any kind, and
//! only ticking "Speak through VoiceOver" off and on brought it back — which nobody does for a
//! failure they did not know about. A timed-out Apple Event, VoiceOver restarting after a wake
//! or being toggled with Command+F5, a VoiceOver too busy to answer: each of those took the
//! user's own voice and their braille display away until the application was restarted. Over
//! days of uptime that is not a rare case but the expected one.
//!
//! So a failure is classified. **Not permitted** — `errAEEventNotPermitted`, -1743: this
//! application may not send Apple Events to VoiceOver — parks the path as before: the answer
//! does not change until the user changes it, and ticking the setting again is what asks for
//! the permission. **Anything else** parks it until the next look on the schedule prism's
//! searcher keeps on Windows ([`super::pace::next_look`]), and at once when VoiceOver runs as a
//! new process or the Mac wakes. **A refusal that kept the speech thread waiting** until the
//! child process was stopped ([`Cause::Stalled`]) puts the rest of its run of refusals on the
//! slow pace at once: each look can cost that wait again.
//!
//! **A look lets one line through** ([`Park::handed`]). That line is the try; every line said
//! while it is under way goes to the system voice at once rather than queueing behind it, so a
//! look that stalls delays one line, not everything said meanwhile.
//!
//! `errAEEventWouldRequireUserConsent` (-1744) is deliberately **not** "not permitted": it is
//! what an Apple Event gets while the Automation question has not been answered, and the look
//! after the user allows it must find the path open. What it costs is decided by the transport
//! ([`after_event`]): `osascript` — which would put the question on screen and wait for it — is
//! run after it once per arming of the path, and after that a look asks the system whether the
//! question has been answered, without asking the user (`voiceover.rs`).
//!
//! Pure — no clock of its own, no Objective-C — so it runs under `cargo test` on Windows; the
//! transport it governs (`voiceover.rs`) runs only on a Mac.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use super::pace::next_look;

/// `errAEEventNotPermitted`: the one refusal that parks the path for the session.
pub(super) const NOT_PERMITTED: i64 = -1743;
/// `errAEEventWouldRequireUserConsent`: the Automation question has not been answered.
pub(super) const WOULD_REQUIRE_CONSENT: i64 = -1744;
/// `procNotFound`: VoiceOver is not running — it quit between the check before the line and
/// the event.
pub(super) const PROC_NOT_FOUND: i64 = -600;

/// Why VoiceOver would not take a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cause {
    /// This application is not allowed to send Apple Events to VoiceOver (-1743).
    NotPermitted,
    /// The child process did not come back within its limit and was stopped: the speech thread
    /// waited the whole limit for this refusal.
    Stalled,
    /// Anything else — VoiceOver quitting or starting, busy, the Automation question not
    /// answered yet.
    Other,
}

/// A line VoiceOver would not take: the Apple Event's error code when there was one, everything
/// said about it (the `osascript` child's error output included, which carries its own code in
/// parentheses, `… (-1743)`), and whether the child had to be stopped at its limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Refusal {
    pub code: Option<i64>,
    pub text: String,
    pub stalled: bool,
}

impl Refusal {
    pub(super) fn cause(&self) -> Cause {
        if self.code == Some(NOT_PERMITTED) || codes_in(&self.text).contains(&NOT_PERMITTED) {
            Cause::NotPermitted
        } else if self.stalled {
            Cause::Stalled
        } else {
            Cause::Other
        }
    }

    /// What tells one kind of refusal from another, for the log: the codes and whether it
    /// stalled, or — with neither — the words. A long run of refusals is written about when
    /// this changes, so a new error inside it is not hidden behind the first.
    fn key(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.code.hash(&mut h);
        self.stalled.hash(&mut h);
        let codes = codes_in(&self.text);
        if self.code.is_none() && codes.is_empty() {
            self.text.hash(&mut h);
        } else {
            codes.hash(&mut h);
        }
        h.finish()
    }
}

/// Every error code written in parentheses in `text` — `(-1743)` — in order. Only a
/// parenthesised negative number counts, so a figure elsewhere in a message is not a code.
/// Also how `vo_script` reads VoiceOver's answer to its AppleScript question.
pub(super) fn codes_in(text: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("(-") {
        let after = &rest[at + 2..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && after.as_bytes().get(digits) == Some(&b')') {
            if let Ok(n) = after[..digits].parse::<i64>() {
                out.push(-n);
            }
        }
        rest = &rest[at + 2..];
    }
    out
}

/// What the transport does with a line after VoiceOver's Apple Event was refused with `code`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rung {
    /// Nothing more: the refusal stands, and the line goes to the system voice now.
    Stop,
    /// The same line through `osascript`, a child process.
    Child,
}

/// Whether a line whose Apple Event came back `code` goes on to `osascript`. `may_prompt` is
/// asked only for -1744 and takes the one allowance per arming of the path (the start of the
/// session, and each tick of the setting).
///
/// - **-1743**: the permission is refused, and `osascript` is refused the same way — the same
///   application asks, through its child.
/// - **-600**: VoiceOver is not running, and `tell application "VoiceOver"` in a child would
///   **start** it — a screen reader switched on for somebody who had not asked for one.
/// - **-1744**: the question has not been answered. `osascript` would put it on screen and wait
///   for it until it is stopped, five seconds on the speech thread; once per arming that is how
///   the question is asked at all, and after that it is not asked again on every look.
/// - Anything else: the event may be what is broken, and the child is the rung that already
///   worked before the event did.
pub(super) fn after_event(code: Option<i64>, may_prompt: impl FnOnce() -> bool) -> Rung {
    match code {
        Some(NOT_PERMITTED) | Some(PROC_NOT_FOUND) => Rung::Stop,
        Some(WOULD_REQUIRE_CONSENT) if !may_prompt() => Rung::Stop,
        _ => Rung::Child,
    }
}

/// Whether `osascript` taking a line the Apple Event was refused with `code` says the event
/// itself does not work here, so that every later line should go through the child. Not for a
/// question of permission: a child that took the line after -1744 took it because the question
/// was answered meanwhile, and the event works from then on as well.
pub(super) fn event_broken(code: Option<i64>) -> bool {
    !matches!(code, Some(NOT_PERMITTED) | Some(WOULD_REQUIRE_CONSENT) | Some(PROC_NOT_FOUND))
}

/// One run of failures, from the first to the line VoiceOver takes again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Episode {
    since: Instant,
    failures: u32,
    /// The wait chosen after the last failure, so a change of pace can be said once.
    nap: Duration,
    /// A refusal in this run stalled: the slow pace from then on.
    slow: bool,
    /// The kind of refusal last written about ([`Refusal::key`]).
    said: u64,
}

impl Episode {
    fn pace(&self, now: Instant, running: bool) -> Duration {
        if self.slow {
            slow_pace(running)
        } else {
            next_look(now.saturating_duration_since(self.since), running)
        }
    }
}

/// The pace a run of refusals ends up at anyway: 10 s while VoiceOver runs, 30 s while it does
/// not.
fn slow_pace(running: bool) -> Duration {
    next_look(Duration::MAX, running)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Taking lines.
    Open,
    /// A look: the next line is handed to VoiceOver — only that one (`handed`) — and the answer
    /// is not in yet.
    Trying { episode: Episode, handed: bool },
    /// Not handed lines until `due`, VoiceOver running as another process than `pid`, or a
    /// wake.
    Waiting { episode: Episode, due: Instant, pid: Option<i32> },
    /// Not permitted: parked until the setting is ticked again.
    Refused,
}

/// Whether VoiceOver is handed lines, and when it is tried again after it would not take one.
/// Every method that changes the answer says so in its return value; the caller keeps the
/// transport's gate (`healthy`) in step with [`Park::open`] and writes the line.
#[derive(Debug)]
pub(super) struct Park {
    state: State,
}

impl Default for Park {
    fn default() -> Self {
        Park { state: State::Open }
    }
}

/// The schedule, in the words every line that promises it uses.
const SCHEDULE: &str = "every 3 s for five minutes and then every 10 s while VoiceOver runs, every \
                        3 s for a minute and then every 30 s while it does not, and at once when \
                        VoiceOver starts again or the Mac wakes";

/// The same for a run of refusals that stalled.
const SLOW_SCHEDULE: &str = "every 10 s while VoiceOver runs and every 30 s while it does not, \
                             since that refusal kept the speech thread waiting, and at once when \
                             VoiceOver starts again or the Mac wakes";

impl Park {
    /// Whether lines may go to VoiceOver.
    pub(super) fn open(&self) -> bool {
        matches!(self.state, State::Open | State::Trying { handed: false, .. })
    }

    /// A line is being handed to VoiceOver. `true` when it is a look's one line: the gate goes
    /// down behind it until VoiceOver has answered it.
    pub(super) fn handed(&mut self) -> bool {
        match self.state {
            State::Trying { episode, handed: false } => {
                self.state = State::Trying { episode, handed: true };
                true
            }
            _ => false,
        }
    }

    /// A line was refused (`r`), seen at `now` while VoiceOver runs as `pid`. Returns the line
    /// to log when this is news: the first refusal, a change of pace, a new kind of refusal.
    pub(super) fn failed(&mut self, r: &Refusal, now: Instant, pid: Option<i32>) -> Option<String> {
        let cause = r.cause();
        let why = r.text.as_str();
        if cause == Cause::NotPermitted {
            let news = self.state != State::Refused;
            self.state = State::Refused;
            return news.then(|| {
                format!(
                    "VoiceOver would not take a line: this application is not allowed to send it \
                     Apple Events ({why}). The system voice speaks instead until \"Speak through \
                     VoiceOver\" is switched off and on again, which asks for the permission; it \
                     is under Privacy & Security, Automation."
                )
            });
        }
        let running = pid.is_some();
        let key = r.key();
        match self.state {
            State::Refused => None,
            // A line handed over before the path was parked, failing after it: counted, and the
            // look stays when it was.
            State::Waiting { ref mut episode, .. } => {
                episode.failures += 1;
                episode.slow |= cause == Cause::Stalled;
                changed(episode, key, why)
            }
            State::Open => {
                let slow = cause == Cause::Stalled;
                let nap = if slow { slow_pace(running) } else { next_look(Duration::ZERO, running) };
                let episode = Episode { since: now, failures: 1, nap, slow, said: key };
                self.state = State::Waiting { episode, due: now + nap, pid };
                Some(format!(
                    "VoiceOver would not take a line ({why}), so the system voice says it. \
                     VoiceOver is tried again in {} s — {}.{}",
                    nap.as_secs(),
                    if slow { SLOW_SCHEDULE } else { SCHEDULE },
                    hint(r, running)
                ))
            }
            State::Trying { mut episode, .. } => {
                episode.failures += 1;
                episode.slow |= cause == Cause::Stalled;
                let nap = episode.pace(now, running);
                let slowed = nap != episode.nap;
                episode.nap = nap;
                let new_kind = changed(&mut episode, key, why);
                self.state = State::Waiting { episode, due: now + nap, pid };
                if slowed {
                    Some(format!(
                        "VoiceOver still would not take a line after {} tries over {} s ({why}); \
                         it is tried again every {} s from now on",
                        episode.failures,
                        now.saturating_duration_since(episode.since).as_secs(),
                        nap.as_secs()
                    ))
                } else {
                    new_kind
                }
            }
        }
    }

    /// VoiceOver took a line after failing. Returns the line to log when it had been failing.
    pub(super) fn spoke(&mut self, now: Instant) -> Option<String> {
        let was = std::mem::replace(&mut self.state, State::Open);
        match was {
            State::Trying { episode, .. } | State::Waiting { episode, .. } => Some(format!(
                "VoiceOver took a line again, after {} failed {} over {} s; lines go to VoiceOver \
                 again",
                episode.failures,
                if episode.failures == 1 { "try" } else { "tries" },
                now.saturating_duration_since(episode.since).as_secs()
            )),
            State::Refused => Some(
                "VoiceOver took a line again after refusing this application's Apple Events — the \
                 permission was given since; lines go to VoiceOver again"
                    .to_string(),
            ),
            State::Open => None,
        }
    }

    /// Whether the next look has come due at `now`. When it has, the path is open again and
    /// `true` comes back.
    pub(super) fn due(&mut self, now: Instant) -> bool {
        match self.state {
            State::Waiting { episode, due, .. } if now >= due => {
                self.state = State::Trying { episode, handed: false };
                true
            }
            _ => false,
        }
    }

    /// VoiceOver is running as `pid` now (`None`: not running). A waiting path whose VoiceOver
    /// is a new process — started, or restarted — opens at once. Returns the line to log when
    /// it did.
    pub(super) fn pid_seen(&mut self, pid: Option<i32>) -> Option<String> {
        let State::Waiting { episode, pid: before, .. } = self.state else {
            return None;
        };
        let now_pid = pid?;
        if before == Some(now_pid) {
            return None;
        }
        self.state = State::Trying { episode, handed: false };
        Some(match before {
            Some(old) => format!(
                "VoiceOver runs as a new process (pid {old}, now {now_pid}); it is tried again at once"
            ),
            None => format!("VoiceOver started (pid {now_pid}); it is tried again at once"),
        })
    }

    /// The Mac woke, or the screen was unlocked, or this session came back: a waiting path
    /// opens at once. `true` when it did.
    pub(super) fn wake(&mut self) -> bool {
        match self.state {
            State::Waiting { episode, .. } => {
                self.state = State::Trying { episode, handed: false };
                true
            }
            _ => false,
        }
    }

    /// The setting was ticked again: open, whatever was parked, the permission included.
    pub(super) fn reset(&mut self) {
        self.state = State::Open;
    }
}

/// What the first line of a run of refusals adds about where to look.
fn hint(r: &Refusal, running: bool) -> &'static str {
    if r.code == Some(WOULD_REQUIRE_CONSENT) {
        " The Automation question for VoiceOver has not been answered yet; it is under Privacy & \
         Security, Automation."
    } else if running {
        " A refusal that keeps coming back while VoiceOver runs can be \"Allow VoiceOver to be \
         controlled with AppleScript\" unticked in VoiceOver Utility's General pane."
    } else {
        ""
    }
}

/// The line for a refusal of another kind than the last one written about in `episode`, which
/// then remembers it; `None` when it is the same kind.
fn changed(episode: &mut Episode, key: u64, why: &str) -> Option<String> {
    if episode.said == key {
        return None;
    }
    episode.said = key;
    Some(format!(
        "VoiceOver now refuses differently ({why}); it is still tried again every {} s",
        episode.nap.as_secs()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn refusal(code: Option<i64>, text: &str) -> Refusal {
        Refusal { code, text: text.to_string(), stalled: false }
    }

    fn timed_out() -> Refusal {
        refusal(Some(-609), "connection invalid [-609]")
    }

    fn stalled() -> Refusal {
        Refusal {
            code: Some(-609),
            text: "connection invalid [-609]; through osascript: osascript did not come back within 5 s".to_string(),
            stalled: true,
        }
    }

    #[test]
    fn only_a_permission_refusal_is_not_permitted() {
        assert_eq!(refusal(Some(-1743), "The operation couldn’t be completed.").cause(), Cause::NotPermitted);
        assert_eq!(
            refusal(None, "execution error: Not authorized to send Apple events to VoiceOver. (-1743)").cause(),
            Cause::NotPermitted
        );
        assert_eq!(refusal(Some(-1744), "would require user consent").cause(), Cause::Other, "consent still being asked");
        assert_eq!(refusal(Some(-1712), "AppleEvent timed out").cause(), Cause::Other);
        assert_eq!(refusal(Some(-600), "procNotFound").cause(), Cause::Other);
        assert_eq!(refusal(None, "VoiceOver got an error: AppleEvent handler failed. (-10000)").cause(), Cause::Other);
        assert_eq!(stalled().cause(), Cause::Stalled);
        // A permission refusal stays one even when the child also stalled.
        let both = Refusal { code: Some(-1743), text: String::new(), stalled: true };
        assert_eq!(both.cause(), Cause::NotPermitted);
        // A number that merely contains the code is not the code.
        assert_eq!(refusal(None, "error (-17430)").cause(), Cause::Other);
        assert_eq!(refusal(None, "error (1743)").cause(), Cause::Other);
        assert_eq!(refusal(None, "code 21743 (-1743x)").cause(), Cause::Other);
        assert_eq!(refusal(None, "(-1743)").cause(), Cause::NotPermitted);
        assert_eq!(codes_in("a (-10000) b (-1743) (12) (-) (-5"), vec![-10000, -1743]);
    }

    /// The rungs after a refused event: the child only where it can help, and after -1744 only
    /// once per arming.
    #[test]
    fn the_child_process_follows_the_event_only_where_it_can_help() {
        let never = || -> bool { panic!("asked about the prompt for a code that is not -1744") };
        assert_eq!(after_event(Some(-1743), never), Rung::Stop, "refused: the child is refused too");
        assert_eq!(after_event(Some(-600), never), Rung::Stop, "not running: the child would start VoiceOver");
        assert_eq!(after_event(Some(-609), never), Rung::Child);
        assert_eq!(after_event(None, never), Rung::Child);
        let allowance = std::cell::Cell::new(true);
        let take = || allowance.replace(false);
        assert_eq!(after_event(Some(-1744), take), Rung::Child, "the first -1744 of an arming asks");
        assert_eq!(after_event(Some(-1744), take), Rung::Stop, "later ones do not wait on the question again");
        // Only a refusal that says nothing about permission switches the transport.
        assert!(event_broken(Some(-609)));
        assert!(event_broken(None));
        assert!(!event_broken(Some(-1744)), "the question was answered meanwhile; the event works now");
        assert!(!event_broken(Some(-1743)));
    }

    #[test]
    fn a_refusal_that_is_not_the_permission_is_tried_again_on_prism_s_schedule() {
        let t0 = Instant::now();
        let mut park = Park::default();
        assert!(park.open());
        assert!(!park.handed(), "an open path hands lines freely");
        let line = park.failed(&timed_out(), t0, Some(400)).expect("news");
        assert!(line.contains("tried again in 3 s"), "{line}");
        assert!(line.contains("AppleScript"), "running, so the likeliest cause is named: {line}");
        assert!(!park.open());
        assert!(!park.due(t0 + secs(2)));
        assert!(park.due(t0 + secs(3)), "the first look, 3 s after");
        assert!(park.open());
        // Refused again, 3 s in: the next look 3 s later, and nothing new to say.
        assert_eq!(park.failed(&timed_out(), t0 + secs(3), Some(400)), None);
        assert!(!park.due(t0 + secs(5)));
        assert!(park.due(t0 + secs(6)));
        // Five minutes of refusals while VoiceOver runs: the pace drops to 10 s, said once.
        let line = park.failed(&timed_out(), t0 + secs(301), Some(400)).expect("slower");
        assert!(line.contains("every 10 s"), "{line}");
        assert!(!park.due(t0 + secs(310)));
        assert!(park.due(t0 + secs(311)));
        assert_eq!(park.failed(&timed_out(), t0 + secs(311), Some(400)), None, "same pace");
        assert!(park.due(t0 + secs(321)));
        let line = park.spoke(t0 + secs(322)).expect("back");
        assert!(line.contains("after 4 failed tries over 322 s"), "{line}");
        assert!(park.open());
        assert_eq!(park.spoke(t0 + secs(330)), None, "nothing to say while it works");
    }

    /// A look lets exactly one line through; the rest go to the system voice until it is
    /// answered, one way or the other.
    #[test]
    fn a_look_hands_one_line_and_closes_behind_it() {
        let t0 = Instant::now();
        let mut park = Park::default();
        park.failed(&timed_out(), t0, Some(400));
        assert!(park.due(t0 + secs(3)));
        assert!(park.open());
        assert!(park.handed(), "the look's line");
        assert!(!park.open(), "closed behind it");
        assert!(!park.handed());
        assert!(!park.due(t0 + secs(60)), "a look under way is not due again");
        assert!(!park.wake(), "nor opened again by a wake");
        assert_eq!(park.pid_seen(Some(512)), None, "nor by a new VoiceOver");
        // Refused: waiting again; the next look hands one line again.
        assert_eq!(park.failed(&timed_out(), t0 + secs(4), Some(400)), None);
        assert!(!park.open());
        assert!(park.due(t0 + secs(7)));
        assert!(park.handed());
        // Taken: open, and lines flow freely again.
        assert!(park.spoke(t0 + secs(8)).is_some());
        assert!(park.open());
        assert!(!park.handed());
        assert!(park.open());
    }

    /// A refusal that kept the speech thread waiting for the child's whole limit is not paid
    /// every 3 s: its run of refusals goes to the slow pace at once, and stays there.
    #[test]
    fn a_refusal_that_stalled_goes_to_the_slow_pace_at_once() {
        let t0 = Instant::now();
        let mut park = Park::default();
        let line = park.failed(&stalled(), t0, Some(400)).expect("news");
        assert!(line.contains("tried again in 10 s"), "{line}");
        assert!(line.contains("kept the speech thread waiting"), "{line}");
        assert!(!park.due(t0 + secs(9)));
        assert!(park.due(t0 + secs(10)));
        // A quick refusal later in the same run does not bring the fast pace back.
        park.handed();
        let line = park.failed(&timed_out(), t0 + secs(11), Some(400)).expect("another kind");
        assert!(line.contains("refuses differently"), "{line}");
        assert!(!park.due(t0 + secs(20)));
        assert!(park.due(t0 + secs(21)));
        // Not running: thirty seconds.
        let mut park = Park::default();
        assert!(park.failed(&stalled(), t0, None).expect("news").contains("tried again in 30 s"));
        // The Automation question left open: the line says where it is, not AppleScript.
        let mut park = Park::default();
        let consent = Refusal {
            code: Some(-1744),
            text: "[-1744]; through osascript: osascript did not come back within 5 s".to_string(),
            stalled: true,
        };
        let line = park.failed(&consent, t0, Some(400)).expect("news");
        assert!(line.contains("Automation question") && !line.contains("AppleScript"), "{line}");
        assert!(line.contains("tried again in 10 s"), "{line}");
        // A run that stalls part-way slows from then on, and says so.
        let mut park = Park::default();
        park.failed(&timed_out(), t0, Some(400));
        assert!(park.due(t0 + secs(3)));
        let line = park.failed(&stalled(), t0 + secs(8), Some(400)).expect("slower");
        assert!(line.contains("every 10 s from now on"), "{line}");
        assert!(!park.due(t0 + secs(17)));
        assert!(park.due(t0 + secs(18)));
    }

    /// A long run of refusals says so when the refusal changes — a new error is not hidden
    /// behind the first — and only then.
    #[test]
    fn a_new_kind_of_refusal_is_written_once() {
        let t0 = Instant::now();
        let mut park = Park::default();
        park.failed(&refusal(None, "VoiceOver got an error (-10000)"), t0, Some(400));
        assert!(park.due(t0 + secs(3)));
        assert_eq!(park.failed(&refusal(None, "VoiceOver got an error (-10000)"), t0 + secs(3), Some(400)), None);
        assert!(park.due(t0 + secs(6)));
        let line = park.failed(&refusal(Some(-1744), "consent [-1744]"), t0 + secs(6), Some(400)).expect("new");
        assert!(line.contains("refuses differently (consent [-1744])"), "{line}");
        assert!(line.contains("every 3 s"), "{line}");
        assert!(park.due(t0 + secs(9)));
        assert_eq!(park.failed(&refusal(Some(-1744), "consent [-1744]"), t0 + secs(9), Some(400)), None);
        // Same codes, other words (a figure in the text): the same kind.
        assert!(park.due(t0 + secs(12)));
        assert_eq!(park.failed(&refusal(Some(-1744), "consent, 2 s [-1744]"), t0 + secs(12), Some(400)), None);
        // No code at all: the words decide.
        let a = refusal(None, "osascript could not be started (No such file)");
        let b = refusal(None, "lost track of osascript (Interrupted)");
        assert_ne!(a.key(), b.key());
        assert_eq!(a.key(), a.clone().key());
    }

    #[test]
    fn while_voiceover_is_not_running_the_slow_pace_is_thirty_seconds() {
        let t0 = Instant::now();
        let mut park = Park::default();
        let line = park.failed(&refusal(Some(-600), "procNotFound [-600]"), t0, None).expect("news");
        assert!(!line.contains("AppleScript"), "not running: the AppleScript hint does not apply: {line}");
        assert!(park.due(t0 + secs(3)));
        let line = park.failed(&refusal(Some(-600), "procNotFound [-600]"), t0 + secs(61), None).expect("slower");
        assert!(line.contains("every 30 s"), "{line}");
        assert!(!park.due(t0 + secs(90)));
        assert!(park.due(t0 + secs(91)));
    }

    #[test]
    fn a_permission_refusal_stays_parked_until_the_setting_is_ticked() {
        let t0 = Instant::now();
        let mut park = Park::default();
        let line = park.failed(&refusal(Some(-1743), "-1743"), t0, Some(400)).expect("news");
        assert!(line.contains("Automation"), "{line}");
        assert!(!park.open());
        assert!(!park.due(t0 + secs(3600)), "no schedule");
        assert!(!park.wake(), "a wake does not change a permission");
        assert_eq!(park.pid_seen(Some(401)), None, "nor does a new VoiceOver");
        assert_eq!(park.failed(&refusal(Some(-1743), "-1743"), t0, Some(401)), None, "said once");
        assert!(!park.open());
        park.reset();
        assert!(park.open());
    }

    #[test]
    fn a_new_voiceover_process_or_a_wake_is_tried_at_once() {
        let t0 = Instant::now();
        let mut park = Park::default();
        park.failed(&timed_out(), t0, Some(400));
        assert_eq!(park.pid_seen(Some(400)), None, "the same process");
        assert_eq!(park.pid_seen(None), None, "not running: nothing to try");
        let line = park.pid_seen(Some(512)).expect("restarted");
        assert!(line.contains("pid 400, now 512"), "{line}");
        assert!(park.open());

        park.failed(&timed_out(), t0 + secs(1), None);
        let line = park.pid_seen(Some(600)).expect("started");
        assert!(line.contains("VoiceOver started (pid 600)"), "{line}");

        park.failed(&timed_out(), t0 + secs(2), Some(600));
        assert!(park.wake());
        assert!(park.open());
        assert!(!park.wake(), "only a waiting path opens");
    }

    #[test]
    fn failures_of_lines_already_handed_over_count_without_moving_the_look() {
        let t0 = Instant::now();
        let mut park = Park::default();
        park.failed(&timed_out(), t0, Some(400));
        assert_eq!(park.failed(&timed_out(), t0 + secs(2), Some(400)), None);
        assert!(park.due(t0 + secs(3)), "the look is where the first failure put it");
        let line = park.spoke(t0 + secs(4)).expect("back");
        assert!(line.contains("after 2 failed tries"), "{line}");
    }

    #[test]
    fn a_line_taken_while_parked_opens_the_path() {
        let t0 = Instant::now();
        let mut park = Park::default();
        park.failed(&timed_out(), t0, Some(400));
        let line = park.spoke(t0 + secs(1)).expect("back");
        assert!(line.contains("after 1 failed try over 1 s"), "{line}");
        assert!(park.open());
        park.failed(&refusal(Some(-1743), "-1743"), t0, Some(400));
        assert!(park.spoke(t0 + secs(2)).expect("granted since").contains("permission was given"));
        assert!(park.open());
    }
}
