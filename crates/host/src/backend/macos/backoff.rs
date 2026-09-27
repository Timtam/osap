//! A back-off for a way of capturing that did not answer: ScreenCaptureKit's rectangle capture,
//! which `capture.rs` waits for with a deadline.
//!
//! It replaced a switch that turned ScreenCaptureKit off for the rest of the session after one
//! capture missed its deadline — and the likeliest capture of a long session to miss it is the
//! one just after a wake or during a display reconfiguration, after which every OCR and pixel
//! read went to the older Core Graphics functions, or, on a macOS without them, nowhere, until
//! the application was restarted. Now a timeout keeps ScreenCaptureKit out for 30 seconds, the
//! next for a minute, doubling to ten minutes, and one probe — never two at once — asks it
//! again when that is over. Only an answer within the deadline ends the back-off.
//!
//! **A probe keeps nobody waiting where it can.** When an older capture function can answer
//! the read, the probe is sent without waiting for it ([`Attempt::Probe`]; `capture.rs` serves
//! the read from Core Graphics) and its answer is reported by the framework's own handler
//! ([`Backoff::probe_answered`]). A probe that has not answered after twice the deadline is
//! written off at the next [`Backoff::attempt`] — which is also what frees a probe whose caller
//! never came back to say how it went. So a ScreenCaptureKit that never answers again costs no
//! thread anything; only on a macOS without the older functions does the probing read wait,
//! because nothing else could answer it.
//!
//! **A wake or a display change brings the next probe forward** ([`Backoff::clear`]) — the
//! slow capture that started the back-off is the kind those moments produce, and the one after
//! them is likely to be answered. It does not end the back-off: once per step, and never sooner
//! than the first wait after the timeout that began the step, and the level stays where it was.
//! A display that flaps, or several wakes in a row, then costs at most one extra probe per
//! step, not a deadline on every capturing thread per flap.
//!
//! Pure — the clock is handed in — so its rules run under `cargo test` on Windows
//! (`backend/mod.rs` borrows this file for its tests).

use std::time::{Duration, Instant};

/// Whether to ask, now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attempt {
    /// Nothing is backing off: ask, and wait for the answer.
    Ask,
    /// The back-off is over, and this caller sends the probe that started at the `Instant` (its
    /// name when it answers, [`Backoff::probe_answered`]). Every other caller skips until it has
    /// been answered or written off.
    Probe(Instant),
    /// Backing off, or a probe is out: do not ask.
    Skip,
    /// Do not ask — and the probe that was out has now been written off without an answer:
    /// nothing asks for the `Duration` from now. Said once, by the caller that got it.
    Missed(Duration),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Open,
    /// Not asked until `until`; `wait` is this step's wait, which began with a timeout at
    /// `began`. `early`: a wake or a display change has already brought this step's probe
    /// forward.
    Waiting { until: Instant, wait: Duration, began: Instant, early: bool },
    /// One probe is out, sent at `since` after a step of `wait`.
    Probing { wait: Duration, since: Instant },
}

#[derive(Debug)]
pub(crate) struct Backoff {
    first: Duration,
    cap: Duration,
    /// How long an answer may take and still count as one.
    deadline: Duration,
    state: State,
}

impl Backoff {
    pub(crate) const fn new(first: Duration, cap: Duration, deadline: Duration) -> Self {
        Backoff { first, cap, deadline, state: State::Open }
    }

    fn step(&mut self, now: Instant, wait: Duration) -> Duration {
        self.state = State::Waiting { until: now + wait, wait, began: now, early: false };
        wait
    }

    fn longer(&self, wait: Duration) -> Duration {
        wait.saturating_mul(2).min(self.cap)
    }

    /// Whether to ask at `now`.
    pub(crate) fn attempt(&mut self, now: Instant) -> Attempt {
        match self.state {
            State::Open => Attempt::Ask,
            State::Waiting { until, wait, .. } if now >= until => {
                self.state = State::Probing { wait, since: now };
                Attempt::Probe(now)
            }
            // Twice the deadline and no word: the probe did not answer in time, or its caller
            // never came back to say so. Written off as a timeout of the probe.
            State::Probing { wait, since } if now >= since + self.deadline.saturating_mul(2) => {
                let longer = self.longer(wait);
                Attempt::Missed(self.step(now, longer))
            }
            State::Waiting { .. } | State::Probing { .. } => Attempt::Skip,
        }
    }

    /// An answer within the deadline — a picture or an error — to a capture that was waited
    /// for (`Ask`, or a probe nothing else could answer). `true` when that ends a back-off.
    pub(crate) fn answered(&mut self) -> bool {
        let was = self.state;
        self.state = State::Open;
        was != State::Open
    }

    /// The probe sent at `since` answered at `now` — from the framework's handler, on its own
    /// queue. Ends the back-off when it is the probe still out and the answer is within the
    /// deadline; `true` when it did. A late answer, or one to a probe already written off,
    /// changes nothing: the rule is the deadline, whichever thread would have been waiting.
    pub(crate) fn probe_answered(&mut self, since: Instant, now: Instant) -> bool {
        match self.state {
            State::Probing { since: out, .. } if out == since && now <= since + self.deadline => {
                self.state = State::Open;
                true
            }
            _ => false,
        }
    }

    /// A capture asked at `attempt`'s word did not answer in time, at `now`. Returns how long
    /// nothing asks from now when this timeout started the back-off or lengthened it; `None`
    /// when another caller's timeout already did (several captures waiting at once all time out
    /// together, and that is one outage, not several).
    pub(crate) fn timed_out(&mut self, now: Instant) -> Option<Duration> {
        let wait = match self.state {
            State::Open => self.first,
            State::Probing { wait, .. } => self.longer(wait),
            State::Waiting { .. } => return None,
        };
        Some(self.step(now, wait))
    }

    /// A wake or a display change at `now`: the next probe comes at once, if this step has not
    /// had its probe brought forward yet and the first wait since the step began is over (or
    /// when that is over). The level stays. Returns this step's wait when the probe moved.
    pub(crate) fn clear(&mut self, now: Instant) -> Option<Duration> {
        let State::Waiting { until, wait, began, early: false } = self.state else {
            return None;
        };
        let at = now.max(began + self.first);
        if at >= until {
            return None;
        }
        self.state = State::Waiting { until: at, wait, began, early: true };
        Some(wait)
    }

    /// Whether ScreenCaptureKit is being kept out right now (waiting or probing).
    #[cfg(test)]
    pub(crate) fn backing_off(&self) -> bool {
        self.state != State::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: Duration = Duration::from_secs(30);
    const CAP: Duration = Duration::from_secs(600);
    const DEADLINE: Duration = Duration::from_millis(1500);

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn backoff() -> Backoff {
        Backoff::new(FIRST, CAP, DEADLINE)
    }

    fn probe(b: &mut Backoff, now: Instant) -> Instant {
        match b.attempt(now) {
            Attempt::Probe(since) => since,
            other => panic!("expected a probe at {:?}, got {other:?}", now),
        }
    }

    #[test]
    fn a_timeout_keeps_it_out_for_thirty_seconds_then_one_caller_asks() {
        let t0 = Instant::now();
        let mut b = backoff();
        assert_eq!(b.attempt(t0), Attempt::Ask);
        assert_eq!(b.timed_out(t0), Some(secs(30)));
        assert!(b.backing_off());
        assert_eq!(b.attempt(t0 + secs(29)), Attempt::Skip);
        assert_eq!(b.attempt(t0 + secs(30)), Attempt::Probe(t0 + secs(30)));
        assert_eq!(b.attempt(t0 + secs(30)), Attempt::Skip, "one probe at a time");
        assert!(b.answered(), "the probe's answer ends the back-off");
        assert!(!b.backing_off());
        assert_eq!(b.attempt(t0 + secs(31)), Attempt::Ask);
        assert!(!b.answered(), "nothing to end");
    }

    #[test]
    fn each_probe_that_times_out_doubles_the_wait_up_to_ten_minutes() {
        let mut now = Instant::now();
        let mut b = backoff();
        assert_eq!(b.attempt(now), Attempt::Ask);
        let mut waits = vec![b.timed_out(now).unwrap()];
        for _ in 0..6 {
            now += *waits.last().unwrap();
            probe(&mut b, now);
            waits.push(b.timed_out(now).unwrap());
        }
        assert_eq!(waits, [30, 60, 120, 240, 480, 600, 600].map(secs));
    }

    #[test]
    fn captures_that_time_out_together_are_one_outage() {
        let t0 = Instant::now();
        let mut b = backoff();
        // Three threads asked while it was open; all three time out.
        assert_eq!(b.attempt(t0), Attempt::Ask);
        assert_eq!(b.attempt(t0), Attempt::Ask);
        assert_eq!(b.attempt(t0), Attempt::Ask);
        assert_eq!(b.timed_out(t0 + secs(1)), Some(secs(30)));
        assert_eq!(b.timed_out(t0 + secs(1)), None);
        assert_eq!(b.timed_out(t0 + secs(2)), None);
        assert!(matches!(b.attempt(t0 + secs(31)), Attempt::Probe(_)), "the first timeout's thirty seconds");
    }

    /// A probe nobody waits for: its answer comes from the handler, and only an answer within
    /// the deadline counts. One that never comes is written off at twice the deadline, and the
    /// wait doubles as for any probe that timed out.
    #[test]
    fn a_probe_nobody_waits_for_is_answered_or_written_off() {
        let t0 = Instant::now();
        let mut b = backoff();
        b.timed_out(t0);
        let p = probe(&mut b, t0 + secs(30));
        assert!(!b.probe_answered(p + secs(2), p + secs(2)), "another probe's answer");
        assert!(b.probe_answered(p, p + Duration::from_millis(900)), "in time");
        assert!(!b.backing_off());

        let mut b = backoff();
        b.timed_out(t0);
        let p = probe(&mut b, t0 + secs(30));
        assert!(!b.probe_answered(p, p + secs(2)), "late: the deadline is the rule");
        assert!(b.backing_off());
        assert_eq!(b.attempt(p + secs(2)), Attempt::Skip, "not written off before twice the deadline");
        assert_eq!(b.attempt(p + secs(3)), Attempt::Missed(secs(60)), "written off, the wait doubled");
        assert_eq!(b.attempt(p + secs(4)), Attempt::Skip, "said once");
        assert!(!b.probe_answered(p, p + secs(5)), "an answer after the write-off changes nothing");
        assert!(matches!(b.attempt(p + secs(63)), Attempt::Probe(_)));
    }

    /// A probe whose caller never said how it went — it unwound, say — does not keep
    /// ScreenCaptureKit out for the rest of the session.
    #[test]
    fn a_probe_that_never_reports_is_written_off() {
        let t0 = Instant::now();
        let mut b = backoff();
        b.timed_out(t0);
        probe(&mut b, t0 + secs(30));
        assert_eq!(b.attempt(t0 + secs(33)), Attempt::Missed(secs(60)));
        // A waiting caller that times out after the write-off is the same outage.
        assert_eq!(b.timed_out(t0 + secs(34)), None);
    }

    /// A wake or a display change brings the next probe forward — once per step, not before the
    /// first wait since the step began, and at the level the step had.
    #[test]
    fn a_wake_brings_the_next_probe_forward_once_a_step() {
        let t0 = Instant::now();
        let mut b = backoff();
        assert_eq!(b.clear(t0), None, "nothing backing off");
        b.timed_out(t0);
        assert_eq!(b.clear(t0 + secs(5)), None, "the first step's probe is due by then anyway");
        let p = probe(&mut b, t0 + secs(30));
        assert_eq!(b.clear(p), None, "a probe is out");
        b.timed_out(p + secs(1)); // step 2: 60 s, began at t0 + 31
        let began = p + secs(1);
        // A wake 10 s in: the probe comes when the first wait since the step began is over.
        assert_eq!(b.clear(began + secs(10)), Some(secs(60)));
        assert_eq!(b.attempt(began + secs(29)), Attempt::Skip);
        assert!(matches!(b.attempt(began + secs(30)), Attempt::Probe(_)), "brought forward to 30 s");
        // It times out: the level doubles as for any probe, to 120 s.
        assert_eq!(b.timed_out(began + secs(31)), Some(secs(120)));
        let began = began + secs(31);
        // A wake long into the step brings it forward to now, once.
        assert_eq!(b.clear(began + secs(40)), Some(secs(120)));
        assert_eq!(b.clear(began + secs(41)), None, "once per step: a flapping display asks no more");
        assert!(matches!(b.attempt(began + secs(41)), Attempt::Probe(_)));
        // Answered: open, and nothing to clear.
        assert!(b.answered());
        assert_eq!(b.clear(began + secs(42)), None);
        assert_eq!(b.attempt(began + secs(42)), Attempt::Ask);
    }
}
