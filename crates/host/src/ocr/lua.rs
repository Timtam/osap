//! The Luau side of `host.ocr.recognize`, `pending`, `languages` and `resolveLanguage`: strict
//! argument parsing, the reads waiting for their answers, and delivery on the event loop.
//!
//! **Only programming mistakes raise.** A wrong type, an unknown option, a malformed language
//! tag, no region or more than 64, corners of too many pixels, a name used twice: those are
//! mistakes in the call, and they raise at the call. Everything that goes wrong routinely — a
//! capture that failed, a language not installed, a read superseded by a newer one, a window
//! whose size leaves nothing to read — arrives as a reading with a `status`, so code that only
//! looks at `.text` never raises for it.
//!
//! **Delivered while the module is enabled, or not at all.** A callback runs exactly once, on
//! the event loop, under the priority of the dispatch that asked for it. It is dropped when its
//! module is disabled, reloaded or unloaded first, as a one-shot timer is — a click in a
//! callback that ran minutes later, over whatever window was in front then, would be worse than
//! none.
//!
//! **Two kinds of waiter.** A read asked by `host.ocr.recognize` with a callback waits with its
//! callback, which runs as a handler of its module, through its mailbox (`mailbox.rs`); one asked
//! without a callback in a handler is that handler's wait (`task.rs`), which the delivery resumes
//! with the reading — it is the busy handler itself, not an event for its mailbox. Both go through
//! the same queue, the same limits and the same delivery, in one order, and both get the same
//! values: one reading, or the list and the table by name.
//!
//! **An answer in a mailbox is still out.** A callback's answer that waits in its module's mailbox
//! — behind a key pressed before it came — counts as waiting for `host.ocr.pending` until it runs,
//! and whether a newer read with its key was asked is decided when it runs: so the key's handler,
//! which runs first, sees the read still out, and the answer comes `newer` when that handler asked
//! again.
//!
//! **When the recogniser hangs, every read still out ends.** Once it has answered no region for
//! `policy::HANG`, the delivery's sweep (`Service::hang_sweep`) answers every read waiting —
//! being recognised, or queued behind that — `"failed"` with the reason, once, both kinds of
//! waiter alike; a late answer finds nobody, and `host.ocr.pending` is false again.
//!
//! The functions that do the work are generic over [`ReadHost`] — the host's `Shared`, or the
//! tests' holder of a real service over a fake recogniser — so the handlers' rules can be tested
//! against a real Luau VM without the speech engines a `Shared` opens.

use std::cell::{Cell, Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::backend::frame::Frame;
use crate::backend::CaptureSource;
use crate::image_search::vm_owner;
use crate::region::Region;
use crate::mailbox::{self, Event, MailHost, Opened};
use crate::task::{Ctx, TaskId, Tasks};
use crate::{capture_source, logging, region_lua, Shared};

use super::lang::{self, LangReq};
use super::policy::{self, MAX_CALL_PIXELS, MAX_REGIONS};
use super::sched::{Owner, Ticket, TicketId, TOO_MANY};
use super::service::{Done, Picture, Service, Spec};
use super::types::{current_priority, enter_priority, Priority, Reading, Rect, Status};

/// What the read path needs of whatever holds it: the host's `Shared`, or the tests' holder.
pub(crate) trait ReadHost: 'static {
    /// The pixels the service's capture stage hands its recognise stage.
    type Shot: Send + 'static;
    fn ocr(&self) -> &Service<Self::Shot>;
    fn ocr_state(&self) -> &OcrState;
    fn tasks(&self) -> &Tasks;
    /// module_idx → the generation of the VM it runs now (`image_search::register_vm`).
    fn vm_gens(&self) -> Ref<'_, HashMap<usize, u64>>;
    fn module_enabled(&self, idx: usize) -> bool;
    /// The module's id, for a log line; "?" when there is none.
    fn module_id(&self, idx: usize) -> String;
    /// Turns `host.epoch()` over: a delivery is a fresh look at the screen.
    fn bump_epoch(&self);
    /// A module's callback failed: the log line and the dialog.
    fn report_error(&self, idx: usize, context: &str, message: &str);
}

impl ReadHost for Shared {
    type Shot = crate::backend::OcrShot;
    fn ocr(&self) -> &Service<Self::Shot> {
        &self.ocr
    }
    fn ocr_state(&self) -> &OcrState {
        &self.ocr_state
    }
    fn tasks(&self) -> &Tasks {
        &self.tasks
    }
    fn vm_gens(&self) -> Ref<'_, HashMap<usize, u64>> {
        self.vm_gens.borrow()
    }
    fn module_enabled(&self, idx: usize) -> bool {
        self.enabled.borrow().get(idx).copied().unwrap_or(false)
    }
    fn module_id(&self, idx: usize) -> String {
        self.ids.borrow().get(idx).cloned().unwrap_or_else(|| "?".to_string())
    }
    fn bump_epoch(&self) {
        Shared::bump_epoch(self);
    }
    fn report_error(&self, idx: usize, context: &str, message: &str) {
        self.report_callback_error(idx, context, message);
    }
}

/// A snapshot a read was handed: compared by identity, shown by its rectangle.
#[derive(Clone)]
pub(crate) struct SnapArg(pub Arc<Frame>);

impl PartialEq for SnapArg {
    fn eq(&self, other: &SnapArg) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for SnapArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SnapArg({:?})", self.0.rect)
    }
}

/// What a `host.ocr.recognize` call asked for, once its arguments passed.
#[derive(Debug, PartialEq)]
pub(crate) struct ReadArgs {
    /// Each region with its name, in the order given: its rectangle, or — a window region whose
    /// client area is empty at the call, or that would take the call past the pixel limit — why
    /// it has none, which its reading will carry.
    pub entries: Vec<(Option<String>, Result<Rect, String>)>,
    /// A list was given (even of one): the reading comes as `(list, byName)`.
    pub list: bool,
    pub lang: LangReq,
    pub key: Option<String>,
    /// Read this snapshot instead of photographing the screen.
    pub snapshot: Option<SnapArg>,
}

fn bad(msg: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::external(format!("host.ocr.recognize: {msg}"))
}

/// A region, read by the Region form's one strict reader (`region_lua.rs`, shared with the
/// cells calls). A mistake raises. `parse_read` resolves it, once every region is read.
fn region(v: &Value, what: &str) -> mlua::Result<Region> {
    region_lua::read(v, what).map_err(bad)
}

/// The regions of a call as rectangles, resolved at the call: corners as given, a window region
/// against the client area its table carries.
///
/// The pixel limit is applied the way the Region form's rule splits mistakes from run time.
/// Corners are what the module wrote, so corners past `MAX_CALL_PIXELS` together are a mistake
/// in the call and raise. A window region's size is the window's at run time — the same call
/// must not raise with the game at full screen and work in a smaller window — so window regions
/// are counted after the corners, in the order given, and one that would take the call past
/// the limit is `Err`: its reading fails with the reason, and it is not counted. So is a window
/// region whose client area is empty. `fname` names the call in a mistake.
fn resolve_all(
    regions: Vec<(Option<String>, Region)>,
    fname: &str,
) -> mlua::Result<Vec<(Option<String>, Result<Rect, String>)>> {
    let rect = |r: crate::region::ScreenRect| Rect::new(r.x, r.y, r.w, r.h);
    // Saturating: each rectangle fits the coordinate range, but three of the largest do not
    // fit an i64 together.
    let corners: i64 = regions
        .iter()
        .map(|(_, r)| match r {
            Region::Rect(s) => rect(*s).area(),
            Region::Window(..) => 0,
        })
        .fold(0, i64::saturating_add);
    if corners > MAX_CALL_PIXELS {
        return Err(mlua::Error::external(format!(
            "{fname}: {corners} pixels in one call; at most {MAX_CALL_PIXELS}"
        )));
    }
    let mut left = MAX_CALL_PIXELS - corners;
    Ok(regions
        .into_iter()
        .map(|(name, r)| {
            let resolved = match r {
                Region::Rect(s) => Ok(rect(s)),
                Region::Window(..) => match r.resolve() {
                    Err(u) => Err(u.to_string()),
                    Ok(s) => {
                        let s = rect(s);
                        if s.area() > left {
                            Err(format!(
                                "not read: at the window's current size this region is {}x{}, which \
                                 would take the call past {MAX_CALL_PIXELS} pixels",
                                s.w, s.h
                            ))
                        } else {
                            left -= s.area();
                            Ok(s)
                        }
                    }
                },
            };
            (name, resolved)
        })
        .collect())
}

/// `{ region = Region, name = string? }`.
fn entry(t: &Table, what: &str) -> mlua::Result<(Option<String>, Region)> {
    let mut rect = None;
    let mut name = None;
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, v) = pair?;
        let key = match &k {
            Value::String(s) => s.to_str()?.to_string(),
            _ => return Err(bad(format!("{what} has an unexpected key — an entry is {{ region = …, name = … }}"))),
        };
        match (key.as_str(), v) {
            ("region", v) => rect = Some(region(&v, &format!("{what}.region"))?),
            ("name", Value::String(s)) => {
                let s = s.to_str()?.to_string();
                if s.is_empty() {
                    return Err(bad(format!("{what}.name is empty")));
                }
                name = Some(s);
            }
            ("name", other) => {
                return Err(bad(format!("{what}.name must be a string, got {}", other.type_name())))
            }
            (other, _) => {
                return Err(bad(format!(
                    "{what} has an unknown field '{other}' — an entry is {{ region = …, name = … }}"
                )))
            }
        }
    }
    let region = rect.ok_or_else(|| bad(format!("{what} has no region")))?;
    Ok((name, region))
}

/// A region or an entry, whichever `t` is. `what` names it in a message: `entry 3` in a list,
/// and for the single argument `the region` or `the entry`, whichever it is.
fn one(t: &Table, what: &str) -> mlua::Result<(Option<String>, Region)> {
    if t.raw_get::<Value>("region")? != Value::Nil {
        entry(t, if what == "the region" { "the entry" } else { what })
    } else {
        Ok((None, region(&Value::Table(t.clone()), what)?))
    }
}

/// `lang` as a request: nil, a tag, or a list of tags in order of preference. A malformed tag
/// raises.
pub(crate) fn lang_request(v: &Value, what: &str) -> mlua::Result<LangReq> {
    let req = match v {
        Value::Nil => LangReq::Default,
        Value::String(s) => LangReq::Tags(vec![s.to_str()?.to_string()]),
        Value::Table(t) => {
            let mut tags = Vec::new();
            for (i, item) in t.clone().sequence_values::<Value>().enumerate() {
                match item? {
                    Value::String(s) => tags.push(s.to_str()?.to_string()),
                    other => {
                        return Err(mlua::Error::external(format!(
                            "{what}: language {} must be a string, got {}",
                            i + 1,
                            other.type_name()
                        )))
                    }
                }
            }
            if t.clone().pairs::<Value, Value>().count() != tags.len() {
                return Err(mlua::Error::external(format!(
                    "{what}: a list of languages has only numbered entries"
                )));
            }
            LangReq::Tags(tags)
        }
        other => {
            return Err(mlua::Error::external(format!(
                "{what}: a language is a tag like \"de-DE\" or a list of them, got {}",
                other.type_name()
            )))
        }
    };
    lang::validate(req).map_err(|e| mlua::Error::external(format!("{what}: {e}")))
}

/// Every argument of `host.ocr.recognize(what, opts?, cb?)` but the callback, checked: the same
/// for both forms, the callback's and the one that waits.
pub(crate) fn parse_read(what: &Value, opts: &Value) -> mlua::Result<ReadArgs> {
    let t = match what {
        Value::Table(t) => t,
        other => {
            return Err(bad(format!(
                "the first argument is a region, an entry or a list of them, got {}",
                other.type_name()
            )))
        }
    };
    let is_list = matches!(t.raw_get::<Value>(1)?, Value::Table(_));
    let regions = if is_list {
        let n = t.raw_len();
        if t.clone().pairs::<Value, Value>().count() != n {
            return Err(bad("a list of regions has only numbered entries, without holes"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 1..=n {
            match t.raw_get::<Value>(i)? {
                Value::Table(item) => out.push(one(&item, &format!("entry {i}"))?),
                other => return Err(bad(format!("entry {i} must be a table, got {}", other.type_name()))),
            }
        }
        out
    } else {
        vec![one(t, "the region")?]
    };
    if regions.is_empty() {
        return Err(bad("no region to read"));
    }
    if regions.len() > MAX_REGIONS {
        return Err(bad(format!(
            "{} regions in one call; at most {MAX_REGIONS}",
            regions.len()
        )));
    }
    let entries = resolve_all(regions, "host.ocr.recognize")?;
    let mut seen = HashSet::new();
    for (name, _) in &entries {
        if let Some(n) = name {
            if !seen.insert(n.clone()) {
                return Err(bad(format!("the name '{n}' is used twice")));
            }
        }
    }

    let (mut lang, mut key, mut snapshot) = (LangReq::Default, None, None);
    match opts {
        Value::Nil => {}
        Value::Table(o) => {
            for pair in o.clone().pairs::<Value, Value>() {
                let (k, v) = pair?;
                let name = match &k {
                    Value::String(s) => s.to_str()?.to_string(),
                    _ => return Err(bad("options have only the fields `lang`, `key` and `snapshot`")),
                };
                match name.as_str() {
                    "lang" => lang = lang_request(&v, "host.ocr.recognize")?,
                    "key" => match v {
                        Value::String(s) if !s.to_str()?.is_empty() => key = Some(s.to_str()?.to_string()),
                        Value::String(_) => return Err(bad("`key` is empty")),
                        other => return Err(bad(format!("`key` must be a string, got {}", other.type_name()))),
                    },
                    // A Snapshot, not released: anything else raises, a Template handle included.
                    "snapshot" => snapshot = crate::snapshot::from_value(&v, "host.ocr.recognize", "opts.snapshot")?.map(SnapArg),
                    other => {
                        return Err(bad(format!(
                            "unknown option '{other}' — the options are `lang`, `key` and `snapshot`"
                        )))
                    }
                }
            }
        }
        other => {
            return Err(bad(format!(
                "the options must be a table or nil, got {} — the callback goes last",
                other.type_name()
            )))
        }
    }
    Ok(ReadArgs { entries, list: is_list, lang, key, snapshot })
}

/// The regions of a read of a snapshot, cut to the part of it the recogniser can read
/// (`Frame::ocr_rect`): a region reads what of it the snapshot holds, and one of which it holds
/// nothing is not read — its reading fails with the reason, as a window region with no rectangle
/// does, and the call's other regions are read.
pub(crate) fn clip_to_snapshot(
    entries: Vec<(Option<String>, Result<Rect, String>)>,
    frame: &Frame,
) -> Vec<(Option<String>, Result<Rect, String>)> {
    let readable = frame.ocr_rect();
    entries
        .into_iter()
        .map(|(name, r)| {
            let r = r.and_then(|r| readable.intersect(&r).ok_or_else(|| crate::snapshot::NO_OVERLAP.to_string()));
            (name, r)
        })
        .collect()
}

/// When a reading's picture was taken, in `host.now()`'s milliseconds, and `host.inputEpoch()`
/// then: a reading's `time` and `inputEpoch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    pub time: i64,
    pub input_epoch: u64,
}

impl Seen {
    pub(crate) fn of(p: Picture) -> Seen {
        Seen {
            time: p.at.saturating_duration_since(crate::clock_origin()).as_millis() as i64,
            input_epoch: p.input_epoch,
        }
    }
}

/// What each reading of a delivery says about its picture: the job's, for a reading of the screen —
/// `"text"`, `"blank"` or `"none"` — and nothing for any other. A `"failed"` one may have had no
/// picture of its region at all (a capture that failed for that region, a window region without
/// a rectangle) and an answer decided without the threads (refused, evicted) never had one; a
/// `"stale"` one was never read. So the moment is there exactly when the screen was read.
fn seen_of(picture: Option<Picture>, unresolved: &[Option<String>], readings: &[Reading]) -> Vec<Option<Seen>> {
    readings
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let screen = matches!(r.status, Status::Text | Status::Blank | Status::None);
            let read = unresolved.get(i).is_none_or(Option::is_none) && screen;
            picture.filter(|_| read).map(Seen::of)
        })
        .collect()
}

/// One reading as the table a callback receives, and a wait returns; `time` and `inputEpoch` from
/// `seen`; `skipped`, whether no recogniser was asked: `status == "blank"`.
pub(crate) fn reading_table(
    lua: &Lua,
    r: &Reading,
    name: Option<&str>,
    newer: bool,
    seen: Option<Seen>,
) -> mlua::Result<Table> {
    let word_table = |w: &super::types::Word| -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("text", w.text.as_str())?;
        t.set("x", w.x)?;
        t.set("y", w.y)?;
        t.set("w", w.w)?;
        t.set("h", w.h)?;
        if w.approx {
            t.set("approx", true)?;
        }
        Ok(t)
    };
    let t = lua.create_table()?;
    if let Some(n) = name {
        t.set("name", n)?;
    }
    t.set("x", r.rect.x)?;
    t.set("y", r.rect.y)?;
    t.set("w", r.rect.w)?;
    t.set("h", r.rect.h)?;
    t.set("status", r.status.as_str())?;
    t.set("skipped", r.status == Status::Blank)?;
    t.set("newer", newer)?;
    t.set("text", r.text.as_str())?;
    let lines = lua.create_table_with_capacity(r.rows.len(), 0)?;
    for row in &r.rows {
        let l = lua.create_table()?;
        l.set("text", row.text.as_str())?;
        l.set("x", row.rect.x)?;
        l.set("y", row.rect.y)?;
        l.set("w", row.rect.w)?;
        l.set("h", row.rect.h)?;
        let words = lua.create_table_with_capacity(row.words.len(), 0)?;
        for w in &row.words {
            words.raw_push(word_table(w)?)?;
        }
        l.set("words", words)?;
        lines.raw_push(l)?;
    }
    t.set("lines", lines)?;
    let words = lua.create_table_with_capacity(r.words.len(), 0)?;
    for w in &r.words {
        words.raw_push(word_table(w)?)?;
    }
    t.set("words", words)?;
    t.set("lang", r.lang.as_str())?;
    if let Some(e) = &r.error {
        t.set("error", e.as_str())?;
    }
    if let Some(s) = seen.filter(|_| r.status != Status::Stale) {
        t.set("time", s.time)?;
        t.set("inputEpoch", s.input_epoch as i64)?;
    }
    Ok(t)
}

/// What a callback is called with, and a wait returns: one reading for a single region or entry,
/// and for a list `(list, byName)` — `list` a plain array in the order asked, `byName` the named readings.
/// Two tables rather than one array with names beside the indices, so the list stays plain data
/// that `host.json.encode` or an export passes on unchanged.
pub(crate) fn callback_args(
    lua: &Lua,
    readings: &[Reading],
    names: &[Option<String>],
    list: bool,
    newer: bool,
    seen: &[Option<Seen>],
) -> mlua::Result<mlua::MultiValue> {
    let array = lua.create_table_with_capacity(readings.len(), 0)?;
    let by_name = lua.create_table()?;
    for (i, r) in readings.iter().enumerate() {
        let name = names.get(i).and_then(|n| n.as_deref());
        let t = reading_table(lua, r, name, newer, seen.get(i).copied().flatten())?;
        if let Some(n) = name {
            by_name.set(n, t.clone())?;
        }
        array.raw_push(t)?;
    }
    if list {
        Ok(mlua::MultiValue::from_vec(vec![Value::Table(array), Value::Table(by_name)]))
    } else {
        Ok(mlua::MultiValue::from_vec(vec![array.raw_get::<Value>(1)?]))
    }
}

/// What the blocking `recognize` returns where it could not wait (`lib.rs`, `legacy_read`): the
/// same values a read hands its callback — one reading, or the list and the table by name — for
/// the `readings` of `names`' regions, a window region that had no rectangle answered with why
/// (`unresolved`), the screen read as `picture` says: when its capture began, and the input epoch.
pub(crate) fn blocking_values(
    lua: &Lua,
    readings: Vec<Reading>,
    names: &[Option<String>],
    unresolved: &[Option<String>],
    list: bool,
    picture: Picture,
) -> mlua::Result<mlua::MultiValue> {
    let readings = with_unresolved(readings, unresolved);
    let seen = seen_of(Some(picture), unresolved, &readings);
    callback_args(lua, &readings, names, list, false, &seen)
}

/// Who waits for a read: the callback of `host.ocr.recognize`, or a handler stopped in it, resumed
/// with [`callback_args`]'s values (`list`: a list was asked for).
pub(crate) enum Waiter {
    Callback(RegistryKey),
    Handler { id: TaskId, list: bool },
}

/// A callback's answer, as it waits in its module's mailbox (`Event::Read`): the read, and what it
/// was answered.
pub(crate) struct ReadAnswer {
    pub(crate) p: PendingOcr,
    pub(crate) readings: Vec<Reading>,
    pub(crate) picture: Option<Picture>,
}

/// A read waiting for its answer. Main thread only.
pub(crate) struct PendingOcr {
    /// Its own ticket: tickets are handed out in order, so a larger one with the same key is a
    /// newer read.
    ticket: TicketId,
    lua: Lua,
    waiter: Waiter,
    /// The identity the call was made under, named in an error report.
    scope: usize,
    owner: Owner,
    prio: Priority,
    names: Vec<Option<String>>,
    rects: Vec<Rect>,
    /// Per region: why it had no rectangle at the call (a window region, its client area
    /// empty). Such a region goes to the queue as an empty rectangle, and its reading is
    /// given this reason on delivery.
    unresolved: Vec<Option<String>>,
    list: bool,
    key: Option<String>,
}

/// Everything `host.ocr.recognize` keeps on the event loop.
#[derive(Default)]
pub(crate) struct OcrState {
    pending: RefCell<HashMap<TicketId, PendingOcr>>,
    /// Answers decided without the threads — stale, evicted, refused — delivered on the next
    /// tick, never from inside the binding.
    ready: RefCell<Vec<(TicketId, Status, Option<String>)>>,
    /// (module index, VM generation, key) → the newest ticket asked with that key, while a read
    /// with that key is waiting: forgotten once none is (`forget_settled`), as `snapshotAsync`
    /// forgets its keys, so a key made from a row number or from text is not kept for the
    /// module's whole life.
    key_seq: RefCell<HashMap<(usize, u64, String), TicketId>>,
    next_ticket: Cell<TicketId>,
    /// Per module: when a slow job was last logged.
    slow_logged: RefCell<HashMap<usize, Instant>>,
    /// Log lines said once per session.
    said: RefCell<HashSet<String>>,
    /// How many answers wait in mailboxes, by module VM and key: still out for `host.ocr.pending`
    /// until they run (`note_queued`).
    queued: RefCell<HashMap<QueuedKey, u32>>,
}

/// (module index, VM generation, key) of an answer waiting in its module's mailbox.
type QueuedKey = (usize, u64, Option<String>);

impl OcrState {
    /// Whether any read is waiting — a reason for the headless loop to run.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.borrow().is_empty()
            || !self.ready.borrow().is_empty()
            || !self.queued.borrow().is_empty()
    }

    fn once(&self, key: String) -> bool {
        self.said.borrow_mut().insert(key)
    }

    /// Whether the line said once under `key` was said.
    #[cfg(test)]
    pub(crate) fn has_said(&self, key: &str) -> bool {
        self.said.borrow().contains(key)
    }

    /// Whether a read of `owner` with `key` is still waiting for its answer — or its answer waits
    /// in the module's mailbox.
    fn waiting_with(&self, owner: Owner, key: &str) -> bool {
        self.pending.borrow().values().any(|p| p.owner == owner && p.key.as_deref() == Some(key))
            || self.queued.borrow().contains_key(&(owner.idx, owner.gen, Some(key.to_string())))
    }

    /// An answer left its module's mailbox: it runs, or it is dropped.
    fn unqueue(&self, p: &PendingOcr) {
        let mut queued = self.queued.borrow_mut();
        let at = (p.owner.idx, p.owner.gen, p.key.clone());
        if let Some(n) = queued.get_mut(&at) {
            *n -= 1;
            if *n == 0 {
                queued.remove(&at);
            }
        }
    }

    /// A read of `owner` with `key` was delivered or dropped: its key's record goes when no read
    /// with it is waiting any more.
    fn settled(&self, owner: Owner, key: Option<&str>) {
        let waiting = key.is_some_and(|k| self.waiting_with(owner, k));
        forget_settled(&mut self.key_seq.borrow_mut(), owner, key, waiting);
    }
}

/// Forgets the newest-ticket record of `owner`'s `key` once no read of that VM with that key is
/// waiting (`still_waiting` false). The record exists to answer `newer` for a delivery, and with
/// nothing left to deliver it answers nothing; kept, every distinct key a module ever used stayed
/// in the table for the module's life. Whatever order the answers of one key come back in, the
/// record stays while any of them is still to be delivered.
fn forget_settled(
    seqs: &mut HashMap<(usize, u64, String), TicketId>,
    owner: Owner,
    key: Option<&str>,
    still_waiting: bool,
) {
    if let (Some(k), false) = (key, still_waiting) {
        seqs.remove(&(owner.idx, owner.gen, k.to_string()));
    }
}

/// Whether an answer for `owner` may be called back: its VM is still the one that asked (not
/// reloaded, not rolled back), and its module is enabled — the rule the mailbox applies to every
/// event (`mailbox.rs`, `deliverable`), and [`deliverable_now`] asks of a holder. The tests ask it
/// with a map and a list of their own.
#[cfg(test)]
pub(crate) fn deliverable(owner: Owner, gens: &HashMap<usize, u64>, enabled: &[bool]) -> bool {
    gens.get(&owner.idx) == Some(&owner.gen) && enabled.get(owner.idx).copied().unwrap_or(false)
}

/// The regions of a slow job's line, `x,y wxh` each in screen coordinates: the first four, and how
/// many more. The rectangle is what tells reads from different modules apart as one question
/// asked several times — the Kontakt header read by five library modules at once, on a Mac — or
/// as several questions.
fn regions_text(rects: &[Rect]) -> String {
    const SHOWN: usize = 4;
    let mut parts: Vec<String> = rects.iter().take(SHOWN).map(|r| format!("{},{} {}x{}", r.x, r.y, r.w, r.h)).collect();
    if rects.len() > SHOWN {
        parts.push(format!("and {} more", rects.len() - SHOWN));
    }
    if parts.is_empty() {
        return "no region".to_string();
    }
    parts.join(", ")
}

/// Whether a newer read with `key` was asked for by `owner` after `ticket`.
fn newer_than(
    seqs: &HashMap<(usize, u64, String), TicketId>,
    owner: Owner,
    key: Option<&str>,
    ticket: TicketId,
) -> bool {
    key.is_some_and(|k| {
        seqs.get(&(owner.idx, owner.gen, k.to_string())).is_some_and(|newest| *newest > ticket)
    })
}

/// The `newer` a delivery carries: a newer read with its key was asked for since — or it is
/// stale, which a newer read is the only reason for.
fn delivered_newer(
    seqs: &HashMap<(usize, u64, String), TicketId>,
    owner: Owner,
    key: Option<&str>,
    ticket: TicketId,
    readings: &[Reading],
) -> bool {
    newer_than(seqs, owner, key, ticket) || readings.iter().any(|r| r.status == Status::Stale)
}

/// Records `ticket` as the newest read `owner` asked for with `key` — only once the queue took
/// it. A read refused at the call (the recogniser not answering, the whole application's queue
/// full) will never answer anything; counted as newer, it would mark the read still running
/// `newer`, and code that returns on `newer` would drop the only answer there is.
fn note_newest(
    seqs: &mut HashMap<(usize, u64, String), TicketId>,
    owner: Owner,
    key: Option<&str>,
    ticket: TicketId,
    taken: bool,
) {
    if let (Some(k), true) = (key, taken) {
        seqs.insert((owner.idx, owner.gen, k.to_string()), ticket);
    }
}

/// A window region whose client area was empty at the call was never read: its reading is
/// `"failed"` with that reason, whatever the queue made of the empty rectangle that stood in for
/// it — unless the whole read went stale, which is the more useful thing to know.
fn with_unresolved(mut readings: Vec<Reading>, unresolved: &[Option<String>]) -> Vec<Reading> {
    for (r, why) in readings.iter_mut().zip(unresolved) {
        if let Some(why) = why {
            if r.status != Status::Stale {
                r.status = Status::Failed;
                r.error = Some(why.clone());
                r.text.clear();
                r.rows.clear();
                r.words.clear();
            }
        }
    }
    readings
}

/// The VM a binding runs in, as `register_vm` tagged it — or, for a state it never tagged, the
/// binding's own index at that index's current generation (the image search's rule). Shared
/// with `snapshotAsync`.
pub(crate) fn owner_of(lua: &Lua, gens: &HashMap<usize, u64>, scope: usize) -> Owner {
    match vm_owner(lua) {
        Some(o) => Owner { idx: o.idx, gen: o.gen },
        None => Owner { idx: scope, gen: gens.get(&scope).copied().unwrap_or(0) },
    }
}

/// `host.ocr.recognize(what, opts?, cb)`, the callback form, called from the VM `lua` under the
/// identity `scope`: the shim's `submit` (`task.rs`).
pub(crate) fn read<H: ReadHost>(h: &H, lua: &Lua, scope: usize, what: Value, opts: Value, cb: Value) -> mlua::Result<()> {
    // `read(what, cb)`: the options are optional, the callback is not.
    let (opts, cb) = match (opts, cb) {
        (Value::Function(f), Value::Nil) => (Value::Nil, Value::Function(f)),
        other => other,
    };
    let cb: Function = match cb {
        Value::Function(f) => f,
        other => return Err(bad(format!("the callback (last argument) must be a function, got {}", other.type_name()))),
    };
    let args = parse_read(&what, &opts)?;
    let cb = lua.create_registry_value(cb)?;
    submit(h, lua, scope, args, current_priority(), Waiter::Callback(cb))?;
    Ok(())
}

/// Queues a read whose arguments passed, for `waiter`, at `prio`, and returns its ticket. Shared
/// by the callback form (`read`) and by a handler's wait (`task.rs`). What is decided without the threads — a read with
/// nothing to photograph, one refused, those it made stale or evicted — is answered on the next
/// tick, never from inside the call.
pub(crate) fn submit<H: ReadHost>(
    h: &H,
    lua: &Lua,
    scope: usize,
    mut args: ReadArgs,
    prio: Priority,
    waiter: Waiter,
) -> mlua::Result<TicketId> {
    // On a snapshot: each region cut to what of it the snapshot holds, before anything else
    // looks at them — a region it holds nothing of is answered like an unresolved one.
    let frame = args.snapshot.take().map(|s| s.0);
    if let Some(f) = &frame {
        args.entries = clip_to_snapshot(std::mem::take(&mut args.entries), f);
    }
    let owner = owner_of(lua, &h.vm_gens(), scope);
    let rects: Vec<Rect> = args.entries.iter().map(|(_, r)| r.clone().unwrap_or_default()).collect();
    let unresolved: Vec<Option<String>> = args.entries.iter().map(|(_, r)| r.clone().err()).collect();
    let nothing_to_read = unresolved.iter().all(Option::is_some);

    let st = h.ocr_state();
    let id = st.next_ticket.get() + 1;
    st.next_ticket.set(id);
    st.pending.borrow_mut().insert(
        id,
        PendingOcr {
            ticket: id,
            lua: lua.clone(),
            waiter,
            scope,
            owner,
            prio,
            names: args.entries.iter().map(|(n, _)| n.clone()).collect(),
            rects: rects.clone(),
            unresolved,
            list: args.list,
            key: args.key.clone(),
        },
    );
    if nothing_to_read {
        // Every region is a window region with no rectangle this time. Answered on the next tick
        // like any read, but without
        // the queue: no ticket against the module's limit or the application's, no picture, no
        // language to resolve, no capture source chosen — the cells calls answer the same case
        // without a capture too. With a key it is still the newest read: the module's waiting
        // reads with that key are stale, and one already recognising is delivered `newer`, as for
        // any later read.
        let stale = match args.key.as_deref() {
            Some(k) => h.ocr().supersede(owner, k),
            None => Vec::new(),
        };
        note_newest(&mut st.key_seq.borrow_mut(), owner, args.key.as_deref(), id, true);
        let mut ready = st.ready.borrow_mut();
        ready.extend(stale.into_iter().map(|s| (s, Status::Stale, None)));
        // `with_unresolved` gives each reading its own reason on delivery.
        ready.push((id, Status::Failed, None));
        return Ok(id);
    }
    let ticket = Ticket { id, owner, key: args.key.clone(), prio };
    let out = match frame {
        // Nothing to photograph and no source to choose: the snapshot is the picture, and a
        // read of it makes no first-read comparison.
        Some(f) => h.ocr().submit_on_frame(
            Spec { regions: rects, lang: args.lang, source: CaptureSource::Standard, compare: None },
            ticket,
            f,
        ),
        // The comparison, when it is owed, goes with the read and is made on the capture thread.
        None => {
            let first = rects.iter().find(|r| !r.is_empty()).copied().unwrap_or_default();
            let source = capture_source::vm_source(lua);
            let compare = capture_source::take_compare(lua, first.tuple());
            h.ocr().submit(Spec { regions: rects, lang: args.lang, source, compare }, ticket)
        }
    };
    // After the queue answered, and only when it took the read. Nothing is delivered before
    // the next tick, so no answer can be judged against a key recorded too late.
    note_newest(&mut st.key_seq.borrow_mut(), owner, args.key.as_deref(), id, out.refused.is_none());
    let crowded = out.refused.clone().or_else(|| (!out.evicted.is_empty()).then(|| TOO_MANY.to_string()));
    {
        let mut ready = st.ready.borrow_mut();
        for s in out.stale {
            ready.push((s, Status::Stale, None));
        }
        for e in out.evicted {
            ready.push((e, Status::Failed, Some(TOO_MANY.to_string())));
        }
        if let Some(why) = out.refused {
            ready.push((id, Status::Failed, Some(why)));
        }
    }
    if let Some(why) = crowded {
        note_crowding(h, owner.idx, &why);
    }
    Ok(id)
}

/// One line per module per `SLOW_LOG_EVERY` when its reads are evicted or refused.
fn note_crowding<H: ReadHost>(h: &H, idx: usize, why: &str) {
    let now = Instant::now();
    let mut logged = h.ocr_state().slow_logged.borrow_mut();
    let key = usize::MAX - idx; // a slot of its own beside the slow-job one
    if logged.get(&key).is_some_and(|t| now.duration_since(*t) < policy::SLOW_LOG_EVERY) {
        return;
    }
    logged.insert(key, now);
    logging::line(
        "ocr",
        &format!("[{}] a read was refused or evicted: {why} (said at most every 10 s)", h.module_id(idx)),
    );
}

/// How many callbacks a delivery ran and how many waiting handlers it resumed. The callbacks are
/// the `[pump]` line's share of text recognition — those that ran at once, their module free; one
/// that waits in its module's mailbox runs in the queued phase and is counted there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Delivered {
    pub callbacks: u32,
    pub tasks: u32,
}

/// Delivers what the threads finished and what was decided without them: each callback handed to
/// its module's mailbox — run at once if the module is free — and each handler waiting for one
/// resumed, in one order: first what was decided without the threads, in the order it was decided,
/// then the finished jobs, in the order they finished. First of all, the hang answer: a recogniser
/// that has answered nothing for `HANG` ends every read still out (`Service::hang_sweep`). Driven
/// by the loop tick, after the image results.
pub(crate) fn fire<H: MailHost>(h: &H) -> Delivered {
    let st = h.ocr_state();
    // Only while a read is out: with none, there is nothing to end, and the service's lock is
    // not taken every tick for nothing.
    if !st.pending.borrow().is_empty() {
        if let Some((tickets, why)) = h.ocr().hang_sweep() {
            st.ready.borrow_mut().extend(tickets.into_iter().map(|t| (t, Status::Failed, Some(why.clone()))));
        }
    }
    let ready = std::mem::take(&mut *st.ready.borrow_mut());
    let done: Vec<Done> = h.ocr().drain();
    let mut out = Delivered::default();
    for d in &done {
        log_job(h, d);
        if d.lang_error.is_some() {
            // Perhaps it was installed since the list was read.
            h.ocr().reread_languages();
        }
    }
    if ready.is_empty() && done.is_empty() {
        return out;
    }
    // Every delivery collected first, with nothing borrowed while a callback or a handler runs: it
    // may read again, disable a module or reload one.
    let mut deliveries: Vec<(PendingOcr, Vec<Reading>, Option<Picture>)> = Vec::new();
    {
        let mut pending = st.pending.borrow_mut();
        for (ticket, status, error) in ready {
            if let Some(p) = pending.remove(&ticket) {
                let readings = p.rects.iter().map(|r| Reading::outcome(*r, status, error.clone())).collect();
                deliveries.push((p, readings, None));
            }
        }
        for d in &done {
            for t in &d.tickets {
                if let Some(p) = pending.remove(&t.id) {
                    deliveries.push((p, d.readings.clone(), d.picture));
                }
            }
        }
    }
    if deliveries.is_empty() {
        return out;
    }
    // One epoch for the drain: every answer in it is a fresh look at the screen.
    h.bump_epoch();
    for (p, readings, picture) in deliveries {
        match p.waiter {
            // Judged when it runs (`open_read`): a key queued before it may read again.
            Waiter::Callback(_) => {
                let (idx, lua, prio) = (p.owner.idx, p.lua.clone(), p.prio);
                let _prio = enter_priority(prio);
                let ran = mailbox::deliver(h, idx, &lua, Event::Read(Box::new(ReadAnswer { p, readings, picture })));
                if matches!(ran, mailbox::Delivered::Ran | mailbox::Delivered::Parked) {
                    out.callbacks += 1;
                }
            }
            // The handler checks for itself whether it may still go on (`task::resume_wait`).
            Waiter::Handler { id, list } => {
                let readings = with_unresolved(readings, &p.unresolved);
                let seen = seen_of(picture, &p.unresolved, &readings);
                let newer = delivered_newer(&st.key_seq.borrow(), p.owner, p.key.as_deref(), p.ticket, &readings);
                match callback_args(&p.lua, &readings, &p.names, list, newer, &seen) {
                    Ok(values) => {
                        if crate::task::resume_wait(h, id, p.ticket, values) {
                            out.tasks += 1;
                        }
                    }
                    Err(e) => {
                        handover_failed(h, &p.lua, p.owner.idx, &e);
                        crate::task::end_waiting(h, id);
                    }
                }
                // After the handler's next stretch, which may have read again with the same key.
                st.settled(p.owner, p.key.as_deref());
            }
        }
    }
    out
}

/// Counts a callback's answer as still out while it waits in its module's mailbox (`mailbox.rs`,
/// as it queues it).
pub(crate) fn note_queued(st: &OcrState, r: &ReadAnswer) {
    *st.queued.borrow_mut().entry((r.p.owner.idx, r.p.owner.gen, r.p.key.clone())).or_insert(0) += 1;
}

/// A callback's answer, as it runs: whether a newer read with its key was asked is decided now —
/// after whatever ran before it in its module's mailbox — and the key's record goes once no read
/// with it is out any more. The call is built in the module's VM; the callback is let go here, as
/// the handler holds it.
pub(crate) fn open_read<H: ReadHost>(h: &H, r: ReadAnswer) -> Opened {
    let ReadAnswer { p, readings, picture } = r;
    let st = h.ocr_state();
    st.unqueue(&p);
    let readings = with_unresolved(readings, &p.unresolved);
    let seen = seen_of(picture, &p.unresolved, &readings);
    let newer = delivered_newer(&st.key_seq.borrow(), p.owner, p.key.as_deref(), p.ticket, &readings);
    st.settled(p.owner, p.key.as_deref());
    let Waiter::Callback(cb) = p.waiter else { return Opened::Gone };
    let f = p.lua.registry_value::<Function>(&cb);
    let _ = p.lua.remove_registry_value(cb);
    let run = f.and_then(|f| Ok((f, callback_args(&p.lua, &readings, &p.names, p.list, newer, &seen)?)));
    match run {
        Ok((f, args)) => Opened::Run { f, args, ctx: Ctx::new("ocr.recognize", p.scope) },
        Err(e) => {
            handover_failed(h, &p.lua, p.scope, &e);
            Opened::Gone
        }
    }
}

/// A reading that could not be built in its module's VM `lua`, for a waiting handler or a callback:
/// a VM that ran out of memory doing it is stopped, and the stop reports itself (`stops.rs`), as
/// any handler's memory error is; anything else is reported as the module's error, under `report`.
pub(crate) fn handover_failed<H: ReadHost>(h: &H, lua: &Lua, report: usize, e: &mlua::Error) {
    if !crate::vm_guard::note_failure(lua, crate::vm_guard::EntryKind::Handler, "ocr.recognize", e) {
        h.report_error(report, "ocr.recognize", &e.to_string());
    }
}

/// A callback's answer that will not run — its module was disabled, reloaded or rolled back while
/// it waited in the mailbox: no longer out, and let go.
pub(crate) fn discard_read<H: ReadHost>(h: &H, r: ReadAnswer) {
    let st = h.ocr_state();
    st.unqueue(&r.p);
    st.settled(r.p.owner, r.p.key.as_deref());
    if let Waiter::Callback(cb) = r.p.waiter {
        let _ = r.p.lua.remove_registry_value(cb);
    }
}

/// Whether an answer for `owner` may be handed over now ([`deliverable`], asked of `h`).
pub(crate) fn deliverable_now<H: ReadHost>(h: &H, owner: Owner) -> bool {
    h.vm_gens().get(&owner.idx) == Some(&owner.gen) && h.module_enabled(owner.idx)
}

/// A slow job's line, and a language that could not be met, once per module and request.
fn log_job<H: ReadHost>(h: &H, d: &Done) {
    let st = h.ocr_state();
    let owners: Vec<usize> = {
        let mut o: Vec<usize> = d.tickets.iter().map(|t| t.owner.idx).collect();
        o.sort_unstable();
        o.dedup();
        o
    };
    if let Some((req, why)) = &d.lang_error {
        for idx in &owners {
            if st.once(format!("lang\u{1}{idx}\u{1}{req:?}")) {
                logging::line("ocr", &format!("[{}] {why}", h.module_id(*idx)));
            }
        }
    }
    if d.timings.total() >= policy::SLOW_JOB.as_millis() as u64 {
        let now = Instant::now();
        let lang = d.readings.iter().map(|r| r.lang.as_str()).find(|l| !l.is_empty()).unwrap_or("-");
        // Worked out for the first line written, not for a job whose lines are all held back.
        let mut regions: Option<String> = None;
        for idx in &owners {
            let mut logged = st.slow_logged.borrow_mut();
            if logged.get(idx).is_some_and(|t| now.duration_since(*t) < policy::SLOW_LOG_EVERY) {
                continue;
            }
            logged.insert(*idx, now);
            logging::line(
                "ocr",
                &format!(
                    "{} region(s) for [{}] ({}) waited {} ms for the capture, which took {}, then {} ms \
                     for the recogniser, which took {} ms ({lang})",
                    d.readings.len(),
                    h.module_id(*idx),
                    regions.get_or_insert_with(|| {
                        regions_text(&d.readings.iter().map(|r| r.rect).collect::<Vec<Rect>>())
                    }),
                    d.timings.before_capture,
                    d.timings.capture,
                    d.timings.before_recognition,
                    d.timings.recognition
                ),
            );
        }
    }
}

/// Drops every read of module `idx`: disabled (`forget_keys` false), or reloaded and unloaded
/// (true, which also forgets which key it last asked with, all at once). Nothing of it is
/// called back, and no handler of it is resumed — its handlers themselves go in
/// `task::drop_owner`, and its answers waiting in its mailbox in `mailbox::drop_owner`, which
/// every caller of this calls beside it. With `false` too, each key's record goes with the
/// last read waiting with it (`forget_settled`), so a module switched off keeps none of its
/// reads' keys either.
pub(crate) fn drop_owner<H: ReadHost>(h: &H, idx: usize, forget_keys: bool) {
    h.ocr().cancel_owner(idx);
    let st = h.ocr_state();
    let gone: Vec<PendingOcr> = {
        let mut pending = st.pending.borrow_mut();
        let ids: Vec<TicketId> =
            pending.iter().filter(|(_, p)| p.owner.idx == idx).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| pending.remove(&id)).collect()
    };
    {
        let pending = st.pending.borrow();
        st.ready.borrow_mut().retain(|(t, ..)| pending.contains_key(t));
    }
    for p in gone {
        st.settled(p.owner, p.key.as_deref());
        if let Waiter::Callback(cb) = p.waiter {
            let _ = p.lua.remove_registry_value(cb);
        }
    }
    if forget_keys {
        st.key_seq.borrow_mut().retain(|(i, ..), _| *i != idx);
    }
}

/// `rollback_to(n)`'s share: every module from index `n` on.
pub(crate) fn drop_from<H: ReadHost>(h: &H, n: usize) {
    let mut owners: Vec<usize> =
        h.ocr_state().pending.borrow().values().map(|p| p.owner.idx).filter(|i| *i >= n).collect();
    owners.extend(h.ocr_state().key_seq.borrow().keys().map(|k| k.0).filter(|i| *i >= n));
    owners.sort_unstable();
    owners.dedup();
    for idx in owners {
        drop_owner(h, idx, true);
    }
}

/// Withdraws one read — the wait of a handler that was cancelled or ended: no longer delivered, and
/// off its job (`Service::withdraw`), which goes unless another call waits for it or it is being
/// recognised. Nothing happens for a ticket that is not waiting.
pub(crate) fn withdraw<H: ReadHost>(h: &H, ticket: TicketId) {
    let st = h.ocr_state();
    let Some(p) = st.pending.borrow_mut().remove(&ticket) else { return };
    st.ready.borrow_mut().retain(|(t, ..)| *t != ticket);
    h.ocr().withdraw(ticket);
    st.settled(p.owner, p.key.as_deref());
    if let Waiter::Callback(cb) = p.waiter {
        let _ = p.lua.remove_registry_value(cb);
    }
}

/// `host.ocr.pending(key)`: whether a read of the calling module's VM with `key` waits for its
/// answer — asked with a callback, or a handler stopped in `recognize` — or its answer waits in the
/// module's mailbox. False in its own callback and in the handler it resumes (the read is answered
/// by then), after a disable (its reads are dropped), and after the hang answer ended it. A key
/// that is not a non-empty string raises.
pub(crate) fn pending<H: ReadHost>(h: &H, lua: &Lua, scope: usize, key: Value) -> mlua::Result<bool> {
    let key = match key {
        Value::String(s) if !s.to_str()?.is_empty() => s.to_str()?.to_string(),
        Value::String(_) => return Err(mlua::Error::external("host.ocr.pending: the key is empty")),
        other => {
            return Err(mlua::Error::external(format!(
                "host.ocr.pending: the key must be a non-empty string, got {}",
                crate::json::luau_type(&other)
            )))
        }
    };
    let owner = owner_of(lua, &h.vm_gens(), scope);
    Ok(h.ocr_state().waiting_with(owner, &key))
}

impl Shared {
    /// Delivers what the threads finished and what was decided without them ([`fire`]).
    pub(crate) fn fire_ocr_results(&self) -> Delivered {
        fire(self)
    }

    /// Drops every read of module `idx` ([`drop_owner`]).
    pub(crate) fn ocr_drop_owner(&self, idx: usize, forget_keys: bool) {
        drop_owner(self, idx, forget_keys);
    }

    /// `rollback_to(n)`'s share ([`drop_from`]).
    pub(crate) fn ocr_drop_from(&self, n: usize) {
        drop_from(self, n);
    }

    /// `call` — `host.input.*` or `host.window.focus` — acts for the module that owns `lua`, at
    /// once: it never waits for a picture ([`note_acting`]).
    pub(crate) fn note_acting(&self, lua: &Lua, call: &str) {
        let Some(o) = vm_owner(lua) else { return };
        note_acting(self, o.idx, call, self.snap_state.has_pending_for(o.idx));
    }

    /// The language the blocking `recognize` hands the engine for `req`: what it resolves to in the
    /// published list, as a read's does — `Ok(None)`, the engine's own default, only while the list
    /// is not known yet and no tag was asked for, and the first tag as written while it is not known
    /// and one was — or `Err` with why nothing here reads it, which every region is answered
    /// `"failed"` with, said in the log once per request.
    pub(crate) fn ocr_legacy_lang(&self, req: &LangReq) -> Result<Option<String>, String> {
        let langs = known_langs(self);
        let resolved = legacy_lang(req, langs.as_ref());
        if let Err(why) = &resolved {
            self.ocr.reread_languages();
            if self.ocr_state.once(format!("legacy-lang\u{1}{req:?}")) {
                logging::line("ocr", &format!("host.ocr.recognize: {why}"));
            }
        }
        resolved
    }
}

/// The line a module's input is noted with, once a session, when it acts while a picture it asked
/// for is still to be taken ([`note_acting`]).
pub(crate) fn acting_line(id: &str, call: &str) -> String {
    format!(
        "[{id}] acted ({call}) while a picture it asked for was not taken yet; that picture may show \
         the input — act once the picture is answered: after the call returns in a handler, or in \
         its callback"
    )
}

/// Module `idx` acts through `call` (`host.input.*`, `host.window.focus`). Acting never waits for
/// a picture: a click made while the capture thread has yet to take one of the module's pictures
/// may be in it, as the picture's `inputEpoch` tells for the input that turns it over. While one
/// of its reads is out — or, with `snaps_out`, one of its snapshot requests — the thread is asked
/// whether such a picture is still to be taken, one meant to show the screen as it was at the call
/// (`Service::picture_due`); when it is, the log says so, once a session per module. True when it
/// did.
pub(crate) fn note_acting<H: ReadHost>(h: &H, idx: usize, call: &str, snaps_out: bool) -> bool {
    let reads_out = h.ocr_state().pending.borrow().values().any(|p| p.owner.idx == idx);
    if !(reads_out || snaps_out) || !h.ocr().picture_due(idx) || !h.ocr_state().once(format!("acted\u{1}{idx}")) {
        return false;
    }
    logging::line("ocr", &acting_line(&h.module_id(idx), call));
    true
}

/// The once-a-session line for a question about the languages asked before they are known.
const LANGS_NOT_KNOWN: &str =
    "the recognition languages are not known yet; answering from what is known, which is nothing";

/// The languages the recognise thread published, as they stand: never waited for. `None` before
/// they are published — the thread's first act, so only in the first moments after start — said
/// in the log once a session. A read the recogniser answers comes after it: its language is
/// resolved on that thread, after the list.
fn known_langs<H: ReadHost>(h: &H) -> Option<lang::Languages> {
    let l = h.ocr().languages(std::time::Duration::ZERO);
    if l.is_none() && h.ocr_state().once("langs-late".to_string()) {
        logging::line("ocr", LANGS_NOT_KNOWN);
    }
    l
}

/// `host.ocr.languages()`: the published list, the default first; `{}` before it is published.
pub(crate) fn languages_value<H: ReadHost>(h: &H, lua: &Lua) -> mlua::Result<Table> {
    let list = known_langs(h).map(|l| lang::listed(&l)).unwrap_or_default();
    let t = lua.create_table_with_capacity(list.len(), 0)?;
    for tag in list {
        t.raw_push(tag)?;
    }
    Ok(t)
}

/// `host.ocr.resolveLanguage(tag | {tag} | nil)`: `nil` for a language nothing here reads, and
/// before the list is published.
pub(crate) fn resolve_value<H: ReadHost>(h: &H, v: &Value) -> mlua::Result<Option<String>> {
    let req = lang_request(v, "host.ocr.resolveLanguage")?;
    let Some(langs) = known_langs(h) else { return Ok(None) };
    match lang::resolve(&req, &langs) {
        Ok(tag) => Ok(Some(tag)),
        Err(_) => {
            h.ocr().reread_languages();
            Ok(None)
        }
    }
}

/// [`Shared::ocr_legacy_lang`]'s rule, given the published list (`None` while it is not known).
pub(crate) fn legacy_lang(req: &LangReq, langs: Option<&lang::Languages>) -> Result<Option<String>, String> {
    match (langs, req) {
        (Some(langs), _) => lang::resolve(req, langs).map(Some),
        (None, LangReq::Default) => Ok(None),
        (None, LangReq::Tags(tags)) => Ok(tags.first().cloned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lua: &Lua, what: &str, opts: &str) -> mlua::Result<ReadArgs> {
        let w: Value = lua.load(what).eval().unwrap();
        let o: Value = lua.load(opts).eval().unwrap();
        parse_read(&w, &o)
    }

    fn fails(lua: &Lua, what: &str, opts: &str, needle: &str) {
        match parse(lua, what, opts) {
            Ok(a) => panic!("{what} / {opts} was accepted: {a:?}"),
            Err(e) => assert!(e.to_string().contains(needle), "{what} / {opts}: {e}"),
        }
    }

    /// The slow job's line names what was read: four regions at most, then how many more.
    #[test]
    fn a_slow_jobs_line_names_its_regions() {
        assert_eq!(regions_text(&[Rect::new(697, 111, 173, 45)]), "697,111 173x45");
        let five: Vec<Rect> = (0..5).map(|i| Rect::new(i * 10, -5, 40, 20)).collect();
        assert_eq!(regions_text(&five), "0,-5 40x20, 10,-5 40x20, 20,-5 40x20, 30,-5 40x20, and 1 more");
        assert_eq!(regions_text(&[]), "no region");
    }

    #[test]
    fn the_region_entry_and_list_forms() {
        let lua = Lua::new();
        let a = parse(&lua, "return { 10, 20, 110, 40 }", "return nil").unwrap();
        assert_eq!(a.entries, vec![(None, Ok(Rect::new(10, 20, 100, 20)))]);
        assert!(!a.list);
        // The Region form's one strict rule (region_lua.rs), shared with the cells calls: a
        // fraction of a pixel, no corners at all and turned-around corners are mistakes.
        fails(&lua, "return { x1 = 10.9, y1 = 20, x2 = 110, y2 = 40 }", "return nil", "the region.x1 must be a whole number, got 10.9");
        fails(&lua, "return {}", "return nil", "the region.x1 is missing");
        let a = parse(&lua, "return { x1 = 10.0, y1 = 20, x2 = 110, y2 = 40 }", "return nil").unwrap();
        assert_eq!(a.entries[0].1, Ok(Rect::new(10, 20, 100, 20)), "a whole float is a whole number");
        let a = parse(&lua, "return { region = { 1, 2, 3, 4 }, name = 'v' }", "return nil").unwrap();
        assert_eq!(a.entries, vec![(Some("v".into()), Ok(Rect::new(1, 2, 2, 2)))]);
        assert!(!a.list);
        let a = parse(
            &lua,
            "return { { region = { 0, 0, 10, 10 }, name = 'item' }, { 20, 0, 30, 10 } }",
            "return { lang = { 'de', 'en' }, key = 'menu' }",
        )
        .unwrap();
        assert!(a.list);
        assert_eq!(a.entries.len(), 2);
        assert_eq!(a.entries[1], (None, Ok(Rect::new(20, 0, 10, 10))));
        assert_eq!(a.lang, LangReq::Tags(vec!["de".into(), "en".into()]));
        assert_eq!(a.key.as_deref(), Some("menu"));
        let a = parse(&lua, "return { { 0, 0, 10, 10 } }", "return nil").unwrap();
        assert!(a.list, "a list of one is still a list");
        fails(&lua, "return { 50, 50, 10, 10 }", "return nil", "the region { 50, 50, 10, 10 } is empty or turned around");
        fails(&lua, "return { { 0, 0, 1, 1 }, { region = { 5, 0, 5, 1 } } }", "return nil", "entry 2.region { 5, 0, 5, 1 } is empty");
    }

    /// The window-relative form, as bare region, entry and list item, resolved at the call with
    /// the same formula as the cells calls. An empty client area is not a mistake: that region
    /// has no rectangle, and its reading will say why.
    #[test]
    fn the_window_form_is_resolved_at_the_call() {
        let lua = Lua::new();
        let win = "local w = { client = { x = 100, y = 50, w = 1280, h = 1024 } } ";
        let a = parse(&lua, &format!("{win} return {{ window = w, fraction = {{ 0.02, 0.33, 0.09, 0.86 }} }}"), "return nil").unwrap();
        assert_eq!(a.entries, vec![(None, Ok(Rect::new(125, 387, 91, 544)))]);
        assert!(!a.list);
        let a = parse(
            &lua,
            &format!(
                "{win} return {{ {{ name = 'menu', region = {{ window = w, fraction = {{ x1 = 0, y1 = 0, x2 = 0.5, y2 = 1 }} }} }}, \
                 {{ window = {{ client = {{ x = 0, y = 0, w = 0, h = 600 }} }}, fraction = {{ 0, 0, 1, 1 }} }}, {{ 0, 0, 10, 10 }} }}"
            ),
            "return nil",
        )
        .unwrap();
        assert!(a.list);
        assert_eq!(a.entries[0], (Some("menu".into()), Ok(Rect::new(100, 50, 640, 1024))));
        assert_eq!(a.entries[1], (None, Err("the window's client area is empty (0x600)".to_string())));
        assert_eq!(a.entries[2], (None, Ok(Rect::new(0, 0, 10, 10))));
        fails(&lua, &format!("{win} return {{ window = w, fraction = {{ 0, 0, 1/0, 1 }} }}"), "return nil", "the region.fraction.x2 is inf");
        fails(&lua, &format!("{win} return {{ window = w }}"), "return nil", "the region.fraction must be a table");
        fails(&lua, "return { region = { window = 5, fraction = { 0, 0, 1, 1 } } }", "return nil", "the entry.region.window must be a window table");
    }

    /// The pixel limit: corners past it raise, as the module wrote them; a window region is
    /// counted at the window's size, after the corners and in order, and one that would go past
    /// is not read — answered, not raised — while a later one that fits still is.
    #[test]
    fn a_window_region_past_the_pixel_limit_is_answered_not_raised() {
        let lua = Lua::new();
        // 8000x4000 is 32 million pixels; the limit is 40 million.
        let big = "{ window = { client = { x = 0, y = 0, w = 8000, h = 4000 } }, fraction = { 0, 0, 1, 1 } }";
        let half = "{ window = { client = { x = 0, y = 0, w = 8000, h = 4000 } }, fraction = { 0, 0, 0.5, 0.5 } }";
        let a = parse(&lua, &format!("return {{ {big}, {big}, {half} }}"), "return nil").unwrap();
        assert_eq!(a.entries[0].1, Ok(Rect::new(0, 0, 8000, 4000)));
        let e = a.entries[1].1.clone().unwrap_err();
        assert!(e.contains("at the window's current size this region is 8000x4000") && e.contains("40000000"), "{e}");
        assert_eq!(a.entries[2].1, Ok(Rect::new(0, 0, 4000, 2000)), "8 million still fit after the 32");
        // Corners are counted first, whatever their place in the list.
        let a = parse(&lua, &format!("return {{ {big}, {{ 0, 0, 5000, 2000 }} }}"), "return nil").unwrap();
        assert!(a.entries[0].1.is_err(), "10 million of corners leave room for 30, not 32");
        assert_eq!(a.entries[1].1, Ok(Rect::new(0, 0, 5000, 2000)));
        // Corners past it raise; so do three of the largest rectangles, whose sum overflows.
        fails(&lua, "return { { 0, 0, 8000, 4000 }, { 0, 0, 8000, 4000 } }", "return nil", "64000000 pixels in one call");
        let huge = "{ -1000000000, -1000000000, 1000000000, 1000000000 }";
        fails(&lua, &format!("return {{ {huge}, {huge}, {huge} }}"), "return nil", "9223372036854775807 pixels in one call");
    }

    /// The reading of a window region that had no rectangle fails with the reason; a stale one
    /// stays stale, and the other regions keep what the queue answered.
    #[test]
    fn an_unresolved_window_region_is_answered_failed_with_the_reason() {
        let why = "the window's client area is empty (0x600)".to_string();
        let readings = vec![
            Reading::failed(Rect::default(), "empty region: x2 must be greater than x1 and y2 greater than y1"),
            Reading::outcome(Rect::new(0, 0, 10, 10), Status::Text, None),
        ];
        let out = with_unresolved(readings, &[Some(why.clone()), None]);
        assert_eq!((out[0].status, out[0].error.as_deref()), (Status::Failed, Some(why.as_str())));
        assert_eq!((out[1].status, out[1].error.as_deref()), (Status::Text, None));
        let stale = vec![Reading::outcome(Rect::default(), Status::Stale, None)];
        let out = with_unresolved(stale, &[Some(why)]);
        assert_eq!((out[0].status, out[0].error.as_deref()), (Status::Stale, None));
    }

    #[test]
    fn mistakes_in_the_call_raise() {
        let lua = Lua::new();
        fails(&lua, "return 'x'", "return nil", "first argument");
        fails(&lua, "return { 1, 2, 3 }", "return nil", "all four corners");
        fails(&lua, "return { x1 = 1, y1 = 2, x2 = 3 }", "return nil", "all four corners");
        fails(&lua, "return { 1, 2, '3', 4 }", "return nil", "must be a whole number, got the string");
        fails(&lua, "return { 1, 2, 0/0, 4 }", "return nil", "must be a whole number, got NaN");
        fails(&lua, "return { x1 = 1, 2, 3, 4, 5 }", "return nil", "mixes");
        fails(&lua, "return { x1 = 0, y1 = 0, x2 = 1, y2 = 1, colour = 3 }", "return nil", "has a key 'colour'");
        fails(&lua, "return { region = { 0, 0, 1, 1 }, label = 'x' }", "return nil", "unknown field 'label'");
        fails(&lua, "return { region = 'here' }", "return nil", "must be a table");
        fails(&lua, "return { { 0, 0, 1, 1 }, 5 }", "return nil", "entry 2 must be a table");
        fails(
            &lua,
            "return { { region = { 0, 0, 1, 1 }, name = 'a' }, { region = { 1, 1, 2, 2 }, name = 'a' } }",
            "return nil",
            "used twice",
        );
        fails(&lua, "local t = {} for i = 1, 65 do t[i] = { 0, 0, 1, 1 } end return t", "return nil", "at most 64");
        fails(&lua, "return { 0, 0, 10000, 5000 }", "return nil", "pixels in one call");
        fails(&lua, "return { 0, 0, 1, 1 }", "return { langs = 'de' }", "unknown option 'langs'");
        fails(&lua, "return { 0, 0, 1, 1 }", "return { lang = 'deutsch' }", "not a language tag");
        fails(&lua, "return { 0, 0, 1, 1 }", "return { lang = { 'de', 5 } }", "must be a string");
        fails(&lua, "return { 0, 0, 1, 1 }", "return { key = 5 }", "`key` must be a string");
        fails(&lua, "return { 0, 0, 1, 1 }", "return 'de'", "options must be a table");
    }

    /// `snapshot` is the third option: a Snapshot, anything else raising, and named in the
    /// message for an unknown option.
    #[test]
    fn the_snapshot_option_is_accepted_and_named_in_the_unknown_option_message() {
        let lua = Lua::new();
        let h = crate::snapshot::test_handle(&lua, crate::snapshot::test_frame(0, 0, 50, 40));
        lua.globals().set("s", h).unwrap();
        let a = parse(&lua, "return { 0, 0, 10, 10 }", "return { snapshot = s, key = 'menu' }").unwrap();
        assert!(a.snapshot.is_some() && a.key.as_deref() == Some("menu"));
        assert!(parse(&lua, "return { 0, 0, 10, 10 }", "return nil").unwrap().snapshot.is_none());
        fails(&lua, "return { 0, 0, 1, 1 }", "return { snap = s }", "unknown option 'snap' — the options are `lang`, `key` and `snapshot`");
        fails(&lua, "return { 0, 0, 1, 1 }", "return { snapshot = 5 }", "host.ocr.recognize: opts.snapshot must be a Snapshot, got 5");
        lua.load("s:release()").exec().unwrap();
        fails(&lua, "return { 0, 0, 1, 1 }", "return { snapshot = s }", "host.ocr.recognize: opts.snapshot: the snapshot was released");
    }

    /// On a snapshot each region reads the part the snapshot holds; one it holds nothing of fails
    /// with the reason, and an unresolved window region keeps its own.
    #[test]
    fn regions_are_clipped_to_the_snapshot_and_no_overlap_fails_that_region() {
        let frame = crate::snapshot::test_frame(100, 100, 200, 100);
        let entries = vec![
            (Some("inside".to_string()), Ok(Rect::new(120, 110, 20, 10))),
            (None, Ok(Rect::new(250, 150, 100, 100))),
            (None, Ok(Rect::new(0, 0, 50, 50))),
            (None, Err("the window's client area is empty (0x0)".to_string())),
        ];
        let out = clip_to_snapshot(entries, &frame);
        assert_eq!(out[0], (Some("inside".to_string()), Ok(Rect::new(120, 110, 20, 10))));
        assert_eq!(out[1].1, Ok(Rect::new(250, 150, 50, 50)), "cut to the snapshot");
        assert_eq!(out[2].1, Err(crate::snapshot::NO_OVERLAP.to_string()));
        assert_eq!(out[3].1, Err("the window's client area is empty (0x0)".to_string()));
    }

    /// Delivered only to the VM that asked, while its module is enabled: a reload (a new
    /// generation), a rollback (no generation) or a disable drops the answer.
    #[test]
    fn an_answer_goes_only_to_the_enabled_vm_that_asked() {
        let owner = Owner { idx: 1, gen: 7 };
        let gens: HashMap<usize, u64> = [(1, 7)].into_iter().collect();
        assert!(deliverable(owner, &gens, &[true, true]));
        assert!(!deliverable(owner, &gens, &[true, false]), "disabled");
        assert!(!deliverable(Owner { idx: 1, gen: 6 }, &gens, &[true, true]), "reloaded since");
        assert!(!deliverable(Owner { idx: 3, gen: 7 }, &gens, &[true, true, true, true]), "rolled back");
        assert!(!deliverable(owner, &gens, &[true]), "no longer loaded");
    }

    #[test]
    fn newer_means_a_later_read_with_the_same_key_from_the_same_vm() {
        let owner = Owner { idx: 1, gen: 7 };
        let seqs: HashMap<(usize, u64, String), TicketId> =
            [((1, 7, "menu".to_string()), 12)].into_iter().collect();
        assert!(newer_than(&seqs, owner, Some("menu"), 10));
        assert!(!newer_than(&seqs, owner, Some("menu"), 12), "it is the newest itself");
        assert!(!newer_than(&seqs, owner, Some("other"), 10));
        assert!(!newer_than(&seqs, owner, None, 10), "no key, never newer");
        assert!(!newer_than(&seqs, Owner { idx: 1, gen: 8 }, Some("menu"), 10), "another VM");
    }

    /// A stale reading is always `newer`, whatever the key table says; any other only when a
    /// later read with its key was taken.
    #[test]
    fn a_stale_delivery_is_newer_and_another_only_when_a_later_read_was_taken() {
        let owner = Owner { idx: 1, gen: 7 };
        let region = Rect::new(0, 0, 10, 10);
        let stale = [Reading::outcome(region, Status::Stale, None)];
        let text = [Reading::outcome(region, Status::Text, None)];
        let mut seqs = HashMap::new();
        assert!(delivered_newer(&seqs, owner, Some("menu"), 5, &stale));
        assert!(!delivered_newer(&seqs, owner, Some("menu"), 5, &text));
        note_newest(&mut seqs, owner, Some("menu"), 6, true);
        assert!(delivered_newer(&seqs, owner, Some("menu"), 5, &text));
    }

    /// A key's record stays while any read with it waits, and goes with the last one, in any
    /// order the answers come back — so the table holds only the keys in use.
    #[test]
    fn a_keys_record_goes_with_the_last_read_waiting_with_it() {
        let owner = Owner { idx: 1, gen: 7 };
        let mut seqs = HashMap::new();
        note_newest(&mut seqs, owner, Some("row 12"), 5, true);
        note_newest(&mut seqs, owner, Some("row 12"), 6, true);
        // The newest answered first while the older one still waits: kept, so the older one is
        // still delivered `newer`.
        forget_settled(&mut seqs, owner, Some("row 12"), true);
        assert!(newer_than(&seqs, owner, Some("row 12"), 5));
        // The older one answered, nothing waits with the key: forgotten.
        forget_settled(&mut seqs, owner, Some("row 12"), false);
        assert!(seqs.is_empty());
        // A read without a key has no record to forget; another VM's record is its own.
        note_newest(&mut seqs, Owner { idx: 1, gen: 8 }, Some("row 12"), 9, true);
        forget_settled(&mut seqs, owner, None, false);
        forget_settled(&mut seqs, owner, Some("row 12"), false);
        assert_eq!(seqs.len(), 1);
    }

    /// A read the queue refused never answers: it must not make the one still running `newer`,
    /// or code that returns on `newer` hears nothing at all.
    #[test]
    fn a_refused_read_does_not_make_the_running_one_newer() {
        let owner = Owner { idx: 1, gen: 7 };
        let mut seqs = HashMap::new();
        note_newest(&mut seqs, owner, Some("menu"), 5, true);
        note_newest(&mut seqs, owner, Some("menu"), 6, false); // refused: the recogniser hangs
        assert!(!newer_than(&seqs, owner, Some("menu"), 5));
        note_newest(&mut seqs, owner, None, 7, true);
        assert_eq!(seqs.len(), 1, "a read without a key is not recorded");
    }

    /// The blocking `recognize`'s language: resolved in the published list as a read's is, or
    /// answered as unavailable; while the list is not known, the first tag as written, and the
    /// engine's own default for none.
    #[test]
    fn the_blocking_calls_lang_goes_through_the_resolver() {
        let langs = lang::Languages {
            available: vec!["en-US".into(), "de-DE".into()],
            fast: vec![],
            preferred: vec!["de-DE".into()],
        };
        let tags = |t: &[&str]| LangReq::Tags(t.iter().map(|s| s.to_string()).collect());
        assert_eq!(legacy_lang(&tags(&["de"]), None), Ok(Some("de".to_string())));
        assert_eq!(legacy_lang(&tags(&["ja", "de"]), None), Ok(Some("ja".to_string())), "the first, as written");
        assert_eq!(legacy_lang(&LangReq::Default, None), Ok(None), "the engine's own default");
        assert_eq!(legacy_lang(&tags(&["de"]), Some(&langs)), Ok(Some("de-DE".to_string())));
        assert_eq!(legacy_lang(&tags(&["ja", "en"]), Some(&langs)), Ok(Some("en-US".to_string())), "the first that is there");
        assert_eq!(legacy_lang(&LangReq::Default, Some(&langs)), Ok(Some("de-DE".to_string())), "the user's language");
        let why = legacy_lang(&tags(&["ja"]), Some(&langs)).unwrap_err();
        assert!(why.starts_with("language: ja is not available here"), "{why}");
    }

    #[test]
    fn a_list_is_a_plain_array_and_by_name_indexes_it() {
        let lua = Lua::new();
        let readings = vec![
            Reading::outcome(Rect::new(0, 0, 10, 10), Status::None, None),
            Reading::outcome(Rect::new(10, 0, 10, 10), Status::Blank, None),
        ];
        let names = vec![Some("item".to_string()), None];
        let args = callback_args(&lua, &readings, &names, true, false, &[]).unwrap();
        let f: Function = lua
            .load(
                r#"return function(list, byName)
                  local keys = 0
                  for k in pairs(list) do
                    if type(k) ~= "number" then return false end
                    keys = keys + 1
                  end
                  return keys == 2 and #list == 2 and byName.item == list[1]
                    and list[2].name == nil and list[2].status == "blank"
                end"#,
            )
            .eval()
            .unwrap();
        assert!(f.call::<bool>(args).unwrap());
        // A single region: one reading, no second argument.
        let args = callback_args(&lua, &readings[..1], &[None], false, true, &[]).unwrap();
        assert_eq!(args.len(), 1);
        let f: Function = lua
            .load(r#"return function(r, extra) return r.status == "none" and r.newer == true and extra == nil end"#)
            .eval()
            .unwrap();
        assert!(f.call::<bool>(args).unwrap());
    }

    #[test]
    fn a_reading_is_plain_data_with_the_documented_fields() {
        let lua = Lua::new();
        let r = super::super::pipeline::normalise(
            Rect::new(10, 20, 100, 20),
            super::super::types::EngineOut::Fallback { text: "7".into(), content: Rect::new(2, 2, 6, 10) },
            "en-US",
        );
        let t = reading_table(&lua, &r, Some("value"), true, Some(Seen { time: 1234, input_epoch: 7 })).unwrap();
        lua.globals().set("r", t).unwrap();
        let ok: bool = lua
            .load(
                r#"
                return r.name == "value" and r.status == "text" and r.skipped == false and r.text == "7" and r.newer == true
                  and r.x == 10 and r.w == 100 and r.lang == "en-US" and r.error == nil
                  and #r.words == 1 and r.words[1].approx == true and r.words[1].x == 12
                  and #r.lines == 1 and r.lines[1].text == "7" and #r.lines[1].words == 1
                  and r.time == 1234 and r.inputEpoch == 7
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
        let failed = Reading::failed(Rect::new(0, 0, 1, 1), "screen capture failed");
        let t = reading_table(&lua, &failed, None, false, None).unwrap();
        lua.globals().set("f", t).unwrap();
        let ok: bool = lua
            .load(r#"return f.name == nil and f.status == "failed" and f.skipped == false and f.text == "" and #f.words == 0 and f.error == "screen capture failed" and f.approx == nil and f.time == nil and f.inputEpoch == nil"#)
            .eval()
            .unwrap();
        assert!(ok);
        // A stale reading never says when, whatever it is handed: it was never recognised.
        let stale = Reading::outcome(Rect::new(0, 0, 1, 1), Status::Stale, None);
        let t = reading_table(&lua, &stale, None, true, Some(Seen { time: 1, input_epoch: 1 })).unwrap();
        lua.globals().set("s", t).unwrap();
        assert!(lua.load("return s.time == nil and s.inputEpoch == nil and #s.words == 0").eval::<bool>().unwrap());
    }

    /// Which readings of a delivery say when their picture was taken: those of a job with a
    /// picture that read the screen — `"text"`, `"blank"`, `"none"` — and no other: not a window
    /// region that was never read, not a region whose own capture failed inside a picture that
    /// was taken, not a stale one; and none of an answer decided without the threads.
    #[test]
    fn a_reading_says_when_only_for_a_picture_it_was_read_from() {
        let picture = Some(Picture { at: Instant::now(), input_epoch: 3 });
        let readings = vec![
            Reading::outcome(Rect::new(0, 0, 10, 10), Status::Text, None),
            Reading::outcome(Rect::default(), Status::Failed, Some("the window's client area is empty (0x0)".into())),
            Reading::outcome(Rect::new(20, 0, 10, 10), Status::Stale, None),
            Reading::outcome(Rect::new(30, 0, 10, 10), Status::Failed, Some("screen capture failed".into())),
            Reading::outcome(Rect::new(40, 0, 10, 10), Status::Blank, None),
            Reading::outcome(Rect::new(50, 0, 10, 10), Status::None, None),
        ];
        let unresolved = vec![None, Some("the window's client area is empty (0x0)".to_string()), None, None, None, None];
        let seen = seen_of(picture, &unresolved, &readings);
        assert_eq!(seen[0].map(|s| s.input_epoch), Some(3));
        assert_eq!((seen[1], seen[2], seen[3]), (None, None, None));
        assert!(seen[4].is_some() && seen[5].is_some(), "blank and none read the screen");
        assert!(seen_of(None, &unresolved, &readings).iter().all(Option::is_none), "refused: no picture");
    }

    /// Every reading carries `skipped`, which is `status == "blank"` — the callback's and the
    /// wait's alike, since both get [`callback_args`]'s values.
    #[test]
    fn every_reading_says_whether_it_was_skipped() {
        let lua = Lua::new();
        let readings = vec![
            Reading::outcome(Rect::new(0, 0, 10, 10), Status::Blank, None),
            Reading::outcome(Rect::new(10, 0, 10, 10), Status::None, None),
        ];
        let one = callback_args(&lua, &readings[..1], &[None], false, false, &[]).unwrap();
        let many = callback_args(&lua, &readings, &[None, None], true, true, &[]).unwrap();
        lua.globals().set("one", one.into_iter().next().unwrap()).unwrap();
        lua.globals().set("many", many.into_iter().next().unwrap()).unwrap();
        assert!(lua
            .load(
                r#"return one.status == "blank" and one.skipped == true and one.newer == false and one.name == nil
                  and #many == 2 and many[1].skipped == true and many[2].skipped == false and many[2].x == 10
                  and many[2].newer == true"#
            )
            .eval::<bool>()
            .unwrap());
    }
}
