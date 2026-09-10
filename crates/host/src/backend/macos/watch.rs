//! Noticing that the user moved somewhere else.
//!
//! Three things count as "somewhere else", and the second is the one the whole embedded
//! overlay system rests on: the frontmost application changed, the keyboard focus moved
//! WITHIN an application (a plugin window inside a DAW that is already frontmost raises no
//! application-level event at all), or the frontmost window's title changed.
//!
//! Only the first of those is a system-wide notification. The other two exist per
//! application and nowhere else: an `AXObserver` is created against one pid and hears
//! nothing about any other process, so this module has to decide which applications are
//! worth listening to. Creating one reaches across into the target application and can
//! block on an application that is busy, so it happens exactly once per pid — the first
//! time that application comes to the front — is cached, and is dropped when the process
//! exits. It never happens on the pump path.
//!
//! Every callback here does the same three things and nothing else: decide whether the
//! notification is interesting, set a flag or push onto the queue, return. The pump picks
//! it up within 15 ms. Reading a window's geometry from inside a notification handler is
//! how a notification storm turns into a stalled keyboard.

use core::ffi::c_void;
use core::ptr::NonNull;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2_app_kit::{
    NSAccessibilityFocusedUIElementChangedNotification,
    NSAccessibilityFocusedWindowChangedNotification, NSAccessibilityTitleChangedNotification,
    NSAccessibilityWindowCreatedNotification, NSRunningApplication, NSWorkspace,
    NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_application_services::{AXError, AXObserver, AXUIElement};
use objc2_core_foundation::{
    kCFRunLoopCommonModes, CFRetained, CFRunLoop, CFRunLoopSource, CFString,
};
use objc2_foundation::{NSNotification, NSOperationQueue, NSString};

/// `start()` has run. The three hooks live for the process lifetime, so this is one-way.
static STARTED: AtomicBool = AtomicBool::new(false);

/// The pid the workspace last told us is frontmost, or 0.
///
/// An atomic rather than a lookup because it is the filter on the noisy notifications and
/// on the menu signal, and both of those are read from callbacks that must not make a call
/// into another process to decide whether they care.
static FRONT_PID: AtomicI32 = AtomicI32::new(0);

/// How many menus the frontmost application currently has open. See [`native_menu_open`].
static MENU_DEPTH: AtomicI32 = AtomicI32::new(0);
/// When [`MENU_DEPTH`] last moved, in milliseconds since the first call to `now_ms`.
static MENU_SINCE_MS: AtomicU64 = AtomicU64::new(0);
/// Whether any accessibility notification has ever arrived, so the log can say so once.
static HEARD_ANYTHING: AtomicBool = AtomicBool::new(false);

/// A menu that has been "open" longer than this is assumed to be a notification we never
/// got rather than a user still reading.
///
/// The number is a guess and wants measuring. What it guards is the one failure this
/// module could cause that a user cannot see: a stuck flag hands every captured key to the
/// application underneath, so the overlay simply stops answering, permanently and
/// silently. Erring long is safe for the other direction — while it is wrongly still
/// "open" the overlay is merely disarmed, which is exactly what a real open menu wants.
const STUCK_MENU_MS: u64 = 60_000;

/// One application we are listening to.
struct Live {
    /// Held because the subscription dies with it; releasing an `AXObserver` also takes its
    /// source back out of the run loop.
    _observer: CFRetained<AXObserver>,
    source: CFRetained<CFRunLoopSource>,
    app: String,
}

/// Why an application is not being observed, and whether it is worth asking again.
struct Refusal {
    reason: String,
    /// `false` when the refusal is about *our* permissions rather than the target: the user
    /// can grant Accessibility mid-session, and an application written off at launch would
    /// then never be re-tried.
    permanent: bool,
}

/// What we know about one application's focus notifications.
enum Watched {
    /// Subscribed, and the subscription is alive as long as this is.
    Live(Live),
    /// Turned us down, this many times.
    Refused(u8),
}

/// How often an application that refused everything is asked again.
///
/// Not once, and not forever. An application asked while it is still starting up can refuse
/// every notification and then work perfectly a second later — sforzando loads a sample
/// engine before its window is worth anything — so writing it off on the first answer means
/// a session-long blindness caused by nothing but timing. Asking on every switch instead
/// would spend a cross-process round trip per notification, every time, on an application
/// that genuinely has no accessibility to offer.
const REFUSAL_RETRIES: u8 = 3;

/// A BUSY refusal is asked again on a clock, not only on the next application switch.
///
/// The third Mac session showed what "only on the next switch" costs. The tester restarted
/// sforzando; the new process refused its first subscription as busy, and the retry that
/// would have come with the next activation never came, because he stayed inside sforzando
/// for the rest of the session — every menu he opened ran in a process with no observer,
/// so whether its menus post `AXMenuOpened` is still unknown. REAPER got the same
/// treatment: refused once, frontmost until the end, never asked again, and the overlay that
/// waits for REAPER's focus notifications could not wake. Both applications were frontmost
/// the whole time, which is the one case a switch-driven retry cannot cover.
///
/// Doubling from two seconds and capped at a minute, ten times, so a plugin that takes a
/// minute to load its samples is caught within a minute and a half and one that never answers
/// costs a bounded handful of calls. Only the FRONTMOST application is retried — it is the
/// one the user is in, and the only one whose notifications matter until they switch, at
/// which point the activation path asks anyway — and never while it is in the busy
/// quarantine, because a subscription attempt against a wedged process is a messaging
/// timeout paid on the thread that carries the event tap.
const RETRY_FIRST: Duration = Duration::from_secs(2);
const RETRY_CAP: Duration = Duration::from_secs(60);
const RETRY_MAX: u8 = 10;

/// One application waiting to be asked again.
struct Retry {
    app: String,
    due: Instant,
    tries: u8,
}

thread_local! {
    /// pid to what we know about it. A `Refused` entry that has used up its retries is an
    /// application that will not be asked again, kept so a DAW that cannot be observed costs
    /// a few attempts rather than one per window switch for the rest of the session.
    static OBSERVERS: RefCell<HashMap<i32, Watched>> = RefCell::new(HashMap::new());

    /// Applications that refused as busy and are owed another attempt. See [`RETRY_FIRST`].
    static RETRY: RefCell<HashMap<i32, Retry>> = RefCell::new(HashMap::new());

    /// The two menu-tracking notification names.
    ///
    /// AppKit exports a constant for every notification this module uses except these:
    /// `kAXMenuOpenedNotification` and `kAXMenuClosedNotification` are HIServices names,
    /// and the generated constants file for those is empty in the bindings we build
    /// against. The strings are the documented values from `AXNotificationConstants.h`.
    /// A `thread_local` and not a `static` because `CFString` is neither `Send` nor `Sync`.
    static MENU_OPENED: CFRetained<CFString> = CFString::from_static_str("AXMenuOpened");
    static MENU_CLOSED: CFRetained<CFString> = CFString::from_static_str("AXMenuClosed");
}

/// Idempotent.
///
/// Installs the workspace hook, then registers the application that is frontmost right now
/// — without that last step nothing would be observed until the user switched applications
/// once, and the plugin they already had open would never be detected.
pub fn start() -> Result<(), String> {
    if STARTED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    // Everything below hangs off run-loop sources and thread-locals belonging to whichever
    // thread runs this. That has to be the thread wxWidgets pumps, or the sources are added
    // to a run loop nobody runs and not one notification is ever delivered — a failure that
    // looks exactly like an application that refuses accessibility.
    if objc2::MainThreadMarker::new().is_none() {
        STARTED.store(false, Ordering::SeqCst);
        return Err("watch_foreground called off the main thread; no events would arrive".into());
    }

    install_workspace_hook();

    let front = NSWorkspace::sharedWorkspace().frontmostApplication();
    match front {
        Some(app) => {
            let pid = app.processIdentifier();
            let name = app_name(&app);
            FRONT_PID.store(pid, Ordering::Relaxed);
            crate::logging::line(
                "macos",
                &format!("watching for focus changes; frontmost is {name} (pid {pid})"),
            );
            ensure_observer(pid, &name);
        }
        None => crate::logging::line(
            "macos",
            "watching for focus changes; no application is frontmost yet",
        ),
    }

    // Whatever is in front already has to be looked at once, or an overlay only ever
    // activates after the first application switch. A focus dispatch is the right shape for
    // it: unlike an activation it does not need the window to have a title yet.
    super::queue::mark_focus_dirty();
    Ok(())
}

/// Is a system menu open right now?
///
/// Called from inside the key path and from a 150 ms timer, so it has to be microseconds.
/// Whatever answers it, it must not be an accessibility traversal.
///
/// The answer is precomputed. Applications post `AXMenuOpened` / `AXMenuClosed` as their
/// menus track, our per-application observer already hears them, and all this does is read
/// the counter that callback keeps — one relaxed load in the overwhelmingly common case
/// where nothing is open.
///
/// Two things are deliberate. It counts rather than latches, because a submenu opening
/// posts a second `AXMenuOpened` and its closing must not clear the parent. And it counts
/// **only for the frontmost application**: the Windows side shipped a version of this that
/// asked a system-wide question, and any other program with a menu open disarmed the
/// overlay entirely — Alt+V stopped reaching it and REAPER's View menu got the key instead.
/// Scoping to the application the user is actually in is the whole fix, and it is free
/// here because the notification arrives already labelled with its pid.
///
/// How much to trust it: the mechanism is right, the coverage is unverified. Cocoa menus
/// post both notifications and that is what a DAW's own menu bar is made of. A plugin that
/// draws its own menu inside its window may post neither — which is why the platform has a
/// second, independent flag for exactly that case (`set_menu_open`, driven by the module
/// menu watch), and why this one answering "no menu" for such a plugin is not a regression.
/// What a real Mac has to settle is whether the two notifications arrive in balanced pairs
/// in the applications we care about; the log records every transition, so one session with
/// a DAW's File menu opened and closed a few times answers it.
pub fn native_menu_open() -> bool {
    if MENU_DEPTH.load(Ordering::Relaxed) <= 0 {
        return false;
    }
    let age = now_ms().saturating_sub(MENU_SINCE_MS.load(Ordering::Relaxed));
    if age > STUCK_MENU_MS {
        MENU_DEPTH.store(0, Ordering::Relaxed);
        crate::logging::line(
            "macos",
            &format!(
                "a menu has been open for {age} ms with no close notification — assuming it is \
                 gone and re-arming; if the overlay felt dead until now, this is why"
            ),
        );
        return false;
    }
    true
}

/// Subscribes to the frontmost-application change.
///
/// Delivered on the main queue on purpose. The queues this feeds are thread-locals of the
/// pump thread, so a notification handled anywhere else would push into a queue that is
/// never drained — silently, and only on whichever machine happens to post it off-thread.
fn install_workspace_hook() {
    let centre = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: the block captures nothing, so it is trivially sendable, and it is scheduled
    // on the main queue, which is the thread everything it touches belongs to.
    let block = RcBlock::new(|notification: NonNull<NSNotification>| {
        // An unwind out of a block called by Foundation would tear through frames that
        // cannot handle it, and the process would be gone with nothing in the log.
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            on_app_activated(unsafe { notification.as_ref() });
        }));
        if caught.is_err() {
            crate::logging::line("macos", "panic while handling an application activation");
        }
    });
    let token = unsafe {
        centre.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    // Leaked deliberately, as the Windows backend leaks its hook handles: the subscription
    // lasts as long as the process, and there is nothing to release it into.
    core::mem::forget(token);
}

/// A different application came to the front.
fn on_app_activated(notification: &NSNotification) {
    let Some(info) = notification.userInfo() else {
        crate::logging::line("macos", "an activation notification arrived with no userInfo");
        return;
    };
    let key: &NSString = unsafe { NSWorkspaceApplicationKey };
    let Some(object) = info.objectForKey(key) else {
        crate::logging::line("macos", "an activation notification named no application");
        return;
    };
    // The dictionary is untyped and this key's value is documented to be the running
    // application; there is no typed accessor to do it for us.
    let app: &NSRunningApplication =
        unsafe { &*(&*object as *const objc2::runtime::AnyObject as *const NSRunningApplication) };

    let pid = app.processIdentifier();
    let name = app_name(app);
    FRONT_PID.store(pid, Ordering::Relaxed);

    // Whatever menu we believed was open belonged to the application we just left. Anything
    // else risks carrying a stuck flag into an application that never opened a menu at all.
    if MENU_DEPTH.swap(0, Ordering::Relaxed) != 0 {
        crate::logging::trace("macos", || {
            format!("menu state cleared: {name} (pid {pid}) came to the front")
        });
    }

    // The window, not the application, is what the host activates on. Resolving it here
    // rather than in the callback-to-pump handoff costs one accessibility read on a path
    // that runs when the user switches applications — rare, and the alternative is the pump
    // asking a question whose answer has already moved on. It is still a call into another
    // process, though, so it is timed: an application that answers slowly here is the shape
    // of the problem that ends with keystrokes going missing, and a log nobody can reproduce
    // from is the only diagnosis anyone will get.
    let started = Instant::now();
    // `None` means the application did not answer. Zero is what the rest of this path is
    // written around — the drain resolves it, fails to match and arms the delayed re-check
    // ladder, which is exactly what an application too busy to talk needs — so the two
    // collapse HERE rather than silently further down, and the log says which it was.
    let answered = super::ax::foreground_window_id();
    let window = answered.unwrap_or(0);
    let took = started.elapsed().as_millis();
    if took >= 50 {
        crate::logging::line(
            "macos",
            &format!("resolving the frontmost window of {name} (pid {pid}) took {took} ms"),
        );
    }
    // The key tap compares against this rather than resolving it itself; doing that inside
    // its callback would be a cross-process call on the one path that must not make any.
    //
    // Not told anything when the application did not answer. Storing the 0 would say "no
    // window is in front", which closes the scope gate for every hotkey an overlay owns; not
    // storing leaves the last thing that was actually known, and `active_window` corrects it
    // on the next tick that gets a real answer.
    match answered {
        Some(w) => super::tap::note_foreground(w),
        None => crate::logging::line(
            "macos",
            &format!(
                "{name} (pid {pid}) came to the front without answering which of its windows \
                 is in front; the tap keeps what it had rather than being told there is none"
            ),
        ),
    }
    crate::logging::trace("macos", || {
        format!("activated: {name} (pid {pid}), frontmost window handle {window}")
    });
    // Pushed even when it is 0: the drain resolves it, fails to match, and arms the delayed
    // re-check ladder — which is exactly what a window that is not titled yet needs.
    super::queue::push_activated(window);

    // Subscribing LAST, and the order is the whole point. It is six synchronous calls into
    // an application that has this instant been brought to the front and is therefore at its
    // busiest — each capped at a quarter second, so up to a second and a half — and it buys
    // nothing for the switch that is happening now. It is about noticing the NEXT change.
    // Ahead of the window read it simply delayed the answer the user is waiting for, on
    // exactly the path where an application slow to answer is the reported complaint.
    ensure_observer(pid, &name);
}

/// Makes sure we are listening to this application, at most once per pid.
fn ensure_observer(pid: i32, app: &str) {
    if pid <= 0 {
        return;
    }
    if pid == std::process::id() as i32 {
        // Observing ourselves would feed our own tray and dialog focus back into the
        // detection path, and asking the accessibility API about the process that is
        // currently inside an accessibility callback is a documented way to hang.
        crate::logging::trace("macos", || "not observing our own process".to_string());
        return;
    }
    let already_refused = match OBSERVERS.with(|o| match o.borrow().get(&pid) {
        Some(Watched::Live(_)) => None,
        Some(Watched::Refused(n)) => Some(*n),
        None => Some(0),
    }) {
        None => return,
        Some(n) if n >= REFUSAL_RETRIES => return,
        Some(n) => n,
    };

    // Only on a miss, so the cost is paid once per newly seen application rather than on
    // every switch, and a long session cannot accumulate observers for processes that have
    // since exited.
    reap_dead();

    match create_observer(pid, app) {
        Ok(live) => {
            let after = RETRY.with(|r| r.borrow_mut().remove(&pid)).map(|r| r.tries);
            crate::logging::line(
                "macos",
                &match after {
                    Some(n) => format!("observing focus in {app} (pid {pid}), on retry {n}"),
                    None => format!("observing focus in {app} (pid {pid})"),
                },
            );
            OBSERVERS.with(|o| o.borrow_mut().insert(pid, Watched::Live(live)));
        }
        Err(refusal) => {
            if refusal.permanent {
                crate::logging::line(
                    "macos",
                    &format!(
                        "no focus observer for {app} (pid {pid}): {} — overlays inside this \
                         application will only notice a change when it is brought to the \
                         front",
                        refusal.reason
                    ),
                );
                OBSERVERS.with(|o| o.borrow_mut().insert(pid, Watched::Refused(already_refused + 1)));
            } else {
                schedule_retry(pid, app, &refusal.reason);
            }
        }
    }
}

/// Puts a busy application on the clock, or gives up on it after [`RETRY_MAX`] attempts.
fn schedule_retry(pid: i32, app: &str, reason: &str) {
    RETRY.with(|r| {
        let mut r = r.borrow_mut();
        let tries = r.get(&pid).map(|e| e.tries).unwrap_or(0);
        if tries >= RETRY_MAX {
            r.remove(&pid);
            crate::logging::line(
                "macos",
                &format!(
                    "no focus observer for {app} (pid {pid}): {reason} — asked {tries} times \
                     over a minute and a half; it will be asked again when it next comes to \
                     the front, and until then overlays inside it only notice a change when \
                     it is brought to the front"
                ),
            );
            return;
        }
        let wait = RETRY_FIRST.saturating_mul(1u32 << tries.min(6)).min(RETRY_CAP);
        crate::logging::line(
            "macos",
            &format!(
                "no focus observer for {app} (pid {pid}): {reason} — asking again in {} s \
                 (attempt {} of {RETRY_MAX})",
                wait.as_secs(),
                tries + 1
            ),
        );
        r.insert(pid, Retry { app: app.to_string(), due: Instant::now() + wait, tries: tries + 1 });
    });
}

/// Asks the frontmost application again if it refused as busy and its turn has come.
///
/// Called from the pump on every iteration; costs one map lookup when nothing is owed. Only
/// the frontmost application, and never one in the busy quarantine — see [`RETRY_FIRST`]
/// for why both.
pub fn retry_refused() {
    let front = FRONT_PID.load(Ordering::Relaxed);
    let owed = RETRY.with(|r| {
        r.borrow()
            .get(&front)
            .filter(|e| Instant::now() >= e.due)
            .map(|e| e.app.clone())
    });
    let Some(app) = owed else {
        return;
    };
    if super::ax::is_busy(front) {
        // The due time stays in the past; the next iteration after the quarantine clears
        // asks. Traced, not logged: this can be true on every iteration for five seconds.
        crate::logging::trace("macos", || {
            format!("not asking {app} (pid {front}) for notifications yet, it is in quarantine")
        });
        return;
    }
    if NSRunningApplication::runningApplicationWithProcessIdentifier(front).is_none() {
        RETRY.with(|r| r.borrow_mut().remove(&front));
        return;
    }
    ensure_observer(front, &app);
}

/// Creates the observer, subscribes it, and puts it on this thread's run loop.
fn create_observer(pid: i32, app: &str) -> Result<Live, Refusal> {
    let mut raw: *mut AXObserver = core::ptr::null_mut();
    // SAFETY: the callback matches AXObserverCallback and outlives the observer (it is a
    // function item), and `raw` is a live local.
    let err = unsafe { AXObserver::create(pid, Some(ax_callback), NonNull::from(&mut raw)) };
    if err != AXError::Success {
        return Err(Refusal {
            reason: format!("AXObserverCreate said {}", ax_err(err)),
            permanent: err != AXError::APIDisabled,
        });
    }
    let Some(raw) = NonNull::new(raw) else {
        return Err(Refusal {
            reason: "AXObserverCreate succeeded but produced no observer".into(),
            permanent: true,
        });
    };
    // Create rule: the crate does not wrap the out-parameter, so we take ownership here.
    let observer = unsafe { CFRetained::from_raw(raw) };

    let element = unsafe { AXUIElement::new_application(pid) };
    // Subscribing is a synchronous call into the target application, and the default
    // timeout is seconds. Against a DAW rendering audio or a plugin mid-repaint that is
    // long enough to lose keystrokes on the way past, so cap it before the first use of
    // this element rather than after.
    // The same budget as everything else, and the reason is measured rather than tidy. A
    // quarter of a second was chosen to keep subscription off the critical path; but this
    // application is asked for on the very tick it comes to the front, when it is at its
    // busiest, and sforzando on the tester's 2015 machine has been timed needing close to
    // seven hundred milliseconds to answer a single read. Six subscriptions at a quarter
    // second each therefore all failed, the refusal was recorded as final, and the menu
    // notifications this backend needs never arrived for the one plugin being tested.
    let _ = unsafe { element.set_messaging_timeout(super::ax::MESSAGING_TIMEOUT) };

    // The pid travels in the refcon rather than in a boxed context: it is the only thing
    // the callback needs, it fits in the pointer, and there is then nothing to keep alive
    // or free when the observer goes.
    let refcon = pid as isize as *mut c_void;

    let menu_opened = MENU_OPENED.with(|s| s.clone());
    let menu_closed = MENU_CLOSED.with(|s| s.clone());
    let wanted: [(&str, &CFString); 6] = [
        // The two that carry the embedded-overlay case: a plugin window opening inside a
        // DAW that is already frontmost moves the focus and nothing else.
        (
            "AXFocusedUIElementChanged",
            ns_to_cf(unsafe { NSAccessibilityFocusedUIElementChangedNotification }),
        ),
        (
            "AXFocusedWindowChanged",
            ns_to_cf(unsafe { NSAccessibilityFocusedWindowChangedNotification }),
        ),
        // For the window that appears before it has a title. Komplete Kontrol's
        // Preferences dialog does exactly that on Windows and settles a beat later with no
        // further event; the title change is what catches it.
        (
            "AXTitleChanged",
            ns_to_cf(unsafe { NSAccessibilityTitleChangedNotification }),
        ),
        (
            "AXWindowCreated",
            ns_to_cf(unsafe { NSAccessibilityWindowCreatedNotification }),
        ),
        ("AXMenuOpened", &menu_opened),
        ("AXMenuClosed", &menu_closed),
    ];

    let mut registered = 0usize;
    let mut disabled = false;
    let mut busy = false;
    for (label, name) in wanted {
        // SAFETY: both arguments outlive the call; the refcon is an integer we never read
        // back as a pointer.
        let err = unsafe { observer.add_notification(&element, name, refcon) };
        if err == AXError::Success || err == AXError::NotificationAlreadyRegistered {
            registered += 1;
            continue;
        }
        disabled |= err == AXError::APIDisabled;
        // Not traced. This used to be behind the trace switch, with the summary below
        // telling the reader to turn it on and try again — advice that only works for
        // somebody who can reproduce the problem on demand, which is nobody here. At most
        // six lines per application per attempt is a price worth paying to never ask a
        // remote tester for another session over it.
        crate::logging::line(
            "macos",
            &format!("{app} (pid {pid}) refused {label}: {}", ax_err(err)),
        );
        // An application that is merely busy is not an application that cannot be observed,
        // and asking it five more times while it is still busy answers nothing. Stop at the
        // first such refusal and let the retry counter come back when it has settled: the
        // remaining five subscriptions would each cost the full messaging timeout, which is
        // the difference between one second and six on the tick a window comes forward.
        if matches!(err, AXError::CannotComplete | AXError::Failure) {
            busy = true;
            break;
        }
    }
    if registered == 0 {
        return Err(Refusal {
            reason: if busy {
                "it was too busy to answer — it may have been starting up".into()
            } else {
                "it accepted no notifications at all".into()
            },
            permanent: !disabled && !busy,
        });
    }
    if registered < wanted.len() {
        crate::logging::line(
            "macos",
            &format!(
                "{app} (pid {pid}) accepted {registered} of {} notifications; the refusals \
                 are named above",
                wanted.len()
            ),
        );
    }

    // Get rule: the crate retains this for us, so holding it is enough.
    let source = unsafe { observer.run_loop_source() };
    let Some(run_loop) = CFRunLoop::current() else {
        return Err(Refusal {
            reason: "this thread has no run loop to deliver on".into(),
            permanent: false,
        });
    };
    // Common modes rather than the default one: a menu tracking or a window being dragged
    // puts the run loop into a modal mode, and those are precisely the moments the overlay
    // needs to keep hearing about.
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });

    Ok(Live {
        _observer: observer,
        source,
        app: app.to_string(),
    })
}

/// Forgets the applications that have exited.
///
/// A `CFRetained<AXObserver>` for a dead process is harmless but not free — it keeps a mach
/// port and a run-loop source — and a session that visits a plugin scanner, a browser and a
/// dozen standalone instruments would keep every one of them.
fn reap_dead() {
    let run_loop = CFRunLoop::current();
    OBSERVERS.with(|observers| {
        observers.borrow_mut().retain(|pid, live| {
            // Two independent answers before anything is declared gone. Reaping an observer
            // that is still needed is the expensive mistake — the application keeps running
            // and simply stops being noticed — whereas keeping a dead one costs a mach port
            // until the next miss. `kill` with signal 0 reports a zombie as alive and says
            // "not yours" for a process owned by somebody else; AppKit's answer is about
            // applications specifically, which is what everything in this table is.
            let alive = unsafe { libc::kill(*pid, 0) } == 0
                || NSRunningApplication::runningApplicationWithProcessIdentifier(*pid).is_some();
            if alive {
                return true;
            }
            if let (Some(run_loop), Watched::Live(live)) = (run_loop.as_ref(), &*live) {
                run_loop.remove_source(Some(&live.source), unsafe { kCFRunLoopCommonModes });
                crate::logging::line(
                    "macos",
                    &format!("stopped observing {} (pid {pid}): it has exited", live.app),
                );
            }
            false
        })
    });
}

/// Every accessibility notification we asked for arrives here, on the run loop.
///
/// Note the ABI: the bindings declare this callback `extern "C-unwind"`, which means a
/// panic would be allowed to travel out into Apple's frames rather than aborting at the
/// boundary. Nothing in this file panics, and the guard makes sure that stays true of
/// anything added later.
unsafe extern "C-unwind" fn ax_callback(
    _observer: NonNull<AXObserver>,
    _element: NonNull<AXUIElement>,
    notification: NonNull<CFString>,
    refcon: *mut c_void,
) {
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pid = refcon as isize as i32;
        // Borrowed for the duration of the callback only; nothing here keeps it.
        let name: &CFString = unsafe { notification.as_ref() };
        on_notification(pid, name);
    }));
    if caught.is_err() {
        crate::logging::line("macos", "panic while handling an accessibility notification");
    }
}

/// Decides what a notification means, and does the smallest possible thing about it.
///
/// Compared by `CFEqual` against the framework's own constants rather than by rendering the
/// name into a `String`: the focus notifications arrive whenever anyone moves the caret, and
/// an allocation each would be a poor trade for readability.
fn on_notification(pid: i32, name: &CFString) {
    if !HEARD_ANYTHING.swap(true, Ordering::Relaxed) {
        crate::logging::line(
            "macos",
            &format!("first accessibility notification received (pid {pid}) — observation works"),
        );
    }

    // Ordered by how often each arrives. Focus first: it is the common case and the one the
    // embedded overlays depend on. Deliberately not filtered by pid — the Windows hook does
    // not filter its focus event either, and a focus move can legitimately arrive a beat
    // before the activation that explains it, so a pid test here would drop exactly the
    // event that says a plugin window has just opened.
    if name == ns_to_cf(unsafe { NSAccessibilityFocusedWindowChangedNotification }) {
        // The other half of what the key tap's scope gate reads. A different window of the
        // same application coming to the front raises no workspace notification, so without
        // this the gate would go on comparing against a window that is no longer in front —
        // and keep swallowing keys that belong to whatever replaced it.
        // Only when the application actually said which window it is. This notification is
        // the ONLY thing that tells the tap about a window change inside one application —
        // no workspace notification is raised for it — so a fabricated 0 written here is
        // sticky: nothing else is due to correct it while the user stays in the plugin, and
        // every scoped hotkey stops matching in the meantime. `active_window` refreshes the
        // same fact from the pump, which is what closes the gap this leaves open.
        match super::ax::foreground_window_id() {
            Some(w) => super::tap::note_foreground(w),
            None => crate::logging::trace("macos", || {
                format!("focused window changed in pid {pid}, which did not say to what")
            }),
        }
        crate::logging::trace("macos", || format!("focused window changed in pid {pid}"));
        super::queue::mark_focus_dirty();
        return;
    }
    if name == ns_to_cf(unsafe { NSAccessibilityFocusedUIElementChangedNotification }) {
        crate::logging::trace("macos", || format!("focus moved within pid {pid}"));
        super::queue::mark_focus_dirty();
        return;
    }

    let front = FRONT_PID.load(Ordering::Relaxed);

    if name == ns_to_cf(unsafe { NSAccessibilityTitleChangedNotification })
        || name == ns_to_cf(unsafe { NSAccessibilityWindowCreatedNotification })
    {
        // Filtered to the frontmost application, as the Windows hook filters title changes
        // to the foreground window. Titles change constantly — a document editor retitles
        // on every keystroke — and each one that gets through costs the most expensive
        // dispatch the host has, fanned out across every module and every overlay it owns.
        if pid != front {
            return;
        }
        crate::logging::trace("macos", || {
            format!("frontmost application (pid {pid}) retitled or opened a window")
        });
        super::queue::mark_focus_dirty();
        return;
    }

    let opened = MENU_OPENED.with(|s| name == &**s);
    let closed = !opened && MENU_CLOSED.with(|s| name == &**s);
    if !opened && !closed {
        return;
    }
    // The lesson from the Windows implementation, applied where it costs nothing: another
    // application's menu must never disarm this one.
    if pid != front {
        crate::logging::trace("macos", || {
            format!("ignoring a menu notification from pid {pid}; {front} is frontmost")
        });
        return;
    }

    // Read-modify-write without a compare-exchange: only the run loop this observer is on
    // ever writes these, and `native_menu_open`'s unstick is a store of the same value it
    // would converge to anyway.
    MENU_SINCE_MS.store(now_ms(), Ordering::Relaxed);
    let before = MENU_DEPTH.load(Ordering::Relaxed);
    let after = if opened { before + 1 } else { (before - 1).max(0) };
    MENU_DEPTH.store(after, Ordering::Relaxed);

    if before == 0 && after == 1 {
        crate::logging::line(
            "macos",
            &format!("a menu opened in pid {pid}; captured keys pass through until it closes"),
        );
    } else if before > 0 && after == 0 {
        crate::logging::line("macos", &format!("the menu in pid {pid} closed"));
    } else {
        crate::logging::trace("macos", || {
            format!("menu depth in pid {pid}: {before} -> {after}")
        });
    }
}

/// An application's name for the log, never empty.
fn app_name(app: &NSRunningApplication) -> String {
    app.localizedName()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| app.bundleIdentifier().map(|s| s.to_string()))
        .unwrap_or_else(|| "an unnamed application".to_string())
}

/// Milliseconds since the first call, monotonic.
///
/// `Instant` rather than the wall clock so that a clock adjustment cannot make an open menu
/// look an hour old and re-arm the overlay underneath a user who is still reading it.
fn now_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// AppKit publishes every accessibility name this module needs; HIServices publishes none
/// of them in these bindings. They are the same strings, and `NSString` and `CFString` are
/// toll-free bridged, so borrowing AppKit's is free and rules out a typo in a name that
/// would otherwise fail silently at registration time.
#[inline]
fn ns_to_cf(s: &NSString) -> &CFString {
    // SAFETY: toll-free bridged, and the reference is only reborrowed, never freed.
    unsafe { &*(s as *const NSString as *const CFString) }
}

/// The symbol as well as the number.
///
/// `Debug` alone prints `AXError(-25204)`, and the whole point of these lines is that
/// somebody reading a log from a machine they do not have can tell "this application does
/// not speak accessibility" from "permission is missing" from "it was simply busy".
fn ax_err(err: AXError) -> String {
    let name = match err {
        AXError::Success => "Success",
        AXError::Failure => "Failure",
        AXError::IllegalArgument => "IllegalArgument",
        AXError::InvalidUIElement => "InvalidUIElement",
        AXError::InvalidUIElementObserver => "InvalidUIElementObserver",
        AXError::CannotComplete => "CannotComplete (the application was busy)",
        AXError::AttributeUnsupported => "AttributeUnsupported",
        AXError::ActionUnsupported => "ActionUnsupported",
        AXError::NotificationUnsupported => "NotificationUnsupported",
        AXError::NotImplemented => "NotImplemented (it does not speak accessibility)",
        AXError::NotificationAlreadyRegistered => "NotificationAlreadyRegistered",
        AXError::NotificationNotRegistered => "NotificationNotRegistered",
        AXError::APIDisabled => "APIDisabled (grant Accessibility in System Settings)",
        AXError::NoValue => "NoValue",
        AXError::ParameterizedAttributeUnsupported => "ParameterizedAttributeUnsupported",
        AXError::NotEnoughPrecision => "NotEnoughPrecision",
        _ => "an unnamed error",
    };
    format!("{name} [{}]", err.0)
}
