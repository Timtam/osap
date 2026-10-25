//! Synthesising mouse and keyboard input.
//!
//! Nothing here may sleep. A press-and-hold gesture is composed by the caller out of
//! `mouse_down`, a timer, and `mouse_up`, precisely so the waiting happens somewhere that
//! is not the thread carrying speech, hotkeys and detection.
//!
//! Every coordinate below is a global point with the origin at the top-left of the primary
//! display, which is already the space `CGEvent` locations are expressed in. So unlike
//! capture, and unlike anything read out of AppKit, nothing in this file flips an axis or
//! divides by a scale factor; a Retina display changes none of these numbers.
//!
//! Two facts about macOS shape the rest of it. Modifiers travel as *flags on the event*
//! rather than as key presses of their own, so `key_send` sets them instead of pressing
//! them — see there for why that is the safer of the two. And a pointer movement made while
//! a button is held is a different event type from one made with nothing held:
//! `LeftMouseDragged`, not `MouseMoved`. Every move here therefore asks the session which
//! buttons are down before it decides what to post. Getting that second one wrong fails
//! quietly in the worst way — the pointer arrives exactly where it was asked to, and the
//! control that was tracking the drag simply never learns that it moved.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use objc2_application_services::AXIsProcessTrusted;
use objc2_core_foundation::{CFRetained, CGPoint};
use objc2_core_graphics::{
    CGAssociateMouseAndMouseCursorPosition, CGError, CGEvent, CGEventField, CGEventFlags, CGEventSource,
    CGEventSourceStateID, CGEventTapLocation, CGEventType, CGMouseButton, CGPreflightPostEventAccess,
    CGScrollEventUnit, CGWarpMouseCursorPosition,
};

use super::{keys, post_click};
use crate::backend::{KeyRefusal, MouseButton};
use crate::errors::{input, HostError};

/// The event field AppKit's own mouse events carry their window's number in, and reads it from:
/// 51, `kCGSEventWindowIDField` — private, named in no SDK header; WebKit's
/// `Tools/TestRunnerShared/spi/CoreGraphicsTestSPI.h` declares it for its test runner's events, and
/// every pid-routed clicker that works sets it (`post_click.rs`).
const WINDOW_ID_FIELD: CGEventField = CGEventField(51);

/// The session state every question about "what is held right now" is asked against.
///
/// `HIDSystemState` is the rawer of the two and reports the physical keyboard and mouse;
/// `CombinedSessionState` reports what the user's login session sees, which is the physical
/// state plus anything already synthesised into it. The second is the right one here:
/// callers ask these questions to decide what their *next* synthesised event should look
/// like, and a modifier this process posted a moment ago is every bit as real to the
/// receiving application as one the user is holding. Neither is the per-application cached
/// state that `GetKeyState` was on Windows and that cost a day of debugging there.
const SESSION: CGEventSourceStateID = CGEventSourceStateID::CombinedSessionState;

thread_local! {
    /// One event source for the whole session.
    ///
    /// Every constructor here accepts `None` for the source and still produces a usable
    /// event, so this is an optimisation rather than a requirement — but `mouse_click` is
    /// the hottest input path in the system and a source carries the timing and coalescing
    /// state the window server uses to tell a stream of events apart from a burst of
    /// unrelated ones. `HIDSystemState` is the source that makes a synthesised event look
    /// like it came from the hardware, which is what the plugins this platform drives are
    /// written to react to.
    static SOURCE: RefCell<Option<CFRetained<CGEventSource>>> = RefCell::new(None);
    /// The event source a key passed on to the program in front is posted from (`pass_on`): a
    /// private state table, so that what it posts never enters the HID system's. That table is
    /// what `keyboard_now` reads as the keys and modifiers the user holds; posted from `SOURCE`, a
    /// Command passed on, or the key itself, could still read as held when the next key's turn
    /// comes in the same delivery, and refuse it for a hand that is not there — and a real key
    /// typed between a posted Command's press and its release could take it on. Every event
    /// passed on carries its flags explicitly, so the program loses nothing by it.
    static PASS_SOURCE: RefCell<Option<CFRetained<CGEventSource>>> = RefCell::new(None);
}

/// The pass-on's own event source ([`PASS_SOURCE`]), created on first use; `None` if the system
/// would not create one, and then nothing is passed on — the shared source would bring back what
/// this one exists to keep out.
fn pass_source() -> Option<CFRetained<CGEventSource>> {
    PASS_SOURCE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = CGEventSource::new(CGEventSourceStateID::Private);
        }
        slot.as_ref().cloned()
    })
}

/// The shared event source, created on first use.
///
/// Returns a clone rather than lending out of the `RefCell`, because the borrow would
/// otherwise be alive across a `CGEventPost` and a second borrow would panic rather than
/// fail — and a panic in the backend takes the process down with it.
fn source() -> Option<CFRetained<CGEventSource>> {
    SOURCE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = CGEventSource::new(CGEventSourceStateID::HIDSystemState);
            if slot.is_none() {
                crate::logging::line(
                    "macos",
                    "CGEventSourceCreate(HIDSystemState) returned NULL; continuing with a null \
                     source, which still posts events but loses their shared timing state",
                );
            }
        }
        slot.as_ref().cloned()
    })
}

/// The post-event access last said (`post_click::PostAccess`, as a number: 0 nothing said yet).
static POST_ACCESS_SAID: AtomicU8 = AtomicU8::new(0);

/// Records in the log whether this process may synthesise input — at the first event posted, and
/// again whenever the answer changes, so a grant made during the run is seen.
///
/// `CGEventPost` returns nothing — not a status, not an error — so from in here a refused
/// event and a delivered one are the same call. The only thing that can be asked is whether
/// the permission is present, and a tester whose clicks do nothing needs that one sentence
/// in the log more than anything else this file could record. Asked lazily, at every event
/// posted, so the answer in the log is the state at the moment it mattered rather than at startup.
/// By both checks (`PostAccess`): the post-event check alone went on answering "denied" in a process
/// granted Accessibility during its run (the second Mac test, 2026-10-24), and said "restart" to a
/// tester whose clicks then worked. Answers what it found.
fn note_post_access() -> post_click::PostAccess {
    let access = post_click::PostAccess::of(CGPreflightPostEventAccess(), unsafe { AXIsProcessTrusted() });
    let code = access as u8 + 1;
    if POST_ACCESS_SAID.swap(code, Ordering::Relaxed) != code {
        crate::logging::line("macos", access.line());
    }
    access
}

/// Posts one event into the session, at the same place hardware enters it.
///
/// `HIDEventTap` rather than `SessionEventTap` so the event passes every tap the system has,
/// including our own — which is exactly the behaviour Windows has with `SendInput`, and
/// exactly the reason `key_post` exists.
fn post(event: &CGEvent) {
    note_post_access();
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(event));
}

// Taken by reference throughout, as they were written when `MouseButton` was neither `Copy` nor
// `Clone`; it is both now (the host keeps the buttons a module holds down, to let go of them).
fn cg_button(button: &MouseButton) -> CGMouseButton {
    match button {
        MouseButton::Left => CGMouseButton::Left,
        MouseButton::Right => CGMouseButton::Right,
        MouseButton::Middle => CGMouseButton::Center,
    }
}

/// The button's name for the log, in the words of the rest of this file's lines.
fn button_name(button: &MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

/// The (down, up, dragged) event types for a button. Middle is one of the numbered "other"
/// buttons on this platform rather than a type of its own.
fn button_events(button: &MouseButton) -> (CGEventType, CGEventType, CGEventType) {
    match button {
        MouseButton::Left => (
            CGEventType::LeftMouseDown,
            CGEventType::LeftMouseUp,
            CGEventType::LeftMouseDragged,
        ),
        MouseButton::Right => (
            CGEventType::RightMouseDown,
            CGEventType::RightMouseUp,
            CGEventType::RightMouseDragged,
        ),
        MouseButton::Middle => (
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
            CGEventType::OtherMouseDragged,
        ),
    }
}

/// Which button the session has down right now, if any.
///
/// Three cheap state reads, and they decide whether a move is a move or a drag. A composed
/// press-and-hold-and-glide — `mouse_down`, timer, `mouse_move`, timer, `mouse_up` — is the
/// gesture the down/up split was created for, and on this platform its middle leg has to be
/// a dragged event or the flyout being glided over never sees it.
fn held_button() -> Option<MouseButton> {
    if CGEventSource::button_state(SESSION, CGMouseButton::Left) {
        Some(MouseButton::Left)
    } else if CGEventSource::button_state(SESSION, CGMouseButton::Right) {
        Some(MouseButton::Right)
    } else if CGEventSource::button_state(SESSION, CGMouseButton::Center) {
        Some(MouseButton::Middle)
    } else {
        None
    }
}

/// Builds one mouse event, for a call to post with the others it makes once every one of them is
/// built: a call whose event the system would not make posts nothing, and so never leaves a
/// button down (`post_all`). `clicks` sets the click-state field, which is what distinguishes a
/// press from the tail of a double-click; a plain click wants 1.
fn mouse_event(etype: CGEventType, x: i32, y: i32, button: CGMouseButton, clicks: Option<i64>) -> Result<CFRetained<CGEvent>, HostError> {
    let point = CGPoint::new(x as f64, y as f64);
    let src = source();
    match CGEvent::new_mouse_event(src.as_deref(), etype, point, button) {
        Some(event) => {
            // Cleared rather than inherited, for the reason `key_post` gives: a source built
            // on the hardware state hands the new event whatever modifiers are physically
            // held, and a click is often posted from inside a hotkey callback — while the
            // combination that triggered it is still down. Found by review of the shortcut
            // that puts the keyboard back into a plugin: it clicks while a five-key chord is
            // held, and a Control-modified left click is a secondary click on this platform,
            // so the click would have opened a context menu instead of moving the keyboard.
            // Keys had this clearing since Melodyne wanted a bare F-key; mouse events did not,
            // and the reference for `send` said so in as many words.
            CGEvent::set_flags(Some(&event), CGEventFlags::empty());
            if let Some(n) = clicks {
                CGEvent::set_integer_value_field(
                    Some(&event),
                    CGEventField::MouseEventClickState,
                    n,
                );
            }
            Ok(event)
        }
        None => {
            crate::logging::line(
                "macos",
                &format!("CGEventCreateMouseEvent(type {}) returned NULL at {x},{y}; nothing of that input was sent", etype.0),
            );
            let said = format!("CGEventCreateMouseEvent(type {}) returned NULL; nothing was sent", etype.0);
            Err(HostError::with(&input::NULL_MOUSE_SENT, said))
        }
    }
}

/// Posts a call's events, every one of them built already: `CGEventPost` answers nothing, so once
/// they are built nothing of the call can fail.
fn post_all(events: &[CFRetained<CGEvent>]) {
    for event in events {
        post(event);
    }
}

/// The movement to `(x, y)`, the drag form of the event when a button is held.
fn glide_event(x: i32, y: i32) -> Result<CFRetained<CGEvent>, HostError> {
    match held_button() {
        Some(b) => {
            let (_, _, dragged) = button_events(&b);
            mouse_event(dragged, x, y, cg_button(&b), Some(1))
        }
        None => mouse_event(CGEventType::MouseMoved, x, y, CGMouseButton::Left, None),
    }
}

pub fn cursor_pos() -> (i32, i32) {
    // An event created with no type is Quartz's way of snapshotting the current input state,
    // and its location is the pointer in the global, top-left, point-based space everything
    // else here uses. `NSEvent::mouseLocation` answers the same question but bottom-left and
    // only on the main thread, so it would need flipping against a screen height that this
    // module has no business knowing.
    match CGEvent::new(None) {
        Some(event) => {
            let p = CGEvent::location(Some(&event));
            (p.x.round() as i32, p.y.round() as i32)
        }
        None => {
            crate::logging::line(
                "macos",
                "CGEventCreate returned NULL; reporting the cursor at 0,0, which is a guess",
            );
            (0, 0)
        }
    }
}

pub fn mouse_move(x: i32, y: i32) -> Result<(), HostError> {
    crate::logging::trace("macos", || format!("mouse_move {x},{y}"));
    let glide = glide_event(x, y)?;
    post_all(&[glide]);
    Ok(())
}

pub fn mouse_click(x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
    crate::logging::trace("macos", || {
        format!("mouse_click {x},{y} {}", button_name(&button))
    });
    let (down, up, _) = button_events(&button);
    let cg = cg_button(&button);
    // The move first, even though the press carries its own location and would warp the
    // pointer by itself. Custom-drawn plugin UIs — which is nearly everything this platform
    // touches — hit-test on movement and highlight on hover, and a press that arrives at a
    // position they were never told about lands on whatever they still believe is under the
    // pointer. This leg is not asked whether a button is held: a click is by definition not
    // part of a held gesture, and this is the hottest input path in the system.
    let built = [
        mouse_event(CGEventType::MouseMoved, x, y, CGMouseButton::Left, None)?,
        mouse_event(down, x, y, cg, Some(1))?,
        mouse_event(up, x, y, cg, Some(1))?,
    ];
    post_all(&built);
    Ok(())
}

/// `host.input.click`'s `clicks = 2`: the job for the host's `input-gesture` thread that makes the
/// double-click there (`double_click.rs`) with a [`Hand`], `hit` the window the system's hit test
/// named at the point at the call (`ax::hit_window_at`), whose program's windows the hand compares.
/// `interval` is the user's double-click time, for the line.
pub fn double_click(x: i32, y: i32, button: MouseButton, hit: Option<u32>, interval: std::time::Duration) -> crate::backend::ActJob {
    crate::backend::double_click::job(x, y, button, "click states 1 and 2", interval, move || Hand { x, y, button, hit, up: None })
}

/// A double-click's hand on a Mac (`double_click.rs`), made on the `input-gesture` thread and used
/// there alone: its events are that thread's, posted from that thread's own event source (`SOURCE`
/// is per thread), at the HID level as every click here — `CGEventPost` may be called from any
/// thread.
///
/// What makes the pair a double-click is the click state each event carries
/// (`kCGMouseEventClickState`), as the VPS Avenger control tool posts it (`clicker/driver.py`,
/// `double_click` and `right_double_click`): 1 on the first press and release, 2 on the second —
/// Apple: "A click state of 1 represents a single click. A click state of 2 represents a
/// double-click." It is what AppKit hands a program as the events' `clickCount`. The move primes the
/// point once, before the first press, as the tool does (`_prime_cursor`): a custom-drawn view that
/// hit-tests on movement learns where the press is. The modifier flags are cleared, as on every click
/// here (`mouse_event`). The right button is `RightMouseDown`/`Up` with `CGMouseButton::Right`.
///
/// A pair's events are all made before the first of them is posted, so one the system would not make
/// posts nothing of the pair: the first pair's answers `input::NULL_MOUSE_SENT`, nothing clicked; the
/// second's `input::DOUBLE_SECOND_NULL`, the first click made. The release is made with its press and
/// posted [`double_click::PRESS_MS`](crate::backend::double_click::PRESS_MS) later, stamped afresh
/// first, so the time from press to release is in the events as well as in their delivery. A press
/// that went out is released when the hand is dropped with its release still owed — the thread
/// unwinding between the two.
///
/// The window at the point is the window server's list's (`ax::listed_window_at`), CG alone, as any
/// thread may ask it: the hit test a click asks (`NSWindow`'s) is the main thread's, and only its
/// window's number comes here — its owner is read off each read's own copy of the list.
struct Hand {
    x: i32,
    y: i32,
    button: MouseButton,
    /// The window the hit test named at the point at the call, whose program's windows alone are
    /// compared; `None` where it named none.
    hit: Option<u32>,
    /// The release owed: the press of its pair went out, it has not.
    up: Option<CFRetained<CGEvent>>,
}

impl crate::backend::double_click::Hand for Hand {
    fn at_point(&mut self) -> Option<u64> {
        super::ax::listed_window_at(self.x, self.y, self.hit).map(u64::from)
    }

    fn press(&mut self, n: u8) -> Result<(), HostError> {
        let (down, up, _) = button_events(&self.button);
        let cg = cg_button(&self.button);
        let (x, y) = (self.x, self.y);
        let clicks = Some(i64::from(n));
        // The second pair's event not made: the first click was.
        let make = |etype: CGEventType, b: CGMouseButton, clicks: Option<i64>| {
            mouse_event(etype, x, y, b, clicks).map_err(|e| {
                if n == 1 {
                    e
                } else {
                    let said = format!("CGEventCreateMouseEvent(type {}) returned NULL for the second click; the first click was made", etype.0);
                    HostError::with(&input::DOUBLE_SECOND_NULL, said)
                }
            })
        };
        let mut built = Vec::with_capacity(2);
        if n == 1 {
            built.push(make(CGEventType::MouseMoved, CGMouseButton::Left, None)?);
        }
        built.push(make(down, cg, clicks)?);
        let release = make(up, cg, clicks)?;
        post_all(&built);
        self.up = Some(release);
        Ok(())
    }

    fn release(&mut self, _n: u8) -> Result<(), HostError> {
        if let Some(up) = self.up.take() {
            // Stamped now: it was made with its press, PRESS_MS ago. A snapshot of the input state
            // carries the moment it was made, in the events' own unit, as `cursor_pos` reads it.
            if let Some(now) = CGEvent::new(None) {
                CGEvent::set_timestamp(Some(&up), CGEvent::timestamp(Some(&now)));
            }
            post(&up);
        }
        Ok(())
    }
}

impl Drop for Hand {
    fn drop(&mut self) {
        let _ = crate::backend::double_click::Hand::release(self, 0);
    }
}

/// `host.input.click`'s `via = "post"`: the move, the press and the release at (x, y) handed to the
/// process of `window` with `CGEventPostToPid`, each carrying the window's number and the place in
/// the window, so that AppKit gives them to that window while its application stays where it is
/// (`post_click.rs` says what that rests on). `Ok` with the log line once they are posted — not
/// that the program did anything with them, which nothing answers; `Err` with the refusal, and then
/// nothing was posted and the pointer was not moved.
///
/// In this order: whether this application may post events at all — `CGEventPostToPid` returns
/// nothing, and without the privilege the window server drops the events in silence, so that is
/// asked first, of the privacy database, and refused rather than answered `true`; the window
/// (`ax::post_target`); every event built and marked before the first is posted, so one that
/// cannot be built, or a macOS without `CGEventSetWindowLocation`, posts nothing; the pointer
/// warped to the point with `move_pointer`; then the three posts, back to back. Never sleeps, and
/// never waits for the program: a post is a message into its event stream.
///
/// The modifier flags are cleared, as on every click here (`mouse_event`): a Control held from the
/// hotkey that asked for the click would make it a secondary click.
pub fn post_click(window: isize, x: i32, y: i32, button: MouseButton, move_pointer: bool) -> Result<String, HostError> {
    // Refused only where neither check allows it: the post-event check's "denied" with
    // Accessibility granted refuses nothing (`PostAccess`).
    if note_post_access().refuses() {
        return Err(HostError::of(&input::NO_POST_ACCESS));
    }
    let target = super::ax::post_target(window, x, y)?;
    let at = post_click::window_location(x, y, target.bounds);
    let point = CGPoint::new(f64::from(x), f64::from(y));
    let number = i64::from(target.number);
    let (down, up, _) = button_events(&button);
    let cg = cg_button(&button);
    let src = source();
    let mut built = Vec::with_capacity(3);
    for (etype, b, clicks) in [(CGEventType::MouseMoved, CGMouseButton::Left, None), (down, cg, Some(1)), (up, cg, Some(1))] {
        let Some(event) = CGEvent::new_mouse_event(src.as_deref(), etype, point, b) else {
            let said = format!("CGEventCreateMouseEvent(type {}) returned NULL; nothing was posted", etype.0);
            return Err(HostError::with(&input::NULL_MOUSE_EVENT, said));
        };
        CGEvent::set_flags(Some(&event), CGEventFlags::empty());
        let set = |field: CGEventField, value: i64| CGEvent::set_integer_value_field(Some(&event), field, value);
        if let Some(n) = clicks {
            set(CGEventField::MouseEventClickState, n);
        }
        // The window, three times: the private field AppKit reads it from, and the two public ones
        // the window server fills in for a real click.
        set(WINDOW_ID_FIELD, number);
        set(CGEventField::MouseEventWindowUnderMousePointer, number);
        set(CGEventField::MouseEventWindowUnderMousePointerThatCanHandleThisEvent, number);
        set(CGEventField::EventTargetUnixProcessID, i64::from(target.pid));
        set(CGEventField::EventSourceUserData, post_click::POSTED_CLICK_MARK);
        // Last: the place in the window, which nothing set after it may move.
        if !super::ffi::set_window_location(&event, CGPoint::new(at.0, at.1)) {
            return Err(HostError::of(&input::NO_WINDOW_LOCATION));
        }
        built.push(event);
    }
    let moved = if move_pointer { warp(point) } else { Ok(false) };
    for event in &built {
        CGEvent::post_to_pid(target.pid as libc::pid_t, Some(event));
    }
    crate::logging::trace("macos", || {
        format!("post_click {x},{y} {} to pid {} window number {}", button_name(&button), target.pid, target.number)
    });
    Ok(post_click::line(window, target.number, target.pid, &super::ax::exe_for_pid(target.pid), at, (x, y), moved))
}

/// The pointer put at `point` for a posted click, so that a program asking where the pointer is
/// while it handles the click — `[NSEvent mouseLocation]` — finds it on the point. A warp posts no
/// event of its own (`CGWarpMouseCursorPosition`: "moves the mouse cursor without generating
/// events"); and after one, macOS holds the mouse's own movement off the pointer for about a
/// quarter of a second, which `CGAssociateMouseAndMouseCursorPosition` ends at once (the remedy SDL,
/// GLFW and Wine apply after their warps). Apple's header describes that call for an application
/// in the foreground, and a posted click is mostly made while this one is not — a page described
/// `activates = false`, an overlay's key with the DAW in front — where whether the user's hand gets
/// the pointer back at once is not measured (TODO.md, MW21). `Ok(true)` when it was warped, `Err`
/// with the system's code when not.
fn warp(point: CGPoint) -> Result<bool, i32> {
    let e = CGWarpMouseCursorPosition(point);
    let _ = CGAssociateMouseAndMouseCursorPosition(true);
    if e == CGError::Success {
        Ok(true)
    } else {
        Err(e.0)
    }
}

pub fn mouse_drag(x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton) -> Result<(), HostError> {
    crate::logging::trace("macos", || {
        format!("mouse_drag {x1},{y1} -> {x2},{y2} {}", button_name(&button))
    });
    let (down, up, dragged) = button_events(&button);
    let cg = cg_button(&button);
    // No dwell between the press and the move, matching Windows. The single intermediate
    // drag event is what a scrollbar needs; a control that only reveals itself after a
    // sustained press is served by `mouse_down`/`mouse_up` and a timer instead, which is
    // the whole reason those two exist separately.
    let built = [
        glide_event(x1, y1)?,
        mouse_event(down, x1, y1, cg, Some(1))?,
        mouse_event(dragged, x2, y2, cg, Some(1))?,
        mouse_event(up, x2, y2, cg, Some(1))?,
    ];
    post_all(&built);
    Ok(())
}

pub fn mouse_down(x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
    crate::logging::trace("macos", || {
        format!("mouse_down {x},{y} {}", button_name(&button))
    });
    let (down, _, _) = button_events(&button);
    let built = [glide_event(x, y)?, mouse_event(down, x, y, cg_button(&button), Some(1))?];
    post_all(&built);
    Ok(())
}

pub fn mouse_up(x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
    crate::logging::trace("macos", || {
        format!("mouse_up {x},{y} {}", button_name(&button))
    });
    let (_, up, _) = button_events(&button);
    // Moving before releasing is what makes "press here, glide there, let go" work when the
    // caller composes it out of down, a timer and up: `glide_event` sees our own button still
    // held and makes a drag, so the release lands on a control that has been following the
    // pointer all the way rather than on one that never heard it leave.
    let built = [glide_event(x, y)?, mouse_event(up, x, y, cg_button(&button), Some(1))?];
    post_all(&built);
    Ok(())
}

pub fn mouse_scroll(x: i32, y: i32, lines: i32) -> Result<(), HostError> {
    crate::logging::trace("macos", || format!("mouse_scroll {x},{y} by {lines} line(s)"));
    let glide = glide_event(x, y)?;
    let src = source();
    // `lines` arrives already converted — see the trait implementation, which is where the
    // two units meet. The trait counts what Windows counts, 120 units to a detent, because
    // `WHEEL_DELTA` is a raw resolution unit with no name of its own and a finer wheel
    // reports less than one. Quartz names its unit instead, and one `Line` is one detent, so
    // there is nothing left to multiply by here — and inventing a factor at this depth would
    // only make a notch mean different things on the two platforms.
    //
    // Sign follows Windows too: positive is a wheel turned away from the user. Whether the
    // user's "natural scrolling" preference then inverts what the content does is a question
    // only a Mac can answer, and it is the first thing to check if scrolling goes the wrong
    // way — the flip is applied to hardware events below the point this one is injected.
    match CGEvent::new_scroll_wheel_event2(src.as_deref(), CGScrollEventUnit::Line, 1, lines, 0, 0)
    {
        Some(event) => {
            // A scroll event has no position of its own until it is given one, and it is
            // delivered to whatever sits under that position rather than under the pointer.
            // Setting it explicitly means the target does not depend on the move posted just
            // above having been processed first.
            CGEvent::set_location(Some(&event), CGPoint::new(x as f64, y as f64));
            // Cleared for the same reason `mouse_event` clears, and here because leaving one
            // synthesised mouse event inheriting the chord is worse than leaving all of
            // them: Shift plus a wheel is a horizontal scroll to AppKit and a fine step to
            // several plug-in frameworks, so a module scrolling from its own hotkey handler
            // would move the wrong axis and then read back "it did not move". No shipped
            // caller does that today — both reach `scroll` from bare captures — which is why
            // this is symmetry rather than a fix for something measured.
            CGEvent::set_flags(Some(&event), CGEventFlags::empty());
            post_all(&[glide, event]);
            Ok(())
        }
        None => {
            crate::logging::line("macos", &format!("CGEventCreateScrollWheelEvent2 returned NULL; no scroll sent at {x},{y}"));
            Err(HostError::of(&input::NULL_SCROLL_EVENT))
        }
    }
}

static KEY_POST_ROUTE_LOGGED: AtomicBool = AtomicBool::new(false);

/// Deliver a key to one window without it entering the input queue. A Win32 idea; see the
/// trait for why it exists and what the honest macOS answer is.
///
/// The Win32 version posts `WM_KEYDOWN`/`WM_KEYUP` to a window, which no other part of the
/// system observes. macOS has no per-window message queue to post into, so the nearest
/// equivalent is one step coarser: `CGEventPostToPid` hands the event to a *process*, and
/// it is delivered without travelling past the session-wide taps — which is the property
/// that matters, because the alternative is that our own key capture sees the key we just
/// sent and fires the handler that sent it, forever.
///
/// Coarser is good enough for the caller that exists. Melodyne is a single-window
/// application being stepped through its sub-tools by repeated function-key presses, and the
/// window addressed is its own front window. Where it is not good enough, the precedent the
/// project already set is to fall back to a click rather than to invent a mechanism: the
/// `WM_COMMAND` menu driver was deleted for exactly that reason. `AXUIElementPostKeyboardEvent`
/// would be the per-element route and is not one — it is deprecated, undocumented as to which
/// element it actually targets, and reported not to work at all on recent systems.
/// A key name that is not one raises; a key the layout cannot type, a handle that is no window's,
/// and an event that could not be made are answered.
pub fn key_post(hwnd: isize, key: &str) -> Result<(), KeyRefusal> {
    let vk = crate::backend::key_to_vk(key).ok_or_else(|| format!("unknown key '{key}'"))?;
    let code = keys::vk_to_keycode(vk).ok_or_else(|| {
        let said = format!("key '{key}' (VK {vk:#04x}): {}", keys::why_no_keycode(vk));
        KeyRefusal::Answer(HostError::with(&input::POST_NO_KEYCODE, said))
    })?;
    let entry = super::handles::get(hwnd).ok_or_else(|| {
        let said = format!("key_post: {hwnd} is not a handle this backend issued, so there is no process to post '{key}' to");
        KeyRefusal::Answer(HostError::with(&input::POST_NO_PROCESS, said))
    })?;
    if !KEY_POST_ROUTE_LOGGED.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "key_post: routing keys with CGEventPostToPid (first call: '{key}' to pid {}). \
                 This is the macOS stand-in for PostMessage and is delivered per process, not \
                 per window. If a posted key ever re-triggers the overlay's own hotkey, or \
                 lands in a different window of the same application, this is the line to \
                 suspect.",
                entry.pid
            ),
        );
    }
    let src = source();
    // Both built before either is posted, so an event the system would not make posts nothing and
    // leaves no key down.
    let mut built = Vec::with_capacity(2);
    for is_down in [true, false] {
        match CGEvent::new_keyboard_event(src.as_deref(), code, is_down) {
            Some(event) => {
                // Cleared rather than inherited. A source built on the hardware state hands
                // the new event whatever modifiers are physically held, and `key_post` is
                // called from inside a hotkey callback — that is, while the combination that
                // triggered it is still down. Melodyne wants a bare F-key, not Alt+F-key. A key
                // of the keypad keeps the keypad's flag, as the keyboard's own event has it.
                CGEvent::set_flags(Some(&event), CGEventFlags(keys::keypad_flag(code)));
                built.push(event);
            }
            None => {
                let said = format!("CGEventCreateKeyboardEvent returned NULL for '{key}' (keycode {code})");
                return Err(KeyRefusal::Answer(HostError::with(&input::NULL_KEY_EVENT, said)));
            }
        }
    }
    note_post_access();
    for event in &built {
        CGEvent::post_to_pid(entry.pid as libc::pid_t, Some(event));
    }
    crate::logging::trace("macos", || {
        format!("key_post '{key}' (keycode {code}) to pid {}", entry.pid)
    });
    Ok(())
}

/// A spec that does not parse, or a modifier tap, raises; a key the layout cannot type, and an
/// event that could not be made, are answered.
pub fn key_send(combo: &str) -> Result<(), KeyRefusal> {
    // The shared parser, so a spec means here what it means to a hotkey: its modifiers are
    // roles, and `keys::MODIFIER_KEYS` makes them Mac keys — `"Ctrl+C"` is Command+C, which
    // copies, as it does on Windows; `"Win+C"` (or `"Meta+C"`) is Control+C.
    let (vk, mask) = crate::backend::parse_key_spec(combo)?;
    if mask & crate::backend::MASK_TAP != 0 {
        return Err(KeyRefusal::Mistake(format!(
            "'{combo}' is a modifier tap, which is something to capture, not a key to send"
        )));
    }

    // The modifiers become flags on the key event rather than key events of their own, and
    // that is a deliberate divergence from the Windows implementation's press-key-release
    // ordering. Three reasons, in order of weight.
    //
    // It cannot leave a modifier stuck. A synthesised Command press that never gets its
    // release — because the release was dropped, or the process died between the two — locks
    // the whole session into a state a blind user cannot see, cannot diagnose and cannot
    // clear without sighted help. A flag lives on one event and is gone when the event is.
    //
    // It is what Quartz documents. A modifier key posted as an ordinary key event does not
    // reliably become a modifier for the events that follow it; applications read the flags
    // off each event, so the flags have to be set even when the modifier key was pressed as
    // well. Setting them and skipping the presses drops the half that was never load-bearing.
    //
    // And it isolates the send from what the user is holding. `CGEventSetFlags` replaces the
    // whole set, so the Alt that is still down from the hotkey which triggered this call does
    // not turn a sent Escape into Alt+Escape. That is strictly better than the Windows
    // behaviour, which does inherit it.
    //
    // The cost, if a tester finds an application that ignores flag-only modifiers: the fix is
    // to bracket the pair below with `FlagsChanged` events carrying the accumulating and then
    // receding flag set. Nothing else in this file would change.
    // The mask also picks the letter table: with Command held (the Ctrl role) a letter is the
    // key the layout's Command table puts it on (`keys::letters_for`).
    let code = keys::vk_to_keycode_for(vk, mask).ok_or_else(|| {
        let said = format!("'{combo}' (VK {vk:#04x}): {}", keys::why_no_keycode(vk));
        KeyRefusal::Answer(HostError::with(&input::SEND_NO_KEYCODE, said))
    })?;
    // A key of the keypad carries the keypad's flag besides the modifiers', as the keyboard's own
    // event does: setting the flags replaces the whole set.
    let flags = CGEventFlags(keys::mask_to_cg_flags(mask) | keys::keypad_flag(code));

    let src = source();
    // Both built before either is posted, so an event the system would not make sends nothing and
    // leaves no key down.
    let mut built = Vec::with_capacity(2);
    for is_down in [true, false] {
        match CGEvent::new_keyboard_event(src.as_deref(), code, is_down) {
            Some(event) => {
                // On the release as well as the press. An application that tracks modifiers
                // by watching the flags on every event it receives reads a release whose
                // flags disagree with its press as a modifier that was let go early, and some
                // of them then behave as though it were still held.
                CGEvent::set_flags(Some(&event), flags);
                // Marked, so that the tap leaves the key-down out of the count that ends a count
                // of quick repeats (`keys::SEND_MARK`); it takes and matches the key as ever.
                CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, keys::SEND_MARK);
                built.push(event);
            }
            None => {
                let said = format!("CGEventCreateKeyboardEvent returned NULL for '{combo}' (keycode {code})");
                return Err(KeyRefusal::Answer(HostError::with(&input::NULL_KEY_EVENT, said)));
            }
        }
    }
    post_all(&built);
    crate::logging::trace("macos", || {
        format!("key_send '{combo}' -> keycode {code} flags {:#x}", flags.0)
    });
    Ok(())
}

/// The keyboard now, for passing key `vk`, pressed with `mask` on keycode `phys`, on to the
/// program in front: the window in front asked afresh of the frontmost application
/// (`ax::foreground_window_id`, one accessibility round trip, asked only for a key about to be
/// passed on) — not the tap's idea of it, which a window opening in an application already in front
/// need not change, so a dialog that opened meanwhile would get the key; `None` when the
/// application did not answer. The key — `phys`, or looked up for `mask` where it is not known —
/// held down as the HID system reports it, the hardware's state below every tap, or as the tap's own
/// record of a key-down it swallowed has it; the modifier roles the HID system reports held; no
/// screen reader's key (VoiceOver's are Control and Option, roles like any other); and the tap's
/// count of key-downs let through. A key with no keycode is not held; `pass_on` then says it has
/// none.
pub fn keyboard_now(vk: u32, mask: u8, phys: Option<u16>) -> crate::backend::KeyboardNow {
    let hid = CGEventSourceStateID::HIDSystemState;
    let key_held = phys
        .or_else(|| keys::vk_to_keycode_for(vk, mask))
        .is_some_and(|code| CGEventSource::key_state(hid, code) || super::tap::held_back(code));
    crate::backend::KeyboardNow {
        front: super::ax::foreground_window_id(),
        key_held,
        mods: keys::mask_of_cg_flags(CGEventSource::flags_state(hid).0),
        reader_mod: false,
        let_through: super::tap::let_through(),
    }
}

/// Posts `strokes` of key `vk`, pressed with `mask` on keycode `phys`, to the program in front
/// (`keys::pass_on_events`): a modifier's key as a `FlagsChanged` event, the key's own down and
/// up — on `phys` where it is known, so that the keypad's Enter is not sent as Return — each with
/// the flags of the roles held from it on, and each marked with [`keys::PASS_ON_MARK`] in its
/// event-source user data, which the tap lets through untouched. At the HID level, as `post` does
/// everything, but from a private event source ([`PASS_SOURCE`]). Every event is built before the
/// first is posted, so one that cannot be built posts nothing and leaves nothing down.
pub fn pass_on(vk: u32, mask: u8, phys: Option<u16>, strokes: &[crate::backend::Stroke]) -> Result<(), String> {
    let code = phys
        .or_else(|| keys::vk_to_keycode_for(vk, mask))
        .ok_or_else(|| format!("VK {vk:#04x}: {}", keys::why_no_keycode(vk)))?;
    let src = pass_source().ok_or_else(|| "CGEventSourceCreate(Private) returned NULL".to_string())?;
    let mut built = Vec::with_capacity(strokes.len());
    for e in keys::pass_on_events(code, mask, strokes) {
        let Some(event) = CGEvent::new_keyboard_event(Some(&*src), e.keycode, e.down) else {
            return Err(format!("CGEventCreateKeyboardEvent returned NULL for keycode {}", e.keycode));
        };
        if e.modifier {
            CGEvent::set_type(Some(&event), CGEventType::FlagsChanged);
        }
        CGEvent::set_flags(Some(&event), CGEventFlags(e.flags));
        CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, keys::PASS_ON_MARK);
        built.push(event);
    }
    for event in &built {
        post(event);
    }
    crate::logging::trace("macos", || format!("pass_on vk {vk:#04x} mask {mask}: {} event(s)", built.len()));
    Ok(())
}

/// Every character's down and up built before they are posted, so an event the system would not
/// make ends the typing between two characters, never inside one: the answer says how far it got.
pub fn type_text(text: &str) -> Result<(), HostError> {
    // The keyboard layout is bypassed entirely, exactly as `KEYEVENTF_UNICODE` does on
    // Windows: the events carry keycode 0 and a Unicode payload that overrides it, so what
    // arrives is the character asked for rather than whatever the user's current layout puts
    // on some key. A German layout, a Dvorak layout and a dead key in the middle of the
    // string all stop mattering.
    let src = source();
    let mut sent = 0usize;
    for ch in text.chars() {
        // One event pair per CHARACTER, not per UTF-16 code unit. Windows sends a unit at a
        // time and gets away with it; here a lone surrogate half in a payload of its own is
        // not text, so anything outside the basic plane would arrive as two pieces of
        // nothing. Encoding per character keeps a surrogate pair together in one payload.
        let mut buf = [0u16; 2];
        let units: &[u16] = ch.encode_utf16(&mut buf);
        let mut pair = Vec::with_capacity(2);
        for is_down in [true, false] {
            match CGEvent::new_keyboard_event(src.as_deref(), 0, is_down) {
                Some(event) => {
                    // Cleared, for the same reason as in `key_post`: a Command still held
                    // from the hotkey that started this would turn every typed character into
                    // a menu shortcut.
                    CGEvent::set_flags(Some(&event), CGEventFlags::empty());
                    // SAFETY: `units` is a slice of one or two initialised `u16`s that
                    // outlives the call, and its length is passed as the count. `UniCharCount`
                    // is not exported by the bindings, so its underlying `c_ulong` is written
                    // out here.
                    unsafe {
                        CGEvent::keyboard_set_unicode_string(
                            Some(&event),
                            units.len() as core::ffi::c_ulong,
                            units.as_ptr(),
                        );
                    }
                    pair.push(event);
                }
                None => {
                    let said = format!(
                        "CGEventCreateKeyboardEvent returned NULL while typing; {sent} of {} characters were sent",
                        text.chars().count()
                    );
                    crate::logging::line("macos", &said);
                    return Err(HostError::with(&input::TEXT_CUT, said));
                }
            }
        }
        post_all(&pair);
        sent += 1;
    }
    crate::logging::trace("macos", || format!("type_text sent {sent} character(s)"));
    Ok(())
}

/// Are any of Command / Option / Control / Shift held right now? Read from the hardware
/// state, not from a per-application cache.
///
/// The caller is a hotkey handler deciding whether to defer: it is running while its own
/// combination is still down, and anything it synthesises next would arrive wearing those
/// modifiers. Caps Lock is excluded deliberately — it is a latch rather than something being
/// held, it is in these flags as `MaskAlphaShift`, and Windows does not count it either.
pub fn modifiers_down() -> bool {
    let flags = CGEventSource::flags_state(SESSION);
    flags.intersects(
        CGEventFlags::MaskCommand
            | CGEventFlags::MaskAlternate
            | CGEventFlags::MaskControl
            | CGEventFlags::MaskShift,
    )
}
