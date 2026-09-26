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
    Function, Lua, MetaMethod, MultiValue, RegistryKey, Table, UserData, UserDataFields,
    UserDataMethods, Value,
};
use rayon::prelude::*;

use crate::backend::frame::Frame;
use crate::backend::{CaptureFn, CaptureSource, CapturedImage};
use crate::capture_source;
use crate::cells;
use crate::ocr::types as geo;
use crate::region::{self, ScreenRect};
use crate::snapshot::{self, Picture};
use crate::template::{self, Decoded, Rect};
use crate::{
    call_guarded, cells_match_table, logging, one_value, opts_region, read_scales, region_arg, region_lua, with_reason,
    Shared, SHORT_CAPTURE,
};

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

/// Whose VM `lua` is, as `register_vm` recorded it; `None` for a state it never tagged.
///
/// Shared with every other registration that has to die with the VM it was made from rather
/// than with the identity it was made under — a settings `onChange` among them.
pub(crate) fn vm_owner(lua: &Lua) -> Option<VmOwner> {
    lua.try_app_data_ref::<VmOwner>().ok().flatten().map(|o| *o)
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
    if let Some(o) = vm_owner(lua) {
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
    /// off. A `matchCellsAsync` is answered instead of read again (`resend`).
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
    /// Each entry's name — or each state's, for `matchCellsAsync` — travelling here rather
    /// than through the worker: the worker only ever holds pixels, and a name is a Lua-side
    /// affair.
    names: Vec<Option<String>>,
    /// The search as it was sent, so it can be sent again after a `Fate::Hold`. `Arc`s and a
    /// few numbers: the pixels are shared with the worker's copy, not duplicated.
    task: ImageTask,
    /// Answered while its owner was disabled; waiting for the owner to be enabled again.
    held: bool,
    /// The priority of the dispatch that asked, which the callback runs under: a key press, then
    /// a search, then a text read stays the user waiting (`ocr::types::Priority`).
    prio: crate::ocr::types::Priority,
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
        task,
        held: false,
        prio: crate::ocr::types::current_priority(),
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
            again.push(resend(&p.task));
        } else {
            stale.push(*id);
        }
    }
    let stale = stale.into_iter().filter_map(|id| map.remove(&id)).collect();
    (again, stale)
}

/// What a held `matchCellsAsync` is answered when its module is enabled again.
const HELD_CELLS: &str = "the module was disabled while the read waited";

/// What a held task is sent as when its owner is enabled again. A search is made again exactly
/// as it was asked — a window region as the rectangle it was resolved to at the call, whose
/// hits are true screen positions whatever the window did since. A `matchCellsAsync` is not
/// read again: its rectangle was worked out at the
/// call — a window region from the window table the module passed then — and minutes later the
/// window may have moved, been resized or closed, so reading the old rectangle could rank the
/// wrong pixels and speak a confident match. It is answered `nil, HELD_CELLS` instead, by the
/// worker and without a capture, so the callback still comes on a later tick and still clears
/// a caller's in-flight flag.
fn resend(task: &ImageTask) -> ImageTask {
    match &task.job {
        Job::Search { .. } => task.clone(),
        Job::Cells(c) => ImageTask {
            job: Job::Cells(Arc::new(CellsJob {
                matcher: c.matcher.clone(),
                unresolved: Some(HELD_CELLS.to_string()),
            })),
            ..task.clone()
        },
    }
}

/// Frees what finished or abandoned entries hold in their VMs' registries.
fn release(gone: impl IntoIterator<Item = PendingImage>) {
    for PendingImage { lua, cb, .. } in gone {
        let _ = lua.remove_registry_value(cb);
    }
}

// ── The worker ─────────────────────────────────────────────────────────────────────────

/// A queued async search. The worker CAPTURES its region itself (off the main thread) then
/// matches `entries` against it — or, given a snapshot, reads the snapshot and captures
/// nothing. The region is (x, y, w, h) in screen coords, whose (x, y) is added back into every
/// reported hit.
///
/// In `Mode::First` the entries are tried in order against the SAME captured frame and the
/// first hit wins — several renderings of the same thing (a dialog's close glyph as two plugin
/// versions draw it). Chaining single-template searches instead would pay a fresh region
/// capture (~1 compositor frame) per template. In `Mode::Each` every entry is answered, each
/// within its own sub-rectangle of that one frame.
#[derive(Clone)]
pub(crate) struct ImageTask {
    pub(crate) id: u64,
    /// What the task reads.
    pub(crate) hay: Haystack,
    /// What is done with the frame.
    pub(crate) job: Job,
}

/// What a task reads.
#[derive(Clone)]
pub(crate) enum Haystack {
    /// The screen, captured by the worker: every task there was before snapshots.
    Screen {
        region: (i32, i32, i32, i32),
        /// How the asking module's VM reads the screen (`capture_source.rs`). Part of the
        /// frame's identity in a batch: the same region read two ways is two frames. Travels
        /// with the task, so a held search sent again on re-enable reads the way it was first
        /// asked to.
        source: CaptureSource,
    },
    /// A snapshot the module passed, read where `region` lies in it — already cut to it at the
    /// call — and never captured. Shared with the module's handle, not copied: a snapshot
    /// released while the task waits keeps its pixels until the task is done.
    Frame { frame: Arc<Frame>, region: geo::Rect },
}

/// What the worker does with a task's frame.
#[derive(Clone)]
pub(crate) enum Job {
    /// Template search.
    Search {
        /// Each template with the part of the captured frame it may be found in (`None`: all
        /// of it), in frame coordinates.
        entries: Vec<(Arc<Decoded>, Option<Rect>)>,
        tol: u8,
        scales: Vec<f32>,
        mode: Mode,
        /// Why there is nothing to read this time — a window region whose client area was
        /// empty at the call. Answered on the worker without a capture, as a cells read is, so
        /// the callback still comes on a later tick.
        unresolved: Option<String>,
    },
    /// `matchCellsAsync`: the whole frame reduced to cells and ranked against the states. A
    /// cells task shares its capture with every search of the same region and source in its
    /// batch.
    Cells(Arc<CellsJob>),
}

/// A `matchCellsAsync` call's work.
pub(crate) struct CellsJob {
    /// Shared, so that a held task can be sent again as an answer without copying the states.
    pub(crate) matcher: Arc<cells::Matcher>,
    /// Why there is nothing to read this time — the window's client area was empty when the
    /// call was made. Answered on the worker, without a capture, so that the callback still
    /// comes on a later tick like every other answer.
    pub(crate) unresolved: Option<String>,
}

impl ImageTask {
    /// Whether the worker reads the screen for this task: never for a snapshot's. The batch asks
    /// `screen_key`; the tests ask this.
    #[cfg(test)]
    fn captures(&self) -> bool {
        self.screen_key().is_some()
    }

    /// The frame the worker captures for this task — its region and source — or `None` when
    /// it captures nothing: a snapshot's task, and one answered with why there is nothing to read.
    fn screen_key(&self) -> Option<capture_source::FrameKey> {
        let unresolved = match &self.job {
            Job::Search { unresolved, .. } => unresolved.is_some(),
            Job::Cells(c) => c.unresolved.is_some(),
        };
        match &self.hay {
            Haystack::Screen { region, source } if !unresolved => Some((*region, *source)),
            _ => None,
        }
    }

    /// The screen rectangle the task reads: where its hits are measured from and what a cells
    /// answer reports.
    pub(crate) fn region(&self) -> (i32, i32, i32, i32) {
        match &self.hay {
            Haystack::Screen { region, .. } => *region,
            Haystack::Frame { region, .. } => region.tuple(),
        }
    }

    /// The binding that asked, for an error report.
    fn binding(&self) -> &'static str {
        match &self.job {
            Job::Search { mode, .. } => mode.binding(),
            Job::Cells(_) => "matchCellsAsync",
        }
    }
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

/// What the worker found for one task.
pub(crate) enum Outcome {
    /// A search: `Err(reason)` when the region could not be read — "could not look", which is
    /// not the same answer as "looked and found nothing". Otherwise one slot per entry in
    /// `Mode::Each`, and exactly one slot, the first hit or none, in `Mode::First`.
    Search(Result<Vec<Option<ScreenHit>>, String>),
    /// A cells match: the live cells and their ranking, or why there are none.
    Cells(Result<(Vec<u8>, cells::Ranked), String>),
}

/// What a search is answered when its batch panicked inside the host.
const SEARCH_PANICKED: &str = "internal error: the search could not be made";

impl Outcome {
    /// The answer of a task whose batch panicked: could not look.
    fn failed(t: &ImageTask) -> Outcome {
        match &t.job {
            Job::Search { .. } => Outcome::Search(Err(SEARCH_PANICKED.to_string())),
            Job::Cells(_) => Outcome::Cells(Err("internal error: the cells could not be computed".to_string())),
        }
    }
}

/// The worker's answer.
pub(crate) struct ImageResult {
    pub(crate) id: u64,
    pub(crate) outcome: Outcome,
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
                        outcome: Outcome::failed(t),
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
    // Only the tasks that read the screen get a frame: a cells task whose window had no client
    // area is answered without one, and a snapshot's task reads the snapshot.
    let reading: Vec<(usize, capture_source::FrameKey)> =
        batch.iter().enumerate().filter_map(|(i, t)| t.screen_key().map(|k| (i, k))).collect();
    let (keys, frame_of_reading) = capture_source::frame_keys(reading.iter().map(|(_, k)| *k));
    let frames: Vec<(Result<CapturedImage, String>, u32)> = capture_source::capture_frames(capture, &keys);
    let mut frame_of: Vec<Option<usize>> = vec![None; batch.len()];
    for (&(i, _), &f) in reading.iter().zip(&frame_of_reading) {
        frame_of[i] = Some(f);
    }
    let plan: Vec<(&ImageTask, Option<usize>)> = batch.iter().zip(frame_of).collect();
    // MATCH IN PARALLEL. Every candidate position is independent of every other, so
    // this is the shape a thread pool is actually for; the tasks in a batch are
    // independent too. Sequentially, twelve installed libraries cost twelve full
    // scans back to back before any of them can answer — each one comfortably under
    // the logging threshold and therefore invisible, while together they were the
    // several seconds before the right overlay appeared.
    let t1 = Instant::now();
    let outcomes: Vec<Outcome> = plan
        .par_iter()
        .map(|(t, idx)| answer(t, idx.map(|i| frames[i].0.as_ref().map_err(String::as_str))))
        .collect();
    let match_ms = t1.elapsed().as_millis() as u32;
    let capture_ms: u32 = frames.iter().map(|f| f.1).sum();
    plan.iter()
        .zip(outcomes)
        .enumerate()
        .map(|(i, ((t, _), outcome))| ImageResult {
            id: t.id,
            outcome,
            capture_ms,
            match_ms,
            batch: batch_len,
            first_of_batch: i == 0,
        })
        .collect()
}

/// What a task's matching reads: a picture, where its top-left pixel is on screen, and the part
/// of it the task's region covers — `None` for all of it, a capture of exactly the region, which
/// is every task there was before snapshots and is read exactly as it always was.
struct Pic<'a> {
    img: &'a CapturedImage,
    origin: (i32, i32),
    area: Option<Rect>,
}

/// The picture a task reads: its snapshot, or the frame the batch captured for it (or why that
/// capture failed; `None` when none was made for it).
fn picture<'a>(t: &'a ImageTask, shot: Option<Result<&'a CapturedImage, &'a str>>) -> Result<Pic<'a>, String> {
    match &t.hay {
        // Its region was cut to the snapshot at the call; a region outside it cannot get here.
        Haystack::Frame { frame, region } => match frame.area(*region) {
            Some(a) => Ok(Pic {
                img: &frame.img,
                origin: (frame.rect.x, frame.rect.y),
                area: Some(Rect { x: a.x, y: a.y, w: a.w, h: a.h }),
            }),
            None => Err(snapshot::NO_OVERLAP.to_string()),
        },
        Haystack::Screen { region, .. } => match shot {
            Some(Ok(img)) => Ok(Pic { img, origin: (region.0, region.1), area: None }),
            Some(Err(why)) => Err(why.to_string()),
            None => Err(crate::backend::CAPTURE_FAILED.to_string()),
        },
    }
}

/// One task against its picture: the frame the batch captured for it, or why the capture
/// failed; `None` for a task that was not captured (an unresolved one, which carries its own
/// reason, and a snapshot's, which reads the snapshot).
fn answer(t: &ImageTask, frame: Option<Result<&CapturedImage, &str>>) -> Outcome {
    match &t.job {
        Job::Search { entries, tol, scales, mode, unresolved } => Outcome::Search(match unresolved {
            Some(why) => Err(why.clone()),
            None => picture(t, frame).map(|pic| match_task(&pic, entries, *tol, scales, *mode)),
        }),
        Job::Cells(job) => Outcome::Cells(cells_task(t, job, frame)),
    }
}

/// A `matchCellsAsync` against its picture. A panic in the reduction is answered, like a
/// template's, rather than taking the worker with it.
fn cells_task(
    t: &ImageTask,
    job: &CellsJob,
    frame: Option<Result<&CapturedImage, &str>>,
) -> Result<(Vec<u8>, cells::Ranked), String> {
    if let Some(why) = &job.unresolved {
        return Err(why.clone());
    }
    let pic = picture(t, frame)?;
    let (_, _, w, h) = t.region();
    match logging::contain(|| match pic.area {
        // A capture of exactly the region, held to that size as it always was.
        None => job.matcher.answer(pic.img, w, h),
        Some(a) => job.matcher.answer_sub(pic.img, cells::Sub { x: a.x, y: a.y, w: a.w, h: a.h }),
    }) {
        Ok(answer) => answer,
        Err(report) => {
            contained("a cells match", "it was answered nil", &report);
            Err("internal error: the cells could not be computed".to_string())
        }
    }
}

/// One search against its picture.
fn match_task(
    pic: &Pic,
    entries: &[(Arc<Decoded>, Option<Rect>)],
    tol: u8,
    scales: &[f32],
    mode: Mode,
) -> Vec<Option<ScreenHit>> {
    let (ox, oy) = pic.origin;
    let one = |n: usize, (dec, within): &(Arc<Decoded>, Option<Rect>)| -> Option<ScreenHit> {
        // A `within` is relative to the region; in a snapshot the region starts at the area's
        // corner, and it was cut to the region at the call, so it stays inside the area.
        let area = match (within, pic.area) {
            (Some(w), Some(b)) => Some(Rect { x: b.x + w.x, y: b.y + w.y, w: w.w, h: w.h }),
            (Some(w), None) => Some(*w),
            (None, b) => b,
        };
        // Per entry, so one template that trips something cannot take its neighbours' answers
        // with it. On whichever pool thread runs it: `contain` marks the thread it runs on.
        match logging::contain(|| template::find(pic.img, area, dec, tol, scales)) {
            Ok(hit) => hit.map(|h| ScreenHit {
                x: ox + h.x as i32,
                y: oy + h.y as i32,
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
    match mode {
        // Templates in order against this one frame; first hit wins.
        Mode::First => vec![entries.iter().enumerate().find_map(|(n, e)| one(n, e))],
        Mode::Each => entries.par_iter().enumerate().map(|(n, e)| one(n, e)).collect(),
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
                    let _prio = crate::ocr::types::enter_priority(p.prio);
                    if let Ok(f) = p.lua.registry_value::<Function>(&p.cb) {
                        let args = result_args(&p.lua, &p.task, &p.names, res.outcome);
                        if let Err(e) = call_guarded(&f, args) {
                            self.report_callback_error(p.scope, p.task.binding(), &e);
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
    /// `Fate::Hold`), and every held cells read as the answer it gets instead (`resend`).
    /// Called by `apply_enabled` when the module is enabled.
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

/// What the callback is called with: a search's one value, or `(nil, reason)` when it could not
/// look; a cells match's `(match, nil)` / `(nil, reason)`.
fn result_args(lua: &Lua, task: &ImageTask, names: &[Option<String>], outcome: Outcome) -> MultiValue {
    let values = match (&task.job, outcome) {
        (Job::Search { mode, .. }, Outcome::Search(Ok(found))) => vec![result_value(lua, *mode, names, Some(found))],
        // One value more than a search that looked, so a callback written for `cb(hit)` reads
        // it exactly as before.
        (Job::Search { .. }, Outcome::Search(Err(why))) => vec![Value::Nil, reason(lua, why)],
        (_, Outcome::Cells(Ok((live, ranked)))) => {
            let (x, y, w, h) = task.region();
            match cells_match_table(lua, &live, ScreenRect { x, y, w, h }, &ranked, names) {
                Ok(t) => vec![Value::Table(t), Value::Nil],
                Err(e) => vec![Value::Nil, reason(lua, format!("internal error: {e}"))],
            }
        }
        (_, Outcome::Cells(Err(why))) => vec![Value::Nil, reason(lua, why)],
        // A search answered as cells, or the reverse: cannot happen, as the worker answers each
        // task by its own job. Answered as "could not look" rather than unwrapped.
        (Job::Cells(_), Outcome::Search(_)) => vec![Value::Nil, reason(lua, "internal error".to_string())],
    };
    MultiValue::from_vec(values)
}

fn reason(lua: &Lua, why: String) -> Value {
    lua.create_string(why).map(Value::String).unwrap_or(Value::Nil)
}

/// What a search's callback receives. Built in the owner's VM, from the answer and the names
/// that stayed on this side.
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
    /// The screen now — or, with `snapshot`, that snapshot's pixels, cut out, never captured.
    Capture { x: i32, y: i32, w: i32, h: i32, snapshot: Option<Arc<Frame>> },
    /// A `capture` given as a window region that has nothing to capture this time — an empty
    /// client area, or one so large that the region is past a template's size limit. Answered
    /// `nil, reason`, like a failed capture: the window's size is a run-time condition.
    NotNow(String),
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
                v => crate::json::luau_type(v).to_string(),
            }
        ))),
    }
}

/// Reads and checks a `host.screen.template` spec without touching the screen or a file.
///
/// Strict, unlike the search options, and deliberately: a spec is written once, and a field
/// that is silently ignored — a misspelt `rbga`, a `tolerance` this version does not take —
/// is a template that does something other than what its author reads in it. The `capture`
/// region is read by the Region form's one reader, as every other call's is: corners the way
/// `opts.region` takes them, or a window region resolved here, at the call. With `snapshot`,
/// a corner left out is the snapshot's edge, and the template is cut from that snapshot.
fn parse_spec(spec: &Table, sw: i32, sh: i32) -> mlua::Result<ParsedSpec> {
    let mut name = None;
    let (mut w, mut h) = (None, None);
    let mut snapshot_value: Option<Value> = None;
    let mut sources: Vec<(String, Value)> = Vec::new();
    for pair in spec.clone().pairs::<Value, Value>() {
        let (k, v) = pair?;
        let key = match &k {
            Value::String(s) => s.to_str()?.to_string(),
            other => {
                return Err(spec_err(format!(
                    "field names must be strings, got a {}",
                    crate::json::luau_type(other)
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
                    return Err(spec_err(format!("name must be a string, got a {}", crate::json::luau_type(&other))))
                }
            },
            "w" => w = Some(whole(&v, "w")?),
            "h" => h = Some(whole(&v, "h")?),
            "snapshot" => snapshot_value = Some(v),
            "rgba" | "rgb" | "capture" | "file" => sources.push((key, v)),
            other => {
                return Err(spec_err(format!(
                    "unknown field '{other}'; a spec takes name and exactly one of rgba, rgb, \
                     capture or file (and snapshot with capture)"
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
    // A snapshot is something to cut a template FROM, so it goes with `capture` and nothing
    // else; read (and refused when it is not one, or was released) before anything is cut.
    let snap = match snapshot_value {
        None => None,
        Some(s) if key == "capture" => snapshot::from_value(&s, "host.screen.template", "snapshot")?,
        Some(_) => {
            return Err(spec_err(format!(
                "snapshot belongs with capture: a template is cut from a snapshot with {{ capture = region, snapshot = s }}, not built from {key}"
            )))
        }
    };
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
                        crate::json::luau_type(&other)
                    )))
                }
            };
            Source::Bytes { rgba: key == "rgba", data, w, h }
        }
        "capture" => {
            if w.is_some() || h.is_some() {
                return Err(spec_err("w and h belong with rgba or rgb; a capture takes its size from its region"));
            }
            if !matches!(v, Value::Table(_)) {
                return Err(spec_err(format!("capture must be a region, got a {}", crate::json::luau_type(&v))));
            }
            let given = match &snap {
                // A corner left out is the snapshot's edge.
                Some(f) => {
                    let e = f.rect;
                    region_lua::read_loose_within(&v, "capture", (e.x, e.y, e.right(), e.bottom())).map_err(spec_err)?
                }
                None => region_lua::read_loose(&v, "capture", (sw, sh)).map_err(spec_err)?,
            };
            match given.resolve() {
                Ok(r) => {
                    // Checked BEFORE capturing: the limit is what the capture would produce, and
                    // capturing first would spend a compositor frame to be told no. Corners are
                    // the module's own, so past the limit is a mistake and raises; a window
                    // region's size is the window's, so it is answered instead.
                    let size = template::check_size(r.w.max(0) as u32, r.h.max(0) as u32);
                    match (size, &given) {
                        (Ok(()), _) => Source::Capture { x: r.x, y: r.y, w: r.w, h: r.h, snapshot: snap },
                        (Err(e), region::Region::Rect(_)) => return Err(spec_err(e)),
                        (Err(e), region::Region::Window(..)) => {
                            // `e` names the size itself: "5000x10 is too large; …".
                            Source::NotNow(format!("at the window's current size {e}"))
                        }
                    }
                }
                Err(u) => Source::NotNow(u.to_string()),
            }
        }
        _ => {
            if w.is_some() || h.is_some() {
                return Err(spec_err("w and h belong with rgba or rgb; a file has its own size"));
            }
            let Value::String(p) = v else {
                return Err(spec_err(format!("file must be a path, got a {}", crate::json::luau_type(&v))));
            };
            Source::File(p.to_str()?.to_string())
        }
    };
    Ok(ParsedSpec { name, source })
}

/// Turns a checked spec into a handle, with the file loader and the screen capture passed in,
/// so every rule here is testable without a backend.
///
/// `nil, reason` only for a capture that failed or had nothing to capture: that is a runtime
/// condition the module has to handle, while every other refusal is a mistake in the code that
/// wrote the spec, and raises.
fn build_handle(
    lua: &Lua,
    spec: ParsedSpec,
    load: impl FnOnce(&str) -> mlua::Result<Arc<Decoded>>,
    capture: impl FnOnce(i32, i32, i32, i32) -> Result<CapturedImage, String>,
) -> mlua::Result<MultiValue> {
    let budget = budget(lua);
    let mut name = spec.name;
    let (dec, charge) = match spec.source {
        Source::NotNow(why) => return with_reason(lua, Value::Nil, why),
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
        Source::Capture { x, y, w, h, snapshot: snap } => {
            let charge = reserve(lua, &budget, template::dense_bytes(w as u32, h as u32))?;
            let got = match snap {
                // Cut from the snapshot, whole or not at all: a template cut from a clipped
                // rectangle would be a different template.
                Some(frame) => frame.crop_image(geo::Rect::new(x, y, w, h)).ok_or_else(|| snapshot::NOT_INSIDE.to_string()),
                None => capture(x, y, w, h),
            };
            let cap = match got {
                Ok(cap) => cap,
                Err(why) => return with_reason(lua, Value::Nil, why),
            };
            match Decoded::from_capture(cap) {
                Ok(d) => (Arc::new(d), charge),
                // A capture that came back short is a capture that failed.
                Err(_) => return with_reason(lua, Value::Nil, SHORT_CAPTURE.to_string()),
            }
        }
    };
    let count = dec.count();
    let handle = TemplateHandle { dec, name, count, _charge: charge };
    one_value(Value::UserData(lua.create_userdata(handle)?))
}

/// `host.screen.template(spec) -> Template | nil, reason`.
pub(crate) fn template(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, spec: Table| {
        let (sw, shh) = sh.backend.screen_size();
        let parsed = parse_spec(&spec, sw, shh)?;
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
            // read the standard way, or is `nil` and the reason under fallback = "none", and the
            // template keeps that picture for the session. docs/api/screen.md says so; TODO.md
            // holds the question of changing it. The source first, outside the timing, as `pixel` does:
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
            crate::json::luau_type(&other)
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

/// `opts.region` of a search, resolved now: the rectangle, or why a window region has none.
fn search_region(
    sh: &Shared,
    opts: Option<&Table>,
    fname: &str,
) -> mlua::Result<Result<(i32, i32, i32, i32), String>> {
    Ok(opts_region(&*sh.backend, opts, fname)?.map(|r| (r.x, r.y, r.w, r.h)))
}

/// `opts.region` of a search on a snapshot: read with the snapshot's edges as its defaults (a
/// region left out is the whole snapshot), then cut to the snapshot — the rectangle, or why
/// there is nothing of it to read (a window region with none this time, or no overlap).
fn snapshot_region(opts: Option<&Table>, fname: &str, frame: &Frame) -> mlua::Result<Result<(i32, i32, i32, i32), String>> {
    Ok(snapshot::opts_region_in(opts, fname, frame)?.and_then(|r| snapshot::clip(frame, r)).map(|r| r.tuple()))
}

/// Captures `region` through this VM's source on the event loop, for the synchronous searches.
fn capture_now(sh: &Shared, lua: &Lua, (x, y, w, h): (i32, i32, i32, i32)) -> Result<CapturedImage, String> {
    let src = capture_source::read_source(lua, &*sh.backend, (x, y, w, h));
    sh.backend.capture(x, y, w, h, src)
}

/// What a synchronous search reads: `region` of the snapshot when it was given one — already
/// cut to it, and nothing is captured — or a capture of `region` made now.
fn picture_now(sh: &Shared, lua: &Lua, snap: Option<Arc<Frame>>, region: (i32, i32, i32, i32)) -> Result<Picture, String> {
    match snap {
        Some(frame) => Ok(Picture::Snap { frame, rect: geo::Rect::from_tuple(region) }),
        None => capture_now(sh, lua, region).map(|img| Picture::Live { img, x: region.0, y: region.1 }),
    }
}

/// `host.screen.imageSearch(template, opts?) -> Hit | nil, reason?`, synchronous. `nil` alone
/// when it looked and found nothing; `nil, reason` when it could not look.
pub(crate) fn image_search(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (tv, opts): (Value, Option<Table>)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let (tmpl, name) = resolve_with(&tv, "imageSearch", &load)?;
        let snap = snapshot::from_opts(opts.as_ref(), "host.screen.imageSearch")?;
        let region = match &snap {
            Some(f) => snapshot_region(opts.as_ref(), "host.screen.imageSearch", f)?,
            None => search_region(&sh, opts.as_ref(), "host.screen.imageSearch")?,
        };
        let region = match region {
            Ok(r) => r,
            Err(why) => return with_reason(lua, Value::Nil, why),
        };
        let pic = match picture_now(&sh, lua, snap, region) {
            Ok(pic) => pic,
            Err(why) => return with_reason(lua, Value::Nil, why),
        };
        let tol = read_tol(opts.as_ref());
        let scales = read_scales(opts.as_ref());
        let (ox, oy) = pic.origin();
        match template::find(pic.image(), pic.search_area(), &tmpl, tol, &scales) {
            Some(h) => one_value(Value::Table(hit_table(lua, ox + h.x as i32, oy + h.y as i32, h.w, h.h, 1, name.as_deref())?)),
            // Looked, and it is not there: `nil` alone, as always.
            None => one_value(Value::Nil),
        }
    })
}

/// `host.screen.imageSearchMulti(templates, opts?) -> (n, Hit) | (nil, nil) | (nil, nil, reason)`.
///
/// Captures the region ONCE and tries each template in order, returning the 1-based index of
/// the first match plus its hit. One capture serves many comparisons (e.g. a toggle's on/off
/// pair) — half the ~1-frame screen touches of two imageSearch calls, and both templates are
/// matched against the SAME frame, so a state change mid-repaint can't fall between two
/// separate captures. Sync, like imageSearch (AHK-style). Paths share the decode cache.
///
/// "Could not look" is a THIRD value, the reason, after the two `nil`s "nothing matched" has
/// always returned: a caller that reads one or two values reads exactly what it did before.
pub(crate) fn image_search_multi(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (templates, opts): (Table, Option<Table>)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let list = resolve_list(&templates, "imageSearchMulti", &load)?;
        let could_not_look = |why: String| -> mlua::Result<MultiValue> {
            Ok(MultiValue::from_vec(vec![Value::Nil, Value::Nil, reason(lua, why)]))
        };
        let snap = snapshot::from_opts(opts.as_ref(), "host.screen.imageSearchMulti")?;
        let region = match &snap {
            Some(f) => snapshot_region(opts.as_ref(), "host.screen.imageSearchMulti", f)?,
            None => search_region(&sh, opts.as_ref(), "host.screen.imageSearchMulti")?,
        };
        let region = match region {
            Ok(r) => r,
            Err(why) => return could_not_look(why),
        };
        let pic = match picture_now(&sh, lua, snap, region) {
            Ok(pic) => pic,
            Err(why) => return could_not_look(why),
        };
        let tol = read_tol(opts.as_ref());
        let scales = read_scales(opts.as_ref());
        let (ox, oy) = pic.origin();
        for (i, (tmpl, name)) in list.iter().enumerate() {
            if let Some(h) = template::find(pic.image(), pic.search_area(), tmpl, tol, &scales) {
                let t = hit_table(lua, ox + h.x as i32, oy + h.y as i32, h.w, h.h, i + 1, name.as_deref())?;
                return Ok(MultiValue::from_vec(vec![Value::Integer(i as i64 + 1), Value::Table(t)]));
            }
        }
        Ok(MultiValue::from_vec(vec![Value::Nil, Value::Nil]))
    })
}

/// What a queued task reads, decided here on the main thread — the worker never sees a Lua
/// state: `region` of the snapshot when the call was given one (already cut to it or found
/// inside it at the call), which is never captured and never asks the module's source; else
/// `region` of the screen through this VM's source. `region` may instead be why there is
/// nothing to read this time, answered on the worker without a capture.
fn haystack(
    sh: &Shared,
    lua: &Lua,
    snap: Option<Arc<Frame>>,
    region: Result<(i32, i32, i32, i32), String>,
) -> (Haystack, Option<String>) {
    match (snap, region) {
        (Some(frame), Ok(r)) => (Haystack::Frame { frame, region: geo::Rect::from_tuple(r) }, None),
        (Some(frame), Err(why)) => (Haystack::Frame { frame, region: geo::Rect::default() }, Some(why)),
        (None, Ok(r)) => (Haystack::Screen { region: r, source: capture_source::read_source(lua, &*sh.backend, r) }, None),
        (None, Err(why)) => {
            (Haystack::Screen { region: (0, 0, 0, 0), source: capture_source::vm_source(lua) }, Some(why))
        }
    }
}

/// Queues a task for the worker, with its callback waiting on this side. `region` may instead
/// be why a window region has nothing to read this time; the worker answers that without a
/// capture, on a later tick like every other answer.
#[allow(clippy::too_many_arguments)]
fn enqueue(
    sh: &Shared,
    lua: &Lua,
    scope: usize,
    cb: Function,
    region: Result<(i32, i32, i32, i32), String>,
    entries: Vec<(Arc<Decoded>, Option<Rect>)>,
    names: Vec<Option<String>>,
    opts: Option<&Table>,
    mode: Mode,
    snap: Option<Arc<Frame>>,
) -> mlua::Result<()> {
    let tol = read_tol(opts);
    let scales = read_scales(opts);
    // The one place both async searches decide what the worker reads.
    let (hay, unresolved) = haystack(sh, lua, snap, region);
    submit(sh, lua, scope, cb, names, hay, Job::Search { entries, tol, scales, mode, unresolved })
}

/// Queues `host.screen.matchCellsAsync`: the worker captures `rect` through this VM's source —
/// or reads it where it lies in `snap` — reduces it to cells and ranks them against the
/// matcher's states. `rect` may instead be why there is nothing to read this time; that is
/// answered on the worker's next batch without a capture, so the callback comes on a later
/// tick either way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue_cells(
    sh: &Shared,
    lua: &Lua,
    scope: usize,
    cb: Function,
    rect: Result<(i32, i32, i32, i32), String>,
    matcher: cells::Matcher,
    names: Vec<Option<String>>,
    snap: Option<Arc<Frame>>,
) -> mlua::Result<()> {
    let (hay, unresolved) = haystack(sh, lua, snap, rect);
    let job = Job::Cells(Arc::new(CellsJob { matcher: Arc::new(matcher), unresolved }));
    submit(sh, lua, scope, cb, names, hay, job)
}

/// Sends a task to the worker, with its callback waiting on this side.
#[allow(clippy::too_many_arguments)]
fn submit(
    sh: &Shared,
    lua: &Lua,
    scope: usize,
    cb: Function,
    names: Vec<Option<String>>,
    hay: Haystack,
    job: Job,
) -> mlua::Result<()> {
    // The worker captures the region itself and ALWAYS posts a result (a failed capture
    // or a contained panic becomes an answer on the tick), so the pending callback is
    // always drained.
    let id = sh.next_image_id.get() + 1;
    sh.next_image_id.set(id);
    let task = ImageTask { id, hay, job };
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
/// calling cb(Hit) | cb(nil) | cb(nil, reason) on a later tick, so a detection poll (e.g. the
/// 500 ms landmark poll) never blocks the event loop on either. Only path resolution and the
/// region happen here, and the paths are cached.
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
        let snap = snapshot::from_opts(opts.as_ref(), "host.screen.imageSearchAsync")?;
        let region = match &snap {
            Some(f) => snapshot_region(opts.as_ref(), "host.screen.imageSearchAsync", f)?,
            None => search_region(&sh, opts.as_ref(), "host.screen.imageSearchAsync")?,
        };
        let (entries, names) = list.into_iter().map(|(d, n)| ((d, None), n)).unzip();
        enqueue(&sh, lua, idx, cb, region, entries, names, opts.as_ref(), Mode::First, snap)
    })
}

/// `host.screen.imageSearchEach(entries, opts?, cb)` — ONE capture, and an answer for every
/// entry: `cb(list)` with a hit or `false` in each slot, or `cb(nil, reason)` when the region
/// could not be captured.
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
        const F: &str = "host.screen.imageSearchEach";
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let snap = snapshot::from_opts(opts.as_ref(), F)?;
        let region = match &snap {
            Some(f) => snapshot_region(opts.as_ref(), F, f)?,
            None => search_region(&sh, opts.as_ref(), F)?,
        };
        // Where the entries' `within` are cut to; nothing when the region itself is not read.
        let frame = region.clone().unwrap_or((0, 0, 0, 0));
        let mut entries = Vec::new();
        let mut names = Vec::new();
        for (i, v) in list.clone().sequence_values::<Value>().enumerate() {
            let v = v?;
            match v {
                Value::Table(e) => {
                    // Named as the `within` below is, `entries[2]`, so one call names its
                    // entries one way.
                    let what = format!("{F}: entries[{}]", i + 1);
                    let tv: Value = e.get("template")?;
                    if tv.is_nil() {
                        return Err(mlua::Error::external(format!(
                            "{what} is a table without a template"
                        )));
                    }
                    let (dec, own) = resolve_with(&tv, &what, &load)?;
                    let within = match e.get::<Value>("within")? {
                        Value::Nil => None,
                        r @ Value::Table(_) => {
                            // The one reader, as for `opts.region` — on a snapshot with its edges
                            // as the defaults. A window `within` whose window has nothing to read
                            // this time is an empty part: that entry is `false`.
                            let part = match &snap {
                                Some(f) => snapshot::loose_region(&r, F, &format!("entries[{}].within", i + 1), f)?,
                                None => region_arg(&*sh.backend, &r, F, &format!("entries[{}].within", i + 1))?,
                            };
                            Some(match part {
                                Ok(p) => within_frame(frame, (p.x, p.y, p.w, p.h)),
                                Err(_) => Rect { x: 0, y: 0, w: 0, h: 0 },
                            })
                        }
                        other => {
                            return Err(mlua::Error::external(format!(
                                "{what}: within must be a region, got a {}",
                                crate::json::luau_type(&other)
                            )))
                        }
                    };
                    let name = match e.get::<Value>("name")? {
                        Value::Nil => own,
                        Value::String(s) => Some(s.to_str()?.to_string()),
                        other => {
                            return Err(mlua::Error::external(format!(
                                "{what}: name must be a string, got a {}",
                                crate::json::luau_type(&other)
                            )))
                        }
                    };
                    entries.push((dec, within));
                    names.push(name);
                }
                other => {
                    let (dec, name) = resolve_with(&other, &format!("{F}: entries[{}]", i + 1), &load)?;
                    entries.push((dec, None));
                    names.push(name);
                }
            }
        }
        if entries.is_empty() {
            return Err(mlua::Error::external("imageSearchEach: no template given".to_string()));
        }
        enqueue(&sh, lua, idx, cb, region, entries, names, opts.as_ref(), Mode::Each, snap)
    })
}

/// `host.screen.imageSearchAll(template, opts?) -> { Hit, … }, reason?` — EVERY match of the
/// template in the region, not just the first.
///
/// For deciding whether a template is safe to click blindly. A close-glyph template that
/// matches twice will eventually click the wrong one, and a template that matches nowhere is
/// a control that silently never fires; "matched exactly once, here" is the answer you want
/// before shipping either.
///
/// The list is never `nil`, as it never was: when the region could not be read it is empty
/// and the reason comes as a SECOND value, so "no match" and "could not look" can be told
/// apart by a caller that asks, and one that does not reads what it always did.
pub(crate) fn image_search_all(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (tv, opts): (Value, Option<Table>)| {
        let load = |p: &str| sh.load_template(&sh.root(idx).join(p));
        let (tmpl, name) = resolve_with(&tv, "imageSearchAll", &load)?;
        let snap = snapshot::from_opts(opts.as_ref(), "host.screen.imageSearchAll")?;
        let region = match &snap {
            Some(f) => snapshot_region(opts.as_ref(), "host.screen.imageSearchAll", f)?,
            None => search_region(&sh, opts.as_ref(), "host.screen.imageSearchAll")?,
        };
        let tol = read_tol(opts.as_ref());
        let out = lua.create_table()?;
        let region = match region {
            Ok(r) => r,
            Err(why) => return with_reason(lua, Value::Table(out), why),
        };
        let pic = match picture_now(&sh, lua, snap, region) {
            Ok(pic) => pic,
            Err(why) => return with_reason(lua, Value::Table(out), why),
        };
        let (ox, oy) = pic.origin();
        for (i, (x, y)) in template::find_all_in(pic.image(), pic.search_area(), &tmpl, tol).into_iter().enumerate() {
            let t = hit_table(lua, ox + x as i32, oy + y as i32, tmpl.w, tmpl.h, 1, name.as_deref())?;
            out.raw_set(i + 1, t)?;
        }
        one_value(Value::Table(out))
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
        parse_spec(&t, 1920, 1080)
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
            .and_then(|s| build_handle(&lua, s, no_file, |_, _, _, _| Err("screen capture failed".to_string()))));
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

    /// `capture` takes the window form through the one reader: resolved at the call, a window
    /// with nothing to capture answered `nil, reason`, and a table that is neither form raised.
    #[test]
    fn a_capture_takes_the_window_form() {
        let lua = Lua::new();
        let win = "window = { client = { x = 100, y = 50, w = 200, h = 100 } }";
        let s = spec(&lua, &format!("return {{ capture = {{ {win}, fraction = {{ 0.5, 0.5, 1, 1 }} }}, name = 'w' }}")).unwrap();
        let got = build_handle(&lua, s, no_file, |x, y, w, h| {
            assert_eq!((x, y, w, h), (200, 100, 100, 50), "the right lower quarter of the client area");
            Ok(opaque(w as u32, h as u32, [1, 2, 3]))
        })
        .unwrap();
        assert_eq!(got.len(), 1, "a handle alone");
        // A minimised window: nothing captured, `nil` and why.
        let s = spec(&lua, "return { capture = { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } } }").unwrap();
        let got = build_handle(&lua, s, no_file, |_, _, _, _| panic!("nothing to capture")).unwrap().into_vec();
        assert!(got[0].is_nil());
        assert_eq!(got[1].as_string().unwrap().to_string_lossy(), "the window's client area is empty (0x0)");
        // A window so large that the region is past a template's limit: the window's doing, so
        // answered, where corners past the limit are the module's mistake and raise.
        let s = spec(&lua, "return { capture = { window = { client = { x = 0, y = 0, w = 5000, h = 10 } }, fraction = { 0, 0, 1, 1 } } }").unwrap();
        let got = build_handle(&lua, s, no_file, |_, _, _, _| panic!("nothing to capture")).unwrap().into_vec();
        assert!(got[0].is_nil());
        assert_eq!(
            got[1].as_string().unwrap().to_string_lossy(),
            "at the window's current size 5000x10 is too large; no side may exceed 4096",
            "the size named once"
        );
        for (src, want) in [
            ("return { capture = {} }", "capture is neither { x1, y1, x2, y2 } nor { window = w, fraction = { x1, y1, x2, y2 } }"),
            ("return { capture = { x = 1, y = 2, w = 3, h = 4 } }", "such as a window's bounds"),
            (&format!("return {{ capture = {{ {win}, fraction = {{ 0, 0, 1 }} }} }}") as &str, "capture.fraction.y2 is missing"),
            ("return { capture = 'screen' }", "capture must be a region, got a string"),
        ] {
            let e = err_text(spec(&lua, src));
            assert!(e.contains("host.screen.template") && e.contains(want), "{src}:\n  got  {e}\n  want {want}");
        }
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
            let got = build_handle(&lua, s, no_file, |_, _, _, _| Err("screen capture failed".to_string())).unwrap();
            assert_eq!(got.len(), 1, "a handle is the one value, as it always was");
            lua.globals().set("t", got.into_vec().remove(0)).unwrap();
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
        let got = build_handle(&lua, s, no_file, |_, _, _, _| Err("screen capture failed".to_string())).unwrap().into_vec();
        assert!(got[0].is_nil(), "a failed capture answers nil");
        assert_eq!(got[1].as_string().map(|s| s.to_string_lossy()).as_deref(), Some("screen capture failed"), "and says why");
        assert_eq!(budget(&lua).get(), 0, "and gives its reservation back");

        let s = spec(&lua, "return { capture = { 10, 20, 14, 23 }, name = 'here' }").unwrap();
        let v = build_handle(&lua, s, no_file, |x, y, w, h| {
            assert_eq!((x, y, w, h), (10, 20, 4, 3), "the region, read like every other");
            let mut c = opaque(4, 3, [9, 9, 9]);
            for px in c.rgba.chunks_exact_mut(4) {
                px[3] = 0; // what an unforced GDI capture could have delivered
            }
            Ok(c)
        })
        .unwrap()
        .into_vec()
        .remove(0);
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
        }, |_, _, _, _| Err("screen capture failed".to_string()))
        .unwrap()
        .into_vec()
        .remove(0);
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
        let e = err_text(build_handle(&lua, s, no_file, |_, _, _, _| Err("screen capture failed".to_string())));
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
                let s = parse_spec(&t, 100, 100)?;
                build_handle(lua, s, no_file, |_, _, _, _| Err("screen capture failed".to_string()))
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
                let s = parse_spec(&t, 100, 100)?;
                build_handle(lua, s, no_file, |_, _, _, _| Err("screen capture failed".to_string()))
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

    /// A search asked while a key is dispatched keeps that priority for its callback, so a read
    /// the callback asks for waits in the interactive lane; one asked from a poll does not.
    #[test]
    fn a_search_remembers_the_priority_it_was_asked_under() {
        use crate::ocr::types::{enter_priority, Priority};
        let lua = Lua::new();
        let gens: HashMap<usize, u64> = [(1, 5)].into_iter().collect();
        let asked_on_a_key = {
            let _key = enter_priority(Priority::Interactive);
            entry(&lua, &gens, 1, 1)
        };
        let asked_by_a_poll = entry(&lua, &gens, 2, 1);
        assert_eq!(asked_on_a_key.prio, Priority::Interactive);
        assert_eq!(asked_by_a_poll.prio, Priority::Background);
        release([asked_on_a_key, asked_by_a_poll]);
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
        set_source(&mut map.get_mut(&10).unwrap().task, CaptureSource::Duplication { or_standard: false });
        map.get_mut(&11).unwrap().held = true;
        // 11 was held for an older VM at the same index.
        map.get_mut(&11).unwrap().gen = 5;
        // 12 is still in flight: its answer is on its way and must not be asked for twice.
        let (again, stale) = unhold(&mut map, 2, gens.get(&2).copied());
        assert_eq!(again.iter().map(|t| t.id).collect::<Vec<_>>(), vec![10]);
        assert_eq!(again[0].region(), (0, 0, 10, 10), "the same search");
        assert_eq!(
            source_of(&again[0]),
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

    /// A held `matchCellsAsync` is not read again on enable — its rectangle is from the window
    /// as it was at the call — but answered `nil, HELD_CELLS`, without a capture.
    #[test]
    fn a_held_cells_read_is_answered_not_read_again() {
        let lua = Lua::new();
        let _ = lua.try_set_app_data(VmOwner { idx: 2, gen: 6 });
        let gens: HashMap<usize, u64> = [(2, 6)].into_iter().collect();
        let cb: Function = lua.load("return function() end").eval().unwrap();
        let mut p = pending(&lua, &gens, 2, cb, vec![None, None], cells_matching(20, (5, 5, 2, 2), None)).unwrap();
        p.held = true;
        let mut map: HashMap<u64, PendingImage> = [(20, p)].into_iter().collect();
        let (again, stale) = unhold(&mut map, 2, Some(6));
        assert!(stale.is_empty());
        assert_eq!(again.len(), 1);
        let t = &again[0];
        assert_eq!(t.id, 20, "the same id, so the waiting callback gets the answer");
        assert!(!t.captures(), "nothing is captured for it");
        match answer(t, None) {
            Outcome::Cells(Err(why)) => assert_eq!(why, HELD_CELLS),
            _ => panic!("a held cells read must be answered with the reason"),
        }
        // Held again before that answer came: answered the same way on the next enable.
        map.get_mut(&20).unwrap().held = true;
        let (again, _) = unhold(&mut map, 2, Some(6));
        assert!(!again[0].captures());
        release(map.into_values());
    }

    // ── the worker ─────────────────────────────────────────────────────────────────────

    impl ImageResult {
        /// A search's answer; a cells answer here is a mistake in the test.
        fn found(&self) -> Result<Vec<Option<ScreenHit>>, String> {
            match &self.outcome {
                Outcome::Search(f) => f.clone(),
                Outcome::Cells(_) => panic!("a cells answer where a search's was expected"),
            }
        }
        fn cells(&self) -> &Result<(Vec<u8>, cells::Ranked), String> {
            match &self.outcome {
                Outcome::Cells(c) => c,
                Outcome::Search(_) => panic!("a search's answer where a cells answer was expected"),
            }
        }
    }

    /// A 2x2 grid of "clearly red", ranked against an all-zero state and an all-255 one.
    fn cells_matching(id: u64, region: (i32, i32, i32, i32), unresolved: Option<&str>) -> ImageTask {
        let spec = cells::CellSpec::new(2, 2, cells::Predicate::parse("r > g + b").unwrap()).unwrap();
        let states: Vec<Box<[u8]>> = vec![vec![0u8; 4].into(), vec![255u8; 4].into()];
        let matcher = cells::Matcher { spec, states, item_of: vec![0, 1] };
        let job = Job::Cells(Arc::new(CellsJob { matcher: Arc::new(matcher), unresolved: unresolved.map(str::to_string) }));
        ImageTask { id, hay: Haystack::Screen { region, source: CaptureSource::Standard }, job }
    }

    fn task(id: u64, region: (i32, i32, i32, i32), entries: Vec<(Arc<Decoded>, Option<Rect>)>, mode: Mode) -> ImageTask {
        ImageTask {
            id,
            hay: Haystack::Screen { region, source: CaptureSource::Standard },
            job: Job::Search { entries, tol: 0, scales: Vec::new(), mode, unresolved: None },
        }
    }

    /// A screen task read through `source` instead.
    fn set_source(t: &mut ImageTask, source: CaptureSource) {
        let region = t.region();
        t.hay = Haystack::Screen { region, source };
    }

    fn source_of(t: &ImageTask) -> CaptureSource {
        match &t.hay {
            Haystack::Screen { source, .. } => *source,
            Haystack::Frame { .. } => panic!("a snapshot's task has no source"),
        }
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
    fn two_dots(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Result<CapturedImage, String>> {
        regions.iter().map(|&(_, _, w, h)| Ok(two_dots_at(w, h))).collect()
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
        let found = out[0].found().expect("captured");
        assert_eq!(found[0], Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 }));
        assert_eq!(found[1], None);
        assert_eq!(found[2], None, "outside its within");
        assert_eq!(found[3], Some(ScreenHit { x: 108, y: 205, w: 1, h: 1, n: 4 }));
    }

    #[test]
    fn first_answers_the_first_entry_that_matches() {
        let entries = vec![(dot([0, 0, 255]), None), (dot([0, 255, 0]), None), (dot([255, 0, 0]), None)];
        let out = run_batch(&[task(1, (0, 0, 10, 10), entries, Mode::First)], two_dots);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 8, y: 5, w: 1, h: 1, n: 2 })]));
    }

    #[test]
    fn a_failed_capture_is_none_not_a_list_of_misses() {
        let entries = vec![(dot([255, 0, 0]), None)];
        let out = run_batch(&[task(1, (0, 0, 10, 10), entries, Mode::Each)], |r, _| r.iter().map(|_| Err("screen capture failed".to_string())).collect());
        assert_eq!(out[0].found(), Err("screen capture failed".to_string()), "could not look, and why");
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

    fn panics_at_666(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, String>> {
        if regions.iter().any(|r| r.0 == 666) {
            panic!("capture exploded");
        }
        two_dots(regions, src)
    }

    /// Two sources, told apart by what they "see": the standard one the two dots, duplication a
    /// frame of plain grey, and a duplication read that could not answer under
    /// `fallback = "none"` nothing at all.
    fn by_source(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, String>> {
        regions
            .iter()
            .map(|&(_, _, w, h)| match src {
                CaptureSource::Standard => Ok(two_dots_at(w, h)),
                CaptureSource::Duplication { or_standard: true } => Ok(opaque(w as u32, h as u32, [50, 50, 50])),
                CaptureSource::Duplication { or_standard: false } => {
                    Err("screen capture failed: desktop duplication could not answer — it is still opening".to_string())
                }
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
        set_source(&mut dup, CaptureSource::Duplication { or_standard: true });
        let mut none = task(3, region, red(), Mode::First);
        set_source(&mut none, CaptureSource::Duplication { or_standard: false });
        let out = run_batch(&[task(1, region, red(), Mode::First), dup, none], by_source);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        assert_eq!(out[1].found(), Ok(vec![None]), "not the standard frame of the same region");
        assert_eq!(
            out[2].found(),
            Err("screen capture failed: desktop duplication could not answer — it is still opening".to_string()),
            "no picture under fallback = \"none\" is could-not-look, with the reason, not a miss"
        );
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
        assert_eq!(first.found(), Err(SEARCH_PANICKED.to_string()), "a panicking batch could not look");
        task_tx.send(task(2, (0, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)).unwrap();
        let second = res_rx.recv_timeout(Duration::from_secs(10)).expect("the worker survived");
        assert_eq!(second.id, 2);
        assert_eq!(second.found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
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

    /// A cells task shares its batch with searches: it reads its own region through its own
    /// source, answers from the whole frame, and names why when it could not look.
    #[test]
    fn a_cells_task_is_answered_from_its_frame() {
        let region = (0, 0, 10, 10);
        let search = task(1, region, vec![(dot([255, 0, 0]), None)], Mode::First);
        let cells_here = cells_matching(2, region, None);
        let unresolved = cells_matching(3, (0, 0, 0, 0), Some("the window's client area is empty (0x0)"));
        let mut not_read = cells_matching(4, region, None);
        set_source(&mut not_read, CaptureSource::Duplication { or_standard: false });
        // Only the two frames that are read are asked for, once each: the unresolved task is
        // never captured.
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, String>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().map(|r| (*r, src))));
            by_source(regions, src)
        }
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<((i32, i32, i32, i32), CaptureSource)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let out = run_batch(&[search, cells_here, unresolved, not_read], record);
        assert_eq!(
            ASKED.with(|a| a.borrow().clone()),
            vec![(region, CaptureSource::Standard), (region, CaptureSource::Duplication { or_standard: false })]
        );
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        // The two dots on grey: only the red one passes, one pixel of the top-left 5x5 cell.
        let (live, ranked) = out[1].cells().as_ref().expect("read");
        assert_eq!(live, &vec![10, 0, 0, 0], "round(255 / 25)");
        assert_eq!(ranked.state, 0, "closer to all-zero than to all-255");
        assert_eq!(out[2].cells().as_ref().unwrap_err(), "the window's client area is empty (0x0)");
        assert!(out[3].cells().as_ref().unwrap_err().contains("desktop duplication could not answer — it is still opening"));
        // A capture the wrong size is named too.
        let short = |regions: &[(i32, i32, i32, i32)], _: CaptureSource| -> Vec<Result<CapturedImage, String>> {
            regions.iter().map(|_| Ok(opaque(3, 3, [0, 0, 0]))).collect()
        };
        let out = run_batch(&[cells_matching(5, region, None)], short);
        assert!(out[0].cells().as_ref().unwrap_err().contains("came back 3x3, not the region's 10x10"));
    }

    // ── snapshots ──────────────────────────────────────────────────────────────────────

    /// A screen with a red pixel at (103, 202) and a green one at (108, 205) on grey: what any
    /// capture of any region of it gives, and what a snapshot of it holds.
    fn scene(x: i32, y: i32, w: i32, h: i32) -> CapturedImage {
        let mut c = opaque(w as u32, h as u32, [50, 50, 50]);
        for (sx, sy, rgb) in [(103, 202, [255, 0, 0]), (108, 205, [0, 255, 0])] {
            if (x..x + w).contains(&sx) && (y..y + h).contains(&sy) {
                let i = (((sy - y) * w + (sx - x)) * 4) as usize;
                c.rgba[i..i + 3].copy_from_slice(&rgb);
            }
        }
        c
    }

    fn scene_capture(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Result<CapturedImage, String>> {
        regions.iter().map(|&(x, y, w, h)| Ok(scene(x, y, w, h))).collect()
    }

    fn no_capture(_: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Result<CapturedImage, String>> {
        panic!("a snapshot's task was captured")
    }

    /// A snapshot of the scene at (100, 200), 20x12.
    fn scene_frame() -> Arc<Frame> {
        let r = geo::Rect::new(100, 200, 20, 12);
        let f = Frame::from_image(r, scene(100, 200, 20, 12), Instant::now(), crate::backend::frame::FrameVia::Gdi, None);
        Arc::new(f.expect("a whole frame"))
    }

    fn on_frame(mut t: ImageTask, frame: &Arc<Frame>) -> ImageTask {
        let region = geo::Rect::from_tuple(t.region());
        t.hay = Haystack::Frame { frame: frame.clone(), region };
        t
    }

    /// The same search against a snapshot and against a live capture of the same region of the
    /// same screen finds the same hits at the same screen positions — for the whole snapshot and
    /// for parts of it, with `within`, in both modes.
    #[test]
    fn snapshot_and_live_haystack_same_hits() {
        let frame = scene_frame();
        let red = || vec![(dot([255, 0, 0]), None)];
        for region in [(100, 200, 20, 12), (102, 201, 10, 8), (103, 202, 1, 1), (104, 200, 16, 12)] {
            let live = run_batch(&[task(1, region, red(), Mode::First)], scene_capture);
            let snap = run_batch(&[on_frame(task(1, region, red(), Mode::First), &frame)], no_capture);
            assert_eq!(snap[0].found(), live[0].found(), "{region:?}");
            let entries = vec![
                (dot([255, 0, 0]), None),
                (dot([0, 255, 0]), None),
                (dot([0, 255, 0]), Some(within_frame(region, (106, 203, 200, 200)))),
                (dot([255, 0, 0]), Some(within_frame(region, (106, 203, 200, 200)))),
            ];
            let live = run_batch(&[task(2, region, entries.clone(), Mode::Each)], scene_capture);
            let snap = run_batch(&[on_frame(task(2, region, entries, Mode::Each), &frame)], no_capture);
            assert_eq!(snap[0].found(), live[0].found(), "{region:?}, each");
        }
        let whole = run_batch(&[on_frame(task(3, (100, 200, 20, 12), red(), Mode::First), &frame)], no_capture);
        assert_eq!(whole[0].found(), Ok(vec![Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 })]));
    }

    /// A snapshot's task never reaches the capture routine, costs no capture time, and does not
    /// take a frame from a screen task of the same region in its batch.
    #[test]
    fn a_frame_task_takes_no_capture() {
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<(i32, i32, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, String>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().copied()));
            scene_capture(regions, src)
        }
        let frame = scene_frame();
        let region = (100, 200, 20, 12);
        let red = || vec![(dot([255, 0, 0]), None)];
        let only = run_batch(&[on_frame(task(1, region, red(), Mode::First), &frame)], no_capture);
        assert_eq!(only[0].capture_ms, 0);
        let out = run_batch(&[on_frame(task(1, region, red(), Mode::First), &frame), task(2, region, red(), Mode::First)], record);
        assert_eq!(ASKED.with(|a| a.borrow().clone()), vec![region], "only the screen task is captured");
        assert_eq!(out[0].found(), out[1].found());
    }

    /// Hits on a snapshot are screen positions: the snapshot's corner plus where they lie in it,
    /// whatever part of it the region is.
    #[test]
    fn frame_task_hits_are_screen_coordinates() {
        let frame = scene_frame();
        let green = vec![(dot([0, 255, 0]), None)];
        let out = run_batch(&[on_frame(task(1, (105, 204, 5, 3), green, Mode::First), &frame)], no_capture);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 108, y: 205, w: 1, h: 1, n: 1 })]));
        // A region of the snapshot the dot is not in: looked, and not there.
        let out = run_batch(&[on_frame(task(2, (110, 200, 5, 3), vec![(dot([0, 255, 0]), None)], Mode::First), &frame)], no_capture);
        assert_eq!(out[0].found(), Ok(vec![None]));
    }

    /// A snapshot region that had nothing to read at the call — no overlap, or a minimised
    /// window — is answered with that reason on the worker, without a capture.
    #[test]
    fn a_frame_region_with_no_overlap_is_answered_without_capture() {
        let frame = scene_frame();
        let mut t = on_frame(task(1, (0, 0, 0, 0), vec![(dot([255, 0, 0]), None)], Mode::Each), &frame);
        let Job::Search { unresolved, .. } = &mut t.job else { unreachable!() };
        *unresolved = Some(snapshot::NO_OVERLAP.to_string());
        assert!(!t.captures());
        let out = run_batch(&[t], no_capture);
        assert_eq!(out[0].found(), Err(snapshot::NO_OVERLAP.to_string()));
    }

    /// A cells match on a snapshot reads the region where it lies in it: the same answer as a
    /// capture of that region.
    #[test]
    fn a_frame_cells_task_equals_the_live_one() {
        let frame = scene_frame();
        for region in [(100, 200, 20, 12), (101, 201, 5, 5)] {
            let live = run_batch(&[cells_matching(1, region, None)], scene_capture);
            let snap = run_batch(&[on_frame(cells_matching(1, region, None), &frame)], no_capture);
            assert_eq!(snap[0].cells(), live[0].cells(), "{region:?}");
            assert!(snap[0].cells().is_ok());
        }
    }

    /// A search on a snapshot held while its module was disabled is sent again as it was — the
    /// same picture, which gives the same answer however long the module was off.
    #[test]
    fn a_held_frame_search_is_resent_as_is() {
        let lua = Lua::new();
        let _ = lua.try_set_app_data(VmOwner { idx: 2, gen: 6 });
        let gens: HashMap<usize, u64> = [(2, 6)].into_iter().collect();
        let frame = scene_frame();
        let cb: Function = lua.load("return function() end").eval().unwrap();
        let t = on_frame(task(30, (100, 200, 20, 12), vec![(dot([255, 0, 0]), None)], Mode::First), &frame);
        let mut p = pending(&lua, &gens, 2, cb, vec![None], t).unwrap();
        p.held = true;
        let mut map: HashMap<u64, PendingImage> = [(30, p)].into_iter().collect();
        let (again, stale) = unhold(&mut map, 2, Some(6));
        assert!(stale.is_empty());
        let Haystack::Frame { frame: f, region } = &again[0].hay else { panic!("sent again as a screen search") };
        assert!(Arc::ptr_eq(f, &frame), "the same picture");
        assert_eq!(*region, geo::Rect::new(100, 200, 20, 12));
        release(map.into_values());
    }

    /// `template{ capture, snapshot }` cuts the template out of the snapshot — whole or not at
    /// all — without capturing, with the snapshot's edges as the defaults of the corners.
    #[test]
    fn template_capture_from_a_snapshot_is_inside_or_nil() {
        let lua = Lua::new();
        lua.globals().set("s", snapshot::test_handle(&lua, snapshot::test_frame(100, 50, 40, 30))).unwrap();
        let cut = |src: &str| {
            let s = spec(&lua, src).unwrap();
            build_handle(&lua, s, no_file, |_, _, _, _| panic!("the screen was captured")).unwrap().into_vec()
        };
        let got = cut("return { capture = { 110, 60, 114, 63 }, snapshot = s, name = 'cut' }");
        let Value::UserData(ud) = &got[0] else { panic!("expected a handle") };
        {
            let h = ud.borrow::<TemplateHandle>().unwrap();
            assert_eq!((h.dec.w, h.dec.h, h.name.as_deref()), (4, 3, Some("cut")));
            let template::Body::Dense { rgba, .. } = &h.dec.body;
            assert_eq!(&rgba[0..4], &[110, 60, 50, 255], "the snapshot's pixel at screen (110, 60)");
        }
        let held = budget(&lua).get();
        assert_eq!(held, template::dense_bytes(4, 3));
        let past = cut("return { capture = { 130, 60, 150, 70 }, snapshot = s }");
        assert!(past[0].is_nil());
        assert_eq!(past[1].as_string().unwrap().to_string_lossy(), snapshot::NOT_INSIDE, "not cut to fit: refused");
        assert_eq!(budget(&lua).get(), held, "and its reservation given back");
        let edges = cut("return { capture = { x1 = 120, y1 = 70 }, snapshot = s }");
        let Value::UserData(ud) = &edges[0] else { panic!("expected a handle") };
        let h = ud.borrow::<TemplateHandle>().unwrap();
        assert_eq!((h.dec.w, h.dec.h), (20, 10), "to the snapshot's right and bottom edges");
    }

    #[test]
    fn snapshot_is_only_allowed_with_capture() {
        let lua = Lua::new();
        lua.globals().set("s", snapshot::test_handle(&lua, snapshot::test_frame(0, 0, 4, 4))).unwrap();
        let e = err_text(spec(&lua, "return { file = 'a.png', snapshot = s }"));
        assert!(e.contains("host.screen.template: snapshot belongs with capture"), "{e}");
        let e = err_text(spec(&lua, "return { rgb = 'abc', w = 1, h = 1, snapshot = s }"));
        assert!(e.contains("snapshot belongs with capture"), "{e}");
        let e = err_text(spec(&lua, "return { capture = { 0, 0, 1, 1 }, snapshot = 5 }"));
        assert!(e.contains("host.screen.template: snapshot must be a Snapshot, got 5"), "{e}");
        lua.load("s:release()").exec().unwrap();
        let e = err_text(spec(&lua, "return { capture = { 0, 0, 1, 1 }, snapshot = s }"));
        assert!(e.contains("host.screen.template: snapshot: the snapshot was released"), "{e}");
        let e = err_text(spec(&lua, "return { capture = { 0, 0, 1, 1 }, snapshots = 1 }"));
        assert!(e.contains("unknown field 'snapshots'"), "{e}");
    }

    /// A search whose window region had nothing to read at the call (a minimised window) is
    /// answered on the worker with that reason and never captured — not a 0x0 capture, and
    /// not a "could not look" of some other kind — while the search beside it reads as usual.
    #[test]
    fn a_search_with_nothing_to_read_is_answered_without_a_capture() {
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, String>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().copied()));
            two_dots(regions, src)
        }
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<(i32, i32, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let region = (0, 0, 10, 10);
        let red = || vec![(dot([255, 0, 0]), None)];
        let empty = "the window's client area is empty (0x0)";
        let mut first = task(2, (0, 0, 0, 0), red(), Mode::First);
        let mut each = task(3, (0, 0, 0, 0), red(), Mode::Each);
        for t in [&mut first, &mut each] {
            let Job::Search { unresolved, .. } = &mut t.job else { unreachable!() };
            *unresolved = Some(empty.to_string());
            assert!(!t.captures());
        }
        let out = run_batch(&[task(1, region, red(), Mode::First), first, each], record);
        assert_eq!(ASKED.with(|a| a.borrow().clone()), vec![region], "only the search that has a region is captured");
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        assert_eq!(out[1].found(), Err(empty.to_string()));
        assert_eq!(out[2].found(), Err(empty.to_string()), "could not look, not a list of false");
    }

    /// The callback's arguments: `(match, nil)`, or `(nil, reason)`.
    #[test]
    fn a_cells_answer_is_a_table_or_a_reason() {
        let lua = Lua::new();
        let t = cells_matching(1, (5, 6, 10, 10), None);
        let names = vec![Some("off".to_string()), Some("on".to_string())];
        let live = vec![0u8; 4];
        let ranked = cells::rank(&live, &[vec![0u8; 4].into(), vec![255u8; 4].into()], &[0, 1]).unwrap();
        let args = result_args(&lua, &t, &names, Outcome::Cells(Ok((live, ranked)))).into_vec();
        let Value::Table(m) = &args[0] else { panic!("expected a match") };
        assert!(args[1].is_nil());
        assert_eq!(m.get::<String>("cells").unwrap(), "00000000");
        assert_eq!(m.get::<String>("name").unwrap(), "off");
        assert_eq!((m.get::<i32>("x").unwrap(), m.get::<i32>("y").unwrap()), (5, 6));
        let args = result_args(&lua, &t, &names, Outcome::Cells(Err("the screen could not be read".into()))).into_vec();
        assert!(args[0].is_nil());
        let Value::String(why) = &args[1] else { panic!("expected a reason") };
        assert_eq!(why.to_string_lossy(), "the screen could not be read");
        assert_eq!(t.binding(), "matchCellsAsync");
        // A search's callback still gets exactly one value when it looked, and the reason
        // beside a nil when it could not.
        let s = task(2, (0, 0, 10, 10), vec![(dot([1, 2, 3]), None)], Mode::First);
        assert_eq!(result_args(&lua, &s, &[None], Outcome::Search(Ok(vec![None]))).len(), 1);
        let args = result_args(&lua, &s, &[None], Outcome::Search(Err("screen capture failed".into()))).into_vec();
        assert!(args[0].is_nil());
        assert_eq!(args[1].as_string().map(|s| s.to_string_lossy()).as_deref(), Some("screen capture failed"));
    }

    /// The reasons that are the host's own rather than the capture path's are listed in
    /// docs/api/screen.md as they are written here (the capture path's are held to the page by
    /// `every_reason_a_module_is_told_is_listed` in backend/windows.rs).
    ///
    /// A reason with numbers in it is made by the code that makes it, with numbers the test
    /// picks, and each of those numbers is then written `…` as the page writes it; a limit the
    /// page states as itself (4096) is left alone. The cells calls' window-size reason is held
    /// to the page where its own test makes it (lib.rs, `cells_binding_tests`).
    #[test]
    fn the_hosts_own_reasons_are_listed() {
        const DOC: &str = include_str!("../../../docs/api/screen.md");
        // `msg` with each of `numbers`, in order, written as the page writes a number: `…`.
        let listed = |msg: &str, numbers: &[&str]| {
            numbers.iter().fold(msg.to_string(), |m, n| {
                assert!(m.contains(n), "`{n}` is not in `{m}`");
                m.replacen(n, "…", 1)
            })
        };
        let spec = crate::cells::CellSpec::new(2, 2, crate::cells::Predicate::parse("r >= 1").unwrap()).unwrap();
        let four = CapturedImage { w: 4, h: 4, rgba: vec![0; 64] };
        let size = |w, h| format!("at the window's current size {}", template::check_size(w, h).unwrap_err());
        for reason in [
            crate::backend::CAPTURE_FAILED.to_string(),
            SHORT_CAPTURE.to_string(),
            HELD_CELLS.to_string(),
            SEARCH_PANICKED.to_string(),
            listed(&region::Unresolved::EmptyClient { w: 0, h: 7 }.to_string(), &["0", "7"]),
            region::Unresolved::OutOfRange.to_string(),
            listed(&format!("{} (0x20)", crate::EMPTY_REGION), &["0", "20"]),
            listed(&crate::cells::of_capture(&four, 4, 5, &spec).unwrap_err(), &["4", "4", "4", "5"]),
            listed(&size(5000, 10), &["5000", "10"]),
            listed(&size(2000, 1000), &["2000", "1000", "2000000"]),
        ] {
            assert!(DOC.contains(&format!("`{reason}`")), "docs/api/screen.md does not list the reason `{reason}`");
        }
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
        assert!(e.contains("expected a path or a Template, got a number"), "{e}");
        // A mixed list resolves completely, in order, before anything is captured.
        let list: Table = lua.load("return { 'a.png', 'b.png' }").eval().unwrap();
        assert_eq!(resolve_list(&list, "imageSearchMulti", &load).unwrap().len(), 2);
        let bad: Table = lua.load("return { 'a.png', true }").eval().unwrap();
        assert!(resolve_list(&bad, "imageSearchMulti", &load).is_err());
    }
}
