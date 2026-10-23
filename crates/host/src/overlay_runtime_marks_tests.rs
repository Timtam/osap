//! The overlay runtime's building blocks for an answer that comes later — `O:here()`,
//! `O:stillHere(mark, opts)` and `O:toScreenRect(r, { whole = true })` — and what a control's hook
//! that raises comes to, run for real against the scripted host of `overlay_menu_tests.rs`, with the
//! focus-read helpers of `overlay_focus_read_tests.rs`.
//!
//! Written for step 3 of the plan that takes text recognition off the event loop (2026-10-04). A
//! read's callback — and a handler's code after it waited for a read — is about the moment
//! it was asked in.
//! The runtime's own announcement has always checked, before it spoke, that the overlay had not
//! moved on since; an OCR button its click before it clicked. Those checks are now one mark, which
//! a module's own code can take and check too, with the reasons in the words the `[read]` lines
//! have always used. Nothing a user hears changes: the runtime's announcements and clicks are made
//! on exactly the conditions they were — `overlay_focus_read_tests.rs` holds them to it, unchanged.
//!
//! And a hook that raises — a control's `text`, a tab control's `current` or `verticalName` — is
//! logged once per control and hook, and has no value: before, a stepper's watch compared one
//! error message with the next. The scripted host fails a scenario that leaves such a line, or that
//! asked it for anything it does not have (`finish` in `overlay_menu_tests.rs`), which is where a
//! `recognize` that cannot wait in a hook is found.
//!
//! And for the step that readies the tree for reads of the screen that wait (2026-10-10): the
//! runtime's own clicks after a pixel — a hotspot toggle's, a `reveal` probe's — are made only while
//! the overlay is still where the pixel was read, and `O.memoByEpoch` keeps an answer under the
//! epoch it came back in.
//!
//! And for the runtime's error values (version 0.11, the typed-errors series): `O:stillHere` and
//! `O:toScreenRect` answer them, O.errors holds their codes, and every one comes from a reason of
//! `modules/overlay-runtime/src/errors.luau`.

use super::overlay_focus_read_tests::FR;
use super::overlay_menu_tests::run_with;

/// A few more helpers, after the focus-read ones.
const MK: &str = r##"
local S = T.S

-- A mark's answer as one string, for messages: "true", or "false: <why>". The why is an error value,
-- `not_in_front` for an overlay no longer active and `moved_on` for every other reason.
function T.still(o, mark, opts)
  local ok, why = o:stillHere(mark, opts)
  if ok then return "true" end
  local want = tostring(why) == "the overlay is no longer active" and T.O.errors.NOT_IN_FRONT or T.O.errors.MOVED_ON
  assert(T.is(why, want), "stillHere's error value: " .. T.what(why))
  return "false: " .. tostring(why)
end

-- A key of the overlay's own on the stepper: Left (-1) or Right (+1), as the captured key delivers it.
function T.arrow(dir)
  local spec = dir < 0 and "Left" or "Right"
  assert(S.holding[spec], spec .. " is not captured, so it would not reach the overlay")
  S.epoch += 1
  T.call("key", S.captured[spec])
end

-- Time passes by `ms`, 100 ms at a time, with every timer that comes due run, as the host runs them.
function T.wait(ms)
  local stop = S.now + ms
  while S.now < stop do
    S.now += 100
    T.runDue()
  end
end
"##;

fn run(scenario: &str) {
    run_with("windows", &format!("{FR}\n{MK}"), scenario)
}

fn run_mac(scenario: &str) {
    run_with("macos", &format!("{FR}\n{MK}"), scenario)
}

// ---------------------------------------------------------------------------------------------
// O:here and O:stillHere.
// ---------------------------------------------------------------------------------------------

/// A mark holds until something it noted moves on, and says what, in the words of the `[read]`
/// lines: the focus — the focus elsewhere, or another control at its place — a key of the overlay's
/// own, an announcement. Each is asked only when its option is given; with none, only the place.
#[test]
fn a_mark_holds_until_what_it_noted_moves_on_and_says_what() {
    run(r##"
        local S = T.S
        local o = T.fields(function(o)
          o:addCustomButton({ label = "Quiet", hotkey = "Alt+Q", hotkeyKeepsFocus = true, onActivate = function() end })
        end)
        local all = { focus = true, keys = true, said = true, menus = true }
        local m = o:here()
        assert(T.still(o, m) == "true" and T.still(o, m, all) == "true", "nothing has moved on")
        assert(select("#", o:stillHere(m)) == 1, "true, and nothing after it")

        o.focus = 2
        assert(T.still(o, m, { focus = true }) == "false: the focus has moved on", T.still(o, m, { focus = true }))
        assert(T.still(o, m, { keys = true, said = true, menus = true }) == "true", "only the focus moved")
        o.focus = 1
        local preset = o.controls[1]
        o.controls[1] = o.controls[2]
        assert(T.still(o, m, { focus = true }) == "false: the focus has moved on", "another control at its place")
        o.controls[1] = preset
        assert(T.still(o, m, all) == "true")

        -- A key of the overlay's own that moves nothing and says nothing.
        S.hotkeyFns["Alt+Q"]()
        assert(o.focus == 1 and #S.speech == 0, "the key moved nothing and said nothing")
        assert(T.still(o, m, all) == "false: a later key reached the overlay", T.still(o, m, all))
        assert(T.still(o, m, { focus = true, said = true, menus = true }) == "true")

        -- An announcement with no key of the overlay's own: the module's code moving the focus there
        -- and back.
        m = o:here()
        o:focusNext(); o:focusPrev()
        assert(o.focus == 1 and #S.speech == 1, "Init said, Preset's read asked")
        assert(T.still(o, m, all) == "false: a later announcement was made", T.still(o, m, all))
        assert(T.still(o, m, { focus = true, keys = true, menus = true }) == "true")
    "##);
}

/// The place: the overlay no longer active, out of the front and back again on the same window,
/// on another window. Always asked, before anything else — and `place = false` leaves exactly these
/// three out, for an action that stays right while another window of the application is in front.
#[test]
fn the_place_is_always_asked_first_unless_place_is_false() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addCustomButton({ label = "Quiet", hotkey = "Alt+Q", hotkeyKeepsFocus = true, onActivate = function() end })
        end)
        local m = o:here()
        o.active = false
        assert(T.still(o, m) == "false: the overlay is no longer active", T.still(o, m))
        assert(T.still(o, m, { place = false }) == "true")
        assert(T.still(o, m, { place = false, focus = true, keys = true, said = true, menus = true }) == "true")
        o.active = true

        o._lastActiveId = S.origin.id
        o:_deactivate()
        o:_activate()
        assert(o.active and o.focus == 1, "back on the same window, on the same control")
        assert(T.still(o, m) == "false: the overlay left the front and came back since", T.still(o, m))
        assert(T.still(o, m, { place = false, focus = true, keys = true }) == "true")

        m = o:here()
        local was = S.origin
        S.origin = { id = 8, class = was.class, app = was.app, client = was.client, bounds = was.bounds }
        assert(T.still(o, m) == "false: the overlay is on another window now", T.still(o, m))
        assert(T.still(o, m, { place = false }) == "true")
        S.origin = was
        assert(T.still(o, m) == "true", "the same handle again")

        -- First the place, then the others, in the order of the [read] lines.
        S.hotkeyFns["Alt+Q"]()
        o.focus = 2
        o.active = false
        assert(T.still(o, m, { focus = true, keys = true }) == "false: the overlay is no longer active")
        assert(T.still(o, m, { place = false, focus = true, keys = true }) == "false: the focus has moved on")
        assert(T.still(o, m, { place = false, keys = true }) == "false: a later key reached the overlay")
    "#);
}

/// `menus`: a menu open over the overlay now, and one that opened since and closed again — after
/// which what was read before it is from before a choice made in it. A menu is not a place: the
/// overlay holds it, and only `menus` asks.
#[test]
fn menus_asks_for_a_menu_open_now_and_one_opened_since() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:addCustomButton({ label = "Init", onActivate = function() end })
        o:_watchMenus({ { name = "seen", cheap = true, test = function(_, answer) answer(S.menuUp) end } })
        T.front(o)
        T.tick(1)
        local m = o:here()
        S.menuUp = true
        T.tick(1)
        assert(o:menuOpen(), "the test sees the menu")
        assert(T.still(o, m, { menus = true }) == "false: a menu is open over the overlay", T.still(o, m, { menus = true }))
        assert(T.still(o, m, { focus = true, keys = true, said = true }) == "true", "only menus asks")
        S.menuUp = false
        T.tick(2)
        assert(not o:menuOpen(), "closed again")
        assert(T.still(o, m, { menus = true }) == "false: a menu opened over the overlay since", T.still(o, m, { menus = true }))
        assert(T.still(o, o:here(), { menus = true }) == "true", "a mark taken now")
    "#);
}

/// `spoken`: an announcement begun is not yet one said. An OCR control's sentence is said when its
/// read answers, so a mark holds for `spoken` until then — where `said` moved as it began — and not
/// once the sentence is out. A control with nothing to read is spoken as it begins.
#[test]
fn spoken_asks_for_an_announcement_said_not_only_begun() {
    run(r#"
        local S = T.S
        local o = T.fields()
        o:focusNext()
        local m = o:here()
        o:focusPrev()
        assert(o.focus == 1 and #S.reads == 1, "Preset's read is out: " .. T.dump())
        assert(T.still(o, m, { said = true }) == "false: a later announcement was made")
        assert(T.still(o, m, { spoken = true }) == "true", "begun, not said")
        T.answerList(1, { value = { status = "text", text = "Init Patch" } })
        assert(T.last().text == "Preset, button, Alt+P, Init Patch", T.last().text)
        assert(T.still(o, m, { spoken = true }) == "false: a later announcement was spoken",
          T.still(o, m, { spoken = true }))
        assert(T.still(o, m, { keys = true, menus = true }) == "true", "no key and no menu: the announcements moved")

        m = o:here()
        o:focusNext()
        assert(T.still(o, m, { spoken = true }) == "false: a later announcement was spoken", "Init is said at once")
    "#);
}

/// The same on a Mac: nothing in a mark is the platform's.
#[test]
fn on_a_mac_a_mark_is_the_same() {
    run_mac(r#"
        local o = T.fields()
        local m = o:here()
        assert(T.still(o, m, { focus = true, keys = true, said = true, menus = true }) == "true")
        T.tab()
        assert(T.still(o, m, { keys = true }) == "false: a later key reached the overlay")
    "#);
}

/// Anything but a mark of this overlay, options that are not a table, or an option it does not
/// know raises, naming the caller's line: a typo in an option would otherwise check nothing at all,
/// and a mark of another overlay compares counters that have nothing to do with this one.
#[test]
fn still_here_raises_for_anything_but_its_own_mark_and_options() {
    run(r#"
        local o = T.fields()
        local other = T.O.new("Other")
        -- Not a tail call in `f`, so the raise's level is the scenario's line.
        local function raised(f)
          local ok, err = pcall(f)
          assert(not ok, "it did not raise")
          assert(string.find(err, '^%[string "scenario"%]:%d+: '), "the caller's line: " .. err)
          return err
        end
        local err = raised(function() local r = o:stillHere({}); return r end)
        assert(string.find(err, "overlay 'Synth': stillHere takes a mark from O:here(), got table", 1, true), err)
        err = raised(function() local r = o:stillHere(nil); return r end)
        assert(string.find(err, "got nil", 1, true), err)
        err = raised(function() local r = o:stillHere(other:here()); return r end)
        assert(string.find(err, "overlay 'Synth': stillHere was handed a mark of overlay 'Other'", 1, true), err)
        err = raised(function() local r = o:stillHere(o:here(), { key = true }); return r end)
        assert(string.find(err, "stillHere has no option 'key' (it has focus, keys, said, spoken, menus and place)", 1, true), err)
        err = raised(function() local r = o:stillHere(o:here(), true); return r end)
        assert(string.find(err, "stillHere's options are a table, got boolean", 1, true), err)
    "#);
}

// ---------------------------------------------------------------------------------------------
// The runtime's own checks are the mark's, and decide what they did.
// ---------------------------------------------------------------------------------------------

/// An OCR button's click is the control's, not the focus's: a hotkey that keeps the focus where it
/// is presses it, and it clicks after its sentence all the same. Its control no longer at its index
/// when the answer comes takes the click along, and says why.
#[test]
fn an_ocr_buttons_click_goes_by_its_control_not_by_the_focus() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addOCRButton({ label = "Bank", region = { 300, 20, 400, 36 }, hotkey = "Alt+B", hotkeyKeepsFocus = true })
        end)
        assert(o.focus == 1 and S.hotkeyFns["Alt+B"], "the focus on Preset, Bank's hotkey held")
        S.hotkeyFns["Alt+B"]()
        assert(o.focus == 1 and #S.reads == 1 and #S.clicks == 0, T.dump())
        T.answerList(1, { value = { status = "text", text = "Bank A" } })
        assert(T.last().text == "Bank, button, Bank A", T.last().text)
        assert(#S.clicks == 1 and S.clicks[1][1] == 450 and S.clicks[1][2] == 78, "clicked, the focus elsewhere")

        S.hotkeyFns["Alt+B"]()
        o.controls[3] = o.controls[2]
        T.answerList(2, { value = { status = "text", text = "Bank A" } })
        assert(#S.clicks == 1, "no click for a control no longer at its place")
        assert(T.count("[read] 'Bank': not clicking — the control is not in the overlay any more") == 1, T.dump())
        assert(T.count("not spoken: the focus has moved on") == 1, "nor its sentence: " .. T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The runtime's own clicks after a pixel, and O.memoByEpoch (0.4.2).
// ---------------------------------------------------------------------------------------------

/// The scripted pixel with something happening while it is read, as it can once a read of the
/// screen waits: `T.during(fn)` runs `fn` inside the next reads, `T.during(nil)` stops it. A
/// slider's thumb search the same way: found centred at x = `S.thumb` (none while it is nil), on
/// the row its region starts at. And the overlay's window replaced by another one,
/// `T.elsewhere()`, and back, `T.backHere()`.
const DURING: &str = r##"
local S = T.S
local during = nil
rawset(T.host.screen, "pixel", function()
  T.screenWait("host.screen.pixel")
  S.pixels += 1
  if during then during() end
  return S.pixel
end)
S.searches = 0
rawset(T.host.screen, "imageSearch", function(_, opts)
  T.screenWait("host.screen.imageSearch")
  S.searches += 1
  if during then during() end
  if not S.thumb then return nil end
  return { x = S.thumb - 5, y = opts.region[2], w = 10, h = 10, n = 1 }
end)
function T.during(fn) during = fn end
local here = S.origin
function T.elsewhere()
  S.origin = { id = 8, class = here.class, app = here.app, client = here.client, bounds = here.bounds }
end
function T.backHere() S.origin = here end
"##;

fn run_during(scenario: &str) {
    run_with("windows", &format!("{FR}\n{MK}\n{DURING}"), scenario)
}

/// A hotspot toggle's press reads the pixel under it before its click, and a read of the screen can
/// wait: the click is made only while the overlay is still where the key was pressed. Another
/// window in front by the time the pixel is read takes the click away — nothing clicked, nothing
/// waiting to be announced, and the log says why. So does another window drawn over the point
/// meanwhile, with the overlay still in front: the point is asked again after the read. Nothing
/// changed, the same press clicks.
#[test]
fn a_hotspot_toggle_clicks_nothing_once_another_window_came_to_the_front_during_its_pixel() {
    run_during(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addHotspotToggle({ label = "Mute", at = { 20, 30 }, onColor = { 255, 255, 255 }, offColor = { 20, 20, 20 } })
        end)
        local timers = #S.after
        T.during(T.elsewhere)
        T.handled(o.activate, o, 3)
        assert(S.pixels == 1 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers, "nothing waits to announce it")
        assert(T.count("[overlay] Synth: 'Mute' not clicking after reading its pixel — the overlay is on another "
          .. "window now") == 1, T.dump())
        T.backHere()
        local owns = T.host.window.ownsPoint
        T.during(function() rawset(T.host.window, "ownsPoint", function() return false end) end)
        T.handled(o.activate, o, 3)
        assert(S.pixels == 2 and #S.clicks == 0, "a click on the window drawn over it: " .. T.dump())
        assert(#S.after == timers, "nothing waits to announce it")
        assert(T.count("Synth: 'Mute' at (120,80) is inside our window's frame but another window is drawn there "
          .. "— not clicking") == 1, T.dump())
        rawset(T.host.window, "ownsPoint", owns)
        T.during(nil)
        T.handled(o.activate, o, 3)
        assert(#S.clicks == 1 and S.clicks[1][1] == 120 and S.clicks[1][2] == 80, "clicked: " .. T.dump())
    "#);
}

/// A graphical toggle's `reveal` probe reads closed, and the probe is clicked to open the panel —
/// only while the overlay is still where the probe was read. Another window in front by then takes
/// the click away, and with it the toggle's own click after the panel opened; the log says why.
#[test]
fn a_reveal_probe_opens_nothing_once_another_window_came_to_the_front_during_its_pixel() {
    run_during(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addGraphicalToggle({ label = "Reverb", region = { 40, 40, 60, 60 }, onImage = "C:/on.png",
            offImage = "C:/off.png", reveal = { at = { 10, 12 }, closedColor = { 20, 20, 20 } } })
        end)
        local timers = #S.after
        T.during(T.elsewhere)
        T.handled(o.activate, o, 3)
        assert(S.pixels == 1 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers, "nothing waits to follow the probe's click")
        assert(T.count("[overlay] Synth: 'Reverb' not going on after reading its panel's probe — the overlay is "
          .. "on another window now") == 1, T.dump())
        T.during(nil)
        T.backHere()
        T.handled(o.activate, o, 3)
        assert(#S.clicks == 1 and S.clicks[1][1] == 110 and S.clicks[1][2] == 62, "the probe clicked: " .. T.dump())
        assert(#S.after == timers + 1, "the toggle's click waits for the panel")
    "#);
}

/// The same probe reading open, or not at all, lets the toggle's own click go ahead at once — and
/// that click too is made only while the overlay is still where the probe was read: another window
/// in front by then takes it away, and the log says why. Nothing changed, the toggle clicks the
/// middle of its region.
#[test]
fn a_reveal_probe_reading_open_lets_no_click_follow_once_another_window_came_to_the_front_during_it() {
    run_during(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addGraphicalToggle({ label = "Reverb", region = { 40, 40, 60, 60 }, onImage = "C:/on.png",
            offImage = "C:/off.png", reveal = { at = { 10, 12 }, closedColor = { 20, 20, 20 } } })
        end)
        local after = "[overlay] Synth: 'Reverb' not going on after reading its panel's probe — the overlay is "
          .. "on another window now"
        local timers = #S.after
        -- Open, and another window in front by the time the probe is read: the toggle is not clicked.
        S.pixel = { r = 240, g = 240, b = 240 }
        T.during(T.elsewhere)
        T.handled(o.activate, o, 3)
        assert(S.pixels == 1 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers and T.count(after) == 1, T.dump())
        -- Not read at all: the same.
        S.pixel = nil
        T.backHere()
        T.handled(o.activate, o, 3)
        assert(S.pixels == 2 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers and T.count(after) == 2, T.dump())
        -- Open, nothing changed: the toggle's click, at once, and its state read waiting for the redraw.
        S.pixel = { r = 240, g = 240, b = 240 }
        T.during(nil)
        T.backHere()
        T.handled(o.activate, o, 3)
        assert(#S.clicks == 1 and S.clicks[1][1] == 150 and S.clicks[1][2] == 100, "the toggle clicked: " .. T.dump())
        assert(#S.after == timers + 1, "the state read waits for the redraw")
    "#);
}

/// Once the probe opened the panel, the toggle's click `settle` ms later is made only while the
/// overlay is still where the probe was read: out of the front and back again on the same window
/// meanwhile, it is not made, and the log says why.
#[test]
fn a_reveal_probes_toggle_clicks_nothing_once_the_overlay_left_the_front_while_its_panel_opened() {
    run_during(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addGraphicalToggle({ label = "Reverb", region = { 40, 40, 60, 60 }, onImage = "C:/on.png",
            offImage = "C:/off.png", reveal = { at = { 10, 12 }, closedColor = { 20, 20, 20 } } })
        end)
        T.handled(o.activate, o, 3)
        assert(#S.clicks == 1 and S.clicks[1][1] == 110 and S.clicks[1][2] == 62, "the probe clicked: " .. T.dump())
        o._lastActiveId = S.origin.id
        o:_deactivate()
        o:_activate()
        assert(o.active, "back on the same window")
        S.now += 250
        T.runDue()
        assert(#S.clicks == 1, "the toggle clicked in another stay: " .. T.dump())
        assert(T.count("[overlay] Synth: 'Reverb' not going on after opening its panel — the overlay left the "
          .. "front and came back since") == 1, T.dump())
    "#);
}

/// A slider's step finds its thumb, and a thumb search waits in a handler: the drag is made only while
/// the overlay is still where the step began. Another window in front by the time the step's first
/// search answers, or its search just before the drag, takes the drag away; another by the time the
/// search after the drag answers takes away the percentage that would be said; and the log says
/// why. Nothing changed, the same step drags by one percent and says where the thumb went.
#[test]
fn a_sliders_step_drags_nothing_once_another_window_came_to_the_front_during_its_search() {
    run_during(r#"
        local S = T.S
        S.drags = {}
        rawset(T.host.input, "drag", function(x1, y1, x2, y2) S.drags[#S.drags + 1] = { x1, y1, x2, y2 } end)
        S.thumb = 145
        local o = T.fields(function(o)
          o:addSlider({ label = "Mix", region = { 10, 10, 110, 30 }, thumbImage = "C:/thumb.png", from = 10, to = 110 })
        end)
        o.focus = 3
        local timers, said = #S.after, #S.speech
        -- The step's first search.
        T.during(T.elsewhere)
        T.handled(o.adjust, o, 1)
        assert(S.searches == 1 and #S.drags == 0 and #S.after == timers and #S.speech == said, T.dump())
        assert(T.count("[overlay] Synth: 'Mix' not going on after finding its thumb — the overlay is on another "
          .. "window now") == 1, T.dump())
        -- The search just before the drag: the first one found the thumb where the step began.
        T.during(nil)
        T.backHere()
        local first = true
        T.during(function()
          if first then first = false else T.elsewhere() end
        end)
        T.handled(o.adjust, o, 1)
        assert(S.searches == 3 and #S.drags == 0 and #S.after == timers, T.dump())
        assert(T.count("[overlay] Synth: 'Mix' not going on after finding its thumb to drag it — the overlay is on "
          .. "another window now") == 1, T.dump())
        -- Dragged, and the search after the drag answers with another window in front.
        T.during(nil)
        T.backHere()
        T.handled(o.adjust, o, 1)
        assert(S.searches == 5 and #S.drags == 1 and #S.after == timers + 1, T.dump())
        local d = S.drags[1]
        assert(d[1] == 145 and d[2] == 65 and d[3] == 146, "one percent of 110..210, from the thumb's centre: " .. table.concat(d, ","))
        T.during(T.elsewhere)
        S.now += 25
        T.runDue()
        assert(S.searches == 6 and #S.speech == said, "nothing said over the other window: " .. T.dump())
        assert(T.count("[overlay] Synth: 'Mix' not going on after finding its thumb again — the overlay is on "
          .. "another window now") == 1, T.dump())
        -- Nothing changed: the step drags and says where the thumb went.
        T.during(nil)
        T.backHere()
        T.handled(o.adjust, o, 1)
        assert(#S.drags == 2, T.dump())
        S.thumb = 146
        S.now += 25
        T.runDue()
        assert(S.speech[#S.speech].text == "36 percent", "(146 - 110) / 100: " .. T.dump())
    "#);
}

/// O.memoByEpoch keeps an answer under the epoch it was given in, read when its function returns:
/// a function during whose work the epoch turned over — as other events turn it while the function
/// waits for a read — is asked once, not again at the next call in the epoch it answered in. A later
/// epoch asks again.
#[test]
fn memo_by_epoch_keeps_an_answer_under_the_epoch_it_was_given_in() {
    run(r#"
        local S = T.S
        local asked = 0
        local memo = T.O.memoByEpoch(function()
          asked += 1
          S.epoch += 1
          return "answer " .. asked
        end)
        assert(memo() == "answer 1" and asked == 1)
        assert(memo() == "answer 1" and asked == 1, "asked again in the epoch it answered in: " .. asked)
        T.turn()
        assert(memo() == "answer 2" and asked == 2, "a later epoch asks again: " .. asked)
    "#);
}

// ---------------------------------------------------------------------------------------------
// O:toScreenRect(r, { whole = true }).
// ---------------------------------------------------------------------------------------------

/// `whole = true` cuts the corners toward zero — the pixels the overlay's own focus read takes —
/// so `host.ocr.recognize` takes them as they are; an overlay with a fractional frame places fractions
/// otherwise. Toward zero left of and above the primary display too. Without `whole` the answer is
/// the plain sum it always was.
#[test]
fn whole_cuts_the_corners_toward_zero_as_the_focus_read_does() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:frame(function() return 0.5, -0.75 end)
        o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(o)
        local r = o:toScreenRect({ 34, 7, 214, 22 })
        assert(r[1] == 134.5 and r[2] == 56.25 and r[3] == 314.5 and r[4] == 71.25, T.region(r))
        local w = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(T.region(w) == "134,56,314,71", T.region(w))

        T.tab(); T.tab()
        assert(o.focus == 1 and #S.reads == 1, T.dump())
        assert(T.region(S.reads[1].regions[1]) == T.region(w), "the focus read's corners: " .. T.region(S.reads[1].regions[1]))
        T.host.ocr.recognize(w, function() end) -- the scripted read checks corners as the host does
        assert(#S.reads == 2)

        local was = S.origin
        S.origin = { id = was.id, class = was.class, app = was.app,
          client = { x = -1000, y = -400, w = 800, h = 600 }, bounds = was.bounds }
        w = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(T.region(w) == "-965,-393,-785,-378", T.region(w))
        assert(o:toScreenRect({ 34, 7, 214, 22 }, { whole = false })[1] == -965.5, "only true is whole")
        S.origin = was
    "#);
}

/// Nothing left once cut — under one across or down, or corners turned around — is nil and
/// "empty on screen" (`outside`), where the plain sum answers the corners as they are. No origin, or
/// a scale with no factor, is nil and its own reason (`unplaced`), as without `whole`. A scaled
/// overlay places whole numbers already, and `whole` changes nothing there.
#[test]
fn whole_is_nil_when_nothing_is_left_and_keeps_the_other_reasons() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:frame(function() return 0.5, 0 end)
        o:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(o)
        local r, why = o:toScreenRect({ 10, 10, 10.4, 20 }, { whole = true })
        assert(r == nil and T.is(why, T.O.errors.OUTSIDE, "empty on screen"), T.what(why))
        r, why = o:toScreenRect({ 20, 10, 10, 20 }, { whole = true })
        assert(r == nil and T.is(why, "outside", "empty on screen"), T.what(why))
        assert(T.region(o:toScreenRect({ 20, 10, 10, 20 })) == "120.5,60,110.5,70", "the plain sum, turned around as asked")

        o.active, o.activeCtx = false, false
        r, why = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(r == nil and T.is(why, T.O.errors.UNPLACED, "the overlay has no origin now"), T.what(why))

        local factor = nil
        local s = T.O.new("Scaled")
        s:scale(function() return factor end)
        s:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(s)
        r, why = s:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(r == nil and T.is(why, "unplaced", "its scale function answered nil"), T.what(why))
        factor = 1.5
        local plain, whole = s:toScreenRect({ 34, 7, 214, 22 }), s:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(T.region(plain) == T.region(whole) and T.region(whole) == "151,61,421,83", T.region(whole))
    "#);
}

// ---------------------------------------------------------------------------------------------
// A hook that raises.
// ---------------------------------------------------------------------------------------------

/// A `text` that raises: the sentence without a value, as before, and one line in the log however
/// often the control is announced — and the read before an activation is the same hook, the same
/// one line.
#[test]
fn a_text_that_raises_is_logged_once_and_has_no_value() {
    run(r#"
        local S = T.S
        S.hooksMayRaise = true
        local pressed = 0
        local o = T.fields(function(o)
          o:addStaticText({ label = "Battery", text = function() error("no battery") end })
          o:addCustomButton({ label = "Charge", text = function() error("no charger") end,
            onActivate = function() pressed += 1 end })
        end)
        T.tab(); T.tab()
        assert(o.focus == 3 and T.last().text == "Battery", T.last().text)
        T.tab()
        assert(o.focus == 4 and T.last().text == "Charge, button", T.last().text)
        T.tab(); T.tab(); T.tab(); T.tab()
        assert(o.focus == 4 and T.last().text == "Charge, button", T.last().text)
        T.ret()
        assert(pressed == 1, "the activation went on")
        assert(T.count("[overlay] 'Battery': its text raised: ") == 1, T.dump())
        assert(T.count("[overlay] 'Charge': its text raised: ") == 1, T.dump())
        assert(T.count("no battery") == 1 and T.count("no charger") == 1, T.dump())
    "#);
}

/// A tab control's `current` and `verticalName` that raise: the tab it knows, named as it is, and
/// one line each. Answering, they are taken as they were.
#[test]
fn a_tab_controls_hooks_that_raise_are_logged_once_each() {
    run(r#"
        local S = T.S
        S.hooksMayRaise = true
        local o = T.O.new("Synth")
        o:addTabControl({ label = "Tools", tabs = { { label = "Main" }, { label = "Pitch" } },
          current = function() error("no tool bar") end,
          verticalName = function() error("no variant") end })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(o)
        T.tab(); T.tab(); T.tab(); T.tab()
        assert(o.focus == 1 and T.last().text == "Main tab selected, tab control", T.last().text)
        assert(T.count("[overlay] 'Tools': its current raised: ") == 1, T.dump())
        assert(T.count("[overlay] 'Tools': its verticalName raised: ") == 1, T.dump())

        o.active = false
        local p = T.O.new("Answering")
        p:addTabControl({ label = "Tools", tabs = { { label = "Main" }, { label = "Pitch" } },
          current = function() return 2 end,
          verticalName = function(_, n) return n == 2 and "Pitch, formant" or nil end })
        p:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(p)
        T.tab(); T.tab()
        assert(p.focus == 1 and T.last().text == "Pitch, formant tab selected, tab control", T.last().text)
    "#);
}

/// A stepper's `text` that raises, with another message each time: the watch after a step has no
/// reading — nil, "cannot read right now" — and gives up at the control's `settle`, where it used to
/// compare one error message with the next and announce the change between them at the first look.
/// The value is then said as it is, without the hook's part. One line for the hook.
#[test]
fn a_steppers_watch_does_not_compare_error_messages() {
    run(r#"
        local S = T.S
        S.hooksMayRaise = true
        local calls, steps = 0, 0
        local o = T.O.new("Synth")
        o:addStepper({ label = "Tone", settle = 300,
          text = function() calls += 1; error("unreadable " .. calls) end,
          onStep = function() steps += 1 end })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(o)
        T.tab(); T.tab()
        assert(o.focus == 1 and T.last().text == "Tone, slider", T.last().text)
        local said = #S.speech
        T.arrow(1)
        assert(steps == 1, "the step was made")
        T.wait(200)
        assert(#S.speech == said, "no announcement at the first looks: " .. T.said(said))
        T.wait(300)
        assert(T.count("[watch] Synth: gave up after") == 1 and T.count("[watch] Synth: changed") == 0, T.dump())
        assert(T.count(" — nil -> nil") == 1, "no reading, not an error's text: " .. T.dump())
        assert(#S.speech == said + 1 and T.last().text == "Tone, slider", T.said(said))
        assert(T.count("[overlay] 'Tone': its text raised: ") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The scripted host's own end: what it lacks, and a hook that raised, fail a scenario.
// ---------------------------------------------------------------------------------------------

/// A member the scripted host does not have fails the scenario at its end, even when a `pcall`
/// swallowed the raise — the runtime calls every hook of a module in one.
#[test]
#[should_panic(expected = "the scripted host was asked for what it does not have: host.ocr.nothing")]
fn a_member_the_host_lacks_fails_the_scenario_even_inside_a_pcall() {
    run(r#"
        local ok = pcall(function() return T.host.ocr.nothing end)
        assert(not ok, "it raised")
    "#);
}

/// A `recognize` without a callback in a `text` waits in the handler of the key that asked for the
/// sentence: the key's handler parks, nothing is said meanwhile, and once the reading comes the
/// sentence is said with it — the mark taken before the hook still holds.
#[test]
fn a_recognize_in_a_text_hook_waits_and_its_reading_is_said() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addStaticText({ label = "Battery",
            text = function() return T.host.ocr.recognize({ region = { 9751, 5, 9781, 15 } }).text end })
        end)
        T.tab()
        local said = #S.speech
        T.tab()
        assert(o.focus == 3 and #S.speech == said, "nothing is said before the reading: " .. T.dump())
        T.settle()
        assert(T.last().text == "Battery, 9751,5", T.last().text)
    "#);
}

// ---------------------------------------------------------------------------------------------
// The runtime's version.
// ---------------------------------------------------------------------------------------------

/// The runtime is 0.12.0 (calibration as its own setting, `O.calibrating()`, a static text's
/// `hotkey`, the checks after its own pixels, `O:probe`, letting go in front of this
/// application's own windows, its accessibility menu tests asked off the event loop, the
/// check after a calibrating run's waited picture before a press, a slider's after each of its
/// searches, its searches and pictures through the calls' callback forms, an OCR button's `value`,
/// an overlay's page, `identify`'s `again` and O:watch's count of readings, and the drawn menu
/// test with a test's `before`, `O.doubleClick`'s button and callback, a trip's hand-over, a menu
/// item's submenu with a test's `chained`, its error values with O.errors, and a hotkey's press
/// handed on to a control's `onActivate` and a hand-over's `onKey`), so a module that depends on
/// `"com.platform.overlay >= 0.2"` — as one that takes a mark has to — still loads against it, and
/// so does one that asks for 0.4, 0.4.1, 0.5, 0.5.6, 0.6, 0.7, 0.8, 0.9, 0.10, 0.11 or 0.12, and
/// neither against anything older.
#[test]
fn the_runtime_is_0_12_so_a_module_that_asks_for_0_2_loads() {
    let m = module_manifest::ModuleManifest::parse(include_str!("../../../modules/overlay-runtime/module.toml"))
        .expect("the runtime's manifest parses");
    assert_eq!(m.id, "com.platform.overlay");
    assert_eq!(m.version, "0.12.0");
    for spec in [
        "com.platform.overlay >= 0.2",
        "com.platform.overlay >= 0.3",
        "com.platform.overlay >= 0.4",
        "com.platform.overlay >= 0.4.1",
        "com.platform.overlay >= 0.5",
        "com.platform.overlay >= 0.5.6",
        "com.platform.overlay >= 0.6",
        "com.platform.overlay >= 0.7",
        "com.platform.overlay >= 0.8",
        "com.platform.overlay >= 0.9",
        "com.platform.overlay >= 0.10",
        "com.platform.overlay >= 0.11",
        "com.platform.overlay >= 0.12",
    ] {
        let req = module_manifest::dep_constraint(spec).unwrap();
        let id = module_manifest::dep_id(spec);
        assert!(crate::check_dep_version("com.example.x", id, req, &m.version).is_ok(), "{spec}");
        assert!(crate::check_dep_version("com.example.x", id, req, "0.1.0").is_err(), "{spec}");
    }
}

// ---------------------------------------------------------------------------------------------
// A static text's hotkey (0.4.1).
// ---------------------------------------------------------------------------------------------

/// A read-out with a key of its own: pressing it moves the focus there and says the control,
/// interrupting, without the key's name — and Space and Return, which a static text never takes,
/// stay the application's. With `hotkeyKeepsFocus` it is said and the focus stays where it was.
#[test]
fn a_static_texts_hotkey_moves_the_focus_to_it_and_says_it() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addStaticText({ label = "Analysis", hotkey = "Alt+A", text = function() return "idle" end })
          o:addStaticText({ label = "Level", hotkey = "Alt+L", hotkeyKeepsFocus = true,
            text = function() return "-6 dB" end })
        end)
        assert(S.holding["Space"], "Space is the OCR button's while the focus is on it")
        local said = #S.speech
        S.hotkeyFns["Alt+A"]()
        assert(o.focus == 3, "the focus moved to the read-out: " .. o.focus)
        assert(T.said(said) == "Analysis, idle", T.said(said))
        assert(T.last().interrupt == true, "said interrupting")
        assert(not S.holding["Space"] and not S.holding["Return"], "Space and Return are not taken there")
        said = #S.speech
        S.hotkeyFns["Alt+L"]()
        assert(o.focus == 3, "the focus stayed: " .. o.focus)
        assert(T.said(said) == "Level, -6 dB", T.said(said))
    "#);
}

// ---------------------------------------------------------------------------------------------
// The runtime's error values (0.11).
// ---------------------------------------------------------------------------------------------

/// O.errors: the runtime's eight codes as constants, every host code beside them through
/// host.errors, a name that is neither raising with the list, read-only, `pairs` giving the eight.
/// And the error values the runtime makes are the host's kind: read-only, `tostring` and `..` the
/// message, `err.code` a code of O.errors.
#[test]
fn o_errors_holds_the_runtimes_codes_and_answers_the_hosts() {
    run(r#"
        local E = T.O.errors
        assert(E.NOT_IN_FRONT == "not_in_front" and E.MOVED_ON == "moved_on" and E.UNPLACED == "unplaced"
          and E.CLOSED == "closed" and E.NO_MENU == "no_menu" and E.NO_SUBMENU == "no_submenu"
          and E.UNFIT_SUBMENU == "unfit_submenu" and E.CLASH == "clash", "the runtime's eight")
        assert(E.BUSY == T.host.errors.BUSY and E.FAILED == "failed" and E.NOT_READY == "not_ready", "the host's beside them")
        local ok, err = pcall(function() return E.NO_MENNU end)
        assert(not ok and string.find(tostring(err), "O.errors has no 'NO_MENNU'; its codes are NOT_IN_FRONT, MOVED_ON, "
          .. "UNPLACED, CLOSED, NO_MENU, NO_SUBMENU, UNFIT_SUBMENU, CLASH and every one of host.errors's", 1, true), tostring(err))
        ok, err = pcall(function() return E.new end)
        assert(not ok and string.find(tostring(err), "O.errors has no 'new'", 1, true), "host.errors.new is no code: " .. tostring(err))
        ok = pcall(function() E.MINE = "mine" end)
        assert(not ok, "read-only")
        local n = 0
        for k, v in pairs(E) do
          n += 1
          assert(type(v) == "string" and string.upper(v) == k, k)
        end
        assert(n == 8, "pairs gives the runtime's eight: " .. n)
        -- One of the runtime's own, as a module gets it.
        local o = T.O.new("Synth")
        local r, why = o:toScreenRect({ 1, 2, 3, 4 })
        assert(r == nil and T.is(why, E.UNPLACED, "the overlay has no origin now"), T.what(why))
        assert("not placed: " .. why == "not placed: the overlay has no origin now" and tostring(why) == why.message)
        assert(why.detail == nil and not pcall(function() why.code = "x" end), "read-only, no detail")
    "#);
}

/// Every reason of errors.luau: a code of the host's or one of the runtime's eight, a message, and
/// entries that are strings; every code of the eight carried by some reason. And main.luau makes
/// its error values only through `fail` — one `host.errors.new` in the file — and names no reason
/// errors.luau does not have, while every reason is named somewhere in main.luau: one the runtime
/// never gives would sit in a docs table for nothing.
#[test]
fn every_reason_of_the_runtime_is_well_formed_and_given() {
    use super::overlay_menu_tests::{RUNTIME, RUNTIME_ERRORS};
    let lua = mlua::Lua::new();
    let t: mlua::Table = lua.load(RUNTIME_ERRORS).set_name("errors.luau").eval().expect("errors.luau loads");
    let codes: Vec<String> = t.get::<Vec<String>>("codes").unwrap();
    assert_eq!(codes.len(), 8, "the runtime's eight codes");
    let host: Vec<&str> = crate::errors::Code::ALL.iter().map(|c| c.as_str()).collect();
    for c in &codes {
        assert!(!host.contains(&c.as_str()), "{c} is a host code already");
    }
    let reasons: mlua::Table = t.get("reasons").unwrap();
    let mut used = std::collections::HashSet::new();
    let mut keys = Vec::new();
    for pair in reasons.pairs::<String, mlua::Table>() {
        let (key, r) = pair.unwrap();
        let code: String = r.get("code").unwrap();
        let message: String = r.get("message").unwrap();
        let entries: Vec<String> = r.get("entries").unwrap();
        assert!(host.contains(&code.as_str()) || codes.contains(&code), "{key}: {code} is no code");
        assert!(!message.is_empty(), "{key}: no message");
        let _ = entries;
        used.insert(code);
        keys.push(key);
    }
    for c in &codes {
        assert!(used.contains(c), "no reason carries {c}");
    }
    assert_eq!(RUNTIME.matches("host.errors.new(").count(), 1, "error values only through `fail`");
    let named = |key: &str| RUNTIME.contains(&format!("\"{key}\""));
    for key in &keys {
        // O:pageKey's are named by the word host.keys.check gives: `"key_" .. word`.
        let word = key.strip_prefix("key_").filter(|w| *w != "clash");
        let given = match word {
            Some(w) => RUNTIME.contains("fail(\"key_\" .. refused") && RUNTIME.contains(&format!("  {w} = \"")),
            None => named(key),
        };
        assert!(given, "errors.luau's '{key}' is given nowhere in main.luau");
    }
    let mut at = 0;
    while let Some(i) = RUNTIME[at..].find("fail(\"") {
        let start = at + i + "fail(\"".len();
        let end = start + RUNTIME[start..].find('"').unwrap();
        let key = &RUNTIME[start..end];
        assert!(key == "key_" || keys.iter().any(|k| k == key), "main.luau fails with '{key}', which errors.luau has not");
        at = end;
    }
}

/// Every `O.errors.X` and `host.errors.X` a module, a tool or an example names is a constant there:
/// O.errors raises for a name it does not have, and host.errors answers nil, but only when the line
/// runs — and some of those lines run only where the host or the runtime refuses, on a user's
/// machine, never in a test.
#[test]
fn every_error_constant_a_module_names_is_one() {
    use super::overlay_menu_tests::RUNTIME_ERRORS;
    let lua = mlua::Lua::new();
    let t: mlua::Table = lua.load(RUNTIME_ERRORS).set_name("errors.luau").eval().expect("errors.luau loads");
    let runtime: Vec<String> = t.get::<Vec<String>>("codes").unwrap().iter().map(|c| c.to_uppercase()).collect();
    let host: Vec<&str> = crate::errors::Code::ALL.iter().map(|c| c.constant()).collect();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut stack: Vec<std::path::PathBuf> = ["modules", "tools", "examples"].iter().map(|d| root.join(d)).collect();
    let (mut named, mut wrong) = (0, Vec::new());
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "luau") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (table, known) in [("O.errors.", &runtime), ("host.errors.", &Vec::new())] {
                let mut at = 0;
                while let Some(i) = text[at..].find(table) {
                    let start = at + i;
                    at = start + table.len();
                    if text[..start].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                        continue;
                    }
                    let name: String = text[at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    if name.is_empty() || (table == "host.errors." && name == "new") {
                        continue;
                    }
                    named += 1;
                    if !host.contains(&name.as_str()) && !known.contains(&name) {
                        wrong.push(format!("{}: {table}{name}", path.strip_prefix(&root).unwrap_or(&path).display()));
                    }
                }
            }
        }
    }
    assert!(named > 20, "only {named} constants named: the walk missed the modules");
    assert!(wrong.is_empty(), "constants that do not exist: {wrong:#?}");
}

/// A key the host cannot capture now — no keyboard hook, or on a Mac no event tap: `nil, err`, with
/// nothing registered — is held by nothing, asked again at the next sync, and said once per overlay
/// and code; and a pair that only means something together, a stepper's Left and Right, is taken
/// whole or not at all, a half let go at once (before, a Left that raised left Right captured with
/// nothing to release it by).
#[test]
fn a_key_the_host_cannot_capture_is_held_by_nothing_and_a_pair_is_whole_or_none() {
    run(r#"
        local S = T.S
        S.captureRaises = { Right = "the keyboard hook is not running" }
        local o = T.O.new("Synth")
        o:addStepper({ label = "Tone", text = function() return "5" end, onStep = function() end })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(o)
        T.tab(); T.tab()
        assert(o.focus == 1, "on the stepper: " .. o.focus)
        assert(not S.holding["Left"] and not S.holding["Right"], "neither half held")
        assert(T.count("[keys] 'Synth': Right is not captured — the keyboard hook is not running") == 1, T.dump())
        T.tab(); T.tab()
        assert(o.focus == 1 and not S.holding["Left"], "asked again, and still not whole")
        assert(T.count("is not captured") == 1, "said once per code: " .. T.dump())
        S.captureRaises = nil
        T.tab(); T.tab()
        assert(S.holding["Left"] and S.holding["Right"], "whole, once the host captures both")
    "#);
}

/// The calibration and capture methods answer why they did nothing (D40): the overlay not active
/// (`not_in_front`), no such control or none focused (`not_found`), nothing to crop around
/// (`unplaced`), and host.screen.save's own error where the picture was not written — the same
/// table, for captureAll the first shot's of all it tried; calibrationVerify answers the count, or
/// nil and the search's own error.
#[test]
fn the_calibration_and_capture_methods_answer_why_they_did_not() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 } })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        local ok, err = o:captureControl("Preset", "C:/x.png")
        assert(ok == false and T.is(err, "not_in_front", "the overlay is not active"), T.what(err))
        ok, err = o:captureFull("C:/x.png")
        assert(ok == false and T.is(err, "not_in_front"), T.what(err))
        T.front(o)
        ok, err = o:captureControl("Nope", "C:/x.png")
        assert(ok == false and T.is(err, "not_found", "no control labelled 'Nope' with a region"), T.what(err))
        ok, err = T.handled(o.captureControl, o, "Preset", "C:/x.png")
        assert(ok == true and err == nil, "saved")
        local unwritten = T.E("failed", "screen.save 'C:/x.png': the file could not be written")
        local save = T.host.screen.save
        rawset(T.host.screen, "save", function(path, opts) save(path, opts); return false, unwritten end)
        ok, err = T.handled(o.captureRegion, o, { 0, 0, 10, 10 }, "C:/x.png")
        assert(ok == false and rawequal(err, unwritten), "save's own error, handed on: " .. T.what(err))
        -- captureAll tries every shot — the content and Preset's box — and answers the first that
        -- was not written.
        local later = T.E("failed", "screen.save 'C:/shots/ctl-Preset.png': the file could not be written")
        local tried = {}
        local function saving(answers)
          tried = {}
          rawset(T.host.screen, "save", function(path, opts)
            save(path, opts); table.insert(tried, path)
            local a = answers[#tried]
            if a == true then return true end
            return false, a
          end)
        end
        saving({ unwritten, later })
        ok, err = T.handled(o.captureAll, o, "C:/shots")
        assert(ok == false and rawequal(err, unwritten), "the first shot's error: " .. T.what(err))
        assert(#tried == 2 and tried[1] == "C:/shots/content-full.png" and tried[2] == "C:/shots/ctl-Preset.png",
          table.concat(tried, ", "))
        saving({ true, later })
        ok, err = T.handled(o.captureAll, o, "C:/shots")
        assert(ok == false and rawequal(err, later), "a later shot's error: " .. T.what(err))
        saving({ true, true })
        ok, err = T.handled(o.captureAll, o, "C:/shots")
        assert(ok == true and err == nil and #tried == 2, "every shot written: " .. T.what(err))
        rawset(T.host.screen, "save", function(path, opts) save(path, opts); return false, unwritten end)
        o.focus = 0
        ok, err = o:calibrationCrop()
        assert(ok == false and T.is(err, "not_found", "no control is focused"), T.what(err))
        o.focus = 2
        ok, err = o:calibrationCrop()
        assert(ok == false and T.is(err, "unplaced", "the focused control has no position to crop around"), T.what(err))
        o.focus = 1
        ok, err = T.handled(o.calibrationCrop, o)
        assert(ok == false and rawequal(err, unwritten), T.what(err))
        rawset(T.host.screen, "save", save)
        rawset(T.host.screen, "imageSearchAll", function() return {}, T.E("failed", "the template could not be read") end)
        local n, verr = T.handled(o.calibrationVerify, o)
        assert(n == nil and T.is(verr, "failed", "the template could not be read"), T.what(verr))
        assert(T.count("[calibrate] Preset: no matches counted for C:/modules/overlay-runtime/calibration/Preset.png — "
          .. "the template could not be read") == 1, T.dump())
        rawset(T.host.screen, "imageSearchAll", function() return { { x = 1, y = 2, w = 3, h = 4 } } end)
        n, verr = T.handled(o.calibrationVerify, o)
        assert(n == 1 and verr == nil, tostring(n))
    "#);
}

// ---------------------------------------------------------------------------------------------
// A control's hotkey hands its press on (0.12).
// ---------------------------------------------------------------------------------------------

/// The host's count of a hotkey's quick repeats reaches the control the hotkey activates:
/// `onActivate(o, press)`, a custom button's and a stepper's, after the wait for the modifiers.
/// Return, and a module's own `O:activate()`, hand on none; `O:activate(i, press)` hands on what
/// it is given.
#[test]
fn a_controls_hotkey_hands_its_press_to_on_activate() {
    run(r#"
        local S = T.S
        local heard = {}
        local function note(word)
          return function(_, press) heard[#heard + 1] = word .. " " .. (press and tostring(press.count) or "none") end
        end
        local o = T.fields(function(o)
          o:addCustomButton({ label = "Twice", hotkey = "Alt+W", onActivate = note("twice") })
          o:addStepper({ label = "Level", hotkey = "Alt+L", text = function() return "3" end,
            onStep = function() end, onActivate = note("level") })
        end)
        S.hotkeyFns["Alt+W"]({ count = 1 })
        S.hotkeyFns["Alt+W"]({ count = 2 })
        assert(o.focus == 3, "the hotkey moved the focus to its control: " .. o.focus)
        S.hotkeyFns["Alt+L"]({ count = 3 })
        S.hotkeyFns["Alt+L"]()
        o.focus = 3
        T.ret()
        o:activate()
        o:activate(3, { count = 5 })
        T.wait(1000)
        assert(table.concat(heard, " | ") == "twice 1 | twice 2 | level 3 | level none | twice none | twice none | twice 5",
          table.concat(heard, " | "))
    "#);
}
