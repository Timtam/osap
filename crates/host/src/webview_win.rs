//! `host.gui.webview` on Windows: WebView2, through wry, inside a wx window of our own — a tool
//! window, or with `window = "app"` an ordinary one (`webview::WindowKind`).
//!
//! **The window.** A wx frame with no owner. A tool page's has the tool-window style
//! (`WS_EX_TOOLWINDOW`), so it has no taskbar button and no Alt+Tab entry. An app page's has the
//! ordinary frame style — an Alt+Tab entry with the page's title, a place on the taskbar (under
//! the application's own button where Windows combines a program's windows, as Windows 11 does by
//! default), a minimise and a maximise box — and is otherwise handled as a tool page is, except
//! that a `show` never hides or moves one the user has open without bringing it to the front
//! (`webview_lua::show_built`). wx shows a tool window with
//! `SW_SHOWNA`, which does not activate it and leaves it where it was in the stacking order; an
//! ordinary frame it shows with `SW_SHOW`, which activates it, so an app page is shown as wx
//! shows a disabled frame — `SW_SHOWNA` — and enabled again at once ([`Native::unhide`]). Then
//! `show` raises it to the top without activating it — a page shown without the front would
//! otherwise stay under whatever covered it last, REAPER's main window say. Taking the foreground
//! is a separate, explicit `SetForegroundWindow`, whose answer is the one `show` gives. Not
//! topmost: NVDA speaks for a topmost window that is not in front, and a page's live region would
//! then talk over the plug-in (research note of 2026-09-28).
//!
//! **Leaving the front.** With no owner, a page hidden or taken down while it is in front would
//! leave the front to whichever window Windows puts next — REAPER's main window, or the desktop,
//! rather than the plug-in's. So the window that had the front when the page last took it is
//! remembered, and given the front back first ([`give_front_back`]): at the close box and Alt+F4,
//! at `hide` (a module's, or its module disabled) and at `close` (or its module reloaded). An app
//! page only when it has kept the front since it took it: one the user came to with Alt+Tab or
//! the taskbar is left as any program's window is (`WindowKind::gives_front_back`). A page that
//! took the front from another page, hidden since — one page shown in place of another, the new
//! one first so the front goes from page to page and never through the plug-in window — gives it
//! to the window that other page came from (`webview::front_goes_to`).
//!
//! **The page.** wry's `build`, which puts WebView2 into a child window of the frame that fills
//! it, and subclasses the frame for three things this needs: its size, its moves (Chromium places
//! a select list's popup by them) and its focus (the frame getting the keyboard hands it to the
//! document, `MoveFocus`). `build_as_child` makes the same child window without those three.
//! Its data folder is beside the executable (`WebContext`); nothing of a page is kept there
//! between runs (`with_incognito`).
//!
//! **What a page may reach.** Its own files, served from the module's folder. Every other request
//! is answered 403 here, by a request filter of our own beside wry's, before WebView2 sends it
//! anywhere; navigations away, new windows, downloads, permissions, external programs and
//! script dialogs are refused, and each refusal is a line in the log (the first twenty of a page,
//! then a count, so a page retrying in a loop cannot push the session out of the log). WebRTC and
//! the browser's own look-ahead (`dns-prefetch`, `preconnect`) are not requests, and nothing here
//! stops them yet — TODO.md, stage 3.
//!
//! **When it is loaded.** WebView2's own report of each navigation, not wry's: wry calls every
//! completed navigation "finished", a refused one included, which would say `onReady` again for
//! a link the host did not follow.
//!
//! **Where it runs.** Everything here is on the event loop's thread, the only one WebView2 may be
//! used from. Part of a first build — WebView2's browser process starting, its data folder opened
//! — is the environment ([`environment`]), which the host creates itself and does not wait for:
//! WebView2 answers through the event loop, and the tick after that answer builds the page. How
//! much of a cold start that part is, and how much stays in the part that is waited for, the log
//! measures (test W8). The environment is kept while it has a browser process and every page is
//! built in it; when its browser process ends — the last page closed, or a crash — it is let go,
//! and the next build makes a new one, again without waiting. What is left of a build waits: wry
//! makes the page's controller in a message loop of its own, so a build is never done inside
//! Luau — `show` or `prepare` queues it and the tick builds it after the modules' callbacks ran
//! (`webview_lua::build_pending`), with a line in the log before it starts. The web view's
//! callbacks do not reach Luau either: they queue a [`PageEvent`] here, and the tick hands it on.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    CreateCoreWebView2EnvironmentWithOptions, GetAvailableCoreWebView2BrowserVersionString,
    ICoreWebView2Environment, ICoreWebView2Environment10, ICoreWebView2Environment5,
    ICoreWebView2Environment8, ICoreWebView2EnvironmentOptions, ICoreWebView2_18,
    COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND, COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL,
    COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED, COREWEBVIEW2_PROCESS_KIND,
    COREWEBVIEW2_PROCESS_KIND_BROWSER, COREWEBVIEW2_PROCESS_KIND_GPU,
    COREWEBVIEW2_PROCESS_KIND_RENDERER, COREWEBVIEW2_PROCESS_KIND_UTILITY, COREWEBVIEW2_WEB_ERROR_STATUS,
    COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
};
use webview2_com::{
    BrowserProcessExitedEventHandler, CoreWebView2EnvironmentOptions,
    CreateCoreWebView2EnvironmentCompletedHandler,
    LaunchingExternalUriSchemeEventHandler, NavigationCompletedEventHandler, ProcessFailedEventHandler,
    WebResourceRequestedEventHandler, WindowCloseRequestedEventHandler,
};
use windows_core::{IUnknown, Interface, HSTRING, PCWSTR, PWSTR};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName, LOCALE_ALLOW_NEUTRAL_NAMES};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromRect, MonitorFromWindow, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetForegroundWindow, GetWindowRect, IsIconic, IsWindow, IsWindowVisible, IsZoomed,
    SetForegroundWindow, SetWindowPos, ShowWindow, WindowFromPoint, GA_ROOT, HWND_TOP, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_SHOWNOACTIVATE,
};
use wry::raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use wry::{
    http, NewWindowResponse, PermissionResponse, WebView, WebViewBuilder, WebViewBuilderExtWindows,
    WebViewExtWindows,
};
use wxdragon::prelude::*;

use crate::webview::{self, PageEvent, Placement, Rect, Refusal, WindowKind};

/// How many events wait for the next tick at most. A page whose script posts in a loop fills the
/// queue faster than a tick empties it; past this, its messages are dropped and counted (a load
/// or a close never is).
const MAX_WAITING_EVENTS: usize = 4096;

thread_local! {
    /// What the pages' windows sent since the last tick, in order. Filled by wx's and WebView2's
    /// callbacks, which run on this thread while the loop dispatches messages; emptied by
    /// [`take_events`].
    static EVENTS: RefCell<Vec<PageEvent>> = const { RefCell::new(Vec::new()) };
    /// Messages dropped because [`MAX_WAITING_EVENTS`] were waiting, since the last tick.
    static DROPPED: Cell<usize> = const { Cell::new(0) };
    /// Every built page's window and the window it came from (its `came_from`), for a page leaving
    /// the front that came from another page (`webview::front_goes_to`). In while the page is built.
    static CAME_FROM: RefCell<HashMap<isize, Rc<Cell<isize>>>> = RefCell::new(HashMap::new());
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
            &format!(
                "{dropped} message(s) from pages were dropped: more than {MAX_WAITING_EVENTS} waited for one tick"
            ),
        );
    }
    EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// How many refusals of one page are written to the log. Past it they are counted, and the
/// count is said when the page goes: the log rotates at 8 MiB, and a page that retries a refused
/// thing in a loop would otherwise push the rest of the session out of it.
const MAX_REFUSAL_LINES: u32 = 20;

/// One page's refusals: its title for the lines, and how many there were. Shared by the web
/// view's handlers, some of which wry wants `Send`.
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
                &format!("page '{}': {what} — the page's further refusals are counted, not logged", self.title),
            );
        }
    }
}

/// The WebView2 runtime's version, or `None` when there is none. Asks the loader, which reads
/// the registry and the runtime's folder: a few milliseconds, and no process is started.
pub(crate) fn runtime_version() -> Option<String> {
    let mut version = PWSTR::null();
    // SAFETY: a null folder asks for the installed runtime; the out-parameter is a PWSTR the
    // loader allocates with CoTaskMemAlloc, which `take_pwstr` frees.
    let asked = unsafe { GetAvailableCoreWebView2BrowserVersionString(windows_core::PCWSTR::null(), &mut version) };
    let text = webview2_com::take_pwstr(version);
    (asked.is_ok() && !text.is_empty()).then_some(text)
}

/// The `WEBVIEW2_*` variables, said once per process, at the first build — the one moment they
/// act on anything.
fn say_overrides_once() {
    static SAID: AtomicBool = AtomicBool::new(false);
    if SAID.swap(true, Ordering::Relaxed) {
        return;
    }
    let vars = std::env::vars_os().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned()));
    let lines = webview::webview2_overrides(vars);
    if lines.is_empty() {
        crate::logging::line("gui", "no WEBVIEW2_* environment variable is set");
    }
    for line in lines {
        crate::logging::line(
            "gui",
            &format!("WebView2 reads this from the environment, whatever the host asks for: {line}"),
        );
    }
}

/// The browser arguments wry passes when it creates the environment itself, with the options this
/// host leaves at wry's defaults (autoplay on): the pages were built with these before the host
/// created the environment, and are now. The first switch removes Edge's "mini menu" over a
/// selection, the PDF one and SmartScreen's look-ups (wry #535, tauri #1345).
const BROWSER_ARGUMENTS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// The one WebView2 environment every page of the application is built in.
enum Env {
    /// Not asked for yet — or let go: its creation failed, or its browser process ended. The next
    /// build asks again.
    Absent,
    /// Asked for at `since`, the call that asked taking `call_ms` (`None` while it runs); WebView2
    /// answers through the event loop.
    Creating { since: Instant, call_ms: Option<u128> },
    /// `used` once a page's controller has been made in it: a later build reuses it. Released only
    /// by [`set_env`], while the application runs: never by the thread-local's own destructor at
    /// the process's end, when WebView2's library may already have been told to detach.
    Ready { env: ManuallyDrop<ICoreWebView2Environment>, used: bool },
    /// WebView2 answered with a failure, not yet handed to the builds that waited for it — and
    /// dropped at the end of a tick in which no build waits ([`drop_unclaimed_failure`]).
    Failed(String),
}

thread_local! {
    /// (The event loop's thread.) The environment.
    static ENV: RefCell<Env> = const { RefCell::new(Env::Absent) };
}

/// Replaces the environment's state, releasing an environment it held.
fn set_env(next: Env) {
    let old = ENV.with(|e| std::mem::replace(&mut *e.borrow_mut(), next));
    if let Env::Ready { env, .. } = old {
        drop(ManuallyDrop::into_inner(env));
    }
}

/// Whether two environment pointers are the same object: COM's identity is the `IUnknown`.
fn same(a: &ICoreWebView2Environment, b: &ICoreWebView2Environment) -> bool {
    match (a.cast::<IUnknown>(), b.cast::<IUnknown>()) {
        (Ok(a), Ok(b)) => a.as_raw() == b.as_raw(),
        _ => false,
    }
}

/// Where the environment is, for a build that needs it.
pub(crate) enum EnvNow {
    /// Ready; `reused` when a page's controller was made in it before — its browser process may
    /// have gone since, which a build that fails in it retries in a new one (Microsoft's advice).
    Ready(ICoreWebView2Environment, bool),
    /// Being created; WebView2 answers through the event loop, and the tick after that builds.
    Waiting,
    /// It could not be created; the next build asks again.
    Failed(String),
}

/// The environment every page is built in: ready, being created — its creation started here if
/// nobody had asked for it — or failed, which is handed to the caller once and then forgotten,
/// so the next build asks again. Never waits: `CreateCoreWebView2EnvironmentWithOptions` starts
/// WebView2's browser process and answers through the event loop, which goes on meanwhile —
/// hotkeys, speech and every module's callbacks. Microsoft's own way to create one; wry's waits
/// for the answer in a message loop of its own, which was part of what held the event loop for
/// 4.4 seconds at the probe's first build (2026-09-28).
pub(crate) fn environment() -> EnvNow {
    match environment_now() {
        Some(EnvNow::Failed(why)) => {
            set_env(Env::Absent);
            EnvNow::Failed(why)
        }
        Some(now) => now,
        None => create_environment(),
    }
}

/// The environment `env` is let go of, if it is still the one kept: its browser process ended or
/// failed, or a page's controller could not be made in it. `true` when it was. An event about an
/// environment let go of before — a page of a crashed browser reporting late, after a new one was
/// made — changes nothing; and one being created is never let go of, so no two creations run at
/// once.
pub(crate) fn forget_environment_if(env: &ICoreWebView2Environment) -> bool {
    let kept = ENV.with(|e| matches!(&*e.borrow(), Env::Ready { env: kept, .. } if same(kept, env)));
    if kept {
        set_env(Env::Absent);
    }
    kept
}

/// A page's controller was made in `env`: a later build in it reuses it.
pub(crate) fn built_in(env: &ICoreWebView2Environment) {
    ENV.with(|e| {
        if let Env::Ready { env: kept, used } = &mut *e.borrow_mut() {
            if same(kept, env) {
                *used = true;
            }
        }
    });
}

/// At the end of a tick in which no page waits for the environment: a failure WebView2 answered
/// that no build took — its pages closed meanwhile — is dropped, so a build much later asks again
/// rather than fail with a stale reason. Said in the log.
pub(crate) fn drop_unclaimed_failure() {
    let why = ENV.with(|e| match &*e.borrow() {
        Env::Failed(why) => Some(why.clone()),
        _ => None,
    });
    if let Some(why) = why {
        set_env(Env::Absent);
        crate::logging::line(
            "gui",
            &format!("the WebView2 environment's failure ({why}) was waited for by no page any more; the next build asks again"),
        );
    }
}

/// Starts creating the environment and answers where it is afterwards: WebView2 may refuse at
/// once, and then answers before the call returns.
fn create_environment() -> EnvNow {
    say_overrides_once();
    let (apartment, usable) = apartment();
    static SAID: AtomicBool = AtomicBool::new(false);
    if !SAID.swap(true, Ordering::Relaxed) || !usable {
        crate::logging::line("gui", &format!("the event loop's COM apartment is {apartment}"));
    }
    if !usable {
        return EnvNow::Failed(format!(
            "the event loop's thread is in COM's {apartment} apartment, and WebView2 needs a \
             single-threaded one — something entered it on this thread before the window system \
             started"
        ));
    }
    // As wry does before it makes an environment: a thread in no apartment yet enters the
    // single-threaded one, which WebView2 needs; one in it already is left as it is.
    // SAFETY: COM initialisation of this thread, which the check above allows.
    let _ = unsafe {
        windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED)
    };
    let folder = crate::portable::base_dir().join(webview::DATA_FOLDER);
    let options = CoreWebView2EnvironmentOptions::default();
    // SAFETY: the options object is ours until it is handed over below.
    unsafe {
        options.set_additional_browser_arguments(BROWSER_ARGUMENTS.to_string());
        options.set_language(user_language());
    }
    let since = Instant::now();
    set_env(Env::Creating { since, call_ms: None });
    crate::logging::line(
        "gui",
        &format!(
            "the WebView2 environment is being created — its browser process starting, its data in \
             {} — and the event loop goes on meanwhile",
            folder.display()
        ),
    );
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(|result, env| {
        environment_answered(match (result, env) {
            (Ok(()), Some(env)) => Ok(env),
            (Ok(()), None) => Err("WebView2 answered with no environment".to_string()),
            (Err(e), _) => Err(e.to_string()),
        });
        Ok(())
    }));
    // SAFETY: a null browser folder asks for the installed runtime; the folder and the options
    // live for the call, and WebView2 keeps its own reference to the handler.
    let asked = unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            PCWSTR::null(),
            &HSTRING::from(folder.as_os_str()),
            &ICoreWebView2EnvironmentOptions::from(options),
            &handler,
        )
    };
    // The call's own time — it loads WebView2's library into this process, which the event loop
    // does wait for — kept for the line that says the environment is ready.
    let call_ms = since.elapsed().as_millis();
    let creating = ENV.with(|e| {
        if let Env::Creating { call_ms: c, .. } = &mut *e.borrow_mut() {
            *c = Some(call_ms);
            true
        } else {
            false
        }
    });
    if let Err(e) = asked {
        // Unless the handler has answered already, inside the call.
        if creating {
            environment_answered(Err(e.to_string()));
        }
    }
    match environment_now() {
        // Refused at once: said now, and the next build asks again.
        Some(EnvNow::Failed(why)) => {
            set_env(Env::Absent);
            EnvNow::Failed(why)
        }
        Some(now) => now,
        None => EnvNow::Waiting,
    }
}

/// Where the environment is, without asking for one.
fn environment_now() -> Option<EnvNow> {
    ENV.with(|e| match &*e.borrow() {
        Env::Ready { env, used } => Some(EnvNow::Ready(ICoreWebView2Environment::clone(env), *used)),
        Env::Creating { .. } => Some(EnvNow::Waiting),
        Env::Failed(why) => Some(EnvNow::Failed(why.clone())),
        Env::Absent => None,
    })
}

/// WebView2's answer, on the event loop's thread: kept for the tick's builds, and said with how
/// long it took — in the call that asked, which the event loop waited for, and in all. An answer
/// nobody waits for any more (one after the call itself had failed) is dropped.
fn environment_answered(answer: Result<ICoreWebView2Environment, String>) {
    let asked = ENV.with(|e| match &*e.borrow() {
        Env::Creating { since, call_ms } => Some((since.elapsed().as_millis(), *call_ms)),
        _ => None,
    });
    let Some((ms, call_ms)) = asked else {
        crate::logging::line(
            "gui",
            &format!(
                "WebView2 answered for an environment nobody waits for any more ({}); dropped",
                if answer.is_ok() { "ready" } else { "failed" }
            ),
        );
        return;
    };
    let call = match call_ms {
        Some(c) => format!("{c} ms of that in the call that asked, which the event loop waited for"),
        None => "answered inside the call that asked".to_string(),
    };
    match answer {
        Ok(env) => {
            crate::logging::line(
                "gui",
                &format!(
                    "the WebView2 environment is ready, {ms} ms after it was asked for ({call}); the pages \
                     waiting for it are built at the next tick"
                ),
            );
            watch_browser_exit(&env);
            set_env(Env::Ready { env: ManuallyDrop::new(env), used: false });
        }
        Err(why) => {
            crate::logging::line("gui", &format!("the WebView2 environment could not be created ({ms} ms, {call}): {why}"));
            set_env(Env::Failed(why));
        }
    }
}

/// The environment's browser process ending — normally, when its last page has closed, or after a
/// failure: WebView2 would start a new one at the next page's controller, inside the build the
/// event loop waits for, and in the runtime version the environment was made with, which an update
/// may have removed meanwhile. So the environment is let go of then, and the next build makes a
/// new one without waiting (`ICoreWebView2Environment5::BrowserProcessExited`). A runtime too old to
/// say so keeps the environment, and a build that fails in it is retried in a new one.
fn watch_browser_exit(env: &ICoreWebView2Environment) {
    let Ok(env5) = env.cast::<ICoreWebView2Environment5>() else {
        crate::logging::line(
            "gui",
            "this WebView2 runtime does not say when its browser process ends: the environment is kept, and a \
             build that fails in it is tried again in a new one",
        );
        return;
    };
    let handler = BrowserProcessExitedEventHandler::create(Box::new(|sender, args| {
        let mut kind = COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND::default();
        if let Some(args) = args {
            // SAFETY: the event's own arguments, for the duration of the callback.
            unsafe { args.BrowserProcessExitKind(&mut kind)? };
        }
        if let Some(sender) = sender {
            if forget_environment_if(&sender) {
                crate::logging::line(
                    "gui",
                    &format!(
                        "the WebView2 browser process ended ({}); the next build makes a new environment, \
                         without waiting",
                        if kind == COREWEBVIEW2_BROWSER_PROCESS_EXIT_KIND_NORMAL {
                            "normally: its last page was closed"
                        } else {
                            "after a failure"
                        }
                    ),
                );
            }
        }
        Ok(())
    }));
    let mut token = 0i64;
    // SAFETY: the environment's own COM object, on its thread.
    if let Err(e) = unsafe { env5.add_BrowserProcessExited(&handler, &mut token) } {
        crate::logging::line(
            "gui",
            &format!("the end of the WebView2 browser process could not be watched ({e}); a build that fails is tried again in a new environment"),
        );
    }
}

/// The user's display language, as wry passes it (`GetUserDefaultUILanguage` as a locale name):
/// what the page's own controls and its `Accept-Language` are in.
fn user_language() -> String {
    let mut name = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    // SAFETY: plain queries into a buffer of ours, its length given.
    let n = unsafe {
        LCIDToLocaleName(u32::from(GetUserDefaultUILanguage()), name.as_mut_ptr(), name.len() as i32, LOCALE_ALLOW_NEUTRAL_NAMES)
    };
    let len = (n.max(1) as usize - 1).min(name.len());
    String::from_utf16_lossy(&name[..len])
}

/// What a build is made from.
pub(crate) struct BuildSpec {
    pub page: u64,
    pub build: u64,
    /// A tool window or an ordinary one.
    pub kind: WindowKind,
    /// The page option `activates`, which `webview_lua.rs` hands both platforms' windows alike. A
    /// Mac's panel is made with it; here it changes nothing, as a window takes the keyboard and the
    /// front together (docs/api/gui.md), so it is never read.
    #[allow(dead_code)]
    pub activates: bool,
    pub title: String,
    pub scheme: String,
    /// The module's folder, canonical.
    pub root: PathBuf,
    pub entry_url: String,
}

/// A frame's HWND as the handle wry takes.
struct Parent(NonZeroIsize);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(self.0));
        // SAFETY: the frame outlives this borrow — `build` holds it for the whole call.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

/// One built page: its window and its web view. Taken down when the last reference to it goes —
/// whoever holds it then, a callback of the module's included — by [`Drop`].
pub(crate) struct Native {
    frame: Frame,
    kind: WindowKind,
    /// Dropped first, in `Drop`: the web view closes its controller and lets go of the frame
    /// before the frame is destroyed.
    webview: ManuallyDrop<WebView>,
    hwnd: isize,
    /// Whether the window has been given a place yet. Until then a `show` without `avoid`
    /// centres it; after that it stays where it was put (or where the user moved it).
    placed: Cell<bool>,
    refusals: Refusals,
    /// The window that had the front when the page last took it — the plug-in's, as a rule — for
    /// [`give_front_back`]; 0 until it first does. Shared with the close box's handler.
    came_from: Rc<Cell<isize>>,
    /// Whether the page has kept the front since it last took it from another window: set when it
    /// does ([`Native::focus`]); an app page's frame clears it when it is deactivated. Asked only
    /// for an app page (`WindowKind::gives_front_back`). Shared with the close box's handler.
    kept: Rc<Cell<bool>>,
}

impl Drop for Native {
    fn drop(&mut self) {
        // Taken down in front — a module's `close`, or its module reloaded — the window it came
        // from gets the front back first.
        let gives = self.kind.gives_front_back(self.kept.get());
        give_front_back(self.hwnd, self.came_from.get(), gives, &self.refusals.title);
        // No page hands the front on through this one any more. (`try_with`: a page dropped as
        // the thread ends finds the table gone.)
        let _ = CAME_FROM.try_with(|pages| pages.borrow_mut().remove(&self.hwnd));
        // SAFETY: dropped here, once; nothing uses the web view after this.
        unsafe { ManuallyDrop::drop(&mut self.webview) };
        // Hidden first: wx leaves a window it destroys on screen until its next idle moment when
        // it is the only one shown, and a build in the same tick delays that. Both calls do
        // nothing for a frame wx has already destroyed (the application ending).
        self.frame.show(false);
        self.frame.destroy();
        let n = self.refusals.said.load(Ordering::Relaxed);
        if n > MAX_REFUSAL_LINES {
            crate::logging::line(
                "gui",
                &format!("page '{}': {n} refusals in all, {} of them not logged", self.refusals.title, n - MAX_REFUSAL_LINES),
            );
        }
    }
}

/// The event loop's COM apartment, in a word, and whether WebView2 can live in it.
///
/// WebView2 needs a single-threaded apartment. wxWidgets puts the event loop in one when it
/// starts — unless something entered the multithreaded one on this thread first. UI Automation
/// did once, on whichever thread asked it, and a module that asked `host.element` at load would
/// have left the loop there; it is asked on the host's accessibility thread now (element.rs),
/// which enters that apartment for itself. Which apartment the loop really has was never
/// measured; the first build says so.
fn apartment() -> (&'static str, bool) {
    use windows::Win32::System::Com::{
        CoGetApartmentType, APTTYPE, APTTYPEQUALIFIER, APTTYPEQUALIFIER_IMPLICIT_MTA,
        APTTYPE_MAINSTA, APTTYPE_MTA, APTTYPE_NA, APTTYPE_STA,
    };
    let (mut kind, mut qualifier) = (APTTYPE(-1), APTTYPEQUALIFIER(0));
    // SAFETY: a query about this thread, into two plain values.
    if unsafe { CoGetApartmentType(&mut kind, &mut qualifier) }.is_err() {
        // CO_E_NOTINITIALIZED: no apartment yet; wry enters a single-threaded one itself.
        return ("none yet", true);
    }
    match kind {
        APTTYPE_STA => ("single-threaded", true),
        APTTYPE_MAINSTA => ("the main single-threaded one", true),
        // A thread that never entered one, in a process that has a multithreaded apartment:
        // it may still enter a single-threaded one, and wry does.
        APTTYPE_MTA if qualifier == APTTYPEQUALIFIER_IMPLICIT_MTA => ("none yet (implicitly multithreaded)", true),
        APTTYPE_MTA => ("multithreaded", false),
        APTTYPE_NA => ("neutral", false),
        _ => ("unknown", false),
    }
}

/// Builds a page in the environment [`environment`] made: its frame, hidden, and its web view,
/// which starts loading at once. Pumps window messages while WebView2 makes the page's controller
/// — see the file.
pub(crate) fn build(spec: &BuildSpec, env: &ICoreWebView2Environment) -> Result<Native, String> {
    let frame = Frame::builder().with_title(&spec.title).with_style(frame_style(spec.kind)).build();
    let hwnd = frame.get_handle() as isize;
    let Some(handle) = NonZeroIsize::new(hwnd) else {
        frame.destroy();
        return Err("wxWidgets gave the page's window no handle".to_string());
    };
    // Its size before anything is in it, so the web view starts at the size it will be shown at
    // (`place` sizes it again for the monitor it goes to).
    // SAFETY: the frame's own window, alive for these calls.
    let dpi = unsafe { GetDpiForWindow(hwnd as HWND) }.max(96) as i32;
    let full = scaled(webview::DEFAULT_SIZE, dpi);
    unsafe {
        SetWindowPos(hwnd as HWND, std::ptr::null_mut(), 0, 0, full.0, full.1, SWP_NOZORDER | SWP_NOACTIVATE);
    }

    // The close box hides the page rather than destroy it: the module decides whether it is gone
    // (`close`), and a page the user closed keeps its state for the next `show`. Only a close
    // that cannot be refused — the application exiting — closes it. The window the page came
    // from gets the front back first. Alt+F4 pressed in the page reaches the frame and does the
    // same (the maintainer's run of 2026-10-11).
    let (page, build, kind) = (spec.page, spec.build, spec.kind);
    let came_from = Rc::new(Cell::new(0isize));
    let kept = Rc::new(Cell::new(false));
    let (back_to, close_kept, close_title) = (came_from.clone(), kept.clone(), spec.title.clone());
    frame.on_close(move |event| {
        if let WindowEventData::General(e) = &event {
            if !e.can_veto() {
                return;
            }
            e.veto();
        }
        give_front_back(hwnd, back_to.get(), kind.gives_front_back(close_kept.get()), &close_title);
        conceal(&frame, kind);
        push(PageEvent::Closed { page, build, why: "user" });
    });
    // An app page left for another window — by Alt+Tab, the taskbar, a click, or the module
    // bringing the plug-in forward — has not kept the front. Passed on: wx's own handler moves the
    // keyboard back to the control that had it when the frame is activated again.
    if kind == WindowKind::App {
        let left = kept.clone();
        frame.on_activate(move |event| {
            if let WindowEventData::Activate(a) = &event {
                if !a.is_active() {
                    left.set(false);
                }
            }
            event.skip(true);
        });
    }

    let origin = webview::origin_for(&spec.scheme);
    let refusals = Refusals::new(&spec.title);
    let (root, scheme, served) = (spec.root.clone(), spec.scheme.clone(), refusals.clone());
    let (ipc_origin, ipc_refusals) = (origin.clone(), refusals.clone());
    let (nav_origin, nav_refusals) = (origin.clone(), refusals.clone());
    let (win_refusals, download_refusals, asked_refusals) = (refusals.clone(), refusals.clone(), refusals.clone());
    // The environment carries the data folder and the browser's arguments; wry only makes the
    // page's controller in it.
    let built = WebViewBuilder::new()
        .with_environment(env.clone())
        .with_url(spec.entry_url.clone())
        .with_custom_protocol(spec.scheme.clone(), move |_id, request| respond(&root, &scheme, &request, &served))
        .with_ipc_handler(move |request: http::Request<String>| {
            // Main frame only: WebView2 reports a frame's messages on the frame, never here. And
            // only from the page's own documents, and no larger than a message may be — checked
            // here, before it waits for the tick.
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
            push(PageEvent::Message { page, build, text });
        })
        .with_initialization_script(webview::platform_shim(webview::Engine::WebView2))
        .with_navigation_handler(move |uri: String| {
            let own = webview::own_origin(&uri, &nav_origin);
            if !own {
                nav_refusals.say(&format!("a navigation to {uri} was refused — a page stays on its own files"));
            }
            own
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
        .with_incognito(true)
        .with_focused(false)
        .with_visible(true)
        // No automatic permission to READ the clipboard (the permission handler refuses it too).
        // Writing it needs no permission in Chromium: a page in front can (docs/api/gui.md).
        .with_clipboard(false)
        .with_general_autofill_enabled(false)
        .with_back_forward_navigation_gestures(false)
        .with_browser_accelerator_keys(false)
        .with_default_context_menus(false)
        .build(&Parent(handle));
    let webview = match built {
        Ok(w) => w,
        Err(e) => {
            frame.destroy();
            // A controller that could not be made in it may mean an environment whose browser
            // process is gone, or whose runtime version an update removed: the next build makes a
            // new one (Microsoft: retry from a new environment).
            forget_environment_if(env);
            return Err(format!("{e}"));
        }
    };
    built_in(env);
    // Taken out again by `Drop`, so from here on with the `Native` that drops it.
    CAME_FROM.with(|pages| pages.borrow_mut().insert(hwnd, came_from.clone()));
    let native = Native {
        frame,
        kind,
        webview: ManuallyDrop::new(webview),
        hwnd,
        placed: Cell::new(false),
        refusals,
        came_from,
        kept,
    };
    // Before the message loop runs again, so before the entry document can have loaded and
    // before any script of the page's has run. A failure here takes the page down again (Drop).
    native.watch(page, build, &spec.scheme, &origin)?;
    Ok(native)
}

/// The frame's style for a page of `kind` (`webview::Recipe`): a title bar, a system menu, a close
/// box and a border to resize it by, and the tool-window style or a minimise and a maximise box.
fn frame_style(kind: WindowKind) -> FrameStyle {
    let recipe = kind.recipe();
    let mut style = FrameStyle::Caption | FrameStyle::SystemMenu | FrameStyle::CloseBox | FrameStyle::ResizeBorder;
    if !recipe.listed {
        style |= FrameStyle::ToolWindow;
    }
    if recipe.minimise {
        style |= FrameStyle::MinimizeBox;
    }
    if recipe.maximise {
        style |= FrameStyle::MaximizeBox;
    }
    style
}

/// Hides the frame — the close box's way and [`Native::hide`]'s. wx remembers a frame hidden while
/// minimised or maximised and shows it so again, with `SW_MINIMIZE`, which activates the next
/// window in the stacking order, or `SW_MAXIMIZE`, which activates the frame — either moves the
/// front; an app page's frame is told it is restored instead, so its next `show` takes
/// [`Native::unhide`]'s way, which does not: a minimised one comes back restored, a maximised one
/// maximised (the window keeps that state while hidden).
fn conceal(frame: &Frame, kind: WindowKind) {
    frame.show(false);
    if kind == WindowKind::App {
        frame.maximize(false);
    }
}

/// Gives the front back to `back_to`, the window that had it when page `hwnd` last took it, while
/// the page still has it: called before the page is hidden or taken down, which would otherwise
/// leave the front to whichever window Windows puts next (see the file). Nothing when the page is
/// not in front — a module that brought the plug-in window forward first, say. Windows lets this
/// process hand the front on while it has it; a window that is gone or hidden by now is not asked,
/// and Windows decides — as it does for an app page that has not kept the front since it took it
/// (`gives` false). `back_to` a page hidden since: the window that page came from
/// (`webview::front_goes_to`). Each case a line in the log.
fn give_front_back(hwnd: isize, back_to: isize, gives: bool, title: &str) {
    // SAFETY: a plain query.
    if unsafe { GetForegroundWindow() } as isize != hwnd {
        return;
    }
    if !gives {
        crate::logging::line(
            "gui",
            &format!(
                "page '{title}': leaves the front; it is an application window that has not kept the front since \
                 it last took it (the user came to it another way), so Windows decides which window gets it"
            ),
        );
        return;
    }
    let to = CAME_FROM
        .try_with(|pages| {
            let pages = pages.borrow();
            webview::front_goes_to(hwnd, back_to, pages.len(), |w| {
                // SAFETY: a query on any handle.
                pages.get(&w).map(|c| (unsafe { IsWindowVisible(w as HWND) } != 0, c.get()))
            })
        })
        .unwrap_or(back_to);
    let via = if to == back_to { String::new() } else { format!(" it was shown in place of (page window {back_to})") };
    // SAFETY: plain queries, and one request for a window checked to exist.
    unsafe {
        if to == 0 || to == hwnd || IsWindow(to as HWND) == 0 || IsWindowVisible(to as HWND) == 0 {
            crate::logging::line(
                "gui",
                &format!("page '{title}': leaves the front; the window it came from is gone or hidden, so Windows decides which window gets it"),
            );
            return;
        }
        let given = SetForegroundWindow(to as HWND) != 0;
        crate::logging::line(
            "gui",
            &format!(
                "page '{title}': leaves the front; given back to window {to}, which had it before the page{via}{}",
                if given { "" } else { " — declined by the system, so Windows decides which window gets it" }
            ),
        );
    }
}

/// `(w, h)` at 96 dots per inch, at `dpi`.
fn scaled((w, h): (i32, i32), dpi: i32) -> (i32, i32) {
    (w * dpi / 96, h * dpi / 96)
}

/// One request of a page, answered from the module's folder (`webview::serve`). Anything but a
/// file is a line in the page's refusals.
fn respond(
    root: &Path,
    scheme: &str,
    request: &http::Request<Vec<u8>>,
    refusals: &Refusals,
) -> http::Response<Cow<'static, [u8]>> {
    let uri = request.uri().to_string();
    let served = webview::serve(root, scheme, request.method().as_str(), &uri);
    if served.status != 200 {
        refusals.say(&format!("{} {uri}: {}", request.method(), served.status));
    }
    let mut response = http::Response::builder().status(served.status);
    for (name, value) in served.headers {
        response = response.header(name, value);
    }
    response.body(Cow::Owned(served.body)).unwrap_or_else(|e| {
        // Not reached with the headers `serve` makes; if it were, the answer still carries the
        // policy rather than being a bare 200.
        refusals.say(&format!("the answer to {uri} could not be made ({e}); answered 500"));
        let mut failed = http::Response::builder().status(500);
        for (name, value) in webview::base_headers() {
            failed = failed.header(name, value);
        }
        failed.body(Cow::Borrowed(&[][..])).unwrap_or_else(|_| http::Response::new(Cow::Borrowed(&[][..])))
    })
}

/// The work areas of every monitor, in physical pixels.
fn work_areas() -> Vec<Rect> {
    unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        // SAFETY: `data` is the Vec below, alive for the whole enumeration.
        let out = unsafe { &mut *(data as *mut Vec<Rect>) };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
            out.push(rect_of(&info.rcWork));
        }
        1
    }
    let mut out: Vec<Rect> = Vec::new();
    // SAFETY: a callback that only writes into `out`.
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), &mut out as *mut Vec<Rect> as LPARAM);
    }
    out
}

fn rect_of(r: &RECT) -> Rect {
    Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
}

/// The work area of the monitor the window in front is on.
fn foreground_work_area() -> Option<Rect> {
    // SAFETY: plain queries; a null or stale window gives the nearest monitor.
    unsafe {
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then(|| rect_of(&info.rcWork))
    }
}

/// The display scaling of the monitor `r` is mostly on, in dots per inch (96 at 100 %).
fn dpi_at(r: Rect) -> i32 {
    let rect = RECT { left: r.x, top: r.y, right: r.x + r.w, bottom: r.y + r.h };
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: plain queries into two plain values.
    let asked = unsafe {
        let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y)
    };
    if asked < 0 {
        return 96;
    }
    (x as i32).max(96)
}

impl Native {
    pub(crate) fn hwnd(&self) -> isize {
        self.hwnd
    }

    /// The window's rectangle now, in physical pixels.
    fn rect(&self) -> Rect {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: the frame's own window.
        unsafe { GetWindowRect(self.hwnd as HWND, &mut r) };
        rect_of(&r)
    }

    fn move_to(&self, x: i32, y: i32) {
        // SAFETY: the frame's own window; position only.
        unsafe {
            SetWindowPos(self.hwnd as HWND, std::ptr::null_mut(), x, y, 0, 0, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE);
        }
    }

    /// Puts the window at `r`. Moving it onto a monitor of another display scaling makes Windows
    /// send `WM_DPICHANGED`, on which wx resizes it to the rectangle Windows suggests — which can
    /// reach over the rectangle `place` just kept clear — so it is set once more when that
    /// happened (on the new monitor already, so no second change comes).
    fn set_rect(&self, r: Rect) {
        for _ in 0..2 {
            // SAFETY: the frame's own window; position and size.
            unsafe {
                SetWindowPos(self.hwnd as HWND, std::ptr::null_mut(), r.x, r.y, r.w, r.h, SWP_NOZORDER | SWP_NOACTIVATE);
            }
            if self.rect() == r {
                return;
            }
        }
        let now = self.rect();
        crate::logging::line(
            "gui",
            &format!(
                "page '{}': put at {},{} {}x{}, and the window is at {},{} {}x{}",
                self.refusals.title, r.x, r.y, r.w, r.h, now.x, now.y, now.w, now.h
            ),
        );
    }

    /// Where the window goes beside `avoid` — at its full size for the display scaling of the
    /// monitor `avoid` is on, or smaller where it has to be — or, where nothing is clear of it, at
    /// its smallest in the corner of that monitor where it covers the least of it
    /// (`webview::place`). Decided, not done: [`Native::put`] puts it there. `None` without
    /// `avoid`, and where no monitor's work area could be read.
    pub(crate) fn plan(&self, avoid: Option<Rect>) -> Option<Placement> {
        let a = avoid?;
        let dpi = dpi_at(a);
        let (full, min) = (scaled(webview::DEFAULT_SIZE, dpi), scaled(webview::MIN_SIZE, dpi));
        webview::place(full, min, a, &work_areas())
    }

    /// Puts the window where [`Native::plan`] decided, for a `show` with `avoid`; without `avoid`,
    /// the first time, in the middle of the monitor the window in front is on. An app page the
    /// user minimised or maximised is put back in its normal state first, without activating it —
    /// a hidden one is shown for that, in the state it was hidden in, and then put where it goes.
    pub(crate) fn put(&self, avoid: Option<Rect>, plan: Option<Placement>) {
        match (avoid, plan) {
            (Some(_), Some(p)) => {
                if self.kind == WindowKind::App && self.minimised_or_maximised() {
                    if !self.is_shown() {
                        self.unhide();
                    }
                    if self.minimised_or_maximised() {
                        // SAFETY: the frame's own window; its state only.
                        unsafe { ShowWindow(self.hwnd as HWND, SW_SHOWNOACTIVATE) };
                    }
                }
                self.set_rect(p.rect());
                self.placed.set(true);
            }
            (Some(_), None) => {
                crate::logging::line(
                    "gui",
                    &format!("page '{}': no monitor's work area could be read; the window stays where it is", self.refusals.title),
                );
            }
            (None, _) if !self.placed.get() => {
                if let Some(area) = foreground_work_area() {
                    let now = self.rect();
                    let r = webview::centred((now.w, now.h), &area);
                    self.move_to(r.x, r.y);
                }
                self.placed.set(true);
            }
            (None, _) => {}
        }
    }

    /// Whether the window is minimised or maximised — asked of Windows, which keeps that state for
    /// a hidden window too.
    fn minimised_or_maximised(&self) -> bool {
        // SAFETY: plain queries on the frame's own window.
        unsafe { IsIconic(self.hwnd as HWND) != 0 || IsZoomed(self.hwnd as HWND) != 0 }
    }

    /// Makes the window visible without activating it. A tool window wx shows with `SW_SHOWNA`.
    /// An ordinary frame it shows with `SW_SHOW`, which activates it — and, where this process may
    /// take the foreground, takes it before `focus` could note the window that had it — except a
    /// disabled one, which it shows with `SW_SHOWNA` (`SW_SHOWNOACTIVATE` when minimised): so an
    /// app page's frame is disabled for the call and enabled again at once, which keeps wx's own
    /// idea of whether it is shown right. A minimised app page that is shown is restored, without
    /// activating it.
    fn unhide(&self) {
        match self.kind {
            WindowKind::Tool => self.frame.show(true),
            WindowKind::App => {
                if !self.is_shown() {
                    self.frame.enable(false);
                    self.frame.show(true);
                    self.frame.enable(true);
                }
                // SAFETY: plain queries and one change of state of the frame's own window.
                unsafe {
                    if IsIconic(self.hwnd as HWND) != 0 {
                        ShowWindow(self.hwnd as HWND, SW_SHOWNOACTIVATE);
                    }
                }
            }
        }
    }

    /// Shows the window without activating it, raised to the top of the ordinary windows, and with
    /// `focus` brings it to the front and puts the keyboard into the document.
    /// [`Refusal::Declined`] when Windows would not bring it forward: it is shown then, and the
    /// window in front keeps the keyboard.
    pub(crate) fn show(&self, focus: bool) -> Result<(), Refusal> {
        let _ = self.webview.set_visible(true);
        self.unhide();
        // SAFETY: the frame's own window; its place in the stacking order only.
        unsafe {
            SetWindowPos(self.hwnd as HWND, HWND_TOP, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        if focus && !self.focus() {
            return Err(Refusal::Declined);
        }
        Ok(())
    }

    /// Whether the point in the middle of the window belongs to it — on top there rather than
    /// covered — asked of Windows, for the log.
    pub(crate) fn on_top_at_centre(&self) -> bool {
        let r = self.rect();
        let centre = POINT { x: r.x + r.w / 2, y: r.y + r.h / 2 };
        // SAFETY: plain queries; the answer is compared, never used as a window.
        unsafe {
            let at = WindowFromPoint(centre);
            !at.is_null() && GetAncestor(at, GA_ROOT) as isize == self.hwnd
        }
    }

    /// Brings the shown window to the front and the keyboard into the document; `false` when
    /// it is hidden, or when Windows refused. A minimised app page is restored first.
    pub(crate) fn focus(&self) -> bool {
        if !self.is_shown() {
            return false;
        }
        if self.kind == WindowKind::App {
            self.unhide();
        }
        // The window the front is taken from, to give it back when the page leaves it.
        // SAFETY: a plain query.
        let before = unsafe { GetForegroundWindow() } as isize;
        if before != 0 && before != self.hwnd {
            self.came_from.set(before);
        }
        // SAFETY: the frame's own window.
        if unsafe { SetForegroundWindow(self.hwnd as HWND) } == 0 {
            return false;
        }
        // Taken from another window: kept until the frame is deactivated. Already in front, it
        // stays as it was: an app page the user came to by Alt+Tab is still not one that was
        // brought from somewhere.
        if before != self.hwnd {
            self.kept.set(true);
        }
        // MoveFocus: the element that had focus in the page before, or the document. (The frame
        // becoming active has wry do the same once already, so NVDA may hear it twice: test W1.)
        if let Err(e) = self.webview.focus() {
            crate::logging::line(
                "gui",
                &format!("page '{}': the keyboard could not be moved into the document (MoveFocus: {e})", self.refusals.title),
            );
        }
        true
    }

    /// Hides the window; hidden in front, it gives the front back to the window it came from
    /// first.
    pub(crate) fn hide(&self) {
        let gives = self.kind.gives_front_back(self.kept.get());
        give_front_back(self.hwnd, self.came_from.get(), gives, &self.refusals.title);
        conceal(&self.frame, self.kind);
        self.after_hidden();
    }

    /// The window is hidden (by `hide`, or by the user's close box): stop drawing it.
    pub(crate) fn after_hidden(&self) {
        let _ = self.webview.set_visible(false);
    }

    /// Asked of the window system, not remembered.
    pub(crate) fn is_shown(&self) -> bool {
        // SAFETY: a query on any handle.
        unsafe { IsWindowVisible(self.hwnd as HWND) != 0 }
    }

    /// Hands the page one message: JSON the host encoded, as data — `PostWebMessageAsJson`, no
    /// script is built from it. It arrives as `platform.onmessage`'s argument.
    pub(crate) fn post_json(&self, json: &str) -> Result<(), String> {
        let text = HSTRING::from(json);
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { self.webview.webview().PostWebMessageAsJson(&text) }.map_err(|e| e.to_string())
    }

    /// What the page's WebView2 processes hold: how many of which kind, and the working sets and
    /// private bytes of those that could be measured — one line for the log. The processes are
    /// shared by every page of this application. `None` where the runtime is too old to list
    /// them.
    pub(crate) fn memory(&self) -> Option<String> {
        let env: ICoreWebView2Environment8 = self.webview.environment().cast().ok()?;
        // SAFETY: COM calls on the web view's own objects, on their thread.
        let infos = unsafe { env.GetProcessInfos() }.ok()?;
        let mut count = 0u32;
        unsafe { infos.Count(&mut count) }.ok()?;
        let (mut working, mut private, mut measured) = (0u64, 0u64, 0u32);
        let mut kinds: Vec<&str> = Vec::new();
        for i in 0..count {
            let Ok(info) = (unsafe { infos.GetValueAtIndex(i) }) else { continue };
            let (mut pid, mut kind) = (0i32, COREWEBVIEW2_PROCESS_KIND::default());
            if unsafe { info.ProcessId(&mut pid) }.is_err() {
                continue;
            }
            let _ = unsafe { info.Kind(&mut kind) };
            kinds.push(match kind {
                COREWEBVIEW2_PROCESS_KIND_BROWSER => "browser",
                COREWEBVIEW2_PROCESS_KIND_RENDERER => "renderer",
                COREWEBVIEW2_PROCESS_KIND_GPU => "gpu",
                COREWEBVIEW2_PROCESS_KIND_UTILITY => "utility",
                _ => "other",
            });
            if let Some((w, p)) = process_memory(pid as u32) {
                working += w;
                private += p;
                measured += 1;
            }
        }
        kinds.sort_unstable();
        Some(format!(
            "{count} WebView2 process(es) ({}): {measured} of them measured, working set {} MB, private {} MB",
            kinds.join(", "),
            working / (1 << 20),
            private / (1 << 20)
        ))
    }

    /// Everything WebView2 reports of this page that wry does not hand on, or hands on wrongly,
    /// and the refusals wry has no switch for. The loading report and the request filter are what
    /// the page's life and its promise to reach nothing but its own files rest on: without them
    /// the build fails. The rest are second guards and are logged when they cannot be had.
    fn watch(&self, page: u64, build: u64, scheme: &str, origin: &str) -> Result<(), String> {
        let title = self.refusals.title.to_string();
        self.watch_loading(page, build, origin).map_err(|e| format!("its loading could not be watched: {e}"))?;
        self.refuse_other_requests(scheme).map_err(|e| format!("its requests could not be filtered: {e}"))?;
        let seconds: [(&str, windows_core::Result<()>); 4] = [
            ("its processes could not be watched", self.watch_process_failures(page, build)),
            ("its window.close() could not be watched", self.watch_close_requests(page, build)),
            ("external programs could not be refused", self.refuse_external_programs()),
            ("script dialogs could not be turned off", self.no_script_dialogs()),
        ];
        for (what, result) in seconds {
            if let Err(e) = result {
                crate::logging::line("gui", &format!("page '{title}': {what}: {e}"));
            }
        }
        if self.webview.environment().cast::<ICoreWebView2Environment10>().is_err() {
            crate::logging::line(
                "gui",
                &format!(
                    "page '{title}': this WebView2 runtime is too old to run a page privately — what the \
                     page stores is kept in the WebView2 folder beside the application"
                ),
            );
        }
        Ok(())
    }

    /// Each finished navigation, as WebView2 reports it: a document of the page's own origin that
    /// loaded is the page ready (again); a refused navigation — cancelled — leaves the page as it
    /// was and says nothing; a failed load is said as one; and a navigation that succeeded to
    /// something not the page's own (`about:blank`, which WebView2 cannot refuse) is a page that
    /// has gone, like a failed process.
    fn watch_loading(&self, page: u64, build: u64, origin: &str) -> windows_core::Result<()> {
        let (origin, refusals) = (origin.to_string(), self.refusals.clone());
        let handler = NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
            let Some(args) = args else { return Ok(()) };
            let mut success = windows_core::BOOL::default();
            let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
            // SAFETY: the event's own arguments and sender, for the duration of the callback.
            unsafe {
                args.IsSuccess(&mut success)?;
                args.WebErrorStatus(&mut status)?;
            }
            let url = match &sender {
                Some(w) => {
                    let mut source = PWSTR::null();
                    unsafe { w.Source(&mut source)? };
                    webview2_com::take_pwstr(source)
                }
                None => String::new(),
            };
            if success.as_bool() && webview::own_origin(&url, &origin) {
                push(PageEvent::Loaded { page, build });
            } else if success.as_bool() {
                refusals.say(&format!(
                    "it went to {url}, which is not a file of its own and could not be refused — \
                     the page is taken down, and the next show builds it again"
                ));
                push(PageEvent::Closed { page, build, why: "failed" });
            } else if status != COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                push(PageEvent::LoadFailed { page, build, why: format!("WebView2 error status {} at {url}", status.0) });
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { self.webview.webview().add_NavigationCompleted(&handler, &mut token) }
    }

    /// Every request that is not for the page's own scheme is answered 403, with the host's
    /// headers, before WebView2 sends it anywhere — whatever the page's policy lets through, and
    /// whatever a navigation that is being refused asks for meanwhile. The page's own scheme is
    /// wry's filter's (`http://<scheme>.*`), answered from the module's folder.
    fn refuse_other_requests(&self, scheme: &str) -> windows_core::Result<()> {
        let env = self.webview.environment();
        let core = self.webview.webview();
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { core.AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)? };
        let ours = format!("http://{scheme}.");
        let headers: String = webview::base_headers().iter().map(|(k, v)| format!("{k}: {v}\n")).collect();
        let refusals = self.refusals.clone();
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            // SAFETY: the event's own arguments, for the duration of the callback.
            unsafe { args.Request()?.Uri(&mut uri)? };
            let uri = webview2_com::take_pwstr(uri);
            if uri.get(..ours.len()).is_some_and(|p| p.eq_ignore_ascii_case(&ours)) {
                return Ok(());
            }
            refusals.say(&format!("a request for {uri} was answered 403 — a page reaches nothing but its own files"));
            // SAFETY: as above; the response is WebView2's own object.
            unsafe {
                let response =
                    env.CreateWebResourceResponse(None, 403, &HSTRING::from("Forbidden"), &HSTRING::from(headers.as_str()))?;
                args.SetResponse(&response)
            }
        }));
        let mut token = 0i64;
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { core.add_WebResourceRequested(&handler, &mut token) }
    }

    /// A page whose browser or renderer process ended is blank and cannot be read: said as a close
    /// (`"failed"`), and the next `show` builds it again. The other processes' failures — the GPU,
    /// a utility process — WebView2 recovers from by itself; they are logged.
    fn watch_process_failures(&self, page: u64, build: u64) -> windows_core::Result<()> {
        let title = self.refusals.title.clone();
        let env = self.webview.environment();
        let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            if let Some(args) = args {
                // SAFETY: the event's own arguments, for the duration of the callback.
                unsafe { args.ProcessFailedKind(&mut kind)? };
            }
            let gone = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
                || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED;
            if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                // Microsoft: the app recreates its web views; the next build makes a new
                // environment as well, rather than trust one whose browser process crashed — if
                // this page's is still the one kept (a page of an earlier browser reporting late
                // leaves a new one alone).
                forget_environment_if(&env);
            }
            crate::logging::line(
                "gui",
                &format!(
                    "page '{title}': a WebView2 process failed (kind {}){}",
                    kind.0,
                    if gone { " — the page is gone, and the next show builds it again" } else { "" }
                ),
            );
            if gone {
                push(PageEvent::Closed { page, build, why: "failed" });
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { self.webview.webview().add_ProcessFailed(&handler, &mut token) }
    }

    /// `window.close()` in the page: wry takes the web view's window down, which leaves the frame
    /// empty. The injected script makes it do nothing; should a page get round that, the page is
    /// gone like a failed one, and the next `show` builds it again.
    fn watch_close_requests(&self, page: u64, build: u64) -> windows_core::Result<()> {
        let title = self.refusals.title.clone();
        let handler = WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
            crate::logging::line(
                "gui",
                &format!("page '{title}': its script closed its web view — the page is gone, and the next show builds it again"),
            );
            push(PageEvent::Closed { page, build, why: "failed" });
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { self.webview.webview().add_WindowCloseRequested(&handler, &mut token) }
    }

    /// A link to another program (`mailto:`, `ms-settings:` …) is a navigation, which the
    /// navigation handler refuses first; this refuses the launch itself as well. Runtimes too old
    /// to report it have only the first guard.
    fn refuse_external_programs(&self) -> windows_core::Result<()> {
        let Ok(core) = self.webview.webview().cast::<ICoreWebView2_18>() else { return Ok(()) };
        let refusals = self.refusals.clone();
        let handler = LaunchingExternalUriSchemeEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut uri = PWSTR::null();
            // SAFETY: the event's own arguments, for the duration of the callback.
            unsafe { args.Uri(&mut uri)? };
            refusals.say(&format!("another program for {} was refused", webview2_com::take_pwstr(uri)));
            unsafe { args.SetCancel(true) }
        }));
        let mut token = 0i64;
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { core.add_LaunchingExternalUriScheme(&handler, &mut token) }
    }

    /// `alert`, `confirm` and `prompt` show nothing and return at once: a dialog of a hidden
    /// page's would be a window of its own on screen, and a page speaks through its live region.
    fn no_script_dialogs(&self) -> windows_core::Result<()> {
        // SAFETY: the web view's own COM object, on its thread.
        unsafe { self.webview.webview().Settings()?.SetAreDefaultScriptDialogsEnabled(false) }
    }
}

/// A process's working set and private bytes. The memory counters need the right to read the
/// process's memory besides the limited query right; a process that will not open with both is
/// asked with the query right alone, and one that is gone or will not answer counts as not
/// measured.
fn process_memory(pid: u32) -> Option<(u64, u64)> {
    // SAFETY: a handle opened here and closed here; the counters are a plain struct.
    unsafe {
        let mut h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if h.is_null() {
            h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        }
        if h.is_null() {
            return None;
        }
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        let ok = K32GetProcessMemoryInfo(h, &mut c as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS, c.cb) != 0;
        CloseHandle(h);
        ok.then_some((c.WorkingSetSize as u64, c.PrivateUsage as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame each kind is built as, with no window made: a tool page in no window list and
    /// with no minimise or maximise box, an app page an ordinary frame with both — and neither
    /// one topmost, nor kept off the taskbar any other way.
    #[test]
    fn a_tool_page_is_a_tool_window_and_an_app_page_an_ordinary_frame() {
        let both = FrameStyle::Caption | FrameStyle::SystemMenu | FrameStyle::CloseBox | FrameStyle::ResizeBorder;
        let tool = frame_style(WindowKind::Tool);
        assert_eq!(tool, both | FrameStyle::ToolWindow);
        let app = frame_style(WindowKind::App);
        assert_eq!(app, both | FrameStyle::MinimizeBox | FrameStyle::MaximizeBox);
        for style in [tool, app] {
            assert!(!style.intersects(FrameStyle::StayOnTop | FrameStyle::NoTaskbar | FrameStyle::FloatOnParent), "{style:?}");
        }
    }
}
