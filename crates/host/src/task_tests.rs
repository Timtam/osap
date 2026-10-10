//! Tasks and their wait, against a real Luau VM and the real read service over a fake recogniser:
//! every rule of task.rs.
//!
//! No module can start a task — the host table has no entry for it — so these tests reach the
//! machinery through its test-only entry, `task::table`, which each VM here has as the global
//! `task`: `task.run(fn)`, `task.cancel(n)`, `task.alive(n)`.
//!
//! The holder below stands where the host's `Shared` stands — the service, the reads waiting on
//! the event loop, the tasks, the timers, which VM each module runs and whether it is enabled —
//! and each test builds module VMs over it with the host's own bindings: `host.ocr.recognize`, both
//! forms, and `pending` (ocr/lua.rs and the shim), and `host.timer` (timers.rs). The tick is
//! `fire`: due timers, then the readings, as the loop runs them.
//!
//! The fake recogniser reads every region as "x,y" — its corner — and can be held: a capture of
//! a region whose x a test `hold`s waits until it lets go, a snapshot round's as well, and so does
//! a recognition of one whose x it `hold_recognition`s; so no test times a picture, it holds it. Every test reads at x
//! coordinates of its own, since these lists are shared by the tests running beside it.
//!
//! The test's own thread stands where the event loop stands, and is marked as the loop's for as
//! long as the holder lives (`loop_guard`); every function of the fake recogniser asks the guard
//! first, as the platforms' do. So in a debug build a test that brought a capture or a
//! recognition onto the loop fails.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::{CaptureSource, Compare, OcrLine, OcrText, OcrWord, OcrWorker, Recognise};
use crate::capture_source;
use crate::image_search::VmOwner;
use crate::mailbox::{self, Event, MailHost, Mailboxes, Opened, Why};
use crate::ocr::lang::Languages;
use crate::ocr::lua::{self as reads, Delivered, OcrState, ReadHost};
use crate::ocr::sched::Owner;
use crate::ocr::service::{Service, ShutdownHandle};
use crate::ocr::snap_queue::{SnapKind, SnapReq, SnapTicket};
use crate::ocr::types::{current_priority, enter_priority, Priority, Rect};
use crate::task::{self, Case, Tasks};
use crate::timers::{self, Timers};

// ── The fake recogniser ──────────────────────────────────────────────────────────────────────

/// The fake's pixels: the regions it was asked to photograph. Also the key-scope tests' (key_scope_tests.rs),
/// whose busy module waits for a read of it.
pub(crate) type Fake = Vec<(i32, i32, i32, i32)>;

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Held captures and recognitions, by x, and the condition their release is told on.
static HELD: Mutex<(Vec<i32>, Vec<i32>)> = Mutex::new((Vec::new(), Vec::new()));
static HELD_CV: Condvar = Condvar::new();
/// Every x the fake photographed, and every x it has begun to recognise.
static TAKEN: Mutex<Vec<i32>> = Mutex::new(Vec::new());
static RECOGNISING: Mutex<Vec<i32>> = Mutex::new(Vec::new());

fn hold(x: i32) {
    locked(&HELD).0.push(x);
}

fn hold_recognition(x: i32) {
    locked(&HELD).1.push(x);
}

fn release(x: i32) {
    let mut h = locked(&HELD);
    h.0.retain(|&v| v != x);
    h.1.retain(|&v| v != x);
    HELD_CV.notify_all();
}

/// Waits while `x` is held — at the capture (`capture` true) or the recognition — 10 s at most.
fn wait_while_held(x: i32, capture: bool) {
    let until = Instant::now() + Duration::from_secs(10);
    let mut h = locked(&HELD);
    while (if capture { h.0.contains(&x) } else { h.1.contains(&x) }) && Instant::now() < until {
        h = HELD_CV.wait_timeout(h, until.saturating_duration_since(Instant::now())).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
    }
}

fn taken(x: i32) -> bool {
    locked(&TAKEN).contains(&x)
}

/// Waits until a recognition of `x` has begun, 10 s at most.
fn wait_recognising(x: i32) {
    let until = Instant::now() + Duration::from_secs(10);
    while !locked(&RECOGNISING).contains(&x) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn fake_capture(regions: &[(i32, i32, i32, i32)], _: CaptureSource) -> (Fake, usize) {
    crate::loop_guard::off_loop("the fake capture");
    for r in regions {
        wait_while_held(r.0, true);
    }
    locked(&TAKEN).extend(regions.iter().map(|r| r.0));
    (regions.to_vec(), 16)
}

fn line_of(text: String) -> OcrText {
    let word = OcrWord { text: text.clone(), x: 0, y: 0, w: 10, h: 10 };
    OcrText { text: text.clone(), words: vec![word.clone()], lines: vec![OcrLine { text, words: vec![word] }], fallback: None, skipped: false }
}

/// "x,y" for every region; "preempt=…" at y = 79, which says whether the read was a background
/// one; a failure at x = 666.
fn fake_recognise(shot: &Fake, _: &[(i32, i32, i32, i32)], ctx: &Recognise) -> Vec<Result<OcrText, String>> {
    crate::loop_guard::off_loop("the fake recognition");
    ctx.each(shot.iter(), |&(x, y, _, _)| {
        locked(&RECOGNISING).push(x);
        wait_while_held(x, false);
        if x == 666 {
            return Err("the fake recogniser failed".to_string());
        }
        if y == 79 {
            return Ok(line_of(format!("preempt={}", ctx.preempt.is_some())));
        }
        Ok(line_of(format!("{x},{y}")))
    })
}

/// Whether the language list of the holder that holds it is still held back (`held_langs`).
static LANGS_HELD: Mutex<bool> = Mutex::new(true);
static LANGS_CV: Condvar = Condvar::new();

/// The fake's language list, published only once the test lets it go (`release_langs`), 10 s at
/// most: a recognise thread that has not published its list yet, without a clock. One test
/// holds it, so the flag is its own.
fn held_langs() -> Languages {
    let until = Instant::now() + Duration::from_secs(10);
    let mut held = locked(&LANGS_HELD);
    while *held && Instant::now() < until {
        held = LANGS_CV.wait_timeout(held, until.saturating_duration_since(Instant::now())).map(|(g, _)| g).unwrap_or_else(|e| e.into_inner().0);
    }
    (worker().languages)()
}

fn release_langs() {
    *locked(&LANGS_HELD) = false;
    LANGS_CV.notify_all();
}

/// Every first-read comparison the fake made: the module, and the thread it was made on.
static COMPARED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Notes the comparison; made, unless the module's name says it cannot be yet.
fn fake_compare(who: &str, _: (i32, i32, i32, i32)) -> bool {
    locked(&COMPARED).push((who.to_string(), std::thread::current().name().unwrap_or("?").to_string()));
    !who.contains("not yet")
}

/// The threads `who`'s comparisons were made on, in order.
fn compared(who: &str) -> Vec<String> {
    locked(&COMPARED).iter().filter(|(w, _)| w == who).map(|(_, t)| t.clone()).collect()
}

pub(crate) fn worker() -> OcrWorker<Fake> {
    OcrWorker {
        present: true,
        init_thread: |_| {},
        warm_up: |_| {},
        capture: fake_capture,
        recognise: fake_recognise,
        languages: || Languages { available: vec!["en-US".into()], fast: vec![], preferred: vec!["en-US".into()] },
        frames: |regions, _, _| {
            crate::loop_guard::off_loop("the fake snapshot round");
            for r in regions {
                wait_while_held(r.0, true);
            }
            regions.iter().map(|_| Err("no snapshots here".to_string())).collect()
        },
        compare: fake_compare,
        display_of: |_, _| 0,
        shot_of: |_, regions| {
            crate::loop_guard::off_loop("the fake snapshot's pixels");
            regions.to_vec()
        },
    }
}

// ── The holder, the VMs, the tick ────────────────────────────────────────────────────────────

/// What the host keeps for the read path and the tasks, without the speech engines.
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
    /// This thread is the loop's while the holder lives.
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
}

/// The events this holder's modules hear: the answers to their reads and their timers.
impl MailHost for Host {
    fn mail(&self) -> &Mailboxes {
        &self.mail
    }
    fn open(&self, idx: usize, _: &Lua, ev: Event) -> Opened {
        let timer = |f: Option<Function>| match f {
            Some(f) => Opened::Run { f, args: MultiValue::new(), ctx: task::Ctx::new("timer", idx) },
            None => Opened::Gone,
        };
        match ev {
            Event::Read(r) => reads::open_read(self, *r),
            Event::After { token } => timer(self.timers.take_queued(token)),
            Event::Every { token } => timer(self.timers.every_fn(token)),
            Event::Call { f, args, what, .. } => mailbox::open_call(idx, f, args, what),
            _ => Opened::Gone,
        }
    }
    fn discard(&self, _: usize, ev: Event, _: Why) {
        match ev {
            Event::Read(r) => reads::discard_read(self, *r),
            Event::After { token } => self.timers.drop_queued(token),
            _ => {}
        }
    }
}

fn host() -> Rc<Host> {
    holder(Service::spawn(worker()))
}

/// A holder whose read service counts a hang after `hang` instead of five seconds.
fn host_hanging_after(hang: Duration) -> Rc<Host> {
    holder(Service::spawn_hanging_after(worker(), hang))
}

fn holder((ocr, stop): (Service<Fake>, ShutdownHandle)) -> Rc<Host> {
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
        _loop: crate::loop_guard::mark_for_a_test(),
    })
}

/// VM generations, process-wide as the host's are.
static NEXT_GEN: AtomicU64 = AtomicU64::new(1000);

/// A fresh VM for module `idx` — a new generation of it, enabled — with the host's own
/// `host.ocr.recognize`, `pending` and timer bindings over `h`, and for the tests: the machinery's test-only entry as the global
/// `task`, `hostCall(f, …)`, a plain `Function::call` as the host makes when it calls Lua back (an
/// arbiter's onActivate, an onChange), and `boom()`, a host function that panics.
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
    lua.globals().set("task", task::table(&lua, idx, h.clone()).unwrap()).unwrap();
    host.set("timer", timers::table(&lua, idx, h.clone(), |h| &h.timers, |_| Instant::now()).unwrap()).unwrap();
    let ocr = lua.create_table().unwrap();
    let hh = h.clone();
    ocr.set("pending", lua.create_function(move |lua, key: Value| reads::pending(&*hh, lua, idx, key)).unwrap()).unwrap();
    let hh = h.clone();
    ocr.set("languages", lua.create_function(move |lua, ()| reads::languages_value(&*hh, lua)).unwrap()).unwrap();
    let hh = h.clone();
    ocr.set("resolveLanguage", lua.create_function(move |_, v: Value| reads::resolve_value(&*hh, &v)).unwrap()).unwrap();
    let hh = h.clone();
    let submit = lua
        .create_function(move |lua, (what, opts, cb): (Value, Value, Value)| reads::read(&*hh, lua, idx, what, opts, cb))
        .unwrap();
    let waits = task::waits(&lua, idx, h.clone(), submit).unwrap();
    ocr.set("recognize", waits.get::<Function>("recognize").unwrap()).unwrap();
    host.set("ocr", ocr).unwrap();
    lua.globals().set("host", host).unwrap();
    lua.globals()
        .set("hostCall", lua.create_function(|_, (f, args): (Function, MultiValue)| f.call::<MultiValue>(args)).unwrap())
        .unwrap();
    lua.globals()
        .set(
            "boom",
            lua.create_function(|_, ()| -> mlua::Result<()> { panic!("{}a host function fell over", crate::EXPECTED_PANIC) })
                .unwrap(),
        )
        .unwrap();
    lua
}

/// Runs module code, as `mod.luau` — so a raise names its line as `mod.luau:<line>:`.
fn run(lua: &Lua, code: &str) {
    if let Err(e) = lua.load(code).set_name("=mod.luau").exec() {
        panic!("{e}");
    }
}

/// Evaluates `code` and insists on a real `true`: mlua reads `nil` as `false`, so a test that
/// read a global as a bool could not tell "answered false" from "never ran".
fn yes(lua: &Lua, code: &str) -> bool {
    matches!(lua.load(code).set_name("=check").eval::<Value>(), Ok(Value::Boolean(true)))
}

/// One turn of the loop's own work: the timers due by now, then the readings.
fn fire(h: &Host) -> Delivered {
    fire_timers(h, Instant::now());
    reads::fire(h)
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

/// Turns of the loop until no read is waiting any more, 10 s at most; what they delivered.
fn settle(h: &Host) -> Delivered {
    let until = Instant::now() + Duration::from_secs(10);
    let mut all = Delivered::default();
    loop {
        let d = reads::fire(h);
        all.callbacks += d.callbacks;
        all.tasks += d.tasks;
        if !h.state.has_pending() || Instant::now() >= until {
            return all;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// What a disable does to a module's reads and tasks (`apply_enabled`, after the arbiter's
/// re-election), and what the enable after it does.
fn disable(h: &Host, idx: usize) {
    h.enabled.borrow_mut()[idx] = false;
    reads::drop_owner(h, idx, false);
    task::drop_owner(h, idx);
}

fn enable(h: &Host, idx: usize) {
    h.enabled.borrow_mut()[idx] = true;
}

fn errors(h: &Host) -> Vec<(usize, String, String)> {
    h.errors.borrow().clone()
}

// ── run, alive, the first stretch ────────────────────────────────────────────────────────────

/// The first stretch runs inside `run`; a task that never waits ends there, and its number is
/// no longer alive. Numbers are positive and new each time. `run` raises only for a mistake in
/// the call; what the function raises is reported, under the module, as a task's error.
#[test]
fn the_first_stretch_runs_in_run_and_a_task_that_never_waits_ends_there() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        order = {}
        a = task.run(function() order[#order + 1] = "body" end)
        order[#order + 1] = "after"
        b = task.run(function() error("the body raised") end)
        "#,
    );
    assert!(yes(&lua, "return order[1] == 'body' and order[2] == 'after' and a > 0 and b > a"));
    assert!(yes(&lua, "return task.alive(a) == false and task.alive(b) == false"));
    let e = errors(&h);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].0 == 1 && e[0].1 == "task" && e[0].2.contains("the body raised"), "{e:?}");
    for (call, want) in [
        ("task.run(5)", "task.run: takes a function, got number"),
        ("task.run()", "task.run: takes a function, got nothing"),
        ("task.run(function() end, 1)", "task.run takes one argument, the function; hand it values as upvalues"),
        ("task.run(function() end, nil)", "task.run takes one argument"),
    ] {
        let err = lua.load(call).exec().expect_err(call).to_string();
        assert!(err.contains(want), "{call}: {err}");
    }
    assert_eq!(h.tasks.len(), 0);
}

/// A task waits at `recognize` while the loop goes on — a timer fires meanwhile — and goes on in
/// the delivery with the reading a callback would be handed, `time` and `inputEpoch` included; a
/// list answers the list in the order of its regions, and the table by name.
#[test]
fn a_task_waits_at_recognize_while_the_loop_goes_on() {
    let h = host();
    let lua = vm(&h, 1);
    h.ocr.note_input_epoch(5);
    hold(9001);
    run(
        &lua,
        r#"
        order = {}
        task.run(function()
          order[#order + 1] = "asked"
          local r = host.ocr.recognize({ region = { 9001, 5, 9031, 15 } })
          order[#order + 1] = "answered " .. r.status .. " " .. r.text
          seen = { time = r.time, epoch = r.inputEpoch, skipped = r.skipped, newer = r.newer, lang = r.lang }
          local m, byName = host.ocr.recognize({ { 9002, 5, 9032, 15 }, { name = "b", region = { 9003, 6, 9033, 16 } } })
          order[#order + 1] = "many " .. #m .. " " .. m[1].text .. " " .. byName.b.text
        end)
        host.timer.after(0, function() order[#order + 1] = "timer" end)
        order[#order + 1] = "returned"
        "#,
    );
    fire(&h);
    assert!(yes(&lua, "return #order == 3 and order[3] == 'timer'"), "the timer fired while the task waited");
    release(9001);
    let d = settle(&h);
    assert_eq!((d.callbacks, d.tasks), (0, 2), "two waits, two resumptions, no callback");
    assert!(
        yes(
            &lua,
            r#"return order[1] == "asked" and order[2] == "returned" and order[3] == "timer"
              and order[4] == "answered text 9001,5" and order[5] == "many 2 9002,5 9003,6""#
        ),
        "{:?}",
        lua.load("return table.concat(order, ' | ')").eval::<String>()
    );
    assert!(yes(&lua, "return type(seen.time) == 'number' and seen.epoch == 5 and seen.skipped == false and seen.newer == false and seen.lang == 'en-US'"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.tasks.len(), 0);
}

/// `pcall(host.ocr.recognize, region)` in a task waits and answers `true, reading` — VPS Avenger's
/// form; and a code dependency's function, evaluated in the dependent's VM with its own `host`,
/// waits in the dependent's task.
#[test]
fn a_wait_inside_pcall_or_a_dependencys_function_waits() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        local dep = (function(host)
          return { readIt = function(r) return host.ocr.recognize({ region = r }) end }
        end)(host)
        task.run(function()
          ok, r = pcall(host.ocr.recognize, { region = { 9011, 5, 9041, 15 } })
          viaDep = dep.readIt({ 9012, 5, 9042, 15 }).text
          xok, inXpcall = xpcall(function() return host.ocr.recognize({ region = { 9013, 5, 9043, 15 } }).text end,
            function(e) return e end)
          for _, x in pairs({ a = 9014 }) do inPairs = host.ocr.recognize({ region = { x, 5, x + 30, 15 } }).text end
          local function upTo(n, i) if i < n then return i + 1 end end
          for i in upTo, 1, 0 do inLoop = host.ocr.recognize({ region = { 9014 + i, 5, 9044 + i, 15 } }).text end
        end)
        "#,
    );
    assert!(yes(&lua, "return ok == nil"), "nothing before the delivery");
    settle(&h);
    assert!(yes(&lua, "return ok == true and r.text == '9011,5' and viaDep == '9012,5'"));
    assert!(yes(&lua, "return xok == true and inXpcall == '9013,5' and inPairs == '9014,5' and inLoop == '9015,5'"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

// ── One call, two forms ──────────────────────────────────────────────────────────────────────

/// `host.ocr.recognize` with a callback waits nowhere — not in a task either, where the call without
/// one would — and takes its options left out (`recognize(what, cb)`). A list answers two values in
/// both forms: the list in order and the table by name. An explicit `nil` as the third argument is
/// almost always a callback that is missing, and raises at the caller's line.
#[test]
fn the_callback_form_waits_nowhere_and_a_list_answers_two_values_in_both_forms() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        "task.run(function()\n\
           host.ocr.recognize({ 9801, 5, 9831, 15 }, function(r) cbText = r.text end)\n\
           afterCallback = cbText == nil\n\
           local list, byName = host.ocr.recognize({ { name = 'a', region = { 9802, 5, 9832, 15 } }, { 9803, 5, 9833, 15 } })\n\
           waitedList = #list .. ' ' .. byName.a.text .. ' ' .. list[2].text\n\
         end)\n\
         host.ocr.recognize({ { name = 'b', region = { 9804, 5, 9834, 15 } } }, nil, function(list, byName)\n\
           cbList = #list .. ' ' .. byName.b.text\n\
         end)\n\
         okNil, errNil = pcall(function()\n\
           return host.ocr.recognize({ 9805, 5, 9835, 15 }, nil, nil)\n\
         end)\n\
         errNil = tostring(errNil)\n",
    );
    assert!(yes(&lua, "return afterCallback == true and waitedList == nil"), "the callback form returned at once, in the task");
    let err: String = lua.globals().get("errNil").unwrap();
    assert!(yes(&lua, "return okNil == false"));
    assert!(err.contains(&format!("mod.luau:11: {}", task::NIL_CB)), "{err}");
    settle(&h);
    assert!(yes(&lua, "return cbText == '9801,5' and waitedList == '2 9802,5 9803,5' and cbList == '1 9804,5'"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// In a module that reads through desktop duplication, its first read carries the comparison of
/// the two ways of reading to the capture thread, which makes it there — never the event loop, in
/// either form. Made, never again for the VM; not made yet, every read carries it again.
#[test]
fn the_first_read_comparison_goes_with_the_read_to_the_capture_thread() {
    let h = host();
    let dup = CaptureSource::Duplication { or_standard: true };
    let lua = vm(&h, 1);
    capture_source::record_for_a_test(&lua, "com.dup.made", dup);
    run(&lua, "host.ocr.recognize({ 9401, 5, 9431, 15 }, function(r) first = r.text end)");
    settle(&h);
    run(&lua, "task.run(function() second = host.ocr.recognize({ 9402, 5, 9432, 15 }).text end)");
    settle(&h);
    assert!(yes(&lua, "return first == '9401,5' and second == '9402,5'"));
    assert_eq!(compared("com.dup.made"), ["screen-capture"], "once, on the capture thread");
    assert_eq!(capture_source::compare_state(&lua), Some(Compare::Done));
    let later = vm(&h, 2);
    capture_source::record_for_a_test(&later, "com.dup.not yet", dup);
    run(&later, "host.ocr.recognize({ 9403, 5, 9433, 15 }, function(r) a = r.text end)");
    settle(&h);
    run(&later, "task.run(function() b = host.ocr.recognize({ 9404, 5, 9434, 15 }).text end)");
    settle(&h);
    assert!(yes(&later, "return a == '9403,5' and b == '9404,5'"));
    assert_eq!(compared("com.dup.not yet"), ["screen-capture", "screen-capture"], "asked again until made");
    assert_eq!(capture_source::compare_state(&later), Some(Compare::Owed));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// Before the recognise thread has published its language list, `languages` answers `{}` and
/// `resolveLanguage` `nil` at once — they never wait for it — and the log says so, once a session
/// through `OcrState::once` (said, not counted, here). A read the recogniser answers comes after
/// the list: its language is resolved on that thread, after it. So that read's callback, or a
/// handler after it, always finds the list, and `nil` there never means "not known yet".
#[test]
fn the_language_list_answers_at_once_and_is_known_before_the_recogniser_answers_a_read() {
    let h = holder(Service::spawn(OcrWorker { languages: held_langs, ..worker() }));
    let lua = vm(&h, 1);
    run(
        &lua,
        "early = #host.ocr.languages()\n\
         earlyResolved = host.ocr.resolveLanguage('en')\n\
         again = #host.ocr.languages()\n\
         host.ocr.recognize({ 9411, 5, 9441, 15 }, function(r)\n\
           text = r.text\n\
           inCallback = host.ocr.languages()[1]\n\
           resolved = host.ocr.resolveLanguage(nil)\n\
         end)\n\
         task.run(function()\n\
           waited = host.ocr.recognize({ 9412, 5, 9442, 15 }).text\n\
           afterWait = host.ocr.resolveLanguage('en')\n\
         end)\n",
    );
    assert!(yes(&lua, "return early == 0 and earlyResolved == nil and again == 0"), "answered at once, from nothing");
    assert!(h.state.has_said("langs-late"), "the line is said");
    // Both photographed at the call, and neither answered: the recogniser reads the list first.
    let until = Instant::now() + Duration::from_secs(10);
    while !(taken(9411) && taken(9412)) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    fire(&h);
    assert!(taken(9411) && taken(9412));
    assert!(yes(&lua, "return text == nil and waited == nil"), "no read is answered before the list");
    release_langs();
    settle(&h);
    assert!(yes(&lua, "return text == '9411,5' and inCallback == 'en-US' and resolved == 'en-US'"));
    assert!(yes(&lua, "return waited == '9412,5' and afterWait == 'en-US'"));
    assert!(yes(&lua, "return #host.ocr.languages() == 1"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// A module that acts while a read of its own is out acts at once — `host.input.*` and
/// `host.window.focus` note it and go on (lib.rs holds that this is all they do first): the
/// picture is still being taken when the note returns. The log says so once a session per module
/// (`OcrState::once`, said, not counted, here), in the words docs/api/input.md gives, while the
/// capture thread has the picture still to take: a module with nothing out says nothing, nor one
/// whose read is taken and being recognised, nor one with only a timed snapshot request still to
/// come — the overlay runtime's menu shots, asked just before the click they are to show — and a
/// plain snapshot request counts as a read does. The read is answered as usual afterwards.
#[test]
fn acting_with_a_read_out_waits_for_nothing_and_is_noted_once() {
    const DOC: &str = include_str!("../../../docs/api/input.md");
    let h = host();
    let lua = vm(&h, 1);
    let _other = vm(&h, 2);
    let recognising = vm(&h, 3);
    // First, while the capture thread is free: a read taken and being recognised.
    hold_recognition(9454);
    run(&recognising, "host.ocr.recognize({ 9454, 5, 9484, 15 }, function(r) seen = r.text end)");
    wait_recognising(9454);
    assert!(!reads::note_acting(&*h, 3, "host.input.click", false), "its picture is taken: nothing said");
    assert!(!h.state.has_said("acted\u{1}3"));

    hold(9451);
    run(&lua, "host.ocr.recognize({ 9451, 5, 9481, 15 }, function(r) text = r.text end)");
    assert!(reads::note_acting(&*h, 1, "host.input.click", false), "noted");
    assert!(!taken(9451), "the picture is still held: the click waited for nothing");
    assert!(h.state.has_said("acted\u{1}1"));
    assert!(!reads::note_acting(&*h, 1, "host.input.send", false), "once a session");
    assert!(!reads::note_acting(&*h, 2, "host.input.click", false), "nothing of its own out: nothing said");

    // The capture thread is held at 9451 now, so module 2's requests wait behind it.
    let snap = |x: i32, kind: SnapKind| SnapReq {
        ticket: SnapTicket { id: x as u64, owner: Owner { idx: 2, gen: 1 }, prio: Priority::Interactive },
        region: Rect::new(x, 5, 30, 10),
        source: CaptureSource::Standard,
        kind,
        asked: Instant::now(),
        cancel: Arc::new(AtomicBool::new(false)),
        compare: None,
    };
    h.ocr.submit_snap(snap(9452, SnapKind::At(Instant::now() + Duration::from_secs(60))));
    assert!(!reads::note_acting(&*h, 2, "host.input.click", true), "a timed picture still to come is not counted");
    hold(9453);
    h.ocr.submit_snap(snap(9453, SnapKind::Plain));
    assert!(reads::note_acting(&*h, 2, "host.window.focus", true), "a plain snapshot request counts");
    release(9453);
    release(9454);
    release(9451);
    settle(&h);
    assert!(yes(&lua, "return text == '9451,5'"));
    assert!(yes(&recognising, "return seen == '9454,5'"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    let line = reads::acting_line("m1", "host.input.click");
    assert_eq!(
        line,
        "[m1] acted (host.input.click) while a picture it asked for was not taken yet; that picture may \
         show the input — act once the picture is answered: after the call returns in a handler, or in \
         its callback"
    );
    let doc = reads::acting_line("<module>", "host.input.click");
    assert!(DOC.contains(&format!("`{doc}`")), "docs/api/input.md does not give `{doc}`");
}

// ── Where a task cannot wait ─────────────────────────────────────────────────────────────────

/// Where a handler cannot stop, `recognize` without a callback raises the message for the place at
/// the caller's line, and reads nothing: outside every handler — the top level of a module, an
/// included file's top level, which the host calls as a function — and inside one, in a
/// metamethod, a sort comparator, a gsub function, a for loop's iterator, an `xpcall` error
/// handler, `table.foreach` and `foreachi`, Lua the host calls back (an arbiter's `onActivate` or
/// `onDeactivate`, an `onChange` from the module's own `set`), and a coroutine of the module's
/// own. A `pcall` returns the message; the handler goes on, and its own wait still waits.
#[test]
fn where_a_task_cannot_wait_recognize_raises_at_the_callers_line() {
    let h = host();
    let lua = vm(&h, 1);
    // An included file, as the host runs one: a chunk of its own name, called as a function.
    let included = lua
        .load("return host.ocr.recognize({ region = { 9022, 5, 9052, 15 } })")
        .set_name("=inc.luau")
        .into_function()
        .unwrap();
    lua.globals().set("included", included).unwrap();
    run(
        &lua,
        "local R = { region = { 9021, 5, 9051, 15 } }\n\
         okTop, top = pcall(function()\n\
           return host.ocr.recognize(R)\n\
         end)\n\
         okInc, inc = pcall(hostCall, included); inc = tostring(inc)\n\
         task.run(function()\n\
           local function try(f, ...) local ok, e = pcall(f, ...); return { ok = ok, e = tostring(e) } end\n\
           got = { handler = {} }\n\
           local meta = setmetatable({}, { __index = function() return host.ocr.recognize(R) end })\n\
           got.meta = try(function() return meta.x end)\n\
           got.sort = try(table.sort, { 3, 1, 2 }, function(a, b) host.ocr.recognize(R); return a < b end)\n\
           got.gsub = try(string.gsub, 'ab', '.', function(c) host.ocr.recognize(R); return c end)\n\
           got.iter = try(function() for _ in function() return host.ocr.recognize(R) end do end end)\n\
           xpcall(error, function()\n\
             got.handler.ok, got.handler.e = pcall(function() return host.ocr.recognize(R) end)\n\
           end, 'raised')\n\
           got.foreach = try(table.foreach, { 1 }, function() host.ocr.recognize(R) end)\n\
           got.foreachi = try(table.foreachi, { 1 }, function() host.ocr.recognize(R) end)\n\
           got.onActivate = try(hostCall, function() host.ocr.recognize(R) end)\n\
           got.onChange = try(hostCall, function() host.ocr.recognize(R) end)\n\
           got.own = try(coroutine.wrap(function() return host.ocr.recognize(R) end))\n\
           got.waited = host.ocr.recognize(R).text\n\
         end)\n",
    );
    let cannot = task::wait_message(Case::CannotWait);
    let own = task::wait_message(Case::OwnCoroutine);
    let top: String = lua.globals().get("top").unwrap();
    assert!(yes(&lua, "return okTop == false and okInc == false"));
    assert!(top.contains(&format!("mod.luau:3: {cannot}")), "{top}");
    let inc: String = lua.globals().get("inc").unwrap();
    assert!(inc.contains(&format!("inc.luau:1: {cannot}")), "{inc}");
    settle(&h);
    assert!(yes(&lua, "return got.waited == '9021,5'"), "the handler went on, and its own wait waited");
    let got: Table = lua.globals().get("got").unwrap();
    for (place, line, message) in [
        ("meta", 9, cannot),
        ("sort", 11, cannot),
        ("gsub", 12, cannot),
        ("iter", 13, cannot),
        ("handler", 15, cannot),
        ("foreach", 17, cannot),
        ("foreachi", 18, cannot),
        ("onActivate", 19, cannot),
        ("onChange", 20, cannot),
        ("own", 21, own),
    ] {
        let t: Table = got.get(place).unwrap();
        let e: String = t.get("e").unwrap();
        assert!(!t.get::<bool>("ok").unwrap(), "{place}: it did not raise");
        assert!(e.contains(&format!("mod.luau:{line}: {message}")), "{place}: {e}");
    }
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert!(!taken(9022), "nothing was photographed where it could not wait");
}

/// A call with a mistake where it cannot wait raises the place's message too: nothing reads
/// there, so the place is the mistake to mend first. The raise's traceback names the shim
/// `host.ocr`, not a task.
#[test]
fn a_mistaken_call_where_it_cannot_wait_raises_the_places_message() {
    let h = host();
    let lua = vm(&h, 1);
    run(&lua, "ok, err = pcall(host.ocr.recognize, { mistake = true }); err = tostring(err)");
    assert!(yes(&lua, "return ok == false"));
    let err: String = lua.globals().get("err").unwrap();
    assert!(err.contains(task::wait_message(Case::CannotWait)), "{err}");
    let traced: String = lua
        .load("return select(2, xpcall(host.ocr.recognize, debug.traceback, { 1, 2, 3, 4 }))")
        .set_name("=mod.luau")
        .eval()
        .unwrap();
    assert!(traced.contains("host.ocr:") && !traced.contains("task_shim"), "{traced}");
}

/// The blocking calls' tally, which a call that cannot wait on the event loop goes through
/// (`task::legacy`): its first answer per module, call and case is written with how long it held
/// the loop — a raise writes nothing, so the first that answers still does — every call is
/// counted per module and call for the summary at exit, a module's lines are news again once
/// forgotten, and the scope it runs in is the loop guard's of its kind.
#[test]
fn a_blocking_call_is_said_once_per_module_call_and_case_and_counted() {
    let h = host();
    let lua = vm(&h, 1);
    let other = vm(&h, 2);
    for l in [&lua, &other] {
        let hh = h.clone();
        let blocking = l
            .create_function(move |lua, (call, why, fail): (String, String, bool)| {
                let call: &'static str = if call == "pixel" { "host.screen.pixel" } else { "host.screen.save" };
                let idx = lua.app_data_ref::<VmOwner>().map(|o| o.idx).unwrap_or(0);
                task::legacy(&*hh, lua, idx, call, crate::loop_guard::Kind::Screen, &why, || {
                    if fail {
                        return Err(mlua::Error::external("a mistake in the call"));
                    }
                    Ok(true)
                })
            })
            .unwrap();
        l.globals().set("blocking", blocking).unwrap();
    }
    run(&lua, "pcall(blocking, 'pixel', 'none', true)");
    assert!(!h.tasks.legacy_said("m1", "host.screen.pixel", Case::CannotWait), "a raise was logged as the first call");
    run(
        &lua,
        "blocking('pixel', 'none', false); blocking('pixel', 'cannot-wait', false)\n\
         blocking('pixel', 'foreign', false); blocking('save', 'none', false)",
    );
    run(&other, "blocking('pixel', 'none', false)");
    for (id, call, case) in [
        ("m1", "host.screen.pixel", Case::CannotWait),
        ("m1", "host.screen.pixel", Case::OwnCoroutine),
        ("m1", "host.screen.save", Case::CannotWait),
        ("m2", "host.screen.pixel", Case::CannotWait),
    ] {
        assert!(h.tasks.legacy_said(id, call, case), "{id} {call} {case:?} not said");
    }
    assert!(!h.tasks.legacy_said("m2", "host.screen.save", Case::CannotWait));
    h.tasks.forget_legacy_said("m1");
    assert!(!h.tasks.legacy_said("m1", "host.screen.pixel", Case::CannotWait));
    assert!(h.tasks.legacy_said("m2", "host.screen.pixel", Case::CannotWait), "only m1's lines are forgotten");
    run(&lua, "blocking('pixel', 'none', false)");
    assert!(h.tasks.legacy_said("m1", "host.screen.pixel", Case::CannotWait), "written again after it was forgotten");
    let summary: Vec<String> = h.tasks.legacy_summary().into_iter().map(|(scope, line)| format!("{scope}: {line}")).collect();
    assert_eq!(summary.len(), 3, "{summary:?}");
    assert!(summary[0].starts_with("screen: [m1] host.screen.pixel held the event loop 5 time(s), "), "{summary:?}");
    assert!(summary[1].starts_with("screen: [m1] host.screen.save held the event loop 1 time(s), "), "{summary:?}");
    assert!(summary[2].starts_with("screen: [m2] host.screen.pixel held the event loop 1 time(s), "), "{summary:?}");
}

// ── cancel, disable, reload ──────────────────────────────────────────────────────────────────

/// `cancel` of a waiting task: it never goes on, its read is withdrawn — the picture is never
/// taken — and its thread lets go of what it held. Of the running task: it ends at its next
/// wait. Of a number that ended, another VM's, or no number: false.
#[test]
fn cancel_ends_a_waiting_task_at_once_and_a_running_one_at_its_next_wait() {
    let h = host();
    let lua = vm(&h, 1);
    let other = vm(&h, 2);
    hold(9041); // the capture thread is busy with another read
    run(
        &lua,
        r#"
        weak = setmetatable({}, { __mode = "v" })
        busy = task.run(function() host.ocr.recognize({ region = { 9041, 5, 9071, 15 } }) end)
        t = task.run(function()
          local mine = {}
          weak[1] = mine
          host.ocr.recognize({ region = { 9042, 5, 9072, 15 } })
          resumed = mine
        end)
        "#,
    );
    other.globals().set("t", lua.globals().get::<i64>("t").unwrap()).unwrap();
    assert!(yes(&other, "return task.cancel(t) == false and task.alive(t) == false"), "another VM's number");
    assert!(yes(&lua, "return task.alive(t) == true and task.cancel(t) == true and task.alive(t) == false"));
    assert!(yes(&lua, "return task.cancel(t) == false and task.cancel(nil) == false and task.cancel('1') == false and task.cancel(1.5) == false"));
    release(9041);
    settle(&h);
    assert!(!taken(9042), "the cancelled task's picture was taken");
    run(&lua, "collectgarbage('collect')");
    assert!(yes(&lua, "return resumed == nil and weak[1] == nil"), "it went on, or its thread still holds its locals");

    run(
        &lua,
        r#"
        steps = {}
        me = task.run(function()
          host.ocr.recognize({ region = { 9043, 5, 9073, 15 } })
          steps[#steps + 1] = "second stretch"
          selfCancel = task.cancel(me)
          selfCancelAgain = task.cancel(me)
          aliveAfter = task.alive(me)
          host.ocr.recognize({ region = { 9044, 5, 9074, 15 } })
          steps[#steps + 1] = "after its next wait"
        end)
        "#,
    );
    settle(&h);
    assert!(yes(
        &lua,
        "return #steps == 1 and selfCancel == true and selfCancelAgain == false and aliveAfter == true and task.alive(me) == false"
    ));
    assert!(!taken(9044), "the wait that ended it asked for nothing");
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.tasks.len(), 0);
}

/// A disable during a wait: the task never goes on, not even after a quick enable, and its thread
/// lets go of what it held. A task started while its module is disabled — an onChange from the
/// settings dialog, an onDeactivate during the disable — ends at its wait, and queues nothing.
#[test]
fn a_disabled_modules_tasks_never_go_on() {
    let h = host();
    let lua = vm(&h, 1);
    hold(9051);
    run(
        &lua,
        r#"
        weak = setmetatable({}, { __mode = "v" })
        t = task.run(function()
          local mine = {}
          weak[1] = mine
          host.ocr.recognize({ region = { 9051, 5, 9081, 15 } })
          resumed = mine
        end)
        "#,
    );
    disable(&h, 1);
    run(
        &lua,
        r#"
        -- an onChange the settings dialog calls while the module is off
        u = task.run(function()
          host.ocr.recognize({ region = { 9052, 5, 9082, 15 } })
          resumedWhileOff = true
        end)
        "#,
    );
    assert!(!h.state.has_pending(), "a wait in a disabled module queued a read");
    enable(&h, 1);
    release(9051);
    settle(&h);
    run(&lua, "collectgarbage('collect')");
    assert!(yes(&lua, "return resumed == nil and resumedWhileOff == nil and weak[1] == nil"));
    assert!(yes(&lua, "return task.alive(t) == false and task.alive(u) == false"));
    assert!(!taken(9052));
    assert_eq!(h.tasks.len(), 0);
}

/// A reload's purge, as `purge_module` does it: what the module had, dropped; then its active
/// overlay's onDeactivate, run in the old VM, which starts a task that waits and arms a timer;
/// then everything dropped once more. Neither is left, nothing reaches the old VM, and the old VM
/// is freed. A rollback drops a module's tasks the same way.
#[test]
fn a_reload_leaves_nothing_of_the_old_vm_and_frees_it() {
    let h = host();
    let old = vm(&h, 1);
    hold(9061);
    run(
        &old,
        r#"
        task.run(function() host.ocr.recognize({ region = { 9061, 5, 9091, 15 } }); reached = true end)
        onDeactivate = function()
          task.run(function() host.ocr.recognize({ region = { 9062, 5, 9092, 15 } }); reached = true end)
          host.timer.after(0, function() reached = true end)
        end
        "#,
    );
    let purge = |h: &Host| {
        h.timers.retain(|i| i != 1);
        reads::drop_owner(h, 1, true);
        task::drop_owner(h, 1);
    };
    purge(&h);
    let deactivate: Function = old.globals().get("onDeactivate").unwrap();
    deactivate.call::<()>(()).unwrap();
    drop(deactivate);
    assert!(h.tasks.has_tasks_of(1) && !h.timers.is_empty(), "the onDeactivate started its task and armed its timer");
    purge(&h);
    assert!(!h.tasks.has_tasks_of(1) && h.timers.is_empty() && !h.state.has_pending());
    let weak = old.weak();
    drop(old);
    assert!(weak.try_upgrade().is_none(), "something still holds the old VM");
    let _new = vm(&h, 1);
    release(9061);
    settle(&h);
    fire_timers(&h, Instant::now() + Duration::from_secs(1));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));

    // A rollback of a hot-load: every module from the first one loaded on.
    let lua = vm(&h, 3);
    run(&lua, "task.run(function() host.ocr.recognize({ region = { 9063, 5, 9093, 15 } }); reached = true end)");
    task::drop_from(&*h, 2);
    reads::drop_from(&*h, 2);
    assert!(!h.tasks.has_tasks_of(3));
    settle(&h);
    assert!(yes(&lua, "return reached == nil"));
}

/// A module dropped while a task of it runs — a reload's purge, from a host function the task
/// called: the task ends at its next wait, and nothing after that wait runs. A `pcall` around the
/// wait does not bring it back: there is nothing to catch.
#[test]
fn a_task_whose_module_goes_while_it_runs_ends_at_its_wait() {
    let h = host();
    let lua = vm(&h, 1);
    let hh = h.clone();
    lua.globals()
        .set(
            "dropMyModule",
            lua.create_function(move |_, ()| {
                task::drop_owner(&*hh, 1);
                Ok(())
            })
            .unwrap(),
        )
        .unwrap();
    run(
        &lua,
        r#"
        t = task.run(function()
          dropMyModule()
          before = task.alive(t)
          ok, err = pcall(host.ocr.recognize, { region = { 9161, 5, 9191, 15 } })
          after = true
        end)
        u = task.run(function()
          dropMyModule()
          -- Options that would raise are not even read: nothing runs on past the wait.
          okBad, errBad = pcall(host.ocr.recognize, { region = { x = 1 } })
          afterBad = true
        end)
        "#,
    );
    assert!(yes(&lua, "return ok == nil and err == nil and after == nil and task.alive(t) == false"));
    assert!(yes(&lua, "return okBad == nil and afterBad == nil and task.alive(u) == false"));
    assert!(!h.state.has_pending(), "the wait queued a read");
    assert_eq!(h.tasks.len(), 0);
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    settle(&h);
    assert!(!taken(9161));
}

/// A task that waited in a module that was then disabled and enabled again quickly is not resumed
/// (F13): a disable ends the task, it does not pause it.
#[test]
fn a_quick_reenable_does_not_resume_what_the_disable_ended() {
    let h = host();
    let lua = vm(&h, 1);
    hold_recognition(9071);
    run(&lua, "t = task.run(function() host.ocr.recognize({ region = { 9071, 5, 9101, 15 } }); resumed = true end)");
    wait_recognising(9071);
    disable(&h, 1);
    enable(&h, 1);
    release(9071);
    std::thread::sleep(Duration::from_millis(30));
    settle(&h);
    fire(&h);
    assert!(yes(&lua, "return resumed == nil and task.alive(t) == false"));
}

// ── What module code can do to a task ────────────────────────────────────────────────────────

/// Module code that resumes a waiting task's coroutine itself: the wait raises there, the task is
/// no task any more — forgotten, its read withdrawn — and a `cancel` from a coroutine it resumes
/// in turn resets no live thread. Module code that closes it: it has ended, and its answer is
/// dropped without a word.
#[test]
fn module_code_resuming_or_closing_a_task_takes_it_from_the_host() {
    let h = host();
    let lua = vm(&h, 1);
    hold(9081);
    run(
        &lua,
        r#"
        id = task.run(function()
          co = coroutine.running()
          ok, err = pcall(host.ocr.recognize, { region = { 9081, 5, 9111, 15 } })
          -- Running now under the module's own resume: a coroutine of its own cancels the task.
          cancelled = coroutine.wrap(function() return task.cancel(id) end)()
          finished = true
        end)
        resumedBy = { coroutine.resume(co, "not the host") }
        "#,
    );
    assert!(yes(&lua, "return ok == false and cancelled == false and finished == true and resumedBy[1] == true"));
    let err: String = lua.load("return tostring(err)").eval().unwrap();
    assert!(err.contains(&format!("host.ocr.recognize{}", task::RESUMED)), "{err}");
    assert!(yes(&lua, "return task.alive(id) == false"));
    assert!(!h.state.has_pending(), "the disowned task's read was not withdrawn");
    release(9081);

    run(
        &lua,
        r#"
        closed = task.run(function()
          co2 = coroutine.running()
          host.ocr.recognize({ region = { 9082, 5, 9112, 15 } })
          resumed = true
        end)
        closedOk = coroutine.close(co2)
        "#,
    );
    assert!(yes(&lua, "return closedOk == true and task.alive(closed) == false"));
    settle(&h);
    assert!(yes(&lua, "return resumed == nil"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.tasks.len(), 0);
}

/// The shim's `start`, taken off the stack with `debug.info` while it reads its arguments — a
/// window region's window, whose `client` a metamethod answers — and called by module code:
/// outside its place it raises and queues nothing; called inside the task's own wait, the task's
/// own wait then raises, and the stray read is withdrawn when the task ends — no reading is left.
#[test]
fn start_called_by_module_code_raises_and_leaves_no_reading() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        -- The function the shim's wait called: the frame below the one whose source is the shim,
        -- `host.ocr` (task::SHIM_NAME) as `debug.info` shows it.
        local function stealStart()
          for level = 2, 30 do
            local src = debug.info(level, "s")
            if src == nil then return nil end
            if src == "host.ocr" then return debug.info(level - 1, "f") end
          end
        end
        local sneaky = setmetatable({}, { __index = function(_, k)
          if k == "client" then
            stolen = stealStart()
            nested = { pcall(stolen, { 9091, 5, 9121, 15 }, nil, {}) }
            return { x = 9093, y = 0, w = 30, h = 10 }
          end
          return nil
        end })
        task.run(function()
          okWait, errWait = pcall(host.ocr.recognize, { window = sneaky, fraction = { 0, 0, 1, 1 } })
          errWait = tostring(errWait)
        end)
        ok, err = pcall(stolen, { 9092, 5, 9122, 15 }, nil, {})
        err = tostring(err)
        "#,
    );
    assert!(yes(&lua, "return type(stolen) == 'function' and debug.info(stolen, 's') == '[C]'"), "start was not found on the stack");
    assert!(yes(&lua, "return ok == false and okWait == false and nested[1] == true"));
    for name in ["err", "errWait"] {
        let e: String = lua.globals().get(name).unwrap();
        assert!(e.contains("host.ocr.recognize: the host's wait was called from outside its place"), "{name}: {e}");
    }
    assert!(!h.state.has_pending(), "a reading was left");
    assert_eq!(h.tasks.len(), 0);
    settle(&h);
    assert!(!taken(9092));
}

/// A task's own `coroutine.yield` through to the host raises where it was made, as the same yield
/// does on the event loop: a `pcall` around it catches it and the task goes on; without one the
/// task ends with that error, reported. Nothing spins.
#[test]
fn a_tasks_own_yield_raises_at_the_yield() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        a = task.run(function()
          okY, errY = pcall(coroutine.yield, "anything")
          after = true
        end)
        b = task.run(function() coroutine.yield() end)
        "#,
    );
    assert!(yes(&lua, "return okY == false and after == true and task.alive(a) == false and task.alive(b) == false"));
    let e: String = lua.load("return tostring(errY)").eval().unwrap();
    assert!(e.contains(task::OWN_YIELD), "{e}");
    let errs = errors(&h);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].1 == "task" && errs[0].2.contains(task::OWN_YIELD), "{errs:?}");
    assert_eq!(h.tasks.len(), 0);
}

/// A Rust panic in a host function a task calls is caught as the task's error, in the first
/// stretch as in a later one.
#[test]
fn a_panic_in_a_host_function_ends_only_the_task() {
    crate::quiet_expected_panics();
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        task.run(function() boom() end)
        task.run(function() host.ocr.recognize({ region = { 9101, 5, 9131, 15 } }); boom() end)
        afterwards = true
        "#,
    );
    settle(&h);
    let errs = errors(&h);
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(errs.iter().all(|e| e.1 == "task" && e.2.contains("a host function fell over")), "{errs:?}");
    assert!(yes(&lua, "return afterwards == true"));
    assert_eq!(h.tasks.len(), 0);
}

// ── Priority, keys, failures, nesting ────────────────────────────────────────────────────────

/// A task asks every wait with the priority of the dispatch that started it — after its first
/// wait as before: interactive from a key, background from a poll.
#[test]
fn a_task_keeps_the_priority_of_what_started_it() {
    let h = host();
    let lua = vm(&h, 1);
    {
        let _key = enter_priority(Priority::Interactive);
        run(
            &lua,
            r#"
            task.run(function()
              keyFirst = host.ocr.recognize({ region = { 9111, 79, 9141, 89 } }).text
              keySecond = host.ocr.recognize({ region = { 9112, 79, 9142, 89 } }).text
            end)
            "#,
        );
    }
    run(
        &lua,
        r#"
        poll = host.timer.every(10, function()
          host.timer.cancel(poll)
          task.run(function()
            pollFirst = host.ocr.recognize({ region = { 9113, 79, 9143, 89 } }).text
            pollSecond = host.ocr.recognize({ region = { 9114, 79, 9144, 89 } }).text
          end)
        end)
        "#,
    );
    fire_timers(&h, Instant::now() + Duration::from_millis(50));
    settle(&h);
    assert!(yes(
        &lua,
        r#"return keyFirst == "preempt=false" and keySecond == "preempt=false"
          and pollFirst == "preempt=true" and pollSecond == "preempt=true""#
    ));
    assert_eq!(current_priority(), Priority::Background, "restored after each stretch");
}

/// With a `key`, the newest wins as for a read with a callback: a wait whose read had not been recognised yet when
/// a newer one with its key was asked comes back `"stale"` — no words, no time, no input epoch —
/// and the task goes on; one already being recognised comes back with `newer`.
#[test]
fn a_superseded_wait_comes_back_stale_and_the_task_goes_on() {
    let h = host();
    let lua = vm(&h, 1);
    hold(9121);
    run(
        &lua,
        r#"
        task.run(function()
          local r = host.ocr.recognize({ 9121, 5, 9151, 15 }, { key = "field" })
          first = { status = r.status, newer = r.newer, words = #r.words, time = r.time, epoch = r.inputEpoch }
        end)
        task.run(function()
          second = host.ocr.recognize({ 9122, 5, 9152, 15 }, { key = "field" }).status
        end)
        "#,
    );
    release(9121);
    settle(&h);
    assert!(yes(
        &lua,
        r#"return first.status == "stale" and first.newer == true and first.words == 0 and first.time == nil
          and first.epoch == nil and second == "text""#
    ));

    hold_recognition(9123);
    run(&lua, r#"task.run(function() third = host.ocr.recognize({ 9123, 5, 9153, 15 }, { key = "row" }) end)"#);
    wait_recognising(9123);
    run(&lua, r#"task.run(function() fourth = host.ocr.recognize({ 9124, 5, 9154, 15 }, { key = "row" }) end)"#);
    release(9123);
    settle(&h);
    assert!(yes(&lua, "return third.status == 'text' and third.newer == true and fourth.status == 'text' and fourth.newer == false"));
}

/// What goes wrong routinely comes back as a reading, never a raise: a recognition that failed,
/// a window with nothing to read. A mistake in the call raises at the wait, in the task, where a
/// `pcall` catches it.
#[test]
fn routine_failures_are_readings_and_mistakes_raise_at_the_wait() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        task.run(function()
          local r = host.ocr.recognize({ region = { 666, 5, 696, 15 } })
          failed = { status = r.status, error = r.error, skipped = r.skipped, time = r.time, epoch = r.inputEpoch }
          local w = host.ocr.recognize({ region = { window = { client = { x = 0, y = 0, w = 0, h = 0 } }, fraction = { 0, 0, 1, 1 } } })
          empty = { status = w.status, error = w.error, time = w.time }
          okBad, bad = pcall(host.ocr.recognize, { region = { x = 1 } })
          bad = tostring(bad)
        end)
        "#,
    );
    settle(&h);
    assert!(yes(
        &lua,
        r#"return failed.status == "failed" and failed.error == "the fake recogniser failed" and failed.skipped == false
          and failed.time == nil and failed.epoch == nil
          and empty.status == "failed" and empty.error == "the window's client area is empty (0x0)" and empty.time == nil
          and okBad == false and string.find(bad, "host.ocr.recognize: the entry.region", 1, true) ~= nil"#
    ));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// The limit of reads holds for waits: a module's seventeenth wait pushes out its oldest one that has
/// no key and is not being recognised, and that task goes on with a `"failed"` reading on the next
/// turn of the loop — never inside the call that pushed it out.
#[test]
fn a_wait_pushed_out_by_the_limit_fails_on_the_next_turn() {
    let h = host();
    let lua = vm(&h, 1);
    hold(9171); // the first wait's picture is not taken yet
    run(
        &lua,
        r#"
        got = {}
        for i = 0, 16 do
          task.run(function()
            local r = host.ocr.recognize({ region = { 9171 + i, 5, 9201 + i, 15 } })
            got[i + 1] = r.status .. "|" .. tostring(r.error)
          end)
        end
        insideTheCall = got[1]
        "#,
    );
    assert!(yes(&lua, "return insideTheCall == nil"), "answered inside the call that pushed it out");
    reads::fire(&*h);
    let first: String = lua.load("return tostring(got[1])").eval().unwrap();
    assert_eq!(first, format!("failed|{}", crate::ocr::sched::TOO_MANY));
    release(9171);
    settle(&h);
    assert!(yes(&lua, "for i = 2, 17 do if got[i] ~= 'text|nil' then return false end end return true"));
}

/// Nothing to read is still a wait: a list of window regions whose windows have nothing to show is
/// answered on a later turn of the loop, each reading `"failed"` with why. Empty corners and an
/// empty list are mistakes in the call, and raise at the wait.
#[test]
fn an_empty_wait_is_answered_on_a_later_turn() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        local gone = { client = { x = 0, y = 0, w = 0, h = 0 } }
        task.run(function()
          local list = host.ocr.recognize({ { window = gone, fraction = { 0, 0, 1, 1 } }, { window = gone, fraction = { 0, 0, 0.5, 1 } } })
          many = { n = #list, status = list[2].status, error = list[2].error, time = list[2].time }
          okEmpty, emptyCorners = pcall(host.ocr.recognize, { 9205, 5, 9205, 15 })
          okNone, noRegion = pcall(host.ocr.recognize, {})
          emptyCorners, noRegion = tostring(emptyCorners), tostring(noRegion)
        end)
        "#,
    );
    assert!(yes(&lua, "return many == nil"), "answered inside the call");
    settle(&h);
    assert!(yes(
        &lua,
        r#"return many.n == 2 and many.status == "failed" and many.time == nil
          and many.error == "the window's client area is empty (0x0)"
          and okEmpty == false and string.find(emptyCorners, "host.ocr.recognize: the region { 9205, 5, 9205, 15 } is empty or turned around", 1, true) ~= nil
          and okNone == false and string.find(noRegion, "host.ocr.recognize: the region.x1 is missing", 1, true) ~= nil"#
    ));
}

/// `run` inside a metamethod and inside a `table.sort` comparator — where the call itself could
/// not wait — starts a task that does: the metamethod and the comparator return at its wait.
#[test]
fn a_task_started_where_nothing_can_wait_waits() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        local meta = setmetatable({}, { __index = function(_, k)
          task.run(function() inMeta = host.ocr.recognize({ region = { 9211, 5, 9241, 15 } }).text end)
          return k
        end })
        metaAnswered = meta.x
        table.sort({ 2, 1 }, function(a, b)
          if not sortStarted then
            sortStarted = true
            task.run(function() inSort = host.ocr.recognize({ region = { 9212, 5, 9242, 15 } }).text end)
          end
          return a < b
        end)
        "#,
    );
    assert!(yes(&lua, "return metaAnswered == 'x' and inMeta == nil and inSort == nil"));
    settle(&h);
    assert!(yes(&lua, "return inMeta == '9211,5' and inSort == '9212,5'"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// Only the stretches on the stack count toward the sixteen: a task that waits is off it. With
/// fifteen stretches nested, a sixteenth that waits, and then one more beside it — it starts.
#[test]
fn a_waiting_task_does_not_count_toward_the_nesting_cap() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        local function nest(n)
          task.run(function()
            if n < 15 then return nest(n + 1) end
            waiter = task.run(function() host.ocr.recognize({ region = { 9221, 5, 9251, 15 } }) end)
            besideIt = task.run(function() started = true end)
          end)
        end
        nest(1)
        "#,
    );
    assert!(yes(&lua, "return started == true and task.alive(waiter) == true"));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    settle(&h);
}

/// A task started while the module loads asks in the background, as a poll does; one started
/// from a `host.timer.after` asks with the priority of the dispatch that armed the timer.
#[test]
fn a_task_asks_at_load_in_the_background_and_from_after_with_its_askers_priority() {
    let h = host();
    let lua = vm(&h, 1);
    run(&lua, "task.run(function() atLoad = host.ocr.recognize({ region = { 9231, 79, 9261, 89 } }).text end)");
    {
        let _key = enter_priority(Priority::Interactive);
        run(
            &lua,
            r#"host.timer.after(0, function()
              task.run(function() afterKey = host.ocr.recognize({ region = { 9232, 79, 9262, 89 } }).text end)
            end)"#,
        );
    }
    fire_timers(&h, Instant::now() + Duration::from_millis(50));
    settle(&h);
    assert!(yes(&lua, r#"return atLoad == "preempt=true" and afterKey == "preempt=false""#));
}

/// A trigger's callback that starts a task which waits returns at the wait, so the report of the
/// window in front (`_dispatchInitial`, through the window prelude) counts it and goes on to the
/// next trigger at once; the task goes on later.
#[test]
fn a_task_that_waits_in_a_trigger_lets_the_initial_report_count_it() {
    let h = host();
    let lua = vm(&h, 1);
    let host_t: Table = lua.globals().get("host").unwrap();
    host_t.set("window", lua.create_table().unwrap()).unwrap();
    let os = lua.create_table().unwrap();
    os.set("current", "windows").unwrap();
    host_t.set("os", os).unwrap();
    crate::install_window_prelude(&lua, &host_t).unwrap();
    run(
        &lua,
        r#"
        order = {}
        host.window.onTrigger({ title = "Synth" }, { initial = true }, function()
          task.run(function()
            order[#order + 1] = "task asks"
            read = host.ocr.recognize({ region = { 9241, 5, 9271, 15 } }).text
            order[#order + 1] = "task goes on"
          end)
          order[#order + 1] = "first callback returns"
        end)
        host.window.onTrigger({ title = "Synth" }, { initial = true }, function()
          order[#order + 1] = "second callback"
        end)
        "#,
    );
    let win = crate::backend::WinInfo {
        hwnd: 7,
        title: "Synth".into(),
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
    };
    let fired = crate::dispatch_initial(&lua, Some(&win), false, None).unwrap();
    assert_eq!(fired, 2);
    assert!(yes(&lua, r#"return #order == 3 and order[2] == "first callback returns" and order[3] == "second callback""#));
    settle(&h);
    assert!(yes(&lua, r#"return order[4] == "task goes on" and read == "9241,5""#));
}

/// A task started in another's first stretch: the inner one waits, the outer one goes on at once,
/// and both are resumed, each with its own reading. Neither waits for the other.
#[test]
fn nested_tasks_wait_each_for_themselves() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        order = {}
        task.run(function()
          task.run(function()
            order[#order + 1] = "inner asks"
            inner = host.ocr.recognize({ region = { 9131, 5, 9161, 15 } }).text
          end)
          order[#order + 1] = "outer goes on"
          outer = host.ocr.recognize({ region = { 9132, 5, 9162, 15 } }).text
        end)
        "#,
    );
    settle(&h);
    assert!(yes(&lua, "return order[1] == 'inner asks' and order[2] == 'outer goes on' and inner == '9131,5' and outer == '9132,5'"));
}

/// Seventeen tasks nested in their first stretches, in a debug build, on a stack of half the size
/// of the main thread's on Windows (1 MiB, which the application does not change): the sixteenth
/// is the last that starts, the seventeenth is reported as its error, and nothing overflows — so
/// sixteen levels leave the event loop's own frames below them at least as much again.
#[test]
fn the_seventeenth_nested_first_stretch_is_reported_not_started() {
    let t = std::thread::Builder::new()
        .stack_size(1 << 19)
        .spawn(|| {
            let h = host();
            let lua = vm(&h, 1);
            run(
                &lua,
                r#"
                reached = 0
                local function nest(n)
                  task.run(function()
                    reached = n
                    if n < 17 then nest(n + 1) end
                  end)
                end
                nest(1)
                "#,
            );
            let reached: i64 = lua.globals().get("reached").unwrap();
            let errs = errors(&h);
            (reached, errs)
        })
        .unwrap();
    let (reached, errs) = t.join().expect("the nesting overflowed the stack");
    assert_eq!(reached, task::MAX_NESTED as i64);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].1 == "task" && errs[0].2.contains("16 task stretches are on the event loop's own stack"), "{errs:?}");
}

/// A task started in an onActivate that the host calls — through `Function::call`, where nothing
/// can wait — does not hold the callback up: the onActivate returns at the task's first wait, so
/// the arbiter's pairs stay in order, and the task goes on later.
#[test]
fn a_task_started_in_a_host_callback_lets_it_return() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        order = {}
        hostCall(function()
          task.run(function()
            order[#order + 1] = "task asks"
            read = host.ocr.recognize({ region = { 9141, 5, 9171, 15 } }).text
            order[#order + 1] = "task goes on"
          end)
          order[#order + 1] = "onActivate returns"
        end)
        hostCall(function() order[#order + 1] = "onDeactivate" end)
        "#,
    );
    settle(&h);
    assert!(yes(
        &lua,
        r#"return order[1] == "task asks" and order[2] == "onActivate returns" and order[3] == "onDeactivate"
          and order[4] == "task goes on" and read == "9141,5""#
    ));
}

/// What a task costs: 10 000 `run`s that never wait, timed and printed — a measurement, not a
/// limit.
#[test]
fn ten_thousand_tasks_that_never_wait() {
    let h = host();
    let lua = vm(&h, 1);
    let t = Instant::now();
    run(&lua, "n = 0 for i = 1, 10000 do task.run(function() n = n + 1 end) end");
    let took = t.elapsed();
    println!("10 000 task.run without a wait: {took:?}, {:.2} µs each", took.as_secs_f64() * 1e6 / 10_000.0);
    assert!(yes(&lua, "return n == 10000"));
    assert_eq!(h.tasks.len(), 0);
}

// ── host.ocr.pending, and two tasks as a module's code would start them ──────────────────────

/// `host.ocr.pending(key)`: true once a read with a callback or a task's wait with the key was
/// asked, false in the read's own callback and in the task it resumes, and after a disable; a key
/// that is not a non-empty string raises.
#[test]
fn pending_says_whether_a_read_or_a_wait_with_the_key_is_out() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        host.ocr.recognize({ 9151, 5, 9181, 15 }, { key = "a" }, function() inCallback = host.ocr.pending("a") end)
        afterRead = host.ocr.pending("a")
        task.run(function()
          host.ocr.recognize({ 9152, 5, 9182, 15 }, { key = "b" })
          inTask = host.ocr.pending("b")
        end)
        afterWait = host.ocr.pending("b")
        other = host.ocr.pending("c")
        "#,
    );
    assert!(yes(&lua, "return afterRead == true and afterWait == true and other == false"));
    settle(&h);
    assert!(yes(&lua, "return inCallback == false and inTask == false and host.ocr.pending('a') == false"));
    hold(9153);
    run(&lua, r#"task.run(function() host.ocr.recognize({ 9153, 5, 9183, 15 }, { key = "d" }) end)"#);
    assert!(yes(&lua, "return host.ocr.pending('d') == true"));
    disable(&h, 1);
    assert!(yes(&lua, "return host.ocr.pending('d') == false"), "after a disable");
    release(9153);
    // The type in Luau's words: a number is no "integer".
    for (bad, says) in [
        ("host.ocr.pending('')", "the key is empty"),
        ("host.ocr.pending(5)", "the key must be a non-empty string, got number"),
        ("host.ocr.pending()", "the key must be a non-empty string, got nil"),
    ] {
        let e = lua.load(bad).exec().expect_err(bad).to_string();
        assert!(e.contains(says), "{bad}: {e}");
    }
}

// ── A read a key waits behind, and the hang answer ───────────────────────────────────────────

/// Turns of the loop — the readings only — until `cond` holds in `lua`, 10 s at most.
fn fire_until(h: &Host, lua: &Lua, cond: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while !yes(lua, cond) {
        assert!(Instant::now() < until, "never: {cond}");
        reads::fire(h);
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// `f`, a global of `lua`, as an event of module 1 through its mailbox: an input or not.
fn event(h: &Host, lua: &Lua, f: &str, args: Vec<Value>, input: bool) -> mailbox::Delivered {
    let f: Function = lua.globals().get(f).unwrap();
    let what = if input { "key" } else { "timer" };
    mailbox::deliver(h, 1, lua, Event::Call { f, args: MultiValue::from_vec(args), input, what })
}

/// A key queued behind a handler that waits at a poll's read makes that read interactive — taken
/// and recognised as a key's, and so never allowed to leave out its last passes
/// (`preempt=false`) — and every read the handler waits for after it: its next read is
/// interactive too. What it arms after them stays a poll's: a read with a callback asked then is
/// recognised as a poll's (`preempt=true`), so a poll that re-arms itself does not become a
/// person's for good. The key runs after it, in order. Without the key, every read is a poll's.
#[test]
fn a_key_queued_behind_a_polls_read_makes_the_read_interactive() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        heard = {}
        function poll(x)
          heard[#heard + 1] = host.ocr.recognize({ region = { x, 79, x + 30, 89 } }).text
          heard[#heard + 1] = host.ocr.recognize({ region = { x + 1, 79, x + 31, 89 } }).text
          host.ocr.recognize({ x + 2, 79, x + 32, 89 }, function(r) armed = r.text end)
        end
        function key() heard[#heard + 1] = "key" end
        "#,
    );
    let heard = || lua.load("return table.concat(heard, ' | ')").eval::<String>().unwrap();
    assert_eq!(event(&h, &lua, "poll", vec![Value::Integer(9301)], false), mailbox::Delivered::Parked);
    settle(&h);
    assert_eq!(heard(), "preempt=true | preempt=true", "a poll's reads, with nothing behind them");
    fire_until(&h, &lua, "return armed ~= nil");
    assert!(yes(&lua, r#"return armed == "preempt=true""#));

    run(&lua, "heard = {}; armed = nil");
    hold(9311); // its picture is not taken before the key has queued
    assert_eq!(event(&h, &lua, "poll", vec![Value::Integer(9311)], false), mailbox::Delivered::Parked);
    {
        let _key = enter_priority(Priority::Interactive);
        assert_eq!(event(&h, &lua, "key", Vec::new(), true), mailbox::Delivered::Queued);
    }
    release(9311);
    settle(&h);
    mailbox::run_queued(&*h);
    assert_eq!(heard(), "preempt=false | preempt=false | key");
    assert_eq!(current_priority(), Priority::Background, "restored after each stretch");
    fire_until(&h, &lua, "return armed ~= nil");
    assert!(yes(&lua, r#"return armed == "preempt=true""#), "what the raised handler armed is still a poll's");

    // Behind it a poll's own timer, which nobody waits on: nothing is raised.
    run(&lua, "heard = {}; armed = nil");
    hold(9341);
    assert_eq!(event(&h, &lua, "poll", vec![Value::Integer(9341)], false), mailbox::Delivered::Parked);
    assert_eq!(event(&h, &lua, "key", Vec::new(), false), mailbox::Delivered::Queued);
    release(9341);
    settle(&h);
    mailbox::run_queued(&*h);
    assert_eq!(heard(), "preempt=true | preempt=true | key");
    fire_until(&h, &lua, "return armed ~= nil");
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.tasks.len(), 0);
}

/// The hang answer on the event loop: once the recogniser has answered no region for the hang
/// bound, every read still out ends `"failed"` with the reason — the one being recognised and the
/// ones behind it, a callback's and a task's wait alike — once; `pending` is false for them
/// afterwards; a read asked meanwhile is refused with the reason; and once the recogniser answers
/// again, reads are taken again and the held recognition's late answer reaches nobody. Held, not
/// timed: the recognition does not come back until the test lets it.
#[test]
fn a_hang_ends_every_read_still_out_with_the_reason() {
    let h = host_hanging_after(Duration::from_millis(100));
    let lua = vm(&h, 1);
    hold_recognition(9321);
    run(
        &lua,
        r#"
        ended = {}
        local function note(what)
          return function(r) ended[#ended + 1] = { what = what, status = r.status, error = r.error } end
        end
        host.ocr.recognize({ 9321, 5, 9351, 15 }, { key = "running" }, note("running"))
        host.ocr.recognize({ 9322, 5, 9352, 15 }, { key = "queued" }, note("queued"))
        task.run(function() note("waited")(host.ocr.recognize({ region = { 9323, 5, 9353, 15 } })) end)
        "#,
    );
    wait_recognising(9321);
    fire_until(&h, &lua, "return #ended == 3");
    assert!(
        yes(
            &lua,
            r#"local seen = {}
            for _, e in ipairs(ended) do
              if e.status ~= "failed" or string.find(e.error, "the text recogniser has not answered a region for", 1, true) ~= 1 then
                return false
              end
              seen[e.what] = true
            end
            return seen.running and seen.queued and seen.waited
              and host.ocr.pending("running") == false and host.ocr.pending("queued") == false"#
        ),
        "{:?}",
        lua.load("local t = {} for _, e in ipairs(ended) do t[#t + 1] = e.what .. ' ' .. e.status .. ' ' .. tostring(e.error) end return table.concat(t, ' | ')").eval::<String>()
    );
    run(&lua, r#"host.ocr.recognize({ 9324, 5, 9354, 15 }, function(r) late = r.status .. " " .. r.error end)"#);
    fire_until(&h, &lua, "return late ~= nil");
    assert!(yes(&lua, r#"return string.find(late, "failed the text recogniser has not answered a region for", 1, true) == 1"#));

    release(9321);
    // Taken again once the recogniser has answered; it reads one job at a time, so by the time
    // this one is answered the held one's late answer has come — and gone to nobody.
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        run(&lua, r#"again = nil; host.ocr.recognize({ 9325, 5, 9355, 15 }, function(r) again = r.status end)"#);
        fire_until(&h, &lua, "return again ~= nil");
        if yes(&lua, "return again == 'text'") {
            break;
        }
        assert!(Instant::now() < until, "never taken again");
        std::thread::sleep(Duration::from_millis(5));
    }
    settle(&h);
    assert!(yes(&lua, "return #ended == 3"), "the hang's reads were answered once");
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.tasks.len(), 0);
}

/// The hang answer for handlers of three modules that wait: each goes on once, with `"failed"` and
/// the reason, in a delivery of its own; `pending` is false for their keys afterwards. A handler
/// that reads again at once is refused — the recogniser still does not answer — and goes on with
/// that `"failed"` on the next turn of the loop, not inside the one that resumed it.
#[test]
fn a_hang_resumes_every_waiting_handler_once_with_failed() {
    let h = host_hanging_after(Duration::from_millis(100));
    let modules: Vec<Lua> = (1..=3).map(|idx| vm(&h, idx)).collect();
    hold_recognition(9851);
    for (i, lua) in modules.iter().enumerate() {
        let x = 9851 + i as i64;
        run(
            lua,
            &format!(
                "function poll()\n\
                   local r = host.ocr.recognize({{ {x}, 5, {x} + 30, 15 }}, {{ key = 'k' }})\n\
                   first = r.status .. ' ' .. tostring(r.error)\n\
                   pendingAfter = host.ocr.pending('k')\n\
                   local again = host.ocr.recognize({{ {x}, 25, {x} + 30, 35 }})\n\
                   second = again.status\n\
                 end"
            ),
        );
        let f: Function = lua.globals().get("poll").unwrap();
        let idx = i + 1;
        let d = mailbox::deliver(&*h, idx, lua, Event::Call { f, args: MultiValue::new(), input: false, what: "timer" });
        assert_eq!(d, mailbox::Delivered::Parked, "module {idx} waits");
    }
    wait_recognising(9851);
    let until = Instant::now() + Duration::from_secs(10);
    while !modules.iter().all(|l| yes(l, "return first ~= nil")) {
        assert!(Instant::now() < until, "the hang never answered");
        reads::fire(&*h);
        std::thread::sleep(Duration::from_millis(2));
    }
    for lua in &modules {
        assert!(yes(lua, "return string.find(first, 'failed the text recogniser has not answered a region for', 1, true) == 1"));
        assert!(yes(lua, "return pendingAfter == false"));
        assert!(yes(lua, "return second == nil"), "the read asked again is not answered inside the turn that resumed it");
    }
    reads::fire(&*h);
    for lua in &modules {
        assert!(yes(lua, "return second == 'failed'"), "answered on the next turn");
    }
    release(9851);
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
    assert_eq!(h.tasks.len(), 0);
}

/// A key reads and speaks, and a new press ends the one before it while that one still waits.
const KEY_EXAMPLE: &str = r#"
local current = nil
host.hotkey.register("Ctrl+Alt+R", function()
  local w = host.window.active()
  if not w then return end
  if current then task.cancel(current) end -- a press while the last one still waits ends it
  current = task.run(function()
    local r = host.ocr.recognize({ region = { window = w, fraction = { 0.1, 0.02, 0.5, 0.06 } } })
    -- From here on, other code may have run.
    if r.status ~= "text" then return end -- blank, none, failed: nothing to say
    local line = r.text:gsub("%s+", " ")
    host.speech.output(line, { interrupt = true })
  end)
end)
"#;

/// Two fields from one picture every 120 ms, a change said only, one read at a time, and the same
/// window checked after the wait.
const POLL_EXAMPLE: &str = r#"
local NOTE, PITCH = { 0.10, 0.02, 0.18, 0.05 }, { 0.20, 0.02, 0.28, 0.05 }
local watching, last = nil, nil
host.timer.every(120, function()
  if watching and task.alive(watching) then return end -- the last read is still out
  local w = host.window.active()
  if not w then return end
  watching = task.run(function()
    local r = host.ocr.recognize({ { window = w, fraction = NOTE }, { window = w, fraction = PITCH } })
    local front = host.window.active()
    if not (front and front.id == w.id) then return end -- another window is in front
    if r[1].status ~= "text" or r[2].status ~= "text" then return end
    local said = (r[1].text .. ", " .. r[2].text):gsub("%s+", " ")
    if said ~= last then
      last = said
      host.speech.output(said, { interrupt = true })
    end
  end)
end)
"#;

/// The scripted world of the two examples: a window in front, a hotkey and the speech.
fn example_world(lua: &Lua) {
    run(
        lua,
        r#"
        said = {}
        hotkeys = {}
        front = { id = 7, title = "Synth", client = { x = 9500, y = 0, w = 1000, h = 500 } }
        host.window = { active = function() return front end }
        host.hotkey = { register = function(spec, f) hotkeys[spec] = f end }
        host.speech = { output = function(text, opts) said[#said + 1] = text end }
        "#,
    );
}

/// The first example: a key reads and speaks, and a new press ends the one before it while that
/// one still waits — its read withdrawn, nothing of it said.
#[test]
fn example_a_key_reads_and_speaks() {
    let h = host();
    let lua = vm(&h, 1);
    example_world(&lua);
    run(&lua, KEY_EXAMPLE);
    hold(9600);
    {
        let _key = enter_priority(Priority::Interactive);
        run(&lua, r#"hotkeys["Ctrl+Alt+R"]()"#);
        run(&lua, r#"hotkeys["Ctrl+Alt+R"]()"#);
    }
    release(9600);
    settle(&h);
    assert!(yes(&lua, r#"return #said == 1 and said[1] == "9600,10""#), "{:?}", lua.load("return table.concat(said, ' | ')").eval::<String>());
    assert_eq!(h.tasks.len(), 0);
}

/// The second example: a poll with one read at a time — the next tick does nothing while one
/// waits — that speaks a change only, and only while the same window is in front.
#[test]
fn example_a_poll_with_one_read_at_a_time() {
    let h = host();
    let lua = vm(&h, 1);
    example_world(&lua);
    run(&lua, POLL_EXAMPLE);
    let t0 = Instant::now();
    hold(9600);
    fire_timers(&h, t0 + Duration::from_millis(130));
    assert_eq!(h.tasks.len(), 1);
    fire_timers(&h, t0 + Duration::from_millis(260));
    assert_eq!(h.tasks.len(), 1, "a second read while the first is out");
    release(9600);
    settle(&h);
    fire_timers(&h, t0 + Duration::from_millis(390));
    settle(&h);
    assert!(yes(&lua, r#"return #said == 1 and said[1] == "9600,10, 9700,10""#), "{:?}", lua.load("return table.concat(said, ' | ')").eval::<String>());
    // Another window in front when the answer comes: nothing said, though the value is new.
    run(&lua, "front = { id = 8, title = 'Other', client = { x = 9800, y = 0, w = 1000, h = 500 } }");
    fire_timers(&h, t0 + Duration::from_millis(520));
    run(&lua, "front = { id = 9, title = 'Third', client = { x = 9800, y = 0, w = 1000, h = 500 } }");
    settle(&h);
    assert!(yes(&lua, "return #said == 1"), "{:?}", lua.load("return table.concat(said, ' | ')").eval::<String>());
    // The same window still in front when it comes: the new value is said.
    run(&lua, "front = { id = 8, title = 'Other', client = { x = 9800, y = 0, w = 1000, h = 500 } }");
    fire_timers(&h, t0 + Duration::from_millis(650));
    settle(&h);
    assert!(
        yes(&lua, r#"return #said == 2 and said[2] == "9900,10, 10000,10""#),
        "{:?}",
        lua.load("return table.concat(said, ' | ')").eval::<String>()
    );
}
