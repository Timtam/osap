//! `host.gui.webview` — the Luau side: the page object a module holds, the host's table of every
//! page, and the drain that hands a page's events to its module. The rules are `webview.rs`; the
//! window is `webview_win.rs` on Windows and `backend/macos/page.rs` on a Mac, which have one
//! interface, so everything here is the same on both (`native_page`): only the log's words for
//! the engine differ (`webview::Engine::words`), and whether the engine is there at all.
//!
//! **Experimental** (stage 1 of the plan of 2026-09-28): the API may change after the Windows
//! probe, `tools/html-probe`, has been run with NVDA, and after the Mac's page has met VoiceOver
//! (stage 4, TODO.md).
//!
//! **Who owns a page.** The module whose code created it names its file — the file is found in
//! that module's folder and the page is served under that module's origin, as every relative path
//! resolves (docs/api/index.md, Paths) — and the module whose VM it was created in owns it: its
//! callbacks are that module's events, disabling it hides the page, and reloading it closes the
//! page. The two differ only for a `code_module`'s code running in a dependent. A module the host's
//! guard stopped (`stops.rs`) is disabled the same way, and turning it on again builds it afresh,
//! as a reload does, which closes its pages.
//!
//! **Nothing here waits.** A build is queued by `show` or `prepare` and done at the end of a tick
//! ([`build_pending`]) — the first one once WebView2's environment has answered, or on a Mac once
//! WebKit has compiled the pages' rule list, which the event loop does not wait for; what the
//! window sends is queued by its callbacks and handed on by [`fire_events`], on the next tick. So
//! a callback of the web view's never runs Luau, and Luau never waits for the web view.
//!
//! **A page's callbacks are its module's events** (`mailbox.rs`), as a key's or a timer's are:
//! [`hand_on`] does the host's part of each event at once — the window, the page's life, the
//! messages that waited for it, the log — and delivers the callback, `Event::Page`, through the
//! owning module's mailbox. So it runs as a handler of that module, in order with its other
//! events — after them while the module is busy — only while the module is enabled and its VM the
//! one that described the page, and a message counts against the page messages a busy module may
//! hold, a budget apart from its keys'. The callback is looked up, and a message decoded, when the
//! event runs ([`open_event`]). As a handler it is an entry of the host's guard (`vm_guard.rs`), as
//! every callback is: one that runs too long, or runs its VM out of memory, stops its module.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use mlua::{Function, Lua, MultiValue, RegistryKey, Table, UserData, UserDataFields, UserDataMethods, Value};

use crate::errors::{gui, HostError};
use crate::mailbox::{self, Event, MailHost, Opened};
use crate::ocr::types::{current_priority, enter_priority, Priority};
use crate::task::Ctx;
use crate::vm_guard::host_call;
use crate::webview::{self, Lifecycle, PageEvent, Phase, Rect, Refusal, ShowStep, Where, WindowKind};
use crate::{appcfg, errors_lua, json, logging, Shared};

// The platform's page window under one name: WebView2's on Windows, WebKit's on a Mac. The two
// have one interface, and `crates/macos-check` holds the Mac's to it from Windows (`page_calls`).
// Elsewhere there is none: `Where::here` answers that no page can be shown there, and nothing
// below that needs a window is built.
#[cfg(windows)]
use crate::webview_win as native_page;
#[cfg(target_os = "macos")]
use crate::backend::page as native_page;

/// What a page's event calls, carried in its module's mailbox (`Event::Page`) and opened when it
/// runs ([`open_event`]).
#[derive(Debug)]
pub(crate) enum PageCall {
    /// `onMessage`, with the JSON text the page's script made of the value: decoded when it runs,
    /// in the VM it runs in.
    Message(String),
    /// `onReady`.
    Ready,
    /// `onClose`, with `"user"` or `"failed"` — and for `"failed"` which failure it was, the
    /// `detail` of the error value beside it.
    Closed(&'static str, Option<String>),
}

impl PageCall {
    /// The callback's name in the log and the error dialog ("… hit an error in a page message
    /// callback").
    pub(crate) fn what(&self) -> &'static str {
        match self {
            PageCall::Message(_) => "page message",
            PageCall::Ready => "page ready",
            PageCall::Closed(..) => "page close",
        }
    }
}

/// What holds the pages and the modules' mailboxes: the host's `Shared`, or a test's holder — so
/// the tests hand events on through [`hand_on`] and the real mailbox, not a copy of their rules.
pub(crate) trait PageHost: MailHost {
    fn pages(&self) -> &Pages;
}

impl PageHost for Shared {
    fn pages(&self) -> &Pages {
        &self.pages
    }
}

/// Every page of every module, by the id its handle carries.
#[derive(Default)]
pub(crate) struct Pages {
    next: Cell<u64>,
    next_build: Cell<u64>,
    entries: RefCell<HashMap<u64, Entry>>,
    /// Pages whose `show` asked for a build, built at the end of the tick.
    builds: RefCell<Vec<u64>>,
    /// Pages whose memory is to be logged at the end of the tick — once ready, and after the first
    /// show of each build — so the query is outside the `show` a module times. WebView2's processes
    /// on Windows; a Mac says once that it does not measure WebKit's.
    memory_due: RefCell<Vec<u64>>,
}

struct Entry {
    /// The module whose VM created the page: its callbacks are events of this module's, delivered
    /// to that VM (`lua`) through its mailbox — which drops them while the module is disabled, and
    /// for a VM since rebuilt at the same index.
    owner: usize,
    /// The module whose file the page is, for the log.
    module: String,
    title: String,
    /// The window it is in (`window = "tool" | "app"`), fixed when it is described.
    kind: WindowKind,
    /// Whether that window makes the application the active one when it takes the keyboard
    /// (`activates`, `true` unless a tool page says `false`): fixed when it is described, as a
    /// Mac's panel is made with it.
    activates: bool,
    /// That module's folder, canonical.
    root: PathBuf,
    scheme: String,
    /// The entry file's path under `root`, `/` between its parts.
    rel: String,
    lua: Lua,
    on_message: RegistryKey,
    on_ready: Option<RegistryKey>,
    on_close: Option<RegistryKey>,
    life: Lifecycle,
    /// The build the page's window belongs to; 0 before the first, after `close` and after a
    /// failure. An event from any other build is dropped when it arrives; one that arrived and
    /// waits in a busy module's mailbox is dropped when it runs only if the module closed the page
    /// since (`closed_upto`).
    build: u64,
    /// The newest build of any page when the module last closed this one, 0 before: every build
    /// of the page up to it is one the module closed, and nothing it sent runs ([`open_event`]).
    /// A build ends only so, by a failure, or at its creation, which leaves no window to send
    /// anything — so a build after it that is not the current one is one a failure ended, whose
    /// events still run, in order, however many failed while the module waited.
    closed_upto: u64,
    /// The window's id, from the first time it was shown until it is closed.
    window: Option<i64>,
    /// When the `show` or `prepare` that started the current build was made, and the lane its
    /// `onReady` runs in: for the "ready after" line, and because an answer runs in the lane of
    /// the call that asked for it, as a read's does — the interactive one when any `show` or
    /// `prepare` made while the build was under way was interactive ([`note_asked`]).
    asked: Option<(Instant, Priority)>,
    /// Whether the current build's wait for WebView2's environment — on a Mac, the pages' rule
    /// list — has been said.
    env_said: bool,
    /// Whether the page's last show put it over the rectangle to avoid: the explanation is said
    /// once for a run of such shows, and again after one that found room beside it.
    over_said: bool,
    /// Why the last build failed, as the log says it, until the `show` or `prepare` that says
    /// `could not be built` gives it as its error value's `detail`.
    failed_why: Option<String>,
    /// Whether the current build has been shown yet: its first show logs the memory.
    #[cfg(any(windows, target_os = "macos"))]
    shown_this_build: bool,
    /// Whether the current build already failed once in an environment kept from before, and is
    /// being tried again in a new one: a second failure is the page's. Windows' only: a Mac's rule
    /// list is never one to try again in a new one.
    #[cfg(any(windows, target_os = "macos"))]
    retried: bool,
    #[cfg(any(windows, target_os = "macos"))]
    native: Option<Rc<native_page::Native>>,
}

impl Entry {
    fn label(&self) -> String {
        format!("[{}] page '{}'", self.module, self.title)
    }
}

/// The platform's window of an entry, cloned out so no borrow of the table is held while the
/// window system is called (it dispatches messages, and a message can reach code that asks the
/// table again).
#[cfg(any(windows, target_os = "macos"))]
type NativeRef = Option<Rc<native_page::Native>>;
#[cfg(not(any(windows, target_os = "macos")))]
type NativeRef = Option<std::convert::Infallible>;

#[cfg(any(windows, target_os = "macos"))]
fn native_of(e: &Entry) -> NativeRef {
    e.native.clone()
}
#[cfg(not(any(windows, target_os = "macos")))]
fn native_of(_: &Entry) -> NativeRef {
    None
}

/// `host.gui`: its one function so far. It, and each method of the page object, names itself
/// for the host's guard (`host_call!`), as every binding does: a stop while a `show` waits on the
/// window system says "in page:show".
pub(crate) fn install(lua: &Lua, host: &Table, shared: &Rc<Shared>, idx: usize) -> mlua::Result<()> {
    let gui = lua.create_table()?;
    let sh = shared.clone();
    gui.set(
        "webview",
        lua.create_function(move |lua, opts: Value| {
            host_call!("host.gui.webview");
            create(lua, &sh, idx, opts)
        })?,
    )?;
    host.set("gui", gui)?;
    Ok(())
}

fn fail(text: String) -> mlua::Error {
    mlua::Error::external(text)
}

const CALL: &str = "host.gui.webview";
const OPTIONS: [&str; 7] = ["file", "title", "window", "activates", "onMessage", "onReady", "onClose"];

/// A callback option: a function, or absent where `required` is false.
fn callback_option(t: &Table, key: &str, required: bool) -> mlua::Result<Option<Function>> {
    match t.get::<Value>(key)? {
        Value::Function(f) => Ok(Some(f)),
        Value::Nil if !required => Ok(None),
        other => Err(fail(format!(
            "{CALL}: {key} is a function, not a {}",
            if other.is_nil() { "nil" } else { json::luau_type(&other) }
        ))),
    }
}

/// A string option that must be there and not be empty.
fn text_option(t: &Table, key: &str) -> mlua::Result<String> {
    match t.get::<Value>(key)? {
        Value::String(s) => {
            let s = s.to_str()?.to_string();
            if s.is_empty() {
                return Err(fail(format!("{CALL}: {key} is empty")));
            }
            Ok(s)
        }
        other => Err(fail(format!(
            "{CALL}: {key} is a string, not a {}",
            if other.is_nil() { "nil" } else { json::luau_type(&other) }
        ))),
    }
}

/// The `window` option: `"tool"` when it is absent, and only the two words.
fn window_option(t: &Table) -> mlua::Result<WindowKind> {
    let words = || format!("\"{}\" or \"{}\"", WindowKind::NAMES[0], WindowKind::NAMES[1]);
    match t.get::<Value>("window")? {
        Value::Nil => Ok(WindowKind::default()),
        Value::String(s) => {
            let s = s.to_string_lossy();
            WindowKind::parse(&s).ok_or_else(|| fail(format!("{CALL}: window is {}, not \"{s}\"", words())))
        }
        other => Err(fail(format!("{CALL}: window is {}, not a {}", words(), json::luau_type(&other)))),
    }
}

/// The `activates` option: `true` when it is absent, a boolean otherwise, and `false` only for a
/// tool page (`WindowKind::allows_activates`).
fn activates_option(t: &Table, kind: WindowKind) -> mlua::Result<bool> {
    let activates = crate::opt_bool(Some(t), "activates", true, CALL)?;
    if !kind.allows_activates(activates) {
        return Err(fail(format!(
            "{CALL}: activates = false is for a tool page; an app page (window = \"{}\") is a window like any \
             program's, which makes the application the active one when it takes the keyboard",
            kind.name()
        )));
    }
    Ok(activates)
}

/// What `host.gui.webview` is given, read strictly.
struct PageOptions {
    file: String,
    title: String,
    kind: WindowKind,
    activates: bool,
    on_message: Function,
    on_ready: Option<Function>,
    on_close: Option<Function>,
}

fn page_options(opts: Value) -> mlua::Result<PageOptions> {
    let Value::Table(t) = opts else {
        return Err(fail(format!("{CALL}: takes a table of options, not a {}", json::luau_type(&opts))));
    };
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, _) = pair?;
        let known = matches!(&k, Value::String(s) if s.to_str().is_ok_and(|s| OPTIONS.contains(&&*s)));
        if !known {
            let name = match &k {
                Value::String(s) => format!("'{}'", s.to_string_lossy()),
                other => format!("a {} key", json::luau_type(other)),
            };
            return Err(fail(format!(
                "{CALL}: {name} is not an option; the options are file, title, window, activates, \
                 onMessage, onReady and onClose"
            )));
        }
    }
    let (file, title) = (text_option(&t, "file")?, text_option(&t, "title")?);
    let kind = window_option(&t)?;
    Ok(PageOptions {
        file,
        title,
        kind,
        activates: activates_option(&t, kind)?,
        // `required`: an absent one has raised by now.
        on_message: callback_option(&t, "onMessage", true)?.expect("required"),
        on_ready: callback_option(&t, "onReady", false)?,
        on_close: callback_option(&t, "onClose", false)?,
    })
}

/// `host.gui.webview{ file, title, window?, activates?, onMessage, onReady?, onClose? }` — describes
/// a page. Builds nothing: the first `show` does. Raises for a mistake in the options, and for a
/// `file` that is not a file inside the module's folder, at the caller's line.
fn create(lua: &Lua, sh: &Rc<Shared>, idx: usize, opts: Value) -> mlua::Result<PageHandle> {
    let PageOptions { file, title, kind, activates, on_message, on_ready, on_close } = page_options(opts)?;
    let module = sh.ids.borrow().get(idx).cloned().unwrap_or_default();
    let root = sh
        .roots
        .borrow()
        .get(idx)
        .and_then(|r| r.canonicalize().ok())
        .ok_or_else(|| fail(format!("{CALL}: the folder of module '{module}' cannot be read")))?;
    let found = webview::resolve_file(&root, &file).ok_or_else(|| {
        fail(format!(
            "{CALL}: file '{file}' is not a file inside the folder of module '{module}' — the path \
             is relative to it, with '/' between its parts, and may not leave it"
        ))
    })?;
    let rel = webview::url_path(&root, &found).unwrap_or_else(|| file.clone());
    let owner = crate::image_search::vm_owner(lua).map_or(idx, |o| o.idx);
    let id = sh.pages.next.get() + 1;
    sh.pages.next.set(id);
    let entry = Entry {
        owner,
        scheme: webview::scheme_for(&module),
        module,
        title,
        kind,
        activates,
        root,
        rel,
        on_message: lua.create_registry_value(on_message)?,
        on_ready: on_ready.map(|f| lua.create_registry_value(f)).transpose()?,
        on_close: on_close.map(|f| lua.create_registry_value(f)).transpose()?,
        lua: lua.clone(),
        life: Lifecycle::default(),
        build: 0,
        closed_upto: 0,
        window: None,
        asked: None,
        env_said: false,
        over_said: false,
        failed_why: None,
        #[cfg(any(windows, target_os = "macos"))]
        shown_this_build: false,
        #[cfg(any(windows, target_os = "macos"))]
        retried: false,
        #[cfg(any(windows, target_os = "macos"))]
        native: None,
    };
    // The model is said only for a page that asks for the other one, so a page described as stage 1
    // described it is said as it was.
    let model = if entry.activates { String::new() } else { format!(", {}", words().not_activating) };
    logging::line(
        "gui",
        &format!(
            "{}: described, from {} (origin {}), window \"{}\"{model}",
            entry.label(),
            entry.rel,
            webview::origin_for(&entry.scheme),
            entry.kind.name()
        ),
    );
    sh.pages.entries.borrow_mut().insert(id, entry);
    Ok(PageHandle { id, sh: sh.clone() })
}

/// What a module holds: the page's id, and the way back to the table.
pub(crate) struct PageHandle {
    id: u64,
    sh: Rc<Shared>,
}

impl UserData for PageHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("window", |_, p| {
            Ok(p.sh.pages.entries.borrow().get(&p.id).and_then(|e| e.window))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("show", |lua, p, opts: Value| {
            host_call!("page:show");
            show(lua, &p.sh, p.id, opts)
        });
        methods.add_method("prepare", |lua, p, ()| {
            host_call!("page:prepare");
            prepare(lua, &p.sh, p.id)
        });
        methods.add_method("focus", |lua, p, ()| {
            host_call!("page:focus");
            errors_lua::done(lua, &focus(&p.sh, p.id))
        });
        methods.add_method("hide", |_, p, ()| {
            host_call!("page:hide");
            hide(&p.sh, p.id);
            Ok(())
        });
        methods.add_method("post", |lua, p, value: Value| {
            host_call!("page:post");
            let posted = post(lua, &p.sh, p.id, value)?;
            errors_lua::done(lua, &posted)
        });
        methods.add_method("isShown", |_, p, ()| {
            host_call!("page:isShown");
            Ok(is_shown(&p.sh, p.id))
        });
        methods.add_method("close", |_, p, ()| {
            host_call!("page:close");
            close(&p.sh.pages, p.id);
            Ok(())
        });
    }
}

/// `show`'s options: `focus` (default `true`) and `avoid`, a `{ x, y, w, h }` in screen
/// coordinates — a window table's `bounds`.
fn show_options(opts: Value) -> mlua::Result<(bool, Option<Rect>)> {
    const SHOW: &str = "page:show";
    let t = match opts {
        Value::Nil => return Ok((true, None)),
        Value::Table(t) => t,
        other => return Err(fail(format!("{SHOW}: takes a table of options or nothing, not a {}", json::luau_type(&other)))),
    };
    for pair in t.clone().pairs::<Value, Value>() {
        let (k, _) = pair?;
        let known = matches!(&k, Value::String(s) if s.to_str().is_ok_and(|s| &*s == "focus" || &*s == "avoid"));
        if !known {
            return Err(fail(format!("{SHOW}: the options are focus and avoid")));
        }
    }
    let focus = crate::opt_bool(Some(&t), "focus", true, SHOW)?;
    let avoid = match t.get::<Value>("avoid")? {
        Value::Nil => None,
        Value::Table(r) => {
            let side = |k: &str| -> mlua::Result<i32> {
                match r.get::<Value>(k)? {
                    Value::Integer(i) => Ok(i as i32),
                    Value::Number(n) if n.is_finite() => Ok(n as i32),
                    other => Err(fail(format!(
                        "{SHOW}: avoid.{k} is a number, not a {}",
                        if other.is_nil() { "nil" } else { json::luau_type(&other) }
                    ))),
                }
            };
            let rect = Rect { x: side("x")?, y: side("y")?, w: side("w")?, h: side("h")? };
            if rect.w <= 0 || rect.h <= 0 {
                return Err(fail(format!("{SHOW}: avoid has no area ({} by {})", rect.w, rect.h)));
            }
            Some(rect)
        }
        other => return Err(fail(format!("{SHOW}: avoid is a table {{ x, y, w, h }}, not a {}", json::luau_type(&other)))),
    };
    Ok((focus, avoid))
}

/// Whether the platform's web engine is installed — WebView2's runtime on Windows, asked each time;
/// a Mac's WebKit is part of the system and always there. Said in the log the first time it is
/// asked, with its version.
fn runtime_present() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SAID: AtomicBool = AtomicBool::new(false);
    #[cfg(windows)]
    {
        let version = native_page::runtime_version();
        if !SAID.swap(true, Ordering::Relaxed) {
            logging::line(
                "gui",
                &match &version {
                    Some(v) => format!("the WebView2 runtime is installed, version {v}"),
                    None => "the WebView2 runtime is not installed: no page can be shown".to_string(),
                },
            );
        }
        version.is_some()
    }
    #[cfg(target_os = "macos")]
    {
        if !SAID.swap(true, Ordering::Relaxed) {
            logging::line(
                "gui",
                &match native_page::runtime_version() {
                    Some(v) => format!("pages run in the system's web engine, {v}"),
                    None => "pages run in the system's web engine, WebKit, whose version could not be read".to_string(),
                },
            );
        }
        true
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (&SAID, Ordering::Relaxed);
        false
    }
}

/// The log's words for this platform's engine.
fn words() -> webview::Words {
    webview::Engine::here().words()
}

/// What a page's method answers once its module was reloaded: the page object it holds is of the
/// VM before, and no longer names a page. Said in the log under the page's number, as its label
/// went with it.
fn gone(id: u64, what: &str) -> HostError {
    let e = HostError::of(&gui::GONE);
    logging::line("gui", &format!("page {id}: {what} answered false, \"{e}\""));
    e
}

/// What a refusal of `show` or `prepare` answers: its error value, and for a build that failed
/// why it failed, as `detail` — said once, as the refusal is.
fn refusal(e: &mut Entry, step: ShowStep) -> Option<HostError> {
    let ShowStep::Refuse(r) = step else { return None };
    let why = if r == Refusal::BuildFailed { e.failed_why.take() } else { None };
    Some(match why {
        Some(why) => r.error().with_detail(why),
        None => r.error(),
    })
}

/// What a `show` or `prepare` notes on the page once `on_show` has decided `step`: the call that
/// starts a build, its time and its lane; a call made while that build is under way — `"not ready
/// yet"` — raises the lane to its own when that is the interactive one, since `onReady` answers it
/// too: a key that asks for a page a poll began building waits for it as a key, as `task::promote`
/// raises the read a key waits on. The time stays the first call's.
fn note_asked(e: &mut Entry, step: ShowStep) {
    match step {
        ShowStep::Build => e.asked = Some((Instant::now(), current_priority())),
        ShowStep::Refuse(Refusal::NotReady) if current_priority() == Priority::Interactive => {
            if let Some((_, lane)) = e.asked.as_mut() {
                *lane = Priority::Interactive;
            }
        }
        _ => {}
    }
}

/// `page:show{ focus?, avoid? } -> true | true, err | false, err` — `true, err` the partial answer
/// of a page in front over the rectangle to avoid ([`webview::over`]).
fn show(lua: &Lua, sh: &Rc<Shared>, id: u64, opts: Value) -> mlua::Result<MultiValue> {
    let (focus, avoid) = show_options(opts)?;
    let at = Where::here(appcfg::headless());
    let decided = {
        let mut entries = sh.pages.entries.borrow_mut();
        entries.get_mut(&id).map(|e| {
            let step = e.life.on_show(at, runtime_present);
            note_asked(e, step);
            (step, e.label(), refusal(e, step))
        })
    };
    let Some((step, label, refused)) = decided else {
        return errors_lua::with_error(lua, Value::Boolean(false), &gone(id, "show"));
    };
    let (shown, err) = match step {
        ShowStep::Refuse(r) => {
            logging::line("gui", &format!("{label}: show answered false, \"{}\"", r.error()));
            (false, refused)
        }
        ShowStep::Build => {
            sh.pages.builds.borrow_mut().push(id);
            logging::line(
                "gui",
                &format!(
                    "{label}: show answered false, \"{}\" — it is built at the end of this tick, once {}",
                    Refusal::NotReady.error(),
                    words().env_there
                ),
            );
            (false, Some(Refusal::NotReady.error()))
        }
        ShowStep::Show => show_built(sh, id, &label, focus, avoid),
    };
    errors_lua::answer(lua, Value::Boolean(shown), err.as_ref())
}

/// `page:prepare() -> true | false, err` — builds the page without showing it: `true` when it is
/// ready already; otherwise the answers of `show` that come before a page is shown, in the same
/// order, a build started or under way being `not_ready`. `onReady` says when it is ready, and the
/// `show` after that shows it at once.
fn prepare(lua: &Lua, sh: &Rc<Shared>, id: u64) -> mlua::Result<MultiValue> {
    let at = Where::here(appcfg::headless());
    let decided = {
        let mut entries = sh.pages.entries.borrow_mut();
        entries.get_mut(&id).map(|e| {
            let step = e.life.on_show(at, runtime_present);
            note_asked(e, step);
            (step, e.label(), refusal(e, step))
        })
    };
    let Some((step, label, refused)) = decided else {
        return errors_lua::with_error(lua, Value::Boolean(false), &gone(id, "prepare"));
    };
    let err = match step {
        ShowStep::Refuse(Refusal::NotReady) => refused,
        ShowStep::Refuse(r) => {
            logging::line("gui", &format!("{label}: prepare answered false, \"{}\"", r.error()));
            refused
        }
        ShowStep::Build => {
            sh.pages.builds.borrow_mut().push(id);
            logging::line(
                "gui",
                &format!("{label}: prepared — it is built, hidden, at the end of this tick, once {}", words().env_there),
            );
            Some(Refusal::NotReady.error())
        }
        ShowStep::Show => return errors_lua::done(lua, &Ok(())),
    };
    errors_lua::answer(lua, Value::Boolean(false), err.as_ref())
}

/// A `show` of a built page: whether it is shown as asked, and the error value beside that.
#[cfg(any(windows, target_os = "macos"))]
fn show_built(sh: &Rc<Shared>, id: u64, label: &str, focus: bool, avoid: Option<Rect>) -> (bool, Option<HostError>) {
    let found = sh.pages.entries.borrow().get(&id).map(|e| (native_of(e), e.kind));
    let Some((Some(native), kind)) = found else {
        return (false, Some(Refusal::NotReady.error()));
    };
    let started = Instant::now();
    // Beside `avoid`, or — with no room beside it — over it, in a corner, at its smallest
    // (`webview::place`), for a show that brings it to the front: never refused for want of room,
    // so the page is always reachable. A show without `focus` never leaves it over `avoid`.
    let plan = native.plan(avoid);
    let over = match plan {
        Some(webview::Placement::Over { covered, .. }) => Some(covered),
        _ => None,
    };
    // An app page the user has open is the user's window, as any program's is: a show that would
    // leave it over `avoid` without the keyboard neither hides it nor moves it, and one that is to
    // put it there moves it only once it is in front.
    let open_app = over.is_some() && kind == WindowKind::App && native.is_shown();
    // Over `avoid` without the keyboard: not shown at all (`webview::over_refused`), and not
    // moved either.
    if let (Some(_), Some(r)) = (over, webview::over_refused(focus, false)) {
        let hidden = if open_app {
            "; it is an application window the user has open, so it stays as and where it is"
        } else if native.is_shown() {
            native.hide();
            "; it was shown and is hidden"
        } else {
            ""
        };
        logging::line(
            "gui",
            &format!(
                "{label}: show answered false, \"{}\" — without the keyboard the page never goes over the \
                 rectangle to avoid ({avoid:?}){hidden}",
                r.error()
            ),
        );
        return (false, Some(r.error()));
    }
    let shown = if open_app {
        // In front first, where the user left it — not raised before that, so a decline leaves it
        // as it was — and only then over `avoid`. `focus` restores a minimised one. On a Mac the
        // front is only asked for here, and the window goes over `avoid` when it gets the keyboard
        // (`backend/macos/page.rs`): one that does not get it stays where it was — raised, though,
        // as AppKit orders it in front with the request.
        if native.focus() {
            native.put(avoid, plan);
            Ok(())
        } else {
            Err(Refusal::Declined)
        }
    } else {
        native.put(avoid, plan);
        native.show(focus)
    };
    // Over `avoid` and declined — raised without the keyboard: hidden again. Not an app page the
    // user has open, which was not moved.
    let unshown = over.is_some() && !open_app && webview::over_refused(focus, shown.is_err()).is_some();
    if unshown {
        native.hide();
    }
    // The first show of this build that succeeded: its memory line is due. The explanation of a
    // page over the rectangle is said once for a run of such shows. (Over `avoid`, only a show
    // with `focus` gets here, and it stays there exactly when it was not declined.)
    let stays_over = over.is_some() && shown.is_ok();
    let (first, explain) = match sh.pages.entries.borrow_mut().get_mut(&id) {
        Some(e) => {
            e.window = Some(native.hwnd() as i64);
            let explain = stays_over && !std::mem::replace(&mut e.over_said, true);
            if !stays_over {
                e.over_said = false;
            }
            (shown.is_ok() && !std::mem::replace(&mut e.shown_this_build, true), explain)
        }
        None => (false, false),
    };
    let ms = started.elapsed().as_millis();
    let front = sh.backend.foreground_window();
    let system = words().system;
    let place = match (avoid, over) {
        (Some(a), Some(_)) if open_app && shown.is_err() => {
            format!(", left where it was (no room beside {},{} {}x{})", a.x, a.y, a.w, a.h)
        }
        (Some(a), Some(covered)) => {
            let area = (i64::from(a.w) * i64::from(a.h)).max(1);
            format!(
                ", OVER {}% of {},{} {}x{} (no room beside it)",
                covered * 100 / area,
                a.x,
                a.y,
                a.w,
                a.h
            )
        }
        (Some(a), None) => format!(", beside {},{} {}x{}", a.x, a.y, a.w, a.h),
        (None, _) => String::new(),
    };
    logging::line(
        "gui",
        &format!(
            "{label}: {} in {ms} ms{place}; in front now: {}; on top at its centre: {}",
            match (&shown, focus, unshown) {
                (Ok(()), true, _) => words().front_taken.to_string(),
                (Ok(()), false, _) => "shown without taking the front".to_string(),
                (Err(_), _, false) if open_app => format!(
                    "{system} DECLINED to bring it to the front — an application window the user has \
                     open, so it stays where it was, neither moved over the rectangle to avoid nor \
                     hidden (restored, if it was minimised)"
                ),
                (Err(_), _, false) => format!("shown, and {system} DECLINED to bring it to the front"),
                (Err(_), _, true) => format!(
                    "shown, and {system} DECLINED to bring it to the front — over the rectangle to avoid \
                     and not in front, so hidden again"
                ),
            },
            front_words(sh, native.hwnd(), front),
            if native.on_top_at_centre() { "yes".to_string() } else { format!("NO, {} is there", native.covering_at_centre()) },
        ),
    );
    if explain {
        logging::line(
            "gui",
            &format!(
                "{label}: there is no room beside the rectangle to avoid on any monitor (a maximised or \
                 full-screen window, say), so the page is over part of it — at its smallest, in the \
                 corner of that monitor where it covers the least — and `show` answers true, \"{}\"; \
                 only while it is in front: declined, it is hidden again. Anything clicked in the \
                 plug-in, and any screen read of it, must bring the plug-in window to the front \
                 first; the page is not topmost, so the plug-in window is then drawn over it",
                webview::over()
            ),
        );
    }
    if first {
        sh.pages.memory_due.borrow_mut().push(id);
    }
    match shown {
        Ok(()) => (true, over.map(|_| webview::over())),
        Err(r) => (false, Some(r.error())),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn show_built(_: &Rc<Shared>, _: u64, _: &str, _: bool, _: Option<Rect>) -> (bool, Option<HostError>) {
    // `on_show` never answers `Show` where no page can be built.
    (false, Some(Refusal::Unsupported.error()))
}

/// The foreground window, for a log line: the page itself, or another window by its id.
#[cfg(any(windows, target_os = "macos"))]
fn front_words(_sh: &Shared, page: isize, front: Option<crate::backend::Foreground>) -> String {
    match front {
        Some(f) if f.id == page => "this page".to_string(),
        Some(f) => format!("window {} of process {}", f.id, f.pid),
        None => "no window".to_string(),
    }
}

/// `page:focus() -> true | false, err` — `false` with why: the page gone with its module, a
/// headless run or a platform with no page window ([`nowhere`]), not built, hidden, or the system
/// declining.
fn focus(sh: &Rc<Shared>, id: u64) -> Result<(), HostError> {
    let (native, label) = match sh.pages.entries.borrow().get(&id) {
        Some(e) => (native_of(e), e.label()),
        None => return Err(gone(id, "focus")),
    };
    // Where no page can ever be shown, none is built: refused as `show` is, not "not built yet".
    let at = Where::here(appcfg::headless());
    if !at.can_show() {
        let e = nowhere(at);
        logging::line("gui", &format!("{label}: focus answered false, \"{e}\""));
        return Err(e);
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        let Some(native) = native else {
            logging::line("gui", &format!("{label}: focus answered false — the page is not built"));
            return Err(HostError::of(&gui::NOT_BUILT));
        };
        let accepted = native.focus();
        // Asked again rather than before: the window system says why it was refused.
        let shown = accepted || native.is_shown();
        let front = sh.backend.foreground_window();
        logging::line(
            "gui",
            &format!(
                "{label}: focus {}; in front now: {}",
                if accepted {
                    words().focus_taken
                } else if shown {
                    "DECLINED"
                } else {
                    "refused — the page is hidden"
                },
                front_words(sh, native.hwnd(), front)
            ),
        );
        focus_answer(accepted, shown)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (native, label);
        Err(Refusal::Unsupported.error())
    }
}

/// What a `focus` of a built page answers: `Ok` when the system accepted; otherwise declined, for a
/// page that is shown, or hidden.
fn focus_answer(accepted: bool, shown: bool) -> Result<(), HostError> {
    match (accepted, shown) {
        (true, _) => Ok(()),
        (false, true) => Err(Refusal::Declined.error()),
        (false, false) => Err(HostError::of(&gui::HIDDEN)),
    }
}

/// Why a `focus` or a `post` is refused where no page can ever be shown ([`Where::can_show`]): a
/// headless run, or a platform with no page window.
fn nowhere(at: Where) -> HostError {
    if at.headless {
        Refusal::Headless.error()
    } else {
        Refusal::Unsupported.error()
    }
}

/// `page:hide()`
fn hide(sh: &Rc<Shared>, id: u64) {
    let (native, label) = match sh.pages.entries.borrow().get(&id) {
        Some(e) => (native_of(e), e.label()),
        None => return,
    };
    #[cfg(any(windows, target_os = "macos"))]
    if let Some(native) = native {
        native.hide();
        logging::line("gui", &format!("{label}: hidden"));
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = (native, label);
}

/// `page:isShown()`, asked of the window system.
fn is_shown(sh: &Rc<Shared>, id: u64) -> bool {
    let native = sh.pages.entries.borrow().get(&id).and_then(native_of);
    #[cfg(any(windows, target_os = "macos"))]
    return native.is_some_and(|n| n.is_shown());
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = native;
        false
    }
}

/// `page:post(value) -> true | false, err`: encoded here, by the host, as `host.json.encode`
/// encodes — a value it cannot encode raises; delivered at once to a ready page, kept in order until
/// it is ready otherwise — `true` either way — and refused where no page can ever be shown, for a
/// page gone with its module, and when the engine would not take it.
fn post(lua: &Lua, sh: &Rc<Shared>, id: u64, value: Value) -> mlua::Result<Result<(), HostError>> {
    let text = json::encode(lua, (value, Value::Nil)).map_err(|e| {
        let why = e.to_string();
        fail(why.replacen("host.json.encode", "page:post", 1))
    })?;
    let (deliver, dropped, native, label) = {
        let mut entries = sh.pages.entries.borrow_mut();
        let Some(e) = entries.get_mut(&id) else { return Ok(Err(gone(id, "post"))) };
        let at = Where::here(appcfg::headless());
        if !at.can_show() {
            // Dropped: no page can ever be shown here to take it.
            return Ok(Err(nowhere(at)));
        }
        let deliver = e.life.post(text);
        (deliver, e.life.take_dropped(), native_of(e), e.label())
    };
    if dropped > 0 {
        logging::line(
            "gui",
            &format!("{label}: {dropped} message(s) that waited for the page were dropped — no more than {} wait", webview::MAX_QUEUED_POSTS),
        );
    }
    #[cfg(any(windows, target_os = "macos"))]
    if let (Some(json), Some(native)) = (deliver, native) {
        if let Err(e) = native.post_json(&json) {
            let err = HostError::with(&gui::NOT_HANDED, format!("a message could not be handed to the page: {e}"));
            logging::line("gui", &format!("{label}: {err}"));
            return Ok(Err(err));
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = (deliver, native);
    Ok(Ok(()))
}

/// `page:close()`: the window and its web view go; the description stays, and the next `show`
/// builds it again. What the page sent that still waits in its module's mailbox is not run: the
/// module said it is done with that page.
pub(crate) fn close(pages: &Pages, id: u64) {
    let (native, label) = {
        let mut entries = pages.entries.borrow_mut();
        let Some(e) = entries.get_mut(&id) else { return };
        e.life.closed();
        e.build = 0;
        e.closed_upto = pages.next_build.get();
        e.window = None;
        e.asked = None;
        #[cfg(any(windows, target_os = "macos"))]
        let native = e.native.take();
        #[cfg(not(any(windows, target_os = "macos")))]
        let native: NativeRef = None;
        (native, e.label())
    };
    take_down(native);
    logging::line("gui", &format!("{label}: closed"));
}

/// Lets go of a page's window, outside any borrow of the table. The window is taken down when the
/// last reference goes (`Native`'s `Drop`), which is this one: the drain lets go of its own before
/// it delivers a module's callback, so a `close` from inside `onReady` or `onClose` takes the
/// window down too.
fn take_down(native: NativeRef) {
    #[cfg(any(windows, target_os = "macos"))]
    drop(native);
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = native;
}

/// Builds the pages whose `show` or `prepare` asked for it, at the end of the tick, and logs the
/// memory of the pages due for it (WebView2's processes; a Mac says once that it measures none).
///
/// **The environment first, without waiting.** A page needs WebView2's environment — its browser
/// process — which the first build asks for and the event loop does not wait for
/// (`native_page::environment`): until WebView2 has answered, the page stays queued, and the tick
/// after the answer builds it. A failed answer is a failed build for every page that waited for
/// it, said by the next `show`; one no page waited for any more is dropped at the end of the
/// tick. A build that fails in an environment a page was built in before — whose browser process
/// may have gone, or whose runtime version an update removed — is tried once more, in a new one.
/// On a Mac the same answers come from the pages' rule list, which WebKit compiles once a session
/// off the event loop and which is never one to try again in.
///
/// **Then the build, which waits.** wry makes the page's controller in the environment in a
/// message loop of its own — on a Mac the window and the web view, with no controller to wait for.
/// Outside Luau: the tick's re-entrancy guard keeps a nested tick from running modules meanwhile.
/// A page closed while it was being built gets nothing: the new window is taken down again. Should
/// WebView2 never answer, the event loop waits here with it — never seen, and the reason for the
/// line before each build: the log's last word is then `building …`.
pub(crate) fn build_pending(sh: &Shared) {
    let ids = std::mem::take(&mut *sh.pages.builds.borrow_mut());
    #[cfg(any(windows, target_os = "macos"))]
    let mut waiting: Vec<u64> = Vec::new();
    // Pages to build again in a new environment, at the next tick.
    #[cfg(any(windows, target_os = "macos"))]
    let mut again: Vec<u64> = Vec::new();
    // The environment's failure, once one page has taken it: every other page of this pass fails
    // with it rather than start a new creation.
    #[cfg(any(windows, target_os = "macos"))]
    let mut failed: Option<String> = None;
    #[cfg(any(windows, target_os = "macos"))]
    for id in ids {
        let queued = {
            let entries = sh.pages.entries.borrow();
            entries.get(&id).is_some_and(|e| e.life.phase() == Phase::Building && e.native.is_none())
        };
        if !queued || waiting.contains(&id) {
            continue;
        }
        let now = match &failed {
            Some(why) => native_page::EnvNow::Failed(why.clone()),
            None => native_page::environment(),
        };
        let (env, reused) = match now {
            native_page::EnvNow::Ready(env, reused) => (env, reused),
            native_page::EnvNow::Waiting => {
                if let Some(e) = sh.pages.entries.borrow_mut().get_mut(&id) {
                    if !std::mem::replace(&mut e.env_said, true) {
                        logging::line(
                            "gui",
                            &format!("{}: waits for {}; the event loop goes on meanwhile", e.label(), words().env),
                        );
                    }
                }
                waiting.push(id);
                continue;
            }
            native_page::EnvNow::Failed(why) => {
                failed = Some(why.clone());
                if let Some(e) = sh.pages.entries.borrow_mut().get_mut(&id) {
                    e.life.built(false);
                    e.build = 0;
                    e.env_said = false;
                    e.retried = false;
                    e.failed_why = Some(format!("{} ({why})", words().env_failed));
                    logging::line(
                        "gui",
                        &format!(
                            "{}: could not be built: {} ({why}); the next show answers \"{}\" and tries again",
                            e.label(),
                            words().env_failed,
                            Refusal::BuildFailed.error()
                        ),
                    );
                }
                continue;
            }
        };
        let (spec, label) = {
            let mut entries = sh.pages.entries.borrow_mut();
            let Some(e) = entries.get_mut(&id) else { continue };
            let build = sh.pages.next_build.get() + 1;
            sh.pages.next_build.set(build);
            e.build = build;
            e.env_said = false;
            e.shown_this_build = false;
            let spec = native_page::BuildSpec {
                page: id,
                build,
                kind: e.kind,
                activates: e.activates,
                title: e.title.clone(),
                scheme: e.scheme.clone(),
                root: e.root.clone(),
                entry_url: webview::entry_url(&e.scheme, &e.rel),
            };
            (spec, e.label())
        };
        logging::line(
            "gui",
            &format!("{label}: {}", words().building),
        );
        let started = Instant::now();
        let result = native_page::build(&spec, &env);
        let ms = started.elapsed().as_millis();
        let leftover = {
            let mut entries = sh.pages.entries.borrow_mut();
            match entries.get_mut(&id) {
                Some(e) if e.build == spec.build && e.life.phase() == Phase::Building => {
                    match result {
                        Ok(native) => {
                            e.native = Some(Rc::new(native));
                            e.life.built(true);
                            e.retried = false;
                            logging::line("gui", &format!("{}: built in {ms} ms; loading {}", e.label(), spec.entry_url));
                            None
                        }
                        Err(why) if reused && !e.retried => {
                            // Still building: the next tick asks for a new environment.
                            e.retried = true;
                            e.build = 0;
                            again.push(id);
                            logging::line(
                                "gui",
                                &format!(
                                    "{}: could not be built ({ms} ms) in the environment an earlier page was built \
                                     in: {why} — tried once more, in a new environment",
                                    e.label()
                                ),
                            );
                            None
                        }
                        Err(why) => {
                            e.life.built(false);
                            e.build = 0;
                            e.retried = false;
                            logging::line("gui", &format!("{}: could not be built ({ms} ms): {why}", e.label()));
                            e.failed_why = Some(why);
                            None
                        }
                    }
                }
                _ => result.ok(),
            }
        };
        if let Some(n) = leftover {
            drop(n);
            logging::line("gui", &format!("page {id}: closed while it was being built; its window was taken down again"));
        }
    }
    // The pages still waiting for the environment, for the tick after WebView2 has answered (on a
    // Mac, WebKit with the rule list) — ahead of any asked for since (a build pumps messages, but
    // no module runs meanwhile).
    #[cfg(any(windows, target_os = "macos"))]
    {
        waiting.extend(again);
        if waiting.is_empty() {
            native_page::drop_unclaimed_failure();
        } else {
            let mut builds = sh.pages.builds.borrow_mut();
            waiting.retain(|id| !builds.contains(id));
            builds.splice(0..0, waiting);
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = (ids, Phase::Idle);
    log_memory_due(sh);
}

/// The memory line of each page due for one, at the end of the tick: WebView2's processes on
/// Windows; on a Mac none, said once a session (`native_page::Native::memory`).
fn log_memory_due(sh: &Shared) {
    let due = std::mem::take(&mut *sh.pages.memory_due.borrow_mut());
    #[cfg(any(windows, target_os = "macos"))]
    for id in due {
        let Some((native, label)) = sh.pages.entries.borrow().get(&id).and_then(|e| Some((native_of(e)?, e.label())))
        else {
            continue;
        };
        if let Some(m) = native.memory() {
            logging::line("gui", &format!("{label}: {m}"));
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = due;
}

/// Hands the modules what their pages' windows sent since the last tick, in order ([`hand_on`]).
pub(crate) fn fire_events(sh: &Shared) {
    #[cfg(any(windows, target_os = "macos"))]
    let events = native_page::take_events();
    #[cfg(not(any(windows, target_os = "macos")))]
    let events: Vec<PageEvent> = Vec::new();
    hand_on(sh, events);
}

/// Each page event, in the order the windows sent them: the host's part at once — the page's
/// life, its window, the messages that waited for it, the log — whether or not its module can
/// hear it now; then its callback, if the page has one, as an event of the module that owns the
/// page, through that module's mailbox (`mailbox::deliver`): it runs now as a handler if the
/// module is free, waits behind the module's earlier events if it is busy, and is dropped while
/// the module is disabled, or for a VM rebuilt since — as every event of a module is.
///
/// **The lane** it runs in, as for every event (ocr/types.rs, `enter_priority`): a message and a
/// close by the user in the interactive one, as a key's — whoever sent the message: the host
/// cannot tell a person's click in the page from the page's own script; `onReady` in the lane of
/// the `show` or `prepare` that started the build, or the interactive one if one made while it was
/// under way was interactive ([`note_asked`]), as an answer runs in the lane of the call that
/// asked for it (a page that reloads itself: the background one); a close by a failure in the
/// background one — nobody asked.
pub(crate) fn hand_on<H: PageHost>(h: &H, events: Vec<PageEvent>) {
    for ev in events {
        match ev {
            PageEvent::Loaded { page, build } => loaded(h, page, build),
            PageEvent::LoadFailed { page, build, why } => load_failed(h, page, build, &why),
            PageEvent::Message { page, build, text } => message(h, page, build, text),
            PageEvent::Closed { page, build, why, detail } => closed(h, page, build, why, detail),
        }
    }
}

/// Delivers `call`, of page `page`'s build `build`, to module `owner`'s VM `lua`, in `lane`.
fn deliver<H: PageHost>(h: &H, owner: usize, lua: &Lua, page: u64, build: u64, call: PageCall, lane: Priority) {
    let _prio = enter_priority(lane);
    mailbox::deliver(h, owner, lua, Event::Page { page, build, call });
}

/// The callback `key` of the page's VM, with what calling it needs.
fn callback(e: &Entry, key: Option<&RegistryKey>) -> Option<Function> {
    key.and_then(|k| e.lua.registry_value::<Function>(k).ok())
}

/// A page event as it runs (`Event::Page`), for module `idx`'s VM `lua`: the page's callback,
/// looked up now, with its value — a message decoded now, in that VM; one that is not JSON, or
/// that the VM has no memory left to hold, is dropped with a line in the log that says which
/// ([`undecoded`]). Nothing to run when the page is gone — its module reloaded —
/// or the event is of a build the module closed since (`Entry::closed_upto`): a page the module
/// closed, or closed and built again, does not hear what the old one sent. A build a failure ended
/// is heard to the end — what it sent before the failure, and its `onClose("failed")`, run in
/// order, as they would have had the module not been busy, however many builds failed meanwhile.
pub(crate) fn open_event(pages: &Pages, idx: usize, lua: &Lua, page: u64, build: u64, call: PageCall) -> Opened {
    let (f, label) = {
        let entries = pages.entries.borrow();
        let Some(e) = entries.get(&page) else { return Opened::Gone };
        if build == 0 || build <= e.closed_upto {
            return Opened::Gone;
        }
        let key = match &call {
            PageCall::Message(_) => Some(&e.on_message),
            PageCall::Ready => e.on_ready.as_ref(),
            PageCall::Closed(..) => e.on_close.as_ref(),
        };
        (callback(e, key), e.label())
    };
    let Some(f) = f else { return Opened::Gone };
    let what = call.what();
    let args = match call {
        PageCall::Message(text) => match lua.create_string(&text).and_then(|s| json::decode(lua, s)) {
            Ok(v) => MultiValue::from_vec(vec![v]),
            Err(e) => {
                logging::line("gui", &undecoded(&label, &e));
                return Opened::Gone;
            }
        },
        PageCall::Ready => MultiValue::new(),
        // `onClose(how, err)`: beside `"failed"` the failure's error value, which says which it was
        // in its `detail`.
        PageCall::Closed(how, detail) => {
            let err = (how == "failed").then(|| match detail {
                Some(d) => HostError::of(&gui::FAILED).with_detail(d),
                None => HostError::of(&gui::FAILED),
            });
            match lua.create_string(how).and_then(|s| errors_lua::answer(lua, Value::String(s), err.as_ref())) {
                Ok(args) => args,
                Err(_) => return Opened::Gone,
            }
        }
    };
    Opened::Run { f, args, ctx: Ctx::new(what, idx) }
}

/// The log line for a message of page `label` that [`open_event`] could not decode. Most often it is
/// not JSON; but the decoding builds up to 1 MiB of JSON (`webview::MAX_MESSAGE_BYTES`) as tables
/// in the module's VM before its handler's entry begins, so a VM kept full meets its memory limit
/// here, and that is said as what it is. Neither is the module's error, and neither is a stop: the
/// guard stops the module at its next callback that meets the limit (TODO.md, G1 open point).
fn undecoded(label: &str, e: &mlua::Error) -> String {
    if crate::vm_guard::is_memory_error(e) {
        format!("{label}: a message was dropped: its module's VM had no memory left to decode it ({e})")
    } else {
        format!("{label}: a message that is not JSON was dropped: {e}")
    }
}

fn loaded<H: PageHost>(h: &H, page: u64, build: u64) {
    let (flush, native, ready, label, asked) = {
        let mut entries = h.pages().entries.borrow_mut();
        let Some(e) = entries.get_mut(&page) else { return };
        if e.build != build {
            return;
        }
        let Some(flush) = e.life.loaded() else { return };
        let ready = e.on_ready.is_some().then(|| (e.owner, e.lua.clone()));
        (flush, native_of(e), ready, e.label(), e.asked.take())
    };
    #[cfg(any(windows, target_os = "macos"))]
    if let Some(native) = &native {
        for json in &flush {
            if let Err(e) = native.post_json(json) {
                logging::line("gui", &format!("{label}: a message could not be handed to the page: {e}"));
            }
        }
    }
    // Let go of the window before the module's code runs: an `onReady` that closes the page
    // must be the last to hold it (see `take_down`).
    take_down(native);
    logging::line(
        "gui",
        &format!(
            "{label}: ready{}; {} message(s) that waited were handed to it",
            asked
                .map(|(at, _)| format!(", {} ms after the show or prepare that asked for it", at.elapsed().as_millis()))
                .unwrap_or_default(),
            flush.len()
        ),
    );
    h.pages().memory_due.borrow_mut().push(page);
    if let Some((owner, lua)) = ready {
        let lane = asked.map_or(Priority::Background, |(_, lane)| lane);
        deliver(h, owner, &lua, page, build, PageCall::Ready, lane);
    }
}

/// A document of the page failed to load. The first one — the page never became ready — is a
/// build that failed: the window goes, and the next `show` says so and builds again. A later one
/// leaves the ready page as it is. No callback: the module learns it from `show`.
fn load_failed<H: PageHost>(h: &H, page: u64, build: u64, why: &str) {
    let (native, first, label) = {
        let mut entries = h.pages().entries.borrow_mut();
        let Some(e) = entries.get_mut(&page) else { return };
        if e.build != build {
            return;
        }
        let first = e.life.load_failed();
        let native: NativeRef = if first {
            e.build = 0;
            e.window = None;
            e.asked = None;
            e.failed_why = Some(format!("its page did not load ({why})"));
            #[cfg(any(windows, target_os = "macos"))]
            let n = e.native.take();
            #[cfg(not(any(windows, target_os = "macos")))]
            let n: NativeRef = None;
            n
        } else {
            None
        };
        (native, first, e.label())
    };
    take_down(native);
    logging::line(
        "gui",
        &if first {
            format!(
                "{label}: its page did not load ({why}); it is taken down, and the next show \
                 answers \"{}\" and builds it again",
                Refusal::BuildFailed.error()
            )
        } else {
            format!("{label}: a document did not load ({why}); the page stays as it was")
        },
    );
}

fn message<H: PageHost>(h: &H, page: u64, build: u64, text: String) {
    // Its size was checked where it arrived (`native_page`'s handler), before it waited for this tick.
    let (owner, lua) = {
        let entries = h.pages().entries.borrow();
        let Some(e) = entries.get(&page) else { return };
        if e.build != build {
            return;
        }
        (e.owner, e.lua.clone())
    };
    deliver(h, owner, &lua, page, build, PageCall::Message(text), Priority::Interactive);
}

fn closed<H: PageHost>(h: &H, page: u64, build: u64, why: &'static str, detail: Option<String>) {
    let (native, close, label) = {
        let mut entries = h.pages().entries.borrow_mut();
        let Some(e) = entries.get_mut(&page) else { return };
        if e.build != build {
            return;
        }
        let native = if why == "failed" {
            e.life.closed();
            e.build = 0;
            e.window = None;
            e.asked = None;
            #[cfg(any(windows, target_os = "macos"))]
            let n = e.native.take();
            #[cfg(not(any(windows, target_os = "macos")))]
            let n: NativeRef = None;
            n
        } else {
            native_of(e)
        };
        (native, e.on_close.is_some().then(|| (e.owner, e.lua.clone())), e.label())
    };
    #[cfg(any(windows, target_os = "macos"))]
    if let Some(n) = native.as_ref().filter(|_| why != "failed") {
        n.after_hidden();
    }
    // Let go of the window before the module's code runs: an `onClose` that closes the page must
    // be the last to hold it (see `take_down`).
    take_down(native);
    logging::line("gui", &format!("{label}: closed by {}", if why == "user" { "the user; it is hidden and keeps its state" } else { "a failure; the next show builds it again" }));
    if let Some((owner, lua)) = close {
        let lane = if why == "user" { Priority::Interactive } else { Priority::Background };
        deliver(h, owner, &lua, page, build, PageCall::Closed(why, detail), lane);
    }
}

/// A disabled module's pages are hidden: its callbacks stop, and a page nobody answers must not
/// stay in front. Unticked in the manager, or stopped by the host's guard, whose second step is the
/// same disable (`Shared::after_toggle`, from `stops::settle`).
pub(crate) fn hide_pages_of(sh: &Shared, owner: usize) {
    let natives: Vec<(NativeRef, String)> = sh
        .pages
        .entries
        .borrow()
        .values()
        .filter(|e| e.owner == owner)
        .map(|e| (native_of(e), e.label()))
        .collect();
    for (native, label) in natives {
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(n) = native {
            if n.is_shown() {
                n.hide();
                logging::line("gui", &format!("{label}: hidden, its module was disabled"));
            }
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        let _ = (native, label);
    }
}

/// Closes and forgets the pages of every module `owned` answers true for: a module reloaded or
/// rolled back takes its pages with its VM.
pub(crate) fn drop_pages(sh: &Shared, owned: impl Fn(usize) -> bool) {
    let gone: Vec<Entry> = {
        let mut entries = sh.pages.entries.borrow_mut();
        let ids: Vec<u64> = entries.iter().filter(|(_, e)| owned(e.owner)).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| entries.remove(&id)).collect()
    };
    for mut e in gone {
        #[cfg(any(windows, target_os = "macos"))]
        let native = e.native.take();
        #[cfg(not(any(windows, target_os = "macos")))]
        let native: NativeRef = None;
        take_down(native);
        logging::line("gui", &format!("{}: closed with its module", e.label()));
        e.life.closed();
    }
}

/// A page of module `owner`'s VM `lua`, described as `host.gui.webview` describes one, with a build
/// built and loading — started by a `show` in `lane` — as `build_pending` leaves it; its id and
/// its build. For the tests that hand its events on through [`hand_on`] and the real mailbox.
#[cfg(test)]
pub(crate) fn test_page(
    pages: &Pages,
    lua: &Lua,
    owner: usize,
    callbacks: (Function, Option<Function>, Option<Function>),
    lane: Priority,
) -> (u64, u64) {
    let (on_message, on_ready, on_close) = callbacks;
    let id = pages.next.get() + 1;
    pages.next.set(id);
    let key = |f: Function| lua.create_registry_value(f).expect("a registry slot");
    let mut entry = Entry {
        owner,
        module: format!("m{owner}"),
        title: "Test page".to_string(),
        kind: WindowKind::Tool,
        activates: true,
        root: PathBuf::new(),
        scheme: "mod-test".to_string(),
        rel: "ui/index.html".to_string(),
        lua: lua.clone(),
        on_message: key(on_message),
        on_ready: on_ready.map(key),
        on_close: on_close.map(key),
        life: Lifecycle::default(),
        build: 0,
        closed_upto: 0,
        window: None,
        asked: None,
        env_said: false,
        over_said: false,
        failed_why: None,
        #[cfg(any(windows, target_os = "macos"))]
        shown_this_build: false,
        #[cfg(any(windows, target_os = "macos"))]
        retried: false,
        #[cfg(any(windows, target_os = "macos"))]
        native: None,
    };
    test_build(pages, &mut entry, lane);
    let build = entry.build;
    pages.entries.borrow_mut().insert(id, entry);
    (id, build)
}

/// Page `id` built again — the next `show` after it was closed or failed, in `lane` — as
/// `build_pending` builds it; the new build.
#[cfg(test)]
pub(crate) fn test_rebuild(pages: &Pages, id: u64, lane: Priority) -> u64 {
    let mut entries = pages.entries.borrow_mut();
    let e = entries.get_mut(&id).expect("the page");
    test_build(pages, e, lane);
    e.build
}

/// A `show` or `prepare` of page `id`, in the lane current now, as both decide it and note it on
/// the page ([`note_asked`]); what was decided.
#[cfg(test)]
pub(crate) fn test_show(pages: &Pages, id: u64) -> ShowStep {
    let mut entries = pages.entries.borrow_mut();
    let e = entries.get_mut(&id).expect("the page");
    let step = e.life.on_show(Where { headless: false, supported: true }, || true);
    note_asked(e, step);
    step
}

#[cfg(test)]
fn test_build(pages: &Pages, e: &mut Entry, lane: Priority) {
    let _lane = enter_priority(lane);
    let step = e.life.on_show(Where { headless: false, supported: true }, || true);
    assert_eq!(step, ShowStep::Build);
    note_asked(e, step);
    e.life.built(true);
    let build = pages.next_build.get() + 1;
    pages.next_build.set(build);
    e.build = build;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(lua: &Lua, src: &str) -> Value {
        lua.load(src).eval().unwrap()
    }

    #[test]
    fn a_page_is_described_by_its_seven_options_and_nothing_else() {
        let lua = Lua::new();
        let ok = page_options(opts(&lua, "return { file = 'ui/a.html', title = 'A', onMessage = function() end }")).unwrap();
        assert_eq!((ok.file.as_str(), ok.title.as_str()), ("ui/a.html", "A"));
        assert!(ok.on_ready.is_none() && ok.on_close.is_none());
        assert_eq!(ok.kind, WindowKind::Tool, "a tool window unless it says otherwise");
        assert!(ok.activates, "one that activates the application unless it says otherwise");
        let all = page_options(opts(
            &lua,
            "return { file = 'a', title = 'A', onMessage = print, onReady = print, onClose = print }",
        ))
        .unwrap();
        assert!(all.on_ready.is_some() && all.on_close.is_some());
        let err = |src: &str| page_options(opts(&lua, src)).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err("return 'ui/a.html'").contains("takes a table of options, not a string"));
        assert!(err("return { title = 'A', onMessage = print }").contains("file is a string, not a nil"));
        assert!(err("return { file = '', title = 'A', onMessage = print }").contains("file is empty"));
        assert!(err("return { file = 'a', onMessage = print }").contains("title is a string, not a nil"));
        assert!(err("return { file = 'a', title = 'A' }").contains("onMessage is a function, not a nil"));
        assert!(err("return { file = 'a', title = 'A', onMessage = print, onReady = 1 }").contains("onReady is a function, not a number"));
        let e = err("return { file = 'a', title = 'A', onMessage = print, url = 'https://x' }");
        assert!(
            e.contains("'url' is not an option; the options are file, title, window, activates, onMessage, onReady and onClose"),
            "{e}"
        );
        assert!(err("return { 'a' }").contains("a number key is not an option"));
    }

    /// `window` names the kind of window, `"tool"` or `"app"`, and nothing else: another word, the
    /// right word in another case, or a value of another type raises, naming both words.
    #[test]
    fn the_window_option_is_tool_or_app() {
        let lua = Lua::new();
        let kind = |w: &str| {
            page_options(opts(&lua, &format!("return {{ file = 'a', title = 'A', onMessage = print, window = {w} }}")))
                .map(|o| o.kind)
                .map_err(|e| e.to_string())
        };
        assert_eq!(kind("'tool'"), Ok(WindowKind::Tool));
        assert_eq!(kind("'app'"), Ok(WindowKind::App));
        assert_eq!(kind("nil"), Ok(WindowKind::Tool));
        let e = kind("'App'").unwrap_err();
        assert!(e.contains(r#"host.gui.webview: window is "tool" or "app", not "App""#), "{e}");
        let e = kind("'application'").unwrap_err();
        assert!(e.contains(r#"window is "tool" or "app", not "application""#), "{e}");
        let e = kind("true").unwrap_err();
        assert!(e.contains(r#"window is "tool" or "app", not a boolean"#), "{e}");
        let e = kind("{ kind = 'app' }").unwrap_err();
        assert!(e.contains(r#"window is "tool" or "app", not a table"#), "{e}");
    }

    /// `activates` is a boolean, `true` when it is absent, and `false` only for a tool page: an app
    /// page described so raises, whichever of the two options is written first, and so does a
    /// value of another type.
    #[test]
    fn activates_is_true_or_false_and_false_only_for_a_tool_page() {
        let lua = Lua::new();
        let read = |extra: &str| {
            page_options(opts(&lua, &format!("return {{ file = 'a', title = 'A', onMessage = print, {extra} }}")))
                .map(|o| (o.kind, o.activates))
                .map_err(|e| e.to_string())
        };
        assert_eq!(read("activates = false"), Ok((WindowKind::Tool, false)));
        assert_eq!(read("window = 'tool', activates = false"), Ok((WindowKind::Tool, false)));
        assert_eq!(read("activates = true"), Ok((WindowKind::Tool, true)));
        assert_eq!(read("window = 'app', activates = true"), Ok((WindowKind::App, true)));
        assert_eq!(read("window = 'app'"), Ok((WindowKind::App, true)));
        for app in ["window = 'app', activates = false", "activates = false, window = 'app'"] {
            let e = read(app).unwrap_err();
            assert!(e.contains("host.gui.webview: activates = false is for a tool page"), "{e}");
            assert!(e.contains(r#"an app page (window = "app")"#), "{e}");
        }
        let e = read("activates = 'no'").unwrap_err();
        assert!(e.contains("host.gui.webview: activates is true or false, not a string"), "{e}");
        let e = read("activates = 0").unwrap_err();
        assert!(e.contains("activates is true or false, not a number"), "{e}");
    }

    #[test]
    fn show_takes_focus_by_default_and_avoid_as_bounds() {
        let lua = Lua::new();
        assert_eq!(show_options(Value::Nil).unwrap(), (true, None));
        assert_eq!(show_options(opts(&lua, "return {}")).unwrap(), (true, None));
        assert_eq!(show_options(opts(&lua, "return { focus = false }")).unwrap(), (false, None));
        let (f, a) = show_options(opts(&lua, "return { avoid = { x = 10, y = 20.7, w = 300, h = 400 } }")).unwrap();
        assert!(f);
        assert_eq!(a, Some(Rect { x: 10, y: 20, w: 300, h: 400 }));
    }

    #[test]
    fn show_refuses_its_mistakes_by_name() {
        let lua = Lua::new();
        let err = |src: &str| show_options(opts(&lua, src)).unwrap_err().to_string();
        assert!(err("return { focus = 1 }").contains("focus is true or false"));
        assert!(err("return { place = 'left' }").contains("the options are focus and avoid"));
        assert!(err("return { avoid = { x = 1, y = 2, w = 3 } }").contains("avoid.h is a number, not a nil"));
        assert!(err("return { avoid = { x = 1, y = 2, w = 0, h = 3 } }").contains("no area"));
        assert!(err("return { avoid = 'plugin' }").contains("avoid is a table"));
        assert!(err("return 5").contains("not a number"));
    }

    /// `post` encodes with `host.json.encode`'s walker, and what it hands the page is data: JSON
    /// text, which `PostWebMessageAsJson` parses on the other side. Strings that would end or
    /// break a script — a closing script tag, the two line separators JavaScript source once
    /// refused, quotes, a backslash — are just characters in it, and come back as they went.
    #[test]
    fn a_posted_value_is_json_that_round_trips() {
        let lua = Lua::new();
        let value: Value = lua
            .load(r#"return { name = "</script><script>alert(1)</script>", lines = "a\u{2028}b\u{2029}c", q = "\"'\\", n = 1.5, list = { 1, 2, 3 } }"#)
            .eval()
            .unwrap();
        let text = json::encode(&lua, (value, Value::Nil)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["name"], "</script><script>alert(1)</script>");
        assert_eq!(parsed["lines"], "a\u{2028}b\u{2029}c");
        assert_eq!(parsed["q"], "\"'\\");
        assert_eq!(parsed["list"], serde_json::json!([1, 2, 3]));
        // And what a page sends back decodes to the same value in Luau.
        let back = json::decode(&lua, lua.create_string(&text).unwrap()).unwrap();
        let Value::Table(t) = back else { panic!("a table") };
        assert_eq!(t.get::<String>("name").unwrap(), "</script><script>alert(1)</script>");
    }

    /// A message its module's VM has no memory left to decode is said as that, not as a message
    /// that is not JSON — the decoding, not the text, failed.
    #[test]
    fn a_message_its_vm_cannot_hold_is_not_called_bad_json() {
        let lua = Lua::new();
        let text = lua.create_string(format!("[{}]", vec!["1"; 50_000].join(","))).unwrap();
        lua.set_memory_limit(lua.used_memory() + 16 * 1024).unwrap();
        let e = json::decode(&lua, text).unwrap_err();
        let line = undecoded("[m1] page 'Test page'", &e);
        assert!(line.contains("its module's VM had no memory left to decode it"), "{line}");
        assert!(!line.contains("not JSON"), "{line}");
        let fresh = Lua::new();
        let e = json::decode(&fresh, fresh.create_string("{not json").unwrap()).unwrap_err();
        let line = undecoded("[m1] page 'Test page'", &e);
        assert!(line.contains("a message that is not JSON was dropped"), "{line}");
    }

    /// Every call into a page's window here goes through `native_page`, the one name both
    /// platforms' windows go by: Windows' window is named by its alias alone, and nothing here is
    /// built for one of the two only but the alias and the question whether the engine is there
    /// (`runtime_present`). So a Mac's page takes every path a Windows page takes, and a path
    /// made for Windows alone shows here, not first on a Mac.
    #[test]
    fn both_platforms_reach_their_page_window_by_one_name() {
        let src = include_str!("webview_lua.rs");
        let code = &src[..src.find("#[cfg(test)]\nmod tests").expect("the tests")];
        assert_eq!(code.matches("crate::webview_win").count(), 1, "only the alias names Windows' window");
        assert!(code.contains("#[cfg(windows)]\nuse crate::webview_win as native_page;"));
        assert!(code.contains("#[cfg(target_os = \"macos\")]\nuse crate::backend::page as native_page;"));
        let runtime = crate::ocr_wiring_tests::body(code, "fn runtime_present() -> bool {");
        let windows_only = |s: &str| s.matches("#[cfg(windows)]").count() + s.matches("#[cfg(not(windows))]").count();
        assert_eq!(windows_only(runtime), 1, "the WebView2 runtime is asked on Windows");
        assert_eq!(windows_only(code), 2, "a path for Windows alone: the alias and the runtime are the only two");
        assert_eq!(code.matches("#[cfg(target_os = \"macos\")]").count(), 2, "the alias and WebKit's version");
    }

    /// A build that failed is answered `could not be built`, once, with why it failed as the error
    /// value's `detail`; the next `show` builds again, and no other refusal takes the detail.
    #[test]
    fn a_failed_build_says_why_once_in_its_detail() {
        use crate::errors::{Code, Reason};
        let (lua, pages) = (Lua::new(), Pages::default());
        let f: Function = lua.load("return function() end").eval().unwrap();
        let (id, _) = test_page(&pages, &lua, 1, (f, None, None), Priority::Background);
        let mut entries = pages.entries.borrow_mut();
        let e = entries.get_mut(&id).unwrap();
        e.failed_why = Some("its page did not load (WebView2 error status 2 at http://x)".into());
        // Loading still: not ready, and the detail stays for the build's own refusal.
        let step = e.life.on_show(Where { headless: false, supported: true }, || true);
        let not_yet = refusal(e, step).unwrap();
        assert_eq!((not_yet.reason(), not_yet.detail()), (&gui::NOT_READY as &Reason, None));
        assert!(e.life.load_failed());
        let step = e.life.on_show(Where { headless: false, supported: true }, || true);
        let failed = refusal(e, step).unwrap();
        assert_eq!((failed.code(), failed.message()), (Code::Failed, "could not be built"));
        assert_eq!(failed.detail(), Some("its page did not load (WebView2 error status 2 at http://x)"));
        assert!(e.failed_why.is_none(), "said once");
        let step = e.life.on_show(Where { headless: false, supported: true }, || true);
        assert_eq!(refusal(e, step), None, "built again");
    }

    /// `focus`, `post` and a page gone with its module answer in the codes of the plan: declined
    /// for a shown page the system would not bring forward, hidden for a hidden one; headless or no
    /// page window for a `focus` or a `post` where no page can ever be shown; and gone — no longer
    /// a raise.
    #[test]
    fn focus_post_and_a_gone_page_answer_with_their_codes() {
        use crate::errors::{common, Code};
        assert_eq!(focus_answer(true, false), Ok(()));
        assert_eq!(focus_answer(false, true), Err(HostError::of(&common::DECLINED)));
        assert_eq!(focus_answer(false, false), Err(HostError::of(&gui::HIDDEN)));
        assert_eq!(focus_answer(false, false).unwrap_err().code(), Code::Hidden);
        assert_eq!(nowhere(Where { headless: true, supported: true }), HostError::of(&gui::HEADLESS));
        assert_eq!(nowhere(Where { headless: false, supported: false }), HostError::of(&common::NOT_YET));
        let gone = gone(7, "show");
        assert_eq!((gone.code(), gone.message()), (Code::Gone, "this page no longer exists: its module was reloaded"));
    }

    /// Where the calls are made: each of `show`, `prepare`, `focus` and `post` answers a page gone
    /// with its module with `false` and `gone`'s error value, not a raise; and `focus` refuses where
    /// no page can ever be shown as `post` does, before it looks for a window — a headless run
    /// builds none, and is not a page "not built yet".
    #[test]
    fn every_call_answers_a_gone_page_and_focus_refuses_where_none_is_shown() {
        let src = include_str!("webview_lua.rs");
        let code = &src[..src.find("#[cfg(test)]\nmod tests").expect("the tests")];
        let body = |sig: &str| crate::ocr_wiring_tests::body(code, sig);
        assert!(body("fn show(lua: &Lua,").contains("return errors_lua::with_error(lua, Value::Boolean(false), &gone(id, \"show\"));"));
        assert!(body("fn prepare(lua: &Lua,").contains("return errors_lua::with_error(lua, Value::Boolean(false), &gone(id, \"prepare\"));"));
        let focus = body("fn focus(sh: &Rc<Shared>,");
        assert!(focus.contains("None => return Err(gone(id, \"focus\")),"));
        assert!(body("fn post(lua: &Lua,").contains("let Some(e) = entries.get_mut(&id) else { return Ok(Err(gone(id, \"post\"))) };"));
        // The two whose body answers a `Result` are answered `true` or `false, err` by the method.
        let methods = body("fn add_methods<");
        assert!(methods.contains("errors_lua::done(lua, &focus(&p.sh, p.id))"));
        assert!(methods.contains("let posted = post(lua, &p.sh, p.id, value)?;\n            errors_lua::done(lua, &posted)"));
        let nowhere = focus.find("if !at.can_show() {").expect("focus asks whether a page can be shown");
        let native = focus.find("let Some(native) = native else").expect("focus looks for the window");
        assert!(nowhere < native, "refused where none is shown before \"not built\"");
        assert!(focus[nowhere..native].contains("let e = nowhere(at);") && focus[nowhere..native].contains("return Err(e);"));
    }

    /// `show`'s partial answer — in front, over the rectangle to avoid — is `true` and the error
    /// value, which reads as the note did: its message is the old words, so a module that put them
    /// in a sentence says what it said.
    #[test]
    fn a_show_over_the_rectangle_answers_true_and_its_error() {
        let lua = Lua::new();
        let mv = errors_lua::answer(&lua, Value::Boolean(true), Some(&webview::over())).unwrap();
        lua.globals().set("ok", mv[0].clone()).unwrap();
        lua.globals().set("err", mv[1].clone()).unwrap();
        let said: String = lua.load("return tostring(ok) .. ' ' .. err.code .. ': page over ' .. err").eval().unwrap();
        assert_eq!(said, "true too_large: page over over the rectangle to avoid");
        let no = errors_lua::answer(&lua, Value::Boolean(false), Some(&Refusal::NotReady.error())).unwrap();
        lua.globals().set("err", no[1].clone()).unwrap();
        assert!(lua.load("return err.code == 'not_ready' and tostring(err) == 'not ready yet'").eval::<bool>().unwrap());
        assert_eq!(errors_lua::answer(&lua, Value::Boolean(true), None).unwrap().len(), 1, "a full success is true alone");
    }

    #[test]
    fn a_value_json_cannot_hold_is_refused_as_posts_own() {
        let lua = Lua::new();
        let f: Value = lua.load("return function() end").eval().unwrap();
        let why = json::encode(&lua, (f, Value::Nil)).unwrap_err().to_string();
        assert!(why.contains("host.json.encode"), "{why}");
        assert!(why.replacen("host.json.encode", "page:post", 1).contains("page:post"));
    }
}
