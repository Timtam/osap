//! One mailbox per module VM: the events that arrive while the module is busy, in the order they
//! came, run one after another as handlers (`task.rs`) once it is free.
//!
//! **Every event of a module comes through here** — a hotkey, a captured key, a controller event,
//! a timer, a window trigger's activation, the report of the window already in front, a focus
//! change, the answer to a read, an image search or a snapshot, a setting's `onChange` — and runs
//! as a handler of its VM. A module that has no handler and nothing queued is free, and its event
//! runs at once, as a callback always did. A module is busy while its handler waits, while its
//! mailbox is not empty, or while any module's handler is on the host's stack: no handler starts
//! inside another. A busy module's event waits here.
//!
//! **No handler waits in this build** (`task.rs`): in the application nothing but a setting
//! changed in the dialog, or by another module, ever waits here, and that for one tick. The rules
//! below are for the build in which handlers wait, and are run now by the tests, whose wait point
//! (`task::test_wait`) makes a module busy.
//!
//! **What waits, and what is folded or dropped.**
//! - A focus change is folded into one queued last: between two other events stands at most one,
//!   and every receiver asks the focus as it is then. A controller axis likewise replaces the last
//!   one queued for the same listener and axis.
//! - A held key's repeats are folded into one while the last of that key queued is a repeat: a hold
//!   keeps one step. Two deliberate presses never fold, and a second hold after a press of the key
//!   keeps a step of its own.
//! - Inputs — keys, hotkeys, controller buttons pressed — are dropped above [`INPUT_LIMIT`] queued,
//!   and the log says how many once the module is free. Answers, timers, window events, settings
//!   and a controller button's release are never dropped: a listener that heard a press always
//!   hears its release.
//! - A queued `every` timer is not queued at all: it is skipped and comes round again.
//! - An event somebody waits on — one that came in the interactive lane — makes the read the
//!   module's handler parked in the background lane waits for an interactive one, and every read
//!   that handler waits for after it, so a key does not wait behind a poll's read for as long as
//!   the polls' lane takes (`task::promote`). What the handler arms after it stays a poll's.
//!
//! **Delivered to what it was for.** An event carries its registration — the capture's token, the
//! hotkey's id, the listener's token, the timer's token, the read's ticket, the setting's
//! registration — and is delivered to that, as it is when it runs; a key or a hotkey whose
//! registration was made again meanwhile goes to the module's new registration of the same key,
//! never to another module's (`captures.rs`).
//!
//! **A wait point is answered only by a host thread or by state the host keeps**, never by an
//! event delivered to the waiting module, which waits here behind it (`task.rs`).
//!
//! Generic over [`MailHost`], as timers.rs and task.rs are over their holders, so the strict
//! scripted host and the dispatcher tests drive this queue and not a copy of its rules.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue};

use crate::backend::gamepad::{Axis, PadEvent};
use crate::backend::WinInfo;
use crate::gamepad_api::Delivery;
use crate::image_search::{Outcome, PendingImage};
use crate::logging;
use crate::ocr::lua::{owner_of, ReadAnswer, ReadHost};
use crate::ocr::sched::Owner;
use crate::ocr::types::{current_priority, enter_priority, Priority};
use crate::snapshot::{Answer, PendingSnap};
use crate::task::{self, Ctx, Ran};

/// How many inputs may wait in one module's mailbox; the next one is dropped.
pub(crate) const INPUT_LIMIT: usize = 256;

/// How long the queued phase of one tick may run: about a tick. Past it the phase ends after the
/// event that is running, and the rest runs on the next tick, in order — the module stays busy
/// meanwhile. Scheduling, so that a burst after a long wait does not hold the thread that also
/// carries the Mac's event tap; it decides nothing that is said.
pub(crate) const PHASE_BUDGET: Duration = Duration::from_millis(15);

/// A controller event's share of the rules: a button pressed or a combination is an input; a
/// button released is not, and never dropped — a listener that heard the press hears the release,
/// and the releases are bounded by the presses; an axis is folded per listener and axis; a
/// connection is none of those.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PadKind {
    Button,
    Release,
    /// The half of the stick, or the trigger, this delivery is about.
    Axis(Axis),
    Other,
}

/// What an event is, as Rust data: the Lua call is built when it runs (`MailHost::open`), as
/// `fire` built a read's table, so a queued event holds nothing in the VM but the registry keys
/// its payload already owned.
pub(crate) enum Event {
    /// Hotkey `id` of module `owner`, for the combination `binding` (`None`: a spec only the OS
    /// read), written `spec`; `front` the window in front when it arrived, noted only when the
    /// module was busy then — so `Some` exactly for a press that was queued. It decides whether the
    /// module's new registration of the same combination may take the press (`hotkey_target`).
    Hotkey { id: i32, owner: usize, front: Option<isize>, binding: Option<(u32, u8)>, spec: String },
    /// A captured key the hook took for module `owner` in window `front`, for the capture
    /// `token`; `repeat` is the keyboard's auto-repeat of a held key.
    Key { owner: usize, token: i64, vk: u32, mods: u8, repeat: bool, front: isize },
    Pad { token: i64, kind: PadKind, event: Box<PadEvent>, delivery: Delivery },
    /// The window that came to the front; `upto`, the newest trigger number when it arrived: a
    /// trigger made after that, by an earlier queued handler, does not get it.
    Activate { win: Box<WinInfo>, upto: i64 },
    /// The report of the window already in front (`onTrigger { initial = true }`).
    Initial { win: Option<Box<WinInfo>>, reprime: bool },
    Focus,
    /// A one-shot timer, still in the timer list and marked queued until it runs.
    After { token: i64 },
    /// A recurring timer: never queued — skipped while its module is busy.
    Every { token: i64 },
    Read(Box<ReadAnswer>),
    /// An image search's answer; `ended`, answered with a reason because a newer request of the
    /// module ended it, rather than searched.
    Image { p: Box<PendingImage>, outcome: Outcome, ended: bool },
    Snapshot { p: Box<PendingSnap>, answer: Answer },
    /// Registration `reg` of an `onChange` for module `setting_of`'s setting `key`.
    Setting { setting_of: usize, reg: u64, key: String, new: crate::settings::Value, old: Option<crate::settings::Value> },
    /// The tests' own event: `f(args)`, an input or not, reported as `what`.
    #[cfg(test)]
    Call { f: Function, args: MultiValue, input: bool, what: &'static str },
}

impl Event {
    /// Counted against [`INPUT_LIMIT`] and dropped above it: what a person pressed.
    fn is_input(&self) -> bool {
        match self {
            Event::Hotkey { .. } | Event::Key { .. } => true,
            Event::Pad { kind, .. } => *kind == PadKind::Button,
            #[cfg(test)]
            Event::Call { input, .. } => *input,
            _ => false,
        }
    }
}

/// What a queued event runs, resolved when it runs.
pub(crate) enum Opened {
    Run { f: Function, args: MultiValue, ctx: Ctx },
    /// Nothing to run: its registration is gone (said in the log by whoever looked), or there was
    /// nothing to call.
    Gone,
}

/// Why an event will not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    /// Its module was disabled. A setting's `onChange` is never discarded for it.
    Disabled,
    /// Its module was reloaded or removed.
    Reloaded,
    /// A hot-load was rolled back.
    RolledBack,
    /// [`INPUT_LIMIT`] inputs were already waiting.
    Limit,
    /// Its VM is not the one that runs at its module's index any more.
    Gone,
}

/// One event in a mailbox.
pub(crate) struct Queued {
    pub ev: Event,
    /// The priority it arrived with — `current_priority()` at `deliver` — which it runs under.
    pub prio: Priority,
}

struct Mailbox {
    /// Strong, as a pending read holds its VM; dropped with the box, which goes when it empties.
    lua: Lua,
    gen: u64,
    queue: VecDeque<Queued>,
    /// Queued inputs, against [`INPUT_LIMIT`].
    inputs: usize,
    /// Inputs dropped at the limit in this busy stretch, and what the module waited for, and for
    /// how long, when the first was: said once it is free.
    over: u32,
    over_why: Option<(String, u128)>,
}

/// Every module's mailbox. Main thread only, like the rest of `Shared`.
#[derive(Default)]
pub(crate) struct Mailboxes {
    /// By module index, so a phase walks the modules in index order, as every dispatch loop does.
    boxes: RefCell<BTreeMap<usize, Mailbox>>,
    /// The module the next queued phase begins with: the one after the module the last phase's
    /// budget ended at.
    resume_at: Cell<Option<usize>>,
    /// When the `[pump]` line for a phase the budget ended was last written, and how many phases
    /// it ended since without one ([`Mailboxes::say_budget`]).
    budget_said: Cell<Option<Instant>>,
    budget_unsaid: Cell<u32>,
}

/// How often the `[pump]` line for a queued phase the budget ended is written at most: a burst
/// that keeps the phases full for a while is one fact, not a line per tick.
pub(crate) const BUDGET_QUIET: Duration = Duration::from_secs(10);

impl Mailboxes {
    /// A queued phase's budget ended it at `now`: whether that is said — the first time in
    /// [`BUDGET_QUIET`] — with how many such phases went unsaid before it.
    pub(crate) fn say_budget(&self, now: Instant) -> Option<u32> {
        match self.budget_said.get() {
            Some(at) if now.saturating_duration_since(at) < BUDGET_QUIET => {
                self.budget_unsaid.set(self.budget_unsaid.get() + 1);
                None
            }
            _ => {
                self.budget_said.set(Some(now));
                Some(self.budget_unsaid.replace(0))
            }
        }
    }

    /// No event waits anywhere: a reason less for the headless loop to keep running.
    pub(crate) fn is_empty(&self) -> bool {
        self.boxes.borrow().is_empty()
    }

    /// How many events wait for module `idx`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn len(&self, idx: usize) -> usize {
        self.boxes.borrow().get(&idx).map_or(0, |b| b.queue.len())
    }
}

/// What the read path, the handlers and the queue need of whatever holds them: the host's
/// `Shared`, or a test's holder.
pub(crate) trait MailHost: ReadHost {
    fn mail(&self) -> &Mailboxes;
    /// Looks the event's registration up now and builds its call, or says there is nothing to
    /// run — releasing what the event owned either way.
    fn open(&self, idx: usize, lua: &Lua, ev: Event) -> Opened;
    /// An event that will not run: a disabled, reloaded or rolled-back module, the input limit.
    /// Releases what it owns, as each site does for a callback it does not call.
    fn discard(&self, idx: usize, ev: Event, why: Why);
}

/// What became of a delivered event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivered {
    /// It ran, and its handler ended.
    Ran,
    /// It ran, and its handler waits: its module is busy now.
    Parked,
    /// Its module is busy: it waits in the mailbox.
    Queued,
    /// Folded into one already queued, or dropped — its module off, or the input limit.
    Dropped,
}

/// Whether module `idx` is busy: its handler is on the stack or parked, or its mailbox is not
/// empty, or another module's handler is on the stack.
pub(crate) fn busy<H: MailHost>(h: &H, idx: usize) -> bool {
    let tasks = h.tasks();
    tasks.handler_of(idx).is_some() || h.mail().boxes.borrow().contains_key(&idx) || tasks.any_handler_running()
}

/// Whether an event for `owner` may run: its VM is still the one at its index, and its module is
/// enabled — but a setting's `onChange` runs for a disabled module too, as it always did.
fn deliverable<H: MailHost>(h: &H, owner: Owner, ev: &Event) -> Result<(), Why> {
    if h.vm_gens().get(&owner.idx) != Some(&owner.gen) {
        return Err(Why::Gone);
    }
    if matches!(ev, Event::Setting { .. }) || h.module_enabled(owner.idx) {
        Ok(())
    } else {
        Err(Why::Disabled)
    }
}

/// Delivers `ev` to the module whose VM is `lua` (`idx` its index, unless the VM says otherwise):
/// runs it now as a handler if the module is free, queues it if it is busy, drops it if the module
/// may not hear it. Every event site calls this in place of calling its callback.
pub(crate) fn deliver<H: MailHost>(h: &H, idx: usize, lua: &Lua, ev: Event) -> Delivered {
    let owner = owner_of(lua, &h.vm_gens(), idx);
    if let Err(why) = deliverable(h, owner, &ev) {
        h.discard(owner.idx, ev, why);
        return Delivered::Dropped;
    }
    if busy(h, owner.idx) {
        return enqueue(h, owner, lua, ev);
    }
    match run(h, owner.idx, lua, ev, current_priority()) {
        Ran::Ended => Delivered::Ran,
        Ran::Parked(_) => Delivered::Parked,
    }
}

/// [`deliver`], but always through the mailbox, to run in the queued phase of this tick or the
/// next: a setting changed in the dialog, or by another module's `host.settings.set` — never run
/// inside the code that changed it.
pub(crate) fn deliver_later<H: MailHost>(h: &H, idx: usize, lua: &Lua, ev: Event) -> Delivered {
    let owner = owner_of(lua, &h.vm_gens(), idx);
    if let Err(why) = deliverable(h, owner, &ev) {
        h.discard(owner.idx, ev, why);
        return Delivered::Dropped;
    }
    enqueue(h, owner, lua, ev)
}

/// Queues `ev` for the busy module `owner`, folding and limiting as the module docs say.
fn enqueue<H: MailHost>(h: &H, owner: Owner, lua: &Lua, mut ev: Event) -> Delivered {
    // Never queued: comes round again.
    if matches!(ev, Event::Every { .. }) {
        return Delivered::Dropped;
    }
    // Somebody waits on this event — it came in the interactive lane: a key, a hotkey, a controller
    // event, a window or focus change, or a timer or an answer one of those asked for. The read a
    // handler of the module parked in the background lane waits for — a poll's — is made an
    // interactive one, and so is every read it waits for after it, so the event does not wait as
    // long as the polls' lane takes (`task::promote`). Before folding and the limit: what folds
    // into an earlier event or is dropped was pressed all the same. A callback-form read of the
    // module holds nothing and is not raised.
    if current_priority() == Priority::Interactive {
        task::promote(h, owner);
    }
    // Queued in the same borrow that counted it: nothing runs between the count — a read noted as
    // in the mailbox, an input against the limit — and the push, so no event is counted that is
    // not there. What is let go — a stale VM's events, one over the limit — is let go after it.
    let prio = current_priority();
    let (stale, over) = {
        let mut boxes = h.mail().boxes.borrow_mut();
        let fresh = || Mailbox {
            lua: lua.clone(),
            gen: owner.gen,
            queue: VecDeque::new(),
            inputs: 0,
            over: 0,
            over_why: None,
        };
        let b = boxes.entry(owner.idx).or_insert_with(fresh);
        // A VM that went without its mailbox going with it: what it held is let go.
        let stale: Vec<Queued> = if b.gen != owner.gen { std::mem::replace(b, fresh()).queue.into() } else { Vec::new() };
        if fold(&mut b.queue, &mut ev) {
            drop(boxes);
            discard_all(h, owner.idx, stale, Why::Gone);
            return Delivered::Dropped;
        }
        if ev.is_input() && b.inputs >= INPUT_LIMIT {
            b.over += 1;
            if b.over_why.is_none() {
                b.over_why = Some(h.tasks().parked_of(owner.idx).unwrap_or_else(|| ("its earlier events".to_string(), 0)));
            }
            (stale, Some(ev))
        } else {
            if ev.is_input() {
                b.inputs += 1;
            }
            if let Event::Read(r) = &ev {
                crate::ocr::lua::note_queued(h.ocr_state(), r);
            }
            b.queue.push_back(Queued { ev, prio });
            (stale, None)
        }
    };
    discard_all(h, owner.idx, stale, Why::Gone);
    if let Some(ev) = over {
        h.discard(owner.idx, ev, Why::Limit);
        return Delivered::Dropped;
    }
    Delivered::Queued
}

/// Folds `ev` into what is queued, if it folds: a focus change after a focus change queued last;
/// an axis after the same listener's same axis queued last, whose value it replaces; a held key's
/// repeat while the last of the same key queued is a repeat too — of this hold: a press of the key
/// queued after a repeat starts another hold, whose repeats keep one step of their own.
fn fold(queue: &mut VecDeque<Queued>, ev: &mut Event) -> bool {
    match ev {
        Event::Focus => matches!(queue.back(), Some(Queued { ev: Event::Focus, .. })),
        Event::Pad { token, kind: PadKind::Axis(axis), event, delivery } => {
            match queue.back_mut() {
                Some(Queued { ev: Event::Pad { token: t, kind: PadKind::Axis(a), event: e, delivery: d }, .. })
                    if *t == *token && *a == *axis =>
                {
                    std::mem::swap(e, event);
                    *d = *delivery;
                    true
                }
                _ => false,
            }
        }
        Event::Key { owner, vk, mods, repeat: true, .. } => queue
            .iter()
            .rev()
            .find_map(|q| match &q.ev {
                Event::Key { owner: o, vk: v, mods: m, repeat, .. } if o == owner && v == vk && m == mods => Some(*repeat),
                _ => None,
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn discard_all<H: MailHost>(h: &H, idx: usize, queued: Vec<Queued>, why: Why) {
    for q in queued {
        h.discard(idx, q.ev, why);
    }
}

/// Opens `ev` and runs it as module `idx`'s handler, under `prio`: the priority it arrived with.
fn run<H: MailHost>(h: &H, idx: usize, lua: &Lua, ev: Event, prio: Priority) -> Ran {
    let _prio = enter_priority(prio);
    match h.open(idx, lua, ev) {
        Opened::Run { f, args, ctx } => task::run_handler(h, idx, lua, f, args, prio, ctx),
        Opened::Gone => Ran::Ended,
    }
}

/// What a queued phase did, for the `[pump]` line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Phase {
    /// Events taken out of mailboxes and run (or found gone).
    pub events: u32,
    /// Handlers that waited.
    pub parked: u32,
    /// The budget ended the phase before every planned event ran.
    pub budget: bool,
}

/// The tick's phase for queued events: every free module with events gets them, oldest first,
/// until its handler waits or the events it had when the phase began are done — an event queued
/// during the phase waits for the next tick, so two modules whose `onChange` set each other's
/// settings cost one turn each per tick, not an endless loop. The epoch turns once before the
/// first event, as a delivery drain turns it. Within [`PHASE_BUDGET`], each module getting at
/// least one event unless the budget was spent before its turn; the next phase begins with the
/// module after the one this one ended at. Nothing runs while a handler is on the stack.
pub(crate) fn run_queued<H: MailHost>(h: &H) -> Phase {
    let mut phase = Phase::default();
    let tasks = h.tasks();
    if tasks.any_handler_running() {
        return phase;
    }
    let mut plan: Vec<(usize, usize)> = h
        .mail()
        .boxes
        .borrow()
        .iter()
        .filter(|(idx, b)| !b.queue.is_empty() && tasks.handler_of(**idx).is_none())
        .map(|(idx, b)| (*idx, b.queue.len()))
        .collect();
    if plan.is_empty() {
        return phase;
    }
    if let Some(start) = h.mail().resume_at.take() {
        let first = plan.iter().position(|(idx, _)| *idx >= start).unwrap_or(0);
        plan.rotate_left(first);
    }
    let began = Instant::now();
    let mut bumped = false;
    'modules: for (n, &(idx, count)) in plan.iter().enumerate() {
        if n > 0 && began.elapsed() >= PHASE_BUDGET {
            h.mail().resume_at.set(Some(idx));
            phase.budget = true;
            break;
        }
        for _ in 0..count {
            let popped = {
                let mut boxes = h.mail().boxes.borrow_mut();
                let Some(b) = boxes.get_mut(&idx) else { break };
                let Some(q) = b.queue.pop_front() else { break };
                if q.ev.is_input() {
                    b.inputs = b.inputs.saturating_sub(1);
                }
                (q, b.lua.clone(), b.gen)
            };
            let (q, lua, gen) = popped;
            if !bumped {
                h.bump_epoch();
                bumped = true;
            }
            phase.events += 1;
            let parked = match deliverable(h, Owner { idx, gen }, &q.ev) {
                Err(why) => {
                    h.discard(idx, q.ev, why);
                    false
                }
                Ok(()) => matches!(run(h, idx, &lua, q.ev, q.prio), Ran::Parked(_)),
            };
            settle_box(h, idx);
            if parked {
                phase.parked += 1;
                break;
            }
            if began.elapsed() >= PHASE_BUDGET {
                let next = plan.get(n + 1).map_or(idx + 1, |(i, _)| *i);
                h.mail().resume_at.set(Some(next));
                phase.budget = true;
                break 'modules;
            }
        }
    }
    phase
}

/// A mailbox that emptied goes, with the VM it held; the inputs it dropped at the limit in this
/// busy stretch are said now.
fn settle_box<H: MailHost>(h: &H, idx: usize) {
    let gone = {
        let mut boxes = h.mail().boxes.borrow_mut();
        match boxes.get(&idx) {
            Some(b) if b.queue.is_empty() => boxes.remove(&idx),
            _ => None,
        }
    };
    if let Some(Mailbox { over, over_why: Some((what, ms)), .. }) = gone {
        if over > 0 {
            logging::line("keys", &limit_line(&h.module_id(idx), over, &what, ms));
        }
    }
}

/// The `[pump]` line for a queued phase the budget ended after `events` ran; `unsaid`, the phases
/// it ended since the last such line ([`Mailboxes::say_budget`]).
pub(crate) fn budget_line(events: u32, unsaid: u32) -> String {
    let since = if unsaid > 0 { format!("; it ended {unsaid} more such phase(s) since the last of these lines") } else { String::new() };
    format!(
        "the queued module events used up their {} ms this tick: {events} ran, and the rest run on the next tick, \
         in order{since}. Said at most once every {} s",
        PHASE_BUDGET.as_millis(),
        BUDGET_QUIET.as_secs()
    )
}

/// The line for inputs a busy module's mailbox dropped at the limit.
pub(crate) fn limit_line(module: &str, n: u32, what: &str, ms: u128) -> String {
    format!(
        "[{module}] {n} key(s) or button(s) pressed while it was busy were dropped: {INPUT_LIMIT} were already \
         waiting (the module waited for {what}, {ms} ms by the first of them)"
    )
}

/// Takes module `idx`'s mailbox away — disabled, reloaded, removed: every event in it is
/// discarded with `why`, but a disable keeps a setting's `onChange`, which a disabled module still
/// hears. Beside `task_drop_owner`, wherever that is called.
pub(crate) fn drop_owner<H: MailHost>(h: &H, idx: usize, why: Why) {
    let taken: Vec<Queued> = {
        let mut boxes = h.mail().boxes.borrow_mut();
        match boxes.get_mut(&idx) {
            None => Vec::new(),
            Some(b) if why == Why::Disabled => {
                let (keep, take): (VecDeque<Queued>, VecDeque<Queued>) =
                    b.queue.drain(..).partition(|q| matches!(q.ev, Event::Setting { .. }));
                b.queue = keep;
                b.inputs = 0;
                if b.queue.is_empty() {
                    boxes.remove(&idx);
                }
                take.into_iter().collect()
            }
            Some(_) => boxes.remove(&idx).map(|b| b.queue.into_iter().collect()).unwrap_or_default(),
        }
    };
    discard_all(h, idx, taken, why);
}

/// `rollback_to(n)`'s share: the mailbox of every module from index `n` on.
pub(crate) fn drop_from<H: MailHost>(h: &H, n: usize) {
    let idxs: Vec<usize> = h.mail().boxes.borrow().keys().copied().filter(|i| *i >= n).collect();
    for idx in idxs {
        drop_owner(h, idx, Why::RolledBack);
    }
}

/// The tests' own event, opened: `f(args)`, reported as `what` under module `idx`.
#[cfg(test)]
pub(crate) fn open_call(idx: usize, f: Function, args: MultiValue, what: &'static str) -> Opened {
    Opened::Run { f, args, ctx: Ctx::new(what, idx) }
}

/// What one event costs as a handler, measured: `n` events of an idle module through [`deliver`],
/// against `n` plain calls of the same function — what every callback cost before. Microseconds
/// per event, both ways; for the line the tests print and module-runtime-and-lifecycle.md quotes.
#[cfg(test)]
pub(crate) fn cost_per_event<H: MailHost>(h: &H, idx: usize, lua: &Lua, n: u32) -> (f64, f64) {
    let f: Function = lua.load("local n = 0; return function() n = n + 1 end").eval().expect("the counter");
    let t = Instant::now();
    for _ in 0..n {
        let _ = f.call::<()>(());
    }
    let plain = t.elapsed().as_secs_f64() * 1e6 / f64::from(n);
    let t = Instant::now();
    for _ in 0..n {
        deliver(h, idx, lua, Event::Call { f: f.clone(), args: MultiValue::new(), input: true, what: "key" });
    }
    let handler = t.elapsed().as_secs_f64() * 1e6 / f64::from(n);
    (handler, plain)
}
