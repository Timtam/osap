//! What the host does with modules the guard stopped (`vm_guard.rs`): turns them off until the
//! next start, gives their keys back, lets go of the mouse buttons they hold, drops the keys pressed
//! while the stalled callback held the application, and says so — the log line, the error dialog,
//! a spoken sentence and the note in each module's row of the module manager. One way for both
//! limits: a callback that ran past its time (G2), and one that ran its VM out of memory (G1).
//!
//! **Two steps.** [`mark`] runs when the outermost entry of a slice ends — the callback that began
//! it has returned — and runs no Lua: it flips each stopped module's enabled flag, so everything
//! that reads it skips the module at once, for the rest of the same dispatch too; hands the keyboard
//! hook and the OS their keys and hotkeys back; releases the mouse buttons they hold; drops the keys
//! and hotkey presses queued during the stall; and writes the log line, so the log has it whatever
//! comes after. [`settle`] runs at the end of the pump's turn: the rest of what disabling does (the
//! arbiter's re-election, the modules' reads, mailboxes and handlers), then the dialog, the
//! sentence and the rows.
//!
//! **Whom a stop takes.** The module whose code was running: the VM's own, or — when the loop was in
//! a library's code — the library and every module whose VM runs its code, in one step
//! (`vm_guard::claim`). A memory stop takes only the VM's own module. A module that was off already
//! gets a log line and nothing else.
//!
//! **Until the next start.** The settings file keeps a stopped module on ([`stored_enabled`]), so
//! the next start loads it; ticking it in the manager turns it on again at once, built afresh as a
//! reload builds it — a stop ends a callback anywhere, and what the module was in the middle of is
//! not picked up again — and ticking the library of a library's stop turns on every module the stop
//! took with it ([`Stops::turned_on_with`]). A reload keeps it off.
//!
//! Generic over [`StopHost`], so the rules are tested without a whole `Shared` (`stops_tests.rs`).

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::vm_guard::{Cause, Clock, EntryKind, Location, Outer, Trip, VmInfo, CPU_BUDGET, MIB, WALL_BUDGET};

/// A module turned off by a stop, by its index, id and manifest name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Member {
    pub idx: usize,
    pub id: String,
    pub name: String,
}

/// What a stop dropped of the input queued for the event loop while the callback held it: how many
/// captured keys and hotkey presses, and their names, each once with a count ("Tab ×3").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Dropped {
    pub keys: usize,
    pub hotkeys: usize,
    pub names: Vec<String>,
}

impl Dropped {
    pub(crate) fn is_empty(&self) -> bool {
        self.keys == 0 && self.hotkeys == 0
    }
}

/// `names` once each, in the order first met, with a count where one came more than once:
/// `["Tab", "Tab", "Space"]` → `["Tab ×2", "Space"]`.
pub(crate) fn counted(names: &[String]) -> Vec<String> {
    let mut order: Vec<(&str, usize)> = Vec::new();
    for n in names {
        match order.iter_mut().find(|(o, _)| *o == n.as_str()) {
            Some((_, c)) => *c += 1,
            None => order.push((n, 1)),
        }
    }
    order.into_iter().map(|(n, c)| if c > 1 { format!("{n} \u{d7}{c}") } else { n.to_string() }).collect()
}

/// One stop: the trip the guard noted, who it turned off, and what it let go.
#[derive(Debug)]
pub(crate) struct StopEvent {
    pub seq: u64,
    pub at: Instant,
    pub trip: Trip,
    /// Turned off: the module whose code ran first, then the others by name. A memory stop has one.
    pub members: Vec<Member>,
    /// The mouse buttons let go of, as (the module that held it, the button: "left", "right",
    /// "middle").
    pub released: Vec<(String, &'static str)>,
    pub dropped: Dropped,
}

/// The stops of this run: each stopped module's record, the stops not settled yet, and the flag
/// flips that had to wait. Main thread only, like the rest of `Shared`.
#[derive(Default)]
pub(crate) struct Stops {
    records: RefCell<BTreeMap<usize, Rc<StopEvent>>>,
    unsettled: RefCell<VecDeque<Rc<StopEvent>>>,
    deferred: RefCell<Vec<usize>>,
    seq: Cell<u64>,
}

impl Stops {
    /// Whether module `idx` is off because it was stopped, and not turned on again.
    pub(crate) fn is_stopped(&self, idx: usize) -> bool {
        self.records.borrow().contains_key(&idx)
    }

    /// How many modules are off because they were stopped.
    pub(crate) fn count(&self) -> usize {
        self.records.borrow().len()
    }

    /// The stop that turned module `idx` off.
    pub(crate) fn record(&self, idx: usize) -> Option<Rc<StopEvent>> {
        self.records.borrow().get(&idx).cloned()
    }

    /// Module `idx` was turned on again: its record goes. True when it had one.
    pub(crate) fn clear(&self, idx: usize) -> bool {
        self.deferred.borrow_mut().retain(|i| *i != idx);
        self.records.borrow_mut().remove(&idx).is_some()
    }

    /// A rolled-back hot-load: the records of every module from index `n` on go with them.
    pub(crate) fn drop_from(&self, n: usize) {
        self.records.borrow_mut().retain(|idx, _| *idx < n);
        self.deferred.borrow_mut().retain(|idx| *idx < n);
        self.unsettled.borrow_mut().retain(|ev| ev.members.iter().all(|m| m.idx < n));
    }

    /// The note the manager's row for module `idx` carries while it is stopped.
    pub(crate) fn row_note(&self, idx: usize) -> Option<String> {
        self.record(idx).map(|ev| row_note(&ev, idx))
    }

    /// The modules that ticking module `idx` turns on again: itself, and — when it is the library
    /// of a stop that took the modules that run its code — every one of them still off for that
    /// stop. Each is built afresh by the caller.
    pub(crate) fn turned_on_with(&self, idx: usize) -> Vec<usize> {
        let Some(ev) = self.record(idx) else { return vec![idx] };
        if library_member(&ev).map(|m| m.idx) != Some(idx) {
            return vec![idx];
        }
        let records = self.records.borrow();
        let mut out = vec![idx];
        out.extend(
            ev.members
                .iter()
                .map(|m| m.idx)
                .filter(|i| *i != idx && records.get(i).is_some_and(|r| r.seq == ev.seq)),
        );
        out
    }

    /// Whether a stop waits for [`settle`].
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn has_unsettled(&self) -> bool {
        !self.unsettled.borrow().is_empty()
    }
}

/// What the settings file keeps for a module: what the user wants. A stopped module was on when it
/// was stopped, so the next start loads it.
pub(crate) fn stored_enabled(enabled: bool, stopped: bool) -> bool {
    enabled || stopped
}

/// What the two steps need of whatever holds the modules: the host's `Shared`, or a test's.
pub(crate) trait StopHost {
    fn stops(&self) -> &Stops;
    fn module_enabled(&self, idx: usize) -> bool;
    /// Turns module `idx`'s enabled flag off and nothing else; false when the flag cannot be
    /// written now (borrowed further up the stack), and the flip waits for [`settle`].
    fn flip_off(&self, idx: usize) -> bool;
    /// Hands the hook and the OS the captured keys, hotkeys and controller demand of the modules
    /// that are on now. Runs no Lua.
    fn refresh_keys(&self);
    /// Lets go of the mouse buttons module `idx` holds down with `host.input.mouseDown`, where the
    /// pointer is now: the buttons' names ("left").
    fn release_buttons(&self, idx: usize) -> Vec<&'static str>;
    /// Drops the captured keys and hotkey presses queued for the event loop: pressed while the
    /// stopped callback held it.
    fn drop_queued_input(&self) -> Dropped;
    /// The rest of disabling module `idx`, as the manager's checkbox does it: the arbiter's
    /// re-election, its reads, snapshots, mailbox, handlers, key scope and menu flag.
    fn after_disable(&self, idx: usize);
    /// One full collection of the VM with slot `serial`.
    fn collect_garbage(&self, serial: u64);
    /// A line of the log, in `category` (`guard`, `keys`).
    fn log(&self, category: &str, line: &str);
    /// The accessible error dialog.
    fn dialog(&self, key: String, title: String, text: String);
    /// The spoken sentence.
    fn announce(&self, text: &str);
    /// Module `idx`'s row in the manager: off, with this note — or on again, without one.
    fn row_changed(&self, idx: usize, note: Option<String>);
}

/// The first step, when the outermost entry of a slice ends: each trip's modules are turned off,
/// their keys go back, their mouse buttons are let go of, the input queued during the stall is
/// dropped, and the log says why. Runs no Lua. A trip in a VM that was loading stops nobody — the
/// load fails with its own message — and a module that was off already gets a log line and nothing
/// else.
pub(crate) fn mark<H: StopHost + ?Sized>(h: &H, trips: Vec<Trip>) {
    let mut taken: Vec<(Trip, Vec<Member>)> = Vec::new();
    for trip in trips {
        if trip.loading {
            continue;
        }
        let mut on: Vec<Member> = Vec::new();
        let mut off: Vec<String> = Vec::new();
        for g in &trip.group {
            let Some(idx) = g.module else { continue };
            if g.loading || on.iter().any(|m| m.idx == idx) {
                continue;
            }
            // Another trip of the same slice took it already.
            if taken.iter().any(|(_, ms)| ms.iter().any(|m| m.idx == idx)) {
                continue;
            }
            if !h.module_enabled(idx) {
                off.push(g.id.clone());
                continue;
            }
            on.push(Member { idx, id: g.id.clone(), name: g.name.clone() });
        }
        // Off already: a line, and nothing else. Its VM stays stopped until it is turned on,
        // which clears it (`vm_guard::clear_module`).
        for id in &off {
            if *id == trip.vm.id {
                h.log("guard", &already_off_line(&trip));
            } else {
                h.log("guard", &off_member_line(id, &trip));
            }
        }
        if on.is_empty() {
            continue;
        }
        for m in &on {
            if !h.flip_off(m.idx) {
                h.stops().deferred.borrow_mut().push(m.idx);
            }
        }
        let members = ordered(on, &trip);
        taken.push((trip, members));
    }
    if taken.is_empty() {
        return;
    }
    // What is handed back to the OS, each step on its own: a panic in one is logged and the rest
    // goes on, so the records below are made whatever happens — a module turned off with none
    // would stay off unexplained, and be stored off.
    let os = |what: &str, f: &mut dyn FnMut()| {
        if let Err(p) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            h.log("guard", &format!("the stop could not {what}: {}", crate::panic_text(&p)));
        }
    };
    os("hand the keys back", &mut || h.refresh_keys());
    let mut dropped = Dropped::default();
    os("drop the input queued during the stall", &mut || dropped = h.drop_queued_input());
    for (trip, members) in taken {
        let mut released = Vec::new();
        for m in &members {
            os("let go of a mouse button", &mut || {
                for b in h.release_buttons(m.idx) {
                    released.push((m.name.clone(), b));
                }
            });
        }
        let seq = h.stops().seq.get() + 1;
        h.stops().seq.set(seq);
        // One stall, one set of dropped keys: the first stop of the slice carries them.
        let ev = Rc::new(StopEvent { seq, at: Instant::now(), trip, members, released, dropped: std::mem::take(&mut dropped) });
        for m in &ev.members {
            h.stops().records.borrow_mut().insert(m.idx, ev.clone());
        }
        h.log("guard", &log_line(&ev));
        if !ev.dropped.is_empty() {
            h.log("keys", &dropped_line(&ev));
        }
        h.stops().unsettled.borrow_mut().push_back(ev);
    }
}

/// The module whose code ran first — the library, when it is one of them — then the others by name.
fn ordered(mut on: Vec<Member>, trip: &Trip) -> Vec<Member> {
    let lib = library_id(trip);
    on.sort_by(|a, b| (a.id != lib).cmp(&(b.id != lib)).then_with(|| a.name.cmp(&b.name)));
    on
}

/// The second step, at the end of the pump's turn: the rest of disabling each stopped module, a
/// collection of the VM's garbage after a memory stop, then the dialog, the sentence and the
/// manager's rows. A stop whose modules were all turned on again in between is only dropped.
pub(crate) fn settle<H: StopHost + ?Sized>(h: &H) {
    loop {
        let Some(ev) = h.stops().unsettled.borrow_mut().pop_front() else { break };
        let current = |idx: usize| h.stops().records.borrow().get(&idx).is_some_and(|r| r.seq == ev.seq);
        let members: Vec<&Member> = ev.members.iter().filter(|m| current(m.idx)).collect();
        if members.is_empty() {
            continue;
        }
        for m in &members {
            let waited = h.stops().deferred.borrow().contains(&m.idx);
            if waited {
                if !h.flip_off(m.idx) {
                    h.log("guard", &format!("[{}] its enabled flag could not be written even at the end of the turn", m.id));
                }
                h.stops().deferred.borrow_mut().retain(|i| *i != m.idx);
            }
            h.after_disable(m.idx);
        }
        if let Cause::Memory { .. } = ev.trip.cause {
            h.collect_garbage(ev.trip.serial);
        }
        h.dialog(format!("{}\u{1}stop\u{1}{}", ev.trip.vm.id, ev.seq), dialog_title(&ev), dialog_text(&ev));
        h.announce(&sentence(&ev));
        for m in &members {
            h.row_changed(m.idx, Some(row_note(&ev, m.idx)));
        }
    }
}

// --- The words -------------------------------------------------------------------------------

/// `bytes` in MiB, to one decimal.
fn mib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / MIB as f64)
}

/// Seconds to one decimal.
fn secs(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64())
}

/// "A", "A and B", "A, B and C".
fn listed(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The stopped call, as the module's own: "its hotkey callback", "its entry file".
fn own_call(kind: EntryKind, what: &str) -> String {
    match kind {
        EntryKind::Load => what.to_string(),
        EntryKind::Handler | EntryKind::Plain => format!("its {what} callback"),
    }
}

/// ", run during Guard probe's hotkey callback," — the call the stopped one ran inside, or nothing.
fn nested(outer: Option<&Outer>) -> String {
    match outer {
        None => String::new(),
        Some(o) if o.kind == EntryKind::Load && o.same_vm => format!(", run during {},", o.what),
        Some(o) if o.kind == EntryKind::Load => format!(", run while {} was loading,", o.name),
        Some(o) if o.same_vm => format!(", run during its own {} callback,", o.what),
        Some(o) => format!(", run during {}'s {} callback,", o.name, o.what),
    }
}

/// "its hotkey callback", or "its arbiter onDeactivate callback, run during Guard probe's hotkey
/// callback,".
fn subject(trip: &Trip) -> String {
    format!("{}{}", own_call(trip.kind, &trip.what), nested(trip.outer.as_ref()))
}

/// The call that ran, as another module names it: "Guard probe's hotkey callback", "Guard probe B
/// while loading, in its entry file", with what it ran inside.
fn call_of(trip: &Trip) -> String {
    let call = match trip.kind {
        EntryKind::Load => format!("{} while loading, in {}", trip.vm.name, trip.what),
        EntryKind::Handler | EntryKind::Plain => format!("{}'s {} callback", trip.vm.name, trip.what),
    };
    format!("{call}{}", nested(trip.outer.as_ref()).trim_end_matches(','))
}

/// The module whose code the stop was about: the library's, or the VM's own.
fn library_id(trip: &Trip) -> String {
    trip.culprit.as_ref().map_or_else(|| trip.vm.id.clone(), |(id, _)| id.clone())
}

fn library_name(trip: &Trip) -> String {
    trip.culprit.as_ref().map_or_else(|| trip.vm.name.clone(), |(_, name)| name.clone())
}

/// Whether the stop is told as a library's: its code ran in another module's VM, or it took more
/// than one module.
fn as_library(ev: &StopEvent) -> bool {
    matches!(ev.trip.cause, Cause::Time { .. }) && (ev.trip.culprit.is_some() || ev.members.len() > 1)
}

/// The library among the modules a library's stop turned off, if it was on.
fn library_member(ev: &StopEvent) -> Option<&Member> {
    if !as_library(ev) {
        return None;
    }
    let lib = library_id(&ev.trip);
    ev.members.iter().find(|m| m.id == lib)
}

/// How much the first words say: all of it, or — when the sentence would not fit where it is
/// shown — the group counted, "ran too long", and no outer callback.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Full,
    Short,
}

/// Whom the stop turned off, for the first words: "Guard probe", or "Guard probe library and the 2
/// modules that run its code, Guard probe B and Guard probe C" — in the short form "Guard probe
/// library and 2 modules that run its code".
fn who(ev: &StopEvent, form: Form) -> String {
    if !as_library(ev) {
        return ev.trip.vm.name.clone();
    }
    let lib = library_id(&ev.trip);
    let others: Vec<String> = ev.members.iter().filter(|m| m.id != lib).map(|m| m.name.clone()).collect();
    let lib_on = ev.members.iter().any(|m| m.id == lib);
    let name = library_name(&ev.trip);
    match (lib_on, others.len(), form) {
        (true, 0, _) => name,
        (true, 1, Form::Full) => format!("{name} and the module that runs its code, {}", others[0]),
        (true, n, Form::Full) => format!("{name} and the {n} modules that run its code, {}", listed(&others)),
        (true, 1, Form::Short) => format!("{name} and 1 module that runs its code"),
        (true, n, Form::Short) => format!("{name} and {n} modules that run its code"),
        (false, 1, _) => format!("{}, which runs {name}'s code", others[0]),
        (false, _, Form::Full) => format!("{}, which run {name}'s code", listed(&others)),
        (false, n, Form::Short) => format!("{n} modules that run {name}'s code"),
    }
}

/// "ran for 2 seconds of processor time without returning" / "ran for 10 seconds without
/// returning" — the budget that ran out — or, short, "ran too long".
fn ran(clock: Clock, form: Form) -> String {
    match (clock, form) {
        (_, Form::Short) => "ran too long".to_string(),
        (Clock::Cpu, Form::Full) => format!("ran for {} seconds of processor time without returning", CPU_BUDGET.as_secs()),
        (Clock::Wall, Form::Full) => format!("ran for {} seconds without returning", WALL_BUDGET.as_secs()),
    }
}

/// ", in host.screen.pixel" — the host call the time ran out in.
fn in_call(trip: &Trip) -> String {
    trip.call.map_or(String::new(), |c| format!(", in {c}"))
}

/// "Overlay runtime's " before a file that is not the file of the module the words are about
/// (the library's, or the VM's own): a loop in a library's code that stopped only the module whose
/// callback ran it.
fn whose_file(trip: &Trip, l: &Location) -> String {
    if l.id == library_id(trip) {
        return String::new();
    }
    format!("{}'s ", trip.vm.name_of(&l.id).unwrap_or_else(|| l.id.clone()))
}

/// ", at src/main.luau line 40".
fn at(trip: &Trip) -> String {
    trip.location.as_ref().map_or(String::new(), |l| format!(", at {}{} line {}", whose_file(trip, l), l.rel, l.line))
}

/// What the stop said, first: "Stopped Guard probe: its hotkey callback ran for 2 seconds of
/// processor time without returning, at src/main.luau line 40".
fn first_words(ev: &StopEvent, form: Form) -> String {
    let t = &ev.trip;
    match &t.cause {
        Cause::Memory { limit_mib, .. } => match form {
            Form::Full => format!(
                "Stopped {}: {} needed more than the {limit_mib} megabytes of memory it may use",
                t.vm.name,
                subject(t)
            ),
            Form::Short => format!("Stopped {}: {} needed more memory than it may use", t.vm.name, own_call(t.kind, &t.what)),
        },
        Cause::Time { clock, .. } if as_library(ev) => {
            let lib = library_id(t);
            let code = if ev.members.iter().any(|m| m.id == lib) { "its code".to_string() } else { format!("{}'s code", library_name(t)) };
            let outer = match form {
                Form::Full => nested(t.outer.as_ref()).trim_end_matches(',').to_string(),
                Form::Short => String::new(),
            };
            let callback = if t.vm.id == lib {
                format!("its own {} callback{outer}", t.what)
            } else {
                format!("{}'s {} callback{outer}", t.vm.name, t.what)
            };
            format!("Stopped {}: {code} {} in {callback}{}{}", who(ev, form), ran(*clock, form), in_call(t), at(t))
        }
        Cause::Time { clock, .. } => {
            let what = match form {
                Form::Full => subject(t),
                Form::Short => own_call(t.kind, &t.what),
            };
            format!("Stopped {}: {what} {}{}{}", t.vm.name, ran(*clock, form), in_call(t), at(t))
        }
    }
}

/// The most a spoken sentence may have, in UTF-16 units: on Windows it is a notification
/// balloon, whose text Windows cuts at 255 (`NOTIFYICONDATA.szInfo`).
pub(crate) const SPOKEN_MAX: usize = 255;

/// The spoken sentence: what happened, and what it means — the module, the callback, the host call
/// and the place, in at most [`SPOKEN_MAX`] units. The first that fits of: everything; a shorter
/// second half; the error window named for the rest of it; then the same two with the first words
/// short — the group counted, "ran too long", no outer callback; then the keys alone after them;
/// and, for names too long for any of those, the last cut with "…".
pub(crate) fn sentence(ev: &StopEvent) -> String {
    let (long, short, least, keys) = if ev.members.len() > 1 {
        (
            "Their keys go to the program in front again, and they stay off until you turn them on in the module \
             manager or start the application again.",
            "Their keys go to the program in front again; they stay off until you turn them on in the module manager.",
            "Their keys go to the program in front again; more in the error window.",
            "Their keys go to the program in front again.",
        )
    } else {
        (
            "Its keys go to the program in front again, and it stays off until you turn it on in the module manager \
             or start the application again.",
            "Its keys go to the program in front again; it stays off until you turn it on in the module manager.",
            "Its keys go to the program in front again; more in the error window.",
            "Its keys go to the program in front again.",
        )
    };
    let (full, brief) = (first_words(ev, Form::Full), first_words(ev, Form::Short));
    let tries = [
        format!("{full}. {long}"),
        format!("{full}. {short}"),
        format!("{full}. {least}"),
        format!("{brief}. {short}"),
        format!("{brief}. {least}"),
        format!("{brief}. {keys}"),
    ];
    for t in &tries {
        if units(t) <= SPOKEN_MAX {
            return t.clone();
        }
    }
    cut(&tries[5], SPOKEN_MAX)
}

/// `s`'s length in UTF-16 units, as Windows counts it.
fn units(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s` cut to `max` UTF-16 units, the last of them "…".
fn cut(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if units(&out) + c.len_utf16() + 1 > max {
            break;
        }
        out.push(c);
    }
    out.push('\u{2026}');
    out
}

/// The dialog's title.
pub(crate) fn dialog_title(ev: &StopEvent) -> String {
    match (ev.members.as_slice(), library_member(ev)) {
        ([] | [_], _) => format!("Module stopped: {}", ev.trip.vm.id),
        ([first, rest @ ..], Some(lib)) if lib.id == first.id => {
            format!("Modules stopped: {} and {} that run its code", first.id, rest.len())
        }
        (all, _) => {
            let ids: Vec<String> = all.iter().map(|m| m.id.clone()).collect();
            format!("Modules stopped: {}, which run {}'s code", listed(&ids), library_name(&ev.trip))
        }
    }
}

/// What the VM may use and why: "256 of its own and 256 for Overlay runtime
/// (com.platform.overlay), whose code runs in it", or "all of it its own".
pub(crate) fn breakdown(vm: &VmInfo) -> String {
    if vm.code.len() <= 1 {
        let declared = vm.code.first().is_some_and(|c| c.declared);
        return if declared { "all of it its own, set in its module.toml".to_string() } else { "all of it its own".to_string() };
    }
    let mut parts: Vec<String> = Vec::new();
    for (i, c) in vm.code.iter().enumerate() {
        let declared = if c.declared { " (set in its module.toml)" } else { "" };
        parts.push(if i == 0 {
            format!("{} of its own{declared}", c.mib)
        } else {
            format!("{} for {} ({}){declared}", c.mib, c.name, c.id)
        });
    }
    let last = parts.pop().unwrap_or_default();
    format!("{} and {last}, whose code runs in it", parts.join(", "))
}

/// "the left mouse button it was holding down", "the left and right mouse buttons Guard probe B was
/// holding down".
fn released_words(ev: &StopEvent) -> Option<String> {
    if ev.released.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for m in &ev.members {
        let buttons: Vec<String> = ev.released.iter().filter(|(n, _)| *n == m.name).map(|(_, b)| b.to_string()).collect();
        if buttons.is_empty() {
            continue;
        }
        let holder = if ev.members.len() == 1 { "it".to_string() } else { m.name.clone() };
        let noun = if buttons.len() == 1 { "mouse button" } else { "mouse buttons" };
        parts.push(format!("the {} {noun} {holder} was holding down", listed(&buttons)));
    }
    Some(parts.join("; "))
}

/// "3 keys pressed while the application waited", "2 hotkey presses made while the application
/// waited", "1 key and 2 hotkey presses, made while the application waited".
fn dropped_count(d: &Dropped) -> String {
    let keys = match d.keys {
        1 => "1 key".to_string(),
        n => format!("{n} keys"),
    };
    let hotkeys = match d.hotkeys {
        1 => "1 hotkey press".to_string(),
        n => format!("{n} hotkey presses"),
    };
    match (d.keys, d.hotkeys) {
        (_, 0) => format!("{keys} pressed while the application waited"),
        (0, _) => format!("{hotkeys} made while the application waited"),
        _ => format!("{keys} and {hotkeys}, made while the application waited"),
    }
}

/// The dialog's text: what the user and the module's author need, in the order they need it.
pub(crate) fn dialog_text(ev: &StopEvent) -> String {
    let t = &ev.trip;
    let mut s = format!("{}.\n\n", first_words(ev, Form::Full));
    match &t.cause {
        Cause::Memory { limit_mib, used } => {
            s.push_str(&format!("What used too much memory: {}.\n", call_of(t)));
            s.push_str(&format!(
                "How much: more than its {limit_mib} MiB \u{2014} {} \u{2014} with {} MiB in use when it was stopped.\n",
                breakdown(&t.vm),
                mib(*used)
            ));
            match &t.traceback {
                Some(tb) => {
                    s.push_str("Where, as far as it is known:\n");
                    for line in tb.lines().map(str::trim).filter(|l| !l.is_empty()) {
                        s.push_str(&format!("  {line}\n"));
                    }
                }
                None => s.push_str("Where: not known. Luau does not say where it ran out of memory.\n"),
            }
        }
        Cause::Time { cpu, wall, .. } => {
            if as_library(ev) {
                s.push_str(&format!("What ran too long: {}'s code, in {}.\n", library_name(t), call_of(t)));
            } else {
                s.push_str(&format!("What ran too long: {}.\n", call_of(t)));
            }
            s.push_str(&format!(
                "How long: {} s of processor time, {} s in all. A callback may take {} s of processor time or {} s in all.\n",
                secs(*cpu),
                secs(*wall),
                CPU_BUDGET.as_secs(),
                WALL_BUDGET.as_secs()
            ));
            match t.call {
                Some(c) => s.push_str(&format!("In a host call: {c} \u{2014} the time ran out in it, or it had just returned.\n")),
                None => s.push_str("In a host call: no, it was running Luau code.\n"),
            }
            if t.frames.is_empty() {
                s.push_str("Where: not known.\n");
            } else {
                s.push_str("Where, innermost first:\n");
                for f in &t.frames {
                    s.push_str(&format!("  {f}\n"));
                }
            }
        }
    }
    let off: Vec<String> = ev.members.iter().map(|m| format!("{} ({})", m.name, m.id)).collect();
    s.push_str(&format!("Turned off: {}.\n", off.join(", ")));
    if let Some(r) = released_words(ev) {
        s.push_str(&format!("Released: {r}.\n"));
    }
    if !ev.dropped.is_empty() {
        s.push_str(&format!("Dropped: {}: {}.\n", dropped_count(&ev.dropped), ev.dropped.names.join(", ")));
    }
    s.push('\n');
    s.push_str(&closing(ev));
    s
}

/// The dialog's last paragraph: the keys, how long it stays off, what turning it on does, and how
/// to keep it off.
fn closing(ev: &StopEvent) -> String {
    let author = "If it happens again, send this text to the module's author.";
    if ev.members.len() <= 1 {
        return format!(
            "Its keys go to the program in front again. It stays off until you turn it on again in the module \
             manager's Installed list, or start the application again; turned on, it is built afresh, as a reload \
             builds it. To keep it off after a restart too, tick it and untick it. {author}"
        );
    }
    let how = match library_member(ev) {
        Some(lib) => format!("ticking {} turns them all on again, each built afresh, as a reload builds it", lib.name),
        None => "tick each of them to turn it on again, built afresh, as a reload builds it".to_string(),
    };
    format!(
        "Their keys go to the program in front again. They stay off until you turn them on again in the module \
         manager's Installed list, or start the application again; {how}. To keep one off after a restart too, \
         tick it and untick it. {author}"
    )
}

/// The `[guard]` line, written by [`mark`] at once.
pub(crate) fn log_line(ev: &StopEvent) -> String {
    let t = &ev.trip;
    let ids: Vec<&str> = ev.members.iter().map(|m| m.id.as_str()).collect();
    let mut s = match &t.cause {
        Cause::Memory { limit_mib, used } => {
            let mut s = format!(
                "[{}] stopped: {} needed more than the {limit_mib} MiB of memory its VM may use ({} MiB in use)",
                t.vm.id,
                subject(t),
                mib(*used)
            );
            match &t.traceback {
                Some(tb) => {
                    let one: Vec<&str> = tb.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
                    s.push_str(&format!("; where, as far as it is known: {}", one.join(" | ")));
                }
                None => s.push_str("; where is not known"),
            }
            s
        }
        Cause::Time { clock, cpu, wall } => {
            let what = if as_library(ev) { format!("the code of {} in {}", library_id(t), subject(t)) } else { subject(t) };
            let limit = match clock {
                Clock::Cpu => "the processor-time limit",
                Clock::Wall => "the wall-clock limit",
            };
            let call = t.call.map_or("in Luau code".to_string(), |c| format!("in {c}"));
            let place = t.location.as_ref().map_or("where is not known".to_string(), |l| format!("at {}/{}:{}", l.id, l.rel, l.line));
            format!(
                "[{}] stopped: {what} ran {} ms of processor time ({} ms in all) without returning, past {limit}; {call}; {place}",
                t.vm.id,
                cpu.as_millis(),
                wall.as_millis()
            )
        }
    };
    s.push_str(&format!("; off until the next start: {}", ids.join(", ")));
    if !ev.released.is_empty() {
        let r: Vec<String> = ev.released.iter().map(|(n, b)| format!("{b} mouse button of {n}")).collect();
        s.push_str(&format!("; released: {}", r.join(", ")));
    }
    if !ev.dropped.is_empty() {
        s.push_str(&format!("; dropped: {} key(s), {} hotkey press(es)", ev.dropped.keys, ev.dropped.hotkeys));
    }
    if !t.frames.is_empty() {
        s.push_str(&format!("; frames: {}", t.frames.join(" | ")));
    }
    s
}

/// The `[keys]` line for the input a stop dropped.
pub(crate) fn dropped_line(ev: &StopEvent) -> String {
    let d = &ev.dropped;
    format!(
        "{} key(s) and {} hotkey press(es) made while {} held the application were dropped, as the module was \
         stopped: {}",
        d.keys,
        d.hotkeys,
        call_of(&ev.trip),
        d.names.join(", ")
    )
}

/// The `[guard]` line for a module that was off already when its code was stopped — a setting's
/// `onChange`, which a disabled module still hears.
pub(crate) fn already_off_line(trip: &Trip) -> String {
    let rest = "it was off already, and runs none of its code until it is turned on again";
    match &trip.cause {
        Cause::Memory { limit_mib, used } => format!(
            "[{}] {} needed more than the {limit_mib} MiB of memory its VM may use ({} MiB in use); {rest}",
            trip.vm.id,
            subject(trip),
            mib(*used)
        ),
        Cause::Time { cpu, wall, .. } => format!(
            "[{}] {} ran too long ({} ms of processor time, {} ms in all); {rest}",
            trip.vm.id,
            subject(trip),
            cpu.as_millis(),
            wall.as_millis()
        ),
    }
}

/// The `[guard]` line for a module that runs a stopped library's code and was off already.
pub(crate) fn off_member_line(id: &str, trip: &Trip) -> String {
    format!(
        "[{id}] runs the code of {}, which was stopped; it was off already, and runs none of its code until it is \
         turned on again",
        library_id(trip)
    )
}

/// The note in the manager's row of module `idx`: "stopped: its hotkey callback needed more than
/// 256 MiB of memory", "stopped: its hotkey callback ran too long, in host.screen.pixel, at
/// src/main.luau:40", "stopped with Guard probe library: its code ran too long in Guard probe B's
/// hotkey callback, at src/main.luau:9" — the callback, the host call and the place, as the
/// sentence names them.
pub(crate) fn row_note(ev: &StopEvent, idx: usize) -> String {
    let t = &ev.trip;
    let place = t.location.as_ref().map_or(String::new(), |l| format!(", at {}{}:{}", whose_file(t, l), l.rel, l.line));
    match &t.cause {
        Cause::Memory { limit_mib, .. } => format!("stopped: {} needed more than {limit_mib} MiB of memory", own_call(t.kind, &t.what)),
        Cause::Time { .. } if as_library(ev) => {
            let lib = library_id(t);
            let named = format!("{}'s {} callback", t.vm.name, t.what);
            if ev.members.iter().any(|m| m.idx == idx && m.id == lib) {
                let callback = if t.vm.id == lib { format!("its own {} callback", t.what) } else { named };
                format!("stopped: its code ran too long in {callback}{}{place}", in_call(t))
            } else {
                format!("stopped with {}: its code ran too long in {named}{}{place}", library_name(t), in_call(t))
            }
        }
        Cause::Time { .. } => format!("stopped: {} ran too long{}{place}", own_call(t.kind, &t.what), in_call(t)),
    }
}

/// What a load that a stop ended fails with: "stopped while loading: its entry file needed more
/// than the 256 MiB of memory it may use", "stopped while loading: its entry file ran for 2 seconds
/// of processor time without returning, at src/main.luau line 3".
pub(crate) fn load_failure(trip: Option<&Trip>) -> String {
    match trip {
        None => "stopped while loading".to_string(),
        Some(t) => match &t.cause {
            Cause::Memory { limit_mib, .. } => {
                format!("stopped while loading: {} needed more than the {limit_mib} MiB of memory it may use", subject(t))
            }
            Cause::Time { clock, .. } => {
                format!("stopped while loading: {} {}{}{}", subject(t), ran(*clock, Form::Full), in_call(t), at(t))
            }
        },
    }
}

/// What the manager's Details… shows for a module.
pub(crate) struct Details<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub id: &'a str,
    pub enabled: bool,
    pub stop: Option<&'a StopEvent>,
    pub vm: &'a VmInfo,
    /// Bytes its VM uses now.
    pub used: u64,
    /// The other modules whose VMs run its code, as (name, id).
    pub carriers: &'a [(String, String)],
    pub dependencies: &'a [String],
    pub now: Instant,
}

/// "less than a minute ago", "3 minutes ago", "2 hours ago".
fn ago(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => "less than a minute ago".to_string(),
        60..=119 => "a minute ago".to_string(),
        120..=3599 => format!("{} minutes ago", s / 60),
        3600..=7199 => "an hour ago".to_string(),
        _ => format!("{} hours ago", s / 3600),
    }
}

/// The text of the manager's Details… for a module.
pub(crate) fn details_text(d: &Details<'_>) -> String {
    let mut s = format!("{} {} ({})\n", d.name, d.version, d.id);
    match (d.enabled, d.stop) {
        (_, Some(ev)) => s.push_str(&format!(
            "State: off \u{2014} stopped by the application {}: {}. Its keys went to the program in front again. It \
             stays off until you tick it in the Installed list or start the application again; ticked, it is built \
             afresh. To keep it off after a restart too, tick it and untick it.\n",
            ago(d.now.saturating_duration_since(ev.at)),
            first_words(ev, Form::Full)
        )),
        (true, None) => s.push_str("State: on.\n"),
        (false, None) => s.push_str("State: off (turned off in the Installed list).\n"),
    }
    s.push_str(&format!(
        "Memory: its VM may use up to {} MiB \u{2014} {}. In use now: {} MiB.\n",
        d.vm.limit_mib(),
        breakdown(d.vm),
        mib(d.used)
    ));
    if d.carriers.is_empty() {
        s.push_str("Its code runs only in its own VM.\n");
    } else {
        let list: Vec<String> = d.carriers.iter().map(|(name, id)| format!("{name} ({id})")).collect();
        s.push_str(&format!("Its code also runs in the VMs of: {}.\n", list.join(", ")));
    }
    if d.dependencies.is_empty() {
        s.push_str("Depends on no other module.");
    } else {
        s.push_str(&format!("Depends on: {}.", d.dependencies.join(", ")));
    }
    s
}
