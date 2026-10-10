//! Host runtime: a module **manager** that hosts many Luau modules concurrently
//! in one process (one VM each, shared services + one event loop), per
//! `docs/module-runtime-and-lifecycle.md`. The OS is reached only through
//! [`backend::Backend`]. The `host` API (design: primitives are first-class —
//! see `docs/host-api-capability-catalog.md`) is installed per module VM and
//! bound to that module's root + the shared services.

mod backend;
/// Which build is running: the executable's commit and the package's. See the file.
pub mod build_info;
mod capture_source;
/// `host.screen.cells`: the predicate, the grid and the ranking, pure — see the file.
mod cells;
/// The golden test of `cells` against an outside reader's own vectors (local data only).
#[cfg(test)]
mod cells_golden_tests;
/// The overlay runtime's menu tests (modules/overlay-runtime), run against a scripted host.
#[cfg(test)]
mod overlay_menu_tests;
/// The Tab pass-through's unnamed stops, read by OCR, against the same scripted host.
#[cfg(test)]
mod overlay_passthrough_tests;
/// A control's value and `ocrLabel`, read off the event loop on focus, against the same host.
#[cfg(test)]
mod overlay_focus_read_tests;
/// The DAW's plug-in panel on a Mac, the stay, and daw-hosts / sforzando / Kontakt loaded against
/// the same scripted host with the window prelude's real matcher.
#[cfg(test)]
mod overlay_host_panel_tests;
/// The runtime's coordinate scaling (O:scale), and ARC ON:EAR held to its old geometry on it.
#[cfg(test)]
mod overlay_scale_tests;
/// Choosing an item from a plug-in's own popup menu, once a menu test sees it.
#[cfg(test)]
mod overlay_menu_item_tests;
/// VPS Avenger's module loaded against the same scripted host: its header read, its zoom and
/// scale, its clicks, its menus and its warning box.
#[cfg(test)]
mod overlay_avenger_tests;
/// The overlay runtime's marks (O:here, O:stillHere), O:toScreenRect's `whole`, and a control's
/// hook that raises, against the same scripted host.
#[cfg(test)]
mod overlay_runtime_marks_tests;
/// The overlay runtime under handlers (runtime 0.3.0): the mark before a control's hooks, the scan
/// in a pass of its own, a stored identify asked in the handler, the first evaluation on the next
/// tick, and identify and present guarded — against the same scripted host.
#[cfg(test)]
mod overlay_handler_tests;
/// Melodyne's module loaded against the same scripted host: its read-out watcher and its note
/// area's time signature, read with a callback, and what their answers may still say.
#[cfg(test)]
mod overlay_melodyne_tests;
/// Tasks and their two waits against a real Luau VM and a real read service over a fake
/// recogniser.
#[cfg(test)]
mod task_tests;
/// Two modules' key scopes and menu flags against real Luau VMs and the stub backend, one of
/// them busy: which module a key goes to, and that a late teardown sets back only its own.
#[cfg(test)]
mod key_scope_tests;
/// The mailboxes against real Luau VMs, the stub backend and a real read service: every event
/// as a handler, one at a time per module, and what waits, folds and drops while a module is busy.
#[cfg(test)]
mod mailbox_tests;
/// Captured keys on the host's side: the registrations, and each module's scope and menu flag —
/// see the file.
mod captures;
/// One mailbox per module: every event runs through it as a handler, one at a time per module —
/// see the file.
mod mailbox;
/// Handlers: each callback of a module as a coroutine that can stop at a wait while the event
/// loop goes on — no module's API; see the file.
mod task;
/// The guard that keeps text recognition off the event loop — see the file.
mod loop_guard;
/// The guard over every module VM: its memory and time limits, every call from the host into its
/// Lua, the watchdog that arms a callback past its time, and the stop of a module past a limit —
/// see the file.
mod vm_guard;
/// The event loop's processor time, read from the guard's watchdog thread — see the file.
/// Borrowed by crates/macos-check.
mod thread_cpu;
/// The guard's entries, limits and stops against real Luau VMs.
#[cfg(test)]
mod vm_guard_tests;
/// What the host does with a stopped module: off until the next start, its keys back, its mouse
/// buttons let go of, the keys of the stall dropped, and the messages — see the file.
mod stops;
/// The stop's two steps, its words and the manager's details, against a recording host.
#[cfg(test)]
mod stops_tests;
mod gui;
mod image_search;
/// One running copy per user: the lock, and the request a second start sends — see the file.
mod instance;
mod json;
pub mod logging;
/// `host.ocr.recognize`: the capture and recognise threads, their queue, languages and the shape a
/// module is handed — see the folder.
mod ocr;
mod appcfg;
mod portable;
/// `host.screen.profile`'s reduction of a picture, per column and row, pure — see the file.
mod profile;
/// The window-relative Region form, `{ window, fraction }`, resolved to pixels, and the rule for
/// strict corners — see the file.
mod region;
/// The one strict reader of a Region argument, for the cells calls and `host.ocr.recognize`.
mod region_lua;
pub mod registry;
mod settings;
/// `host.screen.snapshot` and `pixels`: the Snapshot handle, its budget, and the rules every
/// read of a snapshot applies — see the file.
mod snapshot;
/// `host.sound`'s audio output, opened again when its device went away — see the file.
mod sound;
mod speech;
/// What the operating system does to the whole session (sleep, lock, the session going and
/// coming back, display changes), the `[system]` line that says how long each lasted, and what
/// the host does about it — see the file.
mod system_events;
mod template;
/// `host.timer`: pending timers, their tokens, and the firing rules — see the file.
mod timers;
/// `host.gamepad`: listeners, dispatch rules and bindings — see the file.
mod gamepad_api;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use mlua::{Function, IntoLuaMulti, Lua, LuaSerdeExt, RegistryKey, Table};

use backend::{Backend, ControlInfo, HostEvents, MouseButton, WinInfo};
use image_search::{ImageResult, ImageTask, PendingImage};
use mailbox::{Event, Opened};
use module_manifest::LoadedModule;
use template::Decoded;
use vm_guard::host_call;

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
    /// grabbing the device can interrupt it. Opened again when it failed, when the default
    /// device changed, and after a resume (sound.rs).
    audio: RefCell<Option<sound::Output>>,
    /// Hotkey ids, handed out in order and never reused.
    ///
    /// **Past 0xBFFF on Windows.** `RegisterHotKey`'s documentation gives applications ids
    /// 0x0000–0xBFFF, and these are handed to it as they are. The overlay runtime registers
    /// and unregisters its control hotkeys at every activation, 300 to 1200 a busy hour, so a
    /// session of weeks passes 0xBFFF. Measured on Windows 11 (build 26220, 2026-09-27):
    /// Windows grants ids 1, 0xBFFF, 0xC000, 0xFFFF, 0x10000 and 0x7FFFFFFE alike (the uptime
    /// audit's `hotkey-id-range.ps1`), so nothing is mapped into the documented range — an
    /// `i32` lasts centuries at that rate. TODO.md keeps the check for other Windows builds;
    /// a build that did refuse would say so in the log, as `hotkey '…' rejected by OS`.
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
    /// Captured keys, each with its module, release token, VM and callback, and each module's
    /// key scope and menu flag — see captures.rs. The per-capture token lets `release` target
    /// the exact registration: two overlays in the SAME module that both capture a key (e.g.
    /// Komplete Kontrol's chrome and its Preferences dialog both capturing Tab) would otherwise
    /// collide on (vk, mask, module_idx), so the first to deactivate would release the key the
    /// second still needs. Keyed by token, one overlay's release can't drop another's.
    captures: captures::Captures,
    /// Unified portable store: per-module enabled-state + settings.
    store: RefCell<settings::Store>,
    /// module_idx → (setting key → schema), for validation + the GUI. Not persisted.
    schemas: RefCell<Vec<HashMap<String, settings::Field>>>,
    /// onChange callbacks: (module_idx whose SETTING it watches, key) → [(module_idx that owns
    /// the VM it was registered from, VM, callback)]. The two indices differ for a code
    /// dependency's code, which watches the dependency's settings from inside a dependent's
    /// VM; see `drop_on_change_from`.
    on_change: RefCell<OnChangeMap>,
    /// Coalesces setting auto-saves to the event-loop tick.
    dirty: Cell<bool>,
    /// `host.timer`'s pending timers, one-shot and recurring, fired from the tick. Recurring
    /// ones are re-armed even while the owning module is disabled, so a poll resumes on
    /// re-enable instead of dying. See timers.rs.
    timers: timers::Timers,
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
    /// ms in activate, ms in focus-change, ms in key dispatch, ms in gamepad dispatch). Reset
    /// each iteration and reported when one overruns — "os events 446 ms" names no cause on
    /// its own, and this phase is several different fan-outs with very different multiplicities.
    ev_counts: Cell<(u32, u128, u128, u128, u128)>,
    /// `host.gamepad`'s listeners — see gamepad_api.rs.
    pads: gamepad_api::Pads,
    epoch: Cell<u64>,
    /// See bump_input_epoch: turns over only when something ACTED on the screen.
    input_epoch: Cell<u64>,
    /// Monotonic id source for arbiter claim handles.
    next_arbiter: Cell<i64>,
    /// Module callback failures (Lua errors + caught panics) queued for the GUI to
    /// show in an accessible dialog, as (title, message). Drained each tick.
    errors: RefCell<Vec<(String, String)>>,
    /// Dedup keys (id+context) so a module's fault in a given context dialogs once
    /// per enabled session (cleared on toggle). Keyed coarsely so a per-tick-varying message
    /// can't flood.
    error_seen: RefCell<HashSet<String>>,
    /// The log's side of the same faults: an error of one module, in one context, with one
    /// message, is written the first time and then counted, with at most one summary a minute
    /// (`logging::Repeats`; flushed on the tick by `log_housekeeping`). A poll that fails every
    /// 500 ms wrote 170 000 identical lines a day. Forgotten per module on toggle, as the
    /// dialogs are, and on a reload or a rolled-back hot-load too (`forget_error_repeats`), so
    /// the same error after a fix is written in full.
    error_repeats: RefCell<logging::Repeats>,
    /// When `log_housekeeping` last asked `error_repeats` for its summaries: once a second is
    /// enough for a count written at most once a minute.
    repeats_asked: Cell<Option<Instant>>,
    /// Hotkeys another application holds, per (module, spec), with how often the OS refused
    /// them since the refusal was logged. `refresh_hotkeys` tries such a claim again at every
    /// register and unregister — the overlay runtime makes several per focus move — and each
    /// try logged "rejected by OS" again for as long as the other application held the key.
    /// Now the first refusal is logged, the rest counted, and the count said when the key is
    /// granted. Forgotten per module on toggle and when its registrations go.
    os_refused: RefCell<HashMap<(usize, String), u32>>,
    /// The captured-set line (under trace) waiting for the end of the tick, and
    /// the last one written. `refresh_captured` runs once per key an overlay captures, so an
    /// activation of eight keys wrote eight growing lines of up to 500 bytes; the tick writes
    /// the set once, and only when it differs from the last one written.
    captured_line: RefCell<(Option<String>, String)>,
    /// When the machine went to sleep and the session was locked, for the `[system]` lines, and
    /// the counts of the system events that came again and again.
    system_since: RefCell<system_events::Since>,
    /// The `[env]` display lines last written: at start, and after a display or scale change
    /// that changed them. A display that flaps — a TV or receiver in standby toggling its link —
    /// sends a display change every few seconds, and the same three lines at each were most of
    /// the log.
    display_env_said: RefCell<Vec<(String, String)>>,
    /// Async image search: queued template matches go to a worker thread; results
    /// come back on the loop tick. Pending callbacks are keyed by request id and
    /// touched only on the main thread.
    image_tasks: std::sync::mpsc::Sender<ImageTask>,
    image_results: std::sync::mpsc::Receiver<ImageResult>,
    pending_image: RefCell<HashMap<u64, PendingImage>>,
    /// Requests ended by a newer one of their module past `IMAGE_PER_OWNER`, with the reason
    /// they are answered on the next tick (image_search.rs, `submit`).
    ended_images: RefCell<Vec<(PendingImage, String)>>,
    next_image_id: Cell<u64>,
    /// What every module's snapshots hold between them, against the application's budget
    /// (snapshot.rs). Each VM keeps its own total in its app data beside this one.
    snap_bytes: Rc<Cell<usize>>,
    /// `host.screen.snapshotAsync`'s callbacks waiting for the capture thread, and the answers
    /// decided without it (snapshot.rs).
    snap_state: snapshot::SnapState,
    /// `host.ocr.recognize`: the capture and recognise threads (ocr/service.rs)...
    ocr: ocr::service::Service<backend::OcrShot>,
    /// ...and what the event loop keeps for them: the callbacks waiting, the answers decided
    /// without the threads, the newest read per key (ocr/lua.rs).
    ocr_state: ocr::lua::OcrState,
    /// The handlers, running and waiting, of every module, and what a blocking call held the event
    /// loop for where it could not wait (task.rs).
    tasks: task::Tasks,
    /// Each module's events that wait while it is busy (mailbox.rs).
    mail: mailbox::Mailboxes,
    /// Every module VM's slot: its memory limit, the calls into it, whether it was stopped
    /// (vm_guard.rs). Every VM the host builds comes from `vm_guard::new_vm(&shared.guard)`.
    guard: Arc<vm_guard::Guard>,
    /// The modules the guard stopped: off until the next start, and the stops not settled yet
    /// (stops.rs).
    stops: stops::Stops,
    /// Rows of the manager that changed without the user changing them — a module stopped, or
    /// turned on again after a stop — drained by the GUI's tick.
    module_changes: RefCell<Vec<gui::ModuleChange>>,
    /// The mouse buttons each module pressed with `host.input.mouseDown` and has not let go of,
    /// by module index: released where the pointer is when the module stops running — stopped by
    /// the guard, disabled, reloaded, rolled back, or the application ends
    /// (`release_held_buttons`). A held button belongs to the whole desktop, so this lets go of
    /// it for everyone.
    held_buttons: RefCell<BTreeMap<usize, Vec<MouseButton>>>,
    /// module_idx → the generation of the VM it runs now (see `image_search::VmOwner`). A map,
    /// not a fifth parallel vector: `populate_vm` overwrites the entry for its index, so a
    /// rollback has nothing here to keep aligned.
    vm_gens: RefCell<HashMap<usize, u64>>,
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
    /// Modules owed a report of the window already in front — `onTrigger { initial = true }` —
    /// as (module_idx, reprime), at most one entry per module. Filled by the prelude's
    /// `_requestInitial` (reprime false) and by `apply_enabled(true)` (reprime true: every
    /// `initial` trigger of the module is primed again); drained on the tick by
    /// `Dispatcher::dispatch_initial`.
    initial_pending: RefCell<Vec<(usize, bool)>>,
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
    /// `host.screen.profile` calls this epoch, those on a snapshot among them, and their time
    /// split three ways ([`ProfileCost`]).
    profile: ProfileCost,
    /// `host.screen.profile` answers given to a callback this epoch ([`ProfileCbCost`]).
    profile_cb: ProfileCbCost,
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

/// What `host.screen.profile` cost in one epoch, on the screen and on snapshots alike: a live
/// profile is also one of the epoch's pixel reads, a snapshot's is not, and both run their
/// reduction on the main thread. Microseconds.
#[derive(Default)]
struct ProfileCost {
    calls: u32,
    /// Of `calls`, those on a snapshot: their "capture" is the clip, no screen.
    on_snapshot: u32,
    capture_us: u128,
    reduction_us: u128,
    tables_us: u128,
}

impl ProfileCost {
    /// One call: from its start to the picture, to the reduction's end, and to its tables' end.
    fn add(&mut self, start: Instant, captured: Instant, reduced: Instant, end: Instant, snapshot: bool) {
        self.calls += 1;
        self.on_snapshot += u32::from(snapshot);
        self.capture_us += captured.duration_since(start).as_micros();
        self.reduction_us += reduced.duration_since(captured).as_micros();
        self.tables_us += end.duration_since(reduced).as_micros();
    }

    /// The observe line's words for it, empty when there was no call:
    /// `; profile ×2 94.0 ms = capture 18.1 + reduction 66.3 + tables 9.6 (1 on a snapshot)`.
    fn clause(&self) -> String {
        if self.calls == 0 {
            return String::new();
        }
        let ms = |us: u128| us as f64 / 1000.0;
        let snapshots = if self.on_snapshot > 0 { format!(" ({} on a snapshot)", self.on_snapshot) } else { String::new() };
        format!(
            "; profile ×{} {:.1} ms = capture {:.1} + reduction {:.1} + tables {:.1}{snapshots}",
            self.calls,
            ms(self.capture_us + self.reduction_us + self.tables_us),
            ms(self.capture_us),
            ms(self.reduction_us),
            ms(self.tables_us)
        )
    }
}

/// What `host.screen.profile` with a callback cost in one epoch: the answers handed over, the event
/// loop's share of each — its Lua tables, built as the callback runs — and beside it the reduction,
/// which ran on the image worker. Their pictures were taken on the `screen-capture` thread, whose
/// own lines time them. Microseconds.
#[derive(Default)]
struct ProfileCbCost {
    calls: u32,
    /// Of `calls`, those of a snapshot: nothing was captured for them.
    on_snapshot: u32,
    reduction_us: u128,
    tables_us: u128,
}

impl ProfileCbCost {
    /// One answer: its reduction on the worker, and its tables on the event loop.
    fn add(&mut self, reduction: Duration, tables: Duration, snapshot: bool) {
        self.calls += 1;
        self.on_snapshot += u32::from(snapshot);
        self.reduction_us += reduction.as_micros();
        self.tables_us += tables.as_micros();
    }

    /// The observe line's words for it, empty when there was none:
    /// `; profile with a callback ×2: tables 1.2 ms on the event loop, reduction 30.4 ms on the image worker (1 of a snapshot)`.
    fn clause(&self) -> String {
        if self.calls == 0 {
            return String::new();
        }
        let ms = |us: u128| us as f64 / 1000.0;
        let snapshots = if self.on_snapshot > 0 { format!(" ({} of a snapshot)", self.on_snapshot) } else { String::new() };
        format!(
            "; profile with a callback ×{}: tables {:.1} ms on the event loop, reduction {:.1} ms on the image worker{snapshots}",
            self.calls,
            ms(self.tables_us),
            ms(self.reduction_us)
        )
    }
}

#[cfg(test)]
mod profile_cost_tests {
    use super::{ProfileCbCost, ProfileCost};
    use std::time::{Duration, Instant};

    #[test]
    fn the_profile_cost_splits_into_capture_reduction_and_tables_and_counts_snapshots() {
        let t0 = Instant::now();
        let at = |us: u64| t0 + Duration::from_micros(us);
        let mut c = ProfileCost::default();
        assert_eq!(c.clause(), "", "no call, no words");
        c.add(t0, at(18_100), at(84_400), at(92_000), false);
        c.add(t0, at(0), at(2_000), at(2_000), true);
        assert_eq!(
            c.clause(),
            "; profile ×2 94.0 ms = capture 18.1 + reduction 68.3 + tables 7.6 (1 on a snapshot)"
        );
    }

    /// A profile with a callback is counted apart: only its tables are the event loop's.
    #[test]
    fn a_profile_with_a_callback_counts_its_tables_on_the_loop_and_its_reduction_on_the_worker() {
        let mut c = ProfileCbCost::default();
        assert_eq!(c.clause(), "", "no answer, no words");
        c.add(Duration::from_micros(30_100), Duration::from_micros(900), false);
        c.add(Duration::from_micros(300), Duration::from_micros(300), true);
        assert_eq!(
            c.clause(),
            "; profile with a callback ×2: tables 1.2 ms on the event loop, reduction 30.4 ms on the image worker (1 of a snapshot)"
        );
    }
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
    /// cost that a whole-millisecond clock can see, any pixel read — each one a
    /// compositor frame — or any profile, a snapshot's included, whose reduction runs on the
    /// event loop wherever its picture came from, or one given a callback, whose tables do.
    /// The total keeps only a sanity bound: one question per overlay is what an idle tick
    /// costs, and 251 was the most seen with every library loaded, so a thousand in one
    /// epoch is somebody asking in a loop, whatever the cache made of it.
    fn observations(&self) -> std::cell::RefMut<'_, Observations> {
        let now = self.epoch.get();
        let now_input = self.input_epoch.get();
        let mut obs = self.observations.borrow_mut();
        if obs.epoch != now || obs.input_epoch != now_input {
            let reached_os = obs.asked - obs.served;
            // Taken at every turnover so each epoch's line counts its own; empty — and the line
            // unchanged — until a module reads through desktop duplication.
            let duplication =
                capture_source::duplication_clause(backend::take_duplication_counters());
            if reached_os >= 2
                || obs.binding_us >= 1000
                || obs.pixels > 0
                || obs.profile.calls > 0
                || obs.profile_cb.calls > 0
                || obs.asked >= 1000
                || !duplication.is_empty()
            {
                logging::line(
                    "observe",
                    &format!(
                        "epoch served {} of {} OS question(s) from cache ({} actually \
                         asked), {:.1} ms rebuilding Lua tables in window.controls and \
                         window.focusChain, plus {} screen pixel read(s) costing {:.1} ms{}{}{duplication}",
                        obs.served,
                        obs.asked,
                        obs.asked - obs.served,
                        obs.binding_us as f64 / 1000.0,
                        obs.pixels,
                        obs.pixel_us as f64 / 1000.0,
                        obs.profile.clause(),
                        obs.profile_cb.clause()
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
            obs.profile = ProfileCost::default();
            obs.profile_cb = ProfileCbCost::default();
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
    ///
    /// The `screen-capture` thread is told the new value (`Service::note_input_epoch`) before
    /// anything acts, so a snapshot it takes carries the epoch as it stood once its capture was
    /// back: input the host drives after that turns it over after the picture.
    fn bump_input_epoch(&self) {
        let e = self.input_epoch.get().wrapping_add(1);
        self.input_epoch.set(e);
        self.ocr.note_input_epoch(e);
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
        let dec = Arc::new(Decoded::from_png_rgba(dw, dh, img.into_raw()));
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
    ///
    /// The log line is written the first time an error of this module, in this context, with
    /// this message comes, and then counted — at most one summary a minute, written by the next
    /// occurrence or by the tick (`log_housekeeping`) — see `error_repeats`.
    fn report_callback_error(&self, module_idx: usize, context: &str, message: &str) {
        let id = self.ids.borrow().get(module_idx).cloned().unwrap_or_else(|| "?".into());
        let key = format!("{id}\u{1}{context}\u{1}{message}");
        let said = self.error_repeats.borrow_mut().note(&key, Instant::now());
        match said {
            logging::Said::Line => logging::line(context, &format!("[{id}] {message}")),
            logging::Said::Counted => {}
            logging::Said::Summary { count, over } => {
                logging::line(context, &repeat_summary(&id, context, message, count, over))
            }
        }
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
        // The log keeps the spec; the dialog says the key in the words a user looks for it by,
        // which on a Mac are not the spec's (see `dialog_key_words_for`).
        let key = backend::dialog_key_words_for(backend::KeyOs::CURRENT, spec);
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
                "Module \u{201c}{me}\u{201d} wants the {kind} {key}, and module \u{201c}{owner}\u{201d} is using it. Only one module can hold a combination at a time. {key} passes on by itself as soon as \u{201c}{owner}\u{201d} releases it \u{2014} disabling or removing that module in the module manager is enough, and nothing needs restarting."
            ),
        );
    }

    /// Surfaces a hotkey the OS itself rejected — held by another application, not
    /// one of our modules (or an unparseable spec). The module stays loaded.
    ///
    /// For a hotkey, called for the first refusal only: `refresh_hotkeys` tries the claim again
    /// at every register and unregister, and counts a refusal that repeats (`os_refused`).
    fn report_os_conflict(&self, idx: usize, kind: &str, spec: &str, err: &str) {
        let me = self.ids.borrow().get(idx).cloned().unwrap_or_default();
        logging::line(
            "conflict",
            &format!(
                "[{me}] {kind} '{spec}' rejected by OS: {err} (tried again whenever a hotkey is \
                 registered or released; a refusal after this one is counted, and the count \
                 said when it is granted)"
            ),
        );
        let key = backend::dialog_key_words_for(backend::KeyOs::CURRENT, spec);
        self.queue_dialog(
            format!("{me}\u{1}osconflict\u{1}{kind}\u{1}{spec}"),
            "Binding unavailable".to_string(),
            format!(
                "The {kind} {key} for module \u{201c}{me}\u{201d} couldn\u{2019}t be registered \u{2014} another application already uses it system-wide."
            ),
        );
    }

    /// Takes the queued module errors for the GUI to display (called each tick).
    fn drain_errors(&self) -> Vec<(String, String)> {
        std::mem::take(&mut *self.errors.borrow_mut())
    }

    /// Forgets the log's counts of module `idx`'s errors (and of its image searches ended past
    /// their limit), so their next occurrence is written in full: on disable/enable, on a reload,
    /// and when a failed hot-load is rolled back. Keyed by the module's id, as `error_repeats`
    /// is, so it is asked while `ids` still holds it. Its line for a blocking call that held the
    /// event loop is written again too (`Tasks::forget_legacy_said`).
    fn forget_error_repeats(&self, idx: usize) {
        if let Some(id) = self.ids.borrow().get(idx) {
            self.error_repeats.borrow_mut().forget_prefix(&format!("{id}\u{1}"));
            self.tasks.forget_legacy_said(id);
        }
    }

    /// The log's end-of-tick work, on both event loops: the captured-set line `refresh_captured`
    /// left, once per tick and only when it changed; and, once a second, the counts of module
    /// errors that stopped repeating before their minute was up, and of system events that came
    /// again and again (`system_events::Since::due`).
    fn log_housekeeping(&self) {
        let pending = self.captured_line.borrow_mut().0.take();
        if let Some(line) = pending {
            let mut c = self.captured_line.borrow_mut();
            if line != c.1 {
                logging::line("keys", &line);
                c.1 = line;
            }
        }
        let now = Instant::now();
        if self.repeats_asked.get().is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(1)) {
            return;
        }
        self.repeats_asked.set(Some(now));
        let due = self.error_repeats.borrow_mut().due(now);
        for (key, count, over) in due {
            let mut parts = key.splitn(3, '\u{1}');
            let (id, context, message) =
                (parts.next().unwrap_or("?"), parts.next().unwrap_or("?"), parts.next().unwrap_or(""));
            logging::line(context, &repeat_summary(id, context, message, count, over));
        }
        let system = self.system_since.borrow_mut().due(now);
        for line in system {
            logging::line("system", &line);
        }
    }
}

/// The line that sums up a module error that came again `count` times over `over`.
fn repeat_summary(id: &str, context: &str, message: &str, count: u64, over: Duration) -> String {
    // The message once more, cut: the full text is in the line that was written first.
    let short: String = message.chars().take(200).collect();
    let cut = if short.len() < message.len() { "…" } else { "" };
    format!(
        "[{id}] the same {context} error came {count} more time(s) in the last {} s (written in \
         full the first time; counted after that, with a line like this at most once a minute): \
         {short}{cut}",
        over.as_secs()
    )
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

/// `Shared::on_change`: (setting owner, key) → [(VM owner, VM, callback, registration id)]. The
/// id is how a queued `Event::Setting` finds its registration again when it runs.
pub(crate) type OnChangeMap = HashMap<(usize, String), Vec<(usize, Lua, RegistryKey, u64)>>;

/// Registration `reg` of an `onChange` for module `setting_of`'s `key`, as it runs, looked up in
/// `map` now: its call with the new and the old value, made in its VM — or nothing, when it was
/// dropped meanwhile. The map is borrowed for the look-up only. The host's own `open` and the
/// mailbox tests' holder both run this, so the tests run the host's rule and not a copy of it.
pub(crate) fn open_on_change(
    map: &RefCell<OnChangeMap>,
    setting_of: usize,
    reg: u64,
    key: &str,
    new: &settings::Value,
    old: Option<&settings::Value>,
) -> Opened {
    let found = {
        let map = map.borrow();
        map.get(&(setting_of, key.to_string())).and_then(|list| {
            list.iter()
                .find(|(.., id)| *id == reg)
                .and_then(|(_, lua, rk, _)| lua.registry_value::<Function>(rk).ok().map(|f| (lua.clone(), f)))
        })
    };
    let Some((lua, f)) = found else { return Opened::Gone };
    let new_v = value_to_lua(&lua, new).unwrap_or(mlua::Value::Nil);
    let old_v = old.and_then(|o| value_to_lua(&lua, o).ok()).unwrap_or(mlua::Value::Nil);
    let args = mlua::MultiValue::from_vec(vec![new_v, old_v]);
    Opened::Run { f, args, ctx: task::Ctx::new(format!("settings onChange ({key})"), setting_of) }
}

/// The last `onChange` registration id handed out: positive, never reused.
static NEXT_ON_CHANGE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A module setting `define` found stored with another value than its default: one `[settings]`
/// line per module and key a run, however many VMs define it (a code module's setting is defined
/// in every VM that carries it). The log header names the application's switches; this names a
/// module's, which change how the rest of the log reads as much — calibration among them.
fn note_stored_setting(id: &str, key: &str, value: &settings::Value, default: &settings::Value) {
    static SAID: std::sync::Mutex<std::collections::BTreeSet<(String, String)>> =
        std::sync::Mutex::new(std::collections::BTreeSet::new());
    let first = SAID.lock().unwrap_or_else(|e| e.into_inner()).insert((id.to_string(), key.to_string()));
    if first {
        logging::line("settings", &format!("{id} {key} is {value} as stored (its default is {default})"));
    }
}

/// Drops every `onChange` registration made from a VM `gone` names, and every key left with
/// none.
///
/// Matched on the module that owns the VM the callback was REGISTERED from, never on whose
/// setting it watches, and the difference is a code dependency. Its code runs inside each
/// dependent's VM with the dependency's own settings table (`build_dep_host`), so a callback
/// it registers is filed under the DEPENDENCY's index while living in the DEPENDENT's VM.
/// Purging by the settings owner — which this did — left such a callback behind when the
/// dependent was reloaded: the old VM stayed alive for its sake, and its callback fired beside
/// the new VM's own every time the dependency's setting changed. The same rule is why an image
/// search is purged by its owner (`image_search::purge_owner`).
///
/// The other way round, a dependency's reload no longer takes the callbacks its code
/// registered inside a dependent: that VM is still alive and still running the code that
/// registered them. They go when the dependent is rebuilt — next, by the reload cascade, for
/// a module that lists the code module in `dependencies`; only at its own reload for one that
/// reaches it through `optional_dependencies` (the cascade does not follow those) or when the
/// code module's own rebuild failed (which skips the cascade). Until then that VM still runs
/// the old code, so its callbacks are still the right ones.
fn drop_on_change_from(map: &mut OnChangeMap, gone: impl Fn(usize) -> bool) {
    for list in map.values_mut() {
        list.retain(|(owner, ..)| !gone(*owner));
    }
    map.retain(|_, list| !list.is_empty());
}

/// `purge_module(idx)`'s share of `on_change`: every callback registered from module `idx`'s
/// VM, whoever's setting it watches — see `drop_on_change_from`. Named, and called by the
/// tests, so that the rule the tests check is the one `purge_module` runs.
fn purge_on_change(map: &mut OnChangeMap, idx: usize) {
    drop_on_change_from(map, |owner| owner == idx);
}

/// `rollback_to(n)`'s share of `on_change`, both halves: every entry for a setting of a module
/// that is going away, and every callback a VM that is going away registered for a setting of
/// a dependency that stays.
fn rollback_on_change(map: &mut OnChangeMap, n: usize) {
    map.retain(|(idx, _), _| *idx < n);
    drop_on_change_from(map, |owner| owner >= n);
}

/// Files an `onChange` callback registered from `lua`: under `setting_of`, the module whose
/// setting it watches, and owned by the module whose VM `lua` is — for a code dependency's
/// code, the dependent's. The one place an entry is made, so the rule is tested here.
fn file_on_change(
    map: &mut OnChangeMap,
    lua: &Lua,
    setting_of: usize,
    key: String,
    cb: Function,
) -> mlua::Result<()> {
    // `populate_vm` tags every VM before any code runs in it, so the fallback is only the
    // path of a failed tag — the rule from before owners were recorded.
    let owner = image_search::vm_owner(lua).map_or(setting_of, |o| o.idx);
    let rk = lua.create_registry_value(cb)?;
    let id = NEXT_ON_CHANGE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    map.entry((setting_of, key)).or_default().push((owner, lua.clone(), rk, id));
    Ok(())
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
                    let refused = self.os_refused.borrow_mut().remove(&(idx, spec.clone()));
                    let before = match refused {
                        Some(n) => format!(" (the OS had refused it {n} time(s) before)"),
                        None => String::new(),
                    };
                    logging::line("keys", &format!("hotkey '{spec}' is now held by [{me}]{before}"));
                }
                // Held by another application rather than by us. Left not-live, so a later
                // refresh tries again — which is the one thing a restart used to be for. Said
                // the first time; a retry that is refused again is only counted (`os_refused`).
                Err(e) => {
                    let first = {
                        let mut refused = self.os_refused.borrow_mut();
                        let n = refused.entry((idx, spec.clone())).or_insert(0);
                        *n += 1;
                        *n == 1
                    };
                    if first {
                        self.report_os_conflict(idx, "hotkey", &spec, &e);
                    }
                }
            }
        }
        for (idx, owner, spec) in losers {
            self.report_conflict(idx, owner, "hotkey", &spec);
        }
    }

    /// Hands the hook the captures of *enabled* modules, in registration order, and their
    /// scopes and menu flags (so a disabled module's keys are no longer suppressed).
    fn refresh_captured(&self) {
        let set = self.captures.set(|i| self.module_on(i));
        self.note_captured_line(&set);
        self.backend.set_captured_keys(&set);
        captures::send_owners(self);
    }

    /// Hands the hook the scopes and menu flags of enabled modules, after one of them changed.
    fn refresh_key_owners(&self) {
        if appcfg::trace() {
            let set = self.captures.set(|i| self.module_on(i));
            self.note_captured_line(&set);
        }
        captures::send_owners(self);
    }

    /// Drops module `idx`'s key scope and menu flag: it captures everywhere again, with no menu.
    /// Beside dropping its captures, on a disable, a reload and a rolled-back hot-load — after
    /// the arbiter's re-election, whose `onDeactivate` in the module would otherwise write them
    /// again (`host.keys.scope(false)`, `menuOpen(false)`) into an entry nobody clears.
    fn forget_key_owner(&self, idx: usize) {
        if self.captures.forget_owner(idx) {
            self.refresh_key_owners();
        }
    }

    /// Whether module `idx` is enabled.
    fn module_on(&self, idx: usize) -> bool {
        self.enabled.borrow().get(idx).copied().unwrap_or(false)
    }

    /// The captured-set line, waiting for the end of the tick.
    ///
    /// Under TRACE. The trace switch promises in its own help text to record "every decision the
    /// key handling made", and this is the decision: which keys the application will not see. It
    /// once sat behind the calibration switch alone, which is about measuring coordinates inside
    /// an overlay — so the one line that answers "who is holding my arrow keys" was unavailable to
    /// the person asking, and an evening went into guessing at it instead. Calibration is the
    /// overlay runtime's own setting now, and the host's key lines are trace's alone.
    ///
    /// WHO holds each key, not just which, in the order that decides, with each module's scope
    /// and flag: a key is suppressed while an enabled module scoped to the window in front
    /// captures it, so the module names are the actionable half.
    ///
    /// Written at the end of the tick (`log_housekeeping`), and only when it changed: this runs
    /// once per key an overlay captures, so an activation of eight keys wrote eight growing
    /// lines, and over days of calibrating that was most of the log.
    fn note_captured_line(&self, set: &[backend::Captured]) {
        if appcfg::trace() {
            let ids = self.ids.borrow();
            let line = self
                .captures
                .captured_line(set, |i| ids.get(i).cloned().unwrap_or_else(|| "?".to_string()));
            self.captured_line.borrow_mut().0 = Some(line);
        }
    }

    /// Removes every registration owned by module `idx` (hotkeys + their OS
    /// registration, captured keys, settings `onChange`, timers, arbiter claims,
    /// image searches still in flight, gamepad listeners, data export) WITHOUT
    /// touching the parallel vectors — for an in-place reload that rebuilds the same
    /// index. Unlike `rollback_to` (a suffix truncation) this targets a single module
    /// and leaves its roots/ids/enabled/schemas slots.
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
        self.captures.drop_registrations(|i| i == idx);
        self.os_refused.borrow_mut().retain(|(i, _), _| *i != idx);
        // Its errors are news again in the log: a reload is how the maintainer tries a fix, and
        // the same error after it has to be written in full, not counted into a summary.
        self.forget_error_repeats(idx);
        // By the VM the callback was registered from, not by whose setting it watches — see
        // `drop_on_change_from`.
        purge_on_change(&mut self.on_change.borrow_mut(), idx);
        self.timers.retain(|i| i != idx);
        // The VM that asked is going; the one built in its place asks for itself.
        self.initial_pending.borrow_mut().retain(|(i, _)| *i != idx);
        self.purge_pending_images(idx);
        // Its reads, its handlers and what waits in its mailbox are dropped, not answered: they
        // belong to the VM that is going.
        self.ocr_drop_owner(idx, true);
        self.snap_drop_owner(idx);
        mailbox::drop_owner(self, idx, mailbox::Why::Reloaded);
        self.task_drop_owner(idx);
        self.forget_key_owner(idx);
        self.drop_pad_listeners(|i| i == idx);
        let mut to_resolve: Vec<String> = Vec::new();
        let mut deactivations: Vec<(Lua, Function)> = Vec::new();
        {
            let mut map = self.arbiter.borrow_mut();
            for (slot, s) in map.iter_mut() {
                // If this module owns the slot's ACTIVE claim, run its onDeactivate
                // before dropping it — so an active overlay tears down its side effects
                // exactly once, mirroring arbiter_unregister. (Its key scope and menu flag
                // are its own and go with it either way — `forget_key_owner` — but what
                // else it set would be stranded until the next activation.)
                if let Some(active) = s.active {
                    if let Some(c) =
                        s.claims.iter().find(|c| c.handle == active && c.module_idx == idx)
                    {
                        if let Ok(f) = c.lua.registry_value::<Function>(&c.on_deactivate) {
                            deactivations.push((c.lua.clone(), f));
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
        for (lua, f) in deactivations {
            match call_guarded(&lua, "arbiter onDeactivate", &f, ()) {
                Called::Ok => {}
                Called::Failed(e) => logging::line("arbiter", &format!("onDeactivate error during purge: {e}")),
                stop => self.log_stopped_call(idx, "onDeactivate", &stop),
            }
        }
        for slot in to_resolve {
            self.arbiter_resolve(&slot);
        }
        // A second time, after the onDeactivate above: it runs in the VM that is going, and what it
        // started — a timer, a read, a snapshot, a task that waits — would otherwise outlive the
        // purge and come back into the old VM, or keep it in memory. A timer it armed used to fire
        // into the old VM after a reload. Its key scope and menu flag likewise: the onDeactivate
        // sets them back (`host.keys.scope(false)`, `menuOpen(false)`), which writes them again.
        self.timers.retain(|i| i != idx);
        self.ocr_drop_owner(idx, true);
        self.snap_drop_owner(idx);
        mailbox::drop_owner(self, idx, mailbox::Why::Reloaded);
        self.task_drop_owner(idx);
        self.forget_key_owner(idx);
        // A mouse button the old VM held down: the new one does not know it holds it.
        self.release_held_buttons(idx, "it was reloaded");
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

    /// Queues module `idx` for a report of the window in front on the next tick — see
    /// `initial_pending` and [`queue_initial`].
    fn request_initial(&self, idx: usize, reprime: bool) {
        queue_initial(&mut self.initial_pending.borrow_mut(), idx, reprime);
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
        // Its VM takes calls again, whether or not it has a record: a module that was off when a
        // library whose code runs in it looped was stopped with the library, and has no record,
        // since it was off already. The manager's checkbox rebuilds a stopped VM before it gets here
        // (`toggle_module`); this is for one that was not.
        if enabled {
            vm_guard::clear_module(&self.guard, idx);
        }
        // A module the guard stopped, ticked again: its record goes, and the manager's row loses
        // its note.
        if enabled && self.stops.clear(idx) {
            let id = self.ids.borrow().get(idx).cloned().unwrap_or_default();
            logging::line("guard", &format!("[{id}] turned on again after its stop"));
            self.module_changes.borrow_mut().push(gui::ModuleChange { idx, enabled: true, stopped: None });
        }
        self.after_toggle(idx, enabled);
        true
    }

    /// What enabling or disabling module `idx` does once its flag is flipped: everything held at
    /// the OS recomputed, its slots re-elected, and — disabled — its reads, mailbox, handlers, key
    /// scope and menu flag dropped. The checkbox's, and a stop's second step (`stops::settle`),
    /// which says the inputs that waited in the module's mailbox and were dropped: their names.
    fn after_toggle(&self, idx: usize, enabled: bool) -> Vec<String> {
        // Fresh error-dialog slate on toggle: drop this module's dedup keys so a
        // still-present fault re-surfaces (rather than staying log-only) next run.
        if let Some(id) = self.ids.borrow().get(idx).cloned() {
            let prefix = format!("{id}\u{1}");
            self.error_seen.borrow_mut().retain(|k| !k.starts_with(&prefix));
        }
        // And the log's: its errors are news again, written in full.
        self.forget_error_repeats(idx);
        self.os_refused.borrow_mut().retain(|(i, _), _| *i != idx);
        // Derived, not toggled. This loop used to register or unregister only THIS module's
        // hotkeys, discarding the result (`let _ =`) — so enabling a module whose combination
        // another one held failed silently, and disabling the holder left the waiting module
        // waiting. Both are the same missing step: the set of live registrations is a function
        // of the enabled set, so it is recomputed rather than nudged.
        self.refresh_hotkeys();
        self.refresh_captured();
        self.refresh_gamepad();
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
        // A disabled module's text reads, snapshots and tasks are dropped, as its one-shot timers
        // are: a callback that clicked or spoke minutes later, over whatever is in front then, is
        // worse than none. Enabled again, its polls simply read afresh. After the re-election, so
        // that what an onDeactivate it ran started goes too — a read answered after a quick
        // re-enable would otherwise still be delivered, the generation being the same.
        let mut dropped = Vec::new();
        if !enabled {
            self.ocr_drop_owner(idx, false);
            self.snap_drop_owner(idx);
            // What waits in its mailbox, but a setting's `onChange`, which it still hears; and its
            // handler, which never goes on — not even after a quick enable.
            dropped = mailbox::drop_owner(self, idx, mailbox::Why::Disabled);
            self.task_drop_owner(idx);
            // And its key scope and menu flag, which used to stay set — one switch for the
            // whole application then, left wherever the module last put it.
            self.forget_key_owner(idx);
            // And a mouse button it holds down: nothing of it will let go of it now.
            self.release_held_buttons(idx, "it was disabled");
        }
        // Searches answered while it was off are asked again, so their callbacks still come.
        if enabled {
            self.resume_held_images(idx);
            // And a trigger that asked to hear about the window already in front hears about
            // it again: to a module that was off, whatever is in front now is new.
            self.request_initial(idx, true);
        }
        logging::line(
            "manager",
            &format!("module {idx} {}", if enabled { "enabled" } else { "disabled" }),
        );
        dropped
    }

    /// The line for an arbiter callback the guard did not let run, or that a stop ended.
    fn log_stopped_call(&self, idx: usize, which: &str, how: &Called) {
        let id = self.ids.borrow().get(idx).cloned().unwrap_or_else(|| "?".into());
        let what = match how {
            // Owed to nobody: turning the module on again builds it afresh (`toggle_module`).
            Called::Refused => "not run: the module was stopped by the host",
            _ => "ended by the host's stop of the module",
        };
        logging::line("arbiter", &format!("{which} of {id} {what}"));
    }

    /// Notes that module `idx` pressed `button` with `host.input.mouseDown` and has not let go.
    fn hold_button(&self, idx: usize, button: MouseButton) {
        let mut held = self.held_buttons.borrow_mut();
        let v = held.entry(idx).or_default();
        if !v.contains(&button) {
            v.push(button);
        }
    }

    /// Module `idx` let go of `button`: its `mouseUp`, or a `click` or `drag` of it, whose own
    /// release lets go of a button held down before it.
    fn let_go_button(&self, idx: usize, button: MouseButton) {
        let mut held = self.held_buttons.borrow_mut();
        if let Some(v) = held.get_mut(&idx) {
            v.retain(|b| *b != button);
            if v.is_empty() {
                held.remove(&idx);
            }
        }
    }

    /// The hotkey presses a stall's stop drops, but the application's own reload key: that one is
    /// how a user gets out of a stalled application, not a module's key, so it is answered — every
    /// module reloads at the end of this turn — and the log says so. The others, to be dropped.
    fn answer_reload_key(&self, ids: Vec<i32>) -> Vec<i32> {
        let reload = self.reload_hotkey_id.get();
        let (asked, others): (Vec<i32>, Vec<i32>) = ids.into_iter().partition(|id| *id == reload);
        if !asked.is_empty() {
            logging::line(
                "keys",
                &format!("{} was pressed while the application waited: it is answered after the stop, not dropped", reload_hotkey_shown()),
            );
            self.reload_all.set(true);
        }
        others
    }

    /// Lets go of every mouse button module `idx` holds down, where the pointer is now — so the
    /// pointer does not move — because the module stops running (`why`, for the log). The
    /// buttons' names. Runs no Lua.
    fn release_held_buttons(&self, idx: usize, why: &str) -> Vec<&'static str> {
        let buttons = self.held_buttons.borrow_mut().remove(&idx).unwrap_or_default();
        if buttons.is_empty() {
            return Vec::new();
        }
        let (x, y) = self.backend.cursor_pos();
        let id = self.ids.borrow().get(idx).cloned().unwrap_or_else(|| "?".into());
        let mut names = Vec::new();
        for b in buttons {
            let name = button_name(b);
            self.backend.mouse_up(x, y, b);
            logging::line(
                "input",
                &format!("[{id}] released the {name} mouse button it had pressed with host.input.mouseDown: {why}"),
            );
            names.push(name);
        }
        names
    }

    /// At exit: every mouse button a module still holds is let go of.
    fn release_all_held_buttons(&self) {
        let held: Vec<usize> = self.held_buttons.borrow().keys().copied().collect();
        for idx in held {
            self.release_held_buttons(idx, "the application ended");
        }
    }

    /// A stop's second step (stops.rs): the rest of disabling each stopped module, then the
    /// messages. Both ticks call it, after the report of the window in front — and again after a
    /// `host.window.recheck()` round, whose callbacks run after that and can stop a module too, so
    /// that a stop in it is settled at the end of the same turn as well.
    fn settle_stops(&self) {
        // Trips no outermost entry's end handed over — a panic unwound through it — are marked now.
        let waiting = vm_guard::take_waiting_trips(&self.guard);
        if !waiting.is_empty() {
            logging::line("guard", &format!("{} stop(s) handed over at the end of the turn", waiting.len()));
            if let Err(p) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| stops::mark(self, waiting))) {
                logging::line("guard", &format!("the stop's first step failed: {}", panic_text(&p)));
            }
        }
        stops::settle(self);
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
        self.captures.drop_registrations(|idx| idx >= n);
        self.os_refused.borrow_mut().retain(|(idx, _), _| *idx < n);
        // Before `ids` is cut below: the log's counts are keyed by the module's id.
        let rolled_back = self.ids.borrow().len();
        for idx in n..rolled_back {
            self.forget_error_repeats(idx);
        }
        rollback_on_change(&mut self.on_change.borrow_mut(), n);
        self.timers.retain(|idx| idx < n);
        self.initial_pending.borrow_mut().retain(|(idx, _)| *idx < n);
        self.ocr_drop_from(n);
        self.snap_drop_from(n);
        mailbox::drop_from(self, n);
        self.task_drop_from(n);
        self.drop_pad_listeners(|idx| idx >= n);
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
        // After the re-election, as on a disable: no claim of theirs is left to deactivate, but
        // nothing they set may outlive them either.
        self.captures.forget_owners_from(n);
        for idx in n..rolled_back {
            self.release_held_buttons(idx, "its load was rolled back");
        }
        self.stops.drop_from(n);
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
                    match call_guarded(&c.lua, "arbiter onDeactivate", &f, ()) {
                        Called::Ok => {}
                        Called::Failed(e) => logging::line("arbiter", &format!("onDeactivate error: {e}")),
                        stop => self.log_stopped_call(c.module_idx, "onDeactivate", &stop),
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
            let mut deactivate: Option<(Lua, usize, Function)> = None;
            let mut activate: Option<(Lua, usize, Function)> = None;
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
                        deactivate = c
                            .lua
                            .registry_value::<Function>(&c.on_deactivate)
                            .ok()
                            .map(|f| (c.lua.clone(), c.module_idx, f));
                    }
                }
                if let Some(neu) = winner {
                    if let Some(c) = s.claims.iter().find(|c| c.handle == neu) {
                        activate = c
                            .lua
                            .registry_value::<Function>(&c.on_activate)
                            .ok()
                            .map(|f| (c.lua.clone(), c.module_idx, f));
                    }
                }
                s.active = winner;
            }
            // A claim of a module the guard stopped loses its slot at once, and its onDeactivate is
            // not run: the stopped VM refuses it. Nothing is owed — turning the module on again
            // builds it afresh (`toggle_module`), with no overlay active — and the host takes back
            // what it holds for the module meanwhile (stops.rs).
            if let Some((lua, owner, f)) = deactivate {
                match call_guarded(&lua, "arbiter onDeactivate", &f, ()) {
                    Called::Ok => {}
                    Called::Failed(e) => logging::line("arbiter", &format!("onDeactivate error: {e}")),
                    stop => self.log_stopped_call(owner, "onDeactivate", &stop),
                }
            }
            if let Some((lua, owner, f)) = activate {
                match call_guarded(&lua, "arbiter onActivate", &f, ()) {
                    Called::Ok => {}
                    Called::Failed(e) => logging::line("arbiter", &format!("onActivate error: {e}")),
                    stop => self.log_stopped_call(owner, "onActivate", &stop),
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

    /// Mirrors the in-memory `enabled[]` flags into the store — as the user wants them: a module
    /// the guard stopped was on, and the next start loads it (`stops::stored_enabled`).
    fn sync_enabled_into_store(&self) {
        let ids = self.ids.borrow();
        let enabled = self.enabled.borrow();
        let mut store = self.store.borrow_mut();
        for (i, id) in ids.iter().enumerate() {
            store.set_enabled(id, stops::stored_enabled(enabled.get(i).copied().unwrap_or(true), self.stops.is_stopped(i)));
        }
    }

    /// Persists the store immediately (enable/disable is a deliberate action). The `[app]` table
    /// is the file's: the Application settings tab saves it itself (`save_keeping_app_on_disk`).
    fn save_config(&self) {
        self.sync_enabled_into_store();
        self.store.borrow_mut().save_keeping_app_on_disk();
    }

    /// Flushes pending setting changes to disk (coalesced; driven by the loop), with the file's
    /// `[app]` table, as `save_config`.
    fn flush_if_dirty(&self) {
        if !self.dirty.replace(false) {
            return;
        }
        self.sync_enabled_into_store();
        self.store.borrow_mut().save_keeping_app_on_disk();
    }

    /// Fires the timers whose time has come (driven by the loop tick) — see
    /// `timers::Timers::fire_due` for the order and for what a callback may do to the others.
    fn fire_due_timers(&self) {
        self.timers.fire_due(
            Instant::now(),
            |idx| match (self.module_on(idx), mailbox::busy(self, idx)) {
                (false, _) => timers::Slot::Off,
                (true, true) => timers::Slot::Busy,
                (true, false) => timers::Slot::Free,
            },
            // Only once a one-shot timer actually comes due — this runs on EVERY loop tick, and
            // an idle tick has changed nothing. Bumping there would make the epoch a tick
            // counter and defeat the memoization it exists for. A recurring tick does not bump
            // it either (see `host.epoch` in timer.md).
            || self.bump_epoch(),
            // Through each module's mailbox: at once while it is free (`Event::After`, `Every`).
            |due| match due {
                timers::Due::Once { token, idx, lua } => {
                    mailbox::deliver(self, idx, &lua, Event::After { token });
                }
                timers::Due::Every { token, idx, lua } => {
                    mailbox::deliver(self, idx, &lua, Event::Every { token });
                }
            },
        );
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
        // A line per change, as the Application settings tab writes for its own: otherwise the log
        // shows a module's setting only through what the module does next. OK applies every
        // field, so one left as it was writes nothing.
        if old.as_ref() != Some(&value) {
            logging::line("settings", &format!("{id} {key} is now {value}"));
        }
        // From the dialog every `onChange` is queued: it runs in the next tick's queued phase,
        // never inside the dialog's own event.
        self.fire_on_change(idx, key, &value, old.as_ref(), None);
    }

    /// Fires the `onChange` callbacks registered for (module, key). Those registered from the VM
    /// of `caller` — the module whose `host.settings.set` changed it — run at once, inside `set`,
    /// as a plain call: the module is running, and nothing of it waits. Every other one goes to its
    /// module's mailbox (`mailbox::deliver_later`) and runs as a handler in the queued phase of
    /// this tick or the next — after `set` has returned; no handler starts inside another. A
    /// disabled module's run too, as they always did.
    fn fire_on_change(
        &self,
        idx: usize,
        key: &str,
        new: &settings::Value,
        old: Option<&settings::Value>,
        caller: Option<usize>,
    ) {
        let regs: Vec<(usize, Lua, u64)> = {
            let map = self.on_change.borrow();
            map.get(&(idx, key.to_string()))
                .map(|list| list.iter().map(|(owner, lua, _, reg)| (*owner, lua.clone(), *reg)).collect())
                .unwrap_or_default()
        };
        for (owner, lua, reg) in regs {
            if Some(owner) == caller {
                let Opened::Run { f, args, ctx } = self.open_setting(idx, reg, key, new, old) else { continue };
                // A stop reports itself (stops.rs).
                if let Called::Failed(e) = call_guarded(&lua, ctx.what.clone(), &f, args) {
                    self.report_callback_error(ctx.report, &ctx.what, &e);
                }
            } else {
                let ev = Event::Setting { setting_of: idx, reg, key: key.to_string(), new: new.clone(), old: old.cloned() };
                mailbox::deliver_later(self, owner, &lua, ev);
            }
        }
    }

    /// Registration `reg` of an `onChange` for module `setting_of`'s `key`, as it runs
    /// ([`open_on_change`]).
    fn open_setting(
        &self,
        setting_of: usize,
        reg: u64,
        key: &str,
        new: &settings::Value,
        old: Option<&settings::Value>,
    ) -> Opened {
        open_on_change(&self.on_change, setting_of, reg, key, new, old)
    }
}

/// The host's side of a stop (stops.rs).
impl stops::StopHost for Shared {
    fn stops(&self) -> &stops::Stops {
        &self.stops
    }

    fn module_enabled(&self, idx: usize) -> bool {
        self.module_on(idx)
    }

    fn flip_off(&self, idx: usize) -> bool {
        match self.enabled.try_borrow_mut() {
            Ok(mut en) => {
                if let Some(e) = en.get_mut(idx) {
                    *e = false;
                }
                true
            }
            Err(_) => false,
        }
    }

    fn refresh_keys(&self) {
        self.refresh_hotkeys();
        self.refresh_captured();
        self.refresh_gamepad();
    }

    fn release_buttons(&self, idx: usize) -> Vec<&'static str> {
        self.release_held_buttons(idx, "it was stopped by the host")
    }

    fn drop_queued_input(&self) -> stops::Dropped {
        let d = self.backend.drop_queued_input();
        let dropped = self.answer_reload_key(d.hotkeys);
        let hotkeys = self.hotkeys.borrow();
        let mut names: Vec<String> = d
            .keys
            .iter()
            .map(|(vk, mask, _)| backend::spec_name_for(backend::KeyOs::CURRENT, *vk, *mask))
            .collect();
        names.extend(dropped.iter().map(|id| hotkeys.get(id).map_or_else(|| format!("hotkey {id}"), |r| r.spec.clone())));
        stops::Dropped { keys: d.keys.len(), hotkeys: dropped.len(), names: stops::counted(&names) }
    }

    fn after_disable(&self, idx: usize) -> Vec<String> {
        self.after_toggle(idx, false)
    }

    fn collect_garbage(&self, serial: u64) {
        vm_guard::collect_garbage(serial);
    }

    fn log(&self, category: &str, line: &str) {
        logging::line(category, line);
    }

    fn dialog(&self, key: String, title: String, text: String) {
        self.queue_dialog(key, title, text);
    }

    fn announce(&self, text: &str) {
        Shared::announce(self, text);
    }

    fn row_changed(&self, idx: usize, note: Option<String>) {
        let enabled = note.is_none();
        self.module_changes.borrow_mut().push(gui::ModuleChange { idx, enabled, stopped: note });
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
    out: &mut Vec<capture_source::CodeDep>,
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

/// Whether a module loads on this platform: its `supported_os` claim, unless the override that
/// loads every module anyway is set. The same rule the loader applies to the module itself.
fn runs_here(manifest: &module_manifest::ModuleManifest) -> bool {
    manifest.runs_on(std::env::consts::OS) || appcfg::ignore_supported_os()
}

fn collect_one_code_dep(
    parent: &Path,
    spec: &str,
    optional: bool,
    out: &mut Vec<capture_source::CodeDep>,
    seen: &mut HashSet<String>,
) -> Result<()> {
    let dep_id = module_manifest::dep_id(spec);
    if !seen.insert(dep_id.to_string()) {
        return Ok(()); // already visited
    }
    // The pre-order position, in manifest order: the tie-break of the capture resolution.
    let order = seen.len() as u32;
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
    // An optional dependency that does not run on this platform is as absent as one that is
    // not installed. Its module is skipped at load for its `supported_os`, so it never enters
    // the module table, and evaluating its code here failed the whole dependent with "not in
    // the module table" — which is how declaring Komplete Kontrol Windows-only took Kontakt and
    // every Kontakt library down on macOS (the Mac CI's module smoke run, 2026-09-22). The
    // dependent's `host.tryRequire` answers nil for it, as for a missing one. A REQUIRED
    // dependency that does not run here stays an error: the dependent cannot work without it.
    if optional && !runs_here(&lm.manifest) {
        return Ok(());
    }
    // Its own code-module deps first (required + optional), so they're registered before it.
    collect_code_deps(parent, &lm.manifest.dependencies, &lm.manifest.optional_dependencies, out, seen)?;
    let entry = lm.entry_path();
    let deps: Vec<String> =
        lm.manifest.dependencies.iter().chain(&lm.manifest.optional_dependencies).cloned().collect();
    out.push(capture_source::CodeDep {
        id: dep_id.to_string(),
        name: lm.manifest.name.clone(),
        memory_mib: lm.manifest.memory_mib(),
        memory_declared: lm.manifest.limits.memory_mib.is_some(),
        entry_rel: lm.manifest.entry.clone(),
        entry,
        reads_screen: capture_source::reads_screen(&lm.manifest.capabilities.require),
        screen: lm.manifest.screen,
        deps,
        order,
        depth: 0, // settled by capture_source::apply
    });
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
fn eval_on_host(lua: &Lua, host: &Table, code: &str, name: &str) -> mlua::Result<mlua::Value> {
    let factory: Function =
        lua.load(format!("return function(host) {code}\nend")).set_name(name).eval()?;
    factory.call::<mlua::Value>(host.clone())
}

/// A Luau file the host is about to compile, as text, without a leading UTF-8 byte-order mark.
///
/// Luau's lexer has no notion of one: `U+FEFF` in front of the first token is an error
/// ("Unicode character U+feff"), and that is what a file written by .NET's `Encoding.UTF8`,
/// Notepad before Windows 10 1903 or PowerShell 5's `Out-File -Encoding utf8` starts with. A
/// module generated by such a tool would not load at all, and the message points at nothing
/// visible. The TOML parser and `host.json.decode` already skip one; this does the same for
/// every Luau source the host loads — a module's entry file, a code dependency's entry file
/// and every `host.include` — and only for that. One mark, at the very start: a second, or one
/// further in, is the file's own content and stays for Luau to judge.
fn read_luau_source(path: &Path) -> std::io::Result<String> {
    let mut text = std::fs::read_to_string(path)?;
    if text.starts_with('\u{feff}') {
        text.drain(..'\u{feff}'.len_utf8());
    }
    Ok(text)
}

/// An optional boolean field of an options table, read strictly: absent (no table, or the key
/// nil) is `default`, `true` and `false` are themselves, and anything else raises, naming the
/// call and the type. `call` is how the error names the function.
///
/// Not mlua's `get::<bool>`, which is Lua truthiness: nil comes back `false` — so a default
/// written as `.unwrap_or(true)` never applied to `{}` — and every other value, `0` and `"no"`
/// included, comes back `true` without a word.
fn opt_bool(opts: Option<&Table>, key: &str, default: bool, call: &str) -> mlua::Result<bool> {
    let Some(t) = opts else { return Ok(default) };
    match t.get::<mlua::Value>(key)? {
        mlua::Value::Nil => Ok(default),
        mlua::Value::Boolean(b) => Ok(b),
        other => Err(mlua::Error::external(format!(
            "{call}: {key} is true or false, not a {}",
            json::luau_type(&other)
        ))),
    }
}

/// `opts.interrupt` of `host.speech.output`: `true` unless the caller says `false`.
fn speech_interrupt(opts: Option<&Table>) -> mlua::Result<bool> {
    opt_bool(opts, "interrupt", true, "host.speech.output")
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
    // The free members of a namespace the dependency did not declare, from its OWN table: they
    // carry no identity, so whose copy answers does not matter, and the owner's is exactly what
    // must not be handed over.
    let free = free_subsets(
        lua,
        &dep_full,
        |ns| capability_for(ns).is_some_and(|cap| !dep_caps.contains(cap)),
        &dep_id,
        FOREIGN_VM,
    )?;
    let owner = host_owner.clone();
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |_, (_t, key): (Table, String)| -> mlua::Result<mlua::Value> {
            dep_host_member(&key, &dep_caps, &free, &owner, &dep_id)
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

/// What a dependency's refusal adds to the ordinary one.
const FOREIGN_VM: &str = ". Its code runs inside another module's VM; what that module \
                          declares does not apply to it";

/// What `host.<key>` answers for a dependency's code running in another module's VM: the
/// owner's table for a namespace the dependency declared (or one that needs nothing), the
/// dependency's own free members of one it did not declare, and the refusal otherwise.
///
/// Out of `build_dep_host` so the rule can be tested without a whole `Shared`.
fn dep_host_member(
    key: &str,
    dep_caps: &HashSet<String>,
    free: &HashMap<String, Table>,
    owner: &Table,
    dep_id: &str,
) -> mlua::Result<mlua::Value> {
    if let Some(cap) = capability_for(key) {
        if !dep_caps.contains(cap) {
            if let Some(subset) = free.get(key) {
                return Ok(mlua::Value::Table(subset.clone()));
            }
            return Err(undeclared(dep_id, key, FOREIGN_VM));
        }
    }
    owner.raw_get(key)
}

/// What a plain call of a module's callback came to.
#[derive(Debug, PartialEq, Eq)]
enum Called {
    Ok,
    /// It raised (or a Rust panic under it was caught): the human-readable error.
    Failed(String),
    /// Not run: the VM was stopped before it.
    Refused,
    /// The VM was stopped while it ran — by it, or by a call inside it: the stop reports itself
    /// (stops.rs), and its error is not the module's.
    Stopped,
}

/// Calls a module's callback `f` in its VM `lua` as a plain call — the synchronous places: an
/// arbiter's `onActivate` and `onDeactivate`, a module's own `onChange` inside its own `set` — as
/// an entry of the guard (`vm_guard::Entry`, named `what` in a stop's messages), catching BOTH a
/// Lua error and a Rust panic (re-raised across the mlua boundary), so a faulting module's
/// callback can never abort the process.
fn call_guarded<A: mlua::IntoLuaMulti>(
    lua: &Lua,
    what: impl Into<std::borrow::Cow<'static, str>>,
    f: &Function,
    args: A,
) -> Called {
    let Ok(entry) = vm_guard::Entry::enter(lua, vm_guard::EntryKind::Plain, what) else {
        return Called::Refused;
    };
    let result = guard(|| {
        let r = f.call::<()>(args);
        entry.finish(&r);
        r
    });
    if entry.stopped() {
        return Called::Stopped;
    }
    match result {
        Ok(()) => Called::Ok,
        Err(e) => Called::Failed(e),
    }
}

/// A callback called the old way, outside any entry: for the tests that run a timer's or a
/// snapshot's callback without a mailbox.
#[cfg(test)]
pub(crate) fn call_plain<A: mlua::IntoLuaMulti>(f: &Function, args: A) -> Result<(), String> {
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

/// The start of a panic message a test raises on purpose; see `quiet_expected_panics`.
#[cfg(test)]
pub(crate) const EXPECTED_PANIC: &str = "expected by a test: ";

/// Keeps the panics tests raise on purpose out of the test output.
///
/// Installed ONCE for the whole test binary and never taken down. The tests used to swap the
/// process-wide hook for a silent one and put the old one back afterwards, and tests run in
/// parallel: one test's silent hook swallowed another's failure message, and a restore could
/// put back a hook that was itself another test's silent one. A hook nobody swaps cannot
/// race. It is silent for exactly two kinds of panic — one inside `logging::contain`, as the
/// application's own hook is, and one whose message starts with `EXPECTED_PANIC` — and hands
/// every other to the default hook.
#[cfg(test)]
pub(crate) fn quiet_expected_panics() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let p = info.payload();
            let msg = p
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| p.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("");
            if msg.starts_with(EXPECTED_PANIC) || logging::hold_contained_panic(|| info.to_string()) {
                return;
            }
            default(info);
        }));
    });
}

#[cfg(test)]
mod hotkey_conflict_tests {
    use super::*;

    /// Where a press of a hotkey goes when it runs — at once, or after it waited in its busy
    /// module's mailbox: to its registration while that is there, holds the combination and the
    /// module is on; else to the module's own new registration of the same combination, while the
    /// module's key scope is everywhere or the window the press came in; never to another module's.
    #[test]
    fn a_hotkey_goes_to_its_registration_or_its_modules_new_one_and_never_to_another_module() {
        use captures::{Gone, Target};
        let lua = Lua::new();
        let reg = |module_idx: usize, binding: Option<(u32, u8)>, live: bool| HotkeyReg {
            module_idx,
            lua: lua.clone(),
            cb: lua.create_registry_value(lua.create_function(|_, ()| Ok(())).unwrap()).unwrap(),
            spec: "Alt+V".to_string(),
            binding,
            live,
        };
        const V: (u32, u8) = (0x56, 0b010);
        const W1: isize = 0x111;
        let on = |_: usize| true;
        let mut regs: HashMap<i32, HotkeyReg> = HashMap::new();
        regs.insert(5, reg(1, Some(V), true));
        assert_eq!(hotkey_target(&regs, on, 0, 1, 5, Some(V), None), Target::Same(5));
        assert_eq!(hotkey_target(&regs, |_| false, 0, 1, 5, Some(V), None), Target::Gone(Gone::Disabled));
        // The overlay went and came back while the press waited: id 5 released, 9 made.
        regs.remove(&5);
        regs.insert(9, reg(1, Some(V), true));
        assert_eq!(hotkey_target(&regs, on, 0, 1, 5, Some(V), None), Target::Moved(9), "global scope");
        assert_eq!(hotkey_target(&regs, on, W1, 1, 5, Some(V), Some(W1)), Target::Moved(9), "pressed in its window");
        assert_eq!(hotkey_target(&regs, on, W1, 1, 5, Some(V), Some(0x222)), Target::Gone(Gone::ScopeMoved));
        assert_eq!(hotkey_target(&regs, on, W1, 1, 5, Some(V), None), Target::Gone(Gone::ScopeMoved), "front not known");
        // Another module holds the combination now: never to it.
        regs.remove(&9);
        regs.insert(12, reg(2, Some(V), true));
        regs.insert(13, reg(1, Some(V), false));
        assert_eq!(hotkey_target(&regs, on, 0, 1, 5, Some(V), None), Target::Gone(Gone::TakenOver));
        // Nobody: released. And a spec only the OS read cannot be matched again.
        regs.clear();
        assert_eq!(hotkey_target(&regs, on, 0, 1, 5, Some(V), None), Target::Gone(Gone::Released));
        assert_eq!(hotkey_target(&regs, on, 0, 1, 5, None, None), Target::Gone(Gone::Released));
        // Its own id, still registered but no longer holding the combination, is not run either.
        regs.insert(5, reg(1, Some(V), false));
        regs.insert(14, reg(2, Some(V), true));
        assert_eq!(hotkey_target(&regs, on, 0, 1, 5, Some(V), None), Target::Gone(Gone::TakenOver));
    }

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
        // Through the runtime's own function, so the host's registry handle is in place too.
        install_window_prelude(&lua, &full).unwrap();

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

    /// A whole host table as `populate_vm` has it once the prelude has run, and the module's
    /// view of it for `caps`, installed as the global `host` the way `populate_vm` leaves it.
    fn vm_with_caps(lua: &Lua, caps: &[&str], id: &str) -> Table {
        let full = lua.create_table().unwrap();
        for (key, _) in GATED {
            full.set(*key, lua.create_table().unwrap()).unwrap();
        }
        let os = lua.create_table().unwrap();
        os.set("current", "windows").unwrap();
        full.set("os", os).unwrap();
        // The dispatch times itself with `host.now`; a clock standing still keeps it quiet.
        full.set("now", lua.create_function(|_, ()| Ok(0)).unwrap()).unwrap();
        install_window_prelude(lua, &full).unwrap();
        let caps: HashSet<String> = caps.iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(lua, &full, &caps, id).unwrap();
        lua.globals().set("host", &view).unwrap();
        full
    }

    fn a_window(title: &str) -> WinInfo {
        WinInfo {
            hwnd: 7,
            title: title.to_string(),
            class: "GameWindow".to_string(),
            pid: 42,
            exe: "game.exe".to_string(),
            bundle_id: String::new(),
            x: 0,
            y: 0,
            w: 800,
            h: 600,
            client_x: 0,
            client_y: 0,
            client_w: 800,
            client_h: 600,
        }
    }

    /// Window events reach a module that never declared `window`, and the gate still holds
    /// against the module's own code.
    ///
    /// The shape is the overlay module that relies on the overlay runtime's manifest: its
    /// triggers are registered by the RUNTIME's code, through the runtime's host, which does
    /// declare `window`. Delivery used to look the prelude up through the module's gated view
    /// and raised instead — every foreground and focus change, for every such module — so the
    /// overlay never activated, and `window_has_triggers` said there was nothing to watch for.
    #[test]
    fn window_events_reach_a_module_that_did_not_declare_window() {
        let lua = Lua::new();
        let full = vm_with_caps(&lua, &["speech"], "com.example.overlay-user");

        // Registered the way a code dependency's code does it: against the whole table.
        lua.load(
            r#"
            local host = ...
            fired, focused = nil, 0
            host.window.onTrigger({ title = { contains = "Game" } }, nil, function(win)
                fired = win.title
            end)
            host.window.onFocus(function() focused = focused + 1 end)
            "#,
        )
        .call::<()>(&full)
        .unwrap();

        assert!(window_has_triggers(&lua), "the triggers the runtime registered are seen");
        dispatch_activate(&lua, &a_window("Desktop")).unwrap();
        assert_eq!(lua.globals().get::<Option<String>>("fired").unwrap(), None);
        dispatch_activate(&lua, &a_window("The Game")).unwrap();
        assert_eq!(lua.globals().get::<String>("fired").unwrap(), "The Game");
        dispatch_focus(&lua).unwrap();
        assert_eq!(lua.globals().get::<i64>("focused").unwrap(), 1);

        // The module's own code is still held to its manifest.
        let err = lua.load("return host.window").eval::<mlua::Value>().unwrap_err().to_string();
        assert!(err.contains("without declaring it"), "{err}");
        // And the handle the host dispatches through is not reachable from Luau: the named
        // registry is only open to Luau through `debug.getregistry`, which Luau does not have.
        // If that ever changed, the handle would be a way around the gate.
        let open: bool = lua
            .load("return type(debug) == 'table' and debug.getregistry ~= nil")
            .eval()
            .unwrap();
        assert!(!open, "Luau now exposes the registry; the window handle is reachable");
    }

    /// A module that registered nothing — a plain hotkey module, without `window` — is left
    /// alone by the delivery: no error, and nothing converted for it.
    #[test]
    fn window_events_skip_a_module_with_no_triggers() {
        let lua = Lua::new();
        vm_with_caps(&lua, &["hotkey"], "com.example.hotkeys-only");
        assert!(!window_has_triggers(&lua));
        dispatch_activate(&lua, &a_window("Anything")).unwrap();
        dispatch_focus(&lua).unwrap();
    }

    /// A module that did declare `window` and registers its own triggers is delivered to as
    /// before — through the same table its code wrote to.
    #[test]
    fn window_events_reach_a_module_that_declared_window() {
        let lua = Lua::new();
        vm_with_caps(&lua, &["window"], "com.example.watcher");
        lua.load(
            r#"
            seen = 0
            host.window.onTrigger({ title = "Editor" }, nil, function() seen = seen + 1 end)
            "#,
        )
        .exec()
        .unwrap();
        assert!(window_has_triggers(&lua));
        dispatch_activate(&lua, &a_window("editor")).unwrap();
        assert_eq!(lua.globals().get::<i64>("seen").unwrap(), 1);
    }

    /// A VM the prelude never ran in — the empty one a failed reload leaves in the module's
    /// place — is nothing to deliver to: no triggers, and no error on every window switch.
    #[test]
    fn window_events_skip_a_vm_without_the_prelude() {
        let lua = Lua::new();
        assert!(!window_has_triggers(&lua));
        dispatch_activate(&lua, &a_window("Anything")).unwrap();
        dispatch_focus(&lua).unwrap();
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

    /// A module that only registers a hotkey can put its key into words, and still cannot
    /// capture one: `host.keys.normalize`, `describe` and `check` are free, the rest of
    /// `host.keys` is not. Through the module's own view; the dependency's view is the next
    /// test.
    #[test]
    fn describing_a_key_needs_no_capability_and_capturing_one_still_does() {
        let lua = Lua::new();
        let full = lua.create_table().unwrap();
        let keys = lua.create_table().unwrap();
        for name in ["normalize", "describe", "check", "capture"] {
            let n = name.to_string();
            keys.set(name, lua.create_function(move |_, ()| Ok(n.clone())).unwrap()).unwrap();
        }
        full.set("keys", keys).unwrap();
        let caps: HashSet<String> = ["hotkey"].iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(&lua, &full, &caps, "com.example.hotkey-only").unwrap();
        lua.globals().set("host", view).unwrap();

        for name in ["normalize", "describe", "check"] {
            let got: String = lua.load(format!("return host.keys.{name}()")).eval().unwrap();
            assert_eq!(got, name);
        }
        let err = lua.load("return host.keys.capture").eval::<mlua::Value>().unwrap_err().to_string();
        assert!(err.contains("com.example.hotkey-only") && err.contains("\"keys\""), "{err}");
        // Not a way round the gate either: the subset carries the three and nothing else.
        let leaked: bool = lua
            .load("for k in pairs(host.keys) do if k == 'capture' then return true end end return false")
            .eval()
            .unwrap();
        assert!(!leaked);

        // Declared, the namespace is the whole table as before.
        let all: HashSet<String> = ["keys"].iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(&lua, &full, &all, "com.example.keys").unwrap();
        lua.globals().set("host", view).unwrap();
        let got: String = lua.load("return host.keys.capture()").eval().unwrap();
        assert_eq!(got, "capture");
    }

    /// The same for a dependency's code in another module's VM: its free members come from its
    /// OWN table — never the owner's, which is exactly what must not be handed over — and a
    /// namespace it declared still resolves to the owner's binding.
    #[test]
    fn a_dependency_gets_its_own_free_members_and_not_the_owners_namespace() {
        let lua = Lua::new();
        let table_of = |tag: &str, names: &[&str]| {
            let t = lua.create_table().unwrap();
            for name in names {
                let v = format!("{tag}.{name}");
                t.set(*name, lua.create_function(move |_, ()| Ok(v.clone())).unwrap()).unwrap();
            }
            t
        };
        let dep_full = lua.create_table().unwrap();
        dep_full.set("keys", table_of("dep", &["normalize", "describe", "check", "capture"])).unwrap();
        let owner = lua.create_table().unwrap();
        owner.set("keys", table_of("owner", &["normalize", "capture"])).unwrap();
        owner.set("hotkey", table_of("owner", &["register"])).unwrap();
        let caps: HashSet<String> = ["hotkey"].iter().map(|s| (*s).to_string()).collect();
        let free = free_subsets(
            &lua,
            &dep_full,
            |ns| capability_for(ns).is_some_and(|cap| !caps.contains(cap)),
            "com.example.dep",
            FOREIGN_VM,
        )
        .unwrap();
        let member = |key: &str| dep_host_member(key, &caps, &free, &owner, "com.example.dep");

        let Ok(mlua::Value::Table(keys)) = member("keys") else { panic!("keys was refused") };
        let got: String = keys.get::<Function>("normalize").unwrap().call(()).unwrap();
        assert_eq!(got, "dep.normalize", "the free members are the dependency's own");
        let err = keys.get::<mlua::Value>("capture").unwrap_err().to_string();
        assert!(err.contains("com.example.dep") && err.contains("another module's VM"), "{err}");

        let Ok(mlua::Value::Table(hk)) = member("hotkey") else { panic!("hotkey was refused") };
        let got: String = hk.get::<Function>("register").unwrap().call(()).unwrap();
        assert_eq!(got, "owner.register", "a declared namespace is the owner's binding");

        let err = member("speech").unwrap_err().to_string();
        assert!(err.contains("\"speech\""), "{err}");
    }
}

/// `onTrigger { initial = true }` against the real prelude and a stand-in host: the prelude's
/// priming, `dispatch_initial`, and the queue the host drains on the tick.
#[cfg(test)]
mod initial_trigger_tests {
    use super::*;

    /// A VM whose whole host table has the prelude installed and a `window._requestInitial`
    /// that counts, the way `populate_vm` leaves it; the module's code sees `caps` only.
    fn vm(caps: &[&str]) -> (Lua, Table) {
        let lua = Lua::new();
        let full = lua.create_table().unwrap();
        for (key, _) in GATED {
            full.set(*key, lua.create_table().unwrap()).unwrap();
        }
        let os = lua.create_table().unwrap();
        os.set("current", "windows").unwrap();
        full.set("os", os).unwrap();
        full.set("now", lua.create_function(|_, ()| Ok(0)).unwrap()).unwrap();
        lua.globals().set("requests", 0).unwrap();
        let window: Table = full.get("window").unwrap();
        window
            .set(
                "_requestInitial",
                lua.create_function(|lua, ()| {
                    let n: i64 = lua.globals().get("requests")?;
                    lua.globals().set("requests", n + 1)
                })
                .unwrap(),
            )
            .unwrap();
        install_window_prelude(&lua, &full).unwrap();
        let caps: HashSet<String> = caps.iter().map(|s| (*s).to_string()).collect();
        let view = gated_view(&lua, &full, &caps, "com.example.game").unwrap();
        lua.globals().set("host", &view).unwrap();
        (lua, full)
    }

    fn game() -> WinInfo {
        window_titled("Arcade Game")
    }

    fn window_titled(title: &str) -> WinInfo {
        WinInfo {
            hwnd: 9,
            title: title.to_string(),
            class: "GameWindow".to_string(),
            pid: 42,
            exe: "game.exe".to_string(),
            bundle_id: String::new(),
            x: 0,
            y: 0,
            w: 1280,
            h: 1024,
            client_x: 0,
            client_y: 0,
            client_w: 1280,
            client_h: 1024,
        }
    }

    /// Registers a counting trigger on the WHOLE table (as a runtime's code does) for the game.
    fn register(lua: &Lua, full: &Table, name: &str, opts: &str) {
        lua.load(format!(
            r#"
            local host = ...
            {name} = 0
            host.window.onTrigger({{ title = {{ contains = "Arcade" }} }}, {opts}, function(win)
              {name} = {name} + 1
              {name}_title = win.title
            end)
            "#
        ))
        .call::<()>(full)
        .unwrap();
    }

    fn count(lua: &Lua, name: &str) -> i64 {
        lua.globals().get::<i64>(name).unwrap()
    }

    /// Dispatches the report and returns (fired, how often `before_first` ran).
    fn report(lua: &Lua, win: Option<&WinInfo>, reprime: bool) -> (i64, u32) {
        let before = Rc::new(Cell::new(0u32));
        let b = before.clone();
        let hook = lua.create_function(move |_, ()| {
            b.set(b.get() + 1);
            Ok(())
        })
        .unwrap();
        let fired = dispatch_initial(lua, win, reprime, Some(hook)).unwrap();
        (fired, before.get())
    }

    #[test]
    fn fires_once_after_load_for_the_window_in_front() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "hits", "{ initial = true }");
        assert_eq!(count(&lua, "requests"), 1, "the registration asked the host for a report");
        assert_eq!(count(&lua, "hits"), 0, "not called from inside onTrigger");
        assert_eq!(report(&lua, Some(&game()), false), (1, 1));
        assert_eq!(count(&lua, "hits"), 1);
        assert_eq!(lua.globals().get::<String>("hits_title").unwrap(), "Arcade Game");
        // Once: the next tick has nothing left to report.
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        assert_eq!(count(&lua, "hits"), 1);
    }

    #[test]
    fn a_registration_after_load_is_reported_on_the_next_tick() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "early", "{ initial = true }");
        assert_eq!(report(&lua, Some(&game()), false).0, 1);
        // From a timer, say: a second trigger, registered once the first was reported.
        register(&lua, &full, "late", "{ initial = true }");
        assert_eq!(count(&lua, "requests"), 2);
        assert_eq!(report(&lua, Some(&game()), false), (1, 1));
        assert_eq!((count(&lua, "early"), count(&lua, "late")), (1, 1));
    }

    #[test]
    fn nothing_fires_without_a_match_or_a_window_and_the_report_is_used_up() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "hits", "{ initial = true }");
        assert_eq!(report(&lua, Some(&window_titled("Desktop")), false), (0, 0));
        // That WAS the report: the game coming forward later is an activation, not this.
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        register(&lua, &full, "second", "{ initial = true }");
        assert_eq!(report(&lua, None, false), (0, 0), "no window in front: nothing to report");
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        assert_eq!((count(&lua, "hits"), count(&lua, "second")), (0, 0));
    }

    #[test]
    fn a_trigger_without_initial_is_never_reported() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "plain", "nil");
        register(&lua, &full, "activateOnly", "{ on = \"activate\" }");
        assert_eq!(count(&lua, "requests"), 0, "nothing asked for a report");
        assert_eq!(report(&lua, Some(&game()), true), (0, 0), "not even on re-enable");
        assert_eq!((count(&lua, "plain"), count(&lua, "activateOnly")), (0, 0));
    }

    #[test]
    fn fires_again_on_re_enable() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "hits", "{ initial = true }");
        assert_eq!(report(&lua, Some(&game()), false).0, 1);
        assert_eq!(report(&lua, Some(&game()), true), (1, 1), "enabled again: reported again");
        assert_eq!(count(&lua, "hits"), 2);
    }

    #[test]
    fn an_earlier_activation_prevents_a_double_call() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "hits", "{ initial = true }");
        // The game comes forward before the tick: the activation reports it...
        dispatch_activate(&lua, &game()).unwrap();
        assert_eq!(count(&lua, "hits"), 1);
        // ...and the report the registration asked for finds nothing left to say.
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        assert_eq!(count(&lua, "hits"), 1);

        // An activation of some OTHER window also uses the report up: the game is not in
        // front, and when it comes forward that is an activation of its own.
        register(&lua, &full, "other", "{ initial = true }");
        dispatch_activate(&lua, &window_titled("Desktop")).unwrap();
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        assert_eq!(count(&lua, "other"), 0);
    }

    /// A trigger registered by a callback of an activation is reported that activation by the
    /// dispatch itself, so the report it asked for must not repeat it.
    #[test]
    fn a_trigger_registered_during_an_activation_is_not_reported_twice() {
        let (lua, full) = vm(&["window"]);
        lua.load(
            r#"
            local host = ...
            inner = 0
            host.window.onTrigger({ title = { contains = "Arcade" } }, nil, function()
              if registered then return end
              registered = true
              host.window.onTrigger({ title = { contains = "Arcade" } }, { initial = true },
                function() inner = inner + 1 end)
            end)
            "#,
        )
        .call::<()>(&full)
        .unwrap();
        dispatch_activate(&lua, &game()).unwrap();
        assert_eq!(count(&lua, "inner"), 1);
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        assert_eq!(count(&lua, "inner"), 1);
    }

    #[test]
    fn every_matching_trigger_runs_and_the_preparation_runs_once() {
        let (lua, full) = vm(&["window"]);
        register(&lua, &full, "a", "{ initial = true }");
        register(&lua, &full, "b", "{ initial = true }");
        lua.load(
            r#"
            local host = ...
            host.window.onTrigger({ title = "Somewhere else" }, { initial = true }, function() end)
            "#,
        )
        .call::<()>(&full)
        .unwrap();
        assert_eq!(report(&lua, Some(&game()), false), (2, 1), "fired counts the callbacks that ran");
    }

    /// One raising callback stops the report for that module, and the rest are not left
    /// primed for a later window — the rule an activation follows.
    #[test]
    fn a_raising_callback_uses_the_report_up_for_the_rest() {
        let (lua, full) = vm(&["window"]);
        lua.load(
            r#"
            local host = ...
            host.window.onTrigger({ title = { contains = "Arcade" } }, { initial = true },
              function() error("boom") end)
            "#,
        )
        .call::<()>(&full)
        .unwrap();
        register(&lua, &full, "after", "{ initial = true }");
        let e = dispatch_initial(&lua, Some(&game()), false, None).unwrap_err().to_string();
        assert!(e.contains("boom"), "{e}");
        assert_eq!(report(&lua, Some(&game()), false), (0, 0));
        assert_eq!(count(&lua, "after"), 0);
    }

    /// Reported into a module that never declared `window` — its runtime registered the
    /// trigger — through the host's own handle, as activations are.
    #[test]
    fn reaches_a_module_that_did_not_declare_window() {
        let (lua, full) = vm(&["speech"]);
        register(&lua, &full, "hits", "{ initial = true }");
        assert_eq!(report(&lua, Some(&game()), false), (1, 1));
        let err = lua.load("return host.window").eval::<mlua::Value>().unwrap_err().to_string();
        assert!(err.contains("without declaring it"), "the module's own code is still gated: {err}");
    }

    #[test]
    fn a_vm_without_triggers_or_prelude_is_skipped() {
        let (lua, _full) = vm(&["window"]);
        assert_eq!(report(&lua, Some(&game()), true), (0, 0));
        let bare = Lua::new();
        assert_eq!(report(&bare, Some(&game()), true), (0, 0));
    }

    #[test]
    fn initial_must_be_a_boolean() {
        let (lua, full) = vm(&["window"]);
        let e = lua
            .load(
                r#"
                local host = ...
                host.window.onTrigger({}, { initial = "yes" }, function() end)
                "#,
            )
            .call::<()>(&full)
            .unwrap_err()
            .to_string();
        assert!(e.contains("initial is true or false"), "{e}");
    }

    /// `onTrigger(matcher, cb)` is `onTrigger(matcher, nil, cb)`: it fires on an activation
    /// and, having no `initial`, asks for no report.
    #[test]
    fn the_options_can_be_left_out() {
        let (lua, full) = vm(&["window"]);
        lua.load(
            r#"
            local host = ...
            two = 0
            host.window.onTrigger({ title = { contains = "Arcade" } }, function(win)
              two = two + 1
              two_title = win.title
            end)
            "#,
        )
        .call::<()>(&full)
        .unwrap();
        assert_eq!(count(&lua, "requests"), 0, "no initial report was asked for");
        dispatch_activate(&lua, &window_titled("Desktop")).unwrap();
        assert_eq!(count(&lua, "two"), 0, "the matcher still applies");
        dispatch_activate(&lua, &game()).unwrap();
        assert_eq!(count(&lua, "two"), 1);
        assert_eq!(lua.globals().get::<String>("two_title").unwrap(), "Arcade Game");
        assert_eq!(report(&lua, Some(&game()), true), (0, 0), "not initial, not even on re-enable");
    }

    /// What a registration can get wrong raises from `onTrigger` itself, at the caller's line,
    /// rather than being stored and failing when a window comes forward.
    #[test]
    fn a_bad_registration_raises_at_once() {
        let (lua, full) = vm(&["window"]);
        let raise = |call: &str| -> String {
            // `=` names the chunk as written, so an error reads `game.luau:2:`.
            lua.load(format!("local host = ...\n{call}"))
                .set_name("=game.luau")
                .call::<()>(&full)
                .expect_err(call)
                .to_string()
        };
        for (call, says) in [
            ("host.window.onTrigger({}, nil, nil)", "the callback is a function, not a nil"),
            ("host.window.onTrigger({}, {})", "the callback is a function, not a nil"),
            ("host.window.onTrigger({})", "the callback is a function, not a nil"),
            ("host.window.onTrigger({}, nil, 'cb')", "the callback is a function, not a string"),
            ("host.window.onTrigger({}, { initial = true }, {})", "the callback is a function, not a table"),
            ("host.window.onTrigger({}, 'activate', function() end)", "opts is a table or nil, not a string"),
            ("host.window.onTrigger({}, 1, function() end)", "opts is a table or nil, not a number"),
            ("host.window.onTrigger('Notepad', function() end)", "the matcher is a table, not a string"),
            // Keys whose wrong type would otherwise match every window, or raise at dispatch.
            ("host.window.onTrigger({ app = 'game.exe' }, function() end)", "matcher.app is a table, not a string"),
            ("host.window.onTrigger({ os = 'windows' }, function() end)", "matcher.os is a table, not a string"),
            ("host.window.onTrigger({ windows = 'Game' }, function() end)", "matcher.windows is a table, not a string"),
            ("host.window.onTrigger({ macos = { app = 'Game' } }, function() end)", "matcher.macos.app is a table, not a string"),
            ("host.window.onTrigger({ where = true }, {}, function() end)", "matcher.where is a function, not a boolean"),
            ("host.window.onFocus(nil)", "onFocus: the callback is a function, not a nil"),
        ] {
            let e = raise(call);
            assert!(e.contains(says), "{call}: {e}");
            // Level 2: the error names the caller's chunk and line, not the prelude's.
            assert!(e.contains("game.luau:2:"), "{call} should point at the caller: {e}");
        }
        // None of them was registered: an activation calls nothing, and nothing raises.
        assert_eq!(dispatch_activate(&lua, &game()).map_err(|e| e.to_string()), Ok(()));
        assert_eq!(count(&lua, "requests"), 0);
        // A nil matcher is still every window, as before.
        lua.load("local host = ...\nanyHits = 0\nhost.window.onTrigger(nil, function() anyHits = anyHits + 1 end)")
            .call::<()>(&full)
            .unwrap();
        dispatch_activate(&lua, &game()).unwrap();
        assert_eq!(count(&lua, "anyHits"), 1);
        // Every checked key, each of the right type, registers and still matches.
        lua.load(
            r#"local host = ...
            shapedHits = 0
            host.window.onTrigger({
              app = { exe = "game.exe" },
              os = { "windows" },
              windows = { class = "GameWindow", app = { pid = 42 } },
              macos = { class = "unused here" },
              where = function(w) return w.title == "Arcade Game" end,
            }, function() shapedHits = shapedHits + 1 end)"#,
        )
        .call::<()>(&full)
        .unwrap();
        dispatch_activate(&lua, &window_titled("Desktop")).unwrap();
        assert_eq!(count(&lua, "shapedHits"), 0, "where still applies");
        dispatch_activate(&lua, &game()).unwrap();
        assert_eq!(count(&lua, "shapedHits"), 1);
    }

    /// `test` (like `find` and `findAll`) does not check a matcher's shape, as window.md's
    /// Matchers section says: a string `app`, or a string block for this platform, constrains
    /// nothing; a `where` that is not a function is skipped; an `os` that is not a table raises
    /// from inside the call.
    #[test]
    fn test_takes_a_misshapen_matcher_as_window_md_says() {
        let (lua, full) = vm(&["window"]);
        let win = win_to_table(&lua, &window_titled("Desktop")).unwrap();
        let test = |m: &str| -> mlua::Result<bool> {
            lua.load(format!("local host, win = ...\nreturn host.window.test({m}, win)"))
                .call::<bool>((&full, &win))
        };
        assert_eq!(test("{ app = 'game.exe', title = 'Desktop' }").unwrap(), true);
        assert_eq!(test("{ app = 'nothing.exe' }").unwrap(), true, "a string app constrains nothing");
        assert_eq!(test("{ windows = 'NoSuchClass' }").unwrap(), true, "nor does a string block for this platform");
        assert_eq!(test("{ macos = 'x' }").unwrap(), false, "a block for another platform still gates");
        assert_eq!(test("{ where = 1 }").unwrap(), true, "a where that is not a function is skipped");
        assert!(test("{ os = 'windows' }").is_err(), "an os that is not a table raises");
        assert!(test("{ app = 5 }").is_err(), "an app that is a number raises");
        assert!(test("{ windows = true }").is_err(), "so does a boolean block for this platform");
        assert_eq!(test("{ macos = 5 }").unwrap(), false, "one for another platform only gates");
    }

    /// The host's queue: one entry per module, `reprime` sticks, and only enabled modules
    /// are answered.
    #[test]
    fn the_queue_asks_once_per_module_and_skips_disabled_ones() {
        let mut q = Vec::new();
        queue_initial(&mut q, 2, false);
        queue_initial(&mut q, 0, false);
        queue_initial(&mut q, 2, false);
        assert_eq!(q, vec![(2, false), (0, false)]);
        queue_initial(&mut q, 0, true);
        queue_initial(&mut q, 0, false);
        assert_eq!(q, vec![(2, false), (0, true)], "a re-enable's reprime is not lost");
        let enabled = [true, false, false];
        assert_eq!(initial_due(q, |i| enabled.get(i).copied().unwrap_or(false)), vec![(0, true)]);
    }

    /// An activation dispatched into a module is its report: a queued re-enable of that
    /// module stops re-arming, and a disabled module's entry is left as it was.
    #[test]
    fn an_activation_stops_a_queued_re_enable_from_re_arming() {
        let mut q = vec![(0, true), (1, true), (2, false)];
        let enabled = [true, false, true];
        initial_reported_by_activation(&mut q, |i| enabled[i]);
        assert_eq!(q, vec![(0, false), (1, true), (2, false)]);
    }

    /// The binding the host installs, into a VM with the real prelude: bound to its module,
    /// never re-arming, and one queue entry however many `initial` triggers ask.
    #[test]
    fn the_real_request_binding_queues_its_module_once() {
        let (lua, full) = vm(&["window"]);
        let queue: Rc<RefCell<Vec<(usize, bool)>>> = Rc::new(RefCell::new(Vec::new()));
        let window: Table = full.get("window").unwrap();
        window.set("_requestInitial", initial_request_fn(&lua, 4, queue.clone(), |q| q).unwrap()).unwrap();
        register(&lua, &full, "plain", "nil");
        assert!(queue.borrow().is_empty(), "a trigger without initial asks nothing");
        register(&lua, &full, "a", "{ initial = true }");
        register(&lua, &full, "b", "{ initial = true }");
        assert_eq!(*queue.borrow(), vec![(4, false)]);
    }

    /// What `drain_initial` did with the host's parts, recorded.
    #[derive(Debug, PartialEq)]
    struct Drained {
        /// How often the foreground was asked for.
        asked: u32,
        /// How often the input epoch turned over.
        bumps: u32,
        /// The `vm_id` of every VM the duplication was opened for, in order.
        prewarmed: Vec<i64>,
        /// The modules whose dispatch raised.
        failed: Vec<usize>,
    }

    /// Runs `drain_initial` over `vms` (module index = position) with `front` in front.
    /// Each module's report is delivered at once, as a free module's mailbox runs it, with a hook
    /// standing for the duplication's opening that records the VM it ran in.
    fn drain(vms: &[&Lua], pending: Vec<(usize, bool)>, enabled: &[bool], front: Option<WinInfo>) -> Drained {
        let asked = Cell::new(0);
        let bumps = Cell::new(0);
        let prewarmed: Rc<RefCell<Vec<i64>>> = Rc::default();
        let mut failed = Vec::new();
        drain_initial(
            pending,
            |i| enabled.get(i).copied().unwrap_or(false),
            |i| vms.get(i).copied(),
            || {
                asked.set(asked.get() + 1);
                front
            },
            || bumps.set(bumps.get() + 1),
            |i, lua, win, reprime| {
                let id = lua.globals().get::<i64>("vm_id").unwrap();
                let p = prewarmed.clone();
                let hook = lua
                    .create_function(move |_, ()| {
                        p.borrow_mut().push(id);
                        Ok(())
                    })
                    .unwrap();
                if dispatch_initial(lua, win, reprime, Some(hook)).is_err() {
                    failed.push(i);
                }
            },
        );
        let prewarmed = prewarmed.borrow().clone();
        Drained { asked: asked.get(), bumps: bumps.get(), prewarmed, failed }
    }

    /// A prelude VM for module `id`, told apart by its `vm_id` global.
    fn module_vm(id: i64) -> (Lua, Table) {
        let (lua, full) = vm(&["window"]);
        lua.globals().set("vm_id", id).unwrap();
        (lua, full)
    }

    #[test]
    fn the_drain_asks_for_the_foreground_only_when_a_report_is_wanted() {
        let (a, fa) = module_vm(0);
        let none = Drained { asked: 0, bumps: 0, prewarmed: vec![], failed: vec![] };
        assert_eq!(drain(&[&a], vec![], &[true], Some(game())), none, "nothing queued");
        // Enabling a module whose triggers never asked for a report: nothing to ask about.
        register(&a, &fa, "plain", "nil");
        assert!(!a.load("return host.window._wantsInitial(true)").eval::<bool>().unwrap());
        assert_eq!(drain(&[&a], vec![(0, true)], &[true], Some(game())), none);
        assert_eq!(count(&a, "plain"), 0);
        // One that did: asked for once, and used up, so a second pass asks nothing.
        register(&a, &fa, "hits", "{ initial = true }");
        let once = Drained { asked: 1, bumps: 1, prewarmed: vec![0], failed: vec![] };
        assert_eq!(drain(&[&a], vec![(0, false)], &[true], Some(game())), once);
        assert_eq!(drain(&[&a], vec![(0, false)], &[true], Some(game())), none);
        assert_eq!((count(&a, "hits"), count(&a, "plain")), (1, 0));
    }

    /// Two modules owed a report: one foreground query, one turn of the input epoch before
    /// the first callback of either, and the duplication opened once per VM.
    #[test]
    fn two_modules_share_one_query_and_one_epoch_turn() {
        let (a, fa) = module_vm(0);
        let (b, fb) = module_vm(1);
        register(&a, &fa, "hits", "{ initial = true }");
        register(&b, &fb, "hits", "{ initial = true }");
        let got = drain(&[&a, &b], vec![(0, false), (1, false)], &[true, true], Some(game()));
        assert_eq!(got, Drained { asked: 1, bumps: 1, prewarmed: vec![0, 1], failed: vec![] });
        assert_eq!((count(&a, "hits"), count(&b, "hits")), (1, 1));
    }

    #[test]
    fn a_disabled_module_is_dropped_and_costs_no_query() {
        let (a, fa) = module_vm(0);
        let (b, fb) = module_vm(1);
        register(&a, &fa, "hits", "{ initial = true }");
        register(&b, &fb, "hits", "{ initial = true }");
        let none = Drained { asked: 0, bumps: 0, prewarmed: vec![], failed: vec![] };
        assert_eq!(drain(&[&a, &b], vec![(0, false)], &[false, true], Some(game())), none);
        let got = drain(&[&a, &b], vec![(0, false), (1, false)], &[false, true], Some(game()));
        assert_eq!(got, Drained { asked: 1, bumps: 1, prewarmed: vec![1], failed: vec![] });
        assert_eq!((count(&a, "hits"), count(&b, "hits")), (0, 1));
    }

    /// The input epoch turns when the report is asked for — once, whatever matches — so a module
    /// that is busy when it comes hears it later without a second turn: with a titled window in
    /// front and no trigger of the module matching it, it turns all the same.
    #[test]
    fn the_input_epoch_turns_once_when_the_report_arrives_whatever_matches() {
        let (a, fa) = module_vm(0);
        let (b, fb) = module_vm(1);
        register(&a, &fa, "hits", "{ initial = true }");
        register(&b, &fb, "hits", "{ initial = true }");
        let got = drain(&[&a, &b], vec![(0, false), (1, false)], &[true, true], Some(window_titled("Desktop")));
        assert_eq!(got, Drained { asked: 1, bumps: 1, prewarmed: vec![], failed: vec![] });
        assert_eq!((count(&a, "hits"), count(&b, "hits")), (0, 0));
    }

    /// Neither platform hands an untitled foreground window to a trigger on activation, so
    /// the report does not either — and it is used up all the same.
    #[test]
    fn a_window_without_a_title_is_not_reported() {
        let (a, fa) = module_vm(0);
        lua_catch_all(&a, &fa);
        let got = drain(&[&a], vec![(0, false)], &[true], Some(window_titled("")));
        assert_eq!(got, Drained { asked: 1, bumps: 0, prewarmed: vec![], failed: vec![] });
        assert_eq!(count(&a, "any"), 0);
        assert!(!a.load("return host.window._wantsInitial(false)").eval::<bool>().unwrap(), "used up");
        // The same catch-all hears a titled window when the module is enabled again.
        drain(&[&a], vec![(0, true)], &[true], Some(window_titled("Desktop")));
        assert_eq!(count(&a, "any"), 1);
    }

    fn lua_catch_all(lua: &Lua, full: &Table) {
        lua.load(
            r#"
            local host = ...
            any = 0
            host.window.onTrigger({}, { initial = true }, function() any = any + 1 end)
            "#,
        )
        .call::<()>(full)
        .unwrap();
    }

    /// Enabling a module queues a re-arm; an activation delivered on the same tick, before the
    /// drain, is the report — so the window that came forward is reported once, not twice.
    #[test]
    fn a_re_enable_and_an_activation_in_one_tick_report_once() {
        let (a, fa) = module_vm(0);
        register(&a, &fa, "hits", "{ initial = true }");
        drain(&[&a], vec![(0, false)], &[true], Some(game()));
        assert_eq!(count(&a, "hits"), 1, "the report at load");
        // Enabled from the manager, and the game comes forward before the tick's drain.
        let mut pending = vec![(0, true)];
        dispatch_activate(&a, &game()).unwrap();
        initial_reported_by_activation(&mut pending, |_| true);
        assert_eq!(count(&a, "hits"), 2, "the activation");
        let got = drain(&[&a], pending, &[true], Some(game()));
        assert_eq!(got.asked, 0, "nothing left to report");
        assert_eq!(count(&a, "hits"), 2);
        // Without an activation in between, the re-enable does report again.
        drain(&[&a], vec![(0, true)], &[true], Some(game()));
        assert_eq!(count(&a, "hits"), 3);
    }

    #[test]
    fn a_module_whose_report_raises_is_named_and_the_others_still_hear() {
        let (a, fa) = module_vm(0);
        let (b, fb) = module_vm(1);
        a.load(
            r#"
            local host = ...
            host.window.onTrigger({ title = { contains = "Arcade" } }, { initial = true },
              function() error("boom") end)
            "#,
        )
        .call::<()>(&fa)
        .unwrap();
        register(&b, &fb, "hits", "{ initial = true }");
        let got = drain(&[&a, &b], vec![(0, false), (1, false)], &[true, true], Some(game()));
        assert_eq!(got, Drained { asked: 1, bumps: 1, prewarmed: vec![0, 1], failed: vec![0] });
        assert_eq!(count(&b, "hits"), 1);
    }
}

#[cfg(test)]
mod on_change_ownership_tests {
    use super::*;

    /// A VM tagged as module `idx`'s, the way `populate_vm` tags it.
    fn vm(idx: usize) -> Lua {
        let lua = Lua::new();
        lua.set_app_data(image_search::VmOwner { idx, gen: 100 + idx as u64 });
        lua
    }

    /// Registers a callback through the binding's own filing function.
    fn register(map: &mut OnChangeMap, lua: &Lua, setting_of: usize, key: &str) {
        let cb = lua.create_function(|_, ()| Ok(())).unwrap();
        file_on_change(map, lua, setting_of, key.to_string(), cb).unwrap();
    }

    fn owners(map: &OnChangeMap, setting_of: usize, key: &str) -> Vec<usize> {
        map.get(&(setting_of, key.to_string()))
            .map(|l| l.iter().map(|(o, ..)| *o).collect())
            .unwrap_or_default()
    }

    /// The bug: a dependency's code, running inside a dependent, watches the dependency's
    /// setting. Reloading the dependent purged only the dependent's OWN settings' callbacks,
    /// so the old VM's callback stayed — keeping that VM alive — and fired beside the new
    /// VM's own on every change.
    #[test]
    fn a_dependents_reload_takes_the_callbacks_its_vm_registered() {
        // Module 0 is the dependency, module 1 a dependent running 0's code.
        let dep = vm(0);
        let old_dependent = vm(1);
        let mut map = OnChangeMap::new();
        register(&mut map, &dep, 0, "volume"); // the dependency's own VM
        register(&mut map, &old_dependent, 0, "volume"); // 0's code inside 1's VM
        register(&mut map, &old_dependent, 1, "mode"); // 1's own setting
        assert_eq!(owners(&map, 0, "volume"), vec![0, 1]);

        // What `purge_module(1)` does, through the function it calls.
        let weak = old_dependent.weak();
        drop(old_dependent);
        purge_on_change(&mut map, 1);

        assert_eq!(owners(&map, 0, "volume"), vec![0], "only the dependency's own VM is left");
        assert!(!map.contains_key(&(1, "mode".to_string())), "an emptied key goes with it");
        assert!(
            weak.try_upgrade().is_none(),
            "nothing may keep the reloaded module's old VM alive"
        );
    }

    /// The other direction: reloading the DEPENDENCY leaves the callbacks its code registered
    /// inside dependents, whose VMs are still alive and still running that code. Each goes
    /// when its dependent is rebuilt (see `drop_on_change_from` for when that is).
    #[test]
    fn a_dependencys_reload_leaves_what_its_code_registered_elsewhere() {
        let (dep, a, b) = (vm(0), vm(1), vm(2));
        let mut map = OnChangeMap::new();
        register(&mut map, &dep, 0, "volume");
        register(&mut map, &a, 0, "volume");
        register(&mut map, &b, 0, "volume");
        purge_on_change(&mut map, 0);
        assert_eq!(owners(&map, 0, "volume"), vec![1, 2]);
    }

    /// A failed hot-load rolls back to `n` modules: what the failed VM registered goes, for its
    /// own settings and for its dependencies' alike.
    #[test]
    fn a_rollback_takes_what_the_failed_vm_registered_for_a_surviving_dependency() {
        let (dep, failed) = (vm(0), vm(1));
        let mut map = OnChangeMap::new();
        register(&mut map, &dep, 0, "volume");
        register(&mut map, &failed, 0, "volume");
        register(&mut map, &failed, 1, "mode");
        // What `rollback_to(1)` does, through the function it calls.
        rollback_on_change(&mut map, 1);
        assert_eq!(owners(&map, 0, "volume"), vec![0]);
        assert_eq!(map.len(), 1);
    }

    /// A state nobody tagged — none in production, `populate_vm` tags every VM before any code
    /// runs — is owned by the identity it registered under, which is the rule from before.
    #[test]
    fn an_untagged_vm_falls_back_to_the_settings_owner() {
        let lua = Lua::new();
        let mut map = OnChangeMap::new();
        register(&mut map, &lua, 3, "k");
        assert_eq!(owners(&map, 3, "k"), vec![3]);
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
        assert!(call_plain(&err, ()).unwrap_err().contains("boom"));
        assert!(matches!(call_guarded(&lua, "test", &err, ()), Called::Failed(e) if e.contains("boom")));
        // A clean callback succeeds.
        let ok: Function = lua.load("return function() end").eval().unwrap();
        assert!(call_plain(&ok, ()).is_ok());
        assert_eq!(call_guarded(&lua, "test", &ok, ()), Called::Ok);
        // A panic under a guarded call is a failure of the call, not of the process.
        quiet_expected_panics();
        let boom = lua.create_function(|_, ()| -> mlua::Result<()> { panic!("{EXPECTED_PANIC}under a call") }).unwrap();
        assert!(matches!(call_guarded(&lua, "test", &boom, ()), Called::Failed(e) if e.contains("under a call")));
    }

    /// A call into a VM the guard stopped is not made; one that stops it is not the module's error.
    #[test]
    fn a_plain_call_of_a_stopped_vm_is_refused_and_one_that_stops_it_is_no_failure() {
        let guard = vm_guard::Guard::new();
        let lua = vm_guard::new_vm(&guard);
        vm_guard::describe_for_test(&lua, 0, "com.x.a", &[("com.x.a", 4)]);
        vm_guard::loaded(&lua);
        let fill: Function = lua
            .load("return function() local t = {} while true do t[#t + 1] = string.rep('x', 1048000) .. #t end end")
            .eval()
            .unwrap();
        // Made before the stop: a stopped VM raises at the first safepoint of anything run in it.
        let ok: Function = lua.load("return function() end").eval().unwrap();
        assert_eq!(call_guarded(&lua, "arbiter onDeactivate", &fill, ()), Called::Stopped);
        assert_eq!(call_guarded(&lua, "arbiter onActivate", &ok, ()), Called::Refused);
        assert_eq!(guard.take_trips().len(), 1, "one stop, no hook on this thread");
    }

    /// A Rust panic under the guard is a failure of the call, not of the process.
    #[test]
    fn a_rust_panic_under_the_guard_is_caught() {
        // Kept out of the test output.
        quiet_expected_panics();
        let p = guard(|| panic!("{EXPECTED_PANIC}kaboom")).unwrap_err();
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
        quiet_expected_panics();
        let panicked = run(|| panic!("{EXPECTED_PANIC}load boom"));
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
        // Two spellings of one role are one combination to the contest, so a `Cmd+S` beside an
        // inherited `Ctrl+S` is a conflict, not two holders — on every platform.
        assert_eq!(backend::key_spec("Cmd+S"), backend::key_spec("Ctrl+S"));
        assert_eq!(backend::key_spec("Meta+S"), backend::key_spec("Win+S"));
        assert_ne!(backend::key_spec("Win+S"), backend::key_spec("Ctrl+S"));
    }

    /// The host's own reload key: the four modifiers on Windows, Command-Shift on a Mac — which
    /// is off VoiceOver's Control+Option layer, where the four-modifier chord would be.
    #[test]
    fn the_reload_key_is_the_chord_it_was() {
        use backend::{check_spec_for, key_spec, KeyOs, MASK_ALT, MASK_CTRL, MASK_SHIFT, MASK_WIN};
        let expected = if cfg!(target_os = "macos") { RELOAD_HOTKEY_MACOS } else { RELOAD_HOTKEY_WINDOWS };
        assert_eq!(RELOAD_HOTKEY_SPEC, expected);
        assert_eq!(key_spec(RELOAD_HOTKEY_WINDOWS), Some((0x74, MASK_CTRL | MASK_ALT | MASK_SHIFT | MASK_WIN)));
        assert_eq!(key_spec(RELOAD_HOTKEY_MACOS), Some((0x74, MASK_CTRL | MASK_SHIFT)));
        assert!(backend::hotkey_refusal_for(KeyOs::Windows, RELOAD_HOTKEY_WINDOWS).is_none());
        assert!(backend::hotkey_refusal_for(KeyOs::Linux, RELOAD_HOTKEY_WINDOWS).is_none());
        assert!(check_spec_for(KeyOs::Macos, RELOAD_HOTKEY_MACOS, |_, _| None).reasons.is_empty());
        assert_eq!(
            check_spec_for(KeyOs::Macos, RELOAD_HOTKEY_WINDOWS, |_, _| None).reasons,
            vec![backend::REASON_VOICEOVER],
            "why the Mac has a chord of its own"
        );
        assert_eq!(
            backend::normalize_spec_for(KeyOs::Windows, RELOAD_HOTKEY_WINDOWS).as_deref(),
            Some("Ctrl+Alt+Shift+Win+F5"),
            "the log's spelling on Windows"
        );
        assert_eq!(
            backend::describe_spec_for(KeyOs::Macos, RELOAD_HOTKEY_MACOS, backend::KeyStyle::Spoken)
                .as_deref(),
            Some("Shift+Command+F5")
        );
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
    // Which modules' code will run in this VM, from their manifests on disk — first, because the
    // VM's memory limit is the sum of what each may use (`[limits] memory_mib`, the module itself
    // and each code dependency once), and it is set before anything runs in it. Read again at
    // every build, so a reload after editing a manifest applies it.
    let mut code_deps: Vec<capture_source::CodeDep> = Vec::new();
    collect_code_deps(
        parent,
        &module.manifest.dependencies,
        &module.manifest.optional_dependencies,
        &mut code_deps,
        &mut HashSet::new(),
    )?;
    vm_guard::describe(lua, vm_guard::VmInfo::of(idx, &module.manifest, &code_deps));
    for key in &module.manifest.limits.unknown {
        logging::line("manager", &limits_unknown_line(id, key));
    }
    // Recorded before anything runs in it: whose VM this is. An image search asked for by any
    // code in it, a code dependency's too, is then answered while THIS module is enabled — and
    // never into an older VM that once stood at the same index.
    image_search::register_vm(shared, lua, idx);
    // This VM's host: identity (settings/resource/path) + ownership (hotkeys/timers/
    // keys/arbiter) scoped to the module (idx). Set as the global so its entry resolves it
    // (as the gated view, below); code dependencies get an identity-scoped variant. The
    // host's own window-event dispatch does not go through the global at all — see
    // `install_window_prelude`.
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
    // still match the file. The same call keeps the handle the window events are delivered
    // through, for the same reason one level up: the delivery is the host's, not the module's.
    install_window_prelude(lua, &host_m)?;
    // How long this VM's running handler has been parked, which the prelude's `[dispatch]` line
    // leaves out of the time it reports (`WAITED_KEY`).
    let sh = shared.clone();
    lua.set_named_registry_value(WAITED_KEY, lua.create_function(move |_, ()| Ok(sh.tasks.waited_ms(idx)))?)?;

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
    // Which picture this VM's screen and OCR reads see, from the manifests just read — so a
    // reload after editing `[screen]` needs nothing else refreshed. Before any code runs.
    capture_source::apply(lua, id, &module.manifest, &mut code_deps);
    for capture_source::CodeDep { id: dep_id, entry: dep_entry, entry_rel: dep_rel, .. } in &code_deps {
        let dep_code = read_luau_source(dep_entry).with_context(|| {
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
        let dep_name = dep_entry.display().to_string();
        // Its chunk is the dependency's file: a stop in it names that module, and takes it.
        vm_guard::name_chunk(lua, &dep_name, dep_id, dep_rel);
        let dep_ret = load_step(lua, format!("the code of {dep_id}"), || {
            eval_on_host(lua, &host_dep, &dep_code, &dep_name)
        })
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
    let code = read_luau_source(&entry)
        .with_context(|| format!("entry point not readable: {}", entry.display()))?;
    vm_guard::name_chunk(lua, &entry.display().to_string(), id, &module.manifest.entry);
    let ret: mlua::Value =
        load_step(lua, "its entry file", || lua.load(code).set_name(entry.display().to_string()).eval())
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
    // Looked up inside the step: `get` runs the table's `__index`, which is the module's code too.
    if let mlua::Value::Table(t) = &ret {
        load_step(lua, "its activate function", || match t.get::<mlua::Value>("activate") {
            Ok(mlua::Value::Function(activate)) => activate.call::<()>(()),
            _ => Ok(()),
        })
        .with_context(|| format!("error activating module '{id}'"))?;
    }
    // Loaded: a stop in this VM from now on stops the module, rather than failing its load.
    vm_guard::loaded(lua);
    Ok(())
}

/// The `[manager]` line for a key in `[limits]` this application does not read.
fn limits_unknown_line(id: &str, key: &str) -> String {
    format!("[{id}] module.toml: [limits] has a key this application does not read: {key}")
}

/// One step of loading a module — a code dependency, the entry file, `activate` — as an entry of
/// the guard: a memory error that reaches the host stops the VM, and the load fails with what
/// stopped it (`stops::load_failure`) rather than with Luau's own words. So does a step whose VM
/// was stopped earlier in the load, by a callback of its own that ran inside it.
fn load_step<T>(
    lua: &Lua,
    what: impl Into<std::borrow::Cow<'static, str>>,
    run: impl FnOnce() -> mlua::Result<T>,
) -> Result<T> {
    let Ok(entry) = vm_guard::Entry::enter(lua, vm_guard::EntryKind::Load, what) else {
        anyhow::bail!(stops::load_failure(vm_guard::last_trip(lua).as_ref()));
    };
    let result = run();
    entry.finish(&result);
    // Looked at once the entry has ended too: a step past its limit that returned while its VM
    // was still looking where it was is stopped as the entry ends (`vm_guard`).
    drop(entry);
    if vm_guard::stopped(lua) {
        anyhow::bail!(stops::load_failure(vm_guard::last_trip(lua).as_ref()));
    }
    Ok(result?)
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
    let lua = vm_guard::new_vm(&shared.guard);
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
/// a rebuild error the module is left unregistered (effectively unloaded), its old VM
/// replaced by an empty one, with its store rolled back, and the error returned; a later
/// successful reload recovers.
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

    let lua = vm_guard::new_vm(&shared.guard);
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
        // And the OLD VM goes as well, for an empty one. The purge above took every
        // registration the host holds for it, but its window triggers and focus callbacks live
        // inside the VM, in the window prelude's own lists, and window events are delivered
        // into every enabled module's VM. Left in place, an overlay in it went on activating on
        // the next matching window — registering its hotkeys again — for a module this reload
        // had just reported as failed and inactive. An empty VM has nothing to deliver to
        // (`window_has_triggers` is false, and `dispatch_activate` / `dispatch_focus` skip it);
        // the next successful reload builds the real one in its place.
        if let Some(m) = modules.borrow_mut().get_mut(idx) {
            m.lua = vm_guard::new_vm(&shared.guard);
        }
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

/// The manager's checkbox for loaded module `idx`. Turning on a module whose VM the guard stopped
/// builds it afresh first, as a reload does, while it is still off: a stop ends a callback
/// anywhere — an overlay's `onDeactivate` after it marked itself inactive and before it gave its
/// keys back — and the same VM, turned on again, would carry that half-done state on (its stale
/// hotkeys registered again, swallowing keys while the overlay is not in front). The rebuilt VM
/// runs its entry file and `activate` again and reads its settings as they are now; what waited
/// for the old one — a setting's `onChange` — goes with it. Ticking the library of a library's
/// stop turns on every module the stop took with it (`Stops::turned_on_with`), each rebuilt. A
/// rebuild that fails leaves that module off, with the reason in the log and the error window.
fn toggle_module(shared: &Rc<Shared>, modules: &Rc<RefCell<Vec<Module>>>, idx: usize, enabled: bool) {
    if !enabled {
        shared.set_enabled(idx, false);
        return;
    }
    for i in shared.stops.turned_on_with(idx) {
        let stopped = modules.borrow().get(i).is_some_and(|m| vm_guard::stopped(&m.lua));
        if stopped {
            if let Err(e) = reload_module(shared, modules, i) {
                let (id, name) = modules.borrow().get(i).map(|m| (m.id.clone(), m.name.clone())).unwrap_or_default();
                logging::line("guard", &format!("[{id}] could not be built afresh to be turned on again: {e:#}"));
                shared.queue_dialog(
                    format!("{id}\u{1}turn-on"),
                    format!("Module not turned on: {id}"),
                    format!("{name} could not be built afresh to be turned on again, and stays off:\n\n{e:#}"),
                );
                if i == idx {
                    // Its box stays unticked: what the user asked for did not happen.
                    shared.module_changes.borrow_mut().push(gui::ModuleChange { idx: i, enabled: false, stopped: shared.stops.row_note(i) });
                }
                continue;
            }
        }
        shared.set_enabled(i, true);
    }
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

/// Module `idx`'s settings as the manager shows them: each one's schema and the value stored now
/// (its default when none is), sorted by label. Read again whenever Settings… opens, so the dialog
/// shows what a change since — its own OK or the module's `set` — left there.
fn setting_descs(shared: &Shared, id: &str, idx: usize) -> Vec<gui::SettingDesc> {
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
                        value: store.get(id, key).unwrap_or_else(|| f.default.clone()),
                        min: f.min,
                        max: f.max,
                        choices: f.choices.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    settings.sort_by(|a, b| a.label.cmp(&b.label));
    settings
}

/// Builds the manager's display row for module `idx`: its settings (schema +
/// stored/default values), enabled state, and declared dependencies. Used both
/// for the startup snapshot and when a hot-loaded module is added to the list.
fn module_info(shared: &Shared, m: &Module, idx: usize) -> gui::ModuleInfo {
    let settings = setting_descs(shared, &m.id, idx);
    gui::ModuleInfo {
        name: m.name.clone(),
        version: m.version.clone(),
        id: m.id.clone(),
        module_idx: Some(idx),
        enabled: shared.enabled.borrow().get(idx).copied().unwrap_or(true),
        dependencies: m.dependencies.clone(),
        settings,
        unsupported: None,
        stopped: shared.stops.row_note(idx),
    }
}

/// The text of the manager's Details… for module `idx` (stops.rs, `details_text`): its state, its
/// VM's memory limit and what it uses now, and where else its code runs.
fn module_details(shared: &Shared, modules: &[Module], idx: usize) -> String {
    let Some(m) = modules.get(idx) else {
        return "This module is not loaded.".to_string();
    };
    let vm = vm_guard::info(&m.lua).unwrap_or_default();
    let carriers = vm_guard::carriers_of(&shared.guard, &m.id);
    let stop = shared.stops.record(idx);
    stops::details_text(&stops::Details {
        name: &m.name,
        version: &m.version,
        id: &m.id,
        enabled: shared.module_on(idx),
        stop: stop.as_deref(),
        vm: &vm,
        used: m.lua.used_memory() as u64,
        carriers: &carriers,
        dependencies: &m.dependencies,
        now: Instant::now(),
    })
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
            stopped: None,
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
    /// Stops `host.ocr.recognize`'s two threads. Handed to `run` by [`Manager::ocr_shutdown`] before
    /// anything is loaded, because the manager itself is dropped inside `run`'s closure and the
    /// threads must not depend on that.
    ocr_shutdown: ocr::service::ShutdownHandle,
    modules: Rc<RefCell<Vec<Module>>>,
    /// Module ids the user disabled in a previous run (from the portable config).
    disabled_ids: HashSet<String>,
    /// Module ids currently being loaded — for dependency-cycle detection.
    loading: HashSet<String>,
    /// The guard's watchdog thread (`vm-guard`, vm_guard.rs): stopped by [`Manager::stop_guard`]
    /// at exit, or when the manager is dropped.
    watchdog: Option<vm_guard::Watchdog>,
}

impl Manager {
    pub fn new() -> Result<Self> {
        // Fixes the origin of `host.now()` — and of every gamepad event's `time` — before
        // anything can read it. See `clock_origin`.
        clock_origin();
        // This thread loads the modules and then runs the event loop — the window's tick and the
        // headless loop alike — so it is the loop no recogniser may run on (loop_guard.rs).
        loop_guard::mark_loop_thread();
        let backend = backend::platform();
        // `host.ocr.recognize`'s threads, first: the recognise thread publishes the language list
        // as its first act, and the speech engines below can take seconds to open, so a module
        // asking for the languages in its `activate` finds them there.
        let (ocr, ocr_shutdown) = ocr::service::Service::spawn(backend.ocr_worker());
        // Timed because it is not free and it is not obvious: building the fallback speech
        // engine measured 3.1 seconds in a test, and this call is synchronous — that is
        // start-up time the user waits through, for a voice that on a machine with a screen
        // reader will very likely never say anything.
        let began = Instant::now();
        let speech = match speech::Speech::new() {
            Ok(s) => s,
            Err(e) => {
                // No manager, so nothing will hand `run` the handle: the threads stop here.
                ocr_shutdown.shutdown(ocr::policy::SHUTDOWN);
                return Err(e);
            }
        };
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
        // The display part of it, to write again after a display change only when it changed.
        let display_env = backend.display_environment();
        let store = settings::Store::load();
        let disabled_ids = store.disabled_ids();
        let (image_tasks, image_task_rx) = std::sync::mpsc::channel::<ImageTask>();
        let (image_result_tx, image_results) = std::sync::mpsc::channel::<ImageResult>();
        // Capture and comparison fn pointers taken before `backend` is moved into Shared — they
        // are `Copy` and `Send`, so the worker can capture without the non-`Send` Rc backend.
        image_search::spawn_image_worker(backend.capture_fn(), backend.compare_fn(), image_task_rx, image_result_tx);
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
            captures: captures::Captures::default(),
            store: RefCell::new(store),
            schemas: RefCell::new(Vec::new()),
            on_change: RefCell::new(HashMap::new()),
            dirty: Cell::new(false),
            timers: timers::Timers::default(),
            exports: RefCell::new(HashMap::new()),
            arbiter: RefCell::new(HashMap::new()),
            observations: RefCell::new(Observations::default()),
            ev_counts: Cell::new((0, 0, 0, 0, 0)),
            pads: gamepad_api::Pads::default(),
            epoch: Cell::new(0),
            input_epoch: Cell::new(0),
            next_arbiter: Cell::new(0),
            errors: RefCell::new(Vec::new()),
            error_seen: RefCell::new(HashSet::new()),
            error_repeats: RefCell::new(logging::Repeats::default()),
            repeats_asked: Cell::new(None),
            os_refused: RefCell::new(HashMap::new()),
            captured_line: RefCell::new((None, String::new())),
            system_since: RefCell::new(system_events::Since::default()),
            display_env_said: RefCell::new(display_env),
            image_tasks,
            image_results,
            pending_image: RefCell::new(HashMap::new()),
            ended_images: RefCell::new(Vec::new()),
            next_image_id: Cell::new(0),
            snap_bytes: Rc::new(Cell::new(0)),
            snap_state: snapshot::SnapState::default(),
            ocr,
            ocr_state: ocr::lua::OcrState::default(),
            tasks: task::Tasks::default(),
            mail: mailbox::Mailboxes::default(),
            guard: vm_guard::Guard::new(),
            stops: stops::Stops::default(),
            module_changes: RefCell::new(Vec::new()),
            held_buttons: RefCell::new(BTreeMap::new()),
            vm_gens: RefCell::new(HashMap::new()),
            template_cache: RefCell::new(HashMap::new()),
            template_seq: Cell::new(0),
            recheck_requested: Cell::new(false),
            initial_pending: RefCell::new(Vec::new()),
            notify: RefCell::new(None),
        });
        // Claimed before the first module is loaded, so it is the one id no module can be
        // given (see the field). Registering with the OS happens later, in `run`, on the
        // thread that will receive it.
        shared.reload_hotkey_id.set(shared.alloc_id());
        // What the guard's stops go to when the callback that began them has returned: turned
        // off at once, keys back, the log line (stops.rs). Weak, so the hook does not keep the
        // host alive; this thread runs every module's code.
        {
            let weak = Rc::downgrade(&shared);
            vm_guard::set_exit_hook(Rc::new(move |trips| {
                if let Some(sh) = weak.upgrade() {
                    stops::mark(&*sh, trips);
                }
            }));
        }
        // The guard's own: this thread's guard for the bindings' `host_call!`, and the watchdog
        // that looks at the clocks every 50 ms — before the first module loads, since a load is
        // held to the same budgets as a callback.
        vm_guard::install(&shared.guard);
        let watchdog = vm_guard::spawn_watchdog(shared.guard.clone());
        Ok(Self {
            shared,
            ocr_shutdown,
            modules: Rc::new(RefCell::new(Vec::new())),
            disabled_ids,
            loading: HashSet::new(),
            watchdog: Some(watchdog),
        })
    }

    /// What stops `host.ocr.recognize`'s threads at exit — see the field.
    pub fn ocr_shutdown(&self) -> ocr::service::ShutdownHandle {
        self.ocr_shutdown.clone()
    }

    /// At exit, after the event loop: the watchdog stops, the `[cpu]` summary of the whole run is
    /// written — what each module took of the event loop — and every mouse button a module still
    /// holds down is let go of.
    fn stop_guard(&mut self) {
        if let Some(mut w) = self.watchdog.take() {
            w.stop();
        }
        for line in self.shared.guard.cpu_summary() {
            logging::line("cpu", &line);
        }
        self.shared.release_all_held_buttons();
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
        let has_keys = !self.shared.captures.is_empty();
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
        let has_own_work = !self.shared.timers.is_empty()
            || !self.shared.pending_image.borrow().is_empty()
            || self.shared.ocr_state.has_pending()
            || self.shared.snap_state.has_pending()
            || self.shared.pads.has_listeners()
            // An `onChange` another module's `set` queued at load runs on a tick too.
            || !self.shared.mail.is_empty();
        let headless = appcfg::headless();

        // The tray manager is shown whenever there's a window (non-headless), even
        // with nothing loaded yet, so modules can be browsed/installed/managed.
        // Headless has no window, so it only runs when something is actually pending.
        if has_hotkeys || has_keys || has_triggers || has_own_work || !headless {
            // Sleep, lock, display changes: heard on a thread of the backend's and delivered
            // through the pump as `on_system`. Installs no hook and takes no key.
            self.shared.backend.watch_system();
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
                // By its canonical spelling, like every module hotkey, so a failure line names
                // one chord once rather than the token and the chord side by side.
                match self.shared.backend.register_hotkey(reload_id, &reload_hotkey_shown()) {
                    Ok(()) => logging::line(
                        "manager",
                        &format!("{} reloads every module", reload_hotkey_shown()),
                    ),
                    Err(e) => logging::line(
                        "manager",
                        &format!("reload hotkey {} unavailable: {e}", reload_hotkey_shown()),
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
                let toggle_modules = self.modules.clone();
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
                        Some(i) => toggle_module(&toggle_shared, &toggle_modules, i, enabled),
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
                        // hazard. Every captured key and every hotkey waits for this thread to
                        // run its callback, so a stall is a Tab that moves the overlay's focus
                        // late, for a user who navigates entirely by Tab and cannot see where
                        // the focus went. It used to be worse on Windows: the low-level
                        // keyboard hook ran on THIS thread, a thread busy inside a poll
                        // callback could not answer it, and past LowLevelHooksTimeout Windows
                        // delivered the keystroke WITHOUT us — a Tab the overlay believed it had
                        // captured landed in the plugin instead. The hook has its own thread
                        // now (backend/windows.rs, keyboard_hook_thread); the macOS event tap
                        // still shares this one. Measured landmark polls have already been seen
                        // at 400 ms.
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
                        shared.ev_counts.set((0, 0, 0, 0, 0));
                        backend.pump_pending(&mut dispatcher);
                        let events_ms = pump_started.elapsed().as_millis();
                        let (act_n, act_ms, focus_ms, _, pad_ms) = shared.ev_counts.get();
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
                        let other_ms = events_ms
                            .saturating_sub(act_ms)
                            .saturating_sub(focus_ms)
                            .saturating_sub(pad_ms);
                        let t = std::time::Instant::now();
                        shared.fire_due_timers();
                        shared.fire_pad_tick();
                        let timers_ms = t.elapsed().as_millis();
                        let t = std::time::Instant::now();
                        shared.fire_image_results();
                        let images_ms = t.elapsed().as_millis();
                        let t = std::time::Instant::now();
                        // The callbacks counted, for the line below: one slow callback is told
                        // from many quick ones by the count.
                        let ocr_n = shared.fire_ocr_results();
                        let ocr_ms = t.elapsed().as_millis();
                        let t = std::time::Instant::now();
                        shared.fire_snapshot_results();
                        let snapshots_ms = t.elapsed().as_millis();
                        // The events that waited in a busy module's mailbox, and every setting
                        // changed in the dialog or by another module since the last tick.
                        let t = std::time::Instant::now();
                        let queued = shared.run_queued_events();
                        let queued_ms = t.elapsed().as_millis();
                        // `onTrigger { initial = true }`: the window already in front, for
                        // the triggers that asked since the last tick (at load, on enable, or
                        // from a timer earlier in this very tick). Inside the measured
                        // iteration: its callbacks are trigger callbacks, a detection read
                        // among them, and they hold the keyboard hook up like any other.
                        let t = std::time::Instant::now();
                        dispatcher.dispatch_initial();
                        let initial_ms = t.elapsed().as_millis();
                        // A module the guard stopped during this turn: the rest of disabling it,
                        // then the dialog, the sentence and its row (stops.rs).
                        let t = std::time::Instant::now();
                        shared.settle_stops();
                        let stops_ms = t.elapsed().as_millis();
                        let pump_ms = pump_started.elapsed().as_millis();
                        if pump_ms >= 250 {
                            // The hazard is different on each platform, and the line has to
                            // name the right one: a Mac tester's log full of "Windows stops
                            // waiting for our keyboard hook" was a puzzle before it was a
                            // diagnosis. On macOS the system switches off an event tap whose
                            // thread stops answering; the watchdog in tap.rs re-enables it
                            // and logs that it did, so the two lines can be read together.
                            #[cfg(windows)]
                            let hazard = "every captured key and hotkey pressed meanwhile \
                                          waited this long for its callback (the keyboard hook \
                                          itself answers on its own thread)";
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
                                     {focus_ms} + gamepad {pad_ms} + everything else \
                                     {other_ms}, which is mostly key and hotkey dispatch; \
                                     timers {timers_ms}, image results {images_ms}, text \
                                     recognition results {ocr_ms} ({} callback(s)), snapshot results \
                                     {snapshots_ms}, queued module events {queued_ms} ({} event(s), {} \
                                     waiting), initial window report {initial_ms}, stopped modules \
                                     {stops_ms}) — {hazard}",
                                    ocr_n.callbacks,
                                    queued.events,
                                    queued.parked
                                ),
                            );
                        }
                        // A module asked for a re-check (host.window.recheck) after
                        // changing plugin UI itself — dispatch it like an OS focus event.
                        // Its callbacks can stop a module too, settled in this same turn.
                        if shared.recheck_requested.replace(false) {
                            dispatcher.on_focus_change();
                            shared.settle_stops();
                        }
                        shared.log_housekeeping();
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
                            shared.announce(&reload_report_text(&done, &failed, shared.stops.count()));
                        }
                    },
                    move || errors_shared.drain_errors(),
                    // Rows a stop turned off, or that were turned on again after one.
                    {
                        let shared = self.shared.clone();
                        move || std::mem::take(&mut *shared.module_changes.borrow_mut())
                    },
                    // Details…: the module's state, its memory limit and use, where its code runs.
                    {
                        let shared = self.shared.clone();
                        let modules = self.modules.clone();
                        move |idx: usize| module_details(&shared, &modules.borrow(), idx)
                    },
                    // Settings…: the module's settings with the values stored now.
                    {
                        let shared = self.shared.clone();
                        move |idx: usize| {
                            let id = shared.ids.borrow().get(idx).cloned().unwrap_or_default();
                            setting_descs(&shared, &id, idx)
                        }
                    },
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

/// The folder the application lives in — where its log, settings and modules are. On macOS,
/// the folder around the original `.app` when macOS runs a translocated copy of it.
///
/// Exposed because the launcher needs the same answer the host uses; two rules that agreed
/// on Windows and disagreed inside a macOS application bundle is exactly the bug this
/// module exists to prevent.
pub fn app_dir() -> &'static std::path::Path {
    portable::base_dir()
}

/// `automation-platform ocr-bench [options]` (`ocr::bench::USAGE` lists them): what Apple
/// Vision's text recognition costs on this Mac, measured on fixed pictures the executable carries
/// (`ocr::bench`, and `backend/macos/ocr/bench.rs` for the half that runs Vision). No window, no
/// screen capture, no speech, and the log is not opened. Returns the exit code: 0 when it
/// finished, 1 when it could not measure, 2 for a wrong argument or on another platform, where
/// it only says that it runs on macOS.
pub fn ocr_bench(args: &[String]) -> i32 {
    #[cfg(target_os = "macos")]
    {
        backend::ocr_bench(args)
    }
    #[cfg(not(target_os = "macos"))]
    {
        ocr::bench::elsewhere(args)
    }
}

/// Convenience entry: load each directory as a module and run them together.
pub fn run(dirs: &[String]) -> Result<()> {
    // The settings before the log, because the log's own header reports them — and before
    // anything else, because `headless` decides whether there is going to be a window at all.
    // Read straight from the file rather than through the Manager's store: this happens
    // before a Manager exists, and the two read the same file. Read only — no quarantine, no
    // rewrite, no migration — because this process may yet turn out to be a second copy, and
    // a second copy must not write the running copy's settings file under it. The Manager's
    // own load, once this copy is the running one, does the rest.
    let stored = settings::Store::peek();
    appcfg::load(|key| stored.app_flag(key));
    // One running copy per user, decided before the log is opened: a second copy that opened
    // it would write its session header, and possibly rotate the file, in the middle of the
    // running copy's session. Held until the end of this function, so the lock is released
    // only after everything below has shut down — the hotkeys included.
    //
    // Headless runs are exempt, and neither take the lock nor ask for it: CI and the
    // development tools start a headless host beside a running application on purpose, and it
    // has no window to show. A headless copy therefore does not stop a windowed one from
    // starting, and is not stopped by one.
    let instance = if appcfg::headless() {
        None
    } else {
        // Until the claim is decided, anything this process has to log — a panic, above all —
        // is appended as a single line instead of opening a session of its own.
        logging::set_outside(true);
        match instance::claim(gui::instance_lock) {
            instance::Claim::Primary(p) => {
                logging::set_outside(false);
                Some(p)
            }
            // The running copy has been asked for its window and logs that itself.
            instance::Claim::Shown => return Ok(()),
            instance::Claim::GaveUp(why) => {
                logging::append_from_outside("instance", &why.log_line);
                instance::tell(&why);
                return Ok(());
            }
        }
    };
    logging::init();
    // Every folder under `modules/` that will not load, and why. The launcher read that list
    // before the log was open, so the reasons it met went nowhere.
    registry::log_unloadable();
    settings::sweep_leftovers_at_start();
    match &instance {
        Some(p) => p.log_notes(),
        None => logging::line(
            "instance",
            "headless: not guarded against a second copy, and not visible to one",
        ),
    }
    let warmup = backend::warmup_ocr(); // preload the neural OCR model off the hot path
    // Taken out of the manager the moment it exists: the manager is dropped inside the closure,
    // and `Shared` — which holds the service — very likely never is (Lua closures hold it).
    let mut ocr_stop: Option<ocr::service::ShutdownHandle> = None;
    let result = (|| -> Result<()> {
        let mut manager = Manager::new()?;
        ocr_stop = Some(manager.ocr_shutdown());
        // Nothing to load is a legitimate state — it is how somebody installs their first
        // module — and it is also what a translocated bundle whose original was not found, an
        // application moved away from its folder or an empty `modules` directory look like.
        // Those are indistinguishable from the outside and one of them wastes a remote
        // tester's session, so the log names the folder it read and what was there, rather
        // than leaving the reader to infer it from an absence of loading lines. What an absent
        // folder means is said here and not in the header's `modules folder` line, because only
        // here is it known that no folders were given on the command line instead.
        if dirs.is_empty() {
            let modules = registry::modules_dir();
            let state = match portable::look_into(&modules) {
                portable::Folder::Missing => "does not exist. Installed modules are read only \
                                              from beside the application, so one moved away \
                                              from the folder it came in finds none"
                    .to_string(),
                portable::Folder::Unreadable(why) => format!("could not be read ({why})"),
                portable::Folder::Holds(0) => "is empty".to_string(),
                portable::Folder::Holds(n) => format!(
                    "holds {n} folder{} and none of them loads (the lines above say why)",
                    if n == 1 { "" } else { "s" }
                ),
            };
            logging::line(
                "manager",
                &format!(
                    "no modules to load — {} {state}. Every overlay will be absent and nothing \
                     will say so again{}.",
                    modules.display(),
                    if cfg!(target_os = "macos") {
                        "; the `translocated` line at the start of this session says whether \
                         macOS ran a copy of the application"
                    } else {
                        ""
                    }
                ),
            );
        }
        for dir in dirs {
            if let Err(e) = manager.load(dir) {
                manager.report_load_failure(dir, &e);
            }
        }
        let ran = manager.run();
        // What the blocking calls held the event loop for where they could not wait, per module and
        // call, once: the whole session's cost, beside the first line that said it as it happened.
        manager.shared.log_legacy_summary();
        // And what each module took of the event loop, from the guard's watchdog, which stops
        // here; a mouse button a module still holds goes up.
        manager.stop_guard();
        // The window loop has ended, so a second start from here on is told this copy is
        // quitting, and waits for the lock instead of handing its request to a window that
        // will never open. Here, before the manager — hotkeys, hooks, modules — is dropped at
        // the end of this closure; Quit itself says so earlier still, the moment it is chosen
        // (`instance::begin_quit`).
        if let Some(p) = &instance {
            p.stop_serving();
        }
        ran
    })();
    // And again outside, for the early return above: when the Manager could not be made there
    // never was a window loop, and what follows (the capture thread, the OCR warmup join) can
    // take seconds, during which a second start must not be told that a window is coming.
    if let Some(p) = &instance {
        p.stop_serving();
    }
    // `host.ocr.recognize`'s two threads, first: the capture thread may be inside a duplication read,
    // which the next step stops, and the recognise thread inside ONNX Runtime, which the steps
    // after it wait out. Bounded; a thread still busy after it is logged and left.
    if let Some(stop) = ocr_stop {
        stop.shutdown(ocr::policy::SHUTDOWN);
    }
    // The desktop duplication thread, if a module ever started it: out of the graphics driver
    // before the process tears down, for the reason the OCR warmup is joined below. A no-op
    // everywhere else and in every session that never used it.
    backend::shutdown_capture();
    // What the neural recogniser's shadow counted on a Mac, once more, after the recognise
    // thread was stopped above and while the log is still open (nothing elsewhere).
    backend::ocr_exit_report();
    // Join the OCR warmup before returning: otherwise its background thread can be
    // mid native ONNX-Runtime init when the process tears down, racing ort's static
    // cleanup → an access violation that surfaces as the headless / fast-exit
    // "segfault". The thread is bounded (load the model + one dummy inference) — once it has
    // begun: on a Mac it first waits for Vision's warm-up, and that wait is called off here, so
    // that a Vision warm-up which never ended cannot hold the exit.
    backend::stop_ocr_warmup();
    if let Some(h) = warmup {
        let _ = h.join();
    }
    // And for the same reason, the recognitions of that engine a read did not wait for — it
    // runs beside the system one on every small region, and nobody joins it once the system
    // engine has answered. Bounded at half a second; after the warm-up, which they queue
    // behind for the session lock.
    backend::settle_ocr();
    // And last of text recognition: on a Mac the neural recogniser's session and ONNX Runtime's
    // environment released, now that nothing above can still be inside it. Returning from `main`
    // runs `exit()`, and ONNX Runtime 1.22 aborts in its teardown on an environment still alive —
    // a "quit unexpectedly" on every quit. False only when something was still inside ONNX
    // Runtime after half a second: then nothing was released, and the process ends below
    // without that teardown. Nothing elsewhere.
    let released = backend::release_ocr();
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
    // Last: the next copy may start the moment this is released.
    drop(instance);
    if !released {
        // What `main` would print and return, from here: 1 for an error, as `main` makes it.
        if let Err(e) = &result {
            eprintln!("Error: {e:?}");
        }
        backend::end_now(i32::from(result.is_err()));
    }
    result
}

/// Held by `main` for the whole process: dropped when `main` returns, and when a panic unwinds
/// out of it, it releases ONNX Runtime on a Mac before `exit()` runs ([`backend::release_ocr`]) —
/// for the ways out that skip the end of [`run`], which releases it itself; there this finds
/// nothing. When it cannot release it ends the process without `exit()`'s teardown, with 101
/// while a panic unwinds (Rust's own status for a panic out of `main`) and 0 otherwise (`run`,
/// the one way to have loaded it, ended the process itself when it could not). `ocr-bench`
/// releases before `main` calls `std::process::exit`, which drops nothing. Nothing elsewhere.
pub struct ReleaseAtExit;

impl Drop for ReleaseAtExit {
    fn drop(&mut self) {
        if !backend::release_ocr() {
            backend::end_now(if std::thread::panicking() { 101 } else { 0 });
        }
    }
}

/// Deliberately awkward. This rebuilds every module's VM while the user is working in some
/// other application, so it must be impossible to hit by accident; and the modules themselves
/// claim ordinary combinations, so it has to stay out of their way.
///
/// Two chords, because the awkward one cannot be pressed on a Mac. Its Win and Alt are Control
/// and Option there, and Control-Option together is VoiceOver's own modifier: VoiceOver takes
/// those chords in the window server, above anything an application can register or tap, and
/// answers an unassigned one with its error sound. The third Mac session measured exactly
/// that — registered on both launches, never delivered, a beep for every press — on a Mac with
/// the default VoiceOver modifier, after two sessions on another Mac where the same chord had
/// arrived. Command-Shift is what the probe's key uses, and that one arrived four times out of
/// four on the same machine; `Cmd` is the Ctrl role, so the Mac's spec reads the same under
/// the role rule as it did when it was chosen.
const RELOAD_HOTKEY_SPEC: &str =
    if cfg!(target_os = "macos") { RELOAD_HOTKEY_MACOS } else { RELOAD_HOTKEY_WINDOWS };
/// The reload key on Windows and Linux.
const RELOAD_HOTKEY_WINDOWS: &str = "Ctrl+Shift+Win+Alt+F5";
/// The reload key on macOS: Command+Shift+F5.
const RELOAD_HOTKEY_MACOS: &str = "Cmd+Shift+F5";

/// The reload key as the log names it: the chord it is on this platform.
fn reload_hotkey_shown() -> String {
    backend::normalize_spec_for(backend::KeyOs::CURRENT, RELOAD_HOTKEY_SPEC)
        .unwrap_or_else(|| RELOAD_HOTKEY_SPEC.to_string())
}

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
/// are counted because eleven module names is not feedback, it is a recital. Modules the guard
/// stopped are rebuilt too and stay off — a reload is no way around a stop — and the report counts
/// them, so a module that does not answer afterwards is not a mystery.
fn reload_report_text(done: &[String], failed: &[String], stopped: usize) -> String {
    let report = match (done.len(), failed.len()) {
        (0, 0) => "No modules to reload".to_string(),
        (n, 0) => format!("{n} module{} reloaded", if n == 1 { "" } else { "s" }),
        (0, _) => format!("Reload failed: {}", failed.join(", ")),
        (n, _) => format!("{n} reloaded, {} failed: {}", failed.len(), failed.join(", ")),
    };
    match stopped {
        0 => report,
        1 => format!("{report}; 1 stopped module stays off"),
        n => format!("{report}; {n} stopped modules stay off"),
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

    /// Answers `onTrigger { initial = true }`: reports the window in front to the triggers that
    /// asked since the last tick — registered at load, on enable, or later from a timer. The
    /// rules are [`drain_initial`]'s; this only hands it the host's parts.
    fn dispatch_initial(&self) {
        let pending = std::mem::take(&mut *self.shared.initial_pending.borrow_mut());
        if pending.is_empty() {
            return; // every tick but a handful
        }
        // A trigger reporting the window in front, as an activation does.
        let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);
        let shared = self.shared;
        drain_initial(
            pending,
            |idx| self.enabled(idx),
            |idx| self.modules.get(idx).map(|m| &m.lua),
            || {
                let t = Instant::now();
                let win = shared.backend.active_window();
                slow_observation("window.active", "initial report", t);
                win
            },
            || shared.bump_input_epoch(),
            |idx, lua, win, reprime| {
                mailbox::deliver(shared, idx, lua, Event::Initial { win: win.cloned().map(Box::new), reprime });
            },
        );
    }
}

/// The report `onTrigger { initial = true }` asked for, for every module queued in `pending`.
///
/// On the tick, not inside `onTrigger`, and the same at every moment a report is owed: the
/// module's load has finished by then, so a callback can use what the file assigns after the
/// registration. A disabled module's request is dropped rather than kept: enabling it asks
/// again, with every `initial` trigger primed afresh.
///
/// - **The foreground is asked ONCE**, through `active_window`, for every module owed a report
///   together — and not at all when none of them has an `initial` trigger waiting
///   (`_wantsInitial`): enabling a module that never asked for a report costs no foreground
///   query, which on macOS is an accessibility call into whichever application is in front,
///   at that moment usually our own manager.
/// - **A window without a title is no window**, as it is to an activation: neither platform
///   hands an untitled foreground window to a trigger, so the report does not either. The
///   report is still used up.
/// - **Delivered through the host's own handle** on each window table (see
///   `install_window_prelude`), so a module relying on its runtime's `window` capability is
///   reported to as its activations are — to each module's mailbox (`deliver`), as an
///   `Event::Initial`, which runs at once while the module is free.
/// - **The input epoch turns ONCE, now**, for the whole report, when any module is owed it and a
///   titled window is in front — before any module hears it, as an activation turns it once
///   before any module, and never again when a module that was busy hears it later: a second turn
///   between two modules would make stale what the first module's callback just cached against
///   it. It turns even when no trigger of theirs matches the window.
/// - The duplication a module reads through is opened before its first matching callback, by the
///   prelude, through the hook an activation hands it too (`capture_source::prewarm_hook`).
fn drain_initial<'m>(
    pending: Vec<(usize, bool)>,
    enabled: impl Fn(usize) -> bool,
    vm: impl Fn(usize) -> Option<&'m Lua>,
    active_window: impl FnOnce() -> Option<WinInfo>,
    bump_input_epoch: impl FnOnce(),
    mut deliver: impl FnMut(usize, &'m Lua, Option<&WinInfo>, bool),
) {
    let due: Vec<(usize, bool, &Lua)> = initial_due(pending, enabled)
        .into_iter()
        .filter_map(|(idx, reprime)| vm(idx).map(|lua| (idx, reprime, lua)))
        .filter(|(_, reprime, lua)| wants_initial(lua, *reprime))
        .collect();
    if due.is_empty() {
        return;
    }
    let win = active_window().filter(|w| !w.title.is_empty());
    if win.is_some() {
        bump_input_epoch();
    }
    for (idx, reprime, lua) in due {
        deliver(idx, lua, win.as_ref(), reprime);
    }
}

/// Adds module `idx` to the modules owed a report of the window in front, once: a second
/// request before the tick only widens the first (`reprime` sticks once any request asked for
/// it), so a module that registers ten `initial` triggers is asked about the foreground once.
fn queue_initial(pending: &mut Vec<(usize, bool)>, idx: usize, reprime: bool) {
    match pending.iter_mut().find(|(i, _)| *i == idx) {
        Some(entry) => entry.1 |= reprime,
        None => pending.push((idx, reprime)),
    }
}

/// An activation was just dispatched into every module `enabled` answers true for, and to
/// each of them it IS the report of what is in front: the prelude has un-primed their
/// triggers. A re-enable's queued `reprime` must not prime them again for the same window on
/// this tick, or the window that just came forward is reported twice.
fn initial_reported_by_activation(pending: &mut [(usize, bool)], enabled: impl Fn(usize) -> bool) {
    for entry in pending.iter_mut().filter(|(idx, _)| enabled(*idx)) {
        entry.1 = false;
    }
}

/// The queued requests the tick answers: those of enabled modules, in the order they asked.
fn initial_due(pending: Vec<(usize, bool)>, enabled: impl Fn(usize) -> bool) -> Vec<(usize, bool)> {
    pending.into_iter().filter(|(idx, _)| enabled(*idx)).collect()
}

/// `host.window._requestInitial` for module `idx`: queues it, without re-arming, on the queue
/// `get` finds in `holder` — the host's `Shared`, or a bare queue in the tests.
fn initial_request_fn<S: 'static>(
    lua: &Lua,
    idx: usize,
    holder: Rc<S>,
    get: fn(&S) -> &RefCell<Vec<(usize, bool)>>,
) -> mlua::Result<Function> {
    lua.create_function(move |_, ()| {
        queue_initial(&mut get(&holder).borrow_mut(), idx, false);
        Ok(())
    })
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
    ///
    /// The two window drains after them are the GUI tick's too, in the same order: the report
    /// `onTrigger { initial = true }` asked for, then a `host.window.recheck()` round. The
    /// recheck used to be missing here, so a module that asked for one headless never got it.
    fn on_tick(&mut self) {
        self.shared.speech.pump();
        self.shared.fire_due_timers();
        self.shared.fire_pad_tick();
        self.shared.fire_image_results();
        self.shared.fire_ocr_results();
        self.shared.fire_snapshot_results();
        self.shared.run_queued_events();
        self.dispatch_initial();
        self.shared.settle_stops();
        if self.shared.recheck_requested.replace(false) {
            self.on_focus_change();
            self.shared.settle_stops();
        }
        self.shared.log_housekeeping();
    }

    /// `RegisterHotKey` presses made during the stall a stop ended, dropped as they reached the
    /// queue after it (`Backend::drop_queued_input`): one `[keys]` line, by their combinations.
    fn on_stall_hotkeys_dropped(&mut self, ids: Vec<i32>) {
        let ids = self.shared.answer_reload_key(ids);
        if ids.is_empty() {
            return;
        }
        let names: Vec<String> = {
            let hotkeys = self.shared.hotkeys.borrow();
            ids.iter().map(|id| hotkeys.get(id).map_or_else(|| format!("hotkey {id}"), |r| r.spec.clone())).collect()
        };
        logging::line(
            "keys",
            &format!(
                "{} hotkey press(es) made during the stall that ended in the stop above reached the application after \
                 it, and were dropped: {}",
                ids.len(),
                stops::counted(&names).join(", ")
            ),
        );
    }

    /// A sleep, a lock, the session going and coming back, a display change: a `[system]` line
    /// each — the log was blind to all of them, and a report after a night could only be placed
    /// by arithmetic on the observation counts — with how long what it ends lasted (a display
    /// change that keeps coming is counted, see `system_events::Since::plan`); both epochs
    /// turned over, because an answer cached before a display change or a wake may describe a
    /// screen that is gone; speech's turn (on macOS a VoiceOver path parked after a refusal is
    /// tried again at once after a wake rather than at its next look); and what the plan asks
    /// for — the window in front reported again as an activation, the focus re-checked, the
    /// display lines written again when they changed, the audio output opened afresh at the
    /// next sound. The backend resets its own subsystems for them (see `system_events`).
    fn on_system(&mut self, events: Vec<system_events::Stamped>) {
        let plan = self.shared.system_since.borrow_mut().plan(&events, Instant::now(), SystemTime::now());
        for line in &plan.lines {
            logging::line("system", line);
        }
        // Everything cached before is asked again: pixels cached against the input epoch
        // included, which a resolution or scale change makes wrong.
        self.shared.bump_input_epoch();
        for s in &events {
            self.shared.speech.on_system(s.event);
        }
        if plan.env {
            // Written again only when they changed: a display that keeps dropping its link sends
            // a change every few seconds with nothing changed.
            let now = self.shared.backend.display_environment();
            let mut said = self.shared.display_env_said.borrow_mut();
            if *said != now {
                logging::report("env", &now);
                *said = now;
            }
        }
        if plan.reopen_audio {
            // Dropped here and opened on the next `host.sound.play`: nothing is held open for
            // a sound that may never come.
            self.shared.audio.borrow_mut().take();
        }
        if plan.report_front {
            let t = Instant::now();
            let win = self.shared.backend.active_window();
            slow_observation("window.active", "after a system event", t);
            if let Some(win) = win.filter(|w| !w.title.is_empty()) {
                self.on_window_activate(win);
            }
        }
        if plan.recheck {
            self.on_focus_change();
        }
    }

    fn on_gamepad(&mut self, events: Vec<backend::gamepad::PadEvent>) {
        // Somebody is waiting for what this dispatch does: a read or a timer it asks for goes
        // in the interactive lane (ocr/types.rs, `enter_priority`).
        let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);
        self.shared.dispatch_gamepad(events);
    }

    fn on_hotkey(&mut self, id: i32, seq: u32) {
        // Somebody is waiting for what this dispatch does: a read or a timer it asks for goes
        // in the interactive lane (ocr/types.rs, `enter_priority`).
        let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);
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
            map.get(&id)
                .filter(|reg| self.enabled(reg.module_idx))
                .map(|reg| (reg.module_idx, reg.lua.clone(), reg.binding, reg.spec.clone()))
        };
        if let Some((owner, lua, binding, spec)) = found {
            // The window in front now, noted only for a module that is busy: if its registration
            // is made again while the press waits, the new one takes it only when the module's
            // scope is this window or everywhere (`hotkey_target`). The window the hook or the tap
            // compares keys with (`key_front`), never `resolve_key_scope`: on a Mac that asks the
            // frontmost application, on the tap's thread, and notes a pin nobody made.
            let front = mailbox::busy(self.shared, owner).then(|| self.shared.backend.key_front());
            mailbox::deliver(self.shared, owner, &lua, Event::Hotkey { id, owner, front, seq, binding, spec });
        }
    }

    fn on_key(&mut self, vk: u32, mods: u8, owner: u32, repeat: bool, pressed: backend::Pressed) {
        // Somebody is waiting for what this dispatch does: a read or a timer it asks for goes
        // in the interactive lane (ocr/types.rs, `enter_priority`).
        let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);
        self.shared.bump_epoch();
        let owner = owner as usize;
        // The hook only calls this for keys it took for a module, so the presence of this line
        // IS the answer to "did the overlay swallow that keystroke, or did the application
        // simply do nothing with it" — the two are indistinguishable from the outside, and for
        // a user who cannot see the screen they are indistinguishable from each other twice
        // over. Behind trace, for the reasons written at note_captured_line.
        if appcfg::trace() {
            let id = self.shared.ids.borrow().get(owner).cloned().unwrap_or_else(|| "?".to_string());
            // How long the key waited for this thread since the hook or the tap queued it.
            let waited = pressed
                .queued
                .map(|q| format!(", after {:.1} ms in the queue", q.elapsed().as_secs_f64() * 1000.0))
                .unwrap_or_default();
            logging::line(
                "keys",
                &format!(
                    "dispatch vk 0x{vk:02X}/m{mods} to {id}, pressed in window {:#x}{}{waited}",
                    pressed.front,
                    if repeat { ", a repeat" } else { "" }
                ),
            );
        }
        // To the module the hook took it for, and to that module's own capture of it — never to
        // another module's — through its mailbox (captures.rs, `arrive`).
        captures::arrive(self.shared, vk, mods, owner, repeat, pressed);
    }

    fn on_window_activate(&mut self, win: WinInfo) {
        // Somebody is waiting for what this dispatch does: a read or a timer it asks for goes
        // in the interactive lane (ocr/types.rs, `enter_priority`).
        let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);
        let started = Instant::now();
        // A different window in front is a different screen — this counts as the screen
        // having changed, not merely the world (see bump_input_epoch). Once, now: a module that
        // is busy hears it later without a second turn.
        self.shared.bump_input_epoch();
        let mut queued: Vec<usize> = Vec::new();
        for (idx, m) in self.modules.iter().enumerate() {
            if !self.enabled(idx) {
                continue;
            }
            // Only a VM with triggers, asked through the host's own handle on the window table,
            // not the module's `host` (see `install_window_prelude`); the number of its newest
            // trigger goes with the event, so one made after it arrived does not get it.
            let upto = trigger_seq(&m.lua);
            if upto == 0 {
                continue;
            }
            let ev = Event::Activate { win: Box::new(win.clone()), upto };
            if mailbox::deliver(self.shared, idx, &m.lua, ev) == mailbox::Delivered::Queued {
                queued.push(idx);
            }
        }
        // A module that is busy has this activation waiting in its mailbox: that is its report of
        // the window in front, so a report it asked for on this tick is dropped — its triggers are
        // still primed until the activation runs, and the input epoch must not turn twice.
        self.shared.initial_pending.borrow_mut().retain(|(idx, _)| !queued.contains(idx));
        // To every module it reached, this activation was the report of what is in front, so a
        // re-enable still queued for this tick must not re-arm the triggers it just un-primed.
        initial_reported_by_activation(&mut self.shared.initial_pending.borrow_mut(), |idx| {
            self.enabled(idx)
        });
        let mut c = self.shared.ev_counts.get();
        c.0 += 1;
        c.1 += started.elapsed().as_millis();
        self.shared.ev_counts.set(c);
    }

    fn on_focus_change(&mut self) {
        // Somebody is waiting for what this dispatch does: a read or a timer it asks for goes
        // in the interactive lane (ocr/types.rs, `enter_priority`).
        let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);
        let started = Instant::now();
        self.shared.bump_epoch();
        for (idx, m) in self.modules.iter().enumerate() {
            if !self.enabled(idx) || !window_has_triggers(&m.lua) {
                continue;
            }
            mailbox::deliver(self.shared, idx, &m.lua, Event::Focus);
        }
        let mut c = self.shared.ev_counts.get();
        c.2 += started.elapsed().as_millis();
        self.shared.ev_counts.set(c);
    }
}

/// Whether a hotkey holds the combination `(vk, mask)` at the system now: a module's registration
/// that holds it (`live`, enabled module or not — the system matches what is registered), or the
/// application's own reload key, counted as held whether the system granted it or not. What a key
/// passed on to the program in front would be taken by instead (`captures::pass_on`). A
/// registration whose combination only the system read (`binding` `None`) cannot be compared, and
/// holds nothing here. The host's `KeyHost` and the key-scope tests' holder both ask this.
pub(crate) fn hotkey_holds(regs: &HashMap<i32, HotkeyReg>, vk: u32, mask: u8) -> bool {
    backend::key_spec(RELOAD_HOTKEY_SPEC) == Some((vk, mask)) || regs.values().any(|r| r.live && r.binding == Some((vk, mask)))
}

/// The hotkey registration a press of hotkey `id`, for module `owner`, goes to as it runs: `id`
/// while it is registered for that module, holds its combination at the OS (`live`) and the
/// module is on; and otherwise — the registration released and made again meanwhile, by an
/// overlay that went and came back while the press waited in its busy module's mailbox — the
/// module's own registration that holds the same combination now, while the module's key scope
/// is everywhere or `front`, the window in front when the press arrived. Never another module's:
/// one that holds the combination now makes it `TakenOver`.
fn hotkey_target(
    regs: &HashMap<i32, HotkeyReg>,
    enabled: impl Fn(usize) -> bool,
    scope: isize,
    owner: usize,
    id: i32,
    binding: Option<(u32, u8)>,
    front: Option<isize>,
) -> captures::Target<i32> {
    use captures::{Gone, Target};
    if !enabled(owner) {
        return Target::Gone(Gone::Disabled);
    }
    if regs.get(&id).is_some_and(|r| r.module_idx == owner && r.live) {
        return Target::Same(id);
    }
    let Some(b) = binding else { return Target::Gone(Gone::Released) };
    let holder = regs.iter().filter(|(_, r)| r.live && r.binding == Some(b)).min_by_key(|(i, _)| **i);
    match holder {
        Some((new, r)) if r.module_idx == owner => {
            if scope == 0 || Some(scope) == front {
                Target::Moved(*new)
            } else {
                Target::Gone(Gone::ScopeMoved)
            }
        }
        Some(_) => Target::Gone(Gone::TakenOver),
        None => Target::Gone(Gone::Released),
    }
}

/// A hotkey as it runs (`Event::Hotkey`): its registration in `regs`, or the module's new one of
/// the same combination ([`hotkey_target`]) — said, as a captured key's is — or nothing; a press
/// that waited while its module was busy, for a registration released since and not made again,
/// goes where it would have gone had the module never registered it, as a captured key does: to
/// the next module that captures the combination in the window in front when it arrived, as that
/// module's key, or else to the program in front — while that window still is, and no key-down has
/// reached the program since it came in (`seq`). The host's own `open` and the key-scope tests'
/// holder both run this, so the tests run the host's rule and not a copy of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_hotkey_in<H: captures::KeyHost + mailbox::MailHost>(
    h: &H,
    regs: &RefCell<HashMap<i32, HotkeyReg>>,
    id: i32,
    owner: usize,
    front: Option<isize>,
    seq: u32,
    binding: Option<(u32, u8)>,
    spec: &str,
) -> Opened {
    let target = hotkey_target(
        &regs.borrow(),
        |i| h.key_module_enabled(i),
        h.captures().scope_of(owner),
        owner,
        id,
        binding,
        front,
    );
    // `front` is noted exactly for a press its busy module queued (`on_hotkey`); one with none
    // ran at once, and its line does not say the module was busy.
    let queued = front.is_some();
    let id = match target {
        captures::Target::Same(id) => id,
        captures::Target::Moved(new) => {
            if h.captures().say_moved(owner, Instant::now()) {
                logging::line("keys", &captures::moved_line(&h.key_module_id(owner), spec, queued));
            }
            new
        }
        // A press that waited in its busy module's mailbox for a registration the module released
        // since, and did not make again: kept at the press for nothing, so it goes now where it
        // would have gone — to the next module's capture of the combination in the window in front
        // when it arrived, or to the program in front while that window still is
        // (`captures::pass_on_released`). No physical key: the system matched the press. Not a
        // repeat, and no module let go of it before this one.
        captures::Target::Gone(captures::Gone::Released) if queued => {
            let pressed = backend::Pressed { front: front.unwrap_or(0), seq, phys: None, queued: None };
            captures::pass_on_released(h, owner, spec, binding, (false, pressed), Vec::new());
            return Opened::Gone;
        }
        captures::Target::Gone(gone) => {
            if gone != captures::Gone::Disabled && h.captures().say_dropped(owner, Instant::now()) {
                let line = if queued {
                    captures::busy_dropped_line(&h.key_module_id(owner), spec, gone)
                } else {
                    captures::hotkey_dropped_line(&h.key_module_id(owner), spec, gone)
                };
                logging::line("keys", &line);
            }
            return Opened::Gone;
        }
    };
    let f = regs.borrow().get(&id).and_then(|r| r.lua.registry_value::<Function>(&r.cb).ok());
    match f {
        Some(f) => Opened::Run { f, args: mlua::MultiValue::new(), ctx: task::Ctx::new("hotkey", owner) },
        None => Opened::Gone,
    }
}

impl Shared {
    /// A hotkey as it runs (`Event::Hotkey`), by [`open_hotkey_in`].
    fn open_hotkey(&self, id: i32, owner: usize, front: Option<isize>, seq: u32, binding: Option<(u32, u8)>, spec: &str) -> Opened {
        open_hotkey_in(self, &self.hotkeys, id, owner, front, seq, binding, spec)
    }

    /// The tick's phase for the events that waited in busy modules' mailboxes
    /// (`mailbox::run_queued`). Said when the budget ended it before every event ran — at most once
    /// every ten seconds, with how often since (`Mailboxes::say_budget`).
    fn run_queued_events(&self) -> mailbox::Phase {
        let phase = mailbox::run_queued(self);
        if phase.budget {
            if let Some(unsaid) = self.mail.say_budget(Instant::now()) {
                logging::line("pump", &mailbox::budget_line(phase.events, unsaid));
            }
        }
        phase
    }

    /// A window event's call as it runs, or nothing for a VM without triggers; what could not be
    /// built is reported as the trigger's error.
    fn window_opened(&self, idx: usize, what: &'static str, call: mlua::Result<Option<(Function, mlua::MultiValue)>>) -> Opened {
        match call {
            Ok(Some((f, args))) => Opened::Run { f, args, ctx: task::Ctx::new(what, idx) },
            Ok(None) => Opened::Gone,
            Err(e) => {
                self.report_callback_error(idx, what, &e.to_string());
                Opened::Gone
            }
        }
    }
}

/// Every event of every module, as it runs and as it is dropped: one `match` each, calling the
/// site's own helper (`mailbox.rs`).
impl mailbox::MailHost for Shared {
    fn mail(&self) -> &mailbox::Mailboxes {
        &self.mail
    }

    fn module_stopped(&self, idx: usize) -> bool {
        self.stops.is_stopped(idx)
    }

    fn open(&self, idx: usize, lua: &Lua, ev: Event) -> Opened {
        match ev {
            Event::Hotkey { id, owner, front, seq, binding, spec } => self.open_hotkey(id, owner, front, seq, binding, &spec),
            Event::Key { owner, token, vk, mods, repeat, pressed, let_go } => {
                captures::open_key(self, owner, token, vk, mods, repeat, pressed, let_go)
            }
            Event::Pad { token, event, delivery, .. } => self.open_pad(token, &event, &delivery),
            Event::Activate { win, upto } => self.window_opened(idx, "window trigger", activate_call(lua, &win, Some(upto))),
            Event::Initial { win, reprime } => {
                let call = capture_source::prewarm_hook(lua).and_then(|before| initial_call(lua, win.as_deref(), reprime, before));
                self.window_opened(idx, "window trigger", call)
            }
            Event::Focus => self.window_opened(idx, "focus change", focus_call(lua)),
            Event::After { token } => match self.timers.take_queued(token) {
                Some(f) => Opened::Run { f, args: mlua::MultiValue::new(), ctx: task::Ctx::new("timer", idx) },
                None => Opened::Gone,
            },
            Event::Every { token } => match self.timers.every_fn(token) {
                Some(f) => Opened::Run { f, args: mlua::MultiValue::new(), ctx: task::Ctx::new("timer", idx) },
                None => Opened::Gone,
            },
            Event::Read(r) => ocr::lua::open_read(self, *r),
            Event::Image { p, outcome, ended } => self.open_image(*p, outcome, ended),
            Event::Snapshot { p, answer } => self.open_snapshot(*p, answer),
            Event::Setting { setting_of, reg, key, new, old } => self.open_setting(setting_of, reg, &key, &new, old.as_ref()),
            #[cfg(test)]
            Event::Call { f, args, what, .. } => mailbox::open_call(idx, f, args, what),
        }
    }

    fn discard(&self, _idx: usize, ev: Event, why: mailbox::Why) {
        match ev {
            Event::After { token } => self.timers.drop_queued(token),
            Event::Read(r) => ocr::lua::discard_read(self, *r),
            Event::Image { p, ended, .. } => self.discard_image(*p, ended, why == mailbox::Why::Disabled),
            Event::Snapshot { p, .. } => self.discard_snapshot(*p),
            _ => {}
        }
    }
}

/// Which `host.<key>` a manifest's capability name unlocks.
///
/// Everything not in here is free: `os`, `require`, `tryRequire`, `include`, `epoch`, `now`,
/// `inputEpoch`, `match`, `json`. A clock, a counter, a platform name, a way to reach a declared
/// dependency and a parser of strings the module already holds are not worth asking permission
/// for, and gating them would mean every manifest names them, which is the same as naming none.
///
/// `config` is the second name of the settings table, so it answers to the same capability
/// rather than to one of its own — nothing declares `"config"` and nothing should have to.
///
/// A few members of a gated namespace are free as well; see [`FREE_MEMBERS`].
const GATED: &[(&str, &str)] = &[
    ("window", "window"),
    ("screen", "screen"),
    ("ocr", "ocr"),
    ("element", "element"),
    ("input", "input"),
    ("keys", "keys"),
    ("hotkey", "hotkey"),
    ("gamepad", "gamepad"),
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

/// Members of a gated namespace that need no capability.
///
/// `host.keys` is gated because it CAPTURES keystrokes, which is what a user reviewing a
/// manifest should be told about. Putting a key into words is not that, and a module that only
/// registers a hotkey — daw-hosts, the probe, an example — has to be able to say its own key in
/// the platform's words without asking for the right to swallow everybody's.
///
/// `normalize` and `describe` read nothing but the string they are handed. `check` reads one
/// thing more: for Ctrl+Alt on Windows and Option on a Mac it asks the keyboard layout in use
/// what that chord types, so a module that declared nothing can learn which character that is
/// — which says something about the layout, and nothing about what anybody typed.
const FREE_MEMBERS: &[(&str, &[&str])] = &[("keys", &["normalize", "describe", "check"])];

/// For each namespace in [`FREE_MEMBERS`] that `denied` refuses: a table of just its free
/// members, taken from `source`, whose every other member raises the refusal `host.<key>`
/// would have. Built once per view, not per access.
fn free_subsets(
    lua: &Lua,
    source: &Table,
    denied: impl Fn(&str) -> bool,
    owner: &str,
    note: &'static str,
) -> Result<HashMap<String, Table>> {
    let mut out = HashMap::new();
    for (ns, members) in FREE_MEMBERS {
        if !denied(ns) {
            continue;
        }
        let Ok(mlua::Value::Table(full)) = source.raw_get::<mlua::Value>(*ns) else { continue };
        let subset = lua.create_table()?;
        for m in members.iter() {
            subset.raw_set(*m, full.raw_get::<mlua::Value>(*m)?)?;
        }
        let (owner, name) = (owner.to_string(), (*ns).to_string());
        let mt = lua.create_table()?;
        mt.set(
            "__index",
            lua.create_function(move |_, (_t, _k): (Table, mlua::Value)| -> mlua::Result<mlua::Value> {
                Err(undeclared(&owner, &name, note))
            })?,
        )?;
        subset.set_metatable(Some(mt))?;
        out.insert((*ns).to_string(), subset);
    }
    Ok(out)
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
    let free = free_subsets(lua, full, |ns| denied.contains(ns), id, "")?;
    let owner = id.to_string();
    let source = full.clone();
    let view = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set(
        "__index",
        lua.create_function(move |_, (_t, key): (Table, String)| -> mlua::Result<mlua::Value> {
            if denied.contains(&key) {
                if let Some(subset) = free.get(&key) {
                    return Ok(mlua::Value::Table(subset.clone()));
                }
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

/// Where `host.now()` counts from, and the `time` of every gamepad event with it.
///
/// One origin for the whole process, fixed in `Manager::new`. It used to be taken in each
/// `install_host_api` call, which is once per module VM: two modules' clocks disagreed by
/// however far apart their VMs were built, while the reference promised "since the application
/// started". (A code dependency always shared its owner's clock — `now` is not one of the
/// identity facilities `build_dep_host` copies, so it falls through to the owner's table.) What
/// makes it matter now is a pad event's `time`, which a module compares with `host.now()`, and
/// that only means something if both count from the same moment.
static CLOCK_ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

fn clock_origin() -> Instant {
    *CLOCK_ORIGIN.get_or_init(Instant::now)
}

/// `host.keys.capture`, `release`, `releaseAll`, `scope`, `menuOpen` and `passedThrough` for module
/// `idx` — the module whose VM it is: `keys` is no identity facility, so a code dependency's
/// `host.keys` falls through to the owner's table (`build_dep_host`), and what Kontakt's VM runs
/// of the overlay runtime captures and scopes for Kontakt. Over the captures `h` keeps: the host's
/// `Shared`, or a test's holder (key_scope_tests.rs). In lib.rs with every other binding, so the
/// reference's checks see them.
fn install_key_captures<H: captures::KeyHost>(lua: &Lua, keys: &Table, idx: usize, h: Rc<H>) -> mlua::Result<()> {
    let sh = h.clone();
    keys.set(
        "capture",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            host_call!("host.keys.capture");
            // The parser's own error, which names the part it could not read.
            let (vk, mask) = backend::parse_key_spec(&spec).map_err(mlua::Error::external)?;
            // No cross-module conflict surfacing here: captured keys are routinely
            // shared by window-scoped overlays (each active only while its own
            // window is focused), so a duplicate is usually legitimate, not a clash.
            // The hook takes a key for the earliest capture whose module is scoped to the
            // window in front, and refresh_captured hands it enabled modules' only;
            // conflict dialogs are for process-wide hotkeys only.
            //
            // Within a module we keep ONE entry per (vk, mask): the latest capturer
            // wins (drop any prior same-module entry, then push). Each entry carries a
            // unique token, returned to the caller so `release` can target THIS exact
            // registration and not another overlay's re-capture of the same key.
            let key = lua.create_registry_value(cb)?;
            let token = sh.captures().capture(idx, vk, mask, lua.clone(), key);
            sh.refresh_captured();
            sh.key_backend().watch_keys().map_err(mlua::Error::external)?;
            Ok(token)
        })?,
    )?;
    let sh = h.clone();
    keys.set(
        "release",
        // Releases the exact registration returned by `capture` (its token). Keying on
        // the token, not (vk, mask, module_idx), means one overlay deactivating cannot
        // drop a key another overlay in the same module has since re-captured. An
        // unknown/stale token (already superseded by a later capture) is a harmless
        // no-op.
        lua.create_function(move |_, token: i64| {
            host_call!("host.keys.release");
            if sh.captures().release(token) {
                sh.refresh_captured();
            }
            Ok(())
        })?,
    )?;
    let sh = h.clone();
    keys.set(
        "releaseAll",
        lua.create_function(move |_, ()| {
            host_call!("host.keys.releaseAll");
            sh.captures().release_all(idx);
            sh.refresh_captured();
            Ok(())
        })?,
    )?;
    // host.keys.scope(toForeground) — pins THIS module's captures to the window in front, or
    // makes them global again. Each module's own: one module's scope(false) no longer unpins
    // another's (captures.rs).
    let sh = h.clone();
    keys.set(
        "scope",
        lua.create_function(move |_, to_foreground: bool| {
            host_call!("host.keys.scope");
            let window = if to_foreground { sh.key_backend().resolve_key_scope() } else { 0 };
            if sh.captures().set_scope(idx, window) {
                sh.refresh_key_owners();
            }
            Ok(())
        })?,
    )?;
    // host.keys.menuOpen(open) — while a plugin's own (Qt/UIA) menu is open, let
    // captured nav keys (Tab/Enter) pass through to it instead of the overlay: this module's
    // flag, which counts for the window its captures are scoped to. The overlay runtime writes
    // it on every tick of its menu timer, so an unchanged flag is not handed over again.
    let sh = h.clone();
    keys.set(
        "menuOpen",
        lua.create_function(move |_, open: bool| {
            host_call!("host.keys.menuOpen");
            if sh.captures().set_menu(idx, open) {
                sh.refresh_key_owners();
            }
            Ok(())
        })?,
    )?;
    // host.keys.passedThrough() -> { {vk, mask, key} } — the captured keys the hook let
    // through to the application because a menu was open, since the last call, as filed for
    // this module. Drained on read. The overlay runtime asks on its menu tick and logs what
    // reached an open menu, and whether the menu was still there after an Escape.
    let sh = h;
    keys.set(
        "passedThrough",
        lua.create_function(move |lua, ()| {
            host_call!("host.keys.passedThrough");
            let t = lua.create_table()?;
            for (vk, mask) in sh.key_backend().take_menu_pass_through(idx as u32) {
                let k = lua.create_table()?;
                k.set("vk", vk)?;
                k.set("mask", mask)?;
                k.set("key", backend::vk_name(vk).unwrap_or_else(|| format!("vk {vk:#04x}")))?;
                t.push(k)?;
            }
            Ok(t)
        })?,
    )?;
    Ok(())
}

impl captures::KeyHost for Shared {
    fn captures(&self) -> &captures::Captures {
        &self.captures
    }
    fn key_backend(&self) -> &dyn Backend {
        &*self.backend
    }
    fn key_module_enabled(&self, idx: usize) -> bool {
        self.module_on(idx)
    }
    fn key_module_id(&self, idx: usize) -> String {
        self.ids.borrow().get(idx).cloned().unwrap_or_else(|| "?".to_string())
    }
    fn key_hotkey_holds(&self, vk: u32, mask: u8) -> bool {
        hotkey_holds(&self.hotkeys.borrow(), vk, mask)
    }
    fn refresh_captured(&self) {
        Shared::refresh_captured(self);
    }
    fn refresh_key_owners(&self) {
        Shared::refresh_key_owners(self);
    }
}

fn install_host_api(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> Result<Table> {
    let host = lua.create_table()?;

    // host.log.info(msg)
    let log = lua.create_table()?;
    log.set(
        "info",
        lua.create_function(|_, msg: String| {
            host_call!("host.log.info");
            logging::line("module", &msg);
            Ok(())
        })?,
    )?;
    host.set("log", log)?;

    // host.json.decode(text) / encode(value, opts?) / array(t?) — see json.rs. Ungated, like
    // host.os: they read nothing but the value they are handed.
    let json = lua.create_table()?;
    json.set(
        "decode",
        lua.create_function(|lua, text: mlua::String| {
            host_call!("host.json.decode");
            json::decode(lua, text)
        })?,
    )?;
    json.set(
        "encode",
        lua.create_function(|lua, args: (mlua::Value, mlua::Value)| {
            host_call!("host.json.encode");
            json::encode(lua, args)
        })?,
    )?;
    json.set(
        "array",
        lua.create_function(|lua, t: mlua::Value| {
            host_call!("host.json.array");
            json::array(lua, t)
        })?,
    )?;
    host.set("json", json)?;

    // host.require(id) — access a declared dependency. A `code_module` dependency
    // is evaluated *inside this VM* (top-down loading), so this returns its module
    // object with functions intact (looked up in the per-VM `__module_exports`
    // registry). Otherwise it falls back to the legacy cross-VM path: the
    // dependency's serialized data export (functions can't cross VMs that way).
    let sh = shared.clone();
    host.set(
        "require",
        lua.create_function(move |lua, id: String| -> mlua::Result<mlua::Value> {
            host_call!("host.require");
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
            host_call!("host.tryRequire");
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
            host_call!("host.speech.output");
            // A missing `interrupt` — no table, `{}`, `{ interrupt = nil }` — interrupts, as the
            // page promises. It used to be `get::<bool>(…).unwrap_or(true)`, and mlua reads nil
            // as `false`, so the default only applied when `opts` itself was left out: `{}`
            // queued, and a runtime passing `{ interrupt = pack.interrupt }` lagged behind fast
            // navigation line by line. See `opt_bool`.
            let interrupt = speech_interrupt(opts.as_ref())?;
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
            host_call!("host.speech.engines");
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
                host_call!("host.speech.use");
                Ok(sh.speech.use_engine(idx, id.as_deref()))
            })?,
        )?;
        let sh = shared.clone();
        speech.set(
            "engine",
            lua.create_function(move |_, ()| { host_call!("host.speech.engine"); Ok(sh.speech.chosen_engine(idx)) })?,
        )?;
    }

    host.set("speech", speech)?;

    // host.hotkey.register(spec, cb) -> id ; host.hotkey.unregister(id)
    let hk = lua.create_table()?;
    let sh = shared.clone();
    hk.set(
        "register",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            host_call!("host.hotkey.register");
            // What the operating system can never hold is refused here and now, loudly, rather
            // than recorded as a claim: a tap, a key the platform has no code for, and a
            // combination the system keeps for itself. Command+Q on a Mac would REGISTER, and
            // then take quitting away from every application for as long as the module held
            // it; the other two would fail at the claim, where the only word the user got was
            // a dialog blaming another application. VoiceOver's layer and a character the
            // layout types with the key are said by `host.keys.check`, not refused. Nor is a
            // letter no key of the current macOS layout types: the user can switch layouts at
            // any moment, and the macOS backend parks that hotkey until a layout types it.
            //
            // Otherwise recorded, reported and handed to the OS in its canonical spelling, so
            // every log line names the chord it is on this platform — `Ctrl+Shift+F6` is
            // `Ctrl+Shift+F6` here or `Shift+Cmd+F6` there — and one key written two ways
            // (`Cmd+S` and `ctrl+s`) reads as one key. The dialogs say it from that spelling, in
            // the Mac's own words there (`dialog_key_words_for`). An unparseable spec keeps the
            // author's spelling for the OS to refuse. Both in `hotkey_claim_for`, where the
            // refusal comes first.
            let (binding, spec) = backend::hotkey_claim_for(backend::KeyOs::CURRENT, &spec)
                .map_err(mlua::Error::external)?;
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
            host_call!("host.hotkey.unregister");
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

    // host.keys: low-level key capture + suppression (modifier-aware). `capture`, `release`,
    // `releaseAll`, `scope`, `menuOpen` and `passedThrough` are `install_key_captures`'.
    let keys = lua.create_table()?;
    install_key_captures(lua, &keys, idx, shared.clone())?;
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
        lua.create_function(move |_, ()| { host_call!("host.keys.modifiersDown"); Ok(sh.backend.modifiers_down()) })?,
    )?;
    let sh = shared.clone();
    keys.set(
        "nativeMenuOpen",
        lua.create_function(move |_, ()| { host_call!("host.keys.nativeMenuOpen"); Ok(sh.backend.native_menu_open()) })?,
    )?;
    // host.keys.normalize(spec) -> string? — the key a spec stands for on this platform, in one
    // spelling. What the overlay runtime keys its claims by, so `Cmd+S` and `Ctrl+S` are one key
    // and not two registrations of which only one can live. Needs no capability
    // (see FREE_MEMBERS).
    keys.set(
        "normalize",
        lua.create_function(|_, spec: String| {
            host_call!("host.keys.normalize");
            Ok(backend::normalize_spec_for(backend::KeyOs::CURRENT, &spec))
        })?,
    )?;
    // host.keys.describe(spec, { style = "spoken" | "short" }?) -> string? — the key in this
    // platform's words: "Control+Alt+P" on Windows, "Option+Command+P" for the same spec on a
    // Mac, where the Ctrl role is Command.
    keys.set(
        "describe",
        lua.create_function(|_, (spec, opts): (String, Option<Table>)| {
            host_call!("host.keys.describe");
            let style = match opts {
                None => backend::KeyStyle::Spoken,
                Some(t) => match t.get::<Option<String>>("style")?.as_deref() {
                    None | Some("spoken") => backend::KeyStyle::Spoken,
                    Some("short") => backend::KeyStyle::Short,
                    Some(other) => {
                        return Err(mlua::Error::external(format!(
                            "host.keys.describe: style must be \"spoken\" or \"short\", not \
                             \"{other}\""
                        )))
                    }
                },
            };
            Ok(backend::describe_spec_for(backend::KeyOs::CURRENT, &spec, style))
        })?,
    )?;
    // host.keys.check(spec, { layout = false }?) -> { ok, resolved, reasons, produces?,
    // deadKey? } — what this platform does with a combination, structurally. No list of other
    // programs' keys. `layout = false` leaves the keyboard layout unasked, for a caller that
    // wants only the structural answer: the overlay runtime asks at every bind and every
    // hotkey sync, and has no use for the character.
    let sh = shared.clone();
    keys.set(
        "check",
        lua.create_function(move |lua, (spec, opts): (String, Option<Table>)| {
            host_call!("host.keys.check");
            // Strict, like `interrupt` and `initial`: mlua reads any value but nil and `false`
            // as `true`, so `{ layout = "no" }` asked the layout without a word.
            let ask_layout = opt_bool(opts.as_ref(), "layout", true, "host.keys.check")?;
            let c = backend::check_spec_for(backend::KeyOs::CURRENT, &spec, |vk, mask| {
                if ask_layout {
                    sh.backend.layout_char(vk, mask)
                } else {
                    None
                }
            });
            let t = lua.create_table()?;
            t.set("ok", c.ok())?;
            t.set("resolved", c.resolved.clone())?;
            let reasons = lua.create_table()?;
            for r in &c.reasons {
                reasons.push(*r)?;
            }
            t.set("reasons", reasons)?;
            if let Some(p) = c.produces {
                t.set("produces", p)?;
                t.set("deadKey", c.dead_key)?;
            }
            Ok(t)
        })?,
    )?;
    host.set("keys", keys)?;

    // host.gamepad — game controllers, observed only. Its own file, gamepad_api.rs.
    gamepad_api::install(lua, &host, shared, idx)?;

    // host.now() -> milliseconds since the app started. A CLOCK, not a date: the only
    // thing modules need it for is measuring their own hot paths, and "how long did that
    // take" is exactly what nobody could answer about the overlay runtime — every
    // performance question so far had to be answered from Rust or from log timestamps a
    // second apart. Monotonic, so it cannot go backwards mid-measurement.
    let started = clock_origin();
    host.set(
        "now",
        lua.create_function(move |_, ()| { host_call!("host.now"); Ok(started.elapsed().as_millis() as i64) })?,
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
        lua.create_function(move |_, ()| { host_call!("host.inputEpoch"); Ok(sh_ie.input_epoch.get() as i64) })?,
    )?;

    // host.timer.after / every / cancel — see timers.rs. Owned by `idx`, which for a dependency's
    // code is this VM's owner: `build_dep_host` hands a dependency the owner's table.
    host.set("timer", timers::table(lua, idx, shared.clone(), |s| &s.timers, |_| Instant::now())?)?;

    // host.os.current / host.os.is(name)
    let os = lua.create_table()?;
    os.set("current", std::env::consts::OS)?;
    os.set(
        "is",
        lua.create_function(|_, name: String| { host_call!("host.os.is"); Ok(name == std::env::consts::OS) })?,
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
        lua.create_function(move |lua, id: isize| {
            host_call!("host.window.focus");
            // At once, whatever the module's reads: a picture still out may show the window in
            // front (said once in the log).
            sh.note_acting(lua, "host.window.focus");
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
            host_call!("host.window.list");
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
            host_call!("host.window.apps");
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
            host_call!("host.window.windowsOf");
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
            host_call!("host.window.active");
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
    // host.window.foreground() -> { id, pid, shown }? — the window the keyboard goes to now,
    // shown or not, asked afresh on every call. Deliberately NOT through the observation
    // cache `active` uses: see `foreground_fn`.
    let sh = shared.clone();
    win.set("foreground", foreground_fn(lua, move || sh.backend.foreground_window())?)?;
    // host.window.controls(win?) — child controls (class + geometry) of a window
    // (the active one if omitted), for detecting embedded plugins.
    let sh = shared.clone();
    win.set(
        "controls",
        lua.create_function(move |lua, win_arg: Option<Table>| {
            host_call!("host.window.controls");
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
    // host.window.ownsPoint(id, x, y, opts?) -> true | false | nil.
    //
    // nil means "this platform cannot say", and a caller must read that as permission rather
    // than refusal: a check with no answer must not block what it cannot judge.
    //
    // `opts.listed = true`: `id` is a window as host.window.windowsOf lists it, which on a Mac is
    // the window server's number rather than a host.window id — how the overlay runtime asks
    // about a popup menu it knows only from that list. Anything but a table for `opts`, or a
    // `listed` that is not true or false, raises.
    let sh = shared.clone();
    win.set(
        "ownsPoint",
        lua.create_function(
            move |_, (id, x, y, opts): (isize, i32, i32, Option<Table>)| {
                host_call!("host.window.ownsPoint");
                let listed = opt_bool(opts.as_ref(), "listed", false, "host.window.ownsPoint")?;
                if listed {
                    // A negative number is no window in any list: nothing can be said of it.
                    let Ok(id) = u64::try_from(id) else {
                        return Ok(None);
                    };
                    return Ok(sh.backend.listed_window_owns_point(id, x, y));
                }
                Ok(sh.backend.window_owns_point(id, x, y))
            },
        )?,
    )?;

    let sh = shared.clone();
    win.set(
        "focusChain",
        lua.create_function(move |lua, ()| {
            host_call!("host.window.focusChain");
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
            host_call!("host.window.recheck");
            sh.recheck_requested.set(true);
            Ok(())
        })?,
    )?;
    // host.window._requestInitial() — the prelude's half of `onTrigger { initial = true }`:
    // queues THIS table's owner for a report of the window in front on the next tick (see
    // `Dispatcher::dispatch_initial`). Bound to `idx`, and a dependency's code reaches the
    // owner's window table, so a runtime's trigger is reported into the VM it registered in.
    win.set("_requestInitial", initial_request_fn(lua, idx, shared.clone(), |s| &s.initial_pending)?)?;
    host.set("window", win)?;

    // host.element.find(hwnd, name, controlType) — does the window's UI Automation
    // subtree contain an element with that Name + ControlType? (Plugin identity.)
    let uia = lua.create_table()?;
    let sh = shared.clone();
    uia.set(
        "find",
        lua.create_function(move |_, (hwnd, name, ctype): (isize, String, i32)| {
            host_call!("host.element.find");
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
            host_call!("host.element.findAny");
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
            host_call!("host.element.locate");
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
                host_call!("host.element.locateVia");
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
                host_call!("host.element.pluginLocate");
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
                host_call!("host.element.stateProbe");
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
            host_call!("host.element.dump");
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
            host_call!("host.element.rawDump");
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
                host_call!("host.element.classNavPoint");
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
    // host.element.focusStep(hwnd, direction) -> { name, ctype, index, count, bounds? } | nil —
    // Tab pass-through for a standalone plugin window: SetFocus the next (direction>=0)
    // / previous keyboard-focusable descendant relative to the current focus, wrapping
    // at the ends, and return the newly focused element — where it is included, so a
    // caller can read what a stop with no name shows (see `focus_step_table`).
    let sh = shared.clone();
    uia.set(
        "focusStep",
        lua.create_function(move |lua, (hwnd, direction): (isize, i32)| {
            host_call!("host.element.focusStep");
            match sh.backend.element_focus_step(hwnd, direction) {
                Some(step) => Ok(Some(focus_step_table(lua, step)?)),
                None => Ok(None),
            }
        })?,
    )?;
    // host.element.focusWithin(hwnd, { x, y, w, h }) -> { name, ctype } | nil. Gives the
    // keyboard to the first focusable element inside the rectangle — the way into a plugin
    // that publishes elements, without pressing anything at its corner.
    let sh = shared.clone();
    uia.set(
        "focusWithin",
        lua.create_function(move |lua, (hwnd, rect): (isize, Table)| {
            host_call!("host.element.focusWithin");
            let x: i32 = rect.get("x")?;
            let y: i32 = rect.get("y")?;
            let w: i32 = rect.get("w")?;
            let h: i32 = rect.get("h")?;
            match sh.backend.element_focus_within(hwnd, x, y, w, h) {
                Some((name, ctype)) => {
                    let t = lua.create_table()?;
                    t.set("name", name)?;
                    t.set("ctype", ctype)?;
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
        // pixel(x, y, opts?) or pixel({ window = w, fraction = { x, y } }, opts?) -> colour | nil, reason
        lua.create_function(move |lua, (a, b, c): (mlua::Value, mlua::Value, mlua::Value)| {
            host_call!("host.screen.pixel");
            const F: &str = "host.screen.pixel";
            // The options come after the point: third after two numbers, second after a table.
            let opts = match &a {
                mlua::Value::Table(_) => {
                    if !c.is_nil() {
                        return Err(mlua::Error::external(format!(
                            "{F}: takes (x, y, opts?) or (point, opts?) with point {{ window = w, fraction = {{ x, y }} }}, not a point, {} and {}",
                            describe_value(&b),
                            describe_value(&c)
                        )));
                    }
                    &b
                }
                _ => &c,
            };
            // Read before anything is answered, like every argument: a mistake in the options
            // raises whether or not there is a pixel to read this time.
            let snap = snapshot::strict_opts(opts, F)?;
            let (x, y) = match &a {
                // The window form: the region form's start formula, resolved against the client
                // area the table carries. A minimised window has no pixel: `nil, reason`.
                mlua::Value::Table(_) => {
                    let (client, fx, fy) =
                        region_lua::read_point(&a, "point").map_err(|m| mlua::Error::external(format!("{F}: {m}")))?;
                    match region::resolve_point(client, fx, fy) {
                        Ok(p) => p,
                        Err(u) => return with_reason(lua, mlua::Value::Nil, u.to_string()),
                    }
                }
                // Screen coordinates, converted as they always were (see `pixel_coords`).
                _ => pixel_coords(lua, &a, &b)?,
            };
            // From a snapshot: its pixel, no screen touched and nothing counted — or `nil` and
            // why when the point is not in it.
            if let Some(frame) = snap {
                return match frame.sample(x, y) {
                    Some(px) => one_value(mlua::Value::Table(snapshot::colour_table(lua, px)?)),
                    None => with_reason(lua, mlua::Value::Nil, snapshot::POINT_NOT_INSIDE.to_string()),
                };
            }
            // The source first, outside the timing below: in a module that reads through
            // desktop duplication, the first read also compares the two sources, once.
            let src = capture_source::read_source(lua, &*sh.backend, (x, y, 1, 1));
            // Timed and counted: a single pixel read is a GDI screen touch, and this project
            // has already measured one at a fixed ~16.7 ms — one compositor frame, whatever
            // the size. Sixty of them is a second, and nothing in the log said they were
            // happening.
            let t0 = Instant::now();
            let px = sh.backend.pixel(x, y, src);
            {
                let mut obs = sh.observations();
                obs.pixels += 1;
                obs.pixel_us += t0.elapsed().as_micros();
            }
            // nil only for a module that declared `fallback = "none"` while desktop
            // duplication could not answer: no colour is better than the frozen one. The
            // reason says why, in the words the log uses.
            let px = match px {
                Ok(c) => c,
                Err(why) => return with_reason(lua, mlua::Value::Nil, why),
            };
            one_value(mlua::Value::Table(snapshot::colour_table(lua, px)?))
        })?,
    )?;
    // host.screen.snapshot(opts) / host.screen.pixels(points, opts?) — one picture taken once and
    // read by every call given `{ snapshot = s }`, and many points from as few captures as they
    // allow. The rules are in snapshot.rs; registered HERE, by name, for check-docs.ps1 and
    // tools/api-index.py, which read this file.
    screen.set("snapshot", snapshot::snapshot(lua, shared)?)?;
    screen.set("pixels", snapshot::pixels(lua, shared)?)?;
    // host.screen.snapshotAsync(opts, cb) — the picture taken on the `screen-capture` thread that
    // host.ocr.recognize photographs on: at once, at a set time, or when the region changes.
    screen.set("snapshotAsync", snapshot::snapshot_async(lua, shared, idx)?)?;
    // host.screen.pending(key) — whether this module's `snapshotAsync` or `profile` with a callback
    // with that key is still out (snapshot.rs).
    screen.set("pending", snapshot::pending(lua, shared, idx)?)?;
    let sh = shared.clone();
    screen.set(
        "size",
        lua.create_function(move |lua, ()| {
            host_call!("host.screen.size");
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
    //
    // host.screen.profile(opts, cb) — the same answer, off the event loop: the capture on the
    // `screen-capture` thread (none for a snapshot), the reduction on the image worker, and only
    // the answer's tables here, as `cb` runs (snapshot.rs, `profile_async`). Without a callback
    // the call is what it always was, but for `key` and `at`, which raise there.
    let sh = shared.clone();
    screen.set(
        "profile",
        lua.create_function(move |lua, (opts, cb): (Option<Table>, mlua::Value)| {
            host_call!("host.screen.profile");
            const F: &str = "host.screen.profile";
            let cb = match cb {
                mlua::Value::Nil => None,
                mlua::Value::Function(f) => Some(f),
                other => {
                    return Err(mlua::Error::external(format!(
                        "{F}: the callback (second argument) must be a function, got {}",
                        describe_value(&other)
                    )))
                }
            };
            // Every argument is read before anything is answered, so a mistake in `axes`
            // raises whether or not there is anything to read this time. A snapshot first: a
            // region left out is then the whole snapshot rather than the screen.
            let snap = snapshot::from_opts(opts.as_ref(), F)?;
            let region = match &snap {
                Some(f) => snapshot::opts_region_in(opts.as_ref(), F, f)?,
                None => opts_region(&*sh.backend, opts.as_ref(), F)?,
            };
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
            // How many pixels in each column/row are darker than `dark`.
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
            if let Some(cb) = cb {
                sh.profile_async(lua, idx, cb, opts.as_ref(), snap, region, (want_cols, want_rows), dark_t)?;
                return Ok(mlua::MultiValue::new());
            }
            // `key` and `at` are the callback form's. Here they raise, before anything is
            // answered, rather than being passed over.
            snapshot::callback_only(opts.as_ref(), F, &["key", "at"])?;
            let (rx, ry, rw, rh) = match region {
                Ok(r) => (r.x, r.y, r.w, r.h),
                Err(why) => return with_reason(lua, mlua::Value::Nil, why),
            };
            if rw <= 0 || rh <= 0 {
                return with_reason(lua, mlua::Value::Nil, format!("{EMPTY_REGION} ({rw}x{rh})"));
            }
            let t0 = Instant::now();
            // The picture: the part of the snapshot the region covers, or a capture made now.
            let pic = match snap {
                Some(frame) => match snapshot::clip(&frame, region::ScreenRect { x: rx, y: ry, w: rw, h: rh }) {
                    Ok(rect) => snapshot::Picture::Snap { frame, rect },
                    Err(why) => return with_reason(lua, mlua::Value::Nil, why),
                },
                None => match sh.backend.capture(rx, ry, rw, rh, capture_source::read_source(lua, &*sh.backend, (rx, ry, rw, rh))) {
                    Ok(img) => snapshot::Picture::Live { img, x: rx, y: ry },
                    Err(why) => return with_reason(lua, mlua::Value::Nil, why),
                },
            };
            let captured = Instant::now();
            // The reduction reads only the area it is given, and nothing at all from a buffer
            // shorter than its own size: a capture that came back short — a clipped region, a
            // backend that rounded a dimension — would otherwise index past the end, and a panic
            // inside a Lua binding is not a module error that gets reported: it is the process.
            // Answering nil is what every other failure in this function does.
            let area = pic.area();
            let Some((columns, rows)) = profile::profile(pic.image(), area, want_cols, want_rows, dark_t) else {
                return with_reason(lua, mlua::Value::Nil, SHORT_CAPTURE.to_string());
            };
            let reduced = Instant::now();
            // The rectangle read: the region asked for, or, on a snapshot, its part inside it.
            let (ox, oy) = match &pic {
                snapshot::Picture::Snap { rect, .. } => (rect.x, rect.y),
                snapshot::Picture::Live { .. } => (rx, ry),
            };
            let out = profile_table(lua, (ox, oy, area.w as i32, area.h as i32), columns, rows)?;
            // Timed to HERE, not to the end of the capture. The capture is a fixed frame; the
            // traversal and the six sequences are the only part that grows with the region, and
            // leaving them outside the measurement would hide them from the one report this
            // project uses to find event-loop stalls — while making this call look free. A
            // snapshot's profile touched no screen and is not counted as a pixel read; its
            // reduction runs here all the same, so the profile's own split counts both.
            let end = Instant::now();
            let mut obs = sh.observations();
            if pic.is_live() {
                obs.pixels += 1;
                obs.pixel_us += end.duration_since(t0).as_micros();
            }
            obs.profile.add(t0, captured, reduced, end, !pic.is_live());
            drop(obs);
            one_value(mlua::Value::Table(out))
        })?,
    )?;
    // The image-search bindings and template handles live in image_search.rs, with the rules
    // that belong to them: who an async answer is delivered to, what a module may hold in
    // handles, and what a panic on the worker turns into. Registered HERE, by name, because
    // this file is what check-docs.ps1 and tools/api-index.py read to learn what exists.
    screen.set("template", image_search::template(lua, shared, idx)?)?;
    screen.set("imageSearch", image_search::image_search(lua, shared, idx)?)?;
    screen.set("imageSearchMulti", image_search::image_search_multi(lua, shared, idx)?)?;
    screen.set("imageSearchAsync", image_search::image_search_async(lua, shared, idx)?)?;
    screen.set("imageSearchEach", image_search::image_search_each(lua, shared, idx)?)?;
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
            host_call!("host.screen.saveMarked");
            let full = sh.root(idx).join(&path);
            png_path_only("host.screen.saveMarked", &full)?;
            // A window region is resolved to its screen rectangle first; the marks stay in
            // screen coordinates whichever form the region takes.
            let snap = snapshot::from_opts(Some(&opts), "host.screen.saveMarked")?;
            let region = match &snap {
                Some(f) => snapshot::opts_region_in(Some(&opts), "host.screen.saveMarked", f)?,
                None => opts_region(&*sh.backend, Some(&opts), "host.screen.saveMarked")?,
            };
            let (rx, ry, rw, rh) = match region {
                Ok(r) => (r.x, r.y, r.w, r.h),
                Err(why) => return with_reason(lua, mlua::Value::Boolean(false), why),
            };
            let shot = match snap {
                // On a snapshot, the part of the region inside it, copied out: the marks are
                // then placed relative to that part's corner.
                Some(frame) => snapshot::clip(&frame, region::ScreenRect { x: rx, y: ry, w: rw, h: rh }).and_then(|r| {
                    frame.crop_image(r).map(|img| (r.x, r.y, img)).ok_or_else(|| snapshot::NO_OVERLAP.to_string())
                }),
                None => {
                    let src = capture_source::read_source(lua, &*sh.backend, (rx, ry, rw, rh));
                    sh.backend.capture(rx, ry, rw, rh, src).map(|img| (rx, ry, img))
                }
            };
            let (rx, ry, cap) = match shot {
                Ok(shot) => shot,
                Err(why) => return with_reason(lua, mlua::Value::Boolean(false), why),
            };
            let Some(mut img) = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba) else {
                return with_reason(lua, mlua::Value::Boolean(false), SHORT_CAPTURE.to_string());
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
            one_value(mlua::Value::Boolean(true))
        })?,
    )?;
    // In image_search.rs too; see the note above imageSearch.
    screen.set("imageSearchAll", image_search::image_search_all(lua, shared, idx)?)?;
    // host.screen.predicate / cells / matchCells / matchCellsAsync — a region reduced to a
    // grid of cells, each the share of its pixels that pass a colour test, and the ranking of
    // that grid against stored ones. Built to read the signatures an outside game-menu reader
    // records, bit for bit: the rules are in cells.rs, the window-relative region in region.rs,
    // the Region form's one strict reader in region_lua.rs, and the strict readers of the other
    // arguments above `read_cells_opts`.
    //
    // predicate(expr) -> the canonical form, or raises with the column. For checking a pack's
    // predicate once, at load, instead of in a poll.
    screen.set(
        "predicate",
        lua.create_function(|_, expr: mlua::Value| {
            host_call!("host.screen.predicate");
            const F: &str = "host.screen.predicate";
            let mlua::Value::String(s) = &expr else {
                return Err(mlua::Error::external(format!(
                    "{F}: expects the predicate as a string, got {}",
                    describe_value(&expr)
                )));
            };
            let src = s.to_str()?.to_string();
            cells::Predicate::parse(&src)
                .map(|p| p.to_string())
                .map_err(|e| predicate_error(F, "the predicate", &src, &e))
        })?,
    )?;
    // cells(opts) -> { cells, x, y, w, h } | nil, reason. One capture on the event loop,
    // counted like `profile`; for recording a state, not for a poll.
    let sh = shared.clone();
    screen.set(
        "cells",
        lua.create_function(move |lua, opts: mlua::Value| {
            host_call!("host.screen.cells");
            let call = read_cells_opts(lua, "host.screen.cells", opts)?;
            let r = match call.rect {
                Ok(r) => r,
                Err(why) => return nil_because(lua, why),
            };
            let got = match &call.snapshot {
                Some(frame) => cells_of_snapshot(frame, r, |img, sub| cells::of_sub(img, sub, &call.spec)),
                None => read_cells_with(&sh, lua, r, |cap| cells::of_capture(cap, r.w, r.h, &call.spec)),
            };
            match got {
                Ok(bytes) => Ok((mlua::Value::Table(cells_table(lua, &bytes, r)?), mlua::Value::Nil)),
                Err(why) => nil_because(lua, why),
            }
        })?,
    )?;
    // matchCells(opts, states) -> match | nil, reason. The same capture, ranked against the
    // states in Rust. Every argument is checked before anything is captured, so a bad state
    // raises on the first call, not on the first call whose region resolves.
    let sh = shared.clone();
    screen.set(
        "matchCells",
        lua.create_function(move |lua, (opts, states): (mlua::Value, mlua::Value)| {
            host_call!("host.screen.matchCells");
            const F: &str = "host.screen.matchCells";
            let call = read_cells_opts(lua, F, opts)?;
            let st = read_states(F, states, call.spec.cells())?;
            let r = match call.rect {
                Ok(r) => r,
                Err(why) => return nil_because(lua, why),
            };
            let matcher = cells::Matcher { item_of: cells::items_of(&st.names), spec: call.spec, states: st.bytes };
            let got = match &call.snapshot {
                Some(frame) => cells_of_snapshot(frame, r, |img, sub| matcher.answer_sub(img, sub)),
                None => read_cells_with(&sh, lua, r, |cap| matcher.answer(cap, r.w, r.h)),
            };
            match got {
                Ok((live, ranked)) => Ok((
                    mlua::Value::Table(cells_match_table(lua, &live, r, &ranked, &st.names)?),
                    mlua::Value::Nil,
                )),
                Err(why) => nil_because(lua, why),
            }
        })?,
    )?;
    // matchCellsAsync(opts, states, cb) — capture, reduction and ranking on the image worker,
    // `cb(match, nil)` or `cb(nil, reason)` on a later tick, with the ownership rules of
    // imageSearchAsync (image_search.rs).
    let sh = shared.clone();
    screen.set(
        "matchCellsAsync",
        lua.create_function(move |lua, (opts, states, cb): (mlua::Value, mlua::Value, mlua::Value)| {
            host_call!("host.screen.matchCellsAsync");
            const F: &str = "host.screen.matchCellsAsync";
            let call = read_cells_opts(lua, F, opts)?;
            let st = read_states(F, states, call.spec.cells())?;
            let mlua::Value::Function(cb) = cb else {
                return Err(mlua::Error::external(format!(
                    "{F}: the third argument must be the callback, got {}",
                    describe_value(&cb)
                )));
            };
            let matcher = cells::Matcher { item_of: cells::items_of(&st.names), spec: call.spec, states: st.bytes };
            let rect = call.rect.map(|r| (r.x, r.y, r.w, r.h));
            image_search::enqueue_cells(&sh, lua, idx, cb, rect, matcher, st.names, call.snapshot)
        })?,
    )?;
    // host.screen.save(path, opts?) — capture a screen region (opts.region, else the
    // full screen) and write it to `path` (relative to the calling module's root; an
    // absolute path is used as-is) as a PNG. Returns true on success. A calibration
    // affordance: re-capture an image control's templates against the live plugin.
    let sh = shared.clone();
    screen.set(
        "save",
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            host_call!("host.screen.save");
            let full = sh.root(idx).join(&path);
            png_path_only("host.screen.save", &full)?;
            let snap = snapshot::from_opts(opts.as_ref(), "host.screen.save")?;
            let region = match &snap {
                Some(f) => snapshot::opts_region_in(opts.as_ref(), "host.screen.save", f)?,
                None => opts_region(&*sh.backend, opts.as_ref(), "host.screen.save")?,
            };
            let (rx, ry, rw, rh) = match region {
                Ok(r) => (r.x, r.y, r.w, r.h),
                Err(why) => return with_reason(lua, mlua::Value::Boolean(false), why),
            };
            // On a snapshot, the part of the region inside it, copied out; nothing is captured.
            let shot = snap.map(|frame| {
                snapshot::clip(&frame, region::ScreenRect { x: rx, y: ry, w: rw, h: rh })
                    .and_then(|r| frame.crop_image(r).ok_or_else(|| snapshot::NO_OVERLAP.to_string()))
            });
            // Through the module's own source, like every other read. In a module that reads
            // through duplication, a shot taken before it has opened is the standard picture
            // (or `false` under fallback = "none") — docs/api/screen.md says so.
            let shot = match shot {
                Some(shot) => shot,
                None => sh.backend.capture(rx, ry, rw, rh, capture_source::read_source(lua, &*sh.backend, (rx, ry, rw, rh))),
            };
            match shot {
                Ok(cap) => match image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba) {
                    Some(img) => {
                        if let Some(dir) = full.parent() {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        img.save(&full).map_err(|e| {
                            mlua::Error::external(format!("screen.save '{}': {e}", full.display()))
                        })?;
                        one_value(mlua::Value::Boolean(true))
                    }
                    None => with_reason(lua, mlua::Value::Boolean(false), SHORT_CAPTURE.to_string()),
                },
                Err(why) => with_reason(lua, mlua::Value::Boolean(false), why),
            }
        })?,
    )?;
    host.set("screen", screen)?;

    let ocr = lua.create_table()?;
    // host.ocr.languages() -> { string } — what the engine reads, the default first.
    let sh = shared.clone();
    ocr.set(
        "languages",
        lua.create_function(move |lua, ()| { host_call!("host.ocr.languages"); ocr::lua::languages_value(&*sh, lua) })?,
    )?;
    // host.ocr.resolveLanguage(tag | { tag } | nil) -> string? — what `lang` would read with.
    let sh = shared.clone();
    ocr.set(
        "resolveLanguage",
        lua.create_function(move |_, v: mlua::Value| { host_call!("host.ocr.resolveLanguage"); ocr::lua::resolve_value(&*sh, &v) })?,
    )?;

    // host.ocr.pending(key) -> boolean — whether a read of this module with `key` is still waiting
    // for its answer (ocr/lua.rs).
    let sh = shared.clone();
    ocr.set(
        "pending",
        lua.create_function(move |lua, key: mlua::Value| { host_call!("host.ocr.pending"); ocr::lua::pending(&*sh, lua, idx, key) })?,
    )?;

    // host.ocr.recognize(what, opts?, cb?) — the region(s) photographed at the call and recognised
    // on threads of their own (ocr/lua.rs; the rules in ocr/service.rs and ocr/sched.rs). With a
    // callback, the answer is handed to it on the event loop (`submit`); without one, the handler
    // that called it waits for the answer and only its module is busy (task.rs, task_shim.luau);
    // where it cannot wait, it raises.
    let sh = shared.clone();
    let submit = lua.create_function(move |lua, (what, opts, cb): (mlua::Value, mlua::Value, mlua::Value)| {
        host_call!("host.ocr.recognize");
        ocr::lua::read(&*sh, lua, idx, what, opts, cb)
    })?;
    let recognize = task::waits(lua, idx, shared.clone(), submit)?;
    ocr.set("recognize", recognize.get::<Function>("recognize")?)?;
    host.set("ocr", ocr)?;

    // host.epoch() -> number — a counter that changes whenever the world may have: an
    // OS event dispatched into a module, a timer firing, or the module itself driving
    // input. Memoize an expensive observation against it (`if cachedEpoch == host.epoch()
    // then return cached end`) and repeats within one dispatch cost nothing, while a new
    // situation is always re-observed. Deliberately NOT time-based: a stale coordinate
    // clicks the wrong thing, and "it was fresh 50 ms ago" is not a safety property.
    let sh = shared.clone();
    host.set("epoch", lua.create_function(move |_, ()| { host_call!("host.epoch"); Ok(sh.epoch.get()) })?)?;

    // host.input: cursorPos / move / click / drag / scroll / send / text
    let input = lua.create_table()?;
    let sh = shared.clone();
    input.set(
        "cursorPos",
        lua.create_function(move |lua, ()| {
            host_call!("host.input.cursorPos");
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
        lua.create_function(move |lua, (x, y): (i32, i32)| {
            host_call!("host.input.move");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.move");
            sh.bump_input_epoch();
            sh.backend.mouse_move(x, y);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "click",
        lua.create_function(move |lua, (x, y, opts): (i32, i32, Option<Table>)| {
            host_call!("host.input.click");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.click");
            sh.bump_input_epoch();
            let button = button_from(opts.as_ref());
            sh.backend.mouse_click(x, y, button);
            // Its own release lets go of a button held down before it.
            sh.let_go_button(idx, button);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "post",
        lua.create_function(move |lua, (hwnd, key): (i64, String)| {
            host_call!("host.input.post");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.post");
            sh.backend
                .key_post(hwnd as isize, &key)
                .map_err(mlua::Error::external)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "mouseDown",
        lua.create_function(move |lua, (x, y, opts): (i32, i32, Option<Table>)| {
            host_call!("host.input.mouseDown");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.mouseDown");
            sh.bump_input_epoch();
            let button = button_from(opts.as_ref());
            sh.backend.mouse_down(x, y, button);
            // Held until its mouseUp — or until the module stops running, when the host lets go
            // of it (`release_held_buttons`).
            sh.hold_button(idx, button);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "mouseUp",
        lua.create_function(move |lua, (x, y, opts): (i32, i32, Option<Table>)| {
            host_call!("host.input.mouseUp");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.mouseUp");
            sh.bump_input_epoch();
            let button = button_from(opts.as_ref());
            sh.backend.mouse_up(x, y, button);
            sh.let_go_button(idx, button);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "drag",
        lua.create_function(move |lua, (x1, y1, x2, y2, opts): (i32, i32, i32, i32, Option<Table>)| {
            host_call!("host.input.drag");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.drag");
            sh.bump_input_epoch();
            let button = button_from(opts.as_ref());
            sh.backend.mouse_drag(x1, y1, x2, y2, button);
            // Its own release lets go of a button held down before it.
            sh.let_go_button(idx, button);
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
        lua.create_function(move |lua, (x, y, notches): (i32, i32, f64)| {
            host_call!("host.input.scroll");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.scroll");
            sh.bump_input_epoch();
            let delta = (notches * 120.0).round() as i32;
            sh.backend.mouse_scroll(x, y, delta);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "send",
        lua.create_function(move |lua, combo: String| {
            host_call!("host.input.send");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.send");
            sh.bump_input_epoch();
            sh.backend.key_send(&combo).map_err(mlua::Error::external)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "text",
        lua.create_function(move |lua, text: String| {
            host_call!("host.input.text");
            // At once: a picture of this module's still out may show it (said once in the log).
            sh.note_acting(lua, "host.input.text");
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
            host_call!("host.sound.play");
            let path = sh.root(idx).join(&rel);
            let mut audio = sh.audio.borrow_mut();
            // Opened again when its stream failed or the default device is another one now
            // (sound.rs); the manager drops it after a resume, so it is opened then too.
            if audio.as_ref().is_some_and(sound::Output::stale) {
                if let Some(old) = audio.take() {
                    logging::line(
                        "sound",
                        &format!(
                            "the audio output on {} is opened again: it failed, or the default \
                             output device changed",
                            old.device()
                        ),
                    );
                }
            }
            if audio.is_none() {
                // Open the audio device on first use only (see the field doc).
                match sound::Output::open() {
                    Ok(out) => *audio = Some(out),
                    Err(e) => {
                        logging::line("sound", &format!("no audio output device: {e}"));
                        return Ok(());
                    }
                }
            }
            if let Some(out) = audio.as_ref() {
                let play = || -> anyhow::Result<()> {
                    let file = std::io::BufReader::new(std::fs::File::open(&path)?);
                    let source = rodio::Decoder::new(file)?;
                    out.play(source);
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
            host_call!("host.path");
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
            host_call!("host.resource.read");
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
        lua.create_function(move |_, rel: String| { host_call!("host.resource.exists"); Ok(sh.root(idx).join(&rel).exists()) })?,
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
                host_call!("host.settings.define");
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
                            def.clone()
                        }
                    }
                };
                if current != def {
                    note_stored_setting(&id, &key, &current, &def);
                }
                value_to_lua(lua, &current)
            },
        )?,
    )?;
    let sh = shared.clone();
    settings_api.set(
        "get",
        lua.create_function(move |lua, key: String| {
            host_call!("host.settings.get");
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
        lua.create_function(move |lua, (key, value): (String, mlua::Value)| {
            host_call!("host.settings.set");
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
            // The caller's own `onChange`s run inside this call; everybody else's after it.
            let caller = image_search::vm_owner(lua).map_or(idx, |o| o.idx);
            sh.fire_on_change(idx, &key, &v, old.as_ref(), Some(caller));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    settings_api.set(
        "onChange",
        lua.create_function(move |lua, (key, cb): (String, Function)| {
            host_call!("host.settings.onChange");
            file_on_change(&mut sh.on_change.borrow_mut(), lua, idx, key, cb)
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
                host_call!("host.arbiter.register");
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
            host_call!("host.arbiter.setMatching");
            sh.arbiter_set_matching(&slot, handle, matching);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    arbiter.set(
        "unregister",
        lua.create_function(move |_, (slot, handle): (String, i64)| {
            host_call!("host.arbiter.unregister");
            sh.arbiter_unregister(&slot, handle);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    arbiter.set(
        "winner",
        lua.create_function(move |_, slot: String| { host_call!("host.arbiter.winner"); Ok(sh.arbiter_winner(&slot)) })?,
    )?;
    // host.arbiter.winnerSpecificity(slot) -> number | nil — the rank of whoever currently
    // owns the slot. Lets a claimant skip work it cannot use: with twelve sample-library
    // overlays competing for one plugin, and only one library ever loaded, eleven of their
    // twelve landmark searches are known-pointless the moment any of them wins.
    let sh = shared.clone();
    arbiter.set(
        "winnerSpecificity",
        lua.create_function(move |_, slot: String| { host_call!("host.arbiter.winnerSpecificity"); Ok(sh.arbiter_winner_specificity(&slot)) })?,
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
            host_call!("host.arbiter.participants");
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
///   * A path that leaves the module directory is REJECTED — see `include_target`.
///     `host.path` / `host.resource.read` join without normalizing, which lets a path escape
///     the module directory; that is a nuisance for reading a file and a different thing
///     entirely for executing one.
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
            host_call!("host.include");
            let abs = include_target(&sh.root(idx), &rel).map_err(mlua::Error::external)?;
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

            // Its chunk is this module's file — `idx`, the dependency's own for a dependency's
            // include — named for the guard's frames: a stop in it names that module.
            let owner = sh.ids.borrow().get(idx).cloned().unwrap_or_default();
            vm_guard::name_chunk(lua, &key, &owner, &path_clean::clean(&rel).to_string_lossy());
            let result = (|| -> mlua::Result<mlua::Value> {
                let f = compile_include(lua, &abs, &rel, &key)?;
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

/// Reads and compiles the file `host.include(rel)` resolved to `abs`, and returns the function
/// the include calls with the includer's host. Named `key` (its tidied path) in tracebacks.
///
/// A file that cannot be read raises `include '<rel>': <the system's reason>`; a leading
/// byte-order mark is dropped (`read_luau_source`). The wrapper opens on the SAME line as the
/// file's first line, so reported line numbers match the file.
fn compile_include(lua: &Lua, abs: &Path, rel: &str, key: &str) -> mlua::Result<Function> {
    let src = read_luau_source(abs)
        .map_err(|e| mlua::Error::external(format!("include '{rel}': {e}")))?;
    let chunk = lua
        .load(format!("return function(host) {src}\nend"))
        .set_name(key)
        .into_function()?;
    chunk.call(())
}

/// The file `host.include(rel)` reads for a module rooted at `root`, or why it may not.
///
/// The path is cleaned LEXICALLY (path-clean: `.` dropped, each `..` removes the part before
/// it) before anything is compared, and then must still be relative and must not start with
/// `..`. The check used to be `std::path::absolute(root.join(rel)).starts_with(root)`, which
/// is right on Windows, where making a path absolute also resolves `..` — and wrong on macOS,
/// where it keeps `..`, so `"../other/x.luau"` started with the root and ran code from outside
/// the module. Cleaning first makes the rule the same on both.
///
/// `\` and `:` are refused as TEXT, on every platform. On Windows they are a separator and a
/// drive (`sub\..\..\x`, `C:x`), and a path that means one thing on Windows and another on
/// macOS is the kind of difference the check above existed to close; `/` is the separator
/// everywhere.
///
/// The cleaned path is also the include's cache key, so `"src/a.luau"` and
/// `"src/../src/a.luau"` are one include on both platforms.
fn include_target(root: &Path, rel: &str) -> std::result::Result<PathBuf, String> {
    use std::path::Component;
    if let Some(c) = rel.chars().find(|c| matches!(c, '\\' | ':')) {
        // `{c}`, not `{c:?}`: Debug doubles the backslash, and a screen reader then reads two.
        return Err(format!(
            "include '{rel}': '{c}' is not allowed in an include path — separate folders with '/'"
        ));
    }
    let cleaned = path_clean::clean(rel);
    let first = cleaned.components().next();
    if cleaned.has_root()
        || cleaned.is_absolute()
        || !matches!(first, Some(Component::Normal(_)))
    {
        return Err(format!("include '{rel}' resolves outside the module directory"));
    }
    let root_abs = path_clean::clean(std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf()));
    let target = root_abs.join(&cleaned);
    // Cannot fail after the checks above; kept so that a later change to them cannot quietly
    // let a path out.
    if !target.starts_with(&root_abs) {
        return Err(format!("include '{rel}' resolves outside the module directory"));
    }
    Ok(target)
}

/// Every Luau file this repository ships compiles.
///
/// Compiled, never run: running a module needs a host and a screen, and most of what goes
/// wrong in a Luau edit is caught here already — a syntax error, and Luau's own limits, of
/// which the one that bites a file the overlay runtime's size is 200 locals per function.
/// Until this test, the first to hear of either was the log of whoever loaded the module next.
#[cfg(test)]
mod luau_source_tests {
    #[test]
    fn every_shipped_luau_file_compiles() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut stack: Vec<std::path::PathBuf> =
            ["modules", "tools", "examples"].iter().map(|d| root.join(d)).collect();
        let (lua, mut compiled, mut failures) = (mlua::Lua::new(), 0, Vec::new());
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "luau") {
                    let src = std::fs::read_to_string(&p).unwrap();
                    match lua.load(&src).set_name(p.display().to_string()).into_function() {
                        Ok(_) => compiled += 1,
                        Err(e) => failures.push(format!("{}: {e}", p.display())),
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        assert!(compiled > 20, "only {compiled} Luau files found under {}", root.display());
    }
}

#[cfg(test)]
mod optional_code_dep_tests {
    use super::*;

    /// A code module in its own folder under `parent`, claiming `os` in `supported_os`.
    fn code_module(parent: &Path, id: &str, os: &str) {
        let dir = parent.join(id.rsplit('.').next().unwrap());
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("module.toml"),
            format!(
                "id = \"{id}\"\nname = \"Test\"\nversion = \"1.0.0\"\nentry = \"src/main.luau\"\n\
                 code_module = true\nsupported_os = [\"{os}\"]\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("src").join("main.luau"), "return {}\n").unwrap();
    }

    #[test]
    fn an_optional_code_dependency_for_another_platform_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        code_module(tmp.path(), "com.test.elsewhere", "plan9");
        code_module(tmp.path(), "com.test.here", std::env::consts::OS);
        let mut out = Vec::new();
        collect_code_deps(
            tmp.path(),
            &[],
            &["com.test.elsewhere".to_string(), "com.test.here".to_string()],
            &mut out,
            &mut HashSet::new(),
        )
        .unwrap();
        let ids: Vec<&str> = out.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["com.test.here"], "only the dependency that runs here is evaluated");
    }

    #[test]
    fn a_required_code_dependency_for_another_platform_is_still_collected() {
        // Kept, so the dependent fails loudly where the module table is consulted: it cannot
        // work without a dependency it requires.
        let tmp = tempfile::tempdir().unwrap();
        code_module(tmp.path(), "com.test.elsewhere", "plan9");
        let mut out = Vec::new();
        collect_code_deps(tmp.path(), &["com.test.elsewhere".to_string()], &[], &mut out, &mut HashSet::new())
            .unwrap();
        assert_eq!(out.len(), 1);
    }

    /// A module in its own folder under `parent`: a `code_module` or one for data, its
    /// dependencies, its `[limits] memory_mib` if it writes one, and the system it runs on.
    fn module(parent: &Path, id: &str, code: bool, deps: &[&str], mib: Option<u32>, os: &str) {
        let dir = parent.join(id.rsplit('.').next().unwrap());
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let list = deps.iter().map(|d| format!("{d:?}")).collect::<Vec<_>>().join(", ");
        let limits = mib.map_or(String::new(), |m| format!("\n[limits]\nmemory_mib = {m}\n"));
        std::fs::write(
            dir.join("module.toml"),
            format!(
                "id = \"{id}\"\nname = \"{}\"\nversion = \"1.0.0\"\nentry = \"src/main.luau\"\ncode_module = {code}\n\
                 dependencies = [{list}]\nsupported_os = [\"{os}\"]\n{limits}",
                id.to_uppercase()
            ),
        )
        .unwrap();
        std::fs::write(dir.join("src").join("main.luau"), "return {}\n").unwrap();
    }

    /// A VM's memory limit is summed from the manifests on disk, as `populate_vm` collects them:
    /// each code dependency once, transitively — the diamond's shared one once — with its own
    /// amount; a dependency for data, an absent optional one and an optional one for another
    /// system add nothing; and a dependency that writes the default out counts as set, as the
    /// module itself does.
    #[test]
    fn the_vms_limit_is_summed_from_the_manifests_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let os = std::env::consts::OS;
        module(tmp.path(), "com.t.a", true, &["com.t.c"], Some(512), os);
        module(tmp.path(), "com.t.b", true, &["com.t.c"], None, os);
        module(tmp.path(), "com.t.c", true, &[], Some(256), os);
        module(tmp.path(), "com.t.d", false, &[], Some(2048), os);
        module(tmp.path(), "com.t.f", true, &[], Some(1024), "plan9");
        let root = module_manifest::ModuleManifest::parse(
            "id = \"com.t.root\"\nname = \"Root\"\nversion = \"1.0.0\"\n\
             dependencies = [\"com.t.a\", \"com.t.b\", \"com.t.d\"]\noptional_dependencies = [\"com.t.e\", \"com.t.f\"]\n",
        )
        .unwrap();
        let mut deps = Vec::new();
        collect_code_deps(tmp.path(), &root.dependencies, &root.optional_dependencies, &mut deps, &mut HashSet::new()).unwrap();
        let info = vm_guard::VmInfo::of(0, &root, &deps);
        let parts: Vec<(&str, u32, bool)> = info.code.iter().map(|c| (c.id.as_str(), c.mib, c.declared)).collect();
        assert_eq!(
            parts,
            [("com.t.root", 256, false), ("com.t.c", 256, true), ("com.t.a", 512, true), ("com.t.b", 256, false)],
            "C once, before A; D, E and F not at all"
        );
        assert_eq!(info.limit_mib(), 256 + 256 + 512 + 256);
        assert_eq!(info.name_of("com.t.a").as_deref(), Some("COM.T.A"), "names from the manifests");
    }
}

#[cfg(test)]
mod include_tests {
    use super::include_target;
    use std::path::Path;

    fn root() -> &'static Path {
        if cfg!(windows) {
            Path::new(r"C:\apps\modules\m")
        } else {
            Path::new("/apps/modules/m")
        }
    }

    #[test]
    fn an_include_stays_inside_the_module() {
        for rel in [
            "../other/x.luau",
            "src/../../other/x.luau",
            "..",
            "",
            ".",
            "/etc/x.luau",
            "src/../..",
            // Windows spellings, refused as text on every platform.
            "src\\..\\..\\x.luau",
            "..\\x.luau",
            "C:x.luau",
            "C:/x.luau",
            "\\\\server\\share\\x.luau",
            "sub/C:evil.luau",
        ] {
            assert!(include_target(root(), rel).is_err(), "{rel:?} must be refused");
        }
    }

    #[test]
    fn two_spellings_of_one_file_are_one_include() {
        let plain = include_target(root(), "src/a.luau").expect("plain path");
        assert_eq!(plain, root().join("src").join("a.luau"));
        for rel in ["src/../src/a.luau", "./src/a.luau", "src//a.luau", "src/./a.luau", "x/../src/a.luau"] {
            assert_eq!(include_target(root(), rel).expect(rel), plain, "{rel:?}");
        }
    }

    #[test]
    fn a_root_given_with_dot_dot_still_contains_its_files() {
        // A module folder named on the command line as `../modules/m`: the root is cleaned
        // too, or nothing would ever start with it.
        let odd = if cfg!(windows) {
            Path::new(r"C:\apps\x\..\modules\m")
        } else {
            Path::new("/apps/x/../modules/m")
        };
        assert_eq!(
            include_target(odd, "src/a.luau").expect("inside"),
            root().join("src").join("a.luau")
        );
        assert!(include_target(odd, "../m2/a.luau").is_err());
    }
}

/// Luau sources with a byte-order mark, and the strict boolean options.
#[cfg(test)]
mod source_and_option_tests {
    use super::*;

    const BOM: &str = "\u{feff}";

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    /// Why `read_luau_source` exists. If Luau ever learns to skip the mark itself, this says
    /// so, and the stripping can go.
    #[test]
    fn luau_itself_rejects_a_byte_order_mark() {
        let e = Lua::new().load(format!("{BOM}return 1")).into_function().err();
        assert!(e.is_some_and(|e| e.to_string().to_lowercase().contains("feff")), "Luau compiled a leading U+FEFF");
    }

    #[test]
    fn one_leading_mark_is_dropped_and_nothing_else() {
        let tmp = tempfile::tempdir().unwrap();
        let read = |name: &str, text: &str| read_luau_source(&write(tmp.path(), name, text)).unwrap();
        assert_eq!(read("a.luau", &format!("{BOM}return 1\n")), "return 1\n");
        assert_eq!(read("b.luau", "return 1\n"), "return 1\n");
        assert_eq!(read("c.luau", ""), "");
        assert_eq!(read("d.luau", BOM), "");
        // A second mark is the file's own content, and so is one further in.
        assert_eq!(read("e.luau", &format!("{BOM}{BOM}x")), format!("{BOM}x"));
        assert_eq!(read("f.luau", &format!("local s = '{BOM}'")), format!("local s = '{BOM}'"));
        assert!(read_luau_source(&tmp.path().join("missing.luau")).is_err());
    }

    /// The entry file's path through `run_module_code`: read, then loaded as a chunk.
    #[test]
    fn an_entry_file_with_a_mark_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(tmp.path(), "main.luau", &format!("{BOM}local n = 40\nreturn n + 2\n"));
        let v: i64 = Lua::new().load(read_luau_source(&p).unwrap()).eval().unwrap();
        assert_eq!(v, 42);
    }

    /// A code dependency's entry, evaluated with the dependent's host as `eval_on_host` does.
    #[test]
    fn a_code_dependency_with_a_mark_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let p = write(
            tmp.path(),
            "runtime.luau",
            &format!("{BOM}local M = {{}}\nfunction M.start(pack) return host.name .. ':' .. pack end\nreturn M\n"),
        );
        let lua = Lua::new();
        let host = lua.create_table().unwrap();
        host.set("name", "game").unwrap();
        let v = eval_on_host(&lua, &host, &read_luau_source(&p).unwrap(), "runtime.luau").unwrap();
        let mlua::Value::Table(m) = v else { panic!("the runtime returned no table") };
        let start: Function = m.get("start").unwrap();
        assert_eq!(start.call::<String>("menu").unwrap(), "game:menu");
    }

    /// `host.include`'s compile step: a mark is dropped, line numbers still match the file, and
    /// a missing file names the path it was asked for with the system's reason.
    #[test]
    fn an_include_with_a_mark_loads() {
        let tmp = tempfile::tempdir().unwrap();
        let lua = Lua::new();
        let p = write(tmp.path(), "gamepack.luau", &format!("{BOM}return {{ items = 5 }}\n"));
        let f = compile_include(&lua, &p, "data/gamepack.luau", "=gamepack.luau").unwrap();
        let t: Table = f.call(lua.create_table().unwrap()).unwrap();
        assert_eq!(t.get::<i64>("items").unwrap(), 5);

        let p = write(tmp.path(), "fails.luau", &format!("{BOM}local x = 1\nerror('on line two')\n"));
        let f = compile_include(&lua, &p, "src/fails.luau", "=fails.luau").unwrap();
        let e = f.call::<()>(lua.create_table().unwrap()).unwrap_err().to_string();
        assert!(e.contains("fails.luau:2:"), "the line number moved: {e}");

        let e = compile_include(&lua, &tmp.path().join("nope.luau"), "data/nope.luau", "=nope")
            .unwrap_err()
            .to_string();
        assert!(e.starts_with("include 'data/nope.luau': "), "{e}");
    }

    /// `host.speech.output`'s options as the binding declares them: `interrupt` is true unless
    /// it is `false`, and a value that is neither raises.
    #[test]
    fn speech_interrupts_unless_told_not_to() {
        let lua = Lua::new();
        let output = lua
            .create_function(|_, (_text, opts): (String, Option<Table>)| speech_interrupt(opts.as_ref()))
            .unwrap();
        lua.globals().set("output", output).unwrap();
        let ask = |call: &str| lua.load(format!("return {call}")).eval::<bool>();
        for call in [
            "output('x')",
            "output('x', nil)",
            "output('x', {})",
            "output('x', { interrupt = nil })",
            "output('x', { interrupt = true })",
            "output(5)",
        ] {
            assert!(ask(call).unwrap(), "{call} interrupts");
        }
        assert!(!ask("output('x', { interrupt = false })").unwrap(), "false queues");
        for bad in ["1", "0", "'no'", "{}"] {
            let e = ask(&format!("output('x', {{ interrupt = {bad} }})")).unwrap_err().to_string();
            assert!(e.contains("host.speech.output: interrupt is true or false, not a"), "{bad}: {e}");
        }
        for bad in ["output()", "output(nil)", "output({})", "output(true)", "output('x', 'fast')", "output('x', 1)"] {
            assert!(ask(bad).is_err(), "{bad} must raise");
        }
    }

    /// The same reader for `host.keys.check`'s `layout`.
    #[test]
    fn a_boolean_option_is_strict() {
        let lua = Lua::new();
        let t = lua.create_table().unwrap();
        let check = |t: Option<&Table>| opt_bool(t, "layout", true, "host.keys.check");
        assert!(check(None).unwrap());
        assert!(check(Some(&t)).unwrap());
        t.set("layout", false).unwrap();
        assert!(!check(Some(&t)).unwrap());
        t.set("layout", true).unwrap();
        assert!(check(Some(&t)).unwrap());
        t.set("layout", "no").unwrap();
        let e = check(Some(&t)).unwrap_err().to_string();
        assert!(e.contains("host.keys.check: layout is true or false, not a string"), "{e}");
        t.set("layout", 0).unwrap();
        let e = check(Some(&t)).unwrap_err().to_string();
        assert!(e.contains("not a number"), "{e}");
    }
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

/// The named-registry key under which each VM keeps the host's own handle on its
/// `host.window` table. See `install_window_prelude`.
const WINDOW_DISPATCH: &str = "__host_window";

/// Runs the window prelude against `host_m`, the VM's WHOLE host table, and keeps the host's
/// own handle on the window table it extended.
///
/// The handle is what the host delivers window events through: `window_has_triggers`,
/// `dispatch_activate` and `dispatch_focus`. They used to reach the prelude through the
/// global `host`, which after loading is the module's GATED view — so a module that had not
/// declared `window` got a capability refusal on every foreground and focus change, logged
/// each time and put in a dialog once, for a lookup that was the host's and not the module's.
/// And an overlay module relying on the runtime's manifest never activated at all: its
/// triggers were registered (through the runtime's own host, which does declare `window`),
/// but `window_has_triggers` answered false for it, so the foreground watch was never
/// started on its behalf, and the dispatch that would have fired them raised.
///
/// The named registry is out of reach of Luau code — Luau has no `debug.getregistry` — so
/// this handle opens nothing to a module. The gate still stands between a module's own code
/// and `host.window`; only the host's delivery goes around it.
fn install_window_prelude(lua: &Lua, host_m: &Table) -> mlua::Result<()> {
    let prelude = lua
        .load(format!("return function(host) {WINDOW_PRELUDE}
end"))
        .set_name("window_prelude")
        .into_function()?;
    prelude.call::<Function>(())?.call::<()>(host_m)?;
    let window: Table = host_m.get("window")?;
    lua.set_named_registry_value(WINDOW_DISPATCH, window)
}

/// The window table of `lua`'s host, as the host itself reaches it — never through the
/// module's gated view. See `install_window_prelude`.
fn host_window(lua: &Lua) -> mlua::Result<Table> {
    lua.named_registry_value::<Table>(WINDOW_DISPATCH)
}

fn has_triggers(window: &Table) -> mlua::Result<bool> {
    window.get::<Function>("_hasTriggers")?.call::<bool>(())
}

/// The number of the newest window trigger of `lua`'s VM, 0 for none (or no prelude): asked when
/// an activation arrives, so a VM with no trigger costs no handler, and a trigger made after it
/// arrived — by a handler that ran before it — does not get it.
fn trigger_seq(lua: &Lua) -> i64 {
    host_window(lua).and_then(|w| w.get::<Function>("_triggerSeq")?.call::<i64>(())).unwrap_or(0)
}

/// The named registry's per-VM function the window dispatches subtract the time the module's
/// handler spent parked with (`[dispatch] … ms` counts running time only). Absent in a VM the
/// host did not build — a test's — where nothing waits.
const WAITED_KEY: &str = "__handler_waited";

fn waited_fn(lua: &Lua) -> Option<Function> {
    lua.named_registry_value::<Function>(WAITED_KEY).ok()
}

/// Whether `_dispatchInitial(win, reprime)` would have anything to do in this VM: a trigger
/// still waiting for its report, or — re-arming — any `initial` trigger at all. Asked before
/// the foreground is, so a VM that answers false costs no foreground query. False for a VM
/// the prelude never ran in.
fn wants_initial(lua: &Lua, reprime: bool) -> bool {
    host_window(lua)
        .and_then(|w| w.get::<Function>("_wantsInitial")?.call::<bool>(reprime))
        .unwrap_or(false)
}

/// Returns true if the given module VM registered any window triggers or focus callbacks,
/// whether its own code did or a code dependency's did on its behalf.
fn window_has_triggers(lua: &Lua) -> bool {
    host_window(lua).and_then(|w| has_triggers(&w)).unwrap_or(false)
}

/// The call that delivers a foreground change into one VM's `onTrigger` callbacks — reaching
/// the triggers numbered up to `upto` and those made during the dispatch itself (`None`: every
/// one) — or `None` for a VM that registered nothing, skipped before anything is converted: the
/// window table, the prewarm hook and the prelude's loop are all work for nobody, repeated for
/// every module on every window switch. Built as the event runs, in the module's handler.
fn activate_call(lua: &Lua, win: &WinInfo, upto: Option<i64>) -> mlua::Result<Option<(Function, mlua::MultiValue)>> {
    // No handle is a VM the prelude never ran in — none that loaded — and nothing to deliver
    // to, as `window_has_triggers` answers for it too.
    let Ok(window) = host_window(lua) else { return Ok(None) };
    if !has_triggers(&window)? {
        return Ok(None);
    }
    let table = win_to_table(lua, win)?;
    // For a module that reads the screen through desktop duplication, a hook the dispatch
    // runs before the first matching trigger's callback: its window has just come forward,
    // and that callback's detection read is what opening the duplication ahead of time is
    // for. `nil` for every other module.
    let before = capture_source::prewarm_hook(lua)?;
    let f: Function = window.get("_dispatchActivate")?;
    let args = (table, before, waited_fn(lua), upto).into_lua_multi(lua)?;
    Ok(Some((f, args)))
}

/// The call that delivers the report `onTrigger { initial = true }` asked for into one VM: the
/// window in front, or `None` when there is none (then nothing is called, though the primed
/// triggers are still un-primed — nothing was in front to report). With `reprime`, every
/// `initial` trigger of the VM is primed again first (the module was just enabled). `before`
/// runs once, before the first callback that matches — the host's preparation, as an
/// activation's: `capture_source::prewarm_hook`, a plain function, which nothing scoped has to
/// outlive. `None` for a VM with no triggers, or without the prelude, skipped before anything is
/// converted.
fn initial_call(
    lua: &Lua,
    win: Option<&WinInfo>,
    reprime: bool,
    before: Option<Function>,
) -> mlua::Result<Option<(Function, mlua::MultiValue)>> {
    let Ok(window) = host_window(lua) else { return Ok(None) };
    if !has_triggers(&window)? {
        return Ok(None);
    }
    let table = match win {
        Some(w) => mlua::Value::Table(win_to_table(lua, w)?),
        None => mlua::Value::Nil,
    };
    let f: Function = window.get("_dispatchInitial")?;
    let args = (table, reprime, before, waited_fn(lua)).into_lua_multi(lua)?;
    Ok(Some((f, args)))
}

/// The call that delivers a focus change into one VM's `onFocus` callbacks; `None`, like
/// `activate_call`, for a VM that registered none.
fn focus_call(lua: &Lua) -> mlua::Result<Option<(Function, mlua::MultiValue)>> {
    let Ok(window) = host_window(lua) else { return Ok(None) };
    if !has_triggers(&window)? {
        return Ok(None);
    }
    let f: Function = window.get("_dispatchFocus")?;
    Ok(Some((f, waited_fn(lua).into_lua_multi(lua)?)))
}

/// An activation delivered at once, as a free module's mailbox runs it: for the tests.
#[cfg(test)]
fn dispatch_activate(lua: &Lua, win: &WinInfo) -> mlua::Result<()> {
    match activate_call(lua, win, None)? {
        Some((f, args)) => f.call::<()>(args),
        None => Ok(()),
    }
}

/// A focus change delivered at once, as a free module's mailbox runs it: for the tests.
#[cfg(test)]
fn dispatch_focus(lua: &Lua) -> mlua::Result<()> {
    match focus_call(lua)? {
        Some((f, args)) => f.call::<()>(args),
        None => Ok(()),
    }
}

/// The report of the window in front delivered at once, with `before` as the preparation: for
/// the tests. How many callbacks ran.
#[cfg(test)]
fn dispatch_initial(lua: &Lua, win: Option<&WinInfo>, reprime: bool, before: Option<Function>) -> mlua::Result<i64> {
    match initial_call(lua, win, reprime, before)? {
        Some((f, args)) => f.call::<i64>(args),
        None => Ok(0),
    }
}

/// Reads `{ button = "left"|"right"|"middle" }` from input opts (default left).
/// A button's name in the messages: "left", "right", "middle".
fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
    }
}

fn button_from(opts: Option<&Table>) -> MouseButton {
    let name = opts.and_then(|o| o.get::<String>("button").ok()).unwrap_or_default();
    match name.to_ascii_lowercase().as_str() {
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

/// `host.screen.profile`'s answer as Luau tables: `{ x, y, w, h, columns?, rows? }`, each axis the
/// seven sequences the reference lists, in its order. Built on the event loop in either form — by
/// the call without a callback, and as a callback's answer runs (snapshot.rs, `call_args`).
pub(crate) fn profile_table(
    lua: &Lua,
    (x, y, w, h): (i32, i32, i32, i32),
    columns: Option<profile::Axis>,
    rows: Option<profile::Axis>,
) -> mlua::Result<Table> {
    let axis = |a: profile::Axis| -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("min", lua.create_sequence_from(a.min)?)?;
        t.set("max", lua.create_sequence_from(a.max)?)?;
        t.set("dark", lua.create_sequence_from(a.dark)?)?;
        t.set("mean", lua.create_sequence_from(a.mean)?)?;
        t.set("r", lua.create_sequence_from(a.r)?)?;
        t.set("g", lua.create_sequence_from(a.g)?)?;
        t.set("b", lua.create_sequence_from(a.b)?)?;
        Ok(t)
    };
    let out = lua.create_table()?;
    out.set("x", x)?;
    out.set("y", y)?;
    out.set("w", w)?;
    out.set("h", h)?;
    if let Some(c) = columns {
        out.set("columns", axis(c)?)?;
    }
    if let Some(r) = rows {
        out.set("rows", axis(r)?)?;
    }
    Ok(out)
}

/// A region argument of a call that reads its corners loosely — `profile`, the image searches,
/// `save`, `saveMarked`, `template{ capture }` — read by the Region form's one reader
/// (`region_lua::read_loose`) and resolved now.
///
/// A mistake in it raises, named `{fname}: {what} …`: a table that is neither corners nor the
/// window form, a value that is not a table, a window form that is not exactly right. `nil` is
/// the whole primary screen. `Ok(Err(reason))` is a window region with no rectangle this time —
/// a minimised window — which each call answers in its own shape instead of raising.
pub(crate) fn region_arg(
    backend: &dyn Backend,
    v: &mlua::Value,
    fname: &str,
    what: &str,
) -> mlua::Result<std::result::Result<region::ScreenRect, String>> {
    region_arg_on(backend.screen_size(), v, fname, what)
}

/// [`region_arg`] with the primary screen's size given, so every call's reading can be tested
/// without a backend.
fn region_arg_on(
    screen: (i32, i32),
    v: &mlua::Value,
    fname: &str,
    what: &str,
) -> mlua::Result<std::result::Result<region::ScreenRect, String>> {
    let given = region_lua::read_loose(v, what, screen).map_err(|m| mlua::Error::external(format!("{fname}: {m}")))?;
    Ok(given.resolve().map_err(|u| u.to_string()))
}

/// [`region_arg`] for `opts.region`, with no `opts` read as no region.
pub(crate) fn opts_region(
    backend: &dyn Backend,
    opts: Option<&Table>,
    fname: &str,
) -> mlua::Result<std::result::Result<region::ScreenRect, String>> {
    let v = match opts {
        Some(o) => o.get::<mlua::Value>("region")?,
        None => mlua::Value::Nil,
    };
    region_arg(backend, &v, fname, "opts.region")
}

/// `pixel(x, y)`'s screen coordinates, converted exactly as they always were (a fraction cut
/// toward zero, a numeral string accepted). What does not convert is not a number, or one
/// outside the 32-bit range every coordinate lives in (`1e10`), and raises naming it.
fn pixel_coords(lua: &Lua, x: &mlua::Value, y: &mlua::Value) -> mlua::Result<(i32, i32)> {
    let coord = |v: &mlua::Value, name: &str| {
        <i32 as mlua::FromLua>::from_lua(v.clone(), lua).map_err(|_| {
            mlua::Error::external(format!(
                "host.screen.pixel: {name} must be a number within the coordinate range (-2147483648 to 2147483647), got {}",
                describe_value(v)
            ))
        })
    };
    Ok((coord(x, "x")?, coord(y, "y")?))
}

/// `save` and `saveMarked` write PNG only, and refuse any other path before anything is read:
/// a wrong extension is the caller's mistake whatever the screen holds, so it raises even when
/// the capture would have failed, as every other argument of theirs does. Matched without
/// regard to case, as the image encoder matched it when it was the one to refuse.
fn png_path_only(fname: &str, full: &std::path::Path) -> mlua::Result<()> {
    if full.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        return Ok(());
    }
    Err(mlua::Error::external(format!(
        "{fname}: '{}' does not end in .png; PNG is the one image format the host writes",
        full.display()
    )))
}

/// The reason given when a capture came back smaller than it said it was — which a backend
/// should never do, and a binding must not index past.
pub(crate) const SHORT_CAPTURE: &str = "the capture came back shorter than its own size";

/// The first words of `profile`'s reason for a region with no width or height, which it has
/// always refused before capturing; the size follows in brackets, `(0x20)`.
pub(crate) const EMPTY_REGION: &str = "the region is empty";

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

// ---- host.screen.cells, matchCells and matchCellsAsync: their arguments and answers --------
//
// STRICT, where the older screen calls read their options loosely, and on purpose: a cell
// signature is only worth anything if it is read from exactly the pixels it was recorded from,
// so a misspelt key that silently fell back to a default — a region reading down to the bottom
// of the screen, a grid of the wrong size — would give a module confident, wrong answers. What
// raises is a mistake in the call; what the screen or the window does at run time (an empty
// client area, a capture that failed) is `nil, reason`, never an error dialog over a game.
//
// The options and the states are read with serde through mlua's deserializer, so a wrong key
// or type is reported with its path (`states[3].cells`, 1-based like Luau); the region is read
// by the Region form's one strict reader (`region_lua.rs`), which `host.ocr.recognize` shares.

/// A value as a message names it: numbers as themselves, anything else by its type. Shared by
/// every strict reader, the Region form's (`region_lua.rs`) among them.
pub(crate) fn describe_value(v: &mlua::Value) -> String {
    match v {
        mlua::Value::Integer(i) => i.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        mlua::Value::String(s) => format!("the string {:?}", s.to_string_lossy()),
        mlua::Value::Nil => "nothing".to_string(),
        other => format!("a {}", crate::json::luau_type(other)),
    }
}

/// A whole number for serde, with the platform's word for it in the error rather than `u32`.
struct WholeIn(i64);

impl<'de> serde::Deserialize<'de> for WholeIn {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = WholeIn;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a whole number")
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<WholeIn, E> {
                Ok(WholeIn(v))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<WholeIn, E> {
                i64::try_from(v).map(WholeIn).map_err(|_| E::invalid_value(serde::de::Unexpected::Unsigned(v), &self))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<WholeIn, E> {
                if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15 {
                    Ok(WholeIn(v as i64))
                } else {
                    Err(E::invalid_value(serde::de::Unexpected::Float(v), &self))
                }
            }
        }
        d.deserialize_any(V)
    }
}

/// `opts` of the three cells calls, minus the region and the snapshot (read by hand).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CellsOptsIn {
    /// Only its presence: `read_cells_opts` puts `true` here for the region it reads itself.
    #[serde(rename = "region")]
    _region: serde::de::IgnoredAny,
    cols: WholeIn,
    rows: WholeIn,
    predicate: String,
    /// Only its name, so a misspelt key's message lists it among the keys it expected:
    /// `read_cells_opts` puts `true` here for the snapshot `snapshot::from_value` reads.
    #[serde(rename = "snapshot", default)]
    _snapshot: Option<serde::de::IgnoredAny>,
}

/// Hex text, from a Luau string whatever its bytes; `cells::from_hex` judges it.
struct HexIn(Vec<u8>);

impl<'de> serde::Deserialize<'de> for HexIn {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl serde::de::Visitor<'_> for V {
            type Value = HexIn;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a hex string")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<HexIn, E> {
                Ok(HexIn(v.as_bytes().to_vec()))
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> std::result::Result<HexIn, E> {
                Ok(HexIn(v.to_vec()))
            }
        }
        d.deserialize_any(V)
    }
}

/// One stored state: hex, or `{ cells = hex, name = string }`.
enum StateIn {
    Bare(Vec<u8>),
    Named { cells: Vec<u8>, name: Option<String> },
}

impl<'de> serde::Deserialize<'de> for StateIn {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = StateIn;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a hex string, or a table { cells = hex, name = string }")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<StateIn, E> {
                Ok(StateIn::Bare(v.as_bytes().to_vec()))
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> std::result::Result<StateIn, E> {
                Ok(StateIn::Bare(v.to_vec()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut m: A) -> std::result::Result<StateIn, A::Error> {
                let (mut cells, mut name) = (None, None);
                while let Some(k) = m.next_key::<String>()? {
                    match k.as_str() {
                        "cells" => cells = Some(m.next_value::<HexIn>()?.0),
                        "name" => name = Some(m.next_value::<String>()?),
                        other => return Err(serde::de::Error::unknown_field(other, &["cells", "name"])),
                    }
                }
                let cells = cells.ok_or_else(|| serde::de::Error::missing_field("cells"))?;
                Ok(StateIn::Named { cells, name })
            }
        }
        d.deserialize_any(V)
    }
}

/// A serde error as a Luau author reads it: the path from `root`, 1-based, then the reason.
fn serde_message(root: &str, e: serde_path_to_error::Error<mlua::Error>) -> String {
    use std::fmt::Write as _;
    let mut path = root.to_string();
    for seg in e.path().iter() {
        match seg {
            serde_path_to_error::Segment::Seq { index } => {
                let _ = write!(path, "[{}]", index + 1);
            }
            serde_path_to_error::Segment::Map { key } | serde_path_to_error::Segment::Enum { variant: key } => {
                let _ = write!(path, ".{key}");
            }
            serde_path_to_error::Segment::Unknown => {}
        }
    }
    let why = match e.into_inner() {
        mlua::Error::DeserializeError(m) => m,
        other => other.to_string(),
    };
    format!("{path}: {why}")
}

/// A predicate that did not parse, with where and in what.
fn predicate_error(fname: &str, what: &str, src: &str, e: &cells::PredError) -> mlua::Error {
    let shown: String = if src.chars().count() > 80 {
        src.chars().take(79).chain(std::iter::once('…')).collect()
    } else {
        src.to_string()
    };
    mlua::Error::external(format!("{fname}: {what}: {} (column {} of \"{shown}\")", e.message, e.column))
}

/// What the three cells calls read from `opts`.
struct CellsCall {
    /// Where to read — or why there is nothing to read this time, which the call answers as
    /// `nil, reason` (an empty client area; on a snapshot, a region not wholly inside it).
    rect: std::result::Result<region::ScreenRect, String>,
    spec: cells::CellSpec,
    /// The snapshot to read instead of the screen, when `opts.snapshot` gave one.
    snapshot: Option<Arc<backend::frame::Frame>>,
}

/// Reads `opts = { region, cols, rows, predicate, snapshot? }`, raising for every mistake in it.
fn read_cells_opts(lua: &Lua, fname: &str, opts: mlua::Value) -> mlua::Result<CellsCall> {
    let err = |m: String| mlua::Error::external(format!("{fname}: {m}"));
    let mlua::Value::Table(t) = opts else {
        return Err(err(format!(
            "opts must be a table {{ region, cols, rows, predicate, snapshot? }}, got {}",
            describe_value(&opts)
        )));
    };
    // serde reads everything but the region, which `region_lua::read` reads below. It stands
    // in as `true`, so a missing region is reported like any other missing option. The snapshot
    // is read by `snapshot::from_value`; it stands in as `true` too, so serde knows its name.
    let copy = lua.create_table()?;
    let mut region_value = mlua::Value::Nil;
    let mut snapshot_value = mlua::Value::Nil;
    for pair in t.pairs::<mlua::Value, mlua::Value>() {
        let (k, v) = pair?;
        if matches!(&k, mlua::Value::String(s) if s.as_bytes().as_ref() == b"region") {
            region_value = v;
            copy.raw_set(k, true)?;
        } else if matches!(&k, mlua::Value::String(s) if s.as_bytes().as_ref() == b"snapshot") {
            snapshot_value = v;
            copy.raw_set(k, true)?;
        } else {
            copy.raw_set(k, v)?;
        }
    }
    let o: CellsOptsIn = serde_path_to_error::deserialize(mlua::serde::Deserializer::new(mlua::Value::Table(copy)))
        .map_err(|e| err(serde_message("opts", e)))?;
    for (name, n) in [("cols", o.cols.0), ("rows", o.rows.0)] {
        if !(1..=cells::MAX_SIDE as i64).contains(&n) {
            return Err(err(format!("opts.{name} is {n}; it must be from 1 to {}", cells::MAX_SIDE)));
        }
    }
    let pred = cells::Predicate::parse(&o.predicate)
        .map_err(|e| predicate_error(fname, "opts.predicate", &o.predicate, &e))?;
    let spec = cells::CellSpec::new(o.cols.0 as u32, o.rows.0 as u32, pred).map_err(|m| err(format!("opts: {m}")))?;
    // The Region form's one strict reader, shared with `host.ocr.recognize`: a mistake raises, an
    // empty client area is this call's `nil, reason`.
    let given = region_lua::read(&region_value, "opts.region").map_err(err)?;
    let mut rect = given.resolve().map_err(|u| u.to_string());
    if let Ok(r) = &rect {
        let px = r.w as u64 * r.h as u64;
        if px > cells::MAX_REGION_PIXELS {
            let why = format!("the region is {}x{}, {px} pixels; the limit is {}", r.w, r.h, cells::MAX_REGION_PIXELS);
            match given {
                // Corners are the module's own: a mistake in the call.
                region::Region::Rect(_) => return Err(err(why)),
                // A window region's size is the window's, at run time: answered, like an empty
                // client area, so the same call cannot raise only when the window is large.
                region::Region::Window(..) => rect = Err(format!("at the window's current size {why}")),
            }
        }
    }
    // On a snapshot the region must lie wholly inside it: cut to fit, every block of the grid
    // would move and the cells would be a different signature.
    let snapshot = snapshot::from_value(&snapshot_value, fname, "opts.snapshot")?;
    if let (Some(frame), Ok(r)) = (&snapshot, &rect) {
        if let Err(why) = snapshot::inside(frame, *r) {
            rect = Err(why);
        }
    }
    Ok(CellsCall { rect, spec, snapshot })
}

/// Reads the cells of `r` from a snapshot, where the region lies in it — no capture, not
/// counted in the observation line. `r` is inside the snapshot (`read_cells_opts` saw to it).
fn cells_of_snapshot<T>(
    frame: &backend::frame::Frame,
    r: region::ScreenRect,
    reduce: impl FnOnce(&backend::CapturedImage, cells::Sub) -> std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    match frame.area(ocr::types::Rect::new(r.x, r.y, r.w, r.h)) {
        Some(a) => reduce(&frame.img, cells::Sub { x: a.x, y: a.y, w: a.w, h: a.h }),
        None => Err(snapshot::NOT_INSIDE.to_string()),
    }
}

/// The states a match compares against, decoded, with their names.
struct StatesIn {
    bytes: Vec<Box<[u8]>>,
    names: Vec<Option<String>>,
}

/// Reads `states`: a list of 1 to 1024 entries, each hex of exactly two digits per cell or
/// `{ cells = hex, name = string }`. Raises for anything else.
fn read_states(fname: &str, v: mlua::Value, cells_per_state: usize) -> mlua::Result<StatesIn> {
    let err = |m: String| mlua::Error::external(format!("{fname}: {m}"));
    let mlua::Value::Table(t) = &v else {
        return Err(err(format!(
            "states must be a list of hex strings or {{ cells, name }} tables, got {}",
            describe_value(&v)
        )));
    };
    let len = t.raw_len();
    if len == 0 {
        return Err(err("states is empty; give at least one".to_string()));
    }
    if t.clone().pairs::<mlua::Value, mlua::Value>().count() != len {
        return Err(err("states must be a list (1, 2, 3, …) with nothing else in it".to_string()));
    }
    if len > cells::MAX_STATES {
        return Err(err(format!("states has {len} entries; the limit is {}", cells::MAX_STATES)));
    }
    let list: Vec<StateIn> = serde_path_to_error::deserialize(mlua::serde::Deserializer::new(v.clone()))
        .map_err(|e| err(serde_message("states", e)))?;
    let mut out = StatesIn { bytes: Vec::with_capacity(len), names: Vec::with_capacity(len) };
    for (n, s) in list.into_iter().enumerate() {
        let (text, name, at) = match s {
            StateIn::Bare(b) => (b, None, format!("states[{}]", n + 1)),
            StateIn::Named { cells, name } => (cells, name, format!("states[{}].cells", n + 1)),
        };
        let bytes = cells::from_hex(&text, cells_per_state).map_err(|e| err(format!("{at}: {e}")))?;
        out.bytes.push(bytes.into_boxed_slice());
        out.names.push(name);
    }
    Ok(out)
}

/// `{ cells, x, y, w, h }`: the cells as hex and the rectangle actually read.
fn cells_table(lua: &Lua, bytes: &[u8], r: region::ScreenRect) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("cells", cells::to_hex(bytes))?;
    t.set("x", r.x)?;
    t.set("y", r.y)?;
    t.set("w", r.w)?;
    t.set("h", r.h)?;
    Ok(t)
}

/// `cells_table` plus the ranking: `index` (1-based, the winning state), `name`, `similarity`,
/// `distance`, `runnerUp` and `similarities` (one per state). Shared by `matchCells` and the
/// image worker's delivery of `matchCellsAsync`.
pub(crate) fn cells_match_table(
    lua: &Lua,
    bytes: &[u8],
    r: region::ScreenRect,
    ranked: &cells::Ranked,
    names: &[Option<String>],
) -> mlua::Result<Table> {
    let t = cells_table(lua, bytes, r)?;
    t.set("index", ranked.state + 1)?;
    if let Some(Some(name)) = names.get(ranked.state) {
        t.set("name", name.as_str())?;
    }
    t.set("similarity", ranked.similarity)?;
    t.set("distance", ranked.distance)?;
    t.set("runnerUp", ranked.runner_up)?;
    t.set("similarities", lua.create_sequence_from(ranked.all.iter().copied())?)?;
    Ok(t)
}

/// `nil, reason`: the answer of the cells calls when they could not look — two values always,
/// as those calls have returned from the start. The reason is the backend's, in the words the
/// log uses (see "Failure reasons" in docs/api/screen.md).
pub(crate) fn nil_because(lua: &Lua, why: String) -> mlua::Result<(mlua::Value, mlua::Value)> {
    Ok((mlua::Value::Nil, mlua::Value::String(lua.create_string(why)?)))
}

/// What one of the older screen calls — `pixel`, `profile`, `template`, the synchronous image
/// searches, `save`, `saveMarked` — answers when it looked: exactly the one value it has always
/// returned, and nothing after it, so `f(host.screen.pixel(x, y))` and
/// `table.insert(t, host.screen.imageSearch(p))` pass what they always passed.
pub(crate) fn one_value(v: mlua::Value) -> mlua::Result<mlua::MultiValue> {
    Ok(mlua::MultiValue::from_vec(vec![v]))
}

/// What one of those calls answers when it could not look: the value it has always returned
/// then (`nil`, `false`, an empty list), and the reason as ONE value more. A caller that reads
/// only the first value reads what it always did.
pub(crate) fn with_reason(lua: &Lua, first: mlua::Value, why: String) -> mlua::Result<mlua::MultiValue> {
    Ok(mlua::MultiValue::from_vec(vec![first, mlua::Value::String(lua.create_string(why)?)]))
}

/// Captures `r` through this VM's source on the event loop and hands the picture to `reduce`,
/// counted as one screen touch in the observation log and timed to the end of the reduction,
/// as `profile` is: the capture is a fixed frame, and the part that grows with the region is
/// what the log is there to show.
fn read_cells_with<T>(
    sh: &Shared,
    lua: &Lua,
    r: region::ScreenRect,
    reduce: impl FnOnce(&backend::CapturedImage) -> std::result::Result<T, String>,
) -> std::result::Result<T, String> {
    let src = capture_source::read_source(lua, &*sh.backend, (r.x, r.y, r.w, r.h));
    let t0 = Instant::now();
    let out = match sh.backend.capture(r.x, r.y, r.w, r.h, src) {
        Ok(cap) => reduce(&cap),
        Err(why) => Err(why),
    };
    let mut obs = sh.observations();
    obs.pixels += 1;
    obs.pixel_us += t0.elapsed().as_micros();
    out
}

#[cfg(test)]
mod cells_binding_tests {
    use super::*;

    const WARM: &str = "(red >= 80 && red*10 >= green*13 && red*10 >= blue*12) || \
                        (red >= 100 && green >= 45 && blue <= 150 && red >= green && green >= blue)";

    fn opts(lua: &Lua, src: &str) -> mlua::Result<CellsCall> {
        let v: mlua::Value = lua.load(src).eval()?;
        read_cells_opts(lua, "host.screen.cells", v)
    }

    fn opts_err(lua: &Lua, src: &str) -> String {
        match opts(lua, src) {
            Ok(_) => panic!("expected an error from {src}"),
            Err(e) => e.to_string(),
        }
    }

    fn states(lua: &Lua, src: &str, cells: usize) -> mlua::Result<StatesIn> {
        let v: mlua::Value = lua.load(src).eval()?;
        read_states("host.screen.matchCells", v, cells)
    }

    fn states_err(lua: &Lua, src: &str, cells: usize) -> String {
        match states(lua, src, cells) {
            Ok(_) => panic!("expected an error from {src}"),
            Err(e) => e.to_string(),
        }
    }

    /// A window table the way `win_to_table` builds one, with a client area of 1280x1024 at
    /// (100, 50).
    const WIN: &str = "local w = { id = 7, title = 't', class = 'c', app = { name = 'x', exe = 'x.exe', pid = 1 }, \
                       bounds = { x = 92, y = 19, w = 1296, h = 1063 }, client = { x = 100, y = 50, w = 1280, h = 1024 } }";

    #[test]
    fn the_window_form_resolves_with_his_formula() {
        let lua = Lua::new();
        let c = opts(
            &lua,
            &format!("{WIN} return {{ region = {{ window = w, fraction = {{ 0.02, 0.33, 0.09, 0.86 }} }}, cols = 10, rows = 36, predicate = [[{WARM}]] }}"),
        )
        .unwrap();
        assert_eq!(c.rect, Ok(region::ScreenRect { x: 100 + 25, y: 50 + 337, w: 91, h: 544 }));
        assert_eq!((c.spec.cols, c.spec.rows, c.spec.cells()), (10, 36, 360));
        // Named fractions, and his clamps: an end before its start still reads one pixel.
        let c = opts(
            &lua,
            &format!("{WIN} return {{ region = {{ window = w, fraction = {{ x1 = 0.5, y1 = 0.5, x2 = 0.1, y2 = 2 }} }}, cols = 1, rows = 1, predicate = 'r >= 1' }}"),
        )
        .unwrap();
        assert_eq!(c.rect, Ok(region::ScreenRect { x: 100 + 640, y: 50 + 512, w: 1, h: 512 }));
        // An empty client area is `nil, reason`, not an error: a minimised game.
        let c = opts(
            &lua,
            "return { region = { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } }, cols = 2, rows = 2, predicate = 'r >= 1' }",
        )
        .unwrap();
        assert_eq!(c.rect, Err("the window's client area is empty (0x0)".to_string()));
        // So is a window region past the pixel limit: its size is the window's, and corners of
        // the same size raise (every_mistake_in_opts_raises_naming_it).
        let c = opts(
            &lua,
            "return { region = { window = { client = { x = 0, y = 0, w = 10000, h = 5000 } }, fraction = { 0, 0, 1, 1 } }, cols = 2, rows = 2, predicate = 'r >= 1' }",
        )
        .unwrap();
        assert_eq!(
            c.rect,
            Err("at the window's current size the region is 10000x5000, 50000000 pixels; the limit is 40000000".to_string())
        );
        // docs/api/screen.md lists it, with the numbers this call made written `…` (the other
        // reasons are held to the page by image_search.rs, `the_hosts_own_reasons_are_listed`).
        let listed = c.rect.clone().unwrap_err().replacen("10000", "…", 1).replacen("5000", "…", 1).replacen("50000000", "…", 1);
        assert!(
            include_str!("../../../docs/api/screen.md").contains(&format!("`{listed}`")),
            "docs/api/screen.md does not list the reason `{listed}`"
        );
        // A pack's fractions read through `host.json.decode` land where the reader put them:
        // 59/600, written as the shortest text that reads back, starts at column 59, not 58.
        lua.globals().set("decode", lua.create_function(json::decode).unwrap()).unwrap();
        let c = opts(
            &lua,
            "local f = decode('[0.09833333333333333, 0, 1, 1]') \
             return { region = { window = { client = { x = 0, y = 0, w = 600, h = 10 } }, fraction = f }, cols = 1, rows = 1, predicate = 'r >= 1' }",
        )
        .unwrap();
        assert_eq!(c.rect, Ok(region::ScreenRect { x: 59, y: 0, w: 541, h: 10 }));
        // The absolute form, positional and named.
        let c = opts(&lua, "return { region = { 10, 20, 110, 70 }, cols = 2, rows = 2, predicate = 'r >= 1' }").unwrap();
        assert_eq!(c.rect, Ok(region::ScreenRect { x: 10, y: 20, w: 100, h: 50 }));
        let c = opts(&lua, "return { region = { x1 = -10, y1 = 0, x2 = 0, y2 = 5 }, cols = 2, rows = 2, predicate = 'r >= 1' }").unwrap();
        assert_eq!(c.rect, Ok(region::ScreenRect { x: -10, y: 0, w: 10, h: 5 }));
    }

    #[test]
    fn every_mistake_in_opts_raises_naming_it() {
        let lua = Lua::new();
        let base = "cols = 10, rows = 36, predicate = 'r >= 80'";
        for (src, want) in [
            ("return 5".to_string(), "opts must be a table"),
            (format!("return {{ {base} }}"), "missing field `region`"),
            (format!("return {{ region = {{ 0, 0, 10, 10 }}, {base}, rouding = 'even' }}"), "unknown field `rouding`"),
            (
                format!("return {{ region = {{ 0, 0, 10, 10 }}, {base}, snapshots = 1 }}"),
                "unknown field `snapshots`, expected one of `region`, `cols`, `rows`, `predicate`, `snapshot`",
            ),
            (format!("return {{ region = {{ 0, 0, 10, 10 }}, {base}, snapshot = 1 }}"), "opts.snapshot must be a Snapshot, got 1"),
            ("return { region = { 0, 0, 10, 10 }, rows = 36, predicate = 'r >= 80' }".to_string(), "missing field `cols`"),
            ("return { region = { 0, 0, 10, 10 }, cols = 10.5, rows = 36, predicate = 'r >= 80' }".to_string(), "opts.cols: invalid value: floating point `10.5`, expected a whole number"),
            ("return { region = { 0, 0, 10, 10 }, cols = '10', rows = 36, predicate = 'r >= 80' }".to_string(), "opts.cols: invalid type: string"),
            ("return { region = { 0, 0, 10, 10 }, cols = 0, rows = 36, predicate = 'r >= 80' }".to_string(), "opts.cols is 0; it must be from 1 to 256"),
            ("return { region = { 0, 0, 10, 10 }, cols = 100, rows = 41, predicate = 'r >= 80' }".to_string(), "100x41 is 4100 cells; the limit is 4096"),
            ("return { region = { 0, 0, 10, 10 }, cols = 10, rows = 36, predicate = 5 }".to_string(), "opts.predicate: invalid type"),
            ("return { region = { 0, 0, 10, 10 }, cols = 10, rows = 36, predicate = 'r >= 80 and x > 3' }".to_string(), "opts.predicate: 'x' is not a channel; use r, g or b (or red, green, blue) (column 13 of \"r >= 80 and x > 3\")"),
            ("return { region = { 0, 0, 10, 10 }, cols = 10, rows = 36, predicate = 'r >= 080' }".to_string(), "opts.predicate: '080' has a leading zero, which C and JavaScript can read as octal; write 80 (column 6"),
            (format!("return {{ region = 'screen', {base} }}"), "opts.region must be a table"),
            (format!("return {{ region = {{ 0, 0, 10 }}, {base} }}"), "opts.region.y2 is missing"),
            (format!("return {{ region = {{ x1 = 0, y1 = 0, x2 = 10, yy2 = 10 }}, {base} }}"), "opts.region has a key 'yy2'"),
            (format!("return {{ region = {{ 0, 0, 10, 10, x1 = 0 }}, {base} }}"), "mixes named and positional"),
            (format!("return {{ region = {{ 0, 0, 10.5, 10 }}, {base} }}"), "opts.region.x2 must be a whole number, got 10.5"),
            (format!("return {{ region = {{ 10, 0, 10, 10 }}, {base} }}"), "is empty or turned around"),
            (format!("return {{ region = {{ x = 0, y = 0, w = 10, h = 10 }}, {base} }}"), "has a key"),
            (format!("{WIN} return {{ region = {{ window = w, fraction = {{ 0, 0, 1, 1 }}, x1 = 3 }}, {base} }}"), "a key 'x1' beside window and fraction"),
            (format!("return {{ region = {{ window = 3, fraction = {{ 0, 0, 1, 1 }} }}, {base} }}"), "opts.region.window must be a window table"),
            (format!("return {{ region = {{ window = {{}}, fraction = {{ 0, 0, 1, 1 }} }}, {base} }}"), "has no client table"),
            (format!("return {{ region = {{ window = {{ client = {{ x = 0, y = 0, w = 1.5, h = 3 }} }}, fraction = {{ 0, 0, 1, 1 }} }}, {base} }}"), "opts.region.window.client.w must be a whole number, got 1.5"),
            (format!("{WIN} return {{ region = {{ window = w }}, {base} }}"), "opts.region.fraction must be a table"),
            (format!("{WIN} return {{ region = {{ window = w, fraction = {{ 0, 0, 1/0, 1 }} }}, {base} }}"), "opts.region.fraction.x2 is inf, not a finite number"),
            (format!("{WIN} return {{ region = {{ window = w, fraction = {{ 0, 0, '1', 1 }} }}, {base} }}"), "opts.region.fraction.x2 must be a number"),
            (format!("return {{ region = {{ 0, 0, 10000, 10000 }}, {base} }}"), "100000000 pixels; the limit is 40000000"),
        ] {
            let e = opts_err(&lua, &src);
            assert!(e.contains("host.screen.cells: "), "{src}: {e}");
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
    }

    #[test]
    fn states_are_hex_named_or_bare_and_strictly_read() {
        let lua = Lua::new();
        let s = states(&lua, "return { 'a0ff', { cells = 'A0FE', name = 'Start' }, { cells = '0000' } }", 2).unwrap();
        assert_eq!(s.bytes, vec![vec![0xa0, 0xff].into_boxed_slice(), vec![0xa0, 0xfe].into(), vec![0, 0].into()]);
        assert_eq!(s.names, vec![None, Some("Start".to_string()), None]);
        for (src, want) in [
            ("return 'a0ff'", "states must be a list"),
            ("return {}", "states is empty"),
            ("return { 'a0ff', n = 1 }", "with nothing else in it"),
            ("return { 'a0f' }", "states[1]: 3 characters; this grid's states are 4 hex digits"),
            ("return { 'a0ff', 'zz00' }", "states[2]: 'z' at character 1 is not a hex digit"),
            ("return { 'a0ff', '\\255\\1' }", "states[2]: states are hex (4 characters)"),
            // Counted in characters, and the character itself, not its first byte as Latin-1.
            ("return { 'a0ff', 'a0é' }", "states[2]: 'é' at character 3 is not a hex digit"),
            ("return { 'a0ff', 'a\\255ff' }", "states[2]: byte 2 (0xFF) is not a hex digit, and the state is not text"),
            ("return { { cells = 'a0ff', name = 'x' }, { cels = 'a0ff' } }", "states[2]: unknown field `cels`, expected `cells` or `name`"),
            ("return { { name = 'x' } }", "states[1]: missing field `cells`"),
            ("return { { cells = 'a0ff', name = 5 } }", "states[1].name: invalid type"),
            ("return { 5 }", "states[1]: invalid type: integer `5`, expected a hex string, or a table"),
            ("return { { cells = 'a0' } }", "states[1].cells: 2 characters"),
        ] {
            let e = states_err(&lua, src, 2);
            assert!(e.contains("host.screen.matchCells: "), "{src}: {e}");
            assert!(e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
        let many = format!("return {{ {} }}", vec!["'a0ff'"; cells::MAX_STATES + 1].join(", "));
        assert!(states_err(&lua, &many, 2).contains("the limit is 1024"));
    }

    /// The answer's shape, and hex surviving the two conversions that break raw bytes: the
    /// legacy export's `from_value::<serde_json::Value>` and a string setting's `to_str`.
    #[test]
    fn the_answer_is_plain_data() {
        let lua = Lua::new();
        let r = region::ScreenRect { x: 5, y: 6, w: 7, h: 8 };
        let live = vec![0x80u8, 0x01];
        let states: Vec<Box<[u8]>> = vec![vec![0x80, 0x01].into(), vec![0, 0].into()];
        let names = vec![Some("Start".to_string()), None];
        let ranked = cells::rank(&live, &states, &cells::items_of(&names)).unwrap();
        let t = cells_match_table(&lua, &live, r, &ranked, &names).unwrap();
        assert_eq!(t.get::<String>("cells").unwrap(), "8001");
        assert_eq!((t.get::<i32>("x").unwrap(), t.get::<i32>("w").unwrap()), (5, 7));
        assert_eq!(t.get::<i64>("index").unwrap(), 1);
        assert_eq!(t.get::<String>("name").unwrap(), "Start");
        assert_eq!(t.get::<f64>("similarity").unwrap(), 1.0);
        assert_eq!(t.get::<u64>("distance").unwrap(), 0);
        assert_eq!(t.get::<f64>("runnerUp").unwrap(), 1.0 - 129.0 / 510.0);
        assert_eq!(t.get::<Table>("similarities").unwrap().raw_len(), 2);
        let json = lua.from_value::<serde_json::Value>(mlua::Value::Table(t.clone())).expect("exportable");
        assert_eq!(json["cells"], "8001");
        let s: mlua::String = t.get("cells").unwrap();
        assert_eq!(s.to_str().unwrap().to_string(), "8001");
        // No name for an unnamed winner.
        let ranked = cells::rank(&[0, 0], &states, &cells::items_of(&names)).unwrap();
        let t = cells_match_table(&lua, &[0, 0], r, &ranked, &names).unwrap();
        assert_eq!(t.get::<i64>("index").unwrap(), 2);
        assert!(t.get::<mlua::Value>("name").unwrap().is_nil());
    }
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
            // A dot-folder is not a module: an install's staging folder (registry.rs,
            // `install_one`), or a system's hidden folder. The same rule as the installed list.
            if p.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
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

/// Where `host.element.focusStep` landed, as Lua sees it: `{ name, ctype, index, count,
/// bounds? }`.
///
/// `bounds` is `{ x, y, w, h }` in screen coordinates and is left OUT — not zeros, unlike a
/// dump's — when the platform gave no rectangle or an empty one ([`backend::FocusBounds::new`]).
/// A dump's zeros are a row in a diagnostic listing; this one is read by code that acts on it,
/// and a rectangle of zeros handed to an OCR read raises there, where a missing one is a plain
/// `if` at the caller. Separate from the binding so the shape is tested without a backend.
fn focus_step_table(lua: &Lua, step: backend::FocusStep) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("name", step.name)?;
    t.set("ctype", step.ctype)?;
    t.set("index", step.index)?;
    t.set("count", step.count)?;
    if let Some(r) = step.bounds {
        let b = lua.create_table()?;
        b.set("x", r.x)?;
        b.set("y", r.y)?;
        b.set("w", r.w)?;
        b.set("h", r.h)?;
        t.set("bounds", b)?;
    }
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

/// `host.window.foreground()`, over whatever `read` asks — the backend in the host, a script
/// in the tests below.
///
/// **Asked on every call, never kept.** Everything else that says which window is in front
/// (`active`, `focusChain`) is kept for the epoch, and an epoch is not turned over by a
/// `host.timer.every` tick or by a window that hides without an event. That is how a
/// diagnostic run of 2026-09-27 came to report Komplete Kontrol's menu window as in front
/// after it had hidden: the answer was from before. A question whose whole point is
/// such a change cannot be answered from a store that only an event refreshes, so this one
/// has none, and what it asks is cheap enough not to need one: local on Windows, two
/// accessibility reads on macOS (see `Backend::foreground_window`).
///
/// Nothing noted as acting either: it reads, it does not act.
fn foreground_fn(lua: &Lua, read: impl Fn() -> Option<backend::Foreground> + 'static) -> mlua::Result<Function> {
    lua.create_function(move |lua, ()| {
        host_call!("host.window.foreground");
        let t = Instant::now();
        let f = read();
        slow_observation("window.foreground", "", t);
        let Some(f) = f else { return Ok(None) };
        let out = lua.create_table()?;
        out.set("id", f.id)?;
        out.set("pid", f.pid)?;
        out.set("shown", f.shown)?;
        Ok(Some(out))
    })
}

#[cfg(test)]
mod foreground_binding_tests {
    use super::ocr_wiring_tests::{bindings, body};
    use super::*;

    /// The shape a module sees, and that every call asks: three answers in one epoch are three
    /// readings, a hidden window included, and nothing in front is `nil`.
    #[test]
    fn every_call_asks_and_the_answer_is_id_pid_and_shown_or_nil() {
        let lua = Lua::new();
        let script = Rc::new(RefCell::new(vec![
            Some(backend::Foreground { id: 8327976, pid: 4242, shown: true }),
            Some(backend::Foreground { id: 8327976, pid: 4242, shown: false }),
            None,
        ]));
        let asked = Rc::new(Cell::new(0));
        let (s, a) = (script.clone(), asked.clone());
        let f = foreground_fn(&lua, move || {
            a.set(a.get() + 1);
            s.borrow_mut().remove(0)
        })
        .unwrap();
        lua.globals().set("foreground", f).unwrap();
        let seen: String = lua
            .load(
                r#"
                local out = {}
                for i = 1, 3 do
                  -- Arguments are ignored, as window.md says.
                  local f = if i == 2 then foreground("ignored", 42) else foreground()
                  if f == nil then
                    out[#out + 1] = "nil"
                  else
                    local keys = {}
                    for k in pairs(f) do keys[#keys + 1] = k end
                    table.sort(keys)
                    out[#out + 1] = table.concat(keys, ",") .. "=" .. tostring(f.id) .. "/"
                      .. tostring(f.pid) .. "/" .. tostring(f.shown) .. "/" .. type(f.shown)
                  end
                end
                return table.concat(out, " ")
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(
            seen,
            "id,pid,shown=8327976/4242/true/boolean id,pid,shown=8327976/4242/false/boolean nil"
        );
        assert_eq!(asked.get(), 3, "asked on every call");
    }

    /// The binding in `install_host_api` asks the backend's `foreground_window` and nothing
    /// else: no observation cache, which would hand a tick the answer from before a window hid,
    /// and no epoch turnover, which is for calls that act.
    #[test]
    fn the_binding_reads_the_backend_fresh_and_turns_nothing_over() {
        const LIB: &str = include_str!("lib.rs");
        let api = body(LIB, "fn install_host_api(");
        let (_, text) = bindings(api, "win").into_iter().find(|b| b.0 == "foreground").expect("host.window.foreground");
        let text = text.split("// host.window.controls").next().unwrap();
        assert!(text.contains("foreground_fn(lua, move || sh.backend.foreground_window())"), "{text}");
        for not in ["observations", "bump_epoch", "bump_input_epoch", "note_acting"] {
            assert!(!text.contains(not), "host.window.foreground reaches `{not}`: {text}");
        }
        let f = body(LIB, "fn foreground_fn(");
        for not in ["observations", "bump_epoch", "note_acting"] {
            assert!(!f.contains(not), "foreground_fn reaches `{not}`");
        }
    }
}

/// `host.element.focusStep`'s answer as a module sees it. The binding itself needs a `Shared`,
/// whose speech engines a test must not open, and every backend's focus step moves a real
/// keyboard focus — so the table is built by `focus_step_table`, tested here with the backend's
/// answer written out, and the binding is checked to go through it.
#[cfg(test)]
mod focus_step_binding_tests {
    use super::ocr_wiring_tests::{bindings, body};
    use super::*;

    fn shape(step: backend::FocusStep) -> String {
        let lua = Lua::new();
        let t = focus_step_table(&lua, step).unwrap();
        lua.globals().set("r", t).unwrap();
        lua.load(
            r#"
            local keys = {}
            for k in pairs(r) do keys[#keys + 1] = k end
            table.sort(keys)
            local b = r.bounds
            return table.concat(keys, ",") .. "=" .. r.name .. "/" .. r.ctype .. "/" .. r.index
              .. "/" .. r.count .. (b and string.format(" at %s,%s %sx%s", tostring(b.x),
              tostring(b.y), tostring(b.w), tostring(b.h)) or " nowhere")
            "#,
        )
        .eval()
        .unwrap()
    }

    /// With a rectangle: `bounds = { x, y, w, h }` beside the four fields it always had, whole
    /// numbers as whole numbers (Windows), a point's fraction kept (macOS).
    #[test]
    fn the_landed_elements_rectangle_is_its_bounds() {
        let step = |bounds| backend::FocusStep { name: "FILE".into(), ctype: 50000, index: 3, count: 29, bounds };
        assert_eq!(
            shape(step(backend::FocusBounds::new(812.0, 240.0, 64.0, 22.0))),
            "bounds,count,ctype,index,name=FILE/50000/3/29 at 812,240 64x22"
        );
        assert_eq!(
            shape(step(backend::FocusBounds::new(100.5, 60.0, 31.5, 18.0))),
            "bounds,count,ctype,index,name=FILE/50000/3/29 at 100.5,60 31.5x18"
        );
    }

    /// Without one the field is absent — not a table of zeros — and an unnamed stop's name is
    /// the empty string, not nil.
    #[test]
    fn no_rectangle_is_no_bounds_field() {
        let step = backend::FocusStep { name: String::new(), ctype: 50033, index: 15, count: 29, bounds: None };
        assert_eq!(shape(step), "count,ctype,index,name=/50033/15/29 nowhere");
    }

    /// The one rule both backends go through: empty, turned around or not finite is no rectangle.
    #[test]
    fn an_empty_or_unreadable_rectangle_is_none() {
        assert!(backend::FocusBounds::new(0.0, 0.0, 1.0, 1.0).is_some());
        assert!(backend::FocusBounds::new(-1920.0, -40.0, 30.0, 20.0).is_some(), "left of the primary display is a place");
        for (x, y, w, h) in [
            (10.0, 10.0, 0.0, 20.0),
            (10.0, 10.0, 20.0, 0.0),
            (10.0, 10.0, -5.0, 20.0),
            (10.0, 10.0, 20.0, -5.0),
            (f64::NAN, 10.0, 20.0, 20.0),
            (10.0, f64::INFINITY, 20.0, 20.0),
            (10.0, 10.0, f64::NAN, 20.0),
        ] {
            assert_eq!(backend::FocusBounds::new(x, y, w, h), None, "{x},{y} {w}x{h}");
        }
    }

    /// The binding hands the backend's answer to `focus_step_table` and nothing else builds it.
    #[test]
    fn the_binding_builds_its_answer_with_focus_step_table() {
        const LIB: &str = include_str!("lib.rs");
        let api = body(LIB, "fn install_host_api(");
        let (_, text) = bindings(api, "uia").into_iter().find(|b| b.0 == "focusStep").expect("host.element.focusStep");
        assert!(text.contains("sh.backend.element_focus_step(hwnd, direction)"), "{text}");
        assert!(text.contains("Some(step) => Ok(Some(focus_step_table(lua, step)?))"), "{text}");
    }
}

/// Where `host.ocr.recognize`'s rules meet the event loop: acting while a picture is out, the
/// interactive lane, dropping a module's reads. None of these places
/// can be reached by a unit test — they are methods of `Shared`, whose speech engines a test must
/// not open — and deleting any one of them left every other test green. So each is checked where
/// it is written: a crude check on the source, which fails loudly when the code it looks for
/// moves, rather than silently when the rule goes.
#[cfg(test)]
mod ocr_wiring_tests {
    use super::{one_value, pixel_coords, png_path_only, region, region_arg_on, with_reason};
    use mlua::Lua;

    const LIB: &str = include_str!("lib.rs");
    const IMAGES: &str = include_str!("image_search.rs");
    const GAMEPAD: &str = include_str!("gamepad_api.rs");
    const SNAP: &str = include_str!("snapshot.rs");
    const OCR_LUA: &str = include_str!("ocr/lua.rs");
    const MAILBOX: &str = include_str!("mailbox.rs");
    const TIMERS: &str = include_str!("timers.rs");

    /// The text of the item that starts with `sig`, up to its closing brace at its own
    /// indentation.
    pub(super) fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let start = src.find(sig).unwrap_or_else(|| panic!("`{sig}` is not in the source any more"));
        let line = src[..start].rfind('\n').map_or(0, |i| i + 1);
        let indent = src[line..].len() - src[line..].trim_start_matches(' ').len();
        let close = format!("\n{}}}\n", " ".repeat(indent));
        let end = src[start..].find(&close).unwrap_or_else(|| panic!("the end of `{sig}` was not found"));
        &src[start..start + end]
    }

    /// Every `table.set("name", …)` in `src`: its name and its text, up to the next one.
    pub(super) fn bindings<'a>(src: &'a str, table: &str) -> Vec<(&'a str, &'a str)> {
        src.split(format!("{table}.set(").as_str())
            .skip(1)
            .map(|seg| {
                let seg = seg.split("host.set(").next().unwrap_or(seg);
                (seg.split('"').nth(1).unwrap_or(""), seg)
            })
            .collect()
    }

    /// Every `host.input` call that acts, and `host.window.focus`, acts at once and notes it
    /// (`note_acting`, tested in task_tests.rs): nothing on its way waits for the module's
    /// pictures, whose answer the module waits for itself. A new `host.input` call has to be put
    /// in one list or the other.
    #[test]
    fn every_acting_call_notes_acting_and_waits_for_no_picture() {
        const ACTING: [&str; 9] = ["move", "click", "post", "mouseDown", "mouseUp", "drag", "scroll", "send", "text"];
        const READ_ONLY: [&str; 1] = ["cursorPos"];
        let api = body(LIB, "fn install_host_api(");
        let input = bindings(api, "input");
        assert_eq!(input.len(), ACTING.len() + READ_ONLY.len(), "{:?}", input.iter().map(|b| b.0).collect::<Vec<_>>());
        for (name, text) in input {
            if ACTING.contains(&name) {
                let call = format!("sh.note_acting(lua, \"host.input.{name}\");");
                assert!(text.contains(&call), "host.input.{name} does not note acting");
            } else {
                assert!(READ_ONLY.contains(&name), "host.input.{name} is new: does it act (ACTING) or only read?");
            }
        }
        let focus = bindings(api, "win").into_iter().find(|b| b.0 == "focus").expect("host.window.focus");
        assert!(focus.1.contains("sh.note_acting(lua, \"host.window.focus\");"), "host.window.focus does not note acting");
        // Snapshots count as reads do, and nothing on the way waits: the read service is only asked
        // whether a picture is still to be taken, a look under its lock.
        const SERVICE: &str = include_str!("ocr/service.rs");
        let shared = body(OCR_LUA, "pub(crate) fn note_acting(&self,");
        assert!(shared.contains("self.snap_state.has_pending_for(o.idx)"), "a module's pending snapshots are left out");
        let note = body(OCR_LUA, "pub(crate) fn note_acting<H: ReadHost>(");
        assert_eq!(note.matches(".ocr()").count(), note.matches(".ocr().picture_due(idx)").count(), "acting asks the service more");
        let due = body(SERVICE, "pub fn picture_due(&self,");
        for not in ["self.ocr.", "wait", "barrier"] {
            assert!(!shared.contains(not) && !note.contains(not) && !due.contains(not), "acting reaches `{not}`");
        }
    }

    /// Both ticks, the GUI one and the headless one, deliver the snapshots' answers.
    #[test]
    fn both_ticks_fire_snapshot_results() {
        let call = concat!("shared.fire_snapshot", "_results();");
        assert!(body(LIB, "fn on_tick(&mut self)").contains(call), "the headless tick does not deliver snapshots");
        let calls = LIB.matches(call).count();
        assert_eq!(calls, 2, "lib.rs delivers snapshots {calls} times; the GUI tick and the headless one each do it once");
        assert!(
            LIB.contains(concat!("snapshot results \\", "\n")) && LIB.contains(concat!("{snapshots_", "ms}, queued module events")),
            "the overrun line does not name the snapshots' share"
        );
    }

    /// A module's snapshot requests are dropped, never delivered, when it is disabled, reloaded
    /// or rolled back; and a pending one keeps the headless loop running.
    #[test]
    fn a_modules_snapshot_requests_go_with_it() {
        assert!(body(LIB, "fn after_toggle(").contains("self.snap_drop_owner(idx);"));
        assert!(body(LIB, "fn purge_module(").contains("self.snap_drop_owner(idx);"));
        assert!(body(LIB, "fn rollback_to(").contains("self.snap_drop_from(n);"));
        assert!(body(LIB, "pub fn run(&mut self)").contains("self.shared.snap_state.has_pending()"));
    }

    /// A snapshot's callback runs in the lane of the dispatch that asked for it, as a text read's
    /// and an image search's do.
    #[test]
    fn a_snapshot_callback_runs_in_its_lane() {
        let fire = body(SNAP, "pub(crate) fn fire_snapshot_results(");
        let lane = fire.find("let _prio = enter_priority(p.prio);").expect("no priority scope");
        let deliver = fire.find("mailbox::deliver(self, idx, &lua, Event::Snapshot").expect("not through the mailbox");
        assert!(lane < deliver, "the mailbox takes the priority the answer arrives with");
        assert!(body(SNAP, "pub(crate) fn snap_async(").contains("prio: current_priority(),"));
        // The mailbox runs every event in the priority it arrived with.
        let run = body(MAILBOX, "fn run<H: MailHost>(");
        assert!(run.find("let _prio = enter_priority(prio);").expect("no priority scope") < run.find("h.open(").unwrap());
        let enqueue = body(MAILBOX, "fn enqueue<H: MailHost>(");
        assert!(enqueue.contains("let prio = current_priority();") && enqueue.contains("Queued { ev, prio }"));
    }

    /// `host.screen.profile` given a callback does nothing on the event loop but read its
    /// arguments and file the request: the capture is the `screen-capture` thread's (a snapshot's
    /// picture is taken already), the reduction the image worker's, and the answer — its tables —
    /// is a handler of its module, handed over in the lane it was asked from and counted in the
    /// observation line. `snapshot.rs` and `image_search.rs` hold what each step does; this holds
    /// that the steps are where they are.
    #[test]
    fn a_profile_with_a_callback_captures_and_reduces_off_the_event_loop() {
        let api = body(LIB, "fn install_host_api(");
        let screen = bindings(api, "screen");
        let binding = screen.iter().find(|b| b.0 == "profile").expect("host.screen.profile").1;
        let handed = binding.find("sh.profile_async(lua, idx, cb, ").expect("the callback form");
        let returned = binding[handed..].find("return Ok(mlua::MultiValue::new());").expect("and returns at once") + handed;
        assert!(returned < binding.find("sh.backend.capture(").unwrap(), "the callback form captures on the loop");
        assert!(returned < binding.find("profile::profile(").unwrap(), "the callback form reduces on the loop");
        let f = body(SNAP, "pub(crate) fn profile_async(");
        assert!(!f.contains("profile::profile(") && !f.contains("backend.capture("), "profile_async captures or reduces");
        assert!(f.contains("self.ocr.submit_snap(req);"), "a region's picture is the capture thread's");
        assert!(f.contains("Some(frame) => self.reduce(p, frame),"), "a snapshot goes straight to the worker");
        assert!(f.contains("prio: current_priority(),"));
        let fire = body(SNAP, "pub(crate) fn fire_snapshot_results(");
        let on = fire.find("Answer::Picture { frame, .. } if p.profile.is_some() => self.reduce(p, frame),").expect("a profile's picture goes on");
        assert!(on < fire.find("mailbox::deliver(").unwrap(), "before anything is delivered");
        assert!(body(SNAP, "fn reduce(").contains("self.image_tasks.send(sent)"));
        assert!(body(IMAGES, "pub(crate) fn fire_image_results(").contains("self.deliver_reduced(p, reduced);"));
        let deliver = body(SNAP, "pub(crate) fn deliver_reduced(");
        let lane = deliver.find("let _prio = enter_priority(p.prio);").expect("no priority scope");
        assert!(lane < deliver.find("mailbox::deliver(self, idx, &lua, Event::Snapshot").expect("not through the mailbox"));
        assert!(body(SNAP, "pub(crate) fn open_snapshot(").contains("self.observations().profile_cb.add("));
        assert!(body(IMAGES, "fn profile_task(").contains("let Haystack::Frame { frame, region } = &t.hay else"), "the worker captures for it");
        // host.screen.pending, beside them.
        let pending = screen.iter().find(|b| b.0 == "pending").expect("host.screen.pending").1;
        assert!(pending.contains("snapshot::pending(lua, shared, idx)"));
    }

    /// Without a callback, `profile` refuses `key` and `at` (snapshot.rs, `callback_only`, tested
    /// there): once the callback form has gone its way, so that form still reads both, and before
    /// anything is answered or captured.
    #[test]
    fn a_profile_without_a_callback_refuses_key_and_at_before_it_answers() {
        let api = body(LIB, "fn install_host_api(");
        let binding = bindings(api, "screen").into_iter().find(|b| b.0 == "profile").expect("host.screen.profile").1;
        let refused = binding.find("snapshot::callback_only(opts.as_ref(), F, &[\"key\", \"at\"])?;").expect("key and at refused");
        assert!(binding.find("return Ok(mlua::MultiValue::new());").expect("the callback form") < refused);
        assert!(refused < binding.find("Err(why) => return with_reason(").unwrap(), "after a reason was answered");
        assert!(refused < binding.find("sh.backend.capture(").unwrap(), "after a capture");
    }

    /// What `snapshotAsync` does with its charge: reserved by the region's form
    /// (`snapshot::reserve_for`, tested there), and given back when the application's limit
    /// refuses the request.
    #[test]
    fn snap_async_reserves_by_the_form_and_gives_a_refused_charge_back() {
        let f = body(SNAP, "pub(crate) fn snap_async(");
        // The budget by the form the region was given in: a window region's overrun is answered.
        assert!(f.contains("reserve_for(lua, &self.snap_bytes, reservation_for(*r, change), FA, &args.form)?"));
        // A request the application's limit refuses gives its charge back before it is queued.
        let refused = &f[f.find("if let Some(why) = refused {").expect("the refusal")..];
        let queued = refused.find("st.ready.borrow_mut().push((p, why));").expect("queued");
        assert!(refused[..queued].contains("p.res = None;"), "the charge goes back first");
        // The epoch a picture is handed to the module with is the one stamped when it was taken.
        assert!(body(LIB, "fn bump_input_epoch(").contains("self.ocr.note_input_epoch(e);"));
    }

    /// `host.ocr` is `recognize`, `pending`, `languages` and `resolveLanguage`. `recognize` is the
    /// shim's, with its callback form (`ocr::lua::read`) beside it and nothing else: where it
    /// cannot wait, the shim raises, and nothing reads on the event loop.
    #[test]
    fn recognize_is_the_shims_with_its_callback_form_beside_it() {
        let api = body(LIB, "fn install_host_api(");
        let ocr = bindings(api, "ocr");
        let mut names: Vec<&str> = ocr.iter().map(|b| b.0).collect();
        names.sort_unstable();
        assert_eq!(names, ["languages", "pending", "recognize", "resolveLanguage"]);
        let text = |name: &str| ocr.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("host.ocr.{name}")).1;
        assert!(text("recognize").contains("recognize.get::<Function>(\"recognize\")"));
        assert!(api.contains("let recognize = task::waits(lua, idx, shared.clone(), submit)?;"));
        assert!(api.contains("ocr::lua::read(&*sh, lua, idx, what, opts, cb)"), "the callback form");
        assert!(!api.contains("task::legacy("), "a blocking call beside it");
    }

    /// Somebody waiting: every dispatch a person causes runs in the interactive lane, and an
    /// image search's callback in the lane it was asked from.
    #[test]
    fn the_dispatches_a_person_causes_are_interactive() {
        for sig in [
            "fn on_key(&mut self",
            "fn on_hotkey(&mut self",
            "fn on_gamepad(&mut self",
            "fn on_window_activate(&mut self",
            "fn on_focus_change(&mut self",
            "fn dispatch_initial(&self)",
        ] {
            assert!(
                body(LIB, sig).contains("let _prio = ocr::types::enter_priority(ocr::types::Priority::Interactive);"),
                "`{sig}` does not dispatch in the interactive lane"
            );
        }
        assert!(
            body(IMAGES, "fn fire_image_results(").contains("let _prio = crate::ocr::types::enter_priority(p.prio);"),
            "an image search's callback does not run in the lane it was asked from"
        );
        // A controller combination held for its `holdMs` is fired from the tick, not from
        // `on_gamepad`, and is a person waiting all the same.
        assert!(
            body(GAMEPAD, "fn fire_chord_holds(&self)")
                .contains("let _prio = crate::ocr::types::enter_priority(crate::ocr::types::Priority::Interactive);"),
            "a held controller combination does not fire in the interactive lane"
        );
    }

    /// The pads owe the modules two things on a tick rather than on a drain — the `connected`
    /// replays and the combinations held for their `holdMs` — and both ticks, the GUI one and
    /// the headless one, pay them. Removing any one of these calls left every other test green.
    #[test]
    fn both_ticks_fire_the_pads_replays_and_held_combinations() {
        let tick = body(GAMEPAD, "pub(crate) fn fire_pad_tick(&self)");
        assert!(tick.contains("self.fire_pad_replays();"), "the tick no longer replays `connected`");
        assert!(tick.contains("self.fire_chord_holds();"), "the tick no longer fires held combinations");
        // Spelt in two halves, so that this test's own text is not what is found or counted.
        let call = concat!("shared.fire_pad", "_tick();");
        assert!(body(LIB, "fn on_tick(&mut self)").contains(call), "the headless tick does not fire the pads' tick");
        // The GUI tick is a closure, which `body` cannot find by a signature: the calls are
        // counted instead, the headless tick's and the GUI loop's.
        let calls = LIB.matches(call).count();
        assert_eq!(calls, 2, "lib.rs fires the pads' tick {calls} times; the GUI tick and the headless one each do it once");
    }

    /// Every call that takes a region reads it through the Region form's one reader, so every
    /// one of them takes the window form, and none reads a region any other way. A new screen
    /// call that takes a region has to be put in this list.
    #[test]
    fn every_region_goes_through_the_one_reader() {
        assert!(!LIB.contains(concat!("fn read_", "region(")), "the old loose reader is back");
        let api = body(LIB, "fn install_host_api(");
        let screen = bindings(api, "screen");
        let text = |name: &str| screen.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("host.screen.{name}")).1;
        for name in ["profile", "save", "saveMarked"] {
            assert!(text(name).contains("opts_region(&*sh.backend"), "host.screen.{name} reads its region another way");
        }
        assert!(text("pixel").contains("region_lua::read_point("), "host.screen.pixel has no window form");
        for name in ["cells", "matchCells", "matchCellsAsync"] {
            assert!(text(name).contains("read_cells_opts("), "host.screen.{name}");
        }
        assert!(body(LIB, "fn read_cells_opts(").contains("region_lua::read(&region_value"));
        // `recognize`, both its forms: the strict reader, under its name.
        assert!(body(OCR_LUA, "fn region(").contains("region_lua::read(v, what).map_err(bad)"));
        assert!(body(OCR_LUA, "fn bad(").contains("format!(\"host.ocr.recognize: {msg}\")"));
        assert!(body(LIB, "fn region_arg(").contains("region_arg_on("));
        assert!(body(LIB, "fn region_arg_on(").contains("region_lua::read_loose("));
        for f in ["fn image_search(", "fn image_search_multi(", "fn image_search_async(", "fn image_search_each(", "fn image_search_all("] {
            assert!(body(IMAGES, f).contains("search_region(&sh, opts.as_ref()"), "`{f}` reads opts.region another way");
        }
        assert!(body(IMAGES, "fn image_search_each(").contains("region_arg(&*sh.backend, &r, F"), "a `within` read another way");
        assert!(body(IMAGES, "fn parse_spec(").contains("region_lua::read_loose(&v, \"capture\""), "template{{ capture }}");
        assert!(body(IMAGES, "fn search_region(").contains("opts_region("));
        // Each call names ITSELF to the reader, so its mistakes are reported under its own name.
        for name in ["profile", "save", "saveMarked"] {
            assert!(text(name).contains(&format!("\"host.screen.{name}\"")), "host.screen.{name} reads its region under another name");
        }
        for (f, name) in [
            ("fn image_search(", "imageSearch"),
            ("fn image_search_multi(", "imageSearchMulti"),
            ("fn image_search_async(", "imageSearchAsync"),
            ("fn image_search_each(", "imageSearchEach"),
            ("fn image_search_all(", "imageSearchAll"),
        ] {
            assert!(body(IMAGES, f).contains(&format!("\"host.screen.{name}\"")), "`{f}` reads its region under another name");
        }
        assert!(body(IMAGES, "fn image_search_each(").contains("format!(\"entries[{}].within\""));
        // Snapshots: `snapshot` and `crop` read the Region form strictly, `pixels` its points
        // through the one point reader, and every call given a snapshot reads its region through
        // the same loose reader with the snapshot's edges as the defaults.
        assert!(text("snapshot").contains("snapshot::snapshot(lua, shared)"), "host.screen.snapshot");
        assert!(text("pixels").contains("snapshot::pixels(lua, shared)"), "host.screen.pixels");
        assert!(body(SNAP, "fn read_opts(").contains("region_lua::read(&t.get::<Value>(\"region\")?, \"opts.region\")"));
        assert!(body(SNAP, "fn crop(").contains("region_lua::read(v, \"the region\")"));
        assert!(body(SNAP, "fn read_point_list(").contains("region_lua::read_point_any("));
        assert!(body(SNAP, "fn loose_region(").contains("region_lua::read_loose_within("));
        assert!(body(SNAP, "fn opts_region_in(").contains("loose_region("));
        for name in ["profile", "save", "saveMarked"] {
            assert!(text(name).contains("snapshot::opts_region_in("), "host.screen.{name} reads a snapshot's region another way");
        }
        for f in ["fn image_search(", "fn image_search_multi(", "fn image_search_async(", "fn image_search_each(", "fn image_search_all("] {
            assert!(body(IMAGES, f).contains("snapshot_region(opts.as_ref()"), "`{f}` reads a snapshot's region another way");
        }
        assert!(body(IMAGES, "fn snapshot_region(").contains("snapshot::opts_region_in("));
        assert!(body(IMAGES, "fn image_search_each(").contains("snapshot::loose_region(&r, F, &format!(\"entries[{}].within\""));
        assert!(body(IMAGES, "fn parse_spec(").contains("region_lua::read_loose_within(&v, \"capture\""), "template{{ capture, snapshot }}");
        assert!(body(LIB, "fn read_cells_opts(").contains("snapshot::inside(frame, *r)"), "the cells calls cut a snapshot's region");
        // snapshotAsync: its region and each `watch` region strictly, through the one reader.
        assert!(text("snapshotAsync").contains("snapshot::snapshot_async(lua, shared, idx)"), "host.screen.snapshotAsync");
        assert!(body(SNAP, "pub(crate) fn parse_async(").contains("region_lua::read(&region_v, \"opts.region\")"));
        assert!(body(SNAP, "fn read_watch(").contains("region_lua::read(&item, &format!(\"{W}[{i}]\"))"));
    }

    /// `pixel`'s options are read strictly, before anything is answered, and a read of a
    /// snapshot touches no screen: no capture and no count in the observation line, in `pixel`,
    /// `pixels`, `profile` and the cells calls.
    #[test]
    fn pixel_opts_are_strict_and_a_snapshot_read_touches_no_screen() {
        let api = body(LIB, "fn install_host_api(");
        let screen = bindings(api, "screen");
        let text = |name: &str| screen.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("host.screen.{name}")).1;
        let pixel = text("pixel");
        let opts = pixel.find("let snap = snapshot::strict_opts(opts, F)?;").expect("pixel reads its options strictly");
        let answered = pixel.find("return with_reason(").expect("pixel answers nil and why");
        assert!(opts < answered, "pixel reads its options after it may already have answered");
        let branch = pixel.find("if let Some(frame) = snap {").expect("pixel's snapshot branch");
        let live = pixel.find("sh.backend.pixel(").expect("pixel's live read");
        assert!(branch < live && !pixel[branch..live].contains("observations()"), "a snapshot's pixel is counted as a screen read");
        assert!(text("profile").contains("if pic.is_live() {"), "a snapshot's profile is counted as a screen read");
        assert!(!body(LIB, "fn cells_of_snapshot<T>(").contains("observations()"));
        // `pixel(x, y)` takes its options third and `pixel(point)` second; without them the call
        // reaches the live read as it always did: the source for the one pixel, then the backend.
        assert!(pixel.contains("_ => &c,"), "pixel(x, y) reads its options from somewhere else");
        let live = &pixel[branch..];
        let src = live.find("capture_source::read_source(lua, &*sh.backend, (x, y, 1, 1));").expect("pixel's source, for its one pixel");
        let read = live.find("sh.backend.pixel(x, y, src);").expect("pixel's live read");
        assert!(src < read, "pixel reads before it knows its source");
        let read = body(SNAP, "fn read_pixels(");
        let snap = read.find("else if let Some(frame) = snap {").expect("pixels' snapshot branch");
        let live = read.find("match live(&at)").expect("pixels' live read");
        assert!(snap < live && !read.contains("observations()"));
    }

    /// `host.screen.profile` puts each statistic under its own name, in the order the reference
    /// lists them. `profile::tests` holds the values; a swapped key would pass those.
    #[test]
    fn profile_sets_each_statistic_under_its_own_name() {
        let api = body(LIB, "fn install_host_api(");
        let binding = bindings(api, "screen").into_iter().find(|b| b.0 == "profile").expect("host.screen.profile").1;
        assert!(binding.contains("let out = profile_table(lua, "), "host.screen.profile builds its answer elsewhere");
        // A callback's answer is built by the same function.
        assert!(body(SNAP, "fn call_args(").contains("crate::profile_table(&lua, r.rect.tuple(), r.columns, r.rows)?"));
        let profile = body(LIB, "pub(crate) fn profile_table(");
        let mut at = 0;
        for k in ["min", "max", "dark", "mean", "r", "g", "b"] {
            let line = format!("t.set(\"{k}\", lua.create_sequence_from(a.{k})?)?;");
            let i = profile[at..].find(&line).unwrap_or_else(|| panic!("host.screen.profile no longer sets `{k}` from a.{k}, after the ones before it"));
            at += i + line.len();
        }
        for k in ["x", "y", "w", "h", "columns", "rows"] {
            assert!(profile.contains(&format!("out.set(\"{k}\", ")), "host.screen.profile no longer sets `{k}`");
        }
    }

    /// The reader every call above goes through, with a call's name and argument: the window
    /// form resolved, a minimised window answered rather than raised, a table that is neither
    /// form raised with the call and the argument named, no region the whole primary screen,
    /// and corners exactly as the older calls always read them. That each binding hands it its
    /// own name is `every_region_goes_through_the_one_reader`'s to hold.
    #[test]
    fn the_one_reader_reads_both_forms_and_names_a_table_that_is_neither() {
        let lua = Lua::new();
        let screen = (1920, 1080);
        let v = |src: &str| -> mlua::Value { lua.load(src).eval().unwrap() };
        let rect = |x, y, w, h| region::ScreenRect { x, y, w, h };
        let (fname, what) = ("host.screen.imageSearchEach", "entries[2].within");
        let read = |src: &str| region_arg_on(screen, &v(src), fname, what);
        let win = "window = { client = { x = 100, y = 50, w = 1280, h = 1024 } }";
        assert_eq!(
            read(&format!("return {{ {win}, fraction = {{ 0.02, 0.33, 0.09, 0.86 }} }}")).unwrap(),
            Ok(rect(125, 387, 91, 544)),
            "the window form"
        );
        assert_eq!(
            read("return { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } }").unwrap(),
            Err("the window's client area is empty (0x0)".to_string()),
            "a minimised window is answered"
        );
        assert_eq!(read("return nil").unwrap(), Ok(rect(0, 0, 1920, 1080)), "no region");
        assert_eq!(read("return { 10, 20, 110, 70 }").unwrap(), Ok(rect(10, 20, 100, 50)), "corners");
        assert_eq!(read("return { x1 = 10.9, y1 = 20 }").unwrap(), Ok(rect(10, 20, 1910, 1060)), "loose corners");
        for (src, want) in [
            ("return {}", format!("{fname}: {what} is neither {{ x1, y1, x2, y2 }} nor {{ window = w, fraction = {{ x1, y1, x2, y2 }} }}: it is empty")),
            ("return { x = 1, y = 2, w = 3, h = 4 }", format!("{fname}: {what} is neither")),
            ("return 'screen'", format!("{fname}: {what} must be a table")),
            ("return { window = {}, fraction = { 0, 0, 1, 1 } }", format!("{fname}: {what}.window has no client table")),
            (
                "return { window = { client = { x = 0, y = 0, w = 9, h = 9 } }, fraction = { 0, 0, 1, 1 }, x1 = 0 }",
                format!("{fname}: {what} has a key 'x1' beside window and fraction"),
            ),
        ] {
            let e = read(src).expect_err(src).to_string();
            assert!(e.contains(&want), "{src}:\n  got  {e}\n  want {want}");
        }
    }

    /// `save` and `saveMarked` refuse a path that is not a PNG before they read the screen, so
    /// the mistake raises whether or not the capture would have worked.
    #[test]
    fn save_refuses_a_path_that_is_not_png_before_it_captures() {
        for ok in ["shot.png", "calibration/kontakt.PNG", "C:/x/y.Png"] {
            assert!(png_path_only("host.screen.save", std::path::Path::new(ok)).is_ok(), "{ok}");
        }
        for bad in ["shot.jpg", "shot", "shot.png.bak", "folder.png/shot.bmp"] {
            let e = png_path_only("host.screen.saveMarked", std::path::Path::new(bad)).expect_err(bad).to_string();
            assert!(e.contains("host.screen.saveMarked: '") && e.contains("does not end in .png"), "{bad}: {e}");
        }
        let api = body(LIB, "fn install_host_api(");
        let screen = bindings(api, "screen");
        for name in ["save", "saveMarked"] {
            let t = screen.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("host.screen.{name}")).1;
            let check = t.find(&format!("png_path_only(\"host.screen.{name}\", &full)?;")).unwrap_or_else(|| panic!("host.screen.{name} does not check its path"));
            let capture = t.find("sh.backend.capture(").expect("a capture");
            assert!(check < capture, "host.screen.{name} checks its path only after capturing");
            // And before the picture is a snapshot's, too.
            let snap = t.find("snapshot::from_opts(").expect("a snapshot");
            let cut = t.find("frame.crop_image(").expect("a snapshot's picture");
            assert!(check < snap && check < cut, "host.screen.{name} checks its path only after reading a snapshot");
        }
    }

    /// `pixel(x, y)` converts its numbers as it always has — a fraction cut toward zero, a
    /// numeral string accepted — and raises, naming the coordinate and what it got, for
    /// anything else, a number outside the 32-bit range included.
    #[test]
    fn pixel_reads_its_numbers_as_it_always_has_and_names_what_it_refuses() {
        let lua = Lua::new();
        let v = |src: &str| -> mlua::Value { lua.load(format!("return {src}")).eval().unwrap() };
        for (x, y, want) in [("10", "20", (10, 20)), ("1.9", "-2.7", (1, -2)), ("'12'", "'34'", (12, 34))] {
            assert_eq!(pixel_coords(&lua, &v(x), &v(y)).unwrap(), want, "({x}, {y})");
        }
        for (x, y, want) in [
            ("1e10", "0", "x must be a number within the coordinate range (-2147483648 to 2147483647), got 10000000000"),
            ("0", "nil", "y must be a number within the coordinate range (-2147483648 to 2147483647), got nothing"),
            ("true", "0", "x must be a number within the coordinate range (-2147483648 to 2147483647), got a boolean"),
            ("'left'", "0", "got the string \"left\""),
        ] {
            let e = pixel_coords(&lua, &v(x), &v(y)).expect_err(x).to_string();
            assert!(e.contains("host.screen.pixel: ") && e.contains(want), "({x}, {y}): {e}");
        }
        let api = body(LIB, "fn install_host_api(");
        let pixel = bindings(api, "screen").into_iter().find(|b| b.0 == "pixel").expect("host.screen.pixel").1;
        assert!(pixel.contains("pixel_coords(lua, &a, &b)"), "host.screen.pixel reads its numbers another way");
    }

    /// The older calls answer as they always did when they look — ONE value, nothing after it —
    /// and add the reason as one value more when they could not. Held for every one of them,
    /// since a trailing `nil` after a success would change what `f(host.screen.pixel(x, y))` or
    /// `table.insert(t, host.screen.imageSearch(p))` passes on.
    #[test]
    fn the_older_calls_add_a_reason_and_nothing_else() {
        let lua = Lua::new();
        assert_eq!(one_value(mlua::Value::Boolean(true)).unwrap().len(), 1);
        let told = with_reason(&lua, mlua::Value::Nil, "screen capture failed".into()).unwrap().into_vec();
        assert_eq!(told.len(), 2);
        assert!(told[0].is_nil());
        assert_eq!(told[1].as_string().unwrap().to_string_lossy(), "screen capture failed");
        let api = body(LIB, "fn install_host_api(");
        let screen = bindings(api, "screen");
        let text = |name: &str| screen.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("host.screen.{name}")).1;
        for name in ["pixel", "profile", "save", "saveMarked"] {
            let t = text(name);
            assert!(t.contains("one_value(") && t.contains("with_reason(lua, "), "host.screen.{name}");
            assert!(!t.contains("nil_because(") && !t.contains("mlua::Value::Nil))"), "host.screen.{name} answers two values where it answered one");
        }
        for f in ["fn image_search(", "fn image_search_all(", "fn build_handle("] {
            let t = body(IMAGES, f);
            assert!(t.contains("one_value(") && t.contains("with_reason(lua, "), "`{f}`");
            assert!(!t.contains("nil_because("), "`{f}` answers two values where it answered one");
        }
        let multi = body(IMAGES, "fn image_search_multi(");
        assert!(multi.contains("vec![Value::Nil, Value::Nil, reason(lua, why)]"), "imageSearchMulti's reason is the third value");
    }

    /// A module's reads are dropped, never delivered, when it is disabled, reloaded or rolled
    /// back.
    #[test]
    fn a_modules_reads_go_with_it() {
        assert!(body(LIB, "fn after_toggle(").contains("self.ocr_drop_owner(idx, false);"));
        assert!(body(LIB, "fn purge_module(").contains("self.ocr_drop_owner(idx, true);"));
        assert!(body(LIB, "fn rollback_to(").contains("self.ocr_drop_from(n);"));
    }

    /// A module's tasks go with it as its reads do: disabled, reloaded, rolled back. On a disable,
    /// after the arbiter's re-election, so that what an onDeactivate started goes too; on a
    /// reload, a second time after the onDeactivate the purge runs — timers, reads, snapshots and
    /// tasks — so nothing it started reaches the old VM or keeps it in memory.
    #[test]
    fn a_modules_tasks_go_with_it_and_after_its_deactivation() {
        let enable = body(LIB, "fn after_toggle(");
        let resolve = enable.find("self.arbiter_resolve(&slot);").expect("the re-election");
        for drop in ["self.ocr_drop_owner(idx, false);", "self.snap_drop_owner(idx);", "self.task_drop_owner(idx);"] {
            assert!(enable.find(drop).is_some_and(|at| at > resolve), "`{drop}` before the re-election");
        }
        let purge = body(LIB, "fn purge_module(");
        let deactivated = purge.find("for (lua, f) in deactivations {").expect("the onDeactivate calls");
        let second = &purge[deactivated..];
        for drop in [
            "self.timers.retain(|i| i != idx);",
            "self.ocr_drop_owner(idx, true);",
            "self.snap_drop_owner(idx);",
            "self.task_drop_owner(idx);",
        ] {
            assert!(second.contains(drop), "`{drop}` is not done again after the onDeactivate");
            assert!(purge[..deactivated].contains(drop), "`{drop}` is not done before it either");
        }
        assert!(body(LIB, "fn rollback_to(").contains("self.task_drop_from(n);"));
        // The loop is the thread that builds the manager, and the summary is written at exit.
        assert!(body(LIB, "pub fn new() -> Result<Self>").contains("loop_guard::mark_loop_thread();"));
        assert!(body(LIB, "pub fn run(dirs: &[String]) -> Result<()> {").contains("manager.shared.log_legacy_summary();"));
        // The overrun line counts the deliveries' callbacks, and names no task: no module starts one.
        assert!(LIB.contains(concat!("let ocr_n = shared.fire_ocr", "_results();")));
        assert!(LIB.contains(concat!("({} callback(s)), snapshot results ", "\\")));
        assert!(!LIB.contains(concat!("ocr_n", ".tasks")) && !LIB.contains(concat!("ta", "sk(s)")));
        // Every VM is a plain `Lua::new()`, whose option to catch Rust panics is on: mlua then leaves
        // Luau's own `pcall`, inside which a task waits. With it off, mlua puts in a `pcall` that
        // cannot yield, and `pcall(host.ocr.recognize, …)` in a task would stop waiting.
        for options in [concat!("LuaOp", "tions"), concat!("catch_rust", "_panics"), concat!("Lua::new", "_with")] {
            assert!(!LIB.contains(options), "a VM built with options of its own: `{options}`");
        }
    }

    /// A module's key scope and menu flag go with it as its captures do — disabled, reloaded,
    /// rolled back — after the arbiter's re-election, whose onDeactivate would write them again
    /// (`scope(false)`, `menuOpen(false)`); on a reload in both passes of the purge. A captured key
    /// goes to the module the hook took it for (captures.rs, `deliver`), and `host.keys`' capture
    /// bindings are the generic ones key_scope_tests.rs runs against real VMs.
    #[test]
    fn a_modules_key_scope_and_flag_go_with_it_after_its_deactivation() {
        let enable = body(LIB, "fn after_toggle(");
        let resolve = enable.find("self.arbiter_resolve(&slot);").expect("the re-election");
        assert!(enable.find("self.forget_key_owner(idx);").is_some_and(|at| at > resolve), "before the re-election");
        let purge = body(LIB, "fn purge_module(");
        let deactivated = purge.find("for (lua, f) in deactivations {").expect("the onDeactivate calls");
        assert!(purge[deactivated..].contains("self.forget_key_owner(idx);"), "not again after the onDeactivate");
        assert!(purge[..deactivated].contains("self.forget_key_owner(idx);"), "not before it either");
        let rollback = body(LIB, "fn rollback_to(");
        let resolve = rollback.find("self.arbiter_resolve(&slot);").expect("the re-election");
        let forget = rollback.find("self.captures.forget_owners_from(n);").expect("the scopes and flags");
        assert!(forget > resolve && rollback[forget..].contains("self.refresh_captured();"));
        let on_key = body(LIB, "fn on_key(&mut self");
        assert!(on_key.contains("captures::arrive(self.shared, vk, mods, owner, repeat, pressed);"), "{on_key}");
        assert!(body(LIB, "fn install_host_api(").contains("install_key_captures(lua, &keys, idx, shared.clone())?;"));
    }

    /// Every event site hands its event to the module's mailbox, which runs it as a handler: none
    /// calls a module's callback itself any more. The synchronous places — the arbiter's
    /// `onActivate` and `onDeactivate`, a module's own `onChange` from its own `set` — are the only
    /// plain calls left.
    #[test]
    fn every_event_site_goes_through_the_mailbox() {
        for (sig, call) in [
            ("fn on_hotkey(&mut self", "mailbox::deliver(self.shared, owner, &lua, Event::Hotkey"),
            ("fn on_key(&mut self", "captures::arrive(self.shared"),
            ("fn on_window_activate(&mut self", "mailbox::deliver(self.shared, idx, &m.lua, ev)"),
            ("fn on_focus_change(&mut self", "mailbox::deliver(self.shared, idx, &m.lua, Event::Focus)"),
            ("fn dispatch_initial(&self)", "mailbox::deliver(shared, idx, lua, Event::Initial"),
            ("fn fire_due_timers(&self)", "mailbox::deliver(self, idx, &lua, Event::After { token })"),
            ("fn fire_on_change(", "mailbox::deliver_later(self, owner, &lua, ev)"),
        ] {
            let f = body(LIB, sig);
            assert!(f.contains(call), "`{sig}` does not hand its event to the mailbox");
            assert!(!f.contains("guard(||"), "`{sig}` still calls a callback itself");
        }
        assert!(body(GAMEPAD, "fn deliver_pad_events(").contains("mailbox::deliver(self, idx, &lua, Event::Pad"));
        assert!(body(IMAGES, "pub(crate) fn fire_image_results(").matches("mailbox::deliver(self, owner, &lua, Event::Image").count() == 2);
        assert!(body(SNAP, "pub(crate) fn fire_snapshot_results(").contains("mailbox::deliver(self, idx, &lua, Event::Snapshot"));
        assert!(body(OCR_LUA, "pub(crate) fn fire<H: MailHost>(h: &H)").contains("mailbox::deliver(h, idx, &lua, Event::Read"));
        assert!(body(TIMERS, "pub(crate) fn fire_due(").contains("deliver(Due::Once { token, idx, lua });"));
        // The one place a module's own onChange runs inside its own set: the caller's VM.
        let set = bindings(body(LIB, "fn install_host_api("), "settings_api").into_iter().find(|b| b.0 == "set").expect("set").1;
        assert!(set.contains("sh.fire_on_change(idx, &key, &v, old.as_ref(), Some(caller));"));
        assert!(body(LIB, "fn set_setting(").contains("self.fire_on_change(idx, key, &value, old.as_ref(), None);"));
        // Both ticks run the queued phase, after the snapshots' answers and before the report of the
        // window in front.
        // Spelt in halves, so that this test's own text is not what is found or counted.
        let headless = body(LIB, "fn on_tick(&mut self)");
        let (snaps, queued, initial) = (
            headless.find(concat!("self.shared.fire_snapshot", "_results();")).unwrap(),
            headless.find(concat!("self.shared.run_queued", "_events();")).expect("the headless tick runs no queued phase"),
            headless.find(concat!("self.dispatch", "_initial();")).unwrap(),
        );
        assert!(snaps < queued && queued < initial);
        assert_eq!(LIB.matches(concat!("shared.run_queued", "_events()")).count(), 2, "the GUI tick and the headless one");
        // A mailbox that is not empty keeps the headless loop running.
        assert!(body(LIB, "pub fn run(&mut self)").contains("!self.shared.mail.is_empty()"));
    }

    /// The stretches of `src` only a test build compiles: each item under `#[cfg(test)]`, from the
    /// attribute to its closing brace at the attribute's own indentation — or to the end of its
    /// line, for an item written on one (a `use`, a field, an enum variant, a match arm).
    fn test_only(src: &str) -> Vec<std::ops::Range<usize>> {
        let mut out = Vec::new();
        for (at, attr) in src.match_indices("#[cfg(test)]") {
            let line = src[..at].rfind('\n').map_or(0, |i| i + 1);
            let indent = &src[line..at];
            if !indent.chars().all(|c| c == ' ') {
                continue;
            }
            // The item's first line: past the attribute's own line, more attributes and comments.
            let mut start = at + attr.len();
            let mut item = "";
            for l in src[start..].split_inclusive('\n') {
                let t = l.trim();
                if !(t.is_empty() || t.starts_with("#[") || t.starts_with("//")) {
                    item = t;
                    break;
                }
                start += l.len();
            }
            let one_line = item.ends_with(';') || item.ends_with(',') || item.ends_with('}');
            let end = if one_line {
                start + src[start..].find('\n').unwrap_or(src.len() - start)
            } else {
                let close = format!("\n{indent}}}");
                start + src[start..].find(&close).map_or(src.len() - start, |i| i + close.len())
            };
            out.push(at..end);
        }
        out
    }

    /// The name of the function whose text runs up to `at`: the last line before it that starts a
    /// function.
    fn enclosing_fn(src: &str, at: usize) -> &str {
        for l in src[..at].lines().rev() {
            let mut t = l.trim_start();
            for prefix in ["pub(crate) ", "pub(super) ", "pub ", "unsafe ", "async ", "const "] {
                t = t.strip_prefix(prefix).unwrap_or(t);
            }
            if let Some(rest) = t.strip_prefix("fn ") {
                return rest.split(['(', '<']).next().unwrap_or(rest);
            }
        }
        "?"
    }

    /// A module's callback is called outside its mailbox only at the synchronous places — the
    /// arbiter's `onActivate` and `onDeactivate` (the purge's, the unregistering's and the
    /// re-election's) and a module's own `onChange` from its own `set` — wherever the call is
    /// written: every source file of the crate is searched, its test code left out. So a new
    /// event site that calls a callback itself — the HTML probe's page callbacks, merged without
    /// the mailbox — fails here, rather than run a callback inside another module's handler.
    #[test]
    fn a_modules_callback_is_called_outside_its_mailbox_only_at_the_synchronous_places() {
        let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut found: Vec<String> = Vec::new();
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                let tests = test_only(&src);
                // Spelt in halves, so that this test's own text is not what is found.
                let call = concat!("call_", "guarded(");
                for (at, _) in src.match_indices(call) {
                    if !tests.iter().any(|r| r.contains(&at)) {
                        found.push(format!("{name}: {}", enclosing_fn(&src, at + call.len() - 1)));
                    }
                }
            }
        }
        found.sort();
        assert_eq!(
            found,
            [
                "lib.rs: arbiter_resolve",
                "lib.rs: arbiter_resolve",
                "lib.rs: arbiter_unregister",
                "lib.rs: fire_on_change",
                "lib.rs: purge_module",
            ],
            "a module's callback called outside its mailbox"
        );
    }

    /// Every source file of the crate but `vm_guard.rs`, its test code left out: (name, text,
    /// the stretches only a test build compiles).
    fn production_sources() -> Vec<(String, String, Vec<std::ops::Range<usize>>)> {
        let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let mut out = Vec::new();
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if !name.ends_with(".rs") || name.ends_with("_tests.rs") || name == "vm_guard.rs" {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                let tests = test_only(&src);
                out.push((name, src, tests));
            }
        }
        out
    }

    /// Every VM the host builds is the guard's (`vm_guard::new_vm`): it has its memory limit from
    /// the start, and a slot that can stop it. A `Lua::new()` anywhere else outside test code
    /// would be a VM with neither.
    #[test]
    fn every_vm_is_made_by_the_guard() {
        // Spelt in halves, so that this test's own text is not what is found.
        let new = concat!("Lua::", "new()");
        let mut found = Vec::new();
        for (name, src, tests) in production_sources() {
            for (at, _) in src.match_indices(new) {
                if !tests.iter().any(|r| r.contains(&at)) {
                    found.push(format!("{name}: {}", enclosing_fn(&src, at)));
                }
            }
        }
        assert!(found.is_empty(), "a VM the guard did not make: {found:?}");
        let new_vm = concat!("vm_guard::new", "_vm(&shared.guard)");
        assert!(body(LIB, "fn load_module(").contains(new_vm));
        assert_eq!(body(LIB, "fn reload_module(").matches(new_vm).count(), 2, "the rebuilt VM and the empty one after a failure");
    }

    /// The calls into a VM's Lua that the guard watches: every handler stretch (task.rs), every
    /// plain call (`call_guarded`), every step of a load (`populate_vm`'s `load_step`s), and a
    /// handler whose coroutine could not be made. And the stop's own report replaces the
    /// module's error, wherever a call's error would be reported.
    #[test]
    fn the_calls_into_a_vm_are_entries_of_the_guard() {
        const TASK: &str = include_str!("task.rs");
        let stretch = body(TASK, "fn stretch(");
        assert!(stretch.contains("crate::vm_guard::Entry::enter(&lua, crate::vm_guard::EntryKind::Handler, what)"));
        assert!(stretch.contains("e.finish(&r);"));
        // One entry for the whole delivery, its own yields' answers inside it: the slice the
        // clocks count does not start again at each.
        assert!(body(TASK, "fn enter_delivery(").contains("crate::vm_guard::Entry::enter(&lua, crate::vm_guard::EntryKind::Handler, what)"));
        for f in ["fn start_thread<", "fn resume_parked<"] {
            let b = body(TASK, f);
            let (enter, step, end) = (
                b.find("let delivery = enter_delivery(").unwrap_or_else(|| panic!("{f}: no delivery entry")),
                b.find(", &delivery, outcome);").unwrap_or_else(|| panic!("{f}: the step is not inside the delivery")),
                b.find("drop(delivery);").unwrap_or_else(|| panic!("{f}: the delivery does not end after the step")),
            );
            assert!(enter < step && step < end, "{f}");
        }
        assert!(body(TASK, "fn step<").contains("outcome = stretch(tasks, id, delivery, ||"));
        let step = body(TASK, "fn step<");
        assert!(step.contains("!crate::vm_guard::stopped(lua)"), "a stopped handler's error is reported as the module's");
        assert!(body(TASK, "pub(crate) fn run_handler<").contains("crate::vm_guard::note_failure(lua"));
        let call = body(LIB, "fn call_guarded<");
        assert!(call.contains("vm_guard::Entry::enter(lua, vm_guard::EntryKind::Plain, what)") && call.contains("entry.finish(&r);"));
        let populate = body(LIB, "fn populate_vm(");
        for step in [concat!("load_step(lua, format!(\"the code of ", "{dep_id}\")"), "load_step(lua, \"its entry file\"", "load_step(lua, \"its activate function\""] {
            assert!(populate.contains(step), "`{step}` is not an entry of the guard");
        }
        // The limit is set before any of the module's code runs, and the load is marked done last.
        let describe = populate.find("vm_guard::describe(lua,").expect("the limit");
        assert!(describe < populate.find("install_host_api(").unwrap());
        assert!(populate.trim_end().ends_with("vm_guard::loaded(lua);\n    Ok(())"), "the load is not marked done last");
        assert!(body(LIB, "fn load_step<").contains("entry.finish(&result);"));
    }

    /// A stop is marked when the callback that began it ends (the exit hook, set once), and settled
    /// by both ticks after the report of the window in front; the settings file keeps a stopped
    /// module on; turning it on clears its stop and its VM; a rollback forgets the stops of what it
    /// rolls back.
    #[test]
    fn the_stop_is_wired_where_disabling_is() {
        assert!(body(LIB, "pub fn new() -> Result<Self>").contains("stops::mark(&*sh, trips);"));
        let headless = body(LIB, "fn on_tick(&mut self)");
        let (initial, settle) = (
            headless.find(concat!("self.dispatch", "_initial();")).unwrap(),
            headless.find(concat!("self.shared.settle", "_stops();")).expect("the headless tick settles no stop"),
        );
        assert!(initial < settle);
        // A recheck round runs callbacks after that, in both ticks: what it stops is settled
        // right after it, in the same turn, not one tick later.
        let after_recheck = |tick: &str, round: &str, settle: &str| {
            let at = tick.find(round).unwrap_or_else(|| panic!("no recheck round: {round}"));
            assert!(tick[at..].trim_start_matches(round).trim_start().starts_with(settle), "a stop in the recheck round waits a tick: {round}");
        };
        after_recheck(headless, concat!("self.on_focus", "_change();"), concat!("self.shared.settle", "_stops();"));
        after_recheck(body(LIB, "pub fn run(&mut self)"), concat!("dispatcher.on_focus", "_change();"), concat!("shared.settle", "_stops();"));
        assert_eq!(LIB.matches(concat!("shared.settle", "_stops();")).count(), 4, "each tick, and each tick's recheck round");
        assert!(LIB.contains(concat!("stopped modules \\", "\n")) && LIB.contains(concat!("{stops_", "ms}) — {hazard}")));
        assert!(body(LIB, "fn sync_enabled_into_store(").contains("stops::stored_enabled("));
        let enable = body(LIB, "fn apply_enabled(");
        assert!(enable.contains("self.stops.clear(idx)") && enable.contains("vm_guard::clear_module(&self.guard, idx);"));
        // The manager's checkbox builds a stopped VM afresh before it turns the module on — and
        // every module a library's stop took, when the library is ticked.
        let toggle = body(LIB, "fn toggle_module(");
        let (group, rebuild, on) = (
            toggle.find("shared.stops.turned_on_with(idx)").expect("the group"),
            toggle.find("reload_module(shared, modules, i)").expect("no rebuild"),
            toggle.find("shared.set_enabled(i, true);").expect("not turned on"),
        );
        assert!(group < rebuild && rebuild < on, "rebuilt before it is turned on");
        assert!(toggle.contains("vm_guard::stopped(&m.lua)"), "only a stopped VM is rebuilt");
        assert!(LIB.contains("Some(i) => toggle_module(&toggle_shared, &toggle_modules, i, enabled),"), "the checkbox does not go through it");
        // A reload keeps a stopped module off: neither its record nor its flag is touched by it.
        for f in ["fn reload_module(", "fn purge_module(", "fn reload_everything("] {
            let b = body(LIB, f);
            assert!(!b.contains("stops.clear(") && !b.contains("apply_enabled(") && !b.contains("set_enabled("), "{f} turns a stopped module on");
        }
        assert!(body(LIB, "fn rollback_to(").contains("self.stops.drop_from(n);"));
        assert!(body(MAILBOX, "pub(crate) fn busy<").contains("h.module_stopped(idx)"));
        // A stopped VM counts, record or not; and the queued phase looks again before each event.
        let run = body(MAILBOX, "pub(crate) fn run_queued<");
        assert!(run.contains("!stopped(h, **idx, &b.lua)") && run.contains("if stopped(h, idx, &b.lua) {"));
        assert!(body(MAILBOX, "fn stopped<").contains("h.module_stopped(idx) || crate::vm_guard::stopped(lua)"));
        assert!(body(MAILBOX, "pub(crate) fn deliver<").contains("busy(h, owner.idx) || crate::vm_guard::stopped(lua)"));
        // Trips a panic left in the guard are marked at the end of the turn.
        assert!(body(LIB, "fn settle_stops(").contains("vm_guard::take_waiting_trips(&self.guard)"));
    }

    /// The `[limits]` lines module-package-format.md shows are the ones written.
    #[test]
    fn the_limits_lines_are_the_documented_ones() {
        let doc = include_str!("../../../docs/module-package-format.md").replace("\n  ", " ");
        let unknown = format!("[manager] {}", crate::limits_unknown_line("com.example.game", "memroy_mib"));
        assert!(doc.contains(&unknown), "module-package-format.md does not show: {unknown}");
        let info = crate::vm_guard::VmInfo {
            module: Some(0),
            id: "com.platform.melodyne".into(),
            name: "Melodyne".into(),
            code: vec![
                crate::vm_guard::CodeShare { id: "com.platform.melodyne".into(), name: "Melodyne".into(), mib: 256, declared: false },
                crate::vm_guard::CodeShare { id: "com.platform.overlay".into(), name: "Overlay runtime".into(), mib: 256, declared: false },
            ],
        };
        let limit = format!("[guard] {}", crate::vm_guard::limit_line(&info));
        assert!(doc.contains(&limit), "module-package-format.md does not show: {limit}");
    }

    /// The guard probe's test list cites the lines its loops are on — the line a stop is heard at —
    /// and every loop of the probe is one of them; every loop and every fill ends by itself, so a
    /// build without the guard never hangs on a probe key; and the run line builds first.
    #[test]
    fn the_guard_probes_lines_are_its_loops() {
        let main = include_str!("../../../tools/guard-probe/src/main.luau");
        let others = [include_str!("../../../tools/guard-probe-b/src/main.luau"), include_str!("../../../tools/guard-probe-lib/src/main.luau")];
        let loops = |src: &str| -> Vec<usize> {
            src.lines()
                .enumerate()
                .filter(|(_, l)| !l.trim_start().starts_with("--") && l.contains("while os.clock() - started < 30 do"))
                .map(|(i, _)| i + 1)
                .collect()
        };
        let header: String = main.lines().take_while(|l| l.starts_with("--")).collect::<Vec<_>>().join("\n").replace("\n--", "");
        let numbers_after = |marker: &str| -> Vec<usize> {
            header
                .match_indices(marker)
                .filter_map(|(at, _)| {
                    let digits: String = header[at + marker.len()..].trim_start().chars().take_while(char::is_ascii_digit).collect();
                    digits.parse().ok()
                })
                .collect()
        };
        let cited: std::collections::BTreeSet<usize> = numbers_after("line ").into_iter().chain(numbers_after("main.luau:")).collect();
        let mut all: std::collections::BTreeSet<usize> = loops(main).into_iter().collect();
        for src in others {
            all.extend(loops(src));
        }
        assert!(!cited.is_empty());
        for n in &cited {
            assert!(all.contains(n), "the guard probe expects a stop at line {n}, where none of its loops is");
        }
        for n in &all {
            assert!(cited.contains(n), "the guard probe's loop at line {n} is not in its test list");
        }
        for src in std::iter::once(main).chain(others) {
            let code: Vec<&str> = src.lines().filter(|l| !l.trim_start().starts_with("--")).collect();
            assert!(!code.iter().any(|l| l.contains("while true do")), "a probe loop that never ends by itself");
        }
        assert!(header.contains(r".\run-dev.ps1 -Build -Tools -Only guard-probe,guard-probe-lib,guard-probe-b,guard-probe-c"));
        assert!(header.contains(r".\run-dev.ps1 -Build -Release"));
    }

    /// The time limit is wired: the guard is this thread's and its watchdog runs from before the
    /// first load until exit, which writes the `[cpu]` summary and lets go of every mouse button a
    /// module still holds; the bindings that press and release buttons keep the record of them; a
    /// module stopping lets go of its buttons wherever it stops running; and the chunks of a
    /// module's code are named for the stop's frames.
    #[test]
    fn the_time_limit_is_wired() {
        let new = body(LIB, "pub fn new() -> Result<Self>");
        let install = new.find("vm_guard::install(&shared.guard);").expect("the guard is not this thread's");
        let spawn = new.find("vm_guard::spawn_watchdog(shared.guard.clone())").expect("no watchdog");
        assert!(install < spawn);
        assert!(body(LIB, "pub fn run(dirs: &[String]) -> Result<()> {").contains("manager.stop_guard();"));
        let stop = body(LIB, "fn stop_guard(&mut self)");
        assert!(stop.contains("w.stop();") && stop.contains("self.shared.guard.cpu_summary()") && stop.contains("self.shared.release_all_held_buttons();"));
        let api = body(LIB, "fn install_host_api(");
        let input = bindings(api, "input");
        let held = |name: &str| input.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("host.input.{name}")).1;
        assert!(held("mouseDown").contains("sh.hold_button(idx, button);"));
        for name in ["mouseUp", "click", "drag"] {
            assert!(held(name).contains("sh.let_go_button(idx, button);"), "host.input.{name} does not let go of its button");
        }
        assert!(body(LIB, "fn after_toggle(").contains("self.release_held_buttons(idx, \"it was disabled\");"));
        assert!(body(LIB, "fn purge_module(").contains("self.release_held_buttons(idx, \"it was reloaded\");"));
        assert!(body(LIB, "fn rollback_to(").contains("self.release_held_buttons(idx, \"its load was rolled back\");"));
        assert!(body(LIB, "fn release_buttons(&self, idx: usize)").contains("self.release_held_buttons(idx, \"it was stopped by the host\")"));
        let populate = body(LIB, "fn populate_vm(");
        assert!(populate.contains("vm_guard::name_chunk(lua, &dep_name, dep_id, dep_rel);"));
        assert!(populate.contains("vm_guard::name_chunk(lua, &entry.display().to_string(), id, &module.manifest.entry);"));
        assert!(body(LIB, "fn install_include(").contains("vm_guard::name_chunk(lua, &key, &owner,"));
    }

    /// Every closure Luau can call in production code marks itself with `host_call!` — so the stop's
    /// messages can name the host call the time ran out in (and G3 a call that never returns) — and
    /// each of the host table's bindings by the name a module writes. The exceptions are the host's
    /// own plumbing, which no module calls by name: the gated views' `__index`, the prelude's
    /// `_requestInitial`, `WAITED_KEY`'s clock and the task shim's internals.
    #[test]
    fn every_binding_names_its_host_call() {
        let api = body(LIB, "fn install_host_api(");
        let namespaces = [
            ("log", "log"),
            ("json", "json"),
            ("speech", "speech"),
            ("hk", "hotkey"),
            ("keys", "keys"),
            ("os", "os"),
            ("win", "window"),
            ("uia", "element"),
            ("screen", "screen"),
            ("ocr", "ocr"),
            ("input", "input"),
            ("sound", "sound"),
            ("resource", "resource"),
            ("settings_api", "settings"),
            ("arbiter", "arbiter"),
        ];
        let mut named = 0;
        for (var, ns) in namespaces {
            for (name, text) in bindings(api, var) {
                if !text.contains("create_function(") {
                    continue; // a value, or a function made in another file, checked below
                }
                assert!(text.contains(&format!("host_call!(\"host.{ns}.{name}\")")), "host.{ns}.{name} does not name itself");
                named += 1;
            }
        }
        assert!(named > 60, "only {named} bindings found: the scan no longer reads install_host_api");
        for name in ["require", "tryRequire", "now", "inputEpoch", "epoch", "path"] {
            assert!(api.contains(&format!("host_call!(\"host.{name}\")")), "host.{name} does not name itself");
        }
        let keys = body(LIB, "fn install_key_captures<");
        for name in ["capture", "release", "releaseAll", "scope", "menuOpen", "passedThrough"] {
            assert!(keys.contains(&format!("host_call!(\"host.keys.{name}\")")), "host.keys.{name}");
        }
        assert!(body(LIB, "fn install_include(").contains("host_call!(\"host.include\")"));
        for name in ["after", "every", "cancel"] {
            assert!(TIMERS.contains(&format!("host_call!(\"host.timer.{name}\")")), "host.timer.{name}");
        }
        for name in ["list", "state", "on", "off", "status"] {
            assert!(GAMEPAD.contains(&format!("host_call!(\"host.gamepad.{name}\")")), "host.gamepad.{name}");
        }
        for name in ["template", "imageSearch", "imageSearchMulti", "imageSearchAsync", "imageSearchEach", "imageSearchAll"] {
            assert!(IMAGES.contains(&format!("host_call!(\"host.screen.{name}\")")), "host.screen.{name}");
        }
        for name in ["host.screen.snapshot", "host.screen.pixels", "host.screen.snapshotAsync", "snapshot:crop", "snapshot:release"] {
            assert!(SNAP.contains(&format!("host_call!(\"{name}\")")), "{name}");
        }
        // And every closure in the crate, wherever it is written.
        let exempt = [
            ("lib.rs", "build_dep_host"),
            ("lib.rs", "free_subsets"),
            ("lib.rs", "gated_view"),
            ("lib.rs", "initial_request_fn"),
            ("lib.rs", "populate_vm"),
            ("task.rs", "prims"),
            ("task.rs", "waits"),
        ];
        let mut missing = Vec::new();
        for (name, src, tests) in production_sources() {
            for pat in ["create_function(", "add_method("] {
                for (at, _) in src.match_indices(pat) {
                    if tests.iter().any(|r| r.contains(&at)) {
                        continue;
                    }
                    let f = enclosing_fn(&src, at);
                    if exempt.contains(&(name.as_str(), f)) {
                        continue;
                    }
                    let rest = &src[at + pat.len()..];
                    let window = &rest[..rest.len().min(500)];
                    let next = window.find(pat).unwrap_or(window.len());
                    let first = &window[..next];
                    if !(first.contains("host_call!(") || first.contains("HostCall::enter(")) {
                        missing.push(format!("{name}: {f}"));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "a closure Luau can call without host_call!: {missing:?}");
    }

    /// Every call from the host into a VM's Lua in production code is one of the places the guard
    /// knows: an entry (a handler's stretch, `call_guarded`, a load step), code run inside one (an
    /// include, a dependency's evaluation), or the host's own Luau, which runs before any module
    /// code (the prelude, the task shim) or which a stopped VM answers with nothing (the prelude's
    /// queries). A new place fails here, rather than run a module's code past its limits.
    #[test]
    fn every_host_to_lua_call_goes_through_an_entry() {
        let calls = [".call::<", ".call(", ".resume::<", ".resume(", ".resume_error", ".eval::<", ".eval()", ".exec()", ".into_function()"];
        let mut found = std::collections::BTreeSet::new();
        for (name, src, tests) in production_sources() {
            // ureq's `call()`, not Luau's.
            if name == "registry.rs" {
                continue;
            }
            for c in calls {
                for (at, _) in src.match_indices(c) {
                    let line = src[..at].rfind('\n').map_or(0, |i| i + 1);
                    if tests.iter().any(|r| r.contains(&at)) || src[line..at].trim_start().starts_with("//") {
                        continue;
                    }
                    found.insert(format!("{name}: {}", enclosing_fn(&src, at)));
                }
            }
        }
        let expected = [
            // Entries.
            "lib.rs: call_guarded",
            "lib.rs: populate_vm",
            "lib.rs: eval_on_host",
            "task.rs: start_thread",
            "task.rs: resume_parked",
            "task.rs: step",
            // Inside the includer's entry, in the same VM.
            "lib.rs: compile_include",
            "lib.rs: install_include",
            // The host's own Luau.
            "lib.rs: install_window_prelude",
            "lib.rs: has_triggers",
            "lib.rs: trigger_seq",
            "lib.rs: wants_initial",
            "task.rs: status",
            "task.rs: waits",
        ];
        let expected: std::collections::BTreeSet<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(found, expected, "a call into a VM the guard does not know");
        const TASK: &str = include_str!("task.rs");
        for f in ["fn start_thread<", "fn resume_parked<", "fn step<"] {
            assert!(body(TASK, f).contains("stretch("), "`{f}` resumes a handler outside a stretch");
        }
        // `activate` is looked up inside the step too: the lookup runs the table's `__index`.
        let populate = body(LIB, "fn populate_vm(");
        let step = populate.find("load_step(lua, \"its activate function\", || match t.get::<mlua::Value>(\"activate\") {").expect("the lookup is outside the step");
        assert!(populate[step..].starts_with(concat!("load_step(lua, \"its activate function\", || match t.get::<mlua::Value>(\"activate\") {\n", "            Ok(mlua::Value::Function(activate)) => activate.call::<()>(()),")));
    }

    /// A hotkey that reaches a busy module notes the window the hook or the tap compares keys with,
    /// asked of nobody — never the scope's question, which on a Mac asks the frontmost application
    /// on the tap's thread and notes a pin. And its line says it waited only when it did.
    #[test]
    fn a_hotkey_notes_the_window_in_front_without_asking_the_application() {
        let on = body(LIB, "fn on_hotkey(&mut self");
        assert!(on.contains("mailbox::busy(self.shared, owner).then(|| self.shared.backend.key_front())"), "{on}");
        assert!(!on.contains(concat!("backend.resolve_key", "_scope()")), "{on}");
        assert!(body(LIB, "fn open_hotkey(").contains("open_hotkey_in(self, &self.hotkeys, id, owner, front, seq, binding, spec)"));
        assert!(on.contains("Event::Hotkey { id, owner, front, seq, binding, spec }"), "the press's count goes with it");
        let open = body(LIB, "pub(crate) fn open_hotkey_in<");
        assert!(open.contains("let queued = front.is_some();"));
        assert!(open.contains("captures::moved_line(&h.key_module_id(owner), spec, queued)"));
        assert!(open.contains("captures::hotkey_dropped_line(&h.key_module_id(owner), spec, gone)"));
    }

    /// A module's mailbox goes with it as its handlers do: on a disable — keeping a setting's
    /// `onChange` — after the re-election; on a reload in both of the purge's passes; on a rollback.
    /// And an activation that waits in a busy module's mailbox is its report of the window in
    /// front: its report asked for on that tick is dropped.
    #[test]
    fn a_modules_mailbox_goes_with_it() {
        let enable = body(LIB, "fn after_toggle(");
        let resolve = enable.find("self.arbiter_resolve(&slot);").expect("the re-election");
        let drop = enable.find("mailbox::drop_owner(self, idx, mailbox::Why::Disabled);").expect("the disable's");
        assert!(drop > resolve && drop < enable.find("self.task_drop_owner(idx);").unwrap());
        let purge = body(LIB, "fn purge_module(");
        assert_eq!(purge.matches("mailbox::drop_owner(self, idx, mailbox::Why::Reloaded);").count(), 2);
        assert!(body(LIB, "fn rollback_to(").contains("mailbox::drop_from(self, n);"));
        let activate = body(LIB, "fn on_window_activate(&mut self");
        let queued = activate.find("retain(|(idx, _)| !queued.contains(idx))").expect("L3.9");
        assert!(queued < activate.find("initial_reported_by_activation(").unwrap());
        assert_eq!(activate.matches("bump_input_epoch()").count(), 1, "once per activation, at its arrival");
    }

    /// No module can start a task: the host table has no entry for the machinery, whose only
    /// entry is the tests' (`task::table`, built in test builds alone).
    #[test]
    fn no_module_can_start_a_task() {
        let api = body(LIB, "fn install_host_api(");
        assert!(!api.contains(concat!("host.set(\"ta", "sk\"")), "the host table has a task entry");
        assert!(!api.contains(concat!("task::ta", "ble(")), "a module's host table is given the tests' entry");
        let task_rs = include_str!("task.rs");
        let entry = task_rs.find("pub(crate) fn table<H: ReadHost>(").expect("the tests' entry");
        assert!(task_rs[..entry].trim_end().ends_with("#[cfg(test)]"), "the tests' entry is built outside the tests");
    }

    /// The read service's share of the handlers, checked where it is written — `Shared` is no
    /// test's holder: the delivery sweeps a hang before it drains, and only while a read is out;
    /// an event queued in the interactive lane raises the module's parked handler, before folding;
    /// and a reading that could not be built goes through `handover_failed`, a stop when its VM ran
    /// out of memory.
    #[test]
    fn the_hang_answer_and_the_promotion_are_wired() {
        let fire = body(OCR_LUA, "pub(crate) fn fire<H: MailHost>(h: &H)");
        let sweep = fire.find("h.ocr().hang_sweep()").expect("the hang answer");
        assert!(sweep > fire.find("if !st.pending.borrow().is_empty()").unwrap());
        assert!(sweep < fire.find("h.ocr().drain()").unwrap());
        // A reading that could not be built is a stop when the VM ran out of memory, for a waiting
        // handler and for a callback alike.
        assert!(fire.contains("handover_failed(h, &p.lua, p.owner.idx, &e);"));
        assert!(body(OCR_LUA, "pub(crate) fn open_read<").contains("handover_failed(h, &p.lua, p.scope, &e);"));
        let enqueue = body(MAILBOX, "fn enqueue<H: MailHost>(");
        let promote = enqueue.find("task::promote(h, owner);").expect("the promotion");
        assert!(enqueue[..promote].contains("if current_priority() == Priority::Interactive {"));
        assert!(promote < enqueue.find("fold(&mut b.queue, &mut ev)").unwrap());
    }
}

/// Where the strict options, the byte-order mark and the screen-reader search are wired in. The
/// rules themselves have unit tests (`source_and_option_tests`, `speech::prism::tests`), but
/// those call the helpers directly: putting `t.get::<bool>("interrupt").unwrap_or(true)` back
/// into the binding, or `std::fs::read_to_string` back into `populate_vm`, left every one of
/// them green. The places are methods of `Shared` or closures inside the host table, and the
/// speech ones a thread a test must not start on somebody's working machine — so they are
/// checked where they are written, the way `ocr_wiring_tests` checks its own.
#[cfg(test)]
mod option_and_speech_wiring_tests {
    use super::ocr_wiring_tests::{bindings, body};

    const LIB: &str = include_str!("lib.rs");
    const PRISM: &str = include_str!("speech/prism.rs");
    const SPEECH: &str = include_str!("speech/mod.rs");

    #[test]
    fn strict_options_and_byte_order_marks_are_wired_where_they_are_read() {
        let api = body(LIB, "fn install_host_api(");
        let output = bindings(api, "speech").into_iter().find(|b| b.0 == "output").expect("host.speech.output").1;
        assert!(output.contains("speech_interrupt(opts.as_ref())?"), "host.speech.output reads interrupt loosely again");
        let check = bindings(api, "keys").into_iter().find(|b| b.0 == "check").expect("host.keys.check").1;
        assert!(check.contains("opt_bool(opts.as_ref(), \"layout\""), "host.keys.check reads layout loosely again");
        let pv = body(LIB, "fn populate_vm(");
        assert_eq!(
            pv.matches("read_luau_source(").count(),
            2,
            "a module's entry file and a code dependency's entry must both drop a byte-order mark"
        );
        assert!(!pv.contains("std::fs::read_to_string"), "an entry file is read without dropping a byte-order mark");
    }

    #[test]
    fn the_screen_reader_search_is_wired_where_it_runs() {
        // A searcher stops at its next look once the setting is off, and says so to the loop.
        let wait = body(PRISM, "fn wait_for_screen_reader(");
        assert!(wait.contains("if !crate::appcfg::screen_reader_speech()"), "a searcher no longer stops when the setting is off");
        assert!(wait.contains("stopped.store(true"), "a stopped searcher no longer tells the loop to send another");
        assert!(wait.contains("nobody_listening("), "a replaced searcher goes on looking");
        // The loop decides by the pure rule, reading `reader` before `healthy`.
        let retry = body(PRISM, "pub fn retry_if_due(");
        assert!(retry.contains("searcher_due("), "retry_if_due no longer asks searcher_due");
        let reader = retry.find("self.reader.load(Ordering::Acquire)").expect("retry_if_due reads reader with Acquire");
        let healthy = retry.find("self.healthy.load(").expect("retry_if_due reads healthy");
        assert!(reader < healthy, "retry_if_due reads healthy before reader");
        // A searcher that finds a reader stores `healthy` before releasing `reader`.
        let run = body(PRISM, "fn run(");
        let found = &run[run.find("Start::Retry(why) =>").expect("the Retry branch of run")..];
        let healthy = found.find("healthy.store(true, Ordering::Relaxed)").expect("the Retry branch stores healthy");
        let reader = found.find("reader.store(true, Ordering::Release)").expect("the Retry branch releases reader");
        assert!(healthy < reader, "the Retry branch stores reader before healthy");
        // Ticking the setting is seen on every pass of the loop, not only when a line is said.
        let pump = body(SPEECH, "pub fn pump(&self)");
        assert!(pump.contains("self.prism.borrow_mut().rearm();"), "pump no longer re-arms on a tick of the setting");
    }
}

/// Where the uptime work meets the event loop: the system events and the log's end-of-tick work.
/// They are methods of `Shared`, which a test cannot build (its speech engines must not be
/// opened), so they are checked where they are written, as `ocr_wiring_tests` checks its rules.
#[cfg(test)]
mod uptime_wiring_tests {
    use super::ocr_wiring_tests::body;

    const LIB: &str = include_str!("lib.rs");
    const WINDOWS: &str = include_str!("backend/windows.rs");

    #[test]
    fn system_events_and_the_logs_tick_are_wired_into_both_loops() {
        // Both loops write the captured-set line and the error summaries at the end of a tick.
        assert!(body(LIB, "fn on_tick(&mut self)").contains("self.shared.log_housekeeping();"));
        let run = body(LIB, "pub fn run(&mut self)");
        assert!(run.contains("shared.log_housekeeping();"), "the GUI tick");
        // The system watch starts before either loop runs.
        assert!(run.contains("self.shared.backend.watch_system();"));
        // A batch: the epochs, then the front reported again and a focus round.
        let sys = body(LIB, "fn on_system(&mut self");
        assert!(sys.contains("self.shared.bump_input_epoch();"));
        assert!(sys.contains("self.on_window_activate(win)"));
        assert!(sys.contains("self.on_focus_change();"));
        assert!(sys.contains("self.shared.audio.borrow_mut().take();"));
        // Windows: delivered before the window events, with desktop duplication reset first.
        let pump = body(WINDOWS, "fn pump_pending(&self");
        let take = pump.find("system_events::take()").expect("system events drained");
        let reset = pump.find("dxgi::system_changed(").expect("duplication reset");
        let hotkeys = pump.find("HOTKEY_QUEUE").expect("the hotkeys");
        assert!(take < reset && reset < hotkeys);
    }

    const LOGGING: &str = include_str!("logging.rs");
    const IMAGES: &str = include_str!("image_search.rs");
    const OCR_LUA: &str = include_str!("ocr/lua.rs");
    const HOOK_WATCH: &str = include_str!("backend/hook_watch_thread.rs");
    const DXGI: &str = include_str!("backend/dxgi.rs");
    const PRISM: &str = include_str!("speech/prism.rs");

    /// The call sites of the uptime fixes, each of which the unit tests of its helper would not
    /// notice being reverted: the helpers are tested where they are written, and these say that
    /// the code that should call them still does.
    #[test]
    fn the_uptime_fixes_are_called_where_they_apply() {
        // A module error: written once, then counted, the summary written when due.
        let err = body(LIB, "fn report_callback_error(");
        assert!(err.contains("self.error_repeats.borrow_mut().note(&key, Instant::now())"), "{err}");
        assert!(err.contains("logging::Said::Counted => {}"));
        assert!(err.contains("logging::Said::Summary { count, over }"));
        // Forgotten on toggle, on a reload, and for a rolled-back hot-load — with the lines of the
        // blocking calls that held the event loop.
        assert!(body(LIB, "fn forget_error_repeats(").contains("self.tasks.forget_legacy_said(id);"));
        assert!(body(LIB, "fn after_toggle(").contains("self.forget_error_repeats(idx);"));
        assert!(body(LIB, "fn purge_module(").contains("self.forget_error_repeats(idx);"));
        let rollback = body(LIB, "fn rollback_to(");
        let forget = rollback.find("self.forget_error_repeats(idx);").expect("rollback forgets");
        let cut = rollback.find("self.ids.borrow_mut().truncate(n);").expect("ids cut");
        assert!(forget < cut, "forgotten while the ids are still there");
        // The tick writes the summaries of errors and of system events that stopped.
        let tick = body(LIB, "fn log_housekeeping(&self)");
        assert!(tick.contains("self.error_repeats.borrow_mut().due(now)"));
        assert!(tick.contains("self.system_since.borrow_mut().due(now)"));
        // A hotkey another program holds: said on the first refusal, counted after.
        let hk = body(LIB, "fn refresh_hotkeys(&self)");
        assert!(hk.contains("let n = refused.entry((idx, spec.clone())).or_insert(0);"));
        assert!(hk.contains("if first {\n                        self.report_os_conflict(idx, \"hotkey\", &spec, &e);"));
        assert!(hk.contains("self.os_refused.borrow_mut().remove(&(idx, spec.clone()))"));
        // The display lines only when they changed.
        let sys = body(LIB, "fn on_system(&mut self");
        assert!(sys.contains("if *said != now {\n                logging::report(\"env\", &now);"), "{sys}");
        // A sound: the output checked before it is used.
        let api = body(LIB, "fn install_host_api(");
        assert!(api.contains("if audio.as_ref().is_some_and(sound::Output::stale) {"));
        // The log rotates from the line that crosses the limit.
        let line = body(LOGGING, "pub fn line(scope: &str, msg: &str)");
        assert!(line.contains("guard.as_mut().is_some_and(|sink| sink.write(text.as_bytes()))"));
        assert!(line.contains("sink.rotate(&continued);"));
        // Image searches: the cap at submit, and the worker skipping what was ended.
        let submit = body(IMAGES, "fn submit(");
        assert!(submit.contains("oldest_over_cap(&sh.pending_image.borrow(), owner, IMAGE_PER_OWNER)"));
        assert!(submit.contains("sh.ended_images.borrow_mut().push((p, why));"));
        assert!(submit.contains("logging::Said::Summary { count, over } =>"), "the summary is written");
        assert!(body(IMAGES, "fn worker_loop(").contains("batch.retain(|t| !set.remove(&t.id));"));
        assert!(body(IMAGES, "pub(crate) fn purge_pending_images(").contains("self.ended_images.borrow_mut()"));
        // OCR keys: forgotten when their last read is answered or dropped.
        assert!(body(OCR_LUA, "pub(crate) fn fire<H: MailHost>(h: &H)").contains("st.settled(p.owner, p.key.as_deref());"));
        assert!(body(OCR_LUA, "pub(crate) fn open_read<H: ReadHost>(").contains("st.settled(p.owner, p.key.as_deref());"));
        assert!(body(OCR_LUA, "pub(crate) fn discard_read<H: ReadHost>(").contains("st.settled(r.p.owner, r.p.key.as_deref());"));
        assert!(body(OCR_LUA, "pub(crate) fn drop_owner<H: ReadHost>(").contains("st.settled(p.owner, p.key.as_deref());"));
        assert!(body(OCR_LUA, "pub(crate) fn withdraw<H: ReadHost>(").contains("st.settled(p.owner, p.key.as_deref());"));
        // The keyboard watch's windows push the system events.
        let watch = body(HOOK_WATCH, "unsafe extern \"system\" fn watch_wndproc(");
        assert!(watch.contains("system(system_events::from_wts(wparam as u32));"));
        assert!(watch.contains("system(system_events::from_pbt(wparam as u32));"));
        let broadcast = body(HOOK_WATCH, "unsafe extern \"system\" fn broadcast_wndproc(");
        for ev in [
            "WM_DISPLAYCHANGE => system(Some(SystemEvent::DisplaysChanged))",
            "WM_DPICHANGED => system(Some(SystemEvent::Scale))",
            "WM_SETTINGCHANGE => system(system_events::from_setting(wparam as u32))",
            "system(Some(SystemEvent::TaskbarCreated))",
        ] {
            assert!(broadcast.contains(ev), "{ev}");
        }
        let push = body(HOOK_WATCH, "fn system(event: Option<SystemEvent>)");
        assert!(push.contains("system_events::push(event);") && push.contains("wake_pump();"));
        // Desktop duplication: a stopped engine is tried again when its time has come.
        let engine = body(DXGI, "fn engine() ->");
        assert!(engine.contains("stopped_engine(now_ms(), RETRY_AT_MS.load(Ordering::SeqCst), stuck)"));
        assert!(engine.contains("retire(old, \"had stopped, and its time is up\");"));
        // Screen-reader workers: none started past the parked ones.
        assert!(body(PRISM, "pub fn retry_if_due(&mut self)").contains("if self.worker_allowed() {"));
        assert!(body(PRISM, "pub fn rearm(&mut self)").contains("if !self.worker_allowed() {"));
        let stalled = body(PRISM, "fn stalled(&self)");
        assert!(stalled.contains("PARKED.fetch_add(1, Ordering::AcqRel) + 1"));
    }
}

/// Where the macOS rules for days of uptime are wired in. Their rules have unit tests
/// (`speech::vo_park`, `backend/macos/backoff.rs`, `backend/macos/handle_table.rs`,
/// `system_events`, whose `what_each_event_sets_off` is the table), but the places that call them are macOS code
/// that only a Mac runs — reverting `capture.rs` to a switch, dropping the VoiceOver tick or the
/// system drain, or putting the sweep back inside an intern left every other test and the macOS
/// type check green. So they are checked where they are written, as `ocr_wiring_tests` does.
#[cfg(test)]
mod macos_uptime_wiring_tests {
    use super::ocr_wiring_tests::body;

    const LIB: &str = include_str!("lib.rs");
    const SPEECH: &str = include_str!("speech/mod.rs");
    const VO: &str = include_str!("speech/voiceover.rs");
    const QUEUE: &str = include_str!("backend/macos/queue.rs");
    const SYSTEM: &str = include_str!("backend/macos/system.rs");
    const WATCH: &str = include_str!("backend/macos/watch.rs");
    const MAC: &str = include_str!("backend/macos/mod.rs");
    const CAPTURE: &str = include_str!("backend/macos/capture.rs");
    const HANDLES: &str = include_str!("backend/macos/handles.rs");
    const AX: &str = include_str!("backend/macos/ax.rs");

    #[test]
    fn system_events_are_delivered_first_and_acted_on() {
        let drain = body(QUEUE, "pub fn drain(events: &mut dyn HostEvents)");
        let system = drain.find("super::system::drain(events);").expect("queue::drain no longer delivers system events");
        let hotkeys = drain.find("HOTKEYS").expect("queue::drain has no hotkeys");
        assert!(system < hotkeys, "system events are delivered after the hotkeys of the same drain");
        // One batch from the host's queue; the backend's own part of it done BEFORE it is handed
        // over, as Windows resets duplication first: the host's reaction captures and reads the
        // front at once, and must find the probe brought forward and the front application known.
        let sys_drain = body(SYSTEM, "pub fn drain(events: &mut dyn HostEvents)");
        let take = sys_drain.find("crate::system_events::take()").expect("system::drain no longer takes the host's queue");
        let hand = sys_drain.find("events.on_system(batch);").expect("system::drain no longer hands the batch over");
        let own = sys_drain.find("own_part(s.event);").expect("system::drain no longer does the backend's part");
        let front = sys_drain.find("super::watch::recheck_front(!host_reports_front);").expect("the front is no longer read again");
        assert!(take < own && own < hand && front < hand, "the backend's part runs after the host heard the event");
        assert!(sys_drain.contains("s.event.front_may_have_changed()"));
        assert!(sys_drain.contains("let host_reports_front = batch.iter().any(|s| s.event.reports_front());"));
        let own_part = body(SYSTEM, "fn own_part(kind: SystemEvent)");
        for call in [
            "recheck_soon(super::tap::RECHECK_WAKE)",
            "recheck_soon(super::tap::RECHECK_UNLOCK)",
            "recheck_soon(super::tap::RECHECK_SESSION)",
            "kind.rechecks_capture()",
            "super::capture::clear_sck_backoff(",
        ] {
            assert!(own_part.contains(call), "system::own_part no longer does `{call}`");
        }
        // A front application taken over for an event the host reports the front after is not
        // queued as an activation as well: that reported the same window twice in one drain.
        let activated = body(WATCH, "fn activated(app: &NSRunningApplication, announce: bool)");
        assert!(activated.contains("if announce {\n        super::queue::push_activated(window);\n    } else {\n        super::queue::taken_over(window);"));
        assert!(body(QUEUE, "pub fn taken_over(window: isize)").contains("arm_recheck_ladder();"));
        // The display lines after a change are the host's, read through the backend.
        assert!(body(MAC, "fn display_environment(&self)").contains("system::display_lines()"));
        let on_system = body(LIB, "fn on_system(&mut self, events: Vec<system_events::Stamped>)");
        for call in [
            "self.shared.system_since.borrow_mut().plan(",
            "self.shared.bump_input_epoch();",
            "self.shared.speech.on_system(s.event);",
            "self.on_focus_change();",
        ] {
            assert!(on_system.contains(call), "the host's on_system no longer does `{call}`");
        }
        assert!(body(LIB, "fn on_tick(&mut self)").contains("self.shared.log_housekeeping();"), "counted display changes are never written");
        assert!(body(LIB, "fn log_housekeeping(&self)").contains("self.system_since.borrow_mut().due(now)"));
        assert!(body(SPEECH, "pub fn on_system(").contains("kind.retries_screen_reader()"));
    }

    #[test]
    fn the_voiceover_path_is_looked_at_and_bounded() {
        assert!(body(SPEECH, "pub fn pump(&self)").contains("self.vo.tick(wanted);"), "nothing reopens a parked VoiceOver path");
        let say = body(VO, "pub fn say(&self, text: &str, interrupt: bool) -> bool");
        assert!(say.contains(".handed()"), "a look no longer lets only one line through");
        let run = body(VO, "fn run(");
        let skip = run.find("refused_in.is_some_and(").expect("the worker offers lines from a refused opening again");
        let output = run.find("output(&u.text").expect("the worker no longer says lines");
        assert!(skip < output, "the refused-opening check comes after the line was already offered");
        let out = body(VO, "fn output(");
        for call in ["automation_status()", "vo_park::after_event(", "vo_park::event_broken("] {
            assert!(out.contains(call), "output no longer uses `{call}`");
        }
    }

    /// VoiceOver's AppleScript box (`speech::vo_script`, whose rules are tested there): where it
    /// is asked, where its answer is obeyed, and where it is shown. A Mac session lost every line
    /// to it in silence; reverting any of these call sites would leave every other test green.
    #[test]
    fn voiceover_s_applescript_box_is_asked_obeyed_and_shown() {
        const PERM: &str = include_str!("backend/macos/perm.rs");
        const GUI: &str = include_str!("gui.rs");
        // The worker asks before it offers, and offers nothing VoiceOver would drop.
        let run = body(VO, "fn run(");
        let ask = run.find("ask_voiceover(reads, &accepts, &notes, &mut asked);").expect("the worker never asks");
        let gate = run.find("|| !accepts.load(Ordering::Relaxed)").expect("the worker offers lines VoiceOver would drop");
        let output = run.find("output(&u.text").expect("the worker no longer says lines");
        assert!(ask < gate && gate < output, "a line is offered before the box was asked about");
        // The event loop refuses a line while the box is not ticked, before a look's one line is spent.
        let say = body(VO, "pub fn say(&self, text: &str, interrupt: bool) -> bool");
        let refuses = say.find("!self.accepts.load(Ordering::Relaxed)").expect("say no longer asks the box's gate");
        assert!(refuses < say.find(".handed()").expect("a look"), "a look is spent on a line VoiceOver would drop");
        // Raised only by an answer the event loop has heard; the worker only lowers.
        let apply = body(VO, "fn apply(&self, out: vo_script::Out, answer: bool)");
        assert!(apply.contains("} else if answer {
            self.accepts.store(true"), "raised by a trigger");
        assert_eq!(VO.matches("self.apply(out, true);").count(), 2, "the two answers: the worker's, and not asked");
        assert_eq!(VO.matches("self.apply(out, false);").count(), 2, "the trigger and the look");
        let asks = body(VO, "fn ask_voiceover(");
        assert!(!asks.contains("accepts.store(true"), "the worker raises against a newer trigger");
        // Asked at start, of every VoiceOver process, when the setting is ticked — on the next
        // pass, not the next line — when VoiceOver Utility quits, and on the looks while parked.
        assert!(body(VO, "pub fn note_pid(&self, pid: Option<i32>)").contains("self.asked_pid.replace(Some(now))"));
        assert!(body(VO, "pub fn rearm(&self)").contains("self.asked_pid.set(None);"));
        assert!(body(SPEECH, "pub fn new() -> Result<Self>").contains("speech.vo.note_pid(voiceover::running_pid());"));
        let pump = body(SPEECH, "pub fn pump(&self)");
        assert!(pump.contains("self.vo.rearm();
                self.vo.note_pid(voiceover::running_pid());"));
        assert!(pump.contains("|| self.chosen.borrow().values().any(|id| id == VOICEOVER_ID);"), "a module's choice");
        let tell = pump.find("self.vo.take_tell()").expect("the user is never told");
        let tick = pump.find("self.vo.tick(wanted);").expect("the tick");
        let refused = pump.find("for text in self.vo.refused()").expect("the refused lines");
        assert!(tick < tell && tell < refused, "told after the lines it explains, or before the answer");
        let told = &pump[tell..refused];
        assert!(told.contains("self.av.say(text, false, None);"), "told through VoiceOver, which drops it");
        assert!(told.contains("self.vo.say(text, false)") && !told.contains("self.say(text"), "the word through the system voice");
        let tick = body(VO, "pub fn tick(&self, wanted: bool)");
        for call in [
            "UTILITY_QUIT.swap(false, Ordering::Relaxed)",
            "self.check(\"VoiceOver Utility quit\"",
            "self.script.borrow().due(now)",
            ".looked(&reads, now, running)",
            "Note::Checked { reads, probe } =>",
        ] {
            assert!(tick.contains(call), "VoiceOver::tick no longer does `{call}`");
        }
        assert!(
            SYSTEM.contains("crate::speech::application_quit(bundle_id.as_deref(), bundle_file.as_deref());"),
            "VoiceOver Utility quitting is not heard"
        );
        // The question asks nobody anything: only with the Automation permission granted; and
        // -1744 holds the sentence back only while the switch's own request is out.
        assert!(asks.contains("match automation_status() {
        0 => {"));
        assert!(asks.contains("vo_script::classify(&run_child(&vo_script::PROBE, true))"));
        assert!(asks.contains("status => vo_script::unasked(status, crate::backend::voiceover_automation_asking()),"));
        let request = body(PERM, "pub fn request_voiceover_automation()");
        let up = request.find("AUTOMATION_ASKING.fetch_add(1, Ordering::SeqCst);").expect("the request is not counted");
        let spawn = request.find("std::thread::spawn(").expect("the request's thread");
        let asked = request.find("let after = ask_tcc(true);").expect("the request");
        let down = request.find("AUTOMATION_ASKING.fetch_sub(1, Ordering::SeqCst);").expect("never counted down");
        assert!(up < spawn && spawn < asked && asked < down, "counted while the dialog cannot be up, or not while it is");
        // Shown: the start-up block, from the reads alone, and the Permissions page.
        assert!(PERM.contains("push(\"voiceover applescript\", crate::speech::voiceover_applescript_env());"));
        assert!(SPEECH.contains("vo_script::env_value(&voiceover::read_box())"), "the [env] line depends on timing");
        assert_eq!(GUI.matches("crate::speech::voiceover_applescript_page()").count(), 2, "built and re-checked");
    }

    #[test]
    fn capture_backs_off_and_the_sweep_runs_from_the_pump() {
        let grab = body(CAPTURE, "fn grab(x: i32, y: i32, w: i32, h: i32, best: bool)");
        assert!(grab.contains("sck_backoff().attempt(Instant::now())"), "ScreenCaptureKit is no longer asked through its back-off");
        assert!(grab.contains("sck_probe(rect, since)"), "a probe is waited for again where an older function could answer");
        assert!(!CAPTURE.contains("SCK_DISABLED"), "the session-long switch is back");
        let pump = body(MAC, "fn pump_pending(&self, events: &mut dyn HostEvents)");
        let drain = pump.find("queue::drain(events);").expect("pump_pending no longer drains");
        let sweep = pump.find("handles::sweep_if_due();").expect("nothing sweeps the handle table");
        assert!(drain < sweep, "the sweep runs before the drain it should follow");
        assert!(!body(HANDLES, "fn intern_as(").contains("sweep"), "an intern sweeps again, inside a hot path");
        assert!(body(AX, "pub(super) fn window_id(handle: isize) -> u32").contains("handles::set_window_id(handle, found)"));
    }

    /// Where the uptime change and the Screen Recording request meet. The request's look at
    /// Accessibility runs in the same pump, before the drain that delivers a wake or a lock; it
    /// keeps App Nap away while it waits — the uptime change's activity was held only for
    /// captured keys and controllers, and a napped pump would look late — and the request, from
    /// the pump or the button, lets it go before anything can return early. The first hold is
    /// written per reason, so that one for this does not make the first overlay's a trace line —
    /// a rule `activity_reasons.rs` decides and tests; here only that `want` follows it.
    #[test]
    fn the_screen_recording_request_waits_in_the_pump_and_unnapped() {
        const PERM: &str = include_str!("backend/macos/perm.rs");
        const ACTIVITY: &str = include_str!("backend/macos/activity.rs");
        let pump = body(MAC, "fn pump_pending(&self, events: &mut dyn HostEvents)");
        let asks = pump.find("perm::pump();").expect("nothing makes the automatic Screen Recording request");
        let drain = pump.find("queue::drain(events);").expect("pump_pending no longer drains");
        assert!(asks < drain);
        let waits = body(PERM, "pub fn pump()");
        let look = waits.find("AXIsProcessTrusted()").expect("the pump no longer looks at Accessibility");
        let hold = waits.find("super::activity::want(super::activity::SETUP, true);").expect("the wait is napped again");
        assert!(look < hold, "App Nap is held before Accessibility was found missing");
        let request = body(PERM, "fn ask_for_screen_recording(trigger: Trigger) -> Asked");
        let release = request
            .find("super::activity::want(super::activity::SETUP, false);")
            .expect("a request no longer ends the wait's App Nap activity");
        assert!(release < request.find("return").unwrap(), "a request that returns early keeps App Nap away");
        let want = body(ACTIVITY, "pub(crate) fn want(reason: u8, on: bool) -> bool");
        assert!(
            want.contains("change(WANTED.with(Cell::get), SAID.with(Cell::get), reason, on)"),
            "the activity no longer follows activity_reasons.rs, where the first hold is said per reason"
        );
        assert!(want.contains("SAID.with(|s| s.set(c.said));"), "what has been said is not kept");
    }
}

/// The end of a process that may have loaded ONNX Runtime, on a Mac: it is released after
/// everything that could still be inside it was stopped, and before `exit()` — by the end of
/// `run`, by `ocr-bench` before `main` hands its code to `exit`, and by `main`'s guard on every
/// other way out of `main`. A process that cannot release it ends through `end_now`. None of it
/// runs here (the release is the Mac's); it is checked where it is written.
#[cfg(test)]
mod exit_release_tests {
    use super::ocr_wiring_tests::body;

    const LIB: &str = include_str!("lib.rs");
    const BACKEND: &str = include_str!("backend/mod.rs");
    const MAIN: &str = include_str!("../../app/src/main.rs");

    #[test]
    fn the_exit_releases_onnx_runtime_last_and_ends_without_teardown_when_it_cannot() {
        let run = body(LIB, "pub fn run(dirs: &[String]) -> Result<()> {");
        let at = |what: &str| run.find(what).unwrap_or_else(|| panic!("{what} is no longer in run's exit"));
        let steps = [
            "stop.shutdown(ocr::policy::SHUTDOWN)",
            "backend::shutdown_capture();",
            "backend::ocr_exit_report();",
            "backend::stop_ocr_warmup();",
            "h.join()",
            "backend::settle_ocr();",
            "let released = backend::release_ocr();",
            "drop(instance);",
            "backend::end_now(i32::from(result.is_err()));",
        ];
        for pair in steps.windows(2) {
            assert!(at(pair[0]) < at(pair[1]), "{} comes before {}", pair[0], pair[1]);
        }
        assert!(at("if !released {") < at("backend::end_now("), "ended early only when nothing was released");

        let guard = body(LIB, "impl Drop for ReleaseAtExit {");
        assert!(guard.contains("if !backend::release_ocr() {"));
        assert!(guard.contains("backend::end_now(if std::thread::panicking() { 101 } else { 0 });"));

        let bench = body(BACKEND, "pub(crate) fn ocr_bench(args: &[String]) -> i32 {");
        assert!(bench.find("macos::ocr::bench::run(args)").unwrap() < bench.find("release_ocr()").unwrap());
        assert!(bench.contains("end_now(code);"), "the bench's own code, when it cannot release");
    }

    /// `main` holds the guard from its start, before anything that could load ONNX Runtime and
    /// before `ocr-bench`, whose `std::process::exit` drops nothing — that one releases itself.
    #[test]
    fn main_holds_the_guard_before_anything_else_runs() {
        let main = body(MAIN, "fn main() -> Result<()> {");
        let guard = main.find("let _ort = host::ReleaseAtExit;").expect("main no longer holds the guard");
        for later in ["attach_parent_console();", "host::ocr_bench(", "host::run(&dirs)"] {
            assert!(guard < main.find(later).unwrap_or_else(|| panic!("{later}")), "the guard before {later}");
        }
    }
}
