//! What the OS callbacks leave behind, and the one place it is handed to the host.
//!
//! Nothing is dispatched from a callback. The event tap, the Carbon hotkey handler and the
//! accessibility observers all push here and return immediately — a tap that takes too long
//! inside its callback is **switched off by the system** and does not come back on its own,
//! which would silently disable every captured key the platform has. The wxWidgets timer
//! calls [`drain`] every 15 ms and that is where the host hears about any of it.
//!
//! The order things come out in is part of the contract, not an accident. See [`drain`].

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use crate::backend::HostEvents;

thread_local! {
    static HOTKEYS: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    /// Foreground changes, as the window id at the time. Resolved to a `WinInfo` in
    /// `drain` rather than in the callback: reading a window's geometry and its owner's
    /// name is an accessibility round-trip into another process, and that is not something
    /// to do inside a notification handler.
    static ACTIVATED: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    static KEYS: RefCell<Vec<(u32, u8)>> = const { RefCell::new(Vec::new()) };
    static FOCUS_DIRTY: Cell<bool> = const { Cell::new(false) };
    /// Deadlines for the re-check ladder — see `drain`.
    static RECHECKS: RefCell<Vec<Instant>> = RefCell::new(Vec::new());
}

pub fn push_hotkey(id: i32) {
    HOTKEYS.with(|q| q.borrow_mut().push(id));
}

pub fn push_activated(window: isize) {
    ACTIVATED.with(|q| q.borrow_mut().push(window));
    // A foreground change is also a focus change. Windows learned this the hard way: the
    // activate can be dropped for a window that has no title yet, and the focus path asks
    // `active_window()`, which does not require one.
    mark_focus_dirty();
}

pub fn push_key(vk: u32, mods: u8) {
    KEYS.with(|q| q.borrow_mut().push((vk, mods)));
}

pub fn mark_focus_dirty() {
    FOCUS_DIRTY.with(|f| f.set(true));
}

/// Arms the re-check ladder that follows a foreground change.
///
/// For a window that becomes foreground before it has a title — Komplete Kontrol's
/// Preferences dialog does exactly this, and sets its title a beat later with no further
/// event. Armed only when nothing is pending, so window churn cannot pile ladders up.
///
/// The ladder has to OUTLAST THE PENALTY BOX, and that is not a detail.
///
/// An application that does not answer an accessibility question is quarantined for
/// [`super::ax::BUSY_PENALTY`]. If every rung falls inside that window then every rung is
/// skipped without asking anything, the ladder is exhausted, and nothing tries again until the
/// user switches applications by hand — which is exactly what the tester reported: sforzando
/// needed up to three Alt+Tabs before it was noticed. The rungs were 200, 500 and 1000 ms
/// against a 1.5-second quarantine, and I then raised the quarantine to five seconds to fix a
/// different fault, which made this one worse without touching its code.
///
/// So the last rungs are placed past the quarantine and DERIVED from the same constant rather
/// than copied from it. The two can no longer drift apart.
fn arm_recheck_ladder() {
    RECHECKS.with(|r| {
        let mut r = r.borrow_mut();
        if !r.is_empty() {
            return;
        }
        let now = Instant::now();
        let penalty = super::ax::BUSY_PENALTY;
        for d in [
            Duration::from_millis(200),
            Duration::from_millis(500),
            Duration::from_millis(1000),
            Duration::from_millis(2000),
            penalty + Duration::from_millis(300),
            penalty + Duration::from_millis(1500),
            penalty * 2 + Duration::from_millis(500),
        ] {
            r.push(now + d);
        }
    });
}

/// Hands everything queued since the last call to the host, in the order the host needs.
///
/// 1. hotkeys, 2. window activations, 3. captured keys, 4. **one** focus change if anything
/// dirtied it, 5. any re-check that has come due.
///
/// Activations must precede the keys that arrived in that window: an activation invalidates
/// every cached coordinate in the host, a key does not, and a key handled against stale
/// coordinates clicks where the plugin used to be. The focus dispatch is coalesced to one
/// per drain because it fans out to every module VM and every overlay each of them owns —
/// it is the most expensive thing the host does, and delivering it twice does nothing twice.
pub fn drain(events: &mut dyn HostEvents) {
    for id in HOTKEYS.with(|q| std::mem::take(&mut *q.borrow_mut())) {
        events.on_hotkey(id);
    }

    let activated = ACTIVATED.with(|q| std::mem::take(&mut *q.borrow_mut()));
    for window in activated {
        // Titled only, matching `enumerate_windows` and the Windows activate path: an
        // untitled window is not matchable yet, so it is not announced as an activation —
        // the ladder below comes back for it.
        match super::ax::window_info(window, true) {
            Some(win) => events.on_window_activate(win),
            None => arm_recheck_ladder(),
        }
    }

    for (vk, mods) in KEYS.with(|q| std::mem::take(&mut *q.borrow_mut())) {
        events.on_key(vk, mods);
    }

    if FOCUS_DIRTY.with(|f| f.replace(false)) {
        events.on_focus_change();
    }

    let now = Instant::now();
    let due = RECHECKS.with(|r| {
        let mut r = r.borrow_mut();
        let n = r.iter().filter(|d| **d <= now).count();
        r.retain(|d| *d > now);
        n
    });
    if due > 0 {
        events.on_focus_change();
    }
}

/// One turn of the run loop, for headless mode; the shipped path runs under wxWidgets,
/// which owns the loop. Blocks until something happens or the interval is up, whichever
/// comes first, and returns after handling it — the OS callbacks queue while we are inside.
///
/// Only the turn, deliberately. What follows it — the tap's health check, the observer
/// retries, the drain — is the pump, and the pump has ONE definition (`pump_pending`). This
/// used to be a whole loop of its own that called `drain` directly, so everything the pump
/// gained afterwards was missing from the headless run, which is the run CI reads.
pub fn run_once() {
    unsafe {
        objc2_core_foundation::CFRunLoop::run_in_mode(
            objc2_core_foundation::kCFRunLoopDefaultMode,
            0.015,
            false,
        );
    }
}
