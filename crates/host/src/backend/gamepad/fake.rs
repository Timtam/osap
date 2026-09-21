//! A scripted pad, for the tests. Nobody on this project has a controller at the desk, so this
//! is what the hub's rules are held to: it drives a LOCAL [`Hub`] — never the process-wide one,
//! because `cargo test` runs in parallel — through the same `connect`/`update`/`disconnect`
//! calls a real source makes, with timestamps a test chooses rather than whatever the clock
//! says, so an assertion about `at` is an assertion and not a race.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{names, Axis, Button, DeviceKey, Family, Hub, PadDesc, Snapshot, Source};

pub struct Fake {
    pub hub: Hub,
    t0: Instant,
    /// What each fake device is "holding" now; every change is sent as a whole report, the
    /// way a real source sends one.
    snaps: RefCell<HashMap<u32, Snapshot>>,
}

impl Fake {
    /// A fresh hub with these demand bits already set, as if listeners of those kinds existed.
    pub fn new(demand: u8) -> Fake {
        let hub = Hub::new();
        hub.set_demand(demand);
        Fake { hub, t0: Instant::now(), snaps: RefCell::new(HashMap::new()) }
    }

    /// `ms` milliseconds into the script.
    pub fn at(&self, ms: u64) -> Instant {
        self.t0 + Duration::from_millis(ms)
    }

    fn desc(id: u32) -> PadDesc {
        PadDesc {
            id: format!("fake:{id}"),
            name: format!("Fake pad {id}"),
            family: Family::Xbox,
            source: Source::Fake,
            mapped: true,
            vendor: None,
            product: None,
            buttons: names::PHYSICAL.to_vec(),
            axes: names::AXES.to_vec(),
        }
    }

    pub fn connect(&self, id: u32) -> Option<u8> {
        self.connect_holding(id, &[])
    }

    /// Plugs a pad in with these buttons already held and every axis at rest.
    pub fn connect_holding(&self, id: u32, held: &[Button]) -> Option<u8> {
        let snap = Snapshot {
            pressed: held.iter().copied().collect(),
            axes: names::AXES.iter().map(|a| (*a, 0.0)).collect(),
        };
        self.snaps.borrow_mut().insert(id, snap.clone());
        self.hub.connect(DeviceKey::Fake(id), Self::desc(id), &snap, self.at(0))
    }

    fn send(&self, id: u32, ms: u64, change: impl FnOnce(&mut Snapshot)) {
        let snap = {
            let mut snaps = self.snaps.borrow_mut();
            let s = snaps.entry(id).or_default();
            change(s);
            s.clone()
        };
        self.hub.update(&DeviceKey::Fake(id), &snap, self.at(ms));
    }

    pub fn press(&self, id: u32, b: Button, ms: u64) {
        self.send(id, ms, |s| {
            s.pressed.insert(b);
        });
    }

    pub fn release(&self, id: u32, b: Button, ms: u64) {
        self.send(id, ms, |s| {
            s.pressed.remove(&b);
        });
    }

    /// Exactly these buttons held, in whatever order the caller lists them.
    pub fn set_buttons(&self, id: u32, held: &[Button], ms: u64) {
        self.send(id, ms, |s| s.pressed = held.iter().copied().collect());
    }

    pub fn axis(&self, id: u32, axis: Axis, v: f32, ms: u64) {
        self.send(id, ms, |s| set_axis(s, axis, v));
    }

    /// Both halves of a stick in one report, as every real source sends them.
    pub fn stick(&self, id: u32, ax: Axis, ay: Axis, x: f32, y: f32, ms: u64) {
        self.send(id, ms, |s| {
            set_axis(s, ax, x);
            set_axis(s, ay, y);
        });
    }

    pub fn unplug(&self, id: u32, ms: u64) {
        self.snaps.borrow_mut().remove(&id);
        self.hub.disconnect(&DeviceKey::Fake(id), self.at(ms));
    }
}

fn set_axis(s: &mut Snapshot, axis: Axis, v: f32) {
    match s.axes.iter_mut().find(|(a, _)| *a == axis) {
        Some(slot) => slot.1 = v,
        None => s.axes.push((axis, v)),
    }
}
