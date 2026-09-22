//! The Luau side of `host.ocr.read`, `languages` and `resolveLanguage`: strict argument
//! parsing, the pending callbacks, and delivery on the event loop.
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

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::image_search::vm_owner;
use crate::region::Region;
use crate::{call_guarded, capture_source, logging, region_lua, Shared};

use super::lang::{self, LangReq};
use super::policy::{self, MAX_CALL_PIXELS, MAX_REGIONS};
use super::sched::{Owner, Ticket, TicketId, TOO_MANY};
use super::service::{Done, Spec};
use super::types::{current_priority, enter_priority, Priority, Reading, Rect, Status};

/// What a `host.ocr.read` call asked for, once its arguments passed.
#[derive(Debug, PartialEq)]
pub(crate) struct ReadArgs {
    /// Each region with its name, in the order given: its rectangle, or — a window region whose
    /// client area is empty at the call, or that would take the call past the pixel limit — why
    /// it has none, which its reading will carry.
    pub entries: Vec<(Option<String>, Result<Rect, String>)>,
    /// A list was given (even of one): the callback gets `(list, byName)`.
    pub list: bool,
    pub lang: LangReq,
    pub key: Option<String>,
}

fn bad(msg: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::external(format!("host.ocr.read: {msg}"))
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
/// region whose client area is empty.
fn resolve_all(
    regions: Vec<(Option<String>, Region)>,
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
        return Err(bad(format!("{corners} pixels in one call; at most {MAX_CALL_PIXELS}")));
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

/// Every argument of `host.ocr.read(what, opts?, cb)` but the callback, checked.
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
    let entries = resolve_all(regions)?;
    let mut seen = HashSet::new();
    for (name, _) in &entries {
        if let Some(n) = name {
            if !seen.insert(n.clone()) {
                return Err(bad(format!("the name '{n}' is used twice")));
            }
        }
    }

    let (mut lang, mut key) = (LangReq::Default, None);
    match opts {
        Value::Nil => {}
        Value::Table(o) => {
            for pair in o.clone().pairs::<Value, Value>() {
                let (k, v) = pair?;
                let name = match &k {
                    Value::String(s) => s.to_str()?.to_string(),
                    _ => return Err(bad("options have only the fields `lang` and `key`")),
                };
                match name.as_str() {
                    "lang" => lang = lang_request(&v, "host.ocr.read")?,
                    "key" => match v {
                        Value::String(s) if !s.to_str()?.is_empty() => key = Some(s.to_str()?.to_string()),
                        Value::String(_) => return Err(bad("`key` is empty")),
                        other => return Err(bad(format!("`key` must be a string, got {}", other.type_name()))),
                    },
                    other => {
                        return Err(bad(format!(
                            "unknown option '{other}' — the options are `lang` and `key`"
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
    Ok(ReadArgs { entries, list: is_list, lang, key })
}

/// One reading as the table a callback receives.
pub(crate) fn reading_table(lua: &Lua, r: &Reading, name: Option<&str>, newer: bool) -> mlua::Result<Table> {
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
    Ok(t)
}

/// What a callback is called with: one reading for a single region or entry, and for a list
/// `(list, byName)` — `list` a plain array in the order asked, `byName` the named readings.
/// Two tables rather than one array with names beside the indices, so the list stays plain data
/// that `host.json.encode` or an export passes on unchanged.
pub(crate) fn callback_args(
    lua: &Lua,
    readings: &[Reading],
    names: &[Option<String>],
    list: bool,
    newer: bool,
) -> mlua::Result<mlua::MultiValue> {
    let array = lua.create_table_with_capacity(readings.len(), 0)?;
    let by_name = lua.create_table()?;
    for (i, r) in readings.iter().enumerate() {
        let name = names.get(i).and_then(|n| n.as_deref());
        let t = reading_table(lua, r, name, newer)?;
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

/// A read waiting for its answer. Main thread only.
pub(crate) struct PendingOcr {
    /// Its own ticket: tickets are handed out in order, so a larger one with the same key is a
    /// newer read.
    ticket: TicketId,
    lua: Lua,
    cb: RegistryKey,
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

/// Everything `host.ocr.read` keeps on the event loop.
#[derive(Default)]
pub(crate) struct OcrState {
    pending: RefCell<HashMap<TicketId, PendingOcr>>,
    /// Answers decided without the threads — stale, evicted, refused — delivered on the next
    /// tick, never from inside the binding.
    ready: RefCell<Vec<(TicketId, Status, Option<String>)>>,
    /// (module index, VM generation, key) → the newest ticket asked with that key.
    key_seq: RefCell<HashMap<(usize, u64, String), TicketId>>,
    next_ticket: Cell<TicketId>,
    /// Per module: when a slow job was last logged.
    slow_logged: RefCell<HashMap<usize, Instant>>,
    /// Log lines said once per session.
    said: RefCell<HashSet<String>>,
}

impl OcrState {
    /// Whether any read is waiting — a reason for the headless loop to run.
    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.borrow().is_empty() || !self.ready.borrow().is_empty()
    }

    fn once(&self, key: String) -> bool {
        self.said.borrow_mut().insert(key)
    }
}

/// Whether an answer for `owner` may be called back: its VM is still the one that asked (not
/// reloaded, not rolled back), and its module is enabled.
fn deliverable(owner: Owner, gens: &HashMap<usize, u64>, enabled: &[bool]) -> bool {
    gens.get(&owner.idx) == Some(&owner.gen) && enabled.get(owner.idx).copied().unwrap_or(false)
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

/// What the `lang` of `recognize` / `recognizeMany` becomes, given the published list (`None`
/// while it is not known yet): `Err` — a malformed tag, raised; `Ok(Ok(tag))` — the tag to hand
/// the engine, which is the tag as written while the list is not known, as these two calls
/// always did; `Ok(Err(why))` — nothing here reads it.
pub(crate) fn legacy_lang(
    tag: &str,
    langs: Option<&lang::Languages>,
) -> Result<Result<String, String>, String> {
    lang::parse(tag)?;
    let Some(langs) = langs else { return Ok(Ok(tag.to_string())) };
    Ok(lang::resolve(&LangReq::Tags(vec![tag.to_string()]), langs))
}

/// The VM a binding runs in, as `register_vm` tagged it — or, for a state it never tagged, the
/// binding's own index at that index's current generation (the image search's rule).
fn owner_of(lua: &Lua, gens: &HashMap<usize, u64>, scope: usize) -> Owner {
    match vm_owner(lua) {
        Some(o) => Owner { idx: o.idx, gen: o.gen },
        None => Owner { idx: scope, gen: gens.get(&scope).copied().unwrap_or(0) },
    }
}

impl Shared {
    /// `host.ocr.read(what, opts?, cb)`, called from the VM `lua` under the identity `scope`.
    pub(crate) fn ocr_read(&self, lua: &Lua, scope: usize, what: Value, opts: Value, cb: Value) -> mlua::Result<()> {
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
        let owner = owner_of(lua, &self.vm_gens.borrow(), scope);
        let prio = current_priority();
        let rects: Vec<Rect> = args.entries.iter().map(|(_, r)| r.clone().unwrap_or_default()).collect();
        let unresolved: Vec<Option<String>> = args.entries.iter().map(|(_, r)| r.clone().err()).collect();
        let nothing_to_read = unresolved.iter().all(Option::is_some);

        let st = &self.ocr_state;
        let id = st.next_ticket.get() + 1;
        st.next_ticket.set(id);
        st.pending.borrow_mut().insert(
            id,
            PendingOcr {
                ticket: id,
                lua: lua.clone(),
                cb: lua.create_registry_value(cb)?,
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
            // Every region is a window region with no rectangle this time. Answered on the next
            // tick like any read, but without the queue: no ticket against the module's limit or
            // the application's, no picture, no language to resolve, no capture source chosen —
            // the cells calls answer the same case without a capture too. With a key it is still
            // the newest read: the module's waiting reads with that key are stale, and one already
            // recognising is delivered `newer`, as for any later read.
            let stale = match args.key.as_deref() {
                Some(k) => self.ocr.supersede(owner, k),
                None => Vec::new(),
            };
            note_newest(&mut st.key_seq.borrow_mut(), owner, args.key.as_deref(), id, true);
            let mut ready = st.ready.borrow_mut();
            ready.extend(stale.into_iter().map(|s| (s, Status::Stale, None)));
            // `with_unresolved` gives each reading its own reason on delivery.
            ready.push((id, Status::Failed, None));
            return Ok(());
        }
        let first = rects.iter().find(|r| !r.is_empty()).copied().unwrap_or_default();
        let source = capture_source::read_source(lua, &*self.backend, first.tuple());
        let out = self.ocr.submit(
            Spec { regions: rects, lang: args.lang, source },
            Ticket { id, owner, key: args.key.clone(), prio },
        );
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
            self.ocr_note_crowding(owner.idx, &why);
        }
        Ok(())
    }

    /// One line per module per `SLOW_LOG_EVERY` when its reads are evicted or refused.
    fn ocr_note_crowding(&self, idx: usize, why: &str) {
        let now = Instant::now();
        let mut logged = self.ocr_state.slow_logged.borrow_mut();
        let key = usize::MAX - idx; // a slot of its own beside the slow-job one
        if logged.get(&key).is_some_and(|t| now.duration_since(*t) < policy::SLOW_LOG_EVERY) {
            return;
        }
        logged.insert(key, now);
        let id = self.ids.borrow().get(idx).cloned().unwrap_or_default();
        logging::line(
            "ocr",
            &format!("[{id}] a read was refused or evicted: {why} (said at most every 10 s)"),
        );
    }

    /// Delivers what the threads finished and what was decided without them. Driven by the
    /// loop tick, after the image results.
    pub(crate) fn fire_ocr_results(&self) {
        let st = &self.ocr_state;
        let ready = std::mem::take(&mut *st.ready.borrow_mut());
        let done: Vec<Done> = self.ocr.drain();
        if ready.is_empty() && done.is_empty() {
            return;
        }
        // Every delivery collected first, with nothing borrowed while a callback runs: a callback
        // may read again, disable a module or reload one.
        let mut deliveries: Vec<(PendingOcr, Vec<Reading>)> = Vec::new();
        {
            let mut pending = st.pending.borrow_mut();
            for (ticket, status, error) in ready {
                if let Some(p) = pending.remove(&ticket) {
                    let readings = p.rects.iter().map(|r| Reading::outcome(*r, status, error.clone())).collect();
                    deliveries.push((p, readings));
                }
            }
            for d in &done {
                for t in &d.tickets {
                    if let Some(p) = pending.remove(&t.id) {
                        deliveries.push((p, d.readings.clone()));
                    }
                }
            }
        }
        for d in &done {
            self.ocr_log_job(d);
            if d.lang_error.is_some() {
                // Perhaps it was installed since the list was read.
                self.ocr.reread_languages();
            }
        }
        if deliveries.is_empty() {
            return;
        }
        // One epoch for the drain: every answer in it is a fresh look at the screen.
        self.bump_epoch();
        for (p, readings) in deliveries {
            let readings = with_unresolved(readings, &p.unresolved);
            let alive = deliverable(p.owner, &self.vm_gens.borrow(), &self.enabled.borrow());
            if alive {
                let newer = delivered_newer(&st.key_seq.borrow(), p.owner, p.key.as_deref(), p.ticket, &readings);
                if let Err(e) = self.ocr_call(&p, &readings, newer) {
                    self.report_callback_error(p.scope, "ocr.read", &e);
                }
            }
            let _ = p.lua.remove_registry_value(p.cb);
        }
    }

    fn ocr_call(&self, p: &PendingOcr, readings: &[Reading], newer: bool) -> Result<(), String> {
        let f: Function = p.lua.registry_value(&p.cb).map_err(|e| e.to_string())?;
        let _prio = enter_priority(p.prio);
        let args = callback_args(&p.lua, readings, &p.names, p.list, newer).map_err(|e| e.to_string())?;
        call_guarded(&f, args)
    }

    /// A slow job's line, and a language that could not be met, once per module and request.
    fn ocr_log_job(&self, d: &Done) {
        let st = &self.ocr_state;
        let owners: Vec<usize> = {
            let mut o: Vec<usize> = d.tickets.iter().map(|t| t.owner.idx).collect();
            o.sort_unstable();
            o.dedup();
            o
        };
        let ids = self.ids.borrow();
        let name = |idx: usize| ids.get(idx).cloned().unwrap_or_else(|| "?".to_string());
        if let Some((req, why)) = &d.lang_error {
            for idx in &owners {
                if st.once(format!("lang\u{1}{idx}\u{1}{req:?}")) {
                    logging::line("ocr", &format!("[{}] {why}", name(*idx)));
                }
            }
        }
        if d.timings.total() >= policy::SLOW_JOB.as_millis() as u64 {
            let now = Instant::now();
            let lang = d.readings.iter().map(|r| r.lang.as_str()).find(|l| !l.is_empty()).unwrap_or("-");
            for idx in &owners {
                let mut logged = st.slow_logged.borrow_mut();
                if logged.get(idx).is_some_and(|t| now.duration_since(*t) < policy::SLOW_LOG_EVERY) {
                    continue;
                }
                logged.insert(*idx, now);
                logging::line(
                    "ocr",
                    &format!(
                        "{} region(s) for [{}] waited {} ms for the capture, which took {}, then {} ms \
                         for the recogniser, which took {} ms ({lang})",
                        d.readings.len(),
                        name(*idx),
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
    /// (true, which also forgets which key it last asked with). Nothing of it is called back.
    pub(crate) fn ocr_drop_owner(&self, idx: usize, forget_keys: bool) {
        self.ocr.cancel_owner(idx);
        let st = &self.ocr_state;
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
            let _ = p.lua.remove_registry_value(p.cb);
        }
        if forget_keys {
            st.key_seq.borrow_mut().retain(|(i, ..), _| *i != idx);
        }
    }

    /// `rollback_to(n)`'s share: every module from index `n` on.
    pub(crate) fn ocr_drop_from(&self, n: usize) {
        let mut owners: Vec<usize> =
            self.ocr_state.pending.borrow().values().map(|p| p.owner.idx).filter(|i| *i >= n).collect();
        owners.extend(self.ocr_state.key_seq.borrow().keys().map(|k| k.0).filter(|i| *i >= n));
        owners.sort_unstable();
        owners.dedup();
        for idx in owners {
            self.ocr_drop_owner(idx, true);
        }
    }

    /// The input barrier: before `host.input.*` or `host.window.focus` acts for the module that
    /// owns `lua`, its pending OCR pictures are taken — up to `policy::BARRIER`. A module that
    /// reads a field and then clicks it gets the field as it was before the click.
    pub(crate) fn ocr_barrier(&self, lua: &Lua) {
        let Some(o) = vm_owner(lua) else { return };
        if !self.ocr_state.pending.borrow().values().any(|p| p.owner.idx == o.idx) {
            return;
        }
        let started = Instant::now();
        if !self.ocr.barrier(o.idx, policy::BARRIER) {
            let id = self.ids.borrow().get(o.idx).cloned().unwrap_or_default();
            if self.ocr_state.once(format!("barrier\u{1}{}", o.idx)) {
                logging::line(
                    "ocr",
                    &format!(
                        "[{id}] input waited {} ms for its pending text-recognition picture and went \
                         ahead without it; said once",
                        started.elapsed().as_millis()
                    ),
                );
            }
        }
    }

    /// The published languages, waiting `LANG_WAIT` at most; `None` (said once) when they are not
    /// known yet.
    fn ocr_langs(&self) -> Option<lang::Languages> {
        let l = self.ocr.languages(policy::LANG_WAIT);
        if l.is_none() && self.ocr_state.once("langs-late".to_string()) {
            logging::line(
                "ocr",
                "the recognition languages were not known yet within 50 ms; answering from what is \
                 known, which is nothing",
            );
        }
        l
    }

    /// `host.ocr.languages()`.
    pub(crate) fn ocr_languages_value(&self, lua: &Lua) -> mlua::Result<Table> {
        let list = self.ocr_langs().map(|l| lang::listed(&l)).unwrap_or_default();
        let t = lua.create_table_with_capacity(list.len(), 0)?;
        for tag in list {
            t.raw_push(tag)?;
        }
        Ok(t)
    }

    /// `host.ocr.resolveLanguage(tag | {tag} | nil)`.
    pub(crate) fn ocr_resolve_value(&self, v: &Value) -> mlua::Result<Option<String>> {
        let req = lang_request(v, "host.ocr.resolveLanguage")?;
        let Some(langs) = self.ocr_langs() else { return Ok(None) };
        match lang::resolve(&req, &langs) {
            Ok(tag) => Ok(Some(tag)),
            Err(_) => {
                self.ocr.reread_languages();
                Ok(None)
            }
        }
    }

    /// The `lang` of `recognize` / `recognizeMany`: the platform's tag it resolves to, or the
    /// `error` a result carries when nothing here reads it. A malformed tag raises. When the
    /// list is not known yet, the tag goes to the engine as written, which is what these two
    /// calls always did.
    pub(crate) fn ocr_legacy_lang(&self, tag: &str, binding: &str) -> mlua::Result<Result<String, String>> {
        // Malformed raises before the list is waited for, as it did.
        lang::parse(tag).map_err(|e| mlua::Error::external(format!("{binding}: {e}")))?;
        let langs = self.ocr_langs();
        let resolved = legacy_lang(tag, langs.as_ref()).map_err(|e| mlua::Error::external(format!("{binding}: {e}")))?;
        if let Err(why) = &resolved {
            self.ocr.reread_languages();
            if self.ocr_state.once(format!("legacy-lang\u{1}{tag}")) {
                logging::line("ocr", &format!("{binding}: {why}"));
            }
        }
        Ok(resolved)
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

    /// `recognize` / `recognizeMany`: a malformed tag raises; while the list is not known the tag
    /// goes to the engine as written; then it is resolved to the engine's spelling, or answered
    /// as unavailable.
    #[test]
    fn the_legacy_calls_lang_goes_through_the_resolver() {
        let langs = lang::Languages {
            available: vec!["en-US".into(), "de-DE".into()],
            fast: vec![],
            preferred: vec![],
        };
        assert!(legacy_lang("deutsch", Some(&langs)).is_err());
        assert!(legacy_lang("deutsch", None).is_err(), "raised whether or not the list is known");
        assert_eq!(legacy_lang("de", None), Ok(Ok("de".to_string())));
        assert_eq!(legacy_lang("de", Some(&langs)), Ok(Ok("de-DE".to_string())));
        let why = legacy_lang("ja", Some(&langs)).unwrap().unwrap_err();
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
        let args = callback_args(&lua, &readings, &names, true, false).unwrap();
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
        let args = callback_args(&lua, &readings[..1], &[None], false, true).unwrap();
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
        let t = reading_table(&lua, &r, Some("value"), true).unwrap();
        lua.globals().set("r", t).unwrap();
        let ok: bool = lua
            .load(
                r#"
                return r.name == "value" and r.status == "text" and r.text == "7" and r.newer == true
                  and r.x == 10 and r.w == 100 and r.lang == "en-US" and r.error == nil
                  and #r.words == 1 and r.words[1].approx == true and r.words[1].x == 12
                  and #r.lines == 1 and r.lines[1].text == "7" and #r.lines[1].words == 1
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
        let failed = Reading::failed(Rect::new(0, 0, 1, 1), "screen capture failed");
        let t = reading_table(&lua, &failed, None, false).unwrap();
        lua.globals().set("f", t).unwrap();
        let ok: bool = lua
            .load(r#"return f.name == nil and f.status == "failed" and f.text == "" and #f.words == 0 and f.error == "screen capture failed" and f.approx == nil"#)
            .eval()
            .unwrap();
        assert!(ok);
    }
}
