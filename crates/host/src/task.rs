//! Handlers: every callback of a module runs as a coroutine of its VM — a handler — that can stop
//! at a wait while the event loop goes on; and the waits it stops at: `host.ocr.recognize` without
//! a callback and the `host.screen` calls that wait (`host.screen.snapshot`, `pixel`, `pixels`,
//! `profile`, `cells`, `matchCells`, `template{ capture }`, a snapshot's `crop`, `imageSearch`,
//! `imageSearchAll`, `save` and `saveMarked`), each of which raises where it cannot wait.
//!
//! **One handler per module at a time.** Every event a module hears — a hotkey, a captured key, a
//! controller event, a timer, a window trigger, a focus change, the answer to a read, an image
//! search or a snapshot, a setting's `onChange` — comes through its mailbox (`mailbox.rs`) and runs
//! here, as a handler, one after another. A module whose handler waits is busy; its later events
//! wait in its mailbox, in order. The synchronous places — an arbiter's `onActivate` and
//! `onDeactivate`, a module's own `onChange` from its own `host.settings.set`, the top level of a
//! module or of an included file — are plain calls, as ever.
//!
//! **Every handler waits.** `host.ocr.recognize` without a callback, called in a handler's own
//! coroutine, asks the read service and stops the handler until the reading comes: its module is
//! busy meanwhile, and nothing else is. With a callback it is the read that never waits. So does
//! `host.screen.snapshot`: it asks the `screen-capture` thread for its picture (`snapshot.rs`), and
//! the handler goes on with the handle; and so do `host.screen.pixel` and `pixels`, whose points
//! that thread reads, unless they touch no screen — a snapshot's points — and answer at once
//! ([`Started::Now`]). The calls that work over a region wait for the image worker
//! (`image_search.rs`): `profile` for its picture and then its reduction, `cells` and `matchCells`
//! for their capture and their grid, `template{ capture }` for its picture, a snapshot's `crop` for
//! its copy, the two searches for their capture and their matching, `save` and `saveMarked` for
//! their capture and their file (`save.rs`) — each answered at once when there is nothing to read
//! this time.
//!
//! **Why.** No text recognition may burden the event loop, and no `host.screen` call either.
//! `recognize` used to photograph and recognise on the loop and return the reading, and the
//! `host.screen` calls to capture and work there; where they wait they ask the read service, the
//! `screen-capture` thread or the image worker instead (`ocr/lua.rs`, `snapshot.rs`,
//! `image_search.rs`), and only their module waits. Where they cannot wait — outside every handler,
//! a place Luau cannot suspend, a function the host calls and waits for, a coroutine the module
//! made — they raise at the caller's line ([`wait_message`], one message per call and place), and
//! the callback form works there. A `host.screen` call that touches nothing it would wait for — the
//! points of a snapshot, a window with no pixel this time — is answered at once wherever it is
//! made, as it is in a handler ([`Started::Now`]).
//!
//! **How it waits.** A Rust function cannot yield through mlua, so the wait is a `coroutine.yield`
//! in a small Luau shim the host builds once per VM and library (`task_shim.luau`), with every
//! helper an upvalue. The host creates and resumes every handler with mlua's own `Thread` API: the
//! first stretch at once, inside `run_handler`; every later one from the tick, in the delivery of
//! text readings or of snapshots, while the module is enabled and its VM is the one that asked. A
//! handler is never resumed into a VM that was disabled, reloaded or removed, and nothing of it
//! runs after it ends — Luau has no `__gc` and no `__close`, so throwing a stopped thread away runs
//! no code.
//!
//! **Who answers a wait.** A wait point is answered only by a thread of the host or by state the
//! host keeps — the capture and recognise threads through the delivery of readings and pictures,
//! the tests' `test_release` — never by an event delivered to the waiting module: that event waits
//! in the module's mailbox behind the very handler it would answer. A "wait for my own key-up"
//! built on the module's own key events could never end; one answered from the hook's record of
//! held keys could.
//!
//! **A wait turns no epoch.** The handler going on with its answer continues the dispatch that
//! asked, so the delivery that resumes it leaves `host.epoch()` where it was: a memo the handler
//! kept before its wait still holds after it. A delivery that hands a callback its answer turns
//! the epoch once, before the first such callback, as every delivery always did (`ocr/lua.rs`,
//! `snapshot.rs`).
//!
//! **What module code can do to a handler, and what the host does about it.**
//! - It can get the handler's coroutine (`coroutine.running()`, which is never `nil` in a callback
//!   now) and resume it itself. The shim sees that it was not the host (the wait's nonce is a fresh
//!   table nobody else holds), and the host forgets the handler without resetting it — the
//!   module's code is running it now — and withdraws its read. The wait raises there.
//! - It can close it (`coroutine.close`): the handler has ended, and is found so at its delivery.
//! - It can yield it with its own `coroutine.yield` through to the host: that raises at the yield
//!   with [`OWN_YIELD`], with `resume_error`, as the same yield does on the event loop, so a `pcall`
//!   around it goes on.
//! - It can reach the shim's `start` through `debug.info` while `start` reads its own arguments,
//!   and call it itself: `start` checks that the innermost running handler of the VM calls it in
//!   its own coroutine with no wait yet in this stretch, registers the wait only after every check,
//!   and a handler is parked — and later resumed — only on the very wait it registered.
//! - Before every reset the host asks Luau itself (`coroutine.status`, held from before any module
//!   code ran) whether the thread is suspended or dead. mlua reports a thread that is resuming
//!   another one as new, and resetting it would cut a live stack.
//!
//! Kept in a file of its own, with the bindings generic over [`ReadHost`], so the rules can be
//! tested against a real Luau VM and a real read service (`task_tests.rs`, `mailbox_tests.rs`).

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Thread, ThreadStatus, Value};

use crate::logging;
use crate::ocr::lua::{self as reads, owner_of, ReadHost, Waiter};
use crate::ocr::sched::{Owner, TicketId};
use crate::ocr::snap_queue::SnapId;
use crate::ocr::types::{current_priority, enter_priority, Priority};

/// A handler's number: positive, unique for the life of the process, never reused — as a timer's.
pub(crate) type TaskId = i64;

/// The shim, compiled once per VM (`waits`).
const SHIM: &str = include_str!("task_shim.luau");

/// The shim's chunk name, which a traceback and `debug.info` show: `host.ocr`, the calls it
/// carries. The `=` keeps Luau from wrapping it as `[string "…"]`.
pub(crate) const SHIM_NAME: &str = "=host.ocr";

/// The same shim's chunk name where it carries the `host.screen` calls that wait ([`points`]).
pub(crate) const SCREEN_SHIM_NAME: &str = "=host.screen";

/// How many handler stretches may be on the host's stack at once, one inside the other — a
/// test's task started in the stretch of another, before that one waited again — across every VM.
/// Each is a resume on the event loop's own stack, and a stack that overflows ends the
/// application: Luau counts one C call per level against its limit of 200, and the main thread's
/// stack is used up well before that in a debug build. The seventeenth is not started. Events
/// never nest: no handler starts while another is on the stack (`mailbox.rs`).
pub(crate) const MAX_NESTED: usize = 16;

/// What a callback's own `coroutine.yield` through to the host raises, at the yield.
pub(crate) const OWN_YIELD: &str = "this callback runs as a coroutine of the host, and a coroutine.yield in it cannot wait \
     for one of your own callbacks — wrap the code that yields in coroutine.wrap";

/// What a wait raises after module code resumed the handler's coroutine itself, after the call's
/// name.
pub(crate) const RESUMED: &str = ": this callback's coroutine was resumed by the module's own code, and so the host \
     runs it no more; a callback's coroutine is the host's to resume";

/// What `host.ocr.recognize` raises for an explicit `nil` as its third argument: almost always a
/// callback that is missing.
pub(crate) const NIL_CB: &str = "host.ocr.recognize: the callback is nil — pass a function to be called back with the \
     reading, or leave the third argument out to wait for it";

/// The named registry's per-VM entries: the handles below, the shim's `recognize` and its builder
/// of the `host.screen` calls that wait. Luau has no `debug.getregistry`, so no module reaches
/// them.
const PRIMS_KEY: &str = "__task_prims";
const WAITS_KEY: &str = "__task_waits";
const POINTS_KEY: &str = "__task_points";

/// The kinds of place a call without a callback cannot wait, as the shim tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Case {
    /// Outside every handler of the VM: the top level of a module or of an included file, or a
    /// function the host calls between its events (an arbiter's `onActivate` moved by another
    /// module's claim, say).
    Outside,
    /// In a handler, at a place Luau cannot stop it: a function the host calls and waits for (an
    /// arbiter's `onActivate`, an `onChange` from the module's own `set`, an included file's top
    /// level), a metamethod, a sort comparator.
    CannotWait,
    /// In a coroutine the module made, or the overlay runtime made for a hook that must not read,
    /// inside a handler.
    OwnCoroutine,
}

impl Case {
    /// The shim's word for it: "none" is outside every handler, "foreign" a coroutine of the
    /// module's own; "cannot-wait" — and a word it never says — a place that cannot wait. The shim
    /// picks among the three messages itself; this is for the scripted test host's stand-ins.
    #[cfg(test)]
    pub(crate) fn of(why: &str) -> Case {
        match why {
            "none" => Case::Outside,
            "foreign" => Case::OwnCoroutine,
            _ => Case::CannotWait,
        }
    }
}

/// What `call` — `host.ocr.recognize`, or a `host.screen` call that waits — raises without a
/// callback in a place of `case`, at the caller's line: where the place is, and what to write
/// there. Outside every handler, a handler such as `host.timer.after(0, …)`; at a place Luau cannot
/// suspend, the event's own function; in a coroutine — the overlay runtime's for its hooks, or the
/// module's own, which the shim cannot tell apart — a measurement made in a handler for the hook to
/// read, or the event's own function. Each with the call's callback form, which works in every one
/// of them. The shim raises it, in the scripted test host as in the host, so a scenario that reads
/// where it could not wait fails. docs/api/ocr.md gives `recognize`'s three and docs/api/screen.md
/// those of `host.screen.pixel`, word for word.
pub(crate) fn wait_message(call: &str, case: Case) -> String {
    let form = callback_form(call).map(|f| format!(", {f}")).unwrap_or_default();
    match case {
        Case::Outside => format!(
            "{call} cannot wait here: this runs outside every event of the module — its top level, an included \
             file's, or a function the host calls between its events. Call it from a handler, such as \
             host.timer.after(0, …), or pass a callback{form}"
        ),
        Case::CannotWait => format!(
            "{call} cannot wait here: this function runs inside one the host calls and waits for, or one Luau \
             cannot suspend — an arbiter's onActivate or onDeactivate, an onChange fired by your own \
             host.settings.set, the top level of an included file, a metamethod, a table.sort comparator, a \
             string.gsub function, the iterator of a for loop, an xpcall error handler. Pass a callback{form}, or \
             make the call in the event's own function"
        ),
        Case::OwnCoroutine => format!(
            "{call} cannot wait in a coroutine the module made, or one the overlay runtime made for a hook that \
             must not read (when, present, menu tests, an identify asked on every evaluation): only the host's own \
             coroutine for an event waits. In a hook, measure it in a handler and let the hook read what was \
             measured (see \"Where a hook runs\" in docs/api/overlay.md); in a coroutine of your own, pass a \
             callback{form}, or call it from the event's own function"
        ),
    }
}

/// The library a call belongs to — `ocr` for `host.ocr.recognize`, `screen` for
/// `host.screen.pixel` and for a method of a Snapshot (`Snapshot:crop`) — which names its page in
/// docs/api and the scope of its log lines.
fn library_of(call: &str) -> &str {
    if call.starts_with("Snapshot:") {
        return "screen";
    }
    call.strip_prefix("host.").and_then(|c| c.split('.').next()).unwrap_or(call)
}

/// The form of `call` that waits for nothing, as code to write: what a poll, and a place that
/// cannot wait, are to use instead. `None` for a call that has none.
fn callback_form(call: &str) -> Option<&'static str> {
    match call {
        "host.ocr.recognize" => Some("host.ocr.recognize(what, opts, function(reading) … end)"),
        "host.screen.snapshot" => Some("host.screen.snapshot(opts, function(snap, err) … end)"),
        "host.screen.pixel" => Some("host.screen.pixel(x, y, opts, function(colour, err) … end)"),
        "host.screen.pixels" => Some("host.screen.pixels(points, opts, function(colours, err) … end)"),
        "host.screen.profile" => Some("host.screen.profile(opts, function(profile, err) … end)"),
        "host.screen.cells" => Some("host.screen.cells(opts, function(cells, err) … end)"),
        "host.screen.matchCells" => Some("host.screen.matchCells(opts, states, function(match, err) … end)"),
        "host.screen.template" => Some("host.screen.template(spec, function(template, err) … end)"),
        "Snapshot:crop" => Some("s:crop(region, function(crop, err) … end)"),
        "host.screen.imageSearch" => Some("host.screen.imageSearch(template, opts, function(hit, err) … end)"),
        "host.screen.imageSearchAll" => Some("host.screen.imageSearchAll(template, opts, function(hits, err) … end)"),
        "host.screen.save" => Some("host.screen.save(path, opts, function(saved, err) … end)"),
        "host.screen.saveMarked" => Some("host.screen.saveMarked(path, opts, function(saved, err) … end)"),
        _ => None,
    }
}

/// The line for a timer whose handler waited in `call` while inputs queued behind it, once per
/// module and place in a session: a poll that holds the module's keys (docs/api/ocr.md and
/// docs/api/screen.md show it).
pub(crate) fn timer_wait_line(module: &str, ms: u128, call: &str, place: &str, keys: &[String]) -> String {
    let how = callback_form(call).map(|f| format!(": {f}")).unwrap_or_default();
    format!(
        "[{module}] a timer waited {ms} ms for {call} at {place}; {} key(s) ({}) waited behind it — a poll should \
         not wait{how}",
        keys.len(),
        crate::stops::counted(keys).join(", ")
    )
}

/// Under what a handler's error is reported, kept with it across its waits: the kind of callback
/// the log line and the dialog name, and the module they are reported for — the one `report_callback_error`
/// was given before handlers: the module for most events, the identity that asked for an answer,
/// the module whose setting changed for an `onChange`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ctx {
    /// "hotkey", "key", "gamepad", "window trigger", "focus change", "timer", "ocr.recognize", an
    /// image search's binding, "screen.snapshot", "settings onChange (<key>)", "page message",
    /// "page ready", "page close".
    pub what: Cow<'static, str>,
    pub report: usize,
}

impl Ctx {
    pub(crate) fn new(what: impl Into<Cow<'static, str>>, report: usize) -> Ctx {
        Ctx { what: what.into(), report }
    }
}

/// What started a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// An event's handler (`run_handler`).
    Handler,
    /// The tests' entry (`table`).
    Test,
}

/// What a wait waits for.
pub(crate) enum On {
    /// A read of the service.
    Read(TicketId),
    /// A picture of the `screen-capture` thread (`snapshot.rs`) — and a profile's reduction after
    /// it, which is the same request.
    Snap(SnapId),
    /// A task of the image worker (`image_search.rs`), by its number.
    Image(u64),
    /// The tests' own wait point, by name (`test_wait`): answered by `test_release`.
    #[cfg(test)]
    Test(String),
    /// Nothing: a wait that ends the handler instead — it was cancelled while it ran, or its
    /// module is disabled.
    Void,
}

/// The wait a handler registered in its current stretch, or is parked on.
struct Wait {
    on: On,
    /// The fresh table the shim yields beside `WAIT` and gets back from the host, and nobody else.
    nonce: RegistryKey,
    /// The call that waits, as a module writes it: what the timer's line names.
    call: &'static str,
}

/// One handler, or one task of the tests' entry. Main thread only, like the rest of `Shared`.
struct Task {
    /// Strong: a parked handler keeps its VM, as a pending read does. Dropped with the handler.
    lua: Lua,
    thread: Thread,
    /// The thread's identity, to tell which coroutine is calling.
    ptr: usize,
    /// The VM that runs it, which owns it — a dependency's code included.
    owner: Owner,
    kind: Kind,
    /// Under what its error is reported, after a wait as before.
    ctx: Ctx,
    /// The priority of the dispatch that started it, which each of its later stretches runs under
    /// — and so every timer it arms and every read it asks for with a callback — and each of its
    /// waits asks with, unless it was `raised`.
    prio: Priority,
    /// An event somebody waits on queued behind it while it was parked in the background lane
    /// (`promote`): its read then waiting, and every read it waits for after it, are interactive.
    /// Its own lane is not raised with them: a poll chained through `host.timer.after`, whose read
    /// a key once queued behind, would otherwise be a person's for good.
    raised: bool,
    /// On the host's stack: its first stretch, or one the delivery resumed.
    running: bool,
    wait: Option<Wait>,
    /// Ends at its next wait: cancelled while it ran.
    ending: bool,
    /// Its module went while it ran: forgotten when its stretch returns, whatever it did.
    gone: bool,
    /// When it parked last, and how long it has been parked in all: what the window dispatch's
    /// `[dispatch]` line leaves out of its time (`waited_ms`).
    parked_at: Option<Instant>,
    waited: Duration,
    /// When it began: how long its module has been busy with it.
    began: Instant,
    /// Where in the module its current wait was asked — `<module>/<file>:<line>`, or the chunk's
    /// name — for the timer's line and the busy line.
    place: Option<String>,
    /// The inputs queued behind its current wait, by name: what the timer's line names.
    behind: Vec<String>,
}

impl Task {
    fn is_handler(&self) -> bool {
        matches!(self.kind, Kind::Handler)
    }
}

/// Every handler of every module. Main thread only.
#[derive(Default)]
pub(crate) struct Tasks {
    table: RefCell<HashMap<TaskId, Task>>,
    /// The handlers whose stretches are on the host's stack, innermost last, across every VM:
    /// what `MAX_NESTED` counts.
    running: RefCell<Vec<TaskId>>,
    next: Cell<TaskId>,
    /// (module id, place) of every timer's line written this session (`timer_wait_line`).
    timer_said: RefCell<HashSet<(String, String)>>,
    /// The handlers whose busy line was written (`busy_line`), so it comes once per handler.
    busy_said: RefCell<HashSet<TaskId>>,
}

impl Tasks {
    fn next_id(&self) -> TaskId {
        let n = self.next.get() + 1;
        self.next.set(n);
        n
    }

    /// Whether any handler or task of module `idx` is left — none may be once it is dropped, or
    /// its old VM stays in memory.
    #[cfg(test)]
    pub(crate) fn has_tasks_of(&self, idx: usize) -> bool {
        self.table.borrow().values().any(|t| t.owner.idx == idx)
    }

    /// How many handlers and tasks there are, waiting or running.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.table.borrow().len()
    }

    /// The handler of module `idx`, running or parked: at most one, since the mailbox starts none
    /// while it has one. A task of the tests' entry is no handler.
    pub(crate) fn handler_of(&self, idx: usize) -> Option<TaskId> {
        self.table.borrow().iter().find(|(_, t)| t.owner.idx == idx && t.is_handler()).map(|(id, _)| *id)
    }

    /// Whether a handler of any module is on the host's stack now: then no other handler starts
    /// (`mailbox.rs`).
    pub(crate) fn any_handler_running(&self) -> bool {
        let table = self.table.borrow();
        self.running.borrow().iter().any(|id| table.get(id).is_some_and(Task::is_handler))
    }

    /// How long the handler of module `idx` that runs now has been parked in all, in
    /// milliseconds; 0 when none runs.
    pub(crate) fn waited_ms(&self, idx: usize) -> u64 {
        let table = self.table.borrow();
        self.running
            .borrow()
            .iter()
            .rev()
            .filter_map(|id| table.get(id))
            .find(|t| t.owner.idx == idx && t.is_handler())
            .map_or(0, |t| t.waited.as_millis() as u64)
    }

    /// What the parked handler of module `idx` waits in — the kind of callback — and for how many
    /// milliseconds so far; `None` when none is parked.
    pub(crate) fn parked_of(&self, idx: usize) -> Option<(String, u128)> {
        self.table.borrow().values().find(|t| t.owner.idx == idx && t.is_handler() && !t.running).map(|t| {
            (t.ctx.what.to_string(), t.parked_at.map_or(0, |at| at.elapsed().as_millis()))
        })
    }

    /// An input named `name` queued behind module `idx`'s parked handler: noted for the timer's
    /// line, which names what waited behind a poll's read. Nothing when no handler of it is parked.
    pub(crate) fn note_behind(&self, idx: usize, name: String) {
        let mut table = self.table.borrow_mut();
        if let Some(t) = table.values_mut().find(|t| t.owner.idx == idx && t.is_handler() && !t.running) {
            t.behind.push(name);
        }
    }

    /// Module `idx`'s handler, when it is parked and has kept its module busy for `busy_for` at
    /// `now` or longer, and its busy line was not written yet: what it waits in, how long it has
    /// been busy, and where it waits. Marked written.
    pub(crate) fn long_wait(&self, idx: usize, now: Instant, busy_for: Duration) -> Option<(String, Duration, String)> {
        let table = self.table.borrow();
        let (id, t) = table.iter().find(|(_, t)| t.owner.idx == idx && t.is_handler() && !t.running)?;
        let busy = now.saturating_duration_since(t.began);
        if busy < busy_for || !self.busy_said.borrow_mut().insert(*id) {
            return None;
        }
        Some((t.ctx.what.to_string(), busy, t.place.clone().unwrap_or_else(|| "a place not known".to_string())))
    }

    /// Whether the timer's line for module `id` and `place` was written, for the tests.
    #[cfg(test)]
    pub(crate) fn timer_said(&self, id: &str, place: &str) -> bool {
        self.timer_said.borrow().contains(&(id.to_string(), place.to_string()))
    }

    /// How many timer's lines were written, for the tests.
    #[cfg(test)]
    pub(crate) fn timer_lines(&self) -> usize {
        self.timer_said.borrow().len()
    }

    /// The innermost running handler of the VM `owner`.
    fn innermost(&self, owner: Owner) -> Option<TaskId> {
        let table = self.table.borrow();
        self.running.borrow().iter().rev().copied().find(|id| table.get(id).is_some_and(|t| t.owner == owner))
    }
}

/// The handles a VM's handlers need, held in its named registry from before any module code ran.
struct Prims {
    status: Function,
    noop: Function,
    wait: Table,
}

/// The per-VM entries, made on the first call — which is the host's own, before any module code
/// runs (`install_host_api`), so `coroutine.status`, `yield` and the rest are Luau's own.
fn prims(lua: &Lua) -> mlua::Result<Prims> {
    if let Ok(t) = lua.named_registry_value::<Table>(PRIMS_KEY) {
        return Ok(Prims { status: t.get("status")?, noop: t.get("noop")?, wait: t.get("wait")? });
    }
    let coroutine: Table = lua.globals().get("coroutine")?;
    let status: Function = coroutine.get("status")?;
    let noop = lua.create_function(|_, ()| Ok(()))?;
    let wait = lua.create_table()?;
    let t = lua.create_table()?;
    t.set("status", status.clone())?;
    t.set("noop", noop.clone())?;
    t.set("wait", wait.clone())?;
    t.set("yield", coroutine.get::<Function>("yield")?)?;
    t.set("isyieldable", coroutine.get::<Function>("isyieldable")?)?;
    t.set("rawequal", lua.globals().get::<Function>("rawequal")?)?;
    t.set("error", lua.globals().get::<Function>("error")?)?;
    t.set("select", lua.globals().get::<Function>("select")?)?;
    t.set("type", lua.globals().get::<Function>("type")?)?;
    let table: Table = lua.globals().get("table")?;
    t.set("pack", table.get::<Function>("pack")?)?;
    t.set("unpack", table.get::<Function>("unpack")?)?;
    lua.set_named_registry_value(PRIMS_KEY, t)?;
    Ok(Prims { status, noop, wait })
}

/// Luau's own word for the thread: "suspended", "running", "normal" or "dead" — or `None` when it
/// cannot be asked, which only running out of memory does. `None` means "leave it alone": such a
/// thread is neither resumed nor reset, only forgotten, since mlua's own answer for a thread that
/// is resuming another is "new", and a reset of that would cut a live stack.
fn status(lua: &Lua, thread: &Thread) -> Option<String> {
    prims(lua).and_then(|p| p.status.call::<String>(thread.clone())).ok()
}

/// Lets go of a thread's stack at once, and of whatever its locals held — but only one that Luau
/// says is stopped or finished: one that is running, resuming another coroutine, or could not be
/// asked about, is left alone.
fn reset(lua: &Lua, thread: &Thread) {
    if matches!(status(lua, thread).as_deref(), Some("suspended" | "dead")) {
        if let Ok(p) = prims(lua) {
            let _ = thread.reset(p.noop);
        }
    }
}

/// The owner of the VM `lua`.
fn owner<H: ReadHost>(h: &H, lua: &Lua, scope: usize) -> Owner {
    owner_of(lua, &h.vm_gens(), scope)
}

/// The value `cancel` and `alive` take as a task's number: a whole number, or nothing.
fn number_of(v: &Value) -> Option<TaskId> {
    crate::timers::token_of(v)
}

/// The tests' entry to the machinery, and only theirs: `run`, `cancel` and `alive` for module
/// `idx`, over the handlers `holder` keeps, as a table a test installs under a global of its own —
/// never in the `host` table: no module can start a task. A task belongs to the VM that runs it,
/// whatever identity the code that started it has, and waits in `recognize`: these are what the
/// tests make wait. A task is no handler: it does not make its module busy.
#[cfg(test)]
pub(crate) fn table<H: ReadHost>(lua: &Lua, idx: usize, holder: Rc<H>) -> mlua::Result<Table> {
    prims(lua)?;
    let task = lua.create_table()?;
    let h = holder.clone();
    task.set("run", lua.create_function(move |lua, args: MultiValue| run(&*h, lua, idx, args))?)?;
    let h = holder.clone();
    task.set("cancel", lua.create_function(move |lua, n: Value| Ok(cancel(&*h, lua, idx, &n)))?)?;
    let h = holder;
    task.set("alive", lua.create_function(move |lua, n: Value| Ok(alive(&*h, lua, idx, &n)))?)?;
    Ok(task)
}

/// `run(fn)`: starts `fn` in a coroutine of its own and runs it until it ends or first waits,
/// then returns its number. Raises only for a mistake in the call; what `fn` raises is reported
/// as a task's error, never raised here. Only the tests' entry (`table`) calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn run<H: ReadHost>(h: &H, lua: &Lua, scope: usize, args: MultiValue) -> mlua::Result<TaskId> {
    let mut args = args.into_iter();
    let f = match args.next() {
        Some(Value::Function(f)) => f,
        other => {
            return Err(mlua::Error::external(format!(
                "task.run: takes a function, got {}",
                other.as_ref().map_or("nothing", crate::json::luau_type)
            )))
        }
    };
    if args.next().is_some() {
        return Err(mlua::Error::external(
            "task.run takes one argument, the function; hand it values as upvalues",
        ));
    }
    let owner = owner(h, lua, scope);
    let id = h.tasks().next_id();
    if h.tasks().running.borrow().len() >= MAX_NESTED {
        h.report_error(owner.idx, "task", &too_deep("task.run: this task"));
        return Ok(id);
    }
    start_thread(h, id, lua, owner, f, MultiValue::new(), Kind::Test, Ctx::new("task", owner.idx), current_priority())?;
    Ok(id)
}

/// The message for a handler not started because `MAX_NESTED` stretches are on the stack.
fn too_deep(what: &str) -> String {
    format!(
        "{what} was not started: {MAX_NESTED} task stretches are on the event loop's own stack already, one \
         inside the other, and one more could overflow it and end the application. Start it later — from a \
         timer, or after a wait"
    )
}

/// Makes `f`'s coroutine, files it as `id` and runs its first stretch with `args`.
#[allow(clippy::too_many_arguments)]
fn start_thread<H: ReadHost>(
    h: &H,
    id: TaskId,
    lua: &Lua,
    owner: Owner,
    f: Function,
    args: MultiValue,
    kind: Kind,
    ctx: Ctx,
    prio: Priority,
) -> mlua::Result<()> {
    let thread = lua.create_thread(f)?;
    let ptr = thread.to_pointer() as usize;
    h.tasks().table.borrow_mut().insert(
        id,
        Task {
            lua: lua.clone(),
            thread: thread.clone(),
            ptr,
            owner,
            kind,
            ctx,
            prio,
            raised: false,
            running: true,
            wait: None,
            ending: false,
            gone: false,
            parked_at: None,
            waited: Duration::ZERO,
            began: Instant::now(),
            place: None,
            behind: Vec::new(),
        },
    );
    let delivery = enter_delivery(h.tasks(), id);
    let outcome = stretch(h.tasks(), id, &delivery, || thread.resume::<MultiValue>(args));
    step(h, id, lua, &thread, &delivery, outcome);
    // The delivery's entry ends here, with the handler ended or parked, nothing of it running.
    drop(delivery);
    Ok(())
}

/// The guard's entry for one delivery of handler `id`: from the stretch that starts or resumes it
/// until it ends or waits. The resumes after its own `coroutine.yield`, which the host answers at
/// once (`step`), are inside it, so they do not start its clocks afresh: a handler that yields in a
/// loop — `while true do pcall(coroutine.yield) end` — is one callback to the guard, stopped at its
/// limit like any other loop. `None` when the handler is not in the table.
type Delivery = Option<Result<crate::vm_guard::Entry, crate::vm_guard::Refused>>;

fn enter_delivery(tasks: &Tasks, id: TaskId) -> Delivery {
    let (lua, what) = tasks.table.borrow().get(&id).map(|t| (t.lua.clone(), t.ctx.what.clone()))?;
    Some(crate::vm_guard::Entry::enter(&lua, crate::vm_guard::EntryKind::Handler, what))
}

/// What became of a handler's first stretch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ran {
    /// It returned, raised (reported under its `Ctx`), was closed — or was never started.
    Ended,
    /// It waits; its module is busy until it ends.
    Parked(TaskId),
}

/// Runs `f(args)` as module `idx`'s handler for one event, in the VM `lua`, under `prio` — the
/// priority its waits ask with — until it ends or first waits. What it raises, before a wait or
/// after one, is reported under `ctx`, never raised here. The mailbox calls this, and only for a
/// module that has no handler: one handler per module at a time.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_handler<H: ReadHost>(
    h: &H,
    idx: usize,
    lua: &Lua,
    f: Function,
    args: MultiValue,
    prio: Priority,
    ctx: Ctx,
) -> Ran {
    let owner = owner(h, lua, idx);
    let tasks = h.tasks();
    if tasks.running.borrow().len() >= MAX_NESTED {
        h.report_error(ctx.report, &ctx.what, &too_deep("this callback"));
        return Ran::Ended;
    }
    let id = tasks.next_id();
    let report = (ctx.report, ctx.what.clone());
    if let Err(e) = start_thread(h, id, lua, owner, f, args, Kind::Handler, ctx, prio) {
        // Only running out of memory makes a coroutine fail to be made: a VM that is full, and
        // that is a stop of its module, which reports itself (`vm_guard`, `stops.rs`).
        if !crate::vm_guard::note_failure(lua, crate::vm_guard::EntryKind::Handler, report.1.clone(), &e) {
            h.report_error(report.0, &report.1, &e.to_string());
        }
        return Ran::Ended;
    }
    match tasks.table.borrow().get(&id) {
        Some(t) if !t.running => Ran::Parked(id),
        _ => Ran::Ended,
    }
}

/// Runs one stretch of handler `id`: on the host's stack while it lasts, a Rust panic caught as an
/// error, as every callback's is.
///
/// **The guard's entry.** Every stretch runs inside its delivery's entry ([`enter_delivery`]) —
/// the one an event starts (`mailbox::run` → [`run_handler`] → [`start_thread`]) and every one
/// after a wait ([`resume_parked`]), each spanning the resumes after the handler's own
/// `coroutine.yield` ([`step`]) — and, since no handler starts inside another, that entry is the
/// outermost in the application: a stop of the slice is marked when it ends (`stops::mark`), with
/// the handler ended or parked. Each stretch is an entry of its own nested in it, so a memory error
/// is looked at where it reached the host. A stretch of a stopped VM is not run, and ends with
/// [`vm_guard::NOT_RUN`](crate::vm_guard::NOT_RUN), which [`step`] does not report.
fn stretch(
    tasks: &Tasks,
    id: TaskId,
    delivery: &Delivery,
    go: impl FnOnce() -> mlua::Result<MultiValue>,
) -> Result<MultiValue, String> {
    let refused = match delivery {
        Some(Err(crate::vm_guard::Refused)) => true,
        Some(Ok(d)) => d.stopped(),
        None => false,
    };
    let vm = tasks.table.borrow_mut().get_mut(&id).map(|t| {
        t.running = !refused;
        (t.lua.clone(), t.ctx.what.clone())
    });
    if refused {
        return Err(crate::vm_guard::NOT_RUN.to_string());
    }
    let entry = match vm {
        Some((lua, what)) => match crate::vm_guard::Entry::enter(&lua, crate::vm_guard::EntryKind::Handler, what) {
            Ok(e) => Some(e),
            Err(crate::vm_guard::Refused) => {
                if let Some(t) = tasks.table.borrow_mut().get_mut(&id) {
                    t.running = false;
                }
                return Err(crate::vm_guard::NOT_RUN.to_string());
            }
        },
        None => None,
    };
    tasks.running.borrow_mut().push(id);
    let outcome = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(go)) {
        Ok(r) => {
            if let Some(e) = &entry {
                e.finish(&r);
            }
            r.map_err(|e| e.to_string())
        }
        Err(p) => Err(crate::panic_text(&p)),
    };
    {
        let mut running = tasks.running.borrow_mut();
        if let Some(i) = running.iter().rposition(|r| *r == id) {
            running.remove(i);
        }
    }
    if let Some(t) = tasks.table.borrow_mut().get_mut(&id) {
        t.running = false;
    }
    // Off the stack before the entry ends: a stop it ends with is marked with nothing of it running.
    drop(entry);
    outcome
}

/// What became of a stretch: the handler ended (returned, raised, closed), parked on its wait, or
/// yielded otherwise — then that yield raises where it was made, and the handler goes on.
fn step<H: ReadHost>(
    h: &H,
    id: TaskId,
    lua: &Lua,
    thread: &Thread,
    delivery: &Delivery,
    mut outcome: Result<MultiValue, String>,
) {
    let tasks = h.tasks();
    loop {
        let (gone, ending, prio) = match tasks.table.borrow().get(&id) {
            Some(t) => (t.gone, t.ending, t.prio),
            None => return, // module code resumed it, and it is no handler any more
        };
        if gone {
            forget(h, id, false);
            return;
        }
        let values = match outcome {
            Err(e) => {
                // A handler of a VM the guard stopped — by this very stretch, or before it — is not
                // the module's error: the stop reports itself (stops.rs).
                let ctx = tasks.table.borrow().get(&id).map(|t| t.ctx.clone());
                if let Some(ctx) = ctx.filter(|_| !crate::vm_guard::stopped(lua)) {
                    h.report_error(ctx.report, &ctx.what, &e);
                }
                forget(h, id, true);
                return;
            }
            Ok(v) => v,
        };
        // Back from its own resume, it cannot be resuming another coroutine: mlua's word for a
        // finished one is sure here, and it saves asking Luau for every callback that returned.
        if thread.status() == ThreadStatus::Finished {
            forget(h, id, true);
            return;
        }
        match status(lua, thread).as_deref() {
            // It returned — or Luau could not say, and then it is let go without a reset.
            Some("dead") | None => {
                forget(h, id, true);
                return;
            }
            _ => {}
        }
        if parked_on_its_wait(tasks, id, lua, &values) {
            let void = tasks.table.borrow().get(&id).is_some_and(|t| t.wait.as_ref().is_some_and(|w| matches!(w.on, On::Void)));
            if ending || void {
                forget(h, id, false);
            } else if let Some(t) = tasks.table.borrow_mut().get_mut(&id) {
                t.parked_at = Some(Instant::now());
            }
            return;
        }
        // Its own `coroutine.yield`, through to the host: a wait it registered by calling `start`
        // itself is withdrawn, and the yield raises where it was made.
        let stray = tasks.table.borrow_mut().get_mut(&id).and_then(|t| t.wait.take());
        if let Some(w) = stray {
            end_wait(h, lua, w);
        }
        let _prio = enter_priority(prio);
        outcome = stretch(tasks, id, delivery, || thread.resume_error::<MultiValue>(OWN_YIELD));
    }
}

/// Whether the handler yielded the shim's `(WAIT, nonce)` with the nonce of the wait it registered
/// in this stretch.
fn parked_on_its_wait(tasks: &Tasks, id: TaskId, lua: &Lua, values: &MultiValue) -> bool {
    let (Some(Value::Table(a)), Some(Value::Table(b))) = (values.front(), values.get(1)) else { return false };
    let Ok(p) = prims(lua) else { return false };
    if a.to_pointer() != p.wait.to_pointer() {
        return false;
    }
    let table = tasks.table.borrow();
    let Some(Some(w)) = table.get(&id).map(|t| t.wait.as_ref()) else { return false };
    lua.registry_value::<Table>(&w.nonce).is_ok_and(|n| n.to_pointer() == b.to_pointer())
}

/// A wait given up: its read, its picture or its image worker's task withdrawn — a charge it made
/// given back — its nonce let go.
fn end_wait<H: ReadHost>(h: &H, lua: &Lua, w: Wait) {
    match w.on {
        On::Read(t) => reads::withdraw(h, t),
        On::Snap(s) => crate::snapshot::withdraw(h, s),
        On::Image(t) => crate::image_search::withdraw(h, t),
        _ => {}
    }
    let _ = lua.remove_registry_value(w.nonce);
}

/// Forgets handler `id`: out of the table, its wait withdrawn, its thread reset when that is safe —
/// not for one that `ended` (returned or raised), whose stack is empty already. Nothing of it runs
/// again.
fn forget<H: ReadHost>(h: &H, id: TaskId, ended: bool) {
    let Some(t) = h.tasks().table.borrow_mut().remove(&id) else { return };
    h.tasks().busy_said.borrow_mut().remove(&id);
    if let Some(w) = t.wait {
        end_wait(h, &t.lua, w);
    }
    if !t.running && !ended {
        reset(&t.lua, &t.thread);
    }
}

/// Handler `id`'s read was answered: it goes on with `values` — the reading, or the list and the
/// table by name — under its priority, unless it has ended meanwhile, its module is no longer
/// enabled or no longer runs that VM, or its coroutine is no longer stopped where the host left it
/// (closed by module code). True when it went on.
pub(crate) fn resume_wait<H: ReadHost>(h: &H, id: TaskId, ticket: TicketId, values: MultiValue) -> bool {
    resume_parked(h, id, |on| matches!(on, On::Read(t) if *t == ticket), || Ok(values))
}

/// Handler `id`'s picture `snap` was answered: it goes on with what `answer` builds — what the call
/// that does not wait returns — or raises the message `answer` gives at its call's line, by
/// [`resume_wait`]'s rules. `answer` is called only for a handler found still waiting for it, so
/// nothing is built in a VM that will not go on. True when it went on.
pub(crate) fn resume_snap<H: ReadHost>(
    h: &H,
    id: TaskId,
    snap: SnapId,
    answer: impl FnOnce() -> Result<MultiValue, String>,
) -> bool {
    resume_parked(h, id, |on| matches!(on, On::Snap(s) if *s == snap), answer)
}

/// Handler `id`'s task `image` of the image worker was answered: it goes on with what `answer`
/// builds, by [`resume_snap`]'s rules. True when it went on.
pub(crate) fn resume_image<H: ReadHost>(
    h: &H,
    id: TaskId,
    image: u64,
    answer: impl FnOnce() -> Result<MultiValue, String>,
) -> bool {
    resume_parked(h, id, |on| matches!(on, On::Image(t) if *t == image), answer)
}

/// Resumes handler `id`, parked on a wait `this` names, with what `answer` builds once the checks
/// passed: `(nonce, true, values…)`, or `(nonce, false, message)`, which the shim raises at the
/// call's line — by [`resume_wait`]'s rules. A timer's handler that inputs waited behind is said
/// ([`timer_wait_line`]), once per module and place.
fn resume_parked<H: ReadHost>(
    h: &H,
    id: TaskId,
    this: impl Fn(&On) -> bool,
    answer: impl FnOnce() -> Result<MultiValue, String>,
) -> bool {
    let tasks = h.tasks();
    let found = {
        let table = tasks.table.borrow();
        match table.get(&id) {
            Some(t) if !t.running && t.wait.as_ref().is_some_and(|w| this(&w.on)) => {
                Some((t.lua.clone(), t.owner, t.prio, t.thread.clone()))
            }
            _ => None,
        }
    };
    let Some((lua, owner, prio, thread)) = found else { return false };
    if !reads::deliverable_now(h, owner) || status(&lua, &thread).as_deref() != Some("suspended") {
        forget(h, id, false);
        return false;
    }
    let Some((w, timer)) = tasks.table.borrow_mut().get_mut(&id).and_then(|t| {
        let parked = t.parked_at.take().map(|at| at.elapsed()).unwrap_or_default();
        t.waited += parked;
        let behind = std::mem::take(&mut t.behind);
        let place = t.place.take();
        let timer = (t.ctx.what == "timer" && !behind.is_empty()).then(|| (parked, place.unwrap_or_default(), behind));
        t.wait.take().map(|w| (w, timer))
    }) else {
        return false;
    };
    if let Some((parked, place, behind)) = timer {
        let module = h.module_id(owner.idx);
        if tasks.timer_said.borrow_mut().insert((module.clone(), place.clone())) {
            logging::line(library_of(w.call), &timer_wait_line(&module, parked.as_millis(), w.call, &place, &behind));
        }
    }
    let nonce = lua.registry_value::<Table>(&w.nonce);
    let _ = lua.remove_registry_value(w.nonce);
    let Ok(nonce) = nonce else {
        forget(h, id, false);
        return false;
    };
    let mut args = match answer() {
        Ok(mut values) => {
            values.push_front(Value::Boolean(true));
            values
        }
        Err(message) => match lua.create_string(message) {
            Ok(s) => MultiValue::from_vec(vec![Value::Boolean(false), Value::String(s)]),
            Err(_) => {
                // A VM too full for the message: the handler cannot go on in it.
                forget(h, id, false);
                return false;
            }
        },
    };
    args.push_front(Value::Table(nonce));
    let _prio = enter_priority(prio);
    let delivery = enter_delivery(tasks, id);
    let outcome = stretch(tasks, id, &delivery, || thread.resume::<MultiValue>(args));
    step(h, id, &lua, &thread, &delivery, outcome);
    drop(delivery);
    true
}

/// Somebody waits on an event that queued behind module `owner`'s parked handler — it came in the
/// interactive lane (`mailbox.rs`): the read or the picture the handler waits for is made
/// interactive (`Service::promote`, `Service::promote_snap`), and so is everything it waits for
/// after it (`raised`). A wait of a poll would otherwise hold the key behind it as long as the
/// polls' lane takes. A read being recognised, or a picture being taken, already runs on as it
/// started. The handler's own lane stays: what it arms after the wait — a timer, a read with a
/// callback — is in the lane it began in, so a poll stays a poll. A profile whose picture is still
/// to come goes to the image worker interactive too (`SnapState::raise`). A task of the image worker
/// it waits for keeps the priority it was sent with, since the worker has taken it into its queue
/// already; the handler's next waits are interactive. True when a handler that was parked in the
/// background lane was raised.
pub(crate) fn promote<H: ReadHost>(h: &H, owner: Owner) -> bool {
    let on = {
        let mut table = h.tasks().table.borrow_mut();
        let Some(t) = table
            .values_mut()
            .find(|t| t.owner == owner && t.is_handler() && !t.running && t.prio == Priority::Background && !t.raised)
        else {
            return false;
        };
        t.raised = true;
        match t.wait.as_ref().map(|w| &w.on) {
            Some(On::Read(ticket)) => Some(On::Read(*ticket)),
            Some(On::Snap(snap)) => Some(On::Snap(*snap)),
            _ => None,
        }
    };
    match on {
        Some(On::Read(ticket)) => {
            h.ocr().promote(ticket);
        }
        Some(On::Snap(snap)) => {
            h.snap_state().raise(snap);
            h.ocr().promote_snap(snap);
        }
        _ => {}
    }
    true
}

/// Handler `id`'s read could not be handed over (its reading could not be built): it ends.
pub(crate) fn end_waiting<H: ReadHost>(h: &H, id: TaskId) {
    if tasks_parked(h.tasks(), id) {
        forget(h, id, false);
    }
}

fn tasks_parked(tasks: &Tasks, id: TaskId) -> bool {
    tasks.table.borrow().get(&id).is_some_and(|t| !t.running)
}

/// `cancel(n)`: a parked task ends now and its read is withdrawn; a running one ends at its next
/// wait. True when this call ended something or marked it to end; false for another VM's number,
/// one that ended, one a cancel marked already, or anything that is no number. Never raises. Only
/// the tests' entry calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn cancel<H: ReadHost>(h: &H, lua: &Lua, scope: usize, n: &Value) -> bool {
    let Some(id) = number_of(n) else { return false };
    let me = owner(h, lua, scope);
    let state = {
        let table = h.tasks().table.borrow();
        match table.get(&id) {
            Some(t) if t.owner == me => Some((t.running, t.gone, t.lua.clone(), t.thread.clone())),
            _ => None,
        }
    };
    let Some((running, gone, lua, thread)) = state else { return false };
    if gone {
        return false;
    }
    if running {
        // Ends at its next wait. A second cancel ends nothing the first did not: false.
        let mut table = h.tasks().table.borrow_mut();
        return table.get_mut(&id).is_some_and(|t| !std::mem::replace(&mut t.ending, true));
    }
    // Closed by module code meanwhile: it had ended already.
    let ended = status(&lua, &thread).as_deref() != Some("suspended");
    forget(h, id, false);
    !ended
}

/// `alive(n)`: true from `run` until the task ends. False for another VM's number and
/// for anything that is no number. Never raises. Only the tests' entry calls it.
#[cfg_attr(not(test), allow(dead_code))]
fn alive<H: ReadHost>(h: &H, lua: &Lua, scope: usize, n: &Value) -> bool {
    let Some(id) = number_of(n) else { return false };
    let me = owner(h, lua, scope);
    let state = {
        let table = h.tasks().table.borrow();
        match table.get(&id) {
            Some(t) if t.owner == me => Some((t.running, t.gone, t.lua.clone(), t.thread.clone())),
            _ => None,
        }
    };
    let Some((running, gone, lua, thread)) = state else { return false };
    if running {
        return !gone;
    }
    if status(&lua, &thread).as_deref() == Some("suspended") {
        return true;
    }
    forget(h, id, false); // module code closed it
    false
}

/// Drops every handler of module `idx` — disabled, reloaded, removed: nothing of them runs again,
/// and their reads are withdrawn. A handler whose stretch is on the stack right now is forgotten
/// when it returns, and its wait goes now. The module is free for its mailbox again.
pub(crate) fn drop_owner<H: ReadHost>(h: &H, idx: usize) {
    drop_where(h, |i| i == idx);
}

/// `rollback_to(n)`'s share: every handler of a module from index `n` on.
pub(crate) fn drop_from<H: ReadHost>(h: &H, n: usize) {
    drop_where(h, |i| i >= n);
}

fn drop_where<H: ReadHost>(h: &H, which: impl Fn(usize) -> bool) {
    let ids: Vec<(TaskId, bool)> =
        h.tasks().table.borrow().iter().filter(|(_, t)| which(t.owner.idx)).map(|(id, t)| (*id, t.running)).collect();
    for (id, running) in ids {
        if running {
            let w = {
                let mut table = h.tasks().table.borrow_mut();
                table.get_mut(&id).map(|t| {
                    t.gone = true;
                    t.ending = true;
                    (t.lua.clone(), t.wait.take())
                })
            };
            if let Some((lua, Some(w))) = w {
                end_wait(h, &lua, w);
            }
        } else {
            forget(h, id, false);
        }
    }
}

/// The shim's `where_`: which of its three answers holds for the calling coroutine of `lua`.
/// "handler": the innermost running handler of this VM calls, in its own coroutine; "foreign":
/// another coroutine of the VM calls inside such a handler; "none": no handler of the VM runs —
/// outside every callback.
fn where_now<H: ReadHost>(h: &H, lua: &Lua, scope: usize) -> &'static str {
    let tasks = h.tasks();
    let Some(id) = tasks.innermost(owner(h, lua, scope)) else { return "none" };
    let calling = lua.current_thread().to_pointer() as usize;
    match tasks.table.borrow().get(&id) {
        Some(t) if t.ptr == calling => "handler",
        _ => "foreign",
    }
}

/// The innermost running handler of the VM, if it calls in its own coroutine with no wait in this
/// stretch yet — the one place a wait may be registered from.
fn in_place(tasks: &Tasks, me: Owner, calling: usize) -> Option<TaskId> {
    let id = tasks.innermost(me)?;
    let table = tasks.table.borrow();
    let t = table.get(&id)?;
    // A handler whose module went while it ran (`gone`) is in its place too: it is `ending`, so
    // its wait is a void one, and `step` forgets it there — it never goes on.
    (t.ptr == calling && t.wait.is_none()).then_some(id)
}

/// Where in the module the wait the calling coroutine of `lua` asks for was asked: the first frame
/// of its stack that is Luau and not the shim's — past a `pcall` around the call — as
/// `<module>/<file>:<line>`, or the chunk's name and the line for a chunk no module loaded. Asked of
/// Luau's own stack, so no module code can change the answer.
fn caller_place(lua: &Lua) -> Option<String> {
    (0..12).map_while(|level| crate::vm_guard::raw_frame(lua, level)).find_map(|f| {
        let source = f.source.clone().unwrap_or_default();
        if f.what == "C" || source == SHIM_NAME || source == SCREEN_SHIM_NAME {
            return None;
        }
        Some(crate::vm_guard::frame_place(lua, &f))
    })
}

/// A shim's `start`: files what a wait of `call` asks for, for the innermost running handler of
/// this VM, which must be the one calling, in its own coroutine, with no wait in this stretch yet.
/// `parse` reads the call's arguments, and raises for a mistake in them; `submit` files what they
/// ask for, for the handler it is given at the priority its wait asks with, and says what the wait
/// waits on. In a module that is not enabled, or for a handler cancelled while it ran, nothing is
/// filed: the wait ends the handler.
pub(crate) fn start_wait<H: ReadHost, A>(
    h: &H,
    lua: &Lua,
    scope: usize,
    call: &'static str,
    nonce: Table,
    parse: impl FnOnce() -> mlua::Result<A>,
    submit: impl FnOnce(A, TaskId, Priority) -> mlua::Result<On>,
) -> mlua::Result<()> {
    let waits = |args: A, id: TaskId, prio: Priority| submit(args, id, prio).map(Started::Waits);
    start_point(h, lua, scope, call, nonce, parse, waits).map(|_| ())
}

/// What a wait point's `start` made of a call: a wait for what it asks ([`On`]), or its answer
/// at once — a `host.screen.pixel` or `pixels` of a snapshot, a window point with no pixel this
/// time, an empty list: a call that touches nothing a handler would wait for.
pub(crate) enum Started {
    Waits(On),
    Now(MultiValue),
}

/// [`start_wait`], for a call that may also be answered at once: `submit` says which. What the
/// shim's `start` returns: nothing when the handler is to wait — the wait is registered — or
/// `true` and the answer, which the shim returns without waiting. The checks are `start_wait`'s,
/// and a handler that is to end — its module not enabled, or cancelled while it ran — ends at the
/// call whichever it would have been.
pub(crate) fn start_point<H: ReadHost, A>(
    h: &H,
    lua: &Lua,
    scope: usize,
    call: &'static str,
    nonce: Table,
    parse: impl FnOnce() -> mlua::Result<A>,
    submit: impl FnOnce(A, TaskId, Priority) -> mlua::Result<Started>,
) -> mlua::Result<MultiValue> {
    let me = owner(h, lua, scope);
    let calling = lua.current_thread().to_pointer() as usize;
    let tasks = h.tasks();
    let misplaced = || mlua::Error::external(format!("{call}: the host's wait was called from outside its place"));
    let first = in_place(tasks, me, calling).ok_or_else(misplaced)?;
    // A handler whose module went is not asked anything more: not even its arguments are read,
    // which could raise, and a `pcall` around the wait would then run on in a module that is gone.
    let gone = tasks.table.borrow().get(&first).is_some_and(|t| t.gone);
    let args = if gone { None } else { Some(parse()?) };
    // Reading the arguments can run module code (a metamethod), which may have registered a wait
    // or ended the handler: everything is checked again, and the wait registered only now.
    let id = in_place(tasks, me, calling).ok_or_else(misplaced)?;
    // A raised handler's waits are interactive; its lane is not (`promote`).
    let (ending, prio) = tasks
        .table
        .borrow()
        .get(&id)
        .map(|t| (t.ending, if t.raised { Priority::Interactive } else { t.prio }))
        .unwrap_or((true, Priority::Background));
    let on = match args {
        Some(args) if !ending && h.module_enabled(me.idx) => match submit(args, id, prio)? {
            Started::Waits(on) => on,
            Started::Now(mut values) => {
                values.push_front(Value::Boolean(true));
                return Ok(values);
            }
        },
        _ => On::Void,
    };
    // Asked of Luau's stack, which filing the wait has not touched.
    let place = caller_place(lua);
    let nonce = lua.create_registry_value(nonce)?;
    match tasks.table.borrow_mut().get_mut(&id) {
        Some(t) => {
            t.wait = Some(Wait { on, nonce, call });
            t.place = place;
        }
        None => {
            // Cannot happen: nothing ran between the check and here.
            let _ = lua.remove_registry_value(nonce);
        }
    }
    Ok(MultiValue::new())
}

/// `recognize`'s `start`: the read a wait asks for ([`start_wait`]).
fn start<H: ReadHost>(h: &H, lua: &Lua, scope: usize, nonce: Table, what: Value, opts: Value) -> mlua::Result<()> {
    let idx = owner(h, lua, scope).idx;
    start_wait(h, lua, scope, "host.ocr.recognize", nonce, || reads::parse_read(&what, &opts), |args, id, prio| {
        let list = args.list;
        Ok(On::Read(reads::submit(h, lua, idx, args, prio, Waiter::Handler { id, list })?))
    })
}

/// The shim's `disown`: module code resumed a parked handler's coroutine itself. The host forgets
/// it — without a reset, since that code is running it — and withdraws its read or its picture.
fn disown<H: ReadHost>(h: &H, lua: &Lua) {
    let calling = lua.current_thread().to_pointer() as usize;
    let id = h.tasks().table.borrow().iter().find(|(_, t)| t.ptr == calling && !t.running).map(|(id, _)| *id);
    let Some(id) = id else { return };
    let Some(t) = h.tasks().table.borrow_mut().remove(&id) else { return };
    if let Some(w) = t.wait {
        end_wait(h, &t.lua, w);
    }
}

/// What a shim's `now` returns ([`points`]): `true` and the call's answer, for a call that touches
/// nothing it would wait for — the points of a snapshot, a window with no pixel this time — and is
/// answered at once wherever it is made, as a handler's is ([`Started::Now`]); or nothing, for one
/// that would wait, which the shim then raises [`wait_message`] for, for the place.
pub(crate) fn now_answer(answer: Option<MultiValue>) -> MultiValue {
    match answer {
        Some(mut values) => {
            values.push_front(Value::Boolean(true));
            values
        }
        None => MultiValue::new(),
    }
}

/// The shim of VM `lua`, loaded under `name` — the library whose calls it carries, which a raise in
/// them shows in the traceback of a module's error, in the log and in the error dialog — with its
/// `where_` and `disown` over `holder`, and `messages`, which gives a call's three messages for the
/// places it cannot wait ([`wait_message`]): the table of its two builders, `recognize` and `point`.
fn shim<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>, name: &str) -> mlua::Result<Table> {
    let p = prims(lua)?;
    let prim: Table = lua.named_registry_value(PRIMS_KEY)?;
    let h = holder.clone();
    let where_ = lua.create_function(move |lua, ()| Ok(where_now(&*h, lua, scope)))?;
    let h = holder;
    let disown = lua.create_function(move |lua, ()| {
        disown(&*h, lua);
        Ok(())
    })?;
    let messages = lua.create_function(|_, call: String| {
        Ok((
            wait_message(&call, Case::Outside),
            wait_message(&call, Case::CannotWait),
            wait_message(&call, Case::OwnCoroutine),
        ))
    })?;
    let shim: Function = lua.load(SHIM).set_name(name).eval()?;
    shim.call((
        where_,
        disown,
        messages,
        prim.get::<Function>("yield")?,
        prim.get::<Function>("isyieldable")?,
        prim.get::<Function>("rawequal")?,
        prim.get::<Function>("error")?,
        prim.get::<Function>("select")?,
        prim.get::<Function>("type")?,
        prim.get::<Function>("pack")?,
        prim.get::<Function>("unpack")?,
        p.wait,
        RESUMED,
    ))
}

/// `host.ocr.recognize` of VM `lua` — the table with `recognize` the shim returns — built on the
/// first call and the same table after it. `submit` is the callback form, `submit(what, opts, cb)`,
/// which opens the guard's host call itself, as the shim's `start` does here. Where the call
/// without one cannot wait, the shim raises [`wait_message`] for the place, whatever it was given.
pub(crate) fn waits<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>, submit: Function) -> mlua::Result<Table> {
    if let Ok(t) = lua.named_registry_value::<Table>(WAITS_KEY) {
        return Ok(t);
    }
    let h = holder.clone();
    let start = lua.create_function(move |lua, (nonce, what, opts): (Table, Value, Value)| {
        // Named for the guard as the module wrote it: a stop in it says "in host.ocr.recognize".
        static NAME: &str = "host.ocr.recognize";
        let _host_call = crate::vm_guard::HostCall::enter(&NAME);
        start(&*h, lua, scope, nonce, what, opts)
    })?;
    let builders = shim(lua, scope, holder, SHIM_NAME)?;
    let recognize: Function = builders.get::<Function>("recognize")?.call((start, submit, NIL_CB))?;
    let t = lua.create_table()?;
    t.set("recognize", recognize)?;
    lua.set_named_registry_value(WAITS_KEY, t.clone())?;
    Ok(t)
}

/// The builder of VM `lua`'s `host.screen` calls that wait — `point(name, start, now?, submit?, at?)`
/// of the shim loaded as `host.screen` — made on the first call and the same function after it. The
/// call it builds waits in a handler, by `start(nonce, …)` ([`start_wait`]); anywhere else it is
/// answered by `now(…)` when that answers at once ([`now_answer`]), and otherwise raises
/// [`wait_message`] for the place, at the caller's line; given `submit`, a call whose argument `at`
/// is not `nil` is its callback form, `submit(…)`, wherever it is made. A call without `now` raises
/// wherever it cannot wait, whatever it was given.
pub(crate) fn points<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>) -> mlua::Result<Function> {
    if let Ok(f) = lua.named_registry_value::<Function>(POINTS_KEY) {
        return Ok(f);
    }
    let point: Function = shim(lua, scope, holder, SCREEN_SHIM_NAME)?.get("point")?;
    lua.set_named_registry_value(POINTS_KEY, point.clone())?;
    Ok(point)
}

/// The tests' own wait point: `wait(name)` stops the handler that calls it, in its own coroutine,
/// until a test answers it with [`test_release`] — host state, as the rule for wait points asks.
/// Built like the shim's wait: how the tests make a module busy without a read.
#[cfg(test)]
pub(crate) fn test_wait<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>) -> mlua::Result<Function> {
    const TEST_SHIM: &str = r#"
        local start, disown, yield, rawequal, error, WAIT = ...
        return function(name)
          local nonce = {}
          start(name, nonce)
          local got, _, value = yield(WAIT, nonce)
          if not rawequal(got, nonce) then
            disown()
            error("the test wait was resumed by the module's own code", 2)
          end
          return value
        end
    "#;
    let p = prims(lua)?;
    let prim: Table = lua.named_registry_value(PRIMS_KEY)?;
    let h = holder.clone();
    let start = lua.create_function(move |lua, (name, nonce): (String, Table)| {
        let me = owner(&*h, lua, scope);
        let calling = lua.current_thread().to_pointer() as usize;
        let tasks = h.tasks();
        let id = in_place(tasks, me, calling)
            .ok_or_else(|| mlua::Error::external(format!("the test wait '{name}' was called outside a handler")))?;
        let ending = tasks.table.borrow().get(&id).is_none_or(|t| t.ending);
        let on = if !ending && h.module_enabled(me.idx) { On::Test(name) } else { On::Void };
        let nonce = lua.create_registry_value(nonce)?;
        if let Some(t) = tasks.table.borrow_mut().get_mut(&id) {
            t.wait = Some(Wait { on, nonce, call: "the test wait" });
        }
        Ok(())
    })?;
    let h = holder;
    let disown = lua.create_function(move |lua, ()| {
        disown(&*h, lua);
        Ok(())
    })?;
    lua.load(TEST_SHIM).set_name("=test wait").call((
        start,
        disown,
        prim.get::<Function>("yield")?,
        prim.get::<Function>("rawequal")?,
        prim.get::<Function>("error")?,
        p.wait,
    ))
}

/// A `host.screen` call `call` built as the host builds one ([`points`]), whose wait is the tests'
/// own wait point of the same name — answered by [`test_release`] — and which raises the host's
/// message where it cannot wait: for the scripted host, whose screen has no capture thread, so a
/// scenario can show where such a call waits.
#[cfg(test)]
pub(crate) fn test_point<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>, call: &'static str) -> mlua::Result<Function> {
    let h = holder.clone();
    let start = lua.create_function(move |lua, (nonce, _args): (Table, MultiValue)| {
        start_wait(&*h, lua, scope, call, nonce, || Ok(()), |(), _, _| Ok(On::Test(call.to_string())))
    })?;
    points(lua, scope, holder)?.call((call, start))
}

/// The shim's `where_` for the tests' own stand-ins of `recognize`: `whereNow()` answers what the
/// shim asks before it waits ("handler", "foreign" or "none"), so a fake that answers a wait at once
/// can refuse it where the real one could not wait (`T.answerWaits` in overlay_menu_tests.rs).
#[cfg(test)]
pub(crate) fn test_where<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>) -> mlua::Result<Function> {
    lua.create_function(move |lua, ()| Ok(where_now(&*holder, lua, scope)))
}

/// Answers the tests' wait point `name`: the handler parked there goes on with `value`, by
/// [`resume_wait`]'s rules. True when one went on.
#[cfg(test)]
pub(crate) fn test_release<H: ReadHost>(h: &H, name: &str, value: Value) -> bool {
    let id = h
        .tasks()
        .table
        .borrow()
        .iter()
        .find(|(_, t)| !t.running && t.wait.as_ref().is_some_and(|w| matches!(&w.on, On::Test(n) if n == name)))
        .map(|(id, _)| *id);
    let Some(id) = id else { return false };
    resume_parked(h, id, |on| matches!(on, On::Test(n) if n == name), || Ok(MultiValue::from_vec(vec![value])))
}

/// Whether a handler is parked at the tests' wait point `name`.
#[cfg(test)]
pub(crate) fn test_waiting<H: ReadHost>(h: &H, name: &str) -> bool {
    h.tasks()
        .table
        .borrow()
        .values()
        .any(|t| !t.running && t.wait.as_ref().is_some_and(|w| matches!(&w.on, On::Test(n) if n == name)))
}

impl crate::Shared {
    /// Drops every handler of module `idx` ([`drop_owner`]): beside `ocr_drop_owner`, wherever a
    /// module is disabled, reloaded or removed.
    pub(crate) fn task_drop_owner(&self, idx: usize) {
        drop_owner(self, idx);
    }

    /// `rollback_to(n)`'s share ([`drop_from`]).
    pub(crate) fn task_drop_from(&self, n: usize) {
        drop_from(self, n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every call that waits has three messages, one for each place it cannot wait, and each names
    /// the call, the place and the call's callback form, which waits for nothing — and no way to
    /// start a task, which no module has. docs/api/ocr.md shows `recognize`'s three and
    /// docs/api/screen.md those of `host.screen.pixel`, word for word.
    #[test]
    fn the_three_messages_name_the_call_the_place_and_the_callback_form() {
        const OCR_MD: &str = include_str!("../../../docs/api/ocr.md");
        const SCREEN_MD: &str = include_str!("../../../docs/api/screen.md");
        const CASES: [Case; 3] = [Case::Outside, Case::CannotWait, Case::OwnCoroutine];
        for call in [
            "host.ocr.recognize",
            "host.screen.snapshot",
            "host.screen.pixel",
            "host.screen.pixels",
            "host.screen.profile",
            "host.screen.cells",
            "host.screen.matchCells",
            "host.screen.template",
            "Snapshot:crop",
            "host.screen.imageSearch",
            "host.screen.imageSearchAll",
            "host.screen.save",
            "host.screen.saveMarked",
        ] {
            let form = callback_form(call).expect("every call that waits has a callback form");
            // The callback's error is `err`, as every page names it; a reading has none of its own.
            if call != "host.ocr.recognize" {
                assert!(form.ends_with(", err) … end)"), "{form}");
            }
            for case in CASES {
                let m = wait_message(call, case);
                assert!(m.starts_with(&format!("{call} cannot wait ")), "{m}");
                assert!(m.contains(&format!("ass a callback, {form}")), "{m}");
                assert!(!m.contains("task") && !m.contains("host.ocr.read"), "{m}");
            }
            assert!(wait_message(call, Case::Outside).contains("Call it from a handler, such as host.timer.after(0, …)"));
            assert!(wait_message(call, Case::CannotWait).contains("or make the call in the event's own function"));
            assert!(wait_message(call, Case::OwnCoroutine)
                .contains("In a hook, measure it in a handler and let the hook read what was measured (see \"Where a hook runs\" in docs/api/overlay.md)"));
        }
        for case in CASES {
            let read = wait_message("host.ocr.recognize", case);
            assert!(OCR_MD.contains(&read), "docs/api/ocr.md does not show this message word for word:\n{read}");
            let pixel = wait_message("host.screen.pixel", case);
            assert!(SCREEN_MD.contains(&pixel), "docs/api/screen.md does not show this message word for word:\n{pixel}");
        }
        assert!(OCR_MD.contains(NIL_CB), "docs/api/ocr.md does not show the nil callback's message:\n{NIL_CB}");
        assert_eq!(
            wait_message("host.screen.save", Case::Outside),
            "host.screen.save cannot wait here: this runs outside every event of the module — its top level, an \
             included file's, or a function the host calls between its events. Call it from a handler, such as \
             host.timer.after(0, …), or pass a callback, host.screen.save(path, opts, function(saved, err) … end)"
        );
    }

    /// The shim's three words are the three places; a word it never says is one that cannot wait.
    /// Each place has a message of its own.
    #[test]
    fn the_shims_words_are_read_as_the_three_cases() {
        assert_eq!(Case::of("none"), Case::Outside);
        assert_eq!(Case::of("cannot-wait"), Case::CannotWait);
        assert_eq!(Case::of("foreign"), Case::OwnCoroutine);
        assert_eq!(Case::of("anything else"), Case::CannotWait, "a word the shim never says");
        let [outside, cannot, own] = [Case::Outside, Case::CannotWait, Case::OwnCoroutine].map(|c| wait_message("host.screen.pixel", c));
        assert!(outside != cannot && cannot != own && own != outside);
    }

    /// The timer's line is the one docs/api/ocr.md shows for a read and docs/api/screen.md for a
    /// snapshot, word for word: it names the call that waited and its form that waits for nothing.
    #[test]
    fn the_timers_line_is_the_documented_one() {
        const OCR_MD: &str = include_str!("../../../docs/api/ocr.md");
        const SCREEN_MD: &str = include_str!("../../../docs/api/screen.md");
        let keys = ["Tab".to_string(), "Tab".to_string(), "Down".to_string()];
        let read = timer_wait_line("com.example.game", 2140, "host.ocr.recognize", "com.example.game/src/main.luau:88", &keys);
        assert!(OCR_MD.contains(&read), "docs/api/ocr.md does not show this line word for word:\n{read}");
        let snap = timer_wait_line("com.example.game", 63, "host.screen.snapshot", "com.example.game/src/main.luau:41", &keys);
        assert!(SCREEN_MD.contains(&snap), "docs/api/screen.md does not show this line word for word:\n{snap}");
        let points = timer_wait_line("com.example.game", 24, "host.screen.pixels", "com.example.game/src/main.luau:30", &keys[..2]);
        assert!(SCREEN_MD.contains(&points), "docs/api/screen.md does not show this line word for word:\n{points}");
    }

    /// The timer's line names its call's callback form, and a call that has none, none; a call's
    /// library is the page it is on and the scope of its lines.
    #[test]
    fn the_timers_line_names_the_callback_form_and_a_call_belongs_to_its_library() {
        assert_eq!(library_of("host.ocr.recognize"), "ocr");
        assert_eq!(library_of("host.screen.pixel"), "screen");
        assert_eq!(library_of("Snapshot:crop"), "screen", "a Snapshot's method is host.screen's");
        let keys = ["Tab".to_string(), "Tab".to_string(), "Down".to_string()];
        assert_eq!(
            timer_wait_line("m", 40, "host.ocr.recognize", "m/src/main.luau:12", &keys),
            "[m] a timer waited 40 ms for host.ocr.recognize at m/src/main.luau:12; 3 key(s) (Tab \u{d7}2, Down) waited \
             behind it — a poll should not wait: host.ocr.recognize(what, opts, function(reading) … end)"
        );
        assert_eq!(
            timer_wait_line("m", 40, "host.screen.save", "m/src/main.luau:12", &keys[..1]),
            "[m] a timer waited 40 ms for host.screen.save at m/src/main.luau:12; 1 key(s) (Tab) waited behind it — a \
             poll should not wait: host.screen.save(path, opts, function(saved, err) … end)"
        );
        assert_eq!(
            timer_wait_line("m", 40, "host.screen.size", "m/src/main.luau:12", &keys[..1]),
            "[m] a timer waited 40 ms for host.screen.size at m/src/main.luau:12; 1 key(s) (Tab) waited behind it — a \
             poll should not wait"
        );
    }

    /// The message a callback's own `coroutine.yield` raises is the one module-runtime-and-lifecycle.md
    /// gives, word for word, and names neither a task nor a read.
    #[test]
    fn the_own_yield_message_is_the_documented_one() {
        const LIFECYCLE: &str = include_str!("../../../docs/module-runtime-and-lifecycle.md");
        assert!(LIFECYCLE.contains(OWN_YIELD), "module-runtime-and-lifecycle.md does not show it:\n{OWN_YIELD}");
        assert!(!OWN_YIELD.contains("task") && !OWN_YIELD.contains("recognize"), "{OWN_YIELD}");
        assert!(!RESUMED.contains("task"), "{RESUMED}");
    }
}
