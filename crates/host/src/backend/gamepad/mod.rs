//! Game controllers, observed from the background — the platform-neutral half.
//!
//! **Observe only.** Nothing here can stop the game from seeing a press, on either platform:
//! XInput and GameController read state, they do not own the device. Suppressing a pad would
//! take a filter driver and a virtual pad on Windows and the pad's exclusive seizure on macOS,
//! which the portable, no-installer rule of this project rules out. Every press reaches the
//! game as well as us, and the documentation says so where a module author will read it.
//!
//! **Why this lives beside the [`Backend`](super::Backend) trait, not inside it.** A pad shares
//! no state with windows, accessibility or keys, it is fed from a thread of its own on Windows
//! and from GameController's own queue on macOS, and keeping it out of the trait means a test
//! can drive the [`Hub`] directly (see `fake.rs`) without a whole fake backend. The one point
//! of contact with the event loop is [`drain_into`], which every backend's `pump_pending`
//! calls after keys and before the focus change.
//!
//! **The hub is a `Mutex`, never a thread-local.** The Windows key queues are thread-locals and
//! only work because the keyboard hook runs on the pump thread; a pad source runs on its own
//! thread (Windows) or on a dispatch queue (macOS), and a thread-local written there would be a
//! queue nobody ever drains. Every lock recovers from poisoning: a panic in a source must cost
//! that one report, not every report after it.
//!
//! **What is used where.** The lease, the synchronous refresh and the rebaseline exist for a
//! source that POLLS and parks — the Windows one. The macOS source is driven by callbacks and
//! needs none of them, so they carry `cfg_attr(not(windows), allow(dead_code))` rather than a
//! warning on every Mac build; and on a platform with no source at all (the stub) the whole
//! hub is unused and says so once, here.
#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use super::HostEvents;

pub mod names;

#[cfg(test)]
mod fake;

#[cfg(windows)]
mod win_thread;
#[cfg(windows)]
mod xinput;

#[cfg(target_os = "macos")]
mod apple;

/// Pads are numbered 1..=8, lowest free first. Eight is SDL's practical ceiling as well, and
/// it keeps `opts.pad` a small number somebody can say out loud.
pub const MAX_PADS: u8 = 8;

/// Button and connection events kept between two drains. A drain happens every 15 ms, so this
/// is only reached when the pump stood still — a modal dialog, a stalled callback.
///
/// Axis values are not counted and never dropped: they are coalesced to one per (pad, axis),
/// so there are at most eight pads' worth of them whatever happens, and each one is the only
/// record of where its stick went. Dropping one would leave a listener believing the stick is
/// where the previous value said, because the hub has already taken it as reported and would
/// not send it again until the stick moved.
pub const QUEUE_CAP: usize = 1024;

/// An axis change smaller than this is noise, not a movement. XInput sticks jitter by a few
/// counts at rest, and each count would otherwise be an event.
pub const NOISE_FLOOR: f32 = 0.01;

/// Derived stick directions: pressed at half deflection, released below 0.35. The gap is the
/// hysteresis that stops a stick held near the threshold from chattering down-up-down.
/// Tuned on paper only; see TODO.md — they want a real pad in a real hand.
pub const STICK_PRESS: f32 = 0.5;
pub const STICK_RELEASE: f32 = 0.35;
/// Derived trigger buttons: 0.12 is XInput's own `XINPUT_GAMEPAD_TRIGGER_THRESHOLD` (30/255).
pub const TRIGGER_PRESS: f32 = 0.12;
pub const TRIGGER_RELEASE: f32 = 0.08;

/// How long a `list()` or `state()` keeps the Windows source awake after it asked. Without
/// it a module that asks for the state every 200 ms would park and unpark the thread on every
/// call, each time re-reading all four XInput slots.
#[cfg_attr(not(windows), allow(dead_code))]
pub const LEASE: Duration = Duration::from_secs(5);

/// How long `list()`/`state()` wait for a parked source to read the pads. Short on purpose:
/// they run on the pump thread, and a stale answer is better than a stalled keyboard.
#[cfg_attr(not(windows), allow(dead_code))]
pub const REFRESH_WAIT: Duration = Duration::from_millis(50);

/// Which kinds of listener exist among the enabled modules, as bits. The sources read it to
/// decide how hard to work, and the hub reads it to decide what is worth queueing.
pub mod demand {
    pub const BUTTONS: u8 = 1;
    pub const AXES: u8 = 2;
    pub const CONNECT: u8 = 4;

    /// Whether somebody wants what a pad is DOING — its buttons or its axes — rather than only
    /// whether it is there. This is what makes a source read connected pads continuously (the
    /// Windows poll), and what the macOS App Nap activity is held for; a listener for
    /// `connected`/`disconnected` alone is served by the 2 s probe and the arrival notices, and
    /// must not cost the process a latency-critical activity or a 4 ms poll.
    pub fn wants_values(bits: u8) -> bool {
        bits & (BUTTONS | AXES) != 0
    }
}

/// A button, by position. See `names.rs` for the spelling and for what each is printed as.
///
/// The declaration order is SDL3's and it is load-bearing: the derived `Ord` decides the order
/// in which several edges found in one report are queued.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Button {
    South,
    East,
    West,
    North,
    Back,
    Guide,
    Start,
    LeftStick,
    RightStick,
    LeftShoulder,
    RightShoulder,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
    Misc1,
    RightPaddle1,
    LeftPaddle1,
    RightPaddle2,
    LeftPaddle2,
    Touchpad,
    // Derived from analog values by the hub, with hysteresis. No source reports these.
    LeftTrigger,
    RightTrigger,
    LeftStickUp,
    LeftStickDown,
    LeftStickLeft,
    LeftStickRight,
    RightStickUp,
    RightStickDown,
    RightStickLeft,
    RightStickRight,
    /// A HID button usage on a pad nobody mapped (the step-5 HID source).
    Other(u16),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Axis {
    LeftX,
    LeftY,
    RightX,
    RightY,
    LeftTrigger,
    RightTrigger,
    /// A HID Generic Desktop usage on an unmapped pad (the step-5 HID source).
    Other(u16),
}

/// What is printed on the pad, which decides the spoken labels. Only Xbox is produced on
/// Windows until the HID source exists; GameController reports all four.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Xbox,
    PlayStation,
    Nintendo,
    Generic,
}

/// Where a pad's reports come from.
///
/// Each platform constructs only its own source, `Hid` is the unbuilt step 5 and `Fake` is
/// the tests' — so on any one build most of these are never made, and saying so here is
/// better than a warning that teaches somebody to stop reading warnings.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    XInput,
    Hid,
    GameController,
    Fake,
}

/// A pad as its source describes it, before the hub gives it a number.
#[derive(Clone, Debug)]
pub struct PadDesc {
    /// Where the source sees this connection, NOT which device it is: `"xinput:0"` is XInput's
    /// first slot, whichever pad is in it, and `"gc:3"` is the third controller GameController
    /// reported this run — the same pad reconnected gets a new number.
    pub id: String,
    pub name: String,
    pub family: Family,
    pub source: Source,
    /// Whether the positional names are right for this pad. False only for an unmapped HID
    /// device, whose buttons are `button<N>`.
    pub mapped: bool,
    pub vendor: Option<u16>,
    pub product: Option<u16>,
    /// The PHYSICAL buttons it has; the derived ones follow from `axes`, see [`all_buttons`].
    pub buttons: Vec<Button>,
    pub axes: Vec<Axis>,
}

#[derive(Clone, Debug)]
pub struct PadInfo {
    pub index: u8,
    pub desc: PadDesc,
}

/// What one report says: every physical button that is down, and the normalised axes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub pressed: BTreeSet<Button>,
    pub axes: Vec<(Axis, f32)>,
}

/// A pad as `host.gamepad.state` returns it: raw values, never dead-zoned.
#[derive(Clone, Debug)]
pub struct PadState {
    pub pad: u8,
    /// Every button the pad has, derived ones included, so the table names the released ones
    /// too — a module asking "is south down?" should get `false`, not `nil`.
    pub buttons: Vec<Button>,
    pub pressed: BTreeSet<Button>,
    pub axes: Vec<(Axis, f32)>,
    /// When the source last reported anything for this pad.
    pub at: Instant,
}

#[derive(Clone, Debug)]
pub enum PadEventKind {
    Connected(PadInfo),
    Disconnected(PadInfo),
    Down(Button),
    Up(Button),
    /// `partner` is the other half of the stick at the same moment, carried in the event so
    /// a listener's radial dead zone is computed against the value that went with this one —
    /// not against whatever the hub holds by the time the pump gets to it.
    Axis { axis: Axis, value: f32, partner: f32 },
}

#[derive(Clone, Debug)]
pub struct PadEvent {
    pub pad: u8,
    pub kind: PadEventKind,
    /// When the source detected it. `time` and `age` in Luau are computed from this.
    pub at: Instant,
    /// Made up by the hub rather than reported: the ups sent for buttons still held when a
    /// pad disappears, and the `connected` replays a new listener receives.
    pub synthetic: bool,
    /// The pad's family, for the spoken label; the pad itself may be gone by the time a
    /// synthetic up is delivered.
    pub family: Family,
}

/// How a source names its device to the hub. Platform-specific in the same way as
/// [`Source`], and allowed to be unused for the same reason.
#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DeviceKey {
    XInput(u8),
    Hid(String),
    /// A monotonic id, never a pointer: an address can be reused by the next controller once
    /// the first is freed, and the hub would then update a pad that no longer exists.
    Apple(u64),
    Fake(u32),
}

/// What a source is asked to do from outside its own thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Control {
    /// The demand bits or the lease changed; decide again how hard to work.
    Demand,
    /// Read every pad now and call [`Hub::refresh_done`]. Asked of a polling source only.
    #[cfg_attr(not(windows), allow(dead_code))]
    Refresh,
}

type Waker = Arc<dyn Fn() + Send + Sync>;
type Controller = Arc<dyn Fn(Control) + Send + Sync>;

/// A lock that survives a panic elsewhere. A source that panicked mid-update left the state
/// no worse than one missing report, and refusing every later report over it would turn one
/// malformed report into a dead pad for the rest of the session.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Pad {
    key: DeviceKey,
    info: PadInfo,
    physical: BTreeSet<Button>,
    derived: BTreeSet<Button>,
    axes: Vec<(Axis, f32)>,
    /// The value each axis last produced an event with (or would have, without demand), so
    /// the noise floor measures movement since the last report rather than since the last
    /// poll — a slow drift of a few counts per poll still adds up to an event.
    reported: HashMap<Axis, f32>,
    at: Instant,
}

#[derive(Default)]
struct HubState {
    pads: Vec<Pad>,
    /// Button and connection events, in the order they happened.
    queue: VecDeque<PadEvent>,
    /// Axis events, one per (pad, axis): a newer value replaces the older one, so a stick
    /// swept across its range between two drains costs one event, not sixty.
    axes: Vec<PadEvent>,
    /// Downs the cap refused, so their ups are refused with them. See [`Hub::push_button`].
    dropped_downs: HashSet<(u8, Button)>,
    overflow_logged: bool,
    refused_logged: bool,
}

#[derive(Default)]
struct Refresh {
    asked: u64,
    done: u64,
}

/// Where every source puts what it saw, and where the pump takes it from.
pub struct Hub {
    state: Mutex<HubState>,
    demand: AtomicU8,
    /// Set when the main thread has been woken and has not drained since. The wake is
    /// coalesced through it: a pad mashed at 250 Hz wakes the main loop once per drain, not
    /// once per report, and a stick alone never wakes it at all — the 15 ms tick is soon
    /// enough for a value, and every wake runs a whole pump.
    wake_pending: AtomicBool,
    waker: Mutex<Option<Waker>>,
    control: Mutex<Option<Controller>>,
    refresh: (Mutex<Refresh>, Condvar),
    lease_until: Mutex<Option<Instant>>,
    /// What each source reports about itself, for `host.gamepad.status` and the probe.
    status: Mutex<Vec<(String, String)>>,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Hub {
        Hub {
            state: Mutex::new(HubState::default()),
            demand: AtomicU8::new(0),
            wake_pending: AtomicBool::new(false),
            waker: Mutex::new(None),
            control: Mutex::new(None),
            refresh: (Mutex::new(Refresh::default()), Condvar::new()),
            lease_until: Mutex::new(None),
            status: Mutex::new(Vec::new()),
        }
    }

    // ── Wiring, set once by the platform source ─────────────────────────────────────

    /// How to wake the main loop. Windows posts `WM_NULL` to the main thread; macOS sets
    /// none, because its loop turns every 15 ms whether or not anything arrived.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn set_waker(&self, f: impl Fn() + Send + Sync + 'static) {
        *lock(&self.waker) = Some(Arc::new(f));
    }

    /// How to reach the source from the main thread. See [`Control`].
    pub fn set_control(&self, f: impl Fn(Control) + Send + Sync + 'static) {
        *lock(&self.control) = Some(Arc::new(f));
    }

    fn notify(&self, what: Control) {
        let f = lock(&self.control).clone();
        if let Some(f) = f {
            f(what);
        }
    }

    pub fn set_status(&self, key: &str, value: impl Into<String>) {
        let value = value.into();
        let mut st = lock(&self.status);
        match st.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value,
            None => st.push((key.to_string(), value)),
        }
    }

    pub fn status(&self) -> Vec<(String, String)> {
        lock(&self.status).clone()
    }

    // ── Demand and lease, from the main thread ──────────────────────────────────────

    pub fn demand(&self) -> u8 {
        self.demand.load(Ordering::SeqCst)
    }

    pub fn set_demand(&self, bits: u8) {
        if self.demand.swap(bits, Ordering::SeqCst) != bits {
            self.notify(Control::Demand);
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn lease_active(&self, now: Instant) -> bool {
        lock(&self.lease_until).is_some_and(|t| now < t)
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn lease_until(&self) -> Option<Instant> {
        *lock(&self.lease_until)
    }

    /// Extends the lease. The source is told only when the lease starts, not on every call —
    /// it wakes at the end of the lease on its own and decides again then.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn touch_lease(&self, now: Instant, len: Duration) {
        let was_active = {
            let mut l = lock(&self.lease_until);
            let was = l.is_some_and(|t| now < t);
            *l = Some(now + len);
            was
        };
        if !was_active {
            self.notify(Control::Demand);
        }
    }

    /// Asks the source to read every pad now, and waits up to `timeout` for it. Returns
    /// whether the answer came in time; a late one still lands in the hub, it is merely not
    /// waited for.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn request_refresh(&self, timeout: Duration) -> bool {
        let want = {
            let mut r = lock(&self.refresh.0);
            r.asked += 1;
            r.asked
        };
        self.notify(Control::Refresh);
        let guard = lock(&self.refresh.0);
        let (guard, _) = self
            .refresh
            .1
            .wait_timeout_while(guard, timeout, |r| r.done < want)
            .unwrap_or_else(|e| e.into_inner());
        guard.done >= want
    }

    /// The newest refresh request, for the source to answer with [`Hub::refresh_done`].
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn refresh_asked(&self) -> u64 {
        lock(&self.refresh.0).asked
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn refresh_done(&self, upto: u64) {
        let mut r = lock(&self.refresh.0);
        if upto > r.done {
            r.done = upto;
        }
        self.refresh.1.notify_all();
    }

    // ── Sources, any thread ─────────────────────────────────────────────────────────

    /// A pad appeared. `baseline` is its state at that moment, and produces no button events:
    /// a button held while the pad was plugged in was not pressed now.
    ///
    /// Returns its index, or `None` when eight pads are already tracked. A second connect of
    /// the same device returns the index it already has, which is what lets a source call
    /// this from both an enumeration and an arrival notice without counting a pad twice.
    pub fn connect(&self, key: DeviceKey, desc: PadDesc, baseline: &Snapshot, at: Instant) -> Option<u8> {
        let (index, pushed, name, id) = {
            let mut st = lock(&self.state);
            if let Some(p) = st.pads.iter().find(|p| p.key == key) {
                return Some(p.info.index);
            }
            let Some(index) = (1..=MAX_PADS).find(|i| !st.pads.iter().any(|p| p.info.index == *i)) else {
                if !st.refused_logged {
                    st.refused_logged = true;
                    crate::logging::line(
                        "gamepad",
                        &format!(
                            "{} ({}) was ignored: {MAX_PADS} game controllers are already being watched",
                            desc.name, desc.id
                        ),
                    );
                }
                return None;
            };
            let info = PadInfo { index, desc };
            let family = info.desc.family;
            let (name, id) = (info.desc.name.clone(), info.desc.id.clone());
            let axes = baseline.axes.clone();
            st.pads.push(Pad {
                key,
                info: info.clone(),
                physical: baseline.pressed.clone(),
                derived: derive(&BTreeSet::new(), &axes),
                reported: axes.iter().copied().collect(),
                axes,
                at,
            });
            let ev = PadEvent { pad: index, kind: PadEventKind::Connected(info), at, synthetic: false, family };
            let pushed = Self::push_button(&mut st, ev);
            (index, pushed, name, id)
        };
        crate::logging::line("gamepad", &format!("pad {index} connected: {name} ({id})"));
        self.wake_if(pushed);
        Some(index)
    }

    /// A new report from a connected pad. Edges become events; axis changes become events
    /// only while somebody listens for axes.
    pub fn update(&self, key: &DeviceKey, snap: &Snapshot, at: Instant) {
        self.apply(key, snap, at, true);
    }

    /// Takes a report as the new truth WITHOUT producing events. For a source waking from a
    /// park: a button held across the park was not pressed at the moment it woke, and saying
    /// so would announce a press that happened minutes ago as happening now.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn rebaseline(&self, key: &DeviceKey, snap: &Snapshot, at: Instant) {
        self.apply(key, snap, at, false);
    }

    fn apply(&self, key: &DeviceKey, snap: &Snapshot, at: Instant, emit: bool) {
        let bits = self.demand();
        let buttons_on = emit && bits & demand::BUTTONS != 0;
        let axes_on = emit && bits & demand::AXES != 0;
        let pushed = {
            let mut st = lock(&self.state);
            let Some(i) = st.pads.iter().position(|p| &p.key == key) else {
                return;
            };
            let (index, family, edges, moved) = {
                let pad = &mut st.pads[i];
                pad.at = at;
                for (axis, v) in &snap.axes {
                    match pad.axes.iter_mut().find(|(a, _)| a == axis) {
                        Some(slot) => slot.1 = *v,
                        None => pad.axes.push((*axis, *v)),
                    }
                }
                let derived = derive(&pad.derived, &pad.axes);
                let old_physical = std::mem::replace(&mut pad.physical, snap.pressed.clone());
                let old_derived = std::mem::replace(&mut pad.derived, derived);

                // Every button whose state changed, physical and derived together, in the
                // canonical order (BTreeSet iterates sorted, and the enum is SDL's order).
                let before: BTreeSet<Button> = old_physical.union(&old_derived).copied().collect();
                let after: BTreeSet<Button> = pad.physical.union(&pad.derived).copied().collect();
                let edges: Vec<PadEventKind> = before
                    .symmetric_difference(&after)
                    .map(|b| if after.contains(b) { PadEventKind::Down(*b) } else { PadEventKind::Up(*b) })
                    .collect();

                let mut moved = Vec::new();
                for (axis, v) in &snap.axes {
                    let last = pad.reported.get(axis).copied().unwrap_or(0.0);
                    if axes_on && (v - last).abs() >= NOISE_FLOOR {
                        let partner = names::partner(*axis)
                            .and_then(|p| pad.axes.iter().find(|(a, _)| *a == p).map(|(_, v)| *v))
                            .unwrap_or(0.0);
                        moved.push(PadEventKind::Axis { axis: *axis, value: *v, partner });
                        pad.reported.insert(*axis, *v);
                    } else if !axes_on {
                        // Nobody listens, so the value that WOULD have been reported moves
                        // with the stick. When a listener arrives, its first event is a real
                        // movement from where the stick is, not a jump from where it was.
                        pad.reported.insert(*axis, *v);
                    }
                }
                (pad.info.index, pad.info.desc.family, edges, moved)
            };

            let mut pushed = false;
            if buttons_on {
                for kind in edges {
                    let ev = PadEvent { pad: index, kind, at, synthetic: false, family };
                    pushed |= Self::push_button(&mut st, ev);
                }
            }
            for kind in moved {
                let ev = PadEvent { pad: index, kind, at, synthetic: false, family };
                Self::push_axis(&mut st, ev);
            }
            pushed
        };
        self.wake_if(pushed);
    }

    /// A pad went away. Every button it still held gets a synthetic up first, so no module is
    /// left believing a button is down on a pad that no longer exists; then `disconnected`.
    pub fn disconnect(&self, key: &DeviceKey, at: Instant) {
        let bits = self.demand();
        let (index, name, pushed) = {
            let mut st = lock(&self.state);
            let Some(i) = st.pads.iter().position(|p| &p.key == key) else {
                return;
            };
            let pad = st.pads.remove(i);
            let index = pad.info.index;
            let family = pad.info.desc.family;
            st.axes.retain(|e| e.pad != index);
            let mut pushed = false;
            if bits & demand::BUTTONS != 0 {
                let held: BTreeSet<Button> = pad.physical.union(&pad.derived).copied().collect();
                for b in held {
                    let ev = PadEvent { pad: index, kind: PadEventKind::Up(b), at, synthetic: true, family };
                    pushed |= Self::push_button(&mut st, ev);
                }
            }
            st.dropped_downs.retain(|(p, _)| *p != index);
            let name = pad.info.desc.name.clone();
            let ev = PadEvent {
                pad: index,
                kind: PadEventKind::Disconnected(pad.info),
                at,
                synthetic: false,
                family,
            };
            pushed |= Self::push_button(&mut st, ev);
            (index, name, pushed)
        };
        crate::logging::line("gamepad", &format!("pad {index} disconnected: {name}"));
        self.wake_if(pushed);
    }

    /// Queues a button or connection event, keeping every down paired with its up.
    ///
    /// Only reached at the cap, and the rule there is about pairs, not about age. A DOWN is
    /// refused and remembered, so its UP is refused too; an UP whose down is still waiting
    /// takes that down with it; and an UP whose down was already delivered is queued past the
    /// cap, because refusing it would leave a module holding a button nobody is pressing. That
    /// last case is bounded by the number of buttons a pad has. Pending axis values are not
    /// touched — see [`QUEUE_CAP`].
    fn push_button(st: &mut HubState, ev: PadEvent) -> bool {
        if let PadEventKind::Up(b) = ev.kind {
            if st.dropped_downs.remove(&(ev.pad, b)) {
                return false;
            }
        }
        if st.queue.len() >= QUEUE_CAP {
            match ev.kind {
                PadEventKind::Down(b) => {
                    st.dropped_downs.insert((ev.pad, b));
                    Self::log_overflow(st, "dropped button presses together with their releases");
                    return false;
                }
                PadEventKind::Up(b) => {
                    let waiting = st
                        .queue
                        .iter()
                        .rposition(|e| e.pad == ev.pad && matches!(e.kind, PadEventKind::Down(x) if x == b));
                    if let Some(pos) = waiting {
                        st.queue.remove(pos);
                        Self::log_overflow(st, "dropped button presses together with their releases");
                        return false;
                    }
                }
                _ => {}
            }
        }
        st.queue.push_back(ev);
        true
    }

    /// Replaces the pending value of this (pad, axis), or adds it. Never refused: the list is
    /// bounded by pads times axes, and the hub has already counted this value as reported.
    fn push_axis(st: &mut HubState, ev: PadEvent) {
        let PadEventKind::Axis { axis, .. } = ev.kind else { return };
        if let Some(slot) = st
            .axes
            .iter_mut()
            .find(|e| e.pad == ev.pad && matches!(e.kind, PadEventKind::Axis { axis: a, .. } if a == axis))
        {
            *slot = ev;
            return;
        }
        st.axes.push(ev);
    }

    fn log_overflow(st: &mut HubState, what: &str) {
        if !st.overflow_logged {
            st.overflow_logged = true;
            crate::logging::line(
                "gamepad",
                &format!(
                    "{QUEUE_CAP} controller events are waiting and the pump has not taken them — {what}. \
                     host.gamepad.state() is still current."
                ),
            );
        }
    }

    fn wake_if(&self, pushed: bool) {
        if pushed && !self.wake_pending.swap(true, Ordering::SeqCst) {
            let w = lock(&self.waker).clone();
            if let Some(w) = w {
                w();
            }
        }
    }

    /// How many button and connection events are waiting, for the tests that fill the queue
    /// to its cap.
    #[cfg(test)]
    fn queued(&self) -> usize {
        lock(&self.state).queue.len()
    }

    // ── The host, main thread ───────────────────────────────────────────────────────

    /// Everything queued since the last drain: button and connection events in the order
    /// they happened, then the latest value of each axis that moved.
    pub fn drain(&self) -> Vec<PadEvent> {
        let mut st = lock(&self.state);
        // Cleared under the same lock that takes the events. A source that pushes after this
        // sees the flag clear and wakes the loop again; one that pushed before it has its event
        // taken here. Clearing outside the lock would leave a window where an event is queued,
        // the flag says a wake is pending, and none is.
        self.wake_pending.store(false, Ordering::SeqCst);
        let mut out: Vec<PadEvent> = st.queue.drain(..).collect();
        let mut axes = std::mem::take(&mut st.axes);
        axes.sort_by_key(|e| {
            let a = match e.kind {
                PadEventKind::Axis { axis, .. } => Some(axis),
                _ => None,
            };
            (e.pad, a)
        });
        out.extend(axes);
        st.overflow_logged = false;
        out
    }

    pub fn list(&self) -> Vec<PadInfo> {
        let st = lock(&self.state);
        let mut v: Vec<PadInfo> = st.pads.iter().map(|p| p.info.clone()).collect();
        v.sort_by_key(|i| i.index);
        v
    }

    pub fn state(&self, index: u8) -> Option<PadState> {
        let st = lock(&self.state);
        let p = st.pads.iter().find(|p| p.info.index == index)?;
        Some(PadState {
            pad: index,
            buttons: all_buttons(&p.info.desc),
            pressed: p.physical.union(&p.derived).copied().collect(),
            axes: p.axes.clone(),
            at: p.at,
        })
    }
}

/// Every button a pad can report, physical and derived: the derived ones follow from which
/// axes it has, so a pad without analog triggers does not list `left_trigger`.
pub fn all_buttons(desc: &PadDesc) -> Vec<Button> {
    let has = |a: Axis| desc.axes.contains(&a);
    let mut out = desc.buttons.clone();
    for b in names::DERIVED {
        let from = match b {
            Button::LeftTrigger => has(Axis::LeftTrigger),
            Button::RightTrigger => has(Axis::RightTrigger),
            Button::LeftStickUp | Button::LeftStickDown => has(Axis::LeftY),
            Button::LeftStickLeft | Button::LeftStickRight => has(Axis::LeftX),
            Button::RightStickUp | Button::RightStickDown => has(Axis::RightY),
            Button::RightStickLeft | Button::RightStickRight => has(Axis::RightX),
            _ => false,
        };
        if from {
            out.push(b);
        }
    }
    out
}

/// The derived buttons for these axis values, given which of them were held before.
///
/// Hysteresis per direction: a direction is pressed at [`STICK_PRESS`] and stays pressed until
/// it falls below [`STICK_RELEASE`]. Each direction is its own button, so a diagonal holds two
/// — the way a D-pad does — and a listener for `left_stick_down` never has to know about x.
fn derive(prev: &BTreeSet<Button>, axes: &[(Axis, f32)]) -> BTreeSet<Button> {
    let get = |a: Axis| axes.iter().find(|(x, _)| *x == a).map(|(_, v)| *v);
    let mut out = BTreeSet::new();
    let mut check = |b: Button, v: Option<f32>, press: f32, release: f32| {
        let Some(v) = v else { return };
        let held = prev.contains(&b);
        if (held && v >= release) || (!held && v >= press) {
            out.insert(b);
        }
    };
    let (lx, ly, rx, ry) = (get(Axis::LeftX), get(Axis::LeftY), get(Axis::RightX), get(Axis::RightY));
    check(Button::LeftStickUp, ly.map(|v| -v), STICK_PRESS, STICK_RELEASE);
    check(Button::LeftStickDown, ly, STICK_PRESS, STICK_RELEASE);
    check(Button::LeftStickLeft, lx.map(|v| -v), STICK_PRESS, STICK_RELEASE);
    check(Button::LeftStickRight, lx, STICK_PRESS, STICK_RELEASE);
    check(Button::RightStickUp, ry.map(|v| -v), STICK_PRESS, STICK_RELEASE);
    check(Button::RightStickDown, ry, STICK_PRESS, STICK_RELEASE);
    check(Button::RightStickLeft, rx.map(|v| -v), STICK_PRESS, STICK_RELEASE);
    check(Button::RightStickRight, rx, STICK_PRESS, STICK_RELEASE);
    check(Button::LeftTrigger, get(Axis::LeftTrigger), TRIGGER_PRESS, TRIGGER_RELEASE);
    check(Button::RightTrigger, get(Axis::RightTrigger), TRIGGER_PRESS, TRIGGER_RELEASE);
    out
}

// ── The process-wide hub and its lifecycle ───────────────────────────────────────────

static HUB: OnceLock<Hub> = OnceLock::new();
static STARTED: OnceLock<Result<(), String>> = OnceLock::new();

/// The one hub the sources feed. Tests never touch it: they build their own with
/// [`Hub::new`], because `cargo test` runs in parallel and a shared hub would let one test's
/// pads leak into another's assertions.
pub fn hub() -> &'static Hub {
    HUB.get_or_init(Hub::new)
}

/// Starts the platform's source, once. Called on the MAIN thread by the first `host.gamepad`
/// call — the Windows waker posts to the thread that calls this, and the macOS observers are
/// registered from it.
///
/// On Windows it returns once the first read of the four XInput slots is done, or after
/// 250 ms, whichever is first, so the `list()` that usually follows answers with the pads
/// that are there rather than with none.
///
/// A failure is kept and returned to every later call: it is a property of the machine, and
/// retrying on every call would only repeat the log line.
pub fn ensure_started() -> Result<(), String> {
    STARTED
        .get_or_init(|| {
            // Caught, because the first caller is a module's Luau call and the source's start
            // talks to the OS: on macOS an Objective-C property that unexpectedly answers nil is
            // a panic in objc2, and it should cost the controllers — logged below as a failure
            // like any other — not the module that asked.
            let r = std::panic::catch_unwind(|| platform_start(hub())).unwrap_or_else(|p| {
                let text = p
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "no message".to_string());
                Err(format!("the source panicked while starting: {text}"))
            });
            match &r {
                Ok(()) => crate::logging::line("gamepad", "gamepad watcher started"),
                // The exact words the CI probe step looks for.
                Err(e) => crate::logging::line("gamepad", &format!("gamepad watcher failed: {e}")),
            }
            r
        })
        .clone()
}

/// Whether a source is running. Nothing is drained, and no demand is sent, before it is.
pub fn started() -> bool {
    matches!(STARTED.get(), Some(Ok(())))
}

/// How starting went — `None` while nothing has asked yet. For `host.gamepad.status`, which
/// reports without starting anything.
pub fn start_outcome() -> Option<Result<(), String>> {
    STARTED.get().cloned()
}

fn platform_start(hub: &'static Hub) -> Result<(), String> {
    #[cfg(windows)]
    {
        win_thread::start(hub)
    }
    #[cfg(target_os = "macos")]
    {
        apple::start(hub)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = hub;
        Err(format!("game controllers are not supported on {}", std::env::consts::OS))
    }
}

/// Whether a polling source is reading its connected pads continuously right now, so that
/// what the hub holds is at most one poll old: somebody wants values, or a lease runs. A
/// `connected`-only listener does NOT count — it is served by the 2 s probe, and the hub's
/// buttons may then be two seconds stale. [`refresh_if_idle`] asks for a fresh read exactly
/// when this is false.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn reading_continuously(bits: u8, lease: bool) -> bool {
    demand::wants_values(bits) || lease
}

/// For `list()` and `state()`: when nothing keeps the source reading, ask it to read once
/// now and keep reading for [`LEASE`].
///
/// Windows only in effect. The macOS source is driven by GameController's own callbacks and
/// its state is current whenever a callback has run, so there is nothing to ask for.
pub fn refresh_if_idle() {
    if !started() {
        return;
    }
    #[cfg(windows)]
    {
        let hub = hub();
        let now = Instant::now();
        let idle = !reading_continuously(hub.demand(), hub.lease_active(now));
        // The lease first, so the source sees it when it decides what to do after answering.
        hub.touch_lease(now, LEASE);
        if idle && !hub.request_refresh(REFRESH_WAIT) {
            crate::logging::trace("gamepad", || {
                format!(
                    "a parked source did not answer within {} ms; list()/state() used what the hub had",
                    REFRESH_WAIT.as_millis()
                )
            });
        }
    }
}

/// Hands whatever the sources queued to the host. Called by every backend's `pump_pending`,
/// after keys and before the focus change — a press should be handled against the window that
/// is in front now, and the activations are already through by then.
pub fn drain_into(events: &mut dyn HostEvents) {
    if !started() {
        return;
    }
    let batch = hub().drain();
    if !batch.is_empty() {
        events.on_gamepad(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Fake;
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn kinds(events: &[PadEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| match &e.kind {
                PadEventKind::Connected(_) => format!("{} connected", e.pad),
                PadEventKind::Disconnected(_) => format!("{} disconnected", e.pad),
                PadEventKind::Down(b) => format!("{} down {}", e.pad, names::button_name(*b)),
                PadEventKind::Up(b) => {
                    format!("{} up {}{}", e.pad, names::button_name(*b), if e.synthetic { " (synthetic)" } else { "" })
                }
                PadEventKind::Axis { axis, value, .. } => {
                    format!("{} axis {} {value:.2}", e.pad, names::axis_name(*axis))
                }
            })
            .collect()
    }

    #[test]
    fn a_connect_is_announced_and_is_a_baseline() {
        let f = Fake::new(demand::BUTTONS);
        // South is already held when the pad is plugged in: that is not a press.
        assert_eq!(f.connect_holding(1, &[Button::South]), Some(1));
        assert_eq!(kinds(&f.hub.drain()), vec!["1 connected"]);
        f.release(1, Button::South, 10);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 up south"]);
    }

    #[test]
    fn edges_come_out_in_order() {
        let f = Fake::new(demand::BUTTONS);
        f.connect(1);
        f.hub.drain();
        f.press(1, Button::DpadDown, 5);
        f.release(1, Button::DpadDown, 9);
        f.press(1, Button::South, 12);
        // Two buttons in ONE report come out in the canonical order, whatever order the
        // source listed them in.
        f.set_buttons(1, &[Button::South, Button::Start, Button::East], 20);
        assert_eq!(
            kinds(&f.hub.drain()),
            vec!["1 down dpad_down", "1 up dpad_down", "1 down south", "1 down east", "1 down start"]
        );
        // And a report that changes nothing produces nothing.
        f.set_buttons(1, &[Button::South, Button::Start, Button::East], 24);
        assert!(f.hub.drain().is_empty());
    }

    #[test]
    fn stick_directions_have_hysteresis_and_diagonals_press_two() {
        let f = Fake::new(demand::BUTTONS);
        f.connect(1);
        f.hub.drain();
        // y negative is UP.
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.0, -0.45, 1);
        assert!(f.hub.drain().is_empty(), "0.45 is below the press threshold");
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.0, -0.55, 2);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 down left_stick_up"]);
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.0, -0.40, 3);
        assert!(f.hub.drain().is_empty(), "0.40 is still above the release threshold");
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.0, -0.30, 4);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 up left_stick_up"]);

        f.stick(1, Axis::LeftX, Axis::LeftY, 0.6, 0.6, 5);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 down left_stick_down", "1 down left_stick_right"]);
    }

    #[test]
    fn triggers_become_buttons_with_their_own_thresholds() {
        let f = Fake::new(demand::BUTTONS);
        f.connect(1);
        f.hub.drain();
        for (v, t) in [(0.10, 1), (0.13, 2), (0.09, 3), (0.07, 4)] {
            f.axis(1, Axis::RightTrigger, v, t);
        }
        assert_eq!(kinds(&f.hub.drain()), vec!["1 down right_trigger", "1 up right_trigger"]);
    }

    #[test]
    fn axis_events_exist_only_while_somebody_wants_axes() {
        let f = Fake::new(demand::BUTTONS);
        f.connect(1);
        f.hub.drain();
        f.axis(1, Axis::RightX, 0.3, 1);
        assert!(f.hub.drain().is_empty(), "no AXES demand, no axis event");
        // The state is current regardless.
        let s = f.hub.state(1).unwrap();
        assert_eq!(s.axes.iter().find(|(a, _)| *a == Axis::RightX).unwrap().1, 0.3);

        f.hub.set_demand(demand::BUTTONS | demand::AXES);
        f.axis(1, Axis::RightX, 0.305, 2);
        assert!(f.hub.drain().is_empty(), "the first event after demand is a movement from HERE");
        // 0.45 rather than anything past 0.5, which would also press right_stick_right.
        f.axis(1, Axis::RightX, 0.45, 3);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 axis right_x 0.45"]);
    }

    #[test]
    fn axis_values_are_coalesced_and_small_changes_are_noise() {
        let f = Fake::new(demand::AXES);
        f.connect(1);
        f.hub.drain();
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.2, 0.0, 1);
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.4, 0.0, 2);
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.6, 0.1, 3);
        let got = f.hub.drain();
        assert_eq!(kinds(&got), vec!["1 axis left_x 0.60", "1 axis left_y 0.10"]);
        // The partner travels with the value: left_x's event carries left_y as it was then.
        match got[0].kind {
            PadEventKind::Axis { partner, .. } => assert!((partner - 0.1).abs() < 1e-6),
            _ => unreachable!(),
        }
        // The latest time, not the first.
        assert_eq!(got[0].at, f.at(3));

        f.stick(1, Axis::LeftX, Axis::LeftY, 0.605, 0.1, 4);
        assert!(f.hub.drain().is_empty(), "0.005 is below the noise floor");
        // But the drift is measured from the last REPORTED value, so it adds up.
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.611, 0.1, 5);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 axis left_x 0.61"]);
    }

    #[test]
    fn the_queue_cap_keeps_pairs_whole() {
        let f = Fake::new(demand::BUTTONS | demand::AXES);
        f.connect(1);
        f.hub.drain();
        f.axis(1, Axis::LeftTrigger, 0.05, 0);
        // Fill the queue to the cap with complete presses of south: 512 downs and 512 ups,
        // plus the one pending axis value.
        let mut t = 1;
        for _ in 0..(QUEUE_CAP / 2) {
            f.press(1, Button::South, t);
            f.release(1, Button::South, t + 1);
            t += 2;
        }
        // At the cap now: new presses are refused WITH their releases, never one without the
        // other — and the pending axis value is kept, it is not what the cap is for.
        f.press(1, Button::East, t);
        f.press(1, Button::North, t + 1);
        f.release(1, Button::East, t + 2);
        f.release(1, Button::North, t + 3);
        let got = f.hub.drain();
        let buttons = got.iter().filter(|e| !matches!(e.kind, PadEventKind::Axis { .. })).count();
        assert!(buttons <= QUEUE_CAP, "{buttons} button events");
        assert_eq!(kinds(&got[got.len() - 1..]), vec!["1 axis left_trigger 0.05"]);
        let mut held: HashSet<Button> = HashSet::new();
        for e in &got {
            match e.kind {
                PadEventKind::Down(b) => assert!(held.insert(b), "two downs of {b:?}"),
                PadEventKind::Up(b) => assert!(held.remove(&b), "an up of {b:?} without its down"),
                _ => {}
            }
        }
        assert!(held.is_empty(), "downs without ups: {held:?}");
        assert!(f.hub.state(1).unwrap().pressed.is_empty(), "the state is right regardless");
    }

    /// The review's case B: the pump stalls, the button queue fills, and the stick is let go
    /// meanwhile. The hub counts the return to rest as reported the moment it queues it, so if
    /// the cap dropped that value nothing would ever send it again and a listener would keep
    /// the stick at 0.8.
    #[test]
    fn a_stick_let_go_during_a_stall_is_still_reported() {
        let f = Fake::new(demand::BUTTONS | demand::AXES);
        f.connect(1);
        f.hub.drain();
        f.axis(1, Axis::RightX, 0.45, 1);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 axis right_x 0.45"]);
        let mut t = 2;
        while f.hub.queued() < QUEUE_CAP {
            f.press(1, Button::South, t);
            f.release(1, Button::South, t + 1);
            t += 2;
        }
        // Moved and let go while the queue is full: a new (pad, axis) entry at the cap...
        f.axis(1, Axis::RightX, 0.3, t);
        // ...and a button event after it, which used to clear the pending values to make room.
        f.press(1, Button::East, t + 1);
        f.axis(1, Axis::RightX, 0.0, t + 2);
        f.press(1, Button::West, t + 3);
        let got = f.hub.drain();
        let axes: Vec<String> =
            kinds(&got).into_iter().filter(|k| k.contains("axis")).collect();
        assert_eq!(axes, vec!["1 axis right_x 0.00"]);
    }

    #[test]
    fn reading_continuously_needs_values_or_a_lease() {
        // The review's finding 3: a connected-only listener leaves the Windows source probing
        // every 2 s, so `state()` must still ask for a fresh read then.
        assert!(!reading_continuously(0, false));
        assert!(!reading_continuously(demand::CONNECT, false), "connections alone are the 2 s probe");
        assert!(reading_continuously(demand::CONNECT | demand::BUTTONS, false));
        assert!(reading_continuously(demand::AXES, false));
        assert!(reading_continuously(demand::CONNECT, true), "a lease polls");
        assert!(!demand::wants_values(demand::CONNECT));
        assert!(demand::wants_values(demand::BUTTONS));
    }

    #[test]
    fn an_up_whose_down_was_delivered_is_queued_past_the_cap() {
        let f = Fake::new(demand::BUTTONS);
        f.connect(1);
        f.hub.drain();
        f.press(1, Button::West, 1);
        assert_eq!(kinds(&f.hub.drain()), vec!["1 down west"]);
        let mut t = 2;
        while f.hub.queued() < QUEUE_CAP {
            f.press(1, Button::South, t);
            f.release(1, Button::South, t + 1);
            t += 2;
        }
        f.release(1, Button::West, t);
        let got = f.hub.drain();
        assert_eq!(got.len(), QUEUE_CAP + 1);
        assert_eq!(kinds(&got[QUEUE_CAP..]), vec!["1 up west"]);
    }

    #[test]
    fn the_wake_is_coalesced_and_a_stick_never_wakes() {
        let f = Fake::new(demand::BUTTONS | demand::AXES);
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        f.hub.set_waker(move || {
            w.fetch_add(1, Ordering::SeqCst);
        });
        f.connect(1);
        assert_eq!(woken.load(Ordering::SeqCst), 1, "a connect wakes");
        f.press(1, Button::South, 1);
        f.release(1, Button::South, 2);
        assert_eq!(woken.load(Ordering::SeqCst), 1, "no second wake before a drain");
        f.hub.drain();
        // Below the derived-button threshold, so this is a value and nothing else.
        f.axis(1, Axis::LeftX, 0.3, 3);
        assert_eq!(woken.load(Ordering::SeqCst), 1, "axis values wait for the tick");
        f.press(1, Button::South, 4);
        assert_eq!(woken.load(Ordering::SeqCst), 2, "the first press after a drain wakes again");
    }

    #[test]
    fn indexes_are_the_lowest_free_and_are_reused() {
        let f = Fake::new(demand::CONNECT);
        assert_eq!(f.connect(10), Some(1));
        assert_eq!(f.connect(11), Some(2));
        assert_eq!(f.connect(12), Some(3));
        assert_eq!(f.connect(11), Some(2), "the same device twice keeps its index");
        f.unplug(11, 5);
        assert_eq!(f.connect(13), Some(2), "a gap is filled before the end is extended");
        for id in 14..=18 {
            assert!(f.connect(id).is_some());
        }
        assert_eq!(f.hub.list().len(), MAX_PADS as usize);
        assert_eq!(f.connect(19), None, "a ninth pad is refused");
        let idx: Vec<u8> = f.hub.list().iter().map(|i| i.index).collect();
        assert_eq!(idx, (1..=MAX_PADS).collect::<Vec<u8>>());
    }

    #[test]
    fn unplugging_releases_everything_it_held() {
        let f = Fake::new(demand::BUTTONS | demand::AXES);
        f.connect(1);
        f.press(1, Button::South, 1);
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.0, -0.9, 2);
        f.hub.drain();
        f.stick(1, Axis::LeftX, Axis::LeftY, 0.1, -0.9, 3);
        f.unplug(1, 4);
        let got = f.hub.drain();
        assert_eq!(
            kinds(&got),
            vec!["1 up south (synthetic)", "1 up left_stick_up (synthetic)", "1 disconnected"],
            "the pending axis value of a pad that is gone is dropped too"
        );
        assert!(f.hub.state(1).is_none());
        assert!(f.hub.list().is_empty());
    }

    #[test]
    fn a_rebaseline_moves_the_state_without_events() {
        let f = Fake::new(demand::BUTTONS);
        f.connect(1);
        f.hub.drain();
        let mut snap = Snapshot::default();
        snap.pressed.insert(Button::North);
        f.hub.rebaseline(&DeviceKey::Fake(1), &snap, f.at(1));
        assert!(f.hub.drain().is_empty());
        assert!(f.hub.state(1).unwrap().pressed.contains(&Button::North));
        f.hub.update(&DeviceKey::Fake(1), &Snapshot::default(), f.at(2));
        assert_eq!(kinds(&f.hub.drain()), vec!["1 up north"]);
    }

    #[test]
    fn a_refresh_is_answered_or_times_out() {
        let hub = Arc::new(Hub::new());
        // Nobody answers: the call gives up after its timeout rather than hanging the pump.
        assert!(!hub.request_refresh(Duration::from_millis(5)));
        // A source that answers from another thread, as the Windows one does.
        let h = hub.clone();
        hub.set_control(move |c| {
            if c == Control::Refresh {
                let h = h.clone();
                std::thread::spawn(move || h.refresh_done(h.refresh_asked()));
            }
        });
        assert!(hub.request_refresh(Duration::from_secs(5)));
    }

    #[test]
    fn demand_changes_reach_the_source_once() {
        let hub = Hub::new();
        let told = Arc::new(AtomicUsize::new(0));
        let t = told.clone();
        hub.set_control(move |c| {
            if c == Control::Demand {
                t.fetch_add(1, Ordering::SeqCst);
            }
        });
        hub.set_demand(demand::BUTTONS);
        hub.set_demand(demand::BUTTONS);
        hub.set_demand(0);
        assert_eq!(told.load(Ordering::SeqCst), 2);
        let now = Instant::now();
        hub.touch_lease(now, LEASE);
        hub.touch_lease(now + Duration::from_millis(10), LEASE);
        assert_eq!(told.load(Ordering::SeqCst), 3, "only the start of a lease is news");
        assert!(hub.lease_active(now + Duration::from_secs(1)));
        assert!(!hub.lease_active(now + LEASE + Duration::from_secs(1)));
    }

    #[test]
    fn derived_buttons_follow_the_axes_a_pad_has() {
        let desc = PadDesc {
            id: "x".into(),
            name: "x".into(),
            family: Family::Generic,
            source: Source::Fake,
            mapped: true,
            vendor: None,
            product: None,
            buttons: vec![Button::South],
            axes: vec![Axis::LeftX, Axis::LeftY],
        };
        let all = all_buttons(&desc);
        assert!(all.contains(&Button::LeftStickUp));
        assert!(!all.contains(&Button::LeftTrigger), "no trigger axis, no trigger button");
        assert!(!all.contains(&Button::RightStickUp));
    }
}
