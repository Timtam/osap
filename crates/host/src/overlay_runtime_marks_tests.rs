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

use super::overlay_focus_read_tests::FR;
use super::overlay_menu_tests::run_with;

/// A few more helpers, after the focus-read ones.
const MK: &str = r##"
local S = T.S

-- A mark's answer as one string, for messages: "true", or "false: <why>".
function T.still(o, mark, opts)
  local ok, why = o:stillHere(mark, opts)
  return ok and "true" or ("false: " .. tostring(why))
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
/// screen waits: `T.during(fn)` runs `fn` inside the next reads, `T.during(nil)` stops it. And the
/// overlay's window replaced by another one, `T.elsewhere()`, and back, `T.backHere()`.
const DURING: &str = r##"
local S = T.S
local during = nil
rawset(T.host.screen, "pixel", function()
  S.pixels += 1
  if during then during() end
  return S.pixel
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
        o:activate(3)
        assert(S.pixels == 1 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers, "nothing waits to announce it")
        assert(T.count("[overlay] Synth: 'Mute' not clicking after reading its pixel — the overlay is on another "
          .. "window now") == 1, T.dump())
        T.backHere()
        local owns = T.host.window.ownsPoint
        T.during(function() rawset(T.host.window, "ownsPoint", function() return false end) end)
        o:activate(3)
        assert(S.pixels == 2 and #S.clicks == 0, "a click on the window drawn over it: " .. T.dump())
        assert(#S.after == timers, "nothing waits to announce it")
        assert(T.count("Synth: 'Mute' at (120,80) is inside our window's frame but another window is drawn there "
          .. "— not clicking") == 1, T.dump())
        rawset(T.host.window, "ownsPoint", owns)
        T.during(nil)
        o:activate(3)
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
        o:activate(3)
        assert(S.pixels == 1 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers, "nothing waits to follow the probe's click")
        assert(T.count("[overlay] Synth: 'Reverb' not going on after reading its panel's probe — the overlay is "
          .. "on another window now") == 1, T.dump())
        T.during(nil)
        T.backHere()
        o:activate(3)
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
        o:activate(3)
        assert(S.pixels == 1 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers and T.count(after) == 1, T.dump())
        -- Not read at all: the same.
        S.pixel = nil
        T.backHere()
        o:activate(3)
        assert(S.pixels == 2 and #S.clicks == 0, "a click in the other window: " .. T.dump())
        assert(#S.after == timers and T.count(after) == 2, T.dump())
        -- Open, nothing changed: the toggle's click, at once, and its state read waiting for the redraw.
        S.pixel = { r = 240, g = 240, b = 240 }
        T.during(nil)
        T.backHere()
        o:activate(3)
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
        o:activate(3)
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
/// "empty on screen", where the plain sum answers the corners as they are. No origin, or a scale
/// with no factor, is nil and its own reason, as without `whole`. A scaled overlay places whole
/// numbers already, and `whole` changes nothing there.
#[test]
fn whole_is_nil_when_nothing_is_left_and_keeps_the_other_reasons() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:frame(function() return 0.5, 0 end)
        o:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(o)
        local r, why = o:toScreenRect({ 10, 10, 10.4, 20 }, { whole = true })
        assert(r == nil and why == "empty on screen", tostring(why))
        r, why = o:toScreenRect({ 20, 10, 10, 20 }, { whole = true })
        assert(r == nil and why == "empty on screen", tostring(why))
        assert(T.region(o:toScreenRect({ 20, 10, 10, 20 })) == "120.5,60,110.5,70", "the plain sum, turned around as asked")

        o.active, o.activeCtx = false, false
        r, why = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(r == nil and why == "the overlay has no origin now", tostring(why))

        local factor = nil
        local s = T.O.new("Scaled")
        s:scale(function() return factor end)
        s:addCustomButton({ label = "Init", onActivate = function() end })
        T.front(s)
        r, why = s:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
        assert(r == nil and why == "its scale function answered nil", tostring(why))
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

/// The runtime is 0.5.0 (calibration as its own setting, `O.calibrating()`, a static text's
/// `hotkey`, the checks after its own pixels, and `O:probe`), so a module that depends on
/// `"com.platform.overlay >= 0.2"` — as one that takes a mark has to — still loads against it, and
/// so does one that asks for 0.4, 0.4.1 or 0.5, and neither against anything older.
#[test]
fn the_runtime_is_0_5_so_a_module_that_asks_for_0_2_loads() {
    let m = module_manifest::ModuleManifest::parse(include_str!("../../../modules/overlay-runtime/module.toml"))
        .expect("the runtime's manifest parses");
    assert_eq!(m.id, "com.platform.overlay");
    assert_eq!(m.version, "0.5.0");
    for spec in [
        "com.platform.overlay >= 0.2",
        "com.platform.overlay >= 0.3",
        "com.platform.overlay >= 0.4",
        "com.platform.overlay >= 0.4.1",
        "com.platform.overlay >= 0.5",
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
