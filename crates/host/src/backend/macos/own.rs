//! This application's own windows, answered by AppKit — never by an accessibility query into this
//! process.
//!
//! Every other window the backend describes is asked of its application through accessibility
//! (`ax.rs`), a call into another process. Asked of this process, the same question is a call from
//! the main thread into the process whose main thread is asking: the answer waits for the loop that
//! is waiting for it, and costs the messaging timeout at the least. Whether it can hang outright is
//! not established — the documented hang is a process asked from inside an accessibility callback
//! (`watch::ensure_observer`) — but a timeout is enough to put this process into the not-answering
//! quarantine, after which nothing of ours is asked for five seconds. What AppKit knows in-process
//! answers the same questions at once and for nothing: which of our windows has the keyboard, where
//! each one is, what it is called.
//!
//! **Ids.** An own window's id is `OWN_BASE` plus its window number ([`crate::backend::own_id`]), so
//! it never meets the accessibility handle table's counter and says by itself which window it is.
//!
//! **Which of our windows has the keyboard** ([`has_keyboard`]): set from AppKit's own notifications
//! when any window of this process becomes or stops being the key window — wxWidgets' included —
//! and read again as a level at every tick of the event loop ([`refresh`]), so an edge a
//! notification missed is corrected within a tick and said in the log. The event tap reads it for
//! key-downs only — at its decision (`tap::decide`), and for the repeat of a key it swallowed: while
//! one of our windows has the keyboard, no capture applies.
//!
//! **Kinds.** A page's window is noted as one ([`note_page`]); every other window of ours is
//! [`OwnKind::Other`]. Only `host.window.ownsPoint` tells them apart.
//!
//! Nothing here asks AppKit for its application object before the window system has started
//! ([`start`], called from `gui.rs` once wxWidgets is up): wxWidgets creates that object as an
//! instance of its own subclass, and one created here first would not be it. A headless run has no
//! window of ours, and every answer here is then "none".
//!
//! Main thread only, like the rest of the backend: the notifications arrive on it, and the tick and
//! every backend call run on it. What the event tap reads — whether one of ours has the keyboard,
//! and its title for a log line — is in an atomic and a mutex rather than in thread-locals, so that
//! the tap could move to a thread of its own without it going silently stale (see `tap.rs`).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSWindow, NSWindowDidBecomeKeyNotification,
    NSWindowDidResignKeyNotification, NSWindowStyleMask,
};
use objc2_foundation::{NSNotification, NSNotificationCenter};

use crate::backend::{cocoa_frame_top_left, own_id, own_number, Foreground, OwnKind, WinInfo};

/// [`start`] has run, on the main thread, once the window system was up.
static STARTED: AtomicBool = AtomicBool::new(false);

/// Whether a window of this process is the key window: the one the keys the user types go to.
static OWN_KEY: AtomicBool = AtomicBool::new(false);

/// That window's number, 0 while none of ours is the key window.
static KEY_NUMBER: AtomicU32 = AtomicU32::new(0);

/// Whether that window is a panel that takes the keyboard without activating its application: the
/// one way a window of ours can have the keyboard while another application is the frontmost one.
static KEY_NONACTIVATING: AtomicBool = AtomicBool::new(false);

/// That window's title, for the event tap's line about a key it let through to it. Written at the
/// tick when the key window changes; read by the tap with `try_lock`, never waited for.
static KEY_TITLE: Mutex<String> = Mutex::new(String::new());

thread_local! {
    /// The numbers of the windows that are pages' ([`note_page`]).
    static PAGES: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
    /// The key window the log was last told about (0 for none), and since when one of ours had
    /// the keyboard.
    static SAID: Cell<(u32, Option<Instant>)> = const { Cell::new((0, None)) };
    /// When AppKit last told this process that one of its windows took the keyboard with none of
    /// ours holding it (`true`), or that the one holding it let it go (`false`) — kept until the
    /// workspace's side of the same change comes, for the line that says which came first
    /// ([`pair_edges`]).
    static KEY_EDGE: Cell<Option<(bool, Instant)>> = const { Cell::new(None) };
    /// When the workspace last named this application the frontmost one (`true`), or another one
    /// after this one (`false`), kept the same way.
    static FRONT_EDGE: Cell<Option<(bool, Instant)>> = const { Cell::new(None) };
    /// Whether the workspace's last activation named this application ([`note_front`]).
    static FRONT_OURS: Cell<bool> = const { Cell::new(false) };
}

/// Starts following which of this application's windows has the keyboard. Called once the window
/// system is up (`gui.rs`, as wxWidgets hands over); idempotent; main thread only.
///
/// The two notifications are observed with no queue, so their block runs where AppKit posts them —
/// on the main thread, as the key window changes — and does nothing but store two atomics. What
/// the change means for the log and for the host is the tick's ([`refresh`]).
pub(crate) fn start() {
    if STARTED.load(Ordering::SeqCst) {
        return;
    }
    if MainThreadMarker::new().is_none() {
        crate::logging::line(
            "macos",
            "this application's own windows: asked to follow them off the main thread, so they are \
             not followed — every window of ours reads as none",
        );
        return;
    }
    let centre = NSNotificationCenter::defaultCenter();
    // SAFETY: framework-owned constants that live for the process.
    let names = unsafe { [(NSWindowDidBecomeKeyNotification, true), (NSWindowDidResignKeyNotification, false)] };
    for (name, became) in names {
        let block = RcBlock::new(move |n: NonNull<NSNotification>| {
            // An unwind out of a block AppKit calls would tear through frames that cannot handle it.
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: a live notification for the duration of the call.
                on_key_change(unsafe { n.as_ref() }, became);
            }));
            if caught.is_err() {
                crate::logging::line("macos", "panic while noting a change of this application's key window");
            }
        });
        // SAFETY: the block is copied by the centre; no queue, so it runs on the posting thread.
        let token = unsafe { centre.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) };
        // Leaked deliberately, like every subscription in this backend: it lasts as long as the
        // process.
        core::mem::forget(token);
    }
    STARTED.store(true, Ordering::SeqCst);
    crate::logging::line(
        "macos",
        "this application's own windows are answered by AppKit, never by an accessibility query \
         into this process; which of them has the keyboard is followed from AppKit's notifications \
         and read again at every tick",
    );
    refresh();
}

/// A window of this process became the key window, or stopped being it. Stores, and nothing else:
/// the change itself, and when this application took or lost the keyboard by it, for the line that
/// sets that against the workspace's notice ([`pair_edges`], at the tick).
fn on_key_change(n: &NSNotification, became: bool) {
    let Some(object) = n.object() else { return };
    let Some(window) = object.downcast_ref::<NSWindow>() else { return };
    let number = u32::try_from(window.windowNumber()).unwrap_or(0);
    if became {
        let had = OWN_KEY.load(Ordering::Relaxed);
        KEY_NUMBER.store(number, Ordering::Relaxed);
        KEY_NONACTIVATING.store(nonactivating(window), Ordering::Relaxed);
        OWN_KEY.store(number != 0, Ordering::Relaxed);
        if !had && number != 0 {
            KEY_EDGE.with(|e| e.set(Some((true, Instant::now()))));
        }
    } else if KEY_NUMBER.compare_exchange(number, 0, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
        // Only the window noted as the key one: another of ours may have become key already, and
        // AppKit does not promise the order of the two notifications.
        OWN_KEY.store(false, Ordering::Relaxed);
        KEY_EDGE.with(|e| e.set(Some((false, Instant::now()))));
    }
}

/// The application object, once the window system is up and on the main thread; `None` otherwise.
/// Also what `app::activate_self` and `front.rs` ask for it.
pub(super) fn app() -> Option<Retained<NSApplication>> {
    if !STARTED.load(Ordering::Relaxed) {
        return None;
    }
    Some(NSApplication::sharedApplication(MainThreadMarker::new()?))
}

/// The key window, if it is one of ours: `keyWindow`. Only while the notifications say a panel
/// that takes the keyboard without activating this application has it — which `keyWindow` is not
/// documented to name while another application is the active one — also any window of ours that
/// says it is key; at every other tick, with the DAW in front and nothing of ours key, that look at
/// each of our windows is not made.
fn key_window(app: &NSApplication) -> Option<Retained<NSWindow>> {
    app.keyWindow().or_else(|| {
        let panel = OWN_KEY.load(Ordering::Relaxed) && KEY_NONACTIVATING.load(Ordering::Relaxed);
        if !panel {
            return None;
        }
        app.windows().iter().find(|w| w.isKeyWindow())
    })
}

/// The window number of a window of ours, 0 for one without (never ordered in).
fn number_of(w: &NSWindow) -> u32 {
    u32::try_from(w.windowNumber()).unwrap_or(0)
}

/// Whether a window of ours is a panel that takes the keyboard without activating this application.
pub(super) fn nonactivating(w: &NSWindow) -> bool {
    w.styleMask().contains(NSWindowStyleMask::NonactivatingPanel)
}

/// The tick: which of our windows has the keyboard, read as a level, and what changed said.
///
/// Called from the pump on every pass (`MacBackend::pump_pending`), before the queue is drained:
/// `keyWindow` — a message inside this process, nothing asked of another one. A change from what
/// the notifications stored is corrected here, and said: it means a notification did not come.
/// Every change of which window of ours has the keyboard is a focus change for the host, as a move
/// between two windows of one application is a foreground change on Windows: an overlay lets go in
/// front of our window (and takes hold again after it) even when no application came to the front,
/// and what was read for the window before is read again. Whether one of ours has the keyboard at
/// all is said in one `keys` line each way; a move between two of ours is traced.
pub(super) fn refresh() {
    let Some(app) = app() else { return };
    pair_edges();
    let key = key_window(&app);
    let number = key.as_deref().map_or(0, number_of);
    let noted = if OWN_KEY.load(Ordering::Relaxed) { KEY_NUMBER.load(Ordering::Relaxed) } else { 0 };
    let corrected = number != noted;
    if corrected {
        KEY_NUMBER.store(number, Ordering::Relaxed);
        KEY_NONACTIVATING.store(key.as_deref().is_some_and(nonactivating), Ordering::Relaxed);
        OWN_KEY.store(number != 0, Ordering::Relaxed);
    }
    let (said, since) = SAID.with(Cell::get);
    if number == said {
        return;
    }
    let title = key.as_deref().map(|w| w.title().to_string()).unwrap_or_default();
    *KEY_TITLE.lock().unwrap_or_else(|e| e.into_inner()) = title.clone();
    let how = if corrected { " (read at the tick: no notification said so)" } else { "" };
    match (said, number) {
        (0, _) => {
            SAID.with(|s| s.set((number, Some(Instant::now()))));
            crate::logging::line(
                "keys",
                &format!(
                    "one of our windows has the keyboard ('{title}'){how}: the event tap lets every \
                     captured key through to it until none of ours has the keyboard"
                ),
            );
        }
        (_, 0) => {
            SAID.with(|s| s.set((0, None)));
            let held = since.map(|t| format!(", after {:.1} s", t.elapsed().as_secs_f64())).unwrap_or_default();
            crate::logging::line(
                "keys",
                &format!("none of our windows has the keyboard any more{held}{how}: captures apply again"),
            );
        }
        _ => {
            SAID.with(|s| s.set((number, since)));
            crate::logging::trace("keys", || format!("the keyboard moved to another of our windows ('{title}'){how}"));
        }
    }
    super::queue::mark_focus_dirty();
}

/// The workspace named process `pid` the frontmost application (`watch::activated`). Only a change
/// between this application and another is noted, for the line that sets it against AppKit's word
/// that one of our windows took or let go of the keyboard ([`pair_edges`]). Nothing before
/// [`start`].
pub(super) fn note_front(pid: i32) {
    if !STARTED.load(Ordering::Relaxed) {
        return;
    }
    let ours = pid == std::process::id() as i32;
    if FRONT_OURS.with(|f| f.replace(ours)) == ours {
        return;
    }
    FRONT_EDGE.with(|e| e.set(Some((ours, Instant::now()))));
    pair_edges();
}

/// One `keys` line for each change of where the keyboard is that both sides have told this process
/// of — AppKit, that one of our windows took the keyboard or that ours let it go; the workspace,
/// that this application or another one is now the frontmost — saying which came first and how far
/// apart, as this process heard them. It decides nothing: nobody has seen that order yet (MW17 in
/// TODO.md), and between the two the answers that follow the workspace and the ones that follow
/// AppKit differ. Two halves that are not of one change — the workspace naming another application,
/// then a window of ours taking the keyboard — are not paired: the older is dropped, and the newer
/// waits for its own other half. Called at each tick and at each of the workspace's notices; two
/// loads when there is nothing to say.
fn pair_edges() {
    let (Some((took, key_at)), Some((ours, front_at))) = (KEY_EDGE.with(Cell::get), FRONT_EDGE.with(Cell::get)) else {
        return;
    };
    if took != ours {
        if key_at < front_at {
            KEY_EDGE.with(|e| e.set(None));
        } else {
            FRONT_EDGE.with(|e| e.set(None));
        }
        return;
    }
    KEY_EDGE.with(|e| e.set(None));
    FRONT_EDGE.with(|e| e.set(None));
    let keyboard = if took { "one of our windows took the keyboard" } else { "our windows let the keyboard go" };
    let front = if ours {
        "the workspace named this application the frontmost one"
    } else {
        "the workspace named another application the frontmost one"
    };
    let ms = |later: Instant, earlier: Instant| later.duration_since(earlier).as_secs_f64() * 1000.0;
    let line = if key_at <= front_at {
        format!("{front} {:.1} ms after {keyboard}", ms(front_at, key_at))
    } else {
        format!("{keyboard} {:.1} ms after {front}", ms(key_at, front_at))
    };
    crate::logging::line("keys", &format!("{line} (AppKit's notice and the workspace's, as this process heard them)"));
}

/// Whether one of this application's windows has the keyboard, by AppKit's account inside this
/// process. One atomic load: the event tap asks it at every key-down it decides. Not checked
/// against the application the workspace names, as [`key_id_in_front_of`] is: the tap has nobody to
/// ask, and the reads of where the keyboard is (`ax::keyboard_is_ours`) must not ask this process
/// in the moment before the workspace names it.
pub(super) fn has_keyboard() -> bool {
    OWN_KEY.load(Ordering::Relaxed)
}

/// The title of our window that has the keyboard, for a log line; empty when it cannot be read
/// without waiting.
pub(super) fn key_title() -> String {
    KEY_TITLE.try_lock().map(|t| t.clone()).unwrap_or_default()
}

/// The id of our window that has the keyboard, while one has.
fn key_id() -> Option<isize> {
    if !has_keyboard() {
        return None;
    }
    own_id(i64::from(KEY_NUMBER.load(Ordering::Relaxed)))
}

/// The id of our window that has the keyboard, for the application the workspace names frontmost
/// (`front`): while this application is the frontmost one, or while the window is a panel that takes
/// the keyboard without activating its application. Any other window of ours noted as key while the
/// workspace names another application is not answered, and the application the workspace names is
/// asked instead: these are the answers `watch` resolves the workspace's activation through, and
/// that activation is of the application it names. Which side catches up first is not known (MW17
/// in TODO.md); the second is a focus change as well (`watch` for the workspace, [`refresh`] for
/// AppKit), so the answers are read again once both have come.
pub(super) fn key_id_in_front_of(front: Option<i32>) -> Option<isize> {
    let id = key_id()?;
    let counts = front == Some(std::process::id() as i32) || KEY_NONACTIVATING.load(Ordering::Relaxed);
    counts.then_some(id)
}

/// The window of ours in front, for this application as the frontmost one: the key window, else
/// the main window. Either is the one a click on the Dock icon or a Command-Tab back would bring up.
fn front_window(app: &NSApplication) -> Option<Retained<NSWindow>> {
    key_window(app).or_else(|| app.mainWindow())
}

/// The id of our window in front, 0 when this application has none in front — said for certain,
/// since it is our own: the "answered, and no window" of `ax::foreground_window_id`.
pub(super) fn front_id() -> isize {
    if let Some(id) = key_id() {
        return id;
    }
    app().and_then(|app| front_window(&app)).and_then(|w| own_id(i64::from(number_of(&w)))).unwrap_or(0)
}

/// `host.window.foreground()` while one of our windows has the keyboard, for the application the
/// workspace names frontmost ([`key_id_in_front_of`]): that window, and whether it is on screen.
/// `None` while none of ours has.
pub(super) fn foreground(front: Option<i32>) -> Option<Foreground> {
    let id = key_id_in_front_of(front)?;
    let shown = window(id).is_some_and(|w| w.isVisible() && !w.isMiniaturized());
    Some(Foreground { id, pid: std::process::id(), shown })
}

/// Our window with the id `id`, while it exists.
fn window(id: isize) -> Option<Retained<NSWindow>> {
    let n = own_number(id)?;
    app()?.windowWithWindowNumber(n as isize)
}

/// The window table of our window `id` (`ax::window_info`): `None` when it is gone, hidden or
/// minimised, or, with `require_title`, untitled — as for every other window.
pub(super) fn info(id: isize, require_title: bool) -> Option<WinInfo> {
    let w = window(id)?;
    win_info(&w, require_title)
}

/// `host.window.active()` while this application is in front or one of its windows has the
/// keyboard: the key window, else the main window, as long as it is shown.
pub(super) fn active_info() -> Option<WinInfo> {
    let app = app()?;
    let w = front_window(&app)?;
    win_info(&w, false)
}

/// Every shown, titled window of ours, for the unfiltered window list and a matcher naming this
/// application (`ax::enumerate_windows_in`). At most 64, as for any application there.
pub(super) fn windows_info() -> Vec<WinInfo> {
    let Some(app) = app() else { return Vec::new() };
    app.windows().iter().filter_map(|w| win_info(&w, true)).take(64).collect()
}

/// What `ax::win_info` builds from an accessibility element, built here from the window itself.
///
/// The frame and the content rectangle are AppKit's, flipped where they are read
/// ([`cocoa_frame_top_left`]): the client area is exact here, where another application's has to be
/// worked out (`ax::content_rect`). `class` is the window's own accessibility role, subrole and
/// identifier — read in-process, the values an accessibility query from outside would be answered
/// with — joined as `ax::join_class` joins them.
fn win_info(w: &NSWindow, require_title: bool) -> Option<WinInfo> {
    // Not on screen, or minimised: this platform's hidden, as for every other window.
    if !w.isVisible() || w.isMiniaturized() {
        return None;
    }
    let title = w.title().to_string();
    if require_title && title.is_empty() {
        return None;
    }
    let hwnd = own_id(i64::from(number_of(w)))?;
    let (_, primary_h) = super::capture::screen_size();
    if primary_h <= 0 {
        return None;
    }
    let h = f64::from(primary_h);
    let f = w.frame();
    let c = w.contentRectForFrameRect(f);
    let (x, y, fw, fh) = cocoa_frame_top_left(f.origin.x, f.origin.y, f.size.width, f.size.height, h);
    let (cx, cy, cw, ch) = cocoa_frame_top_left(c.origin.x, c.origin.y, c.size.width, c.size.height, h);
    let role = NSAccessibility::accessibilityRole(w).map(|s| s.to_string()).unwrap_or_default();
    let subrole = NSAccessibility::accessibilitySubrole(w).map(|s| s.to_string()).unwrap_or_default();
    let identifier = NSAccessibility::accessibilityIdentifier(w).map(|s| s.to_string()).unwrap_or_default();
    let pid = std::process::id();
    Some(WinInfo {
        hwnd,
        title,
        class: format!("{role}/{subrole}/{identifier}"),
        pid,
        exe: super::ax::exe_for_pid(pid as i32),
        bundle_id: super::ax::bundle_id_for_pid(pid as i32),
        x,
        y,
        w: fw,
        h: fh,
        client_x: cx,
        client_y: cy,
        client_w: cw,
        client_h: ch,
    })
}

/// What the window with the number `number` is, when it is one of ours; `None` for any other
/// application's (`ax::number_owns_point`).
pub(super) fn kind_of(number: u32) -> Option<OwnKind> {
    app()?.windowWithWindowNumber(number as isize)?;
    Some(if PAGES.with(|p| p.borrow().contains(&number)) { OwnKind::Page } else { OwnKind::Other })
}

/// Notes the window with the number `number` as a page's window (`page`), or as no longer one:
/// `page.rs`, when a page's window is built and when it is taken down.
pub(super) fn note_page(number: u32, page: bool) {
    PAGES.with(|p| {
        let mut p = p.borrow_mut();
        if page {
            p.insert(number);
        } else {
            p.remove(&number);
        }
    });
}

/// `host.window.focus` for our window `id`: brought back if minimised, this application asked to
/// become the active one, and the window ordered in front and made key — AppKit's own calls, where
/// another application's window is raised and focused through accessibility. `true` when the window
/// is shown or minimised: what AppKit then does is asynchronous, and the window's becoming key is
/// what says it happened. A hidden one — ordered out, as the manager is once it is closed, or with
/// this application hidden — is `false`, as [`info`] leaves it out: ordering it in from here would
/// show it behind the back of whatever hid it, which would go on believing it hidden.
pub(super) fn focus(id: isize) -> bool {
    let Some(w) = window(id) else {
        crate::logging::line("macos", &format!("cannot focus window {id}: no such window of ours"));
        return false;
    };
    if !w.isVisible() && !w.isMiniaturized() {
        crate::logging::line(
            "macos",
            &format!(
                "cannot focus our own window {id} ('{}'): it is hidden, and only what hid it shows it again",
                w.title()
            ),
        );
        return false;
    }
    if w.isMiniaturized() {
        w.deminiaturize(None);
    }
    super::app::activate_self();
    w.makeKeyAndOrderFront(None);
    crate::logging::line(
        "macos",
        &format!(
            "focusing our own window {id} ('{}'): this application asked to become active, the window \
             ordered in front and made key",
            w.title()
        ),
    );
    true
}
