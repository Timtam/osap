//! The DAW's plug-in panel, run for real against the scripted host of `overlay_menu_tests.rs`.
//!
//! Written for the maintainer's decisions of 2026-09-27: all DAW knowledge lives in daw-hosts
//! entries, so that adding a DAW is adding an entry and every plug-in module is then recognised
//! in it. On a Mac, where a plug-in publishes no control of its own, an entry's `pluginOrigin`
//! gives the overlay runtime the plug-in's panel inside the DAW's window, and an embedded binding
//! with no control of its own for the platform takes that panel as its control — the "host
//! panel" — and asks its `identify` which plug-in the panel shows, once per STAY: the keyboard
//! inside one window's panel, which a visit to the DAW's own controls ends, because that is
//! where another plug-in is selected in the same window. Where neither the module nor the DAW
//! gives a control, the binding is inert and says why. The focus key searches the same entries.
//!
//! The scripted host is the menu tests' one with one change: `host.window.test`, `find` and
//! `findAll` are the window prelude's real matcher (`crates/host/src/window_prelude.luau`), over
//! the scripted window list, so an entry is matched here exactly as the host matches it — its
//! bundle, its subrole, its title pattern. And `T.source(path)` compiles a file of this
//! repository the way the host loads a module's code, so daw-hosts, sforzando and Kontakt are
//! loaded for real in the last scenarios, with the few host calls they make scripted there.
//!
//! And for the review of 2026-09-28: the keyboard is inside only on a chain that ends in the window
//! in front, and the origin is not asked for while nobody can say where it is; an origin that fails
//! is said and asked again; the panel's verdict is shared by the overlays of one binding and its
//! line names the overlay; a binding that joins an overlay already bound is evaluated at once;
//! sforzando reads its wordmark off the event loop on a panel, and nothing read is no answer;
//! Kontakt 7's FILE needs LIBRARY beside it, Kontakt 8's own name a wider box, and a miss is asked
//! again up to eight times in a stay; daw-hosts' REAPER origin finds the FX list by where it is, and
//! its focus key searches only the entries the platform can match and says when it has no origin.

use std::path::PathBuf;

use mlua::{Function, Lua, Table};

use super::overlay_menu_tests::{HARNESS, RUNTIME};

pub(crate) const WINDOW_PRELUDE: &str = include_str!("window_prelude.luau");

/// The helpers, added to the harness's `T`: the real matcher, the Mac windows and the entries.
/// Shared with `overlay_avenger_tests.rs`, which loads VPS Avenger's module on top of them.
pub(crate) const HP: &str = r##"
local S = T.S
S.originAsks = 0
S.rechecks = 0
S.clicks = {}

-- The applications of the scripted window list, as host.window.apps() lists them.
rawset(T.host.window, "apps", function()
  local out, seen = {}, {}
  for _, w in ipairs(S.listed or T.LISTED) do
    local a = w.app
    if a and not seen[a.pid] then
      seen[a.pid] = true
      out[#out + 1] = { pid = a.pid, exe = a.exe, name = a.name or a.exe, bundleId = a.bundleId or "" }
    end
  end
  return out
end)

-- The window prelude's matcher, over the list above: evaluated against a stand-in host of its
-- own and its three functions put on the scripted host.
do
  local proxy = {
    window = {
      list = function(f) return T.host.window.list(f) end,
      apps = function() return T.host.window.apps() end,
    },
    os = { current = T.host.os.current },
  }
  T.windowPrelude(proxy)
  for _, k in ipairs({ "test", "find", "findAll" }) do rawset(T.host.window, k, proxy.window[k]) end
end
rawset(T.host.window, "recheck", function() S.rechecks += 1 end)
rawset(T.host.os, "is", function(n) return rawget(T.host.os, "current") == n end)

-- REAPER and Logic on a Mac. REAPER's plug-in windows are standard windows: one floated out of
-- the chain (the plug-in 22 below the content origin, over the whole width) and an FX chain (the
-- plug-in right of the FX list, 52 down); Logic's is an AXDialog titled by the channel strip,
-- whose content origin is its frame.
T.MAC_REAPER = { pid = 5000, exe = "REAPER", name = "REAPER", bundleId = "com.cockos.reaper" }
T.MAC_LOGIC = { pid = 6000, exe = "Logic Pro X", name = "Logic Pro", bundleId = "com.apple.logic10" }
T.FLOAT = { id = 11, title = 'VST3i: sforzando (Plogue) - Track 1', class = "AXWindow/AXStandardWindow/",
  app = T.MAC_REAPER, bounds = { x = 100, y = 38, w = 900, h = 700 }, client = { x = 100, y = 60, w = 900, h = 678 } }
T.CHAIN = { id = 12, title = 'FX: Track 1 "Track 1"', class = "AXWindow/AXStandardWindow/",
  app = T.MAC_REAPER, bounds = { x = 0, y = 38, w = 1300, h = 760 }, client = { x = 0, y = 60, w = 1300, h = 738 } }
-- REAPER's FX list in the chain window: the DAW's own chrome, left of the panel (which begins
-- at x 240).
T.FXLIST = { id = 13, class = "AXTable//", bounds = { x = 8, y = 90, w = 222, h = 600 } }
T.LOGIC = { id = 21, title = "Inst 1", class = "AXWindow/AXDialog/", app = T.MAC_LOGIC,
  bounds = { x = 200, y = 30, w = 1000, h = 800 }, client = { x = 200, y = 30, w = 1000, h = 800 } }
T.LISTED_MAC = { T.FLOAT, T.CHAIN, T.LOGIC }

-- A DAW entry for REAPER on a Mac, as daw-hosts writes one, whose origin counts its questions.
T.REAPER_MAC = {
  macos = { app = { bundleId = "com.cockos.reaper" }, axSubrole = "AXStandardWindow" },
  pluginOrigin = { macos = function(w)
    S.originAsks += 1
    if string.sub(w.title or "", 1, 4) == "FX: " then return { 240, 52 } end
    return { 0, 22 }
  end },
}
-- The same REAPER with no origin: a DAW entry that says nothing about where a plug-in is.
T.NO_ORIGIN = { macos = { app = { bundleId = "com.cockos.reaper" }, axSubrole = "AXStandardWindow" } }
-- REAPER on Windows (the harness's FX window), with an origin for macOS only, as daw-hosts has it.
T.REAPER_WIN = {
  app = { exe = { contains = "reaper" } }, windows = { class = "#32770" },
  chrome = { "^SysListView32", "^reaperPluginHostWrapProc", "^#32770$" },
  pluginOrigin = { macos = function() S.originAsks += 1; return { 0, 22 } end },
}

-- An overlay with one button, bound embedded with `spec` (hosts: REAPER on a Mac unless given),
-- on no slot unless `opts` names one: the scripted arbiter does not tell slots apart, so two
-- overlays on slots would outrank each other.
function T.bind(spec, label, opts)
  local o = T.O.new(label or "Plug-in")
  o:addCustomButton({ label = "Preset", hotkey = "Alt+P", onActivate = function() end })
  spec.hosts = spec.hosts or { T.REAPER_MAC }
  o:attachEmbedded(spec, opts or {})
  return o
end

-- `win` in front with the keyboard on `chain` (default: the window itself), and no event: what an
-- overlay bound next sees first.
function T.at(win, chain)
  S.front, S.chain = win, chain or { win }
  T.turn()
end

-- Every overlay the runtime makes from here on, in order: a module keeps its own to itself.
function T.collect()
  local made = {}
  local new = T.O.new
  T.O.new = function(label)
    local o = new(label)
    made[#made + 1] = o
    return o
  end
  return made
end

-- What a module needs of the host beyond the menu tests' script: its dependencies, its own
-- files, and the calls daw-hosts, sforzando and Kontakt make. `root` is the module's folder, for
-- host.include; each file is evaluated once, as the host evaluates it once per VM.
function T.modules(root)
  local H = T.host
  T.daw = T.daw or T.source("modules/daw-hosts/src/main.luau")(H)
  rawset(H, "require", function(id)
    if id == "com.platform.overlay" then return T.O end
    if id == "com.platform.daw-hosts" then return T.daw end
    error("the scripted host has no module " .. id)
  end)
  rawset(H, "tryRequire", function() return nil end)
  local included = {}
  rawset(H, "include", function(rel)
    local key = root .. rel
    if included[key] == nil then included[key] = { T.source(key)(H) } end
    return included[key][1]
  end)
  rawset(H, "inputEpoch", function() return S.epoch end)
  rawset(H, "input", T.strict("host.input", {
    click = function(x, y) S.clicks[#S.clicks + 1] = { x, y } end,
    send = function() end,
  }))
  -- The accessibility questions these modules ask, answered from S.buttons (name -> centre);
  -- everything else finds nothing. The menu idiom is the harness's.
  S.buttons = S.buttons or {}
  S.located = {}
  local E = H.element
  local find = E.find
  rawset(E, "find", function(id, name, ctype)
    if name == "" and ctype == 50009 then return find(id, name, ctype) end
    return false
  end)
  rawset(E, "findAny", function() return nil end)
  rawset(E, "locate", function(id, name)
    S.located[#S.located + 1] = name
    return S.buttons[name]
  end)
  rawset(E, "pluginLocate", function() return nil end)
  rawset(E, "stateProbe", function() return nil end)
  rawset(E, "classNavPoint", function() return nil end)
  rawset(E, "focusWithin", function() return S.focusWithin end)
  rawset(H.screen, "size", function() return { w = 2560, h = 1440 } end)
  rawset(H.screen, "imageSearchAsync", function() end)
  -- A synchronous read: S.ocr(region) says what is written there (default: nothing).
  S.recognized = {}
  rawset(H.ocr, "recognize", function(opts)
    local r = opts.region
    S.recognized[#S.recognized + 1] = { r[1], r[2], r[3], r[4] }
    local text = S.ocr and S.ocr(r) or ""
    return { text = text, words = {}, skipped = false }
  end)
  -- daw-hosts' focus key: the callback the host would run on the press.
  S.hotkeyFns = {}
  local register = H.hotkey.register
  rawset(H.hotkey, "register", function(spec, fn)
    S.hotkeyFns[spec] = fn
    return register(spec, fn)
  end)
end

-- How many times host.element.locate was asked for `name` (scripted by T.modules).
function T.located(name)
  local n = 0
  for _, x in ipairs(S.located or {}) do
    if x == name then n += 1 end
  end
  return n
end

-- The corners of region `r` as one string, for messages.
function T.region(r) return table.concat({ r[1], r[2], r[3], r[4] }, ",") end
"##;

/// A fresh VM with the scripted host, the runtime, the prelude's matcher and the helpers above;
/// the host plays `os`.
fn run_on(os: &str, scenario: &str) {
    let lua = Lua::new();
    let t: Table = lua.load(HARNESS).set_name("harness").eval().expect("the harness loads");
    let host: Table = t.get("host").unwrap();
    let os_table: Table = host.raw_get("os").unwrap();
    os_table.raw_set("current", os).unwrap();
    let wrapped: Function = lua
        .load(format!("return function(host) {RUNTIME}\nend"))
        .set_name("overlay-runtime/src/main.luau")
        .eval()
        .expect("the runtime compiles");
    let o: Table = wrapped.call(host).expect("the runtime loads against the scripted host");
    t.set("O", o).unwrap();
    let prelude: Function = lua
        .load(format!("return function(host) {WINDOW_PRELUDE}\nend"))
        .set_name("window_prelude.luau")
        .eval()
        .expect("the window prelude compiles");
    t.set("windowPrelude", prelude).unwrap();
    // A file of this repository, compiled the way the host compiles a module's code: wrapped in
    // `function(host)`, and handed the scripted host by the scenario.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = lua
        .create_function(move |lua, rel: String| {
            let src = std::fs::read_to_string(root.join(&rel)).map_err(mlua::Error::external)?;
            let src = src.strip_prefix('\u{feff}').unwrap_or(&src).to_string();
            lua.load(format!("return function(host) {src}\nend"))
                .set_name(rel)
                .eval::<Function>()
        })
        .unwrap();
    t.set("source", source).unwrap();
    lua.globals().set("T", t).unwrap();
    if let Err(e) = lua.load(HP).set_name("host-panel helpers").exec() {
        panic!("{e}");
    }
    if let Err(e) = lua.load(scenario).set_name("scenario").exec() {
        panic!("{e}");
    }
}

fn run(scenario: &str) {
    run_on("windows", scenario)
}

fn run_mac(scenario: &str) {
    run_on("macos", scenario)
}

// ---------------------------------------------------------------------------------------------
// The panel is the control where the module has none.
// ---------------------------------------------------------------------------------------------

/// A binding with a Windows pattern only, on a Mac, in a REAPER window whose entry has an origin:
/// the control is the panel — the window's id and application, the panel as bounds and client —
/// and `identify` is asked about it. The origin is asked once per window and size, `identify`
/// once for the stay; a move keeps both, a resize asks both again.
#[test]
fn the_daws_panel_is_the_control_where_the_module_has_none() {
    run_mac(r##"
        local S = T.S
        T.at(T.FLOAT)
        local asked, seen = 0, nil
        local o = T.bind({ control = { windows = "^Plugin%x+$" },
          identify = function(c) asked += 1; seen = c; return true end })
        assert(o.active, T.dump())
        local c = o:origin()
        assert(c.class == "host-panel" and c.id == 11 and c.app.pid == 5000, T.dump())
        assert(c.client.x == 100 and c.client.y == 82 and c.client.w == 900 and c.client.h == 656,
          "the panel from 0,22 to the window's edges: " .. T.region({ c.client.x, c.client.y, c.client.w, c.client.h }))
        assert(c.bounds.x == 100 and c.bounds.y == 82 and c.bounds.w == 900 and c.bounds.h == 656)
        assert(seen ~= nil and seen.client.y == 82, "identify is asked about the panel")
        assert(T.count("attachEmbedded: [host-panel] panel of 'VST3i: sforzando (Plogue) - Track 1' id=11 "
          .. "at 0,22 900x656 from its content origin, identify=true") == 1, T.dump())
        assert(T.count("takes the DAW's plug-in panel (its daw-hosts entry's pluginOrigin) as the "
          .. "plug-in's control; 'Plug-in' is the first here") == 1, T.dump())
        for _ = 1, 3 do T.event() end
        assert(S.originAsks == 1 and asked == 1, "once each: " .. S.originAsks .. ", " .. asked)
        -- Moved: the same stay, the panel with the window.
        T.FLOAT.client.x, T.FLOAT.bounds.x = 300, 300
        T.event()
        assert(o.active and o:origin().client.x == 300 and o:origin().client.y == 82)
        assert(S.originAsks == 1 and asked == 1, "a move asks nothing: " .. S.originAsks .. ", " .. asked)
        -- Resized: laid out again, and another stay.
        T.FLOAT.client.h, T.FLOAT.bounds.h = 500, 522
        T.event()
        assert(o.active and o:origin().client.h == 478, T.dump())
        assert(S.originAsks == 2 and asked == 2, "a resize asks both again: " .. S.originAsks .. ", " .. asked)
    "##);
}

/// The same binding on Windows finds its own control, as it always did: the DAW's entry has an
/// origin for macOS only, so none is asked for and nothing about the panel is said.
#[test]
fn the_same_binding_finds_its_own_control_on_windows_and_asks_no_origin() {
    run(r##"
        local S = T.S
        S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
        T.turn()
        local asked = 0
        local o = T.bind({ hosts = { T.REAPER_WIN }, control = { windows = "Qt%d+.-QWindowIcon" },
          identify = function() asked += 1; return true end })
        assert(o.active, T.dump())
        assert(o:origin().id == 7 and o:origin().class == T.KK.class, "the plug-in's own control")
        assert(S.originAsks == 0, "no origin is asked on Windows")
        assert(T.count("host-panel") == 0 and T.count("plug-in panel") == 0, T.dump())
        assert(T.count("attachEmbedded: matched control [Qt671NI_6_7_1_R3QWindowIcon{8d61}] id=7, identify=true") == 1, T.dump())
        for _ = 1, 3 do T.event() end
        assert(asked == 1, "kept per control, as before")
    "##);
}

/// A plain pattern — the same string on every platform, Win32 vocabulary — finds nothing among a
/// Mac window's accessibility classes, and the binding takes the panel instead; one with no
/// `identify` has nothing to ask of a panel and stays inert, which the log says. A Mac pattern
/// that does match the plug-in's own container is used as before.
#[test]
fn a_plain_pattern_that_finds_nothing_on_a_mac_takes_the_panel_and_without_identify_nothing() {
    run_mac(r##"
        local S = T.S
        S.controls = { { id = 30, class = "AXGroup//", bounds = { x = 400, y = 300, w = 200, h = 100 },
          client = { x = 400, y = 300, w = 200, h = 100 } } }
        T.at(T.FLOAT)
        local o = T.bind({ control = "^Plugin%x+$", identify = function() return true end }, "Pattern")
        assert(o.active and o:origin().class == "host-panel", T.dump())
        assert(T.count("[Pattern] embedded control pattern '^Plugin%x+$' looks like a Win32 window class") == 1
          and T.count("then the binding takes the DAW's plug-in panel wherever the DAW's entry gives one") == 1, T.dump())
        local bare = T.bind({ control = "^com%.u%-he%.Diva%.vst%.window%d*$" }, "No identify")
        T.event()
        assert(not bare.active, "the panel says nothing about WHICH plug-in: " .. T.dump())
        assert(T.count("then the binding stays inert: it has no `identify` to ask of the DAW's plug-in panel") == 1, T.dump())
        -- A plug-in that publishes a container of its own there: the pattern's control, not the panel.
        local own = T.bind({ control = { windows = "x", macos = "^AXGroup/" } }, "Own")
        assert(own.active and own:origin().id == 30, T.dump())
    "##);
}

/// Neither the module nor the DAW gives a control: the binding is inert on this platform and one
/// line says which side gave nothing. `false` for a platform is the module's word that the
/// binding does not apply there, panel or not.
#[test]
fn neither_the_module_nor_the_host_gives_a_control_the_binding_is_inert_and_says_why() {
    run_mac(r##"
        local S = T.S
        T.at(T.FLOAT)
        local a = T.bind({ hosts = { T.NO_ORIGIN }, control = { windows = "^Plugin%x+$" },
          identify = function() return true end }, "No origin")
        local b = T.bind({ control = { windows = "^Plugin%x+$", macos = false },
          identify = function() return true end }, "Declined")
        local c = T.bind({ control = { windows = "^Plugin%x+$" } }, "Unidentified")
        for _ = 1, 3 do T.event() end
        assert(not a.active and not b.active and not c.active, T.dump())
        assert(T.count("[No origin] embedded binding is inactive on macos: neither the module (no control "
          .. "pattern for this platform) nor any of its hosts (no plug-in panel, `pluginOrigin`, on this "
          .. "platform) gives a control here") == 1, T.dump())
        assert(T.count("[Declined] embedded binding is inactive on macos: the module declares no control "
          .. "for this platform") == 1, T.dump())
        assert(T.count("[Unidentified] embedded binding is inactive on macos: the module gives no control "
          .. "pattern for this platform, and the DAW's plug-in panel does not say which plug-in it shows") == 1, T.dump())
        assert(S.originAsks == 0, "an inert binding asks nothing")
        assert(#S.triggers == 0, "and registers nothing")
    "##);
    run(r##"
        local S = T.S
        S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
        T.turn()
        local o = T.bind({ hosts = { T.REAPER_WIN }, control = { macos = "^AXGroup/" },
          identify = function() return true end }, "Mac only")
        assert(not o.active)
        assert(T.count("[Mac only] embedded binding is inactive on windows: neither the module (no control "
          .. "pattern for this platform) nor any of its hosts") == 1, T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// The keyboard on the DAW's own controls.
// ---------------------------------------------------------------------------------------------

/// The Win32 chrome classes mean nothing on a Mac, so the panel's gate is geometric: a focused
/// element outside the panel is the DAW's chrome and the overlay stays out; the window itself —
/// all a view that publishes nothing leaves on the chain — and an element inside the panel are
/// inside; an empty chain is nobody knowing, and not inside.
#[test]
fn the_keyboard_on_the_daws_own_controls_keeps_the_overlay_out() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN, { T.FXLIST, T.CHAIN })
        local o = T.bind({ control = { windows = "x" }, identify = function() return true end })
        assert(not o.active, "on the FX list: " .. T.dump())
        T.show(T.CHAIN, { T.CHAIN })
        assert(o.active, "in the plug-in's view: " .. T.dump())
        local c = o:origin()
        assert(c.client.x == 240 and c.client.y == 112 and c.client.w == 1060 and c.client.h == 686,
          "right of the FX list: " .. T.region({ c.client.x, c.client.y, c.client.w, c.client.h }))
        local inside = { id = 14, class = "AXButton//", bounds = { x = 400, y = 120, w = 40, h = 20 } }
        T.show(T.CHAIN, { inside, T.CHAIN })
        assert(o.active, "an element of the plug-in's own")
        T.show(T.CHAIN, { T.FXLIST, T.CHAIN })
        assert(not o.active, "back on the FX list")
        T.show(T.CHAIN, { T.CHAIN })
        assert(o.active)
        T.show(T.CHAIN, {})
        assert(not o.active, "an empty chain is not inside: " .. T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// Which plug-in the panel shows: the stay.
// ---------------------------------------------------------------------------------------------

/// One FX chain window shows whichever FX is selected in its list. A verdict is kept while the
/// keyboard stays in the plug-in, and asked again once it has been on the FX list — where the
/// other FX was selected — so the second plug-in is not taken for the first. The line saying the
/// verdict comes when it changes, not at every return.
#[test]
fn another_plugin_selected_in_the_same_window_is_asked_about_afresh() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local shown, asked = "mine", 0
        local o = T.bind({ control = { windows = "x" },
          identify = function() asked += 1; return shown == "mine" end },
          "Mine", { slot = "com.test.plugin", pollMatch = 500 })
        assert(o.active and asked == 1, T.dump())
        for _ = 1, 3 do T.event() end
        T.poll(500)
        assert(asked == 1, "kept while the keyboard stays inside: " .. asked)
        -- To the FX list, another FX selected there, and back into the plug-in.
        T.show(T.CHAIN, { T.FXLIST, T.CHAIN })
        assert(not o.active)
        shown = "ReaEQ"
        T.show(T.CHAIN, { T.CHAIN })
        assert(asked == 2 and not o.active, "the other plug-in is not taken for mine: " .. T.dump())
        T.poll(500)
        assert(asked == 2, "and that verdict is kept in turn")
        -- Mine selected again.
        T.show(T.CHAIN, { T.FXLIST, T.CHAIN })
        shown = "mine"
        T.show(T.CHAIN, { T.CHAIN })
        assert(asked == 3 and o.active, T.dump())
        assert(T.count("panel of 'FX: Track 1 \"Track 1\"' id=12 at 240,52 1060x686 from its content origin, identify=true") == 2
          and T.count("identify=false") == 1, "said at each change: " .. T.dump())
        assert(T.count("identify=false — 'Mine'") == 1, "naming the overlay that asked: " .. T.dump())
        -- Another window in front and back: the keyboard left, so the next stay asks again.
        T.show(T.OTHER)
        assert(not o.active)
        T.show(T.CHAIN, { T.CHAIN })
        assert(asked == 4 and o.active, T.dump())
    "##);
}

/// The rule holds for an overlay that is outranked on its slot and skips its rechecks — a
/// Kontakt header while a library overlay in another module's VM holds the slot: the VM's own
/// watcher sees the visit to the FX list, so when the slot comes back the old verdict does not.
#[test]
fn a_visit_to_the_chrome_ends_the_stay_even_while_the_overlay_is_outranked() {
    run_mac(r##"
        local S = T.S
        local foreign = false
        local winner = T.host.arbiter.winner
        rawset(T.host.arbiter, "winner", function(slot)
          if foreign then return 999 end
          return winner(slot)
        end)
        rawset(T.host.arbiter, "winnerSpecificity", function() return foreign and 100 or 0 end)
        T.at(T.CHAIN)
        local shown, asked = "mine", 0
        local o = T.bind({ control = { windows = "x" },
          identify = function() asked += 1; return shown == "mine" end }, "Mine", { slot = "com.test.plugin" })
        assert(o.active and asked == 1, T.dump())
        -- Another VM's overlay takes the slot: this one no longer looks.
        foreign = true
        T.show(T.CHAIN, { T.FXLIST, T.CHAIN })
        shown = "ReaEQ"
        T.show(T.CHAIN, { T.CHAIN })
        assert(asked == 1, "outranked, it asked nothing: " .. asked)
        -- The other overlay lets go.
        foreign = false
        T.event()
        assert(asked == 2, "asked afresh, not the old verdict: " .. asked .. "\n" .. T.dump())
        assert(not o.active, "ReaEQ is not mine: " .. T.dump())
    "##);
}

/// A module's function for the platform is handed the panel (with its stay) and may return it
/// moved — Kontakt's corner from its own FILE button. What it returns is the control, gated and
/// kept per stay like the panel. Where the DAW gives no panel it is handed nil.
#[test]
fn a_function_entry_is_handed_the_panel_and_its_answer_is_gated_and_kept_per_stay() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local got, calls, asked = nil, 0, 0
        local function moved(active, panel)
          calls += 1
          got = panel
          if not panel then return nil end
          local x, y = panel.client.x + 3, panel.client.y - 2
          local w, h = active.client.x + active.client.w - x, active.client.y + active.client.h - y
          return { id = active.id, app = active.app, class = "moved",
            bounds = { x = x, y = y, w = w, h = h }, client = { x = x, y = y, w = w, h = h } }
        end
        local o = T.bind({ control = { windows = "x", macos = moved },
          identify = function() asked += 1; return true end })
        assert(o.active, T.dump())
        assert(got.class == "host-panel" and type(got.stay) == "number", "handed the DAW's panel")
        local c = o:origin()
        assert(c.class == "moved" and c.client.x == 243 and c.client.y == 110 and c.stay == got.stay, T.dump())
        T.event()
        assert(asked == 1 and calls == 2, "identify kept for the stay; the function asked each time: " .. asked .. ", " .. calls)
        T.show(T.CHAIN, { T.FXLIST, T.CHAIN })
        assert(not o.active)
        T.show(T.CHAIN, { T.CHAIN })
        assert(o.active and asked == 2, T.dump())
        local handedNil = false
        local n = T.bind({ hosts = { T.NO_ORIGIN }, control = { macos = function(_, panel)
          handedNil = panel == nil
          return nil
        end } }, "No panel")
        T.event()
        assert(handedNil and not n.active, T.dump())
    "##);
}

/// "Could not tell yet" is asked again at the next recheck, up to eight times in a stay, and then
/// taken as no for the rest of it; the next stay asks again.
#[test]
fn could_not_tell_yet_is_asked_again_and_the_next_stay_asks_afresh() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local answer, asked = nil, 0
        local o = T.bind({ control = { windows = "x" }, identify = function() asked += 1; return answer end })
        assert(not o.active and asked == 1)
        for _ = 1, 10 do T.event() end
        assert(asked == 8, "asked " .. asked .. " times")
        assert(T.count("identify could not tell yet") == 1
          and T.count("identify could not tell 8 times in a row, so it is taken as no until the keyboard "
            .. "next leaves the panel") == 1, T.dump())
        answer = true
        T.event()
        assert(asked == 8 and not o.active, "no for the rest of the stay")
        T.show(T.CHAIN, { T.FXLIST, T.CHAIN })
        T.show(T.CHAIN, { T.CHAIN })
        assert(asked == 9 and o.active, T.dump())
    "##);
}

/// `O.hosts` takes a single entry that has nothing but a platform block — Logic's — as one entry,
/// and a list as its entries; nil and an empty table add nothing.
#[test]
fn o_hosts_takes_an_entry_with_only_a_platform_block() {
    run(r##"
        local logic = { macos = { app = { bundleId = "com.apple.logic10" } }, pluginOrigin = {} }
        local list = { { windows = { class = "a" } }, { windows = { class = "b" } } }
        local h = T.O.hosts(list, nil, logic, {})
        assert(#h == 3 and h[1] == list[1] and h[3] == logic, #h)
        assert(#T.O.hosts(logic) == 1 and #T.O.hosts({}) == 0 and #T.O.hosts(nil) == 0)
    "##);
}

// ---------------------------------------------------------------------------------------------
// daw-hosts, sforzando and Kontakt, loaded for real.
// ---------------------------------------------------------------------------------------------

/// The focus key searches every entry of `all` — the list every plug-in module binds with — and
/// REAPER's entry, which takes every dialog of its process on Windows, only by its plug-in
/// windows' titles: not REAPER's Preferences. It finds an Ableton VST2 window, which its old list
/// of its own did not know, and says which DAWs it searched when it finds nothing — those whose
/// entries can match on this platform.
#[test]
fn the_focus_key_searches_every_entry_of_all() {
    run(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        local press = S.hotkeyFns["Ctrl+Shift+Win+Alt+F6"]
        assert(press, "the key is registered")
        local PREFS = { id = 101, title = "REAPER Preferences", class = "#32770", app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 0, y = 0, w = 600, h = 500 }, client = { x = 0, y = 0, w = 600, h = 500 } }
        local SERUM = { id = 300, title = "Serum/1-MIDI", class = "AbletonVstPlugClass",
          app = { pid = 800, exe = "Ableton Live 12 Suite.exe" },
          bounds = { x = 0, y = 0, w = 800, h = 600 }, client = { x = 0, y = 0, w = 800, h = 600 } }
        S.listed = { T.MAIN, PREFS, T.FX }
        press()
        assert(S.focusCalls[#S.focusCalls] == T.FX.id, "the FX window, not the Preferences: " .. tostring(S.focusCalls[#S.focusCalls]))
        assert(S.speech[#S.speech].text == T.FX.title)
        S.listed = { T.MAIN, PREFS, SERUM }
        press()
        assert(S.focusCalls[#S.focusCalls] == 300 and S.speech[#S.speech].text == "Serum/1-MIDI", T.dump())
        S.listed = { T.MAIN, PREFS }
        local calls = #S.focusCalls
        press()
        assert(#S.focusCalls == calls and S.speech[#S.speech].text == "could not find a plugin window")
        -- The DAWs a Windows machine can have: Logic's entry is macOS-only and is not searched here.
        assert(T.count("[daw-hosts] no plug-in window found (the plug-in windows of REAPER, Ableton Live); "
          .. "in front:") == 1, T.dump())
    "##);
}

/// On a Mac the key finds Logic's plug-in window by its entry, asks the entry's origin — Logic's
/// header read off the window — and, the keyboard on Logic's header, clicks just inside the
/// plug-in's panel and announces what the reading after it says.
#[test]
fn the_focus_key_finds_a_logic_plugin_window_and_asks_its_origin() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        local press = S.hotkeyFns["Cmd+Shift+F6"]
        assert(press, "the key is registered")
        S.listed = { T.LOGIC }
        -- Logic's header: three groups ending 84 below the window's top.
        S.controls = { { id = 40, class = "AXGroup//", bounds = { x = 210, y = 60, w = 300, h = 54 } } }
        local headerButton = { id = 41, class = "AXButton//", bounds = { x = 220, y = 70, w = 20, h = 20 } }
        T.at(T.LOGIC, { headerButton, T.LOGIC })
        press()
        assert(S.focusCalls[#S.focusCalls] == 21, T.dump())
        assert(T.count("[daw-hosts] Logic plug-in window 'Inst 1': the plug-in starts at 2,88 from the content "
          .. "origin (from its header's bottom edge)") == 1, T.dump())
        assert(#S.clicks == 1 and S.clicks[1][1] == 205 and S.clicks[1][2] == 121,
          "3 in from the panel's corner: " .. tostring(S.clicks[1] and T.region({ S.clicks[1][1], S.clicks[1][2], 0, 0 })))
        S.chain = { T.LOGIC }
        T.run(300)
        assert(S.speech[#S.speech].text == "Inst 1" and S.rechecks == 1, T.dump())
    "##);
}

/// On a Mac the key searches only the entries that can match there — REAPER's and Logic's, not
/// the Windows-only bridged REAPER or Ableton — and names those DAWs when it finds nothing.
#[test]
fn the_focus_key_on_a_mac_searches_the_entries_a_mac_can_match() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        local press = S.hotkeyFns["Cmd+Shift+F6"]
        S.listed = { T.OTHER }
        T.at(T.OTHER)
        press()
        assert(S.speech[#S.speech].text == "could not find a plugin window", T.dump())
        assert(T.count("[daw-hosts] no plug-in window found (the plug-in windows of REAPER, Logic Pro); in front:") == 1, T.dump())
    "##);
}

/// The key's origin is asked only when the keyboard is somewhere it can read, and one that raises
/// is said, not kept, and gives no click: "could not tell where the plugin is". The next press
/// asks again. After asking an element to take the focus, a reading that says nothing is "could
/// not tell", not "could not move".
#[test]
fn the_focus_key_with_no_origin_says_so_and_asks_again() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        local press = S.hotkeyFns["Cmd+Shift+F6"]
        S.listed = { T.CHAIN }
        local title = 'FX: Track 1 "Track 1"'
        -- The keyboard nowhere anybody can read: nothing asked of the window.
        T.at(T.CHAIN, {})
        local controls = T.host.window.controls
        local walks = 0
        rawset(T.host.window, "controls", function() walks += 1; error("no walk today") end)
        press()
        assert(walks == 0 and S.speech[#S.speech].text == "could not tell whether the keyboard is in " .. title, T.dump())
        -- On REAPER's FX list, and the origin's walk raises.
        T.at(T.CHAIN, { T.FXLIST, T.CHAIN })
        press()
        assert(walks == 1 and #S.clicks == 0, T.dump())
        assert(S.speech[#S.speech].text == "could not tell where the plugin is in " .. title, S.speech[#S.speech].text)
        assert(T.count("[daw-hosts] the entry's pluginOrigin for '" .. title .. "' (1300x738) raised:") == 1, T.dump())
        -- The walk answers: the FX list read, the panel right of it, a click just inside.
        rawset(T.host.window, "controls", controls)
        S.controls = { T.FXLIST }
        press()
        assert(#S.clicks == 1 and S.clicks[1][1] == 242 and S.clicks[1][2] == 115,
          "3 in from 239,52: " .. tostring(S.clicks[1] and T.region({ S.clicks[1][1], S.clicks[1][2], 0, 0 })))
        S.chain = { T.CHAIN }
        T.run(300)
        assert(S.speech[#S.speech].text == title, T.dump())
        -- Asked to take the focus, and the reading after it says nothing.
        T.at(T.CHAIN, { T.FXLIST, T.CHAIN })
        S.focusWithin = { name = "FILE" }
        press()
        S.chain = {}
        T.run(300)
        assert(S.speech[#S.speech].text == "could not tell whether the keyboard is in " .. title, S.speech[#S.speech].text)
    "##);
}

/// daw-hosts' REAPER origin, for real, in an FX chain window: the FX list is found by where it is
/// — the leftmost surface that begins where a plug-in would not and does not span the window —
/// not by coming first, so a plug-in's own group and a container of the whole window do not move
/// the panel. A window whose surfaces the host could not read gives no origin that time, which is
/// not kept: the next evaluation asks again.
#[test]
fn the_reaper_origin_finds_the_fx_list_by_where_it_is() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        S.listed = T.LISTED_MAC
        local group = { id = 31, class = "AXGroup//", bounds = { x = 400, y = 150, w = 300, h = 200 } }
        local whole = { id = 32, class = "AXSplitGroup//", bounds = { x = 0, y = 60, w = 1300, h = 738 } }
        S.controls = { group, whole, T.FXLIST }
        T.at(T.CHAIN)
        local o = T.bind({ hosts = T.daw.all, control = { windows = "x" }, identify = function() return true end })
        assert(o.active, T.dump())
        local c = o:origin()
        assert(c.client.x == 239 and c.client.y == 112, "right of the FX list, 8 + 222 + 9: "
          .. T.region({ c.client.x, c.client.y, c.client.w, c.client.h }))
        -- Resized, and the host could not read the window's surfaces this time.
        T.CHAIN.client.w, T.CHAIN.bounds.w = 1200, 1200
        S.controls = {}
        T.event()
        assert(not o.active, T.dump())
        assert(T.count("[overlay] the DAW entry's pluginOrigin for 'FX: Track 1 \"Track 1\"' (id=12, 1200x738) "
          .. "answered a nil, not { dx, dy } — no plug-in panel there") == 1, T.dump())
        S.controls = { T.FXLIST }
        T.event()
        assert(o.active and o:origin().client.x == 239, "asked again, and read this time: " .. T.dump())
    "##);
}

/// sforzando, with no macOS entry of its own, comes up in a REAPER window floated out of its
/// chain and in a Logic window titled "Inst 1" — a title that names no plug-in — by its wordmark,
/// read OFF the event loop where it belongs in the DAW's panel: nil until the answer lands, then a
/// recheck. Text that is not the wordmark is another plug-in, kept for the stay; nothing read is
/// no answer, and the next evaluation reads again. It is evaluated as it is bound, and its
/// read-outs land relative to the panel.
#[test]
fn sforzando_binds_in_a_logic_window_titled_inst_1_and_in_a_reaper_floating_window() {
    run_mac(r##"
        local S = T.S
        local made = T.collect()
        T.modules("modules/sforzando/")
        S.listed = T.LISTED_MAC
        -- The wordmark read's answer: "sforzando" when the region holds the box the Windows layout
        -- puts the wordmark in (605..762 x 33..70 of the plug-in) — an assumption, since the Mac
        -- build's wordmark has never been read; the Mac region is that box grown by a few points —
        -- and "Piano" when it does not.
        local panel = nil
        local function answer(r)
          local g = r.region
          local text = (panel and g[1] <= panel.x + 605 and g[3] >= panel.x + 762
            and g[2] <= panel.y + 33 and g[4] >= panel.y + 70) and "sforzando" or "Piano"
          r.cb({ status = "text", text = text, words = { { text = text, x = g[1], y = g[2], w = 60, h = 12 } } })
        end
        panel = { x = 100, y = 82 }
        T.at(T.FLOAT)
        T.source("modules/sforzando/src/main.luau")(T.host)
        local ov = made[1]
        assert(ov and ov.label == "sforzando" and #made == 1, "one overlay, for every place it lives")
        -- Evaluated as it was bound, its embedded context too: the read is out, nothing was read on
        -- the event loop, and the panel is not sforzando's until the answer says so.
        assert(#S.reads == 1 and #S.recognized == 0, T.dump())
        assert(T.region(S.reads[1].region) == "697,111,870,156",
          "the wordmark, relative to the panel: " .. T.region(S.reads[1].region))
        assert(not ov.active, T.dump())
        T.event()
        assert(#S.reads == 1, "one read per stay, not one per evaluation")
        answer(S.reads[1])
        assert(S.rechecks == 1, "the answer asks the overlays to look again")
        T.event()
        assert(ov.active, T.dump())
        local c = ov:origin()
        assert(c.class == "host-panel" and c.client.x == 100 and c.client.y == 82, T.dump())
        -- Its read-outs are read relative to the panel: Polyphony's Mac region from its corner.
        local polyphony = nil
        for _ = 1, 3 do
          ov:focusNext()
          for _, r in ipairs(S.recognized) do
            if T.region(r) == "590,120,630,140" then polyphony = r end
          end
        end
        assert(polyphony, "Polyphony read at the panel + {490,38,530,58}: " .. T.dump())
        -- Logic: its window is titled by the channel strip; the panel starts 2 in and 88 down.
        S.controls = {}
        panel = { x = 202, y = 118 }
        T.show(T.LOGIC)
        assert(not ov.active, "another stay: asked again")
        answer(S.reads[#S.reads])
        T.event()
        assert(ov.active, T.dump())
        c = ov:origin()
        assert(c.client.x == 202 and c.client.y == 118 and c.client.w == 998 and c.client.h == 712, T.dump())
        assert(T.count("panel of 'Inst 1' id=21 at 2,88 998x712 from its content origin, identify=true — 'sforzando'") == 1, T.dump())
        -- The keyboard on Logic's header: out of the way.
        local headerButton = { id = 41, class = "AXButton//", bounds = { x = 220, y = 70, w = 20, h = 20 } }
        T.show(T.LOGIC, { headerButton, T.LOGIC })
        assert(not ov.active, T.dump())
        -- Another plug-in's window, not painted yet: nothing read, no answer, read again.
        panel = nil
        T.show(T.FLOAT)
        local reads = #S.reads
        S.reads[reads].cb({ status = "none", text = "", words = {} })
        T.event()
        assert(not ov.active and #S.reads == reads + 1, "nothing read is asked again: " .. T.dump())
        -- Painted: its text is not the wordmark, and that is kept for the stay.
        answer(S.reads[#S.reads])
        T.event()
        T.event()
        assert(not ov.active and #S.reads == reads + 1, "asked afresh, and not sforzando: " .. T.dump())
    "##);
}

/// sforzando on Windows is found by its own control, as it always was — evaluated as it is bound
/// — and its wordmark read relative to that control, on the event loop, once per control.
#[test]
fn sforzando_still_binds_by_its_control_on_windows() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.modules("modules/sforzando/")
        local plugin = { id = 70, class = "Plugin00A1B2C3", bounds = { x = 240, y = 90, w = 760, h = 560 },
          client = { x = 240, y = 90, w = 760, h = 560 } }
        S.front, S.controls, S.chain = T.FX, { T.LIST, plugin }, { plugin, T.FX }
        S.ocr = function(r)
          if T.region(r) == "845,123,1002,160" then return "sforzando" end
          return ""
        end
        T.turn()
        T.source("modules/sforzando/src/main.luau")(T.host)
        local ov = made[1]
        assert(ov.active and ov:origin().id == 70, T.dump())
        assert(#S.reads == 0, "no read off the loop on Windows")
        assert(T.count("attachEmbedded: matched control [Plugin00A1B2C3] id=70, identify=true") == 1, T.dump())
    "##);
}

/// Kontakt 7, with no title check, comes up in a REAPER floating window and in a Logic window
/// titled "Inst 1": found by its own FILE button, with LIBRARY beside it, near where its geometry
/// puts it in the DAW's panel, its corner taken from that button and the difference to the DAW's
/// origin logged; the evidence is kept for the stay. A Kontakt 7 that publishes nothing is found
/// by its top row read as text, off the loop, with nothing searched while the read is out. A FILE
/// with no LIBRARY beside it is not taken for Kontakt's, nor one somewhere else in the window;
/// a read that saw nothing is asked again, text that is not Kontakt's top row is kept, and an
/// answer for a stay that is over is dropped.
#[test]
fn kontakt_7_binds_in_a_logic_window_titled_inst_1_and_in_a_reaper_floating_window() {
    run_mac(r##"
        local S = T.S
        local made = T.collect()
        T.modules("modules/kontakt/")
        S.listed = T.LISTED_MAC
        local FLOAT = T.FLOAT
        FLOAT.title = "VST3i: Kontakt 7 (Native Instruments) (64 out) - Track 1"
        -- Kontakt 7's FILE at x,y, and LIBRARY 55 to its right, as the third session found them.
        local function k7At(x, y)
          S.buttons = { FILE = { x = x, y = y }, LIBRARY = { x = x + 55, y = y } }
        end
        local function answerHeader(r, fx, fy)
          r.cb({ status = "text", words = {
            { text = "FILE", x = fx - 12, y = fy - 4, w = 24, h = 8 },
            { text = "LIBRARY", x = fx + 56 - 25, y = fy - 4, w = 50, h = 8 },
          } })
        end
        local header = { id = 41, class = "AXButton//", bounds = { x = 220, y = 70, w = 20, h = 20 } }
        -- REAPER: the panel at 100,82; FILE's centre 176,19 from Kontakt's corner.
        k7At(100 + 176, 82 + 19)
        T.at(FLOAT)
        local K = T.source("modules/kontakt/src/main.luau")(T.host)
        K.activate()
        local k7
        for _, o in ipairs(made) do
          if o.label == "Kontakt 7" and o.active then k7 = o end
        end
        assert(k7, "Kontakt 7's in-DAW overlay is up: " .. T.dump())
        local c = k7:origin()
        assert(c.variant == "Kontakt 7" and c.client.x == 101 and c.client.y == 82, "Kontakt's own corner: " .. T.dump())
        assert(T.count("[kontakt] Kontakt 7 in 'VST3i: Kontakt 7 (Native Instruments) (64 out) - Track 1' (900x678): "
          .. "FILE at 276,101 puts Kontakt's corner 1,0 from the DAW's plug-in origin, which is 0,22 from the "
          .. "content origin") == 1, T.dump())
        for _, o in ipairs(made) do
          assert(o == k7 or not o.active, "only Kontakt 7's in-DAW cell: " .. tostring(o.label))
        end
        assert(T.count("[Content Missing] embedded binding is inactive on macos: the module declares no "
          .. "control for this platform") == 1, T.dump())
        -- The evidence is kept for the stay: the evaluations after the one that found it search nothing.
        local files = T.located("FILE")
        for _ = 1, 4 do T.event() end
        assert(k7.active and T.located("FILE") == files, "FILE searched again in one stay: " .. T.located("FILE") - files)
        -- Logic, "Inst 1": the panel at 202,118, FILE where Kontakt's would be.
        S.controls = {}
        k7At(202 + 175, 118 + 21)
        T.show(T.LOGIC)
        assert(k7.active, T.dump())
        c = k7:origin()
        assert(c.client.x == 202 and c.client.y == 120, "corner 0,2 from Logic's origin: " .. T.dump())
        -- A Kontakt 7 that publishes nothing: its top row read as text, in a small region.
        T.show(T.LOGIC, { header, T.LOGIC })
        assert(not k7.active)
        S.buttons = {}
        local reads = #S.reads
        T.show(T.LOGIC)
        assert(not k7.active, "not Kontakt's until the read says so")
        assert(#S.reads == reads + 1, "one read: " .. T.dump())
        local r = S.reads[#S.reads]
        assert(r.region[3] - r.region[1] <= 200 and r.region[4] - r.region[2] <= 80,
          "a small region, not the top band: " .. T.region(r.region))
        -- While it is out, nothing is searched and nothing more is read.
        files = T.located("FILE")
        T.event()
        assert(T.located("FILE") == files and #S.reads == reads + 1, T.dump())
        answerHeader(r, 202 + 175, 118 + 19)
        assert(S.rechecks >= 1, "the answer asks the overlays to look again")
        T.event()
        assert(k7.active and k7:origin().client.x == 202 and k7:origin().client.y == 118, T.dump())
        -- FILE where Kontakt's would be, with no LIBRARY beside it: not taken by accessibility, and
        -- the top row is read instead. That read sees nothing — asked again at the next evaluation —
        -- and then text that is not Kontakt's top row, which is kept for the stay.
        T.show(T.LOGIC, { header, T.LOGIC })
        S.buttons = { FILE = { x = 202 + 175, y = 118 + 19 } }
        reads = #S.reads
        T.show(T.LOGIC)
        assert(not k7.active and #S.reads == reads + 1, T.dump())
        assert(T.count("[kontakt] the button named FILE in 'Inst 1' is where Kontakt 7's would be, but no button "
          .. "named LIBRARY beside it — not taken for Kontakt 7's") == 1, T.dump())
        S.reads[#S.reads].cb({ status = "none", words = {} })
        T.event()
        assert(not k7.active and #S.reads == reads + 2, "nothing read is asked again: " .. T.dump())
        S.reads[#S.reads].cb({ status = "text", words = { { text = "Serum", x = 380, y = 130, w = 40, h = 10 } } })
        files = T.located("FILE")
        T.event()
        T.event()
        assert(not k7.active and #S.reads == reads + 2 and T.located("FILE") == files, "kept for the stay: " .. T.dump())
        -- Another plug-in, with a FILE button of its own far from Kontakt's: not taken for Kontakt.
        T.show(T.LOGIC, { header, T.LOGIC })
        S.buttons = { FILE = { x = 202 + 600, y = 118 + 300 } }
        T.show(T.LOGIC)
        assert(not k7.active, T.dump())
        assert(T.count("[kontakt] the button named FILE in 'Inst 1' is at 802,418") == 1, T.dump())
        -- The keyboard leaves and comes back while that read is out: its answer, for a stay that is
        -- over, is dropped — even one that would anchor — and the new stay's own read decides.
        local old = S.reads[#S.reads]
        T.show(T.LOGIC, { header, T.LOGIC })
        T.show(T.LOGIC)
        local new = S.reads[#S.reads]
        assert(new ~= old, "the new stay reads for itself")
        local rechecks = S.rechecks
        answerHeader(old, 202 + 175, 118 + 19)
        T.event()
        assert(not k7.active and S.rechecks == rechecks, "an answer for a stay that is over: " .. T.dump())
        answerHeader(new, 202 + 175, 118 + 19)
        T.event()
        assert(k7.active, T.dump())
    "##);
}

/// Kontakt 8's button carries its own name, "Kontakt File Menu", so it is taken in a box wider
/// across than Kontakt 7's FILE — its {86,19} was authored as a click point, not measured — but
/// still 24 points down and up. A window with no such button is searched at each evaluation of a
/// stay, up to eight times, and then not again in that stay; the next stay asks again.
#[test]
fn kontakt_8_is_taken_in_a_wider_box_and_a_miss_is_asked_again_up_to_eight_times() {
    run_mac(r##"
        local S = T.S
        local made = T.collect()
        T.modules("modules/kontakt/")
        S.listed = T.LISTED_MAC
        S.controls = {}
        S.buttons = {}
        local header = { id = 41, class = "AXButton//", bounds = { x = 220, y = 70, w = 20, h = 20 } }
        T.at(T.LOGIC)
        local K = T.source("modules/kontakt/src/main.luau")(T.host)
        K.activate()
        local NAME = "Kontakt File Menu"
        assert(T.located(NAME) == 1, "asked once as it was bound: " .. T.located(NAME))
        for _ = 1, 10 do T.event() end
        assert(T.located(NAME) == 8, "eight evaluations of the stay, then no more: " .. T.located(NAME))
        assert(T.count("[kontakt] 'Inst 1' (1000x800) publishes no button named Kontakt File Menu — no Kontakt 8 "
          .. "there yet; asked again at the next evaluation, up to 8 in a row") == 1, T.dump())
        -- The next stay asks again, and finds it 60 points right of the authored point.
        S.buttons = { [NAME] = { x = 202 + 86 + 60, y = 118 + 19 } }
        T.show(T.LOGIC, { header, T.LOGIC })
        T.show(T.LOGIC)
        local k8
        for _, o in ipairs(made) do
          if o.label == "Kontakt 8" and o.active then k8 = o end
        end
        assert(k8, "Kontakt 8's in-DAW overlay is up: " .. T.dump())
        assert(k8:origin().client.x == 262 and k8:origin().client.y == 118, T.dump())
        assert(T.count("Kontakt File Menu at 348,137 puts Kontakt's corner 60,0 from the DAW's plug-in origin") == 1, T.dump())
        -- 130 across is too far, and so is 30 down.
        S.buttons = { [NAME] = { x = 202 + 86 + 130, y = 118 + 19 } }
        T.show(T.LOGIC, { header, T.LOGIC })
        T.show(T.LOGIC)
        assert(not k8.active, T.dump())
        S.buttons = { [NAME] = { x = 202 + 86, y = 118 + 19 + 30 } }
        T.show(T.LOGIC, { header, T.LOGIC })
        T.show(T.LOGIC)
        assert(not k8.active, T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// The review of 2026-09-28.
// ---------------------------------------------------------------------------------------------

/// An origin function that raises or answers something other than two numbers gives no panel,
/// is said once per window and size, is not kept, and is asked again at the next evaluation.
#[test]
fn an_origin_that_fails_gives_no_panel_is_said_once_and_asked_again() {
    run_mac(r##"
        local S = T.S
        local mode = "raise"
        local entry = {
          macos = { app = { bundleId = "com.cockos.reaper" }, axSubrole = "AXStandardWindow" },
          pluginOrigin = { macos = function()
            S.originAsks += 1
            if mode == "raise" then error("no FX list today") end
            if mode == "junk" then return { "left", 52 } end
            return { 0, 22 }
          end },
        }
        T.at(T.FLOAT)
        local asked = 0
        local o = T.bind({ hosts = { entry }, control = { windows = "x" },
          identify = function() asked += 1; return true end })
        T.event()
        T.event()
        assert(not o.active and asked == 0 and S.originAsks == 3, "asked at each evaluation: " .. S.originAsks)
        assert(T.count("[overlay] the DAW entry's pluginOrigin for 'VST3i: sforzando (Plogue) - Track 1' "
          .. "(id=11, 900x678) raised:") == 1, T.dump())
        T.FLOAT.client.h, T.FLOAT.bounds.h = 600, 622
        mode = "junk"
        T.event()
        assert(T.count("(id=11, 900x600) answered a table, not { dx, dy } — no plug-in panel there") == 1, T.dump())
        mode = "ok"
        T.event()
        T.event()
        assert(o.active and asked == 1 and S.originAsks == 5, "kept once it answered: " .. S.originAsks)
    "##);
}

/// An origin that leaves the plug-in no width or height is no panel: nothing is asked of it.
#[test]
fn a_panel_with_no_room_is_no_panel() {
    run_mac(r##"
        local S = T.S
        local entry = {
          macos = { app = { bundleId = "com.cockos.reaper" }, axSubrole = "AXStandardWindow" },
          pluginOrigin = { macos = function() return { 0, 700 } end },
        }
        T.at(T.FLOAT)
        local asked = 0
        local o = T.bind({ hosts = { entry }, control = { windows = "x" },
          identify = function() asked += 1; return true end })
        T.event()
        assert(not o.active and asked == 0, T.dump())
    "##);
}

/// A window retitled with the keyboard in it — REAPER's chain window for another track — ends the
/// stay, as a resize does; a control a function moves within one stay is asked about at its new
/// place.
#[test]
fn a_title_change_ends_the_stay_and_a_moved_control_is_asked_about_again() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local asked = 0
        local o = T.bind({ control = { windows = "x" }, identify = function() asked += 1; return true end })
        assert(o.active and asked == 1)
        T.event()
        assert(asked == 1)
        T.CHAIN.title = 'FX: Track 2 "Track 2"'
        T.event()
        assert(o.active and asked == 2, "another stay: " .. asked)
        local shift, moved = 0, 0
        local m = T.bind({ control = { windows = "x", macos = function(active, panel)
            if not panel then return nil end
            local x, y = panel.client.x + shift, panel.client.y
            local w, h = active.client.x + active.client.w - x, active.client.y + active.client.h - y
            return { id = active.id, app = active.app, class = "moved",
              bounds = { x = x, y = y, w = w, h = h }, client = { x = x, y = y, w = w, h = h } }
          end }, identify = function() moved += 1; return true end }, "Moved")
        T.event()
        assert(m.active and moved == 1, T.dump())
        shift = 10
        T.event()
        assert(m.active and moved == 2, "at its new place: " .. moved)
    "##);
}

/// The keyboard has to be inside what a function returned as well as inside the DAW's panel: an
/// element between the two is neither the plug-in nor the DAW's chrome, and the stay goes on.
#[test]
fn the_keyboard_has_to_be_inside_what_a_function_returned_too() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local asked = 0
        local o = T.bind({ control = { windows = "x", macos = function(active, panel)
            if not panel then return nil end
            local x, y = panel.client.x + 100, panel.client.y
            local w, h = active.client.x + active.client.w - x, active.client.y + active.client.h - y
            return { id = active.id, app = active.app, class = "right",
              bounds = { x = x, y = y, w = w, h = h }, client = { x = x, y = y, w = w, h = h } }
          end }, identify = function() asked += 1; return true end })
        assert(o.active and asked == 1, T.dump())
        local between = { id = 15, class = "AXButton//", bounds = { x = 260, y = 200, w = 20, h = 20 } }
        T.show(T.CHAIN, { between, T.CHAIN })
        assert(not o.active, "inside the DAW's panel, left of the plug-in's corner")
        T.show(T.CHAIN, { T.CHAIN })
        assert(o.active and asked == 1, "the stay went on: " .. asked)
    "##);
}

/// A chain that does not end in the window in front describes some other window, and an element
/// without a rectangle is nowhere: neither is inside, and the first ends the stay.
#[test]
fn a_chain_that_is_another_windows_or_an_element_without_a_place_is_not_inside() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local asked = 0
        local o = T.bind({ control = { windows = "x" }, identify = function() asked += 1; return true end })
        assert(o.active and asked == 1)
        local foreign = { id = 5100, class = "AXButton//", bounds = { x = 400, y = 300, w = 20, h = 20 } }
        T.show(T.CHAIN, { foreign, T.OTHER })
        assert(not o.active, "a chain that ends in another window")
        T.show(T.CHAIN, { T.CHAIN })
        assert(o.active and asked == 2, "and it ended the stay: " .. asked)
        T.show(T.CHAIN, { { id = 16, class = "AXButton//" }, T.CHAIN })
        assert(not o.active, "an element without a rectangle")
    "##);
}

/// With the keyboard nowhere anybody can read, the origin is not asked for: on a Mac that is the
/// busy quarantine, when the origin's own walk would answer nothing and its fallback be kept.
#[test]
fn an_empty_chain_asks_no_origin() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN, {})
        local o = T.bind({ control = { windows = "x" }, identify = function() return true end })
        T.event()
        assert(not o.active and S.originAsks == 0, "asked " .. S.originAsks)
        T.show(T.CHAIN, { T.CHAIN })
        assert(o.active and S.originAsks == 1, T.dump())
    "##);
}

/// A pattern that matches a control whose `identify` says no has found nothing of the module's
/// own, and the binding takes the panel.
#[test]
fn a_pattern_whose_control_is_not_the_plugin_falls_through_to_the_panel() {
    run_mac(r##"
        local S = T.S
        S.controls = { { id = 30, class = "AXGroup//", bounds = { x = 400, y = 300, w = 200, h = 100 },
          client = { x = 400, y = 300, w = 200, h = 100 } } }
        T.at(T.FLOAT)
        local o = T.bind({ control = { windows = "x", macos = "^AXGroup/" },
          identify = function(c) return c.class == "host-panel" end })
        assert(o.active and o:origin().class == "host-panel", T.dump())
    "##);
}

/// The stay's watcher is registered once per VM, and only where a binding can take a panel: on
/// Windows an overlay registers its own two triggers and nothing more.
#[test]
fn the_stay_watcher_is_registered_once_and_not_on_windows() {
    run(r##"
        local S = T.S
        S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
        T.turn()
        T.bind({ hosts = { T.REAPER_WIN }, control = { windows = "Qt%d+.-QWindowIcon" },
          identify = function() return true end })
        assert(#S.triggers == 2, "the overlay's own two: " .. #S.triggers)
    "##);
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        T.bind({ control = { windows = "x" }, identify = function() return true end })
        assert(#S.triggers == 4, "the overlay's two and the watcher's two: " .. #S.triggers)
        T.bind({ control = { windows = "y" }, identify = function() return true end }, "Second")
        assert(#S.triggers == 6, "the watcher once per VM: " .. #S.triggers)
    "##);
}

/// Every overlay bound to one `O.embedded` value shares the panel's verdict: an overlay that asks
/// while the first is outranked does not ask again. The line saying it names the overlay.
#[test]
fn overlays_bound_to_one_binding_share_the_panels_verdict() {
    run_mac(r##"
        local S = T.S
        local foreign = false
        local winner = T.host.arbiter.winner
        rawset(T.host.arbiter, "winner", function(slot)
          if foreign then return 999 end
          return winner(slot)
        end)
        rawset(T.host.arbiter, "winnerSpecificity", function() return foreign and 100 or 0 end)
        T.at(T.CHAIN)
        local asked = 0
        local b = T.O.embedded({ hosts = { T.REAPER_MAC }, control = { windows = "x" },
          identify = function() asked += 1; return true end })
        local first = T.O.new("First")
        first:addCustomButton({ label = "Preset", hotkey = "Alt+P", onActivate = function() end })
        first:bind(b, { slot = "com.test.plugin" })
        assert(first.active and asked == 1, T.dump())
        assert(T.count("identify=true — 'First'") == 1, T.dump())
        foreign = true
        local second = T.O.new("Second")
        second:addCustomButton({ label = "Preset", hotkey = "Alt+O", onActivate = function() end })
        second:bind(b)
        T.event()
        assert(second.active and asked == 1, "one verdict for the stay, whichever overlay asks: " .. asked)
    "##);
}

/// A DAW entry that matches the window in front and gives no origin on this platform is said once,
/// when the binding's other hosts do give one — the bind-time line cannot say it. A Win32-looking
/// pattern whose hosts give no origin says so, not that it has no `identify`.
#[test]
fn an_entry_without_an_origin_and_a_pattern_without_a_panel_say_why() {
    run_mac(r##"
        local S = T.S
        local nowhere = { daw = "Nowhere", macos = { app = { bundleId = "com.apple.logic10" } } }
        T.at(T.LOGIC)
        local o = T.bind({ hosts = { T.REAPER_MAC, nowhere }, control = { windows = "x" },
          identify = function() return true end })
        T.event()
        T.event()
        assert(not o.active, T.dump())
        assert(T.count("[overlay] 'Inst 1' is a plug-in window of Nowhere, whose daw-hosts entry gives no "
          .. "pluginOrigin on macos — no plug-in panel there") == 1, T.dump())
        T.bind({ hosts = { T.NO_ORIGIN }, control = "^Plugin%x+$", identify = function() return true end }, "Plain")
        assert(T.count("[Plain] embedded control pattern '^Plugin%x+$' looks like a Win32 window class") == 1
          and T.count("then the binding stays inert: none of its hosts gives a plug-in panel (`pluginOrigin`) "
            .. "on this platform") == 1, T.dump())
    "##);
}

/// An overlay already bound — to its standalone window — is evaluated again when an embedded
/// binding joins it, so a plug-in already in front is recognised with no event to prompt it.
#[test]
fn an_embedded_binding_joining_a_bound_overlay_is_evaluated_at_once() {
    run_mac(r##"
        local S = T.S
        T.at(T.CHAIN)
        local o = T.O.new("Twice")
        o:addCustomButton({ label = "Preset", hotkey = "Alt+P", onActivate = function() end })
        o:attach({ title = { exact = "never a window" } })
        assert(not o.active)
        o:attachEmbedded({ hosts = { T.REAPER_MAC }, control = { windows = "x" },
          identify = function() return true end }, {})
        assert(o.active, "evaluated as it joined: " .. T.dump())
    "##);
}
