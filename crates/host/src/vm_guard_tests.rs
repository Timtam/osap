//! The guard against real Luau VMs.
//!
//! **Memory (G1):** a VM's limit as the sum over the code in it, an uncaught memory error as a stop
//! and a caught one as the module's own business, entries refused once a VM is stopped, the end of
//! the outermost entry as the moment the trips go to the hook, the trip's words for a call that ran
//! inside another, and mlua's errors as the guard reads them. Small limits — a few MiB — so nothing
//! here fills 256.
//!
//! **Time (G2):** the watchdog's look as a pure function of fake clocks — once per round, never
//! idle, its baseline at first sight, only VMs on the stack written — the interrupt armed and taken
//! by the VM that runs, a quick callee inside another's callback letting it pass, the samples that
//! find the loop and not the helper it calls, nothing escaping the stop, the library rule and where
//! it does not apply (the wall clock, a host call, the host's own Luau), a load stopping nobody
//! else, the frames and the host call named, the `[cpu]` lines, one test where the real watchdog
//! thread stops a loop on this one, and one on the real processor clock. No test waits for 2 s: the
//! clocks are fake, or a Rust function the script calls arms the round itself (`trip()`); a stop
//! takes the samples' 10 ms of real time.
//!
//! **mlua pinned (`mod mlua_pin`):** what the guard relies on mlua for, held against mlua itself,
//! so another mlua fails here in CI before it ships.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua};

use crate::vm_guard::{self, Budgets, Cause, Clock, Clocks, Entry, EntryKind, Guard, Trip, VmInfo, STOPPED};

/// A VM of `guard` for module `idx`, described with `code` as (id, MiB), and loaded.
fn vm(guard: &Arc<Guard>, idx: usize, id: &str, code: &[(&str, u32)]) -> Lua {
    let lua = vm_guard::new_vm(guard);
    vm_guard::describe_for_test(&lua, idx, id, code);
    vm_guard::loaded(&lua);
    lua
}

/// Collects the trips the exit hook is handed on this thread, until dropped.
struct Hooked(Rc<RefCell<Vec<Vec<Trip>>>>);

fn hook() -> Hooked {
    let got: Rc<RefCell<Vec<Vec<Trip>>>> = Rc::new(RefCell::new(Vec::new()));
    let g = got.clone();
    vm_guard::set_exit_hook(Rc::new(move |t| g.borrow_mut().push(t)));
    Hooked(got)
}

impl Drop for Hooked {
    fn drop(&mut self) {
        vm_guard::clear_exit_hook();
    }
}

impl Hooked {
    fn trips(&self) -> Vec<Trip> {
        self.0.borrow().iter().flatten().cloned().collect()
    }
    fn handed(&self) -> usize {
        self.0.borrow().len()
    }
}

/// A function that keeps strings of about 1 MiB in a table until the VM refuses one. Each string
/// different: Luau keeps one copy of equal strings, and a table of the same string grows only by
/// its slots — the VM would then run out of memory on the garbage of `string.rep`, if at all.
const FILL: &str = "return function() local t = {} while true do t[#t + 1] = string.rep('x', 1048000) .. #t end end";

fn func(lua: &Lua, code: &str) -> Function {
    lua.load(code).eval().expect("the test's function")
}

/// `f()` in `lua` as an entry of `kind` named `what`: its result, and whether the VM is stopped.
fn call(lua: &Lua, kind: EntryKind, what: &'static str, f: &Function) -> (mlua::Result<()>, bool) {
    let entry = Entry::enter(lua, kind, what).expect("the VM is not stopped");
    let r = f.call::<()>(());
    entry.finish(&r);
    let stopped = entry.stopped();
    (r, stopped)
}

/// The limit mlua holds for `lua`, in bytes: read by setting it, and set back.
fn limit_of(lua: &Lua) -> usize {
    let limit = lua.set_memory_limit(0).expect("a limit");
    lua.set_memory_limit(limit).expect("set back");
    limit
}

// --- Fake clocks and the trip -----------------------------------------------------------------

/// Clocks a test sets by hand: wall and processor time in nanoseconds.
#[derive(Default)]
struct Fake {
    wall: AtomicU64,
    cpu: AtomicU64,
}

impl Fake {
    fn advance(&self, wall: Duration, cpu: Duration) {
        self.wall.fetch_add(wall.as_nanos() as u64, Ordering::SeqCst);
        self.cpu.fetch_add(cpu.as_nanos() as u64, Ordering::SeqCst);
    }
}

struct FakeClocks(Arc<Fake>);

impl Clocks for FakeClocks {
    fn wall_ns(&self) -> u64 {
        self.0.wall.load(Ordering::SeqCst)
    }
    fn loop_cpu(&self) -> Option<Duration> {
        Some(Duration::from_nanos(self.0.cpu.load(Ordering::SeqCst)))
    }
}

/// A guard with the application's budgets and clocks the test moves, installed on this thread.
fn fake_guard() -> (Arc<Guard>, Arc<Fake>) {
    let fake = Arc::new(Fake::default());
    let guard = Guard::with(Budgets::APP, Box::new(FakeClocks(fake.clone())));
    vm_guard::install(&guard);
    (guard, fake)
}

/// Installs `trip()` in `lua`: it arms a round for the slice that runs now, as the watchdog would
/// past the budget, so the script's next safepoint takes the stop.
fn give_trip(lua: &Lua, guard: &Arc<Guard>) {
    let g = guard.clone();
    let trip = lua
        .create_function(move |_, ()| {
            g.arm_now(Clock::Cpu);
            Ok(())
        })
        .unwrap();
    lua.globals().set("trip", trip).unwrap();
}

/// Installs `age(ms)` in `lua`: the fake clocks move on by `ms` on both, as a loop that runs that
/// long would move them, and the watchdog looks — which arms again a VM that let a round pass.
fn give_clock(lua: &Lua, guard: &Arc<Guard>, fake: &Arc<Fake>) {
    let (g, f) = (guard.clone(), fake.clone());
    let age = lua
        .create_function(move |_, ms: u64| {
            f.advance(Duration::from_millis(ms), Duration::from_millis(ms));
            g.poll();
            Ok(())
        })
        .unwrap();
    lua.globals().set("age", age).unwrap();
}

/// Runs `code` in `lua` as a hotkey's handler entry: its result and whether the VM is stopped.
fn run(lua: &Lua, code: &str) -> (mlua::Result<mlua::Value>, bool) {
    let entry = Entry::enter(lua, EntryKind::Handler, "hotkey").expect("not stopped");
    let r = lua.load(code).set_name("=probe").eval::<mlua::Value>();
    entry.finish(&r);
    let stopped = entry.stopped();
    (r, stopped)
}

fn is_stop(r: &mlua::Result<mlua::Value>) -> bool {
    matches!(r, Err(e) if vm_guard::is_stop_error(e) || e.to_string().contains(STOPPED))
}

// --- G1: memory --------------------------------------------------------------------------------

#[test]
fn a_new_vm_has_the_default_limit_at_once() {
    let guard = Guard::new();
    let lua = vm_guard::new_vm(&guard);
    assert_eq!(limit_of(&lua), 256 << 20, "a VM nothing was described for is limited too");
    assert_eq!(vm_guard::info(&lua), Some(VmInfo::default()));
    assert!(!vm_guard::armed(&lua), "and runs with no interrupt");
}

#[test]
fn the_limit_is_the_sum_over_the_code_in_the_vm() {
    let guard = Guard::new();
    let lua = vm(&guard, 0, "com.x.b", &[("com.x.b", 300), ("com.x.lib", 256)]);
    assert_eq!(limit_of(&lua), 556 << 20);
    let info = vm_guard::info(&lua).unwrap();
    assert_eq!(info.limit_mib(), 556);
    assert_eq!(
        vm_guard::limit_line(&info),
        "[com.x.b] memory limit 556 MiB: 300 of its own (set in its module.toml), 256 for com.x.lib"
    );
    // From the manifests, as `populate_vm` builds it: each code dependency once, with its own
    // amount, declared or not.
    let manifest = module_manifest::ModuleManifest::parse(
        "id = \"com.x.k\"\nname = \"K\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    let dep = |id: &str, mib: u32| crate::capture_source::CodeDep {
        id: id.to_string(),
        name: format!("{id} name"),
        memory_mib: mib,
        memory_declared: mib != 256,
        entry: std::path::PathBuf::new(),
        entry_rel: String::new(),
        screen: Default::default(),
        reads_screen: false,
        deps: Vec::new(),
        order: 0,
        depth: 0,
    };
    let info = VmInfo::of(7, &manifest, &[dep("com.platform.overlay", 256), dep("com.platform.daw-hosts", 256), dep("com.x.big", 1024)]);
    assert_eq!(info.module, Some(7));
    assert_eq!(info.limit_mib(), 256 + 256 + 256 + 1024);
    assert_eq!(
        vm_guard::limit_line(&info),
        "[com.x.k] memory limit 1792 MiB: 256 of its own, 256 for com.platform.overlay, 256 for \
         com.platform.daw-hosts, 1024 for com.x.big (set in its module.toml)"
    );
    // And it holds: 7 MiB in all, two modules' worth.
    let small = vm(&guard, 1, "com.x.s", &[("com.x.s", 4), ("com.x.l", 3)]);
    let keep = |n: u32| {
        small
            .load(format!("local t = {{}} for i = 1, {n} do t[i] = string.rep(string.char(64 + i), 1048000) end return #t"))
            .eval::<u32>()
    };
    assert_eq!(keep(5).expect("5 MiB of 7"), 5);
    small.gc_collect().unwrap();
    assert!(vm_guard::is_memory_error(&keep(9).expect_err("9 MiB of 7")));
}

#[test]
fn an_uncaught_memory_error_stops_the_vm_and_a_caught_one_does_not() {
    let hooked = hook();
    let guard = Guard::new();
    let lua = vm(&guard, 3, "com.x.m", &[("com.x.m", 4)]);
    // Caught by the module's own pcall: its business. The VM goes on, and nobody hears of it.
    let caught = func(&lua, &format!("local fill = ({}) return function() local ok = pcall(fill) assert(not ok) end", FILL.trim_start_matches("return ")));
    let (r, stopped) = call(&lua, EntryKind::Handler, "hotkey", &caught);
    assert!(r.is_ok() && !stopped, "{r:?}");
    assert!(hooked.trips().is_empty());
    // And it really goes on: the guard collected the garbage the loop left as the entry ended, so
    // the next callback's allocations fit — three more strings of a MiB, in a new entry.
    assert!(lua.used_memory() < 2 << 20, "{} bytes after the entry", lua.used_memory());
    let more = func(&lua, "return function() local k = {} for i = 1, 3 do k[i] = string.rep('y', 1048000) end end");
    let (r, stopped) = call(&lua, EntryKind::Handler, "hotkey", &more);
    assert!(r.is_ok() && !stopped, "the next callback fits: {r:?}");
    assert_eq!(lua.load("return 1 + 1").eval::<i32>().unwrap(), 2);

    // Not caught: a stop, noted for the end of the slice — this entry is the outermost.
    let fill = func(&lua, FILL);
    let (r, stopped) = call(&lua, EntryKind::Handler, "hotkey", &fill);
    assert!(vm_guard::is_memory_error(&r.unwrap_err()));
    assert!(stopped && vm_guard::stopped(&lua));
    assert!(vm_guard::armed(&lua), "armed: any Luau that runs in it now raises");
    assert_eq!(hooked.handed(), 1, "handed over once, when the outermost entry ended");
    let trips = hooked.trips();
    assert_eq!(trips.len(), 1);
    let t = &trips[0];
    assert_eq!((t.vm.module, t.vm.id.as_str(), t.kind, t.what.as_str()), (Some(3), "com.x.m", EntryKind::Handler, "hotkey"));
    assert!(!t.loading && t.outer.is_none() && t.traceback.is_none(), "{t:?}");
    assert_eq!(t.group.len(), 1, "a memory stop takes only its own VM");
    let Cause::Memory { limit_mib, used } = t.cause else { panic!("{t:?}") };
    assert_eq!(limit_mib, 4);
    assert!(used > 3 << 20 && used <= 4 << 20, "{used} bytes in use");
    assert_eq!(vm_guard::last_trip(&lua).as_ref(), Some(t));

    // Every entry is refused from now on, and a failure outside one is the stop's too.
    assert!(Entry::enter(&lua, EntryKind::Plain, "arbiter onActivate").is_err());
    assert!(vm_guard::note_failure(&lua, EntryKind::Handler, "hotkey", &mlua::Error::runtime("x")));

    // Its garbage goes at once, by the trip's serial — a collection runs no Luau, armed or not.
    vm_guard::collect_garbage(t.serial);
    assert!(lua.used_memory() < 1 << 20, "{} bytes after the collection", lua.used_memory());

    // Turned on again: the same VM takes calls again, unarmed.
    vm_guard::clear_module(&guard, 3);
    assert!(!vm_guard::stopped(&lua) && !vm_guard::armed(&lua));
    let two = func(&lua, "return function() return 1 + 1 end");
    let (r, stopped) = call(&lua, EntryKind::Handler, "hotkey", &two);
    assert!(r.is_ok() && !stopped);
}

#[test]
fn a_memory_error_through_a_rust_callback_is_one_too_and_keeps_its_traceback() {
    let hooked = hook();
    let guard = Guard::new();
    let lua = vm(&guard, 0, "com.x.r", &[("com.x.r", 4)]);
    let alloc = lua.create_function(|lua, n: usize| lua.create_string(vec![b'y'; n]).map(|_| ())).unwrap();
    lua.globals().set("alloc", alloc).unwrap();
    let f = func(&lua, "return function() local keep = {} for i = 1, 100 do keep[i] = string.rep('z', 1048000) alloc(1) end end");
    let (r, stopped) = call(&lua, EntryKind::Plain, "arbiter onActivate", &f);
    let e = r.unwrap_err();
    assert!(vm_guard::is_memory_error(&e), "{e:?}");
    assert!(stopped);
    let t = &hooked.trips()[0];
    // Either the string or the Rust call ran out first; through the call, mlua kept where.
    if matches!(e, mlua::Error::CallbackError { .. }) {
        assert!(t.traceback.as_deref().is_some_and(|tb| tb.contains("traceback")), "{t:?}");
    }
}

#[test]
fn the_trips_go_to_the_hook_when_the_outermost_entry_ends_and_name_the_call_they_ran_in() {
    let hooked = hook();
    let guard = Guard::new();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 4)]);
    let fill_b = func(&b, FILL);
    let seen_inside = Rc::new(Cell::new(usize::MAX));
    // A's hotkey calls into the host, which runs B's onDeactivate: B runs out of memory there.
    let (bb, h, seen) = (b.clone(), hooked.0.clone(), seen_inside.clone());
    let nested = a
        .create_function(move |_, ()| {
            let entry = Entry::enter(&bb, EntryKind::Plain, "arbiter onDeactivate").expect("B is not stopped yet");
            let r = fill_b.call::<()>(());
            entry.finish(&r);
            drop(entry);
            seen.set(h.borrow().len());
            Ok(())
        })
        .unwrap();
    a.globals().set("nested", nested).unwrap();
    let hotkey = func(&a, "return function() nested() return 1 end");
    let (r, a_stopped) = call(&a, EntryKind::Handler, "hotkey", &hotkey);
    assert!(r.is_ok() && !a_stopped, "A goes on: {r:?}");
    assert_eq!(seen_inside.get(), 0, "nothing is handed over while A's callback still runs");
    assert!(vm_guard::stopped(&b));
    let trips = hooked.trips();
    assert_eq!(trips.len(), 1, "{trips:?}");
    let t = &trips[0];
    assert_eq!((t.vm.id.as_str(), t.kind, t.what.as_str()), ("com.x.b", EntryKind::Plain, "arbiter onDeactivate"));
    let o = t.outer.as_ref().expect("it ran inside A's hotkey callback");
    assert_eq!((o.module, o.id.as_str(), o.name.as_str(), o.kind, o.what.as_str(), o.same_vm), (Some(0), "com.x.a", "Module com.x.a", EntryKind::Handler, "hotkey", false));
}

#[test]
fn a_stop_inside_its_own_vms_call_says_so_and_a_load_is_marked() {
    let hooked = hook();
    let guard = Guard::new();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 4)]);
    let fill = func(&a, FILL);
    let aa = a.clone();
    let own = a
        .create_function(move |_, ()| {
            let entry = Entry::enter(&aa, EntryKind::Plain, "settings onChange (volume)").expect("not stopped");
            let r = fill.call::<()>(());
            entry.finish(&r);
            Ok(())
        })
        .unwrap();
    a.globals().set("set", own).unwrap();
    let hotkey = func(&a, "return function() set() return 'ran on' end");
    let (r, stopped) = call(&a, EntryKind::Handler, "hotkey", &hotkey);
    assert!(stopped, "the VM is stopped");
    // Its outer callback, in the same VM, does not run on: the stopped VM raises at its next
    // safepoint, here the return — as the stop's error, or, in a VM still full, as Luau's own
    // out-of-memory while it raises.
    assert!(r.is_err(), "{r:?}");
    let t = &hooked.trips()[0];
    assert_eq!(t.what, "settings onChange (volume)");
    assert!(t.outer.as_ref().is_some_and(|o| o.same_vm && o.what == "hotkey"), "{t:?}");

    // A VM that has not finished loading: the trip says so, for the load to fail with it.
    let loading = vm_guard::new_vm(&guard);
    vm_guard::describe_for_test(&loading, 1, "com.x.l", &[("com.x.l", 4)]);
    let fill = func(&loading, FILL);
    let (_, stopped) = call(&loading, EntryKind::Load, "its entry file", &fill);
    assert!(stopped);
    let t = vm_guard::last_trip(&loading).unwrap();
    assert!(t.loading && t.kind == EntryKind::Load, "{t:?}");
}

#[test]
fn trips_wait_in_the_guard_without_a_hook_and_a_vm_no_guard_made_passes_through() {
    let guard = Guard::new();
    let lua = vm(&guard, 0, "com.x.n", &[("com.x.n", 4)]);
    let fill = func(&lua, FILL);
    let (_, stopped) = call(&lua, EntryKind::Handler, "timer", &fill);
    assert!(stopped);
    assert_eq!(guard.take_trips().len(), 1, "no hook on this thread: the trips stay in the guard");
    // A plain `Lua::new()` — the tests' VMs — is not the guard's: nothing refused, nothing noted.
    let bare = Lua::new();
    let _ = bare.set_memory_limit(4 << 20);
    let fill = func(&bare, FILL);
    let (r, stopped) = call(&bare, EntryKind::Handler, "timer", &fill);
    assert!(r.is_err() && !stopped && !vm_guard::stopped(&bare));
    assert!(Entry::enter(&bare, EntryKind::Plain, "x").is_ok());
}

#[test]
fn carriers_are_the_live_vms_that_run_a_modules_code() {
    let guard = Guard::new();
    let _lib = vm(&guard, 0, "com.x.lib", &[("com.x.lib", 256)]);
    let _a = vm(&guard, 1, "com.x.a", &[("com.x.a", 256), ("com.x.lib", 256)]);
    let b = vm(&guard, 2, "com.x.b", &[("com.x.b", 256), ("com.x.lib", 256)]);
    let names = |v: Vec<(String, String)>| v.into_iter().map(|(_, id)| id).collect::<Vec<_>>();
    assert_eq!(names(vm_guard::carriers_of(&guard, "com.x.lib")), ["com.x.a", "com.x.b"]);
    assert!(vm_guard::carriers_of(&guard, "com.x.a").is_empty());
    drop(b);
    assert_eq!(names(vm_guard::carriers_of(&guard, "com.x.lib")), ["com.x.a"], "a dropped VM leaves the registry");
}

/// mlua 0.11's errors as the guard reads them: a memory error, plain and inside every wrapper a
/// Rust callback or a context puts around it, and nothing else. A renamed or rewrapped variant in
/// another mlua fails here.
#[test]
fn the_memory_errors_the_guard_matches() {
    use std::sync::Arc as A;
    let mem = mlua::Error::MemoryError("not enough memory".into());
    assert!(vm_guard::is_memory_error(&mem));
    let wrapped = mlua::Error::CallbackError { traceback: "stack traceback:".into(), cause: A::new(mem.clone()) };
    assert!(vm_guard::is_memory_error(&wrapped));
    assert!(vm_guard::is_memory_error(&mlua::Error::WithContext { context: "c".into(), cause: A::new(wrapped) }));
    assert!(!vm_guard::is_memory_error(&mlua::Error::runtime("not enough memory")));
    // And a real limit gives the plain variant.
    let lua = Lua::new();
    lua.set_memory_limit(2 << 20).unwrap();
    let e = func(&lua, FILL).call::<()>(()).unwrap_err();
    assert!(matches!(e, mlua::Error::MemoryError(_)), "{e:?}");
}

// --- G2: the watchdog's look --------------------------------------------------------------------

/// Idle, nothing is armed; in a slice, nothing until a budget is spent, then once — not again at
/// the next look of the same slice — and a new slice starts from a fresh baseline.
#[test]
fn the_watchdog_arms_once_per_round_and_never_when_idle() {
    let _h = hook();
    let (guard, fake) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    fake.advance(Duration::from_secs(60), Duration::from_secs(30));
    assert!(guard.poll().armed.is_none(), "no slice: nothing to arm");

    let entry = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
    assert!(guard.poll().armed.is_none(), "the first look takes the baseline");
    fake.advance(Duration::from_millis(1999), Duration::from_millis(1999));
    assert!(guard.poll().armed.is_none(), "1 ms short of the budget");
    assert!(!vm_guard::armed(&lua));
    fake.advance(Duration::from_millis(1), Duration::from_millis(1));
    let note = guard.poll().armed.expect("at the budget");
    assert_eq!((note.clock, note.cpu, note.call), (Clock::Cpu, Duration::from_secs(2), None));
    assert!(vm_guard::armed(&lua), "the VM on the stack is armed");
    assert!(guard.poll().armed.is_none(), "one round per slice");
    assert!(vm_guard::armed(&lua), "and kept armed while the round waits");
    drop(entry);
    assert!(!vm_guard::armed(&lua), "the slice ended: switched off, the round cancelled");
    assert!(guard.take_trips().is_empty(), "no Luau ran: nobody took it");

    let entry = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
    let p = guard.poll();
    assert!(p.armed.is_none(), "a new slice: a fresh baseline");
    assert!(
        p.lines.contains(&("guard", vm_guard::returned_line(Some("com.x.a")))),
        "the round nobody took is said not to have stopped anything: {:?}",
        p.lines
    );
    assert_eq!(
        vm_guard::returned_line(Some("com.x.a")),
        "[com.x.a] the callback past its limit returned before it was stopped; nothing was stopped"
    );
    // Both lines, and the arm line, as module-runtime-and-lifecycle.md shows them.
    let doc = include_str!("../../../docs/module-runtime-and-lifecycle.md");
    let arm = vm_guard::arm_line(
        Some("com.example.game"),
        &vm_guard::ArmNote { clock: Clock::Cpu, cpu: Duration::from_millis(2003), wall: Duration::from_millis(2049), call: None, top: 0 },
    );
    for line in [arm, vm_guard::returned_line(Some("com.example.game"))] {
        assert!(doc.contains(&format!("[guard] {line}")), "not in the docs: {line}");
    }
    fake.advance(Duration::from_millis(500), Duration::from_millis(500));
    assert!(guard.poll().armed.is_none());
    drop(entry);
}

/// The processor time before a slice began is not the slice's: the baseline is taken at the
/// watchdog's first look at it. Nor is what the callback itself ran before that look — up to one
/// look of 50 ms — which is why the documented stop is 2.0 to about 2.1 s of processor time.
#[test]
fn the_cpu_baseline_is_taken_at_first_sight() {
    let _h = hook();
    let (guard, fake) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    fake.advance(Duration::ZERO, Duration::from_secs(5));
    let entry = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
    assert!(guard.poll().armed.is_none(), "5 s of processor time before the slice count for nothing");
    fake.advance(Duration::from_secs(1), Duration::from_secs(1));
    assert!(guard.poll().armed.is_none());
    drop(entry);

    // Inside the slice, before the first look: 40 ms the callback ran is not counted.
    let (guard, fake) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let _entry = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
    fake.advance(Duration::from_millis(40), Duration::from_millis(40));
    assert!(guard.poll().armed.is_none(), "the first look takes the baseline");
    fake.advance(Duration::from_millis(1990), Duration::from_millis(1990));
    assert!(guard.poll().armed.is_none(), "2.03 s since the entry, 1.99 s since the first look: not yet");
    fake.advance(Duration::from_millis(10), Duration::from_millis(10));
    let note = guard.poll().armed.expect("2.0 s since the first look");
    assert_eq!(note.cpu, Duration::from_secs(2));
}

/// The wall clock decides when the processor time does not: a loop that waits for the processor
/// behind other programs, or a slow host call.
#[test]
fn the_wall_budget_arms_when_the_cpu_does_not() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let entry = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
    guard.poll();
    fake.advance(Duration::from_secs(10), Duration::from_millis(500));
    let note = guard.poll().armed.expect("10 s on the wall clock");
    assert_eq!(note.clock, Clock::Wall);
    // The loop takes it, once it has sampled its stack.
    let r = lua.load("local n = 0 while true do n += 1 end return n").eval::<i32>();
    assert!(matches!(&r, Err(e) if vm_guard::is_stop_error(e)), "{r:?}");
    drop(entry);
    let t = &hooked.trips()[0];
    let Cause::Time { clock, cpu, wall } = t.cause else { panic!("{t:?}") };
    assert_eq!((clock, cpu, wall), (Clock::Wall, Duration::from_millis(500), Duration::from_secs(10)));
}

/// The watchdog writes only into VMs on the entry stack: the one that is not stays as it was.
#[test]
fn the_watchdog_writes_only_vms_on_the_stack() {
    let _h = hook();
    let (guard, fake) = fake_guard();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    let _entry = Entry::enter(&a, EntryKind::Handler, "hotkey").unwrap();
    guard.poll();
    fake.advance(Duration::from_secs(3), Duration::from_secs(3));
    assert!(guard.poll().armed.is_some());
    assert!(vm_guard::armed(&a) && !vm_guard::armed(&b));
}

// --- G2: the stop -------------------------------------------------------------------------------

/// A loop that spends its budget: the watchdog's look arms it, the loop's next safepoint takes the
/// stop with a traceback that names the module's file and line, and the trip goes to the hook when
/// the callback has returned. The clocks move inside the loop, from the script.
#[test]
fn the_watchdog_arms_a_loop_that_uses_its_cpu_budget() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let lua = vm(&guard, 2, "com.x.game", &[("com.x.game", 256)]);
    vm_guard::name_chunk(&lua, "=game", "com.x.game", "src/main.luau");
    // Each call until the round is armed: 100 ms of processor time, and a look of the watchdog.
    // Raises past 100 calls with nothing armed, so a guard that never arms fails the test instead
    // of hanging it; after the arm the loop runs on while the VM samples its stack.
    let (g, f, calls) = (guard.clone(), fake.clone(), Rc::new(Cell::new(0u32)));
    let c = calls.clone();
    let armed = Rc::new(Cell::new(false));
    let a = armed.clone();
    let tick = lua
        .create_function(move |_, ()| {
            if a.get() {
                return Ok(());
            }
            c.set(c.get() + 1);
            if c.get() > 100 {
                return Err(mlua::Error::runtime("never armed"));
            }
            f.advance(Duration::from_millis(100), Duration::from_millis(100));
            a.set(g.poll().armed.is_some());
            Ok(())
        })
        .unwrap();
    lua.globals().set("tick", tick).unwrap();
    let hotkey = lua
        .load("local function spin()\n  while true do\n    tick()\n  end\nend\nreturn function() spin() end")
        .set_name("=game")
        .eval::<Function>()
        .unwrap();
    let (r, stopped) = call(&lua, EntryKind::Handler, "hotkey", &hotkey);
    assert!(stopped, "{r:?}");
    assert!(matches!(&r, Err(e) if vm_guard::is_stop_error(e)), "{r:?}");
    assert!((20..=22).contains(&calls.get()), "stopped after 2 s of processor time, not {} looks", calls.get());
    let trips = hooked.trips();
    assert_eq!(trips.len(), 1);
    let t = &trips[0];
    assert!(matches!(t.cause, Cause::Time { clock: Clock::Cpu, .. }), "{t:?}");
    assert_eq!((t.kind, t.what.as_str(), t.vm.module), (EntryKind::Handler, "hotkey", Some(2)));
    let loc = t.location.as_ref().expect("a location");
    // The loop's back edge, which Luau places on the `while` line.
    assert_eq!((loc.id.as_str(), loc.rel.as_str(), loc.line), ("com.x.game", "src/main.luau", 2));
    assert_eq!(t.frames[0], "com.x.game/src/main.luau:2: in function 'spin'", "{:?}", t.frames);
    assert!(t.culprit.is_none() && t.group.len() == 1, "its own code: only it");
}

/// The stop is sticky: whatever the module does to escape it — catch it, loop in an error handler,
/// in its own coroutine, in a comparator or a metamethod that C calls — it raises again at the next
/// safepoint, until the host has its callback back. Afterwards, turned on again, the VM works.
#[test]
fn nothing_escapes_the_stop() {
    let cases = [
        ("pcall loops", "while true do pcall(function() trip() while true do end end) end"),
        ("xpcall handler loops", "xpcall(function() trip() while true do end end, function() while true do end end) while true do end"),
        ("its own coroutine", "local co = coroutine.create(function() trip() while true do end end) while true do coroutine.resume(co) end"),
        ("table.sort comparator", "local t = {} for i = 1, 100 do t[i] = i end table.sort(t, function(a, b) trip() while true do end end)"),
        ("__index", "local t = setmetatable({}, { __index = function() trip() while true do end end }) return t.x"),
        ("__tostring from tostring", "return tostring(setmetatable({}, { __tostring = function() trip() while true do end end }))"),
        ("gsub with a function", "return (string.gsub('abc', '.', function() trip() while true do end end))"),
        ("catch and retry 1000 times", "for i = 1, 1000 do pcall(function() trip() while true do end end) end return 'escaped'"),
    ];
    for (name, code) in cases {
        let _h = hook();
        let (guard, _) = fake_guard();
        let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
        give_trip(&lua, &guard);
        let (r, stopped) = run(&lua, code);
        assert!(stopped, "{name}: not stopped");
        assert!(is_stop(&r), "{name}: {r:?}");
        vm_guard::clear_module(&guard, 0);
        assert_eq!(lua.load("return 1 + 1").eval::<i32>().unwrap(), 2, "{name}: the VM works after");
    }
}

/// Pattern backtracking in C only (`.-.-b` over 20 000 `a`, seconds of work with no Luau
/// instruction in it): armed from another thread, the matcher's own safepoints stop it.
#[test]
fn the_pattern_matcher_is_stopped_from_another_thread() {
    let _h = hook();
    let (guard, _) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let g = guard.clone();
    let armer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        g.arm_now(Clock::Wall);
    });
    let began = Instant::now();
    let (r, stopped) = run(&lua, "local s = string.rep('a', 20000) local found = string.find(s, '.-.-b') return 'not stopped'");
    armer.join().unwrap();
    assert!(stopped && is_stop(&r), "{r:?} after {:?}", began.elapsed());
}

/// The VM that runs takes the stop, not the one that waits on it: A's hotkey calls the host, which
/// runs B's callback, and B loops. B takes the round; A's call sees B's end as a failed call and A
/// returns normally — with its clocks started again.
#[test]
fn the_vm_that_runs_takes_the_stop() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    give_trip(&b, &guard);
    give_clock(&b, &guard, &fake);
    // B loops: it has run past half the budget itself when it takes the round.
    let spin = func(&b, "return function() trip() while true do age(100) end end");
    let (g, bb, seqs) = (guard.clone(), b.clone(), Rc::new(Cell::new((0u64, 0u64))));
    let s = seqs.clone();
    let nested = a
        .create_function(move |_, ()| {
            let before = g.slice_seq();
            let entry = Entry::enter(&bb, EntryKind::Plain, "arbiter onDeactivate").unwrap();
            let r = spin.call::<()>(());
            entry.finish(&r);
            let ended = entry.stopped();
            drop(entry);
            s.set((before, g.slice_seq()));
            Ok(ended)
        })
        .unwrap();
    a.globals().set("nested", nested).unwrap();
    let (r, a_stopped) = run(&a, "local ended = nested() return ended");
    assert!(!a_stopped, "A is not stopped");
    assert!(matches!(r, Ok(mlua::Value::Boolean(true))), "A ran on to its end: {r:?}");
    assert!(vm_guard::stopped(&b) && !vm_guard::stopped(&a));
    let (before, after) = seqs.get();
    assert_eq!(after, before + 2, "the claim restarted A's slice");
    let trips = hooked.trips();
    assert_eq!(trips.len(), 1);
    let t = &trips[0];
    assert_eq!(t.vm.id, "com.x.b");
    assert!(t.outer.as_ref().is_some_and(|o| o.id == "com.x.a" && o.what == "hotkey"), "{t:?}");
}

/// After B's stop, A's callback has a fresh budget: it is stopped only after 2 s more of its own.
#[test]
fn the_outer_callback_is_stopped_after_a_fresh_budget() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    give_trip(&b, &guard);
    give_clock(&b, &guard, &fake);
    let spin = func(&b, "return function() trip() while true do age(100) end end");
    let bb = b.clone();
    let nested = a
        .create_function(move |_, ()| {
            let entry = Entry::enter(&bb, EntryKind::Plain, "arbiter onDeactivate").unwrap();
            let r = spin.call::<()>(());
            entry.finish(&r);
            Ok(())
        })
        .unwrap();
    a.globals().set("nested", nested).unwrap();
    let (g, f, steps) = (guard.clone(), fake.clone(), Rc::new(Cell::new(0u32)));
    let st = steps.clone();
    let armed = Rc::new(Cell::new(false));
    let ar = armed.clone();
    let step = a
        .create_function(move |_, ()| {
            if ar.get() {
                return Ok(());
            }
            st.set(st.get() + 1);
            if st.get() > 100 {
                return Err(mlua::Error::runtime("never armed"));
            }
            f.advance(Duration::from_millis(100), Duration::from_millis(100));
            ar.set(g.poll().armed.is_some());
            Ok(())
        })
        .unwrap();
    a.globals().set("step", step).unwrap();
    let (r, a_stopped) = run(&a, "nested() while true do step() end");
    assert!(a_stopped && is_stop(&r), "{r:?}");
    assert!((20..=22).contains(&steps.get()), "A had a fresh 2 s: {} steps", steps.get());
    let trips = hooked.trips();
    assert_eq!(trips.iter().map(|t| t.vm.id.as_str()).collect::<Vec<_>>(), ["com.x.b", "com.x.a"]);
}

/// A stopped VM refuses every entry, and raises at the first safepoint of anything that runs in it
/// without one.
#[test]
fn a_stopped_vm_refuses_entry_and_raises_everywhere() {
    let _h = hook();
    let (guard, _) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    give_trip(&lua, &guard);
    let (r, stopped) = run(&lua, "trip() while true do end");
    assert!(stopped && is_stop(&r));
    assert!(Entry::enter(&lua, EntryKind::Plain, "arbiter onActivate").is_err());
    assert!(vm_guard::armed(&lua));
    let r = lua.load("local n = 0 for i = 1, 3 do n += i end return n").eval::<i32>();
    assert!(matches!(&r, Err(e) if vm_guard::is_stop_error(e)), "{r:?}");
}

/// The three VMs of a library: its own, and two that run its code. `lib.spin` is the library's
/// function, `own_spin` the module's own, each in a chunk named for its module.
fn library_set(guard: &Arc<Guard>) -> (Lua, Lua, Lua) {
    let lib = vm(guard, 0, "com.x.lib", &[("com.x.lib", 256)]);
    let a = vm(guard, 1, "com.x.a", &[("com.x.a", 256), ("com.x.lib", 256)]);
    let b = vm(guard, 2, "com.x.b", &[("com.x.b", 256), ("com.x.lib", 256)]);
    for (lua, id) in [(&a, "com.x.a"), (&b, "com.x.b")] {
        give_trip(lua, guard);
        vm_guard::name_chunk(lua, "=lib", "com.x.lib", "src/main.luau");
        vm_guard::name_chunk(lua, "=own", id, "src/main.luau");
        let m: mlua::Table = lua.load("local M = {}\nfunction M.spin()\n  trip() while true do end\nend\nreturn M").set_name("=lib").eval().unwrap();
        lua.globals().set("lib", m).unwrap();
        let own: Function = lua.load("return function()\n  trip() while true do end\nend").set_name("=own").eval().unwrap();
        lua.globals().set("own_spin", own).unwrap();
    }
    (lib, a, b)
}

/// A loop in a library's code stops every VM that runs that code — the library's own and its
/// dependents' — in one step: each is marked and armed, and an entry into one is refused in the
/// same slice.
#[test]
fn a_library_loop_stops_every_vm_that_runs_its_code() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    let (r, stopped) = run(&a, "lib.spin()");
    assert!(stopped && is_stop(&r), "{r:?}");
    for (lua, name) in [(&lib, "lib"), (&a, "a"), (&b, "b")] {
        assert!(vm_guard::stopped(lua) && vm_guard::armed(lua), "{name} is stopped and armed");
    }
    assert!(Entry::enter(&b, EntryKind::Handler, "hotkey").is_err(), "B refuses entry at once");
    let t = &hooked.trips()[0];
    assert_eq!(t.vm.id, "com.x.a");
    assert_eq!(t.culprit, Some(("com.x.lib".to_string(), "Module com.x.lib".to_string())));
    let ids: Vec<&str> = t.group.iter().map(|g| g.id.as_str()).collect();
    assert_eq!(ids, ["com.x.a", "com.x.lib", "com.x.b"], "the tripping VM first");
    let loc = t.location.as_ref().unwrap();
    assert_eq!((loc.id.as_str(), loc.rel.as_str(), loc.line), ("com.x.lib", "src/main.luau", 3));
}

/// A loop in the module's own code stops only that module, though a library's code runs in it.
#[test]
fn a_loop_in_the_modules_own_code_stops_only_it() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    let (r, stopped) = run(&a, "own_spin()");
    assert!(stopped && is_stop(&r));
    assert!(!vm_guard::stopped(&lib) && !vm_guard::stopped(&b));
    let t = &hooked.trips()[0];
    assert!(t.culprit.is_none());
    assert_eq!(t.group.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["com.x.a"]);
}

/// A loop in a chunk the host owns is charged to the VM's own module.
#[test]
fn a_loop_in_host_code_is_charged_to_the_vms_module() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let (_lib, a, b) = library_set(&guard);
    let host_fn: Function = a.load("return function()\n  trip() while true do end\nend").set_name("window_prelude").eval().unwrap();
    a.globals().set("host_fn", host_fn).unwrap();
    let (r, stopped) = run(&a, "host_fn()");
    assert!(stopped && is_stop(&r));
    assert!(!vm_guard::stopped(&b));
    let t = &hooked.trips()[0];
    assert!(t.culprit.is_none() && t.location.is_none(), "{t:?}");
    assert!(t.frames[0].starts_with("(host) window_prelude:2: in "), "{:?}", t.frames);
}

/// A VM that is loading takes nobody with it, though the loop is in a library's code: its load
/// fails, and the library and its other dependents run on.
#[test]
fn a_stop_while_loading_stops_nobody_else() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let (lib, _a, b) = library_set(&guard);
    let loading = vm_guard::new_vm(&guard);
    vm_guard::describe_for_test(&loading, 3, "com.x.new", &[("com.x.new", 256), ("com.x.lib", 256)]);
    give_trip(&loading, &guard);
    vm_guard::name_chunk(&loading, "=lib", "com.x.lib", "src/main.luau");
    let m: mlua::Table = loading.load("local M = {}\nfunction M.spin()\n  trip() while true do end\nend\nreturn M").set_name("=lib").eval().unwrap();
    loading.globals().set("lib", m).unwrap();
    let entry = Entry::enter(&loading, EntryKind::Load, "its entry file").unwrap();
    let r = loading.load("lib.spin()").exec();
    entry.finish(&r);
    assert!(entry.stopped());
    drop(entry);
    assert!(!vm_guard::stopped(&lib) && !vm_guard::stopped(&b), "nobody else");
    let t = vm_guard::last_trip(&loading).unwrap();
    assert!(t.loading && t.group.len() == 1, "{t:?}");
    assert_eq!(hooked.trips().len(), 1, "handed over, for the hook to skip");
}

/// The frames' words: a module's file, a C function, a chunk the host owns, any other chunk.
#[test]
fn the_frames_name_module_files_lines_c_and_host() {
    use vm_guard::{frame_text, RawFrame};
    let chunks: std::collections::HashMap<String, (String, String)> =
        [("C:\\m\\game\\src\\main.luau".to_string(), ("com.example.game".to_string(), "src/presets.luau".to_string()))].into_iter().collect();
    let f = |source: Option<&str>, what: &'static str, line: Option<usize>, name: Option<&str>| RawFrame {
        source: source.map(str::to_string),
        what,
        line,
        defined: Some(230),
        name: name.map(str::to_string),
    };
    let (t, m) = frame_text(&f(Some("C:\\m\\game\\src\\main.luau"), "Lua", Some(236), Some("ratio")), &chunks);
    assert_eq!(t, "com.example.game/src/presets.luau:236: in function 'ratio'");
    assert_eq!(m, Some(("com.example.game".into(), "src/presets.luau".into())));
    assert_eq!(frame_text(&f(None, "C", None, Some("upper")), &chunks).0, "[C]: in function 'upper'");
    assert_eq!(frame_text(&f(None, "C", None, None), &chunks).0, "[C]: in ?");
    assert_eq!(
        frame_text(&f(Some("window_prelude"), "Lua", Some(120), Some("_dispatchActivate")), &chunks).0,
        "(host) window_prelude:120: in function '_dispatchActivate'"
    );
    assert_eq!(frame_text(&f(Some("=host.ocr"), "Lua", Some(4), None), &chunks).0, "(host) host.ocr:4: in function <(host) host.ocr:230>");
    assert_eq!(frame_text(&f(Some("=probe"), "main", Some(1), None), &chunks).0, "probe:1: in the file's top level");
}

/// The host call running when the time ran out is named: in the watchdog's note and in the trip.
/// The markers nest, each giving back its predecessor; an entry hides its caller's.
#[test]
fn the_host_call_in_progress_is_named() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    {
        crate::vm_guard::host_call!("host.test.outer");
        assert_eq!(guard.call_now(), Some("host.test.outer"));
        {
            crate::vm_guard::host_call!("host.test.inner");
            assert_eq!(guard.call_now(), Some("host.test.inner"));
        }
        assert_eq!(guard.call_now(), Some("host.test.outer"));
    }
    assert_eq!(guard.call_now(), None);

    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let (g, f) = (guard.clone(), fake.clone());
    let slow = lua
        .create_function(move |_, ()| {
            crate::vm_guard::host_call!("host.test.slow");
            g.poll();
            f.advance(Duration::from_secs(3), Duration::from_secs(3));
            g.poll();
            Ok(())
        })
        .unwrap();
    lua.globals().set("slow", slow).unwrap();
    let (r, stopped) = run(&lua, "while true do slow() end");
    assert!(stopped && is_stop(&r), "{r:?}");
    let t = &hooked.trips()[0];
    assert_eq!(t.call, Some("host.test.slow"), "{t:?}");

    // Inside an entry, the caller's host call is not the callee's.
    let other = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    crate::vm_guard::host_call!("host.arbiter.setMatching");
    let entry = Entry::enter(&other, EntryKind::Plain, "arbiter onDeactivate").unwrap();
    assert_eq!(guard.call_now(), None);
    drop(entry);
    assert_eq!(guard.call_now(), Some("host.arbiter.setMatching"));
}

/// The real watchdog thread, every 50 ms, stops a loop on this thread: the interrupt written from
/// there, taken here. The clocks are fake and move inside the loop.
#[test]
fn the_watchdog_thread_stops_a_loop_on_the_event_loop() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let f = fake.clone();
    let began = Instant::now();
    let tick = lua
        .create_function(move |_, ()| {
            if began.elapsed() > Duration::from_secs(20) {
                return Err(mlua::Error::runtime("the watchdog never armed"));
            }
            f.advance(Duration::from_micros(100), Duration::from_micros(100));
            Ok(())
        })
        .unwrap();
    lua.globals().set("tick", tick).unwrap();
    let mut dog = vm_guard::spawn_watchdog(guard.clone());
    let (r, stopped) = run(&lua, "while true do tick() end");
    dog.stop();
    assert!(stopped && is_stop(&r), "{r:?}");
    assert_eq!(hooked.trips().len(), 1);
}

// --- G2: whose loop it is -------------------------------------------------------------------------

/// A module's own loop that calls a helper of a library, armed at many points — at the helper's
/// own safepoints most of the time: the samples find the loop's function in every sample and the
/// helper in only some, so the module is stopped and the library, and the other module that runs
/// its code, run on. One sample would have laid it at the library's door most of the time.
#[test]
fn a_loop_that_calls_a_library_helper_is_the_modules_own() {
    for n in 0..12u64 {
        let hooked = hook();
        let (guard, _) = fake_guard();
        let lib = vm(&guard, 0, "com.x.lib", &[("com.x.lib", 256)]);
        let a = vm(&guard, 1, "com.x.a", &[("com.x.a", 256), ("com.x.lib", 256)]);
        let b = vm(&guard, 2, "com.x.b", &[("com.x.b", 256), ("com.x.lib", 256)]);
        vm_guard::name_chunk(&a, "=lib", "com.x.lib", "src/main.luau");
        vm_guard::name_chunk(&a, "=own", "com.x.a", "src/main.luau");
        // A helper with a 20-step loop of its own: most of the safepoints are in it.
        let m: mlua::Table = a
            .load("local M = {}\nfunction M.ready()\n  local s = 0\n  for i = 1, 20 do s += i end\n  return false\nend\nreturn M")
            .set_name("=lib")
            .eval()
            .unwrap();
        a.globals().set("lib", m).unwrap();
        let poll: Function = a.load("return function()\n  while not lib.ready() do end\nend").set_name("=own").eval().unwrap();
        a.globals().set("poll", poll).unwrap();
        let g = guard.clone();
        let armer = std::thread::spawn(move || {
            // Into the loop first: a round armed before the slice began would wait for nothing.
            while g.slice_seq() % 2 == 0 {
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_micros(300 + n * 397));
            g.arm_now(Clock::Cpu);
        });
        let (r, stopped) = run(&a, "poll()");
        armer.join().unwrap();
        assert!(stopped && is_stop(&r), "{n}: {r:?}");
        let t = &hooked.trips()[0];
        assert!(t.culprit.is_none(), "{n}: laid at the library's door: {t:?}");
        assert_eq!(t.group.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["com.x.a"], "{n}");
        assert!(!vm_guard::stopped(&lib) && !vm_guard::stopped(&b), "{n}: the library and B run on");
        let loc = t.location.as_ref().unwrap();
        assert_eq!((loc.id.as_str(), loc.line), ("com.x.a", 2), "{n}: the loop's own line");
    }
}

/// A slow host call that a library's code makes — a whole-window image match, a walk into a
/// plug-in that hangs — stops only the module whose callback ran it, though the time ran out
/// inside the library's code: it is one window's or one plug-in's fault, not code that every
/// dependent shares. The place is still the library's line, for the author.
#[test]
fn a_slow_host_call_in_a_librarys_code_stops_only_the_caller() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    let (g, f) = (guard.clone(), fake.clone());
    let slow = a
        .create_function(move |_, ()| {
            crate::vm_guard::host_call!("host.test.slowMatch");
            g.poll();
            f.advance(Duration::from_secs(3), Duration::from_secs(3));
            g.poll();
            Ok(())
        })
        .unwrap();
    a.globals().set("slowMatch", slow).unwrap();
    let m: mlua::Table = a.load("local M = {}\nfunction M.find()\n  while true do slowMatch() end\nend\nreturn M").set_name("=lib").eval().unwrap();
    a.globals().set("finder", m).unwrap();
    let (r, stopped) = run(&a, "finder.find()");
    assert!(stopped && is_stop(&r), "{r:?}");
    assert!(!vm_guard::stopped(&lib) && !vm_guard::stopped(&b), "the library and B run on");
    let t = &hooked.trips()[0];
    assert_eq!(t.call, Some("host.test.slowMatch"));
    assert!(t.culprit.is_none() && t.group.len() == 1, "{t:?}");
    let loc = t.location.as_ref().unwrap();
    assert_eq!((loc.id.as_str(), loc.line), ("com.x.lib", 3), "the library's line, for its author");
}

/// The wall clock alone never takes a library's dependents with it: a loop in a library's code
/// that waited for the processor behind other programs stops only the module whose callback ran it.
#[test]
fn the_wall_clock_stops_only_the_caller_in_a_librarys_code() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    a.globals()
        .set("trip", a.create_function({
            let g = guard.clone();
            move |_, ()| {
                g.arm_now(Clock::Wall);
                Ok(())
            }
        })
        .unwrap())
        .unwrap();
    let (r, stopped) = run(&a, "lib.spin()");
    assert!(stopped && is_stop(&r), "{r:?}");
    assert!(!vm_guard::stopped(&lib) && !vm_guard::stopped(&b));
    let t = &hooked.trips()[0];
    assert!(t.culprit.is_none() && t.group.len() == 1, "{t:?}");
}

/// A library's own VM, looping in its own code, is a loop in a library's code: the library and
/// every module that runs its code are stopped. A loop in the host's own Luau in that VM stops only
/// the library.
#[test]
fn a_librarys_own_loop_takes_its_dependents_and_host_code_in_it_does_not() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    give_trip(&lib, &guard);
    vm_guard::name_chunk(&lib, "=lib", "com.x.lib", "src/main.luau");
    let spin: Function = lib.load("return function()\n  trip() while true do end\nend").set_name("=lib").eval().unwrap();
    lib.globals().set("spin", spin).unwrap();
    let (r, stopped) = run(&lib, "spin()");
    assert!(stopped && is_stop(&r), "{r:?}");
    let t = &hooked.trips()[0];
    assert!(t.culprit.is_none(), "its own code: no other module's");
    assert_eq!(t.group.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["com.x.lib", "com.x.a", "com.x.b"]);
    assert!(vm_guard::stopped(&a) && vm_guard::stopped(&b));

    let hooked = hook();
    let (guard, _) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    give_trip(&lib, &guard);
    let host_fn: Function = lib.load("return function()\n  trip() while true do end\nend").set_name("window_prelude").eval().unwrap();
    lib.globals().set("host_fn", host_fn).unwrap();
    let (r, stopped) = run(&lib, "host_fn()");
    assert!(stopped && is_stop(&r), "{r:?}");
    let t = &hooked.trips()[0];
    assert_eq!(t.group.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["com.x.lib"], "the host's Luau: only the VM's module");
    assert!(!vm_guard::stopped(&a) && !vm_guard::stopped(&b));
}

/// A quick callback inside another module's lets the round pass: A's hotkey spent the time, the
/// round is armed while B's `onDeactivate` runs, and B returns at once — so A takes the round at
/// its next safepoint, and B is not stopped. The host call armed during B is not A's either.
#[test]
fn a_quick_callee_lets_the_callers_round_pass() {
    let hooked = hook();
    let (guard, _) = fake_guard();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    give_trip(&b, &guard);
    let quick = func(&b, "return function() trip() local n = 0 for i = 1, 1000 do n += i end end");
    let bb = b.clone();
    let nested = a
        .create_function(move |_, ()| {
            crate::vm_guard::host_call!("host.arbiter.setMatching");
            let entry = Entry::enter(&bb, EntryKind::Plain, "arbiter onDeactivate").unwrap();
            let r = quick.call::<()>(());
            entry.finish(&r);
            Ok(r.is_ok() && !entry.stopped())
        })
        .unwrap();
    a.globals().set("nested", nested).unwrap();
    let (r, a_stopped) = run(&a, "local ok = nested() assert(ok, 'B was stopped') while true do end");
    assert!(a_stopped && is_stop(&r), "{r:?}");
    assert!(!vm_guard::stopped(&b), "B, which ran 0 s of its own, is not stopped");
    let trips = hooked.trips();
    assert_eq!(trips.len(), 1, "{trips:?}");
    assert_eq!(trips[0].vm.id, "com.x.a");
    assert_eq!(trips[0].call, None, "armed while B ran: not A's host call");
}

/// A callback past its limit that returns before its samples are complete — one long host call, and
/// then its end — is stopped all the same, its own module only.
#[test]
fn a_callback_that_returns_past_its_limit_is_stopped() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let (lib, a, b) = library_set(&guard);
    let (g, f) = (guard.clone(), fake.clone());
    let slow = a
        .create_function(move |_, ()| {
            crate::vm_guard::host_call!("host.test.hangs");
            g.poll();
            f.advance(Duration::from_secs(11), Duration::from_millis(300));
            g.poll();
            Ok(())
        })
        .unwrap();
    a.globals().set("hangs", slow).unwrap();
    let m: mlua::Table = a.load("local M = {}\nfunction M.once()\n  hangs()\n  return 1\nend\nreturn M").set_name("=lib").eval().unwrap();
    a.globals().set("once", m).unwrap();
    let (r, _) = run(&a, "return once.once()");
    // It returned: the stop is taken as its call's entry ends.
    assert!(matches!(r, Ok(mlua::Value::Integer(1))), "{r:?}");
    assert!(vm_guard::stopped(&a), "returned, but past its limit");
    let t = &hooked.trips()[0];
    let Cause::Time { clock, .. } = t.cause else { panic!("{t:?}") };
    assert_eq!((clock, t.call), (Clock::Wall, Some("host.test.hangs")));
    assert!(t.culprit.is_none() && t.group.len() == 1, "{t:?}");
    assert!(!vm_guard::stopped(&lib) && !vm_guard::stopped(&b));
}

/// A callee that loops is not shielded by its caller's samples: A's round is armed and A begins to
/// sample, then A calls B, which loops; once B has run half the budget itself, it samples in A's
/// place and takes the stop, and A goes on.
#[test]
fn a_callee_that_loops_takes_the_round_from_its_caller() {
    let hooked = hook();
    let (guard, fake) = fake_guard();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    give_trip(&a, &guard);
    give_clock(&b, &guard, &fake);
    let spin = func(&b, "return function() while true do age(100) end end");
    let bb = b.clone();
    let nested = a
        .create_function(move |_, ()| {
            let entry = Entry::enter(&bb, EntryKind::Plain, "arbiter onDeactivate").unwrap();
            let r = spin.call::<()>(());
            entry.finish(&r);
            Ok(entry.stopped())
        })
        .unwrap();
    a.globals().set("nested", nested).unwrap();
    let (r, a_stopped) = run(&a, "trip() local x = 0 for i = 1, 3 do x += i end return nested()");
    assert!(matches!(r, Ok(mlua::Value::Boolean(true))), "B was stopped and A ran on: {r:?}");
    assert!(!a_stopped && vm_guard::stopped(&b));
    let trips = hooked.trips();
    assert_eq!(trips.iter().map(|t| t.vm.id.as_str()).collect::<Vec<_>>(), ["com.x.b"]);
}

/// The trips no outermost entry's end handed over — a panic unwound through it — are taken at the
/// end of the turn, when no entry is on the stack; or by the next outermost entry's end, if one
/// comes first.
#[test]
fn trips_a_panic_left_behind_are_taken_at_the_end_of_the_turn() {
    let hooked = hook();
    let guard = Guard::new();
    let stop_under_a_panic = |idx: usize| {
        let lua = vm(&guard, idx, &format!("com.x.m{idx}"), &[("com.x.m", 4)]);
        let fill = func(&lua, FILL);
        crate::quiet_expected_panics();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let entry = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
            let r = fill.call::<()>(());
            entry.finish(&r);
            panic!("{}unwinds through the entry", crate::EXPECTED_PANIC);
        }));
        assert!(r.is_err());
        lua
    };
    let _first = stop_under_a_panic(0);
    assert_eq!(hooked.handed(), 0, "not handed over while the panic unwound");
    let other = vm(&guard, 9, "com.x.o", &[("com.x.o", 256)]);
    let entry = Entry::enter(&other, EntryKind::Handler, "timer").unwrap();
    assert!(vm_guard::take_waiting_trips(&guard).is_empty(), "not while an entry is on the stack");
    drop(entry);
    assert_eq!(hooked.trips().len(), 1, "the next outermost entry's end handed it over");
    let _second = stop_under_a_panic(1);
    let waiting = vm_guard::take_waiting_trips(&guard);
    assert_eq!(waiting.len(), 1, "the end of the turn takes it");
    assert_eq!(waiting[0].vm.module, Some(1));
    assert_eq!(hooked.trips().len(), 1);
}

// --- G2: the real clocks ------------------------------------------------------------------------

/// The real processor clock of this thread, read by the real watchdog thread, stops a loop: a
/// budget of 100 ms, and a loop that would run 20 s.
#[test]
fn the_real_processor_clock_stops_a_loop() {
    let hooked = hook();
    let guard = Guard::with(Budgets { cpu: Duration::from_millis(100), wall: Duration::from_secs(10) }, Box::new(vm_guard::RealClocks::here()));
    vm_guard::install(&guard);
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let mut dog = vm_guard::spawn_watchdog(guard.clone());
    let began = Instant::now();
    let (r, stopped) = run(&lua, "local t = os.clock() while os.clock() - t < 20 do end return 'ran out'");
    dog.stop();
    assert!(stopped && is_stop(&r), "{r:?} after {:?}", began.elapsed());
    let t = &hooked.trips()[0];
    let Cause::Time { clock, wall, .. } = t.cause else { panic!("{t:?}") };
    assert!(clock == Clock::Cpu || wall >= Duration::from_secs(10), "{t:?}");
}

/// Garbage alone does not run a VM out: dropped strings of a MiB, 200 of them in an 8 MiB VM, are
/// collected as the VM goes.
#[test]
fn garbage_alone_does_not_meet_the_limit() {
    // Values of a thirty-second of the limit, made and dropped 400 times: collected as the VM goes.
    // (Measured on this Luau: values of an eighth — 1 MiB in 8 MiB — or a sixteenth — 4 MiB in 64
    // MiB — met the limit with garbage alone; module-package-format.md says so.)
    for (limit, kb) in [(32u32, 1024usize), (8, 256)] {
        let guard = Guard::new();
        let lua = vm(&guard, 0, "com.x.g", &[("com.x.g", limit)]);
        let churn = func(&lua, &format!("return function() for i = 1, 400 do local s = string.rep('g', {}) .. i end end", kb * 1024));
        let (r, stopped) = call(&lua, EntryKind::Handler, "timer", &churn);
        assert!(r.is_ok() && !stopped, "{limit} MiB, values of {kb} KiB: {r:?}");
    }
}


// --- G2: the [cpu] lines ------------------------------------------------------------------------

/// The processor time each look sees is charged to the VM on top; a module above its share of a
/// minute gets one line, one below gets none; one long callback gets a line of its own; and the run
/// ends with a summary.
#[test]
fn the_cpu_line_names_a_module_above_its_share() {
    let _h = hook();
    let (guard, fake) = fake_guard();
    let a = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let b = vm(&guard, 1, "com.x.b", &[("com.x.b", 256)]);
    let mut lines = guard.poll().lines;
    for i in 0..60 {
        let (lua, cpu) = if i % 2 == 0 { (&a, 400) } else { (&b, 20) };
        fake.advance(Duration::from_secs(1), Duration::from_millis(cpu));
        let _e = Entry::enter(lua, EntryKind::Handler, "timer").unwrap();
        lines.extend(guard.poll().lines);
    }
    let cpu: Vec<&String> = lines.iter().filter(|(c, _)| *c == "cpu").map(|(_, l)| l).collect();
    assert_eq!(
        cpu,
        [&vm_guard::share_line("com.x.a", Duration::from_secs(12), Duration::from_secs(60), Duration::ZERO)],
        "{lines:?}"
    );
    assert_eq!(
        cpu[0],
        "[com.x.a] used 12.0 s of the event loop's processor time in the last 60 s (20 %); none of its callbacks ran \
         across two looks of the watchdog. A measurement for module authors and for deciding on threads per module; it \
         changes nothing"
    );

    // One long callback.
    let e = Entry::enter(&b, EntryKind::Handler, "hotkey").unwrap();
    guard.poll();
    fake.advance(Duration::from_millis(300), Duration::from_millis(300));
    guard.poll();
    drop(e);
    let lines = guard.poll().lines;
    assert!(lines.contains(&("cpu", vm_guard::callback_line("com.x.b", Duration::from_millis(300)))), "{lines:?}");
    assert_eq!(
        vm_guard::callback_line("com.x.b", Duration::from_millis(300)),
        "[com.x.b] one callback took 0.3 s of the event loop's processor time. A measurement for module authors and \
         for deciding on threads per module; it changes nothing"
    );
    let summary = guard.cpu_summary();
    assert_eq!(summary.len(), 2, "{summary:?}");
    assert!(summary[0].starts_with("[com.x.a] 12.0 s of the event loop's processor time over 1 min of running ("), "{summary:?}");
    assert!(summary[1].contains("its longest callback took 0.3 s"), "{summary:?}");
}

/// What an entry costs the event loop: reported, not asserted, like `HANDLER COST`.
#[test]
fn guard_entry_cost() {
    let (guard, _) = fake_guard();
    let lua = vm(&guard, 0, "com.x.a", &[("com.x.a", 256)]);
    let n = 20_000;
    let began = Instant::now();
    for _ in 0..n {
        let e = Entry::enter(&lua, EntryKind::Handler, "hotkey").unwrap();
        drop(e);
    }
    let per = began.elapsed().as_nanos() as f64 / n as f64 / 1000.0;
    println!("GUARD ENTRY COST: {per:.2} µs per entry and exit, over {n}");
    vm_guard::clear_exit_hook();
}

/// mlua as the guard uses it, held against mlua itself: if another mlua changes any of this, the
/// pin in `crates/host/Cargo.toml` has to be a decision, made with these tests.
mod mlua_pin {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use mlua::{Lua, VmState};

    /// The address of `lua_Callbacks.interrupt` in `lua`.
    fn field(lua: &Lua) -> &'static AtomicUsize {
        let mut f = 0usize;
        unsafe {
            lua.exec_raw::<()>((), |state| {
                f = std::ptr::addr_of_mut!((*mlua::ffi::lua_callbacks(state)).interrupt) as usize;
            })
            .unwrap();
            &*(f as *const AtomicUsize)
        }
    }

    /// Every byte of `lua_Callbacks`.
    fn callbacks(lua: &Lua) -> Vec<u8> {
        let mut out = Vec::new();
        unsafe {
            lua.exec_raw::<()>((), |state| {
                let cb = mlua::ffi::lua_callbacks(state) as *const u8;
                out = std::slice::from_raw_parts(cb, std::mem::size_of::<mlua::ffi::lua_Callbacks>()).to_vec();
            })
            .unwrap();
        }
        out
    }

    /// A VM whose interrupt counts its calls and raises while `stop` is set: installed, saved and
    /// switched off, as `new_vm` does it. The saved procedure, the count and the switch.
    fn counted() -> (Lua, usize, Rc<Cell<u32>>, Rc<Cell<bool>>) {
        let lua = Lua::new();
        let (n, stop) = (Rc::new(Cell::new(0u32)), Rc::new(Cell::new(false)));
        let (nn, ss) = (n.clone(), stop.clone());
        lua.set_interrupt(move |_| {
            nn.set(nn.get() + 1);
            if ss.get() {
                Err(mlua::Error::runtime("pinned stop"))
            } else {
                Ok(VmState::Continue)
            }
        });
        let saved = field(&lua).swap(0, Ordering::SeqCst);
        (lua, saved, n, stop)
    }

    #[test]
    fn mlua_is_pinned_in_the_manifest_and_the_lock() {
        let manifest = include_str!("../Cargo.toml");
        assert!(manifest.contains(r#"mlua = { version = "=0.11.6""#), "crates/host/Cargo.toml no longer pins mlua");
        let dev = include_str!("../../module-manifest/Cargo.toml");
        assert!(dev.contains(r#"mlua = { version = "=0.11.6""#), "module-manifest's mlua is not pinned with it");
        let lock = include_str!("../../../Cargo.lock").replace("\r\n", "\n");
        for (name, version) in [("mlua", "0.11.6"), ("mlua-sys", "0.10.0"), ("luau0-src", "0.18.3+luau709")] {
            assert!(
                lock.contains(&format!("name = \"{name}\"\nversion = \"{version}\"")),
                "Cargo.lock: {name} is not {version}"
            );
        }
    }

    #[test]
    fn a_new_vm_has_no_interrupt() {
        assert_eq!(field(&Lua::new()).load(Ordering::SeqCst), 0);
    }

    #[test]
    fn set_interrupt_writes_the_interrupt_field_and_nothing_else() {
        let lua = Lua::new();
        let before = callbacks(&lua);
        lua.set_interrupt(|_| Ok(VmState::Continue));
        let after = callbacks(&lua);
        let off = std::mem::offset_of!(mlua::ffi::lua_Callbacks, interrupt);
        let len = std::mem::size_of::<usize>();
        assert_eq!(before[..off], after[..off], "a field before `interrupt` changed");
        assert_eq!(before[off + len..], after[off + len..], "a field after `interrupt` changed");
        assert_ne!(field(&lua).load(Ordering::SeqCst), 0, "and `interrupt` is set");
    }

    #[test]
    fn mlua_leaves_the_field_alone_afterwards() {
        let (lua, saved, _, _) = counted();
        assert_ne!(saved, 0);
        let exercise = |lua: &Lua| {
            let f: mlua::Function = lua.load("return function(x) return x + 1 end").eval().unwrap();
            assert_eq!(f.call::<i32>(1).unwrap(), 2);
            let co = lua.create_thread(lua.load("return function() coroutine.yield(1) return 2 end").eval().unwrap()).unwrap();
            assert_eq!(co.resume::<i32>(()).unwrap(), 1);
            assert_eq!(co.resume::<i32>(()).unwrap(), 2);
            // A handler's own yield is answered with `resume_error` (task.rs).
            let co = lua.create_thread(lua.load("return function() local ok = pcall(coroutine.yield) return ok end").eval().unwrap()).unwrap();
            co.resume::<()>(()).unwrap();
            assert!(!co.resume_error::<bool>("raised at the yield").unwrap());
            lua.gc_collect().unwrap();
            lua.set_memory_limit(64 << 20).unwrap();
            lua.set_app_data(7u32);
            assert!(lua.load("error('x')").exec().is_err());
        };
        exercise(&lua);
        assert_eq!(field(&lua).load(Ordering::SeqCst), 0, "mlua wrote the field it no longer owns");
        field(&lua).store(saved, Ordering::SeqCst);
        exercise(&lua);
        assert_eq!(field(&lua).load(Ordering::SeqCst), saved, "mlua changed the procedure written back");
    }

    #[test]
    fn the_saved_pointer_calls_the_closure_and_its_error_ends_the_loop() {
        let (lua, saved, n, stop) = counted();
        lua.load("local x = 0 for i = 1, 1000000 do x += 1 end").exec().unwrap();
        assert_eq!(n.get(), 0, "unarmed, never called");
        field(&lua).store(saved, Ordering::SeqCst);
        lua.load("local x = 0 for i = 1, 1000 do x += 1 end").exec().unwrap();
        assert!(n.get() >= 1000, "armed, called at the safepoints: {}", n.get());
        let co = lua.create_thread(lua.load("return function() while true do end end").eval().unwrap()).unwrap();
        stop.set(true);
        let e = lua.load("while true do end").exec().unwrap_err();
        assert!(e.to_string().contains("pinned stop"), "{e}");
        assert!(co.resume::<()>(()).unwrap_err().to_string().contains("pinned stop"));
        field(&lua).store(0, Ordering::SeqCst);
        stop.set(false);
        assert_eq!(lua.load("return 1 + 1").eval::<i32>().unwrap(), 2);
    }

    #[test]
    fn the_closure_sees_the_interrupted_coroutine() {
        let lua = Lua::new();
        let seen: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
        let source: Rc<std::cell::RefCell<String>> = Rc::default();
        let (s, src) = (seen.clone(), source.clone());
        let co_slot: Rc<std::cell::RefCell<Option<mlua::Thread>>> = Rc::default();
        let cs = co_slot.clone();
        lua.set_interrupt(move |lua| {
            let Some(co) = cs.borrow().clone() else { return Ok(VmState::Continue) };
            if s.get().is_none() {
                s.set(Some(lua.current_thread() == co));
                *src.borrow_mut() = lua.inspect_stack(0, |d| d.source().source.map(|c| c.into_owned())).flatten().unwrap_or_default();
            }
            Err(mlua::Error::runtime("enough"))
        });
        let f: mlua::Function = lua.load("return function() while true do end end").set_name("=in the coroutine").eval().unwrap();
        let co = lua.create_thread(f).unwrap();
        *co_slot.borrow_mut() = Some(co.clone());
        assert!(co.resume::<()>(()).is_err());
        assert_eq!(seen.get(), Some(true), "the closure's `current_thread` is the coroutine");
        assert_eq!(&*source.borrow(), "=in the coroutine");
    }

    /// Luau calls the interrupt from its collector's steps too (`luaC_step`, with the GC state as
    /// its second argument), and mlua returns before the closure for those (`gc >= 0`): a VM the
    /// guard stopped stays armed, and would otherwise raise out of an allocation. Armed, with the
    /// closure raising, a whole cycle of steps — asked from Rust, and run by allocations from Rust
    /// — never reaches it.
    #[test]
    fn the_closure_is_not_called_for_gc_steps() {
        let (lua, saved, n, stop) = counted();
        lua.load("garbage = {} for i = 1, 20000 do garbage[i] = { i } end").exec().unwrap();
        lua.load("garbage = nil").exec().unwrap();
        field(&lua).store(saved, Ordering::SeqCst);
        stop.set(true);
        n.set(0);
        let mut cycle_done = false;
        for _ in 0..100_000 {
            if lua.gc_step().expect("a step of the collector does not raise") {
                cycle_done = true;
                break;
            }
        }
        assert!(cycle_done, "no cycle completed");
        for _ in 0..20_000 {
            lua.create_table().expect("an allocation that runs a step of the collector does not raise");
        }
        lua.gc_collect().expect("a full collection does not raise");
        assert_eq!(n.get(), 0, "the collector's steps called the closure");
        field(&lua).store(0, Ordering::SeqCst);
    }

    #[test]
    fn memory_limit_works_with_luau() {
        let lua = Lua::new();
        let fresh = lua.used_memory();
        assert!(lua.set_memory_limit(8 << 20).is_ok());
        // Every string different: Luau interns strings, and the same one made again costs nothing,
        // so a loop of equal strings fills only the table that holds them — slowly, and by chance.
        let e = lua.load("local t = {} while true do t[#t + 1] = string.rep('x', 65536) .. #t end").exec().unwrap_err();
        assert!(matches!(e, mlua::Error::MemoryError(_)), "{e:?}");
        // Full again, from Rust: an error, not an abort.
        let keep: mlua::Table = lua.create_table().unwrap();
        let mut refused = false;
        for i in 0..1000 {
            let mut bytes = vec![b'y'; 65536];
            bytes.extend_from_slice(i.to_string().as_bytes());
            match lua.create_string(bytes) {
                Ok(s) => keep.raw_set(i, s).unwrap(),
                Err(_) => {
                    refused = true;
                    break;
                }
            }
        }
        assert!(refused, "8 MiB never ran out");
        drop(keep);
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        assert!(lua.used_memory() < fresh + (1 << 20), "{} bytes after collecting, {fresh} fresh", lua.used_memory());
    }

    #[test]
    fn the_variants_the_guard_matches_exist() {
        // The stop's own error, through a Rust callback, is still the stop's.
        let lua = Lua::new();
        let raise = lua.create_function(|_, ()| -> mlua::Result<()> { Err(mlua::Error::runtime(crate::vm_guard::STOPPED)) }).unwrap();
        lua.globals().set("raise", raise).unwrap();
        let e = lua.load("raise()").exec().unwrap_err();
        assert!(crate::vm_guard::is_stop_error(&e), "{e:?}");
        assert!(!crate::vm_guard::is_stop_error(&mlua::Error::runtime("something else")));
        // A real memory error through a Rust callback, wrapped as mlua wraps it — or, where the
        // limit is met while mlua builds the callback's error (it depends on where the strings
        // land, which differs from run to run), the bare nil error of mlua's handler. Either way,
        // in a VM of the guard's, a stop.
        let script = "local k = {} for i = 1, 1000 do k[i] = string.rep('z', 65536) .. i alloc(65536 + i) end";
        lua.set_memory_limit(2 << 20).unwrap();
        let alloc = |lua: &Lua| {
            let f = lua.create_function(|lua, n: usize| lua.create_string(vec![b'y'; n]).map(|_| ())).unwrap();
            lua.globals().set("alloc", f).unwrap();
        };
        alloc(&lua);
        let e = lua.load(script).exec().unwrap_err();
        let nil = matches!(&e, mlua::Error::RuntimeError(m) if m == "<nil>");
        assert!(crate::vm_guard::is_memory_error(&e) || nil, "{e:?}");
        for _ in 0..20 {
            let guard = crate::vm_guard::Guard::new();
            let vm = crate::vm_guard::new_vm(&guard);
            crate::vm_guard::describe_for_test(&vm, 0, "com.x.m", &[("com.x.m", 2)]);
            crate::vm_guard::loaded(&vm);
            alloc(&vm);
            let entry = crate::vm_guard::Entry::enter(&vm, crate::vm_guard::EntryKind::Handler, "hotkey").unwrap();
            let r = vm.load(script).exec();
            entry.finish(&r);
            assert!(entry.stopped(), "not taken for a memory stop: {r:?}");
            drop(entry);
            assert_eq!(guard.take_trips().len(), 1);
        }
    }
}
