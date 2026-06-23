//! Secondary OCR engine: a PaddleOCR recognition model run in-process via ONNX
//! Runtime (the `ort` crate), recognition-only. Windows.Media.Ocr is fast but
//! blind to isolated single glyphs (a lone "1"); this neural recognizer reads
//! them. Used only as a fallback when the system OCR returns nothing on a small
//! region — so the fast path stays the OS engine and this pays its cost rarely.
//!
//! The recognition model and its dictionary are embedded in the binary (via
//! `include_bytes!`), so the app stays fully self-contained and portable.

use std::sync::{Mutex, OnceLock};

use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;

use super::CapturedImage;

struct Engine {
    session: Mutex<Session>,
    /// Recognition alphabet: class index `i` (1-based, after the CTC blank at 0)
    /// maps to `dict[i - 1]`.
    dict: Vec<String>,
}

static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();

fn engine() -> Option<&'static Engine> {
    ENGINE.get_or_init(|| init().ok()).as_ref()
}

/// Recognition model + dictionary, embedded so the app is self-contained.
const MODEL: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocr-rec.onnx"));
const DICT: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/models/ppocr-dict.txt"));

fn init() -> anyhow::Result<Engine> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(1)?
        .commit_from_memory(MODEL)?;
    let mut dict: Vec<String> = DICT.lines().map(|l| l.to_string()).collect();
    dict.push(" ".to_string()); // PaddleOCR appends a space char (use_space_char)
    Ok(Engine {
        session: Mutex::new(session),
        dict,
    })
}

/// Recognizes text in a captured region via the neural recognizer, or `None`
/// when the engine is unavailable or finds nothing.
pub fn recognize(cap: &CapturedImage) -> Option<String> {
    let eng = engine()?;

    let rgba = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone())?;
    let rgb = image::RgbImage::from_fn(cap.w, cap.h, |x, y| {
        let p = rgba.get_pixel(x, y).0;
        image::Rgb([p[0], p[1], p[2]])
    });

    let tight = tighten(&rgb);
    let input = preprocess(&tight);
    let tensor = Tensor::from_array(input).ok()?;

    let mut session = eng.session.lock().ok()?;
    let outputs = session.run(ort::inputs![tensor]).ok()?;
    let logits = outputs[0].try_extract_array::<f32>().ok()?;
    let text = decode(&logits, &eng.dict);

    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Loads the model and warms the inference graph on a background thread, so the
/// first real OCR doesn't pay the (one-time, hundreds-of-ms) init cost on the
/// hot path. No-op if the model files aren't present. Returns the thread handle:
/// the caller MUST join it before the process exits, or a fast exit can tear the
/// process down while this thread is still inside ONNX Runtime init, racing ort's
/// static cleanup (an access violation — the "headless segfault").
pub fn warmup() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        let Some(eng) = engine() else { return };
        let dummy = image::RgbImage::from_pixel(40, 16, image::Rgb([20, 20, 20]));
        let input = preprocess(&tighten(&dummy));
        if let Ok(tensor) = Tensor::from_array(input) {
            if let Ok(mut session) = eng.session.lock() {
                let _ = session.run(ort::inputs![tensor]);
            }
        }
    })
}

/// Crops to the content bounding box (pixels far from the corner background) and
/// upscales so tiny glyphs become large — a cheap "poor man's detection" that is
/// what lets the recognizer read small, isolated digits.
fn tighten(img: &image::RgbImage) -> image::RgbImage {
    use image::imageops::{resize, FilterType};
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return img.clone();
    }
    let at = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let cs = [at(0, 0), at(w - 1, 0), at(0, h - 1), at(w - 1, h - 1)];
    let bg = [
        (cs[0][0] + cs[1][0] + cs[2][0] + cs[3][0]) / 4.0,
        (cs[0][1] + cs[1][1] + cs[2][1] + cs[3][1]) / 4.0,
        (cs[0][2] + cs[1][2] + cs[2][2] + cs[3][2]) / 4.0,
    ];
    let thr = 55.0f32;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    let mut found = false;
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            let d = ((p[0] as f32 - bg[0]).powi(2)
                + (p[1] as f32 - bg[1]).powi(2)
                + (p[2] as f32 - bg[2]).powi(2))
            .sqrt();
            if d > thr {
                found = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !found {
        return img.clone();
    }
    let m = 3i32;
    let x0 = (x0 as i32 - m).max(0) as u32;
    let y0 = (y0 as i32 - m).max(0) as u32;
    let x1 = (x1 as i32 + m).min(w as i32 - 1) as u32;
    let y1 = (y1 as i32 + m).min(h as i32 - 1) as u32;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let cropped = image::imageops::crop_imm(img, x0, y0, cw, ch).to_image();
    let scale = (48.0 / ch as f32).max(2.0);
    let nw = ((cw as f32 * scale).round() as u32).max(1);
    let nh = ((ch as f32 * scale).round() as u32).max(1);
    resize(&cropped, nw, nh, FilterType::Lanczos3)
}

/// Resizes to height 48 (preserving aspect, width capped at 320) and normalizes
/// to [-1, 1] in NCHW RGB — the PP-OCR mobile recognition input format.
fn preprocess(img: &image::RgbImage) -> ndarray::Array4<f32> {
    let (w0, h0) = img.dimensions();
    let target_h = 48u32;
    let ratio = w0 as f32 / h0.max(1) as f32;
    let target_w = ((target_h as f32 * ratio).ceil() as u32).clamp(1, 320);
    let resized = image::imageops::resize(img, target_w, target_h, image::imageops::FilterType::Triangle);

    let mut arr = ndarray::Array4::<f32>::zeros((1, 3, target_h as usize, target_w as usize));
    for y in 0..target_h {
        for x in 0..target_w {
            let p = resized.get_pixel(x, y).0;
            for c in 0..3 {
                arr[[0, c, y as usize, x as usize]] = (p[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
    }
    arr
}

/// CTC greedy decode: argmax per timestep over the class axis, drop the blank
/// (index 0) and collapse repeats, mapping class `i` to `dict[i - 1]`.
fn decode(logits: &ndarray::ArrayViewD<f32>, dict: &[String]) -> String {
    let shape = logits.shape();
    if shape.len() != 3 {
        return String::new();
    }
    let (t, c) = (shape[1], shape[2]);
    let mut out = String::new();
    let mut prev = usize::MAX;
    for ti in 0..t {
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for ci in 0..c {
            let v = logits[[0, ti, ci]];
            if v > best_v {
                best_v = v;
                best = ci;
            }
        }
        if best != 0 && best != prev {
            if let Some(s) = dict.get(best - 1) {
                out.push_str(s);
            }
        }
        prev = best;
    }
    out
}
