//! The shape of a small region's retry ladder on a Mac: which level of Vision reads first, what
//! the neural recogniser does beside it, whether the rest of the ladder runs after a first pass
//! that read nothing, and which request revision the first passes ask for. The ladder itself is
//! `backend/macos/ocr.rs` (`recognize_captured` and the rungs after it); this is what it is told,
//! and who answered a read ([`Answer`], [`Path`]).
//!
//! The application climbs [`Shape::production`]. Where the neural recogniser has not loaded it is
//! [`Shape::TODAY`]'s rungs: an accurate pass over the content crop, the whole region when that
//! read nothing, then the enlarged crop and the fast model within the ladder's budget. Where it
//! has, it is Windows' rule with Vision's accurate ladder in the system recogniser's place
//! ([`PaddleUse::Merge`], `ocr/merge.rs`), and on an Intel Mac the fast level checked by the
//! recogniser before it ([`PaddleUse::FastMerge`]). The other shapes are `ocr-bench`'s strategies
//! (`ocr/bench.rs`, `STRATEGIES`): today's ladder as it read before, for comparison, and the other
//! ways of reading small regions that were measured side by side. Modules never see any of this
//! beyond what Windows shows them too: where the recogniser answers alone, text with no words of
//! the engine's, and an approximate box.
//!
//! Pure, so the bench's strategies are tested on every platform; `crates/macos-check` borrows it.

// Everywhere but on a Mac only the tests use it.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::cost::Stage;

/// The level of Vision's first pass over the content crop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// The accurate model, language-aware: what every first pass is today.
    Accurate,
    /// The fast model, character by character: 4–6 % of the accurate pass's time on the CI's
    /// Macs without a Neural Engine, and blind to a lone digit there (ocr-bench, 2026-10-02).
    Fast,
}

/// What the neural recogniser (PaddleOCR, `backend/paddle_ocr.rs`) does in a read, once it is
/// ready. The application's two ([`PaddleUse::Merge`], [`PaddleUse::FastMerge`]) ask it about every
/// small region the blank guard did not answer, as Windows does; `ocr-bench`'s others only about
/// one whose ink is one line and not too wide (`paddle_pre::fits`), and leave any other region to
/// today's ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaddleUse {
    /// Not asked: the ladder ends with the fast rung, as it always did.
    Off,
    /// Windows' rule (`ocr/merge.rs`), the application's on every Mac where the recogniser has
    /// loaded: asked when the read has its content crop, at the reading thread's priority; Vision's
    /// accurate ladder — the tight crop, the whole region, the enlarged crop within the budget, no
    /// fast rung — answers when it reads anything; otherwise the recogniser's text, waited for
    /// within the budget, unless Vision refused the first pass. Not asked after all (the exit has
    /// begun, its queue is full, its thread has gone): today's ladder, the fast rung included. On
    /// `host.ocr.read`'s recognise thread
    /// one pass of the fast level is made once the answer is picked, for the shadow's counts only
    /// (`ocr/shadow.rs`), so that it changes nothing of the answer or the budget.
    Merge,
    /// An Intel Mac's: [`PaddleUse::Merge`], with the fast level first for a region that fits and a
    /// language it reads — its text, with the recogniser's spacing, where the recogniser read the
    /// same, spaces included (`merge::check`); otherwise `Merge` from its first rung. The check is
    /// waited for no longer than the fast pass took (`merge::check_wait`).
    FastMerge,
    /// `ocr-bench`'s `acc+paddle`: asked when the read begins, at low priority; its text answers
    /// only when the first accurate pass read nothing.
    Fallback,
    /// Asked when the read begins, at high priority, beside a first pass of the fast level: the
    /// fast text, with the recogniser's spacing, answers when the recogniser read the same as the
    /// application's check judges it (`merge::check`); the recogniser's text answers at once when
    /// the fast level read nothing.
    Agree,
    /// As [`PaddleUse::Agree`], but when the fast level read nothing an accurate pass over the
    /// content crop goes first, and the recogniser's text answers only when that reads nothing too.
    AgreeStrict,
    /// As [`PaddleUse::Agree`], but the recogniser's text never answers: when the fast level read
    /// nothing, the accurate ladder without the fast rung at its end, and nothing when that reads
    /// nothing. The reading rules as the maintainer set them on 2026-10-02 — never an unverified
    /// value; the fast level only where the recogniser reads the same; the recogniser alone only
    /// once measurements show it invents nothing — before that last step.
    AgreeChecked,
    /// The recogniser alone, Vision not asked: `ocr-bench`'s `paddle`.
    Alone,
}

impl PaddleUse {
    /// Whether the read may answer with the recogniser's text, or waits for it.
    pub fn reads(self) -> bool {
        !matches!(self, PaddleUse::Off)
    }

    /// Whether it is the application's way: Windows' rule, with or without the fast level's check.
    pub fn merges(self) -> bool {
        matches!(self, PaddleUse::Merge | PaddleUse::FastMerge)
    }

    /// Whether the read waits for the recogniser on its way rather than only where the first
    /// pass read nothing: then it is asked at high priority. The application's ways ask at the
    /// reading thread's own (`paddle_ocr::Qos::Reader`), which is never below a read's.
    pub fn urgent(self) -> bool {
        matches!(
            self,
            PaddleUse::Merge
                | PaddleUse::FastMerge
                | PaddleUse::Agree
                | PaddleUse::AgreeStrict
                | PaddleUse::AgreeChecked
                | PaddleUse::Alone
        )
    }
}

/// One shape of the ladder. Every field is something a strategy of `ocr-bench` varies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// The level of the first pass over the content crop.
    pub first: Level,
    pub paddle: PaddleUse,
    /// Whether today's rungs after the first pass run when the first pass (and the neural
    /// recogniser, where it is asked) read nothing.
    pub rest: bool,
    /// The request revision the first accurate pass asks for; `None` leaves Vision's default,
    /// which is revision 3 on every Mac measured so far (and does not exist on macOS 12).
    pub rung1_revision: Option<usize>,
    /// After a first accurate pass that read nothing, revision 2 over the same crop instead of a
    /// pass over the whole region.
    pub rev2_second: bool,
}

impl Shape {
    /// Today's ladder, as every read climbed it before the neural recogniser answered on a Mac,
    /// and as one still climbs it where the recogniser has not loaded.
    pub const TODAY: Shape =
        Shape { first: Level::Accurate, paddle: PaddleUse::Off, rest: true, rung1_revision: None, rev2_second: false };

    /// What the application passes: today's ladder until the neural recogniser is ready
    /// (`paddle_ready`), and then Windows' rule over the accurate ladder — on an Intel Mac
    /// (`intel`, [`intel_mac`]) with the fast level checked by the recogniser first, where Vision has
    /// no Neural Engine and one accurate pass costs fifteen to twenty fast ones.
    pub const fn production(paddle_ready: bool, intel: bool) -> Shape {
        match (paddle_ready, intel) {
            (false, _) => Shape::TODAY,
            (true, false) => Shape { paddle: PaddleUse::Merge, ..Shape::TODAY },
            (true, true) => Shape { first: Level::Fast, paddle: PaddleUse::FastMerge, ..Shape::TODAY },
        }
    }

    /// Whether a read of this shape reads what today's ladder reads, pass for pass.
    pub fn is_today(&self) -> bool {
        *self == Shape::TODAY
    }
}

/// Whether this Mac is an Intel one, which reads with the fast level checked first
/// ([`Shape::production`]): this is the x86_64 slice (`x86_64`), and it is not translated by
/// Rosetta on Apple silicon (`translated`, `sysctl.proc_translated`: 1 when it is, absent on an
/// Intel Mac). Decided by the processor, not by what Vision reports of a Neural Engine: every Intel
/// Mac lacks one, and CI's Apple-silicon virtual machines, which report none either, read a small
/// field 15 to 20 times faster than its Intel one.
pub fn intel_mac(x86_64: bool, translated: Option<i32>) -> bool {
    x86_64 && translated != Some(1)
}

/// Who answered a read: the Vision pass whose text it is, the neural recogniser, or nobody (every
/// pass read nothing or was refused). [`Path`] says it in the four words the log uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Pass(Stage),
    Paddle,
    Nobody,
}

impl Answer {
    /// The short word a bench row counts it by.
    pub fn word(self) -> &'static str {
        match self {
            Answer::Pass(Stage::AsCaptured) => "as captured",
            Answer::Pass(Stage::Tight) => "tight",
            Answer::Pass(Stage::Whole) => "whole",
            Answer::Pass(Stage::Enlarged) => "enlarged",
            Answer::Pass(Stage::Fast) => "fast last",
            Answer::Pass(Stage::FastFirst) => "fast first",
            Answer::Pass(Stage::TightAgain) => "tight again",
            // Never an answer: the shadow's pass is only compared (`ocr/shadow.rs`).
            Answer::Pass(Stage::FastCompared) => "fast, compared",
            Answer::Pass(Stage::WarmUp) => "warm-up",
            Answer::Paddle => "Paddle",
            Answer::Nobody => "nobody",
        }
    }

    /// Who answered, from the last pass a read made — `(stage, words read)`, `None` for a pass
    /// Vision refused — when the neural recogniser did not. Every rung runs only when the rung
    /// before it read nothing, but for the first pass of the fast level, which a second opinion
    /// may follow whatever it read; so the answer is the last pass if it read any word, and
    /// nobody's otherwise.
    pub fn of_last_pass(last: Option<(Stage, Option<usize>)>) -> Answer {
        match last {
            Some((stage, Some(words))) if words > 0 => Answer::Pass(stage),
            _ => Answer::Nobody,
        }
    }
}

/// Which way a read was answered, as the log says it and the shadow counts it (`ocr/shadow.rs`):
/// the fast level and the neural recogniser agreeing, Vision, the neural recogniser alone (where
/// Vision read nothing), or nothing at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Agreed,
    Vision,
    Paddle,
    Nothing,
}

impl Path {
    pub const ALL: [Path; 4] = [Path::Agreed, Path::Vision, Path::Paddle, Path::Nothing];

    /// From who answered. The fast level's first pass answers only where the recogniser read the
    /// same, in the application and in every strategy of `ocr-bench` but `fast>acc`, a comparison
    /// that takes it unchecked.
    pub fn of(answer: Answer) -> Path {
        match answer {
            Answer::Pass(Stage::FastFirst) => Path::Agreed,
            Answer::Pass(_) => Path::Vision,
            Answer::Paddle => Path::Paddle,
            Answer::Nobody => Path::Nothing,
        }
    }

    /// The words the log says it in: `answered by <word>`.
    pub fn word(self) -> &'static str {
        match self {
            Path::Agreed => "the fast level and Paddle agreeing",
            Path::Vision => "Vision",
            Path::Paddle => "Paddle alone",
            Path::Nothing => "nobody",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The application's shape: today's ladder until the recogniser is ready, then Windows' rule,
    /// and on an Intel Mac the fast level checked first.
    #[test]
    fn production_is_today_until_the_recogniser_is_ready_then_windows_rule() {
        assert_eq!(Shape::production(false, false), Shape::TODAY);
        assert_eq!(Shape::production(false, true), Shape::TODAY, "no check without the recogniser");
        let apple = Shape::production(true, false);
        assert_eq!((apple.first, apple.paddle, apple.rest), (Level::Accurate, PaddleUse::Merge, true));
        let intel = Shape::production(true, true);
        assert_eq!((intel.first, intel.paddle, intel.rest), (Level::Fast, PaddleUse::FastMerge, true));
        for s in [apple, intel] {
            assert!(!s.is_today() && s.paddle.merges() && s.paddle.urgent() && s.paddle.reads());
            assert_eq!((s.rung1_revision, s.rev2_second), (None, false), "the accurate ladder as today");
        }
        assert!(!PaddleUse::Fallback.merges() && !PaddleUse::Off.merges());
    }

    #[test]
    fn an_intel_mac_is_the_x86_64_slice_not_translated() {
        assert!(intel_mac(true, None), "an Intel Mac has no sysctl.proc_translated");
        assert!(intel_mac(true, Some(0)));
        assert!(!intel_mac(true, Some(1)), "Rosetta on Apple silicon");
        assert!(!intel_mac(false, None) && !intel_mac(false, Some(0)));
    }

    #[test]
    fn who_answered_in_the_logs_four_words() {
        assert_eq!(Path::of(Answer::Pass(Stage::FastFirst)), Path::Agreed);
        for s in [Stage::Tight, Stage::Whole, Stage::Enlarged, Stage::Fast, Stage::AsCaptured, Stage::TightAgain] {
            assert_eq!(Path::of(Answer::Pass(s)), Path::Vision, "{s:?}");
        }
        assert_eq!(Path::of(Answer::Paddle), Path::Paddle);
        assert_eq!(Path::of(Answer::Nobody), Path::Nothing);
        let words: std::collections::HashSet<&str> = Path::ALL.iter().map(|p| p.word()).collect();
        assert_eq!(words.len(), 4);
        assert_eq!(Path::Paddle.word(), "Paddle alone");
    }

    #[test]
    fn today_is_today() {
        assert!(Shape::TODAY.is_today());
        assert!(!Shape { rest: false, ..Shape::TODAY }.is_today());
        assert!(!Shape { first: Level::Fast, ..Shape::TODAY }.is_today());
        assert!(!Shape { paddle: PaddleUse::Fallback, ..Shape::TODAY }.is_today());
        assert!(!Shape { rung1_revision: Some(2), ..Shape::TODAY }.is_today());
        assert!(!Shape { rev2_second: true, ..Shape::TODAY }.is_today());
    }

    #[test]
    fn what_the_recogniser_does() {
        assert!(!PaddleUse::Off.reads());
        for u in [PaddleUse::Fallback, PaddleUse::Agree, PaddleUse::AgreeStrict, PaddleUse::AgreeChecked, PaddleUse::Alone] {
            assert!(u.reads(), "{u:?}");
        }
        assert!(!PaddleUse::Fallback.urgent(), "acc+paddle waits only after a miss: low priority");
        for u in [PaddleUse::Agree, PaddleUse::AgreeStrict, PaddleUse::AgreeChecked, PaddleUse::Alone] {
            assert!(u.urgent(), "{u:?}: the fast level's check is waited for");
        }
    }

    #[test]
    fn the_answer_is_the_last_pass_that_read_a_word() {
        assert_eq!(Answer::of_last_pass(None), Answer::Nobody);
        assert_eq!(Answer::of_last_pass(Some((Stage::Tight, Some(1)))), Answer::Pass(Stage::Tight));
        assert_eq!(Answer::of_last_pass(Some((Stage::Whole, Some(2)))), Answer::Pass(Stage::Whole));
        assert_eq!(Answer::of_last_pass(Some((Stage::Whole, Some(0)))), Answer::Nobody);
        assert_eq!(Answer::of_last_pass(Some((Stage::Tight, None))), Answer::Nobody, "refused");
        // The fast level read something, the recogniser disagreed, and the accurate ladder read
        // nothing: its last pass read nothing, and nobody answers.
        assert_eq!(Answer::of_last_pass(Some((Stage::Tight, Some(0)))), Answer::Nobody);
        let words: std::collections::HashSet<&str> = [
            Answer::Pass(Stage::AsCaptured),
            Answer::Pass(Stage::Tight),
            Answer::Pass(Stage::Whole),
            Answer::Pass(Stage::Enlarged),
            Answer::Pass(Stage::Fast),
            Answer::Pass(Stage::FastFirst),
            Answer::Pass(Stage::TightAgain),
            Answer::Pass(Stage::FastCompared),
            Answer::Paddle,
            Answer::Nobody,
        ]
        .iter()
        .map(|a| a.word())
        .collect();
        assert_eq!(words.len(), 10);
    }
}
