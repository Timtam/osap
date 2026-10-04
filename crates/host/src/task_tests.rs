//! Tasks and their two waits, against a real Luau VM and the real read service over a fake
//! recogniser: every rule of task.rs.
//!
//! No module can start a task — the host table has no entry for it — so these tests reach the
//! machinery through its test-only entry, `task::table`, which each VM here has as the global
//! `task`: `task.run(fn)`, `task.cancel(n)`, `task.alive(n)`.
//!
//! The holder below stands where the host's `Shared` stands — the service, the reads waiting on
//! the event loop, the tasks, the timers, which VM each module runs and whether it is enabled —
//! and each test builds module VMs over it with the host's own bindings: `host.ocr.read`,
//! `pending`, `recognize` and `recognizeMany` (ocr/lua.rs and the shim), and `host.timer`
//! (timers.rs). Only the blocking legacy call is the holder's: it answers at once, or raises the
//! message that is to be raised once the blocking call is gone (`Host::raise`). The tick is
//! `fire`: due timers, then the readings, as the loop runs them.
//!
//! The fake recogniser reads every region as "x,y" — its corner — and can be held: a capture of
//! a region whose x a test `hold`s waits until it lets go, and so does a recognition of one whose
//! x it `hold_recognition`s; so no test times a picture, it holds it. Every test reads at x
//! coordinates of its own, since these lists are shared by the tests running beside it.
//!
//! The test's own thread stands where the event loop stands, and is marked as the loop's for as
//! long as the holder lives (`loop_guard`); every function of the fake recogniser asks the guard
//! first, as the platforms' do. So in a debug build a test that brought a capture or a
//! recognition onto the loop fails — only the blocking legacy call may, inside its scope.

use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, Table, Value};

use crate::backend::{CaptureSource, OcrLine, OcrText, OcrWord, OcrWorker, Recognise};
use crate::image_search::VmOwner;
use crate::ocr::lang::Languages;
use crate::ocr::lua::{self as reads, Delivered, OcrState, ReadHost};
use crate::ocr::service::{Service, ShutdownHandle};
use crate::ocr::types::{current_priority, enter_priority, Priority};
use crate::task::{self, Case, Tasks};
use crate::timers::{self, Timers};

// ── The fake recogniser ──────────────────────────────────────────────────────────────────────

/// The fake's pixels: the regions it was asked to photograph.
type Fake = Vec<(i32, i32, i32, i32)>;

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

fn worker() -> OcrWorker<Fake> {
    OcrWorker {
        present: true,
        init_thread: |_| {},
        warm_up: |_| {},
        capture: fake_capture,
        recognise: fake_recognise,
        languages: || Languages { available: vec!["en-US".into()], fast: vec![], preferred: vec!["en-US".into()] },
        frames: |regions, _, _| {
            crate::loop_guard::off_loop("the fake snapshot round");
            regions.iter().map(|_| Err("no snapshots here".to_string())).collect()
        },
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
    timers: Timers,
    gens: RefCell<HashMap<usize, u64>>,
    enabled: RefCell<Vec<bool>>,
    epoch: Cell<u64>,
    /// (module, context, message) of every error reported.
    errors: RefCell<Vec<(usize, String, String)>>,
    /// The legacy call raises its message, as it is to once the blocking call is gone, instead of
    /// answering.
    raise: Cell<bool>,
    /// (module, call, the shim's word for where) of every legacy call.
    legacy: RefCell<Vec<(usize, String, String)>>,
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
    fn read_source(&self, _: &Lua, _: (i32, i32, i32, i32)) -> CaptureSource {
        CaptureSource::Standard
    }
    fn screen_size(&self) -> (i32, i32) {
        (1920, 1080)
    }
}

fn host() -> Rc<Host> {
    let (ocr, stop) = Service::spawn(worker());
    Rc::new(Host {
        ocr,
        stop,
        state: OcrState::default(),
        tasks: Tasks::default(),
        timers: Timers::default(),
        gens: RefCell::new(HashMap::new()),
        enabled: RefCell::new(Vec::new()),
        epoch: Cell::new(0),
        errors: RefCell::new(Vec::new()),
        raise: Cell::new(false),
        legacy: RefCell::new(Vec::new()),
        _loop: crate::loop_guard::mark_for_a_test(),
    })
}

/// VM generations, process-wide as the host's are.
static NEXT_GEN: AtomicU64 = AtomicU64::new(1000);

/// A fresh VM for module `idx` — a new generation of it, enabled — with the host's own read, wait
/// and timer bindings over `h`, and for the tests: the machinery's test-only entry as the global
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
    ocr.set(
        "read",
        lua.create_function(move |lua, (what, opts, cb): (Value, Value, Value)| reads::read(&*hh, lua, idx, what, opts, cb))
            .unwrap(),
    )
    .unwrap();
    let hh = h.clone();
    ocr.set("pending", lua.create_function(move |lua, key: Value| reads::pending(&*hh, lua, idx, key)).unwrap()).unwrap();
    let hh = h.clone();
    let legacy = lua
        .create_function(move |lua, (name, opts, why): (String, Value, String)| {
            hh.legacy.borrow_mut().push((idx, name.clone(), why.clone()));
            if hh.raise.get() {
                return Ok((false, Value::String(lua.create_string(task::wait_message(&name, Case::of(&why)))?)));
            }
            // `{ mistake = true }`: a call the real one raises for, reading nothing.
            let mistake = matches!(&opts, Value::Table(t) if t.raw_get::<bool>("mistake").unwrap_or(false));
            let t = task::legacy(&*hh, lua, idx, &name, &why, || {
                if mistake {
                    return Err(mlua::Error::runtime(format!("{name}: opts.region is neither form")));
                }
                // On the loop, inside the legacy call's scope: the one place the guard lets be.
                crate::loop_guard::off_loop("the fake legacy call");
                let t = lua.create_table()?;
                t.set("text", "held the loop")?;
                t.set("skipped", false)?;
                Ok(t)
            })?;
            Ok((true, Value::Table(t)))
        })
        .unwrap();
    let waits = task::waits(&lua, idx, h.clone(), legacy).unwrap();
    ocr.set("recognize", waits.get::<Function>("recognize").unwrap()).unwrap();
    ocr.set("recognizeMany", waits.get::<Function>("recognizeMany").unwrap()).unwrap();
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

fn fire_timers(h: &Host, at: Instant) {
    h.timers.fire_due(
        at,
        |idx| h.module_enabled(idx),
        || h.bump_epoch(),
        |idx, e| h.report_error(idx, "timer", e),
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
/// the delivery with the reading `read` would hand a callback, `time` and `inputEpoch` included.
/// `recognizeMany` answers a list in the order of its regions.
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
          local m = host.ocr.recognizeMany({ regions = { { 9002, 5, 9032, 15 }, { 9003, 6, 9033, 16 } } })
          order[#order + 1] = "many " .. #m .. " " .. m[1].text .. " " .. m[2].text
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

/// `pcall(host.ocr.recognize, opts)` in a task waits and answers `true, reading` — VPS Avenger's
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
    assert!(h.legacy.borrow().is_empty(), "nothing went the blocking way");
}

// ── Where a task cannot wait ─────────────────────────────────────────────────────────────────

/// The places a task cannot stop take the old blocking call: a metamethod, a sort comparator, a
/// gsub function, a for loop's iterator, Lua the host itself calls back (an onActivate), and a
/// coroutine the module made — and outside every task, as ever. Each says which, and its first
/// time per module is logged.
#[test]
fn where_a_task_cannot_wait_the_blocking_call_runs_and_says_why() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        local R = { region = { 9021, 5, 9051, 15 } }
        outside = host.ocr.recognize(R).text
        task.run(function()
          local meta = setmetatable({}, { __index = function() return host.ocr.recognize(R).text end })
          got = { meta = meta.x }
          table.sort({ 3, 1, 2 }, function(a, b) got.sort = host.ocr.recognize(R).text; return a < b end)
          local _ = ("ab"):gsub(".", function(c) got.gsub = host.ocr.recognize(R).text; return c end)
          for v in function(_, last) if last then return nil end got.iter = host.ocr.recognize(R).text; return 1 end do end
          xpcall(error, function(e) got.handler = host.ocr.recognize(R).text; return e end, "raised")
          table.foreach({ 1 }, function() got.foreach = host.ocr.recognize(R).text end)
          table.foreachi({ 1 }, function() got.foreachi = host.ocr.recognize(R).text end)
          hostCall(function() got.called = host.ocr.recognize(R).text end)
          got.own = coroutine.wrap(function() return host.ocr.recognize(R).text end)()
          got.waited = host.ocr.recognize(R).text
        end)
        "#,
    );
    settle(&h);
    assert!(yes(
        &lua,
        r#"return outside == "held the loop" and got.meta == "held the loop" and got.sort == "held the loop"
          and got.gsub == "held the loop" and got.iter == "held the loop" and got.called == "held the loop"
          and got.handler == "held the loop" and got.foreach == "held the loop" and got.foreachi == "held the loop"
          and got.own == "held the loop" and got.waited == "9021,5""#
    ));
    let whys: Vec<String> = h.legacy.borrow().iter().map(|l| l.2.clone()).collect();
    assert_eq!(whys[0], "none");
    assert!(whys[1..whys.len() - 1].iter().all(|w| w == "cannot-wait"), "{whys:?}");
    assert_eq!(whys.last().map(String::as_str), Some("foreign"));
    for case in [Case::Outside, Case::CannotWait, Case::OwnCoroutine] {
        assert!(h.tasks.legacy_said("m1", case), "{case:?} not said");
    }
    // Keyed by the module's id, and news again once forgotten — as a reload, a disable or enable
    // and a rolled-back hot-load forget it (`forget_error_repeats`).
    assert!(!h.tasks.legacy_said("m2", Case::Outside));
    h.tasks.forget_legacy_said("m1");
    assert!(!h.tasks.legacy_said("m1", Case::Outside));
    run(&lua, "host.ocr.recognize({ region = { 9022, 5, 9052, 15 } })");
    assert!(h.tasks.legacy_said("m1", Case::Outside), "written again after it was forgotten");
    // Once per module and case; every call counted for the summary at exit.
    let summary = h.tasks.legacy_summary();
    assert_eq!(summary.len(), 1);
    let calls = h.legacy.borrow().len();
    assert!(calls >= 12, "every place called it: {calls}");
    assert!(
        summary[0].starts_with(&format!("[m1] host.ocr.recognize and recognizeMany held the event loop {calls} time(s), ")),
        "{summary:?}"
    );
}

/// A first blocking call that raises — a mistake in its arguments, which reads nothing — writes no
/// line, so the module's first call that answers still says how long it held the loop; both are
/// counted for the summary. The raise's traceback names the shim `host.ocr`, not a task.
#[test]
fn a_blocking_call_that_raises_writes_no_line_and_the_next_one_does() {
    let h = host();
    let lua = vm(&h, 1);
    run(&lua, "ok, err = pcall(host.ocr.recognize, { mistake = true }); err = tostring(err)");
    assert!(yes(&lua, "return ok == false"));
    let err: String = lua.globals().get("err").unwrap();
    assert!(err.contains("opts.region is neither form") && err.contains("host.ocr:"), "{err}");
    assert!(!err.contains("task_shim"), "{err}");
    assert!(!h.tasks.legacy_said("m1", Case::Outside), "a raise was logged as the first call");
    run(&lua, "host.ocr.recognize({ region = { 9025, 5, 9055, 15 } })");
    assert!(h.tasks.legacy_said("m1", Case::Outside), "the first call that answered was not logged");
    let summary = h.tasks.legacy_summary();
    assert!(summary[0].starts_with("[m1] host.ocr.recognize and recognizeMany held the event loop 2 time(s), "), "{summary:?}");
}

/// Once the blocking call is gone, each of the three places raises its own message, and the raise
/// names the module's own line — played here with the switch the scripted host plays it with.
#[test]
fn where_it_cannot_wait_the_later_release_raises_naming_the_callers_line() {
    let h = host();
    h.raise.set(true);
    let lua = vm(&h, 1);
    run(
        &lua,
        "local R = { region = { 9031, 5, 9061, 15 } }\n\
         ok, outside = pcall(function()\n\
           return host.ocr.recognizeMany({ regions = { R.region } })\n\
         end)\n\
         task.run(function()\n\
           local meta = setmetatable({}, { __index = function()\n\
             return host.ocr.recognize(R)\n\
           end })\n\
           ok2, inMeta = pcall(function() return meta.x end)\n\
           ok3, inOwn = pcall(coroutine.wrap(function()\n\
             return host.ocr.recognize(R)\n\
           end))\n\
         end)\n",
    );
    let get = |name: &str| lua.globals().get::<String>(name).unwrap();
    assert!(yes(&lua, "return ok == false and ok2 == false and ok3 == false"));
    for (name, line, message) in [
        ("outside", 3, task::wait_message("host.ocr.recognizeMany", Case::Outside)),
        ("inMeta", 7, task::wait_message("host.ocr.recognize", Case::CannotWait)),
        ("inOwn", 11, task::wait_message("host.ocr.recognize", Case::OwnCoroutine)),
    ] {
        let raised = get(name);
        assert!(raised.contains(&format!("mod.luau:{line}: {message}")), "{name}: {raised}");
    }
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

/// The shim's `start`, taken off the stack with `debug.info` while it reads its options, and
/// called by module code: outside its place it raises and queues nothing; called inside the
/// task's own wait, the task's own wait then raises, and the stray read is withdrawn when the
/// task ends — no reading is left.
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
          if k == "region" then
            stolen = stealStart()
            nested = { pcall(stolen, "host.ocr.recognize", { region = { 9091, 5, 9121, 15 } }, {}) }
          end
          return nil
        end })
        task.run(function()
          okWait, errWait = pcall(host.ocr.recognize, sneaky)
          errWait = tostring(errWait)
        end)
        ok, err = pcall(stolen, "host.ocr.recognize", { region = { 9092, 5, 9122, 15 } }, {})
        err = tostring(err)
        "#,
    );
    assert!(yes(&lua, "return type(stolen) == 'function' and debug.info(stolen, 's') == '[C]'"), "start was not found on the stack");
    assert!(yes(&lua, "return ok == false and okWait == false and nested[1] == true"));
    for name in ["err", "errWait"] {
        let e: String = lua.globals().get(name).unwrap();
        assert!(e.contains("the host's wait was called from outside its place"), "{name}: {e}");
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
    assert!(e.contains(task::FOREIGN_YIELD), "{e}");
    let errs = errors(&h);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].1 == "task" && errs[0].2.contains(task::FOREIGN_YIELD), "{errs:?}");
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

/// With a `key`, the newest wins as for `read`: a wait whose read had not been recognised yet when
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
          local r = host.ocr.recognize({ region = { 9121, 5, 9151, 15 }, key = "field" })
          first = { status = r.status, newer = r.newer, words = #r.words, time = r.time, epoch = r.inputEpoch }
        end)
        task.run(function()
          second = host.ocr.recognize({ region = { 9122, 5, 9152, 15 }, key = "field" }).status
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
    run(&lua, r#"task.run(function() third = host.ocr.recognize({ region = { 9123, 5, 9153, 15 }, key = "row" }) end)"#);
    wait_recognising(9123);
    run(&lua, r#"task.run(function() fourth = host.ocr.recognize({ region = { 9124, 5, 9154, 15 }, key = "row" }) end)"#);
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
          and okBad == false and string.find(bad, "host.ocr.recognize: opts.region is neither", 1, true) ~= nil"#
    ));
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// `read`'s limit holds for waits: a module's seventeenth wait pushes out its oldest one that has
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

/// Nothing to read is still a wait: `recognizeMany` of an empty list answers an empty list, and
/// empty corners a `"failed"` reading, on a later turn of the loop.
#[test]
fn an_empty_wait_is_answered_on_a_later_turn() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        task.run(function()
          many = #host.ocr.recognizeMany({ regions = {} })
          local e = host.ocr.recognize({ region = { 9205, 5, 9205, 15 } })
          empty = { status = e.status, error = e.error, time = e.time }
        end)
        "#,
    );
    assert!(yes(&lua, "return many == nil"), "answered inside the call");
    settle(&h);
    assert!(yes(
        &lua,
        r#"return many == 0 and empty.status == "failed" and empty.time == nil
          and string.find(empty.error, "empty region: x2 must be greater than x1", 1, true) ~= nil"#
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
    assert!(h.legacy.borrow().is_empty(), "nothing went the blocking way");
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
    let fired = crate::dispatch_initial(&lua, Some(&win), false, &|| {}).unwrap();
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

/// `host.ocr.pending(key)`: true once a read or a task's wait with the key was asked, false in the
/// read's own callback and in the task it resumes, and after a disable; a key that is not a
/// non-empty string raises.
#[test]
fn pending_says_whether_a_read_or_a_wait_with_the_key_is_out() {
    let h = host();
    let lua = vm(&h, 1);
    run(
        &lua,
        r#"
        host.ocr.read({ 9151, 5, 9181, 15 }, { key = "a" }, function() inCallback = host.ocr.pending("a") end)
        afterRead = host.ocr.pending("a")
        task.run(function()
          host.ocr.recognize({ region = { 9152, 5, 9182, 15 }, key = "b" })
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
    run(&lua, r#"task.run(function() host.ocr.recognize({ region = { 9153, 5, 9183, 15 }, key = "d" }) end)"#);
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
    local r = host.ocr.recognizeMany({ regions = {
      { window = w, fraction = NOTE }, { window = w, fraction = PITCH } } })
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
