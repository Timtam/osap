//! A double-click made as one gesture: `host.input.click`'s `clicks = 2` (lib.rs), made on the
//! host's `input-gesture` thread (acting.rs, `Lane::Gesture`) — never on the event loop, where a
//! turn that runs long would stretch a press into a long press or the gap past the double-click
//! time. Two plain clicks sent apart are not one on a Mac: each carries a click state of 1, and a
//! JUCE plug-in there takes the pair for two single clicks (the third Mac test, 2026-10-25: VPS
//! Avenger's resets did not take).
//!
//! **The order** ([`run`]), the same on every platform, each step a platform's [`Hand`]:
//!
//! 1. the window at the point read — none read, and nothing is clicked;
//! 2. the first press — the pointer moved to the point with it — and [`PRESS_MS`] later its
//!    release;
//! 3. [`GAP_MS`] later the window at the point read again: another one, or none read, and the
//!    second click is not sent (`input::DOUBLE_CUT`), since the first click changed what is under
//!    the point — a menu it opened, a window it closed;
//! 4. the second press, and [`PRESS_MS`] later its release.
//!
//! A press that went out always has its release made before anything else is done: right after it
//! in the order, and by the hand's own `Drop` should the thread unwind in between. What the system
//! did not take or make stops the gesture where it is, with its answer.
//!
//! **The timing defines the gesture**; it is no guess at anything's state: a press held about as
//! long as a finger holds one, and the second press well inside every double-click time a system
//! uses. They are the VPS Avenger control tool's, whose double-clicks Avenger takes on a Mac
//! (`clicker/driver.py`, `_CLICK_DOWN_UP_DELAY` and `_DOUBLE_CLICK_GAP`).
//!
//! One `[input]` line per double-click ([`line`]), with the times as they were measured.

use std::time::{Duration, Instant};

use super::{ActJob, MouseButton};
use crate::errors::{input, HostError};

/// From a press to its release, in milliseconds.
pub(crate) const PRESS_MS: u64 = 20;

/// From the first release to the second press, in milliseconds.
pub(crate) const GAP_MS: u64 = 60;

/// What a platform does for [`run`]. Each call acts at once and never waits for a program; the
/// waits are [`Hand::wait`]'s.
pub(crate) trait Hand {
    /// The window at the point now, as the platform names one: Windows, the handle
    /// `WindowFromPoint` answers; a Mac, the window server's number of the front-most window on
    /// screen there of the program the click lands in. 0 for none; `None` when it cannot be read.
    fn at_point(&mut self) -> Option<u64>;
    /// Press `n` (1 or 2) of the button at the point, the pointer moved there first. `Ok` when it
    /// went out; `Err` with what the system did not take or make, and then nothing of it went out.
    fn press(&mut self, n: u8) -> Result<(), HostError>;
    /// The release of press `n`, made whatever came before it. `Err` with what the system did not
    /// take — on Windows the release is sent again on its own first.
    fn release(&mut self, n: u8) -> Result<(), HostError>;
    /// Waits `ms` milliseconds: the thread sleeps.
    fn wait(&mut self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
    /// The moment now, for the times in the log.
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// One event of the gesture that went out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Press(u8),
    Release(u8),
}

/// What [`run`] did: when each press and release went out, in order, and the window at the point
/// as it read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Timeline {
    /// Each event that went out, with the moment right after it did.
    pub(crate) out: Vec<(Step, Instant)>,
    /// The window at the point before the first press; `None` when it could not be read.
    pub(crate) first: Option<u64>,
    /// Before the second press: `None` when the gesture stopped before it was read again,
    /// `Some(None)` when it could not be read.
    pub(crate) second: Option<Option<u64>>,
}

impl Timeline {
    /// Milliseconds from the first press to the event that went out `i`-th.
    fn at(&self, i: usize) -> Option<u128> {
        let start = self.out.first()?.1;
        self.out.get(i).map(|(_, t)| t.duration_since(start).as_millis())
    }
}

/// The gesture, in the order the file's head gives, through `hand`: its answer — `Ok` when both
/// clicks went out — and what went out when.
pub(crate) fn run(hand: &mut impl Hand) -> (Result<(), HostError>, Timeline) {
    let mut t = Timeline { first: hand.at_point(), ..Timeline::default() };
    let Some(first) = t.first else {
        return (Err(HostError::of(&input::DOUBLE_NO_WINDOW)), t);
    };
    for n in [1u8, 2] {
        if n == 2 {
            hand.wait(GAP_MS);
            let now = hand.at_point();
            t.second = Some(now);
            if now != Some(first) {
                return (Err(HostError::of(&input::DOUBLE_CUT)), t);
            }
        }
        if let Err(e) = hand.press(n) {
            return (Err(e), t);
        }
        t.out.push((Step::Press(n), hand.now()));
        hand.wait(PRESS_MS);
        if let Err(e) = hand.release(n) {
            return (Err(e), t);
        }
        t.out.push((Step::Release(n), hand.now()));
    }
    (Ok(()), t)
}

/// The gesture as a job for the host's `input-gesture` thread: the hand made there by `make` — a
/// platform's events are made on the thread that posts them — [`run`] through it, the hand dropped
/// (which makes a release still owed), and its `[input]` line written. `how` says what makes the
/// pair a double-click on this platform, for the line; `system` is the user's double-click time,
/// read at the call.
pub(crate) fn job<H: Hand>(
    x: i32,
    y: i32,
    button: MouseButton,
    how: &'static str,
    system: Duration,
    make: impl FnOnce() -> H + Send + 'static,
) -> ActJob {
    Box::new(move || {
        let mut hand = make();
        let (result, t) = run(&mut hand);
        drop(hand);
        crate::logging::line("input", &line(x, y, button, &t, &result, how, system));
        result
    })
}

/// A button's name in the line.
fn button_word(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// The `[input]` line of one double-click at (x, y): what went out when, measured from the first
/// press — `press 1 at +0 ms, release at +21, press 2 at +82, release at +103` — and then how it
/// ended. Done: `how` (what makes the pair a double-click here), the window at the point, and the
/// system's double-click time, with a word where the presses were further apart than it. Cut: the
/// window before and after. Not taken or made: the step and the answer. Pure.
pub(crate) fn line(
    x: i32,
    y: i32,
    button: MouseButton,
    t: &Timeline,
    result: &Result<(), HostError>,
    how: &str,
    system: Duration,
) -> String {
    let head = format!("double-click at {x},{y} with the {} button: ", button_word(button));
    let times: Vec<String> = t
        .out
        .iter()
        .enumerate()
        .map(|(i, (step, _))| {
            let ms = t.at(i).unwrap_or(0);
            match (i, step) {
                (0, _) => format!("press 1 at +{ms} ms"),
                (_, Step::Press(n)) => format!("press {n} at +{ms}"),
                (_, Step::Release(_)) => format!("release at +{ms}"),
            }
        })
        .collect();
    let times = times.join(", ");
    let window = |w: u64| if w == 0 { "no window".to_string() } else { format!("window {w}") };
    match result {
        Ok(()) => {
            let system_ms = system.as_millis();
            let apart = t.at(2).unwrap_or(0);
            let slow = if apart > system_ms {
                format!(", shorter than the {apart} ms between the presses: the program may take them for two clicks")
            } else {
                String::new()
            };
            format!(
                "{head}{times} ({how}); {} at the point before both presses; the system's double-click time {system_ms} ms{slow}",
                window(t.first.unwrap_or(0))
            )
        }
        Err(e) if t.first.is_none() => format!("{head}nothing sent — {e}"),
        Err(e) if e.reason() == &input::DOUBLE_CUT => {
            let number = |w: u64| if w == 0 { "none".to_string() } else { w.to_string() };
            let now = match t.second {
                Some(Some(w)) => format!("is {} now", number(w)),
                _ => "cannot be read now".to_string(),
            };
            format!(
                "{head}{times}; the window at the point was {} and {now} — the second click not sent",
                number(t.first.unwrap_or(0))
            )
        }
        Err(e) => {
            let what = match t.out.len() {
                0 => return format!("{head}nothing sent — {e}"),
                1 => "its release not made",
                2 => "the second press not made",
                _ => "the second release not made",
            };
            format!("{head}{times}; {what} — {e}")
        }
    }
}

/// One window of the window server's list, as [`front_window_at`] reads it: its number, its
/// owner's process, its alpha, and its rectangle in the global top-left space clicks are in.
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Listed {
    pub(crate) number: u32,
    pub(crate) pid: i32,
    pub(crate) alpha: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// macOS: the window at the point for a double-click's check ([`Hand::at_point`]) — the front-most
/// of `list`, which runs front to back, whose rectangle holds the point and which is drawn (an
/// alpha above 0), of process `owner` when one is given; its number, 0 for none. Pure.
///
/// Of the program the click lands in, `owner`, which the system's hit test names at the call: a
/// window of another program drawn over the point that lets clicks through — a screen reader's
/// cursor ring, which follows the pointer to the press — is not the window the click is in, and its
/// coming or going between the two clicks changes nothing the click does. The window list says
/// what is drawn, not what takes a click, and the hit test is the main thread's alone.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn front_window_at(list: impl IntoIterator<Item = Listed>, x: i32, y: i32, owner: Option<i32>) -> u32 {
    let (px, py) = (f64::from(x), f64::from(y));
    list.into_iter()
        .find(|w| {
            owner.is_none_or(|o| o == w.pid)
                && w.alpha > 0.0
                && px >= w.x
                && px < w.x + w.w
                && py >= w.y
                && py < w.y + w.h
        })
        .map_or(0, |w| w.number)
}

/// macOS: [`front_window_at`] for the program that owns window `hit` — the window the system's hit
/// test named at the point at the call — as `list` names its owner; with no `hit`, or one `list`
/// does not carry (that window has gone), of any program. The owner is read off the same copy of
/// the list as the windows, on the gesture's thread, so the event loop asks the hit test alone.
/// Pure.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn front_window_for_hit(list: &[Listed], x: i32, y: i32, hit: Option<u32>) -> u32 {
    let owner = hit.and_then(|n| list.iter().find(|w| w.number == n)).map(|w| w.pid);
    front_window_at(list.iter().copied(), x, y, owner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::Code;

    /// A hand that records what it was asked, on a clock of its own that only its waits move, and
    /// answers as a test scripts it.
    struct Fake {
        start: Instant,
        clock: Duration,
        /// What `at_point` answers, in order; the last one again once they run out.
        windows: Vec<Option<u64>>,
        asked: usize,
        /// Refusals by step: `("press", n)` or `("release", n)`.
        refuse: Vec<(&'static str, u8, HostError)>,
        log: Vec<String>,
        /// A release owed: a press that went out and was not released yet.
        owed: Option<u8>,
    }

    impl Fake {
        fn new(windows: &[Option<u64>]) -> Fake {
            Fake { start: Instant::now(), clock: Duration::ZERO, windows: windows.to_vec(), asked: 0, refuse: Vec::new(), log: Vec::new(), owed: None }
        }
        fn refusing(mut self, step: &'static str, n: u8, e: HostError) -> Fake {
            self.refuse.push((step, n, e));
            self
        }
        fn refused(&self, step: &str, n: u8) -> Option<HostError> {
            self.refuse.iter().find(|r| r.0 == step && r.1 == n).map(|r| r.2.clone())
        }
    }

    impl Hand for Fake {
        fn at_point(&mut self) -> Option<u64> {
            let w = self.windows.get(self.asked).or(self.windows.last()).copied().flatten();
            self.asked += 1;
            self.log.push(format!("window {w:?}"));
            w
        }
        fn press(&mut self, n: u8) -> Result<(), HostError> {
            if let Some(e) = self.refused("press", n) {
                self.log.push(format!("press {n} refused"));
                return Err(e);
            }
            self.log.push(format!("press {n}"));
            self.owed = Some(n);
            Ok(())
        }
        fn release(&mut self, n: u8) -> Result<(), HostError> {
            assert_eq!(self.owed.take(), Some(n), "a release of the press that went out");
            if let Some(e) = self.refused("release", n) {
                self.log.push(format!("release {n} refused"));
                return Err(e);
            }
            self.log.push(format!("release {n}"));
            Ok(())
        }
        fn wait(&mut self, ms: u64) {
            self.log.push(format!("wait {ms}"));
            self.clock += Duration::from_millis(ms) + Duration::from_millis(1);
        }
        fn now(&self) -> Instant {
            self.start + self.clock
        }
    }

    /// The whole gesture: the window read, the first pair 20 ms apart, the window read again 60 ms
    /// later and found the same, the second pair — click numbers 1, 1, 2, 2 — and its answer `Ok`;
    /// nothing is left owed, and the times are each step's.
    #[test]
    fn a_double_click_is_two_pairs_twenty_ms_long_and_sixty_apart_at_one_window() {
        let mut hand = Fake::new(&[Some(30088)]);
        let (r, t) = run(&mut hand);
        assert_eq!(r, Ok(()));
        assert_eq!(
            hand.log,
            ["window Some(30088)", "press 1", "wait 20", "release 1", "wait 60", "window Some(30088)", "press 2", "wait 20", "release 2"]
        );
        assert_eq!(hand.owed, None);
        let steps: Vec<Step> = t.out.iter().map(|s| s.0).collect();
        assert_eq!(steps, [Step::Press(1), Step::Release(1), Step::Press(2), Step::Release(2)]);
        assert_eq!((0..4).map(|i| t.at(i).unwrap()).collect::<Vec<_>>(), [0, 21, 82, 103]);
        assert_eq!((t.first, t.second), (Some(30088), Some(Some(30088))));
        assert_eq!((PRESS_MS, GAP_MS), (20, 60), "the control tool's driver's timings");
    }

    /// Another window at the point after the first click, or none read: the second click is not
    /// sent — `covered`, the first click made and released — and nothing is pressed after the read.
    #[test]
    fn another_window_at_the_point_after_the_first_click_stops_the_second() {
        for second in [Some(31002), None, Some(0)] {
            let mut hand = Fake::new(&[Some(30088), second]);
            let (r, t) = run(&mut hand);
            let e = r.expect_err("cut");
            assert!(e.reason() == &input::DOUBLE_CUT && e.code() == Code::Covered, "{e:?}");
            assert_eq!(hand.log, ["window Some(30088)", "press 1", "wait 20", "release 1", "wait 60", &format!("window {second:?}")]);
            assert_eq!((t.out.len(), t.second), (2, Some(second)));
            assert_eq!(hand.owed, None, "nothing held");
        }
    }

    /// The window at the point unreadable before the first press: nothing is pressed at all.
    #[test]
    fn a_window_that_cannot_be_read_first_clicks_nothing() {
        let mut hand = Fake::new(&[None]);
        let (r, t) = run(&mut hand);
        assert_eq!(r, Err(HostError::of(&input::DOUBLE_NO_WINDOW)));
        assert_eq!(hand.log, ["window None"]);
        assert!(t.out.is_empty());
    }

    /// The first press refused: nothing more. Its release refused — Windows' button still down —
    /// no second pair, and the answer is the release's, which the host holds the button for. A
    /// second press or release refused ends it there, with that answer.
    #[test]
    fn what_the_system_does_not_take_stops_the_gesture_where_it_is() {
        let placed = HostError::with(&input::NOT_PLACED, "Windows did not put the pointer there (error 5)".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("press", 1, placed.clone());
        let (r, t) = run(&mut hand);
        assert_eq!((r, t.out.len()), (Err(placed), 0));
        assert_eq!(hand.log, ["window Some(7)", "press 1 refused"]);

        let down = HostError::with(&input::LEFT_DOWN, "Windows took the press but not its release (error 5): the button is still down".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("release", 1, down.clone());
        let (r, t) = run(&mut hand);
        assert_eq!((r, t.out.len()), (Err(down), 1));
        assert_eq!(hand.log, ["window Some(7)", "press 1", "wait 20", "release 1 refused"]);

        let null = HostError::with(&input::DOUBLE_SECOND_NULL, "CGEventCreateMouseEvent(type 1) returned NULL for the second click; the first click was made".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("press", 2, null.clone());
        let (r, t) = run(&mut hand);
        assert_eq!((r, t.out.len()), (Err(null), 2));
        assert_eq!(hand.log.last().map(String::as_str), Some("press 2 refused"));

        let taken = HostError::with(&input::NOT_TAKEN, "Windows did not take the input (error 5)".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("release", 2, taken.clone());
        let (r, t) = run(&mut hand);
        assert_eq!((r, t.out.len()), (Err(taken), 3));
    }

    /// The `[input]` line of each ending: done, with how the pair is counted, the window and the
    /// system's time — and a word where the presses were further apart than it; cut, with the two
    /// windows; nothing sent; and a step not taken, with its answer.
    #[test]
    fn the_line_says_what_went_out_when_and_how_it_ended() {
        let at = |windows: &[Option<u64>]| {
            let mut hand = Fake::new(windows);
            run(&mut hand)
        };
        let (r, t) = at(&[Some(30088)]);
        let ms = Duration::from_millis(500);
        assert_eq!(
            line(1150, 621, MouseButton::Left, &t, &r, "click states 1 and 2", ms),
            "double-click at 1150,621 with the left button: press 1 at +0 ms, release at +21, press 2 at +82, \
             release at +103 (click states 1 and 2); window 30088 at the point before both presses; the system's \
             double-click time 500 ms"
        );
        assert_eq!(
            line(1150, 621, MouseButton::Right, &t, &r, "counted by Windows", Duration::from_millis(50)),
            "double-click at 1150,621 with the right button: press 1 at +0 ms, release at +21, press 2 at +82, \
             release at +103 (counted by Windows); window 30088 at the point before both presses; the system's \
             double-click time 50 ms, shorter than the 82 ms between the presses: the program may take them for two clicks"
        );
        let (r, t) = at(&[Some(30088), Some(31002)]);
        assert_eq!(
            line(1150, 621, MouseButton::Left, &t, &r, "click states 1 and 2", ms),
            "double-click at 1150,621 with the left button: press 1 at +0 ms, release at +21; the window at the \
             point was 30088 and is 31002 now — the second click not sent"
        );
        let (r, t) = at(&[Some(30088), None]);
        assert!(line(1, 2, MouseButton::Left, &t, &r, "", ms).ends_with("was 30088 and cannot be read now — the second click not sent"));
        let (r, t) = at(&[Some(30088), Some(0)]);
        assert!(line(1, 2, MouseButton::Left, &t, &r, "", ms).ends_with("was 30088 and is none now — the second click not sent"));
        let (r, t) = at(&[Some(0)]);
        assert!(line(1, 2, MouseButton::Left, &t, &r, "", ms).contains("; no window at the point before both presses;"));
        let (r, t) = at(&[None]);
        assert_eq!(
            line(1, 2, MouseButton::Middle, &t, &r, "", ms),
            "double-click at 1,2 with the middle button: nothing sent — the window at the point could not be read: nothing was clicked"
        );
        let down = HostError::with(&input::LEFT_DOWN, "Windows took the press but not its release (error 5): the button is still down".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("release", 1, down);
        let (r, t) = run(&mut hand);
        assert_eq!(
            line(3, 4, MouseButton::Left, &t, &r, "", ms),
            "double-click at 3,4 with the left button: press 1 at +0 ms; its release not made — Windows took the press \
             but not its release (error 5): the button is still down"
        );
        let placed = HostError::with(&input::NOT_PLACED, "Windows did not put the pointer there (error 5)".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("press", 1, placed);
        let (r, t) = run(&mut hand);
        assert_eq!(line(3, 4, MouseButton::Left, &t, &r, "", ms), "double-click at 3,4 with the left button: nothing sent — Windows did not put the pointer there (error 5)");
        let taken = HostError::with(&input::NOT_TAKEN, "Windows did not take the input (error 5)".to_string());
        let mut hand = Fake::new(&[Some(7)]).refusing("press", 2, taken);
        let (r, t) = run(&mut hand);
        assert!(line(3, 4, MouseButton::Left, &t, &r, "", ms).ends_with("release at +21; the second press not made — Windows did not take the input (error 5)"));
    }

    /// The window at the point on a Mac: the front-most drawn window of the click's program that
    /// holds the point, a window drawn over it by another program — one that lets the click
    /// through — passed over; with no program named, the front-most drawn one; 0 for none.
    #[test]
    fn the_window_at_the_point_is_the_front_most_of_the_clicks_program() {
        let w = |number, pid, alpha, x, y, w, h| Listed { number, pid, alpha, x, y, w, h };
        let list = [
            w(900, 55, 1.0, 1000.0, 400.0, 400.0, 300.0), // another program's ring over the knob
            w(800, 42, 0.0, 0.0, 0.0, 1920.0, 1080.0),    // an invisible window of the plug-in's program
            w(30088, 42, 1.0, 596.0, 365.0, 800.0, 600.0), // the plug-in window
            w(2, 42, 1.0, 0.0, 30.0, 1920.0, 1050.0),     // the DAW's main window
        ];
        assert_eq!(front_window_at(list, 1150, 621, Some(42)), 30088);
        assert_eq!(front_window_at(list, 1150, 621, None), 900);
        assert_eq!(front_window_at(list, 100, 621, Some(42)), 2, "outside the plug-in window");
        assert_eq!(front_window_at(list, 1396, 965, Some(42)), 2, "the right and bottom edges are outside");
        assert_eq!(front_window_at(list, 596, 365, Some(42)), 30088, "the top-left corner is inside");
        assert_eq!(front_window_at(list, 100, 10, Some(42)), 0, "none there");
        assert_eq!(front_window_at(list, 1150, 621, Some(7)), 0, "none of that program");
        // By the window the hit test named: its owner read off the same list.
        assert_eq!(front_window_for_hit(&list, 1150, 621, Some(30088)), 30088, "the plug-in window's program");
        assert_eq!(front_window_for_hit(&list, 1150, 621, Some(2)), 30088, "the DAW's main window names the same program");
        assert_eq!(front_window_for_hit(&list, 1150, 621, Some(31002)), 900, "a window gone: any program's");
        assert_eq!(front_window_for_hit(&list, 1150, 621, None), 900, "none named: any program's");
    }
}
