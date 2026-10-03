//! The shape of a small region's retry ladder on a Mac: which level of Vision reads first, what
//! the neural recogniser does beside it, whether the rest of the ladder runs after a first pass
//! that read nothing, and which request revision the first passes ask for. The ladder itself is
//! `backend/macos/ocr.rs` (`recognize_captured` and the rungs after it); this is what it is told.
//!
//! The application climbs one shape, [`Shape::TODAY`]'s rungs: an accurate pass over the content
//! crop, the whole region when that read nothing, then the enlarged crop and the fast model
//! within the ladder's budget. With the neural recogniser loaded it is [`Shape::today`] with the
//! recogniser beside it as a shadow, whose answers are never used: the same rungs, and the same
//! answers but where the recogniser's work beside a pass, like any other load, takes a read past
//! the budget and so skips its last rungs. The other shapes are
//! `ocr-bench`'s strategies (`ocr/bench.rs`, `STRATEGIES`): the candidates for reading small
//! regions faster on Intel Macs, measured side by side before any of them is chosen. Modules never
//! see any of this.
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

/// What the neural recogniser (PaddleOCR, `backend/paddle_ocr.rs`) does in a read. It is asked
/// only about a small region whose ink is one line and not too wide (`paddle_pre::fits`), and only
/// once it is ready; any other region is read by today's ladder whatever this says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaddleUse {
    /// Not asked.
    Off,
    /// Asked beside the read and compared with it, never used: what a Mac does while the
    /// recogniser is measured in real sessions (`ocr/shadow.rs`). Climbs the rungs of
    /// [`PaddleUse::Off`]; on `host.ocr.read`'s recognise thread one pass of the fast level is
    /// made beside the ladder for the comparison, and its time is not counted against the
    /// ladder's budget. The recogniser's own work, at utility priority on its own thread, is load
    /// like any other: it can take a read past the budget.
    Shadow,
    /// Windows' rule: asked when the read begins, at low priority; its text answers only when the
    /// first accurate pass read nothing.
    Fallback,
    /// Asked when the read begins, at high priority, beside a first pass of the fast level: the
    /// fast text answers when the recogniser read the same (`paddle_pre::same`); the recogniser's
    /// text answers at once when the fast level read nothing.
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
    /// Whether the read may answer with the recogniser's text, or waits for it. The shadow does
    /// neither.
    pub fn reads(self) -> bool {
        !matches!(self, PaddleUse::Off | PaddleUse::Shadow)
    }

    /// Whether the read waits for the recogniser on its way rather than only where the first
    /// pass read nothing: then it is asked at high priority.
    pub fn urgent(self) -> bool {
        matches!(self, PaddleUse::Agree | PaddleUse::AgreeStrict | PaddleUse::AgreeChecked | PaddleUse::Alone)
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
    /// Today's ladder, as every read climbs it.
    pub const TODAY: Shape =
        Shape { first: Level::Accurate, paddle: PaddleUse::Off, rest: true, rung1_revision: None, rev2_second: false };

    /// What the application passes: today's ladder, and the neural recogniser beside it as a
    /// shadow where it has loaded (`paddle_ready`). The shadow reads nothing into the answer.
    pub const fn today(paddle_ready: bool) -> Shape {
        Shape { paddle: if paddle_ready { PaddleUse::Shadow } else { PaddleUse::Off }, ..Shape::TODAY }
    }

    /// Whether a read of this shape reads what today's ladder reads, pass for pass.
    pub fn is_today(&self) -> bool {
        Shape { paddle: PaddleUse::Off, ..*self } == Shape::TODAY && !self.paddle.reads()
    }
}

/// Who answered a read: the Vision pass whose text it is, the neural recogniser, or nobody (every
/// pass read nothing or was refused).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_with_or_without_the_shadow_is_today() {
        assert!(Shape::TODAY.is_today());
        assert_eq!(Shape::today(false), Shape::TODAY);
        let shadowed = Shape::today(true);
        assert_eq!(shadowed.paddle, PaddleUse::Shadow);
        assert!(shadowed.is_today(), "the shadow reads nothing into the answer");
        assert!(!Shape { rest: false, ..Shape::TODAY }.is_today());
        assert!(!Shape { first: Level::Fast, ..Shape::TODAY }.is_today());
        assert!(!Shape { paddle: PaddleUse::Fallback, ..Shape::TODAY }.is_today());
        assert!(!Shape { rung1_revision: Some(2), ..Shape::TODAY }.is_today());
        assert!(!Shape { rev2_second: true, ..Shape::TODAY }.is_today());
    }

    #[test]
    fn what_the_recogniser_does() {
        assert!(!PaddleUse::Off.reads() && !PaddleUse::Shadow.reads());
        for u in [PaddleUse::Fallback, PaddleUse::Agree, PaddleUse::AgreeStrict, PaddleUse::AgreeChecked, PaddleUse::Alone] {
            assert!(u.reads(), "{u:?}");
        }
        assert!(!PaddleUse::Fallback.urgent(), "Windows' rule waits only after a miss: low priority");
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
