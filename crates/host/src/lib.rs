//! Host runtime: a module **manager** that hosts many Luau modules concurrently
//! in one process (one VM each, shared services + one event loop), per
//! `docs/module-runtime-and-lifecycle.md`. The OS is reached only through
//! [`backend::Backend`]. The `host` API (design: primitives are first-class —
//! see `docs/host-api-capability-catalog.md`) is installed per module VM and
//! bound to that module's root + the shared services.

mod backend;
mod gui;
pub mod logging;
mod appcfg;
mod portable;
pub mod registry;
mod settings;
mod speech;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use rayon::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, Table};

use backend::{Backend, CapturedImage, ControlInfo, HostEvents, MouseButton, WinInfo};
use module_manifest::LoadedModule;

const WINDOW_PRELUDE: &str = include_str!("window_prelude.luau");
// The overlay runtime is the code module `com.platform.overlay`
// (modules/overlay-runtime/src/main.luau); modules that need it depend on it and
// receive it via host.require — it is no longer injected into every VM.

/// A registered global hotkey: which module owns it, the VM + callback to fire,
/// and the spec so it can be re-registered with the OS after a disable/enable.
struct HotkeyReg {
    module_idx: usize,
    lua: Lua,
    cb: RegistryKey,
    spec: String,
    /// The spec parsed to `(vk, modifier-mask)` for cross-module conflict
    /// detection (`None` if unparseable — then conflict-checked only via the OS).
    binding: Option<(u32, u8)>,
    /// Whether this registration currently HOLDS its combination at the OS.
    ///
    /// Recorded rather than inferred, because the two are not the same thing and the
    /// difference was the bug: a registration skipped because another module held the combo
    /// stayed skipped for the session, so disabling that other module did not hand the key
    /// over — it needed a restart. `refresh_hotkeys` derives this from the enabled set every
    /// time that set changes, the way `refresh_captured` already derives the captured-key
    /// set. Liveness that is stored goes stale; liveness that is derived cannot.
    live: bool,
}

/// One overlay's claim on an arbiter slot: which module/VM owns it, how specific
/// it is, whether it currently matches, and the callbacks to (de)activate it.
struct ArbiterClaim {
    handle: i64,
    module_idx: usize,
    specificity: i64,
    lua: Lua,
    on_activate: RegistryKey,
    on_deactivate: RegistryKey,
    matching: bool,
}

/// A competition group of overlays: at most one claim (the matching one of
/// highest specificity) is active at a time.
#[derive(Default)]
struct ArbiterSlot {
    claims: Vec<ArbiterClaim>,
    active: Option<i64>,
    /// Re-entrancy guard: true while [`Shared::arbiter_resolve`] is firing this
    /// slot's callbacks (which may legally call back into the arbiter).
    resolving: bool,
    /// Set when a (possibly re-entrant) change needs another resolve pass.
    dirty: bool,
}

/// Services shared by every module: the OS backend, one speech engine, one audio
/// output, the global hotkey-id counter, and the central event routing.
struct Shared {
    backend: Rc<dyn Backend>,
    speech: speech::Speech,
    /// The app's OWN hotkey id, taken from `alloc_id` before any module is loaded so that
    /// no module can ever be handed the same one. A fixed number would not do: the counter
    /// never resets, and re-registering the same id at the same window silently REPLACES
    /// the earlier hotkey — the reload key would just stop working one day.
    reload_hotkey_id: Cell<i32>,
    /// Asked for by the reload-everything hotkey, drained by the pump.
    ///
    /// The keystroke cannot do the work itself: it arrives at a Dispatcher, which BORROWS the
    /// module list, and rebuilding a module has to replace entries in that same list. So the
    /// key only asks, and the loop — which owns the list — answers on its next turn.
    reload_all: Cell<bool>,
    /// Audio output, opened lazily on first `host.sound.play` so we don't hold
    /// the audio device at startup — this tool overlays audio software, and
    /// grabbing the device can interrupt it. Kept alive as (stream, handle).
    audio: RefCell<Option<(rodio::OutputStream, rodio::OutputStreamHandle)>>,
    next_id: Cell<i32>,
    /// module_idx → root directory.
    roots: RefCell<Vec<PathBuf>>,
    /// module_idx → module id (for persisting the disabled set).
    ids: RefCell<Vec<String>>,
    /// module_idx → enabled.
    enabled: RefCell<Vec<bool>>,
    /// Modules this platform will not run, kept so the manager can list them anyway.
    ///
    /// They have no module index — nothing was loaded — so they are held here rather than in
    /// the parallel vectors, and the list is deduplicated by id because a hot-load re-reads
    /// the same directory.
    excluded: RefCell<Vec<ExcludedModule>>,
    /// module_idx → the capability names its manifest declares.
    ///
    /// Parallel to `ids`, and pushed with it: `install_host_api` runs later (from
    /// `populate_vm`) and reads this to decide what the module's `host` table carries.
    caps: RefCell<Vec<HashSet<String>>>,
    /// Global hotkey id → its registration (owning module, VM, callback, spec).
    hotkeys: RefCell<HashMap<i32, HotkeyReg>>,
    /// Captured keys: (vk, modifier-mask, module_idx, token, VM, callback). The
    /// per-capture `token` lets `release` target the exact registration: two overlays
    /// in the SAME module that both capture a key (e.g. Komplete Kontrol's chrome and
    /// its Preferences dialog both capturing Tab) would otherwise collide on
    /// (vk, mask, module_idx), so the first to deactivate would release the key the
    /// second still needs. Keyed by token, one overlay's release can't drop another's.
    keys: RefCell<Vec<(u32, u8, usize, i64, Lua, RegistryKey)>>,
    /// Unified portable store: per-module enabled-state + settings.
    store: RefCell<settings::Store>,
    /// module_idx → (setting key → schema), for validation + the GUI. Not persisted.
    schemas: RefCell<Vec<HashMap<String, settings::Field>>>,
    /// onChange callbacks: (module_idx, key) → [(VM, callback)].
    on_change: RefCell<HashMap<(usize, String), Vec<(Lua, RegistryKey)>>>,
    /// Coalesces setting auto-saves to the event-loop tick.
    dirty: Cell<bool>,
    /// One-shot timers: (deadline, module_idx, VM, callback), fired from the tick.
    timers: RefCell<Vec<(Instant, usize, Lua, RegistryKey)>>,
    /// Data exported by library modules (module id → value), exposed to dependent
    /// modules via `host.require`. Data only — Lua functions can't cross VMs.
    exports: RefCell<HashMap<String, serde_json::Value>>,
    /// Overlay arbiter: slot key → competing claims. Only the matching claim of
    /// highest specificity in a slot is active; cross-VM, so an inheriting overlay
    /// (higher specificity) suppresses its base when it matches. See
    /// docs/nested-overlays-design.md.
    arbiter: RefCell<HashMap<String, ArbiterSlot>>,
    /// Monotonic "the world may have changed" counter. Bumped whenever an OS event is
    /// dispatched into a module, a timer fires, or a module drives input — i.e. at every
    /// point where what is on screen, focused or enumerable can differ from a moment ago.
    /// A module memoizes an expensive observation (which plugin control is focused, say)
    /// against it, so repeats within one dispatch are free while a genuinely new
    /// situation is always re-observed. See `host.epoch()`.
    /// Answers the OS gave us during the CURRENT epoch (see `Observations`).
    observations: RefCell<Observations>,
    /// Per-pump-iteration accounting for the OS-event phase: (activate dispatches,
    /// ms in activate, ms in focus-change, ms in key dispatch). Reset each iteration and
    /// reported when one overruns — "os events 446 ms" names no cause on its own, and this
    /// phase is three different fan-outs with very different multiplicities.
    ev_counts: Cell<(u32, u128, u128, u128)>,
    epoch: Cell<u64>,
    /// See bump_input_epoch: turns over only when something ACTED on the screen.
    input_epoch: Cell<u64>,
    /// Monotonic id source for arbiter claim handles.
    next_arbiter: Cell<i64>,
    /// Monotonic id source for captured-key registration tokens.
    next_key_token: Cell<i64>,
    /// Recurring timers: (next deadline, interval, module_idx, VM, callback). Fired
    /// from the tick and re-armed even while the owning module is disabled, so a
    /// poll resumes on re-enable instead of dying (unlike a Lua self-rescheduling
    /// host.timer.after chain, whose reschedule is skipped while disabled).
    recurring: RefCell<Vec<(Instant, Duration, usize, Lua, RegistryKey)>>,
    /// Module callback failures (Lua errors + caught panics) queued for the GUI to
    /// show in an accessible dialog, as (title, message). Drained each tick.
    errors: RefCell<Vec<(String, String)>>,
    /// Dedup keys (id+context) so a module's fault in a given context dialogs once
    /// per enabled session (cleared on toggle); the full message still goes to the
    /// log every time. Keyed coarsely so a per-tick-varying message can't flood.
    error_seen: RefCell<HashSet<String>>,
    /// Async image search: queued template matches go to a worker thread; results
    /// come back on the loop tick. Pending callbacks are keyed by request id and
    /// touched only on the main thread.
    image_tasks: std::sync::mpsc::Sender<ImageTask>,
    image_results: std::sync::mpsc::Receiver<ImageResult>,
    pending_image: RefCell<HashMap<u64, (Lua, RegistryKey, usize)>>,
    next_image_id: Cell<u64>,
    /// Decoded-template cache (path → (mtime, RGBA, last-used seq)); avoids re-reading +
    /// re-decoding a PNG on every imageSearch / imageSearchAsync / imageSearchMulti call,
    /// including the recurring landmark poll. Invalidated per-entry when the file's mtime
    /// changes (a calibration re-capture), so a re-captured template is picked up live.
    /// The seq is bumped on each access and used to evict the least-recently-used entry
    /// once the cache exceeds TEMPLATE_CACHE_CAP — a safety valve against unbounded growth
    /// (practically never hit: modules use a handful of fixed template paths). Main-thread
    /// only (the worker holds an already-cloned Arc, never the map).
    template_cache: RefCell<HashMap<PathBuf, (SystemTime, Arc<Decoded>, u64)>>,
    /// Monotonic last-used stamp source for the template cache's LRU eviction.
    template_seq: Cell<u64>,
    /// Set by `host.window.recheck()`; drained on the tick to fire a cross-VM overlay
    /// re-check (as an OS focus event would), for state changes an overlay itself caused.
    recheck_requested: Cell<bool>,
    /// Shows a notification, and says whether it managed to.
    ///
    /// Installed by the GUI once its tray icon exists, so `None` means headless — there is
    /// nothing to show anything in. Returns false where the platform has no notifications at
    /// all, which is macOS: `show_balloon` is Windows-only and answers false everywhere else.
    notify: RefCell<Option<Box<dyn Fn(&str, &str) -> bool>>>,
}

/// Max distinct template PNGs kept decoded in the cache before least-recently-used
/// eviction. Far above any real module's fixed template count — purely a growth cap.
const TEMPLATE_CACHE_CAP: usize = 64;

/// A decoded template image (RGBA), shared behind an `Arc` and cached by path+mtime so
/// repeated searches (e.g. the recurring landmark poll, a toggle's on/off pair) don't
/// re-read and re-decode the PNG on every call.
struct Decoded {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    /// Template pixel indices to compare FIRST, rarest colour first — see `probe_order`.
    probes: Vec<u32>,
}

/// The order in which a template's pixels should be compared, most discriminating first.
///
/// `matches_at` rejects a candidate position at its first mismatching pixel, so which
/// pixel it looks at first decides how much work the ~700k rejections cost. Row-major
/// order starts at (0,0), which on a wordmark or a label is plain background — measured
/// against a real plugin frame, the top-left pixel of a Cerberus landmark survives at
/// 68.8 % of all candidate positions, while a pixel on a glyph stroke survives at 0.49 %.
/// Testing the second one first is 139x fewer positions that need any further comparison.
///
/// The template alone tells us which pixels those are: its own rarest colours are its
/// content, its commonest is its background. So order by ascending frequency of the
/// coarsely-quantised colour and keep the front of that list. Wildcard (alpha 0) pixels
/// are skipped — they match everything by definition.
///
/// This changes NOTHING about the verdict. It is the same conjunction over the same
/// pixels, evaluated in a better order.
fn probe_order(w: u32, h: u32, rgba: &[u8]) -> Vec<u32> {
    const PROBES: usize = 12;
    let n = (w as usize) * (h as usize);
    let mut hist = std::collections::HashMap::<u32, u32>::new();
    for i in 0..n {
        let o = i * 4;
        if rgba[o + 3] == 0 {
            continue;
        }
        let key = ((rgba[o] as u32 >> 4) << 8) | ((rgba[o + 1] as u32 >> 4) << 4) | (rgba[o + 2] as u32 >> 4);
        *hist.entry(key).or_insert(0) += 1;
    }
    let mut idx: Vec<u32> = (0..n as u32)
        .filter(|i| rgba[(*i as usize) * 4 + 3] != 0)
        .collect();
    idx.sort_by_key(|i| {
        let o = (*i as usize) * 4;
        let key = ((rgba[o] as u32 >> 4) << 8) | ((rgba[o + 1] as u32 >> 4) << 4) | (rgba[o + 2] as u32 >> 4);
        hist.get(&key).copied().unwrap_or(0)
    });
    idx.truncate(PROBES);
    idx
}

/// A queued async template match. The worker CAPTURES `region` itself (off the main
/// thread) then matches `tmpls` against it; `region` = (x, y, w, h) in screen coords,
/// whose (x, y) is added back into the reported hit.
///
/// `tmpls` holds ONE OR MORE templates tried in order against the SAME captured frame,
/// first hit wins — several renderings of the same thing (a dialog's close glyph as two
/// plugin versions draw it). Chaining single-template searches instead would pay a fresh
/// region capture (~1 compositor frame) per template.
struct ImageTask {
    id: u64,
    region: (i32, i32, i32, i32),
    tmpls: Vec<Arc<Decoded>>,
    tol: u8,
    scales: Vec<f32>,
}

/// The worker's answer: the hit rect in screen coords plus the 1-based index of the
/// template that matched, or None.
struct ImageResult {
    id: u64,
    hit: Option<(i32, i32, u32, u32, usize)>,
    /// How long this search actually took, split into the shared capture and this
    /// task's own matching. Reported so a slow search is a NUMBER in the log rather
    /// than an inference from the gap between two events — the landmark poll is
    /// invisible otherwise, and "my overlay takes 16 seconds to appear" has to be
    /// diagnosable without rebuilding the host.
    capture_ms: u32,
    match_ms: u32,
    /// Tasks sharing this batch — the fan-out that the capture was shared across.
    batch: u32,
    /// Set on exactly one result per batch, so the timing is logged once rather than
    /// once per task (they all carry the same batch figures).
    first_of_batch: bool,
}

/// The image-search worker: CAPTURES each task's region and runs the CPU-heavy
/// template match, both off the main thread, so neither the ~1-frame capture nor the
/// match blocks the event loop. `capture` is the backend's stateless capture routine
/// (a plain `fn` pointer, hence `Send`). Exits when the task sender is dropped (app
/// teardown). Touches only owned data — safe to leave running across process exit.
///
/// FAN-OUT SHARING: after the first task, it briefly collects any others queued in the
/// same tick (several libraries' landmark polls fire together, staggered by a few ms)
/// into one batch, then CAPTURES EACH DISTINCT REGION ONCE and matches every task's
/// template against its region's shared frame. So N libraries polling the same plugin
/// region cost 1 capture per tick, not N — "one frame, many comparisons" over
/// simultaneous consumers. Landmark polling isn't latency-critical, so the tiny
/// collection wait is invisible; results are byte-for-byte what per-task capture gave.
fn spawn_image_worker(
    capture: fn(i32, i32, i32, i32) -> Option<CapturedImage>,
    tasks: std::sync::mpsc::Receiver<ImageTask>,
    results: std::sync::mpsc::Sender<ImageResult>,
) {
    std::thread::spawn(move || {
        let batch_window = Duration::from_millis(5);
        while let Ok(first) = tasks.recv() {
            // Collect this tick's batch: the first task, plus any queued within a short
            // window (same-tick polls arrive microseconds-to-ms apart).
            let mut batch = vec![first];
            let deadline = Instant::now() + batch_window;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match tasks.recv_timeout(deadline - now) {
                    Ok(t) => batch.push(t),
                    Err(_) => break, // window elapsed (batch complete) or sender dropped
                }
            }
            // Capture each DISTINCT region once; match every task's template against its
            // region's shared frame. The batch is small (one task per active library), so
            // a linear region lookup is fine.
            //
            // The captures stay SERIAL on purpose: each is a compositor-synchronised screen
            // read, so running them together would contend rather than overlap, and the
            // whole point of the batch is that there is only one per region anyway. The
            // MATCHING is what gets spread across cores — see below.
            let mut frames: Vec<((i32, i32, i32, i32), Option<CapturedImage>, u32)> = Vec::new();
            let batch_len = batch.len() as u32;
            let mut plan: Vec<(&ImageTask, usize)> = Vec::with_capacity(batch.len());
            for t in &batch {
                let idx = match frames.iter().position(|(r, _, _)| *r == t.region) {
                    Some(i) => i,
                    None => {
                        let (rx, ry, rw, rh) = t.region;
                        let t0 = Instant::now();
                        let cap = capture(rx, ry, rw, rh);
                        let ms = t0.elapsed().as_millis() as u32;
                        frames.push((t.region, cap, ms));
                        frames.len() - 1
                    }
                };
                plan.push((t, idx));
            }
            // MATCH IN PARALLEL. Every candidate position is independent of every other, so
            // this is the shape a thread pool is actually for; the tasks in a batch are
            // independent too. Sequentially, twelve installed libraries cost twelve full
            // scans back to back before any of them can answer — each one comfortably under
            // the logging threshold and therefore invisible, while together they were the
            // several seconds before the right overlay appeared.
            let t1 = Instant::now();
            let hits: Vec<Option<(i32, i32, u32, u32, usize)>> = plan
                .par_iter()
                .map(|(t, idx)| {
                    let (rx, ry, _, _) = t.region;
                    frames[*idx].1.as_ref().and_then(|cap| {
                        // Templates in order against this one frame; first hit wins.
                        t.tmpls.iter().enumerate().find_map(|(n, tm)| {
                            find_template_scaled(
                                cap, tm.w, tm.h, &tm.rgba, t.tol, &t.scales, &tm.probes,
                            )
                            .map(|(ox, oy, mw, mh)| (rx + ox as i32, ry + oy as i32, mw, mh, n + 1))
                        })
                    })
                })
                .collect();
            let match_ms = t1.elapsed().as_millis() as u32;
            let capture_ms: u32 = frames.iter().map(|f| f.2).sum();
            for (i, ((t, _), hit)) in plan.iter().zip(hits).enumerate() {
                let res = ImageResult {
                    id: t.id,
                    hit,
                    capture_ms,
                    match_ms,
                    batch: batch_len,
                    first_of_batch: i == 0,
                };
                if results.send(res).is_err() {
                    return; // main thread gone
                }
            }
        }
    });
}

/// One cached UIA element lookup. `key` renders the arguments; `go` does the traversal.
fn located(
    sh: &Rc<Shared>,
    what: &str,
    key: String,
    go: impl FnOnce() -> Option<(i32, i32)>,
) -> Option<(i32, i32)> {
    {
        let mut obs = sh.observations();
        obs.asked += 1;
        if let Some(cached) = obs.points.get(&key).copied() {
            obs.served += 1;
            return cached;
        }
    }
    let t = Instant::now();
    let found = go();
    slow_observation(what, &key, t);
    sh.observations().points.insert(key, found);
    found
}

/// Names a single OS observation that blocked the pump thread for a long time.
///
/// These are CROSS-PROCESS calls: a UIA traversal runs inside the target application, so its
/// cost is that application's to decide, and a DAW that is mid-redraw because its window just
/// came to the front can take hundreds of milliseconds to answer. That is invisible from
/// here — the call simply returns late — and it lands on the one thread that also owns the
/// keyboard hook.
fn slow_observation(what: &str, detail: &str, started: Instant) {
    let ms = started.elapsed().as_millis();
    if ms >= 50 {
        logging::line(
            "observe",
            &format!("{what}({detail}) blocked the pump for {ms} ms"),
        );
    }
}

/// What the OS told us during one epoch, so it is asked ONCE.
///
/// An epoch means "the world may have changed"; within one, the same question necessarily
/// has the same answer, so asking it twice is pure duplication. And it is duplicated hard:
/// each Kontakt cell's `identify` asks whether a control is a Komplete Kontrol, whether it
/// is a Kontakt, and which version — and there are six cells, and the runtime is a CODE
/// module, so all of that exists separately inside every dependent module's VM. Nine
/// modules, one shared epoch, the same handful of distinct questions, ~100 times over.
///
/// The layer above this already memoizes (`originMemo`, per spec and epoch), but it caches
/// the COMPOSITE verdict — the duplication is one level down, where `host.element.findAny` and
/// `host.window.controls` go straight to the backend on every single call. UIA traversal is
/// the expensive part of a poll tick, and it is being paid for answers we already have.
///
/// NEGATIVE answers are cached too, deliberately, and that is where nearly all the saving
/// is: in a poll, almost every identity check is expected to fail. "Not found" is a real
/// answer within an epoch, not an "ask me later" — the epoch turning over IS ask-me-later,
/// and it turns over on every image batch and every focus change.
#[derive(Default)]
struct Observations {
    epoch: u64,
    /// Also tracked, and also invalidating: the LOCATE family below returns points that get
    /// CLICKED, and a click must never be aimed at a coordinate worked out before the last
    /// thing that acted on the screen. `input_epoch` turns over on exactly that (driven
    /// input, window activate), so pairing it with `epoch` makes a stale click impossible
    /// while leaving the read-only questions as cheap as before.
    input_epoch: u64,
    /// (hwnd, names, control types) → 1-based index of the matching name, or None.
    uia_any: HashMap<(isize, Vec<String>, Vec<i32>), Option<usize>>,
    /// (hwnd, name, control type) → present?
    element_find: HashMap<(isize, String, i32), bool>,
    /// hwnd → its child controls. Rc so handing it out does not copy the vector.
    controls: HashMap<isize, Rc<Vec<backend::ControlInfo>>>,
    /// The foreground window. Outer Option = not asked yet; inner = there isn't one.
    /// Cheap-looking and not: it reads the title and class, then OPENS THE PROCESS to
    /// read its image name, and every embedded overlay asks for it on every recheck.
    active: Option<Option<backend::WinInfo>>,
    /// The focused control up to its top-level window — a walk with a class read per
    /// level, likewise asked once per overlay per recheck.
    focus_chain: Option<Rc<Vec<backend::ControlInfo>>>,
    served: u32,
    asked: u32,
    /// UIA element LOOKUPS: "where is the element called X?", keyed by a rendering of the
    /// arguments. These are the expensive ones — element_plugin_locate walks the RAW tree, the
    /// only view that reaches into a DAW-embedded plugin's hosted fragment — and one of them
    /// sits in `isRackView`, which every Kontakt cell consults on every recheck.
    points: HashMap<String, Option<(i32, i32)>>,
    /// Screen pixel reads this epoch, and what they cost. NOT cached — a pixel is the one
    /// observation whose whole purpose can be to change between two reads — but counted,
    /// because each is a compositor frame and they were invisible.
    pixels: u32,
    pixel_us: u128,
    /// Total time spent INSIDE these bindings this epoch, cache hits included. The
    /// backend call is only half of what one costs: every hit still rebuilds the answer
    /// as fresh Lua tables, once per calling overlay, in each of nine VMs.
    ///
    /// In MICROseconds, and that is the whole point. Accumulated in whole milliseconds this
    /// **It is not "inside the bindings", though the line used to say so.** It is incremented
    /// at exactly two sites — `window.controls` and `window.focusChain` — and at each it times
    /// only the Lua-table rebuild, not the backend call beside it. The `asked` counter printed
    /// in the same sentence spans SIX families, `window.active` among them, and that is the
    /// one binding with a stall line of its own. A numerator over two and a denominator over
    /// six read as a price for everything just counted. The line names what it measures now.
    ///
    /// read 0 for an epoch of 1858 calls — because each individual conversion rounds down to
    /// zero, and eighteen hundred zeroes are still zero. The reading was not evidence that
    /// the conversions are free; it was a unit too coarse to see them, and it retired a
    /// correct hypothesis. Sub-millisecond costs need a sub-millisecond clock.
    binding_us: u128,
}

impl Shared {
    fn root(&self, idx: usize) -> PathBuf {
        self.roots.borrow()[idx].clone()
    }

    /// The observation cache, emptied whenever the epoch has turned over.
    ///
    /// Reports the epoch it is discarding when that epoch did enough work to be worth
    /// knowing about — the ratio is the whole claim of this cache, and a claim about
    /// performance that nobody can check is just an assertion.
    ///
    /// "Enough work" was twenty questions in total, and that let the idle tick through:
    /// every bound overlay asks `window.active` once per 500 ms poll, one of those reaches
    /// the OS and the rest are served, so with 43 overlays bound the line read `served 42
    /// of 43 OS question(s) from cache (1 actually asked), 0.0 ms rebuilding Lua tables …,
    /// plus 0 screen pixel read(s) costing 0.0 ms` twice a second — 463 lines of the
    /// tester's last log, each saying the cache works and nothing else. On an idle tick
    /// the total is just the overlay count, a number the arbiter roster already prints,
    /// so the total no longer decides. What earns an epoch a line is the OS actually
    /// being interrogated (two or more distinct questions got past the cache), a rebuild
    /// cost that a whole-millisecond clock can see, or any pixel read — each one a
    /// compositor frame. The total keeps only a sanity bound: one question per overlay is
    /// what an idle tick costs, and 251 was the most seen with every library loaded, so a
    /// thousand in one epoch is somebody asking in a loop, whatever the cache made of it.
    fn observations(&self) -> std::cell::RefMut<'_, Observations> {
        let now = self.epoch.get();
        let now_input = self.input_epoch.get();
        let mut obs = self.observations.borrow_mut();
        if obs.epoch != now || obs.input_epoch != now_input {
            let reached_os = obs.asked - obs.served;
            if reached_os >= 2 || obs.binding_us >= 1000 || obs.pixels > 0 || obs.asked >= 1000 {
                logging::line(
                    "observe",
                    &format!(
                        "epoch served {} of {} OS question(s) from cache ({} actually \
                         asked), {:.1} ms rebuilding Lua tables in window.controls and \
                         window.focusChain, plus {} screen pixel read(s) costing {:.1} ms",
                        obs.served,
                        obs.asked,
                        obs.asked - obs.served,
                        obs.binding_us as f64 / 1000.0,
                        obs.pixels,
                        obs.pixel_us as f64 / 1000.0
                    ),
                );
            }
            obs.epoch = now;
            obs.input_epoch = now_input;
            obs.points.clear();
            obs.uia_any.clear();
            obs.element_find.clear();
            obs.controls.clear();
            obs.active = None;
            obs.focus_chain = None;
            obs.served = 0;
            obs.asked = 0;
            obs.binding_us = 0;
            obs.pixels = 0;
            obs.pixel_us = 0;
        }
        obs
    }

    /// Marks the world as possibly changed (see the `epoch` field).
    fn bump_epoch(&self) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
    }

    /// Marks the SCREEN as possibly changed — something was driven (a click, a keystroke
    /// we sent, a drag) or a window came forward. Distinct from `epoch`, which also turns
    /// over on a timer tick and on the user's own keys.
    ///
    /// The difference is worth a counter because reading the screen costs a fixed
    /// compositor frame, ~17-25 ms, and a lot of what gets read is a property that only
    /// something ACTING can change. Kontakt's classic-vs-play view is the case that
    /// prompted this: it gates which header controls exist, so it is consulted on every
    /// focus move, and against `epoch` that meant a fresh pixel read on every Tab —
    /// measured at 21-38 ms of the ~70 ms step. Pressing Tab cannot change which view
    /// Kontakt is in. Sending F10 can, and that goes through host.input, which bumps this.
    fn bump_input_epoch(&self) {
        self.input_epoch.set(self.input_epoch.get().wrapping_add(1));
        self.bump_epoch();
    }

    fn alloc_id(&self) -> i32 {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        id
    }

    /// Decodes a template PNG to RGBA, cached by path + mtime. Repeated searches of the
    /// same template (the landmark poll, a toggle's on/off pair) reuse the decoded copy;
    /// a re-captured template (changed mtime) is re-decoded, so calibration takes effect
    /// without a restart. Returns a cheap `Arc` clone.
    fn load_template(&self, path: &Path) -> mlua::Result<Arc<Decoded>> {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        // Cache hit (same path, unchanged mtime): bump the LRU stamp and return.
        if let Some(mt) = mtime {
            let mut cache = self.template_cache.borrow_mut();
            if let Some(entry) = cache.get_mut(path) {
                if entry.0 == mt {
                    let seq = self.template_seq.get() + 1;
                    self.template_seq.set(seq);
                    entry.2 = seq;
                    return Ok(entry.1.clone());
                }
            }
        }
        // Miss, or the file changed on disk: (re)decode.
        let img = image::open(path)
            .map_err(|e| {
                mlua::Error::external(format!("cannot open template '{}': {e}", path.display()))
            })?
            .to_rgba8();
        let (dw, dh) = (img.width(), img.height());
        let raw = img.into_raw();
        let probes = probe_order(dw, dh, &raw);
        let dec = Arc::new(Decoded { w: dw, h: dh, rgba: raw, probes });
        if let Some(mt) = mtime {
            let seq = self.template_seq.get() + 1;
            self.template_seq.set(seq);
            let mut cache = self.template_cache.borrow_mut();
            cache.insert(path.to_path_buf(), (mt, dec.clone(), seq));
            // Growth cap: drop the least-recently-used entry if over capacity.
            if cache.len() > TEMPLATE_CACHE_CAP {
                let lru = cache.iter().min_by_key(|(_, v)| v.2).map(|(k, _)| k.clone());
                if let Some(k) = lru {
                    cache.remove(&k);
                }
            }
        }
        Ok(dec)
    }

    /// Tells the user something ON THE APPLICATION'S OWN BEHALF — not a module's.
    ///
    /// Shown rather than spoken, because a notification serves everybody: a screen reader
    /// reads it out, and somebody who is not using one can see it. That is one channel for
    /// two kinds of user, instead of a voice that talks at people who did not ask for one.
    ///
    /// Speech is only the way out where there is nothing to show it in — macOS has no
    /// balloon — and even then only if a screen reader is actually listening. With none
    /// running, the application says nothing at all, which is the right amount for an
    /// audience that is not there.
    ///
    /// Module speech is not routed through here. `host.speech` always speaks: whoever
    /// installed and enabled that module decided so.
    fn announce(&self, text: &str) {
        if let Some(show) = self.notify.borrow().as_ref() {
            if show("Automation Platform", text) {
                // Logged here too, and not only on the speech path below. An announcement has
                // three possible ends — shown, spoken, or deliberately dropped — and a log
                // that only records one of them cannot answer "what happened to it", which is
                // the question this instrumentation exists for.
                //
                // The TEXT, not its length. A length can only be compared against a guess at
                // what was probably said; the sentence itself is what tells a remote reader
                // which of several announcements this was, and there are a handful per
                // session rather than a stream.
                logging::line(
                    "speech",
                    &format!("announcement, shown as a notification: {text}"),
                );
                return;
            }
        }
        // Asked about PRESENCE, not about the transport. The two were the same call until
        // 2026-09-04, and the difference is what the macOS tester reported: he heard no
        // startup announcement at all, because "Speak through VoiceOver" is off by default
        // and this gate required it — while VoiceOver itself was running the whole time and
        // there was no balloon on that platform to show him anything instead. The line goes
        // out through whatever `say` chooses; the question here is only whether anybody is
        // there to hear it.
        // Logged either way, and this is the whole point of the line. The tester reported a
        // launch on which nothing was said and the log had nothing to say about it — silence
        // that was correct (nobody listening) and silence that was a fault looked the same
        // from here, and only one of them is worth investigating. One line per announcement,
        // and there are a handful per session.
        let present = self.speech.a_reader_is_present();
        logging::line(
            "speech",
            &format!(
                "announcement, {}: {text}",
                if present {
                    "handed to speech because a screen reader is running"
                } else {
                    "NOT said because no screen reader is running — this application does not \
                     speak at somebody who never asked it to"
                }
            ),
        );
        if present {
            self.speech.say(text, false);
        }
    }

    /// Logs + queues an accessible dialog `(title, message)`, deduped on
    /// `dedup_key` (so a repeating issue surfaces once per enabled session).
    fn queue_dialog(&self, dedup_key: String, title: String, message: String) {
        if self.error_seen.borrow_mut().insert(dedup_key) {
            self.errors.borrow_mut().push((title, message));
        }
    }

    /// Logs a module callback failure (a Lua error or a caught Rust panic) and
    /// queues it — deduped — for the GUI to surface in an accessible error dialog.
    fn report_callback_error(&self, module_idx: usize, context: &str, message: &str) {
        let id = self.ids.borrow().get(module_idx).cloned().unwrap_or_else(|| "?".into());
        logging::line(context, &format!("[{id}] {message}"));
        self.queue_dialog(
            format!("{id}\u{1}{context}"),
            format!("Module error: {id}"),
            format!("The module \u{201c}{id}\u{201d} hit an error in a {context} callback:\n\n{message}"),
        );
    }

    /// Surfaces a cross-module hotkey clash: logs it + queues an accessible dialog naming
    /// both modules. The module that loaded first keeps the combination; the other stays
    /// loaded with a standing claim that `refresh_hotkeys` honours the moment the holder
    /// gives it up.
    ///
    /// Hotkeys only, despite the generic `kind` parameter — captured keys do not contend for
    /// exclusive ownership (the hook suppresses a key for the process and every enabled
    /// module that captured it is dispatched to), so there is nothing to report there. The
    /// doc used to say "hotkey or captured key" and named a caller that does not exist.
    fn report_conflict(&self, idx: usize, owner_idx: usize, kind: &str, spec: &str) {
        let (me, owner) = {
            let ids = self.ids.borrow();
            (
                ids.get(idx).cloned().unwrap_or_default(),
                ids.get(owner_idx).cloned().unwrap_or_default(),
            )
        };
        logging::line("conflict", &format!("[{me}] {kind} '{spec}' conflicts with [{owner}]"));
        self.queue_dialog(
            // The OWNER is part of the key. Without it, three modules wanting one combination
            // produced one message per loser naming the first holder, and then the corrected
            // message — naming whoever took over after the user disabled that holder — had
            // the same key and was silently dropped. The user would have been left having
            // carried out an instruction that was never retracted.
            format!("{me}\u{1}conflict\u{1}{kind}\u{1}{spec}\u{1}{owner}"),
            "Binding conflict".to_string(),
            // States who holds it and what makes it move, and promises nothing about WHO it
            // moves to. The old wording said disabling the holder passed the key to the
            // reader "straight away", which is true only when there are exactly two
            // claimants: with three, disabling the named holder hands it to the next module
            // in load order, not to the one reading the message. It also said "tried to
            // bind", which stopped being true once this report could name a module that had
            // been holding the key and was asked to give it up.
            format!(
                "Module \u{201c}{me}\u{201d} wants the {kind} {spec}, and module \u{201c}{owner}\u{201d} is using it. Only one module can hold a combination at a time. {spec} passes on by itself as soon as \u{201c}{owner}\u{201d} releases it \u{2014} disabling or removing that module in the module manager is enough, and nothing needs restarting."
            ),
        );
    }

    /// Surfaces a hotkey the OS itself rejected — held by another application, not
    /// one of our modules (or an unparseable spec). The module stays loaded.
    fn report_os_conflict(&self, idx: usize, kind: &str, spec: &str, err: &str) {
        let me = self.ids.borrow().get(idx).cloned().unwrap_or_default();
        logging::line("conflict", &format!("[{me}] {kind} '{spec}' rejected by OS: {err}"));
        self.queue_dialog(
            format!("{me}\u{1}osconflict\u{1}{kind}\u{1}{spec}"),
            "Binding unavailable".to_string(),
            format!(
                "The {kind} {spec} for module \u{201c}{me}\u{201d} couldn\u{2019}t be registered \u{2014} another application already uses it system-wide."
            ),
        );
    }

    /// Takes the queued module errors for the GUI to display (called each tick).
    fn drain_errors(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.errors.borrow_mut())
    }
}

/// Which registration holds each combination: **whoever holds it already keeps it**, and
/// otherwise the earliest claim among enabled modules — load order first, registration order
/// second.
///
/// Lifted out of `refresh_hotkeys` so the RULE can be tested without an OS to register
/// against — the plumbing around it is untestable here, and the rule is the part that was
/// wrong.
///
/// Ordering by id alone was the obvious reading of "first-come", and it does not survive a
/// reload. Ids are monotonic and never reused (`alloc_id`), so a reloaded module registers
/// again with a HIGHER id than any standing claim on its combination — and would hand its own
/// key to whoever had been waiting, permanently, for the rest of the session. Reloading a
/// module would silently cost it its hotkey. Keeping the refresh out of `purge_module` closed
/// the window during the rebuild but not the outcome, because the outcome is decided here.
///
/// A module's index survives a reload (the VM is swapped in at the same index), so ordering by
/// `(module_idx, id)` is stable across one. At start-up the two readings agree — module 0
/// loads and registers before module 1 — so this changes nothing for the ordinary case and
/// only settles the cases where they disagree.
///
/// **The holder is deliberately not remembered across a restart**, and that was decided
/// rather than overlooked. Re-enabling a module does not take a combination back from
/// whoever has been using it — which is right — but at start-up nobody holds anything, so
/// the order below decides again and the earlier module has it. Persisting the holder would
/// have turned enabling and disabling into a durable way to say who owns a shared key; the
/// answer was that it solves the wrong problem, because the problem is two modules wanting
/// one combination at all, and a user settles that by turning off what they do not need.
/// The thing worth building instead is a remap — see TODO.md.
///
/// Order alone is still not enough, and the case that shows why is not hypothetical.
/// Overlays register their control hotkeys when they activate and release them when they
/// deactivate, and `Alt+B` is a control in more than one of the shipped overlays. Switching
/// between two plug-ins can therefore have the arriving overlay register while the leaving one
/// is still holding — and pure order would let the lower-indexed module take a LIVE key away
/// from the module that is using it, mid-gesture, on the way out. So a live claim wins: taking
/// a key off a module that is holding it needs a reason better than "I load earlier", and the
/// reasons that count (the holder is disabled, unregisters, or goes away) all remove the claim
/// and let the order decide again.
///
/// A registration whose spec did not parse has no `(vk, mask)` and so cannot be compared with
/// anybody else's; it is left out and answered by the OS alone.
fn hotkey_winners(
    regs: impl Iterator<Item = (i32, Option<(u32, u8)>, usize, bool)>,
    enabled: impl Fn(usize) -> bool,
) -> HashMap<(u32, u8), i32> {
    // Sorted so `false` (a live claim) comes before `true`: the incumbent outranks the order,
    // and the order decides among everybody else. At most one claim per combination is live.
    let mut best: HashMap<(u32, u8), (bool, usize, i32)> = HashMap::new();
    for (id, binding, module_idx, live) in regs {
        if !enabled(module_idx) {
            continue;
        }
        let Some(b) = binding else { continue };
        let rank = (!live, module_idx, id);
        best.entry(b).and_modify(|cur| *cur = (*cur).min(rank)).or_insert(rank);
    }
    best.into_iter().map(|(b, (_, _, id))| (b, id)).collect()
}

impl Shared {
    /// Recomputes which registrations hold their combination at the OS, from *enabled*
    /// modules — the counterpart to `refresh_captured` below, and needed for the same reason.
    ///
    /// Conflicts used to be resolved once, at registration, and never revisited. Worse: a
    /// registration that lost the contest was not even recorded — `host.hotkey.register`
    /// returned early, before the insert, so the callback was dropped and the module got the
    /// id `0`. There was therefore nothing to revive, and disabling the module that held the
    /// combination did not hand the key over; it needed a restart. That is what the TODO
    /// entry meant by "resolved at registration only", and it was the deeper half of it.
    ///
    /// Now every registration is recorded and liveness is DERIVED here: for each combination,
    /// the earliest registration among enabled modules holds it. Ids are handed out in order,
    /// so the lowest id is first-come — the rule the conflict message already promises. Stale
    /// state cannot accumulate, because there is no stored decision to go stale.
    ///
    /// Release before claim, in two passes. The OS refuses a combination that is already
    /// held, and the previous holder is usually one of ours: claiming first would fail on our
    /// own registration and leave the key belonging to nobody.
    fn refresh_hotkeys(&self) {
        // Who should hold what.
        let want = {
            let map = self.hotkeys.borrow();
            let enabled = self.enabled.borrow();
            hotkey_winners(
                map.iter().map(|(id, reg)| (*id, reg.binding, reg.module_idx, reg.live)),
                |idx| enabled.get(idx).copied().unwrap_or(false),
            )
        };

        // What has to change, decided before anything is touched so no borrow is held across
        // an OS call or a report.
        let (mut release, mut claim, mut losers) = (Vec::new(), Vec::new(), Vec::new());
        {
            let map = self.hotkeys.borrow();
            let enabled = self.enabled.borrow();
            for (id, reg) in map.iter() {
                let winner = reg.binding.and_then(|b| want.get(&b).copied());
                let should = winner == Some(*id);
                if reg.live && !should {
                    release.push(*id);
                }
                if !reg.live && should {
                    claim.push((*id, reg.spec.clone(), reg.module_idx));
                }
                // An enabled module whose combination belongs to somebody else.
                //
                // Only reported when the winner ACTUALLY HOLDS it. `want` is computed from
                // claims, not from what the OS granted, so a winner whose own registration
                // another application refused is still the winner on paper — and telling
                // somebody "module A already uses it" when nobody does sends them to disable
                // A for nothing. A is separately told the truth by `report_os_conflict`.
                if !should
                    && enabled.get(reg.module_idx).copied().unwrap_or(false)
                    && reg.binding.is_some()
                {
                    if let Some(owner) = winner.and_then(|w| map.get(&w)) {
                        if owner.module_idx != reg.module_idx && owner.live {
                            losers.push((reg.module_idx, owner.module_idx, reg.spec.clone()));
                        }
                    }
                }
            }
        }

        for id in release {
            self.backend.unregister_hotkey(id);
            if let Some(reg) = self.hotkeys.borrow_mut().get_mut(&id) {
                reg.live = false;
            }
        }
        for (id, spec, idx) in claim {
            match self.backend.register_hotkey(id, &spec) {
                Ok(()) => {
                    if let Some(reg) = self.hotkeys.borrow_mut().get_mut(&id) {
                        reg.live = true;
                    }
                    let me = self.ids.borrow().get(idx).cloned().unwrap_or_default();
                    logging::line("keys", &format!("hotkey '{spec}' is now held by [{me}]"));
                }
                // Held by another application rather than by us. Left not-live, so a later
                // refresh tries again — which is the one thing a restart used to be for.
                Err(e) => self.report_os_conflict(idx, "hotkey", &spec, &e),
            }
        }
        for (idx, owner, spec) in losers {
            self.report_conflict(idx, owner, "hotkey", &spec);
        }
    }

    /// Recomputes the global captured-key set from *enabled* modules and updates
    /// the hook (so a disabled module's keys are no longer suppressed).
    fn refresh_captured(&self) {
        let enabled = self.enabled.borrow();
        let mut set: Vec<(u32, u8)> = self
            .keys
            .borrow()
            .iter()
            .filter(|(_, _, idx, ..)| enabled.get(*idx).copied().unwrap_or(false))
            .map(|(vk, m, ..)| (*vk, *m))
            .collect();
        set.sort_unstable();
        set.dedup();
        // Under TRACE, not only under calibration. The trace switch promises in its own help
        // text to record "every decision the key handling made", and this is the decision:
        // which keys the application will not see. It sat behind the calibration switch, which
        // is about measuring coordinates inside an overlay and needs a module reload to arm —
        // so the one line that answers "who is holding my arrow keys" was unavailable to the
        // person asking. An evening went into guessing at it instead.
        //
        // WHO holds each key, not just which: a key is suppressed for the whole process while
        // any enabled module captures it, so the module names are the actionable half.
        if appcfg::trace() || appcfg::calibrate() {
            let ids = self.ids.borrow();
            let keys = self.keys.borrow();
            logging::line(
                "keys",
                &format!(
                    "captured set: {}",
                    set.iter()
                        .map(|(vk, m)| {
                            let mut owners: Vec<&str> = keys
                                .iter()
                                .filter(|(k, mm, idx, ..)| {
                                    *k == *vk
                                        && *mm == *m
                                        && enabled.get(*idx).copied().unwrap_or(false)
                                })
                                .map(|(_, _, idx, ..)| {
                                    ids.get(*idx).map(String::as_str).unwrap_or("?")
                                })
                                .collect();
                            owners.dedup();
                            format!("vk 0x{vk:02X}/m{m}[{}]", owners.join(","))
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            );
        }
        self.backend.set_captured_keys(&set);
    }

    /// Removes every registration owned by module `idx` (hotkeys + their OS
    /// registration, captured keys, settings `onChange`, timers, arbiter claims,
    /// data export) WITHOUT touching the parallel vectors — for an in-place reload
    /// that rebuilds the same index. Unlike `rollback_to` (a suffix truncation) this
    /// targets a single module and leaves its roots/ids/enabled/schemas slots.
    fn purge_module(&self, idx: usize) {
        let stale: Vec<i32> = self
            .hotkeys
            .borrow()
            .iter()
            .filter(|(_, reg)| reg.module_idx == idx)
            .map(|(id, _)| *id)
            .collect();
        {
            let mut hk = self.hotkeys.borrow_mut();
            for id in stale {
                self.backend.unregister_hotkey(id);
                hk.remove(&id);
            }
        }
        self.keys.borrow_mut().retain(|(_, _, i, ..)| *i != idx);
        self.on_change.borrow_mut().retain(|(i, _), _| *i != idx);
        self.timers.borrow_mut().retain(|(_, i, ..)| *i != idx);
        self.recurring.borrow_mut().retain(|(_, _, i, ..)| *i != idx);
        let mut to_resolve: Vec<String> = Vec::new();
        let mut deactivations: Vec<Function> = Vec::new();
        {
            let mut map = self.arbiter.borrow_mut();
            for (slot, s) in map.iter_mut() {
                // If this module owns the slot's ACTIVE claim, run its onDeactivate
                // before dropping it — so an active overlay tears down its global
                // side effects (key scope / menu-open) exactly once, mirroring
                // arbiter_unregister. (Otherwise a reload of an active overlay would
                // strand those process-global flags until the next activation.)
                if let Some(active) = s.active {
                    if let Some(c) =
                        s.claims.iter().find(|c| c.handle == active && c.module_idx == idx)
                    {
                        if let Ok(f) = c.lua.registry_value::<Function>(&c.on_deactivate) {
                            deactivations.push(f);
                        }
                    }
                }
                s.claims.retain(|c| c.module_idx != idx);
                if s.active.is_some_and(|a| !s.claims.iter().any(|c| c.handle == a)) {
                    s.active = None;
                    to_resolve.push(slot.clone());
                }
            }
            map.retain(|_, s| !s.claims.is_empty());
        }
        for f in deactivations {
            if let Err(e) = call_guarded(&f, ()) {
                logging::line("arbiter", &format!("onDeactivate error during purge: {e}"));
            }
        }
        for slot in to_resolve {
            self.arbiter_resolve(&slot);
        }
        let id = self.ids.borrow().get(idx).cloned();
        if let Some(id) = id {
            self.exports.borrow_mut().remove(&id);
        }
        // Deliberately NOT `refresh_hotkeys()` here, and this is the subtle one — it was
        // written that way first and it was wrong. Both callers of this function are the
        // RELOAD path: it purges, rebuilds the VM, and the fresh VM re-registers. A refresh
        // in between would hand the module's own combination to whoever else had a standing
        // claim on it, and since the rebuilt module registers again with a HIGHER id it
        // would never get it back — reloading a module would silently cost it its hotkey.
        // The refresh belongs at the end of the operation: the fresh VM's own
        // `host.hotkey.register` does it on success, and `reload_module` does it on failure,
        // where the module really is gone.
        self.refresh_captured();
    }

    /// Enables or disables a module at runtime: (un)registers its OS hotkeys and
    /// recomputes the captured-key set. The dispatcher already skips disabled
    /// modules' hotkeys/keys/triggers via the `enabled` flag.
    fn apply_enabled(&self, idx: usize, enabled: bool) -> bool {
        {
            let mut en = self.enabled.borrow_mut();
            if idx >= en.len() || en[idx] == enabled {
                return false;
            }
            en[idx] = enabled;
        }
        // Fresh error-dialog slate on toggle: drop this module's dedup keys so a
        // still-present fault re-surfaces (rather than staying log-only) next run.
        if let Some(id) = self.ids.borrow().get(idx).cloned() {
            let prefix = format!("{id}\u{1}");
            self.error_seen.borrow_mut().retain(|k| !k.starts_with(&prefix));
        }
        // Derived, not toggled. This loop used to register or unregister only THIS module's
        // hotkeys, discarding the result (`let _ =`) — so enabling a module whose combination
        // another one held failed silently, and disabling the holder left the waiting module
        // waiting. Both are the same missing step: the set of live registrations is a function
        // of the enabled set, so it is recomputed rather than nudged.
        self.refresh_hotkeys();
        self.refresh_captured();
        // A disabled module must not stay the active overlay (and an enabled one
        // may now win): re-elect every arbiter slot it participates in.
        let slots: Vec<String> = self
            .arbiter
            .borrow()
            .iter()
            .filter(|(_, s)| s.claims.iter().any(|c| c.module_idx == idx))
            .map(|(k, _)| k.clone())
            .collect();
        for slot in slots {
            self.arbiter_resolve(&slot);
        }
        logging::line(
            "manager",
            &format!("module {idx} {}", if enabled { "enabled" } else { "disabled" }),
        );
        true
    }

    /// Rolls back every module-indexed collection to `n` modules: truncates the
    /// four parallel vectors and drops any hotkeys/keys/timers/on-change/exports
    /// the partial load registered (unregistering OS hotkeys). Used when a
    /// runtime hot-load fails part-way so the shared state stays aligned with
    /// `modules` (at startup a failed load aborts the process, so this only
    /// matters for hot-loading into a running app).
    fn rollback_to(&self, n: usize, failed_id: &str) {
        let stale: Vec<i32> = self
            .hotkeys
            .borrow()
            .iter()
            .filter(|(_, reg)| reg.module_idx >= n)
            .map(|(id, _)| *id)
            .collect();
        {
            let mut hk = self.hotkeys.borrow_mut();
            for id in stale {
                self.backend.unregister_hotkey(id);
                hk.remove(&id);
            }
        }
        self.keys.borrow_mut().retain(|(_, _, idx, ..)| *idx < n);
        self.on_change.borrow_mut().retain(|(idx, _), _| *idx < n);
        self.timers.borrow_mut().retain(|(_, idx, ..)| *idx < n);
        self.recurring.borrow_mut().retain(|(_, _, idx, ..)| *idx < n);
        {
            // Drop arbiter claims owned by the rolled-back modules; clear a now-
            // dangling active winner and re-elect that slot, so a surviving lower-
            // specificity matcher (e.g. a base overlay suppressed under the dropped
            // one) takes over — set_matching won't, its match state is unchanged.
            let mut to_resolve: Vec<String> = Vec::new();
            {
                let mut map = self.arbiter.borrow_mut();
                for (slot, s) in map.iter_mut() {
                    s.claims.retain(|c| c.module_idx < n);
                    if s.active.is_some_and(|a| !s.claims.iter().any(|c| c.handle == a)) {
                        s.active = None;
                        to_resolve.push(slot.clone());
                    }
                }
                map.retain(|_, s| !s.claims.is_empty());
            }
            for slot in to_resolve {
                self.arbiter_resolve(&slot);
            }
        }
        self.roots.borrow_mut().truncate(n);
        self.ids.borrow_mut().truncate(n);
        self.enabled.borrow_mut().truncate(n);
        self.caps.borrow_mut().truncate(n);
        self.schemas.borrow_mut().truncate(n);
        self.exports.borrow_mut().remove(failed_id);
        self.refresh_hotkeys();
        self.refresh_captured();
    }

    /// Registers an overlay claim on `slot` and returns its handle. The claim is
    /// initially not matching; the owner reports matches via `arbiter_set_matching`.
    fn arbiter_register(
        &self,
        slot: String,
        module_idx: usize,
        specificity: i64,
        lua: Lua,
        on_activate: RegistryKey,
        on_deactivate: RegistryKey,
    ) -> i64 {
        let handle = self.next_arbiter.get() + 1;
        self.next_arbiter.set(handle);
        self.arbiter.borrow_mut().entry(slot).or_default().claims.push(ArbiterClaim {
            handle,
            module_idx,
            specificity,
            lua,
            on_activate,
            on_deactivate,
            matching: false,
        });
        handle
    }

    /// Reports whether claim `handle` currently matches in `slot`; re-elects the
    /// slot winner and drives onDeactivate/onActivate on a change.
    fn arbiter_set_matching(&self, slot: &str, handle: i64, matching: bool) {
        {
            let mut map = self.arbiter.borrow_mut();
            let Some(s) = map.get_mut(slot) else {
                logging::line("arbiter", &format!("setMatching: unknown slot '{slot}'"));
                return;
            };
            let Some(c) = s.claims.iter_mut().find(|c| c.handle == handle) else {
                logging::line(
                    "arbiter",
                    &format!("setMatching: unknown handle {handle} in slot '{slot}'"),
                );
                return;
            };
            if c.matching == matching {
                return;
            }
            c.matching = matching;
        }
        self.arbiter_resolve(slot);
    }

    /// Removes claim `handle` from `slot` and re-elects (promoting the next winner
    /// if the removed claim was active).
    fn arbiter_unregister(&self, slot: &str, handle: i64) {
        let (removed, was_active) = {
            let mut map = self.arbiter.borrow_mut();
            let Some(s) = map.get_mut(slot) else { return };
            let was_active = s.active == Some(handle);
            let removed = s
                .claims
                .iter()
                .position(|c| c.handle == handle)
                .map(|p| s.claims.remove(p));
            if was_active {
                s.active = None;
            }
            (removed, was_active)
        };
        if let Some(c) = removed {
            // If it was the active winner, run its onDeactivate so the overlay
            // tears down its activation (idempotent — _deactivate guards on its
            // own state) before the callbacks are freed; arbiter_resolve below
            // then promotes the next matcher.
            if was_active {
                if let Ok(f) = c.lua.registry_value::<Function>(&c.on_deactivate) {
                    if let Err(e) = call_guarded(&f, ()) {
                        logging::line("arbiter", &format!("onDeactivate error: {e}"));
                    }
                }
            }
            // Free the callback registry values — an overlay can tear down a
            // single claim while its VM lives on, so they would otherwise linger.
            let _ = c.lua.remove_registry_value(c.on_activate);
            let _ = c.lua.remove_registry_value(c.on_deactivate);
        }
        self.arbiter_resolve(slot);
    }

    /// The handle of the slot's currently-active claim, if any. Lets an overlay
    /// (or a debugger) confirm whether it is the active winner rather than trust a
    /// locally-mirrored flag.
    fn arbiter_winner(&self, slot: &str) -> Option<i64> {
        self.arbiter.borrow().get(slot).and_then(|s| s.active)
    }

    /// The SPECIFICITY of the slot's active claim, if any. An overlay uses this to find
    /// out whether something at least as specific as itself already owns the slot — which,
    /// where only one claimant can be right at a time (one sample library loaded in one
    /// plugin), makes its own detection work provably pointless until that changes.
    fn arbiter_winner_specificity(&self, slot: &str) -> Option<i64> {
        let map = self.arbiter.borrow();
        let s = map.get(slot)?;
        let active = s.active?;
        s.claims.iter().find(|c| c.handle == active).map(|c| c.specificity)
    }

    /// Elects the winner of `slot` — the matching claim of highest specificity
    /// owned by an enabled module — and, if it changed, deactivates the previous
    /// winner then activates the new one.
    ///
    /// Lua callbacks fire outside the arbiter borrow and may call back into the
    /// arbiter for this slot (an overlay re-evaluating its own match inside
    /// onActivate, a library poll, etc.). Rather than recurse — which would
    /// commit `active` then run a stale activation — a re-entrant call just marks
    /// the slot dirty, and this loop re-resolves to a fixpoint, so transitions
    /// fire in order and each onActivate pairs with exactly one later
    /// onDeactivate. Ties on specificity break on the (monotonic) handle, so the
    /// winner is deterministic and unaffected by claim insertion/removal order.
    fn arbiter_resolve(&self, slot: &str) {
        {
            let mut map = self.arbiter.borrow_mut();
            let Some(s) = map.get_mut(slot) else { return };
            if s.resolving {
                s.dirty = true;
                return;
            }
            s.resolving = true;
            s.dirty = true;
        }
        // Bounded so a pathological oscillating callback is logged rather than
        // hanging the event loop; sane overlays settle in one or two passes.
        for _ in 0..100 {
            let mut deactivate: Option<Function> = None;
            let mut activate: Option<Function> = None;
            {
                let mut map = self.arbiter.borrow_mut();
                let Some(s) = map.get_mut(slot) else { return };
                if !s.dirty {
                    s.resolving = false;
                    return;
                }
                s.dirty = false;
                let winner = {
                    let enabled = self.enabled.borrow();
                    s.claims
                        .iter()
                        .filter(|c| {
                            c.matching && enabled.get(c.module_idx).copied().unwrap_or(false)
                        })
                        .max_by_key(|c| (c.specificity, c.handle))
                        .map(|c| c.handle)
                };
                if winner == s.active {
                    s.resolving = false;
                    return;
                }
                if let Some(old) = s.active {
                    if let Some(c) = s.claims.iter().find(|c| c.handle == old) {
                        deactivate = c.lua.registry_value::<Function>(&c.on_deactivate).ok();
                    }
                }
                if let Some(neu) = winner {
                    if let Some(c) = s.claims.iter().find(|c| c.handle == neu) {
                        activate = c.lua.registry_value::<Function>(&c.on_activate).ok();
                    }
                }
                s.active = winner;
            }
            if let Some(f) = deactivate {
                if let Err(e) = call_guarded(&f, ()) {
                    logging::line("arbiter", &format!("onDeactivate error: {e}"));
                }
            }
            if let Some(f) = activate {
                if let Err(e) = call_guarded(&f, ()) {
                    logging::line("arbiter", &format!("onActivate error: {e}"));
                }
            }
        }
        {
            let mut map = self.arbiter.borrow_mut();
            if let Some(s) = map.get_mut(slot) {
                s.resolving = false;
                s.dirty = false;
            }
        }
        logging::line(
            "arbiter",
            &format!("slot '{slot}' did not settle in 100 passes (callback oscillation?)"),
        );
    }

    /// As [`Self::apply_enabled`], persisting the new disabled-set to the
    /// portable config. Used by the GUI toggle.
    fn set_enabled(&self, idx: usize, enabled: bool) {
        if self.apply_enabled(idx, enabled) {
            self.save_config();
        }
    }

    /// Records the enabled flag for a module that is not loaded here.
    ///
    /// There is no index and no VM to revoke or restore — the module was skipped because its
    /// manifest says it does not run on this platform. The flag is stored by id like every
    /// other, so ticking the box says "I want this on" and that answer is waiting wherever the
    /// module does load.
    fn set_enabled_unloaded(&self, id: &str, enabled: bool) {
        self.store.borrow_mut().set_enabled(id, enabled);
        self.save_config();
        logging::line(
            "manager",
            &format!(
                "'{id}' is not loaded here, so {} is remembered for wherever it is",
                if enabled { "enabled" } else { "disabled" }
            ),
        );
    }

    /// Mirrors the in-memory `enabled[]` flags into the store.
    fn sync_enabled_into_store(&self) {
        let ids = self.ids.borrow();
        let enabled = self.enabled.borrow();
        let mut store = self.store.borrow_mut();
        for (i, id) in ids.iter().enumerate() {
            store.set_enabled(id, enabled.get(i).copied().unwrap_or(true));
        }
    }

    /// Persists the store immediately (enable/disable is a deliberate action).
    fn save_config(&self) {
        self.sync_enabled_into_store();
        self.store.borrow().save();
    }

    /// Flushes pending setting changes to disk (coalesced; driven by the loop).
    fn flush_if_dirty(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        self.sync_enabled_into_store();
        self.store.borrow().save();
    }

    /// Fires one-shot timers whose deadline has passed (driven by the loop tick).
    fn fire_due_timers(&self) {
        let now = Instant::now();
        let mut due: Vec<(usize, Lua, RegistryKey)> = Vec::new();
        {
            let mut timers = self.timers.borrow_mut();
            let mut i = 0;
            while i < timers.len() {
                if timers[i].0 <= now {
                    let (_, idx, lua, cb) = timers.remove(i);
                    due.push((idx, lua, cb));
                } else {
                    i += 1;
                }
            }
        }
        // Only once something actually fires — this runs on EVERY loop tick, and an idle
        // tick has changed nothing. Bumping there would make the epoch a tick counter and
        // defeat the memoization it exists for.
        if !due.is_empty() {
            self.bump_epoch();
        }
        for (idx, lua, cb) in due {
            if self.enabled.borrow().get(idx).copied().unwrap_or(false) {
                if let Ok(f) = lua.registry_value::<Function>(&cb) {
                    if let Err(e) = call_guarded(&f, ()) {
                        self.report_callback_error(idx, "timer", &e);
                    }
                }
            }
            let _ = lua.remove_registry_value(cb);
        }

        // Recurring timers: re-arm every due one (so the schedule survives a
        // disable), but fire only those whose module is enabled.
        let mut due_recurring: Vec<(usize, Function)> = Vec::new();
        {
            let mut rec = self.recurring.borrow_mut();
            for t in rec.iter_mut() {
                if t.0 <= now {
                    t.0 = now + t.1;
                    if let Ok(f) = t.3.registry_value::<Function>(&t.4) {
                        due_recurring.push((t.2, f));
                    }
                }
            }
        }
        for (idx, f) in due_recurring {
            if self.enabled.borrow().get(idx).copied().unwrap_or(false) {
                if let Err(e) = call_guarded(&f, ()) {
                    self.report_callback_error(idx, "timer", &e);
                }
            }
        }
    }

    /// Delivers finished async image-search results to their callbacks (driven by
    /// the loop tick): each fires its stored callback with the hit table `{x,y,w,h}`
    /// or nil, then drops the one-shot registry value.
    fn fire_image_results(&self) {
        // ONE epoch for the whole drain, not one per result.
        //
        // The bump used to sit next to each callback, which reads correctly on its own — a
        // result IS a fresh observation — and is wrong for a batch. Every search in a batch
        // was answered from ONE captured frame, so the batch is one observation, not eighty-
        // four; and the epoch is what every memo is keyed on. Turning it over per result
        // meant that during the delivery of an 84-result batch, the shared per-(spec, epoch)
        // origin memo was invalidated eighty-four times — cold on exactly the tick that had
        // the most callers to share it with, each re-resolving what the one before it had
        // just worked out. Bumped once, before the first callback runs, so they all see the
        // same new world. Fewer invalidations AND a truer statement about what changed.
        let mut bumped = false;
        while let Ok(res) = self.image_results.try_recv() {
            // A search nobody can see taking long is a bug nobody can diagnose. The
            // landmark poll is entirely invisible from Luau — it runs on a worker and only
            // its verdict is observable — so a slow one gets a line naming both halves,
            // capture and match, and how many tasks shared the capture. Only when it is
            // actually slow: the steady state stays silent.
            // Reported per BATCH, not per search, and that distinction was itself a
            // measurement bug: with twelve libraries each search sat at ~39 ms — just under
            // a 40 ms per-search threshold, so the log fell completely silent while the
            // twelve of them together still cost ~470 ms before any overlay could answer.
            // The batch is the unit somebody actually waits for. Deduped on the first
            // result of each batch so one line is logged, not twelve.
            if res.first_of_batch && res.capture_ms + res.match_ms >= 25 {
                logging::line(
                    "image",
                    &format!(
                        "batch of {} search(es) took {} ms (capture {}, match {} across {} thread-pool tasks)",
                        res.batch,
                        res.capture_ms + res.match_ms,
                        res.capture_ms,
                        res.match_ms,
                        res.batch
                    ),
                );
            }
            let entry = self.pending_image.borrow_mut().remove(&res.id);
            if let Some((lua, cb, idx)) = entry {
                // A result arriving is a fresh observation of the screen, and its callback
                // may re-check an overlay — so the epoch turns once here, rather than on
                // every (mostly empty) poll tick. See the note above for why once per DRAIN.
                if !bumped {
                    self.bump_epoch();
                    bumped = true;
                }
                if self.enabled.borrow().get(idx).copied().unwrap_or(false) {
                    if let Ok(f) = lua.registry_value::<Function>(&cb) {
                        // The hit table carries `n`, the 1-based index of the template
                        // that matched — meaningful for a multi-template search, and
                        // always 1 for a single-template one.
                        let arg = match res.hit {
                            Some((x, y, w, h, n)) => match lua.create_table() {
                                Ok(t) => {
                                    let _ = t.set("x", x);
                                    let _ = t.set("y", y);
                                    let _ = t.set("w", w);
                                    let _ = t.set("h", h);
                                    let _ = t.set("n", n);
                                    mlua::Value::Table(t)
                                }
                                Err(_) => mlua::Value::Nil,
                            },
                            None => mlua::Value::Nil,
                        };
                        if let Err(e) = call_guarded(&f, arg) {
                            self.report_callback_error(idx, "imageSearchAsync", &e);
                        }
                    }
                }
                let _ = lua.remove_registry_value(cb);
            }
        }
    }

    /// Applies a setting change from the GUI: validates against the schema,
    /// updates the store (auto-persisted on the next tick), and fires onChange.
    fn set_setting(&self, idx: usize, key: &str, value: settings::Value) {
        let valid = {
            let schemas = self.schemas.borrow();
            schemas
                .get(idx)
                .and_then(|m| m.get(key))
                .map(|f| f.validate(&value).is_ok())
                .unwrap_or(false)
        };
        if !valid {
            return;
        }
        let id = self.ids.borrow()[idx].clone();
        let old = self.store.borrow_mut().set(&id, key, value.clone());
        self.dirty.set(true);
        self.fire_on_change(idx, key, &value, old.as_ref());
    }

    /// Fires the `onChange` callbacks registered for (module, key).
    fn fire_on_change(
        &self,
        idx: usize,
        key: &str,
        new: &settings::Value,
        old: Option<&settings::Value>,
    ) {
        let cbs: Vec<(Lua, Function)> = {
            let map = self.on_change.borrow();
            match map.get(&(idx, key.to_string())) {
                Some(list) => list
                    .iter()
                    .filter_map(|(lua, rk)| {
                        lua.registry_value::<Function>(rk).ok().map(|f| (lua.clone(), f))
                    })
                    .collect(),
                None => Vec::new(),
            }
        };
        for (lua, f) in cbs {
            let new_v = value_to_lua(&lua, new).unwrap_or(mlua::Value::Nil);
            let old_v = old
                .and_then(|o| value_to_lua(&lua, o).ok())
                .unwrap_or(mlua::Value::Nil);
            if let Err(e) = call_guarded(&f, (new_v, old_v)) {
                self.report_callback_error(idx, &format!("settings onChange ({key})"), &e);
            }
        }
    }
}

/// A loaded module: its identity + its own Luau VM.
struct Module {
    id: String,
    name: String,
    version: String,
    /// Ids this module depends on (used to mark depended-upon modules as
    /// libraries, hidden from the manager's toggle list).
    dependencies: Vec<String>,
    lua: Lua,
}

/// Loads a module from an unpacked directory and runs its entry point, appending
/// it to the shared module list. Declared dependencies are loaded first
/// (auto-discovered among sibling directories), so their exports are available
/// via `host.require`. Returns the loaded module's id. Used both at startup and
/// at runtime — hot-loading a freshly installed module into the running app.
/// Collects, in dependency order, the transitive set of `code_module`
/// dependencies whose code must be evaluated inside a dependent's VM (so their
/// functions — not just data — are reachable via `host.require`). Non-code
/// dependencies are marked visited but not collected: they stay on the data
/// path, and a dependent never pulls their code (or their deps' code) into its
/// VM. `seen` guards cycles/repeats; `out` ends topologically sorted (a
/// dependency precedes the modules that depend on it).
fn collect_code_deps(
    parent: &Path,
    dep_ids: &[String],
    optional_dep_ids: &[String],
    out: &mut Vec<(String, std::path::PathBuf)>,
    seen: &mut HashSet<String>,
) -> Result<()> {
    for spec in dep_ids {
        collect_one_code_dep(parent, spec, false, out, seen)?;
    }
    // Optional code deps: same, but a missing one is skipped (best-effort) instead of
    // erroring, so the dependent still loads and just doesn't get its code/functions.
    for spec in optional_dep_ids {
        collect_one_code_dep(parent, spec, true, out, seen)?;
    }
    Ok(())
}

fn collect_one_code_dep(
    parent: &Path,
    spec: &str,
    optional: bool,
    out: &mut Vec<(String, std::path::PathBuf)>,
    seen: &mut HashSet<String>,
) -> Result<()> {
    let dep_id = module_manifest::dep_id(spec);
    if !seen.insert(dep_id.to_string()) {
        return Ok(()); // already visited
    }
    let dir = match find_module_dir(parent, dep_id) {
        Some(d) => d,
        None if optional => return Ok(()), // absent optional dependency — skip
        None => anyhow::bail!(
            "dependency '{dep_id}' not found beside {} or in a sibling modules/ dir",
            parent.display()
        ),
    };
    let lm = LoadedModule::load(&dir)?;
    if !lm.manifest.code_module {
        return Ok(()); // legacy data dependency — not loaded into the VM
    }
    // Its own code-module deps first (required + optional), so they're registered before it.
    collect_code_deps(parent, &lm.manifest.dependencies, &lm.manifest.optional_dependencies, out, seen)?;
    out.push((dep_id.to_string(), lm.entry_path()));
    Ok(())
}

/// Verifies a loaded dependency's version against a dependent's declared semver
/// requirement (e.g. `"com.x >= 1.2"`). A requirement or dependency version that
/// isn't valid semver is logged and skipped (best-effort) rather than blocking the
/// load; a parseable-but-unsatisfied requirement fails the load with a clear error.
fn check_dep_version(dependent: &str, dep_id: &str, req: &str, dep_version: &str) -> Result<()> {
    // An unparseable requirement is a module-author typo we can't act on — log + skip
    // (best-effort) rather than block the load.
    let vr = match semver::VersionReq::parse(req) {
        Ok(vr) => vr,
        Err(_) => {
            logging::line(
                "manager",
                &format!("version check skipped: requirement '{req}' for '{dep_id}' is not valid semver"),
            );
            return Ok(());
        }
    };
    // The requirement IS valid, so a non-semver dependency version is a hard error:
    // silently NOT enforcing a real constraint (e.g. because the dependency has a
    // 2-part `version = "1.2"`) would defeat its purpose.
    let v = semver::Version::parse(dep_version).map_err(|_| {
        anyhow::anyhow!(
            "module '{dependent}' requires '{dep_id}' {req}, but its version '{dep_version}' is not valid semver"
        )
    })?;
    if !vr.matches(&v) {
        anyhow::bail!(
            "module '{dependent}' requires '{dep_id}' {req}, but the loaded version is v{dep_version}"
        );
    }
    Ok(())
}

/// Runs `code` with `host` passed as a parameter — `function(host) <code> end` —
/// so the chunk and every closure it creates capture this exact host table, not
/// the VM global. Used to give a code dependency its own identity-scoped host.
/// Returns whatever the chunk returns. (Because the code becomes a function body,
/// a `code_module`'s file must not use top-level `...` varargs.)
fn eval_on_host(lua: &Lua, host: &Table, code: &str, name: &str) -> Result<mlua::Value> {
    let factory: Function =
        lua.load(format!("return function(host) {code}\nend")).set_name(name).eval()?;
    Ok(factory.call::<mlua::Value>(host.clone())?)
}

/// Builds the host a code dependency runs with: its identity facilities
/// (path / resource / settings / config / screen / sound — anchored to the
/// module's id + root) are scoped to `dep_idx`, while everything else (ownership:
/// hotkeys / keys / timers / arbiter; plus window / uia / input / speech / …)
/// falls through to `host_owner` (this VM's host) via the metatable. So a shared
/// base module's settings stay under the base's id, while the hotkeys/timers its
/// code registers belong to the VM owner (disable the owner and they go too).
fn build_dep_host(
    lua: &Lua,
    shared: &Rc<Shared>,
    host_owner: &Table,
    dep_idx: usize,
) -> Result<Table> {
    // Already gated by the DEPENDENCY's own manifest, which is the whole point: the module
    // that wrote the code is the one whose declaration decides what the code may reach.
    let dep_full = install_host_api(lua, shared, dep_idx)?;
    let dep_caps = shared.caps.borrow().get(dep_idx).cloned().unwrap_or_default();
    let host = lua.create_table()?;
    for key in ["path", "resource", "settings", "config", "screen", "sound"] {
        // Identity facilities the dependency did not ask for are left off, so the refusal
        // below answers for them like any other.
        if capability_for(key).is_none_or(|cap| dep_caps.contains(cap)) {
            let v: mlua::Value = dep_full.raw_get(key)?;
            host.set(key, v)?;
        }
    }

    // PERMISSION FROM THE DEPENDENCY, BINDING FROM THE OWNER.
    //
    // The fall-through used to be `__index = host_owner`, which is what makes a hotkey or a
    // timer the dependency registers belong to the VM that runs it — disable the owner and
    // they go with it. That has to survive, so a permitted namespace still resolves to the
    // owner's table.
    //
    // What must not survive is the fall-through for a namespace the dependency did NOT ask
    // for: handing back the owner's copy would return exactly what was just denied, and would
    // mean a module could reach anything its dependents happen to have declared.
    let dep_id = shared.ids.borrow().get(dep_idx).cloned().unwrap_or_default();
    let owner = host_owner.clone();
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |_, (_t, key): (Table, String)| -> mlua::Result<mlua::Value> {
            if let Some(cap) = capability_for(&key) {
                if !dep_caps.contains(cap) {
                    return Err(undeclared(
                        &dep_id,
                        &key,
                        ". Its code runs inside another module's VM; what that module declares \
                         does not apply to it",
                    ));
                }
            }
            owner.raw_get(key)
        })?,
    )?;
    host.set_metatable(Some(mt))?;
    // Installed explicitly, NOT inherited through the metatable: an included file must be
    // resolved against — and handed the host of — the module that includes it. Falling
    // through to the owner would resolve a code module's own files under the DEPENDENT's
    // root, and hand them the dependent's host.
    install_include(lua, shared, dep_idx, &host, &host)?;
    Ok(host)
}

/// Calls a no-arg-or-args Lua callback, catching BOTH a Lua error and a Rust
/// panic (re-raised across the mlua boundary), so a faulting module's callback can
/// never abort the process. Returns a human-readable error string.
fn call_guarded<A: mlua::IntoLuaMulti>(f: &Function, args: A) -> Result<(), String> {
    guard(|| f.call::<()>(args))
}

/// Runs `f` (an mlua-returning closure, e.g. the window-trigger dispatch),
/// catching BOTH a Lua error and a re-raised Rust panic, so a faulting module
/// can never abort the process. Returns a human-readable error string.
fn guard<F: FnOnce() -> mlua::Result<()>>(f: F) -> Result<(), String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e.to_string()),
        Err(p) => Err(panic_text(&p)),
    }
}

fn panic_text(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .map(|s| format!("Rust panic: {s}"))
        .unwrap_or_else(|| "Rust panic (no message)".into())
}

#[cfg(test)]
mod hotkey_conflict_tests {
    use super::*;

    /// Ctrl+Alt+H, wanted by two modules.
    const H: (u32, u8) = (0x48, 0b011);
    /// A different combination, so the tests can tell "nobody holds it" from "the wrong one".
    const J: (u32, u8) = (0x4A, 0b011);

    /// `(id, binding, module_idx)` claims, none of them holding the key yet.
    fn winners(regs: &[(i32, Option<(u32, u8)>, usize)], on: &[bool]) -> HashMap<(u32, u8), i32> {
        hotkey_winners(regs.iter().map(|(i, b, m)| (*i, *b, *m, false)), |i| {
            on.get(i).copied().unwrap_or(false)
        })
    }

    /// The same, with a fourth field saying which claim currently HOLDS its combination.
    fn winners_live(
        regs: &[(i32, Option<(u32, u8)>, usize, bool)],
        on: &[bool],
    ) -> HashMap<(u32, u8), i32> {
        hotkey_winners(regs.iter().copied(), |i| on.get(i).copied().unwrap_or(false))
    }

    /// A module that is holding a key does not lose it to a module that merely loads earlier.
    ///
    /// Not hypothetical: overlays register their control hotkeys on activation and release
    /// them on deactivation, and `Alt+B` is a control in more than one shipped overlay. On a
    /// switch between two plug-ins the arriving overlay can register while the leaving one is
    /// still holding, and pure load order would take the key off the module still using it —
    /// mid-gesture, on its way out, with a dialog telling the user the wrong thing.
    #[test]
    fn a_module_holding_a_key_keeps_it_against_a_lower_index() {
        // Module 1 holds Ctrl+Alt+H; module 0 loads earlier and now claims it too.
        let regs = [(9, Some(H), 1usize, true), (12, Some(H), 0usize, false)];
        assert_eq!(winners_live(&regs, &[true, true]).get(&H), Some(&9));

        // The moment the holder is disabled, order decides again and module 0 takes it.
        assert_eq!(winners_live(&regs, &[true, false]).get(&H), Some(&12));

        // And with nobody holding it, order decides from the start.
        let fresh = [(9, Some(H), 1usize, false), (12, Some(H), 0usize, false)];
        assert_eq!(winners_live(&fresh, &[true, true]).get(&H), Some(&12));
    }

    /// The bug this was written for: disabling the module that holds a combination has to
    /// hand it to the module that has been waiting since the session started.
    #[test]
    fn disabling_the_holder_hands_the_key_over() {
        // Module 0 loads first (id 1), module 1 second (id 2), both want Ctrl+Alt+H.
        let regs = [(1, Some(H), 0usize), (2, Some(H), 1usize)];

        // Both enabled: the one that loaded first keeps it, which is what the conflict
        // message promises the user.
        assert_eq!(winners(&regs, &[true, true]).get(&H), Some(&1));

        // Module 0 disabled: module 1's standing claim becomes the live one. Before the fix
        // this was not merely wrong — module 1's registration had never been recorded, so
        // there was nothing here to promote and it took a restart.
        assert_eq!(winners(&regs, &[false, true]).get(&H), Some(&2));

        // And back again when it is re-enabled.
        assert_eq!(winners(&regs, &[true, true]).get(&H), Some(&1));

        // Neither enabled: nobody holds it, and the combination is left to other applications.
        assert!(winners(&regs, &[false, false]).is_empty());
    }

    /// Load order decides between modules, and the reason is the reload.
    ///
    /// Ordering by id alone reads like "first-come" and fails: ids are monotonic and never
    /// reused, so a module that reloads registers again with a higher id than any standing
    /// claim on its combination and hands its own key away for the rest of the session. This
    /// is that scenario, written as the numbers actually come out.
    #[test]
    fn a_reloaded_module_keeps_its_own_key() {
        // Module 0 registered id 2 at start-up and holds Ctrl+Alt+H; module 1 has a standing
        // claim with id 3. Then module 0 is reloaded: its old registration is gone and the
        // fresh VM registers again — as id 42, because ids only ever go up.
        let after_reload = [(42, Some(H), 0usize), (3, Some(H), 1usize)];
        assert_eq!(
            winners(&after_reload, &[true, true]).get(&H),
            Some(&42),
            "the reloaded module keeps its key; ordering by id alone would give it to module 1"
        );
        // And the ordinary case is unchanged, because at start-up the two readings agree.
        let at_startup = [(2, Some(H), 0usize), (3, Some(H), 1usize)];
        assert_eq!(winners(&at_startup, &[true, true]).get(&H), Some(&2));
    }

    /// Within one module, its own earliest registration wins — load order only decides
    /// BETWEEN modules.
    #[test]
    fn registration_order_is_the_tiebreak_inside_a_module() {
        let regs = [(7, Some(H), 0usize), (3, Some(H), 0usize)];
        assert_eq!(winners(&regs, &[true]).get(&H), Some(&3));
    }

    /// Disabling the holder passes it down the load order, not to whoever has the lowest id.
    #[test]
    fn the_next_module_in_load_order_takes_over() {
        let regs = [(7, Some(H), 0usize), (3, Some(H), 1usize), (5, Some(H), 2usize)];
        assert_eq!(winners(&regs, &[true, true, true]).get(&H), Some(&7));
        assert_eq!(winners(&regs, &[false, true, true]).get(&H), Some(&3));
        assert_eq!(winners(&regs, &[false, false, true]).get(&H), Some(&5));
    }

    /// One module holding two different combinations keeps both, and a self-rebind of the
    /// same combination does not fight itself into nobody holding it.
    #[test]
    fn separate_combinations_and_a_self_rebind() {
        let regs = [(1, Some(H), 0usize), (2, Some(J), 0usize), (3, Some(H), 0usize)];
        let w = winners(&regs, &[true]);
        assert_eq!(w.get(&H), Some(&1), "the module's own earlier claim keeps it");
        assert_eq!(w.get(&J), Some(&2));
        assert_eq!(w.len(), 2);
    }

    /// An unparseable spec has no comparable binding, so it never wins or loses a contest
    /// here — the OS is its only judge, and it was already asked at registration.
    #[test]
    fn an_unparseable_spec_is_left_to_the_os() {
        let regs = [(1, None, 0usize), (2, Some(H), 0usize)];
        let w = winners(&regs, &[true]);
        assert_eq!(w.len(), 1);
        assert_eq!(w.get(&H), Some(&2));
    }
}

#[cfg(test)]
mod capability_gate_tests {
    use super::*;

    /// Can a module reach a namespace it did not declare?
    ///
    /// This started as a scratch module run against the real application, after the window
    /// prelude was changed to hold the whole host table as an upvalue and the owner asked the
    /// right question: privileged Luau running inside a module's VM knows a way to the ungated
    /// table, so is that way open to every module? It is not, and this is here so it stays
    /// shut — a regression would be silent, and the manifest a module shows the user before
    /// installation is only worth reading if the runtime holds the author to it.
    ///
    /// The last case is the sharp one. `host.log.privileged` is a Luau function holding the
    /// FULL table as an upvalue, reachable from a namespace the module does have: the shape
    /// of the prelude, reduced to what makes it dangerous.
    #[test]
    fn a_module_cannot_reach_an_undeclared_namespace() {
        let lua = Lua::new();
        let full = lua.create_table().unwrap();
        for (key, _) in GATED {
            let ns = lua.create_table().unwrap();
            ns.set("marker", *key).unwrap();
            full.set(*key, ns).unwrap();
        }

        // `host.os` is not gated and the prelude reads it, so the stub needs it too.
        let os = lua.create_table().unwrap();
        os.set("current", "windows").unwrap();
        full.set("os", os).unwrap();

        // The REAL prelude, loaded the way the runtime loads it: wrapped in `function(host)`
        // and called with the ungated table, so `host` is an upvalue and not a global. This
        // is the privileged Luau the question was about, so it is the privileged Luau the
        // test uses — a stand-in written for the test would only prove things about itself.
        lua.load(format!("return function(host) {WINDOW_PRELUDE}
end"))
            .set_name("window_prelude")
            .into_function()
            .unwrap()
            .call::<Function>(())
            .unwrap()
            .call::<()>(&full)
            .unwrap();

        // `window` is declared, `speech` and the rest are not: the module may hold the
        // prelude's own functions and still must not get from them to anything else.
        let caps: HashSet<String> =
            ["log", "window"].iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(&lua, &full, &caps, "com.platform.probe").unwrap();
        lua.globals().set("host", &view).unwrap();

        let failures: Vec<String> = lua
            .load(
                r#"
                local holes = {}
                local function check(what, f)
                    local ok, res = pcall(f)
                    if ok and res then table.insert(holes, what .. ": " .. tostring(res)) end
                end
                -- Does this value hand over a namespace we never declared?
                local function opens(v)
                    return type(v) == "table" and rawget(v, "speech") ~= nil
                end

                check("host.speech directly", function()
                    return opens(host.speech) and "reached" or nil
                end)
                check("_G.host", function()
                    return opens(_G.host.screen) and "reached" or nil
                end)
                check("a scan of _G", function()
                    for k, v in pairs(_G) do
                        if opens(v) then return "global " .. tostring(k) end
                    end
                    return nil
                end)
                check("debug.getupvalue on a prelude function", function()
                    local d = debug
                    if type(d) ~= "table" or type(d.getupvalue) ~= "function" then return nil end
                    for i = 1, 20 do
                        local name, value = d.getupvalue(host.window.find, i)
                        if name == nil then break end
                        if opens(value) then return "upvalue " .. tostring(name) end
                    end
                    return nil
                end)
                check("getfenv", function()
                    if type(getfenv) ~= "function" then return nil end
                    return opens(getfenv(1).host.speech) and "reached" or nil
                end)
                check("the metatable of host", function()
                    local mt = getmetatable(host)
                    if mt == nil then return nil end
                    local idx = rawget(mt, "__index")
                    if opens(idx) then return "__index is the whole table" end
                    if type(idx) == "function" then
                        local ok, v = pcall(idx, host, "speech")
                        if ok and v ~= nil then return "__index handed it over" end
                    end
                    return nil
                end)
                check("a scan of host.log", function()
                    for k, v in pairs(host.log) do
                        if opens(v) then return "host.log." .. tostring(k) end
                    end
                    return nil
                end)
                -- Everything the prelude hung on the namespace it extends, two levels deep.
                -- A privileged table left reachable there would be the whole gate undone.
                check("what the prelude left on host.window", function()
                    for k, v in pairs(host.window) do
                        if opens(v) then return "host.window." .. tostring(k) end
                        if type(v) == "table" then
                            for k2, v2 in pairs(v) do
                                if opens(v2) then
                                    return "host.window." .. tostring(k) .. "." .. tostring(k2)
                                end
                            end
                        end
                    end
                    return nil
                end)

                return holes
            "#,
            )
            .eval()
            .unwrap();

        assert!(failures.is_empty(), "capability gate has holes: {failures:?}");
    }

    /// The one rule privileged Luau has to follow, written down as a test because it is the
    /// only thing between the gate and nothing.
    ///
    /// Host code running inside a module VM holds the ungated table. Nothing stops such a
    /// function from handing it out — no mechanism could, short of not writing the function
    /// — so this asserts the leak rather than denying it. It is here so that anyone adding
    /// to `window_prelude.luau` meets the rule before they meet the consequences: **do not
    /// return the host table, and do not store it anywhere a module can name.** The test
    /// above checks that the prelude as it stands obeys that.
    #[test]
    fn privileged_luau_that_hands_out_its_host_defeats_the_gate() {
        let lua = Lua::new();
        let full = lua.create_table().unwrap();
        full.set("speech", lua.create_table().unwrap()).unwrap();
        let leaky: Function = lua
            .load("return function(host) return function() return host end end")
            .eval::<Function>()
            .unwrap()
            .call(&full)
            .unwrap();

        let caps: HashSet<String> = ["log"].iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(&lua, &full, &caps, "com.example.probe").unwrap();
        let log = lua.create_table().unwrap();
        log.set("leaky", leaky).unwrap();
        full.set("log", log).unwrap();
        lua.globals().set("host", view).unwrap();

        // Denied through the front door...
        assert!(lua.load("return host.speech").eval::<mlua::Value>().is_err());
        // ...and wide open through a function that gives its table away.
        let reached: bool =
            lua.load("return host.log.leaky().speech ~= nil").eval().unwrap();
        assert!(reached, "the demonstration this test exists for stopped working");
    }

    /// The require guard: what it catches, and what it lets through on purpose.
    #[test]
    fn a_dependency_may_export_functions_but_not_its_host() {
        let lua = Lua::new();
        let dep_host = lua.create_table().unwrap();
        dep_host.set("screen", lua.create_table().unwrap()).unwrap();

        // The two shapes that are a handover.
        assert!(hands_over_its_host(&dep_host, &dep_host), "returning the table itself");
        let wrapped = lua.create_table().unwrap();
        wrapped.set("host", &dep_host).unwrap();
        assert!(hands_over_its_host(&wrapped, &dep_host), "one level down is still a handover");

        // And the shapes that are an API, which must keep working — this is the whole
        // inheritance model: a dependency acts with ITS permissions on the dependent's behalf.
        let api = lua.create_table().unwrap();
        api.set("look", lua.create_function(|_, ()| Ok(1)).unwrap()).unwrap();
        api.set("name", "kontakt").unwrap();
        assert!(!hands_over_its_host(&api, &dep_host), "exported functions are the model");

        // A namespace the dependency was granted is not the host table, and passing one on is
        // the dependency's own decision to make — the guard is about the accident, not about
        // policing what a module chooses to expose.
        let ns = lua.create_table().unwrap();
        ns.set("screen", dep_host.get::<Table>("screen").unwrap()).unwrap();
        assert!(!hands_over_its_host(&ns, &dep_host));

        // A different module's host table must not trip it either.
        let other = lua.create_table().unwrap();
        assert!(!hands_over_its_host(&other, &dep_host));
    }

    /// The gate refuses by NAME, so the message can say which module and which capability —
    /// a refusal a user cannot act on is only half a refusal.
    #[test]
    fn the_refusal_names_the_module_and_the_capability() {
        let lua = Lua::new();
        let full = lua.create_table().unwrap();
        full.set("speech", lua.create_table().unwrap()).unwrap();
        let caps: HashSet<String> = ["log"].iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(&lua, &full, &caps, "com.example.quiet").unwrap();
        lua.globals().set("host", view).unwrap();

        let err = lua.load("return host.speech").eval::<mlua::Value>().unwrap_err().to_string();
        assert!(err.contains("com.example.quiet"), "{err}");
        assert!(err.contains("speech"), "{err}");
    }

    /// A module that declares everything gets the table itself, with no view in the way.
    /// Worth pinning: it is the path every dependency's code takes.
    #[test]
    fn declaring_everything_removes_the_view() {
        let lua = Lua::new();
        let full = lua.create_table().unwrap();
        full.set("speech", "yes").unwrap();
        let caps: HashSet<String> = GATED.iter().map(|(_, cap)| (*cap).to_string()).collect();
        let view = gated_view(&lua, &full, &caps, "com.example.everything").unwrap();
        assert_eq!(view.get::<String>("speech").unwrap(), "yes");
        assert!(view.metatable().is_none());
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    #[test]
    fn guard_catches_lua_error_and_rust_panic() {
        let lua = Lua::new();
        // A Lua error in a callback is surfaced as a string, not a crash.
        let err: Function = lua.load("return function() error('boom') end").eval().unwrap();
        assert!(call_guarded(&err, ()).unwrap_err().contains("boom"));
        // A clean callback succeeds.
        let ok: Function = lua.load("return function() end").eval().unwrap();
        assert!(call_guarded(&ok, ()).is_ok());
        // A Rust panic is caught too (silence the hook so the test output is clean).
        let saved = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let p = guard(|| panic!("kaboom")).unwrap_err();
        std::panic::set_hook(saved);
        assert!(p.contains("kaboom"));
    }

    #[test]
    fn load_isolation_catches_panic_and_passes_errors() {
        // Mirrors load_module's wrapper: a panic in the load closure becomes a load
        // error (not a process abort), while a Lua error and success pass through.
        let run = |body: fn() -> Result<()>| -> Result<()> {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(body))
                .unwrap_or_else(|p| Err(anyhow::anyhow!("panic while loading: {}", panic_text(&p))))
        };
        let saved = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let panicked = run(|| panic!("load boom"));
        std::panic::set_hook(saved);
        assert!(panicked.unwrap_err().to_string().contains("load boom"));
        assert!(run(|| Err(anyhow::anyhow!("lua err"))).unwrap_err().to_string().contains("lua err"));
        assert!(run(|| Ok(())).is_ok());
    }

    #[test]
    fn key_spec_is_stable_for_conflict_comparison() {
        // Conflict detection compares backend::key_spec(spec) across modules, so
        // equivalent specs must normalize identically (modifier order + case) and
        // distinct combos must differ.
        assert_eq!(backend::key_spec("Ctrl+Alt+H"), backend::key_spec("alt+ctrl+h"));
        assert!(backend::key_spec("Ctrl+Alt+H").is_some());
        assert_ne!(backend::key_spec("Ctrl+Alt+H"), backend::key_spec("Ctrl+H"));
        assert_ne!(backend::key_spec("Tab"), backend::key_spec("Shift+Tab"));
    }

    #[test]
    fn dep_version_check_enforces_semver_requirements() {
        assert!(check_dep_version("a", "b", ">= 1.0.0", "1.2.0").is_ok());
        assert!(check_dep_version("a", "b", ">= 2.0.0", "1.2.0").is_err());
        assert!(check_dep_version("a", "b", "^0.1", "0.1.5").is_ok());
        assert!(check_dep_version("a", "b", "^0.2", "0.1.5").is_err());
        // A valid requirement but a non-semver (2-part) dependency version is a hard
        // error — the constraint must not silently no-op.
        assert!(check_dep_version("a", "b", ">= 1.0.0", "1.2").is_err());
    }
}

/// Builds module `idx`'s VM in place: installs its host, runs the window prelude,
/// evaluates its code dependencies into the VM, runs its entry (publishing its data
/// export), and runs its `activate`. Shared by initial load and reload; the caller
/// owns the surrounding catch_unwind + rollback, and the module's parallel-vector
/// slots (roots/ids/enabled/schemas at `idx`) must already be set up.
fn populate_vm(
    shared: &Rc<Shared>,
    parent: &Path,
    idx: usize,
    module: &LoadedModule,
    lua: &Lua,
) -> Result<()> {
    let id = &module.manifest.id;
    // This VM's host: identity (settings/resource/path) + ownership (hotkeys/timers/
    // keys/arbiter) scoped to the module (idx). Set as the global so its entry + the
    // host's dispatch resolve it; code dependencies get an identity-scoped variant.
    let host_m = install_host_api(lua, shared, idx).context("failed to install host API")?;
    // The prelude EXTENDS the host — `host.os.pick`, the window matchers — so it runs against
    // the whole table, before anything is gated. Gating it first would have the prelude write
    // its additions onto the view instead, where a dependency's fall-through cannot see them.
    lua.globals().set("host", &host_m)?;
    // Wrapped so that `host` inside the prelude is an UPVALUE holding the whole table, not a
    // global looked up when its functions later run — by which time the global has become the
    // module's gated view.
    //
    // That distinction was not academic. `W._dispatchFocus` logs when a dispatch is slow, and
    // that `host.log` resolved at call time against whichever module's VM it happened to be
    // running in. In a module that had not declared `log` — and it had no reason to; the line
    // is OURS — the platform's own diagnostic raised a capability refusal into the middle of
    // the focus dispatch it was measuring, aborting it and putting an error dialog in front of
    // somebody whose module had done nothing wrong.
    //
    // The wrapper opens on the same line as the prelude's first, so reported line numbers
    // still match the file.
    let prelude = lua
        .load(format!("return function(host) {WINDOW_PRELUDE}
end"))
        .set_name("window_prelude")
        .into_function()?;
    prelude.call::<Function>(())?.call::<()>(&host_m)?;

    // From here the module's own code sees only what its manifest declares. `host_m` stays
    // whole, and is what a code dependency's permitted namespaces bind to, so a hotkey or a
    // timer the dependency registers still belongs to THIS module and dies with it.
    let caps = shared.caps.borrow().get(idx).cloned().unwrap_or_default();
    let host_view = gated_view(lua, &host_m, &caps, id)?;
    // Registered on the whole table (that is what the view falls through to), handing over
    // the VIEW (an included file is this module's own code, judged by this module's manifest).
    install_include(lua, shared, idx, &host_m, &host_view)?;
    lua.globals().set("host", &host_view)?;

    // Top-down dependency loading: evaluate each `code_module` dependency
    // (transitively, in dependency order) inside this VM and record its returned
    // object in the per-VM `__module_exports` registry, so host.require(id) gets its
    // functions. Legacy (non-code) dependencies stay on the data path.
    let reg = lua.create_table()?;
    lua.set_named_registry_value("__module_exports", reg.clone())?;
    let mut code_deps: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect_code_deps(
        parent,
        &module.manifest.dependencies,
        &module.manifest.optional_dependencies,
        &mut code_deps,
        &mut HashSet::new(),
    )?;
    for (dep_id, dep_entry) in &code_deps {
        let dep_code = std::fs::read_to_string(dep_entry).with_context(|| {
            format!("dependency '{dep_id}' entry not readable: {}", dep_entry.display())
        })?;
        // Run the dependency's code with a host whose identity facilities are scoped
        // to the DEPENDENCY's id, while ownership + everything else falls through to
        // this VM's host. The dep is already loaded (deps resolve first), so its id is
        // in shared.ids; fail loud rather than silently mis-scope its identity.
        let dep_idx = shared.ids.borrow().iter().position(|i| i == dep_id).ok_or_else(|| {
            anyhow::anyhow!("code dependency '{dep_id}' of '{id}' not in the module table")
        })?;
        let host_dep = build_dep_host(lua, shared, &host_m, dep_idx)?;
        let dep_ret = eval_on_host(lua, &host_dep, &dep_code, &dep_entry.display().to_string())
            .with_context(|| format!("error running dependency '{dep_id}' of '{id}'"))?;
        if let mlua::Value::Table(t) = &dep_ret {
            // A dependency must not hand its own host table back to the module that required
            // it. Everything else here is deliberate — a dependency acts with ITS permissions
            // on the dependent's behalf, and a wrapper function it exports is exactly that
            // working as intended. But the table itself is not an API: passing it over grants
            // every capability the dependency holds, in one object, to a module whose manifest
            // names none of them. Nobody designs that on purpose, so it is caught.
            //
            // Deliberately shallow, and by identity. A closure that returns the table when
            // called cannot be caught by any amount of scanning — that is the rule
            // `privileged_luau_that_hands_out_its_host_defeats_the_gate` records — so this
            // catches the accident and does not pretend to be a boundary.
            if hands_over_its_host(t, &host_dep) {
                anyhow::bail!(
                    "dependency '{dep_id}' of '{id}' returns its own host table, which would \
                     hand '{id}' every capability '{dep_id}' declares. Export functions that \
                     use the host, not the host itself."
                );
            }
            reg.set(dep_id.as_str(), dep_ret.clone())?;
        }
    }

    let entry = module.entry_path();
    let code = std::fs::read_to_string(&entry)
        .with_context(|| format!("entry point not readable: {}", entry.display()))?;
    let ret: mlua::Value = lua
        .load(code)
        .set_name(entry.display().to_string())
        .eval()
        .with_context(|| format!("error running module '{id}'"))?;
    // Publish the module's returned table as a best-effort cross-VM data export for
    // the legacy host.require path (functions aren't serializable, so it's skipped
    // rather than fatal when the return isn't plain data).
    if let mlua::Value::Table(_) = &ret {
        if let Ok(exported) = lua.from_value::<serde_json::Value>(ret.clone()) {
            shared.exports.borrow_mut().insert(id.clone(), exported);
        }
    }
    // A returned `activate` runs here, in the module's OWN VM only — never when this
    // module is evaluated as a code dependency inside another VM — so e.g. a base
    // overlay is created once, not duplicated in every dependent that inherits it.
    if let mlua::Value::Table(t) = &ret {
        if let Ok(mlua::Value::Function(activate)) = t.get::<mlua::Value>("activate") {
            activate.call::<()>(()).with_context(|| format!("error activating module '{id}'"))?;
        }
    }
    Ok(())
}

fn load_module(
    shared: &Rc<Shared>,
    modules: &Rc<RefCell<Vec<Module>>>,
    disabled_ids: &HashSet<String>,
    loading: &mut HashSet<String>,
    dir: &Path,
) -> Result<(String, bool)> {
    let module = LoadedModule::load(dir)?;
    let id = module.manifest.id.clone();
    if modules.borrow().iter().any(|m| m.id == id) {
        return Ok((id, false)); // already loaded (e.g. a shared dependency)
    }

    // A module that says it is not for this system is not loaded on it.
    //
    // Its window matchers would already keep it inert — one without a block for this
    // platform never matches — so this is not about correctness. It is about what an inert
    // module still costs: a VM, a matcher evaluated on every focus change, any timer it
    // starts, and any global shortcut it registers, which on the wrong platform claims a
    // combination for something that can never happen and shows the user a conflict dialog
    // about it.
    //
    // Skipped LOUDLY, and only on an explicit claim. A stale manifest is the predictable
    // failure here — Sforzando was Windows-only one day and worked on both the next — so a
    // module that says nothing loads everywhere, an exclusion always names itself in the
    // log, and the override exists for the case where the manifest is simply behind the
    // code.
    let os = std::env::consts::OS;
    if !module.manifest.runs_on(os) {
        let claimed = module.manifest.supported_os.join(", ");
        if appcfg::ignore_supported_os() {
            logging::line(
                "manager",
                &format!(
                    "loading '{id}' anyway: it declares {claimed} and this is {os}, but AUTOMATION_PLATFORM_IGNORE_SUPPORTED_OS is set"
                ),
            );
        } else {
            logging::line(
                "manager",
                &format!(
                    "not loading '{id}' ({}): it declares supported_os = [{claimed}] and this is {os}. If it does work here, the manifest is behind the code — set AUTOMATION_PLATFORM_IGNORE_SUPPORTED_OS=1 to load it anyway.",
                    module.manifest.name
                ),
            );
            // Remembered so the manager can still list it. A module nobody can see is a
            // module nobody can remove, and the person who cannot see it is the one this
            // window exists for.
            {
                let mut ex = shared.excluded.borrow_mut();
                if !ex.iter().any(|e| e.id == id) {
                    ex.push(ExcludedModule {
                        id: id.clone(),
                        name: module.manifest.name.clone(),
                        version: module.manifest.version.clone(),
                        claimed: claimed.clone(),
                    });
                }
            }
            return Ok((id, false));
        }
    }
    if !loading.insert(id.clone()) {
        anyhow::bail!("dependency cycle involving module '{id}'");
    }
    let parent = dir.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    // Resolve + load dependencies first, in a scope so `id` leaves the in-progress
    // set on every exit (including an error) — keeping insert/remove symmetric so
    // a missing/failing dependency can't leave a stale cycle-detection entry.
    let deps = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        for spec in module.manifest.dependencies.clone() {
            let dep = module_manifest::dep_id(&spec);
            if !modules.borrow().iter().any(|m| m.id == dep) {
                let dep_dir = find_module_dir(&parent, dep).ok_or_else(|| {
                    anyhow::anyhow!(
                        "module '{id}' depends on '{dep}', not found beside {} or in a \
                         sibling modules/ dir",
                        parent.display()
                    )
                })?;
                load_module(shared, modules, disabled_ids, loading, &dep_dir)?;
            }
            // Verify a declared min/compatible-version requirement against the
            // now-loaded dependency (fails the load with a clear error if unmet).
            if let Some(req) = module_manifest::dep_constraint(&spec) {
                let found =
                    modules.borrow().iter().find(|m| m.id == dep).map(|m| m.version.clone());
                if let Some(ver) = found {
                    check_dep_version(&id, dep, req, &ver)?;
                }
            }
        }
        // Optional dependencies: load each ONLY if it is actually present (already
        // loaded, or discoverable as a sibling). A missing optional dependency is
        // skipped silently — it never fails the load, and host.require of it returns nil
        // (see the host.require binding) so the dependent adapts. No version gate: an
        // optional dependency is best-effort, and failing the load on a mismatch would
        // defeat the point.
        for spec in module.manifest.optional_dependencies.clone() {
            let dep = module_manifest::dep_id(&spec);
            if !modules.borrow().iter().any(|m| m.id == dep) {
                if let Some(dep_dir) = find_module_dir(&parent, dep) {
                    load_module(shared, modules, disabled_ids, loading, &dep_dir)?;
                }
            }
        }
        Ok(())
    }))
    .unwrap_or_else(|p| {
        Err(anyhow::anyhow!("panic while resolving dependencies of module '{id}': {}", panic_text(&p)))
    });
    loading.remove(&id);
    deps?;

    let idx = modules.borrow().len();
    logging::line(
        "manager",
        &format!(
            "loading module: {} v{} (id {})",
            module.manifest.name, module.manifest.version, module.manifest.id
        ),
    );
    if !module.manifest.capabilities.require.is_empty() {
        logging::line(
            "manager",
            &format!("  capabilities: {}", module.manifest.capabilities.require.join(", ")),
        );
    }

    shared.roots.borrow_mut().push(module.root.clone());
    shared.ids.borrow_mut().push(module.manifest.id.clone());
    // Reflect the persisted enabled state up front (rather than enabled-then-
    // revoked), so the module's entry registers — and conflict-checks — against its
    // true target state: a disabled module records its bindings without claiming or
    // contesting them.
    shared.enabled.borrow_mut().push(!disabled_ids.contains(&module.manifest.id));
    shared
        .caps
        .borrow_mut()
        .push(module.manifest.capabilities.require.iter().cloned().collect());
    shared.schemas.borrow_mut().push(HashMap::new());

    // Everything past the four parallel-vector pushes above is fallible
    // (install, prelude, reading + running the entry, exports conversion). A failure must
    // never leave the shared vectors longer than `modules` — that would mis-index every
    // later module — and that is as true at startup as on a hot-load: startup used to abort
    // the process instead, which is why this rollback was described as being for hot-loads.
    // It is not. See Manager::report_load_failure. Run the fallible work in a scope and, on any error,
    // roll the pushes (and any side effects the partial entry registered) back.
    // Snapshot this module's persisted settings before the fallible load so a
    // partial load's store writes (host.settings.define/set) roll back too —
    // rollback_to undoes registrations but not the store. First load → snapshot is
    // empty (the orphan entry is removed on failure); reload → the prior values are
    // restored, never lost to a transient failure.
    let store_snapshot = shared.store.borrow().snapshot(&id);
    let lua = Lua::new();
    // Catch a Lua error (propagated by `?`) OR a re-raised Rust panic from the
    // module's OWN code — code-dependency eval, entry eval, or `activate` — so a
    // faulty module fails to load and rolls back cleanly instead of aborting the
    // app. This matters most on a runtime hot-load (a startup failure already aborts
    // by design). mlua re-raises a panicking host fn across the call boundary;
    // catch_unwind turns it into a load error. RefCell borrows release on unwind,
    // so the rollback below can clean up the partial registrations safely.
    let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        populate_vm(shared, &parent, idx, &module, &lua)
    }))
    .unwrap_or_else(|p| {
        Err(anyhow::anyhow!("panic while loading module '{id}': {}", panic_text(&p)))
    });
    if let Err(e) = loaded {
        shared.rollback_to(idx, &id);
        shared.store.borrow_mut().restore(&id, store_snapshot);
        return Err(e);
    }

    // If the module registered window triggers, ensure the foreground/focus hook
    // is armed (idempotent) — covers a trigger module hot-loaded into an app that
    // started without one, mirroring how captured keys self-arm at capture time.
    if window_has_triggers(&lua) {
        if let Err(e) = shared.backend.watch_foreground() {
            logging::line("manager", &format!("watch_foreground failed: {e}"));
        }
    }

    // Store the dependency IDS (stripped of any version constraint) — the
    // install/uninstall graph + host.require match on ids; the constraint was
    // already verified above.
    let dep_ids: Vec<String> = module
        .manifest
        .dependencies
        .iter()
        .map(|s| module_manifest::dep_id(s).to_string())
        .collect();
    modules.borrow_mut().push(Module {
        id: module.manifest.id,
        name: module.manifest.name,
        version: module.manifest.version,
        dependencies: dep_ids,
        lua,
    });
    debug_assert_eq!(shared.ids.borrow().len(), modules.borrow().len());
    Ok((id, true))
}

/// Reloads module `idx` in place from its source directory: re-reads the manifest,
/// purges the running module's registrations, and rebuilds its VM under the SAME
/// index (other modules' indices are unaffected — a suffix-only `rollback_to` can't
/// drop a middle module). The enabled flag + persisted settings are kept. Returns
/// the ids of loaded modules that hold this one's code as a (transitive) dependency
/// — they keep the OLD copy until restarted (a live cascade is a separate TODO). On
/// a rebuild error the module is left unregistered (effectively unloaded) with its
/// store rolled back, and the error returned; a later successful reload recovers.
fn reload_module(
    shared: &Rc<Shared>,
    modules: &Rc<RefCell<Vec<Module>>>,
    idx: usize,
) -> Result<Vec<String>> {
    let (dir, old_id) = {
        let dir = shared
            .roots
            .borrow()
            .get(idx)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("module index {idx} out of range"))?;
        let id = modules
            .borrow()
            .get(idx)
            .map(|m| m.id.clone())
            .ok_or_else(|| anyhow::anyhow!("module index {idx} out of range"))?;
        (dir, id)
    };
    // Re-read BEFORE purging, so a now-broken manifest leaves the running module intact.
    let module = LoadedModule::load(&dir)
        .with_context(|| format!("reloading '{old_id}' from {}", dir.display()))?;
    if module.manifest.id != old_id {
        anyhow::bail!(
            "reloaded module changed its id ('{old_id}' -> '{}'); restart to apply",
            module.manifest.id
        );
    }
    let parent = dir.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();

    let store_snapshot = shared.store.borrow().snapshot(&old_id);
    shared.purge_module(idx);
    if let Some(s) = shared.schemas.borrow_mut().get_mut(idx) {
        s.clear();
    }

    let lua = Lua::new();
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        populate_vm(shared, &parent, idx, &module, &lua)
    }))
    .unwrap_or_else(|p| {
        Err(anyhow::anyhow!("panic while reloading module '{old_id}': {}", panic_text(&p)))
    });
    if let Err(e) = built {
        // A partial rebuild may have registered hotkeys/keys against the fresh VM
        // (about to be dropped) — purge again so nothing dangles; restore the store.
        shared.purge_module(idx);
        // Now the module really is gone, so its combinations go to whoever was waiting.
        shared.refresh_hotkeys();
        shared.store.borrow_mut().restore(&old_id, store_snapshot);
        return Err(e);
    }

    // Swap in the fresh VM (the old one, already unregistered, drops here).
    {
        let mut mods = modules.borrow_mut();
        let m = mods
            .get_mut(idx)
            .ok_or_else(|| anyhow::anyhow!("module index {idx} out of range"))?;
        m.lua = lua;
        m.name = module.manifest.name.clone();
        m.version = module.manifest.version.clone();
        m.dependencies = module
            .manifest
            .dependencies
            .iter()
            .map(|s| module_manifest::dep_id(s).to_string())
            .collect();
    }
    if window_has_triggers(&modules.borrow()[idx].lua) {
        if let Err(e) = shared.backend.watch_foreground() {
            logging::line("manager", &format!("watch_foreground failed: {e}"));
        }
    }
    // The end of the operation, which is where the refresh belongs — `purge_module`
    // deliberately does not do it (see the note there). The fresh VM's own
    // `host.hotkey.register` usually covers this, but not always: a module that HAD a hotkey
    // and comes back without one registers nothing, so nothing would run, and the
    // combination its purge released at the OS would be held by nobody while another
    // module's standing claim went on waiting.
    shared.refresh_hotkeys();
    logging::line("manager", &format!("reloaded module: {old_id}"));

    let graph: Vec<(String, Vec<String>)> = modules
        .borrow()
        .iter()
        .map(|m| (m.id.clone(), m.dependencies.clone()))
        .collect();
    Ok(registry::transitive_dependents(&old_id, &graph))
}

/// What a cascading reload did: the module itself, then each dependent that had to be
/// rebuilt with it, with the error for any that failed.
pub struct ReloadReport {
    /// Ids rebuilt successfully, in the order they were rebuilt.
    pub reloaded: Vec<String>,
    /// (id, error) for each dependent whose rebuild failed — it is now inactive.
    pub failed: Vec<(String, String)>,
}

/// Reloads module `idx` AND every module that transitively depends on it, in
/// dependency order.
///
/// The cascade is not a nicety: a code module's source is copied into each dependent's
/// VM when that dependent is built, so rebuilding only the changed module leaves every
/// dependent running the OLD copy. That is why updating one used to end in "restart to
/// apply" — the running app had no way to reach the stale copies.
///
/// Order matters for the same reason: a dependent must be rebuilt AFTER the dependency
/// whose code it will copy, or it copies the old code and the fix is invisible until the
/// next restart, which is precisely the bug being fixed.
///
/// A dependent that fails to rebuild is left inactive and reported, rather than aborting
/// the cascade: the remaining ones are independent of it, and stopping halfway would
/// leave more of the system stale than continuing does.
fn reload_module_tree(
    shared: &Rc<Shared>,
    modules: &Rc<RefCell<Vec<Module>>>,
    idx: usize,
) -> Result<ReloadReport> {
    let dependents = reload_module(shared, modules, idx)?;
    let root_id = modules.borrow()[idx].id.clone();
    let mut report = ReloadReport { reloaded: vec![root_id.clone()], failed: Vec::new() };
    if dependents.is_empty() {
        return Ok(report);
    }

    let graph: Vec<(String, Vec<String>)> =
        modules.borrow().iter().map(|m| (m.id.clone(), m.dependencies.clone())).collect();
    for id in registry::reload_order(&root_id, &graph) {
        let dep_idx = modules.borrow().iter().position(|m| m.id == id);
        let Some(dep_idx) = dep_idx else { continue };
        match reload_module(shared, modules, dep_idx) {
            Ok(_) => report.reloaded.push(id),
            Err(e) => {
                logging::line(
                    "manager",
                    &format!("cascading reload of dependent '{id}' failed: {e:#}"),
                );
                report.failed.push((id, format!("{e:#}")));
            }
        }
    }
    logging::line(
        "manager",
        &format!(
            "reloaded '{root_id}' and {} dependent(s){}",
            report.reloaded.len() - 1,
            if report.failed.is_empty() {
                String::new()
            } else {
                format!(", {} failed", report.failed.len())
            }
        ),
    );
    Ok(report)
}

/// Builds the manager's display row for module `idx`: its settings (schema +
/// stored/default values), enabled state, and declared dependencies. Used both
/// for the startup snapshot and when a hot-loaded module is added to the list.
fn module_info(shared: &Shared, m: &Module, idx: usize) -> gui::ModuleInfo {
    let mut settings: Vec<gui::SettingDesc> = {
        let schemas = shared.schemas.borrow();
        let store = shared.store.borrow();
        schemas
            .get(idx)
            .map(|map| {
                map.iter()
                    .map(|(key, f)| gui::SettingDesc {
                        key: key.clone(),
                        label: f.label.clone(),
                        kind: f.kind,
                        value: store.get(&m.id, key).unwrap_or_else(|| f.default.clone()),
                        min: f.min,
                        max: f.max,
                        choices: f.choices.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    settings.sort_by(|a, b| a.label.cmp(&b.label));
    gui::ModuleInfo {
        name: m.name.clone(),
        version: m.version.clone(),
        id: m.id.clone(),
        module_idx: Some(idx),
        enabled: shared.enabled.borrow().get(idx).copied().unwrap_or(true),
        dependencies: m.dependencies.clone(),
        settings,
        unsupported: None,
    }
}

/// The rows for modules this platform will not run: listed, and refusing everything that
/// would need a VM they do not have.
fn excluded_infos(shared: &Shared) -> Vec<gui::ModuleInfo> {
    shared
        .excluded
        .borrow()
        .iter()
        .map(|e| gui::ModuleInfo {
            name: e.name.clone(),
            version: e.version.clone(),
            id: e.id.clone(),
            module_idx: None,
            enabled: false,
            dependencies: Vec::new(),
            settings: Vec::new(),
            unsupported: Some(e.claimed.clone()),
        })
        .collect()
}

/// A module that was found and deliberately not loaded, because its manifest says it does not
/// run here. Everything the manager needs to show it and to remove it.
struct ExcludedModule {
    id: String,
    name: String,
    version: String,
    /// The platforms it does claim, as written.
    claimed: String,
}

/// Loads and runs many modules concurrently in one process.
pub struct Manager {
    shared: Rc<Shared>,
    modules: Rc<RefCell<Vec<Module>>>,
    /// Module ids the user disabled in a previous run (from the portable config).
    disabled_ids: HashSet<String>,
    /// Module ids currently being loaded — for dependency-cycle detection.
    loading: HashSet<String>,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let backend = backend::platform();
        // Timed because it is not free and it is not obvious: building the fallback speech
        // engine measured 3.1 seconds in a test, and this call is synchronous — that is
        // start-up time the user waits through, for a voice that on a machine with a screen
        // reader will very likely never say anything.
        let began = Instant::now();
        let speech = speech::Speech::new()?;
        logging::line(
            "speech",
            &format!("the speech engines took {} ms to open", began.elapsed().as_millis()),
        );
        // What the backend sees of this machine, before anything else can fail. On a
        // machine we cannot touch — and increasingly that is the case — this block is the
        // difference between "it does not work" and a cause.
        //
        // After the speech engine, not before: part of the report is which screen reader
        // answered, and none has been asked until the engine is up.
        logging::report("env", &backend.environment());
        let store = settings::Store::load();
        let disabled_ids = store.disabled_ids();
        let (image_tasks, image_task_rx) = std::sync::mpsc::channel::<ImageTask>();
        let (image_result_tx, image_results) = std::sync::mpsc::channel::<ImageResult>();
        // Capture fn pointer taken before `backend` is moved into Shared — it's `Copy`
        // and `Send`, so the worker can capture without the non-`Send` Rc backend.
        spawn_image_worker(backend.capture_fn(), image_task_rx, image_result_tx);
        let shared = Rc::new(Shared {
            backend,
            speech,
            reload_hotkey_id: Cell::new(0),
            reload_all: Cell::new(false),
            audio: RefCell::new(None),
            next_id: Cell::new(0),
            roots: RefCell::new(Vec::new()),
            ids: RefCell::new(Vec::new()),
            enabled: RefCell::new(Vec::new()),
            caps: RefCell::new(Vec::new()),
            excluded: RefCell::new(Vec::new()),
            hotkeys: RefCell::new(HashMap::new()),
            keys: RefCell::new(Vec::new()),
            store: RefCell::new(store),
            schemas: RefCell::new(Vec::new()),
            on_change: RefCell::new(HashMap::new()),
            dirty: Cell::new(false),
            timers: RefCell::new(Vec::new()),
            exports: RefCell::new(HashMap::new()),
            arbiter: RefCell::new(HashMap::new()),
            observations: RefCell::new(Observations::default()),
            ev_counts: Cell::new((0, 0, 0, 0)),
            epoch: Cell::new(0),
            input_epoch: Cell::new(0),
            next_arbiter: Cell::new(0),
            next_key_token: Cell::new(0),
            recurring: RefCell::new(Vec::new()),
            errors: RefCell::new(Vec::new()),
            error_seen: RefCell::new(HashSet::new()),
            image_tasks,
            image_results,
            pending_image: RefCell::new(HashMap::new()),
            next_image_id: Cell::new(0),
            template_cache: RefCell::new(HashMap::new()),
            template_seq: Cell::new(0),
            recheck_requested: Cell::new(false),
            notify: RefCell::new(None),
        });
        // Claimed before the first module is loaded, so it is the one id no module can be
        // given (see the field). Registering with the OS happens later, in `run`, on the
        // thread that will receive it.
        shared.reload_hotkey_id.set(shared.alloc_id());
        Ok(Self {
            shared,
            modules: Rc::new(RefCell::new(Vec::new())),
            disabled_ids,
            loading: HashSet::new(),
        })
    }

    /// Loads a module from an unpacked directory and runs its entry point.
    /// Declared dependencies are loaded first (auto-discovered among sibling
    /// module directories), so their exports are available via `host.require`.
    pub fn load(&mut self, dir: impl AsRef<Path>) -> Result<()> {
        load_module(
            &self.shared,
            &self.modules,
            &self.disabled_ids,
            &mut self.loading,
            dir.as_ref(),
        )?;
        Ok(())
    }

    /// Says that a module did not load, and lets the rest of the application carry on.
    ///
    /// A module that will not load must not take the application with it. That was always
    /// the rule for a hot-load — `load_module` rolls back every registration a partial load
    /// made, and the comment there calls startup different "by design" — but the design was
    /// wrong. A blind user whose application vanishes at launch is left with a process that
    /// is simply gone and a log somebody has to talk them through finding, while the one
    /// place that could disable or remove the offending module is the window that never
    /// opened.
    ///
    /// So: a log line, and the same queue the module-error report drains, which puts it in
    /// front of the user as an accessible window as soon as there is one. Not a modal any
    /// more — that queue is drained into a top-level frame (`report_error` in `gui.rs`), so a
    /// load failure no longer stops the application while it waits to be read.
    ///
    /// Note that this pushes STRAIGHT onto the queue rather than through `queue_dialog`, so
    /// it is not deduped: one failing code dependency reports once per dependent that could
    /// not load because of it. They land in one window, in one tick, which is why that window
    /// gives every report its own heading.
    fn report_load_failure(&self, dir: &str, e: &anyhow::Error) {
        let name = std::path::Path::new(dir)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string());
        logging::line(
            "manager",
            &format!("module '{name}' did not load; the others carry on without it: {e:#}"),
        );
        self.shared.errors.borrow_mut().push((
            "A module did not load".to_string(),
            format!(
                "\u{201c}{name}\u{201d} is not running. Everything else loaded, and you can \
                 disable or remove it from the module manager.\n\n{e:#}"
            ),
        ));
    }

    /// Runs the shared event loop if any module has something to wait for; otherwise waits
    /// for pending speech and returns.
    pub fn run(&mut self) -> Result<()> {
        let has_hotkeys = !self.shared.hotkeys.borrow().is_empty();
        let has_keys = !self.shared.keys.borrow().is_empty();
        let has_triggers = self.modules.borrow().iter().any(|m| window_has_triggers(&m.lua));
        // A timer or an outstanding image search is a reason to keep running too, and this
        // is the second half of a bug found by measurement: a headless module that armed
        // `host.timer.every(500, …)` in `activate` logged that line and then nothing at all,
        // because the condition below decided it had nothing to wait for and fell through to
        // waiting on speech. Nothing was broken inside the loop — the loop was never entered.
        //
        // The condition was right when it was written: `host.timer` fired from the GUI tick
        // only, so headless genuinely had nothing to run for except an OS trigger. It stopped
        // being right when the tick grew a headless counterpart, and nothing failed loudly
        // enough to say so — which is what makes it worth naming here. A module's own reasons
        // to be alive are not only the ones the OS delivers.
        let has_own_work = !self.shared.timers.borrow().is_empty()
            || !self.shared.recurring.borrow().is_empty()
            || !self.shared.pending_image.borrow().is_empty();
        let headless = appcfg::headless();

        // The tray manager is shown whenever there's a window (non-headless), even
        // with nothing loaded yet, so modules can be browsed/installed/managed.
        // Headless has no window, so it only runs when something is actually pending.
        if has_hotkeys || has_keys || has_triggers || has_own_work || !headless {
            if has_triggers {
                self.shared
                    .backend
                    .watch_foreground()
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("failed to watch foreground windows")?;
            }
            if headless {
                // No window: block on the platform message loop. Same event
                // delivery as the GUI path; useful for testing/automation.
                logging::line("manager", "listening for events (headless)");
                let backend = self.shared.backend.clone();
                let mods = self.modules.borrow();
                let mut dispatcher = Dispatcher {
                    shared: &self.shared,
                    modules: &mods[..],
                };
                backend
                    .run_event_loop(&mut dispatcher)
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("event loop failed")?;
                self.shared.flush_if_dirty();
            } else {
                // wxWidgets owns the loop. Snapshot the module list for the tray
                // manager window, then drain our OS events from its timer tick.
                logging::line("manager", "module manager running in the system tray");
                // Registered HERE, not in `new`: on Windows a hotkey is delivered to the
                // thread that registered it, and this is the thread whose loop pumps below.
                // A failure is logged rather than fatal — the combination could already be
                // held by something else, and everything apart from this key still works.
                // Logged either way: the key has no visible presence at all, so "did it
                // even register" is otherwise unanswerable after the fact.
                let reload_id = self.shared.reload_hotkey_id.get();
                match self.shared.backend.register_hotkey(reload_id, RELOAD_HOTKEY_SPEC) {
                    Ok(()) => logging::line(
                        "manager",
                        &format!("{RELOAD_HOTKEY_SPEC} reloads every module"),
                    ),
                    Err(e) => logging::line(
                        "manager",
                        &format!("reload hotkey {RELOAD_HOTKEY_SPEC} unavailable: {e}"),
                    ),
                }
                let mut module_infos: Vec<gui::ModuleInfo> = self
                    .modules
                    .borrow()
                    .iter()
                    .enumerate()
                    .map(|(i, m)| module_info(&self.shared, m, i))
                    .collect();
                module_infos.extend(excluded_infos(&self.shared));
                // What the window will contain, said once. The Installed list is the only
                // place a module can be reached from, and a module that is missing from it is
                // invisible to somebody who cannot look — so the count belongs in the log the
                // tester sends, not only on the screen they cannot read.
                let cannot_run = module_infos.iter().filter(|m| m.unsupported.is_some()).count();
                logging::line(
                    "manager",
                    &format!(
                        "the installed list has {} row(s){}",
                        module_infos.len(),
                        if cannot_run > 0 {
                            format!(
                                ", {cannot_run} of them for module(s) this platform will not run — listed so they can still be removed"
                            )
                        } else {
                            String::new()
                        }
                    ),
                );
                let backend = self.shared.backend.clone();
                let shared = self.shared.clone();
                let speak_shared = self.shared.clone();
                let toggle_shared = self.shared.clone();
                let set_shared = self.shared.clone();
                let modules = self.modules.clone();
                let load_shared = self.shared.clone();
                let load_modules = self.modules.clone();
                let load_disabled = self.disabled_ids.clone();
                let remove_shared = self.shared.clone();
                let reload_shared = self.shared.clone();
                let reload_modules = self.modules.clone();
                let errors_shared = self.shared.clone();
                // Before the window system starts: wxWidgets activates an agent application
                // at launch on purpose, and this is the last moment at which we can still
                // see who it is about to take the front from.
                #[cfg(target_os = "macos")]
                crate::backend::note_frontmost_before_gui();
                gui::run_gui(
                    module_infos,
                    move |idx, id: String, enabled| match idx {
                        Some(i) => toggle_shared.set_enabled(i, enabled),
                        None => toggle_shared.set_enabled_unloaded(&id, enabled),
                    },
                    move |idx, key, value| set_shared.set_setting(idx, &key, value),
                    // Hot-load a freshly installed module into the running app so
                    // it's usable without a restart. Returns the module id, or an
                    // error string if loading failed.
                    move |dir: std::path::PathBuf| {
                        let before = load_modules.borrow().len();
                        match load_module(
                            &load_shared,
                            &load_modules,
                            &load_disabled,
                            &mut HashSet::new(),
                            &dir,
                        ) {
                            // Report every newly loaded module, not just the target
                            // — a hot-load can pull in not-yet-loaded dependencies,
                            // which must get list rows too. They're appended in load
                            // order (dependencies before the module that needs them).
                            Ok((id, true)) => {
                                let mods = load_modules.borrow();
                                let infos = (before..mods.len())
                                    .map(|i| module_info(&load_shared, &mods[i], i))
                                    .collect::<Vec<_>>();
                                Ok((id, infos))
                            }
                            Ok((id, false)) => Ok((id, Vec::new())),
                            Err(e) => Err(format!("{e:#}")),
                        }
                    },
                    // Disable a module at runtime when its files are removed (revoke
                    // its hotkeys/keys/triggers) — no restart needed, and not
                    // persisted, so re-installing it later loads it enabled again.
                    move |idx| {
                        remove_shared.apply_enabled(idx, false);
                    },
                    // Reload the selected module's VM in place from its source dir --
                    // and every module that depends on it, since each holds a COPY of its
                    // code and would otherwise keep running the old one.
                    move |idx| {
                        let report = reload_module_tree(&reload_shared, &reload_modules, idx)
                            .map_err(|e| format!("{e:#}"))?;
                        let info = {
                            let mods = reload_modules.borrow();
                            module_info(&reload_shared, &mods[idx], idx)
                        };
                        Ok((info, report))
                    },
                    move || {
                        // How long ONE pump iteration takes, reported when it runs long.
                        //
                        // This is not a general performance counter — it watches a specific
                        // hazard. The low-level keyboard hook (WH_KEYBOARD_LL) is installed
                        // on THIS thread (backend/windows.rs, watch_keys), and Windows calls
                        // a low-level hook back on its owning thread. A thread busy inside a
                        // poll callback cannot answer, and past LowLevelHooksTimeout (300 ms
                        // by default) Windows stops waiting and delivers the keystroke
                        // WITHOUT us — so a Tab the overlay believed it had captured lands
                        // in the plugin instead, intermittently, with nothing logged.
                        //
                        // For a user who navigates entirely by Tab and cannot see where the
                        // focus went, that is not a performance problem, it is a correctness
                        // one. Measured landmark polls have already been seen at 400 ms.
                        let pump_started = std::time::Instant::now();
                        let mods = modules.borrow();
                        let mut dispatcher = Dispatcher {
                            shared: &shared,
                            modules: &mods[..],
                        };
                        // Broken down by phase, because "one iteration took 729 ms" names a
                        // symptom and no cause — and the cause moved once already: the poll
                        // was the whole story until the observation cache took it out, after
                        // which the overruns lined up with window switches instead.
                        shared.ev_counts.set((0, 0, 0, 0));
                        backend.pump_pending(&mut dispatcher);
                        let events_ms = pump_started.elapsed().as_millis();
                        let (act_n, act_ms, focus_ms, _) = shared.ev_counts.get();
                        // What the two named phases do NOT account for, printed rather than
                        // left to be inferred as zero.
                        //
                        // `ev_counts` is written in two places only — the window-activate and
                        // focus-change handlers — while `pump_pending` also dispatches every
                        // KEY and every HOTKEY into every module's Lua. So the cost of what
                        // the pump mostly does while somebody is pressing Tab had no term of
                        // its own, and the line printed an equation that did not balance: all
                        // thirty-one stalls in the macOS tester's log read "= 0x
                        // window-activate 0 + focus-change 0", which reads as "the cause is
                        // none of these" when it means "the cause is not measured".
                        let other_ms = events_ms.saturating_sub(act_ms).saturating_sub(focus_ms);
                        let t = std::time::Instant::now();
                        shared.fire_due_timers();
                        let timers_ms = t.elapsed().as_millis();
                        let t = std::time::Instant::now();
                        shared.fire_image_results();
                        let images_ms = t.elapsed().as_millis();
                        let pump_ms = pump_started.elapsed().as_millis();
                        if pump_ms >= 250 {
                            // The hazard is different on each platform, and the line has to
                            // name the right one: a Mac tester's log full of "Windows stops
                            // waiting for our keyboard hook" was a puzzle before it was a
                            // diagnosis. On macOS the system switches off an event tap whose
                            // thread stops answering; the watchdog in tap.rs re-enables it
                            // and logs that it did, so the two lines can be read together.
                            #[cfg(windows)]
                            let hazard = "past ~300 ms Windows stops waiting for our \
                                          keyboard hook and delivers the key without us";
                            #[cfg(target_os = "macos")]
                            let hazard = "a stall this long is what gets the event tap \
                                          switched off, and keys go uncaptured until the \
                                          watchdog re-enables it";
                            #[cfg(not(any(windows, target_os = "macos")))]
                            let hazard = "keys can be delivered without us while the pump \
                                          is this busy";
                            logging::line(
                                "pump",
                                &format!(
                                    "one iteration took {pump_ms} ms (os events {events_ms} \
                                     = {act_n}x window-activate {act_ms} + focus-change \
                                     {focus_ms} + everything else {other_ms}, which is \
                                     mostly key and hotkey dispatch; timers {timers_ms}, \
                                     image results {images_ms}) — {hazard}"
                                ),
                            );
                        }
                        // A module asked for a re-check (host.window.recheck) after
                        // changing plugin UI itself — dispatch it like an OS focus event.
                        if shared.recheck_requested.replace(false) {
                            dispatcher.on_focus_change();
                        }
                        shared.flush_if_dirty();
                        // Anything the screen reader turned down, said by the fallback —
                        // here rather than at the call site, because the answer arrives from
                        // another thread a moment after the line was handed over.
                        shared.speech.pump();
                        // The reload key only ASKED (see Shared::reload_all). Answering it
                        // means replacing entries in the very list the dispatch above holds
                        // borrowed, so it happens here, once that borrow is gone.
                        if shared.reload_all.replace(false) {
                            drop(dispatcher);
                            drop(mods);
                            // The report, and nothing before it. There used to be a
                            // "Reloading modules" first, because rebuilding every VM takes
                            // long enough that silence would read as "the key did nothing" —
                            // but that only works if it arrives at once, and it was written
                            // for a channel where it did. Windows shows notifications when it
                            // is ready to, so both of them turned up late and together, which
                            // is two interruptions where one would do and no reassurance at
                            // all. The reassurance is gone either way; the noise need not be.
                            let (done, failed) = reload_everything(&shared, &modules);
                            shared.announce(&reload_report_text(&done, &failed));
                        }
                    },
                    move || errors_shared.drain_errors(),
                    move |text: &str| speak_shared.announce(text),
                    {
                        let shared = self.shared.clone();
                        move |show: Box<dyn Fn(&str, &str) -> bool>| {
                            *shared.notify.borrow_mut() = Some(show);
                        }
                    },
                )
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("wx GUI loop failed")?;
                self.shared.flush_if_dirty();
            }
        } else {
            self.wait_for_speech();
            self.shared.flush_if_dirty();
        }
        Ok(())
    }

    fn wait_for_speech(&self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.shared.speech.pump();
            let speaking = self.shared.speech.is_speaking();
            if !speaking || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The folder the application lives in — where its log, settings and modules are.
///
/// Exposed because the launcher needs the same answer the host uses; two rules that agreed
/// on Windows and disagreed inside a macOS application bundle is exactly the bug this
/// module exists to prevent.
pub fn app_dir() -> &'static std::path::Path {
    portable::base_dir()
}

/// Convenience entry: load each directory as a module and run them together.
pub fn run(dirs: &[String]) -> Result<()> {
    // The settings before the log, because the log's own header reports them — and before
    // anything else, because `headless` decides whether there is going to be a window at all.
    // Read straight from the file rather than through the Manager's store: this happens
    // before a Manager exists, and the two read the same file.
    appcfg::load(|key| settings::Store::load().app_flag(key));
    logging::init();
    let warmup = backend::warmup_ocr(); // preload the neural OCR model off the hot path
    let result = (|| -> Result<()> {
        let mut manager = Manager::new()?;
        for dir in dirs {
            if let Err(e) = manager.load(dir) {
                manager.report_load_failure(dir, &e);
            }
        }
        manager.run()
    })();
    // Join the OCR warmup before returning: otherwise its background thread can be
    // mid native ONNX-Runtime init when the process tears down, racing ort's static
    // cleanup → an access violation that surfaces as the headless / fast-exit
    // "segfault". The thread is bounded (load the model + one dummy inference).
    if let Some(h) = warmup {
        let _ = h.join();
    }
    // Whatever went wrong, it goes in the log before it goes anywhere else.
    //
    // Returning the error is enough on a developer's machine, where it lands in a terminal.
    // It is enough nowhere else: a windowed process has no stderr worth the name, and an
    // application launched from a macOS Finder writes it to the unified log, which a blind
    // tester will never be asked to find. Without this line the most likely first-run
    // failures — no speech engine, a module directory that will not load — produce a log
    // that contains the session header and then simply stops.
    if let Err(e) = &result {
        logging::line("host", &format!("fatal: {e:#}"));
    }
    result
}

/// Deliberately awkward. This rebuilds every module's VM while the user is working in some
/// other application, so it must be impossible to hit by accident; and the modules themselves
/// claim ordinary combinations, so it has to stay out of their way.
///
/// Two spellings, because the awkward one cannot be pressed on a Mac. Win and Alt become
/// Command and Option there, and Control-Option together is VoiceOver's own modifier:
/// VoiceOver takes those chords in the window server, above anything an application can
/// register or tap, and answers an unassigned one with its error sound. The third Mac session
/// measured exactly that — registered on both launches, never delivered, a beep for every
/// press — on a Mac with the default VoiceOver modifier, after two sessions on another Mac
/// where the same chord had arrived. Command-Shift is what the probe's key uses, and that one
/// arrived four times out of four on the same machine.
const RELOAD_HOTKEY_SPEC: &str =
    if cfg!(target_os = "macos") { "Cmd+Shift+F5" } else { "Ctrl+Shift+Win+Alt+F5" };

/// Rebuild every module from source, dependencies before dependents.
///
/// Order is the whole difficulty. A dependent holds a COPY of a code dependency's functions —
/// that is what `host.require` does for a code module — so rebuilding a dependent before its
/// dependency copies the old code and the change stays invisible until a restart, which is
/// exactly the symptom this is meant to remove.
///
/// A module that fails to rebuild is reported and skipped rather than aborting the rest: the
/// others do not depend on it, and stopping halfway leaves more of the system stale than
/// carrying on does.
fn reload_everything(
    shared: &Rc<Shared>,
    modules: &Rc<RefCell<Vec<Module>>>,
) -> (Vec<String>, Vec<String>) {
    let graph: Vec<(String, Vec<String>)> =
        modules.borrow().iter().map(|m| (m.id.clone(), m.dependencies.clone())).collect();

    // Dependencies first: repeatedly take whatever has no unbuilt dependency left. A cycle
    // would stall this, so anything still unplaced at the end is appended in its own order —
    // rebuilding it in the wrong order is better than not rebuilding it at all.
    let mut order: Vec<String> = Vec::new();
    let mut placed: HashSet<String> = HashSet::new();
    loop {
        let mut progressed = false;
        for (id, deps) in &graph {
            if placed.contains(id) {
                continue;
            }
            if deps.iter().all(|d| placed.contains(d) || !graph.iter().any(|(g, _)| g == d)) {
                order.push(id.clone());
                placed.insert(id.clone());
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    for (id, _) in &graph {
        if !placed.contains(id) {
            order.push(id.clone());
        }
    }

    // Names rather than ids from here on: the result is spoken, and
    // "com.platform.kontakt" is not something anyone wants to hear read out.
    let (mut done, mut failed) = (Vec::new(), Vec::new());
    for id in order {
        let found = modules.borrow().iter().position(|m| m.id == id);
        let Some(idx) = found else { continue };
        let name = modules.borrow()[idx].name.clone();
        match reload_module(shared, modules, idx) {
            Ok(_) => done.push(name),
            Err(e) => {
                logging::line("manager", &format!("reload of '{id}' failed: {e:#}"));
                failed.push(name);
            }
        }
    }
    logging::line(
        "manager",
        &format!(
            "reload-all: {} reloaded{}",
            done.len(),
            if failed.is_empty() {
                String::new()
            } else {
                format!(", {} failed: {}", failed.len(), failed.join(", "))
            }
        ),
    );
    (done, failed)
}

/// Announce the outcome of a reload-all.
///
/// Spoken, not shown: the key can be pressed from inside any application, and the tray
/// window is usually not open — a silent rebuild is indistinguishable from a key that did
/// not arrive. Failures are named because that is the part that needs acting on; successes
/// are counted because eleven module names is not feedback, it is a recital.
fn reload_report_text(done: &[String], failed: &[String]) -> String {
    match (done.len(), failed.len()) {
        (0, 0) => "No modules to reload".to_string(),
        (n, 0) => format!("{n} module{} reloaded", if n == 1 { "" } else { "s" }),
        (0, _) => format!("Reload failed: {}", failed.join(", ")),
        (n, _) => format!("{n} reloaded, {} failed: {}", failed.len(), failed.join(", ")),
    }
}

/// Bridges OS events from the backend into the owning module's Luau callbacks.
struct Dispatcher<'a> {
    shared: &'a Shared,
    modules: &'a [Module],
}

impl Dispatcher<'_> {
    fn enabled(&self, idx: usize) -> bool {
        self.shared.enabled.borrow().get(idx).copied().unwrap_or(false)
    }
}

impl HostEvents for Dispatcher<'_> {
    /// The headless loop's equivalent of the GUI timer tick, and now actually equivalent.
    ///
    /// It used to pump speech and nothing else, with a comment saying the two missing calls
    /// were "a separate, older gap". They were — and the gap was wider than the comment: the
    /// Windows headless loop had no cadence at all (it blocked in `GetMessageW`, and nothing
    /// ever sent it a message), so this function was not being called late, it was not being
    /// called. Measured before touching it: a module arming `host.timer.every(500, …)` logged
    /// its `activate` line and then nothing for ten seconds.
    ///
    /// The three calls are in the same order as the GUI path's — OS events are drained by the
    /// loop before this runs, then timers, then image results — because a timer that fires
    /// before the window activation it was waiting for reads as a race in module code.
    ///
    /// Not instrumented the way the GUI path is (which breaks its iteration into phases and
    /// logs anything over 250 ms). Worth adding when there is a reason to look; a headless
    /// session is a test harness, not somebody's desktop.
    fn on_tick(&mut self) {
        self.shared.speech.pump();
        self.shared.fire_due_timers();
        self.shared.fire_image_results();
    }

    fn on_hotkey(&mut self, id: i32) {
        self.shared.bump_epoch();
        // Logged on ARRIVAL, before anything is looked up. Registration and delivery are
        // different things: `RegisterHotKey` can succeed while the keystroke still never
        // reaches us, because a low-level hook ahead of ours in the chain — a screen reader
        // installs one — can consume it first, and nothing in the chain reports that. With
        // this line, "the overlay holds Alt+V but the DAW acted on it" splits cleanly into
        // "we never got it" and "we got it and did the wrong thing".
        {
            let spec = self
                .shared
                .hotkeys
                .borrow()
                .get(&id)
                .map(|r| r.spec.clone())
                .unwrap_or_else(|| format!("id {id}"));
            logging::line("keys", &format!("hotkey {spec} arrived"));
        }
        // The app's own key, before any module lookup: it belongs to no module, so it is not in
        // that map and would otherwise fall through as an unknown id.
        if id == self.shared.reload_hotkey_id.get() {
            self.shared.reload_all.set(true);
            return;
        }
        let found = {
            let map = self.shared.hotkeys.borrow();
            map.get(&id).and_then(|reg| {
                if self.enabled(reg.module_idx) {
                    reg.lua
                        .registry_value::<Function>(&reg.cb)
                        .ok()
                        .map(|f| (reg.module_idx, f))
                } else {
                    None
                }
            })
        };
        if let Some((idx, f)) = found {
            if let Err(e) = call_guarded(&f, ()) {
                self.shared.report_callback_error(idx, "hotkey", &e);
            }
        }
    }

    fn on_key(&mut self, vk: u32, mods: u8) {
        self.shared.bump_epoch();
        // The hook only calls this for keys in the captured set, so the presence of this line
        // IS the answer to "did the overlay swallow that keystroke, or did the application
        // simply do nothing with it" — the two are indistinguishable from the outside, and for
        // a user who cannot see the screen they are indistinguishable from each other twice
        // over. Behind trace, for the reasons written at refresh_captured.
        if appcfg::trace() || appcfg::calibrate() {
            logging::line("keys", &format!("dispatch vk 0x{vk:02X}/m{mods}"));
        }
        let found = {
            let keys = self.shared.keys.borrow();
            keys.iter()
                .find(|(k, m, idx, ..)| *k == vk && *m == mods && self.enabled(*idx))
                .and_then(|(_, _, idx, _tok, lua, key)| {
                    lua.registry_value::<Function>(key).ok().map(|f| (*idx, lua.clone(), f))
                })
        };
        if let Some((idx, lua, f)) = found {
            let table = lua.create_table().ok();
            if let Some(t) = &table {
                let _ = t.set("shift", mods & backend::MASK_SHIFT != 0);
                let _ = t.set("ctrl", mods & backend::MASK_CTRL != 0);
                let _ = t.set("alt", mods & backend::MASK_ALT != 0);
                let _ = t.set("win", mods & backend::MASK_WIN != 0);
            }
            let res = match table {
                Some(t) => call_guarded(&f, t),
                None => call_guarded(&f, ()),
            };
            if let Err(e) = res {
                self.shared.report_callback_error(idx, "key", &e);
            }
        }
    }

    fn on_window_activate(&mut self, win: WinInfo) {
        let started = Instant::now();
        // A different window in front is a different screen — this counts as the screen
        // having changed, not merely the world (see bump_input_epoch).
        self.shared.bump_input_epoch();
        for (idx, m) in self.modules.iter().enumerate() {
            if !self.enabled(idx) {
                continue;
            }
            let table = match win_to_table(&m.lua, &win) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if let Err(e) = guard(|| {
                let host: Table = m.lua.globals().get("host")?;
                let window: Table = host.get("window")?;
                let dispatch: Function = window.get("_dispatchActivate")?;
                dispatch.call::<()>(table)
            }) {
                self.shared.report_callback_error(idx, "window trigger", &e);
            }
        }
        let mut c = self.shared.ev_counts.get();
        c.0 += 1;
        c.1 += started.elapsed().as_millis();
        self.shared.ev_counts.set(c);
    }

    fn on_focus_change(&mut self) {
        let started = Instant::now();
        self.shared.bump_epoch();
        for (idx, m) in self.modules.iter().enumerate() {
            if !self.enabled(idx) {
                continue;
            }
            if let Err(e) = guard(|| {
                let host: Table = m.lua.globals().get("host")?;
                let window: Table = host.get("window")?;
                let dispatch: Function = window.get("_dispatchFocus")?;
                dispatch.call::<()>(())
            }) {
                self.shared.report_callback_error(idx, "focus change", &e);
            }
        }
        let mut c = self.shared.ev_counts.get();
        c.2 += started.elapsed().as_millis();
        self.shared.ev_counts.set(c);
    }
}

/// Which `host.<key>` a manifest's capability name unlocks.
///
/// Everything not in here is free: `os`, `require`, `tryRequire`, `include`, `epoch`, `now`,
/// `inputEpoch`, `calibrating`, `match`. A clock, a counter, a platform name and a way to
/// reach a declared dependency are not worth asking permission for, and gating them would
/// mean every manifest names them, which is the same as naming none.
///
/// `config` is the second name of the settings table, so it answers to the same capability
/// rather than to one of its own — nothing declares `"config"` and nothing should have to.
const GATED: &[(&str, &str)] = &[
    ("window", "window"),
    ("screen", "screen"),
    ("ocr", "ocr"),
    ("element", "element"),
    ("input", "input"),
    ("keys", "keys"),
    ("hotkey", "hotkey"),
    ("speech", "speech"),
    ("sound", "sound"),
    ("timer", "timer"),
    ("settings", "settings"),
    ("config", "settings"),
    ("log", "log"),
    ("path", "path"),
    ("resource", "resource"),
    ("arbiter", "arbiter"),
];

/// The capability a `host.<key>` access needs, or `None` when it needs none.
fn capability_for(key: &str) -> Option<&'static str> {
    GATED.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
}

/// The refusal a module gets for a namespace it did not declare.
///
/// A missing table would surface three frames away as "attempt to index a nil value", naming
/// neither the module nor what it forgot to ask for. `note` carries the one thing that is not
/// obvious when the code is a dependency's: the declaration that counts is the author's, not
/// the VM owner's.
fn undeclared(id: &str, key: &str, note: &str) -> mlua::Error {
    let cap = capability_for(key).unwrap_or(key);
    mlua::Error::external(format!(
        "module '{id}' used host.{key} without declaring it — add \"{cap}\" to \
         [capabilities] require in its module.toml{note}"
    ))
}

/// Does `exported` hand `host_dep` — the dependency's own host table — back to the module
/// that required it?
///
/// A named function rather than four lines inline, because it is the rule that decides
/// whether a dependency's export is an API or a capability handover, and a rule nobody can
/// call is a rule nobody can test.
///
/// Deliberately shallow, and by identity. A closure that returns the table when called
/// cannot be caught by any amount of scanning — see
/// `privileged_luau_that_hands_out_its_host_defeats_the_gate` — so this catches the accident
/// and does not pretend to be a boundary.
fn hands_over_its_host(exported: &Table, host_dep: &Table) -> bool {
    if exported == host_dep {
        return true;
    }
    exported
        .clone()
        .pairs::<mlua::Value, mlua::Value>()
        .flatten()
        .any(|(_, v)| matches!(v, mlua::Value::Table(t) if &t == host_dep))
}

/// A view over `full` that refuses the gated namespaces `caps` does not name.
///
/// A VIEW rather than a trimmed table, because `full` is also what a code dependency's
/// fall-through binds to. Cutting a namespace out of it took that binding away from a
/// dependency that was entitled to it whenever the dependent had not declared the same thing —
/// which is backwards: permission belongs to the module that wrote the code.
///
/// An unknown key still answers nil. That is a typo, not a permission problem.
fn gated_view(lua: &Lua, full: &Table, caps: &HashSet<String>, id: &str) -> Result<Table> {
    let denied: HashSet<String> = GATED
        .iter()
        .filter(|(_, cap)| !caps.contains(*cap))
        .map(|(key, _)| (*key).to_string())
        .collect();
    if denied.is_empty() {
        return Ok(full.clone());
    }
    let owner = id.to_string();
    let source = full.clone();
    let view = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |_, (_t, key): (Table, String)| -> mlua::Result<mlua::Value> {
            if denied.contains(&key) {
                return Err(undeclared(&owner, &key, ""));
            }
            source.raw_get(key)
        })?,
    )?;
    // Anything a module assigns to `host.x` lands on the view rather than on the table every
    // other module shares, which is the right way round: one module must not be able to change
    // what another one sees.
    view.set_metatable(Some(mt))?;
    Ok(view)
}

fn install_host_api(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> Result<Table> {
    let host = lua.create_table()?;

    // host.log.info(msg)
    let log = lua.create_table()?;
    log.set(
        "info",
        lua.create_function(|_, msg: String| {
            logging::line("module", &msg);
            Ok(())
        })?,
    )?;
    host.set("log", log)?;

    // host.require(id) — access a declared dependency. A `code_module` dependency
    // is evaluated *inside this VM* (top-down loading), so this returns its module
    // object with functions intact (looked up in the per-VM `__module_exports`
    // registry). Otherwise it falls back to the legacy cross-VM path: the
    // dependency's serialized data export (functions can't cross VMs that way).
    let sh = shared.clone();
    host.set(
        "require",
        lua.create_function(move |lua, id: String| -> mlua::Result<mlua::Value> {
            if let Ok(reg) = lua.named_registry_value::<mlua::Table>("__module_exports") {
                let m: mlua::Value = reg.get(id.as_str())?;
                if !m.is_nil() {
                    return Ok(m);
                }
            }
            let exports = sh.exports.borrow();
            match exports.get(&id) {
                Some(v) => lua.to_value(v),
                None => Err(mlua::Error::external(format!(
                    "required module '{id}' is not loaded or exports nothing"
                ))),
            }
        })?,
    )?;

    // host.tryRequire(id) — like host.require, but returns nil instead of erroring when
    // the module isn't loaded. For an OPTIONAL dependency, so the module can adapt to
    // whether it is present: `local kk = host.tryRequire(id); if kk then … end`.
    let sh = shared.clone();
    host.set(
        "tryRequire",
        lua.create_function(move |lua, id: String| -> mlua::Result<mlua::Value> {
            if let Ok(reg) = lua.named_registry_value::<mlua::Table>("__module_exports") {
                let m: mlua::Value = reg.get(id.as_str())?;
                if !m.is_nil() {
                    return Ok(m);
                }
            }
            let exports = sh.exports.borrow();
            match exports.get(&id) {
                Some(v) => lua.to_value(v),
                None => Ok(mlua::Value::Nil),
            }
        })?,
    )?;

    // (host.providers was removed: the `provides`/contract discovery mechanism went
    // unused — the two collaboration patterns it was meant for are both solved more
    // directly by the dependency system. An extension that extends a container depends on
    // it (hard) and registers via its imported functions (e.g. a Kontakt library →
    // Kontakt, calling kontakt.library); a module that merely CAN use another declares it
    // as an OPTIONAL dependency and imports it with host.tryRequire when present (e.g.
    // Kontakt → Komplete Kontrol). Neither needs contract discovery.)

    // host.speech.output(text, { interrupt = true })  (do not echo to console:
    // a screen reader would read the terminal and double the speech)
    let speech = lua.create_table()?;
    let sh = shared.clone();
    speech.set(
        "output",
        lua.create_function(move |_, (text, opts): (String, Option<Table>)| {
            let interrupt = match opts {
                Some(t) => t.get::<bool>("interrupt").unwrap_or(true),
                None => true,
            };
            #[cfg(any(windows, target_os = "macos"))]
            sh.speech.say_for(idx, &text, interrupt);
            #[cfg(not(any(windows, target_os = "macos")))]
            sh.speech.say(&text, interrupt);
            Ok(())
        })?,
    )?;
    // host.speech.engines() -> { { id, name, screenReader, available }, … }
    //
    // What could speak on this machine, and what can right now. A module asks so that it can
    // choose; nothing is chosen by asking. Costs about 35 ms — a question, not a hot path.
    let sh = shared.clone();
    speech.set(
        "engines",
        lua.create_function(move |lua, ()| {
            let list = lua.create_table()?;
            for (i, e) in sh.speech.engines().into_iter().enumerate() {
                let t = lua.create_table()?;
                t.set("id", e.id)?;
                t.set("name", e.name)?;
                t.set("screenReader", e.screen_reader)?;
                t.set("available", e.available)?;
                list.set(i + 1, t)?;
            }
            Ok(list)
        })?,
    )?;

    // host.speech.use(id | nil) -> boolean ; host.speech.engine() -> id | nil
    //
    // A module chooses what speaks for IT. Refused when the engine is not there, because
    // accepting and quietly speaking somewhere else would be a promise not kept. Choosing
    // costs the engine's opening time on its own thread — 2.0 s for SAPI, 3.5 s for OneCore —
    // which the lines queue behind rather than being lost to.
    // Registered wherever `Speech` can honour them, which since 2026-09-04 is macOS as well.
    // These went cross-platform in `Speech` and were left behind here for one commit, which
    // made `engines()` list voices no module could select — a control claiming exactly what
    // it cannot honour, and the rule this project treats as the important one.
    #[cfg(any(windows, target_os = "macos"))]
    {
        let sh = shared.clone();
        speech.set(
            "use",
            lua.create_function(move |_, id: Option<String>| {
                Ok(sh.speech.use_engine(idx, id.as_deref()))
            })?,
        )?;
        let sh = shared.clone();
        speech.set(
            "engine",
            lua.create_function(move |_, ()| Ok(sh.speech.chosen_engine(idx)))?,
        )?;
    }

    host.set("speech", speech)?;

    // host.hotkey.register(spec, cb) -> id ; host.hotkey.unregister(id)
    let hk = lua.create_table()?;
    let sh = shared.clone();
    hk.set(
        "register",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            let binding = backend::key_spec(&spec);
            let id = sh.alloc_id();
            // Only an ENABLED module actually claims the OS combo and can clash. A
            // disabled (persisted-off) module just records its binding here so a
            // later enable registers it — it neither registers with the OS nor
            // contests a combo (avoids a spurious conflict for a module the user
            // turned off, and keeps its binding so re-enable restores it).
            // An unparseable spec is a module-author bug and is still answered here, loudly
            // and immediately, because there is nothing for `refresh_hotkeys` to derive from:
            // with no `(vk, mask)` the combination cannot be compared with anybody else's, so
            // the OS is the only authority and its error is the useful one. Matches
            // host.keys.capture.
            if binding.is_none() {
                if let Err(e) = sh.backend.register_hotkey(id, &spec) {
                    return Err(mlua::Error::external(e));
                }
            }

            // Recorded whether or not it wins the combination — and THAT is the fix. It used
            // to return here, before this insert, whenever another enabled module held the
            // combo: the callback was dropped, the module got the id 0, and nothing was left
            // to hand the key to when that other module was later disabled. A registration is
            // now a standing claim, and `refresh_hotkeys` decides which claims are live.
            let key = lua.create_registry_value(cb)?;
            sh.hotkeys.borrow_mut().insert(
                id,
                HotkeyReg {
                    module_idx: idx,
                    lua: lua.clone(),
                    cb: key,
                    spec,
                    binding,
                    // Set by the refresh below; for an unparseable spec, by the OS call above.
                    live: binding.is_none(),
                },
            );
            // Which also reports the clash, so a module that lost the contest is told once
            // here rather than in two places that could disagree.
            sh.refresh_hotkeys();
            Ok(id)
        })?,
    )?;
    let sh = shared.clone();
    hk.set(
        "unregister",
        lua.create_function(move |_, id: i32| {
            // Whatever number Lua passes goes straight to the OS, so a stale or
            // made-up id could release the app's own reload key — and nothing would
            // report it; the key would simply stop working. It is not a module's to
            // release.
            if id == sh.reload_hotkey_id.get() {
                return Ok(());
            }
            sh.backend.unregister_hotkey(id);
            sh.hotkeys.borrow_mut().remove(&id);
            // Offer the freed combination to whoever else asked for it.
            sh.refresh_hotkeys();
            Ok(())
        })?,
    )?;
    host.set("hotkey", hk)?;

    // host.keys: low-level key capture + suppression (modifier-aware)
    let keys = lua.create_table()?;
    let sh = shared.clone();
    keys.set(
        "capture",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            let (vk, mask) = backend::key_spec(&spec)
                .ok_or_else(|| mlua::Error::external(format!("unknown key spec '{spec}'")))?;
            // No cross-module conflict surfacing here: captured keys are routinely
            // shared by window-scoped overlays (each active only while its own
            // window is focused), so a duplicate is usually legitimate, not a clash.
            // The dispatcher routes to the first match and refresh_captured filters
            // by enabled; conflict dialogs are for process-wide hotkeys only.
            //
            // Within a module we keep ONE entry per (vk, mask): the latest capturer
            // wins (drop any prior same-module entry, then push). Each entry carries a
            // unique token, returned to the caller so `release` can target THIS exact
            // registration and not another overlay's re-capture of the same key.
            let key = lua.create_registry_value(cb)?;
            let token = sh.next_key_token.get() + 1;
            sh.next_key_token.set(token);
            sh.keys
                .borrow_mut()
                .retain(|(k, m, i, ..)| !(*k == vk && *m == mask && *i == idx));
            sh.keys.borrow_mut().push((vk, mask, idx, token, lua.clone(), key));
            sh.refresh_captured();
            sh.backend.watch_keys().map_err(mlua::Error::external)?;
            Ok(token)
        })?,
    )?;
    let sh = shared.clone();
    keys.set(
        "release",
        // Releases the exact registration returned by `capture` (its token). Keying on
        // the token, not (vk, mask, module_idx), means one overlay deactivating cannot
        // drop a key another overlay in the same module has since re-captured. An
        // unknown/stale token (already superseded by a later capture) is a harmless
        // no-op.
        lua.create_function(move |_, token: i64| {
            let before = sh.keys.borrow().len();
            sh.keys.borrow_mut().retain(|(.., t, _, _)| *t != token);
            if sh.keys.borrow().len() != before {
                sh.refresh_captured();
            }
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    keys.set(
        "releaseAll",
        lua.create_function(move |_, ()| {
            // Element .2 is module_idx in (vk, mask, module_idx, token, VM, callback).
            sh.keys.borrow_mut().retain(|entry| entry.2 != idx);
            sh.refresh_captured();
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    keys.set(
        "scope",
        lua.create_function(move |_, to_foreground: bool| {
            sh.backend.set_key_scope(to_foreground);
            Ok(())
        })?,
    )?;
    // host.keys.menuOpen(open) — while a plugin's own (Qt/UIA) menu is open, let
    // captured nav keys (Tab/Enter) pass through to it instead of the overlay.
    let sh = shared.clone();
    keys.set(
        "menuOpen",
        lua.create_function(move |_, open: bool| {
            sh.backend.set_menu_open(open);
            Ok(())
        })?,
    )?;
    // host.keys.nativeMenuOpen() -> bool. The CHEAP half of "is a menu open": one
    // window-class lookup for a native popup, no accessibility traversal. The menu watch
    // asks this before paying for the expensive question — measured at 50-194 ms per call,
    // every 150 ms, which is more than its own interval.
    // host.keys.modifiersDown() -> bool. For a hotkey callback that synthesises input: it
    // runs while its own combination is still held, so anything it sends carries those
    // modifiers with it. ReaHotkey's HotkeyWait solves this by waiting for release
    // (AccessibilityOverlay.ahk:1073-1075, `KeyWait` per key of the combination).
    let sh = shared.clone();
    keys.set(
        "modifiersDown",
        lua.create_function(move |_, ()| Ok(sh.backend.modifiers_down()))?,
    )?;
    let sh = shared.clone();
    keys.set(
        "nativeMenuOpen",
        lua.create_function(move |_, ()| Ok(sh.backend.native_menu_open()))?,
    )?;
    // host.keys.passedThrough() -> { {vk, mask, key} } — the captured keys the hook let
    // through to the application because a menu was open, since the last call. Drained on
    // read. The menu watch asks on its tick: a Return or Escape in here means the menu is
    // closing, which on a platform where nothing can see the menu is the only word it gets.
    let sh = shared.clone();
    keys.set(
        "passedThrough",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for (vk, mask) in sh.backend.take_menu_pass_through() {
                let k = lua.create_table()?;
                k.set("vk", vk)?;
                k.set("mask", mask)?;
                k.set("key", backend::vk_name(vk).unwrap_or_else(|| format!("vk {vk:#04x}")))?;
                t.push(k)?;
            }
            Ok(t)
        })?,
    )?;
    host.set("keys", keys)?;

    // host.now() -> milliseconds since the app started. A CLOCK, not a date: the only
    // thing modules need it for is measuring their own hot paths, and "how long did that
    // take" is exactly what nobody could answer about the overlay runtime — every
    // performance question so far had to be answered from Rust or from log timestamps a
    // second apart. Monotonic, so it cannot go backwards mid-measurement.
    let started = Instant::now();
    host.set(
        "now",
        lua.create_function(move |_, ()| Ok(started.elapsed().as_millis() as i64))?,
    )?;

    // host.inputEpoch() — a counter that turns over only when something ACTED on the
    // screen: input we drove, or a window coming forward. host.epoch() also turns over on
    // timer ticks and on the user's own keystrokes, which is right for "re-resolve where
    // the plugin is" and far too eager for "what does this pixel say". A screen read costs
    // a fixed compositor frame, so a property that only an action can change should be
    // cached against this instead. See bump_input_epoch.
    let sh_ie = shared.clone();
    host.set(
        "inputEpoch",
        lua.create_function(move |_, ()| Ok(sh_ie.input_epoch.get() as i64))?,
    )?;

    // host.timer: one-shot delayed callbacks, fired from the event-loop tick.
    let timer = lua.create_table()?;
    let sh = shared.clone();
    timer.set(
        "after",
        lua.create_function(move |lua, (ms, cb): (u64, Function)| {
            let key = lua.create_registry_value(cb)?;
            let deadline = Instant::now() + Duration::from_millis(ms);
            sh.timers.borrow_mut().push((deadline, idx, lua.clone(), key));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    timer.set(
        "every",
        // Recurring timer. Unlike a Lua self-rescheduling host.timer.after, this
        // survives a disable/enable cycle (the schedule is re-armed by the host),
        // so it's the right tool for a poll (e.g. watching for a plugin library's
        // landmark to appear).
        lua.create_function(move |lua, (ms, cb): (u64, Function)| {
            let key = lua.create_registry_value(cb)?;
            let interval = Duration::from_millis(ms.max(1));
            sh.recurring
                .borrow_mut()
                .push((Instant::now() + interval, interval, idx, lua.clone(), key));
            Ok(())
        })?,
    )?;
    host.set("timer", timer)?;

    // host.os.current / host.os.is(name)
    let os = lua.create_table()?;
    os.set("current", std::env::consts::OS)?;
    os.set(
        "is",
        lua.create_function(|_, name: String| Ok(name == std::env::consts::OS))?,
    )?;
    host.set("os", os)?;

    // host.window.list() / host.window.active() / host.window.focus(id)
    // (find/findAll/onTrigger via prelude)
    let win = lua.create_table()?;

    // host.window.focus(id) -> boolean
    //
    // Brings a window to the front. It exists because of a gap a tester named: on Windows a
    // screen-reader user returns to a plugin's window with OSARA's F6, and macOS offers no
    // such command from VoiceOver or from anywhere else — a plugin window opened inside a
    // DAW can be genuinely hard to get back to. Nothing here could offer one either, because
    // nothing here could focus a window.
    //
    // Returns whether the system accepted it, and a module must believe that answer: both
    // platforms can decline, and a module that announces "back in the plugin" when the focus
    // did not move has told somebody who cannot check that they are somewhere they are not.
    let sh = shared.clone();
    win.set(
        "focus",
        lua.create_function(move |_, id: isize| {
            let accepted = sh.backend.focus_window(id);
            // A focus change is a fresh observation of the world, so the cache that served
            // the last one is turned over — the same way a click does it. Without this, a
            // module asking `focusChain()` right after this call, to find out whether the
            // keyboard actually moved, would be answered from BEFORE the move: the two
            // reads share the epoch the hotkey dispatch began, and the answer is the one
            // thing the caller is asking for.
            sh.bump_epoch();
            Ok(accepted)
        })?,
    )?;

    // host.window.list(filter?) — every titled top-level window, or with `{ pids = {…} }`
    // only those applications' windows. The prelude's `find`/`findAll` pass the pids of the
    // applications a matcher names, because on macOS the unfiltered listing is a
    // cross-process question per running application and the answer to "where is REAPER's
    // FX window" should not wait on a process the matcher never mentioned.
    let sh = shared.clone();
    win.set(
        "list",
        lua.create_function(move |lua, filter: Option<Table>| {
            let pids: Option<Vec<u32>> = match filter {
                Some(f) => f.get::<Option<Vec<u32>>>("pids")?,
                None => None,
            };
            let t = lua.create_table()?;
            let windows = match pids {
                Some(p) => sh.backend.enumerate_windows_of(&p),
                None => sh.backend.enumerate_windows(),
            };
            for w in windows {
                t.push(win_to_table(lua, &w)?)?;
            }
            Ok(t)
        })?,
    )?;
    // host.window.apps() — the running applications, as the `app` table of a window would
    // describe them, without asking any of them anything. What a matcher's `app` clause is
    // tested against before a single window is listed.
    let sh = shared.clone();
    win.set(
        "apps",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for a in sh.backend.running_apps() {
                let app = lua.create_table()?;
                let name = a
                    .exe
                    .rsplit_once('.')
                    .map(|(stem, _)| stem.to_string())
                    .unwrap_or_else(|| a.exe.clone());
                app.set("name", name)?;
                app.set("exe", a.exe.clone())?;
                app.set("bundleId", a.bundle_id.clone())?;
                app.set("pid", a.pid)?;
                t.push(app)?;
            }
            Ok(t)
        })?,
    )?;
    // host.window.windowsOf(pid) — every on-screen window a process owns, from the window
    // manager rather than from accessibility. A popup menu drawn as a window of its own is
    // in this list and in no accessibility tree; the overlay runtime's menu watch compares
    // the list before and after the click that opens a menu.
    let sh = shared.clone();
    win.set(
        "windowsOf",
        lua.create_function(move |lua, pid: u32| {
            let t = lua.create_table()?;
            for s in sh.backend.windows_of(pid) {
                let w = lua.create_table()?;
                w.set("id", s.id)?;
                w.set("layer", s.layer)?;
                w.set("class", s.class)?;
                w.set("x", s.x)?;
                w.set("y", s.y)?;
                w.set("w", s.w)?;
                w.set("h", s.h)?;
                t.push(w)?;
            }
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    win.set(
        "active",
        lua.create_function(move |lua, ()| {
            let cached = {
                let mut obs = sh.observations();
                obs.asked += 1;
                match obs.active.clone() {
                    Some(w) => {
                        obs.served += 1;
                        w
                    }
                    None => {
                        drop(obs);
                        let t = Instant::now();
                        let w = sh.backend.active_window();
                        slow_observation("window.active", "", t);
                        sh.observations().active = Some(w.clone());
                        w
                    }
                }
            };
            match cached {
                Some(w) => Ok(Some(win_to_table(lua, &w)?)),
                None => Ok(None),
            }
        })?,
    )?;
    // host.window.controls(win?) — child controls (class + geometry) of a window
    // (the active one if omitted), for detecting embedded plugins.
    let sh = shared.clone();
    win.set(
        "controls",
        lua.create_function(move |lua, win_arg: Option<Table>| {
            let hwnd: isize = match win_arg {
                Some(t) => t.get("id")?,
                None => match sh.backend.active_window() {
                    Some(w) => w.hwnd,
                    None => return Ok(lua.create_table()?),
                },
            };
            // Enumerating a window's children is the other question asked once per overlay
            // per tick and answered identically every time — it is the same window.
            let controls = {
                let mut obs = sh.observations();
                obs.asked += 1;
                match obs.controls.get(&hwnd).cloned() {
                    Some(c) => {
                        obs.served += 1;
                        c
                    }
                    None => {
                        drop(obs);
                        let t = Instant::now();
                        let c = Rc::new(sh.backend.window_controls(hwnd));
                        slow_observation("window.controls", "", t);
                        sh.observations().controls.insert(hwnd, c.clone());
                        c
                    }
                }
            };
            let conv = Instant::now();
            let t = lua.create_table()?;
            for c in controls.iter() {
                t.push(control_to_table(lua, c)?)?;
            }
            sh.observations().binding_us += conv.elapsed().as_micros();
            Ok(t)
        })?,
    )?;
    // host.window.focusChain() — controls from the focused element up to its
    // top-level window, for detecting focus inside an embedded plugin.
    // host.window.ownsPoint(id, x, y) -> true | false | nil.
    //
    // nil means "this platform cannot say", and a caller must read that as permission rather
    // than refusal: a check with no answer must not block what it cannot judge.
    let sh = shared.clone();
    win.set(
        "ownsPoint",
        lua.create_function(move |_, (id, x, y): (isize, i32, i32)| {
            Ok(sh.backend.window_owns_point(id, x, y))
        })?,
    )?;

    let sh = shared.clone();
    win.set(
        "focusChain",
        lua.create_function(move |lua, ()| {
            let chain = {
                let mut obs = sh.observations();
                obs.asked += 1;
                match obs.focus_chain.clone() {
                    Some(c) => {
                        obs.served += 1;
                        c
                    }
                    None => {
                        drop(obs);
                        let t = Instant::now();
                        let c = Rc::new(sh.backend.window_focus_chain());
                        slow_observation("window.focusChain", "", t);
                        sh.observations().focus_chain = Some(c.clone());
                        c
                    }
                }
            };
            let conv = Instant::now();
            let t = lua.create_table()?;
            for c in chain.iter() {
                t.push(control_to_table(lua, c)?)?;
            }
            sh.observations().binding_us += conv.elapsed().as_micros();
            Ok(t)
        })?,
    )?;
    // host.window.recheck() — request a cross-VM overlay re-check on the next tick
    // (same effect as an OS focus event). For use after an overlay action that changes
    // the plugin's detectable UI WITHOUT a focus event — e.g. Komplete Kontrol
    // auto-closing its browser reveals the nested Kontakt, which nothing else would
    // notice until the library landmark poll (~500 ms). Coalesced: any number of calls
    // in one tick cost a single re-check.
    let sh = shared.clone();
    win.set(
        "recheck",
        lua.create_function(move |_, ()| {
            sh.recheck_requested.set(true);
            Ok(())
        })?,
    )?;
    host.set("window", win)?;

    // host.element.find(hwnd, name, controlType) — does the window's UI Automation
    // subtree contain an element with that Name + ControlType? (Plugin identity.)
    let uia = lua.create_table()?;
    let sh = shared.clone();
    uia.set(
        "find",
        lua.create_function(move |_, (hwnd, name, ctype): (isize, String, i32)| {
            let key = (hwnd, name, ctype);
            let mut obs = sh.observations();
            obs.asked += 1;
            if let Some(cached) = obs.element_find.get(&key).copied() {
                obs.served += 1;
                return Ok(cached);
            }
            drop(obs);
            let t = Instant::now();
            let answer = sh.backend.element_find(key.0, &key.1, key.2);
            slow_observation("uia.find", &key.1, t);
            sh.observations().element_find.insert(key, answer);
            Ok(answer)
        })?,
    )?;
    // host.element.type — the UIA ControlType ids, by name. Modules were declaring these as
    // local constants (and writing bare integers where they hadn't), which reads as a
    // magic number at every call site: `host.element.find(id, "Kontakt 8", 50033)`.
    let types = lua.create_table()?;
    for (name, id) in [
        ("Button", 50000),
        ("Calendar", 50001),
        ("CheckBox", 50002),
        ("ComboBox", 50003),
        ("Edit", 50004),
        ("Hyperlink", 50005),
        ("Image", 50006),
        ("ListItem", 50007),
        ("List", 50008),
        ("Menu", 50009),
        ("MenuBar", 50010),
        ("MenuItem", 50011),
        ("ProgressBar", 50012),
        ("RadioButton", 50013),
        ("ScrollBar", 50014),
        ("Slider", 50015),
        ("Spinner", 50016),
        ("StatusBar", 50017),
        ("Tab", 50018),
        ("TabItem", 50019),
        ("Text", 50020),
        ("ToolBar", 50021),
        ("ToolTip", 50022),
        ("Tree", 50023),
        ("TreeItem", 50024),
        ("Custom", 50025),
        ("Group", 50026),
        ("Thumb", 50027),
        ("DataGrid", 50028),
        ("DataItem", 50029),
        ("Document", 50030),
        ("SplitButton", 50031),
        ("Window", 50032),
        ("Pane", 50033),
        ("Header", 50034),
        ("HeaderItem", 50035),
        ("Table", 50036),
        ("TitleBar", 50037),
        ("Separator", 50038),
    ] {
        types.set(name, id)?;
    }
    uia.set("type", types)?;
    // host.element.findAny(hwnd, names, types) -> index | nil — "is any of these names
    // present as any of these control types?", the shape a plugin-identity check takes
    // ("Kontakt 8" as Window OR Pane). One tree traversal per NAME rather than one per
    // name×type pair, and it returns WHICH name matched (1-based), so a caller can read
    // the plugin's version straight out of the answer instead of asking once per version.
    let sh = shared.clone();
    uia.set(
        "findAny",
        lua.create_function(move |_, (hwnd, names, types): (isize, Vec<String>, Vec<i32>)| {
            let key = (hwnd, names, types);
            let mut obs = sh.observations();
            obs.asked += 1;
            if let Some(cached) = obs.uia_any.get(&key).copied() {
                obs.served += 1;
                return Ok(cached);
            }
            // Dropped before the traversal: it can re-enter Lua, and holding the RefMut
            // across that would panic on the next observation from inside a callback.
            drop(obs);
            let t = Instant::now();
            let answer = sh.backend.element_find_any(key.0, &key.1, &key.2);
            slow_observation("uia.findAny", &key.1.join("/"), t);
            sh.observations().uia_any.insert(key, answer);
            Ok(answer)
        })?,
    )?;
    // host.element.locate(hwnd, name, controlType) -> { x, y } (screen centre of the
    // matching element, to click it) or nil. For driving plugin UI via UIA.
    let sh = shared.clone();
    uia.set(
        "locate",
        lua.create_function(move |lua, (hwnd, name, ctype): (isize, String, i32)| {
            match sh.backend.element_locate(hwnd, &name, ctype) {
                Some((x, y)) => {
                    let t = lua.create_table()?;
                    t.set("x", x)?;
                    t.set("y", y)?;
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        })?,
    )?;
    // host.element.locateVia(hwnd, viaName, viaType, name, controlType) -> { x, y } | nil.
    // Like locate, but descends into a container element (viaName/viaType) first and
    // searches within it — reaches a plugin's UI that hangs off an identity pane as a
    // nested UIA fragment (a DAW-embedded Kontakt). Ports ReaHotkey's two-level find.
    let sh = shared.clone();
    uia.set(
        "locateVia",
        lua.create_function(
            move |lua, (hwnd, via_name, via_type, name, ctype): (isize, String, i32, String, i32)| {
                match sh.backend.element_locate_via(hwnd, &via_name, via_type, &name, ctype) {
                    Some((x, y)) => {
                        let t = lua.create_table()?;
                        t.set("x", x)?;
                        t.set("y", y)?;
                        Ok(Some(t))
                    }
                    None => Ok(None),
                }
            },
        )?,
    )?;
    // host.element.pluginLocate(hwnd, containerName, name, controlType) -> { x, y } | nil.
    // ReaHotkey's GetPluginUIAElement + FindElement: find the element that IS the plugin
    // (containerName, Window/Pane, QuickWindow class preferred) and search within it over
    // the RAW tree — the only view that reaches a DAW-embedded plugin's hosted Qt UI.
    let sh = shared.clone();
    uia.set(
        "pluginLocate",
        lua.create_function(
            move |lua, (hwnd, container, name, ctype): (isize, String, String, i32)| {
                let key = format!("pluginLocate {hwnd} {container} {name} {ctype}");
                match located(&sh, "uia.pluginLocate", key, || {
                    sh.backend.element_plugin_locate(hwnd, &container, &name, ctype)
                }) {
                    Some((x, y)) => {
                        let t = lua.create_table()?;
                        t.set("x", x)?;
                        t.set("y", y)?;
                        Ok(Some(t))
                    }
                    None => Ok(None),
                }
            },
        )?,
    )?;
    // host.element.stateProbe(hwnd, container, name, ctype) -> string | nil — what a named
    // element says about its OWN state, rather than what its control type implies.
    let sh = shared.clone();
    uia.set(
        "stateProbe",
        lua.create_function(
            move |lua, (hwnd, container, name, ctype): (isize, String, String, i32)| {
                match sh.backend.element_state_probe(hwnd, &container, &name, ctype) {
                    None => Ok(None),
                    Some((toggle, legacy)) => {
                        let t = lua.create_table()?;
                        if toggle >= 0 {
                            t.set("toggle", toggle)?;
                        }
                        t.set("legacyState", legacy)?;
                        t.set("checked", legacy & 0x10 != 0)?;
                        Ok(Some(t))
                    }
                }
            },
        )?,
    )?;
    // host.element.dump(hwnd) -> { {depth, name, class, ctype}, … } — diagnostic UIA
    // tree walk (raw view) for discovering plugin identity properties.
    let sh = shared.clone();
    uia.set(
        "dump",
        lua.create_function(move |lua, hwnd: isize| {
            let arr = lua.create_table()?;
            for (i, n) in sh.backend.element_dump(hwnd).into_iter().enumerate() {
                arr.set(i + 1, dump_node_to_table(lua, n)?)?;
            }
            Ok(arr)
        })?,
    )?;
    // host.element.rawDump(hwnd) — like dump, but over the RAW TreeWalker, which crosses
    // into a hosted Qt fragment (a DAW-embedded plugin's real UI).
    let sh = shared.clone();
    uia.set(
        "rawDump",
        lua.create_function(move |lua, hwnd: isize| {
            let arr = lua.create_table()?;
            for (i, n) in sh.backend.element_raw_dump(hwnd).into_iter().enumerate() {
                arr.set(i + 1, dump_node_to_table(lua, n)?)?;
            }
            Ok(arr)
        })?,
    )?;
    // host.element.classNavPoint(hwnd, className, ctype, child, sibling) -> {x,y} | nil —
    // click-point of the element reached from the first ClassName-contains +
    // ControlType match by walking `child` (nth child) then `sibling` siblings. Ports
    // ReaHotkey's FindElement(ClassName) + WalkTree(path).Click.
    let sh = shared.clone();
    uia.set(
        "classNavPoint",
        lua.create_function(
            move |lua, (hwnd, class, ctype, child, sibling): (isize, String, i32, i32, i32)| {
                match sh.backend.element_class_nav_point(hwnd, &class, ctype, child, sibling) {
                    Some((x, y)) => {
                        let t = lua.create_table()?;
                        t.set("x", x)?;
                        t.set("y", y)?;
                        Ok(Some(t))
                    }
                    None => Ok(None),
                }
            },
        )?,
    )?;
    // host.element.focusStep(hwnd, direction) -> { name, ctype, index, count } | nil —
    // Tab pass-through for a standalone plugin window: SetFocus the next (direction>=0)
    // / previous keyboard-focusable descendant relative to the current focus, wrapping
    // at the ends, and return the newly focused element so the overlay can announce it.
    let sh = shared.clone();
    uia.set(
        "focusStep",
        lua.create_function(move |lua, (hwnd, direction): (isize, i32)| {
            match sh.backend.element_focus_step(hwnd, direction) {
                Some((name, ctype, index, count)) => {
                    let t = lua.create_table()?;
                    t.set("name", name)?;
                    t.set("ctype", ctype)?;
                    t.set("index", index)?;
                    t.set("count", count)?;
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        })?,
    )?;
    host.set("element", uia)?;

    // host.screen.pixel / .size / .imageSearch
    let screen = lua.create_table()?;
    let sh = shared.clone();
    screen.set(
        "pixel",
        lua.create_function(move |lua, (x, y): (i32, i32)| {
            // Timed and counted: a single pixel read is a GDI screen touch, and this project
            // has already measured one at a fixed ~16.7 ms — one compositor frame, whatever
            // the size. Sixty of them is a second, and nothing in the log said they were
            // happening.
            let t0 = Instant::now();
            let (r, g, b) = sh.backend.pixel(x, y);
            {
                let mut obs = sh.observations();
                obs.pixels += 1;
                obs.pixel_us += t0.elapsed().as_micros();
            }
            let t = lua.create_table()?;
            t.set("r", r)?;
            t.set("g", g)?;
            t.set("b", b)?;
            t.set("hex", format!("#{r:02X}{g:02X}{b:02X}"))?;
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    screen.set(
        "size",
        lua.create_function(move |lua, ()| {
            let (w, h) = sh.backend.screen_size();
            let t = lua.create_table()?;
            t.set("w", w)?;
            t.set("h", h)?;
            Ok(t)
        })?,
    )?;
    // host.screen.profile{ region = {x1,y1,x2,y2}, axes = "both"|"columns"|"rows" }
    //   -> { x, y, w, h, columns = {min,max,mean,r,g,b}, rows = {…} }  |  nil
    //
    // ONE capture, reduced in Rust to a shape a module can reason about.
    //
    // The reason this exists is a measurement, not a preference. On this machine a BitBlt
    // capture costs ~16.6 ms at 55x27 and ~16.7 ms at 1028x666, while GetPixel costs 16.7 ms
    // for ONE pixel: the compositor serialises every screen touch to its vsync, so area is
    // free and touching is what costs. A module that wants to know where an edge is therefore
    // had only one honest option — probe a handful of points and hope they were the right ones
    // — and the alternative, shipping a whole bitmap into Lua, would trade a cheap capture for
    // an expensive marshal and a slow scan in a scripting language.
    //
    // So the reduction happens here. Per column and per row: the darkest pixel, the lightest,
    // the mean, and the mean of each channel. That is enough to find a vertical line (a dark
    // column in a light band), an edge (where the mean steps), the extent of a shape, and a
    // patch of a different colour — which is the whole of what these overlays have ever needed
    // to read out of a picture, and none of it is expressible with point probes.
    //
    // Counted as one screen touch in the observation log, because that is exactly what it is.
    let sh = shared.clone();
    screen.set(
        "profile",
        lua.create_function(move |lua, opts: Option<Table>| {
            let (sw, sh2) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh2);
            if rw <= 0 || rh <= 0 {
                return Ok(mlua::Value::Nil);
            }
            let axes = opts
                .as_ref()
                .and_then(|o| o.get::<String>("axes").ok())
                .unwrap_or_else(|| "both".to_string());
            // Refused, not silently widened. A typo would otherwise cost exactly the work the
            // option exists to save, and say nothing about why.
            let (want_cols, want_rows) = match axes.as_str() {
                "both" => (true, true),
                "columns" => (true, false),
                "rows" => (false, true),
                other => {
                    return Err(mlua::Error::external(format!(
                        "host.screen.profile: axes must be \"both\", \"columns\" or \"rows\", got \"{other}\""
                    )))
                }
            };
            let t0 = Instant::now();
            let cap = match sh.backend.capture(rx, ry, rw, rh) {
                Some(c) => c,
                None => return Ok(mlua::Value::Nil),
            };
            let (w, h) = (cap.w as usize, cap.h as usize);
            // Trust the buffer only as far as it goes. Every index below is computed from w and
            // h, so a capture that came back short — a clipped region, a backend that rounded a
            // dimension — would index past the end, and a panic inside a Lua binding is not a
            // module error that gets reported: it is the process. Answering nil is what every
            // other failure in this function does.
            if w == 0 || h == 0 || cap.rgba.len() < w * h * 4 {
                return Ok(mlua::Value::Nil);
            }
            // Accumulators for both axes in ONE traversal: the capture is already the whole
            // cost, and walking it twice to keep the code symmetrical would be the only part
            // of this that scales with area.
            // How many pixels in this column/row are darker than `dark`.
            //
            // The third statistic, and for finding a SHAPE it is the only one of the three that
            // works. A mean over hundreds of rows dilutes a note-sized object to a couple of
            // units; a minimum saturates, because in a busy picture almost every column already
            // contains something black and cannot go blacker. A count does neither — it is
            // proportional to how much of the column the object covers. Measured on a real
            // Melodyne capture at threshold 190: background and grid lines about 19 rows, note
            // blobs 29 to 57, a full-height cursor 470 to 516. Three classes, cleanly apart,
            // where the other two channels saw one.
            let dark_t: u8 = opts
                .as_ref()
                .and_then(|o| o.get::<u8>("dark").ok())
                .unwrap_or(128);
            let (mut cdark, mut rdark) = (vec![0u32; w], vec![0u32; h]);
            let (mut cmin, mut cmax) = (vec![255u8; w], vec![0u8; w]);
            let (mut rmin, mut rmax) = (vec![255u8; h], vec![0u8; h]);
            let (mut csum, mut rsum) = (vec![0u64; w], vec![0u64; h]);
            let mut cch = vec![[0u64; 3]; w];
            let mut rch = vec![[0u64; 3]; h];
            for y in 0..h {
                for x in 0..w {
                    let o = (y * w + x) * 4;
                    let (r, g, b) = (cap.rgba[o], cap.rgba[o + 1], cap.rgba[o + 2]);
                    // ITU-R BT.601, so a coloured mark is weighted the way an eye would weight
                    // it. Melodyne's chrome is neutral grey (r == g == b), where this agrees
                    // with a plain average anyway; the note blobs are not.
                    let l = ((r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000) as u8;
                    if want_cols {
                        if l < dark_t {
                            cdark[x] += 1;
                        }
                        if l < cmin[x] {
                            cmin[x] = l;
                        }
                        if l > cmax[x] {
                            cmax[x] = l;
                        }
                        csum[x] += l as u64;
                        cch[x][0] += r as u64;
                        cch[x][1] += g as u64;
                        cch[x][2] += b as u64;
                    }
                    if want_rows {
                        if l < dark_t {
                            rdark[y] += 1;
                        }
                        if l < rmin[y] {
                            rmin[y] = l;
                        }
                        if l > rmax[y] {
                            rmax[y] = l;
                        }
                        rsum[y] += l as u64;
                        rch[y][0] += r as u64;
                        rch[y][1] += g as u64;
                        rch[y][2] += b as u64;
                    }
                }
            }
            // The means are REAL numbers, not truncated to a byte.
            //
            // Integer division looked harmless and is not: a mean over 522 rows moves by less
            // than one whole unit when a note-sized shape changes inside it, so `as u8` would
            // floor exactly the signal a caller is looking for down to zero. Lua numbers are
            // doubles; there is nothing to save by rounding on the way in. `min` and `max` stay
            // bytes because they ARE bytes — a particular pixel's value, not an average.
            let axis = |lua: &Lua,
                        min: &[u8],
                        max: &[u8],
                        dark: &[u32],
                        sum: &[u64],
                        ch: &[[u64; 3]],
                        n: u64|
             -> mlua::Result<Table> {
                let n = n.max(1) as f64;
                let t = lua.create_table()?;
                t.set("min", lua.create_sequence_from(min.iter().copied())?)?;
                t.set("max", lua.create_sequence_from(max.iter().copied())?)?;
                t.set("dark", lua.create_sequence_from(dark.iter().copied())?)?;
                t.set(
                    "mean",
                    lua.create_sequence_from(sum.iter().map(|s| *s as f64 / n))?,
                )?;
                for (i, name) in ["r", "g", "b"].iter().enumerate() {
                    t.set(
                        *name,
                        lua.create_sequence_from(ch.iter().map(|c| c[i] as f64 / n))?,
                    )?;
                }
                Ok(t)
            };
            let out = lua.create_table()?;
            out.set("x", rx)?;
            out.set("y", ry)?;
            out.set("w", cap.w)?;
            out.set("h", cap.h)?;
            if want_cols {
                out.set("columns", axis(lua, &cmin, &cmax, &cdark, &csum, &cch, h as u64)?)?;
            }
            if want_rows {
                out.set("rows", axis(lua, &rmin, &rmax, &rdark, &rsum, &rch, w as u64)?)?;
            }
            // Timed to HERE, not to the end of the capture. The capture is a fixed frame; the
            // traversal and the six sequences are the only part that grows with the region, and
            // leaving them outside the measurement would hide them from the one report this
            // project uses to find event-loop stalls — while making this call look free.
            {
                let mut obs = sh.observations();
                obs.pixels += 1;
                obs.pixel_us += t0.elapsed().as_micros();
            }
            Ok(mlua::Value::Table(out))
        })?,
    )?;
    let sh = shared.clone();
    screen.set(
        "imageSearch",
        lua.create_function(move |lua, (template, opts): (String, Option<Table>)| {
            let tmpl = sh.load_template(&sh.root(idx).join(&template))?;
            let (sw, sh_) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
            let cap = match sh.backend.capture(rx, ry, rw, rh) {
                Some(c) => c,
                None => return Ok(None),
            };
            let tol: u8 = opts.as_ref().and_then(|o| o.get::<u8>("tolerance").ok()).unwrap_or(0);
            let scales = read_scales(opts.as_ref());
            match find_template_scaled(&cap, tmpl.w, tmpl.h, &tmpl.rgba, tol, &scales, &tmpl.probes) {
                Some((ox, oy, mw, mh)) => {
                    let t = lua.create_table()?;
                    t.set("x", rx + ox as i32)?;
                    t.set("y", ry + oy as i32)?;
                    t.set("w", mw)?;
                    t.set("h", mh)?;
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        })?,
    )?;

    // host.screen.imageSearchMulti(templates, opts?) -> (index, {x,y,w,h}) | (nil, nil)
    // Captures the region ONCE and tries each template path in order, returning the
    // 1-based index of the first match plus its hit rect. One capture serves many
    // comparisons (e.g. a toggle's on/off pair) — half the ~1-frame screen touches of
    // two imageSearch calls, and both templates are matched against the SAME frame, so
    // a state change mid-repaint can't fall between two separate captures. Sync, like
    // imageSearch (AHK-style). Templates share the decode cache.
    let sh = shared.clone();
    screen.set(
        "imageSearchMulti",
        lua.create_function(move |lua, (templates, opts): (Table, Option<Table>)| {
            let (sw, sh_) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
            let cap = match sh.backend.capture(rx, ry, rw, rh) {
                Some(c) => c,
                None => return Ok((mlua::Value::Nil, mlua::Value::Nil)),
            };
            let tol: u8 = opts.as_ref().and_then(|o| o.get::<u8>("tolerance").ok()).unwrap_or(0);
            let scales = read_scales(opts.as_ref());
            let mut i = 0i64;
            for entry in templates.sequence_values::<String>() {
                i += 1;
                let tmpl = sh.load_template(&sh.root(idx).join(&entry?))?;
                if let Some((ox, oy, mw, mh)) =
                    find_template_scaled(&cap, tmpl.w, tmpl.h, &tmpl.rgba, tol, &scales, &tmpl.probes)
                {
                    let t = lua.create_table()?;
                    t.set("x", rx + ox as i32)?;
                    t.set("y", ry + oy as i32)?;
                    t.set("w", mw)?;
                    t.set("h", mh)?;
                    return Ok((mlua::Value::Integer(i), mlua::Value::Table(t)));
                }
            }
            Ok((mlua::Value::Nil, mlua::Value::Nil))
        })?,
    )?;
    // host.screen.imageSearchAsync(image | {image, …}, opts, cb) — offloads BOTH the
    // region capture (the ~1-frame DWM-compositor cost) and the template match to a
    // worker thread, calling cb({x,y,w,h,n}) | cb(nil) on a later tick, so a detection
    // poll (e.g. the 500 ms landmark poll) never blocks the event loop on either. Only
    // the template decode happens here, and it is cached.
    //
    // Given a LIST of images, they are tried in order against the SAME captured frame and
    // the first hit wins, with `n` reporting which one matched — for a thing with several
    // renderings (a dialog's close glyph as two plugin versions draw it). Chaining
    // separate searches instead costs a fresh capture per template.
    let sh = shared.clone();
    screen.set(
        "imageSearchAsync",
        lua.create_function(
            move |lua, (template, opts, cb): (mlua::Value, Option<Table>, Function)| {
                let mut tmpls = Vec::new();
                match &template {
                    mlua::Value::Table(list) => {
                        for p in list.clone().sequence_values::<String>() {
                            tmpls.push(sh.load_template(&sh.root(idx).join(&p?))?);
                        }
                    }
                    _ => {
                        let p: String = lua.from_value(template.clone())?;
                        tmpls.push(sh.load_template(&sh.root(idx).join(&p))?);
                    }
                }
                if tmpls.is_empty() {
                    return Err(mlua::Error::external(
                        "imageSearchAsync: no template given".to_string(),
                    ));
                }
                let (sw, sh_) = sh.backend.screen_size();
                let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
                let tol: u8 = opts.as_ref().and_then(|o| o.get::<u8>("tolerance").ok()).unwrap_or(0);
                let scales = read_scales(opts.as_ref());
                // The worker captures the region itself and ALWAYS posts a result (a
                // capture failure becomes a no-match on the tick), so the pending
                // callback is always drained.
                let id = sh.next_image_id.get() + 1;
                sh.next_image_id.set(id);
                let key = lua.create_registry_value(cb)?;
                sh.pending_image.borrow_mut().insert(id, (lua.clone(), key, idx));
                if sh
                    .image_tasks
                    .send(ImageTask { id, region: (rx, ry, rw, rh), tmpls, tol, scales })
                    .is_err()
                {
                    // Worker thread gone (should never happen — it's panic-proof): drop
                    // the pending entry we just inserted so it can't leak (its registry
                    // value is freed with it), instead of a callback that never fires.
                    sh.pending_image.borrow_mut().remove(&id);
                }
                Ok(())
            },
        )?,
    )?;
    // host.screen.saveMarked(path, opts) — like save, but draws a crosshair (and an index
    // tick) at each point in `opts.marks` = { {x, y}, … } in SCREEN coordinates.
    //
    // This is the calibration question in one picture: "is my control on its button?"
    // Reading a control's coordinates out of a log and finding that spot in a screenshot
    // by hand is the step that hides errors — a set of toggles in this repo sat 16 px off,
    // on the caption row under the buttons, for as long as it existed, because the colours
    // sampled there happened to look plausible. A crosshair drawn where the click will
    // actually land makes that a glance instead of an arithmetic exercise.
    let sh = shared.clone();
    screen.set(
        "saveMarked",
        lua.create_function(move |lua, (path, opts): (String, Table)| {
            let full = sh.root(idx).join(&path);
            let (sw, sh_) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(Some(&opts), sw, sh_);
            let Some(cap) = sh.backend.capture(rx, ry, rw, rh) else { return Ok(false) };
            let Some(mut img) = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba) else {
                return Ok(false);
            };
            let marks: Table = opts.get("marks").unwrap_or(lua.create_table()?);
            for (n, m) in marks.sequence_values::<Table>().enumerate() {
                let Ok(m) = m else { continue };
                let (mx, my): (i32, i32) = (m.get("x").unwrap_or(0), m.get("y").unwrap_or(0));
                // Region-relative, and only what is actually inside the shot.
                let (cx, cy) = (mx - rx, my - ry);
                if cx < 0 || cy < 0 || cx >= cap.w as i32 || cy >= cap.h as i32 {
                    continue;
                }
                // Magenta: not a colour these dark plugin UIs use, so it cannot be mistaken
                // for part of the interface.
                let ink = image::Rgba([255u8, 0, 255, 255]);
                for d in -9i32..=9 {
                    for (px, py) in [(cx + d, cy), (cx, cy + d)] {
                        if px >= 0 && py >= 0 && px < cap.w as i32 && py < cap.h as i32 {
                            img.put_pixel(px as u32, py as u32, ink);
                        }
                    }
                }
                // n+1 ticks below the crosshair, so marks stay tellable apart without text.
                for t in 0..=(n as i32) {
                    let (px, py) = (cx - (n as i32) + 2 * t, cy + 12);
                    if px >= 0 && py >= 0 && px < cap.w as i32 && py < cap.h as i32 {
                        img.put_pixel(px as u32, py as u32, ink);
                    }
                }
            }
            // Create the folder: image::save does not, and "the system cannot find the
            // path" is a poor answer to "put a screenshot here".
            if let Some(dir) = full.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            img.save(&full).map_err(|e| {
                mlua::Error::external(format!("screen.saveMarked '{}': {e}", full.display()))
            })?;
            Ok(true)
        })?,
    )?;
    // host.screen.imageSearchAll(image, opts?) -> { {x,y,w,h}, … } — EVERY match of the
    // template in the region, not just the first.
    //
    // For deciding whether a template is safe to click blindly. A close-glyph template
    // that matches twice will eventually click the wrong one, and a template that matches
    // nowhere is a control that silently never fires; "matched exactly once, here" is the
    // answer you want before shipping either.
    let sh = shared.clone();
    screen.set(
        "imageSearchAll",
        lua.create_function(move |lua, (template, opts): (String, Option<Table>)| {
            let tmpl = sh.load_template(&sh.root(idx).join(&template))?;
            let (sw, sh_) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
            let tol: u8 = opts.as_ref().and_then(|o| o.get::<u8>("tolerance").ok()).unwrap_or(0);
            let out = lua.create_table()?;
            let Some(cap) = sh.backend.capture(rx, ry, rw, rh) else { return Ok(out) };
            let mut n = 0;
            // Non-overlapping: after a hit, resume past its right edge on that row, so one
            // match is reported once rather than once per pixel of slop.
            let (tw, th) = (tmpl.w, tmpl.h);
            let mut y = 0;
            while y + th <= cap.h {
                let mut x = 0;
                while x + tw <= cap.w {
                    if matches_at(&cap, x, y, tw, th, &tmpl.rgba, tol, &tmpl.probes) {
                        let t = lua.create_table()?;
                        t.set("x", rx + x as i32)?;
                        t.set("y", ry + y as i32)?;
                        t.set("w", tw)?;
                        t.set("h", th)?;
                        n += 1;
                        out.set(n, t)?;
                        x += tw;
                    } else {
                        x += 1;
                    }
                }
                y += 1;
            }
            Ok(out)
        })?,
    )?;
    // host.screen.save(path, opts?) — capture a screen region (opts.region, else the
    // full screen) and write it to `path` (relative to the calling module's root; an
    // absolute path is used as-is) as a PNG. Returns true on success. A calibration
    // affordance: re-capture an image control's templates against the live plugin.
    let sh = shared.clone();
    screen.set(
        "save",
        lua.create_function(move |_, (path, opts): (String, Option<Table>)| {
            let full = sh.root(idx).join(&path);
            let (sw, sh_) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
            match sh.backend.capture(rx, ry, rw, rh) {
                Some(cap) => match image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba) {
                    Some(img) => {
                        if let Some(dir) = full.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        img.save(&full).map_err(|e| {
                            mlua::Error::external(format!("screen.save '{}': {e}", full.display()))
                        })?;
                        Ok(true)
                    }
                    None => Ok(false),
                },
                None => Ok(false),
            }
        })?,
    )?;
    host.set("screen", screen)?;

    // host.ocr.recognize({ region, lang }) -> { text, words = {{text,x,y,w,h}, ...}, skipped }
    let ocr = lua.create_table()?;
    let sh = shared.clone();
    ocr.set(
        "recognize",
        lua.create_function(move |lua, opts: Option<Table>| {
            let (sw, shh) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, shh);
            let lang: Option<String> = opts.as_ref().and_then(|o| o.get::<String>("lang").ok());
            let res = sh
                .backend
                .ocr(rx, ry, rw, rh, lang.as_deref())
                .map_err(mlua::Error::external)?;
            let t = lua.create_table()?;
            t.set("text", res.text)?;
            let words = lua.create_table()?;
            for wd in res.words {
                let w = lua.create_table()?;
                w.set("text", wd.text)?;
                w.set("x", rx + wd.x)?;
                w.set("y", ry + wd.y)?;
                w.set("w", wd.w)?;
                w.set("h", wd.h)?;
                words.push(w)?;
            }
            t.set("words", words)?;
            // Whether the engine was asked at all. A blank region answers empty either way,
            // and a module that wants to know which — the probe does — has nothing else to
            // look at from Lua.
            t.set("skipped", res.skipped)?;
            Ok(t)
        })?,
    )?;
    // host.ocr.recognizeMany{ regions = { Region, … }, lang? } -> { { text, words, skipped }, … }
    //
    // The same recognitions, ONE screen touch. Measured on this machine: recognising a 67x13
    // read-out costs 4-6 ms and cropping it 0.5, while the capture underneath is a fixed ~17 ms
    // compositor frame whatever its size — so two adjacent read-outs read one after the other
    // spent two thirds of their time photographing the screen twice. Melodyne's selection
    // watcher did exactly that, eight times a second, and measured 44 ms a tick against 27 for
    // one region.
    //
    // The regions are NOT merged into one recognition, and that distinction is the whole reason
    // this is a new call rather than a wider rectangle: the fallback to the neural recogniser
    // fires per region, only when that region came back empty, and a merged strip is never
    // empty — so a note name Windows.Media.Ocr dropped stayed dropped while the cents beside it
    // came through. Every region is still recognised on its own, with its own fallback, and its
    // word boxes are still relative to itself.
    //
    // A region that failed comes back as `{ text = "", error = "…" }` rather than as a hole in
    // the sequence: a caller indexing results[2] must never get the third region's answer.
    let sh = shared.clone();
    ocr.set(
        "recognizeMany",
        lua.create_function(move |lua, opts: Table| {
            let (sw, shh) = sh.backend.screen_size();
            let regions: Table = opts.get("regions")?;
            let lang: Option<String> = opts.get::<String>("lang").ok();
            let mut rects: Vec<(i32, i32, i32, i32)> = Vec::new();
            for r in regions.sequence_values::<Table>() {
                let r = r?;
                let x1: i32 = r.get("x1").or_else(|_| r.get(1)).unwrap_or(0);
                let y1: i32 = r.get("y1").or_else(|_| r.get(2)).unwrap_or(0);
                let x2: i32 = r.get("x2").or_else(|_| r.get(3)).unwrap_or(sw);
                let y2: i32 = r.get("y2").or_else(|_| r.get(4)).unwrap_or(shh);
                rects.push((x1, y1, (x2 - x1).max(0), (y2 - y1).max(0)));
            }
            let results = sh.backend.ocr_regions(&rects, lang.as_deref());
            let out = lua.create_table()?;
            for (i, res) in results.into_iter().enumerate() {
                let (rx, ry) = rects.get(i).map(|r| (r.0, r.1)).unwrap_or((0, 0));
                let t = lua.create_table()?;
                match res {
                    Ok(r) => {
                        t.set("text", r.text)?;
                        let words = lua.create_table()?;
                        for wd in r.words {
                            let w = lua.create_table()?;
                            w.set("text", wd.text)?;
                            w.set("x", rx + wd.x)?;
                            w.set("y", ry + wd.y)?;
                            w.set("w", wd.w)?;
                            w.set("h", wd.h)?;
                            words.push(w)?;
                        }
                        t.set("words", words)?;
                        t.set("skipped", r.skipped)?;
                    }
                    Err(e) => {
                        t.set("text", "")?;
                        t.set("words", lua.create_table()?)?;
                        // Present so every entry has the same shape; false because a failed
                        // read is not the blank guard answering, and `error` says what it was.
                        t.set("skipped", false)?;
                        t.set("error", e)?;
                    }
                }
                out.push(t)?;
            }
            Ok(out)
        })?,
    )?;
    host.set("ocr", ocr)?;

    // host.epoch() -> number — a counter that changes whenever the world may have: an
    // OS event dispatched into a module, a timer firing, or the module itself driving
    // input. Memoize an expensive observation against it (`if cachedEpoch == host.epoch()
    // then return cached end`) and repeats within one dispatch cost nothing, while a new
    // situation is always re-observed. Deliberately NOT time-based: a stale coordinate
    // clicks the wrong thing, and "it was fresh 50 ms ago" is not a safety property.
    let sh = shared.clone();
    host.set("epoch", lua.create_function(move |_, ()| Ok(sh.epoch.get()))?)?;

    // host.input: cursorPos / move / click / drag / scroll / send / text
    let input = lua.create_table()?;
    let sh = shared.clone();
    input.set(
        "cursorPos",
        lua.create_function(move |lua, ()| {
            let (x, y) = sh.backend.cursor_pos();
            let t = lua.create_table()?;
            t.set("x", x)?;
            t.set("y", y)?;
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "move",
        lua.create_function(move |_, (x, y): (i32, i32)| {
            sh.bump_input_epoch();
            sh.backend.mouse_move(x, y);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "click",
        lua.create_function(move |_, (x, y, opts): (i32, i32, Option<Table>)| {
            sh.bump_input_epoch();
            sh.backend.mouse_click(x, y, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "post",
        lua.create_function(move |_, (hwnd, key): (i64, String)| {
            sh.backend
                .key_post(hwnd as isize, &key)
                .map_err(mlua::Error::external)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "mouseDown",
        lua.create_function(move |_, (x, y, opts): (i32, i32, Option<Table>)| {
            sh.bump_input_epoch();
            sh.backend.mouse_down(x, y, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "mouseUp",
        lua.create_function(move |_, (x, y, opts): (i32, i32, Option<Table>)| {
            sh.bump_input_epoch();
            sh.backend.mouse_up(x, y, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "drag",
        lua.create_function(move |_, (x1, y1, x2, y2, opts): (i32, i32, i32, i32, Option<Table>)| {
            sh.bump_input_epoch();
            sh.backend.mouse_drag(x1, y1, x2, y2, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    // host.input.scroll(x, y, notches) — FRACTIONAL, because one notch is not always a small
    // enough step.
    //
    // A wheel notch is 120 units and a control decides for itself what that is worth; ON:EAR's
    // Tone knob moves by five of its own units per notch, which is as fine as this could ever
    // ask for while notches were whole numbers. They no longer are: half a notch is 60 units,
    // and an application that scales its step by the delta — which is the ordinary way to
    // handle a wheel — moves by half as much. One that rounds to its own interval instead
    // simply does not move, and says the same value back, which is the honest outcome.
    input.set(
        "scroll",
        lua.create_function(move |_, (x, y, notches): (i32, i32, f64)| {
            sh.bump_input_epoch();
            let delta = (notches * 120.0).round() as i32;
            sh.backend.mouse_scroll(x, y, delta);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "send",
        lua.create_function(move |_, combo: String| {
            sh.bump_input_epoch();
            sh.backend.key_send(&combo).map_err(mlua::Error::external)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "text",
        lua.create_function(move |_, text: String| {
            sh.bump_input_epoch();
            sh.backend.type_text(&text);
            Ok(())
        })?,
    )?;
    host.set("input", input)?;

    // host.sound.play(path) — fire-and-forget audio asset playback
    let sound = lua.create_table()?;
    let sh = shared.clone();
    sound.set(
        "play",
        lua.create_function(move |_, rel: String| {
            let path = sh.root(idx).join(&rel);
            let mut audio = sh.audio.borrow_mut();
            if audio.is_none() {
                // Open the audio device on first use only (see the field doc).
                match rodio::OutputStream::try_default() {
                    Ok(pair) => *audio = Some(pair),
                    Err(e) => {
                        logging::line("sound", &format!("no audio output device: {e}"));
                        return Ok(());
                    }
                }
            }
            if let Some((_, handle)) = audio.as_ref() {
                let play = || -> anyhow::Result<()> {
                    let file = std::io::BufReader::new(std::fs::File::open(&path)?);
                    let source = rodio::Decoder::new(file)?;
                    let sink = rodio::Sink::try_new(handle)?;
                    sink.append(source);
                    sink.detach();
                    Ok(())
                };
                if let Err(e) = play() {
                    logging::line("sound", &format!("cannot play '{}': {e}", path.display()));
                }
            }
            Ok(())
        })?,
    )?;
    host.set("sound", sound)?;

    // host.path(rel) -> real path (escape hatch)
    let sh = shared.clone();
    host.set(
        "path",
        lua.create_function(move |_, rel: String| {
            // Absolute, so a path handed to another module (e.g. a library's image
            // searched by its container) resolves correctly regardless of that
            // module's own working directory.
            let p = sh.root(idx).join(&rel);
            Ok(std::path::absolute(&p).unwrap_or(p).to_string_lossy().to_string())
        })?,
    )?;

    // host.resource.read(rel) -> bytes/string from the package
    let resource = lua.create_table()?;
    let sh = shared.clone();
    resource.set(
        "read",
        lua.create_function(move |_, rel: String| {
            let path = sh.root(idx).join(&rel);
            std::fs::read_to_string(&path).map_err(mlua::Error::external)
        })?,
    )?;
    // host.resource.exists(rel) -> bool. `read` cannot answer this for anything binary
    // (it decodes as UTF-8 and fails on a PNG whether or not the file is there), and the
    // calibrator needs it to number a shot without overwriting an earlier one from a
    // PREVIOUS RUN — an in-memory counter resets on restart, which is the normal case
    // while a module is being written.
    let sh = shared.clone();
    resource.set(
        "exists",
        lua.create_function(move |_, rel: String| Ok(sh.root(idx).join(&rel).exists()))?,
    )?;
    host.set("resource", resource)?;

    // host.settings: define/get/set/onChange — persisted in the unified store.
    // A module only ever sees its own settings (the id is resolved from `idx`).
    let settings_api = lua.create_table()?;
    let sh = shared.clone();
    settings_api.set(
        "define",
        lua.create_function(
            move |lua, (key, default, opts): (String, mlua::Value, Option<Table>)| {
                let def = lua_to_value(&default)?;
                let field = field_from(&key, &def, opts.as_ref());
                let id = sh.ids.borrow()[idx].clone();
                sh.schemas.borrow_mut()[idx].insert(key.clone(), field);
                let current = {
                    let mut store = sh.store.borrow_mut();
                    match store.get(&id, &key) {
                        Some(v) if v.kind() == def.kind() => v, // persisted wins
                        _ => {
                            store.set(&id, &key, def.clone());
                            sh.dirty.set(true);
                            def
                        }
                    }
                };
                value_to_lua(lua, &current)
            },
        )?,
    )?;
    let sh = shared.clone();
    settings_api.set(
        "get",
        lua.create_function(move |lua, key: String| {
            if !sh.schemas.borrow()[idx].contains_key(&key) {
                return Err(mlua::Error::external(format!("setting '{key}' was not defined")));
            }
            let id = sh.ids.borrow()[idx].clone();
            let v = sh
                .store
                .borrow()
                .get(&id, &key)
                .ok_or_else(|| mlua::Error::external(format!("setting '{key}' has no value")))?;
            value_to_lua(lua, &v)
        })?,
    )?;
    let sh = shared.clone();
    settings_api.set(
        "set",
        lua.create_function(move |_, (key, value): (String, mlua::Value)| {
            let v = lua_to_value(&value)?;
            {
                let schemas = sh.schemas.borrow();
                let field = schemas[idx].get(&key).ok_or_else(|| {
                    mlua::Error::external(format!("setting '{key}' was not defined"))
                })?;
                field
                    .validate(&v)
                    .map_err(|e| mlua::Error::external(format!("setting '{key}': {e}")))?;
            }
            let id = sh.ids.borrow()[idx].clone();
            let old = sh.store.borrow_mut().set(&id, &key, v.clone());
            sh.dirty.set(true);
            sh.fire_on_change(idx, &key, &v, old.as_ref());
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    settings_api.set(
        "onChange",
        lua.create_function(move |lua, (key, cb): (String, Function)| {
            let rk = lua.create_registry_value(cb)?;
            sh.on_change.borrow_mut().entry((idx, key)).or_default().push((lua.clone(), rk));
            Ok(())
        })?,
    )?;
    host.set("settings", settings_api.clone())?;
    host.set("config", settings_api)?; // catalog-compat alias (host.config.get/set)

    // host.arbiter — cross-VM overlay election (see Shared::arbiter_*). Overlays
    // competing for the same `slot` register a claim with a `specificity`; only
    // the matching claim of highest specificity is active. The host drives the new
    // winner's onActivate and the old winner's onDeactivate on every change —
    // including poll-driven ones with no window event (a library landmark
    // appearing/vanishing inside a plugin). Low-level: the overlay runtime wraps it.
    //
    // Conventions the overlay runtime must uphold at step 4: a base overlay and the
    // overlays that inherit it MUST pass byte-identical `slot` strings (derive it
    // from a shared id — the plugin contract / attachEmbedded host+control spec —
    // not a hand-typed literal, else they silently never suppress each other).
    // `specificity` is a static tier: base = 0, an inheriting overlay = base + 1;
    // "which library" is expressed by separate library overlays each matching only
    // their own landmark, not by mutating specificity. Handles are small monotonic
    // ints (well within Luau's 2^53 exact-integer range).
    let arbiter = lua.create_table()?;
    let sh = shared.clone();
    arbiter.set(
        "register",
        lua.create_function(
            move |lua,
                  (slot, specificity, on_activate, on_deactivate): (
                String,
                i64,
                Function,
                Function,
            )| {
                let ak = lua.create_registry_value(on_activate)?;
                let dk = lua.create_registry_value(on_deactivate)?;
                Ok(sh.arbiter_register(slot, idx, specificity, lua.clone(), ak, dk))
            },
        )?,
    )?;
    let sh = shared.clone();
    arbiter.set(
        "setMatching",
        lua.create_function(move |_, (slot, handle, matching): (String, i64, bool)| {
            sh.arbiter_set_matching(&slot, handle, matching);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    arbiter.set(
        "unregister",
        lua.create_function(move |_, (slot, handle): (String, i64)| {
            sh.arbiter_unregister(&slot, handle);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    arbiter.set(
        "winner",
        lua.create_function(move |_, slot: String| Ok(sh.arbiter_winner(&slot)))?,
    )?;
    // host.arbiter.winnerSpecificity(slot) -> number | nil — the rank of whoever currently
    // owns the slot. Lets a claimant skip work it cannot use: with twelve sample-library
    // overlays competing for one plugin, and only one library ever loaded, eleven of their
    // twelve landmark searches are known-pointless the moment any of them wins.
    let sh = shared.clone();
    arbiter.set(
        "winnerSpecificity",
        lua.create_function(move |_, slot: String| Ok(sh.arbiter_winner_specificity(&slot)))?,
    )?;
    // host.arbiter.participants(slot) -> { {module, specificity, matching, active}, … }
    // A DIAGNOSTIC, and the only way to catch the failure this design is most prone to:
    // slots are plain strings typed independently in modules that never see each other,
    // so a typo doesn't error — it silently creates a private slot where the overlay wins
    // every time (or never competes). Nothing else can notice that, because a slot with
    // one participant is perfectly legal. Every failure mode here is silent to a user who
    // cannot see the screen, so it has to be inspectable.
    let sh = shared.clone();
    arbiter.set(
        "participants",
        lua.create_function(move |lua, slot: String| {
            let arr = lua.create_table()?;
            let map = sh.arbiter.borrow();
            if let Some(s) = map.get(&slot) {
                let names = sh.ids.borrow();
                for (i, c) in s.claims.iter().enumerate() {
                    let t = lua.create_table()?;
                    t.set(
                        "module",
                        names.get(c.module_idx).cloned().unwrap_or_else(|| "?".to_string()),
                    )?;
                    t.set("specificity", c.specificity)?;
                    t.set("matching", c.matching)?;
                    t.set("active", s.active == Some(c.handle))?;
                    arr.set(i + 1, t)?;
                }
            }
            Ok(arr)
        })?,
    )?;
    host.set("arbiter", arbiter)?;

    // `include` is deliberately NOT installed here. It has to hand over the module's GATED
    // view, which does not exist yet at this point — see the call after `gated_view` in
    // `run_module_code`, and the note on `install_include` about why the two tables differ.

    // host.calibrating — true while "Calibration keys in overlays" is on (the Application
    // settings tab, or the AUTOMATION_PLATFORM_CALIBRATE variable for a launch with no window
    // to click in). The overlay runtime arms its calibration keys only then, so a normal user
    // never has those combinations taken away.
    //
    // Read once, when the VM is built — which is exactly why that setting's label says "reload
    // modules to apply" and not "immediately". Modules also use it to gate MEASUREMENTS that
    // are too expensive to run for somebody who is not measuring: a capture per selection
    // change is an instrument, not a feature.
    host.set(
        "calibrating",
        appcfg::calibrate(),
    )?;

    Ok(host)
}

/// `host.include(rel)` — load another FILE OF THIS MODULE and return whatever it returns.
///
/// A module was one file, which is why long modules exist: there was no way to put shared
/// helpers, a table of coordinates, or one overlay per file. This is that way. It is not
/// overlay-specific — an included file returns any value, typically a table of functions.
///
/// Semantics that matter:
///   * The file sees the SAME `host` as the file that included it, passed in rather than
///     taken from the globals. So for a code module — whose source is evaluated inside each
///     dependent's VM — an included file resolves paths, settings and resources against the
///     DEFINING module, exactly as its includer does. `install_include` is therefore
///     re-run for a dependency host with that host's own table.
///   * Executed ONCE per VM; later includes of the same file return the same value (a
///     module split into files must not re-run side effects per include). Cached per VM,
///     because a code module legitimately runs once per dependent VM.
///   * `..` and absolute paths are REJECTED. `host.path` / `host.resource.read` join
///     without normalizing, which lets a path escape the module directory; that is a
///     nuisance for reading a file and a different thing entirely for executing one.
///   * An include cycle is an error naming the file, not a stack overflow.
fn install_include(
    lua: &Lua,
    shared: &Rc<Shared>,
    idx: usize,
    host: &Table,
    hand: &Table,
) -> Result<()> {
    let sh = shared.clone();
    // The host table the included file will receive — `hand`, which is NOT necessarily the
    // table `include` is registered on.
    //
    // These were one argument, and it was a hole. `include` has to be REGISTERED on the
    // ungated table, because that is what a module's gated view falls through to; but what
    // an included file is HANDED must be the gated view, because an included file is the
    // module's own code and gets the module's own permissions. Passing the same table for
    // both meant every included file ran ungated — and since `include` needs no capability,
    // every module had that route. Measured, not deduced: a module declaring only `log`
    // reached `host.speech` and `host.screen` from a second file in its own package.
    let host_ref = std::rc::Rc::new(lua.create_registry_value(hand.clone())?);
    host.set(
        "include",
        lua.create_function(move |lua, rel: String| {
            let root = sh.root(idx);
            let root_abs = std::path::absolute(&root).unwrap_or(root.clone());
            let joined = root.join(&rel);
            let abs = std::path::absolute(&joined).unwrap_or(joined);
            if !abs.starts_with(&root_abs) {
                return Err(mlua::Error::external(format!(
                    "include '{rel}' resolves outside the module directory"
                )));
            }
            let key = abs.to_string_lossy().to_string();

            let cache: Table = match lua.named_registry_value::<Table>("__include_cache") {
                Ok(t) => t,
                Err(_) => {
                    let t = lua.create_table()?;
                    lua.set_named_registry_value("__include_cache", &t)?;
                    t
                }
            };
            // Cached as a one-element table so a file returning nil is still "done".
            if let Ok(box_) = cache.get::<Table>(key.as_str()) {
                return box_.get::<mlua::Value>(1);
            }
            let loading: Table = match lua.named_registry_value::<Table>("__include_loading") {
                Ok(t) => t,
                Err(_) => {
                    let t = lua.create_table()?;
                    lua.set_named_registry_value("__include_loading", &t)?;
                    t
                }
            };
            if loading.get::<bool>(key.as_str()).unwrap_or(false) {
                return Err(mlua::Error::external(format!(
                    "include cycle: '{rel}' is already being loaded"
                )));
            }
            loading.set(key.as_str(), true)?;

            let result = (|| -> mlua::Result<mlua::Value> {
                let src = std::fs::read_to_string(&abs).map_err(|e| {
                    mlua::Error::external(format!("include '{rel}': {e}"))
                })?;
                // The wrapper opens on the SAME line as the file's first line, so reported
                // line numbers match the file.
                let chunk = lua
                    .load(format!("return function(host) {src}\nend"))
                    .set_name(&key)
                    .into_function()?;
                let f: Function = chunk.call(())?;
                let host_tbl: Table = lua.registry_value(&host_ref)?;
                f.call::<mlua::Value>(host_tbl)
            })();
            loading.set(key.as_str(), mlua::Value::Nil)?;
            let value = result?;
            let box_ = lua.create_table()?;
            box_.set(1, value.clone())?;
            cache.set(key.as_str(), box_)?;
            Ok(value)
        })?,
    )?;
    Ok(())
}

/// Converts a Luau value into a stored setting value (scalars only).
fn lua_to_value(v: &mlua::Value) -> mlua::Result<settings::Value> {
    match v {
        mlua::Value::Boolean(b) => Ok(settings::Value::Bool(*b)),
        mlua::Value::Integer(i) => Ok(settings::Value::Int(*i as i64)),
        mlua::Value::Number(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                Ok(settings::Value::Int(*n as i64))
            } else {
                Ok(settings::Value::Float(*n))
            }
        }
        mlua::Value::String(s) => Ok(settings::Value::Str(s.to_str()?.to_string())),
        _ => Err(mlua::Error::external(
            "settings value must be a boolean, number, or string",
        )),
    }
}

/// Converts a stored setting value back into a Luau value.
fn value_to_lua(lua: &Lua, v: &settings::Value) -> mlua::Result<mlua::Value> {
    Ok(match v {
        settings::Value::Bool(b) => mlua::Value::Boolean(*b),
        settings::Value::Int(i) => mlua::Value::Integer(*i as mlua::Integer),
        settings::Value::Float(f) => mlua::Value::Number(*f),
        settings::Value::Str(s) => mlua::Value::String(lua.create_string(s)?),
    })
}

/// Builds a setting's schema from its `define(key, default, opts)` call.
fn field_from(key: &str, default: &settings::Value, opts: Option<&Table>) -> settings::Field {
    let mut field = settings::Field {
        kind: default.kind(),
        label: key.to_string(),
        default: default.clone(),
        min: None,
        max: None,
        choices: None,
    };
    if let Some(o) = opts {
        if let Ok(l) = o.get::<String>("label") {
            field.label = l;
        }
        if let Ok(m) = o.get::<f64>("min") {
            field.min = Some(m);
        }
        if let Ok(m) = o.get::<f64>("max") {
            field.max = Some(m);
        }
        if let Ok(choices) = o.get::<Table>("oneOf") {
            let list: Vec<String> = choices.sequence_values::<String>().flatten().collect();
            if !list.is_empty() {
                field.choices = Some(list);
            }
        }
    }
    field
}

/// Returns true if the given module VM registered any window triggers.
fn window_has_triggers(lua: &Lua) -> bool {
    (|| -> mlua::Result<bool> {
        let host: Table = lua.globals().get("host")?;
        let window: Table = host.get("window")?;
        let has: Function = window.get("_hasTriggers")?;
        has.call::<bool>(())
    })()
    .unwrap_or(false)
}

/// Reads `{ button = "left"|"right"|"middle" }` from input opts (default left).
fn button_from(opts: Option<&Table>) -> MouseButton {
    let name = opts.and_then(|o| o.get::<String>("button").ok()).unwrap_or_default();
    match name.to_ascii_lowercase().as_str() {
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

/// Reads an optional `{ region = { x1, y1, x2, y2 } }` (named or positional) and
/// returns `(x, y, w, h)`, defaulting to the full screen.
fn read_region(opts: Option<&Table>, sw: i32, sh: i32) -> (i32, i32, i32, i32) {
    match opts.and_then(|o| o.get::<Table>("region").ok()) {
        Some(region) => {
            let x1: i32 = region.get("x1").or_else(|_| region.get(1)).unwrap_or(0);
            let y1: i32 = region.get("y1").or_else(|_| region.get(2)).unwrap_or(0);
            let x2: i32 = region.get("x2").or_else(|_| region.get(3)).unwrap_or(sw);
            let y2: i32 = region.get("y2").or_else(|_| region.get(4)).unwrap_or(sh);
            (x1, y1, (x2 - x1).max(0), (y2 - y1).max(0))
        }
        None => (0, 0, sw, sh),
    }
}

/// Reads an optional `{ scales = { 1.0, 0.9, ... } }` list of scale factors for the
/// template search; empty when absent (= a single 1.0 pass, i.e. no scaling).
fn read_scales(opts: Option<&Table>) -> Vec<f32> {
    let mut v = Vec::new();
    if let Some(t) = opts.and_then(|o| o.get::<Table>("scales").ok()) {
        for s in t.sequence_values::<f32>().flatten() {
            v.push(s);
        }
    }
    v
}

/// Naive template search over a captured region (early-out per position; compares
/// RGB and honors the template's alpha as a mask). Returns the top-left offset.
fn find_template(
    hay: &CapturedImage,
    tw: u32,
    th: u32,
    tmpl: &[u8],
    tol: u8,
    probes: &[u32],
) -> Option<(u32, u32)> {
    if tw == 0 || th == 0 || tw > hay.w || th > hay.h {
        return None;
    }
    for oy in 0..=(hay.h - th) {
        for ox in 0..=(hay.w - tw) {
            if matches_at(hay, ox, oy, tw, th, tmpl, tol, probes) {
                return Some((ox, oy));
            }
        }
    }
    None
}

/// True if the template sits at (ox, oy) with every non-wildcard pixel inside `tol`.
///
/// `probes` are template pixel indices to test FIRST (see `probe_order`). They are a
/// subset of the same conjunction, so testing them early cannot change the verdict — it
/// only decides how fast the overwhelming majority of positions, which do NOT match, are
/// rejected. Row-major order begins at (0,0), which on a wordmark is background and
/// therefore agrees almost everywhere; measured on a real frame, that first comparison
/// eliminated 31 % of positions where a glyph pixel eliminates 99.5 %.
fn matches_at(
    hay: &CapturedImage,
    ox: u32,
    oy: u32,
    tw: u32,
    th: u32,
    tmpl: &[u8],
    tol: u8,
    probes: &[u32],
) -> bool {
    let tol = tol as i16;
    let px = |i: u32| -> bool {
        let ti = (i as usize) * 4;
        let (tx, ty) = (i % tw, i / tw);
        let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
        for c in 0..3 {
            if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                return false;
            }
        }
        true
    };
    for &i in probes {
        if !px(i) {
            return false;
        }
    }
    for ty in 0..th {
        for tx in 0..tw {
            let ti = ((ty * tw + tx) * 4) as usize;
            if tmpl[ti + 3] == 0 {
                continue; // transparent template pixel = wildcard
            }
            let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
            for c in 0..3 {
                if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                    return false;
                }
            }
        }
    }
    true
}

/// Resizes an RGBA template (`tw`×`th`) to `sw`×`sh` (bilinear), returning the new
/// raw RGBA bytes. Falls back to the original bytes if the buffer can't be wrapped.
fn resize_rgba(tmpl: &[u8], tw: u32, th: u32, sw: u32, sh: u32) -> Vec<u8> {
    match image::RgbaImage::from_raw(tw, th, tmpl.to_vec()) {
        Some(img) => {
            image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Triangle).into_raw()
        }
        None => tmpl.to_vec(),
    }
}

/// Multi-scale template search: tries each factor in `scales` (the needle resized to
/// factor·{tw,th}; 1.0 uses it as-is), returning the first match's top-left AND the
/// matched (scaled) size. Empty `scales` = a single 1.0 pass (the plain search). 1.0,
/// when present, is tried first, so an exact hit costs nothing extra and the other
/// scales are only reached when it misses. Resized needles rely on `tol` (bilinear
/// interpolation perturbs pixels), so pair scaling with a non-zero colour tolerance.
fn find_template_scaled(
    hay: &CapturedImage,
    tw: u32,
    th: u32,
    tmpl: &[u8],
    tol: u8,
    scales: &[f32],
    probes: &[u32],
) -> Option<(u32, u32, u32, u32)> {
    let one = [1.0f32];
    let list: &[f32] = if scales.is_empty() { &one } else { scales };
    for &s in list {
        if s <= 0.0 {
            continue;
        }
        let (sw, sh) = if (s - 1.0).abs() < 1e-4 {
            (tw, th)
        } else {
            (((tw as f32) * s).round() as u32, ((th as f32) * s).round() as u32)
        };
        if sw == 0 || sh == 0 || sw > hay.w || sh > hay.h {
            continue;
        }
        let hit = if sw == tw && sh == th {
            find_template(hay, tw, th, tmpl, tol, probes)
        } else {
            // A resized needle has different pixels, so the precomputed probe indices no
            // longer point at the same content — fall back to the plain scan for those.
            let scaled = resize_rgba(tmpl, tw, th, sw, sh);
            find_template(hay, sw, sh, &scaled, tol, &[])
        };
        if let Some((ox, oy)) = hit {
            return Some((ox, oy, sw, sh));
        }
    }
    None
}

/// Converts a native window snapshot into the Lua table modules see:
/// `{ id, title, class, app = { name, exe, pid }, bounds = { x, y, w, h } }`.
/// Finds the module directory whose manifest id matches — for auto-discovering
/// declared dependencies. Looks BESIDE the module first, then in a `modules`
/// directory next to that.
///
/// The first is the installed layout, where everything sits side by side in the
/// portable `modules/` dir. The second is for a source tree that separates its
/// modules from its examples: `examples/overlay-attach` depends on the overlay
/// runtime, which lives in `modules/` and is therefore not its sibling. Without
/// the fallback, splitting the two would break exactly the examples that show
/// how a dependency is used.
fn find_module_dir(parent: &Path, id: &str) -> Option<PathBuf> {
    fn scan(dir: &Path, id: &str) -> Option<PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Ok(m) = LoadedModule::load_dir(&p) {
                    if m.manifest.id == id {
                        return Some(p);
                    }
                }
            }
        }
        None
    }
    if let Some(found) = scan(parent, id) {
        return Some(found);
    }
    let beside = parent.parent()?.join("modules");
    if beside == parent {
        return None; // already scanned
    }
    scan(&beside, id)
}

fn control_to_table(lua: &Lua, c: &ControlInfo) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", c.hwnd)?;
    t.set("class", c.class.clone())?;
    let b = lua.create_table()?;
    b.set("x", c.x)?;
    b.set("y", c.y)?;
    b.set("w", c.w)?;
    b.set("h", c.h)?;
    t.set("bounds", b)?;
    let cl = lua.create_table()?;
    cl.set("x", c.client_x)?;
    cl.set("y", c.client_y)?;
    cl.set("w", c.client_w)?;
    cl.set("h", c.client_h)?;
    t.set("client", cl)?;
    Ok(t)
}

/// One element of an accessibility dump, as Lua sees it.
///
/// The rectangle is screen coordinates, and it is the half of a dump that makes it usable:
/// a list of what a plugin contains, without where any of it is, cannot be turned into an
/// overlay by somebody who cannot look at the screen.
fn dump_node_to_table(lua: &Lua, n: backend::DumpNode) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("depth", n.depth)?;
    t.set("name", n.name)?;
    t.set("class", n.class)?;
    t.set("ctype", n.ctype)?;
    let b = lua.create_table()?;
    b.set("x", n.x)?;
    b.set("y", n.y)?;
    b.set("w", n.w)?;
    b.set("h", n.h)?;
    t.set("bounds", b)?;
    Ok(t)
}

fn win_to_table(lua: &Lua, w: &WinInfo) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", w.hwnd)?;
    t.set("title", w.title.clone())?;
    t.set("class", w.class.clone())?;

    let app = lua.create_table()?;
    let name = w
        .exe
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| w.exe.clone());
    app.set("name", name)?;
    app.set("exe", w.exe.clone())?;
    app.set("bundleId", w.bundle_id.clone())?;
    app.set("pid", w.pid)?;
    t.set("app", app)?;

    let b = lua.create_table()?;
    b.set("x", w.x)?;
    b.set("y", w.y)?;
    b.set("w", w.w)?;
    b.set("h", w.h)?;
    t.set("bounds", b)?;

    // Client-area origin (screen coords) — overlay regions are relative to this.
    let cl = lua.create_table()?;
    cl.set("x", w.client_x)?;
    cl.set("y", w.client_y)?;
    cl.set("w", w.client_w)?;
    cl.set("h", w.client_h)?;
    t.set("client", cl)?;

    Ok(t)
}
