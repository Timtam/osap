//! Image search as a module sees it: template handles, the search bindings, the worker that
//! captures and matches off the event loop, and the delivery of its answers. The worker also
//! reduces the pictures of `host.screen.profile` (`Job::Profile`), whose requests and answers are
//! snapshot.rs's.
//!
//! **The calls that wait for it.** In a handler, `host.screen.cells`, `matchCells`,
//! `template{ capture }`, a snapshot's `crop`, the searches `imageSearch` and `imageSearchAll`, and
//! `save` and `saveMarked` are tasks of this worker, and the handler waits for the answer
//! (`task.rs`): `cells` and `matchCells` read and reduce their region (`Job::Cells`, with or without
//! the ranking), `template{ capture }` captures — or cuts from its snapshot — and prepares its
//! picture (`Job::Template`), `crop` copies its part of the snapshot (`Job::Crop`), a search captures
//! and matches (`Job::Search`, every match for `imageSearchAll`: `Mode::All`), and a shot captures —
//! or cuts from its snapshot — marks, encodes and writes its picture (`Job::Save`, save.rs). Their
//! answers are built on the event loop as the handler goes on, by the same functions that build a
//! callback's ([`cells_values`], [`template_values`], [`search_values`], `save::save_values`).
//! Where such a call cannot wait it raises (`task::wait_message`), unless there is nothing to read
//! this time, which is answered at once there too ([`now_plan`]).
//!
//! **The same task with a callback.** Given a callback — `imageSearch(template, opts, cb)`,
//! `matchCells(opts, states, cb)`, and `imageSearchEach`, which has no other form — the call is the
//! same task with the callback as its waiter ([`ImageWaiter::Callback`]): it never waits, and its
//! answer — what the call returns, built by the same function — is handed to the module's mailbox as
//! a handler, then `info` with the moment of the picture read for it ([`ImageResult::seen`]). It is
//! dropped when its module is disabled, reloaded, removed or stopped first, as a read's is; a newer
//! one of its module with the same `opts.key`, of the same call, ends it, and `host.screen.pending`
//! asks about it ([`ImageState::waiting_with`]). An answer known at the call — nothing to read this
//! time — comes on the next tick, as every callback's does ([`submit_now`]).
//!
//! **Who goes first.** The worker takes, of the tasks waiting for it, those of the highest priority
//! as one batch ([`next_batch`]): a key's wait is not batched with the library polls queued before
//! it, and goes ahead of them.
//!
//! **The templates are the worker's too.** A search hands it its templates as needles — a path
//! checked at the call, a handle's picture — and the worker opens the files and makes the pictures
//! before it matches the batch (`template_store.rs`); a handle from bytes or a file has its
//! picture made there by a task of its own (`Job::Prepare`), queued when the handle is made, which
//! holds it weakly: a handle let go before the worker gets to it is gone with its bytes, and its
//! task makes nothing.
//!
//! The matcher is `template.rs`; this file is everything around it that knows about Lua, about
//! modules, or about threads. It lives outside `lib.rs` so that the part of the host every
//! feature touches stays small, and so that the pieces with rules of their own — who a result
//! belongs to, what a module may hold, what happens when a search panics — can be read, and
//! tested, in one place.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use mlua::{
    Function, Lua, MetaMethod, MultiValue, RegistryKey, Table, UserData, UserDataFields,
    UserDataMethods, Value,
};
use rayon::prelude::*;

use crate::backend::frame::Frame;
use crate::backend::{CaptureFn, CaptureSource, CapturedImage, CompareFn, CompareReq};
use crate::vm_guard::host_call;
use crate::capture_source;
use crate::cells;
use crate::errors::{screen, HostError, Reason};
use crate::ocr::lua::ReadHost;
use crate::ocr::policy::IMAGE_PER_OWNER;
use crate::profile;
use crate::ocr::types::{self as geo, Priority};
use crate::region::{self, ScreenRect};
use crate::snapshot;
use crate::template::{self, Decoded, Rect};
use crate::template_store::{self, Needle, Prepared, Raw};
use crate::mailbox::{self, Event, MailHost, Opened};
use crate::task::{Ctx, On, Started, TaskId};
use crate::{
    cells_match_table, cells_table, logging, one_value, read_scales, region_arg, region_lua, short_capture, with_reason,
    Shared,
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
/// in-flight flag stayed set for good: the poll kept firing and never searched again. A gate asks
/// `host.screen.pending(key)` now, which a dropped answer leaves `false`, so it asks again.
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
/// which no VM ever has (`NEXT_GEN` starts at 1), so the mailbox drops the answer: there is no
/// VM the host knows of to hand it to.
fn owner_of(lua: &Lua, gens: &HashMap<usize, u64>, scope: usize) -> (usize, u64) {
    if let Some(o) = vm_owner(lua) {
        return (o.idx, o.gen);
    }
    (scope, gens.get(&scope).copied().unwrap_or(0))
}

/// Whether the worker answers with the first entry that matched, with every entry, or with every
/// match of its one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    First,
    Each,
    /// `imageSearchAll`: every match of the one template, at its own size.
    All,
}

/// What a task's waiter goes on with: what the call returns, built in the VM from the worker's
/// outcome.
pub(crate) type BuildAnswer = Box<dyn FnOnce(&Lua, Outcome) -> mlua::Result<MultiValue>>;

/// Who waits for a task's answer.
pub(crate) enum ImageWaiter {
    /// A callback, handed its answer through its module's mailbox: `imageSearchEach`'s, and the
    /// callback form of the calls that wait.
    Callback {
        cb: RegistryKey,
        /// The call, as a stop of its VM while the answer is built names it, and as its error and
        /// its keys are told apart: `screen.imageSearch`.
        call: &'static str,
        /// Builds what the call returns, then handed to `cb` before `info`. Called only for an
        /// answer that runs; it holds what that answer needs and the worker does not carry — a
        /// charge reserved at the call, the states' and the templates' names.
        answer: BuildAnswer,
    },
    /// A handler stopped in a `host.screen` call that waits for the worker (`task.rs`).
    Handler {
        task: TaskId,
        /// The call, as a stop of its VM while the answer is built names it: `screen.cells`.
        call: &'static str,
        /// Builds the answer; called only for a handler found still waiting for it. It holds what
        /// that answer needs and the worker does not carry — a charge reserved at the call, the
        /// states' names — and gives a charge back when it is dropped without being called.
        answer: BuildAnswer,
    },
}

/// What a task does, as an answer decided without the worker needs it: one of its own kind
/// ([`Outcome::failing`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobKind {
    Search,
    Cells,
    Profile,
    Crop,
    Template,
    Save,
    Prepare,
}

/// A task waiting for its answer: a callback's, or a handler's wait. Main thread only.
pub(crate) struct PendingImage {
    lua: Lua,
    waiter: ImageWaiter,
    /// The identity the call was made under — a code dependency's own, for its code. Named in
    /// an error report, because that is the code that raised.
    scope: usize,
    /// Whose VM the callback runs in, and which incarnation of it; see `VmOwner`.
    owner: usize,
    gen: u64,
    /// The task's number, which the worker answers with.
    id: u64,
    /// What the task does: what an answer decided without the worker is made of.
    kind: JobKind,
    /// A callback's `opts.key`: a newer task of its module VM with the same key, of the same call,
    /// ends it ([`ImageState::supersede`]), and `host.screen.pending` asks about it.
    key: Option<String>,
    /// Handed to its module's mailbox, and counted there for `host.screen.pending` until it runs
    /// or is dropped ([`ImageState::hand`]).
    handed: bool,
    /// The priority of the dispatch that asked, which the callback runs under: a key press, then
    /// a search, then a text read stays the user waiting (`ocr::types::Priority`).
    prio: Priority,
}

impl PendingImage {
    /// The call that asked, as its keys are told apart: a key of `imageSearch` never ends a
    /// `matchCells`.
    fn asked_as(&self) -> &'static str {
        match &self.waiter {
            ImageWaiter::Callback { call, .. } | ImageWaiter::Handler { call, .. } => call,
        }
    }
}

/// What a search or cells read is answered when a newer one of its module pushed it past
/// [`IMAGE_PER_OWNER`]: `nil, err` (`too_many`), on the next tick.
pub(crate) fn too_many_waiting() -> HostError {
    HostError::with(
        &screen::IMAGE_TOO_MANY,
        format!("too many image searches waiting for this module ({IMAGE_PER_OWNER}); the oldest was ended"),
    )
}

/// The request of `owner`'s VM that a newer one pushes past `cap`: its oldest, when it has more
/// than `cap` waiting. The newest always proceeds: it is what the module asked for last. A
/// handler's wait is neither counted nor ended: a handler waits for one answer at a time, so its
/// waits cannot pile up, and the one it waits for is a call the module is in the middle of. Pure,
/// for the tests.
fn oldest_over_cap(map: &HashMap<u64, PendingImage>, owner: usize, cap: usize) -> Option<u64> {
    let mine: Vec<u64> = map
        .iter()
        .filter(|(_, p)| p.owner == owner && matches!(p.waiter, ImageWaiter::Callback { .. }))
        .map(|(id, _)| *id)
        .collect();
    (mine.len() > cap).then(|| mine.into_iter().min()).flatten()
}

/// The ids of tasks ended before the worker reached them ([`oldest_over_cap`]): the worker skips
/// them rather than capture and match for an answer nobody reads. Each id leaves again — taken by
/// the worker when it skips it, or by the event loop when the worker had answered it already —
/// so the set holds only what is on its way.
static ENDED: Mutex<Option<HashSet<u64>>> = Mutex::new(None);

fn ended() -> std::sync::MutexGuard<'static, Option<HashSet<u64>>> {
    ENDED.lock().unwrap_or_else(|p| p.into_inner())
}

/// Task `id`, sent and no longer wanted: the worker skips it if it has not begun it, and its
/// answer is let go if it has. For the profiles' reductions, whose requests snapshot.rs ends.
pub(crate) fn end_task(id: u64) {
    ended().get_or_insert_with(HashSet::new).insert(id);
}

/// Whether task `id` was ended and its answer is still on its way: a shot asks before it writes
/// (`save_task`). Never the event loop's own task, numbered 0.
fn is_ended(id: u64) -> bool {
    ended().as_ref().is_some_and(|s| s.contains(&id))
}

/// Takes every entry owned by module `owner`'s VM out of the map. Matched on the OWNER, never
/// on the scope the call was made under.
fn purge_owner(map: &mut HashMap<u64, PendingImage>, owner: usize) -> Vec<PendingImage> {
    let ids: Vec<u64> = map.iter().filter(|(_, p)| p.owner == owner).map(|(id, _)| *id).collect();
    ids.into_iter().filter_map(|id| map.remove(&id)).collect()
}

/// Frees what finished or abandoned entries hold: a callback in its VM's registry, and what an
/// answer held — a charge it gives back.
fn release(gone: impl IntoIterator<Item = PendingImage>) {
    for p in gone {
        if let ImageWaiter::Callback { cb, .. } = p.waiter {
            let _ = p.lua.remove_registry_value(cb);
        }
    }
}

// ── The worker ─────────────────────────────────────────────────────────────────────────

/// A queued task: a search, a cells read, a profile's reduction, a crop, a template, a shot, or a
/// handle's preparation. For a search the worker CAPTURES its region itself (off the main thread)
/// then matches `entries` against it — or, given a snapshot, reads the snapshot and captures
/// nothing. The region is (x, y, w, h) in screen coords, whose (x, y) is added back into every
/// reported hit.
///
/// In `Mode::First` the entries are tried in order against the SAME captured frame and the
/// first hit wins — several renderings of the same thing (a dialog's close glyph as two plugin
/// versions draw it). Chaining single-template searches instead would pay a fresh region
/// capture (~1 compositor frame) per template. In `Mode::Each` every entry is answered, each
/// within its own sub-rectangle of that one frame. In `Mode::All` every match of the one entry
/// is.
#[derive(Clone)]
pub(crate) struct ImageTask {
    pub(crate) id: u64,
    /// What the task reads.
    pub(crate) hay: Haystack,
    /// What is done with the frame.
    pub(crate) job: Job,
    /// The priority of the dispatch that asked — of the wait, for a handler's: the worker takes
    /// the highest waiting first ([`next_batch`]).
    pub(crate) prio: Priority,
}

/// What a task reads.
#[derive(Clone)]
pub(crate) enum Haystack {
    /// The screen, captured by the worker: every task there was before snapshots.
    Screen {
        region: (i32, i32, i32, i32),
        /// How the asking module's VM reads the screen (`capture_source.rs`). Part of the
        /// frame's identity in a batch: the same region read two ways is two frames.
        source: CaptureSource,
        /// The module's first-read comparison, when this task carries it: the worker makes it
        /// before the batch's captures (`capture_source::take_compare`).
        compare: Option<CompareReq>,
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
        /// of it), in frame coordinates. Opened, or made, by the worker before it matches
        /// (`template_store::resolve`).
        entries: Vec<(Needle, Option<Rect>)>,
        tol: u8,
        scales: Vec<f32>,
        mode: Mode,
        /// Why there is nothing to read this time — a window region whose client area was
        /// empty at the call. Answered on the worker without a capture, as a cells read is, so
        /// the callback still comes on a later tick.
        unresolved: Option<HostError>,
    },
    /// `cells` and `matchCells`, waited for or given a callback: the whole frame reduced to cells,
    /// and ranked against the states unless it is `cells`'. A cells task shares its capture with
    /// every search of the same region and source in its batch.
    Cells(Arc<CellsJob>),
    /// `host.screen.profile`: the part of a picture the task's region covers, reduced per column
    /// and row (`profile.rs`). Always a picture already taken — the `screen-capture` thread's, or
    /// a snapshot the module passed — so the worker never captures for it. Its request waits in
    /// snapshot.rs (`SnapState`), not here, and is answered there.
    Profile { cols: bool, rows: bool, dark: u8 },
    /// A snapshot's `crop` a handler waits for: the part of its picture the task's region covers,
    /// copied into a picture of its own. Always a snapshot's (`Haystack::Frame`), never captured.
    Crop,
    /// `host.screen.template{ capture }` a handler waits for: the region captured — or cut, whole,
    /// from the snapshot it carries — and made a template.
    Template,
    /// `host.screen.save` or `saveMarked` a handler waits for: the region captured — or the part of
    /// it inside the snapshot it carries, copied — marked, encoded and written (save.rs).
    Save(Arc<crate::save::SaveJob>),
    /// A `host.screen.template` handle's picture, made from the bytes or the file it was given,
    /// queued as the handle is made: nobody waits for it, and nothing is read for it. A search that
    /// needs the picture first makes it itself.
    ///
    /// Held weakly. The handle's charge on its VM's budget goes with the handle, and so must the
    /// bytes: a module that builds a template on every poll and lets it go would otherwise leave
    /// its bytes queued here past the budget, for pictures nobody can use, ahead of every other
    /// module's reads. So the bytes live only as long as the handle or a search that holds them,
    /// and a task whose handle is gone makes nothing.
    Prepare(Weak<Prepared>),
}

/// A cells call's work: `cells`' or `matchCells`'.
pub(crate) struct CellsJob {
    /// For `cells` it has no states, and only its grid is used.
    pub(crate) matcher: Arc<cells::Matcher>,
    /// Why there is nothing to read this time — the window's client area was empty when the
    /// call was made. Answered on the worker, without a capture, so that the callback still
    /// comes on a later tick like every other answer.
    pub(crate) unresolved: Option<HostError>,
    /// Ranked against the states (`matchCells`), or the grid alone (`cells`).
    pub(crate) rank: bool,
}

impl ImageTask {
    /// Whether the worker reads the screen for this task: never for a snapshot's. The batch asks
    /// `screen_key`; the tests ask this.
    #[cfg(test)]
    fn captures(&self) -> bool {
        self.screen_key().is_some()
    }

    /// The frame the worker captures for this task — its region and source — or `None` when
    /// it captures nothing: a snapshot's task, one answered with why there is nothing to read, and
    /// a handle's preparation.
    fn screen_key(&self) -> Option<capture_source::FrameKey> {
        let unresolved = match &self.job {
            Job::Search { unresolved, .. } => unresolved.is_some(),
            Job::Cells(c) => c.unresolved.is_some(),
            Job::Prepare(_) => true,
            Job::Profile { .. } | Job::Crop | Job::Template | Job::Save(_) => false,
        };
        match &self.hay {
            Haystack::Screen { region, source, .. } if !unresolved => Some((*region, *source)),
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

}

impl Job {
    /// What it does, as an answer decided without the worker needs it.
    pub(crate) fn kind(&self) -> JobKind {
        match self {
            Job::Search { .. } => JobKind::Search,
            Job::Cells(_) => JobKind::Cells,
            Job::Profile { .. } => JobKind::Profile,
            Job::Crop => JobKind::Crop,
            Job::Template => JobKind::Template,
            Job::Save(_) => JobKind::Save,
            Job::Prepare(_) => JobKind::Prepare,
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
    /// `Mode::Each`, exactly one slot, the first hit or none, in `Mode::First`, and one slot per
    /// match, each a hit, in `Mode::All`.
    Search(Result<Vec<Option<ScreenHit>>, HostError>),
    /// A cells read: the live cells and, for a match, their ranking — or why there are none.
    Cells(Result<(Vec<u8>, Option<cells::Ranked>), HostError>),
    /// A profile's reduction, or why there is none.
    Profile(Result<Box<Reduced>, HostError>),
    /// A snapshot's part, copied — or why there is none.
    Crop(Result<Frame, HostError>),
    /// A template's picture, prepared, and how many of its pixels a search compares — counted on
    /// the worker, so the event loop builds only the handle — or why there is none.
    Template(Result<(Arc<Decoded>, u32), HostError>),
    /// A shot written, or why not: no picture to write, or a file that could not be written.
    Save(Result<(), HostError>),
    /// A handle's picture made — or why it could not be, which the handle keeps — for nobody.
    Prepared,
    /// Never the worker's: a callback's answer decided at the call, which its waiter holds
    /// ([`submit_now`]).
    AtCall,
}

/// A profile's reduction, as the worker hands it back: plain data, the Lua tables are built when
/// the callback runs.
pub(crate) struct Reduced {
    pub(crate) columns: Option<profile::Axis>,
    pub(crate) rows: Option<profile::Axis>,
    /// What was reduced, on screen.
    pub(crate) rect: geo::Rect,
    /// How long the reduction took on the worker, for the observation line.
    pub(crate) took: Duration,
}

impl Outcome {
    /// The answer of a task whose batch panicked: could not look.
    fn failed(t: &ImageTask) -> Outcome {
        let kind = t.job.kind();
        Outcome::failing(kind, failed(kind))
    }

    /// The answer of a task of `kind` that could not look, for `why`.
    pub(crate) fn failing(kind: JobKind, why: HostError) -> Outcome {
        match kind {
            JobKind::Search => Outcome::Search(Err(why)),
            JobKind::Cells => Outcome::Cells(Err(why)),
            JobKind::Profile => Outcome::Profile(Err(why)),
            JobKind::Crop => Outcome::Crop(Err(why)),
            JobKind::Template => Outcome::Template(Err(why)),
            JobKind::Save => Outcome::Save(Err(why)),
            // The handle is left to the first search that needs its picture.
            JobKind::Prepare => Outcome::Prepared,
        }
    }
}

/// The reason a task of `kind` gets when the host's own code failed it — its work or its batch
/// panicked, or the worker was gone when it was sent: `internal error: the search could not be
/// made`, and the like, each `failed`.
fn failed_reason(kind: JobKind) -> &'static Reason {
    match kind {
        JobKind::Search => &screen::SEARCH_FAILED,
        JobKind::Cells => &screen::CELLS_FAILED,
        JobKind::Profile => &screen::PROFILE_FAILED,
        JobKind::Crop => &screen::CROP_FAILED,
        JobKind::Template | JobKind::Prepare => &screen::TEMPLATE_FAILED,
        JobKind::Save => &screen::SAVE_FAILED,
    }
}

/// [`failed_reason`]'s error.
fn failed(kind: JobKind) -> HostError {
    HostError::of(failed_reason(kind))
}

/// When a task's picture was taken — `host.now()` milliseconds — and `host.inputEpoch()` once it
/// had been: what a callback's `info` says.
pub(crate) type Moment = crate::snapshot::Moment;

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
    /// The moment of the picture the task read: its snapshot's, or when the worker's capture of
    /// its region came back and the input epoch then. `None` for a task that read no picture — one
    /// answered with why there is nothing to read, a search whose templates could not be opened, a
    /// capture that failed.
    pub(crate) seen: Option<Moment>,
}

/// The image-search worker: CAPTURES each task's region and runs the CPU-heavy
/// template match, both off the main thread, so neither the ~1-frame capture nor the
/// match blocks the event loop. `capture` is the backend's stateless capture routine
/// (a plain `fn` pointer, hence `Send`), which reads each region through the source its
/// task carries. `input_epoch` is `host.inputEpoch()` as the event loop last turned it over
/// (`ImageState::note_input_epoch`), read as each capture comes back. Exits when the task sender
/// is dropped (app teardown). Touches only owned data — safe to leave running across process exit.
///
/// FAN-OUT SHARING: after the first task, it briefly collects any others queued in the
/// same tick (several libraries' landmark polls fire together, staggered by a few ms)
/// into one batch, then CAPTURES EACH DISTINCT REGION ONCE and matches every task's
/// template against its region's shared frame. So N libraries polling the same plugin
/// region cost 1 capture per tick, not N — "one frame, many comparisons" over
/// simultaneous consumers. Landmark polling isn't latency-critical, so the tiny
/// collection wait is invisible; results are byte-for-byte what per-task capture gave.
///
/// A KEY GOES FIRST. A batch is only the tasks of the highest priority waiting
/// ([`next_batch`]); the others wait for the next one, and whatever came meanwhile is
/// weighed with them. So a handler that waits for a key — a cells match, a profile — is
/// one batch of its own after the one in progress, not one more task in the polls' batch.
///
/// PANICS ARE ANSWERED, NOT FATAL. This thread used to have no `catch_unwind`, so one panic —
/// in a capture, in a resize, in a scan — ended it, every later `send` failed, and every
/// callback in the session silently never fired again. Worse than an error, because nothing
/// reported it: the overlay runtime's landmark gate waits for its callback before it searches
/// again, so its overlays simply stopped appearing. Now a panicking entry is answered as "no
/// match", a panicking batch as "could not look", and the thread carries on.
///
/// Below normal priority, as the matching pool is ([`below_normal_priority`]).
pub(crate) fn spawn_image_worker(
    capture: CaptureFn,
    compare: CompareFn,
    input_epoch: Arc<AtomicU64>,
    tasks: Receiver<ImageTask>,
    results: Sender<ImageResult>,
) {
    let spawned = std::thread::Builder::new().name("image-worker".to_string()).spawn(move || {
        below_normal_priority();
        worker_loop(capture, compare, &input_epoch, tasks, results)
    });
    if let Err(e) = spawned {
        // Every task sent is then refused, and answered as the host's own failure.
        logging::line("image", &format!("the image worker could not be started ({e}); every search and screen read it does fails"));
    }
}

/// Lowers the calling thread below normal priority, on Windows (`THREAD_PRIORITY_BELOW_NORMAL`);
/// nothing elsewhere.
///
/// For the image worker and the matching pool, the application's heaviest work: while an overlay
/// is in front its matches keep every logical processor busy several times a second ("batch of 16
/// read(s) took 216 ms … across 16 thread-pool tasks"). At normal priority they compete with every
/// other program's normal-priority threads for those processors — among them another program's
/// low-level keyboard hook, which Windows calls for every key on the machine and waits for. Below
/// normal they take what the others leave: as fast as before on a machine that is otherwise idle,
/// behind everybody else's work on one that is not. The keyboard hook's own thread runs at the
/// highest normal priority and is none of these.
pub(crate) fn below_normal_priority() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };
        // SAFETY: the calling thread's pseudo-handle, which needs no closing.
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
    }
}

/// The matching pool as it is built: rayon's pool, one thread per logical processor, each
/// lowered below normal priority as it starts ([`below_normal_priority`]).
fn matching_pool() -> rayon::ThreadPoolBuilder {
    rayon::ThreadPoolBuilder::new()
        .thread_name(|i| format!("image-match-{i}"))
        .start_handler(|_| below_normal_priority())
}

/// Builds rayon's global pool, where the matches run ([`matching_pool`]): at start-up, before the
/// image worker exists, so before anything matches. A pool already built — by a dependency that
/// matched in parallel first — keeps its threads at normal priority, and the log says so.
pub(crate) fn build_matching_pool() {
    if let Err(e) = matching_pool().build_global() {
        logging::line(
            "image",
            &format!(
                "the image matching pool could not be built ({e}); the matches run on the pool \
                 that was there, at normal priority"
            ),
        );
    }
}

fn worker_loop(
    capture: CaptureFn,
    compare: CompareFn,
    input_epoch: &AtomicU64,
    tasks: Receiver<ImageTask>,
    results: Sender<ImageResult>,
) {
    let batch_window = Duration::from_millis(5);
    // The tasks that came and were not taken yet: those of a lower priority than the batch that
    // went before them, in the order they came.
    let mut waiting: Vec<ImageTask> = Vec::new();
    loop {
        if waiting.is_empty() {
            // Collect this tick's tasks: the first, plus any queued within a short window
            // (same-tick polls arrive microseconds-to-ms apart).
            match tasks.recv() {
                Ok(first) => waiting.push(first),
                Err(_) => return, // app teardown
            }
            let deadline = Instant::now() + batch_window;
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match tasks.recv_timeout(deadline - now) {
                    Ok(t) => waiting.push(t),
                    Err(_) => break, // window elapsed (batch complete) or sender dropped
                }
            }
        } else {
            // What came while the last batch ran, weighed with what it left, without waiting.
            while let Ok(t) = tasks.try_recv() {
                waiting.push(t);
            }
        }
        // Ended while they waited here: already answered on the event loop's side.
        {
            let mut ended = ended();
            if let Some(set) = ended.as_mut().filter(|s| !s.is_empty()) {
                waiting.retain(|t| !set.remove(&t.id));
            }
        }
        let batch = next_batch(&mut waiting);
        if batch.is_empty() {
            continue;
        }
        // The first-read comparisons the tasks carry, before the batch's captures.
        for t in &batch {
            if let Haystack::Screen { compare: Some(c), .. } = &t.hay {
                c.make(compare);
            }
        }
        let out = match logging::contain(|| run_batch(&batch, capture, input_epoch)) {
            Ok(out) => out,
            Err(report) => {
                contained("a batch", "every search, cells match and profile in it was answered nil", &report);
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
                        seen: None,
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

/// Of the tasks `waiting`, the next batch: every one of the highest priority among them, in the
/// order they came. The others stay, in their order, for a later batch.
fn next_batch(waiting: &mut Vec<ImageTask>) -> Vec<ImageTask> {
    let Some(top) = waiting.iter().map(|t| t.prio).max() else { return Vec::new() };
    let (batch, rest): (Vec<ImageTask>, Vec<ImageTask>) = std::mem::take(waiting).into_iter().partition(|t| t.prio == top);
    *waiting = rest;
    batch
}

/// Captures each distinct frame of the batch once and answers every task from its frame, each
/// with the moment of the picture it read: its snapshot's, or its frame's, stamped with
/// `input_epoch` as it stood once that capture came back.
fn run_batch(batch: &[ImageTask], capture: CaptureFn, input_epoch: &AtomicU64) -> Vec<ImageResult> {
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
    // Every search's templates first, here on the worker, before the captures: each PNG of the
    // batch opened once — a path two searches share is decoded once, the first time it is seen
    // or after it changed — and each handle's picture made, if its own task has not made it yet.
    // Their time is counted with the matching's.
    let t0 = Instant::now();
    let mut opened = template_store::Opened::default();
    let found: Vec<Option<Templates>> = batch.iter().map(|t| templates_of(t, &mut opened)).collect();
    let opening = t0.elapsed();
    // Only the tasks that read the screen get a frame: a cells task whose window had no client
    // area is answered without one, a snapshot's task reads the snapshot, and a search whose
    // templates cannot be opened is answered without a picture.
    let reading: Vec<(usize, capture_source::FrameKey)> = batch
        .iter()
        .enumerate()
        .filter(|(i, _)| !matches!(found[*i], Some(Err(_))))
        .filter_map(|(i, t)| t.screen_key().map(|k| (i, k)))
        .collect();
    let (keys, frame_of_reading) = capture_source::frame_keys(reading.iter().map(|(_, k)| *k));
    let frames: Vec<capture_source::Captured> = capture_source::capture_frames(capture, &keys, input_epoch);
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
        .zip(found.into_par_iter())
        .map(|(&(t, idx), found)| answer(t, idx.map(|i| frames[i].img.as_ref()), found))
        .collect();
    let match_ms = (opening + t1.elapsed()).as_millis() as u32;
    let capture_ms: u32 = frames.iter().map(|f| f.ms).sum();
    plan.iter()
        .zip(outcomes)
        .enumerate()
        .map(|(i, ((t, idx), outcome))| ImageResult {
            id: t.id,
            seen: seen_of(t, idx.map(|f| &frames[f])),
            outcome,
            capture_ms,
            match_ms,
            batch: batch_len,
            first_of_batch: i == 0,
        })
        .collect()
}

/// The moment of the picture task `t` read, given the frame the batch captured for it: its
/// snapshot's own; its frame's, when that capture came back; none when it read no picture — there
/// was nothing to read, the capture failed, or nothing was captured for it (a search whose
/// templates could not be opened, a handle's preparation).
fn seen_of(t: &ImageTask, frame: Option<&capture_source::Captured>) -> Option<Moment> {
    let unresolved = match &t.job {
        Job::Search { unresolved, .. } => unresolved.is_some(),
        Job::Cells(c) => c.unresolved.is_some(),
        Job::Prepare(_) => true,
        Job::Profile { .. } | Job::Crop | Job::Template | Job::Save(_) => false,
    };
    match (&t.hay, frame) {
        _ if unresolved => None,
        (Haystack::Frame { frame, .. }, _) => Some(crate::snapshot::moment_of(frame)),
        (Haystack::Screen { .. }, Some(f)) if f.img.is_ok() => Some((crate::snapshot::millis_of(f.at), f.input_epoch)),
        (Haystack::Screen { .. }, _) => None,
    }
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
fn picture<'a>(t: &'a ImageTask, shot: Option<Result<&'a CapturedImage, &'a HostError>>) -> Result<Pic<'a>, HostError> {
    match &t.hay {
        // Its region was cut to the snapshot at the call; a region outside it cannot get here.
        Haystack::Frame { frame, region } => match frame.area(*region) {
            Some(a) => Ok(Pic {
                img: &frame.img,
                origin: (frame.rect.x, frame.rect.y),
                area: Some(Rect { x: a.x, y: a.y, w: a.w, h: a.h }),
            }),
            None => Err(snapshot::no_overlap()),
        },
        Haystack::Screen { region, .. } => match shot {
            Some(Ok(img)) => Ok(Pic { img, origin: (region.0, region.1), area: None }),
            Some(Err(why)) => Err(why.clone()),
            None => Err(crate::backend::capture_failed()),
        },
    }
}

/// A search's templates, each its picture, in order — or why one of them has none.
type Templates = Result<Vec<Arc<Decoded>>, HostError>;

/// The templates of task `t` when it is a search that looks — opened through `opened`, made if
/// they are handles still to be made — and `None` for every other task.
fn templates_of(t: &ImageTask, opened: &mut template_store::Opened) -> Option<Templates> {
    match &t.job {
        Job::Search { entries, unresolved: None, .. } => Some(template_store::resolve(entries.iter().map(|(n, _)| n), opened)),
        _ => None,
    }
}

/// One task against its picture: the frame the batch captured for it, or why the capture
/// failed; `None` for a task that was not captured (an unresolved one, which carries its own
/// reason, a snapshot's, which reads the snapshot, and a search whose templates could not be
/// opened). `found` is a search's templates as the batch opened them ([`templates_of`]); given
/// `None`, a search opens them itself.
fn answer(t: &ImageTask, frame: Option<Result<&CapturedImage, &HostError>>, found: Option<Templates>) -> Outcome {
    match &t.job {
        Job::Search { entries, tol, scales, mode, unresolved } => Outcome::Search(match unresolved {
            Some(why) => Err(why.clone()),
            None => found
                .unwrap_or_else(|| template_store::resolve(entries.iter().map(|(n, _)| n), &mut template_store::Opened::default()))
                .and_then(|decs| {
                    let paired: Vec<(Arc<Decoded>, Option<Rect>)> = decs.into_iter().zip(entries.iter().map(|(_, w)| *w)).collect();
                    picture(t, frame).map(|pic| match_task(&pic, &paired, *tol, scales, *mode))
                }),
        }),
        Job::Cells(job) => Outcome::Cells(cells_task(t, job, frame)),
        Job::Profile { cols, rows, dark } => Outcome::Profile(profile_task(t, *cols, *rows, *dark)),
        Job::Crop => Outcome::Crop(crop_task(t)),
        Job::Template => Outcome::Template(template_task(t, frame)),
        Job::Save(job) => Outcome::Save(save_task(t, job, frame)),
        Job::Prepare(p) => {
            // Kept by the handle — a failure from bytes included, which its searches answer; a
            // file's is tried again by the next. Nothing to make for a handle that is gone.
            if let Some(p) = p.upgrade() {
                let _ = p.make();
            }
            Outcome::Prepared
        }
    }
}

/// A shot's picture — the frame the batch captured for it, or the part of its snapshot it covers,
/// copied — marked, encoded and written (`save::write_shot`). A panic in it is answered, as a
/// template's is.
///
/// Not for a shot nobody waits for any more — ended ([`end_task`]) after the worker took it, its
/// handler gone with its module or cancelled: asked before the copy and again once the PNG is
/// made, so no file is written, and no folder made again, for a module that is gone. Before the
/// worker, the write was on the event loop and could not outlive its module. A shot already being
/// written when its module goes is still written.
fn save_task(t: &ImageTask, job: &crate::save::SaveJob, frame: Option<Result<&CapturedImage, &HostError>>) -> Result<(), HostError> {
    let wanted = || !is_ended(t.id);
    if !wanted() {
        return Err(crate::save::not_waited());
    }
    let (img, origin) = match &t.hay {
        // Its region was cut to the snapshot at the call.
        Haystack::Frame { frame, region } => match frame.crop_image(*region) {
            Some(img) => (img, (region.x, region.y)),
            None => return Err(snapshot::no_overlap()),
        },
        Haystack::Screen { region, .. } => match frame {
            // The batch's frame of the region may be another task's too: the shot keeps a copy of
            // its own, which it marks.
            Some(Ok(img)) => (CapturedImage { w: img.w, h: img.h, rgba: img.rgba.clone() }, (region.0, region.1)),
            Some(Err(why)) => return Err(why.clone()),
            None => return Err(crate::backend::capture_failed()),
        },
    };
    match logging::contain(|| crate::save::write_shot(img, origin, &job.marks, &job.path, job.shot, &wanted)) {
        Ok(done) => done,
        Err(report) => {
            contained("a shot", "it was answered false", &report);
            Err(crate::save::save_failed())
        }
    }
}

/// A snapshot's `crop`: the part of its picture the task's region covers, copied (`Frame::crop`). A
/// panic in it is answered.
fn crop_task(t: &ImageTask) -> Result<Frame, HostError> {
    // Never the screen: a crop's task is sent with its snapshot.
    let Haystack::Frame { frame, region } = &t.hay else { return Err(failed(JobKind::Crop)) };
    match logging::contain(|| frame.crop(*region)) {
        Ok(Some(f)) => Ok(f),
        Ok(None) => Err(snapshot::not_inside()),
        Err(report) => {
            contained("a crop", "it was answered nil", &report);
            Err(failed(JobKind::Crop))
        }
    }
}

/// A `template{ capture }`'s picture — the frame the batch captured for it, or the part cut, whole,
/// from its snapshot — made a template ([`template_picture`]).
fn template_task(t: &ImageTask, frame: Option<Result<&CapturedImage, &HostError>>) -> Result<(Arc<Decoded>, u32), HostError> {
    let got = match &t.hay {
        Haystack::Frame { frame, region } => frame.crop_image(*region).ok_or_else(snapshot::not_inside),
        Haystack::Screen { .. } => match frame {
            // The batch's frame of the region may be another task's too: the template keeps a
            // copy of its own.
            Some(Ok(img)) => Ok(CapturedImage { w: img.w, h: img.h, rgba: img.rgba.clone() }),
            Some(Err(why)) => Err(why.clone()),
            None => Err(crate::backend::capture_failed()),
        },
    };
    let cap = got?;
    match logging::contain(|| template_picture(cap)) {
        Ok(made) => made,
        Err(report) => {
            contained("a template", "it was answered nil", &report);
            Err(failed(JobKind::Template))
        }
    }
}

/// A picture made a template, and how many of its pixels a search compares: every one. A capture
/// that came back short is a capture that failed.
fn template_picture(cap: CapturedImage) -> Result<(Arc<Decoded>, u32), HostError> {
    let dec = Decoded::from_capture(cap).map_err(|_| short_capture())?;
    let count = dec.count();
    Ok((Arc::new(dec), count))
}

/// A profile's reduction of its picture: the part of it the task's region covers, which was cut
/// to the picture before the task was sent. A panic in the reduction is answered, like a
/// template's, rather than taking the worker with it.
fn profile_task(t: &ImageTask, cols: bool, rows: bool, dark: u8) -> Result<Box<Reduced>, HostError> {
    // Never the screen: a profile's task is sent with its picture.
    let Haystack::Frame { frame, region } = &t.hay else { return Err(failed(JobKind::Profile)) };
    let area = frame.area(*region).ok_or_else(snapshot::no_overlap)?;
    let began = Instant::now();
    match logging::contain(|| profile::profile(&frame.img, area, cols, rows, dark)) {
        Ok(Some((columns, rows))) => Ok(Box::new(Reduced { columns, rows, rect: *region, took: began.elapsed() })),
        // The picture holds fewer bytes than its own size says: read nothing.
        Ok(None) => Err(short_capture()),
        Err(report) => {
            contained("a profile", "it was answered nil", &report);
            Err(failed(JobKind::Profile))
        }
    }
}

/// A cells read against its picture: a callback's or a handler's `cells` or `matchCells`.
/// A panic in the reduction is answered, like a template's, rather than taking the worker with it.
fn cells_task(
    t: &ImageTask,
    job: &CellsJob,
    frame: Option<Result<&CapturedImage, &HostError>>,
) -> Result<(Vec<u8>, Option<cells::Ranked>), HostError> {
    if let Some(why) = &job.unresolved {
        return Err(why.clone());
    }
    let pic = picture(t, frame)?;
    let (_, _, w, h) = t.region();
    let sub = pic.area.map(|a| cells::Sub { x: a.x, y: a.y, w: a.w, h: a.h });
    match logging::contain(|| cells_of(&job.matcher, job.rank, pic.img, (w, h), sub)) {
        Ok(answer) => answer,
        Err(report) => {
            contained("a cells match", "it was answered nil", &report);
            Err(failed(JobKind::Cells))
        }
    }
}

/// The cells of a picture — a capture of exactly the region, `w` x `h`, held to that size as it
/// always was, or `sub` of a snapshot — and, when `rank`, their ranking against the matcher's
/// states.
fn cells_of(
    matcher: &cells::Matcher,
    rank: bool,
    img: &CapturedImage,
    (w, h): (i32, i32),
    sub: Option<cells::Sub>,
) -> Result<(Vec<u8>, Option<cells::Ranked>), HostError> {
    match (sub, rank) {
        (None, true) => matcher.answer(img, w, h).map(|(live, r)| (live, Some(r))),
        (Some(s), true) => matcher.answer_sub(img, s).map(|(live, r)| (live, Some(r))),
        (None, false) => cells::of_capture(img, w, h, &matcher.spec).map(|live| (live, None)),
        (Some(s), false) => cells::of_sub(img, s, &matcher.spec).map(|live| (live, None)),
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
    // A `within` is relative to the region; in a snapshot the region starts at the area's corner,
    // and it was cut to the region at the call, so it stays inside the area.
    let area_of = |within: &Option<Rect>| match (within, pic.area) {
        (Some(w), Some(b)) => Some(Rect { x: b.x + w.x, y: b.y + w.y, w: w.w, h: w.h }),
        (Some(w), None) => Some(*w),
        (None, b) => b,
    };
    let one = |n: usize, (dec, within): &(Arc<Decoded>, Option<Rect>)| -> Option<ScreenHit> {
        let area = area_of(within);
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
        // Every match of the one template, at its own size: `scales` is not read.
        Mode::All => {
            let Some((dec, within)) = entries.first() else { return Vec::new() };
            match logging::contain(|| template::find_all_in(pic.img, area_of(within), dec, tol)) {
                Ok(found) => found
                    .into_iter()
                    .map(|(x, y)| Some(ScreenHit { x: ox + x as i32, y: oy + y as i32, w: dec.w, h: dec.h, n: 1 }))
                    .collect(),
                Err(report) => {
                    contained("a template match", "that template was answered as no match", &report);
                    Vec::new()
                }
            }
        }
    }
}

/// Contained panics this session, so the log gets the 1st, 2nd, 4th, 8th… rather than one
/// line per poll: a template that panics will panic every 500 ms, and the first line already
/// carries the message and the place. This throttle is the only one: the application's panic
/// hook writes nothing for a panic inside `logging::contain`, which is how every panic on the
/// worker is caught.
static CONTAINED: AtomicU64 = AtomicU64::new(0);

/// Says, within that throttle, that `what` panicked on the image worker and how it was `answered`.
pub(crate) fn contained(what: &str, answered: &str, report: &str) {
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

/// A callback's key as one module VM asked it of one call: its index, its generation, the call's
/// name and the key.
type CallKey = (usize, u64, &'static str, String);

/// What the event loop keeps of the worker's work: the way a task reaches it and the way its
/// answers come back, the tasks waiting for an answer — a callback's or a handler's — and the
/// answers decided without the worker, delivered on the next tick; the callbacks' keys, and the
/// input epoch the worker stamps its pictures with. Main thread only.
pub(crate) struct ImageState {
    tasks: Sender<ImageTask>,
    results: Receiver<ImageResult>,
    /// The tasks waiting for the worker's answer, by number.
    pending: RefCell<HashMap<u64, PendingImage>>,
    /// Answered without the worker, on the next tick: a task ended by a newer one with its key or
    /// past its module's cap, one the worker could not take, and a callback's answer decided at
    /// the call (`Outcome::AtCall`).
    ready: RefCell<Vec<(PendingImage, Outcome)>>,
    /// Per module VM (its index and generation), call and key, the newest callback's task asked
    /// with it, while it has not been delivered: an older answer with the key that is delivered
    /// after a newer task was asked — by an event that ran before it — is answered
    /// [`SUPERSEDED`](crate::snapshot::SUPERSEDED) instead ([`open_image_in`]).
    newest_key: RefCell<HashMap<CallKey, u64>>,
    /// Per module VM and key, the answers in its mailbox, neither run nor dropped yet: still out
    /// for `host.screen.pending`.
    handed: RefCell<HashMap<(usize, u64, String), u32>>,
    /// `host.inputEpoch()` as the event loop last turned it over, which the worker reads as each
    /// of its captures comes back.
    input_epoch: Arc<AtomicU64>,
}

/// Task numbers, process-wide and never reused: the worker's list of ended tasks (`ENDED`) is
/// process-wide too, and a number that two holders gave out would end the other's task.
static NEXT_TASK: AtomicU64 = AtomicU64::new(1);

impl ImageState {
    /// The state over the worker that takes `tasks` and answers on `results`, stamping its
    /// pictures with `input_epoch` (`spawn_image_worker`'s).
    pub(crate) fn new(tasks: Sender<ImageTask>, results: Receiver<ImageResult>, input_epoch: Arc<AtomicU64>) -> ImageState {
        ImageState {
            tasks,
            results,
            pending: RefCell::new(HashMap::new()),
            ready: RefCell::new(Vec::new()),
            newest_key: RefCell::new(HashMap::new()),
            handed: RefCell::new(HashMap::new()),
            input_epoch,
        }
    }

    /// A state with no worker behind it, for the tests' holders that make no image reads: every
    /// task sent to it is refused.
    #[cfg(test)]
    pub(crate) fn detached() -> ImageState {
        let (tasks, _) = std::sync::mpsc::channel();
        let (_, results) = std::sync::mpsc::channel();
        ImageState::new(tasks, results, Arc::new(AtomicU64::new(0)))
    }

    /// A new task's number.
    pub(crate) fn next_id(&self) -> u64 {
        NEXT_TASK.fetch_add(1, Ordering::Relaxed)
    }

    /// Hands `task` to the worker; false when the worker is gone (the application is closing).
    pub(crate) fn send(&self, task: ImageTask) -> bool {
        self.tasks.send(task).is_ok()
    }

    /// Whether a task waits for its answer, or an answer for the next tick — a reason for the
    /// headless loop to run.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.borrow().is_empty() || !self.ready.borrow().is_empty()
    }

    /// `host.inputEpoch()` turned over to `epoch`: a picture the worker takes from now on carries
    /// it. Told before anything acts, as the `screen-capture` thread is.
    pub(crate) fn note_input_epoch(&self, epoch: u64) {
        self.input_epoch.store(epoch, Ordering::SeqCst);
    }

    /// Whether a callback's task of module `owner`'s VM `gen` with `key` is out: with the worker,
    /// answered and not delivered yet, or in its module's mailbox.
    pub(crate) fn waiting_with(&self, owner: usize, gen: u64, key: &str) -> bool {
        let mine = |p: &PendingImage| p.owner == owner && p.gen == gen && p.key.as_deref() == Some(key);
        self.pending.borrow().values().any(mine)
            || self.ready.borrow().iter().any(|(p, _)| mine(p))
            || self.handed.borrow().contains_key(&(owner, gen, key.to_string()))
    }

    /// `p` goes to its module's mailbox: counted as out until it runs or is dropped (`unhand`).
    fn hand(&self, p: &mut PendingImage) {
        if let Some(k) = &p.key {
            *self.handed.borrow_mut().entry((p.owner, p.gen, k.clone())).or_insert(0) += 1;
            p.handed = true;
        }
    }

    /// `p` left its module's mailbox: it runs, or it is dropped.
    fn unhand(&self, p: &PendingImage) {
        let Some(k) = p.key.as_ref().filter(|_| p.handed) else { return };
        let mut handed = self.handed.borrow_mut();
        let at = (p.owner, p.gen, k.clone());
        if let Some(n) = handed.get_mut(&at) {
            *n -= 1;
            if *n == 0 {
                handed.remove(&at);
            }
        }
    }

    /// `p` is the newest callback's task of its module VM and call with its key.
    fn note_key(&self, p: &PendingImage) {
        if let Some(k) = &p.key {
            self.newest_key.borrow_mut().insert((p.owner, p.gen, p.asked_as(), k.clone()), p.id);
        }
    }

    /// Whether a newer task with `p`'s key, of its call, was asked after `p` — so `p`'s answer,
    /// not delivered yet, is superseded.
    fn replaced(&self, p: &PendingImage) -> bool {
        p.key.as_ref().is_some_and(|k| {
            self.newest_key.borrow().get(&(p.owner, p.gen, p.asked_as(), k.clone())).is_some_and(|n| *n > p.id)
        })
    }

    /// `p` was delivered or dropped: when it was the newest with its key, the key is forgotten.
    fn forget_key(&self, p: &PendingImage) {
        if let Some(k) = &p.key {
            let mut keys = self.newest_key.borrow_mut();
            let at = (p.owner, p.gen, p.asked_as(), k.clone());
            if keys.get(&at) == Some(&p.id) {
                keys.remove(&at);
            }
        }
    }

    /// Takes task `id` off the worker's hands — it skips it if it has not begun it — and answers it
    /// `why` on the next tick. False when it was not waiting.
    fn end(&self, id: u64, why: HostError) -> bool {
        let Some(p) = self.pending.borrow_mut().remove(&id) else { return false };
        end_task(id);
        let outcome = Outcome::failing(p.kind, why);
        self.ready.borrow_mut().push((p, outcome));
        true
    }

    /// Latest wins per key: the callbacks' tasks of module `owner`'s VM `gen` of `call` with `key`
    /// still with the worker are answered `SUPERSEDED`. Keys are each call's own: an `imageSearch`'s
    /// never ends a `matchCells`. One answered and not delivered yet is judged as it runs
    /// ([`open_image_in`]).
    fn supersede(&self, owner: usize, gen: u64, call: &'static str, key: &str) {
        let mut ids: Vec<u64> = self
            .pending
            .borrow()
            .values()
            .filter(|p| {
                p.owner == owner
                    && p.gen == gen
                    && p.asked_as() == call
                    && p.key.as_deref() == Some(key)
                    && matches!(p.waiter, ImageWaiter::Callback { .. })
            })
            .map(|p| p.id)
            .collect();
        ids.sort_unstable();
        for id in ids {
            self.end(id, crate::snapshot::superseded());
        }
    }
}

/// Delivers what the worker answered, and what was decided without it: a handler that waited for
/// a task goes on with its answer at once ([`crate::task::resume_image`]); a callback's answer is
/// handed to its module's mailbox in the lane it was asked from, which drops it for a module that
/// is disabled, reloaded, removed or stopped; a profile's reduction goes to its request in
/// snapshot.rs (`snapshot::deliver_reduced`). The host's tick and the tests' holders both run
/// this.
///
/// ONE epoch for the whole drain, not one per result — and none for a drain that only resumes
/// handlers, which go on in the dispatch that asked, as a read's or a picture's wait does.
///
/// The bump used to sit next to each callback, which reads correctly on its own — a result IS a
/// fresh observation — and is wrong for a batch. Every search in a batch was answered from ONE
/// captured frame, so the batch is one observation, not eighty-four; and the epoch is what every
/// memo is keyed on. Turning it over per result meant that during the delivery of an 84-result
/// batch, the shared per-(spec, epoch) origin memo was invalidated eighty-four times — cold on
/// exactly the tick that had the most callers to share it with, each re-resolving what the one
/// before it had just worked out. Bumped once, before the first callback runs, so they all see
/// the same new world. Fewer invalidations AND a truer statement about what changed.
pub(crate) fn fire_images<H: MailHost>(h: &H) {
    let im = h.images();
    let mut bumped = false;
    // Answered without the worker: ended by a newer task with its key or past its module's cap,
    // not taken by the worker, or a callback's answer decided at the call.
    let ready = std::mem::take(&mut *im.ready.borrow_mut());
    for (p, outcome) in ready {
        hand_over(h, p, outcome, None, &mut bumped);
    }
    while let Ok(res) = im.results.try_recv() {
        // The worker answered one that was ended meanwhile: its id leaves the set here.
        if let Some(set) = ended().as_mut() {
            set.remove(&res.id);
        }
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
        //
        // A shot's PNG encode and file write run on the pool with the matches, and every read of
        // its batch waits for them: they are in the second figure, and the line names them, so a
        // calibration shot that trips it is not read as a slow match.
        //
        // The templates the batch opened, and the pictures of handles it made, are in the second
        // figure too: a PNG decoded for the first time, or again after a calibration captured it
        // again, is the worker's work now.
        if res.first_of_batch && res.capture_ms + res.match_ms >= 50 {
            logging::line(
                "image",
                &format!(
                    "batch of {} read(s) took {} ms (capture {}, templates, matching, reduction and shot writing {} across {} thread-pool tasks)",
                    res.batch,
                    res.capture_ms + res.match_ms,
                    res.capture_ms,
                    res.match_ms,
                    res.batch
                ),
            );
        }
        // A profile's reduction: its request waits in snapshot.rs, which hands the answer over
        // (`deliver_reduced`) — or has let it go, ended or dropped meanwhile.
        if let Outcome::Profile(reduced) = res.outcome {
            if let Some(p) = h.snap_state().reduced(res.id) {
                crate::snapshot::deliver_reduced(h, p, reduced, &mut bumped);
            }
            continue;
        }
        // A handle's preparation: its picture is in the handle, and nobody waits for it here.
        if matches!(res.outcome, Outcome::Prepared) {
            continue;
        }
        let entry = im.pending.borrow_mut().remove(&res.id);
        let Some(p) = entry else { continue };
        hand_over(h, p, res.outcome, res.seen, &mut bumped);
    }
}

/// One answer, to whoever waits for it: a handler goes on with it — what its call returns, built
/// in its VM only if it is still waiting — and turns no epoch; a callback's goes to its module's
/// mailbox with the moment of its picture (`seen`), the epoch turned once before the drain's
/// first. The mailbox drops it for a module that may not hear it any more ([`discard_image_in`]).
fn hand_over<H: MailHost>(h: &H, p: PendingImage, outcome: Outcome, seen: Option<Moment>, bumped: &mut bool) {
    let PendingImage { lua, waiter, scope, owner, gen, id, kind, key, handed, prio } = p;
    let waiter = match waiter {
        ImageWaiter::Handler { task, call, answer } => {
            crate::task::resume_image(h, task, id, || {
                answer(&lua, outcome).map_err(|e| {
                    // A VM that ran out of memory building it is stopped, and the stop reports
                    // itself; the handler is not run in it again.
                    crate::vm_guard::note_failure(&lua, crate::vm_guard::EntryKind::Handler, call, &e);
                    e.to_string()
                })
            });
            return;
        }
        callback => callback,
    };
    if !*bumped {
        h.bump_epoch();
        *bumped = true;
    }
    let mut p = PendingImage { lua, waiter, scope, owner, gen, id, kind, key, handed, prio };
    // In the lane it was asked from, through its module's mailbox (`open_image`).
    let _prio = crate::ocr::types::enter_priority(p.prio);
    h.images().hand(&mut p);
    let lua = p.lua.clone();
    mailbox::deliver(h, owner, &lua, Event::Image { p: Box::new(p), outcome, seen });
}

/// A handler's wait for task `id` given up — the handler was cancelled or ended, its module went,
/// or module code resumed it itself: off the list, not made if the worker has not begun it, and
/// what its answer held — a charge reserved at the call — given back now. Nothing happens for a
/// task that is not waiting.
pub(crate) fn withdraw<H: ReadHost>(h: &H, id: u64) {
    let im = h.images();
    let found = im.pending.borrow_mut().remove(&id);
    let p = match found {
        Some(p) => {
            end_task(id);
            Some(p)
        }
        None => {
            let mut list = im.ready.borrow_mut();
            list.iter().position(|(p, _)| p.id == id).map(|at| list.remove(at).0)
        }
    };
    if let Some(p) = p {
        im.forget_key(&p);
        release([p]);
    }
}

/// Files a task handler `task` waits for in `call` (`screen.cells`, …), of the VM `lua` called
/// under the identity `scope`, at the wait's priority: the worker reads `hay` and does `job`, and
/// the handler goes on with what `answer` builds of it. Its number, which the handler's wait is on
/// (`On::Image`). A worker that is gone answers it, on the next tick, as the host's own failure.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_wait<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    task: TaskId,
    prio: Priority,
    hay: Haystack,
    job: Job,
    call: &'static str,
    answer: BuildAnswer,
) -> u64 {
    let im = h.images();
    let id = im.next_id();
    let t = ImageTask { id, hay, job, prio };
    let (owner, gen) = owner_of(lua, &h.vm_gens(), scope);
    let p = PendingImage {
        lua: lua.clone(),
        waiter: ImageWaiter::Handler { task, call, answer },
        scope,
        owner,
        gen,
        id,
        kind: t.job.kind(),
        key: None,
        handed: false,
        prio,
    };
    if im.send(t) {
        im.pending.borrow_mut().insert(id, p);
    } else {
        let outcome = Outcome::failing(p.kind, failed(p.kind));
        im.ready.borrow_mut().push((p, outcome));
    }
    id
}

/// What a call that works over a region asks of the image worker, its arguments read: its answer
/// at once — nothing to read this time — or the work, and how its answer is built. One plan for
/// both of the call's forms: a handler waits for it ([`start_plan`]), a callback is handed it
/// ([`submit_plan`]). An answer at once comes with how the call answers a task that could not
/// look, for a callback's that a newer request with its key replaced before it ran
/// ([`submit_now`]).
pub(crate) enum Plan {
    Now(MultiValue, BuildAnswer),
    Work { hay: Haystack, job: Job, answer: BuildAnswer },
}

/// A waited call `call` (as a module writes it), whose arguments `parse` reads into its plan as the
/// call that does not wait reads them — a mistake raises at the call: answered at once when there is
/// nothing to read this time; otherwise a task of the image worker filed for the handler that waits,
/// as `stop` names it ([`submit_wait`]).
pub(crate) fn start_plan<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    call: &'static str,
    stop: &'static str,
    nonce: Table,
    parse: impl FnOnce() -> mlua::Result<Plan>,
) -> mlua::Result<MultiValue> {
    crate::task::start_point(h, lua, scope, call, nonce, parse, |plan, task, prio| match plan {
        Plan::Now(values, _) => Ok(Started::Now(values)),
        Plan::Work { hay, job, answer } => Ok(Started::Waits(On::Image(submit_wait(h, lua, scope, task, prio, hay, job, stop, answer)))),
    })
}

/// What a call of the image worker's answers where it cannot wait (`task::now_answer`): a plan's
/// answer at once, when there is nothing to read this time; nothing for work, which the call raises
/// for. A plan of work is let go unsent: what it reserved is given back, a first-read comparison it
/// carried is owed again.
pub(crate) fn now_plan(plan: Plan) -> MultiValue {
    crate::task::now_answer(match plan {
        Plan::Now(values, _) => Some(values),
        Plan::Work { .. } => None,
    })
}

/// A call's callback form, its plan read as the call reads it ([`Plan`]): the task filed with `cb`
/// as its waiter, under `key` ([`submit_callback`]); an answer decided at the call delivered on the
/// next tick all the same ([`submit_now`]). `kind` is what the call's task does.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_plan<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    cb: Function,
    key: Option<String>,
    stop: &'static str,
    kind: JobKind,
    plan: Plan,
) -> mlua::Result<()> {
    match plan {
        Plan::Now(values, otherwise) => submit_now(h, lua, scope, cb, key, stop, kind, values, otherwise),
        Plan::Work { hay, job, answer } => submit_callback(h, lua, scope, cb, key, stop, hay, job, answer),
    }
}

/// Files a callback's task of the VM `lua`, called under the identity `scope` as `call`
/// (`screen.imageSearch`, …), at the priority of the dispatch that asks: the worker reads `hay`
/// and does `job`, and `cb` is handed what `answer` builds of it, then `info`. The newest of its
/// module's tasks of `call` with `key` — older ones still with the worker are answered
/// `SUPERSEDED` on the next tick — and one of the module's [`IMAGE_PER_OWNER`] callbacks' tasks: a
/// newer one past that ends its oldest. A worker that is gone answers it, on the next tick, as the
/// host's own failure. Never answered from inside the call.
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_callback<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    cb: Function,
    key: Option<String>,
    call: &'static str,
    hay: Haystack,
    job: Job,
    answer: BuildAnswer,
) -> mlua::Result<()> {
    let im = h.images();
    let prio = crate::ocr::types::current_priority();
    let task = ImageTask { id: im.next_id(), hay, job, prio };
    let p = callback_entry(h, lua, scope, cb, key, call, task.id, task.job.kind(), answer)?;
    file_callback(h, p, Some(task));
    Ok(())
}

/// A callback's answer decided at the call — nothing to read this time, or nothing the worker has to
/// do — handed to `cb` on the next tick, as every callback's answer is: `values`, then `info`. Under
/// `key` and its call's rules as [`submit_callback`]'s, but it waits for nothing. Replaced by a newer
/// request with its key before it ran, it is answered as a task replaced so is: what `otherwise`,
/// the call's own answer to a task that could not look, builds of that reason ([`open_image_in`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn submit_now<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    cb: Function,
    key: Option<String>,
    call: &'static str,
    kind: JobKind,
    values: MultiValue,
    otherwise: BuildAnswer,
) -> mlua::Result<()> {
    let answer: BuildAnswer = Box::new(move |lua, outcome| match outcome {
        Outcome::AtCall => Ok(values),
        replaced => otherwise(lua, replaced),
    });
    let p = callback_entry(h, lua, scope, cb, key, call, h.images().next_id(), kind, answer)?;
    file_callback(h, p, None);
    Ok(())
}

/// A callback's entry, numbered `id`, owned by the VM `lua` runs in, at the priority of the dispatch
/// that asks.
#[allow(clippy::too_many_arguments)]
fn callback_entry<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    cb: Function,
    key: Option<String>,
    call: &'static str,
    id: u64,
    kind: JobKind,
    answer: BuildAnswer,
) -> mlua::Result<PendingImage> {
    let (owner, gen) = owner_of(lua, &h.vm_gens(), scope);
    Ok(PendingImage {
        lua: lua.clone(),
        waiter: ImageWaiter::Callback { cb: lua.create_registry_value(cb)?, call, answer },
        scope,
        owner,
        gen,
        id,
        kind,
        key,
        handed: false,
        prio: crate::ocr::types::current_priority(),
    })
}

/// Files callback entry `p`: the newest with its key, then its task to the worker — or, with no
/// task, its answer for the next tick — and past its module's cap, its module's oldest ended.
fn file_callback<H: ReadHost>(h: &H, p: PendingImage, task: Option<ImageTask>) {
    let im = h.images();
    // A newer task with the key is the newest whether or not it can be taken.
    if let Some(k) = &p.key {
        im.supersede(p.owner, p.gen, p.asked_as(), k);
    }
    im.note_key(&p);
    let Some(task) = task else {
        im.ready.borrow_mut().push((p, Outcome::AtCall));
        return;
    };
    // The worker captures the region itself and ALWAYS posts a result (a failed capture or a
    // contained panic becomes an answer on the tick), so the pending callback is always drained.
    if !im.send(task) {
        // Worker thread gone (it contains its own panics, so this means teardown): answered as the
        // host's own failure, as a handler's wait is.
        let outcome = Outcome::failing(p.kind, failed(p.kind));
        im.ready.borrow_mut().push((p, outcome));
        return;
    }
    let owner = p.owner;
    im.pending.borrow_mut().insert(p.id, p);
    // Past the module's cap, its oldest request is ended — the rule OCR reads and snapshots
    // have. A module searching on a timer faster than the worker answers grew this map and the
    // worker's queue without a bound. The cap is for that runaway, not for a busy module: a
    // library module's VM holds one overlay per library and per Kontakt cell, each landmark gate
    // with one search waiting and a near-miss probe beside it, and every gate asks in the same
    // poll, so a sample-library module can have a dozen or two waiting at once by design.
    let over = oldest_over_cap(&im.pending.borrow(), owner, IMAGE_PER_OWNER);
    if let Some(old) = over {
        let why = too_many_waiting();
        if im.end(old, why.clone()) {
            h.note_image_crowding(owner, why.message());
        }
    }
}

impl Shared {
    /// Delivers finished image-worker results ([`fire_images`]); driven by both ticks.
    pub(crate) fn fire_image_results(&self) {
        fire_images(self);
    }

    /// An answer as it runs (`Event::Image`), by [`open_image_in`].
    pub(crate) fn open_image(&self, p: PendingImage, outcome: Outcome, seen: Option<Moment>) -> Opened {
        open_image_in(&self.images, p, outcome, seen, &|idx, what, e| self.report_callback_error(idx, what, e))
    }

    /// An answer that will not run, by [`discard_image_in`].
    pub(crate) fn discard_image(&self, p: PendingImage) {
        discard_image_in(&self.images, p);
    }

    /// Drops every task still waiting for an answer on behalf of module `owner`'s VM, and every
    /// answer of it not delivered yet: its module disabled, reloaded or removed.
    ///
    /// Part of `purge_module`: a pending entry holds a clone of the VM, so leaving it would keep
    /// the old VM alive past a reload — and hand it a callback once the answer came. And of a
    /// disable: a callback's answer is dropped then, as a read's is, and the worker skips what it
    /// has not begun.
    pub(crate) fn purge_pending_images(&self, owner: usize) {
        drop_owner(&self.images, owner);
    }
}

/// Every task and answer of module `owner`'s VM, dropped ([`Shared::purge_pending_images`]): the
/// host's and the tests' holders'.
pub(crate) fn drop_owner(im: &ImageState, owner: usize) {
    let gone = purge_owner(&mut im.pending.borrow_mut(), owner);
    // Ended at the worker as well, as a handler's withdrawn wait is: one it has not begun is
    // not made, and a shot it has is not written (`save_task`). This runs before the module's
    // handlers are dropped, whose withdrawal then finds nothing here to end.
    for p in &gone {
        end_task(p.id);
    }
    release(gone);
    // And the ones answered without the worker, not delivered yet: they hold the same VM.
    let ready: Vec<PendingImage> = {
        let mut list = im.ready.borrow_mut();
        let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut *list).into_iter().partition(|(p, _)| p.owner == owner);
        *list = rest;
        mine.into_iter().map(|(p, _)| p).collect()
    };
    release(ready);
    im.newest_key.borrow_mut().retain(|(o, ..), _| *o != owner);
    // Its answers in the mailbox go with the mailbox; none of them is out any more.
    im.handed.borrow_mut().retain(|(o, ..), _| *o != owner);
}

/// An answer as it runs (`Event::Image`), its module still the VM that asked and enabled: answered
/// `SUPERSEDED` when a newer task with its key, of its call, was asked since — by an event that ran
/// before it — as it would have been had the newer one come before the worker answered. Its key is
/// forgotten when it was the newest with it, and it is no longer out. The callback is called with
/// what the call returns, built in the VM ([`ImageWaiter::Callback`]'s `answer`), then `info` with
/// `seen`, the moment of its picture ([`crate::snapshot::callback_values`]). An answer that cannot
/// be built — its VM out of memory — is reported as the callback's error, through `report`
/// (module, kind of callback, message), and the callback is not called. The host's own `open` and the tests' holders both run this, so the tests run the host's
/// rule and not a copy of it.
pub(crate) fn open_image_in(
    im: &ImageState,
    p: PendingImage,
    outcome: Outcome,
    seen: Option<Moment>,
    report: &dyn Fn(usize, &str, &str),
) -> Opened {
    im.unhand(&p);
    let (outcome, seen) = if im.replaced(&p) {
        (Outcome::failing(p.kind, crate::snapshot::superseded()), None)
    } else {
        (outcome, seen)
    };
    im.forget_key(&p);
    let PendingImage { lua, waiter, scope, .. } = p;
    // Only a callback's answer comes through the mailbox; a handler's goes on in the drain.
    let ImageWaiter::Callback { cb, call, answer } = waiter else { return Opened::Gone };
    let f = lua.registry_value::<Function>(&cb);
    let _ = lua.remove_registry_value(cb);
    let built = f.and_then(|f| {
        let values = answer(&lua, outcome)?;
        Ok((f, crate::snapshot::callback_values(values, lua.create_table()?, seen)?))
    });
    match built {
        Ok((f, args)) => Opened::Run { f, args, ctx: Ctx::new(call, scope) },
        Err(e) => {
            // A VM that ran out of memory building it is stopped, and the stop reports itself.
            if !crate::vm_guard::note_failure(&lua, crate::vm_guard::EntryKind::Handler, call, &e) {
                report(scope, call, &e.to_string());
            }
            Opened::Gone
        }
    }
}

/// An answer that will not run — its module disabled, reloaded, removed or stopped: no longer out,
/// its key forgotten when it was the newest with it, its callback and what its answer held let go.
pub(crate) fn discard_image_in(im: &ImageState, p: PendingImage) {
    im.unhand(&p);
    im.forget_key(&p);
    release([p]);
}

/// A callback's search `id` of the VM `lua` under the identity `scope`, answered to `cb` as
/// `imageSearch(template, opts, cb)` answers it — counted as handed to its module's mailbox, as the
/// delivery hands it over: for the tests' holders, which have no `Shared`.
#[cfg(test)]
pub(crate) fn test_pending(im: &ImageState, lua: &Lua, gens: &HashMap<usize, u64>, id: u64, scope: usize, cb: Function) -> PendingImage {
    let (owner, gen) = owner_of(lua, gens, scope);
    let answer: BuildAnswer = Box::new(|lua, outcome| search_values(lua, Search::One, outcome, &[]));
    let mut p = PendingImage {
        lua: lua.clone(),
        waiter: ImageWaiter::Callback { cb: lua.create_registry_value(cb).expect("the callback is kept"), call: "screen.imageSearch", answer },
        scope,
        owner,
        gen,
        id,
        kind: JobKind::Search,
        key: None,
        handed: false,
        prio: crate::ocr::types::current_priority(),
    };
    im.hand(&mut p);
    p
}

/// [`test_pending`] asked with `opts.key` `key`: counted as out under it while it waits in its
/// module's mailbox.
#[cfg(test)]
pub(crate) fn test_keyed(im: &ImageState, lua: &Lua, gens: &HashMap<usize, u64>, id: u64, scope: usize, cb: Function, key: &str) -> PendingImage {
    let mut p = test_pending(im, lua, gens, id, scope, cb);
    p.key = Some(key.to_string());
    im.hand(&mut p);
    p
}

/// What the worker answers `t` with, its picture already there: a profile's, or a snapshot's
/// search — for other files' tests.
#[cfg(test)]
pub(crate) fn test_answer(t: &ImageTask) -> Outcome {
    answer(t, None, None)
}

/// A template as the tests compare it: its width and height, its pixels as RGBA, `count` and name.
#[cfg(test)]
pub(crate) type TestTemplate = (u32, u32, Vec<u8>, u32, Option<String>);

/// A template handle as the tests compare it, its picture made if it was still to be; `None` for
/// a value that is no template, or one whose picture cannot be made: for other files' tests.
#[cfg(test)]
pub(crate) fn test_template_of(v: &Value) -> Option<TestTemplate> {
    let Value::UserData(ud) = v else { return None };
    let t = ud.borrow::<TemplateHandle>().ok()?;
    let (dec, count) = t.pic.make().ok()?;
    let template::Body::Dense { rgba, .. } = &dec.body;
    Some((dec.w, dec.h, rgba.clone(), count, t.name.clone()))
}

/// Whether task `id` was ended and the worker has not taken it off the list yet.
#[cfg(test)]
pub(crate) fn test_ended(id: u64) -> bool {
    ended().as_ref().is_some_and(|s| s.contains(&id))
}

/// A search's answer that found nothing.
#[cfg(test)]
pub(crate) fn test_found_nothing() -> Outcome {
    Outcome::Search(Ok(vec![None]))
}

/// What `imageSearchEach` answers — its callback's first two values: the list, a hit or `false` for
/// each entry, its hits named from `names`; or `nil` and the error value when it could not look.
fn each_values(lua: &Lua, outcome: Outcome, names: &[Option<String>]) -> mlua::Result<MultiValue> {
    let found = match outcome {
        Outcome::Search(found) => found,
        // Never: the worker answers a search with a search.
        _ => Err(failed(JobKind::Search)),
    };
    match found {
        Ok(slots) => {
            let list = lua.create_table_with_capacity(slots.len(), 0)?;
            for (i, slot) in slots.iter().enumerate() {
                let item = match slot {
                    Some(h) => Value::Table(hit_table(lua, h.x, h.y, h.w, h.h, h.n, names.get(h.n - 1).and_then(|n| n.as_deref()))?),
                    None => Value::Boolean(false),
                };
                list.raw_set(i + 1, item)?;
            }
            one_value(Value::Table(list))
        }
        Err(why) => with_reason(lua, Value::Nil, why),
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
/// the process without the collector ever noticing. The budget is what notices — and a handle's
/// bytes still to be made into its picture are held by nothing that outlives the handle and its
/// searches (`Job::Prepare` holds them weakly), so what the budget gave back is gone. A constant
/// like `template_store::CACHE_CAP`, not a setting — no module needs anywhere near it, and a
/// number nobody has a reason to change does not belong in front of the user.
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

/// Takes `bytes` from this VM's budget, collecting garbage first if they do not fit, or answers
/// `Ok(Err(err))` (`too_large`) if they still do not: for every form of the call, as the snapshot
/// budget is (D2 of the typed-errors plan) — what the module holds now decides it, not the call.
///
/// TWICE, because one full collection may only finish the cycle already in progress; mlua's
/// own documentation says so. Handles dropped by the module are unreachable but not yet
/// finalised until then, and their bytes come back only when they are.
fn reserve(lua: &Lua, budget: &Rc<Cell<usize>>, bytes: usize) -> mlua::Result<Result<Charge, HostError>> {
    let fits = |b: &Rc<Cell<usize>>| b.get().saturating_add(bytes) <= TEMPLATE_BUDGET_PER_VM;
    if !fits(budget) {
        lua.gc_collect()?;
        lua.gc_collect()?;
        if !fits(budget) {
            return Ok(Err(HostError::with(
                &screen::MODULE_TEMPLATES,
                format!(
                    "this module's templates would hold {:.1} MiB; the limit is {} MiB per module VM. \
                     Build templates once, when the module loads, rather than on every poll.",
                    (budget.get() + bytes) as f64 / (1 << 20) as f64,
                    TEMPLATE_BUDGET_PER_VM >> 20
                ),
            )));
        }
    }
    budget.set(budget.get() + bytes);
    Ok(Ok(Charge { bytes, budget: budget.clone() }))
}

/// A template in a module's hands: `host.screen.template`'s answer, and accepted by every
/// search wherever a path is.
///
/// VM-local, like any Lua value. It never leaves the main thread — searches take a clone of
/// `pic`, which is all the worker ever sees — so a handle collected while a search is in
/// flight is harmless.
pub(crate) struct TemplateHandle {
    /// Its picture: made already for a `capture`, made on the image worker for bytes and a file.
    pic: Arc<Prepared>,
    /// Its size as the call knew it: the bytes' `w` and `h`, the file's header, the capture's.
    w: u32,
    h: u32,
    name: Option<String>,
    _charge: Charge,
}

impl TemplateHandle {
    /// Its size: the picture's once it is made — a file captured again between the call and its
    /// decode has the size it was decoded at — and the call's until then.
    fn size(&self) -> (u32, u32) {
        match self.pic.get() {
            Some(Ok((dec, _))) => (dec.w, dec.h),
            _ => (self.w, self.h),
        }
    }

    /// How many of its pixels a search compares, once its picture is made.
    fn count(&self) -> Option<u32> {
        match self.pic.get() {
            Some(Ok((_, count))) => Some(*count),
            _ => None,
        }
    }
}

impl UserData for TemplateHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("w", |_, t| Ok(t.size().0));
        fields.add_field_method_get("h", |_, t| Ok(t.size().1));
        fields.add_field_method_get("name", |_, t| Ok(t.name.clone()));
        fields.add_field_method_get("count", |_, t| Ok(t.count()));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            let (w, h) = t.size();
            let state = match t.pic.get() {
                Some(Ok((_, count))) => format!("{count} compared"),
                Some(Err(why)) => format!("cannot be used: {why}"),
                // A file that could not be opened the last time it was tried: the next search
                // tries again.
                None => match t.pic.failed() {
                    Some(why) => format!("cannot be used: {why}"),
                    None => "preparing".to_string(),
                },
            };
            Ok(match &t.name {
                Some(n) => format!("Template({n} {w}x{h}, {state})"),
                None => format!("Template({w}x{h}, {state})"),
            })
        });
    }
}

/// Where a template's pixels come from.
enum Source {
    Bytes { rgba: bool, data: Vec<u8>, w: u32, h: u32 },
    /// The screen now — or, with `snapshot`, that snapshot's pixels, cut out, never captured.
    /// `window`: given as a window region, whose budget overrun says so.
    Capture { x: i32, y: i32, w: i32, h: i32, snapshot: Option<Arc<Frame>>, window: bool },
    /// A `capture` given as a window region that has nothing to capture this time — an empty
    /// client area, or one so large that the region is past a template's size limit. Answered
    /// `nil, err`, like a failed capture: the window's size is a run-time condition.
    NotNow(HostError),
    File(String),
}

struct ParsedSpec {
    name: Option<String>,
    source: Source,
    /// The callback form's `key`.
    key: Option<String>,
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
/// a corner left out is the snapshot's edge, and the template is cut from that snapshot. `key` is
/// the callback form's (`cb`): a non-empty string there, and a mistake without one.
fn parse_spec(spec: &Table, sw: i32, sh: i32, cb: bool) -> mlua::Result<ParsedSpec> {
    let mut name = None;
    let mut wanted_key = None;
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
            "key" if !cb => return Err(spec_err("key is only taken with a callback (the second argument)")),
            "key" => match v {
                Value::String(s) if !s.to_str()?.is_empty() => wanted_key = Some(s.to_str()?.to_string()),
                other => return Err(spec_err(format!("key must be a non-empty string, got {}", crate::describe_value(&other)))),
            },
            "rgba" | "rgb" | "capture" | "file" => sources.push((key, v)),
            other => {
                return Err(spec_err(format!(
                    "unknown field '{other}'; a spec takes name and exactly one of rgba, rgb, \
                     capture or file (and snapshot with capture, and key with a callback)"
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
                    let (w, h) = (r.w.max(0) as u32, r.h.max(0) as u32);
                    let window = matches!(given, region::Region::Window(..));
                    match (template::check_size(w, h), window) {
                        (Ok(()), _) => Source::Capture { x: r.x, y: r.y, w: r.w, h: r.h, snapshot: snap, window },
                        (Err(e), false) => return Err(spec_err(e)),
                        (Err(e), true) => {
                            // `e` names the size itself: "5000x10 is too large; …". A resolved
                            // window region is never empty, so it is past one of the two limits.
                            let past = if w > template::MAX_SIDE || h > template::MAX_SIDE {
                                &screen::TEMPLATE_SIDE
                            } else {
                                &screen::TEMPLATE_PIXELS
                            };
                            Source::NotNow(HostError::with(past, format!("at the window's current size {e}")))
                        }
                    }
                }
                Err(u) => Source::NotNow(u.error()),
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
    Ok(ParsedSpec { name, source, key: wanted_key })
}

/// A `file` template's path, resolved, and the width and height its PNG header gives.
type FileHeader = (PathBuf, u32, u32);

/// What `host.screen.template` needs of the host for a spec that captures nothing: a `file`'s
/// path resolved and its PNG header read ([`template_file`]) — a file outside the package that
/// cannot be read is answered, `Ok(Err(err))` — and a handle's picture queued for the image
/// worker to make ([`queue_preparation`]). Passed in, so every rule here is testable without a
/// backend or a worker.
struct Making<'a> {
    file: &'a dyn Fn(&str) -> mlua::Result<Result<FileHeader, HostError>>,
    prepare: &'a dyn Fn(Arc<Prepared>),
}

/// Turns a checked spec that captures nothing into a handle, with the host's part (`making`)
/// passed in, so every rule here is testable without a backend.
///
/// A spec from bytes or a file is checked here and its picture made on the image worker: the
/// sizes and the byte count, or the file's PNG header, at the call, and the handle answered at
/// once, its picture still to be made. `nil, err` for a capture that had nothing to capture, for
/// a budget with no room, and for a file outside the module's package that cannot be read: what
/// the moment decides, which the module has to handle, while every other refusal is a mistake in
/// the code that wrote the spec, and raises. A capture is the image worker's ([`template_plan`]).
fn build_handle(lua: &Lua, spec: ParsedSpec, making: &Making) -> mlua::Result<MultiValue> {
    let budget = budget(lua);
    let mut name = spec.name;
    let (raw, w, h, charge) = match spec.source {
        Source::NotNow(why) => return with_reason(lua, Value::Nil, why),
        // The cache's decode itself, shared once it is made: the bytes are held by the image
        // worker's cache, so the handle costs this VM nothing. It is therefore a SNAPSHOT — a
        // re-captured PNG is picked up live only by a search that passes the path. Unnamed, it is
        // named by its path as written, which is what a search given the path would report.
        Source::File(path) => {
            let (full, w, h) = match (making.file)(&path)? {
                Ok(header) => header,
                Err(why) => return with_reason(lua, Value::Nil, why),
            };
            name = name.or(Some(path));
            (Raw::File(full), w, h, Charge { bytes: 0, budget: budget.clone() })
        }
        Source::Bytes { rgba, data, w, h } => {
            // The size limits first: a 4096x4096 spec must be told it is too large, not that
            // it would overrun the budget — and a wrong byte count before the budget too.
            template::check_size(w, h).map_err(spec_err)?;
            let channels = if rgba { template::Channels::Rgba } else { template::Channels::Rgb };
            template::check_bytes(w, h, data.len(), channels).map_err(spec_err)?;
            let charge = match reserve(lua, &budget, template::dense_bytes(w, h))? {
                Ok(charge) => charge,
                Err(why) => return with_reason(lua, Value::Nil, why),
            };
            let raw = if rgba { Raw::Rgba { w, h, bytes: data } } else { Raw::Rgb { w, h, bytes: data } };
            (raw, w, h, charge)
        }
        // Never: a capture is planned as a task of the image worker.
        Source::Capture { .. } => return with_reason(lua, Value::Nil, failed(JobKind::Template)),
    };
    let pic = Prepared::of(raw);
    (making.prepare)(pic.clone());
    let handle = TemplateHandle { pic, w, h, name, _charge: charge };
    one_value(Value::UserData(lua.create_userdata(handle)?))
}

/// What `template{ capture }` answers of its picture made a template, or of why there is none —
/// to a handler that waited and to a callback alike: the handle, named and charged as asked, or
/// `nil` and the error value, the charge given back.
fn template_values(lua: &Lua, outcome: Outcome, name: Option<String>, charge: Charge) -> mlua::Result<MultiValue> {
    match outcome {
        Outcome::Template(Ok((dec, count))) => {
            let (w, h) = (dec.w, dec.h);
            let handle = TemplateHandle { pic: Prepared::made(dec, count), w, h, name, _charge: charge };
            one_value(Value::UserData(lua.create_userdata(handle)?))
        }
        other => template_failed(lua, other),
    }
}

/// What `template` answers when there is no template: `nil` and the error value.
fn template_failed(lua: &Lua, outcome: Outcome) -> mlua::Result<MultiValue> {
    match outcome {
        Outcome::Template(Err(why)) => with_reason(lua, Value::Nil, why),
        // Never: the worker answers a template's task with a template.
        _ => with_reason(lua, Value::Nil, failed(JobKind::Template)),
    }
}

/// `host.screen.template`, as messages and log lines name it.
const FT: &str = "host.screen.template";

/// The spec of `host.screen.template`, which must be a table, read and checked — its `key` too for
/// the callback form (`cb`).
fn spec_of(v: &Value, (sw, sh): (i32, i32), cb: bool) -> mlua::Result<ParsedSpec> {
    match v {
        Value::Table(spec) => parse_spec(spec, sw, sh, cb),
        other => Err(spec_err(format!("takes a spec table, got {}", crate::describe_value(other)))),
    }
}

/// The stop, the callback and the keys of a template go by this name.
const TEMPLATE_STOP: &str = "screen.template";

/// A template's plan: for a `capture`, its charge reserved at the call and a task of the image
/// worker — the region captured through the module's source, with the first-read comparison when
/// it is owed, or cut from its snapshot; then made a template ([`template_values`]). A spec that
/// captures nothing — bytes, a file, a window region with nothing to capture this time, a region
/// not wholly inside its snapshot, a budget with no room for it — is answered at once, a handle's
/// picture left to the worker.
fn template_plan(lua: &Lua, parsed: ParsedSpec, making: &Making) -> mlua::Result<Plan> {
    let ParsedSpec { name, source, key } = parsed;
    let Source::Capture { x, y, w, h: ht, snapshot: snap, window } = source else {
        let now = build_handle(lua, ParsedSpec { name, source, key }, making)?;
        return Ok(Plan::Now(now, Box::new(template_failed)));
    };
    let charge = match reserve(lua, &budget(lua), template::dense_bytes(w as u32, ht as u32))? {
        Ok(charge) => charge,
        Err(why) => {
            // A window region's message begins as its size limit's does.
            let why = if window { HostError::with(why.reason(), format!("at the window's current size {why}")) } else { why };
            return Ok(Plan::Now(with_reason(lua, Value::Nil, why)?, Box::new(template_failed)));
        }
    };
    let r = geo::Rect::new(x, y, w, ht);
    let hay = match snap {
        // Cut from the snapshot, whole or not at all — a template cut from a clipped rectangle
        // would be a different template: nothing to cut is answered at once.
        Some(frame) if !frame.contains(r) => {
            let now = template_values(lua, Outcome::Template(Err(snapshot::not_inside())), name, charge)?;
            return Ok(Plan::Now(now, Box::new(template_failed)));
        }
        Some(frame) => Haystack::Frame { frame, region: r },
        None => Haystack::Screen {
            region: r.tuple(),
            source: capture_source::vm_source(lua),
            compare: capture_source::take_compare(lua, r.tuple()),
        },
    };
    let answer: BuildAnswer = Box::new(move |lua, outcome| template_values(lua, outcome, name, charge));
    Ok(Plan::Work { hay, job: Job::Template, answer })
}

/// A waited `host.screen.template`: the spec read — a mistake raises at the call — and planned
/// ([`template_plan`]): a `capture` waits for the image worker, anything else is answered at once.
fn start_template<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    nonce: Table,
    parse: impl FnOnce() -> mlua::Result<ParsedSpec>,
    making: &Making,
) -> mlua::Result<MultiValue> {
    start_plan(h, lua, scope, FT, TEMPLATE_STOP, nonce, || template_plan(lua, parse()?, making))
}

/// `host.screen.template(spec, cb)`: the same template with `cb` as its waiter — it never waits,
/// and the handle, or `nil` and the reason, is handed to `cb` on a later tick, a `capture`'s once the
/// image worker made it, any other on the next tick — under the spec's `key`.
fn submit_template<H: ReadHost>(h: &H, lua: &Lua, scope: usize, cb: Value, parse: impl FnOnce() -> mlua::Result<ParsedSpec>, making: &Making) -> mlua::Result<()> {
    let cb = callback_arg(FT, cb, "second argument")?;
    let mut parsed = parse()?;
    let key = parsed.key.take();
    let plan = template_plan(lua, parsed, making)?;
    submit_plan(h, lua, scope, cb, key, TEMPLATE_STOP, JobKind::Template, plan)
}

/// `template{ file }`'s path, resolved against `root` as a search's is, and the size its PNG
/// header gives: read at every such call — a few bytes of the file, nothing decoded — so a path
/// inside the module's package that is not there or not a PNG raises there, and one outside it is
/// answered `nil, err` — `not_found`, `invalid` or `failed` (D3 of the typed-errors plan).
fn template_file(root: &Path, path: &str) -> mlua::Result<Result<FileHeader, HostError>> {
    let full = root.join(path);
    match template_store::header(&full) {
        Ok((w, h)) => Ok(Ok((full, w, h))),
        Err(e) if template_store::in_package(root, &full) => Err(mlua::Error::external(e.to_string())),
        Err(e) => Ok(Err(e)),
    }
}

/// A search's template path, resolved against `root`: inside the module's package checked the
/// first time it is named (`template_store::locate`), so a file there that is not there or not a
/// PNG raises; outside it taken as it is, and a file there that cannot be opened is answered by
/// the search, `nil, err` (D3 of the typed-errors plan).
fn locate_in(root: &Path, path: &str) -> mlua::Result<PathBuf> {
    let full = root.join(path);
    if !template_store::in_package(root, &full) {
        return Ok(full);
    }
    template_store::locate(full).map_err(|e| mlua::Error::external(e.to_string()))
}

/// Queues the making of a handle's picture `p` on the image worker, ahead of every search that
/// uses it. A worker that is gone — the application is closing — leaves it to the first search
/// that needs it.
fn queue_preparation<H: ReadHost>(h: &H, p: Arc<Prepared>) {
    let im = h.images();
    let _ = im.send(preparation(im.next_id(), &p));
}

/// The task that makes `p`, numbered `id`: it reads nothing, and holds `p` weakly (`Job::Prepare`).
fn preparation(id: u64, p: &Arc<Prepared>) -> ImageTask {
    let hay = Haystack::Screen { region: (0, 0, 0, 0), source: CaptureSource::Standard, compare: None };
    ImageTask { id, hay, job: Job::Prepare(Arc::downgrade(p)), prio: crate::ocr::types::current_priority() }
}

/// `host.screen.template(spec, cb?) -> Template | nil, err`. A `capture` waits in a handler for
/// the image worker ([`start_template`]); where it cannot wait it raises, unless it has nothing to
/// capture this time ([`now_plan`]). Bytes and a file never wait: their picture is the worker's,
/// and they are answered at once wherever the call is made. With a callback nothing waits
/// ([`submit_template`]).
pub(crate) fn template(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    let start = lua.create_function(move |lua, (nonce, spec): (Table, Value)| {
        host_call!("host.screen.template");
        let screen = sh.backend.screen_size();
        let file = |path: &str| template_file(&sh.root(idx), path);
        let prepare = |p: Arc<Prepared>| queue_preparation(&*sh, p);
        start_template(&*sh, lua, idx, nonce, || spec_of(&spec, screen, false), &Making { file: &file, prepare: &prepare })
    })?;
    let sh = shared.clone();
    let now = lua.create_function(move |lua, spec: Value| {
        host_call!("host.screen.template");
        let parsed = spec_of(&spec, sh.backend.screen_size(), false)?;
        let file = |path: &str| template_file(&sh.root(idx), path);
        let prepare = |p: Arc<Prepared>| queue_preparation(&*sh, p);
        Ok(now_plan(template_plan(lua, parsed, &Making { file: &file, prepare: &prepare })?))
    })?;
    let sh = shared.clone();
    let submit = lua.create_function(move |lua, (spec, cb): (Value, Value)| {
        host_call!("host.screen.template");
        let screen = sh.backend.screen_size();
        let file = |path: &str| template_file(&sh.root(idx), path);
        let prepare = |p: Arc<Prepared>| queue_preparation(&*sh, p);
        submit_template(&*sh, lua, idx, cb, || spec_of(&spec, screen, true), &Making { file: &file, prepare: &prepare })
    })?;
    crate::task::points(lua, idx, shared.clone())?.call((FT, start, now, submit, 2))
}

/// `host.screen.template` as the host builds it, over `holder`, its wait and where it cannot wait
/// the host's own ([`start_template`], [`now_plan`]) and a handle's picture made by the holder's
/// image worker, and a file's path is resolved against `root`: for the tests' holders, which have
/// no backend. The primary screen is 1920x1080.
#[cfg(test)]
pub(crate) fn test_template<H: ReadHost>(lua: &Lua, idx: usize, holder: Rc<H>, root: PathBuf) -> mlua::Result<Function> {
    let (h, r) = (holder.clone(), root.clone());
    let start = lua.create_function(move |lua, (nonce, spec): (Table, Value)| {
        let file = |path: &str| template_file(&r, path);
        let prepare = |p: Arc<Prepared>| queue_preparation(&*h, p);
        start_template(&*h, lua, idx, nonce, || spec_of(&spec, (1920, 1080), false), &Making { file: &file, prepare: &prepare })
    })?;
    let (h, r) = (holder.clone(), root.clone());
    let now = lua.create_function(move |lua, spec: Value| {
        let parsed = spec_of(&spec, (1920, 1080), false)?;
        let file = |path: &str| template_file(&r, path);
        let prepare = |p: Arc<Prepared>| queue_preparation(&*h, p);
        Ok(now_plan(template_plan(lua, parsed, &Making { file: &file, prepare: &prepare })?))
    })?;
    let h = holder.clone();
    let submit = lua.create_function(move |lua, (spec, cb): (Value, Value)| {
        let file = |path: &str| template_file(&root, path);
        let prepare = |p: Arc<Prepared>| queue_preparation(&*h, p);
        submit_template(&*h, lua, idx, cb, || spec_of(&spec, (1920, 1080), true), &Making { file: &file, prepare: &prepare })
    })?;
    crate::task::points(lua, idx, holder)?.call((FT, start, now, submit, 2))
}

// ── Template arguments ─────────────────────────────────────────────────────────────────

/// One template argument — a path or a handle — as the needle the image worker opens and the
/// name a hit reports. A path is resolved and checked by `locate` ([`locate_in`]): nothing of it
/// is read here but, the first time a path inside the module's package is named, its header.
///
/// A path's name is the string exactly as the caller wrote it, so a module can tell its hits
/// apart without keeping a second list.
fn resolve_with(
    v: &Value,
    what: &str,
    locate: &dyn Fn(&str) -> mlua::Result<PathBuf>,
) -> mlua::Result<(Needle, Option<String>)> {
    match v {
        Value::String(s) => {
            let path = s.to_str()?.to_string();
            Ok((Needle::File(locate(&path)?), Some(path)))
        }
        Value::UserData(ud) => match ud.borrow::<TemplateHandle>() {
            Ok(h) => Ok((Needle::Handle(h.pic.clone()), h.name.clone())),
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

/// Every template in a list, checked BEFORE anything is captured.
///
/// A search of a list used to open each file inside its loop, after the capture, so a bad path
/// late in the list lay dormant until every earlier template happened to miss. Now a bad path
/// raises on the first call, wherever it sits.
fn resolve_list(
    list: &Table,
    what: &str,
    locate: &dyn Fn(&str) -> mlua::Result<PathBuf>,
) -> mlua::Result<Vec<(Needle, Option<String>)>> {
    let mut out = Vec::new();
    for v in list.clone().sequence_values::<Value>() {
        out.push(resolve_with(&v?, what, locate)?);
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

/// `opts.region` of a search, resolved now against a primary screen of `screen`'s size: the
/// rectangle, or why a window region has none.
fn search_region(
    screen: (i32, i32),
    opts: Option<&Table>,
    fname: &str,
) -> mlua::Result<Result<(i32, i32, i32, i32), HostError>> {
    let v = match opts {
        Some(o) => o.get::<Value>("region")?,
        None => Value::Nil,
    };
    Ok(crate::region_arg_on(screen, &v, fname, "opts.region")?.map(|r| (r.x, r.y, r.w, r.h)))
}

/// `opts.region` of a search on a snapshot: read with the snapshot's edges as its defaults (a
/// region left out is the whole snapshot), then cut to the snapshot — the rectangle, or why
/// there is nothing of it to read (a window region with none this time, or no overlap).
fn snapshot_region(opts: Option<&Table>, fname: &str, frame: &Frame) -> mlua::Result<Result<(i32, i32, i32, i32), HostError>> {
    Ok(snapshot::opts_region_in(opts, fname, frame)?.and_then(|r| snapshot::clip(frame, r)).map(|r| r.tuple()))
}

// ── host.screen.imageSearch and imageSearchAll ──────────────────────────────────────────────

/// The two searches a handler waits for, or a callback is handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Search {
    /// `imageSearch`: the first match of one template, or of the first template of a list that
    /// matches.
    One,
    /// `imageSearchAll`: every match of one template, at its own size.
    All,
}

impl Search {
    /// The call, as its messages and log lines name it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Search::One => "host.screen.imageSearch",
            Search::All => "host.screen.imageSearchAll",
        }
    }

    /// The call as a stop of its VM while its answer is built names it, and as its callback's
    /// keys and errors do.
    fn stop(self) -> &'static str {
        match self {
            Search::One => "screen.imageSearch",
            Search::All => "screen.imageSearchAll",
        }
    }

    /// The call as a template it cannot take names it, as it always has: `imageSearch: expected a
    /// path or a Template, …`.
    fn short(self) -> &'static str {
        match self {
            Search::One => "imageSearch",
            Search::All => "imageSearchAll",
        }
    }

    /// How the worker answers it.
    fn mode(self) -> Mode {
        match self {
            Search::One => Mode::First,
            Search::All => Mode::All,
        }
    }
}

/// What a search asks, its arguments read ([`search_ask`]).
pub(crate) struct SearchAsk {
    /// Each template, in order — one but for `imageSearch` given a list — and the name its hit
    /// reports.
    templates: Vec<(Needle, Option<String>)>,
    /// Where to look — or why there is nothing to look at this time (a window region with no
    /// rectangle; on a snapshot, a region with no part inside it), answered at once.
    region: Result<(i32, i32, i32, i32), HostError>,
    /// The snapshot to search instead of the screen.
    snap: Option<Arc<Frame>>,
    tol: u8,
    scales: Vec<f32>,
    /// `opts.key`, read only for the callback form.
    key: Option<String>,
}

/// A search's arguments, read as the call has always read them, before anything is captured: the
/// type of `opts`; then the templates — `imageSearch` takes one, or a list of them tried in order
/// against one frame, which may not be empty; every path of the package checked through `locate` the
/// first time it is named, so a bad one raises wherever it sits in a list, and opened on the image
/// worker; then
/// `opts.snapshot`, then `opts.region`, resolved now against a primary screen of `screen`'s size;
/// then the tolerance and, but for `imageSearchAll`, which matches at the template's own size only,
/// the scales; then `opts.key`, which only the callback form (`cb`) takes. A mistake raises; a region
/// with nothing to look at this time is answered.
pub(crate) fn search_ask(
    kind: Search,
    tv: Value,
    opts: Value,
    screen: (i32, i32),
    locate: &dyn Fn(&str) -> mlua::Result<PathBuf>,
    cb: bool,
) -> mlua::Result<SearchAsk> {
    let fname = kind.name();
    let opts = match opts {
        Value::Nil => None,
        Value::Table(t) => Some(t),
        other => return Err(mlua::Error::external(format!("{fname}: opts must be a table, got {}", crate::describe_value(&other)))),
    };
    let templates = match (kind, &tv) {
        // A list: several renderings of one thing, matched in order against the same frame.
        (Search::One, Value::Table(list)) => {
            let templates = resolve_list(list, kind.short(), locate)?;
            if templates.is_empty() {
                return Err(mlua::Error::external(format!("{fname}: no template given; the list is empty")));
            }
            templates
        }
        _ => vec![resolve_with(&tv, kind.short(), locate)?],
    };
    let o = opts.as_ref();
    let snap = snapshot::from_opts(o, fname)?;
    let region = match &snap {
        Some(f) => snapshot_region(o, fname, f)?,
        None => search_region(screen, o, fname)?,
    };
    let scales = if kind == Search::All { Vec::new() } else { read_scales(o) };
    let key = snapshot::opts_key(o, fname, cb, "the third argument")?;
    Ok(SearchAsk { templates, region, snap, tol: read_tol(o), scales, key })
}

/// What a search answers — to a handler that waited and to a callback alike, in the shape the call
/// has always had. `imageSearch`: the hit, `nil` alone when it looked and
/// found nothing, `nil` and the error value when it could not look. `imageSearchAll`: the list of
/// hits, never `nil` — empty and alone when it found none, empty with the error value after it when
/// it could not look. `names` is the name each template's hit reports.
fn search_values(lua: &Lua, kind: Search, outcome: Outcome, names: &[Option<String>]) -> mlua::Result<MultiValue> {
    let found = match outcome {
        Outcome::Search(found) => found,
        // Never: the worker answers a search with a search.
        _ => Err(failed(JobKind::Search)),
    };
    let hit = |h: &ScreenHit| hit_table(lua, h.x, h.y, h.w, h.h, h.n, names.get(h.n - 1).and_then(|n| n.as_deref()));
    match (kind, found) {
        (Search::One, Ok(v)) => match v.into_iter().next().flatten() {
            Some(h) => one_value(Value::Table(hit(&h)?)),
            // Looked, and it is not there: `nil` alone, as always.
            None => one_value(Value::Nil),
        },
        (Search::One, Err(why)) => with_reason(lua, Value::Nil, why),
        (Search::All, Ok(v)) => {
            let list = lua.create_table_with_capacity(v.len(), 0)?;
            for (i, h) in v.iter().flatten().enumerate() {
                list.raw_set(i + 1, hit(h)?)?;
            }
            one_value(Value::Table(list))
        }
        // The list is never `nil`: empty, and the error value after it.
        (Search::All, Err(why)) => with_reason(lua, Value::Table(lua.create_table()?), why),
    }
}

/// The work of a search of `region`: its templates — opened, and made, by the image worker —
/// matched against the region of the screen — or of its snapshot — in order, the first that
/// matches answering, or every match for `imageSearchAll`, the module's first-read comparison with
/// it when it is owed. The names stay on this side, for the answer.
fn search_work(lua: &Lua, kind: Search, ask: SearchAsk, region: (i32, i32, i32, i32)) -> (Haystack, Job, Vec<Option<String>>) {
    let SearchAsk { templates, snap, tol, scales, .. } = ask;
    let (entries, names): (Vec<_>, Vec<_>) = templates.into_iter().map(|(d, n)| ((d, None), n)).unzip();
    let (hay, _) = haystack(lua, snap, Ok(region));
    (hay, Job::Search { entries, tol, scales, mode: kind.mode(), unresolved: None }, names)
}

/// A search's plan ([`Plan`]): answered at once when there is nothing to look at this time;
/// otherwise a search of the image worker, which captures the region through the module's source,
/// with the first-read comparison when it is owed, or reads its snapshot, and matches
/// ([`search_values`] answers it).
fn search_plan(lua: &Lua, kind: Search, ask: SearchAsk) -> mlua::Result<Plan> {
    let region = match &ask.region {
        Ok(r) => *r,
        // Nothing to look at this time: answered at once, wherever the call is made.
        Err(none) => {
            let otherwise: BuildAnswer = Box::new(move |lua, outcome| search_values(lua, kind, outcome, &[]));
            return Ok(Plan::Now(search_values(lua, kind, Outcome::Search(Err(none.clone())), &[])?, otherwise));
        }
    };
    let (hay, job, names) = search_work(lua, kind, ask, region);
    let answer: BuildAnswer = Box::new(move |lua, outcome| search_values(lua, kind, outcome, &names));
    Ok(Plan::Work { hay, job, answer })
}

/// A search's callback form, `imageSearch(template | list, opts, cb)`: its arguments read as the
/// call reads them, `opts.key` with them, and its plan handed to `cb` ([`submit_plan`]): it never
/// waits, wherever it is called.
fn submit_search<H: ReadHost>(h: &H, lua: &Lua, scope: usize, kind: Search, cb: Value, parse: impl FnOnce() -> mlua::Result<SearchAsk>) -> mlua::Result<()> {
    let cb = callback_arg(kind.name(), cb, "third argument")?;
    let mut ask = parse()?;
    let key = ask.key.take();
    let plan = search_plan(lua, kind, ask)?;
    submit_plan(h, lua, scope, cb, key, kind.stop(), JobKind::Search, plan)
}

/// The callback of `fname`, the argument `which` names: a function, or a raise naming what it got.
pub(crate) fn callback_arg(fname: &str, cb: Value, which: &str) -> mlua::Result<Function> {
    match cb {
        Value::Function(f) => Ok(f),
        other => Err(mlua::Error::external(format!(
            "{fname}: the callback ({which}) must be a function, got {}",
            crate::describe_value(&other)
        ))),
    }
}

/// `host.screen.imageSearch(template | list, opts?, cb?) -> (Hit?, Error?)`: `nil` alone when it
/// looked and found nothing, `nil` and the error value when it could not look; given a list, the first
/// template that matches answers, its `n` saying which. In a handler it waits while the image worker
/// captures and matches ([`search_plan`]); where it cannot wait it raises, unless there is nothing
/// to look at this time ([`now_plan`]). With a callback it never waits ([`submit_search`]).
///
/// A LIST is tried in order against the SAME captured frame, first hit wins: several renderings of
/// one thing (a toggle's on/off pair, a dialog's close glyph as two plugin versions draw it). One
/// capture serves every comparison — half the ~1-frame screen touches of two searches — and the
/// templates are matched against the same pixels, so a state that changes mid-repaint cannot fall
/// between two captures.
pub(crate) fn image_search(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    let start = lua.create_function(move |lua, (nonce, tv, opts): (Table, Value, Value)| {
        host_call!("host.screen.imageSearch");
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        let parse = || search_plan(lua, Search::One, search_ask(Search::One, tv, opts, sh.backend.screen_size(), &locate, false)?);
        start_plan(&*sh, lua, idx, Search::One.name(), Search::One.stop(), nonce, parse)
    })?;
    let sh = shared.clone();
    let now = lua.create_function(move |lua, (tv, opts): (Value, Value)| {
        host_call!("host.screen.imageSearch");
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        Ok(now_plan(search_plan(lua, Search::One, search_ask(Search::One, tv, opts, sh.backend.screen_size(), &locate, false)?)?))
    })?;
    let sh = shared.clone();
    let submit = lua.create_function(move |lua, (tv, opts, cb): (Value, Value, Value)| {
        host_call!("host.screen.imageSearch");
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        submit_search(&*sh, lua, idx, Search::One, cb, || search_ask(Search::One, tv, opts, sh.backend.screen_size(), &locate, true))
    })?;
    crate::task::points(lua, idx, shared.clone())?.call((Search::One.name(), start, now, submit, 3))
}

/// `host.screen.imageSearchAll(template, opts?) -> ({ Hit }, Error?)` — EVERY match of the
/// template in the region, not just the first.
///
/// For deciding whether a template is safe to click blindly. A close-glyph template that
/// matches twice will eventually click the wrong one, and a template that matches nowhere is
/// a control that silently never fires; "matched exactly once, here" is the answer you want
/// before shipping either.
///
/// The list is never `nil`, as it never was: when the region could not be read it is empty
/// and the error value comes as a SECOND value, so "no match" and "could not look" can be told
/// apart by a caller that asks, and one that does not reads what it always did. It waits in a
/// handler, and raises where it cannot wait, as `imageSearch` does; with a callback it never waits.
pub(crate) fn image_search_all(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    let start = lua.create_function(move |lua, (nonce, tv, opts): (Table, Value, Value)| {
        host_call!("host.screen.imageSearchAll");
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        let parse = || search_plan(lua, Search::All, search_ask(Search::All, tv, opts, sh.backend.screen_size(), &locate, false)?);
        start_plan(&*sh, lua, idx, Search::All.name(), Search::All.stop(), nonce, parse)
    })?;
    let sh = shared.clone();
    let now = lua.create_function(move |lua, (tv, opts): (Value, Value)| {
        host_call!("host.screen.imageSearchAll");
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        Ok(now_plan(search_plan(lua, Search::All, search_ask(Search::All, tv, opts, sh.backend.screen_size(), &locate, false)?)?))
    })?;
    let sh = shared.clone();
    let submit = lua.create_function(move |lua, (tv, opts, cb): (Value, Value, Value)| {
        host_call!("host.screen.imageSearchAll");
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        submit_search(&*sh, lua, idx, Search::All, cb, || search_ask(Search::All, tv, opts, sh.backend.screen_size(), &locate, true))
    })?;
    crate::task::points(lua, idx, shared.clone())?.call((Search::All.name(), start, now, submit, 3))
}

/// `imageSearch` and `imageSearchAll` as the host builds them, over `holder`, their waits and where
/// they cannot wait the host's own ([`search_plan`], [`now_plan`]), and their callback forms
/// ([`submit_search`]), and every template path is resolved against `root`: for the tests'
/// holders, which have no backend. The primary screen is 1920x1080.
#[cfg(test)]
pub(crate) fn test_searches<H: ReadHost>(lua: &Lua, idx: usize, holder: Rc<H>, root: PathBuf) -> mlua::Result<[Function; 2]> {
    let build = |kind: Search| -> mlua::Result<Function> {
        let (h, r) = (holder.clone(), root.clone());
        let start = lua.create_function(move |lua, (nonce, tv, opts): (Table, Value, Value)| {
            let locate = |p: &str| locate_in(&r, p);
            let parse = || search_plan(lua, kind, search_ask(kind, tv, opts, (1920, 1080), &locate, false)?);
            start_plan(&*h, lua, idx, kind.name(), kind.stop(), nonce, parse)
        })?;
        let r = root.clone();
        let now = lua.create_function(move |lua, (tv, opts): (Value, Value)| {
            let locate = |p: &str| locate_in(&r, p);
            Ok(now_plan(search_plan(lua, kind, search_ask(kind, tv, opts, (1920, 1080), &locate, false)?)?))
        })?;
        let (h, r) = (holder.clone(), root.clone());
        let submit = lua.create_function(move |lua, (tv, opts, cb): (Value, Value, Value)| {
            let locate = |p: &str| locate_in(&r, p);
            submit_search(&*h, lua, idx, kind, cb, || search_ask(kind, tv, opts, (1920, 1080), &locate, true))
        })?;
        crate::task::points(lua, idx, holder.clone())?.call((kind.name(), start, now, submit, 3))
    };
    Ok([build(Search::One)?, build(Search::All)?])
}

/// What a queued task reads, decided here on the main thread — the worker never sees a Lua
/// state: `region` of the snapshot when the call was given one (already cut to it or found
/// inside it at the call), which is never captured and never asks the module's source; else
/// `region` of the screen through this VM's source, with the VM's first-read comparison when it
/// is owed. `region` may instead be why there is nothing to read this time, answered on the worker
/// without a capture.
pub(crate) fn haystack(lua: &Lua, snap: Option<Arc<Frame>>, region: Result<(i32, i32, i32, i32), HostError>) -> (Haystack, Option<HostError>) {
    match (snap, region) {
        (Some(frame), Ok(r)) => (Haystack::Frame { frame, region: geo::Rect::from_tuple(r) }, None),
        (Some(frame), Err(why)) => (Haystack::Frame { frame, region: geo::Rect::default() }, Some(why)),
        (None, Ok(r)) => (
            Haystack::Screen {
                region: r,
                source: capture_source::vm_source(lua),
                compare: capture_source::take_compare(lua, r),
            },
            None,
        ),
        (None, Err(why)) => {
            (Haystack::Screen { region: (0, 0, 0, 0), source: capture_source::vm_source(lua), compare: None }, Some(why))
        }
    }
}

// ── host.screen.cells and matchCells wait ──────────────────────────────────────────────────

/// What a call of `host.screen.cells` or `matchCells` asks, its arguments read (`crate::cells_ask`).
pub(crate) struct CellsAsk {
    /// Where to read — or why there is nothing to read this time (an empty client area; on a
    /// snapshot, a region not wholly inside it), answered at once.
    pub(crate) rect: Result<ScreenRect, HostError>,
    /// The grid, its colour test and, for `matchCells`, the states.
    pub(crate) matcher: cells::Matcher,
    /// The states' names, for the match's answer; none for `cells`.
    pub(crate) names: Vec<Option<String>>,
    /// `matchCells`: ranked against the states; `cells`: the grid alone.
    pub(crate) rank: bool,
    /// The snapshot to read instead of the screen.
    pub(crate) snapshot: Option<Arc<Frame>>,
    /// `opts.key`, which only the callback form takes.
    pub(crate) key: Option<String>,
}

/// What a cells call answers — to a handler that waited and to a callback alike, two values as
/// these calls always returned: `cells`' `{ cells, x, y, w, h }` or `matchCells`' match, then
/// `nil`; or `nil` and the error value.
fn cells_values(lua: &Lua, outcome: Outcome, r: ScreenRect, names: &[Option<String>]) -> mlua::Result<MultiValue> {
    let (first, second) = match outcome {
        Outcome::Cells(Ok((live, Some(ranked)))) => (Value::Table(cells_match_table(lua, &live, r, &ranked, names)?), Value::Nil),
        Outcome::Cells(Ok((live, None))) => (Value::Table(cells_table(lua, &live, r)?), Value::Nil),
        Outcome::Cells(Err(why)) => return with_reason(lua, Value::Nil, why),
        // Never: the worker answers a cells task with cells.
        _ => return with_reason(lua, Value::Nil, failed(JobKind::Cells)),
    };
    Ok(MultiValue::from_vec(vec![first, second]))
}

/// A cells call's plan ([`Plan`]): answered at once when there is nothing to read this time;
/// otherwise a cells task of the image worker, which reads the region through the module's source,
/// with the first-read comparison when it is owed, or where it lies in the snapshot, and reduces —
/// and for `matchCells` ranks — it ([`cells_values`] answers it).
pub(crate) fn cells_plan(lua: &Lua, ask: CellsAsk) -> mlua::Result<Plan> {
    let r = match ask.rect {
        Ok(r) => r,
        Err(why) => {
            const NONE: ScreenRect = ScreenRect { x: 0, y: 0, w: 0, h: 0 };
            let otherwise: BuildAnswer = Box::new(|lua, outcome| cells_values(lua, outcome, NONE, &[]));
            return Ok(Plan::Now(cells_values(lua, Outcome::Cells(Err(why)), NONE, &[])?, otherwise));
        }
    };
    let (hay, _) = haystack(lua, ask.snapshot, Ok((r.x, r.y, r.w, r.h)));
    let job = Job::Cells(Arc::new(CellsJob { matcher: Arc::new(ask.matcher), unresolved: None, rank: ask.rank }));
    let names = ask.names;
    let answer: BuildAnswer = Box::new(move |lua, outcome| cells_values(lua, outcome, r, &names));
    Ok(Plan::Work { hay, job, answer })
}

/// The name a cells call's wait, callback and stop go by: `screen.matchCells` or `screen.cells`.
pub(crate) fn cells_stop(rank: bool) -> &'static str {
    if rank {
        "screen.matchCells"
    } else {
        "screen.cells"
    }
}

/// `host.screen.cells` and `matchCells` as the host builds them (lib.rs, `cells_ask`), over
/// `holder`, their waits and where they cannot wait the host's own ([`cells_plan`], [`now_plan`]),
/// and their callback forms: for the tests' holders, which have no backend. The pair, `cells`
/// first.
#[cfg(test)]
pub(crate) fn test_cells<H: ReadHost>(lua: &Lua, idx: usize, holder: Rc<H>) -> mlua::Result<(Function, Function)> {
    let build = |call: &'static str, rank: bool| -> mlua::Result<Function> {
        let h = holder.clone();
        let start = lua.create_function(move |lua, (nonce, opts, states): (Table, Value, Value)| {
            let parse = || cells_plan(lua, crate::cells_ask(lua, call, opts, rank.then_some(states), false)?);
            start_plan(&*h, lua, idx, call, cells_stop(rank), nonce, parse)
        })?;
        let now = lua.create_function(move |lua, (opts, states): (Value, Value)| {
            Ok(now_plan(cells_plan(lua, crate::cells_ask(lua, call, opts, rank.then_some(states), false)?)?))
        })?;
        let points = crate::task::points(lua, idx, holder.clone())?;
        let h = holder.clone();
        if !rank {
            let submit = lua.create_function(move |lua, (opts, cb): (Value, Value)| {
                let cb = callback_arg(call, cb, "second argument")?;
                let mut ask = crate::cells_ask(lua, call, opts, None, true)?;
                let key = ask.key.take();
                submit_plan(&*h, lua, idx, cb, key, cells_stop(false), JobKind::Cells, cells_plan(lua, ask)?)
            })?;
            return points.call((call, start, now, submit, 2));
        }
        let submit = lua.create_function(move |lua, (opts, states, cb): (Value, Value, Value)| {
            let cb = callback_arg(call, cb, "third argument")?;
            let mut ask = crate::cells_ask(lua, call, opts, Some(states), true)?;
            let key = ask.key.take();
            let plan = cells_plan(lua, ask)?;
            submit_plan(&*h, lua, idx, cb, key, cells_stop(true), JobKind::Cells, plan)
        })?;
        points.call((call, start, now, submit, 3))
    };
    Ok((build("host.screen.cells", false)?, build("host.screen.matchCells", true)?))
}

/// `host.screen.imageSearchEach(entries, opts?, cb)` — ONE capture, and an answer for every
/// entry: `cb(list)` with a hit or `false` in each slot, or `cb(nil, err)` when the region
/// could not be captured; then `info`, as every callback of host.screen gets it.
///
/// For "which of these states is showing": a menu with a cursor that can be in one of several
/// places, a panel that shows one of several pages. `imageSearch` with a list answers only the
/// first match; separate calls share a frame only when they happen to land in the worker's 5 ms
/// batch window, which is an implementation detail. Here one frame is the contract, and each
/// entry may look only inside its own `within` part of it — a state's own region, so a template
/// is neither slower nor more likely to invent a hit elsewhere.
pub(crate) fn image_search_each(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> mlua::Result<Function> {
    let sh = shared.clone();
    lua.create_function(move |lua, (list, opts, cb): (Table, Option<Table>, Value)| {
        host_call!("host.screen.imageSearchEach");
        const F: &str = "host.screen.imageSearchEach";
        let cb = callback_arg(F, cb, "third argument")?;
        let locate = |p: &str| locate_in(&sh.root(idx), p);
        let snap = snapshot::from_opts(opts.as_ref(), F)?;
        let region = match &snap {
            Some(f) => snapshot_region(opts.as_ref(), F, f)?,
            None => search_region(sh.backend.screen_size(), opts.as_ref(), F)?,
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
                    let (needle, own) = resolve_with(&tv, &what, &locate)?;
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
                    entries.push((needle, within));
                    names.push(name);
                }
                other => {
                    let (needle, name) = resolve_with(&other, &format!("{F}: entries[{}]", i + 1), &locate)?;
                    entries.push((needle, None));
                    names.push(name);
                }
            }
        }
        if entries.is_empty() {
            return Err(mlua::Error::external("imageSearchEach: no template given".to_string()));
        }
        let key = snapshot::opts_key(opts.as_ref(), F, true, "the third argument")?;
        let o = opts.as_ref();
        let (tol, scales) = (read_tol(o), read_scales(o));
        let stop = "screen.imageSearchEach";
        match region {
            // Nothing to look at this time: answered on the next tick, as every callback is.
            Err(why) => {
                let otherwise: BuildAnswer = Box::new(|lua, outcome| each_values(lua, outcome, &[]));
                submit_now(&*sh, lua, idx, cb, key, stop, JobKind::Search, each_values(lua, Outcome::Search(Err(why)), &[])?, otherwise)
            }
            Ok(r) => {
                let (hay, _) = haystack(lua, snap, Ok(r));
                let job = Job::Search { entries, tol, scales, mode: Mode::Each, unresolved: None };
                let answer: BuildAnswer = Box::new(move |lua, outcome| each_values(lua, outcome, &names));
                submit_callback(&*sh, lua, idx, cb, key, stop, hay, job, answer)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use crate::errors_lua::Told;
    use mlua::FromLua;

    fn opaque(w: u32, h: u32, rgb: [u8; 3]) -> CapturedImage {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        CapturedImage { w, h, rgba }
    }

    fn no_file(p: &str) -> mlua::Result<Result<FileHeader, HostError>> {
        Err(mlua::Error::external(format!("no file {p} in a test")))
    }

    /// What a read desktop duplication could not answer gets under `fallback = "none"`: `busy`.
    fn overdue() -> HostError {
        HostError::with(&screen::DUPLICATION_OVERDUE, screen::DUPLICATION_OVERDUE.message.to_string())
    }

    /// The error value `v` of `lua`, as a test compares it.
    fn told(lua: &Lua, v: &Value) -> Told {
        Told::from_lua(v.clone(), lua).unwrap()
    }

    /// The image worker, as these tests stand in for it: a handle's picture made the moment it is
    /// queued.
    fn at_once(p: Arc<Prepared>) {
        let _ = p.make();
    }

    /// No file, and every handle's picture made at once.
    fn making() -> Making<'static> {
        Making { file: &no_file, prepare: &at_once }
    }

    /// `host.screen.template` of `s` as a handler's call answers it, the image worker's work done
    /// here: a spec that captures nothing answered at once ([`template_plan`]), a capture read
    /// through `capture` — or cut from its snapshot — and made a template as the worker makes it
    /// ([`answer`]).
    fn made(lua: &Lua, s: ParsedSpec, capture: impl FnOnce(i32, i32, i32, i32) -> Result<CapturedImage, HostError>) -> mlua::Result<MultiValue> {
        match template_plan(lua, s, &making())? {
            Plan::Now(values, _) => Ok(values),
            Plan::Work { hay, job, answer: build } => {
                let t = ImageTask { id: 0, hay, job, prio: crate::ocr::types::current_priority() };
                let shot = t.screen_key().map(|((x, y, w, h), _)| capture(x, y, w, h));
                build(lua, answer(&t, shot.as_ref().map(|r| r.as_ref()), None))
            }
        }
    }

    /// The picture of handle `v` and its count, made as the worker makes it.
    fn picture_of(v: &Value) -> (Arc<Decoded>, u32) {
        let Value::UserData(ud) = v else { panic!("expected a handle") };
        let h = ud.borrow::<TemplateHandle>().unwrap();
        h.pic.make().expect("a picture")
    }

    fn rgba_of(d: &Decoded) -> Vec<u8> {
        let template::Body::Dense { rgba, .. } = &d.body;
        rgba.clone()
    }

    /// A folder of the test's own, empty.
    fn test_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("host-image-search-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn spec(lua: &Lua, src: &str) -> mlua::Result<ParsedSpec> {
        let t: Table = lua.load(src).eval()?;
        parse_spec(&t, 1920, 1080, false)
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
            .and_then(|s| build_handle(&lua, s, &making())));
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
        let got = made(&lua, s, |x, y, w, h| {
            assert_eq!((x, y, w, h), (200, 100, 100, 50), "the right lower quarter of the client area");
            Ok(opaque(w as u32, h as u32, [1, 2, 3]))
        })
        .unwrap();
        assert_eq!(got.len(), 1, "a handle alone");
        // A minimised window: nothing captured, `nil` and why.
        let s = spec(&lua, "return { capture = { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } } }").unwrap();
        let got = made(&lua, s, |_, _, _, _| panic!("nothing to capture")).unwrap().into_vec();
        assert!(got[0].is_nil());
        assert_eq!(told(&lua, &got[1]), Told("hidden".into(), "the window's client area is empty (0x0)".into()));
        // A window so large that the region is past a template's limit: the window's doing, so
        // answered, where corners past the limit are the module's mistake and raise.
        let s = spec(&lua, "return { capture = { window = { client = { x = 0, y = 0, w = 5000, h = 10 } }, fraction = { 0, 0, 1, 1 } } }").unwrap();
        let got = made(&lua, s, |_, _, _, _| panic!("nothing to capture")).unwrap().into_vec();
        assert!(got[0].is_nil());
        assert_eq!(
            told(&lua, &got[1]),
            Told("too_large".into(), "at the window's current size 5000x10 is too large; no side may exceed 4096".into()),
            "the size named once"
        );
        let s = spec(&lua, "return { capture = { window = { client = { x = 0, y = 0, w = 2000, h = 1000 } }, fraction = { 0, 0, 1, 1 } } }").unwrap();
        let got = made(&lua, s, |_, _, _, _| panic!("nothing to capture")).unwrap().into_vec();
        assert_eq!(
            told(&lua, &got[1]),
            Told("too_large".into(), "at the window's current size 2000x1000 is 2000000 pixels; the limit is 1048576".into()),
            "past the pixel limit"
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

    /// A handle from bytes is answered at once, its picture still to be made: its size and name are
    /// there, its `count` is `nil` and `tostring` says it is preparing. The picture is made off the
    /// call — by the worker, queued as the handle is made — and then the handle has it, the bytes
    /// of a string or a buffer alike, converted to RGBA there.
    #[test]
    fn strings_and_buffers_both_carry_bytes() {
        let lua = Lua::new();
        for src in [
            "return { rgb = '\\1\\2\\3\\4\\5\\6', w = 2, h = 1, name = 'pair' }",
            "local b = buffer.create(6) for i = 0, 5 do buffer.writeu8(b, i, i + 1) end \
             return { rgb = b, w = 2, h = 1, name = 'pair' }",
        ] {
            let s = spec(&lua, src).unwrap();
            let queued: RefCell<Vec<Arc<Prepared>>> = RefCell::new(Vec::new());
            let later = |p: Arc<Prepared>| queued.borrow_mut().push(p);
            let got = build_handle(&lua, s, &Making { file: &no_file, prepare: &later }).unwrap();
            assert_eq!(got.len(), 1, "a handle is the one value, as it always was");
            lua.globals().set("t", got.into_vec().remove(0)).unwrap();
            let read = || -> (u32, u32, String, Option<u32>, String) { lua.load("return t.w, t.h, t.name, t.count, tostring(t)").eval().unwrap() };
            assert_eq!(read(), (2, 1, "pair".to_string(), None, "Template(pair 2x1, preparing)".to_string()), "nothing made at the call");
            assert_eq!(queued.borrow().len(), 1, "its preparation queued");
            let _ = queued.borrow()[0].make();
            assert_eq!(read(), (2, 1, "pair".to_string(), Some(2), "Template(pair 2x1, 2 compared)".to_string()));
            assert_eq!(rgba_of(&picture_of(&lua.globals().get("t").unwrap()).0), [1, 2, 3, 255, 4, 5, 6, 255]);
        }
    }

    #[test]
    fn a_failed_capture_is_nil_and_a_good_one_is_opaque() {
        let lua = Lua::new();
        let s = spec(&lua, "return { capture = { 10, 20, 14, 23 } }").unwrap();
        let got = made(&lua, s, |_, _, _, _| Err(crate::backend::capture_failed())).unwrap().into_vec();
        assert!(got[0].is_nil(), "a failed capture answers nil");
        assert_eq!(told(&lua, &got[1]), Told::of(&crate::backend::capture_failed()), "and says why");
        assert_eq!(budget(&lua).get(), 0, "and gives its reservation back");

        let s = spec(&lua, "return { capture = { 10, 20, 14, 23 }, name = 'here' }").unwrap();
        let v = made(&lua, s, |x, y, w, h| {
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
        let (dec, count) = picture_of(&v);
        assert_eq!((dec.w, dec.h, count), (4, 3, 12), "every captured pixel is compared");
        let Value::UserData(ud) = v else { panic!("expected a handle") };
        assert!(ud.borrow::<TemplateHandle>().unwrap().pic.get().is_some(), "made with the handle, not queued");
    }

    /// A handle from a file: its size from the PNG's header at the call, nothing decoded there; its
    /// picture the image worker's cache's decode, shared, so it costs the VM nothing. A path that
    /// is not there raises at the call.
    #[test]
    fn a_file_handle_shares_the_cached_decode_and_costs_nothing() {
        let lua = Lua::new();
        let root = test_dir("file-handle");
        std::fs::create_dir_all(root.join("images")).unwrap();
        let path = root.join("images").join("x.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([5, 6, 7, 255])).save(&path).unwrap();
        let file = |p: &str| template_file(&root, p);
        let queued: RefCell<Vec<Arc<Prepared>>> = RefCell::new(Vec::new());
        let later = |p: Arc<Prepared>| queued.borrow_mut().push(p);
        let making = Making { file: &file, prepare: &later };
        let s = spec(&lua, "return { file = 'images/x.png' }").unwrap();
        let v = build_handle(&lua, s, &making).unwrap().into_vec().remove(0);
        assert!(template_store::test_made_on(&path.display().to_string()).is_empty(), "nothing decoded at the call");
        lua.globals().set("t", v.clone()).unwrap();
        let (w, h, count): (u32, u32, Option<u32>) = lua.load("return t.w, t.h, t.count").eval().unwrap();
        assert_eq!((w, h, count), (3, 2, None), "its size from the header");
        let _ = queued.borrow()[0].make();
        let (dec, count) = picture_of(&v);
        assert!(Arc::ptr_eq(&dec, &template_store::open(&path).unwrap()), "the cache's decode");
        assert_eq!(count, 6);
        let Value::UserData(ud) = &v else { panic!("expected a handle") };
        assert_eq!(ud.borrow::<TemplateHandle>().unwrap().name.as_deref(), Some("images/x.png"), "unnamed, it is named by its path");
        assert_eq!(budget(&lua).get(), 0);
        let s = spec(&lua, "return { file = 'images/missing.png' }").unwrap();
        let e = err_text(build_handle(&lua, s, &making));
        assert!(e.contains("cannot open template '") && e.contains("missing.png"), "{e}");
        // A file gone between the call and the making: the handle says why, and the next making
        // — a search's — tries again, and has the picture once the file is back.
        let back = root.join("images").join("back.png");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255])).save(&back).unwrap();
        let s = spec(&lua, "return { file = 'images/back.png' }").unwrap();
        let v = build_handle(&lua, s, &making).unwrap().into_vec().remove(0);
        lua.globals().set("b", v).unwrap();
        std::fs::remove_file(&back).unwrap();
        let _ = queued.borrow()[1].make();
        let (count, text): (Option<u32>, String) = lua.load("return b.count, tostring(b)").eval().unwrap();
        assert_eq!(count, None);
        assert!(text.starts_with("Template(images/back.png 3x2, cannot be used: cannot open template '"), "{text}");
        image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255])).save(&back).unwrap();
        let _ = queued.borrow()[1].make();
        let (count, text): (Option<u32>, String) = lua.load("return b.count, tostring(b)").eval().unwrap();
        assert_eq!((count, text.as_str()), (Some(6), "Template(images/back.png 3x2, 6 compared)"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A spec that is too large is told so, before the budget is consulted.
    #[test]
    fn size_limits_come_before_the_budget() {
        let lua = Lua::new();
        let s = spec(&lua, "return { rgba = '', w = 4096, h = 4096 }").unwrap();
        let e = err_text(build_handle(&lua, s, &making()));
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
                let s = parse_spec(&t, 100, 100, false)?;
                build_handle(lua, s, &making())
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

    /// Past the budget the collector runs twice, and a handle that still does not fit is answered
    /// `nil, err` (`too_large`), for every form (D2 of the typed-errors plan): what the module
    /// holds now decides it.
    #[test]
    fn the_budget_collects_before_it_refuses() {
        let lua = Lua::new();
        let f = lua
            .create_function(|lua, t: Table| {
                let s = parse_spec(&t, 100, 100, false)?;
                build_handle(lua, s, &making())
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
        let (none, e): (Value, Told) = lua
            .load(
                "local bytes = string.rep('\\0', 1024 * 1024 * 4) held = {} \
                 for i = 1, 40 do local t, err = make({ rgba = bytes, w = 1024, h = 1024 }) \
                 if t == nil then return t, err end held[i] = t end",
            )
            .eval()
            .unwrap();
        assert!(none.is_nil());
        assert_eq!(e.0, "too_large");
        assert!(e.1.starts_with("this module's templates would hold ") && e.1.contains("32 MiB") && e.1.contains("once"), "{e:?}");
    }

    // ── owners, generations and the purge ──────────────────────────────────────────────

    /// A callback's search `id`, as `imageSearch(template, opts, cb)` files it: owned by the VM
    /// `lua` runs in, at the priority of the dispatch that asks.
    fn entry(lua: &Lua, gens: &HashMap<usize, u64>, id: u64, scope: usize) -> PendingImage {
        let cb: Function = lua.load("return function() end").eval().unwrap();
        test_pending(&ImageState::detached(), lua, gens, id, scope, cb)
    }

    /// The bug this fixes: a search made by a code dependency's code (scope = the dependency)
    /// inside a dependent's VM belongs to the DEPENDENT: its answer goes to the dependent's mailbox,
    /// which a disable of the dependency does not touch.
    #[test]
    fn a_dependency_search_belongs_to_the_vm_that_ran_it() {
        let lua = Lua::new();
        let _ = lua.try_set_app_data(VmOwner { idx: 4, gen: 11 });
        // The dependency's generation is recorded too; it must not be the one taken.
        let gens: HashMap<usize, u64> = [(1, 2), (4, 11)].into_iter().collect();
        let p = entry(&lua, &gens, 1, 1);
        assert_eq!((p.scope, p.owner, p.gen), (1, 4, 11), "scope 1, owned by the VM of module 4");
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
    /// whose index has no generation either is generation 0, which no VM has, so the mailbox never
    /// delivers its answer.
    #[test]
    fn an_untagged_state_falls_back_to_its_scope() {
        let lua = Lua::new();
        let gens: HashMap<usize, u64> = [(1, 5)].into_iter().collect();
        assert_eq!(owner_of(&lua, &gens, 1), (1, 5));
        assert_eq!(owner_of(&lua, &gens, 2), (2, 0));
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
        let gone = purge_owner(&mut map, 3);
        let mut ids: Vec<u64> = gone.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![2, 3]);
        release(gone);
        let mut left: Vec<u64> = map.keys().copied().collect();
        left.sort_unstable();
        assert_eq!(left, vec![1, 4], "the scope (9) is not what a purge matches on");
        assert!(purge_owner(&mut map, 9).is_empty());
    }

    /// Past the cap, the module's oldest request is the one ended — other modules' never touched —
    /// and within it nothing is.
    #[test]
    fn past_the_cap_the_modules_oldest_request_is_ended() {
        let lua = Lua::new();
        let mut map: HashMap<u64, PendingImage> = HashMap::new();
        for id in 1..=3u64 {
            let mut p = entry(&lua, &HashMap::new(), id, 9);
            p.owner = 4;
            map.insert(id, p);
        }
        let mut other = entry(&lua, &HashMap::new(), 7, 9);
        other.owner = 5;
        map.insert(7, other);
        assert_eq!(oldest_over_cap(&map, 4, 3), None, "at the cap, nothing is ended");
        let mut newer = entry(&lua, &HashMap::new(), 8, 9);
        newer.owner = 4;
        map.insert(8, newer);
        assert_eq!(oldest_over_cap(&map, 4, 3), Some(1), "its oldest");
        assert_eq!(oldest_over_cap(&map, 5, 0), Some(7), "another module is counted on its own");
        release(map.into_values());
        assert!(too_many_waiting().message().contains(&IMAGE_PER_OWNER.to_string()));
        assert_eq!(too_many_waiting().reason(), &screen::IMAGE_TOO_MANY);
    }

    /// A batch as the worker runs it, its pictures stamped with input epoch 0.
    fn batch(tasks: &[ImageTask], capture: CaptureFn) -> Vec<ImageResult> {
        run_batch(tasks, capture, &AtomicU64::new(0))
    }

    // ── the worker ─────────────────────────────────────────────────────────────────────

    impl ImageResult {
        /// A search's answer; a cells answer here is a mistake in the test.
        fn found(&self) -> Result<Vec<Option<ScreenHit>>, HostError> {
            match &self.outcome {
                Outcome::Search(f) => f.clone(),
                _ => panic!("another answer where a search's was expected"),
            }
        }
        fn cells(&self) -> &Result<(Vec<u8>, Option<cells::Ranked>), HostError> {
            match &self.outcome {
                Outcome::Cells(c) => c,
                _ => panic!("another answer where a cells answer was expected"),
            }
        }
        fn profile(&self) -> &Result<Box<Reduced>, HostError> {
            match &self.outcome {
                Outcome::Profile(p) => p,
                _ => panic!("another answer where a profile was expected"),
            }
        }
    }

    /// A 2x2 grid of "clearly red", ranked against an all-zero state and an all-255 one.
    fn cells_matching(id: u64, region: (i32, i32, i32, i32), unresolved: Option<HostError>) -> ImageTask {
        let spec = cells::CellSpec::new(2, 2, cells::Predicate::parse("r > g + b").unwrap()).unwrap();
        let states: Vec<Box<[u8]>> = vec![vec![0u8; 4].into(), vec![255u8; 4].into()];
        let matcher = cells::Matcher { spec, states, item_of: vec![0, 1] };
        let job = Job::Cells(Arc::new(CellsJob { matcher: Arc::new(matcher), unresolved, rank: true }));
        ImageTask { id, hay: Haystack::Screen { region, source: CaptureSource::Standard, compare: None }, job, prio: Priority::Background }
    }

    fn task(id: u64, region: (i32, i32, i32, i32), entries: Vec<(Needle, Option<Rect>)>, mode: Mode) -> ImageTask {
        ImageTask {
            id,
            hay: Haystack::Screen { region, source: CaptureSource::Standard, compare: None },
            job: Job::Search { entries, tol: 0, scales: Vec::new(), mode, unresolved: None },
            prio: Priority::Background,
        }
    }

    /// A screen task read through `source` instead.
    fn set_source(t: &mut ImageTask, source: CaptureSource) {
        let region = t.region();
        t.hay = Haystack::Screen { region, source, compare: None };
    }

    /// A handle's picture of one pixel of `rgb`, made already.
    fn dot(rgb: [u8; 3]) -> Needle {
        Needle::Handle(Prepared::made(Arc::new(Decoded::from_png_rgba(1, 1, vec![rgb[0], rgb[1], rgb[2], 255])), 1))
    }

    /// A frame of grey with one red and one green pixel, wherever it is asked for.
    fn two_dots_at(w: i32, h: i32) -> CapturedImage {
        let mut c = opaque(w as u32, h as u32, [50, 50, 50]);
        c.rgba[(2 * w as usize + 3) * 4..][..3].copy_from_slice(&[255, 0, 0]);
        c.rgba[(5 * w as usize + 8) * 4..][..3].copy_from_slice(&[0, 255, 0]);
        c
    }

    /// `two_dots_at` for every region asked, whichever source: the worker's capture routine.
    fn two_dots(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
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
        let out = batch(&[task(1, (100, 200, 10, 10), entries, Mode::Each)], two_dots);
        let found = out[0].found().expect("captured");
        assert_eq!(found[0], Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 }));
        assert_eq!(found[1], None);
        assert_eq!(found[2], None, "outside its within");
        assert_eq!(found[3], Some(ScreenHit { x: 108, y: 205, w: 1, h: 1, n: 4 }));
    }

    /// `imageSearchAll`'s mode answers every match of its one entry, each in screen coordinates and
    /// numbered 1: the grey of a frame with two coloured dots matches everywhere but at them, a row
    /// at a time; a colour that is not there is an empty list, which is not "could not look".
    #[test]
    fn all_answers_every_match_of_its_one_entry() {
        let out = batch(&[task(1, (100, 200, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::All)], two_dots);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 })]));
        let out = batch(&[task(1, (100, 200, 10, 10), vec![(dot([50, 50, 50]), None)], Mode::All)], two_dots);
        let hits = out[0].found().expect("captured");
        assert_eq!(hits.len(), 98);
        assert!(hits.iter().all(|h| h.is_some_and(|h| h.n == 1 && h.w == 1 && h.h == 1)));
        assert_eq!(hits[0], Some(ScreenHit { x: 100, y: 200, w: 1, h: 1, n: 1 }));
        assert!(!hits.contains(&Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 })), "not at the red dot");
        let out = batch(&[task(1, (0, 0, 10, 10), vec![(dot([0, 0, 255]), None)], Mode::All)], two_dots);
        assert_eq!(out[0].found(), Ok(Vec::new()));
    }

    #[test]
    fn first_answers_the_first_entry_that_matches() {
        let entries = vec![(dot([0, 0, 255]), None), (dot([0, 255, 0]), None), (dot([255, 0, 0]), None)];
        let out = batch(&[task(1, (0, 0, 10, 10), entries, Mode::First)], two_dots);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 8, y: 5, w: 1, h: 1, n: 2 })]));
    }

    #[test]
    fn a_failed_capture_is_none_not_a_list_of_misses() {
        let entries = vec![(dot([255, 0, 0]), None)];
        let out = batch(&[task(1, (0, 0, 10, 10), entries, Mode::Each)], |r, _| r.iter().map(|_| Err(crate::backend::capture_failed())).collect());
        assert_eq!(out[0].found(), Err(crate::backend::capture_failed()), "could not look, and why");
        assert_eq!(out[0].seen, None, "no picture was read");
        let lua = Lua::new();
        let v = each_values(&lua, Outcome::Search(Err(crate::backend::capture_failed())), &[None]).unwrap().into_vec();
        assert!(v[0].is_nil());
        assert_eq!(told(&lua, &v[1]), Told::of(&crate::backend::capture_failed()));
        // ...while a capture that worked but matched nothing is a list of `false`.
        let v = each_values(&lua, Outcome::Search(Ok(vec![None, None])), &[None, None]).unwrap().into_vec();
        let Value::Table(t) = &v[0] else { panic!("expected a list") };
        assert_eq!(v.len(), 1, "the list alone");
        assert_eq!(t.raw_len(), 2);
        assert_eq!(t.raw_get::<Value>(1).unwrap(), Value::Boolean(false));
    }

    #[test]
    fn hits_carry_their_names() {
        let lua = Lua::new();
        let names = vec![Some("menu".to_string()), None];
        let hit = ScreenHit { x: 1, y: 2, w: 3, h: 4, n: 1 };
        let v = search_values(&lua, Search::One, Outcome::Search(Ok(vec![Some(hit)])), &names).unwrap().into_vec();
        let Value::Table(t) = &v[0] else { panic!("expected a hit") };
        assert_eq!(t.get::<String>("name").unwrap(), "menu");
        assert_eq!(t.get::<u32>("n").unwrap(), 1);
        let v = each_values(&lua, Outcome::Search(Ok(vec![None, Some(ScreenHit { n: 2, ..hit })])), &names).unwrap().into_vec();
        let Value::Table(list) = &v[0] else { panic!("expected a list") };
        let second: Table = list.raw_get(2).unwrap();
        assert!(second.get::<Value>("name").unwrap().is_nil(), "an unnamed handle has no name");
    }

    fn panics_at_666(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
        if regions.iter().any(|r| r.0 == 666) {
            panic!("capture exploded");
        }
        two_dots(regions, src)
    }

    /// Two sources, told apart by what they "see": the standard one the two dots, duplication a
    /// frame of plain grey, and a duplication read that could not answer under
    /// `fallback = "none"` nothing at all.
    fn by_source(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
        regions
            .iter()
            .map(|&(_, _, w, h)| match src {
                CaptureSource::Standard => Ok(two_dots_at(w, h)),
                CaptureSource::Duplication { or_standard: true } => Ok(opaque(w as u32, h as u32, [50, 50, 50])),
                CaptureSource::Duplication { or_standard: false } => {
                    Err(overdue())
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
        let out = batch(&[task(1, region, red(), Mode::First), dup, none], by_source);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        assert_eq!(out[1].found(), Ok(vec![None]), "not the standard frame of the same region");
        assert_eq!(out[2].found(), Err(overdue()), "no picture under fallback = \"none\" is could-not-look, with the reason, not a miss");
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
        spawn_image_worker(panics_at_666, crate::backend::no_comparison, Arc::new(AtomicU64::new(0)), task_rx, res_tx);
        task_tx.send(task(1, (666, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)).unwrap();
        let first = res_rx.recv_timeout(Duration::from_secs(10)).expect("an answer, not silence");
        assert_eq!(first.id, 1);
        assert_eq!(first.found(), Err(failed(JobKind::Search)), "a panicking batch could not look");
        task_tx.send(task(2, (0, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)).unwrap();
        let second = res_rx.recv_timeout(Duration::from_secs(10)).expect("the worker survived");
        assert_eq!(second.id, 2);
        assert_eq!(second.found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
    }

    /// A key's task does not share a batch with the polls queued before it: a batch is the tasks
    /// of the highest priority waiting, in the order they came, and the rest wait for the next one
    /// in theirs. On the worker, a key's task queued behind three polls is a batch of its own, and
    /// answered first.
    #[test]
    fn an_interactive_task_queued_behind_poll_tasks_forms_a_batch_of_its_own() {
        let red = || vec![(dot([255, 0, 0]), None)];
        let at = |id: u64, prio: Priority| ImageTask { prio, ..task(id, (0, 0, 10, 10), red(), Mode::First) };
        let ids = |b: &[ImageTask]| b.iter().map(|t| t.id).collect::<Vec<_>>();
        let mut waiting = vec![
            at(1, Priority::Background),
            at(2, Priority::Background),
            at(3, Priority::Interactive),
            at(4, Priority::Background),
            at(5, Priority::Interactive),
        ];
        assert_eq!(ids(&next_batch(&mut waiting)), [3, 5], "the keys' first, in the order they came");
        assert_eq!(ids(&waiting), [1, 2, 4], "the polls wait, in theirs");
        assert_eq!(ids(&next_batch(&mut waiting)), [1, 2, 4]);
        assert!(waiting.is_empty() && next_batch(&mut waiting).is_empty());
        // Queued before the worker takes any of them, so all four are waiting at its first look.
        let (task_tx, task_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        for t in [at(11, Priority::Background), at(12, Priority::Background), at(13, Priority::Background), at(14, Priority::Interactive)] {
            task_tx.send(t).unwrap();
        }
        spawn_image_worker(two_dots, crate::backend::no_comparison, Arc::new(AtomicU64::new(0)), task_rx, res_tx);
        let answers: Vec<ImageResult> = (0..4).map(|_| res_rx.recv_timeout(Duration::from_secs(10)).expect("an answer")).collect();
        assert_eq!((answers[0].id, answers[0].batch), (14, 1), "the key's task first, alone");
        assert_eq!(answers[1..].iter().map(|r| (r.id, r.batch)).collect::<Vec<_>>(), [(11, 3), (12, 3), (13, 3)]);
    }

    /// On Windows the matches run below normal priority: a task on the matching pool — one built
    /// as the global pool is, so that whatever another test did to the global pool first does not
    /// matter — and the image worker, where the capture runs, both see
    /// `THREAD_PRIORITY_BELOW_NORMAL`. The thread that asked is left as it was.
    #[cfg(windows)]
    #[test]
    fn the_matches_run_below_normal_priority() {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, GetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL,
        };
        // SAFETY: the calling thread's pseudo-handle.
        let priority = || unsafe { GetThreadPriority(GetCurrentThread()) };
        let pool = matching_pool().num_threads(2).build().expect("the pool is built");
        let seen: Vec<i32> = pool.install(|| (0..8).into_par_iter().map(|_| priority()).collect());
        assert!(seen.iter().all(|p| *p == THREAD_PRIORITY_BELOW_NORMAL), "{seen:?}");
        assert_eq!(priority(), THREAD_PRIORITY_NORMAL, "the asking thread keeps its priority");
        static AT_CAPTURE: Mutex<Option<i32>> = Mutex::new(None);
        fn capture(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            // SAFETY: the calling thread's pseudo-handle.
            *AT_CAPTURE.lock().unwrap() = Some(unsafe { GetThreadPriority(GetCurrentThread()) });
            two_dots(regions, src)
        }
        let (task_tx, task_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        spawn_image_worker(capture, crate::backend::no_comparison, Arc::new(AtomicU64::new(0)), task_rx, res_tx);
        task_tx.send(task(1, (0, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)).unwrap();
        res_rx.recv_timeout(Duration::from_secs(10)).expect("an answer");
        assert_eq!(*AT_CAPTURE.lock().unwrap(), Some(THREAD_PRIORITY_BELOW_NORMAL));
        // The keyboard hook's thread is none of these: it stays at the highest normal priority.
        const HOOK: &str = include_str!("backend/windows.rs");
        let at = HOOK.find("fn keyboard_hook_thread(").unwrap();
        assert!(HOOK[at..at + 200].contains("SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);"));
    }

    /// The first-read comparison a task carries is made on the worker, before the batch's
    /// capture — never on the thread that asked, which stands where the event loop stands.
    #[test]
    fn the_worker_makes_a_tasks_first_read_comparison_before_its_capture() {
        static SEEN: Mutex<Vec<(String, std::thread::ThreadId)>> = Mutex::new(Vec::new());
        fn compare(who: &str, _: (i32, i32, i32, i32)) -> bool {
            SEEN.lock().unwrap().push((format!("compared {who}"), std::thread::current().id()));
            true
        }
        fn capture(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            SEEN.lock().unwrap().push(("captured".to_string(), std::thread::current().id()));
            two_dots(regions, src)
        }
        let _loop = crate::loop_guard::mark_for_a_test();
        let (task_tx, task_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        spawn_image_worker(capture, compare, Arc::new(AtomicU64::new(0)), task_rx, res_tx);
        let slot = Arc::new(crate::backend::CompareSlot::new(true));
        let region = (0, 0, 10, 10);
        let mut t = task(1, region, vec![(dot([255, 0, 0]), None)], Mode::First);
        t.hay = Haystack::Screen { region, source: CaptureSource::Standard, compare: slot.take("image.worker", region) };
        task_tx.send(t).unwrap();
        let answer = res_rx.recv_timeout(Duration::from_secs(10)).expect("an answer");
        assert_eq!(answer.found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        assert_eq!(slot.state(), crate::backend::Compare::Done);
        let seen = SEEN.lock().unwrap().clone();
        assert_eq!(seen.iter().map(|(what, _)| what.as_str()).collect::<Vec<_>>(), ["compared image.worker", "captured"]);
        assert!(seen.iter().all(|(_, thread)| *thread != std::thread::current().id()), "on the worker");
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
        let empty = HostError::with(&screen::CLIENT_EMPTY, "the window's client area is empty (0x0)".into());
        let unresolved = cells_matching(3, (0, 0, 0, 0), Some(empty.clone()));
        let mut not_read = cells_matching(4, region, None);
        set_source(&mut not_read, CaptureSource::Duplication { or_standard: false });
        // Only the two frames that are read are asked for, once each: the unresolved task is
        // never captured.
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().map(|r| (*r, src))));
            by_source(regions, src)
        }
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<((i32, i32, i32, i32), CaptureSource)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let out = batch(&[search, cells_here, unresolved, not_read], record);
        assert_eq!(
            ASKED.with(|a| a.borrow().clone()),
            vec![(region, CaptureSource::Standard), (region, CaptureSource::Duplication { or_standard: false })]
        );
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        // The two dots on grey: only the red one passes, one pixel of the top-left 5x5 cell.
        let (live, ranked) = out[1].cells().as_ref().expect("read");
        assert_eq!(live, &vec![10, 0, 0, 0], "round(255 / 25)");
        assert_eq!(ranked.as_ref().expect("ranked").state, 0, "closer to all-zero than to all-255");
        assert_eq!(out[2].cells().as_ref().unwrap_err(), &empty);
        assert_eq!(out[3].cells().as_ref().unwrap_err(), &overdue());
        // A capture the wrong size is named too.
        let short = |regions: &[(i32, i32, i32, i32)], _: CaptureSource| -> Vec<Result<CapturedImage, HostError>> {
            regions.iter().map(|_| Ok(opaque(3, 3, [0, 0, 0]))).collect()
        };
        let out = batch(&[cells_matching(5, region, None)], short);
        let e = out[0].cells().as_ref().unwrap_err();
        assert_eq!((e.reason(), e.message()), (&screen::CAPTURE_SIZE, "the capture came back 3x3, not the region's 10x10"));
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

    fn scene_capture(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
        regions.iter().map(|&(x, y, w, h)| Ok(scene(x, y, w, h))).collect()
    }

    fn no_capture(_: &[(i32, i32, i32, i32)], _: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
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
            let live = batch(&[task(1, region, red(), Mode::First)], scene_capture);
            let snap = batch(&[on_frame(task(1, region, red(), Mode::First), &frame)], no_capture);
            assert_eq!(snap[0].found(), live[0].found(), "{region:?}");
            let entries = vec![
                (dot([255, 0, 0]), None),
                (dot([0, 255, 0]), None),
                (dot([0, 255, 0]), Some(within_frame(region, (106, 203, 200, 200)))),
                (dot([255, 0, 0]), Some(within_frame(region, (106, 203, 200, 200)))),
            ];
            let live = batch(&[task(2, region, entries.clone(), Mode::Each)], scene_capture);
            let snap = batch(&[on_frame(task(2, region, entries, Mode::Each), &frame)], no_capture);
            assert_eq!(snap[0].found(), live[0].found(), "{region:?}, each");
        }
        let whole = batch(&[on_frame(task(3, (100, 200, 20, 12), red(), Mode::First), &frame)], no_capture);
        assert_eq!(whole[0].found(), Ok(vec![Some(ScreenHit { x: 103, y: 202, w: 1, h: 1, n: 1 })]));
    }

    /// A snapshot's task never reaches the capture routine, costs no capture time, and does not
    /// take a frame from a screen task of the same region in its batch.
    #[test]
    fn a_frame_task_takes_no_capture() {
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<(i32, i32, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().copied()));
            scene_capture(regions, src)
        }
        let frame = scene_frame();
        let region = (100, 200, 20, 12);
        let red = || vec![(dot([255, 0, 0]), None)];
        let only = batch(&[on_frame(task(1, region, red(), Mode::First), &frame)], no_capture);
        assert_eq!(only[0].capture_ms, 0);
        let out = batch(&[on_frame(task(1, region, red(), Mode::First), &frame), task(2, region, red(), Mode::First)], record);
        assert_eq!(ASKED.with(|a| a.borrow().clone()), vec![region], "only the screen task is captured");
        assert_eq!(out[0].found(), out[1].found());
    }

    /// Hits on a snapshot are screen positions: the snapshot's corner plus where they lie in it,
    /// whatever part of it the region is.
    #[test]
    fn frame_task_hits_are_screen_coordinates() {
        let frame = scene_frame();
        let green = vec![(dot([0, 255, 0]), None)];
        let out = batch(&[on_frame(task(1, (105, 204, 5, 3), green, Mode::First), &frame)], no_capture);
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 108, y: 205, w: 1, h: 1, n: 1 })]));
        // A region of the snapshot the dot is not in: looked, and not there.
        let out = batch(&[on_frame(task(2, (110, 200, 5, 3), vec![(dot([0, 255, 0]), None)], Mode::First), &frame)], no_capture);
        assert_eq!(out[0].found(), Ok(vec![None]));
    }

    /// A profile's task — `host.screen.profile` with a callback — is the reduction of the picture
    /// it carries, where its region lies in it: exactly what the call without a callback answers
    /// for the same part of the same picture. It never reaches the capture routine, and a screen
    /// task of its batch is captured as if it were not there.
    #[test]
    fn a_profile_task_is_reduced_from_its_picture_without_a_capture() {
        let frame = scene_frame();
        let profile_of = |id: u64, region: geo::Rect| ImageTask {
            id,
            hay: Haystack::Frame { frame: frame.clone(), region },
            job: Job::Profile { cols: true, rows: true, dark: 128 },
            prio: Priority::Background,
        };
        for region in [geo::Rect::new(100, 200, 20, 12), geo::Rect::new(102, 201, 10, 8), geo::Rect::new(103, 202, 1, 1)] {
            let t = profile_of(1, region);
            assert!(!t.captures(), "nothing is captured for it");
            let out = batch(&[t], no_capture);
            let got = out[0].profile().as_ref().expect("reduced");
            let want = profile::profile(&frame.img, frame.area(region).unwrap(), true, true, 128).unwrap();
            assert_eq!((got.columns.clone(), got.rows.clone()), want, "{region:?}");
            assert_eq!(got.rect, region, "the part reduced, on screen");
            assert_eq!(out[0].capture_ms, 0);
        }
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<(i32, i32, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().copied()));
            scene_capture(regions, src)
        }
        let out = batch(&[profile_of(1, geo::Rect::new(100, 200, 20, 12)), task(2, (0, 0, 4, 4), vec![(dot([255, 0, 0]), None)], Mode::First)], record);
        assert_eq!(ASKED.with(|a| a.borrow().clone()), vec![(0, 0, 4, 4)], "only the screen task is captured");
        assert!(out[0].profile().is_ok());
        // Columns or rows alone, and a batch that panicked: answered, never left without an answer.
        let mut t = profile_of(3, geo::Rect::new(100, 200, 20, 12));
        t.job = Job::Profile { cols: false, rows: true, dark: 128 };
        let out = batch(&[t.clone()], no_capture);
        let got = out[0].profile().as_ref().unwrap();
        assert!(got.columns.is_none() && got.rows.as_ref().is_some_and(|r| r.min.len() == 12));
        assert!(matches!(Outcome::failed(&t), Outcome::Profile(Err(why)) if why == HostError::of(&screen::PROFILE_FAILED)));
        assert_eq!(t.job.kind(), JobKind::Profile);
    }

    /// A snapshot region that had nothing to read at the call — no overlap, or a minimised
    /// window — is answered with that reason on the worker, without a capture.
    #[test]
    fn a_frame_region_with_no_overlap_is_answered_without_capture() {
        let frame = scene_frame();
        let mut t = on_frame(task(1, (0, 0, 0, 0), vec![(dot([255, 0, 0]), None)], Mode::Each), &frame);
        let Job::Search { unresolved, .. } = &mut t.job else { unreachable!() };
        *unresolved = Some(snapshot::no_overlap());
        assert!(!t.captures());
        let out = batch(&[t], no_capture);
        assert_eq!(out[0].found(), Err(snapshot::no_overlap()));
    }

    /// A cells match on a snapshot reads the region where it lies in it: the same answer as a
    /// capture of that region.
    #[test]
    fn a_frame_cells_task_equals_the_live_one() {
        let frame = scene_frame();
        for region in [(100, 200, 20, 12), (101, 201, 5, 5)] {
            let live = batch(&[cells_matching(1, region, None)], scene_capture);
            let snap = batch(&[on_frame(cells_matching(1, region, None), &frame)], no_capture);
            assert_eq!(snap[0].cells(), live[0].cells(), "{region:?}");
            assert!(snap[0].cells().is_ok());
        }
    }

    /// A task's answer carries the moment of the picture it read: a snapshot's its own, a capture's
    /// when it came back with the input epoch as it stood then; one that read nothing, none.
    #[test]
    fn a_tasks_answer_carries_the_moment_of_its_picture() {
        let frame = scene_frame();
        let on_snap = on_frame(task(1, (100, 200, 20, 12), vec![(dot([255, 0, 0]), None)], Mode::First), &frame);
        let epoch = AtomicU64::new(41);
        let before = crate::snapshot::millis_of(Instant::now());
        let out = run_batch(&[on_snap, task(2, (0, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First)], scene_capture, &epoch);
        assert_eq!(out[0].seen, Some(crate::snapshot::moment_of(&frame)), "the snapshot's own");
        let (at, input_epoch) = out[1].seen.expect("a capture's");
        assert!(at >= before && input_epoch == 41, "{at} {before} {input_epoch}");
        let mut unresolved = task(3, (0, 0, 0, 0), vec![(dot([255, 0, 0]), None)], Mode::First);
        let Job::Search { unresolved: u, .. } = &mut unresolved.job else { unreachable!() };
        *u = Some(snapshot::no_overlap());
        assert_eq!(batch(&[unresolved], scene_capture)[0].seen, None);
    }

    /// A newer callback's task of the same call with the key ends an older one still with the
    /// worker at the call — answered `SUPERSEDED` on the next tick, and the worker told to skip it
    /// — and leaves another call's task with the key, another module VM's and one without a key
    /// with the worker.
    #[test]
    fn a_newer_key_ends_the_older_task_of_its_call_at_the_call() {
        let lua = Lua::new();
        let im = ImageState::detached();
        let gens = HashMap::new();
        let cb = lua.create_function(|_, ()| Ok(())).unwrap();
        let file = |scope: usize, call: &'static str, key: Option<&str>| -> u64 {
            let mut p = test_pending(&im, &lua, &gens, im.next_id(), scope, cb.clone());
            p.key = key.map(str::to_string);
            if let ImageWaiter::Callback { call: c, .. } = &mut p.waiter {
                *c = call;
            }
            let id = p.id;
            im.pending.borrow_mut().insert(id, p);
            id
        };
        let older = file(1, "screen.imageSearch", Some("k"));
        let cells = file(1, "screen.matchCells", Some("k"));
        let elsewhere = file(2, "screen.imageSearch", Some("k"));
        let unkeyed = file(1, "screen.imageSearch", None);
        im.supersede(1, 0, "screen.imageSearch", "k");
        {
            let ready = im.ready.borrow();
            assert_eq!(ready.iter().map(|(p, _)| p.id).collect::<Vec<_>>(), vec![older], "the older one ended");
            assert!(
                matches!(&ready[0].1, Outcome::Search(Err(why)) if *why == crate::snapshot::superseded()),
                "answered as replaced"
            );
        }
        assert!(is_ended(older) && !im.pending.borrow().contains_key(&older), "and the worker skips it");
        for id in [cells, elsewhere, unkeyed] {
            assert!(im.pending.borrow().contains_key(&id) && !is_ended(id), "task {id} left with the worker");
        }
        let gone: Vec<PendingImage> = im.ready.borrow_mut().drain(..).map(|(p, _)| p).collect();
        release(gone);
        let left: Vec<PendingImage> = im.pending.borrow_mut().drain().map(|(_, p)| p).collect();
        release(left);
        if let Some(s) = ended().as_mut() {
            s.remove(&older);
        }
    }

    /// `template{ capture, snapshot }` cuts the template out of the snapshot — whole or not at
    /// all — without capturing, with the snapshot's edges as the defaults of the corners.
    #[test]
    fn template_capture_from_a_snapshot_is_inside_or_nil() {
        let lua = Lua::new();
        lua.globals().set("s", snapshot::test_handle(&lua, snapshot::test_frame(100, 50, 40, 30))).unwrap();
        let cut = |src: &str| {
            let s = spec(&lua, src).unwrap();
            made(&lua, s, |_, _, _, _| panic!("the screen was captured")).unwrap().into_vec()
        };
        let got = cut("return { capture = { 110, 60, 114, 63 }, snapshot = s, name = 'cut' }");
        let Value::UserData(ud) = &got[0] else { panic!("expected a handle") };
        assert_eq!(ud.borrow::<TemplateHandle>().unwrap().name.as_deref(), Some("cut"));
        let (dec, _) = picture_of(&got[0]);
        assert_eq!((dec.w, dec.h), (4, 3));
        assert_eq!(&rgba_of(&dec)[0..4], &[110, 60, 50, 255], "the snapshot's pixel at screen (110, 60)");
        let held = budget(&lua).get();
        assert_eq!(held, template::dense_bytes(4, 3));
        let past = cut("return { capture = { 130, 60, 150, 70 }, snapshot = s }");
        assert!(past[0].is_nil());
        assert_eq!(told(&lua, &past[1]), Told::of(&snapshot::not_inside()), "not cut to fit: refused");
        assert_eq!(budget(&lua).get(), held, "and its reservation given back");
        let edges = cut("return { capture = { x1 = 120, y1 = 70 }, snapshot = s }");
        let (dec, _) = picture_of(&edges[0]);
        assert_eq!((dec.w, dec.h), (20, 10), "to the snapshot's right and bottom edges");
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
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().copied()));
            two_dots(regions, src)
        }
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<(i32, i32, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let region = (0, 0, 10, 10);
        let red = || vec![(dot([255, 0, 0]), None)];
        let empty = HostError::with(&screen::CLIENT_EMPTY, "the window's client area is empty (0x0)".into());
        let mut first = task(2, (0, 0, 0, 0), red(), Mode::First);
        let mut each = task(3, (0, 0, 0, 0), red(), Mode::Each);
        for t in [&mut first, &mut each] {
            let Job::Search { unresolved, .. } = &mut t.job else { unreachable!() };
            *unresolved = Some(empty.clone());
            assert!(!t.captures());
        }
        let out = batch(&[task(1, region, red(), Mode::First), first, each], record);
        assert_eq!(ASKED.with(|a| a.borrow().clone()), vec![region], "only the search that has a region is captured");
        assert_eq!(out[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        assert_eq!(out[1].found(), Err(empty.clone()));
        assert_eq!(out[2].found(), Err(empty), "could not look, not a list of false");
    }

    /// A match's answer: `(match, nil)`, or `(nil, err)`; a search's, its one value when it
    /// looked and the error value beside a nil when it could not — and a callback's, those two,
    /// then `info` (`snapshot::callback_values`).
    #[test]
    fn a_cells_answer_is_a_table_or_a_reason() {
        let lua = Lua::new();
        let r = ScreenRect { x: 5, y: 6, w: 10, h: 10 };
        let names = vec![Some("off".to_string()), Some("on".to_string())];
        let live = vec![0u8; 4];
        let ranked = cells::rank(&live, &[vec![0u8; 4].into(), vec![255u8; 4].into()], &[0, 1]).unwrap();
        let args = cells_values(&lua, Outcome::Cells(Ok((live, Some(ranked)))), r, &names).unwrap().into_vec();
        let Value::Table(m) = &args[0] else { panic!("expected a match") };
        assert!(args[1].is_nil());
        assert_eq!(m.get::<String>("cells").unwrap(), "00000000");
        assert_eq!(m.get::<String>("name").unwrap(), "off");
        assert_eq!((m.get::<i32>("x").unwrap(), m.get::<i32>("y").unwrap()), (5, 6));
        let args = cells_values(&lua, Outcome::Cells(Err(crate::backend::capture_failed())), r, &names).unwrap().into_vec();
        assert_eq!(args.len(), 2);
        assert!(args[0].is_nil());
        assert_eq!(told(&lua, &args[1]), Told::of(&crate::backend::capture_failed()));
        // A search still answers exactly one value when it looked, and the reason beside a nil
        // when it could not.
        assert_eq!(search_values(&lua, Search::One, Outcome::Search(Ok(vec![None])), &[None]).unwrap().len(), 1);
        let args = search_values(&lua, Search::One, Outcome::Search(Err(crate::backend::capture_failed())), &[None]).unwrap().into_vec();
        assert!(args[0].is_nil());
        assert_eq!(told(&lua, &args[1]), Told::of(&crate::backend::capture_failed()));
        // A callback gets the two, the reason always second, then `info`.
        let found = search_values(&lua, Search::One, Outcome::Search(Ok(vec![None])), &[None]).unwrap();
        let args = crate::snapshot::callback_values(found, lua.create_table().unwrap(), Some((12, 3))).unwrap().into_vec();
        assert_eq!(args.len(), 3);
        assert!(args[0].is_nil() && args[1].is_nil());
        let Value::Table(info) = &args[2] else { panic!("expected info") };
        assert_eq!((info.get::<i64>("time").unwrap(), info.get::<i64>("inputEpoch").unwrap()), (12, 3));
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
        let why = image::ImageError::IoError(std::io::Error::other("WHY"));
        for reason in [
            crate::backend::capture_failed().to_string(),
            crate::short_capture().to_string(),
            failed(JobKind::Search).to_string(),
            failed(JobKind::Crop).to_string(),
            failed(JobKind::Template).to_string(),
            crate::save::save_failed().to_string(),
            listed(&region::Unresolved::EmptyClient { w: 0, h: 7 }.to_string(), &["0", "7"]),
            region::Unresolved::OutOfRange.to_string(),
            listed(&crate::empty_region(0, 20).to_string(), &["0", "20"]),
            listed(&crate::cells::of_capture(&four, 4, 5, &spec).unwrap_err().to_string(), &["4", "4", "4", "5"]),
            listed(&size(5000, 10), &["5000", "10"]),
            listed(&size(2000, 1000), &["2000", "1000", "2000000"]),
            listed(&template_store::cannot_open(Path::new("PATH"), &why).to_string(), &["PATH", "WHY"]),
        ] {
            assert!(DOC.contains(&format!("`{reason}`")), "docs/api/screen.md does not list the reason `{reason}`");
        }
    }

    #[test]
    fn a_within_is_clipped_to_the_captured_region() {
        assert_eq!(within_frame((100, 100, 50, 50), (90, 120, 30, 100)), Rect { x: 0, y: 20, w: 20, h: 30 });
        assert_eq!(within_frame((100, 100, 50, 50), (0, 0, 10, 10)).w, 0);
    }

    /// A template file inside the module's package that is not there or not a PNG raises where a
    /// call names it; one outside the package is answered (D3 of the typed-errors plan): a
    /// `template{ file }` with `nil, err` — `not_found`, `invalid` — and a search's path left
    /// unchecked at the call, for the search to answer.
    #[test]
    fn a_file_outside_the_package_is_answered_and_one_inside_raises() {
        let lua = Lua::new();
        let root = test_dir("package-root");
        let outside = test_dir("package-outside");
        let missing = outside.join("gone.png");
        let e = template_file(&root, "missing.png").unwrap_err().to_string();
        assert!(e.contains("cannot open template '") && e.contains("missing.png"), "inside: raised, {e}");
        let e = template_file(&root, &missing.to_string_lossy()).unwrap().unwrap_err();
        assert_eq!(e.reason(), &screen::TEMPLATE_NOT_FOUND, "outside: answered");
        assert!(e.message().starts_with(&format!("cannot open template '{}': ", missing.display())), "{e}");
        let not_png = outside.join("not.png");
        std::fs::write(&not_png, b"not a PNG at all").unwrap();
        assert_eq!(template_file(&root, &not_png.to_string_lossy()).unwrap().unwrap_err().reason(), &screen::TEMPLATE_INVALID);
        // The answer of `template{ file }` itself: `nil` and the error value, nothing built.
        let src = format!("return {{ file = {:?} }}", missing.to_string_lossy());
        let file = |path: &str| template_file(&root, path);
        let got = build_handle(&lua, spec(&lua, &src).unwrap(), &Making { file: &file, prepare: &at_once }).unwrap().into_vec();
        assert!(got[0].is_nil());
        assert_eq!(told(&lua, &got[1]).0, "not_found");
        assert_eq!(budget(&lua).get(), 0, "nothing charged");
        // A search's path: checked inside the package, taken as it is outside it.
        assert!(locate_in(&root, "missing.png").is_err(), "inside: raised at the call");
        assert_eq!(locate_in(&root, &missing.to_string_lossy()).unwrap(), missing, "outside: the search answers it");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// A path is the needle of its file under the module's root, named as written, and nothing of
    /// it is decoded where it is read — the reading of every search, a list given to `imageSearch`
    /// and `imageSearchEach`'s among them. A list is read whole before anything is captured, so a path
    /// that is not there raises wherever it sits in it; a value that is no template raises too.
    #[test]
    fn a_path_names_itself_and_a_stranger_is_refused() {
        let lua = Lua::new();
        let root = test_dir("paths");
        std::fs::create_dir_all(root.join("images")).unwrap();
        for p in ["images/a.png", "b.png"] {
            template_store::test_write_dot(&root.join(p), [0, 0, 0], 0);
        }
        let locate = |p: &str| locate_in(&root, p);
        let v = Value::String(lua.create_string("images/a.png").unwrap());
        let (got, name) = resolve_with(&v, "imageSearch", &locate).unwrap();
        assert!(matches!(&got, Needle::File(p) if *p == root.join("images/a.png")));
        assert_eq!(name.as_deref(), Some("images/a.png"));
        let e = err_text(resolve_with(&Value::Integer(3), "imageSearch", &locate));
        assert!(e.contains("expected a path or a Template, got a number"), "{e}");
        let list: Table = lua.load("return { 'images/a.png', 'b.png' }").eval().unwrap();
        assert_eq!(resolve_list(&list, "imageSearch", &locate).unwrap().len(), 2);
        for p in ["images/a.png", "b.png"] {
            assert!(template_store::test_made_on(&root.join(p).display().to_string()).is_empty(), "{p}: decoded where it was read");
        }
        let bad: Table = lua.load("return { 'b.png', true }").eval().unwrap();
        assert!(resolve_list(&bad, "imageSearch", &locate).is_err());
        let missing: Table = lua.load("return { 'b.png', 'images/missing.png' }").eval().unwrap();
        let e = err_text(resolve_list(&missing, "imageSearch", &locate));
        assert!(e.contains("cannot open template '") && e.contains("missing.png"), "{e}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The worker opens a search's PNG and makes a handle's picture still to be made, on its own
    /// thread, before it matches: the search finds what the file and the bytes hold.
    #[test]
    fn the_worker_opens_a_searchs_templates_itself() {
        let root = test_dir("worker-opens");
        let path = root.join("red.png");
        template_store::test_write_dot(&path, [255, 0, 0], 0);
        let (task_tx, task_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        spawn_image_worker(two_dots, crate::backend::no_comparison, Arc::new(AtomicU64::new(0)), task_rx, res_tx);
        task_tx.send(task(1, (0, 0, 10, 10), vec![(Needle::File(path.clone()), None)], Mode::First)).unwrap();
        // Red, then four wildcards: what is still to be made when the search comes.
        let mut bytes = vec![0u8; 5 * 4];
        bytes[..4].copy_from_slice(&[255, 0, 0, 255]);
        let pending = Prepared::of(Raw::Rgba { w: 5, h: 1, bytes });
        task_tx.send(task(2, (0, 0, 10, 10), vec![(Needle::Handle(pending.clone()), None)], Mode::First)).unwrap();
        for id in [1, 2] {
            let r = res_rx.recv_timeout(Duration::from_secs(10)).expect("an answer");
            assert_eq!(r.id, id);
            assert_eq!(r.found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: if id == 1 { 1 } else { 5 }, h: 1, n: 1 })]), "{id}");
        }
        assert_eq!(template_store::test_made_on(&path.display().to_string()), ["image-worker"], "the PNG decoded once, on the worker");
        assert_eq!(template_store::test_made_on("bytes 5x1"), ["image-worker"], "the bytes made into a picture there");
        assert_eq!(pending.get().unwrap().as_ref().unwrap().1, 1, "and kept by the handle");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A template captured again — the same path, a new picture, a new modification time — is what
    /// the next search on the worker finds.
    #[test]
    fn a_png_captured_again_is_what_the_next_search_finds() {
        let root = test_dir("again");
        let path = root.join("dot.png");
        template_store::test_write_dot(&path, [255, 0, 0], 0);
        let search = || batch(&[task(1, (0, 0, 10, 10), vec![(Needle::File(path.clone()), None)], Mode::First)], two_dots);
        assert_eq!(search()[0].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]), "the red dot");
        template_store::test_write_dot(&path, [0, 255, 0], 10);
        assert_eq!(search()[0].found(), Ok(vec![Some(ScreenHit { x: 8, y: 5, w: 1, h: 1, n: 1 })]), "the green one now");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A template that cannot be opened — its file gone since its path was checked, or a body that
    /// cannot be decoded — is what its search answers, in each mode, as "could not look", and no
    /// picture is captured for it; a search beside it in the batch reads as usual.
    #[test]
    fn a_template_that_cannot_be_opened_is_the_searchs_answer_without_a_capture() {
        thread_local! {
            static ASKED: std::cell::RefCell<Vec<(i32, i32, i32, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        fn record(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
            ASKED.with(|a| a.borrow_mut().extend(regions.iter().copied()));
            two_dots(regions, src)
        }
        let root = test_dir("cannot-open");
        let gone = Needle::File(root.join("gone.png"));
        let red = || vec![(dot([255, 0, 0]), None)];
        let out = batch(
            &[
                task(1, (5, 5, 10, 10), vec![(dot([255, 0, 0]), None), (gone.clone(), None)], Mode::First),
                task(2, (5, 5, 10, 10), vec![(gone.clone(), None)], Mode::Each),
                task(3, (5, 5, 10, 10), vec![(gone, None)], Mode::All),
                task(4, (0, 0, 10, 10), red(), Mode::First),
            ],
            record,
        );
        assert_eq!(ASKED.with(|a| a.borrow().clone()), vec![(0, 0, 10, 10)], "only the search whose templates opened is captured");
        let why = format!("cannot open template '{}': ", root.join("gone.png").display());
        for r in &out[..3] {
            let e = r.found().unwrap_err();
            assert!(e.message().starts_with(&why), "{e}");
            assert_eq!(e.reason(), &screen::TEMPLATE_NOT_FOUND, "a file gone since its path was checked");
        }
        assert_eq!(out[3].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A handle's preparation and a search that uses it in one batch make its picture once, and
    /// read nothing for the preparation, which is answered to nobody.
    #[test]
    fn a_handles_preparation_beside_its_search_makes_it_once() {
        let p = Prepared::of(Raw::Rgb { w: 1, h: 1, bytes: vec![0, 255, 0] });
        let prepare = preparation(1, &p);
        assert!(!prepare.captures());
        let out = batch(&[prepare, task(2, (0, 0, 10, 10), vec![(Needle::Handle(p.clone()), None)], Mode::First)], two_dots);
        assert!(matches!(out[0].outcome, Outcome::Prepared));
        assert_eq!(out[1].found(), Ok(vec![Some(ScreenHit { x: 8, y: 5, w: 1, h: 1, n: 1 })]));
        assert_eq!(p.get().unwrap().as_ref().unwrap().1, 1);
    }

    /// A handle's preparation holds its picture weakly: a handle the module has let go — collected,
    /// no search holding it — is gone with its bytes while its task still waits, and the task makes
    /// nothing; a handle still held is made as usual. So a module that builds a template on every
    /// poll leaves no bytes queued past the budget it was given back.
    #[test]
    fn a_preparation_whose_handle_is_gone_makes_nothing() {
        let lua = Lua::new();
        let queued: RefCell<Vec<ImageTask>> = RefCell::new(Vec::new());
        let later = |p: Arc<Prepared>| {
            let id = queued.borrow().len() as u64 + 1;
            queued.borrow_mut().push(preparation(id, &p));
        };
        let making = Making { file: &no_file, prepare: &later };
        for (name, w) in [("dropped", 9), ("kept", 11)] {
            let src = format!("return {{ rgb = string.rep('\\0', {w} * 13 * 3), w = {w}, h = 13, name = '{name}' }}");
            let v = build_handle(&lua, spec(&lua, &src).unwrap(), &making).unwrap();
            lua.globals().set(name, v.into_vec().remove(0)).unwrap();
        }
        lua.globals().set("dropped", Value::Nil).unwrap();
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        let tasks = queued.take();
        let Job::Prepare(gone) = &tasks[0].job else { panic!("a preparation") };
        assert!(gone.upgrade().is_none(), "the bytes went with the handle");
        let out = batch(&tasks, two_dots);
        assert!(out.iter().all(|r| matches!(r.outcome, Outcome::Prepared)));
        assert!(template_store::test_made_on("bytes 9x13").is_empty(), "nothing made for a handle nobody holds");
        assert_eq!(template_store::test_made_on("bytes 11x13").len(), 1, "the one still held is made");
        assert_eq!(lua.load("return kept.count").eval::<Option<u32>>().unwrap(), Some(11 * 13));
    }

    /// A PNG whose decode panics is answered for its own search alone — the host's own failure,
    /// with no capture for it — and the rest of its batch reads as usual.
    #[test]
    fn a_decode_that_panics_fails_its_own_search_alone() {
        let root = test_dir("decode-panics");
        let bad = root.join(template_store::TEST_PANICS);
        template_store::test_write_dot(&bad, [255, 0, 0], 0);
        let out = batch(
            &[
                task(1, (0, 0, 10, 10), vec![(Needle::File(bad), None)], Mode::First),
                task(2, (0, 0, 10, 10), vec![(dot([255, 0, 0]), None)], Mode::First),
            ],
            two_dots,
        );
        assert_eq!(out[0].found(), Err(failed(JobKind::Template)));
        assert_eq!(out[1].found(), Ok(vec![Some(ScreenHit { x: 3, y: 2, w: 1, h: 1, n: 1 })]));
        let _ = std::fs::remove_dir_all(&root);
    }
}
