//! `automation-platform page-check`: how this application takes the front and gives it back,
//! measured on the Mac it runs on — the first section of the check the Mac's page window will grow
//! (TODO.md, MW5).
//!
//! No wxWidgets and no module: an application object of its own, an accessory application as the
//! bundle makes the real one, and an event loop of its own. A plain panel set up as the reference
//! tool sets up its own — titled, closable, resizable, a utility window, created at once, not
//! hidden when the application is not the active one, on every Space and beside a full-screen
//! application, out of the window cycle — is shown the way a page will be, and the front is handed
//! back both ways to the application that was in front before, and over once more the way
//! `host.window.focus` hands it, with no window of ours leaving. What runs is the application's own
//! code: `app::activate_self` takes the front, `front::hand_front_to` gives it back. What it prints
//! is what AppKit and the workspace said, and after how long: every activation the workspace
//! announced, in order, and whether the focus the hand-over waited for was made or given up.
//!
//! The answers depend on the desktop it runs on — CI starts TextEdit first, as the application to
//! hand the front back to — so every line is data: the check fails only when it cannot run (exit
//! status 1) or for a wrong argument (2). Every line starts with `PAGE CHECK:`; nothing goes into
//! the application's log, which is never opened here.
//!
//! The waits are a test's, not the application's: a section waits for the notices it measures for
//! at most [`SECTION`] all together, and the activation policy, which announces nothing, is read
//! after [`SETTLE`]. Four shows, three hand-overs and the non-activating panel, five seconds each at
//! the most, and three settles bound the run to about 42 seconds.

use std::cell::RefCell;
use std::io::Write as _;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{autoreleasepool, Retained};
use objc2::{MainThreadMarker, MainThreadOnly as _};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSBackingStoreType, NSEventMask, NSFloatingWindowLevel,
    NSPanel, NSRunningApplication, NSWindow, NSWindowCollectionBehavior, NSWindowDidBecomeKeyNotification,
    NSWindowStyleMask, NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSNotification, NSNotificationCenter, NSOperationQueue, NSPoint,
    NSProcessInfo, NSRect, NSSize, NSString,
};

use super::front::{self, Ended, Handed};
use crate::backend::Handing;

const USAGE: &str = "\
automation-platform page-check

Measures how this application takes the front and gives it back on this Mac: a plain panel shown
the way a page will be, the front handed back both ways to the application that was in front
before (start one first, TextEdit for instance) and over once more the way host.window.focus hands
it, a panel that does not activate the application, and the activation policy switched while a
panel is shown. Opens windows of its own for some seconds, loads no module and writes nothing to
the application's log; every line it prints starts with PAGE CHECK:. Exit status 0 when it ran to
the end, 1 when it could not run, 2 for a wrong argument.";

/// How long a section waits for the notices it measures: a bound on the test, never a decision
/// of the application's.
const SECTION: Duration = Duration::from_secs(5);

/// How long the event loop runs after a change that announces nothing — the activation policy, and
/// the launch — before what it did is read.
const SETTLE: Duration = Duration::from_millis(500);

/// What a hand-over is measured as.
#[derive(Clone, Copy)]
enum Over {
    /// A page leaving, this way: the panel is the window of ours giving up the keyboard.
    Leaving(Handing),
    /// `host.window.focus`: nothing of ours leaves, the way the application uses; the panel is
    /// ordered out once it is measured.
    Focus,
}

/// What a notice said, in the order they came.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seen {
    /// This application became the active one.
    Active,
    /// This application stopped being the active one.
    Resigned,
    /// The window of ours with this number became the key window.
    Key(isize),
    /// The workspace named this process the active application.
    Front(i32),
}

thread_local! {
    /// Every notice since the start, with when it came.
    static SEEN: RefCell<Vec<(Instant, Seen)>> = const { RefCell::new(Vec::new()) };
}

/// The check: 0 when it ran to the end, 1 when it could not run, 2 for a wrong argument.
pub(crate) fn run(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        None => {}
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return 0;
        }
        Some(other) => {
            eprintln!("page-check: unknown argument '{other}'\n\n{USAGE}");
            return 2;
        }
    }
    let Some(mtm) = MainThreadMarker::new() else {
        say("cannot run: not on the main thread");
        return 1;
    };
    autoreleasepool(|_| check(mtm));
    say("done");
    0
}

/// One line of output, written at once.
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "PAGE CHECK: {line}");
    let _ = out.flush();
}

fn check(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    // Asked before told: AppKit answers a request for the policy it is in already with `false`.
    if app.activationPolicy() != NSApplicationActivationPolicy::Accessory {
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
    app.finishLaunching();
    // From here the application object is this one, as wxWidgets' is in the application once it is
    // up: what `activate_self` and the hand-over ask for (`own::app`).
    super::own::start();
    observe();
    let info = NSProcessInfo::processInfo();
    say(&format!("{} on {}", info.operatingSystemVersionString(), std::env::consts::ARCH));
    // What the launch set off arrives before anything is read.
    settle(&app);
    let me = std::process::id() as i32;
    let before = frontmost();
    say(&format!("before | frontmost: {}; this application active: {}", describe(before), yes(app.isActive())));
    let target = before.filter(|&pid| pid != me);

    let panel = panel(mtm, "page-check: activating panel", true);
    show(&app, &panel, "activating show (activateIgnoringOtherApps:, makeKeyAndOrderFront, orderFrontRegardless)");
    hand_back(&app, &panel, target, Over::Leaving(Handing::Deactivate));
    show(&app, &panel, "shown again");
    hand_back(&app, &panel, target, Over::Leaving(Handing::Yield));
    show(&app, &panel, "shown for host.window.focus");
    hand_back(&app, &panel, target, Over::Focus);
    nonactivating_panel(&app, mtm);
    policy_switch(&app, &panel);
}

/// A plain panel set up as a page's will be; `activating: false` adds the style that keeps this
/// application from becoming the active one when the panel takes the keyboard.
fn panel(mtm: MainThreadMarker, title: &str, activating: bool) -> Retained<NSPanel> {
    let mut style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Resizable
        | NSWindowStyleMask::UtilityWindow;
    if !activating {
        style |= NSWindowStyleMask::NonactivatingPanel;
    }
    let rect = NSRect::new(NSPoint::new(120.0, 120.0), NSSize::new(520.0, 640.0));
    // `defer: NO`, so that the window server numbers it at once, as a page's will be.
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: the panel is held by its `Retained` for as long as it is used, and it is never closed,
    // only ordered out — so AppKit must not release it as well.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setTitle(&NSString::from_str(title));
    panel.setHidesOnDeactivate(false);
    panel.setFloatingPanel(true);
    panel.setBecomesKeyOnlyIfNeeded(false);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    panel
}

/// The panel shown the way a page with `focus` will be — the floating level, this application asked
/// for the front (`activate_self`), the panel made key and ordered in front — and what came of it.
fn show(app: &NSApplication, panel: &NSPanel, what: &str) {
    let from = mark();
    let t0 = Instant::now();
    panel.setLevel(NSFloatingWindowLevel);
    super::app::activate_self();
    panel.makeKeyAndOrderFront(None);
    panel.orderFrontRegardless();
    let n = panel.windowNumber();
    let me = std::process::id() as i32;
    let until = t0 + SECTION;
    let active = wait(app, from, t0, until, |s| s == Seen::Active);
    let key = wait(app, from, t0, until, |s| s == Seen::Key(n));
    let named = wait(app, from, t0, until, |s| s == Seen::Front(me));
    say(&format!(
        "{what} | this application active {}; the panel key {}; the workspace named this application {}; \
         frontmost now: {}",
        after(active),
        after(key),
        after(named),
        describe(frontmost())
    ));
}

/// The front handed to `target` through the application's own hand-over with no window named, as
/// `over` says, and what came of it: the request's answer, when the workspace named the target and
/// this application stopped being the active one, every activation the workspace announced in the
/// meantime, in order, and how the focus the hand-over waited for ended — made at the target's
/// activation, or given up at another application's coming first.
///
/// A page hands the front back only while it has the keyboard, so a hand-back is measured as one
/// only then. When the show before it left the panel without the keyboard but this application the
/// active one, the same way is measured with nothing of ours leaving, and the line says so; when it
/// did not make this application the active one at all, no way can be, and the line says that.
fn hand_back(app: &NSApplication, panel: &NSPanel, target: Option<i32>, over: Over) {
    let (label, how) = match over {
        Over::Leaving(Handing::Deactivate) => ("hand back (a)", Handing::Deactivate),
        Over::Leaving(Handing::Yield) => ("hand back (b)", Handing::Yield),
        Over::Focus => ("hand over with nothing of ours leaving (host.window.focus)", front::HANDING),
    };
    let Some(pid) = target else {
        panel.orderOut(None);
        say(&format!("{label} | nothing to hand the front to: no other application was in front before"));
        return;
    };
    let (key, active) = (panel.isKeyWindow(), app.isActive());
    let had = format!("the panel key {}, this application active {}", yes(key), yes(active));
    let (leaving, measured_as) = match over {
        Over::Leaving(_) if key && active => (Some(&**panel), ""),
        Over::Leaving(_) if active => (None, "measured with nothing of ours leaving, since the panel did not have the keyboard; "),
        Over::Leaving(_) => {
            panel.orderOut(None);
            say(&format!(
                "{label} | not measured: the show before it did not make this application the active one, and \
                 only the active one can hand the front back ({had})"
            ));
            return;
        }
        Over::Focus => (None, ""),
    };
    let _ = front::take_ended();
    let from = mark();
    let t0 = Instant::now();
    let asked = match front::hand_front_to(pid, 0, leaving, how) {
        Handed::Kept(why) => format!("not handed over: {why}"),
        Handed::AtOnce(_) => "not asked: it was the active application already".to_string(),
        Handed::Asked { how, taken, yield_refused } => format!(
            "asked {}{}: {}",
            if yield_refused { "(b) first: REFUSED, then " } else { "" },
            how.word(),
            if taken { "taken" } else { "REFUSED" }
        ),
    };
    let until = t0 + SECTION;
    let named = wait(app, from, t0, until, |s| s == Seen::Front(pid));
    let resigned = wait(app, from, t0, until, |s| s == Seen::Resigned);
    // Once more round the loop, so that every observer of the same notice has had it.
    pump(app, Duration::from_millis(20));
    if leaving.is_none() {
        panel.orderOut(None);
    }
    let ended = match (front::take_ended(), front::waiting_for()) {
        (Some(Ended::Made), _) => "made at its activation".to_string(),
        (Some(Ended::WentTo(p)), _) => format!("given up, {} became the active one first", describe(Some(p))),
        (Some(Ended::Typed), _) => "given up, a key was pressed".to_string(),
        (Some(Ended::GivenUp), _) => "given up before an activation came".to_string(),
        (None, Some(p)) if p == pid => "still waiting".to_string(),
        (None, Some(_)) => "waiting for another application".to_string(),
        (None, None) => "none, nothing waited".to_string(),
    };
    let order = SEEN.with(|s| {
        s.borrow()
            .iter()
            .skip(from)
            .filter_map(|(at, w)| match *w {
                Seen::Front(p) => Some(format!(
                    "{} after {:.0} ms",
                    describe(Some(p)),
                    at.saturating_duration_since(t0).as_secs_f64() * 1000.0
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    say(&format!(
        "{label} | before: {had}; {measured_as}{asked}; the workspace named {} {}; this application resigned {}; \
         activations announced, in order: {}; the focus it waited for: {ended}; frontmost now: {}",
        describe(Some(pid)),
        after(named),
        after(resigned),
        if order.is_empty() { "none".to_string() } else { order.join(", ") },
        describe(frontmost())
    ));
}

/// A panel that does not activate this application, made key with nothing else asked — the
/// experimental way the setting will offer — and what came of it.
fn nonactivating_panel(app: &NSApplication, mtm: MainThreadMarker) {
    let panel = panel(mtm, "page-check: non-activating panel", false);
    let before = frontmost();
    let from = mark();
    let t0 = Instant::now();
    panel.setLevel(NSFloatingWindowLevel);
    panel.makeKeyAndOrderFront(None);
    let n = panel.windowNumber();
    let key = wait(app, from, t0, t0 + SECTION, |s| s == Seen::Key(n));
    let became = SEEN.with(|s| s.borrow().iter().skip(from).any(|(_, w)| *w == Seen::Active));
    say(&format!(
        "non-activating panel (makeKeyAndOrderFront alone) | frontmost before: {}; the panel key {} (isKeyWindow \
         {}); this application active {}{}; frontmost now: {}",
        describe(before),
        after(key),
        yes(panel.isKeyWindow()),
        yes(app.isActive()),
        if became { ", having become so meanwhile" } else { "" },
        describe(frontmost())
    ));
    panel.orderOut(None);
}

/// The activation policy switched to regular and back while the panel is shown, as an app page's
/// Dock claim will switch it: whether the panel is still shown after each.
fn policy_switch(app: &NSApplication, panel: &NSPanel) {
    show(app, panel, "shown for the policy switch");
    let state = || {
        format!(
            "the panel visible {}, key {}, this application active {}",
            yes(panel.isVisible()),
            yes(panel.isKeyWindow()),
            yes(app.isActive())
        )
    };
    let regular = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    settle(app);
    let as_regular = state();
    let accessory = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    settle(app);
    say(&format!(
        "policy switch with the panel shown | regular: {} — {as_regular}; accessory again: {} — {}",
        if regular { "accepted" } else { "REFUSED" },
        if accessory { "accepted" } else { "REFUSED" },
        state()
    ));
    panel.orderOut(None);
}

/// Subscribes to the notices the sections measure: this application becoming and stopping being
/// the active one and a window of ours becoming key, where AppKit posts them, and the workspace's
/// activations, on the main queue as `watch.rs` takes them.
fn observe() {
    let centre = NSNotificationCenter::defaultCenter();
    // SAFETY: framework-owned constants that live for the process.
    let names = unsafe {
        [
            (NSApplicationDidBecomeActiveNotification, Some(Seen::Active)),
            (NSApplicationDidResignActiveNotification, Some(Seen::Resigned)),
            (NSWindowDidBecomeKeyNotification, None),
        ]
    };
    for (name, seen) in names {
        let block = RcBlock::new(move |n: NonNull<NSNotification>| {
            // An unwind out of a block AppKit calls would tear through frames that cannot handle it.
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: a live notification for the duration of the call.
                let n = unsafe { n.as_ref() };
                let what = seen.or_else(|| {
                    let object = n.object()?;
                    Some(Seen::Key(object.downcast_ref::<NSWindow>()?.windowNumber()))
                });
                if let Some(what) = what {
                    note(what);
                }
            }));
            if caught.is_err() {
                say("a panic while noting a notice");
            }
        });
        // SAFETY: the block is copied by the centre; no queue, so it runs where AppKit posts.
        let token = unsafe { centre.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
        // For the life of the process, which is the check's.
        core::mem::forget(token);
    }
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    let block = RcBlock::new(|n: NonNull<NSNotification>| {
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: a live notification for the duration of the call.
            if let Some(pid) = activated_pid(unsafe { n.as_ref() }) {
                note(Seen::Front(pid));
            }
        }));
        if caught.is_err() {
            say("a panic while noting an activation");
        }
    });
    // SAFETY: the block captures nothing and runs on the main queue, whose thread the check is on.
    let token = unsafe {
        workspace.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    core::mem::forget(token);
}

/// The process the workspace's activation notice names.
fn activated_pid(n: &NSNotification) -> Option<i32> {
    let info = n.userInfo()?;
    // SAFETY: a framework constant that lives for the process.
    let key: &NSString = unsafe { NSWorkspaceApplicationKey };
    let object = info.objectForKey(key)?;
    Some(object.downcast_ref::<NSRunningApplication>()?.processIdentifier())
}

fn note(what: Seen) {
    SEEN.with(|s| s.borrow_mut().push((Instant::now(), what)));
}

/// Where the notices from now on start.
fn mark() -> usize {
    SEEN.with(|s| s.borrow().len())
}

/// Runs the event loop until a notice since `from` is one `want` takes — how long after `t0` it
/// came — or until `until`, which is `None`.
fn wait(app: &NSApplication, from: usize, t0: Instant, until: Instant, want: impl Fn(Seen) -> bool) -> Option<Duration> {
    loop {
        let found = SEEN.with(|s| s.borrow().iter().skip(from).find(|(_, w)| want(*w)).map(|(at, _)| *at));
        if let Some(at) = found {
            return Some(at.saturating_duration_since(t0));
        }
        if Instant::now() >= until {
            return None;
        }
        pump(app, Duration::from_millis(20));
    }
}

/// The event loop for [`SETTLE`].
fn settle(app: &NSApplication) {
    let until = Instant::now() + SETTLE;
    while Instant::now() < until {
        pump(app, Duration::from_millis(20));
    }
}

/// The event loop for at most `slice`: every event that comes is dispatched, and the main queue,
/// which the workspace's notices arrive on, is served by the run loop meanwhile.
fn pump(app: &NSApplication, slice: Duration) {
    autoreleasepool(|_| {
        let until = NSDate::dateWithTimeIntervalSinceNow(slice.as_secs_f64());
        // SAFETY: a framework constant that lives for the process.
        let mode = unsafe { NSDefaultRunLoopMode };
        while let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&until), mode, true) {
            app.sendEvent(&event);
        }
        app.updateWindows();
    });
}

/// The process the workspace names the active application.
fn frontmost() -> Option<i32> {
    NSWorkspace::sharedWorkspace().frontmostApplication().map(|a| a.processIdentifier())
}

/// A process, by its name and pid.
fn describe(pid: Option<i32>) -> String {
    let Some(pid) = pid else { return "none".to_string() };
    if pid == std::process::id() as i32 {
        return format!("this application (pid {pid})");
    }
    let name = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)
        .and_then(|a| a.localizedName())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string());
    format!("{name} (pid {pid})")
}

/// When a notice came, or that it did not within the section.
fn after(d: Option<Duration>) -> String {
    match d {
        Some(d) => format!("after {:.0} ms", d.as_secs_f64() * 1000.0),
        None => format!("not within {} s", SECTION.as_secs()),
    }
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
