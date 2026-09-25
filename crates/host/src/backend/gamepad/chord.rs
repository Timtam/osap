//! Button combinations on a game controller — "chords" — as a trigger: the rule, as pure code.
//!
//! A chord is a set of at least two buttons. It fires ONCE when the whole set is held on one
//! pad, at the press that completes it — in whatever order the buttons went down — and again
//! only after a button of the set has been let go and pressed again. That is the keyboard's
//! rule for a hotkey with modifiers, carried over: what counts is what is held at the moment
//! the last button goes down, not the order of the presses before it, and holding the
//! combination does not repeat it.
//!
//! **What "held" means here** is the hub's own record of the pad, carried in every button
//! event as [`PadEvent::held`]: the buttons held right after the report the press came in. So
//! a button that was already down when the pad was plugged in, when the Windows source woke
//! from a park, or before the listener was registered counts as held — it never produced a
//! press, but it is down, exactly as a Ctrl held before a hotkey was registered still makes
//! Ctrl+F10. Derived buttons (a stick pushed past half way, a trigger pulled) are buttons like
//! the others, both as members of a set and as "other buttons" for [`exact`](Chord::exact).
//!
//! **Why no "fired, wait for a release" flag.** A completion is a press of a member after
//! which the whole set is held. A member cannot go down again without going up first, so the
//! rule fires once per holding by itself — the only repeat to stop is inside ONE report, where
//! two members that went down together each carry the same complete `held` set. That is what
//! [`PadEvent::report`] is for. A flag that waited for the release would be wrong exactly when
//! the release is never reported: the Windows source parks while nobody listens for buttons
//! (a module disabled), takes the pad afresh as a baseline when it wakes, and a set let go
//! meanwhile would have left the flag set and the next real completion silent.
//!
//! **Nothing is claimed.** The game gets every press of the combination, as it gets every
//! press; a chord is a broadcast like every other controller event, and two listeners for the
//! same set both fire. A hold time ([`Chord::hold`]) is the answer to "the game uses this pair
//! too": a quick press does something in the game, a long one also reaches the module.
//!
//! **A hold is judged on the tick, and the hub is ahead of the pump.** The events a hold
//! depends on reach it through the pump, which takes them from the hub on its own schedule;
//! the hub's record of the pad is ahead of that. So [`Chord::due`] does not fire a hold while a release of a
//! member is still waiting in the hub ([`PadNow::releases_waiting`]) — the drain decides it,
//! by the release's own time: let go before the hold time was up, the hold is cancelled; let
//! go after it, the hold had been held out and fires on the next tick
//! ([`Chord::release`]). Otherwise a member let go and pressed again while a slow callback
//! held the loop would look to the tick like a set held throughout, and fire a hold that was
//! broken — and a set let go a moment after its time was up, before the tick got to it, would
//! be dropped.
//!
//! Kept free of Luau and of the host's state so it is compiled — and its tests run — on every
//! platform, macOS's type check included (`crates/macos-check` borrows the backend).

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use super::{Button, Family, PadEvent, PadEventKind};

/// Fewer than two is a button, and `down` is the event for a button.
pub const MIN_BUTTONS: usize = 2;

/// The two directions of each stick axis. The hub derives them from one value, which cannot
/// be past half way both ways at once, so a set with both of a pair could never be held.
const OPPOSITE: [(Button, Button); 4] = [
    (Button::LeftStickUp, Button::LeftStickDown),
    (Button::LeftStickLeft, Button::LeftStickRight),
    (Button::RightStickUp, Button::RightStickDown),
    (Button::RightStickLeft, Button::RightStickRight),
];

/// A chord waiting out its hold time on one pad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pending {
    /// When the press that completed the set was detected: the chord event's `time`.
    pub at: Instant,
    /// The pad's family, for the spoken labels.
    pub family: Family,
}

/// What the hub knows of one pad when a running hold is judged on the tick — taken under one
/// lock, so the two halves agree.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PadNow {
    /// Every button held on the pad now, physical and derived: the hub's record, which is
    /// ahead of what the pump has delivered.
    pub held: BTreeSet<Button>,
    /// The buttons whose release the pad reported and the pump has not taken from the hub yet.
    pub releases_waiting: BTreeSet<Button>,
}

/// One listener's combination and what it has done on each pad.
#[derive(Clone, Debug)]
pub struct Chord {
    /// The set, in the order the module named it, each button once.
    buttons: Vec<Button>,
    exact: bool,
    hold: Duration,
    /// The report in which the set was last completed on each pad — so the second member that
    /// went down in that same report is not a second completion. See the module notes.
    completed: HashMap<u8, u64>,
    /// The pads on which the set was completed and is waiting out `hold`.
    pending: HashMap<u8, Pending>,
    /// Holds that had run their full time when a member was let go, before the tick got to
    /// them: they fire on the next tick whatever is held then. See [`Chord::release`].
    held_out: Vec<(u8, Pending)>,
}

impl Chord {
    /// `buttons` must name at least [`MIN_BUTTONS`] buttons, and none twice: a repeated name is
    /// far more likely a slip than a meaning, and a set that shrank to one button would be a
    /// `down` listener nobody asked for. Nor two directions of one stick, which are never held
    /// together: the listener could never fire.
    pub fn new(buttons: Vec<Button>, exact: bool, hold: Duration) -> Result<Chord, String> {
        let name = super::names::button_name;
        let mut set: Vec<Button> = Vec::with_capacity(buttons.len());
        for b in buttons {
            if set.contains(&b) {
                return Err(format!("'{}' is named twice", name(b)));
            }
            set.push(b);
        }
        if set.len() < MIN_BUTTONS {
            return Err(format!(
                "a combination needs at least {MIN_BUTTONS} buttons, not {} — for one button, listen for \"down\"",
                set.len()
            ));
        }
        if let Some((a, b)) = OPPOSITE.iter().find(|(a, b)| set.contains(a) && set.contains(b)) {
            return Err(format!(
                "'{}' and '{}' are two directions of one stick, which are never held together",
                name(*a),
                name(*b)
            ));
        }
        Ok(Chord {
            buttons: set,
            exact,
            hold,
            completed: HashMap::new(),
            pending: HashMap::new(),
            held_out: Vec::new(),
        })
    }

    /// The set, in the order it was given.
    pub fn buttons(&self) -> &[Button] {
        &self.buttons
    }

    /// Whether any OTHER button held on the pad — derived ones included — stops the chord,
    /// both at the completing press and, for a hold, when the hold time is up.
    pub fn exact(&self) -> bool {
        self.exact
    }

    /// How long the whole set has to stay held after the completing press before the chord
    /// fires. Zero fires at the press.
    pub fn hold(&self) -> Duration {
        self.hold
    }

    /// Whether a hold is running, or held out and waiting for the tick, on any pad — for the
    /// tick, which has nothing to do otherwise.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.held_out.is_empty()
    }

    fn complete_in(&self, held: &BTreeSet<Button>) -> bool {
        self.buttons.iter().all(|b| held.contains(b))
    }

    /// For a `held` that already contains the set: whether nothing else is held, if that was
    /// asked for. The set has no repeats, so "nothing else" is a count.
    fn exact_in(&self, held: &BTreeSet<Button>) -> bool {
        !self.exact || held.len() == self.buttons.len()
    }

    /// A button event a source reported, `ev`, judged by the `held` set and the `report` it
    /// carries. `fresh` is whether it is young enough to act on (the listener's `maxAge`).
    ///
    /// Returns whether the chord fires NOW: `ev` is a press of a member, the whole set is held
    /// after it, no earlier press of the same report completed it already, the press is fresh,
    /// and — for `exact` — nothing else is held. With a hold time it never fires here: the
    /// completion is remembered, and [`Chord::due`] fires it once the time is up. A completion
    /// that is not fired — stale, or not exact — is simply not fired; it does not wait for a
    /// later press, which could only be a press of another button while the set stays held.
    pub fn press(&mut self, ev: &PadEvent, fresh: bool) -> bool {
        let PadEventKind::Down(b) = ev.kind else { return false };
        // Only a press of a member completes the set: pressing another button while the set
        // is already held is not "completing" anything, even for a listener that has not
        // fired yet because it was registered while the set was down.
        if !self.buttons.contains(&b) || !self.complete_in(&ev.held) {
            return false;
        }
        if self.completed.insert(ev.pad, ev.report) == Some(ev.report) {
            return false;
        }
        if !fresh || !self.exact_in(&ev.held) {
            return false;
        }
        if self.hold.is_zero() {
            return true;
        }
        self.pending.insert(ev.pad, Pending { at: ev.at, family: ev.family });
        false
    }

    /// A release a source reported, `ev` (anything but an `Up` is ignored). Letting go of a
    /// member ends a hold running on that pad. Let go before the hold time was up — by the
    /// release's own time, not by when the pump got to it — the hold is cancelled. Let go at or
    /// after it, the set was held out and the tick had not got to it yet: it fires on the next
    /// tick all the same, and for `exact` it is judged by what is held after this release
    /// (`ev.held`), the nearest record there is of the moment the time was up. Releases of
    /// other buttons change nothing.
    pub fn release(&mut self, ev: &PadEvent) {
        let PadEventKind::Up(b) = ev.kind else { return };
        if !self.buttons.contains(&b) {
            return;
        }
        let Some(p) = self.pending.remove(&ev.pad) else { return };
        let done = p.at.checked_add(self.hold).is_some_and(|end| ev.at >= end);
        if done && (!self.exact || ev.held.iter().all(|h| self.buttons.contains(h))) {
            self.held_out.push((ev.pad, p));
        }
    }

    /// The pad went away: nothing of it is remembered, and a hold running on it ends, one held
    /// out included.
    pub fn forget(&mut self, pad: u8) {
        self.completed.remove(&pad);
        self.pending.remove(&pad);
        self.held_out.retain(|(p, _)| *p != pad);
    }

    /// Cancels every running hold, and every one held out and not fired yet: for a module that
    /// was disabled. Enabled again, it fires at the next completion, not at one that happened
    /// while nobody was allowed to listen.
    pub fn cancel_holds(&mut self) {
        self.pending.clear();
        self.held_out.clear();
    }

    /// The holds to fire at `now`, in pad order.
    ///
    /// * Every hold held out ([`Chord::release`]) fires, whatever is held now.
    /// * A running hold whose time is up is left running while a release of a member is still
    ///   waiting in the hub (`pad_now(pad).releases_waiting`): the drain before the next tick
    ///   hands that release to [`Chord::release`], which cancels the hold or holds it out by the
    ///   release's own time. Every other one is taken out of the running holds whatever the
    ///   answer, and fires only if the whole set is held on that pad now (`pad_now(pad).held`;
    ///   `pad_now` is `None` for a pad that is gone) and, for `exact`, nothing else is.
    /// * Either kind is dropped instead when the moment its hold time ended is more than
    ///   `max_age` ago: a pump that stood still for seconds must not fire a combination the
    ///   player finished holding long before.
    ///
    /// One that is not fired is gone: the next completion — a button of the set let go and
    /// pressed again — starts a hold of its own.
    pub fn due(
        &mut self,
        now: Instant,
        max_age: Duration,
        pad_now: impl Fn(u8) -> Option<PadNow>,
    ) -> Vec<(u8, Pending)> {
        let hold = self.hold;
        let in_time = |p: &Pending| {
            let ended = p.at.checked_add(hold).unwrap_or(now);
            now.saturating_duration_since(ended) <= max_age
        };
        let mut out: Vec<(u8, Pending)> =
            std::mem::take(&mut self.held_out).into_iter().filter(|(_, p)| in_time(p)).collect();
        let ready: Vec<u8> = self
            .pending
            .iter()
            .filter(|(_, p)| now.saturating_duration_since(p.at) >= hold)
            .map(|(pad, _)| *pad)
            .collect();
        for pad in ready {
            let seen = pad_now(pad);
            if seen.as_ref().is_some_and(|n| self.buttons.iter().any(|b| n.releases_waiting.contains(b))) {
                continue;
            }
            let Some(p) = self.pending.remove(&pad) else { continue };
            let still = seen.is_some_and(|n| self.complete_in(&n.held) && self.exact_in(&n.held));
            if still && in_time(&p) {
                out.push((pad, p));
            }
        }
        out.sort_by_key(|(pad, p)| (*pad, p.at));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use Button::*;

    const X: Family = Family::Xbox;

    thread_local! {
        static REPORT: Cell<u64> = const { Cell::new(0) };
    }

    /// A report of its own, as the hub numbers them.
    fn next_report() -> u64 {
        REPORT.with(|r| {
            r.set(r.get() + 1);
            r.get()
        })
    }

    fn set(bs: &[Button]) -> BTreeSet<Button> {
        bs.iter().copied().collect()
    }

    /// A press of `b` on `pad` in a report of its own, after which `held` is held.
    fn down(pad: u8, b: Button, held: &[Button], at: Instant) -> PadEvent {
        down_in(next_report(), pad, b, held, at)
    }

    /// The same, in report `report`: several of these with one number came in one report.
    fn down_in(report: u64, pad: u8, b: Button, held: &[Button], at: Instant) -> PadEvent {
        PadEvent { pad, kind: PadEventKind::Down(b), at, synthetic: false, family: X, held: set(held), report }
    }

    /// A release of `b` on `pad` in a report of its own, after which `held` is held.
    fn up(pad: u8, b: Button, held: &[Button], at: Instant) -> PadEvent {
        PadEvent { kind: PadEventKind::Up(b), ..down(pad, b, held, at) }
    }

    fn chord(bs: &[Button]) -> Chord {
        Chord::new(bs.to_vec(), false, Duration::ZERO).unwrap()
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// The hub's answer for a pad holding `held`, with nothing of it waiting to be drained.
    fn settled(held: &[Button]) -> impl Fn(u8) -> Option<PadNow> {
        let held = set(held);
        move |_| Some(PadNow { held: held.clone(), releases_waiting: BTreeSet::new() })
    }

    #[test]
    fn it_needs_two_different_buttons() {
        assert!(Chord::new(vec![LeftShoulder], false, Duration::ZERO).unwrap_err().contains("at least 2"));
        assert!(Chord::new(vec![], false, Duration::ZERO).is_err());
        let e = Chord::new(vec![South, East, South], false, Duration::ZERO).unwrap_err();
        assert!(e.contains("'south' is named twice"), "{e}");
        let c = Chord::new(vec![RightShoulder, LeftShoulder], true, Duration::from_millis(600)).unwrap();
        assert_eq!(c.buttons(), &[RightShoulder, LeftShoulder], "the order given is kept");
        assert!(c.exact());
        assert_eq!(c.hold(), Duration::from_millis(600));
    }

    /// The hub derives the four directions of a stick from its two values, so both directions
    /// of one axis are never held together, and a set with both could never fire.
    #[test]
    fn two_directions_of_one_stick_axis_are_refused() {
        for (a, b, names) in [
            (LeftStickUp, LeftStickDown, "'left_stick_up' and 'left_stick_down'"),
            (LeftStickRight, LeftStickLeft, "'left_stick_left' and 'left_stick_right'"),
            (RightStickDown, RightStickUp, "'right_stick_up' and 'right_stick_down'"),
            (RightStickLeft, RightStickRight, "'right_stick_left' and 'right_stick_right'"),
        ] {
            let e = Chord::new(vec![South, a, b], false, Duration::ZERO).unwrap_err();
            assert!(e.contains(names) && e.contains("never held together"), "{e}");
        }
        // A diagonal holds two directions of different axes, and the two sticks are two sticks.
        assert!(Chord::new(vec![LeftStickUp, LeftStickLeft], false, Duration::ZERO).is_ok());
        assert!(Chord::new(vec![LeftStickUp, RightStickDown], false, Duration::ZERO).is_ok());
        // The D-pad is buttons: some pads do report up and down together.
        assert!(Chord::new(vec![DpadUp, DpadDown], false, Duration::ZERO).is_ok());
    }

    #[test]
    fn it_fires_once_at_the_completing_press_in_either_order() {
        let t = Instant::now();
        for (first, second) in [(LeftShoulder, RightShoulder), (RightShoulder, LeftShoulder)] {
            let mut c = chord(&[LeftShoulder, RightShoulder]);
            assert!(!c.press(&down(1, first, &[first], t), true), "half a chord");
            assert!(c.press(&down(1, second, &[LeftShoulder, RightShoulder], t), true), "{first:?} then {second:?}");
            // Pressing more while the set is held is not a second completion.
            assert!(!c.press(&down(1, South, &[LeftShoulder, RightShoulder, South], t), true));
        }
    }

    /// Two members down in one poll: both events carry the whole set, and the chord comes once,
    /// with the first of them.
    #[test]
    fn two_members_in_one_report_are_one_completion() {
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        let r = next_report();
        let both = [LeftShoulder, RightShoulder];
        assert!(c.press(&down_in(r, 1, LeftShoulder, &both, t), true));
        assert!(!c.press(&down_in(r, 1, RightShoulder, &both, t), true), "the same report");
        // The same numbers on another pad are that pad's own.
        assert!(c.press(&down_in(r, 2, LeftShoulder, &both, t), true));
    }

    #[test]
    fn letting_go_of_a_member_and_pressing_it_again_fires_again() {
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        c.press(&down(1, LeftShoulder, &[LeftShoulder], t), true);
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder], t), true));
        // South goes down and up while the set stays held: nothing.
        assert!(!c.press(&down(1, South, &[LeftShoulder, RightShoulder, South], t), true));
        c.release(&up(1, South, &[LeftShoulder, RightShoulder], t));
        c.release(&up(1, RightShoulder, &[LeftShoulder], t));
        assert!(!c.press(&down(1, South, &[LeftShoulder, South], t), true), "not a member");
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder, South], t), true), "RB again, LB still held");
    }

    /// The Windows source parks while nobody listens for buttons and takes the pad afresh when
    /// it wakes, so a release can happen without ever being reported. The next completion must
    /// fire all the same.
    #[test]
    fn a_release_that_was_never_reported_does_not_silence_the_next_completion() {
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder], t), true));
        // RB let go while parked: no release reaches the chord. Pressed again after the wake:
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder], t), true));
    }

    #[test]
    fn a_button_held_before_counts_as_held() {
        // LB was down before the pad was plugged in (or before the listener existed): no press
        // was ever seen for it, but the hub's record says it is held, and that is what counts.
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder], t), true));
    }

    #[test]
    fn a_press_of_another_button_does_not_complete_a_set_already_held() {
        // Registered while LB and RB were both down: pressing south is not the combination.
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        assert!(!c.press(&down(1, South, &[LeftShoulder, RightShoulder, South], t), true));
        // Letting go of RB and pressing it again is.
        c.release(&up(1, RightShoulder, &[LeftShoulder, South], t));
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder, South], t), true));
    }

    #[test]
    fn exact_refuses_other_buttons_held_derived_ones_included() {
        let t = Instant::now();
        let mut c = Chord::new(vec![LeftShoulder, RightShoulder], true, Duration::ZERO).unwrap();
        // The left stick leans past half way: a derived button, and it is held.
        assert!(!c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder, LeftStickDown], t), true));
        // The stick comes back; that is a release, not a press, so nothing fires until a member
        // is let go and pressed again.
        c.release(&up(1, LeftStickDown, &[LeftShoulder, RightShoulder], t));
        c.release(&up(1, LeftShoulder, &[RightShoulder], t));
        assert!(c.press(&down(1, LeftShoulder, &[LeftShoulder, RightShoulder], t), true));
        // Without `exact` the stick does not matter.
        let mut loose = chord(&[LeftShoulder, RightShoulder]);
        assert!(loose.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder, LeftStickDown], t), true));
    }

    #[test]
    fn a_stale_completion_is_not_fired_later() {
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        assert!(!c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder], t), false));
        // A later press while still held does not fire it after all...
        assert!(!c.press(&down(1, South, &[LeftShoulder, RightShoulder, South], t), true));
        // ...a member let go and pressed again does.
        c.release(&up(1, LeftShoulder, &[RightShoulder, South], t));
        assert!(c.press(&down(1, LeftShoulder, &[LeftShoulder, RightShoulder, South], t), true));
    }

    #[test]
    fn each_pad_is_its_own() {
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        // LB on pad 1 and RB on pad 2 is not a combination.
        assert!(!c.press(&down(1, LeftShoulder, &[LeftShoulder], t), true));
        assert!(!c.press(&down(2, RightShoulder, &[RightShoulder], t), true));
        assert!(c.press(&down(2, LeftShoulder, &[LeftShoulder, RightShoulder], t), true));
        assert!(c.press(&down(1, RightShoulder, &[LeftShoulder, RightShoulder], t), true));
        // A pad that goes away is forgotten, holds included.
        let mut h = Chord::new(vec![Back, Start], false, Duration::from_millis(100)).unwrap();
        h.press(&down(1, Start, &[Back, Start], t), true);
        h.forget(1);
        assert!(!h.has_pending());
    }

    #[test]
    fn only_a_down_is_a_press() {
        let t = Instant::now();
        let mut c = chord(&[LeftShoulder, RightShoulder]);
        let up = PadEvent { kind: PadEventKind::Up(South), ..down(1, South, &[LeftShoulder, RightShoulder], t) };
        assert!(!c.press(&up, true));
    }

    #[test]
    fn a_hold_fires_when_its_time_is_up_and_the_set_is_still_held() {
        let t = Instant::now();
        let mut c = Chord::new(vec![Back, Start], false, ms(600)).unwrap();
        assert!(!c.press(&down(1, Start, &[Back, Start], t), true), "a hold never fires at the press");
        assert!(c.has_pending());
        let max_age = Duration::from_secs(1);
        assert!(c.due(t + ms(590), max_age, settled(&[Back, Start])).is_empty(), "not yet");
        let fired = c.due(t + ms(612), max_age, settled(&[Back, Start]));
        assert_eq!(fired, vec![(1, Pending { at: t, family: X })], "`at` is the completing press");
        assert!(!c.has_pending());
        assert!(c.due(t + ms(700), max_age, settled(&[Back, Start])).is_empty(), "once");
        // Pressing more while held is nothing.
        assert!(!c.press(&down(1, South, &[Back, Start, South], t), true));
        assert!(!c.has_pending());
    }

    #[test]
    fn letting_go_during_the_hold_cancels_it() {
        let t = Instant::now();
        let mut c = Chord::new(vec![Back, Start], false, ms(600)).unwrap();
        c.press(&down(1, Start, &[Back, Start], t), true);
        c.release(&up(1, South, &[Back, Start], t + ms(100)));
        assert!(c.has_pending(), "another button's release changes nothing");
        c.release(&up(1, Back, &[Start], t + ms(599)));
        assert!(!c.has_pending());
        assert!(c.due(t + Duration::from_secs(1), Duration::from_secs(1), settled(&[Back, Start])).is_empty());
    }

    /// The review's race: Start let go at 790 ms of an 800 ms hold and pressed again at 850 ms,
    /// while a slow callback held the loop. At the tick the hub holds the whole set again — but
    /// the release is still waiting to be drained, so the tick leaves the hold to the drain,
    /// which cancels it by the release's own time. The new press starts a hold of its own, and
    /// the combination fires once, 800 ms after it.
    #[test]
    fn a_hold_is_left_for_the_drain_while_a_release_of_its_set_waits_in_the_hub() {
        let t = Instant::now();
        let max_age = Duration::from_secs(1);
        let mut c = Chord::new(vec![Back, Start], false, ms(800)).unwrap();
        c.press(&down(1, Start, &[Back, Start], t), true);
        let waiting = |_: u8| Some(PadNow { held: set(&[Back, Start]), releases_waiting: set(&[Start]) });
        assert!(c.due(t + ms(1090), max_age, waiting).is_empty());
        assert!(c.has_pending(), "still running, for the drain to decide");
        // A release of another button waiting changes nothing.
        let other = |_: u8| Some(PadNow { held: set(&[Back, Start]), releases_waiting: set(&[South]) });
        let mut o = Chord::new(vec![Back, Start], false, ms(800)).unwrap();
        o.press(&down(1, Start, &[Back, Start], t), true);
        assert_eq!(o.due(t + ms(810), max_age, other), vec![(1, Pending { at: t, family: X })]);

        // The drain: the release came 10 ms before the time was up, and the press after it.
        c.release(&up(1, Start, &[Back], t + ms(790)));
        assert!(!c.has_pending(), "cancelled");
        assert!(!c.press(&down(1, Start, &[Back, Start], t + ms(850)), true));
        assert!(c.due(t + ms(1105), max_age, settled(&[Back, Start])).is_empty(), "the old hold is gone");
        assert_eq!(
            c.due(t + ms(1655), max_age, settled(&[Back, Start])),
            vec![(1, Pending { at: t + ms(850), family: X })],
            "the new one fires, once"
        );
        assert!(!c.has_pending());
    }

    /// Let go a moment after the hold time was up, before the tick got to it: the set was held
    /// out, and the combination fires on the next tick although nothing is held any more.
    #[test]
    fn letting_go_after_the_hold_time_but_before_the_tick_still_fires() {
        let t = Instant::now();
        let max_age = Duration::from_secs(1);
        let mut c = Chord::new(vec![Back, Start], false, ms(800)).unwrap();
        c.press(&down(1, Start, &[Back, Start], t), true);
        c.release(&up(1, Start, &[Back], t + ms(800)));
        assert!(c.has_pending(), "held out, waiting for the tick");
        // The other member let go too: still once.
        c.release(&up(1, Back, &[], t + ms(805)));
        assert_eq!(c.due(t + ms(812), max_age, settled(&[])), vec![(1, Pending { at: t, family: X })]);
        assert!(!c.has_pending());
        assert!(c.due(t + ms(830), max_age, settled(&[])).is_empty(), "once");

        // Pressed again straight away: the held-out hold fires, and the new one runs on.
        let mut r = Chord::new(vec![Back, Start], false, ms(800)).unwrap();
        r.press(&down(1, Start, &[Back, Start], t), true);
        r.release(&up(1, Start, &[Back], t + ms(810)));
        r.press(&down(1, Start, &[Back, Start], t + ms(850)), true);
        assert_eq!(r.due(t + ms(860), max_age, settled(&[Back, Start])), vec![(1, Pending { at: t, family: X })]);
        assert!(r.has_pending(), "the second holding is running");
        assert_eq!(
            r.due(t + ms(1660), max_age, settled(&[Back, Start])),
            vec![(1, Pending { at: t + ms(850), family: X })]
        );

        // `exact`: judged by what is held after the release — another button held refuses it.
        let mut e = Chord::new(vec![Back, Start], true, ms(800)).unwrap();
        e.press(&down(1, Start, &[Back, Start], t), true);
        e.release(&up(1, Start, &[Back, South], t + ms(810)));
        assert!(!e.has_pending());
        let mut e = Chord::new(vec![Back, Start], true, ms(800)).unwrap();
        e.press(&down(1, Start, &[Back, Start], t), true);
        e.release(&up(1, Start, &[Back], t + ms(810)));
        let later = e.due(t + ms(815), max_age, settled(&[Back, South]));
        assert_eq!(later.len(), 1, "what is held later is not looked at");

        // `maxAge` still applies: the pump stood still for two seconds after the hold ended.
        let mut s = Chord::new(vec![Back, Start], false, ms(800)).unwrap();
        s.press(&down(1, Start, &[Back, Start], t), true);
        s.release(&up(1, Start, &[Back], t + ms(900)));
        assert!(s.due(t + ms(2900), max_age, settled(&[])).is_empty());
        assert!(!s.has_pending());

        // A pad that goes away, a disabled module: a held-out hold is dropped with the rest.
        let mut g = Chord::new(vec![Back, Start], false, ms(800)).unwrap();
        g.press(&down(1, Start, &[Back, Start], t), true);
        g.release(&up(1, Start, &[Back], t + ms(810)));
        g.forget(1);
        assert!(!g.has_pending());
        g.press(&down(2, Start, &[Back, Start], t), true);
        g.release(&up(2, Start, &[Back], t + ms(810)));
        g.cancel_holds();
        assert!(!g.has_pending());
    }

    #[test]
    fn a_hold_is_checked_against_what_is_held_when_it_ends() {
        let t = Instant::now();
        let max_age = Duration::from_secs(1);
        let later = t + ms(650);
        // A member is no longer held, and no release of it is waiting: a release no source
        // reported. Dropped, not kept.
        let mut c = Chord::new(vec![Back, Start], false, ms(600)).unwrap();
        c.press(&down(1, Start, &[Back, Start], t), true);
        assert!(c.due(later, max_age, settled(&[Start])).is_empty());
        assert!(!c.has_pending());
        // The pad is gone.
        c.press(&down(1, Back, &[Back, Start], t), true);
        assert!(c.due(later, max_age, |_| None).is_empty());
        // Exact, and another button went down during the hold.
        let mut e = Chord::new(vec![Back, Start], true, ms(600)).unwrap();
        e.press(&down(1, Start, &[Back, Start], t), true);
        assert!(e.due(later, max_age, settled(&[Back, Start, South])).is_empty());
        // Too late: the pump stood still for two seconds after the hold ended.
        let mut s = Chord::new(vec![Back, Start], false, ms(600)).unwrap();
        s.press(&down(1, Start, &[Back, Start], t), true);
        assert!(s.due(t + ms(2600), max_age, settled(&[Back, Start])).is_empty());
    }

    #[test]
    fn cancelled_holds_do_not_fire() {
        let t = Instant::now();
        let mut c = Chord::new(vec![Back, Start], false, ms(100)).unwrap();
        c.press(&down(1, Start, &[Back, Start], t), true);
        c.cancel_holds();
        assert!(c.due(t + Duration::from_secs(1), Duration::from_secs(1), settled(&[Back, Start])).is_empty());
        assert!(!c.press(&down(1, South, &[Back, Start, South], t), true));
        assert!(!c.has_pending(), "and pressing another button does not start one");
        c.release(&up(1, Start, &[Back, South], t));
        c.press(&down(1, Start, &[Back, Start], t), true);
        assert!(c.has_pending(), "the next completion does");
    }
}
