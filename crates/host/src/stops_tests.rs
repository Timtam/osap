//! The stop's two steps against a recording host — what happens at once and what waits for the
//! end of the turn — and its words: the sentence, the dialog, the log line, the row's note, a
//! failed load's message and the manager's details, each the one the docs show. For both limits:
//! memory (one module) and time (one module, a callback inside another's, a library and the
//! modules that run its code). The modules are the docs' examples — Game helper, and the Overlay
//! kit library with Synth overlay and Sampler overlay — not the guard probe's, so the docs are
//! never read against what the probe says.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use crate::stops::{self, Details, Dropped, StopEvent, StopHost, Stops};
use crate::vm_guard::{Cause, Clock, CodeShare, EntryKind, GroupMember, Location, Outer, Trip, VmInfo};

/// Everything the host was asked, in order.
#[derive(Default)]
struct Rec {
    stops: Stops,
    enabled: RefCell<Vec<bool>>,
    /// While set, the enabled flag cannot be written (borrowed further up, in the host).
    busy: Cell<bool>,
    /// The buttons each module holds, and the input queued during the stall.
    held: RefCell<Vec<(usize, &'static str)>>,
    queued: RefCell<Dropped>,
    /// A step of handing things back to the OS that panics.
    panics_in: Cell<Option<&'static str>>,
    calls: RefCell<Vec<String>>,
}

impl Rec {
    fn with(n: usize) -> Rec {
        let r = Rec::default();
        *r.enabled.borrow_mut() = vec![true; n];
        r
    }
    fn calls(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.borrow_mut())
    }
    fn note(&self, s: String) {
        self.calls.borrow_mut().push(s);
    }
    fn maybe_panic(&self, step: &'static str) {
        if self.panics_in.get() == Some(step) {
            crate::quiet_expected_panics();
            panic!("{}{step}", crate::EXPECTED_PANIC);
        }
    }
}

impl StopHost for Rec {
    fn stops(&self) -> &Stops {
        &self.stops
    }
    fn module_enabled(&self, idx: usize) -> bool {
        self.enabled.borrow()[idx]
    }
    fn flip_off(&self, idx: usize) -> bool {
        if self.busy.get() {
            self.note(format!("flip {idx} waits"));
            return false;
        }
        self.enabled.borrow_mut()[idx] = false;
        self.note(format!("flip {idx}"));
        true
    }
    fn refresh_keys(&self) {
        self.note("keys".into());
        self.maybe_panic("keys");
    }
    fn release_buttons(&self, idx: usize) -> Vec<&'static str> {
        self.maybe_panic("release");
        let mut held = self.held.borrow_mut();
        let mine: Vec<&'static str> = held.iter().filter(|(i, _)| *i == idx).map(|(_, b)| *b).collect();
        held.retain(|(i, _)| *i != idx);
        self.note(format!("release {idx} {mine:?}"));
        mine
    }
    fn drop_queued_input(&self) -> Dropped {
        self.note("drop".into());
        self.maybe_panic("drop");
        std::mem::take(&mut *self.queued.borrow_mut())
    }
    fn after_disable(&self, idx: usize) {
        self.note(format!("disable {idx}"));
    }
    fn collect_garbage(&self, serial: u64) {
        self.note(format!("gc {serial}"));
    }
    fn log(&self, category: &str, line: &str) {
        self.note(format!("log {category} {line}"));
    }
    fn dialog(&self, key: String, title: String, text: String) {
        self.note(format!("dialog {} | {title} | {}", key.replace('\u{1}', "/"), text.len()));
    }
    fn announce(&self, text: &str) {
        self.note(format!("say {text}"));
    }
    fn row_changed(&self, idx: usize, note: Option<String>) {
        self.note(format!("row {idx} {note:?}"));
    }
}

fn share(id: &str, name: &str, mib: u32, declared: bool) -> CodeShare {
    CodeShare { id: id.into(), name: name.into(), mib, declared }
}

/// Game helper's VM: its own 256 MiB and nothing else.
fn game_vm(idx: usize) -> VmInfo {
    VmInfo { module: Some(idx), id: "com.example.game".into(), name: "Game helper".into(), code: vec![share("com.example.game", "Game helper", 256, false)] }
}

/// Synth overlay's VM: 300 of its own, set in its manifest, and the Overlay kit library's 256.
fn synth_vm(idx: usize) -> VmInfo {
    VmInfo {
        module: Some(idx),
        id: "com.example.synth".into(),
        name: "Synth overlay".into(),
        code: vec![share("com.example.synth", "Synth overlay", 300, true), share("com.example.kit", "Overlay kit", 256, false)],
    }
}

/// The Overlay kit library's own VM.
fn kit_vm(idx: usize) -> VmInfo {
    VmInfo { module: Some(idx), id: "com.example.kit".into(), name: "Overlay kit".into(), code: vec![share("com.example.kit", "Overlay kit", 256, false)] }
}

/// Sampler overlay's VM, which runs the library's code too.
fn sampler_vm(idx: usize) -> VmInfo {
    VmInfo {
        module: Some(idx),
        id: "com.example.sampler".into(),
        name: "Sampler overlay".into(),
        code: vec![share("com.example.sampler", "Sampler overlay", 256, false), share("com.example.kit", "Overlay kit", 256, false)],
    }
}

fn member(vm: &VmInfo) -> GroupMember {
    GroupMember { module: vm.module, id: vm.id.clone(), name: vm.name.clone(), loading: false }
}

fn trip(vm: VmInfo, what: &str, outer: Option<Outer>, used: u64) -> Trip {
    Trip {
        serial: 9,
        cause: Cause::Memory { limit_mib: vm.limit_mib(), used },
        group: vec![member(&vm)],
        vm,
        loading: false,
        kind: EntryKind::Handler,
        what: what.into(),
        outer,
        traceback: None,
        call: None,
        frames: Vec::new(),
        location: None,
        culprit: None,
    }
}

/// A time stop of `vm`'s `what` callback, 2.003 s of processor time and 2.11 s in all, at
/// `rel:line` of `id`, with these frames.
fn time_trip(vm: VmInfo, what: &str, at: (&str, &str, usize), frames: &[&str]) -> Trip {
    Trip {
        cause: Cause::Time { clock: Clock::Cpu, cpu: Duration::from_millis(2003), wall: Duration::from_millis(2110) },
        frames: frames.iter().map(|f| f.to_string()).collect(),
        location: Some(Location { id: at.0.into(), rel: at.1.into(), line: at.2 }),
        ..trip(vm, what, None, 0)
    }
}

fn event(t: Trip) -> StopEvent {
    let m = stops::Member { idx: t.vm.module.unwrap(), id: t.vm.id.clone(), name: t.vm.name.clone() };
    StopEvent { seq: 1, at: Instant::now(), trip: t, members: vec![m], released: Vec::new(), dropped: Dropped::default() }
}

/// Game helper's hotkey, run when its module took the slot of another's overlay.
fn game_hotkey() -> Outer {
    Outer { module: Some(0), id: "com.example.game".into(), name: "Game helper".into(), kind: EntryKind::Handler, what: "hotkey".into(), same_vm: false }
}

/// The library's stop, in Synth overlay's hotkey callback: the library, Sampler overlay and Synth
/// overlay turned off — the library first, then by name, as `mark` orders them.
fn kit_stop() -> StopEvent {
    let mut t = time_trip(
        synth_vm(1),
        "hotkey",
        ("com.example.kit", "src/main.luau", 9),
        &["com.example.kit/src/main.luau:9: in function 'spin'", "com.example.synth/src/main.luau:41: in function <com.example.synth/src/main.luau:40>"],
    );
    t.culprit = Some(("com.example.kit".into(), "Overlay kit".into()));
    t.group = vec![member(&synth_vm(1)), member(&kit_vm(0)), member(&sampler_vm(2))];
    let mut ev = event(t);
    ev.members = vec![
        stops::Member { idx: 0, id: "com.example.kit".into(), name: "Overlay kit".into() },
        stops::Member { idx: 2, id: "com.example.sampler".into(), name: "Sampler overlay".into() },
        stops::Member { idx: 1, id: "com.example.synth".into(), name: "Synth overlay".into() },
    ];
    ev
}

const MIB: u64 = 1 << 20;

#[test]
fn a_stop_turns_the_module_off_at_once_and_settles_after() {
    let h = Rec::with(3);
    h.held.borrow_mut().push((1, "left"));
    *h.queued.borrow_mut() = Dropped { keys: 2, hotkeys: 0, names: vec!["Tab \u{d7}2".into()] };
    stops::mark(&h, vec![trip(game_vm(1), "hotkey", None, 256 * MIB - MIB / 10)]);
    let calls = h.calls();
    assert_eq!(&calls[..4], ["flip 1", "keys", "drop", "release 1 [\"left\"]"], "{calls:?}");
    assert!(calls[4].starts_with("log guard [com.example.game] stopped: its hotkey callback"), "{calls:?}");
    assert!(calls[4].ends_with("; released: left mouse button of Game helper; dropped: 2 key(s), 0 hotkey press(es)"), "{calls:?}");
    assert!(calls[5].starts_with("log keys 2 key(s) and 0 hotkey press(es) made while Game helper's hotkey callback held"), "{calls:?}");
    assert_eq!(calls.len(), 6, "nothing else at once: {calls:?}");
    assert!(h.stops.is_stopped(1) && !h.enabled.borrow()[1] && h.stops.has_unsettled());
    assert_eq!(h.stops.row_note(1).as_deref(), Some("stopped: its hotkey callback needed more than 256 MiB of memory"));

    stops::settle(&h);
    let calls = h.calls();
    assert_eq!(calls[0], "disable 1");
    assert_eq!(calls[1], "gc 9");
    assert!(calls[2].starts_with("dialog com.example.game/stop/1 | Module stopped: com.example.game |"), "{calls:?}");
    assert!(calls[3].starts_with("say Stopped Game helper: "), "{calls:?}");
    assert_eq!(calls[4], "row 1 Some(\"stopped: its hotkey callback needed more than 256 MiB of memory\")");
    assert_eq!(calls.len(), 5);
    stops::settle(&h);
    assert!(h.calls().is_empty(), "settled once");
    assert!(h.stops.is_stopped(1), "and stopped until turned on");
}

/// A panic while the keys are handed back, the queued input dropped or a button let go of is
/// logged, and the rest goes on: the module is turned off with its record, so it is reported and
/// not stored off unexplained.
#[test]
fn a_panic_handing_things_back_loses_no_stop() {
    for step in ["keys", "drop", "release"] {
        let h = Rec::with(1);
        h.held.borrow_mut().push((0, "left"));
        h.panics_in.set(Some(step));
        stops::mark(&h, vec![trip(game_vm(0), "hotkey", None, 0)]);
        let calls = h.calls();
        assert!(calls.iter().any(|c| c.starts_with("log guard the stop could not ")), "{step}: {calls:?}");
        assert!(h.stops.is_stopped(0) && h.stops.has_unsettled(), "{step}: no record");
        assert!(calls.iter().any(|c| c.starts_with("log guard [com.example.game] stopped:")), "{step}: no log line");
    }
}

#[test]
fn a_module_that_was_off_gets_a_line_and_a_load_gets_nothing() {
    let h = Rec::with(2);
    h.enabled.borrow_mut()[0] = false;
    stops::mark(&h, vec![trip(game_vm(0), "settings onChange (volume)", None, 255 * MIB)]);
    let calls = h.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        calls[0],
        "log guard [com.example.game] its settings onChange (volume) callback needed more than the 256 MiB of memory \
         its VM may use (255.0 MiB in use); it was off already, and runs none of its code until it is turned on again"
    );
    assert!(!h.stops.is_stopped(0));
    let mut loading = trip(game_vm(1), "its entry file", None, 255 * MIB);
    loading.loading = true;
    loading.kind = EntryKind::Load;
    stops::mark(&h, vec![loading.clone()]);
    assert!(h.calls().is_empty() && !h.stops.is_stopped(1), "a load fails with its own message");
    assert_eq!(
        stops::load_failure(Some(&loading)),
        "stopped while loading: its entry file needed more than the 256 MiB of memory it may use"
    );
    // And a time stop while loading.
    let mut t = time_trip(game_vm(1), "its entry file", ("com.example.game", "src/main.luau", 3), &[]);
    t.kind = EntryKind::Load;
    assert_eq!(
        stops::load_failure(Some(&t)),
        "stopped while loading: its entry file ran for 2 seconds of processor time without returning, at src/main.luau line 3"
    );
}

#[test]
fn a_flag_that_cannot_be_written_at_once_is_written_at_the_settle() {
    let h = Rec::with(1);
    h.busy.set(true);
    stops::mark(&h, vec![trip(game_vm(0), "hotkey", None, 0)]);
    assert_eq!(h.calls()[0], "flip 0 waits");
    assert!(h.enabled.borrow()[0], "still on until the settle");
    h.busy.set(false);
    stops::settle(&h);
    let calls = h.calls();
    assert_eq!(&calls[..2], ["flip 0", "disable 0"]);
    assert!(!h.enabled.borrow()[0]);
}

#[test]
fn turned_on_again_before_the_settle_it_is_only_dropped() {
    let h = Rec::with(1);
    stops::mark(&h, vec![trip(game_vm(0), "hotkey", None, 0)]);
    h.calls();
    assert!(h.stops.clear(0));
    assert!(!h.stops.clear(0), "cleared once");
    stops::settle(&h);
    assert!(h.calls().is_empty(), "no dialog, no sentence, no row for a stop that is over");
    // A rolled-back hot-load takes the records from its first index on.
    stops::mark(&h, vec![trip(game_vm(0), "hotkey", None, 0)]);
    h.stops.drop_from(0);
    assert!(!h.stops.is_stopped(0) && !h.stops.has_unsettled());
}

/// A library's stop takes the library and every module that runs its code that is on; one that is
/// off stays as it is, and the members are the library first, then the others by name. Ticking
/// the library turns on every member still off for that stop; ticking another turns on only it.
#[test]
fn a_library_stop_turns_the_whole_group_off_in_one_step() {
    let h = Rec::with(4);
    h.enabled.borrow_mut()[3] = false;
    let off = VmInfo { module: Some(3), id: "com.example.off".into(), name: "Off".into(), code: vec![] };
    let mut t = time_trip(synth_vm(1), "hotkey", ("com.example.kit", "src/main.luau", 9), &[]);
    t.culprit = Some(("com.example.kit".into(), "Overlay kit".into()));
    t.group = vec![member(&synth_vm(1)), member(&sampler_vm(2)), member(&kit_vm(0)), member(&off)];
    stops::mark(&h, vec![t]);
    let calls = h.calls();
    assert_eq!(
        calls[0],
        "log guard [com.example.off] runs the code of com.example.kit, which was stopped; it was off already, and \
         runs none of its code until it is turned on again"
    );
    assert_eq!(&calls[1..5], ["flip 1", "flip 2", "flip 0", "keys"], "{calls:?}");
    for idx in [0, 1, 2] {
        assert!(h.stops.is_stopped(idx), "{idx}");
    }
    assert!(!h.stops.is_stopped(3), "off already: no record");
    let ev = h.stops.record(1).unwrap();
    let ids: Vec<&str> = ev.members.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["com.example.kit", "com.example.sampler", "com.example.synth"]);
    assert_eq!(h.stops.count(), 3);
    assert_eq!(h.stops.turned_on_with(0), [0, 2, 1], "ticking the library turns them all on");
    assert_eq!(h.stops.turned_on_with(2), [2], "ticking another turns on only it");
    assert!(h.stops.clear(1));
    assert_eq!(h.stops.turned_on_with(0), [0, 2], "one ticked on its own already");
    assert_eq!(h.stops.turned_on_with(3), [3], "no stop: only it");
    stops::settle(&h);
    let calls = h.calls();
    assert_eq!(&calls[..2], ["disable 0", "disable 2"], "{calls:?}");
    assert!(calls.iter().any(|c| c.starts_with("dialog com.example.synth/stop/1 | Modules stopped: com.example.kit and 2 that run its code |")), "{calls:?}");
    assert!(!calls.iter().any(|c| c.starts_with("gc ")), "no collection after a time stop");
}

#[test]
fn the_store_keeps_a_stopped_module_on() {
    assert!(stops::stored_enabled(false, true), "stopped: on at the next start");
    assert!(stops::stored_enabled(true, false));
    assert!(!stops::stored_enabled(false, false), "unticked by the user: off");
}

#[test]
fn names_are_counted_and_ticks_wrap() {
    let names: Vec<String> = ["Tab", "Tab", "Space", "Tab", "Ctrl+Alt+Win+9", "Ctrl+Alt+Win+9"].iter().map(|s| s.to_string()).collect();
    assert_eq!(stops::counted(&names), ["Tab \u{d7}3", "Space", "Ctrl+Alt+Win+9 \u{d7}2"]);
    use crate::backend::tick_not_after;
    assert!(tick_not_after(100, 100) && tick_not_after(99, 100) && !tick_not_after(101, 100));
    assert!(tick_not_after(u32::MAX - 5, 10), "across the wrap, before");
    assert!(!tick_not_after(15, u32::MAX - 5), "across the wrap, after");
}

/// On Windows the sentence is a notification balloon, whose text Windows cuts at 255 units. The
/// worst case — a library with a long name, 12 modules with 30-character names that run its code,
/// nested in another module's callback, in a host call, with a long path — still fits, and still
/// names the module, the callback, the host call and the place; and names longer than any form
/// can hold are cut with "…" at 255.
#[test]
fn the_sentence_fits_a_notification() {
    let long = |n: usize| format!("Module with a thirty-char nm {n:02}");
    // The callback of one of the twelve, inside Game helper's hotkey, in a host call.
    let mut vm = synth_vm(1);
    vm.name = long(1);
    let mut t = time_trip(vm, "arbiter onDeactivate", ("com.example.kit", "src/main.luau", 1595), &[]);
    t.kind = EntryKind::Plain;
    t.outer = Some(game_hotkey());
    t.call = Some("host.screen.imageSearchMulti");
    t.culprit = Some(("com.example.kit".into(), "Overlay runtime".into()));
    let mut ev = event(t);
    ev.members = std::iter::once(stops::Member { idx: 0, id: "com.example.kit".into(), name: "Overlay runtime".into() })
        .chain((1..=12).map(|i| stops::Member { idx: i, id: format!("com.example.m{i}"), name: long(i) }))
        .collect();
    let said = stops::sentence(&ev);
    assert!(said.encode_utf16().count() <= stops::SPOKEN_MAX, "{} units: {said}", said.encode_utf16().count());
    assert_eq!(
        said,
        "Stopped Overlay runtime and 12 modules that run its code: its code ran too long in Module with a thirty-char \
         nm 01's arbiter onDeactivate callback, in host.screen.imageSearchMulti, at src/main.luau line 1595. Their \
         keys go to the program in front again."
    );
    // A name no form can hold.
    let mut huge = event(time_trip(game_vm(0), "hotkey", ("com.example.game", "src/main.luau", 40), &[]));
    huge.trip.vm.name = "N".repeat(400);
    let said = stops::sentence(&huge);
    assert_eq!(said.encode_utf16().count(), stops::SPOKEN_MAX);
    assert!(said.ends_with('\u{2026}'));
}

/// The words of a stop, each found word for word in the docs that show them.
#[test]
fn the_messages_are_the_documented_ones() {
    let lifecycle = include_str!("../../../docs/module-runtime-and-lifecycle.md");
    let manager = include_str!("../../../docs/module-manager.md");
    let found = |text: &str| {
        assert!(lifecycle.contains(text) || manager.contains(text), "not in the docs, word for word:\n{text}");
    };

    // One module, in its own hotkey callback.
    let one = event(trip(game_vm(0), "hotkey", None, 256 * MIB - MIB / 10));
    let said = stops::sentence(&one);
    assert_eq!(
        said,
        "Stopped Game helper: its hotkey callback needed more than the 256 megabytes of memory it may use. Its keys \
         go to the program in front again, and it stays off until you turn it on in the module manager or start the \
         application again."
    );
    found(&said);
    let dialog = stops::dialog_text(&one);
    assert_eq!(
        dialog,
        "Stopped Game helper: its hotkey callback needed more than the 256 megabytes of memory it may use.\n\n\
         What used too much memory: Game helper's hotkey callback.\n\
         How much: more than its 256 MiB \u{2014} all of it its own \u{2014} with 255.9 MiB in use when it was stopped.\n\
         Where: not known. Luau does not say where it ran out of memory.\n\
         Turned off: Game helper (com.example.game).\n\n\
         Its keys go to the program in front again. It stays off until you turn it on again in the module manager's \
         Installed list, or start the application again; turned on, it is built afresh, as a reload builds it. To keep \
         it off after a restart too, tick it and untick it. If it happens again, send this text to the module's author."
    );
    found(&dialog);
    assert_eq!(stops::dialog_title(&one), "Module stopped: com.example.game");
    let line = stops::log_line(&one);
    assert_eq!(
        line,
        "[com.example.game] stopped: its hotkey callback needed more than the 256 MiB of memory its VM may use \
         (255.9 MiB in use); where is not known; off until the next start: com.example.game"
    );
    found(&line);
    let note = stops::row_note(&one, 0);
    assert_eq!(note, "stopped: its hotkey callback needed more than 256 MiB of memory");
    found(&note);

    // Inside another module's callback, in a VM with a library's code, through a host call whose
    // traceback mlua kept.
    let mut t = trip(synth_vm(1), "arbiter onDeactivate", Some(game_hotkey()), 555 * MIB + 9 * MIB / 10);
    t.kind = EntryKind::Plain;
    t.traceback = Some(
        "stack traceback:\n\t[C]: in function 'read'\n\t[string \"C:\\AutomationPlatform\\modules\\synth\\src\\main.luau\"]:12: in \
         function 'loadPresets'"
            .into(),
    );
    let nested = event(t);
    let said = stops::sentence(&nested);
    assert_eq!(
        said,
        "Stopped Synth overlay: its arbiter onDeactivate callback, run during Game helper's hotkey callback, needed \
         more than the 556 megabytes of memory it may use. Its keys go to the program in front again; more in the \
         error window."
    );
    found(&said);
    let dialog = stops::dialog_text(&nested);
    found(&dialog);
    assert!(dialog.contains(
        "Where, as far as it is known:\n  stack traceback:\n  [C]: in function 'read'\n  [string \
         \"C:\\AutomationPlatform\\modules\\synth\\src\\main.luau\"]:12: in function 'loadPresets'\n"
    ));
    let note = stops::row_note(&nested, 1);
    assert_eq!(note, "stopped: its arbiter onDeactivate callback needed more than 556 MiB of memory");
    found(&note);

    // Its own onChange, inside its own set.
    let same = Outer { same_vm: true, ..game_hotkey() };
    let mut t = trip(game_vm(0), "settings onChange (volume)", Some(same), 256 * MIB);
    t.kind = EntryKind::Plain;
    assert_eq!(
        stops::sentence(&event(t)),
        "Stopped Game helper: its settings onChange (volume) callback, run during its own hotkey callback, needed more \
         than the 256 megabytes of memory it may use. Its keys go to the program in front again; it stays off until \
         you turn it on in the module manager."
    );

    // --- Time ---
    // One module, its own loop, a mouse button it held and keys pressed while it held the loop.
    let mut ev = event(time_trip(
        game_vm(0),
        "hotkey",
        ("com.example.game", "src/main.luau", 40),
        &["com.example.game/src/main.luau:40: in function 'spin'", "com.example.game/src/main.luau:44: in function <com.example.game/src/main.luau:43>"],
    ));
    ev.released = vec![("Game helper".into(), "left")];
    ev.dropped = Dropped { keys: 3, hotkeys: 0, names: vec!["Tab \u{d7}2".into(), "Space".into()] };
    let said = stops::sentence(&ev);
    assert_eq!(
        said,
        "Stopped Game helper: its hotkey callback ran for 2 seconds of processor time without returning, at \
         src/main.luau line 40. Its keys go to the program in front again; it stays off until you turn it on in the \
         module manager."
    );
    found(&said);
    let dialog = stops::dialog_text(&ev);
    assert_eq!(
        dialog,
        "Stopped Game helper: its hotkey callback ran for 2 seconds of processor time without returning, at \
         src/main.luau line 40.\n\n\
         What ran too long: Game helper's hotkey callback.\n\
         How long: 2.0 s of processor time, 2.1 s in all. A callback may take 2 s of processor time or 10 s in all.\n\
         In a host call: no, it was running Luau code.\n\
         Where, innermost first:\n\
         \x20 com.example.game/src/main.luau:40: in function 'spin'\n\
         \x20 com.example.game/src/main.luau:44: in function <com.example.game/src/main.luau:43>\n\
         Turned off: Game helper (com.example.game).\n\
         Released: the left mouse button it was holding down.\n\
         Dropped: 3 keys pressed while the application waited: Tab \u{d7}2, Space.\n\n\
         Its keys go to the program in front again. It stays off until you turn it on again in the module manager's \
         Installed list, or start the application again; turned on, it is built afresh, as a reload builds it. To keep \
         it off after a restart too, tick it and untick it. If it happens again, send this text to the module's author."
    );
    found(&dialog);
    let line = stops::log_line(&ev);
    assert_eq!(
        line,
        "[com.example.game] stopped: its hotkey callback ran 2003 ms of processor time (2110 ms in all) without \
         returning, past the processor-time limit; in Luau code; at com.example.game/src/main.luau:40; off until \
         the next start: com.example.game; released: left mouse button of Game helper; dropped: 3 key(s), 0 hotkey \
         press(es); frames: com.example.game/src/main.luau:40: in function 'spin' | \
         com.example.game/src/main.luau:44: in function <com.example.game/src/main.luau:43>"
    );
    found(&line);
    let keys = stops::dropped_line(&ev);
    assert_eq!(
        keys,
        "3 key(s) and 0 hotkey press(es) made while Game helper's hotkey callback held the application were dropped, as \
         the module was stopped: Tab \u{d7}2, Space"
    );
    found(&keys);
    let note = stops::row_note(&ev, 0);
    assert_eq!(note, "stopped: its hotkey callback ran too long, at src/main.luau:40");
    found(&note);

    // The wall clock, in a host call.
    let mut t = time_trip(game_vm(0), "hotkey", ("com.example.game", "src/main.luau", 71), &["[C]: in function 'pixel'"]);
    t.cause = Cause::Time { clock: Clock::Wall, cpu: Duration::from_millis(700), wall: Duration::from_millis(10020) };
    t.call = Some("host.screen.pixel");
    let wall = event(t);
    let said = stops::sentence(&wall);
    assert_eq!(
        said,
        "Stopped Game helper: its hotkey callback ran for 10 seconds without returning, in host.screen.pixel, at \
         src/main.luau line 71. Its keys go to the program in front again; it stays off until you turn it on in the \
         module manager."
    );
    found(&said);
    assert!(stops::dialog_text(&wall).contains(
        "How long: 0.7 s of processor time, 10.0 s in all. A callback may take 2 s of processor time or 10 s in all.\n\
         In a host call: host.screen.pixel \u{2014} the time ran out in it, or it had just returned.\n"
    ));
    let note = stops::row_note(&wall, 0);
    assert_eq!(note, "stopped: its hotkey callback ran too long, in host.screen.pixel, at src/main.luau:71");
    found(&note);

    // Inside another module's callback.
    let mut t = time_trip(synth_vm(1), "arbiter onDeactivate", ("com.example.synth", "src/main.luau", 22), &[]);
    t.kind = EntryKind::Plain;
    t.outer = Some(game_hotkey());
    let said = stops::sentence(&event(t));
    assert_eq!(
        said,
        "Stopped Synth overlay: its arbiter onDeactivate callback, run during Game helper's hotkey callback, ran for 2 \
         seconds of processor time without returning, at src/main.luau line 22. Its keys go to the program in front \
         again; more in the error window."
    );
    found(&said);

    // A slow host call in the library's code: only the module whose callback ran it, at the
    // library's line, which is named as the library's.
    let mut t = time_trip(synth_vm(1), "hotkey", ("com.example.kit", "src/main.luau", 1595), &[]);
    t.cause = Cause::Time { clock: Clock::Wall, cpu: Duration::from_millis(1200), wall: Duration::from_millis(10040) };
    t.call = Some("host.screen.imageSearchMulti");
    let slow = event(t);
    let said = stops::sentence(&slow);
    assert_eq!(
        said,
        "Stopped Synth overlay: its hotkey callback ran for 10 seconds without returning, in \
         host.screen.imageSearchMulti, at Overlay kit's src/main.luau line 1595. Its keys go to the program in front \
         again; it stays off until you turn it on in the module manager."
    );
    found(&said);
    let note = stops::row_note(&slow, 1);
    assert_eq!(note, "stopped: its hotkey callback ran too long, in host.screen.imageSearchMulti, at Overlay kit's src/main.luau:1595");
    found(&note);

    // A library's code, in Synth overlay's hotkey callback: the library and the two modules that
    // run its code.
    let library = kit_stop();
    let said = stops::sentence(&library);
    assert_eq!(
        said,
        "Stopped Overlay kit and 2 modules that run its code: its code ran too long in Synth overlay's hotkey callback, \
         at src/main.luau line 9. Their keys go to the program in front again; they stay off until you turn them on in \
         the module manager."
    );
    found(&said);
    assert_eq!(stops::dialog_title(&library), "Modules stopped: com.example.kit and 2 that run its code");
    let dialog = stops::dialog_text(&library);
    assert_eq!(
        dialog,
        "Stopped Overlay kit and the 2 modules that run its code, Sampler overlay and Synth overlay: its code ran for 2 \
         seconds of processor time without returning in Synth overlay's hotkey callback, at src/main.luau line 9.\n\n\
         What ran too long: Overlay kit's code, in Synth overlay's hotkey callback.\n\
         How long: 2.0 s of processor time, 2.1 s in all. A callback may take 2 s of processor time or 10 s in all.\n\
         In a host call: no, it was running Luau code.\n\
         Where, innermost first:\n\
         \x20 com.example.kit/src/main.luau:9: in function 'spin'\n\
         \x20 com.example.synth/src/main.luau:41: in function <com.example.synth/src/main.luau:40>\n\
         Turned off: Overlay kit (com.example.kit), Sampler overlay (com.example.sampler), Synth overlay \
         (com.example.synth).\n\n\
         Their keys go to the program in front again. They stay off until you turn them on again in the module \
         manager's Installed list, or start the application again; ticking Overlay kit turns them all on again, each \
         built afresh, as a reload builds it. To keep one off after a restart too, tick it and untick it. If it happens \
         again, send this text to the module's author."
    );
    found(&dialog);
    let note = stops::row_note(&library, 0);
    assert_eq!(note, "stopped: its code ran too long in Synth overlay's hotkey callback, at src/main.luau:9");
    found(&note);
    let with = stops::row_note(&library, 2);
    assert_eq!(with, "stopped with Overlay kit: its code ran too long in Synth overlay's hotkey callback, at src/main.luau:9");
    found(&with);

    // The library's own VM, looping in its own hotkey callback: its code, wherever it runs.
    let mut t = time_trip(kit_vm(0), "hotkey", ("com.example.kit", "src/main.luau", 30), &[]);
    t.group = vec![member(&kit_vm(0)), member(&synth_vm(1)), member(&sampler_vm(2))];
    let mut own = event(t);
    own.members = library.members.clone();
    let said = stops::sentence(&own);
    assert_eq!(
        said,
        "Stopped Overlay kit and 2 modules that run its code: its code ran too long in its own hotkey callback, at \
         src/main.luau line 30. Their keys go to the program in front again; they stay off until you turn them on in the \
         module manager."
    );
    found(&said);
    assert_eq!(stops::dialog_title(&own), "Modules stopped: com.example.kit and 2 that run its code");
    let note = stops::row_note(&own, 0);
    assert_eq!(note, "stopped: its code ran too long in its own hotkey callback, at src/main.luau:30");
    found(&note);
    let with = stops::row_note(&own, 2);
    assert_eq!(with, "stopped with Overlay kit: its code ran too long in Overlay kit's hotkey callback, at src/main.luau:30");
    found(&with);

    // The library was off already: the modules that run its code, and how to turn them on.
    let mut off = kit_stop();
    off.members.remove(0);
    let said = stops::sentence(&off);
    assert_eq!(
        said,
        "Stopped 2 modules that run Overlay kit's code: Overlay kit's code ran too long in Synth overlay's hotkey \
         callback, at src/main.luau line 9. Their keys go to the program in front again; they stay off until you turn \
         them on in the module manager."
    );
    found(&said);
    let title = stops::dialog_title(&off);
    assert_eq!(title, "Modules stopped: com.example.sampler and com.example.synth, which run Overlay kit's code");
    found(&title);
    assert!(stops::dialog_text(&off).contains("; tick each of them to turn it on again, built afresh, as a reload builds it."));
}

#[test]
fn the_details_text() {
    let now = Instant::now();
    let vm = synth_vm(1);
    let carriers = vec![("Sampler overlay".to_string(), "com.example.sampler".to_string())];
    let deps = vec!["com.example.kit".to_string()];
    let base = Details {
        name: "Synth overlay",
        version: "0.1.0",
        id: "com.example.synth",
        enabled: true,
        stop: None,
        vm: &vm,
        used: 19 * MIB / 10,
        carriers: &[],
        dependencies: &deps,
        now,
    };
    let manager = include_str!("../../../docs/module-manager.md");
    let on = stops::details_text(&base);
    assert_eq!(
        on,
        "Synth overlay 0.1.0 (com.example.synth)\n\
         State: on.\n\
         Memory: its VM may use up to 556 MiB \u{2014} 300 of its own (set in its module.toml) and 256 for Overlay kit \
         (com.example.kit), whose code runs in it. In use now: 1.9 MiB.\n\
         Its code runs only in its own VM.\n\
         Depends on: com.example.kit."
    );
    assert!(manager.contains(&on), "module-manager.md shows this, word for word:\n{on}");
    let off = stops::details_text(&Details { enabled: false, carriers: &carriers, dependencies: &[], ..base });
    assert!(off.contains("State: off (turned off in the Installed list).\n"), "{off}");
    assert!(off.contains("Its code also runs in the VMs of: Sampler overlay (com.example.sampler).\n"), "{off}");
    assert!(off.ends_with("Depends on no other module."), "{off}");
    let ev = StopEvent { at: now - Duration::from_secs(185), ..event(trip(synth_vm(1), "hotkey", None, 556 * MIB)) };
    let stopped = stops::details_text(&Details { enabled: false, stop: Some(&ev), ..base });
    let state = "State: off \u{2014} stopped by the application 3 minutes ago: Stopped Synth overlay: its hotkey callback \
                 needed more than the 556 megabytes of memory it may use. Its keys went to the program in front again. It \
                 stays off until you tick it in the Installed list or start the application again; ticked, it is built \
                 afresh. To keep it off after a restart too, tick it and untick it.\n";
    assert!(stopped.contains(state), "{stopped}");
    assert!(manager.contains(state.trim_end()), "module-manager.md shows the stopped state, word for word:\n{state}");
    // A VM with only its own code, at the default.
    let lone = game_vm(0);
    let text = stops::details_text(&Details { name: "Game helper", id: "com.example.game", vm: &lone, dependencies: &[], ..base });
    assert!(text.contains("Memory: its VM may use up to 256 MiB \u{2014} all of it its own. In use now: 1.9 MiB.\n"), "{text}");
}
