//! The neural recogniser's arithmetic around its model: what a region becomes before PaddleOCR's
//! recognition model reads it, what the model's answer becomes after, and when two answers are
//! the same. The recogniser itself — the session, its thread, the queue — is
//! `backend/paddle_ocr.rs`; this file was taken out of it, unchanged in what it computes for
//! Windows, so that it can be compiled and tested where that one cannot be.
//!
//! Only the standard library and `image`, on every platform. `crates/macos-check` borrows it, and
//! so does `tools/paddle-engines`, which runs the same preprocessing under two inference engines.
//! The dictionary is the file's own `include_str!`, relative to this file, so it resolves
//! wherever the file is borrowed.
//!
//! Every function here runs on the calling thread and allocates only what it returns. On the
//! i7-8700K a small region's `tighten` and `preprocess` together took 0.1 to 2 ms, the most for
//! the 2x pictures, against 3 to 36 ms for the model itself (`ocr-bench --paddle`, 2026-10-02,
//! which prints both for every picture).

use image::imageops::{crop_imm, resize, FilterType};
use image::RgbImage;

/// How [`tighten`] crops and enlarges a region before the model sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tighten {
    /// Pixels of background kept around the ink on every side (fewer where the region ends).
    pub margin_px: u32,
    /// The least the crop is enlarged by, even when it is already 48 pixels tall or more.
    pub min_up: f32,
}

impl Tighten {
    /// What Windows has used since the recogniser came in: 3 pixels of margin, at least twice
    /// the size. Its captures are at 1x; the golden test below pins what this gives.
    pub const WINDOWS: Tighten = Tighten { margin_px: 3, min_up: 2.0 };

    /// The same margin and the same least enlargement in POINTS, for a capture at `scale` pixels
    /// a point: margin `round(3 · scale)` pixels, least enlargement `max(2 / scale, 1)`. At 1x it
    /// is [`Tighten::WINDOWS`]; at 2x (a Retina capture) 6 pixels and no forced enlargement, since
    /// the glyphs are already twice the size. A scale below 1 or not finite counts as 1.
    ///
    /// Measured on the drawn 2x pictures only (`ocr-bench --paddle`, 2026-10-02): margin 3 or 6,
    /// least enlargement 1x or 2x, all four read the same 7 of 8 pictures with text right and
    /// invented text on the same 3 of 5 without, so the rule in points stands. No real Retina
    /// capture has been read with it yet.
    pub fn for_scale(scale: f64) -> Tighten {
        let s = if scale.is_finite() && scale >= 1.0 { scale } else { 1.0 };
        Tighten { margin_px: (3.0 * s).round() as u32, min_up: (2.0 / s).max(1.0) as f32 }
    }
}

/// How far a pixel must lie from the background (the mean of the four corners), as a distance in
/// RGB, to count as ink.
const INK_DISTANCE: f32 = 55.0;

/// The box around the ink: every pixel further than [`INK_DISTANCE`] from the mean of the four
/// corners, as `[x0, y0, x1, y1]`, inclusive. `None` when the picture is smaller than 3x3 or has
/// no such pixel.
pub fn ink_box(img: &RgbImage) -> Option<[u32; 4]> {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return None;
    }
    let bg = background(img);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    let mut found = false;
    for y in 0..h {
        for x in 0..w {
            if is_ink(img.get_pixel(x, y).0, bg) {
                found = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    found.then_some([x0, y0, x1, y1])
}

/// One flag a pixel column: whether any pixel in it is ink, by [`ink_box`]'s rule. Empty for a
/// picture smaller than 3x3. For the bench, which cuts a line between its words.
pub fn ink_columns(img: &RgbImage) -> Vec<bool> {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return Vec::new();
    }
    let bg = background(img);
    (0..w).map(|x| (0..h).any(|y| is_ink(img.get_pixel(x, y).0, bg))).collect()
}

/// The mean of the four corners, a picture of at least 1x1.
fn background(img: &RgbImage) -> [f32; 3] {
    let (w, h) = img.dimensions();
    let at = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let cs = [at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
    [
        (cs[0][0] + cs[1][0] + cs[2][0] + cs[3][0]) / 4.0,
        (cs[0][1] + cs[1][1] + cs[2][1] + cs[3][1]) / 4.0,
        (cs[0][2] + cs[1][2] + cs[2][2] + cs[3][2]) / 4.0,
    ]
}

fn is_ink(p: [u8; 3], bg: [f32; 3]) -> bool {
    let d = ((p[0] as f32 - bg[0]).powi(2) + (p[1] as f32 - bg[1]).powi(2) + (p[2] as f32 - bg[2]).powi(2)).sqrt();
    d > INK_DISTANCE
}

/// Crops to the ink ([`ink_box`]) with `t.margin_px` around it and enlarges the crop to 48
/// pixels tall, or by `t.min_up` if that is more — a cheap "poor man's detection", which is what
/// lets the recogniser read small, isolated digits. A picture with no ink, or smaller than 3x3,
/// comes back as it is. Lanczos3.
pub fn tighten(img: &RgbImage, t: Tighten) -> RgbImage {
    let (w, h) = img.dimensions();
    let Some([x0, y0, x1, y1]) = ink_box(img) else {
        return img.clone();
    };
    let m = t.margin_px as i32;
    let x0 = (x0 as i32 - m).max(0) as u32;
    let y0 = (y0 as i32 - m).max(0) as u32;
    let x1 = (x1 as i32 + m).min(w as i32 - 1) as u32;
    let y1 = (y1 as i32 + m).min(h as i32 - 1) as u32;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let cropped = crop_imm(img, x0, y0, cw, ch).to_image();
    let scale = (48.0 / ch as f32).max(t.min_up);
    let nw = ((cw as f32 * scale).round() as u32).max(1);
    let nh = ((ch as f32 * scale).round() as u32).max(1);
    resize(&cropped, nw, nh, FilterType::Lanczos3)
}

/// The model's input height in pixels.
pub const INPUT_H: u32 = 48;
/// The widest input the recogniser makes: anything wider is squeezed to this many pixels.
pub const INPUT_MAX_W: u32 = 320;

/// The model's input: resized to [`INPUT_H`] pixels tall keeping the aspect, at most
/// [`INPUT_MAX_W`] wide (a wider line is squeezed), and normalised to [-1, 1] in NCHW RGB — the
/// PP-OCR mobile recognition format. Returns the width and the data, `3 · 48 · width` values,
/// channel by channel, row by row. Triangle filter.
pub fn preprocess(img: &RgbImage) -> (u32, Vec<f32>) {
    let (w0, h0) = img.dimensions();
    let ratio = w0 as f32 / h0.max(1) as f32;
    let target_w = ((INPUT_H as f32 * ratio).ceil() as u32).clamp(1, INPUT_MAX_W);
    let resized = resize(img, target_w, INPUT_H, FilterType::Triangle);
    let (w, h) = (target_w as usize, INPUT_H as usize);
    let mut data = vec![0.0f32; 3 * h * w];
    for y in 0..h {
        for x in 0..w {
            let p = resized.get_pixel(x as u32, y as u32).0;
            for c in 0..3 {
                data[(c * h + y) * w + x] = (p[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
    }
    (target_w, data)
}

/// CTC greedy decoding of one line: per step the class with the largest value; the blank (class
/// 0) and repeats of the class before are dropped; class `i` is `dict[i - 1]`. `logits` holds
/// `steps · classes` values, step by step (the model's `[1, steps, classes]` output).
///
/// The second value is the "score": the smallest winning value over the characters kept, NaN
/// when nothing was kept. The model ends in a softmax (its last node; `tools/paddle-engines`
/// checks that every step sums to one), so it is the probability the model gave its least
/// certain character. Spaces are characters here; the caller trims.
///
/// Nothing filters by it: no minimum score separates what the model invents on ink that is not
/// text from what it reads right (`ocr-bench --paddle`, 2026-10-02: inventions on the bench's
/// pictures without text scored up to 0.74, a right lone "0" on a real sforzando capture 0.55).
pub fn decode(logits: &[f32], steps: usize, classes: usize, dict: &[String]) -> (String, f32) {
    let mut out = String::new();
    let mut score = f32::NAN;
    if classes == 0 || logits.len() < steps * classes {
        return (out, score);
    }
    let mut prev = usize::MAX;
    for ti in 0..steps {
        let row = &logits[ti * classes..(ti + 1) * classes];
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (ci, &v) in row.iter().enumerate() {
            if v > best_v {
                best_v = v;
                best = ci;
            }
        }
        if best != 0 && best != prev {
            if let Some(s) = dict.get(best - 1) {
                out.push_str(s);
                score = if score.is_nan() { best_v } else { score.min(best_v) };
            }
        }
        prev = best;
    }
    (out, score)
}

/// The recognition alphabet: one entry a line of `models/ppocr-dict.txt`, and the space PaddleOCR
/// appends (`use_space_char`) after them.
pub fn dict() -> Vec<String> {
    let mut d: Vec<String> = include_str!("../../models/ppocr-dict.txt").lines().map(str::to_string).collect();
    d.push(" ".to_string());
    d
}

/// Text as two readings are compared: trimmed, every whitespace character removed, the dashes
/// U+2010 to U+2015 and U+2212 (minus) made `-` (a value's minus comes out of either recogniser
/// as any of them), U+2018 to U+201B made `'`, U+201C and U+201D made `"`. Nothing else: case,
/// `O` against `0` and `l` against `1` stay apart.
pub fn fold(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            '\u{2018}'..='\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            other => other,
        })
        .collect()
}

/// Whether two readings say the same, by [`fold`].
pub fn same(a: &str, b: &str) -> bool {
    fold(a) == fold(b)
}

/// The widest ink, as a multiple of its height, that the recogniser is handed. Wider ink is
/// squeezed into [`INPUT_MAX_W`] pixels, and from some width on the model misreads it.
/// `ocr-bench --paddle` cut the words of `line@1x` and `line@2x` into every run of whole words
/// (Aileron, 12 points) and read each: every run up to 14.6 times as wide as tall was read right
/// (50 runs), the narrowest misread was 14.9 (2026-10-02, ONNX Runtime on Windows). 12 keeps a
/// fifth below that, for type denser than Aileron's and for macOS's renderer, which no run has
/// measured yet.
pub const MAX_ASPECT: f32 = 12.0;

/// Whether ink `content_w` by `content_h` pixels, in `lines` lines, is the recogniser's to read:
/// exactly one line, and no wider than [`MAX_ASPECT`] times its height.
pub fn fits(content_w: usize, content_h: usize, lines: usize) -> bool {
    lines == 1 && content_h > 0 && content_w as f32 <= MAX_ASPECT * content_h as f32
}

/// How many lines of ink a crop holds: `rows` has one flag a pixel row, whether any pixel in it
/// is ink, and runs of ink rows count as one line unless at least `min_gap` rows without ink
/// part them. The dot over an `i` is a row or two above its stem and stays with it; a rule drawn
/// under a word, or a second line, is further off. `min_gap` 0 counts as 1. On a Mac the gap is
/// the content crop's margin, `content_margin(scale)`: 3 pixels at 1x, 6 at 2x.
// Only a Mac's content crop counts lines; elsewhere only the tests use it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn ink_lines(rows: &[bool], min_gap: usize) -> usize {
    let min_gap = min_gap.max(1);
    let mut lines = 0;
    let mut gap = None;
    for &ink in rows {
        if ink {
            if gap.is_none_or(|g| g >= min_gap) {
                lines += 1;
            }
            gap = Some(0);
        } else if let Some(g) = gap.as_mut() {
            *g += 1;
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FNV-1a over the bits of every value, in order: a checksum that changes when one value
    /// changes in its last bit. For the golden test, which is Windows'.
    #[cfg(windows)]
    fn fnv(data: &[f32]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for v in data {
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }

    #[cfg(windows)]
    macro_rules! drawn {
        ($name:literal, $w:literal, $h:literal, $iw:literal, $sum:literal) => {
            ($name, include_bytes!(concat!("../../bench-data/ocr/", $name, ".png")).as_slice(), ($w, $h), $iw, $sum)
        };
    }

    /// Windows reads bit for bit as it did before this file was taken out of `paddle_ocr.rs`.
    /// Every value below was printed by the code as it was at 2fe8b8f (its `tighten` and
    /// `preprocess`, run over every picture `tools/ocr-fixtures/make.py` draws and over the
    /// warm-up's dummy), BEFORE the move: the size after `tighten`, the model input's width, and
    /// a checksum of the input's every value. The same code with `Tighten::WINDOWS` must give
    /// them again. A redrawn picture changes its row; take the new row from the code before the
    /// change, never from the code after it. (`ocr::bench`'s tests check that every drawn picture
    /// has a row here.)
    ///
    /// Windows only: Lanczos3 calls `sin`, which another platform's maths library may round
    /// differently in the last bit, and what is pinned here is what Windows reads.
    #[cfg(windows)]
    #[test]
    fn windows_reads_bit_for_bit_as_before_the_move() {
        // The picture, its PNG, the size after `tighten`, the input's width, the checksum.
        type Row = (&'static str, &'static [u8], (u32, u32), u32, u64);
        let rows: &[Row] = &[
            drawn!("clipped-label@1x", 343, 48, 320, 0xc293b1c87fc599c8),
            drawn!("clipped-label@2x", 401, 48, 320, 0x26dfde01aa42a554),
            drawn!("field-64@1x", 96, 48, 96, 0x6fe759d12e5d23fe),
            drawn!("field-64@2x", 156, 76, 99, 0xe46b639a7b6aac4c),
            drawn!("field-DEF@1x", 83, 48, 83, 0x17e9f7a8c707627e),
            drawn!("field-DEF@2x", 148, 84, 85, 0xc69482812a538d72),
            drawn!("field-empty@1x", 311, 48, 311, 0x1f3c58d87312021d),
            drawn!("field-empty@2x", 492, 64, 320, 0x36b951dc33e3d196),
            drawn!("line@1x", 1152, 48, 320, 0xade37b08017527f5),
            drawn!("line@2x", 1688, 58, 320, 0x32b3eecf0f1f991d),
            drawn!("lone-1@1x", 48, 48, 48, 0x4c545e49f1cec6b8),
            drawn!("lone-1@2x", 76, 76, 48, 0x40defd336036d4cd),
            drawn!("none-bars@1x", 96, 48, 96, 0x666a15f153f9c29b),
            drawn!("none-bars@2x", 156, 76, 99, 0xf8680bd17a0aa67b),
            drawn!("none-caret@1x", 21, 48, 21, 0x03364750fd30824f),
            drawn!("none-caret@2x", 16, 52, 15, 0xafe0c8a498748d6b),
            drawn!("none-icon@1x", 48, 48, 48, 0xe869c3cdc851a851),
            drawn!("none-icon@2x", 76, 76, 48, 0xf56a4455154a6c9f),
            drawn!("none-well@1x", 96, 48, 96, 0xf290425f4d3533d8),
            drawn!("none-well@2x", 156, 76, 99, 0xafc41a13b4cf8e82),
            drawn!("val-050@1x", 106, 48, 106, 0xaad5319f9c971f7b),
            drawn!("val-050@2x", 172, 76, 109, 0x44a9fbc76aaa5815),
            drawn!("val-ct@1x", 122, 48, 122, 0x0a910b80731fe562),
            drawn!("val-ct@2x", 124, 48, 124, 0xdd762357656d687f),
            drawn!("val-db@1x", 162, 48, 162, 0x20d6ca493eeef865),
            drawn!("val-db@2x", 165, 48, 165, 0x5526b1ef7b599d0f),
            drawn!("val-minus12@1x", 96, 48, 96, 0xd0c0645206c69a98),
            drawn!("val-minus12@2x", 156, 76, 99, 0xb84e6152a641cc69),
        ];
        let check = |name: &str, img: &RgbImage, size: (u32, u32), width: u32, sum: u64| {
            let t = tighten(img, Tighten::WINDOWS);
            assert_eq!(t.dimensions(), size, "{name}: the size after tighten");
            let (w, data) = preprocess(&t);
            assert_eq!(w, width, "{name}: the model input's width");
            assert_eq!(data.len(), 3 * INPUT_H as usize * w as usize, "{name}");
            assert_eq!(fnv(&data), sum, "{name}: the model input");
        };
        for (name, png, size, width, sum) in rows {
            let img = image::load_from_memory(png).expect(name).to_rgb8();
            check(name, &img, *size, *width, *sum);
        }
        // The warm-up's dummy: no ink, so `tighten` hands it back as it is.
        let dummy = RgbImage::from_pixel(40, 16, image::Rgb([20, 20, 20]));
        check("warm-up dummy", &dummy, (40, 16), 120, 0xe3063f5d0caccf25);
    }

    #[test]
    fn for_scale_is_windows_at_1x_and_the_same_in_points_at_2x() {
        assert_eq!(Tighten::for_scale(1.0), Tighten::WINDOWS);
        assert_eq!(Tighten::for_scale(2.0), Tighten { margin_px: 6, min_up: 1.0 });
        assert_eq!(Tighten::for_scale(1.5), Tighten { margin_px: 5, min_up: 2.0 / 1.5 });
        assert_eq!(Tighten::for_scale(0.5), Tighten::WINDOWS);
        assert_eq!(Tighten::for_scale(f64::NAN), Tighten::WINDOWS);
    }

    #[test]
    fn tighten_crops_to_the_ink_and_its_margin() {
        // A 2x3 block of ink in a 20x10 picture: 3 pixels of margin, then 48 pixels tall.
        let mut img = RgbImage::from_pixel(20, 10, image::Rgb([200, 200, 200]));
        for (x, y) in [(8, 4), (9, 4), (8, 5), (9, 5), (8, 6), (9, 6)] {
            img.put_pixel(x, y, image::Rgb([0, 0, 0]));
        }
        assert_eq!(ink_box(&img), Some([8, 4, 9, 6]));
        // 8x9 with the margin (rows 1..=9), enlarged by 48/9.
        assert_eq!(tighten(&img, Tighten::WINDOWS).dimensions(), (43, 48));
        // A 6-pixel margin reaches the edges: rows 0..=9 and columns 2..=15.
        assert_eq!(tighten(&img, Tighten { margin_px: 6, min_up: 1.0 }).dimensions(), (67, 48));
        // No ink, and too small: handed back as they are.
        let flat = RgbImage::from_pixel(9, 9, image::Rgb([20, 20, 20]));
        assert_eq!(ink_box(&flat), None);
        assert_eq!(tighten(&flat, Tighten::WINDOWS), flat);
        assert_eq!(ink_box(&RgbImage::new(2, 9)), None);
    }

    #[test]
    fn preprocess_is_48_tall_at_most_320_wide_and_in_minus_one_to_one() {
        let white = RgbImage::from_pixel(10, 24, image::Rgb([255, 255, 255]));
        let (w, data) = preprocess(&white);
        assert_eq!(w, 20);
        assert_eq!(data.len(), 3 * 48 * 20);
        assert!(data.iter().all(|v| *v == 1.0));
        let black = RgbImage::from_pixel(1000, 10, image::Rgb([0, 0, 0]));
        let (w, data) = preprocess(&black);
        assert_eq!(w, INPUT_MAX_W, "squeezed");
        assert!(data.iter().all(|v| *v == -1.0));
        // NCHW: the red plane first.
        let red = RgbImage::from_pixel(4, 48, image::Rgb([255, 0, 0]));
        let (w, data) = preprocess(&red);
        let plane = 48 * w as usize;
        assert!(data[..plane].iter().all(|v| *v == 1.0) && data[plane..].iter().all(|v| *v == -1.0));
    }

    #[test]
    fn decode_drops_blanks_and_repeats_and_scores_the_weakest_kept_character() {
        let dict: Vec<String> = ["1", "2", " "].iter().map(|s| s.to_string()).collect();
        // Classes: blank, "1", "2", " ". Steps: 1 1 blank 1 2 " " 2 blank.
        let steps = [
            [0.1, 0.8, 0.05, 0.05],
            [0.1, 0.9, 0.0, 0.0],
            [0.7, 0.1, 0.1, 0.1],
            [0.2, 0.6, 0.1, 0.1],
            [0.1, 0.1, 0.75, 0.05],
            [0.2, 0.1, 0.1, 0.6],
            [0.1, 0.1, 0.95, 0.0],
            [0.9, 0.0, 0.1, 0.0],
        ];
        let logits: Vec<f32> = steps.iter().flatten().copied().collect();
        let (text, score) = decode(&logits, 8, 4, &dict);
        assert_eq!(text, "112 2");
        // The repeat of the first "1" (0.9) is dropped, not scored; the weakest kept is 0.6.
        assert_eq!(score, 0.6);
        // Nothing but blanks: nothing, and no score.
        let blank: Vec<f32> = [[0.9, 0.1, 0.0, 0.0]; 3].iter().flatten().copied().collect();
        let (text, score) = decode(&blank, 3, 4, &dict);
        assert!(text.is_empty() && score.is_nan());
        // A class past the dictionary is dropped, and too short an output reads as nothing.
        assert_eq!(decode(&[0.0, 0.0, 0.0, 0.0, 1.0], 1, 5, &dict).0, "");
        assert_eq!(decode(&[0.0; 3], 1, 4, &dict).0, "");
    }

    #[test]
    fn the_dictionary_ends_with_the_space() {
        let d = dict();
        assert_eq!(d.len(), 437);
        assert_eq!(d.last().map(String::as_str), Some(" "));
        assert_eq!(d[0], "0");
    }

    #[test]
    fn same_ignores_spaces_and_dash_and_quote_forms_and_nothing_else() {
        assert!(same("0.00 dB", "0.00dB"));
        assert!(same(" 64 ", "64"));
        assert!(same("\u{2212}12", "-12"));
        assert!(same("\u{2010}12", "\u{2014}12"));
        assert!(same("\u{201C}Legato\u{201D}", "\"Legato\""));
        assert!(same("it\u{2019}s", "it's"));
        assert!(!same("O", "0"));
        assert!(!same("l", "1"));
        assert!(!same("DEF", "def"));
        assert!(!same("-12", "12"), "a lost minus is a different value");
        assert!(!same("0.50", "050"), "and so is a lost point");
        assert!(same("\u{2013}12", "-12"));
        assert!(same("\u{2011}12", "\u{2015}12"));
        assert_eq!(fold("  +3 ct\t"), "+3ct");
    }

    #[test]
    fn fits_takes_one_line_up_to_the_widest_ratio() {
        let h = 10;
        let widest = (MAX_ASPECT * h as f32) as usize;
        assert!(fits(widest, h, 1));
        assert!(!fits(widest + 1, h, 1));
        assert!(fits(1, h, 1));
        assert!(!fits(20, h, 2), "two lines");
        assert!(!fits(20, h, 0), "no line");
        assert!(!fits(20, 0, 1), "no height");
    }

    #[test]
    fn ink_lines_count_runs_apart_by_the_gap() {
        let rows = |s: &str| s.chars().map(|c| c == '#').collect::<Vec<bool>>();
        assert_eq!(ink_lines(&rows(""), 3), 0);
        assert_eq!(ink_lines(&rows("...."), 3), 0);
        assert_eq!(ink_lines(&rows("..####.."), 3), 1);
        // The dot of an i, two rows above its stem: one line.
        assert_eq!(ink_lines(&rows("#..#####"), 3), 1);
        // A rule three rows under the word: two.
        assert_eq!(ink_lines(&rows("#####...#"), 3), 2);
        assert_eq!(ink_lines(&rows("#####...#"), 4), 1);
        assert_eq!(ink_lines(&rows("##.##.##..##"), 0), 4, "a gap of 0 counts as 1");
        assert_eq!(ink_lines(&rows("##...##...##"), 3), 3);
    }
}
