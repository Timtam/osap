//! `host.gui.webview` on a Mac: WKWebView, through wry, in a window of the application's own — a
//! panel for a tool page, an ordinary window for an app page (`webview::WindowKind`). The Mac's
//! `webview_win.rs`, with the same interface, so `webview_lua.rs` can build, show and take down a
//! page the same way on both. It does not yet (stage 4, TODO.md): until it does, `page-check`
//! (`page_check.rs`) is what builds pages here, on every Mac runner CI has.
//!
//! **The tool page** is the VPS Avenger control tool's panel — a tool VoiceOver users run in
//! production with Logic: an `NSPanel` that activates the application when it takes the keyboard
//! (the maintainer's decision of 2026-10-11; one that does not is to be an experimental page option,
//! `activates = false`, still to come), titled, closable, resizable, a utility window, created at
//! once (`defer: NO`, so the window server numbers it from the start), never hidden because the
//! application is not the active one, on every Space and beside a full-screen application, out of
//! the window cycle. It can become the key and the main window, Escape does nothing in it, and the
//! close box hides it rather than close it. One departure: **the level follows the keyboard.** The
//! tool keeps its panel floating and hides it before every click it sends; here the panel floats
//! only while it is the key window and is at the normal level otherwise — also while a show waits
//! for the keyboard, which the tool sets floating first — as wxWidgets drops its own floating
//! windows when its application resigns; so a plug-in window brought forward is drawn over the
//! page, which is the round trip docs/api/gui.md describes.
//!
//! **The app page** is an ordinary `NSWindow`: titled, closable, miniaturisable, resizable, at the
//! normal level, in the window cycle. While one is shown the application claims the Dock
//! (`dock.rs`, under the setting `dock_while_open`), and lets it go when the last one is hidden or
//! taken down.
//!
//! **Taking the front** ([`Native::focus`]) is the tool's sequence — this application asked for
//! the front (`app::activate_self`, `activateIgnoringOtherApps:`), the window made key and ordered
//! in front regardless, the web view made the first responder — and the answer is that it was
//! asked: an activation is asynchronous. What came of it is decided by events only and said in the
//! log: the window becoming the key window (got); another application's activation announced first
//! — not the one a page of ours leaving asked for just before, which was asked first and comes
//! first — or a key the event tap saw pressed while none of our windows has the keyboard (not got —
//! and a tool page placed over the rectangle to avoid is hidden again, Windows' rule); the page
//! hidden first (abandoned). A request this application makes while it is the active one and that
//! leaves the window without the keyboard is declined at once.
//!
//! **A request that outlives its page.** A page hidden before its show had an answer — or hidden
//! again because a key went elsewhere — leaves the request for the front on its way, and it can
//! still make this application the active one with nothing of its own on screen: VoiceOver users
//! are then in no application at all (`app.rs`). So the request is remembered, and when AppKit says
//! this application became the active one and no window of ours that could take the keyboard is on
//! screen, the front goes back to where that page would have given it (`front::hand_front_to`, as
//! `host.window.focus` hands it). A newer request for the front — a show, a hand-over, this
//! application taking the front itself — and another application's activation announced after it
//! forget it.
//!
//! **Leaving the front.** A page hidden, closed by its close box or Command-W, or taken down while
//! it has the keyboard gives the front back to the window and application it took it from — or,
//! through a page it was shown in place of and hidden since, to the window that one came from, or
//! to its application where that window had no name (`webview::front_goes_to_app`) — by
//! `front::hand_front_to`: the activation asked for at once, the window raised and focused when the
//! workspace announces it. Only while it has the keyboard (`backend::hand_plan`), and an app page
//! only when it has kept the front since it took it, as on Windows. An app page lets its Dock claim
//! go after that, as the last thing.
//!
//! **Keys.** The page and WebKit see every key first; a key equivalent they leave unhandled is the
//! host's table (`webview::mac_key_of`): Command-W closes, the Edit keys edit the field the keyboard
//! is in, and Command-Q and Command-H do nothing in a tool page and what they do anywhere in an app
//! page — never what whichever main menu wxWidgets last installed would do, under a keyboard layout
//! whose letters are not Latin as well. The keyboard put back
//! where it was in the page is the shim's (`webview::platform_shim`), asked for when the window
//! becomes the key window; the classes AppKit makes the first responder while a page is shown are
//! said in the log, for whoever has to find out where the keyboard went.
//!
//! **What a page may reach.** Its own files, served from the module's folder by the scheme handler
//! (`webview::serve`); every other load is blocked by a content rule list ([`environment`],
//! `webview::RULE_LIST`) compiled once a session into a store in the folder beside the application,
//! not WebKit's default one under `~/Library`. Navigations away, new windows, downloads and
//! permissions are refused, each refusal a line in the log (the first twenty of a page, then a
//! count). Nothing of a page is kept: its data store is a non-persistent one, and a fresh
//! configuration is made for every build, since a scheme handler cannot be registered twice in one.
//! Pages are not throttled while their window is not shown, where macOS has the switch (14 and
//! later; wry's `BackgroundThrottlingPolicy`).
//!
//! **When it is loaded.** wry's report that a navigation finished, for a document of the page's own
//! origin — unless the scheme handler answered that document with anything but 200, which is a
//! load that failed: wry reports no failed navigation on a Mac, and WebKit finishes one whatever
//! its status.
//!
//! **Where it runs.** On the event loop's thread, the only one AppKit and WebKit may be used from.
//! There is no controller to wait for: the web view is made at once and loads asynchronously, so a
//! build waits only for its own making, which `page-check` measures. The rule list is the one thing
//! a first build waits for, and the loop does not: it is compiled asynchronously, and the tick after
//! WebKit's answer builds. The web view's callbacks queue a `PageEvent` here and return.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use block2::RcBlock;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, Bool, Sel};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSEvent, NSEventModifierFlags, NSFloatingWindowLevel,
    NSNormalWindowLevel, NSPanel, NSResponder, NSScreen, NSWindow, NSWindowCollectionBehavior,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSBundle, NSDictionary, NSError, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKContentWorld, WKWebView, WKWebViewConfiguration,
    WKWebsiteDataStore,
};
use wry_mac as wry;
use wry::raw_window_handle::{AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use wry::{
    http, BackgroundThrottlingPolicy, NewWindowResponse, PageLoadEvent, PermissionResponse, WebView,
    WebViewBuilder, WebViewBuilderExtDarwin, WebViewBuilderExtMacos, WebViewExtMacOS,
};

use crate::backend::{cocoa_frame_top_left, own_id};
use crate::webview::{self, MacKey, PageEvent, Placement, Rect, Refusal, WindowKind};

/// How many events wait for the next tick at most, as on Windows: past this a page's messages are
/// dropped and counted (a load or a close never is).
const MAX_WAITING_EVENTS: usize = 4096;

/// How many refusals of one page are written to the log, as on Windows; past it they are counted.
const MAX_REFUSAL_LINES: u32 = 20;

/// The Dock claim every app page shares (`dock.rs`): made when the first is shown, let go when the
/// last is hidden or taken down.
const DOCK_OWNER: &str = "pages";

/// How many first-responder classes a page keeps for `page-check` (the log says each change).
const MAX_RESPONDERS: usize = 32;

thread_local! {
    /// What the pages' windows sent since the last tick, in order; emptied by [`take_events`].
    static EVENTS: RefCell<Vec<PageEvent>> = const { RefCell::new(Vec::new()) };
    /// Messages dropped because [`MAX_WAITING_EVENTS`] were waiting, since the last tick.
    static DROPPED: Cell<usize> = const { Cell::new(0) };
    /// Every built page, by its window's id: its state and its window, for a page leaving the front
    /// that came from another page (`webview::front_goes_to_app`) and for a show's outcome.
    static PAGES: RefCell<HashMap<isize, Built>> = RefCell::new(HashMap::new());
    /// The app pages counted as shown, for the Dock claim.
    static APP_SHOWN: RefCell<HashSet<isize>> = RefCell::new(HashSet::new());
    /// The one show whose request for the front has no outcome yet ([`Native::focus`]).
    static ASKED: RefCell<Option<Asked>> = const { RefCell::new(None) };
    /// A request for the front whose page left before it was answered, while it may still make this
    /// application the active one ([`became_active`]).
    static UNANSWERED: RefCell<Option<Unanswered>> = const { RefCell::new(None) };
    /// The rule list every page is built with ([`environment`]).
    static ENV: RefCell<Env> = const { RefCell::new(Env::Absent) };
}

fn push(ev: PageEvent) {
    EVENTS.with(|q| {
        let mut q = q.borrow_mut();
        if q.len() >= MAX_WAITING_EVENTS && matches!(ev, PageEvent::Message { .. }) {
            DROPPED.with(|d| d.set(d.get() + 1));
            return;
        }
        q.push(ev);
    });
}

/// Everything the pages' windows sent since the last call.
pub(crate) fn take_events() -> Vec<PageEvent> {
    let dropped = DROPPED.with(|d| d.replace(0));
    if dropped > 0 {
        crate::logging::line(
            "gui",
            &format!("{dropped} message(s) from pages were dropped: more than {MAX_WAITING_EVENTS} waited for one tick"),
        );
    }
    EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// One page's refusals, or its shim's notes: its title for the lines, and how many there were.
/// Shared by the web view's handlers, some of which wry wants `Send`.
#[derive(Clone)]
struct Refusals {
    title: Arc<str>,
    said: Arc<AtomicU32>,
}

impl Refusals {
    fn new(title: &str) -> Self {
        Refusals { title: Arc::from(title), said: Arc::new(AtomicU32::new(0)) }
    }

    fn say(&self, what: &str) {
        let n = self.said.fetch_add(1, Ordering::Relaxed);
        if n < MAX_REFUSAL_LINES {
            crate::logging::line("gui", &format!("page '{}': {what}", self.title));
        } else if n == MAX_REFUSAL_LINES {
            crate::logging::line(
                "gui",
                &format!("page '{}': {what} — the page's further lines of this kind are counted, not logged", self.title),
            );
        }
    }

    fn count(&self) -> u32 {
        self.said.load(Ordering::Relaxed)
    }
}

/// WebKit's version, from its bundle: always there on a Mac. `None` only where the bundle cannot
/// be read, which says nothing about whether a page can be built.
pub(crate) fn runtime_version() -> Option<String> {
    let bundle = NSBundle::bundleWithIdentifier(&NSString::from_str("com.apple.WebKit"))?;
    let read = |key: &str| {
        bundle
            .objectForInfoDictionaryKey(&NSString::from_str(key))
            .and_then(|v| v.downcast_ref::<NSString>().map(|s| s.to_string()))
    };
    match (read("CFBundleShortVersionString"), read("CFBundleVersion")) {
        (Some(short), Some(build)) => Some(format!("WebKit {short} ({build})")),
        (Some(v), None) | (None, Some(v)) => Some(format!("WebKit {v}")),
        (None, None) => None,
    }
}

/// The rule list every page of the session is built with.
enum Env {
    /// Not asked for yet, or a failure handed to a build: the next build asks again.
    Absent,
    /// Being compiled, since then; WebKit answers on this thread.
    Compiling { since: Instant },
    /// Compiled, and kept for the rest of the session.
    Ready(Retained<WKContentRuleList>),
    /// WebKit could not compile it, not yet handed to a build — and dropped at the end of a tick
    /// in which no build waits ([`drop_unclaimed_failure`]).
    Failed(String),
}

/// Where the rule list is, for a build that needs it: `webview_win.rs`'s answer, with the rule list
/// where Windows has WebView2's environment.
pub(crate) enum EnvNow {
    /// Compiled. The second field is Windows' "reused, retry once in a new one", which has no
    /// counterpart here: always `false`. Allowed dead: `webview_lua.rs` reads it once it takes this
    /// window as it takes webview_win.rs (TODO.md, stage 4); macos-check names it meanwhile.
    Ready(Retained<WKContentRuleList>, #[allow(dead_code)] bool),
    /// Being compiled; WebKit answers through the event loop, and the tick after that builds.
    Waiting,
    /// It could not be compiled; the next build asks again. A page is never built without it.
    Failed(String),
}

/// The rule list every page is built with: compiled, being compiled — the compilation started here
/// if nobody had asked for it — or failed, which is handed to the caller once and then forgotten, so
/// the next build asks again. Never waits: WebKit compiles it off the main thread and answers on it.
pub(crate) fn environment() -> EnvNow {
    let now = ENV.with(|e| match &*e.borrow() {
        Env::Ready(list) => Some(EnvNow::Ready(list.clone(), false)),
        Env::Compiling { .. } => Some(EnvNow::Waiting),
        Env::Failed(why) => Some(EnvNow::Failed(why.clone())),
        Env::Absent => None,
    });
    match now {
        Some(EnvNow::Failed(why)) => {
            ENV.with(|e| *e.borrow_mut() = Env::Absent);
            EnvNow::Failed(why)
        }
        Some(now) => now,
        None => compile(),
    }
}

/// At the end of a tick in which no page waits for the rule list: a failure WebKit answered that no
/// build took is dropped, so a build much later asks again rather than fail with a stale reason.
// Allowed dead: `webview_lua.rs` calls it once it takes this window as it takes webview_win.rs
// (TODO.md, stage 4); macos-check names it meanwhile.
#[allow(dead_code)]
pub(crate) fn drop_unclaimed_failure() {
    let why = ENV.with(|e| match &*e.borrow() {
        Env::Failed(why) => Some(why.clone()),
        _ => None,
    });
    if let Some(why) = why {
        ENV.with(|e| *e.borrow_mut() = Env::Absent);
        crate::logging::line(
            "gui",
            &format!("the pages' rule list's failure ({why}) was waited for by no page any more; the next build asks again"),
        );
    }
}

/// Starts compiling the rule list into the store beside the application, and answers where it is
/// afterwards: WebKit may answer inside the call.
fn compile() -> EnvNow {
    let Some(mtm) = MainThreadMarker::new() else {
        return EnvNow::Failed("the rule list was asked for off the main thread".to_string());
    };
    let folder = crate::portable::base_dir().join(webview::RULE_FOLDER);
    if let Err(e) = std::fs::create_dir_all(&folder) {
        return EnvNow::Failed(format!("its folder {} could not be made: {e}", folder.display()));
    }
    let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(&folder.to_string_lossy()), true);
    // SAFETY: a file URL of a folder that exists, on the main thread.
    let Some(store) = (unsafe { WKContentRuleListStore::storeWithURL(Some(&url), mtm) }) else {
        return EnvNow::Failed(format!("WebKit made no rule store in {}", folder.display()));
    };
    let since = Instant::now();
    ENV.with(|e| *e.borrow_mut() = Env::Compiling { since });
    crate::logging::line(
        "gui",
        &format!(
            "the pages' rule list is being compiled into {} — the event loop goes on meanwhile",
            folder.display()
        ),
    );
    let block = RcBlock::new(|list: *mut WKContentRuleList, error: *mut NSError| {
        guarded("the rule list's answer", || {
            // SAFETY: WebKit's own objects for the duration of the call, retained if kept.
            let list = unsafe { Retained::retain(list) };
            let error = unsafe { error.as_ref() }.map(|e| e.localizedDescription().to_string());
            compiled(list, error);
        });
    });
    // SAFETY: the identifier and the list are strings of ours; the store copies the block.
    unsafe {
        store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
            Some(&NSString::from_str(webview::RULE_LIST_ID)),
            Some(&NSString::from_str(webview::RULE_LIST)),
            Some(&block),
        );
    }
    match ENV.with(|e| match &*e.borrow() {
        Env::Ready(list) => EnvNow::Ready(list.clone(), false),
        Env::Failed(why) => EnvNow::Failed(why.clone()),
        _ => EnvNow::Waiting,
    }) {
        EnvNow::Failed(why) => {
            ENV.with(|e| *e.borrow_mut() = Env::Absent);
            EnvNow::Failed(why)
        }
        now => now,
    }
}

/// WebKit's answer: kept for the session, or the failure for the next build. An answer nobody
/// waits for any more is dropped.
fn compiled(list: Option<Retained<WKContentRuleList>>, error: Option<String>) {
    let since = ENV.with(|e| match &*e.borrow() {
        Env::Compiling { since } => Some(*since),
        _ => None,
    });
    let Some(since) = since else {
        crate::logging::line("gui", "WebKit answered for a rule list nobody waits for any more; dropped");
        return;
    };
    let ms = since.elapsed().as_millis();
    match (list, error) {
        (Some(list), None) => {
            crate::logging::line(
                "gui",
                &format!("the pages' rule list is compiled, {ms} ms after it was asked for; the pages waiting for it are built at the next tick"),
            );
            ENV.with(|e| *e.borrow_mut() = Env::Ready(list));
        }
        (_, error) => {
            let why = error.unwrap_or_else(|| "WebKit answered with no rule list".to_string());
            crate::logging::line("gui", &format!("the pages' rule list could not be compiled ({ms} ms): {why}"));
            ENV.with(|e| *e.borrow_mut() = Env::Failed(why));
        }
    }
}

/// What a build is made from, as on Windows.
pub(crate) struct BuildSpec {
    pub page: u64,
    pub build: u64,
    /// A tool page's panel or an app page's window.
    pub kind: WindowKind,
    pub title: String,
    pub scheme: String,
    /// The module's folder, canonical.
    pub root: PathBuf,
    pub entry_url: String,
}

/// The window, and its application, a page took the front from. `window` is `host.window`'s id — 0
/// when only the application could be named, whose own choice of window then stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Came {
    window: isize,
    pid: i32,
}

/// A built page, as [`PAGES`] keeps it: its state, and its window while it exists.
struct Built {
    state: Rc<PageState>,
    window: Weak<NSWindow>,
}

/// What a page's window and the host share: the page's own facts, and what its window's overrides
/// read and change. Held by the window (its ivars), by the [`Native`] and by [`PAGES`].
struct PageState {
    /// The window's id (`backend::own_id`): set once the window exists, before anything reads it.
    id: Cell<isize>,
    page: u64,
    build: u64,
    kind: WindowKind,
    title: String,
    refusals: Refusals,
    /// Where the page took the front from when it last took it; `None` until it first does.
    came_from: Cell<Option<Came>>,
    /// Whether the page has kept the front since it last took it from another window: set when it
    /// does ([`Native::focus`]), cleared when its window lets the keyboard go. Asked only for an
    /// app page (`WindowKind::gives_front_back`).
    kept: Cell<bool>,
    /// Shown by the host and not hidden since: an app page miniaturised, or in an application that
    /// is hidden, is still shown.
    shown: Cell<bool>,
    /// Whether the window has been given a place yet: until then a show without `avoid` centres it.
    placed: Cell<bool>,
    /// Whether its last place was over the rectangle to avoid (`webview::Placement::Over`).
    over: Cell<bool>,
    /// The web view, once built.
    wk: RefCell<Option<Weak<WKWebView>>>,
    /// The classes AppKit made the first responder while the page was shown, each change once.
    responders: RefCell<Vec<String>>,
    /// The keys of the table said in the log already (Command-Q, Command-H): once a page each.
    keys_said: Cell<u8>,
}

impl PageState {
    fn id(&self) -> isize {
        self.id.get()
    }

    fn webview(&self) -> Option<Retained<WKWebView>> {
        self.wk.borrow().as_ref().and_then(Weak::load)
    }
}

/// What a page's window class carries.
struct Ivars {
    state: Rc<PageState>,
}

/// A page's window class: the overrides both kinds share. A panel for a tool page and a window for
/// an app page are two classes, as AppKit has them; their behaviour is the functions below.
macro_rules! page_window_class {
    ($name:ident, $super:ident, $objc:literal) => {
        define_class!(
            // SAFETY: AppKit's own behaviour is kept where it is wanted — every override that needs
            // it calls `super` — and the class does not implement `Drop`.
            #[unsafe(super($super))]
            #[thread_kind = MainThreadOnly]
            #[name = $objc]
            #[ivars = Ivars]
            struct $name;

            impl $name {
                #[unsafe(method(canBecomeKeyWindow))]
                fn can_become_key_window(&self) -> Bool {
                    Bool::YES
                }

                #[unsafe(method(canBecomeMainWindow))]
                fn can_become_main_window(&self) -> Bool {
                    Bool::YES
                }

                /// Escape: nothing, as in the control tool's panel. A panel would close itself.
                #[unsafe(method(cancelOperation:))]
                fn cancel_operation(&self, _sender: Option<&AnyObject>) {}

                /// The close box, VoiceOver's close action and Command-W: the page hidden, not closed.
                #[unsafe(method(performClose:))]
                fn perform_close(&self, _sender: Option<&AnyObject>) {
                    guarded("the close box", || closed_by_user(self, &self.ivars().state));
                }

                #[unsafe(method(performKeyEquivalent:))]
                fn perform_key_equivalent(&self, event: &NSEvent) -> Bool {
                    // SAFETY: the superclass's own method, with the event AppKit handed over.
                    let handled: Bool = unsafe { msg_send![super(self), performKeyEquivalent: event] };
                    let mut taken = handled.as_bool();
                    if !taken {
                        guarded("a key equivalent", || taken = key_equivalent(self, &self.ivars().state, event));
                    }
                    Bool::new(taken)
                }

                #[unsafe(method(becomeKeyWindow))]
                fn become_key_window(&self) {
                    // SAFETY: the superclass's own method, as AppKit calls it.
                    let () = unsafe { msg_send![super(self), becomeKeyWindow] };
                    guarded("taking the keyboard", || became_key(self, &self.ivars().state));
                }

                #[unsafe(method(resignKeyWindow))]
                fn resign_key_window(&self) {
                    // SAFETY: the superclass's own method, as AppKit calls it.
                    let () = unsafe { msg_send![super(self), resignKeyWindow] };
                    guarded("letting the keyboard go", || resigned_key(self, &self.ivars().state));
                }

                #[unsafe(method(makeFirstResponder:))]
                fn make_first_responder(&self, responder: Option<&NSResponder>) -> Bool {
                    // SAFETY: the superclass's own method, with the responder AppKit was handed.
                    let made: Bool = unsafe { msg_send![super(self), makeFirstResponder: responder] };
                    // Asked first, so nothing of the page is read while the window is being set up
                    // or taken apart.
                    if self.isVisible() {
                        guarded("the first responder", || note_responder(self, &self.ivars().state));
                    }
                    made
                }
            }
        );
    };
}

page_window_class!(PagePanel, NSPanel, "AutomationPlatformPagePanel");
page_window_class!(PageWindow, NSWindow, "AutomationPlatformPageWindow");

/// Runs `f`, and says so in the log if it panicked rather than unwind into AppKit's frames.
fn guarded(what: &str, f: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
        crate::logging::line("gui", &format!("a page's window: a panic in {what}; carried on"));
    }
}

/// The page's window became the key window: a tool page floats while it has the keyboard, a show
/// that asked for the front has its answer, and the keyboard goes back where it was in the page.
fn became_key(window: &NSWindow, state: &PageState) {
    if state.kind == WindowKind::Tool {
        window.setLevel(NSFloatingWindowLevel);
    }
    if let Some(asked) = take_asked(state.id()) {
        crate::logging::line(
            "gui",
            &format!(
                "page '{}': got the keyboard, {} ms after the show asked for the front",
                state.title,
                asked.since.elapsed().as_millis()
            ),
        );
    }
    restore_focus(state);
}

/// The page's window let the keyboard go: a tool page drops to the normal level, so the window that
/// took the keyboard is drawn over it, and an app page has not kept the front.
fn resigned_key(window: &NSWindow, state: &PageState) {
    if state.kind == WindowKind::Tool {
        window.setLevel(NSNormalWindowLevel);
    }
    state.kept.set(false);
}

/// The keyboard put back where it was in the page (`webview::WEBKIT_RESTORE_FOCUS`), and where it is
/// said in the log.
fn restore_focus(state: &PageState) {
    let Some(wk) = state.webview() else { return };
    let title = state.title.clone();
    run_script(&wk, webview::WEBKIT_RESTORE_FOCUS, None, move |answer, error| {
        let line = match (answer, error) {
            (_, Some(e)) => format!("page '{title}': the keyboard could not be put back in the page: {e}"),
            (Some(a), None) => format!("page '{title}': the keyboard in the page is {a}"),
            (None, None) => format!("page '{title}': the keyboard in the page: no answer"),
        };
        crate::logging::line("gui", &line);
    });
}

/// The class AppKit made the first responder of the shown page's window, said when it changes.
/// Changes nothing: it is where to look when the keyboard is not where it should be (MW14).
fn note_responder(window: &NSWindow, state: &PageState) {
    let Some(responder) = window.firstResponder() else { return };
    let class = responder.class().name().to_string_lossy().into_owned();
    let mut seen = state.responders.borrow_mut();
    if seen.last() == Some(&class) {
        return;
    }
    if seen.len() < MAX_RESPONDERS {
        seen.push(class.clone());
    }
    drop(seen);
    crate::logging::line("gui", &format!("page '{}': AppKit's first responder is now a {class}", state.title));
}

/// The close box, VoiceOver's close action or Command-W: the page hidden, the front given back,
/// and the module told (`onClose("user")`). The page keeps its state for the next show.
fn closed_by_user(window: &NSWindow, state: &PageState) {
    if !window.isVisible() && !state.shown.get() {
        return;
    }
    leave(window, state, "closed by the user");
    if let Some(wk) = state.webview() {
        wk.setHidden(true);
    }
    push(PageEvent::Closed { page: state.page, build: state.build, why: "user" });
}

/// A key equivalent the page and WebKit left unhandled: the host's table (`webview::mac_key_of`),
/// by the key's characters without the modifiers and, where those are not the table's, by the ones
/// it types with Command held, as AppKit matches a menu's. Whether it was taken. Only for the window
/// with the keyboard: one AppKit offers it to as the main window while another has the keyboard —
/// the module manager's, say — leaves it to that one.
fn key_equivalent(window: &NSWindow, state: &PageState, event: &NSEvent) -> bool {
    if !window.isKeyWindow() {
        return false;
    }
    let flags = event.modifierFlags() & NSEventModifierFlags::DeviceIndependentFlagsMask;
    let ignoring = event.charactersIgnoringModifiers().map(|s| s.to_string()).unwrap_or_default();
    let typed = event.characters().map(|s| s.to_string()).unwrap_or_default();
    let key = webview::mac_key_of(
        state.kind,
        &ignoring,
        &typed,
        flags.contains(NSEventModifierFlags::Command),
        flags.contains(NSEventModifierFlags::Shift),
        flags.contains(NSEventModifierFlags::Option),
        flags.contains(NSEventModifierFlags::Control),
    );
    let Some(app) = super::own::app() else { return false };
    match key {
        MacKey::Pass => false,
        MacKey::Nothing => {
            let bit = if [&ignoring, &typed].iter().any(|c| c.eq_ignore_ascii_case("q")) { 1 } else { 2 };
            if state.keys_said.get() & bit == 0 {
                state.keys_said.set(state.keys_said.get() | bit);
                crate::logging::line(
                    "gui",
                    &format!(
                        "page '{}': {} does nothing in a tool page — the application keeps running and shown, \
                         and the DAW's own shortcuts stay the DAW's (said once a page)",
                        state.title,
                        if bit == 1 { "Command-Q" } else { "Command-H" }
                    ),
                );
            }
            true
        }
        MacKey::Close => {
            closed_by_user(window, state);
            true
        }
        MacKey::Edit(command) => {
            let action = Sel::register(command.selector());
            let sender: &AnyObject = window;
            // SAFETY: an action every responder may answer, to the first responder (no target),
            // with the window as the sender, as an Edit menu's item would send it.
            unsafe { app.sendAction_to_from(action, None, Some(sender)) }
        }
        MacKey::Miniaturise => {
            window.performMiniaturize(None);
            true
        }
        MacKey::Quit => {
            crate::logging::line(
                "gui",
                &format!("page '{}': Command-Q in an application page — the application quits, as from its Quit item", state.title),
            );
            crate::instance::request_quit();
            true
        }
        MacKey::Hide => {
            app.hide(None);
            true
        }
        MacKey::HideOthers => {
            app.hideOtherApplications(None);
            true
        }
    }
}

/// The page leaves the screen — hidden by its module, closed by the user, or taken down — and gives
/// the front back to where it took it from while it has the keyboard (`front::hand_front_to`, which
/// orders the window out whatever it decides). A show still waiting for its answer leaves its
/// request behind ([`unanswered`]). An app page's Dock claim goes last, once the window is out and
/// the front asked for: let go while the window still has the keyboard, the switch to an accessory
/// application could take it before it is given back. `how` is for the log.
fn leave(window: &NSWindow, state: &PageState, how: &str) {
    if let Some(asked) = take_asked(state.id()) {
        crate::logging::line(
            "gui",
            &format!(
                "page '{}': {how} {} ms after its show asked for the front, before it had the keyboard",
                state.title,
                asked.since.elapsed().as_millis()
            ),
        );
        unanswered(state, asked);
    }
    state.shown.set(false);
    let gives = state.kind.gives_front_back(state.kept.get());
    match (gives, state.came_from.get()) {
        (true, Some(came)) => {
            let to = goes_to(state.id(), came);
            crate::logging::line(
                "gui",
                &format!(
                    "page '{}': {how}; it took the front from window {} of pid {}{}, which gets it back if the \
                     page has the keyboard",
                    state.title,
                    to.window,
                    to.pid,
                    if to == came { "" } else { " (where the page it was shown in place of came from)" }
                ),
            );
            super::front::hand_front_to(to.pid, to.window, Some(window), super::front::HANDING);
        }
        (true, None) => {
            window.orderOut(None);
            crate::logging::line(
                "gui",
                &format!("page '{}': {how}; it never took the front, so nothing is handed back", state.title),
            );
        }
        (false, _) => {
            window.orderOut(None);
            crate::logging::line(
                "gui",
                &format!(
                    "page '{}': {how}; an application window the user came to some other way, so the system decides \
                     where the front goes",
                    state.title
                ),
            );
        }
    }
    if state.kind == WindowKind::Tool {
        window.setLevel(NSNormalWindowLevel);
    }
    if state.kind == WindowKind::App {
        dock_release(state.id());
    }
}

/// Where the front goes from page `this`, which came from `came`: through pages hidden since to the
/// window the first of them came from, with that window's application — or to the application,
/// where that window had no name (`webview::front_goes_to_app`).
fn goes_to(this: isize, came: Came) -> Came {
    PAGES.with(|pages| {
        let pages = pages.borrow();
        let shown = |w: &Weak<NSWindow>| w.load().is_some_and(|w| w.isVisible());
        let (window, pid) = webview::front_goes_to_app(this, (came.window, came.pid), pages.len(), |id| {
            pages.get(&id).map(|b| (shown(&b.window), b.state.came_from.get().map(|c| (c.window, c.pid))))
        });
        Came { window, pid }
    })
}

fn dock_claim(id: isize) {
    let first = APP_SHOWN.with(|s| {
        let mut s = s.borrow_mut();
        s.insert(id) && s.len() == 1
    });
    if first {
        crate::dock::want(DOCK_OWNER, "an application page is shown");
    }
}

fn dock_release(id: isize) {
    let last = APP_SHOWN.with(|s| {
        let mut s = s.borrow_mut();
        s.remove(&id) && s.is_empty()
    });
    if last {
        crate::dock::release(DOCK_OWNER, "the last application page is hidden");
    }
}

/// A show whose request for the front has no outcome yet.
struct Asked {
    id: isize,
    since: Instant,
    /// The event tap's count of keys pressed when it was asked (`tap::pressed`).
    pressed: u32,
    /// The application a hand-over of ours had asked to activate when this show asked for the
    /// front, and whose activation was not announced yet (`front::waiting_for`): asked first, it
    /// comes first, and says nothing of this request. Its first announcement is passed over.
    before: Option<i32>,
}

/// A request for the front whose page left before it was answered ([`UNANSWERED`]).
struct Unanswered {
    /// The page's title, for the log.
    title: String,
    /// Where the page would have given the front back.
    to: Came,
    /// When the show asked.
    since: Instant,
    /// As [`Asked::before`].
    before: Option<i32>,
}

/// Page `state` left before the request for the front its show made was answered: remembered, with
/// where the page would have given the front back, for [`became_active`]. A page that never knew
/// where the front was leaves nothing to give back.
fn unanswered(state: &PageState, asked: Asked) {
    let Some(came) = state.came_from.get() else { return };
    let to = goes_to(state.id(), came);
    UNANSWERED.with(|u| {
        *u.borrow_mut() = Some(Unanswered { title: state.title.clone(), to, since: asked.since, before: asked.before });
    });
}

/// A newer request for the front — a show, a hand-over, this application taking the front itself
/// (`app::activate_self`, `front::hand_front_to`): a request whose page left is forgotten, as the
/// newer one decides where the front goes. One borrow when none is remembered.
pub(super) fn superseded() {
    UNANSWERED.with(|u| u.borrow_mut().take());
}

/// AppKit said this application became the active one (`front.rs`, in-process, in order). A request
/// whose page left before it was answered may be what made it so: with no window of ours on screen
/// that could take the keyboard — and no app page counted as shown, which the Dock brings back —
/// this application would be in front with nothing in it, and the front goes back to where the page
/// would have given it, as `host.window.focus` hands it. With such a window, it stays. Either way the
/// request is answered. One borrow when none is remembered.
pub(super) fn became_active() {
    let Some(left) = UNANSWERED.with(|u| u.borrow_mut().take()) else { return };
    let Some(app) = super::own::app() else { return };
    let ms = left.since.elapsed().as_millis();
    let window_for_keys = app.windows().iter().any(|w| w.isVisible() && w.canBecomeKeyWindow());
    let app_page = APP_SHOWN.with(|s| !s.borrow().is_empty());
    if !app.isActive() || window_for_keys || app_page {
        crate::logging::line(
            "gui",
            &format!(
                "page '{}': this application became the active one {ms} ms after the show, after the page had left; {}",
                left.title,
                if app.isActive() {
                    "a window of its own is there to be in, so it stays"
                } else {
                    "it no longer is, so nothing is given back"
                }
            ),
        );
        return;
    }
    crate::logging::line(
        "gui",
        &format!(
            "page '{}': this application became the active one {ms} ms after the show, after the page had left, and has \
             no window on screen to be in — the front goes back to window {} of pid {}",
            left.title, left.to.window, left.to.pid
        ),
    );
    super::front::hand_front_to(left.to.pid, left.to.window, None, super::front::HANDING);
}

fn take_asked(id: isize) -> Option<Asked> {
    ASKED.with(|a| {
        let mut a = a.borrow_mut();
        if a.as_ref().is_some_and(|x| x.id == id) {
            a.take()
        } else {
            None
        }
    })
}

/// The workspace named process `pid` the active application (`watch.rs`). A show still waiting for
/// the front, with the page not key, has its answer when another application's activation is
/// announced: it did not get it — unless that is the activation a hand-over of ours asked for
/// before the show, which is passed over once ([`Asked::before`]). Announced after this
/// application's request, it also means that request no longer decides where the front is: a
/// request whose page left is forgotten, with the same exception. One borrow each when nothing
/// waits.
pub(super) fn activated(pid: i32) {
    if pid == std::process::id() as i32 {
        return;
    }
    UNANSWERED.with(|u| {
        let mut u = u.borrow_mut();
        match u.as_mut() {
            Some(left) if left.before == Some(pid) => left.before = None,
            Some(_) => *u = None,
            None => {}
        }
    });
    let asked = ASKED.with(|a| {
        let mut a = a.borrow_mut();
        match a.as_mut() {
            Some(x) if x.before == Some(pid) => {
                x.before = None;
                None
            }
            _ => a.take(),
        }
    });
    let Some(asked) = asked else { return };
    let name = super::ax::exe_for_pid(pid);
    not_got(asked, &format!("the front went to {name} (pid {pid}) instead"), false);
}

/// The tick (`MacBackend::pump_pending`): a show still waiting for the front has its answer when the
/// event tap has seen a key pressed since it asked while none of our windows has the keyboard — that
/// key went to another application. Without the tap running, this never happens. One borrow when
/// nothing waits.
pub(super) fn refresh() {
    let typed = ASKED.with(|a| a.borrow().as_ref().is_some_and(|x| x.pressed != super::tap::pressed()));
    if !typed || super::own::has_keyboard() {
        return;
    }
    let Some(asked) = ASKED.with(|a| a.borrow_mut().take()) else { return };
    let front = NSWorkspace::sharedWorkspace().frontmostApplication().map(|a| a.processIdentifier());
    let name = front.map(super::ax::exe_for_pid).unwrap_or_else(|| "nothing".to_string());
    not_got(asked, &format!("a key went to {name}"), true);
}

/// A show did not get the front: said; a tool page drops to the normal level, and one placed over
/// the rectangle to avoid is hidden again — Windows' rule, as the page would cover what the
/// application's own clicks and screen reads land on. An app page the user has open stays as and
/// where it is. `on_its_way`: nothing has been announced since the request, which may still make
/// this application the active one — a page hidden here leaves it behind ([`unanswered`]).
fn not_got(asked: Asked, why: &str, on_its_way: bool) {
    let found = PAGES.with(|p| p.borrow().get(&asked.id).map(|b| (b.state.clone(), b.window.load())));
    let Some((state, Some(window))) = found else { return };
    let ms = asked.since.elapsed().as_millis();
    let hidden = state.kind == WindowKind::Tool && state.over.get();
    if state.kind == WindowKind::Tool {
        window.setLevel(NSNormalWindowLevel);
    }
    if hidden {
        window.orderOut(None);
        state.shown.set(false);
        if let Some(wk) = state.webview() {
            wk.setHidden(true);
        }
        if on_its_way {
            unanswered(&state, asked);
        }
    }
    crate::logging::line(
        "gui",
        &format!(
            "page '{}': {why}, {ms} ms after the show asked for the front — the page did not get the keyboard{}",
            state.title,
            if hidden { "; it was over the rectangle to avoid, so it is hidden again" } else { "" }
        ),
    );
}

/// A page's content view as the window handle wry builds into.
struct Parent(NonNull<std::ffi::c_void>);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(self.0));
        // SAFETY: the view outlives this borrow — `build` holds its window for the whole call.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// One built page: its window and its web view. Taken down when the last reference to it goes, by
/// [`Drop`].
pub(crate) struct Native {
    window: Retained<NSWindow>,
    state: Rc<PageState>,
    /// Dropped first, in `Drop`: wry lets go of its handlers and takes its view out of the window.
    webview: ManuallyDrop<WebView>,
    wk: Retained<WKWebView>,
    /// The window's number, for the own-window table (`own.rs`).
    number: u32,
}

impl Drop for Native {
    fn drop(&mut self) {
        // Taken down in front — a module's `close`, or its module reloaded — the window it came from
        // gets the front back first.
        if self.window.isVisible() || self.state.shown.get() {
            leave(&self.window, &self.state, "taken down");
        }
        dock_release(self.state.id());
        let _ = PAGES.try_with(|p| p.borrow_mut().remove(&self.state.id()));
        super::own::note_page(self.number, false);
        // SAFETY: dropped here, once; nothing uses the web view after this.
        unsafe { ManuallyDrop::drop(&mut self.webview) };
        self.window.orderOut(None);
        // Not released by closing (`releasedWhenClosed` is off): this `Retained` holds the last
        // reference of ours.
        self.window.close();
        let n = self.state.refusals.count();
        if n > MAX_REFUSAL_LINES {
            crate::logging::line(
                "gui",
                &format!("page '{}': {n} refusals in all, {} of them not logged", self.state.title, n - MAX_REFUSAL_LINES),
            );
        }
    }
}

/// The document a page is navigating to and whether its file was answered with anything but 200 —
/// what the scheme handler and the page-load handler share to tell a load that failed.
#[derive(Default)]
struct Doc {
    navigating: Option<String>,
    failed: bool,
}

/// `uri` without its fragment, which no request carries.
fn without_fragment(uri: &str) -> &str {
    uri.split('#').next().unwrap_or(uri)
}

/// Builds a page with the rule list [`environment`] compiled: its window, hidden, and its web view,
/// which starts loading at once. Waits for nothing but the web view's own making.
pub(crate) fn build(spec: &BuildSpec, rules: &WKContentRuleList) -> Result<Native, String> {
    let mtm = MainThreadMarker::new().ok_or_else(|| "a page was built off the main thread".to_string())?;
    let refusals = Refusals::new(&spec.title);
    let state = Rc::new(PageState {
        id: Cell::new(0),
        page: spec.page,
        build: spec.build,
        kind: spec.kind,
        title: spec.title.clone(),
        refusals: refusals.clone(),
        came_from: Cell::new(None),
        kept: Cell::new(false),
        shown: Cell::new(false),
        placed: Cell::new(false),
        over: Cell::new(false),
        wk: RefCell::new(None),
        responders: RefCell::new(Vec::new()),
        keys_said: Cell::new(0),
    });
    let window = make_window(mtm, spec.kind, &spec.title, &state)?;
    let number = u32::try_from(window.windowNumber()).unwrap_or(0);

    // A fresh configuration every build: a scheme handler cannot be registered twice in one, and
    // AppKit raises if it is. A data store that keeps nothing, no windows opened by script, and
    // the rule list.
    // SAFETY: WebKit's own objects, made and set up on the main thread.
    let config = unsafe {
        let config = WKWebViewConfiguration::new(mtm);
        config.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
        config.preferences().setJavaScriptCanOpenWindowsAutomatically(false);
        config.userContentController().addContentRuleList(rules);
        config
    };

    let (page, build) = (spec.page, spec.build);
    let origin = webview::webkit_origin_for(&spec.scheme);
    let notes = Refusals::new(&spec.title);
    let doc = Arc::new(Mutex::new(Doc::default()));
    let (root, scheme, served, served_doc) = (spec.root.clone(), spec.scheme.clone(), refusals.clone(), doc.clone());
    let (ipc_origin, ipc_refusals) = (origin.clone(), refusals.clone());
    let (nav_origin, nav_refusals, nav_doc) = (origin.clone(), refusals.clone(), doc.clone());
    let (load_origin, load_refusals, load_doc) = (origin.clone(), refusals.clone(), doc);
    let (win_refusals, download_refusals, asked_refusals) = (refusals.clone(), refusals.clone(), refusals.clone());
    let gone_title = spec.title.clone();
    let built = WebViewBuilder::new()
        .with_webview_configuration(config)
        .with_url(spec.entry_url.clone())
        .with_custom_protocol(spec.scheme.clone(), move |_id, request| {
            let uri = request.uri().to_string();
            let (response, status) = respond(&root, &scheme, &request, &served);
            if status != 200 {
                let mut d = served_doc.lock().unwrap_or_else(|e| e.into_inner());
                if d.navigating.as_deref() == Some(without_fragment(&uri)) {
                    // The document being navigated to: a load that failed, which WebKit finishes
                    // all the same.
                    d.failed = true;
                    push(PageEvent::LoadFailed { page, build, why: format!("its file was answered {status}: {uri}") });
                }
            }
            response
        })
        .with_ipc_handler(move |request: http::Request<String>| {
            // Only from the page's own documents, no larger than a message may be, and a note of
            // the shim's own (`webview::SHIM_NOTE`) to the log, never to the module.
            let source = request.uri().to_string();
            if !webview::own_origin(&source, &ipc_origin) {
                ipc_refusals.say(&format!("a message from {source} was dropped: not the page's own origin"));
                return;
            }
            let text = request.into_body();
            if text.len() > webview::MAX_MESSAGE_BYTES {
                ipc_refusals.say(&format!(
                    "a message of {} bytes was dropped — the most one may be is {}",
                    text.len(),
                    webview::MAX_MESSAGE_BYTES
                ));
                return;
            }
            if let Some(note) = webview::shim_note(&text) {
                notes.say(&format!("its platform object says: {}", webview::escape_controls(note)));
                return;
            }
            push(PageEvent::Message { page, build, text });
        })
        .with_initialization_script(webview::platform_shim(webview::Engine::WebKit))
        .with_navigation_handler(move |uri: String| {
            let own = webview::own_origin(&uri, &nav_origin);
            if own {
                let mut d = nav_doc.lock().unwrap_or_else(|e| e.into_inner());
                d.navigating = Some(without_fragment(&uri).to_string());
                d.failed = false;
            } else {
                nav_refusals.say(&format!("a navigation to {uri} was refused — a page stays on its own files"));
            }
            own
        })
        .with_on_page_load_handler(move |event, url| {
            if !matches!(event, PageLoadEvent::Finished) {
                return;
            }
            let failed = {
                let mut d = load_doc.lock().unwrap_or_else(|e| e.into_inner());
                d.navigating = None;
                std::mem::take(&mut d.failed)
            };
            if failed {
                return;
            }
            if webview::own_origin(&url, &load_origin) {
                push(PageEvent::Loaded { page, build });
            } else {
                load_refusals.say(&format!(
                    "it went to {url}, which is not a file of its own and could not be refused — the page is taken \
                     down, and the next show builds it again"
                ));
                push(PageEvent::Closed { page, build, why: "failed" });
            }
        })
        .with_on_web_content_process_terminate_handler(move || {
            crate::logging::line(
                "gui",
                &format!("page '{gone_title}': its WebKit content process ended — the page is gone, and the next show builds it again"),
            );
            push(PageEvent::Closed { page, build, why: "failed" });
        })
        .with_new_window_req_handler(move |uri, _| {
            win_refusals.say(&format!("a new window for {uri} was refused"));
            NewWindowResponse::Deny
        })
        .with_download_started_handler(move |uri, _| {
            download_refusals.say(&format!("a download of {uri} was refused"));
            false
        })
        .with_permission_handler(move |kind| {
            asked_refusals.say(&format!("it asked for {kind:?}; refused"));
            PermissionResponse::Deny
        })
        .with_devtools(false)
        // Built hidden from the keyboard: this revision of wry then neither makes the web view the
        // first responder nor activates the application (#1866). Showing does both, when asked.
        .with_focused(false)
        .with_visible(true)
        .with_back_forward_navigation_gestures(false)
        // No preview of a link's target on a force click: WebKit's own, of a page that is not the
        // page's own, which the navigation handler might never be asked about.
        .with_allow_link_preview(false)
        .with_accept_first_mouse(false)
        .with_background_throttling(BackgroundThrottlingPolicy::Disabled)
        .build(&match window.contentView() {
            Some(view) => Parent(NonNull::from(&*view).cast()),
            None => {
                window.close();
                return Err("AppKit gave the page's window no content view".to_string());
            }
        });
    let webview = match built {
        Ok(w) => w,
        Err(e) => {
            window.close();
            return Err(format!("{e}"));
        }
    };
    let wk: Retained<WKWebView> = Retained::into_super(webview.webview());
    state.wk.replace(Some(Weak::from_retained(&wk)));
    PAGES.with(|p| p.borrow_mut().insert(state.id(), Built { state: state.clone(), window: Weak::from_retained(&window) }));
    super::own::note_page(number, true);
    crate::logging::line(
        "gui",
        &format!(
            "page '{}': {} (window {}), built with WebKit",
            spec.title,
            match spec.kind {
                WindowKind::Tool => "a tool page's panel, which makes this application the active one when it takes the keyboard",
                WindowKind::App => "an application page's window",
            },
            state.id()
        ),
    );
    Ok(Native { window, state, webview: ManuallyDrop::new(webview), wk, number })
}


/// The window a page of `kind` is in (`webview::WindowKind::recipe`), made at once so that the
/// window server numbers it, at the page's size, hidden, carrying `state` — whose id it fills in.
fn make_window(mtm: MainThreadMarker, kind: WindowKind, title: &str, state: &Rc<PageState>) -> Result<Retained<NSWindow>, String> {
    let recipe = kind.recipe();
    let mut style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable;
    if recipe.minimise {
        style |= NSWindowStyleMask::Miniaturizable;
    }
    if recipe.panel {
        style |= NSWindowStyleMask::UtilityWindow;
    }
    let (w, h) = webview::DEFAULT_SIZE;
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(f64::from(w), f64::from(h)));
    // Each kind is told at once that closing it does not release it — before anything can close it,
    // the failure below included: an `NSWindow` is released by its own close otherwise, and its
    // `Retained` would release it again.
    let window: Retained<NSWindow> = if recipe.panel {
        let this = PagePanel::alloc(mtm).set_ivars(Ivars { state: state.clone() });
        // SAFETY: NSPanel's designated initialiser, with a style, a backing and `defer: NO`.
        let panel: Retained<PagePanel> = unsafe {
            msg_send![super(this), initWithContentRect: rect, styleMask: style, backing: NSBackingStoreType::Buffered, defer: false]
        };
        // SAFETY: held by its `Retained` for as long as it is used, and closed only when it is
        // taken down — so AppKit must not release it as well.
        unsafe { panel.setReleasedWhenClosed(false) };
        // A floating panel, as the control tool's is; `floatingPanel` sets the floating level, and
        // the level follows the keyboard from here (see the file).
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(false);
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        Retained::into_super(Retained::into_super(panel))
    } else {
        let this = PageWindow::alloc(mtm).set_ivars(Ivars { state: state.clone() });
        // SAFETY: NSWindow's designated initialiser, with a style, a backing and `defer: NO`.
        let window: Retained<PageWindow> = unsafe {
            msg_send![super(this), initWithContentRect: rect, styleMask: style, backing: NSBackingStoreType::Buffered, defer: false]
        };
        // SAFETY: as the panel's, above.
        unsafe { window.setReleasedWhenClosed(false) };
        Retained::into_super(window)
    };
    let Some(id) = own_id(window.windowNumber() as i64) else {
        window.close();
        return Err("the window server gave the page's window no number".to_string());
    };
    state.id.set(id);
    window.setHidesOnDeactivate(false);
    window.setTitle(&NSString::from_str(title));
    let (mw, mh) = webview::MIN_SIZE;
    window.setMinSize(NSSize::new(f64::from(mw), f64::from(mh)));
    window.setLevel(NSNormalWindowLevel);
    Ok(window)
}

/// One request of a page, answered from the module's folder (`webview::serve`), and its status.
/// Anything but a file is a line in the page's refusals.
fn respond(
    root: &Path,
    scheme: &str,
    request: &http::Request<Vec<u8>>,
    refusals: &Refusals,
) -> (http::Response<Cow<'static, [u8]>>, u16) {
    let uri = request.uri().to_string();
    let served = webview::serve(root, scheme, request.method().as_str(), &uri);
    let status = served.status;
    if status != 200 {
        refusals.say(&format!("{} {uri}: {status}", request.method()));
    }
    let mut response = http::Response::builder().status(status);
    for (name, value) in served.headers {
        response = response.header(name, value);
    }
    let response = response.body(Cow::Owned(served.body)).unwrap_or_else(|e| {
        // Not reached with the headers `serve` makes; if it were, the answer still carries the
        // policy rather than being a bare 200.
        refusals.say(&format!("the answer to {uri} could not be made ({e}); answered 500"));
        let mut failed = http::Response::builder().status(500);
        for (name, value) in webview::base_headers() {
            failed = failed.header(name, value);
        }
        failed.body(Cow::Borrowed(&[][..])).unwrap_or_else(|_| http::Response::new(Cow::Borrowed(&[][..])))
    });
    (response, status)
}

/// Runs `body`, a constant script, as an asynchronous function in the page's own world, with
/// `args` as its parameters — data, never part of the script — and hands `done` its answer as text,
/// or the error WebKit gave.
fn run_script(
    wk: &WKWebView,
    body: &str,
    args: Option<&NSDictionary<NSString, AnyObject>>,
    done: impl Fn(Option<String>, Option<String>) + 'static,
) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
        guarded("a script's answer", || {
            // SAFETY: WebKit's own objects, for the duration of the call.
            let value = unsafe { value.as_ref() }.and_then(|v| v.downcast_ref::<NSString>()).map(|s| s.to_string());
            let error = unsafe { error.as_ref() }.map(|e| e.localizedDescription().to_string());
            done(value, error);
        });
    });
    // SAFETY: a constant body in the page's world, the main frame, on the main thread; WebKit
    // copies the block.
    unsafe {
        wk.callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(
            &NSString::from_str(body),
            args,
            None,
            &WKContentWorld::pageWorld(mtm),
            Some(&block),
        );
    }
}

/// The primary display's height in points, which every flip between AppKit's frames and the host's
/// top-left points goes by; 0 with no display.
fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm).firstObject().map_or(0.0, |s| s.frame().size.height)
}

/// An AppKit rectangle as the host's top-left points.
fn top_left(r: NSRect, primary_h: f64) -> Rect {
    let (x, y, w, h) = cocoa_frame_top_left(r.origin.x, r.origin.y, r.size.width, r.size.height, primary_h);
    Rect { x, y, w, h }
}

/// The host's top-left points as an AppKit rectangle.
fn cocoa_rect(r: Rect, primary_h: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(f64::from(r.x), primary_h - f64::from(r.y) - f64::from(r.h)),
        NSSize::new(f64::from(r.w), f64::from(r.h)),
    )
}

/// The work areas of every display — each one's frame without the menu bar and the Dock — in the
/// host's points.
fn work_areas(mtm: MainThreadMarker) -> Vec<Rect> {
    let h = primary_height(mtm);
    NSScreen::screens(mtm).iter().map(|s| top_left(s.visibleFrame(), h)).collect()
}

/// The window and application in front before a page takes the front: `host.window`'s answer, or,
/// where accessibility cannot name the window, the application the workspace names.
fn front_before() -> Option<Came> {
    if let Some(f) = super::ax::foreground_window() {
        return Some(Came { window: f.id, pid: f.pid as i32 });
    }
    NSWorkspace::sharedWorkspace().frontmostApplication().map(|a| Came { window: 0, pid: a.processIdentifier() })
}

// `hwnd`, `plan`, `focus` and `memory` are allowed dead: `webview_lua.rs` calls them once it takes
// this window as it takes webview_win.rs (TODO.md, stage 4); macos-check names them meanwhile.
impl Native {
    /// The page's window as `host.window` names it (`backend::own_id`).
    #[allow(dead_code)]
    pub(crate) fn hwnd(&self) -> isize {
        self.state.id()
    }

    /// Where the window goes beside `avoid`, in points — at its full size, or smaller where it has
    /// to be — or, where nothing is clear of it, at its smallest in the corner of that display where
    /// it covers the least of it (`webview::place`). Decided, not done: [`Native::put`] puts it
    /// there. `None` without `avoid`, and with no display.
    #[allow(dead_code)]
    pub(crate) fn plan(&self, avoid: Option<Rect>) -> Option<Placement> {
        let a = avoid?;
        let mtm = MainThreadMarker::new()?;
        webview::place(webview::DEFAULT_SIZE, webview::MIN_SIZE, a, &work_areas(mtm))
    }

    /// Puts the window where [`Native::plan`] decided, for a `show` with `avoid`; without `avoid`,
    /// the first time, in the middle of the display the window with the keyboard is on. A
    /// miniaturised app page is brought back first.
    pub(crate) fn put(&self, avoid: Option<Rect>, plan: Option<Placement>) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let h = primary_height(mtm);
        match (avoid, plan) {
            (Some(_), Some(p)) => {
                if self.state.kind == WindowKind::App && self.window.isMiniaturized() {
                    self.window.deminiaturize(None);
                }
                self.window.setFrame_display(cocoa_rect(p.rect(), h), true);
                self.state.placed.set(true);
                self.state.over.set(matches!(p, Placement::Over { .. }));
            }
            (Some(_), None) => {
                crate::logging::line(
                    "gui",
                    &format!("page '{}': no display's area could be read; the window stays where it is", self.state.title),
                );
            }
            (None, _) if !self.state.placed.get() => {
                if let Some(area) = NSScreen::mainScreen(mtm).map(|s| top_left(s.visibleFrame(), h)) {
                    let now = top_left(self.window.frame(), h);
                    let r = webview::centred((now.w, now.h), &area);
                    self.window.setFrame_display(cocoa_rect(r, h), true);
                }
                self.state.placed.set(true);
                self.state.over.set(false);
            }
            (None, _) => {}
        }
    }

    /// Shows the window, and with `focus` takes the front and puts the keyboard into the document
    /// ([`Native::focus`]); [`Refusal::Declined`] when the system left it without the keyboard at
    /// once. Without `focus`, ordered in front of the other windows at the normal level without
    /// taking the front — "raised above the ordinary windows", as on Windows. An app page claims the
    /// Dock before it is shown, and a miniaturised one is brought back.
    pub(crate) fn show(&self, focus: bool) -> Result<(), Refusal> {
        let _ = self.webview.set_visible(true);
        if self.state.kind == WindowKind::App {
            dock_claim(self.state.id());
            if self.window.isMiniaturized() {
                self.window.deminiaturize(None);
            }
        }
        self.state.shown.set(true);
        if focus {
            return if self.take_front() { Ok(()) } else { Err(Refusal::Declined) };
        }
        if self.state.kind == WindowKind::Tool && !self.window.isKeyWindow() {
            self.window.setLevel(NSNormalWindowLevel);
        }
        self.window.orderFrontRegardless();
        Ok(())
    }

    /// Brings the shown page to the front with the keyboard in its document ([`Native::take_front`]);
    /// `false` when it is not shown, or when the system declined at once.
    #[allow(dead_code)]
    pub(crate) fn focus(&self) -> bool {
        self.is_shown() && self.take_front()
    }

    /// The control tool's sequence for its panel: this application asked for the front
    /// (`app::activate_self`), the window made key and ordered in front regardless, and the web view
    /// made the first responder. At the normal level: the tool sets its panel floating first, and
    /// here a page floats once it has the keyboard (`became_key`), so a request the system declines
    /// without a word leaves no floating page over the plug-in window. `true` when that was asked —
    /// what comes of it is an event (see the file) — and `false` when this application was the
    /// active one already and the window is still not key: declined by the system. Where the front
    /// came from is kept, to give it back. An app page comes back from the Dock and from a hidden
    /// application first.
    fn take_front(&self) -> bool {
        let Some(app) = super::own::app() else { return false };
        let w = &*self.window;
        let id = self.state.id();
        if self.state.kind == WindowKind::App {
            if app.isHidden() {
                app.unhide(None);
            }
            if w.isMiniaturized() {
                w.deminiaturize(None);
            }
        }
        let before = front_before();
        if let Some(came) = before.filter(|c| c.window != id) {
            self.state.came_from.set(Some(came));
        }
        let was_active = app.isActive();
        // The outcome is the workspace's to tell as well, and AppKit's for a request whose page
        // leaves first; both waited for from before the request.
        super::watch::hook_activations();
        super::front::hook_own();
        // Read before `activate_self` gives that hand-over up: its activation is still on its way.
        let earlier = super::front::waiting_for();
        ASKED.with(|a| {
            let pressed = super::tap::pressed();
            *a.borrow_mut() = Some(Asked { id, since: Instant::now(), pressed, before: earlier });
        });
        super::app::activate_self();
        w.makeKeyAndOrderFront(None);
        w.orderFrontRegardless();
        if !w.makeFirstResponder(Some(&self.wk)) {
            crate::logging::line("gui", &format!("page '{}': AppKit did not make the web view the first responder", self.state.title));
        }
        // Taken from another window: kept until the window lets the keyboard go. Already in front,
        // it stays as it was.
        if before.is_none_or(|c| c.window != id) {
            self.state.kept.set(true);
        }
        if w.isKeyWindow() {
            // Its `becomeKeyWindow` has said so, inside the calls above.
            let _ = take_asked(id);
            return true;
        }
        if was_active {
            let _ = take_asked(id);
            crate::logging::line(
                "gui",
                &format!(
                    "page '{}': this application is the active one and the window is not key after it was made so — \
                     declined by the system",
                    self.state.title
                ),
            );
            return false;
        }
        crate::logging::line(
            "gui",
            &format!(
                "page '{}': the front asked for — whether it comes is said when AppKit makes the window key, or when \
                 another application's activation or a key comes first",
                self.state.title
            ),
        );
        true
    }

    /// Hides the window; hidden while it has the keyboard, it gives the front back first.
    pub(crate) fn hide(&self) {
        if self.window.isVisible() || self.state.shown.get() {
            leave(&self.window, &self.state, "hidden");
        }
        self.after_hidden();
    }

    /// The window is hidden (by `hide`, or by the user's close box): its web view is hidden too.
    pub(crate) fn after_hidden(&self) {
        let _ = self.webview.set_visible(false);
    }

    /// Asked of AppKit: the window on screen — or, for an app page shown by the host, miniaturised
    /// or in this application hidden, which brings it back with itself.
    pub(crate) fn is_shown(&self) -> bool {
        if self.window.isVisible() {
            return true;
        }
        let hidden = super::own::app().is_some_and(|a| a.isHidden());
        self.state.kind == WindowKind::App && self.state.shown.get() && (self.window.isMiniaturized() || hidden)
    }

    /// Whether the window is the one AppKit names at the point in its middle — on top there rather
    /// than covered — for the log.
    pub(crate) fn on_top_at_centre(&self) -> bool {
        let Some(mtm) = MainThreadMarker::new() else { return false };
        let f = self.window.frame();
        let centre = NSPoint::new(f.origin.x + f.size.width / 2.0, f.origin.y + f.size.height / 2.0);
        NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(centre, 0, mtm) == self.window.windowNumber()
    }

    /// Hands the page one message: JSON the host encoded, as the one argument of a constant script
    /// (`webview::WEBKIT_DELIVER`) — no script is built from it. It arrives as `platform.onmessage`'s
    /// argument. Asynchronous: a message WebKit could not hand over is a line in the page's
    /// refusals.
    pub(crate) fn post_json(&self, json: &str) -> Result<(), String> {
        if MainThreadMarker::new().is_none() {
            return Err("a message was posted off the main thread".to_string());
        }
        let key = NSString::from_str("m");
        let text = NSString::from_str(json);
        let value: &AnyObject = &text;
        let args = NSDictionary::<NSString, AnyObject>::from_slices(&[&*key], &[value]);
        let refusals = self.state.refusals.clone();
        run_script(&self.wk, webview::WEBKIT_DELIVER, Some(&args), move |_, error| {
            if let Some(e) = error {
                refusals.say(&format!("a message could not be handed to it: {e}"));
            }
        });
        Ok(())
    }

    /// What the page's processes hold: not this application's to measure on a Mac — WebKit's
    /// content and networking processes are the system's, started for this application's web views
    /// and not its own — so always `None`, and said once a session.
    #[allow(dead_code)]
    pub(crate) fn memory(&self) -> Option<String> {
        static SAID: AtomicBool = AtomicBool::new(false);
        if !SAID.swap(true, Ordering::Relaxed) {
            crate::logging::line("gui", "a page's memory is not measured on a Mac: WebKit's processes are not this application's");
        }
        None
    }

    /// The window itself, for `page-check`.
    pub(super) fn window(&self) -> &NSWindow {
        &self.window
    }

    /// The document the web view shows, for `page-check`.
    pub(super) fn url(&self) -> String {
        // SAFETY: a plain query of the web view's own URL.
        unsafe { self.wk.URL() }.and_then(|u| u.absoluteString()).map(|s| s.to_string()).unwrap_or_default()
    }

    /// How many refusals the page has had, for `page-check`.
    pub(super) fn refused(&self) -> u32 {
        self.state.refusals.count()
    }

    /// The first-responder classes AppKit has had while the page was shown, for `page-check`.
    pub(super) fn responders(&self) -> Vec<String> {
        self.state.responders.borrow().clone()
    }
}
