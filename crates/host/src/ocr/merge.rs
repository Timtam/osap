//! How a small region's readings become its answer where the neural recogniser (PaddleOCR,
//! `backend/paddle_ocr.rs`) reads beside the system recogniser: Windows' rule, which a Mac mirrors
//! with Vision in `Windows.Media.Ocr`'s place, and the check an Intel Mac's fast level gets from
//! the neural recogniser. The code that reads by it on a Mac is `backend/macos/ocr.rs`
//! (`Rungs::merged`); Windows' own is `backend/windows.rs`, `recognize_image`, which this file
//! does not change and a test below holds to the rule as written here.
//!
//! **Windows' rule** (`recognize_image`):
//!
//! 1. A region of 400x200 or less (`policy::is_small`) is handed to the neural recogniser
//!    (`paddle_ocr::ask`) the moment its pixels are in hand, before `Windows.Media.Ocr` starts on
//!    it, so that the two read side by side: a miss costs the slower of the two, not their sum. A
//!    larger region never is. Not asked either once the exit's wait has begun, while 32 regions
//!    wait for the recogniser, or when its thread has gone: the system recogniser then reads alone.
//! 2. A small region whose content crop finds no ink is answered nothing (`skipped`) without either
//!    recogniser's answer, and the neural recognition is cancelled.
//! 3. When the system recogniser fails, the read fails (`OCR failed: …`) before the merge, and the
//!    neural recognition is cancelled unread: a failure is no reading of nothing ([`Pick::Refused`]).
//! 4. When the system recogniser read anything — its text, trimmed, not empty — that is the answer,
//!    its words and lines with it, and the neural recognition is cancelled unread ([`pick`]).
//! 5. When it read nothing, the read waits for the neural answer, as long as that recognition
//!    takes ([`waits`]); its text, trimmed and never empty, is the answer, with no words and no lines
//!    and the content crop as the answer's box (`OcrText::fallback`), which `host.ocr.recognize` shares
//!    out among the text's tokens as approximate boxes. When it read nothing too, so does the read.
//! 6. Nothing filters what it reads: no minimum score — none separates a right lone "0" from text
//!    it invents on ink that is no text (`paddle_pre::decode`) — and no check of its shape. A
//!    single glyph is read as any other text.
//! 7. With the switch "Save the images OCR was given" or trace logging on, one log line a region:
//!    `<w>x<h> tighten <ms>ms winrt <ms>ms + waited <ms>ms for paddle (which answered|which had
//!    nothing) -> '<text>'`, the wait only when there was one.
//!
//! **A Mac's mirror**, wherever the recogniser has loaded (`ladder::Shape::production`): the same
//! seven, with Vision's accurate ladder — the tight crop, the whole region, the enlarged crop within
//! the ladder's budget — in `Windows.Media.Ocr`'s place, and so without the fast rung that ends the
//! ladder where the recogniser is not there: Windows has no such rung, and on CI's Macs it read
//! "o" for sforzando's "0" and invented text on a level meter before the recogniser could answer
//! (`ocr-bench`, 2026-10-03). Mapped where Windows' rule leans on what a Mac does not have:
//!
//! - **When it is asked.** At the moment the read has its content crop, which is what it is handed
//!   (`paddle_crop`: a Mac's region holds more surroundings than a Windows one, and the crop finds
//!   a well's inside), right after the blank guard rather than before it: Windows cancels a blank
//!   region's recognition the moment its crop finds it blank, so the answers are the same, and a
//!   Mac does not start one to cancel.
//! - **Only once it is ready.** A Mac reads with Vision alone, its fast rung included, until the
//!   recogniser's warm-up has made it (`paddle_ocr::ready`), where Windows' first region makes it.
//!   The application's first seconds are Vision's warm-up anyway.
//! - **A failure.** Vision refusing the first accurate pass is a Mac's failure of the system
//!   recogniser. A read on a Mac has always answered a refusal with nothing rather than an error
//!   (`docs/api/ocr.md`), and still does; the recogniser's answer is not used for it, as Windows
//!   uses none for a failed read.
//! - **How long a read waits.** Within the ladder's budget, not for as long as it takes ([`left`]):
//!   a Mac's event loop carries the keyboard's event tap, which macOS switches off when the loop
//!   stops answering, and that budget is what keeps the ladder from holding it. The recogniser
//!   began when the ladder did, so it has had every pass of the ladder to answer in. One that has
//!   not answered by then answers nothing, and the log says so. A read asked from a poll that made
//!   way for one asked from a key press, which skips the ladder's last rungs, does not wait at all:
//!   it takes an answer that is there already, or none.
//! - **At whose priority.** At the reading thread's own quality of service (`paddle_ocr::Qos::
//!   Reader`): the event loop's, or the recognise thread's. Windows has one priority for all.
//! - **The log.** The read's own line (`macos/ocr.rs`, `recognize_noted`), written as Windows'
//!   is — at line level with the switch on, at trace level otherwise — says who answered
//!   ([`super::ladder::Path`]) and, in Windows' words, `+ waited <ms>ms for paddle (which
//!   answered|which had nothing)`, with `which had not answered by then` for a wait the budget
//!   ended; an Intel Mac's check has a fragment of its own before it (`cost::CheckWait`).
//!
//! **An Intel Mac's check** (`ladder::PaddleUse::FastMerge`): Vision there has no Neural Engine, and
//! one accurate pass over a value field cost 220–470 ms on CI's Intel Mac, over a wider field of
//! words up to 1.3 s, against 11–27 ms for the fast level. So for a region whose ink is one line no
//! wider than twelve times its height (`paddle_pre::fits`), and a language the fast level reads,
//! one fast pass over the tight crop goes first, the recogniser beside it; where the two read the
//! same (`paddle_pre::same`), and the recogniser reads a space wherever the fast level does, that is
//! the answer, with the fast level's words and the recogniser's spacing ([`check`]); otherwise the
//! mirror above, from its first rung. The read waits for the check no longer than the fast pass
//! took, and never past the budget ([`check_wait`]) — a poll's read too, since a check that fails
//! costs the accurate ladder, the dearest thing a waiting key press could be made to wait for.
//!
//! Pure, so it is tested on every platform; `crates/macos-check` borrows it.

// Everywhere but on a Mac only the tests use it.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::time::Duration;

use super::paddle_pre::same;
use crate::backend::{OcrLine, OcrWord};

/// What answers a region by Windows' rule (`windows.rs`, `recognize_image`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick<'a> {
    /// The system recogniser's reading, words and lines with it: it read something.
    System,
    /// The neural recogniser's text, trimmed: the system recogniser read nothing, and it did.
    Paddle(&'a str),
    /// Neither read anything.
    Nothing,
    /// The system recogniser failed — on a Mac, Vision refused the first accurate pass — which is
    /// no reading of nothing: Windows' read fails then, before the merge, and a Mac's answers
    /// nothing, as it always has; the neural recogniser's answer is not used either way.
    Refused,
}

/// Whether a read waits for the neural recogniser's answer: only when the system recogniser read
/// nothing (`system`, its whole text; `None` when it failed). Otherwise the recognition is
/// cancelled unread.
pub fn waits(system: Option<&str>) -> bool {
    system.is_some_and(|s| s.trim().is_empty())
}

/// Windows' rule: the system recogniser's reading when it read anything, the neural recogniser's
/// text (`paddle`, its answer if it was asked and answered) when it did not, and nothing when
/// neither read anything — or when the system recogniser failed (`system` `None`). The neural
/// text answers as it is, whatever it is.
pub fn pick<'a>(system: Option<&str>, paddle: Option<&'a str>) -> Pick<'a> {
    let Some(system) = system else { return Pick::Refused };
    if !waits(Some(system)) {
        return Pick::System;
    }
    match paddle.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Pick::Paddle(p),
        None => Pick::Nothing,
    }
}

/// What is left of the ladder's `budget` once a read has `spent` this much of it: how long it may
/// still wait for the neural recogniser. Nothing once the budget is spent — the read then takes
/// the answer only if it is already there.
pub fn left(budget: Duration, spent: Duration) -> Duration {
    budget.saturating_sub(spent)
}

/// How long an Intel Mac's read waits for the neural recogniser's check once its fast pass, which
/// took `fast`, has answered: no longer than that pass took — the recogniser began with it, and on
/// CI's Macs a small field took it 4–25 ms against the fast pass's 11–27 ms — and no further than
/// what is `left` of the budget. So the check costs at most twice the fast pass, a tenth of one
/// accurate pass on CI's Intel Mac, before the accurate ladder takes over where it fails.
pub fn check_wait(left: Duration, fast: Duration) -> Duration {
    left.min(fast)
}

/// What an Intel Mac's check came to: whether the fast level and the neural recogniser read the
/// same (`None` when neither read anything), and the answer where they did.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub agreed: Option<bool>,
    /// The fast level's lines with the recogniser's spacing ([`respaced`]).
    pub answer: Option<Vec<OcrLine>>,
}

/// The check of the fast level's reading (`fast`, its lines; `None` when Vision refused the pass)
/// by the neural recogniser's (`paddle`, its text if it answered in time): the same where the two
/// read the same characters and the recogniser reads a space wherever the fast level does
/// ([`spaces_to_add`]), and then the fast level's words with the recogniser's spacing. Anything
/// else is no agreement — one reading nothing where the other reads text included — and leaves
/// the read to the accurate ladder.
pub fn check(fast: Option<&[OcrLine]>, paddle: Option<&str>) -> Check {
    let fast = fast.filter(|lines| lines.iter().any(|l| !l.text.trim().is_empty()));
    let paddle = paddle.map(str::trim).filter(|p| !p.is_empty());
    match (fast, paddle) {
        (None, None) => Check { agreed: None, answer: None },
        (Some(lines), Some(p)) => {
            let answer = respaced(lines, p);
            Check { agreed: Some(answer.is_some()), answer }
        }
        _ => Check { agreed: Some(false), answer: None },
    }
}

/// The places the neural recogniser reads a space and `fast` reads none: the indices, among
/// `fast`'s characters other than white space, of those a space goes before. `None` when the two
/// do not read the same (`paddle_pre::same`), and when `fast` reads a space where the recogniser
/// reads none: then neither spacing is known to be right, and taking either would speak a value
/// one of the two did not read (`1 23` against `12 3` would become `1 2 3`). White space before
/// the first character or after the last is no place, and a run of it is one.
pub fn spaces_to_add(fast: &str, paddle: &str) -> Option<Vec<usize>> {
    if !same(fast, paddle) {
        return None;
    }
    // Indices `k` with white space between the characters `k - 1` and `k`.
    let gaps = |s: &str| {
        let mut out = Vec::new();
        let (mut k, mut gap) = (0usize, false);
        for c in s.chars() {
            if c.is_whitespace() {
                gap = k > 0;
            } else {
                if gap {
                    out.push(k);
                }
                gap = false;
                k += 1;
            }
        }
        out
    };
    let (have, theirs) = (gaps(fast), gaps(paddle));
    if !have.iter().all(|k| theirs.contains(k)) {
        return None;
    }
    Some(theirs.into_iter().filter(|k| !have.contains(k)).collect())
}

/// The fast level's reading — its lines, each with its words, as Vision gave them — with the
/// spaces the neural recogniser reads where the fast level reads none ([`spaces_to_add`]): the
/// spacing the recogniser's, every character the fast level's own (so a minus or a quote keeps its
/// form). A word the space falls in becomes two, its box shared between them by their characters,
/// the height kept. `None` when the two do not read the same, characters or spaces — no
/// agreement. A reading whose words are not its lines' characters, which Vision's are by
/// construction (`perform`), comes back as it is.
///
/// Measured where it matters: on CI's three Macs (2026-10-03) the fast level read `-0.5 dB` and
/// `+3 ct` at 2x as `-0.5dB` and `+3ct`, the recogniser with their spaces, and on every other
/// picture the two agreed on they had the same spaces — so this changes those two and no other,
/// and no agreement of that run fails on spaces.
pub fn respaced(lines: &[OcrLine], paddle: &str) -> Option<Vec<OcrLine>> {
    let text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    let add = spaces_to_add(&text, paddle)?;
    let chars = |s: &str| s.chars().filter(|c| !c.is_whitespace()).count();
    let consistent = lines.iter().all(|l| chars(&l.text) == l.words.iter().map(|w| w.text.chars().count()).sum::<usize>())
        && lines.iter().flat_map(|l| &l.words).all(|w| !w.text.is_empty() && chars(&w.text) == w.text.chars().count());
    if add.is_empty() || !consistent {
        return Some(lines.to_vec());
    }
    let mut at = 0usize; // the index of the next character, over every line
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let mut text = String::with_capacity(line.text.len() + add.len());
        let mut k = at;
        for c in line.text.chars() {
            if !c.is_whitespace() {
                if add.contains(&k) {
                    text.push(' ');
                }
                k += 1;
            }
            text.push(c);
        }
        let mut words = Vec::with_capacity(line.words.len() + add.len());
        for w in &line.words {
            let n = w.text.chars().count();
            let cuts: Vec<usize> = add.iter().filter(|k| **k > at && **k < at + n).map(|k| k - at).collect();
            words.extend(split_word(w, &cuts));
            at += n;
        }
        out.push(OcrLine { text, words });
    }
    Some(out)
}

/// `w` cut before each of its characters at `cuts` (ascending, inside it), each piece with its
/// share of the box: from `x + start · w / n` to `x + end · w / n`, at least a point wide, the same
/// top and height — `pipeline::approx_words`' rule within one word, a space counting for nothing.
fn split_word(w: &OcrWord, cuts: &[usize]) -> Vec<OcrWord> {
    let chars: Vec<char> = w.text.chars().collect();
    let n = chars.len().max(1) as i64;
    let mut bounds = vec![0usize];
    bounds.extend(cuts.iter().copied().filter(|c| *c > 0 && *c < chars.len()));
    bounds.push(chars.len());
    bounds.dedup();
    let width = i64::from(w.w.max(0));
    bounds
        .windows(2)
        .map(|b| {
            let x0 = i64::from(w.x) + b[0] as i64 * width / n;
            let x1 = i64::from(w.x) + b[1] as i64 * width / n;
            OcrWord { text: chars[b[0]..b[1]].iter().collect(), x: x0 as i32, y: w.y, w: (x1 - x0).max(1) as i32, h: w.h }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x: i32, w: i32) -> OcrWord {
        OcrWord { text: text.to_string(), x, y: 3, w, h: 12 }
    }

    fn line(words: &[OcrWord]) -> OcrLine {
        OcrLine { text: words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "), words: words.to_vec() }
    }

    fn texts(lines: &[OcrLine]) -> String {
        lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")
    }

    /// Windows' rule, case by case: the system recogniser's reading whenever it read anything,
    /// whatever the neural one read; the neural text, trimmed, only when the system read nothing;
    /// nothing when neither did, and nothing of the neural one's when the system failed — and a
    /// lone glyph, or text on ink that is none, answers as any text would.
    #[test]
    fn the_system_recogniser_answers_when_it_reads_anything_and_the_neural_one_only_when_it_reads_nothing() {
        assert_eq!(pick(Some("64"), Some("84")), Pick::System, "the system's reading stands, the neural one unread");
        assert_eq!(pick(Some("64"), None), Pick::System);
        assert_eq!(pick(Some(" x "), Some("")), Pick::System);
        assert_eq!(pick(Some(""), Some("0")), Pick::Paddle("0"), "the lone digit");
        assert_eq!(pick(Some(" \n"), Some(" 1 ")), Pick::Paddle("1"), "white space is nothing; the text trimmed");
        assert_eq!(pick(Some(""), Some("l.")), Pick::Paddle("l."), "invented on a level meter: answered, as on Windows");
        assert_eq!(pick(Some(""), Some("  ")), Pick::Nothing);
        assert_eq!(pick(Some(""), None), Pick::Nothing, "not asked, not answered in time, or nothing read");
        assert_eq!(pick(None, Some("0")), Pick::Refused, "a failure is no reading of nothing: not waited for, not used");
        assert_eq!(pick(None, None), Pick::Refused);
        assert!(waits(Some("")) && waits(Some(" \n\t")) && !waits(Some("0")) && !waits(None));
    }

    /// The rule is Windows' as `recognize_image` is written: checked where it is written, since
    /// that file compiles only on Windows and must not change for this one.
    #[test]
    fn the_rule_is_windows_own() {
        const SRC: &str = include_str!("../backend/windows.rs");
        let start = SRC.find("fn recognize_image(").expect("recognize_image");
        let body = &SRC[start..start + SRC[start..].find("\n}\n").unwrap()];
        let at = |what: &str| body.find(what).unwrap_or_else(|| panic!("recognize_image no longer says {what}"));
        // 1. Asked for a small region only, before the system recogniser starts on it.
        assert!(at("let small = crate::ocr::policy::is_small(") < at("super::paddle_ocr::ask(cap)"));
        assert!(body.contains("let paddle = small.then(|| super::paddle_ocr::ask(cap)).flatten();"));
        assert!(at("super::paddle_ocr::ask(cap)") < at("run_ocr(img, lang)"));
        // 2. A blank crop: cancelled, nothing, skipped.
        let blank = &body[at("if tight.as_ref().is_some_and(|t| t.blank) {")..at("run_ocr(img, lang)")];
        assert!(blank.contains("drop(paddle);") && blank.contains("skipped: true"));
        // 3. A failed system recogniser fails the read before the merge: the neural answer unused.
        assert!(body.contains("run_ocr(img, lang).map_err(|e| format!(\"OCR failed: {e}\"))?;"));
        assert!(at("run_ocr(img, lang).map_err(") < at("if let Some(asked) = paddle {"));
        // 4 and 5. Waited for only when the system recogniser read nothing; its text then, no words,
        // no lines, the content crop as the box.
        let merge = &body[at("if let Some(asked) = paddle {")..at("// Behind TRACE")];
        let order = ["if text.trim().is_empty() {", "asked.wait()", "text = t;", "words.clear();", "lines.clear();", "used_paddle = true;"];
        let found: Vec<usize> = order.iter().map(|w| merge.find(w).unwrap_or_else(|| panic!("{w}"))).collect();
        assert!(found.windows(2).all(|p| p[0] < p[1]), "{merge}");
        assert!(body.contains("let fallback = used_paddle"));
        assert!(body.contains("(t.off_x as i32, t.off_y as i32, t.cw as i32, t.ch as i32)"), "the content crop as the box");
        // 6. Nothing filters it.
        assert!(!merge.contains("score"), "no minimum score");
        // 7. The log line, with the switch or trace on.
        assert!(body.contains("if debug || crate::appcfg::trace() {"));
        assert!(body.contains("waited {wait_ms:.1}ms for paddle (") && body.contains("\"which answered\"") && body.contains("\"which had nothing\""));
        // And what `pick` and `waits` say is that.
        assert_eq!((waits(Some("")), waits(Some("x")), waits(None)), (true, false, false));
    }

    /// A Mac reads by it as written above: checked where it is written, since that file compiles
    /// only for a Mac. Asked before the first pass, at the reader's priority; the fast level's check
    /// first on an Intel Mac, waited for within its bound; then the accurate ladder without its
    /// fast rung; the recogniser waited for only where that read nothing and was not refused,
    /// within the budget, and not by a read that made way for a key press's; its text by `pick`;
    /// the pass made only for the counts after that. Not asked: today's ladder, the fast rung
    /// included.
    #[test]
    fn a_mac_reads_by_windows_rule() {
        const SRC: &str = include_str!("../backend/macos/ocr.rs");
        let body = |head: &str| {
            let start = SRC.find(head).unwrap_or_else(|| panic!("{head}"));
            &SRC[start..start + SRC[start..].find("\n    }\n").or_else(|| SRC[start..].find("\n}\n")).unwrap()]
        };
        let noted = body("fn recognize_noted(");
        let at = |s: &str, what: &str| s.find(what).unwrap_or_else(|| panic!("{what}"));
        assert!(at(noted, "if plan.blank {") < at(noted, "Neural::ask(&rgba, px_w, &plan, scale)"), "after the blank guard");
        assert!(at(noted, "Neural::ask(&rgba, px_w, &plan, scale)") < at(noted, "rungs.climb(neural.as_mut())"), "before the first pass");
        assert!(SRC.contains("paddle_ocr::ask_with(cw, ch, &pixels, Tighten::for_scale(scale), Qos::Reader)"), "at the reader's priority");
        let merged = body("    fn merged(");
        let order = [
            "self.accurate(tight, true), Climbed { today: true",
            "PaddleUse::FastMerge && ladder.fast_ok && neural.fits",
            "Stage::FastFirst",
            "neural.poll(merge::check_wait(self.left(), began.elapsed()))",
            "merge::check(",
            "return (Some(vision_read_of(lines)), climbed);",
            "let first = self.rung_one(tight);",
            "let refused = first.is_none();",
            "self.after_rung_one(first, tight, false)",
            "let bound = if ladder.preempted() { Duration::ZERO } else { self.left() };",
            "if merge::waits(system) { neural.wait(bound) } else { None }",
            "merge::pick(system, paddle.as_deref())",
            "climbed.crop = Some(crop_in_points(plan, scale))",
            "if self.counts_fast() {",
        ];
        let found: Vec<usize> = order.iter().map(|w| at(merged, w)).collect();
        assert!(found.windows(2).all(|p| p[0] < p[1]), "{merged}");
        assert!(merged.contains("let system = (!refused).then("), "{merged}");
        assert_eq!(merged.matches("self.accurate(tight, true)").count(), 1, "the fast rung only where it was not asked");
        // The pass made only for the counts is one fast pass, after the answer, and nothing else.
        let counted = body("    fn counted(");
        assert!(counted.contains("Stage::FastCompared") && !counted.contains("rung_one") && !counted.contains("after_rung_one"), "{counted}");
        assert!(!SRC.contains("self.aside"), "no pass is set aside from the budget any more");
    }

    #[test]
    fn a_read_waits_only_within_the_budget_and_the_check_no_longer_than_the_fast_pass() {
        let ms = Duration::from_millis;
        assert_eq!(left(ms(250), ms(40)), ms(210));
        assert_eq!(left(ms(250), ms(250)), Duration::ZERO);
        assert_eq!(left(ms(250), ms(612)), Duration::ZERO, "spent: only an answer already there");
        assert_eq!(check_wait(ms(230), ms(14)), ms(14), "as long again as the fast pass");
        assert_eq!(check_wait(ms(9), ms(40)), ms(9), "never past the budget");
        assert_eq!(check_wait(Duration::ZERO, ms(14)), Duration::ZERO);
    }

    /// The spacing, proved on the readings CI's three Macs gave (`ocr-bench`, run 37089023246):
    /// the two the fast level ran together come out exactly right, and every other pair the two
    /// agreed on comes out as the fast level read it. Where the fast level reads a space the
    /// recogniser does not, the two do not agree: no pair of that run did that.
    #[test]
    fn the_spacing_is_the_recognisers_where_the_fast_level_dropped_a_space_and_nothing_else_changes() {
        let one = |t: &str| vec![line(&[word(t, 10, 40)])];
        // val-db@2x and val-ct@2x: fast "-0.5dB" and "+3ct", the recogniser "-0.5 dB" and "+3 ct".
        assert_eq!(texts(&respaced(&one("-0.5dB"), "-0.5 dB").unwrap()), "-0.5 dB");
        assert_eq!(texts(&respaced(&one("+3ct"), "+3 ct").unwrap()), "+3 ct");
        assert_eq!(spaces_to_add("-0.5dB", "-0.5 dB"), Some(vec![4]));
        // Every other agreed pair of that run: the same spaces, so unchanged.
        let agreed = [
            ("64", "64"),
            ("DEF", "DEF"),
            ("1", "1"),
            ("empty", "empty"),
            ("0.50", "0.50"),
            ("-12", "-12"),
            ("Legato", "Legato"),
        ];
        for (fast, paddle) in agreed {
            let lines = one(fast);
            assert_eq!(respaced(&lines, paddle), Some(lines.clone()), "{fast}");
        }
        let db = vec![line(&[word("-0.5", 10, 24), word("dB", 38, 12)])];
        assert_eq!(respaced(&db, "-0.5 dB"), Some(db.clone()), "val-db@1x: both read the space");
        // A space the fast level reads and the recogniser does not: no agreement, since neither
        // spacing is known to be right — and taking both would speak a value neither read.
        let gain = vec![line(&[word("0.00", 0, 24), word("dB", 28, 12)])];
        assert_eq!(respaced(&gain, "0.00dB"), None);
        assert_eq!(spaces_to_add("1 23", "12 3"), None, "not 1 2 3");
        assert_eq!(spaces_to_add("1 2", "12"), None);
        assert_eq!(spaces_to_add("1 2", "1 2 "), Some(vec![]), "a space at the end is no place");
        // Its own characters kept: a typographic minus stays one.
        assert_eq!(texts(&respaced(&one("\u{2212}0.5dB"), "-0.5 dB").unwrap()), "\u{2212}0.5 dB");
        // Not the same: nothing to respace, and the check failed.
        assert_eq!(respaced(&one("o"), "0"), None);
        assert_eq!(spaces_to_add("O", "0"), None);
    }

    /// The check as a read makes it: agreed, with the respaced lines as the answer; any other pair
    /// no agreement, one side reading nothing included; nothing on both sides neither.
    #[test]
    fn the_check_agrees_only_on_the_same_characters_and_spaces_the_recogniser_has_too() {
        let one = |t: &str| vec![line(&[word(t, 10, 40)])];
        let ok = check(Some(&one("+3ct")), Some(" +3 ct "));
        assert_eq!(ok.agreed, Some(true));
        assert_eq!(texts(&ok.answer.unwrap()), "+3 ct");
        assert_eq!(check(Some(&one("64")), Some("64")), Check { agreed: Some(true), answer: Some(one("64")) });
        let no = Check { agreed: Some(false), answer: None };
        assert_eq!(check(Some(&one("o")), Some("0")), no, "o is not 0");
        assert_eq!(check(Some(&one("64")), None), no, "the recogniser read nothing, or not in time");
        assert_eq!(check(None, Some("64")), no, "Vision refused the fast pass");
        assert_eq!(check(Some(&one(" ")), Some("1")), no, "the fast level read nothing");
        let gain = vec![line(&[word("0.00", 0, 24), word("dB", 28, 12)])];
        assert_eq!(check(Some(&gain), Some("0.00dB")), no, "spaced apart");
        assert_eq!(check(Some(&one("")), Some("  ")), Check { agreed: None, answer: None });
        assert_eq!(check(None, None), Check { agreed: None, answer: None });
    }

    /// A word the recogniser reads as two is cut in two, its box shared by characters; the line's
    /// text follows; a space at either end, or a run of them, is one place or none; two lines are
    /// already apart at their break.
    #[test]
    fn a_respaced_word_is_cut_and_its_box_shared_by_characters() {
        let got = respaced(&[line(&[word("-0.5dB", 10, 40)])], " -0.5  dB ").unwrap();
        assert_eq!(got[0].text, "-0.5 dB");
        assert_eq!(got[0].words, vec![word("-0.5", 10, 26), word("dB", 36, 14)], "four of six characters, two of six");
        // Two cuts in one word, among other words.
        let got = respaced(&[line(&[word("A", 0, 10), word("bcd", 20, 30)])], "A b c d").unwrap();
        assert_eq!(got[0].text, "A b c d");
        assert_eq!(got[0].words.iter().map(|w| (w.text.as_str(), w.x, w.w)).collect::<Vec<_>>(), [("A", 0, 10), ("b", 20, 10), ("c", 30, 10), ("d", 40, 10)]);
        // Two observations on one row, as Vision gives two runs: the break is their space already.
        let two = [line(&[word("+3", 0, 12)]), line(&[word("ct", 16, 10)])];
        assert_eq!(respaced(&two, "+3 ct"), Some(two.to_vec()));
        // A space where the second line begins, and one inside its word.
        let got = respaced(&[line(&[word("C#4", 0, 18)]), line(&[word("+36Ct", 30, 30)])], "C#4 +36 Ct").unwrap();
        assert_eq!(texts(&got), "C#4\n+36 Ct");
        assert_eq!(got[1].words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["+36", "Ct"]);
        // A word too narrow to share still gives each piece a point.
        let got = respaced(&[line(&[word("ab", 5, 1)])], "a b").unwrap();
        assert!(got[0].words.iter().all(|w| w.w >= 1), "{got:?}");
        // Words that are not the line's characters: left as they are, the text too.
        let odd = vec![OcrLine { text: "+3ct".into(), words: vec![word("+3", 0, 10)] }];
        assert_eq!(respaced(&odd, "+3 ct"), Some(odd.clone()));
    }
}
