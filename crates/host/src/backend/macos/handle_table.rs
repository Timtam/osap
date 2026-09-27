//! The bookkeeping behind `handles.rs`: which element has which handle, which window each
//! handle lies in, and what a sweep drops. Generic over the element, so it has no
//! accessibility type in it and its rules run under `cargo test` on Windows (`backend/mod.rs`
//! borrows this file for its tests); `handles.rs` is the instance with `AXUIElement`s in it.
//!
//! **What a sweep drops.** The table grows by one per element first seen, and a plugin's tree
//! is hundreds of them. It used to drop only what belonged to processes that had exited, so a
//! DAW left open for days kept every element of every plugin window ever opened in it — each a
//! retained `AXUIElement`, a few megabytes an hour of plugin work, returned only when the DAW
//! quit. So each entry now knows the window it lies in (`owner`: its own handle for a window,
//! the window's for a control, 0 when that is not known), and a sweep also drops every window
//! its process says is gone, with every control inside it — asking only about the windows the
//! window server's list cannot vouch for ([`needs_asking`]). Handles are never reused — the
//! counter only goes up — so a module still holding a dropped one finds it unmatched, which is
//! what it should find: the contract (`handles.rs`) is unchanged.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// One interned thing.
#[derive(Clone, Debug)]
pub struct Entry<E> {
    pub element: E,
    pub pid: i32,
    /// The `CGWindowID` behind a window, when it has been paired; 0 otherwise.
    pub window_id: u32,
    /// The handle of the window this lies in: its own for a window, 0 when not known.
    pub owner: isize,
}

/// What an element is interned as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A window: it is its own owner, and a sweep asks its process whether it still exists.
    Window,
    /// Something inside the window with this handle (0: not known). Dropped with that window.
    In(isize),
}

/// Whether a sweep has to ask a window's application whether the window still exists. Not for
/// a window paired with a `CGWindowID` that the window server still lists (`listed`: every
/// window number it has, on screen or not, when that could be read) — the window list is one
/// system-wide call, while asking is a message to another process. A window that is not paired,
/// or whose number the list no longer carries, is asked: only its application's answer drops it.
pub fn needs_asking(window_id: u32, listed: Option<&HashSet<u32>>) -> bool {
    !(window_id != 0 && listed.is_some_and(|l| l.contains(&window_id)))
}

pub struct Table<E> {
    next: isize,
    by_element: HashMap<E, isize>,
    by_id: HashMap<isize, Entry<E>>,
    /// The size at which the next sweep happens. A watermark rather than a constant: see
    /// [`Table::sweep`].
    next_sweep: usize,
    first_sweep: usize,
}

impl<E: Clone + Eq + Hash> Table<E> {
    pub fn new(first_sweep: usize) -> Self {
        Table { next: 0, by_element: HashMap::new(), by_id: HashMap::new(), next_sweep: first_sweep, first_sweep }
    }

    /// The handle for `element` — the same one as last time, if it has been seen before. Never
    /// 0. What is learned late is filled in: a window id, that an element first seen inside a
    /// window is itself a window, or the window a control lies in.
    pub fn intern(&mut self, element: E, pid: i32, window_id: u32, kind: Kind) -> isize {
        if let Some(&id) = self.by_element.get(&element) {
            if let Some(e) = self.by_id.get_mut(&id) {
                if window_id != 0 {
                    e.window_id = window_id;
                }
                match kind {
                    Kind::Window => e.owner = id,
                    Kind::In(w) if w != 0 && e.owner == 0 => e.owner = w,
                    Kind::In(_) => {}
                }
            }
            return id;
        }
        self.next += 1;
        let id = self.next;
        let owner = match kind {
            Kind::Window => id,
            Kind::In(w) => w,
        };
        self.by_element.insert(element.clone(), id);
        self.by_id.insert(id, Entry { element, pid, window_id, owner });
        id
    }

    pub fn get(&self, id: isize) -> Option<&Entry<E>> {
        self.by_id.get(&id)
    }

    /// The `CGWindowID` paired with handle `id` — and nothing else: what the handle is, and the
    /// window it lies in, stay as they were.
    pub fn set_window_id(&mut self, id: isize, window_id: u32) {
        if let Some(e) = self.by_id.get_mut(&id) {
            e.window_id = window_id;
        }
    }

    /// The controls `ids` lie in `window` — for a chain whose window is found only at its end.
    /// Fills in only what is not known: a window stays its own owner.
    pub fn adopt(&mut self, ids: &[isize], window: isize) {
        if window == 0 {
            return;
        }
        for id in ids {
            if let Some(e) = self.by_id.get_mut(id) {
                if e.owner == 0 {
                    e.owner = window;
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether the next intern should sweep first.
    pub fn sweep_due(&self) -> bool {
        self.by_id.len() >= self.next_sweep
    }

    /// Every process the table holds something of.
    pub fn pids(&self) -> Vec<i32> {
        let set: HashSet<i32> = self.by_id.values().map(|e| e.pid).collect();
        set.into_iter().collect()
    }

    /// Every window the table holds — handle, process, element, `CGWindowID` (0: not paired) —
    /// in handle order, so the ones asked about first when a sweep is cut short are the oldest.
    pub fn windows(&self) -> Vec<(isize, i32, E, u32)> {
        let mut out: Vec<(isize, i32, E, u32)> = self
            .by_id
            .iter()
            .filter(|(id, e)| e.owner == **id)
            .map(|(id, e)| (*id, e.pid, e.element.clone(), e.window_id))
            .collect();
        out.sort_by_key(|(id, ..)| *id);
        out
    }

    /// Drops everything of a process `dead` says has exited, every window in `gone`, and
    /// everything inside a dropped window; returns the dropped handles.
    ///
    /// Then sets where the next sweep happens. Without that the threshold is a trap: once the
    /// table reaches it with nothing to drop — one DAW, one large plugin, and it will — the
    /// condition stays true for ever and every newly seen element pays for a full two-map
    /// rebuild. Doubling means a sweep that frees nothing buys twice as long before the next
    /// attempt, and one that frees most of the table returns the threshold to where it was.
    pub fn sweep(&mut self, dead: impl Fn(i32) -> bool, gone: &HashSet<isize>) -> Vec<isize> {
        let dropped_windows: HashSet<isize> = self
            .by_id
            .iter()
            .filter(|(id, e)| e.owner == **id && (gone.contains(id) || dead(e.pid)))
            .map(|(id, _)| *id)
            .collect();
        let mut dropped = Vec::new();
        self.by_id.retain(|id, e| {
            let keep = !dead(e.pid) && !dropped_windows.contains(id) && !dropped_windows.contains(&e.owner);
            if !keep {
                dropped.push(*id);
            }
            keep
        });
        let live: HashSet<isize> = self.by_id.keys().copied().collect();
        self.by_element.retain(|_, id| live.contains(id));
        self.next_sweep = self.first_sweep.max(self.by_id.len() * 2);
        dropped.sort_unstable();
        dropped
    }

    /// Where the next sweep happens, for the log.
    pub fn next_sweep(&self) -> usize {
        self.next_sweep
    }

    /// Drops everything of `pid`, which has quit; returns the dropped handles. The sweep
    /// threshold is left alone.
    pub fn forget_pid(&mut self, pid: i32) -> Vec<isize> {
        let mut dropped = Vec::new();
        self.by_id.retain(|id, e| {
            let keep = e.pid != pid;
            if !keep {
                dropped.push(*id);
            }
            keep
        });
        if !dropped.is_empty() {
            let live: HashSet<isize> = self.by_id.keys().copied().collect();
            self.by_element.retain(|_, id| live.contains(id));
        }
        dropped.sort_unstable();
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An element: a name, as the tests' stand-in for an `AXUIElement`.
    type T = Table<&'static str>;

    #[test]
    fn the_same_element_keeps_its_handle_and_learns_late() {
        let mut t = T::new(100);
        let w = t.intern("win", 10, 0, Kind::Window);
        assert_ne!(w, 0, "never 0");
        let c = t.intern("ctl", 10, 0, Kind::In(0));
        assert_eq!(t.get(c).unwrap().owner, 0, "not known yet");
        assert_eq!(t.intern("ctl", 10, 0, Kind::In(w)), c, "same handle");
        assert_eq!(t.get(c).unwrap().owner, w, "learned");
        assert_eq!(t.intern("win", 10, 77, Kind::Window), w);
        assert_eq!(t.get(w).unwrap().window_id, 77);
        // A control that turns out to be a window becomes its own owner, and stays one.
        assert_eq!(t.intern("ctl", 10, 0, Kind::Window), c);
        assert_eq!(t.get(c).unwrap().owner, c);
        assert_eq!(t.intern("ctl", 10, 0, Kind::In(w)), c);
        assert_eq!(t.get(c).unwrap().owner, c, "a window stays a window");
    }

    /// Pairing a handle with a window id fills in the id and nothing else: a control whose frame
    /// happened to match a window-list entry stays in its window.
    #[test]
    fn a_window_id_is_filled_in_without_making_a_window() {
        let mut t = T::new(100);
        let w = t.intern("win", 10, 0, Kind::Window);
        let c = t.intern("ctl", 10, 0, Kind::In(w));
        t.set_window_id(c, 55);
        assert_eq!(t.get(c).unwrap().window_id, 55);
        assert_eq!(t.get(c).unwrap().owner, w, "still inside its window");
        assert_eq!(t.windows().iter().map(|(id, ..)| *id).collect::<Vec<_>>(), vec![w]);
        t.set_window_id(w, 77);
        assert_eq!(t.windows()[0].3, 77);
        t.set_window_id(999, 1); // not a handle: nothing happens
        assert_eq!(t.len(), 2);
        // Dropped with its window, as before.
        assert_eq!(t.sweep(|_| false, &HashSet::from([w])), vec![w, c]);
    }

    #[test]
    fn a_chain_is_adopted_by_the_window_at_its_end() {
        let mut t = T::new(100);
        let a = t.intern("focused", 10, 0, Kind::In(0));
        let b = t.intern("group", 10, 0, Kind::In(0));
        let w = t.intern("win", 10, 0, Kind::Window);
        t.adopt(&[a, b, w], w);
        assert_eq!(t.get(a).unwrap().owner, w);
        assert_eq!(t.get(b).unwrap().owner, w);
        assert_eq!(t.get(w).unwrap().owner, w);
        let other = t.intern("win2", 10, 0, Kind::Window);
        t.adopt(&[a], other);
        assert_eq!(t.get(a).unwrap().owner, w, "what is known is not overwritten");
    }

    #[test]
    fn a_sweep_drops_a_gone_window_with_its_controls_and_keeps_the_rest() {
        let mut t = T::new(6);
        let w1 = t.intern("w1", 10, 0, Kind::Window);
        let c1 = t.intern("c1", 10, 0, Kind::In(w1));
        let c2 = t.intern("c2", 10, 0, Kind::In(w1));
        let w2 = t.intern("w2", 10, 0, Kind::Window);
        let c3 = t.intern("c3", 10, 0, Kind::In(w2));
        let loose = t.intern("loose", 10, 0, Kind::In(0));
        assert!(t.sweep_due());
        assert_eq!(t.windows().iter().map(|(id, ..)| *id).collect::<Vec<_>>(), vec![w1, w2]);
        let dropped = t.sweep(|_| false, &HashSet::from([w1]));
        assert_eq!(dropped, vec![w1, c1, c2]);
        assert!(t.get(w2).is_some() && t.get(c3).is_some() && t.get(loose).is_some());
        assert_eq!(t.len(), 3);
        // Handles are never reused: the same element comes back under a new one.
        let again = t.intern("w1", 10, 0, Kind::Window);
        assert!(again > loose, "{again} after {loose}");
        assert_eq!(t.next_sweep(), 6, "the first threshold again: most of the table went");
    }

    #[test]
    fn a_sweep_drops_everything_of_a_process_that_exited() {
        let mut t = T::new(4);
        let w = t.intern("w", 10, 0, Kind::Window);
        t.intern("c", 10, 0, Kind::In(w));
        let other = t.intern("x", 20, 0, Kind::Window);
        t.intern("y", 20, 0, Kind::In(0));
        let dropped = t.sweep(|pid| pid == 20, &HashSet::new());
        assert_eq!(dropped.len(), 2);
        assert!(t.get(other).is_none());
        assert!(t.get(w).is_some());
    }

    #[test]
    fn a_sweep_that_frees_nothing_doubles_the_threshold() {
        let mut t = T::new(3);
        for name in ["a", "b", "c"] {
            t.intern(name, 10, 0, Kind::In(0));
        }
        assert!(t.sweep_due());
        assert!(t.sweep(|_| false, &HashSet::new()).is_empty());
        assert_eq!(t.next_sweep(), 6);
        assert!(!t.sweep_due());
    }

    #[test]
    fn a_listed_window_is_not_asked_about() {
        let listed: HashSet<u32> = HashSet::from([10, 11]);
        assert!(!needs_asking(10, Some(&listed)), "paired and listed: kept without a message");
        assert!(needs_asking(12, Some(&listed)), "paired, no longer listed: its application decides");
        assert!(needs_asking(0, Some(&listed)), "not paired");
        assert!(needs_asking(10, None), "no list to go by");
    }

    #[test]
    fn a_process_that_quits_is_forgotten_at_once() {
        let mut t = T::new(100);
        let w = t.intern("w", 10, 0, Kind::Window);
        let c = t.intern("c", 10, 0, Kind::In(w));
        let keep = t.intern("k", 11, 0, Kind::Window);
        assert_eq!(t.forget_pid(10), vec![w, c]);
        assert!(t.get(keep).is_some());
        assert_eq!(t.pids(), vec![11]);
        assert!(t.forget_pid(10).is_empty());
        // A new process with the old pid gets new handles for its elements.
        let fresh = t.intern("w", 10, 0, Kind::Window);
        assert!(fresh > keep);
    }
}
