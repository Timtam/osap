//! `automation-platform ocr-bench`: what Apple Vision costs on this Mac, read off fixed pictures.
//!
//! A measurement, not a feature. It answers the questions a tester's Intel MacBook Air raised —
//! a small read-out took 143 to 786 ms there, against 22 to 42 ms on the Windows machine and 25
//! to 56 ms on a warm Mac mini M1 — with numbers a machine produces by itself: what the machine
//! is, what one Vision pass costs under today's settings and under each setting that might
//! change it, how many passes the retry ladder makes for one read, what the first pass on a
//! thread or in a process costs, what a pass costs after the recogniser has sat idle, and what
//! two passes at once cost. The same pictures are read on a CI runner, on the Air and on an
//! Apple-silicon Mac, so only the machine differs. It opens no window, captures no screen,
//! needs no permission and never speaks; it does not open the application's log either.
//!
//! This is the pure half: the pictures and the text each must read, the command line, the
//! order the passes run in, the statistics, the verdict and every line and table it prints.
//! Std only, so its tests run on Windows, and `crates/macos-check` borrows it. The half that
//! runs Vision is `backend/macos/ocr/bench.rs`, a child of the recogniser it measures, so it
//! calls the production code itself and names each deviation from it.
//!
//! The pictures are drawn by `tools/ocr-fixtures/make.py` and carried inside the executable.

// Everywhere but on a Mac only the command line and the "macOS only" answer are used; the rest
// is compiled there for its tests.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One fixed picture and the text it must read.
pub struct Fixture {
    /// The layout, e.g. `field-64`.
    pub name: &'static str,
    /// 1 or 2: drawn for a standard display or for a Retina one (twice the pixels, the type at
    /// twice the size).
    pub scale: u32,
    /// The region it stands for, in points. A region of 400x200 or less takes the small-text
    /// path (content crop, enlargement, the ladder); a larger one goes to Vision as it is.
    pub w_pt: i32,
    pub h_pt: i32,
    /// What it reads, compared after whitespace is collapsed, case and all.
    pub expected: &'static str,
    /// The PNG, as `tools/ocr-fixtures/make.py` drew it.
    pub png: &'static [u8],
}

impl Fixture {
    /// `field-64@2x`.
    pub fn label(&self) -> String {
        format!("{}@{}x", self.name, self.scale)
    }

    /// The picture's size in pixels.
    pub fn px(&self) -> (usize, usize) {
        ((self.w_pt as u32 * self.scale) as usize, (self.h_pt as u32 * self.scale) as usize)
    }
}

macro_rules! fixture {
    ($name:literal, $scale:literal, $w:literal, $h:literal, $text:literal) => {
        Fixture {
            name: $name,
            scale: $scale,
            w_pt: $w,
            h_pt: $h,
            expected: $text,
            png: include_bytes!(concat!("../../bench-data/ocr/", $name, "@", $scale, "x.png")),
        }
    };
}

/// Every picture, each layout at 1x and 2x. The fields imitate sforzando's read-outs (a black
/// well in grey chrome, the digits about 11 pixels tall at 1x as in sforzando's own): `field-64`
/// its Polyphony, `field-DEF` its Pitchbend range, `lone-1` a single glyph, the ladder's case,
/// `field-empty` its Instrument. `line` is wider than 400 points and so is handed to Vision as it
/// is. Kept in step with `LAYOUTS` in make.py.
pub static FIXTURES: &[Fixture] = &[
    fixture!("field-64", 1, 40, 20, "64"),
    fixture!("field-64", 2, 40, 20, "64"),
    fixture!("field-DEF", 1, 38, 22, "DEF"),
    fixture!("field-DEF", 2, 38, 22, "DEF"),
    fixture!("lone-1", 1, 20, 20, "1"),
    fixture!("lone-1", 2, 20, 20, "1"),
    fixture!("field-empty", 1, 123, 23, "empty"),
    fixture!("field-empty", 2, 123, 23, "empty"),
    fixture!("line", 1, 520, 24, "Instrument Polyphony Pitchbend Range Velocity Curve Release Time Volume"),
    fixture!("line", 2, 520, 24, "Instrument Polyphony Pitchbend Range Velocity Curve Release Time Volume"),
];

/// The picture with this label.
pub fn fixture(label: &str) -> Option<&'static Fixture> {
    FIXTURES.iter().find(|f| f.label() == label)
}

/// The picture the single-picture sections read: the Polyphony field at Retina size, the
/// tester's commonest slow read.
pub const PROBE: &str = "field-64@2x";

/// The width and height a PNG's header gives, or `None` when this is not a PNG.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((w, h))
}

// ── What is varied ───────────────────────────────────────────────────────────────────────────

/// Which compute device a request is pinned to (macOS 14 and later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Cpu,
    Gpu,
    NeuralEngine,
}

impl Device {
    /// The kind of a Core ML device, from its class name (`MLCPUComputeDevice`, …).
    pub fn of_class(name: &str) -> Option<Device> {
        if name.contains("NeuralEngine") {
            Some(Device::NeuralEngine)
        } else if name.contains("GPU") {
            Some(Device::Gpu)
        } else if name.contains("CPU") {
            Some(Device::Cpu)
        } else {
            None
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Device::Cpu => "CPU",
            Device::Gpu => "GPU",
            Device::NeuralEngine => "Neural Engine",
        }
    }
}

/// One deviation from today's request, and only one: the variants are compared with `prod`, so
/// each must differ from it in exactly the thing it is named for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tweak {
    /// Today's request, as `host.ocr.recognize` sends it.
    None,
    /// Today's request again, under another name: the control. Any verdict on it is noise.
    Control,
    /// The fast recognition level.
    Fast,
    /// This request revision, named explicitly.
    Revision(usize),
    /// This `minimumTextHeight` instead of 0.
    MinTextHeight(f32),
    /// One request kept on the thread and handed to a new handler for each picture.
    Reuse,
    /// The preprocessing enlarges the ink toward this many pixels instead of 64.
    TargetPx(usize),
    /// Every compute stage that offers this device is pinned to it.
    Device(Device),
    /// This one recognition language, as `host.ocr.read` sends it.
    Lang(&'static str),
}

impl Tweak {
    /// Whether it calls something no Mac has run for this application yet, and so is tried once
    /// in a process of its own before the benchmark's own process risks it.
    pub fn probed(self) -> bool {
        matches!(self, Tweak::Revision(_) | Tweak::Device(_))
    }
}

pub struct Variant {
    pub name: &'static str,
    pub tweak: Tweak,
    pub what: &'static str,
}

/// `prod` first: every other variant is reported against it.
pub static VARIANTS: &[Variant] = &[
    Variant {
        name: "prod",
        tweak: Tweak::None,
        what: "today's request: accurate, the newest revision, no language (as host.ocr.recognize \
               without lang), correction off, minimum text height 0, a new request per pass",
    },
    Variant {
        name: "prod-b",
        tweak: Tweak::Control,
        what: "prod again under another name, the control: it differs from prod in nothing, so a \
               verdict on it is noise and says this run is too noisy for verdicts",
    },
    Variant { name: "fast", tweak: Tweak::Fast, what: "the fast recognition level" },
    Variant { name: "rev2", tweak: Tweak::Revision(2), what: "request revision 2 (deprecated from macOS 15)" },
    Variant { name: "rev3", tweak: Tweak::Revision(3), what: "request revision 3 named explicitly (macOS 13 and later)" },
    Variant { name: "mth-1/32", tweak: Tweak::MinTextHeight(1.0 / 32.0), what: "minimum text height 1/32, Apple's default" },
    Variant { name: "mth-0.25", tweak: Tweak::MinTextHeight(0.25), what: "minimum text height 0.25 of the picture" },
    Variant { name: "reuse", tweak: Tweak::Reuse, what: "one request reused on the thread, a new handler per picture" },
    Variant { name: "target-48", tweak: Tweak::TargetPx(48), what: "the ink enlarged toward 48 px instead of 64 (small path only)" },
    Variant { name: "cpu", tweak: Tweak::Device(Device::Cpu), what: "every stage that offers it pinned to the CPU (macOS 14 and later)" },
    Variant { name: "gpu", tweak: Tweak::Device(Device::Gpu), what: "every stage that offers it pinned to the GPU (macOS 14 and later)" },
    Variant { name: "ane", tweak: Tweak::Device(Device::NeuralEngine), what: "every stage that offers it pinned to the Neural Engine (macOS 14 and later)" },
    Variant { name: "lang-en", tweak: Tweak::Lang("en-US"), what: "recognition language en-US, as host.ocr.read sends it to an English-speaking user" },
];

/// The variant with this name, by its index in [`VARIANTS`].
pub fn variant(name: &str) -> Option<usize> {
    VARIANTS.iter().position(|v| v.name == name)
}

/// Which warm-up a first-pass child makes before its first real pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warmup {
    /// The application's warm-up until 2026-10: six dark bars, one accurate pass, on a thread of
    /// its own.
    Bars,
    /// The same over a line of printed words, `line@1x`: the application's own since then
    /// (`warm_up_page` in backend/macos/ocr.rs reads this very picture).
    Word,
    /// None at all.
    Nothing,
}

impl Warmup {
    pub const ALL: [Warmup; 3] = [Warmup::Bars, Warmup::Word, Warmup::Nothing];

    pub fn word(self) -> &'static str {
        match self {
            Warmup::Bars => "bars",
            Warmup::Word => "word",
            Warmup::Nothing => "none",
        }
    }

    fn parse(s: &str) -> Option<Warmup> {
        Warmup::ALL.into_iter().find(|w| w.word() == s)
    }
}

/// The warm-up kinds in the order round `r` starts their processes: rotated by one each round,
/// so that no kind is always the one after the discarded first process, or always the last.
pub fn warmup_order(r: usize) -> [Warmup; 3] {
    let a = Warmup::ALL;
    [a[r % 3], a[(r + 1) % 3], a[(r + 2) % 3]]
}

// ── The command line ─────────────────────────────────────────────────────────────────────────

pub const USAGE: &str = "\
automation-platform ocr-bench [--quick] [--quiet] [--long-idle] [--capture-ms N]
                              [--out FILE] [--summary FILE]

Measures what Apple Vision's text recognition costs on this Mac, on fixed pictures carried
inside the application: no screen capture, no window, no permission, no speech. Quit
Automation Platform first, so that its own reads do not compete for the processor.

  --quick         a shorter run: 3 passes a cell instead of 6 (too few for a verdict),
                  2 pictures for the variants instead of 5, one process per warm-up
                  instead of three, and 24 s of idle instead of 84
  --quiet         print only where the file is and that it is done; everything else
                  goes to the file alone (for a screen reader, which reads every line)
  --long-idle     also one pass after 1, 2, 5 and 10 minutes with nothing to do, on the
                  thread that read before and on a fresh one: 36 minutes more
  --capture-ms N  count the retry ladder's time budget from N ms before each pipeline
                  read, for the screen capture it does not make (default 50)
  --out FILE      where to write what it prints (default: ocr-bench-N.txt beside the
                  application, the first N that is free)
  --summary FILE  append each section's table as Markdown to FILE as soon as the
                  section is done (CI passes $GITHUB_STEP_SUMMARY)

macOS only. Apart from the tables, every line it prints starts with \"OCR BENCH:\", and
the last one says \"done\".";

/// The pretend capture time the pipeline's ladder budget starts with, unless `--capture-ms`
/// says otherwise: the median capture on the tester's Intel Air, 49 ms, rounded.
pub const DEFAULT_CAPTURE_MS: u64 = 50;

/// An internal role: this process was started by a running benchmark to do one thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    /// A first pass in a fresh process, after this warm-up.
    FirstPass(Warmup),
    /// One pass of the variant at this index in [`VARIANTS`], before the benchmark's own
    /// process risks it.
    Probe(usize),
}

impl Child {
    /// What follows `--child` for it.
    pub fn arg(self) -> String {
        match self {
            Child::FirstPass(w) => format!("first-pass:{}", w.word()),
            Child::Probe(i) => format!("probe:{}", VARIANTS[i].name),
        }
    }

    fn parse(v: &str) -> Option<Child> {
        if let Some(w) = v.strip_prefix("first-pass:") {
            return Warmup::parse(w).map(Child::FirstPass);
        }
        v.strip_prefix("probe:").and_then(variant).map(Child::Probe)
    }
}

#[derive(Debug, PartialEq)]
pub struct Options {
    pub quick: bool,
    pub quiet: bool,
    pub long_idle: bool,
    pub capture_ms: u64,
    pub out: Option<PathBuf>,
    pub summary: Option<PathBuf>,
    pub help: bool,
    /// Internal: this process is a child of a running benchmark.
    pub child: Option<Child>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            quick: false,
            quiet: false,
            long_idle: false,
            capture_ms: DEFAULT_CAPTURE_MS,
            out: None,
            summary: None,
            help: false,
            child: None,
        }
    }
}

/// The arguments after `ocr-bench`.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quick" => o.quick = true,
            "--quiet" => o.quiet = true,
            "--long-idle" => o.long_idle = true,
            "--help" | "-h" => o.help = true,
            "--out" | "--summary" | "--child" | "--capture-ms" => {
                let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                match a.as_str() {
                    "--out" => o.out = Some(PathBuf::from(v)),
                    "--summary" => o.summary = Some(PathBuf::from(v)),
                    "--capture-ms" => {
                        o.capture_ms = v
                            .parse::<u64>()
                            .ok()
                            .filter(|ms| *ms <= 10_000)
                            .ok_or_else(|| format!("--capture-ms takes milliseconds, 0 to 10000, not '{v}'"))?
                    }
                    _ => o.child = Some(Child::parse(v).ok_or_else(|| format!("unknown child '{v}'"))?),
                }
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(o)
}

/// One pause in the idle section, and the thread that reads after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Idle {
    pub secs: u64,
    /// A thread started for this one pass, rather than the one that read before the pause.
    pub fresh: bool,
}

impl Idle {
    const fn known(secs: u64) -> Idle {
        Idle { secs, fresh: false }
    }

    const fn fresh(secs: u64) -> Idle {
        Idle { secs, fresh: true }
    }

    pub fn thread(self) -> &'static str {
        if self.fresh {
            "a fresh thread"
        } else {
            "the thread that read before"
        }
    }
}

/// What one run does, by mode.
#[derive(Debug, PartialEq)]
pub struct Plan {
    /// Warm samples per engine and pipeline cell; one more is taken first and kept apart.
    pub samples: usize,
    /// A cell stops taking samples once its warm ones have taken `cell_cap` and it has this many.
    pub min_samples: usize,
    pub cell_cap: Duration,
    /// The pictures every variant is run over.
    pub engine_pictures: &'static [&'static str],
    /// Rounds of first-pass children, one per warm-up kind in each, after one discarded child.
    pub child_rounds: usize,
    /// First passes on fresh threads, the process warm.
    pub fresh_threads: usize,
    /// Passes per thread in the two-threads section.
    pub pair_passes: usize,
    /// The pauses, in the order they are taken: the lengths and the threads interleaved.
    pub idle: Vec<Idle>,
}

impl Plan {
    pub fn for_mode(quick: bool, long_idle: bool) -> Plan {
        let mut plan = if quick {
            Plan {
                samples: 3,
                min_samples: 3,
                cell_cap: Duration::from_secs(3),
                engine_pictures: &["field-64@2x", "field-empty@2x"],
                child_rounds: 1,
                fresh_threads: 1,
                pair_passes: 4,
                idle: vec![Idle::known(2), Idle::fresh(10), Idle::fresh(2), Idle::known(10)],
            }
        } else {
            Plan {
                samples: 6,
                // 5 against prod's 6 can still reach the verdict's p < 0.01; 3 could not.
                min_samples: 5,
                cell_cap: Duration::from_secs(5),
                engine_pictures: &["field-64@1x", "field-64@2x", "field-empty@2x", "lone-1@2x", "line@2x"],
                child_rounds: 3,
                fresh_threads: 3,
                pair_passes: 8,
                idle: vec![
                    Idle::known(2),
                    Idle::fresh(10),
                    Idle::known(30),
                    Idle::fresh(2),
                    Idle::known(10),
                    Idle::fresh(30),
                ],
            }
        };
        if long_idle {
            plan.idle.extend([
                Idle::known(60),
                Idle::fresh(120),
                Idle::known(300),
                Idle::fresh(600),
                Idle::fresh(60),
                Idle::known(120),
                Idle::fresh(300),
                Idle::known(600),
            ]);
        }
        plan
    }

    /// The seconds of idle in all.
    pub fn idle_secs(&self) -> u64 {
        self.idle.iter().map(|i| i.secs).sum()
    }
}

// ── The order the engine's passes run in ─────────────────────────────────────────────────────

/// The engine's cells (every variant over every picture) in the order one round visits them.
#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub start: usize,
    pub step: usize,
    /// Cell `k` of `variants × pictures`, variant-major: variant `k / pictures`, picture
    /// `k % pictures`.
    pub cells: Vec<usize>,
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// SplitMix64: a fixed sequence of well-spread numbers, so the order differs from round to
/// round and is the same in every run, on every machine.
fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE5_E9B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The order round `r` visits `n` cells in: `start + k·step` modulo `n`, with a step coprime to
/// `n` so that every cell comes once, and a start and a step that change from round to round.
/// So no cell always follows the same cell: whatever a switch between models costs falls on
/// every variant and every picture in turn, not on the same one each round. Steps of 1 and
/// `n − 1`, which keep neighbours together, are used only when nothing else is coprime.
pub fn order(n: usize, r: usize) -> Order {
    if n == 0 {
        return Order { start: 0, step: 1, cells: Vec::new() };
    }
    let coprime: Vec<usize> = (1..n.max(2)).filter(|s| gcd(*s, n) == 1).collect();
    let wide: Vec<usize> = coprime.iter().copied().filter(|&s| s != 1 && s + 1 != n).collect();
    let pool = if wide.is_empty() { coprime } else { wide };
    let a = splitmix(r as u64 + 1);
    let b = splitmix(a);
    let start = (a % n as u64) as usize;
    let step = pool[(b % pool.len() as u64) as usize];
    Order { start, step, cells: (0..n).map(|k| (start + k * step) % n).collect() }
}

// ── Measurements and statistics ──────────────────────────────────────────────────────────────

/// One timed read.
#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub ms: f64,
    /// What was read; `None` when Vision refused the request.
    pub text: Option<String>,
    /// Vision passes the read made (1 for an engine pass).
    pub passes: u64,
    /// The blank guard answered without asking Vision.
    pub skipped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub n: usize,
    pub median: f64,
    pub p10: f64,
    pub p90: f64,
    pub min: f64,
    pub max: f64,
}

impl Stats {
    pub fn of(values: &[f64]) -> Option<Stats> {
        let mut v: Vec<f64> = values.iter().copied().filter(|x| x.is_finite()).collect();
        if v.is_empty() {
            return None;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        let n = v.len();
        let median = if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 };
        // Nearest rank: the smallest value with at least p of the samples at or below it. Below
        // ten samples that is the minimum and the maximum, so they are printed only from ten.
        let rank = |p: f64| v[((p * n as f64).ceil() as usize).clamp(1, n) - 1];
        Some(Stats { n, median, p10: rank(0.1), p90: rank(0.9), min: v[0], max: v[n - 1] })
    }
}

/// Text as it is compared: whitespace collapsed to single spaces, nothing else changed.
pub fn normalise(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How a cell's reads compare with the expected text.
#[derive(Clone, Debug, PartialEq)]
pub enum Accuracy {
    /// Every read was right.
    Right,
    /// Not every read was right: how many were, of how many, and the commonest wrong reading.
    Wrong { right: usize, of: usize, got: String },
    /// Every request was refused.
    Refused,
    /// Nothing was measured.
    Nothing,
}

/// The samples of one cell — a variant over a picture, or one picture through the pipeline.
#[derive(Clone, Debug, Default)]
pub struct Cell {
    /// The first sample, kept apart: switching to a variant can load a model.
    pub first: Option<Sample>,
    pub warm: Vec<Sample>,
    /// Why the cell was not measured at all.
    pub skipped: Option<String>,
    /// The first reason Vision gave for refusing a request.
    pub error: Option<String>,
    /// Time spent on its warm samples; the first, which may load a model, is not counted, so
    /// that a dear first pass does not cut the cell short.
    pub spent: Duration,
}

impl Cell {
    pub fn skip(why: impl Into<String>) -> Cell {
        Cell { skipped: Some(why.into()), ..Cell::default() }
    }

    pub fn push(&mut self, s: Sample) {
        if self.first.is_none() {
            self.first = Some(s);
        } else {
            if s.ms.is_finite() && s.ms >= 0.0 {
                self.spent += Duration::from_secs_f64(s.ms / 1000.0);
            }
            self.warm.push(s);
        }
    }

    /// Whether this cell still takes a sample under `plan`.
    pub fn wants_more(&self, plan: &Plan) -> bool {
        if self.skipped.is_some() || self.warm.len() >= plan.samples {
            return false;
        }
        !(self.spent >= plan.cell_cap && self.warm.len() >= plan.min_samples)
    }

    /// The warm samples' milliseconds.
    pub fn warm_ms(&self) -> Vec<f64> {
        self.warm.iter().map(|s| s.ms).collect()
    }

    pub fn stats(&self) -> Option<Stats> {
        Stats::of(&self.warm_ms())
    }

    /// Mean Vision passes per warm read.
    pub fn passes(&self) -> Option<f64> {
        (!self.warm.is_empty())
            .then(|| self.warm.iter().map(|s| s.passes as f64).sum::<f64>() / self.warm.len() as f64)
    }

    pub fn accuracy(&self, expected: &str) -> Accuracy {
        let all: Vec<&Sample> = self.first.iter().chain(self.warm.iter()).collect();
        if all.is_empty() {
            return Accuracy::Nothing;
        }
        let read: Vec<String> = all.iter().filter_map(|s| s.text.as_deref().map(normalise)).collect();
        if read.is_empty() {
            return Accuracy::Refused;
        }
        let want = normalise(expected);
        let right = read.iter().filter(|t| **t == want).count();
        if right == all.len() {
            return Accuracy::Right;
        }
        let mut wrong: Vec<&String> = read.iter().filter(|t| **t != want).collect();
        wrong.sort();
        let got = wrong
            .iter()
            .max_by_key(|t| wrong.iter().filter(|u| u == t).count())
            .map(|t| t.to_string())
            .unwrap_or_else(|| "(refused)".to_string());
        Accuracy::Wrong { right, of: all.len(), got }
    }
}

/// Milliseconds as they are printed: whole above 10, one decimal below.
pub fn ms(x: f64) -> String {
    if x >= 10.0 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

fn stats_text(s: &Stats) -> String {
    let ranks = if s.n >= 10 { format!("p10 {}, p90 {}, ", ms(s.p10), ms(s.p90)) } else { String::new() };
    format!("n {}: median {} ms ({ranks}min {}, max {})", s.n, ms(s.median), ms(s.min), ms(s.max))
}

fn accuracy_text(a: &Accuracy, cell: &Cell) -> String {
    let shown = |t: &str| if t.is_empty() { "nothing".to_string() } else { format!("\"{t}\"") };
    match a {
        Accuracy::Right => {
            let t = cell.warm.first().or(cell.first.as_ref()).and_then(|s| s.text.as_deref()).unwrap_or("");
            format!("read {} right", shown(&normalise(t)))
        }
        Accuracy::Wrong { right, of, got } => format!("WRONG: read {} ({right} of {of} right)", shown(got)),
        Accuracy::Refused => format!(
            "Vision refused every request{}",
            cell.error.as_deref().map(|e| format!(": {e}")).unwrap_or_default()
        ),
        Accuracy::Nothing => "nothing measured".to_string(),
    }
}

// ── The verdict ──────────────────────────────────────────────────────────────────────────────

/// The significance a verdict needs.
pub const ALPHA: f64 = 0.01;

/// The rule a variant is judged by, printed with the results so a reader can disagree with the
/// rule rather than only with the result.
pub const RULE: &str = "\"clearly faster\" = an exact two-sided Mann-Whitney test between its warm \
                        passes and prod's gives p < 0.01, and its median is at least 10 % and 5 ms \
                        below prod's; \"clearly slower\" the same the other way round; anything \
                        else is \"no clear difference\". With 3 passes against 3, as --quick takes, \
                        or 3 against 6, p cannot fall below 0.01, and the cell says \"too few \
                        passes for a verdict\". prod-b is prod under another name: a verdict on it \
                        means this run is too noisy for verdicts. Speed says nothing about reading \
                        right, which is reported beside it.";

/// How many ways `n1` values from one group and `n2` from another can be ordered so that the
/// first group's values exceed exactly `u` of the second's, for every `u` from 0 to `n1·n2`: the
/// Mann-Whitney U distribution with no ties, from the recurrence on the largest value (from the
/// first group it exceeds all `n2`; from the second, none).
fn u_counts(n1: usize, n2: usize) -> Vec<f64> {
    let mut table: Vec<Vec<Vec<f64>>> = vec![vec![Vec::new(); n2 + 1]; n1 + 1];
    for i in 0..=n1 {
        for j in 0..=n2 {
            let mut v = vec![0.0; i * j + 1];
            if i == 0 || j == 0 {
                v[0] = 1.0;
            } else {
                for (u, c) in table[i - 1][j].iter().enumerate() {
                    v[u + j] += c;
                }
                for (u, c) in table[i][j - 1].iter().enumerate() {
                    v[u] += c;
                }
            }
            table[i][j] = v;
        }
    }
    std::mem::take(&mut table[n1][n2])
}

/// The exact two-sided p of a Mann-Whitney U test between `a` and `b`. A tie counts half a pair
/// either way, against the distribution without ties, which errs toward a larger p. `None` when
/// either has no finite value.
pub fn mann_whitney_p(a: &[f64], b: &[f64]) -> Option<f64> {
    let a: Vec<f64> = a.iter().copied().filter(|x| x.is_finite()).collect();
    let b: Vec<f64> = b.iter().copied().filter(|x| x.is_finite()).collect();
    if a.is_empty() || b.is_empty() {
        return None;
    }
    // Twice U, so that a tie's half stays a whole number.
    let mut u2 = 0usize;
    for x in &a {
        for y in &b {
            u2 += if x > y {
                2
            } else if x == y {
                1
            } else {
                0
            };
        }
    }
    let counts = u_counts(a.len(), b.len());
    let total: f64 = counts.iter().sum();
    let low: f64 = counts.iter().enumerate().filter(|(u, _)| 2 * u <= u2).map(|(_, c)| c).sum();
    let high: f64 = counts.iter().enumerate().filter(|(u, _)| 2 * u >= u2).map(|(_, c)| c).sum();
    Some((2.0 * low.min(high) / total).min(1.0))
}

/// The smallest two-sided p that `n1` samples against `n2` can give: every one on one side.
pub fn smallest_p(n1: usize, n2: usize) -> f64 {
    if n1 == 0 || n2 == 0 {
        return 1.0;
    }
    let counts = u_counts(n1, n2);
    (2.0 * counts[0] / counts.iter().sum::<f64>()).min(1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// The change of the median against prod's, in per cent, and the test's p.
    Faster(f64, f64),
    Slower(f64, f64),
    Same(f64),
    /// Too few samples on one side or the other for p to reach [`ALPHA`].
    TooFew,
}

/// A variant's warm samples against prod's, by [`RULE`].
pub fn verdict(prod: &[f64], v: &[f64]) -> Verdict {
    let (Some(ps), Some(vs)) = (Stats::of(prod), Stats::of(v)) else {
        return Verdict::TooFew;
    };
    if smallest_p(ps.n, vs.n) >= ALPHA {
        return Verdict::TooFew;
    }
    let p = mann_whitney_p(v, prod).unwrap_or(1.0);
    let gap = (0.10 * ps.median).max(5.0);
    let change = if ps.median > 0.0 { (vs.median - ps.median) / ps.median * 100.0 } else { 0.0 };
    if p < ALPHA && ps.median - vs.median >= gap {
        Verdict::Faster(change, p)
    } else if p < ALPHA && vs.median - ps.median >= gap {
        Verdict::Slower(change, p)
    } else {
        Verdict::Same(p)
    }
}

fn p_text(p: f64) -> String {
    if p < 0.001 {
        "p < 0.001".to_string()
    } else {
        format!("p {p:.3}")
    }
}

fn verdict_text(v: Verdict) -> String {
    match v {
        Verdict::Faster(c, p) => format!("clearly faster ({c:+.0} %, {})", p_text(p)),
        Verdict::Slower(c, p) => format!("clearly slower ({c:+.0} %, {})", p_text(p)),
        Verdict::Same(p) => format!("no clear difference ({})", p_text(p)),
        Verdict::TooFew => "too few passes for a verdict".to_string(),
    }
}

// ── The engine section ───────────────────────────────────────────────────────────────────────

/// Every variant over every picture: `cells[v][p]`.
pub struct Grid {
    pub variants: Vec<&'static Variant>,
    pub pictures: Vec<&'static Fixture>,
    pub cells: Vec<Vec<Cell>>,
}

impl Grid {
    pub fn new(variants: Vec<&'static Variant>, pictures: Vec<&'static Fixture>) -> Grid {
        let cells = variants.iter().map(|_| pictures.iter().map(|_| Cell::default()).collect()).collect();
        Grid { variants, pictures, cells }
    }

    /// How many cells there are; [`order`] permutes `0..count()`.
    pub fn count(&self) -> usize {
        self.variants.len() * self.pictures.len()
    }

    /// The variant and the picture of cell `k`.
    pub fn at(&self, k: usize) -> (usize, usize) {
        (k / self.pictures.len().max(1), k % self.pictures.len().max(1))
    }

    fn index_of(&self, tweak: Tweak) -> Option<usize> {
        self.variants.iter().position(|v| v.tweak == tweak)
    }

    fn prod(&self, p: usize) -> Option<&Cell> {
        self.index_of(Tweak::None).map(|i| &self.cells[i][p])
    }

    /// What `v` against prod comes to on picture `p`, when both were measured.
    pub fn verdict(&self, v: usize, p: usize) -> Option<Verdict> {
        if self.variants[v].tweak == Tweak::None {
            return None;
        }
        let prod = self.prod(p)?;
        let cell = &self.cells[v][p];
        if cell.skipped.is_some() || prod.skipped.is_some() || cell.warm.is_empty() || prod.warm.is_empty() {
            return None;
        }
        Some(verdict(&prod.warm_ms(), &cell.warm_ms()))
    }

    /// What a cell measured, printed as soon as it is done (so that a crash later keeps it).
    pub fn line(&self, v: usize, p: usize) -> String {
        let (variant, picture, cell) = (self.variants[v], self.pictures[p], &self.cells[v][p]);
        let head = format!("engine | {} | {}", variant.name, picture.label());
        if let Some(why) = &cell.skipped {
            return format!("{head} | not measured: {why}");
        }
        let Some(s) = cell.stats() else {
            return format!("{head} | {}", accuracy_text(&cell.accuracy(picture.expected), cell));
        };
        let first = cell.first.as_ref().map(|f| format!(", first {} ms", ms(f.ms))).unwrap_or_default();
        format!("{head} | {}{first} | {}", stats_text(&s), accuracy_text(&cell.accuracy(picture.expected), cell))
    }

    /// A variant against prod on one picture, printed once every cell is done.
    pub fn verdict_line(&self, v: usize, p: usize) -> Option<String> {
        let verdict = self.verdict(v, p)?;
        let prod = self.prod(p)?.stats()?;
        Some(format!(
            "engine | verdict | {} | {} | against prod {} ms: {}",
            self.variants[v].name,
            self.pictures[p].label(),
            ms(prod.median),
            verdict_text(verdict)
        ))
    }

    /// What the control, prod-b, came to: whether this run is quiet enough for its verdicts.
    pub fn control_line(&self) -> Option<String> {
        let c = self.index_of(Tweak::Control)?;
        let verdicts: Vec<(usize, Verdict)> =
            (0..self.pictures.len()).filter_map(|p| self.verdict(c, p).map(|v| (p, v))).collect();
        let named = |pick: fn(&Verdict) -> bool| -> Vec<String> {
            verdicts.iter().filter(|(_, v)| pick(v)).map(|(p, v)| format!("{} ({})", self.pictures[*p].label(), verdict_text(*v))).collect()
        };
        let loud = named(|v| matches!(v, Verdict::Faster(..) | Verdict::Slower(..)));
        let same = verdicts.iter().filter(|(_, v)| matches!(v, Verdict::Same(_))).count();
        Some(if !loud.is_empty() {
            format!(
                "control | prod-b, the same request as prod, came out {}: this run is too noisy for the verdicts; \
                 read the medians, and take no verdict from it",
                loud.join("; ")
            )
        } else if same == 0 {
            "control | prod-b: too few passes for a verdict, as for every variant in this run".to_string()
        } else {
            format!(
                "control | prod-b, the same request as prod, came out no clear difference on {same} of {} pictures, \
                 as it should{}",
                self.pictures.len(),
                if same < self.pictures.len() { " (too few passes or nothing measured on the rest)" } else { "" }
            )
        })
    }

    /// The compact table: per cell the median, the range, the ratio to prod and the verdict; and
    /// how many pictures the variant read right.
    pub fn table(&self) -> String {
        let mut t = String::from("| variant |");
        for p in &self.pictures {
            t.push_str(&format!(" {} |", p.label()));
        }
        t.push_str(" read right |\n|---|");
        for _ in &self.pictures {
            t.push_str("---:|");
        }
        t.push_str("---:|\n");
        for (v, variant) in self.variants.iter().enumerate() {
            t.push_str(&format!("| {} |", variant.name));
            let (mut right, mut measured) = (0, 0);
            for (p, picture) in self.pictures.iter().enumerate() {
                let cell = &self.cells[v][p];
                let acc = cell.accuracy(picture.expected);
                if !matches!(acc, Accuracy::Nothing) {
                    measured += 1;
                    if acc == Accuracy::Right {
                        right += 1;
                    }
                }
                let text = match (cell.skipped.as_ref(), cell.stats()) {
                    (Some(_), _) => "n/a".to_string(),
                    (None, None) => "refused".to_string(),
                    (None, Some(_)) if acc == Accuracy::Refused => "refused".to_string(),
                    (None, Some(s)) => {
                        let ratio = self
                            .prod(p)
                            .and_then(Cell::stats)
                            .filter(|prod| variant.tweak != Tweak::None && prod.median > 0.0)
                            .map(|prod| format!(", {:.2}x", s.median / prod.median))
                            .unwrap_or_default();
                        let mark = match self.verdict(v, p) {
                            Some(Verdict::Faster(..)) => " faster",
                            Some(Verdict::Slower(..)) => " slower",
                            _ => "",
                        };
                        let wrong = if acc == Accuracy::Right { "" } else { ", wrong" };
                        format!("{} ({} to {}){ratio}{mark}{wrong}", ms(s.median), ms(s.min), ms(s.max))
                    }
                };
                t.push_str(&format!(" {text} |"));
            }
            t.push_str(&format!(" {right} of {measured} |\n"));
        }
        t
    }
}

/// prod over the probe picture interleaved with the other variants (the engine) against the
/// same request pass after pass (the threads section): what switching between variants costs,
/// which every variant in the engine pays alike now that the order is shuffled.
pub fn switching_line(interleaved: Option<f64>, alone: Option<f64>) -> String {
    let head = format!("switching | prod over {PROBE}");
    let (Some(a), Some(b)) = (interleaved, alone) else {
        return format!("{head}: not both measured, nothing to compare");
    };
    let ratio = if b > 0.0 { a / b } else { f64::NAN };
    let reading = if !ratio.is_finite() {
        "nothing to compare"
    } else if ratio >= 1.15 {
        "more than 15 % apart: switching between variants costs time here, every variant in the engine paid \
         it alike, and the engine's milliseconds are dearer than the application's"
    } else if ratio <= 1.0 / 1.15 {
        "more than 15 % apart the other way: the later passes were the slower ones, so the machine drifted \
         during the run (heat, another process); compare variants only within the engine"
    } else {
        "within 15 %: switching between variants costs little here"
    };
    format!(
        "{head}: median {} ms interleaved with the other variants (the engine), {} ms pass after pass (the \
         threads section, later): {ratio:.2}x, {reading}",
        ms(a),
        ms(b)
    )
}

// ── The pipeline section ─────────────────────────────────────────────────────────────────────

/// One picture through the whole production pipeline, capture excepted, under one request.
pub struct PipelineRow {
    /// `prod`, or `rev2` when every request the read makes is asked for revision 2.
    pub request: &'static str,
    pub picture: &'static Fixture,
    pub cell: Cell,
}

impl PipelineRow {
    pub fn line(&self) -> String {
        let head = format!("pipeline | {} | {}", self.request, self.picture.label());
        let acc = accuracy_text(&self.cell.accuracy(self.picture.expected), &self.cell);
        if let Some(why) = &self.cell.skipped {
            return format!("{head} | not measured: {why}");
        }
        let skipped = self.cell.warm.iter().any(|s| s.skipped);
        match (self.cell.stats(), self.cell.passes()) {
            (Some(s), Some(p)) => format!(
                "{head} | {}, {p:.1} Vision passes a read{}{} | {acc}",
                stats_text(&s),
                self.cell.first.as_ref().map(|f| format!(", first read {} ms", ms(f.ms))).unwrap_or_default(),
                if skipped { ", the blank guard answered" } else { "" }
            ),
            _ => format!("{head} | {acc}"),
        }
    }
}

pub fn pipeline_table(rows: &[PipelineRow]) -> String {
    let mut t = String::from(
        "| request | picture | median ms | min to max ms | Vision passes a read | read right |\n|---|---|---:|---:|---:|---|\n",
    );
    for r in rows {
        let (median, range) = r
            .cell
            .stats()
            .map(|s| (ms(s.median), format!("{} to {}", ms(s.min), ms(s.max))))
            .unwrap_or(("n/a".into(), "n/a".into()));
        let passes = r.cell.passes().map(|p| format!("{p:.1}")).unwrap_or_else(|| "n/a".into());
        let right = match r.cell.accuracy(r.picture.expected) {
            Accuracy::Right => "yes".to_string(),
            Accuracy::Wrong { got, .. } => format!("no: \"{got}\""),
            Accuracy::Refused => "refused".to_string(),
            Accuracy::Nothing => "n/a".to_string(),
        };
        t.push_str(&format!("| {} | {} | {median} | {range} | {passes} | {right} |\n", r.request, r.picture.label()));
    }
    t
}

// ── Children: first passes and probes ────────────────────────────────────────────────────────

const CHILD_TAG: &str = "OCR BENCH CHILD:";

/// What a first-pass child measured.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildResult {
    /// The warm-up's own time and whether it read any text, when there was one.
    pub warmup: Option<(f64, bool)>,
    /// The first real pass, on another thread than the warm-up's, the request made inside the
    /// clock as the application makes one per pass.
    pub first: f64,
    /// The pass after it, on the same thread.
    pub second: f64,
}

pub fn child_line(r: &ChildResult) -> String {
    let warm = match r.warmup {
        Some((ms, read)) => format!("warmup={ms:.1} read={}", u8::from(read)),
        None => "warmup=- read=-".to_string(),
    };
    format!("{CHILD_TAG} {warm} first={:.1} second={:.1}", r.first, r.second)
}

fn tagged(output: &str) -> Option<&str> {
    output.lines().find_map(|l| l.trim().strip_prefix(CHILD_TAG)).map(str::trim)
}

/// The child's line out of its whole output, or `None` when it has none.
pub fn parse_child(output: &str) -> Option<ChildResult> {
    let line = tagged(output)?;
    let field = |key: &str| {
        line.split_whitespace().find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')))
    };
    let num = |key: &str| field(key).and_then(|v| v.parse::<f64>().ok());
    let warmup = match (field("warmup")?, field("read")?) {
        ("-", _) => None,
        (w, r) => Some((w.parse::<f64>().ok()?, r == "1")),
    };
    Some(ChildResult { warmup, first: num("first")?, second: num("second")? })
}

/// What a probe child found: the variant ran one pass, and pinned these stages; or it could not
/// be made or Vision refused it, and why.
#[derive(Clone, Debug, PartialEq)]
pub enum Probe {
    Ran { pinned: Vec<String> },
    No(String),
}

pub fn probe_line(p: &Probe) -> String {
    match p {
        Probe::Ran { pinned } if pinned.is_empty() => format!("{CHILD_TAG} probe ran pinned=-"),
        Probe::Ran { pinned } => format!("{CHILD_TAG} probe ran pinned={}", pinned.join(", ")),
        Probe::No(why) => format!("{CHILD_TAG} probe no {}", why.replace('\n', " ")),
    }
}

pub fn parse_probe(output: &str) -> Option<Probe> {
    let rest = tagged(output)?.strip_prefix("probe ")?;
    if let Some(list) = rest.strip_prefix("ran pinned=") {
        let pinned = if list.trim() == "-" {
            Vec::new()
        } else {
            list.split(", ").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };
        return Some(Probe::Ran { pinned });
    }
    rest.strip_prefix("no ").map(|why| Probe::No(why.trim().to_string()))
}

/// The "first passes, threads and idle" table: one measurement a row.
pub fn timing_table(rows: &[(String, String)]) -> String {
    let mut t = String::from("| measurement | ms |\n|---|---:|\n");
    for (what, value) in rows {
        t.push_str(&format!("| {what} | {value} |\n"));
    }
    t
}

/// A list of milliseconds, comma-separated; "n/a" for none.
pub fn ms_list(v: &[f64]) -> String {
    if v.is_empty() {
        return "n/a".to_string();
    }
    v.iter().map(|x| ms(*x)).collect::<Vec<_>>().join(", ")
}

// ── Where it is written ──────────────────────────────────────────────────────────────────────

/// `ocr-bench-N.txt` in `dir`, the first N from 1 that is not taken.
pub fn free_name(dir: &Path) -> PathBuf {
    (1u32..)
        .map(|n| dir.join(format!("ocr-bench-{n}.txt")))
        .find(|p| !p.exists())
        .expect("an unbounded range ends only at a free name")
}

/// Everything printed: to the file when one could be opened, and to the terminal unless the run
/// is quiet. A quiet run that has no file prints everything after all, or nothing would be kept.
pub struct Out {
    file: Option<std::fs::File>,
    pub path: Option<PathBuf>,
    quiet: bool,
}

impl Out {
    pub fn open(path: PathBuf, quiet: bool) -> Out {
        match std::fs::File::create(&path) {
            Ok(f) => Out { file: Some(f), path: Some(path), quiet },
            Err(e) => {
                println!("OCR BENCH: cannot write {}: {e}; printing everything here instead", path.display());
                Out { file: None, path: None, quiet: false }
            }
        }
    }

    /// One `OCR BENCH:` line.
    pub fn say(&mut self, msg: &str) {
        self.raw(&format!("OCR BENCH: {msg}"));
    }

    /// One `OCR BENCH:` line that a quiet run prints as well: where the file is, that it is done.
    pub fn loud(&mut self, msg: &str) {
        let line = format!("OCR BENCH: {msg}");
        println!("{line}");
        self.write_file(&line);
    }

    /// A line as it is, for the tables.
    pub fn raw(&mut self, text: &str) {
        if !self.quiet {
            println!("{text}");
        }
        self.write_file(text);
    }

    fn write_file(&mut self, text: &str) {
        // Unbuffered on purpose: a pass that kills the process must not take the lines before it
        // with it.
        if let Some(f) = self.file.as_mut() {
            let _ = writeln!(f, "{text}");
        }
    }
}

/// Appends `markdown` to `path`, as GitHub's step summary expects.
pub fn append_summary(path: &Path, markdown: &str) -> Result<(), String> {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    f.write_all(markdown.as_bytes()).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// What the subcommand does where there is no Vision.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn elsewhere(args: &[String]) -> i32 {
    match parse(args) {
        Ok(o) if o.help => {
            println!("{USAGE}");
            0
        }
        Ok(_) => {
            println!(
                "OCR BENCH: ocr-bench measures Apple Vision and runs on macOS only; this is {}.",
                std::env::consts::OS
            );
            2
        }
        Err(e) => {
            println!("{e}\n\n{USAGE}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(ms: f64, text: &str) -> Sample {
        Sample { ms, text: Some(text.to_string()), passes: 1, skipped: false }
    }

    #[test]
    fn every_picture_is_the_size_its_entry_says() {
        assert_eq!(FIXTURES.len(), 10);
        for f in FIXTURES {
            let (w, h) = f.px();
            assert_eq!(png_size(f.png), Some((w as u32, h as u32)), "{}", f.label());
            assert!(!f.expected.is_empty(), "{}", f.label());
        }
        // Each layout at both scales, and the labels unique.
        let mut labels: Vec<String> = FIXTURES.iter().map(Fixture::label).collect();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), FIXTURES.len());
        assert!(fixture(PROBE).is_some());
        for mode in [true, false] {
            let plan = Plan::for_mode(mode, false);
            assert!(plan.engine_pictures.contains(&PROBE), "the switching line compares {PROBE}");
            for p in plan.engine_pictures {
                assert!(fixture(p).is_some(), "{p}");
            }
        }
    }

    #[test]
    fn small_and_large_pictures_are_both_there() {
        use super::super::policy::{SMALL_H, SMALL_W};
        assert!(FIXTURES.iter().any(|f| f.w_pt <= SMALL_W && f.h_pt <= SMALL_H));
        assert!(FIXTURES.iter().any(|f| f.w_pt > SMALL_W || f.h_pt > SMALL_H), "a pass-through picture");
    }

    #[test]
    fn a_png_header_is_read_and_anything_else_is_not() {
        assert_eq!(png_size(b"not a png at all, but long enough"), None);
        assert_eq!(png_size(&[]), None);
    }

    #[test]
    fn prod_comes_first_the_control_is_there_and_names_are_unique() {
        assert_eq!(VARIANTS[0].tweak, Tweak::None);
        assert_eq!(VARIANTS.iter().filter(|v| v.tweak == Tweak::None).count(), 1);
        assert_eq!(VARIANTS.iter().filter(|v| v.tweak == Tweak::Control).count(), 1);
        let mut names: Vec<&str> = VARIANTS.iter().map(|v| v.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), VARIANTS.len());
        // Only what has never run on a Mac is tried in a process of its own first.
        let probed: Vec<&str> = VARIANTS.iter().filter(|v| v.tweak.probed()).map(|v| v.name).collect();
        assert_eq!(probed, ["rev2", "rev3", "cpu", "gpu", "ane"]);
    }

    #[test]
    fn the_command_line() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let plain = parse(&[]).unwrap();
        assert_eq!(plain, Options::default());
        assert_eq!(plain.capture_ms, DEFAULT_CAPTURE_MS);
        let o = parse(&a(&["--quick", "--quiet", "--long-idle", "--out", "x.txt", "--summary", "s.md", "--capture-ms", "0"]))
            .unwrap();
        assert!(o.quick && o.quiet && o.long_idle);
        assert_eq!(o.out, Some(PathBuf::from("x.txt")));
        assert_eq!(o.summary, Some(PathBuf::from("s.md")));
        assert_eq!(o.capture_ms, 0);
        assert!(parse(&a(&["--capture-ms", "fifty"])).is_err());
        assert!(parse(&a(&["--capture-ms", "99999"])).is_err());
        assert_eq!(parse(&a(&["--child", "first-pass:word"])).unwrap().child, Some(Child::FirstPass(Warmup::Word)));
        assert_eq!(parse(&a(&["--child", "probe:gpu"])).unwrap().child, Some(Child::Probe(variant("gpu").unwrap())));
        assert!(parse(&a(&["--child", "first-pass:soup"])).is_err());
        assert!(parse(&a(&["--child", "probe:soup"])).is_err());
        assert!(parse(&a(&["--out"])).is_err());
        assert!(parse(&a(&["--fast"])).is_err());
        assert!(parse(&a(&["-h"])).unwrap().help);
        // What the parent passes is what the child reads.
        for c in [Child::FirstPass(Warmup::Nothing), Child::Probe(variant("rev2").unwrap())] {
            assert_eq!(Child::parse(&c.arg()), Some(c));
        }
    }

    #[test]
    fn the_plans() {
        let full = Plan::for_mode(false, false);
        assert_eq!(full.idle_secs(), 84);
        let quick = Plan::for_mode(true, false);
        assert_eq!(quick.idle_secs(), 24);
        assert_eq!(Plan::for_mode(false, true).idle_secs(), 84 + 2160);
        for plan in [&full, &quick, &Plan::for_mode(false, true)] {
            // Every pause once on the thread that read before and once on a fresh one.
            let mut known: Vec<u64> = plan.idle.iter().filter(|i| !i.fresh).map(|i| i.secs).collect();
            let mut fresh: Vec<u64> = plan.idle.iter().filter(|i| i.fresh).map(|i| i.secs).collect();
            known.sort();
            fresh.sort();
            assert_eq!(known, fresh);
            assert!(plan.min_samples <= plan.samples);
        }
        // The full run's cells can reach a verdict even when the cap cuts them short; quick's never.
        assert!(smallest_p(full.samples, full.min_samples) < ALPHA);
        assert!(smallest_p(quick.samples, quick.samples) >= ALPHA);
    }

    #[test]
    fn statistics() {
        let st = Stats::of(&[5.0, 1.0, 4.0, 2.0, 3.0]).unwrap();
        assert_eq!((st.n, st.median, st.min, st.max), (5, 3.0, 1.0, 5.0));
        assert_eq!((st.p10, st.p90), (1.0, 5.0));
        let even = Stats::of(&[1.0, 2.0, 3.0, 10.0]).unwrap();
        assert_eq!(even.median, 2.5);
        let ten: Vec<f64> = (1..=10).map(f64::from).collect();
        let st = Stats::of(&ten).unwrap();
        assert_eq!((st.p10, st.p90), (1.0, 9.0));
        assert_eq!(Stats::of(&[]), None);
        assert_eq!(Stats::of(&[f64::NAN, 2.0]).unwrap().n, 1);
        // Below ten samples p10 and p90 are the extremes, and only those are printed.
        assert!(!stats_text(&Stats::of(&[1.0, 2.0, 3.0]).unwrap()).contains("p90"));
        assert!(stats_text(&st).contains("p90 9.0"));
    }

    #[test]
    fn the_mann_whitney_test() {
        // Complete separation: 2 of C(6,3) = 20 orderings, either way round.
        assert!((mann_whitney_p(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]).unwrap() - 0.1).abs() < 1e-12);
        assert!((mann_whitney_p(&[4.0, 5.0, 6.0], &[1.0, 2.0, 3.0]).unwrap() - 0.1).abs() < 1e-12);
        assert!((smallest_p(6, 6) - 2.0 / 924.0).abs() < 1e-12);
        assert!((smallest_p(5, 6) - 2.0 / 462.0).abs() < 1e-12);
        assert!((smallest_p(4, 6) - 2.0 / 210.0).abs() < 1e-12);
        assert!(smallest_p(3, 6) > ALPHA);
        // Six against six, one pair the wrong way round: U = 1, P(U <= 1) = 2/924.
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 7.0];
        let b = [6.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        assert!((mann_whitney_p(&a, &b).unwrap() - 4.0 / 924.0).abs() < 1e-12);
        // The same values: no evidence at all.
        assert_eq!(mann_whitney_p(&[3.0; 6], &[3.0; 6]), Some(1.0));
        // Interleaved halves: nowhere near.
        assert!(mann_whitney_p(&[1.0, 3.0, 5.0, 7.0, 9.0, 11.0], &[2.0, 4.0, 6.0, 8.0, 10.0, 12.0]).unwrap() > 0.5);
        assert_eq!(mann_whitney_p(&[], &[1.0]), None);
        // The distribution is a distribution.
        let counts = u_counts(4, 3);
        assert_eq!(counts.len(), 13);
        assert_eq!(counts.iter().sum::<f64>(), 35.0);
        assert_eq!(counts.first(), counts.last());
    }

    #[test]
    fn the_verdict_rule() {
        let prod = [200.0, 190.0, 210.0, 205.0, 195.0, 220.0];
        let fast = [60.0, 55.0, 70.0, 58.0, 65.0, 62.0];
        // Medians 202.5 and 61: -69.9 %.
        assert!(matches!(verdict(&prod, &fast), Verdict::Faster(c, p) if (c + 69.88).abs() < 0.01 && p < ALPHA));
        assert!(matches!(verdict(&fast, &prod), Verdict::Slower(..)));
        // Separated, but by 6 %: not clearly faster.
        let close = [192.0, 188.0, 189.0, 187.0, 186.0, 185.0];
        assert!(matches!(verdict(&[201.0, 199.0, 198.0, 202.0, 200.0, 197.0], &close), Verdict::Same(p) if p < ALPHA));
        // 25 % faster by the median but overlapping: the test does not let it through.
        let wide = [150.0, 140.0, 260.0, 145.0, 250.0, 155.0];
        assert!(matches!(verdict(&prod, &wide), Verdict::Same(p) if p >= ALPHA));
        // The 5 ms floor on a fast machine: 4 ms of 20 is 20 %, and still not a finding.
        let small = [20.0, 19.5, 20.5, 19.8, 20.2, 20.1];
        let less = [16.0, 15.5, 16.5, 15.8, 16.2, 16.1];
        assert!(matches!(verdict(&small, &less), Verdict::Same(_)));
        // Three against three never gives a verdict, however far apart.
        assert_eq!(verdict(&[200.0, 201.0, 202.0], &[50.0, 51.0, 52.0]), Verdict::TooFew);
        assert_eq!(verdict(&[], &[50.0]), Verdict::TooFew);
    }

    /// The old rule's weakness, kept as a test: two variants drawn from the same spread, six
    /// passes each, at 20 % spread. By "p90 below prod's median" that came out "clearly" about
    /// one comparison in eleven. The test alone allows 4 orderings of 924 each way round (0.87 %),
    /// and the 10 % gap takes some of those away; 2 % leaves room for the draw.
    #[test]
    fn the_rule_rarely_calls_noise_a_difference() {
        let mut x = 1u64;
        let mut next = || {
            x = splitmix(x);
            (x >> 11) as f64 / (1u64 << 53) as f64
        };
        // A spread like the Air's: a lognormal-ish factor around 200 ms.
        let mut draw = || -> Vec<f64> {
            (0..6)
                .map(|_| {
                    let (u1, u2) = (next().max(1e-12), next());
                    let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                    200.0 * (0.2 * z).exp()
                })
                .collect()
        };
        let trials = 4000;
        let loud = (0..trials)
            .filter(|_| {
                let (a, b) = (draw(), draw());
                matches!(verdict(&a, &b), Verdict::Faster(..) | Verdict::Slower(..))
            })
            .count();
        assert!((loud as f64) / (trials as f64) < 0.02, "{loud} of {trials}");
    }

    #[test]
    fn a_cell_keeps_its_first_sample_apart_and_stops_at_its_cap() {
        let plan = Plan::for_mode(false, false);
        let mut c = Cell::default();
        c.push(s(900.0, "64"));
        assert_eq!(c.spent, Duration::ZERO, "the first sample is not counted against the cap");
        for _ in 0..plan.samples {
            assert!(c.wants_more(&plan));
            c.push(s(20.0, "64"));
        }
        assert!(!c.wants_more(&plan));
        assert_eq!(c.first.as_ref().unwrap().ms, 900.0);
        assert_eq!(c.stats().unwrap().median, 20.0);

        // A dear first pass does not cut the cell short.
        let mut cold = Cell::default();
        cold.push(s(9000.0, "64"));
        cold.push(s(20.0, "64"));
        assert!(cold.wants_more(&plan));

        // Past the cap, a cell still takes passes until it has enough for a verdict.
        let mut slow = Cell::default();
        for _ in 0..plan.min_samples {
            slow.push(s(2000.0, "64"));
            assert!(slow.wants_more(&plan));
        }
        slow.push(s(2000.0, "64"));
        assert!(!slow.wants_more(&plan), "past its cap with {} warm samples", plan.min_samples);
        assert!(!Cell::skip("not here").wants_more(&plan));
    }

    #[test]
    fn accuracy_collapses_whitespace_and_keeps_case() {
        let mut c = Cell::default();
        c.push(s(1.0, "Instrument  Polyphony\nPitchbend"));
        c.push(s(1.0, "Instrument Polyphony Pitchbend"));
        assert_eq!(c.accuracy("Instrument Polyphony Pitchbend"), Accuracy::Right);
        let mut d = Cell::default();
        d.push(s(1.0, "DEF"));
        d.push(s(1.0, "def"));
        d.push(s(1.0, "def"));
        assert_eq!(d.accuracy("DEF"), Accuracy::Wrong { right: 1, of: 3, got: "def".into() });
        let mut r = Cell::default();
        r.push(Sample { ms: 1.0, text: None, passes: 1, skipped: false });
        assert_eq!(r.accuracy("64"), Accuracy::Refused);
        assert_eq!(Cell::default().accuracy("64"), Accuracy::Nothing);
    }

    #[test]
    fn the_order_visits_every_cell_once_and_changes_its_neighbours() {
        for n in [0, 1, 2, 3, 26, 65] {
            for r in 0..8 {
                let o = order(n, r);
                let mut cells = o.cells.clone();
                cells.sort();
                assert_eq!(cells, (0..n).collect::<Vec<_>>(), "n {n}, round {r}");
                assert_eq!(o, order(n, r), "the same in every run");
            }
        }
        // 13 variants over 5 pictures: what follows a cell differs from round to round, and is
        // not the next variant on the same picture (which a plain rotation always made it).
        let n = 65;
        let rounds: Vec<Order> = (0..7).map(|r| order(n, r)).collect();
        for cell in [0, 5, 17] {
            let next: Vec<usize> = rounds
                .iter()
                .map(|o| {
                    let at = o.cells.iter().position(|c| *c == cell).unwrap();
                    o.cells[(at + 1) % n]
                })
                .collect();
            let mut distinct = next.clone();
            distinct.sort();
            distinct.dedup();
            assert!(distinct.len() >= 4, "cell {cell} is followed by {next:?}");
        }
        assert!(rounds.iter().all(|o| o.step != 1 && o.step != n - 1));
    }

    #[test]
    fn the_warm_ups_take_turns() {
        assert_eq!(warmup_order(0), Warmup::ALL);
        let firsts: Vec<Warmup> = (0..3).map(|r| warmup_order(r)[0]).collect();
        assert_eq!(firsts, Warmup::ALL);
    }

    #[test]
    fn a_childs_line_survives_the_round_trip() {
        let r = ChildResult { warmup: Some((1790.25, false)), first: 640.5, second: 210.0 };
        let out = format!("noise\n{}\n", child_line(&r));
        let back = parse_child(&out).unwrap();
        assert_eq!(back.warmup.map(|w| w.1), Some(false));
        assert!((back.first - 640.5).abs() < 0.01 && (back.second - 210.0).abs() < 0.01);
        let none = ChildResult { warmup: None, first: 1.0, second: 2.0 };
        assert_eq!(parse_child(&child_line(&none)).unwrap().warmup, None);
        assert_eq!(parse_child("no such line"), None);
    }

    #[test]
    fn a_probes_line_survives_the_round_trip() {
        for p in [
            Probe::Ran { pinned: vec![] },
            Probe::Ran { pinned: vec!["VNComputeStageMain".into(), "VNComputeStagePostProcessing".into()] },
            Probe::No("Vision offers text recognition no GPU here".into()),
        ] {
            assert_eq!(parse_probe(&format!("2026 noise\n{}\n", probe_line(&p))), Some(p));
        }
        assert_eq!(parse_probe("OCR BENCH CHILD: warmup=- read=- first=1.0 second=2.0"), None);
    }

    fn grid(names: &[&str]) -> Grid {
        let variants: Vec<&Variant> = names.iter().map(|n| &VARIANTS[variant(n).unwrap()]).collect();
        Grid::new(variants, vec![fixture(PROBE).unwrap()])
    }

    #[test]
    fn the_engine_lines_and_table() {
        let mut g = grid(&["prod", "fast", "gpu", "prod-b"]);
        // Every second millisecond, so that no median falls on a half.
        for i in 0..7 {
            g.cells[0][0].push(s(200.0 + 2.0 * i as f64, "64"));
            g.cells[1][0].push(s(50.0 + 2.0 * i as f64, "6A"));
            g.cells[3][0].push(s(199.0 + 2.0 * i as f64, "64"));
        }
        g.cells[2][0] = Cell::skip("Vision offers no GPU here");
        assert_eq!(g.count(), 4);
        assert_eq!(g.at(3), (3, 0));
        let prod = g.line(0, 0);
        assert!(prod.contains("engine | prod | field-64@2x | n 6: median 207 ms (min 202, max 212), first 200 ms"), "{prod}");
        assert!(prod.contains("read \"64\" right"), "{prod}");
        assert!(g.verdict_line(0, 0).is_none(), "prod is not judged against itself");
        let fast = g.verdict_line(1, 0).unwrap();
        assert!(fast.contains("engine | verdict | fast | field-64@2x | against prod 207 ms: clearly faster (-72 %, p 0.002)"), "{fast}");
        assert!(g.line(1, 0).contains("WRONG: read \"6A\" (0 of 7 right)"));
        assert!(g.line(2, 0).contains("not measured: Vision offers no GPU here"));
        assert!(g.verdict_line(2, 0).is_none());
        let control = g.control_line().unwrap();
        assert!(control.contains("no clear difference on 1 of 1 pictures, as it should"), "{control}");
        let table = g.table();
        assert!(table.contains("| prod | 207 (202 to 212) | 1 of 1 |"), "{table}");
        assert!(table.contains("| fast | 57 (52 to 62), 0.28x faster, wrong | 0 of 1 |"), "{table}");
        assert!(table.contains("| gpu | n/a | 0 of 0 |"), "{table}");
        assert!(table.contains("| prod-b | 206 (201 to 211), 1.00x | 1 of 1 |"), "{table}");
    }

    #[test]
    fn a_loud_control_says_the_run_is_too_noisy() {
        let mut g = grid(&["prod", "prod-b"]);
        for i in 0..7 {
            g.cells[0][0].push(s(200.0 + i as f64, "64"));
            g.cells[1][0].push(s(300.0 + i as f64, "64"));
        }
        let line = g.control_line().unwrap();
        assert!(line.contains("came out field-64@2x (clearly slower"), "{line}");
        assert!(line.contains("too noisy"), "{line}");
        let mut few = grid(&["prod", "prod-b"]);
        for i in 0..4 {
            few.cells[0][0].push(s(200.0 + i as f64, "64"));
            few.cells[1][0].push(s(300.0 + i as f64, "64"));
        }
        assert!(few.control_line().unwrap().contains("too few passes"));
        assert!(grid(&["prod", "fast"]).control_line().is_none());
    }

    #[test]
    fn the_switching_line() {
        assert!(switching_line(Some(220.0), Some(200.0)).contains("costs little"));
        assert!(switching_line(Some(240.0), Some(200.0)).contains("switching between variants costs time here"));
        assert!(switching_line(Some(160.0), Some(200.0)).contains("drifted"));
        assert!(switching_line(None, Some(200.0)).contains("not both measured"));
    }

    #[test]
    fn the_pipeline_line_names_the_passes() {
        let mut cell = Cell::default();
        for _ in 0..4 {
            cell.push(Sample { ms: 450.0, text: Some("1".into()), passes: 3, skipped: false });
        }
        let row = PipelineRow { request: "rev2", picture: fixture("lone-1@2x").unwrap(), cell };
        assert!(row.line().starts_with("pipeline | rev2 | lone-1@2x | n 3: median 450 ms"), "{}", row.line());
        assert!(row.line().contains("3.0 Vision passes a read"), "{}", row.line());
        let table = pipeline_table(std::slice::from_ref(&row));
        assert!(table.contains("| rev2 | lone-1@2x | 450 | 450 to 450 | 3.0 | yes |"), "{table}");
    }

    #[test]
    fn milliseconds_are_printed_short() {
        assert_eq!(ms(212.4), "212");
        assert_eq!(ms(4.56), "4.6");
        assert_eq!(ms_list(&[1.0, 20.0]), "1.0, 20");
        assert_eq!(ms_list(&[]), "n/a");
        assert_eq!(p_text(0.0004), "p < 0.001");
        assert_eq!(p_text(0.0216), "p 0.022");
    }

    #[test]
    fn the_file_name_is_the_first_free_one_and_a_quiet_run_still_writes_everything() {
        let dir = std::env::temp_dir().join(format!("ocr-bench-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir), dir.join("ocr-bench-1.txt"));
        std::fs::write(dir.join("ocr-bench-1.txt"), "").unwrap();
        assert_eq!(free_name(&dir), dir.join("ocr-bench-2.txt"));
        let summary = dir.join("summary.md");
        append_summary(&summary, "a\n").unwrap();
        append_summary(&summary, "b\n").unwrap();
        assert_eq!(std::fs::read_to_string(&summary).unwrap(), "a\nb\n");
        let file = dir.join("quiet.txt");
        let mut out = Out::open(file.clone(), true);
        out.loud("writing to it");
        out.say("a section");
        out.raw("| a | table |");
        drop(out);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "OCR BENCH: writing to it\nOCR BENCH: a section\n| a | table |\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn elsewhere_says_so_and_still_helps() {
        assert_eq!(elsewhere(&["--help".to_string()]), 0);
        assert_eq!(elsewhere(&["--bogus".to_string()]), 2);
        assert_eq!(elsewhere(&[]), 2);
    }
}
