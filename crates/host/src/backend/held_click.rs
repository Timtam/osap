//! A single click held out of a double-click (Windows): `host.input.click` with `clicks = 1`
//! through the system's input, when Windows would pair its press with the host's last press of the
//! same button into a double-click. The maintainer's decision D4 of 2026-10-31, after the second
//! Windows session (2026-10-31): three clicks of VPS Avenger's scroll arrow in the expansion grid,
//! asked one after another at one point, moved the grid once and then stood still, and the
//! overlay opened the expansion beside the one it was asked for. The likeliest cause, which the log
//! cannot prove (a grid shorter on the tester's Avenger fits it too): two `SendInput` presses at one
//! point 100 to 300 ms apart are a double-click to Windows, as from a mouse, and a scroll arrow that
//! takes a double-click as one step steps once.
//!
//! **Windows' rule** ("About Mouse Input", Double-Click Messages): a press of the same button
//! within the user's double-click time (`GetDoubleClickTime`) of the one before and inside the
//! double-click rectangle around it (`SM_CXDOUBLECLK` by `SM_CYDOUBLECLK`) is a double-click's
//! second press — `WM_xBUTTONDBLCLK` for a window whose class has `CS_DBLCLKS`, a second
//! `WM_xBUTTONDOWN` that a toolkit counts itself for any other. `MOUSEINPUT` carries no click
//! count, so nothing in the press can say "a single click" ([`wait_before_single`]).
//!
//! **So such a click waits** — on the host's `input-gesture` thread (acting.rs, `Lane::Gesture`),
//! never on the event loop — until that time has passed since the host's last press of the button,
//! and is made then; the call answers when it is handed over, its callback when it is made, as a
//! double-click's (`clicks = 2`, double_click.rs). And a click asked for while such a click is
//! still on its way goes the same way, behind it, wherever it is, so that clicks go out in the
//! order they were asked for ([`ON_ITS_WAY`]). One `[input]` line per wait ([`wait_line`]).
//!
//! **No timer decides anything here**: the wait is the system's own rule, read afresh for each
//! click, against the moment the host's own last press of that button went out ([`note_press`]) —
//! every press through `SendInput`: a click's, a double-click's two, a `mouseDown`'s and a drag's.
//! A press of the user's own hand is not known here, and a posted click is no press of the
//! system's input, which never pairs it (click_win.rs). A Mac is unchanged: its clicks carry their
//! click state, 1 for a single click.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXDOUBLECLK, SM_CYDOUBLECLK};

use super::{ActJob, MouseButton};
use crate::errors::HostError;
use crate::logging;

/// How much longer than the double-click time a click is held: one step of the clock Windows
/// stamps each press with, rounded up. A `MOUSEINPUT` whose `time` is 0 is stamped with the
/// system's tick count as Windows takes it, and that count moves on in steps of 15.6 ms by default,
/// so the time Windows measures between two presses can be up to a step shorter than the time
/// between them — and a press the double-click time apart by the host's clock could be inside it
/// by Windows'.
pub(super) const STAMP_MS: u64 = 16;

/// The most waits one click makes: each against the host's last press of its button as it is then
/// — a press made meanwhile on another thread (a no-activate click's, a `mouseDown`) is waited out
/// as well — and after the last the click is made all the same.
const WAITS_MAX: usize = 4;

/// A press of the host's through the system's input: the button, the point, the moment it went out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Press {
    pub button: MouseButton,
    pub x: i32,
    pub y: i32,
    pub at: Instant,
}

/// The host's last press through the system's input, of any button: what a single click is
/// measured against. Every thread that presses writes it, right after the press went out.
static LAST: Mutex<Option<Press>> = Mutex::new(None);

/// Single clicks handed to the gesture thread and not made yet: while there is one, every single
/// click through the input goes there too, behind it, so that a click asked for later — at another
/// point, which needs no wait of its own — is never made before it. Counted from the hand-over until
/// the job is dropped — made, refused, or never run ([`OnItsWay`]).
static ON_ITS_WAY: AtomicUsize = AtomicUsize::new(0);

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A held click on its way, counted in [`ON_ITS_WAY`] until it is dropped.
struct OnItsWay;

impl OnItsWay {
    fn new() -> OnItsWay {
        ON_ITS_WAY.fetch_add(1, Ordering::SeqCst);
        OnItsWay
    }
}

impl Drop for OnItsWay {
    fn drop(&mut self) {
        ON_ITS_WAY.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Notes that the host's press of `button` at (x, y) went out through the system's input — called
/// right after `SendInput` took it, on whichever thread made it.
pub(super) fn note_press(button: MouseButton, x: i32, y: i32) {
    *locked(&LAST) = Some(Press { button, x, y, at: Instant::now() });
}

/// How long a single click of `button` at `at` waits at `now` so that Windows cannot take it for a
/// double-click's second press: until the double-click time `dct`, and [`STAMP_MS`] more, has
/// passed since `last`, the host's last press — when that press was of the same button and the
/// click is inside the double-click rectangle `rect` (`SM_CXDOUBLECLK`, `SM_CYDOUBLECLK`) around
/// it; nothing otherwise. Inside means no further from the press each way than half the
/// rectangle's side, rounded up, its edges included: Windows' own test — a rectangle of that size
/// centred on the press, its right and bottom edges outside — is never wider, so a click it would
/// pair is always held. Pure.
pub(super) fn wait_before_single(
    last: Option<&Press>,
    now: Instant,
    at: (i32, i32),
    button: MouseButton,
    dct: Duration,
    rect: (i32, i32),
) -> Duration {
    let Some(p) = last else { return Duration::ZERO };
    if p.button != button {
        return Duration::ZERO;
    }
    let half = |side: i32| (i64::from(side.max(0)) + 1) / 2;
    let (dx, dy) = ((i64::from(at.0) - i64::from(p.x)).abs(), (i64::from(at.1) - i64::from(p.y)).abs());
    if dx > half(rect.0) || dy > half(rect.1) {
        return Duration::ZERO;
    }
    (dct + Duration::from_millis(STAMP_MS)).saturating_sub(now.saturating_duration_since(p.at))
}

/// Windows' rule as the user has it set now: the double-click time — 500 ms for a setting of 0,
/// which Windows does not answer — and the double-click rectangle. Local calls into user32, no
/// message; any thread may ask.
fn system_rule() -> (Duration, (i32, i32)) {
    // SAFETY: reads of the user's settings.
    let (ms, cx, cy) = unsafe { (GetDoubleClickTime(), GetSystemMetrics(SM_CXDOUBLECLK), GetSystemMetrics(SM_CYDOUBLECLK)) };
    let dct = if ms == 0 { super::DEFAULT_DOUBLE_CLICK } else { Duration::from_millis(u64::from(ms)) };
    (dct, (cx, cy))
}

/// A button's name in the lines.
fn button_word(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// The `[input]` line of one wait: the click, how long it is held, and the press and the rule it
/// is held for — `ago` how long before the wait that press went out. Pure.
pub(super) fn wait_line(x: i32, y: i32, button: MouseButton, wait: Duration, last: &Press, ago: Duration, rule: (Duration, (i32, i32))) -> String {
    let (dct, rect) = rule;
    format!(
        "click at {x},{y} with the {} button held {} ms: the host pressed that button at {},{} {} ms before, within \
         the system's double-click time of {} ms and its double-click rectangle of {}x{} — made once that time and a \
         step of Windows' clock ({STAMP_MS} ms) have passed, so that Windows cannot take it for a double-click's second press",
        button_word(button),
        wait.as_millis(),
        last.x,
        last.y,
        ago.as_millis(),
        dct.as_millis(),
        rect.0,
        rect.1,
    )
}

/// The `[input]` line of a held click Windows did not take or make, for a click handed over without
/// a callback as for one with: the line the callback's answer is in.
pub(super) fn failed_line(x: i32, y: i32, button: MouseButton, why: &HostError) -> String {
    format!("click at {x},{y} with the {} button, held out of a double-click, not made: {why}", button_word(button))
}

/// Waits on this thread — never the event loop's — until a single click of `button` at (x, y) can
/// be made without Windows taking it for a double-click's second press: each wait against the
/// host's last press as it is then, one line each, at most [`WAITS_MAX`] of them. What the
/// no-activate click (click_win.rs) waits with, on its own thread, and the held click below.
pub(super) fn wait_here(x: i32, y: i32, button: MouseButton) {
    for _ in 0..WAITS_MAX {
        let (dct, rect) = system_rule();
        let last = *locked(&LAST);
        let now = Instant::now();
        let wait = wait_before_single(last.as_ref(), now, (x, y), button, dct, rect);
        let Some(p) = last.filter(|_| !wait.is_zero()) else { return };
        logging::line("input", &wait_line(x, y, button, wait, &p, now.saturating_duration_since(p.at), (dct, rect)));
        std::thread::sleep(wait);
    }
}

/// Whether a single click goes to the gesture thread rather than out at once: when it has a wait of
/// its own ([`wait_before_single`]), and when a held click is still on its way (`behind`) — a click
/// at another point, which needs no wait, all the same, so that it is never made before the one
/// asked for first. Pure.
pub(super) fn hand_over(behind: bool, wait: Duration) -> bool {
    behind || !wait.is_zero()
}

/// `host.input.click` with `clicks = 1` through the input, its part on the event loop: `None` when
/// the click is made now, as ever — no press of the host's it could be paired with, and no held
/// click on its way; otherwise the job that makes it on the host's `input-gesture` thread, once the
/// wait is over ([`wait_here`]), through the system's input as every click (windows.rs,
/// `click_input`), and writes a line when Windows did not take or make it. Local calls only: the
/// user's settings, and a look at the last press under its lock.
pub(super) fn held_job(x: i32, y: i32, button: MouseButton) -> Option<ActJob> {
    let behind = ON_ITS_WAY.load(Ordering::SeqCst) > 0;
    let (dct, rect) = system_rule();
    let wait = wait_before_single(locked(&LAST).as_ref(), Instant::now(), (x, y), button, dct, rect);
    if !hand_over(behind, wait) {
        return None;
    }
    let on_its_way = OnItsWay::new();
    Some(Box::new(move || {
        let _on_its_way = on_its_way;
        wait_here(x, y, button);
        let made = super::windows::click_input(x, y, button);
        if let Err(why) = &made {
            logging::line("input", &failed_line(x, y, button, why));
        }
        made
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DCT: Duration = Duration::from_millis(500);
    const RECT: (i32, i32) = (4, 4);

    fn press(button: MouseButton, x: i32, y: i32, at: Instant) -> Press {
        Press { button, x, y, at }
    }

    /// The decision's cases: the same point 100 ms after the press, at a double-click time of 500 ms,
    /// waits the 400 ms left and a step of Windows' clock; 5 px away, another button, 600 ms later,
    /// or no press at all, nothing.
    #[test]
    fn a_click_inside_the_double_click_time_and_rectangle_of_the_last_press_waits_the_time_out() {
        let t0 = Instant::now();
        let last = press(MouseButton::Left, 463, 177, t0);
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let wait = |p: Option<&Press>, now: Instant, x: i32, y: i32, b: MouseButton| wait_before_single(p, now, (x, y), b, DCT, RECT);
        assert_eq!(wait(Some(&last), at(100), 463, 177, MouseButton::Left), Duration::from_millis(400 + STAMP_MS));
        assert_eq!(STAMP_MS, 16, "a step of Windows' tick count, 15.6 ms, rounded up");
        assert_eq!(wait(Some(&last), at(100), 468, 177, MouseButton::Left), Duration::ZERO, "5 px away");
        assert_eq!(wait(Some(&last), at(100), 463, 172, MouseButton::Left), Duration::ZERO, "5 px above");
        assert_eq!(wait(Some(&last), at(100), 463, 177, MouseButton::Right), Duration::ZERO, "another button");
        assert_eq!(wait(Some(&last), at(600), 463, 177, MouseButton::Left), Duration::ZERO, "600 ms later");
        assert_eq!(wait(None, at(100), 463, 177, MouseButton::Left), Duration::ZERO, "no press of the host's");
    }

    /// The rectangle: half its side each way, rounded up, its edges included — so every point
    /// Windows' own centred rectangle holds is held, and the first one outside is not; an odd side
    /// rounds up, a side of 0 holds the press's own point alone.
    #[test]
    fn the_rectangle_is_half_its_side_each_way_rounded_up_with_its_edges() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_millis(50);
        let last = press(MouseButton::Left, 100, 200, t0);
        let held = |x: i32, y: i32, rect: (i32, i32)| !wait_before_single(Some(&last), now, (x, y), MouseButton::Left, DCT, rect).is_zero();
        for (x, y) in [(98, 200), (102, 200), (100, 198), (100, 202), (102, 202), (98, 198)] {
            assert!(held(x, y, RECT), "({x}, {y}) is inside a 4x4 rectangle");
        }
        for (x, y) in [(97, 200), (103, 200), (100, 197), (100, 203)] {
            assert!(!held(x, y, RECT), "({x}, {y}) is outside it");
        }
        assert!(held(103, 203, (5, 5)) && !held(104, 200, (5, 5)), "an odd side rounds up");
        assert!(held(100, 200, (0, 0)) && !held(101, 200, (0, 0)), "a side of 0");
        assert!(held(100, 200, (-3, -3)), "a side Windows would not answer counts as 0");
        // A far point does not overflow the arithmetic.
        let far = press(MouseButton::Left, i32::MIN, i32::MAX, t0);
        assert!(wait_before_single(Some(&far), now, (i32::MAX, i32::MIN), MouseButton::Left, DCT, RECT).is_zero());
    }

    /// The wait is what is left of the double-click time and the clock's step, whatever the time:
    /// right after the press nearly all of it, just before its end a millisecond, at its end none —
    /// and a double-click time the user set longer is waited out in full.
    #[test]
    fn the_wait_is_what_is_left_of_the_time() {
        let t0 = Instant::now();
        let last = press(MouseButton::Middle, 10, 10, t0);
        let wait = |ms: u64, dct: u64| {
            wait_before_single(Some(&last), t0 + Duration::from_millis(ms), (10, 10), MouseButton::Middle, Duration::from_millis(dct), RECT)
        };
        assert_eq!(wait(0, 500), Duration::from_millis(516));
        assert_eq!(wait(515, 500), Duration::from_millis(1));
        assert_eq!(wait(516, 500), Duration::ZERO);
        assert_eq!(wait(900, 1200), Duration::from_millis(316));
        // A moment before the press — another thread's press noted after this one read the clock.
        assert_eq!(wait_before_single(Some(&press(MouseButton::Middle, 10, 10, t0 + Duration::from_millis(5))), t0, (10, 10), MouseButton::Middle, DCT, RECT), Duration::from_millis(516));
    }

    /// The order (the review of 0.16, its finding 3): a click asked for while a held click is on its
    /// way is handed to the gesture thread behind it — at another point and of another button, with
    /// no wait of its own, all the same — so that it is never made before the one asked for first;
    /// one with neither goes out at once. `held_job` counts a held click as on its way from the
    /// hand-over until its job is dropped, run or not.
    #[test]
    fn a_click_asked_for_while_a_held_one_is_on_its_way_goes_behind_it() {
        assert!(hand_over(true, Duration::ZERO), "behind a held click, with no wait of its own");
        assert!(hand_over(false, Duration::from_millis(1)), "a wait of its own");
        assert!(hand_over(true, Duration::from_millis(400)), "both");
        assert!(!hand_over(false, Duration::ZERO), "neither: made at once");
        // A held click on its way, and a click far from any press of the host's: handed over.
        let before = ON_ITS_WAY.load(Ordering::SeqCst);
        let on_its_way = OnItsWay::new();
        let job = held_job(i32::MIN + 7, i32::MAX - 7, MouseButton::Middle);
        assert!(job.is_some(), "behind the held click on its way");
        assert_eq!(ON_ITS_WAY.load(Ordering::SeqCst), before + 2, "that one and this one");
        drop(job);
        assert_eq!(ON_ITS_WAY.load(Ordering::SeqCst), before + 1, "a job dropped unrun is no longer on its way");
        drop(on_its_way);
        assert_eq!(ON_ITS_WAY.load(Ordering::SeqCst), before);
    }

    /// The lines: the wait, with the press and the rule it waits out; a click not made, with why.
    #[test]
    fn the_lines_say_the_wait_the_press_and_the_rule() {
        let last = press(MouseButton::Left, 463, 177, Instant::now());
        assert_eq!(
            wait_line(463, 177, MouseButton::Left, Duration::from_millis(416), &last, Duration::from_millis(100), (DCT, RECT)),
            "click at 463,177 with the left button held 416 ms: the host pressed that button at 463,177 100 ms before, \
             within the system's double-click time of 500 ms and its double-click rectangle of 4x4 — made once that time \
             and a step of Windows' clock (16 ms) have passed, so that Windows cannot take it for a double-click's second press"
        );
        let placed = HostError::with(&crate::errors::input::NOT_PLACED, "Windows did not put the pointer there (error 5)".to_string());
        assert_eq!(
            failed_line(463, 177, MouseButton::Right, &placed),
            "click at 463,177 with the right button, held out of a double-click, not made: Windows did not put the pointer there (error 5)"
        );
    }

    /// The wiring, as the source has it: every press through `SendInput` is noted — a click's (on
    /// the event loop, the window-style thread and the gesture thread alike), a double-click's, a
    /// `mouseDown`'s and a drag's — right after Windows took it; the event loop asks for the job
    /// before it clicks through the input, and hands it to the gesture thread; a no-activate click
    /// waits on its own thread before its style and its click; and the held click is the same
    /// `click_input` as every click.
    #[test]
    fn every_press_is_noted_and_every_single_click_through_the_input_asks_first() {
        use crate::ocr_wiring_tests::body;
        const WIN: &str = include_str!("windows.rs");
        const CLICK_WIN: &str = include_str!("click_win.rs");
        const LIB: &str = include_str!("../lib.rs");
        let click = body(WIN, "pub(super) fn click_input(x: i32, y: i32, button: MouseButton)");
        let sent = click.find("let sent = send_inputs(").expect("the click's events");
        let noted = click.find("super::held_click::note_press(button, x, y);").expect("a click's press noted");
        assert!(sent < noted && noted < click.find("released(button, &sent)").unwrap(), "{click}");
        assert!(click.contains("if sent.taken == 0 {\n        return sent.answer();\n    }"), "noted only when the press went out");
        let hand = body(WIN, "fn press(&mut self, _n: u8)");
        assert!(hand.find(".answer()?;").unwrap() < hand.find("super::held_click::note_press(self.button, self.x, self.y);").unwrap());
        let down = body(WIN, "fn mouse_down(&self, x: i32, y: i32, button: MouseButton)");
        assert!(down.find(".answer()?;").unwrap() < down.find("super::held_click::note_press(button, x, y);").unwrap());
        let drag = body(WIN, "fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton)");
        assert!(drag.find(".answer()?;").unwrap() < drag.find("super::held_click::note_press(button, x1, y1);").unwrap());
        assert!(body(WIN, "fn held_click(&self, x: i32, y: i32, button: MouseButton)").contains("super::held_click::held_job(x, y, button)"));
        let style = body(CLICK_WIN, "fn hold_and_click(");
        let waited = style.find("super::held_click::wait_here(x, y, button);").expect("the no-activate click waits");
        assert!(waited < style.find("if !who.still_there()").unwrap() && waited < style.find("super::windows::click_input(x, y, button)?;").unwrap());
        let api = body(LIB, "fn install_host_api(");
        let asked = api.find("if let Some(job) = sh.backend.held_click(x, y, button) {").expect("the event loop asks");
        let handed = api[asked..].find("sh.acting.run_releasing(acting::Lane::Gesture, job, answer, release);").expect("handed to the gesture thread");
        let made = api.find("let made = sh.backend.mouse_click(x, y, button);").expect("made at once");
        assert!(asked < made && asked + handed < made, "asked before the click made at once");
        let job = body(include_str!("held_click.rs"), "pub(super) fn held_job(");
        assert!(job.find("wait_here(x, y, button);").unwrap() < job.find("super::windows::click_input(x, y, button)").unwrap());
    }
}
