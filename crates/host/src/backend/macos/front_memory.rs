//! What each application last said about which of its own windows is in front.
//!
//! Its own file for the reason `keys.rs` has one: this is bookkeeping rather than
//! accessibility, so it compiles — and runs its tests — on a platform that has no
//! accessibility API at all. `backend/mod.rs` borrows it by `#[path]` under `cfg(test)`, and
//! that is the only compiler this project can run at home. Every rule below was got wrong at
//! least once under review, and a rule that can be executed is worth more than one that can
//! only be read.
//!
//! **Why the memory exists.** Every reading in `ax.rs` is a synchronous call into another
//! application. One that is mid-repaint does not answer, so it is quarantined for five
//! seconds and not asked again. On its own that turns a two-second stall into an instant
//! "there is no active window" — which is what an overlay gates its own activation on, so
//! the overlay switches itself off and on again every time a plug-in is briefly busy. The
//! application is still in front; it is simply not talking. This is what gets served meanwhile.
//!
//! **Why it is dangerous.** What a caller reads out of the answer is a rectangle, and an
//! overlay places its synthetic clicks inside that rectangle for somebody who cannot see
//! where they land. So a remembered answer is served only while it can still be true: for
//! the window the application itself last named, and not past [`STALE_LIMIT`].

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::backend::WinInfo;

/// How old a remembered window may be before it stops being served.
///
/// A guess, and said to be one. The quarantine re-arms itself every few seconds against an
/// application that has stopped answering for good, so without a ceiling here a plug-in that
/// wedged once would have its last geometry served for the rest of the session — and the
/// project's own rule is that a wrong answer is worse than "I do not know" for somebody who
/// cannot check it against the screen.
///
/// Thirty seconds because the two failures it sits between are asymmetric. Below it are the
/// ordinary ones this memory is for: a plug-in repainting, a menu closing, a dialog coming
/// up — none of which last seconds, and none of which move the window. Above it is an
/// application that has been unable to describe its own window for half a minute, which is
/// not a state a user is working in; going quiet is then the honest answer and the one that
/// tells them something is wrong. Nobody has measured where the line really falls, and the
/// log says how old every served answer was, which is what would move it.
pub(crate) const STALE_LIMIT: Duration = Duration::from_secs(30);

/// The size at which the map is first pruned of applications that have exited.
const FIRST_PRUNE: usize = 32;

/// What was learned the last time an application answered the frontmost-window question.
#[derive(Clone)]
pub(crate) struct LastFront {
    /// The interned handle of the window it named.
    pub handle: isize,
    /// The full snapshot, where somebody took one. `active_window` cannot rebuild this from
    /// the handle without asking the application a dozen further questions, which is
    /// precisely the cost being avoided.
    pub info: Option<WinInfo>,
    /// When the SNAPSHOT was taken — not when the handle was last confirmed.
    ///
    /// The distinction is the whole of one review finding. A caller that wants identity only
    /// refreshes the handle without taking a new snapshot, and stamping the time there would
    /// have made the age reported in the log the age of the last question rather than the
    /// age of the geometry being served. Since that question is asked on every tick, the
    /// reported age would have been near zero for arbitrarily old coordinates.
    pub at: Instant,
}

/// What the memory has to say about an application that has stopped answering.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Recall {
    /// Nothing is remembered, or nothing with a snapshot in it. A plug-in that was wedged the
    /// first time it was ever asked has told us nothing, and inventing a window is worse than
    /// admitting there is none.
    Nothing,
    /// The application named a window, then stopped describing it — and it is not the window
    /// remembered. Not an old answer but a wrong one: a plug-in opening a dialog names it
    /// immediately, and the dialog is the likeliest thing to stall while it comes up. Serving
    /// the main window's rectangle would put an overlay's clicks underneath it.
    Mismatch { remembered: isize },
    /// Remembered, and too old to stand behind. See [`STALE_LIMIT`].
    TooOld { age: Duration },
    /// Serve this. `age` is for the log, so a session can be explained afterwards.
    Serve { info: WinInfo, age: Duration },
}

/// The per-application memory. One per thread, held by `ax.rs`.
pub(crate) struct Memory {
    entries: HashMap<i32, LastFront>,
    /// The size at which the next prune happens. A watermark rather than a constant, exactly
    /// as `handles::sweep` uses one and for the same reason: with a bare threshold, a session
    /// that reaches it with every application still alive stays above it for ever, and every
    /// subsequent write pays a full rebuild plus a syscall per process.
    next_prune: usize,
}

impl Default for Memory {
    fn default() -> Self {
        Memory { entries: HashMap::new(), next_prune: FIRST_PRUNE }
    }
}

impl Memory {
    /// Records what an application answered.
    ///
    /// `alive` decides which processes are still running; it is passed in rather than called
    /// directly because the test build has no `libc::kill` to call.
    pub(crate) fn remember(
        &mut self,
        pid: i32,
        handle: isize,
        info: Option<WinInfo>,
        now: Instant,
        alive: impl Fn(i32) -> bool,
    ) {
        if pid <= 0 || handle == 0 {
            return;
        }
        let (info, at) = match (info, self.entries.get(&pid)) {
            // An identity-only refresh of the SAME window carries the snapshot over, and its
            // timestamp with it. A caller that only wanted the handle does not erase a
            // snapshot somebody else took — and does not make it look newer than it is.
            (None, Some(prev)) if prev.handle == handle => (prev.info.clone(), prev.at),
            // A DIFFERENT window: the previous snapshot describes something else, and
            // dropping it here is what stops `recall` answering the new window with the old
            // one's rectangle for the rest of the quarantine.
            (info, _) => (info, now),
        };
        if self.entries.len() >= self.next_prune {
            self.prune(&alive);
        }
        self.entries.insert(pid, LastFront { handle, info, at });
    }

    fn prune(&mut self, alive: &impl Fn(i32) -> bool) {
        self.entries.retain(|&pid, _| alive(pid));
        self.next_prune = FIRST_PRUNE.max(self.entries.len() * 2);
    }

    /// What can be served for this application, and why not, when it cannot.
    ///
    /// `named` is the window the application managed to name before it went quiet, where the
    /// caller got that far. `None` means it was never asked — it was already in the
    /// quarantine — so there is no identity to check against and the memory is the best that
    /// exists.
    pub(crate) fn recall(&self, pid: i32, named: Option<isize>, now: Instant) -> Recall {
        let Some(last) = self.entries.get(&pid) else {
            return Recall::Nothing;
        };
        if let Some(named) = named {
            if last.handle != named {
                return Recall::Mismatch { remembered: last.handle };
            }
        }
        let Some(info) = last.info.clone() else {
            return Recall::Nothing;
        };
        let age = now.saturating_duration_since(last.at);
        if age > STALE_LIMIT {
            return Recall::TooOld { age };
        }
        Recall::Serve { info, age }
    }

    /// Forgets an application's window, because it has told us it no longer has one, or
    /// because what is remembered has just been ruled out.
    pub(crate) fn forget(&mut self, pid: i32) {
        self.entries.remove(&pid);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(hwnd: isize, x: i32) -> WinInfo {
        WinInfo {
            hwnd,
            title: "Kontakt".into(),
            class: "AXWindow//".into(),
            pid: 7,
            exe: "Kontakt".into(),
            bundle_id: "com.native-instruments.Kontakt8".into(),
            x,
            y: 0,
            w: 800,
            h: 600,
            client_x: x,
            client_y: 0,
            client_w: 800,
            client_h: 600,
        }
    }

    fn alive_always(_: i32) -> bool {
        true
    }

    fn rect_of(r: &Recall) -> i32 {
        match r {
            Recall::Serve { info, .. } => info.x,
            other => panic!("expected a served window, got {other:?}"),
        }
    }

    /// The ordinary case the memory exists for: the application answered once, then stopped.
    #[test]
    fn serves_the_window_it_last_reported() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 100, Some(win(100, 10)), t, alive_always);
        assert_eq!(rect_of(&m.recall(7, None, t + Duration::from_secs(1))), 10);
    }

    /// An identity-only write keeps the snapshot AND its age. Getting this wrong made the log
    /// report the age of the last question instead of the age of the geometry.
    #[test]
    fn an_identity_refresh_does_not_make_the_snapshot_look_newer() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 100, Some(win(100, 10)), t, alive_always);
        // Ten seconds later somebody asks only for the handle. Same window.
        m.remember(7, 100, None, t + Duration::from_secs(10), alive_always);
        match m.recall(7, None, t + Duration::from_secs(10)) {
            Recall::Serve { age, info } => {
                assert_eq!(info.x, 10, "the snapshot survives an identity-only write");
                assert_eq!(age, Duration::from_secs(10), "and keeps its true age");
            }
            other => panic!("expected the remembered window, got {other:?}"),
        }
    }

    /// A different window means the old snapshot describes something else. It must not be
    /// carried forward — this is the plug-in-opens-a-dialog case.
    #[test]
    fn a_different_window_drops_the_snapshot() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 100, Some(win(100, 10)), t, alive_always);
        m.remember(7, 200, None, t + Duration::from_secs(1), alive_always);
        assert_eq!(
            m.recall(7, None, t + Duration::from_secs(1)),
            Recall::Nothing,
            "the dialog has no snapshot of its own, and the main window's will not do"
        );
    }

    /// The caller knows which window was named. A mismatch is reported as one rather than
    /// silently served.
    #[test]
    fn a_named_window_that_is_not_the_one_remembered_is_a_mismatch() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 100, Some(win(100, 10)), t, alive_always);
        assert_eq!(
            m.recall(7, Some(200), t),
            Recall::Mismatch { remembered: 100 },
            "it named 200; what is remembered is 100"
        );
        assert!(matches!(m.recall(7, Some(100), t), Recall::Serve { .. }), "the same window is fine");
    }

    /// Past the ceiling nothing is served, however often the quarantine re-arms.
    #[test]
    fn a_window_older_than_the_ceiling_is_not_served() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 100, Some(win(100, 10)), t, alive_always);
        assert!(matches!(m.recall(7, None, t + STALE_LIMIT), Recall::Serve { .. }), "at the limit");
        match m.recall(7, None, t + STALE_LIMIT + Duration::from_millis(1)) {
            Recall::TooOld { age } => assert!(age > STALE_LIMIT),
            other => panic!("expected TooOld, got {other:?}"),
        }
    }

    #[test]
    fn forgetting_leaves_nothing_to_serve() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 100, Some(win(100, 10)), t, alive_always);
        m.forget(7);
        assert_eq!(m.recall(7, None, t), Recall::Nothing);
    }

    #[test]
    fn nothing_is_remembered_for_an_application_never_seen() {
        assert_eq!(Memory::default().recall(99, None, Instant::now()), Recall::Nothing);
    }

    /// A write with nothing usable in it is not a write.
    #[test]
    fn a_zero_handle_is_not_remembered() {
        let t = Instant::now();
        let mut m = Memory::default();
        m.remember(7, 0, Some(win(0, 10)), t, alive_always);
        m.remember(0, 100, Some(win(100, 10)), t, alive_always);
        assert_eq!(m.len(), 0);
    }

    /// The watermark. With a bare threshold, a session holding 32 live applications would
    /// rebuild the map and call the liveness predicate 32 times on every single write — and
    /// the write happens on every pump tick.
    #[test]
    fn a_prune_that_frees_nothing_does_not_run_again_immediately() {
        let t = Instant::now();
        let mut m = Memory::default();
        let calls = std::cell::Cell::new(0usize);
        let alive = |_: i32| {
            calls.set(calls.get() + 1);
            true
        };
        for pid in 1..=40 {
            m.remember(pid, 1000 + pid as isize, Some(win(1000 + pid as isize, 0)), t, &alive);
        }
        let after_fill = calls.get();
        assert!(after_fill > 0, "the prune ran at least once while filling");
        for pid in 1..=40 {
            m.remember(pid, 1000 + pid as isize, None, t, &alive);
        }
        assert_eq!(calls.get(), after_fill, "and not again while nothing can be reclaimed");
        assert_eq!(m.len(), 40, "nothing was dropped: every process is alive");
    }

    /// And when processes do exit, the map really does shrink.
    #[test]
    fn a_prune_drops_the_applications_that_have_gone() {
        let t = Instant::now();
        let mut m = Memory::default();
        for pid in 1..=40 {
            m.remember(pid, 1000 + pid as isize, Some(win(1000 + pid as isize, 0)), t, alive_always);
        }
        // Only pid 40 is still running when the next prune comes round.
        for pid in 41..=90 {
            m.remember(pid, 1000 + pid as isize, Some(win(1000 + pid as isize, 0)), t, |p| p >= 40);
        }
        assert!(m.len() < 90, "the dead ones were dropped");
        assert_eq!(m.recall(1, None, t), Recall::Nothing, "pid 1 is gone");
        assert!(matches!(m.recall(40, None, t), Recall::Serve { .. }), "pid 40 is not");
    }
}
