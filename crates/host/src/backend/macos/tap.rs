//! Capturing and suppressing keys, through a `CGEventTap`.
//!
//! This is the fragile one. A tap that takes too long inside its callback is switched off
//! by the system and stays off, so the callback matches against a table and pushes; anything
//! else happens later, on the pump. A watchdog re-enables it and says so in the log, because
//! a tap that has quietly died looks exactly like an overlay that stopped working for no
//! reason.
//!
//! Matching is EXACT: the pressed modifier state must equal the mask, so a capture of "Tab"
//! does not swallow Command-Tab. Both halves of a suppressed key are swallowed — letting
//! the key-up through hands the application underneath an orphan release.
//!
//! The state lives in atomics and one small mutex rather than in a `thread_local`, even
//! though everything here runs on the main thread today. The captured set is written by the
//! pump and read by the callback, and if the tap ever has to move to a run loop of its own —
//! the escape hatch if the main loop turns out to be too slow to service it — a thread-local
//! would go silently empty rather than fail to compile.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::Instant;

use objc2_core_foundation::{
    kCFRunLoopCommonModes, CFMachPort, CFRetained, CFRunLoop, CFRunLoopActivity, CFRunLoopMode,
    CFRunLoopObserver,
};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};

use super::{ax, keys, queue, watch};
use crate::backend::{MASK_ALT, MASK_CTRL, MASK_SHIFT, MASK_TAP, MASK_WIN};
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
/// host knows it by.
///
/// Left and right collapse onto one `vk` because that is what `key_spec` produces for a tap
/// spec — `"Alt tap"` is `(0x12, MASK_TAP)`, with no way to say which side — and because a
/// module asking for a bare Option press does not care which thumb produced it.
///
/// The table is here rather than in `keys.rs` on purpose: `key_to_vk` deliberately has no
/// entry for a bare modifier, so that `"Alt"` cannot be used as an ordinary hotkey, and the
/// left/right collapse is a rule about taps rather than about key translation.
struct Modifier {
    keycode: u16,
    flag: CGEventFlags,
    vk: u32,
}

/// Keycodes are Carbon's `kVK_*`: Command 0x37, Shift 0x38, Option 0x3A, Control 0x3B, and
/// the right-hand ones 0x36, 0x3C, 0x3D, 0x3E. Caps Lock (0x39) and Fn (0x3F) are absent
/// because neither carries a mask bit the host can name.
const MODIFIERS: [Modifier; 8] = [
    Modifier { keycode: 0x38, flag: CGEventFlags::MaskShift, vk: 0x10 },
    Modifier { keycode: 0x3C, flag: CGEventFlags::MaskShift, vk: 0x10 },
    Modifier { keycode: 0x3B, flag: CGEventFlags::MaskControl, vk: 0x11 },
    Modifier { keycode: 0x3E, flag: CGEventFlags::MaskControl, vk: 0x11 },
    Modifier { keycode: 0x3A, flag: CGEventFlags::MaskAlternate, vk: 0x12 },
    Modifier { keycode: 0x3D, flag: CGEventFlags::MaskAlternate, vk: 0x12 },
    Modifier { keycode: 0x37, flag: CGEventFlags::MaskCommand, vk: 0x5B },
    Modifier { keycode: 0x36, flag: CGEventFlags::MaskCommand, vk: 0x5B },
];

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The tap's mach port, kept as a raw pointer so the callback and the watchdog can re-enable
/// it. The retain taken at creation is never released: the tap lives as long as the process,
/// and a dangling port here would be dereferenced from inside an OS callback.
static PORT: AtomicPtr<CFMachPort> = AtomicPtr::new(std::ptr::null_mut());

/// The (vk, mask) pairs the overlay currently wants. Replaced wholesale by the host.
static CAPTURED: Mutex<Vec<(u32, u8)>> = Mutex::new(Vec::new());

/// The window suppression is scoped to, 0 for everywhere. A snapshot the caller took.
static KEY_SCOPE: AtomicIsize = AtomicIsize::new(0);

/// A plugin-drawn menu is open, so captured navigation keys belong to it, not to us.
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

/// Keycode + 1 of the modifier held with nothing pressed since, 0 when none is.
static TAP_ARMED: AtomicU32 = AtomicU32::new(0);

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

static LAST_HEALTH_MS: AtomicU64 = AtomicU64::new(0);
static LAST_SLOW_SCOPE_MS: AtomicU64 = AtomicU64::new(0);
static LAST_LOCK_FAIL_MS: AtomicU64 = AtomicU64::new(0);

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

pub fn install() -> Result<(), String> {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return Ok(()); // already installed; `host.keys.capture` re-arms this on every call
    }

    // Created with `Default` rather than `ListenOnly`, which is the whole point: a listening
    // tap sees the key and cannot stop it, and an overlay that speaks the control under Tab
    // while the plugin also acts on the Tab is worse than no overlay.
    //
    // Head-inserted at the HID location so we are ahead of anything else that taps, and so
    // the suppression happens before the window server has decided who the key belongs to.
    // The consequence to know: keys this platform synthesises through `CGEvent::post` come
    // back through here as well, exactly as a Windows low-level hook sees `SendInput`.
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
        INSTALLED.store(false, Ordering::SeqCst);
        // Almost always a permission, and the failure is total: without a tap nothing is
        // captured and nothing is suppressed, so every overlay looks dead while the
        // application underneath behaves perfectly normally. Name both switches — which one
        // is required depends on the macOS version, and a tester cannot see the dialog.
        let trusted = unsafe { objc2_application_services::AXIsProcessTrusted() };
        let msg = format!(
            "CGEventTapCreate refused (AXIsProcessTrusted = {trusted}) — grant this \
             application Accessibility AND Input Monitoring in System Settings > Privacy & \
             Security; until then no key can be captured or suppressed"
        );
        logging::line("macos", &msg);
        return Err(msg);
    };

    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&port), 0) else {
        INSTALLED.store(false, Ordering::SeqCst);
        let msg = "CFMachPortCreateRunLoopSource failed for the event tap".to_string();
        logging::line("macos", &msg);
        return Err(msg);
    };

    // The MAIN run loop, which wxWidgets already runs: a tap only fires while a run loop is
    // running in a mode its source was added to, and this application has no other loop to
    // put it on. Common modes rather than the default one, because AppKit switches the main
    // loop into its own modes while a menu is tracking or a modal panel is up — the moments
    // an overlay most needs its keys.
    let Some(run_loop) = CFRunLoop::main().or_else(CFRunLoop::current) else {
        INSTALLED.store(false, Ordering::SeqCst);
        let msg = "no CFRunLoop to attach the event tap to".to_string();
        logging::line("macos", &msg);
        return Err(msg);
    };
    let mode = unsafe { kCFRunLoopCommonModes };
    run_loop.add_source(Some(&source), mode);

    CGEvent::tap_enable(&port, true);
    let enabled = CGEvent::tap_is_enabled(&port);

    // The run loop retains the source, so letting ours go is right. The port is kept by
    // hand: `tap_enable` needs it from inside the callback, where there is nothing to own it.
    PORT.store(CFRetained::into_raw(port).as_ptr(), Ordering::SeqCst);
    install_watchdog(&run_loop, mode);

    // Creation succeeding is not proof that keys will arrive: on the versions where Input
    // Monitoring is a separate switch, a tap can be created and simply never fire. The line
    // below is therefore the thing to look for FIRST in a log where nothing is captured —
    // present, with no "tap: captured" trace after it, means the permission, not the code.
    logging::line(
        "macos",
        &format!(
            "event tap installed on the main run loop (enabled = {enabled}); if no key is \
             ever captured after this, check Input Monitoring"
        ),
    );
    if !enabled {
        logging::line(
            "macos",
            "the event tap reports itself DISABLED immediately after creation — keys will \
             not be captured; this is a permission or a secure-input session",
        );
    }
    Ok(())
}

/// Replaces the whole captured set. Called on every focus move inside an overlay, so it
/// has to stay cheap.
pub fn set_captured_keys(keys: &[(u32, u8)]) {
    // Reuse the vector's capacity: the set is a handful of pairs, replaced on every focus
    // move, and the allocator is not something to visit that often on the pump thread.
    let mut held = match CAPTURED.lock() {
        Ok(g) => g,
        // Only a panic while the lock was held can poison it, and nothing here panics. Take
        // the contents anyway rather than give up the key set for the rest of the session.
        Err(poisoned) => poisoned.into_inner(),
    };
    held.clear();
    held.extend_from_slice(keys);
    let n = held.len();
    drop(held);
    logging::trace("macos", || format!("tap: {n} captured key(s)"));
}

/// Which window suppression applies to; 0 means everywhere. The value is a SNAPSHOT taken
/// when the caller asked, which is what lets a menu opened by a control receive keys
/// natively — the menu is a different window, so the comparison stops matching.
pub fn set_key_scope(window: isize) {
    KEY_SCOPE.store(window, Ordering::Relaxed);
    logging::trace("macos", || match window {
        0 => "tap: key scope is global".to_string(),
        w => format!("tap: key scope pinned to window {w}"),
    });
}

/// A plugin-drawn menu is open; let captured navigation keys through to it.
pub fn set_menu_open(open: bool) {
    MENU_OPEN.store(open, Ordering::Relaxed);
    logging::trace("macos", || format!("tap: plugin menu open = {open}"));
}

/// Is the tap still alive? Re-enables it and says so if not.
///
/// Rate-limited internally, so it is safe to call from anywhere that runs often. The
/// in-callback re-enable below covers the case where the system tells us; this covers the
/// case where it does not, and where nobody would otherwise notice until the user reported
/// that the overlay had gone quiet.
pub fn health_check() {
    let port = PORT.load(Ordering::Relaxed);
    if port.is_null() || !due(&LAST_HEALTH_MS, 2000) {
        return;
    }
    // SAFETY: the pointer came from a `CFRetained` whose retain is never released.
    let port = unsafe { &*port };
    if !CGEvent::tap_is_enabled(port) {
        CGEvent::tap_enable(port, true);
        let n = REENABLES.fetch_add(1, Ordering::Relaxed) + 1;
        logging::line(
            "macos",
            &format!(
                "the event tap had been switched off and the watchdog found it (re-enable \
                 #{n}) — keys were not being captured until now; something on this thread is \
                 taking too long"
            ),
        );
    }
}

/// A run-loop observer, fired every time the loop is about to sleep.
///
/// The watchdog needs a heartbeat and this module cannot add one to the pump. An observer on
/// the loop we are already on costs an atomic load per iteration and, unlike a timer, needs
/// no extra CoreFoundation feature. It stops when the loop stops — but so does the tap, so
/// there is nothing to watch at that point either.
fn install_watchdog(run_loop: &CFRunLoop, mode: Option<&CFRunLoopMode>) {
    let observer = unsafe {
        CFRunLoopObserver::new(
            None,
            CFRunLoopActivity::BeforeWaiting.0,
            true, // repeats
            0,
            Some(watchdog_observer),
            std::ptr::null_mut(),
        )
    };
    match observer {
        Some(observer) => run_loop.add_observer(Some(&observer), mode),
        None => logging::line(
            "macos",
            "could not create the run-loop observer for the tap watchdog — a tap the system \
             switches off will only recover when it tells us it did",
        ),
    }
}

unsafe extern "C-unwind" fn watchdog_observer(
    _observer: *mut CFRunLoopObserver,
    _activity: CFRunLoopActivity,
    _info: *mut c_void,
) {
    health_check();
}

/// Everything the callback is allowed to do, and no more: match a table, push, return.
///
/// Anything slower than that gets the tap switched off by the system, and a switched-off tap
/// is indistinguishable, from the user's side, from an overlay that has stopped working.
unsafe extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    etype: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    let pass = event.as_ptr();

    // The system announces its own damage through the tap. Neither of these recovers on its
    // own, and both are silent everywhere else.
    if etype == CGEventType::TapDisabledByTimeout || etype == CGEventType::TapDisabledByUserInput {
        let why = if etype == CGEventType::TapDisabledByTimeout {
            "a callback took longer than the system was willing to wait"
        } else {
            "kCGEventTapDisabledByUserInput"
        };
        let n = REENABLES.fetch_add(1, Ordering::Relaxed) + 1;
        let port = PORT.load(Ordering::Relaxed);
        if !port.is_null() {
            // SAFETY: the pointer came from a `CFRetained` that is never released.
            CGEvent::tap_enable(unsafe { &*port }, true);
        }
        logging::line(
            "macos",
            &format!("the system disabled the event tap ({why}) — re-enabled (#{n})"),
        );
        return pass;
    }

    // SAFETY: the tap owns the event for the duration of the call and we only read it.
    let ev: &CGEvent = unsafe { event.as_ref() };
    let raw = CGEvent::integer_value_field(Some(ev), CGEventField::KeyboardEventKeycode);
    if !(0..128).contains(&raw) {
        return pass; // not a keyboard event we can name; nothing to match against
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

    // Any ordinary key ends a pending modifier tap: "pressed and released with nothing in
    // between" is the whole definition, and this is the "in between".
    TAP_ARMED.store(0, Ordering::Relaxed);

    let Some(vk) = keys::keycode_to_vk(keycode) else {
        logging::trace("macos", || format!("tap: keycode {keycode} has no Win32 equivalent"));
        return pass;
    };
    let mask = mask_of(flags);
    if !captured(vk, mask) {
        return pass;
    }
    // Only now, with a match in hand, is it worth asking the questions that cost something.
    if let Some(why) = gate_closed() {
        logging::trace("macos", || {
            format!("tap: vk {vk:#04x} mask {mask} matched but passed through ({why})")
        });
        return pass;
    }
    queue::push_key(vk, mask);
    note_suppressed(keycode);
    logging::trace("macos", || format!("tap: captured vk {vk:#04x} mask {mask}"));
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
    if !captured(modifier.vk, MASK_TAP) {
        return;
    }
    if let Some(why) = gate_closed() {
        logging::trace("macos", || format!("tap: modifier tap not dispatched ({why})"));
        return;
    }
    queue::push_key(modifier.vk, MASK_TAP);
    logging::trace("macos", || format!("tap: modifier tap vk {:#04x}", modifier.vk));
}

/// Is this pair in the captured set? Exact equality on the mask, never "at least these".
///
/// `"Tab"` is mask 0 and must not swallow Command-Tab, which is the same key with a mask of
/// 8; a subset test would take both.
fn captured(vk: u32, mask: u8) -> bool {
    match CAPTURED.try_lock() {
        Ok(set) => set.iter().any(|&(v, m)| v == vk && m == mask),
        Err(TryLockError::Poisoned(p)) => p.into_inner().iter().any(|&(v, m)| v == vk && m == mask),
        Err(TryLockError::WouldBlock) => {
            // The pump was mid-replacement when the key arrived. It cannot happen while both
            // live on this thread, and it is a key silently not captured if it ever does.
            if due(&LAST_LOCK_FAIL_MS, 5000) {
                logging::line(
                    "macos",
                    "the captured-key set was locked when a key arrived — the key was let \
                     through; the tap and the pump are no longer on one thread",
                );
            }
            false
        }
    }
}

/// The four bits the host names, out of the event's own flags.
///
/// From the event rather than from any cached per-application state — that mistake on
/// Windows made every combination collapse to a mask of 0 and stayed hidden for a long time,
/// because unmodified keys like Tab went on working.
///
/// Only these four bits: an arrow key on macOS also carries `MaskSecondaryFn` and
/// `MaskNumericPad`, so comparing whole `CGEventFlags` for equality would mean no arrow key
/// ever matched a capture of "Down".
fn mask_of(flags: CGEventFlags) -> u8 {
    let mut mask = 0u8;
    if flags.contains(CGEventFlags::MaskShift) {
        mask |= MASK_SHIFT;
    }
    if flags.contains(CGEventFlags::MaskControl) {
        mask |= MASK_CTRL;
    }
    if flags.contains(CGEventFlags::MaskAlternate) {
        mask |= MASK_ALT;
    }
    if flags.contains(CGEventFlags::MaskCommand) {
        mask |= MASK_WIN;
    }
    mask
}

/// The three conditions that have to hold together before a matched key is taken: in scope,
/// no native menu, no plugin-drawn menu. `None` means take it, `Some(reason)` means let it
/// past — and when it goes past, nothing is queued either.
///
/// Ordered cheapest first. The scoped-window comparison is last because it is the only one
/// that can reach into another process.
fn gate_closed() -> Option<&'static str> {
    if MENU_OPEN.load(Ordering::Relaxed) {
        return Some("a plugin menu is open");
    }
    if watch::native_menu_open() {
        return Some("a native menu is open");
    }
    let scope = KEY_SCOPE.load(Ordering::Relaxed);
    if scope == 0 {
        return None; // global: nothing to compare, and nothing to pay for
    }
    let started = Instant::now();
    let foreground = ax::foreground_window_id();
    let took = started.elapsed();
    if took.as_millis() >= 5 && due(&LAST_SLOW_SCOPE_MS, 5000) {
        // This runs inside the tap callback. If it is slow the system will eventually take
        // the tap away, and the log should already have said why by then.
        logging::line(
            "macos",
            &format!(
                "resolving the foreground window inside the key tap took {} ms — the tap is \
                 at risk of being disabled for being slow",
                took.as_millis()
            ),
        );
    }
    if foreground == scope {
        None
    } else {
        Some("the scoped window is not frontmost")
    }
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
