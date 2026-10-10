//! The mailboxes against real Luau VMs and the real read service over the fake recogniser of
//! task_tests.rs: every event as a handler of its module, one at a time per module, and what waits,
//! folds and drops while a module is busy (`mailbox.rs`).
//!
//! The holder below stands where the host's `Shared` stands for the events it carries: the answers
//! to reads, timers, window events through the real window prelude, settings, controller events
//! and the tests' own (`Event::Call`). Which callback an event of each kind runs is its site's own
//! helper — `ocr::lua::open_read`, the timers', the prelude's calls — so the rules met here are the
//! host's.
//!
//! **Busy** is a module whose handler waits: at a read, `host.ocr.recognize` without a callback,
//! or at the tests' own wait point (`task::test_wait`), answered by `task::test_release` — a wait
//! no read stands behind, so a test decides when it ends.

use std::cell::{Cell, Ref, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::gamepad::{Axis, Button, Family, PadEvent, PadEventKind};
use crate::backend::{CaptureSource, WinInfo};
use crate::gamepad_api::Delivery;
use crate::image_search::VmOwner;
use crate::mailbox::{self, Delivered, Event, MailHost, Mailboxes, Opened, PadKind, Phase, Why};
use crate::ocr::lua::{self as reads, OcrState, ReadHost};
use crate::ocr::service::{Service, ShutdownHandle};
use crate::task::{self, Ctx, Tasks};
use crate::task_tests::{worker, Fake};
use crate::timers::{self, Timers};

/// What the host keeps for these events, without the speech engines.
struct Host {
    ocr: Service<Fake>,
    stop: ShutdownHandle,
    state: OcrState,
    tasks: Tasks,
    mail: Mailboxes,
    timers: Timers,
    gens: RefCell<HashMap<usize, u64>>,
    enabled: RefCell<Vec<bool>>,
    epoch: Cell<u64>,
    /// (module, context, message) of every error reported.
    errors: RefCell<Vec<(usize, String, String)>>,
    /// `onChange` registrations, as the host keeps them (`Event::Setting`, opened by the host's own
    /// `open_on_change`).
    settings: RefCell<crate::OnChangeMap>,
    /// Controller listeners, as the host keeps them (`Event::Pad`, opened by the host's own
    /// `open_pad_in`).
    pads: crate::gamepad_api::Pads,
    /// Image searches waiting or held, as the host keeps them (`Event::Image`, opened by the host's
    /// own `open_image_in`).
    images: RefCell<HashMap<u64, crate::image_search::PendingImage>>,
    /// Snapshot requests' keys, and the bytes their pictures hold (`Event::Snapshot`, opened by the
    /// host's own `open_snapshot_in`).
    snaps: crate::snapshot::SnapState,
    snap_bytes: Rc<Cell<usize>>,
    /// Every event that was discarded, and why.
    discarded: RefCell<Vec<(usize, &'static str, Why)>>,
    /// The modules the host's guard stopped (`stops.rs`), as the host's `Stops` answers.
    stopped: RefCell<BTreeSet<usize>>,
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
}

fn kind_of(ev: &Event) -> &'static str {
    match ev {
        Event::Hotkey { .. } => "hotkey",
        Event::Key { .. } => "key",
        Event::Pad { .. } => "pad",
        Event::Activate { .. } => "activate",
        Event::Initial { .. } => "initial",
        Event::Focus => "focus",
        Event::After { .. } => "after",
        Event::Every { .. } => "every",
        Event::Read(_) => "read",
        Event::Image { .. } => "image",
        Event::Snapshot { .. } => "snapshot",
        Event::Setting { .. } => "setting",
        Event::Call { .. } => "call",
    }
}

impl MailHost for Host {
    fn mail(&self) -> &Mailboxes {
        &self.mail
    }
    fn module_stopped(&self, idx: usize) -> bool {
        self.stopped.borrow().contains(&idx)
    }
    fn open(&self, idx: usize, lua: &Lua, ev: Event) -> Opened {
        let window = |what: &'static str, call: mlua::Result<Option<(Function, MultiValue)>>| match call {
            Ok(Some((f, args))) => Opened::Run { f, args, ctx: Ctx::new(what, idx) },
            Ok(None) => Opened::Gone,
            Err(e) => {
                self.report_error(idx, what, &e.to_string());
                Opened::Gone
            }
        };
        let timer = |f: Option<Function>| match f {
            Some(f) => Opened::Run { f, args: MultiValue::new(), ctx: Ctx::new("timer", idx) },
            None => Opened::Gone,
        };
        match ev {
            Event::Read(r) => reads::open_read(self, *r),
            Event::After { token } => timer(self.timers.take_queued(token)),
            Event::Every { token } => timer(self.timers.every_fn(token)),
            Event::Activate { win, upto } => window("window trigger", crate::activate_call(lua, &win, Some(upto))),
            Event::Initial { win, reprime } => {
                let hook = lua.globals().get::<Option<Function>>("prewarm").unwrap_or(None);
                window("window trigger", crate::initial_call(lua, win.as_deref(), reprime, hook))
            }
            Event::Focus => window("focus change", crate::focus_call(lua)),
            Event::Setting { reg, key, new, old, setting_of } => {
                crate::open_on_change(&self.settings, setting_of, reg, &key, &new, old.as_ref())
            }
            Event::Pad { token, event, delivery, .. } => {
                crate::gamepad_api::open_pad_in(&self.pads, token, &event, &delivery, &|i, w, m| self.report_error(i, w, m))
            }
            Event::Image { p, outcome, ended } => {
                crate::image_search::open_image_in(&self.images, &self.enabled, &self.gens, *p, outcome, ended)
            }
            Event::Snapshot { p, answer } => crate::snapshot::open_snapshot_in(&self.snaps, *p, answer, &self.snap_bytes, &|i, w, m| {
                self.report_error(i, w, m)
            }),
            Event::Call { f, args, what, .. } => mailbox::open_call(idx, f, args, what),
            _ => Opened::Gone,
        }
    }
    fn discard(&self, idx: usize, ev: Event, why: Why) {
        self.discarded.borrow_mut().push((idx, kind_of(&ev), why));
        match ev {
            Event::Read(r) => reads::discard_read(self, *r),
            Event::After { token } => self.timers.drop_queued(token),
            Event::Image { p, ended, .. } => crate::image_search::discard_image_in(&self.images, *p, ended, why == Why::Disabled),
            Event::Snapshot { p, .. } => crate::snapshot::drop_answer(&self.snaps, *p),
            _ => {}
        }
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
        timers: Timers::default(),
        gens: RefCell::new(HashMap::new()),
        enabled: RefCell::new(Vec::new()),
        epoch: Cell::new(0),
        errors: RefCell::new(Vec::new()),
        settings: RefCell::new(HashMap::new()),
        pads: crate::gamepad_api::Pads::default(),
        images: RefCell::new(HashMap::new()),
        snaps: crate::snapshot::SnapState::default(),
        snap_bytes: Rc::new(Cell::new(0)),
        discarded: RefCell::new(Vec::new()),
        stopped: RefCell::new(BTreeSet::new()),
        _loop: crate::loop_guard::mark_for_a_test(),
    })
}

/// VM generations, process-wide as the host's are; apart from the other tests' holders'.
static NEXT_GEN: AtomicU64 = AtomicU64::new(700_000);

/// The clock `host.now()` reads in these VMs: real milliseconds, as the host's.
fn now_ms() -> i64 {
    crate::clock_origin().elapsed().as_millis() as i64
}

/// A fresh VM for module `idx` — a new generation of it, enabled — with the host's
/// `host.ocr.recognize`, timer and window bindings over `h`, the window prelude, the tests' wait
/// point as the global `wait`, and `heard`, a list the callbacks write to. `host.log.info` writes
/// to the global `logs`. Where `recognize` cannot wait, it raises the message it is to raise there.
fn vm(h: &Rc<Host>, idx: usize) -> Lua {
    vm_in(h, idx, Lua::new())
}

/// [`vm`], in the VM `lua` given — one the host's guard made, say.
fn vm_in(h: &Rc<Host>, idx: usize, lua: Lua) -> Lua {
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
    host.set("timer", timers::table(&lua, idx, h.clone(), |h| &h.timers, |_| Instant::now()).unwrap()).unwrap();
    let ocr = lua.create_table().unwrap();
    let hh = h.clone();
    let submit = lua
        .create_function(move |lua, (what, opts, cb): (Value, Value, Value)| reads::read(&*hh, lua, idx, what, opts, cb))
        .unwrap();
    let legacy = lua
        .create_function(|lua, (_what, _opts, why): (Value, Value, String)| {
            let message = lua.create_string(task::wait_message(task::Case::of(&why)))?;
            Ok((false, Value::String(message)))
        })
        .unwrap();
    let waits = task::waits(&lua, idx, h.clone(), legacy, submit).unwrap();
    ocr.set("recognize", waits.get::<Function>("recognize").unwrap()).unwrap();
    let hh = h.clone();
    ocr.set("pending", lua.create_function(move |lua, key: Value| reads::pending(&*hh, lua, idx, key)).unwrap()).unwrap();
    host.set("ocr", ocr).unwrap();
    host.set("window", lua.create_table().unwrap()).unwrap();
    let os = lua.create_table().unwrap();
    os.set("current", "windows").unwrap();
    host.set("os", os).unwrap();
    host.set("now", lua.create_function(|_, ()| Ok(now_ms())).unwrap()).unwrap();
    let log = lua.create_table().unwrap();
    log.set(
        "info",
        lua.create_function(|lua, line: String| {
            let logs: Table = lua.globals().get("logs")?;
            logs.raw_push(line)
        })
        .unwrap(),
    )
    .unwrap();
    host.set("log", log).unwrap();
    crate::install_window_prelude(&lua, &host).unwrap();
    lua.globals().set("host", host).unwrap();
    lua.globals().set("wait", task::test_wait(&lua, idx, h.clone()).unwrap()).unwrap();
    lua.load("heard = {}; logs = {}").exec().unwrap();
    lua
}

/// Gives VM `lua` of module `idx` the host's `waited`, which the prelude's `[dispatch]` line
/// subtracts — as `populate_vm` does.
fn with_waited(h: &Rc<Host>, lua: &Lua, idx: usize) {
    let hh = h.clone();
    lua.set_named_registry_value(crate::WAITED_KEY, lua.create_function(move |_, ()| Ok(hh.tasks.waited_ms(idx))).unwrap())
        .unwrap();
}

/// Runs module code, as `mod.luau` — at the top level, or as a callback the host calls directly:
/// an arbiter's `onActivate`.
fn run(lua: &Lua, code: &str) {
    if let Err(e) = lua.load(code).set_name("=mod.luau").exec() {
        panic!("{e}");
    }
}

fn yes(lua: &Lua, code: &str) -> bool {
    matches!(lua.load(code).set_name("=check").eval::<Value>(), Ok(Value::Boolean(true)))
}

fn heard(lua: &Lua) -> String {
    lua.load("return table.concat(heard, ' | ')").eval::<String>().unwrap()
}

/// `code`, as an event of module `idx`'s through its mailbox: an input (a key) or not.
fn call(h: &Host, lua: &Lua, idx: usize, code: &str, input: bool) -> Delivered {
    let f: Function = lua.load(code).set_name("=mod.luau").into_function().unwrap();
    let what = if input { "key" } else { "timer" };
    mailbox::deliver(h, idx, lua, Event::Call { f, args: MultiValue::new(), input, what })
}

/// One that says `word` in `heard`.
fn say(h: &Host, lua: &Lua, idx: usize, word: &str, input: bool) -> Delivered {
    call(h, lua, idx, &format!("heard[#heard + 1] = '{word}'"), input)
}

/// Queued phases until no mailbox holds anything, at most `n`; how many ran.
fn drain(h: &Host, n: usize) -> usize {
    for i in 0..n {
        if h.mail.is_empty() {
            return i;
        }
        mailbox::run_queued(h);
    }
    n
}

fn window(title: &str) -> WinInfo {
    WinInfo {
        hwnd: 7,
        title: title.into(),
        class: "SynthWindow".into(),
        pid: 1,
        exe: "synth.exe".into(),
        bundle_id: String::new(),
        x: 0,
        y: 0,
        w: 800,
        h: 600,
        client_x: 0,
        client_y: 0,
        client_w: 800,
        client_h: 600,
    }
}

/// Turns of the loop until `done` or 10 s: the read service's answers handed to the mailboxes.
fn fire_until(h: &Host, done: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !done() && Instant::now() < until {
        reads::fire(h);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// The timers due at `at`, through each module's mailbox as the host fires them.
fn fire_timers(h: &Host, at: Instant) {
    h.timers.fire_due(
        at,
        |idx| match (h.module_enabled(idx), mailbox::busy(h, idx)) {
            (false, _) => timers::Slot::Off,
            (true, true) => timers::Slot::Busy,
            (true, false) => timers::Slot::Free,
        },
        || h.bump_epoch(),
        |due| match due {
            timers::Due::Once { token, idx, lua } => {
                mailbox::deliver(h, idx, &lua, Event::After { token });
            }
            timers::Due::Every { token, idx, lua } => {
                mailbox::deliver(h, idx, &lua, Event::Every { token });
            }
        },
    );
}

// ── One handler per module ───────────────────────────────────────────────────────────────────

/// A free module's event runs at once, as a handler; one that comes while its handler waits waits
/// in its mailbox, in order, and runs in the next queued phase after the handler ended — not
/// while it waits. Another module is not held up. And a callback is a coroutine now:
/// `coroutine.running()` is not nil in it, and it can yield.
#[test]
fn a_busy_modules_events_wait_in_order_and_another_module_is_free() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    assert_eq!(say(&h, &a, 1, "one", false), Delivered::Ran);
    assert_eq!(
        call(&h, &a, 1, "heard[#heard + 1] = 'waits'; wait('w'); heard[#heard + 1] = 'goes on'", false),
        Delivered::Parked
    );
    assert_eq!(say(&h, &a, 1, "two", true), Delivered::Queued);
    assert_eq!(say(&h, &a, 1, "three", false), Delivered::Queued);
    assert_eq!(say(&h, &b, 2, "b", true), Delivered::Ran, "another module is free");
    assert_eq!(heard(&a), "one | waits");
    assert_eq!(mailbox::run_queued(&*h), Phase::default(), "nothing runs while the handler waits");
    assert!(task::test_release(&*h, "w", Value::Nil));
    assert_eq!(heard(&a), "one | waits | goes on", "the queue waits for the next phase");
    let epoch = h.epoch.get();
    let phase = mailbox::run_queued(&*h);
    assert_eq!((phase.events, phase.parked, phase.budget), (2, 0, false));
    assert_eq!(h.epoch.get(), epoch + 1, "the phase turns the epoch once");
    assert_eq!(heard(&a), "one | waits | goes on | two | three");
    assert!(h.mail.is_empty() && h.tasks.handler_of(1).is_none());
    // A callback is a coroutine of the host now.
    call(&h, &a, 1, "inCallback = { running = coroutine.running() ~= nil, yieldable = coroutine.isyieldable() }", false);
    assert!(yes(&a, "return inCallback.running == true and inCallback.yieldable == true"));
    assert!(yes(&a, "return coroutine.running() == nil"), "at the top level, as ever");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// No handler starts inside another: an event for module B delivered while A's handler runs
/// waits for the queued phase, however free B is.
#[test]
fn no_handler_starts_inside_another() {
    let h = host();
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    let (hh, bb) = (h.clone(), b.clone());
    a.globals()
        .set(
            "toB",
            a.create_function(move |_, code: String| {
                let d = say(&hh, &bb, 2, &code, false);
                Ok(format!("{d:?}"))
            })
            .unwrap(),
        )
        .unwrap();
    call(&h, &a, 1, "got = toB('from A'); heard[#heard + 1] = 'A done'", false);
    assert!(yes(&a, "return got == 'Queued'"));
    assert_eq!((heard(&a).as_str(), heard(&b).as_str()), ("A done", ""));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&b), "from A");
}

/// What a handler raises, before a wait or after one, is reported under its event's kind and its
/// module — the error context stays with it across the wait. The mailbox goes on with the next.
#[test]
fn a_handlers_error_after_a_wait_keeps_its_context() {
    let h = host();
    let a = vm(&h, 1);
    let f: Function = a.load("wait('w'); error('after the wait')").into_function().unwrap();
    let d = mailbox::deliver(&*h, 1, &a, Event::Call { f, args: MultiValue::new(), input: true, what: "hotkey" });
    assert_eq!(d, Delivered::Parked);
    say(&h, &a, 1, "next", true);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    let errors = h.errors.borrow().clone();
    assert!(errors.len() == 1 && errors[0].0 == 1 && errors[0].1 == "hotkey" && errors[0].2.contains("after the wait"), "{errors:?}");
    assert_eq!(heard(&a), "next");
}

/// A callback's own `coroutine.yield` through to the host raises at the yield, with the message
/// module-runtime-and-lifecycle.md gives: a `pcall` around it goes on; without one the callback
/// ends with it, reported.
#[test]
fn a_callbacks_own_yield_raises_at_the_yield() {
    let h = host();
    let a = vm(&h, 1);
    call(&h, &a, 1, "ok, err = pcall(coroutine.yield, 'anything'); err = tostring(err); after = true", false);
    assert!(yes(&a, "return ok == false and after == true"));
    let err: String = a.globals().get("err").unwrap();
    assert!(err.contains(task::OWN_YIELD), "{err}");
    call(&h, &a, 1, "coroutine.yield()", false);
    let errors = h.errors.borrow().clone();
    assert!(errors.len() == 1 && errors[0].1 == "timer" && errors[0].2.contains(task::OWN_YIELD), "{errors:?}");
    assert!(h.tasks.handler_of(1).is_none(), "nothing spins, nothing waits");
}

// ── What waits, what folds, what drops ───────────────────────────────────────────────────────

/// The limit counts inputs only: the 257th key a busy module has waiting is dropped, and an answer,
/// a timer, a window event or a setting after it still waits — and a controller button's release,
/// whose press was heard, while another press is dropped. Everything that waited runs, in order,
/// once the module is free.
#[test]
fn the_limit_drops_inputs_only() {
    let h = host();
    let a = vm(&h, 1);
    run(&a, "n = 0");
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    for _ in 0..mailbox::INPUT_LIMIT {
        assert_eq!(call(&h, &a, 1, "n = n + 1", true), Delivered::Queued);
    }
    assert_eq!(call(&h, &a, 1, "n = n + 1", true), Delivered::Dropped, "the 257th key");
    assert_eq!(say(&h, &a, 1, "a timer", false), Delivered::Queued, "not an input: never dropped");
    assert_eq!(
        mailbox::deliver(&*h, 1, &a, Event::Focus),
        Delivered::Queued,
        "a window event waits too"
    );
    let f: Function = a.load("return function() heard[#heard + 1] = 'button' end").eval().unwrap();
    let token = h.pads.listen_for_test(1, &a, f, crate::gamepad_api::Kind::Down);
    let button = |kind: PadKind, ev: PadEventKind| Event::Pad {
        token,
        kind,
        event: Box::new(PadEvent {
            pad: 0,
            kind: ev,
            at: Instant::now(),
            synthetic: false,
            family: Family::Xbox,
            held: BTreeSet::new(),
            report: 0,
        }),
        delivery: Delivery { token, event: 0, value: None, partner: false },
    };
    assert_eq!(
        mailbox::deliver(&*h, 1, &a, button(PadKind::Button, PadEventKind::Down(Button::South))),
        Delivered::Dropped,
        "a button's press is an input"
    );
    assert_eq!(
        mailbox::deliver(&*h, 1, &a, button(PadKind::Release, PadEventKind::Up(Button::South))),
        Delivered::Queued,
        "its release is never dropped: the listener that heard a press hears its release"
    );
    assert_eq!(h.discarded.borrow().as_slice(), &[(1, "call", Why::Limit), (1, "pad", Why::Limit)]);
    assert!(task::test_release(&*h, "w", Value::Nil));
    drain(&h, 10);
    assert!(yes(&a, &format!("return n == {}", mailbox::INPUT_LIMIT)));
    assert_eq!(heard(&a), "a timer | button");
    assert_eq!(
        mailbox::limit_line("m1", 1, "timer", 2300),
        "[m1] 1 key(s) or button(s) pressed while it was busy were dropped: 256 were already waiting (the module \
         waited for timer, 2300 ms by the first of them)"
    );
}

/// A focus change is folded into the one queued last only: [focus, key] and a new focus are two
/// rechecks, one before the key and one after it; [key, focus] and a new focus are one. An axis
/// likewise replaces the value of the same listener's same axis queued last, and comes last.
#[test]
fn focus_and_axes_fold_into_the_last_one_only() {
    let h = host();
    let a = vm(&h, 1);
    run(&a, "host.window.onFocus(function() heard[#heard + 1] = 'focus' end)");
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, Event::Focus), Delivered::Queued);
    say(&h, &a, 1, "key", true);
    assert_eq!(mailbox::deliver(&*h, 1, &a, Event::Focus), Delivered::Queued, "after the key: a recheck of its own");
    assert_eq!(mailbox::deliver(&*h, 1, &a, Event::Focus), Delivered::Dropped, "folded into the last");
    assert_eq!(h.mail.len(1), 3);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "focus | key | focus");

    // Axes.
    run(&a, "heard = {}");
    let f: Function = a.load("return function(e) heard[#heard + 1] = 'x=' .. e.value end").eval().unwrap();
    let token = h.pads.listen_for_test(1, &a, f, crate::gamepad_api::Kind::Axis);
    let axis = |axis: Axis, v: f32| Event::Pad {
        token,
        kind: PadKind::Axis(axis),
        event: Box::new(PadEvent {
            pad: 0,
            kind: PadEventKind::Axis { axis, value: v, partner: 0.0 },
            at: Instant::now(),
            synthetic: false,
            family: Family::Xbox,
            held: BTreeSet::new(),
            report: 0,
        }),
        delivery: Delivery { token, event: 0, value: Some(v), partner: false },
    };
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, axis(Axis::LeftX, 0.25)), Delivered::Queued);
    assert_eq!(mailbox::deliver(&*h, 1, &a, axis(Axis::LeftX, 0.5)), Delivered::Dropped, "replaces the last");
    say(&h, &a, 1, "button", true);
    assert_eq!(mailbox::deliver(&*h, 1, &a, axis(Axis::LeftX, 0.75)), Delivered::Queued, "after the button: its own");
    assert_eq!(mailbox::deliver(&*h, 1, &a, axis(Axis::RightTrigger, 1.0)), Delivered::Queued, "another axis");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "x=0.5 | button | x=0.75 | x=1");
}

// ── Every handler waits ──────────────────────────────────────────────────────────────────────

/// Every kind of event a module hears runs as a handler that waits at `host.ocr.recognize` without
/// a callback — a timer, a window trigger, a focus change, a setting's `onChange`, a controller
/// press, the answers of an image search, of a snapshot and of a read with a callback — and goes on
/// with the reading once it comes. One module each, so each waits on its own and none holds another.
#[test]
fn every_kind_of_handler_waits_at_a_read() {
    let h = host();
    let read = |kind: &str, x: i32| format!("heard[#heard + 1] = '{kind} ' .. host.ocr.recognize({{ {x}, 5, {x} + 30, 15 }}).text");
    let f = |lua: &Lua, body: String| -> Function { lua.load(format!("return function() {body} end")).eval().unwrap() };
    let mut modules: Vec<(Lua, &str)> = Vec::new();
    // A read's answer, to its callback: first, since the turns of the loop that deliver it would
    // answer the others' reads too.
    let answer = vm(&h, 8);
    run(&answer, &format!("host.ocr.recognize({{ 9868, 5, 9898, 15 }}, function() {} end)", read("answer", 9869)));
    fire_until(&h, || h.tasks.handler_of(8).is_some());
    // A timer.
    let a = vm(&h, 1);
    run(&a, &format!("host.timer.after(0, function() {} end)", read("timer", 9861)));
    fire_timers(&h, Instant::now() + Duration::from_millis(5));
    modules.push((a, "timer 9861,5"));
    // A window trigger.
    let a = vm(&h, 2);
    run(&a, &format!("host.window.onTrigger({{ title = 'Synth' }}, function() {} end)", read("trigger", 9862)));
    let upto = crate::trigger_seq(&a);
    assert_eq!(mailbox::deliver(&*h, 2, &a, Event::Activate { win: Box::new(window("Synth")), upto }), Delivered::Parked);
    modules.push((a, "trigger 9862,5"));
    // A focus change.
    let a = vm(&h, 3);
    run(&a, &format!("host.window.onFocus(function() {} end)", read("focus", 9863)));
    assert_eq!(mailbox::deliver(&*h, 3, &a, Event::Focus), Delivered::Parked);
    modules.push((a, "focus 9863,5"));
    // A setting's onChange, from the dialog: in the queued phase.
    let a = vm(&h, 4);
    let cb = f(&a, read("setting", 9864));
    h.settings.borrow_mut().entry((4, "lang".to_string())).or_default().push((4, a.clone(), a.create_registry_value(cb).unwrap(), 41));
    let setting = Event::Setting { setting_of: 4, reg: 41, key: "lang".into(), new: crate::settings::Value::Str("de".into()), old: None };
    assert_eq!(mailbox::deliver_later(&*h, 4, &a, setting), Delivered::Queued);
    assert_eq!(mailbox::run_queued(&*h).parked, 1);
    modules.push((a, "setting 9864,5"));
    // A controller press.
    let a = vm(&h, 5);
    let token = h.pads.listen_for_test(5, &a, f(&a, read("pad", 9865)), crate::gamepad_api::Kind::Down);
    let press = Event::Pad {
        token,
        kind: PadKind::Button,
        event: Box::new(PadEvent {
            pad: 0,
            kind: PadEventKind::Down(Button::South),
            at: Instant::now(),
            synthetic: false,
            family: Family::Xbox,
            held: BTreeSet::new(),
            report: 0,
        }),
        delivery: Delivery { token, event: 0, value: None, partner: false },
    };
    assert_eq!(mailbox::deliver(&*h, 5, &a, press), Delivered::Parked);
    modules.push((a, "pad 9865,5"));
    // An image search's answer.
    let a = vm(&h, 6);
    let p = crate::image_search::test_pending(&a, &h.gens.borrow(), 61, 6, f(&a, read("image", 9866)));
    let ev = Event::Image { p: Box::new(p), outcome: crate::image_search::test_found_nothing(), ended: false };
    assert_eq!(mailbox::deliver(&*h, 6, &a, ev), Delivered::Parked);
    modules.push((a, "image 9866,5"));
    // A snapshot's answer.
    let a = vm(&h, 7);
    let owner = reads::owner_of(&a, &h.gens.borrow(), 7);
    let p = crate::snapshot::test_request(&h.snaps, &a, owner, None, f(&a, read("snapshot", 9867)));
    let ev = Event::Snapshot { p: Box::new(p), answer: crate::snapshot::test_failed("screen capture failed") };
    assert_eq!(mailbox::deliver(&*h, 7, &a, ev), Delivered::Parked);
    modules.push((a, "snapshot 9867,5"));
    modules.push((answer, "answer 9869,5"));

    for (i, (lua, _)) in modules.iter().enumerate() {
        assert!(h.tasks.handler_of(i + 1).is_some(), "module {} waits", i + 1);
        assert_eq!(heard(lua), "", "module {} went on before its reading came", i + 1);
    }
    fire_until(&h, || modules.iter().all(|(lua, _)| !heard(lua).is_empty()));
    for (lua, said) in &modules {
        assert_eq!(heard(lua), *said);
    }
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
    assert_eq!(h.tasks.len(), 0);
}

/// A timer's handler that waited for a read while keys queued behind it — a poll that holds its
/// module's keys — is said once per module and place in a session, the module's own line past a
/// `pcall` around the call; a key's handler that waits is not a poll, and a timer's that nothing
/// waited behind says nothing.
#[test]
fn a_poll_that_waited_with_keys_behind_it_is_said_once_per_place() {
    let h = host();
    let a = vm(&h, 1);
    let poll = "local ok, r = pcall(host.ocr.recognize, { 9871, 5, 9901, 15 })";
    for _ in 0..2 {
        assert_eq!(call(&h, &a, 1, poll, false), Delivered::Parked);
        say(&h, &a, 1, "Tab", true);
        say(&h, &a, 1, "Tab", true);
        fire_until(&h, || h.tasks.handler_of(1).is_none());
        mailbox::run_queued(&*h);
    }
    assert!(h.tasks.timer_said("m1", "mod.luau:1"), "the module's line, not pcall's [C]");
    assert_eq!(h.tasks.timer_lines(), 1, "once per module and place");
    // Nothing behind it, or a key's handler: no line.
    assert_eq!(call(&h, &a, 1, "\nhost.ocr.recognize({ 9872, 5, 9902, 15 })", false), Delivered::Parked);
    fire_until(&h, || h.tasks.handler_of(1).is_none());
    assert_eq!(call(&h, &a, 1, "\n\nhost.ocr.recognize({ 9873, 5, 9903, 15 })", true), Delivered::Parked);
    say(&h, &a, 1, "Tab", true);
    fire_until(&h, || h.tasks.handler_of(1).is_none());
    mailbox::run_queued(&*h);
    assert_eq!(h.tasks.timer_lines(), 1);
    assert_eq!(heard(&a), "Tab | Tab | Tab | Tab | Tab");
}

/// A module one handler has kept busy for 30 s with keys waiting behind it is said once for that
/// handler: what it waits in, for how long, where, and how many keys wait.
#[test]
fn a_module_busy_long_with_keys_behind_it_is_said_once() {
    let h = host();
    let a = vm(&h, 1);
    assert_eq!(call(&h, &a, 1, "host.ocr.recognize({ 9881, 5, 9911, 15 })", false), Delivered::Parked);
    assert!(mailbox::say_long_waits(&*h, Instant::now() + Duration::from_secs(31)).is_empty(), "no key waits yet");
    say(&h, &a, 1, "Tab", true);
    assert!(mailbox::say_long_waits(&*h, Instant::now()).is_empty(), "not 30 s yet");
    let said = mailbox::say_long_waits(&*h, Instant::now() + Duration::from_secs(31));
    assert_eq!(said, [mailbox::busy_line("m1", 31, "timer", "mod.luau:1", 1)]);
    assert_eq!(said[0], "[m1] has been busy for 31 s in a timer handler, waiting at mod.luau:1; 1 key(s) wait behind it");
    assert!(mailbox::say_long_waits(&*h, Instant::now() + Duration::from_secs(62)).is_empty(), "once per handler");
    fire_until(&h, || h.tasks.handler_of(1).is_none());
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "Tab");
}

/// What a module's mailbox held when it is taken away — a disable, a stop — is said by name for
/// its inputs: what a stop says it dropped of the keys that waited for the module.
#[test]
fn a_mailbox_taken_away_names_the_inputs_it_held() {
    let h = host();
    let a = vm(&h, 1);
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    say(&h, &a, 1, "a key", true);
    say(&h, &a, 1, "a timer", false);
    say(&h, &a, 1, "another key", true);
    assert_eq!(mailbox::drop_owner(&*h, 1, Why::Disabled), ["key", "key"]);
    task::drop_owner(&*h, 1);
    assert!(h.mail.is_empty());
}

// ── The host's own arms, against a busy module ───────────────────────────────────────────────

/// A controller press that waited in a busy module's mailbox runs its listener through the host's
/// own arm (`open_pad_in`), with the event's table, once the module is free; one whose listener the
/// module took away meanwhile runs nothing.
#[test]
fn a_busy_modules_controller_press_reaches_its_listener_or_nothing_once_it_went() {
    let h = host();
    let a = vm(&h, 1);
    let f: Function = a.load("return function(e) heard[#heard + 1] = e.type .. ' ' .. e.pad end").eval().unwrap();
    let kept = h.pads.listen_for_test(1, &a, f.clone(), crate::gamepad_api::Kind::Down);
    let gone = h.pads.listen_for_test(1, &a, f, crate::gamepad_api::Kind::Down);
    let press = |token: i64| Event::Pad {
        token,
        kind: PadKind::Button,
        event: Box::new(PadEvent {
            pad: 2,
            kind: PadEventKind::Down(Button::South),
            at: Instant::now(),
            synthetic: false,
            family: Family::Xbox,
            held: BTreeSet::new(),
            report: 0,
        }),
        delivery: Delivery { token, event: 0, value: None, partner: false },
    };
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, press(kept)), Delivered::Queued);
    assert_eq!(mailbox::deliver(&*h, 1, &a, press(gone)), Delivered::Queued);
    h.pads.forget_for_test(gone);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "down 2", "the kept listener's press, and nothing for the one that went");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// An image search's answer that waited in a busy module's mailbox runs its callback through the
/// host's own arm (`open_image_in`) once the module is free; one whose module was disabled while
/// it waited is held, to be searched again when the module is on again, and runs nothing now.
#[test]
fn a_busy_modules_image_answer_runs_once_it_is_free_and_is_held_when_it_was_disabled() {
    let h = host();
    let a = vm(&h, 1);
    let cb: Function = a.load("return function(hit) heard[#heard + 1] = 'searched ' .. tostring(hit) end").eval().unwrap();
    let answer = |p| Event::Image { p: Box::new(p), outcome: crate::image_search::test_found_nothing(), ended: false };
    let p = crate::image_search::test_pending(&a, &h.gens.borrow(), 41, 1, cb.clone());
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(p)), Delivered::Queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "searched nil");
    // Disabled while it waits: held, not run, not let go.
    let p = crate::image_search::test_pending(&a, &h.gens.borrow(), 42, 1, cb);
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(p)), Delivered::Queued);
    h.enabled.borrow_mut()[1] = false;
    mailbox::drop_owner(&*h, 1, Why::Disabled);
    task::drop_owner(&*h, 1);
    assert!(crate::image_search::test_held(&h.images, 42), "held for the module's next turn on");
    assert_eq!(heard(&a), "searched nil");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// A snapshot's answer that waited in a busy module's mailbox runs its callback through the host's
/// own arm (`open_snapshot_in`) once the module is free — superseded when a newer request with its
/// key was made before it ran, as by a key queued ahead of it.
#[test]
fn a_busy_modules_snapshot_answer_runs_once_it_is_free_and_is_judged_when_it_runs() {
    let h = host();
    let a = vm(&h, 1);
    let owner = reads::owner_of(&a, &h.gens.borrow(), 1);
    let cb: Function = a.load("return function(snap, why) heard[#heard + 1] = tostring(snap) .. ': ' .. why end").eval().unwrap();
    let answer = |p| Event::Snapshot { p: Box::new(p), answer: crate::snapshot::test_failed("screen capture failed") };
    let p = crate::snapshot::test_request(&h.snaps, &a, owner, Some("k"), cb.clone());
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(p)), Delivered::Queued);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "nil: screen capture failed");
    // A newer request with the key made while the answer waited: superseded when it runs.
    let older = crate::snapshot::test_request(&h.snaps, &a, owner, Some("k"), cb.clone());
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(older)), Delivered::Queued);
    let _newer = crate::snapshot::test_request(&h.snaps, &a, owner, Some("k"), cb);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), format!("nil: screen capture failed | nil: {}", crate::snapshot::SUPERSEDED));
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The answer of a `host.screen.profile` with a callback is a handler of its module: it waits in
/// a busy module's mailbox and runs once the module is free, through the host's own arm
/// (`open_snapshot_in`), with its table and the picture's moment — and `host.screen.pending` holds
/// its key out all the while. A newer profile with its key made meanwhile answers it `nil, reason`
/// when it runs; a disable drops it unrun, and the key is no longer out.
#[test]
fn a_busy_modules_profile_answer_waits_as_a_handler_and_its_key_is_out_until_it_runs() {
    let h = host();
    let a = vm(&h, 1);
    let owner = reads::owner_of(&a, &h.gens.borrow(), 1);
    let pending = |key: &str| {
        crate::snapshot::pending_in(&h.snaps, &h.gens.borrow(), &a, 1, &Value::String(a.create_string(key).unwrap())).unwrap()
    };
    let cb: Function = a
        .load(
            "return function(p, why) heard[#heard + 1] = p and (p.w .. 'x' .. p.h .. ' ' .. p.columns.min[1] .. ' at ' .. p.time .. '/' .. p.inputEpoch) or ('nil: ' .. why) end",
        )
        .eval()
        .unwrap();
    let rect = crate::ocr::types::Rect::new(5, 6, 3, 2);
    let answer = |p| Event::Snapshot { p: Box::new(p), answer: crate::snapshot::test_reduced(rect, 77) };
    let p = crate::snapshot::test_profile(&h.snaps, &a, owner, Some("area"), cb.clone(), rect, (1234, 9));
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(p)), Delivered::Queued, "a busy module's answer waits");
    assert!(pending("area") && !pending("other"), "its key is out while it waits in the mailbox");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "3x2 77 at 1234/9");
    assert!(!pending("area"), "and not once it ran");
    // A newer profile with the key, made while the answer waited: superseded when it runs.
    let older = crate::snapshot::test_profile(&h.snaps, &a, owner, Some("area"), cb.clone(), rect, (1, 1));
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(older)), Delivered::Queued);
    let _newer = crate::snapshot::test_profile(&h.snaps, &a, owner, Some("area"), cb.clone(), rect, (2, 2));
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), format!("3x2 77 at 1234/9 | nil: {}", crate::snapshot::SUPERSEDED));
    // Disabled while it waits: dropped, never run, and its key no longer out.
    let p = crate::snapshot::test_profile(&h.snaps, &a, owner, Some("late"), cb, rect, (3, 3));
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    assert_eq!(mailbox::deliver(&*h, 1, &a, answer(p)), Delivered::Queued);
    h.enabled.borrow_mut()[1] = false;
    mailbox::drop_owner(&*h, 1, Why::Disabled);
    task::drop_owner(&*h, 1);
    assert!(!pending("late"), "a disabled module's answers are dropped");
    assert_eq!(heard(&a), format!("3x2 77 at 1234/9 | nil: {}", crate::snapshot::SUPERSEDED));
    assert!(h.discarded.borrow().iter().any(|d| d.1 == "snapshot" && d.2 == Why::Disabled));
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

// ── Teardown while a module waits ────────────────────────────────────────────────────────────

/// A disable while the module waits: its handler never goes on, not even after a quick enable,
/// and its queued events are dropped — but a setting's `onChange`, which a disabled module still
/// hears, runs, and the module has the new value when it is enabled again. A reload and a rollback
/// drop everything, settings included, and nothing reaches the old VM.
#[test]
fn teardown_while_waiting_drops_what_waits_and_keeps_a_disabled_modules_settings() {
    let h = host();
    let a = vm(&h, 1);
    let cb: Function = a.load("return function(new) lang = new end").eval().unwrap();
    h.settings.borrow_mut().entry((1, "lang".to_string())).or_default().push((1, a.clone(), a.create_registry_value(cb).unwrap(), 77));
    let setting = |v: &str| Event::Setting {
        setting_of: 1,
        reg: 77,
        key: "lang".into(),
        new: crate::settings::Value::Str(v.into()),
        old: None,
    };
    assert_eq!(call(&h, &a, 1, "wait('w'); heard[#heard + 1] = 'resumed'", false), Delivered::Parked);
    say(&h, &a, 1, "queued key", true);
    assert_eq!(mailbox::deliver_later(&*h, 1, &a, setting("de")), Delivered::Queued);
    // The disable, as `apply_enabled(false)` makes it.
    h.enabled.borrow_mut()[1] = false;
    mailbox::drop_owner(&*h, 1, Why::Disabled);
    task::drop_owner(&*h, 1);
    assert_eq!(h.mail.len(1), 1, "only the setting is left");
    assert!(!task::test_release(&*h, "w", Value::Nil), "the handler is gone");
    // A setting changed while it is off still reaches it.
    assert_eq!(mailbox::deliver_later(&*h, 1, &a, setting("fr")), Delivered::Queued);
    assert_eq!(say(&h, &a, 1, "while off", true), Delivered::Dropped);
    mailbox::run_queued(&*h);
    assert!(yes(&a, "return lang == 'fr'"));
    // A quick enable resumes nothing.
    h.enabled.borrow_mut()[1] = true;
    assert_eq!(heard(&a), "");
    assert!(h.mail.is_empty() && h.tasks.handler_of(1).is_none());

    // A reload: everything goes, the setting too.
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    mailbox::deliver_later(&*h, 1, &a, setting("it"));
    say(&h, &a, 1, "old VM", true);
    mailbox::drop_owner(&*h, 1, Why::Reloaded);
    task::drop_owner(&*h, 1);
    let _new = vm(&h, 1);
    assert!(h.mail.is_empty());
    mailbox::run_queued(&*h);
    assert!(yes(&a, "return lang == 'fr'"), "the reloaded module's setting never reached the old VM");
    assert_eq!(heard(&a), "");
    // A rollback of everything from index 1 on.
    let c = vm(&h, 3);
    assert_eq!(call(&h, &c, 3, "wait('w')", false), Delivered::Parked);
    say(&h, &c, 3, "rolled back", true);
    mailbox::drop_from(&*h, 2);
    task::drop_from(&*h, 2);
    assert!(h.mail.is_empty() && h.tasks.handler_of(3).is_none());
    let why: Vec<Why> = h.discarded.borrow().iter().map(|d| d.2).collect();
    assert!(why.contains(&Why::Disabled) && why.contains(&Why::Reloaded) && why.contains(&Why::RolledBack), "{why:?}");
}

/// A delivery to a VM that is no longer the one at its index is dropped, never run.
#[test]
fn an_event_for_a_vm_that_went_is_dropped() {
    let h = host();
    let old = vm(&h, 1);
    let _new = vm(&h, 1);
    assert_eq!(say(&h, &old, 1, "late", true), Delivered::Dropped);
    assert_eq!(heard(&old), "");
}

// ── The three ways into a waiting module ─────────────────────────────────────────────────────

/// While A's handler waits, code of A runs only through the host's direct calls: an arbiter's
/// `onActivate` and `onDeactivate`, in their pairs, at once. And when that code resumes A's own
/// parked handler itself — way 3 — the wait raises there, the host lets the handler go, and A is
/// free for its mailbox.
#[test]
fn the_arbiter_reaches_a_waiting_module_and_its_own_resume_frees_it() {
    let h = host();
    let a = vm(&h, 1);
    assert_eq!(
        call(&h, &a, 1, "co = coroutine.running(); ok, err = pcall(wait, 'w'); err = tostring(err); heard[#heard + 1] = 'went on'", false),
        Delivered::Parked
    );
    say(&h, &a, 1, "queued", true);
    // onActivate and onDeactivate, as the host calls them: at once, in order.
    run(&a, "heard[#heard + 1] = 'onActivate'");
    run(&a, "heard[#heard + 1] = 'onDeactivate'");
    assert_eq!(heard(&a), "onActivate | onDeactivate");
    // Way 3: an onActivate that resumes the handler's coroutine itself.
    run(&a, "assert(coroutine.resume(co, 'not the host'))");
    assert!(yes(&a, "return ok == false and string.find(err, 'resumed by the module', 1, true) ~= nil"));
    assert_eq!(heard(&a), "onActivate | onDeactivate | went on");
    assert!(h.tasks.handler_of(1).is_none(), "the module is free");
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "onActivate | onDeactivate | went on | queued");
}

// ── Answers that wait in a mailbox (M1) ──────────────────────────────────────────────────────

/// A read's answer that waits in the mailbox behind a key is still out: the key's handler, which
/// runs first, sees `pending("k")` true and reads again with the key, and the old answer then
/// comes with `newer = true` — decided when it runs, not when it came.
#[test]
fn an_answer_in_the_mailbox_is_still_out_and_judged_when_it_runs() {
    let h = host();
    let a = vm(&h, 1);
    run(
        &a,
        r#"
        answers = {}
        host.ocr.recognize({ 9601, 5, 9631, 15 }, { key = "k" }, function(r)
          answers[#answers + 1] = { text = r.text, newer = r.newer, pending = host.ocr.pending("k") }
        end)
        "#,
    );
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    // The key comes before the answer.
    call(
        &h,
        &a,
        1,
        r#"
        keySaw = host.ocr.pending("k")
        host.ocr.recognize({ 9602, 5, 9632, 15 }, { key = "k" }, function(r)
          answers[#answers + 1] = { text = r.text, newer = r.newer, pending = host.ocr.pending("k") }
        end)
        "#,
        true,
    );
    fire_until(&h, || h.mail.len(1) == 2);
    assert_eq!(h.mail.len(1), 2, "the answer waits behind the key");
    assert!(yes(&a, "return host.ocr.pending('k') == true"), "an answer in the mailbox is still out");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert!(yes(&a, "return keySaw == true"), "the key saw the read still out");
    assert!(
        yes(&a, "return #answers == 1 and answers[1].text == '9601,5' and answers[1].newer == true and answers[1].pending == true"),
        "the old answer comes newer, the key's own read still out"
    );
    fire_until(&h, || yes(&a, "return #answers == 2"));
    assert!(yes(&a, "return answers[2].text == '9602,5' and answers[2].newer == false and answers[2].pending == false"));
    assert!(!h.state.has_pending());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// An answer that waited in the mailbox of a module disabled meanwhile is dropped, no longer out,
/// and its key forgotten.
#[test]
fn an_answer_in_the_mailbox_of_a_disabled_module_is_dropped() {
    let h = host();
    let a = vm(&h, 1);
    run(&a, r#"host.ocr.recognize({ 9611, 5, 9641, 15 }, { key = "k" }, function() called = true end)"#);
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    fire_until(&h, || h.mail.len(1) == 1);
    assert!(h.state.has_pending(), "waiting in the mailbox");
    h.enabled.borrow_mut()[1] = false;
    mailbox::drop_owner(&*h, 1, Why::Disabled);
    task::drop_owner(&*h, 1);
    assert!(!h.state.has_pending(), "no longer out");
    h.enabled.borrow_mut()[1] = true;
    mailbox::run_queued(&*h);
    assert!(yes(&a, "return called == nil and host.ocr.pending('k') == false"));
}

/// `pending(key)` in a handler that waited for a read: `true` while the answer of a read with that
/// key and a callback waits in the module's mailbox behind it — so a handler that waits until it
/// turns `false` waits for good (docs/api/ocr.md, "Where it waits").
#[test]
fn pending_is_true_in_a_waiting_handler_while_its_keys_answer_waits_behind_it() {
    let h = host();
    let a = vm(&h, 1);
    // Asked first, so answered first: into the mailbox, as the module waits by then.
    run(&a, r#"host.ocr.recognize({ 9621, 5, 9651, 15 }, { key = "k" }, function() called = host.ocr.pending("k") end)"#);
    let waits = r#"local r = host.ocr.recognize({ 9622, 5, 9652, 15 }); got, sawPending = r.text, host.ocr.pending("k")"#;
    assert_eq!(call(&h, &a, 1, waits, false), Delivered::Parked);
    fire_until(&h, || yes(&a, "return got ~= nil"));
    assert!(yes(&a, "return got == '9622,5' and sawPending == true"), "the answer behind it is still out");
    assert_eq!(h.mail.len(1), 1, "the callback's answer waited behind the handler");
    mailbox::run_queued(&*h);
    assert!(yes(&a, "return called == false and host.ocr.pending('k') == false"));
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

// ── Timers ───────────────────────────────────────────────────────────────────────────────────

/// While a module is busy, an `after` that comes due waits in its mailbox — collected once: the
/// epoch does not turn on every later tick for it — runs late, never early, and can be cancelled
/// until it runs. An `every` is skipped and comes round again.
#[test]
fn a_busy_modules_after_waits_once_and_its_every_is_skipped() {
    let h = host();
    let a = vm(&h, 1);
    run(&a, "polls = 0; poll = host.timer.every(5, function() polls = polls + 1 end)");
    run(&a, "late = host.timer.after(0, function() heard[#heard + 1] = 'late' end)");
    run(&a, "cancelled = host.timer.after(0, function() heard[#heard + 1] = 'cancelled' end)");
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    let t0 = Instant::now();
    let epoch = h.epoch.get();
    fire_timers(&h, t0 + Duration::from_millis(10));
    assert_eq!(h.epoch.get(), epoch + 1, "the one-shots came due");
    assert_eq!(h.mail.len(1), 2, "both one-shots wait; the poll was skipped");
    for step in 2..6 {
        fire_timers(&h, t0 + Duration::from_millis(10 * step));
    }
    assert_eq!(h.epoch.get(), epoch + 1, "a queued one-shot is not collected again");
    assert!(yes(&a, "return polls == 0"), "skipped while busy");
    assert!(yes(&a, "return host.timer.cancel(cancelled) == true"), "cancellable while it waits");
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "late");
    fire_timers(&h, t0 + Duration::from_millis(100));
    assert!(yes(&a, "return polls == 1"), "free again: the poll runs");
    assert!(yes(&a, "return host.timer.cancel(poll) == true"));
    assert!(h.timers.is_empty());
}

// ── Window events ────────────────────────────────────────────────────────────────────────────

/// A trigger that a handler registers while an activation waits in the mailbox — made after the
/// activation arrived — is neither reached nor un-primed by it: its own initial report is still to
/// come. The trigger that was there gets the activation.
#[test]
fn a_trigger_made_while_an_activation_waits_does_not_get_it() {
    let h = host();
    let a = vm(&h, 1);
    run(&a, r#"host.window.onTrigger({ title = "Synth" }, function() heard[#heard + 1] = 'old trigger' end)"#);
    assert_eq!(call(&h, &a, 1, "wait('w')", false), Delivered::Parked);
    let upto = crate::trigger_seq(&a);
    assert_eq!(upto, 1);
    assert_eq!(
        mailbox::deliver(&*h, 1, &a, Event::Activate { win: Box::new(window("Synth")), upto }),
        Delivered::Queued
    );
    // An earlier queued handler — here, the arbiter's onActivate — registers a new trigger.
    run(&a, r#"host.window.onTrigger({ title = "Synth" }, { initial = true }, function() heard[#heard + 1] = 'new trigger' end)"#);
    assert!(task::test_release(&*h, "w", Value::Nil));
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "old trigger");
    assert!(yes(&a, "return host.window._wantsInitial(false) == true"), "the new trigger's report is still to come");
}

/// The report of the window in front, with a trigger whose `where` waits before the first match:
/// the handler parks inside the prelude's dispatch, and the preparation it runs before the first
/// matching callback, after the wait, is a plain function still there — nothing scoped to a
/// dispatch that returned.
#[test]
fn an_initial_report_whose_where_waits_calls_nothing_destroyed() {
    let h = host();
    let a = vm(&h, 1);
    run(
        &a,
        r#"
        prepared = 0
        prewarm = function() prepared = prepared + 1 end
        host.window.onTrigger({ title = "Synth", where = function() wait("where"); return true end },
          { initial = true }, function(win) heard[#heard + 1] = 'reported ' .. win.title end)
        "#,
    );
    let ev = Event::Initial { win: Some(Box::new(window("Synth"))), reprime: false };
    assert_eq!(mailbox::deliver(&*h, 1, &a, ev), Delivered::Parked);
    assert!(yes(&a, "return prepared == 0"));
    assert!(task::test_release(&*h, "where", Value::Nil));
    assert!(yes(&a, "return prepared == 1"));
    assert_eq!(heard(&a), "reported Synth");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}

/// The `[dispatch]` line counts the dispatch's own time: what its handler spent parked is left
/// out. Without the host's `waited`, as before handlers, the same dispatch is said slow.
#[test]
fn the_dispatch_line_leaves_out_the_time_parked() {
    let h = host();
    for (idx, waited) in [(1, true), (2, false)] {
        let lua = vm(&h, idx);
        if waited {
            with_waited(&h, &lua, idx);
        }
        run(&lua, r#"host.window.onTrigger({ title = "Synth" }, function() wait("slow") end)"#);
        let upto = crate::trigger_seq(&lua);
        let ev = Event::Activate { win: Box::new(window("Synth")), upto };
        assert_eq!(mailbox::deliver(&*h, idx, &lua, ev), Delivered::Parked);
        std::thread::sleep(Duration::from_millis(80));
        assert!(task::test_release(&*h, "slow", Value::Nil));
        let logs: String = lua.load("return table.concat(logs, ' | ')").eval().unwrap();
        if waited {
            assert!(!logs.contains("[dispatch]"), "the time parked was counted: {logs}");
        } else {
            assert!(logs.contains("[dispatch] activate") && logs.contains("1 of 1 trigger(s) fired"), "{logs}");
        }
    }
}

// ── Settings ─────────────────────────────────────────────────────────────────────────────────

/// A setting's `onChange` handed to `deliver_later` waits for the queued phase even when its
/// module is free: never run inside the code that changed it.
#[test]
fn a_setting_runs_in_the_queued_phase_even_for_a_free_module() {
    let h = host();
    let a = vm(&h, 1);
    let cb: Function = a.load("return function(new, old) heard[#heard + 1] = tostring(new) .. ' from ' .. tostring(old) end").eval().unwrap();
    h.settings.borrow_mut().entry((1, "speed".to_string())).or_default().push((1, a.clone(), a.create_registry_value(cb).unwrap(), 5));
    let ev = Event::Setting {
        setting_of: 1,
        reg: 5,
        key: "speed".into(),
        new: crate::settings::Value::Int(3),
        old: Some(crate::settings::Value::Int(2)),
    };
    assert_eq!(mailbox::deliver_later(&*h, 1, &a, ev), Delivered::Queued);
    assert_eq!(heard(&a), "");
    assert!(mailbox::busy(&*h, 1), "until the phase runs it, the module is busy");
    assert_eq!(say(&h, &a, 1, "a key after it", true), Delivered::Queued, "and runs after it");
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "3 from 2 | a key after it");
}

/// An event whose handler runs its VM out of memory, and does not catch it, stops the VM: the
/// handler's stretch is the guard's entry, its error is not reported as the module's, the trip goes
/// to the exit hook as the stretch ends, and the VM's next event is not run — nor reported.
#[test]
fn a_handler_that_runs_its_vm_out_of_memory_stops_it_and_is_not_reported() {
    let h = host();
    let guard = crate::vm_guard::Guard::new();
    let lua = crate::vm_guard::new_vm(&guard);
    crate::vm_guard::describe_for_test(&lua, 1, "m1", &[("m1", 8)]);
    crate::vm_guard::loaded(&lua);
    let a = vm_in(&h, 1, lua);
    let got: Rc<RefCell<Vec<crate::vm_guard::Trip>>> = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    crate::vm_guard::set_exit_hook(Rc::new(move |t| g.borrow_mut().extend(t)));
    let fill = "local t = {} while true do t[#t + 1] = string.rep('x', 1048000) .. #t end";
    assert_eq!(call(&h, &a, 1, fill, true), Delivered::Ran);
    assert!(h.errors.borrow().is_empty(), "not the module's error: {:?}", h.errors.borrow());
    assert_eq!(got.borrow().len(), 1, "handed over when the handler's stretch ended");
    assert_eq!((got.borrow()[0].what.as_str(), got.borrow()[0].kind), ("key", crate::vm_guard::EntryKind::Handler));
    assert!(crate::vm_guard::stopped(&a));
    a.gc_collect().unwrap();
    // Compiled before it is delivered: a stopped VM raises at the first safepoint of anything run
    // in it, the host's own reading of `heard` too — so that is read from Rust. Its VM is stopped,
    // so the event waits rather than be refused (this holder keeps the module enabled; the host
    // turns it off, and drops it), and the queued phase passes it by.
    assert_eq!(say(&h, &a, 1, "after", true), Delivered::Queued);
    assert_eq!(mailbox::run_queued(&*h), Phase::default());
    let heard_n = a.globals().raw_get::<mlua::Table>("heard").unwrap().raw_len();
    assert_eq!(heard_n, 0, "the stopped VM ran nothing");
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
    assert_eq!(got.borrow().len(), 1, "one stop");
    crate::vm_guard::clear_exit_hook();
}

/// A module the host's guard stopped is off, and busy until it is turned on again: a setting's
/// `onChange` — which a module unticked in the manager hears at once — waits in its mailbox rather
/// than be refused by the stopped VM, and the queued phase passes it by. Its other events are
/// dropped, as for any module that is off. (The manager's checkbox rebuilds a stopped module, which
/// drops what waited and reads its settings afresh; the mailbox runs them when nothing rebuilt it.)
#[test]
fn a_stopped_modules_settings_wait_until_it_is_turned_on_again() {
    let h = host();
    let a = vm(&h, 1);
    let cb: Function = a.load("return function(new, old) heard[#heard + 1] = tostring(new) .. ' from ' .. tostring(old) end").eval().unwrap();
    h.settings.borrow_mut().entry((1, "speed".to_string())).or_default().push((1, a.clone(), a.create_registry_value(cb).unwrap(), 5));
    let setting = |v: i64| Event::Setting {
        setting_of: 1,
        reg: 5,
        key: "speed".into(),
        new: crate::settings::Value::Int(v),
        old: Some(crate::settings::Value::Int(v - 1)),
    };
    // The stop: off, and stopped.
    h.enabled.borrow_mut()[1] = false;
    h.stopped.borrow_mut().insert(1);
    assert!(mailbox::busy(&*h, 1));
    assert_eq!(mailbox::deliver(&*h, 1, &a, setting(3)), Delivered::Queued, "even where a free module's runs at once");
    assert_eq!(mailbox::deliver_later(&*h, 1, &a, setting(4)), Delivered::Queued);
    assert_eq!(say(&h, &a, 1, "a key", true), Delivered::Dropped);
    assert_eq!(mailbox::run_queued(&*h), Phase::default(), "the phase passes it by");
    assert_eq!(heard(&a), "");
    assert_eq!(h.mail.len(1), 2);
    // Turned on again: the settings run, in order, in the next queued phase.
    h.stopped.borrow_mut().remove(&1);
    h.enabled.borrow_mut()[1] = true;
    mailbox::run_queued(&*h);
    assert_eq!(heard(&a), "3 from 2 | 4 from 3");
    assert!(!mailbox::busy(&*h, 1));
}

/// A handler that yields in a loop — `while true do pcall(coroutine.yield) end` — is one callback to
/// the guard: the host answers its own yields at once, inside the delivery, so its clocks do not
/// start afresh at each answer, and it is stopped at its limit like any other loop. Each `tick()`
/// is 100 ms of both clocks and a look of the watchdog, until the round is armed; a guard that never
/// arms fails the test after 1000 instead of hanging it.
#[test]
fn a_handler_that_yields_in_a_loop_is_stopped() {
    struct Fake(std::sync::Arc<AtomicU64>);
    impl crate::vm_guard::Clocks for Fake {
        fn wall_ns(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
        fn loop_cpu(&self) -> Option<Duration> {
            Some(Duration::from_nanos(self.0.load(Ordering::SeqCst)))
        }
    }
    let h = host();
    let now = std::sync::Arc::new(AtomicU64::new(0));
    let guard = crate::vm_guard::Guard::with(crate::vm_guard::Budgets::APP, Box::new(Fake(now.clone())));
    let lua = crate::vm_guard::new_vm(&guard);
    crate::vm_guard::describe_for_test(&lua, 1, "m1", &[("m1", 256)]);
    crate::vm_guard::loaded(&lua);
    let a = vm_in(&h, 1, lua);
    let got: Rc<RefCell<Vec<crate::vm_guard::Trip>>> = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    crate::vm_guard::set_exit_hook(Rc::new(move |t| g.borrow_mut().extend(t)));
    let (ticks, armed) = (Rc::new(Cell::new(0u32)), Rc::new(Cell::new(false)));
    let (t, ar, gd) = (ticks.clone(), armed.clone(), guard.clone());
    let tick = a
        .create_function(move |_, ()| {
            if ar.get() {
                return Ok(());
            }
            t.set(t.get() + 1);
            if t.get() > 1000 {
                return Err(mlua::Error::runtime("never armed"));
            }
            now.fetch_add(100_000_000, Ordering::SeqCst);
            ar.set(gd.poll().armed.is_some());
            Ok(())
        })
        .unwrap();
    a.globals().set("tick", tick).unwrap();
    assert_eq!(call(&h, &a, 1, "while true do pcall(coroutine.yield) tick() end", true), Delivered::Ran);
    assert!(crate::vm_guard::stopped(&a), "the loop of yields was not stopped: {:?}", h.errors.borrow());
    assert!((20..=22).contains(&ticks.get()), "stopped after 2 s, not {} ticks", ticks.get());
    assert_eq!(got.borrow().len(), 1);
    assert!(h.errors.borrow().is_empty(), "the stop is not the module's error: {:?}", h.errors.borrow());
    assert!(h.tasks.handler_of(1).is_none());
    crate::vm_guard::clear_exit_hook();
}

/// A guard VM for module `idx` with `mib` MiB, over clocks a test steps: the watchdog's clocks
/// read `now` (nanoseconds).
fn guarded(h: &Rc<Host>, idx: usize, mib: u32, now: &std::sync::Arc<AtomicU64>) -> (Lua, std::sync::Arc<crate::vm_guard::Guard>) {
    struct Fake(std::sync::Arc<AtomicU64>);
    impl crate::vm_guard::Clocks for Fake {
        fn wall_ns(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
        fn loop_cpu(&self) -> Option<Duration> {
            Some(Duration::from_nanos(self.0.load(Ordering::SeqCst)))
        }
    }
    let guard = crate::vm_guard::Guard::with(crate::vm_guard::Budgets::APP, Box::new(Fake(now.clone())));
    let lua = crate::vm_guard::new_vm(&guard);
    let id = format!("m{idx}");
    crate::vm_guard::describe_for_test(&lua, idx, &id, &[(id.as_str(), mib)]);
    crate::vm_guard::loaded(&lua);
    (vm_in(h, idx, lua), guard)
}

/// A handler that waits for three readings, each 2.5 s of both clocks away, is not stopped: the
/// waiting counts on no clock, and each stretch after a wait is an entry of its own.
#[test]
fn a_handler_that_waits_three_times_is_not_stopped() {
    let h = host();
    let now = std::sync::Arc::new(AtomicU64::new(0));
    let (a, guard) = guarded(&h, 1, 256, &now);
    let got: Rc<RefCell<Vec<crate::vm_guard::Trip>>> = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    crate::vm_guard::set_exit_hook(Rc::new(move |t| g.borrow_mut().extend(t)));
    let code = "for i = 1, 3 do heard[#heard + 1] = host.ocr.recognize({ 9890 + i, 5, 9920 + i, 15 }).text end";
    assert_eq!(call(&h, &a, 1, code, false), Delivered::Parked);
    for n in 1..=3 {
        now.fetch_add(2_500_000_000, Ordering::SeqCst);
        assert!(guard.poll().armed.is_none(), "armed while the handler waited ({n})");
        let before = heard(&a);
        fire_until(&h, || heard(&a) != before);
    }
    assert!(!crate::vm_guard::stopped(&a), "a handler that waited was stopped");
    assert_eq!(heard(&a), "9891,5 | 9892,5 | 9893,5");
    assert!(got.borrow().is_empty() && h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
    crate::vm_guard::clear_exit_hook();
}

/// A reading that could not be built in its module's VM — for a handler that waited, or for a
/// read's callback — because the VM ran out of memory doing it is a stop, which reports itself: no
/// module error beside it. Any other failure is the module's error. (The delivery and the callback's
/// opening both go through this, `ocr::lua::handover_failed`; lib.rs's wiring test holds them to it.)
#[test]
fn a_memory_error_building_a_reading_is_a_stop_not_a_modules_error() {
    let h = host();
    let now = std::sync::Arc::new(AtomicU64::new(0));
    let (a, _guard) = guarded(&h, 1, 8, &now);
    let got: Rc<RefCell<Vec<crate::vm_guard::Trip>>> = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    crate::vm_guard::set_exit_hook(Rc::new(move |t| g.borrow_mut().extend(t)));
    reads::handover_failed(&*h, &a, 1, &mlua::Error::runtime("a table could not be made"));
    assert!(!crate::vm_guard::stopped(&a) && got.borrow().is_empty());
    assert_eq!(h.errors.borrow().len(), 1, "any other failure is the module's error");
    reads::handover_failed(&*h, &a, 1, &mlua::Error::MemoryError("not enough memory".into()));
    assert!(crate::vm_guard::stopped(&a));
    let trips = got.borrow();
    assert_eq!(trips.len(), 1);
    assert_eq!((trips[0].what.as_str(), trips[0].kind), ("ocr.recognize", crate::vm_guard::EntryKind::Handler));
    assert_eq!(h.errors.borrow().len(), 1, "the stop reports itself: no module error beside it");
    crate::vm_guard::clear_exit_hook();
}

// ── The phase's budget ───────────────────────────────────────────────────────────────────────

/// 300 events that take about 0.2 ms each, for two modules, spread over several ticks' phases
/// within the budget: each module's in order, and each phase after the first begins with the
/// module after the one the last ended at.
#[test]
fn a_burst_spreads_over_several_ticks_in_order_and_takes_turns() {
    let h = host();
    let spin = |lua: &Lua| {
        lua.globals()
            .set(
                "spin",
                lua.create_function(|_, ()| {
                    let until = Instant::now() + Duration::from_micros(200);
                    while Instant::now() < until {}
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
    };
    let a = vm(&h, 1);
    let b = vm(&h, 2);
    spin(&a);
    spin(&b);
    run(&a, "order = {}");
    run(&b, "order = {}");
    assert_eq!(call(&h, &a, 1, "wait('a')", false), Delivered::Parked);
    assert_eq!(call(&h, &b, 2, "wait('b')", false), Delivered::Parked);
    for i in 1..=150 {
        call(&h, &a, 1, &format!("spin(); order[#order + 1] = {i}"), false);
        call(&h, &b, 2, &format!("spin(); order[#order + 1] = {i}"), false);
    }
    assert!(task::test_release(&*h, "a", Value::Nil) && task::test_release(&*h, "b", Value::Nil));
    let mut phases = Vec::new();
    while !h.mail.is_empty() && phases.len() < 100 {
        let before = (h.mail.len(1), h.mail.len(2));
        let p = mailbox::run_queued(&*h);
        phases.push((before, (h.mail.len(1), h.mail.len(2)), p.budget));
    }
    assert!(phases.len() >= 2, "one phase ran 60 ms of events: {phases:?}");
    assert!(phases[0].2, "the budget ended the first phase");
    // The first phase began with A; the second with B, the module after the one it ended at.
    let ((a0, b0), (a1, b1), _) = phases[0];
    assert!(a1 < a0 && b1 == b0, "{phases:?}");
    let ((a0, b0), (a1, b1), _) = phases[1];
    assert!(b1 < b0, "the second phase began with B: {phases:?}");
    let _ = (a0, a1);
    for lua in [&a, &b] {
        assert!(yes(lua, "if #order ~= 150 then return false end for i = 1, 150 do if order[i] ~= i then return false end end return true"));
    }
}

/// The `[pump]` line for a phase the budget ended: once every ten seconds at most, with how many
/// such phases went by unsaid since the last — a burst that keeps the phases full for a while is
/// one line, not one a tick.
#[test]
fn the_budget_line_is_said_once_every_ten_seconds_with_a_count() {
    let m = Mailboxes::default();
    let t0 = Instant::now();
    assert_eq!(m.say_budget(t0), Some(0));
    assert_eq!(m.say_budget(t0 + Duration::from_millis(15)), None);
    assert_eq!(m.say_budget(t0 + Duration::from_secs(9)), None);
    assert_eq!(m.say_budget(t0 + Duration::from_secs(10)), Some(2));
    assert_eq!(m.say_budget(t0 + Duration::from_secs(21)), Some(0));
    assert_eq!(
        mailbox::budget_line(12, 2),
        "the queued module events used up their 15 ms this tick: 12 ran, and the rest run on the next tick, in \
         order; it ended 2 more such phase(s) since the last of these lines. Said at most once every 10 s"
    );
    assert!(!mailbox::budget_line(3, 0).contains("more such"));
}

// ── The cost ─────────────────────────────────────────────────────────────────────────────────

/// What an event costs as a handler, against the plain call it was: 20 000 of each, timed and
/// printed — a measurement, not a limit. Run in release (`cargo test --release`) for the number
/// module-runtime-and-lifecycle.md quotes. In a VM the host's guard made, as every module's is:
/// each stretch is an entry of the guard.
#[test]
fn one_event_as_a_handler_costs_a_few_microseconds() {
    let h = host();
    let guard = crate::vm_guard::Guard::new();
    let lua = crate::vm_guard::new_vm(&guard);
    crate::vm_guard::describe_for_test(&lua, 1, "m1", &[("m1", 256)]);
    crate::vm_guard::loaded(&lua);
    let a = vm_in(&h, 1, lua);
    let (handler, plain) = mailbox::cost_per_event(&*h, 1, &a, 20_000);
    println!(
        "HANDLER COST: one event as a handler {handler:.2} µs, as a plain call {plain:.2} µs ({} build)",
        if cfg!(debug_assertions) { "debug" } else { "release" }
    );
    assert!(h.mail.is_empty() && h.tasks.handler_of(1).is_none());
    assert!(h.errors.borrow().is_empty(), "{:?}", h.errors.borrow());
}
