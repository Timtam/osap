//! VPS Avenger's overlay module (`modules/vps-avenger`), loaded for real against the scripted host
//! of `overlay_menu_tests.rs` with the window prelude's real matcher and daw-hosts, as
//! `overlay_host_panel_tests.rs` loads sforzando and Kontakt.
//!
//! Written for the maintainer's decisions of 2026-09-28, the first stage of VPS Avenger: the
//! header — the preset's name read and said on arrival, ◀ and ▶ with the new name said once it is
//! on screen, MENU's four items chosen in Avenger's own menu, UNDO and its redo list, and a zoom
//! stepper over Avenger's zoom list — at every zoom from 50 to 200 %, in every DAW daw-hosts
//! knows. Avenger publishes no accessibility element anywhere, so everything is a click and a text
//! read: which plug-in the panel shows and at which zoom are ONE read of its header (MENU and UNDO
//! where the table puts them, the zoom field's caption as text), off the event loop; the factor
//! every coordinate is scaled by is that zoom over 50, times a display factor the header's own
//! size says (1 on a Mac, 1.5 on a Windows display at 150 %); and Avenger's corner is the DAW's
//! panel's plus one named constant, (-2, +2), on a panel only.
//!
//! The scenarios answer the module's reads with words placed where Avenger would draw them — at
//! the table's points, from the corner the module is meant to take, at the factor of the zoom —
//! so what they check is the module's arithmetic and its decisions, not Avenger: nothing here has
//! met a real one. Where a number is written out, it is worked out in the comment beside it.
//!
//! And for the review that followed: identity takes MENU and UNDO in capitals, only in boxes a
//! recogniser located, and a factor that fits them, and half an answer is read again rather than
//! taken for a "no"; on a DAW's panel the display factor is 1, on Windows an unread zoom is not
//! guessed, and a zoom step that resizes Avenger's child window keeps it Avenger's; a stay carried
//! over after our own popup is overturned by what is read; the warning box is given up only on two
//! readings without its buttons, starts on its own text every time it comes, and is not opened
//! where it would lie off the screen; a popup a choice left open is closed; a press while a step is
//! still being carried out sends nothing; and a name with nothing read before the click is never
//! said as new before the watch is over.
//!
//! And for the preset database, the same day's later decision: the module may orient itself on the
//! avenger_control project's database, installed as an optional data module
//! (`modules/vps-avenger-presets`), and marks whatever it says that was not read off the screen.
//! Its scenarios load the data module's real reader against a small fixture catalog (`T.FIXTURE`),
//! never the real data: without the module the ring and the steps are stage 1's; a database that
//! cannot be read changes nothing but Preset info's answer; a name read is placed in the catalog
//! the tool's way; ◀ and ▶ say what was read and add the database's neighbour, marked, when the
//! read failed or disagrees; and the position is forgotten when the preset may have changed.

use std::path::PathBuf;

use mlua::{Function, Lua, Table};

use super::overlay_host_panel_tests::{HP, WINDOW_PRELUDE};
use super::overlay_menu_tests::{finish, harness, RUNTIME};

/// The helpers, added on top of the host-panel ones.
const AV: &str = r##"
local S = T.S
S.clicks = {}
S.owns = true
S.ownsAsked = {}
S.marked = {}

-- The arbiter with the rule a module with a dialog layer needs: of a slot's matching claims, the
-- most specific is active, and the one it replaces is deactivated first. (The harness's own
-- activates every matching claim, which two overlays on one slot cannot live with.) Called back
-- through `Function::call`, as the host calls them (T.hostCall).
local claims = {}
local function settle(slot)
  local best
  for i, c in ipairs(claims) do
    if c.slot == slot and c.matching and (not best or c.spec > claims[best].spec) then best = i end
  end
  for i, c in ipairs(claims) do
    if c.slot == slot and c.active and i ~= best then
      c.active = false
      T.hostCall(c.off)
    end
  end
  if best and not claims[best].active then
    claims[best].active = true
    T.hostCall(claims[best].on)
  end
end
rawset(T.host, "arbiter", T.strict("host.arbiter", {
  register = function(slot, specificity, on, off)
    claims[#claims + 1] = { slot = slot, spec = specificity or 0, on = on, off = off,
      matching = false, active = false }
    return #claims
  end,
  setMatching = function(slot, id, m)
    claims[id].matching = m == true
    settle(slot)
  end,
  winner = function(slot)
    for i, c in ipairs(claims) do
      if c.slot == slot and c.active then return i end
    end
    return nil
  end,
  winnerSpecificity = function(slot)
    for _, c in ipairs(claims) do
      if c.slot == slot and c.active then return c.spec end
    end
    return nil
  end,
  participants = function(slot)
    local out = {}
    for _, c in ipairs(claims) do
      if c.slot == slot then out[#out + 1] = { module = "com.platform.vps-avenger" } end
    end
    return out
  end,
}))

rawset(T.host.window, "ownsPoint", function(id, x, y)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y }
  if type(S.owns) == "function" then return S.owns(id, x, y) end
  return S.owns
end)
-- Every process's windows, for newWindow: the harness's answers for REAPER's pid only.
rawset(T.host.window, "windowsOf", function()
  S.windowLists += 1
  return S.windows
end)
-- The recognition languages: English and German, or German alone with S.english = false (a German
-- Windows without English OCR); every read records the language it asked for.
rawset(T.host.ocr, "resolveLanguage", function(l)
  if l == nil then return "de-DE" end
  if l == "en" and S.english ~= false then return "en-US" end
  return nil
end)
do
  local read = T.submit
  T.submit = function(what, opts, cb)
    local n = #S.reads
    read(what, opts, cb)
    -- Only a read that stayed in S.reads: the runtime's focus reads go to S.focusReads.
    if #S.reads > n then S.reads[#S.reads].lang = opts and opts.lang end
  end
end
rawset(T.host.screen, "saveMarked", function(path, opts)
  local marks = {}
  for _, m in ipairs(opts.marks or {}) do marks[#marks + 1] = { m.x, m.y } end
  S.marked[#S.marked + 1] = { path = path, region = opts.region, marks = marks }
  return true
end)

-- Avenger's size at `zoom` % on a display of factor `d`: 871.25 across and 561 down at 50 %.
function T.size(zoom, d)
  local k = zoom / 50 * (d or 1)
  return math.floor(871.25 * k + 0.5), math.floor(561 * k + 0.5)
end

-- REAPER's floating window on a Mac, sized to Avenger at `zoom`: the plug-in 22 below the content
-- origin (daw-hosts), so the panel is at (100, 82) and Avenger's corner at (98, 84).
function T.float(zoom)
  local w, h = T.size(zoom)
  return { id = 31, title = "VST3i: VPS Avenger - Track 1", class = "AXWindow/AXStandardWindow/",
    app = T.MAC_REAPER, bounds = { x = 100, y = 38, w = w, h = h + 50 },
    client = { x = 100, y = 60, w = w, h = h + 22 } }
end

-- Logic's plug-in window, titled by the channel strip: the panel 2 in and 88 down from the frame
-- (daw-hosts' fallback, with no header published here), at (2, 118); Avenger's corner at (0, 120).
function T.logic(zoom)
  local w, h = T.size(zoom)
  return { id = 41, title = "Inst 1", class = "AXWindow/AXDialog/", app = T.MAC_LOGIC,
    bounds = { x = 0, y = 30, w = w + 4, h = h + 117 }, client = { x = 0, y = 30, w = w + 4, h = h + 117 } }
end

-- REAPER's FX chain on Windows, with Avenger's JUCE child at (248, 60) sized for `zoom` on a
-- display of factor `d`, the keyboard in it.
T.WFX = { id = 60, title = 'FX: Track 1 "VPS Avenger"', class = "#32770",
  app = { pid = 4242, exe = "reaper.exe" },
  bounds = { x = 0, y = 0, w = 3800, h = 2600 }, client = { x = 8, y = 30, w = 3784, h = 2562 } }
T.WLIST = { id = 61, class = "SysListView32", bounds = { x = 8, y = 60, w = 230, h = 600 },
  client = { x = 8, y = 60, w = 230, h = 600 } }
function T.juce(zoom, d)
  local w, h = T.size(zoom, d)
  return { id = 70, class = "JUCE_1a05739a091", bounds = { x = 248, y = 60, w = w, h = h },
    client = { x = 248, y = 60, w = w, h = h } }
end
function T.onWindows(zoom, d)
  local j = T.juce(zoom, d)
  S.listed = { T.WFX }
  S.front, S.controls, S.chain = T.WFX, { T.WLIST, j }, { j, T.WFX }
  T.turn()
  return j
end

-- Avenger's header as a recogniser would hand it back: words centred where the table puts them,
-- from Avenger's corner (cx, cy) at factor k — the zoom caption (unless `zoom` is false), the
-- preset's name, ◀ ▶ as nothing a recogniser reads, MENU and UNDO; `shift` moves MENU and UNDO.
function T.header(cx, cy, k, zoom, shift)
  local dx, dy = shift and shift[1] or 0, shift and shift[2] or 0
  local function word(text, ax, ay, aw, ah, sx, sy)
    local w, h = aw * k, ah * k
    return { text = text, x = cx + ax * k - w / 2 + (sx or 0), y = cy + ay * k - h / 2 + (sy or 0), w = w, h = h }
  end
  local words = {}
  if zoom then words[#words + 1] = word(zoom .. "%", 21, 14, 18, 9) end
  words[#words + 1] = word("Init", 110, 14, 18, 9)
  words[#words + 1] = word("Preset", 132, 14, 26, 9)
  words[#words + 1] = word("MENU", 262, 14, 22, 9, dx, dy)
  words[#words + 1] = word("UNDO", 305, 14, 22, 9, dx, dy)
  local text = {}
  for _, w in ipairs(words) do text[#text + 1] = w.text end
  return { status = "text", text = table.concat(text, " "), words = words }
end

-- Answers read `r` with `reading`, as the host delivers one: the epoch turns over first.
function T.answer(r, reading)
  assert(r and not r.answered, "no read waiting to be answered")
  r.answered = true
  S.epoch += 1
  T.call("answer", r.cb, reading)
end

-- The last read asked for with `key`.
function T.lastRead(key)
  for i = #S.reads, 1, -1 do
    if S.reads[i].key == key then return S.reads[i] end
  end
  return nil
end

-- Loads daw-hosts and the module as the host loads them, and hands back its two overlays: the
-- header and the warning box. The module gets a host of its own whose `host.path` is its own
-- folder, as the host scopes a module's identity; the runtime's pictures stay in the runtime's
-- (the harness's `host.path`), and everything else is the scripted host. Then the first tick,
-- which runs each overlay's first evaluation, asked for as it bound (`host.timer.after(0)`).
function T.avenger()
  local made = T.collect()
  T.modules("modules/vps-avenger/")
  rawset(T.host, "input", T.strict("host.input", {
    click = function(x, y, opts)
      S.clicks[#S.clicks + 1] = { x, y, button = opts and opts.button }
    end,
    send = function() end,
  }))
  local AH = setmetatable({}, { __index = T.host })
  AH.path = function(p) return "C:/modules/vps-avenger/" .. p end
  -- Its optional dependency: the preset database T.presets loaded, or nil — not installed.
  AH.tryRequire = function(id)
    if id == "com.platform.vps-avenger-presets" then return S.presets end
    return nil
  end
  local included = {}
  -- An included file's top level runs through `Function::call`, as the host runs it (T.hostCall).
  AH.include = function(rel)
    local key = "modules/vps-avenger/" .. rel
    if included[key] == nil then included[key] = { T.hostCall(T.source(key), AH) } end
    return included[key][1]
  end
  T.source("modules/vps-avenger/src/main.luau")(AH)
  assert(#made == 2, "two overlays: " .. #made)
  T.runDue()
  return made[1], made[2]
end

-- A reading of `words` (each { text, x, y, w, h, approx? }), as host.ocr.recognize hands one back.
function T.reading(words, lines)
  local text = {}
  for _, w in ipairs(words) do text[#text + 1] = w.text end
  return { status = "text", text = table.concat(text, " "), words = words, lines = lines or {} }
end

-- MENU's entry `i` of `header`, chosen in a popup at (x, y) w x h that opens and then closes, as
-- one that takes the choice does: the press, the tick the popup is seen and the item clicked, and
-- the two ticks that see it gone.
function T.choose(header, i, x, y, w, h)
  T.pressAt(header, i)
  T.popup(300, x, y, w, h)
  T.tick(1)
  T.closePopups()
  T.tick(2)
end

-- The box's reading: `text` above YES and NO where Avenger at 80 % in the floating window has them.
function T.boxReading(text, yes, no)
  local function w(t, x, y, ww, hh) return { text = t, x = x, y = y, w = ww, h = hh } end
  return { status = "text", text = text .. "\n" .. (yes or "YES") .. " " .. (no or "NO"),
    lines = { w(text, 600, 490, 250, 16) },
    words = { w(yes or "YES", 703.6, 573, 28, 16), w(no or "NO", 867.2, 572, 20, 16) } }
end

-- A reading of the plug-in with no box in it.
function T.noBox()
  return { status = "text", text = "EDITOR ARP", lines = {},
    words = { { text = "EDITOR", x = 600, y = 420, w = 60, h = 16 }, { text = "ARP", x = 680, y = 420, w = 40, h = 16 } } }
end

-- The focused control `i` of overlay `o`, pressed with Return as the captured key delivers it.
function T.pressAt(o, i)
  o.focus = i
  o:_syncNativeKeys()
  assert(S.holding["Return"], "Return is not captured on control " .. i)
  S.epoch += 1
  T.call("key", S.captured["Return"])
end

-- Right (dir 1) or Left on control `i`, a stepper.
function T.stepAt(o, i, dir)
  o.focus = i
  o:_syncNativeKeys()
  local key = dir > 0 and "Right" or "Left"
  assert(S.holding[key], key .. " is not captured on control " .. i)
  S.epoch += 1
  T.call("key", S.captured[key])
end

function T.clickAt(i)
  local c = S.clicks[i or #S.clicks]
  return c and (c[1] .. "," .. c[2]) or "none"
end

function T.said(from)
  local out = {}
  for i = from or 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

function T.lastSaid() return S.speech[#S.speech] and S.speech[#S.speech].text end

-- A popup window of the DAW's process appears: id `id`, at x, y, w x h.
function T.popup(id, x, y, w, h)
  S.windows[#S.windows + 1] = { id = id, layer = 101, class = "", x = x, y = y, w = w, h = h }
end
function T.closePopups() S.windows = { S.windows[1] } end

-- An assertion whose failure prints the whole log and everything said to the test's output: a Luau
-- error message is cut at about 500 bytes, which is a few log lines.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  print("said: " .. T.said())
  error(tostring(what or "assertion failed"), 2)
end

-- ── The preset database ──────────────────────────────────────────────────────
-- A small catalog in the data module's format (modules/vps-avenger-presets/README.md), NOT the
-- real data: four expansions whose presets carry the cases the lookup has to get right — a name
-- in three of them ("BA Deep Sub"), one differing from another only in case ("BA Reese", "Ba
-- Reese"), the acute accent as an apostrophe ("LD Palm´s Lead 1"), a trailing space and a double
-- one, a capital outside ASCII ("PL Üsch"); oscillators and macros with Avenger's default names
-- ("" in the file), and a preset with two macro tabs. Alpha's list is in category blocks that are
-- not in the categories' alphabetical order: Arp, Bass, Pads, Leads. Delta is a numbered series,
-- whose names are all similar to each other.
T.FIXTURE = {
  ["data/index.json"] = [=[{"format":1,"exported":"2026-09-26 18:57","factoryStandard":["Alpha"],
"expansions":[
{"name":"Alpha","file":"data/expansions/alpha.json","presets":["AR Band Pass Slide","AR Deep Arp","BA Deep Sub","BA Reese","PD Warm Pad","LD Palm´s Lead 1"]},
{"name":"Beta","file":"data/expansions/beta.json","presets":["BA Deep Sub","SQ Feel It","PD Soft Pad ","FX  Riser"]},
{"name":"Gamma","file":"data/expansions/gamma.json","presets":["Ba Reese","BA Deep Sub","PL Üsch"]},
{"name":"Delta","file":"data/expansions/delta.json","presets":["LD Lead 1","LD Lead 2","LD Lead 3"]}
]}]=],
  ["data/expansions/alpha.json"] = [=[{"format":1,"name":"Alpha",
"categories":[{"name":"Arp","start":1,"count":2},{"name":"Bass","start":3,"count":2},{"name":"Leads","start":6,"count":1},{"name":"Pads","start":5,"count":1}],
"presets":[
{"name":"AR Band Pass Slide","osc":["Arp","","Lead"],"macros":[["Tone","","Drive","+12",""]]},
{"name":"AR Deep Arp","osc":["One Osc"],"macros":[["Cut","Res","Env","Hold","Gate"],["","Wobble","","",""]]},
{"name":"BA Deep Sub","osc":["Sub","Bass"],"macros":[["Tone","Chorus","Drive","+12","Crush"]]},
{"name":"BA Reese","osc":["Reese"],"macros":[["","","","",""]]},
{"name":"PD Warm Pad","osc":["Pad"],"macros":[["Air","","","",""]]},
{"name":"LD Palm´s Lead 1","osc":["Lead","Sub"],"macros":[["Glide","","","",""]]}
]}]=],
  ["data/expansions/beta.json"] = [=[{"format":1,"name":"Beta",
"categories":[{"name":"Bass","start":1,"count":1},{"name":"Effects","start":4,"count":1},{"name":"Pads","start":3,"count":1},{"name":"Sequences","start":2,"count":1}],
"presets":[
{"name":"BA Deep Sub","osc":["Deep"],"macros":[["Tone","","","",""]]},
{"name":"SQ Feel It","osc":["Seq"],"macros":[["Gate","","","",""]]},
{"name":"PD Soft Pad ","osc":["Soft"],"macros":[["Air","","","",""]]},
{"name":"FX  Riser","osc":["Noise"],"macros":[["Rise","","","",""]]}
]}]=],
  ["data/expansions/gamma.json"] = [=[{"format":1,"name":"Gamma",
"categories":[{"name":"Bass","start":1,"count":2},{"name":"Plucked","start":3,"count":1}],
"presets":[
{"name":"Ba Reese","osc":["Reese"],"macros":[["","","","",""]]},
{"name":"BA Deep Sub","osc":["Sub"],"macros":[["","","","",""]]},
{"name":"PL Üsch","osc":["Pluck"],"macros":[["","","","",""]]}
]}]=],
  ["data/expansions/delta.json"] = [=[{"format":1,"name":"Delta",
"categories":[{"name":"Leads","start":1,"count":3}],
"presets":[
{"name":"LD Lead 1","osc":["Lead"],"macros":[["","","","",""]]},
{"name":"LD Lead 2","osc":["Lead"],"macros":[["","","","",""]]},
{"name":"LD Lead 3","osc":["Lead"],"macros":[["","","","",""]]}
]}]=],
}

-- The preset database installed: its own reader (modules/vps-avenger-presets/src/main.luau), run
-- as the host runs a code module in a dependent's VM — with a host of its own, whose
-- host.resource.read reads `files` (a published path -> its text; default T.FIXTURE) and raises
-- as the host does for one that is not there, and the host's real JSON decoder. Every read is
-- recorded in S.presetReads. Call it before T.avenger, which hands it to the module.
function T.presets(files)
  files = files or T.FIXTURE
  S.presetReads = {}
  local PH = setmetatable({}, { __index = T.host })
  PH.resource = { read = function(rel)
    S.presetReads[#S.presetReads + 1] = rel
    local text = files[rel]
    if text == nil then error("The system cannot find the file specified. (os error 2)", 0) end
    return text
  end }
  PH.json = { decode = T.jsonDecode }
  S.presets = T.source("modules/vps-avenger-presets/src/main.luau")(PH)
  return S.presets
end

-- REAPER's floating window at 80 % with Avenger identified, the name reading `name` on the event
-- loop (nil: nothing), arrival said: the header overlay, the box overlay.
function T.avengerAt80(name)
  local W = T.float(80)
  S.listed = { W }
  T.at(W)
  local header, box = T.avenger()
  T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
  T.event()
  T.nameIs(name)
  T.run(400)
  return header, box, W
end

-- What the preset's name reads, on the event loop (the region the module reads it in at 80 %).
function T.nameIs(name)
  S.ocr = function(g) return T.region(g) == "152,95,440,119" and (name or "") or "" end
end

-- A read of the name off the event loop, answered.
function T.nameRead(text) return { status = "text", text = text, words = {} } end

-- ◀ or ▶ (control `i`) with the name reading `before` on the event loop, then the reads after the
-- click answered with `after`: a name, or nil for none at all. Runs the watch to its end, answering
-- every read it makes, and gives back what was said.
function T.step(header, i, before, after)
  T.nameIs(before)
  local said = #S.speech
  T.pressAt(header, i)
  for _ = 1, 25 do
    T.run(150)
    local r = T.lastRead("vps-avenger name")
    if r and not r.answered then
      T.answer(r, after and T.nameRead(after) or { status = "none", text = "", words = {} })
    end
  end
  T.ok(#S.speech == said + 1, "one thing said per step: " .. T.said(said + 1))
  return T.lastSaid()
end

-- Whether `s` begins with `prefix`.
function T.starts(s, prefix) return string.sub(s or "", 1, #prefix) == prefix end

-- Preset info (control 4 with the database) with the name reading `name`: what it said.
function T.info(header, name)
  T.nameIs(name)
  T.pressAt(header, 4)
  return T.lastSaid()
end
"##;

/// A fresh VM with the scripted host, the runtime, the prelude's matcher, the host-panel helpers
/// and the ones above; the host plays `os`.
fn run_on(os: &str, scenario: &str) {
    let lua = Lua::new();
    let t: Table = harness(&lua);
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
    // The host's own JSON decoder, for the preset database's reader (T.presets).
    t.set("jsonDecode", lua.create_function(crate::json::decode).unwrap()).unwrap();
    lua.globals().set("T", t).unwrap();
    for (name, chunk) in [("host-panel helpers", HP), ("avenger helpers", AV)] {
        if let Err(e) = lua.load(chunk).set_name(name).exec() {
            panic!("{e}");
        }
    }
    if let Err(e) = lua.load(scenario).set_name("scenario").exec() {
        panic!("{e}");
    }
    finish(&lua);
}

fn run(scenario: &str) {
    run_on("windows", scenario)
}

fn run_mac(scenario: &str) {
    run_on("macos", scenario)
}

// ---------------------------------------------------------------------------------------------
// Which plug-in the panel shows, and at which zoom: one read of the header.
// ---------------------------------------------------------------------------------------------

/// In REAPER's floating window on a Mac, whose title names Avenger but is never asked: the panel
/// is not Avenger's until its header is READ, off the event loop — MENU and UNDO where the table
/// puts them, the zoom field's "80%". Then the overlay comes up, scaled by 80 / 50, from Avenger's
/// corner at the panel's + (-2, 2), and arriving says the preset's name read there.
#[test]
fn avenger_is_recognised_by_its_header_in_a_reaper_floating_window() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.ok(not header.active and not box.active, "nothing before the header is read: " .. T.dump())
        local r = T.lastRead("vps-avenger header")
        -- The band: from the panel's corner (100, 82), 0.45 of Avenger's width across — the panel's
        -- 1394, or 898 x 871/561 = 1394.3 by its height, the larger — 627.4 -> 628, and 32/871 of
        -- it down, 51.2 -> 52.
        T.ok(r and T.region(r.region) == "100,82,728,134", r and T.region(r.region))
        T.ok(r.lang == "en-US", "in English, which Avenger's words are: " .. tostring(r.lang))
        T.ok(#S.recognized == 0, "nothing read on the event loop")
        T.event()
        T.ok(#S.reads == 1, "one read per stay, not one per evaluation: " .. #S.reads)
        T.answer(r, T.header(98, 84, 1.6, 80))
        T.ok(S.rechecks == 1, "the answer asks the overlays to look again")
        T.event()
        T.ok(header.active and not box.active)
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1394x898): VPS Avenger, the zoom field "
          .. "reads 80 % — zoom 80 % (read), factor 1.6000, display factor 1.00. MENU read at (517,106), UNDO "
          .. "at (586,106): (+0.0,+0.0) and (+0.0,+0.0) from where the table puts them with Avenger's "
          .. "corner at the panel's -2,+2") == 1)
        -- Arriving says the preset, read at the name's box: (34, 7)-(214, 22) x 1.6 from (98, 84) =
        -- (152.4, 95.2)-(440.4, 119.2), rounded once.
        S.ocr = function(g) return T.region(g) == "152,95,440,119" and "Init Preset" or "" end
        T.run(400)
        T.ok(T.lastSaid() == "Preset, Init Preset", T.said())
        T.ok(T.count("[scale] 'VPS Avenger': factor 1.6000 about (0,0)") == 1)
    "##);
}

/// In Logic, whose plug-in window is titled by its channel strip ("Inst 1") and names no plug-in,
/// at 50 %: the panel 2 in and 88 down, Avenger's corner at the frame's (0, 90) — where the
/// project's tables start — and a factor of 1. Another plug-in shown in the same window after the
/// keyboard has been on Logic's header is asked about afresh, and its text is a real "no"; a read
/// that saw nothing is no answer, and is made again.
#[test]
fn avenger_is_recognised_in_a_logic_window_titled_inst_1_and_another_plugin_is_not() {
    run_mac(r##"
        local S = T.S
        local L = T.logic(50)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The panel at (2, 118), 873 x 590 — Logic's footer makes it taller than Avenger, so its
        -- height gives the larger width, 590 x 871/561 = 916.0: 412.2 -> 413 across, 33.7 -> 34
        -- down.
        T.ok(r and T.region(r.region) == "2,118,415,152", r and T.region(r.region))
        T.answer(r, T.header(0, 120, 1, 50))
        T.event()
        T.ok(header.active)
        T.ok(T.count("panel of 'Inst 1' id=41 at 2,88 873x590 from its content origin, identify=true — 'VPS Avenger'") == 1)
        T.ok(T.count("zoom 50 % (read), factor 1.0000, display factor 1.00") == 1)
        S.ocr = function(g) return T.region(g) == "34,127,214,142" and "Init Preset" or "" end
        T.run(400)
        T.ok(T.lastSaid() == "Preset, Init Preset", T.said())
        -- The keyboard on Logic's header, where another insert can be picked; back in, and the
        -- panel shows Logic's own EQ now.
        local headerButton = { id = 42, class = "AXButton//", bounds = { x = 20, y = 70, w = 20, h = 20 } }
        T.show(L, { headerButton, L })
        T.ok(not header.active)
        T.show(L)
        local again = T.lastRead("vps-avenger header")
        T.ok(again ~= r and not header.active, "asked afresh in the new stay")
        -- Nothing read: no answer, read again at the next evaluation.
        T.answer(again, { status = "none", text = "", words = {} })
        T.event()
        local third = T.lastRead("vps-avenger header")
        T.ok(third ~= again and not header.active, "nothing read is asked again")
        T.ok(T.count("read nothing (none) — asked again at the next evaluation") == 1)
        T.answer(third, { status = "text", text = "Channel EQ 32 Hz",
          words = { { text = "Channel", x = 10, y = 125, w = 50, h = 10 }, { text = "EQ", x = 64, y = 125, w = 16, h = 10 } } })
        T.event()
        T.event()
        T.ok(not header.active and T.lastRead("vps-avenger header") == third, "text that is not Avenger's header is kept as no")
        T.ok(T.count("[avenger] 'Inst 1' (873x590): not VPS Avenger — neither MENU nor UNDO was read") == 1)
    "##);
}

/// On Windows Avenger is its own JUCE child window, matched by the shape of JUCE's class and
/// identified by the same header read; its corner is the child's, with no DAW difference added —
/// the log line gives the offsets with the panel's (-2, +2) as well, so one Windows session says
/// which of the two it is. At 150 % display scaling Avenger is drawn half again as large at the
/// same zoom, which the header says: the zoom field reads 80 %, MENU and UNDO sit where a factor of
/// 2.4 puts them, and the display factor is 1.5. A display scaled to 110 % — no quarter step —
/// fits nothing: the captions are Avenger's, but it is not taken as Avenger until a factor fits.
/// Where English OCR is not installed the header is read in the user's own language.
#[test]
fn on_windows_avenger_is_its_juce_child_and_a_display_factor_is_learned_from_the_header() {
    run(r##"
        local S = T.S
        local J = T.onWindows(80, 1.5)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The child at (248, 60), 2091 across: 940.95 -> 941, and 76.8 -> 77 down.
        T.ok(r and T.region(r.region) == "248,60,1189,137", r and T.region(r.region))
        T.ok(not header.active)
        T.answer(r, T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active and header:origin().id == 70)
        T.ok(T.count("attachEmbedded: matched control [JUCE_1a05739a091] id=70, identify=true") == 1)
        T.ok(T.count("zoom 80 % (read), factor 2.4000, display factor 1.50") == 1)
        T.ok(T.count("from where the table puts them with Avenger's corner at the panel's +0,+0, "
          .. "(+2.0,-2.0) and (+2.0,-2.0) with it at -2,+2") == 1, "no DAW difference on the plug-in's own window")
        -- Undo: (305, 14) x 2.4 from (248, 60) = (980, 93.6), rounded once: (980, 94).
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "980,94", T.clickAt())
        T.ok(S.ownsAsked[#S.ownsAsked][1] == 70, "asked of Avenger's window")
        T.ok(T.lastSaid() == "Undo, activated", T.said())
    "##);
    run(r##"
        local S = T.S
        -- A Windows without English OCR: the user's own language, not a read that fails.
        S.english = false
        local J = T.onWindows(80, 1.1)
        local header = T.avenger()
        T.ok(T.lastRead("vps-avenger header").lang == "de-DE", tostring(T.lastRead("vps-avenger header").lang))
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 1.76, 80))
        T.event()
        T.ok(not header.active, "not taken as Avenger with no factor: it would take Tab, Return and Space to place nothing")
        T.ok(T.count("the header reads like VPS Avenger's, but no factor: the zoom field reads 80 %, and MENU and UNDO are read "
          .. "where a factor of 1.760 puts them, 10 % off the 1.600 that zoom gives at a display factor of 1.00 — "
          .. "not taken as VPS Avenger until one fits; read again") == 1)
        local r = T.lastRead("vps-avenger header")
        T.ok(r.answered == false, "read again at that evaluation")
        T.ok(T.count("not VPS Avenger") == 0, "and never a no")
    "##);
}

/// On Windows the runtime asks `identify` once per CONTROL, and a zoom step resizes Avenger's child
/// window without making it another control: the new size is Avenger's still, a first read there
/// that misses the header is read again ("lost"), never a "no" that would leave the overlay up with
/// every control dead. On Windows with the zoom field unread, the size is the factor and the zoom is
/// not guessed: the Zoom control says it cannot be read and opens nothing.
#[test]
fn on_windows_a_zoom_step_keeps_the_control_avengers_and_an_unread_zoom_is_not_guessed() {
    run(r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        -- The zoom field: 248 + 2.4 x 21 = 298.4 -> 298, 60 + 33.6 -> 94.
        T.ok(T.clickAt() == "298,94", T.clickAt())
        -- The list, rows of 27.6, 584 tall: a border of 2.2; entry 8 at 110 + 2.2 + 27.6 x 7.5 =
        -- 319.2 -> 319, across 270 + 75 = 345.
        T.popup(500, 270, 110, 150, 584)
        T.tick(1)
        T.ok(T.clickAt() == "345,319", T.clickAt())
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] zoom step from 80 % chosen") == 1, T.dump())
        -- REAPER resizes the child to Avenger at 85 %: 2222 x 1431.
        T.onWindows(85, 1.5)
        T.event()
        T.tick(1)
        local r = T.lastRead("vps-avenger header")
        T.ok(r and not r.answered and T.region(r.region) == "248,60,1248,142", r and T.region(r.region))
        T.answer(r, T.reading({ { text = "Loading", x = 300, y = 80, w = 60, h = 20 } }))
        T.ok(header.active, "still up")
        T.ok(T.count("not VPS Avenger") == 0, "a miss at a new size of Avenger's own control is no 'no'")
        T.ok(T.count("the header is not read where it was (neither MENU nor UNDO was read)") == 1, T.dump())
        T.tick(1)
        local r2 = T.lastRead("vps-avenger header")
        T.ok(r2 ~= r and not r2.answered, "read again")
        T.answer(r2, T.header(248, 60, 2.55, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        -- Undo at the new factor: 248 + 2.55 x 305 = 1025.75 -> 1026, 60 + 35.7 -> 96.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "1026,96", T.clickAt())
    "##);
    run(r##"
        local S = T.S
        T.onWindows(80, 1.25)
        local header = T.avenger()
        -- 80 % at a display of 125 %: the size of 100 % at 100 %. The caption unread.
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2, false))
        T.event()
        T.ok(header.active)
        T.ok(T.count("zoom not known (the display's scaling is not known, and the zoom field was not read), "
          .. "factor 2.0000 from the header's size") == 1, T.dump())
        T.ok(header.controls[10].text(header) == "cannot be read now")
        local clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.tick(1)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Zoom, cannot be read now, slider", "nothing opened: " .. T.said())
        -- Undo at the size's factor: 248 + 610 = 858, 60 + 28 = 88.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "858,88", T.clickAt())
    "##);
}

/// Identity is strict. A caption misread as another step fits nothing on a DAW's panel, where the
/// display factor is 1 (60 % read as "50%"), and is read again rather than taken; another
/// plug-in's "Menu" and "Undo" in mixed case are not Avenger's; captions only in boxes the
/// recogniser guessed are no evidence and are read again, not a "no"; and a percentage in the
/// preset's name is not the zoom.
#[test]
fn identity_takes_capitals_located_boxes_and_a_factor_that_fits() {
    run_mac(r##"
        local S = T.S
        local W = T.float(60)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.2, 60)
        reading.words[1].text = "50%"
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active)
        T.ok(T.count("the header reads like VPS Avenger's, but no factor: the zoom field reads 50 %, and MENU and "
          .. "UNDO are read where a factor of 1.200 puts them, 20 % off the 1.000 that zoom gives at a display "
          .. "factor of 1.00") == 1, T.dump())
        -- Read again at the next evaluation, and read right this time.
        T.event()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.2, 60))
        T.event()
        T.ok(header.active, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.6, 80)
        reading.words[4].text, reading.words[5].text = "Menu", "Undo"
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active)
        T.ok(T.count("not VPS Avenger — neither MENU nor UNDO was read") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.6, 80)
        reading.words[4].approx, reading.words[5].approx = true, true
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active)
        T.ok(T.count("MENU and UNDO were read only in boxes the recogniser guessed in the header band") == 1, T.dump())
        T.ok(T.count("not VPS Avenger") == 0, "no answer, not a no")
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.ok(header.active)
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        -- The caption unread, and the preset called "Pad 100%": its "100%" on the header's row,
        -- right of where the name begins (98 + 34 x 1.6 = 152.4).
        local reading = T.header(98, 84, 1.6, false)
        table.insert(reading.words, 3, { text = "100%", x = 328, y = 99.4, w = 20, h = 14 })
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(header.active)
        T.ok(T.count("the zoom field was not read — zoom 80 % (seen from the header's size, not read)") == 1, T.dump())
    "##);
}

/// Every control's click point at 50, 80 and 200 %, in REAPER's floating window: the table's
/// point x zoom / 50 from Avenger's corner, (98, 84) — the panel's (100, 82) + (-2, 2) — rounded
/// once.
#[test]
fn every_click_lands_where_the_table_puts_it_at_50_80_and_200_percent() {
    run_mac(r##"
        local S = T.S
        local header
        -- zoom -> { previous, next, MENU, UNDO, zoom field } as "x,y", worked out:
        --   50 %:  98 + 220 = 318, 98 + 232 = 330, 98 + 262 = 360, 98 + 305 = 403, 98 + 21 = 119;
        --          y 84 + 14 = 98
        --   80 %:  98 + 352 = 450, 98 + 371.2 -> 469, 98 + 419.2 -> 517, 98 + 488 = 586,
        --          98 + 33.6 -> 132; y 84 + 22.4 -> 106
        --   200 %: 98 + 880 = 978, 98 + 928 = 1026, 98 + 1048 = 1146, 98 + 1220 = 1318,
        --          98 + 84 = 182; y 84 + 56 = 140
        local expected = {
          [50] = { "318,98", "330,98", "360,98", "403,98", "119,98" },
          [80] = { "450,106", "469,106", "517,106", "586,106", "132,106" },
          [200] = { "978,140", "1026,140", "1146,140", "1318,140", "182,140" },
        }
        for _, zoom in ipairs({ 50, 80, 200 }) do
          local W = T.float(zoom)
          S.listed = { W }
          if not header then
            T.at(W)
            header = T.avenger()
          else
            T.show(W)
          end
          local k = zoom / 50
          T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, k, zoom))
          T.event()
          T.ok(header.active, zoom .. " %: active")
          S.ocr = function() return "Init Preset" end
          local e = expected[zoom]
          T.pressAt(header, 2)
          T.ok(T.clickAt() == e[1], zoom .. " % previous: " .. T.clickAt())
          T.pressAt(header, 3)
          T.ok(T.clickAt() == e[2], zoom .. " % next: " .. T.clickAt())
          T.pressAt(header, 4)
          T.ok(T.clickAt() == e[3] and T.lastSaid() == "Load preset, activated", zoom .. " % MENU: " .. T.clickAt())
          T.pressAt(header, 8)
          T.ok(T.clickAt() == e[4] and S.clicks[#S.clicks].button == nil, zoom .. " % undo: " .. T.clickAt())
          T.pressAt(header, 9)
          T.ok(T.clickAt() == e[4] and S.clicks[#S.clicks].button == "right", zoom .. " % redo list: " .. T.clickAt())
          -- A step that has an entry beside it: up from 50 and 80, down from 200.
          T.stepAt(header, 10, zoom == 200 and -1 or 1)
          T.ok(T.clickAt() == e[5], zoom .. " % zoom field: " .. T.clickAt())
          T.run(9000)
          T.closePopups()
        end
    "##);
}

// ---------------------------------------------------------------------------------------------
// What the plug-in did, read back.
// ---------------------------------------------------------------------------------------------

/// ▶ is clicked, and then nothing is said until the preset's name READ on screen differs from the
/// one read just before the click: a read that still shows the old name is not said, the new one
/// is. ◀ that changes nothing is said as it is, "unchanged", when the watch gives up. Something
/// drawn over ▶ is not clicked.
#[test]
fn previous_and_next_say_the_new_name_once_it_is_on_screen() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        S.ocr = function(g) return T.region(g) == "152,95,440,119" and "Init Preset" or "" end
        T.run(400)
        local function name(text) return { status = "text", text = text, words = {} } end
        local said = #S.speech
        T.pressAt(header, 3)
        T.ok(T.clickAt() == "469,106", T.clickAt())
        T.ok(#S.speech == said, "nothing is said at the click: " .. T.said(said + 1))
        T.run(150)
        local r1 = T.lastRead("vps-avenger name")
        T.ok(r1 and T.region(r1.region) == "152,95,440,119", "the name's box, off the event loop")
        T.answer(r1, name("Init Preset"))
        T.run(150)
        T.ok(#S.speech == said, "the old name, not redrawn yet, is not said: " .. T.said(said + 1))
        local r2 = T.lastRead("vps-avenger name")
        T.ok(r2 ~= r1, "read again")
        T.answer(r2, name("BA  Deep Sub"))
        T.run(150)
        T.ok(T.lastSaid() == "BA Deep Sub", T.said())
        T.ok(T.count("[watch] VPS Avenger: changed after") == 1)
        -- ◀ at the first preset: the name stays, and is said as it is when the watch gives up.
        S.ocr = function(g) return T.region(g) == "152,95,440,119" and "BA Deep Sub" or "" end
        T.pressAt(header, 2)
        T.ok(T.clickAt() == "450,106", T.clickAt())
        -- The watch looks every 100 ms for 2000: twenty looks, one per tick of 150.
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, name("BA Deep Sub")) end
        end
        T.ok(T.lastSaid() == "BA Deep Sub, unchanged", T.said())
        -- Something drawn over ▶: not clicked, and said.
        S.owns = function(id) return id ~= 31 end
        local clicks = #S.clicks
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks and T.lastSaid() == "something else is covering Next preset", T.said())
    "##);
}

/// The name before the click cannot be read (small text at 50 %): with no name read before at all,
/// nothing counts as a change — the first reading after the click is taken before Avenger redraws,
/// and would be the OLD name said as new — and the watch says the last name it read when it is
/// over, without "unchanged". Once a name has been read, it stands in for the one that could not be.
#[test]
fn a_name_before_the_click_that_cannot_be_read_is_not_taken_for_a_change() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        S.ocr = function() return "" end
        T.run(400)
        local function name(text) return { status = "text", text = text, words = {} } end
        local said = #S.speech
        T.pressAt(header, 3)
        local answers = { "Init Preset", "BA Deep Sub" }
        for i = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, name(answers[math.min(i, 2)])) end
          if i < 13 then
            T.ok(#S.speech == said, "nothing said before the watch is over: " .. T.said(said + 1))
          end
        end
        T.ok(T.lastSaid() == "BA Deep Sub", "the last name read, with no claim of a change: " .. T.said(said + 1))
        for i = said + 1, #S.speech do
          T.ok(S.speech[i].text ~= "Init Preset", "the old name is never said as new")
        end
        -- The next press: the name before is the last one read.
        said = #S.speech
        T.pressAt(header, 3)
        T.run(150)
        T.answer(T.lastRead("vps-avenger name"), name("BA Deep Sub"))
        T.run(150)
        T.ok(#S.speech == said, "the last name, not redrawn yet, is not said")
        T.answer(T.lastRead("vps-avenger name"), name("CH Chord Stack"))
        T.run(150)
        T.ok(T.lastSaid() == "CH Chord Stack", T.said(said + 1))
    "##);
}

/// MENU's items are chosen in Avenger's own popup once newWindow sees it — not after a wait —
/// across the popup's middle, at the row the table puts the item from Avenger's corner, and on a
/// Mac asked of the popup's own window by its number in the window list. A popup that opened above
/// MENU, where JUCE puts one with no room below, is not chosen in, and is closed by clicking MENU
/// again. On Windows the popup's own window is asked whether it is drawn at the item.
#[test]
fn menu_items_are_chosen_in_avengers_popup_once_it_is_seen() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.pressAt(header, 4)
        T.ok(T.clickAt() == "517,106" and T.lastSaid() == "Load preset, activated", T.clickAt())
        local clicks = #S.clicks
        T.tick(3)
        T.ok(#S.clicks == clicks, "no popup, no item: nothing on a timer")
        -- MENU's popup below MENU: (245, 20) from Avenger's corner in 50 % units, 120 x 60. Load's row
        -- is y 32: 84 + 51.2 -> 135; the middle of the popup across, 490 + 96 = 586.
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,135", T.clickAt())
        T.ok(T.count("[avenger] 'Load preset': MENU's popup at (245.0,20.0) 120.0x60.0 from Avenger's "
          .. "corner in 50 % units; the item 11.9 below its top, across its middle") == 1)
        T.ok(T.count("its window 300 of the window list is asked what is drawn there") == 1)
        T.closePopups()
        T.tick(2)
        -- Save as, and a popup JUCE moved up above MENU: (28 - 84) / 1.6 = -35. Said, and closed with
        -- a second click on MENU.
        T.pressAt(header, 6)
        T.popup(301, 490, 28, 192, 96)
        T.tick(1)
        T.ok(T.lastSaid() == "Save preset as is not available now", T.said())
        T.ok(T.count("[avenger] 'Save preset as': MENU's popup opened at (245.0,-35.0) 120.0x60.0 from "
          .. "Avenger's corner in 50 % units — not below MENU, where the table's rows are; nothing chosen") == 1)
        T.ok(T.clickAt() == "517,106", "MENU again, to close it: " .. T.clickAt())
        T.ok(T.count("[avenger] 'Save preset as': its popup was left open (unplaced) — clicking its opener again to close it") == 1)
        T.closePopups()
        T.tick(2)
        -- A tooltip-sized window first is passed over; the popup after it is chosen in.
        T.pressAt(header, 4)
        T.popup(302, 520, 120, 40, 14)
        T.tick(1)
        T.ok(T.count("which its menuItem function says is not its menu (under two rows tall)") == 1, T.dump())
        T.popup(303, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,135", T.clickAt())
    "##);
    run(r##"
        local S = T.S
        T.onWindows(80, 1)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 1.6, 80))
        T.event()
        T.pressAt(header, 4)
        -- MENU: 248 + 419.2 -> 667, 60 + 22.4 -> 82.
        T.ok(T.clickAt() == "667,82", T.clickAt())
        T.popup(300, 640, 92, 192, 96)
        T.tick(1)
        -- Load's row: 60 + 51.2 -> 111; across, 640 + 96 = 736.
        T.ok(T.clickAt() == "736,111", T.clickAt())
        local asked = S.ownsAsked[#S.ownsAsked]
        T.ok(asked[1] == 300 and asked[2] == 736 and asked[3] == 111, "asked of the popup's own window")
    "##);
}

/// The zoom stepper opens Avenger's zoom list and chooses the entry beside the zoom READ — placed by
/// the list's own height, 21 rows of 11.5 x the factor — and says the zoom the field reads once it
/// shows another: a read that still shows the old zoom is read again. The new factor places every
/// click from then on. At 200 % there is nothing above, and nothing is opened. A list too short for
/// its 21 rows scrolls: nothing is chosen in it, and it is closed with a second click on the field.
/// A press while a step is still being carried out sends nothing.
#[test]
fn the_zoom_stepper_chooses_the_entry_beside_the_zoom_read_and_says_the_new_one() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        T.ok(T.clickAt() == "132,106", "the zoom field: " .. T.clickAt())
        -- Right again before the list is seen: not sent.
        local clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.ok(#S.clicks == clicks, "the field is not clicked a second time")
        -- The list below the field, 96 x 390 at (114, 114): rows of 18.4, a border of
        -- (390 - 386.4) / 2 = 1.8. Entry 8, 85 %: 114 + 1.8 + 18.4 x 7.5 = 253.8 -> 254; across, the
        -- middle, 114 + 48 = 162. The table's row would be 84 + 1.6 x 104.5 = 251.2 -> 251.
        T.popup(310, 114, 114, 96, 390)
        T.tick(1)
        T.ok(T.clickAt() == "162,254", T.clickAt())
        T.ok(T.count("[avenger] 'Zoom': 80 % to 85 %, entry 8 of the list at (10.0,18.8) 60.0x243.8 from "
          .. "Avenger's corner in 50 % units: at y 254 by the list's height, 251 by the table") == 1)
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] zoom step from 80 % chosen") == 1)
        -- Right again while the field has not been read again: not sent either.
        clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.ok(#S.clicks == clicks and T.count("the last step still waits for the zoom field — this press is not sent") == 1, T.dump())
        local r1 = T.lastRead("vps-avenger header")
        T.ok(r1 and not r1.answered, "the header is read again")
        T.answer(r1, T.header(98, 84, 1.6, 80))
        T.tick(1)
        T.ok(not string.find(T.lastSaid() or "", "Zoom", 1, true), "the old zoom is not said: " .. T.said())
        local r2 = T.lastRead("vps-avenger header")
        T.ok(r2 ~= r1 and not r2.answered, "read again")
        T.answer(r2, T.header(98, 84, 1.7, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        local says = 0
        for _, s in ipairs(S.speech) do if s.text == "Zoom, 85 percent, slider" then says += 1 end end
        T.ok(says == 1, "said once, not once per press: " .. T.said())
        -- Undo at the new factor: 98 + 1.7 x 305 = 616.5 -> 617, 84 + 23.8 -> 108.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "617,108", T.clickAt())
        -- A list that scrolls: 200 tall where 21 rows of 19.55 are 410.6. Said, and closed with the
        -- field: 98 + 1.7 x 21 = 133.7 -> 134, 84 + 23.8 -> 108.
        T.stepAt(header, 10, 1)
        T.popup(311, 114, 114, 96, 200)
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom is not available now", T.said())
        T.ok(T.count("the popup is 200 tall where the list's 21 rows are 411 — cut short, so it scrolls; nothing chosen") == 1)
        T.ok(T.clickAt() == "134,108", "the field again, to close it: " .. T.clickAt())
        T.closePopups()
        T.run(3000)
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(200)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 4, 200))
        T.event()
        T.run(400)
        local clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.tick(1)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Zoom, 200 percent, slider", "at once, nothing opened: " .. T.said())
        T.ok(T.count("[avenger] 'Zoom': no step up from 200 % — nothing opened") == 1)
    "##);
}

/// REAPER resizes its floating window to Avenger's new size after a zoom step, which on a Mac ends
/// the runtime's stay and asks `identify` again: the header of a window that was Avenger's, whose
/// popup this module opened, is still Avenger's, so the overlay stays up through its own step and
/// the stepper says the new zoom.
#[test]
fn the_overlay_stays_up_through_its_own_zoom_step_when_the_window_resizes() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        T.popup(310, 114, 114, 96, 390)
        T.tick(1)
        T.closePopups()
        -- 871.25 x 1.7 = 1481.1 -> 1481 across, 561 x 1.7 = 953.7 -> 954 down: the resize comes
        -- before the menu tests have seen the list go.
        local W85 = T.float(85)
        S.listed = { W85 }
        T.show(W85)
        T.ok(header.active, "still up")
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1481x954): a new stay after a popup of ours — "
          .. "still VPS Avenger; its header is read again") == 1, T.dump())
        local r = T.lastRead("vps-avenger header")
        T.ok(r and T.region(r.region) == "100,82,767,137", r and T.region(r.region))
        T.answer(r, T.header(98, 84, 1.7, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        -- The step is over: a later stay asks the header as always.
        T.tick(2)
        T.show(W85, { { id = 50, class = "AXButton//", bounds = { x = 110, y = 62, w = 20, h = 16 } }, W85 })
        T.ok(not header.active)
        T.show(W85)
        T.ok(not header.active and not T.lastRead("vps-avenger header").answered, "asked afresh")
    "##);
}

/// A stay is carried over only after our own popup, at the size the header was read at, and once:
/// the redo list in front and back is Avenger's at once, and the read that stay starts confirms
/// it; the window retitled in place after that — Logic's Link mode switching the insert, say — is
/// asked afresh. A carried stay whose reads find text without the header twice in a row is
/// overturned: nothing is placed there from then on, and the next stay asks the header.
#[test]
fn a_stay_is_carried_over_once_after_our_own_popup_and_checked_by_its_read() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local POP = { id = 77, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_REAPER,
          bounds = { x = 560, y = 116, w = 200, h = 150 }, client = { x = 560, y = 116, w = 200, h = 150 } }
        T.pressAt(header, 9)
        T.popup(77, 560, 116, 200, 150)
        T.show(POP)
        T.closePopups()
        T.show(W)
        T.ok(header.active, "carried over")
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        -- Retitled in place, the keyboard still inside: a new stay, asked afresh.
        local W2 = T.float(80)
        W2.title = "VST3i: Channel EQ - Track 1"
        S.listed = { W2 }
        T.show(W2)
        T.ok(not header.active, "not carried a second time")
        T.ok(not T.lastRead("vps-avenger header").answered, "asked afresh")
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local POP = { id = 77, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_REAPER,
          bounds = { x = 560, y = 116, w = 200, h = 150 }, client = { x = 560, y = 116, w = 200, h = 150 } }
        T.pressAt(header, 9)
        T.popup(77, 560, 116, 200, 150)
        T.show(POP)
        T.closePopups()
        T.show(W)
        T.ok(header.active)
        local eq = T.reading({ { text = "Channel", x = 110, y = 90, w = 50, h = 12 }, { text = "EQ", x = 170, y = 90, w = 16, h = 12 } })
        T.answer(T.lastRead("vps-avenger header"), eq)
        T.ok(T.count("not VPS Avenger after all") == 0, "one read without the header is not enough")
        header.controls[10].text(header)
        T.answer(T.lastRead("vps-avenger header"), eq)
        T.ok(T.count("not VPS Avenger after all — neither MENU nor UNDO was read in 2 reads in a row since "
          .. "the stay was carried over") == 1, T.dump())
        local clicks = #S.clicks
        T.pressAt(header, 8)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Undo is not available now", T.said())
    "##);
}

/// A zoom field the recogniser cannot read leaves the header's own size: on a DAW's panel the step
/// it lands on is taken as the zoom, said to be seen rather than read. A header read 12 points
/// right of where the table puts it still fits its zoom, and the line says by how much — the
/// measurement a tester's log brings back. A caption misread as the step beside it ("85%" at
/// 80 %) fits no factor on a panel, where the display factor is 1 and the nearest two steps are
/// more than the 3 % apart a reading may be off by: not taken as Avenger until one fits.
#[test]
fn an_unread_zoom_field_takes_the_step_the_header_lands_on_and_the_offset_is_logged() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, false))
        T.event()
        T.ok(header.active)
        T.ok(T.count("the zoom field was not read — zoom 80 % (seen from the header's size, not read), factor 1.6000") == 1)
        header.focus = 10
        T.ok(header.controls[10].text(header) == "80 percent")
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80, { 12, 0 }))
        T.event()
        T.ok(header.active)
        T.ok(T.count("factor 1.6000, display factor 1.00. MENU read at (529,106), UNDO at (598,106): (+12.0,+0.0) "
          .. "and (+12.0,+0.0) from where the table puts them") == 1)
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.6, 80)
        reading.words[1].text = "85%"
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active, "no factor, not taken as Avenger")
        T.ok(T.count("the header reads like VPS Avenger's, but no factor: the zoom field reads 85 %, and MENU and "
          .. "UNDO are read where a factor of 1.600 puts them, 6 % off the 1.700 that zoom gives at a display "
          .. "factor of 1.00") == 1)
    "##);
}

// ---------------------------------------------------------------------------------------------
// Avenger's warning box.
// ---------------------------------------------------------------------------------------------

/// Initialize is chosen in MENU; once the popup has taken the choice, the warning box's overlay
/// looks for the box, and only then — its poll does not even run before. Once YES and NO are read,
/// the box overlay takes the keyboard: its first stop is the box's own text, and YES is clicked
/// where it was read. When the box has gone — two readings without YES and NO — the header is
/// back, on the control the user was on.
#[test]
fn the_warning_box_is_an_overlay_of_its_own_while_it_is_up() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.poll(500)
        T.ok(T.lastRead("vps-avenger dialog") == nil, "not looked for before it is expected")
        T.pressAt(header, 7)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        -- Initialize's row: 84 + 1.6 x 68 = 192.8 -> 193.
        T.ok(T.clickAt() == "586,193", T.clickAt())
        T.ok(T.count("Avenger's warning box is looked for") == 0, "not before the popup has taken it")
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] 'Initialize preset' chosen: Avenger's warning box is looked for, up to 8 reads") == 1)
        T.poll(500)
        local r = T.lastRead("vps-avenger dialog")
        -- The box's band, (280, 200)-(600, 336) x 1.6 from (98, 84).
        T.ok(r and T.region(r.region) == "546,404,1058,622", r and T.region(r.region))
        T.ok(header.active and not box.active)
        local function w(text, x, y, ww, hh) return { text = text, x = x, y = y, w = ww, h = hh } end
        T.answer(r, { status = "text", text = "Warning\nThis will initialize all settings\nAre you sure?\nYES NO",
          lines = {
            w("Warning", 640, 430, 90, 16), w("This will initialize all settings", 560, 460, 330, 16),
            w("Are you sure?", 630, 490, 150, 16), w("YES NO", 703, 572, 194, 17),
          },
          words = { w("Warning", 640, 430, 90, 16), w("YES", 703.6, 573, 28, 16), w("NO", 867.2, 572, 20, 16) } })
        T.ok(T.count("[avenger] 'Initialize preset': Avenger's warning box is up: \"Warning, This will "
          .. "initialize all settings, Are you sure?\"; YES read at (718,581), NO at (877,580) — (+2,+1) and "
          .. "(+0,+0) from where the table puts them") == 1)
        T.event()
        T.ok(box.active and not header.active)
        T.run(400)
        T.ok(T.lastSaid() == "Avenger asks, Warning, This will initialize all settings, Are you sure?", T.said())
        -- YES where it was read: 717.6 - 100 -> 618 and 581 - 82 = 499 into the plug-in.
        T.pressAt(box, 2)
        T.ok(T.clickAt() == "718,581" and T.lastSaid() == "Yes, activated", T.clickAt())
        -- The box has gone: one reading without it is not enough.
        T.poll(500)
        local gone = T.lastRead("vps-avenger dialog")
        T.ok(gone ~= r)
        T.answer(gone, T.noBox())
        T.ok(T.count("the warning box has gone") == 0, "one reading is not enough")
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.noBox())
        T.ok(T.count("[avenger] 'Initialize preset': the warning box has gone") == 1)
        T.event()
        T.ok(header.active and not box.active)
        T.run(400)
        T.ok(T.lastSaid() == "Initialize preset, button", "back where the user was: " .. T.said())
        local last = T.lastRead("vps-avenger dialog")
        T.poll(500)
        T.ok(T.lastRead("vps-avenger dialog") == last, "not looked for any more")
    "##);
}

/// A second box on the same window — Save's after Initialize's — starts on its own text again, not
/// on the button the first was answered with: a user who counts stops from the text lands where
/// they expect. While it is up, a reading of nothing, a reading with one of the buttons misread
/// ("N0") and one text reading without them do not give it up.
#[test]
fn a_second_warning_box_starts_on_its_text_and_is_not_given_up_on_one_miss() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.choose(header, 7, 490, 116, 192, 96)
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Are you sure?"))
        T.event()
        T.run(400)
        T.ok(box.active and T.lastSaid() == "Avenger asks, Are you sure?", T.said())
        -- No, the third stop: 877.2 - 100 -> 777 and 580 - 82 = 498 into the plug-in.
        T.pressAt(box, 3)
        T.ok(T.clickAt() == "877,580", T.clickAt())
        for _ = 1, 2 do
          T.poll(500)
          T.answer(T.lastRead("vps-avenger dialog"), T.noBox())
        end
        T.event()
        T.run(400)
        T.ok(header.active and T.lastSaid() == "Initialize preset, button", T.said())
        -- Save's box: row 43.5, 84 + 69.6 -> 154.
        T.pressAt(header, 5)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,154", T.clickAt())
        T.closePopups()
        T.tick(2)
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Overwrite preset?", "YES", "N0"))
        T.event()
        T.run(400)
        T.ok(box.active and box.focus == 1, "on its first stop: " .. tostring(box.focus))
        T.ok(T.lastSaid() == "Avenger asks, Overwrite preset?", T.said())
        -- Nothing read, then text without the buttons once: still up.
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), { status = "none", text = "", words = {} })
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.noBox())
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Overwrite preset?"))
        T.event()
        T.ok(box.active, "not given up on one miss: " .. T.dump())
        T.ok(T.count("the warning box has gone") == 1, "only the first box's")
    "##);
}

/// A box that does not come is looked for eight reads, and no more.
#[test]
fn a_warning_box_that_does_not_come_is_looked_for_eight_reads() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.pressAt(header, 5)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        -- Save's row, 43.5 (the project's "about 43", between Load's 32 and Save as's 55): 84 + 69.6
        -- -> 154.
        T.ok(T.clickAt() == "586,154", T.clickAt())
        T.closePopups()
        T.tick(2)
        local reads = 0
        for _ = 1, 12 do
          T.poll(500)
          local r = T.lastRead("vps-avenger dialog")
          if r and not r.answered then
            reads += 1
            T.answer(r, { status = "text", text = "EDITOR", lines = {}, words = { { text = "EDITOR", x = 600, y = 420, w = 60, h = 16 } } })
          end
        end
        T.ok(reads == 8, reads)
        T.ok(T.count("[avenger] 'Save preset': no warning box in 8 reads after it was chosen — not looked for any more") == 1)
    "##);
}

/// Avenger at 200 % in Logic reaches past the bottom of the screen, and its warning box is drawn in
/// its middle: Initialize and Save would open a modal box no read and no key of ours reaches. They
/// are refused, said with the reason; Load, which opens no box, is not.
#[test]
fn initialize_and_save_are_refused_where_their_box_would_lie_off_the_screen() {
    run_mac(r##"
        local S = T.S
        local L = T.logic(200)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(0, 120, 4, 200))
        T.event()
        T.ok(header.active)
        local clicks = #S.clicks
        -- The box's band, (280, 200)-(600, 336) x 4 from (0, 120): down to 1464, past the 1440 of
        -- the screen.
        T.pressAt(header, 7)
        T.ok(#S.clicks == clicks, "nothing clicked")
        T.ok(T.lastSaid() == "Initialize preset: Avenger's warning box would open off the screen at 200 percent — zoom out first", T.said())
        T.ok(T.count("[avenger] 'Initialize preset' not chosen: Avenger's warning box would lie at (1120,920)-(2400,1464), past the screen's 2560x1440") == 1, T.dump())
        T.pressAt(header, 5)
        T.ok(#S.clicks == clicks)
        T.pressAt(header, 4)
        T.ok(#S.clicks == clicks + 1 and T.lastSaid() == "Load preset, activated", T.said())
    "##);
}

// ---------------------------------------------------------------------------------------------
// Calibration.
// ---------------------------------------------------------------------------------------------

/// In a calibrating run the header is photographed at each zoom and size it is read at: crosshairs
/// where the overlay clicks the zoom field, ◀, ▶, MENU and UNDO, then on the words read — the
/// origin question in one picture. The runtime's own shot marks the hotspots and the name's box at
/// the scaled places, and the warning box is photographed with YES and NO read and as the table has
/// them. The module's pictures go to its own folder, the runtime's to the runtime's.
#[test]
fn a_calibrating_run_draws_where_the_overlay_clicks_and_reads() {
    run_mac(r##"
        local S = T.S
        T.calibrate(true)
        rawset(T.host.element, "rawDump", function() return {} end)
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        local shot = S.marked[1]
        T.ok(shot and shot.path == "C:/modules/vps-avenger/calibration/header-80pct-1394x898.png", shot and shot.path)
        T.ok(T.region(shot.region) == "100,82,728,134")
        local marks = {}
        for _, m in ipairs(shot.marks) do marks[#marks + 1] = m[1] .. "," .. m[2] end
        T.ok(table.concat(marks, " ") == "132,106 450,106 469,106 517,106 586,106 132,106 517,106 586,106",
          table.concat(marks, " "))
        T.ok(header:calibrationShot() == true)
        T.ok(T.count("scale 1.6000 about (0,0), frame (-2,2)") == 1)
        T.ok(T.count(string.format("  %2d %-24s screen (%d,%d)  pixel %s", 1, "Preset", 296, 107, "20,20,20")
          .. "  region (152,95)-(440,119)") == 1)
        -- Undo's crosshair is the redo list's point as well; the controls the module clicks itself
        -- are in the header picture above.
        T.ok(T.count(string.format("  %2d %-24s screen (%d,%d)  pixel %s", 2, "Undo", 586, 106, "20,20,20")) == 1)
        T.ok(T.count("-> C:/modules/overlay-runtime/calibration/VPS-Avenger.png") == 1, "the runtime's shot in the runtime's folder")
        -- The warning box, after Initialize.
        T.choose(header, 7, 490, 116, 192, 96)
        T.poll(500)
        local function w(text, x, y, ww, hh) return { text = text, x = x, y = y, w = ww, h = hh } end
        T.answer(T.lastRead("vps-avenger dialog"), { status = "text", text = "Are you sure?\nYES NO",
          lines = { w("Are you sure?", 630, 490, 150, 16) },
          words = { w("YES", 703.6, 573, 28, 16), w("NO", 867.2, 572, 20, 16) } })
        local last = S.marked[#S.marked]
        T.ok(last.path == "C:/modules/vps-avenger/calibration/dialog-Initialize-preset-80pct.png", last.path)
        marks = {}
        for _, m in ipairs(last.marks) do marks[#marks + 1] = m[1] .. "," .. m[2] end
        T.ok(table.concat(marks, " ") == "718,581 877,580 716,580 877,580", table.concat(marks, " "))
    "##);
}

/// One of Avenger's popups that becomes the window in front — the redo list here — is held over by
/// the overlay, and ends the runtime's stay; back in the plug-in, the new stay is Avenger's at once,
/// because nothing but our own popup was in front (its header is read all the same). Once the
/// overlay has left the front, a new stay asks the header as always.
#[test]
fn a_popup_of_ours_in_front_does_not_take_the_overlay_down_with_it() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local reads = #S.reads
        T.pressAt(header, 9)
        T.ok(S.clicks[#S.clicks].button == "right" and T.clickAt() == "586,106", T.clickAt())
        T.ok(T.lastSaid() == "Redo list, activated", T.said())
        local POP = { id = 77, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_REAPER,
          bounds = { x = 560, y = 116, w = 200, h = 150 }, client = { x = 560, y = 116, w = 200, h = 150 } }
        T.popup(77, 560, 116, 200, 150)
        T.show(POP)
        T.ok(header.active, "held over its popup")
        T.ok(T.count("came to the front after 'Redo list' was pressed and a test sees a menu") == 1)
        -- An entry chosen with the list's own keys: the list goes, and the plug-in has the keyboard.
        T.closePopups()
        T.show(W)
        T.ok(header.active, "never out of the front")
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1394x898): a new stay after a popup of ours — "
          .. "still VPS Avenger; its header is read again") == 1)
        T.ok(#S.reads == reads + 1 and not T.lastRead("vps-avenger header").answered)
        T.show(T.OTHER)
        T.ok(not header.active)
        T.show(W)
        T.ok(not header.active and not T.lastRead("vps-avenger header").answered, "asked afresh")
    "##);
}

/// In REAPER's FX chain on a Mac the panel starts right of the FX list (daw-hosts: the list's edge
/// + 9, and 52 down), and Avenger's corner 2 left of it and 2 below; the clicks follow.
#[test]
fn avenger_in_a_reaper_fx_chain_on_a_mac() {
    run_mac(r##"
        local S = T.S
        local CHAIN = { id = 32, title = 'FX: Track 1 "Track 1"', class = "AXWindow/AXStandardWindow/",
          app = T.MAC_REAPER, bounds = { x = 0, y = 38, w = 1700, h = 1000 }, client = { x = 0, y = 60, w = 1700, h = 978 } }
        S.listed = { CHAIN }
        S.controls = { T.FXLIST }
        T.at(CHAIN)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The panel at (8 + 222 + 9, 60 + 52) = (239, 112), 1461 x 926 — its width the larger:
        -- 657.45 -> 658 across, 53.7 -> 54 down.
        T.ok(r and T.region(r.region) == "239,112,897,166", r and T.region(r.region))
        T.answer(r, T.header(237, 114, 1.6, 80))
        T.event()
        T.ok(header.active)
        -- Undo: 237 + 488 = 725, 114 + 22.4 -> 136.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "725,136", T.clickAt())
    "##);
}

// ---------------------------------------------------------------------------------------------
// The preset database (modules/vps-avenger-presets), an optional dependency: the maintainer's
// decisions of 2026-09-28 — the module may orient itself on it, and says what it did not read off
// the screen marked "from the database". Against T.FIXTURE, a catalog of four small expansions,
// never the real data.
// ---------------------------------------------------------------------------------------------

/// Not installed: the module is stage 1's — ten stops and no Preset info, and a step says the name
/// it read, as it was read.
#[test]
fn without_the_preset_database_the_module_is_stage_1s() {
    run_mac(r##"
        local S = T.S
        local header = T.avengerAt80("AR Band Pass Slide")
        local labels = {}
        for i, c in ipairs(header.controls) do labels[i] = c.label end
        T.ok(table.concat(labels, "|") == "Preset|Previous preset|Next preset|Load preset|Save preset|"
          .. "Save preset as|Initialize preset|Undo|Redo list|Zoom", table.concat(labels, "|"))
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Bond Pess Slide") == "AR Bond Pess Slide", T.said())
        T.ok(T.step(header, 2, nil, nil) == "the preset name cannot be read now", T.said())
        T.ok(T.count("database") == 0, "nothing of a database: " .. T.dump())
    "##);
}

/// Installed but not readable — its index missing, or of another format: Preset info says so, the
/// log says why, the index is not read again, and ◀ and ▶ say what they read, as without it.
/// Nothing is read at load.
#[test]
fn a_preset_database_that_cannot_be_read_leaves_the_steps_as_they_were() {
    run_mac(r##"
        local S = T.S
        T.presets({})
        local header = T.avengerAt80("AR Band Pass Slide")
        T.ok(#header.controls == 11 and header.controls[4].label == "Preset info", tostring(#header.controls))
        T.ok(#S.presetReads == 0, "nothing read at load")
        T.ok(T.info(header, "AR Band Pass Slide") == "the preset database cannot be read", T.said())
        T.ok(T.count("[avenger] the preset database cannot be read: data/index.json cannot be read: The "
          .. "system cannot find the file specified. (os error 2) — Previous and Next work without it") == 1, T.dump())
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Bond Pess Slide") == "AR Bond Pess Slide", T.said())
        T.ok(T.step(header, 3, "AR Bond Pess Slide", nil) == "the preset name cannot be read now", T.said())
        T.ok(#S.presetReads == 1, "asked once: " .. #S.presetReads)
    "##);
    run_mac(r##"
        local S = T.S
        T.presets({ ["data/index.json"] = '{"format":2,"expansions":[]}' })
        local header = T.avengerAt80("AR Band Pass Slide")
        T.ok(T.info(header, "AR Band Pass Slide") == "the preset database cannot be read", T.said())
        T.ok(T.count("data/index.json is format 2, and this reader reads format 1") == 1, T.dump())
    "##);
}

/// The reader's own answers, as the module asks them: the index in its order, an expansion's file
/// read at the first question and kept, a category by the blocks of the list (Alpha's Pads come
/// before its Leads), and Avenger's default names for what the file leaves empty — the macros
/// counted across the tabs.
#[test]
fn the_preset_databases_reader_interprets_its_format() {
    run_mac(r##"
        local S = T.S
        local P = T.presets()
        local idx = P.index()
        T.ok(idx.exported == "2026-09-26 18:57" and #idx.expansions == 4 and idx.expansions[2].name == "Beta")
        local x = P.expansion("Alpha")
        T.ok(x and #x.presets == 6, "Alpha's file")
        T.ok(P.expansion("Alpha") == x, "kept")
        local name, k, n = P.category(x, 5)
        T.ok(name == "Pads" and k == 1 and n == 1, tostring(name))
        name, k, n = P.category(x, 2)
        T.ok(name == "Arp" and k == 2 and n == 2, tostring(name))
        local osc = P.oscillators(x, 1)
        T.ok(#osc == 3 and osc[1].name == "Arp" and not osc[1].default and osc[2].name == "OSC 2" and osc[2].default)
        local tabs = P.macros(x, 2)
        T.ok(#tabs == 2 and tabs[1].buttons[2].name == "Gate" and tabs[2].knobs[1].name == "Macro 4"
          and tabs[2].knobs[1].default and tabs[2].knobs[2].name == "Wobble" and tabs[2].buttons[1].name == "MacroBtn 3")
        local none, why = P.expansion("Omega")
        T.ok(none == nil and why == "the index has no expansion 'Omega'", tostring(why))
        local reads = table.concat(S.presetReads, " ")
        T.ok(reads == "data/index.json data/expansions/alpha.json", reads)
    "##);
}

/// A name read is placed in the catalog as the project's tool places it: the same words (runs of
/// spaces as one, the acute accent as an apostrophe, any case) are said in the catalog's spelling
/// and unmarked; a name only similar enough (0.72) is the database's reading, marked — looked for
/// first in the expansion Avenger was last known in, then in the whole catalog, where it also has
/// to stand out from every other name by 0.05; a name that begins with a catalog name and goes on,
/// and "Init Preset", are not in it. A name in several expansions is placed in the one Avenger was
/// last known in, and otherwise names them, the marker before the list. Preset info says where the
/// preset is, its oscillators and its macros, each part marked.
#[test]
fn preset_info_places_the_name_read_in_the_catalog_and_marks_what_the_database_says() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        local band = "AR Band Pass Slide. Expansion Alpha, category Arp, 1 of 2, from the database. 3 oscillators, "
          .. "from the database: Arp, OSC 2, Lead. Macros, from the database: Tone, Macro 2, Drive; buttons: +12, MacroBtn 2"
        T.ok(T.info(header, "AR Band Pass Slide") == band, T.said())
        T.ok(T.info(header, "ar band pass slide") == band, T.said())
        T.ok(T.info(header, "AR Bond Pess Slide") == "AR Band Pass Slide, from the database"
          .. string.sub(band, #"AR Band Pass Slide" + 1), T.said())
        T.ok(T.count("[avenger] 'Preset info': read 'AR Bond Pess Slide' — Alpha 1 of 6 (by similarity 0.89)") == 1, T.dump())
        T.ok(T.info(header, "LD Palm's Lead 1") == "LD Palm's Lead 1. Expansion Alpha, category Leads, 1 of 1, from the "
          .. "database. 2 oscillators, from the database: Lead, Sub. Macros, from the database: Glide, Macro 2, Macro 3; "
          .. "buttons: MacroBtn 1, MacroBtn 2", T.said())
        T.ok(T.info(header, "AR Deep Arp") == "AR Deep Arp. Expansion Alpha, category Arp, 2 of 2, from the database. "
          .. "1 oscillator, from the database: One Osc. 2 macro tabs, from the database. Tab 1: Cut, Res, Env; buttons: "
          .. "Hold, Gate. Tab 2: Macro 4, Wobble, Macro 6; buttons: MacroBtn 3, MacroBtn 4", T.said())
        T.ok(T.info(header, "PD  Soft Pad") == "PD Soft Pad. Expansion Beta, category Pads, 1 of 1, from the database. "
          .. "1 oscillator, from the database: Soft. Macros, from the database: Air, Macro 2, Macro 3; buttons: "
          .. "MacroBtn 1, MacroBtn 2", T.said())
        T.ok(T.starts(T.info(header, "pl üsch"), "PL Üsch. Expansion Gamma, category Plucked, 1 of 1, from the database"), T.said())
        -- Not in the catalog: somebody's own variant, and Avenger's blank preset.
        T.ok(T.info(header, "BA Deep Sub mine") == "BA Deep Sub mine, not in the database", T.said())
        T.ok(T.count("read 'BA Deep Sub mine' — a name of its own that begins with 'BA Deep Sub'") == 1, T.dump())
        T.ok(T.info(header, "Init Preset") == "Init Preset, not in the database", T.said())
        -- In three expansions, with none known: named, not placed.
        T.ok(T.info(header, "BA Deep Sub") == "BA Deep Sub, in 3 expansions, from the database: Alpha, Beta and Gamma", T.said())
        -- Known to be in Beta: placed there.
        T.info(header, "SQ Feel It")
        T.ok(T.starts(T.info(header, "BA Deep Sub"), "BA Deep Sub. Expansion Beta, category Bass, 1 of 1, from the database. "), T.said())
        -- Nothing read: the position the last read placed, marked.
        T.ok(T.starts(T.info(header, nil), "the preset name cannot be read now; BA Deep Sub, from the "
          .. "database. Expansion Beta, category Bass, 1 of 1, from the database. "), T.said())
        -- One case apart from Alpha's "BA Reese": exact is Gamma's; any case is both.
        T.ok(T.starts(T.info(header, "Ba Reese"), "Ba Reese. Expansion Gamma, category Bass, 1 of 2, from the database. "), T.said())
        T.info(header, "Init Preset")
        T.ok(T.info(header, "BA REESE") == "BA Reese, in 2 expansions, from the database: Alpha and Gamma", T.said())
        T.ok(T.info(header, nil) == "the preset name cannot be read now", T.said())
        -- A name the catalog does not have, as like three of its names as like each: none of them.
        T.ok(T.info(header, "LD Lead 4") == "LD Lead 4, not in the database", T.said())
        T.ok(T.count("[avenger] 'Preset info': read 'LD Lead 4' — not in the catalog: 'LD Lead 1' (0.89) is no "
          .. "closer than another name (0.89)") == 1, T.dump())
        -- Found by similarity, and in three expansions: each part marked.
        T.ok(T.info(header, "BA Dcep Sub") == "BA Deep Sub, from the database, in 3 expansions, from the database: "
          .. "Alpha, Beta and Gamma", T.said())
        -- A poor read like names in two expansions: over the whole catalog, the one it is clearly
        -- closer to; in the expansion Avenger was last known in, that one's, by the threshold alone.
        T.ok(T.starts(T.info(header, "PD Sarm Pad"), "PD Warm Pad, from the database. Expansion Alpha, category Pads, "), T.said())
        T.info(header, "SQ Feel It")
        T.ok(T.starts(T.info(header, "PD Sarm Pad"), "PD Soft Pad, from the database. Expansion Beta, category Pads, "), T.said())
        -- Each expansion's file read once, at its first question.
        local reads = table.concat(S.presetReads, " ")
        T.ok(reads == "data/index.json data/expansions/alpha.json data/expansions/beta.json data/expansions/gamma.json", reads)
    "##);
    run_mac(r##"
        local S = T.S
        -- Gamma's file missing: where the index puts the preset, and no more.
        local files = {}
        for k, v in pairs(T.FIXTURE) do if k ~= "data/expansions/gamma.json" then files[k] = v end end
        T.presets(files)
        local header = T.avengerAt80("PL Üsch")
        T.ok(T.info(header, "PL Üsch") == "PL Üsch. Expansion Gamma, from the database; its categories, oscillators "
          .. "and macros cannot be read from it", T.said())
        T.ok(T.count("[avenger] 'Preset info': Gamma's file cannot be read: data/expansions/gamma.json cannot be read") == 1, T.dump())
    "##);
    run_mac(r##"
        -- A name in seven expansions: the first five named, and how many more.
        local rows = {}
        for k = 1, 7 do
          rows[k] = string.format('{"name":"E%d","file":"data/expansions/e%d.json","presets":["PD Everywhere"]}', k, k)
        end
        T.presets({ ["data/index.json"] = '{"format":1,"exported":"2026-09-26 18:57","factoryStandard":[],'
          .. '"expansions":[' .. table.concat(rows, ",") .. ']}' })
        local header = T.avengerAt80("PD Everywhere")
        T.ok(T.info(header, "PD Everywhere") == "PD Everywhere, in 7 expansions, from the database: E1, E2, E3, E4, E5 "
          .. "and 2 more", T.said())
    "##);
}

/// ◀ and ▶ with the database: the name read just before the click places the preset, and the
/// database's neighbour in its list is what the read-back is compared with. The read-back is said
/// when it is the neighbour (marked when only similar); the neighbour, marked, when nothing could be
/// read — and the next step goes on from it; both when they disagree, the screen's first — and the
/// position is then where the screen says. A read that is another catalog name disagrees, however
/// similar it is to the neighbour (a numbered series). At an expansion's first or last preset there
/// is no neighbour: Avenger goes on into the user's own next expansion, which the catalog cannot
/// know. Nothing is said before the watch is over, one thing per step.
#[test]
fn previous_and_next_say_what_was_read_and_the_databases_neighbour_marked_when_it_differs() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- The same words as the neighbour: said, unmarked.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Deep Arp") == "AR Deep Arp", T.said())
        T.ok(T.count("[avenger] 'Next preset': the database's neighbour: 'AR Deep Arp' (Alpha 2 of 6)") == 1, T.dump())
        -- Similar to the neighbour: the neighbour's spelling, marked.
        T.ok(T.step(header, 3, "AR Deep Arp", "BA Dcep Sub") == "BA Deep Sub, from the database", T.said())
        T.ok(T.count("[avenger] 'Next preset': read 'BA Dcep Sub', the database's neighbour by similarity 0.91") == 1, T.dump())
        -- Nothing read, before or after: the neighbour of the last position, marked.
        T.ok(T.step(header, 3, nil, nil) == "BA Reese, from the database", T.said())
        -- ... and the step after that goes on from it.
        T.ok(T.step(header, 3, nil, "PD Warm Pad") == "PD Warm Pad", T.said())
        -- Another name read (Avenger's own search open, say): the screen's name, then the database's.
        T.ok(T.step(header, 3, "PD Warm Pad", "PD Soft Pad") == "PD Soft Pad; LD Palm's Lead 1, from the database", T.said())
        -- The screen wins: the position is Beta's PD Soft Pad now.
        T.ok(T.step(header, 2, "PD Soft Pad", "SQ Feel It") == "SQ Feel It", T.said())
        T.ok(T.step(header, 2, "SQ Feel It", "BA Deep Sub") == "BA Deep Sub", T.said())
        -- Beta's first preset: no neighbour before it, so the step is as without the database.
        T.ok(T.step(header, 2, "BA Deep Sub", "My Own Preset") == "My Own Preset", T.said())
        T.ok(T.count("the database's neighbour: none, BA Deep Sub is at the start of its expansion") == 1, T.dump())
        -- Unchanged on screen: said so, and the neighbour the database expected.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Band Pass Slide")
          == "AR Band Pass Slide, unchanged; AR Deep Arp, from the database", T.said())
        -- A name the catalog does not have: as read, and the neighbour.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "Zzz Custom Thing") == "Zzz Custom Thing; AR Deep Arp, from the database", T.said())
        -- ... and with no position after it, the next step has no neighbour to add.
        T.ok(T.step(header, 3, nil, nil) == "the preset name cannot be read now", T.said())
        -- Another catalog name, however similar to the neighbour, is that name: in a numbered
        -- series, LD Lead 3 read where LD Lead 1 was expected.
        T.ok(T.step(header, 2, "LD Lead 2", "LD Lead 3") == "LD Lead 3; LD Lead 1, from the database", T.said())
        -- A poor read of the neighbour is the neighbour, marked.
        T.ok(T.step(header, 2, "LD Lead 3", "LD Laed 2") == "LD Lead 2, from the database", T.said())
    "##);
}

/// Where the screen and the database's neighbour disagree, the screen's words are said: a name read
/// unchanged is not the neighbour, however like it; nor is a poor read closer to another name than
/// to the neighbour, or somebody's own variant of the neighbour's name — and a read the catalog has
/// only by similarity is said as read then, with the neighbour after it, and places nothing. At an
/// expansion's last preset there is no neighbour, and a step from there that reads nothing leaves
/// no position behind; read unchanged, the position stays.
#[test]
fn a_step_says_the_screens_words_where_they_disagree_with_the_databases_neighbour() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("LD Laed 1")
        -- A swallowed click, the name read poorly the same way before and after: placed on LD Lead 1
        -- before the click, and the screen then says it did not move — LD Lead 2, as like the read
        -- as it is, is only what the database expected.
        T.ok(T.step(header, 3, "LD Laed 1", "LD Laed 1") == "LD Laed 1, unchanged; LD Lead 2, from the database", T.said())
        T.ok(T.count("[avenger] 'Next preset': read 'LD Laed 1', as before the click — Delta 1 of 3 (by similarity "
          .. "0.89, said as read); not the database's neighbour 'LD Lead 2'") == 1, T.dump())
        -- Even when the unchanged read is as like the neighbour as like the name it was placed on
        -- (in the known expansion the first of equals, LD Lead 1).
        T.ok(T.step(header, 3, "LD Lead 4", "LD Lead 4") == "LD Lead 4, unchanged; LD Lead 2, from the database", T.said())
        -- The name after the neighbour, read poorly: closer to LD Lead 3 than to the expected LD Lead
        -- 1 — as read, then the neighbour; and no position after it.
        T.ok(T.step(header, 2, "LD Lead 2", "LD Laed 3") == "LD Laed 3; LD Lead 1, from the database", T.said())
        T.ok(T.step(header, 2, nil, nil) == "the preset name cannot be read now", T.said())
        -- Somebody's own variant of the neighbour's name is not the neighbour.
        T.ok(T.step(header, 3, "AR Deep Arp", "BA Deep Sub mine") == "BA Deep Sub mine; BA Deep Sub, from the database", T.said())
        -- Alpha's last preset: no neighbour after it, and a click from there that reads nothing
        -- leaves no position — Avenger is in an expansion the catalog cannot name.
        T.ok(T.step(header, 3, "LD Palm's Lead 1", nil) == "the preset name cannot be read now", T.said())
        T.ok(T.count("the database's neighbour: none, LD Palm's Lead 1 is at the end of its expansion") == 1, T.dump())
        T.ok(T.step(header, 2, nil, nil) == "the preset name cannot be read now", T.said())
        -- Read unchanged there, it stays where it was: the step back has its neighbour.
        T.ok(T.step(header, 3, "LD Palm's Lead 1", "LD Palm's Lead 1") == "LD Palm's Lead 1, unchanged", T.said())
        T.ok(T.step(header, 2, nil, "PD Warm Pad") == "PD Warm Pad", T.said())
        T.ok(T.count("read 'PD Warm Pad', the database's neighbour exactly") == 1, T.dump())
    "##);
}

/// The position is forgotten whenever the preset may have changed where the module does not read
/// it: a MENU item chosen, the redo list opened, the overlay leaving the front. A step overtaken
/// by another press decides nothing: its neighbour is never said, and the position is the later
/// step's — and a press made before the step before it has seen its name change has no neighbour
/// when it reads the name that step started from. After Save preset as and Load preset the name
/// is the user's own until a step shows another, and is not looked for by similarity.
#[test]
fn the_databases_position_is_forgotten_when_the_preset_may_have_changed() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header, box, W = T.avengerAt80("AR Band Pass Slide")
        T.info(header, "AR Deep Arp")
        T.choose(header, 5, 490, 116, 192, 96)
        T.ok(T.count("[avenger] the database's position (Alpha 2 of 6) is forgotten: Load preset chosen") == 1, T.dump())
        T.ok(T.info(header, nil) == "the preset name cannot be read now", T.said())
        T.info(header, "AR Deep Arp")
        T.pressAt(header, 10)
        T.ok(T.lastSaid() == "Redo list, activated", T.said())
        T.ok(T.count("is forgotten: the redo list opened") == 1, T.dump())
        T.run(3000)
        T.info(header, "AR Deep Arp")
        T.show(T.OTHER)
        T.ok(not header.active)
        T.ok(T.count("is forgotten: the overlay left the front") == 1, T.dump())
        T.show(W)
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.ok(header.active)
        T.ok(T.info(header, nil) == "the preset name cannot be read now", T.said())
    "##);
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- ▶ twice before the first name is read back: the first step's neighbour is AR Deep Arp,
        -- the second's BA Deep Sub.
        T.nameIs("AR Band Pass Slide")
        T.pressAt(header, 3)
        T.run(150)
        T.nameIs("AR Deep Arp")
        T.pressAt(header, 3)
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, T.nameRead("BA Deep Sub")) end
        end
        T.ok(string.find(T.said(), "BA Deep Sub", 1, true) ~= nil, T.said())
        T.ok(string.find(T.said(), "AR Deep Arp, from the database", 1, true) == nil, "the overtaken step's neighbour: " .. T.said())
        T.ok(T.starts(T.info(header, nil), "the preset name cannot be read now; BA Deep Sub, from the database. Expansion Alpha, "), T.said())
    "##);
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- ▶ twice before the name has changed on screen, the second press reading the name from
        -- before the first click: a neighbour of that would be one preset behind, so there is none,
        -- and the step says what it reads.
        T.nameIs("AR Band Pass Slide")
        T.pressAt(header, 3)
        T.run(150)
        T.pressAt(header, 3)
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, T.nameRead("BA Deep Sub")) end
        end
        T.ok(T.count("[avenger] 'Next preset': the step before has not settled, and the name may still be the one "
          .. "from before its click — no neighbour") == 1, T.dump())
        T.ok(string.find(T.said(), "BA Deep Sub", 1, true) ~= nil, T.said())
        T.ok(string.find(T.said(), "from the database", 1, true) == nil, T.said())
        -- Settled, the next step has its neighbour again.
        T.ok(T.step(header, 3, "AR Deep Arp", "BA Deep Sub") == "BA Deep Sub", T.said())
        T.ok(T.count("read 'BA Deep Sub', the database's neighbour exactly") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- After Save preset as, Avenger shows the name the user typed: a catalog name like it is a
        -- coincidence, so it is looked up as the same words only — until a step shows another name.
        T.choose(header, 7, 490, 116, 192, 96)
        T.ok(T.info(header, "AR Bond Pess Slide") == "AR Bond Pess Slide, not in the database", T.said())
        T.ok(T.count("read 'AR Bond Pess Slide' — not in the catalog as it stands — after Load or Save as, not "
          .. "looked for by similarity") == 1, T.dump())
        T.ok(T.step(header, 3, "AR Bond Pess Slide", "AR Bond Pess Slide") == "AR Bond Pess Slide, unchanged", T.said())
        T.ok(T.step(header, 3, "AR Bond Pess Slide", "AR Deap Arp") == "AR Deep Arp, from the database", T.said())
        T.ok(T.starts(T.info(header, "AR Bond Pess Slide"), "AR Band Pass Slide, from the database. Expansion Alpha, "), T.said())
        -- Load preset alike.
        T.choose(header, 5, 490, 116, 192, 96)
        T.ok(T.count("is forgotten: Load preset chosen") == 1, T.dump())
        T.ok(T.info(header, "AR Bond Pess Slide") == "AR Bond Pess Slide, not in the database", T.said())
    "##);
}

/// A step waits for the preset's name before its click, and only its module waits: another program
/// brought to the front meanwhile takes the press away — nothing is clicked or said in it, and the
/// log says why.
#[test]
fn a_step_whose_window_left_the_front_during_its_name_read_clicks_nothing() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local said, clicks = #S.speech, #S.clicks
        S.ocr = function(g)
          if T.region(g) ~= "152,95,440,119" then return "" end
          -- Another program comes to the front while the name is being read.
          S.front, S.chain = T.OTHER, { T.OTHER }
          T.turn()
          return "Init Preset"
        end
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks, "a click in the other program: " .. tostring(T.clickAt()))
        T.ok(#S.speech == said, "something said: " .. T.said(said + 1))
        T.ok(T.count("[avenger] 'Next preset': not going on after the name was read — the overlay is on another "
          .. "window now") == 1, T.dump())
    "##);
}
