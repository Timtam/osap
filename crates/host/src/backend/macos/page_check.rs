//! `automation-platform page-check`: how this application takes the front and gives it back,
//! measured on the Mac it runs on (TODO.md, MW5), and the Mac's page window built and checked
//! (`page.rs`; TODO.md, stage 4).
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
//! hand the front back to — so every line of those sections is data.
//!
//! **The web view** (`web_view`, after them): the pages' rule list compiled; a tool page built from
//! a small page the executable carries (`tools/page-check`), with no activation asked for by the
//! build — counted in `NSApplication` itself, whose two activation calls are wrapped for the rest
//! of this process (`count_activations`) — and loaded; the page's own script running and an inline
//! one not; a message's round trip and twenty in order; its own file fetched and one outside
//! blocked; a navigation away refused; `window.close()` doing nothing; and, as data, what
//! `platform.post` makes of a function and a symbol (`odd_posts`). Then, shown with the
//! keyboard under a main menu with what wxWidgets' Apple menu has, whose items only count:
//! Command-A and Command-C copying its field's text, Command-V pasting, Return on its button giving
//! one message, Command-Q, Command-H and Option-Command-H doing nothing, and Command-W hiding it —
//! the keys made in-process and sent through the application as the window server's would be —
//! and before the keys a click posted into its button as `host.input.click`'s `via = "post"` posts
//! one (`posted_clicks`), checked by where the page says it was pressed, or `skipped` on a runner
//! where this application may not post events; and, before it gets the keyboard, two such clicks as
//! data (`posted_unkeyed`). Then
//! a tool page described `activates = false` (`nonactivating_page`): no activation asked for by its
//! build or by its show with the keyboard, the show answered `true` exactly when its window is key
//! at the call, the same keys while it has the keyboard, and, where the workspace still names the
//! application that was in front, the front back with nothing left waiting for an activation that
//! application never announces; then, as data, the same page beside a tool page that activates the
//! application (`beside_a_page`). Last, the rule list on its own
//! (`rule_list_alone`), since the host's policy blocks that outside fetch before a rule list is
//! asked: a page built with one rule more, which blocks a file of its own the policy lets it fetch,
//! must not get that file. These are `check` lines, `ok` or `FAILED`: what the host promises,
//! whatever the desktop. Where the page does not get the keyboard on a runner the keys are not
//! checked, and a line says so. The rest is data: the times, what the page says of its context
//! (`isSecureContext`, the clipboard, WebRTC), the classes AppKit made the first responder, an app
//! page's Dock claim and keys, where the keyboard goes with the non-activating page beside another
//! page, and the folders WebKit made under `~/Library`.
//!
//! Exit status 0 when it ran to the end and no check failed, 3 when it ran to the end and a check
//! failed, 1 when it could not run, 2 for a wrong argument. Every line starts with `PAGE CHECK:`;
//! nothing goes into the application's log, which is never opened here.
//!
//! The waits are a test's, not the application's: a section waits for the notices it measures for
//! at most [`SECTION`] all together, and the activation policy, which announces nothing, is read
//! after [`SETTLE`]. Four shows, three hand-overs and the non-activating panel, five seconds each at
//! the most, and three settles bound the first part to about 42 seconds; each of the web view's
//! waits is bounded the same way, and on a Mac where everything answers they take a few seconds.

use std::cell::RefCell;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, Bool, Imp, NSObject, Sel};
use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSBackingStoreType, NSEvent, NSEventMask, NSEventModifierFlags,
    NSEventType, NSFloatingWindowLevel, NSMenu, NSMenuItem, NSPanel, NSPasteboard, NSPasteboardTypeString,
    NSRunningApplication, NSWindow, NSWindowCollectionBehavior, NSWindowDidBecomeKeyNotification, NSWindowStyleMask,
    NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{
    NSBundle, NSDate, NSDefaultRunLoopMode, NSError, NSNotification, NSNotificationCenter, NSOperationQueue, NSPoint,
    NSProcessInfo, NSRect, NSSize, NSString, NSURL,
};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{CGPreflightPostEventAccess, CGWarpMouseCursorPosition};
use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore};

use super::front::{self, Ended, Handed};
use super::page::{self, EnvNow};
use crate::backend::{Handing, MouseButton};
use crate::errors::input::COVERED;
use crate::webview::{self, PageEvent, WindowKind};

const USAGE: &str = "\
automation-platform page-check

Measures how this application takes the front and gives it back on this Mac: a plain panel shown
the way a page will be, the front handed back both ways to the application that was in front
before (start one first, TextEdit for instance) and over once more the way host.window.focus hands
it, a panel that does not activate the application, and the activation policy switched while a
panel is shown. Then builds pages with the page window host.gui.webview uses and checks what the
host promises of them: the rule list, also on its own, the page's own script and no inline one,
messages both ways (and, as data, what platform.post makes of a function and a symbol), its own
file and nothing outside, no navigation away, and the keys in a page with the keyboard (Command-A,
C and V in a field, Return on a button, Command-Q and Command-H doing
nothing in a tool page, Command-W hiding it, and a click posted into its button as host.input.click
with via = \"post\" posts one, landing where it was asked for), also in a tool page described
activates = false, which must ask for no activation and answer its show at the call, and which it
shows beside another tool page as well; it sets the pasteboard and moves the pointer while it does.
Opens windows of its own for some seconds, loads no module and writes nothing to the application's
log; every line it prints starts with PAGE CHECK:, and a check that fails says FAILED, one it cannot
make here says skipped. Exit status 0 when it ran to the end and no check failed, 3 when one failed,
1 when it could not run, 2 for a wrong argument.";

/// How long a section waits for the notices it measures: a bound on the test, never a decision
/// of the application's.
const SECTION: Duration = Duration::from_secs(5);

/// How long the run's first page may take to load: it starts WebKit's own processes, which on a
/// cold Intel runner took longer than a section (run 39035233368: every later page there loaded
/// in 0.3 to 1 s). A bound on the test as `SECTION` is; the time it took is printed either way.
const FIRST_LOAD: Duration = Duration::from_secs(20);

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
    let tally = autoreleasepool(|_| check(mtm));
    say(&format!("done; {} of the web view's {} checks failed", tally.failed, tally.run));
    if tally.failed > 0 {
        3
    } else {
        0
    }
}

/// One line of output, written at once.
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "PAGE CHECK: {line}");
    let _ = out.flush();
}

fn check(mtm: MainThreadMarker) -> Tally {
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
    let mut tally = Tally::default();
    web_view(&app, mtm, &mut tally);
    tally
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

/// A panel that does not activate this application, made key with nothing else asked — the way a
/// page described `activates = false` takes the keyboard — and what came of it.
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

/// The module id the page is served under, for its scheme (`webview::scheme_for`).
const PAGE_MODULE: &str = "com.platform.page-check";

/// The page the web view sections build, carried in the executable — so that a Mac with only the
/// download runs the whole check, a module's developer's included — and written to a folder of its
/// own when the check runs.
const PAGE_FILES: [(&str, &str); 4] = [
    ("ui/index.html", include_str!("../../../../../tools/page-check/ui/index.html")),
    ("ui/page.js", include_str!("../../../../../tools/page-check/ui/page.js")),
    ("ui/data.json", include_str!("../../../../../tools/page-check/ui/data.json")),
    ("ui/rules-only.json", include_str!("../../../../../tools/page-check/ui/rules-only.json")),
];

/// The rule [`rule_list_alone`] adds to the pages' rule list: `ui/rules-only.json`, a file of the
/// page's own, blocked. Last, so the rules before it that let the page's own files past do not
/// undo it. JSON, whose `\\` is the regular expression's `\`.
const RULES_ONLY_RULE: &str = r#"{"trigger":{"url-filter":"/ui/rules-only\\.json"},"action":{"type":"block"}}"#;

/// The web view's checks: how many ran, and how many failed.
#[derive(Default)]
struct Tally {
    run: u32,
    failed: u32,
}

impl Tally {
    /// One check's line: `ok` with what was seen, or `FAILED` with why. Such a line fails the CI leg.
    fn check(&mut self, name: &str, result: Result<String, String>) {
        self.run += 1;
        match result {
            Ok(seen) if seen.is_empty() => say(&format!("check {name}: ok")),
            Ok(seen) => say(&format!("check {name}: ok — {seen}")),
            Err(why) => {
                self.failed += 1;
                say(&format!("check {name}: FAILED — {why}"));
            }
        }
    }
}

thread_local! {
    /// Everything the pages sent since the web view sections began, with when it was taken from the
    /// page window's queue.
    static INBOX: RefCell<Vec<(Instant, PageEvent)>> = const { RefCell::new(Vec::new()) };
}

/// How often the activation was asked for, counted in `NSApplication` itself ([`count_activations`]).
static ACTIVATIONS: AtomicU32 = AtomicU32::new(0);
/// The counting is in place; never twice, or a counter would call itself.
static COUNTING: AtomicBool = AtomicBool::new(false);
static ORIGINAL_ACTIVATE: OnceLock<Imp> = OnceLock::new();
static ORIGINAL_IGNORING: OnceLock<Imp> = OnceLock::new();

/// The counts of the menu items a wxWidgets-like Apple menu has ([`MenuCounter`]): Quit, Hide and
/// Hide Others.
static MENU_QUIT: AtomicU32 = AtomicU32::new(0);
static MENU_HIDE: AtomicU32 = AtomicU32::new(0);
static MENU_HIDE_OTHERS: AtomicU32 = AtomicU32::new(0);

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and the class does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AutomationPlatformPageCheckMenuCounter"]
    struct MenuCounter;

    impl MenuCounter {
        #[unsafe(method(countQuit:))]
        fn count_quit(&self, _sender: Option<&AnyObject>) {
            MENU_QUIT.fetch_add(1, Ordering::Relaxed);
        }

        #[unsafe(method(countHide:))]
        fn count_hide(&self, _sender: Option<&AnyObject>) {
            MENU_HIDE.fetch_add(1, Ordering::Relaxed);
        }

        #[unsafe(method(countHideOthers:))]
        fn count_hide_others(&self, _sender: Option<&AnyObject>) {
            MENU_HIDE_OTHERS.fetch_add(1, Ordering::Relaxed);
        }
    }
);

/// `-[NSApplication activate]`, counted, then itself.
unsafe extern "C-unwind" fn counted_activate(this: *mut AnyObject, cmd: Sel) {
    ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
    if let Some(&imp) = ORIGINAL_ACTIVATE.get() {
        // SAFETY: the method's own implementation, called with its own signature.
        let original: unsafe extern "C-unwind" fn(*mut AnyObject, Sel) = unsafe { std::mem::transmute(imp) };
        unsafe { original(this, cmd) };
    }
}

/// `-[NSApplication activateIgnoringOtherApps:]`, counted, then itself.
unsafe extern "C-unwind" fn counted_ignoring(this: *mut AnyObject, cmd: Sel, flag: Bool) {
    ACTIVATIONS.fetch_add(1, Ordering::Relaxed);
    if let Some(&imp) = ORIGINAL_IGNORING.get() {
        // SAFETY: the method's own implementation, called with its own signature.
        let original: unsafe extern "C-unwind" fn(*mut AnyObject, Sel, Bool) = unsafe { std::mem::transmute(imp) };
        unsafe { original(this, cmd, flag) };
    }
}

/// Puts a counter in front of `NSApplication`'s two activation calls — `activate` from macOS 14,
/// `activateIgnoringOtherApps:` everywhere — for the rest of this process, which is the check's: a
/// build of a page that asked for the front would be counted, not inferred from where the front
/// went. In this subcommand only. Whether the counting is in place.
fn count_activations() -> bool {
    if COUNTING.swap(true, Ordering::SeqCst) {
        return true;
    }
    let class = NSApplication::class();
    let Some(ignoring) = class.instance_method(sel!(activateIgnoringOtherApps:)) else {
        return false;
    };
    // SAFETY: the counter has the method's signature and calls the implementation it replaces.
    unsafe {
        let counter: Imp = std::mem::transmute(counted_ignoring as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, Bool));
        let _ = ORIGINAL_IGNORING.set(ignoring.set_implementation(counter));
        if let Some(activate) = class.instance_method(sel!(activate)) {
            let counter: Imp = std::mem::transmute(counted_activate as unsafe extern "C-unwind" fn(*mut AnyObject, Sel));
            let _ = ORIGINAL_ACTIVATE.set(activate.set_implementation(counter));
        }
    }
    true
}

/// A main menu with what wxWidgets' Apple menu has where a page might meet it — Hide (Command-H),
/// Hide Others (Option-Command-H) and Quit (Command-Q) — whose items only count. The counter it
/// targets is returned: a menu item does not keep its target.
fn install_menu(app: &NSApplication, mtm: MainThreadMarker) -> Retained<MenuCounter> {
    let this = MenuCounter::alloc(mtm).set_ivars(());
    // SAFETY: NSObject's initialiser.
    let counter: Retained<MenuCounter> = unsafe { msg_send![super(this), init] };
    let bar = NSMenu::new(mtm);
    let apple = NSMenu::new(mtm);
    let items = [
        ("Hide", sel!(countHide:), NSEventModifierFlags::Command),
        ("Hide Others", sel!(countHideOthers:), NSEventModifierFlags::Command | NSEventModifierFlags::Option),
        ("Quit", sel!(countQuit:), NSEventModifierFlags::Command),
    ];
    for (title, action, modifiers) in items {
        let key = if title == "Quit" { "q" } else { "h" };
        // SAFETY: an item of ours, with a selector its target answers.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(action),
                &NSString::from_str(key),
            )
        };
        item.setKeyEquivalentModifierMask(modifiers);
        // SAFETY: the counter lives for the section that uses the menu.
        unsafe { item.setTarget(Some(&counter)) };
        apple.addItem(&item);
    }
    let top = NSMenuItem::new(mtm);
    top.setSubmenu(Some(&apple));
    bar.addItem(&top);
    app.setMainMenu(Some(&bar));
    counter
}

/// The page's files, written to a fresh folder of this run's.
fn write_page() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!("automation-platform-page-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (rel, text) in PAGE_FILES {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    root.canonicalize().map_err(|e| format!("{}: {e}", root.display()))
}

/// The folders WebKit is reported to make under `~/Library` for an application — its data and its
/// caches — with a store that keeps nothing as well (TODO.md, MW): looked at before and after.
fn library_folders() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else { return Vec::new() };
    let id = NSBundle::mainBundle().bundleIdentifier().map(|s| s.to_string()).unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default()
    });
    vec![home.join("Library/WebKit").join(&id), home.join("Library/Caches").join(&id).join("WebKit")]
}

fn folders_line(folders: &[PathBuf]) -> String {
    folders
        .iter()
        .map(|f| format!("{} {}", f.display(), if f.exists() { "exists" } else { "absent" }))
        .collect::<Vec<_>>()
        .join("; ")
}

/// What the page window sent since the last call, kept.
fn drain() {
    let now = Instant::now();
    let events = page::take_events();
    INBOX.with(|i| i.borrow_mut().extend(events.into_iter().map(|e| (now, e))));
}

/// Where the page's events from now on start.
fn inbox_mark() -> usize {
    drain();
    INBOX.with(|i| i.borrow().len())
}

/// A message's text, for the plain strings the page posts.
fn plain(text: &str) -> Option<&str> {
    text.strip_prefix('"')?.strip_suffix('"').filter(|t| !t.contains('\\'))
}

/// The messages of page `page` since `from`, as text.
fn messages(page: u64, from: usize) -> Vec<String> {
    drain();
    INBOX.with(|i| {
        i.borrow()
            .iter()
            .skip(from)
            .filter_map(|(_, e)| match e {
                PageEvent::Message { page: p, text, .. } if *p == page => plain(text).map(str::to_string),
                _ => None,
            })
            .collect()
    })
}

/// Runs the event loop until an event of the page window's since `from` is one `want` takes — when
/// it was taken, and the event — or until `until`, which is `None`.
fn wait_page(app: &NSApplication, from: usize, until: Instant, want: impl Fn(&PageEvent) -> bool) -> Option<(Instant, PageEvent)> {
    loop {
        drain();
        let found = INBOX.with(|i| i.borrow().iter().skip(from).find(|(_, e)| want(e)).cloned());
        if found.is_some() {
            return found;
        }
        if Instant::now() >= until {
            return None;
        }
        pump(app, Duration::from_millis(20));
    }
}

/// Runs the event loop until page `page` has sent a message since `from` that `want` takes.
fn wait_message(app: &NSApplication, page: u64, from: usize, until: Instant, want: impl Fn(&str) -> bool) -> Option<(Instant, String)> {
    wait_page(app, from, until, |e| match e {
        PageEvent::Message { page: p, text, .. } => *p == page && plain(text).is_some_and(&want),
        _ => false,
    })
    .and_then(|(at, e)| match e {
        PageEvent::Message { text, .. } => plain(&text).map(|t| (at, t.to_string())),
        _ => None,
    })
}

/// Hands the page one command, as a plain string.
fn tell(native: &page::Native, command: &str) {
    if let Err(e) = native.post_json(&format!("\"{command}\"")) {
        say(&format!("web view | the command '{command}' could not be posted: {e}"));
    }
}

/// Hands page `page` one command and waits for its answer, the first message starting with `prefix`.
fn ask(app: &NSApplication, native: &page::Native, page: u64, command: &str, prefix: &str) -> Option<String> {
    let from = inbox_mark();
    let t = Instant::now();
    tell(native, command);
    wait_message(app, page, from, t + SECTION, |m| m.starts_with(prefix)).map(|(_, m)| m)
}

/// A round trip: `echo n` posted, and its echo waited for; how long it took.
fn round_trip(app: &NSApplication, native: &page::Native, page: u64, n: u32) -> Option<Duration> {
    let from = inbox_mark();
    let t0 = Instant::now();
    tell(native, &format!("echo {n}"));
    let want = format!("echo {n}");
    wait_message(app, page, from, t0 + SECTION, |m| m == want).map(|(at, _)| at.saturating_duration_since(t0))
}

/// A key pressed and let go in `window`, made in-process and sent through the application as the
/// window server's would be: a key equivalent goes to the key window first, then to the main menu.
fn press(app: &NSApplication, window: &NSWindow, chars: &str, code: u16, modifiers: NSEventModifierFlags) {
    let at = NSProcessInfo::processInfo().systemUptime();
    let text = NSString::from_str(chars);
    for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            modifiers,
            at,
            window.windowNumber(),
            None,
            &text,
            &text,
            false,
            code,
        );
        match event {
            Some(event) => app.sendEvent(&event),
            None => say(&format!("web view | AppKit made no key event for '{chars}'")),
        }
    }
}

/// The pasteboard's text, and how often it has changed.
fn pasteboard() -> (Option<String>, isize) {
    let board = NSPasteboard::generalPasteboard();
    // SAFETY: a framework constant that lives for the process.
    let kind = unsafe { NSPasteboardTypeString };
    (board.stringForType(kind).map(|s| s.to_string()), board.changeCount())
}

fn set_pasteboard(text: &str) {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    // SAFETY: a framework constant that lives for the process.
    let kind = unsafe { NSPasteboardTypeString };
    board.setString_forType(&NSString::from_str(text), kind);
}

/// Builds a page of `kind` as page `n` with the rule list, and checks that it was built without an
/// activation being asked for; the page, if it was built. `name` is the page's in the lines;
/// `activates` the page option.
#[allow(clippy::too_many_arguments)]
fn build_page(
    tally: &mut Tally,
    rules: &WKContentRuleList,
    root: &Path,
    kind: WindowKind,
    activates: bool,
    name: &str,
    n: u64,
    counting: bool,
) -> Option<page::Native> {
    let scheme = webview::scheme_for(PAGE_MODULE);
    let spec = page::BuildSpec {
        page: n,
        build: n,
        kind,
        activates,
        title: format!("page-check: {name} page"),
        scheme: scheme.clone(),
        root: root.to_path_buf(),
        entry_url: webview::entry_url(&scheme, "ui/index.html"),
    };
    let before = ACTIVATIONS.load(Ordering::Relaxed);
    let t0 = Instant::now();
    let built = page::build(&spec, rules);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let asked = ACTIVATIONS.load(Ordering::Relaxed) - before;
    tally.check(
        &format!("the {name} page is built"),
        built.as_ref().map(|_| format!("in {ms:.0} ms, which the event loop waits for")).map_err(Clone::clone),
    );
    if built.is_ok() {
        tally.check(
            &format!("building the {name} page asked for no activation"),
            match (counting, asked) {
                (false, _) => Err("the activation calls could not be counted".to_string()),
                (true, 0) => Ok("none counted".to_string()),
                (true, n) => Err(format!("{n} asked for")),
            },
        );
    }
    built.ok()
}

/// The page loaded: its document's own report, waited for.
fn wait_loaded(app: &NSApplication, tally: &mut Tally, page: u64, from: usize, t0: Instant, name: &str) -> bool {
    wait_loaded_within(app, tally, page, from, t0, name, SECTION)
}

/// `wait_loaded` with a bound of its own, for the run's first page (`FIRST_LOAD`).
fn wait_loaded_within(
    app: &NSApplication,
    tally: &mut Tally,
    page: u64,
    from: usize,
    t0: Instant,
    name: &str,
    bound: Duration,
) -> bool {
    let ended = wait_page(app, from, t0 + bound, |e| match e {
        PageEvent::Loaded { page: p, .. } | PageEvent::LoadFailed { page: p, .. } | PageEvent::Closed { page: p, .. } => *p == page,
        PageEvent::Message { .. } => false,
    });
    let result = match ended {
        Some((at, PageEvent::Loaded { .. })) => Ok(format!("after {:.0} ms", at.saturating_duration_since(t0).as_secs_f64() * 1000.0)),
        Some((_, PageEvent::LoadFailed { why, .. })) => Err(format!("its document did not load: {why}")),
        Some((_, PageEvent::Closed { why, .. })) => Err(format!("the page was closed ({why}) before it loaded")),
        _ => Err(format!("not within {} s", bound.as_secs())),
    };
    let ok = result.is_ok();
    tally.check(&format!("the {name} page loads"), result);
    ok
}

/// The web view's sections: the rule list, a tool page built, loaded and spoken to, its keys, an app
/// page, a tool page that does not activate the application, and the rule list on its own. Hard
/// checks are `check` lines; everything else is data.
fn web_view(app: &NSApplication, mtm: MainThreadMarker, tally: &mut Tally) {
    say(&format!(
        "web view | {}",
        page::runtime_version().unwrap_or_else(|| "WebKit's version could not be read".to_string())
    ));
    let library = library_folders();
    let library_before = folders_line(&library);
    let t0 = Instant::now();
    let rules = loop {
        match page::environment() {
            EnvNow::Ready(list, _) => break Ok(list),
            EnvNow::Failed(why) => break Err(why),
            EnvNow::Waiting if t0.elapsed() >= SECTION => break Err(format!("not compiled within {} s", SECTION.as_secs())),
            EnvNow::Waiting => pump(app, Duration::from_millis(20)),
        }
    };
    tally.check(
        "the rule list compiles",
        rules.as_ref().map(|_| format!("after {:.0} ms", t0.elapsed().as_secs_f64() * 1000.0)).map_err(Clone::clone),
    );
    let Ok(rules) = rules else { return };
    let root = match write_page() {
        Ok(root) => root,
        Err(why) => {
            tally.check("the page's files are written", Err(why));
            return;
        }
    };
    let counting = count_activations();
    let from = inbox_mark();
    let t0 = Instant::now();
    if let Some(tool) = build_page(tally, &rules, &root, WindowKind::Tool, true, "tool", 1, counting) {
        if wait_loaded_within(app, tally, 1, from, t0, "tool", FIRST_LOAD) {
            tool_page(app, mtm, tally, &tool, from, t0);
        }
        drop(tool);
    }
    let from = inbox_mark();
    let t0 = Instant::now();
    if let Some(page) = build_page(tally, &rules, &root, WindowKind::App, true, "app", 2, counting) {
        if wait_loaded(app, tally, 2, from, t0, "app") {
            app_page(app, &page);
        }
        drop(page);
    }
    nonactivating_page(app, mtm, tally, &rules, &root, counting);
    rule_list_alone(app, mtm, tally, &root, counting);
    pump(app, Duration::from_millis(20));
    let _ = std::fs::remove_dir_all(&root);
    say(&format!("web view | ~/Library before: {library_before}; after: {}", folders_line(&library)));
}

/// The rule list on its own. The tool page's fetch of `https://example.com` is blocked by the host's
/// policy (`connect-src 'self'`) before any rule list is asked, so it says nothing of the rule list.
/// Here a tool page is built with the pages' rule list and one rule more ([`RULES_ONLY_RULE`]),
/// which blocks a file of the page's own that the policy lets it fetch: that fetch blocked — while
/// the page's other file of its own is fetched — is the rule list's doing alone, applied to the
/// loads of a document the application's scheme handler served. Which is what the pages' own list
/// relies on for everything the policy does not cover.
fn rule_list_alone(app: &NSApplication, mtm: MainThreadMarker, tally: &mut Tally, root: &Path, counting: bool) {
    const NAME: &str = "rule-list";
    let folder = std::env::temp_dir().join(format!("automation-platform-page-check-rules-{}", std::process::id()));
    let base = webview::RULE_LIST.strip_suffix(']').unwrap_or(webview::RULE_LIST);
    let list = compile_list(app, mtm, &folder, &format!("{base},{RULES_ONLY_RULE}]"));
    tally.check(
        "the rule list with one rule more compiles",
        list.as_ref().map(|_| String::new()).map_err(Clone::clone),
    );
    if let Ok(list) = list {
        let from = inbox_mark();
        let t0 = Instant::now();
        if let Some(page) = build_page(tally, &list, root, WindowKind::Tool, true, NAME, 3, counting) {
            if wait_loaded(app, tally, 3, from, t0, NAME) {
                let own = ask(app, &page, 3, "fetch-own", "fetch-own ");
                let blocked = ask(app, &page, 3, "fetch-rules-only", "fetch-rules-only ");
                tally.check(
                    "the rule list on its own blocks a fetch the policy lets through",
                    match (own.as_deref(), blocked.as_deref()) {
                        (Some("fetch-own ok data.json"), Some(m)) if m.starts_with("fetch-rules-only blocked") => {
                            Ok(format!("{}; its own data.json fetched", m.trim_start_matches("fetch-rules-only ")))
                        }
                        (own, blocked) => Err(format!(
                            "data.json: {}; rules-only.json, which only the rule list blocks: {}",
                            own.unwrap_or("no answer"),
                            blocked.unwrap_or("no answer")
                        )),
                    },
                );
            }
            drop(page);
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
}

/// A rule list WebKit compiled, or why it did not.
type Compiled = Result<Retained<WKContentRuleList>, String>;

/// Compiles `list` into a rule store of the check's own in `folder`, the event loop running until
/// WebKit answers, for at most [`SECTION`].
fn compile_list(app: &NSApplication, mtm: MainThreadMarker, folder: &Path, list: &str) -> Compiled {
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&folder.to_string_lossy()), true);
    // SAFETY: a file URL of a folder that exists, on the main thread.
    let store = unsafe { WKContentRuleListStore::storeWithURL(Some(&url), mtm) }
        .ok_or_else(|| format!("WebKit made no rule store in {}", folder.display()))?;
    let answer: Rc<RefCell<Option<Compiled>>> = Rc::default();
    let put = answer.clone();
    let block = RcBlock::new(move |list: *mut WKContentRuleList, error: *mut NSError| {
        // An unwind out of a block WebKit calls would tear through frames that cannot handle it.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: WebKit's own objects for the duration of the call, retained if kept.
            let list = unsafe { Retained::retain(list) };
            let error = unsafe { error.as_ref() }.map(|e| e.localizedDescription().to_string());
            *put.borrow_mut() = Some(match (list, error) {
                (Some(list), None) => Ok(list),
                (_, error) => Err(error.unwrap_or_else(|| "WebKit answered with no rule list".to_string())),
            });
        }));
    });
    // SAFETY: the identifier and the list are strings of ours; the store copies the block.
    unsafe {
        store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
            Some(&NSString::from_str("page-check")),
            Some(&NSString::from_str(list)),
            Some(&block),
        );
    }
    let t0 = Instant::now();
    loop {
        if let Some(done) = answer.borrow_mut().take() {
            return done;
        }
        if t0.elapsed() >= SECTION {
            return Err(format!("not compiled within {} s", SECTION.as_secs()));
        }
        pump(app, Duration::from_millis(20));
    }
}

/// The tool page: what ran in it, what it reaches, its messages both ways, and its keys.
fn tool_page(app: &NSApplication, mtm: MainThreadMarker, tally: &mut Tally, tool: &page::Native, from: usize, t0: Instant) {
    let hello = wait_message(app, 1, from, t0 + SECTION, |m| m.starts_with("hello "));
    tally.check(
        "the page's own script runs",
        hello.as_ref().map(|_| String::new()).ok_or_else(|| "page.js said nothing: the policy may block the page's own scripts".to_string()),
    );
    if let Some((_, hello)) = &hello {
        say(&format!("web view | the page says: {}", hello.trim_start_matches("hello ")));
    }
    let not_shown = round_trip(app, tool, 1, 0);
    tally.check(
        "a message makes the round trip",
        not_shown.map(|d| format!("{:.0} ms, with the page not on screen", d.as_secs_f64() * 1000.0)).ok_or_else(|| "no echo".to_string()),
    );
    let inline = messages(1, from).iter().any(|m| m == "inline ran");
    tally.check(
        "an inline script does not run",
        if inline { Err("it ran: the policy is not applied".to_string()) } else { Ok(String::new()) },
    );

    let start = inbox_mark();
    let t = Instant::now();
    for n in 1..=20 {
        tell(tool, &format!("echo {n}"));
    }
    let last = wait_message(app, 1, start, t + SECTION, |m| m == "echo 20");
    let echoes: Vec<String> = messages(1, start).into_iter().filter(|m| m.starts_with("echo ")).collect();
    let wanted: Vec<String> = (1..=20).map(|n| format!("echo {n}")).collect();
    tally.check(
        "20 messages come back as 20, in order",
        if last.is_some() && echoes == wanted {
            Ok(format!("in {:.0} ms", t.elapsed().as_secs_f64() * 1000.0))
        } else {
            Err(format!("{} came back: {}", echoes.len(), echoes.join(", ")))
        },
    );

    let asked = |command: &str, prefix: &str| ask(app, tool, 1, command, prefix);
    let own = asked("fetch-own", "fetch-own ");
    tally.check(
        "the page fetches its own file",
        match own.as_deref() {
            Some("fetch-own ok data.json") => Ok(String::new()),
            Some(other) => Err(other.to_string()),
            None => Err("no answer".to_string()),
        },
    );
    let outside = asked("fetch-outside", "fetch-outside ");
    tally.check(
        "a fetch outside is blocked",
        match outside.as_deref() {
            Some(m) if m.starts_with("fetch-outside blocked") => Ok(m.trim_start_matches("fetch-outside ").to_string()),
            Some(other) => Err(other.to_string()),
            None => Err("no answer".to_string()),
        },
    );

    let entry = tool.url();
    let refused = tool.refused();
    tell(tool, "navigate");
    let echoed = round_trip(app, tool, 1, 30);
    tally.check(
        "a navigation to https://example.com is refused",
        if echoed.is_some() && tool.url() == entry {
            Ok(format!("still on {entry}; {} refusal(s) said", tool.refused() - refused))
        } else {
            Err(format!("the page is on '{}', echo {}", tool.url(), if echoed.is_some() { "answered" } else { "unanswered" }))
        },
    );
    let closed_from = inbox_mark();
    let called = asked("close", "close called").is_some();
    let echoed = round_trip(app, tool, 1, 31);
    let closed = INBOX.with(|i| {
        i.borrow().iter().skip(closed_from).any(|(_, e)| matches!(e, PageEvent::Closed { page: 1, .. }))
    });
    tally.check(
        "window.close() does nothing",
        if called && echoed.is_some() && !closed {
            Ok(String::new())
        } else {
            Err(format!("close called {called}, echo after it {}, closed {closed}", echoed.is_some()))
        },
    );

    // What platform.post makes of a function and a symbol: data.
    odd_posts(app, tool, 1);

    // A posted click while the page does not have the keyboard: data.
    posted_unkeyed(app, tool, 1);

    // With the keyboard: the page shown as a key would show it, beside nothing.
    let counter = install_menu(app, mtm);
    let window = tool.window();
    let n = window.windowNumber();
    let seen = mark();
    let t = Instant::now();
    tool.put(None, None);
    let shown = tool.show(true);
    let key = wait(app, seen, t, t + SECTION, |s| s == Seen::Key(n));
    say(&format!(
        "web view | the tool page shown with the keyboard: show answered {}; the window key {}; this application active {}; \
         its level {}; on top at its centre {}",
        if shown.is_ok() { "true" } else { "\"declined by the system\"" },
        after(key),
        yes(app.isActive()),
        window.level(),
        yes(tool.on_top_at_centre())
    ));
    let on_screen = round_trip(app, tool, 1, 40);
    say(&format!(
        "web view | a round trip with the page on screen: {}",
        on_screen.map(|d| format!("{:.0} ms", d.as_secs_f64() * 1000.0)).unwrap_or_else(|| "no echo".to_string())
    ));
    if window.isKeyWindow() {
        posted_clicks(app, tally, tool, 1);
        keys(app, tally, tool, 1, "");
    } else {
        say("web view | the keys and the posted clicks are not checked: the page did not get the keyboard on this Mac");
    }
    say(&format!("web view | first responders while shown: {}", {
        let r = tool.responders();
        if r.is_empty() { "none said".to_string() } else { r.join(" → ") }
    }));
    app.setMainMenu(None);
    drop(counter);
}

/// `platform.post` given a value `JSON.stringify` writes no text for — a function, a symbol: what
/// the page says it answered or threw, and what the host was handed between the page's two markers
/// around it, each text as it came and whether it is JSON — which a module's `onMessage` is handed —
/// or not, which the host drops with a line in the log. Data, not a check: what it measures is to be
/// written into gui.md (`platform.post`'s Returns; TODO.md).
fn odd_posts(app: &NSApplication, tool: &page::Native, n: u64) {
    for kind in ["function", "symbol"] {
        let from = inbox_mark();
        let t = Instant::now();
        tell(tool, &format!("post-odd {kind}"));
        let (before, after) = (format!("odd-before {kind}"), format!("odd-after {kind} "));
        let answered = wait_message(app, n, from, t + SECTION, |m| m.starts_with(&after));
        let texts: Vec<String> = INBOX.with(|i| {
            i.borrow()
                .iter()
                .skip(from)
                .filter_map(|(_, e)| match e {
                    PageEvent::Message { page: p, text, .. } if *p == n => Some(text.clone()),
                    _ => None,
                })
                .collect()
        });
        let start = texts.iter().position(|m| plain(m) == Some(before.as_str()));
        let end = texts.iter().position(|m| plain(m).is_some_and(|m| m.starts_with(&after)));
        let handed = match (start, end) {
            (Some(s), Some(e)) if s < e && e == s + 1 => "no message".to_string(),
            (Some(s), Some(e)) if s < e => texts[s + 1..e].iter().map(|m| handed_text(m)).collect::<Vec<_>>().join(", "),
            _ => "nothing that can be measured: the markers did not both come".to_string(),
        };
        say(&format!(
            "web view | platform.post given a {kind}: the page says it {}; the host was handed {handed}",
            answered.map(|(_, m)| m.trim_start_matches(after.as_str()).to_string()).unwrap_or_else(|| "said nothing".to_string())
        ));
    }
}

/// A message's text as the host was handed it, for [`odd_posts`]'s line: written out, and whether
/// a module's `onMessage` would be handed it.
fn handed_text(text: &str) -> String {
    let json = serde_json::from_str::<serde_json::Value>(text).is_ok();
    format!(
        "{:?} ({})",
        webview::escape_controls(text),
        if json { "JSON: onMessage is handed it" } else { "not JSON: dropped with a line in the log" }
    )
}

/// Where to click page `n`'s button, as the page measures it: the middle of its rectangle as a
/// point on the screen, from the top-left of the primary display, and the same point in the page —
/// its `clientX` and `clientY`. The web view fills the window's content, so the page's origin is the
/// content's top-left corner; a page that says it was pressed elsewhere says that this is wrong, or
/// that the click landed elsewhere, and the line names both points.
fn button_point(app: &NSApplication, tool: &page::Native, n: u64) -> Option<((i32, i32), (i32, i32))> {
    let m = ask(app, tool, n, "button-rect", "button-rect ")?;
    let v: Vec<f64> = m.trim_start_matches("button-rect ").split(' ').filter_map(|s| s.parse().ok()).collect();
    let [left, top, w, h] = v[..] else { return None };
    let (cx, cy) = content_top_left(tool.window())?;
    let (px, py) = ((left + w / 2.0).floor(), (top + h / 2.0).floor());
    Some((((cx + px).floor() as i32, (cy + py).floor() as i32), (px as i32, py as i32)))
}

/// The top-left corner of `window`'s content on the screen, in points from the top-left of the
/// primary display, as the host's coordinates are: AppKit's frame flipped against that display's
/// height, as the host flips it (`capture::screen_size`).
fn content_top_left(window: &NSWindow) -> Option<(f64, f64)> {
    let (_, primary) = super::capture::screen_size();
    if primary <= 0 {
        return None;
    }
    let c = window.contentRectForFrameRect(window.frame());
    Some((c.origin.x, f64::from(primary) - (c.origin.y + c.size.height)))
}

/// The presses page `n` said it saw since `from`, as `(x, y)` in the page.
fn presses(n: u64, from: usize) -> Vec<(i32, i32)> {
    messages(n, from)
        .iter()
        .filter_map(|m| {
            let mut v = m.strip_prefix("down ")?.split(' ').filter_map(|s| s.parse::<i32>().ok());
            Some((v.next()?, v.next()?))
        })
        .collect()
}

/// `host.input.click` with `via = "post"` into the tool page while it has the keyboard, through the
/// application's own path (`input::post_click` and `ax::post_target`, as the backend's
/// `mouse_click_posted` makes it): the page's button gets one press, at the point asked for, and
/// one click — with the pointer moved there first, and again with it left where it was put
/// beforehand — and the pointer is on the point, or where it was. The page's own window, so the hit
/// test answers the page, and the click carries the window's number and its place in it as one
/// into another program's window does. On a runner where this application may not post events, a
/// `skipped` line, which is neither a pass nor a failure; and where the window server names another
/// window over the page's button after all, the same.
fn posted_clicks(app: &NSApplication, tally: &mut Tally, tool: &page::Native, n: u64) {
    const NAME: &str = "a posted click reaches the page's button at the point";
    if !CGPreflightPostEventAccess() {
        say(&format!(
            "check {NAME}: skipped — post-event access is not granted on this runner (CGPreflightPostEventAccess \
             answered no), so nothing can be posted; neither a pass nor a failure"
        ));
        return;
    }
    let Some((screen, inside)) = button_point(app, tool, n) else {
        tally.check(NAME, Err("the page did not say where its button is".to_string()));
        return;
    };
    // Where the pointer is put before the posted click that leaves it: inside the page, clear of
    // the button — a few points in from its content's top-left corner.
    let aside = content_top_left(tool.window()).map(|(x, y)| CGPoint::new(x + 4.0, y + 4.0));
    let near = |p: (i32, i32), q: (i32, i32)| (p.0 - q.0).abs() <= 1 && (p.1 - q.1).abs() <= 1;
    for (move_pointer, which) in [(true, ""), (false, ", with the pointer left where it is")] {
        let name = format!("{NAME}{which}");
        if let (false, Some(aside)) = (move_pointer, aside) {
            let _ = CGWarpMouseCursorPosition(aside);
            pump(app, Duration::from_millis(20));
        }
        let before = super::input::cursor_pos();
        let from = inbox_mark();
        let t = Instant::now();
        let line = match super::input::post_click(tool.hwnd(), screen.0, screen.1, MouseButton::Left, move_pointer) {
            Ok(line) => line,
            Err(why) if why.reason() == &COVERED => {
                say(&format!(
                    "check {name}: skipped — the window server names another window over the page's button on this \
                     runner; neither a pass nor a failure"
                ));
                continue;
            }
            Err(why) => {
                tally.check(&name, Err(format!("refused: {why}")));
                continue;
            }
        };
        say(&format!("web view | {line}"));
        let clicked = wait_message(app, n, from, t + SECTION, |m| m.starts_with("clicked "));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        // Anything still on its way after the click: a second press would be a click made twice.
        let echoed = round_trip(app, tool, n, if move_pointer { 70 } else { 71 }).is_some();
        let pressed = presses(n, from);
        let clicks = messages(n, from).iter().filter(|m| m.starts_with("clicked ")).count();
        tally.check(
            &name,
            match (clicked, pressed.as_slice()) {
                (Some(_), [p]) if clicks == 1 && echoed && near(*p, inside) => {
                    Ok(format!("pressed at {},{} in the page, clicked {ms:.0} ms after the call", p.0, p.1))
                }
                _ => Err(format!(
                    "asked at {},{} on the screen, {},{} in the page; the page saw {} press(es) at {:?} and {clicks} click(s), \
                     the echo after them {}",
                    screen.0,
                    screen.1,
                    inside.0,
                    inside.1,
                    pressed.len(),
                    pressed,
                    if echoed { "answered" } else { "unanswered" }
                )),
            },
        );
        let after = super::input::cursor_pos();
        let (want, said) = if move_pointer { (screen, "on the point") } else { (before, "where it was") };
        tally.check(
            &format!("a posted click{which} leaves the pointer {said}"),
            if near(after, want) {
                Ok(String::new())
            } else {
                Err(format!("the pointer is at {},{}; before the click it was at {},{}", after.0, after.1, before.0, before.1))
            },
        );
    }
}

/// A posted click into the tool page shown without the keyboard, twice, while another application
/// is in front where the runner has one: what the page saw of each, and what the front, this
/// application's activation and the page window's key state did — data, no check. A plug-in window
/// a page in front clicks into is in this state: not the key window, its application not the
/// active one. The web view refuses the first mouse (`with_accept_first_mouse(false)`, `page.rs`),
/// and AppKit makes a window that is not the key window key with such a press rather than handing
/// the press to the view, so the first may be taken for that; whether the second then lands while
/// the application stays in the background is not known (the prior art saw such clicks dropped),
/// and this records it. Whether either brings this application forward is the other half of the
/// answer. The page is hidden again after.
fn posted_unkeyed(app: &NSApplication, tool: &page::Native, n: u64) {
    if !CGPreflightPostEventAccess() {
        say("web view | posted clicks into the page without the keyboard: not tried — post-event access is not granted on this runner");
        return;
    }
    tool.put(None, None);
    if tool.show(false).is_err() {
        say("web view | posted clicks into the page without the keyboard: not tried — the page could not be shown");
        return;
    }
    settle(app);
    let window = tool.window();
    let Some((screen, inside)) = button_point(app, tool, n) else {
        say("web view | posted clicks into the page without the keyboard: not tried — the page did not say where its button is");
        tool.hide();
        settle(app);
        return;
    };
    for attempt in 1..=2 {
        let front = frontmost();
        let (active, key) = (app.isActive(), window.isKeyWindow());
        let from = inbox_mark();
        let t = Instant::now();
        let posted = super::input::post_click(tool.hwnd(), screen.0, screen.1, MouseButton::Left, true);
        let clicked = wait_message(app, n, from, t + Duration::from_secs(2), |m| m.starts_with("clicked "));
        settle(app);
        let pressed = presses(n, from);
        say(&format!(
            "web view | a posted click into the tool page shown without the keyboard, {attempt} of 2, at {},{} in the page: {}; \
             the page saw {} press(es) at {:?} and {}; frontmost {} → {}; this application active {} → {}; the page's window key \
             {} → {}",
            inside.0,
            inside.1,
            match &posted {
                Ok(_) => "posted".to_string(),
                Err(why) => format!("refused: {why}"),
            },
            pressed.len(),
            pressed,
            match clicked {
                Some((at, _)) => format!("a click {:.0} ms after the call", at.saturating_duration_since(t).as_secs_f64() * 1000.0),
                None => "no click".to_string(),
            },
            describe(front),
            describe(frontmost()),
            yes(active),
            yes(app.isActive()),
            yes(key),
            yes(window.isKeyWindow())
        ));
        if posted.is_err() {
            break;
        }
    }
    tool.hide();
    settle(app);
}

/// A tool page's keys, sent while it has the keyboard: the Edit keys in its field, Return on its
/// button, Command-Q and the two Command-H that do nothing, and Command-W, which hides it — last.
/// `n` is the page's number; `which` follows each check's name, to tell the pages' lines apart.
fn keys(app: &NSApplication, tally: &mut Tally, tool: &page::Native, n: u64, which: &str) {
    let window = tool.window();
    let cmd = NSEventModifierFlags::Command;
    let focused = |what: &str| {
        let from = inbox_mark();
        let t = Instant::now();
        tell(tool, &format!("focus-{what}"));
        wait_message(app, n, from, t + SECTION, |m| m.starts_with("focused ")).map(|(_, m)| m)
    };
    let field = focused("field");
    if field.as_deref() != Some("focused field") {
        tally.check(&format!("the field takes the keyboard{which}"), Err(format!("{field:?}")));
        return;
    }
    // Command-A, Command-C: the field's text on the pasteboard.
    set_pasteboard("page-check before the copy");
    let (_, count) = pasteboard();
    press(app, window, "a", 0, cmd);
    press(app, window, "c", 8, cmd);
    let t = Instant::now();
    let copied = loop {
        let (text, now) = pasteboard();
        if now != count && text.as_deref() == Some("copied by page-check") {
            break Ok(format!("after {:.0} ms", t.elapsed().as_secs_f64() * 1000.0));
        }
        if t.elapsed() >= SECTION {
            break Err(format!("the pasteboard holds {text:?}"));
        }
        pump(app, Duration::from_millis(20));
    };
    tally.check(&format!("Command-A and Command-C copy the field's text{which}"), copied);
    // Command-A, Command-V: the field's text replaced by the pasteboard's.
    set_pasteboard("pasted by page-check");
    let from = inbox_mark();
    let t = Instant::now();
    press(app, window, "a", 0, cmd);
    press(app, window, "v", 9, cmd);
    let pasted = wait_message(app, n, from, t + SECTION, |m| m == "value pasted by page-check");
    tally.check(
        &format!("Command-V pastes{which}"),
        pasted.map(|_| String::new()).ok_or_else(|| format!("the field said: {}", messages(n, from).join(", "))),
    );
    // Return on the button: one message.
    let button = focused("button");
    if button.as_deref() == Some("focused button") {
        let from = inbox_mark();
        press(app, window, "\r", 36, NSEventModifierFlags::empty());
        let echoed = round_trip(app, tool, n, 50).is_some();
        let clicks = messages(n, from).iter().filter(|m| m.starts_with("clicked ")).count();
        tally.check(
            &format!("Return on a button gives one message{which}"),
            if echoed && clicks == 1 { Ok(String::new()) } else { Err(format!("{clicks} message(s), echo after them {echoed}")) },
        );
    } else {
        tally.check(&format!("the button takes the keyboard{which}"), Err(format!("{button:?}")));
    }
    // Command-Q, Command-H, Option-Command-H: nothing, and none of the menu's items.
    let counts = || (MENU_QUIT.load(Ordering::Relaxed), MENU_HIDE.load(Ordering::Relaxed), MENU_HIDE_OTHERS.load(Ordering::Relaxed));
    let before = counts();
    press(app, window, "q", 12, cmd);
    press(app, window, "h", 4, cmd);
    press(app, window, "h", 4, cmd | NSEventModifierFlags::Option);
    let echoed = round_trip(app, tool, n, 60).is_some();
    pump(app, Duration::from_millis(20));
    let hidden = app.isHidden();
    let quit = crate::instance::take_quit_request();
    tally.check(
        &format!("Command-Q, Command-H and Option-Command-H do nothing in a tool page{which}"),
        if echoed && counts() == before && !hidden && !quit && window.isVisible() {
            Ok(String::new())
        } else {
            Err(format!(
                "the menu's Quit, Hide and Hide Others counted {:?} → {:?}; this application hidden {hidden}; a quit asked for \
                 {quit}; the page shown {}",
                before,
                counts(),
                window.isVisible()
            ))
        },
    );
    if hidden {
        app.unhide(None);
    }
    // Command-W: hidden, and the module told the user closed it.
    let from = inbox_mark();
    let t = Instant::now();
    press(app, window, "w", 13, cmd);
    let closed = wait_page(app, from, t + SECTION, |e| matches!(e, PageEvent::Closed { page, why: "user", .. } if *page == n));
    tally.check(
        &format!("Command-W hides the tool page{which}"),
        if closed.is_some() && !window.isVisible() {
            Ok(String::new())
        } else {
            Err(format!("closed by the user said {}; the window shown {}", closed.is_some(), window.isVisible()))
        },
    );
}

/// A tool page described `activates = false` (experimental), built and shown with the keyboard while
/// another application is the frontmost one. What the host promises of it whatever the desktop is a
/// `check`: its build and its show ask for no activation — counted, as for the other pages — and,
/// where it gets the keyboard, its keys are the tool page's; and where the workspace still names
/// the other application when Command-W hides the page, the front goes back to it at once, with no
/// focus left waiting for an activation that application never announces
/// (`backend::active_for_handing`); and its show answers `true` exactly when its window is key at
/// the call, as nothing is on its way. Where the keyboard went, what AppKit and the workspace say
/// is active, and the page's level are data, and so is the page beside another
/// ([`beside_a_page`]).
fn nonactivating_page(app: &NSApplication, mtm: MainThreadMarker, tally: &mut Tally, rules: &WKContentRuleList, root: &Path, counting: bool) {
    const NAME: &str = "non-activating";
    const N: u64 = 4;
    let from = inbox_mark();
    let t0 = Instant::now();
    let Some(page) = build_page(tally, rules, root, WindowKind::Tool, false, NAME, N, counting) else { return };
    if !wait_loaded(app, tally, N, from, t0, NAME) {
        return;
    }
    let counter = install_menu(app, mtm);
    let window = page.window();
    let n = window.windowNumber();
    let me = std::process::id() as i32;
    let before = frontmost();
    let asked = ACTIVATIONS.load(Ordering::Relaxed);
    let seen = mark();
    let t = Instant::now();
    page.put(None, None);
    let shown = page.show(true);
    // Before the event loop runs again: nothing is on its way, so the answer is the window's state
    // at the call (docs/api/gui.md).
    let key_at_call = window.isKeyWindow();
    let key = wait(app, seen, t, t + SECTION, |s| s == Seen::Key(n));
    let activations = ACTIVATIONS.load(Ordering::Relaxed) - asked;
    tally.check(
        "showing the non-activating page with the keyboard asked for no activation",
        match (counting, activations) {
            (false, _) => Err("the activation calls could not be counted".to_string()),
            (true, 0) => Ok("none counted".to_string()),
            (true, k) => Err(format!("{k} asked for")),
        },
    );
    let answer = if shown.is_ok() { "true" } else { "\"declined by the system\"" };
    tally.check(
        "the non-activating page's show answers true exactly when its window is key at the call",
        if shown.is_ok() == key_at_call {
            Ok(format!("answered {answer}, key {}", yes(key_at_call)))
        } else {
            Err(format!(
                "answered {answer}, but the window {} the key window when the show returned",
                if key_at_call { "was" } else { "was not" }
            ))
        },
    );
    say(&format!(
        "web view | the non-activating page shown with the keyboard: show answered {}; the window key {}; made \
         non-activating {}; this application active (AppKit) {}; frontmost before: {}; frontmost now: {}; its level {}",
        if shown.is_ok() { "true" } else { "\"declined by the system\"" },
        after(key),
        yes(window.styleMask().contains(NSWindowStyleMask::NonactivatingPanel)),
        yes(app.isActive()),
        describe(before),
        describe(frontmost()),
        window.level()
    ));
    if window.isKeyWindow() {
        // Read before the keys: Command-W, the last of them, hides the page.
        let lent = frontmost().is_some_and(|f| f != me);
        keys(app, tally, &page, N, " (activates = false)");
        pump(app, Duration::from_millis(20));
        if lent {
            tally.check(
                "the non-activating page hidden gives the front back at once",
                match front::waiting_for() {
                    None => Ok(format!("nothing waits; frontmost now: {}", describe(frontmost()))),
                    Some(p) => Err(format!(
                        "a focus waits for {} to become the active application, which it already is and never announces",
                        describe(Some(p))
                    )),
                },
            );
        } else {
            say("web view | the non-activating page's hand-back is not checked: the workspace named this application the frontmost one");
        }
    } else {
        say("web view | the non-activating page's keys are not checked: the page did not get the keyboard on this Mac");
        page.hide();
    }
    say(&format!(
        "web view | the non-activating page hidden: this application active (AppKit) {}; frontmost now: {}",
        yes(app.isActive()),
        describe(frontmost())
    ));
    app.setMainMenu(None);
    drop(counter);
    beside_a_page(app, tally, rules, root, counting, &page);
    drop(page);
}

/// The non-activating page beside a tool page that activates this application, the default — as
/// data, what AppKit does there being nothing the host can promise. Hidden while the other page is
/// shown without the keyboard: whether AppKit gives the keyboard to the other page as it leaves, so
/// that this application is deactivated (`front::keyboard_kept`), and where the keyboard is after
/// that. Then the other page shown with the keyboard while the non-activating one has it: whether
/// the activation that show asks for makes the workspace name this application, though AppKit calls
/// it active already; and the two hidden, the non-activating one first, so that the other one gives
/// the front back to the application that was in front through it: whether its activation is asked
/// for and announced, or a focus is left waiting for an activation that application, the active one
/// throughout, never announces. Each part only where the non-activating page gets the keyboard with
/// another application named the frontmost one; a line says so otherwise.
fn beside_a_page(app: &NSApplication, tally: &mut Tally, rules: &WKContentRuleList, root: &Path, counting: bool, page: &page::Native) {
    const NAME: &str = "second tool";
    const N: u64 = 5;
    let from = inbox_mark();
    let t0 = Instant::now();
    let Some(other) = build_page(tally, rules, root, WindowKind::Tool, true, NAME, N, counting) else { return };
    if !wait_loaded(app, tally, N, from, t0, NAME) {
        return;
    }
    let me = std::process::id() as i32;
    let window = page.window();
    let n = window.windowNumber();
    let other_n = other.window().windowNumber();
    let key = || match super::own::key_now(app).map(|w| w.windowNumber()) {
        Some(k) if k == n => "the non-activating page",
        Some(k) if k == other_n => "the other tool page",
        Some(_) => "another window of ours",
        None => "none of ours",
    };
    // The non-activating page shown with the keyboard: whether it has it with another application
    // named the frontmost one.
    let lend = || {
        let seen = mark();
        let t = Instant::now();
        let _ = page.show(true);
        let _ = wait(app, seen, t, t + SECTION, |s| s == Seen::Key(n));
        window.isKeyWindow() && frontmost().is_some_and(|f| f != me)
    };
    let not_measured = |what: &str| {
        say(&format!(
            "web view | beside another page: {what} not measured, the non-activating page did not get the keyboard with \
             another application the frontmost one (frontmost: {})",
            describe(frontmost())
        ));
        page.hide();
        other.hide();
    };

    let _ = other.show(false);
    if !lend() {
        return not_measured("the hand-back");
    }
    let _ = front::take_kept();
    page.hide();
    let straight = key();
    let kept = front::take_kept();
    pump(app, Duration::from_millis(20));
    say(&format!(
        "web view | beside another page: the non-activating page hidden with the other tool page shown without the \
         keyboard: AppKit gave the keyboard to a window of ours as it left: {}; straight after, the key window {straight}; \
         after the event loop ran, the key window {}; this application active (AppKit) {}; frontmost {}; a focus waits {}",
        match kept {
            Some(true) => "yes, so this application was deactivated",
            Some(false) => "no",
            None => "not asked, the front was not handed back at once",
        },
        key(),
        yes(app.isActive()),
        describe(frontmost()),
        front::waiting_for().map_or_else(|| "for nothing".to_string(), |p| format!("for {}", describe(Some(p))))
    ));

    if !lend() {
        return not_measured("the default page shown over it");
    }
    let lent_to = frontmost();
    let asked = ACTIVATIONS.load(Ordering::Relaxed);
    let seen = mark();
    let t = Instant::now();
    let answered = other.show(true);
    let at_call = other.window().isKeyWindow();
    let got = wait(app, seen, t, t + SECTION, |s| s == Seen::Key(other_n));
    let named = wait(app, seen, t, t + SECTION, |s| s == Seen::Front(me));
    say(&format!(
        "web view | beside another page: the other tool page shown with the keyboard while the non-activating page has \
         it, {} the frontmost one: show answered {}; key at the call {}; the window key {}; activations asked for {}; \
         this application active (AppKit) {}; the workspace named this application {}; frontmost now: {}",
        describe(lent_to),
        if answered.is_ok() { "true" } else { "\"declined by the system\"" },
        yes(at_call),
        after(got),
        if counting { (ACTIVATIONS.load(Ordering::Relaxed) - asked).to_string() } else { "not counted".to_string() },
        yes(app.isActive()),
        after(named),
        describe(frontmost())
    ));

    page.hide();
    let before = frontmost();
    let _ = front::take_ended();
    let seen = mark();
    let t = Instant::now();
    other.hide();
    let waited = front::waiting_for();
    let came = waited.and_then(|p| wait(app, seen, t, t + SECTION, |s| s == Seen::Front(p)));
    pump(app, Duration::from_millis(20));
    let ended = match (front::take_ended(), front::waiting_for()) {
        (Some(Ended::Made), _) => "made at its activation".to_string(),
        (Some(Ended::WentTo(p)), _) => format!("given up, {} became the active one first", describe(Some(p))),
        (Some(Ended::Typed), _) => "given up, a key was pressed".to_string(),
        (Some(Ended::GivenUp), _) => "given up before an activation came".to_string(),
        (None, Some(p)) => format!("STILL WAITING for {}", describe(Some(p))),
        (None, None) => "none, nothing waited".to_string(),
    };
    say(&format!(
        "web view | beside another page: the non-activating page hidden, then the other one, which had the keyboard, \
         with {} the frontmost one: {}; the focus: {ended}; the key window {}; this application active (AppKit) {}; \
         frontmost now: {}",
        describe(before),
        match waited {
            Some(p) => format!("the activation of {} asked for, announced {}", describe(Some(p)), after(came)),
            None => "no activation waited for".to_string(),
        },
        key(),
        yes(app.isActive()),
        describe(frontmost())
    ));
    // What is left waiting is the check's, not the rule list's that follows.
    front::cancel("page-check moves on");
}

/// The app page, as data: the Dock claim, the keyboard, Command-M and Command-Q, and the claim let go.
fn app_page(app: &NSApplication, page: &page::Native) {
    let window = page.window();
    let n = window.windowNumber();
    let seen = mark();
    let t = Instant::now();
    page.put(None, None);
    let shown = page.show(true);
    let key = wait(app, seen, t, t + SECTION, |s| s == Seen::Key(n));
    settle(app);
    say(&format!(
        "app page | shown with the keyboard: show answered {}; the window key {}; this application active {}; its policy {}",
        if shown.is_ok() { "true" } else { "\"declined by the system\"" },
        after(key),
        yes(app.isActive()),
        policy(app)
    ));
    if !window.isKeyWindow() {
        page.hide();
        settle(app);
        say(&format!("app page | its keys are not tried: it did not get the keyboard; hidden, its policy {}", policy(app)));
        return;
    }
    press(app, window, "m", 46, NSEventModifierFlags::Command);
    settle(app);
    let minimised = window.isMiniaturized();
    let shown_while = page.is_shown();
    let again = page.show(true);
    settle(app);
    say(&format!(
        "app page | Command-M: miniaturised {}, counted as shown {}; shown again: {}, miniaturised {}",
        yes(minimised),
        yes(shown_while),
        if again.is_ok() { "true" } else { "\"declined by the system\"" },
        yes(window.isMiniaturized())
    ));
    let before = MENU_QUIT.load(Ordering::Relaxed);
    press(app, window, "q", 12, NSEventModifierFlags::Command);
    pump(app, Duration::from_millis(20));
    say(&format!(
        "app page | Command-Q: the host asked to quit {}; a menu's Quit counted {}",
        yes(crate::instance::take_quit_request()),
        MENU_QUIT.load(Ordering::Relaxed) - before
    ));
    page.hide();
    settle(app);
    say(&format!("app page | hidden: its policy {}", policy(app)));
}

fn policy(app: &NSApplication) -> &'static str {
    match app.activationPolicy() {
        NSApplicationActivationPolicy::Regular => "regular (the Dock claimed)",
        NSApplicationActivationPolicy::Accessory => "accessory",
        _ => "prohibited",
    }
}
