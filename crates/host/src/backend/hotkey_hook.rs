//! Registered hotkeys, matched in the low-level keyboard hook as well — the pure half.
//!
//! **Why.** `RegisterHotKey` is the right way to hold a combination on Windows: the OS says
//! whether somebody else already has it, and it keeps working in front of an elevated window,
//! where a low-level hook sees nothing. But a program can switch it off for everybody while it
//! is in front. WinUAE registers its keyboard as raw input with `RIDEV_NOHOTKEYS`, and while it
//! is the foreground window no `WM_HOTKEY` of any program is generated at all — any
//! combination, not only the Ctrl ones. The low-level hook still sees the keys. So a hotkey the
//! OS granted is matched in the host's own hook too, the way AutoHotkey's `#UseHook` does it,
//! and `RegisterHotKey` stays as the claim on the combination and as the path whenever the hook
//! is not there to answer.
//!
//! **Only what Windows granted.** A combination whose registration was refused belongs to
//! another program. Taking it through the hook would silently steal it — the other program's
//! key would simply stop working — so a refused one never enters [`Table`], and it keeps being
//! reported as unavailable exactly as before. [`file_if_granted`] is the one way in, and it
//! takes `RegisterHotKey`'s answer; the backend removes an entry with the registration.
//!
//! **One press, one callback.** The hook swallows a press it matched, and Windows then never
//! turns it into a `WM_HOTKEY`: low-level hooks run before hotkey processing, which is why a
//! screen reader's hook can take a registered combination away from us today. The one case
//! with two deliveries is a hook that answered too late — its thread did not get to run within
//! `LowLevelHooksTimeout`, the system passed the key on without waiting, `RegisterHotKey`
//! fired, and the hook's own call still ran afterwards. The hook has a thread of its own that
//! does nothing else (see the backend), so that takes a machine too loaded to schedule it; when
//! it happens, [`Dedupe`] recognises the pair by its timestamps. Auto-repeat is the other way
//! one press could fire twice, and [`Table::on_down`] swallows repeats without dispatching
//! them, which is what `MOD_NOREPEAT` does for the registered path.
//!
//! **Judged as of the key event.** A late call is handed an event from the past, and the
//! keyboard's asynchronous state is the present: plain `v` typed during a stall, with Alt held
//! by the time the hook ran, would read as Alt+V. So a late call is judged by the modifiers
//! the hook itself saw go by ([`Mods`]), and only a call on time asks the system.
//!
//! Everything here is integers and fixed-size arrays: no OS call, no allocation after
//! [`Table::new`], and a lookup is one index into a table — it runs inside the hook, for every
//! keystroke on the machine.

use super::{MASK_ALT, MASK_CTRL, MASK_SHIFT, MASK_WIN};

/// No hotkey. Host ids start at 1 (`alloc_id` counts up from 0), so 0 never names one.
pub(crate) const NO_ID: i32 = 0;

/// Virtual keys are one byte, and the hook reports the four Windows modifiers as four bits.
const VKS: usize = 256;
const MASKS: usize = 16;

/// A key-down this soon after the last down of the same key, with no key-up in between, is the
/// keyboard's auto-repeat, whatever the modifiers did meanwhile — at the Keyboard control
/// panel's settings, whose longest delay before the first repeat is one second. Longer when the
/// settings in force leave longer gaps; see [`repeat_threshold`].
///
/// Not "no key-up in between" alone, because an up can go missing — the secure desktop of
/// Ctrl+Alt+Del or a UAC prompt takes the keyboard, and a key released there is never seen by
/// the hook — and a press remembered as held forever would swallow that key once, without a
/// callback, the next time it is pressed. Auto-repeat's first repeat comes after the keyboard
/// delay, every later one sooner, so a down after a longer gap is a new press.
pub(crate) const REPEAT_MS: u32 = 1_200;

/// What [`repeat_threshold`] adds to the longest gap auto-repeat can leave between two downs.
const REPEAT_MARGIN_MS: u32 = 200;

/// Added to how late the hook ran when pairing its press with a `WM_HOTKEY`, for the tick
/// clock's resolution (about 16 ms) with room to spare. Also the most a call may be late and
/// still count as on time, for which modifiers it is judged by (see [`event_mask`]).
pub(crate) const SLACK_MS: u32 = 50;

/// How late a hook call is taken to be at most: a sanity limit against a timestamp that is
/// wrong, not a model of the system. The system waits `LowLevelHooksTimeout` for each event
/// (at most a second since Windows 10 1709), but it waits for every event queued ahead of this
/// one first — the modifiers pressed just before the key among them — so a call can be late by
/// several timeouts, and a cap near one timeout would open a pairing window too short for the
/// press it has to catch.
pub(crate) const LATE_CAP_MS: u32 = 30_000;

/// How late the hook got to run for a key event, in the tick clock's milliseconds: `now` is
/// `GetTickCount` when it ran, `time` the event's own stamp.
///
/// 0 when the stamp cannot be trusted. An injected event (`LLKHF_INJECTED`) carries whatever
/// time its sender put in `KEYBDINPUT::time`, and a stamp ahead of the clock is wrong by
/// definition; both are taken as on time — the pairing window then is [`SLACK_MS`], and the
/// modifiers are the system's, as they always were.
pub(crate) fn lateness(now: u32, time: u32, injected: bool) -> u32 {
    if injected {
        return 0;
    }
    let d = now.wrapping_sub(time) as i32;
    if d <= 0 {
        0
    } else {
        (d as u32).min(LATE_CAP_MS)
    }
}

/// The auto-repeat threshold for the keyboard settings in force.
///
/// `delay_setting` is `SPI_GETKEYBOARDDELAY`'s answer, 0 to 3 for 250 to 1000 ms before the
/// first repeat; `None` when it could not be read. `filter_keys` is FilterKeys' delay before
/// the first repeat and its interval between repeats (`iDelayMSec`, `iRepeatMSec`), while
/// FilterKeys is on. Either can be several seconds, and a gap longer than the threshold makes
/// every repeat of a held hotkey a new press that fires it again.
pub(crate) fn repeat_threshold(delay_setting: Option<u32>, filter_keys: Option<(u32, u32)>) -> u32 {
    let mut gap = delay_setting.map_or(1_000, |d| (d.min(3) + 1) * 250);
    if let Some((delay, repeat)) = filter_keys {
        gap = gap.max(delay).max(repeat);
    }
    gap.saturating_add(REPEAT_MARGIN_MS).max(REPEAT_MS)
}

/// Win32 `MOD_*` flags, as `RegisterHotKey` takes them, as the host's `MASK_*` bits, as the
/// hook computes them. The numbers differ — `MOD_ALT` is 1 where `MASK_ALT` is 4 — and a
/// table filed under the wrong ones matches a different combination from the one granted.
pub(crate) fn mods_to_mask(mods: u32) -> u8 {
    // MOD_ALT 0x1, MOD_CONTROL 0x2, MOD_SHIFT 0x4, MOD_WIN 0x8 (WinUser.h); MOD_NOREPEAT
    // (0x4000) and anything else is not a modifier and is ignored.
    let mut mask = 0u8;
    if mods & 0x1 != 0 {
        mask |= MASK_ALT;
    }
    if mods & 0x2 != 0 {
        mask |= MASK_CTRL;
    }
    if mods & 0x4 != 0 {
        mask |= MASK_SHIFT;
    }
    if mods & 0x8 != 0 {
        mask |= MASK_WIN;
    }
    mask
}

/// Whether swallowing a key while these modifiers are held needs a masking keystroke.
///
/// The hook hides the key from everything after it, including the parts of Windows that
/// watch for a modifier pressed and released ON ITS OWN. When the key between is gone, those
/// see a bare modifier: Alt released activates the window's menu bar, Win released opens
/// Start, and Alt+Shift — or Ctrl+Shift, where the user chose it — switches the input language.
/// The registered path never had this problem, because the key still went through Windows
/// before it became a hotkey. AutoHotkey solves it the same way for its hook hotkeys: an
/// unassigned key pressed and released while the modifier is still down (see the backend).
/// Ctrl alone and Shift alone do nothing when tapped, so they need nothing.
pub(crate) fn needs_mask_key(mask: u8) -> bool {
    mask & (MASK_ALT | MASK_WIN) != 0 || mask & (MASK_CTRL | MASK_SHIFT) == (MASK_CTRL | MASK_SHIFT)
}

/// The four Windows modifiers as the hook has seen them go by, by side — left and right Shift
/// are two keys, and letting go of one while the other is held leaves Shift down.
///
/// Kept because a late hook call must be judged by the modifiers held when its key went down,
/// and the only record of that is the hook's own: the events before it, in order. A call on
/// time asks the system instead and brings this record in line ([`Mods::resync`]), so a
/// release the hook never saw — on the secure desktop, say — is forgotten at the next key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Mods {
    held: u8,
}

const L_SHIFT: u8 = 1 << 0;
const R_SHIFT: u8 = 1 << 1;
const L_CTRL: u8 = 1 << 2;
const R_CTRL: u8 = 1 << 3;
const L_ALT: u8 = 1 << 4;
const R_ALT: u8 = 1 << 5;
const L_WIN: u8 = 1 << 6;
const R_WIN: u8 = 1 << 7;

/// Each modifier as (its mask bit, its left key, both of its keys).
const SIDES: [(u8, u8, u8); 4] = [
    (MASK_SHIFT, L_SHIFT, L_SHIFT | R_SHIFT),
    (MASK_CTRL, L_CTRL, L_CTRL | R_CTRL),
    (MASK_ALT, L_ALT, L_ALT | R_ALT),
    (MASK_WIN, L_WIN, L_WIN | R_WIN),
];

impl Mods {
    /// The key this virtual key is, if it is a modifier. The generic codes (0x10 to 0x12)
    /// arrive from injected input and count as the left key.
    fn key(vk: u32) -> Option<u8> {
        match vk {
            0x10 | 0xA0 => Some(L_SHIFT),
            0xA1 => Some(R_SHIFT),
            0x11 | 0xA2 => Some(L_CTRL),
            0xA3 => Some(R_CTRL),
            0x12 | 0xA4 => Some(L_ALT),
            0xA5 => Some(R_ALT),
            0x5B => Some(L_WIN),
            0x5C => Some(R_WIN),
            _ => None,
        }
    }

    /// A key event went by; only a modifier changes anything.
    pub(crate) fn on_key(&mut self, vk: u32, is_down: bool) {
        if let Some(bit) = Self::key(vk) {
            if is_down {
                self.held |= bit;
            } else {
                self.held &= !bit;
            }
        }
    }

    /// As the host's `MASK_*` bits.
    pub(crate) fn mask(&self) -> u8 {
        SIDES
            .iter()
            .filter(|(_, _, both)| self.held & both != 0)
            .fold(0, |m, (bit, _, _)| m | bit)
    }

    /// Brought in line with the modifiers the system says are held, `mask`: one it says is up
    /// is up on both sides, and one it says is down and this record had up is taken as the left.
    pub(crate) fn resync(&mut self, mask: u8) {
        for (bit, left, both) in SIDES {
            if mask & bit == 0 {
                self.held &= !both;
            } else if self.held & both == 0 {
                self.held |= left;
            }
        }
    }
}

/// The modifiers a key event is judged by, before `mods` takes the event itself in.
///
/// On time (late by at most [`SLACK_MS`]): what `asked` says the system holds — the
/// asynchronous key state, which then reflects every event before this one and not yet this
/// one — and `mods` is brought in line with it. Late: `mods`, the record of what went by
/// before this event, because the system's answer would be the keyboard as it is now.
pub(crate) fn event_mask(mods: &mut Mods, late: u32, asked: impl FnOnce() -> u8) -> u8 {
    if late <= SLACK_MS {
        let mask = asked();
        mods.resync(mask);
        mask
    } else {
        mods.mask()
    }
}

/// What the hook does with a key-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Down {
    /// A registered combination: swallow it and dispatch this hotkey.
    Fire(i32),
    /// The keyboard repeating a combination already dispatched: swallow it, dispatch nothing.
    Repeat,
    /// A registered combination the hook lets through anyway: the key was already down before
    /// the combination was complete, so it is not a new press here. `RegisterHotKey` may still
    /// deliver it, and the pump is told to expect that.
    Pass(i32),
    /// Not ours; let it through.
    Miss,
}

/// One key's state, as the hook has seen it go by — every key, not only granted ones, because
/// "is this down a repeat" is a question about the key, whatever its modifiers were.
#[derive(Clone, Copy, Default)]
struct Held {
    /// A key-down was seen and its key-up has not been.
    down: bool,
    /// That press was dispatched as a hotkey, so its repeats are swallowed too.
    swallowing: bool,
    /// `KBDLLHOOKSTRUCT::time` of the last down, the press or its latest repeat.
    time: u32,
}

/// The hotkeys Windows granted, by (virtual key, modifier mask), and which keys are held.
pub(crate) struct Table {
    /// `slots[vk * 16 + mask]` is the id granted for that combination, or [`NO_ID`].
    slots: Box<[i32]>,
    held: Box<[Held]>,
    /// The auto-repeat threshold in force — [`REPEAT_MS`] until [`Table::set_repeat_ms`].
    repeat_ms: u32,
}

fn slot(vk: u32, mask: u8) -> Option<usize> {
    let vk = vk as usize;
    let mask = mask as usize;
    (vk < VKS && mask < MASKS).then_some(vk * MASKS + mask)
}

/// Files a combination for the hook if, and only if, `RegisterHotKey` granted it — `granted` is
/// its answer. The table is created with the first grant, which is its only allocation; a
/// refused combination neither creates it nor enters it. `false` when nothing was filed.
pub(crate) fn file_if_granted(
    table: &mut Option<Table>,
    granted: bool,
    id: i32,
    vk: u32,
    mods: u32,
) -> bool {
    if !granted {
        return false;
    }
    table.get_or_insert_with(Table::new).insert(id, vk, mods_to_mask(mods))
}

impl Table {
    /// The only allocation: done when the first hotkey is granted, outside the hook.
    pub(crate) fn new() -> Self {
        Table {
            slots: vec![NO_ID; VKS * MASKS].into_boxed_slice(),
            held: vec![Held::default(); VKS].into_boxed_slice(),
            repeat_ms: REPEAT_MS,
        }
    }

    /// Puts an auto-repeat threshold in force — see [`repeat_threshold`].
    pub(crate) fn set_repeat_ms(&mut self, ms: u32) {
        self.repeat_ms = ms;
    }

    /// Files a combination Windows has just granted to `id`. A combination outside the table
    /// (a virtual key above 255 or a mask beyond the four modifiers) is not filed and stays
    /// on the registered path alone; `false` says so. Called through [`file_if_granted`].
    fn insert(&mut self, id: i32, vk: u32, mask: u8) -> bool {
        if id == NO_ID {
            return false;
        }
        let Some(i) = slot(vk, mask) else {
            return false;
        };
        // Overwritten, not refused: Windows grants a combination once, so a different id
        // still filed here can only be a registration whose release was never seen, and the
        // one Windows just granted is the one that holds it now.
        self.slots[i] = id;
        true
    }

    /// Forgets every combination filed under `id`; `false` when there was none. A scan of the
    /// whole table, which is microseconds and happens on unregistration, never in the hook.
    pub(crate) fn remove(&mut self, id: i32) -> bool {
        if id == NO_ID {
            return false;
        }
        let mut found = false;
        for s in self.slots.iter_mut().filter(|s| **s == id) {
            *s = NO_ID;
            found = true;
        }
        found
    }

    /// Whether `id` is filed here, for telling a `WM_HOTKEY` the hook should have seen from
    /// one it could not have.
    pub(crate) fn holds(&self, id: i32) -> bool {
        id != NO_ID && self.slots.iter().any(|s| *s == id)
    }

    /// The id granted for this exact combination, or [`NO_ID`].
    pub(crate) fn lookup(&self, vk: u32, mask: u8) -> i32 {
        slot(vk, mask).map_or(NO_ID, |i| self.slots[i])
    }

    /// Whether a down of `h`'s key stamped `time` is its auto-repeat.
    fn repeats(&self, h: &Held, time: u32) -> bool {
        h.down && time.wrapping_sub(h.time) <= self.repeat_ms
    }

    /// A key-down of `vk` with `mask` held, stamped `time` (the event's own timestamp). Called
    /// for every key-down that is not a modifier and that the captured keys did not take.
    pub(crate) fn on_down(&mut self, vk: u32, mask: u8, time: u32) -> Down {
        let id = self.lookup(vk, mask);
        let Some(&h) = self.held.get(vk as usize) else {
            return Down::Miss;
        };
        if self.repeats(&h, time) {
            // The keyboard repeating a key that is still down. Never a new press, whatever
            // the modifiers did in the meantime — `MOD_NOREPEAT` on the registered path judges
            // a repeat by the key, not by the combination, and so does this.
            self.held[vk as usize].time = time;
            // Swallowed while it is a press we dispatched and a combination still granted —
            // also when that was granted again under a new id in the meantime, which an
            // overlay does on every focus move. A repeat of a press that was not ours goes to
            // the application, and `RegisterHotKey` answers for it as it would with no hook;
            // a repeat of a combination no longer granted goes to the application as well.
            return if id == NO_ID {
                Down::Miss
            } else if h.swallowing {
                Down::Repeat
            } else {
                Down::Pass(id)
            };
        }
        // A new press — or one remembered as held for too long to be repeating, whose release
        // went missing.
        self.held[vk as usize] = Held {
            down: true,
            swallowing: id != NO_ID,
            time,
        };
        if id == NO_ID {
            Down::Miss
        } else {
            Down::Fire(id)
        }
    }

    /// A key-down of `vk` stamped `time` that the captured keys took, or let through for a
    /// screen reader, before the hotkeys were asked. Recorded as held and not dispatched here,
    /// so the rules above know about it: its repeats are not a new press, and its key-up
    /// ends it like any other.
    pub(crate) fn note_down(&mut self, vk: u32, time: u32) {
        let Some(&h) = self.held.get(vk as usize) else {
            return;
        };
        if self.repeats(&h, time) {
            self.held[vk as usize].time = time;
        } else {
            self.held[vk as usize] = Held {
                down: true,
                swallowing: false,
                time,
            };
        }
    }

    /// A key-up of `vk`. Always let through by the caller — the registered path lets the
    /// key-up reach the application too — so this only ends the press. Called for every
    /// key-up that is not a modifier, whatever else takes it.
    pub(crate) fn on_up(&mut self, vk: u32) {
        if let Some(h) = self.held.get_mut(vk as usize) {
            h.down = false;
            h.swallowing = false;
        }
    }
}

/// How a hotkey press reached the pump.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Route {
    /// The hook swallowed it. `time` is the key event's timestamp, `late` how long after it
    /// the hook got to run (both in the tick clock's milliseconds).
    Hook { time: u32, late: u32 },
    /// The hook saw it and deliberately let it through (a screen reader's modifier was held,
    /// or the key was already down before the combination was complete), so `RegisterHotKey`
    /// is expected to deliver it.
    Passed { time: u32, late: u32 },
    /// `WM_HOTKEY`, stamped `time` by `GetMessageTime`.
    Os { time: u32 },
}

/// What the pump does with a `WM_HOTKEY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OsPress {
    /// Deliver it. `expected` when the hook had let this press through on purpose, so there is
    /// nothing to explain about it having come this way.
    Deliver { expected: bool },
    /// The hook already dispatched this press; drop it. `late` is how late the hook was.
    Duplicate { late: u32 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fired,
    Passed,
    Os,
}

#[derive(Clone, Copy)]
struct Entry {
    id: i32,
    kind: Kind,
    time: u32,
    /// How long after `time` a counterpart may be stamped and still be the same press.
    window: u32,
}

const RING: usize = 16;

/// Pairs the two deliveries of one press, so it reaches the module once.
///
/// **The rule.** A `WM_HOTKEY` and a hook press of the same hotkey are one press when the
/// message is stamped no earlier than the key event and no later than the moment the hook got
/// to run, plus [`SLACK_MS`]. A system that stopped waiting for the hook stopped at the latest
/// when the hook finally ran, and posted its `WM_HOTKEY` then; a hook that ran on time was
/// answered in time and there is no `WM_HOTKEY` at all — so a prompt hook's window is a few
/// dozen milliseconds, which no second press with a key-up in between can fall into. The
/// window depends on nothing but the two timestamps: not on `LowLevelHooksTimeout`, which
/// cannot be read reliably and whose default Microsoft does not document.
///
/// **Either order.** The hook runs on a thread of its own and the `WM_HOTKEY` arrives on the
/// pump's, so either can reach the queue first — a late hook, by definition, usually second.
/// Both orders are paired the same way.
///
/// A matched entry is consumed, so one press can cancel at most one other. The ring is small
/// and old entries are overwritten; an entry is only ever wanted for the length of its window.
pub(crate) struct Dedupe {
    ring: [Entry; RING],
    next: usize,
}

impl Default for Dedupe {
    fn default() -> Self {
        Dedupe {
            ring: [Entry {
                id: NO_ID,
                kind: Kind::Os,
                time: 0,
                window: 0,
            }; RING],
            next: 0,
        }
    }
}

/// `later` is stamped between `earlier` and `earlier + window`, on a clock that wraps.
fn within(earlier: u32, later: u32, window: u32) -> bool {
    let d = later.wrapping_sub(earlier) as i32;
    d >= 0 && (d as u32) <= window
}

impl Dedupe {
    fn push(&mut self, e: Entry) {
        self.ring[self.next] = e;
        self.next = (self.next + 1) % RING;
    }

    fn take(&mut self, pred: impl Fn(&Entry) -> bool) -> Option<Entry> {
        let i = self.ring.iter().position(|e| e.id != NO_ID && pred(e))?;
        let e = self.ring[i];
        self.ring[i].id = NO_ID;
        Some(e)
    }

    /// The hook dispatched a press of `id` (event `time`, hook `late`). `false` when a
    /// `WM_HOTKEY` has already delivered this same press, and it must not be dispatched again.
    pub(crate) fn hook_press(&mut self, id: i32, time: u32, late: u32) -> bool {
        let window = late.min(LATE_CAP_MS) + SLACK_MS;
        if self
            .take(|e| e.id == id && e.kind == Kind::Os && within(time, e.time, window))
            .is_some()
        {
            return false;
        }
        self.push(Entry {
            id,
            kind: Kind::Fired,
            time,
            window,
        });
        true
    }

    /// The hook saw a press of `id` and let it through, so a `WM_HOTKEY` for it is expected.
    pub(crate) fn hook_pass(&mut self, id: i32, time: u32, late: u32) {
        let window = late.min(LATE_CAP_MS) + SLACK_MS;
        self.push(Entry {
            id,
            kind: Kind::Passed,
            time,
            window,
        });
    }

    /// A `WM_HOTKEY` for `id`, stamped `time`.
    pub(crate) fn os_press(&mut self, id: i32, time: u32) -> OsPress {
        if let Some(e) =
            self.take(|e| e.id == id && e.kind == Kind::Fired && within(e.time, time, e.window))
        {
            return OsPress::Duplicate {
                late: e.window - SLACK_MS,
            };
        }
        if self
            .take(|e| e.id == id && e.kind == Kind::Passed && within(e.time, time, e.window))
            .is_some()
        {
            return OsPress::Deliver { expected: true };
        }
        self.push(Entry {
            id,
            kind: Kind::Os,
            time,
            window: 0,
        });
        OsPress::Deliver { expected: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: u32 = 0x56; // VK 'V'
    const F10: u32 = 0x79;
    const ALT: u8 = MASK_ALT;
    const CTRL_SHIFT: u8 = MASK_CTRL | MASK_SHIFT;

    /// A table with `id` granted for (`vk`, `mask`), through the one way in.
    fn granted(id: i32, vk: u32, mask: u8) -> Table {
        let mut t = Table::new();
        assert!(t.insert(id, vk, mask));
        t
    }

    #[test]
    fn modifier_flags_become_hook_masks() {
        // MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN | MOD_NOREPEAT
        assert_eq!(mods_to_mask(0x1), MASK_ALT);
        assert_eq!(mods_to_mask(0x2), MASK_CTRL);
        assert_eq!(mods_to_mask(0x4), MASK_SHIFT);
        assert_eq!(mods_to_mask(0x8), MASK_WIN);
        assert_eq!(mods_to_mask(0x4000 | 0x2 | 0x4), CTRL_SHIFT);
        assert_eq!(mods_to_mask(0), 0);
    }

    /// The parser the backend registers with and the one the hook's mask is compared against
    /// must describe the same combination, or the hook matches a key Windows did not grant.
    /// Both are the real ones: the backend's `parse_spec`, whose flags go to `RegisterHotKey`
    /// and to the table, and the shared `key_spec`, whose mask is what the hook computes.
    #[test]
    fn the_registered_spec_and_the_hook_mask_agree() {
        for spec in [
            "Ctrl+Shift+F10",
            "Alt+V",
            "Ctrl+Shift+Win+Alt+F6",
            "F5",
            "Win+Delete",
            "control+option+meta+shift+F24",
        ] {
            let (mods, vk) = crate::backend::windows::parse_spec(spec).unwrap();
            let (key_vk, mask) = crate::backend::key_spec(spec).unwrap();
            assert_eq!(vk, key_vk, "{spec}");
            assert_eq!(mods_to_mask(mods), mask, "{spec}");
            let mut t = None;
            assert!(file_if_granted(&mut t, true, 7, vk, mods), "{spec}");
            assert_eq!(t.unwrap().lookup(key_vk, mask), 7, "{spec}");
        }
    }

    #[test]
    fn a_refused_combination_is_never_matched() {
        // What register_hotkey hands on when RegisterHotKey said no.
        let (mods, vk) = crate::backend::windows::parse_spec("Ctrl+Shift+F10").unwrap();
        let mut t = None;
        assert!(!file_if_granted(&mut t, false, 3, vk, mods));
        assert!(t.is_none(), "a refusal must not even create the table");
        // And with a table there already, from another hotkey that was granted.
        assert!(file_if_granted(&mut t, true, 4, V, 0x1));
        assert!(!file_if_granted(&mut t, false, 3, vk, mods));
        let mut t = t.unwrap();
        assert_eq!(t.lookup(F10, CTRL_SHIFT), NO_ID);
        assert!(!t.holds(3));
        assert_eq!(t.on_down(F10, CTRL_SHIFT, 1000), Down::Miss);
        assert_eq!(t.on_down(V, ALT, 1000), Down::Fire(4));
    }

    /// The key grammar and the hook together: what `host.hotkey.register` claims
    /// (`hotkey_claim_for`, which the binding hands the backend) reaches the hook's table as the
    /// chord the spec the module wrote stands for — whichever spelling of a role it used —
    /// while a spec `register` refuses yields no claim at all, only the refusal the binding
    /// raises, so there is no spec to register with Windows or to file for the hook.
    #[test]
    fn a_spec_reaches_the_hook_as_its_chord_and_a_refused_one_is_never_claimed() {
        use crate::backend::windows::parse_spec;
        use crate::backend::{hotkey_claim_for, key_spec, KeyOs};
        for spec in [
            "Ctrl+S",
            "Cmd+S",
            "Ctrl+Shift+Win+Alt+F5",
            "Ctrl+Shift+Win+Alt+F6",
            "Ctrl+Shift+F9",
            "Ctrl+Alt+Shift+V",
            "Meta+Option+Shift+V",
            "ctrl + alt + q",
        ] {
            let (binding, shown) = hotkey_claim_for(KeyOs::Windows, spec).unwrap();
            let (mods, vk) = parse_spec(&shown).unwrap();
            assert_eq!(parse_spec(spec).unwrap(), (mods, vk), "{spec}, written as {shown}");
            let (key_vk, mask) = key_spec(spec).unwrap();
            assert_eq!(binding, Some((key_vk, mask)), "{spec}");
            assert_eq!((vk, mods_to_mask(mods)), (key_vk, mask), "{spec}");
            let mut t = None;
            assert!(file_if_granted(&mut t, true, 9, vk, mods), "{spec}");
            assert_eq!(t.unwrap().lookup(key_vk, mask), 9, "{spec}");
        }
        for spec in ["F12", "Win+L", "Meta+L", "Alt tap", "Ctrl tap"] {
            assert!(hotkey_claim_for(KeyOs::Windows, spec).is_err(), "{spec} must be refused");
        }
        // The tokens of 2026-09-21 are no modifiers any more: the claim keeps the spelling, and
        // the backend's parser refuses it before anything is filed.
        for spec in ["Mod+S", "Global+F6"] {
            assert_eq!(hotkey_claim_for(KeyOs::Windows, spec), Ok((None, spec.to_string())));
            assert!(parse_spec(spec).is_err(), "{spec}");
        }
        // The host's own registrations do not go through `register`; the backend refuses a tap
        // by itself, before `RegisterHotKey` and before the hook's table.
        assert!(parse_spec("Alt tap").is_err());
    }

    #[test]
    fn only_the_exact_combination_matches() {
        let mut t = granted(3, F10, CTRL_SHIFT);
        assert_eq!(t.on_down(F10, CTRL_SHIFT, 1000), Down::Fire(3));
        t.on_up(F10);
        // One modifier more or less is somebody else's key.
        assert_eq!(t.on_down(F10, MASK_CTRL, 2000), Down::Miss);
        t.on_up(F10);
        assert_eq!(t.on_down(F10, CTRL_SHIFT | MASK_ALT, 3000), Down::Miss);
        t.on_up(F10);
        assert_eq!(t.on_down(F10, 0, 4000), Down::Miss);
    }

    #[test]
    fn auto_repeat_is_swallowed_without_a_second_dispatch() {
        let mut t = granted(5, V, ALT);
        assert_eq!(t.on_down(V, ALT, 10_000), Down::Fire(5));
        // The keyboard delay, then the repeat rate.
        assert_eq!(t.on_down(V, ALT, 10_500), Down::Repeat);
        assert_eq!(t.on_down(V, ALT, 10_533), Down::Repeat);
        assert_eq!(t.on_down(V, ALT, 10_566), Down::Repeat);
        t.on_up(V);
        // Released and pressed again: a new press.
        assert_eq!(t.on_down(V, ALT, 10_700), Down::Fire(5));
    }

    #[test]
    fn a_press_whose_release_was_never_seen_does_not_eat_the_next_one() {
        let mut t = granted(5, V, ALT);
        assert_eq!(t.on_down(V, ALT, 10_000), Down::Fire(5));
        // The key-up happened on the secure desktop; the next press is minutes later.
        assert_eq!(t.on_down(V, ALT, 200_000), Down::Fire(5));
    }

    #[test]
    fn a_key_held_down_never_fires_again_whatever_the_modifiers_do() {
        let mut t = granted(5, V, ALT);
        assert_eq!(t.on_down(V, ALT, 1000), Down::Fire(5));
        // Alt released while V still repeats: plain V repeats belong to the application.
        assert_eq!(t.on_down(V, 0, 1500), Down::Miss);
        // Alt pressed again while V is still down: still the same press, as MOD_NOREPEAT
        // judges it, so it is swallowed and not dispatched a second time.
        assert_eq!(t.on_down(V, ALT, 1533), Down::Repeat);
        t.on_up(V);
        assert_eq!(t.on_down(V, ALT, 1700), Down::Fire(5));
    }

    #[test]
    fn a_key_already_held_does_not_become_a_hotkey_when_the_modifier_arrives() {
        let mut t = granted(5, V, ALT);
        // Typing: v is held and repeating, then Alt goes down on top of it. Not a press here;
        // let through, and RegisterHotKey answers for it — the pump is told to expect that.
        assert_eq!(t.on_down(V, 0, 1000), Down::Miss);
        assert_eq!(t.on_down(V, ALT, 1500), Down::Pass(5));
        assert_eq!(t.on_down(V, ALT, 1533), Down::Pass(5));
        t.on_up(V);
        assert_eq!(t.on_down(V, ALT, 1600), Down::Fire(5));
    }

    #[test]
    fn a_hotkey_granted_again_mid_hold_does_not_fire_on_the_repeat() {
        // An overlay releases and registers its control hotkeys on every focus move, so a
        // held combination can change id between two repeats.
        let mut t = granted(5, V, ALT);
        assert_eq!(t.on_down(V, ALT, 1000), Down::Fire(5));
        t.remove(5);
        t.insert(9, V, ALT);
        assert_eq!(t.on_down(V, ALT, 1500), Down::Repeat);
        t.on_up(V);
        assert_eq!(t.on_down(V, ALT, 1700), Down::Fire(9));
    }

    #[test]
    fn a_hotkey_released_mid_hold_hands_its_repeats_back() {
        let mut t = granted(5, V, ALT);
        assert_eq!(t.on_down(V, ALT, 1000), Down::Fire(5));
        assert!(t.remove(5));
        assert_eq!(t.on_down(V, ALT, 1500), Down::Miss);
        assert!(!t.holds(5));
        assert!(!t.remove(5), "nothing left to remove");
    }

    /// The captured keys' branch can take a key before the hotkeys see it. Its key-up and its
    /// key-down still have to reach the record, or the next real press reads as a repeat.
    #[test]
    fn a_key_the_captures_took_is_still_recorded() {
        const ONE: u32 = 0x31;
        let mut t = granted(5, ONE, ALT);
        // Alt+1 fires; then Ctrl goes down and Alt up, and the 1 is released as Ctrl+1 —
        // which a capture takes, key-up and all. The backend records the up all the same.
        assert_eq!(t.on_down(ONE, ALT, 1000), Down::Fire(5));
        t.on_up(ONE);
        // Pressed again at once: a new press, not a repeat swallowed without a callback.
        assert_eq!(t.on_down(ONE, ALT, 1400), Down::Fire(5));
        t.on_up(ONE);
        // A captured press of the key, then Alt arriving while it repeats: the key was held
        // before the combination, so the hotkey does not fire on its repeat.
        t.note_down(ONE, 2000);
        assert_eq!(t.on_down(ONE, ALT, 2500), Down::Pass(5));
        t.on_up(ONE);
        assert_eq!(t.on_down(ONE, ALT, 2600), Down::Fire(5));
        // Out of range is ignored rather than a panic in the hook.
        t.note_down(0x1_00, 0);
    }

    #[test]
    fn the_repeat_window_survives_the_tick_clock_wrapping() {
        let mut t = granted(5, V, ALT);
        assert_eq!(t.on_down(V, ALT, u32::MAX - 100), Down::Fire(5));
        assert_eq!(t.on_down(V, ALT, 400), Down::Repeat);
    }

    #[test]
    fn a_longer_keyboard_delay_raises_the_repeat_threshold() {
        // The Keyboard control panel's settings all stay under the default.
        for d in 0..=3 {
            assert_eq!(repeat_threshold(Some(d), None), REPEAT_MS, "setting {d}");
        }
        assert_eq!(repeat_threshold(None, None), REPEAT_MS);
        assert_eq!(repeat_threshold(Some(99), None), REPEAT_MS, "out of range reads as 3");
        // FilterKeys: a two-second delay, or a slow repeat, is a longer gap.
        assert_eq!(repeat_threshold(Some(1), Some((2_000, 500))), 2_200);
        assert_eq!(repeat_threshold(Some(1), Some((1_000, 1_500))), 1_700);
        assert_eq!(repeat_threshold(Some(3), Some((0, 0))), REPEAT_MS);
    }

    #[test]
    fn a_held_hotkey_fires_once_under_filter_keys() {
        let mut t = granted(5, V, ALT);
        t.set_repeat_ms(repeat_threshold(Some(1), Some((2_000, 1_000))));
        assert_eq!(t.on_down(V, ALT, 10_000), Down::Fire(5));
        // The first repeat two seconds later, then one a second.
        assert_eq!(t.on_down(V, ALT, 12_000), Down::Repeat);
        assert_eq!(t.on_down(V, ALT, 13_000), Down::Repeat);
        // With the default threshold the same repeat would have fired again.
        let mut d = granted(5, V, ALT);
        assert_eq!(d.on_down(V, ALT, 10_000), Down::Fire(5));
        assert_eq!(d.on_down(V, ALT, 12_000), Down::Fire(5));
    }

    #[test]
    fn remove_takes_only_its_own_id_and_holds_reports_it() {
        let mut t = granted(1, V, ALT);
        t.insert(2, F10, CTRL_SHIFT);
        assert!(t.holds(1) && t.holds(2));
        assert!(t.remove(1));
        assert!(!t.holds(1));
        assert_eq!(t.lookup(F10, CTRL_SHIFT), 2);
        assert!(t.remove(2));
        assert!(!t.holds(2));
        assert_eq!(t.lookup(F10, CTRL_SHIFT), NO_ID);
    }

    #[test]
    fn out_of_range_combinations_are_not_filed() {
        let mut t = Table::new();
        assert!(!t.insert(1, 0x1_00, 0));
        assert!(!t.insert(1, V, 0x10), "the tap bit is not a modifier");
        assert!(!t.insert(NO_ID, V, ALT));
        assert_eq!(t.lookup(V, ALT), NO_ID);
        assert_eq!(t.on_down(0x1_00, 0, 0), Down::Miss);
        let mut none = None;
        assert!(!file_if_granted(&mut none, true, 1, 0x1_00, 0));
    }

    #[test]
    fn mask_key_rule() {
        assert!(
            needs_mask_key(MASK_ALT),
            "Alt released alone opens the menu bar"
        );
        assert!(needs_mask_key(MASK_WIN), "Win released alone opens Start");
        assert!(
            needs_mask_key(MASK_ALT | MASK_SHIFT),
            "the default language switch"
        );
        assert!(needs_mask_key(CTRL_SHIFT), "the other language switch");
        assert!(needs_mask_key(MASK_CTRL | MASK_SHIFT | MASK_ALT | MASK_WIN));
        assert!(!needs_mask_key(MASK_CTRL));
        assert!(!needs_mask_key(MASK_SHIFT));
        assert!(!needs_mask_key(0));
    }

    #[test]
    fn lateness_trusts_only_a_stamp_it_can() {
        assert_eq!(lateness(10_000, 10_000, false), 0);
        assert_eq!(lateness(10_600, 10_000, false), 600);
        // Several timeouts in a row are still measured, not cut to one.
        assert_eq!(lateness(12_900, 10_000, false), 2_900);
        assert_eq!(lateness(100_000, 10_000, false), LATE_CAP_MS);
        // Ahead of the clock, or injected: taken as on time.
        assert_eq!(lateness(10_000, 10_500, false), 0);
        assert_eq!(lateness(10_600, 10_000, true), 0);
        // Across the wrap.
        assert_eq!(lateness(200, u32::MAX - 99, false), 300);
    }

    /// Judged as of the key event: plain v typed during a stall, with Alt held by the time the
    /// hook got to it, is plain v.
    #[test]
    fn a_late_call_is_judged_by_the_modifiers_it_saw_go_by() {
        let mut m = Mods::default();
        // On time: the system's answer, and the record follows it.
        assert_eq!(event_mask(&mut m, 0, || 0), 0);
        m.on_key(V, true);
        m.on_key(V, false);
        // A stall. The v went down with nothing held; Alt went down after it. The hook
        // gets to both late, in order, while the system says Alt is down now.
        let asked_now = || MASK_ALT;
        assert_eq!(event_mask(&mut m, 700, asked_now), 0, "v was plain");
        m.on_key(V, true);
        assert_eq!(event_mask(&mut m, 690, asked_now), 0, "Alt's own event");
        m.on_key(0xA4, true);
        // A key after Alt, still late, is judged with Alt.
        assert_eq!(event_mask(&mut m, 680, asked_now), MASK_ALT);
    }

    #[test]
    fn the_modifier_record_keeps_sides_and_follows_the_system_on_time() {
        let mut m = Mods::default();
        m.on_key(0xA0, true); // left Shift
        m.on_key(0xA1, true); // right Shift
        m.on_key(0xA0, false);
        assert_eq!(m.mask(), MASK_SHIFT, "the right one is still down");
        m.on_key(0xA1, false);
        assert_eq!(m.mask(), 0);
        // A release the hook never saw (the secure desktop): the next call on time repairs it.
        m.on_key(0xA2, true); // left Ctrl
        assert_eq!(m.mask(), MASK_CTRL);
        assert_eq!(event_mask(&mut m, 10, || 0), 0);
        assert_eq!(m.mask(), 0);
        // One the system has down and the record does not: taken as the left key, and its
        // right-hand release does not clear it — the next call on time does.
        assert_eq!(event_mask(&mut m, 0, || MASK_WIN | MASK_ALT), MASK_WIN | MASK_ALT);
        assert_eq!(m.mask(), MASK_WIN | MASK_ALT);
        m.on_key(0x5C, false); // right Win up
        assert_eq!(m.mask(), MASK_WIN | MASK_ALT);
        assert_eq!(event_mask(&mut m, 0, || MASK_ALT), MASK_ALT);
        assert_eq!(m.mask(), MASK_ALT);
        // Generic codes, as injected input carries them.
        let mut g = Mods::default();
        g.on_key(0x10, true);
        g.on_key(0x11, true);
        g.on_key(0x12, true);
        assert_eq!(g.mask(), MASK_SHIFT | MASK_CTRL | MASK_ALT);
        g.on_key(0x56, true);
        assert_eq!(g.mask(), MASK_SHIFT | MASK_CTRL | MASK_ALT, "not a modifier");
    }

    #[test]
    fn a_prompt_hook_leaves_nothing_for_a_later_press_to_collide_with() {
        let mut d = Dedupe::default();
        // The hook ran 2 ms after the key; Windows got its answer and posted nothing.
        assert!(d.hook_press(5, 10_000, 2));
        // Half a second later the same hotkey arrives through RegisterHotKey — the hook did
        // not see this second press (an elevated window was in front, say). It is a press.
        assert_eq!(d.os_press(5, 10_500), OsPress::Deliver { expected: false });
    }

    #[test]
    fn a_late_hook_and_its_wm_hotkey_are_one_press() {
        let mut d = Dedupe::default();
        // The hook's thread did not run for 600 ms: the system gave up on the hook,
        // RegisterHotKey fired (stamped when it did), and the hook's call still ran after.
        assert!(d.hook_press(5, 10_000, 600));
        assert_eq!(d.os_press(5, 10_310), OsPress::Duplicate { late: 600 });
        // Consumed: the next press is a press again.
        assert_eq!(d.os_press(5, 10_320), OsPress::Deliver { expected: false });
    }

    #[test]
    fn the_other_order_is_one_press_too() {
        let mut d = Dedupe::default();
        assert_eq!(d.os_press(5, 10_310), OsPress::Deliver { expected: false });
        assert!(
            !d.hook_press(5, 10_000, 600),
            "already delivered through WM_HOTKEY"
        );
        // A later press through the hook is not mistaken for that one.
        assert!(d.hook_press(5, 11_000, 3));
    }

    /// Ctrl+Shift+F10 with a one-second timeout and a hook that could not run: Ctrl and Shift
    /// each waited their timeout before F10 did, so the WM_HOTKEY is stamped nearly three
    /// seconds after the F10 event. Still one press.
    #[test]
    fn a_hook_late_by_several_timeouts_still_pairs() {
        let mut d = Dedupe::default();
        let late = lateness(13_400, 10_000, false);
        assert!(d.hook_press(3, 10_000, late));
        assert_eq!(d.os_press(3, 12_900), OsPress::Duplicate { late: 3_400 });
        let mut d = Dedupe::default();
        assert_eq!(d.os_press(3, 12_900), OsPress::Deliver { expected: false });
        assert!(!d.hook_press(3, 10_000, late));
    }

    #[test]
    fn a_wm_hotkey_stamped_before_the_key_event_is_another_press() {
        let mut d = Dedupe::default();
        assert!(d.hook_press(5, 10_000, 600));
        assert_eq!(d.os_press(5, 9_990), OsPress::Deliver { expected: false });
    }

    #[test]
    fn pairing_is_per_hotkey() {
        let mut d = Dedupe::default();
        assert!(d.hook_press(5, 10_000, 600));
        assert_eq!(d.os_press(6, 10_100), OsPress::Deliver { expected: false });
        assert_eq!(d.os_press(5, 10_100), OsPress::Duplicate { late: 600 });
    }

    #[test]
    fn a_pass_is_expected_and_is_delivered() {
        let mut d = Dedupe::default();
        // A screen reader's modifier was held: the hook let the press through.
        d.hook_pass(5, 10_000, 1);
        assert_eq!(d.os_press(5, 10_004), OsPress::Deliver { expected: true });
        assert_eq!(d.os_press(5, 12_000), OsPress::Deliver { expected: false });
    }

    #[test]
    fn a_bogus_timestamp_cannot_open_an_absurd_window() {
        let mut d = Dedupe::default();
        assert!(d.hook_press(5, 0, u32::MAX));
        assert_eq!(
            d.os_press(5, LATE_CAP_MS + SLACK_MS + 1),
            OsPress::Deliver { expected: false }
        );
        // And an injected press, whose stamp is its sender's, pairs only within the slack.
        let mut d = Dedupe::default();
        assert!(d.hook_press(5, 10_000, lateness(20_000, 10_000, true)));
        assert_eq!(d.os_press(5, 10_000 + SLACK_MS + 1), OsPress::Deliver { expected: false });
    }

    #[test]
    fn pairing_survives_the_tick_clock_wrapping() {
        let mut d = Dedupe::default();
        assert!(d.hook_press(5, u32::MAX - 100, 400));
        assert_eq!(d.os_press(5, 200), OsPress::Duplicate { late: 400 });
    }

    #[test]
    fn the_ring_forgets_the_oldest_first() {
        let mut d = Dedupe::default();
        assert!(d.hook_press(1, 1_000, 900));
        for id in 2..=RING as i32 + 1 {
            assert!(d.hook_press(id, 1_000, 900));
        }
        // Hotkey 2's entry is the oldest left, and still pairs.
        assert_eq!(d.os_press(2, 1_500), OsPress::Duplicate { late: 900 });
        // Hotkey 1's was overwritten; its WM_HOTKEY is delivered rather than dropped.
        assert_eq!(d.os_press(1, 1_500), OsPress::Deliver { expected: false });
    }
}
