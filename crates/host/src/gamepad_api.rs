//! `host.gamepad` — game controllers, as a module sees them.
//!
//! The OS half is `backend/gamepad`: sources feed a hub, the pump drains it into
//! [`Shared::dispatch_gamepad`]. This file is everything after that — who is listening, what
//! each listener is owed, and the Luau tables — kept out of `lib.rs` so the dispatch rules can
//! be tested as a pure function ([`pad_deliveries`]) the way `hotkey_winners` is.
//!
//! **A broadcast, not a claim.** Nothing is consumed — the game gets every press whatever we
//! do — so there is no owner to resolve the way a hotkey has one. Every enabled module whose
//! listener matches gets the event, in registration order. "Only while my overlay is active"
//! is the module's own lifecycle: register on activation, `off` on deactivation.
//!
//! **Ownership follows the VM, permission follows the author**, as for every other callback: a
//! listener registered by a code dependency's code belongs to the module whose VM runs it and
//! goes away when that module is disabled, while the right to call `host.gamepad` at all comes
//! from the manifest of the module that wrote the code (`build_dep_host`).

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, RegistryKey, Table};

use crate::backend::gamepad::{self as gp, demand, names, Axis, Button, PadEvent, PadEventKind, PadInfo, PadState};
use crate::{appcfg, call_guarded, clock_origin, logging, Shared};

/// Which events a listener is for — the first argument of `host.gamepad.on`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Down,
    Up,
    Axis,
    Connected,
    Disconnected,
}

impl Kind {
    fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "down" => Kind::Down,
            "up" => Kind::Up,
            "axis" => Kind::Axis,
            "connected" => Kind::Connected,
            "disconnected" => Kind::Disconnected,
            _ => return None,
        })
    }
}

/// Microsoft's own dead zones, as fractions of full deflection: `XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE`
/// (7849), `..._RIGHT_THUMB_DEADZONE` (8689) and `XINPUT_GAMEPAD_TRIGGER_THRESHOLD` (30 of 255).
/// Used for every source, because a module should not get a different stick on a Mac.
const LEFT_STICK_DZ: f32 = 7849.0 / 32767.0;
const RIGHT_STICK_DZ: f32 = 8689.0 / 32767.0;
const TRIGGER_DZ: f32 = 30.0 / 255.0;

/// One listener's options and what it has been delivered so far.
pub(crate) struct PadFilter {
    pub kind: Kind,
    pub pad: Option<u8>,
    pub buttons: Option<Vec<Button>>,
    pub axes: Option<Vec<Axis>>,
    /// `None` is the per-axis default; `Some(0.0)` switches the dead zone off.
    pub deadzone: Option<f32>,
    pub step: f32,
    /// A press older than this when the pump gets to it is not delivered. A modal dialog
    /// stops the pump, and a "D-pad down" acted on seconds later would read the screen of a
    /// menu the game has long since moved on from. Buttons only: an axis value is the stick's
    /// latest position however old, and dropping it would leave the listener with the one
    /// before, which the hub will not send again.
    pub max_age: Duration,
    /// The value last delivered per (pad, axis), which `step` is measured from.
    pub last: HashMap<(u8, Axis), f32>,
    /// The buttons this listener saw go down, fresh, and not yet up. An up is delivered only
    /// for one of these: a release whose press was too old, happened before the listener
    /// existed, or happened while its module was disabled is a release of something the
    /// module never saw pressed. Kept by every listener, `up` ones included, because the
    /// `down` that pairs with an `up` is seen here even when it is not delivered.
    pub held: HashSet<(u8, Button)>,
    /// For a `connected` listener: whether the replay — a `connected` for every pad already
    /// there — is still owed.
    pub replay_pending: bool,
    /// For a `connected` listener: every pad it has been told is connected, by a real event or
    /// a replay, and not told since has gone. What stops it hearing about one pad twice: the
    /// replay reads the pads under a different lock from the queue the real event waits in, so
    /// a pad can be in both.
    pub announced: HashSet<u8>,
}

impl PadFilter {
    pub fn new(kind: Kind) -> PadFilter {
        PadFilter {
            kind,
            pad: None,
            buttons: None,
            axes: None,
            deadzone: None,
            step: 0.05,
            max_age: Duration::from_millis(1000),
            last: HashMap::new(),
            held: HashSet::new(),
            replay_pending: kind == Kind::Connected,
            announced: HashSet::new(),
        }
    }

    /// Everything this listener remembers of pad `pad`, forgotten because it went away: a pad
    /// that comes back, or another given the same index, starts from rest and is new.
    fn forget_pad(&mut self, pad: u8) {
        self.last.retain(|(p, _), _| *p != pad);
        self.held.retain(|(p, _)| *p != pad);
        self.announced.remove(&pad);
    }

    /// One half of a stick (or a trigger), dead-zoned for this listener: the value to deliver,
    /// or `None` when it is not news — it moved less than `step` since the last one delivered,
    /// and did not come to rest or reach full deflection, which a listener always hears.
    fn axis_news(&mut self, pad: u8, axis: Axis, raw: f32, other: f32) -> Option<f32> {
        if !self.wants_axis(axis) {
            return None;
        }
        let dz = self.deadzone.unwrap_or_else(|| default_deadzone(axis));
        let v = dead_zoned(axis, raw, other, dz);
        let last = self.last.get(&(pad, axis)).copied().unwrap_or(0.0);
        let moved = if self.step > 0.0 { (v - last).abs() >= self.step } else { v != last };
        let settled = (v == 0.0 || v.abs() >= 1.0) && v != last;
        if moved || settled {
            self.last.insert((pad, axis), v);
            Some(v)
        } else {
            None
        }
    }

    fn wants_button(&self, b: Button) -> bool {
        self.buttons.as_ref().is_none_or(|list| list.contains(&b))
    }

    fn wants_axis(&self, a: Axis) -> bool {
        self.axes.as_ref().is_none_or(|list| list.contains(&a))
    }
}

struct PadListener {
    token: i64,
    module_idx: usize,
    lua: Lua,
    cb: RegistryKey,
    filter: PadFilter,
}

/// The listeners of every module, held in `Shared`.
#[derive(Default)]
pub(crate) struct Pads {
    listeners: RefCell<Vec<PadListener>>,
    next_token: Cell<i64>,
    /// Set by `refresh_gamepad` when an enabled `connected` listener is owed a replay, cleared
    /// once the replays have run.
    replay_due: Cell<bool>,
}

impl Pads {
    /// For the headless keep-alive: a module whose only reason to run is a pad listener must
    /// not see the loop decide there is nothing to wait for.
    pub(crate) fn has_listeners(&self) -> bool {
        !self.listeners.borrow().is_empty()
    }
}

/// One callback to make: listener `token`, event `event` of the batch, and for an axis the
/// value after that listener's dead zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Delivery {
    pub token: i64,
    pub event: usize,
    pub value: Option<f32>,
    /// For a stick event: this delivery is about the OTHER half of the stick, whose raw value
    /// the event carries as `partner`. See [`pad_deliveries`] on why a stick is dead-zoned and
    /// delivered as a pair.
    pub partner: bool,
}

fn default_deadzone(a: Axis) -> f32 {
    match a {
        Axis::LeftX | Axis::LeftY => LEFT_STICK_DZ,
        Axis::RightX | Axis::RightY => RIGHT_STICK_DZ,
        Axis::LeftTrigger | Axis::RightTrigger => TRIGGER_DZ,
        Axis::Other(_) => 0.0,
    }
}

/// An axis value after a dead zone of `dz`, rescaled so the edge of the dead zone is 0 and
/// full deflection is still 1.
///
/// Sticks are dead-zoned RADIALLY, over the pair: a per-axis dead zone turns a stick held
/// slightly off the diagonal into a pure horizontal and makes the rest position a cross
/// instead of a circle. Triggers and unmapped axes have no partner and are dead-zoned alone.
pub(crate) fn dead_zoned(axis: Axis, value: f32, partner: f32, dz: f32) -> f32 {
    if dz <= 0.0 {
        return value;
    }
    if dz >= 1.0 {
        return 0.0;
    }
    if names::partner(axis).is_some() {
        let mag = (value * value + partner * partner).sqrt();
        if mag < dz || mag == 0.0 {
            return 0.0;
        }
        let scaled = ((mag - dz) / (1.0 - dz)).min(1.0);
        (value / mag * scaled).clamp(-1.0, 1.0)
    } else {
        let m = value.abs();
        if m < dz {
            0.0
        } else {
            (value.signum() * ((m - dz) / (1.0 - dz))).clamp(-1.0, 1.0)
        }
    }
}

/// Who gets which event of `batch` — the dispatch rules as a pure function, like
/// `hotkey_winners`, so they are executed by tests rather than merely read.
///
/// Per listener, in registration order, for each event in order:
/// * a pad that disconnected is forgotten by every listener, a disabled module's included —
///   that is bookkeeping, not delivery, and a listener that skipped it would take the next pad
///   given that index for the one it already knew;
/// * apart from that, a disabled module's listeners are skipped entirely;
/// * `opts.pad` filters by index, `buttons`/`axes` by name;
/// * a down older than `maxAge` is not delivered; an up is delivered only if this listener saw
///   its down fresh — never for its own age, because the module that got the down needs the
///   release, and never without the down, because a module must not hear a release of
///   something it never saw pressed;
/// * a stick is dead-zoned radially, so moving one half changes the dead-zoned value of the
///   other: each stick event is delivered as a PAIR, both halves computed from the value and
///   the partner the event carries, each half only when it moved at least `step` since the
///   value this listener last got — or reached rest or full deflection, which a listener must
///   always hear about even if the step there was small. When both halves of a stick moved
///   in the same batch, only the newer event is used, since it carries the newer pair, so a
///   half is delivered at most once per batch. Axis values are never dropped for age;
/// * a `connected` is delivered once per pad per connection — a pad already announced by the
///   replay is not announced again by its real event; connections are never dropped for age.
pub(crate) fn pad_deliveries<'a>(
    listeners: impl Iterator<Item = (i64, usize, &'a mut PadFilter)>,
    batch: &[PadEvent],
    now: Instant,
    enabled: impl Fn(usize) -> bool,
) -> Vec<Delivery> {
    let mut ls: Vec<(i64, usize, &mut PadFilter)> = listeners.collect();
    let newest = newest_stick_events(batch);
    let mut out = Vec::new();
    for (i, ev) in batch.iter().enumerate() {
        let age = now.saturating_duration_since(ev.at);
        for (token, module_idx, f) in ls.iter_mut() {
            if matches!(ev.kind, PadEventKind::Disconnected(_)) {
                f.forget_pad(ev.pad);
            }
            if !enabled(*module_idx) || f.pad.is_some_and(|p| p != ev.pad) {
                continue;
            }
            let deliver = |value: Option<f32>, partner: bool| Delivery { token: *token, event: i, value, partner };
            match &ev.kind {
                PadEventKind::Down(b) => {
                    let fresh = age <= f.max_age;
                    if fresh {
                        f.held.insert((ev.pad, *b));
                    } else {
                        f.held.remove(&(ev.pad, *b));
                    }
                    if f.kind == Kind::Down && fresh && f.wants_button(*b) {
                        out.push(deliver(None, false));
                    }
                }
                PadEventKind::Up(b) => {
                    let saw_its_down = f.held.remove(&(ev.pad, *b));
                    if f.kind == Kind::Up && saw_its_down && f.wants_button(*b) {
                        out.push(deliver(None, false));
                    }
                }
                PadEventKind::Axis { axis, value, partner } => {
                    if f.kind != Kind::Axis {
                        continue;
                    }
                    let Some(other) = names::partner(*axis) else {
                        // A trigger, or an unmapped axis: no partner, dead-zoned alone.
                        if let Some(v) = f.axis_news(ev.pad, *axis, *value, 0.0) {
                            out.push(deliver(Some(v), false));
                        }
                        continue;
                    };
                    if newest.get(&(ev.pad, (*axis).min(other))) != Some(&i) {
                        continue;
                    }
                    // Both halves, in axis order (x before y) whichever of them moved.
                    let mine = (*axis, *value, *partner, false);
                    let theirs = (other, *partner, *value, true);
                    let halves = if *axis < other { [mine, theirs] } else { [theirs, mine] };
                    for (a, raw, with, is_partner) in halves {
                        if let Some(v) = f.axis_news(ev.pad, a, raw, with) {
                            out.push(deliver(Some(v), is_partner));
                        }
                    }
                }
                PadEventKind::Connected(_) => {
                    if f.kind == Kind::Connected && f.announced.insert(ev.pad) {
                        out.push(deliver(None, false));
                    }
                }
                PadEventKind::Disconnected(_) => {
                    if f.kind == Kind::Disconnected {
                        out.push(deliver(None, false));
                    }
                }
            }
        }
    }
    out
}

/// For each (pad, stick) with events in `batch`, the index of the newest one — the event that
/// carries the stick's most recent pair of values. Keyed by the stick's lower axis (its x).
fn newest_stick_events(batch: &[PadEvent]) -> HashMap<(u8, Axis), usize> {
    let mut newest: HashMap<(u8, Axis), usize> = HashMap::new();
    for (i, ev) in batch.iter().enumerate() {
        let PadEventKind::Axis { axis, .. } = ev.kind else { continue };
        let Some(other) = names::partner(axis) else { continue };
        newest
            .entry((ev.pad, axis.min(other)))
            .and_modify(|j| {
                if batch[*j].at <= ev.at {
                    *j = i;
                }
            })
            .or_insert(i);
    }
    newest
}

/// The `connected` replays that are due: for each enabled listener that is owed one, every
/// pad present now that its filter admits and that it has not been told about already.
///
/// A disabled module's listener keeps its replay owed, for when the module is enabled again.
/// The pads replayed are marked announced, so their real `connected`, if it is still queued,
/// is not delivered a second time.
pub(crate) fn replay_deliveries<'a>(
    listeners: impl Iterator<Item = (i64, usize, &'a mut PadFilter)>,
    pads: &[PadInfo],
    enabled: impl Fn(usize) -> bool,
) -> Vec<(i64, PadInfo)> {
    let mut out = Vec::new();
    for (token, module_idx, f) in listeners {
        if !f.replay_pending || !enabled(module_idx) {
            continue;
        }
        f.replay_pending = false;
        for info in pads {
            if f.pad.is_some_and(|p| p != info.index) || !f.announced.insert(info.index) {
                continue;
            }
            out.push((token, info.clone()));
        }
    }
    out
}

/// The demand bits the enabled modules' listeners add up to.
pub(crate) fn demand_bits(listeners: impl Iterator<Item = (usize, Kind)>, enabled: impl Fn(usize) -> bool) -> u8 {
    listeners.filter(|(idx, _)| enabled(*idx)).fold(0, |bits, (_, kind)| {
        bits | match kind {
            Kind::Down | Kind::Up => demand::BUTTONS,
            Kind::Axis => demand::AXES,
            Kind::Connected | Kind::Disconnected => demand::CONNECT,
        }
    })
}

impl Shared {
    /// Tells the source how hard to work, from the listeners of ENABLED modules. Derived every
    /// time rather than counted up and down, for the reason `refresh_hotkeys` gives: stored
    /// liveness goes stale, derived liveness cannot.
    ///
    /// It also owes every `connected` listener a replay again. Called when a listener comes or
    /// goes and when a module is enabled or disabled, and the replay only announces pads the
    /// listener has not been told about — for a listener that is up to date it is nothing, and
    /// for one whose module was disabled while a pad arrived it is exactly that pad.
    pub(crate) fn refresh_gamepad(&self) {
        if !gp::started() {
            return;
        }
        let (bits, replay) = {
            let enabled = self.enabled.borrow();
            let is_on = |i: usize| enabled.get(i).copied().unwrap_or(false);
            let mut ls = self.pads.listeners.borrow_mut();
            let mut replay = false;
            for l in ls.iter_mut().filter(|l| l.filter.kind == Kind::Connected) {
                l.filter.replay_pending = true;
                replay |= is_on(l.module_idx);
            }
            (demand_bits(ls.iter().map(|l| (l.module_idx, l.filter.kind)), is_on), replay)
        };
        if replay {
            self.pads.replay_due.set(true);
        }
        let hub = gp::hub();
        if hub.demand() != bits {
            logging::trace("gamepad", || format!("demand is now {bits:#05b} (buttons 1, axes 2, connections 4)"));
        }
        hub.set_demand(bits);
    }

    /// Drops the listeners of the modules `gone` names, for a purge, a reload and a rollback.
    pub(crate) fn drop_pad_listeners(&self, gone: impl Fn(usize) -> bool) {
        self.pads.listeners.borrow_mut().retain(|l| !gone(l.module_idx));
        self.refresh_gamepad();
    }

    /// One drained batch, delivered. Called from `Dispatcher::on_gamepad`.
    ///
    /// The epoch turns once per batch and only when a callback actually runs: a stick held off
    /// centre sends a value every drain, and turning the epoch for each would throw away every
    /// memo in every module 66 times a second while nobody is listening. A delivered press or
    /// release also turns the INPUT epoch, because unlike a captured key it reaches the game and
    /// the game's screen changes — though a frame or more later, which is why the reference
    /// tells modules not to cache the game's screen against `inputEpoch` alone.
    pub(crate) fn dispatch_gamepad(&self, batch: Vec<PadEvent>) {
        let started = Instant::now();
        let deliveries = {
            let enabled = self.enabled.borrow();
            let mut ls = self.pads.listeners.borrow_mut();
            pad_deliveries(
                ls.iter_mut().map(|l| (l.token, l.module_idx, &mut l.filter)),
                &batch,
                started,
                |i| enabled.get(i).copied().unwrap_or(false),
            )
        };
        if appcfg::trace() {
            let notable = batch.iter().filter(|e| !matches!(e.kind, PadEventKind::Axis { .. })).count();
            if notable > 0 {
                logging::line(
                    "gamepad",
                    &format!(
                        "{} event(s) drained ({notable} button or connection), {} callback(s) to make, oldest {} ms",
                        batch.len(),
                        deliveries.len(),
                        batch.iter().map(|e| started.saturating_duration_since(e.at).as_millis()).max().unwrap_or(0)
                    ),
                );
            }
        }
        if !deliveries.is_empty() {
            let pressed = deliveries
                .iter()
                .any(|d| matches!(batch[d.event].kind, PadEventKind::Down(_) | PadEventKind::Up(_)));
            if pressed {
                self.bump_input_epoch();
            } else {
                self.bump_epoch();
            }
            self.deliver_pad_events(&batch, &deliveries);
        }
        let mut c = self.ev_counts.get();
        c.4 += started.elapsed().as_millis();
        self.ev_counts.set(c);
    }

    /// Delivers the `connected` replays owed since the last tick. On the tick rather than
    /// inside `on`, so a callback never runs in the middle of the call that registered it.
    /// The pads are read from the hub under a different lock from the queue, so a pad can be
    /// both listed here and still waiting as a real `connected`; the listener's `announced`
    /// set is what makes it hear about that pad once.
    pub(crate) fn fire_pad_replays(&self) {
        if !self.pads.replay_due.replace(false) || !gp::started() {
            return;
        }
        let pads = gp::hub().list();
        let due = {
            let enabled = self.enabled.borrow();
            let mut ls = self.pads.listeners.borrow_mut();
            replay_deliveries(
                ls.iter_mut().map(|l| (l.token, l.module_idx, &mut l.filter)),
                &pads,
                |i| enabled.get(i).copied().unwrap_or(false),
            )
        };
        if due.is_empty() {
            return;
        }
        self.bump_epoch();
        let now = Instant::now();
        let batch: Vec<PadEvent> = due
            .iter()
            .map(|(_, info)| PadEvent {
                pad: info.index,
                kind: PadEventKind::Connected(info.clone()),
                at: now,
                synthetic: true,
                family: info.desc.family,
            })
            .collect();
        let deliveries: Vec<Delivery> = due
            .iter()
            .enumerate()
            .map(|(i, (token, _))| Delivery { token: *token, event: i, value: None, partner: false })
            .collect();
        self.deliver_pad_events(&batch, &deliveries);
    }

    /// Makes the callbacks. The listener list is NOT borrowed while one runs: a callback may
    /// call `on` or `off` — the usual way to stop listening is from inside the listener — and
    /// each callback is looked up by token afresh, so one that was removed by an earlier
    /// callback in the same batch is skipped instead of called.
    fn deliver_pad_events(&self, batch: &[PadEvent], deliveries: &[Delivery]) {
        for d in deliveries {
            let found = {
                let ls = self.pads.listeners.borrow();
                ls.iter().find(|l| l.token == d.token).and_then(|l| {
                    l.lua.registry_value::<Function>(&l.cb).ok().map(|f| (l.module_idx, l.lua.clone(), f))
                })
            };
            let Some((idx, lua, f)) = found else { continue };
            let result = event_table(&lua, &batch[d.event], d.value, d.partner, Instant::now())
                .map_err(|e| e.to_string())
                .and_then(|t| call_guarded(&f, t));
            if let Err(e) = result {
                self.report_callback_error(idx, "gamepad", &e);
            }
        }
    }
}

/// Milliseconds on the `host.now()` clock.
fn clock_ms(at: Instant) -> i64 {
    at.saturating_duration_since(clock_origin()).as_millis() as i64
}

/// Four decimals, which is finer than any pad reports: an f32 widened to a Lua number
/// otherwise reads 0.7300000190734863 in every log line a module writes.
fn tidy(v: f32) -> f64 {
    (v as f64 * 10_000.0).round() / 10_000.0
}

/// `partner`: for a stick event, the table is about the other half of the stick — its name,
/// and the raw value the event carries for it.
fn event_table(lua: &Lua, ev: &PadEvent, value: Option<f32>, partner: bool, now: Instant) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("pad", ev.pad)?;
    t.set("time", clock_ms(ev.at))?;
    t.set("age", now.saturating_duration_since(ev.at).as_millis() as i64)?;
    t.set("synthetic", ev.synthetic)?;
    match &ev.kind {
        PadEventKind::Down(b) | PadEventKind::Up(b) => {
            t.set("type", if matches!(ev.kind, PadEventKind::Down(_)) { "down" } else { "up" })?;
            t.set("button", names::button_name(*b))?;
            t.set("label", names::label(*b, ev.family))?;
        }
        PadEventKind::Axis { axis, value: raw, partner: other } => {
            let (axis, raw) = match names::partner(*axis) {
                Some(p) if partner => (p, *other),
                _ => (*axis, *raw),
            };
            t.set("type", "axis")?;
            t.set("axis", names::axis_name(axis))?;
            t.set("value", tidy(value.unwrap_or(raw)))?;
            t.set("raw", tidy(raw))?;
        }
        PadEventKind::Connected(info) | PadEventKind::Disconnected(info) => {
            t.set("type", if matches!(ev.kind, PadEventKind::Connected(_)) { "connected" } else { "disconnected" })?;
            t.set("info", info_table(lua, info)?)?;
        }
    }
    Ok(t)
}

fn info_table(lua: &Lua, info: &PadInfo) -> mlua::Result<Table> {
    let d = &info.desc;
    let t = lua.create_table()?;
    t.set("index", info.index)?;
    t.set("id", d.id.as_str())?;
    t.set("name", d.name.as_str())?;
    t.set("family", names::family_name(d.family))?;
    t.set("source", names::source_name(d.source))?;
    t.set("mapped", d.mapped)?;
    if let Some(v) = d.vendor {
        t.set("vendor", v)?;
    }
    if let Some(p) = d.product {
        t.set("product", p)?;
    }
    let buttons = lua.create_table()?;
    for b in gp::all_buttons(d) {
        buttons.push(names::button_name(b))?;
    }
    t.set("buttons", buttons)?;
    let axes = lua.create_table()?;
    for a in &d.axes {
        axes.push(names::axis_name(*a))?;
    }
    t.set("axes", axes)?;
    Ok(t)
}

fn state_table(lua: &Lua, s: &PadState) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("pad", s.pad)?;
    let buttons = lua.create_table()?;
    for b in &s.buttons {
        buttons.set(names::button_name(*b), s.pressed.contains(b))?;
    }
    t.set("buttons", buttons)?;
    let axes = lua.create_table()?;
    for (a, v) in &s.axes {
        axes.set(names::axis_name(*a), tidy(*v))?;
    }
    t.set("axes", axes)?;
    t.set("time", clock_ms(s.at))?;
    Ok(t)
}

/// `host.gamepad.on`'s options, checked strictly. A misspelt option or name would otherwise be
/// a listener that silently never fires — the one failure a module author cannot see from the
/// inside — so every unknown key and name is an error naming what is allowed.
pub(crate) fn parse_filter(event: &str, opts: Option<&Table>) -> mlua::Result<PadFilter> {
    let kind = Kind::parse(event).ok_or_else(|| {
        mlua::Error::external(format!(
            "host.gamepad.on: unknown event '{event}' — expected \"down\", \"up\", \"axis\", \"connected\" or \"disconnected\""
        ))
    })?;
    let mut f = PadFilter::new(kind);
    let Some(opts) = opts else { return Ok(f) };
    let fail = |msg: String| mlua::Error::external(format!("host.gamepad.on(\"{event}\"): {msg}"));
    let allowed: &[&str] = match kind {
        Kind::Down | Kind::Up => &["pad", "buttons", "maxAge"],
        // No `maxAge`: see `PadFilter::max_age`. Refused rather than ignored, so a module that
        // sets it learns that it has no effect here.
        Kind::Axis => &["pad", "axes", "deadzone", "step"],
        Kind::Connected | Kind::Disconnected => &["pad"],
    };
    for pair in opts.pairs::<mlua::Value, mlua::Value>() {
        let (k, v) = pair?;
        let key = match &k {
            mlua::Value::String(s) => s.to_str()?.to_string(),
            other => return Err(fail(format!("option names are strings, not {}", other.type_name()))),
        };
        if !allowed.contains(&key.as_str()) {
            return Err(fail(format!("'{key}' is not an option here; this event takes {}", allowed.join(", "))));
        }
        let number = |v: &mlua::Value| -> mlua::Result<f64> {
            match v {
                mlua::Value::Integer(i) => Ok(*i as f64),
                mlua::Value::Number(n) => Ok(*n),
                other => Err(fail(format!("'{key}' must be a number, not {}", other.type_name()))),
            }
        };
        match key.as_str() {
            "pad" => {
                let n = number(&v)?;
                if n.fract() != 0.0 || !(1.0..=gp::MAX_PADS as f64).contains(&n) {
                    return Err(fail(format!("pad must be a whole number from 1 to {}, not {n}", gp::MAX_PADS)));
                }
                f.pad = Some(n as u8);
            }
            "buttons" => {
                let names_given = string_list(&v).map_err(|e| fail(format!("buttons: {e}")))?;
                let mut list = Vec::new();
                for s in names_given {
                    list.push(names::parse_button(&s).ok_or_else(|| {
                        fail(format!(
                            "unknown button '{s}' — names are positional, like south, dpad_down, left_shoulder, \
                             left_stick_up or button7"
                        ))
                    })?);
                }
                f.buttons = Some(list);
            }
            "axes" => {
                let names_given = string_list(&v).map_err(|e| fail(format!("axes: {e}")))?;
                let mut list = Vec::new();
                for s in names_given {
                    list.push(names::parse_axis(&s).ok_or_else(|| {
                        fail(format!("unknown axis '{s}' — left_x, left_y, right_x, right_y, left_trigger or right_trigger"))
                    })?);
                }
                f.axes = Some(list);
            }
            "deadzone" => {
                let n = number(&v)?;
                if !(0.0..1.0).contains(&n) {
                    return Err(fail(format!("deadzone must be at least 0 and below 1, not {n}")));
                }
                f.deadzone = Some(n as f32);
            }
            "step" => {
                let n = number(&v)?;
                if !(0.0..=2.0).contains(&n) {
                    return Err(fail(format!("step must be from 0 to 2, not {n}")));
                }
                f.step = n as f32;
            }
            "maxAge" => {
                let n = number(&v)?;
                if n.is_nan() || n < 0.0 {
                    return Err(fail(format!("maxAge is milliseconds and cannot be negative, not {n}")));
                }
                f.max_age = Duration::from_millis(n.min(u32::MAX as f64) as u64);
            }
            _ => unreachable!("checked against `allowed` above"),
        }
    }
    Ok(f)
}

fn string_list(v: &mlua::Value) -> Result<Vec<String>, String> {
    let t: Table = match v {
        mlua::Value::Table(t) => t.clone(),
        other => return Err(format!("expected a list of names, not {}", other.type_name())),
    };
    let mut out = Vec::new();
    for s in t.sequence_values::<String>() {
        out.push(s.map_err(|e| format!("every entry must be a name ({e})"))?);
    }
    if out.is_empty() {
        // An empty filter admits nothing, which is never what was meant.
        return Err("the list is empty, so the listener would never fire".to_string());
    }
    Ok(out)
}

/// Installs `host.gamepad` on one module's host table. `idx` is the module that OWNS what is
/// registered through it — the VM's owner, even for a dependency's code.
pub(crate) fn install(lua: &Lua, host: &Table, shared: &Rc<Shared>, idx: usize) -> mlua::Result<()> {
    let gamepad = lua.create_table()?;

    // host.gamepad.list() -> { PadInfo }
    gamepad.set(
        "list",
        lua.create_function(|lua, ()| {
            gp::ensure_started().map_err(mlua::Error::external)?;
            gp::refresh_if_idle();
            let t = lua.create_table()?;
            for info in gp::hub().list() {
                t.push(info_table(lua, &info)?)?;
            }
            Ok(t)
        })?,
    )?;

    // host.gamepad.state(pad) -> PadState?
    gamepad.set(
        "state",
        lua.create_function(|lua, pad: f64| -> mlua::Result<mlua::Value> {
            gp::ensure_started().map_err(mlua::Error::external)?;
            if pad.fract() != 0.0 || !(1.0..=gp::MAX_PADS as f64).contains(&pad) {
                return Ok(mlua::Value::Nil);
            }
            gp::refresh_if_idle();
            match gp::hub().state(pad as u8) {
                Some(s) => Ok(mlua::Value::Table(state_table(lua, &s)?)),
                None => Ok(mlua::Value::Nil),
            }
        })?,
    )?;

    // host.gamepad.on(event, callback, opts?) -> token
    let sh = shared.clone();
    gamepad.set(
        "on",
        lua.create_function(move |lua, (event, cb, opts): (String, Function, Option<Table>)| {
            let filter = parse_filter(&event, opts.as_ref())?;
            gp::ensure_started().map_err(mlua::Error::external)?;
            let key = lua.create_registry_value(cb)?;
            let token = sh.pads.next_token.get() + 1;
            sh.pads.next_token.set(token);
            sh.pads.listeners.borrow_mut().push(PadListener {
                token,
                module_idx: idx,
                lua: lua.clone(),
                cb: key,
                filter,
            });
            // Also arranges the replay a new `connected` listener is owed.
            sh.refresh_gamepad();
            Ok(token)
        })?,
    )?;

    // host.gamepad.off(token) -> boolean
    let sh = shared.clone();
    gamepad.set(
        "off",
        lua.create_function(move |_, token: i64| {
            let removed = {
                let mut ls = sh.pads.listeners.borrow_mut();
                let before = ls.len();
                // Only this module's own: a token is a small number, and one module must not be
                // able to silence another's listener by guessing it.
                ls.retain(|l| !(l.token == token && l.module_idx == idx));
                ls.len() != before
            };
            if removed {
                sh.refresh_gamepad();
            }
            Ok(removed)
        })?,
    )?;

    // host.gamepad.status() -> { [string]: string }. Does not start the watcher: it answers
    // "not started" instead, so asking has no side effect.
    gamepad.set(
        "status",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            match gp::start_outcome() {
                None => t.set("watcher", "not started")?,
                Some(Err(e)) => t.set("watcher", format!("failed: {e}"))?,
                Some(Ok(())) => {
                    t.set("watcher", "running")?;
                    for (k, v) in gp::hub().status() {
                        t.set(k, v)?;
                    }
                }
            }
            Ok(t)
        })?,
    )?;

    host.set("gamepad", gamepad)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::gamepad::{Family, PadDesc, Source};

    fn t0() -> Instant {
        Instant::now()
    }

    fn ev(pad: u8, kind: PadEventKind, at: Instant) -> PadEvent {
        PadEvent { pad, kind, at, synthetic: false, family: Family::Xbox }
    }

    fn info(index: u8) -> PadInfo {
        PadInfo {
            index,
            desc: PadDesc {
                id: format!("fake:{index}"),
                name: "Fake".into(),
                family: Family::Xbox,
                source: Source::Fake,
                mapped: true,
                vendor: None,
                product: None,
                buttons: names::PHYSICAL.to_vec(),
                axes: names::AXES.to_vec(),
            },
        }
    }

    /// (token, module, filter) triples, run through `pad_deliveries` with every module in
    /// `on` enabled; returns (token, event index) pairs.
    fn run(ls: &mut [(i64, usize, PadFilter)], batch: &[PadEvent], now: Instant, on: &[bool]) -> Vec<(i64, usize)> {
        pad_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), batch, now, |i| on.get(i).copied().unwrap_or(false))
            .into_iter()
            .map(|d| (d.token, d.event))
            .collect()
    }

    #[test]
    fn filters_pick_by_kind_pad_and_name() {
        let now = t0();
        let mut down = PadFilter::new(Kind::Down);
        down.buttons = Some(vec![Button::DpadDown, Button::South]);
        let mut pad2 = PadFilter::new(Kind::Down);
        pad2.pad = Some(2);
        let up = PadFilter::new(Kind::Up);
        let mut ls = vec![(1, 0, down), (2, 0, pad2), (3, 0, up)];
        let batch = [
            ev(1, PadEventKind::Down(Button::DpadDown), now),
            ev(1, PadEventKind::Down(Button::East), now),
            ev(2, PadEventKind::Down(Button::East), now),
            ev(1, PadEventKind::Up(Button::DpadDown), now),
        ];
        assert_eq!(run(&mut ls, &batch, now, &[true]), vec![(1, 0), (2, 2), (3, 3)]);
    }

    #[test]
    fn a_disabled_module_hears_nothing() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Down)), (2, 1, PadFilter::new(Kind::Down))];
        let batch = [ev(1, PadEventKind::Down(Button::South), now)];
        assert_eq!(run(&mut ls, &batch, now, &[false, true]), vec![(2, 0)]);
    }

    #[test]
    fn a_stale_press_is_dropped_with_its_release() {
        let now = t0();
        let old = now - Duration::from_millis(1500);
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Down)), (2, 0, PadFilter::new(Kind::Up))];
        // A down that waited 1.5 s (a modal dialog held the pump), then its fresh up.
        let batch = [
            ev(1, PadEventKind::Down(Button::South), old),
            ev(1, PadEventKind::Up(Button::South), now),
        ];
        assert!(run(&mut ls, &batch, now, &[true]).is_empty(), "neither half of a stale press");

        // The next press is fresh and both halves arrive.
        let batch = [
            ev(1, PadEventKind::Down(Button::South), now),
            ev(1, PadEventKind::Up(Button::South), now),
        ];
        assert_eq!(run(&mut ls, &batch, now, &[true]), vec![(1, 0), (2, 1)]);
    }

    #[test]
    fn a_release_is_never_dropped_for_its_own_age() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Down)), (2, 0, PadFilter::new(Kind::Up))];
        // The down was delivered in time; its up then sat through a stalled pump.
        assert_eq!(run(&mut ls, &[ev(1, PadEventKind::Down(Button::West), now)], now, &[true]), vec![(1, 0)]);
        let later = now + Duration::from_secs(5);
        assert_eq!(run(&mut ls, &[ev(1, PadEventKind::Up(Button::West), now)], later, &[true]), vec![(2, 0)]);
    }

    #[test]
    fn max_age_is_per_listener() {
        let now = t0();
        let old = now - Duration::from_millis(300);
        let mut patient = PadFilter::new(Kind::Down);
        patient.max_age = Duration::from_millis(1000);
        let mut strict = PadFilter::new(Kind::Down);
        strict.max_age = Duration::from_millis(100);
        let mut ls = vec![(1, 0, patient), (2, 0, strict)];
        assert_eq!(run(&mut ls, &[ev(1, PadEventKind::Down(Button::North), old)], now, &[true]), vec![(1, 0)]);
    }

    #[test]
    fn connections_are_delivered_however_old() {
        let now = t0();
        let old = now - Duration::from_secs(60);
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Connected)), (2, 0, PadFilter::new(Kind::Disconnected))];
        let batch = [
            ev(1, PadEventKind::Connected(info(1)), old),
            ev(1, PadEventKind::Disconnected(info(1)), old),
        ];
        assert_eq!(run(&mut ls, &batch, now, &[true]), vec![(1, 0), (2, 1)]);
    }

    fn axis(pad: u8, a: Axis, value: f32, partner: f32, at: Instant) -> PadEvent {
        ev(pad, PadEventKind::Axis { axis: a, value, partner }, at)
    }

    #[test]
    fn the_dead_zone_is_radial_and_rescaled() {
        // Inside the circle: nothing.
        assert_eq!(dead_zoned(Axis::LeftX, 0.2, 0.1, 0.24), 0.0);
        // Radial, not per axis: each half of (0.2, 0.2) is inside a 0.24 dead zone on its own,
        // but the stick as a whole is outside the circle and has moved.
        assert!(dead_zoned(Axis::LeftX, 0.2, 0.2, 0.24) > 0.0);
        // Full deflection stays full.
        assert!((dead_zoned(Axis::LeftX, 1.0, 0.0, 0.24) - 1.0).abs() < 1e-6);
        // Just past the edge is just past zero, not a jump to 0.24.
        let v = dead_zoned(Axis::LeftX, 0.25, 0.0, 0.24);
        assert!(v > 0.0 && v < 0.02, "{v}");
        // The direction survives: a diagonal keeps its two halves equal.
        let x = dead_zoned(Axis::RightX, 0.5, 0.5, 0.27);
        let y = dead_zoned(Axis::RightY, 0.5, 0.5, 0.27);
        assert!((x - y).abs() < 1e-6 && x > 0.0);
        // A trigger has no partner.
        assert_eq!(dead_zoned(Axis::LeftTrigger, 0.1, 0.9, 0.12), 0.0);
        assert!((dead_zoned(Axis::LeftTrigger, 1.0, 0.0, 0.12) - 1.0).abs() < 1e-6);
        // Zero switches it off.
        assert_eq!(dead_zoned(Axis::LeftX, 0.05, 0.0, 0.0), 0.05);
    }

    #[test]
    fn an_axis_listener_hears_steps_and_always_hears_rest() {
        let now = t0();
        let mut f = PadFilter::new(Kind::Axis);
        f.deadzone = Some(0.0);
        f.step = 0.3;
        let mut ls = vec![(1, 0, f)];
        let go = |ls: &mut Vec<(i64, usize, PadFilter)>, v: f32| {
            run(ls, &[axis(1, Axis::LeftX, v, 0.0, now)], now, &[true]).len()
        };
        assert_eq!(go(&mut ls, 0.05), 0, "below the step");
        assert_eq!(go(&mut ls, 0.35), 1);
        assert_eq!(go(&mut ls, 0.2), 0, "0.15 since the last delivered value");
        assert_eq!(go(&mut ls, 0.04), 1, "0.31 since the last delivered value");
        // 0.04 to rest is far below the step, and still delivered: a module that was told
        // 0.04 must learn that the stick came back, or it believes it is leaning forever.
        assert_eq!(go(&mut ls, 0.0), 1, "rest is always news");
        assert_eq!(go(&mut ls, 0.0), 0, "but only once");
        assert_eq!(go(&mut ls, 0.8), 1);
        assert_eq!(go(&mut ls, 1.0), 1, "and so is full deflection");
        assert_eq!(go(&mut ls, 0.95), 0);
    }

    #[test]
    fn the_axis_value_delivered_is_the_dead_zoned_one() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Axis))];
        let d = pad_deliveries(
            ls.iter_mut().map(|(t, m, f)| (*t, *m, f)),
            &[axis(1, Axis::LeftY, 0.1, 0.1, now), axis(1, Axis::LeftY, 1.0, 0.0, now)],
            now,
            |_| true,
        );
        // The first is inside the default left dead zone and was never news.
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].event, 1);
        assert!((d[0].value.unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn an_axis_filter_picks_by_name() {
        let now = t0();
        let mut f = PadFilter::new(Kind::Axis);
        f.axes = Some(vec![Axis::RightTrigger]);
        let mut ls = vec![(1, 0, f)];
        let batch = [axis(1, Axis::LeftTrigger, 1.0, 0.0, now), axis(1, Axis::RightTrigger, 0.9, 0.0, now)];
        assert_eq!(run(&mut ls, &batch, now, &[true]), vec![(1, 1)]);
    }

    /// The review's case A. Axis events are coalesced, so the one pending is the stick's
    /// position NOW, however long ago it got there, and the hub has already counted it as
    /// reported. Dropping it for its age would leave the listener at 0.8 for good.
    #[test]
    fn an_axis_value_is_never_dropped_for_its_age() {
        let now = t0();
        let mut f = PadFilter::new(Kind::Axis);
        f.deadzone = Some(0.0);
        let mut ls = vec![(1, 0, f)];
        assert_eq!(run(&mut ls, &[axis(1, Axis::LeftTrigger, 0.8, 0.0, now)], now, &[true]).len(), 1);
        // Let go 250 ms into a pump iteration that took 2 s.
        let later = now + Duration::from_secs(2);
        let let_go = now + Duration::from_millis(250);
        let d = pad_deliveries(
            ls.iter_mut().map(|(t, m, f)| (*t, *m, f)),
            &[axis(1, Axis::LeftTrigger, 0.0, 0.0, let_go)],
            later,
            |_| true,
        );
        assert_eq!(d.len(), 1, "the return to rest is delivered however late");
        assert_eq!(d[0].value, Some(0.0));
    }

    /// The review's finding 10: the radial dead zone ties the halves together, so moving x
    /// changes the dead-zoned y even though y itself did not move.
    #[test]
    fn moving_one_half_of_a_stick_updates_the_other() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Axis))];
        let deliver = |ls: &mut Vec<(i64, usize, PadFilter)>, batch: &[PadEvent]| {
            pad_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), batch, now, |_| true)
                .into_iter()
                .map(|d| (d.value.unwrap(), d.partner))
                .collect::<Vec<_>>()
        };
        // x to 0.5 with y held at 0.2 raw: outside the circle together, so both halves move —
        // y by about 0.15, although on its own 0.2 is inside the 0.24 dead zone.
        let got = deliver(&mut ls, &[axis(1, Axis::LeftX, 0.5, 0.2, now)]);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(!got[0].1 && got[0].0 > 0.3, "x first, as the event's own half: {got:?}");
        assert!(got[1].1 && (got[1].0 - 0.146).abs() < 0.01, "then y, as the partner: {got:?}");
        // x back to rest with y still at 0.2: the stick is inside the circle, so the TRUE
        // dead-zoned y is 0 — and the listener must hear it, not keep 0.146.
        let got = deliver(&mut ls, &[axis(1, Axis::LeftX, 0.0, 0.2, now)]);
        assert_eq!(got, vec![(0.0, false), (0.0, true)]);

        // And the Luau table for the partner half names the partner, with its raw value.
        let lua = Lua::new();
        let t = event_table(&lua, &axis(1, Axis::LeftX, 0.0, 0.2, now), Some(0.0), true, now).unwrap();
        assert_eq!(t.get::<String>("axis").unwrap(), "left_y");
        assert_eq!(t.get::<f64>("raw").unwrap(), 0.2);
    }

    #[test]
    fn both_halves_moving_in_one_batch_are_delivered_once_each_from_the_newer_pair() {
        let now = t0();
        let mut f = PadFilter::new(Kind::Axis);
        f.deadzone = Some(0.0);
        let mut ls = vec![(1, 0, f)];
        // y moved last, so its event carries the newest x as its partner.
        let batch = [
            axis(1, Axis::RightX, 0.6, 0.0, now - Duration::from_millis(8)),
            axis(1, Axis::RightY, -0.7, 0.6, now),
        ];
        let d = pad_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), &batch, now, |_| true);
        let got: Vec<(usize, bool, f32)> = d.iter().map(|d| (d.event, d.partner, d.value.unwrap())).collect();
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!((got[0].0, got[0].1), (1, true), "x, from the newer event, as its partner");
        assert!((got[0].2 - 0.6).abs() < 1e-6);
        assert_eq!((got[1].0, got[1].1), (1, false));
        assert!((got[1].2 + 0.7).abs() < 1e-6);
    }

    #[test]
    fn a_disconnect_resets_what_a_listener_remembers_of_that_pad() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Axis))];
        run(&mut ls, &[axis(1, Axis::LeftTrigger, 1.0, 0.0, now)], now, &[true]);
        run(&mut ls, &[ev(1, PadEventKind::Disconnected(info(1)), now)], now, &[true]);
        // A new pad at index 1, trigger fully pulled: news, although the old pad's last
        // delivered value was the same.
        assert_eq!(run(&mut ls, &[axis(1, Axis::LeftTrigger, 1.0, 0.0, now)], now, &[true]).len(), 1);
    }

    #[test]
    fn a_replay_announces_only_what_was_not_announced() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Connected)), (2, 0, {
            let mut f = PadFilter::new(Kind::Connected);
            f.pad = Some(2);
            f
        })];
        // Pad 1's real `connected` was still queued and reached the first listener first.
        run(&mut ls, &[ev(1, PadEventKind::Connected(info(1)), now)], now, &[true]);
        let pads = [info(1), info(2), info(3)];
        let due = replay_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), &pads, |_| true);
        let got: Vec<(i64, u8)> = due.iter().map(|(t, i)| (*t, i.index)).collect();
        assert_eq!(got, vec![(1, 2), (1, 3), (2, 2)]);
        // Once.
        assert!(replay_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), &pads, |_| true).is_empty());
    }

    /// The review's finding 2: the replay reads the pads while pad 2's real `connected` is still
    /// queued — the first `on("connected")` made from a timer, or a pad the Windows thread
    /// found while un-parking. The replay announces it, and the real event must not again.
    #[test]
    fn a_replay_then_the_real_event_announces_once() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Connected)), (2, 0, PadFilter::new(Kind::Disconnected))];
        let due = replay_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), &[info(2)], |_| true);
        assert_eq!(due.len(), 1);
        assert!(run(&mut ls, &[ev(2, PadEventKind::Connected(info(2)), now)], now, &[true]).is_empty());
        // Gone and back is a new connection, and both halves of it are heard.
        let batch = [ev(2, PadEventKind::Disconnected(info(2)), now), ev(2, PadEventKind::Connected(info(2)), now)];
        assert_eq!(run(&mut ls, &batch, now, &[true]), vec![(2, 0), (1, 1)]);
        // A later replay (another listener arrived) does not repeat it either.
        ls[0].2.replay_pending = true;
        assert!(replay_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), &[info(2)], |_| true).is_empty());
    }

    /// The review's finding 15, and what goes with it: a disabled module's listener keeps the
    /// replay it is owed, and still forgets a pad that leaves while it is disabled — or the
    /// pad plugged into that index afterwards would be taken for the one it already knew.
    #[test]
    fn a_disabled_module_keeps_its_replay_and_still_forgets_a_pad_that_left() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Connected))];
        let replay = |ls: &mut Vec<(i64, usize, PadFilter)>, on: bool| {
            replay_deliveries(ls.iter_mut().map(|(t, m, f)| (*t, *m, f)), &[info(1)], |_| on).len()
        };
        assert_eq!(replay(&mut ls, false), 0);
        assert!(ls[0].2.replay_pending, "still owed");
        assert_eq!(replay(&mut ls, true), 1, "delivered once the module is enabled");

        // Pad 1 leaves and another takes its index while the module is disabled...
        assert!(run(&mut ls, &[ev(1, PadEventKind::Disconnected(info(1)), now)], now, &[false]).is_empty());
        // ...and after it is enabled again, that new pad's `connected` is news.
        assert_eq!(run(&mut ls, &[ev(1, PadEventKind::Connected(info(1)), now)], now, &[true]), vec![(1, 0)]);
    }

    /// The review's finding 8: a release is delivered only to a listener that saw the press.
    #[test]
    fn an_up_is_delivered_only_after_its_down() {
        let now = t0();
        let mut ls = vec![(1, 0, PadFilter::new(Kind::Up))];
        // Registered while south was held: the release of a press it never saw.
        assert!(run(&mut ls, &[ev(1, PadEventKind::Up(Button::South), now)], now, &[true]).is_empty());
        // Pressed while the module was disabled, released after it was enabled: the same.
        run(&mut ls, &[ev(1, PadEventKind::Down(Button::East), now)], now, &[false]);
        assert!(run(&mut ls, &[ev(1, PadEventKind::Up(Button::East), now)], now, &[true]).is_empty());
        // Seen down — by an `up` listener, which is not delivered downs — then up: delivered,
        // and so is the synthetic up of a pad unplugged with a button held.
        let mut synthetic_up = ev(1, PadEventKind::Up(Button::West), now);
        synthetic_up.synthetic = true;
        let batch = [
            ev(1, PadEventKind::Down(Button::North), now),
            ev(1, PadEventKind::Up(Button::North), now),
            ev(1, PadEventKind::Down(Button::West), now),
            synthetic_up,
        ];
        assert_eq!(run(&mut ls, &batch, now, &[true]), vec![(1, 1), (1, 3)]);
    }

    #[test]
    fn demand_counts_enabled_modules_only() {
        let ls = [(0, Kind::Down), (1, Kind::Axis), (2, Kind::Connected)];
        assert_eq!(demand_bits(ls.iter().copied(), |_| true), demand::BUTTONS | demand::AXES | demand::CONNECT);
        assert_eq!(demand_bits(ls.iter().copied(), |i| i != 1), demand::BUTTONS | demand::CONNECT);
        assert_eq!(demand_bits(ls.iter().copied(), |_| false), 0);
    }

    /// The tables a callback and `state()` hand to Luau, field by field, as the reference page
    /// describes them. Nothing else runs these conversions without a pad in a hand.
    #[test]
    fn the_luau_tables_have_the_documented_shape() {
        let lua = Lua::new();
        let now = Instant::now();

        let down = event_table(&lua, &ev(1, PadEventKind::Down(Button::South), now), None, false, now + Duration::from_millis(7))
            .unwrap();
        assert_eq!(down.get::<String>("type").unwrap(), "down");
        assert_eq!(down.get::<String>("button").unwrap(), "south");
        assert_eq!(down.get::<String>("label").unwrap(), "A");
        assert_eq!(down.get::<i64>("pad").unwrap(), 1);
        assert_eq!(down.get::<i64>("age").unwrap(), 7);
        assert!(!down.get::<bool>("synthetic").unwrap());
        // `time` is on host.now()'s clock, which started no later than this test did.
        let time = down.get::<i64>("time").unwrap();
        assert!(time >= 0 && time <= clock_ms(Instant::now()));

        let axis = PadEventKind::Axis { axis: Axis::LeftY, value: -0.8, partner: 0.0 };
        let a = event_table(&lua, &ev(2, axis, now), Some(-0.73), false, now).unwrap();
        assert_eq!(a.get::<String>("type").unwrap(), "axis");
        assert_eq!(a.get::<String>("axis").unwrap(), "left_y");
        assert_eq!(a.get::<f64>("value").unwrap(), -0.73, "tidied, not 0.7300000190734863");
        assert_eq!(a.get::<f64>("raw").unwrap(), -0.8);

        let c = event_table(&lua, &ev(1, PadEventKind::Connected(info(1)), now), None, false, now).unwrap();
        assert_eq!(c.get::<String>("type").unwrap(), "connected");
        let i: Table = c.get("info").unwrap();
        assert_eq!(i.get::<i64>("index").unwrap(), 1);
        assert_eq!(i.get::<String>("family").unwrap(), "xbox");
        assert_eq!(i.get::<String>("source").unwrap(), "fake");
        assert!(i.get::<Option<i64>>("vendor").unwrap().is_none());
        let buttons: Vec<String> =
            i.get::<Table>("buttons").unwrap().sequence_values().collect::<mlua::Result<_>>().unwrap();
        assert!(buttons.contains(&"south".to_string()));
        assert!(buttons.contains(&"left_stick_up".to_string()), "derived buttons are listed too");

        let state = PadState {
            pad: 3,
            buttons: vec![Button::South, Button::East],
            pressed: [Button::South].into_iter().collect(),
            axes: vec![(Axis::LeftX, 0.25)],
            at: now,
        };
        let s = state_table(&lua, &state).unwrap();
        assert_eq!(s.get::<i64>("pad").unwrap(), 3);
        let b: Table = s.get("buttons").unwrap();
        assert!(b.get::<bool>("south").unwrap());
        assert!(!b.get::<bool>("east").unwrap(), "a released button is false, not nil");
        assert_eq!(s.get::<Table>("axes").unwrap().get::<f64>("left_x").unwrap(), 0.25);
    }

    #[test]
    fn options_are_checked_strictly() {
        let lua = Lua::new();
        let parse = |event: &str, src: &str| -> Result<PadFilter, String> {
            let opts: Option<Table> = if src.is_empty() { None } else { Some(lua.load(src).eval().unwrap()) };
            parse_filter(event, opts.as_ref()).map_err(|e| e.to_string())
        };
        let f = parse("down", "{ pad = 2, buttons = { 'dpad_up', 'south' }, maxAge = 250 }").unwrap();
        assert_eq!(f.pad, Some(2));
        assert_eq!(f.buttons, Some(vec![Button::DpadUp, Button::South]));
        assert_eq!(f.max_age, Duration::from_millis(250));
        let f = parse("axis", "{ axes = { 'left_x' }, deadzone = 0, step = 0.1 }").unwrap();
        assert_eq!(f.deadzone, Some(0.0));
        assert!((f.step - 0.1).abs() < 1e-6);
        assert!(parse("connected", "").unwrap().replay_pending);

        for (event, src, needle) in [
            ("press", "", "unknown event"),
            ("down", "{ buttons = { 'Cross' } }", "unknown button 'Cross'"),
            ("down", "{ button = { 'south' } }", "'button' is not an option"),
            ("down", "{ deadzone = 0.2 }", "'deadzone' is not an option"),
            ("axis", "{ axes = { 'throttle' } }", "unknown axis"),
            ("axis", "{ deadzone = 1 }", "deadzone"),
            ("down", "{ pad = 9 }", "pad must be"),
            ("down", "{ pad = 1.5 }", "pad must be"),
            ("down", "{ buttons = {} }", "empty"),
            ("down", "{ buttons = 'south' }", "list of names"),
            ("up", "{ maxAge = -1 }", "maxAge"),
            ("axis", "{ maxAge = 100 }", "'maxAge' is not an option"),
        ] {
            let err = parse(event, src).err().unwrap_or_else(|| panic!("{event} {src} was accepted"));
            assert!(err.contains(needle), "{event} {src}: {err}");
        }
    }
}
