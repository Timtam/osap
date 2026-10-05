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

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use mlua::{Function, Lua, Value};

use crate::backend::stub::StubBackend;
use crate::backend::{self, Backend, Capture, CaptureSource, OwnerKeys, PassWhy};
use crate::captures::{self, Arrived, Captures, Gone, KeyHost};
use crate::image_search::VmOwner;
use crate::mailbox::{self, Delivered, Event, MailHost, Mailboxes, Opened, Why};
use crate::ocr::lua::{OcrState, ReadHost};
use crate::ocr::service::{Service, ShutdownHandle};
use crate::task::{self, Tasks};
use crate::task_tests::{worker, Fake};

/// What the host keeps for the keys and for a module that waits, without the speech engines.
struct Host {
    ocr: Service<Fake>,
    stop: ShutdownHandle,
    state: OcrState,
    tasks: Tasks,
    mail: Mailboxes,
    gens: RefCell<HashMap<usize, u64>>,
    enabled: RefCell<Vec<bool>>,
    epoch: Cell<u64>,
    /// (module, context, message) of every error reported.
    errors: RefCell<Vec<(usize, String, String)>>,
    captures: Captures,
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
    fn read_source(&self, _: &Lua, _: (i32, i32, i32, i32)) -> CaptureSource {
        CaptureSource::Standard
    }
    fn slow_reads(&self) -> bool {
        false
    }
}

/// The events these modules hear: captured keys, hotkeys, and the tests' own.
impl MailHost for Host {
    fn mail(&self) -> &Mailboxes {
        &self.mail
    }
    fn open(&self, idx: usize, _: &Lua, ev: Event) -> Opened {
        match ev {
            Event::Key { owner, token, vk, mods, front, .. } => captures::open_key(self, owner, token, vk, mods, front),
            Event::Hotkey { id, owner, front, binding, spec } => {
                crate::open_hotkey_in(self, &self.hotkeys, id, owner, front, binding, &spec)
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
    fn key_backend(&self) -> &dyn Backend {
        &self.backend
    }
    fn key_module_enabled(&self, idx: usize) -> bool {
        self.enabled.borrow().get(idx).copied().unwrap_or(false)
    }
    fn key_module_id(&self, idx: usize) -> String {
        format!("m{idx}")
    }
}

fn host() -> Rc<Host> {
    let (ocr, stop) = Service::spawn(worker());
    Rc::new(Host {
        ocr,
        stop,
        state: OcrState::default(),
        tasks: Tasks::default(),
        mail: Mailboxes::default(),
        gens: RefCell::new(HashMap::new()),
        enabled: RefCell::new(Vec::new()),
        epoch: Cell::new(0),
        errors: RefCell::new(Vec::new()),
        captures: Captures::default(),
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
    let legacy = lua
        .create_function(|lua, (_what, _opts, why): (Value, Value, String)| {
            Ok((false, Value::String(lua.create_string(task::wait_message(task::Case::of(&why)))?)))
        })
        .unwrap();
    let waits = task::waits(&lua, idx, h.clone(), legacy, submit).unwrap();
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
    backend::capture_decision(&set, &owners, vk, mask, front, || false)
}

/// The pump's delivery of a key-down of `spec` the hook took for `owner` in window `front`.
fn deliver(h: &Host, spec: &str, owner: u32, front: isize) -> Arrived {
    let (vk, mask) = backend::parse_key_spec(spec).unwrap();
    captures::arrive(h, vk, mask, owner as usize, false, front)
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
        captures::arrive(&*h, vk, mask, 1, repeat, W1)
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
/// everywhere; never to another module's capture of the key, and not when the scope is another
/// window now. One whose capture is gone for good is dropped, said.
#[test]
fn a_key_that_waited_goes_to_its_module_s_new_capture_and_never_to_another_module() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    h.backend.set_front(W1);
    run(&a, r#"heard = {}; tab = host.keys.capture("Tab", function() heard[#heard + 1] = "old tab" end); host.keys.scope(true)"#);
    run(&b, r#"heard = {}; host.keys.capture("Tab", function() heard[#heard + 1] = "B tab" end)"#);
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    let tab = || {
        let (vk, mask) = backend::parse_key_spec("Tab").unwrap();
        captures::arrive(&*h, vk, mask, 1, false, W1)
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
    // Released for good while it waited: dropped.
    run(&a, "host.keys.scope(false)");
    assert_eq!(event(&h, &a, 1, r#"wait("w")"#), Delivered::Parked);
    assert_eq!(tab(), Arrived::Delivered(Delivered::Queued));
    run(&a, "host.keys.releaseAll()");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "new tab | new tab");
    assert_eq!(heard(&b), "");
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
    assert_eq!(
        captures::busy_dropped_line("m1", "Tab", Gone::Released),
        "[m1] Tab, pressed while the module was busy, was dropped: its registration was released meanwhile"
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
        let reg = crate::HotkeyReg { module_idx: owner, lua: lua.clone(), cb, spec: "Alt+V".into(), binding, live: true };
        h.hotkeys.borrow_mut().insert(id, reg);
    };
    let press = |id: i32| Event::Hotkey { id, owner: 1, front: Some(W1), binding, spec: "Alt+V".into() };
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
