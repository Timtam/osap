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

use backend::{Backend, CapturedImage, HostEvents, MouseButton, WinInfo};
use module_manifest::LoadedModule;

/// Luau prelude that adds the OS-gated `host.window.find`/`findAll` matcher on
/// top of the native `host.window.list`/`active` bindings.
const WINDOW_PRELUDE: &str = include_str!("window_prelude.luau");

/// Luau prelude implementing the `host.overlay` self-voicing control-tree runtime.
const OVERLAY_PRELUDE: &str = include_str!("overlay_prelude.luau");

/// Shared host state that the `host` API closures access via `Rc<RefCell<…>>`.
struct HostState {
    root: PathBuf,
    tts: Tts,
    /// Registered global hotkeys: backend id → Luau callback (kept in the Lua registry).
    hotkeys: Vec<(i32, RegistryKey)>,
    hotkey_counter: i32,
    /// Audio output stream (kept alive so detached sounds keep playing) + its handle.
    _audio_stream: Option<rodio::OutputStream>,
    audio: Option<rodio::OutputStreamHandle>,
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

    fn on_window_activate(&mut self, win: WinInfo) {
        let table = match win_to_table(self.lua, &win) {
            Ok(t) => t,
            Err(_) => return,
        };
        let res = (|| -> mlua::Result<()> {
            let host: Table = self.lua.globals().get("host")?;
            let window: Table = host.get("window")?;
            let dispatch: Function = window.get("_dispatchActivate")?;
            dispatch.call::<()>(table)
        })();
        if let Err(e) = res {
            eprintln!("  [trigger] dispatch error: {e}");
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
    let (audio_stream, audio_handle) = match rodio::OutputStream::try_default() {
        Ok((s, h)) => (Some(s), Some(h)),
        Err(e) => {
            eprintln!("  [sound] no audio output device: {e}");
            (None, None)
        }
    };
    let state = Rc::new(RefCell::new(HostState {
        root: module.root.clone(),
        tts,
        hotkeys: Vec::new(),
        hotkey_counter: 0,
        _audio_stream: audio_stream,
        audio: audio_handle,
    }));

    let lua = Lua::new();
    install_host_api(&lua, &state, &backend).context("failed to install host API")?;
    lua.load(WINDOW_PRELUDE)
        .set_name("window_prelude")
        .exec()
        .context("failed to load host.window matcher prelude")?;
    lua.load(OVERLAY_PRELUDE)
        .set_name("overlay_prelude")
        .exec()
        .context("failed to load host.overlay runtime")?;

    let entry = module.entry_path();
    let code = std::fs::read_to_string(&entry)
        .with_context(|| format!("entry point not readable: {}", entry.display()))?;
    lua.load(code)
        .set_name(entry.display().to_string())
        .exec()
        .context("error while running the module entry point")?;

    let has_hotkeys = !state.borrow().hotkeys.is_empty();
    let has_triggers = window_has_triggers(&lua);
    if has_hotkeys || has_triggers {
        if has_triggers {
            backend
                .watch_foreground()
                .map_err(|e| anyhow::anyhow!("{e}"))
                .context("failed to watch foreground windows")?;
        }
        println!("» Listening for events — press Ctrl+C to quit.");
        let mut dispatcher = Dispatcher {
            lua: &lua,
            state: state.clone(),
        };
        backend
            .run_event_loop(&mut dispatcher)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("event loop failed")?;
    } else {
        wait_for_speech(&state);
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
            // Note: do NOT echo the spoken text to the console — a screen reader
            // reading the terminal would announce it a second time (double speech).
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
            Ok(id)
        })?,
    )?;
    let s_unhk = state.clone();
    let b_unhk = backend.clone();
    hk.set(
        "unregister",
        lua.create_function(move |_, id: i32| {
            b_unhk.unregister_hotkey(id);
            s_unhk.borrow_mut().hotkeys.retain(|(hid, _)| *hid != id);
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

    // host.screen.pixel(x,y) / .size() / .imageSearch(template, { region, tolerance })
    let screen = lua.create_table()?;
    let b_pixel = backend.clone();
    screen.set(
        "pixel",
        lua.create_function(move |lua, (x, y): (i32, i32)| {
            let (r, g, b) = b_pixel.pixel(x, y);
            let t = lua.create_table()?;
            t.set("r", r)?;
            t.set("g", g)?;
            t.set("b", b)?;
            t.set("hex", format!("#{r:02X}{g:02X}{b:02X}"))?;
            Ok(t)
        })?,
    )?;
    let b_size = backend.clone();
    screen.set(
        "size",
        lua.create_function(move |lua, ()| {
            let (w, h) = b_size.screen_size();
            let t = lua.create_table()?;
            t.set("w", w)?;
            t.set("h", h)?;
            Ok(t)
        })?,
    )?;
    let b_search = backend.clone();
    let s_search = state.clone();
    screen.set(
        "imageSearch",
        lua.create_function(
            move |lua, (template, opts): (String, Option<Table>)| {
                let path = s_search.borrow().resolve(&template);
                let img = image::open(&path)
                    .map_err(|e| {
                        mlua::Error::external(format!(
                            "imageSearch: cannot open '{}': {e}",
                            path.display()
                        ))
                    })?
                    .to_rgba8();
                let (tw, th) = (img.width(), img.height());
                let (sw, sh) = b_search.screen_size();
                let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh);
                let tol: u8 = opts
                    .as_ref()
                    .and_then(|o| o.get::<u8>("tolerance").ok())
                    .unwrap_or(0);
                let cap = match b_search.capture(rx, ry, rw, rh) {
                    Some(c) => c,
                    None => return Ok(None),
                };
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
            },
        )?,
    )?;
    host.set("screen", screen)?;

    // host.ocr.recognize({ region, lang }) -> { text, words = {{text,x,y,w,h}, ...} }
    let ocr = lua.create_table()?;
    let b_ocr = backend.clone();
    ocr.set(
        "recognize",
        lua.create_function(move |lua, opts: Option<Table>| {
            let (sw, sh) = b_ocr.screen_size();
            let (rx, ry, rw, rh) = read_region(opts.as_ref(), sw, sh);
            let lang: Option<String> = opts.as_ref().and_then(|o| o.get::<String>("lang").ok());
            let res = b_ocr
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
    let b_curs = backend.clone();
    input.set(
        "cursorPos",
        lua.create_function(move |lua, ()| {
            let (x, y) = b_curs.cursor_pos();
            let t = lua.create_table()?;
            t.set("x", x)?;
            t.set("y", y)?;
            Ok(t)
        })?,
    )?;
    let b_move = backend.clone();
    input.set(
        "move",
        lua.create_function(move |_, (x, y): (i32, i32)| {
            b_move.mouse_move(x, y);
            Ok(())
        })?,
    )?;
    let b_click = backend.clone();
    input.set(
        "click",
        lua.create_function(move |_, (x, y, opts): (i32, i32, Option<Table>)| {
            b_click.mouse_click(x, y, button_from(opts.as_ref()));
            Ok(())
        })?,
    )?;
    let b_drag = backend.clone();
    input.set(
        "drag",
        lua.create_function(
            move |_, (x1, y1, x2, y2, opts): (i32, i32, i32, i32, Option<Table>)| {
                b_drag.mouse_drag(x1, y1, x2, y2, button_from(opts.as_ref()));
                Ok(())
            },
        )?,
    )?;
    let b_scroll = backend.clone();
    input.set(
        "scroll",
        lua.create_function(move |_, (x, y, amount): (i32, i32, i32)| {
            b_scroll.mouse_scroll(x, y, amount);
            Ok(())
        })?,
    )?;
    let b_send = backend.clone();
    input.set(
        "send",
        lua.create_function(move |_, combo: String| {
            b_send.key_send(&combo).map_err(mlua::Error::external)
        })?,
    )?;
    let b_text = backend.clone();
    input.set(
        "text",
        lua.create_function(move |_, text: String| {
            b_text.type_text(&text);
            Ok(())
        })?,
    )?;
    host.set("input", input)?;

    // host.sound.play(path) — fire-and-forget playback of a bundled audio asset
    let sound = lua.create_table()?;
    let s_sound = state.clone();
    sound.set(
        "play",
        lua.create_function(move |_, rel: String| {
            let st = s_sound.borrow();
            if let Some(handle) = &st.audio {
                let path = st.resolve(&rel);
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

/// Returns true if the loaded module registered any window triggers (added by
/// the window prelude).
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
    let name = opts
        .and_then(|o| o.get::<String>("button").ok())
        .unwrap_or_default();
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
