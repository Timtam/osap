//! Host runtime: a module **manager** that hosts many Luau modules concurrently
//! in one process (one VM each, shared services + one event loop), per
//! `docs/module-runtime-and-lifecycle.md`. The OS is reached only through
//! [`backend::Backend`]. The `host` API (design: primitives are first-class —
//! see `docs/host-api-capability-catalog.md`) is installed per module VM and
//! bound to that module's root + the shared services.

mod backend;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mlua::{Function, Lua, RegistryKey, Table};
use tts::Tts;

use backend::{Backend, CapturedImage, HostEvents, MouseButton, WinInfo};
use module_manifest::LoadedModule;

const WINDOW_PRELUDE: &str = include_str!("window_prelude.luau");
const OVERLAY_PRELUDE: &str = include_str!("overlay_prelude.luau");

/// Services shared by every module: the OS backend, one speech engine, one audio
/// output, the global hotkey-id counter, and the central event routing.
struct Shared {
    backend: Rc<dyn Backend>,
    tts: RefCell<Tts>,
    _audio_stream: Option<rodio::OutputStream>,
    audio: Option<rodio::OutputStreamHandle>,
    next_id: Cell<i32>,
    /// module_idx → root directory.
    roots: RefCell<Vec<PathBuf>>,
    /// module_idx → enabled.
    enabled: RefCell<Vec<bool>>,
    /// Global hotkey id → (module_idx, that module's VM, callback in its registry).
    hotkeys: RefCell<HashMap<i32, (usize, Lua, RegistryKey)>>,
    /// Captured keys: (vk, modifier-mask, module_idx, VM, callback).
    keys: RefCell<Vec<(u32, u8, usize, Lua, RegistryKey)>>,
}

impl Shared {
    fn root(&self, idx: usize) -> PathBuf {
        self.roots.borrow()[idx].clone()
    }
    fn alloc_id(&self) -> i32 {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        id
    }
    /// Recomputes the global captured-key set from all modules and updates the hook.
    fn refresh_captured(&self) {
        let mut set: Vec<(u32, u8)> =
            self.keys.borrow().iter().map(|(vk, m, ..)| (*vk, *m)).collect();
        set.sort_unstable();
        set.dedup();
        self.backend.set_captured_keys(&set);
    }
}

/// A loaded module: its id + its own Luau VM.
struct Module {
    id: String,
    lua: Lua,
}

/// Loads and runs many modules concurrently in one process.
pub struct Manager {
    shared: Rc<Shared>,
    modules: Vec<Module>,
}

impl Manager {
    pub fn new() -> Result<Self> {
        let backend = backend::platform();
        let tts = Tts::default().context("failed to initialize TTS engine")?;
        let (audio_stream, audio) = match rodio::OutputStream::try_default() {
            Ok((s, h)) => (Some(s), Some(h)),
            Err(e) => {
                eprintln!("  [sound] no audio output device: {e}");
                (None, None)
            }
        };
        let shared = Rc::new(Shared {
            backend,
            tts: RefCell::new(tts),
            _audio_stream: audio_stream,
            audio,
            next_id: Cell::new(0),
            roots: RefCell::new(Vec::new()),
            enabled: RefCell::new(Vec::new()),
            hotkeys: RefCell::new(HashMap::new()),
            keys: RefCell::new(Vec::new()),
        });
        Ok(Self {
            shared,
            modules: Vec::new(),
        })
    }

    /// Loads a module from an unpacked directory and runs its entry point.
    pub fn load(&mut self, dir: impl AsRef<Path>) -> Result<()> {
        let module = LoadedModule::load_dir(dir)?;
        let idx = self.modules.len();
        println!(
            "» Loading module: {} v{} (id {})",
            module.manifest.name, module.manifest.version, module.manifest.id
        );
        if !module.manifest.capabilities.require.is_empty() {
            println!("  Capabilities: {}", module.manifest.capabilities.require.join(", "));
        }

        self.shared.roots.borrow_mut().push(module.root.clone());
        self.shared.enabled.borrow_mut().push(true);

        let lua = Lua::new();
        install_host_api(&lua, &self.shared, idx).context("failed to install host API")?;
        lua.load(WINDOW_PRELUDE).set_name("window_prelude").exec()?;
        lua.load(OVERLAY_PRELUDE).set_name("overlay_prelude").exec()?;

        let entry = module.entry_path();
        let code = std::fs::read_to_string(&entry)
            .with_context(|| format!("entry point not readable: {}", entry.display()))?;
        lua.load(code)
            .set_name(entry.display().to_string())
            .exec()
            .with_context(|| format!("error running module '{}'", module.manifest.id))?;

        self.modules.push(Module {
            id: module.manifest.id,
            lua,
        });
        Ok(())
    }

    /// Runs the shared event loop if any module registered hotkeys, keys, or
    /// window triggers; otherwise waits for pending speech and returns.
    pub fn run(&mut self) -> Result<()> {
        let has_hotkeys = !self.shared.hotkeys.borrow().is_empty();
        let has_keys = !self.shared.keys.borrow().is_empty();
        let has_triggers = self.modules.iter().any(|m| window_has_triggers(&m.lua));

        if has_hotkeys || has_keys || has_triggers {
            if has_triggers {
                self.shared
                    .backend
                    .watch_foreground()
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .context("failed to watch foreground windows")?;
            }
            println!("» Listening for events — press Ctrl+C to quit.");
            let backend = self.shared.backend.clone();
            let mut dispatcher = Dispatcher {
                shared: &self.shared,
                modules: &self.modules,
            };
            backend
                .run_event_loop(&mut dispatcher)
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("event loop failed")?;
        } else {
            self.wait_for_speech();
        }
        Ok(())
    }

    fn wait_for_speech(&self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let speaking = self.shared.tts.borrow().is_speaking().unwrap_or(false);
            if !speaking || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Convenience entry: load each directory as a module and run them together.
pub fn run(dirs: &[String]) -> Result<()> {
    let mut manager = Manager::new()?;
    for dir in dirs {
        manager.load(dir)?;
    }
    manager.run()
}

/// Bridges OS events from the backend into the owning module's Luau callbacks.
struct Dispatcher<'a> {
    shared: &'a Shared,
    modules: &'a [Module],
}

impl Dispatcher<'_> {
    fn enabled(&self, idx: usize) -> bool {
        self.shared.enabled.borrow().get(idx).copied().unwrap_or(false)
    }
}

impl HostEvents for Dispatcher<'_> {
    fn on_hotkey(&mut self, id: i32) {
        let found = {
            let map = self.shared.hotkeys.borrow();
            map.get(&id).and_then(|(idx, lua, key)| {
                if self.enabled(*idx) {
                    lua.registry_value::<Function>(key).ok().map(|f| (lua.clone(), f))
                } else {
                    None
                }
            })
        };
        if let Some((_lua, f)) = found {
            if let Err(e) = f.call::<()>(()) {
                eprintln!("  [hotkey] callback error: {e}");
            }
        }
    }

    fn on_key(&mut self, vk: u32, mods: u8) {
        let found = {
            let keys = self.shared.keys.borrow();
            keys.iter()
                .find(|(k, m, idx, ..)| *k == vk && *m == mods && self.enabled(*idx))
                .and_then(|(_, _, _, lua, key)| {
                    lua.registry_value::<Function>(key).ok().map(|f| (lua.clone(), f))
                })
        };
        if let Some((lua, f)) = found {
            let table = lua.create_table().ok();
            if let Some(t) = &table {
                let _ = t.set("shift", mods & backend::MASK_SHIFT != 0);
                let _ = t.set("ctrl", mods & backend::MASK_CTRL != 0);
                let _ = t.set("alt", mods & backend::MASK_ALT != 0);
                let _ = t.set("win", mods & backend::MASK_WIN != 0);
            }
            let res = match table {
                Some(t) => f.call::<()>(t),
                None => f.call::<()>(()),
            };
            if let Err(e) = res {
                eprintln!("  [keys] callback error: {e}");
            }
        }
    }

    fn on_window_activate(&mut self, win: WinInfo) {
        for (idx, m) in self.modules.iter().enumerate() {
            if !self.enabled(idx) {
                continue;
            }
            let table = match win_to_table(&m.lua, &win) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let res = (|| -> mlua::Result<()> {
                let host: Table = m.lua.globals().get("host")?;
                let window: Table = host.get("window")?;
                let dispatch: Function = window.get("_dispatchActivate")?;
                dispatch.call::<()>(table)
            })();
            if let Err(e) = res {
                eprintln!("  [trigger] dispatch error ({}): {e}", m.id);
            }
        }
    }
}

fn install_host_api(lua: &Lua, shared: &Rc<Shared>, idx: usize) -> Result<()> {
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

    // host.speech.output(text, { interrupt = true })  (do not echo to console:
    // a screen reader would read the terminal and double the speech)
    let speech = lua.create_table()?;
    let sh = shared.clone();
    speech.set(
        "output",
        lua.create_function(move |_, (text, opts): (String, Option<Table>)| {
            let interrupt = match opts {
                Some(t) => t.get::<bool>("interrupt").unwrap_or(true),
                None => true,
            };
            sh.tts
                .borrow_mut()
                .speak(text, interrupt)
                .map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    host.set("speech", speech)?;

    // host.hotkey.register(spec, cb) -> id ; host.hotkey.unregister(id)
    let hk = lua.create_table()?;
    let sh = shared.clone();
    hk.set(
        "register",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            let id = sh.alloc_id();
            sh.backend.register_hotkey(id, &spec).map_err(mlua::Error::external)?;
            let key = lua.create_registry_value(cb)?;
            sh.hotkeys.borrow_mut().insert(id, (idx, lua.clone(), key));
            Ok(id)
        })?,
    )?;
    let sh = shared.clone();
    hk.set(
        "unregister",
        lua.create_function(move |_, id: i32| {
            sh.backend.unregister_hotkey(id);
            sh.hotkeys.borrow_mut().remove(&id);
            Ok(())
        })?,
    )?;
    host.set("hotkey", hk)?;

    // host.keys: low-level key capture + suppression (modifier-aware)
    let keys = lua.create_table()?;
    let sh = shared.clone();
    keys.set(
        "capture",
        lua.create_function(move |lua, (spec, cb): (String, Function)| {
            let (vk, mask) = backend::key_spec(&spec)
                .ok_or_else(|| mlua::Error::external(format!("unknown key spec '{spec}'")))?;
            let key = lua.create_registry_value(cb)?;
            sh.keys
                .borrow_mut()
                .retain(|(k, m, i, ..)| !(*k == vk && *m == mask && *i == idx));
            sh.keys.borrow_mut().push((vk, mask, idx, lua.clone(), key));
            sh.refresh_captured();
            sh.backend.watch_keys().map_err(mlua::Error::external)?;
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    keys.set(
        "release",
        lua.create_function(move |_, spec: String| {
            if let Some((vk, mask)) = backend::key_spec(&spec) {
                sh.keys
                    .borrow_mut()
                    .retain(|(k, m, i, ..)| !(*k == vk && *m == mask && *i == idx));
                sh.refresh_captured();
            }
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    keys.set(
        "releaseAll",
        lua.create_function(move |_, ()| {
            sh.keys.borrow_mut().retain(|(.., i, _, _)| *i != idx);
            sh.refresh_captured();
            Ok(())
        })?,
    )?;
    host.set("keys", keys)?;

    // host.os.current / host.os.is(name)
    let os = lua.create_table()?;
    os.set("current", std::env::consts::OS)?;
    os.set(
        "is",
        lua.create_function(|_, name: String| Ok(name == std::env::consts::OS))?,
    )?;
    host.set("os", os)?;

    // host.window.list() / host.window.active()  (find/findAll/onTrigger via prelude)
    let win = lua.create_table()?;
    let sh = shared.clone();
    win.set(
        "list",
        lua.create_function(move |lua, ()| {
            let t = lua.create_table()?;
            for w in sh.backend.enumerate_windows() {
                t.push(win_to_table(lua, &w)?)?;
            }
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    win.set(
        "active",
        lua.create_function(move |lua, ()| match sh.backend.active_window() {
            Some(w) => Ok(Some(win_to_table(lua, &w)?)),
            None => Ok(None),
        })?,
    )?;
    host.set("window", win)?;

    // host.screen.pixel / .size / .imageSearch
    let screen = lua.create_table()?;
    let sh = shared.clone();
    screen.set(
        "pixel",
        lua.create_function(move |lua, (x, y): (i32, i32)| {
            let (r, g, b) = sh.backend.pixel(x, y);
            let t = lua.create_table()?;
            t.set("r", r)?;
            t.set("g", g)?;
            t.set("b", b)?;
            t.set("hex", format!("#{r:02X}{g:02X}{b:02X}"))?;
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    screen.set(
        "size",
        lua.create_function(move |lua, ()| {
            let (w, h) = sh.backend.screen_size();
            let t = lua.create_table()?;
            t.set("w", w)?;
            t.set("h", h)?;
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    screen.set(
        "imageSearch",
        lua.create_function(move |lua, (template, opts): (String, Option<Table>)| {
            let path = sh.root(idx).join(&template);
            let img = image::open(&path)
                .map_err(|e| {
                    mlua::Error::external(format!("imageSearch: cannot open '{}': {e}", path.display()))
                })?
                .to_rgba8();
            let (tw, th) = (img.width(), img.height());
            let (sw, sh_) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh_);
            let cap = match sh.backend.capture(rx, ry, rw, rh) {
                Some(c) => c,
                None => return Ok(None),
            };
            let tol: u8 = opts.as_ref().and_then(|o| o.get::<u8>("tolerance").ok()).unwrap_or(0);
            match find_template(&cap, tw, th, img.as_raw(), tol) {
                Some((ox, oy)) => {
                    let t = lua.create_table()?;
                    t.set("x", rx + ox as i32)?;
                    t.set("y", ry + oy as i32)?;
                    t.set("w", tw)?;
                    t.set("h", th)?;
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        })?,
    )?;
    host.set("screen", screen)?;

    // host.ocr.recognize({ region, lang }) -> { text, words = {{text,x,y,w,h}, ...} }
    let ocr = lua.create_table()?;
    let sh = shared.clone();
    ocr.set(
        "recognize",
        lua.create_function(move |lua, opts: Option<Table>| {
            let (sw, shh) = sh.backend.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, shh);
            let lang: Option<String> = opts.as_ref().and_then(|o| o.get::<String>("lang").ok());
            let res = sh
                .backend
                .ocr(rx, ry, rw, rh, lang.as_deref())
                .map_err(mlua::Error::external)?;
            let t = lua.create_table()?;
            t.set("text", res.text)?;
            let words = lua.create_table()?;
            for wd in res.words {
                let w = lua.create_table()?;
                w.set("text", wd.text)?;
                w.set("x", rx + wd.x)?;
                w.set("y", ry + wd.y)?;
                w.set("w", wd.w)?;
                w.set("h", wd.h)?;
                words.push(w)?;
            }
            t.set("words", words)?;
            Ok(t)
        })?,
    )?;
    host.set("ocr", ocr)?;

    // host.input: cursorPos / move / click / drag / scroll / send / text
    let input = lua.create_table()?;
    let sh = shared.clone();
    input.set(
        "cursorPos",
        lua.create_function(move |lua, ()| {
            let (x, y) = sh.backend.cursor_pos();
            let t = lua.create_table()?;
            t.set("x", x)?;
            t.set("y", y)?;
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "move",
        lua.create_function(move |_, (x, y): (i32, i32)| {
            sh.backend.mouse_move(x, y);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "click",
        lua.create_function(move |_, (x, y, opts): (i32, i32, Option<Table>)| {
            sh.backend.mouse_click(x, y, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "drag",
        lua.create_function(move |_, (x1, y1, x2, y2, opts): (i32, i32, i32, i32, Option<Table>)| {
            sh.backend.mouse_drag(x1, y1, x2, y2, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "scroll",
        lua.create_function(move |_, (x, y, amount): (i32, i32, i32)| {
            sh.backend.mouse_scroll(x, y, amount);
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "send",
        lua.create_function(move |_, combo: String| {
            sh.backend.key_send(&combo).map_err(mlua::Error::external)
        })?,
    )?;
    let sh = shared.clone();
    input.set(
        "text",
        lua.create_function(move |_, text: String| {
            sh.backend.type_text(&text);
            Ok(())
        })?,
    )?;
    host.set("input", input)?;

    // host.sound.play(path) — fire-and-forget audio asset playback
    let sound = lua.create_table()?;
    let sh = shared.clone();
    sound.set(
        "play",
        lua.create_function(move |_, rel: String| {
            if let Some(handle) = &sh.audio {
                let path = sh.root(idx).join(&rel);
                let play = || -> anyhow::Result<()> {
                    let file = std::io::BufReader::new(std::fs::File::open(&path)?);
                    let source = rodio::Decoder::new(file)?;
                    let sink = rodio::Sink::try_new(handle)?;
                    sink.append(source);
                    sink.detach();
                    Ok(())
                };
                if let Err(e) = play() {
                    eprintln!("  [sound] cannot play '{}': {e}", path.display());
                }
            }
            Ok(())
        })?,
    )?;
    host.set("sound", sound)?;

    // host.path(rel) -> real path (escape hatch)
    let sh = shared.clone();
    host.set(
        "path",
        lua.create_function(move |_, rel: String| {
            Ok(sh.root(idx).join(&rel).to_string_lossy().to_string())
        })?,
    )?;

    // host.resource.read(rel) -> bytes/string from the package
    let resource = lua.create_table()?;
    let sh = shared.clone();
    resource.set(
        "read",
        lua.create_function(move |_, rel: String| {
            let path = sh.root(idx).join(&rel);
            std::fs::read_to_string(&path).map_err(mlua::Error::external)
        })?,
    )?;
    host.set("resource", resource)?;

    lua.globals().set("host", host)?;
    Ok(())
}

/// Returns true if the given module VM registered any window triggers.
fn window_has_triggers(lua: &Lua) -> bool {
    (|| -> mlua::Result<bool> {
        let host: Table = lua.globals().get("host")?;
        let window: Table = host.get("window")?;
        let has: Function = window.get("_hasTriggers")?;
        has.call::<bool>(())
    })()
    .unwrap_or(false)
}

/// Reads `{ button = "left"|"right"|"middle" }` from input opts (default left).
fn button_from(opts: Option<&Table>) -> MouseButton {
    let name = opts.and_then(|o| o.get::<String>("button").ok()).unwrap_or_default();
    match name.to_ascii_lowercase().as_str() {
        "right" => MouseButton::Right,
        "middle" => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

/// Reads an optional `{ region = { x1, y1, x2, y2 } }` (named or positional) and
/// returns `(x, y, w, h)`, defaulting to the full screen.
fn read_region(opts: Option<&Table>, sw: i32, sh: i32) -> (i32, i32, i32, i32) {
    match opts.and_then(|o| o.get::<Table>("region").ok()) {
        Some(region) => {
            let x1: i32 = region.get("x1").or_else(|_| region.get(1)).unwrap_or(0);
            let y1: i32 = region.get("y1").or_else(|_| region.get(2)).unwrap_or(0);
            let x2: i32 = region.get("x2").or_else(|_| region.get(3)).unwrap_or(sw);
            let y2: i32 = region.get("y2").or_else(|_| region.get(4)).unwrap_or(sh);
            (x1, y1, (x2 - x1).max(0), (y2 - y1).max(0))
        }
        None => (0, 0, sw, sh),
    }
}

/// Naive template search over a captured region (early-out per position; compares
/// RGB and honors the template's alpha as a mask). Returns the top-left offset.
fn find_template(hay: &CapturedImage, tw: u32, th: u32, tmpl: &[u8], tol: u8) -> Option<(u32, u32)> {
    if tw == 0 || th == 0 || tw > hay.w || th > hay.h {
        return None;
    }
    for oy in 0..=(hay.h - th) {
        for ox in 0..=(hay.w - tw) {
            if matches_at(hay, ox, oy, tw, th, tmpl, tol) {
                return Some((ox, oy));
            }
        }
    }
    None
}

fn matches_at(hay: &CapturedImage, ox: u32, oy: u32, tw: u32, th: u32, tmpl: &[u8], tol: u8) -> bool {
    let tol = tol as i16;
    for ty in 0..th {
        for tx in 0..tw {
            let ti = ((ty * tw + tx) * 4) as usize;
            if tmpl[ti + 3] == 0 {
                continue; // transparent template pixel = wildcard
            }
            let hi = (((oy + ty) * hay.w + (ox + tx)) * 4) as usize;
            for c in 0..3 {
                if (hay.rgba[hi + c] as i16 - tmpl[ti + c] as i16).abs() > tol {
                    return false;
                }
            }
        }
    }
    true
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
