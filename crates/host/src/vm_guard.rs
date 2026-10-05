//! The guard over every module VM: what each VM may use, every call from the host into a VM's
//! Lua, and the stop of a module that goes past a limit.
//!
//! **Two limits.**
//! - **Memory (G1).** Every VM has one: the sum of what each module whose code runs in it may use —
//!   the module itself and each of its code dependencies, transitively, each counted once — 256 MiB
//!   a module unless its manifest's `[limits] memory_mib` asks for more ([`describe`]). A memory
//!   error the module's own code does not catch, reaching the host at an entry, stops the module.
//! - **Time (G2).** A callback may take 2 s of the event loop's processor time, or 10 s on the wall
//!   clock, counted from the outermost call into Lua — the slice ([`Entry`]). Past either, the VM
//!   that runs Lua next takes the stop ([`on_interrupt`]): the one that loops, or the one whose slow
//!   host call just returned, never one that waits on it.
//!
//! **Every VM is made here** ([`new_vm`]), with the default memory limit at once, a slot of the
//! guard's in its app data ([`Slot`]) — which module it is, whose code runs in it, whether it was
//! stopped — and mlua's interrupt installed once and then switched off: its procedure is saved and
//! the VM's `lua_Callbacks.interrupt` set back to NULL, so a VM costs nothing while it is not
//! overdue. To stop it, the saved procedure is written back (Luau: "interrupt is safe to set from an
//! arbitrary thread", lua.h:452), and Luau calls it at its next safepoint — a loop's back edge, a
//! call, a return, a step of the pattern matcher. The way-A spike's mechanism
//! (`a-spike/src/arm.rs`); the guard writes the field only in a VM that is on the entry stack now
//! (the watchdog, under the slot's `on_stack` lock) or that is alive on the event loop's own thread
//! (the stop itself), so it never writes into a VM that is being freed.
//!
//! **Entries.** Every call from the host into a VM's Lua goes through an [`Entry`]: a handler's
//! delivery (`task::start_thread` and `task::resume_parked`, one entry from the stretch that starts
//! or resumes the handler until it ends or waits — the resumes after its own `coroutine.yield`
//! included, which the host answers at once, so they do not start its clocks afresh — with each
//! stretch an entry nested in it, `task::stretch`), a plain call (`call_guarded`: the arbiter's
//! `onActivate` and `onDeactivate`, a module's own `onChange` from its own `set`), and the three
//! steps of a load (`populate_vm`: a code dependency, the entry file, `activate`). With the
//! mailboxes, the mailbox runner's delivery is `mailbox::run` → `task::run_handler` →
//! `start_thread`, whose entry is the outermost. Entries nest — a hotkey's handler calls
//! `host.arbiter.setMatching`, which runs another VM's `onDeactivate` — and the outermost one is the
//! **slice** both clocks count: so a handler that waits ends its slice at the wait, and waiting never
//! counts.
//!
//! **The watchdog** (`vm-guard`, [`spawn_watchdog`]) looks every 50 ms ([`Guard::poll`]): the wall
//! clock, and the event loop's processor time from `thread_cpu.rs` — its baseline taken at its first
//! look at a slice, so the event loop makes no system call per callback. Past a budget it arms every
//! VM on the entry stack once per slice (a **round**), and keeps them armed while the round waits.
//! A VM whose interrupt sees the round takes it, unless it runs inside another VM's callback and has
//! run for less than half the budget that ran out itself: then the time is its caller's, and it
//! declines, so a quick `onDeactivate` is not stopped for the loop of the module that called
//! `setMatching`. The VM that takes the round samples its own stack at each safepoint for at least
//! [`SAMPLE_TIME`] and [`SAMPLE_MIN`] samples, and the innermost Luau function common to every sample
//! is the loop ([`name_chunk`] says whose file it is in); then it takes the round with a
//! compare-exchange and stops that code's module. That is a library's — a `code_module` whose code
//! runs in other VMs — only for a loop in Luau that spent the processor-time budget outside any host
//! call: then the library's own VM and every VM that runs its code are stopped in the same step, each
//! marked and armed so that it raises at its next safepoint and refuses every entry. Otherwise — the
//! wall clock, a slow host call, a loop in the host's own Luau — only the VM's own module is stopped.
//! A VM whose call returns while it is still taking its samples is stopped as that call's entry
//! ends ([`finish_samples`]), its own module only. The outer callback's clocks then start again, so
//! the next stop does not hit an innocent caller.
//!
//! **The stop.** When the outermost entry ends — the callback that began the slice has returned, and
//! no Lua runs any more — the trips go to the exit hook, which the application sets to `stops::mark`
//! (`Manager::new`): the modules are turned off, their keys go back, the mouse buttons they hold are
//! released, keys pressed during the stall are dropped, and the log says why, before anything else
//! runs; the rest, with the dialog, the sentence and the manager's row, at the end of the pump's
//! turn (`stops::settle`). A memory stop takes only the VM's own module: memory cannot be laid at the
//! door of the code that holds it. A stop while a module loads fails its load and stops nobody else.
//!
//! **What a module's `pcall` sees** of a time stop: the error [`STOPPED`], again at every safepoint
//! after it, so nothing escapes. One that catches its own memory error goes on: that is the module's
//! business. Its garbage stays until the collector reaches it — Luau has no emergency collection —
//! so the guard collects a VM whose last entry ended with less than an eighth of its limit free.
//!
//! **Naming the host call.** Every binding marks itself with [`host_call!`]: one atomic swap in and
//! one store out, so the messages can say "in host.screen.pixel" when the time ran out inside it (G3
//! will need the same marker to name a call that never returns).
//!
//! **Measuring.** The watchdog also charges the event loop's processor time, sampled at every look,
//! to the VM on top of the entry stack, and writes a `[cpu]` line for a module that takes a large
//! share of the loop or one long callback, at most once a minute, and a summary at exit. A
//! measurement; it changes nothing.
//!
//! **mlua is pinned to 0.11.6** (`crates/host/Cargo.toml`): the guard writes Luau's interrupt field
//! behind mlua's back, and `vm_guard_tests::mlua_pin` fails if mlua changes how it uses it.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use mlua::{Lua, VmState, WeakLua};
use module_manifest::DEFAULT_MEMORY_MIB;

use crate::thread_cpu::LoopThread;

/// Bytes in a MiB, the unit of `[limits] memory_mib`.
pub(crate) const MIB: u64 = 1 << 20;

/// The text a stopped VM's refused entry gives in place of the call's own error: never shown to the
/// user — the stop has its own report — but what a log line or a test sees.
pub(crate) const NOT_RUN: &str = "not run: the module was stopped by the host";

/// What a module's `pcall` sees of a time stop, at the trip and at every safepoint after it.
pub(crate) const STOPPED: &str = "stopped by the host: this callback ran too long, and the module is turned off";

/// A callback's budget of the event loop's processor time.
pub(crate) const CPU_BUDGET: Duration = Duration::from_secs(2);
/// A callback's budget on the wall clock, host calls and waiting for the processor included.
pub(crate) const WALL_BUDGET: Duration = Duration::from_secs(10);
/// How often the watchdog looks. The budgets are met up to one look late.
pub(crate) const POLL: Duration = Duration::from_millis(50);
/// The window of the `[cpu]` share line.
pub(crate) const CPU_WINDOW: Duration = Duration::from_secs(60);
/// The share of a window's time a module's callbacks take before the `[cpu]` line names it, in
/// percent. Provisional: set after the first measurements (TODO.md).
pub(crate) const CPU_LINE_PERCENT: u64 = 10;
/// One callback's processor time from which the `[cpu]` line names it. Provisional, like the share.
pub(crate) const CPU_LINE_CALLBACK: Duration = Duration::from_millis(250);
/// How many frames a trip keeps.
const FRAMES_MAX: usize = 16;
/// How deep a sample of the stack looks for the function common to every sample.
const SAMPLE_DEPTH: usize = 64;
/// The fewest samples of the stack the VM that takes a round takes before it decides whose code
/// loops, and the least time it spends on them (a real clock, the tests' too). A module's loop that
/// calls a helper of a library is in the helper at some samples and in the loop's own function at
/// all of them; one sample would lay a quarter to most of such loops at the library's door.
pub(crate) const SAMPLE_MIN: u32 = 16;
pub(crate) const SAMPLE_TIME: Duration = Duration::from_millis(10);
/// A VM's last entry ended with less than this share of its limit free — an eighth, and at least
/// 2 MiB — and it is collected in full then: Luau collects nothing in an emergency, and a module
/// that caught its own memory error would otherwise meet the limit again at its next allocation,
/// with the garbage of the loop that ran it out.
const COLLECT_HEADROOM_DIVISOR: u64 = 8;
const COLLECT_HEADROOM_MIN: u64 = 2 * MIB;

/// The two budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Budgets {
    pub cpu: Duration,
    pub wall: Duration,
}

impl Budgets {
    pub(crate) const APP: Budgets = Budgets { cpu: CPU_BUDGET, wall: WALL_BUDGET };
}

/// The two clocks: monotonic wall time, and the event loop's processor time.
pub(crate) trait Clocks: Send + Sync {
    fn wall_ns(&self) -> u64;
    fn loop_cpu(&self) -> Option<Duration>;
}

/// The application's clocks: `Instant`, and the processor time of the thread they were made on.
pub(crate) struct RealClocks {
    origin: Instant,
    loop_thread: Option<LoopThread>,
}

impl RealClocks {
    /// On the thread whose processor time counts: the event loop's.
    pub(crate) fn here() -> RealClocks {
        RealClocks { origin: Instant::now(), loop_thread: LoopThread::current() }
    }
}

impl Clocks for RealClocks {
    fn wall_ns(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
    fn loop_cpu(&self) -> Option<Duration> {
        self.loop_thread.as_ref().and_then(LoopThread::cpu)
    }
}

/// Which budget ran out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Clock {
    /// The event loop's processor time.
    Cpu,
    /// The wall clock.
    Wall,
}

/// What the watchdog saw when it armed a round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArmNote {
    pub clock: Clock,
    pub cpu: Duration,
    pub wall: Duration,
    /// The host call running then, if any.
    pub call: Option<&'static str>,
    /// The serial of the VM on top of the entry stack then: the host call is its, and only a stop
    /// of that VM names it.
    pub top: u64,
}

/// What one look of the watchdog came to: the round it armed, if any, and the lines to log.
#[derive(Debug, Default)]
pub(crate) struct Polled {
    pub armed: Option<ArmNote>,
    pub lines: Vec<(&'static str, String)>,
}

/// One per event-loop thread: the application's, or a test's. Holds every VM [`new_vm`] made, the
/// slice that runs now, and the trips noted in it.
pub(crate) struct Guard {
    budgets: Budgets,
    clocks: Box<dyn Clocks>,
    slots: Mutex<Vec<Arc<Slot>>>,
    next_serial: AtomicU64,
    trips: Mutex<Vec<Trip>>,
    /// Odd while a slice runs; +1 at its start and end, +2 when a stop restarts its clocks.
    slice_seq: AtomicU64,
    /// `wall_ns` at the slice's start or restart.
    slice_start_ns: AtomicU64,
    /// The slice a round is armed for; 0 when none waits to be taken.
    armed_seq: AtomicU64,
    /// The serial of the innermost VM entered now; 0 for none.
    top: AtomicU64,
    /// The serial of the VM whose entry began the slice.
    outer: AtomicU64,
    /// The host call the innermost entry's Lua is in now ([`host_call!`]); null for none.
    call: AtomicPtr<&'static str>,
    /// What the watchdog saw when it armed the round that waits.
    arm_note: Mutex<Option<ArmNote>>,
    /// The samples of the stack the VM that took the round is taking. The event loop's thread only.
    sampling: Mutex<Option<Sampling>>,
    /// The slice whose round a VM took last: the watchdog's line for a round nobody took.
    claimed_seq: AtomicU64,
    /// The watchdog's own bookkeeping and the processor-time samples.
    watch: Mutex<Watch>,
}

impl Guard {
    /// The application's: the budgets of 2 s and 10 s, and the clocks of the calling thread, which
    /// must be the event loop's.
    pub(crate) fn new() -> Arc<Guard> {
        Guard::with(Budgets::APP, Box::new(RealClocks::here()))
    }

    /// A guard with these budgets and clocks: the tests' fake ones.
    pub(crate) fn with(budgets: Budgets, clocks: Box<dyn Clocks>) -> Arc<Guard> {
        Arc::new(Guard {
            budgets,
            clocks,
            slots: Mutex::new(Vec::new()),
            next_serial: AtomicU64::new(0),
            trips: Mutex::new(Vec::new()),
            slice_seq: AtomicU64::new(0),
            slice_start_ns: AtomicU64::new(0),
            armed_seq: AtomicU64::new(0),
            top: AtomicU64::new(0),
            outer: AtomicU64::new(0),
            call: AtomicPtr::new(std::ptr::null_mut()),
            arm_note: Mutex::new(None),
            sampling: Mutex::new(None),
            claimed_seq: AtomicU64::new(0),
            watch: Mutex::new(Watch::default()),
        })
    }

    /// The slots of the VMs alive now: a slot only the guard still holds belongs to a VM that was
    /// dropped, and is let go here.
    fn live_slots(&self) -> Vec<Arc<Slot>> {
        let mut slots = lock(&self.slots);
        slots.retain(|s| Arc::strong_count(s) > 1);
        slots.clone()
    }

    /// The host call running now, as its binding named it.
    pub(crate) fn call_now(&self) -> Option<&'static str> {
        let p = self.call.load(Ordering::Acquire);
        // SAFETY: only `host_call!` stores here, the address of a `static &str`, or null.
        (!p.is_null()).then(|| unsafe { *p })
    }

    /// The id of the module whose VM has slot `serial`.
    fn module_of(&self, serial: u64) -> Option<String> {
        let slots = lock(&self.slots);
        let s = slots.iter().find(|s| s.serial == serial)?;
        let info = lock(&s.info);
        (!info.id.is_empty()).then(|| info.id.clone())
    }

    /// Writes the interrupt of every VM on the entry stack: armed, or — `false` — back to NULL for
    /// every one that is not stopped. Any thread: under each slot's `on_stack` lock, and only while
    /// it counts an entry, whose `Lua` keeps the VM alive.
    fn write_on_stack(&self, armed: bool) {
        for s in self.live_slots() {
            let n = lock(&s.on_stack);
            if *n > 0 && (armed || !s.stopped.load(Ordering::Acquire)) {
                s.write(armed);
            }
        }
    }

    /// One look of the watchdog. A pure function of the clocks and the slice, so the tests drive it
    /// with fake clocks and no thread. Arms a round once per slice when a budget is spent; keeps
    /// the fields of the VMs on the stack armed while that round waits, and switches off any that
    /// a round armed and nobody took.
    pub(crate) fn poll(&self) -> Polled {
        let now = self.clocks.wall_ns();
        let cpu = self.clocks.loop_cpu();
        let mut out = Polled::default();
        let top = self.top.load(Ordering::Acquire);
        let top_module = if top == 0 { None } else { self.module_of(top) };
        let seq = self.slice_seq.load(Ordering::Acquire);
        let outer_module = if seq % 2 == 1 { self.module_of(self.outer.load(Ordering::Acquire)) } else { None };
        let mut w = lock(&self.watch);
        // 1. The processor time since the last look, charged to the VM on top now.
        w.sample(now, cpu, top_module);
        // 2. The slice.
        if seq != w.seen {
            // A round armed for the slice that went, and nobody took it: the callback returned
            // before its stop — or before the VM that loops had sampled enough — and nothing was
            // stopped. Said, so the arm line above it is not read as a stop.
            if w.armed_for == Some(w.seen) && self.claimed_seq.load(Ordering::Acquire) != w.seen {
                out.lines.push(("guard", returned_line(w.slice_module.as_deref())));
            }
            w.end_slice(now, &mut out.lines);
            w.seen = seq;
            w.cpu_base = cpu;
            w.slice_module = outer_module;
            w.slice_used = Duration::ZERO;
        }
        let mut keep_armed = false;
        if seq % 2 == 1 {
            let used = cpu.zip(w.cpu_base).map(|(c, b)| c.saturating_sub(b)).unwrap_or_default();
            w.slice_used = used;
            let wall = Duration::from_nanos(now.saturating_sub(self.slice_start_ns.load(Ordering::Acquire)));
            if used >= self.budgets.cpu || wall >= self.budgets.wall {
                if w.armed_for != Some(seq) {
                    let note = ArmNote {
                        clock: if used >= self.budgets.cpu { Clock::Cpu } else { Clock::Wall },
                        cpu: used,
                        wall,
                        call: self.call_now(),
                        top,
                    };
                    *lock(&self.arm_note) = Some(note.clone());
                    self.armed_seq.store(seq, Ordering::Release);
                    w.armed_for = Some(seq);
                    out.lines.push(("guard", arm_line(w.slice_module.as_deref(), &note)));
                    out.armed = Some(note);
                }
                keep_armed = self.armed_seq.load(Ordering::Acquire) == seq;
            }
        }
        drop(w);
        // Armed again at every look while the round waits — a write that crossed the end of a slice
        // is made good 50 ms later — and switched off otherwise.
        self.write_on_stack(keep_armed);
        let mut w = lock(&self.watch);
        w.window(now, &mut out.lines);
        out
    }

    /// The `[cpu]` lines for the whole run, one per module that used the event loop: written at exit.
    pub(crate) fn cpu_summary(&self) -> Vec<String> {
        let now = self.clocks.wall_ns();
        let w = lock(&self.watch);
        let ran = Duration::from_nanos(now.saturating_sub(w.started_ns.unwrap_or(now)));
        let mut mods: Vec<(&String, &ModuleCpu)> = w.modules.iter().filter(|(_, m)| !m.total.is_zero()).collect();
        mods.sort_by(|a, b| b.1.total.cmp(&a.1.total).then(a.0.cmp(b.0)));
        mods.into_iter().map(|(id, m)| summary_line(id, m.total, ran, m.longest)).collect()
    }

    /// The trips noted and not yet taken — what an exit with no hook leaves, for the tests.
    #[cfg(test)]
    pub(crate) fn take_trips(&self) -> Vec<Trip> {
        std::mem::take(&mut *lock(&self.trips))
    }

    /// The slice counter, for the tests.
    #[cfg(test)]
    pub(crate) fn slice_seq(&self) -> u64 {
        self.slice_seq.load(Ordering::Acquire)
    }

    /// Arms a round for the slice that runs now, as `poll` does when a budget is spent, with no
    /// clock: for the tests, from a Rust function a script calls (`trip(); while true do end`).
    #[cfg(test)]
    pub(crate) fn arm_now(&self, clock: Clock) {
        let seq = self.slice_seq.load(Ordering::Acquire);
        if self.armed_seq.load(Ordering::Acquire) != seq {
            *lock(&self.arm_note) = Some(ArmNote {
                clock,
                cpu: self.budgets.cpu,
                wall: Duration::from_millis(2100),
                call: self.call_now(),
                top: self.top.load(Ordering::Acquire),
            });
            self.armed_seq.store(seq, Ordering::Release);
        }
        self.write_on_stack(true);
    }
}

/// A poisoned lock still holds data that is whole: nothing here panics while holding one.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The watchdog's bookkeeping, under `Guard::watch`.
#[derive(Default)]
struct Watch {
    /// The slice counter at the last look.
    seen: u64,
    /// The event loop's processor time at the first look at the slice.
    cpu_base: Option<Duration>,
    /// The slice a round was armed for: one round per slice, or per restart of one.
    armed_for: Option<u64>,
    last_cpu: Option<Duration>,
    started_ns: Option<u64>,
    window_start_ns: u64,
    modules: HashMap<String, ModuleCpu>,
    /// The module whose callback began the slice being watched, and its processor time so far.
    slice_module: Option<String>,
    slice_used: Duration,
}

/// One module's share of the event loop, as the watchdog sampled it.
#[derive(Default)]
struct ModuleCpu {
    window: Duration,
    total: Duration,
    longest_window: Duration,
    longest: Duration,
    /// When the line for one long callback was written last.
    callback_line_ns: Option<u64>,
}

impl Watch {
    fn sample(&mut self, now: u64, cpu: Option<Duration>, top: Option<String>) {
        if self.started_ns.is_none() {
            self.started_ns = Some(now);
            self.window_start_ns = now;
        }
        if let (Some(c), Some(l), Some(id)) = (cpu, self.last_cpu, top) {
            let d = c.saturating_sub(l);
            if !d.is_zero() {
                let m = self.modules.entry(id).or_default();
                m.window += d;
                m.total += d;
            }
        }
        self.last_cpu = cpu;
    }

    /// The slice that was watched has ended (or restarted): its longest callback, and the line for
    /// one long one.
    fn end_slice(&mut self, now: u64, lines: &mut Vec<(&'static str, String)>) {
        let (Some(id), used) = (self.slice_module.take(), self.slice_used) else { return };
        if used.is_zero() {
            return;
        }
        let m = self.modules.entry(id.clone()).or_default();
        m.longest = m.longest.max(used);
        m.longest_window = m.longest_window.max(used);
        let due = m.callback_line_ns.is_none_or(|t| now.saturating_sub(t) >= CPU_WINDOW.as_nanos() as u64);
        if used >= CPU_LINE_CALLBACK && due {
            m.callback_line_ns = Some(now);
            lines.push(("cpu", callback_line(&id, used)));
        }
    }

    /// At the end of a window: the line for each module above its share, and a new window.
    fn window(&mut self, now: u64, lines: &mut Vec<(&'static str, String)>) {
        let len = Duration::from_nanos(now.saturating_sub(self.window_start_ns));
        if len < CPU_WINDOW {
            return;
        }
        let mut ids: Vec<&String> = self.modules.keys().collect();
        ids.sort();
        for id in ids {
            let m = &self.modules[id];
            if m.window.as_nanos() * 100 >= len.as_nanos() * u128::from(CPU_LINE_PERCENT) {
                lines.push(("cpu", share_line(id, m.window, len, m.longest_window)));
            }
        }
        for m in self.modules.values_mut() {
            m.window = Duration::ZERO;
            m.longest_window = Duration::ZERO;
        }
        self.window_start_ns = now;
    }
}

/// Seconds to one decimal.
fn secs(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64())
}

/// `[cpu]` for a module above its share of a window.
pub(crate) fn share_line(id: &str, used: Duration, window: Duration, longest: Duration) -> String {
    let percent = used.as_nanos() * 100 / window.as_nanos().max(1);
    format!(
        "[{id}] used {} s of the event loop's processor time in the last {} s ({percent} %); {}. A measurement \
         for module authors and for deciding on threads per module; it changes nothing",
        secs(used),
        window.as_secs(),
        longest_words(longest)
    )
}

/// `[cpu]` for one long callback.
pub(crate) fn callback_line(id: &str, used: Duration) -> String {
    format!(
        "[{id}] one callback took {} s of the event loop's processor time. A measurement for module authors and \
         for deciding on threads per module; it changes nothing",
        secs(used)
    )
}

/// `[cpu]` at exit, for one module.
pub(crate) fn summary_line(id: &str, total: Duration, ran: Duration, longest: Duration) -> String {
    let per_mille = total.as_nanos() * 1000 / ran.as_nanos().max(1);
    format!(
        "[{id}] {} s of the event loop's processor time over {} of running ({}.{} %); {}",
        secs(total),
        running(ran),
        per_mille / 10,
        per_mille % 10,
        longest_words(longest)
    )
}

fn longest_words(longest: Duration) -> String {
    if longest < POLL * 2 {
        "none of its callbacks ran across two looks of the watchdog".to_string()
    } else {
        format!("its longest callback took {} s", secs(longest))
    }
}

/// "2 h 10 min", "7 min", "40 s".
fn running(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min", s / 60),
        _ => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

/// The `[guard]` line the watchdog writes when it arms a round.
pub(crate) fn arm_line(module: Option<&str>, n: &ArmNote) -> String {
    let who = module.map_or(String::new(), |m| format!("[{m}] "));
    let call = n.call.map_or(String::new(), |c| format!(", in {c}"));
    format!(
        "{who}a callback is past its limit ({} ms of processor time, {} ms in all{call}): the module whose code it \
         runs is stopped at its next Luau safepoint, unless the callback returns first",
        n.cpu.as_millis(),
        n.wall.as_millis()
    )
}

/// The `[guard]` line the watchdog writes when the slice a round was armed for ended and nobody
/// took the round: the callback returned first.
pub(crate) fn returned_line(module: Option<&str>) -> String {
    let who = module.map_or(String::new(), |m| format!("[{m}] "));
    format!("{who}the callback past its limit returned before it was stopped; nothing was stopped")
}

/// The watchdog thread, `vm-guard`: [`Guard::poll`] every [`POLL`], its lines to the log. Stops when
/// [`Watchdog::stop`] is called or the handle is dropped. Never runs module code.
pub(crate) struct Watchdog {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Starts the watchdog over `guard`. `None` when the system would not start a thread: the guard
/// then never arms, as before it existed, and the log says so.
pub(crate) fn spawn_watchdog(guard: Arc<Guard>) -> Watchdog {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let thread = std::thread::Builder::new()
        .name("vm-guard".into())
        .spawn(move || {
            while !s.load(Ordering::Acquire) {
                std::thread::park_timeout(POLL);
                if s.load(Ordering::Acquire) {
                    break;
                }
                for (category, line) in guard.poll().lines {
                    crate::logging::line(category, &line);
                }
            }
        });
    match thread {
        Ok(t) => Watchdog { stop, thread: Some(t) },
        Err(e) => {
            crate::logging::line("guard", &format!("the watchdog thread could not start ({e}): no callback can be stopped for running too long"));
            Watchdog { stop, thread: None }
        }
    }
}

impl Watchdog {
    /// Stops the thread and waits for it: at most one look.
    pub(crate) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.stop();
    }
}

/// What the guard keeps per VM.
pub(crate) struct Slot {
    serial: u64,
    /// Weak: the guard holds the slot.
    guard: Weak<Guard>,
    /// The address of `lua_Callbacks.interrupt` in this VM's global state; 0 when unknown.
    field: AtomicUsize,
    /// mlua's interrupt procedure, saved after `set_interrupt`; 0 when mlua installed none.
    proc_: AtomicUsize,
    /// How many entries of this VM are on the stack now. `field` is written under it.
    on_stack: Mutex<u32>,
    /// Sticky: every entry is refused, and the interrupt raises at every safepoint, until the
    /// module is turned on again ([`clear_module`]).
    stopped: AtomicBool,
    /// Between [`new_vm`] and the end of a successful load ([`loaded`]): a stop then fails the load
    /// and stops nobody else.
    loading: AtomicBool,
    /// The VM's memory limit in bytes, as last set.
    limit_bytes: AtomicU64,
    info: Mutex<VmInfo>,
    /// Chunk name → (module id, path in its folder), for the frames of a trip.
    chunks: Mutex<HashMap<String, (String, String)>>,
    /// The last trip of this VM, for a load that it failed and for the tests.
    last: Mutex<Option<Trip>>,
}

impl Slot {
    /// Writes the VM's interrupt field: mlua's procedure, or NULL. The caller holds `on_stack` and
    /// knows the VM is alive.
    fn write(&self, armed: bool) {
        let field = self.field.load(Ordering::Relaxed);
        if field == 0 {
            return;
        }
        let value = if armed { self.proc_.load(Ordering::Relaxed) } else { 0 };
        // SAFETY: `field` is the address of `lua_Callbacks.interrupt`, a pointer-sized, aligned
        // `Option<fn>` in the VM's global state, which the caller knows is alive; Luau documents it
        // as safe to set from any thread (lua.h:452), and 0 is `None`.
        unsafe { &*(field as *const AtomicUsize) }.store(value, Ordering::SeqCst);
    }

    /// Whether the VM's interrupt field is set now. On a VM the caller knows is alive.
    #[cfg(test)]
    fn armed(&self) -> bool {
        let field = self.field.load(Ordering::Relaxed);
        // SAFETY: as in `write`.
        field != 0 && unsafe { &*(field as *const AtomicUsize) }.load(Ordering::SeqCst) != 0
    }

    /// Marks the VM stopped and arms it, so it raises at its next safepoint. On the event loop's
    /// thread, where VMs are dropped: alive when on the stack or still held by somebody.
    fn mark_stopped(&self) {
        let n = lock(&self.on_stack);
        self.stopped.store(true, Ordering::Release);
        if *n > 0 || alive(self.serial).is_some() {
            self.write(true);
        }
    }
}

/// The slot in a VM's app data.
struct SlotRef(Arc<Slot>);

/// Whose VM it is and whose code runs in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VmInfo {
    /// The module whose VM it is; `None` until [`describe`] — a VM that never runs a module's code.
    pub module: Option<usize>,
    pub id: String,
    pub name: String,
    /// Whose code runs in it, and what each may use: the module itself first, then each code
    /// dependency, in the order they are evaluated into it.
    pub code: Vec<CodeShare>,
}

/// One module's share of a VM's memory limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodeShare {
    pub id: String,
    pub name: String,
    pub mib: u32,
    /// Whether its manifest sets `[limits] memory_mib`, rather than taking the default.
    pub declared: bool,
}

impl VmInfo {
    /// The VM's info for module `idx` and the code dependencies `populate_vm` collected.
    pub(crate) fn of(idx: usize, manifest: &module_manifest::ModuleManifest, deps: &[crate::capture_source::CodeDep]) -> VmInfo {
        let mut code = vec![CodeShare {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            mib: manifest.memory_mib(),
            declared: manifest.limits.memory_mib.is_some(),
        }];
        code.extend(deps.iter().map(|d| CodeShare {
            id: d.id.clone(),
            name: d.name.clone(),
            mib: d.memory_mib,
            declared: d.memory_declared,
        }));
        VmInfo { module: Some(idx), id: manifest.id.clone(), name: manifest.name.clone(), code }
    }

    /// The VM's limit in MiB: the sum over the code in it, each module once; the default for a VM
    /// nothing was described for.
    pub(crate) fn limit_mib(&self) -> u64 {
        if self.code.is_empty() {
            u64::from(DEFAULT_MEMORY_MIB)
        } else {
            self.code.iter().map(|c| u64::from(c.mib)).sum()
        }
    }

    /// The name of module `id` as this VM knows it: its own, or a code dependency's.
    pub(crate) fn name_of(&self, id: &str) -> Option<String> {
        if self.id == id {
            return Some(self.name.clone());
        }
        self.code.iter().find(|c| c.id == id).map(|c| c.name.clone())
    }
}

/// What the trips of a slice go to when its outermost entry ends.
pub(crate) type ExitHook = Rc<dyn Fn(Vec<Trip>)>;

thread_local! {
    /// The entries on this thread's stack, innermost last.
    static FRAMES: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
    /// What the trips of a slice go to when its outermost entry ends (`stops::mark` for the
    /// application). With none, they stay in the guard (the tests read them there).
    static EXIT_HOOK: RefCell<Option<ExitHook>> = const { RefCell::new(None) };
    /// Every VM by its slot's serial, weakly: a stopped VM's garbage is collected by serial at the
    /// settle, and a stop arms a VM that is not on the stack only while it is alive. The slot itself
    /// must stay `Sync` and cannot hold a `Lua`.
    static VMS: RefCell<HashMap<u64, WeakLua>> = RefCell::new(HashMap::new());
    /// The guard of this thread's event loop, for [`host_call!`] (`install`).
    static CURRENT: RefCell<Option<Arc<Guard>>> = const { RefCell::new(None) };
}

/// Sets what the trips of a slice go to on this thread (`Manager::new`: `stops::mark`).
pub(crate) fn set_exit_hook(hook: ExitHook) {
    EXIT_HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

/// Makes `guard` this thread's: the one [`host_call!`] notes the host call in.
pub(crate) fn install(guard: &Arc<Guard>) {
    CURRENT.with(|c| *c.borrow_mut() = Some(guard.clone()));
}

/// The VM with slot `serial`, if it is alive. The event loop's thread only.
fn alive(serial: u64) -> Option<Lua> {
    VMS.try_with(|v| v.try_borrow().ok().and_then(|v| v.get(&serial).and_then(WeakLua::try_upgrade))).ok().flatten()
}

/// A VM for a module, made the one way the host makes them: `Lua::new()`, the default memory
/// limit set at once, mlua's interrupt installed and switched off, and a slot of the guard's in its
/// app data. Nothing of a module has run in it.
pub(crate) fn new_vm(guard: &Arc<Guard>) -> Lua {
    let lua = Lua::new();
    // A state made by `Lua::new` always has mlua's allocator, so this does not fail; were it ever
    // to, the VM would only run without a limit.
    let _ = lua.set_memory_limit(bytes(u64::from(DEFAULT_MEMORY_MIB)));
    let slot = Arc::new(Slot {
        serial: guard.next_serial.fetch_add(1, Ordering::Relaxed) + 1,
        guard: Arc::downgrade(guard),
        field: AtomicUsize::new(0),
        proc_: AtomicUsize::new(0),
        on_stack: Mutex::new(0),
        stopped: AtomicBool::new(false),
        loading: AtomicBool::new(true),
        limit_bytes: AtomicU64::new(u64::from(DEFAULT_MEMORY_MIB) * MIB),
        info: Mutex::new(VmInfo::default()),
        chunks: Mutex::new(HashMap::new()),
        last: Mutex::new(None),
    });
    // mlua's interrupt, once, on this thread: what a trip raises through. Then saved and switched
    // off, so the VM runs at full speed until the watchdog writes it back.
    let field = interrupt_field(&lua);
    let s = slot.clone();
    lua.set_interrupt(move |lua| on_interrupt(&s, lua));
    if field != 0 {
        // SAFETY: as in `Slot::write`; the VM was made above and runs nothing yet.
        let proc_ = unsafe { &*(field as *const AtomicUsize) }.swap(0, Ordering::SeqCst);
        slot.field.store(field, Ordering::Relaxed);
        slot.proc_.store(proc_, Ordering::Relaxed);
        if proc_ == 0 {
            crate::logging::line("guard", "mlua did not install its interrupt; this VM cannot be stopped for running too long");
        }
    }
    // `try_`: a fresh state has nothing borrowed. A VM without a slot runs unguarded: entries pass
    // it through, and it has the default limit all the same.
    let _ = lua.try_set_app_data(SlotRef(slot.clone()));
    {
        let mut slots = lock(&guard.slots);
        slots.retain(|s| Arc::strong_count(s) > 1);
        slots.push(slot.clone());
    }
    VMS.with(|v| {
        let mut v = v.borrow_mut();
        v.retain(|_, w| w.try_upgrade().is_some());
        v.insert(slot.serial, lua.weak());
    });
    lua
}

/// The address of `lua_Callbacks.interrupt` in `lua`'s global state, or 0.
fn interrupt_field(lua: &Lua) -> usize {
    let mut field = 0usize;
    // SAFETY: `lua_callbacks` returns the global state's callbacks, which live as long as the VM;
    // only the field's address is taken here.
    let _ = unsafe {
        lua.exec_raw::<()>((), |state| {
            let cb = mlua::ffi::lua_callbacks(state);
            field = std::ptr::addr_of_mut!((*cb).interrupt) as usize;
        })
    };
    field
}

/// Bytes of `mib` MiB, as `set_memory_limit` takes them.
fn bytes(mib: u64) -> usize {
    usize::try_from(mib.saturating_mul(MIB)).unwrap_or(usize::MAX)
}

fn slot_of(lua: &Lua) -> Option<Arc<Slot>> {
    lua.try_app_data_ref::<SlotRef>().ok().flatten().map(|s| s.0.clone())
}

/// Records whose VM `lua` is and whose code runs in it, and sets its memory limit to the sum —
/// before any of that code runs. Writes the `[guard]` line that says the limit and its parts.
pub(crate) fn describe(lua: &Lua, info: VmInfo) {
    let limit = info.limit_mib();
    let _ = lua.set_memory_limit(bytes(limit));
    crate::logging::line("guard", &limit_line(&info));
    if let Some(slot) = slot_of(lua) {
        slot.limit_bytes.store(limit.saturating_mul(MIB), Ordering::Relaxed);
        *lock(&slot.info) = info;
    }
}

/// `[com.platform.melodyne] memory limit 512 MiB: 256 of its own, 256 for com.platform.overlay`.
pub(crate) fn limit_line(info: &VmInfo) -> String {
    let mut parts = Vec::new();
    for (i, c) in info.code.iter().enumerate() {
        let declared = if c.declared { " (set in its module.toml)" } else { "" };
        if i == 0 {
            parts.push(format!("{} of its own{declared}", c.mib));
        } else {
            parts.push(format!("{} for {}{declared}", c.mib, c.id));
        }
    }
    format!("[{}] memory limit {} MiB: {}", info.id, info.limit_mib(), parts.join(", "))
}

/// Records that the chunk named `chunk` in `lua` is module `id`'s file `rel` (its path in its
/// folder), so a trip's frames say `id/rel:line` and find whose code was running. Called with the
/// names `populate_vm` and `host.include` give the chunks they load.
pub(crate) fn name_chunk(lua: &Lua, chunk: &str, id: &str, rel: &str) {
    if let Some(slot) = slot_of(lua) {
        lock(&slot.chunks).insert(chunk.to_string(), (id.to_string(), rel.replace('\\', "/")));
    }
}

/// The load of `lua`'s module went through: a stop in it from now on stops the module.
pub(crate) fn loaded(lua: &Lua) {
    if let Some(slot) = slot_of(lua) {
        slot.loading.store(false, Ordering::Release);
    }
}

/// Whose VM `lua` is and whose code runs in it, as [`describe`] recorded it.
pub(crate) fn info(lua: &Lua) -> Option<VmInfo> {
    slot_of(lua).map(|s| lock(&s.info).clone())
}

/// Whether `lua`'s VM was stopped and not turned on again.
pub(crate) fn stopped(lua: &Lua) -> bool {
    slot_of(lua).is_some_and(|s| s.stopped.load(Ordering::Acquire))
}

/// Whether `lua`'s interrupt is armed now (the tests).
#[cfg(test)]
pub(crate) fn armed(lua: &Lua) -> bool {
    slot_of(lua).is_some_and(|s| s.armed())
}

/// The last trip of `lua`'s VM.
pub(crate) fn last_trip(lua: &Lua) -> Option<Trip> {
    slot_of(lua).and_then(|s| lock(&s.last).clone())
}

/// Module `idx` was turned on again: every VM of it takes entries again, and its interrupt is
/// switched off. The old VMs of the same index are dead or going; clearing them too is harmless.
pub(crate) fn clear_module(guard: &Guard, idx: usize) {
    for s in guard.live_slots() {
        if lock(&s.info).module == Some(idx) {
            let n = lock(&s.on_stack);
            s.stopped.store(false, Ordering::Release);
            if *n > 0 || alive(s.serial).is_some() {
                s.write(false);
            }
        }
    }
}

/// The modules other than module `id` whose VMs run its code, as (name, id), each once, by name.
pub(crate) fn carriers_of(guard: &Guard, id: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for s in guard.live_slots() {
        let info = lock(&s.info);
        if info.module.is_some() && info.id != id && info.code.iter().any(|c| c.id == id) && !out.iter().any(|(_, i)| *i == info.id) {
            out.push((info.name.clone(), info.id.clone()));
        }
    }
    out.sort();
    out
}

/// One full garbage collection of the VM with slot `serial`, if it is still alive: what a stopped
/// callback held in its locals is let go at once rather than at the VM's next cycle.
pub(crate) fn collect_garbage(serial: u64) {
    if let Some(lua) = alive(serial) {
        let _ = lua.gc_collect();
    }
}

/// What a call into a VM is, for the messages: an event's handler, a plain call of a callback, or
/// a step of loading a module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Handler,
    Plain,
    Load,
}

/// An entry not made: the VM was stopped.
#[derive(Debug)]
pub(crate) struct Refused;

struct Frame {
    slot: Arc<Slot>,
    kind: EntryKind,
    what: Cow<'static, str>,
    /// `wall_ns` when it was entered: how long a VM inside another's callback has run itself.
    entered_ns: u64,
}

/// One call from the host into a VM's Lua, from [`Entry::enter`] until it is dropped. Made around
/// the call itself; the call's result goes to [`Entry::finish`].
pub(crate) struct Entry {
    inner: Option<(Arc<Slot>, Lua)>,
    /// The host call the entry was made from, given back to the guard when it ends.
    prev_call: *mut &'static str,
}

impl Entry {
    /// Enters `lua`'s VM for a call of kind `kind`, named `what` as the messages name it ("hotkey",
    /// "arbiter onDeactivate", "its entry file"). Refused when the VM was stopped. For a VM no guard
    /// made — a test's — a pass-through that refuses nothing and notes nothing. The outermost entry
    /// starts the slice: its wall clock now, its processor time at the watchdog's first look.
    pub(crate) fn enter(lua: &Lua, kind: EntryKind, what: impl Into<Cow<'static, str>>) -> Result<Entry, Refused> {
        let none = Entry { inner: None, prev_call: std::ptr::null_mut() };
        let Some(slot) = slot_of(lua) else { return Ok(none) };
        if slot.stopped.load(Ordering::Acquire) {
            return Err(Refused);
        }
        // Everything that could fail or panic before the count goes up: once it is up, the entry
        // exists to bring it down again, so the watchdog never writes into a VM nobody holds.
        let what = what.into();
        let held = lua.clone();
        let guard = slot.guard.upgrade();
        let now = guard.as_ref().map_or(0, |g| g.clocks.wall_ns());
        let frame = Frame { slot: slot.clone(), kind, what, entered_ns: now };
        let outermost = FRAMES.with(|f| {
            let mut f = f.borrow_mut();
            f.push(frame);
            f.len() == 1
        });
        *lock(&slot.on_stack) += 1;
        let mut entry = Entry { inner: Some((slot, held)), prev_call: std::ptr::null_mut() };
        if let (Some(g), Some((slot, _))) = (&guard, &entry.inner) {
            g.top.store(slot.serial, Ordering::Release);
            // The host call the caller is in is not this VM's: inside the entry, none runs yet.
            entry.prev_call = g.call.swap(std::ptr::null_mut(), Ordering::AcqRel);
            if outermost {
                g.outer.store(slot.serial, Ordering::Release);
                g.slice_start_ns.store(now, Ordering::Release);
                g.slice_seq.fetch_add(1, Ordering::AcqRel);
            }
        }
        Ok(entry)
    }

    /// Looks at what the call came to: a memory error that reached the host stops the VM — mlua's
    /// own, or the bare `nil` error mlua sometimes makes of one ([`is_mangled_memory_error`]).
    pub(crate) fn finish<T>(&self, result: &mlua::Result<T>) {
        if let (Some((slot, lua)), Err(e)) = (&self.inner, result) {
            if is_memory_error(e) || is_mangled_memory_error(slot, lua, e) {
                memory_stop(slot, lua, e);
            }
        }
    }

    /// Whether the VM was stopped — during this call, or before it in the same slice.
    pub(crate) fn stopped(&self) -> bool {
        self.inner.as_ref().is_some_and(|(s, _)| s.stopped.load(Ordering::Acquire))
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        let Some((slot, lua)) = self.inner.take() else { return };
        // A VM still taking its samples for a round, whose call returns now: it took the round all
        // the same, and is stopped with the samples it has.
        if !std::thread::panicking() {
            finish_samples(&slot);
        }
        let (outermost, below) = FRAMES.with(|f| {
            let mut f = f.borrow_mut();
            f.pop();
            (f.is_empty(), f.last().map_or(0, |fr| fr.slot.serial))
        });
        let last_out = {
            let mut n = lock(&slot.on_stack);
            *n = n.saturating_sub(1);
            let last = *n == 0 && !slot.stopped.load(Ordering::Acquire);
            if last {
                slot.write(false);
            }
            last
        };
        let guard = slot.guard.upgrade();
        if let Some(g) = &guard {
            g.top.store(below, Ordering::Release);
            g.call.store(self.prev_call, Ordering::Release);
            if outermost {
                // The slice ends; a round nobody took goes with it, and so do the samples of one
                // that a VM was still taking.
                g.slice_seq.fetch_add(1, Ordering::AcqRel);
                g.armed_seq.store(0, Ordering::Release);
                g.outer.store(0, Ordering::Release);
                *lock(&g.sampling) = None;
            }
        }
        // The VM's last entry ended, and it is nearly full: what it caught of its own memory error
        // — the garbage of a loop that ran it out — goes now, not at its next allocation, which
        // would meet the limit again (Luau collects nothing in an emergency). No Lua of it runs.
        if last_out && !std::thread::panicking() {
            let limit = slot.limit_bytes.load(Ordering::Relaxed);
            let headroom = (limit / COLLECT_HEADROOM_DIVISOR).max(COLLECT_HEADROOM_MIN);
            if limit > 0 && limit.saturating_sub(lua.used_memory() as u64) < headroom {
                let _ = lua.gc_collect();
            }
        }
        // Not while a panic unwinds through the entry (a load's, which `load_module` catches): the
        // trips stay in the guard, for the next outermost entry's end or the end of the turn
        // ([`take_waiting_trips`]).
        if outermost && !std::thread::panicking() {
            if let Some(g) = &guard {
                let hook = EXIT_HOOK.with(|h| h.borrow().clone());
                if let Some(hook) = hook {
                    let trips = std::mem::take(&mut *lock(&g.trips));
                    if !trips.is_empty() {
                        // A panic in the hook is caught here: this runs in a destructor, on the
                        // event loop, where an unwinding panic would end the application. The hook
                        // keeps its own bookkeeping out of what can panic (`stops::mark`).
                        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook(trips)));
                        if let Err(p) = ran {
                            crate::logging::line("guard", &format!("the stop's first step failed: {}", crate::panic_text(&p)));
                        }
                    }
                }
            }
        }
        // Last: if this was the last clone of a VM a reload replaced, it closes only now, after the
        // slot's bookkeeping.
        drop(lua);
    }
}

/// `slot`'s last entry on the stack is ending while it takes samples for the round of the slice
/// that runs: a callback past its limit that returns before the samples are complete — one long
/// host call, then its end — is stopped as it would have been at its next safepoint, its own module
/// only, since too few samples cannot tell a library's loop from a module's.
fn finish_samples(slot: &Arc<Slot>) {
    let Some(g) = slot.guard.upgrade() else { return };
    let last = FRAMES.with(|f| f.try_borrow().is_ok_and(|f| f.iter().filter(|fr| Arc::ptr_eq(&fr.slot, slot)).count() == 1));
    if !last {
        return;
    }
    let seq = g.slice_seq.load(Ordering::Acquire);
    let s = {
        let mut sampling = lock(&g.sampling);
        if !sampling.as_ref().is_some_and(|s| s.owner == slot.serial && s.seq == seq) {
            return;
        }
        sampling.take()
    };
    let Some(s) = s else { return };
    if slot.stopped.load(Ordering::Acquire) || g.armed_seq.compare_exchange(seq, 0, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return;
    }
    claim(&g, slot, s);
}

/// The trips no outermost entry's end handed over — a panic unwound through it — taken at the end
/// of the turn, when no entry is on the stack: so a stopped module is turned off and reported even
/// then. Empty while an entry is on the stack.
pub(crate) fn take_waiting_trips(guard: &Guard) -> Vec<Trip> {
    let idle = FRAMES.with(|f| f.try_borrow().is_ok_and(|f| f.is_empty()));
    if !idle {
        return Vec::new();
    }
    std::mem::take(&mut *lock(&guard.trips))
}

/// A failure that reached the host outside a call — the coroutine of a handler that could not even
/// be made — looked at as if it had come from one: true when it stopped the VM (or the VM was
/// stopped already), and then the stop reports it, not the caller.
pub(crate) fn note_failure(lua: &Lua, kind: EntryKind, what: impl Into<Cow<'static, str>>, e: &mlua::Error) -> bool {
    match Entry::enter(lua, kind, what) {
        Err(Refused) => true,
        Ok(entry) => {
            entry.finish::<()>(&Err(e.clone()));
            entry.stopped()
        }
    }
}

/// Whether `e` is Luau running out of memory: mlua's `MemoryError`, also inside the wrappers a
/// Rust callback or a context put around it. The variants are mlua 0.11's; `vm_guard_tests` holds
/// them against a real limit.
pub(crate) fn is_memory_error(e: &mlua::Error) -> bool {
    match e {
        mlua::Error::MemoryError(_) => true,
        mlua::Error::CallbackError { cause, .. }
        | mlua::Error::WithContext { cause, .. }
        | mlua::Error::BadArgument { cause, .. } => is_memory_error(cause),
        _ => false,
    }
}

/// Whether `e` is a memory error mlua mangled: a bare `nil` error (`RuntimeError("<nil>")`) from a
/// VM that has less than a sixty-fourth of its limit free, or less than 256 KiB when that is more. Luau calls the
/// error handler of a protected call for a memory error too, and mlua's handler then returns
/// nothing, so the error that reaches the host — through a Rust callback whose own error could not
/// be built, say — is sometimes a run-time error with no value instead of a memory error
/// (`vm_guard_tests::mlua_pin`). A `nil` error from a VM that full is taken for what it is.
fn is_mangled_memory_error(slot: &Slot, lua: &Lua, e: &mlua::Error) -> bool {
    fn bare_nil(e: &mlua::Error) -> bool {
        match e {
            mlua::Error::RuntimeError(m) => m == "<nil>",
            mlua::Error::CallbackError { cause, .. }
            | mlua::Error::WithContext { cause, .. }
            | mlua::Error::BadArgument { cause, .. } => bare_nil(cause),
            _ => false,
        }
    }
    let limit = slot.limit_bytes.load(Ordering::Relaxed);
    bare_nil(e) && limit > 0 && limit.saturating_sub(lua.used_memory() as u64) < (limit / 64).max(256 << 10)
}

/// Whether `e` is the guard's time stop, as it reaches the host: the error the interrupt raised,
/// also inside the wrappers a Rust callback or a context put around it. The host itself asks the
/// VM whether it was stopped (`stopped`), not the error; the tests ask the error.
#[cfg(test)]
pub(crate) fn is_stop_error(e: &mlua::Error) -> bool {
    match e {
        mlua::Error::RuntimeError(m) => m == STOPPED,
        mlua::Error::CallbackError { cause, .. }
        | mlua::Error::WithContext { cause, .. }
        | mlua::Error::BadArgument { cause, .. } => is_stop_error(cause),
        _ => false,
    }
}

/// The traceback mlua kept when the error crossed a Rust callback — the innermost one — if any.
fn callback_traceback(e: &mlua::Error) -> Option<String> {
    match e {
        mlua::Error::CallbackError { cause, traceback } => callback_traceback(cause).or_else(|| Some(traceback.clone())),
        mlua::Error::WithContext { cause, .. } | mlua::Error::BadArgument { cause, .. } => callback_traceback(cause),
        _ => None,
    }
    .filter(|t| !t.trim().is_empty())
}

/// Why a VM was stopped.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cause {
    /// It needed more than its limit: `limit_mib`, with `used` bytes in use when the error reached
    /// the host.
    Memory { limit_mib: u64, used: u64 },
    /// A callback ran past a budget: which one, and how long it had run on each clock when the
    /// watchdog armed.
    Time { clock: Clock, cpu: Duration, wall: Duration },
}

/// The entry that began the slice, when the stopped VM's own entry ran inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outer {
    pub module: Option<usize>,
    pub id: String,
    pub name: String,
    pub kind: EntryKind,
    pub what: String,
    /// Whether it is the same VM: a module's own `onChange` inside its own `set`.
    pub same_vm: bool,
}

/// Where in a module's files a trip was: the innermost frame of a module's chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Location {
    pub id: String,
    pub rel: String,
    pub line: usize,
}

/// One VM a stop takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupMember {
    pub module: Option<usize>,
    pub id: String,
    pub name: String,
    /// It was loading: its load fails, and it is not turned off.
    pub loading: bool,
}

/// What the stop of a VM noted: which VM, why, in which call, inside which, and whom it takes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Trip {
    pub serial: u64,
    pub vm: VmInfo,
    /// It was loading: the load fails, and nobody is stopped.
    pub loading: bool,
    pub cause: Cause,
    /// The stopped VM's own entry: its kind and what the messages call it.
    pub kind: EntryKind,
    pub what: String,
    pub outer: Option<Outer>,
    /// mlua's traceback, when a memory error crossed a Rust callback; Luau gives none of its own for
    /// running out of memory.
    pub traceback: Option<String>,
    /// The host call the time ran out in, or had just returned from.
    pub call: Option<&'static str>,
    /// The stack at the trip, innermost first, as the messages write it.
    pub frames: Vec<String>,
    /// The innermost frame of a module's own chunk.
    pub location: Option<Location>,
    /// Whose code was running, as (id, name), when it is not the VM's own module's: a library's.
    pub culprit: Option<(String, String)>,
    /// Every VM the stop takes: the culprit's code wherever it runs, the tripping VM first.
    pub group: Vec<GroupMember>,
}

impl GroupMember {
    fn of(info: &VmInfo, loading: bool) -> GroupMember {
        GroupMember { module: info.module, id: info.id.clone(), name: info.name.clone(), loading }
    }
}

/// The stopped VM's own entry — its kind and name — and the entry that began the slice when it is
/// another, from the entry stack.
fn entry_context(slot: &Arc<Slot>) -> (EntryKind, String, Option<Outer>) {
    FRAMES.with(|f| {
        let Ok(f) = f.try_borrow() else { return (EntryKind::Plain, String::new(), None) };
        // This entry is the innermost frame of this slot; the first frame began the slice.
        let Some(own) = f.iter().rposition(|fr| Arc::ptr_eq(&fr.slot, slot)) else {
            return (EntryKind::Plain, String::new(), None);
        };
        let outer = (own > 0).then(|| {
            let first = &f[0];
            let info = lock(&first.slot.info);
            Outer {
                module: info.module,
                id: info.id.clone(),
                name: info.name.clone(),
                kind: first.kind,
                what: first.what.to_string(),
                same_vm: Arc::ptr_eq(&first.slot, slot),
            }
        });
        (f[own].kind, f[own].what.to_string(), outer)
    })
}

/// Stops the VM of `slot` for the memory error `e`, which reached the host at the innermost entry:
/// marks and arms it, and notes the trip for the end of the slice.
fn memory_stop(slot: &Arc<Slot>, lua: &Lua, e: &mlua::Error) {
    if slot.stopped.load(Ordering::Acquire) {
        return; // stopped already, earlier in the slice
    }
    slot.mark_stopped();
    let vm = lock(&slot.info).clone();
    let (kind, what, outer) = entry_context(slot);
    let loading = slot.loading.load(Ordering::Acquire);
    let trip = Trip {
        serial: slot.serial,
        loading,
        cause: Cause::Memory { limit_mib: vm.limit_mib(), used: lua.used_memory() as u64 },
        group: vec![GroupMember::of(&vm, loading)],
        vm,
        kind,
        what,
        outer,
        traceback: callback_traceback(e),
        call: None,
        frames: Vec::new(),
        location: None,
        culprit: None,
    };
    *lock(&slot.last) = Some(trip.clone());
    if let Some(g) = slot.guard.upgrade() {
        lock(&g.trips).push(trip);
    }
}

/// The error a stopped VM raises.
fn stop_error() -> mlua::Error {
    mlua::Error::runtime(STOPPED)
}

/// mlua's interrupt, on the event loop's thread, at a safepoint of `slot`'s VM after the field was
/// written: a stopped VM raises again (the stop is sticky); otherwise the VM's turn in the round
/// ([`round_turn`]): it samples its stack, takes the round and stops, or lets it pass.
fn on_interrupt(slot: &Arc<Slot>, lua: &Lua) -> mlua::Result<VmState> {
    if slot.stopped.load(Ordering::Acquire) {
        return Err(stop_error());
    }
    let Some(g) = slot.guard.upgrade() else { return Ok(VmState::Continue) };
    let seq = g.slice_seq.load(Ordering::Acquire);
    if seq % 2 == 1 && g.armed_seq.load(Ordering::Acquire) == seq {
        match round_turn(&g, slot, lua, seq) {
            Turn::Stop => return Err(stop_error()),
            // Armed still: the next safepoint is the next sample.
            Turn::Sample => return Ok(VmState::Continue),
            Turn::Pass => {}
        }
    }
    // A round that is not this slice's, one another VM samples, or one this VM declines: it runs
    // on, unarmed — the watchdog arms it again within one look if a round of this slice waits.
    let n = lock(&slot.on_stack);
    if !slot.stopped.load(Ordering::Acquire) {
        slot.write(false);
    }
    drop(n);
    Ok(VmState::Continue)
}

/// What a VM does with a round at one of its safepoints.
enum Turn {
    /// It took the round and stopped: raise.
    Stop,
    /// It samples its stack and runs on, armed.
    Sample,
    /// Not its round, or it declines it: it runs on, unarmed.
    Pass,
}

/// The samples of the stack the VM that took a round takes before it decides whose code loops.
struct Sampling {
    /// The slice the round is for.
    seq: u64,
    /// The VM that samples.
    owner: u64,
    began: Instant,
    count: u32,
    /// The first sample, innermost first: the frames the messages show.
    first: Vec<SampleFrame>,
    /// The Luau functions in every sample so far, by (chunk, line where the function begins).
    common: HashSet<(String, usize)>,
}

impl Sampling {
    fn new(seq: u64, owner: u64, first: Vec<SampleFrame>) -> Sampling {
        let common = first.iter().filter_map(|f| f.func.clone()).collect();
        Sampling { seq, owner, began: Instant::now(), count: 1, first, common }
    }

    fn add(&mut self, funcs: HashSet<(String, usize)>) {
        self.common.retain(|f| funcs.contains(f));
        self.count += 1;
    }

    fn done(&self) -> bool {
        self.count >= SAMPLE_MIN && self.began.elapsed() >= SAMPLE_TIME
    }
}

/// Whether the VM with slot `serial` has an entry on this thread's stack.
fn on_frames(serial: u64) -> bool {
    FRAMES.with(|f| f.try_borrow().map_or(true, |f| f.iter().any(|fr| fr.slot.serial == serial)))
}

/// Whether `slot`'s VM lets a round pass: it runs inside another VM's callback — it did not begin
/// the slice — and its own first entry on the stack is younger than half the budget that ran out.
/// The time is then mostly its caller's, whose Luau takes the round when this call returns: a quick
/// `onDeactivate` is not stopped for the loop of the module whose `setMatching` ran it. One that
/// loops itself ages past half the budget and takes it then.
fn declines(g: &Guard, slot: &Arc<Slot>) -> bool {
    let clock = lock(&g.arm_note).as_ref().map_or(Clock::Cpu, |n| n.clock);
    let half = match clock {
        Clock::Cpu => g.budgets.cpu / 2,
        Clock::Wall => g.budgets.wall / 2,
    };
    let now = g.clocks.wall_ns();
    FRAMES.with(|f| {
        let Ok(f) = f.try_borrow() else { return false };
        let Some(first) = f.first() else { return false };
        if Arc::ptr_eq(&first.slot, slot) {
            return false;
        }
        let Some(own) = f.iter().find(|fr| Arc::ptr_eq(&fr.slot, slot)) else { return false };
        Duration::from_nanos(now.saturating_sub(own.entered_ns)) < half
    })
}

/// `slot`'s VM at a safepoint while a round of slice `seq` waits: the VM that samples adds a
/// sample, and once it has enough takes the round with a compare-exchange and stops; with nobody
/// sampling, a VM that does not decline begins to. Another VM runs only while the one that samples
/// waits for it further down the stack: a quick one lets the round pass, and one that has run long
/// enough not to decline samples in its place — a callee that loops is not shielded by its caller.
fn round_turn(g: &Arc<Guard>, slot: &Arc<Slot>, lua: &Lua, seq: u64) -> Turn {
    let mut sampling = lock(&g.sampling);
    // The samples of an older slice, or of a VM whose call has returned, are given up: the round
    // goes to whoever runs now.
    if sampling.as_ref().is_some_and(|s| s.seq != seq || !on_frames(s.owner)) {
        *sampling = None;
    }
    match sampling.as_mut() {
        Some(s) if s.owner != slot.serial => {
            if declines(g, slot) {
                return Turn::Pass;
            }
            let chunks = lock(&slot.chunks).clone();
            *s = Sampling::new(seq, slot.serial, sample_stack(lua, &chunks));
            return Turn::Sample;
        }
        Some(s) => {
            s.add(stack_functions(lua));
            if !s.done() {
                return Turn::Sample;
            }
        }
        None => {
            if declines(g, slot) {
                return Turn::Pass;
            }
            let chunks = lock(&slot.chunks).clone();
            *sampling = Some(Sampling::new(seq, slot.serial, sample_stack(lua, &chunks)));
            return Turn::Sample;
        }
    }
    let Some(s) = sampling.take() else { return Turn::Pass };
    drop(sampling);
    if g.armed_seq.compare_exchange(seq, 0, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return Turn::Pass;
    }
    claim(g, slot, s);
    Turn::Stop
}

/// The VM of `slot` took the round with these samples: finds whose code loops, and stops that
/// code's module — a library's wherever its code runs, when the rule allows — in one step; then
/// restarts the slice's clocks.
fn claim(g: &Arc<Guard>, slot: &Arc<Slot>, s: Sampling) {
    g.claimed_seq.store(s.seq, Ordering::Release);
    let note = lock(&g.arm_note).take().unwrap_or(ArmNote {
        clock: Clock::Wall,
        cpu: Duration::ZERO,
        wall: Duration::ZERO,
        call: None,
        top: slot.serial,
    });
    // The host call was this VM's only when it was on top as the round was armed.
    let call = if note.top == slot.serial { note.call } else { None };
    let vm = lock(&slot.info).clone();
    let loading = slot.loading.load(Ordering::Acquire);
    let (code, location) = whose_code(&s);
    // A library's code — every VM that runs it — only for a loop in Luau that spent the processor
    // time itself, found by a whole set of samples: the wall clock and a host call are one window's
    // or one plug-in's, not code that every dependent shares. A load stops nobody else.
    let library_rule = s.done() && !loading && note.clock == Clock::Cpu && call.is_none();
    let code_id = code.filter(|_| library_rule).map(|l| l.id).filter(|id| !id.is_empty());
    let culprit = code_id.as_ref().filter(|id| **id != vm.id).map(|id| {
        let name = vm.name_of(id).unwrap_or_else(|| id.clone());
        (id.clone(), name)
    });
    let all = g.live_slots();
    let group: Vec<Arc<Slot>> = match &code_id {
        None => vec![slot.clone()],
        Some(code_id) => {
            let mut v = vec![slot.clone()];
            for other in &all {
                if !Arc::ptr_eq(other, slot) && lock(&other.info).code.iter().any(|c| c.id == *code_id) {
                    v.push(other.clone());
                }
            }
            v
        }
    };
    for m in &group {
        m.mark_stopped();
    }
    // The rest of the stack runs on at full speed.
    for other in &all {
        if !group.iter().any(|m| Arc::ptr_eq(m, other)) {
            let n = lock(&other.on_stack);
            if *n > 0 && !other.stopped.load(Ordering::Acquire) {
                other.write(false);
            }
        }
    }
    // The outer callback starts afresh: a new slice to the watchdog, with its own budgets.
    g.slice_start_ns.store(g.clocks.wall_ns(), Ordering::Release);
    g.slice_seq.fetch_add(2, Ordering::AcqRel);
    let (kind, what, outer) = entry_context(slot);
    let trip = Trip {
        serial: slot.serial,
        loading,
        cause: Cause::Time { clock: note.clock, cpu: note.cpu, wall: note.wall },
        group: group
            .iter()
            .map(|m| GroupMember::of(&lock(&m.info), m.loading.load(Ordering::Acquire)))
            .collect(),
        vm,
        kind,
        what,
        outer,
        traceback: None,
        call,
        frames: s.first.iter().take(FRAMES_MAX).map(|f| f.text.clone()).collect(),
        location,
        culprit,
    };
    *lock(&slot.last) = Some(trip.clone());
    lock(&g.trips).push(trip);
}

/// Whose code loops, from the samples: the innermost Luau function of the first sample that is in
/// every sample — the loop itself, where a helper it calls comes and goes. Its module's file and
/// line when it is a module's chunk; `None` when it is the host's own Luau or another chunk, or no
/// Luau function is in every sample (a loop that resumes coroutines of its own). And where the
/// messages place the stop: that frame, or else the innermost frame of a module's file in the first
/// sample, for the module's author.
fn whose_code(s: &Sampling) -> (Option<Location>, Option<Location>) {
    let at = |f: &SampleFrame| match (&f.module, f.line) {
        (Some((id, rel)), Some(line)) => Some(Location { id: id.clone(), rel: rel.clone(), line }),
        _ => None,
    };
    let code = s.first.iter().find(|f| f.func.as_ref().is_some_and(|func| s.common.contains(func))).and_then(at);
    let location = code.clone().or_else(|| s.first.iter().find_map(at));
    (code, location)
}

/// The chunks the host owns, written `(host) name:line` in a trip's frames.
const HOST_CHUNKS: [&str; 2] = ["window_prelude", crate::task::SHIM_NAME];

/// One frame of a sample of the stack.
struct SampleFrame {
    /// Its words in the messages.
    text: String,
    /// The module file it is in, as (id, path in its folder).
    module: Option<(String, String)>,
    line: Option<usize>,
    /// The Luau function, as (chunk, line where it begins); `None` for a C function.
    func: Option<(String, usize)>,
}

/// The stack of the running coroutine of `lua`, innermost first, [`SAMPLE_DEPTH`] frames at most.
/// In the interrupt, mlua's state is the interrupted coroutine, so level 0 is the code that ran.
fn sample_stack(lua: &Lua, chunks: &HashMap<String, (String, String)>) -> Vec<SampleFrame> {
    let mut out = Vec::new();
    for level in 0..SAMPLE_DEPTH {
        let Some(f) = raw_frame(lua, level) else { break };
        let (text, module) = frame_text(&f, chunks);
        out.push(SampleFrame { text, module, line: f.line, func: func_of(&f) });
    }
    out
}

/// The Luau functions of the running coroutine's stack: a later sample, which needs no words.
fn stack_functions(lua: &Lua) -> HashSet<(String, usize)> {
    (0..SAMPLE_DEPTH).map_while(|level| raw_frame(lua, level)).filter_map(|f| func_of(&f)).collect()
}

fn raw_frame(lua: &Lua, level: usize) -> Option<RawFrame> {
    lua.inspect_stack(level, |d| {
        let src = d.source();
        RawFrame {
            source: src.source.map(|s| s.into_owned()),
            what: src.what,
            line: d.current_line(),
            defined: src.line_defined,
            name: d.names().name.map(|n| n.into_owned()),
        }
    })
}

/// A Luau function's identity across samples: its chunk and the line it begins on.
fn func_of(f: &RawFrame) -> Option<(String, usize)> {
    if f.what == "C" {
        return None;
    }
    Some((f.source.clone().unwrap_or_default(), f.defined.unwrap_or(0)))
}

/// One frame as `inspect_stack` gives it.
pub(crate) struct RawFrame {
    pub source: Option<String>,
    pub what: &'static str,
    pub line: Option<usize>,
    pub defined: Option<usize>,
    pub name: Option<String>,
}

/// One frame's words, and the module chunk it is in:
/// - `com.example.game/src/presets.luau:236: in function 'ratio'` — a module's chunk;
/// - `[C]: in function 'upper'` — a C function;
/// - `(host) window_prelude:120: in function '_dispatchActivate'` — a chunk the host owns;
/// - `name:12: in function <name:10>` — any other chunk.
pub(crate) fn frame_text(f: &RawFrame, chunks: &HashMap<String, (String, String)>) -> (String, Option<(String, String)>) {
    if f.what == "C" {
        return (format!("[C]: in {}", f.name.as_deref().map_or("?".to_string(), |n| format!("function '{n}'"))), None);
    }
    let source = f.source.clone().unwrap_or_default();
    let (place, module) = match chunks.get(&source) {
        Some((id, rel)) => (format!("{id}/{rel}"), Some((id.clone(), rel.clone()))),
        None => {
            let short = source.trim_start_matches(['=', '@']).to_string();
            if HOST_CHUNKS.contains(&source.as_str()) {
                (format!("(host) {short}"), None)
            } else {
                (short, None)
            }
        }
    };
    let line = f.line.map_or(String::new(), |l| format!(":{l}"));
    let func = match (&f.name, f.what) {
        (Some(n), _) => format!("function '{n}'"),
        (None, "main") => "the file's top level".to_string(),
        (None, _) => format!("function <{place}:{}>", f.defined.unwrap_or(0)),
    };
    (format!("{place}{line}: in {func}"), module)
}

/// Notes the host call a binding runs, for the guard: the stop's messages say "in host.screen.pixel"
/// when the time ran out inside it (and G3 will name a call that never returns). One atomic swap in,
/// one store out; nothing when no guard is installed on the thread (the tests' VMs). The first
/// statement of every closure Luau can call; `lib.rs`'s `every_binding_names_its_host_call` holds
/// every binding to it.
macro_rules! host_call {
    ($name:literal) => {
        let _host_call = {
            static NAME: &str = $name;
            $crate::vm_guard::HostCall::enter(&NAME)
        };
    };
}
pub(crate) use host_call;

/// The marker [`host_call!`] leaves for the length of a binding.
pub(crate) struct HostCall {
    prev: Option<*mut &'static str>,
}

impl HostCall {
    /// Notes `name` as the host call running now, until the marker is dropped.
    #[inline]
    pub(crate) fn enter(name: &'static &'static str) -> HostCall {
        let p = name as *const &'static str as *mut &'static str;
        let prev = CURRENT.try_with(|c| c.try_borrow().ok().and_then(|g| g.as_ref().map(|g| g.call.swap(p, Ordering::AcqRel)))).ok().flatten();
        HostCall { prev }
    }
}

impl Drop for HostCall {
    #[inline]
    fn drop(&mut self) {
        if let Some(prev) = self.prev {
            let _ = CURRENT.try_with(|c| {
                if let Some(g) = c.try_borrow().ok().as_deref().and_then(Option::as_ref) {
                    g.call.store(prev, Ordering::Release);
                }
            });
        }
    }
}

/// A VM described for module `idx`, for the tests: `code` as (id, MiB).
#[cfg(test)]
pub(crate) fn describe_for_test(lua: &Lua, idx: usize, id: &str, code: &[(&str, u32)]) {
    describe(
        lua,
        VmInfo {
            module: Some(idx),
            id: id.to_string(),
            name: format!("Module {id}"),
            code: code
                .iter()
                .map(|(i, mib)| CodeShare { id: i.to_string(), name: format!("Module {i}"), mib: *mib, declared: *mib != DEFAULT_MEMORY_MIB })
                .collect(),
        },
    );
}

/// Drops this thread's exit hook and guard, for a test that installed them.
#[cfg(test)]
pub(crate) fn clear_exit_hook() {
    EXIT_HOOK.with(|h| *h.borrow_mut() = None);
    CURRENT.with(|c| *c.borrow_mut() = None);
}
