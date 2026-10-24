//! Quick repeats of a key — once, twice, three times — counted in one place for every press the
//! host is handed: a hotkey's, however it came (Windows' `RegisterHotKey` or its keyboard hook, a
//! Mac's Carbon handler), and a captured key's (the hook or the event tap). The design the
//! maintainer decided on 2026-10-20 (TODO.md, "Quick repeats of a key").
//!
//! **A count on every press.** A press carries `press.count` to its callback: 1, and one more for
//! each press of the same key that follows the one before it within the interval, with nothing else
//! between. It is counted as it arrives on the event loop, after the backend has paired a late
//! hook's report with the system's delivery of the same press (so a press is counted once), and
//! before its module's mailbox: a press that waits behind a busy handler keeps the count it had,
//! because what decides is when it was pressed, not when it runs.
//!
//! **When the count starts again** — the press is a 1: after the interval, the system's double-click
//! time or the binding's `within`, measured from the press before it; at a press of another key,
//! or of the same key with other modifiers, which is another combination; and when a key-down
//! reached the program in between — typed, let through, or sent by another program
//! (`Pressed::taps_seq`). Not one this application sent with `host.input.send`, which the hook and
//! the tap know by its mark (decided by the maintainer on 2026-10-23): a callback that sends a key
//! at every press does not end its own count. Text typed with `host.input.text` carries no mark,
//! and ends it as a typed key does. A held key's auto-repeat counts nothing and starts nothing: it
//! carries the count of the press it repeats.
//!
//! **`taps = "final"`**: the binding hears its sequence once, when it is over, with its count — one
//! interval after its last press, or at once when another press starts a count of its own. Kept
//! here until then, as the press it ends with ([`Waiting`]), and handed to its module's mailbox by
//! the tick ([`tick`]) or by that next press: a deadline the tick looks at, as a timer's is,
//! never a wait on the event loop. The tick measures it against the moment the pump took the
//! presses queued so far, not against its own: a press made before the deadline that still waited
//! in the backend's queue then goes on with the sequence. A sequence once heard has ended its count,
//! so a press after it — one the clock's few milliseconds placed just inside the interval — begins
//! another, and a binding never hears 1 and then 2 for one sequence.
//!
//! Generic over [`KeyHost`] and [`MailHost`], as captures.rs is, so the scripted host of
//! key_scope_tests.rs counts with the host's own rules.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use mlua::{Lua, Table, Value};

use crate::captures::KeyHost;
use crate::json;
use crate::mailbox::{self, Event, MailHost};
use crate::ocr::types::{enter_priority, Priority};

/// The shortest `within` a binding may name, in milliseconds: shorter than a hand can press a key
/// twice.
pub(crate) const WITHIN_MIN_MS: u64 = 100;
/// The longest, in milliseconds: the longest double-click time Windows accepts.
pub(crate) const WITHIN_MAX_MS: u64 = 5000;

/// When a binding's callback runs: at every press with its count (the default), or once its
/// sequence is over, with the count it ended on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Every,
    Final,
}

/// A binding's options for its quick repeats: `taps` and `within`, as `host.hotkey.register` and
/// `host.keys.capture` read them ([`read_opts`]). `within` `None` is the system's double-click
/// time, asked at every press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Taps {
    pub mode: Mode,
    pub within: Option<Duration>,
}

impl Taps {
    /// The interval this binding counts by: its `within`, or `system` — the double-click time,
    /// asked only when the binding names none.
    pub(crate) fn interval(&self, system: impl FnOnce() -> Duration) -> Duration {
        self.within.unwrap_or_else(system)
    }
}

/// Which key a press is, as the count tells keys apart: a combination, the key and its modifier
/// roles; or a hotkey whose combination only the system read, by its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    Combo(u32, u8),
    Id(i32),
}

/// What a press carries to its callback: its count, and whether it is the press a binding with
/// `taps = "final"` hears once its sequence is over (`ended`) — such a press stands for all of the
/// sequence's presses, so one whose registration went before it ran is not passed on to the program
/// as a single key would be (captures.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Press {
    pub count: u32,
    pub ended: bool,
}

impl Press {
    /// A press counted `count`, heard at once.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn at_once(count: u32) -> Press {
        Press { count, ended: false }
    }
}

/// The table a callback is handed: `{ count = n }`.
pub(crate) fn table(lua: &Lua, press: Press) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("count", press.count)?;
    Ok(t)
}

/// The last press counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Last {
    pub key: Key,
    pub count: u32,
    /// When it was pressed.
    pub at: Instant,
    /// How many key-downs that end a count had reached the program by then (`Pressed::taps_seq`).
    pub seq: u32,
}

/// The count of a press of `key` made at `at`, with `seq` key-downs that end a count let through to
/// the program by then (`Pressed::taps_seq`), `own` of them its own key-down, against the last
/// press counted: one more than that press's while it was the same key, the count of key-downs
/// moved by no more than `own`, and it came less than `within` after it; 1 otherwise. A press
/// stamped before the last one — the two arrived in the other order — is no time after it.
pub(crate) fn next_count(last: Option<Last>, key: Key, at: Instant, seq: u32, own: u32, within: Duration) -> u32 {
    match last {
        Some(l) if l.key == key && seq.wrapping_sub(l.seq) <= own && at.saturating_duration_since(l.at) < within => {
            l.count.saturating_add(1)
        }
        _ => 1,
    }
}

/// A binding's sequence, kept until it is over: the press it ended on, as the event its module's
/// mailbox is handed, with that press's count, `ended` set.
pub(crate) struct Waiting {
    pub key: Key,
    /// One interval after its last press.
    pub due: Instant,
    pub owner: usize,
    pub lua: Lua,
    pub ev: Event,
}

/// The count, and the one sequence a `"final"` binding waits for. Main thread only, like the rest
/// of `Shared`. One for the whole application: a press of another key starts a count of its own,
/// whichever module it is for, so there is only ever one sequence going on.
#[derive(Default)]
pub(crate) struct Counter {
    last: Cell<Option<Last>>,
    waiting: RefCell<Option<Waiting>>,
}

impl Counter {
    /// Counts a press ([`next_count`]) and makes it the last; its count.
    pub(crate) fn press(&self, key: Key, at: Instant, seq: u32, own: u32, within: Duration) -> u32 {
        let count = next_count(self.last.get(), key, at, seq, own, within);
        self.last.set(Some(Last { key, count, at, seq }));
        count
    }

    /// The count a held key's auto-repeat of `key` carries: the count of the press it repeats, when
    /// that was the last one counted, else 1. Counts nothing.
    pub(crate) fn repeat_count(&self, key: Key) -> u32 {
        self.last.get().filter(|l| l.key == key).map_or(1, |l| l.count)
    }

    /// The last press counted.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn last(&self) -> Option<Last> {
        self.last.get()
    }

    /// Whether a sequence waits to be heard, and when it is due.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn waiting_due(&self) -> Option<Instant> {
        self.waiting.borrow().as_ref().map(|w| w.due)
    }

    /// The waiting sequence, taken out, unless a press of `key` counted `count` goes on with it.
    fn take_unless_continued(&self, key: Key, count: u32) -> Option<Waiting> {
        let mut w = self.waiting.borrow_mut();
        match w.as_ref() {
            Some(old) if count > 1 && old.key == key => None,
            _ => w.take(),
        }
    }

    /// The waiting sequence, taken out, when it is due by `taken` ([`tick`]). Its count ends with
    /// it: the last press counted is its own (a press of another key would have taken it out), and
    /// a press of its key that comes later is a 1.
    fn take_due(&self, taken: Instant) -> Option<Waiting> {
        let mut w = self.waiting.borrow_mut();
        if w.as_ref().is_some_and(|w| w.due <= taken) {
            self.last.set(None);
            w.take()
        } else {
            None
        }
    }
}

/// Counts a press of `key` the hook, the tap or the system handed over — `at`, `seq` and `own` as
/// [`next_count`] reads them, `within` its binding's interval — and answers its count. A sequence
/// that waited for its end and that this press does not go on with is over: it is handed to its
/// module first ([`deliver_waiting`]), being the earlier. A held key's auto-repeat (`repeat`) only
/// carries the count of the press it repeats.
pub(crate) fn count<H: KeyHost + MailHost>(h: &H, key: Key, at: Instant, seq: u32, own: u32, repeat: bool, within: Duration) -> u32 {
    if repeat {
        return h.taps().repeat_count(key);
    }
    let n = h.taps().press(key, at, seq, own, within);
    if let Some(w) = h.taps().take_unless_continued(key, n) {
        deliver_waiting(h, w);
    }
    n
}

/// What became of a counted press, by its binding's `taps`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handed {
    /// Handed to its module's mailbox: run at once, or queued while the module is busy.
    Delivered(mailbox::Delivered),
    /// Kept as its sequence's last press, for a `"final"` binding, until the sequence is over.
    Kept,
    /// A held key's auto-repeat of a `"final"` binding's key: no press, and nothing to run.
    Skipped,
}

/// Hands `ev`, a press counted by [`count`], to module `owner`'s mailbox in the VM `lua`, by its
/// binding's `mode`: at once — after the sequence of the same key a `"final"` binding still waited
/// for, which it ends — or, for a `"final"` binding, kept until `due` in place of the press before
/// it (`ev` carries `ended` then). A held key's repeat of a `"final"` binding is no press of it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hand_on<H: KeyHost + MailHost>(
    h: &H,
    key: Key,
    owner: usize,
    lua: &Lua,
    ev: Event,
    mode: Mode,
    repeat: bool,
    due: Instant,
) -> Handed {
    match mode {
        Mode::Every => {
            // Taken before it runs: the borrow must not outlive the statement while its callback runs.
            let waiting = h.taps().waiting.borrow_mut().take();
            if let Some(w) = waiting {
                deliver_waiting(h, w);
            }
            Handed::Delivered(mailbox::deliver(h, owner, lua, ev))
        }
        Mode::Final if repeat => Handed::Skipped,
        Mode::Final => {
            *h.taps().waiting.borrow_mut() = Some(Waiting { key, due, owner, lua: lua.clone(), ev });
            Handed::Kept
        }
    }
}

/// The tick's share, in both loops (`fire_due_timers`): the waiting sequence handed to its module
/// once it is due by the moment the pump took the presses queued so far
/// (`Backend::input_taken_at`; the tick's own moment for a backend that queues none). Every press
/// made before that moment has been counted, so a sequence due by then is over. Not the tick's own
/// moment: a press made before the deadline can still wait in the backend's queue while the pump
/// runs other modules' callbacks, and would go on with a sequence already heard.
pub(crate) fn tick<H: KeyHost + MailHost>(h: &H) {
    let taken = h.key_backend().input_taken_at().unwrap_or_else(Instant::now);
    if let Some(w) = h.taps().take_due(taken) {
        deliver_waiting(h, w);
    }
}

/// A sequence that is over, handed to its module's mailbox as the press it ended on: in the
/// interactive lane, as its press was — somebody is waiting for it — after the epoch has turned, as
/// a press's dispatch turns it. A hotkey's notes the window in front at its press only while its
/// module is busy, as a hotkey press does that waits in the mailbox (`Event::Hotkey`).
fn deliver_waiting<H: KeyHost + MailHost>(h: &H, w: Waiting) {
    let _prio = enter_priority(Priority::Interactive);
    h.bump_epoch();
    let mut ev = w.ev;
    if let Event::Hotkey { front, .. } = &mut ev {
        if !mailbox::busy(h, w.owner) {
            *front = None;
        }
    }
    mailbox::deliver(h, w.owner, &w.lua, ev);
}

/// Raises for a key of `opts` that is not one of `allowed`: a misspelt option must not leave a
/// binding as it was without a word. `{call}: 'tap' is not an option; the options are taps and
/// within`.
pub(crate) fn only_options(opts: Option<&Table>, call: &str, allowed: &[&str]) -> mlua::Result<()> {
    let Some(t) = opts else { return Ok(()) };
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, _) = pair?;
        if matches!(&k, Value::String(s) if s.to_str().is_ok_and(|s| allowed.contains(&&*s))) {
            continue;
        }
        let name = match &k {
            Value::String(s) => format!("'{}'", s.to_string_lossy()),
            other => format!("a {} key", json::luau_type(other)),
        };
        let words = match allowed {
            [one] => format!("the only one is {one}"),
            [rest @ .., last] => format!("the options are {} and {last}", rest.join(", ")),
            [] => "there are none".to_string(),
        };
        return Err(mlua::Error::external(format!("{call}: {name} is not an option; {words}")));
    }
    Ok(())
}

/// `taps` and `within` of a binding's options, read strictly; the other keys are the caller's to
/// check ([`only_options`]). `taps`: `nil` or `"every"` is a count on every press, `"final"` the
/// sequence heard once it is over; anything else raises. `within`: `nil` is the system's
/// double-click time; a number of milliseconds from [`WITHIN_MIN_MS`] to [`WITHIN_MAX_MS`], a
/// fraction cut to the whole number below, is the binding's own; anything else raises.
pub(crate) fn read_opts(opts: Option<&Table>, call: &str) -> mlua::Result<Taps> {
    let Some(t) = opts else { return Ok(Taps::default()) };
    let mode = match t.get::<Value>("taps")? {
        Value::Nil => Mode::Every,
        Value::String(s) => match &*s.to_string_lossy() {
            "every" => Mode::Every,
            "final" => Mode::Final,
            other => {
                return Err(mlua::Error::external(format!("{call}: taps is \"every\" or \"final\", not \"{other}\"")))
            }
        },
        other => {
            return Err(mlua::Error::external(format!(
                "{call}: taps is \"every\" or \"final\", not a {}",
                json::luau_type(&other)
            )))
        }
    };
    let within = match t.get::<Value>("within")? {
        Value::Nil => None,
        v @ (Value::Integer(_) | Value::Number(_)) => {
            let ms = match v {
                Value::Integer(i) => i as f64,
                Value::Number(n) => n,
                _ => unreachable!(),
            };
            let whole = ms.trunc();
            if !(whole >= WITHIN_MIN_MS as f64 && whole <= WITHIN_MAX_MS as f64) {
                return Err(mlua::Error::external(format!(
                    "{call}: within is a number of milliseconds from {WITHIN_MIN_MS} to {WITHIN_MAX_MS}, not {}",
                    shown(ms)
                )));
            }
            Some(Duration::from_millis(whole as u64))
        }
        other => {
            return Err(mlua::Error::external(format!(
                "{call}: within is a number of milliseconds from {WITHIN_MIN_MS} to {WITHIN_MAX_MS}, not a {}",
                json::luau_type(&other)
            )))
        }
    };
    Ok(Taps { mode, within })
}

/// A number as a message says it: a whole one without its `.0`.
fn shown(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALT_V: Key = Key::Combo(0x56, crate::backend::MASK_ALT);
    const ALT_B: Key = Key::Combo(0x42, crate::backend::MASK_ALT);
    const MS: fn(u64) -> Duration = Duration::from_millis;

    /// The count's rules, each on its own: the same key within the interval counts on, from the
    /// press before it; past the interval, another key, other modifiers, or a key-down that reached
    /// the program in between starts again; a press's own key-down in the count (a Mac's hotkey) is
    /// no other key; a press stamped before the last is no time after it.
    #[test]
    fn a_press_counts_on_only_for_the_same_key_within_the_interval_with_nothing_between() {
        let t0 = Instant::now();
        let last = |key, count, ms, seq| Some(Last { key, count, at: t0 + MS(ms), seq });
        let w = MS(500);
        assert_eq!(next_count(None, ALT_V, t0, 0, 0, w), 1, "the first press");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_V, t0 + MS(499), 7, 0, w), 2);
        assert_eq!(next_count(last(ALT_V, 2, 400, 7), ALT_V, t0 + MS(800), 7, 0, w), 3, "from the press before it, not the first");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_V, t0 + MS(500), 7, 0, w), 1, "the interval itself is past it");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_B, t0 + MS(100), 7, 0, w), 1, "another key");
        let shift_alt_v = Key::Combo(0x56, crate::backend::MASK_ALT | crate::backend::MASK_SHIFT);
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), shift_alt_v, t0 + MS(100), 7, 0, w), 1, "other modifiers");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_V, t0 + MS(100), 8, 0, w), 1, "a key-down reached the program");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_V, t0 + MS(100), 8, 1, w), 2, "its own key-down");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_V, t0 + MS(100), 9, 1, w), 1, "its own and another");
        assert_eq!(next_count(last(ALT_V, 1, 0, u32::MAX), ALT_V, t0 + MS(100), 0, 1, w), 2, "the count wraps");
        assert_eq!(next_count(last(ALT_V, 1, 300, 7), ALT_V, t0 + MS(100), 7, 0, w), 2, "stamped before the last");
        assert_eq!(next_count(last(ALT_V, 1, 0, 7), ALT_V, t0 + MS(900), 7, 0, MS(1000)), 2, "the binding's own interval");
        assert_eq!(next_count(last(Key::Id(4), 1, 0, 7), Key::Id(4), t0 + MS(100), 7, 0, w), 2, "a hotkey by its id");
        assert_eq!(next_count(last(Key::Id(4), 1, 0, 7), Key::Id(5), t0 + MS(100), 7, 0, w), 1);
    }

    /// The counter keeps the last press; a repeat carries its count and changes nothing.
    #[test]
    fn the_counter_keeps_the_last_press_and_a_repeat_changes_nothing() {
        let c = Counter::default();
        let t0 = Instant::now();
        assert_eq!(c.press(ALT_V, t0, 0, 0, MS(500)), 1);
        assert_eq!(c.press(ALT_V, t0 + MS(200), 0, 0, MS(500)), 2);
        assert_eq!(c.repeat_count(ALT_V), 2, "a repeat of the second press");
        assert_eq!(c.repeat_count(ALT_B), 1, "a repeat of a key not counted last");
        assert_eq!(c.last().map(|l| (l.key, l.count)), Some((ALT_V, 2)), "the repeats counted nothing");
        assert_eq!(c.press(ALT_V, t0 + MS(400), 0, 0, MS(500)), 3);
        assert_eq!(c.press(ALT_B, t0 + MS(500), 0, 0, MS(500)), 1);
        assert_eq!(c.press(ALT_V, t0 + MS(600), 0, 0, MS(500)), 1, "Alt+B went down between");
    }

    /// The options as both bindings read them: `taps` "every", "final" or nothing; `within` a
    /// number of milliseconds within its limits, a fraction cut; everything else raises, naming the
    /// call; and a key that is no option raises too.
    #[test]
    fn taps_and_within_are_read_strictly() {
        let lua = Lua::new();
        let read = |src: &str| -> mlua::Result<Taps> {
            let t: Option<Table> = lua.load(src).eval().unwrap();
            only_options(t.as_ref(), "host.keys.capture", &["taps", "within"])?;
            read_opts(t.as_ref(), "host.keys.capture")
        };
        assert_eq!(read("return nil").unwrap(), Taps::default());
        assert_eq!(read("return {}").unwrap(), Taps { mode: Mode::Every, within: None });
        assert_eq!(read("return { taps = 'every' }").unwrap().mode, Mode::Every);
        assert_eq!(read("return { taps = 'final' }").unwrap().mode, Mode::Final);
        assert_eq!(read("return { within = 300 }").unwrap().within, Some(MS(300)));
        assert_eq!(read("return { within = 100 }").unwrap().within, Some(MS(100)));
        assert_eq!(read("return { within = 5000 }").unwrap().within, Some(MS(5000)));
        assert_eq!(read("return { within = 250.9 }").unwrap().within, Some(MS(250)), "a fraction is cut");
        assert_eq!(read("return { within = 5000.5 }").unwrap().within, Some(MS(5000)));
        let raised = |src: &str| read(src).unwrap_err().to_string();
        assert!(raised("return { taps = 'twice' }").contains("host.keys.capture: taps is \"every\" or \"final\", not \"twice\""));
        assert!(raised("return { taps = 2 }").contains("taps is \"every\" or \"final\", not a number"));
        assert!(raised("return { taps = true }").contains("not a boolean"));
        assert!(raised("return { within = 99 }").contains("host.keys.capture: within is a number of milliseconds from 100 to 5000, not 99"));
        assert!(raised("return { within = 99.9 }").contains("not 99.9"));
        assert!(raised("return { within = 5001 }").contains("not 5001"));
        assert!(raised("return { within = -300 }").contains("not -300"));
        assert!(raised("return { within = 0/0 }").contains("from 100 to 5000, not"));
        assert!(raised("return { within = math.huge }").contains("not inf"));
        assert!(raised("return { within = '300' }").contains("from 100 to 5000, not a string"));
        assert!(raised("return { tap = 'final' }").contains("host.keys.capture: 'tap' is not an option; the options are taps and within"));
        assert!(raised("return { 'final' }").contains("a number key is not an option"));
        let t: Table = lua.load("return { forground = true }").eval().unwrap();
        let e = only_options(Some(&t), "host.hotkey.register", &["foreground", "taps", "within"]).unwrap_err().to_string();
        assert!(e.contains("'forground' is not an option; the options are foreground, taps and within"), "{e}");
    }
}
