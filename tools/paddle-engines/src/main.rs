//! The neural recogniser's model under ONNX Runtime and under tract, on the same input.
//!
//!     paddle-engines [--pictures DIR] [--rounds N]
//!
//! Every PNG in crates/host/bench-data/ocr/ (and in DIR) is prepared once by the recogniser's own
//! preprocessing (crates/host/src/ocr/paddle_pre.rs, borrowed here as it is) and handed to each
//! engine: with Windows' crop, and the 2x pictures also with the Retina crop. For each input it
//! prints every engine's text and score, the largest difference between its output and ONNX
//! Runtime's, and its time (the median of N runs after 3 discarded, one thread each); before that,
//! what making each engine cost. The last lines judge the first two of the three criteria
//! README.md lists — the same text on every input, and tract at most twice ONNX Runtime's time —
//! for tract as the application would run it: one plan for every width.
//!
//! tract runs twice: with one optimised plan whose input width is a symbol, resolved at each run
//! (one model for every region, as the application has one session), and with a plan optimised
//! for each width it meets (faster per run, but each new width costs a whole optimisation, which
//! the first, discarded runs absorb and the last lines report). The second says how fast tract can
//! be on this model, not how the application could use it.
//!
//! A measurement, run by hand in a release build. It opens nothing but the files it reads.

#[path = "../../../crates/host/src/ocr/paddle_pre.rs"]
#[allow(dead_code)]
mod pre;

use std::path::{Path, PathBuf};
use std::time::Instant;

use pre::Tighten;

/// The recognition model, the file the application embeds.
const MODEL: &[u8] = include_bytes!("../../../crates/host/models/ppocr-rec.onnx");

/// The model's output for one input: `steps · classes` values.
struct Output {
    steps: usize,
    classes: usize,
    logits: Vec<f32>,
}

trait Engine {
    fn run(&mut self, width: u32, input: &[f32]) -> Result<Output, String>;
}

#[cfg(feature = "ort")]
mod onnx_runtime {
    use super::{Engine, Output, MODEL};
    use ort::session::builder::GraphOptimizationLevel;
    use ort::session::Session;
    use ort::value::Tensor;

    /// As the application makes it (crates/host/src/backend/paddle_ocr.rs, `init`).
    pub struct Ort(Session);

    pub fn make() -> Result<Ort, String> {
        let s = Session::builder()
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.with_intra_threads(1))
            .and_then(|b| b.commit_from_memory(MODEL))
            .map_err(|e| e.to_string())?;
        Ok(Ort(s))
    }

    impl Engine for Ort {
        fn run(&mut self, width: u32, input: &[f32]) -> Result<Output, String> {
            let tensor = Tensor::from_array(([1usize, 3, 48, width as usize], input.to_vec())).map_err(|e| e.to_string())?;
            let outputs = self.0.run(ort::inputs![tensor]).map_err(|e| e.to_string())?;
            let (shape, logits) = outputs[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
            if shape.len() != 3 {
                return Err(format!("an output of shape {shape:?}"));
            }
            Ok(Output { steps: shape[1] as usize, classes: shape[2] as usize, logits: logits.to_vec() })
        }
    }
}

#[cfg(feature = "tract")]
mod pure_rust {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;

    use super::{Engine, Output, MODEL};
    use tract_onnx::prelude::*;

    type Plan = Arc<TypedRunnableModel>;

    /// The model with its input fixed to `[1, 3, 48, width]`, `width` a number or a symbol.
    fn plan(width: Option<usize>) -> TractResult<Plan> {
        let mut model = tract_onnx::onnx().model_for_read(&mut &MODEL[..])?;
        let w = match width {
            Some(w) => w.to_dim(),
            None => model.sym("W").to_dim(),
        };
        model.set_input_fact(0, f32::fact([1.to_dim(), 3.to_dim(), 48.to_dim(), w]).into())?;
        model.into_optimized()?.into_runnable()
    }

    fn run_plan(plan: &Plan, width: u32, input: &[f32]) -> TractResult<Output> {
        let tensor = Tensor::from_shape(&[1, 3, 48, width as usize], input)?;
        let mut out = plan.run(tvec!(tensor.into()))?;
        let first = out.remove(0).into_tensor();
        let view = first.to_plain_array_view::<f32>()?;
        let shape = view.shape().to_vec();
        if shape.len() != 3 {
            return Err(TractError::msg(format!("an output of shape {shape:?}")));
        }
        Ok(Output { steps: shape[1], classes: shape[2], logits: view.iter().copied().collect() })
    }

    /// One optimised plan for every width: the input's last dimension is a symbol, resolved at
    /// each run, as one session serves every region in the application.
    pub struct Tract(Plan);

    pub fn make() -> Result<Tract, String> {
        plan(None).map(Tract).map_err(|e| format!("{e:?}"))
    }

    impl Engine for Tract {
        fn run(&mut self, width: u32, input: &[f32]) -> Result<Output, String> {
            run_plan(&self.0, width, input).map_err(|e| format!("{e:?}"))
        }
    }

    /// A plan optimised for each width met, kept: what tract costs at its fastest.
    #[derive(Default)]
    pub struct TractPerWidth {
        plans: HashMap<u32, Plan>,
        /// Milliseconds spent making plans, and how many.
        pub making: (f64, usize),
    }

    impl Engine for TractPerWidth {
        fn run(&mut self, width: u32, input: &[f32]) -> Result<Output, String> {
            if !self.plans.contains_key(&width) {
                let t = Instant::now();
                let p = plan(Some(width as usize)).map_err(|e| format!("{e:?}"))?;
                self.making.0 += t.elapsed().as_secs_f64() * 1000.0;
                self.making.1 += 1;
                self.plans.insert(width, p);
            }
            run_plan(&self.plans[&width], width, input).map_err(|e| format!("{e:?}"))
        }
    }
}

/// One input every engine is given.
struct Input {
    label: String,
    width: u32,
    data: Vec<f32>,
}

fn pictures(dir: &Path, into: &mut Vec<Input>) -> Result<(), String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    files.sort();
    for f in files {
        let name = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let img = image::open(&f).map_err(|e| format!("{}: {e}", f.display()))?.to_rgb8();
        let mut settings = vec![("Windows crop", Tighten::WINDOWS)];
        if name.ends_with("@2x") {
            settings.push(("Retina crop", Tighten::for_scale(2.0)));
        }
        for (what, t) in settings {
            let (width, data) = pre::preprocess(&pre::tighten(&img, t));
            into.push(Input { label: format!("{name} ({what})"), width, data });
        }
    }
    Ok(())
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    if v.is_empty() {
        return f64::NAN;
    }
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// What one engine made of one input.
struct Read {
    text: String,
    score: f32,
    ms: f64,
    out: Output,
}

fn read(engine: &mut dyn Engine, input: &Input, dict: &[String], rounds: usize) -> Result<Read, String> {
    for _ in 0..3 {
        engine.run(input.width, &input.data)?;
    }
    let mut times = Vec::with_capacity(rounds);
    let mut last = None;
    for _ in 0..rounds {
        let t = Instant::now();
        let out = engine.run(input.width, &input.data)?;
        times.push(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(out);
    }
    let out = last.ok_or("no run")?;
    let (text, score) = pre::decode(&out.logits, out.steps, out.classes, dict);
    Ok(Read { text: text.trim().to_string(), score, ms: median(times), out })
}

/// What one engine came to over every input, against ONNX Runtime.
#[derive(Default)]
struct Tally {
    differ: Vec<String>,
    failed: Vec<String>,
    ms: f64,
    reference_ms: f64,
    worst: (f64, String),
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut extra: Option<PathBuf> = None;
    let mut rounds = 20usize;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--pictures" => extra = args.next().map(PathBuf::from),
            "--rounds" => rounds = args.next().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(20),
            other => {
                eprintln!("unknown argument '{other}'\n\nusage: paddle-engines [--pictures DIR] [--rounds N]");
                std::process::exit(2);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/host/bench-data/ocr");
    let mut inputs = Vec::new();
    let mut folders = vec![root];
    folders.extend(extra);
    for dir in &folders {
        if let Err(e) = pictures(dir, &mut inputs) {
            eprintln!("{e}");
            std::process::exit(2);
        }
    }
    let dict = pre::dict();
    println!("paddle-engines: {} inputs from {} folder(s), {rounds} timed runs each after 3 discarded", inputs.len(), folders.len());

    #[cfg(not(all(feature = "ort", feature = "tract")))]
    {
        println!("verdict | built without both engines: nothing to compare (README.md, criterion 3, is what such a build is for)");
        std::process::exit(1);
    }
    #[cfg(all(feature = "ort", feature = "tract"))]
    {
        let t = Instant::now();
        let mut ort = match onnx_runtime::make() {
            Ok(e) => e,
            Err(e) => {
                println!("load | ONNX Runtime | FAILED: {e}");
                std::process::exit(1);
            }
        };
        println!("load | ONNX Runtime | the session made in {:.0} ms (level 3, one intra-op thread)", t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        let mut tract = match pure_rust::make() {
            Ok(e) => e,
            Err(e) => {
                println!("load | tract | FAILED: {e}");
                std::process::exit(1);
            }
        };
        println!("load | tract | read, typed and optimised in {:.0} ms (one plan, the width a symbol)", t.elapsed().as_secs_f64() * 1000.0);
        let mut per_width = pure_rust::TractPerWidth::default();

        let mut tallies = [Tally::default(), Tally::default()];
        let mut softmax = true;
        for input in &inputs {
            let reference = match read(&mut ort, input, &dict, rounds) {
                Ok(r) => r,
                Err(e) => {
                    println!("input | {} | ONNX Runtime failed: {e}", input.label);
                    for t in &mut tallies {
                        t.failed.push(input.label.clone());
                    }
                    continue;
                }
            };
            // Whether each step's values sum to one: a softmax at the end makes the score a probability.
            softmax &= reference.out.logits.chunks(reference.out.classes.max(1)).all(|c| (c.iter().sum::<f32>() - 1.0).abs() < 1e-3);
            let mut line = format!(
                "input | {} | {} px wide | ONNX Runtime \"{}\" score {:.4}, {:.2} ms",
                input.label, input.width, reference.text, reference.score, reference.ms
            );
            let others: [(&str, &mut dyn Engine); 2] = [("tract", &mut tract), ("tract, a plan per width", &mut per_width)];
            for ((name, engine), tally) in others.into_iter().zip(tallies.iter_mut()) {
                match read(engine, input, &dict, rounds) {
                    Ok(r) => {
                        let gap = if r.out.logits.len() == reference.out.logits.len() {
                            r.out.logits.iter().zip(&reference.out.logits).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max)
                        } else {
                            f32::INFINITY
                        };
                        let ratio = r.ms / reference.ms;
                        if r.text != reference.text {
                            tally.differ.push(format!("{}: \"{}\" against \"{}\"", input.label, r.text, reference.text));
                        }
                        if ratio > tally.worst.0 {
                            tally.worst = (ratio, input.label.clone());
                        }
                        tally.ms += r.ms;
                        tally.reference_ms += reference.ms;
                        line.push_str(&format!(
                            " | {name} \"{}\" score {:.4}, {:.2} ms ({ratio:.2}x), {}, largest output difference {gap:.2e}",
                            r.text,
                            r.score,
                            r.ms,
                            if r.text == reference.text { "same text" } else { "DIFFERENT TEXT" }
                        ));
                    }
                    Err(e) => {
                        tally.failed.push(input.label.clone());
                        line.push_str(&format!(" | {name} FAILED: {e}"));
                    }
                }
            }
            println!("{line}");
        }
        println!("model | every output step sums to one (the model ends in a softmax): {}", if softmax { "yes" } else { "no" });
        println!(
            "tract, a plan per width | {} plans made in {:.0} ms, {:.0} ms each; the medians summed: {:.1} ms against ONNX Runtime's {:.1} ({:.2}x)",
            per_width.making.1,
            per_width.making.0,
            per_width.making.0 / per_width.making.1.max(1) as f64,
            tallies[1].ms,
            tallies[1].reference_ms,
            tallies[1].ms / tallies[1].reference_ms
        );
        let t = &tallies[0];
        println!(
            "criterion 1 | tract reads the same text as ONNX Runtime on every input: {} ({} of {} inputs the same{}{})",
            if t.differ.is_empty() && t.failed.is_empty() { "MET" } else { "NOT MET" },
            inputs.len() - t.failed.len() - t.differ.len(),
            inputs.len(),
            if t.differ.is_empty() { String::new() } else { format!("; differ: {}", t.differ.join("; ")) },
            if t.failed.is_empty() { String::new() } else { format!("; failed: {}", t.failed.join(", ")) }
        );
        let ratio = t.ms / t.reference_ms;
        println!(
            "criterion 2 | tract at most twice ONNX Runtime's time: {} (the medians summed over every input: ONNX Runtime {:.1} ms, tract {:.1} ms, {ratio:.2}x; the worst input {:.2}x, {})",
            if ratio <= 2.0 { "MET" } else { "NOT MET" },
            t.reference_ms,
            t.ms,
            t.worst.0,
            t.worst.1
        );
        println!("criterion 3 | is not measured here: README.md says how (cargo check for both Mac targets)");
    }
}
