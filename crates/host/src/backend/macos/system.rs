//! The machine and the session: the Mac going to sleep and waking, the screen locked and
//! unlocked, this user's session switched away from and back to, the displays asleep, awake or
//! rearranged — and applications quitting, which is what lets the pid-keyed tables forget them.
//!
//! Heard here, queued with the host's own queue (`crate::system_events::push`), and handed to
//! the host from the pump as one batch of `SystemEvent`s ([`drain`], first thing in
//! `queue::drain`), so the `[system]` line, the epochs, the focus round and the rest happen on
//! the event loop, never inside a notification (see `system_events.rs` for the host's part).
//! What the backend itself owes the batch is done just BEFORE the host hears it ([`own_part`],
//! and the front read again once per batch), as the Windows backend resets desktop duplication
//! before it hands its batch over: the host's reaction — the window in front reported again, the
//! focus round — captures and asks about the front at once, and must find the probe brought
//! forward and the application in front already known.
//!
//! - a **wake**, an **unlock**, this session **back at the console**: the event tap is checked
//!   at once rather than within the watchdog's two seconds (`tap::recheck_soon`) — the three
//!   moments it is most likely to have been touched;
//! - a **wake**, an **unlock**, this session **back**, the displays **waking**, a **display
//!   change** (`SystemEvent::rechecks_capture`): a ScreenCaptureKit back-off's next probe comes
//!   at once (`capture::clear_sck_backoff`, once per step of the back-off) — the slow capture
//!   that started it is the kind these moments produce;
//! - each of the moments after which the window in front may have changed underneath us
//!   (`SystemEvent::front_may_have_changed`): the frontmost application is read again. One that
//!   came to the front unannounced is taken over — the menu state, the tap's window, its
//!   observer — and announced as an activation only when the host does not report the window in
//!   front itself after this batch (`SystemEvent::reports_front`: a wake, an unlock, the session
//!   back); announced then as well, it was reported twice. The focus round itself is the host's.
//!
//! After a display change the host writes the `[env] display` lines again when they differ from
//! the last ones written, from [`display_lines`] (`MacBackend::display_environment`) — a display
//! that flaps writes them once per real change of the arrangement, not once per flap.
//!
//! **Where each comes from.** `NSWorkspace`'s notification centre for sleep, wake, the screens
//! and the session (delivered on the main queue, which is the pump's thread); the distributed
//! `com.apple.screenIsLocked` and `com.apple.screenIsUnlocked` for the lock — there is no public
//! notification for it, and every utility that cares listens for these — through the
//! CoreFoundation centre, which alone can ask to have them delivered while this accessory
//! application is in the background (as `layout.rs` does); and
//! `CGDisplayRegisterReconfigurationCallback` for the displays, rather than
//! `NSApplicationDidChangeScreenParametersNotification`, because that one needs a running
//! `NSApplication` and a headless run has none. Every subscription lasts as long as the process.
//!
//! The event tap's own subscription to the wake, the unlock and the session (25713e5) moved
//! here, so each notification is registered once.

use core::ffi::c_void;
use core::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use block2::RcBlock;
use objc2_app_kit::{
    NSRunningApplication, NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceDidTerminateApplicationNotification,
    NSWorkspaceDidWakeNotification, NSWorkspaceScreensDidSleepNotification, NSWorkspaceScreensDidWakeNotification,
    NSWorkspaceSessionDidBecomeActiveNotification, NSWorkspaceSessionDidResignActiveNotification,
    NSWorkspaceWillSleepNotification,
};
use objc2_core_foundation::{CFDictionary, CFNotificationCenter, CFNotificationSuspensionBehavior, CFString};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayChangeSummaryFlags, CGDisplayCopyDisplayMode, CGDisplayMode,
    CGDisplayRegisterReconfigurationCallback, CGError, CGGetActiveDisplayList, CGMainDisplayID,
};
use objc2_foundation::{NSNotification, NSNotificationName, NSOperationQueue, NSString};

use crate::backend::HostEvents;
use crate::system_events::{push, SystemEvent};

static STARTED: AtomicBool = AtomicBool::new(false);

// The events themselves go to the host's queue (`crate::system_events::push`): a `Mutex`, so
// the display callback's thread — which is not documented — is as good as the main thread; each
// stamped with the wall-clock time it was heard, since the notice of a sleep may only be drained
// after the wake; and a reconfiguration that calls back once per display (a dock or an undock
// touches several) folded into one event, as long as nothing else came between them.

/// Applications that quit since the last drain, by pid.
static TERMINATED: Mutex<Vec<i32>> = Mutex::new(Vec::new());
/// More applications quitting than a pump tick could ever collect — generous, because an
/// application not forgotten keeps its name for a later process given its pid. Past it the
/// rest are not forgotten, and the log says so.
const QUIT_MAX: usize = 1024;
static QUIT_OVERFLOW: AtomicBool = AtomicBool::new(false);

/// The observer identities for the two distributed notifications: any stable address will do.
static LOCK_OBSERVER: u8 = 0;
static UNLOCK_OBSERVER: u8 = 0;

/// Subscribes to everything this file hears. Idempotent; main thread only, because the
/// workspace notifications are delivered on the main queue and the pump that drains them is
/// the main thread's.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if objc2::MainThreadMarker::new().is_none() {
        STARTED.store(false, Ordering::SeqCst);
        crate::logging::line(
            "macos",
            "system events: asked for off the main thread, so none are subscribed — a sleep, a lock \
             or a display change goes unnoticed this session",
        );
        return;
    }
    subscribe_workspace();
    let lock = subscribe_lock();
    let displays = subscribe_displays();
    crate::logging::line(
        "macos",
        &format!(
            "system events: sleep and wake, the displays' sleep and wake, this session going and \
             coming back, applications quitting — yes; the screen lock — {}; display changes — {}",
            if lock { "yes" } else { "NO (no distributed notification centre)" },
            match displays {
                Ok(()) => "yes".to_string(),
                Err(e) => format!("NO (CGDisplayRegisterReconfigurationCallback said {e:?})"),
            }
        ),
    );
}

fn subscribe_workspace() {
    let centre = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: framework-owned constants that live for the process.
    let events: [(&NSNotificationName, SystemEvent); 6] = unsafe {
        [
            (NSWorkspaceWillSleepNotification, SystemEvent::Sleep),
            (NSWorkspaceDidWakeNotification, SystemEvent::Wake),
            (NSWorkspaceScreensDidSleepNotification, SystemEvent::DisplaysAsleep),
            (NSWorkspaceScreensDidWakeNotification, SystemEvent::DisplaysAwake),
            (NSWorkspaceSessionDidResignActiveNotification, SystemEvent::Disconnected { remote: false }),
            (NSWorkspaceSessionDidBecomeActiveNotification, SystemEvent::Connected { remote: false }),
        ]
    };
    for (name, kind) in events {
        // Delivered on the main queue; the block pushes and returns, and nothing in it panics.
        let block = RcBlock::new(move |_n: NonNull<NSNotification>| push(kind));
        // SAFETY: the block is copied by the centre; the queue is the main one.
        let token = unsafe {
            centre.addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        };
        // Leaked deliberately, like every subscription in this backend: it lasts as long as
        // the process.
        core::mem::forget(token);
    }
    let quit = RcBlock::new(|n: NonNull<NSNotification>| {
        // An unwind out of a block Foundation calls would tear through frames that cannot
        // handle it.
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: a live notification for the duration of the call.
            if let Some(pid) = pid_of(unsafe { n.as_ref() }) {
                let mut q = TERMINATED.lock().unwrap_or_else(|e| e.into_inner());
                if q.len() < QUIT_MAX {
                    q.push(pid);
                } else {
                    QUIT_OVERFLOW.store(true, Ordering::Relaxed);
                }
            }
        }));
        if caught.is_err() {
            crate::logging::line("macos", "panic while handling an application quitting");
        }
    });
    // SAFETY: as above.
    let token = unsafe {
        centre.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidTerminateApplicationNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &quit,
        )
    };
    core::mem::forget(token);
}

/// The process a workspace notification names, from its `NSWorkspaceApplicationKey`.
fn pid_of(notification: &NSNotification) -> Option<i32> {
    let info = notification.userInfo()?;
    // SAFETY: a framework-owned constant.
    let key: &NSString = unsafe { NSWorkspaceApplicationKey };
    let object = info.objectForKey(key)?;
    // The dictionary is untyped and this key's value is documented to be the running
    // application; there is no typed accessor to do it for us (as in `watch::on_app_activated`).
    let app: &NSRunningApplication =
        unsafe { &*(&*object as *const objc2::runtime::AnyObject as *const NSRunningApplication) };
    Some(app.processIdentifier())
}

fn subscribe_lock() -> bool {
    let Some(centre) = CFNotificationCenter::distributed_center() else {
        return false;
    };
    let locked = CFString::from_static_str("com.apple.screenIsLocked");
    let unlocked = CFString::from_static_str("com.apple.screenIsUnlocked");
    // SAFETY: the observers are addresses of statics; the callbacks match
    // `CFNotificationCallback`; the centre keeps its own reference to each name.
    unsafe {
        centre.add_observer(
            &LOCK_OBSERVER as *const u8 as *const c_void,
            Some(on_locked),
            Some(&locked),
            core::ptr::null(),
            CFNotificationSuspensionBehavior::DeliverImmediately,
        );
        centre.add_observer(
            &UNLOCK_OBSERVER as *const u8 as *const c_void,
            Some(on_unlocked),
            Some(&unlocked),
            core::ptr::null(),
            CFNotificationSuspensionBehavior::DeliverImmediately,
        );
    }
    true
}

unsafe extern "C-unwind" fn on_locked(
    _centre: *mut CFNotificationCenter,
    _observer: *mut c_void,
    _name: *const CFString,
    _object: *const c_void,
    _info: *const CFDictionary,
) {
    push(SystemEvent::Lock);
}

unsafe extern "C-unwind" fn on_unlocked(
    _centre: *mut CFNotificationCenter,
    _observer: *mut c_void,
    _name: *const CFString,
    _object: *const c_void,
    _info: *const CFDictionary,
) {
    push(SystemEvent::Unlock);
}

fn subscribe_displays() -> Result<(), CGError> {
    // SAFETY: a callback that lives for the process, and no user data.
    let err = unsafe { CGDisplayRegisterReconfigurationCallback(Some(on_reconfigured), core::ptr::null_mut()) };
    if err == CGError::Success {
        Ok(())
    } else {
        Err(err)
    }
}

/// Called once per display before a reconfiguration (`BeginConfigurationFlag`) and once per
/// display after it; only the second kind counts — the change is not there yet on the first.
unsafe extern "C-unwind" fn on_reconfigured(
    _display: CGDirectDisplayID,
    flags: CGDisplayChangeSummaryFlags,
    _info: *mut c_void,
) {
    if !flags.contains(CGDisplayChangeSummaryFlags::BeginConfigurationFlag) {
        push(SystemEvent::DisplaysChanged);
    }
}

/// Does the backend's part of what happened, then hands it to the host, in order and as one
/// batch. Called first thing in `queue::drain`, so the keys and activations of the same drain see
/// the epochs a wake turned over.
pub fn drain(events: &mut dyn HostEvents) {
    let quit = std::mem::take(&mut *TERMINATED.lock().unwrap_or_else(|e| e.into_inner()));
    for pid in quit {
        forget_process(pid);
    }
    if QUIT_OVERFLOW.swap(false, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!(
                "more than {QUIT_MAX} applications quit between two turns of the loop; the rest are \
                 not forgotten — a later process given one of their pids may be named after it"
            ),
        );
    }
    let batch = crate::system_events::take();
    if batch.is_empty() {
        return;
    }
    for s in &batch {
        own_part(s.event);
    }
    if batch.iter().any(|s| s.event.front_may_have_changed()) {
        // Announced as an activation only when the host will not report the window in front
        // itself after this batch; otherwise that window would be reported twice in this drain.
        let host_reports_front = batch.iter().any(|s| s.event.reports_front());
        super::watch::recheck_front(!host_reports_front);
    }
    events.on_system(batch);
}

/// The backend's part of one event, before the host hears it. See the module note. Which event
/// brings the capture probe forward and which may have changed what is in front are
/// `SystemEvent`'s own answers, tested where `cargo test` runs. The focus round those moments
/// owe is the host's (`system_events::Plan::recheck`), in the same drain; an activation that
/// `watch::recheck_front` announces dirties the focus again itself (`queue::push_activated`).
fn own_part(kind: SystemEvent) {
    match kind {
        SystemEvent::Wake => super::tap::recheck_soon(super::tap::RECHECK_WAKE),
        SystemEvent::Unlock => super::tap::recheck_soon(super::tap::RECHECK_UNLOCK),
        SystemEvent::Connected { .. } => super::tap::recheck_soon(super::tap::RECHECK_SESSION),
        _ => {}
    }
    if kind.rechecks_capture() {
        super::capture::clear_sck_backoff(&format!("the {}", kind.words()));
    }
}

/// An application quit: everything kept about its pid goes, before macOS can hand the pid to
/// another process. A remembered executable name, bundle id, busy quarantine or window would
/// otherwise describe the new process as the old one — a window trigger matching the wrong
/// application — and a remembered observer, or a written-off one, would leave the new one
/// unobserved. See `watch::forget_process`, `ax::forget_process` and `handles::forget_pid`.
fn forget_process(pid: i32) {
    super::watch::forget_process(pid);
    super::ax::forget_process(pid);
    let dropped = super::handles::forget_pid(pid);
    super::ax::forget_handles(&dropped);
    crate::logging::trace("macos", || {
        format!("pid {pid} quit: forgotten, with {} handle(s)", dropped.len())
    });
}

/// The displays as the start-up block describes them (`perm::environment_report`, which this
/// repeats for the display part only, after a change): each display's size and origin in
/// points, its mode in points and pixels, the scale, and which is the primary. The macOS
/// `Backend::display_environment`: the host keeps the last lines written and writes these
/// again after a display change only when they differ.
pub(super) fn display_lines() -> Vec<(String, String)> {
    let mut ids = [0 as CGDirectDisplayID; 16];
    let mut count: u32 = 0;
    // SAFETY: a buffer of ours, its length passed in.
    let err = unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    if err != CGError::Success {
        return vec![("display".to_string(), format!("CGGetActiveDisplayList failed ({err:?}) after the change"))];
    }
    let count = (count as usize).min(ids.len());
    if count == 0 {
        return vec![("display".to_string(), "none reported by CoreGraphics after the change".to_string())];
    }
    let main = CGMainDisplayID();
    ids[..count]
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let bounds = CGDisplayBounds(id);
            let mode = CGDisplayCopyDisplayMode(id);
            let (pt_w, pt_h, px_w, px_h) = match mode.as_deref() {
                Some(m) => (
                    CGDisplayMode::width(Some(m)),
                    CGDisplayMode::height(Some(m)),
                    CGDisplayMode::pixel_width(Some(m)),
                    CGDisplayMode::pixel_height(Some(m)),
                ),
                None => (0, 0, 0, 0),
            };
            let scale = if pt_w > 0 { px_w as f64 / pt_w as f64 } else { 0.0 };
            (
                format!("display {i}"),
                format!(
                    "{}x{} pt at ({},{}) / mode {pt_w}x{pt_h} pt {px_w}x{px_h} px / scale {scale:.2} \
                     [cgid {id}]{} — after a change",
                    bounds.size.width,
                    bounds.size.height,
                    bounds.origin.x,
                    bounds.origin.y,
                    if id == main { " PRIMARY" } else { "" },
                ),
            )
        })
        .collect()
}
