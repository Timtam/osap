//! Handlers: every callback of a module runs as a coroutine of its VM — a handler — that can stop
//! at a wait while the event loop goes on; and the wait it stops at, `host.ocr.recognize` without a
//! callback, which raises where it cannot wait.
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
//! busy meanwhile, and nothing else is. With a callback it is the read that never waits.
//!
//! **Why.** No text recognition may burden the event loop. `recognize` used to photograph and
//! recognise on the loop and return the reading; where it waits it asks the read service instead
//! (`ocr/lua.rs`), and only its module waits. Where it cannot wait — a place Luau cannot suspend, a
//! function the host calls and waits for, a coroutine the module made — it raises
//! ([`wait_message`]); the callback form reads there.
//!
//! **How it waits.** A Rust function cannot yield through mlua, so the wait is a `coroutine.yield`
//! in a small Luau shim the host builds once per VM (`task_shim.luau`), with every helper an
//! upvalue. The host creates and resumes every handler with mlua's own `Thread` API: the first
//! stretch at once, inside `run_handler`; every later one from the tick, in the delivery of text
//! readings, while the module is enabled and its VM is the one that asked. A handler is never
//! resumed into a VM that was disabled, reloaded or removed, and nothing of it runs after it ends —
//! Luau has no `__gc` and no `__close`, so throwing a stopped thread away runs no code.
//!
//! **Who answers a wait.** A wait point is answered only by a thread of the host or by state the
//! host keeps — the capture and recognise threads through the delivery of readings, the tests'
//! `test_release` — never by an event delivered to the waiting module: that event waits in the
//! module's mailbox behind the very handler it would answer. A "wait for my own key-up" built on
//! the module's own key events could never end; one answered from the hook's record of held keys
//! could.
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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, RegistryKey, Table, Thread, ThreadStatus, Value};

use crate::logging;
use crate::ocr::lua::{self as reads, owner_of, ReadHost, Waiter};
use crate::ocr::sched::{Owner, TicketId};
use crate::ocr::types::{current_priority, enter_priority, Priority};

/// A handler's number: positive, unique for the life of the process, never reused — as a timer's.
pub(crate) type TaskId = i64;

/// The shim, compiled once per VM (`waits`).
const SHIM: &str = include_str!("task_shim.luau");

/// The shim's chunk name, which a traceback and `debug.info` show: `host.ocr`, the calls it
/// carries. The `=` keeps Luau from wrapping it as `[string "…"]`.
pub(crate) const SHIM_NAME: &str = "=host.ocr";

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

/// The named registry's per-VM entries: the handles below, and the shim's `recognize`. Luau has no
/// `debug.getregistry`, so no module reaches them.
const PRIMS_KEY: &str = "__task_prims";
const WAITS_KEY: &str = "__task_waits";

/// The two kinds of place `recognize` without a callback cannot wait, as the shim names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Case {
    /// Outside every handler of the VM, or in one at a place Luau cannot stop it: the top level of
    /// a module or of an included file, a function the host calls and waits for (an arbiter's
    /// `onActivate`, an `onChange` from the module's own `set`), a metamethod, a sort comparator.
    CannotWait,
    /// In a coroutine the module made, or the overlay runtime made for a hook that must not read,
    /// inside a handler.
    OwnCoroutine,
}

impl Case {
    /// The shim's word for it: "foreign" is a coroutine of the module's own; "none" and
    /// "cannot-wait" — and a word it never says — are a place that cannot wait.
    pub(crate) fn of(why: &str) -> Case {
        match why {
            "foreign" => Case::OwnCoroutine,
            _ => Case::CannotWait,
        }
    }

    /// The words the blocking call's log line gives the place.
    fn words(self) -> &'static str {
        match self {
            Case::CannotWait => "a place Luau cannot suspend, or a function the host calls and waits for",
            Case::OwnCoroutine => "a coroutine the module made",
        }
    }
}

/// What `recognize` without a callback raises in a place of `case`, at the caller's line. The shim
/// raises it, in the scripted test host as in the host, so a scenario that reads where it could
/// not wait fails. docs/api/ocr.md gives both, word for word.
pub(crate) fn wait_message(case: Case) -> &'static str {
    match case {
        Case::CannotWait => {
            "host.ocr.recognize cannot wait here: this function runs inside one the host calls and waits for, or \
             one Luau cannot suspend — an arbiter's onActivate or onDeactivate, an onChange fired by your own \
             host.settings.set, the top level of a module or of an included file, a metamethod, a table.sort \
             comparator, a string.gsub function, the iterator of a for loop, an xpcall error handler. Pass a \
             callback, host.ocr.recognize(what, opts, function(reading) … end), or make the call in the event's \
             own function"
        }
        Case::OwnCoroutine => {
            "host.ocr.recognize cannot wait in a coroutine the module made, or one the overlay runtime made for a \
             hook that must not read (when, present, menu tests, an identify asked on every evaluation): only the \
             host's own coroutine for an event waits. Pass a callback, or call it from the event's own function"
        }
    }
}

/// The library a call belongs to — `ocr` for `host.ocr.recognize`, `screen` for
/// `host.screen.pixel` — which names its page in docs/api and the scope of its log lines.
fn library_of(call: &str) -> &str {
    call.strip_prefix("host.").and_then(|c| c.split('.').next()).unwrap_or(call)
}

/// The line a blocking call writes, once per module, call and case: where it could not wait, how
/// long it held the event loop instead, and what to write.
///
/// What to write is one remedy for every place, as the shim tells only two kinds apart. Before a
/// `host.screen` call comes here it is to depend on the place: at a module's top level, a handler
/// such as `host.timer.after(0, …)`; in the overlay runtime's hooks, a measurement made in a
/// handler for the hook to read; elsewhere the event's own function, or a callback (TODO.md, "The
/// synchronous `host.screen` captures").
pub(crate) fn legacy_line(module: &str, call: &str, case: Case, ms: u128) -> String {
    format!(
        "[{module}] {call} could not wait here ({}) and held the event loop {ms} ms instead; call it from the \
         event's own function; see \"Where it waits\" in docs/api/{}.md",
        case.words(),
        library_of(call)
    )
}

/// The line for a timer whose handler waited for a read while inputs queued behind it, once per
/// module and place in a session: a poll that holds the module's keys (docs/api/ocr.md shows it).
pub(crate) fn timer_wait_line(module: &str, ms: u128, place: &str, keys: &[String]) -> String {
    format!(
        "[{module}] a timer waited {ms} ms for a text read at {place}; {} key(s) ({}) waited behind it — a poll that \
         reads should pass a callback: host.ocr.recognize(what, opts, function(reading) … end)",
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
    /// image search's binding, "screen.snapshotAsync", "settings onChange (<key>)".
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
enum On {
    /// A read of the service.
    Read(TicketId),
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

/// What the blocking calls cost, per module and call.
#[derive(Default)]
struct LegacyTally {
    /// (module id, call, case): written once — and once more after `forget_legacy_said`, which a
    /// reload, a disable or enable and a rolled-back hot-load call as they forget the module's
    /// error repeats.
    said: HashSet<(String, &'static str, Case)>,
    /// (module id, call) → (calls, milliseconds held), for the summary at exit.
    held: BTreeMap<(String, &'static str), (u64, u128)>,
}

/// Every handler of every module, and what the blocking calls held. Main thread only.
#[derive(Default)]
pub(crate) struct Tasks {
    table: RefCell<HashMap<TaskId, Task>>,
    /// The handlers whose stretches are on the host's stack, innermost last, across every VM:
    /// what `MAX_NESTED` counts.
    running: RefCell<Vec<TaskId>>,
    next: Cell<TaskId>,
    legacy: RefCell<LegacyTally>,
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

    /// The summary at exit: per module and call, how often it held the event loop where it could
    /// not wait and for how long in all, with its call's library as the line's scope. Nothing for
    /// a session in which no call did.
    pub(crate) fn legacy_summary(&self) -> Vec<(&'static str, String)> {
        self.legacy
            .borrow()
            .held
            .iter()
            .map(|((id, call), (n, ms))| {
                let line = format!("[{id}] {call} held the event loop {n} time(s), {ms} ms in all, where it could not wait");
                (library_of(call), line)
            })
            .collect()
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

    /// Whether the line for module `id`, `call` and `case` was written, for the tests.
    #[cfg(test)]
    pub(crate) fn legacy_said(&self, id: &str, call: &'static str, case: Case) -> bool {
        self.legacy.borrow().said.contains(&(id.to_string(), call, case))
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

    /// Lets module `id`'s lines be written again: its next blocking call of each call and case is
    /// news, as its next error is (`forget_error_repeats`, which calls this). Keyed by id, so a
    /// module that a rolled-back hot-load's index goes to next has lines of its own. The summary
    /// at exit keeps counting.
    pub(crate) fn forget_legacy_said(&self, id: &str) {
        self.legacy.borrow_mut().said.retain(|(m, _, _)| m != id);
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

/// A wait given up: its read withdrawn, its nonce let go.
fn end_wait<H: ReadHost>(h: &H, lua: &Lua, w: Wait) {
    if let On::Read(t) = w.on {
        reads::withdraw(h, t);
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
    resume_parked(h, id, |on| matches!(on, On::Read(t) if *t == ticket), values)
}

/// Resumes handler `id`, parked on a wait `this` names, with `values`: [`resume_wait`]'s rules. A
/// timer's handler that inputs waited behind is said ([`timer_wait_line`]), once per module and
/// place.
fn resume_parked<H: ReadHost>(h: &H, id: TaskId, this: impl Fn(&On) -> bool, values: MultiValue) -> bool {
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
            logging::line("ocr", &timer_wait_line(&module, parked.as_millis(), &place, &behind));
        }
    }
    let nonce = lua.registry_value::<Table>(&w.nonce);
    let _ = lua.remove_registry_value(w.nonce);
    let Ok(nonce) = nonce else {
        forget(h, id, false);
        return false;
    };
    let mut args = values;
    args.push_front(Value::Table(nonce));
    let _prio = enter_priority(prio);
    let delivery = enter_delivery(tasks, id);
    let outcome = stretch(tasks, id, &delivery, || thread.resume::<MultiValue>(args));
    step(h, id, &lua, &thread, &delivery, outcome);
    drop(delivery);
    true
}

/// Somebody waits on an event that queued behind module `owner`'s parked handler — it came in the
/// interactive lane (`mailbox.rs`): the read the handler waits for is made interactive
/// (`Service::promote`), and so is every read it waits for after it (`raised`). A read of a poll
/// would otherwise hold the key behind it as long as the polls' lane takes. A read being recognised
/// already runs on as it started. The handler's own lane stays: what it arms after the read — a
/// timer, a read with a callback — is in the lane it began in, so a poll stays a poll. True when a
/// handler that was parked in the background lane was raised.
pub(crate) fn promote<H: ReadHost>(h: &H, owner: Owner) -> bool {
    let ticket = {
        let mut table = h.tasks().table.borrow_mut();
        let Some(t) = table
            .values_mut()
            .find(|t| t.owner == owner && t.is_handler() && !t.running && t.prio == Priority::Background && !t.raised)
        else {
            return false;
        };
        t.raised = true;
        match t.wait.as_ref().map(|w| &w.on) {
            Some(On::Read(ticket)) => Some(*ticket),
            _ => None,
        }
    };
    if let Some(ticket) = ticket {
        h.ocr().promote(ticket);
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
/// of its stack that is Luau and not the shim's — past a `pcall` around `host.ocr.recognize` — as
/// `<module>/<file>:<line>`, or the chunk's name and the line for a chunk no module loaded. Asked of
/// Luau's own stack, so no module code can change the answer.
fn caller_place(lua: &Lua) -> Option<String> {
    (0..12).map_while(|level| crate::vm_guard::raw_frame(lua, level)).find_map(|f| {
        let source = f.source.clone().unwrap_or_default();
        if f.what == "C" || source == SHIM_NAME {
            return None;
        }
        Some(crate::vm_guard::frame_place(lua, &f))
    })
}

/// The shim's `start`: queues the read a wait asks for, for the innermost running handler of this
/// VM, which must be the one calling, in its own coroutine, with no wait in this stretch yet.
/// Raises for a mistake in the call. In a module that is not enabled, or for a handler cancelled
/// while it ran, nothing is queued: the wait ends the handler.
fn start<H: ReadHost>(h: &H, lua: &Lua, scope: usize, what: Value, opts: Value, nonce: Table) -> mlua::Result<()> {
    let me = owner(h, lua, scope);
    let calling = lua.current_thread().to_pointer() as usize;
    let tasks = h.tasks();
    let misplaced = || mlua::Error::external("host.ocr.recognize: the host's wait was called from outside its place");
    let first = in_place(tasks, me, calling).ok_or_else(misplaced)?;
    // A handler whose module went is not asked anything more: not even its arguments are read,
    // which could raise, and a `pcall` around the wait would then run on in a module that is gone.
    let gone = tasks.table.borrow().get(&first).is_some_and(|t| t.gone);
    let args = if gone { None } else { Some(reads::parse_read(&what, &opts)?) };
    let place = caller_place(lua);
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
        Some(args) if !ending && h.module_enabled(me.idx) => {
            let list = args.list;
            On::Read(reads::submit(h, lua, me.idx, args, prio, Waiter::Handler { id, list })?)
        }
        _ => On::Void,
    };
    let nonce = lua.create_registry_value(nonce)?;
    match tasks.table.borrow_mut().get_mut(&id) {
        Some(t) => {
            t.wait = Some(Wait { on, nonce });
            t.place = place;
        }
        None => {
            // Cannot happen: nothing ran between the check and here.
            let _ = lua.remove_registry_value(nonce);
        }
    }
    Ok(())
}

/// The shim's `disown`: module code resumed a parked handler's coroutine itself. The host forgets
/// it — without a reset, since that code is running it — and withdraws its read.
fn disown<H: ReadHost>(h: &H, lua: &Lua) {
    let calling = lua.current_thread().to_pointer() as usize;
    let id = h.tasks().table.borrow().iter().find(|(_, t)| t.ptr == calling && !t.running).map(|(id, _)| *id);
    let Some(id) = id else { return };
    let Some(t) = h.tasks().table.borrow_mut().remove(&id) else { return };
    if let Some(w) = t.wait {
        end_wait(h, &t.lua, w);
    }
}

/// A blocking call: `call` — a host function, by the name a module writes — where it could not
/// wait (`why`, the shim's word for it), run by `body` on the event loop inside a scope of the loop
/// guard's `kind`: the work that guard lets be there while it lasts. The module's first such call
/// of each call and case that answers is logged with how long it held the loop, and every call is
/// counted for the summary at exit. A call that raises — a mistake in its arguments, mostly —
/// writes no line, so the first that answers still does.
///
/// Nothing calls it yet: `host.ocr.recognize` raises where it cannot wait. The `host.screen` calls
/// that capture are to wait in a handler and to come here where they cannot (TODO.md, "The
/// synchronous `host.screen` captures").
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn legacy<H: ReadHost, T>(
    h: &H,
    lua: &Lua,
    scope: usize,
    call: &'static str,
    kind: crate::loop_guard::Kind,
    why: &str,
    body: impl FnOnce() -> mlua::Result<T>,
) -> mlua::Result<T> {
    let case = Case::of(why);
    let idx = owner(h, lua, scope).idx;
    let began = Instant::now();
    let out = {
        let _legacy = crate::loop_guard::legacy(kind);
        body()
    };
    let ms = began.elapsed().as_millis();
    let id = h.module_id(idx);
    let first = {
        let mut tally = h.tasks().legacy.borrow_mut();
        let held = tally.held.entry((id.clone(), call)).or_insert((0, 0));
        held.0 += 1;
        held.1 += ms;
        out.is_ok() && tally.said.insert((id.clone(), call, case))
    };
    if first {
        logging::line(library_of(call), &legacy_line(&id, call, case, ms));
    }
    out
}

/// `host.ocr.recognize` of VM `lua` — the table with `recognize` the shim returns — built on the
/// first call and the same table after it. `submit` is the callback form, `submit(what, opts, cb)`,
/// which opens the guard's host call itself, as the shim's `start` does here. Where the call
/// without one cannot wait, the shim raises [`wait_message`] for the place.
pub(crate) fn waits<H: ReadHost>(lua: &Lua, scope: usize, holder: Rc<H>, submit: Function) -> mlua::Result<Table> {
    if let Ok(t) = lua.named_registry_value::<Table>(WAITS_KEY) {
        return Ok(t);
    }
    let p = prims(lua)?;
    let prim: Table = lua.named_registry_value(PRIMS_KEY)?;
    let h = holder.clone();
    let start = lua.create_function(move |lua, (what, opts, nonce): (Value, Value, Table)| {
        // Named for the guard as the module wrote it: a stop in it says "in host.ocr.recognize".
        static NAME: &str = "host.ocr.recognize";
        let _host_call = crate::vm_guard::HostCall::enter(&NAME);
        start(&*h, lua, scope, what, opts, nonce)
    })?;
    let h = holder.clone();
    let where_ = lua.create_function(move |lua, ()| Ok(where_now(&*h, lua, scope)))?;
    let h = holder;
    let disown = lua.create_function(move |lua, ()| {
        disown(&*h, lua);
        Ok(())
    })?;
    // Named for the calls it carries: a raise in `recognize` shows its frames in the traceback of
    // a module's error, in the log and in the error dialog.
    let shim: Function = lua.load(SHIM).set_name(SHIM_NAME).eval()?;
    let t: Table = shim.call((
        start,
        where_,
        disown,
        submit,
        prim.get::<Function>("yield")?,
        prim.get::<Function>("isyieldable")?,
        prim.get::<Function>("rawequal")?,
        prim.get::<Function>("error")?,
        prim.get::<Function>("select")?,
        prim.get::<Function>("type")?,
        p.wait,
        RESUMED,
        NIL_CB,
        wait_message(Case::CannotWait),
        wait_message(Case::OwnCoroutine),
    ))?;
    lua.set_named_registry_value(WAITS_KEY, t.clone())?;
    Ok(t)
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
          local got, value = yield(WAIT, nonce)
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
            t.wait = Some(Wait { on, nonce });
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
    resume_parked(h, id, |on| matches!(on, On::Test(n) if n == name), MultiValue::from_vec(vec![value]))
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

    /// The summary at exit: per module and call, what the blocking calls held the event loop for
    /// where they could not wait ([`Tasks::legacy_summary`]).
    pub(crate) fn log_legacy_summary(&self) {
        for (scope, line) in self.tasks.legacy_summary() {
            logging::line(scope, &line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two messages name the call, the place and the callback form, which waits for nothing —
    /// and no way to start a task, which no module has. docs/api/ocr.md shows both, word for word.
    #[test]
    fn the_two_messages_name_the_place_and_the_callback_form() {
        const OCR_MD: &str = include_str!("../../../docs/api/ocr.md");
        for case in [Case::CannotWait, Case::OwnCoroutine] {
            let m = wait_message(case);
            assert!(m.starts_with("host.ocr.recognize cannot wait "), "{m}");
            assert!(m.contains("Pass a callback"), "{m}");
            assert!(!m.contains("task") && !m.contains("host.ocr.read"), "{m}");
            assert!(OCR_MD.contains(m), "docs/api/ocr.md does not show this message word for word:\n{m}");
        }
        assert!(OCR_MD.contains(NIL_CB), "docs/api/ocr.md does not show the nil callback's message:\n{NIL_CB}");
    }

    #[test]
    fn the_shims_words_are_read_as_the_two_cases() {
        assert_eq!(Case::of("none"), Case::CannotWait);
        assert_eq!(Case::of("cannot-wait"), Case::CannotWait);
        assert_eq!(Case::of("foreign"), Case::OwnCoroutine);
        assert_eq!(Case::of("anything else"), Case::CannotWait, "a word the shim never says");
    }

    /// The timer's line is the one docs/api/ocr.md shows, word for word.
    #[test]
    fn the_timers_line_is_the_documented_one() {
        const OCR_MD: &str = include_str!("../../../docs/api/ocr.md");
        let keys = ["Tab".to_string(), "Tab".to_string(), "Down".to_string()];
        let timer = timer_wait_line("com.example.game", 2140, "com.example.game/src/main.luau:88", &keys);
        assert!(OCR_MD.contains(&timer), "docs/api/ocr.md does not show this line word for word:\n{timer}");
    }

    /// A blocking call's line names the module, the call, the place it could not wait and the time
    /// it held the loop, in the words CI looks for (`could not wait here`), and points at its call's
    /// own page; the summary counts per module and call, with the call's library as its scope.
    /// Nothing about tasks, which no module has.
    #[test]
    fn the_line_names_the_call_where_and_how_long() {
        let l = legacy_line("com.example.game", "host.screen.pixel", Case::OwnCoroutine, 37);
        assert_eq!(
            l,
            "[com.example.game] host.screen.pixel could not wait here (a coroutine the module made) and held the \
             event loop 37 ms instead; call it from the event's own function; see \"Where it waits\" in \
             docs/api/screen.md"
        );
        assert!(legacy_line("m", "host.screen.save", Case::CannotWait, 1).contains(
            "host.screen.save could not wait here (a place Luau cannot suspend, or a function the host calls and waits for)"
        ));
        let summary = {
            let tasks = Tasks::default();
            tasks.legacy.borrow_mut().held.insert(("m".to_string(), "host.screen.pixel"), (2, 50));
            tasks.legacy.borrow_mut().held.insert(("m".to_string(), "host.screen.save"), (1, 9));
            tasks.legacy_summary()
        };
        assert_eq!(
            summary,
            [
                ("screen", "[m] host.screen.pixel held the event loop 2 time(s), 50 ms in all, where it could not wait".to_string()),
                ("screen", "[m] host.screen.save held the event loop 1 time(s), 9 ms in all, where it could not wait".to_string()),
            ]
        );
        for text in std::iter::once(&l).chain(summary.iter().map(|(_, s)| s)) {
            assert!(!text.contains("task") && !text.contains("host.ocr"), "{text}");
        }
        assert_eq!(library_of("host.ocr.recognize"), "ocr");
        assert_eq!(library_of("host.screen.pixel"), "screen");
        let keys = ["Tab".to_string(), "Tab".to_string(), "Down".to_string()];
        assert_eq!(
            timer_wait_line("m", 40, "m/src/main.luau:12", &keys),
            "[m] a timer waited 40 ms for a text read at m/src/main.luau:12; 3 key(s) (Tab \u{d7}2, Down) waited behind \
             it — a poll that reads should pass a callback: host.ocr.recognize(what, opts, function(reading) … end)"
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
