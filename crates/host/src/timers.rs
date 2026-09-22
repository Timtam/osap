//! `host.timer`: one-shot and recurring callbacks, fired from the event-loop tick, each with a
//! token its module can cancel it by.
//!
//! The timers used to have no identity at all. `after` and `every` returned nothing, so a poll
//! could only be stopped by returning early from inside it, and a runtime that several game
//! modules depend on armed one poll per VM that ran its code with no way to take any of them
//! back. A token is a number rather than a userdata handle on purpose: every existing caller
//! throws the return value away, and a handle that cancelled its timer when it was collected
//! would have stopped every one of those timers at the next garbage collection.
//!
//! Kept in a file of its own, with the bindings generic over whatever holds the [`Timers`], so
//! the firing rules can be tested against a real Luau VM without building the whole host.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, RegistryKey, Table, Value};

/// A one-shot timer: fired once at or after `deadline`, then gone.
struct Once {
    token: i64,
    deadline: Instant,
    /// The module that owns it — the VM's owner, for a dependency's code too.
    idx: usize,
    lua: Lua,
    cb: RegistryKey,
}

/// A recurring timer: fired at `next`, then re-armed `interval` later — also while its module
/// is disabled, so a poll resumes on re-enable instead of dying (a self-rescheduling `after`
/// chain does die: its reschedule is skipped with the callback).
struct Every {
    token: i64,
    next: Instant,
    interval: Duration,
    idx: usize,
    lua: Lua,
    cb: RegistryKey,
}

/// Every pending timer of every module. Main-thread only, like the rest of `Shared`.
#[derive(Default)]
pub(crate) struct Timers {
    once: RefCell<Vec<Once>>,
    every: RefCell<Vec<Every>>,
    /// The last token handed out. Tokens are positive, unique for the life of the process and
    /// never reused, so a stale token cannot cancel a timer somebody armed later.
    next_token: Cell<i64>,
}

impl Timers {
    fn token(&self) -> i64 {
        let t = self.next_token.get() + 1;
        self.next_token.set(t);
        t
    }

    /// Arms a one-shot timer owned by module `idx` and returns its token.
    pub(crate) fn after(&self, lua: &Lua, idx: usize, delay: Duration, cb: Function, now: Instant) -> mlua::Result<i64> {
        let cb = lua.create_registry_value(cb)?;
        let token = self.token();
        self.once.borrow_mut().push(Once { token, deadline: now + delay, idx, lua: lua.clone(), cb });
        Ok(token)
    }

    /// Arms a recurring timer owned by module `idx` and returns its token. An interval below
    /// one millisecond counts as one.
    pub(crate) fn every(&self, lua: &Lua, idx: usize, interval: Duration, cb: Function, now: Instant) -> mlua::Result<i64> {
        let cb = lua.create_registry_value(cb)?;
        let interval = interval.max(Duration::from_millis(1));
        let token = self.token();
        self.every
            .borrow_mut()
            .push(Every { token, next: now + interval, interval, idx, lua: lua.clone(), cb });
        Ok(token)
    }

    /// Removes the pending timer `token` if module `idx` owns it, and says whether it did.
    ///
    /// Only the owner's: a token is a small number, and one module must not be able to stop
    /// another's poll by guessing it — the rule `host.gamepad.off` follows too.
    pub(crate) fn cancel(&self, idx: usize, token: i64) -> bool {
        let once = {
            let mut once = self.once.borrow_mut();
            once.iter()
                .position(|t| t.token == token && t.idx == idx)
                .map(|i| once.remove(i))
        };
        if let Some(t) = once {
            let _ = t.lua.remove_registry_value(t.cb);
            return true;
        }
        let every = {
            let mut every = self.every.borrow_mut();
            every
                .iter()
                .position(|t| t.token == token && t.idx == idx)
                .map(|i| every.remove(i))
        };
        if let Some(t) = every {
            let _ = t.lua.remove_registry_value(t.cb);
            return true;
        }
        false
    }

    /// Keeps only the timers of the modules `keep` answers true for — `purge_module` (one
    /// index) and `rollback_to` (a suffix).
    pub(crate) fn retain(&self, keep: impl Fn(usize) -> bool) {
        self.once.borrow_mut().retain(|t| keep(t.idx));
        self.every.borrow_mut().retain(|t| keep(t.idx));
    }

    /// Whether any timer is pending — a reason for the headless loop to keep running.
    pub(crate) fn is_empty(&self) -> bool {
        self.once.borrow().is_empty() && self.every.borrow().is_empty()
    }

    /// Fires every timer due at `now`: the one-shot ones first, then the recurring ones, each
    /// group in the order it was armed.
    ///
    /// **No list is borrowed while a callback runs.** A callback may arm, and cancel, timers —
    /// the usual way to stop a poll is from inside it — so the due timers are collected as
    /// TOKENS, and each is looked up again just before it is called. One that an earlier
    /// callback of the same tick cancelled is gone by then and is skipped, which is what makes
    /// "stop the other timer" work when both were due together. A timer armed during the tick
    /// is not among the collected tokens, so it waits for a later tick, whatever its delay.
    ///
    /// `enabled` says whether a module's callbacks may run; a one-shot timer that comes due
    /// while its module is disabled is discarded uncalled, and a recurring one is re-armed
    /// uncalled. `before_once` runs once, before the first one-shot callback, when any one-shot
    /// timer was due (the host turns its epoch over there). `failed` hears every callback error.
    pub(crate) fn fire_due(
        &self,
        now: Instant,
        enabled: impl Fn(usize) -> bool,
        before_once: impl FnOnce(),
        mut failed: impl FnMut(usize, &str),
    ) {
        let due_once: Vec<i64> =
            self.once.borrow().iter().filter(|t| t.deadline <= now).map(|t| t.token).collect();
        if !due_once.is_empty() {
            before_once();
        }
        for token in due_once {
            let taken = {
                let mut once = self.once.borrow_mut();
                once.iter().position(|t| t.token == token).map(|i| once.remove(i))
            };
            let Some(t) = taken else { continue };
            if enabled(t.idx) {
                if let Ok(f) = t.lua.registry_value::<Function>(&t.cb) {
                    if let Err(e) = crate::call_guarded(&f, ()) {
                        failed(t.idx, &e);
                    }
                }
            }
            let _ = t.lua.remove_registry_value(t.cb);
        }

        let due_every: Vec<i64> =
            self.every.borrow().iter().filter(|t| t.next <= now).map(|t| t.token).collect();
        for token in due_every {
            let found = {
                let mut every = self.every.borrow_mut();
                every.iter_mut().find(|t| t.token == token).map(|t| {
                    t.next = now + t.interval;
                    (t.idx, t.lua.registry_value::<Function>(&t.cb).ok())
                })
            };
            let Some((idx, Some(f))) = found else { continue };
            if enabled(idx) {
                if let Err(e) = crate::call_guarded(&f, ()) {
                    failed(idx, &e);
                }
            }
        }
    }
}

/// A token as `cancel` receives it: a whole number, or nothing. Anything else — `nil`, a
/// string, a fraction — is no token of ours, and cancelling it is simply false, never an
/// error: a module that cancels "whatever poll is running" should not have to test for nil.
fn token_of(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(i) => Some(*i),
        Value::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Some(*n as i64),
        _ => None,
    }
}

/// Builds the `host.timer` table for module `idx` — the module that OWNS what is armed
/// through it, which for a dependency's code is the VM's owner (`build_dep_host` hands a
/// dependency the owner's `timer` table).
///
/// `get` reaches the [`Timers`] inside whatever `holder` is, and `now` is the clock a timer is
/// armed by: the host's `Shared` and `Instant::now`, or, in the tests, a holder with a clock of
/// its own — so a test arms and fires by ONE clock, and what is due does not depend on how
/// long the machine took to run the chunk.
pub(crate) fn table<S: 'static>(
    lua: &Lua,
    idx: usize,
    holder: Rc<S>,
    get: fn(&S) -> &Timers,
    now: fn(&S) -> Instant,
) -> mlua::Result<Table> {
    let timer = lua.create_table()?;
    let h = holder.clone();
    timer.set(
        "after",
        lua.create_function(move |lua, (ms, cb): (u64, Function)| {
            get(&h).after(lua, idx, Duration::from_millis(ms), cb, now(&h))
        })?,
    )?;
    let h = holder.clone();
    timer.set(
        "every",
        // Recurring. Unlike a self-rescheduling `after`, this survives a disable and enable
        // (the host re-arms it), so it is the right tool for a poll — a library's landmark
        // appearing, a game's menu cursor moving.
        lua.create_function(move |lua, (ms, cb): (u64, Function)| {
            get(&h).every(lua, idx, Duration::from_millis(ms), cb, now(&h))
        })?,
    )?;
    let h = holder;
    timer.set(
        "cancel",
        lua.create_function(move |_, token: Value| {
            Ok(token_of(&token).is_some_and(|t| get(&h).cancel(idx, t)))
        })?,
    )?;
    Ok(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The timers and the clock they are armed by, for the tests: the clock stands at `t0`
    /// until a tick moves it, so a timer armed from Luau is armed at a known instant and the
    /// tick that must find it due does, however slowly the machine ran the chunk. (With the
    /// real clock, a loaded machine could arm `every(5)` more than 5 ms after `t0`, and a tick
    /// at `t0 + 10 ms` would find nothing due.)
    struct Clocked {
        timers: Timers,
        t0: Instant,
        now: Cell<Instant>,
    }

    fn clocked() -> Rc<Clocked> {
        let t0 = Instant::now();
        Rc::new(Clocked { timers: Timers::default(), t0, now: Cell::new(t0) })
    }

    /// A VM with `host.timer` for module `idx`, over `c`'s timers and clock.
    fn vm(c: &Rc<Clocked>, idx: usize) -> Lua {
        let lua = Lua::new();
        let host = lua.create_table().unwrap();
        host.set("timer", table(&lua, idx, c.clone(), |c| &c.timers, |c| c.now.get()).unwrap())
            .unwrap();
        lua.globals().set("host", host).unwrap();
        lua
    }

    /// Moves the clock to `ms` after `t0` and fires what is due then — a callback that arms a
    /// timer arms it at that instant too.
    fn fire(
        c: &Clocked,
        ms: u64,
        enabled: impl Fn(usize) -> bool,
        before_once: impl FnOnce(),
        failed: impl FnMut(usize, &str),
    ) {
        let at = c.t0 + Duration::from_millis(ms);
        c.now.set(at);
        c.timers.fire_due(at, enabled, before_once, failed);
    }

    /// [`fire`] with every module enabled, no epoch hook and no callback allowed to fail.
    fn tick(c: &Clocked, ms: u64) {
        fire(c, ms, |_| true, || {}, |_, e| panic!("{e}"));
    }

    fn get_i(lua: &Lua, name: &str) -> i64 {
        lua.globals().get::<i64>(name).unwrap()
    }

    /// Evaluates `code` and insists on a real `true`. mlua reads `nil` as `false`, so a test
    /// that reads a global as a bool cannot tell "answered false" from "never ran".
    fn is_true(lua: &Lua, code: &str) -> bool {
        matches!(lua.load(code).eval::<Value>().unwrap(), Value::Boolean(true))
    }

    #[test]
    fn tokens_are_unique_positive_and_never_reused() {
        let c = clocked();
        let lua = vm(&c, 0);
        let (a, b, d): (i64, i64, i64) = lua
            .load(
                r#"
                local a = host.timer.after(0, function() end)
                local b = host.timer.every(10, function() end)
                host.timer.cancel(a)
                local d = host.timer.after(0, function() end)
                return a, b, d
                "#,
            )
            .eval()
            .unwrap();
        assert!(a > 0 && b > a && d > b, "{a} {b} {d}");
    }

    #[test]
    fn a_pending_after_is_cancelled_and_never_fires() {
        let c = clocked();
        let lua = vm(&c, 0);
        let first: bool = lua
            .load(
                r#"
                fired = 0
                local t = host.timer.after(0, function() fired = fired + 1 end)
                local first = host.timer.cancel(t)
                again = host.timer.cancel(t)
                return first
                "#,
            )
            .eval()
            .unwrap();
        assert!(first, "cancelling a pending timer says so");
        assert!(is_true(&lua, "return again == false"), "a second cancel finds nothing");
        tick(&c, 1000);
        assert_eq!(get_i(&lua, "fired"), 0);
        assert!(c.timers.is_empty());
    }

    #[test]
    fn an_every_stops_when_cancelled_including_from_its_own_callback() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            r#"
            runs, own = 0, nil
            poll = host.timer.every(10, function()
              runs = runs + 1
              if runs == 3 then own = host.timer.cancel(poll) end
            end)
            "#,
        )
        .exec()
        .unwrap();
        for step in 1..=10 {
            tick(&c, step * 10);
        }
        assert_eq!(get_i(&lua, "runs"), 3);
        assert!(is_true(&lua, "return own"), "an every cancels itself: true");
        assert!(c.timers.is_empty());
    }

    #[test]
    fn an_after_cancelling_itself_is_told_it_already_fired() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            "ran, own = false, nil
             me = host.timer.after(0, function() ran = true; own = host.timer.cancel(me) end)",
        )
        .exec()
        .unwrap();
        tick(&c, 0);
        // Both halves in Luau: that it ran, and that `cancel` answered a real `false`.
        assert!(is_true(&lua, "return ran == true and own == false"));
    }

    /// The case the collect-tokens-then-look-again loop is for: two timers due in the same
    /// tick, and the first to run cancels the second.
    #[test]
    fn a_callback_cancels_another_timer_due_in_the_same_tick() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            r#"
            onceB, everyB, cancelled = 0, 0, {}
            local b, d
            host.timer.after(0, function() cancelled.once = host.timer.cancel(b) end)
            b = host.timer.after(0, function() onceB = onceB + 1 end)
            host.timer.every(5, function()
              if cancelled.every == nil then cancelled.every = host.timer.cancel(d) end
            end)
            d = host.timer.every(5, function() everyB = everyB + 1 end)
            "#,
        )
        .exec()
        .unwrap();
        // Everything is due at +5 ms; the second tick would run `d` had it survived.
        tick(&c, 5);
        tick(&c, 10);
        assert_eq!(get_i(&lua, "onceB"), 0, "the one-shot cancelled in its own tick never ran");
        assert_eq!(get_i(&lua, "everyB"), 0, "the recurring one cancelled in its own tick never ran");
        assert!(is_true(&lua, "return cancelled.once == true and cancelled.every == true"));
    }

    /// A one-shot callback may cancel a recurring timer due in the same tick: one-shots run
    /// first, so the recurring one is gone before its turn.
    #[test]
    fn a_one_shot_cancels_a_recurring_timer_due_in_the_same_tick() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            r#"
            polls, cancelled = 0, nil
            local poll = host.timer.every(5, function() polls = polls + 1 end)
            host.timer.after(5, function() cancelled = host.timer.cancel(poll) end)
            "#,
        )
        .exec()
        .unwrap();
        tick(&c, 5);
        assert!(is_true(&lua, "return cancelled"), "the one-shot ran, and found the poll pending");
        assert_eq!(get_i(&lua, "polls"), 0);
    }

    #[test]
    fn another_modules_token_cannot_be_cancelled() {
        let c = clocked();
        let owner = vm(&c, 0);
        let other = vm(&c, 1);
        let token: i64 =
            owner.load("fired = 0; return host.timer.every(5, function() fired = fired + 1 end)").eval().unwrap();
        other.globals().set("token", token).unwrap();
        assert!(is_true(&other, "return host.timer.cancel(token) == false"));
        tick(&c, 5);
        assert_eq!(get_i(&owner, "fired"), 1, "the owner's poll kept running");
        assert!(owner.load(format!("return host.timer.cancel({token})")).eval::<bool>().unwrap());
    }

    #[test]
    fn cancel_never_raises_and_answers_false_for_what_is_not_a_pending_token() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load("ran = false; done = host.timer.after(0, function() ran = true end)").exec().unwrap();
        tick(&c, 0);
        assert!(is_true(&lua, "return ran"), "`done` has fired, so it is a spent token");
        assert!(is_true(
            &lua,
            r#"
            return host.timer.cancel(nil) == false
              and host.timer.cancel() == false
              and host.timer.cancel(done) == false      -- already fired
              and host.timer.cancel(123456) == false    -- never handed out
              and host.timer.cancel("1") == false
              and host.timer.cancel(1.5) == false
              and host.timer.cancel(0/0) == false
              and host.timer.cancel(math.huge) == false
              and host.timer.cancel({}) == false
            "#,
        ));
    }

    /// What the page says about `ms`, for both calls: a negative one raises and arms nothing,
    /// a fraction is cut to the whole number below, and an interval below 1 counts as 1.
    #[test]
    fn a_negative_delay_raises_and_a_fraction_is_cut() {
        let c = clocked();
        let lua = vm(&c, 0);
        for call in ["host.timer.after(-1, function() end)", "host.timer.every(-1, function() end)"] {
            assert!(lua.load(call).exec().is_err(), "{call} raises");
        }
        assert!(c.timers.is_empty(), "nothing was armed by a call that raised");
        lua.load(
            r#"
            once, polls = 0, 0
            host.timer.after(1.9, function() once = once + 1 end)
            host.timer.every(0, function() polls = polls + 1 end)
            "#,
        )
        .exec()
        .unwrap();
        tick(&c, 0);
        assert_eq!((get_i(&lua, "once"), get_i(&lua, "polls")), (0, 0), "neither is due at once");
        tick(&c, 1);
        assert_eq!((get_i(&lua, "once"), get_i(&lua, "polls")), (1, 1), "1.9 ms is 1 ms, and 0 ms is 1 ms");
    }

    /// A disabled module's timers keep their schedule and are not called; cancel still works.
    #[test]
    fn a_disabled_module_is_not_called_and_can_still_cancel() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            r#"
            once, polls = 0, 0
            host.timer.after(5, function() once = once + 1 end)
            poll = host.timer.every(5, function() polls = polls + 1 end)
            "#,
        )
        .exec()
        .unwrap();
        fire(&c, 5, |_| false, || {}, |_, e| panic!("{e}"));
        assert_eq!((get_i(&lua, "once"), get_i(&lua, "polls")), (0, 0));
        // The one-shot is gone for good; the poll was re-armed for +10 ms and runs, enabled.
        tick(&c, 10);
        assert_eq!((get_i(&lua, "once"), get_i(&lua, "polls")), (0, 1));
        assert!(lua.load("return host.timer.cancel(poll)").eval::<bool>().unwrap());
        assert!(c.timers.is_empty());
    }

    #[test]
    fn purge_and_rollback_drop_a_modules_timers() {
        let c = clocked();
        let a = vm(&c, 0);
        let b = vm(&c, 1);
        a.load("n = 0; host.timer.every(5, function() n = n + 1 end)").exec().unwrap();
        let tb: i64 = b.load("n = 0; return host.timer.every(5, function() n = n + 1 end)").eval().unwrap();
        c.timers.retain(|i| i != 1);
        tick(&c, 5);
        assert_eq!((get_i(&a, "n"), get_i(&b, "n")), (1, 0));
        b.globals().set("tb", tb).unwrap();
        assert!(is_true(&b, "return host.timer.cancel(tb) == false"), "purged: nothing to cancel");
        c.timers.retain(|_| false);
        assert!(c.timers.is_empty());
    }

    /// The callers that exist today throw the return value away; they keep working, and the
    /// epoch hook still runs once per tick with a one-shot due, and not for a recurring tick.
    #[test]
    fn existing_callers_and_the_epoch_hook_are_unchanged() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            r#"
            n = 0
            host.timer.after(0, function() n = n + 1 end)
            host.timer.after(0, function() n = n + 1 end)
            host.timer.every(1000, function() n = n + 10 end)
            "#,
        )
        .exec()
        .unwrap();
        let bumps = Cell::new(0);
        fire(&c, 0, |_| true, || bumps.set(bumps.get() + 1), |_, e| panic!("{e}"));
        assert_eq!((get_i(&lua, "n"), bumps.get()), (2, 1));
        fire(&c, 999, |_| true, || bumps.set(bumps.get() + 1), |_, e| panic!("{e}"));
        assert_eq!((get_i(&lua, "n"), bumps.get()), (2, 1), "an idle tick changes nothing");
        fire(&c, 1000, |_| true, || bumps.set(bumps.get() + 1), |_, e| panic!("{e}"));
        assert_eq!((get_i(&lua, "n"), bumps.get()), (12, 1), "a recurring tick does not turn the epoch");
    }

    /// A callback that raises is reported under its module and does not stop the others.
    #[test]
    fn a_failing_callback_is_reported_and_the_rest_still_run() {
        let c = clocked();
        let lua = vm(&c, 3);
        lua.load(
            r#"
            ran = 0
            host.timer.after(0, function() error("boom") end)
            host.timer.after(0, function() ran = ran + 1 end)
            "#,
        )
        .exec()
        .unwrap();
        let mut seen = Vec::new();
        fire(&c, 0, |_| true, || {}, |idx, e| seen.push((idx, e.to_string())));
        assert_eq!(get_i(&lua, "ran"), 1);
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0 == 3 && seen[0].1.contains("boom"), "{seen:?}");
    }

    /// A timer armed during a tick waits for a later tick, even with a delay of 0.
    #[test]
    fn a_timer_armed_during_a_tick_waits_for_the_next() {
        let c = clocked();
        let lua = vm(&c, 0);
        lua.load(
            r#"
            outer, inner = 0, 0
            host.timer.after(0, function()
              outer = outer + 1
              host.timer.after(0, function() inner = inner + 1 end)
            end)
            "#,
        )
        .exec()
        .unwrap();
        tick(&c, 20);
        assert_eq!((get_i(&lua, "outer"), get_i(&lua, "inner")), (1, 0));
        // Armed at +20 ms — the clock of the tick that ran the outer callback — with a delay of
        // 0, so due at once, and still left for the next tick, which may be at the same instant.
        tick(&c, 20);
        assert_eq!((get_i(&lua, "outer"), get_i(&lua, "inner")), (1, 1));
    }
}
