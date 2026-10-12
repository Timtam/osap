//! The keyboard after `host.window.focus` — where it lands, decided by events (HW1, the plan of
//! 2026-10-12). Pure: what a look at the window's GUI thread found goes in, what to do comes out.
//! The Windows backend makes the looks and the puts (`backend/windows.rs`); this file is built on
//! every platform, so `check-macos.ps1` builds it and its tests run everywhere.
//!
//! **Why.** Bringing a window to the front is not putting the keyboard where the user was. W7 of the
//! HTML probe saw REAPER's FX window come forward with the keyboard on the FX window and then on
//! REAPER's FX list; W10, after a round trip, saw the FX window in front with its dialog's keyboard
//! nowhere, and NVDA needed three presses of F7 before the keyboard was really in the plug-in. So
//! `host.window.focus(id)` of a CONTROL (HW1a) brings the control's top-level window forward and
//! then LANDS the keyboard on the control; and a page leaving the front gives the keyboard back to
//! the control it took the front from (HW1b), as well as the front.
//!
//! **How a landing decides.** It looks — on the call's own turn when the window is in front already,
//! at the window's foreground event, and at every focus event the event loop drains — and each look
//! reads the window's thread (`GetGUIThreadInfo`, local, no message sent):
//!
//! - the thread's active window is not the root yet: the activation is not processed; wait;
//! - the keyboard is on the control, or inside it: landed — logged once, and still looked at, since
//!   an application may move the keyboard on once more after its activation (W7);
//! - elsewhere in the window, or nowhere: a PUT — UI Automation's SetFocus on the control, asked on
//!   the host's accessibility thread and read back there — while fewer than [`LANDING_PUTS`] were
//!   made and the control is still a visible window inside the root;
//! - past the count: logged, and left there. The application keeps the keyboard where it wants it.
//!
//! **What ends it**, so it never moves the keyboard after the user acted: the user's first key-down
//! after the call (a repeat of a key already down at the call is not one), any `host.input` call,
//! another window's foreground event, the control destroyed or hidden, a newer landing. No timer.

// Its callers are the Windows backend's; on a Mac only its tests use it.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicU64, Ordering};

/// How many times a landing puts the keyboard on its control: once when the window's activation left
/// it elsewhere or nowhere, and once more when the application moved it on once after landing — W7's
/// "the FX window, then the FX list". A count of puts, never a time; a log that shows the
/// application moving it on after the second raises it, by a measurement.
pub(crate) const LANDING_PUTS: u32 = 2;

/// What a look at the root window's GUI thread found, with the counters a landing compares against
/// those at its start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Look {
    /// The thread's active window is the root: its activation has been processed.
    pub activated: bool,
    /// The thread's keyboard focus is the control or a window inside it.
    pub on_control: bool,
    /// The class of the window with the keyboard, or `None`: nowhere.
    pub focus_class: Option<String>,
    /// The control is still a visible window inside the root.
    pub control_there: bool,
    /// Fresh key-downs so far ([`KeyDowns::count`]).
    pub key_downs: u64,
    /// `host.input` calls so far.
    pub input_calls: u64,
}

/// What a landing says to do after a look.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Nothing now: look again at the next event.
    Wait,
    /// Put the keyboard on the control.
    Put,
    /// It is over.
    Ended(End),
}

/// Why a landing is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// The user pressed a key after the call.
    Key,
    /// A module made a `host.input` call.
    Input,
    /// Another window came to the front.
    Foreground,
    /// The control was destroyed or hidden.
    Gone,
    /// A newer `host.window.focus` started a landing of its own.
    Newer,
    /// The application moved the keyboard on after the last put: left there.
    LeftThere,
}

impl End {
    fn words(self) -> &'static str {
        match self {
            End::Key => "ended: a key was pressed",
            End::Input => "ended: a host.input call",
            End::Foreground => "ended: another window came to the front",
            End::Gone => "ended: the control is gone or hidden",
            End::Newer => "ended: a newer focus",
            End::LeftThere => "ended: left where the application put it",
        }
    }
}

/// One landing: the counters at the call, the puts made, and the lines its log owes.
#[derive(Debug)]
pub(crate) struct Landing {
    key_downs: u64,
    input_calls: u64,
    puts: u32,
    put_out: bool,
    put_from: Option<String>,
    landed: bool,
    waiting_said: bool,
    over: Option<End>,
    lines: Vec<String>,
}

impl Landing {
    /// A landing that starts now, with the fresh key-downs and the `host.input` calls counted so far.
    pub(crate) fn new(key_downs: u64, input_calls: u64) -> Landing {
        Landing {
            key_downs,
            input_calls,
            puts: 0,
            put_out: false,
            put_from: None,
            landed: false,
            waiting_said: false,
            over: None,
            lines: Vec::new(),
        }
    }

    /// The lines its log owes since the last call, in order.
    pub(crate) fn take_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    fn end(&mut self, why: End) -> Step {
        if self.over.is_none() {
            self.over = Some(why);
            self.lines.push(why.words().to_string());
        }
        Step::Ended(self.over.unwrap_or(why))
    }

    /// One look (see the file).
    pub(crate) fn look(&mut self, l: &Look) -> Step {
        if let Some(why) = self.over {
            return Step::Ended(why);
        }
        if l.key_downs != self.key_downs {
            return self.end(End::Key);
        }
        if l.input_calls != self.input_calls {
            return self.end(End::Input);
        }
        if !l.control_there {
            return self.end(End::Gone);
        }
        // A put on its way decides nothing: its answer looks again.
        if self.put_out {
            return Step::Wait;
        }
        if !l.activated {
            if !self.waiting_said {
                self.waiting_said = true;
                self.lines.push("waiting for the window's activation".to_string());
            }
            return Step::Wait;
        }
        if l.on_control {
            if !self.landed {
                self.landed = true;
                self.lines.push("the keyboard landed on it".to_string());
            }
            return Step::Wait;
        }
        if self.puts >= LANDING_PUTS {
            self.lines.push(format!(
                "the application moved it to {} after {} puts — left there",
                where_words(&l.focus_class),
                self.puts
            ));
            return self.end(End::LeftThere);
        }
        self.puts += 1;
        self.put_out = true;
        self.put_from = l.focus_class.clone();
        Step::Put
    }

    /// The answer to the last put: `Ok(true)` it took (the keyboard is on the control, read back),
    /// `Ok(false)` it did not, `Err` why it could not be asked. The next look decides what follows.
    pub(crate) fn put_answered(&mut self, took: Result<bool, String>) {
        self.put_out = false;
        if self.over.is_some() {
            return;
        }
        let from = where_words(&self.put_from.take());
        let answer = match took {
            Ok(true) => "took".to_string(),
            Ok(false) => "refused (the keyboard is not on it afterwards)".to_string(),
            Err(why) => format!("refused ({why})"),
        };
        self.lines.push(format!("the keyboard was on {from}; put on the control: {answer}"));
    }

    /// A foreground event of window `root_is_it` false: another window came to the front — over.
    /// The root's own is a look, which the caller makes.
    pub(crate) fn foreground(&mut self, root_is_it: bool) -> Option<End> {
        if !root_is_it && self.over.is_none() {
            self.end(End::Foreground);
        }
        self.over
    }

    /// A newer `host.window.focus` replaces it.
    pub(crate) fn newer(&mut self) {
        self.end(End::Newer);
    }
}

fn where_words(class: &Option<String>) -> String {
    match class {
        Some(c) => format!("'{c}'"),
        None => "nothing (nowhere)".to_string(),
    }
}

/// What `host.window.focus(id)` asks a landing for, given `root`, the top-level window of `id`;
/// whether `root` was in front at the call; and, when a page of this application is the foreground
/// window, the window that page took the front from and the control the keyboard was on there
/// (`came_from`, its `came_from_focus`; 0 for none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The control to land on, or `None`: no landing.
    pub land_on: Option<isize>,
    /// Look on the call's own turn: the window was in front already, and no foreground event will
    /// come (W10's case).
    pub look_now: bool,
}

/// HW1a and HW1b's third clause: a control's id lands on that control; a top-level id lands only
/// when a page of this application is in front and the id is the window that page came from — the
/// "focus first, then hide" order — on the control the page took the front from.
pub(crate) fn plan(id: isize, root: isize, root_in_front: bool, page: Option<(isize, isize)>) -> Plan {
    if id != root {
        return Plan { land_on: Some(id), look_now: root_in_front };
    }
    match page {
        Some((came_from, focus)) if came_from == id && focus != 0 => Plan { land_on: Some(focus), look_now: false },
        _ => Plan { land_on: None, look_now: false },
    }
}

/// The control a page keeps beside `came_from` when it takes the front (HW1b): `focus_now`, the
/// control the keyboard is on in `came_from` (0 when nowhere, or not inside it); when that is 0, the
/// one it kept before for the same `came_from` — a page shown again from a window whose keyboard
/// went nowhere (W10) still knows the control; 0 for another window.
pub(crate) fn came_from_focus(before: Option<(isize, isize)>, came_from: isize, focus_now: isize) -> isize {
    if focus_now != 0 {
        return focus_now;
    }
    match before {
        Some((was, focus)) if was == came_from => focus,
        _ => 0,
    }
}

/// The user's key-downs, counted for the landings: a key-down of a key that is not down already —
/// the keyboard's auto-repeat of one held since before a call is not a new key. Fed with every
/// physical key event raw input reports (the keyboard hook's watch, `hook_watch_thread.rs`, which
/// runs whenever a module holds a key). One bit per virtual key, and a counter; atomics, since raw
/// input arrives on a thread of its own and the landing reads on the event loop.
pub(crate) struct KeyDowns {
    held: [AtomicU64; 4],
    fresh: AtomicU64,
}

impl KeyDowns {
    pub(crate) const fn new() -> KeyDowns {
        KeyDowns { held: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)], fresh: AtomicU64::new(0) }
    }

    /// Key `vk` went down (`down`) or up.
    pub(crate) fn note(&self, vk: u32, down: bool) {
        let (word, bit) = (((vk & 0xff) / 64) as usize, 1u64 << (vk % 64));
        if down {
            if self.held[word].fetch_or(bit, Ordering::Relaxed) & bit == 0 {
                self.fresh.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.held[word].fetch_and(!bit, Ordering::Relaxed);
        }
    }

    /// How many fresh key-downs there have been.
    pub(crate) fn count(&self) -> u64 {
        self.fresh.load(Ordering::Relaxed)
    }
}

/// The application's one count of the user's key-downs (see [`KeyDowns`]).
pub(crate) static KEY_DOWNS: KeyDowns = KeyDowns::new();

#[cfg(test)]
mod tests {
    use super::*;

    fn look(activated: bool, on_control: bool, focus: Option<&str>) -> Look {
        Look {
            activated,
            on_control,
            focus_class: focus.map(str::to_string),
            control_there: true,
            key_downs: 0,
            input_calls: 0,
        }
    }

    #[test]
    fn waits_while_the_windows_activation_is_not_processed() {
        let mut l = Landing::new(0, 0);
        assert_eq!(l.look(&look(false, false, Some("#32770"))), Step::Wait);
        assert_eq!(l.look(&look(false, false, None)), Step::Wait);
        assert_eq!(l.take_lines(), vec!["waiting for the window's activation"], "said once");
    }

    #[test]
    fn landed_when_the_keyboard_is_on_the_control_or_inside_it() {
        let mut l = Landing::new(0, 0);
        assert_eq!(l.look(&look(true, true, Some("JUCE_PlugIn"))), Step::Wait);
        assert_eq!(l.look(&look(true, true, Some("JUCE_Child"))), Step::Wait);
        assert_eq!(l.take_lines(), vec!["the keyboard landed on it"], "said once");
        assert_eq!(l.look(&look(true, true, Some("JUCE_PlugIn"))), Step::Wait, "and still looked at");
    }

    #[test]
    fn a_put_when_the_keyboard_is_elsewhere_or_nowhere_and_a_second_when_it_moves_on_once() {
        let mut l = Landing::new(0, 0);
        assert_eq!(l.look(&look(true, false, None)), Step::Put, "nowhere: a put");
        assert_eq!(l.look(&look(true, false, None)), Step::Wait, "nothing while the put is out");
        l.put_answered(Ok(true));
        assert_eq!(l.look(&look(true, true, Some("JUCE_PlugIn"))), Step::Wait);
        assert_eq!(l.look(&look(true, false, Some("SysListView32"))), Step::Put, "moved on once: a second put");
        l.put_answered(Err("the program did not answer within 1000 ms".to_string()));
        assert_eq!(
            l.take_lines(),
            vec![
                "the keyboard was on nothing (nowhere); put on the control: took",
                "the keyboard landed on it",
                "the keyboard was on 'SysListView32'; put on the control: refused (the program did not answer within 1000 ms)",
            ]
        );
    }

    #[test]
    fn keeps_looking_after_it_landed_and_puts_again_when_the_application_moves_it() {
        let mut l = Landing::new(0, 0);
        assert_eq!(l.look(&look(true, true, Some("JUCE_PlugIn"))), Step::Wait);
        assert_eq!(l.look(&look(true, false, Some("SysListView32"))), Step::Put);
        l.put_answered(Ok(true));
        assert_eq!(l.look(&look(true, true, Some("JUCE_PlugIn"))), Step::Wait);
    }

    #[test]
    fn no_third_put_the_application_keeps_it_and_it_is_said() {
        let mut l = Landing::new(0, 0);
        for _ in 0..LANDING_PUTS {
            assert_eq!(l.look(&look(true, false, Some("SysListView32"))), Step::Put);
            l.put_answered(Ok(false));
        }
        assert_eq!(l.look(&look(true, false, Some("SysListView32"))), Step::Ended(End::LeftThere));
        let lines = l.take_lines();
        assert_eq!(lines[lines.len() - 2], "the application moved it to 'SysListView32' after 2 puts — left there");
        assert_eq!(lines[lines.len() - 1], "ended: left where the application put it");
        assert_eq!(l.look(&look(true, false, None)), Step::Ended(End::LeftThere), "nothing after it");
    }

    #[test]
    fn a_key_down_after_the_call_ends_it_but_a_repeat_of_a_key_held_at_the_call_does_not() {
        let keys = KeyDowns::new();
        keys.note(0x11, true); // Ctrl, held at the call
        let mut l = Landing::new(keys.count(), 0);
        keys.note(0x11, true); // its auto-repeat
        keys.note(0x11, true);
        let mut at = look(true, false, None);
        at.key_downs = keys.count();
        assert_eq!(l.look(&at), Step::Put, "a repeat is not a key");
        l.put_answered(Ok(true));
        keys.note(0x11, false);
        keys.note(0x09, true); // Tab: the user's own
        at.key_downs = keys.count();
        assert_eq!(l.look(&at), Step::Ended(End::Key));
        assert_eq!(l.take_lines().last().map(String::as_str), Some("ended: a key was pressed"));
        // A key released and pressed again is a new key-down.
        keys.note(0x09, false);
        let before = keys.count();
        keys.note(0x09, true);
        assert_eq!(keys.count(), before + 1);
    }

    #[test]
    fn a_host_input_call_ends_it() {
        let mut l = Landing::new(0, 3);
        let mut at = look(false, false, None);
        at.input_calls = 4;
        assert_eq!(l.look(&at), Step::Ended(End::Input));
    }

    #[test]
    fn another_windows_foreground_ends_it() {
        let mut l = Landing::new(0, 0);
        assert_eq!(l.foreground(true), None, "the root's own is a look");
        assert_eq!(l.foreground(false), Some(End::Foreground));
        assert_eq!(l.look(&look(true, false, None)), Step::Ended(End::Foreground));
        assert_eq!(l.take_lines(), vec!["ended: another window came to the front"]);
    }

    #[test]
    fn the_control_destroyed_or_hidden_ends_it() {
        let mut l = Landing::new(0, 0);
        let mut at = look(true, false, None);
        at.control_there = false;
        assert_eq!(l.look(&at), Step::Ended(End::Gone));
    }

    #[test]
    fn a_window_already_in_front_is_looked_at_on_the_calls_own_turn() {
        assert_eq!(plan(77, 1, true, None), Plan { land_on: Some(77), look_now: true });
        assert_eq!(plan(77, 1, false, None), Plan { land_on: Some(77), look_now: false }, "its foreground event looks");
    }

    #[test]
    fn a_top_level_id_has_no_landing_unless_a_page_in_front_came_from_it() {
        assert_eq!(plan(1, 1, true, None), Plan { land_on: None, look_now: false });
        assert_eq!(plan(1, 1, false, Some((2, 77))), Plan { land_on: None, look_now: false }, "a page from another window");
        assert_eq!(plan(1, 1, false, Some((1, 0))), Plan { land_on: None, look_now: false }, "a page that knows no control");
        assert_eq!(plan(1, 1, false, Some((1, 77))), Plan { land_on: Some(77), look_now: false });
    }

    #[test]
    fn a_newer_focus_replaces_the_pending_landing() {
        let mut l = Landing::new(0, 0);
        assert_eq!(l.look(&look(true, false, None)), Step::Put);
        l.newer();
        assert_eq!(l.look(&look(true, false, None)), Step::Ended(End::Newer));
        l.put_answered(Ok(true));
        assert_eq!(l.take_lines(), vec!["ended: a newer focus"], "a put answered after the end says nothing");
    }

    /// The put is asked on the host's accessibility thread, never on the event loop: the Windows
    /// backend only queues it, the host hands it to the thread, and UI Automation's SetFocus is
    /// made there, read back there.
    #[test]
    fn the_put_is_asked_on_the_accessibility_thread_never_on_the_pump() {
        const WINDOWS: &str = include_str!("backend/windows.rs");
        const UIA: &str = include_str!("backend/uia.rs");
        const LIB: &str = include_str!("lib.rs");
        assert!(!WINDOWS.contains("SetFocus("), "the pump makes no SetFocus of its own");
        assert!(WINDOWS.contains("FOCUS_PUTS.with(|p| p.borrow_mut().push((l.id, l.control)));"), "it queues the put");
        let fire = &LIB[LIB.find("fn fire_focus_puts(&self)").unwrap()..];
        let fire = &fire[..fire.find("\n    }\n").unwrap()];
        assert!(fire.contains("self.elements.ask_host(runner, backend::ElementAsk::FocusPut { hwnd }"), "the host asks it on the thread");
        assert!(UIA.contains("ElementAsk::FocusPut { hwnd } => ElementAnswer::Put(element_focus_put(*hwnd))"));
        let put = &UIA[UIA.find("fn element_focus_put(").unwrap()..];
        let put = &put[..put.find("\n}\n").unwrap()];
        assert!(put.contains("el.SetFocus()") && put.contains("GetGUIThreadInfo"), "made and read back there");
    }

    /// Our own code never attaches input to another thread: UI Automation's window proxy may, on the
    /// accessibility thread, for a window with no provider of its own.
    #[test]
    fn our_code_never_calls_attach_thread_input() {
        for (name, src) in [
            ("backend/windows.rs", include_str!("backend/windows.rs")),
            ("webview_win.rs", include_str!("webview_win.rs")),
            ("backend/uia.rs", include_str!("backend/uia.rs")),
        ] {
            assert!(!src.contains("AttachThreadInput("), "{name} calls AttachThreadInput");
        }
    }

    #[test]
    fn a_page_gives_back_the_control_it_took_the_front_from() {
        assert_eq!(came_from_focus(None, 1, 77), 77, "taken with came_from");
        assert_eq!(came_from_focus(Some((1, 77)), 1, 0), 77, "the keyboard nowhere: the last one for the same window");
        assert_eq!(came_from_focus(Some((1, 77)), 2, 0), 0, "another window: none");
        assert_eq!(came_from_focus(Some((1, 77)), 1, 78), 78, "a new one replaces it");
        // A page shown in place of another hands that one's on: the front goes where the other page
        // came from, and the control with it (webview::front_goes_to finds the window).
        let pages = [(10isize, false, 1isize)];
        let to = crate::webview::front_goes_to(11, 10, pages.len(), |w| pages.iter().find(|p| p.0 == w).map(|p| (p.1, p.2)));
        assert_eq!(to, 1);
    }
}
