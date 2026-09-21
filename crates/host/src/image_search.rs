//! Image search as a module sees it: template handles, the five search bindings, the worker
//! that captures and matches off the event loop, and the delivery of its answers.
//!
//! The matcher is `template.rs`; this file is everything around it that knows about Lua, about
//! modules, or about threads. It lives outside `lib.rs` so that the part of the host every
//! feature touches stays small, and so that the pieces with rules of their own — who a result
//! belongs to, what a module may hold, what happens when a search panics — can be read, and
//! tested, in one place.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{
    Function, Lua, MetaMethod, RegistryKey, Table, UserData, UserDataFields, UserDataMethods,
    Value,
};
use rayon::prelude::*;

use crate::backend::{CaptureFn, CaptureSource, CapturedImage};
use crate::capture_source;
use crate::template::{self, Decoded, Rect};
use crate::{call_guarded, logging, read_region, read_scales, Shared};

// ── Who a search belongs to ─────────────────────────────────────────────────────────────

/// The module that OWNS a Lua state, and which incarnation of it that state is.
///
/// Stored in the state's app data by `register_vm` before any module code runs, so every
/// binding can ask "whose VM am I running in?" — a different question from the `idx` a
/// binding was installed with. For a code dependency's code those two differ: its bindings are
/// scoped to the DEPENDENCY's identity (paths resolve under its root), while the VM, its
/// callbacks and its lifetime belong to the module that loaded it.
///
/// That difference was a real bug. An async search remembered the scoped index, and the
/// drain delivered only while THAT module was enabled — so disabling the overlay runtime in
/// the manager silently dropped every dependent's landmark callback, and each landmark gate's
/// in-flight flag stayed set for good: the poll kept firing and never searched again. (The
/// same wedge, for the owner's OWN disable, is what `Fate::Hold` is for.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VmOwner {
    pub idx: usize,
    pub gen: u64,
}

/// Generations are process-wide and never reused, so a result can never be mistaken for one
/// asked by a later VM that happens to sit at the same index.
static NEXT_GEN: AtomicU64 = AtomicU64::new(1);

/// Marks `lua` as module `idx`'s VM, a fresh generation of it. Called by `populate_vm` before
/// the host is installed, so nothing a module runs can precede it.
pub(crate) fn register_vm(shared: &Shared, lua: &Lua, idx: usize) {
    let gen = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
    // `try_`: a fresh state has nothing borrowed, so this cannot fail in practice — and if it
    // ever did, a search would fall back to its scoped index (see `owner_of`) rather than the
    // application panicking while it loads a module.
    let _ = lua.try_set_app_data(VmOwner { idx, gen });
    shared.vm_gens.borrow_mut().insert(idx, gen);
}

/// The owner of the VM a binding is running in, and that VM's generation.
///
/// For a state `register_vm` never tagged — none in production: `populate_vm` tags every VM
/// before any module code runs, so this is only the path of a failed `try_set_app_data` — it is
/// the binding's own index at that index's current generation. That is the rule from before
/// owners were recorded plus the reload check: answered while that module is enabled and has
/// not been reloaded. With no generation recorded for the index either, the generation is 0,
/// which no VM ever has (`NEXT_GEN` starts at 1), so `fate` drops the answer: there is no VM
/// the host knows of to hand it to.
fn owner_of(lua: &Lua, gens: &HashMap<usize, u64>, scope: usize) -> (usize, u64) {
    if let Ok(Some(o)) = lua.try_app_data_ref::<VmOwner>() {
        return (o.idx, o.gen);
    }
    (scope, gens.get(&scope).copied().unwrap_or(0))
}

/// What becomes of an answer when it arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    /// The owner is enabled and is still the VM that asked: call back.
    Deliver,
    /// The owner is still the VM that asked, but disabled. Kept, and searched AGAIN when the
    /// owner is enabled (`Shared::resume_held_images`), so that the callback, when it comes,
    /// describes the screen the module can act on rather than one from before it was switched
    /// off.
    ///
    /// Dropping it instead was a wedge. A caller that waits for its callback before it searches
    /// again — the overlay runtime's landmark gate, which clears its in-flight flag only there
    /// — never searched again after a disable and re-enable, and kept answering with whatever
    /// it had last seen. For a library overlay that had matched, that meant claiming a match in
    /// every Kontakt window, over the library that was really loaded. `host.timer.every`
    /// promises a poll that survives a disable; a poll whose answer does not would break that.
    Hold,
    /// The VM that asked is gone — reloaded (a new generation at the same index), or rolled
    /// back after a failed hot-load (no index at all). Nobody is left to call.
    Drop,
}

fn fate(owner: usize, gen: u64, enabled: &[bool], gens: &HashMap<usize, u64>) -> Fate {
    if gens.get(&owner) != Some(&gen) {
        return Fate::Drop;
    }
    match enabled.get(owner) {
        Some(true) => Fate::Deliver,
        Some(false) => Fate::Hold,
        None => Fate::Drop,
    }
}

/// Whether the worker answers with the first entry that matched, or with every entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    First,
    Each,
}

impl Mode {
    fn binding(self) -> &'static str {
        match self {
            Mode::First => "imageSearchAsync",
            Mode::Each => "imageSearchEach",
        }
    }
}

/// An async search waiting for the worker's answer. Main thread only.
pub(crate) struct PendingImage {
    lua: Lua,
    cb: RegistryKey,
    /// The identity the call was made under — a code dependency's own, for its code. Named in
    /// an error report, because that is the code that raised.
    scope: usize,
    /// Whose VM the callback runs in, and which incarnation of it; see `VmOwner`.
    owner: usize,
    gen: u64,
    /// Each entry's name, travelling here rather than through the worker: the worker only
    /// ever holds pixels, and a name is a Lua-side affair.
    names: Vec<Option<String>>,
    mode: Mode,
    /// The search as it was sent, so it can be sent again after a `Fate::Hold`. `Arc`s and a
    /// few numbers: the pixels are shared with the worker's copy, not duplicated.
    task: ImageTask,
    /// Answered while its owner was disabled; waiting for the owner to be enabled again.
    held: bool,
}

/// A pending entry for a search made in `lua` under the identity `scope`, owned by the VM it
/// runs in (see `VmOwner`). The one place an entry is made, so the rule is tested here.
fn pending(
    lua: &Lua,
    gens: &HashMap<usize, u64>,
    scope: usize,
    cb: Function,
    names: Vec<Option<String>>,
    task: ImageTask,
) -> mlua::Result<PendingImage> {
    let (owner, gen) = owner_of(lua, gens, scope);
    Ok(PendingImage {
        lua: lua.clone(),
        cb: lua.create_registry_value(cb)?,
        scope,
        owner,
        gen,
        names,
        mode: task.mode,
        task,
        held: false,
    })
}

/// Takes every entry owned by module `owner`'s VM out of the map. Matched on the OWNER, never
/// on the scope the call was made under.
fn purge_owner(map: &mut HashMap<u64, PendingImage>, owner: usize) -> Vec<PendingImage> {
    let ids: Vec<u64> = map.iter().filter(|(_, p)| p.owner == owner).map(|(id, _)| *id).collect();
    ids.into_iter().filter_map(|id| map.remove(&id)).collect()
}

/// The held searches of `owner`, now that it is enabled: the ones asked by its current VM
/// (`gen`) are marked in flight again and returned to be sent; any held for an older VM at
/// that index are taken out, to be dropped. (A reload purges them first, so the second list is
/// empty in practice; it is here so that a held entry can never outlive the VM it belongs to.)
fn unhold(
    map: &mut HashMap<u64, PendingImage>,
    owner: usize,
    gen: Option<u64>,
) -> (Vec<ImageTask>, Vec<PendingImage>) {
    let mut again = Vec::new();
    let mut stale = Vec::new();
    for (id, p) in map.iter_mut() {
        if p.owner != owner || !p.held {
            continue;
        }
        if Some(p.gen) == gen {
            p.held = false;
            again.push(p.task.clone());
        } else {
            stale.push(*id);
        }
    }
    let stale = stale.into_iter().filter_map(|id| map.remove(&id)).collect();
    (again, stale)
}

/// Frees what finished or abandoned entries hold in their VMs' registries.
fn release(gone: impl IntoIterator<Item = PendingImage>) {
    for PendingImage { lua, cb, .. } in gone {
        let _ = lua.remove_registry_value(cb);
    }
}

// ── The worker ─────────────────────────────────────────────────────────────────────────

/// A queued async search. The worker CAPTURES `region` itself (off the main thread) then
/// matches `entries` against it; `region` = (x, y, w, h) in screen coords, whose (x, y) is
/// added back into every reported hit.
///
/// In `Mode::First` the entries are tried in order against the SAME captured frame and the
/// first hit wins — several renderings of the same thing (a dialog's close glyph as two plugin
/// versions draw it). Chaining single-template searches instead would pay a fresh region
/// capture (~1 compositor frame) per template. In `Mode::Each` every entry is answered, each
/// within its own sub-rectangle of that one frame.
#[derive(Clone)]
pub(crate) struct ImageTask {
    pub(crate) id: u64,
    pub(crate) region: (i32, i32, i32, i32),
    /// Each template with the part of the captured frame it may be found in (`None`: all of
    /// it), in frame coordinates.
    pub(crate) entries: Vec<(Arc<Decoded>, Option<Rect>)>,
    pub(crate) tol: u8,
    pub(crate) scales: Vec<f32>,
    pub(crate) mode: Mode,
    /// How the asking module's VM reads the screen (`capture_source.rs`). Part of the frame's
    /// identity in a batch: the same region read two ways is two frames. Travels with the
    /// task, so a held search sent again on re-enable reads the way it was first asked to.
    pub(crate) source: CaptureSource,
}

/// A hit in screen coordinates, with the 1-based index of the entry that made it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenHit {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub n: usize,
}

/// The worker's answer.
pub(crate) struct ImageResult {
    pub(crate) id: u64,
    /// `None` when the region could not be captured — "could not look", which is not the same
    /// answer as "looked and found nothing". Otherwise one slot per entry in `Mode::Each`, and
    /// exactly one slot, the first hit or none, in `Mode::First`.
    pub(crate) found: Option<Vec<Option<ScreenHit>>>,
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
/// (a plain `fn` pointer, hence `Send`), which reads each region through the source its
/// task carries. Exits when the task sender is dropped (app teardown). Touches only owned
/// data — safe to leave running across process exit.
///
/// FAN-OUT SHARING: after the first task, it briefly collects any others queued in the
/// same tick (several libraries' landmark polls fire together, staggered by a few ms)
/// into one batch, then CAPTURES EACH DISTINCT REGION ONCE and matches every task's
/// template against its region's shared frame. So N libraries polling the same plugin
/// region cost 1 capture per tick, not N — "one frame, many comparisons" over
/// simultaneous consumers. Landmark polling isn't latency-critical, so the tiny
/// collection wait is invisible; results are byte-for-byte what per-task capture gave.
///
/// PANICS ARE ANSWERED, NOT FATAL. This thread used to have no `catch_unwind`, so one panic —
/// in a capture, in a resize, in a scan — ended it, every later `send` failed, and every
/// callback in the session silently never fired again. Worse than an error, because nothing
/// reported it: the overlay runtime's landmark gate waits for its callback before it searches
/// again, so its overlays simply stopped appearing. Now a panicking entry is answered as "no
/// match", a panicking batch as "could not look", and the thread carries on.
pub(crate) fn spawn_image_worker(
    capture: CaptureFn,
    tasks: Receiver<ImageTask>,
    results: Sender<ImageResult>,
) {
    std::thread::spawn(move || worker_loop(capture, tasks, results));
}

fn worker_loop(
    capture: CaptureFn,
    tasks: Receiver<ImageTask>,
    results: Sender<ImageResult>,
) {
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
        let out = match logging::contain(|| run_batch(&batch, capture)) {
            Ok(out) => out,
            Err(report) => {
                contained("a search batch", "every search in it was answered nil, could not look", &report);
                // Every task is still answered. A callback that never fires is worse than a
                // wrong one: a caller that waits for it before searching again never does.
                let n = batch.len() as u32;
                batch
                    .iter()
                    .enumerate()
                    .map(|(i, t)| ImageResult {
                        id: t.id,
                        found: None,
                        capture_ms: 0,
                        match_ms: 0,
                        batch: n,
                        first_of_batch: i == 0,
                    })
                    .collect()
            }
        };
        for res in out {
            if results.send(res).is_err() {
                return; // main thread gone
            }
        }
    }
}

/// Captures each distinct frame of the batch once and answers every task from its frame.
fn run_batch(batch: &[ImageTask], capture: CaptureFn) -> Vec<ImageResult> {
    // Capture each DISTINCT region once; match every task's template against its
    // region's shared frame. The batch is small (one task per active library), so
    // a linear region lookup is fine.
    //
    // The captures stay SERIAL on purpose: each is a compositor-synchronised screen
    // read, so running them together would contend rather than overlap, and the
    // whole point of the batch is that there is only one per region anyway. The
    // MATCHING is what gets spread across cores — see below.
    //
    // A frame is a region AND the source it is read through, and the desktop
    // duplication regions of a batch go to the capture thread as one request — see
    // capture_source::capture_frames.
    let batch_len = batch.len() as u32;
    let (keys, frame_of) = capture_source::frame_keys(batch.iter().map(|t| (t.region, t.source)));
    let frames: Vec<(Option<CapturedImage>, u32)> = capture_source::capture_frames(capture, &keys);
    let plan: Vec<(&ImageTask, usize)> = batch.iter().zip(frame_of).collect();
    // MATCH IN PARALLEL. Every candidate position is independent of every other, so
    // this is the shape a thread pool is actually for; the tasks in a batch are
    // independent too. Sequentially, twelve installed libraries cost twelve full
    // scans back to back before any of them can answer — each one comfortably under
    // the logging threshold and therefore invisible, while together they were the
    // several seconds before the right overlay appeared.
    let t1 = Instant::now();
    let found: Vec<Option<Vec<Option<ScreenHit>>>> = plan
        .par_iter()
        .map(|(t, idx)| frames[*idx].0.as_ref().map(|cap| match_task(t, cap)))
        .collect();
    let match_ms = t1.elapsed().as_millis() as u32;
    let capture_ms: u32 = frames.iter().map(|f| f.1).sum();
    plan.iter()
        .zip(found)
        .enumerate()
        .map(|(i, ((t, _), found))| ImageResult {
            id: t.id,
            found,
            capture_ms,
            match_ms,
            batch: batch_len,
            first_of_batch: i == 0,
        })
        .collect()
}

/// One task against its captured frame.
fn match_task(t: &ImageTask, cap: &CapturedImage) -> Vec<Option<ScreenHit>> {
    let (rx, ry, _, _) = t.region;
    let one = |n: usize, (dec, within): &(Arc<Decoded>, Option<Rect>)| -> Option<ScreenHit> {
        // Per entry, so one template that trips something cannot take its neighbours' answers
        // with it. On whichever pool thread runs it: `contain` marks the thread it runs on.
        match logging::contain(|| template::find(cap, *within, dec, t.tol, &t.scales)) {
            Ok(hit) => hit.map(|h| ScreenHit {
                x: rx + h.x as i32,
                y: ry + h.y as i32,
                w: h.w,
                h: h.h,
                n: n + 1,
            }),
            Err(report) => {
                contained("a template match", "that template was answered as no match", &report);
                None
            }
        }
    };
    match t.mode {
        // Templates in order against this one frame; first hit wins.
        Mode::First => vec![t.entries.iter().enumerate().find_map(|(n, e)| one(n, e))],
        Mode::Each => t.entries.par_iter().enumerate().map(|(n, e)| one(n, e)).collect(),
    }
}

/// Contained panics this session, so the log gets the 1st, 2nd, 4th, 8th… rather than one
/// line per poll: a template that panics will panic every 500 ms, and the first line already
/// carries the message and the place. This throttle is the only one: the application's panic
/// hook writes nothing for a panic inside `logging::contain`, which is how every panic on the
/// worker is caught.
static CONTAINED: AtomicU64 = AtomicU64::new(0);

fn contained(what: &str, answered: &str, report: &str) {
    let n = CONTAINED.fetch_add(1, Ordering::Relaxed) + 1;
    if n.is_power_of_two() {
        logging::line(
            "image",
            &format!(
                "{what} panicked on the image worker; {answered}, and the worker carries on \
                 ({n} so far this session): {report}"
            ),
        );
    }
}

// ── Delivery, on the main thread ───────────────────────────────────────────────────────

impl Shared {
    /// Delivers finished async image-search results to their callbacks (driven by the loop
    /// tick), then drops each one-shot registry value.
    pub(crate) fn fire_image_results(&self) {
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
            //
            // 50 ms, the same bar as a slow observation. It was 25, below what a Mac pays for
            // every batch (capture ~19 ms plus match ~17), so all 3909 batches of the fifth
            // session were logged and the 57 slow ones were lost among them.
            if res.first_of_batch && res.capture_ms + res.match_ms >= 50 {
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
            let Some(mut p) = entry else { continue };
            // A result arriving is a fresh observation of the screen, and its callback
            // may re-check an overlay — so the epoch turns once here, rather than on
            // every (mostly empty) poll tick. See the note above for why once per DRAIN.
            if !bumped {
                self.bump_epoch();
                bumped = true;
            }
            // Its own statement, so both borrows end HERE. A match on the call itself would keep
            // them alive through the callback, and a callback that enables a module or loads
            // one would then panic on a borrow the drain still held.
            let verdict = fate(p.owner, p.gen, &self.enabled.borrow(), &self.vm_gens.borrow());
            match verdict {
                Fate::Deliver => {
                    if let Ok(f) = p.lua.registry_value::<Function>(&p.cb) {
                        let arg = result_value(&p.lua, p.mode, &p.names, res.found);
                        if let Err(e) = call_guarded(&f, arg) {
                            self.report_callback_error(p.scope, p.mode.binding(), &e);
                        }
                    }
                }
                Fate::Hold => {
                    // The answer itself is let go: it is searched again on re-enable.
                    p.held = true;
                    self.pending_image.borrow_mut().insert(res.id, p);
                    continue;
                }
                Fate::Drop => {}
            }
            release([p]);
        }
    }

    /// Drops every search still waiting for an answer on behalf of module `owner`'s VM, held
    /// ones included.
    ///
    /// Part of `purge_module`: a pending entry holds a clone of the VM, so leaving it would keep
    /// the old VM alive past a reload — and hand it a callback once the answer came.
    pub(crate) fn purge_pending_images(&self, owner: usize) {
        let gone = purge_owner(&mut self.pending_image.borrow_mut(), owner);
        release(gone);
    }

    /// Sends again every search that was answered while module `owner` was disabled (see
    /// `Fate::Hold`). Called by `apply_enabled` when the module is enabled.
    pub(crate) fn resume_held_images(&self, owner: usize) {
        let gen = self.vm_gens.borrow().get(&owner).copied();
        let (again, stale) = unhold(&mut self.pending_image.borrow_mut(), owner, gen);
        release(stale);
        for task in again {
            let id = task.id;
            if self.image_tasks.send(task).is_err() {
                // Worker gone (teardown): nothing will answer, so nothing may wait.
                let gone = self.pending_image.borrow_mut().remove(&id);
                release(gone);
            }
        }
    }
}

/// What the callback receives. Built in the owner's VM, from the answer and the names that
/// stayed on this side.
fn result_value(
    lua: &Lua,
    mode: Mode,
    names: &[Option<String>],
    found: Option<Vec<Option<ScreenHit>>>,
) -> Value {
    let name = |h: &ScreenHit| names.get(h.n - 1).and_then(|n| n.as_deref());
    let hit = |h: &ScreenHit| match hit_table(lua, h.x, h.y, h.w, h.h, h.n, name(h)) {
        Ok(t) => Value::Table(t),
        Err(_) => Value::Nil,
    };
    match (mode, found) {
        (_, None) => Value::Nil,
        (Mode::First, Some(v)) => v.into_iter().next().flatten().map(|h| hit(&h)).unwrap_or(Value::Nil),
        (Mode::Each, Some(v)) => {
            let Ok(list) = lua.create_table_with_capacity(v.len(), 0) else { return Value::Nil };
            for (i, slot) in v.iter().enumerate() {
                let item = match slot {
                    Some(h) => hit(h),
                    None => Value::Boolean(false),
                };
                let _ = list.raw_set(i + 1, item);
            }
            Value::Table(list)
        }
    }
}

/// `{ x, y, w, h, n, name }` — the one shape every search returns a hit in.
fn hit_table(
    lua: &Lua,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    n: usize,
    name: Option<&str>,
) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("x", x)?;
    t.set("y", y)?;
    t.set("w", w)?;
    t.set("h", h)?;
    t.set("n", n)?;
    if let Some(name) = name {
        t.set("name", name)?;
    }
    Ok(t)
}

// ── Template handles ───────────────────────────────────────────────────────────────────

/// What one module VM may hold in template handles.
///
/// Luau's collector sees a handle as a few dozen bytes of userdata, not as the pixels behind
/// it, so it has no reason to hurry: a module that built a template on every poll would grow
/// the process without the collector ever noticing. The budget is what notices. A constant
/// like `TEMPLATE_CACHE_CAP`, not a setting — no module needs anywhere near it, and a number
/// nobody has a reason to change does not belong in front of the user.
const TEMPLATE_BUDGET_PER_VM: usize = 32 << 20;

/// The running total for one Lua state, kept in its app data.
struct TemplateBudget(Rc<Cell<usize>>);

/// This VM's budget, created the first time any binding asks.
///
/// Get-or-insert, per LUA STATE — not per `install_host_api` call. A code dependency's host is
/// installed into the same state once per dependency, and a counter set up at install would
/// be replaced each time, leaving earlier handles to give their bytes back to a counter
/// nobody reads any more. The reference is dropped before this returns, so nothing holds the
/// app-data container while a collection runs finalisers.
fn budget(lua: &Lua) -> Rc<Cell<usize>> {
    if let Ok(Some(b)) = lua.try_app_data_ref::<TemplateBudget>() {
        return b.0.clone();
    }
    let b = Rc::new(Cell::new(0));
    let _ = lua.try_set_app_data(TemplateBudget(b.clone()));
    b
}

/// Bytes taken from a budget, given back when dropped — with the handle that holds them, or
/// straight away when construction fails after reserving.
struct Charge {
    bytes: usize,
    budget: Rc<Cell<usize>>,
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.budget.set(self.budget.get().saturating_sub(self.bytes));
    }
}

/// Takes `bytes` from this VM's budget, collecting garbage first if they do not fit.
///
/// TWICE, because one full collection may only finish the cycle already in progress; mlua's
/// own documentation says so. Handles dropped by the module are unreachable but not yet
/// finalised until then, and their bytes come back only when they are.
fn reserve(lua: &Lua, budget: &Rc<Cell<usize>>, bytes: usize) -> mlua::Result<Charge> {
    let fits = |b: &Rc<Cell<usize>>| b.get().saturating_add(bytes) <= TEMPLATE_BUDGET_PER_VM;
    if !fits(budget) {
        lua.gc_collect()?;
        lua.gc_collect()?;
        if !fits(budget) {
            return Err(mlua::Error::external(format!(
                "host.screen.template: this module's templates would hold {:.1} MiB; the limit \
                 is {} MiB per module VM. Build templates once, when the module loads, rather \
                 than on every poll.",
                (budget.get() + bytes) as f64 / (1 << 20) as f64,
                TEMPLATE_BUDGET_PER_VM >> 20
            )));
        }
    }
    budget.set(budget.get() + bytes);
    Ok(Charge { bytes, budget: budget.clone() })
}

/// A template in a module's hands: `host.screen.template`'s answer, and accepted by every
/// search wherever a path is.
///
/// VM-local, like any Lua value. It never leaves the main thread — searches take a clone of
/// `dec`, which is all the worker ever sees — so a handle collected while a search is in
/// flight is harmless.
pub(crate) struct TemplateHandle {
    dec: Arc<Decoded>,
    name: Option<String>,
    count: u32,
    _charge: Charge,
}

impl UserData for TemplateHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("w", |_, t| Ok(t.dec.w));
        fields.add_field_method_get("h", |_, t| Ok(t.dec.h));
        fields.add_field_method_get("name", |_, t| Ok(t.name.clone()));
        fields.add_field_method_get("count", |_, t| Ok(t.count));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(match &t.name {
                Some(n) => format!("Template({n} {}x{}, {} compared)", t.dec.w, t.dec.h, t.count),
                None => format!("Template({}x{}, {} compared)", t.dec.w, t.dec.h, t.count),
            })
        });
    }
}

/// Where a template's pixels come from.
enum Source {
    Bytes { rgba: bool, data: Vec<u8>, w: u32, h: u32 },
    Capture { x: i32, y: i32, w: i32, h: i32 },
    File(String),
}

struct ParsedSpec {
    name: Option<String>,
    source: Source,
}

/// Names longer than this are refused: a name goes into hits and log lines, and a paragraph
/// there is a mistake rather than a name.
const MAX_NAME: usize = 64;

fn spec_err(msg: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::external(format!("host.screen.template: {msg}"))
}

/// A whole number from Luau, which has no integer type of its own.
fn whole(v: &Value, field: &str) -> mlua::Result<i64> {
    match v {
        Value::Integer(i) => Ok(*i),
        Value::Number(n) if n.is_finite() && n.fract() == 0.0 => Ok(*n as i64),
        other => Err(spec_err(format!(
            "{field} must be a whole number, got {}",
            match other {
                Value::Number(n) => n.to_string(),
                v => v.type_name().to_string(),
            }
        ))),
    }
}

/// `{ region = r }` read the way every other binding reads a region, so a template's
/// `capture` and an entry's `within` default their missing edges exactly as `opts.region`
/// does rather than by a second, slightly different reading of the same form.
fn region_of(lua: &Lua, region: Table, sw: i32, sh: i32) -> mlua::Result<(i32, i32, i32, i32)> {
    let wrap = lua.create_table()?;
    wrap.set("region", region)?;
    Ok(read_region(Some(&wrap), sw, sh))
}

/// Reads and checks a `host.screen.template` spec without touching the screen or a file.
///
/// Strict, unlike the search options, and deliberately: a spec is written once, and a field
/// that is silently ignored — a misspelt `rbga`, a `tolerance` this version does not take —
/// is a template that does something other than what its author reads in it.
fn parse_spec(lua: &Lua, spec: &Table, sw: i32, sh: i32) -> mlua::Result<ParsedSpec> {
    let mut name = None;
    let (mut w, mut h) = (None, None);
    let mut sources: Vec<(String, Value)> = Vec::new();
    for pair in spec.clone().pairs::<Value, Value>() {
        let (k, v) = pair?;
        let key = match &k {
            Value::String(s) => s.to_str()?.to_string(),
            other => {
                return Err(spec_err(format!(
                    "field names must be strings, got a {}",
                    other.type_name()
                )))
            }
        };
        match key.as_str() {
            "name" => match v {
                Value::String(s) => {
                    let s = s.to_str()?.to_string();
                    if s.chars().count() > MAX_NAME {
                        return Err(spec_err(format!(
                            "name is {} characters; the limit is {MAX_NAME}",
                            s.chars().count()
                        )));
                    }
                    name = Some(s);
                }
                other => {
                    return Err(spec_err(format!("name must be a string, got a {}", other.type_name())))
                }
            },
            "w" => w = Some(whole(&v, "w")?),
            "h" => h = Some(whole(&v, "h")?),
            "rgba" | "rgb" | "capture" | "file" => sources.push((key, v)),
            other => {
                return Err(spec_err(format!(
                    "unknown field '{other}'; a spec takes name and exactly one of rgba, rgb, \
                     capture or file"
                )))
            }
        }
    }
    if sources.len() != 1 {
        let mut given: Vec<&str> = sources.iter().map(|(k, _)| k.as_str()).collect();
        given.sort_unstable();
        return Err(spec_err(if given.is_empty() {
            "give exactly one of rgba, rgb, capture or file; none was given".to_string()
        } else {
            format!("give exactly one of rgba, rgb, capture or file, not {}", given.join(" and "))
        }));
    }
    let (key, v) = sources.pop().expect("exactly one source");
    let source = match key.as_str() {
        "rgba" | "rgb" => {
            let (Some(w), Some(h)) = (w, h) else {
                return Err(spec_err(format!("{key} needs w and h")));
            };
            let side = |n: i64, f: &str| -> mlua::Result<u32> {
                if (1..=template::MAX_SIDE as i64).contains(&n) {
                    Ok(n as u32)
                } else {
                    Err(spec_err(format!("{f} is {n}; it must be from 1 to {}", template::MAX_SIDE)))
                }
            };
            let (w, h) = (side(w, "w")?, side(h, "h")?);
            let data = match v {
                Value::String(s) => s.as_bytes().to_vec(),
                Value::Buffer(b) => b.to_vec(),
                other => {
                    return Err(spec_err(format!(
                        "{key} must be a string or a buffer of bytes, got a {}",
                        other.type_name()
                    )))
                }
            };
            Source::Bytes { rgba: key == "rgba", data, w, h }
        }
        "capture" => {
            if w.is_some() || h.is_some() {
                return Err(spec_err("w and h belong with rgba or rgb; a capture takes its size from its region"));
            }
            let Value::Table(r) = v else {
                return Err(spec_err(format!("capture must be a region, got a {}", v.type_name())));
            };
            let (x, y, rw, rh) = region_of(lua, r, sw, sh)?;
            // Checked BEFORE capturing: the limit is what the capture would produce, and
            // capturing first would spend a compositor frame to be told no.
            template::check_size(rw.max(0) as u32, rh.max(0) as u32).map_err(spec_err)?;
            Source::Capture { x, y, w: rw, h: rh }
        }
        _ => {
            if w.is_some() || h.is_some() {
                return Err(spec_err("w and h belong with rgba or rgb; a file has its own size"));
            }
            let Value::String(p) = v else {
                return Err(spec_err(format!("file must be a path, got a {}", v.type_name())));
            };
            Source::File(p.to_str()?.to_string())
        }
    };
    Ok(ParsedSpec { name, source })
}

/// Turns a checked spec into a handle, with the file loader and the screen capture passed in,
/// so every rule here is testable without a backend.
///
/// `nil` only for a failed capture: that is a runtime condition the module has to handle,
/// while every other refusal is a mistake in the code that wrote the spec, and raises.
fn build_handle(
    lua: &Lua,
    spec: ParsedSpec,
    load: impl FnOnce(&str) -> mlua::Result<Arc<Decoded>>,
    capture: impl FnOnce(i32, i32, i32, i32) -> Option<CapturedImage>,
) -> mlua::Result<Value> {
    let budget = budget(lua);
    let mut name = spec.name;
    let (dec, charge) = match spec.source {
        // The cached decode itself, shared: the bytes are already held by the path cache, so
        // the handle costs this VM nothing. It is therefore a SNAPSHOT — a re-captured PNG is
        // picked up live only by a search that passes the path. Unnamed, it is named by its
        // path as written, which is what a search given the path would report.
        Source::File(path) => {
            let dec = load(&path)?;
            name = name.or(Some(path));
            (dec, Charge { bytes: 0, budget: budget.clone() })
        }
        Source::Bytes { rgba, data, w, h } => {
            // The size limits first: a 4096x4096 spec must be told it is too large, not that
            // it would overrun the budget.
            template::check_size(w, h).map_err(spec_err)?;
            let charge = reserve(lua, &budget, template::dense_bytes(w, h))?;
            let dec = if rgba { Decoded::from_rgba(w, h, data) } else { Decoded::from_rgb(w, h, &data) }
                .map_err(spec_err)?;
            (Arc::new(dec), charge)
        }
        Source::Capture { x, y, w, h } => {
            let charge = reserve(lua, &budget, template::dense_bytes(w as u32, h as u32))?;
            let Some(cap) = capture(x, y, w, h) else { return Ok(Value::Nil) };
            match Decoded::from_capture(cap) {
                Ok(d) => (Arc::new(d), charge),
                // A capture that came back short is a capture that failed.
                Err(_) => return Ok(Value::Nil),
            }
        }
    };
    let count = dec.count();
    let handle = TemplateHandle { dec, name, count, _charge: charge };
    Ok(Value::UserData(lua.create_userdata(handle)?))
}

/// `host.screen.template(spec) -> Template | nil`.
pub(crate) fn template(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, spec: Table| {
        let (sw, shh) = sh.backend.screen_size();
        let parsed = parse_spec(lua, &spec, sw, shh)?;
        build_handle(
            lua,
            parsed,
            |path| sh.load_template(&sh.root(idx).join(path)),
            // A live read of the screen, every time, and counted like `pixel` and `profile`:
            // it is a compositor frame on the event loop. Never to be served from a frame
            // cache, now or once one exists — a template learned from a stale frame is a
            // template of something that is no longer there (see
            // docs/screen-frame-sharing-design.md on calibration paths).
            //
            // Through the module's own source, like `host.screen.save` and every other read.
            // Desktop duplication answers with the most recently composed frame, which is still
            // a read made now — but only once it has opened: a capture made before that (at
            // load, where nothing opens it, or while a window trigger's prewarm still is) is
            // read the standard way, or is nil under fallback = "none", and the template keeps
            // that picture for the session. docs/api/screen.md says so; TODO.md holds the
            // question of changing it. The source first, outside the timing, as `pixel` does:
            // in a module that reads through duplication, the first read also compares the two
            // sources, once.
            |x, y, w, h| {
                let src = capture_source::read_source(lua, &*sh.backend, (x, y, w, h));
                let t0 = Instant::now();
                let cap = sh.backend.capture(x, y, w, h, src);
                let mut obs = sh.observations();
                obs.pixels += 1;
                obs.pixel_us += t0.elapsed().as_micros();
                cap
            },
        )
    })
}

// ── Template arguments ─────────────────────────────────────────────────────────────────

/// One template argument — a path or a handle — as its pixels and the name a hit reports.
///
/// A path's name is the string exactly as the caller wrote it, so a module can tell its hits
/// apart without keeping a second list.
fn resolve_with(
    v: &Value,
    what: &str,
    load: &dyn Fn(&str) -> mlua::Result<Arc<Decoded>>,
) -> mlua::Result<(Arc<Decoded>, Option<String>)> {
    match v {
        Value::String(s) => {
            let path = s.to_str()?.to_string();
            Ok((load(&path)?, Some(path)))
        }
        Value::UserData(ud) => match ud.borrow::<TemplateHandle>() {
            Ok(h) => Ok((h.dec.clone(), h.name.clone())),
            Err(_) => Err(mlua::Error::external(format!(
                "{what}: expected a path or a Template, got a userdata of another kind"
            ))),
        },
        other => Err(mlua::Error::external(format!(
            "{what}: expected a path or a Template, got a {}",
            other.type_name()
        ))),
    }
}

/// Every template in a list, resolved BEFORE anything is captured.
///
/// `imageSearchMulti` used to open each file inside its loop, after the capture, so a bad
/// path late in the list lay dormant until every earlier template happened to miss. Now a bad
/// path raises on the first call, wherever it sits.
fn resolve_list(
    list: &Table,
    what: &str,
    load: &dyn Fn(&str) -> mlua::Result<Arc<Decoded>>,
) -> mlua::Result<Vec<(Arc<Decoded>, Option<String>)>> {
    let mut out = Vec::new();
    for v in list.clone().sequence_values::<Value>() {
        out.push(resolve_with(&v?, what, load)?);
    }
    Ok(out)
}

/// The call-level `tolerance`, read as it always has been.
///
/// Known and left alone here: a value that does not fit a byte — 300, or -1 — becomes 0, an
/// EXACT match, without a word. Changing that changes what existing modules get, so it is a
/// separate step (TODO.md), and every search reads it through this one function so the step
/// is one edit.
fn read_tol(opts: Option<&Table>) -> u8 {
    opts.and_then(|o| o.get::<u8>("tolerance").ok()).unwrap_or(0)
}

/// The part of the captured `region` that `within` covers, in the frame's own coordinates —
/// empty when they do not meet, which a search answers with no match.
fn within_frame(region: (i32, i32, i32, i32), within: (i32, i32, i32, i32)) -> Rect {
    let (rx, ry, rw, rh) = region;
    let (wx, wy, ww, wh) = within;
    let x1 = wx.max(rx);
    let y1 = wy.max(ry);
    let x2 = wx.saturating_add(ww).min(rx.saturating_add(rw));
    let y2 = wy.saturating_add(wh).min(ry.saturating_add(rh));
    if x2 <= x1 || y2 <= y1 {
        return Rect { x: 0, y: 0, w: 0, h: 0 };
    }
    Rect { x: (x1 - rx) as u32, y: (y1 - ry) as u32, w: (x2 - x1) as u32, h: (y2 - y1) as u32 }
}

// ── The bindings ───────────────────────────────────────────────────────────────────────

/// `host.screen.imageSearch(template, opts?) -> Hit | nil`, synchronous.
pub(crate) fn image_search(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (tv, opts): (Value, Option<Table>)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let (tmpl, name) = resolve_with(&tv, "imageSearch", &load)?;
        let (sw, sh_) = sh.backend.screen_size();
        let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
        let src = capture_source::read_source(lua, &*sh.backend, (rx, ry, rw, rh));
        let Some(cap) = sh.backend.capture(rx, ry, rw, rh, src) else { return Ok(Value::Nil) };
        let tol = read_tol(opts.as_ref());
        let scales = read_scales(opts.as_ref());
        match template::find(&cap, None, &tmpl, tol, &scales) {
            Some(h) => Ok(Value::Table(hit_table(
                lua,
                rx + h.x as i32,
                ry + h.y as i32,
                h.w,
                h.h,
                1,
                name.as_deref(),
            )?)),
            None => Ok(Value::Nil),
        }
    })
}

/// `host.screen.imageSearchMulti(templates, opts?) -> (n, Hit) | (nil, nil)`.
///
/// Captures the region ONCE and tries each template in order, returning the 1-based index of
/// the first match plus its hit. One capture serves many comparisons (e.g. a toggle's on/off
/// pair) — half the ~1-frame screen touches of two imageSearch calls, and both templates are
/// matched against the SAME frame, so a state change mid-repaint can't fall between two
/// separate captures. Sync, like imageSearch (AHK-style). Paths share the decode cache.
pub(crate) fn image_search_multi(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (templates, opts): (Table, Option<Table>)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let list = resolve_list(&templates, "imageSearchMulti", &load)?;
        let (sw, sh_) = sh.backend.screen_size();
        let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
        let src = capture_source::read_source(lua, &*sh.backend, (rx, ry, rw, rh));
        let Some(cap) = sh.backend.capture(rx, ry, rw, rh, src) else {
            return Ok((Value::Nil, Value::Nil));
        };
        let tol = read_tol(opts.as_ref());
        let scales = read_scales(opts.as_ref());
        for (i, (tmpl, name)) in list.iter().enumerate() {
            if let Some(h) = template::find(&cap, None, tmpl, tol, &scales) {
                let t = hit_table(lua, rx + h.x as i32, ry + h.y as i32, h.w, h.h, i + 1, name.as_deref())?;
                return Ok((Value::Integer(i as i64 + 1), Value::Table(t)));
            }
        }
        Ok((Value::Nil, Value::Nil))
    })
}

/// Queues a task for the worker, with its callback waiting on this side.
#[allow(clippy::too_many_arguments)]
fn enqueue(
    sh: &Shared,
    lua: &Lua,
    scope: usize,
    cb: Function,
    region: (i32, i32, i32, i32),
    entries: Vec<(Arc<Decoded>, Option<Rect>)>,
    names: Vec<Option<String>>,
    opts: Option<&Table>,
    mode: Mode,
) -> mlua::Result<()> {
    let tol = read_tol(opts);
    let scales = read_scales(opts);
    // Which picture the worker reads for this VM, decided here on the main thread: the worker
    // never sees a Lua state. The one place both async searches set it.
    let source = capture_source::read_source(lua, &*sh.backend, region);
    // The worker captures the region itself and ALWAYS posts a result (a failed capture
    // or a contained panic becomes an answer on the tick), so the pending callback is
    // always drained.
    let id = sh.next_image_id.get() + 1;
    sh.next_image_id.set(id);
    let task = ImageTask { id, region, entries, tol, scales, mode, source };
    let entry = pending(lua, &sh.vm_gens.borrow(), scope, cb, names, task.clone())?;
    sh.pending_image.borrow_mut().insert(id, entry);
    if sh.image_tasks.send(task).is_err() {
        // Worker thread gone (it contains its own panics, so this means teardown): drop the
        // pending entry we just inserted so it can't leak (its registry value is freed with
        // it), instead of a callback that never fires.
        let gone = sh.pending_image.borrow_mut().remove(&id);
        release(gone);
    }
    Ok(())
}

/// `host.screen.imageSearchAsync(template | {template, …}, opts?, cb)` — offloads BOTH the
/// region capture (the ~1-frame DWM-compositor cost) and the template match to the worker,
/// calling cb(Hit) | cb(nil) on a later tick, so a detection poll (e.g. the 500 ms landmark
/// poll) never blocks the event loop on either. Only path resolution happens here, and it is
/// cached.
///
/// Given a LIST, the templates are tried in order against the SAME captured frame and the
/// first hit wins, with `n` reporting which one matched — for a thing with several
/// renderings (a dialog's close glyph as two plugin versions draw it). Chaining separate
/// searches instead costs a fresh capture per template.
pub(crate) fn image_search_async(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (tv, opts, cb): (Value, Option<Table>, Function)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let list = match &tv {
            Value::Table(list) => resolve_list(list, "imageSearchAsync", &load)?,
            other => vec![resolve_with(other, "imageSearchAsync", &load)?],
        };
        if list.is_empty() {
            return Err(mlua::Error::external("imageSearchAsync: no template given".to_string()));
        }
        let (sw, sh_) = sh.backend.screen_size();
        let region = read_region(opts.as_ref(), sw, sh_);
        let (entries, names) = list.into_iter().map(|(d, n)| ((d, None), n)).unzip();
        enqueue(&sh, lua, idx, cb, region, entries, names, opts.as_ref(), Mode::First)
    })
}

/// `host.screen.imageSearchEach(entries, opts?, cb)` — ONE capture, and an answer for every
/// entry: `cb(list)` with a hit or `false` in each slot, or `cb(nil)` when the region could
/// not be captured.
///
/// For "which of these states is showing": a menu with a cursor that can be in one of several
/// places, a panel that shows one of several pages. `imageSearchAsync` with a list answers
/// only the first match; separate calls share a frame only when they happen to land in the
/// worker's 5 ms batch window, which is an implementation detail. Here one frame is the
/// contract, and each entry may look only inside its own `within` part of it — a state's own
/// region, so a template is neither slower nor more likely to invent a hit elsewhere.
pub(crate) fn image_search_each(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (list, opts, cb): (Table, Option<Table>, Function)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let (sw, sh_) = sh.backend.screen_size();
        let region = read_region(opts.as_ref(), sw, sh_);
        let mut entries = Vec::new();
        let mut names = Vec::new();
        for (i, v) in list.clone().sequence_values::<Value>().enumerate() {
            let v = v?;
            match v {
                Value::Table(e) => {
                    let what = format!("imageSearchEach: entry {}", i + 1);
                    let tv: Value = e.get("template")?;
                    if tv.is_nil() {
                        return Err(mlua::Error::external(format!(
                            "{what} is a table without a template"
                        )));
                    }
                    let (dec, own) = resolve_with(&tv, &what, &load)?;
                    let within = match e.get::<Value>("within")? {
                        Value::Nil => None,
                        Value::Table(r) => Some(within_frame(region, region_of(lua, r, sw, sh_)?)),
                        other => {
                            return Err(mlua::Error::external(format!(
                                "{what}: within must be a region, got a {}",
                                other.type_name()
                            )))
                        }
                    };
                    let name = match e.get::<Value>("name")? {
                        Value::Nil => own,
                        Value::String(s) => Some(s.to_str()?.to_string()),
                        other => {
                            return Err(mlua::Error::external(format!(
                                "{what}: name must be a string, got a {}",
                                other.type_name()
                            )))
                        }
                    };
                    entries.push((dec, within));
                    names.push(name);
                }
                other => {
                    let (dec, name) =
                        resolve_with(&other, &format!("imageSearchEach: entry {}", i + 1), &load)?;
                    entries.push((dec, None));
                    names.push(name);
                }
            }
        }
        if entries.is_empty() {
            return Err(mlua::Error::external("imageSearchEach: no template given".to_string()));
        }
        enqueue(&sh, lua, idx, cb, region, entries, names, opts.as_ref(), Mode::Each)
    })
}

/// `host.screen.imageSearchAll(template, opts?) -> { Hit, … }` — EVERY match of the template
/// in the region, not just the first.
///
/// For deciding whether a template is safe to click blindly. A close-glyph template that
/// matches twice will eventually click the wrong one, and a template that matches nowhere is
/// a control that silently never fires; "matched exactly once, here" is the answer you want
/// before shipping either.
pub(crate) fn image_search_all(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (tv, opts): (Value, Option<Table>)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let (tmpl, name) = resolve_with(&tv, "imageSearchAll", &load)?;
        let (sw, sh_) = sh.backend.screen_size();
        let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
        let tol = read_tol(opts.as_ref());
        let out = lua.create_table()?;
        let src = capture_source::read_source(lua, &*sh.backend, (rx, ry, rw, rh));
        let Some(cap) = sh.backend.capture(rx, ry, rw, rh, src) else { return Ok(out) };
        for (i, (x, y)) in template::find_all(&cap, &tmpl, tol).into_iter().enumerate() {
            let t = hit_table(lua, rx + x as i32, ry + y as i32, tmpl.w, tmpl.h, 1, name.as_deref())?;
            out.raw_set(i + 1, t)?;
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn opaque(w: u32, h: u32, rgb: [u8; 3]) -> CapturedImage {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        CapturedImage { w, h, rgba }
    }

    fn no_file(p: &str) -> mlua::Result<Arc<Decoded>> {
        Err(mlua::Error::external(format!("no file {p} in a test")))
    }

    fn spec(lua: &Lua, src: &str) -> mlua::Result<ParsedSpec> {
        let t: Table = lua.load(src).eval()?;
        parse_spec(lua, &t, 1920, 1080)
    }

    fn err_text<T>(r: mlua::Result<T>) -> String {
        match r {
            Ok(_) => panic!("expected an error"),
            Err(e) => e.to_string(),
        }
    }

    // ── spec parsing ───────────────────────────────────────────────────────────────────

    #[test]
    fn a_spec_names_its_mistakes() {
        let lua = Lua::new();
        let e = err_text(spec(&lua, "return { rgba = string.rep('\\0', 1436), w = 10, h = 36 }")
            .and_then(|s| build_handle(&lua, s, no_file, |_, _, _, _| None)));
        assert!(e.contains("host.screen.template") && e.contains("1436") && e.contains("1440"), "{e}");

        let e = err_text(spec(&lua, "return { rgba = 'x', rgb = 'y', w = 1, h = 1 }"));
        assert!(e.contains("rgb and rgba"), "{e}");
        let e = err_text(spec(&lua, "return { name = 'x' }"));
        assert!(e.contains("none was given"), "{e}");
        let e = err_text(spec(&lua, "return { rbga = 'x', w = 1, h = 1 }"));
        assert!(e.contains("unknown field 'rbga'"), "{e}");
        // Fields of the design's later steps are refused, not silently ignored.
        let e = err_text(spec(&lua, "return { file = 'a.png', tolerance = 8 }"));
        assert!(e.contains("unknown field 'tolerance'"), "{e}");
        let e = err_text(spec(&lua, "return { rgba = '', w = 1.5, h = 1 }"));
        assert!(e.contains("w must be a whole number"), "{e}");
        let e = err_text(spec(&lua, "return { rgba = '', w = 0, h = 1 }"));
        assert!(e.contains("w is 0"), "{e}");
        let e = err_text(spec(&lua, "return { rgb = 'abc' }"));
        assert!(e.contains("rgb needs w and h"), "{e}");
        let e = err_text(spec(&lua, "return { file = 'a.png', w = 3 }"));
        assert!(e.contains("belong with rgba or rgb"), "{e}");
        let e = err_text(spec(&lua, "return { rgba = 5, w = 1, h = 1 }"));
        assert!(e.contains("string or a buffer"), "{e}");
        let e = err_text(spec(&lua, &format!("return {{ file = 'a.png', name = '{}' }}", "n".repeat(65))));
        assert!(e.contains("limit is 64"), "{e}");
        // A capture larger than the limit is refused before anything is captured.
        let e = err_text(spec(&lua, "return { capture = { 0, 0, 5000, 10 } }"));
        assert!(e.contains("too large"), "{e}");
        let e = err_text(spec(&lua, "return { capture = { 10, 10, 10, 40 } }"));
        assert!(e.contains("empty"), "{e}");
    }

    #[test]
    fn strings_and_buffers_both_carry_bytes() {
        let lua = Lua::new();
        for src in [
            "return { rgb = '\\1\\2\\3\\4\\5\\6', w = 2, h = 1, name = 'pair' }",
            "local b = buffer.create(6) for i = 0, 5 do buffer.writeu8(b, i, i + 1) end \
             return { rgb = b, w = 2, h = 1, name = 'pair' }",
        ] {
            let s = spec(&lua, src).unwrap();
            let v = build_handle(&lua, s, no_file, |_, _, _, _| None).unwrap();
            lua.globals().set("t", v).unwrap();
            let (w, h, name, count, text): (u32, u32, String, u32, String) = lua
                .load("return t.w, t.h, t.name, t.count, tostring(t)")
                .eval()
                .unwrap();
            assert_eq!((w, h, name.as_str(), count), (2, 1, "pair", 2));
            assert_eq!(text, "Template(pair 2x1, 2 compared)");
        }
    }

    #[test]
    fn a_failed_capture_is_nil_and_a_good_one_is_opaque() {
        let lua = Lua::new();
        let s = spec(&lua, "return { capture = { 10, 20, 14, 23 } }").unwrap();
        let v = build_handle(&lua, s, no_file, |_, _, _, _| None).unwrap();
        assert!(v.is_nil(), "a failed capture answers nil");
        assert_eq!(budget(&lua).get(), 0, "and gives its reservation back");

        let s = spec(&lua, "return { capture = { 10, 20, 14, 23 }, name = 'here' }").unwrap();
        let v = build_handle(&lua, s, no_file, |x, y, w, h| {
            assert_eq!((x, y, w, h), (10, 20, 4, 3), "the region, read like every other");
            let mut c = opaque(4, 3, [9, 9, 9]);
            for px in c.rgba.chunks_exact_mut(4) {
                px[3] = 0; // what an unforced GDI capture could have delivered
            }
            Some(c)
        })
        .unwrap();
        let Value::UserData(ud) = v else { panic!("expected a handle") };
        let h = ud.borrow::<TemplateHandle>().unwrap();
        assert_eq!((h.dec.w, h.dec.h, h.count), (4, 3, 12), "every captured pixel is compared");
    }

    #[test]
    fn a_file_handle_shares_the_cached_decode_and_costs_nothing() {
        let lua = Lua::new();
        let cached = Arc::new(Decoded::from_png_rgba(3, 3, vec![5; 36]));
        let c2 = cached.clone();
        let s = spec(&lua, "return { file = 'images/x.png' }").unwrap();
        let v = build_handle(&lua, s, move |p| {
            assert_eq!(p, "images/x.png");
            Ok(c2)
        }, |_, _, _, _| None)
        .unwrap();
        let Value::UserData(ud) = v else { panic!("expected a handle") };
        let h = ud.borrow::<TemplateHandle>().unwrap();
        assert!(Arc::ptr_eq(&h.dec, &cached));
        assert_eq!(h.name.as_deref(), Some("images/x.png"), "unnamed, it is named by its path");
        assert_eq!(budget(&lua).get(), 0);
    }

    /// A spec that is too large is told so, before the budget is consulted.
    #[test]
    fn size_limits_come_before_the_budget() {
        let lua = Lua::new();
        let s = spec(&lua, "return { rgba = '', w = 4096, h = 4096 }").unwrap();
        let e = err_text(build_handle(&lua, s, no_file, |_, _, _, _| None));
        assert!(e.contains("16777216 pixels") && e.contains("1048576"), "{e}");
        assert_eq!(budget(&lua).get(), 0);
    }

    // ── the budget ─────────────────────────────────────────────────────────────────────

    /// Critique issue 9: a code dependency's host is installed into the same Lua state once
    /// per dependency. Two bindings made separately in one state must charge ONE counter.
    #[test]
    fn two_installs_in_one_state_share_one_budget() {
        let lua = Lua::new();
        assert!(Rc::ptr_eq(&budget(&lua), &budget(&lua)));
        let make = |lua: &Lua| {
            lua.create_function(|lua, t: Table| {
                let s = parse_spec(lua, &t, 100, 100)?;
                build_handle(lua, s, no_file, |_, _, _, _| None)
            })
            .unwrap()
        };
        lua.globals().set("a", make(&lua)).unwrap();
        lua.globals().set("b", make(&lua)).unwrap();
        lua.load(
            "keep1 = a({ rgb = string.rep('\\0', 300), w = 10, h = 10 }) \
             keep2 = b({ rgb = string.rep('\\0', 300), w = 10, h = 10 })",
        )
        .exec()
        .unwrap();
        assert_eq!(budget(&lua).get(), 2 * template::dense_bytes(10, 10));
        lua.load("keep1 = nil keep2 = nil").exec().unwrap();
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        assert_eq!(budget(&lua).get(), 0, "both handles gave their bytes back");
    }

    #[test]
    fn the_budget_collects_before_it_refuses() {
        let lua = Lua::new();
        let f = lua
            .create_function(|lua, t: Table| {
                let s = parse_spec(lua, &t, 100, 100)?;
                build_handle(lua, s, no_file, |_, _, _, _| None)
            })
            .unwrap();
        lua.globals().set("make", f).unwrap();
        // 4 MiB each, the most one handle may be. Forty of them dropped at once add up to five
        // times the budget, which only a collection can make room for.
        lua.load(
            "local bytes = string.rep('\\0', 1024 * 1024 * 4) \
             for i = 1, 40 do local t = make({ rgba = bytes, w = 1024, h = 1024 }) end",
        )
        .exec()
        .unwrap();
        // Holding them is refused, and the message says what to do.
        let e = lua
            .load(
                "local bytes = string.rep('\\0', 1024 * 1024 * 4) held = {} \
                 for i = 1, 40 do held[i] = make({ rgba = bytes, w = 1024, h = 1024 }) end",
            )
            .exec()
            .unwrap_err()
            .to_string();
        assert!(e.contains("32 MiB") && e.contains("once"), "{e}");
    }

    // ── owners, generations and the purge ──────────────────────────────────────────────

    #[test]
    fn only_the_live_owner_is_answered_and_a_disabled_one_is_held() {
        let enabled = vec![true, false, true];
        let gens: HashMap<usize, u64> = [(0, 7), (1, 8), (2, 9)].into_iter().collect();
        assert_eq!(fate(0, 7, &enabled, &gens), Fate::Deliver);
        assert_eq!(fate(1, 8, &enabled, &gens), Fate::Hold, "a disabled owner is held, not dropped");
        assert_eq!(fate(2, 3, &enabled, &gens), Fate::Drop, "an older incarnation of an enabled owner");
        assert_eq!(fate(1, 3, &enabled, &gens), Fate::Drop, "an older incarnation of a disabled owner");
        assert_eq!(fate(5, 1, &enabled, &gens), Fate::Drop, "an index nobody holds");
        // Rolled back after a failed hot-load: the generation is still recorded, the index is not.
        let rolled: HashMap<usize, u64> = [(3, 4)].into_iter().collect();
        assert_eq!(fate(3, 4, &enabled, &rolled), Fate::Drop);
    }

    fn entry(lua: &Lua, gens: &HashMap<usize, u64>, id: u64, scope: usize) -> PendingImage {
        let cb: Function = lua.load("return function() end").eval().unwrap();
        pending(lua, gens, scope, cb, vec![None], task(id, (0, 0, 10, 10), vec![(dot([1, 2, 3]), None)], Mode::First))
            .unwrap()
    }

    /// The bug this fixes: a search made by a code dependency's code (scope = the dependency)
    /// inside a dependent's VM belongs to the DEPENDENT. Disabling the dependency must not
    /// drop it; disabling the dependent holds it for that dependent.
    #[test]
    fn a_dependency_search_belongs_to_the_vm_that_ran_it() {
        let lua = Lua::new();
        let _ = lua.try_set_app_data(VmOwner { idx: 4, gen: 11 });
        // The dependency's generation is recorded too; it must not be the one taken.
        let gens: HashMap<usize, u64> = [(1, 2), (4, 11)].into_iter().collect();
        let p = entry(&lua, &gens, 1, 1);
        assert_eq!((p.scope, p.owner, p.gen), (1, 4, 11), "scope 1, owned by the VM of module 4");
        // Dependency (idx 1) disabled, dependent (idx 4) enabled: delivered.
        let enabled = vec![true, false, true, true, true];
        assert_eq!(fate(p.owner, p.gen, &enabled, &gens), Fate::Deliver);
        let mut off = enabled.clone();
        off[4] = false;
        assert_eq!(fate(p.owner, p.gen, &off, &gens), Fate::Hold);
        release([p]);
    }

    /// A state nobody tagged answers to the binding's own index at its current generation; one
    /// whose index has no generation either is never answered.
    #[test]
    fn an_untagged_state_falls_back_to_its_scope() {
        let lua = Lua::new();
        let gens: HashMap<usize, u64> = [(1, 5)].into_iter().collect();
        assert_eq!(owner_of(&lua, &gens, 1), (1, 5));
        assert_eq!(owner_of(&lua, &gens, 2), (2, 0));
        assert_eq!(fate(2, 0, &[true, true, true], &gens), Fate::Drop);
    }

    #[test]
    fn a_purge_takes_only_that_owners_entries() {
        let lua = Lua::new();
        let mut map: HashMap<u64, PendingImage> = HashMap::new();
        for (id, owner) in [(1u64, 0usize), (2, 3), (3, 3), (4, 5)] {
            let mut p = entry(&lua, &HashMap::new(), id, 9);
            p.owner = owner;
            map.insert(id, p);
        }
        map.get_mut(&3).unwrap().held = true;
        let gone = purge_owner(&mut map, 3);
        let mut ids: Vec<u64> = gone.iter().map(|p| p.task.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![2, 3], "held entries go too");
        release(gone);
        let mut left: Vec<u64> = map.keys().copied().collect();
        left.sort_unstable();
        assert_eq!(left, vec![1, 4], "the scope (9) is not what a purge matches on");
        assert!(purge_owner(&mut map, 9).is_empty());
    }

    /// Disable while a search is in flight, then enable: the search is sent again under the
    /// same id, so the callback still comes — the landmark gate's wedge.
    #[test]
    fn a_held_search_is_sent_again_on_enable() {
        let lua = Lua::new();
        let _ = lua.try_set_app_data(VmOwner { idx: 2, gen: 6 });
        let gens: HashMap<usize, u64> = [(2, 6)].into_iter().collect();
        let mut map: HashMap<u64, PendingImage> = HashMap::new();
        for id in [10u64, 11, 12] {
            map.insert(id, entry(&lua, &gens, id, 0));
        }
        map.get_mut(&10).unwrap().held = true;
        // It was asked through desktop duplication: sent again, it reads the same way, not the
        // default the task type would give a rebuilt task.
        map.get_mut(&10).unwrap().task.source = CaptureSource::Duplication { or_standard: false };
        map.get_mut(&11).unwrap().held = true;
        // 11 was held for an older VM at the same index.
        map.get_mut(&11).unwrap().gen = 5;
        // 12 is still in flight: its answer is on its way and must not be asked for twice.
        let (again, stale) = unhold(&mut map, 2, gens.get(&2).copied());
        assert_eq!(again.iter().map(|t| t.id).collect::<Vec<_>>(), vec![10]);
        assert_eq!(again[0].region, (0, 0, 10, 10), "the same search");
        assert_eq!(
            again[0].source,
            CaptureSource::Duplication { or_standard: false },
            "read the way it was first asked to"
        );
        assert_eq!(stale.iter().map(|p| p.task.id).collect::<Vec<_>>(), vec![11]);
        release(stale);
        assert!(!map[&10].held, "in flight again, so its answer is delivered");
        assert!(map.contains_key(&12) && !map.contains_key(&11));
        // A second enable finds nothing more to send.
        assert!(unhold(&mut map, 2, Some(6)).0.is_empty());
        release(map.into_values());
    }

    // ── the worker ─────────────────────────────────────────────────────────────────────

    fn task(id: u64, region: (i32, i32, i32, i32), entries: Vec<(Arc<Decoded>, Option<Rect>)>, mode: Mode) -> ImageTask {
        ImageTask { id, region, entries, tol: 0, scales: Vec::new(), mode, source: CaptureSource::Standard }
    }

    fn dot(rgb: [u8; 3]) -> Arc<Decoded> {
        Arc::new(Decoded::from_png_rgba(1, 1, vec![rgb[0], rgb[1], rgb[2], 255]))
    }

    /// A frame of grey with one red and one green pixel, wherever it is asked for.
    fn two_dots_at(w: i32, h: i32) -> CapturedImage {
        let mut c = opaque(w as u32, h as u32, [50, 50, 50]);
        c.rgba[(2 * w as usize + 3) * 4..][..3].copy_from_slice(&[255, 0, 0]);
        c.rgba[(5 * w as usize + 8) * 4..][..3].copy_from_slice(&[0, 255, 0]);
        c
    }

    /// `two_dots_at` for every region asked, whichever source: the worker's capture routine.
    fn two_dots(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Option<CapturedImage>> {
        regions.iter().map(|&(_, _, w, h)| Some(two_dots_at(w, h))).collect()
    }

    #[test]
    fn each_answers_every_entry_from_one_frame() {
        let entries = vec![
            (dot([255, 0, 0]), None),
            (dot([0, 0, 255]), None),
            // Green is at frame (8, 5); a `within` that excludes it answers false.
            (dot([0, 255, 0]), Some(Rect { x: 0, y: 0, w: 8, h: 10 })),
            (dot([0, 255, 0]), Some(within_frame((100, 200, 10, 10), (106, 203, 200, 200)))),
        ];
        let out = run_batch(&[task(1, (100, 200, 10, 10), entries, Mode::Each)], two_dots);
        let found = out[0].found.clone().expect("captured");
        assert_eq!(found[0], Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 }));
        assert_eq!(found[1], None);
        assert_eq!(found[2], None, "outside its within");
        assert_eq!(found[3], Some(ScreenHit { x: 108, y: 205, w: 1, h: 1, n: 4 }));
    }

    #[test]
    fn first_answers_the_first_entry_that_matches() {
        let entries = vec![(dot([0, 0, 255]), None), (dot([0, 255, 0]), None), (dot([255, 0, 0]), None)];
        let out = run_batch(&[task(1, (0, 0, 10, 10), entries, Mode::First)], two_dots);
        assert_eq!(out[0].found, Some(vec![Some(ScreenHit { x: 8, y: 5, w: 1, h: 1, n: 2 })]));
    }

    #[test]
    fn a_failed_capture_is_none_not_a_list_of_misses() {
        let entries = vec![(dot([255, 0, 0]), None)];
        let out = run_batch(&[task(1, (0, 0, 10, 10), entries, Mode::Each)], |r, _| r.iter().map(|_| None).collect());
        assert!(out[0].found.is_none());
        let lua = Lua::new();
        assert!(result_value(&lua, Mode::Each, &[None], None).is_nil());
        // ...while a capture that worked but matched nothing is a list of `false`.
        let v = result_value(&lua, Mode::Each, &[None, None], Some(vec![None, None]));
        let Value::Table(t) = v else { panic!("expected a list") };
        assert_eq!(t.raw_len(), 2);
        assert_eq!(t.raw_get::<Value>(1).unwrap(), Value::Boolean(false));
    }

    #[test]
    fn hits_carry_their_names() {
        let lua = Lua::new();
        let names = vec![Some("menu".to_string()), None];
        let hit = ScreenHit { x: 1, y: 2, w: 3, h: 4, n: 1 };
        let Value::Table(t) = result_value(&lua, Mode::First, &names, Some(vec![Some(hit)])) else {
            panic!("expected a hit")
        };
        assert_eq!(t.get::<String>("name").unwrap(), "menu");
        assert_eq!(t.get::<u32>("n").unwrap(), 1);
        let Value::Table(list) =
            result_value(&lua, Mode::Each, &names, Some(vec![None, Some(ScreenHit { n: 2, ..hit })]))
        else {
            panic!("expected a list")
        };
        let second: Table = list.raw_get(2).unwrap();
        assert!(second.get::<Value>("name").unwrap().is_nil(), "an unnamed handle has no name");
    }

    fn panics_at_666(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Option<CapturedImage>> {
        if regions.iter().any(|r| r.0 == 666) {
            panic!("capture exploded");
        }
        two_dots(regions, src)
    }

    /// Two sources, told apart by what they "see": the standard one the two dots, duplication a
    /// frame of plain grey, and a duplication read that could not answer under
    /// `fallback = "none"` nothing at all.
    fn by_source(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Option<CapturedImage>> {
        regions
            .iter()
            .map(|&(_, _, w, h)| match src {
                CaptureSource::Standard => Some(two_dots_at(w, h)),
                CaptureSource::Duplication { or_standard: true } => Some(opaque(w as u32, h as u32, [50, 50, 50])),
                CaptureSource::Duplication { or_standard: false } => None,
            })
            .collect()
    }

    /// The capture feature's change to the worker, as ported here: a task is answered from a
    /// frame read through ITS OWN source, the same region read two ways is two frames, and a
    /// read the module's `fallback = "none"` left without a picture is "could not look".
    #[test]
    fn each_task_is_answered_from_a_frame_read_through_its_own_source() {
        let red = || vec![(dot([255, 0, 0]), None)];
        let region = (0, 0, 10, 10);
        let mut dup = task(2, region, red(), Mode::First);
        dup.source = CaptureSource::Duplication { or_standard: true };
        let mut none = task(3, region, red(), Mode::First);
        none.source = CaptureSource::Duplication { or_standard: false };
        let out = run_batch(&[task(1, region, red(), Mode::First), dup, none], by_source);
        assert_eq!(out[0].found, Some(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        assert_eq!(out[1].found, Some(vec![None]), "not the standard frame of the same region");
        assert!(out[2].found.is_none(), "no picture under fallback = \"none\" is nil, not a miss");
        assert!(out.iter().all(|r| r.batch == 3));
    }

    /// Critique issue 2: a panic on the worker used to end it for the session, and every
    /// callback after that silently never fired. Now it is answered, and the NEXT search
    /// still works.
    #[test]
    fn a_panic_is_answered_and_the_worker_lives_on() {
        crate::quiet_expected_panics();
        let (task_tx, task_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        spawn_image_worker(panics_at_666, task_rx, res_tx);
        task_tx.send(task(1, (666, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)).unwrap();
        let first = res_rx.recv_timeout(Duration::from_secs(10)).expect("an answer, not silence");
        assert_eq!(first.id, 1);
        assert!(first.found.is_none(), "a panicking batch could not look");
        task_tx.send(task(2, (0, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)).unwrap();
        let second = res_rx.recv_timeout(Duration::from_secs(10)).expect("the worker survived");
        assert_eq!(second.id, 2);
        assert_eq!(second.found, Some(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
    }

    /// The worker's panics are reported by the worker, with their place, and kept from the
    /// panic hook — which in the application would otherwise log each one in full.
    #[test]
    fn a_contained_panic_is_reported_with_its_place() {
        crate::quiet_expected_panics();
        let e = logging::contain(|| -> u8 { panic!("tripped inside") }).unwrap_err();
        assert!(e.contains("tripped inside") && e.contains("image_search.rs"), "{e}");
        assert_eq!(logging::contain(|| 5), Ok(5));
        // Outside `contain` the hook is not told to hold anything, and asks for no report.
        assert!(!logging::hold_contained_panic(|| unreachable!()));
    }

    #[test]
    fn a_within_is_clipped_to_the_captured_region() {
        assert_eq!(within_frame((100, 100, 50, 50), (90, 120, 30, 100)), Rect { x: 0, y: 20, w: 20, h: 30 });
        assert_eq!(within_frame((100, 100, 50, 50), (0, 0, 10, 10)).w, 0);
    }

    #[test]
    fn a_path_names_itself_and_a_stranger_is_refused() {
        let lua = Lua::new();
        let d = Arc::new(Decoded::from_png_rgba(1, 1, vec![0, 0, 0, 255]));
        let d2 = d.clone();
        let load = move |_: &str| Ok(d2.clone());
        let v = Value::String(lua.create_string("images/a.png").unwrap());
        let (got, name) = resolve_with(&v, "imageSearch", &load).unwrap();
        assert!(Arc::ptr_eq(&got, &d));
        assert_eq!(name.as_deref(), Some("images/a.png"));
        let e = err_text(resolve_with(&Value::Integer(3), "imageSearch", &load));
        assert!(e.contains("expected a path or a Template"), "{e}");
        // A mixed list resolves completely, in order, before anything is captured.
        let list: Table = lua.load("return { 'a.png', 'b.png' }").eval().unwrap();
        assert_eq!(resolve_list(&list, "imageSearchMulti", &load).unwrap().len(), 2);
        let bad: Table = lua.load("return { 'a.png', true }").eval().unwrap();
        assert!(resolve_list(&bad, "imageSearchMulti", &load).is_err());
    }
}
