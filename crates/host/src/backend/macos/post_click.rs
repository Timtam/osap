//! A posted click on a Mac — `host.input.click`'s `via = "post"` — the part of it with nothing
//! macOS in it: which refusal a point gets, where the point is in the window as the event carries
//! it, what the events are marked with, and the log line. Its own file for the reason `budget.rs`
//! has one: `backend/mod.rs` borrows it by `#[path]` under `cfg(test)`, so it runs on the machine
//! it is written on. `ax::post_target` asks the window server and accessibility what these
//! functions are handed; `input::post_click` builds and posts the events.
//!
//! **What a posted click is here** (TODO.md, "Posted clicks on a Mac"). There is no window message
//! to post on a Mac: the move, the press and the release are `CGEvent`s handed to the window's
//! process with `CGEventPostToPid`, which puts them in that process's event stream past the
//! window server's routing — no hit test of the window server's, no activation, no pointer
//! moved. What routes them inside the process is the event itself: AppKit gives a mouse event to
//! the window whose number it carries, at the place in that window it carries. So each event
//! carries the window's number three times — in the two public fields
//! (`kCGMouseEventWindowUnderMousePointer`, `…ThatCanHandleThisEvent`) and in the private one
//! AppKit's own events are made with (51, `kCGSEventWindowIDField` in WebKit's
//! `Tools/TestRunnerShared/spi/CoreGraphicsTestSPI.h`) — and the place in the window, set with the
//! private `CGEventSetWindowLocation` (declared beside it there). A bare `CGEventPostToPid` names no
//! window, and the event is dropped (a window number of 0); with the numbers but not the place,
//! Finder and VS Code took the events and ignored them; with both, AppKit, SwiftUI and WKWebView
//! test windows took 54 clicks of 54 while their application stayed in the background (the public
//! measurements behind this: callstack/agent-device PR 3245, lahfir/agent-desktop PR 227,
//! TomiXRM/kagi issue 542). The place is measured from the window's top-left corner, as the window
//! server measures a window (`kCGWindowBounds`), as the two working clickers that set it do it
//! (trycua/cua's cua-driver, and the one Lakr233/bgclick-rev-skill takes apart); `page-check`
//! checks it on every Mac runner, by the place a page says it was clicked at.

use crate::errors::input::{FRAME, OUTSIDE};
use crate::errors::Reason;

// The Mac's own refusals of a posted click are Reasons of errors.rs (`errors::input`):
//
// - `NO_POST_ACCESS`: this application may not post events — Accessibility is not granted (the
//   system's `PostEvent` privilege, `CGPreflightPostEventAccess`). Without it the window server
//   drops every posted event without a word, so the click is refused rather than answered `true`.
// - `NO_WINDOW_LOCATION`: the macOS running has no `CGEventSetWindowLocation`, so a posted click
//   would carry no place in the window, and land where AppKit makes one of the screen's.
// - `NOT_PAIRED`: the window could not be paired with the window server's window (`ax::window_id`),
//   so there is no number to post to — or, for something inside a window, the window it lies in is
//   not known.
// - `NOT_ON_SCREEN`: the window's number is not in the window server's list of windows on screen:
//   minimised, hidden, or on another Space.
// - `NOT_ANSWERING`: the window's program did not answer an accessibility question a moment ago
//   (`ax::is_busy`), or the one this click asked: its window's place could not be read.

/// Whether this process may post events, by the two checks there are: the post-event check
/// (`CGPreflightPostEventAccess`) and Accessibility (`AXIsProcessTrusted`), which is what grants it.
/// They can disagree: in the second Mac test (macOS 26.6, 2026-10-24) the post-event check went on
/// answering "denied" in a process that had got Accessibility during its run, while its clicks were
/// taken. So a disagreement is no refusal — a click's own effect decides — and only both saying no
/// is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostAccess {
    Granted,
    /// Accessibility granted, the post-event check still "denied".
    Disagree,
    Denied,
}

impl PostAccess {
    pub fn of(preflight: bool, trusted: bool) -> PostAccess {
        match (preflight, trusted) {
            (true, _) => PostAccess::Granted,
            (false, true) => PostAccess::Disagree,
            (false, false) => PostAccess::Denied,
        }
    }

    /// Whether a posted click is refused (`NO_POST_ACCESS`): only where neither check allows it.
    pub fn refuses(self) -> bool {
        self == PostAccess::Denied
    }

    /// The log line, said when the state changes. Never "restart": Accessibility reaches the running
    /// process (`perm.rs`, ACCESSIBILITY_NOTE), and while it is granted nothing a restart would mend
    /// is known.
    pub fn line(self) -> &'static str {
        match self {
            PostAccess::Granted => "input: post-event access granted",
            PostAccess::Disagree => {
                "input: the post-event check reads DENIED while Accessibility is granted — the two \
                 checks disagree, so the events are posted all the same, and whether a click did \
                 anything is the click's own effect to show"
            }
            PostAccess::Denied => {
                "input: post-event access DENIED — Accessibility is not granted, so every synthesised \
                 click and keystroke from here on is discarded by the window server without a word. \
                 Grant this application Accessibility under System Settings > Privacy & Security; it \
                 takes effect in the running application."
            }
        }
    }
}

/// What the event-source user data field (`kCGEventSourceUserData`) carries on every event of a
/// posted click: the host's own, as `keys::PASS_ON_MARK` marks a key passed on. The key tap sees no
/// mouse event; the mark is there for whatever reads the events after us — a tap of the program's,
/// a session's diagnosis. "POST".
pub const POSTED_CLICK_MARK: i64 = 0x504F_5354;

/// A rectangle in points, the origin at the top-left of the primary display, as the window
/// server and accessibility give one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Frame {
    /// Whether the point lies in it: its left and top edges in, its right and bottom ones out.
    pub fn holds(&self, x: i32, y: i32) -> bool {
        let (x, y) = (f64::from(x), f64::from(y));
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

/// Whether a posted click at (x, y) may be made for a window: in `frame` — the window's, or the
/// control's own rectangle for something inside a window — and, for a window, in `content` too,
/// its client area; `None` where `frame` is a control's, whose rectangle is content already.
/// `Err` with the refusal: [`OUTSIDE`], or [`FRAME`] on the title bar or a border.
pub fn place(x: i32, y: i32, frame: Frame, content: Option<Frame>) -> Result<(), &'static Reason> {
    if !frame.holds(x, y) {
        return Err(&OUTSIDE);
    }
    match content {
        Some(c) if !c.holds(x, y) => Err(&FRAME),
        _ => Ok(()),
    }
}

/// The point (x, y) of the screen as the event's place in the window carries it: from the top-left
/// corner of `bounds`, the window's rectangle in the window server's list.
pub fn window_location(x: i32, y: i32, bounds: Frame) -> (f64, f64) {
    (f64::from(x) - bounds.x, f64::from(y) - bounds.y)
}

/// The log line of a posted click: for `window` (the id it was asked for), into the window
/// server's window `number` of process `pid` (`exe`), at `at` in that window, from (x, y) on the
/// screen; whether the pointer was moved there, and what moving it answered when it failed.
pub fn line(window: isize, number: u32, pid: i32, exe: &str, at: (f64, f64), screen: (i32, i32), moved: Result<bool, i32>) -> String {
    let moved = match moved {
        Ok(true) => "yes".to_string(),
        Ok(false) => "no".to_string(),
        Err(code) => format!("no (CGWarpMouseCursorPosition answered {code})"),
    };
    format!(
        "posted click for window {window}: into window number {number} of process {pid} ({exe}) at {:.0},{:.0} from \
         the window's top-left corner ({},{} on the screen); pointer moved: {moved}",
        at.0, at.1, screen.0, screen.1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The second Mac test's finding 20: the post-event check "denied" with Accessibility granted is
    /// no refusal and says no restart; only both saying no refuses, and nothing says restart.
    #[test]
    fn the_post_event_check_alone_refuses_nothing_and_no_line_says_restart() {
        assert_eq!(PostAccess::of(true, false), PostAccess::Granted);
        assert_eq!(PostAccess::of(false, true), PostAccess::Disagree);
        assert_eq!(PostAccess::of(false, false), PostAccess::Denied);
        assert!(!PostAccess::Disagree.refuses() && !PostAccess::Granted.refuses() && PostAccess::Denied.refuses());
        for a in [PostAccess::Granted, PostAccess::Disagree, PostAccess::Denied] {
            assert!(!a.line().contains("restart"), "{a:?}: {}", a.line());
            assert!(a.line().starts_with("input: "));
        }
        assert!(PostAccess::Disagree.line().contains("disagree"));
    }

    const WINDOW: Frame = Frame { x: 100.0, y: 50.0, w: 400.0, h: 300.0 };
    /// The window above with a title bar of 28 points.
    const CONTENT: Frame = Frame { x: 100.0, y: 78.0, w: 400.0, h: 272.0 };

    #[test]
    fn a_point_in_the_content_is_clicked_and_one_on_the_title_bar_or_outside_is_not() {
        assert_eq!(place(300, 200, WINDOW, Some(CONTENT)), Ok(()));
        // The title bar: in the window, not in its content.
        assert_eq!(place(300, 60, WINDOW, Some(CONTENT)), Err(&FRAME));
        // Beside the window, and on its right and bottom edges, which are outside it.
        assert_eq!(place(99, 200, WINDOW, Some(CONTENT)), Err(&OUTSIDE));
        assert_eq!(place(500, 200, WINDOW, Some(CONTENT)), Err(&OUTSIDE));
        assert_eq!(place(300, 350, WINDOW, Some(CONTENT)), Err(&OUTSIDE));
        // The top-left corner is in, as on Windows' rectangles.
        assert_eq!(place(100, 78, WINDOW, Some(CONTENT)), Ok(()));
    }

    #[test]
    fn a_control_has_no_frame_of_its_own() {
        let control = Frame { x: 120.0, y: 90.0, w: 40.0, h: 20.0 };
        assert_eq!(place(130, 95, control, None), Ok(()));
        assert_eq!(place(170, 95, control, None), Err(&OUTSIDE));
    }

    #[test]
    fn the_place_in_the_window_is_measured_from_its_top_left_corner() {
        assert_eq!(window_location(340, 210, WINDOW), (240.0, 160.0));
        // A window on a display left of and above the primary one: negative bounds.
        let left = Frame { x: -1200.0, y: -300.0, w: 600.0, h: 400.0 };
        assert_eq!(window_location(-1000, -250, left), (200.0, 50.0));
    }

    #[test]
    fn the_log_line_names_the_process_the_window_and_the_pointer() {
        let l = line(12, 8812, 4242, "Logic Pro", (240.0, 160.0), (340, 210), Ok(true));
        assert_eq!(
            l,
            "posted click for window 12: into window number 8812 of process 4242 (Logic Pro) at 240,160 from the \
             window's top-left corner (340,210 on the screen); pointer moved: yes"
        );
        assert!(line(12, 1, 2, "x", (0.0, 0.0), (0, 0), Ok(false)).ends_with("pointer moved: no"));
        assert!(line(12, 1, 2, "x", (0.0, 0.0), (0, 0), Err(1001))
            .ends_with("pointer moved: no (CGWarpMouseCursorPosition answered 1001)"));
    }

    #[test]
    fn the_macs_own_refusals_are_said_as_every_other_reason_is() {
        use crate::errors::input::{NOT_ANSWERING, NOT_ON_SCREEN, NOT_PAIRED, NO_POST_ACCESS, NO_WINDOW_LOCATION};
        use crate::errors::Code;
        // A module is told these as it is told Windows': a lower-case clause, no full stop.
        for why in [&NO_POST_ACCESS, &NO_WINDOW_LOCATION, &NOT_PAIRED, &NOT_ON_SCREEN, &NOT_ANSWERING] {
            assert!(why.message.starts_with(|c: char| c.is_ascii_lowercase()) && !why.message.ends_with('.'), "{}", why.message);
        }
        assert!(NO_POST_ACCESS.message.contains("Accessibility"), "the permission is named, as the start block names it");
        let codes = [NO_POST_ACCESS.code, NO_WINDOW_LOCATION.code, NOT_PAIRED.code, NOT_ON_SCREEN.code, NOT_ANSWERING.code];
        assert_eq!(codes, [Code::Denied, Code::NotSupported, Code::Hidden, Code::Hidden, Code::Timeout]);
    }

    #[test]
    fn the_mark_is_not_the_pass_on_mark() {
        // `keys::PASS_ON_MARK`, "PASS": the tap lets an event carrying it through untouched.
        assert_ne!(POSTED_CLICK_MARK, 0x5041_5353);
        assert_eq!(&POSTED_CLICK_MARK.to_be_bytes()[4..], b"POST");
    }
}
