//! `automation-platform ocr-bench --paddle`: the neural recogniser alone, over the bench's
//! pictures, on this machine — on Windows, and on a Mac whose bundle carries ONNX Runtime; and
//! `--paddle-probe`, whether it loads and reads at all ([`probe`]). The pure half — the pictures,
//! the judging, the advice lines — is `ocr/bench.rs`, and the arithmetic around the model is
//! `ocr/paddle_pre.rs`. On a Mac the recogniser beside Vision — the pipeline's strategies, the
//! threads — is `backend/macos/ocr/bench.rs`'s.
//!
//! What it answers, in this order:
//! 1. **Each picture**: what the recogniser reads with Windows' crop (`Tighten::WINDOWS`), right
//!    or wrong against the picture's accepted answers, its score, and what the preprocessing and
//!    the model cost (medians of 20 recognitions after 3 discarded, on this thread). Then, where
//!    the system recogniser is given (Windows), what `Windows.Media.Ocr` reads and what Windows
//!    answers (`recognize_image`, unchanged), as a second opinion on every accepted answer.
//! 2. **Width**: `line@1x` and `line@2x` cut between their words into every run of whole words,
//!    and how wide (the ink's width over its height) a run may be and still be read right. Its
//!    last line suggests `paddle_pre::MAX_ASPECT`.
//! 3. **Retina crop**: every small 2x picture under four crop settings — 3 or 6 pixels of margin,
//!    at least 2x or 1x enlargement — and which reads the most right and invents the least.
//! 4. **Score**: the scores of right readings against those invented on pictures without text,
//!    and whether a minimum score would separate them.
//!
//! It opens no window, captures nothing, never speaks, and writes nothing to the application's
//! log (which this process never opens). Everything runs on the calling thread, but for the
//! system recogniser's reads, which hand each region to the recogniser thread as Windows does;
//! those come last, and the run waits for that thread before it ends.

use std::time::{Duration, Instant};

use image::RgbImage;

use super::{engine_words, init_timed, recognize_timed, PaddleSample, IN_FLIGHT};
use crate::backend::{CapturedImage, OcrText};
use crate::ocr::bench::{self as pure, Accuracy, AspectAdvice, Fixture, Options, Out, SettingResult};
use crate::ocr::paddle_pre::{self, Tighten};

/// What reads a captured region as the application does: Windows' `recognize_image`.
pub(crate) type SystemRead<'a> = &'a dyn Fn(&CapturedImage) -> Result<OcrText, String>;

/// One picture, decoded once.
struct Pic {
    fixture: &'static Fixture,
    rgb: RgbImage,
}

/// What the recogniser made of one picture in section 1.
struct Row {
    pic: usize,
    text: String,
    acc: Accuracy,
    score: f32,
    pre_ms: f64,
    run_ms: f64,
    /// Whether every timed recognition read the same text as the first.
    steady: bool,
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    match v.len() {
        0 => f64::NAN,
        n if n % 2 == 1 => v[n / 2],
        n => (v[n / 2 - 1] + v[n / 2]) / 2.0,
    }
}

/// The run, written to `path`: the exit code (0 done, 1 when the recogniser could not be made).
pub(crate) fn bench_rows(o: &Options, system: Option<SystemRead<'_>>, path: std::path::PathBuf) -> i32 {
    let started = Instant::now();
    let mut out = Out::open(path, o.quiet);
    if let Some(p) = &out.path {
        out.loud(&format!("writing to {}", p.display()));
    }
    let (discard, samples) = if o.quick { (1, 5) } else { (3, 20) };
    out.say(&format!(
        "start: automation-platform ocr-bench --paddle, {}, {} mode, {} on {} logical processors; each picture \
         read {} times, the first {} not timed",
        crate::build_info::log_line(env!("CARGO_PKG_VERSION")),
        if o.quick { "quick" } else { "full" },
        std::env::consts::OS,
        std::thread::available_parallelism().map_or(0, |n| n.get()),
        discard + samples,
        discard
    ));
    let load = match init_timed() {
        Ok(ms) => ms,
        Err(why) => {
            out.loud(&format!("stopped: the neural recogniser could not be made: {why}"));
            return 1;
        }
    };
    out.say(&format!(
        "paddle | the session was made in {} ms: {}, one intra-op thread, graph optimisation level 3",
        pure::ms(load),
        engine_words()
    ));

    let mut pics = Vec::new();
    for f in pure::all_pictures(o) {
        match image::load_from_memory(f.png) {
            Ok(img) => pics.push(Pic { fixture: f, rgb: img.to_rgb8() }),
            Err(e) => out.say(&format!("picture {}: not usable: {e}", f.label())),
        }
    }

    // ── 1. Each picture ──
    let section = Instant::now();
    let mut rows = Vec::new();
    let mut first_ever = None;
    for (i, p) in pics.iter().enumerate() {
        let mut timed: Vec<PaddleSample> = Vec::new();
        let mut first = None;
        for k in 0..discard + samples {
            let t = Instant::now();
            let Some(s) = recognize_timed(&p.rgb, Tighten::WINDOWS) else {
                out.loud(&format!("stopped: the session could not run {}", p.fixture.label()));
                return 1;
            };
            if first_ever.is_none() {
                first_ever = Some(t.elapsed().as_secs_f64() * 1000.0);
            }
            if k == 0 {
                first = Some(s.text.clone());
            }
            if k >= discard {
                timed.push(s);
            }
        }
        let first = first.unwrap_or_default();
        let steady = timed.iter().all(|s| s.text == first);
        let reads: Vec<String> = timed.iter().map(|s| pure::normalise(&s.text)).collect();
        let acc = pure::judge(&reads, reads.len(), p.fixture.accept);
        let row = Row {
            pic: i,
            score: timed.first().map_or(f32::NAN, |s| s.score),
            pre_ms: median(timed.iter().map(|s| s.pre_ms).collect()),
            run_ms: median(timed.iter().map(|s| s.run_ms).collect()),
            text: first,
            acc,
            steady,
        };
        out.say(&row_line(&row, p));
        rows.push(row);
    }
    if let Some(ms) = first_ever {
        out.say(&format!("paddle | the first recognition after the session was made took {} ms", pure::ms(ms)));
    }
    let must: Vec<&Row> = rows.iter().filter(|r| pics[r.pic].fixture.must_read && pics[r.pic].fixture.small()).collect();
    let nothing: Vec<&Row> = rows.iter().filter(|r| pics[r.pic].fixture.holds_nothing()).collect();
    let named = |rs: &[&Row], pick: &dyn Fn(&Row) -> bool| -> String {
        let v: Vec<String> =
            rs.iter().filter(|r| pick(r)).map(|r| format!("{} \"{}\"", pics[r.pic].fixture.label(), r.text)).collect();
        if v.is_empty() {
            "none".to_string()
        } else {
            v.join(", ")
        }
    };
    out.say(&format!(
        "paddle | small pictures that must be read: {} of {} right, {} more right but for spaces or dash forms; \
         wrong: {}",
        must.iter().filter(|r| r.acc.right()).count(),
        must.len(),
        must.iter().filter(|r| matches!(r.acc, Accuracy::Loosely { .. })).count(),
        named(&must, &|r| !r.acc.loosely_right())
    ));
    // Text on which nothing is right too — a lone dash, half a word — is still misread when the
    // recogniser reads something else.
    let optional: Vec<&Row> = rows
        .iter()
        .filter(|r| {
            let f = pics[r.pic].fixture;
            !f.must_read && !f.holds_nothing() && f.small()
        })
        .collect();
    if !optional.is_empty() {
        out.say(&format!(
            "paddle | small pictures with text on which nothing is right too: {} of {} right; wrong: {}",
            optional.iter().filter(|r| r.acc.right()).count(),
            optional.len(),
            named(&optional, &|r| !r.acc.right())
        ));
    }
    out.say(&format!(
        "paddle | pictures without text: {} of {} read as something (invented): {}",
        nothing.iter().filter(|r| !r.acc.right()).count(),
        nothing.len(),
        named(&nothing, &|r| !r.acc.right())
    ));
    out.say(&format!("paddle | section took {:.1} s", section.elapsed().as_secs_f64()));

    // ── 2. Width ──
    let section = Instant::now();
    let mut cuts = Vec::new();
    for label in ["line@1x", "line@2x"] {
        let Some(p) = pics.iter().find(|p| p.fixture.label() == label) else { continue };
        let words: Vec<&str> = p.fixture.accept[0].split(' ').collect();
        let Some(spans) = pure::split_words(&paddle_pre::ink_columns(&p.rgb), words.len()) else {
            out.say(&format!("width | {label}: its ink does not split into its {} words; not measured", words.len()));
            continue;
        };
        let (w, h) = p.rgb.dimensions();
        let t = Tighten::for_scale(p.fixture.scale as f64);
        for i in 0..spans.len() {
            for j in i..spans.len() {
                let x0 = (if i == 0 { 0 } else { (spans[i - 1].1 + spans[i].0) / 2 }) as u32;
                let x1 = (if j + 1 == spans.len() { w as usize - 1 } else { (spans[j].1 + spans[j + 1].0) / 2 }) as u32;
                let cut = image::imageops::crop_imm(&p.rgb, x0, 0, x1 - x0 + 1, h).to_image();
                let Some([a, b, c, d]) = paddle_pre::ink_box(&cut) else { continue };
                let aspect = (c - a + 1) as f32 / (d - b + 1) as f32;
                let want = words[i..=j].join(" ");
                let read = recognize_timed(&cut, t).map(|s| s.text).unwrap_or_default();
                let acc = pure::judge(&[pure::normalise(&read)], 1, &[want.as_str()]);
                out.say(&format!(
                    "width | {label} | words {}-{} | ink {}x{}, {aspect:.1} times as wide as tall ({} MAX_ASPECT), model \
                     input {} px wide | {}",
                    i + 1,
                    j + 1,
                    c - a + 1,
                    d - b + 1,
                    if paddle_pre::fits((c - a + 1) as usize, (d - b + 1) as usize, 1) { "within" } else { "beyond" },
                    paddle_pre::preprocess(&paddle_pre::tighten(&cut, t)).0,
                    pure::accuracy_words(&acc, &read, None)
                ));
                cuts.push((aspect, acc.loosely_right(), acc.right()));
            }
        }
    }
    // Per whole ratio, how many cuts were read right: the curve the advice is read off.
    for top in 2..=21 {
        // The last step takes everything wider than 20.
        let wider = if top == 21 { f32::INFINITY } else { top as f32 };
        let these: Vec<&(f32, bool, bool)> = cuts.iter().filter(|c| c.0 > (top - 1) as f32 && c.0 <= wider).collect();
        if !these.is_empty() {
            out.say(&format!(
                "width | over {}{} times as wide as tall: {} of {} right, {} but for spaces",
                top - 1,
                if top == 21 { String::new() } else { format!(" up to {top}") },
                these.iter().filter(|c| c.2).count(),
                these.len(),
                these.iter().filter(|c| c.1 && !c.2).count()
            ));
        }
    }
    let advice = pure::aspect_advice(&cuts.iter().map(|c| (c.0, c.1)).collect::<Vec<_>>());
    out.say(&advice.line());
    out.say(&format!(
        "width | paddle_pre::MAX_ASPECT is {} in this build{}",
        paddle_pre::MAX_ASPECT,
        match advice {
            AspectAdvice::Below { widest_right, .. } if paddle_pre::MAX_ASPECT > widest_right => {
                " — WIDER than this run read right"
            }
            AspectAdvice::Below { .. } => ", within what this run read right",
            AspectAdvice::NoMiss { widest, .. } if paddle_pre::MAX_ASPECT > widest => ", wider than any cut measured",
            _ => "",
        }
    ));
    out.say(&format!("width | section took {:.1} s", section.elapsed().as_secs_f64()));

    // ── 3. Retina crop ──
    let section = Instant::now();
    let settings: [(&str, Tighten); 4] = [
        ("margin 6, at least 1x (the Windows crop in points, Tighten::for_scale(2))", Tighten::for_scale(2.0)),
        ("margin 3, at least 2x (Tighten::WINDOWS)", Tighten::WINDOWS),
        ("margin 6, at least 2x", Tighten { margin_px: 6, min_up: 2.0 }),
        ("margin 3, at least 1x", Tighten { margin_px: 3, min_up: 1.0 }),
    ];
    let retina: Vec<&Pic> = pics.iter().filter(|p| p.fixture.scale == 2 && p.fixture.small()).collect();
    let mut results = Vec::new();
    let mut retina_reads: Vec<Vec<(String, Accuracy, f32)>> = Vec::new();
    for (name, t) in settings {
        let mut r = SettingResult { name: name.to_string(), right: 0, loose: 0, of: 0, invented: 0, nothing_of: 0 };
        let mut reads = Vec::new();
        let mut missed = Vec::new();
        for p in &retina {
            let s = recognize_timed(&p.rgb, t);
            let (text, score) = s.map(|s| (s.text, s.score)).unwrap_or((String::new(), f32::NAN));
            let acc = pure::judge(&[pure::normalise(&text)], 1, p.fixture.accept);
            if p.fixture.holds_nothing() {
                r.nothing_of += 1;
                if !acc.right() {
                    r.invented += 1;
                    missed.push(format!("{} invented \"{text}\"", p.fixture.label()));
                }
            } else {
                r.of += 1;
                r.right += usize::from(acc.right());
                r.loose += usize::from(acc.loosely_right());
                if !acc.loosely_right() {
                    missed.push(format!("{} \"{text}\"", p.fixture.label()));
                }
            }
            reads.push((text, acc, score));
        }
        out.say(&format!(
            "retina | {name} | {} of {} right ({} but for spaces), {} of {} without text invented | missed: {}",
            r.right,
            r.of,
            r.loose - r.right,
            r.invented,
            r.nothing_of,
            if missed.is_empty() { "none".to_string() } else { missed.join(", ") }
        ));
        results.push(r);
        retina_reads.push(reads);
    }
    let best = pure::best_setting(&results);
    if let Some(b) = best {
        out.say(&format!(
            "retina | reads the most and invents the least (the first of equals): {}{}",
            results[b].name,
            if b == 0 { "" } else { " — not Tighten::for_scale(2)" }
        ));
    }
    out.say(&format!("retina | section took {:.1} s", section.elapsed().as_secs_f64()));

    // ── 4. Score ──
    // Section 1's readings, and the Retina pictures' under the setting that did best.
    let mut right = Vec::new();
    let mut invented = Vec::new();
    let mut wrong = Vec::new();
    let mut sort = |text: &str, acc: &Accuracy, score: f32, f: &Fixture| {
        if text.is_empty() {
            return;
        }
        if f.holds_nothing() {
            invented.push(score);
        } else if acc.loosely_right() {
            right.push(score);
        } else {
            wrong.push(score);
        }
    };
    for r in rows.iter().filter(|r| pics[r.pic].fixture.small()) {
        sort(&r.text, &r.acc, r.score, pics[r.pic].fixture);
    }
    if let Some(b) = best {
        for (p, (text, acc, score)) in retina.iter().zip(&retina_reads[b]) {
            sort(text, acc, *score, p.fixture);
        }
    }
    let list = |v: &[f32]| {
        let mut v = v.to_vec();
        v.sort_by(|a, b| a.total_cmp(b));
        v.iter().map(|s| format!("{s:.3}")).collect::<Vec<_>>().join(", ")
    };
    out.say(&format!("score | right readings ({}): {}", right.len(), list(&right)));
    out.say(&format!("score | invented readings ({}): {}", invented.len(), list(&invented)));
    out.say(&format!("score | other wrong readings ({}): {}", wrong.len(), list(&wrong)));
    out.say(&pure::score_advice(&right, &invented, &wrong).line());

    // ── 5. The system recogniser beside it ──
    let mut system_reads: Vec<Option<(String, String)>> = vec![None; pics.len()];
    if let Some(read) = system {
        let section = Instant::now();
        for (i, p) in pics.iter().enumerate() {
            let cap = CapturedImage {
                w: p.rgb.width(),
                h: p.rgb.height(),
                rgba: p.rgb.pixels().flat_map(|px| [px.0[0], px.0[1], px.0[2], 255]).collect(),
            };
            let line = match read(&cap) {
                Ok(t) => {
                    let (alone, answer) = if t.skipped {
                        (String::new(), "nothing: the blank guard answered".to_string())
                    } else if t.fallback.is_some() {
                        (String::new(), format!("\"{}\" from the neural recogniser", t.text.trim()))
                    } else {
                        (t.text.trim().to_string(), format!("\"{}\" from Windows.Media.Ocr", pure::normalise(&t.text)))
                    };
                    let acc = pure::judge(&[pure::normalise(&alone)], 1, p.fixture.accept);
                    let said = format!("Windows.Media.Ocr: {}; Windows answers {answer}", pure::accuracy_words(&acc, &alone, None));
                    system_reads[i] = Some((pure::normalise(&alone), answer));
                    said
                }
                Err(e) => format!("the system recogniser failed: {e}"),
            };
            out.say(&format!("system | {} | {line}", p.fixture.label()));
        }
        // The recogniser thread may still finish a region the system recogniser answered first.
        IN_FLIGHT.wait_idle(Duration::from_secs(2));
        out.say(&format!("system | section took {:.1} s", section.elapsed().as_secs_f64()));
    }

    out.raw("");
    out.raw(&table(&pics, &rows, &system_reads));
    out.say(&format!("done in {:.1} s", started.elapsed().as_secs_f64()));
    0
}

/// `ocr-bench --paddle-probe`: makes the recogniser and reads `lone-1@2x` once with the crop in
/// points (`Tighten::for_scale(2)`), then prints one line — `ok` with what was read and how long
/// it took, or `not available` with the reason — and returns 0 or 1. On a Mac this is the check
/// that the bundle's ONNX Runtime loads on this macOS; it writes no file.
pub(crate) fn probe() -> i32 {
    let session = match init_timed() {
        Ok(ms) => ms,
        Err(why) => {
            println!("OCR BENCH: paddle probe | not available: {why}");
            return 1;
        }
    };
    let picture = pure::fixture("lone-1@2x").and_then(|f| image::load_from_memory(f.png).ok()).map(|i| i.to_rgb8());
    let Some(rgb) = picture else {
        println!("OCR BENCH: paddle probe | not available: the picture lone-1@2x could not be decoded");
        return 1;
    };
    match recognize_timed(&rgb, Tighten::for_scale(2.0)) {
        Some(s) => {
            let acc = pure::judge(&[pure::normalise(&s.text)], 1, &["1"]);
            println!(
                "OCR BENCH: paddle probe | ok: {}; the session made in {} ms; lone-1@2x {} in {} ms (preparing {} ms, \
                 the model {} ms)",
                engine_words(),
                pure::ms(session),
                pure::accuracy_words(&acc, &s.text, None),
                pure::ms(s.pre_ms + s.run_ms),
                pure::ms(s.pre_ms),
                pure::ms(s.run_ms)
            );
            0
        }
        None => {
            println!("OCR BENCH: paddle probe | not available: the session was made but could not run the model");
            1
        }
    }
}

fn row_line(r: &Row, p: &Pic) -> String {
    let f = p.fixture;
    let input = paddle_pre::preprocess(&paddle_pre::tighten(&p.rgb, Tighten::WINDOWS)).0;
    format!(
        "paddle | {} | accepts {}{} | {}{} | score {} | preparing {} ms, the model {} ms (medians), model input {} px wide{}",
        f.label(),
        f.accept_text(),
        if f.small() { "" } else { " (larger than 400x200: production never hands it to the recogniser)" },
        pure::accuracy_words(&r.acc, &r.text, None),
        if r.steady { "" } else { ", NOT the same text every time" },
        if r.score.is_finite() { format!("{:.3}", r.score) } else { "-".to_string() },
        pure::ms(r.pre_ms),
        pure::ms(r.run_ms),
        input,
        match f.origin {
            pure::Origin::Drawn => String::new(),
            pure::Origin::Captured { platform } => format!(", captured on {platform}"),
        }
    )
}

fn table(pics: &[Pic], rows: &[Row], system: &[Option<(String, String)>]) -> String {
    let mut t = String::from(
        "| picture | accepts | must read | neural recogniser | score | ms (prep + model) | Windows.Media.Ocr | Windows answers |\n\
         |---|---|---|---|---:|---:|---|---|\n",
    );
    let shown = |s: &str| if s.is_empty() { "nothing".to_string() } else { format!("\"{s}\"") };
    for r in rows {
        let f = pics[r.pic].fixture;
        let (winrt, answer) = system[r.pic].clone().map(|(a, b)| (shown(&a), b)).unwrap_or(("n/a".into(), "n/a".into()));
        t.push_str(&format!(
            "| {} | {} | {} | {}{} | {} | {} + {} | {winrt} | {answer} |\n",
            f.label(),
            f.accept_text(),
            if f.must_read { "yes" } else { "no" },
            shown(&r.text),
            pure::accuracy_mark(&r.acc),
            if r.score.is_finite() { format!("{:.3}", r.score) } else { "-".to_string() },
            pure::ms(r.pre_ms),
            pure::ms(r.run_ms)
        ));
    }
    t
}
