//! What the event tap's thread hands the main thread, and how: the keys it took, under one lock
//! with the moment the drain takes them; the lines it may not write itself; the counts it had at
//! the key-down of a shortcut of this application's; and how long a taken key waited for the event
//! loop.
//!
//! Its own file for the reason `tap_handoff.rs` has one: nothing macOS in it, so `backend/mod.rs`
//! borrows it by `#[path]` under `cfg(test)` and its rules run, with real threads, on the machine it
//! is written on. The tap answers on a thread and run loop of its own (`tap.rs`); the drain and
//! Carbon's shortcuts are the main thread's (`queue.rs`, `hotkey.rs`). Everything the two share
//! here sits behind a `std` mutex held for a push, a look or a swap, never across a call out of the
//! process, so the tap waits on the main thread for microseconds at most, and never for its work.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A lock a panic elsewhere cannot poison for good: what each one guards is whole after any
/// single push or swap, and the tap must go on answering.
fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A queue one thread fills and another empties, with the moment it was last emptied.
///
/// **The moment and the items are one fact.** A taken key carries when it was queued, and the
/// host measures a `taps = "final"` sequence's end against the moment the drain took the keys
/// (`Backend::input_taken_at`): every key queued before that moment must be in that drain or an
/// earlier one, or a press made before the deadline follows its sequence as a second one. So
/// [`push`](Self::push) reads the clock under the lock it pushes under, and [`take`](Self::take)
/// notes its moment under the lock it empties the queue under. A channel cannot say that: a key
/// stamped before the drain's moment and sent after the drain looked would come a drain late.
pub(crate) struct Handover<T> {
    inner: Mutex<Queued<T>>,
}

struct Queued<T> {
    items: Vec<T>,
    taken: Option<Instant>,
}

impl<T> Handover<T> {
    pub(crate) const fn new() -> Self {
        Handover { inner: Mutex::new(Queued { items: Vec::new(), taken: None }) }
    }

    /// Queues what `make` builds from the moment it is queued, read under the lock.
    pub(crate) fn push(&self, make: impl FnOnce(Instant) -> T) {
        let mut q = locked(&self.inner);
        let at = Instant::now();
        q.items.push(make(at));
    }

    /// Takes everything queued, in the order it was queued, and notes the moment, both under the
    /// lock. Drained rather than taken, so that the queue keeps its capacity and the thread that
    /// fills it — the tap's, in its callback — does not allocate again.
    #[allow(clippy::drain_collect)]
    pub(crate) fn take(&self) -> (Vec<T>, Instant) {
        let mut q = locked(&self.inner);
        let now = Instant::now();
        q.taken = Some(now);
        (q.items.drain(..).collect(), now)
    }

    /// The moment the last [`take`](Self::take) noted; `None` before the first.
    pub(crate) fn taken_at(&self) -> Option<Instant> {
        locked(&self.inner).taken
    }

    /// Takes everything queued without noting a moment: what a stop drops.
    pub(crate) fn drop_all(&self) -> Vec<T> {
        std::mem::take(&mut locked(&self.inner).items)
    }
}

/// What a thread that may not write the log leaves for the one that does: at most `cap` items
/// between two looks, and how many more came. Each item is a value, not a line: the words are put
/// together by the thread that writes them.
pub(crate) struct Notes<T> {
    inner: Mutex<(Vec<T>, u32)>,
    cap: usize,
}

impl<T> Notes<T> {
    pub(crate) const fn new(cap: usize) -> Self {
        Notes { inner: Mutex::new((Vec::new(), 0)), cap }
    }

    /// Leaves `note`, or counts it when `cap` are waiting already.
    pub(crate) fn push(&self, note: T) {
        let mut n = locked(&self.inner);
        if n.0.len() < self.cap {
            n.0.push(note);
        } else {
            n.1 = n.1.saturating_add(1);
        }
    }

    /// Takes what was left, and how many did not fit.
    pub(crate) fn take(&self) -> (Vec<T>, u32) {
        let mut n = locked(&self.inner);
        let dropped = std::mem::take(&mut n.1);
        (std::mem::take(&mut n.0), dropped)
    }
}

/// A shortcut's key-down as the tap saw it: the counts of key-downs it had let through by then,
/// its own included (`tap::let_through`, `tap::taps_through`), and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub(crate) seq: u32,
    pub(crate) taps_seq: u32,
    pub(crate) at: Instant,
}

/// The keys this application's shortcuts are registered on (keycode, and the roles' mask), each
/// with the stamp of its last key-down that Carbon's delivery has not claimed yet.
///
/// **Why the tap stamps a shortcut.** Carbon hands a shortcut to the main thread. Once the tap has
/// a thread of its own, keys typed after a shortcut while the main thread is busy are counted by
/// the tap before Carbon's event reaches the main thread, and counts read there would hold them:
/// a key passed on later would miss that it was overtaken (`NotPassed::Overtaken`), and a quick
/// repeat would be counted across a key typed in between. So the tap stamps the shortcut's own
/// key-down, and Carbon's delivery claims that stamp.
///
/// **One stamp per key, the latest.** A key-down the tap saw and Carbon never delivers — a later
/// tap swallowed it, as VoiceOver's does its own chords — leaves a stamp no delivery claims; the
/// next key-down of that key replaces it, so it misleads no later press. The cost is two presses
/// of one shortcut inside one stall of the main thread: the first is handed the second's counts,
/// and the second those at its delivery — as both were before the tap had a thread of its own. A
/// delivery that finds no stamp (the tap was off, or not installed) is counted at its delivery.
pub(crate) struct Shortcuts {
    keys: Vec<(u16, u8, Option<Stamp>)>,
}

impl Shortcuts {
    pub(crate) const fn new() -> Self {
        Shortcuts { keys: Vec::new() }
    }

    /// The keys the shortcuts are registered on now. A key that stays keeps its stamp.
    pub(crate) fn set_keys(&mut self, keys: &[(u16, u8)]) {
        let old = std::mem::take(&mut self.keys);
        for &(code, mask) in keys {
            if self.keys.iter().any(|k| k.0 == code && k.1 == mask) {
                continue;
            }
            let kept = old.iter().find(|k| k.0 == code && k.1 == mask).and_then(|k| k.2);
            self.keys.push((code, mask, kept));
        }
    }

    /// Stamps a key-down of `code` with `mask` when a shortcut is registered on it; whether one is.
    pub(crate) fn stamp(&mut self, code: u16, mask: u8, stamp: Stamp) -> bool {
        match self.keys.iter_mut().find(|k| k.0 == code && k.1 == mask) {
            Some(k) => {
                k.2 = Some(stamp);
                true
            }
            None => false,
        }
    }

    /// The stamp of the last key-down of `code` with `mask`, taken, for Carbon's delivery of it.
    pub(crate) fn claim(&mut self, code: u16, mask: u8) -> Option<Stamp> {
        self.keys.iter_mut().find(|k| k.0 == code && k.1 == mask).and_then(|k| k.2.take())
    }

    /// The keycodes a shortcut is registered on, one bit each, for the tap's look before the lock.
    pub(crate) fn codes(&self) -> u128 {
        self.keys.iter().filter(|k| k.0 < 128).fold(0, |acc, k| acc | (1u128 << k.0))
    }
}

/// A taken key that waited this long for the event loop is counted for the drain's line.
pub(crate) const WAITED_LINE: Duration = Duration::from_millis(250);

/// The taken keys that waited [`WAITED_LINE`] or more between the tap and their module, for one
/// line at most once a second: what an overlay's user still feels of a busy main thread, now that
/// the tap no longer waits for it.
#[derive(Debug, Default)]
pub(crate) struct Waits {
    n: u32,
    worst: Duration,
    vk: u32,
    said: Option<Instant>,
}

impl Waits {
    /// A key `vk` handed to its module `waited` after the tap queued it.
    pub(crate) fn note(&mut self, waited: Duration, vk: u32) {
        if waited < WAITED_LINE {
            return;
        }
        self.n += 1;
        self.worst = self.worst.max(waited);
        self.vk = vk;
    }

    /// The line for what was noted, once at least a second has passed since the last one; the
    /// count starts again with it.
    pub(crate) fn line(&mut self, now: Instant) -> Option<String> {
        if self.n == 0 || self.said.is_some_and(|s| now.saturating_duration_since(s) < Duration::from_secs(1)) {
            return None;
        }
        let line = format!(
            "{} captured key(s) waited for the event loop, the worst {} ms (last vk {:#04x}) — the event \
             tap took them at once, and the main thread handed them to their module once it was free; \
             the pump line nearby says what held it",
            self.n,
            self.worst.as_millis(),
            self.vk
        );
        *self = Waits { said: Some(now), ..Waits::default() };
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    /// Several threads push while another drains: every item comes out once, none is lost or
    /// doubled, and each thread's items come out in the order it pushed them.
    #[test]
    fn pushes_from_several_threads_while_the_drain_runs_lose_and_double_nothing() {
        static Q: Handover<(usize, usize)> = Handover::new();
        const THREADS: usize = 4;
        const EACH: usize = 5000;
        let start = Arc::new(Barrier::new(THREADS + 1));
        let pushers: Vec<_> = (0..THREADS)
            .map(|t| {
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    for i in 0..EACH {
                        Q.push(|_| (t, i));
                    }
                })
            })
            .collect();
        start.wait();
        let mut got = Vec::new();
        while got.len() < THREADS * EACH {
            got.extend(Q.take().0);
            std::thread::yield_now();
        }
        for p in pushers {
            p.join().unwrap();
        }
        got.extend(Q.take().0);
        assert_eq!(got.len(), THREADS * EACH, "nothing lost or doubled");
        for t in 0..THREADS {
            let mine: Vec<usize> = got.iter().filter(|g| g.0 == t).map(|g| g.1).collect();
            assert_eq!(mine, (0..EACH).collect::<Vec<_>>(), "thread {t}'s items in its order");
        }
    }

    /// Every item stamped before a take's moment is in that take or an earlier one, however the
    /// threads interleave: the moment a `"final"` sequence's end is measured against.
    #[test]
    fn an_item_stamped_before_a_takes_moment_is_in_that_take_or_an_earlier_one() {
        static Q: Handover<Instant> = Handover::new();
        let pusher = std::thread::spawn(|| {
            for _ in 0..20_000 {
                Q.push(|at| at);
            }
        });
        let mut takes: Vec<(Vec<Instant>, Instant)> = Vec::new();
        while !pusher.is_finished() {
            takes.push(Q.take());
        }
        pusher.join().unwrap();
        takes.push(Q.take());
        for (i, (_, moment)) in takes.iter().enumerate() {
            for (later, _) in &takes[i + 1..] {
                assert!(later.iter().all(|at| at >= moment), "an item stamped before a take came after it");
            }
        }
        assert_eq!(takes.iter().map(|t| t.0.len()).sum::<usize>(), 20_000);
        assert_eq!(Q.taken_at(), Some(takes.last().unwrap().1));
    }

    /// A stop's drop takes the queue without noting a moment.
    #[test]
    fn a_drop_notes_no_moment() {
        let q: Handover<u32> = Handover::new();
        assert_eq!(q.taken_at(), None);
        q.push(|_| 1);
        q.push(|_| 2);
        assert_eq!(q.drop_all(), vec![1, 2]);
        assert_eq!(q.taken_at(), None);
        q.push(|_| 3);
        let (items, at) = q.take();
        assert_eq!((items, q.taken_at()), (vec![3], Some(at)));
        assert!(q.take().0.is_empty());
    }

    /// Notes beyond the cap are counted, not kept, and the count starts again after a look.
    #[test]
    fn notes_beyond_the_cap_are_counted() {
        let n: Notes<u8> = Notes::new(2);
        for i in 0..5 {
            n.push(i);
        }
        assert_eq!(n.take(), (vec![0, 1], 3));
        assert_eq!(n.take(), (vec![], 0));
    }

    /// The record of keys let through for a menu, filed on the tap's thread and taken on the main
    /// thread, as `tap.rs` keeps it behind one mutex: every key filed before the take is in it, in
    /// order, and a module's keys are not another's.
    #[test]
    fn a_menu_pass_filed_on_one_thread_is_taken_on_another() {
        static RECORD: Mutex<Vec<(u32, u32, u8)>> = Mutex::new(Vec::new());
        std::thread::spawn(|| {
            for vk in [0x0D, 0x1B, 0x28] {
                crate::backend::file_menu_pass(&mut locked(&RECORD), 7, vk, 0);
            }
            crate::backend::file_menu_pass(&mut locked(&RECORD), 8, 0x26, 0);
        })
        .join()
        .unwrap();
        assert_eq!(crate::backend::take_menu_passes(&mut locked(&RECORD), 7), vec![(0x0D, 0), (0x1B, 0), (0x28, 0)]);
        assert_eq!(crate::backend::take_menu_passes(&mut locked(&RECORD), 8), vec![(0x26, 0)]);
        assert!(locked(&RECORD).is_empty());
    }

    fn stamp(seq: u32, at: Instant) -> Stamp {
        Stamp { seq, taps_seq: seq, at }
    }

    /// A shortcut's key-down is stamped; Carbon's delivery claims that stamp once, and a key no
    /// shortcut is on is not stamped.
    #[test]
    fn a_shortcuts_key_down_is_stamped_and_claimed_once() {
        let t = Instant::now();
        let mut s = Shortcuts::new();
        s.set_keys(&[(0x25, 2), (0x30, 0)]);
        assert!(!s.stamp(0x25, 0, stamp(1, t)), "another mask is another key");
        assert!(!s.stamp(0x26, 2, stamp(1, t)));
        assert!(s.stamp(0x25, 2, stamp(5, t)));
        assert_eq!(s.claim(0x30, 0), None, "its own key only");
        assert_eq!(s.claim(0x25, 2), Some(stamp(5, t)));
        assert_eq!(s.claim(0x25, 2), None, "claimed once");
        assert_eq!(s.codes(), (1u128 << 0x25) | (1u128 << 0x30));
    }

    /// A key-down Carbon never delivered is replaced by the next one of that key, so it misleads no
    /// later press; and re-registering keeps the stamp of a key that stays.
    #[test]
    fn an_undelivered_stamp_is_replaced_and_a_key_that_stays_keeps_its_stamp() {
        let t = Instant::now();
        let mut s = Shortcuts::new();
        s.set_keys(&[(0x25, 2)]);
        s.stamp(0x25, 2, stamp(3, t));
        s.stamp(0x25, 2, stamp(9, t + Duration::from_millis(5)));
        s.set_keys(&[(0x30, 0), (0x25, 2), (0x25, 2)]);
        assert_eq!(s.claim(0x25, 2), Some(stamp(9, t + Duration::from_millis(5))));
        s.stamp(0x30, 0, stamp(11, t));
        s.set_keys(&[(0x25, 2)]);
        s.set_keys(&[(0x30, 0)]);
        assert_eq!(s.claim(0x30, 0), None, "a key let go of loses its stamp");
        assert_eq!(s.codes(), 1u128 << 0x30);
    }

    /// The drain's line: only waits of 250 ms or more, at most once a second, and the count starts
    /// again with each line.
    #[test]
    fn a_long_wait_is_said_at_most_once_a_second() {
        let t = Instant::now();
        let mut w = Waits::default();
        w.note(Duration::from_millis(249), 0x09);
        assert_eq!(w.line(t), None, "a short wait is not counted");
        w.note(Duration::from_millis(400), 0x09);
        w.note(Duration::from_millis(900), 0x28);
        let line = w.line(t).unwrap();
        assert!(line.starts_with("2 captured key(s) waited for the event loop, the worst 900 ms (last vk 0x28) — "), "{line}");
        w.note(Duration::from_millis(300), 0x0D);
        assert_eq!(w.line(t + Duration::from_millis(999)), None, "within the second");
        let line = w.line(t + Duration::from_secs(1)).unwrap();
        assert!(line.starts_with("1 captured key(s) waited for the event loop, the worst 300 ms (last vk 0x0d)"), "{line}");
        assert_eq!(w.line(t + Duration::from_secs(5)), None, "nothing new");
    }
}
