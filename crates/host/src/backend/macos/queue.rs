//! What the OS callbacks leave behind, and the one place it is handed to the host.
//!
//! Nothing is dispatched from a callback. The event tap, the Carbon hotkey handler and the
//! accessibility observers all push here and return immediately — a tap that takes too long
//! inside its callback is **switched off by the system** and does not come back on its own,
//! which would silently disable every captured key the platform has. The wxWidgets timer
//! calls [`drain`] every 15 ms and that is where the host hears about any of it.
//!
//! The event tap pushes from a thread of its own (`tap.rs`), into [`KEYS`], a queue behind one
//! lock that the drain empties with one swap (`tap_queue::Handover`); everything else here is
//! pushed and drained on the main thread, in thread-locals.
//!
//! The order things come out in is part of the contract, not an accident. See [`drain`].

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use super::tap_queue::{Handover, Waits};
use crate::backend::{HostEvents, Taken};

/// Captured key-downs the tap took, each with its module — see [`Taken`]. Pushed on the tap's
/// thread, each stamped with the moment it was queued under the queue's lock, and taken by the
/// drain with the moment it took them under the same lock ([`taken_at`]).
static KEYS: Handover<Taken> = Handover::new();

/// A hotkey press, as Carbon hands it over: its id, the counts of key-downs the tap had let
/// through with its key-down and when that was — the tap's stamp (`tap::claim_shortcut`), or for a
/// press the tap did not see, the counts and the moment as Carbon hands it to this thread — and
/// whether the moment is the tap's.
#[derive(Clone, Copy)]
struct Hotkey {
    id: i32,
    seq: u32,
    taps_seq: u32,
    at: Instant,
    stamped: bool,
}

thread_local! {
    /// Hotkey presses, in the order Carbon handed them over ([`Hotkey`]).
    static HOTKEYS: RefCell<Vec<Hotkey>> = const { RefCell::new(Vec::new()) };
    /// Foreground changes, as the window id at the time. Resolved to a `WinInfo` in
    /// `drain` rather than in the callback: reading a window's geometry and its owner's
    /// name is an accessibility round-trip into another process, and that is not something
    /// to do inside a notification handler.
    static ACTIVATED: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    static FOCUS_DIRTY: Cell<bool> = const { Cell::new(false) };
    /// Deadlines for the re-check ladder — see `drain`.
    static RECHECKS: RefCell<Vec<Instant>> = RefCell::new(Vec::new());
    /// The one focus round armed for the end of an application's not-answering quarantine — see
    /// [`arm_after_busy`].
    static AFTER_BUSY: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The end of the stall the host's guard last ended in a stop (`drop_pending`): a hotkey
    /// press the tap stamped at or before it was made during that stall, and is dropped when
    /// Carbon hands it over after it ([`drain`]).
    static DROPPED_AT: Cell<Option<Instant>> = const { Cell::new(None) };
    /// When the latest captured key handed on so far was queued, for telling a hotkey press that
    /// Carbon handed over after keys pressed after it had gone on ([`drain`]).
    static LAST_KEY_QUEUED: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The hotkey presses handed on after captured keys pressed after them since the last line, and
    /// when that line was ([`say_overtaken`]).
    static OVERTAKEN: Cell<(u32, Option<Instant>)> = const { Cell::new((0, None)) };
    /// The captured keys that waited long for the event loop, for the drain's line.
    static WAITS: RefCell<Waits> = RefCell::new(Waits::default());
}

/// A hotkey press, as Carbon hands it over on this thread, with the key it is registered on
/// (`hotkey.rs`): its counts of key-downs let through and its moment are the tap's stamp of its own
/// key-down (`tap::claim_shortcut`, `tap_queue::Shortcuts`). The tap, at the HID level, sees the
/// hotkey's key-down before Carbon matches it, and stamps it with that key in the counts and no key
/// typed after it — the Mac session confirmed the first (TODO.md, "The pass-on never ran on a
/// Mac"), and the tap's own thread keeps the second true however long this thread was busy before
/// Carbon's event reached it. A press the tap did not see — it was off, or not installed — is
/// counted and timed here, as Carbon hands it over.
pub fn push_hotkey(id: i32, key: Option<(u16, u8)>) {
    let press = match key.and_then(|(code, mask)| super::tap::claim_shortcut(code, mask)) {
        Some(s) => Hotkey { id, seq: s.seq, taps_seq: s.taps_seq, at: s.at, stamped: true },
        None => Hotkey {
            id,
            seq: super::tap::let_through(),
            taps_seq: super::tap::taps_through(),
            at: Instant::now(),
            stamped: false,
        },
    };
    HOTKEYS.with(|q| q.borrow_mut().push(press));
}

pub fn push_activated(window: isize) {
    ACTIVATED.with(|q| q.borrow_mut().push(window));
    // A foreground change is also a focus change. Windows learned this the hard way: the
    // activate can be dropped for a window that has no title yet, and the focus path asks
    // `active_window()`, which does not require one.
    mark_focus_dirty();
}

/// An application found in front without an announced activation, whose window the host is
/// about to report itself — a wake, an unlock or the session back (`watch::recheck_front`, from
/// `system::drain`) — so it is not queued: it would be reported twice in this drain. The host
/// reports only a window with a title, as the activation's drain does; for one without, the
/// re-check ladder is armed here as that drain would have armed it.
pub fn taken_over(window: isize) {
    if super::ax::window_info(window, true).is_none() {
        arm_recheck_ladder();
    }
}

/// Queues the key `make` builds from the moment it is queued: the tap's thread, from its callback.
pub fn push_key(make: impl FnOnce(Instant) -> Taken) {
    KEYS.push(make);
}

/// What a stop of the host's guard drops: the captured keys and hotkey presses queued and not
/// drained yet (`Backend::drop_queued_input`) — among them the keys the tap took during the stall,
/// which its own thread queued while this one was held. A hotkey press made during the stall that
/// Carbon hands to this thread only after it is dropped as it is drained, by the moment the tap
/// stamped its key-down ([`DROPPED_AT`]).
pub fn drop_pending() -> crate::backend::DroppedInput {
    let keys = KEYS.drop_all().into_iter().map(|k| (k.vk, k.mask, k.owner)).collect();
    let hotkeys = HOTKEYS.with(|q| std::mem::take(&mut *q.borrow_mut())).into_iter().map(|h| h.id).collect();
    DROPPED_AT.with(|d| d.set(Some(Instant::now())));
    crate::backend::DroppedInput { keys, hotkeys }
}

/// When the last [`drain`] took the presses (`Backend::input_taken_at`): the tap queues a key with
/// its moment under the lock the drain takes the keys and this moment under, so a key queued before
/// it is in that drain; Carbon queues on this thread, before the drain.
pub fn taken_at() -> Option<Instant> {
    KEYS.taken_at()
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

/// Arms one focus round for `until`, the end of an application's not-answering quarantine: a read
/// of where the keyboard is was skipped for it, or failed into it (`ax::focus_after_busy`).
///
/// While the quarantine lasts every read of the focus chain answers empty, and the overlay runtime
/// keeps a plug-in's stay through an empty chain in the same window (docs/api/overlay.md, "A
/// verdict is kept for a stay") — so a visit to the DAW's own controls made meanwhile went unseen
/// until something else brought a round, the overlay holding its keys over them. This bounds that:
/// one round as the quarantine ends, whose read is put to the application again. One, the latest
/// replacing it: the latest read was of the application in front. An application that goes on not
/// answering fails that read too, which puts it in the quarantine again and arms the next round —
/// one read every five or six seconds, the quarantine's own rate, and only while something asks
/// where the keyboard is.
///
/// Not a rung of the ladder above, which is armed only while nothing is pending: a ladder of an
/// untitled window would keep this out, and this one would keep the ladder out.
pub fn arm_after_busy(until: Instant) {
    AFTER_BUSY.with(|a| a.set(Some(until)));
}

/// Hands everything queued since the last call to the host, in the order the host needs:
/// system events — a wake, a lock, a display change (`system.rs`) — first (step 0), then
/// hotkeys (1), window activations (2), captured keys (3), game-controller events (4), **one**
/// focus change if anything dirtied it (5), and any re-check that has come due (6) — a rung of the
/// ladder, or the round at the end of a quarantine ([`arm_after_busy`]), one round for them all.
///
/// System events first: a wake or a display change turns the host's epochs over, and what
/// follows in the same drain must not be answered from a cache taken before it. For a system
/// event that may have changed what is in front the host runs its own focus round as it hears
/// it (`system_events::Plan::recheck`). Before that the backend reads the application in front
/// again: one it finds there without an announced activation is queued as one after a display
/// change or the displays waking, and its focus change is the one in step 5; after a wake, an
/// unlock or the session coming back the host reports the window in front itself, so it is only
/// taken over ([`taken_over`]).
///
/// Activations must precede the keys that arrived in that window: an activation invalidates
/// every cached coordinate in the host, a key does not, and a key handled against stale
/// coordinates clicks where the plugin used to be. Pad events follow the keys for the same
/// reason, and they come from the gamepad hub rather than from a thread-local here: they are
/// pushed from GameController's own dispatch queue, which is not this thread. The Windows
/// pump drains in the same order. The focus dispatch is coalesced to one
/// per drain because it fans out to every module VM and every overlay each of them owns —
/// it is the most expensive thing the host does, and delivering it twice does nothing twice.
pub fn drain(events: &mut dyn HostEvents) {
    super::system::drain(events);

    // The press's own key-down is in the counts (`push_hotkey`): the count of quick repeats takes it
    // for no other key going down. A press made during a stall that ended in a stop is dropped, by
    // the tap's stamp, as the stall's keys were (`drop_pending`); a press without one cannot be
    // placed, and goes on.
    let dropped_at = DROPPED_AT.with(Cell::get);
    let handed_before = LAST_KEY_QUEUED.with(Cell::get);
    let mut late = Vec::new();
    for h in HOTKEYS.with(|q| std::mem::take(&mut *q.borrow_mut())) {
        if h.stamped && dropped_at.is_some_and(|d| h.at <= d) {
            late.push(h.id);
            continue;
        }
        // Keys pressed after it, handed on in an earlier drain: Carbon's event reached this thread
        // only after them. Counted for a line; the order is not put right, which would need the
        // keys held back for an event that may never come (`tap_queue::Shortcuts`).
        if h.stamped && handed_before.is_some_and(|k| h.at < k) {
            OVERTAKEN.with(|o| o.set((o.get().0 + 1, o.get().1)));
        }
        events.on_hotkey(h.id, crate::backend::HotkeyPress { seq: h.seq, taps_seq: h.taps_seq, own: 1, at: h.at });
    }
    if !late.is_empty() {
        events.on_stall_hotkeys_dropped(late);
    }
    say_overtaken();

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

    // The captured keys, and the moment they were taken, under one lock (`Handover::take`); what
    // the tap's thread queues after it is the next drain's.
    let (keys, _taken) = KEYS.take();
    for k in keys {
        if let Some(queued) = k.pressed.queued {
            WAITS.with(|w| w.borrow_mut().note(queued.elapsed(), k.vk));
            LAST_KEY_QUEUED.with(|l| l.set(Some(l.get().map_or(queued, |at| at.max(queued)))));
        }
        events.on_key(k.vk, k.mask, k.owner, k.repeat, k.pressed);
    }
    if let Some(line) = WAITS.with(|w| w.borrow_mut().line(Instant::now())) {
        crate::logging::line("macos", &line);
    }

    crate::backend::gamepad::drain_into(events);

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
    // And the round armed for the end of a quarantine, in the same one: from its instant on the
    // application is asked again (`ax::is_busy`).
    let after_busy = AFTER_BUSY.with(|a| match a.get() {
        Some(at) if at <= now => {
            a.set(None);
            true
        }
        _ => false,
    });
    if due > 0 || after_busy {
        events.on_focus_change();
    }
}

/// Says the hotkey presses handed on after captured keys pressed after them ([`OVERTAKEN`]), at
/// most once a second.
fn say_overtaken() {
    let (n, said) = OVERTAKEN.with(Cell::get);
    let now = Instant::now();
    if n == 0 || said.is_some_and(|s| now.saturating_duration_since(s) < Duration::from_secs(1)) {
        return;
    }
    OVERTAKEN.with(|o| o.set((0, Some(now))));
    crate::logging::line(
        "macos",
        &format!(
            "{n} shortcut press(es) reached their module after captured keys pressed after them: \
             Carbon hands shortcuts to the main thread, which was busy, and the event tap had already \
             handed those keys on"
        ),
    );
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
