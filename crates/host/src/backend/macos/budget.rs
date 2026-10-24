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
//!
//! **A walk stopped at a bound is not "not there".** What a module is told of a question whose
//! walk stopped before the end of the tree is the bound, as an error value ([`Budget::cut`]), so
//! that nothing found reads as "not known" and a dump says it is not all of the tree (`ax.rs`).

use std::time::{Duration, Instant};

use crate::errors::{element, HostError};

/// What a walk is for, which says what a module is told when it stops at one of its bounds
/// ([`Budget::cut`]): each kind has its own bounds in `ax.rs`, and the messages name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Walk {
    /// A question's search: 1500 elements and 300 ms (`QUERY_NODES`, `HOT_DEADLINE`).
    Search,
    /// `focusStep`'s and `focusWithin`'s: 800 elements and 300 ms (`FOCUS_NODES`).
    Focus,
    /// A dump's: 2000 elements and 5 s (`DUMP_NODES`, `DUMP_DEADLINE`).
    Dump,
    /// The host's own walks, which answer no module's question: a window's controls.
    Host,
}

impl Walk {
    /// What the walk is for, as a log line names it (`unanswered.rs`: `an accessibility search
    /// walk in …`).
    pub(crate) fn words(self) -> &'static str {
        match self {
            Walk::Search => "search",
            Walk::Focus => "focus",
            Walk::Dump => "dump",
            Walk::Host => "controls",
        }
    }
}

pub(crate) struct Budget {
    walk: Walk,
    nodes: i32,
    until: Instant,
    /// Which bound stopped it, so the log can tell "this tree is bigger than we look at" from
    /// "this application is answering too slowly to look at" — different faults, one symptom,
    /// and opposite responses: the first wants a larger count, the second cannot be helped by
    /// any count at all.
    ran_out_of_time: bool,
    /// A node was asked for with none left to spend: the tree went on past the count. A tree of
    /// exactly the count spends its last node and stops nothing.
    ran_out_of_nodes: bool,
}

impl Budget {
    // Called by the walks in `ax.rs`, which exist on macOS only; borrowed elsewhere for the tests,
    // which hand the clock in.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn new(walk: Walk, nodes: i32, within: Duration) -> Self {
        Self::starting_at(walk, nodes, within, Instant::now())
    }

    /// The same thing with the clock handed in, so the tests do not have to sleep.
    fn starting_at(walk: Walk, nodes: i32, within: Duration, now: Instant) -> Self {
        Self { walk, nodes, until: now + within, ran_out_of_time: false, ran_out_of_nodes: false }
    }

    /// Spends one node. `false` when there is nothing left to spend.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
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
    ///
    /// Out of time stays out of time. It once refused only the node at the clock check, so the
    /// walk went on with that node's next sibling and was refused again only 64 nodes later: the
    /// deadline dropped one node in 64, with its subtree, and the walk ran on to its count.
    fn spend_at(&mut self, now: Instant) -> bool {
        if self.ran_out_of_time {
            return false;
        }
        if self.nodes <= 0 {
            self.ran_out_of_nodes = true;
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

    /// What the walk is for.
    pub(crate) fn kind(&self) -> Walk {
        self.walk
    }

    pub(crate) fn ran_out_of_time(&self) -> bool {
        self.ran_out_of_time
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn ran_out_of_nodes(&self) -> bool {
        self.ran_out_of_nodes
    }

    /// What a module is told of a walk of a question that stopped at a bound before the end of the
    /// tree: its time first, which no larger count would help; `None` for a walk that reached the
    /// end, and for the host's own walks.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn cut(&self) -> Option<HostError> {
        let reason = match (self.walk, self.ran_out_of_time, self.ran_out_of_nodes) {
            (Walk::Host, _, _) | (_, false, false) => return None,
            (Walk::Dump, true, _) => &element::TOO_SLOW,
            (Walk::Search | Walk::Focus, true, _) => &element::HOT_TOO_SLOW,
            (Walk::Search, false, true) => &element::SEARCH_CUT,
            (Walk::Focus, false, true) => &element::FOCUS_CUT,
            (Walk::Dump, false, true) => &element::MAC_DUMP_CUT,
        };
        Some(HostError::of(reason))
    }
}

/// The applications a walk of a module's question ran out of time in, in this turn of the event
/// loop: their later questions in the same turn are answered "too slow" at once, without a walk.
///
/// The second Mac test (2026-10-24, its finding 5) had Logic's file panel open while seven VMs —
/// every embedded overlay's — asked their identity questions in it, each walk stopping at its
/// 300 ms (55c5608) and the next starting from scratch: about 2 s of one turn, on the thread that
/// carries the event tap, which macOS switches off past its own limit. An application that could
/// not be walked in 300 ms is not walked again in the turn it showed it; the next turn asks it
/// afresh. A count of turns, not a clock: nothing here decides how long.
#[derive(Default)]
pub(crate) struct SlowThisTurn {
    /// The applications whose walk ran out of time in this turn.
    pids: Vec<i32>,
    /// Questions answered without a walk in this turn, per application, for the turn's one line.
    skipped: Vec<(i32, u32)>,
}

impl SlowThisTurn {
    /// A walk of a question to `pid` ran out of time.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn ran_out(&mut self, pid: i32) {
        if !self.pids.contains(&pid) {
            self.pids.push(pid);
        }
    }

    /// Whether a question to `pid` is answered "too slow" without a walk: its walk ran out of time
    /// earlier in this turn. Counted for the turn's line.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn skip(&mut self, pid: i32) -> bool {
        if !self.pids.contains(&pid) {
            return false;
        }
        match self.skipped.iter_mut().find(|(p, _)| *p == pid) {
            Some((_, n)) => *n += 1,
            None => self.skipped.push((pid, 1)),
        }
        true
    }

    /// The next turn begins: everything is asked afresh. What the turn that ended answered without
    /// a walk, per application, for its one line; empty when nothing was.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn next_turn(&mut self) -> Vec<(i32, u32)> {
        self.pids.clear();
        std::mem::take(&mut self.skipped)
    }
}

/// The turn's one line: how many questions to each application were answered "too slow" without
/// a walk, after one of its walks ran out of time in that turn. `exe` names an application.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn slow_turn_line(skipped: &[(i32, u32)], exe: impl Fn(i32) -> String) -> Option<String> {
    if skipped.is_empty() {
        return None;
    }
    let each: Vec<String> = skipped.iter().map(|(pid, n)| format!("{n} to '{}' (pid {pid})", exe(*pid))).collect();
    Some(format!(
        "accessibility: {} question(s) of modules in one turn were answered 'too slow' without a walk — \
         an earlier walk of the same application ran out of time in that turn: {}",
        skipped.iter().map(|(_, n)| n).sum::<u32>(),
        each.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Finding 5 of the second Mac test: after one walk of Logic's ran out of time, its later
    /// questions in the same turn are answered at once, another application's are walked, and the
    /// next turn walks Logic again — with one line for the turn that skipped.
    #[test]
    fn an_application_slow_in_this_turn_is_not_walked_again_in_it() {
        let mut slow = SlowThisTurn::default();
        assert!(!slow.skip(6000), "nothing ran out yet");
        slow.ran_out(6000);
        slow.ran_out(6000);
        for _ in 0..6 {
            assert!(slow.skip(6000), "the six other VMs' questions in the same turn");
        }
        assert!(!slow.skip(5000), "another application is walked");
        let ended = slow.next_turn();
        assert_eq!(ended, [(6000, 6)]);
        assert!(!slow.skip(6000), "the next turn asks it afresh");
        assert!(slow.next_turn().is_empty());
        let line = slow_turn_line(&ended, |_| "Logic Pro X".into()).unwrap();
        assert_eq!(
            line,
            "accessibility: 6 question(s) of modules in one turn were answered 'too slow' without a walk — an \
             earlier walk of the same application ran out of time in that turn: 6 to 'Logic Pro X' (pid 6000)"
        );
        assert_eq!(slow_turn_line(&[], |_| String::new()), None);
    }

    #[test]
    fn it_spends_exactly_the_nodes_it_was_given() {
        let t = Instant::now();
        let mut b = Budget::starting_at(Walk::Search, 5, Duration::from_secs(60), t);
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
        let mut b = Budget::starting_at(Walk::Search, 1000, Duration::from_millis(300), t);
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
        let mut b = Budget::starting_at(Walk::Search, 100, Duration::from_millis(1), t);
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
        let mut b = Budget::starting_at(Walk::Search, 16, Duration::from_millis(1), t);
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
        let mut b = Budget::starting_at(Walk::Search, 1, Duration::from_secs(60), t);
        assert!(b.spend_at(t));
        assert!(!b.spend_at(t));
        assert!(!b.spend_at(t), "and asking again does not make it affordable");
        assert_eq!(b.nodes_left(), 0, "it never goes negative");
    }

    /// Past its deadline a walk stays stopped: the siblings after the refused node are refused too,
    /// without a node spent, rather than walked until the next clock check.
    #[test]
    fn a_walk_out_of_time_stays_out_of_time() {
        let t = Instant::now();
        let late = t + Duration::from_millis(301);
        let mut b = Budget::starting_at(Walk::Search, 1000, Duration::from_millis(300), t);
        while b.spend_at(late) {}
        assert!(b.ran_out_of_time());
        let left = b.nodes_left();
        for _ in 0..200 {
            assert!(!b.spend_at(late), "a sibling after the deadline is not walked");
        }
        assert_eq!(b.nodes_left(), left, "and spends nothing");
        assert!(!b.ran_out_of_nodes());
    }

    /// A walk carries what it is for, and says it in the words of the line a walk writes when an
    /// application did not complete one of its reads (`unanswered.rs`), each kind its own.
    #[test]
    fn a_walk_says_what_it_is_for() {
        let t = Instant::now();
        for (walk, words) in [(Walk::Search, "search"), (Walk::Focus, "focus"), (Walk::Dump, "dump"), (Walk::Host, "controls")] {
            assert_eq!(Budget::starting_at(walk, 1, Duration::from_secs(1), t).kind(), walk);
            assert_eq!(walk.words(), words);
        }
    }

    /// What a module is told of each bound of each walk: nothing for a walk that reached the end —
    /// a tree of exactly the count included — or for the host's own; time before nodes; and the
    /// numbers in the words are the bounds ax.rs gives each walk.
    #[test]
    fn a_walk_stopped_at_a_bound_says_which() {
        let t = Instant::now();
        let ran = |walk: Walk, nodes: i32, spend: usize, at: Instant| {
            let mut b = Budget::starting_at(walk, nodes, Duration::from_millis(300), t);
            for _ in 0..spend {
                b.spend_at(at);
            }
            b
        };
        for walk in [Walk::Search, Walk::Focus, Walk::Dump, Walk::Host] {
            assert_eq!(ran(walk, 16, 16, t).cut(), None, "{walk:?}: a tree of exactly the count is all walked");
            assert_eq!(ran(walk, 16, 3, t).cut(), None);
        }
        let over = |walk| ran(walk, 16, 17, t).cut();
        assert_eq!(over(Walk::Search), Some(HostError::of(&element::SEARCH_CUT)));
        assert_eq!(over(Walk::Focus), Some(HostError::of(&element::FOCUS_CUT)));
        assert_eq!(over(Walk::Dump), Some(HostError::of(&element::MAC_DUMP_CUT)));
        assert_eq!(over(Walk::Host), None, "the host's own walks answer no question");
        let late = t + Duration::from_secs(10);
        let slow = |walk| ran(walk, 200, 200, late).cut();
        assert_eq!(slow(Walk::Search), Some(HostError::of(&element::HOT_TOO_SLOW)));
        assert_eq!(slow(Walk::Focus), Some(HostError::of(&element::HOT_TOO_SLOW)));
        assert_eq!(slow(Walk::Dump), Some(HostError::of(&element::TOO_SLOW)));
        assert_eq!(slow(Walk::Host), None);

        // The words are the bounds: each constant of ax.rs, as the walk of its kind is made with it.
        const AX: &str = include_str!("ax.rs");
        let constant = |name: &str| {
            let at = AX.find(&format!("const {name}: ")).unwrap_or_else(|| panic!("ax.rs has no {name}"));
            AX[at..].split_once(" = ").unwrap().1.split_once(';').unwrap().0.trim().to_string()
        };
        assert_eq!(constant("QUERY_NODES"), "1500");
        assert_eq!(constant("FOCUS_NODES"), "800");
        assert_eq!(constant("DUMP_NODES"), "2000");
        assert_eq!(constant("HOT_DEADLINE"), "Duration::from_millis(300)");
        assert_eq!(constant("DUMP_DEADLINE"), "Duration::from_secs(5)");
        assert!(element::SEARCH_CUT.message.contains("the 1500 elements"));
        assert!(element::FOCUS_CUT.message.contains("the 800 elements"));
        assert!(element::MAC_DUMP_CUT.message.contains("the 2000 elements"));
        assert!(element::HOT_TOO_SLOW.message.ends_with("300 ms"));
        assert!(element::TOO_SLOW.message.ends_with("5 s"));
        // Every walk of a question is made with its kind's bounds, and the control walk is the
        // host's own.
        for (kind, nodes, deadline) in [
            ("Walk::Search", "QUERY_NODES", "HOT_DEADLINE"),
            ("Walk::Focus", "FOCUS_NODES", "HOT_DEADLINE"),
            ("Walk::Dump", "DUMP_NODES", "DUMP_DEADLINE"),
            ("Walk::Host", "CONTROL_NODES", "HOT_DEADLINE"),
        ] {
            assert!(AX.contains(&format!("Budget::new({kind}, {nodes}, {deadline})")), "{kind}");
        }
        let made = AX.matches("Budget::new(").count();
        let kinds = ["Walk::Search", "Walk::Focus", "Walk::Dump", "Walk::Host"];
        let named = kinds.iter().map(|k| AX.matches(&format!("Budget::new({k}, ")).count()).sum::<usize>();
        assert_eq!(made, named, "every walk says what it is for");
        assert_eq!(AX.matches("Budget::new(Walk::Search, nodes, HOT_DEADLINE)").count(), 1, "first_match, with QUERY_NODES");
        // And each walk is the kind of the function that makes it: a question's walk made as the
        // host's would lose its "not known" without a word, so the whole list is held. The
        // function a walk is made in is the last `fn` at column 0 before it.
        let mut walks: Vec<(String, &str)> = Vec::new();
        for (at, _) in AX.match_indices("Budget::new(") {
            let head = AX[..at].rmatch_indices("\nfn ").chain(AX[..at].rmatch_indices("\npub fn ")).map(|(i, _)| i).max().unwrap();
            let name = AX[head..].split('(').next().unwrap().rsplit(' ').next().unwrap().to_string();
            let kind = *kinds.iter().find(|k| AX[at..].starts_with(&format!("Budget::new({k}, "))).unwrap();
            walks.push((name, kind));
        }
        let want: Vec<(String, &str)> = [
            ("window_controls", "Walk::Host"),
            ("find", "Walk::Search"),
            ("find_any", "Walk::Search"),
            ("first_match", "Walk::Search"),
            ("plugin_containers", "Walk::Search"),
            ("dump", "Walk::Dump"),
            ("class_nav_point", "Walk::Search"),
            ("class_nav_point", "Walk::Search"),
            ("focus_step", "Walk::Focus"),
            ("focus_within", "Walk::Focus"),
        ]
        .into_iter()
        .map(|(f, k)| (f.to_string(), k))
        .collect();
        assert_eq!(walks, want, "a walk's kind, by the function that makes it");
    }
}
