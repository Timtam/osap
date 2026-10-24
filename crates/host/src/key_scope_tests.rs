//! Each module's key scope and menu flag, against real Luau VMs with the host's own `host.keys`
//! bindings (`install_key_captures`) over the stub backend, which keeps what it is handed as the
//! keyboard hook keeps it: which module a key-down goes to, and that a module that tears its
//! overlay down late sets back its own scope and flag and nobody else's.
//!
//! The hook is played by `press`: [`backend::capture_decision`] over the captures and the owners
//! the stub backend was handed last, with a window in front the test names — what the Windows
//! hook and the Mac tap both call. The pump's delivery is [`captures::arrive`], which the host's
//! `on_key` calls, into each module's real mailbox (`mailbox.rs`).
//!
//! **Busy** is a module whose handler waits: an event of its VM, run as a handler through its
//! mailbox, parked at a read (`host.ocr.recognize` without a callback, answered by the read service
//! over the fake recogniser when the test turns the loop) or at the tests' own wait point
//! (`task::test_wait`), which goes on only when the test answers it (`task::test_release`). Until
//! then the other module works.
//!
//! **Passed on** is a key the busy module let go of while it waited, sent to the program in front
//! (`captures::pass_on`): the stub backend reads the keyboard the test sets (`hold`, `hold_reader`,
//! `set_front`, and `type_through` for a key typed past every capture to the program) and records
//! what would be sent (`take_passed`), sending nothing. A key the hook takes carries the stub's
//! count of keys let through at that moment, as the hook's would ([`at`]).
//!
//! **Quick repeats** (taps.rs) are counted at the same delivery: a key timed by when the hook queued
//! it (`Pressed::queued`, which the tests set), a hotkey by the press the pump hands over
//! (`hotkey_arrive_in`); the tick that hands a `"final"` binding its sequence is
//! [`taps::tick`], the pump having taken the presses queued until a moment the test names.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, Value};

use crate::backend::stub::StubBackend;
use crate::backend::{self, Backend, Capture, NotPassed, OwnerKeys, PassWhy, Pressed, Stroke};
use crate::captures::{self, Arrived, Captures, Gone, KeyHost, PassedOn};
use crate::image_search::VmOwner;
use crate::mailbox::{self, Delivered, Event, MailHost, Mailboxes, Opened, Why};
use crate::ocr::lua::{OcrState, ReadHost};
use crate::ocr::service::{Service, ShutdownHandle};
use crate::taps::{self, Press, Taps};
use crate::task::{self, Tasks};
use crate::task_tests::{worker, Fake};

/// What the host keeps for the keys and for a module that waits, without the speech engines.
struct Host {
    ocr: Service<Fake>,
    stop: ShutdownHandle,
    state: OcrState,
    snaps: crate::snapshot::SnapState,
    snap_bytes: Rc<Cell<usize>>,
    /// No image worker: these tests make no image reads.
    images: crate::image_search::ImageState,
    tasks: Tasks,
    mail: Mailboxes,
    gens: RefCell<HashMap<usize, u64>>,
    enabled: RefCell<Vec<bool>>,
    epoch: Cell<u64>,
    /// (module, context, message) of every error reported.
    errors: RefCell<Vec<(usize, String, String)>>,
    captures: Captures,
    /// The count of quick repeats, as the host keeps it (taps.rs).
    taps: crate::taps::Counter,
    /// Hotkey registrations, as the host keeps them (`Event::Hotkey`, opened by the host's own
    /// `open_hotkey_in`).
    hotkeys: RefCell<HashMap<i32, crate::HotkeyReg>>,
    backend: StubBackend,
    _loop: crate::loop_guard::TestLoop,
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.shutdown(Duration::from_secs(2));
    }
}

impl ReadHost for Host {
    type Shot = Fake;
    fn ocr(&self) -> &Service<Fake> {
        &self.ocr
    }
    fn ocr_state(&self) -> &OcrState {
        &self.state
    }
    fn snap_state(&self) -> &crate::snapshot::SnapState {
        &self.snaps
    }
    fn snap_bytes(&self) -> &Rc<Cell<usize>> {
        &self.snap_bytes
    }
    fn images(&self) -> &crate::image_search::ImageState {
        &self.images
    }
    fn tasks(&self) -> &Tasks {
        &self.tasks
    }
    fn vm_gens(&self) -> Ref<'_, HashMap<usize, u64>> {
        self.gens.borrow()
    }
    fn module_enabled(&self, idx: usize) -> bool {
        self.enabled.borrow().get(idx).copied().unwrap_or(false)
    }
    fn module_id(&self, idx: usize) -> String {
        format!("m{idx}")
    }
    fn bump_epoch(&self) {
        self.epoch.set(self.epoch.get() + 1);
    }
    fn report_error(&self, idx: usize, context: &str, message: &str) {
        self.errors.borrow_mut().push((idx, context.to_string(), message.to_string()));
    }
}

/// The events these modules hear: captured keys, hotkeys, and the tests' own.
impl MailHost for Host {
    fn mail(&self) -> &Mailboxes {
        &self.mail
    }
    fn open(&self, idx: usize, _: &Lua, ev: Event) -> Opened {
        match ev {
            Event::Key { owner, token, vk, mods, repeat, pressed, let_go, press } => {
                captures::open_key(self, owner, token, vk, mods, repeat, pressed, let_go, press)
            }
            Event::Hotkey { id, owner, front, seq, binding, spec, press } => {
                crate::open_hotkey_in(self, &self.hotkeys, id, owner, front, seq, binding, &spec, press)
            }
            Event::Call { f, args, what, .. } => mailbox::open_call(idx, f, args, what),
            _ => Opened::Gone,
        }
    }
    fn discard(&self, _: usize, _: Event, _: Why) {}
}

impl KeyHost for Host {
    fn captures(&self) -> &Captures {
        &self.captures
    }
    fn taps(&self) -> &crate::taps::Counter {
        &self.taps
    }
    fn key_backend(&self) -> &dyn Backend {
        &self.backend
    }
    fn key_module_enabled(&self, idx: usize) -> bool {
        self.enabled.borrow().get(idx).copied().unwrap_or(false)
    }
    fn key_module_id(&self, idx: usize) -> String {
        format!("m{idx}")
    }
    fn key_hotkey_holds(&self, vk: u32, mask: u8) -> bool {
        crate::hotkey_holds(&self.hotkeys.borrow(), vk, mask)
    }
}

fn host() -> Rc<Host> {
    let (ocr, stop) = Service::spawn(worker());
    Rc::new(Host {
        ocr,
        stop,
        state: OcrState::default(),
        snaps: crate::snapshot::SnapState::default(),
        snap_bytes: Rc::new(Cell::new(0)),
        images: crate::image_search::ImageState::detached(),
        tasks: Tasks::default(),
        mail: Mailboxes::default(),
        gens: RefCell::new(HashMap::new()),
        enabled: RefCell::new(Vec::new()),
        epoch: Cell::new(0),
        errors: RefCell::new(Vec::new()),
        captures: Captures::default(),
        taps: crate::taps::Counter::default(),
        hotkeys: RefCell::new(HashMap::new()),
        backend: StubBackend::default(),
        _loop: crate::loop_guard::mark_for_a_test(),
    })
}

/// VM generations, process-wide as the host's are; apart from task_tests.rs's.
static NEXT_GEN: AtomicU64 = AtomicU64::new(500_000);

/// A fresh VM for module `idx`, enabled, with the host's own `host.keys` capture bindings and
/// `host.ocr.recognize` — raising where it cannot wait — and the tests' wait point as the global
/// `wait`.
fn vm(h: &Rc<Host>, idx: usize) -> Lua {
    let lua = Lua::new();
    let gen = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
    lua.set_app_data(VmOwner { idx, gen });
    h.gens.borrow_mut().insert(idx, gen);
    {
        let mut en = h.enabled.borrow_mut();
        if en.len() <= idx {
            en.resize(idx + 1, false);
        }
        en[idx] = true;
    }
    let host = lua.create_table().unwrap();
    let keys = lua.create_table().unwrap();
    crate::install_key_captures(&lua, &keys, idx, h.clone()).unwrap();
    host.set("keys", keys).unwrap();
    let ocr = lua.create_table().unwrap();
    let hh = h.clone();
    let submit = lua
        .create_function(move |lua, (what, opts, cb): (Value, Value, Value)| crate::ocr::lua::read(&*hh, lua, idx, what, opts, cb))
        .unwrap();
    let waits = task::waits(&lua, idx, h.clone(), submit).unwrap();
    ocr.set("recognize", waits.get::<Function>("recognize").unwrap()).unwrap();
    host.set("ocr", ocr).unwrap();
    lua.globals().set("wait", task::test_wait(&lua, idx, h.clone()).unwrap()).unwrap();
    lua.globals().set("host", host).unwrap();
    lua
}

/// Runs module code, as `mod.luau`.
fn run(lua: &Lua, code: &str) {
    if let Err(e) = lua.load(code).set_name("=mod.luau").exec() {
        panic!("{e}");
    }
}

/// Runs `code` as an event of module `idx`'s — a handler, through its mailbox.
fn event(h: &Host, lua: &Lua, idx: usize, code: &str) -> Delivered {
    let f: Function = lua.load(code).set_name("=mod.luau").into_function().unwrap();
    mailbox::deliver(h, idx, lua, Event::Call { f, args: mlua::MultiValue::new(), input: false, what: "timer" })
}

/// Evaluates `code` and insists on a real `true`.
fn yes(lua: &Lua, code: &str) -> bool {
    matches!(lua.load(code).set_name("=check").eval::<Value>(), Ok(Value::Boolean(true)))
}

/// What a module's callbacks heard, in order, joined.
fn heard(lua: &Lua) -> String {
    lua.load("return table.concat(heard, ' | ')").eval::<String>().unwrap()
}

/// What the keyboard hook decides for a key-down of `spec` with window `front` in front, by what
/// the stub backend was handed last. No native menu.
fn press(h: &Host, spec: &str, front: isize) -> Option<Capture> {
    let (vk, mask) = backend::parse_key_spec(spec).unwrap();
    let (set, owners) = h.backend.keys();
    backend::capture_decision(&set, &owners, vk, mask, front, false, || false)
}

/// A key-down the hook takes in window `front` now: with the counts of keys let through so far, as
/// the hook would take it, and no physical key.
fn at(h: &Host, front: isize) -> Pressed {
    Pressed { front, seq: h.backend.let_through(), taps_seq: h.backend.taps_through(), phys: None, queued: None }
}

/// The pump's delivery of a key-down of `spec` the hook took for `owner` in window `front`.
fn deliver(h: &Host, spec: &str, owner: u32, front: isize) -> Arrived {
    let (vk, mask) = backend::parse_key_spec(spec).unwrap();
    captures::arrive(h, vk, mask, owner as usize, false, at(h, front))
}

/// It ran, and its handler ended.
const RAN: Arrived = Arrived::Delivered(Delivered::Ran);

fn owner_entry(h: &Host, owner: u32) -> Option<OwnerKeys> {
    h.backend.keys().1.into_iter().find(|e| e.owner == owner)
}

const W1: isize = 0x1001;
const W2: isize = 0x2002;

/// Module A's overlay is active in W1 and captures Space and Tab; its handler then waits, after
/// which it tears the overlay down. Meanwhile module B's overlay comes up in W2 and captures Tab.
/// With W2 in front, Space reaches the application (only A captures it, scoped to W1), and Tab goes
/// to B — A's capture of it is earlier but scoped to W1 — and runs at once: B is free, whatever A
/// does. B's menu flag counts for W2 only. When A's wait is answered, A tears down late — releases
/// its keys, `scope(false)`, `menuOpen(false)` — and B's scope and flag stay as B set them. At HEAD
/// the scope and the flag were one switch: A's late `scope(false)` unpinned B's captures, and its
/// `menuOpen(false)` closed B's menu.
#[test]
fn a_busy_module_s_late_teardown_leaves_the_other_module_s_scope_and_flag() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(
        &a,
        r#"
        heard = {}
        tokens = {
          host.keys.capture("Space", function() heard[#heard + 1] = "A space" end),
          host.keys.capture("Tab", function() heard[#heard + 1] = "A tab" end),
        }
        host.keys.scope(true)
        "#,
    );
    // Busy: a handler of A waits, and tears the overlay down after it.
    let parked = event(
        &h,
        &a,
        1,
        r#"
        seen = wait("read")
        for _, t in ipairs(tokens) do host.keys.release(t) end
        host.keys.scope(false)
        host.keys.menuOpen(false)
        tornDown = true
        "#,
    );
    assert_eq!(parked, Delivered::Parked, "A waits");
    assert_eq!(owner_entry(&h, 1), Some(OwnerKeys { owner: 1, scope: W1, menu: false }));

    // B's overlay comes up in W2, which is in front now.
    h.backend.set_front(W2);
    run(
        &b,
        r#"
        heard = {}
        host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end)
        host.keys.scope(true)
        "#,
    );
    assert_eq!(owner_entry(&h, 2), Some(OwnerKeys { owner: 2, scope: W2, menu: false }));
    assert_eq!(owner_entry(&h, 1), Some(OwnerKeys { owner: 1, scope: W1, menu: false }), "A's own, untouched");

    // Space: only A captures it, scoped to W1 — it reaches the application.
    assert_eq!(press(&h, "Space", W2), Some(Capture::Pass { why: PassWhy::OutOfScope, owner: None }));
    // Tab: A's capture is the earlier one, but scoped to W1; B's is in scope.
    assert_eq!(press(&h, "Tab", W2), Some(Capture::Take { owner: 2 }));
    assert_eq!(deliver(&h, "Tab", 2, W2), RAN, "B is free, whatever A does");
    assert_eq!(heard(&b), "B tab");
    assert_eq!(heard(&a), "", "A heard nothing");
    // With W1 in front A's keys would still be A's.
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 1 }));

    // B says a menu is open: the keys captured in W2 go to the menu; W1's are not B's to give.
    run(&b, "host.keys.menuOpen(true)");
    assert_eq!(press(&h, "Tab", W2), Some(Capture::Pass { why: PassWhy::MenuFlag, owner: Some(2) }));
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 1 }));

    // A's wait is answered; its handler goes on and tears A's overlay down, late.
    assert!(task::test_release(&*h, "read", Value::String(a.create_string("the read").unwrap())));
    assert!(yes(&a, "return tornDown == true and seen == 'the read'"), "A's handler went on after its wait");
    assert_eq!(h.tasks.len(), 0);
    assert_eq!(owner_entry(&h, 1), Some(OwnerKeys { owner: 1, scope: 0, menu: false }), "A's own, set back");
    assert_eq!(
        owner_entry(&h, 2),
        Some(OwnerKeys { owner: 2, scope: W2, menu: true }),
        "B's scope and flag stay where B put them"
    );
    assert_eq!(press(&h, "Tab", W2), Some(Capture::Pass { why: PassWhy::MenuFlag, owner: Some(2) }), "B's menu still has the keys");
    assert_eq!(press(&h, "Space", W2), None, "A released Space");
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Pass { why: PassWhy::OutOfScope, owner: None }), "only B's Tab is left");

    // B's menu closes: Tab is B's again.
    run(&b, "host.keys.menuOpen(false)");
    assert_eq!(press(&h, "Tab", W2), Some(Capture::Take { owner: 2 }));
    assert_eq!(deliver(&h, "Tab", 2, W2), RAN);
    assert_eq!(heard(&b), "B tab | B tab");
    assert_eq!(heard(&a), "");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The limit of a scope: it is a whole top-level window, so two plug-ins of two modules inside
/// one DAW window share it, and there the earlier registration takes the key until its module
/// lets it go — busy or not.
#[test]
fn two_modules_in_one_window_the_earlier_registration_takes_the_key() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; tab = host.keys.capture("Tab", function() heard[#heard + 1] = "A tab" end); host.keys.scope(true)"#);
    run(&b, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end); host.keys.scope(true)"#);
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 1 }));
    assert_eq!(deliver(&h, "Tab", 1, W1), RAN);
    assert_eq!((heard(&a).as_str(), heard(&b).as_str()), ("A tab", ""));
    run(&a, "host.keys.release(tab)");
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 2 }), "once A lets it go, B's");
    // A module that captures again goes to the back.
    run(&a, r#"host.keys.capture("Tab", function() heard[#heard + 1] = "A tab" end)"#);
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 2 }));
}

/// A key goes to the module the hook took it for, and to that module's own capture — never to
/// another module's, even one that captures the key in scope. It is dropped, and the log says so,
/// when the module's capture went before the delivery or its scope moved to another window
/// meanwhile; silently when the module was disabled.
#[test]
fn a_key_goes_to_its_own_module_or_is_dropped() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; tab = host.keys.capture("Tab", function(m) heard[#heard + 1] = "A tab shift=" .. tostring(m.shift) end); host.keys.scope(true)"#);
    run(&b, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end)"#);
    run(&a, r#"host.keys.capture("Shift+Tab", function(m) heard[#heard + 1] = "A shift-tab shift=" .. tostring(m.shift) end)"#);
    assert_eq!(deliver(&h, "Shift+Tab", 1, W1), RAN);
    assert_eq!(heard(&a), "A shift-tab shift=true", "the mods table, as ever");
    // A's capture of Tab is released between the press and its delivery: B also captures Tab,
    // everywhere, and still does not get A's key.
    run(&a, "host.keys.release(tab)");
    assert_eq!(deliver(&h, "Tab", 1, W1), Arrived::Dropped(Gone::Released));
    assert_eq!(heard(&b), "");
    // A captures Tab again, and its scope moves to W2 before a key pressed in W1 is delivered.
    run(&a, r#"host.keys.capture("Tab", function() heard[#heard + 1] = "A tab" end)"#);
    h.backend.set_front(W2);
    run(&a, "host.keys.scope(true)");
    assert_eq!(deliver(&h, "Tab", 1, W1), Arrived::Dropped(Gone::ScopeMoved));
    assert_eq!(deliver(&h, "Tab", 1, W2), RAN, "pressed in its window, it runs");
    // Global again: wherever it was pressed.
    run(&a, "host.keys.scope(false)");
    assert_eq!(deliver(&h, "Tab", 1, W1), RAN);
    // Disabled: not run, not said.
    h.enabled.borrow_mut()[1] = false;
    assert_eq!(deliver(&h, "Tab", 1, W1), Arrived::Dropped(Gone::Disabled));
    assert_eq!(heard(&a), "A shift-tab shift=true | A tab | A tab");
    assert_eq!(heard(&b), "");
    // A callback that raises is its handler's error, reported as a key's.
    run(&b, r#"host.keys.capture("Space", function() error("B fell over") end)"#);
    assert_eq!(deliver(&h, "Space", 2, W1), RAN);
    let errors = h.errors.borrow();
    assert!(errors.len() == 1 && errors[0].0 == 2 && errors[0].1 == "key" && errors[0].2.contains("B fell over"), "{errors:?}");
}

/// A disabled module's captures, scope and flag are not handed to the hook; forgetting its entry
/// — what a disable, a reload and a removal do after the arbiter's re-election — drops what its
/// `onDeactivate` wrote meanwhile, so it is everywhere with no menu when it comes back.
#[test]
fn a_disabled_module_takes_its_scope_and_flag_with_it() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"host.keys.capture("Tab", function() end); host.keys.scope(true); host.keys.menuOpen(true)"#);
    run(&b, r#"host.keys.capture("Tab", function() end)"#);
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Pass { why: PassWhy::MenuFlag, owner: Some(1) }));
    // The disable: off, the set handed over again, the re-election's onDeactivate, forgotten.
    h.enabled.borrow_mut()[1] = false;
    h.refresh_captured();
    run(&a, "host.keys.scope(false); host.keys.menuOpen(false)");
    assert!(h.captures.forget_owner(1));
    h.refresh_key_owners();
    let (set, owners) = h.backend.keys();
    assert!(set.iter().all(|c| c.owner == 2), "{set:?}");
    assert!(owners.iter().all(|e| e.owner != 1), "{owners:?}");
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 2 }), "A's flag went with it");
    // Enabled again: its capture is back, everywhere, with no menu, until it says otherwise.
    h.enabled.borrow_mut()[1] = true;
    h.refresh_captured();
    assert_eq!(press(&h, "Tab", W2), Some(Capture::Take { owner: 1 }));
    assert_eq!(owner_entry(&h, 1), None);
}

// ── Keys that wait in a busy module's mailbox ────────────────────────────────────────────────

/// While A waits, its keys wait in its mailbox, in the order they came — each to the capture it was
/// pressed for. A held key's repeats fold while one waits; two deliberate presses never do, and a
/// second hold of the key, after a press of it, keeps a repeat of its own rather than fold into the
/// first hold's. When the wait is answered, the handler ends and the next tick's queued phase runs
/// them, one after another, each as its own handler.
#[test]
fn a_busy_module_s_keys_wait_in_order_and_a_held_key_s_repeats_fold() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "tab" end); host.keys.capture("Down", function() heard[#heard + 1] = "down" end)"#);
    assert_eq!(event(&h, &a, 1, r#"heard[#heard + 1] = "waits"; wait("w"); heard[#heard + 1] = "goes on""#), Delivered::Parked);
    let key = |spec: &str, repeat: bool| {
        let (vk, mask) = backend::parse_key_spec(spec).unwrap();
        captures::arrive(&*h, vk, mask, 1, repeat, at(&h, W1))
    };
    let queued = Arrived::Delivered(Delivered::Queued);
    let folded = Arrived::Delivered(Delivered::Dropped);
    assert_eq!(key("Tab", false), queued);
    assert_eq!(key("Down", false), queued);
    assert_eq!(key("Down", true), queued, "the first repeat waits");
    assert_eq!(key("Down", true), folded, "the next repeats fold into it");
    assert_eq!(key("Down", true), folded);
    assert_eq!(key("Tab", false), queued, "a deliberate press never folds");
    assert_eq!(key("Tab", false), queued);
    // Down held again: its press, then its repeats — the first of which keeps a step of its own.
    assert_eq!(key("Down", false), queued);
    assert_eq!(key("Down", true), queued, "the second hold's first repeat waits too");
    assert_eq!(key("Down", true), folded, "and its next ones fold into it");
    assert_eq!(h.mail.len(1), 7);
    assert_eq!(mailbox::run_queued(&*h), mailbox::Phase::default(), "nothing runs while the handler waits");
    assert!(task::test_release(&*h, "w", Value::Nil));
    assert_eq!(heard(&a), "waits | goes on");
    let phase = mailbox::run_queued(&*h);
    assert_eq!((phase.events, phase.parked, phase.budget), (7, 0, false));
    assert_eq!(heard(&a), "waits | goes on | tab | down | down | tab | tab | down | down");
    assert!(h.mail.is_empty());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The overlay goes and comes back while its module waits — Alt+Tab away and back — and its keys
/// are captured again with new tokens. A key pressed before that goes to the module's new capture
/// of the same key, said in the log, when the module's scope is the window it was pressed in or
/// everywhere; not to another module's capture of the key while its own module captures it, and
/// not at all when the scope is another window now. One whose capture is gone for good goes to the
/// next module that captures it where it was pressed — B, everywhere — as B's key, whichever window
/// is in front by then, and is not passed on.
#[test]
fn a_key_that_waited_goes_to_its_module_s_new_capture_before_another_module_s() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; tab = host.keys.capture("Tab", function() heard[#heard + 1] = "old tab" end); host.keys.scope(true)"#);
    run(&b, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end)"#);
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    let tab = || {
        let (vk, mask) = backend::parse_key_spec("Tab").unwrap();
        captures::arrive(&*h, vk, mask, 1, false, at(&h, W1))
    };
    assert_eq!(tab(), Arrived::Delivered(Delivered::Queued));
    // A handler of A can run only through the arbiter meanwhile (way 1): its overlay's onDeactivate
    // and onActivate, which capture Tab again.
    run(&a, r#"host.keys.release(tab); host.keys.capture("Tab", function() heard[#heard + 1] = "new tab" end)"#);
    assert_eq!(tab(), Arrived::Delivered(Delivered::Queued));
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new tab | new tab", "both went to the new capture");
    assert_eq!(heard(&b), "", "never to another module's");

    // Pressed in W1, and A's scope is W2 by the time it runs: dropped.
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    assert_eq!(tab(), Arrived::Delivered(Delivered::Queued));
    h.backend.set_front(W2);
    run(&a, "host.keys.scope(true)");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new tab | new tab");
    assert_eq!(heard(&b), "", "a scope that moved drops the key: it is not let go of");
    // Released for good while it waited, and W2 in front by then: B captures Tab everywhere, so it
    // would have been B's had A never captured it — B's now, as a key of B's, whichever window is in
    // front. Nothing is passed on to the program.
    run(&a, "host.keys.scope(false)");
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    assert_eq!(tab(), Arrived::Delivered(Delivered::Queued));
    run(&a, "host.keys.releaseAll()");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new tab | new tab");
    assert_eq!(heard(&b), "B tab");
    assert_eq!(h.backend.take_passed(), vec![], "nothing passed on");
    assert_eq!(
        captures::moved_line("m1", "Tab", true),
        "[m1] Tab, pressed while the module was busy, went to the module's current registration of it"
    );
    // A hotkey its free module did not run as it was pressed — its registration changed hands
    // between the press and the turn that delivered it — is not said to have waited.
    assert_eq!(
        captures::moved_line("m1", "Alt+V", false),
        "[m1] Alt+V went to the module's current registration of it, made again between the press and its delivery"
    );
    assert_eq!(
        captures::hotkey_dropped_line("m1", "Alt+V", Gone::TakenOver),
        "[m1] Alt+V was dropped: the hotkey went to another module between the press and its delivery. Said at most \
         once every 10 s per module"
    );
    assert!(captures::hotkey_dropped_line("m1", "Alt+V", Gone::Released).contains("its registration was released"));
    assert_eq!(
        captures::busy_dropped_line("m1", "Tab", Gone::ScopeMoved),
        "[m1] Tab, pressed while the module was busy, was dropped: the module's scope moved to another window meanwhile"
    );
    assert!(captures::busy_dropped_line("m1", "Alt+V", Gone::TakenOver).contains("the hotkey went to another module meanwhile"));
}

/// A hotkey press that waited in a busy module's mailbox runs through the host's own arm
/// (`open_hotkey_in`): to the module's new registration of the combination when the one it was
/// pressed for was released and made again meanwhile — the overlay went and came back — while the
/// module's scope is everywhere or the window it was pressed in; never to another module's, which
/// holds the combination by the time it runs.
#[test]
fn a_busy_module_s_hotkey_goes_to_its_new_registration_and_never_to_another_module_s() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    run(&a, "heard = {}");
    run(&b, "heard = {}");
    let binding = backend::parse_key_spec("Alt+V").ok();
    let register = |id: i32, lua: &Lua, owner: usize, word: &str| {
        let f: Function = lua.load(format!("return function() heard[#heard + 1] = '{word}' end")).eval().unwrap();
        let cb = lua.create_registry_value(f).unwrap();
        let reg = crate::HotkeyReg { module_idx: owner, lua: lua.clone(), cb, spec: "Alt+V".into(), binding, live: true, foreground: false, taps: Default::default() };
        h.hotkeys.borrow_mut().insert(id, reg);
    };
    let press = |id: i32| Event::Hotkey { id, owner: 1, front: Some(W1), seq: 0, binding, spec: "Alt+V".into(), press: Press::at_once(1) };
    register(5, &a, 1, "old");
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, press(5)), Delivered::Queued);
    h.hotkeys.borrow_mut().remove(&5);
    register(6, &a, 1, "new");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new", "the module's new registration of the combination");
    // B holds the combination by the time A's press runs: dropped, never B's.
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, press(6)), Delivered::Queued);
    h.hotkeys.borrow_mut().remove(&6);
    register(7, &b, 2, "B");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new");
    assert_eq!(heard(&b), "", "never another module's");
    assert!(h.backend.take_passed().is_empty(), "a hotkey another module holds now is not passed on");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

// ── What a busy module let go of goes to the program in front ────────────────────────────────

const KEY_DOWN: Stroke = Stroke::Key { down: true };
const KEY_UP: Stroke = Stroke::Key { down: false };

/// A key-down of `spec` the hook took for module 1 in window `front`, as the pump delivers it, a
/// repeat or not.
fn arrive_for_1(h: &Host, spec: &str, repeat: bool, front: isize) -> Arrived {
    let (vk, mask) = backend::parse_key_spec(spec).unwrap();
    captures::arrive(h, vk, mask, 1, repeat, at(h, front))
}

/// The maintainer's decision of 2026-10-05, in the scripted host: keys pressed while A waits, whose
/// captures A releases meanwhile without making them again, and which no other module captures in
/// the window they were pressed in, go to the program in front when A's wait ends — passed on as
/// they were pressed, each in its turn. Tab, the same key alone; Ctrl+V and Shift+Tab, their
/// modifier pressed around them; Down held, its press and its repeats folded into one, twice. None
/// of them runs a callback of A's, and B, whose capture of Tab is pinned to another window, does
/// not get A's Tab. The capture A keeps runs as before.
#[test]
fn keys_a_busy_module_released_meanwhile_go_to_the_program_in_front() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    run(
        &a,
        r#"heard = {}
        local function note(word) return function() heard[#heard + 1] = word end end
        tokens = {
          host.keys.capture("Tab", note("tab")),
          host.keys.capture("Ctrl+V", note("ctrl v")),
          host.keys.capture("Shift+Tab", note("shift tab")),
          host.keys.capture("Down", note("down")),
        }
        host.keys.capture("Space", note("space"))
        host.keys.scope(true)"#,
    );
    h.backend.set_front(W2);
    run(&b, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end); host.keys.scope(true)"#);
    h.backend.set_front(W1);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); for _, t in ipairs(tokens) do host.keys.release(t) end"#), Delivered::Parked);
    let queued = Arrived::Delivered(Delivered::Queued);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert_eq!(arrive_for_1(&h, "Ctrl+V", false, W1), queued);
    assert_eq!(arrive_for_1(&h, "Shift+Tab", false, W1), queued);
    assert_eq!(arrive_for_1(&h, "Down", false, W1), queued);
    assert_eq!(arrive_for_1(&h, "Down", true, W1), queued, "the first repeat waits");
    assert_eq!(arrive_for_1(&h, "Down", true, W1), Arrived::Delivered(Delivered::Dropped), "the next ones fold into it");
    assert_eq!(arrive_for_1(&h, "Space", false, W1), queued);
    // The wait ends: the handler releases A's captures, all but Space's.
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    let ctrl = |down| Stroke::Modifier { role: backend::MASK_CTRL, down };
    let shift = |down| Stroke::Modifier { role: backend::MASK_SHIFT, down };
    assert_eq!(
        h.backend.take_passed(),
        vec![
            (0x09, 0, vec![KEY_DOWN, KEY_UP]),
            (0x56, backend::MASK_CTRL, vec![ctrl(true), KEY_DOWN, KEY_UP, ctrl(false)]),
            (0x09, backend::MASK_SHIFT, vec![shift(true), KEY_DOWN, KEY_UP, shift(false)]),
            (0x28, 0, vec![KEY_DOWN, KEY_UP]),
            (0x28, 0, vec![KEY_DOWN, KEY_UP]),
        ],
        "each as it was pressed, in the order they came; the hold's folded repeats once"
    );
    assert_eq!(heard(&a), "space", "only the capture A kept ran");
    assert_eq!(heard(&b), "", "not a capture pinned to another window");
    assert!(h.mail.is_empty());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// What is not passed on, in the scripted host: a key pressed in another window than the one in
/// front by the time it runs, a key still held down, a key pressed without a modifier that is held
/// now — dropped, as before; and a key the backend could not send, which it was asked to. A key
/// whose capture was made again goes to the new one, and one the module's scope moved away from
/// is dropped, neither of them passed on. A key whose capture went before it even arrived, its
/// module free, is dropped as before.
#[test]
fn keys_a_busy_module_released_are_not_passed_on_elsewhere_or_while_held() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    let capture_tab = r#"heard = heard or {}; tab = host.keys.capture("Tab", function() heard[#heard + 1] = "tab" end)"#;
    // Waits, has Tab pressed in `pressed_in`, releases Tab, and runs it under `setup`.
    let round = |pressed_in: isize, setup: &dyn Fn()| {
        run(&a, capture_tab);
        assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab)"#), Delivered::Parked);
        assert_eq!(arrive_for_1(&h, "Tab", false, pressed_in), Arrived::Delivered(Delivered::Queued));
        assert!(task::test_release(&*h, "w", Value::Nil));
        setup();
        mailbox::run_queued(&*h);
        h.backend.hold(&[], 0);
        h.backend.set_front(W1);
        h.backend.take_passed()
    };
    assert_eq!(round(W1, &|| h.backend.set_front(W2)), vec![], "another window in front");
    assert_eq!(round(0, &|| h.backend.set_front(0)), vec![], "no window known at the press");
    assert_eq!(round(W1, &|| h.backend.hold(&[0x09], 0)), vec![], "Tab still held");
    assert_eq!(round(W1, &|| h.backend.hold(&[], backend::MASK_CTRL)), vec![], "Ctrl held, which Tab was pressed without");
    assert_eq!(round(W1, &|| ()), vec![(0x09, 0, vec![KEY_DOWN, KEY_UP])], "and with none of that, passed on");
    // Asked of the backend, and refused by it: tried once, and dropped.
    assert_eq!(round(W1, &|| h.backend.set_sends(false)), vec![(0x09, 0, vec![KEY_DOWN, KEY_UP])]);
    h.backend.set_sends(true);
    assert_eq!(heard(&a), "", "never A's callback");

    // The decision itself, as `pass_on` reads the backend: each reason, then a key sent.
    run(&a, capture_tab);
    let tab = |pressed_in| captures::pass_on(&*h, 0x09, 0, at(&h, pressed_in));
    h.backend.set_front(W2);
    assert_eq!(tab(W1), PassedOn::NotSent(NotPassed::OtherWindow));
    h.backend.set_front(W1);
    assert_eq!(tab(0), PassedOn::NotSent(NotPassed::NoWindow));
    h.backend.hold(&[0x09], 0);
    assert_eq!(tab(W1), PassedOn::NotSent(NotPassed::KeyHeld));
    h.backend.hold(&[], backend::MASK_ALT | backend::MASK_WIN);
    assert_eq!(tab(W1), PassedOn::NotSent(NotPassed::OtherModifiers(backend::MASK_ALT | backend::MASK_WIN)));
    h.backend.hold(&[], 0);
    h.backend.set_sends(false);
    assert_eq!(tab(W1), PassedOn::Failed("input is not implemented on this platform yet".to_string()));
    h.backend.set_sends(true);
    assert_eq!(tab(W1), PassedOn::Sent);
    let (alt, _) = backend::parse_key_spec("Alt tap").unwrap();
    assert_eq!(captures::pass_on(&*h, alt, backend::MASK_TAP, at(&h, W1)), PassedOn::NotSent(NotPassed::Tap));
    // Shift+Tab with Shift still held: Tab alone, Shift left to the hand that holds it.
    h.backend.hold(&[], backend::MASK_SHIFT);
    assert_eq!(captures::pass_on(&*h, 0x09, backend::MASK_SHIFT, at(&h, W1)), PassedOn::Sent);
    h.backend.hold(&[], 0);
    let passed = h.backend.take_passed();
    assert_eq!(passed.last(), Some(&(0x09, backend::MASK_SHIFT, vec![KEY_DOWN, KEY_UP])));
    assert_eq!(passed.len(), 3, "the refusal's attempt, the one sent, and Shift+Tab: {passed:?}");

    // Made again meanwhile: the new capture, nothing passed on.
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab); host.keys.capture("Tab", function() heard[#heard + 1] = "new tab" end)"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), Arrived::Delivered(Delivered::Queued));
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new tab");
    // Its scope moved to W2 meanwhile: dropped, nothing passed on.
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), Arrived::Delivered(Delivered::Queued));
    h.backend.set_front(W2);
    run(&a, "host.keys.scope(true)");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new tab");
    // Free, and its capture gone before the key arrived: dropped as before, nothing passed on.
    run(&a, "host.keys.scope(false); host.keys.releaseAll()");
    assert_eq!(arrive_for_1(&h, "Tab", false, W2), Arrived::Dropped(Gone::Released));
    assert!(h.backend.take_passed().is_empty());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A hotkey press that waited while its module was busy, for a registration released since and
/// not made again: passed on to the program in front as it was pressed — Alt+V, Alt pressed around
/// V — while the window in front when it arrived still is; dropped when another window is in
/// front, and when only the system could read its combination.
#[test]
fn a_hotkey_its_busy_module_released_goes_to_the_program_in_front() {
    let h = host();
    let a = vm(&h, 1);
    run(&a, "heard = {}");
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    let alt_v = backend::parse_key_spec("Alt+V").ok();
    let register = |id: i32, binding: Option<(u32, u8)>| {
        let f: Function = a.load("return function() heard[#heard + 1] = 'hotkey' end").eval().unwrap();
        let cb = a.create_registry_value(f).unwrap();
        let reg = crate::HotkeyReg { module_idx: 1, lua: a.clone(), cb, spec: "Alt+V".into(), binding, live: true, foreground: false, taps: Default::default() };
        h.hotkeys.borrow_mut().insert(id, reg);
    };
    let press = |id: i32, binding| Event::Hotkey { id, owner: 1, front: Some(W1), seq: h.backend.let_through(), binding, spec: "Alt+V".into(), press: Press::at_once(1) };
    // Waits, has hotkey `id` pressed, unregisters it, and runs it with `front` in front.
    let round = |id: i32, binding: Option<(u32, u8)>, front: isize| {
        register(id, binding);
        assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
        assert_eq!(mailbox::deliver(&*h, 1, &a, press(id, binding)), Delivered::Queued);
        h.hotkeys.borrow_mut().remove(&id);
        assert!(task::test_release(&*h, "w", Value::Nil));
        h.backend.set_front(front);
        mailbox::run_queued(&*h);
        h.backend.set_front(W1);
        h.backend.take_passed()
    };
    let alt = |down| Stroke::Modifier { role: backend::MASK_ALT, down };
    assert_eq!(round(5, alt_v, W1), vec![(0x56, backend::MASK_ALT, vec![alt(true), KEY_DOWN, KEY_UP, alt(false)])]);
    assert_eq!(h.backend.take_passed_phys(), vec![None], "the system matched it: no physical key");
    assert_eq!(round(6, alt_v, W2), vec![], "another window in front");
    assert_eq!(round(7, None, W1), vec![], "a combination only the system read: no key to send");
    assert_eq!(heard(&a), "", "the module's callback never ran");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The lines: a key gone on to another module, a key passed on, and each reason one is not.
#[test]
fn the_released_key_s_lines_say_where_it_went() {
    use captures::released_line as line;
    assert_eq!(
        line("m1", "Tab", &PassedOn::Offered("m2".into())),
        "[m1] Tab, pressed while the module was busy, went to m2, the next module that captures it in the window it \
         was pressed in: its registration was released meanwhile"
    );
    assert_eq!(
        line("m1", "Tab", &PassedOn::Sent),
        "[m1] Tab, pressed while the module was busy, was passed on to the program in front: its registration was \
         released meanwhile"
    );
    let dropped = "[m1] Tab, pressed while the module was busy, was dropped: its registration was released meanwhile, and ";
    for (outcome, why) in [
        (PassedOn::NotSent(NotPassed::OtherWindow), "the window it was pressed in is no longer in front"),
        (PassedOn::NotSent(NotPassed::NoWindow), "no window was known to be in front when it was pressed"),
        (PassedOn::NotSent(NotPassed::KeyHeld), "the key is still held down"),
        (PassedOn::Failed("SendInput took 0 of its 2 key events".into()), "passing it on to the program in front failed: SendInput took 0 of its 2 key events"),
        (PassedOn::NoKey, "its combination is one only the system could read, so it cannot be sent again"),
        (PassedOn::NotSent(NotPassed::FrontUnknown), "nobody could say which window is in front now"),
        (
            PassedOn::NotSent(NotPassed::ReaderModifier),
            "the screen reader's key is held down now, which the press was made without",
        ),
        (PassedOn::NotSent(NotPassed::Overtaken), "a key typed after it has reached the program first"),
        (
            PassedOn::NotSent(NotPassed::HotkeyHolds),
            "a hotkey holds its combination now, which would take it instead of the program",
        ),
    ] {
        assert_eq!(line("m1", "Tab", &outcome), format!("{dropped}{why}"));
    }
    let held = line("m1", "Tab", &PassedOn::NotSent(NotPassed::OtherModifiers(backend::MASK_CTRL | backend::MASK_SHIFT)));
    let words = if cfg!(target_os = "macos") { "Shift+Cmd" } else { "Ctrl+Shift" };
    assert_eq!(held, format!("{dropped}{words} is held down now, which the press was made without"));
    assert_eq!(
        line("m1", "Alt tap", &PassedOn::NotSent(NotPassed::Tap)),
        "[m1] Alt tap, pressed while the module was busy, was not run: its registration was released meanwhile, and \
         the program in front had it already, as a modifier tap is never kept from it"
    );
}

/// Never out of order: a key passed on arrives where it would have, had nothing captured it — so
/// not once a key typed after it has reached the program, which it would then follow. A waits with
/// Delete and Tab captured and released: Delete is pressed, Down is typed past every capture (the
/// program's list moves on), then Tab. Delete is dropped — sent now, it would delete the line Down
/// moved to — and Tab, pressed after Down, is passed on. Down held past the release: its press and
/// its folded repeat are dropped, its later repeats having reached the program first. A hotkey
/// press the same way.
#[test]
fn a_key_typed_after_it_reached_the_program_first_and_it_is_not_passed_on() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    run(
        &a,
        r#"heard = {}
        tokens = {
          host.keys.capture("Delete", function() heard[#heard + 1] = "delete" end),
          host.keys.capture("Tab", function() heard[#heard + 1] = "tab" end),
          host.keys.capture("Down", function() heard[#heard + 1] = "down" end),
        }"#,
    );
    let queued = Arrived::Delivered(Delivered::Queued);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); for _, t in ipairs(tokens) do host.keys.release(t) end"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Delete", false, W1), queued);
    h.backend.type_through();
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(h.backend.take_passed(), vec![(0x09, 0, vec![KEY_DOWN, KEY_UP])], "Tab only: Down came before it");
    // The same, decided directly: a press whose count is behind the backend's.
    let before = Pressed { front: W1, seq: h.backend.let_through().wrapping_sub(1), ..at(&h, W1) };
    assert_eq!(captures::pass_on(&*h, 0x2E, 0, before), PassedOn::NotSent(NotPassed::Overtaken));

    // Down held: the press and its first repeat wait; the capture goes, and the repeats after it
    // reach the program on their own before the key comes up.
    run(&a, r#"down = host.keys.capture("Down", function() heard[#heard + 1] = "down" end)"#);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(down)"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Down", false, W1), queued);
    assert_eq!(arrive_for_1(&h, "Down", true, W1), queued);
    h.backend.type_through();
    h.backend.type_through();
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert!(h.backend.take_passed().is_empty(), "neither the press nor its repeat");

    // A hotkey press, Alt+V, that waited for a registration A unregistered: a key typed after it
    // reached the program, and it is dropped; with nothing typed, it goes.
    let alt_v = backend::parse_key_spec("Alt+V").ok();
    let round = |id: i32, typed: bool| {
        let f: Function = a.load("return function() heard[#heard + 1] = 'hotkey' end").eval().unwrap();
        let cb = a.create_registry_value(f).unwrap();
        let reg = crate::HotkeyReg { module_idx: 1, lua: a.clone(), cb, spec: "Alt+V".into(), binding: alt_v, live: true, foreground: false, taps: Default::default() };
        h.hotkeys.borrow_mut().insert(id, reg);
        assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
        let press = Event::Hotkey { id, owner: 1, front: Some(W1), seq: h.backend.let_through(), binding: alt_v, spec: "Alt+V".into(), press: Press::at_once(1) };
        assert_eq!(mailbox::deliver(&*h, 1, &a, press), Delivered::Queued);
        if typed {
            h.backend.type_through();
        }
        h.hotkeys.borrow_mut().remove(&id);
        assert!(task::test_release(&*h, "w", Value::Nil));
        mailbox::run_queued(&*h);
        h.backend.take_passed().len()
    };
    assert_eq!(round(5, true), 0, "overtaken");
    assert_eq!(round(6, false), 1, "nothing typed since: passed on");
    assert_eq!(heard(&a), "", "no callback of A's ran");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// Nor while a screen reader's key is held — sent then, it would be the screen reader's command —
/// nor while a hotkey holds the combination, which the system would hand the key to instead: a
/// module's live registration, of this module or another, enabled or not, or the application's own
/// reload or quit key; one that does not hold it at the system lets it go. And a key goes on the physical key it was pressed on — the keypad's
/// Enter — where the hook named one; a hotkey's, which the system matched, on none.
#[test]
fn a_key_is_not_passed_on_to_a_screen_reader_or_a_hotkey_and_goes_on_its_own_key() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    run(&a, "heard = {}");
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    let (ctrl_v, ctrl) = backend::parse_key_spec("Ctrl+V").unwrap();
    // A screen reader's key held: dropped; let go, sent.
    h.backend.hold_reader(true);
    assert_eq!(captures::pass_on(&*h, 0x09, 0, at(&h, W1)), PassedOn::NotSent(NotPassed::ReaderModifier));
    h.backend.hold_reader(false);
    assert_eq!(captures::pass_on(&*h, 0x09, 0, at(&h, W1)), PassedOn::Sent);
    // B's hotkey holds Ctrl+V at the system: A's Ctrl+V is not sent, whatever else allows it.
    let f: Function = b.load("return function() end").eval().unwrap();
    let cb = b.create_registry_value(f).unwrap();
    let reg = crate::HotkeyReg { module_idx: 2, lua: b.clone(), cb, spec: "Ctrl+V".into(), binding: Some((ctrl_v, ctrl)), live: true, foreground: false, taps: Default::default() };
    h.hotkeys.borrow_mut().insert(9, reg);
    assert_eq!(captures::pass_on(&*h, ctrl_v, ctrl, at(&h, W1)), PassedOn::NotSent(NotPassed::HotkeyHolds));
    h.enabled.borrow_mut()[2] = false;
    assert_eq!(captures::pass_on(&*h, ctrl_v, ctrl, at(&h, W1)), PassedOn::NotSent(NotPassed::HotkeyHolds), "B off, its key still registered");
    // The other reasons come first: the window.
    h.backend.set_front(W2);
    assert_eq!(captures::pass_on(&*h, ctrl_v, ctrl, at(&h, W1)), PassedOn::NotSent(NotPassed::OtherWindow));
    h.backend.set_front(W1);
    // Not holding it at the system any more: sent.
    h.hotkeys.borrow_mut().get_mut(&9).unwrap().live = false;
    assert_eq!(captures::pass_on(&*h, ctrl_v, ctrl, at(&h, W1)), PassedOn::Sent);
    // The application's own reload and quit keys, always.
    let (f5, all) = backend::key_spec(crate::RELOAD_HOTKEY_SPEC).unwrap();
    assert_eq!(captures::pass_on(&*h, f5, all, at(&h, W1)), PassedOn::NotSent(NotPassed::HotkeyHolds));
    let (q, all) = backend::key_spec(crate::QUIT_HOTKEY_SPEC).unwrap();
    assert_eq!(captures::pass_on(&*h, q, all, at(&h, W1)), PassedOn::NotSent(NotPassed::HotkeyHolds));
    h.backend.take_passed();
    h.backend.take_passed_phys();

    // Through the mailbox, the keypad's Enter: on its own key.
    run(&a, r#"enter = host.keys.capture("Enter", function() heard[#heard + 1] = "enter" end)"#);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(enter)"#), Delivered::Parked);
    let (enter, none) = backend::parse_key_spec("Enter").unwrap();
    let keypad = Pressed { phys: Some(0xE01C), ..at(&h, W1) };
    assert_eq!(captures::arrive(&*h, enter, none, 1, false, keypad), Arrived::Delivered(Delivered::Queued));
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(h.backend.take_passed(), vec![(0x0D, 0, vec![KEY_DOWN, KEY_UP])]);
    assert_eq!(h.backend.take_passed_phys(), vec![Some(0xE01C)], "the keypad's Enter, not the main one");
    assert_eq!(heard(&a), "");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The maintainer's answer of 2026-10-05 to question a, in the scripted host: two modules in one
/// window, A's capture of Tab the earlier registration, B's the next. A is busy when Tab is pressed
/// — the hook takes it for A — and lets Tab go before the key's turn: the key goes to B, where the
/// hook would have sent it had A never captured it, through B's mailbox as a key of B's — at once
/// while B is free, behind B's own wait while B is busy — and the log says so; nothing reaches the
/// program. B letting go of it as well sends it on to the program, never back to A, whose capture
/// is there again by then: a key goes to one place, and on at most once per module. With no second
/// module, or B's capture pinned to another window, the program gets it, as before. A hotkey press
/// its busy module unregistered goes to B's capture of the combination the same way.
#[test]
fn a_key_a_busy_module_let_go_of_goes_to_the_next_module_in_its_window_first() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    let capture_a = r#"heard = heard or {}; tab = host.keys.capture("Tab", function() heard[#heard + 1] = "A tab" end); host.keys.scope(true)"#;
    let capture_b = r#"heard = heard or {}; btab = host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end); host.keys.scope(true)"#;
    let queued = Arrived::Delivered(Delivered::Queued);
    let tab_down_up = vec![(0x09, 0, vec![KEY_DOWN, KEY_UP])];
    run(&a, capture_a);
    run(&b, capture_b);
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 1 }), "A's is the earlier registration");

    // B free: B's at once.
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab)"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 2 }), "what the hook does now A let go");
    mailbox::run_queued(&*h);
    assert_eq!((heard(&a).as_str(), heard(&b).as_str()), ("", "B tab"));
    assert!(h.backend.take_passed().is_empty(), "the program got nothing");

    // B busy too: the key waits behind B's own wait, in B's mailbox, and runs once B is free.
    run(&a, capture_a);
    run(&b, "host.keys.release(btab)");
    run(&b, capture_b);
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Take { owner: 1 }));
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab)"#), Delivered::Parked);
    assert_eq!(event(&h, &b, 2, r#"wait("v")"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!((h.mail.len(1), h.mail.len(2)), (0, 1), "it waits for B now");
    assert_eq!(heard(&b), "B tab");
    assert!(task::test_release(&*h, "v", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!((heard(&a).as_str(), heard(&b).as_str()), ("", "B tab | B tab"));
    assert!(h.backend.take_passed().is_empty());

    // B lets go of it too, and A has captured Tab again meanwhile: not back to A — to the program.
    run(&a, capture_a);
    run(&b, "host.keys.release(btab)");
    run(&b, capture_b);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab)"#), Delivered::Parked);
    assert_eq!(event(&h, &b, 2, r#"wait("v"); host.keys.release(btab)"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(h.mail.len(2), 1, "with B");
    run(&a, capture_a);
    assert!(task::test_release(&*h, "v", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(h.backend.take_passed(), tab_down_up, "on to the program, once");
    assert_eq!((heard(&a).as_str(), heard(&b).as_str()), ("", "B tab | B tab"), "neither ran it");
    assert!(h.mail.is_empty());

    // B's capture pinned to another window: not B's — the program's.
    h.backend.set_front(W2);
    run(&b, capture_b);
    h.backend.set_front(W1);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab)"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(h.backend.take_passed(), tab_down_up, "B's capture is W2's");
    assert_eq!(heard(&b), "B tab | B tab");

    // No second module at all: the program's, as before.
    run(&b, "host.keys.release(btab)");
    run(&a, capture_a);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(tab)"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Tab", false, W1), queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(h.backend.take_passed(), tab_down_up);

    // A hotkey press, Alt+V, that waited for a registration A unregistered: B captures Alt+V in W1,
    // and gets it.
    run(&b, r#"host.keys.capture("Alt+V", function() heard[#heard + 1] = "B alt v" end); host.keys.scope(true)"#);
    let alt_v = backend::parse_key_spec("Alt+V").ok();
    let f: Function = a.load("return function() heard[#heard + 1] = 'A hotkey' end").eval().unwrap();
    let cb = a.create_registry_value(f).unwrap();
    let reg = crate::HotkeyReg { module_idx: 1, lua: a.clone(), cb, spec: "Alt+V".into(), binding: alt_v, live: true, foreground: false, taps: Default::default() };
    h.hotkeys.borrow_mut().insert(5, reg);
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    let hotkey = Event::Hotkey { id: 5, owner: 1, front: Some(W1), seq: h.backend.let_through(), binding: alt_v, spec: "Alt+V".into(), press: Press::at_once(1) };
    assert_eq!(mailbox::deliver(&*h, 1, &a, hotkey), Delivered::Queued);
    h.hotkeys.borrow_mut().remove(&5);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&b), "B tab | B tab | B alt v");
    assert_eq!(heard(&a), "");
    assert!(h.backend.take_passed().is_empty());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// Turns of the loop — the readings — until module `idx` has no handler any more, 10 s at most.
fn settle(h: &Host, idx: usize) {
    let until = std::time::Instant::now() + Duration::from_secs(10);
    while h.tasks.handler_of(idx).is_some() {
        assert!(std::time::Instant::now() < until, "the read never came");
        crate::ocr::lua::fire(h);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A module that waits for a read: its keys wait behind it, and a key whose capture was made again
/// meanwhile — the overlay went and came back through the arbiter — goes to the module's new
/// capture once the reading has come.
#[test]
fn keys_wait_behind_a_read_and_go_to_the_new_capture() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; tab = host.keys.capture("Tab", function() heard[#heard + 1] = "old tab" end); host.keys.scope(true)"#);
    let read = r#"heard[#heard + 1] = host.ocr.recognize({ 9921, 5, 9951, 15 }).text"#;
    assert_eq!(event(&h, &a, 1, read), Delivered::Parked, "it waits for the reading");
    assert_eq!(deliver(&h, "Tab", 1, W1), Arrived::Delivered(Delivered::Queued));
    run(&a, r#"host.keys.release(tab); host.keys.capture("Tab", function() heard[#heard + 1] = "new tab" end)"#);
    settle(&h, 1);
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "9921,5 | new tab");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A menu flag counts for its module's window while that module holds a capture there — also while
/// the module waits for a read, until the wait ends: Tab pressed in that window goes to the menu.
/// Another module's window is not touched. A stop turns the module off at once, and its flag goes
/// with its captures in the same hand-over to the hook.
#[test]
fn a_waiting_modules_menu_flag_holds_its_window_until_a_stop_lifts_it() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"host.keys.capture("Tab", function() end); host.keys.scope(true); host.keys.menuOpen(true)"#);
    h.backend.set_front(W2);
    run(&b, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end); host.keys.scope(true)"#);
    assert_eq!(event(&h, &a, 1, "host.ocr.recognize({ 9931, 5, 9961, 15 })"), Delivered::Parked);
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Pass { why: PassWhy::MenuFlag, owner: Some(1) }), "the menu has the keys");
    assert_eq!(press(&h, "Tab", W2), Some(Capture::Take { owner: 2 }), "B's window is B's");
    assert_eq!(deliver(&h, "Tab", 2, W2), RAN, "and B is free");
    // The stop's first step: off, and the keys handed back to the hook (`stops::mark`).
    h.enabled.borrow_mut()[1] = false;
    h.refresh_captured();
    assert_eq!(press(&h, "Tab", W1), Some(Capture::Pass { why: PassWhy::OutOfScope, owner: None }), "the flag went with the stop");
    assert_eq!(heard(&b), "B tab");
}

/// Without the keyboard hook — a Mac that has not granted Accessibility, a hook the system would
/// not install — `host.keys.capture` answers `nil, err` and records nothing: no capture is handed
/// to the backend, and none is left for a token the module never got (D30 of the typed-errors
/// plan). Once the hook is in, the next capture is taken as ever.
#[test]
fn a_capture_without_the_hook_answers_nil_and_records_nothing() {
    use crate::errors::{keys, HostError};
    let h = host();
    let a = vm(&h, 1);
    let refused = HostError::with(&keys::HOOK_REFUSED, "SetWindowsHookExW(WH_KEYBOARD_LL) failed (error 5)".into());
    h.backend.refuse_hook(Some(refused));
    run(&a, r#"token, err = host.keys.capture("Space", function() end)"#);
    assert!(yes(&a, "return token == nil and err.code == 'failed' and tostring(err) == 'SetWindowsHookExW(WH_KEYBOARD_LL) failed (error 5)'"));
    assert!(h.backend.keys().0.is_empty(), "nothing handed to the backend");
    assert!(press(&h, "Space", 0).is_none(), "the key is not taken");
    h.backend.refuse_hook(None);
    run(&a, r#"token, err = host.keys.capture("Space", function() end)"#);
    assert!(yes(&a, "return type(token) == 'number' and err == nil"));
    assert_eq!(h.backend.keys().0.len(), 1, "the capture is handed over once the hook is in");
}

// ── Quick repeats of a key: `press.count`, and `taps = "final"` (taps.rs) ───────────────────────

/// A key-down of `spec` the hook took for module 1 in W1, queued `ms` after `t0` — the moment the
/// count times it by — a repeat or not, as the pump delivers it.
fn tap(h: &Host, spec: &str, repeat: bool, t0: Instant, ms: u64) -> Arrived {
    let (vk, mask) = backend::parse_key_spec(spec).unwrap();
    let pressed = Pressed { queued: Some(t0 + Duration::from_millis(ms)), ..at(h, W1) };
    captures::arrive(h, vk, mask, 1, repeat, pressed)
}

/// Registers hotkey `id` on `spec` for module 1, in its VM `lua`, with `taps`: its callback the
/// Luau function `cb` evaluates to. Held at the system, as a granted registration is.
fn hotkey(h: &Host, lua: &Lua, id: i32, spec: &str, taps: Taps, cb: &str) {
    let f: Function = lua.load(cb).eval().unwrap();
    let cb = lua.create_registry_value(f).unwrap();
    let binding = backend::parse_key_spec(spec).ok();
    let reg = crate::HotkeyReg { module_idx: 1, lua: lua.clone(), cb, spec: spec.into(), binding, live: true, foreground: false, taps };
    h.hotkeys.borrow_mut().insert(id, reg);
}

/// A press of hotkey `id`, made `ms` after `t0`, `own` of the key-downs let through by then its
/// own (a Mac's tap counts a hotkey's key-down): as the pump hands it over (`on_hotkey`).
fn hotkey_press(h: &Host, id: i32, t0: Instant, ms: u64, own: u32) -> Option<taps::Handed> {
    let press = backend::HotkeyPress { seq: h.backend.let_through(), taps_seq: h.backend.taps_through(), own, at: t0 + Duration::from_millis(ms) };
    crate::hotkey_arrive_in(h, &h.hotkeys, id, press)
}

/// The tick, the pump having taken the presses queued until `ms` after `t0` (`input_taken_at`): a
/// sequence a `"final"` binding waits for is handed on once due by then.
fn tick(h: &Host, t0: Instant, ms: u64) {
    h.backend.set_input_taken(t0 + Duration::from_millis(ms));
    taps::tick(h);
}

const KEPT: Arrived = Arrived::Kept;

/// Every press carries its count to its callback at once — 1, 2, 3 for quick presses of one key: a
/// captured key's `press` after its `mods`, a hotkey's as its only argument — and the count starts
/// again for a press that comes the interval or more after the one before it. The interval is the
/// system's double-click time, read at every press, so a setting the user changes holds at the
/// next; or the binding's own `within`. A callback that takes no `press` runs as it always did.
#[test]
fn every_press_carries_its_count_and_the_interval_starts_it_again() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Tab", function(mods, press) heard[#heard + 1] = "tab " .. press.count end)
        host.keys.capture("Space", function() heard[#heard + 1] = "space" end)
        host.keys.capture("End", function(_, press) heard[#heard + 1] = "end " .. press.count end, { within = 1000 })"#,
    );
    let t0 = Instant::now();
    for ms in [0, 200, 400] {
        assert_eq!(tap(&h, "Tab", false, t0, ms), RAN);
    }
    // 499 ms after the press before it counts on, from that press and not the first; 500 ms is the
    // interval itself, past it.
    assert_eq!(tap(&h, "Tab", false, t0, 899), RAN);
    assert_eq!(tap(&h, "Tab", false, t0, 1399), RAN);
    assert_eq!(heard(&a), "tab 1 | tab 2 | tab 3 | tab 4 | tab 1");
    // The user sets 300 ms: read at the next press.
    h.backend.set_double_click(Duration::from_millis(300));
    run(&a, "heard = {}");
    for ms in [2000, 2250, 2600, 2850] {
        tap(&h, "Tab", false, t0, ms);
    }
    assert_eq!(heard(&a), "tab 1 | tab 2 | tab 1 | tab 2");
    // A binding's own interval, whatever the system's.
    run(&a, "heard = {}");
    for ms in [4000, 4900, 5850, 6900] {
        tap(&h, "End", false, t0, ms);
    }
    assert_eq!(heard(&a), "end 1 | end 2 | end 3 | end 1");
    // An old callback, with no `press`, is unaffected.
    run(&a, "heard = {}");
    assert_eq!(tap(&h, "Space", false, t0, 8000), RAN);
    assert_eq!(heard(&a), "space");

    // A hotkey's callback gets its press as its one argument.
    hotkey(&h, &a, 7, "Alt+V", Taps::default(), "return function(press) heard[#heard + 1] = 'alt v ' .. press.count end");
    run(&a, "heard = {}");
    for ms in [9000, 9100, 9200] {
        assert_eq!(hotkey_press(&h, 7, t0, ms, 0), Some(taps::Handed::Delivered(Delivered::Ran)));
    }
    assert_eq!(heard(&a), "alt v 1 | alt v 2 | alt v 3");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The count starts again at a press of another key, captured or a hotkey; of the same key with
/// other modifiers, another combination; and after a key-down that reached the program in between —
/// typed past every capture, let through, sent. A hotkey's own key-down in that count, which a Mac's
/// tap counts before Carbon hands the press over, is no other key; one more is. A registration made
/// again on the same combination — an overlay at every focus move — counts on, the key being the
/// combination and not the registration.
#[test]
fn the_count_starts_again_at_another_key_other_modifiers_or_a_key_let_through() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        local function note(word) return function(_, press) heard[#heard + 1] = word .. " " .. press.count end end
        host.keys.capture("Tab", note("tab"))
        host.keys.capture("Shift+Tab", note("shift tab"))
        host.keys.capture("Space", note("space"))"#,
    );
    hotkey(&h, &a, 7, "Alt+V", Taps::default(), "return function(press) heard[#heard + 1] = 'alt v ' .. press.count end");
    let t0 = Instant::now();
    tap(&h, "Tab", false, t0, 0);
    tap(&h, "Tab", false, t0, 100);
    tap(&h, "Space", false, t0, 200);
    tap(&h, "Tab", false, t0, 300);
    assert_eq!(heard(&a), "tab 1 | tab 2 | space 1 | tab 1", "another captured key");
    run(&a, "heard = {}");
    tap(&h, "Tab", false, t0, 400);
    hotkey_press(&h, 7, t0, 450, 0);
    tap(&h, "Tab", false, t0, 500);
    assert_eq!(heard(&a), "tab 2 | alt v 1 | tab 1", "a hotkey is another key");
    run(&a, "heard = {}");
    tap(&h, "Shift+Tab", false, t0, 600);
    tap(&h, "Tab", false, t0, 700);
    assert_eq!(heard(&a), "shift tab 1 | tab 1", "other modifiers");
    run(&a, "heard = {}");
    tap(&h, "Tab", false, t0, 800);
    h.backend.type_through();
    tap(&h, "Tab", false, t0, 900);
    assert_eq!(heard(&a), "tab 2 | tab 1", "a key-down reached the program between");

    // Windows: a hotkey's key-down is in no count; a key typed between starts it again.
    run(&a, "heard = {}");
    hotkey_press(&h, 7, t0, 2000, 0);
    hotkey_press(&h, 7, t0, 2100, 0);
    h.backend.type_through();
    hotkey_press(&h, 7, t0, 2200, 0);
    assert_eq!(heard(&a), "alt v 1 | alt v 2 | alt v 1");
    // A Mac: the tap counts each press's own key-down before Carbon hands the press over.
    run(&a, "heard = {}");
    for ms in [3000, 3100, 3200] {
        h.backend.type_through();
        hotkey_press(&h, 7, t0, ms, 1);
    }
    h.backend.type_through(); // a letter typed
    h.backend.type_through(); // and the press's own key-down
    hotkey_press(&h, 7, t0, 3300, 1);
    assert_eq!(heard(&a), "alt v 1 | alt v 2 | alt v 3 | alt v 1");
    // Made again under another id between two presses: one combination, one count.
    run(&a, "heard = {}");
    hotkey_press(&h, 7, t0, 4000, 0);
    h.hotkeys.borrow_mut().remove(&7);
    hotkey(&h, &a, 8, "Alt+V", Taps::default(), "return function(press) heard[#heard + 1] = 'again ' .. press.count end");
    hotkey_press(&h, 8, t0, 4100, 0);
    assert_eq!(heard(&a), "alt v 1 | again 2");
    // A press for an id nobody holds any more is still a key going down.
    tap(&h, "Tab", false, t0, 4200);
    hotkey_press(&h, 7, t0, 4250, 0);
    tap(&h, "Tab", false, t0, 4300);
    assert_eq!(heard(&a), "alt v 1 | again 2 | tab 1 | tab 1");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A key this application sent itself — `host.input.send`, from a module's callback — ends no count
/// (decided by the maintainer on 2026-10-23): the hook and the tap know it by its mark and leave it
/// out of the count that does. So a callback that sends a key at every press counts on, a captured
/// key's on the keypad — the request's own example, Numpad1 — as well as a hotkey's, and a key
/// another module sends between two presses of a `"final"` binding is no end of its sequence
/// either; a key typed between still ends the count. Sent, it reached the program all the same: a
/// key its busy module let go of meanwhile is not passed on after it.
#[test]
fn a_key_this_application_sent_ends_no_count() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    // `host.input.send`, as the backend sends it: here it reaches the program, as the hook lets a
    // sent key go on.
    let hh = h.clone();
    let send = a.create_function(move |_, spec: String| Ok(hh.backend.key_send(&spec).answer.is_ok())).unwrap();
    a.globals().set("send", send).unwrap();
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Numpad1", function(_, press) heard[#heard + 1] = "numpad1 " .. press.count; send("Down") end)
        host.keys.capture("Numpad2", function(_, press) heard[#heard + 1] = "numpad2 " .. press.count end, { taps = "final" })"#,
    );
    hotkey(&h, &a, 7, "Ctrl+NumpadAdd", Taps::default(), "return function(press) heard[#heard + 1] = 'add ' .. press.count; send('Ctrl+S') end");
    let t0 = Instant::now();
    for ms in [0, 100, 200] {
        assert_eq!(tap(&h, "Numpad1", false, t0, ms), RAN);
    }
    assert_eq!(heard(&a), "numpad1 1 | numpad1 2 | numpad1 3");
    assert_eq!((h.backend.let_through(), h.backend.taps_through()), (3, 0), "each key sent reached the program");
    // A key typed between still ends it.
    run(&a, "heard = {}");
    tap(&h, "Numpad1", false, t0, 300);
    h.backend.type_through();
    tap(&h, "Numpad1", false, t0, 400);
    assert_eq!(heard(&a), "numpad1 4 | numpad1 1");
    // A hotkey whose callback sends a key at every press.
    run(&a, "heard = {}");
    for ms in [1000, 1100, 1200] {
        assert_eq!(hotkey_press(&h, 7, t0, ms, 0), Some(taps::Handed::Delivered(Delivered::Ran)));
    }
    assert_eq!(heard(&a), "add 1 | add 2 | add 3");
    // A `"final"` binding: keys another module sent between its presses leave its sequence whole.
    run(&a, "heard = {}");
    assert_eq!(tap(&h, "Numpad2", false, t0, 2000), KEPT);
    h.backend.send_through();
    assert_eq!(tap(&h, "Numpad2", false, t0, 2100), KEPT);
    h.backend.send_through();
    h.backend.send_through();
    assert_eq!(tap(&h, "Numpad2", false, t0, 2200), KEPT);
    tick(&h, t0, 2700);
    assert_eq!(heard(&a), "numpad2 3");

    // Sent, it reached the program: a key its busy module let go of meanwhile is not passed on
    // after it, as for a key typed.
    run(&a, r#"heard = {}; tokens = { host.keys.capture("Delete", function() heard[#heard + 1] = "delete" end) }"#);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); for _, t in ipairs(tokens) do host.keys.release(t) end"#), Delivered::Parked);
    assert_eq!(arrive_for_1(&h, "Delete", false, W1), Arrived::Delivered(Delivered::Queued));
    h.backend.send_through();
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert!(h.backend.take_passed().is_empty(), "overtaken by the key sent");
    let before = Pressed { seq: h.backend.let_through().wrapping_sub(1), ..at(&h, W1) };
    assert_eq!(captures::pass_on(&*h, 0x2E, 0, before), PassedOn::NotSent(NotPassed::Overtaken));
    assert_eq!(heard(&a), "");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A held key's auto-repeat never counts and never starts the count again: each repeat carries the
/// count of the press it repeats, and the next press counts on from that press. For a `"final"`
/// capture a repeat is no press at all: nothing runs for it, and its sequence is heard once.
#[test]
fn a_held_key_s_auto_repeat_never_counts() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Down", function(_, press) heard[#heard + 1] = "down " .. press.count end)
        host.keys.capture("Up", function(_, press) heard[#heard + 1] = "up " .. press.count end, { taps = "final" })"#,
    );
    let t0 = Instant::now();
    tap(&h, "Down", false, t0, 0);
    tap(&h, "Down", true, t0, 30);
    tap(&h, "Down", true, t0, 60);
    tap(&h, "Down", false, t0, 300);
    tap(&h, "Down", true, t0, 330);
    tap(&h, "Down", false, t0, 600);
    assert_eq!(heard(&a), "down 1 | down 1 | down 1 | down 2 | down 2 | down 3");
    run(&a, "heard = {}");
    assert_eq!(tap(&h, "Up", false, t0, 2000), KEPT);
    assert_eq!(tap(&h, "Up", true, t0, 2030), Arrived::Skipped);
    assert_eq!(tap(&h, "Up", true, t0, 2060), Arrived::Skipped);
    assert_eq!(tap(&h, "Up", false, t0, 2300), KEPT);
    tick(&h, t0, 2799);
    assert_eq!(heard(&a), "", "due 500 ms after the last press, not a repeat");
    tick(&h, t0, 2800);
    assert_eq!(heard(&a), "up 2");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// `taps = "final"`: the callback runs once the sequence is over, with its count — one interval
/// after its last press, by the tick, never earlier; or at once when a press of another key starts
/// a count of its own, before that press runs. Its own `within` sets the interval. A hotkey's runs
/// the same way. A binding without it in the same module runs at every press.
#[test]
fn a_final_binding_hears_its_sequence_once_with_its_count() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Return", function(_, press) heard[#heard + 1] = "return " .. press.count end, { taps = "final" })
        host.keys.capture("End", function(_, press) heard[#heard + 1] = "end " .. press.count end, { taps = "final", within = 200 })
        host.keys.capture("Tab", function(_, press) heard[#heard + 1] = "tab " .. press.count end, { taps = "every" })"#,
    );
    let t0 = Instant::now();
    for ms in [0, 200, 400] {
        assert_eq!(tap(&h, "Return", false, t0, ms), KEPT);
    }
    assert_eq!(h.taps.waiting_due(), Some(t0 + Duration::from_millis(900)), "one interval after the last press");
    tick(&h, t0, 899);
    assert_eq!(heard(&a), "");
    let epoch = h.epoch.get();
    tick(&h, t0, 900);
    assert_eq!(heard(&a), "return 3", "once, with its count");
    assert_eq!(h.epoch.get(), epoch + 1, "the epoch turned for it, as a press's dispatch turns it");
    tick(&h, t0, 2000);
    assert_eq!(heard(&a), "return 3", "and only once");
    assert_eq!(h.taps.waiting_due(), None);

    // Another key ends it at once, and runs after it.
    run(&a, "heard = {}");
    tap(&h, "Return", false, t0, 3000);
    tap(&h, "Return", false, t0, 3200);
    assert_eq!(tap(&h, "Tab", false, t0, 3300), RAN);
    assert_eq!(heard(&a), "return 2 | tab 1");
    // A count that starts again for its own key ends it too: the next sequence is the next.
    run(&a, "heard = {}");
    tap(&h, "Return", false, t0, 4000);
    tap(&h, "Return", false, t0, 4600);
    assert_eq!(heard(&a), "return 1", "the second press came past the interval");
    tick(&h, t0, 5100);
    assert_eq!(heard(&a), "return 1 | return 1");
    // Its own interval.
    run(&a, "heard = {}");
    tap(&h, "End", false, t0, 6000);
    tap(&h, "End", false, t0, 6150);
    assert_eq!(h.taps.waiting_due(), Some(t0 + Duration::from_millis(6350)));
    tick(&h, t0, 6350);
    assert_eq!(heard(&a), "end 2");

    // A hotkey with taps = "final".
    let fin = Taps { mode: taps::Mode::Final, within: None };
    hotkey(&h, &a, 9, "Alt+M", fin, "return function(press) heard[#heard + 1] = 'alt m ' .. press.count end");
    run(&a, "heard = {}");
    for ms in [7000, 7300, 7600, 7900] {
        assert_eq!(hotkey_press(&h, 9, t0, ms, 0), Some(taps::Handed::Kept));
    }
    tick(&h, t0, 8399);
    assert_eq!(heard(&a), "");
    tick(&h, t0, 8400);
    assert_eq!(heard(&a), "alt m 4");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// While its module is busy — a handler waits — a press waits in the mailbox with the count it had
/// when it arrived, and runs with it once the module is free: what counts is when it was pressed. A
/// `"final"` sequence that ends meanwhile waits there too, with its count, in its turn.
#[test]
fn a_press_that_waits_behind_a_busy_module_keeps_its_count() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Tab", function(_, press) heard[#heard + 1] = "tab " .. press.count end)
        host.keys.capture("Return", function(_, press) heard[#heard + 1] = "return " .. press.count end, { taps = "final" })"#,
    );
    hotkey(&h, &a, 7, "Alt+V", Taps::default(), "return function(press) heard[#heard + 1] = 'alt v ' .. press.count end");
    assert_eq!(event(&h, &a, 1, r#"heard[#heard + 1] = "waits"; wait("w")"#), Delivered::Parked);
    let queued = Arrived::Delivered(Delivered::Queued);
    let t0 = Instant::now();
    for ms in [0, 100, 200] {
        assert_eq!(tap(&h, "Tab", false, t0, ms), queued);
    }
    assert_eq!(tap(&h, "Return", false, t0, 300), KEPT);
    assert_eq!(tap(&h, "Return", false, t0, 400), KEPT);
    tick(&h, t0, 900);
    assert_eq!(h.mail.len(1), 4, "the sequence's end waits in the mailbox too");
    assert_eq!(hotkey_press(&h, 7, t0, 1000, 0), Some(taps::Handed::Delivered(Delivered::Queued)));
    assert_eq!(hotkey_press(&h, 7, t0, 1100, 0), Some(taps::Handed::Delivered(Delivered::Queued)));
    assert_eq!(heard(&a), "waits");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "waits | tab 1 | tab 2 | tab 3 | return 2 | alt v 1 | alt v 2");
    assert!(h.mail.is_empty());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A `"final"` sequence stands for all of its presses: one whose capture or hotkey the module let go
/// of before it was heard is dropped and said, and never passed on to the program as one key, as a
/// single press would be. A disabled module's is dropped without a word.
#[test]
fn a_final_sequence_whose_registration_went_is_dropped_not_passed_on() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    h.backend.set_sends(true);
    run(
        &a,
        r#"heard = {}
        ret = host.keys.capture("Return", function(_, press) heard[#heard + 1] = "return " .. press.count end, { taps = "final" })"#,
    );
    let t0 = Instant::now();
    tap(&h, "Return", false, t0, 0);
    tap(&h, "Return", false, t0, 100);
    run(&a, "host.keys.release(ret)");
    tick(&h, t0, 600);
    assert_eq!(heard(&a), "");
    assert_eq!(h.backend.take_passed(), vec![], "not passed on");
    // Released while its busy module waited, the sequence waiting in the mailbox: the same.
    run(&a, r#"ret = host.keys.capture("Return", function(_, press) heard[#heard + 1] = "return " .. press.count end, { taps = "final" })"#);
    assert_eq!(event(&h, &a, 1, r#"wait("w"); host.keys.release(ret)"#), Delivered::Parked);
    tap(&h, "Return", false, t0, 1000);
    tap(&h, "Return", false, t0, 1100);
    tick(&h, t0, 1600);
    assert_eq!(h.mail.len(1), 1);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "");
    assert_eq!(h.backend.take_passed(), vec![]);
    // A hotkey's, the same.
    let fin = Taps { mode: taps::Mode::Final, within: None };
    hotkey(&h, &a, 9, "Alt+M", fin, "return function(press) heard[#heard + 1] = 'alt m ' .. press.count end");
    hotkey_press(&h, 9, t0, 2000, 0);
    h.hotkeys.borrow_mut().remove(&9);
    tick(&h, t0, 2500);
    assert_eq!(heard(&a), "");
    assert_eq!(h.backend.take_passed(), vec![]);
    // A module disabled meanwhile hears nothing.
    run(&a, r#"host.keys.capture("End", function() heard[#heard + 1] = "end" end, { taps = "final" })"#);
    tap(&h, "End", false, t0, 3000);
    h.enabled.borrow_mut()[1] = false;
    tick(&h, t0, 3500);
    assert_eq!(heard(&a), "");
    // A hotkey's whose busy module released it while the sequence waited in its mailbox: not sent
    // to the program as one Alt+M, as a single press of it would be.
    h.enabled.borrow_mut()[1] = true;
    hotkey(&h, &a, 10, "Alt+M", fin, "return function(press) heard[#heard + 1] = 'alt m ' .. press.count end");
    assert_eq!(event(&h, &a, 1, r#"wait("w2")"#), Delivered::Parked);
    hotkey_press(&h, 10, t0, 4000, 0);
    hotkey_press(&h, 10, t0, 4100, 0);
    tick(&h, t0, 4600);
    assert_eq!(h.mail.len(1), 1);
    h.hotkeys.borrow_mut().remove(&10);
    assert!(task::test_release(&*h, "w2", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "");
    assert_eq!(h.backend.take_passed(), vec![], "not passed on");
    assert_eq!(
        captures::final_dropped_line("m1", "Return", 2, Gone::Released),
        "[m1] Return, pressed 2 time(s) in a row, was not run: its registration was released before the sequence \
         was handed over. Said at most once every 10 s per module"
    );
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A `"final"` sequence is over by what the pump has taken: a press made before its deadline that
/// still waited in the backend's queue when the pump took it — the loop then busy with other
/// callbacks past the deadline — goes on with the sequence at the next pump, and the binding hears
/// 2, once. And a sequence once heard has ended its count: a press counted after it, though stamped
/// inside its interval by a clock a few milliseconds off, begins another — 1 and 1, never 1 and
/// then 2 for one sequence.
#[test]
fn a_final_sequence_is_heard_once_with_a_press_that_was_still_queued() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Return", function(_, press) heard[#heard + 1] = "return " .. press.count end, { taps = "final" })"#,
    );
    let t0 = Instant::now();
    assert_eq!(tap(&h, "Return", false, t0, 0), KEPT);
    // The pump took the queues at 470, the second press was queued at 480, and the tick ran at 510,
    // past the deadline: not over by what the pump had.
    tick(&h, t0, 470);
    assert_eq!(heard(&a), "");
    assert_eq!(tap(&h, "Return", false, t0, 480), KEPT);
    tick(&h, t0, 520);
    tick(&h, t0, 979);
    assert_eq!(heard(&a), "");
    tick(&h, t0, 980);
    assert_eq!(heard(&a), "return 2", "once, with both presses");

    // Heard, and then a press stamped inside its interval: another sequence.
    run(&a, "heard = {}");
    tap(&h, "Return", false, t0, 2000);
    tick(&h, t0, 2510);
    assert_eq!(heard(&a), "return 1");
    assert_eq!(tap(&h, "Return", false, t0, 2480), KEPT);
    tick(&h, t0, 2980);
    assert_eq!(heard(&a), "return 1 | return 1", "never 1 and then 2");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A key that ends a `"final"` sequence goes to the capture its module holds once that sequence has
/// run: the sequence is handed on at the key's arrival and runs at once, and its callback may give
/// the key back and capture it again — an overlay's focus move does so with its keys — and the key
/// goes to the new capture, as it would have without the sequence.
#[test]
fn a_key_that_ends_a_final_sequence_goes_to_the_capture_its_callback_made() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        local function tab(word) return function(_, press) heard[#heard + 1] = word .. " " .. press.count end end
        tabToken = host.keys.capture("Tab", tab("old tab"))
        host.keys.capture("Ctrl+1", function(_, press)
            heard[#heard + 1] = "ctrl 1 " .. press.count
            host.keys.release(tabToken)
            tabToken = host.keys.capture("Tab", tab("new tab"))
        end, { taps = "final" })"#,
    );
    let t0 = Instant::now();
    assert_eq!(tap(&h, "Ctrl+1", false, t0, 0), KEPT);
    assert_eq!(tap(&h, "Ctrl+1", false, t0, 150), KEPT);
    assert_eq!(tap(&h, "Tab", false, t0, 300), RAN);
    assert_eq!(heard(&a), "ctrl 1 2 | new tab 1");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A capture made again without `taps = "final"` between two quick presses: the second counts on,
/// and the sequence the earlier capture kept is handed on first, at once — to the capture the module
/// holds now — not after the press, by the tick.
#[test]
fn a_press_that_goes_on_with_a_kept_sequence_for_every_press_hands_it_on_first() {
    let h = host();
    let a = vm(&h, 1);
    h.backend.set_front(W1);
    run(
        &a,
        r#"heard = {}
        host.keys.capture("Return", function(_, press) heard[#heard + 1] = "final " .. press.count end, { taps = "final" })"#,
    );
    let t0 = Instant::now();
    assert_eq!(tap(&h, "Return", false, t0, 0), KEPT);
    run(&a, r#"host.keys.capture("Return", function(_, press) heard[#heard + 1] = "every " .. press.count end)"#);
    assert_eq!(tap(&h, "Return", false, t0, 200), RAN);
    assert_eq!(heard(&a), "every 1 | every 2");
    assert_eq!(h.taps.waiting_due(), None);
    tick(&h, t0, 1000);
    assert_eq!(heard(&a), "every 1 | every 2", "nothing left for the tick");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// `host.keys.capture`'s options as the binding reads them: a bad one raises — before the hook is
/// asked for, so nothing is captured — naming the call and what was wrong. A good one is kept with
/// the capture.
#[test]
fn a_capture_s_bad_option_raises_and_captures_nothing() {
    let h = host();
    let a = vm(&h, 1);
    for (opts, said) in [
        ("{ taps = 'twice' }", "host.keys.capture: taps is \"every\" or \"final\", not \"twice\""),
        ("{ within = 50 }", "host.keys.capture: within is a number of milliseconds from 100 to 5000, not 50"),
        ("{ within = '300' }", "within is a number of milliseconds from 100 to 5000, not a string"),
        ("{ final = true }", "host.keys.capture: 'final' is not an option; the options are taps and within"),
    ] {
        let e = a.load(format!("host.keys.capture('Tab', function() end, {opts})")).exec().unwrap_err().to_string();
        assert!(e.contains(said), "{opts}: {e}");
    }
    assert!(h.backend.keys().0.is_empty(), "nothing captured");
    let token: i64 = a.load("return host.keys.capture('Tab', function() end, { taps = 'final', within = 250.5 })").eval().unwrap();
    assert_eq!(h.captures.taps_of(token), Taps { mode: taps::Mode::Final, within: Some(Duration::from_millis(250)) });
}

/// A module releases its own captures only: the token of a capture another module holds raises,
/// naming that module, and the capture stands; the module's own token releases
/// as before. A token no capture holds now — released already, or replaced by a later capture —
/// releases nothing and raises nothing, whoever it was given to. Until 2026-10-24 the other
/// module's token released that module's capture.
#[test]
fn a_module_releases_its_own_captures_only() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; atab = host.keys.capture("Tab", function() heard[#heard + 1] = "A tab" end)"#);
    run(&b, r#"heard = {}; btab = host.keys.capture("Space", function() heard[#heard + 1] = "B space" end)"#);
    let btab: i64 = b.load("return btab").eval().unwrap();
    a.globals().set("btab", btab).unwrap();
    let e = a.load("host.keys.release(btab)").set_name("=mod.luau").exec().unwrap_err().to_string();
    assert!(
        e.contains(&format!("host.keys.release: token {btab} is a capture of module 'm2'; a module releases only its own captures")),
        "{e}"
    );
    assert_eq!(press(&h, "Space", W1), Some(Capture::Take { owner: 2 }), "B's capture stands");
    run(&a, "host.keys.release(atab)");
    assert_eq!(press(&h, "Tab", W1), None, "A's own token releases it");
    // Stale: released already, or replaced; nobody's capture now, so nothing to refuse.
    run(&a, "host.keys.release(atab)");
    run(&b, r#"old = btab; btab = host.keys.capture("Space", function() heard[#heard + 1] = "B space again" end)"#);
    let old: i64 = b.load("return old").eval().unwrap();
    a.globals().set("old", old).unwrap();
    run(&a, "host.keys.release(old)");
    assert_eq!(press(&h, "Space", W1), Some(Capture::Take { owner: 2 }));
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}
