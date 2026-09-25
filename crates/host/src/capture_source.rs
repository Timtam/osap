//! Which picture a module's screen reads see: the `[screen]` table of its manifest, resolved
//! once per VM when the VM is built, and handed to every `host.screen` and `host.ocr` read the
//! VM makes. See `docs/api/screen.md#which-picture-a-read-sees` for the author's view, and
//! `backend/dxgi.rs` for what the one alternative to the standard path is.
//!
//! Kept out of lib.rs on purpose. Everything here is bookkeeping that can be unit-tested on
//! its own — the resolution rule above all, which decides for a whole tree of modules at once —
//! and lib.rs and image_search.rs only call in at the places where a VM is built, a read is
//! made, or a batch of image searches is captured.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use mlua::{Function, Lua};
use module_manifest::{ModuleManifest, ScreenDecl};

use crate::backend::{self, Backend, CaptureFn, CaptureSource, CapturedImage};
use crate::logging;

/// One `code_module` dependency whose code runs inside a dependent's VM, with what the
/// capture resolution needs to know about it. Collected by `collect_code_deps` in lib.rs,
/// which loads each dependency's manifest anyway.
pub(crate) struct CodeDep {
    pub id: String,
    pub entry: PathBuf,
    pub screen: ScreenDecl,
    /// Whether its manifest requires `screen` or `ocr`. A `[screen]` table only counts from a
    /// module that reads the screen: a helper library that happens to carry one must not
    /// decide how every module above it reads.
    pub reads_screen: bool,
    /// Its own direct dependencies (ids, required and optional), for the depth below.
    pub deps: Vec<String>,
    /// When it was first reached in the manifest-order walk: `dependencies` before
    /// `optional_dependencies`, depth-first. The tie-break after depth.
    pub order: u32,
    /// Its shortest distance from the VM owner, 1 for a direct dependency. Filled in by
    /// [`settle_depths`], because the walk that collects these visits each module once, the
    /// first way it finds it, which is not always the shortest.
    pub depth: u32,
}

/// Whether a capability list lets a module read the screen at all.
pub(crate) fn reads_screen(require: &[String]) -> bool {
    require.iter().any(|c| c == "screen" || c == "ocr")
}

/// Sets each dependency's `depth` to its shortest distance from the owner, breadth-first from
/// the owner's own dependencies. A dependency the walk cannot reach from them keeps the
/// largest depth, which only ever makes it lose a tie.
pub(crate) fn settle_depths(owner_deps: &[String], deps: &mut [CodeDep]) {
    let index: HashMap<String, usize> =
        deps.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
    for d in deps.iter_mut() {
        d.depth = u32::MAX;
    }
    let mut queue: VecDeque<(usize, u32)> = VecDeque::new();
    for id in owner_deps {
        if let Some(&i) = index.get(module_manifest::dep_id(id)) {
            queue.push_back((i, 1));
        }
    }
    while let Some((i, depth)) = queue.pop_front() {
        if deps[i].depth <= depth {
            continue;
        }
        deps[i].depth = depth;
        let next: Vec<usize> = deps[i]
            .deps
            .iter()
            .filter_map(|id| index.get(module_manifest::dep_id(id)).copied())
            .collect();
        for j in next {
            queue.push_back((j, depth + 1));
        }
    }
}

/// What one `[screen]` table asks for, or `None` when it asks for nothing (no `capture`).
/// Unknown values are noted and read as the safe choice: `standard` for `capture`, and the
/// standard fallback for `fallback`.
fn source_of(decl: &ScreenDecl, who: &str, notes: &mut Vec<String>) -> Option<CaptureSource> {
    let Some(capture) = decl.capture.as_deref() else {
        if let Some(f) = decl.fallback.as_deref() {
            notes.push(format!(
                "{who} declares [screen] fallback = \"{f}\" without a capture, so it is ignored"
            ));
        }
        return None;
    };
    // Matched case-insensitively, like `supported_os`: the file is written by hand.
    if capture.eq_ignore_ascii_case("standard") {
        if let Some(f) = decl.fallback.as_deref() {
            notes.push(format!(
                "{who} declares [screen] fallback = \"{f}\", which only applies to capture = \"duplication\", so it is ignored"
            ));
        }
        return Some(CaptureSource::Standard);
    }
    if !capture.eq_ignore_ascii_case("duplication") {
        notes.push(format!(
            "{who} declares [screen] capture = \"{capture}\", which this host does not know (it knows \"standard\" and \"duplication\"), so it reads the standard way"
        ));
        return Some(CaptureSource::Standard);
    }
    let or_standard = match decl.fallback.as_deref() {
        None => true,
        Some(f) if f.eq_ignore_ascii_case("standard") => true,
        Some(f) if f.eq_ignore_ascii_case("none") => false,
        Some(f) => {
            notes.push(format!(
                "{who} declares [screen] fallback = \"{f}\", which this host does not know (it knows \"standard\" and \"none\"), so a read duplication cannot answer is read the standard way"
            ));
            true
        }
    };
    Some(CaptureSource::Duplication { or_standard })
}

/// The source a VM reads through, and the notes worth a log line.
///
/// The rule, in order:
/// 1. The VM owner's own `[screen]` wins. A generated game module can override the runtime it
///    is built on.
/// 2. Otherwise the declaring code dependency with the smallest depth, then the earliest in
///    manifest order — among those whose manifest requires `screen` or `ocr`. So a game runtime
///    declares it once for every game built on it.
/// 3. Otherwise the standard way, which is every module written before `[screen]` existed.
///
/// Two dependencies at the same depth that disagree produce a note, and the first one wins:
/// nothing is gained by refusing to load over it, and the log says which was used.
pub(crate) fn resolve_capture(owner: &ScreenDecl, deps: &[CodeDep]) -> (CaptureSource, Vec<String>) {
    let mut notes = Vec::new();
    if let Some(src) = source_of(owner, "the module itself", &mut notes) {
        return (src, notes);
    }
    let mut declaring: Vec<(&CodeDep, CaptureSource)> = Vec::new();
    for d in deps {
        // Only what the host reads counts as a declaration; a table of unknown keys alone is
        // named in that dependency's own log line, not here.
        if d.screen.capture.is_none() && d.screen.fallback.is_none() {
            continue;
        }
        if !d.reads_screen {
            notes.push(format!(
                "its dependency '{}' declares [screen] but requires neither \"screen\" nor \"ocr\", so its declaration is not used",
                d.id
            ));
            continue;
        }
        if let Some(src) = source_of(&d.screen, &format!("its dependency '{}'", d.id), &mut notes) {
            declaring.push((d, src));
        }
    }
    declaring.sort_by_key(|(d, _)| (d.depth, d.order));
    let Some(&(first, src)) = declaring.first() else {
        return (CaptureSource::Standard, notes);
    };
    for (other, other_src) in declaring.iter().skip(1) {
        if other.depth == first.depth && *other_src != src {
            notes.push(format!(
                "its dependencies '{}' and '{}' are equally near and declare different [screen] tables; '{}' is used, being listed first",
                first.id, other.id, first.id
            ));
        }
    }
    notes.push(format!("the [screen] declaration comes from its dependency '{}'", first.id));
    (src, notes)
}

/// The source a VM reads the screen through, kept in the VM itself.
///
/// Keyed by VM, not by module index, and that is load-bearing: a code dependency's
/// `host.screen` is built for the dependency's index while its `host.ocr` falls through to the
/// owner's, so an index-keyed source would give one runtime's screen reads one source and its
/// OCR reads another.
struct VmCapture {
    src: CaptureSource,
    /// The VM owner's id, which the comparison line names.
    who: String,
    /// Whether this VM's first-read comparison is still to be made. Per VM, so every module
    /// that declares duplication gets its own answer, under its own name.
    compare: Cell<bool>,
}

/// The source for reads made from this VM — the standard one for a VM built before this
/// existed, or one whose module declared nothing.
pub(crate) fn vm_source(lua: &Lua) -> CaptureSource {
    lua.app_data_ref::<VmCapture>().map(|v| v.src).unwrap_or_default()
}

/// The source for a read of `region` from this VM — what every screen and OCR binding asks.
///
/// The first time after the VM was built with duplication, it also has the backend compare
/// the two sources and log the answer under the module's name: the question an author has
/// right after declaring the key and reloading. Asked again at later reads until the backend
/// could make it (duplication may still be opening). A VM that reads the standard way pays
/// one app-data lookup, as `vm_source` does.
pub(crate) fn read_source(lua: &Lua, backend: &dyn Backend, region: (i32, i32, i32, i32)) -> CaptureSource {
    read_source_with(lua, region, |who, r| backend.compare_capture_sources(who, r))
}

/// [`read_source`] with the comparison passed in, so the once-per-VM rule can be tested.
fn read_source_with(
    lua: &Lua,
    region: (i32, i32, i32, i32),
    compare: impl FnOnce(&str, (i32, i32, i32, i32)) -> bool,
) -> CaptureSource {
    let Some(v) = lua.app_data_ref::<VmCapture>() else {
        return CaptureSource::Standard;
    };
    if v.compare.get() && compare(&v.who, region) {
        v.compare.set(false);
    }
    v.src
}

/// Resolves and records the source for the VM being built for `manifest`, and says so in the
/// log when anything was declared. Silent for every module that declares nothing, which is
/// every module that exists today — the log of an ordinary session does not change.
///
/// Called on every build of the VM, first load and reload alike, from the manifest just read:
/// a reload after editing `[screen]` takes effect with nothing else to refresh.
pub(crate) fn apply(lua: &Lua, id: &str, manifest: &ModuleManifest, deps: &mut [CodeDep]) {
    let (src, lines) = plan(id, manifest, deps);
    for l in &lines {
        logging::line("capture", l);
    }
    // The author's question after declaring it is "does it change anything for this
    // application?", and the first read of the freshly built VM answers it — see `read_source`.
    let compare = Cell::new(matches!(src, CaptureSource::Duplication { .. }));
    lua.set_app_data(VmCapture { src, who: id.to_string(), compare });
}

/// What [`apply`] decides for a VM, and every `[capture]` log line it writes about it, in
/// order — apart from the VM, so that the lines themselves are tested.
fn plan(id: &str, manifest: &ModuleManifest, deps: &mut [CodeDep]) -> (CaptureSource, Vec<String>) {
    let owner_deps: Vec<String> = manifest
        .dependencies
        .iter()
        .chain(manifest.optional_dependencies.iter())
        .cloned()
        .collect();
    settle_depths(&owner_deps, deps);
    let (mut src, notes) = resolve_capture(&manifest.screen, deps);
    // Whether to say which source the module ended up with. A dependency counts only with what
    // the host reads (`resolve_capture` skips a table of unknown keys alone, and so does this);
    // the module's own table counts with unknown keys too, so a misspelt `captur` is answered
    // with the source the module actually reads through.
    let reads = |d: &ScreenDecl| d.capture.is_some() || d.fallback.is_some();
    let declared =
        reads(&manifest.screen) || !manifest.screen.unknown.is_empty() || deps.iter().any(|d| reads(&d.screen));
    let mut lines = Vec::new();
    // A misspelt key (`captur`) would otherwise leave nothing but the absence of a line to say
    // the module reads the standard way. Named here, for this module's own table only: every
    // dependency is loaded as a module of its own first and names its own.
    if let Some(line) = unknown_keys_line(&manifest.screen) {
        lines.push(format!("[{id}] {line}"));
    }
    for n in &notes {
        lines.push(format!("[{id}] {n}"));
    }
    if src != CaptureSource::Standard && !cfg!(windows) {
        // Accepted and ignored off Windows: the declaration names a Windows mechanism, and on
        // this platform every read keeps going the one way it always has.
        lines.push(format!(
            "[{id}] asks for desktop duplication, which exists on Windows only; on {} it reads the screen as it always has",
            std::env::consts::OS
        ));
        src = CaptureSource::Standard;
    }
    if declared {
        lines.push(format!("[{id}] reads the screen {}", describe(src)));
    }
    (src, lines)
}

/// The one warning line for the keys of a `[screen]` table this host does not know, or `None`
/// when there are none. The manifest still loads: a key a later host adds must not stop a
/// module on this one.
fn unknown_keys_line(decl: &ScreenDecl) -> Option<String> {
    let quoted: Vec<String> = decl.unknown.iter().map(|k| format!("'{k}'")).collect();
    let (last, rest) = quoted.split_last()?;
    let (noun, named) = if rest.is_empty() {
        ("a key", last.clone())
    } else {
        ("keys", format!("{} and {last}", rest.join(", ")))
    };
    Some(format!(
        "[screen] has {noun} {named} this host does not know (it knows capture and fallback), so {} ignored; check the spelling",
        if rest.is_empty() { "it is" } else { "they are" }
    ))
}

fn describe(src: CaptureSource) -> &'static str {
    match src {
        CaptureSource::Standard => "the standard way",
        CaptureSource::Duplication { or_standard: true } => {
            "through desktop duplication, and the standard way whenever duplication cannot answer"
        }
        CaptureSource::Duplication { or_standard: false } => {
            "through desktop duplication only (fallback = \"none\"): a read it cannot answer fails"
        }
    }
}

/// What the window dispatch runs before the first trigger callback that matches: opening the
/// duplication, for a VM that reads through it — its window just came forward, and the
/// callback about to run is where its first detection read is. `None` for every other VM, so
/// the dispatch of a module that declared nothing is exactly what it was.
///
/// Before the first callback, not after them all: after, that first read has already found
/// the duplication closed and paid for it. Not at load either: loading the graphics driver at
/// every start of a machine that merely has such a module installed costs everyone.
pub(crate) fn prewarm_hook(lua: &Lua) -> mlua::Result<Option<Function>> {
    if let CaptureSource::Duplication { .. } = vm_source(lua) {
        return Ok(Some(lua.create_function(|_, ()| {
            backend::prewarm_capture();
            Ok(())
        })?));
    }
    Ok(None)
}

/// Opens the duplication now, for a VM that reads through it; nothing for every other VM.
///
/// What [`prewarm_hook`] does, called directly: the report of a window already in front
/// (`onTrigger { initial = true }`) runs it before its first matching callback, as the
/// activation dispatch runs the hook. Without it, a game that is already in front when its
/// module loads would pay the opening cost on its first detection read.
pub(crate) fn prewarm_if_declared(lua: &Lua) {
    prewarm_if_declared_with(lua, backend::prewarm_capture);
}

/// [`prewarm_if_declared`] with the opening passed in, so which VMs it opens for can be tested
/// without touching the graphics driver.
fn prewarm_if_declared_with(lua: &Lua, open: impl FnOnce()) {
    if let CaptureSource::Duplication { .. } = vm_source(lua) {
        open();
    }
}

/// Whether an OCR error in a VM with `src` answers with an `error` field instead of raising.
///
/// Only a missing PICTURE — duplication had none to give — and only for a module that chose
/// `fallback = "none"`: there that is routine (every fullscreen toggle, every UAC prompt), and
/// raising would put a module-error dialog in front of the game, which can itself cost the
/// next read. A degenerate or oversized region is a mistake in the call, and a failed
/// recognition is a failure; both still raise everywhere, as they always have.
pub(crate) fn answers_instead_of_raising(src: CaptureSource, error: &str) -> bool {
    matches!(src, CaptureSource::Duplication { or_standard: false })
        && error.starts_with(backend::DUPLICATION_UNANSWERED)
}

/// The duplication part of the observation log line, or nothing when it did nothing. The
/// time is every round trip to the capture thread, the unanswered ones included.
pub(crate) fn duplication_clause((reads, us, unanswered): (u64, u64, u64)) -> String {
    if reads == 0 && unanswered == 0 {
        return String::new();
    }
    format!(
        ", and {reads} desktop duplication read(s){} costing {:.1} ms in all",
        if unanswered > 0 {
            format!(" plus {unanswered} it could not answer,")
        } else {
            String::new()
        },
        us as f64 / 1000.0
    )
}

// ---- The image worker's batch ---------------------------------------------------------------
//
// Kept apart from the worker itself (`run_batch` in image_search.rs) so that the change the
// source makes to it is two calls, and so the de-duplication can be tested without a thread.

/// What the worker captures once per batch: a region, read through a source.
pub(crate) type FrameKey = ((i32, i32, i32, i32), CaptureSource);

/// The distinct frames a batch needs, and for each task the index of its frame.
///
/// The source is part of the key: two modules polling the same region, one through each
/// source, must not share a frame — a template cut from one picture is matched against the
/// same picture, never the other.
pub(crate) fn frame_keys(tasks: impl Iterator<Item = FrameKey>) -> (Vec<FrameKey>, Vec<usize>) {
    let mut keys: Vec<FrameKey> = Vec::new();
    let mut idx = Vec::new();
    for key in tasks {
        let i = match keys.iter().position(|k| *k == key) {
            Some(i) => i,
            None => {
                keys.push(key);
                keys.len() - 1
            }
        };
        idx.push(i);
    }
    (keys, idx)
}

/// Captures every frame of a batch, each with the milliseconds it cost — or why it could not,
/// which every task of that frame is answered with.
///
/// Standard regions are read one at a time, as they always were — each is a
/// compositor-synchronised touch of the screen, and running them together would contend
/// rather than overlap. The duplication regions of one source go in ONE call: one request to
/// the capture thread and one GPU sync for all of them. Its time is booked to the first of
/// them, so the batch's total stays right.
pub(crate) fn capture_frames(capture: CaptureFn, keys: &[FrameKey]) -> Vec<(Result<CapturedImage, String>, u32)> {
    let missing = || Err(backend::CAPTURE_FAILED.to_string());
    let mut out: Vec<(Result<CapturedImage, String>, u32)> = keys.iter().map(|_| (missing(), 0)).collect();
    let mut groups: Vec<(CaptureSource, Vec<usize>)> = Vec::new();
    for (i, (region, src)) in keys.iter().enumerate() {
        if *src == CaptureSource::Standard {
            let t0 = std::time::Instant::now();
            let img = capture(std::slice::from_ref(region), *src).pop().unwrap_or_else(missing);
            out[i] = (img, t0.elapsed().as_millis() as u32);
        } else {
            match groups.iter_mut().find(|(s, _)| s == src) {
                Some((_, members)) => members.push(i),
                None => groups.push((*src, vec![i])),
            }
        }
    }
    for (src, members) in groups {
        let regions: Vec<(i32, i32, i32, i32)> = members.iter().map(|&i| keys[i].0).collect();
        let t0 = std::time::Instant::now();
        let images = capture(&regions, src);
        let ms = t0.elapsed().as_millis() as u32;
        for (n, (i, img)) in members.into_iter().zip(images).enumerate() {
            out[i] = (img, if n == 0 { ms } else { 0 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn decl(capture: Option<&str>, fallback: Option<&str>) -> ScreenDecl {
        ScreenDecl { capture: capture.map(str::to_string), fallback: fallback.map(str::to_string), unknown: Vec::new() }
    }

    #[test]
    fn unknown_keys_are_named_in_one_line() {
        assert_eq!(unknown_keys_line(&decl(Some("duplication"), None)), None);
        let one = ScreenDecl { unknown: vec!["captur".into()], ..decl(None, Some("none")) };
        assert_eq!(
            unknown_keys_line(&one).as_deref(),
            Some("[screen] has a key 'captur' this host does not know (it knows capture and fallback), so it is ignored; check the spelling")
        );
        let three = ScreenDecl { unknown: vec!["a".into(), "b".into(), "c".into()], ..ScreenDecl::default() };
        assert!(unknown_keys_line(&three).unwrap().contains("keys 'a', 'b' and 'c' this host does not know"));
        // A table of unknown keys alone declares nothing: the dependency is not consulted.
        let deps = [dep("runtime", three.clone(), true, 1, 1)];
        let (src, notes) = resolve_capture(&ScreenDecl::default(), &deps);
        assert_eq!(src, CaptureSource::Standard);
        assert!(notes.is_empty(), "{notes:?}");
    }

    /// The lines `apply` writes: a misspelt key of the module's own is named, with the source it
    /// reads through after it; a dependency whose table holds nothing the host reads makes its
    /// dependents say nothing at all, as every module that declares nothing says nothing.
    #[test]
    fn the_log_names_a_misspelt_key_and_stays_quiet_about_a_dependencys_stray_one() {
        let manifest = |screen: &str| -> ModuleManifest {
            toml::from_str(&format!("id = \"com.game.menu\"\nname = \"m\"\nversion = \"1.0.0\"\n{screen}")).unwrap()
        };
        let base = manifest("");
        let misspelt = manifest("[screen]\ncaptur = \"duplication\"\n");
        let (src, lines) = plan("com.game.menu", &misspelt, &mut []);
        assert_eq!(src, CaptureSource::Standard);
        assert_eq!(
            lines,
            vec![
                "[com.game.menu] [screen] has a key 'captur' this host does not know (it knows capture and fallback), so it is ignored; check the spelling".to_string(),
                "[com.game.menu] reads the screen the standard way".to_string(),
            ]
        );
        let stray = ScreenDecl { unknown: vec!["some_later_key".into()], ..ScreenDecl::default() };
        let mut deps = [dep("runtime", stray, true, 1, 1)];
        let (src, lines) = plan("com.game.menu", &base, &mut deps);
        assert_eq!(src, CaptureSource::Standard);
        assert!(lines.is_empty(), "{lines:?}");
        // A dependency that does declare something is still reported, stray keys or not.
        let mut deps = [dep("runtime", ScreenDecl { unknown: vec!["x".into()], ..decl(Some("standard"), None) }, true, 1, 1)];
        let (_, lines) = plan("com.game.menu", &base, &mut deps);
        assert_eq!(lines.last().map(String::as_str), Some("[com.game.menu] reads the screen the standard way"));
    }

    fn dep(id: &str, screen: ScreenDecl, reads: bool, depth: u32, order: u32) -> CodeDep {
        CodeDep {
            id: id.into(),
            entry: PathBuf::new(),
            screen,
            reads_screen: reads,
            deps: Vec::new(),
            order,
            depth,
        }
    }

    const DUP: CaptureSource = CaptureSource::Duplication { or_standard: true };
    const DUP_ONLY: CaptureSource = CaptureSource::Duplication { or_standard: false };

    #[test]
    fn nothing_declared_anywhere_reads_the_standard_way_silently() {
        let (src, notes) = resolve_capture(&ScreenDecl::default(), &[dep("a", ScreenDecl::default(), true, 1, 1)]);
        assert_eq!(src, CaptureSource::Standard);
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn the_owner_wins_over_every_dependency() {
        let deps = [dep("runtime", decl(Some("duplication"), Some("none")), true, 1, 1)];
        let (src, _) = resolve_capture(&decl(Some("standard"), None), &deps);
        assert_eq!(src, CaptureSource::Standard);
        let (src, _) = resolve_capture(&decl(Some("duplication"), None), &deps);
        assert_eq!(src, DUP);
    }

    #[test]
    fn a_runtime_declares_it_once_for_every_module_built_on_it() {
        let deps = [dep("runtime", decl(Some("duplication"), Some("none")), true, 1, 1)];
        let (src, notes) = resolve_capture(&ScreenDecl::default(), &deps);
        assert_eq!(src, DUP_ONLY);
        assert!(notes.iter().any(|n| n.contains("'runtime'")), "{notes:?}");
    }

    #[test]
    fn the_nearest_declaring_dependency_wins_then_manifest_order() {
        let deps = [
            dep("deep", decl(Some("standard"), None), true, 2, 1),
            dep("near-second", decl(Some("duplication"), Some("none")), true, 1, 3),
            dep("near-first", decl(Some("duplication"), None), true, 1, 2),
        ];
        let (src, notes) = resolve_capture(&ScreenDecl::default(), &deps);
        assert_eq!(src, DUP, "the earlier of the two nearest");
        // They disagree at the same depth, which is worth a line naming both.
        assert!(
            notes.iter().any(|n| n.contains("'near-first'") && n.contains("'near-second'")),
            "{notes:?}"
        );
    }

    #[test]
    fn a_declaration_from_a_module_that_does_not_read_the_screen_is_not_used() {
        let deps = [dep("helper", decl(Some("duplication"), None), false, 1, 1)];
        let (src, notes) = resolve_capture(&ScreenDecl::default(), &deps);
        assert_eq!(src, CaptureSource::Standard);
        assert!(notes.iter().any(|n| n.contains("'helper'") && n.contains("not used")), "{notes:?}");
    }

    #[test]
    fn unknown_values_are_named_and_read_the_safe_way() {
        let (src, notes) = resolve_capture(&decl(Some("window"), None), &[]);
        assert_eq!(src, CaptureSource::Standard);
        assert!(notes.iter().any(|n| n.contains("\"window\"")), "{notes:?}");
        let (src, notes) = resolve_capture(&decl(Some("duplication"), Some("never")), &[]);
        assert_eq!(src, DUP, "an unknown fallback keeps the fallback");
        assert!(notes.iter().any(|n| n.contains("\"never\"")), "{notes:?}");
        // Written by hand, so capitals are what a person types.
        let (src, _) = resolve_capture(&decl(Some("Duplication"), Some("None")), &[]);
        assert_eq!(src, DUP_ONLY);
    }

    #[test]
    fn a_fallback_without_a_capture_is_ignored_and_says_so() {
        let (src, notes) = resolve_capture(&decl(None, Some("none")), &[]);
        assert_eq!(src, CaptureSource::Standard);
        assert!(notes.iter().any(|n| n.contains("without a capture")), "{notes:?}");
        // And it does not stop a dependency's declaration from applying.
        let deps = [dep("runtime", decl(Some("duplication"), None), true, 1, 1)];
        let (src, _) = resolve_capture(&decl(None, Some("none")), &deps);
        assert_eq!(src, DUP);
    }

    #[test]
    fn depth_is_the_shortest_path_not_the_first_one_found() {
        // owner -> a -> b, and owner -> b directly: the walk meets b through a first (depth 2),
        // but b is a direct dependency, so its depth is 1.
        let mut deps = vec![
            CodeDep { deps: vec![], ..dep("b", ScreenDecl::default(), true, 0, 2) },
            CodeDep { deps: vec!["b >= 1.0".into()], ..dep("a", ScreenDecl::default(), true, 0, 1) },
            CodeDep { deps: vec![], ..dep("orphan", ScreenDecl::default(), true, 0, 3) },
        ];
        settle_depths(&["a".to_string(), "b".to_string()], &mut deps);
        let depth = |id: &str| deps.iter().find(|d| d.id == id).unwrap().depth;
        assert_eq!(depth("a"), 1);
        assert_eq!(depth("b"), 1);
        assert_eq!(depth("orphan"), u32::MAX);
    }

    #[test]
    fn only_a_missing_picture_under_fallback_none_answers_instead_of_raising() {
        let missing = format!("{} — it is still opening", backend::DUPLICATION_UNANSWERED);
        assert!(answers_instead_of_raising(DUP_ONLY, &missing));
        assert!(!answers_instead_of_raising(DUP, &missing));
        assert!(!answers_instead_of_raising(CaptureSource::Standard, &missing));
        assert!(!answers_instead_of_raising(DUP_ONLY, "OCR failed: no language"));
        // A region that could never be read is the caller's mistake and still raises, though
        // its message is a capture failure too.
        assert!(!answers_instead_of_raising(DUP_ONLY, backend::CAPTURE_FAILED));
        let too_large = format!("{}: the region is larger than 40 million pixels", backend::CAPTURE_FAILED);
        assert!(!answers_instead_of_raising(DUP_ONLY, &too_large));
        // The family stays one family: the missing picture is still a capture failure.
        assert!(backend::DUPLICATION_UNANSWERED.starts_with(backend::CAPTURE_FAILED));
    }

    #[test]
    fn the_log_clause_is_empty_until_duplication_did_something() {
        assert_eq!(duplication_clause((0, 0, 0)), "");
        assert_eq!(duplication_clause((3, 1500, 0)), ", and 3 desktop duplication read(s) costing 1.5 ms in all");
        assert_eq!(
            duplication_clause((0, 61_000, 2)),
            ", and 0 desktop duplication read(s) plus 2 it could not answer, costing 61.0 ms in all"
        );
    }

    fn built(src: CaptureSource) -> Lua {
        let lua = Lua::new();
        let mut manifest: ModuleManifest = toml::from_str(
            "id = \"com.game.menu\"\nname = \"m\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        manifest.screen = match src {
            CaptureSource::Standard => ScreenDecl::default(),
            CaptureSource::Duplication { or_standard } => decl(
                Some("duplication"),
                Some(if or_standard { "standard" } else { "none" }),
            ),
        };
        apply(&lua, "com.game.menu", &manifest, &mut []);
        lua
    }

    #[test]
    fn each_vm_compares_once_under_its_own_name_and_asks_again_until_it_could() {
        let asked = Cell::new(0u32);
        let can = Cell::new(false);
        let read = |lua: &Lua| {
            read_source_with(lua, (1, 1, 1, 1), |who, _| {
                assert_eq!(who, "com.game.menu", "the line names the module that read");
                asked.set(asked.get() + 1);
                can.get()
            })
        };
        let lua = built(DUP_ONLY);
        if !cfg!(windows) {
            // Accepted and ignored off Windows: the standard source, and nothing to compare.
            assert_eq!(read(&lua), CaptureSource::Standard);
            assert_eq!(asked.get(), 0);
            return;
        }
        // Still opening: not made, so the next read asks again.
        assert_eq!(read(&lua), DUP_ONLY);
        assert_eq!(read(&lua), DUP_ONLY);
        assert_eq!(asked.get(), 2);
        // Made: never again for this VM.
        can.set(true);
        read(&lua);
        read(&lua);
        assert_eq!(asked.get(), 3);
        // A second VM declaring the same gets its own, rather than none because the first
        // module took the process's only one.
        read(&built(DUP_ONLY));
        assert_eq!(asked.get(), 4);
        // A VM reading the standard way never compares.
        assert_eq!(read(&built(CaptureSource::Standard)), CaptureSource::Standard);
        assert_eq!(asked.get(), 4);
    }

    #[test]
    fn the_duplication_is_opened_before_the_first_matching_trigger_runs() {
        // The real window prelude, as the runtime loads it, with a host table holding only
        // what dispatch touches.
        let lua = Lua::new();
        let host = lua.create_table().unwrap();
        host.set("window", lua.create_table().unwrap()).unwrap();
        let os = lua.create_table().unwrap();
        os.set("current", std::env::consts::OS).unwrap();
        host.set("os", os).unwrap();
        lua.load(format!("return function(host) {}\nend", crate::WINDOW_PRELUDE))
            .into_function()
            .unwrap()
            .call::<Function>(())
            .unwrap()
            .call::<()>(&host)
            .unwrap();
        lua.globals().set("host", &host).unwrap();
        let order: Vec<String> = lua
            .load(
                r#"
                local order = {}
                host.window.onTrigger({ title = "Game" }, nil, function() table.insert(order, "first trigger") end)
                host.window.onTrigger({ title = "Game" }, nil, function() table.insert(order, "second trigger") end)
                host.window.onTrigger({ title = "Other" }, nil, function() table.insert(order, "never") end)
                local function prepare() table.insert(order, "opened") end
                host.window._dispatchActivate({ title = "Game" }, prepare)
                -- No match: nothing to prepare for.
                host.window._dispatchActivate({ title = "Desktop" }, prepare)
                -- A module that declared nothing passes no hook at all.
                host.window._dispatchActivate({ title = "Game" }, nil)
                return order
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(order, ["opened", "first trigger", "second trigger", "first trigger", "second trigger"]);
        // And the hook exists only for a VM that reads through duplication, which off Windows
        // none does: there the declaration resolves to the standard source.
        assert_eq!(prewarm_hook(&built(DUP)).unwrap().is_some(), cfg!(windows));
        assert!(prewarm_hook(&built(CaptureSource::Standard)).unwrap().is_none());
    }

    /// The initial window report's half of the same rule (`prewarm_if_declared`): it opens the
    /// duplication for a VM that reads through it, with or without the fallback, and for no
    /// other VM — including one built before any source was recorded.
    #[test]
    fn the_initial_report_opens_the_duplication_only_for_a_vm_that_reads_through_it() {
        let opened = Cell::new(0u32);
        let open = || opened.set(opened.get() + 1);
        prewarm_if_declared_with(&built(DUP), open);
        prewarm_if_declared_with(&built(DUP_ONLY), open);
        // Off Windows the declaration resolves to the standard source, so nothing opens.
        let expected = if cfg!(windows) { 2 } else { 0 };
        assert_eq!(opened.get(), expected);
        prewarm_if_declared_with(&built(CaptureSource::Standard), open);
        prewarm_if_declared_with(&Lua::new(), open);
        assert_eq!(opened.get(), expected);
    }

    #[test]
    fn the_worker_does_not_share_a_frame_across_sources() {
        let r = (10, 20, 30, 40);
        let (keys, idx) = frame_keys(
            [(r, CaptureSource::Standard), (r, DUP), (r, CaptureSource::Standard), ((0, 0, 5, 5), DUP)].into_iter(),
        );
        assert_eq!(keys, vec![(r, CaptureSource::Standard), (r, DUP), ((0, 0, 5, 5), DUP)]);
        assert_eq!(idx, vec![0, 1, 0, 2]);
    }

    static CALLS: Mutex<Vec<(usize, CaptureSource)>> = Mutex::new(Vec::new());

    fn recording(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, String>> {
        CALLS.lock().unwrap().push((regions.len(), src));
        regions.iter().map(|&(_, _, w, h)| Ok(CapturedImage { w: w as u32, h: h as u32, rgba: vec![] })).collect()
    }

    #[test]
    fn duplication_regions_of_a_batch_are_one_call_and_standard_ones_one_each() {
        let keys = [
            ((0, 0, 1, 1), CaptureSource::Standard),
            ((0, 0, 2, 2), DUP),
            ((5, 5, 3, 3), CaptureSource::Standard),
            ((9, 9, 4, 4), DUP),
            ((1, 1, 5, 5), DUP_ONLY),
        ];
        let frames = capture_frames(recording, &keys);
        let calls = CALLS.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec![(1, CaptureSource::Standard), (1, CaptureSource::Standard), (2, DUP), (1, DUP_ONLY)]
        );
        // Every frame lands at its own key, whichever call brought it.
        let sizes: Vec<u32> = frames.iter().map(|(f, _)| f.as_ref().unwrap().w).collect();
        assert_eq!(sizes, vec![1, 2, 3, 4, 5]);
    }
}
