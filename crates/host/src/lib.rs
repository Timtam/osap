//! Host runtime: a module **manager** that hosts many Luau modules concurrently
//! in one process (one VM each, shared services + one event loop), per
//! `docs/module-runtime-and-lifecycle.md`. The OS is reached only through
//! [`backend::Backend`]. The `host` API (design: primitives are first-class —
//! see `docs/host-api-capability-catalog.md`) is installed per module VM and
//! bound to that module's root + the shared services.

mod backend;
mod gui;
mod logging;
pub mod registry;
mod settings;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, Table};
use tts::Tts;

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
    tts: RefCell<Tts>,
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
    epoch: Cell<u64>,
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
            let mut frames: Vec<((i32, i32, i32, i32), Option<CapturedImage>)> = Vec::new();
            for t in &batch {
                let idx = match frames.iter().position(|(r, _)| *r == t.region) {
                    Some(i) => i,
                    None => {
                        let (rx, ry, rw, rh) = t.region;
                        frames.push((t.region, capture(rx, ry, rw, rh)));
                        frames.len() - 1
                    }
                };
                let (rx, ry, _, _) = t.region;
                let hit = frames[idx].1.as_ref().and_then(|cap| {
                    // Templates in order against this one frame; first hit wins.
                    t.tmpls.iter().enumerate().find_map(|(n, tm)| {
                        find_template_scaled(cap, tm.w, tm.h, &tm.rgba, t.tol, &t.scales)
                            .map(|(ox, oy, mw, mh)| (rx + ox as i32, ry + oy as i32, mw, mh, n + 1))
                    })
                });
                if results.send(ImageResult { id: t.id, hit }).is_err() {
                    return; // main thread gone
                }
            }
        }
    });
}

impl Shared {
    fn root(&self, idx: usize) -> PathBuf {
        self.roots.borrow()[idx].clone()
    }
    /// Marks the world as possibly changed (see the `epoch` field).
    fn bump_epoch(&self) {
        self.epoch.set(self.epoch.get().wrapping_add(1));
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
        let dec = Arc::new(Decoded { w: img.width(), h: img.height(), rgba: img.into_raw() });
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

    /// The enabled module that already owns global hotkey `binding` (if any) —
    /// includes the registering module itself, so a redundant self-rebind is also
    /// caught (and skipped) instead of failing against the OS.
    fn hotkey_owner(&self, binding: (u32, u8)) -> Option<usize> {
        let enabled = self.enabled.borrow();
        self.hotkeys
            .borrow()
            .values()
            .find(|reg| {
                reg.binding == Some(binding) && enabled.get(reg.module_idx).copied().unwrap_or(false)
            })
            .map(|reg| reg.module_idx)
    }

    /// Whether module `idx` is currently enabled.
    fn is_enabled(&self, idx: usize) -> bool {
        self.enabled.borrow().get(idx).copied().unwrap_or(false)
    }

    /// Surfaces a cross-module binding clash (two modules want the same hotkey or
    /// captured key): logs it + queues an accessible dialog naming both modules.
    /// First-come keeps the binding; the later module stays loaded with it inactive.
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
            format!("{me}\u{1}conflict\u{1}{kind}\u{1}{spec}"),
            "Binding conflict".to_string(),
            format!(
                "Module \u{201c}{me}\u{201d} tried to bind the {kind} {spec}, but module \u{201c}{owner}\u{201d} already uses it. {spec} stays with \u{201c}{owner}\u{201d} \u{2014} disable one of them (then restart) to switch."
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
        for (id, reg) in self.hotkeys.borrow().iter() {
            if reg.module_idx != idx {
                continue;
            }
            if enabled {
                let _ = self.backend.register_hotkey(*id, &reg.spec);
            } else {
                self.backend.unregister_hotkey(*id);
            }
        }
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
        self.schemas.borrow_mut().truncate(n);
        self.exports.borrow_mut().remove(failed_id);
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
        while let Ok(res) = self.image_results.try_recv() {
            let entry = self.pending_image.borrow_mut().remove(&res.id);
            if let Some((lua, cb, idx)) = entry {
                // A result arriving is a fresh observation of the screen, and its callback
                // may re-check an overlay — bump per delivery, not per (mostly empty) poll.
                self.bump_epoch();
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
    let dir = match find_sibling_module(parent, dep_id) {
        Some(d) => d,
        None if optional => return Ok(()), // absent optional dependency — skip
        None => anyhow::bail!("dependency '{dep_id}' not found in {}", parent.display()),
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
    let dep_full = install_host_api(lua, shared, dep_idx)?;
    let host = lua.create_table()?;
    for key in ["path", "resource", "settings", "config", "screen", "sound"] {
        let v: mlua::Value = dep_full.get(key)?;
        host.set(key, v)?;
    }
    let mt = lua.create_table()?;
    mt.set("__index", host_owner)?;
    host.set_metatable(Some(mt))?;
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
    lua.globals().set("host", &host_m)?;
    lua.load(WINDOW_PRELUDE).set_name("window_prelude").exec()?;

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
        if let mlua::Value::Table(_) = dep_ret {
            reg.set(dep_id.as_str(), dep_ret)?;
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
                let dep_dir = find_sibling_module(&parent, dep).ok_or_else(|| {
                    anyhow::anyhow!(
                        "module '{id}' depends on '{dep}', not found in {}",
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
                if let Some(dep_dir) = find_sibling_module(&parent, dep) {
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
    shared.schemas.borrow_mut().push(HashMap::new());

    // Everything past the four parallel-vector pushes above is fallible
    // (install, prelude, reading + running the entry, exports conversion). At
    // startup a failure aborts the process, but a runtime hot-load must never
    // leave the shared vectors longer than `modules` — that would mis-index
    // every later module. Run the fallible work in a scope and, on any error,
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
    logging::line("manager", &format!("reloaded module: {old_id}"));

    let graph: Vec<(String, Vec<String>)> = modules
        .borrow()
        .iter()
        .map(|m| (m.id.clone(), m.dependencies.clone()))
        .collect();
    Ok(registry::transitive_dependents(&old_id, &graph))
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
        module_idx: idx,
        enabled: shared.enabled.borrow().get(idx).copied().unwrap_or(true),
        dependencies: m.dependencies.clone(),
        settings,
    }
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
        let tts = Tts::default().context("failed to initialize TTS engine")?;
        let store = settings::Store::load();
        let disabled_ids = store.disabled_ids();
        let (image_tasks, image_task_rx) = std::sync::mpsc::channel::<ImageTask>();
        let (image_result_tx, image_results) = std::sync::mpsc::channel::<ImageResult>();
        // Capture fn pointer taken before `backend` is moved into Shared — it's `Copy`
        // and `Send`, so the worker can capture without the non-`Send` Rc backend.
        spawn_image_worker(backend.capture_fn(), image_task_rx, image_result_tx);
        let shared = Rc::new(Shared {
            backend,
            tts: RefCell::new(tts),
            audio: RefCell::new(None),
            next_id: Cell::new(0),
            roots: RefCell::new(Vec::new()),
            ids: RefCell::new(Vec::new()),
            enabled: RefCell::new(Vec::new()),
            hotkeys: RefCell::new(HashMap::new()),
            keys: RefCell::new(Vec::new()),
            store: RefCell::new(store),
            schemas: RefCell::new(Vec::new()),
            on_change: RefCell::new(HashMap::new()),
            dirty: Cell::new(false),
            timers: RefCell::new(Vec::new()),
            exports: RefCell::new(HashMap::new()),
            arbiter: RefCell::new(HashMap::new()),
            epoch: Cell::new(0),
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
        });
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

    /// Runs the shared event loop if any module registered hotkeys, keys, or
    /// window triggers; otherwise waits for pending speech and returns.
    pub fn run(&mut self) -> Result<()> {
        let has_hotkeys = !self.shared.hotkeys.borrow().is_empty();
        let has_keys = !self.shared.keys.borrow().is_empty();
        let has_triggers = self.modules.borrow().iter().any(|m| window_has_triggers(&m.lua));
        let headless = std::env::var_os("AUTOMATION_PLATFORM_HEADLESS").is_some();

        // The tray manager is shown whenever there's a window (non-headless), even
        // with nothing loaded yet, so modules can be browsed/installed/managed.
        // Headless has no window, so it only runs with an OS trigger registered.
        if has_hotkeys || has_keys || has_triggers || !headless {
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
                let module_infos: Vec<gui::ModuleInfo> = self
                    .modules
                    .borrow()
                    .iter()
                    .enumerate()
                    .map(|(i, m)| module_info(&self.shared, m, i))
                    .collect();
                let backend = self.shared.backend.clone();
                let shared = self.shared.clone();
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
                gui::run_gui(
                    module_infos,
                    move |idx, enabled| toggle_shared.set_enabled(idx, enabled),
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
                    // Reload the selected module's VM in place from its source dir.
                    move |idx| {
                        let deps = reload_module(&reload_shared, &reload_modules, idx)
                            .map_err(|e| format!("{e:#}"))?;
                        let info = {
                            let mods = reload_modules.borrow();
                            module_info(&reload_shared, &mods[idx], idx)
                        };
                        Ok((info, deps))
                    },
                    move || {
                        let mods = modules.borrow();
                        let mut dispatcher = Dispatcher {
                            shared: &shared,
                            modules: &mods[..],
                        };
                        backend.pump_pending(&mut dispatcher);
                        shared.fire_due_timers();
                        shared.fire_image_results();
                        // A module asked for a re-check (host.window.recheck) after
                        // changing plugin UI itself — dispatch it like an OS focus event.
                        if shared.recheck_requested.replace(false) {
                            dispatcher.on_focus_change();
                        }
                        shared.flush_if_dirty();
                    },
                    move || errors_shared.drain_errors(),
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
            let speaking = self.shared.tts.borrow().is_speaking().unwrap_or(false);
            if !speaking || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Convenience entry: load each directory as a module and run them together.
pub fn run(dirs: &[String]) -> Result<()> {
    logging::init();
    let warmup = backend::warmup_ocr(); // preload the neural OCR model off the hot path
    let result = (|| -> Result<()> {
        let mut manager = Manager::new()?;
        for dir in dirs {
            manager.load(dir)?;
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
    result
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
    fn on_hotkey(&mut self, id: i32) {
        self.shared.bump_epoch();
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
        self.shared.bump_epoch();
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
    }

    fn on_focus_change(&mut self) {
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
    }
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
            sh.tts
                .borrow_mut()
                .speak(text, interrupt)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
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
            if sh.is_enabled(idx) {
                // Conflict: this combo is already held by another enabled module
                // (surface the cross-module clash) or already by this one (a
                // redundant self-rebind). Skip either way — first-come keeps it and
                // the module stays loaded with this binding inactive.
                if let Some(b) = binding {
                    if let Some(owner) = sh.hotkey_owner(b) {
                        if owner != idx {
                            sh.report_conflict(idx, owner, "hotkey", &spec);
                        }
                        return Ok(0);
                    }
                }
                if let Err(e) = sh.backend.register_hotkey(id, &spec) {
                    // binding Some ⇒ a valid combo the OS rejected (another app holds
                    // it) → surface as a conflict, keep the module loaded. binding
                    // None ⇒ the spec didn't parse → a module-author bug; fail loudly
                    // with the real error (matching host.keys.capture).
                    if binding.is_some() {
                        sh.report_os_conflict(idx, "hotkey", &spec, &e);
                        return Ok(0);
                    }
                    return Err(mlua::Error::external(e));
                }
            }
            let key = lua.create_registry_value(cb)?;
            sh.hotkeys.borrow_mut().insert(
                id,
                HotkeyReg { module_idx: idx, lua: lua.clone(), cb: key, spec, binding },
            );
            Ok(id)
        })?,
    )?;
    let sh = shared.clone();
    hk.set(
        "unregister",
        lua.create_function(move |_, id: i32| {
            sh.backend.unregister_hotkey(id);
            sh.hotkeys.borrow_mut().remove(&id);
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
    host.set("keys", keys)?;

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

    // host.window.list() / host.window.active()  (find/findAll/onTrigger via prelude)
    let win = lua.create_table()?;
    let sh = shared.clone();
    win.set(
        "list",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for w in sh.backend.enumerate_windows() {
                t.push(win_to_table(lua, &w)?)?;
            }
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    win.set(
        "active",
        lua.create_function(move |lua, ()| match sh.backend.active_window() {
            Some(w) => Ok(Some(win_to_table(lua, &w)?)),
            None => Ok(None),
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
            let t = lua.create_table()?;
            for c in sh.backend.window_controls(hwnd) {
                t.push(control_to_table(lua, &c)?)?;
            }
            Ok(t)
        })?,
    )?;
    // host.window.focusChain() — controls from the focused element up to its
    // top-level window, for detecting focus inside an embedded plugin.
    let sh = shared.clone();
    win.set(
        "focusChain",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for c in sh.backend.window_focus_chain() {
                t.push(control_to_table(lua, &c)?)?;
            }
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

    // host.uia.find(hwnd, name, controlType) — does the window's UI Automation
    // subtree contain an element with that Name + ControlType? (Plugin identity.)
    let uia = lua.create_table()?;
    let sh = shared.clone();
    uia.set(
        "find",
        lua.create_function(move |_, (hwnd, name, ctype): (isize, String, i32)| {
            Ok(sh.backend.uia_find(hwnd, &name, ctype))
        })?,
    )?;
    // host.uia.type — the UIA ControlType ids, by name. Modules were declaring these as
    // local constants (and writing bare integers where they hadn't), which reads as a
    // magic number at every call site: `host.uia.find(id, "Kontakt 8", 50033)`.
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
    // host.uia.findAny(hwnd, names, types) -> index | nil — "is any of these names
    // present as any of these control types?", the shape a plugin-identity check takes
    // ("Kontakt 8" as Window OR Pane). One tree traversal per NAME rather than one per
    // name×type pair, and it returns WHICH name matched (1-based), so a caller can read
    // the plugin's version straight out of the answer instead of asking once per version.
    let sh = shared.clone();
    uia.set(
        "findAny",
        lua.create_function(move |_, (hwnd, names, types): (isize, Vec<String>, Vec<i32>)| {
            Ok(sh.backend.uia_find_any(hwnd, &names, &types))
        })?,
    )?;
    // host.uia.locate(hwnd, name, controlType) -> { x, y } (screen centre of the
    // matching element, to click it) or nil. For driving plugin UI via UIA.
    let sh = shared.clone();
    uia.set(
        "locate",
        lua.create_function(move |lua, (hwnd, name, ctype): (isize, String, i32)| {
            match sh.backend.uia_locate(hwnd, &name, ctype) {
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
    // host.uia.locateVia(hwnd, viaName, viaType, name, controlType) -> { x, y } | nil.
    // Like locate, but descends into a container element (viaName/viaType) first and
    // searches within it — reaches a plugin's UI that hangs off an identity pane as a
    // nested UIA fragment (a DAW-embedded Kontakt). Ports ReaHotkey's two-level find.
    let sh = shared.clone();
    uia.set(
        "locateVia",
        lua.create_function(
            move |lua, (hwnd, via_name, via_type, name, ctype): (isize, String, i32, String, i32)| {
                match sh.backend.uia_locate_via(hwnd, &via_name, via_type, &name, ctype) {
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
    // host.uia.pluginLocate(hwnd, containerName, name, controlType) -> { x, y } | nil.
    // ReaHotkey's GetPluginUIAElement + FindElement: find the element that IS the plugin
    // (containerName, Window/Pane, QuickWindow class preferred) and search within it over
    // the RAW tree — the only view that reaches a DAW-embedded plugin's hosted Qt UI.
    let sh = shared.clone();
    uia.set(
        "pluginLocate",
        lua.create_function(
            move |lua, (hwnd, container, name, ctype): (isize, String, String, i32)| {
                match sh.backend.uia_plugin_locate(hwnd, &container, &name, ctype) {
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
    // host.uia.dump(hwnd) -> { {depth, name, class, ctype}, … } — diagnostic UIA
    // tree walk (raw view) for discovering plugin identity properties.
    let sh = shared.clone();
    uia.set(
        "dump",
        lua.create_function(move |lua, hwnd: isize| {
            let arr = lua.create_table()?;
            for (i, (depth, name, class, ctype)) in sh.backend.uia_dump(hwnd).into_iter().enumerate() {
                let t = lua.create_table()?;
                t.set("depth", depth)?;
                t.set("name", name)?;
                t.set("class", class)?;
                t.set("ctype", ctype)?;
                arr.set(i + 1, t)?;
            }
            Ok(arr)
        })?,
    )?;
    // host.uia.rawDump(hwnd) — like dump, but over the RAW TreeWalker, which crosses
    // into a hosted Qt fragment (a DAW-embedded plugin's real UI).
    let sh = shared.clone();
    uia.set(
        "rawDump",
        lua.create_function(move |lua, hwnd: isize| {
            let arr = lua.create_table()?;
            for (i, (depth, name, class, ctype)) in
                sh.backend.uia_raw_dump(hwnd).into_iter().enumerate()
            {
                let t = lua.create_table()?;
                t.set("depth", depth)?;
                t.set("name", name)?;
                t.set("class", class)?;
                t.set("ctype", ctype)?;
                arr.set(i + 1, t)?;
            }
            Ok(arr)
        })?,
    )?;
    // host.uia.classNavPoint(hwnd, className, ctype, child, sibling) -> {x,y} | nil —
    // click-point of the element reached from the first ClassName-contains +
    // ControlType match by walking `child` (nth child) then `sibling` siblings. Ports
    // ReaHotkey's FindElement(ClassName) + WalkTree(path).Click.
    let sh = shared.clone();
    uia.set(
        "classNavPoint",
        lua.create_function(
            move |lua, (hwnd, class, ctype, child, sibling): (isize, String, i32, i32, i32)| {
                match sh.backend.uia_class_nav_point(hwnd, &class, ctype, child, sibling) {
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
    // host.uia.focusStep(hwnd, direction) -> { name, ctype, index, count } | nil —
    // Tab pass-through for a standalone plugin window: SetFocus the next (direction>=0)
    // / previous keyboard-focusable descendant relative to the current focus, wrapping
    // at the ends, and return the newly focused element so the overlay can announce it.
    let sh = shared.clone();
    uia.set(
        "focusStep",
        lua.create_function(move |lua, (hwnd, direction): (isize, i32)| {
            match sh.backend.uia_focus_step(hwnd, direction) {
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
    host.set("uia", uia)?;

    // host.screen.pixel / .size / .imageSearch
    let screen = lua.create_table()?;
    let sh = shared.clone();
    screen.set(
        "pixel",
        lua.create_function(move |lua, (x, y): (i32, i32)| {
            let (r, g, b) = sh.backend.pixel(x, y);
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
            match find_template_scaled(&cap, tmpl.w, tmpl.h, &tmpl.rgba, tol, &scales) {
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
                    find_template_scaled(&cap, tmpl.w, tmpl.h, &tmpl.rgba, tol, &scales)
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

    // host.ocr.recognize({ region, lang }) -> { text, words = {{text,x,y,w,h}, ...} }
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
            Ok(t)
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
            sh.bump_epoch();
            sh.backend.mouse_move(x, y);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "click",
        lua.create_function(move |_, (x, y, opts): (i32, i32, Option<Table>)| {
            sh.bump_epoch();
            sh.backend.mouse_click(x, y, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "drag",
        lua.create_function(move |_, (x1, y1, x2, y2, opts): (i32, i32, i32, i32, Option<Table>)| {
            sh.bump_epoch();
            sh.backend.mouse_drag(x1, y1, x2, y2, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "scroll",
        lua.create_function(move |_, (x, y, amount): (i32, i32, i32)| {
            sh.bump_epoch();
            sh.backend.mouse_scroll(x, y, amount);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "send",
        lua.create_function(move |_, combo: String| {
            sh.bump_epoch();
            sh.backend.key_send(&combo).map_err(mlua::Error::external)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "text",
        lua.create_function(move |_, text: String| {
            sh.bump_epoch();
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

    Ok(host)
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
fn find_template(hay: &CapturedImage, tw: u32, th: u32, tmpl: &[u8], tol: u8) -> Option<(u32, u32)> {
    if tw == 0 || th == 0 || tw > hay.w || th > hay.h {
        return None;
    }
    for oy in 0..=(hay.h - th) {
        for ox in 0..=(hay.w - tw) {
            if matches_at(hay, ox, oy, tw, th, tmpl, tol) {
                return Some((ox, oy));
            }
        }
    }
    None
}

fn matches_at(hay: &CapturedImage, ox: u32, oy: u32, tw: u32, th: u32, tmpl: &[u8], tol: u8) -> bool {
    let tol = tol as i16;
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
            find_template(hay, tw, th, tmpl, tol)
        } else {
            let scaled = resize_rgba(tmpl, tw, th, sw, sh);
            find_template(hay, sw, sh, &scaled, tol)
        };
        if let Some((ox, oy)) = hit {
            return Some((ox, oy, sw, sh));
        }
    }
    None
}

/// Converts a native window snapshot into the Lua table modules see:
/// `{ id, title, class, app = { name, exe, pid }, bounds = { x, y, w, h } }`.
/// Finds a sibling module directory (under `parent`) whose manifest id matches —
/// for auto-discovering declared dependencies.
fn find_sibling_module(parent: &Path, id: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(parent).ok()?.flatten() {
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
    t.set("client", cl)?;
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
    t.set("client", cl)?;

    Ok(t)
}
