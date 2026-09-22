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
    CGEvent, CGEventField, CGEventFlags, CGEventMask, CGEventTapInformation, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType, CGError,
    CGGetEventTapList,
};

use super::{key_age, keys, queue, watch};
use crate::backend::MASK_TAP;
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

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The tap's mach port, kept as a raw pointer so the callback and the watchdog can re-enable
/// it. The retain taken at creation is never released: the tap lives as long as the process,
/// and a dangling port here would be dereferenced from inside an OS callback.
static PORT: AtomicPtr<CFMachPort> = AtomicPtr::new(std::ptr::null_mut());

/// The (vk, mask) pairs the overlay currently wants. Replaced wholesale by the host.
static CAPTURED: Mutex<Vec<(u32, u8)>> = Mutex::new(Vec::new());

/// The window suppression is scoped to, 0 for everywhere. A snapshot the caller took.
static KEY_SCOPE: AtomicIsize = AtomicIsize::new(0);

/// The frontmost window as `watch` last saw it. See `gate_closed`.
static FOREGROUND: AtomicIsize = AtomicIsize::new(0);

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
static LAST_LOCK_FAIL_MS: AtomicU64 = AtomicU64::new(0);

/// Key presses that reached this callback late, and how late the worst of them was.
///
/// The question behind it: does a busy main thread delay keys? The tap runs on the main run
/// loop, and the pump that also runs there stalls for about a second at a time against an
/// application that does not answer. The fifth session's two switch-offs were the probe's own
/// long hotkey callback; during the ordinary one-second stalls the system did not switch the tap
/// off, and nothing could say whether keys arrived late meanwhile. Moving the tap to its own
/// thread is a real rebuild (the pump's queues are thread-local), so it waits for this answer.
///
/// Recorded in the callback — three relaxed atomic writes and a keycode lookup, nothing that
/// locks, allocates or logs — and written by the run-loop observer
/// once the loop is idle again, because a log line from inside the callback is exactly the kind
/// of slowness that gets a tap switched off.
static LATE_KEYS: AtomicU32 = AtomicU32::new(0);
static LATE_WORST_MS: AtomicU64 = AtomicU64::new(0);
static LATE_LAST_VK: AtomicU32 = AtomicU32::new(0);
static LAST_LATE_REPORT_MS: AtomicU64 = AtomicU64::new(0);

/// A key press this much older than the callback is reported. Well above a normal run-loop
/// turn and well below the ~1 s at which the system gives up on a tap.
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

/// Writes what `note_lateness` gathered, at most once a second, from the idle run loop.
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
             pressed (last vk {vk:#04x}) — the main thread was busy; the pump line nearby says \
             with what"
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

    report_tap_list();

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
    // A captured chord on VoiceOver's modifier fails the same way a registered one does —
    // the tap never sees the press — and is explained the same way, once per chord. The
    // permissions page says the log warns for captures as well as registrations, and a
    // module's own capture can sit on the layer as easily as a hotkey can.
    for &(vk, mask) in keys {
        super::hotkey::warn_if_voiceover_owns(vk, mask, "captured");
        warn_if_reserved(vk, mask);
    }
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

/// Which window suppression applies to; 0 means everywhere. The value is a SNAPSHOT taken
/// when the caller asked, which is what lets a menu opened by a control receive keys
/// natively — the menu is a different window, so the comparison stops matching.
///
/// It also settles a disagreement, and that is the interesting half. The gate compares this
/// against [`FOREGROUND`], which is told to us by two notifications: the frontmost
/// application changing, and the focused window changing inside an application we already
/// observe. Neither is guaranteed when a window merely *opens* — the application was already
/// frontmost, and the observer for it may be a moment younger than the window. The overlay
/// gets there by another route (the re-check ladder resolves the window itself), so it can
/// pin a scope while the tap still believes something else is in front — and the gate then
/// declines to claim a single key. Tab does nothing, and switching out and back fixes it,
/// because that finally raises the notification.
///
/// So a pin that disagrees asks once, here, on the activation path where an accessibility
/// round trip is already the going rate, rather than in the callback where it would be paid
/// per keystroke. And it says so in the log: this was diagnosed from a tester's description,
/// not from evidence, and the line is what turns the next occurrence into evidence.
pub fn set_key_scope(window: isize) {
    KEY_SCOPE.store(window, Ordering::Relaxed);
    if window != 0 && FOREGROUND.load(Ordering::Relaxed) != window {
        let stale = FOREGROUND.load(Ordering::Relaxed);
        match super::ax::foreground_window_id() {
            Some(now) => {
                FOREGROUND.store(now, Ordering::Relaxed);
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
        forget_held_keys();
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
    report_lateness();
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
        forget_held_keys();
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
    // Every key-down, captured or not: the question is whether the THREAD delays keys, and a
    // key the overlay does not want waits in the same queue as one it does.
    note_lateness(ev, keycode);

    // Any ordinary key ends a pending modifier tap: "pressed and released with nothing in
    // between" is the whole definition, and this is the "in between".
    TAP_ARMED.store(0, Ordering::Relaxed);

    // The modifiers first: with Command held a letter is named by the layout's Command table.
    let mask = mask_of(flags);
    let Some(vk) = keys::keycode_to_vk_for(keycode, mask) else {
        logging::trace("macos", || format!("tap: keycode {keycode} has no Win32 equivalent"));
        return pass;
    };
    // The two keys that END a menu, remembered whenever the runtime says a plugin menu is
    // open — captured or not. Escape is captured by no overlay, and Return only while the
    // focused control wants it, so a record kept only for captured keys never held the one
    // Escape that cancelled a menu, and the hold ran its full course after it.
    if is_menu_key(vk) && mask == 0 && MENU_OPEN.load(Ordering::Relaxed) {
        note_menu_pass(vk, mask, "a plugin menu is open");
        if !captured(vk, mask) {
            return pass;
        }
    }
    if !captured(vk, mask) {
        // Traced, because the alternative is a whole class of question nobody can answer.
        // "The overlay did not react to that key" has two causes that look identical from
        // outside: the event never reached this tap, or it reached it and no module had
        // claimed that exact combination. Silence here made them indistinguishable, and the
        // Control-Option experiment turned on precisely that difference.
        logging::trace("macos", || {
            format!("tap: saw vk {vk:#04x} mask {mask}, nothing had claimed it")
        });
        return pass;
    }
    // Only now, with a match in hand, is it worth asking the questions that cost something.
    if let Some(why) = gate_closed() {
        // Said out loud, not traced, because this is the one event that explains a whole
        // class of report. "The overlay did not react to that key" has three causes that
        // sound identical from outside — the event never arrived, nothing had claimed it, or
        // something HAD claimed it and this gate let it through anyway — and only the third
        // means a key reached the application underneath and moved its focus. The tester met
        // exactly that and could only describe it as "VoiceOver said dimmed button".
        //
        // Rate-limited on the reason rather than on the key, because a gate that is closed
        // stays closed for as long as a menu is open or a window is not frontmost, and one
        // line per keystroke would bury the log it is meant to explain.
        report_gate_pass(vk, mask, why);
        note_menu_pass(vk, mask, why);
        return pass;
    }
    queue::push_key(vk, mask);
    note_suppressed(keycode);

    // The first suppression of a run, said out loud once.
    //
    // Everything else about this tap is diagnosed by lines that appear when something goes
    // WRONG. The commonest first-day failure on a Mac has no such line: Input Monitoring not
    // granted leaves a tap that was created successfully, reports `enabled`, and never fires.
    // Its symptom is the absence of a trace line the tester does not have switched on, which
    // means the log reads exactly the same as a working session. One positive statement is
    // worth more than any number of negative ones — after it, this costs a relaxed swap per
    // keystroke and no allocation.
    if !FIRST_SUPPRESSION.swap(true, Ordering::Relaxed) {
        logging::line(
            "macos",
            &format!(
 "the event tap suppressed its first key (vk {vk:#04x} mask {mask}) — the tap is live and Input Monitoring is granted"
            ),
        );
    }

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
/// `"Tab"` is mask 0 and must not swallow Command-Tab, which is the same key with the Ctrl
/// role's mask (2); a subset test would take both.
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

thread_local! {
    // Captured keys let through because a menu was open, waiting for the overlay runtime
    // to ask. Bounded: nobody presses more than a few keys inside a menu between two ticks
    // of the watch, and a reader that never comes must not grow this for ever.
    static MENU_PASS: std::cell::RefCell<Vec<(u32, u8)>> = const { std::cell::RefCell::new(Vec::new()) };
}
const MENU_PASS_MAX: usize = 32;

/// A captured key went to the application because a menu was open. Remembered so the
/// overlay runtime can learn that Return or Escape reached the menu — where nothing can see
/// the menu itself, that is the best available word that it is closing, and the alternative
/// was a stopwatch running its full course while Tab stayed dead.
fn note_menu_pass(vk: u32, mask: u8, why: &'static str) {
    if !why.contains("menu") {
        return;
    }
    MENU_PASS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() < MENU_PASS_MAX {
            m.push((vk, mask));
        }
    });
}

/// Return or Escape — the keys that end a menu, and the two the runtime's menu watch
/// wants to hear about whether or not an overlay had claimed them.
fn is_menu_key(vk: u32) -> bool {
    vk == 0x0D || vk == 0x1B
}

/// Hands over, and forgets, everything `note_menu_pass` saw since the last call. Main
/// thread only, like the tap itself.
pub fn take_menu_pass_through() -> Vec<(u32, u8)> {
    MENU_PASS.with(|m| std::mem::take(&mut *m.borrow_mut()))
}

/// One line per reason, at most every few seconds. See the call site for why it exists.
fn report_gate_pass(vk: u32, mask: u8, why: &'static str) {
    const QUIET: std::time::Duration = std::time::Duration::from_secs(3);
    thread_local! {
        static LAST: std::cell::RefCell<Option<(&'static str, Instant)>> =
            const { std::cell::RefCell::new(None) };
    }
    let say = LAST.with(|last| {
        let mut last = last.borrow_mut();
        match *last {
            Some((reason, at)) if reason == why && at.elapsed() < QUIET => false,
            _ => {
                *last = Some((why, Instant::now()));
                true
            }
        }
    });
    if say {
        logging::line(
            "macos",
            &format!(
                "a key the overlay had claimed reached the application instead: vk {vk:#04x} \
                 mask {mask}, because {why}"
            ),
        );
    }
}

/// The three conditions that have to hold together before a matched key is taken: in scope,
/// no native menu, no plugin-drawn menu. `None` means take it, `Some(reason)` means let it
/// past — and when it goes past, nothing is queued either.
///
/// Every one of them is an atomic read. Nothing in here reaches into another process, and
/// that is a requirement rather than an optimisation — see the scope comparison.
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
    // Read, not asked. Resolving the frontmost window here would mean a synchronous
    // cross-process accessibility round trip inside the callback whose promptness decides
    // whether the system leaves this tap switched on at all — and against an application
    // that is busy redrawing, that round trip can take the whole messaging timeout. The
    // keystroke that pays for it is the one the user is pressing.
    //
    // `watch` is already listening to the two notifications that can change the answer —
    // the frontmost application changing, and the focused window changing within one — so
    // the answer is here before the key arrives.
    if FOREGROUND.load(Ordering::Relaxed) == scope {
        None
    } else {
        Some("the scoped window is not frontmost")
    }
}

/// Told to us by [`super::watch`] whenever the frontmost window can have changed.
///
/// A number the callback can read rather than a question it has to ask. Deliberately not
/// resolved here: this is called from notification handlers on the main thread, where the
/// work is already being done for other reasons.
pub fn note_foreground(window: isize) {
    FOREGROUND.store(window, Ordering::Relaxed);
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
