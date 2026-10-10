//! The stretches the event loop spends inside a modal loop of this application's own — a modal
//! dialog, a menu of ours — counted for the `[gui]` lines that say when one opened and closed, how
//! many pump iterations ran inside it, and the longest of them.
//!
//! **Why.** The pump runs from a timer of the event loop's, so it goes on inside a dialog's or a
//! menu's own message loop: while a module's callback runs there, the dialog or the menu answers
//! nothing — not the user, and not a program asking it what it shows, as a screen reader does.
//! The lines say how much ran there, so a log can tell a menu that stood still from one that did
//! not.
//!
//! **Where the spans come from.** A dialog's is opened and closed around its `show_modal` on the
//! event loop (`gui`). A menu's comes from the keyboard watch on Windows, which hears the
//! system's menu events for this process on its own thread (`backend::hook_watch_thread`);
//! there is none on macOS.
//!
//! **What it costs the pump.** One relaxed load per iteration while no span is open, which is
//! nearly always; a lock, and an add and a comparison per open span, while one is — that is,
//! while the event loop is inside a dialog or a menu.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

/// What ran inside one span.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Inside {
    /// Pump iterations that ran while it was open.
    pub(crate) iterations: u32,
    /// The longest of them, in milliseconds; 0 when none ran.
    pub(crate) longest_ms: u64,
}

/// The spans open now, each with what ran inside it so far. Spans can nest — a menu inside a
/// dialog, a dialog opened from another — and an iteration counts for every one open.
#[derive(Debug, Default)]
pub(crate) struct Spans {
    open: Vec<(u64, Inside)>,
    next: u64,
}

impl Spans {
    const fn new() -> Self {
        Spans { open: Vec::new(), next: 0 }
    }

    /// Opens a span; its id closes it.
    pub(crate) fn open(&mut self) -> u64 {
        self.next += 1;
        self.open.push((self.next, Inside::default()));
        self.next
    }

    /// One pump iteration of `ms` milliseconds ran: it counts for every span open.
    pub(crate) fn iteration(&mut self, ms: u64) {
        for (_, inside) in &mut self.open {
            inside.iterations = inside.iterations.saturating_add(1);
            inside.longest_ms = inside.longest_ms.max(ms);
        }
    }

    /// Closes span `id`, whichever order the spans close in, with what ran inside it; `None` for an
    /// id not open (closed already).
    pub(crate) fn close(&mut self, id: u64) -> Option<Inside> {
        let at = self.open.iter().position(|(open, _)| *open == id)?;
        Some(self.open.remove(at).1)
    }

    fn len(&self) -> usize {
        self.open.len()
    }
}

static SPANS: Mutex<Spans> = Mutex::new(Spans::new());
/// How many spans are open, for the pump's one load per iteration ([`iteration`]).
static OPEN: AtomicUsize = AtomicUsize::new(0);

fn spans() -> MutexGuard<'static, Spans> {
    SPANS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Opens a span (any thread); the id is for [`close`].
pub(crate) fn open() -> u64 {
    let mut s = spans();
    let id = s.open();
    OPEN.store(s.len(), Ordering::Relaxed);
    id
}

/// Closes span `id` (any thread), with what ran inside it; nothing ran for an id not open.
pub(crate) fn close(id: u64) -> Inside {
    let mut s = spans();
    let inside = s.close(id).unwrap_or_default();
    OPEN.store(s.len(), Ordering::Relaxed);
    inside
}

/// The pump ran one iteration of `ms` milliseconds (the event loop's thread). Nothing but one
/// relaxed load while no span is open.
pub(crate) fn iteration(ms: u64) {
    if OPEN.load(Ordering::Relaxed) == 0 {
        return;
    }
    spans().iteration(ms);
}

/// A menu's popups, by window, from the system's menu events for this process: a submenu opens
/// inside the menu, and the menu is closed when the last of them is. An end for a popup not open
/// is ignored.
///
/// A popup whose end the system never reports would keep the menu open for good — no menu line
/// again, and a lock in every pump iteration ([`iteration`]) for the rest of the process. So a
/// popup that is no longer shown (`shown`, asked of the window) is dropped when the next popup
/// opens and when a window comes to the front ([`MenuPopups::settle`]); the menu it leaves with
/// none is closed then.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MenuPopups(Vec<isize>);

impl MenuPopups {
    /// Popup `hwnd` opened (`EVENT_SYSTEM_MENUPOPUPSTART`). The popups open before it that are no
    /// longer shown, and an earlier one with the same window, are dropped first. Answers whether
    /// that closed a menu whose end went unheard, and whether this popup opened the menu rather
    /// than a submenu of one open.
    pub(crate) fn popup_start(&mut self, hwnd: isize, shown: impl Fn(isize) -> bool) -> (bool, bool) {
        let was_open = !self.0.is_empty();
        self.0.retain(|&p| p != hwnd && shown(p));
        let unheard = was_open && self.0.is_empty();
        self.0.push(hwnd);
        (unheard, self.0.len() == 1)
    }

    /// Popup `hwnd` closed (`EVENT_SYSTEM_MENUPOPUPEND`): whether that closed the menu.
    pub(crate) fn popup_end(&mut self, hwnd: isize) -> bool {
        let Some(at) = self.0.iter().position(|&p| p == hwnd) else {
            return false;
        };
        self.0.remove(at);
        self.0.is_empty()
    }

    /// A window came to the front: the popups no longer shown are dropped. Whether that closed
    /// the menu.
    pub(crate) fn settle(&mut self, shown: impl Fn(isize) -> bool) -> bool {
        if self.0.is_empty() {
            return false;
        }
        self.0.retain(|&p| shown(p));
        self.0.is_empty()
    }
}

/// Milliseconds as seconds with one decimal: `12.3 s`.
pub(crate) fn seconds(ms: u64) -> String {
    format!("{}.{} s", ms / 1_000, ms % 1_000 / 100)
}

/// The line for a span that closed: `what` (`dialog 'Module settings'`, `the tray menu`), open
/// for `open_ms`, and what ran inside it.
pub(crate) fn closed_line(what: &str, open_ms: u64, inside: Inside) -> String {
    let longest = if inside.iterations == 0 {
        String::new()
    } else {
        format!(", the longest {} ms", inside.longest_ms)
    };
    format!(
        "{what} closed after {}; {} pump iteration(s) ran inside it{longest}",
        seconds(open_ms),
        inside.iterations
    )
}

/// The line for a menu of this process that opened: the tray's when the window in front is a
/// hidden one of ours (`tray`), which the tray icon puts in front for its menu; else whichever
/// it is, with the window in front (`front`).
pub(crate) fn menu_open_line(tray: bool, front: &str) -> String {
    if tray {
        "the tray menu is open (our hidden tray window took the foreground)".to_string()
    } else {
        format!("a menu of ours is open ({front} in front)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each span counts the iterations that ran while it was open, nested ones included, and the
    /// longest of them; closing one leaves the others counting.
    #[test]
    fn a_span_counts_the_pump_iterations_inside_it_and_the_longest() {
        let mut s = Spans::default();
        s.iteration(900);
        let dialog = s.open();
        s.iteration(12);
        let menu = s.open();
        s.iteration(340);
        s.iteration(20);
        assert_eq!(s.close(menu), Some(Inside { iterations: 2, longest_ms: 340 }));
        s.iteration(30);
        assert_eq!(s.close(dialog), Some(Inside { iterations: 4, longest_ms: 340 }));
        assert_eq!(s.close(dialog), None, "closed already");
        let empty = s.open();
        assert_eq!(s.close(empty), Some(Inside::default()));
        assert_eq!(s.len(), 0);
    }

    /// Spans may close in any order.
    #[test]
    fn spans_close_in_any_order() {
        let mut s = Spans::default();
        let a = s.open();
        let b = s.open();
        s.iteration(5);
        assert_eq!(s.close(a), Some(Inside { iterations: 1, longest_ms: 5 }));
        s.iteration(7);
        assert_eq!(s.close(b), Some(Inside { iterations: 2, longest_ms: 7 }));
    }

    /// The menu is open from its first popup to the end of the last; an end with nothing open, a
    /// submenu's, or one of a popup never opened, closes nothing.
    #[test]
    fn a_menu_is_open_from_its_first_popup_to_the_end_of_its_last() {
        let shown = |_| true;
        let mut m = MenuPopups::default();
        assert!(!m.popup_end(10), "nothing open");
        assert_eq!(m.popup_start(10, shown), (false, true));
        assert_eq!(m.popup_start(11, shown), (false, false), "a submenu");
        assert!(!m.popup_end(11));
        assert!(!m.popup_end(12), "a popup never opened");
        assert!(m.popup_end(10));
        assert!(!m.popup_end(10));
        assert_eq!(m.popup_start(20, shown), (false, true), "the next menu");
    }

    /// A popup whose end went unheard does not keep the menu open: once it is no longer shown it
    /// is dropped when the next popup opens — which then opens a menu of its own — or when a
    /// window comes to the front. One still shown stays, and so does the menu.
    #[test]
    fn a_popup_whose_end_went_unheard_is_dropped_once_it_is_no_longer_shown() {
        let gone = |p: isize| p != 10 && p != 11;
        let mut m = MenuPopups::default();
        assert_eq!(m.popup_start(10, gone), (false, true));
        assert_eq!(m.popup_start(11, |_| true), (false, false));
        // Neither end came; both are gone by the next menu.
        assert_eq!(m.popup_start(20, gone), (true, true), "the old menu closed, a new one opened");
        assert!(m.popup_end(20));
        // The same window again with no end between: its end went unheard.
        assert_eq!(m.popup_start(30, |_| true), (false, true));
        assert_eq!(m.popup_start(30, |_| true), (true, true));
        // A window in front: shown, it stays; no longer shown, the menu is closed.
        assert!(!m.settle(|_| true));
        assert!(m.settle(|_| false));
        assert!(!m.popup_end(30), "closed already");
        assert!(!m.settle(|_| false), "nothing open");
    }

    /// Every modal dialog of the application opens through the one function that says so and
    /// counts what ran inside it, and the pump counts each iteration it runs.
    #[test]
    fn every_dialog_is_said_and_every_pump_iteration_counted() {
        const GUI: &str = include_str!("gui.rs");
        assert_eq!(GUI.matches(".show_modal();").count(), 1, "only inside show_modal_said");
        let said = GUI.find("fn show_modal_said(").unwrap();
        assert!(GUI[said..].find(".show_modal();").unwrap() < GUI[said..].find("\n}\n").unwrap());
        assert_eq!(GUI.matches("show_modal_said(&dialog, title)").count(), 3);
        const LIB: &str = include_str!("lib.rs");
        let measured = LIB.find("let pump_ms = pump_started.elapsed().as_millis();").unwrap();
        let counted = LIB.find("modal_spans::iteration(pump_ms as u64);").unwrap();
        assert!(measured < counted && counted - measured < 300, "right after the iteration is measured");
    }

    #[test]
    fn the_lines_say_how_long_and_what_ran_inside() {
        assert_eq!(
            closed_line("dialog 'Settings'", 12_345, Inside { iterations: 41, longest_ms: 120 }),
            "dialog 'Settings' closed after 12.3 s; 41 pump iteration(s) ran inside it, the longest 120 ms"
        );
        assert_eq!(
            closed_line("the tray menu", 800, Inside::default()),
            "the tray menu closed after 0.8 s; 0 pump iteration(s) ran inside it"
        );
        assert_eq!(menu_open_line(true, "untitled, class 'x'"), "the tray menu is open (our hidden tray window took the foreground)");
        assert_eq!(menu_open_line(false, "'Modules'"), "a menu of ours is open ('Modules' in front)");
    }
}
