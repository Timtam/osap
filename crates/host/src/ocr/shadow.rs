//! The neural recogniser's shadow on a Mac: what it would have changed, counted from real
//! sessions, while nothing it reads is used.
//!
//! Wherever the neural recogniser (`backend/paddle_ocr.rs`) has loaded, every small region a read
//! climbs today's ladder for is handed to it as well, at utility priority, the moment the content
//! crop is known; and on `host.ocr.read`'s recognise thread the fast level of Vision makes one pass
//! over the same rendered crop right after the first accurate pass. Neither answer is used, and the
//! ladder climbs the rungs it climbs without them, the fast pass's time set aside from its budget —
//! though the recogniser's work beside it is load like any other, and can take a read past that
//! budget. Once the ladder has answered, the recogniser's
//! answer is taken if it is there — never waited for: one that has not answered is cancelled and
//! counted as late — and the three are compared ([`Tally::add`]).
//!
//! The counts are what the two ways of reading small regions faster on an Intel Mac would have
//! done in real sessions, before either of them is built: the recogniser answering where the first
//! accurate pass read nothing (the plan's c1, Windows' rule), and the fast level first, believed
//! only where the recogniser reads the same (c2). Above all they count the one mistake c2 could not
//! see once it is built, because the accurate pass that shows it would no longer run: the fast
//! level and the recogniser agreeing on a reading the ladder did not give. And they count how often
//! the recogniser reads text where Vision reads none — a lone digit gained, or text invented on ink
//! that is not text, which `ocr-bench` saw it do on Windows on four of nine pictures without
//! text; the pictures ([`Pictures`]) tell the two apart.
//!
//! The log gets one line with every count after each [`LINE_EVERY`]th read and at exit
//! ([`Tally::line`]) — counted, not timed — and a trace line for each read in which they disagree
//! ([`trace_line`]). Readings are compared by `paddle_pre::same`: spaces, dash and quote forms
//! aside, and nothing else.
//!
//! Pure, so it is tested on every platform; `crates/macos-check` borrows it.

// Everywhere but on a Mac only the tests use it.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use super::paddle_pre::same;

/// What the neural recogniser had to say about one region by the time the ladder had answered.
#[derive(Clone, Debug, PartialEq)]
pub enum Paddle {
    /// Not asked: the application's exit had begun, its queue was full, or its thread had gone.
    NotAsked,
    /// Asked, and not answered yet when the ladder had: cancelled then, since nobody waits for it.
    NotDone,
    /// Asked, and no answer will come: the recognition panicked, the pixels did not make a picture,
    /// or the recogniser's thread had gone — each of which the log says on its own.
    Failed,
    /// It read nothing.
    Nothing,
    /// Its text, trimmed and never empty, the score `paddle_pre::decode` gave it, and the
    /// milliseconds its recognition took on its own thread.
    Text { text: String, score: f32, ms: f64 },
}

/// One small read, as the shadow compares it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// The first accurate pass over the content crop: the words it read, `None` when Vision refused
    /// the request.
    pub first: Option<usize>,
    /// What the read answered — the ladder's text; empty for nothing.
    pub answer: String,
    /// Whether the ladder read the whole region, its second rung, after a first pass that read
    /// nothing: the pass the plan's c1 would spare where the recogniser reads.
    pub whole: bool,
    /// The fast level's pass over the content crop, made only on the recognise thread and only for a
    /// language that level reads: its text, empty for nothing; `None` where none was made or Vision
    /// refused it.
    pub fast: Option<String>,
    pub paddle: Paddle,
    /// Whether the region was the recogniser's to read by `paddle_pre::fits` — one line of ink, not
    /// too wide. The plan's ways of reading leave any other region to today's ladder, so it is
    /// counted apart.
    pub fits: bool,
    /// The read's Vision passes, and how many of them the recogniser ran beside
    /// (`cost::Pass::paddle_beside`).
    pub passes: usize,
    pub passes_beside: usize,
}

/// How the recogniser's answer stood to the ladder's, by what the first accurate pass read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum First {
    // The first accurate pass read text, which is then the answer.
    /// The recogniser read the same.
    Same,
    /// It read something else.
    Differs,
    /// It read nothing.
    Nothing,
    // The first accurate pass read nothing, or was refused, and the ladder went on.
    /// The ladder read nothing and the recogniser text: a lone digit gained, or text invented on ink
    /// that is none — Windows' rule would have said it.
    PaddleAlone,
    /// The ladder read text and the recogniser nothing.
    LadderAlone,
    /// Both read text, the same.
    BothSame,
    /// Both read text, and not the same.
    BothDiffer,
    /// Neither read anything.
    Neither,
}

impl First {
    const ALL: [First; 8] = [
        First::Same,
        First::Differs,
        First::Nothing,
        First::PaddleAlone,
        First::LadderAlone,
        First::BothSame,
        First::BothDiffer,
        First::Neither,
    ];

    /// The name of a case worth looking at — the recogniser and the ladder apart, or the
    /// recogniser alone reading text — for its trace line and its picture; `None` when they agree.
    pub fn slug(self) -> Option<&'static str> {
        match self {
            First::Differs => Some("paddle-differs"),
            First::Nothing => Some("paddle-nothing"),
            First::PaddleAlone => Some("paddle-alone"),
            First::LadderAlone => Some("ladder-alone"),
            First::BothDiffer => Some("both-differ"),
            First::Same | First::BothSame | First::Neither => None,
        }
    }
}

/// How the pair the plan's c2 would read with — the fast level and the recogniser — stood to each
/// other and to the answer, where the fast level made its pass. "Apart" counts the recogniser
/// reading nothing where the fast level read text: under c2's rules that is a disagreement too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pair {
    /// Fast and the recogniser read the same, and so did the ladder.
    AgreeRight,
    /// Fast and the recogniser read the same, and the ladder something else, or nothing: the mistake
    /// the plan's c2 could not see.
    AgreeWrong,
    /// Fast and the recogniser apart, and the answer fast's.
    SplitFast,
    /// … the answer the recogniser's.
    SplitPaddle,
    /// … the answer neither's.
    SplitOther,
    /// … no answer.
    SplitSilent,
    /// Fast read nothing and the recogniser text, and the answer is the recogniser's.
    PaddleOnlySame,
    /// … another.
    PaddleOnlyOther,
    /// … nothing.
    PaddleOnlySilent,
    /// Neither read anything.
    BothNothing,
}

impl Pair {
    const ALL: [Pair; 10] = [
        Pair::AgreeRight,
        Pair::AgreeWrong,
        Pair::SplitFast,
        Pair::SplitPaddle,
        Pair::SplitOther,
        Pair::SplitSilent,
        Pair::PaddleOnlySame,
        Pair::PaddleOnlyOther,
        Pair::PaddleOnlySilent,
        Pair::BothNothing,
    ];

    /// As [`First::slug`]: every case but the two in which the fast level and the recogniser read
    /// the same as each other and as the answer, or nothing at all.
    pub fn slug(self) -> Option<&'static str> {
        match self {
            Pair::AgreeRight | Pair::BothNothing => None,
            Pair::AgreeWrong => Some("fast-paddle-wrong"),
            Pair::SplitFast => Some("split-fast"),
            Pair::SplitPaddle => Some("split-paddle"),
            Pair::SplitOther => Some("split-other"),
            Pair::SplitSilent => Some("split-silent"),
            Pair::PaddleOnlySame => Some("fast-none-paddle"),
            Pair::PaddleOnlyOther => Some("fast-none-other"),
            Pair::PaddleOnlySilent => Some("fast-none-silent"),
        }
    }
}

/// What one read came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    /// The recogniser was not asked.
    NotAsked,
    /// It had not answered when the ladder had.
    NotDone,
    /// It could not read the region ([`Paddle::Failed`]).
    Failed,
    /// The region was not the recogniser's to read; what it read is counted against the answer apart.
    Unfit,
    /// Compared: by what the first accurate pass read, and, where the fast level made its pass, by
    /// what fast and the recogniser read.
    Compared { first: First, fast: Option<Pair> },
}

impl Case {
    /// The name of a case worth looking at, for the trace line and the picture: the fast level's,
    /// the dangerous one first, then the first pass's; `None` when everything agreed.
    pub fn slug(self) -> Option<&'static str> {
        match self {
            Case::Compared { first, fast } => fast.and_then(Pair::slug).or_else(|| first.slug()),
            Case::NotAsked | Case::NotDone | Case::Failed | Case::Unfit => None,
        }
    }
}

/// The shadow's counts since the application started. One for the process, in
/// `backend/macos/ocr.rs`, behind a lock; a test makes its own.
#[derive(Clone, Debug, PartialEq)]
pub struct Tally {
    /// Small reads counted: every one that climbed the ladder with the recogniser loaded.
    pub reads: u64,
    /// Of those, the first accurate pass read nothing — and of these, it was refused.
    pub first_empty: u64,
    pub first_refused: u64,
    /// The ladder read the whole region.
    pub whole: u64,
    pub not_asked: u64,
    pub not_done: u64,
    pub failed: u64,
    /// Not the recogniser's to read, and of those, it read what the ladder answered.
    pub unfit: u64,
    pub unfit_same: u64,
    /// Per [`First`] case, in the order of [`First::ALL`].
    first: [u64; 8],
    /// Reads with a fast pass and an answer of the recogniser's, and per [`Pair`] case, in the order
    /// of [`Pair::ALL`].
    pub fast_reads: u64,
    fast: [u64; 10],
    /// The recogniser's milliseconds where it read text: their sum, how many, the most.
    pub paddle_ms: f64,
    pub paddle_texts: u64,
    pub paddle_max_ms: f64,
    /// Vision passes of the reads counted, and those the recogniser ran beside.
    pub passes: u64,
    pub passes_beside: u64,
}

/// A line with every count after this many reads, and again after as many more: counted, not timed,
/// so a session that reads little says little.
pub const LINE_EVERY: u64 = 200;

impl Tally {
    pub const fn new() -> Tally {
        Tally {
            reads: 0,
            first_empty: 0,
            first_refused: 0,
            whole: 0,
            not_asked: 0,
            not_done: 0,
            failed: 0,
            unfit: 0,
            unfit_same: 0,
            first: [0; 8],
            fast_reads: 0,
            fast: [0; 10],
            paddle_ms: 0.0,
            paddle_texts: 0,
            paddle_max_ms: 0.0,
            passes: 0,
            passes_beside: 0,
        }
    }

    /// How many reads came to `c`.
    pub fn first(&self, c: First) -> u64 {
        First::ALL.iter().position(|x| *x == c).map_or(0, |i| self.first[i])
    }

    /// How many reads with a fast pass came to `c`.
    pub fn fast(&self, c: Pair) -> u64 {
        Pair::ALL.iter().position(|x| *x == c).map_or(0, |i| self.fast[i])
    }

    /// Counts one read and says what it came to. A recogniser that was not asked, had not answered or
    /// could not read the region is counted as that whatever the region was; a region that was not its to read is compared only
    /// with the answer.
    pub fn add(&mut self, r: &Reading) -> Case {
        self.reads += 1;
        self.passes += r.passes as u64;
        self.passes_beside += r.passes_beside as u64;
        let first_read = matches!(r.first, Some(n) if n > 0);
        if !first_read {
            self.first_empty += 1;
            if r.first.is_none() {
                self.first_refused += 1;
            }
        }
        if r.whole {
            self.whole += 1;
        }
        let paddle = match &r.paddle {
            Paddle::NotAsked => {
                self.not_asked += 1;
                return Case::NotAsked;
            }
            Paddle::NotDone => {
                self.not_done += 1;
                return Case::NotDone;
            }
            Paddle::Failed => {
                self.failed += 1;
                return Case::Failed;
            }
            Paddle::Nothing => None,
            Paddle::Text { text, ms, .. } => {
                self.paddle_ms += ms;
                self.paddle_texts += 1;
                self.paddle_max_ms = self.paddle_max_ms.max(*ms);
                Some(text.trim()).filter(|t| !t.is_empty())
            }
        };
        let answer = Some(r.answer.trim()).filter(|a| !a.is_empty());
        if !r.fits {
            self.unfit += 1;
            if matches!((paddle, answer), (Some(p), Some(a)) if same(p, a)) {
                self.unfit_same += 1;
            }
            return Case::Unfit;
        }

        let first = if first_read {
            match (paddle, answer) {
                (Some(p), Some(a)) if same(p, a) => First::Same,
                (Some(_), _) => First::Differs,
                (None, _) => First::Nothing,
            }
        } else {
            match (answer, paddle) {
                (None, Some(_)) => First::PaddleAlone,
                (Some(_), None) => First::LadderAlone,
                (Some(a), Some(p)) if same(a, p) => First::BothSame,
                (Some(_), Some(_)) => First::BothDiffer,
                (None, None) => First::Neither,
            }
        };
        if let Some(i) = First::ALL.iter().position(|x| *x == first) {
            self.first[i] += 1;
        }

        let fast = r.fast.as_deref().map(|f| {
            let f = Some(f.trim()).filter(|f| !f.is_empty());
            let is = |a: Option<&str>, b: &str| a.is_some_and(|a| same(a, b));
            match (f, paddle) {
                (Some(f), Some(p)) if same(f, p) => {
                    if is(answer, f) {
                        Pair::AgreeRight
                    } else {
                        Pair::AgreeWrong
                    }
                }
                (None, None) => Pair::BothNothing,
                (None, Some(p)) => match answer {
                    None => Pair::PaddleOnlySilent,
                    Some(a) if same(a, p) => Pair::PaddleOnlySame,
                    Some(_) => Pair::PaddleOnlyOther,
                },
                (Some(f), p) => match answer {
                    None => Pair::SplitSilent,
                    Some(a) if same(a, f) => Pair::SplitFast,
                    Some(a) if p.is_some_and(|p| same(a, p)) => Pair::SplitPaddle,
                    Some(_) => Pair::SplitOther,
                },
            }
        });
        if let Some(c) = fast {
            self.fast_reads += 1;
            if let Some(i) = Pair::ALL.iter().position(|x| *x == c) {
                self.fast[i] += 1;
            }
        }
        Case::Compared { first, fast }
    }

    /// [`Tally::line`] when the reads counted have just reached a multiple of [`LINE_EVERY`].
    pub fn due(&self) -> Option<String> {
        (self.reads > 0 && self.reads.is_multiple_of(LINE_EVERY)).then(|| self.line())
    }

    /// Every count, as one line of the log.
    pub fn line(&self) -> String {
        if self.reads == 0 {
            return "ocr: the neural recogniser's shadow: no small read was counted".to_string();
        }
        let pct = |n: u64| format!("{:.0} %", n as f64 * 100.0 / self.reads as f64);
        let f = |c: First| self.first(c);
        let q = |c: Pair| self.fast(c);
        let apart = q(Pair::SplitFast) + q(Pair::SplitPaddle) + q(Pair::SplitOther) + q(Pair::SplitSilent);
        let alone = q(Pair::PaddleOnlySame) + q(Pair::PaddleOnlyOther) + q(Pair::PaddleOnlySilent);
        let time = if self.paddle_texts == 0 {
            "Paddle read no text".to_string()
        } else {
            format!(
                "Paddle took {:.0} ms on average where it read text, {:.0} at most",
                self.paddle_ms / self.paddle_texts as f64,
                self.paddle_max_ms
            )
        };
        format!(
            "ocr: the neural recogniser's shadow (Paddle) after {} small reads: the first accurate pass read \
             nothing in {} ({}, refused in {}), the whole region was read in {}; Paddle was not asked in {}, could \
             not read the region in {}, had not answered when the ladder had in {}, and {} were not its to read (it read the answer in {}). Where \
             the first pass read text: Paddle the same in {}, something else in {}, nothing in {}; where it read \
             nothing: Paddle alone read text in {}, the ladder alone in {}, both the same in {}, both differently \
             in {}, neither in {}. With a fast pass ({}): fast = Paddle = the answer in {}, fast = Paddle but not \
             the answer in {}, fast and Paddle apart in {} (the answer fast's in {}, Paddle's in {}, another in {}, \
             nothing in {}), fast nothing and Paddle text in {} (the answer Paddle's in {}, another in {}, nothing \
             in {}), both nothing in {}. {time}; Vision passes with Paddle beside them: {} of {}",
            self.reads,
            self.first_empty,
            pct(self.first_empty),
            self.first_refused,
            self.whole,
            self.not_asked,
            self.failed,
            self.not_done,
            self.unfit,
            self.unfit_same,
            f(First::Same),
            f(First::Differs),
            f(First::Nothing),
            f(First::PaddleAlone),
            f(First::LadderAlone),
            f(First::BothSame),
            f(First::BothDiffer),
            f(First::Neither),
            self.fast_reads,
            q(Pair::AgreeRight),
            q(Pair::AgreeWrong),
            apart,
            q(Pair::SplitFast),
            q(Pair::SplitPaddle),
            q(Pair::SplitOther),
            q(Pair::SplitSilent),
            alone,
            q(Pair::PaddleOnlySame),
            q(Pair::PaddleOnlyOther),
            q(Pair::PaddleOnlySilent),
            q(Pair::BothNothing),
            self.passes_beside,
            self.passes,
        )
    }
}

impl Default for Tally {
    fn default() -> Tally {
        Tally::new()
    }
}

/// The trace line of a read the shadow found worth looking at ([`Case::slug`]): the case, the
/// region in points, and every reading; `None` for the others.
pub fn trace_line(r: &Reading, case: Case, (x, y, w, h): (i32, i32, i32, i32)) -> Option<String> {
    let slug = case.slug()?;
    let quoted = |t: &str| if t.trim().is_empty() { "nothing".to_string() } else { format!("\"{}\"", t.trim()) };
    let fast = match &r.fast {
        Some(t) => format!("the fast level {}", quoted(t)),
        None => "no fast pass".to_string(),
    };
    let paddle = match &r.paddle {
        Paddle::Text { text, score, ms } => format!("Paddle {} (score {score:.3}, {ms:.0} ms)", quoted(text)),
        Paddle::Nothing => "Paddle nothing".to_string(),
        Paddle::NotDone => "Paddle not done".to_string(),
        Paddle::Failed => "Paddle could not read it".to_string(),
        Paddle::NotAsked => "Paddle not asked".to_string(),
    };
    let first = match r.first {
        None => "refused".to_string(),
        Some(0) => "nothing".to_string(),
        Some(1) => "1 word".to_string(),
        Some(n) => format!("{n} words"),
    };
    Some(format!(
        "ocr: shadow, {slug}, a {w}x{h} pt region at {x},{y}: the answer {}, the first accurate pass {first}, {fast}, \
         {paddle}",
        quoted(&r.answer)
    ))
}

/// A picture's file name: `ocr-<kind>-<x>,<y>,<w>x<h>-<hash>.bmp`, the region in points and the
/// hash of its pixels ([`pixels_hash`]) in eight hexadecimal digits. `kind` is `debug-raw` for what
/// the switch "Save the images OCR was given" keeps of every small region, `shadow-<case>` for a
/// read the shadow found worth looking at.
pub fn picture_name(kind: &str, (x, y, w, h): (i32, i32, i32, i32), hash: u32) -> String {
    format!("ocr-{kind}-{x},{y},{w}x{h}-{hash:08x}.bmp")
}

/// FNV-1a over the pixels, 32 bits: which content a picture of a region is. Tens of microseconds for
/// a small region; asked only while the switch is on.
pub fn pixels_hash(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in bytes {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// The pictures of one region and content a session writes at most: a field that keeps changing —
/// a meter, a counter — would otherwise write a file on every read.
pub const PER_REGION: usize = 8;

/// The regions a session keeps pictures of at most: a module that reads at positions that move — a
/// list row by row, a cursor — would otherwise fill the folder.
pub const MAX_REGIONS: usize = 64;

/// The bytes one list of pictures writes in a session at most, files counted whole: 32 MiB, so
/// the two lists of `backend/macos/ocr.rs` write 64 MiB a session at most. The counts alone would
/// allow 512 files a list, and a picture of a 400x200 point region on a Retina display is 1.3 MB.
pub const MAX_BYTES: usize = 32 << 20;

/// What [`Pictures::admit`] decided for one picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picture {
    /// A new content of this region: write it under this name.
    Write(String),
    /// This content of this region was written already.
    Seen,
    /// [`PER_REGION`] pictures of this region were written; `say`: for the first time, so the log
    /// says no more will be.
    Full { say: bool },
    /// [`MAX_REGIONS`] regions have pictures and this is another; `say` as above, once a session.
    Regions { say: bool },
    /// [`MAX_BYTES`] would be passed; `say` as above, once a session.
    Bytes { say: bool },
}

/// The pictures written this session, per region: each content once, [`PER_REGION`] a region,
/// [`MAX_REGIONS`] regions and [`MAX_BYTES`] in all. One for the debug pictures and one for the
/// shadow's, in `backend/macos/ocr.rs`.
#[derive(Debug, Default)]
pub struct Pictures {
    regions: Vec<((i32, i32, i32, i32), Kept)>,
    regions_said: bool,
    bytes: usize,
    bytes_said: bool,
}

#[derive(Debug, Default)]
struct Kept {
    hashes: Vec<u32>,
    full_said: bool,
}

impl Pictures {
    pub const fn new() -> Pictures {
        Pictures { regions: Vec::new(), regions_said: false, bytes: 0, bytes_said: false }
    }

    /// Whether a picture of `region` with content `hash`, a file of `bytes`, is written, and under
    /// which name, and remembers it when it is.
    pub fn admit(&mut self, kind: &str, region: (i32, i32, i32, i32), hash: u32, bytes: usize) -> Picture {
        let at = match self.regions.iter().position(|(r, _)| *r == region) {
            Some(i) => i,
            None if self.regions.len() >= MAX_REGIONS => {
                let say = !std::mem::replace(&mut self.regions_said, true);
                return Picture::Regions { say };
            }
            None => {
                self.regions.push((region, Kept::default()));
                self.regions.len() - 1
            }
        };
        let kept = &mut self.regions[at].1;
        if kept.hashes.contains(&hash) {
            return Picture::Seen;
        }
        if kept.hashes.len() >= PER_REGION {
            let say = !std::mem::replace(&mut kept.full_said, true);
            return Picture::Full { say };
        }
        if self.bytes.saturating_add(bytes) > MAX_BYTES {
            let say = !std::mem::replace(&mut self.bytes_said, true);
            return Picture::Bytes { say };
        }
        self.bytes += bytes;
        kept.hashes.push(hash);
        Picture::Write(picture_name(kind, region, hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(t: &str) -> Paddle {
        Paddle::Text { text: t.to_string(), score: 0.9, ms: 12.0 }
    }

    /// A read that fits, with one pass that read `first` words, answered `answer`, with no fast pass.
    fn read(first: Option<usize>, answer: &str, paddle: Paddle) -> Reading {
        Reading {
            first,
            answer: answer.to_string(),
            whole: false,
            fast: None,
            paddle,
            fits: true,
            passes: 1,
            passes_beside: 0,
        }
    }

    fn with_fast(fast: &str, answer: &str, paddle: Paddle) -> Reading {
        Reading { fast: Some(fast.to_string()), ..read(Some(1), answer, paddle) }
    }

    #[test]
    fn where_the_first_pass_read_text_the_recogniser_is_held_against_it() {
        let mut t = Tally::new();
        assert_eq!(t.add(&read(Some(1), "64", text("64"))), Case::Compared { first: First::Same, fast: None });
        // Spaces and a typographic minus are the same reading; O and 0 are not.
        assert_eq!(t.add(&read(Some(2), "0.00 dB", text("0.00dB"))).slug(), None);
        assert_eq!(t.add(&read(Some(1), "-12", text("\u{2212}12"))).slug(), None);
        assert_eq!(t.add(&read(Some(1), "0", text("O"))), Case::Compared { first: First::Differs, fast: None });
        assert_eq!(t.add(&read(Some(1), "1", Paddle::Nothing)), Case::Compared { first: First::Nothing, fast: None });
        assert_eq!((t.first(First::Same), t.first(First::Differs), t.first(First::Nothing)), (3, 1, 1));
        assert_eq!(t.first_empty, 0);
        assert_eq!(t.reads, 5);
    }

    #[test]
    fn where_the_first_pass_read_nothing_the_five_cases() {
        let mut t = Tally::new();
        let miss = |answer: &str, paddle: Paddle| Reading { whole: true, ..read(Some(0), answer, paddle) };
        let case = |c: Case| match c {
            Case::Compared { first, fast: None } => first,
            other => panic!("{other:?}"),
        };
        assert_eq!(case(t.add(&miss("", text("1")))), First::PaddleAlone, "a digit gained, or an invention");
        assert_eq!(case(t.add(&miss("1", Paddle::Nothing))), First::LadderAlone);
        assert_eq!(case(t.add(&miss("1", text("1")))), First::BothSame);
        assert_eq!(case(t.add(&miss("7", text("1")))), First::BothDiffer);
        assert_eq!(case(t.add(&miss("", Paddle::Nothing))), First::Neither);
        // Refused counts as having read nothing, and is said apart.
        assert_eq!(case(t.add(&read(None, "", text("1")))), First::PaddleAlone);
        assert_eq!((t.first_empty, t.first_refused, t.whole), (6, 1, 5));
        assert_eq!(t.first(First::PaddleAlone), 2);
    }

    #[test]
    fn with_a_fast_pass_every_way_the_three_can_stand() {
        let mut t = Tally::new();
        let fast = |c: Case| match c {
            Case::Compared { fast: Some(f), .. } => f,
            other => panic!("{other:?}"),
        };
        assert_eq!(fast(t.add(&with_fast("64", "64", text("64")))), Pair::AgreeRight);
        assert_eq!(fast(t.add(&with_fast("-12", "12", text("-12")))), Pair::AgreeWrong, "both lost nothing the ladder kept");
        assert_eq!(fast(t.add(&with_fast("84", "", text("84")))), Pair::AgreeWrong, "the ladder read nothing at all");
        assert_eq!(fast(t.add(&with_fast("64", "64", text("84")))), Pair::SplitFast);
        assert_eq!(fast(t.add(&with_fast("84", "64", text("64")))), Pair::SplitPaddle);
        assert_eq!(fast(t.add(&with_fast("84", "64", text("34")))), Pair::SplitOther);
        assert_eq!(fast(t.add(&with_fast("84", "", text("34")))), Pair::SplitSilent);
        // The recogniser reading nothing where fast read text is a disagreement under c2's rules.
        assert_eq!(fast(t.add(&with_fast("64", "64", Paddle::Nothing))), Pair::SplitFast);
        assert_eq!(fast(t.add(&with_fast("64", "", Paddle::Nothing))), Pair::SplitSilent);
        assert_eq!(fast(t.add(&with_fast("", "1", text("1")))), Pair::PaddleOnlySame);
        assert_eq!(fast(t.add(&with_fast("", "7", text("1")))), Pair::PaddleOnlyOther);
        assert_eq!(fast(t.add(&with_fast(" ", "", text("1")))), Pair::PaddleOnlySilent);
        assert_eq!(fast(t.add(&with_fast("", "", Paddle::Nothing))), Pair::BothNothing);
        assert_eq!(t.fast_reads, 13);
        assert_eq!((t.fast(Pair::AgreeWrong), t.fast(Pair::SplitFast), t.fast(Pair::SplitSilent)), (2, 2, 2));
    }

    #[test]
    fn late_unasked_and_unfit_reads_are_counted_apart() {
        let mut t = Tally::new();
        assert_eq!(t.add(&read(Some(1), "64", Paddle::NotDone)), Case::NotDone);
        assert_eq!(t.add(&read(Some(0), "", Paddle::NotAsked)), Case::NotAsked);
        assert_eq!(t.add(&read(Some(0), "", Paddle::Failed)), Case::Failed, "a panic or a thread gone is no reading");
        let unfit = |answer: &str, paddle: Paddle| Reading { fits: false, ..read(Some(3), answer, paddle) };
        assert_eq!(t.add(&unfit("Voices: 0", text("Voices:0"))), Case::Unfit);
        assert_eq!(t.add(&unfit("Voices: 0", text("In Voices: 0"))), Case::Unfit);
        assert_eq!(t.add(&unfit("Legato", Paddle::Nothing)), Case::Unfit);
        // A late recogniser on a region that was not its to read is late first.
        assert_eq!(t.add(&Reading { fits: false, ..read(Some(1), "x", Paddle::NotDone) }), Case::NotDone);
        assert_eq!((t.not_done, t.not_asked, t.failed, t.unfit, t.unfit_same), (2, 1, 1, 3, 1));
        assert_eq!((t.reads, t.first_empty), (7, 2));
        assert!(First::ALL.iter().all(|c| t.first(*c) == 0), "nothing was compared");
        for c in [Case::NotDone, Case::NotAsked, Case::Failed, Case::Unfit] {
            assert_eq!(c.slug(), None);
        }
    }

    #[test]
    fn the_cases_worth_a_look_and_their_names() {
        let compared = |first, fast| Case::Compared { first, fast };
        assert_eq!(compared(First::Same, Some(Pair::AgreeRight)).slug(), None);
        assert_eq!(compared(First::Neither, Some(Pair::BothNothing)).slug(), None);
        assert_eq!(compared(First::BothSame, None).slug(), None);
        assert_eq!(compared(First::PaddleAlone, None).slug(), Some("paddle-alone"));
        // The fast level's case before the first pass's: the dangerous one is named first.
        assert_eq!(compared(First::Differs, Some(Pair::AgreeWrong)).slug(), Some("fast-paddle-wrong"));
        assert_eq!(compared(First::Nothing, Some(Pair::BothNothing)).slug(), Some("paddle-nothing"));
        let mut names = std::collections::HashSet::new();
        for c in First::ALL.iter().filter_map(|c| c.slug()).chain(Pair::ALL.iter().filter_map(|c| c.slug())) {
            assert!(c.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'), "{c} goes into a file name");
            assert!(names.insert(c), "{c} twice");
        }
        assert_eq!(names.len(), 13);
    }

    #[test]
    fn the_line_says_every_count() {
        let mut t = Tally::new();
        assert_eq!(t.line(), "ocr: the neural recogniser's shadow: no small read was counted");
        t.add(&Reading { passes: 2, passes_beside: 1, ..with_fast("64", "64", text("64")) });
        t.add(&Reading { whole: true, ..read(Some(0), "", Paddle::Text { text: "1".into(), score: 0.5, ms: 30.0 }) });
        t.add(&read(Some(1), "x", Paddle::NotDone));
        t.add(&Reading { fits: false, ..read(Some(2), "a b", Paddle::Nothing) });
        assert_eq!(
            t.line(),
            "ocr: the neural recogniser's shadow (Paddle) after 4 small reads: the first accurate pass read nothing \
             in 1 (25 %, refused in 0), the whole region was read in 1; Paddle was not asked in 0, could not read the \
             region in 0, had not answered when the ladder had in 1, and 1 were not its to read (it read the answer in 0). Where the first pass \
             read text: Paddle the same in 1, something else in 0, nothing in 0; where it read nothing: Paddle alone \
             read text in 1, the ladder alone in 0, both the same in 0, both differently in 0, neither in 0. With a \
             fast pass (1): fast = Paddle = the answer in 1, fast = Paddle but not the answer in 0, fast and Paddle \
             apart in 0 (the answer fast's in 0, Paddle's in 0, another in 0, nothing in 0), fast nothing and \
             Paddle text in 0 (the answer Paddle's in 0, another in 0, nothing in 0), both nothing in 0. Paddle \
             took 21 ms on average where it read text, 30 at most; Vision passes with Paddle beside them: 1 of 5"
        );
    }

    #[test]
    fn the_line_is_due_every_two_hundred_reads() {
        let mut t = Tally::new();
        assert_eq!(t.due(), None);
        for n in 1..=(2 * LINE_EVERY + 1) {
            t.add(&read(Some(1), "64", text("64")));
            assert_eq!(t.due().is_some(), n % LINE_EVERY == 0, "after {n}");
        }
    }

    #[test]
    fn a_disagreement_is_traced_with_every_reading() {
        let r = with_fast("84", "64", Paddle::Text { text: "84".into(), score: 0.912, ms: 11.6 });
        let mut t = Tally::new();
        let case = t.add(&r);
        assert_eq!(
            trace_line(&r, case, (100, 200, 40, 20)).unwrap(),
            "ocr: shadow, fast-paddle-wrong, a 40x20 pt region at 100,200: the answer \"64\", the first accurate \
             pass 1 word, the fast level \"84\", Paddle \"84\" (score 0.912, 12 ms)"
        );
        let r = read(Some(0), "", text("1"));
        let case = t.add(&r);
        assert_eq!(
            trace_line(&r, case, (-1440, 0, 12, 20)).unwrap(),
            "ocr: shadow, paddle-alone, a 12x20 pt region at -1440,0: the answer nothing, the first accurate pass \
             nothing, no fast pass, Paddle \"1\" (score 0.900, 12 ms)"
        );
        let agreed = read(Some(1), "64", text("64"));
        let case = t.add(&agreed);
        assert_eq!(trace_line(&agreed, case, (0, 0, 1, 1)), None, "nothing to look at");
    }

    #[test]
    fn a_picture_is_named_by_its_region_and_content() {
        assert_eq!(picture_name("debug-raw", (100, -20, 40, 20), 0xab), "ocr-debug-raw-100,-20,40x20-000000ab.bmp");
        assert_eq!(
            picture_name("shadow-paddle-alone", (0, 0, 12, 20), 0xdead_beef),
            "ocr-shadow-paddle-alone-0,0,12x20-deadbeef.bmp"
        );
        // FNV-1a's published values: the empty input is the offset basis, "a" is e40c292c.
        assert_eq!(pixels_hash(b""), 0x811c_9dc5);
        assert_eq!(pixels_hash(b"a"), 0xe40c_292c);
        assert_ne!(pixels_hash(&[0, 0, 0, 255]), pixels_hash(&[0, 0, 1, 255]));
    }

    #[test]
    fn each_content_once_and_at_most_eight_a_region() {
        let mut p = Pictures::new();
        let field = (100, 200, 40, 20);
        assert_eq!(p.admit("debug-raw", field, 1, 100), Picture::Write(picture_name("debug-raw", field, 1)));
        assert_eq!(p.admit("debug-raw", field, 1, 100), Picture::Seen, "the same content again");
        for h in 2..=PER_REGION as u32 {
            assert!(matches!(p.admit("debug-raw", field, h, 100), Picture::Write(_)), "{h}");
        }
        assert_eq!(p.admit("debug-raw", field, 99, 100), Picture::Full { say: true });
        assert_eq!(p.admit("debug-raw", field, 100, 100), Picture::Full { say: false }, "said once");
        assert_eq!(p.admit("debug-raw", field, 3, 100), Picture::Seen, "a content kept is still known");
        assert!(matches!(p.admit("debug-raw", field, 99, 100), Picture::Full { .. }), "refused, so not kept");
        // Another region has its own eight.
        assert!(matches!(p.admit("debug-raw", (100, 230, 40, 20), 99, 100), Picture::Write(_)));
    }

    #[test]
    fn at_most_sixty_four_regions() {
        let mut p = Pictures::new();
        for i in 0..MAX_REGIONS as i32 {
            assert!(matches!(p.admit("shadow-paddle-alone", (i, 0, 10, 10), 7, 100), Picture::Write(_)));
        }
        assert_eq!(p.admit("shadow-paddle-alone", (-1, 0, 10, 10), 7, 100), Picture::Regions { say: true });
        assert_eq!(p.admit("shadow-paddle-alone", (-2, 0, 10, 10), 7, 100), Picture::Regions { say: false });
        assert!(matches!(p.admit("shadow-paddle-alone", (0, 0, 10, 10), 8, 100), Picture::Write(_)), "a region it has");
    }

    /// The counts would allow 512 pictures of up to 1.3 MB; the bytes stop it first.
    #[test]
    fn at_most_thirty_two_mebibytes_a_list() {
        let mut p = Pictures::new();
        let big = 1_280_054; // a 400x200 pt region at 2x, as a BMP
        let fit = MAX_BYTES / big;
        for i in 0..fit {
            let region = ((i / PER_REGION) as i32, 0, 400, 200);
            assert!(matches!(p.admit("debug-raw", region, i as u32, big), Picture::Write(_)), "{i}");
        }
        assert_eq!(p.admit("debug-raw", (63, 0, 400, 200), 9999, big), Picture::Bytes { say: true });
        assert_eq!(p.admit("debug-raw", (63, 0, 400, 200), 9998, big), Picture::Bytes { say: false }, "said once");
        assert!(matches!(p.admit("debug-raw", (63, 0, 400, 200), 9997, MAX_BYTES - fit * big), Picture::Write(_)), "what still fits");
        assert!(fit < MAX_REGIONS * PER_REGION);
    }
}
