//! Capturing and suppressing keys, through a `CGEventTap` on a thread of its own.
//!
//! This is the fragile one. Every key of the Mac passes the tap before any application sees it,
//! and a tap that takes too long inside its callback is switched off by the system and stays off,
//! so the callback matches against a table and pushes; anything else happens later, on the pump.
//! A watchdog re-enables it and says so in the log, because a tap that has quietly died looks
//! exactly like an overlay that stopped working for no reason.
//!
//! **Its own thread** (`key-tap`, at user-interactive priority, running a run loop of its own), as
//! the Windows hook has one (`backend/hotkey_hook.rs`). On the main run loop, where it was until
//! 2026-10, every stall of the main thread held every key of the Mac, VoiceOver's included, and
//! from about a second on the system switched the tap off: the second Mac tester's log has 128
//! late-key lines and 68 switch-offs, each after a stall of ours. Now:
//!
//! - the callback decides alone, from what the main thread told it — the captured set and the
//!   scopes (one mutex, which the main thread holds for a copy), the window in front, the menu and
//!   own-window gates and the layout's letters (atomics) — and never waits for the main thread's
//!   work;
//! - a key it takes is suppressed at once and queued (`tap_queue::Handover`, `queue.rs`): while the
//!   main thread is busy the key waits there, never reaching the application underneath, and the
//!   rest of the keyboard is not held at all;
//! - so that a busy main thread cannot have keys taken for a window the user has left, the tap asks
//!   the window server which application is in front for each key it would take for a capture
//!   pinned to a window, and lets the key through to another one ([`front_moved`], `tap_front.rs`);
//! - nothing on the tap's thread writes a line, the callback least of all — the log's lock is held
//!   for the main thread's file writes: what a line needs is left in atomics and small queues
//!   (`tap_handoff.rs`, `tap_queue::Notes`), and an observer on the main run loop writes it
//!   ([`report_observer`]);
//! - the watchdog and a re-creation run on the tap's thread, whose run loop holds the port.
//!
//! **Over days of uptime** the tap can fail in four ways, and each has an answer here:
//!
//! - *Switched off, and the system says so* (`kCGEventTapDisabledByTimeout`,
//!   `kCGEventTapDisabledByUserInput`, delivered through the callback): re-enabled in the
//!   callback at once; the log line is written afterwards, by the observer on the main run loop,
//!   because a file write inside the callback — just after a wake, with the disk still spinning
//!   up — is the kind of delay that switches a tap off again.
//! - *Switched off without a word*: the watchdog, a two-second timer on the tap's run loop, asks
//!   `CGEventTapIsEnabled` and re-enables it.
//! - *Its mach port invalid* — the tap is gone, and re-enabling a dead port does nothing, so a
//!   watchdog that only re-enabled would announce a repair every two seconds while no key
//!   arrived: the tap is created again (`recreate`). One that stays off after being re-enabled
//!   is created again once; if the new one is off as well — a permission, or secure input — it
//!   is left alone until the tap reports itself on or one of the moments below, rather than
//!   created again every two seconds.
//! - *Asleep, locked, switched away from* — the moments the tap is most likely to have been
//!   touched: when the Mac wakes, the screen is unlocked or this user's session becomes active
//!   again after fast user switching (`system.rs` hears all three and calls [`recheck_soon`]),
//!   the watchdog checks at once, woken on the tap's run loop, without waiting for its two
//!   seconds, and says what it found. While the screen is locked, or another user's session is in front,
//!   the tap sees no keys at all; that is the system's secure input, not a fault, and nothing is
//!   done about it.
//!
//! Unlike Windows, where a removed low-level hook leaves no trace and needs a witness to notice
//! (see `backend/hook_watch.rs`), each of these four the application can see for itself. One
//! failure it sees only by a witness: a tap that is valid and enabled and never called — Input
//! Monitoring withdrawn, or never granted on the versions that ask for it separately. The witness
//! is a shortcut of this application's own, which Carbon delivers without any permission and
//! whose key passes the tap's place first: one that arrives while the tap has seen no key at all
//! is that failure ([`note_shortcut`]), and the Permissions page then counts Input Monitoring as
//! needed. Without a shortcut pressed, the line written when the tap is installed still says to
//! check Input Monitoring when no key is ever captured after it.
//!
//! Matching is EXACT: the pressed modifier state must equal the mask, so a capture of "Tab"
//! does not swallow Command-Tab. Both halves of a suppressed key are swallowed — letting
//! the key-up through hands the application underneath an orphan release.
//!
//! The state the two threads share lives in atomics and in `std` mutexes held for a copy, a push
//! or a swap, never across a call out of the process, and never in a `thread_local`, which the
//! other thread would find silently empty.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use objc2_core_foundation::{
    kCFRunLoopCommonModes, kCFRunLoopDefaultMode, CFAbsoluteTimeGetCurrent, CFMachPort, CFRetained,
    CFRunLoop, CFRunLoopActivity, CFRunLoopObserver, CFRunLoopSource, CFRunLoopSourceContext,
    CFRunLoopTimer, Type,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapInformation, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType, CGError,
    CGGetEventTapList,
};

use super::tap_front::{self, FrontCheck};
use super::tap_queue::{Notes, Shortcuts, Stamp};
use super::{key_age, keys, queue, tap_handoff, watch};
use crate::backend::{capture_decision, menu_flag_owners, Capture, Captured, OwnerKeys, PassWhy, Pressed, Taken, MASK_TAP};
use crate::errors::HostError;
use crate::logging;

/// What the tap asks to see.
///
/// `FlagsChanged` is in here for the modifier tap and for nothing else: macOS raises no
/// key-down for a modifier at all, so without it a bare Option press is simply invisible and
/// `MASK_TAP` could never fire. The two `TapDisabled*` types are deliberately NOT in the
/// mask — they are delivered regardless, and their numeric values are near `u32::MAX`, so
/// feeding them to a `1 << type` mask builder would shift past the end of the word.
const TAP_MASK: CGEventMask = (1u64 << CGEventType::KeyDown.0)
    | (1u64 << CGEventType::KeyUp.0)
    | (1u64 << CGEventType::FlagsChanged.0);

/// One physical modifier key: its keycode, the flag it contributes, and the Win32 code the
/// host knows its ROLE by.
///
/// Left and right collapse onto one `vk` because that is what `key_spec` produces for a tap
/// spec — `"Alt tap"` is `(0x12, MASK_TAP)`, with no way to say which side — and because a
/// module asking for a bare Option press does not care which thumb produced it.
///
/// The collapse is here rather than in `keys.rs` on purpose: `key_to_vk` deliberately has no
/// entry for a bare modifier, so that `"Alt"` cannot be used as an ordinary hotkey, and the
/// left/right collapse is a rule about taps rather than about key translation. Which key each
/// role is comes from `keys::MODIFIER_KEYS`, the one role table: a Command key is the Ctrl
/// role's key (vk 0x11, what `"Ctrl tap"` names), a Control key the Win role's (0x5B).
struct Modifier {
    keycode: u16,
    flag: CGEventFlags,
    vk: u32,
}

/// One side of one role's key.
const fn side(role: &keys::MacModifier, i: usize) -> Modifier {
    Modifier { keycode: role.keycodes[i], flag: CGEventFlags(role.cg_flag), vk: role.vk }
}

/// Keycodes are Carbon's `kVK_*`: Command 0x37, Shift 0x38, Option 0x3A, Control 0x3B, and
/// the right-hand ones 0x36, 0x3C, 0x3D, 0x3E, all from `keys::MODIFIER_KEYS`. Caps Lock (0x39)
/// and Fn (0x3F) are absent because neither carries a mask bit the host can name.
const MODIFIERS: [Modifier; 8] = {
    let k = &keys::MODIFIER_KEYS;
    [
        side(&k[0], 0),
        side(&k[0], 1),
        side(&k[1], 0),
        side(&k[1], 1),
        side(&k[2], 0),
        side(&k[2], 1),
        side(&k[3], 0),
        side(&k[3], 1),
    ]
};

// The role table writes Quartz's flags out as numbers so that it compiles, and is tested, on
// any platform; these hold them to the bindings' own, where the bindings exist.
const _: () = assert!(keys::CG_FLAG_SHIFT == CGEventFlags::MaskShift.0);
const _: () = assert!(keys::CG_FLAG_CONTROL == CGEventFlags::MaskControl.0);
const _: () = assert!(keys::CG_FLAG_ALTERNATE == CGEventFlags::MaskAlternate.0);
const _: () = assert!(keys::CG_FLAG_COMMAND == CGEventFlags::MaskCommand.0);

/// Whether this run has ever suppressed a key. See the one-shot line in the tap callback:
/// on macOS a tap that was never granted Input Monitoring is indistinguishable in the log
/// from one that works, unless something says so when it first does work.
static FIRST_SUPPRESSION: AtomicBool = AtomicBool::new(false);
/// That first key, its vk and mask (`tap_handoff::pack`), for the observer to say once
/// ([`report_first_suppression`]); 0 before it and once said.
static FIRST_SUPPRESSED: AtomicU64 = AtomicU64::new(0);

/// The captured keys let through since the observer last looked, one slot per reason
/// (`tap_handoff::slot`): the last such key's vk and mask, packed, 0 while there is none. One
/// relaxed store in the callback; taken and written down by [`report_gate_pass`]. As long as the
/// list of reasons, which `tap_handoff` holds to its slots.
static GATE_PASSED: [AtomicU64; tap_handoff::REASONS.len()] = [const { AtomicU64::new(0) }; tap_handoff::REASONS.len()];
/// When the observer last said each reason's line, for its quiet three seconds ([`due`]).
static GATE_SAID_MS: [AtomicU64; tap_handoff::REASONS.len()] = [const { AtomicU64::new(0) }; tap_handoff::REASONS.len()];

static INSTALLED: AtomicBool = AtomicBool::new(false);

// What the key capture has shown about Input Monitoring, for [`keys_seen`]: the evidence by which
// it is optional (`evidence.rs`). Each is one relaxed access where it is noted.
/// The callback has been handed a key-down, from anybody, since the tap was installed.
static CALLED: AtomicBool = AtomicBool::new(false);
/// A shortcut of this application arrived while the installed tap had seen no key at all.
static DEAF: AtomicBool = AtomicBool::new(false);
/// The last creation of the tap was refused while Accessibility was granted.
static REFUSED_TRUSTED: AtomicBool = AtomicBool::new(false);
/// When the tap was last switched on — installed, re-enabled after the system or the watchdog
/// found it off, or created again — in [`now_ms`]'s clock, plus one; 0 while there is none.
static ON_SINCE_MS: AtomicU64 = AtomicU64::new(0);

/// How long the tap must have been on before a shortcut that it did not see counts: a key pressed
/// just before the tap existed, or while it was off, reaches a shortcut without passing it, and a
/// second is far longer than the event takes from the keyboard to the shortcut's handler.
const DEAF_AFTER_MS: u64 = 1000;

/// Notes that the tap has just been switched on, for [`note_shortcut`]. One clock read and one
/// relaxed store, so the callback can afford it.
fn note_on() {
    ON_SINCE_MS.store(now_ms() + 1, Ordering::Relaxed);
}

/// The tap's mach port, kept as a raw pointer so the callback and the watchdog can re-enable
/// it. The retain taken at creation is released only by [`recreate`], after it has swapped a
/// new port in: the tap lives as long as the process, and a dangling port here would be
/// dereferenced from inside an OS callback. The callback, the watchdog and [`recreate`] run on the
/// tap's thread, so none of them can hold the old port while it goes; the one reader on another
/// thread, [`note_shortcut`] on the main thread, reads it under [`PORT_LOCK`], which [`recreate`]
/// holds while it swaps and releases.
static PORT: AtomicPtr<CFMachPort> = AtomicPtr::new(std::ptr::null_mut());
static PORT_LOCK: Mutex<()> = Mutex::new(());

/// The tap's run loop and the source that wakes its watchdog at once ([`recheck_soon`]), for the
/// main thread to signal. Retained for the life of the process — the tap's thread never ends — so
/// the main thread can use them through these pointers at any time.
static TAP_LOOP: AtomicPtr<CFRunLoop> = AtomicPtr::new(std::ptr::null_mut());
static RECHECK_SOURCE: AtomicPtr<CFRunLoopSource> = AtomicPtr::new(std::ptr::null_mut());

/// The captures the modules currently want, each with its module, and what each module set with
/// `host.keys.scope` and `host.keys.menuOpen`: what [`crate::backend::capture_decision`] decides
/// every key by. Replaced by the host, each half wholesale; one lock, so the callback never
/// reads a set and the owners from two different moments. The main thread holds it for a copy of
/// a few pairs, and the tap's thread for one decision, so the callback waits for it — a
/// microsecond at most — rather than let a key through untaken.
static KEY_STATE: Mutex<KeyState> = Mutex::new(KeyState { set: Vec::new(), owners: Vec::new() });

/// The tap's copy of the host's key table — see [`KEY_STATE`].
struct KeyState {
    set: Vec<Captured>,
    owners: Vec<OwnerKeys>,
}

/// The frontmost window as `watch` last saw it: the window every module's scope is compared
/// with, read rather than asked — see [`decide`].
static FOREGROUND: AtomicIsize = AtomicIsize::new(0);

/// The application the main thread named with the window it last told the tap of
/// ([`note_foreground`]): the window, its application's pid, and that application as the Process
/// Manager names it, for the tap's question which application is in front ([`front_moved`]).
static TOLD: Mutex<Told> = Mutex::new(Told { window: 0, pid: None, psn: None });

struct Told {
    window: isize,
    pid: Option<i32>,
    psn: Option<u64>,
}

/// What the tap's question which application is in front ([`front_moved`]) found, for the observer
/// to write ([`report_front`]): the first answer's time in µs plus one (0 before it, and once said),
/// the answers of a millisecond or more since the last line and the worst of them, the run of
/// answers too slow in a row (`tap_front::slow_run`, the tap's thread alone), a question switched
/// off by that run (the last answer's time in ms plus one; cleared by [`health_check`] after a
/// wake, an unlock or the session back), and a key it let through — its key and mask packed
/// (`tap_handoff::pack`), and the application the window server named.
static FRONT_FIRST_US: AtomicU64 = AtomicU64::new(0);
static FRONT_FIRST_SAID: AtomicBool = AtomicBool::new(false);
static FRONT_SLOW: AtomicU32 = AtomicU32::new(0);
static FRONT_SLOW_WORST_US: AtomicU64 = AtomicU64::new(0);
static FRONT_SLOW_SAID_MS: AtomicU64 = AtomicU64::new(0);
static FRONT_SLOW_RUN: AtomicU32 = AtomicU32::new(0);
static FRONT_OFF_MS: AtomicU64 = AtomicU64::new(0);
static FRONT_OFF_SAID: AtomicBool = AtomicBool::new(false);
static FRONT_PASSED: AtomicU64 = AtomicU64::new(0);
static FRONT_PASSED_PSN: AtomicU64 = AtomicU64::new(0);
static FRONT_PASSED_SAID_MS: AtomicU64 = AtomicU64::new(0);

/// The keys this application's shortcuts are registered on, with the stamp of each one's last
/// key-down (`tap_queue::Shortcuts`), and their keycodes as a bitmap over the 128 keycodes in two
/// words, for the callback to look before it takes the lock. Written by `hotkey.rs` on the main
/// thread ([`set_shortcut_keys`]); stamped by the callback, claimed by Carbon's handler.
static SHORTCUTS: Mutex<Shortcuts> = Mutex::new(Shortcuts::new());
static SHORTCUT_CODES: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

/// What the tap's thread found about the tap, for the observer to write ([`report_notes`]).
static NOTES: Notes<TapNote> = Notes::new(16);
/// The watchdog's timer or its wake-up source could not be made on the tap's thread
/// ([`install_watchdog`]), for [`install`] to say.
static WATCHDOG_MISSING: AtomicBool = AtomicBool::new(false);
/// The trace lines of the callback, for the observer to write while tracing is on
/// ([`report_traces`]).
static TRACES: Notes<TraceNote> = Notes::new(64);

/// Keycode + 1 of the modifier held with nothing pressed since, 0 when none is.
static TAP_ARMED: AtomicU32 = AtomicU32::new(0);

/// How many key-downs this tap has let go on towards the program, wrapping: every `KeyDown` it
/// returns, captured by nobody — typed, let through for a menu, a held key's repeats, posted by
/// another program or by `host.input` — and never one it swallowed for a capture, nor one posted to
/// pass a key on (`input::pass_on`). A modifier is a `FlagsChanged`, never counted. Each taken key
/// carries the count of its press (`crate::backend::Pressed::seq`), and a key passed on later is
/// sent only while the count has not moved since: else a key typed after it has reached the program
/// first (`crate::backend::NotPassed::Overtaken`). One relaxed add per key-down.
static LET_THROUGH: AtomicU32 = AtomicU32::new(0);

/// The key-downs of [`LET_THROUGH`] that end a count of quick repeats: all but those
/// `host.input.send` posted, which carry [`keys::SEND_MARK`], wrapping. Each press carries it
/// beside `LET_THROUGH`'s (`crate::backend::Pressed::taps_seq`), so that a key a module sends ends
/// no count while it still overtakes a key waiting to be passed on. One relaxed add per key-down it
/// counts.
static TAPS_THROUGH: AtomicU32 = AtomicU32::new(0);

/// The count of key-downs let through so far ([`LET_THROUGH`]).
pub fn let_through() -> u32 {
    LET_THROUGH.load(Ordering::Relaxed)
}

/// How many of them end a count of quick repeats ([`TAPS_THROUGH`]).
pub fn taps_through() -> u32 {
    TAPS_THROUGH.load(Ordering::Relaxed)
}

/// How many keys this tap has seen pressed, wrapping: every `KeyDown` that is not the keyboard's
/// auto-repeat, whatever became of it, and none that this process posted — to pass a key on, or
/// through `host.input`. A focus waiting for another application's activation (`front.rs`) is given
/// up when the count has moved by the time that activation comes: the user pressed a key after it
/// was asked for, and that key went somewhere else. One relaxed add per key pressed.
static PRESSED: AtomicU32 = AtomicU32::new(0);

/// The count of keys pressed so far ([`PRESSED`]); it never moves while the tap is not installed.
pub(super) fn pressed() -> u32 {
    PRESSED.load(Ordering::Relaxed)
}

/// Which entries of `MODIFIERS` are physically down, one bit each.
///
/// Needed because a `FlagsChanged` event does not say whether the key went down or up: it
/// reports the keycode and the resulting flags, and with two Options held the Alternate flag
/// stays set through the release of one of them. Remembering which side was down is the only
/// way to read the direction of the change without the undocumented device-dependent flag
/// bits.
static MOD_DOWN: AtomicU32 = AtomicU32::new(0);

/// Keycodes whose key-DOWN this tap swallowed, so the matching key-up can be swallowed too.
///
/// The Windows hook re-derives the match on the key-up and suppresses that instead, which
/// leaves two holes: releasing the modifier first changes the mask so the up no longer
/// matches, and a menu opening between the two halves opens the gate for an up whose down
/// the application never saw. Remembering the down closes both — the application sees a
/// matched pair or neither half, never one of them.
static SUPPRESSED_LO: AtomicU64 = AtomicU64::new(0);
static SUPPRESSED_HI: AtomicU64 = AtomicU64::new(0);

/// How often the system has switched the tap off. A number that keeps climbing is the
/// early warning that something on this thread is too slow, not a curiosity.
static REENABLES: AtomicU32 = AtomicU32::new(0);

/// How often the system said it switched the tap off, by each of its two reasons, since the
/// observer last wrote it down — counted in the callback, written by [`report_disabled`].
static DISABLED_BY_TIMEOUT: AtomicU32 = AtomicU32::new(0);
static DISABLED_BY_USER_INPUT: AtomicU32 = AtomicU32::new(0);

/// How often the tap had to be created again (see [`recreate`]).
static RECREATES: AtomicU32 = AtomicU32::new(0);
/// A failed re-creation has been said; cleared by the next one that works.
static RECREATE_FAILURE_SAID: AtomicBool = AtomicBool::new(false);
/// The tap was created again because it stayed off, and the new one is off as well. Until the
/// tap reports itself on, or a wake, an unlock or this session becoming active again asks for a
/// fresh look, it is not created again: whatever keeps it off (a permission, secure input)
/// would keep every new one off too, and each round would cost a line and a port.
static STAYS_OFF: AtomicBool = AtomicBool::new(false);

/// Why the tap is to be checked at once rather than within the watchdog's two seconds: bits set
/// through [`recheck_soon`] when `system.rs` hears of a wake, an unlock or the session coming back.
static RECHECK: AtomicU32 = AtomicU32::new(0);
pub(super) const RECHECK_WAKE: u32 = 1;
pub(super) const RECHECK_UNLOCK: u32 = 2;
pub(super) const RECHECK_SESSION: u32 = 4;

/// Key presses that reached this callback late, and how late the worst of them was.
///
/// The question behind it was whether a busy main thread delays keys, while the tap ran on the
/// main run loop: the second Mac tester's log answered it — 128 such lines, each beside a stall
/// of ours — and the tap moved to a thread of its own. The line stays, and now measures that
/// thread: no work of the main thread can make a key late here any more, so a late key is the
/// system's delay in handing it over — App Nap, or a Mac too busy to run the tap's thread
/// (`activity.rs`). What a busy main thread still costs an overlay's user is measured where the
/// key is handed to its module (`queue.rs`, "waited for the event loop").
///
/// Recorded in the callback — three relaxed atomic writes and a keycode lookup, nothing that
/// locks, allocates or logs — and written by the observer on the main run loop, because a log
/// line from inside the callback is exactly the kind of slowness that gets a tap switched off.
static LATE_KEYS: AtomicU32 = AtomicU32::new(0);
static LATE_WORST_MS: AtomicU64 = AtomicU64::new(0);
static LATE_LAST_VK: AtomicU32 = AtomicU32::new(0);
static LAST_LATE_REPORT_MS: AtomicU64 = AtomicU64::new(0);

/// A key press this much older than the callback is reported. Well above a normal hand-over
/// and well below the ~1 s at which the system gives up on a tap.
const LATE_KEY_MS: u64 = 250;

// The two clocks an event timestamp can be in; see `key_age`. Declared here rather than taken
// from `libc`, which marks the mach one deprecated in favour of a crate this needs nothing else
// from. Both are in libSystem, which every process links.
extern "C" {
    fn mach_absolute_time() -> u64;
    fn clock_gettime_nsec_np(clock_id: u32) -> u64;
}
/// `CLOCK_UPTIME_RAW`: `mach_absolute_time` in nanoseconds, not counting sleep.
const CLOCK_UPTIME_RAW: u32 = 8;

/// Notes a key press that reached the callback late. Called for every key-down; when the key
/// was on time it costs the event's timestamp, two clock reads and the age arithmetic.
fn note_lateness(ev: &CGEvent, keycode: u16) {
    let ts = CGEvent::timestamp(Some(ev));
    // SAFETY: neither takes arguments that could be wrong; both read a clock.
    let (ticks, ns) = unsafe { (mach_absolute_time(), clock_gettime_nsec_np(CLOCK_UPTIME_RAW)) };
    let Some(age) = key_age::age_ns(ts, ticks, ns) else {
        return;
    };
    let ms = age / 1_000_000;
    if ms < LATE_KEY_MS {
        return;
    }
    LATE_KEYS.fetch_add(1, Ordering::Relaxed);
    LATE_WORST_MS.fetch_max(ms, Ordering::Relaxed);
    LATE_LAST_VK.store(keys::keycode_to_vk(keycode).unwrap_or(0), Ordering::Relaxed);
}

/// Writes what `note_lateness` gathered, at most once a second, from the idle main run loop.
fn report_lateness() {
    if LATE_KEYS.load(Ordering::Relaxed) == 0 || !due(&LAST_LATE_REPORT_MS, 1000) {
        return;
    }
    let n = LATE_KEYS.swap(0, Ordering::Relaxed);
    let worst = LATE_WORST_MS.swap(0, Ordering::Relaxed);
    let vk = LATE_LAST_VK.load(Ordering::Relaxed);
    if n == 0 {
        return;
    }
    logging::line(
        "macos",
        &format!(
            "{n} key press(es) reached the event tap late, the worst {worst} ms after it was \
             pressed (last vk {vk:#04x}) — the tap's own thread was late, which no work of the main \
             thread delays: the system handed the key over late (App Nap, or a Mac too busy to run \
             the tap's thread)"
        ),
    );
}

/// Milliseconds since the first call, for rate limiting. `Instant` cannot be a `const`
/// initialiser and the callback must not allocate or lock to find out what time it is.
fn now_ms() -> u64 {
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    CLOCK.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// True at most once per `gap_ms`, for the log lines that would otherwise repeat per
/// keystroke. A permanently missing permission should say so, not fill the file.
fn due(last: &AtomicU64, gap_ms: u64) -> bool {
    let now = now_ms();
    let prev = last.load(Ordering::Relaxed);
    if prev != 0 && now.saturating_sub(prev) < gap_ms {
        return false;
    }
    last.store(now.max(1), Ordering::Relaxed);
    true
}

/// Installs the tap on a thread of its own, once: called on the main thread by `host.keys.capture`,
/// which waits here until the thread has created the tap or failed to — a few milliseconds — and
/// answers what it said. A thread that could not create the tap ends, and the next capture starts
/// another.
pub fn install() -> Result<(), HostError> {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return Ok(()); // already installed; `host.keys.capture` re-arms this on every call
    }
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    let spawned = std::thread::Builder::new().name("key-tap".into()).spawn(move || tap_thread(ready_tx));
    let answer = match spawned {
        Err(e) => Err(HostError::with(&crate::errors::keys::TAP_THREAD, format!("could not start the event tap's thread: {e}"))),
        Ok(_) => ready_rx.recv().unwrap_or_else(|_| Err(HostError::of(&crate::errors::keys::TAP_ENDED))),
    };
    let (enabled, qos) = match answer {
        Ok(created) => created,
        Err(e) => {
            INSTALLED.store(false, Ordering::SeqCst);
            logging::line("macos", e.message());
            return Err(e);
        }
    };
    report_tap_list();
    install_report_observer();

    // Creation succeeding is not proof that keys will arrive: on the versions where Input
    // Monitoring is a separate switch, a tap can be created and simply never fire. The line
    // below is therefore the thing to look for FIRST in a log where nothing is captured —
    // present, with no "tap: captured" trace after it, means the permission, not the code.
    logging::line(
        "macos",
        &format!(
            "event tap installed on its own thread (enabled = {enabled}{}); if no key is ever \
             captured after this, check Input Monitoring",
            if qos == 0 { String::new() } else { format!("; its priority could not be raised ({qos})") }
        ),
    );
    if !enabled {
        logging::line(
            "macos",
            "the event tap reports itself DISABLED immediately after creation — keys will \
             not be captured; this is a permission or a secure-input session",
        );
    }
    if process_calls().is_none() {
        logging::line(
            "macos",
            "the Process Manager's GetFrontProcess is not in this macOS: the event tap cannot ask \
             which application is in front, and decides every key by the window the main thread \
             last told it of — while the main thread is busy, keys can be taken for a window the \
             user has left",
        );
    }
    Ok(())
}

/// The tap's thread: creates the tap and attaches it to this thread's run loop, with the watchdog
/// and the source that wakes it ([`install_watchdog`]); tells [`install`] whether the tap reports
/// itself enabled, and how raising the thread's priority went (0 when it took); then runs the run
/// loop for the rest of the process. A tap it cannot create is what it tells [`install`] instead,
/// and the thread ends.
///
/// User-interactive priority, the class the system gives the work a user is waiting for: every
/// key of the Mac waits for this thread's callback.
fn tap_thread(ready: std::sync::mpsc::SyncSender<Result<(bool, i32), HostError>>) {
    let qos = super::ocr::set_thread_qos(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE);
    let (port, run_loop, enabled) = match create_and_attach() {
        Ok(created) => created,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // The run loop retains the source, so letting ours go is right. The port is kept by
    // hand: `tap_enable` needs it from inside the callback, where there is nothing to own it.
    PORT.store(CFRetained::into_raw(port).as_ptr(), Ordering::SeqCst);
    note_on();
    install_watchdog(&run_loop);
    let _ = ready.send(Ok((enabled, qos)));
    loop {
        // Returns only when no source or timer is left in the default mode, and the watchdog's
        // timer is never removed; were it ever to return, a pause keeps this from spinning.
        CFRunLoop::run();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Creates the tap, puts its source on the calling thread's run loop — the tap's own — and
/// enables it: the port, the run loop, and whether the tap reports itself enabled. `Err` is what
/// `host.keys.capture` answers, and the line for the log. The tap's thread only: [`tap_thread`]
/// and [`recreate`].
///
/// Created with `Default` rather than `ListenOnly`, which is the whole point: a listening
/// tap sees the key and cannot stop it, and an overlay that speaks the control under Tab
/// while the plugin also acts on the Tab is worse than no overlay.
///
/// Head-inserted at the HID location so we are ahead of anything else that taps, and so
/// the suppression happens before the window server has decided who the key belongs to.
/// The consequence to know: keys this platform synthesises through `CGEvent::post` come
/// back through here as well, exactly as a Windows low-level hook sees `SendInput`.
fn create_and_attach() -> Result<(CFRetained<CFMachPort>, CFRetained<CFRunLoop>, bool), HostError> {
    let port = unsafe {
        CGEvent::tap_create(
            CGEventTapLocation::HIDEventTap,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            TAP_MASK,
            Some(tap_callback),
            std::ptr::null_mut(),
        )
    };
    let Some(port) = port else {
        // Almost always a permission, and the failure is total: without a tap nothing is
        // captured and nothing is suppressed, so every overlay looks dead while the
        // application underneath behaves perfectly normally. Accessibility is the one known to
        // refuse the tap; with it granted, Input Monitoring is what is left, and the Permissions
        // page counts it as needed from here on ([`keys_seen`]) — named as the next thing to try
        // rather than as a requirement, since a tester cannot see the dialog.
        let trusted = unsafe { objc2_application_services::AXIsProcessTrusted() };
        REFUSED_TRUSTED.store(trusted, Ordering::Relaxed);
        return Err(HostError::with(
            &crate::errors::keys::TAP_REFUSED,
            format!(
                "CGEventTapCreate refused (AXIsProcessTrusted = {trusted}) — grant this \
                 application Accessibility in System Settings > Privacy & Security, and Input \
                 Monitoring too if keys are still not captured after that; until then no key can \
                 be captured or suppressed"
            ),
        ));
    };
    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        return Err(HostError::of(&crate::errors::keys::TAP_SOURCE));
    };
    // The tap's own run loop, which runs nothing but the tap and its watchdog, in the default mode
    // only: the main loop's common modes were for AppKit, which switches it into modes of its own
    // while a menu is tracking or a modal panel is up, and this thread runs no AppKit.
    let Some(run_loop) = CFRunLoop::current() else {
        return Err(HostError::of(&crate::errors::keys::TAP_NO_RUN_LOOP));
    };
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopDefaultMode });
    CGEvent::tap_enable(&port, true);
    let enabled = CGEvent::tap_is_enabled(&port);
    REFUSED_TRUSTED.store(false, Ordering::Relaxed);
    Ok((port, run_loop, enabled))
}

/// What the key capture has shown in this run about keys reaching an overlay — the evidence by
/// which Input Monitoring counts as needed or not (`evidence::listen_needed`). A tap that has seen
/// a key since a shortcut it missed is no longer deaf. Any thread; a few relaxed loads.
pub(super) fn keys_seen() -> super::evidence::Keys {
    use super::evidence::Keys;
    if CALLED.load(Ordering::Relaxed) {
        return if FIRST_SUPPRESSION.load(Ordering::Relaxed) { Keys::Reached } else { Keys::Seen };
    }
    if DEAF.load(Ordering::Relaxed) {
        Keys::Deaf
    } else if REFUSED_TRUSTED.load(Ordering::Relaxed) {
        Keys::Refused
    } else {
        Keys::NotYet
    }
}

/// A shortcut of this application arrived (`hotkey.rs`, from Carbon's dispatcher on the main
/// thread). Its key passed the event tap's place on its way, so a tap that has been on for at
/// least [`DEAF_AFTER_MS`] and has seen no key-down at all is not being handed the keyboard —
/// unless secure input is on, which keeps every tap from the keys and leaves Carbon's shortcuts
/// working, or the tap is off now: then nothing is concluded. Off is what a tap the system gave up
/// on leaves: it switches the tap off and hands the key on, so the key becomes the shortcut without
/// the tap ever being called for it, and the tap was not deaf. Said once, the first time.
pub(super) fn note_shortcut() {
    if CALLED.load(Ordering::Relaxed) || DEAF.load(Ordering::Relaxed) {
        return;
    }
    let on_since = ON_SINCE_MS.load(Ordering::Relaxed);
    if on_since == 0 || now_ms() + 1 < on_since + DEAF_AFTER_MS {
        return;
    }
    // Switched off by the system, and the callback not yet told. Under `PORT_LOCK`: this is the
    // main thread, and the tap's thread may be creating the tap again.
    let enabled = {
        let _held = port_lock();
        let port = PORT.load(Ordering::Acquire);
        // SAFETY: the pointer came from a `CFRetained` whose retain only `recreate` releases, under
        // `PORT_LOCK`, after swapping it out of `PORT`.
        !port.is_null() && CGEvent::tap_is_enabled(unsafe { &*port })
    };
    if !enabled {
        return;
    }
    match secure_input() {
        Some(false) => {}
        // On, or not known: a tap that sees nothing then proves nothing.
        _ => return,
    }
    DEAF.store(true, Ordering::Relaxed);
    logging::line(
        "macos",
        "a shortcut of this application arrived while the event tap, on for over a second and not \
         under secure input, has seen no key at all — captured keys do not reach the overlay, so Input \
         Monitoring is counted as needed now (the Permissions page says so); grant it in System \
         Settings > Privacy & Security > Input Monitoring",
    );
}

/// Whether some process has secure keyboard entry on (Carbon's `IsSecureEventInputEnabled`), or
/// `None` where this macOS does not have the call. Looked up by name rather than linked, like the
/// older capture functions in `capture.rs`: a symbol a later macOS drops would otherwise stop the
/// application before it starts. Carbon is loaded already, for the shortcuts.
fn secure_input() -> Option<bool> {
    type IsSecureEventInputEnabledFn = unsafe extern "C" fn() -> u8;
    static F: OnceLock<Option<usize>> = OnceLock::new();
    let f = F.get_or_init(|| {
        // SAFETY: RTLD_DEFAULT with a NUL-terminated name; dlsym only reads.
        let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"IsSecureEventInputEnabled".as_ptr()) };
        (!p.is_null()).then_some(p as usize)
    });
    // SAFETY: the address of `Boolean IsSecureEventInputEnabled(void)`, whose C signature is the
    // type above; it takes nothing and answers a byte.
    f.map(|p| unsafe { core::mem::transmute::<usize, IsSecureEventInputEnabledFn>(p)() } != 0)
}

/// [`PORT_LOCK`], whatever a panic elsewhere left it as: it guards no data.
fn port_lock() -> MutexGuard<'static, ()> {
    PORT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Creates the tap again, because the one we have cannot be brought back: its mach port is
/// invalid, or it stays switched off after being re-enabled. The new tap is at the head of the
/// taps again; the captured set, the scope and the menu flag are statics it reads as the old one
/// did, and what the old one remembered of held keys is forgotten.
///
/// The tap's thread only (its watchdog), never inside the callback: the new tap's source goes on
/// this thread's run loop, and the old port is invalidated here, which removes its source from
/// the run loop and the tap from the system. Its retain is then released: [`PORT`] no longer holds
/// it, the callback and the watchdog run on this thread, and the main thread's one reader holds
/// [`PORT_LOCK`] as this does, so nothing can be holding the old pointer while it goes.
///
/// When the new tap reports itself off as well, [`STAYS_OFF`] is set: the watchdog then leaves
/// it alone until it reports itself on, or a wake, an unlock or this session becoming active
/// again asks for a fresh look. Writes no line, as nothing on this thread does: what it did is
/// left for the observer ([`TapNote`]).
fn recreate(cause: Cause) {
    match create_and_attach() {
        Ok((port, _run_loop, enabled)) => {
            {
                let _held = port_lock();
                let old = PORT.swap(CFRetained::into_raw(port).as_ptr(), Ordering::SeqCst);
                if let Some(old) = NonNull::new(old) {
                    // SAFETY: the pointer came from `CFRetained::into_raw`, and this is the one
                    // place its retain is given back — after the swap above took it out of `PORT`.
                    let old = unsafe { CFRetained::from_raw(old) };
                    old.invalidate();
                    drop(old);
                }
            }
            forget_held_keys();
            note_on();
            RECREATE_FAILURE_SAID.store(false, Ordering::Relaxed);
            let n = RECREATES.fetch_add(1, Ordering::Relaxed) + 1;
            STAYS_OFF.store(!enabled, Ordering::Relaxed);
            NOTES.push(TapNote::Recreated { cause, n, enabled });
        }
        Err(e) => {
            if !RECREATE_FAILURE_SAID.swap(true, Ordering::Relaxed) {
                NOTES.push(TapNote::NotRecreated { cause, error: e.message().to_string() });
            }
        }
    }
}

/// What the tap's thread found about the tap, left for the observer on the main run loop to write
/// ([`report_notes`]): the lines [`health_check`] and [`recreate`] wrote themselves while they
/// ran on the main thread. `who` is the [`RECHECK`] bits the check was asked for, 0 for the
/// watchdog's own round.
enum TapNote {
    /// Found switched off and re-enabled: re-enable `n` of the session.
    Reenabled { who: u32, n: u32 },
    /// Created again: re-creation `n`, and whether the new tap reports itself enabled.
    Recreated { cause: Cause, n: u32, enabled: bool },
    /// Could not be created again; said once until a re-creation works.
    NotRecreated { cause: Cause, error: String },
    /// Checked after a wake, an unlock or the session back, and valid and enabled.
    Fine { who: u32 },
    /// The question which application is in front, switched off by answers too slow (the last
    /// took `took_ms`), asked again after a wake, an unlock or the session back.
    FrontOn { who: u32, took_ms: u64 },
}

/// Why the tap was created again.
enum Cause {
    /// Its mach port was invalid: the tap was gone.
    PortInvalid { who: u32 },
    /// It stayed off when re-enabled (re-enable `n`).
    StayedOff { who: u32, n: u32 },
}

/// Who found it: the watchdog, or the check after the moments in `who` ([`recheck_words`]).
fn who_words(who: u32) -> String {
    if who == 0 {
        "the watchdog".to_string()
    } else {
        format!("the check after {}", recheck_words(who))
    }
}

fn cause_words(cause: &Cause) -> String {
    match *cause {
        Cause::PortInvalid { who } => format!("{} found its mach port invalid — the tap was gone", who_words(who)),
        Cause::StayedOff { who, n } => {
            format!("{} found it switched off, and it stayed off when re-enabled (#{n})", who_words(who))
        }
    }
}

/// Writes what the tap's thread left ([`TapNote`]). Called by the observer on every turn of the
/// main run loop: one lock and an empty swap when there is nothing to say.
fn report_notes() {
    let (notes, dropped) = NOTES.take();
    for note in notes {
        let line = match note {
            TapNote::Reenabled { who, n } => format!(
                "the event tap had been switched off and {} found it (re-enable #{n}) — keys were not \
                 being captured until now; the system switched it off, around a sleep or a lock, or for \
                 a callback it found too slow",
                who_words(who)
            ),
            TapNote::Recreated { cause, n, enabled } => format!(
                "the event tap was created again ({}) — #{n}, enabled = {enabled}; it is at the head \
                 of the taps again, and the captured keys carried over{}",
                cause_words(&cause),
                if enabled {
                    ""
                } else {
                    ". The new tap is off as well — a permission, or secure input: it is not \
                     created again until it reports itself on, or the Mac wakes, the screen \
                     is unlocked or this session becomes active again"
                }
            ),
            TapNote::NotRecreated { cause, error } => format!(
                "the event tap had to be created again ({}) and could not be: {error}. Keys are not \
                 captured; tried again at every watchdog check, and said again only once it works",
                cause_words(&cause)
            ),
            TapNote::Fine { who } => {
                format!("{}: the event tap was checked and is valid and enabled", recheck_words(who))
            }
            TapNote::FrontOn { who, took_ms } => {
                // A later switch-off is said again.
                FRONT_OFF_SAID.store(false, Ordering::Relaxed);
                format!(
                    "{}: the event tap asks the window server again which application is in front, \
                     for each key it would take for a capture pinned to a window — switched off \
                     after answers too slow, the last {took_ms} ms",
                    recheck_words(who)
                )
            }
        };
        logging::line("macos", &line);
    }
    if dropped > 0 {
        logging::line("macos", &format!("{dropped} more event-tap line(s) were not kept between two turns of the main run loop"));
    }
}

/// Checks the tap at once rather than within the watchdog's two seconds, and writes what was
/// found even when it is fine, because `bits` — a wake, an unlock, this session active again — is
/// a moment the tap is most likely to have been touched. Called by `system.rs` on the main thread,
/// which hears those moments for the whole backend: the bits are left for the tap's thread, and
/// its run loop is woken through the source that runs the check there ([`install_watchdog`]),
/// the cross-thread signal Core Foundation documents (`CFRunLoopSourceSignal`, then
/// `CFRunLoopWakeUp`). Nothing is asked before the tap exists: the first check after its
/// installation would otherwise report on a wake it never went through.
pub(super) fn recheck_soon(bits: u32) {
    if !INSTALLED.load(Ordering::SeqCst) {
        return;
    }
    RECHECK.fetch_or(bits, Ordering::Relaxed);
    let source = RECHECK_SOURCE.load(Ordering::Acquire);
    let run_loop = TAP_LOOP.load(Ordering::Acquire);
    if !source.is_null() && !run_loop.is_null() {
        // SAFETY: both are retained for the life of the process ([`TAP_LOOP`]).
        unsafe {
            (*source).signal();
            (*run_loop).wake_up();
        }
    }
}

/// The moments a set of [`RECHECK`] bits stands for, for the log.
fn recheck_words(bits: u32) -> String {
    let mut words = Vec::new();
    if bits & RECHECK_WAKE != 0 {
        words.push("the Mac woke from sleep");
    }
    if bits & RECHECK_UNLOCK != 0 {
        words.push("the screen was unlocked");
    }
    if bits & RECHECK_SESSION != 0 {
        words.push("this user's session became active again");
    }
    words.join(", ")
}

/// The key table, for the host's side and the callback's: only a panic while the lock was held
/// can poison it, and nothing here panics. Take the contents anyway rather than give up the key
/// set for the rest of the session.
fn key_state() -> MutexGuard<'static, KeyState> {
    match KEY_STATE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Replaces the whole captured set. Called on every focus move inside an overlay, so it
/// has to stay cheap.
pub fn set_captured_keys(keys: &[Captured]) {
    // Reuse the vector's capacity: the set is a handful of pairs, replaced on every focus
    // move, and the allocator is not something to visit that often on the pump thread.
    let mut held = key_state();
    held.set.clear();
    held.set.extend_from_slice(keys);
    let n = held.set.len();
    drop(held);
    logging::trace("macos", || format!("tap: {n} captured key(s)"));
    // While anything is captured, App Nap is kept away: a napped main thread hands these keys
    // over late, and a tap that answers too late is switched off (see `activity.rs`).
    super::activity::want(super::activity::KEYS, n > 0);
    // A captured chord on VoiceOver's modifier fails the same way a registered one does —
    // the tap never sees the press — and is explained the same way, once per chord. The
    // permissions page says the log warns for captures as well as registrations, and a
    // module's own capture can sit on the layer as easily as a hotkey can.
    for c in keys {
        super::hotkey::warn_if_voiceover_owns(c.vk, c.mask, "captured");
        warn_if_reserved(c.vk, c.mask);
    }
}

/// Replaces what each module set with `host.keys.scope` and `host.keys.menuOpen`. The record of
/// keys let through for a menu of a module whose entry the host dropped — disabled, reloaded,
/// removed — goes with the entry.
pub fn set_key_owners(owners: &[OwnerKeys]) {
    let mut held = key_state();
    let gone: Vec<u32> =
        held.owners.iter().map(|e| e.owner).filter(|o| !owners.iter().any(|e| e.owner == *o)).collect();
    held.owners.clear();
    held.owners.extend_from_slice(owners);
    drop(held);
    if !gone.is_empty() {
        menu_pass().retain(|p| !gone.contains(&p.0));
    }
    logging::trace("macos", || {
        let each: Vec<String> = owners
            .iter()
            .map(|e| match (e.scope, e.menu) {
                (0, m) => format!("module {} everywhere{}", e.owner, if m { ", menu open" } else { "" }),
                (w, m) => format!("module {} in window {w}{}", e.owner, if m { ", menu open" } else { "" }),
            })
            .collect();
        format!("tap: key scopes: {}", if each.is_empty() { "none".to_string() } else { each.join("; ") })
    });
}

/// Chords already said to be the system's, by (vk, mask). The set is replaced on every focus
/// move, so a per-call line would repeat for as long as the capture held.
static RESERVED_WARNED: Mutex<Vec<(u32, u8)>> = Mutex::new(Vec::new());

/// Says once, per chord, when a capture holds a combination macOS keeps for itself: a
/// Windows-written `"Ctrl+Q"` is Command+Q here, which `host.hotkey.register` refuses and
/// `host.keys.capture` does not. The words are `reserved_capture_line`'s, which is tested.
fn warn_if_reserved(vk: u32, mask: u8) {
    let Some(line) = crate::backend::reserved_capture_line(crate::backend::KeyOs::Macos, vk, mask)
    else {
        return;
    };
    let mut warned = match RESERVED_WARNED.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if warned.contains(&(vk, mask)) {
        return;
    }
    warned.push((vk, mask));
    drop(warned);
    logging::line("macos", &line);
}

/// The window the tap was last told is in front, for the scope's fallback
/// (`crate::backend::key_scope_window`).
pub fn foreground() -> isize {
    FOREGROUND.load(Ordering::Relaxed)
}

/// A module's captures were just pinned to `window`, the application's answer for the window in
/// front; 0 means everywhere. The value is a SNAPSHOT taken when the module asked, which is what
/// lets a menu opened by a control receive keys natively — the menu is a different window, so
/// the comparison stops matching.
///
/// This settles a disagreement, and that is the interesting half. The callback compares every
/// module's scope against [`FOREGROUND`], which is told to us by two notifications: the
/// frontmost application changing, and the focused window changing inside an application we
/// already observe. Neither is guaranteed when a window merely *opens* — the application was
/// already frontmost, and the observer for it may be a moment younger than the window. The
/// overlay gets there by another route (the re-check ladder resolves the window itself), so it
/// can pin a scope while the tap still believes something else is in front — and the callback
/// then declines to claim a single key. Tab does nothing, and switching out and back fixes it,
/// because that finally raises the notification.
///
/// So a pin that disagrees asks once, here, on the activation path where an accessibility
/// round trip is already the going rate, rather than in the callback where it would be paid
/// per keystroke. And it says so in the log: this was diagnosed from a tester's description,
/// not from evidence, and the line is what turns the next occurrence into evidence.
pub fn note_pinned(window: isize) {
    if window != 0 && FOREGROUND.load(Ordering::Relaxed) != window {
        let stale = FOREGROUND.load(Ordering::Relaxed);
        match super::ax::foreground_window_id() {
            Some(now) => {
                note_foreground(now);
                logging::line(
                    "macos",
                    &format!(
                        "key scope pinned to window {window} while the tap still thought \
                         {stale} was in front; asked again and it is {now}{}",
                        if now == window {
                            ""
                        } else {
                            " — which still does not match, so keys stay unclaimed"
                        }
                    ),
                );
            }
            // The disagreement stands unresolved rather than being resolved wrongly. Storing
            // a 0 here would replace a possibly-correct number with a definitely-wrong one,
            // on the path whose whole purpose is to correct the tap's idea of what is in
            // front.
            None => logging::line(
                "macos",
                &format!(
                    "key scope pinned to window {window} while the tap still thought {stale} \
                     was in front; the application did not answer, so the tap keeps {stale} \
                     until the pump gets a real answer"
                ),
            ),
        }
    }
    logging::trace("macos", || match window {
        0 => "tap: key scope is global".to_string(),
        w => format!("tap: key scope pinned to window {w}"),
    });
}

/// Is the tap still alive? Re-enables it, or creates it again, and leaves what it found for the
/// observer to write ([`TapNote`]).
///
/// The tap's thread only: the watchdog's timer and the source [`recheck_soon`] signals both run
/// it there ([`install_watchdog`]), so that a busy main thread never delays a re-enable, and a
/// re-creation is made on the run loop that holds the port. The in-callback re-enable covers the
/// case where the system tells us; this covers the case where it does not, and where nobody would
/// otherwise notice until the user reported that the overlay had gone quiet.
///
/// A wake, an unlock or this session becoming active again ([`recheck_soon`]) makes it check at
/// once, past its two seconds, and say what it found even when the tap is fine — the line a live
/// test of a sleep or a lock looks for.
fn health_check() {
    let port = PORT.load(Ordering::Relaxed);
    if port.is_null() {
        return;
    }
    // Every call checks: the timer that makes most of them is the watchdog's two seconds. A gap
    // asked of the clock here as well skipped a round whenever a firing came a millisecond early.
    let asked = RECHECK.swap(0, Ordering::Relaxed);
    if asked != 0 {
        // A wake, an unlock or the session back: whatever kept the tap off may be over, so a
        // tap that stays off is worth one more new one.
        STAYS_OFF.store(false, Ordering::Relaxed);
        // And whatever made the window server slow to say which application is in front.
        let off = FRONT_OFF_MS.swap(0, Ordering::Relaxed);
        if off != 0 {
            FRONT_SLOW_RUN.store(0, Ordering::Relaxed);
            NOTES.push(TapNote::FrontOn { who: asked, took_ms: off - 1 });
        }
    }
    // SAFETY: the pointer came from a `CFRetained` whose retain only `recreate` releases, on
    // this thread, after swapping it out of `PORT`.
    let port = unsafe { &*port };
    if !port.is_valid() {
        // Always: an invalid port is a tap that is gone, and nothing but a new one brings it
        // back. The old one's retain is released with it.
        recreate(Cause::PortInvalid { who: asked });
        return;
    }
    if !CGEvent::tap_is_enabled(port) {
        CGEvent::tap_enable(port, true);
        forget_held_keys();
        note_on();
        let n = REENABLES.fetch_add(1, Ordering::Relaxed) + 1;
        if !CGEvent::tap_is_enabled(port) {
            // Created again once; a new tap that is off as well is left alone (STAYS_OFF).
            if !STAYS_OFF.load(Ordering::Relaxed) {
                recreate(Cause::StayedOff { who: asked, n });
            }
            return;
        }
        STAYS_OFF.store(false, Ordering::Relaxed);
        NOTES.push(TapNote::Reenabled { who: asked, n });
        return;
    }
    STAYS_OFF.store(false, Ordering::Relaxed);
    if asked != 0 {
        NOTES.push(TapNote::Fine { who: asked });
    }
}

/// Writes what the callback counted when the system switched the tap off — see the callback
/// for why not there. Called by the observer on every turn of the main run loop: two relaxed
/// loads when there is nothing to say.
fn report_disabled() {
    let timeouts = DISABLED_BY_TIMEOUT.swap(0, Ordering::Relaxed);
    let user_input = DISABLED_BY_USER_INPUT.swap(0, Ordering::Relaxed);
    let total = REENABLES.load(Ordering::Relaxed);
    if timeouts > 0 {
        logging::line(
            "macos",
            &format!(
                "the system disabled the event tap {timeouts} time(s) because a callback took \
                 longer than it was willing to wait — re-enabled in the callback each time \
                 ({total} re-enable(s) this session)"
            ),
        );
    }
    if user_input > 0 {
        logging::line(
            "macos",
            &format!(
                "the system disabled the event tap {user_input} time(s) \
                 (kCGEventTapDisabledByUserInput) — re-enabled in the callback each time \
                 ({total} re-enable(s) this session)"
            ),
        );
    }
}

/// The watchdog, on the tap's run loop (the calling thread's): a timer that runs [`health_check`]
/// every two seconds, and a source that runs it at once when [`recheck_soon`] signals it from the
/// main thread. Both stay for the life of the process, retained here with the run loop, so that
/// the main thread can reach them through [`TAP_LOOP`] and [`RECHECK_SOURCE`].
///
/// A timer is the watchdog's clock, as the run-loop observer's turns were on the main loop: it
/// decides nothing but when to ask the system whether the tap is still on.
fn install_watchdog(run_loop: &CFRunLoop) {
    let mode = unsafe { kCFRunLoopDefaultMode };
    // SAFETY: a plain timer with no context; its callout is `watchdog_timer`.
    let timer = unsafe {
        CFRunLoopTimer::new(None, CFAbsoluteTimeGetCurrent() + 2.0, 2.0, 0, 0, Some(watchdog_timer), std::ptr::null_mut())
    };
    match timer {
        Some(timer) => {
            run_loop.add_timer(Some(&timer), mode);
            // Kept for the life of the process, as the run loop keeps it.
            let _ = CFRetained::into_raw(timer);
        }
        None => WATCHDOG_MISSING.store(true, Ordering::Relaxed),
    }
    let mut context = CFRunLoopSourceContext {
        version: 0,
        info: std::ptr::null_mut(),
        retain: None,
        release: None,
        copyDescription: None,
        equal: None,
        hash: None,
        schedule: None,
        cancel: None,
        perform: Some(recheck_perform),
    };
    // SAFETY: the context is copied by the call and holds no pointer that has to outlive it.
    match unsafe { CFRunLoopSource::new(None, 0, &mut context) } {
        Some(source) => {
            run_loop.add_source(Some(&source), mode);
            RECHECK_SOURCE.store(CFRetained::into_raw(source).as_ptr(), Ordering::Release);
        }
        None => WATCHDOG_MISSING.store(true, Ordering::Relaxed),
    }
    // A retain of the main thread's own, for ever: the caller's is the caller's.
    TAP_LOOP.store(CFRetained::into_raw(run_loop.retain()).as_ptr(), Ordering::Release);
}

unsafe extern "C-unwind" fn watchdog_timer(_timer: *mut CFRunLoopTimer, _info: *mut c_void) {
    health_check();
}

unsafe extern "C-unwind" fn recheck_perform(_info: *mut c_void) {
    health_check();
}

/// The observer that writes what the tap's thread and its callback left, on the main run loop,
/// whenever it is about to sleep, in common modes, so that a menu tracking or a modal panel does
/// not hold the lines back. Installed once, by [`install`] on the main thread.
///
/// An observer rather than a timer: it costs a few relaxed loads and two empty swaps per turn of
/// the loop, and runs as soon as the loop is idle. It stops while the main thread is busy; the tap
/// does not, and its lines wait.
fn install_report_observer() {
    let Some(main) = CFRunLoop::main() else {
        return;
    };
    let observer = unsafe {
        CFRunLoopObserver::new(
            None,
            CFRunLoopActivity::BeforeWaiting.0,
            true, // repeats
            0,
            Some(report_observer),
            std::ptr::null_mut(),
        )
    };
    match observer {
        Some(observer) => main.add_observer(Some(&observer), unsafe { kCFRunLoopCommonModes }),
        None => logging::line(
            "macos",
            "could not create the run-loop observer that writes the event tap's lines — the tap \
             works, and its switch-offs, re-creations and late keys go unsaid",
        ),
    }
    if WATCHDOG_MISSING.load(Ordering::Relaxed) {
        logging::line(
            "macos",
            "the event tap's watchdog could not be set up on its thread — a tap the system switches \
             off without a word stays off, and a wake or an unlock is not checked",
        );
    }
}

unsafe extern "C-unwind" fn report_observer(
    _observer: *mut CFRunLoopObserver,
    _activity: CFRunLoopActivity,
    _info: *mut c_void,
) {
    report_notes();
    report_disabled();
    report_first_suppression();
    report_gate_pass();
    report_front();
    report_traces();
    report_lateness();
}

/// Everything the callback is allowed to do, and no more: match a table, push, return — and,
/// for a key it would take for a capture pinned to a window, ask the window server which
/// application is in front ([`front_moved`]).
///
/// Anything slower than that gets the tap switched off by the system, and a switched-off tap
/// is indistinguishable, from the user's side, from an overlay that has stopped working; and every
/// key of the Mac waits for it. So it writes no line, not even the trace lines a tester switches
/// on: what a line needs is left in an atomic or a small queue, and the observer on the main run
/// loop writes it at the next turn of that loop ([`report_observer`]).
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    etype: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    let pass = event.as_ptr();

    // The system announces its own damage through the tap. Neither of these recovers on its
    // own, and both are silent everywhere else. Re-enabled here and now; the line in the log
    // is written by the main run loop's observer (`report_disabled`), not here: the tap is back on
    // the moment `tap_enable` returns, and every key from then on waits for this callback to
    // return — a file write in here, just after a wake with the disk still spinning up, is
    // exactly the delay that switches it off again.
    if etype == CGEventType::TapDisabledByTimeout || etype == CGEventType::TapDisabledByUserInput {
        if etype == CGEventType::TapDisabledByTimeout {
            DISABLED_BY_TIMEOUT.fetch_add(1, Ordering::Relaxed);
        } else {
            DISABLED_BY_USER_INPUT.fetch_add(1, Ordering::Relaxed);
        }
        REENABLES.fetch_add(1, Ordering::Relaxed);
        let port = PORT.load(Ordering::Relaxed);
        if !port.is_null() {
            // SAFETY: the pointer came from a `CFRetained` whose retain only `recreate`
            // releases, on this same thread, after swapping it out of `PORT`.
            CGEvent::tap_enable(unsafe { &*port }, true);
        }
        forget_held_keys();
        note_on();
        return pass;
    }

    // SAFETY: the tap owns the event for the duration of the call and we only read it.
    let ev: &CGEvent = unsafe { event.as_ref() };
    // A key this application posted to pass a key on to the program in front (`input::pass_on`):
    // through, before anything is recorded or matched. No capture takes it again, its modifier
    // events are no press, hold or tap of the user's, and it is not counted as let through. Only
    // its key-down ends a pending modifier tap, as the key would have: the program has had a key
    // between that modifier's press and its release. One field read.
    let mark = CGEvent::integer_value_field(Some(ev), CGEventField::EventSourceUserData);
    if mark == keys::PASS_ON_MARK {
        if etype == CGEventType::KeyDown {
            TAP_ARMED.store(0, Ordering::Relaxed);
        }
        return pass;
    }
    // A key `host.input.send` posted (`input::key_send`): taken, matched and counted as let through
    // as a typed key is, and left out of the count that ends a count of quick repeats.
    let sent = mark == keys::SEND_MARK;
    // Every key-down returned from here on goes towards the program: counted on its way.
    let through = || {
        if etype == CGEventType::KeyDown {
            LET_THROUGH.fetch_add(1, Ordering::Relaxed);
            if !sent {
                TAPS_THROUGH.fetch_add(1, Ordering::Relaxed);
            }
        }
        pass
    };
    let raw = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);
    if !(0..128).contains(&raw) {
        return through(); // not a keyboard event we can name; nothing to match against
    }
    let keycode = raw as u16;
    let flags = CGEvent::flags(Some(ev));

    if etype == CGEventType::FlagsChanged {
        // Never suppressed, whatever happens in there: the modifier has to keep working as
        // a modifier, so a tap is dispatched on the way past.
        modifier_changed(keycode, flags);
        return pass;
    }

    if etype == CGEventType::KeyUp {
        // What happens to a key-up is decided entirely by what happened to its key-down, and
        // never by re-matching — see SUPPRESSED_LO. Re-matching is what leaves an orphan:
        // release the modifier first and the mask no longer matches, close a menu between
        // the two halves and the gate opens for an up whose down the application did see.
        return if take_suppressed(keycode) { std::ptr::null_mut() } else { pass };
    }
    if etype != CGEventType::KeyDown {
        return pass;
    }
    // Every key-down, captured or not: the question is whether the THREAD delays keys, and a
    // key the overlay does not want waits in the same queue as one it does.
    note_lateness(ev, keycode);

    // Any ordinary key ends a pending modifier tap: "pressed and released with nothing in
    // between" is the whole definition, and this is the "in between".
    TAP_ARMED.store(0, Ordering::Relaxed);

    // The system's own word for the keyboard's auto-repeat of a key held down: asked of a key held
    // since a capture took it, just below, and handed to the host with a key that is taken. One
    // field read, no call out.
    let repeat = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventAutorepeat) != 0;
    // A key pressed, counted for a hand-over of the front still waiting (`front.rs`) — not one
    // this process posted through `host.input`, which the posting process's id in the event says
    // (the keys it posts to pass a key on have returned above). One field read and one add.
    let source = CGEvent::integer_value_field(Some(ev), CGEventField::EventSourceUnixProcessID);
    if !repeat && source != i64::from(std::process::id()) {
        PRESSED.fetch_add(1, Ordering::Relaxed);
        // And the evidence that the keyboard reaches the tap at all (`keys_seen`): one load once
        // it is noted.
        if !CALLED.load(Ordering::Relaxed) {
            CALLED.store(true, Ordering::Relaxed);
        }
    }
    // A key held down since a capture took it, repeating while one of our windows has the keyboard
    // now — the capture's own callback showed a page, say: swallowed as its key-down was and its
    // key-up will be, never let through to our window as a key that does not come up, and not
    // handed to the module either (`swallow_held_repeat`). After everything above is recorded, as
    // the decision below is.
    if crate::backend::swallow_held_repeat(repeat, held_back(keycode), super::own::has_keyboard()) {
        trace(TraceNote::HeldRepeat(keycode));
        return std::ptr::null_mut();
    }

    // The modifiers first: with Command held a letter is named by the layout's Command table.
    let mask = mask_of(flags);
    // A shortcut of this application's on this key: its key-down is stamped with the counts once
    // it is counted, for Carbon's delivery to claim (`tap_queue::Shortcuts`). Not a repeat, which
    // Carbon does not deliver. Two relaxed loads for every other key.
    let shortcut = !repeat && shortcut_on(keycode);
    // Every key-down let go from here on: counted, and stamped when it is a shortcut's.
    let let_go = || {
        let p = through();
        if shortcut {
            stamp_shortcut(keycode, mask);
        }
        p
    };
    let Some(vk) = keys::keycode_to_vk_for(keycode, mask) else {
        trace(TraceNote::NoVk(keycode));
        return let_go();
    };
    let front = FOREGROUND.load(Ordering::Relaxed);
    // The keypad's Enter is read as NumpadEnter or as Return by `decide`, which answers with it.
    let (vk, decided, pinned) = decide(vk, mask, front, true);
    let owner = match decided {
        None => {
            // Traced, because the alternative is a whole class of question nobody can answer.
            // "The overlay did not react to that key" has two causes that look identical from
            // outside: the event never reached this tap, or it reached it and no module had
            // claimed that exact combination. Silence here made them indistinguishable, and the
            // Control-Option experiment turned on precisely that difference.
            trace(TraceNote::Unclaimed(vk, mask));
            return let_go();
        }
        Some(Capture::Pass { why, .. }) => {
            // Said out loud, not traced, because this is the one event that explains a whole
            // class of report. "The overlay did not react to that key" has three causes that
            // sound identical from outside — the event never arrived, nothing had claimed it, or
            // something HAD claimed it and the tap let it through anyway — and only the third
            // means a key reached the application underneath and moved its focus. The tester met
            // exactly that and could only describe it as "VoiceOver said dimmed button".
            //
            // Rate-limited on the reason rather than on the key, because a reason that holds
            // holds for as long as a menu is open or a window is not frontmost, and one line per
            // keystroke would bury the log it is meant to explain. Written by the observer
            // (`report_gate_pass`), not here: one store in the reason's slot — a slot looked up,
            // not indexed, so a reason left off the list goes unsaid instead of panicking in here.
            if let Some(slot) = GATE_PASSED.get(tap_handoff::slot(why)) {
                slot.store(tap_handoff::pack(vk, mask), Ordering::Relaxed);
            }
            return let_go();
        }
        Some(Capture::Take { owner }) => owner,
    };
    // Taken for a window the main thread told the tap of — and the application the window server
    // names in front now is another one, which the main thread, busy, has not told it of yet: the
    // key goes to that application, as it would once the main thread had (`front_moved`).
    if pinned && front_moved(front) {
        FRONT_PASSED.store(tap_handoff::pack(vk, mask), Ordering::Relaxed);
        return let_go();
    }
    // With the key, whether it repeats (read above) and the press as it was: the window, the counts
    // of keys let through so far, and the keycode itself — the keypad's Enter is not Return,
    // though both are VK_RETURN to the host where no capture of NumpadEnter is in scope. Stamped
    // with the moment it is queued, under the queue's lock (`tap_queue::Handover::push`).
    queue::push_key(|queued| Taken {
        vk,
        mask,
        owner,
        repeat,
        pressed: Pressed { front, seq: let_through(), taps_seq: taps_through(), phys: Some(keycode), queued: Some(queued) },
    });
    note_suppressed(keycode);

    // The first suppression of a run, said out loud once.
    //
    // Everything else about this tap is diagnosed by lines that appear when something goes
    // WRONG. The commonest first-day failure on a Mac has no such line: Input Monitoring not
    // granted leaves a tap that was created successfully, reports `enabled`, and never fires.
    // Its symptom is the absence of a trace line the tester does not have switched on, which
    // means the log reads exactly the same as a working session. One positive statement is
    // worth more than any number of negative ones — after it, this costs a relaxed swap per
    // keystroke and no allocation. Said by the observer (`report_first_suppression`), not here.
    if !FIRST_SUPPRESSION.swap(true, Ordering::Relaxed) {
        FIRST_SUPPRESSED.store(tap_handoff::pack(vk, mask), Ordering::Relaxed);
    }

    trace(TraceNote::Captured(vk, mask, owner));
    std::ptr::null_mut()
}

/// A modifier went down or came up. Arms, disarms, or fires a `MASK_TAP`.
fn modifier_changed(keycode: u16, flags: CGEventFlags) {
    let Some(index) = MODIFIERS.iter().position(|m| m.keycode == keycode) else {
        // Caps Lock or Fn. Neither can be a tap, and neither is nothing: treat it as the
        // "something in between" that ends a pending one. Missing a tap is a nuisance; a
        // spurious one opens a menu bar the user did not ask for.
        TAP_ARMED.store(0, Ordering::Relaxed);
        return;
    };
    let modifier = &MODIFIERS[index];
    let bit = 1u32 << index;
    let was_down = MOD_DOWN.load(Ordering::Relaxed) & bit != 0;

    let went_down = if !flags.contains(modifier.flag) {
        // The flag is gone, so nothing in this group is held any more. Clearing the whole
        // group also resynchronises us if a press was missed while the tap was disabled.
        let group: u32 = MODIFIERS
            .iter()
            .enumerate()
            .filter(|(_, m)| m.vk == modifier.vk)
            .fold(0, |acc, (i, _)| acc | (1u32 << i));
        MOD_DOWN.fetch_and(!group, Ordering::Relaxed);
        false
    } else if was_down {
        // The flag survives because the other side of the same modifier is still held, so
        // this one was released.
        MOD_DOWN.fetch_and(!bit, Ordering::Relaxed);
        false
    } else {
        MOD_DOWN.fetch_or(bit, Ordering::Relaxed);
        true
    };

    if went_down {
        // Arm only from rest. A second modifier pressed on top means the user is building a
        // combination, not tapping, and holding one down while another arrives must not
        // hand the arm over.
        let armed = TAP_ARMED.load(Ordering::Relaxed);
        if armed == 0 {
            TAP_ARMED.store(keycode as u32 + 1, Ordering::Relaxed);
        } else if armed != keycode as u32 + 1 {
            TAP_ARMED.store(0, Ordering::Relaxed);
        }
        return;
    }

    // A release always ends the arm, whether or not it was this key's.
    if TAP_ARMED.swap(0, Ordering::Relaxed) != keycode as u32 + 1 {
        return;
    }
    let front = FOREGROUND.load(Ordering::Relaxed);
    match decide(modifier.vk, MASK_TAP, front, false) {
        // Not dispatched for a window the user has left while the main thread was busy, as a key
        // is not taken then (`front_moved`); the modifier itself went on as ever.
        (_, Some(Capture::Take { .. }), true) if front_moved(front) => {
            FRONT_PASSED.store(tap_handoff::pack(modifier.vk, MASK_TAP), Ordering::Relaxed);
        }
        (_, Some(Capture::Take { owner }), _) => {
            queue::push_key(|queued| Taken {
                vk: modifier.vk,
                mask: MASK_TAP,
                owner,
                repeat: false,
                pressed: Pressed { front, seq: let_through(), taps_seq: taps_through(), phys: None, queued: Some(queued) },
            });
            trace(TraceNote::ModifierTap(modifier.vk, owner));
        }
        (_, Some(Capture::Pass { why, .. }), _) => trace(TraceNote::ModifierTapPassed(why)),
        // Nothing claims the tap.
        (_, None, _) => {}
    }
}

/// Whether a key-down of `(vk, mask)` is taken, and for which module, by the modules' captures,
/// scopes and menu flags ([`capture_decision`]): the earliest capture whose module is scoped to
/// `front` or to everywhere, unless a native menu is open or a module's flag counts for `front`.
/// Matching is exact on the mask, never "at least these": `"Tab"` is mask 0 and must not swallow
/// Command-Tab, which is the same key with the Ctrl role's mask (2); a subset test would take
/// both.
///
/// With `menu_keys`, also files the keys let through for a menu in the record of every module
/// whose flag counts for `front` — or, for a native menu, of the module that would have taken
/// it — and an unmodified Return or Escape whenever a flag counts, captured or not: Escape is
/// captured by no overlay, and Return only while the focused control wants it, so a record kept
/// only for captured keys never held the one Escape that cancelled a menu. Each press is filed
/// once: a captured Return is both.
///
/// **What it costs, in the callback whose promptness decides whether the system leaves the tap
/// switched on.** The key table's lock, which the main thread holds only to copy a few pairs in
/// ([`KEY_STATE`]): a key that came while it did waits that microsecond rather than go through
/// untaken, as it went while the tap shared the main thread and tried the lock instead. One walk
/// of the captured set that stops at the first capture in scope, the owners looked up by a linear search of one entry per module that
/// ever set a scope or a flag, and the menu rule only for the modules whose flag is up — tens of
/// comparisons. Nothing allocates unless a key is filed for a menu. `front` is a number the tap
/// was told ([`note_foreground`]), never a question to another process: resolving the frontmost
/// window here would be a synchronous accessibility round trip against an application that may be
/// busy redrawing, and the keystroke that paid for it is the one the user is pressing. The native
/// menu's answer is a counter `watch` keeps, read only once a capture is in scope.
///
/// **This application's own window.** While one of its windows has the keyboard
/// ([`super::own::has_keyboard`], one atomic load), every captured combination is let through to
/// it, a modifier tap included, and nothing is filed for a menu: the key went to our window, not to
/// a menu in front. Asked at this decision, and before it only for a key-down that repeats a key
/// the tap swallowed (`crate::backend::swallow_held_repeat`) — both after everything the callback
/// records for every event: a modifier's change, the up of a down it swallowed, how late the key
/// came. So what it holds stays true while our window has the keys, for the moment another
/// application's window has them again.
///
/// **The keypad's Enter** arrives as the host's `NumpadEnter` ([`keys::keycode_to_vk_for`]) and is
/// decided as NumpadEnter while a capture of that combination is in scope for `front`, and as
/// Return everywhere else, as the Windows hook decides it (`crate::backend::keypad_enter_as`):
/// the key it was decided as comes back with the decision. A menu's record holds it as Return, as
/// the Windows hook's does, unless a capture of NumpadEnter let it past.
///
/// With the key and the decision comes whether the module that takes it has its captures pinned to
/// a window rather than everywhere (`host.keys.scope`): the decision then rests on `front` being the
/// window in front, which the callback asks the window server about ([`front_moved`]).
fn decide(vk: u32, mask: u8, front: isize, menu_keys: bool) -> (u32, Option<Capture>, bool) {
    let state = key_state();
    let own = super::own::has_keyboard();
    let keypad_enter = vk == crate::backend::VK_NUMPAD_ENTER;
    let vk = if keypad_enter { crate::backend::keypad_enter_as(&state.set, &state.owners, mask, front) } else { vk };
    let decided = capture_decision(&state.set, &state.owners, vk, mask, front, own, watch::native_menu_open);
    // A key taken never went through: with a menu flag counting here that is one a capture takes in
    // a menu (`menu = "taken"`), which the record must not say reached the program.
    let taken = matches!(decided, Some(Capture::Take { .. }));
    if menu_keys && !own && !taken {
        let mut noted = false;
        // The keypad's Enter ends a menu as Return does, and is recorded as Return.
        let menu_vk = if keypad_enter { crate::backend::VK_RETURN } else { vk };
        if is_menu_key(menu_vk) && mask == 0 && state.owners.iter().any(|e| e.menu) {
            for owner in menu_flag_owners(&state.set, &state.owners, front) {
                note_menu_pass(owner, menu_vk, mask);
                noted = true;
            }
        }
        // A Return filed above and let through by the same flag is the same press, filed once.
        match (noted, decided) {
            (false, Some(Capture::Pass { why: PassWhy::MenuFlag, .. })) => {
                for o in menu_flag_owners(&state.set, &state.owners, front) {
                    note_menu_pass(o, vk, mask);
                }
            }
            (false, Some(Capture::Pass { why: PassWhy::MenuMode, owner: Some(o) })) => note_menu_pass(o, vk, mask),
            _ => {}
        }
    }
    let pinned = matches!(decided, Some(Capture::Take { owner }) if crate::backend::scope_of(&state.owners, owner) != 0);
    (vk, decided, pinned)
}

/// The words a let-through says, for the log line.
fn pass_words(why: PassWhy) -> &'static str {
    match why {
        PassWhy::OutOfScope => "no module that claimed it is scoped to the window in front",
        PassWhy::MenuMode => "a native menu is open",
        PassWhy::MenuFlag => "a module scoped to the window in front says a plugin menu is open",
        PassWhy::OwnWindow => "one of our windows has the keyboard",
    }
}

/// The roles the host names, out of the event's own flags: Command held is the Ctrl role and
/// Control held the Win role (`keys::mask_of_cg_flags`, the one role table).
///
/// From the event rather than from any cached per-application state — that mistake on
/// Windows made every combination collapse to a mask of 0 and stayed hidden for a long time,
/// because unmodified keys like Tab went on working.
///
/// Only the four modifier flags: an arrow key on macOS also carries `MaskSecondaryFn` and
/// `MaskNumericPad`, so comparing whole `CGEventFlags` for equality would mean no arrow key
/// ever matched a capture of "Down".
fn mask_of(flags: CGEventFlags) -> u8 {
    keys::mask_of_cg_flags(flags.0)
}

/// Captured keys let through because a menu was open, as (module, vk, mask), waiting for each
/// module's overlay runtime to ask. Bounded per module (`crate::backend::file_menu_pass`). Filed by
/// the callback, inside [`decide`] with the key table held (that order, never the other); taken and
/// pruned by the main thread.
static MENU_PASS: Mutex<Vec<(u32, u32, u8)>> = Mutex::new(Vec::new());

fn menu_pass() -> MutexGuard<'static, Vec<(u32, u32, u8)>> {
    MENU_PASS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A captured key went to the application because a menu was open. Remembered for module
/// `owner`, so its overlay runtime can log which keys reached the menu, and whether the menu was
/// still there after an Escape went through.
fn note_menu_pass(owner: u32, vk: u32, mask: u8) {
    crate::backend::file_menu_pass(&mut menu_pass(), owner, vk, mask);
}

/// Return or Escape — the keys that end a menu, and the two the overlay runtime wants to hear
/// about whether or not an overlay had claimed them.
fn is_menu_key(vk: u32) -> bool {
    vk == 0x0D || vk == 0x1B
}

/// Hands over, and forgets, everything `note_menu_pass` filed for module `owner` since the last
/// call. The main thread's.
pub fn take_menu_pass_through(owner: u32) -> Vec<(u32, u8)> {
    crate::backend::take_menu_passes(&mut menu_pass(), owner)
}

/// Writes the captured keys the callback let through ([`GATE_PASSED`]): one line per reason, at
/// most every three seconds, naming the last such key since the observer last looked. See the
/// callback for why the line exists. Called by the observer on every turn of the run loop: four
/// relaxed loads when there is nothing to say.
///
/// A key let through to one of our own windows is the counterpart of the Windows hook being out
/// of the chain there, and is said in its words, under `keys`, with the window's title.
fn report_gate_pass() {
    for why in tap_handoff::REASONS {
        let slot = tap_handoff::slot(why);
        if GATE_PASSED[slot].load(Ordering::Relaxed) == 0 {
            continue;
        }
        let Some((vk, mask)) = tap_handoff::unpack(GATE_PASSED[slot].swap(0, Ordering::Relaxed)) else {
            continue;
        };
        if !due(&GATE_SAID_MS[slot], 3000) {
            continue;
        }
        if why == PassWhy::OwnWindow {
            logging::line(
                "keys",
                &format!(
                    "the tap lets keys through while one of our windows has the keyboard ('{}'): a \
                     captured key, vk {vk:#04x} mask {mask}, went to it",
                    super::own::key_title()
                ),
            );
            continue;
        }
        logging::line(
            "macos",
            &format!(
                "a key the overlay had claimed reached the application instead: vk {vk:#04x} \
                 mask {mask}, because {}",
                pass_words(why)
            ),
        );
    }
}

/// Says the first key the tap suppressed ([`FIRST_SUPPRESSED`]), once — see the callback for why.
/// Called by the observer on every turn of the run loop: one relaxed load after it is said.
fn report_first_suppression() {
    if FIRST_SUPPRESSED.load(Ordering::Relaxed) == 0 {
        return;
    }
    let Some((vk, mask)) = tap_handoff::unpack(FIRST_SUPPRESSED.swap(0, Ordering::Relaxed)) else {
        return;
    };
    logging::line(
        "macos",
        &format!(
            "the event tap suppressed its first key (vk {vk:#04x} mask {mask}) — the tap is live, with \
             whatever permissions it needs"
        ),
    );
}

/// Told to us by [`super::watch`] whenever the frontmost window can have changed, on the main
/// thread.
///
/// A number the callback can read rather than a question it has to ask ([`decide`]): `watch`
/// is already listening to the two notifications that can change the answer — the frontmost
/// application changing, and the focused window changing within one — so the answer is here
/// before the key arrives. Deliberately not resolved here: this is called from notification
/// handlers on the main thread, where the work is already being done for other reasons.
///
/// With it, the application it belongs to, for the tap's question which application is in front
/// ([`front_moved`]): its pid from the handle table (no call out), and that application as the
/// Process Manager names it — one call, made only when the window or its application changed,
/// and never with [`TOLD`] held, which the callback waits for. Written before the window, so that
/// a callback that reads the new window finds its application told; one that reads the window and
/// the application from two moments compares nothing (`tap_front::told_for`).
pub fn note_foreground(window: isize) {
    let pid = super::ax::pid_of_handle(window);
    let same = {
        let told = told();
        told.window == window && told.pid == pid
    };
    if !same {
        let psn = pid.and_then(psn_of);
        *told() = Told { window, pid, psn };
    }
    FOREGROUND.store(window, Ordering::Relaxed);
}

fn told() -> MutexGuard<'static, Told> {
    TOLD.lock().unwrap_or_else(|e| e.into_inner())
}

/// A process serial number, as the Process Manager's calls take one.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Psn {
    high: u32,
    low: u32,
}

/// The three Process Manager calls the tap's question which application is in front needs,
/// looked up by name rather than linked, like [`secure_input`]: all three have been deprecated
/// since macOS 10.9, and a symbol a later macOS drops would otherwise stop the application before
/// it starts. `None` when any is missing: the tap then decides every key as before it asked.
///
/// Why these: `GetFrontProcess` answers which application is in front as the system's process
/// manager knows it — the application the window server routes keys to, kept by the system and not
/// by this process, so it is current while this process's main thread is busy. AppKit's answer,
/// `NSWorkspace.frontmostApplication`, is not: it changes only as this process's main run loop
/// handles the workspace's notifications, the very thing a busy main thread does not do.
/// Accessibility's system-wide focused application asks the same system process over a longer road.
/// The Process Manager's header marks these calls thread-safe (since Mac OS X 10.3), and they take
/// and give plain values: nothing of this process's main thread is touched.
struct ProcessCalls {
    front: unsafe extern "C" fn(*mut Psn) -> i16,
    pid_of: unsafe extern "C" fn(*const Psn, *mut libc::pid_t) -> i32,
    of_pid: unsafe extern "C" fn(libc::pid_t, *mut Psn) -> i32,
}

fn process_calls() -> Option<&'static ProcessCalls> {
    static CALLS: OnceLock<Option<ProcessCalls>> = OnceLock::new();
    CALLS
        .get_or_init(|| {
            let find = |name: &std::ffi::CStr| {
                // SAFETY: RTLD_DEFAULT with a NUL-terminated name; dlsym only reads.
                let p = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
                (!p.is_null()).then_some(p)
            };
            let (front, pid_of, of_pid) = (find(c"GetFrontProcess")?, find(c"GetProcessPID")?, find(c"GetProcessForPID")?);
            // SAFETY: the addresses of `OSErr GetFrontProcess(ProcessSerialNumber *)`, `OSStatus
            // GetProcessPID(const ProcessSerialNumber *, pid_t *)` and `OSStatus
            // GetProcessForPID(pid_t, ProcessSerialNumber *)`, whose C signatures are the types
            // they become; `Psn` is `ProcessSerialNumber`'s layout, two 32-bit halves.
            unsafe {
                Some(ProcessCalls {
                    front: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*mut Psn) -> i16>(front),
                    pid_of: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(*const Psn, *mut libc::pid_t) -> i32>(pid_of),
                    of_pid: std::mem::transmute::<*mut c_void, unsafe extern "C" fn(libc::pid_t, *mut Psn) -> i32>(of_pid),
                })
            }
        })
        .as_ref()
}

/// The application in front as the Process Manager names it now (`tap_front::psn_word`); `None`
/// when the call is missing or answers an error. The tap's thread, for each key it would take for
/// a pinned capture: one call into the system, timed by [`front_moved`].
fn front_now() -> Option<u64> {
    let calls = process_calls()?;
    let mut psn = Psn::default();
    // SAFETY: a valid pointer to a `ProcessSerialNumber`-shaped value the call fills.
    let err = unsafe { (calls.front)(&mut psn) };
    (err == 0).then(|| tap_front::psn_word(psn.high, psn.low)).filter(|w| *w != 0)
}

/// Process `pid` as the Process Manager names it; `None` when it cannot say. The main thread.
fn psn_of(pid: i32) -> Option<u64> {
    let calls = process_calls()?;
    let mut psn = Psn::default();
    // SAFETY: a valid pointer the call fills.
    let err = unsafe { (calls.of_pid)(pid, &mut psn) };
    (err == 0).then(|| tap_front::psn_word(psn.high, psn.low)).filter(|w| *w != 0)
}

/// The pid of the application `word` names; `None` when it cannot say. The main thread, for the
/// log.
fn pid_of(word: u64) -> Option<i32> {
    let calls = process_calls()?;
    let (high, low) = tap_front::psn_halves(word);
    let psn = Psn { high, low };
    let mut pid: libc::pid_t = 0;
    // SAFETY: valid pointers, the first read and the second filled.
    let err = unsafe { (calls.pid_of)(&psn, &mut pid) };
    (err == 0 && pid > 0).then_some(pid)
}

/// Whether a key decided by window `front`, which a capture pinned to a window would take, is to go
/// to the application in front instead: the window server names another application in front than
/// the one the main thread named with `front` ([`note_foreground`]) — the user switched while the
/// main thread was busy, and it has not heard yet (`tap_front.rs`). The tap's thread, inside the
/// callback.
///
/// **What it costs**, per key a pinned capture would take and for no other key: the told
/// application, under a lock the main thread holds only to write it, and one call into the system
/// ([`front_now`]), timed — the first answer's time is written once, every answer of a millisecond
/// or more is counted for a line, and [`tap_front::TOO_SLOW_IN_A_ROW`] answers of
/// [`tap_front::TOO_SLOW`] or more in a row switch the question off until the Mac wakes, the
/// screen is unlocked or this session becomes active again ([`health_check`], [`report_front`]):
/// the keys are then decided by the window alone, as before the tap asked. Nothing is asked when
/// the told application is not known.
///
/// **What it does not see**: another window of the same application in front — one the user
/// switched to, or the told window closed — while the main thread is busy. The window server names
/// the active application, not the window its keys go to; that is the application's own key
/// window, which only the application can say (Accessibility, a question to the very process that
/// may be the busy one, waited for inside the callback), and the window list's order is not it
/// (a floating plug-in window stays above the window that has the keys). Such keys are taken for
/// the window the main thread last told, as before the tap asked (TODO.md).
fn front_moved(front: isize) -> bool {
    if FRONT_OFF_MS.load(Ordering::Relaxed) != 0 {
        return false;
    }
    let told = {
        let t = told();
        tap_front::told_for((t.window, t.psn), front)
    };
    let Some(told) = told else {
        return false;
    };
    let asked = Instant::now();
    let now = front_now();
    let took = asked.elapsed();
    let us = took.as_micros() as u64;
    let _ = FRONT_FIRST_US.compare_exchange(0, us + 1, Ordering::Relaxed, Ordering::Relaxed);
    if took >= tap_front::SLOW {
        FRONT_SLOW.fetch_add(1, Ordering::Relaxed);
        FRONT_SLOW_WORST_US.fetch_max(us, Ordering::Relaxed);
    }
    // The tap's thread alone runs this, so the run is read and written without a race.
    let (run, off) = tap_front::slow_run(FRONT_SLOW_RUN.load(Ordering::Relaxed), took);
    FRONT_SLOW_RUN.store(run, Ordering::Relaxed);
    if off {
        FRONT_OFF_MS.store(took.as_millis() as u64 + 1, Ordering::Relaxed);
    }
    match tap_front::front_check(Some(told), now) {
        FrontCheck::Other => {
            FRONT_PASSED_PSN.store(now.unwrap_or(0), Ordering::Relaxed);
            true
        }
        FrontCheck::Same | FrontCheck::Unknown => false,
    }
}

/// Writes what [`front_moved`] left: the first answer's time, once; the slow answers, at most every
/// five seconds; the question switched off, once; and a key it let through, at most every three
/// seconds. Called by the observer on every turn of the main run loop: a few relaxed loads when
/// there is nothing to say.
fn report_front() {
    let first = FRONT_FIRST_US.load(Ordering::Relaxed);
    if first != 0 && !FRONT_FIRST_SAID.swap(true, Ordering::Relaxed) {
        logging::line(
            "macos",
            &format!(
                "the event tap asked the window server which application is in front, for the first \
                 key it would take for a capture pinned to a window: the answer took {} µs \
                 (GetFrontProcess, asked for each such key)",
                first - 1
            ),
        );
    }
    if FRONT_SLOW.load(Ordering::Relaxed) != 0 && due(&FRONT_SLOW_SAID_MS, 5000) {
        let n = FRONT_SLOW.swap(0, Ordering::Relaxed);
        let worst = FRONT_SLOW_WORST_US.swap(0, Ordering::Relaxed);
        logging::line(
            "macos",
            &format!(
                "the event tap's question which application is in front took a millisecond or more \
                 {n} time(s), the worst {worst} µs — each key it takes for a pinned capture waits for \
                 the answer"
            ),
        );
    }
    let off = FRONT_OFF_MS.load(Ordering::Relaxed);
    if off != 0 && !FRONT_OFF_SAID.swap(true, Ordering::Relaxed) {
        logging::line(
            "macos",
            &format!(
                "the window server took {} ms to say which application is in front, inside the event \
                 tap, {} times in a row — the question is switched off until the Mac wakes, the \
                 screen is unlocked or this session becomes active again, and keys are decided by \
                 the window the main thread last told the tap of, as before it asked",
                off - 1,
                tap_front::TOO_SLOW_IN_A_ROW
            ),
        );
    }
    if FRONT_PASSED.load(Ordering::Relaxed) == 0 {
        return;
    }
    let Some((vk, mask)) = tap_handoff::unpack(FRONT_PASSED.swap(0, Ordering::Relaxed)) else {
        return;
    };
    if !due(&FRONT_PASSED_SAID_MS, 3000) {
        return;
    }
    let now = FRONT_PASSED_PSN.load(Ordering::Relaxed);
    let in_front = pid_of(now).map_or_else(|| "another application".to_string(), |p| format!("pid {p}"));
    let was = told().pid.map_or_else(|| "an application".to_string(), |p| format!("pid {p}"));
    logging::line(
        "macos",
        &format!(
            "a key the overlay had claimed went to the application in front instead: vk {vk:#04x} \
             mask {mask}, because the window server names {in_front} in front, and the main thread, \
             busy, had last told the event tap of a window of {was} — captured keys go through until \
             it hears of the change"
        ),
    );
}

/// The keys this application's shortcuts are registered on now, as (keycode, mask): from
/// `hotkey.rs`, on the main thread, after every registration and release. A key that stays keeps
/// the stamp of its last key-down (`tap_queue::Shortcuts`).
pub(super) fn set_shortcut_keys(keys: &[(u16, u8)]) {
    let codes = {
        let mut s = shortcuts();
        s.set_keys(keys);
        s.codes()
    };
    SHORTCUT_CODES[0].store(codes as u64, Ordering::Relaxed);
    SHORTCUT_CODES[1].store((codes >> 64) as u64, Ordering::Relaxed);
}

/// The stamp of the last key-down of `code` with `mask` the tap saw, taken: for Carbon's delivery
/// of that shortcut (`queue::push_hotkey`), which carries its counts and its moment. `None` when the
/// tap saw none since the last delivery — it was off, or not installed.
pub(super) fn claim_shortcut(code: u16, mask: u8) -> Option<Stamp> {
    shortcuts().claim(code, mask)
}

fn shortcuts() -> MutexGuard<'static, Shortcuts> {
    SHORTCUTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a shortcut is registered on `keycode` with any mask: two relaxed loads, the callback's
/// look before it takes the lock.
fn shortcut_on(keycode: u16) -> bool {
    let word = &SHORTCUT_CODES[usize::from(keycode >= 64)];
    word.load(Ordering::Relaxed) & (1u64 << (keycode % 64)) != 0
}

/// Stamps a key-down the callback let go, when a shortcut is registered on its key and mask: the
/// counts with its own key-down in them, and the moment. The tap's thread.
fn stamp_shortcut(keycode: u16, mask: u8) {
    let stamp = Stamp { seq: let_through(), taps_seq: taps_through(), at: Instant::now() };
    shortcuts().stamp(keycode, mask, stamp);
}

/// A trace line of the callback's, as values: the words are put together by the main thread
/// ([`report_traces`]), and only while tracing is on.
#[derive(Clone, Copy)]
enum TraceNote {
    /// A key held since a capture took it repeated into one of our windows, and was swallowed.
    HeldRepeat(u16),
    /// A keycode with no Win32 key.
    NoVk(u16),
    /// A key nothing had claimed.
    Unclaimed(u32, u8),
    /// A key taken for a module.
    Captured(u32, u8, u32),
    /// A modifier tap dispatched to a module.
    ModifierTap(u32, u32),
    /// A modifier tap let through, and why.
    ModifierTapPassed(PassWhy),
}

/// Leaves `note` for the observer while tracing is on: one relaxed load otherwise.
fn trace(note: TraceNote) {
    if logging::is_trace() {
        TRACES.push(note);
    }
}

/// Writes the callback's trace lines ([`TraceNote`]). Called by the observer on every turn of the
/// main run loop: one lock and an empty swap when there is nothing to say.
fn report_traces() {
    let (notes, dropped) = TRACES.take();
    for note in notes {
        logging::trace("macos", || match note {
            TraceNote::HeldRepeat(keycode) => format!("tap: keycode {keycode} repeats, held since a capture took it; swallowed"),
            TraceNote::NoVk(keycode) => format!("tap: keycode {keycode} has no Win32 equivalent"),
            TraceNote::Unclaimed(vk, mask) => format!("tap: saw vk {vk:#04x} mask {mask}, nothing had claimed it"),
            TraceNote::Captured(vk, mask, owner) => format!("tap: captured vk {vk:#04x} mask {mask} for module {owner}"),
            TraceNote::ModifierTap(vk, owner) => format!("tap: modifier tap vk {vk:#04x} for module {owner}"),
            TraceNote::ModifierTapPassed(why) => format!("tap: modifier tap not dispatched ({})", pass_words(why)),
        });
    }
    if dropped > 0 {
        logging::trace("macos", || format!("tap: {dropped} more trace line(s) came between two turns of the main run loop than are kept"));
    }
}

/// Forgets every key the tap believes is still held.
///
/// Called whenever the tap comes back from being switched off, because everything it
/// remembers from before is now a lie: the key-ups it was waiting for happened while it was
/// deaf. A remembered key-down that never gets its up leaves a bit set, and that bit is then
/// spent swallowing the release of some later, unrelated press — the application sees a key
/// go down and never come back up, which for a plugin means a stuck note or a stuck modifier.
///
/// The same reasoning covers the armed modifier tap: a modifier released while the tap was
/// off would otherwise fire a tap the user never made, the next time they let go of anything.
fn forget_held_keys() {
    SUPPRESSED_LO.store(0, Ordering::Relaxed);
    SUPPRESSED_HI.store(0, Ordering::Relaxed);
    TAP_ARMED.store(0, Ordering::Relaxed);
    MOD_DOWN.store(0, Ordering::Relaxed);
}

/// A bitmap over the 128 keycodes rather than a set, so remembering a swallowed key-down
/// costs one atomic operation and never allocates inside the callback.
fn note_suppressed(keycode: u16) {
    let bit = 1u64 << (keycode % 64);
    let bank = if keycode < 64 { &SUPPRESSED_LO } else { &SUPPRESSED_HI };
    bank.fetch_or(bit, Ordering::Relaxed);
}

fn take_suppressed(keycode: u16) -> bool {
    let bit = 1u64 << (keycode % 64);
    let bank = if keycode < 64 { &SUPPRESSED_LO } else { &SUPPRESSED_HI };
    bank.fetch_and(!bit, Ordering::Relaxed) & bit != 0
}

/// Whether the tap swallowed `keycode`'s key-down and has not seen its key-up yet: the key is held
/// down, for passing a key on to the program in front (`input::keyboard_now`), asked without
/// changing the record.
pub(super) fn held_back(keycode: u16) -> bool {
    let bit = 1u64 << (keycode % 64);
    let bank = if keycode < 64 { &SUPPRESSED_LO } else { &SUPPRESSED_HI };
    bank.load(Ordering::Relaxed) & bit != 0
}

/// Every event tap on this login session, once, at startup.
///
/// It exists because of a question this project could not answer for a day: an overlay did
/// not react to a key, and from outside there is no way to tell "the event never arrived"
/// from "it arrived and nobody had claimed it" from "somebody upstream took it". The first
/// two are now separated by a trace line in the callback. This separates the third: it names
/// every process holding a tap, where it sits, and whether it is enabled and suppressing.
///
/// It also settles, by proof rather than by inference, the specific case that prompted it.
/// Control-Option-arrow never reached this tap. If VoiceOver appears in this list at the HID
/// location, it is a tap and something is wrong with our placement; if it does not appear —
/// which is what all the evidence says, since VoiceOver's key handling lives in the window
/// server, above the whole Quartz tap layer — then no tap of ours could ever have won those
/// keys and that idea is closed for good.
///
/// Not behind tracing. It is a handful of lines, once, and it is the sort of thing a remote
/// log has to already contain, because the moment somebody thinks to ask for it is the moment
/// the session is over.
fn report_tap_list() {
    let mut count: u32 = 0;
    // SAFETY: null list with a valid count pointer is the documented way to ask how many
    // there are before allocating for them.
    let err = unsafe { CGGetEventTapList(0, std::ptr::null_mut(), &mut count) };
    if err != CGError::Success || count == 0 {
        logging::line("macos", &format!("event taps: none reported (CGGetEventTapList = {err:?})"));
        return;
    }
    let mut taps: Vec<CGEventTapInformation> = Vec::with_capacity(count as usize);
    let mut filled: u32 = 0;
    // SAFETY: the buffer has room for `count` entries and the call fills at most that many;
    // `filled` is set to how many it wrote, and nothing reads past it.
    let err = unsafe { CGGetEventTapList(count, taps.as_mut_ptr(), &mut filled) };
    if err != CGError::Success {
        logging::line("macos", &format!("event taps: could not be listed ({err:?})"));
        return;
    }
    // SAFETY: `filled` entries were written by the call above.
    unsafe { taps.set_len(filled.min(count) as usize) };

    let me = std::process::id() as i32;
    logging::line("macos", &format!("event taps on this session: {}", taps.len()));
    for t in &taps {
        let who = if t.tappingProcess == me { " (this application)" } else { "" };
        logging::line(
            "macos",
            &format!(
                "  pid {}{who} at {:?}, {:?}, enabled {}",
                t.tappingProcess, t.tapPoint, t.options, t.enabled
            ),
        );
    }
}
