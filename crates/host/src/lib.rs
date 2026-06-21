//! Host runtime: embeds a Luau VM and exposes the `host` API (design principle:
//! primitives are first-class, see `docs/host-api-capability-catalog.md`).
//!
//! Walking-skeleton scope: `host.log`, `host.speech` (tts-rs), `host.path`,
//! `host.resource.read`, `host.hotkey` (global hotkeys → Luau callbacks).
//! Further capabilities (window/input/screen/ocr/overlay) to follow.

mod hotkey;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mlua::{Function, Lua, RegistryKey, Table};
use tts::Tts;

use module_manifest::LoadedModule;

/// Shared host state that the `host` API closures access via `Rc<RefCell<…>>`.
struct HostState {
    root: PathBuf,
    tts: Tts,
    /// Registered global hotkeys: Win32 id → Luau callback (kept in the Lua registry).
    hotkeys: Vec<(i32, RegistryKey)>,
    hotkey_counter: i32,
}

impl HostState {
    fn resolve(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

/// Loads a module from an unpacked directory, installs the `host` API and
/// runs the entry point. If the module registered hotkeys, enters the always-on
/// event loop; otherwise just waits for any pending speech and exits.
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

    let tts = Tts::default().context("failed to initialize TTS engine")?;
    let state = Rc::new(RefCell::new(HostState {
        root: module.root.clone(),
        tts,
        hotkeys: Vec::new(),
        hotkey_counter: 0,
    }));

    let lua = Lua::new();
    install_host_api(&lua, &state).context("failed to install host API")?;

    let entry = module.entry_path();
    let code = std::fs::read_to_string(&entry)
        .with_context(|| format!("entry point not readable: {}", entry.display()))?;
    lua.load(code)
        .set_name(entry.display().to_string())
        .exec()
        .context("error while running the module entry point")?;

    let has_hotkeys = !state.borrow().hotkeys.is_empty();
    if has_hotkeys {
        hotkey::run_loop(&lua, &state).context("hotkey event loop failed")?;
    } else {
        wait_for_speech(&state);
    }
    Ok(())
}

fn install_host_api(lua: &Lua, state: &Rc<RefCell<HostState>>) -> Result<()> {
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
    hk.set(
        "register",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            hotkey::register(lua, &s_hk, &spec, cb)
        })?,
    )?;
    host.set("hotkey", hk)?;

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
