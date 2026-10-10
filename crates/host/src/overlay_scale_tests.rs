//! The overlay runtime's coordinate scaling (`O:scale`), run for real against the scripted host of
//! `overlay_menu_tests.rs`.
//!
//! Written for the maintainer's decisions of 2026-09-28, the first stage of VPS Avenger: a
//! plug-in that zooms its whole interface from 50 to 200 % moves every control with the zoom, so
//! the runtime scales an overlay's authored coordinates — every one of them, through one formula,
//!
//!     screen = origin + frame + k * (authored - about), rounded once, half up
//!
//! — for the overlay that asks, and for no other: an overlay without `O:scale` is placed exactly as
//! it was, fractions and all. A factor the module cannot tell yet places nothing and says so; a
//! `rawOrigin` control is neither framed nor scaled; a region may be a function.
//!
//! And for the review that followed: every kind of coordinate the documentation says is scaled —
//! a slider's track and step, a graphical button's click offset and fixed drag, a reveal probe, a
//! captured region, `fromRight` beside an `about`, a landmark-anchored overlay — has a scenario of
//! its own, and so do a scale set after binding and a sequence whose factor goes half-way.
//!
//! And ARC ON:EAR, which scaled by hand until then, moved onto it with its behaviour on Windows
//! unchanged: the module is loaded for real (`T.source`, as the host compiles a module's files)
//! and every point it clicks, over a grid of window sizes and places, is held against the formula
//! its own geometry used before — to the pixel — with a few of those points written out as
//! numbers, so the two cannot drift together unnoticed.
//!
//! And for the step that readies the tree for reads of the screen that wait (2026-10-10): a
//! calibration shot reads the pixels under its crosshairs in one read, ON:EAR's Close clicks only
//! while its panel is still where its pixel was read, and its "Show all" reads its rows in one read.

use std::path::PathBuf;

use mlua::{Function, Lua, Table};

use super::overlay_menu_tests::{finish, harness, RUNTIME};

/// The helpers, added to the harness's `T`: input, OCR and pixels that record where they were
/// asked, and a window that owns every point unless a scenario says otherwise.
const SC: &str = r##"
local S = T.S
T.autoFocusReads()
S.clicks = {}
S.moves = {}
S.recognized = {}
S.pixelsAt = {}
S.owns = true
S.ocr = nil     -- function(region) -> text, for host.ocr.recognize
S.dump = {}     -- host.element.rawDump(id)

rawset(T.host, "input", T.strict("host.input", {
  click = function(x, y, opts)
    S.clicks[#S.clicks + 1] = { x, y, button = opts and opts.button }
    S.order[#S.order + 1] = "click"
  end,
  move = function(x, y) S.moves[#S.moves + 1] = { x, y } end,
  scroll = function() end,
  drag = function() end,
}))
rawset(T.host.window, "ownsPoint", function(id, x, y)
  if type(S.owns) == "function" then return S.owns(id, x, y) end
  return S.owns
end)
-- host.ocr.recognize without a callback, answered at once with what S.ocr says is written there:
-- a region or an entry, checked as the host checks it (T.answerWaits).
T.answerWaits(function(what)
  local r = what.region or what
  S.recognized[#S.recognized + 1] = { r[1], r[2], r[3], r[4] }
  local text = S.ocr and S.ocr(r) or ""
  return { text = text, words = {}, skipped = false, status = text == "" and "none" or "text" }
end)
rawset(T.host.screen, "pixel", function(x, y)
  S.pixels += 1
  S.pixelsAt[#S.pixelsAt + 1] = { x, y }
  return S.pixel
end)
rawset(T.host.element, "rawDump", function() return S.dump end)
-- The calibration shot's pictures, with where their crosshairs are.
rawset(T.host.screen, "saveMarked", function(path, opts)
  S.order[#S.order + 1] = "save"
  local marks = {}
  for _, m in ipairs(opts.marks or {}) do marks[#marks + 1] = { m.x, m.y } end
  S.shots[#S.shots + 1] = { path = path, region = opts.region, snapshot = opts.snapshot, marks = marks,
    at = S.now }
  return true
end)
rawset(T.host.window, "focusChain", function() return { S.focus } end)

-- Whether a log line is exactly `line`.
function T.logged(line)
  for _, l in ipairs(S.logs) do
    if l == line then return true end
  end
  return false
end

-- The last click, as "x,y".
function T.lastClick()
  local c = S.clicks[#S.clicks]
  return c and (c[1] .. "," .. c[2]) or "none"
end

-- Speech from entry `from` on, joined.
function T.said(from)
  local out = {}
  for i = from or 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

-- An overlay over S.origin (client 100,50 800x600) with the given controls, in front, unbound.
function T.scaled(build, factor, opts)
  local o = T.O.new("Synth")
  if factor ~= false then o:scale(factor or function() return S.k end, opts) end
  build(o)
  T.front(o)
  return o
end

-- What the module files need of the host beyond the menu tests' script: its dependencies and its
-- own files, each evaluated once, as the host evaluates them once per VM. `root` is its folder.
function T.module(root)
  local H = T.host
  rawset(H, "require", function(id)
    if id == "com.platform.overlay" then return T.O end
    error("the scripted host has no module " .. id)
  end)
  local included = {}
  rawset(H, "include", function(rel)
    local key = root .. rel
    if included[key] == nil then included[key] = { T.source(key)(H) } end
    return included[key][1]
  end)
  return T.source(root .. "src/main.luau")(H)
end

-- Every overlay the runtime makes from here on, in order.
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
"##;

/// A fresh VM with the scripted host, the runtime and the helpers above; `T.source(path)` compiles
/// a file of this repository the way the host compiles a module's code.
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
    if let Err(e) = lua.load(SC).set_name("scale helpers").exec() {
        panic!("{e}");
    }
    if let Err(e) = lua.load(scenario).set_name("scenario").exec() {
        panic!("{e}");
    }
    finish(&lua);
}

fn run(scenario: &str) {
    run_on("windows", scenario)
}

// ---------------------------------------------------------------------------------------------
// One formula, for every authored coordinate.
// ---------------------------------------------------------------------------------------------

/// A hotspot of a scaled overlay clicks origin + frame + k * (at - about), rounded once, half up;
/// `fromRight` scales its distance from the content's right edge; `rawOrigin` is neither framed
/// nor scaled; a `points` sequence is placed step by step.
#[test]
fn a_hotspot_is_placed_through_the_frame_and_the_scale() {
    run(r##"
        local S = T.S
        S.k = 2
        local o = T.scaled(function(o)
          o:addHotspotButton({ label = "Plain", at = { 10, 20 } })
          o:addHotspotButton({ label = "Right", at = { 10, 20 }, fromRight = true })
          o:addHotspotButton({ label = "Raw", at = { 10, 20 }, rawOrigin = true })
          o:addHotspotButton({ label = "Steps", points = { { 10, 20 }, { 30, 40 } }, settle = 100 })
          o:addHotspotButton({ label = "Half", at = { 3, 5 } })
        end)
        o:activate(1)
        assert(T.lastClick() == "120,90", "100 + 2*10, 50 + 2*20: " .. T.lastClick())
        o:activate(2)
        assert(T.lastClick() == "880,90", "100 + 800 - 2*10: " .. T.lastClick())
        o:activate(3)
        assert(T.lastClick() == "110,70", "rawOrigin: the origin's own pixels: " .. T.lastClick())
        o:activate(4)
        assert(T.lastClick() == "120,90", T.lastClick())
        S.now += 100
        T.runDue()
        assert(T.lastClick() == "160,130", "the second step, scaled: " .. T.lastClick())
        S.k = 1.5
        o:activate(5)
        assert(T.lastClick() == "105,58", "104.5 and 57.5 round half up: " .. T.lastClick())
        assert(T.count("[scale] 'Synth': factor 2.0000 about (0,0)") == 1, T.dump())
        assert(T.count("[scale] 'Synth': factor 1.5000 about (0,0)") == 1, "said when it changes: " .. T.dump())
    "##);
}

/// The frame is unscaled and composes first; `about` is the authored point the frame puts in
/// place. ON:EAR's model, in those terms: the frame is half the content's width, `about` the
/// design's centre line.
#[test]
fn the_frame_is_unscaled_and_about_is_the_point_it_places() {
    run(r##"
        local S = T.S
        S.k = 2
        local o = T.O.new("Synth")
        o:frame(function() return 3, 4 end)
        o:scale(function() return S.k end, { about = { 100, 10 } })
        o:addHotspotButton({ label = "At about", at = { 100, 10 } })
        o:addHotspotButton({ label = "Beside it", at = { 110, 30 } })
        o:addHotspotButton({ label = "Raw", at = { 110, 30 }, rawOrigin = true })
        T.front(o)
        o:activate(1)
        assert(T.lastClick() == "103,54", "`about` lands on origin + frame: " .. T.lastClick())
        o:activate(2)
        assert(T.lastClick() == "123,94", "103 + 2*10, 54 + 2*20: " .. T.lastClick())
        o:activate(3)
        assert(T.lastClick() == "210,80", "rawOrigin skips both: " .. T.lastClick())
        local x, y = o:toScreen(110, 30)
        assert(x == 123 and y == 94, "toScreen is the same point: " .. tostring(x) .. "," .. tostring(y))
        local rx, ry = o:toScreen(110, 30, { rawOrigin = true })
        assert(rx == 210 and ry == 80)
        local r = o:toScreenRect({ 100, 10, 110, 30 })
        assert(r[1] == 103 and r[2] == 54 and r[3] == 123 and r[4] == 94, table.concat(r, ","))
    "##);
}

/// No factor now: nothing is clicked, the control says it is not available, and the log says why
/// — once per change of the factor, and once per press. A factor that raises or is not a number
/// above 0 is no factor either.
#[test]
fn no_factor_places_nothing_and_says_so() {
    run(r##"
        local S = T.S
        S.k = nil
        local o = T.scaled(function(o)
          o:addHotspotButton({ label = "Undo", at = { 305, 14 } })
          o:addHotspotToggle({ label = "Mute", at = { 20, 20 }, onColor = { 255, 255, 255 }, offColor = { 0, 0, 0 } })
          o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 } })
        end)
        o:activate(1)
        assert(#S.clicks == 0, "no click without a factor")
        assert(T.said() == "Undo is not available now", T.said())
        assert(T.count("[scale] 'Synth': no factor now — its scale function answered nil") == 1, T.dump())
        assert(T.count("[place] 'Synth': 'Undo' cannot be placed now — its scale function answered nil; nothing clicked") == 1, T.dump())
        o:activate(2)
        assert(#S.clicks == 0 and S.speech[#S.speech].text == "Mute is not available now", T.said())
        -- The OCR control: not read, and said so instead of "no text"; its click not made either.
        local from = #S.speech + 1
        o:activate(3)
        assert(#S.focusReads == 0 and #S.clicks == 0, "nothing read, nothing clicked")
        assert(T.said(from) == "Preset, button, cannot be read now", T.said(from))
        assert(T.count("[read] 'Preset' not read: its scale function answered nil") == 1, T.dump())
        assert(T.count("[scale] 'Synth': no factor now") == 1, "the state is said once, not per control: " .. T.dump())
        -- A raising factor and one that is not a factor.
        S.k = nil
        o._scale.fn = function() error("no caption") end
        o:activate(1)
        assert(#S.clicks == 0 and T.count("its scale function raised") >= 1, T.dump())
        o._scale.fn = function() return 0 end
        o:activate(1)
        assert(#S.clicks == 0 and T.count("answered 0, not a factor above 0") >= 1, T.dump())
        o._scale.fn = function() return 2 end
        o:activate(1)
        assert(T.lastClick() == "710,78", "and a factor again places it: " .. T.lastClick())
    "##);
}

/// Regions scale corner by corner, may be functions of the overlay, and a function that answers
/// nothing reads nothing. `ocrLabel` likewise; the name falls back to the label. The click of an
/// OCR button is the middle of the region it read, made once its read is answered. The reads are
/// host.ocr.recognize's, answered by T.deliver as the host answers them, on a later turn of the loop.
#[test]
fn regions_and_name_regions_scale_and_may_be_functions() {
    run(r##"
        local S = T.S
        S.k = 2
        S.where = { 34, 7, 214, 22 }
        S.ocr = function(r) if r[1] == 168 then return "Init Preset" end return "Name" end
        local o = T.scaled(function(o)
          o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true })
          o:addOCRButton({ label = "Moving", region = function() return S.where end })
          o:addHotspotButton({ label = "Named", at = { 10, 20 }, ocrLabel = { 30, 40, 50, 50 } })
        end)
        o:activate(1)
        local r = S.focusReads[1].regions[1]
        assert(r[1] == 168 and r[2] == 64 and r[3] == 528 and r[4] == 94, "every corner scaled: "
          .. table.concat(r, ","))
        T.deliver()
        assert(S.speech[#S.speech].text == "Preset, Init Preset", S.speech[#S.speech].text)
        assert(#S.clicks == 0, "readOnly: never clicked")
        assert(T.count('[read] \'Preset\' = "Init Preset" (region 34,7 180x15, 0 ms, 1 word, en-US) at 168,64 360x30 on screen; spoken') == 1, T.dump())
        o:activate(2)
        assert(#S.clicks == 0, "the click waits for the read's answer")
        T.deliver()
        assert(T.lastClick() == "348,79", "the middle of 168,64-528,94: " .. T.lastClick())
        S.where = nil
        local clicks = #S.clicks
        o:activate(2)
        assert(#S.clicks == clicks, "a region function with no answer: nothing clicked")
        assert(S.speech[#S.speech].text == "Moving, button, cannot be read now", T.said())
        assert(T.count("[read] 'Moving' not read: its `region` function answered nil") == 1, T.dump())
        -- A name read from a scaled region, on arrival.
        o.focus = 2
        T.tab()
        local last = S.focusReads[#S.focusReads].regions[1]
        assert(last[1] == 160 and last[2] == 130 and last[3] == 200 and last[4] == 150, table.concat(last, ","))
        T.deliver()
        assert(S.speech[#S.speech].text == "Name, button", S.speech[#S.speech].text)
    "##);
}

/// A tab's point, a hotspot toggle's probe and a region's hover point go through the same
/// formula; a tab that cannot be placed now is not "selected" and stays where it was.
#[test]
fn tabs_toggles_and_hover_points_scale_too() {
    run(r##"
        local S = T.S
        S.k = 2
        local tabs
        local o = T.scaled(function(o)
          tabs = o:addTabControl({ label = "Sections", tabs = { { label = "Osc", at = { 10, 20 } },
            { label = "Macros", at = { 40, 20 } } } })
          o:addHotspotToggle({ label = "Mute", at = { 20, 20 }, onColor = { 255, 255, 255 },
            offColor = { 0, 0, 0 } })
          o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true })
        end)
        o.hoverToRead = true
        o.focus = 1
        o:_syncNativeKeys()
        assert(S.holding["Right"], "the focused tab control holds Right")
        S.captured["Right"]()
        assert(T.lastClick() == "180,90", "tab 2 at 100 + 2*40: " .. T.lastClick())
        assert(tabs.control._current == 2 and S.speech[#S.speech].text == "Macros tab selected", T.said())
        S.k = nil
        local clicks = #S.clicks
        S.captured["Right"]()
        assert(#S.clicks == clicks and tabs.control._current == 2, "no factor: no click, and still tab 2")
        assert(S.speech[#S.speech].text == "Osc tab is not available now", T.said())
        S.k = 2
        -- The toggle's state is read at its placed point.
        local n = #S.pixelsAt
        o.focus = 1
        T.tab()
        local p = S.pixelsAt[n + 1]
        assert(p and p[1] == 140 and p[2] == 90, "the probe at 100 + 2*20: " .. (p and (p[1] .. "," .. p[2]) or "none"))
        -- hoverToRead moves to the middle of the placed region.
        T.tab()
        local m = S.moves[#S.moves]
        assert(m and m[1] == 348 and m[2] == 79, "hover at the middle: " .. (m and (m[1] .. "," .. m[2]) or "none"))
    "##);
}

/// An overlay that never calls O:scale is placed exactly as before: no rounding, a fractional
/// frame included, and toScreen answers the plain sum. A region read with such a frame covers the
/// pixels the read on the event loop covered: its corners cut toward zero, which that call's loose
/// reading did, and which host.ocr.recognize, reading corners strictly, would otherwise refuse.
#[test]
fn an_overlay_without_a_scale_is_placed_exactly_as_before() {
    run(r##"
        local S = T.S
        local o = T.scaled(function(o)
          o:frame(function() return 0.5, 0.25 end)
          o:addHotspotButton({ label = "Play", at = { 10, 20 } })
          o:addOCRButton({ label = "Value", region = { 11, 21, 30, 40 } })
        end, false)
        o:activate(1)
        local c = S.clicks[1]
        assert(c[1] == 110.5 and c[2] == 70.25, "no rounding without a scale: " .. c[1] .. "," .. c[2])
        o:activate(2)
        local r = S.focusReads[1].regions[1]
        assert(r[1] == 111 and r[2] == 71 and r[3] == 130 and r[4] == 90, table.concat(r, ","))
        T.deliver()
        c = S.clicks[2]
        assert(c[1] == 120.5 and c[2] == 80.25, "the old centre, origin + floor(middle): " .. c[1] .. "," .. c[2])
        local x, y = o:toScreen(10, 20)
        assert(x == 110.5 and y == 70.25)
        assert(T.count("[scale]") == 0 and T.count("[place]") == 0, T.dump())
        assert(T.logged('[read] \'Value\' = "" (region 11,21 19x19, 0 ms, 0 words, en-US): the recogniser read nothing; spoken'),
          "the read line as it always was, with what became of the read: " .. T.dump())
        assert(S.speech[#S.speech].text == "Value, button, no text", T.said())
    "##);
}

/// The calibration shot of a scaled overlay marks where it clicks and says the factor, the frame
/// and every region on screen; an unscaled one's lines are what they were.
#[test]
fn the_calibration_shot_of_a_scaled_overlay_says_its_factor_and_regions() {
    run(r##"
        local S = T.S
        S.k = 2
        local o = T.scaled(function(o)
          o:addHotspotButton({ label = "Undo", at = { 10, 20 } })
          o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true })
        end)
        assert(o:calibrationShot() == true)
        local marks = S.shots[1].marks
        assert(marks[1][1] == 120 and marks[1][2] == 90, "the hotspot's crosshair")
        assert(marks[2][1] == 348 and marks[2][2] == 79, "the region's middle")
        assert(T.count("scale 2.0000 about (0,0), frame (0,0)") == 1, T.dump())
        assert(T.logged("[calibrate]" .. string.format("  %2d %-24s screen (%d,%d)  pixel %s%s%s", 2, "Preset",
          348, 79, "20,20,20", "", "  region (168,64)-(528,94)")), T.dump())
        local plain = T.scaled(function(p)
          p:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true })
        end, false)
        local lines = #S.logs
        assert(plain:calibrationShot() == true)
        assert(T.logged("[calibrate]" .. string.format("  %2d %-24s screen (%d,%d)  pixel %s", 1, "Preset",
          224, 64, "20,20,20")), "the line as it always was: " .. T.dump())
        assert(T.count("region (", lines) == 0 and T.count("scale", lines) == 0, T.dump())
    "##);
}

/// A calibration shot reads the pixel under every crosshair — the controls' and a menu opener's —
/// in one read of the screen, where it read one pixel per crosshair, and each line names the pixel
/// under its own crosshair. A read that fails leaves every pixel "?", and says why first.
#[test]
fn a_calibration_shot_reads_every_crosshairs_pixel_in_one_read() {
    run(r##"
        local S = T.S
        local o = T.scaled(function(o)
          o:addHotspotButton({ label = "Undo", at = { 10, 20 } })
          o:addHotspotButton({ label = "Hidden", at = { 50, 60 }, when = function() return false end })
          o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true })
        end, false)
        o._pickControls = { Zoom = { kind = "pick", label = "Zoom", at = { 21, 14 } } }
        local asked, fails = {}, nil
        rawset(T.host.screen, "pixels", function(points)
          asked[#asked + 1] = points
          if fails then return nil, fails end
          local out = {}
          for i in ipairs(points) do out[i] = { r = i, g = 2 * i, b = 3 * i } end
          return out
        end)
        assert(o:calibrationShot() == true)
        assert(#asked == 1 and S.pixels == 0, "one read, and no pixel read on its own: " .. #asked .. ", " .. S.pixels)
        local pts = {}
        for _, p in ipairs(asked[1]) do pts[#pts + 1] = p[1] .. "," .. p[2] end
        assert(table.concat(pts, " ") == "110,70 224,64 121,64", table.concat(pts, " "))
        local function line(n, label, x, y, px, tail)
          return "[calibrate]" .. string.format("  %2d %-24s screen (%d,%d)  pixel %s", n, label, x, y, px) .. (tail or "")
        end
        assert(T.logged(line(1, "Undo", 110, 70, "1,2,3")), T.dump())
        assert(T.logged(line(2, "Preset", 224, 64, "2,4,6")), T.dump())
        assert(T.logged(line(3, "Zoom", 121, 64, "3,6,9", "  [menu opener]")), T.dump())
        assert(T.logged("[calibrate]" .. string.format("  -- %-24s hidden (its `when` is false)", "Hidden")), T.dump())
        local from = #S.logs + 1
        fails = "the screen capture failed"
        assert(o:calibrationShot() == true)
        assert(T.count("[calibrate]  the pixels could not be read: the screen capture failed", from) == 1, T.dump())
        assert(T.logged(line(1, "Undo", 110, 70, "?")) and T.logged(line(3, "Zoom", 121, 64, "?", "  [menu opener]")),
          T.dump())
        assert(T.count("  pixel ?", from) == 3, T.dump())
    "##);
}

/// The declaration report names a region that is neither corners nor a function, and a button
/// the click would read as the left one.
#[test]
fn a_malformed_region_or_button_is_reported_at_bind() {
    run(r##"
        local o = T.O.new("Synth")
        o:addOCRButton({ label = "Preset", region = { 1, 2, 3 } })
        o:addHotspotButton({ label = "Redo", at = { 1, 2 }, button = "secondary" })
        o:attach({ class = "nothing" }, {})
        assert(T.count("`region` must be {x1, y1, x2, y2} or a function(overlay) answering that") == 1, T.dump())
        assert(T.count("`button` is \"secondary\"") == 1, T.dump())
        local ok = pcall(function() T.O.new("x"):scale(2) end)
        assert(not ok, "scale takes a function")
        ok = pcall(function() T.O.new("x"):scale(function() return 1 end, { about = 5 }) end)
        assert(not ok, "about is {x, y}")
    "##);
}

/// A slider's `from` and `to` are x coordinates like any other, and one arrow-key step is one per
/// cent of the track AS DRAWN: at a factor of 2 a track authored 100 wide is 200 on screen, a step
/// of 2. Its region is searched where it is placed. With no factor the slider says it is not
/// available now — not "not found", which would describe a search never made. Without a scale the
/// step is the authored one and the region the plain sum.
#[test]
fn a_slider_scales_its_track_and_its_step() {
    run(r##"
        local S = T.S
        S.k = 2
        S.thumbX = 170
        S.drags = {}
        rawset(T.host.input, "drag", function(x1, y1, x2, y2) S.drags[#S.drags + 1] = { x1, y1, x2, y2 } end)
        rawset(T.host.screen, "imageSearchMulti", function(_, opts)
          S.searched = opts.region
          return 1, { x = S.thumbX - 5, y = 85, w = 10, h = 10 }
        end)
        local o = T.scaled(function(o)
          o:addSlider({ label = "Mix", region = { 10, 10, 110, 30 }, thumbImage = "thumb.png", from = 10, to = 110 })
        end)
        o.focus = 1
        o:adjust(1)
        local r = S.searched
        assert(r[1] == 120 and r[2] == 70 and r[3] == 320 and r[4] == 110, "the region placed: " .. table.concat(r, ","))
        local d = S.drags[1]
        assert(d and d[1] == 170 and d[3] == 172, "a step of 2, 1 % of 120..320: " .. (d and table.concat(d, ",") or "none"))
        S.thumbX = 172
        S.now += 25
        T.runDue()
        assert(S.speech[#S.speech].text == "26 percent", "(172 - 120) / 200: " .. T.said())
        -- No factor: said as a control that cannot be placed, nothing searched or dragged.
        S.k = nil
        S.searched = nil
        o:adjust(1)
        assert(S.searched == nil and #S.drags == 1, "nothing searched, nothing dragged")
        assert(S.speech[#S.speech].text == "Mix is not available now", T.said())
        -- Without a scale: the authored step, the region the plain sum.
        local p = T.scaled(function(p)
          p:addSlider({ label = "Mix", region = { 10, 10, 110, 30 }, thumbImage = "thumb.png", from = 10, to = 310 })
        end, false)
        S.thumbX = 150
        p.focus = 1
        p:adjust(-1)
        r = S.searched
        assert(r[1] == 110 and r[3] == 210, table.concat(r, ","))
        d = S.drags[2]
        assert(d[1] == 150 and d[3] == 147, "a step of ceil(300 / 100) = 3, authored: " .. table.concat(d, ","))
    "##);
}

/// A graphical button's `clickOffset` and a fixed `dragBy` are distances from where the template
/// was found: scaled, never framed, and not moved by `about`. A `dragBy` function's answer is in
/// screen pixels already and is not scaled. A factor gone by the time the search answers is said.
/// Without a scale both are the plain sums they always were.
#[test]
fn a_graphical_buttons_click_offset_and_fixed_drag_are_scaled_distances() {
    run(r##"
        local S = T.S
        S.k = 2
        S.drags = {}
        rawset(T.host.input, "drag", function(x1, y1, x2, y2) S.drags[#S.drags + 1] = { x1, y1, x2, y2 } end)
        rawset(T.host.screen, "imageSearchAsync", function(_, opts, cb) S.search = { region = opts.region, cb = cb } end)
        local o = T.O.new("Synth")
        o:frame(function() return 3, 4 end)
        o:scale(function() return S.k end, { about = { 50, 50 } })
        o:addGraphicalButton({ label = "Fold", image = "fold.png", region = { 50, 50, 250, 150 }, clickOffset = { 10, 4 } })
        o:addGraphicalButton({ label = "Grip", image = "grip.png", dragBy = { 5, 0 } })
        o:addGraphicalButton({ label = "Handle", image = "grip.png", dragBy = function() return 7, 0 end })
        T.front(o)
        o:activate(1)
        local r = S.search.region
        assert(r[1] == 103 and r[2] == 54 and r[3] == 503 and r[4] == 254, "the region, framed and scaled about (50,50): " .. table.concat(r, ","))
        S.search.cb({ x = 200, y = 100, w = 20, h = 10 })
        assert(T.lastClick() == "220,108", "200 + 2*10, 100 + 2*4 — no frame, no about: " .. T.lastClick())
        o:activate(2)
        S.search.cb({ x = 200, y = 100, w = 20, h = 10 })
        local d = S.drags[1]
        assert(d[1] == 210 and d[2] == 105 and d[3] == 220 and d[4] == 105, "a drag of 2*5: " .. table.concat(d, ","))
        o:activate(3)
        S.search.cb({ x = 200, y = 100, w = 20, h = 10 })
        d = S.drags[2]
        assert(d[3] == 217, "a function's 7, not scaled: " .. table.concat(d, ","))
        -- The factor gone between the search and its answer.
        o:activate(1)
        S.k = nil
        local clicks = #S.clicks
        S.search.cb({ x = 200, y = 100, w = 20, h = 10 })
        assert(#S.clicks == clicks and S.speech[#S.speech].text == "Fold is not available now", T.said())
        -- Without a scale, a fractional frame: the plain sums.
        local p = T.O.new("Plain")
        p:frame(function() return 0.5, 0.25 end)
        p:addGraphicalButton({ label = "Fold", image = "fold.png", clickOffset = { 10, 4 } })
        p:addGraphicalButton({ label = "Grip", image = "grip.png", dragBy = { 5, 0 } })
        T.front(p)
        p:activate(1)
        S.search.cb({ x = 200, y = 100, w = 20, h = 10 })
        assert(T.lastClick() == "210,104", T.lastClick())
        p:activate(2)
        S.search.cb({ x = 200, y = 100, w = 20, h = 10 })
        d = S.drags[3]
        assert(d[3] == 215, table.concat(d, ","))
    "##);
}

/// A graphical toggle's `reveal` probe is a point like any other: read and, while the panel is
/// shut, clicked at its placed point. `captureRegion` saves the placed rectangle.
#[test]
fn a_reveal_probe_and_a_captured_region_are_placed_through_the_scale() {
    run(r##"
        local S = T.S
        S.k = 2
        rawset(T.host.screen, "imageSearchMulti", function() return nil end)
        local o = T.scaled(function(o)
          o:addGraphicalToggle({ label = "Chorus", region = { 10, 10, 60, 30 }, onImage = "on.png",
            offImage = "off.png", reveal = { at = { 50, 60 }, closedColor = { 0, 0, 0 } } })
        end)
        S.pixel = { r = 0, g = 0, b = 0 }
        o:activate(1)
        local p = S.pixelsAt[#S.pixelsAt]
        assert(p[1] == 200 and p[2] == 170, "the probe at 100 + 2*50, 50 + 2*60: " .. p[1] .. "," .. p[2])
        assert(T.lastClick() == "200,170", "the shut panel opened at the probe: " .. T.lastClick())
        assert(o:captureRegion({ 10, 10, 20, 20 }, "C:/shot.png") == true)
        local shot = S.shots[#S.shots]
        assert(shot.region[1] == 120 and shot.region[2] == 70 and shot.region[3] == 140 and shot.region[4] == 90,
          table.concat(shot.region, ","))
        S.k = nil
        assert(o:captureRegion({ 10, 10, 20, 20 }, "C:/shot.png") == false)
        assert(T.count("captureRegion: cannot be placed now (its scale function answered nil)") == 1, T.dump())
        local u = T.scaled(function(u) end, false)
        assert(u:captureRegion({ 10, 10, 20, 20 }, "C:/shot.png") == true)
        shot = S.shots[#S.shots]
        assert(shot.region[1] == 110 and shot.region[4] == 70, table.concat(shot.region, ","))
    "##);
}

/// `fromRight` is a distance leftwards from the content's right edge, scaled, with the frame's dx
/// added and `about` applying to its y only; a landmark-anchored overlay places from the
/// landmark's corner, with no frame.
#[test]
fn from_right_with_an_about_and_a_landmark_anchored_overlay() {
    run(r##"
        local S = T.S
        S.k = 2
        local o = T.O.new("Synth")
        o:frame(function() return 3, 4 end)
        o:scale(function() return S.k end, { about = { 100, 10 } })
        o:addHotspotButton({ label = "Right", at = { 10, 20 }, fromRight = true })
        o:addHotspotButton({ label = "Plain", at = { 110, 30 } })
        T.front(o)
        o:activate(1)
        assert(T.lastClick() == "883,74", "100 + 3 + 800 - 2*10, 50 + 4 + 2*(20 - 10): " .. T.lastClick())
        local a = T.O.new("Anchored")
        a:frame(function() return 3, 4 end)
        a:scale(function() return S.k end)
        a:landmark("C:/wordmark.png", { anchor = true })
        a:addHotspotButton({ label = "Knob", at = { 10, 20 } })
        T.front(a)
        a._landmarkHit = { x = 300, y = 200, w = 40, h = 10 }
        a:activate(1)
        assert(T.lastClick() == "320,240", "the landmark's corner + 2*(10,20), no frame: " .. T.lastClick())
    "##);
}

/// `O:scale` after the overlay is bound raises; a `points` sequence whose factor goes away half-way
/// ends where it is, said once, and clicks nothing after.
#[test]
fn scale_after_binding_raises_and_a_sequence_ends_where_its_factor_goes() {
    run(r##"
        local S = T.S
        local b = T.O.new("Bound")
        b:attach({ class = "nothing" }, {})
        local ok, err = pcall(function() b:scale(function() return 1 end) end)
        assert(not ok and string.find(tostring(err), "scale must be called BEFORE binding it", 1, true), tostring(err))
        S.k = 2
        local o = T.scaled(function(o)
          o:addHotspotButton({ label = "Steps", points = { { 10, 20 }, { 30, 40 }, { 50, 60 } }, settle = 100 })
        end)
        o:activate(1)
        assert(T.lastClick() == "120,90", T.lastClick())
        S.k = nil
        S.now += 100
        T.runDue()
        S.now += 100
        T.runDue()
        assert(#S.clicks == 1, "nothing after the factor went: " .. #S.clicks)
        assert(S.speech[#S.speech].text == "Steps is not available now", T.said())
    "##);
}

// ---------------------------------------------------------------------------------------------
// ARC ON:EAR, moved onto the runtime's scale with its points unchanged.
// ---------------------------------------------------------------------------------------------

/// The module, loaded for real; every hotspot and hotspot toggle with a design point, in all four
/// of its overlays, over a grid of window sizes and places — each click held against the formula
/// ON:EAR's own geometry used until 2026-09-28:
///
///     at = { floor(w / 2 + k * (x - 960) + 0.5), floor(k * y + 0.5) },  k = h / 1009
///
/// added to the content's top-left. A few points are written out as numbers as well.
#[test]
fn on_ear_clicks_every_design_point_where_its_own_geometry_did() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        assert(#made == 4, "the window, two choosers and the settings panel: " .. #made)
        local function old(client, dx, dy)
          local k = client.h / 1009
          local at = { math.floor(client.w / 2 + k * (dx - 1920 / 2) + 0.5), math.floor(k * dy + 0.5) }
          return client.x + at[1], client.y + at[2]
        end
        -- The frame is made wide, so that no point is refused for lying outside a window drawn
        -- narrower than the design's aspect: the question here is where a point is placed, and
        -- the refusal is the same code either way.
        local function place(x, y, w, h)
          S.origin = { id = 7, class = "JUCE_1a05739a091", app = { pid = 4242 },
            client = { x = x, y = y, w = w, h = h },
            bounds = { x = x - 5000, y = y - 5000, w = 20000, h = 20000 } }
        end
        -- Every control that clicks a design point.
        local pts, checked = {}, 0
        for _, o in ipairs(made) do
          for i, c in ipairs(o.controls) do
            if (c.kind == "hotspot" or c.kind == "hotspottoggle") and type(c.at) == "table" then
              assert(not c.rawOrigin, c.label)
              pts[#pts + 1] = { o = o, i = i, c = c }
            end
          end
        end
        -- The window's 21 (2 views, Settings, 4 curves, 7 FN buttons, 5 switches, the two Select
        -- buttons), the choosers' 3 filters and the settings panel's 5.
        assert(#pts == 29, "29 design points: " .. #pts)
        for _, x0 in ipairs({ 0, 7, -1913 }) do
          for _, w in ipairs({ 400, 401, 826, 827, 1000, 1280, 1366, 1600, 1919, 1920, 1921, 2560, 3840 }) do
            for _, h in ipairs({ 300, 481, 482, 600, 768, 1009, 1010, 1080, 1440, 2018, 2160 }) do
              place(x0, 11, w, h)
              for _, p in ipairs(pts) do
                if not p.o.active then T.front(p.o) end
                S.clicks, S.logs, S.speech, S.after = {}, {}, {}, {}
                p.o:activate(p.i)
                local c = S.clicks[1]
                local ex, ey = old(S.origin.client, p.c.at[1], p.c.at[2])
                assert(c and c[1] == ex and c[2] == ey, ("%s at %d,%d %dx%d: clicked %s, the old "
                  .. "geometry %d,%d"):format(p.c.label, x0, 11, w, h, c and (c[1] .. "," .. c[2]) or "nothing",
                  ex, ey))
                checked += 1
              end
            end
          end
        end
        assert(checked == 29 * 3 * 13 * 11, checked)
        -- Written out: the probe's own 826x481 at the screen's corner, and 1280x720 at 100,50.
        local function clickOf(label, x, y, w, h)
          place(x, y, w, h)
          for _, p in ipairs(pts) do
            if p.c.label == label then
              S.clicks = {}
              p.o:activate(p.i)
              return S.clicks[1][1] .. "," .. S.clicks[1][2]
            end
          end
        end
        assert(clickOf("Settings", 0, 0, 826, 481) == "791,20")
        assert(clickOf("Select speakers", 0, 0, 826, 481) == "415,462")
        assert(clickOf("FN Mute", 0, 0, 826, 481) == "471,281")
        assert(clickOf("Space view", 0, 0, 826, 481) == "66,13")
        assert(clickOf("Calibration", 0, 0, 826, 481) == "145,310")
        assert(clickOf("Close", 0, 0, 826, 481) == "416,452")
        assert(clickOf("Firmware recovery", 0, 0, 826, 481) == "660,195")
        assert(clickOf("Settings", 100, 50, 1280, 720) == "1306,80")
        assert(clickOf("Select speakers", 100, 50, 1280, 720) == "744,742")
        -- At the design's own size, the design's own numbers.
        assert(clickOf("FN Mute", 0, 0, 1920, 1009) == "1082,589")
    "##);
}

/// The points ON:EAR's own code clicks — Tone and Width, the virtual-speaker arrows, the chooser
/// grid, whose rows slide by fractions of a pixel — and the rectangles it reads, through
/// `toScreen` / `toScreenRect`, against its old screen-point helpers, fractions included.
#[test]
fn on_ear_places_its_own_clicks_and_reads_where_its_helpers_did() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local function oldPoint(cl, dx, dy)
          local k = cl.h / 1009
          return math.floor(cl.x + cl.w / 2 + k * (dx - 1920 / 2) + 0.5), math.floor(cl.y + k * dy + 0.5)
        end
        local function oldRect(cl, x1, y1, x2, y2)
          local k = cl.h / 1009
          local half = cl.w / 2
          return {
            math.floor(cl.x + half + k * (x1 - 1920 / 2) + 0.5), math.floor(cl.y + k * y1 + 0.5),
            math.floor(cl.x + half + k * (x2 - 1920 / 2) + 0.5), math.floor(cl.y + k * y2 + 0.5),
          }
        end
        -- What those helpers were handed: Tone, the arrows, the Width track from 10 to 200 % with
        -- its overshoot, the tile probes with the grid slid by fractions, and the three read
        -- rectangles.
        local points = { { 290, 780 }, { 855, 891 }, { 1072, 891 }, { 1539, 151 }, { 449, 201 }, { 466, 600 } }
        for pct = 10, 200 do points[#points + 1] = { 629.1 + 0.4812 * pct, 869 } end
        points[#points + 1] = { 629.1 + 0.4812 * 200 + 8, 869 }
        for _, col in ipairs({ 747, 941, 1134, 1328, 1521 }) do
          for shift = -96, 96, 7.37 do
            points[#points + 1] = { col - 81, 264 + shift }
            points[#points + 1] = { col - 72, 507 + shift }
          end
        end
        local rects = { { 865, 876, 1065, 945 }, { 330, 186, 565, 214 }, { 652, 164, 1616, 801 } }
        local n = 0
        for _, o in ipairs(made) do
          T.front(o)
          for _, x0 in ipairs({ 0, 7, -1913 }) do
            for _, w in ipairs({ 401, 826, 1280, 1920, 1921, 3840 }) do
              for _, h in ipairs({ 481, 600, 1009, 1010, 2160 }) do
                S.origin = { id = 7, app = { pid = 4242 }, client = { x = x0, y = 13, w = w, h = h },
                  bounds = { x = x0, y = 13, w = w, h = h } }
                for _, p in ipairs(points) do
                  local x, y = o:toScreen(p[1], p[2])
                  local ex, ey = oldPoint(S.origin.client, p[1], p[2])
                  assert(x == ex and y == ey, ("%s: %s,%s at %dx%d: %s,%s, the old helper %d,%d")
                    :format(o.label, p[1], p[2], w, h, tostring(x), tostring(y), ex, ey))
                  n += 1
                end
                for _, r in ipairs(rects) do
                  local got = o:toScreenRect(r)
                  local want = oldRect(S.origin.client, r[1], r[2], r[3], r[4])
                  for k = 1, 4 do assert(got[k] == want[k], o.label .. " rect corner " .. k) end
                end
              end
            end
          end
        end
        assert(n > 100000, n)
        -- No height to read, no point: the old helpers answered nil there too.
        S.origin = { id = 7, client = { x = 0, y = 0, w = 800, h = 0 }, bounds = { x = 0, y = 0, w = 800, h = 0 } }
        assert(made[1]:toScreen(290, 780) == nil)
        assert(made[1]:toScreenRect({ 865, 876, 1065, 945 }) == nil)
    "##);
}

/// A chooser row is found through the tree, and its probe is in the window's own pixels: the row
/// toggles are `rawOrigin`, so neither the panel's frame nor its scale moves them — the point the
/// module clicked before.
#[test]
fn on_ear_chooser_rows_are_clicked_in_the_windows_own_pixels() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local speakers = made[2]
        assert(speakers.label == "Speaker Browser", speakers.label)
        local row
        for i, c in ipairs(speakers.controls) do
          if c.label == "Category" and not row then row = i end
        end
        assert(speakers.controls[row].rawOrigin == true, "the rows are rawOrigin")
        S.origin = { id = 7, app = { pid = 4242 }, client = { x = 30, y = 40, w = 826, h = 481 },
          bounds = { x = 30, y = 40, w = 826, h = 481 } }
        S.dump = {
          { name = "SpeakerBrowserWindow", ctype = 50032, depth = 1, bounds = { x = 150, y = 100, w = 600, h = 360 } },
          { name = "Row 1", ctype = 50020, depth = 2, bounds = { x = 170, y = 160, w = 180, h = 16 } },
        }
        T.front(speakers)
        speakers:activate(row)
        assert(T.lastClick() == "340,168", "the row's left edge + 170, its middle: " .. T.lastClick())
    "##);
}

/// A tile's press reads nothing of the tile first — the runtime reads a stepper's value before a
/// press and nobody else's (runtime 0.3.0) — and still clicks where the grid sits NOW: the tile
/// learns that itself, from one read of the whole grid, before it clicks. The read before the press
/// used to do it on the side, through the tile's `text`.
#[test]
fn on_ear_a_tile_press_reads_no_text_first_and_clicks_on_a_fresh_grid() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local phones
        for _, o in ipairs(made) do
          if o.label == "Headphone Browser" then phones = o end
        end
        assert(phones, "the headphone chooser")
        -- The design's own size, at the screen's corner: one design pixel is one screen pixel.
        S.origin = { id = 7, app = { pid = 4242 }, client = { x = 0, y = 0, w = 1920, h = 1009 },
          bounds = { x = 0, y = 0, w = 1920, h = 1009 } }
        T.front(phones)
        local tile
        for i, c in ipairs(phones.controls) do
          if c.label == "Headphone 1" then tile = i end
        end
        local texts, text = 0, phones.controls[tile].text
        phones.controls[tile].text = function(...)
          texts += 1
          return text(...)
        end
        -- The grid has slid 20 down: the first tile's printed name sits 70 below its middle,
        -- at 314 + 20 + 70 = 404.
        T.answerWaits(function(what)
          local r = what.region or what
          S.recognized[#S.recognized + 1] = { r[1], r[2], r[3], r[4] }
          return { text = "HD600", skipped = false, status = "text",
            words = { { text = "HD600", x = 727, y = 398, w = 40, h = 12 } } }
        end)
        -- As the host presses it: in the key's handler, where a read may wait.
        T.call("key", function() phones:activate(tile) end)
        assert(texts == 0, "the tile's text is not read before the press: " .. texts)
        assert(#S.recognized == 1 and table.concat(S.recognized[1], ",") == "652,214,1616,801",
          "one read of the whole grid: " .. #S.recognized)
        assert(T.lastClick() == "747,334", "the tile where the grid sits now: " .. T.lastClick())
        assert(T.count("[on-ear] Headphone Browser: read the grid in") == 1, T.dump())
    "##);
}

/// A tile's press waits for its read of the grid, and only its module waits: another window
/// brought to the front meanwhile takes the press's click away — nothing is clicked in that window,
/// and the log says why.
#[test]
fn on_ear_a_tile_press_clicks_nothing_once_another_window_came_to_the_front_during_its_read() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local phones
        for _, o in ipairs(made) do
          if o.label == "Headphone Browser" then phones = o end
        end
        S.origin = { id = 7, app = { pid = 4242 }, client = { x = 0, y = 0, w = 1920, h = 1009 },
          bounds = { x = 0, y = 0, w = 1920, h = 1009 } }
        T.front(phones)
        local tile
        for i, c in ipairs(phones.controls) do
          if c.label == "Headphone 1" then tile = i end
        end
        local clicks = #S.clicks
        T.answerWaits(function(what)
          -- Another program comes to the front while the grid is being read.
          S.origin = { id = 5000, app = { pid = 777 }, client = { x = 0, y = 0, w = 800, h = 600 },
            bounds = { x = 0, y = 0, w = 800, h = 600 } }
          return { text = "HD600", skipped = false, status = "text",
            words = { { text = "HD600", x = 727, y = 398, w = 40, h = 12 } } }
        end)
        -- As the host presses it: in the key's handler, where a read may wait.
        T.call("key", function() phones:activate(tile) end)
        assert(#S.clicks == clicks, "a click in the other window: " .. T.lastClick())
        assert(T.count("[on-ear] 'Headphone 1' not clicking after the grid was read — the overlay is on another "
          .. "window now") == 1, T.dump())
    "##);
}

/// "Grid down" says where the grid has arrived once it has read the grid, and that read waits with
/// only its module waiting: another window brought to the front during the read, or in the quarter
/// second before it, takes the sentence away — nothing is said in that window, and the log says
/// why. A read that failed is no grid without names: the grid is read again at the next ask.
#[test]
fn on_ear_grid_down_says_nothing_once_another_window_came_to_the_front_and_reads_again_after_a_failed_read() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local phones
        for _, o in ipairs(made) do
          if o.label == "Headphone Browser" then phones = o end
        end
        local here = { id = 7, app = { pid = 4242 }, client = { x = 0, y = 0, w = 1920, h = 1009 },
          bounds = { x = 0, y = 0, w = 1920, h = 1009 } }
        local there = { id = 5000, app = { pid = 777 }, client = { x = 0, y = 0, w = 800, h = 600 },
          bounds = { x = 0, y = 0, w = 800, h = 600 } }
        -- The evaluations the bindings asked for as they loaded, done before the scenario puts
        -- the overlay in front: the timers below are then the module's own.
        T.runDue()
        S.origin = here
        T.front(phones)
        local down, tile
        for i, c in ipairs(phones.controls) do
          if c.label == "Grid down" then down = i end
          if c.label == "Headphone 1" then tile = i end
        end
        local reads, during, answer = 0, nil, nil
        local GRID = { text = "HD600", skipped = false, status = "text",
          words = { { text = "HD600", x = 727, y = 398, w = 40, h = 12 } } }
        T.answerWaits(function()
          reads += 1
          if during then during() end
          return answer or GRID
        end)
        local function press(i)
          T.call("key", function() phones:activate(i) end)
        end
        -- The quarter second, and only the timers that came due in it: no tick of the runtime's,
        -- whose evaluation would ask for a window this scenario does not script.
        local function later(ms)
          S.now += ms
          T.runDue()
        end

        -- Nothing changes: the grid is read once, and where it arrived is said.
        local spoken = #S.speech
        press(down)
        later(300)
        assert(reads == 1 and #S.speech == spoken + 1, "read and said: " .. T.dump())

        -- Another program comes to the front while the grid is read: nothing said.
        during = function() S.origin = there end
        spoken = #S.speech
        press(down)
        later(300)
        assert(reads == 2 and #S.speech == spoken, "said in the other window: " .. tostring(S.speech[#S.speech].text))
        assert(T.count("[on-ear] Headphone Browser: Grid down says nothing after reading where it arrived — the "
          .. "overlay is on another window now") == 1, T.dump())

        -- Another program in front before the quarter second is up: nothing read, nothing said.
        during, S.origin = nil, here
        T.front(phones)
        press(down)
        S.origin = there
        spoken = #S.speech
        later(300)
        assert(reads == 2 and #S.speech == spoken, "read or said in the other window: " .. T.dump())
        assert(T.count("[on-ear] Headphone Browser: Grid down says nothing after the scroll — the overlay is on "
          .. "another window now") == 1, T.dump())

        -- A read that failed: the grid stays to be read, so the next press of a tile reads it again.
        S.origin = here
        T.front(phones)
        answer = { status = "failed", text = "", words = {}, skipped = false,
          error = "the text recogniser has not answered a region for 5 s" }
        press(down)
        later(300)
        assert(reads == 3, "read: " .. reads)
        assert(T.count("[on-ear] Headphone Browser: the grid could not be read — the text recogniser has not "
          .. "answered a region for 5 s") == 1, T.dump())
        answer = nil
        press(tile)
        assert(reads == 4, "the grid read again before the tile's click: " .. reads)
    "##);
}

/// ON:EAR's Close reads the pixel under it, for its log line, before it clicks, and a read of the
/// screen can wait: another window in front by the time the pixel is read takes the click away —
/// nothing focused, nothing clicked, and the log says why. Nothing changed, it clicks.
#[test]
fn on_ear_close_clicks_nothing_once_another_window_came_to_the_front_during_its_pixel() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local speakers
        for _, o in ipairs(made) do
          if o.label == "Speaker Browser" then speakers = o end
        end
        local here = { id = 7, app = { pid = 4242 }, client = { x = 0, y = 0, w = 1920, h = 1009 },
          bounds = { x = 0, y = 0, w = 1920, h = 1009 } }
        local there = { id = 5000, app = { pid = 777 }, client = { x = 0, y = 0, w = 800, h = 600 },
          bounds = { x = 0, y = 0, w = 800, h = 600 } }
        S.origin = here
        T.front(speakers)
        local close
        for i, c in ipairs(speakers.controls) do
          if c.label == "Close" then close = i end
        end
        local during = function() S.origin = there end
        rawset(T.host.screen, "pixel", function()
          S.pixels += 1
          if during then during() end
          return S.pixel
        end)
        speakers:activate(close)
        assert(S.pixels == 1 and #S.clicks == 0 and #S.focusCalls == 0, "a click in the other window: " .. T.lastClick())
        assert(T.count("[on-ear] 'Speaker Browser' Close not clicking after its pixel was read — the overlay is on "
          .. "another window now") == 1, T.dump())
        during, S.origin = nil, here
        speakers:activate(close)
        assert(T.lastClick() == "1539,151", "Close where the design puts it: " .. T.lastClick())
    "##);
}

/// ON:EAR's "Show all" is in force when none of the rows on screen reads selected, and it reads
/// those rows — the first four — together, in one read of the screen, where it read them one pixel
/// at a time. A row reading selected says nothing; so does a read that failed.
#[test]
fn on_ear_show_all_reads_its_rows_in_one_read() {
    run(r##"
        local S = T.S
        local made = T.collect()
        T.module("modules/ik-on-ear/")
        local speakers
        for _, o in ipairs(made) do
          if o.label == "Speaker Browser" then speakers = o end
        end
        S.origin = { id = 7, app = { pid = 4242 }, client = { x = 30, y = 40, w = 826, h = 481 },
          bounds = { x = 30, y = 40, w = 826, h = 481 } }
        S.dump = { { name = "SpeakerBrowserWindow", ctype = 50032, depth = 1, bounds = { x = 150, y = 100, w = 600, h = 360 } } }
        for r = 1, 6 do
          S.dump[#S.dump + 1] = { name = "Row " .. r, ctype = 50020, depth = 2,
            bounds = { x = 170, y = 140 + 20 * r, w = 180, h = 16 } }
        end
        T.front(speakers)
        local show
        for i, c in ipairs(speakers.controls) do
          if c.label == "Show all" then show = i end
        end
        local OFF, ON = { r = 26, g = 26, b = 26 }, { r = 46, g = 46, b = 46 }
        local asked, lit, fails = {}, nil, nil
        rawset(T.host.screen, "pixels", function(points)
          asked[#asked + 1] = points
          if fails then return nil, fails end
          local out = {}
          for i in ipairs(points) do out[i] = i == lit and ON or OFF end
          return out
        end)
        local text = speakers.controls[show].text
        assert(text(speakers) == "in force")
        assert(#asked == 1 and S.pixels == 0, "one read, and no pixel read on its own: " .. #asked .. ", " .. S.pixels)
        local pts = {}
        for _, p in ipairs(asked[1]) do pts[#pts + 1] = p[1] .. "," .. p[2] end
        assert(table.concat(pts, " ") == "340,168 340,188 340,208 340,228",
          "the first four rows, 170 into each, at its middle: " .. table.concat(pts, " "))
        lit = 3
        T.turn()
        assert(text(speakers) == nil, "a category is in force")
        lit, fails = nil, "screen capture failed"
        T.turn()
        assert(text(speakers) == nil, "nothing read is no answer")
        assert(#asked == 3 and S.pixels == 0, #asked .. ", " .. S.pixels)

        -- The Headphone Browser's Favorites is a filter of its own, which no row's state answers: it
        -- reads no row at all. Its "Show all" reads them as the speakers' does.
        local headphones
        for _, o in ipairs(made) do
          if o.label == "Headphone Browser" then headphones = o end
        end
        S.dump[1].name = "HeadphonesBrowserWindow"
        fails = nil
        T.front(headphones)
        local favorites, all
        for _, c in ipairs(headphones.controls) do
          if c.label == "Favorites" then favorites = c end
          if c.label == "Show all" then all = c end
        end
        T.turn()
        assert(favorites.text(headphones) == nil and #asked == 3, "Favorites read the rows: " .. #asked)
        assert(all.text(headphones) == "in force" and #asked == 4 and #asked[4] == 4, #asked)
    "##);
}
