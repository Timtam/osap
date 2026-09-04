//! What a bounded accessibility walk has left: nodes, and time.
//!
//! Its own file for the reason `keys.rs` and `front_memory.rs` have one: this is arithmetic
//! over a counter and a clock, with nothing macOS about it, so `backend/mod.rs` borrows it by
//! `#[path]` under `cfg(test)` and it can be executed on the machine where it is written. The
//! rest of that backend can only be compiled here.
//!
//! **Why a deadline as well as a count.** The node counts in `ax.rs` bound how many elements a
//! walk visits. They do not bound how long that takes, and on this platform every element is a
//! cross-process round trip whose own ceiling is the messaging timeout — so six hundred nodes
//! is bounded at six hundred seconds, which is not a bound anybody can use. The Windows
//! backend learned this on 2026-09-04: after its per-call timeout went in, one window still
//! took thirty seconds to walk, because a budget that counts nodes cannot see a clock.

use std::time::{Duration, Instant};

pub(crate) struct Budget {
    nodes: i32,
    until: Instant,
    /// Which bound stopped it, so the log can tell "this tree is bigger than we look at" from
    /// "this application is answering too slowly to look at" — different faults, one symptom,
    /// and opposite responses: the first wants a larger count, the second cannot be helped by
    /// any count at all.
    ran_out_of_time: bool,
}

impl Budget {
    pub(crate) fn new(nodes: i32, within: Duration) -> Self {
        Self::starting_at(nodes, within, Instant::now())
    }

    /// The same thing with the clock handed in, so the tests do not have to sleep.
    fn starting_at(nodes: i32, within: Duration, now: Instant) -> Self {
        Self { nodes, until: now + within, ran_out_of_time: false }
    }

    /// Spends one node. `false` when there is nothing left to spend.
    pub(crate) fn spend(&mut self) -> bool {
        self.spend_at(Instant::now())
    }

    /// The clock is read every 64 nodes rather than every one: `Instant::now` is a syscall on
    /// some platforms, and a walk that measured itself more often than it worked would be its
    /// own problem. Sixty-four nodes is well under the hot deadline even at the slowest read
    /// the tester's machine has produced.
    ///
    /// A consequence worth knowing rather than discovering: a tree of fewer than 64 nodes
    /// never consults the clock at all. That is the intended trade — such a walk cannot be
    /// slow enough to matter — and sforzando's entire window came to sixteen.
    fn spend_at(&mut self, now: Instant) -> bool {
        if self.nodes <= 0 {
            return false;
        }
        self.nodes -= 1;
        // `self.nodes != 0` is not tidiness. Without it, a walk whose node count lands exactly
        // on zero consults the clock on that last step — and if the deadline has also passed,
        // reports that it ran out of TIME when what ran out was nodes. The two want opposite
        // responses (a larger count, or no count at all will help), so getting the attribution
        // backwards sends the next reader the wrong way. Found by the tests below, not by
        // reading this.
        if self.nodes != 0 && self.nodes % 64 == 0 && now >= self.until {
            self.ran_out_of_time = true;
            return false;
        }
        true
    }

    /// How many nodes are left, for the callers that report how many were visited.
    pub(crate) fn nodes_left(&self) -> i32 {
        self.nodes
    }

    pub(crate) fn ran_out_of_time(&self) -> bool {
        self.ran_out_of_time
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_spends_exactly_the_nodes_it_was_given() {
        let t = Instant::now();
        let mut b = Budget::starting_at(5, Duration::from_secs(60), t);
        for i in 0..5 {
            assert!(b.spend_at(t), "node {i} should have been affordable");
        }
        assert!(!b.spend_at(t), "the sixth was not paid for");
        assert!(!b.ran_out_of_time(), "it ran out of NODES, and the log must not say time");
        assert_eq!(b.nodes_left(), 0);
    }

    #[test]
    fn a_walk_past_its_deadline_stops_and_says_so() {
        let t = Instant::now();
        let mut b = Budget::starting_at(1000, Duration::from_millis(300), t);
        // Up to the first clock check, which is when the count reaches a multiple of 64.
        let late = t + Duration::from_millis(301);
        let mut spent = 0;
        while b.spend_at(late) {
            spent += 1;
            assert!(spent < 100, "it should have stopped at the first clock check");
        }
        assert!(b.ran_out_of_time(), "it ran out of TIME, and the log must say so");
        // 39, not 40: the call that brings the count to 960 is the one that consults the
        // clock, and it is refused rather than counted.
        assert_eq!(spent, 1000 - 960 - 1);
    }

    #[test]
    fn time_is_only_consulted_every_64_nodes() {
        let t = Instant::now();
        let late = t + Duration::from_secs(10);
        let mut b = Budget::starting_at(100, Duration::from_millis(1), t);
        // 100 -> 99 … the first multiple of 64 below 100 is 64, so thirty-five nodes are
        // spent even though the deadline passed before the walk began. That is the
        // granularity, stated rather than discovered.
        let mut spent = 0;
        while b.spend_at(late) {
            spent += 1;
        }
        assert_eq!(spent, 100 - 64 - 1, "the step that lands on 64 is the one refused");
        assert!(b.ran_out_of_time());
    }

    #[test]
    fn a_small_tree_never_asks_the_clock() {
        let t = Instant::now();
        let long_past = t + Duration::from_secs(600);
        // Sixteen nodes — sforzando's whole window — with a deadline that expired ten minutes
        // ago. It still completes, because nothing that short can be slow enough to matter.
        let mut b = Budget::starting_at(16, Duration::from_millis(1), t);
        let mut spent = 0;
        while b.spend_at(long_past) {
            spent += 1;
        }
        assert_eq!(spent, 16, "all sixteen were spent");
        assert!(!b.ran_out_of_time(), "it finished on nodes, never having looked at the clock");
    }

    #[test]
    fn an_exhausted_budget_stays_exhausted() {
        let t = Instant::now();
        let mut b = Budget::starting_at(1, Duration::from_secs(60), t);
        assert!(b.spend_at(t));
        assert!(!b.spend_at(t));
        assert!(!b.spend_at(t), "and asking again does not make it affordable");
        assert_eq!(b.nodes_left(), 0, "it never goes negative");
    }
}
