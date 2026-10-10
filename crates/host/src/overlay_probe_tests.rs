//! The overlay runtime's probes (`O:probe`, runtime 0.5.0), run for real against the scripted host
//! of `overlay_menu_tests.rs`.
//!
//! Written for the maintainer's decision of 2026-10-10 (Q2 = A of wbr-final.md, step T2): a `when`
//! runs where nothing can wait and must not read the screen, so the pixels it answers from are the
//! overlay's probes, which the runtime reads — every probe of the overlay that is due, in one
//! `host.screen.pixels` — in the handler before a scan, and only when something acted since the last
//! reading. Where nothing can wait, as the arbiter brings an overlay to the front, nothing is read,
//! and the check 350 ms later reads and corrects. An unrecognised reading is not kept. A value that
//! changed registers the overlay's hotkeys again. Nothing is read on the menu tick.
//!
//! And for its review: nothing is read in what a menu test's or the item shot's answer goes on to do
//! either; a reading of a window the overlay left while it was taken is not kept; an origin that
//! raises reads nothing and does not stop the Tab's scan; a probe can be due in every epoch, for what
//! the user's own keys change.
//!
//! And for the NVDA session of the same day, where a Tab straight after Kontakt's F10 read the view
//! before Kontakt had redrawn it, and that reading stood for the act: a module says which of its acts
//! change what a probe shows (`O:expectChange`), and the probe is read at every key until a reading
//! shows the change, then at none.
//!
//! Every read here records where it was made — the handler's own coroutine, where it may wait, or a
//! pass of the runtime's, where it may not — so each scenario also holds that no probe is read
//! where a capture could not wait.

use super::overlay_menu_tests::run_with;

/// The helpers, added to the harness's `T`.
const PR: &str = r##"
local S = T.S
-- host.inputEpoch apart from host.epoch: an act turns both over (T.act), a key only the epoch.
S.inputEpoch = 0
rawset(T.host, "inputEpoch", function() return S.inputEpoch end)
-- host.screen.pixels as the host answers it, all or nothing, each point's colour from S.at
-- ("x,y" -> colour) or S.pixel; and where it was asked: in a handler's own coroutine, where Luau can
-- suspend it, or in a pass of the runtime's.
S.at = {}
S.probeReads = {}
rawset(T.host.screen, "pixels", function(points)
  local _, inPass = T.O._passes()
  S.probeReads[#S.probeReads + 1] = { points = points, where = T.whereNow(),
    yieldable = coroutine.isyieldable(), inPass = inPass }
  if S.pixelsFail then return nil, S.pixelsFail end
  local out = {}
  for i, p in ipairs(points) do out[i] = S.at[p[1] .. "," .. p[2]] or S.pixel end
  return out
end)

T.LIT, T.DARK = { r = 240, g = 240, b = 240 }, { r = 20, g = 20, b = 20 }

-- Something the module did acted on the screen: a click, a key it sent.
function T.act()
  S.inputEpoch += 1
  S.epoch += 1
end

-- How many reads the probes have made.
function T.reads() return #S.probeReads end

-- Every read so far was made where a capture may wait: in a handler, outside every pass.
function T.allWaitable()
  for i, r in ipairs(S.probeReads) do
    assert(r.where == "handler" and r.yieldable and not r.inPass,
      ("read %d made where it cannot wait: %s, yieldable %s, in a pass %s"):format(i, r.where,
        tostring(r.yieldable), tostring(r.inPass)))
  end
end

-- The label of the focused control.
function T.on(o) local c = o.controls[o.focus]; return c and c.label end

-- An overlay over S.origin (client 100,50 800x600) with a lamp at (10,20) — on screen (110,70) —
-- and three buttons, the middle one shown while the lamp is lit; in front, unbound.
function T.lampOverlay(extra)
  local o = T.O.new("Plug-in")
  S.lampReads = 0
  local lit = o:probe("lamp", { { 10, 20 } }, { read = function(c)
    S.lampReads += 1
    return c[1].r > 200
  end })
  o:addCustomButton({ label = "One", onActivate = function() end })
  o:addCustomButton({ label = "Lamp", hotkey = "Alt+L", when = function() return lit() == true end,
    onActivate = function() end })
  o:addCustomButton({ label = "Three", onActivate = function() end })
  if extra then extra(o, lit) end
  T.front(o)
  return o, lit
end
"##;

fn run(scenario: &str) {
    run_with("windows", PR, scenario)
}

/// A `when` answers from its probe, which the runtime reads in the Tab's handler before the scan —
/// once, in one call, where it may wait — and Tabs in a row read nothing: the key it was read under
/// has not changed. A lamp lit meanwhile is seen after the next act, at the next Tab.
#[test]
fn a_when_answers_from_its_probe_read_once_before_the_scan_and_not_per_tab() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o = T.lampOverlay()
        assert(T.reads() == 0, "nothing is read outside a handler: " .. T.reads())
        o.focus = 1
        T.tab()
        assert(T.reads() == 1, "one read before the scan: " .. T.reads())
        local pts = S.probeReads[1].points
        assert(#pts == 1 and pts[1][1] == 110 and pts[1][2] == 70, "the lamp's point on screen")
        assert(T.on(o) == "Three", "the lamp is dark: " .. tostring(T.on(o)))
        T.tab()
        T.tab()
        T.tab()
        assert(T.reads() == 1 and S.lampReads == 1, "Tabs in a row read nothing: " .. T.reads())
        -- Lit now, but nothing acted: the reading stands.
        S.pixel = T.LIT
        o.focus = 1
        T.tab()
        assert(T.on(o) == "Three" and T.reads() == 1, "read without an act")
        -- An act, and the next Tab reads it.
        T.act()
        o.focus = 1
        T.tab()
        assert(T.reads() == 2 and T.on(o) == "Lamp", "the lamp after the act: " .. tostring(T.on(o)))
        -- A press after an act reads before its `when` is asked.
        S.pixel = T.DARK
        T.act()
        local spoken = #S.speech
        T.call("key", function() o.focus = 2; o:activate() end)
        assert(T.reads() == 3, "the press read first: " .. T.reads())
        assert(S.speech[spoken + 1] and S.speech[spoken + 1].text == "Lamp is not available now",
          "the lamp's control is hidden now")
        T.allWaitable()
    "##);
}

/// The check 250 ms after an overlay came to the front turns the state generation over, which makes
/// the probes due again with nothing acted, as Kontakt's view probe always was: a reading taken while
/// the screen still showed the window before is not kept past it.
#[test]
fn the_look_again_after_coming_to_the_front_makes_the_probes_due() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o = T.lampOverlay()
        o.contexts = { { match = function() return true end, origin = function() return S.origin end } }
        T.tab()
        assert(T.reads() == 1)
        -- Out of the front and back, at the top level: nothing read there.
        o:_deactivate()
        o:_activate()
        T.tab()
        assert(T.reads() == 1, "nothing acted, nothing due: " .. T.reads())
        S.now += 250
        T.runDue()
        T.tab()
        assert(T.reads() == 2, "due again after the look again: " .. T.reads())
        T.allWaitable()
    "##);
}

/// Where nothing can wait — the arbiter's call bringing an overlay on a slot to the front — nothing is
/// read: the `when`s answer from the last reading (none yet, so the lamp is dark), and the arrival is
/// on the dark side. The check 350 ms later, in a timer's handler, reads the lamp, finds it lit,
/// moves the focus off the control it hides and says the one it shows.
#[test]
fn nothing_is_read_where_nothing_can_wait_and_the_check_350_ms_later_corrects() {
    run(r##"
        local S = T.S
        S.pixel = T.LIT
        S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
        T.turn()
        local o = T.O.new("Plug-in")
        local lit = o:probe("lamp", { { 10, 20 } }, { read = function(c) return c[1].r > 200 end })
        o:addCustomButton({ label = "Lit", when = function() return lit() == true end, onActivate = function() end })
        o:addCustomButton({ label = "Dark", when = function() return lit() ~= true end, onActivate = function() end })
        o:attachEmbedded({ hosts = { T.HOST }, control = "Qt%d+.-QWindowIcon" }, { slot = "com.platform.kontakt" })
        T.runDue()
        assert(o.active, T.dump())
        assert(T.reads() == 0, "read in the arbiter's call: " .. T.reads())
        assert(T.on(o) == "Dark", "the arrival answers from the last reading, none yet: " .. tostring(T.on(o)))
        local spoken = #S.speech
        S.now += 400
        T.runDue()
        assert(T.reads() == 1, "the check 350 ms later reads: " .. T.reads())
        assert(T.on(o) == "Lit", tostring(T.on(o)))
        assert(T.count("[focus] 'Plug-in': 'Dark' is hidden now; resuming on 'Lit'") == 1, T.dump())
        assert(S.speech[spoken + 1] and string.find(S.speech[spoken + 1].text, "Lit", 1, true) == 1,
          "the arrival says the control the reading shows")
        T.allWaitable()
    "##);
}

/// A reading `read` does not recognise is not kept: the last value stands, and the probe is due again
/// at the next scan of a later epoch — but not twice in one, however many scans that handler makes.
#[test]
fn an_unrecognised_reading_is_not_kept_and_is_read_again_in_a_later_epoch_only() {
    run(r##"
        local S = T.S
        local o = T.O.new("Plug-in")
        local asked = 0
        local view = o:probe("view", { { 10, 20 } }, { read = function(c, last, pts)
          asked += 1
          assert(pts[1][1] == 110 and pts[1][2] == 70, "the points read are handed over")
          if c[1].r == 24 then return "play" end
          if c[1].r == 99 then return "classic" end
          return nil
        end })
        o:addCustomButton({ label = "Play", when = function() return view() == "play" end, onActivate = function() end })
        o:addCustomButton({ label = "Classic", when = function() return view() == "classic" end, onActivate = function() end })
        o:addCustomButton({ label = "Other", onActivate = function() end })
        T.front(o)
        S.pixel = { r = 24, g = 24, b = 24 }
        o.focus = 3
        T.tab()
        assert(asked == 1 and T.on(o) == "Play", tostring(T.on(o)))
        -- Acted, and something else is in front of the plug-in: not recognised, the view stands.
        T.act()
        S.pixel = { r = 255, g = 255, b = 255 }
        o.focus = 3
        T.tab()
        assert(asked == 2 and T.on(o) == "Play", "the last value stands: " .. tostring(T.on(o)))
        -- One handler, two scans: read once.
        S.epoch += 1
        T.call("key", function() o:focusNext(); o:focusNext() end)
        assert(asked == 3, "due in a later epoch, once in it: " .. asked)
        -- Recognised again: kept, and Tabs in a row read nothing.
        S.pixel = { r = 99, g = 99, b = 99 }
        o.focus = 3
        T.tab()
        assert(asked == 4 and T.on(o) == "Classic", tostring(T.on(o)))
        T.tab()
        T.tab()
        assert(asked == 4, "kept: " .. asked)
        T.allWaitable()
    "##);
}

/// The probes due in one handler are read in one call, in the order they were declared, and each
/// one's value is set before the next one's `read` runs: a `read` may ask an earlier probe's reader.
/// Without `read` the value is the list of colours. A probe whose function places nothing reads
/// nothing and keeps its value.
#[test]
fn the_probes_due_are_read_in_one_call_in_order() {
    run(r##"
        local S = T.S
        local o = T.O.new("Plug-in")
        local seen
        local first = o:probe("first", { { 10, 20 } })
        local second = o:probe("second", function(ov)
          local c = ov:origin().client
          return { { c.x + c.w - 5, c.y + 5 }, { c.x + 1.4, c.y + 1.6 } }
        end, { read = function(c)
          seen = first()
          return #c
        end })
        local placed = true
        local third = o:probe("third", function() if placed then return { { 1, 1 } } end end)
        o:addCustomButton({ label = "One", onActivate = function() end })
        o:addCustomButton({ label = "Two", onActivate = function() end })
        T.front(o)
        S.at["110,70"] = T.LIT
        T.tab()
        assert(T.reads() == 1, "one call: " .. T.reads())
        local pts = {}
        for _, p in ipairs(S.probeReads[1].points) do pts[#pts + 1] = p[1] .. "," .. p[2] end
        assert(table.concat(pts, " ") == "110,70 895,55 101,52 1,1",
          "in order, rounded to whole pixels: " .. table.concat(pts, " "))
        assert(type(seen) == "table" and seen[1].r == 240, "the first probe's value, set before the second's read")
        assert(second() == 2, "two colours")
        local c = first()
        assert(#c == 1 and c[1].r == 240, "the colours themselves")
        -- Placed nothing: not read, and the value stands.
        placed = false
        T.act()
        T.tab()
        assert(T.reads() == 2 and #S.probeReads[2].points == 3, "the third not read: " .. #S.probeReads[2].points)
        assert(third() ~= nil, "its value stands")
        T.allWaitable()
    "##);
}

/// A reading that changed what a probe says registers the overlay's hotkeys again: a control the
/// lamp hid claimed no combination, and once the lamp is read lit, its Alt+L is held.
#[test]
fn a_value_that_changed_registers_the_hotkeys_again() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o = T.lampOverlay()
        local function holds(spec)
          for _, s in pairs(S.hot) do if s == spec then return true end end
          return false
        end
        assert(not holds("Alt+L"), "a hidden control claims nothing")
        S.pixel = T.LIT
        T.tab()
        assert(holds("Alt+L"), "taken once the lamp is read lit: " .. T.dump())
        assert(T.count("[keys] 'Plug-in' holds: Alt+L | not taken: ") == 1, T.dump())
        -- Nothing changed: not registered again.
        local registered = S.registered
        T.act()
        T.tab()
        assert(T.reads() == 2 and S.registered == registered, "registered again with nothing changed")
        T.allWaitable()
    "##);
}

/// The reader called in a handler outside a scan reads a due probe first — a `text`, a timer — and
/// inside a scan it answers what stands; a module's top level reads nothing.
#[test]
fn a_reader_in_a_handler_outside_a_scan_reads_first() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o, lit = T.lampOverlay()
        assert(lit() == nil and T.reads() == 0, "the top level reads nothing")
        local got
        T.call("timer", function() got = lit() end)
        assert(got == false and T.reads() == 1, "a timer's handler reads first: " .. T.reads())
        S.pixel = T.LIT
        T.act()
        T.call("timer", function() got = lit(); got = lit() end)
        assert(got == true and T.reads() == 2, "once: " .. T.reads())
        T.allWaitable()
    "##);
}

/// Nothing is read on the menu tick, which comes on its own clock: a reading taken right after an act,
/// before the plug-in has redrawn, would stand as fresh until the next act. Nor in what the tick runs:
/// the hotkeys taken back as a menu closes are registered from what stands. The next Tab reads it.
#[test]
fn nothing_is_read_on_the_menu_tick() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o, lit = T.lampOverlay(function(o) o:_watchMenus({ T.O.menuTests.nativePopup }) end)
        T.tab()
        assert(T.reads() == 1)
        T.act()
        T.tick(4)
        assert(T.reads() == 1, "read on the menu tick: " .. T.reads())
        -- A menu up and closed again: the keys are given up and taken back on the tick.
        S.native = true
        T.tick(1)
        S.native = false
        T.tick(4)
        assert(T.count("[keys] 'Plug-in' took back its per-control hotkeys") == 1, T.dump())
        assert(T.reads() == 1, "read as the keys were taken back: " .. T.reads())
        T.tab()
        assert(T.reads() == 2)
        -- Nor in what the tick runs after its tests: an item whose menu was never seen is given up
        -- on the tick, and its `onDone` asks the lamp, due since the opener's click.
        rawset(T.host, "input", T.strict("host.input", { click = function() T.act() end }))
        rawset(T.host.window, "ownsPoint", function() return true end)
        local seen = "not asked"
        S.epoch += 1
        T.call("key", function()
          o:chooseMenuItem({ label = "Menu", at = { 50, 10 }, menuItem = { 1, 1 },
            onDone = function() seen = lit() end })
        end)
        T.run(8200)
        assert(seen == false, "onDone asked the lamp on the tick: " .. tostring(seen))
        assert(T.reads() == 2, "read in the tick's onDone: " .. T.reads())
        T.tab()
        assert(T.reads() == 3)
        T.allWaitable()
    "##);
}

/// What a menu test's answer goes on to do when it comes in a callback of its own is the tick's work,
/// and reads no probe either: the hotkeys taken back as the menu closes are registered from what
/// stands, though the menu's own close made the lamp due. The next Tab reads it.
#[test]
fn nothing_is_read_in_a_menu_tests_answer_that_comes_in_a_callback() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local seesMenu = false
        local later = { name = "later", cheap = true, test = function(_, answer)
          T.host.timer.after(0, function() answer(seesMenu) end)
        end }
        local o, lit = T.lampOverlay(function(o) o:_watchMenus({ later }) end)
        T.tab()
        assert(T.reads() == 1)
        seesMenu = true
        T.tick(2)
        assert(T.count("[keys] 'Plug-in' gave up its per-control hotkeys") == 1, T.dump())
        T.act()
        S.pixel = T.LIT
        seesMenu = false
        T.tick(4)
        assert(T.count("[keys] 'Plug-in' took back its per-control hotkeys") == 1, T.dump())
        assert(T.reads() == 1, "read in the test's answer: " .. T.reads())
        assert(lit() == false, "the lamp as it stood")
        T.tab()
        assert(T.reads() == 2 and lit() == true)
        T.allWaitable()
    "##);
}

/// In a calibrating run the item is clicked in the answer of its picture, and with `retries = 0` the
/// pick's `onDone` runs there, right after the click: it reads no probe, since a reading then would
/// be of the menu still drawn. The next Tab reads.
#[test]
fn nothing_is_read_in_the_menu_item_shots_answer() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o, lit = T.lampOverlay(function(o) o:_watchMenus({ T.O.menuTests.newWindow }) end)
        T.tab()
        assert(T.reads() == 1)
        rawset(T.host, "input", T.strict("host.input", { click = function() T.act() end }))
        rawset(T.host.window, "ownsPoint", function() return true end)
        T.calibrate(true)
        local seen = "not asked"
        S.epoch += 1
        T.call("key", function()
          o:chooseMenuItem({ label = "Menu", at = { 50, 10 }, menuItem = { 10, 20 }, retries = 0,
            onDone = function(_, chosen) seen = tostring(chosen) .. ", " .. tostring(lit()) end })
        end)
        S.pixel = T.LIT
        S.windows[#S.windows + 1] = { id = 300, layer = 0, class = "JUCE_1d4a", x = 362, y = 80, w = 120, h = 200 }
        T.tick(1)
        assert(seen == "not asked", "chosen before its picture: " .. seen)
        T.tick(1)
        assert(seen == "true, false", "onDone at the item's click, from what stands: " .. seen)
        assert(T.reads() == 1, "read in the item shot's answer: " .. T.reads())
        T.tab()
        assert(T.reads() == 2 and lit() == true)
        T.allWaitable()
    "##);
}

/// A reading of a window the overlay has left while it was taken is not kept: once a read waits, the
/// arbiter can take the overlay out of the front meanwhile, as it does here from inside the capture.
/// Nothing is stored, no hotkey is registered for the overlay that left, and the log says so. The
/// probe stays due, and the next Tab where the overlay is reads it.
#[test]
fn a_reading_the_overlay_left_while_it_was_taken_is_not_kept() {
    run(r##"
        local S = T.S
        S.pixel = T.LIT
        local o, lit = T.lampOverlay()
        local pixels = T.host.screen.pixels
        rawset(T.host.screen, "pixels", function(points)
          local got = pixels(points)
          o:_deactivate()
          return got
        end)
        o.focus = 1
        T.tab()
        assert(T.reads() == 1 and lit() == nil, "a reading of the window it left kept: " .. tostring(lit()))
        for _, spec in pairs(S.hot) do assert(spec ~= "Alt+L", "Alt+L taken for an overlay not in front") end
        assert(T.count("[probe] 'Plug-in': 1 probe(s) read and not kept — the overlay is no longer active") == 1, T.dump())
        rawset(T.host.screen, "pixels", pixels)
        T.front(o)
        T.tab()
        assert(T.reads() == 2 and lit() == true, "due still: " .. T.reads())
        T.allWaitable()
    "##);
}

/// An origin that raises reads no probe, and the Tab's scan goes on: the `when`s answer what stands,
/// and the focus moves. Before, the raise came out of the probes ahead of the scan, and the focus
/// stayed where it was. (The sentence after the move asks the origin too, and raises with it, as it
/// did before probes.)
#[test]
fn an_origin_that_raises_reads_no_probe_and_the_scan_goes_on() {
    run(r##"
        local S = T.S
        S.pixel = T.LIT
        local o = T.lampOverlay()
        o.activeCtx = { origin = function() error("the control function raised", 0) end }
        o.focus = 1
        S.tasksMayRaise = true
        T.tab()
        assert(T.reads() == 0, "read with no origin: " .. T.reads())
        assert(T.on(o) == "Three", "the Tab moved on past the hidden lamp: " .. tostring(T.on(o)))
    "##);
}

/// A probe declared `everyEpoch` is due in every epoch as well — for what the user's own keys change,
/// which turns over no input epoch: one read per Tab, and still one per handler.
#[test]
fn an_every_epoch_probe_is_read_once_per_tab() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o = T.O.new("Plug-in")
        local lit = o:probe("lamp", { { 10, 20 } }, { everyEpoch = true, read = function(c) return c[1].r > 200 end })
        o:addCustomButton({ label = "One", onActivate = function() end })
        o:addCustomButton({ label = "Lamp", when = function() return lit() == true end, onActivate = function() end })
        o:addCustomButton({ label = "Three", onActivate = function() end })
        T.front(o)
        o.focus = 1
        T.tab()
        assert(T.reads() == 1 and T.on(o) == "Three", tostring(T.on(o)))
        -- Lit by something the user typed into the plug-in: nothing acted, and the next Tab sees it.
        S.pixel = T.LIT
        o.focus = 1
        T.tab()
        assert(T.reads() == 2 and T.on(o) == "Lamp", "the lamp at the next Tab: " .. tostring(T.on(o)))
        -- One handler, two scans: read once.
        S.epoch += 1
        T.call("key", function() o:focusNext(); o:focusNext() end)
        assert(T.reads() == 3, "once in a handler: " .. T.reads())
        T.allWaitable()
    "##);
}

/// A change the module says its act will make (`O:expectChange`) is looked for at every key: the
/// first Tab after the act comes before the plug-in has redrawn and reads the lamp still dark, the
/// next one reads again and sees it lit — the control it shows comes into the ring and its key is
/// held — and the Tabs after that read nothing. Without it, an act's one reading stands until the
/// next act, as a probe's always does.
#[test]
fn a_change_expected_is_read_at_every_key_until_it_shows() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local o, lit = T.lampOverlay()
        local function lampKey()
          for _, spec in pairs(S.hot) do
            if spec == "Alt+L" then return true end
          end
          return false
        end
        o.focus = 1
        T.tab()
        assert(T.reads() == 1 and T.on(o) == "Three", tostring(T.on(o)))
        -- A press that lights the lamp: it says so, then acts.
        S.epoch += 1
        T.call("key", function()
          o:expectChange("lamp")
          T.act()
        end)
        o.focus = 1
        T.tab()
        assert(T.reads() == 2 and T.on(o) == "Three", "the Tab before the redraw reads the lamp still dark")
        o.focus = 1
        T.tab()
        assert(T.reads() == 3 and T.on(o) == "Three", "read again at the next key: " .. T.reads())
        S.pixel = T.LIT
        o.focus = 1
        T.tab()
        assert(T.reads() == 4 and T.on(o) == "Lamp", "the lamp lit at the next key: " .. tostring(T.on(o)))
        assert(lampKey(), "the control the lamp shows holds its key")
        for _ = 1, 3 do T.tab() end
        assert(T.reads() == 4, "the change read, Tabs in a row read nothing: " .. T.reads())
        -- An act that darkens the lamp and does not say so: its one reading, taken before the
        -- plug-in redrew, stands until the next act.
        S.epoch += 1
        T.call("key", function() T.act() end)
        T.tab()
        assert(T.reads() == 5 and lit() == true, "read once after the act: " .. T.reads())
        S.pixel = T.DARK
        T.tab()
        T.tab()
        assert(T.reads() == 5 and lit() == true, "once per act, the dark lamp is not read")
        T.allWaitable()
    "##);
}

/// An expected change ends for every probe named at the first kept reading of any of them that
/// differs from what that probe held at the call; nothing read yet at the call, the first kept
/// reading ends it. A reading `read` does not recognise ends nothing, nor does one the same as
/// before. On another window the change is not looked for: it ends there, and Tabs in a row read
/// nothing again.
#[test]
fn a_change_expected_ends_for_every_probe_named_once_one_of_them_shows_it() {
    run(r##"
        local S = T.S
        S.pixel = T.DARK
        local GREY = { r = 128, g = 128, b = 128 }
        local o = T.O.new("Plug-in")
        local function lamp(c)
          if c[1].r == 128 then return nil end
          return c[1].r > 200
        end
        local a = o:probe("a", { { 10, 20 } }, { read = lamp })
        local b = o:probe("b", { { 30, 40 } }, { read = lamp })
        o:addCustomButton({ label = "One", onActivate = function() end })
        o:addCustomButton({ label = "Two", onActivate = function() end })
        T.front(o)
        -- Nothing read yet: the first reading kept ends it.
        o:expectChange("a", "b")
        T.tab()
        assert(T.reads() == 1 and a() == false and b() == false, "read: " .. T.reads())
        T.tab()
        assert(T.reads() == 1, "ended by the first reading: " .. T.reads())
        -- Both named; a reads grey, which is not recognised, and b reads as before: it goes on.
        S.epoch += 1
        T.call("key", function()
          o:expectChange("a", "b")
          T.act()
        end)
        S.at["110,70"] = GREY
        T.tab()
        assert(T.reads() == 2 and a() == false, "the grey not kept")
        S.at["110,70"] = nil
        T.tab()
        assert(T.reads() == 3, "the same as before ends nothing: " .. T.reads())
        -- b lit: the change is read, for a as well.
        S.at["130,90"] = T.LIT
        T.tab()
        assert(T.reads() == 4 and b() == true and a() == false, "b lit")
        T.tab()
        T.tab()
        assert(T.reads() == 4, "ended for both: " .. T.reads())
        -- Expected on this window, and the overlay is on another: read once there, as any probe is
        -- on a new window, and not looked for again.
        S.epoch += 1
        T.call("key", function()
          o:expectChange("a")
          T.act()
        end)
        T.tab()
        assert(T.reads() == 5, "read after the act: " .. T.reads())
        local there = table.clone(S.origin)
        there.id = 5000
        S.origin = there
        T.tab()
        assert(T.reads() == 6, "read on the new window: " .. T.reads())
        T.tab()
        T.tab()
        assert(T.reads() == 6, "not looked for on another window: " .. T.reads())
        T.allWaitable()
    "##);
}

/// expectChange names one or more of the overlay's probes; anything else raises at the call.
#[test]
fn expect_change_raises_for_what_is_not_a_probe() {
    run(r##"
        local o = T.O.new("Plug-in")
        o:probe("lamp", { { 1, 2 } })
        local function raises(fragment, ...)
          local ok, err = pcall(o.expectChange, o, ...)
          assert(not ok and string.find(tostring(err), fragment, 1, true), tostring(err))
        end
        raises("overlay 'Plug-in': expectChange takes the names of one or more of its probes")
        raises("overlay 'Plug-in': expectChange takes probe names, got number", "lamp", 3)
        raises("overlay 'Plug-in': expectChange: no probe named 'lamb'", "lamp", "lamb")
        o:expectChange("lamp")
    "##);
}

/// A capture that fails reads nothing: the value stands, the probe is due again in a later epoch, and
/// the log says why once until a read succeeds.
#[test]
fn a_capture_that_fails_keeps_the_value_and_says_why_once() {
    run(r##"
        local S = T.S
        S.pixel = T.LIT
        local o, lit = T.lampOverlay()
        T.tab()
        assert(lit() == true)
        T.act()
        S.pixelsFail = "screen capture failed"
        T.tab()
        T.tab()
        assert(T.reads() == 3 and lit() == true, "due again, the value standing: " .. T.reads())
        assert(T.count("[probe] 'Plug-in': 1 probe(s) not read — screen capture failed") == 1, T.dump())
        S.pixelsFail, S.pixel = nil, T.DARK
        T.tab()
        assert(T.reads() == 4 and lit() == false, "read once the capture works")
        T.allWaitable()
    "##);
}

/// A probe is declared before the overlay is bound, under a name of its own, with points it can place
/// and only the options it has; anything else raises at the call.
#[test]
fn a_probe_that_cannot_work_raises_at_the_call() {
    run(r##"
        local o = T.O.new("Plug-in")
        local function raises(fragment, ...)
          local ok, err = pcall(o.probe, o, ...)
          assert(not ok and string.find(tostring(err), fragment, 1, true), tostring(err))
        end
        o:probe("lamp", { { 1, 2 } })
        raises("a probe named 'lamp' is declared already", "lamp", { { 1, 2 } })
        raises("probe takes a name, got nil", nil, { { 1, 2 } })
        raises("probe 'x': points[2] is not {x, y}", "x", { { 1, 2 }, { 3 } })
        raises("probe 'x' takes a list of {x, y} or a function, got string", "x", "here")
        raises("probe 'x' has no option 'reed' (it has read, rawOrigin and everyEpoch)", "x", { { 1, 2 } }, { reed = print })
        raises("probe 'x': read is a function, got number", "x", { { 1, 2 } }, { read = 1 })
        o:attach({ class = "nothing" }, {})
        raises("overlay 'Plug-in': probe must be called BEFORE binding it", "late", { { 1, 2 } })
    "##);
}
