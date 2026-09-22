//! From what an engine said to what a module is handed: rows instead of engine lines, absolute
//! coordinates, and the same shape whichever recogniser answered.
//!
//! **Rows, not lines.** Vision returns one observation per RUN of text, so a row with a gap in
//! it — two read-outs side by side — arrives as two lines whose tops differ by a pixel.
//! Windows' `OcrLine` is whatever that engine decides. A module that walked `lines` would see
//! different structures for the same screen on the two platforms, so the host groups the
//! engine's lines into rows itself: lines whose boxes overlap vertically by at least half the
//! smaller height are one row, ordered left to right, their text joined with a space.
//!
//! **Words always come with text.** When the Windows fallback recogniser answers (the lone
//! digit the system engine refuses), it reads without locating. The legacy calls hand that
//! back as text with no words; `read` gives each whitespace-separated token a box of its own,
//! in proportion to its length, inside the content crop that recogniser read, and marks it
//! `approx`. So `words` is non-empty exactly when `text` has something in it, on both
//! platforms, whichever engine answered.
//!
//! Std only; borrowed by `crates/macos-check`.

use super::types::{EngineLine, EngineOut, Reading, Rect, Row, Status, Word, WordBox};

/// Engine lines grouped into rows, region-relative: `(text, box, words)` per row, top to bottom.
/// A line with no text is dropped; a line whose words give it no box stands as a row of its
/// own.
pub fn rows(lines: &[EngineLine]) -> Vec<(String, Rect, Vec<WordBox>)> {
    struct L<'a> {
        text: &'a str,
        rect: Rect,
        words: Vec<&'a WordBox>,
    }
    let mut ls: Vec<L> = lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| {
            let words: Vec<&WordBox> = l.words.iter().filter(|w| !w.text.trim().is_empty()).collect();
            let rect = words
                .iter()
                .fold(Rect::default(), |acc, w| acc.union(&Rect::new(w.x, w.y, w.w, w.h)));
            L { text: l.text.trim(), rect, words }
        })
        .collect();
    ls.sort_by_key(|l| (l.rect.y, l.rect.x));

    // Each row: its box and the indices of its lines.
    let mut grouped: Vec<(Rect, Vec<usize>)> = Vec::new();
    for (i, l) in ls.iter().enumerate() {
        let joins = grouped.iter_mut().rev().find(|(r, _)| same_row(r, &l.rect));
        match joins {
            Some((r, members)) => {
                *r = r.union(&l.rect);
                members.push(i);
            }
            None => grouped.push((l.rect, vec![i])),
        }
    }
    grouped.sort_by_key(|(r, _)| (r.y, r.x));
    grouped
        .into_iter()
        .map(|(rect, mut members)| {
            members.sort_by_key(|&i| (ls[i].rect.x, ls[i].rect.y));
            let text = members.iter().map(|&i| ls[i].text).collect::<Vec<_>>().join(" ");
            let words = members.iter().flat_map(|&i| ls[i].words.iter().map(|w| (*w).clone())).collect();
            (text, rect, words)
        })
        .collect()
}

/// Whether a line with box `b` belongs to the row with box `row`: they overlap vertically by at
/// least half the smaller of the two heights. A box with no height joins nothing.
fn same_row(row: &Rect, b: &Rect) -> bool {
    if row.is_empty() || b.is_empty() {
        return false;
    }
    let overlap = row.bottom().min(b.bottom()) - row.y.max(b.y);
    overlap > 0 && overlap * 2 >= row.h.min(b.h)
}

/// Boxes for `text`'s whitespace-separated tokens, laid out inside `within` in proportion to
/// their length (a space counts as one character), each marked `approx`.
pub fn approx_words(text: &str, within: Rect) -> Vec<Word> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let lens: Vec<i64> = tokens.iter().map(|t| t.chars().count() as i64).collect();
    let total: i64 = lens.iter().sum::<i64>() + (tokens.len() as i64 - 1);
    let w = within.w.max(0) as i64;
    let mut start = 0i64;
    let mut out = Vec::with_capacity(tokens.len());
    for (t, len) in tokens.iter().zip(&lens) {
        let x0 = within.x as i64 + start * w / total;
        let x1 = within.x as i64 + (start + len) * w / total;
        out.push(Word {
            text: (*t).to_string(),
            x: x0 as i32,
            y: within.y,
            w: ((x1 - x0).max(1)) as i32,
            h: within.h.max(1),
            approx: true,
        });
        start += len + 1;
    }
    out
}

fn absolute(region: &Rect, w: &WordBox) -> Word {
    Word {
        text: w.text.trim().to_string(),
        x: region.x + w.x,
        y: region.y + w.y,
        w: w.w,
        h: w.h,
        approx: false,
    }
}

/// One region's `Reading` from what its engine said. `lang` is the tag the engine was asked
/// for, as the platform names it.
pub fn normalise(region: Rect, out: EngineOut, lang: &str) -> Reading {
    let with_lang = |mut r: Reading| {
        r.lang = lang.to_string();
        r
    };
    match out {
        EngineOut::Blank => with_lang(Reading::outcome(region, Status::Blank, None)),
        EngineOut::Failed(e) => with_lang(Reading::failed(region, e)),
        EngineOut::Fallback { text, content } => {
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                return with_lang(Reading::outcome(region, Status::None, None));
            }
            let within = Rect::new(region.x + content.x, region.y + content.y, content.w, content.h);
            let words = approx_words(&text, within);
            let row = Row { text: text.clone(), rect: within, words: words.clone() };
            with_lang(Reading {
                rect: region,
                status: Status::Text,
                text,
                rows: vec![row],
                words,
                lang: String::new(),
                error: None,
            })
        }
        EngineOut::Lines(lines) => {
            let mut out_rows = Vec::new();
            for (text, rect, words) in rows(&lines) {
                let abs = Rect::new(region.x + rect.x, region.y + rect.y, rect.w, rect.h);
                let mut words: Vec<Word> = words.iter().map(|w| absolute(&region, w)).collect();
                if words.is_empty() {
                    // Text the engine did not locate: the row's box, shared out, or the region's
                    // when the row has none either.
                    let within = if abs.is_empty() { region } else { abs };
                    words = approx_words(&text, within);
                }
                out_rows.push(Row { text, rect: abs, words });
            }
            if out_rows.is_empty() {
                return with_lang(Reading::outcome(region, Status::None, None));
            }
            let text = out_rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
            let words = out_rows.iter().flat_map(|r| r.words.iter().cloned()).collect();
            with_lang(Reading {
                rect: region,
                status: Status::Text,
                text,
                rows: out_rows,
                words,
                lang: String::new(),
                error: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wb(text: &str, x: i32, y: i32, w: i32, h: i32) -> WordBox {
        WordBox { text: text.to_string(), x, y, w, h }
    }

    fn line(text: &str, words: Vec<WordBox>) -> EngineLine {
        EngineLine { text: text.to_string(), words }
    }

    /// The text/words invariant `host.ocr.read` promises, for every reading.
    fn holds(r: &Reading) {
        let has_text = r.text.chars().any(|c| !c.is_whitespace());
        assert_eq!(has_text, !r.words.is_empty(), "{r:?}");
        assert_eq!(has_text, r.status == Status::Text, "{r:?}");
        assert!(r.words.iter().all(|w| !w.text.is_empty()));
    }

    #[test]
    fn two_runs_on_one_row_become_one_row_in_left_to_right_order() {
        // Vision's shape: the cents first, a pixel higher, then the note name to its left.
        let lines = vec![
            line("+36 Ct", vec![wb("+36", 100, 10, 20, 12), wb("Ct", 124, 10, 12, 12)]),
            line("C#4", vec![wb("C#4", 0, 11, 24, 12)]),
            line("Volume", vec![wb("Volume", 2, 40, 50, 12)]),
        ];
        let got = rows(&lines);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "C#4 +36 Ct");
        assert_eq!(got[0].1, Rect::new(0, 10, 136, 13));
        assert_eq!(got[0].2.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["C#4", "+36", "Ct"]);
        assert_eq!(got[1].0, "Volume");
    }

    #[test]
    fn lines_that_barely_touch_stay_on_separate_rows() {
        let lines = vec![
            line("upper", vec![wb("upper", 0, 0, 40, 12)]),
            line("lower", vec![wb("lower", 0, 10, 40, 12)]), // 2 px of 12 overlap
        ];
        assert_eq!(rows(&lines).len(), 2);
    }

    #[test]
    fn a_reading_is_absolute_and_keeps_its_invariant() {
        let region = Rect::new(300, 200, 400, 60);
        let r = normalise(
            region,
            EngineOut::Lines(vec![
                line("Load", vec![wb("Load", 4, 30, 30, 12)]),
                line("File  Edit", vec![wb("File", 4, 2, 24, 12), wb("Edit", 40, 2, 24, 12)]),
                line("   ", vec![]),
            ]),
            "de-DE",
        );
        holds(&r);
        assert_eq!(r.status, Status::Text);
        assert_eq!(r.text, "File  Edit\nLoad");
        assert_eq!(r.lang, "de-DE");
        assert_eq!(r.words[0], Word { text: "File".into(), x: 304, y: 202, w: 24, h: 12, approx: false });
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[1].rect, Rect::new(304, 230, 30, 12));
    }

    #[test]
    fn blank_empty_and_failed_have_their_own_statuses_and_no_text() {
        let region = Rect::new(0, 0, 10, 10);
        for (out, status) in [
            (EngineOut::Blank, Status::Blank),
            (EngineOut::Lines(vec![]), Status::None),
            (EngineOut::Lines(vec![line(" ", vec![wb(" ", 0, 0, 1, 1)])]), Status::None),
            (EngineOut::Fallback { text: "  ".into(), content: region }, Status::None),
            (EngineOut::Failed("screen capture failed".into()), Status::Failed),
        ] {
            let r = normalise(region, out, "en-US");
            assert_eq!(r.status, status);
            holds(&r);
            assert_eq!(r.error.is_some(), status == Status::Failed);
        }
    }

    #[test]
    fn the_fallbacks_text_gets_approximate_boxes_inside_its_crop() {
        let region = Rect::new(1000, 500, 80, 20);
        let r = normalise(
            region,
            EngineOut::Fallback { text: "7".into(), content: Rect::new(30, 3, 9, 14) },
            "en-US",
        );
        holds(&r);
        assert_eq!(r.text, "7");
        assert_eq!(r.words, vec![Word { text: "7".into(), x: 1030, y: 503, w: 9, h: 14, approx: true }]);

        let words = approx_words("ab c", Rect::new(0, 0, 40, 10));
        assert_eq!(words.len(), 2);
        assert_eq!((words[0].x, words[0].w), (0, 20), "two of four characters");
        assert_eq!((words[1].x, words[1].w), (30, 10), "after the space");
        assert!(words.iter().all(|w| w.approx));
    }

    #[test]
    fn text_an_engine_did_not_locate_still_has_words() {
        let region = Rect::new(10, 10, 100, 20);
        let r = normalise(region, EngineOut::Lines(vec![line("42 dB", vec![])]), "en-US");
        holds(&r);
        assert!(r.words.iter().all(|w| w.approx));
        assert!(r.words.iter().all(|w| w.x >= 10 && w.x + w.w <= 110));
    }
}
