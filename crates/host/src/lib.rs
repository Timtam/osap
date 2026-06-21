//! Host runtime: embeds a Luau VM and exposes the `host` API (design principle:
//! primitives are first-class, see `docs/host-api-capability-catalog.md`).
//!
//! The OS is reached only through [`backend::Backend`] (one impl per platform).
//! Walking-skeleton scope: `host.log`, `host.speech` (tts-rs), `host.hotkey`,
//! `host.window` + `host.os`, `host.path`, `host.resource.read`.

mod backend;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mlua::{Function, Lua, RegistryKey, Table};
use tts::Tts;

use backend::{Backend, HostEvents, WinInfo};
use module_manifest::LoadedModule;

/// Luau prelude that adds the OS-gated `host.window.find`/`findAll` matcher on
/// top of the native `host.window.list`/`active` bindings.
const WINDOW_PRELUDE: &str = include_str!("window_prelude.luau");

/// Shared host state that the `host` API closures access via `Rc<RefCell<…>>`.
struct HostState {
    root: PathBuf,
    tts: Tts,
    /// Registered global hotkeys: backend id → Luau callback (kept in the Lua registry).
    hotkeys: Vec<(i32, RegistryKey)>,
    hotkey_counter: i32,
}

impl HostState {
    fn resolve(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

/// Bridges OS events from the backend into the modules' Luau callbacks.
struct Dispatcher<'a> {
    lua: &'a Lua,
    state: Rc<RefCell<HostState>>,
}

impl HostEvents for Dispatcher<'_> {
    fn on_hotkey(&mut self, id: i32) {
        let func: Option<Function> = {
            let st = self.state.borrow();
            st.hotkeys
                .iter()
                .find(|(hid, _)| *hid == id)
                .and_then(|(_, key)| self.lua.registry_value::<Function>(key).ok())
        };
        if let Some(f) = func {
            if let Err(e) = f.call::<()>(()) {
                eprintln!("  [hotkey] callback error: {e}");
            }
        }
    }
}

/// Loads a module from an unpacked directory, installs the `host` API and runs
/// the entry point. If the module registered hotkeys, enters the always-on event
/// loop; otherwise waits for any pending speech and exits.
pub fn run_module(dir: impl AsRef<Path>) -> Result<()> {
    let module = LoadedModule::load_dir(dir)?;
    println!(
        "» Loading module: {} v{} (id {})",
        module.manifest.name, module.manifest.version, module.manifest.id
    );
    if !module.manifest.capabilities.require.is_empty() {
        println!(
            "  Requested capabilities: {}",
            module.manifest.capabilities.require.join(", ")
        );
    }

    let backend = backend::platform();

    let tts = Tts::default().context("failed to initialize TTS engine")?;
    let state = Rc::new(RefCell::new(HostState {
        root: module.root.clone(),
        tts,
        hotkeys: Vec::new(),
        hotkey_counter: 0,
    }));

    let lua = Lua::new();
    install_host_api(&lua, &state, &backend).context("failed to install host API")?;
    lua.load(WINDOW_PRELUDE)
        .set_name("window_prelude")
        .exec()
        .context("failed to load host.window matcher prelude")?;

    let entry = module.entry_path();
    let code = std::fs::read_to_string(&entry)
        .with_context(|| format!("entry point not readable: {}", entry.display()))?;
    lua.load(code)
        .set_name(entry.display().to_string())
        .exec()
        .context("error while running the module entry point")?;

    if state.borrow().hotkeys.is_empty() {
        wait_for_speech(&state);
    } else {
        println!("» Listening for hotkeys — press Ctrl+C to quit.");
        let mut dispatcher = Dispatcher {
            lua: &lua,
            state: state.clone(),
        };
        backend
            .run_event_loop(&mut dispatcher)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("event loop failed")?;
    }
    Ok(())
}

fn install_host_api(lua: &Lua, state: &Rc<RefCell<HostState>>, backend: &Rc<dyn Backend>) -> Result<()> {
    let host = lua.create_table()?;

    // host.log.info(msg)
    let log = lua.create_table()?;
    log.set(
        "info",
        lua.create_function(|_, msg: String| {
            println!("  [module] {msg}");
            Ok(())
        })?,
    )?;
    host.set("log", log)?;

    // host.speech.output(text, { interrupt = true })
    let speech = lua.create_table()?;
    let s = state.clone();
    speech.set(
        "output",
        lua.create_function(move |_, (text, opts): (String, Option<Table>)| {
            let interrupt = match opts {
                Some(t) => t.get::<bool>("interrupt").unwrap_or(true),
                None => true,
            };
            println!("  [speech] {text}");
            s.borrow_mut()
                .tts
                .speak(text, interrupt)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    host.set("speech", speech)?;

    // host.hotkey.register(spec, callback)
    let hk = lua.create_table()?;
    let s_hk = state.clone();
    let b_hk = backend.clone();
    hk.set(
        "register",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            let id = {
                let mut st = s_hk.borrow_mut();
                st.hotkey_counter += 1;
                st.hotkey_counter
            };
            b_hk.register_hotkey(id, &spec).map_err(mlua::Error::external)?;
            let key = lua.create_registry_value(cb)?;
            s_hk.borrow_mut().hotkeys.push((id, key));
            println!("  [hotkey] registered '{spec}' (id {id})");
            Ok(())
        })?,
    )?;
    host.set("hotkey", hk)?;

    // host.os.current / host.os.is(name)
    let os = lua.create_table()?;
    os.set("current", std::env::consts::OS)?;
    os.set(
        "is",
        lua.create_function(|_, name: String| Ok(name == std::env::consts::OS))?,
    )?;
    host.set("os", os)?;

    // host.window.list() / host.window.active()  (find/findAll added by the prelude)
    let win = lua.create_table()?;
    let b_list = backend.clone();
    win.set(
        "list",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for w in b_list.enumerate_windows() {
                t.push(win_to_table(lua, &w)?)?;
            }
            Ok(t)
        })?,
    )?;
    let b_active = backend.clone();
    win.set(
        "active",
        lua.create_function(move |lua, ()| match b_active.active_window() {
            Some(w) => Ok(Some(win_to_table(lua, &w)?)),
            None => Ok(None),
        })?,
    )?;
    host.set("window", win)?;

    // host.path(rel) -> real path (escape hatch)
    let s2 = state.clone();
    host.set(
        "path",
        lua.create_function(move |_, rel: String| {
            Ok(s2.borrow().resolve(&rel).to_string_lossy().to_string())
        })?,
    )?;

    // host.resource.read(rel) -> bytes/string from the package
    let resource = lua.create_table()?;
    let s3 = state.clone();
    resource.set(
        "read",
        lua.create_function(move |_, rel: String| {
            let path = s3.borrow().resolve(&rel);
            std::fs::read_to_string(&path).map_err(mlua::Error::external)
        })?,
    )?;
    host.set("resource", resource)?;

    lua.globals().set("host", host)?;
    Ok(())
}

/// Converts a native window snapshot into the Lua table modules see:
/// `{ id, title, class, app = { name, exe, pid }, bounds = { x, y, w, h } }`.
fn win_to_table(lua: &Lua, w: &WinInfo) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", w.hwnd)?;
    t.set("title", w.title.clone())?;
    t.set("class", w.class.clone())?;

    let app = lua.create_table()?;
    let name = w
        .exe
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| w.exe.clone());
    app.set("name", name)?;
    app.set("exe", w.exe.clone())?;
    app.set("pid", w.pid)?;
    t.set("app", app)?;

    let b = lua.create_table()?;
    b.set("x", w.x)?;
    b.set("y", w.y)?;
    b.set("w", w.w)?;
    b.set("h", w.h)?;
    t.set("bounds", b)?;

    Ok(t)
}

/// Keeps the process alive until speech output has finished (CLI skeleton: tts-rs
/// speaks asynchronously). Bounded by a timeout.
fn wait_for_speech(state: &Rc<RefCell<HostState>>) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let speaking = state.borrow().tts.is_speaking().unwrap_or(false);
        if !speaking || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
