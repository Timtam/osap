//! `automation-platform ocr-bench`: what Apple Vision costs on this Mac, read off fixed pictures.
//!
//! A measurement, not a feature. It answers the questions a tester's Intel MacBook Air raised —
//! a small read-out took 143 to 786 ms there, against 22 to 42 ms on the Windows machine and 25
//! to 56 ms on a warm Mac mini M1 — with numbers a machine produces by itself: what the machine
//! is, what one Vision pass costs under today's settings and under each setting that might
//! change it, how many passes the retry ladder makes for one read, what the first pass on a
//! thread or in a process costs, what a pass costs after the recogniser has sat idle, and what
//! two passes at once cost. And, where the bundle carries ONNX Runtime, the same for the neural
//! recogniser beside Vision: what it reads and costs alone, what each way of reading a small
//! region with it reads and costs ([`STRATEGIES`]: the application's own, `prod`, the ladder it
//! read with before, `old`, and the others), whether it slows Vision, and the
//! cheapest way that reads every picture right ([`closing_lines`]). The same pictures are read on
//! a CI runner, on the Air and on an Apple-silicon Mac, so only the machine differs. It opens no
//! window, captures no screen, needs no permission and never speaks; it does not open the
//! application's log either.
//!
//! This is the pure half: the pictures and the answers each accepts, the command line, the
//! order the passes run in, the statistics, the verdict and every line and table it prints.
//! The standard library, and `toml` for the manifest of `--pictures`, so its tests run on
//! Windows, and `crates/macos-check` borrows it. The half that runs Vision is
//! `backend/macos/ocr/bench.rs`, a child of the recogniser it measures, so it calls the
//! production code itself and names each deviation from it. `ocr-bench --paddle` on Windows is
//! `backend/paddle_ocr/bench.rs`, which prints with this file too.
//!
//! The pictures are drawn by `tools/ocr-fixtures/make.py` and carried inside the executable;
//! real captures, cut by `tools/ocr-fixtures/crop.py`, are read from a folder (`--pictures`).

// Everywhere but on a Mac only the command line, the "macOS only" answer and what `--paddle`
// prints are used; the rest is compiled there for its tests.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::cost::Stage;
use super::ladder::{Answer, Level, PaddleUse, Shape};
use super::paddle_pre;

/// Where a picture came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Drawn by `tools/ocr-fixtures/make.py`, with FreeType.
    Drawn,
    /// Captured off a real plug-in on this platform (`windows` or `macos`), by its renderer.
    Captured { platform: &'static str },
}

/// One fixed picture and the answers that count as reading it right.
#[derive(PartialEq)]
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
    /// The answers that are right, compared after whitespace is collapsed, case and all. The
    /// empty string is the answer "nothing": on a picture whose only answer it is, any text is
    /// invented. More than one where a reader may rightly differ (a lone dash it may drop).
    pub accept: &'static [&'static str],
    /// Whether a way of reading must read it right to count as one that works. False where
    /// nothing is an accepted answer.
    pub must_read: bool,
    pub origin: Origin,
    /// The PNG, as `tools/ocr-fixtures/make.py` drew it or `crop.py` cut it.
    pub png: &'static [u8],
}

impl std::fmt::Debug for Fixture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fixture({}, {:?})", self.label(), self.accept)
    }
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

    /// Whether the only right answer is nothing: ink that is not text.
    pub fn holds_nothing(&self) -> bool {
        self.accept == [""]
    }

    /// Whether the small-text path reads it (400x200 points or less), which is also the only
    /// size the neural recogniser is handed.
    pub fn small(&self) -> bool {
        super::policy::is_small(self.w_pt, self.h_pt)
    }

    /// The accepted answers as they are printed: `"64"`, or `"-" or nothing`.
    pub fn accept_text(&self) -> String {
        let shown: Vec<String> =
            self.accept.iter().map(|a| if a.is_empty() { "nothing".to_string() } else { format!("\"{a}\"") }).collect();
        shown.join(" or ")
    }
}

macro_rules! fixture {
    ($name:literal, $scale:literal, $w:literal, $h:literal, $text:literal) => {
        Fixture {
            name: $name,
            scale: $scale,
            w_pt: $w,
            h_pt: $h,
            accept: &[$text],
            must_read: true,
            origin: Origin::Drawn,
            png: include_bytes!(concat!("../../bench-data/ocr/", $name, "@", $scale, "x.png")),
        }
    };
    (nothing $name:literal, $scale:literal, $w:literal, $h:literal) => {
        Fixture {
            name: $name,
            scale: $scale,
            w_pt: $w,
            h_pt: $h,
            accept: &[""],
            must_read: false,
            origin: Origin::Drawn,
            png: include_bytes!(concat!("../../bench-data/ocr/", $name, "@", $scale, "x.png")),
        }
    };
    (or_nothing $name:literal, $scale:literal, $w:literal, $h:literal, $text:literal) => {
        Fixture {
            name: $name,
            scale: $scale,
            w_pt: $w,
            h_pt: $h,
            accept: &[$text, ""],
            must_read: false,
            origin: Origin::Drawn,
            png: include_bytes!(concat!("../../bench-data/ocr/", $name, "@", $scale, "x.png")),
        }
    };
}

/// Every drawn picture, each layout at 1x and 2x. The fields imitate sforzando's read-outs (a
/// black well in grey chrome, the digits about 11 pixels tall at 1x as in sforzando's own):
/// `field-64` its Polyphony, `field-DEF` its Pitchbend range, `lone-1` a single glyph, the
/// ladder's case; `field-empty` is its Instrument field, light with dark text. `val-` are values
/// with a sign or a decimal point, which a reader can lose: `-12` and `0.50` in a well, `-0.5 dB`
/// and `+3 ct` in a light field like Melodyne's inspector. `none-` hold ink that is not text — a
/// level meter, a speaker symbol, an empty well with its bevel, a text caret — and nothing is the
/// only right answer. `clipped-label` is a word cut off at half its height: its upper half is
/// there to be read, so the word is right and so is nothing, and anything else is wrong. `line` is
/// wider than 400 points and so is handed to Vision as it is. Kept in step with `LAYOUTS` in
/// make.py.
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
    fixture!("val-minus12", 1, 40, 20, "-12"),
    fixture!("val-minus12", 2, 40, 20, "-12"),
    fixture!("val-db", 1, 70, 14, "-0.5 dB"),
    fixture!("val-db", 2, 70, 14, "-0.5 dB"),
    fixture!("val-ct", 1, 50, 14, "+3 ct"),
    fixture!("val-ct", 2, 50, 14, "+3 ct"),
    fixture!("val-050", 1, 44, 20, "0.50"),
    fixture!("val-050", 2, 44, 20, "0.50"),
    fixture!(nothing "none-bars", 1, 40, 20),
    fixture!(nothing "none-bars", 2, 40, 20),
    fixture!(nothing "none-icon", 1, 20, 20),
    fixture!(nothing "none-icon", 2, 20, 20),
    fixture!(nothing "none-well", 1, 40, 20),
    fixture!(nothing "none-well", 2, 40, 20),
    fixture!(or_nothing "clipped-label", 1, 60, 8, "Velocity"),
    fixture!(or_nothing "clipped-label", 2, 60, 8, "Velocity"),
    fixture!(nothing "none-caret", 1, 40, 16),
    fixture!(nothing "none-caret", 2, 40, 16),
];

/// Every picture a run reads: the drawn ones, then those `--pictures` added.
pub fn all_pictures(o: &Options) -> Vec<&'static Fixture> {
    FIXTURES.iter().chain(o.pictures.iter().copied()).collect()
}

/// The keys a `[[picture]]` entry of a manifest may have; any other is a mistake and refused.
const MANIFEST_KEYS: [&str; 9] = ["name", "scale", "w_pt", "h_pt", "accept", "must_read", "platform", "source", "checked"];

/// The pictures `dir/manifest.toml` lists, each read from `<name>@<scale>x.png` beside it. Read
/// once, when the command line is, and kept for the rest of the process (they are leaked, as
/// the drawn ones are static). Refused, with the entry and the reason, when an entry lacks a key
/// or has one this does not know, when a name is not a plain file name, when its PNG is missing
/// or not the size `w_pt · scale` by `h_pt · scale`, when a picture that must be read accepts
/// nothing, or when a label is taken twice, by the manifest or by a drawn picture.
///
/// An entry:
///
/// ```toml
/// [[picture]]
/// name = "sfz-polyphony"       # the file is sfz-polyphony@1x.png
/// scale = 1                    # 1 to 4 pixels a point
/// w_pt = 30                    # the region, in points
/// h_pt = 30
/// accept = ["64"]              # "" is the answer "nothing"
/// must_read = true
/// platform = "windows"         # whose renderer drew it: "windows" or "macos"
/// source = "…"                 # where it was cut from (kept for the reader, not read)
/// checked = "…"                # how the accepted answers were checked (the same)
/// ```
pub fn load_pictures(dir: &Path) -> Result<Vec<&'static Fixture>, String> {
    let path = dir.join("manifest.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
    let Some(entries) = table.get("picture").and_then(toml::Value::as_array) else {
        return Err(format!("{}: no [[picture]] entries", path.display()));
    };
    let mut labels: Vec<String> = FIXTURES.iter().map(Fixture::label).collect();
    let mut out = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let at = format!("{}, picture {}", path.display(), i + 1);
        let e = e.as_table().ok_or_else(|| format!("{at}: not a table"))?;
        if let Some(k) = e.keys().find(|k| !MANIFEST_KEYS.contains(&k.as_str())) {
            return Err(format!("{at}: unknown key '{k}'"));
        }
        let text = |k: &str| e.get(k).and_then(toml::Value::as_str).ok_or_else(|| format!("{at}: '{k}' is missing or not text"));
        let int = |k: &str, max: i64| {
            e.get(k)
                .and_then(toml::Value::as_integer)
                .filter(|v| (1..=max).contains(v))
                .ok_or_else(|| format!("{at}: '{k}' is missing or not a whole number from 1 to {max}"))
        };
        let name = text("name")?;
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(format!("{at}: the name '{name}' is not letters, digits, '-', '_' and '.'"));
        }
        let at = format!("{at} ({name})");
        let scale = int("scale", 4)? as u32;
        let (w_pt, h_pt) = (int("w_pt", 10_000)? as i32, int("h_pt", 10_000)? as i32);
        let accept: Vec<String> = e
            .get("accept")
            .and_then(toml::Value::as_array)
            .and_then(|a| a.iter().map(|v| v.as_str().map(str::to_string)).collect::<Option<Vec<_>>>())
            .filter(|a| !a.is_empty())
            .ok_or_else(|| format!("{at}: 'accept' is missing or not a list of texts"))?;
        let must_read =
            e.get("must_read").and_then(toml::Value::as_bool).ok_or_else(|| format!("{at}: 'must_read' is missing or not true or false"))?;
        if must_read && accept.iter().any(|a| a.trim().is_empty()) {
            return Err(format!("{at}: a picture that must be read cannot accept nothing"));
        }
        let platform = match text("platform")? {
            "windows" => "windows",
            "macos" => "macos",
            other => return Err(format!("{at}: platform '{other}' is neither \"windows\" nor \"macos\"")),
        };
        let label = format!("{name}@{scale}x");
        if labels.contains(&label) {
            return Err(format!("{at}: the label {label} is taken already"));
        }
        let file = dir.join(format!("{label}.png"));
        let png = std::fs::read(&file).map_err(|e| format!("{at}: cannot read {}: {e}", file.display()))?;
        let want = (w_pt as u32 * scale, h_pt as u32 * scale);
        match png_size(&png) {
            Some(got) if got == want => {}
            Some((w, h)) => return Err(format!("{at}: {} is {w}x{h} pixels, the entry says {}x{}", file.display(), want.0, want.1)),
            None => return Err(format!("{at}: {} is not a PNG", file.display())),
        }
        labels.push(label);
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let accept: Vec<&'static str> = accept.into_iter().map(leak).collect();
        out.push(&*Box::leak(Box::new(Fixture {
            name: leak(name.to_string()),
            scale,
            w_pt,
            h_pt,
            accept: Box::leak(accept.into_boxed_slice()),
            must_read,
            origin: Origin::Captured { platform },
            png: Box::leak(png.into_boxed_slice()),
        })));
    }
    Ok(out)
}

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
    /// The neural recogniser instead of Vision, handed this much of the region. Small regions
    /// only.
    Paddle(PaddleInput),
}

/// What the neural recogniser is handed of a small region on a Mac.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaddleInput {
    /// The whole region; its own crop finds the ink (`paddle_pre::tighten`, with the margin and
    /// the least enlargement in points, `Tighten::for_scale`).
    Raw,
    /// The content crop a read makes (`Plan::content`), a well's inside included, with its
    /// margin: what the strategies hand it.
    Crop,
}

impl Tweak {
    /// Whether it calls something no Mac has run for this application yet, and so is tried once
    /// in a process of its own before the benchmark's own process risks it.
    pub fn probed(self) -> bool {
        matches!(self, Tweak::Revision(_) | Tweak::Device(_) | Tweak::Paddle(_))
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
    Variant {
        name: "paddle-raw",
        tweak: Tweak::Paddle(PaddleInput::Raw),
        what: "the neural recogniser instead of Vision, over the whole region, cropped by its own rule in points (small \
               regions only; preparing and the model, on this thread)",
    },
    Variant {
        name: "paddle-crop",
        tweak: Tweak::Paddle(PaddleInput::Crop),
        what: "the neural recogniser instead of Vision, over the content crop a read makes, a well's inside included \
               (small regions only; preparing and the model, on this thread)",
    },
];

/// What a strategy is in the run: the one every other is held against, a way the application may
/// come to read by, or one measured for comparison only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `prod`, the application's reading on this Mac (`ladder::Shape::production`).
    Production,
    /// Another way of reading small regions, which the closing line may name.
    Candidate,
    /// Measured beside them, never named: it speaks a value nothing checked (`fast>acc`, `paddle`),
    /// leaves out what the rest of the ladder gives (`acc+paddle`), changes every rung (`rev2`), or
    /// is how the application read before (`old`).
    Comparison,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Role::Production => "production",
            Role::Candidate => "candidate",
            Role::Comparison => "comparison",
        }
    }
}

/// One way of reading a small region, end to end: a shape of the ladder (`ocr/ladder.rs`), and
/// for `rev2` a revision every request of every rung asks for. The pipeline reads every picture
/// with each, as `host.ocr.recognize` reads a region, and compares each with `prod`.
pub struct Strategy {
    pub name: &'static str,
    /// The shape it reads with; for `prod`, the one it reads with where the neural recogniser has
    /// not loaded ([`Strategy::shape_here`]).
    pub shape: Shape,
    /// Every request of every rung asks for this revision.
    pub revision_all: Option<usize>,
    pub role: Role,
    pub what: &'static str,
}

impl Strategy {
    /// The request revision it asks for somewhere, which this macOS must have and a probe must
    /// have run.
    pub fn revision(&self) -> Option<usize> {
        self.revision_all.or(self.shape.rung1_revision).or(self.shape.rev2_second.then_some(2))
    }

    /// Whether it needs the neural recogniser. `prod` does not: without it, it reads as the
    /// application does then.
    pub fn paddle(&self) -> bool {
        self.shape.paddle.reads()
    }

    /// The shape it reads with on this Mac: `prod`'s is the application's
    /// (`ladder::Shape::production`) — with the neural recogniser where it is ready in this process
    /// (`paddle_ready`), checked by the fast level first on an Intel Mac (`intel`) — and every
    /// other strategy's its own.
    pub fn shape_here(&self, paddle_ready: bool, intel: bool) -> Shape {
        if self.role == Role::Production {
            Shape::production(paddle_ready, intel)
        } else {
            self.shape
        }
    }

    /// Whether it reads a region larger than 400x200 points as `prod` does: every one but
    /// `rev2`, since the strategies change only the small-text path.
    pub fn as_prod_when_large(&self) -> bool {
        self.revision_all.is_none()
    }
}

const fn shape(first: Level, paddle: PaddleUse, rest: bool, rung1_revision: Option<usize>, rev2_second: bool) -> Shape {
    Shape { first, paddle, rest, rung1_revision, rev2_second }
}

/// `prod` first: every other strategy is reported against it. `prod` reads as the application
/// does on this Mac (`ladder::Shape::production`); `old` as it did before the neural recogniser
/// answered on a Mac, so that every run shows the difference. The other ways of reading small
/// regions measured beside it: the lone digit (`acc+paddle+rest`, `rev2-tight`, `rev3>rev2`), and
/// the fast level first with the neural recogniser as its check (`fast=paddle-checked`,
/// `fast=paddle-strict` and `fast=paddle`, which differ in what follows a check that failed); the
/// rest for comparison, never named ([`Role`]). A strategy that needs the neural recogniser reads a
/// region whose ink is not one line, or too wide, as `old` does.
pub static STRATEGIES: &[Strategy] = &[
    Strategy {
        name: "prod",
        shape: Shape::TODAY,
        revision_all: None,
        role: Role::Production,
        what: "the application's reading: where the neural recogniser has loaded, Windows' rule over the accurate \
               ladder — it is asked when the read begins, the accurate ladder (the content crop; the whole region \
               when that read nothing; the enlarged crop within the 250 ms budget; no fast model) answers when it \
               reads anything, and its text when it reads nothing — and on an Intel Mac the fast level checked by it \
               first, its text with the recogniser's spacing where the two read the same, spaces included; where it \
               has not loaded, old",
    },
    Strategy {
        name: "old",
        shape: Shape::TODAY,
        revision_all: None,
        role: Role::Comparison,
        what: "the ladder the application read with before the neural recogniser answered on a Mac: accurate over \
               the content crop; the whole region when that read nothing; then the enlarged crop and the fast model \
               within the 250 ms budget",
    },
    Strategy {
        name: "rev2",
        shape: Shape::TODAY,
        revision_all: Some(2),
        role: Role::Comparison,
        what: "old's ladder with every request of every rung asking for revision 2",
    },
    Strategy {
        name: "paddle",
        shape: shape(Level::Accurate, PaddleUse::Alone, false, None, false),
        revision_all: None,
        role: Role::Comparison,
        what: "the neural recogniser alone over the content crop, Vision not asked",
    },
    Strategy {
        name: "acc+paddle",
        shape: shape(Level::Accurate, PaddleUse::Fallback, false, None, false),
        revision_all: None,
        role: Role::Comparison,
        what: "Windows' rule after the first accurate pass alone: the neural recogniser asked when the read begins, \
               at low priority; its text when the first accurate pass read nothing; nothing after",
    },
    Strategy {
        name: "acc+paddle+rest",
        shape: shape(Level::Accurate, PaddleUse::Fallback, true, None, false),
        revision_all: None,
        role: Role::Candidate,
        what: "acc+paddle, and old's rungs after it when the neural recogniser read nothing too",
    },
    Strategy {
        name: "rev2-tight",
        shape: shape(Level::Accurate, PaddleUse::Off, true, Some(2), false),
        revision_all: None,
        role: Role::Candidate,
        what: "old's ladder with its first pass under revision 2",
    },
    Strategy {
        name: "rev3>rev2",
        shape: shape(Level::Accurate, PaddleUse::Off, true, None, true),
        revision_all: None,
        role: Role::Candidate,
        what: "old's ladder, but when the first pass (Vision's default revision, 3) read nothing, revision 2 over \
               the same crop instead of the whole region",
    },
    Strategy {
        name: "fast=paddle-checked",
        shape: shape(Level::Fast, PaddleUse::AgreeChecked, true, None, false),
        revision_all: None,
        role: Role::Candidate,
        what: "the fast level first, the neural recogniser beside it at high priority: fast's text, with the \
               recogniser's spacing, when the recogniser read the same, spaces included (as prod's check judges); \
               otherwise old's accurate ladder without the fast rung, and nothing when that reads nothing — the \
               recogniser's text never (the reading rules of 2026-10-02)",
    },
    Strategy {
        name: "fast=paddle-strict",
        shape: shape(Level::Fast, PaddleUse::AgreeStrict, true, None, false),
        revision_all: None,
        role: Role::Candidate,
        what: "the fast level first, the neural recogniser beside it at high priority: fast's text, with the \
               recogniser's spacing, when the recogniser read the same, spaces included (as prod's check judges); \
               otherwise old's accurate ladder without the fast rung, and nothing when that reads nothing; when fast \
               read nothing, an accurate pass first and the recogniser's text only when that reads nothing too",
    },
    Strategy {
        name: "fast=paddle",
        shape: shape(Level::Fast, PaddleUse::Agree, true, None, false),
        revision_all: None,
        role: Role::Candidate,
        what: "fast=paddle-strict, but the recogniser's text at once when fast read nothing",
    },
    Strategy {
        name: "fast>acc",
        shape: shape(Level::Fast, PaddleUse::Off, true, None, false),
        revision_all: None,
        role: Role::Comparison,
        what: "the fast level first, its text taken unchecked; old's ladder when it read nothing (for comparison: \
               no neural recogniser)",
    },
];

/// The strategy with this name, by its index in [`STRATEGIES`].
pub fn strategy(name: &str) -> Option<usize> {
    STRATEGIES.iter().position(|s| s.name == name)
}

/// The role of the strategy with this name; a name that is none is a comparison, never named.
pub fn role(name: &str) -> Role {
    strategy(name).map_or(Role::Comparison, |i| STRATEGIES[i].role)
}

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
    /// One accurate pass over a small field, `field-DEF@1x`, through the content crop as a read
    /// makes it: whether a warm-up a third as long warms as well as the line of words.
    Field,
    /// None at all.
    Nothing,
}

/// The field the `Field` warm-up reads.
pub const WARM_UP_FIELD: &str = "field-DEF@1x";

impl Warmup {
    pub const ALL: [Warmup; 4] = [Warmup::Bars, Warmup::Word, Warmup::Field, Warmup::Nothing];

    pub fn word(self) -> &'static str {
        match self {
            Warmup::Bars => "bars",
            Warmup::Word => "word",
            Warmup::Field => "field",
            Warmup::Nothing => "none",
        }
    }

    fn parse(s: &str) -> Option<Warmup> {
        Warmup::ALL.into_iter().find(|w| w.word() == s)
    }
}

/// What a neural recogniser's first-pass child does before its first recognition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaddleStart {
    /// Only the session made, timed.
    Nothing,
    /// The application's warm-up — the session, and one run over a dark dummy — on a thread of
    /// its own, waited for.
    Warm,
    /// That warm-up and Vision's over the line of words started together, as they would run if
    /// the neural recogniser did not wait for Vision's: whether one delays the other, and Vision's
    /// first real pass after them.
    WithVision,
}

impl PaddleStart {
    pub const ALL: [PaddleStart; 3] = [PaddleStart::Nothing, PaddleStart::Warm, PaddleStart::WithVision];

    pub fn word(self) -> &'static str {
        match self {
            PaddleStart::Nothing => "none",
            PaddleStart::Warm => "warm",
            PaddleStart::WithVision => "with-vision",
        }
    }

    fn parse(s: &str) -> Option<PaddleStart> {
        PaddleStart::ALL.into_iter().find(|w| w.word() == s)
    }
}

/// The language of the second-language child's first passes: one of the accurate level's, and
/// the testers' own, not Vision's default.
pub const SECOND_LANGUAGE: &str = "de-DE";

/// The first-pass children of round `r`, in the order they are started: every kind once, the
/// list turned by one each round, so that no kind is always the one after the discarded first
/// process, or always the last.
pub fn first_roles(r: usize) -> Vec<Child> {
    let mut all: Vec<Child> = Warmup::ALL.iter().map(|w| Child::FirstPass(*w)).collect();
    all.extend([Child::FastFirst, Child::SecondLanguage]);
    all.extend(PaddleStart::ALL.iter().map(|p| Child::PaddleFirst(*p)));
    let n = all.len();
    all.rotate_left(r % n);
    all
}

// ── The command line ─────────────────────────────────────────────────────────────────────────

pub const USAGE: &str = "\
automation-platform ocr-bench [--quick] [--quiet] [--long-idle] [--capture-ms N]
                              [--pictures DIR] [--out FILE] [--summary FILE]
automation-platform ocr-bench --paddle [--quick] [--quiet] [--pictures DIR] [--out FILE]
automation-platform ocr-bench --paddle-probe

Measures what Apple Vision's text recognition costs on this Mac, on fixed pictures carried
inside the application: no screen capture, no window, no permission, no speech; and, where
the application carries ONNX Runtime, the neural recogniser beside it. Quit Automation
Platform first, so that its own reads do not compete for the processor.

  --quick         a shorter run: 3 passes a cell instead of 6 (too few for a verdict),
                  2 pictures for the variants instead of 5, 3 pictures for the ways of
                  reading instead of every one, one process per first-pass kind instead
                  of three, blocks of 5 s instead of 30 under sustained load, and 48 s of
                  idle instead of 168
  --quiet         print only where the file is and that it is done; everything else
                  goes to the file alone (for a screen reader, which reads every line)
  --long-idle     also one pass after 1, 2, 5 and 10 minutes with nothing to do, on the
                  thread that read before and on a fresh one: 36 minutes more
  --capture-ms N  count the retry ladder's time budget from N ms before each pipeline
                  read, for the screen capture it does not make (default 50)
  --pictures DIR  also read every picture DIR/manifest.toml lists, each a PNG beside it
                  (the real captures in crates/host/bench-data/ocr/real/, which
                  tools/ocr-fixtures/crop.py cuts); a wrong manifest is refused at once
  --paddle        the neural recogniser (PaddleOCR) alone over every picture (on Windows
                  beside what the system recogniser reads); then how wide a line it reads
                  right, its crop for Retina pictures, and its scores
  --paddle-probe  only whether the neural recogniser loads and reads one picture: one
                  line, \"ok\" or \"not available\" with the reason; exit 0 or 1
  --out FILE      where to write what it prints (default: ocr-bench-N.txt beside the
                  application, the first N that is free)
  --summary FILE  append each section's table as Markdown to FILE as soon as the
                  section is done (CI passes $GITHUB_STEP_SUMMARY)

macOS only, but for --paddle and --paddle-probe, which run on Windows too. Apart from the
tables, every line it prints starts with \"OCR BENCH:\", and the last one says \"done\".";

/// The pretend capture time the pipeline's ladder budget starts with, unless `--capture-ms`
/// says otherwise: the median capture on the tester's Intel Air, 49 ms, rounded.
pub const DEFAULT_CAPTURE_MS: u64 = 50;

/// An internal role: this process was started by a running benchmark to do one thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Child {
    /// A first pass in a fresh process, after this warm-up.
    FirstPass(Warmup),
    /// The fast level's first pass in a fresh process, with no warm-up of any kind.
    FastFirst,
    /// The word warm-up in Vision's default language, then the first passes in
    /// [`SECOND_LANGUAGE`].
    SecondLanguage,
    /// The neural recogniser's first recognitions in a fresh process, after this start.
    PaddleFirst(PaddleStart),
    /// One pass of the variant at this index in [`VARIANTS`], before the benchmark's own
    /// process risks it.
    Probe(usize),
}

impl Child {
    /// What follows `--child` for it.
    pub fn arg(self) -> String {
        match self {
            Child::FirstPass(w) => format!("first-pass:{}", w.word()),
            Child::FastFirst => "fast-first".to_string(),
            Child::SecondLanguage => "second-language".to_string(),
            Child::PaddleFirst(p) => format!("paddle-first:{}", p.word()),
            Child::Probe(i) => format!("probe:{}", VARIANTS[i].name),
        }
    }

    /// How the first-pass section names it.
    pub fn word(self) -> String {
        match self {
            Child::FirstPass(w) => format!("warm-up {}", w.word()),
            Child::FastFirst => "the fast level, no warm-up".to_string(),
            Child::SecondLanguage => format!("warm-up word, then {SECOND_LANGUAGE}"),
            Child::PaddleFirst(p) => format!("neural recogniser, {}", p.word()),
            Child::Probe(i) => format!("probe {}", VARIANTS[i].name),
        }
    }

    fn parse(v: &str) -> Option<Child> {
        if let Some(w) = v.strip_prefix("first-pass:") {
            return Warmup::parse(w).map(Child::FirstPass);
        }
        if let Some(p) = v.strip_prefix("paddle-first:") {
            return PaddleStart::parse(p).map(Child::PaddleFirst);
        }
        match v {
            "fast-first" => return Some(Child::FastFirst),
            "second-language" => return Some(Child::SecondLanguage),
            _ => {}
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
    /// `--paddle`: the neural recogniser's measurement instead of Vision's.
    pub paddle: bool,
    /// `--paddle-probe`: whether the neural recogniser loads and reads, and nothing else.
    pub paddle_probe: bool,
    /// The pictures `--pictures` added, read as the command line is.
    pub pictures: Vec<&'static Fixture>,
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
            paddle: false,
            paddle_probe: false,
            pictures: Vec::new(),
            child: None,
        }
    }
}

/// The arguments after `ocr-bench`. `--pictures` reads its manifest and pictures here, so that a
/// wrong one is said before a run of many minutes starts rather than after.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--quick" => o.quick = true,
            "--quiet" => o.quiet = true,
            "--long-idle" => o.long_idle = true,
            "--paddle" => o.paddle = true,
            "--paddle-probe" => o.paddle_probe = true,
            "--help" | "-h" => o.help = true,
            "--out" | "--summary" | "--child" | "--capture-ms" | "--pictures" => {
                let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                match a.as_str() {
                    "--out" => o.out = Some(PathBuf::from(v)),
                    "--summary" => o.summary = Some(PathBuf::from(v)),
                    "--pictures" => {
                        if !o.pictures.is_empty() {
                            return Err("--pictures is given once".to_string());
                        }
                        o.pictures = load_pictures(Path::new(v))?;
                    }
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

/// What reads after a pause in the idle section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleEngine {
    /// One accurate pass of today's request.
    Accurate,
    /// One pass of the fast level.
    Fast,
    /// One recognition of the neural recogniser, on its own thread.
    Paddle,
}

impl IdleEngine {
    pub fn word(self) -> &'static str {
        match self {
            IdleEngine::Accurate => "an accurate pass",
            IdleEngine::Fast => "a fast pass",
            IdleEngine::Paddle => "a recognition of the neural recogniser",
        }
    }
}

/// One pause in the idle section, the thread that reads after it, and what it reads with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Idle {
    pub secs: u64,
    /// A thread started for this one pass, rather than the one that read before the pause.
    pub fresh: bool,
    pub engine: IdleEngine,
}

impl Idle {
    const fn known(secs: u64) -> Idle {
        Idle { secs, fresh: false, engine: IdleEngine::Accurate }
    }

    const fn fresh(secs: u64) -> Idle {
        Idle { secs, fresh: true, engine: IdleEngine::Accurate }
    }

    const fn fast(secs: u64) -> Idle {
        Idle { secs, fresh: false, engine: IdleEngine::Fast }
    }

    const fn paddle(secs: u64) -> Idle {
        Idle { secs, fresh: false, engine: IdleEngine::Paddle }
    }

    pub fn thread(self) -> &'static str {
        match (self.engine, self.fresh) {
            (IdleEngine::Paddle, _) => "the neural recogniser's thread",
            (_, true) => "a fresh thread",
            (_, false) => "the thread that read before",
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
    /// The pauses, in the order they are taken: the lengths, the threads and the engines
    /// interleaved.
    pub idle: Vec<Idle>,
    /// The pictures every strategy of the pipeline reads `samples` times, for a verdict on its
    /// speed, by label — a drawn one, or one `--pictures` may add (left out where it is not
    /// there). Every other small picture is read twice, for whether it is read right.
    pub speed_pictures: &'static [&'static str],
    /// Whether the pipeline reads the pictures off that list at all.
    pub every_picture: bool,
    /// Each block of the sustained load, in seconds; four blocks.
    pub sustained_secs: u64,
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
                idle: vec![
                    Idle::known(2),
                    Idle::fresh(10),
                    Idle::fast(2),
                    Idle::paddle(10),
                    Idle::fresh(2),
                    Idle::known(10),
                    Idle::fast(10),
                    Idle::paddle(2),
                ],
                speed_pictures: &["field-64@2x", "lone-1@2x", "sfz-tune@1x"],
                every_picture: false,
                sustained_secs: 5,
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
                    Idle::fast(2),
                    Idle::paddle(10),
                    Idle::known(30),
                    Idle::fast(10),
                    Idle::fresh(2),
                    Idle::paddle(2),
                    Idle::known(10),
                    Idle::fast(30),
                    Idle::fresh(30),
                    Idle::paddle(30),
                ],
                // A field read in one pass at both scales, the lone digit at both, the light
                // field with its rule, a signed value in a light field and in a well, ink that is
                // not text, and the real captures the CI set carries: sforzando's two lone zeros,
                // its Polyphony and Instrument fields, and Melodyne's cents.
                speed_pictures: &[
                    "field-64@1x",
                    "field-64@2x",
                    "lone-1@1x",
                    "lone-1@2x",
                    "field-empty@2x",
                    "val-db@2x",
                    "val-minus12@2x",
                    "none-bars@2x",
                    "sfz-tune@1x",
                    "sfz-trans@1x",
                    "sfz-polyphony@1x",
                    "sfz-instrument@1x",
                    "mel-cents@1x",
                ],
                every_picture: true,
                sustained_secs: 30,
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

/// Whether `read` is one of `accept`, after [`normalise`]: exactly, case and all.
pub fn accepted(read: &str, accept: &[&str]) -> bool {
    let read = normalise(read);
    accept.iter().any(|a| normalise(a) == read)
}

/// Whether `read` is one of `accept` once spaces are dropped and dash and quote forms made one
/// ([`paddle_pre::same`]): "0.00dB" for "0.00 dB", "−12" for "-12". Nothing still has to be
/// nothing.
pub fn accepted_loosely(read: &str, accept: &[&str]) -> bool {
    accept.iter().any(|a| paddle_pre::same(read, a))
}

/// How a cell's reads compare with the accepted answers.
#[derive(Clone, Debug, PartialEq)]
pub enum Accuracy {
    /// Every read was right.
    Right,
    /// Every read was right once spaces and dash and quote forms are set aside
    /// ([`accepted_loosely`]), and not every one exactly: how many were exactly right, of how
    /// many, and the commonest reading that was not.
    Loosely { right: usize, of: usize, got: String },
    /// Not every read was right even so: how many were exactly right and how many loosely, of
    /// how many, the commonest wrong reading, and whether it was text on a picture that holds
    /// none — an invented reading.
    Wrong { right: usize, loose: usize, of: usize, got: String, invented: bool },
    /// Every request was refused.
    Refused,
    /// Nothing was measured.
    Nothing,
}

impl Accuracy {
    /// Exactly right on every read.
    pub fn right(&self) -> bool {
        matches!(self, Accuracy::Right)
    }

    /// Right on every read, loosely or exactly.
    pub fn loosely_right(&self) -> bool {
        matches!(self, Accuracy::Right | Accuracy::Loosely { .. })
    }

    /// Wrong only by refusals: every request refused, or every reading that was not refused
    /// accepted, loosely at least.
    pub fn refused_only(&self) -> bool {
        match self {
            Accuracy::Refused => true,
            Accuracy::Wrong { got, .. } => got == "(refused)",
            _ => false,
        }
    }
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

    /// Its reads, the first included, against the answers `accept` allows (see [`Fixture`]).
    pub fn accuracy(&self, accept: &[&str]) -> Accuracy {
        let all: Vec<&Sample> = self.first.iter().chain(self.warm.iter()).collect();
        if all.is_empty() {
            return Accuracy::Nothing;
        }
        let read: Vec<String> = all.iter().filter_map(|s| s.text.as_deref().map(normalise)).collect();
        judge(&read, all.len(), accept)
    }
}

/// `read` (the readings that were not refused) of `of` reads, against `accept`.
pub fn judge(read: &[String], of: usize, accept: &[&str]) -> Accuracy {
    if of == 0 {
        return Accuracy::Nothing;
    }
    if read.is_empty() {
        return Accuracy::Refused;
    }
    let right = read.iter().filter(|t| accepted(t, accept)).count();
    let loose = read.iter().filter(|t| accepted_loosely(t, accept)).count();
    if right == of {
        return Accuracy::Right;
    }
    let commonest = |pick: &dyn Fn(&String) -> bool| -> String {
        let mut these: Vec<&String> = read.iter().filter(|t| pick(t)).collect();
        these.sort();
        these
            .iter()
            .max_by_key(|t| these.iter().filter(|u| u == t).count())
            .map(|t| t.to_string())
            .unwrap_or_else(|| "(refused)".to_string())
    };
    if loose == of {
        return Accuracy::Loosely { right, of, got: commonest(&|t| !accepted(t, accept)) };
    }
    let got = commonest(&|t| !accepted_loosely(t, accept));
    let invented = accept == [""] && !got.is_empty() && got != "(refused)";
    Accuracy::Wrong { right, loose, of, got, invented }
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
    let t = cell.warm.first().or(cell.first.as_ref()).and_then(|s| s.text.as_deref()).unwrap_or("");
    accuracy_words(a, t, cell.error.as_deref())
}

/// How [`Accuracy`] is said in a line: `read` is a right reading (for [`Accuracy::Right`]), `error`
/// the reason the recogniser gave for refusing (for [`Accuracy::Refused`]).
pub fn accuracy_words(a: &Accuracy, read: &str, error: Option<&str>) -> String {
    let shown = |t: &str| if t.is_empty() { "nothing".to_string() } else { format!("\"{t}\"") };
    match a {
        Accuracy::Right if normalise(read).is_empty() => "read nothing, which is right".to_string(),
        Accuracy::Right => format!("read {} right", shown(&normalise(read))),
        Accuracy::Loosely { right, of, got } => {
            format!("read {} right but for spaces or dash forms ({right} of {of} exactly right)", shown(got))
        }
        Accuracy::Wrong { got, invented: true, right, of, .. } => {
            format!("WRONG, INVENTED: read {} where there is no text ({right} of {of} right)", shown(got))
        }
        Accuracy::Wrong { right, loose, of, got, .. } if loose > right => {
            format!("WRONG: read {} ({right} of {of} right, {loose} but for spaces or dash forms)", shown(got))
        }
        Accuracy::Wrong { right, of, got, .. } => format!("WRONG: read {} ({right} of {of} right)", shown(got)),
        Accuracy::Refused => {
            format!("the recogniser refused every request{}", error.map(|e| format!(": {e}")).unwrap_or_default())
        }
        Accuracy::Nothing => "nothing measured".to_string(),
    }
}

/// A short mark for a table cell: "" when right, else what was not.
pub fn accuracy_mark(a: &Accuracy) -> &'static str {
    match a {
        Accuracy::Right | Accuracy::Nothing => "",
        Accuracy::Loosely { .. } => ", right but for spaces",
        Accuracy::Wrong { invented: true, .. } => ", wrong, invented",
        Accuracy::Wrong { .. } => ", wrong",
        Accuracy::Refused => ", refused",
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
            return format!("{head} | {}", accuracy_text(&cell.accuracy(picture.accept), cell));
        };
        let first = cell.first.as_ref().map(|f| format!(", first {} ms", ms(f.ms))).unwrap_or_default();
        format!("{head} | {}{first} | {}", stats_text(&s), accuracy_text(&cell.accuracy(picture.accept), cell))
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

    /// Whether the control, prod-b, came out "clearly" faster or slower than prod on any
    /// picture: then this run is too noisy for any verdict, the pipeline's included.
    pub fn control_loud(&self) -> bool {
        self.index_of(Tweak::Control).is_some_and(|c| {
            (0..self.pictures.len()).any(|p| matches!(self.verdict(c, p), Some(Verdict::Faster(..) | Verdict::Slower(..))))
        })
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
                let acc = cell.accuracy(picture.accept);
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
                        let wrong = accuracy_mark(&acc);
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

/// Something measured beside another thing against itself alone, by the engine's rule:
/// `median 340 ms against 300 ms alone: clearly slower (+13 %, p 0.002)`.
pub fn against_alone(alone: &[f64], beside: &[f64]) -> String {
    match (Stats::of(alone), Stats::of(beside)) {
        (Some(a), Some(b)) => {
            format!("median {} ms against {} ms alone: {}", ms(b.median), ms(a.median), verdict_text(verdict(alone, beside)))
        }
        _ => "not both measured".to_string(),
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

/// What a read did besides its answer, as the pipeline counts it per strategy.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadNote {
    /// The Vision passes it made, and their milliseconds in all.
    pub passes: usize,
    pub vision_ms: f64,
    /// Runs of the neural recogniser's model it began.
    pub paddle_runs: u64,
    pub answer: Answer,
    /// A fast first pass beside the neural recogniser: whether the two read the same, when either
    /// read anything.
    pub agreed: Option<bool>,
    /// The strategy wanted the neural recogniser and old's ladder read the region instead: not one
    /// line of ink, too wide, or the recogniser was not ready or not asked.
    pub today: bool,
}

/// One picture through the whole production pipeline, capture excepted, under one strategy.
pub struct PipelineRow {
    /// A name in [`STRATEGIES`].
    pub strategy: &'static str,
    pub picture: &'static Fixture,
    pub cell: Cell,
    /// One a read, the first included, in the order taken.
    pub notes: Vec<ReadNote>,
    /// Read `samples` times for a verdict on its speed; otherwise twice, for whether it reads it
    /// right.
    pub speed: bool,
}

impl PipelineRow {
    pub fn new(strategy: &'static str, picture: &'static Fixture, speed: bool) -> PipelineRow {
        PipelineRow { strategy, picture, cell: Cell::default(), notes: Vec::new(), speed }
    }

    pub fn push(&mut self, sample: Sample, note: ReadNote) {
        self.cell.push(sample);
        self.notes.push(note);
    }

    /// Whether this row still takes a read under `plan`.
    pub fn wants_more(&self, plan: &Plan) -> bool {
        if self.speed {
            self.cell.wants_more(plan)
        } else {
            self.cell.skipped.is_none() && self.cell.warm.is_empty()
        }
    }

    /// The notes of the warm reads.
    fn warm_notes(&self) -> &[ReadNote] {
        self.notes.get(usize::from(self.cell.first.is_some())..).unwrap_or_default()
    }

    /// Mean neural recogniser runs per warm read.
    pub fn paddle_runs(&self) -> Option<f64> {
        let n = self.warm_notes();
        (!n.is_empty()).then(|| n.iter().map(|r| r.paddle_runs as f64).sum::<f64>() / n.len() as f64)
    }

    /// Who answered how often, over every read, in a fixed order: `tight 6, Paddle 1`.
    pub fn answers(&self) -> String {
        let order = [
            Answer::Pass(Stage::AsCaptured),
            Answer::Pass(Stage::FastFirst),
            Answer::Pass(Stage::Tight),
            Answer::Pass(Stage::TightAgain),
            Answer::Pass(Stage::Whole),
            Answer::Pass(Stage::Enlarged),
            Answer::Pass(Stage::Fast),
            Answer::Paddle,
            Answer::Nobody,
        ];
        let said: Vec<String> = order
            .iter()
            .filter_map(|a| {
                let n = self.notes.iter().filter(|r| r.answer == *a).count();
                (n > 0).then(|| format!("{} {n}", a.word()))
            })
            .collect();
        if said.is_empty() {
            "none".to_string()
        } else {
            said.join(", ")
        }
    }

    /// How often the fast level and the neural recogniser read the same, of the reads where
    /// either read anything: `(same, of)`.
    pub fn agreement(&self) -> Option<(usize, usize)> {
        let asked: Vec<bool> = self.notes.iter().filter_map(|r| r.agreed).collect();
        (!asked.is_empty()).then(|| (asked.iter().filter(|a| **a).count(), asked.len()))
    }

    /// The median of what a warm read spent outside Vision's passes: the conversion, the crop,
    /// rendering, the neural recogniser's wait.
    pub fn not_vision(&self) -> Option<f64> {
        let v: Vec<f64> =
            self.cell.warm.iter().zip(self.warm_notes()).map(|(s, n)| (s.ms - n.vision_ms).max(0.0)).collect();
        Stats::of(&v).map(|s| s.median)
    }

    /// How many reads old's ladder made instead of the strategy's.
    pub fn read_by_today(&self) -> usize {
        self.notes.iter().filter(|r| r.today).count()
    }

    pub fn line(&self) -> String {
        let head = format!("pipeline | {} | {}", self.strategy, self.picture.label());
        let acc = accuracy_text(&self.cell.accuracy(self.picture.accept), &self.cell);
        if let Some(why) = &self.cell.skipped {
            return format!("{head} | not measured: {why}");
        }
        let skipped = self.cell.warm.iter().any(|s| s.skipped);
        let (Some(s), Some(p)) = (self.cell.stats(), self.cell.passes()) else {
            return format!("{head} | {acc}");
        };
        let runs = self.paddle_runs().filter(|r| *r > 0.0).map(|r| format!(" and {r:.1} neural runs")).unwrap_or_default();
        let today = match self.read_by_today() {
            0 => String::new(),
            n => format!(", {n} of {} read by old's ladder (not the neural recogniser's to read, or it was not asked)", self.notes.len()),
        };
        let agreed = self
            .agreement()
            .map(|(same, of)| format!(" | fast and the neural recogniser read the same {same} of {of} times"))
            .unwrap_or_default();
        let not_vision = self.not_vision().map(|m| format!(" | not Vision: median {} ms", ms(m))).unwrap_or_default();
        format!(
            "{head} | {}, {p:.1} Vision passes{runs} a read{}{}{today} | answered by {}{agreed}{not_vision} | {acc}",
            stats_text(&s),
            self.cell.first.as_ref().map(|f| format!(", first read {} ms", ms(f.ms))).unwrap_or_default(),
            if skipped { ", the blank guard answered" } else { "" },
            self.answers()
        )
    }
}

/// Every strategy over every picture: `rows[s][p]`.
pub struct Pipeline {
    pub strategies: Vec<&'static Strategy>,
    pub pictures: Vec<&'static Fixture>,
    pub rows: Vec<Vec<PipelineRow>>,
}

impl Pipeline {
    /// Every strategy over every picture, each row marked for speed or for reading only by
    /// `plan`, and left out — with the reason — where it is no measurement: a large picture
    /// under a strategy that reads it as prod does, a picture a quick run does not read.
    pub fn new(strategies: Vec<&'static Strategy>, pictures: Vec<&'static Fixture>, plan: &Plan) -> Pipeline {
        let rows = strategies
            .iter()
            .map(|s| {
                pictures
                    .iter()
                    .map(|p| {
                        let speed = plan.speed_pictures.contains(&p.label().as_str());
                        let mut row = PipelineRow::new(s.name, p, speed && p.small());
                        if !p.small() && s.name != "prod" && s.as_prod_when_large() {
                            row.cell = Cell::skip("a region this large is read as prod reads it, whatever the strategy");
                        } else if !speed && !plan.every_picture {
                            row.cell = Cell::skip("a quick run reads only its few pictures");
                        }
                        row
                    })
                    .collect()
            })
            .collect();
        Pipeline { strategies, pictures, rows }
    }

    /// Leaves out every row of strategy `s`, with the reason.
    pub fn skip(&mut self, s: usize, why: &str) {
        for row in &mut self.rows[s] {
            if row.cell.skipped.is_none() {
                row.cell = Cell::skip(why);
            }
        }
    }

    /// Leaves out `old` where `prod` reads with its ladder (`prod`, prod's shape on this Mac, is
    /// [`Shape::TODAY`]): where the neural recogniser is not there. Read twice, the same ladder
    /// tells nothing, and noise could make `old` "clearly faster" somewhere and the closing line
    /// call the way the application reads there a comparison only. The reason, when it does.
    pub fn without_old_where_prod_is_old(&mut self, prod: Shape) -> Option<&'static str> {
        let i = self.strategies.iter().position(|s| s.name == "old").filter(|_| prod.is_today())?;
        let why = "prod reads as old here: the neural recogniser is not there";
        self.skip(i, why);
        Some(why)
    }

    pub fn count(&self) -> usize {
        self.strategies.len() * self.pictures.len()
    }

    /// The strategy and the picture of row `k`.
    pub fn at(&self, k: usize) -> (usize, usize) {
        (k / self.pictures.len().max(1), k % self.pictures.len().max(1))
    }

    fn prod(&self, p: usize) -> Option<&PipelineRow> {
        self.strategies.iter().position(|s| s.name == "prod").map(|i| &self.rows[i][p])
    }

    /// Strategy `s` against prod on picture `p`, when both were read for speed.
    pub fn verdict(&self, s: usize, p: usize) -> Option<Verdict> {
        if self.strategies[s].name == "prod" {
            return None;
        }
        let (prod, row) = (self.prod(p)?, &self.rows[s][p]);
        let measured = |r: &PipelineRow| r.cell.skipped.is_none() && !r.cell.warm.is_empty();
        if !row.speed || !measured(row) || !measured(prod) {
            return None;
        }
        Some(verdict(&prod.cell.warm_ms(), &row.cell.warm_ms()))
    }

    pub fn verdict_line(&self, s: usize, p: usize) -> Option<String> {
        let v = self.verdict(s, p)?;
        let prod = self.prod(p)?.cell.stats()?;
        Some(format!(
            "pipeline | verdict | {} | {} | against prod {} ms: {}",
            self.strategies[s].name,
            self.pictures[p].label(),
            ms(prod.median),
            verdict_text(v)
        ))
    }

    /// One row a picture, one column a strategy: the median of a whole read, and what was not as
    /// it should be — "faster" or "slower" than prod by the rule, "wrong", "invented", "refused".
    /// "-" for a row not measured.
    pub fn table(&self) -> String {
        let mut t = String::from("| picture |");
        for s in &self.strategies {
            t.push_str(&format!(" {} |", s.name));
        }
        t.push_str("\n|---|");
        for _ in &self.strategies {
            t.push_str("---:|");
        }
        t.push('\n');
        for (p, picture) in self.pictures.iter().enumerate() {
            let mark = if picture.holds_nothing() {
                " (no text)"
            } else if picture.accept.contains(&"") {
                " (or nothing)"
            } else {
                ""
            };
            t.push_str(&format!("| {}{mark} |", picture.label()));
            for s in 0..self.strategies.len() {
                let row = &self.rows[s][p];
                let cell = match (row.cell.skipped.as_ref(), row.cell.stats()) {
                    (Some(_), _) => "-".to_string(),
                    (None, None) => "refused".to_string(),
                    (None, Some(st)) => {
                        let mark = match self.verdict(s, p) {
                            Some(Verdict::Faster(..)) => " faster",
                            Some(Verdict::Slower(..)) => " slower",
                            _ => "",
                        };
                        format!("{}{mark}{}", ms(st.median), accuracy_mark(&row.cell.accuracy(picture.accept)))
                    }
                };
                t.push_str(&format!(" {cell} |"));
            }
            t.push('\n');
        }
        t
    }

    /// What each strategy came to over every picture, for the closing line.
    pub fn outcomes(&self) -> Vec<Outcome> {
        let prod = self.strategies.iter().position(|s| s.name == "prod");
        (0..self.strategies.len())
            .map(|s| {
                let mut o = Outcome { name: self.strategies[s].name.to_string(), ..Outcome::default() };
                let rows = &self.rows[s];
                if rows.iter().all(|r| r.cell.skipped.is_some()) {
                    // The strategy's own reason, which a small picture's row carries; a large
                    // one's says only that prod reads it.
                    let small = rows.iter().zip(&self.pictures).find(|(_, p)| p.small()).map(|(r, _)| r);
                    o.skipped = small.or(rows.first()).and_then(|r| r.cell.skipped.clone()).or(Some("no picture".into()));
                    return o;
                }
                for (p, row) in rows.iter().enumerate() {
                    let picture = self.pictures[p];
                    let prod_row = prod.map(|i| &self.rows[i][p]).filter(|r| r.cell.skipped.is_none());
                    // A row this strategy leaves to prod is prod's: read as prod reads it.
                    let row = if row.cell.skipped.is_some() && !picture.small() { prod_row.unwrap_or(row) } else { row };
                    if row.cell.skipped.is_some() {
                        continue;
                    }
                    let acc = row.cell.accuracy(picture.accept);
                    let prod_right = prod_row.is_some_and(|r| r.cell.accuracy(picture.accept).right());
                    let label = picture.label();
                    // Judged exactly, as prod is. Text on a picture without any is invented; a
                    // picture on which nothing is right and that was only refused said nothing,
                    // which is no mistake; anything else not right is a misreading, on every
                    // picture with text, whether it must be read or may be read as nothing.
                    if !acc.right() {
                        let entry = (label.clone(), !prod_right);
                        if matches!(acc, Accuracy::Wrong { invented: true, .. }) {
                            o.invented.push(entry);
                        } else if acc.refused_only() && picture.accept.contains(&"") {
                            o.refused.push(entry);
                        } else {
                            o.misread.push(entry);
                        }
                    }
                    match self.verdict(s, p) {
                        Some(Verdict::Faster(..)) => o.faster.push(label.clone()),
                        Some(Verdict::Slower(..)) => o.slower.push(label.clone()),
                        _ => {}
                    }
                    if let Some(v) = self.verdict(s, p) {
                        if !matches!(v, Verdict::TooFew) {
                            o.judged += 1;
                            o.sum_ms += row.cell.stats().map_or(0.0, |st| st.median);
                            o.prod_sum_ms += prod_row.and_then(|r| r.cell.stats()).map_or(0.0, |st| st.median);
                        }
                    }
                }
                o
            })
            .collect()
    }

    /// One row a strategy: its role, what it read right, and its verdicts.
    pub fn summary_table(&self) -> String {
        let mut t = String::from(
            "| strategy | role | pictures read wrong | invented | refused | clearly faster on | clearly slower on | \
             medians over the judged pictures, ms (prod's) |\n|---|---|---|---|---|---|---|---|\n",
        );
        let names = |v: &[(String, bool)]| {
            if v.is_empty() {
                "none".to_string()
            } else {
                let shown: Vec<String> =
                    v.iter().map(|(l, prod_too)| format!("{l}{}", if *prod_too { " (prod too)" } else { "" })).collect();
                shown.join(", ")
            }
        };
        for o in self.outcomes() {
            let role = role(&o.name).word();
            if let Some(why) = &o.skipped {
                t.push_str(&format!("| {} | {role} | not measured: {why} | | | | | |\n", o.name));
                continue;
            }
            t.push_str(&format!(
                "| {} | {role} | {} | {} | {} | {} | {} | {} |\n",
                o.name,
                names(&o.misread),
                names(&o.invented),
                names(&o.refused),
                if o.faster.is_empty() { "none".into() } else { o.faster.join(", ") },
                if o.slower.is_empty() { "none".into() } else { o.slower.join(", ") },
                if o.judged == 0 { "n/a".to_string() } else { format!("{} ({})", ms(o.sum_ms), ms(o.prod_sum_ms)) }
            ));
        }
        t
    }

    /// What the application's reading came to, in one line whatever the run's verdicts: the
    /// pictures `prod` read wrong, the text it invented, the refusals — the line CI warns by.
    /// `None` when `prod` was not measured.
    pub fn prod_line(&self) -> Option<String> {
        self.came_to("prod", "the application's reading (prod)")
    }

    /// The same of `old`, the ladder the application read with before, beside it: what the
    /// change made of every picture. Not warned by.
    pub fn old_line(&self) -> Option<String> {
        self.came_to("old", "the ladder before it (old)")
    }

    /// `pipeline | <who> …` for the strategy `name`: what it read wrong, invented and was refused
    /// on, or that it read every picture right; `None` when it was not measured.
    fn came_to(&self, name: &str, who: &str) -> Option<String> {
        let outcomes = self.outcomes();
        let o = outcomes.iter().find(|o| o.name == name && o.skipped.is_none())?;
        let labels = |v: &[(String, bool)]| v.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>().join(", ");
        let mut parts = Vec::new();
        if !o.misread.is_empty() {
            parts.push(format!("reads wrong: {}", labels(&o.misread)));
        }
        if !o.invented.is_empty() {
            parts.push(format!("invents text on: {}", labels(&o.invented)));
        }
        if !o.refused.is_empty() {
            parts.push(format!("was refused on: {}", labels(&o.refused)));
        }
        Some(if parts.is_empty() {
            format!("pipeline | {who} reads every picture it read right")
        } else {
            format!("pipeline | {who} {}", parts.join("; "))
        })
    }
}

/// The pipeline's line saying which reading `prod` is on this Mac (`ladder::Shape::production`):
/// with the neural recogniser where it is ready in this process (`paddle_ready`), checked by the
/// fast level first on an Intel Mac (`intel`), and old's ladder where it is not there.
pub fn prod_reads_as(paddle_ready: bool, intel: bool) -> String {
    format!(
        "pipeline | prod reads as the application does on this Mac: {}",
        match (paddle_ready, intel) {
            (false, _) => "as old, the neural recogniser not being there",
            (true, false) => "Windows' rule over the accurate ladder, as on Apple silicon",
            (true, true) => {
                "the fast level checked by the neural recogniser, then Windows' rule over the accurate ladder, as on an \
                 Intel Mac"
            }
        }
    )
}

/// What one strategy came to over every picture, as the closing line weighs it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub name: String,
    /// Why it was not measured at all.
    pub skipped: Option<String>,
    /// Pictures with text that it did not read right on every read — those that must be read, and
    /// those on which nothing is right too — each with whether prod did not either.
    pub misread: Vec<(String, bool)>,
    /// Pictures without text on which it read text, each with whether prod did too.
    pub invented: Vec<(String, bool)>,
    /// Pictures on which nothing is right and which Vision only refused: nothing was said, so no
    /// mistake, but said apart.
    pub refused: Vec<(String, bool)>,
    /// Where it was clearly faster than prod, and clearly slower.
    pub faster: Vec<String>,
    pub slower: Vec<String>,
    /// The pictures with a verdict, and its medians and prod's over them.
    pub judged: usize,
    pub sum_ms: f64,
    pub prod_sum_ms: f64,
}

/// The rule the closing line names a strategy by, printed with it.
pub const CLOSING_RULE: &str = "a candidate way of reading is named when it reads every picture with text right on \
                                every read (exactly, as prod is judged; nothing counts as right where a picture \
                                accepts it), reads nothing on every picture without text, is clearly faster than \
                                prod on at least one picture and clearly slower on none (the engine's rule, over \
                                the pictures read for speed); the cheapest is the one whose medians over those \
                                pictures sum to the least. One that is faster but not right is said, never named, \
                                and so is a comparison that would pass: those speak values nothing checked, or \
                                change more than the plan's ways would";

/// The closing lines of a run: the cheapest candidate ([`Role::Candidate`]) that reads right and
/// saves time; the strategies that are faster but not right, a misreading prod shares marked so;
/// when prod itself reads a picture wrong or invents on one without text, the same choice with
/// those pictures set aside; and the comparisons that would pass the rule, which are said and
/// never named. `noisy`: the engine's control came out "clearly" different, and no verdict of the
/// run counts.
pub fn closing_lines(outcomes: &[Outcome], noisy: bool) -> Vec<String> {
    let head = "closing";
    if noisy {
        return vec![format!(
            "{head} | this run is too noisy for verdicts (prod-b, the engine's control, came out clearly different \
             from prod): no way of reading is named"
        )];
    }
    let measured: Vec<&Outcome> = outcomes.iter().filter(|o| o.skipped.is_none() && o.name != "prod").collect();
    if measured.iter().all(|o| o.judged == 0) {
        return vec![format!("{head} | too few reads for a verdict (as in a quick run): no way of reading is named")];
    }
    let pick = |relative: bool, of: Role| -> Vec<&Outcome> {
        let counts = |v: &[(String, bool)]| v.iter().filter(|(_, prod_too)| !(relative && *prod_too)).count();
        let mut c: Vec<&Outcome> = measured
            .iter()
            .copied()
            .filter(|o| role(&o.name) == of)
            .filter(|o| counts(&o.misread) == 0 && counts(&o.invented) == 0 && !o.faster.is_empty() && o.slower.is_empty())
            .collect();
        c.sort_by(|a, b| a.sum_ms.total_cmp(&b.sum_ms));
        c
    };
    let said = |o: &Outcome| {
        format!(
            "{} (clearly faster on {} of {} pictures judged; medians {} ms against prod's {} ms over them)",
            o.name,
            o.faster.len(),
            o.judged,
            ms(o.sum_ms),
            ms(o.prod_sum_ms)
        )
    };
    let named = |c: &[&Outcome]| -> String {
        match c.split_first() {
            None => "none does".to_string(),
            Some((best, [])) => said(best),
            Some((best, rest)) => {
                let others: Vec<&str> = rest.iter().map(|o| o.name.as_str()).collect();
                format!("{}; the others that do, dearer: {}", said(best), others.join(", "))
            }
        }
    };
    let mut lines = vec![format!(
        "{head} | the cheapest way of reading that reads right and saves time: {}",
        named(&pick(false, Role::Candidate))
    )];
    // Each picture with "(prod too)" where prod did not read it right either.
    let shown = |v: &[(String, bool)]| {
        v.iter().map(|(l, prod_too)| format!("{l}{}", if *prod_too { " (prod too)" } else { "" })).collect::<Vec<_>>().join(", ")
    };
    let wrong: Vec<String> = measured
        .iter()
        .filter(|o| !o.faster.is_empty() && (!o.misread.is_empty() || !o.invented.is_empty()))
        .map(|o| {
            let mut why = Vec::new();
            if !o.misread.is_empty() {
                why.push(format!("reads {} wrong", shown(&o.misread)));
            }
            if !o.invented.is_empty() {
                why.push(format!("invents text on {}", shown(&o.invented)));
            }
            format!("{} ({})", o.name, why.join("; "))
        })
        .collect();
    if !wrong.is_empty() {
        lines.push(format!("{head} | faster than prod but not right: {}", wrong.join(", ")));
    }
    let prod = outcomes.iter().find(|o| o.name == "prod");
    let prod_wrong: Vec<&str> = prod
        .map(|p| p.misread.iter().chain(&p.invented).map(|(l, _)| l.as_str()).collect())
        .unwrap_or_default();
    if !prod_wrong.is_empty() {
        lines.push(format!(
            "{head} | prod itself does not read {} right; set aside, the cheapest that reads the rest right and \
             saves time: {}",
            prod_wrong.join(", "),
            named(&pick(true, Role::Candidate))
        ));
    }
    // By the same rule as the line before it, with prod's own misses set aside when it has any.
    let compared = pick(!prod_wrong.is_empty(), Role::Comparison);
    if !compared.is_empty() {
        let these: Vec<String> = compared.iter().map(|o| said(o)).collect();
        lines.push(format!(
            "{head} | for comparison only, never a way the application may read: {} would also pass the rule",
            these.join("; ")
        ));
    }
    lines
}

// ── The neural recogniser alone: `--paddle` ──────────────────────────────────────────────────

/// The words of a line, as column spans `(first, last)` of `ink` (one flag a pixel column: any
/// ink in it). Ink runs closer than `min_gap` empty columns are one word: the gaps inside a word
/// are a pixel or two, the space between words more.
pub fn word_spans(ink: &[bool], min_gap: usize) -> Vec<(usize, usize)> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut x = 0;
    while x < ink.len() {
        if !ink[x] {
            x += 1;
            continue;
        }
        let start = x;
        while x < ink.len() && ink[x] {
            x += 1;
        }
        match spans.last_mut() {
            Some(last) if start - last.1 - 1 < min_gap => last.1 = x - 1,
            _ => spans.push((start, x - 1)),
        }
    }
    spans
}

/// The `words` words of a line, by [`word_spans`] with the gap that makes exactly that many:
/// the narrowest of the `words − 1` widest gaps between ink runs, provided it is wider than every
/// other gap. `None` when the ink does not split that way — too few runs, or a space no wider
/// than a gap inside a word.
pub fn split_words(ink: &[bool], words: usize) -> Option<Vec<(usize, usize)>> {
    let runs = word_spans(ink, 1);
    if words == 0 || runs.len() < words {
        return None;
    }
    let mut gaps: Vec<usize> = runs.windows(2).map(|r| r[1].0 - r[0].1 - 1).collect();
    gaps.sort_unstable_by(|a, b| b.cmp(a));
    if words == 1 {
        return Some(word_spans(ink, gaps.first().map_or(1, |g| g + 1)));
    }
    let space = gaps[words - 2];
    if gaps.get(words - 1).is_some_and(|inside| *inside >= space) {
        return None;
    }
    Some(word_spans(ink, space)).filter(|s| s.len() == words)
}

/// What the width measurement suggests for `paddle_pre::MAX_ASPECT`, from cuts of a line: each
/// the ink's width over its height, and whether it was read right (loosely, as `same` would).
#[derive(Clone, Debug, PartialEq)]
pub enum AspectAdvice {
    /// No cut was read wrong; the widest measured.
    NoMiss { widest: f32, cuts: usize },
    /// Every cut up to `widest_right` was read right; the narrowest misread was `first_miss` wide.
    /// `suggest` is `widest_right` rounded down to a whole number.
    Below { suggest: f32, widest_right: f32, first_miss: f32, right_below: usize },
    /// Even the narrowest misread was narrower than every right one.
    NothingRightBelow { first_miss: f32 },
}

pub fn aspect_advice(cuts: &[(f32, bool)]) -> AspectAdvice {
    let first_miss = cuts.iter().filter(|c| !c.1).map(|c| c.0).fold(f32::INFINITY, f32::min);
    if first_miss.is_infinite() {
        let widest = cuts.iter().map(|c| c.0).fold(0.0, f32::max);
        return AspectAdvice::NoMiss { widest, cuts: cuts.len() };
    }
    let below: Vec<f32> = cuts.iter().filter(|c| c.1 && c.0 < first_miss).map(|c| c.0).collect();
    match below.iter().copied().reduce(f32::max) {
        Some(widest_right) => {
            AspectAdvice::Below { suggest: widest_right.floor(), widest_right, first_miss, right_below: below.len() }
        }
        None => AspectAdvice::NothingRightBelow { first_miss },
    }
}

impl AspectAdvice {
    pub fn line(&self) -> String {
        match self {
            AspectAdvice::NoMiss { widest, cuts } => format!(
                "width | all {cuts} cuts were read right, the widest {widest:.1} times as wide as tall: the limit lies \
                 beyond what this line can show; MAX_ASPECT up to {:.0} is measured",
                widest.floor()
            ),
            AspectAdvice::Below { suggest, widest_right, first_miss, right_below } => format!(
                "width | every cut up to {widest_right:.1} times as wide as tall was read right ({right_below} cuts), the \
                 narrowest misread was {first_miss:.1}: MAX_ASPECT suggested {suggest:.0}"
            ),
            AspectAdvice::NothingRightBelow { first_miss } => format!(
                "width | a cut only {first_miss:.1} times as wide as tall was misread, narrower than every right one: \
                 no width limit can be read from this line; see the cuts above"
            ),
        }
    }
}

/// What the scores say about a minimum: the scores of right readings (text only; a right
/// "nothing" has no score), of readings invented on pictures that hold no text, and of the
/// other wrong readings.
#[derive(Clone, Debug, PartialEq)]
pub enum ScoreAdvice {
    /// Nothing was invented: no minimum is needed.
    NoneNeeded { lowest_right: Option<f32> },
    /// Every invented reading scored below every right one: `at`, halfway, separates them, and
    /// would also drop `wrong_dropped` of `wrong` other wrong readings.
    Threshold { at: f32, highest_invented: f32, lowest_right: f32, wrong_dropped: usize, wrong: usize },
    /// An invented reading scored at least as high as a right one: no minimum separates them.
    Inseparable { highest_invented: f32, lowest_right: f32 },
}

pub fn score_advice(right: &[f32], invented: &[f32], wrong: &[f32]) -> ScoreAdvice {
    let finite = |v: &[f32]| v.iter().copied().filter(|s| s.is_finite()).collect::<Vec<f32>>();
    let (right, invented, wrong) = (finite(right), finite(invented), finite(wrong));
    let lowest_right = right.iter().copied().reduce(f32::min);
    let Some(highest_invented) = invented.iter().copied().reduce(f32::max) else {
        return ScoreAdvice::NoneNeeded { lowest_right };
    };
    match lowest_right {
        Some(lowest_right) if highest_invented < lowest_right => {
            let at = (highest_invented + lowest_right) / 2.0;
            ScoreAdvice::Threshold {
                at,
                highest_invented,
                lowest_right,
                wrong_dropped: wrong.iter().filter(|s| **s < at).count(),
                wrong: wrong.len(),
            }
        }
        // Nothing right to keep: any minimum above the inventions would do, and none can be
        // checked against a right reading.
        None => ScoreAdvice::Inseparable { highest_invented, lowest_right: f32::NAN },
        Some(lowest_right) => ScoreAdvice::Inseparable { highest_invented, lowest_right },
    }
}

impl ScoreAdvice {
    pub fn line(&self) -> String {
        match self {
            ScoreAdvice::NoneNeeded { lowest_right } => format!(
                "score | no reading was invented on a picture without text: no minimum score is needed{}",
                lowest_right.map(|s| format!(" (the lowest score of a right reading was {s:.3})")).unwrap_or_default()
            ),
            ScoreAdvice::Threshold { at, highest_invented, lowest_right, wrong_dropped, wrong } => format!(
                "score | invented readings scored at most {highest_invented:.3}, right ones at least {lowest_right:.3}: a \
                 minimum score of {at:.3} separates them, and would also drop {wrong_dropped} of {wrong} other wrong readings"
            ),
            ScoreAdvice::Inseparable { highest_invented, lowest_right } => format!(
                "score | invented readings scored up to {highest_invented:.3}, right ones as low as {lowest_right:.3}: no \
                 minimum score separates them"
            ),
        }
    }
}

/// One crop setting of the neural recogniser over the 2x pictures.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingResult {
    pub name: String,
    /// Pictures with text read right exactly, and loosely, of `of`.
    pub right: usize,
    pub loose: usize,
    pub of: usize,
    /// Pictures without text read as something, of `nothing_of`.
    pub invented: usize,
    pub nothing_of: usize,
}

/// The setting that reads the most pictures right (loosely), then invents the least, then comes
/// first in `results` — which the caller orders by preference.
pub fn best_setting(results: &[SettingResult]) -> Option<usize> {
    (0..results.len()).min_by_key(|&i| (std::cmp::Reverse(results[i].loose), results[i].invented, i))
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

/// What a neural recogniser's first-pass child measured, in milliseconds and megabytes.
#[derive(Clone, Debug, PartialEq)]
pub struct PaddleChild {
    /// What came before the first recognition: the session made (`none`), the warm-up (`warm`),
    /// or the warm-up beside Vision's (`with-vision`).
    pub start: f64,
    /// Vision's word warm-up beside it (`with-vision`).
    pub vision: Option<f64>,
    /// The first and the second recognition of `lone-1@2x`, the content crop handed to the
    /// recogniser's thread and waited for, as a read would.
    pub first: f64,
    pub second: f64,
    /// Vision's first real pass after both warm-ups (`with-vision`).
    pub vision_first: Option<f64>,
    /// The process's peak resident memory before the recogniser was made and after its second
    /// recognition.
    pub mem_before: Option<f64>,
    pub mem_after: Option<f64>,
    /// Whether the first recognition read "1".
    pub right: bool,
}

/// A neural recogniser's first-pass child: what it measured, or why it could not.
#[derive(Clone, Debug, PartialEq)]
pub enum PaddleFirst {
    Ran(PaddleChild),
    No(String),
}

pub fn paddle_child_line(r: &PaddleFirst) -> String {
    let opt = |v: Option<f64>| v.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".to_string());
    match r {
        PaddleFirst::Ran(c) => format!(
            "{CHILD_TAG} paddle start={:.1} vision={} first={:.1} second={:.1} vfirst={} mem0={} mem1={} right={}",
            c.start,
            opt(c.vision),
            c.first,
            c.second,
            opt(c.vision_first),
            opt(c.mem_before),
            opt(c.mem_after),
            u8::from(c.right)
        ),
        PaddleFirst::No(why) => format!("{CHILD_TAG} paddle no {}", why.replace('\n', " ")),
    }
}

pub fn parse_paddle_child(output: &str) -> Option<PaddleFirst> {
    let rest = tagged(output)?.strip_prefix("paddle ")?;
    if let Some(why) = rest.strip_prefix("no ") {
        return Some(PaddleFirst::No(why.trim().to_string()));
    }
    let field = |key: &str| rest.split_whitespace().find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')));
    let num = |key: &str| field(key).and_then(|v| v.parse::<f64>().ok());
    let opt = |key: &str| -> Option<Option<f64>> {
        match field(key)? {
            "-" => Some(None),
            v => v.parse::<f64>().ok().map(Some),
        }
    };
    Some(PaddleFirst::Ran(PaddleChild {
        start: num("start")?,
        vision: opt("vision")?,
        first: num("first")?,
        second: num("second")?,
        vision_first: opt("vfirst")?,
        mem_before: opt("mem0")?,
        mem_after: opt("mem1")?,
        right: field("right")? == "1",
    }))
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

/// Whether a child's output holds its answer — whether it got as far as reading: a first pass's
/// line, the neural recogniser's measurement (not its "no"), a probe that ran.
pub fn child_read(role: Child, output: &str) -> bool {
    match role {
        Child::FirstPass(_) | Child::FastFirst | Child::SecondLanguage => parse_child(output).is_some(),
        Child::PaddleFirst(_) => matches!(parse_paddle_child(output), Some(PaddleFirst::Ran(_))),
        Child::Probe(_) => matches!(parse_probe(output), Some(Probe::Ran { .. })),
    }
}

/// How a child of the benchmark ended, from its signal or status and its standard output: the
/// output when it ended with status 0, or what became of it. `read` ([`child_read`]) says that it
/// had printed its answer, so that a child that read and then died — at its exit, which is where
/// ONNX Runtime 1.22's teardown aborted every process that had loaded it — is said apart from one
/// that died, or could not load, before: `died of signal 6 at exit after reading: …`. Its answer is
/// not used all the same: a process that dies is a process that dies.
pub fn child_ended(signal: Option<i32>, code: Option<i32>, output: &str, read: bool) -> Result<String, String> {
    if signal.is_none() && code == Some(0) {
        return Ok(output.to_string());
    }
    let printed = output.trim();
    let after = if read {
        format!(" at exit after reading: {printed}")
    } else if printed.is_empty() {
        String::new()
    } else {
        format!(" after printing: {printed}")
    };
    Err(match (signal, code) {
        (Some(sig), _) => format!("died of signal {sig}{after}"),
        (None, Some(code)) => format!("ended with status {code}{after}"),
        (None, None) => format!("ended without a status{after}"),
    })
}

/// A child the parent ended at its limit of `limit_s` seconds, from what it had printed by then:
/// one that had printed its answer hung at its exit — the other way a teardown can go wrong —
/// and is said as one at exit after reading, as [`child_ended`] says a death there; one that had
/// not did not finish.
pub fn child_timed_out(limit_s: u64, output: &str, read: bool) -> String {
    if read {
        format!("hung at exit after reading, and was ended after {limit_s} s: {}", output.trim())
    } else {
        format!("did not finish within {limit_s} s")
    }
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

/// What the subcommand does where there is no Vision: `--paddle` on Windows, the usage, or
/// that it runs on macOS.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn elsewhere(args: &[String]) -> i32 {
    match parse(args) {
        Ok(o) if o.help => {
            println!("{USAGE}");
            0
        }
        #[cfg(windows)]
        Ok(o) if o.paddle || o.paddle_probe => crate::backend::ocr_bench_paddle(&o),
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

    /// `crates/host/bench-data/ocr/real`, from either crate that compiles this file: both
    /// manifests sit beside `host`.
    fn real_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../host/bench-data/ocr/real")
    }

    #[test]
    fn every_picture_is_the_size_its_entry_says() {
        assert_eq!(FIXTURES.len(), 28);
        // The real ones too, through the manifest that `--pictures` reads (the loader checks
        // each PNG's size against its entry; this checks the loader ran over every file).
        let real = load_pictures(&real_dir()).expect("the committed manifest");
        let files = std::fs::read_dir(real_dir()).unwrap().filter(|e| {
            e.as_ref().is_ok_and(|e| e.path().extension().is_some_and(|x| x == "png"))
        });
        assert_eq!(files.count(), real.len(), "every PNG in the folder has an entry");
        assert!(real.iter().all(|f| f.origin != Origin::Drawn));
        for f in FIXTURES.iter().chain(real.iter().copied()) {
            let (w, h) = f.px();
            assert_eq!(png_size(f.png), Some((w as u32, h as u32)), "{}", f.label());
            assert!(!f.accept.is_empty(), "{}", f.label());
            // Nothing is an answer only where reading is not required.
            assert!(!(f.must_read && f.accept.iter().any(|a| a.is_empty())), "{}", f.label());
            assert_eq!(f.holds_nothing(), f.accept == [""], "{}", f.label());
        }
        // The drawn pictures: text must be read, only the none- ones hold none, and the clipped
        // label's half a word may be read or not.
        for f in FIXTURES {
            assert_eq!(f.origin, Origin::Drawn);
            assert_eq!(f.holds_nothing(), f.name.starts_with("none-"), "{}", f.label());
            let optional = f.name == "clipped-label";
            assert_eq!(f.must_read, !f.holds_nothing() && !optional, "{}", f.label());
            if optional {
                assert_eq!(f.accept, ["Velocity", ""], "{}", f.label());
            }
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

    /// Every drawn picture has its row in the neural recogniser's golden test, which pins what
    /// Windows reads (paddle_pre.rs): a new drawing is not left out of it by accident.
    #[test]
    fn every_drawn_picture_has_a_golden_row() {
        let golden = include_str!("paddle_pre.rs");
        for f in FIXTURES {
            assert!(golden.contains(&format!("drawn!(\"{}\"", f.label())), "{} has no golden row", f.label());
        }
    }

    /// A manifest is refused, with the entry and the reason, for each way it can be wrong.
    #[test]
    fn a_wrong_manifest_is_refused_and_says_why() {
        let dir = std::env::temp_dir().join(format!("ocr-bench-manifest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A real 30x30 picture to point at.
        let png = std::fs::read(real_dir().join("sfz-polyphony@1x.png")).unwrap();
        std::fs::write(dir.join("p@1x.png"), &png).unwrap();
        let entry = |extra: &str| {
            format!(
                "[[picture]]\nname = \"p\"\nscale = 1\nw_pt = 30\nh_pt = 30\naccept = [\"64\"]\nmust_read = true\n\
                 platform = \"windows\"\n{extra}"
            )
        };
        let load = |text: &str| {
            std::fs::write(dir.join("manifest.toml"), text).unwrap();
            load_pictures(&dir)
        };
        let ok = load(&entry("source = \"a shot\"\nchecked = \"by eye\"\n")).unwrap();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].label(), "p@1x");
        assert_eq!(ok[0].accept, ["64"]);
        assert_eq!(ok[0].origin, Origin::Captured { platform: "windows" });
        let refused = |text: &str, says: &str| {
            let e = load(text).unwrap_err();
            assert!(e.contains(says), "{e}");
        };
        refused(&entry("colour = 3\n"), "unknown key 'colour'");
        refused(&entry("").replace("w_pt = 30", "w_pt = 31"), "is 30x30 pixels, the entry says 31x30");
        refused(&entry("").replace("accept = [\"64\"]", "accept = [\"64\", \"\"]"), "cannot accept nothing");
        refused(&entry("").replace("accept = [\"64\"]", "accept = []"), "'accept'");
        refused(&entry("").replace("\"windows\"", "\"linux\""), "platform 'linux'");
        refused(&entry("").replace("name = \"p\"", "name = \"../p\""), "is not letters");
        refused(&entry("").replace("name = \"p\"", "name = \"q\""), "cannot read");
        refused(&entry("").replace("scale = 1", "scale = 0"), "'scale'");
        refused(&format!("{}{}", entry(""), entry("")), "taken already");
        refused("title = 1\n", "no [[picture]] entries");
        refused("[[picture]\n", "manifest.toml");
        // A drawn picture's label cannot be taken.
        std::fs::write(dir.join("field-64@1x.png"), FIXTURES[0].png).unwrap();
        refused(
            &entry("").replace("name = \"p\"", "name = \"field-64\"").replace("w_pt = 30\nh_pt = 30", "w_pt = 40\nh_pt = 20"),
            "the label field-64@1x is taken",
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_pictures(&dir).unwrap_err().contains("cannot read"));
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
        assert_eq!(probed, ["rev2", "rev3", "cpu", "gpu", "ane", "paddle-raw", "paddle-crop"]);
    }

    /// prod is the application's reading and comes first, and old the ladder it read with before,
    /// beside it; every other strategy differs from old; the names are unique; and what each needs
    /// — a revision, the neural recogniser — is what its shape says.
    #[test]
    fn the_strategies() {
        assert_eq!(STRATEGIES[0].name, "prod");
        let prod = &STRATEGIES[0];
        assert!(prod.revision().is_none() && !prod.paddle(), "prod is measured wherever the bench runs");
        // The application's own shape, on each kind of Mac, and old's where the recogniser is not.
        assert_eq!(prod.shape_here(false, true), Shape::TODAY);
        assert_eq!(prod.shape_here(true, false), Shape::production(true, false));
        assert_eq!(prod.shape_here(true, true), Shape::production(true, true));
        let mut names: Vec<&str> = STRATEGIES.iter().map(|s| s.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), STRATEGIES.len());
        let by = |n: &str| &STRATEGIES[strategy(n).unwrap()];
        let old = by("old");
        assert!(old.shape.is_today() && old.revision_all.is_none() && old.role == Role::Comparison);
        assert_eq!(old.shape_here(true, true), Shape::TODAY, "old is old whatever the Mac");
        for s in STRATEGIES.iter().filter(|s| !matches!(s.name, "prod" | "old")) {
            assert!(!s.shape.is_today() || s.revision_all.is_some(), "{} is old again", s.name);
            assert_eq!(s.shape_here(true, true), s.shape, "{}", s.name);
        }
        assert_eq!(by("rev2").revision(), Some(2));
        assert_eq!(by("rev2-tight").revision(), Some(2));
        assert_eq!(by("rev3>rev2").revision(), Some(2), "its second pass asks for revision 2");
        assert!(!by("rev2").as_prod_when_large() && by("fast=paddle").as_prod_when_large());
        let with_paddle: Vec<&str> = STRATEGIES.iter().filter(|s| s.paddle()).map(|s| s.name).collect();
        assert_eq!(
            with_paddle,
            ["paddle", "acc+paddle", "acc+paddle+rest", "fast=paddle-checked", "fast=paddle-strict", "fast=paddle"]
        );
        // The closing line names candidates only: the lone digit's three and the fast level's three.
        let candidates: Vec<&str> = STRATEGIES.iter().filter(|s| s.role == Role::Candidate).map(|s| s.name).collect();
        assert_eq!(
            candidates,
            ["acc+paddle+rest", "rev2-tight", "rev3>rev2", "fast=paddle-checked", "fast=paddle-strict", "fast=paddle"]
        );
        assert_eq!(role("prod"), Role::Production);
        assert_eq!(role("old"), Role::Comparison, "how it read before is never a way to read now");
        assert_eq!(role("fast>acc"), Role::Comparison, "unchecked fast text is never a way to read");
        assert_eq!(role("paddle"), Role::Comparison);
        assert_eq!(role("nobody"), Role::Comparison);
        assert_eq!(by("fast=paddle-checked").shape.paddle, PaddleUse::AgreeChecked);
        // acc+paddle asks at low priority, the fast level's checks at high.
        assert!(!by("acc+paddle+rest").shape.paddle.urgent() && by("fast=paddle").shape.paddle.urgent());
        assert!(!by("acc+paddle").shape.rest && by("acc+paddle+rest").shape.rest);
        assert_eq!(by("fast>acc").shape.first, Level::Fast);
        assert!(!by("fast>acc").paddle());
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
        assert!(parse(&a(&["--paddle"])).unwrap().paddle);
        assert!(parse(&a(&["--paddle-probe"])).unwrap().paddle_probe);
        let dir = real_dir().to_string_lossy().into_owned();
        let with = parse(&a(&["--pictures", &dir])).unwrap();
        assert!(!with.pictures.is_empty());
        assert_eq!(all_pictures(&with).len(), FIXTURES.len() + with.pictures.len());
        assert!(parse(&a(&["--pictures", &dir, "--pictures", &dir])).unwrap_err().contains("once"));
        assert!(parse(&a(&["--pictures", "no such folder"])).unwrap_err().contains("cannot read"));
        assert!(parse(&a(&["--pictures"])).is_err());
        assert!(parse(&a(&["-h"])).unwrap().help);
        // What the parent passes is what the child reads.
        for c in first_roles(0).into_iter().chain([Child::Probe(variant("rev2").unwrap()), Child::Probe(variant("paddle-crop").unwrap())]) {
            assert_eq!(Child::parse(&c.arg()), Some(c), "{}", c.arg());
        }
        assert!(parse(&a(&["--child", "paddle-first:cold"])).is_err());
    }

    #[test]
    fn the_plans() {
        let full = Plan::for_mode(false, false);
        assert_eq!(full.idle_secs(), 168);
        let quick = Plan::for_mode(true, false);
        assert_eq!(quick.idle_secs(), 48);
        assert_eq!(Plan::for_mode(false, true).idle_secs(), 168 + 2160);
        for plan in [&full, &quick, &Plan::for_mode(false, true)] {
            // Every accurate pause once on the thread that read before and once on a fresh one,
            // and the fast level and the neural recogniser after the same lengths.
            let of = |e: IdleEngine, fresh: bool| {
                let mut v: Vec<u64> = plan.idle.iter().filter(|i| i.engine == e && i.fresh == fresh).map(|i| i.secs).collect();
                v.sort();
                v
            };
            assert_eq!(of(IdleEngine::Accurate, false), of(IdleEngine::Accurate, true));
            assert!(of(IdleEngine::Fast, true).is_empty() && of(IdleEngine::Paddle, true).is_empty());
            let short: Vec<u64> = of(IdleEngine::Accurate, true).into_iter().filter(|s| *s <= 30).collect();
            assert_eq!(of(IdleEngine::Fast, false), short);
            assert_eq!(of(IdleEngine::Paddle, false), short);
            assert!(plan.min_samples <= plan.samples);
            // The pictures read for speed are drawn ones or the CI set's real ones.
            let real = load_pictures(&real_dir()).unwrap();
            for p in plan.speed_pictures {
                assert!(fixture(p).is_some() || real.iter().any(|f| f.label() == *p), "{p}");
            }
            assert!(plan.speed_pictures.contains(&PROBE) && plan.speed_pictures.contains(&"lone-1@2x"));
        }
        assert!(full.every_picture && !quick.every_picture);
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
        assert_eq!(c.accuracy(&["Instrument Polyphony Pitchbend"]), Accuracy::Right);
        let mut d = Cell::default();
        d.push(s(1.0, "DEF"));
        d.push(s(1.0, "def"));
        d.push(s(1.0, "def"));
        let wrong = Accuracy::Wrong { right: 1, loose: 1, of: 3, got: "def".into(), invented: false };
        assert_eq!(d.accuracy(&["DEF"]), wrong);
        assert!(accuracy_words(&wrong, "", None).starts_with("WRONG: read \"def\" (1 of 3 right)"));
        let mut r = Cell::default();
        r.push(Sample { ms: 1.0, text: None, passes: 1, skipped: false });
        assert_eq!(r.accuracy(&["64"]), Accuracy::Refused);
        assert_eq!(Cell::default().accuracy(&["64"]), Accuracy::Nothing);
    }

    /// Any answer of the set is right; spaces and dash forms make a reading right loosely, not
    /// exactly; on a picture without text, text is invented, and nothing is right.
    #[test]
    fn accuracy_judges_against_the_set() {
        let read = |v: &[&str]| v.iter().map(|t| normalise(t)).collect::<Vec<String>>();
        assert_eq!(judge(&read(&["-", ""]), 2, &["-", ""]), Accuracy::Right);
        assert_eq!(judge(&read(&["0.00 dB"]), 1, &["0.00 dB"]), Accuracy::Right);
        let loose = judge(&read(&["0.00dB", "0.00 dB", "\u{2212}0.5 dB", "0.00dB"]), 4, &["0.00 dB", "-0.5 dB"]);
        assert_eq!(loose, Accuracy::Loosely { right: 1, of: 4, got: "0.00dB".into() });
        assert!(loose.loosely_right() && !loose.right());
        assert!(accuracy_words(&loose, "", None).contains("right but for spaces or dash forms (1 of 4 exactly right)"));
        // Nothing, on a picture that holds nothing: right, and said so.
        assert_eq!(judge(&read(&["", " "]), 2, &[""]), Accuracy::Right);
        assert_eq!(accuracy_words(&Accuracy::Right, "", None), "read nothing, which is right");
        // Text there is invented.
        let invented = judge(&read(&["", "1111", "1111"]), 3, &[""]);
        assert_eq!(invented, Accuracy::Wrong { right: 1, loose: 1, of: 3, got: "1111".into(), invented: true });
        assert!(accuracy_words(&invented, "", None).starts_with("WRONG, INVENTED: read \"1111\""));
        assert_eq!(accuracy_mark(&invented), ", wrong, invented");
        // Nothing where text must be read is wrong, and not invented; a lost minus is wrong.
        let missed = judge(&read(&["", "-12"]), 2, &["-12"]);
        assert_eq!(missed, Accuracy::Wrong { right: 1, loose: 1, of: 2, got: String::new(), invented: false });
        assert!(matches!(judge(&read(&["12"]), 1, &["-12"]), Accuracy::Wrong { invented: false, .. }));
        // A refused request counts against the reads, not as a reading.
        assert!(matches!(judge(&read(&["64"]), 2, &["64"]), Accuracy::Loosely { .. } | Accuracy::Wrong { .. }));
        assert!(accepted("Voices:  0", &["Voices: 0"]));
        assert!(!accepted("Voices:0", &["Voices: 0"]) && accepted_loosely("Voices:0", &["Voices: 0"]));
        assert!(!accepted_loosely("", &["-"]));
    }

    #[test]
    fn the_words_of_a_line_are_its_ink_runs_apart_by_a_gap() {
        let cols = |s: &str| s.chars().map(|c| c == '#').collect::<Vec<bool>>();
        // Two words of two letters each, one-column gaps inside them, three between.
        assert_eq!(word_spans(&cols("..##.#...###.##.."), 2), vec![(2, 5), (9, 14)]);
        assert_eq!(word_spans(&cols("..##.#...###.##.."), 1), vec![(2, 3), (5, 5), (9, 11), (13, 14)]);
        assert_eq!(word_spans(&cols("#"), 3), vec![(0, 0)]);
        assert!(word_spans(&cols("...."), 3).is_empty());
        // The gap is found from the number of words.
        let line = cols("##.#...###.##....#.#");
        assert_eq!(split_words(&line, 3), Some(vec![(0, 3), (7, 12), (17, 19)]));
        assert_eq!(split_words(&line, 1), Some(vec![(0, 19)]));
        assert_eq!(split_words(&line, 7), None, "more words than ink runs");
        // A space no wider than a gap inside a word cannot be told from it.
        assert_eq!(split_words(&cols("#..#..#"), 2), None);
        assert_eq!(split_words(&line, 0), None);
    }

    #[test]
    fn the_width_advice() {
        let cuts = [(4.0, true), (9.6, true), (12.5, false), (11.0, true), (20.0, false), (13.0, true)];
        assert_eq!(
            aspect_advice(&cuts),
            AspectAdvice::Below { suggest: 11.0, widest_right: 11.0, first_miss: 12.5, right_below: 3 }
        );
        assert!(aspect_advice(&cuts).line().contains("MAX_ASPECT suggested 11"));
        assert_eq!(aspect_advice(&[(3.0, true), (8.5, true)]), AspectAdvice::NoMiss { widest: 8.5, cuts: 2 });
        assert_eq!(aspect_advice(&[(5.0, true), (2.0, false)]), AspectAdvice::NothingRightBelow { first_miss: 2.0 });
    }

    #[test]
    fn the_score_advice() {
        assert_eq!(score_advice(&[0.9, 0.95, f32::NAN], &[], &[0.2]), ScoreAdvice::NoneNeeded { lowest_right: Some(0.9) });
        assert!(score_advice(&[], &[], &[]).line().contains("no minimum score is needed"));
        let t = score_advice(&[0.9, 0.95], &[0.3, 0.5], &[0.4, 0.8]);
        assert_eq!(t, ScoreAdvice::Threshold { at: 0.7, highest_invented: 0.5, lowest_right: 0.9, wrong_dropped: 1, wrong: 2 });
        assert!(t.line().contains("a minimum score of 0.700 separates them"));
        assert_eq!(score_advice(&[0.6], &[0.7], &[]), ScoreAdvice::Inseparable { highest_invented: 0.7, lowest_right: 0.6 });
        assert!(matches!(score_advice(&[], &[0.7], &[]), ScoreAdvice::Inseparable { .. }));
    }

    #[test]
    fn the_best_setting_reads_most_then_invents_least_then_comes_first() {
        let r = |name: &str, loose, invented| SettingResult { name: name.into(), right: loose, loose, of: 10, invented, nothing_of: 5 };
        assert_eq!(best_setting(&[r("a", 8, 0), r("b", 9, 1), r("c", 9, 0), r("d", 9, 0)]), Some(2));
        assert_eq!(best_setting(&[r("a", 8, 0), r("b", 8, 0)]), Some(0));
        assert_eq!(best_setting(&[]), None);
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
    fn the_first_pass_kinds_take_turns() {
        let all = first_roles(0);
        assert_eq!(all.len(), 9);
        assert_eq!(all[0], Child::FirstPass(Warmup::Bars));
        let firsts: Vec<Child> = (0..all.len()).map(|r| first_roles(r)[0]).collect();
        assert_eq!(firsts, all, "each kind is first once in as many rounds");
        let mut sorted = first_roles(4).iter().map(|c| c.arg()).collect::<Vec<_>>();
        sorted.sort();
        let mut want = all.iter().map(|c| c.arg()).collect::<Vec<_>>();
        want.sort();
        assert_eq!(sorted, want, "every round has every kind");
    }

    #[test]
    fn a_neural_childs_line_survives_the_round_trip() {
        let full = PaddleChild {
            start: 812.25,
            vision: Some(1650.0),
            first: 21.5,
            second: 9.0,
            vision_first: Some(431.0),
            mem_before: Some(41.0),
            mem_after: Some(96.5),
            right: true,
        };
        let plain = PaddleChild { vision: None, vision_first: None, mem_before: None, right: false, ..full.clone() };
        for r in [PaddleFirst::Ran(full), PaddleFirst::Ran(plain), PaddleFirst::No("ONNX Runtime did not load\nfrom x".into())] {
            let back = parse_paddle_child(&format!("noise\n{}\n", paddle_child_line(&r))).unwrap();
            match (&r, &back) {
                (PaddleFirst::Ran(a), PaddleFirst::Ran(b)) => {
                    assert!((a.start - b.start).abs() < 0.1 && (a.first - b.first).abs() < 0.1);
                    assert_eq!((a.vision.is_some(), a.vision_first.is_some(), a.mem_before.is_some(), a.right), (b.vision.is_some(), b.vision_first.is_some(), b.mem_before.is_some(), b.right));
                }
                (PaddleFirst::No(_), PaddleFirst::No(why)) => assert_eq!(why, "ONNX Runtime did not load from x"),
                _ => panic!("{r:?} came back as {back:?}"),
            }
        }
        assert_eq!(parse_paddle_child(&child_line(&ChildResult { warmup: None, first: 1.0, second: 2.0 })), None);
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

    /// A child that read and then died at its exit — CI's first run with ONNX Runtime on a Mac,
    /// every process that had loaded it — is said apart from one that died before it read, and
    /// from one that could not load the recogniser at all, which ended normally saying so.
    #[test]
    fn a_child_that_read_and_died_at_exit_is_told_apart() {
        let probe = Child::Probe(variant("paddle-crop").unwrap());
        let ran = probe_line(&Probe::Ran { pinned: vec![] });
        assert!(child_read(probe, &ran));
        assert_eq!(
            child_ended(Some(6), None, &format!("{ran}\n"), child_read(probe, &ran)),
            Err(format!("died of signal 6 at exit after reading: {ran}"))
        );
        // Could not load: the child says so, and ends normally; its answer is for the caller.
        let no = probe_line(&Probe::No("the neural recogniser is not available: no x86_64 slice".into()));
        assert!(!child_read(probe, &no));
        assert_eq!(child_ended(None, Some(0), &no, false), Ok(no.clone()));
        // Died before its answer: what it printed, if anything, without "at exit".
        assert_eq!(child_ended(Some(11), None, "", false), Err("died of signal 11".to_string()));
        assert_eq!(child_ended(Some(6), None, " half a line ", false), Err("died of signal 6 after printing: half a line".to_string()));
        // A status other than 0 after its answer is an end that went wrong too.
        assert_eq!(child_ended(None, Some(1), &ran, true), Err(format!("ended with status 1 at exit after reading: {ran}")));
        assert_eq!(child_ended(None, None, "", false), Err("ended without a status".to_string()));
        // Ended at the parent's limit: a hang at exit after its answer, or no answer at all.
        assert_eq!(
            child_timed_out(120, &format!("{ran}\n"), child_read(probe, &ran)),
            format!("hung at exit after reading, and was ended after 120 s: {ran}")
        );
        assert_eq!(child_timed_out(120, "", false), "did not finish within 120 s");

        // What counts as reading, by role: the measurement, not its "no".
        let paddle = Child::PaddleFirst(PaddleStart::Nothing);
        let measured = paddle_child_line(&PaddleFirst::Ran(PaddleChild {
            start: 134.4,
            vision: None,
            first: 8.7,
            second: 5.1,
            vision_first: None,
            mem_before: Some(15.1),
            mem_after: Some(60.9),
            right: true,
        }));
        assert!(child_read(paddle, &measured));
        assert!(!child_read(paddle, &paddle_child_line(&PaddleFirst::No("not available".into()))));
        let first = Child::FirstPass(Warmup::Word);
        assert!(child_read(first, &child_line(&ChildResult { warmup: None, first: 200.0, second: 170.0 })));
        assert!(!child_read(first, "IOServiceMatchingfailed for: AppleM2ScalerParavirtDriver"));
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
        assert!(!g.control_loud());
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
        assert!(g.control_loud());
        let mut few = grid(&["prod", "prod-b"]);
        for i in 0..4 {
            few.cells[0][0].push(s(200.0 + i as f64, "64"));
            few.cells[1][0].push(s(300.0 + i as f64, "64"));
        }
        assert!(few.control_line().unwrap().contains("too few passes"));
        assert!(grid(&["prod", "fast"]).control_line().is_none());
    }

    #[test]
    fn beside_against_alone() {
        let alone = [300.0, 301.0, 302.0, 303.0, 304.0, 305.0];
        let beside = [400.0, 401.0, 402.0, 403.0, 404.0, 405.0];
        assert_eq!(
            against_alone(&alone, &beside),
            "median 402 ms against 302 ms alone: clearly slower (+33 %, p 0.002)"
        );
        assert!(against_alone(&alone, &alone).contains("no clear difference"));
        assert_eq!(against_alone(&[], &beside), "not both measured");
    }

    #[test]
    fn the_switching_line() {
        assert!(switching_line(Some(220.0), Some(200.0)).contains("costs little"));
        assert!(switching_line(Some(240.0), Some(200.0)).contains("switching between variants costs time here"));
        assert!(switching_line(Some(160.0), Some(200.0)).contains("drifted"));
        assert!(switching_line(None, Some(200.0)).contains("not both measured"));
    }

    fn note(answer: Answer, vision_ms: f64, paddle_runs: u64, agreed: Option<bool>) -> ReadNote {
        ReadNote { passes: 1, vision_ms, paddle_runs, answer, agreed, today: false }
    }

    #[test]
    fn the_pipeline_line_names_the_passes_the_runs_and_who_answered() {
        let mut row = PipelineRow::new("fast=paddle", fixture("lone-1@2x").unwrap(), true);
        row.push(s(80.0, "1"), note(Answer::Paddle, 12.0, 1, Some(false)));
        for i in 0..3 {
            let mut sample = s(40.0 + i as f64, "1");
            sample.passes = 1;
            row.push(sample, note(Answer::Pass(Stage::FastFirst), 10.0, 1, Some(true)));
        }
        let line = row.line();
        assert!(line.starts_with("pipeline | fast=paddle | lone-1@2x | n 3: median 41 ms"), "{line}");
        assert!(line.contains("1.0 Vision passes and 1.0 neural runs a read"), "{line}");
        assert!(line.contains("answered by fast first 3, Paddle 1"), "{line}");
        assert!(line.contains("fast and the neural recogniser read the same 3 of 4 times"), "{line}");
        assert!(line.contains("not Vision: median 31 ms"), "{line}");
        assert!(line.contains("read \"1\" right"), "{line}");
        // A row read for whether it reads right takes two reads.
        let plan = Plan::for_mode(false, false);
        let mut once = PipelineRow::new("prod", fixture("val-ct@1x").unwrap(), false);
        assert!(once.wants_more(&plan));
        once.push(s(400.0, "+3 ct"), note(Answer::Pass(Stage::Tight), 300.0, 0, None));
        assert!(once.wants_more(&plan));
        once.push(s(400.0, "+3 ct"), note(Answer::Pass(Stage::Tight), 300.0, 0, None));
        assert!(!once.wants_more(&plan));
        assert!(!once.line().contains("neural runs") && !once.line().contains("read the same"), "{}", once.line());
    }

    /// A pipeline over fabricated reads: which rows are measured, the table, and what each
    /// strategy comes to.
    #[test]
    fn the_pipeline_grid_and_its_outcomes() {
        let mut plan = Plan::for_mode(false, false);
        plan.speed_pictures = &["lone-1@2x", "field-64@2x"];
        let names = ["prod", "rev2", "fast>acc", "acc+paddle+rest"];
        let strategies: Vec<&Strategy> = names.iter().map(|n| &STRATEGIES[strategy(n).unwrap()]).collect();
        let pictures: Vec<&Fixture> = ["lone-1@2x", "field-64@2x", "none-bars@2x", "line@1x"].iter().map(|l| fixture(l).unwrap()).collect();
        let mut g = Pipeline::new(strategies, pictures, &plan);
        assert_eq!(g.count(), 16);
        assert_eq!(g.at(5), (1, 1));
        // A large picture: prod and rev2 read it, the others leave it to prod.
        assert!(g.rows[0][3].cell.skipped.is_none() && g.rows[1][3].cell.skipped.is_none());
        assert!(g.rows[2][3].cell.skipped.as_deref().unwrap().contains("as prod reads it"));
        assert!(g.rows[0][0].speed && !g.rows[0][2].speed && !g.rows[0][3].speed);
        g.skip(3, "the neural recogniser is not available here");
        let read = |g: &mut Pipeline, s: usize, p: usize, base: f64, text: &str| {
            for i in 0..7 {
                g.rows[s][p].push(Sample { ms: base + 2.0 * i as f64, text: Some(text.into()), passes: 1, skipped: false }, note(Answer::Pass(Stage::Tight), base, 0, None));
            }
        };
        // prod: lone-1 in two passes, slow; field-64 right; nothing on the bars; the line right.
        read(&mut g, 0, 0, 900.0, "1");
        read(&mut g, 0, 1, 450.0, "64");
        read(&mut g, 0, 2, 900.0, "");
        read(&mut g, 0, 3, 1400.0, "Instrument Polyphony Pitchbend Range Velocity Curve Release Time Volume");
        // rev2: right everywhere, faster on lone-1, the same on field-64.
        read(&mut g, 1, 0, 450.0, "1");
        read(&mut g, 1, 1, 451.0, "64");
        read(&mut g, 1, 2, 900.0, "");
        read(&mut g, 1, 3, 1400.0, "Instrument Polyphony Pitchbend Range Velocity Curve Release Time Volume");
        // fast>acc: far faster, and invents on the bars.
        read(&mut g, 2, 0, 470.0, "1");
        read(&mut g, 2, 1, 60.0, "64");
        read(&mut g, 2, 2, 60.0, "ll");
        assert!(matches!(g.verdict(1, 0), Some(Verdict::Faster(..))));
        assert!(matches!(g.verdict(1, 1), Some(Verdict::Same(_))));
        assert_eq!(g.verdict(1, 2), None, "the bars were read for whether they read right, not for speed");
        assert_eq!(g.verdict(0, 0), None, "prod is not judged against itself");
        let table = g.table();
        assert!(table.contains("| lone-1@2x | 907 | 457 faster | 477 faster | - |"), "{table}");
        assert!(table.contains("| none-bars@2x (no text) | 907 | 907 | 67, wrong, invented | - |"), "{table}");
        let outcomes = g.outcomes();
        assert_eq!(outcomes[1].faster, ["lone-1@2x"]);
        assert!(outcomes[1].misread.is_empty() && outcomes[1].invented.is_empty());
        assert_eq!(outcomes[1].judged, 2);
        assert_eq!(outcomes[2].invented, [("none-bars@2x".to_string(), false)]);
        assert!(outcomes[2].misread.is_empty(), "the line, left to prod, is prod's and right");
        assert!(outcomes[3].skipped.as_deref().unwrap().contains("not available"));
        let lines = closing_lines(&outcomes, false);
        // rev2 reads right and saves time, but changes every rung: said for comparison, not named.
        assert!(lines[0].ends_with("reads right and saves time: none does"), "{lines:?}");
        assert!(lines[1].contains("faster than prod but not right: fast>acc (invents text on none-bars@2x)"), "{lines:?}");
        assert!(lines[2].contains("for comparison only, never a way the application may read: rev2 (clearly faster on 1 of 2 pictures judged"), "{lines:?}");
        assert_eq!(lines.len(), 3);
        let summary = g.summary_table();
        assert!(summary.contains("| acc+paddle+rest | candidate | not measured: the neural recogniser is not available here |"), "{summary}");
        assert!(summary.contains("| fast>acc | comparison | none | none-bars@2x | none |"), "{summary}");
        assert_eq!(g.prod_line().unwrap(), "pipeline | the application's reading (prod) reads every picture it read right");
        assert_eq!(g.old_line(), None, "old was not measured here");
    }

    /// prod's line and old's beside it: what the change made of every picture, the one CI warns
    /// by and the one it does not.
    #[test]
    fn the_applications_reading_and_the_ladder_before_it() {
        let plan = Plan::for_mode(false, false);
        let names = ["prod", "old"];
        let strategies: Vec<&Strategy> = names.iter().map(|n| &STRATEGIES[strategy(n).unwrap()]).collect();
        let pictures: Vec<&Fixture> = ["lone-1@2x", "none-bars@2x"].iter().map(|l| fixture(l).unwrap()).collect();
        let mut g = Pipeline::new(strategies, pictures, &plan);
        let read = |g: &mut Pipeline, s: usize, p: usize, text: &str, answer: Answer| {
            for _ in 0..7 {
                g.rows[s][p].push(Sample { ms: 100.0, text: Some(text.into()), passes: 1, skipped: false }, note(answer, 100.0, 0, None));
            }
        };
        // The application: the lone digit by the neural recogniser alone, and its invention on the bars.
        read(&mut g, 0, 0, "1", Answer::Paddle);
        read(&mut g, 0, 1, "l", Answer::Paddle);
        // The ladder before it: nothing on either.
        read(&mut g, 1, 0, "", Answer::Nobody);
        read(&mut g, 1, 1, "", Answer::Nobody);
        assert_eq!(g.prod_line().unwrap(), "pipeline | the application's reading (prod) invents text on: none-bars@2x");
        assert_eq!(g.old_line().unwrap(), "pipeline | the ladder before it (old) reads wrong: lone-1@2x");
        assert!(g.summary_table().contains("| old | comparison | lone-1@2x | none | none |"), "{}", g.summary_table());
        assert!(g.summary_table().contains("| prod | production | none | none-bars@2x (prod too) | none |"), "{}", g.summary_table());
        // Where prod is old — the neural recogniser not there — old is not read at all.
        assert_eq!(g.without_old_where_prod_is_old(Shape::production(true, true)), None);
        assert_eq!(g.without_old_where_prod_is_old(Shape::production(true, false)), None);
        assert!(g.old_line().is_some());
        let why = g.without_old_where_prod_is_old(Shape::production(false, true)).unwrap();
        assert!(why.contains("the neural recogniser is not there"));
        assert_eq!(g.old_line(), None);
        assert!(g.rows[1].iter().all(|r| r.cell.skipped.is_some()));
        assert!(g.summary_table().contains("| old | comparison | not measured: prod reads as old here"), "{}", g.summary_table());
    }

    /// Which reading prod is, in the three ways a Mac can have it.
    #[test]
    fn prod_says_which_reading_it_is() {
        assert_eq!(
            prod_reads_as(false, true),
            "pipeline | prod reads as the application does on this Mac: as old, the neural recogniser not being there"
        );
        assert!(prod_reads_as(true, false).ends_with("Windows' rule over the accurate ladder, as on Apple silicon"));
        assert!(prod_reads_as(true, true).ends_with(
            "the fast level checked by the neural recogniser, then Windows' rule over the accurate ladder, as on an Intel Mac"
        ));
    }

    /// A picture with text on which nothing is right too — Melodyne's lone dash — still holds a
    /// misreading against a strategy; a refusal where nothing is right is no invention.
    #[test]
    fn every_picture_with_text_counts_and_a_refusal_is_not_an_invention() {
        let dash: &'static Fixture = Box::leak(Box::new(Fixture {
            name: "dash",
            scale: 1,
            w_pt: 30,
            h_pt: 14,
            accept: &["-", ""],
            must_read: false,
            origin: Origin::Captured { platform: "windows" },
            png: &[],
        }));
        let plan = Plan::for_mode(false, false);
        let names = ["prod", "fast=paddle-checked", "rev2-tight"];
        let strategies: Vec<&Strategy> = names.iter().map(|n| &STRATEGIES[strategy(n).unwrap()]).collect();
        let bars = fixture("none-bars@2x").unwrap();
        let mut g = Pipeline::new(strategies, vec![dash, bars], &plan);
        let read = |g: &mut Pipeline, s: usize, p: usize, text: Option<&str>| {
            for _ in 0..2 {
                g.rows[s][p].push(
                    Sample { ms: 100.0, text: text.map(str::to_string), passes: 1, skipped: false },
                    note(Answer::Pass(Stage::Tight), 100.0, 0, None),
                );
            }
        };
        read(&mut g, 0, 0, Some("-"));
        read(&mut g, 0, 1, Some(""));
        read(&mut g, 1, 0, Some("4"));
        read(&mut g, 1, 1, None);
        read(&mut g, 2, 0, Some(""));
        read(&mut g, 2, 1, Some(""));
        let table = g.table();
        assert!(table.contains("| dash@1x (or nothing) |") && table.contains("| none-bars@2x (no text) |"), "{table}");
        let outcomes = g.outcomes();
        assert_eq!(outcomes[1].misread, [("dash@1x".to_string(), false)], "a 4 for a dash is wrong, required or not");
        assert!(outcomes[1].invented.is_empty(), "refused is not invented");
        assert_eq!(outcomes[1].refused, [("none-bars@2x".to_string(), false)]);
        assert!(outcomes[2].misread.is_empty() && outcomes[2].invented.is_empty(), "nothing is right on both");
        assert!(g.summary_table().contains("| fast=paddle-checked | candidate | dash@1x | none | none-bars@2x |"));
        // prod's line, which CI warns by, says what prod did not read right.
        read(&mut g, 0, 0, Some("4"));
        assert_eq!(g.prod_line().unwrap(), "pipeline | the application's reading (prod) reads wrong: dash@1x");
        assert!(Accuracy::Refused.refused_only() && !Accuracy::Right.refused_only());
    }

    #[test]
    fn the_closing_lines() {
        let o = |name: &str, faster: &[&str], slower: &[&str], misread: &[(&str, bool)], sum: f64| Outcome {
            name: name.into(),
            faster: faster.iter().map(|s| s.to_string()).collect(),
            slower: slower.iter().map(|s| s.to_string()).collect(),
            misread: misread.iter().map(|(l, p)| (l.to_string(), *p)).collect(),
            judged: 4,
            sum_ms: sum,
            prod_sum_ms: 2000.0,
            ..Outcome::default()
        };
        // The cheapest of those that read right and save time; one slower somewhere is out.
        let outcomes = [
            o("prod", &[], &[], &[], 0.0),
            o("rev2-tight", &["lone-1@2x"], &[], &[], 1500.0),
            o("fast=paddle-strict", &["lone-1@2x", "field-64@2x"], &[], &[], 400.0),
            o("fast=paddle", &["field-64@2x"], &["sfz-tune@1x"], &[], 300.0),
            o("fast>acc", &["field-64@2x"], &[], &[("lone-1@2x", false)], 200.0),
        ];
        let lines = closing_lines(&outcomes, false);
        assert!(lines[0].starts_with("closing | the cheapest way of reading that reads right and saves time: fast=paddle-strict (clearly faster on 2 of 4"), "{lines:?}");
        assert!(lines[0].contains("the others that do, dearer: rev2-tight"), "{lines:?}");
        assert!(lines[1].contains("fast>acc (reads lone-1@2x wrong)"), "{lines:?}");
        // Prod itself misreads a picture: the strict line names nobody who misreads it too, the
        // second line sets it aside.
        let outcomes = [
            o("prod", &[], &[], &[("k8-voices@1x", true)], 0.0),
            o("rev2-tight", &["lone-1@2x"], &[], &[("k8-voices@1x", true)], 1500.0),
        ];
        let lines = closing_lines(&outcomes, false);
        assert!(lines[0].ends_with("saves time: none does"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("prod itself does not read k8-voices@1x right; set aside") && l.contains(": rev2-tight (")), "{lines:?}");
        // A comparison that would pass — unchecked fast text, right on every picture and the
        // cheapest — is said apart and never named; the candidate is.
        let outcomes = [
            o("prod", &[], &[], &[], 0.0),
            o("fast>acc", &["field-64@2x", "lone-1@2x"], &[], &[], 100.0),
            o("fast=paddle-strict", &["field-64@2x"], &[], &[], 400.0),
            o("fast=paddle-checked", &["field-64@2x"], &[], &[("mel-note-dash@1x", false)], 380.0),
        ];
        let lines = closing_lines(&outcomes, false);
        assert!(lines[0].ends_with("saves time: fast=paddle-strict (clearly faster on 1 of 4 pictures judged; medians 400 ms against prod's 2000 ms over them)"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("faster than prod but not right: fast=paddle-checked (reads mel-note-dash@1x wrong)")), "{lines:?}");
        assert!(lines.last().unwrap().starts_with("closing | for comparison only, never a way the application may read: fast>acc (clearly faster on 2 of 4"), "{lines:?}");
        assert!(!lines[0].contains("fast>acc"), "{lines:?}");
        // A misreading prod shares is marked in the line of those faster but not right.
        let outcomes = [
            o("prod", &[], &[], &[("k8-voices@1x", true)], 0.0),
            o("fast=paddle", &["field-64@2x"], &[], &[("k8-voices@1x", true), ("val-db@2x", false)], 300.0),
        ];
        let lines = closing_lines(&outcomes, false);
        assert!(lines[1].contains("fast=paddle (reads k8-voices@1x (prod too), val-db@2x wrong)"), "{lines:?}");
        // The case of every Mac with the neural recogniser: prod itself invents on the drawn level
        // meter, as Windows does, and so does a candidate. Named only with that picture set aside;
        // old — slower everywhere, wrong where prod is right — in no line.
        let invents = |name: &str, faster: &[&str], slower: &[&str], misread: &[(&str, bool)], sum: f64| Outcome {
            invented: vec![("none-bars@2x".to_string(), true)],
            ..o(name, faster, slower, misread, sum)
        };
        let outcomes = [
            invents("prod", &[], &[], &[], 0.0),
            invents("acc+paddle+rest", &["lone-1@2x", "sfz-tune@1x"], &[], &[], 900.0),
            o("old", &[], &["lone-1@2x", "sfz-tune@1x"], &[("sfz-tune@1x", false)], 3000.0),
        ];
        let lines = closing_lines(&outcomes, false);
        assert!(lines[0].ends_with("saves time: none does"), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("faster than prod but not right: acc+paddle+rest (invents text on none-bars@2x (prod too))")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("prod itself does not read none-bars@2x right; set aside") && l.contains(": acc+paddle+rest (")), "{lines:?}");
        assert!(lines.iter().all(|l| !l.contains("old (") && !l.contains("old;")), "{lines:?}");
        // Noise, and too few reads, name nobody.
        assert!(closing_lines(&outcomes, true)[0].contains("too noisy"));
        let few = [o("prod", &[], &[], &[], 0.0), Outcome { name: "rev2".into(), ..Outcome::default() }];
        assert!(closing_lines(&few, false)[0].contains("too few reads for a verdict"));
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
