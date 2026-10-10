//! Choosing an item from a plug-in's own popup menu, run for real against the scripted host of
//! `overlay_menu_tests.rs`.
//!
//! Written for the maintainer's decisions of 2026-09-28, the first stage of VPS Avenger: its MENU,
//! its zoom list and its redo list are popups JUCE draws as windows of their own, and an item in
//! one is chosen by clicking the opener and then the item. As a click sequence the second click is
//! refused — it lands on another window than the plug-in's — and the wait between the two would
//! be a timer deciding that the menu is up. So: the opener is placed, checked and clicked; the item
//! is clicked when a menu test SEES the menu and says where it is (newWindow answers with the
//! window that appeared), at an offset from the menu's corner, checked against the menu's own
//! window; and when no test places it — no menu seen, a menu that says nothing of where it is, an
//! item outside it — nothing is clicked, the log says why and the user hears it. No timer decides
//! any of it: the one bound is how long a press may wait for its menu, the time the tests are asked
//! on every tick after it.
//!
//! And for the review that followed: the item counts as chosen only once the menu is seen to have
//! taken it — JUCE ignores a mouse-up that comes too soon after its popup opened, so the same menu
//! still there a tick after the click is clicked again, a few times, and then said; a second press
//! of the same control while its menu is awaited clicks nothing; the item's offset is placed at the
//! factor the opener was placed with, and a `menuItem` function answers screen pixels; a cheap test
//! that does not say where holds back none that does; a menu that is a window is asked about on a
//! Mac as well, by its number in the window list; and every way a pick can end is held.

use super::overlay_menu_tests::run_with;

/// The helpers, added to the harness's `T`.
const MI: &str = r##"
local S = T.S
S.clicks = {}
S.owns = true
S.ownsAsked = {}

rawset(T.host, "input", T.strict("host.input", {
  click = function(x, y, opts)
    S.clicks[#S.clicks + 1] = { x, y, button = opts and opts.button }
    S.order[#S.order + 1] = "click"
  end,
}))
-- Records what it was asked, `listed` included: a window as windowsOf numbers it.
rawset(T.host.window, "ownsPoint", function(id, x, y, opts)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y, listed = opts and opts.listed }
  if type(S.owns) == "function" then return S.owns(id, x, y) end
  return S.owns
end)
rawset(T.host.element, "rawDump", function() return {} end)
rawset(T.host.screen, "saveMarked", function(path, opts)
  S.order[#S.order + 1] = "save"
  local marks = {}
  for _, m in ipairs(opts.marks or {}) do marks[#marks + 1] = { m.x, m.y } end
  S.shots[#S.shots + 1] = { path = path, snapshot = opts.snapshot, marks = marks, at = S.now }
  return true
end)

-- Avenger's header over S.origin (client 100,50 800x600): "Load preset", the MENU at (262,14)
-- choosing the item `item` (default 10,20 from the menu's corner), and "Undo". `tests` its menu
-- tests; `scale` a factor function, when given.
function T.header(tests, item, scale)
  local o = T.O.new("Avenger")
  if scale then o:scale(scale) end
  o:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = item or { 10, 20 } })
  o:addHotspotButton({ label = "Undo", at = { 305, 14 } })
  if tests then o:_watchMenus(tests) end
  T.front(o)
  return o
end

-- A popup window of the plug-in's process appears: id `id`, at x, y, w x h.
function T.popup(id, x, y, w, h)
  S.windows[#S.windows + 1] = { id = id, layer = 0, class = "JUCE_1d4a", x = x, y = y, w = w, h = h }
end

-- Every popup has gone.
function T.closePopups() S.windows = { S.windows[1] } end

function T.clickAt(i)
  local c = S.clicks[i]
  return c and (c[1] .. "," .. c[2]) or "none"
end

function T.said(from)
  local out = {}
  for i = from or 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

-- A stepper over a list, as Avenger's zoom is: each step chooses the entry `dir` rows from the
-- seventh, and records what onDone said in S.done. `spec` is merged into chooseMenuItem's.
function T.stepper(spec)
  S.done = {}
  local o = T.O.new("Avenger")
  o:addStepper({ label = "Zoom", settle = 3000,
    text = function() return S.zoom end,
    onStep = function(dir, ov)
      local s = { label = "Zoom", at = { 21, 14 },
        menuItem = function(_, menu) return { 5, 6 + 11.5 * (6 + dir) } end,
        onDone = function(_, chosen, why) S.done[#S.done + 1] = tostring(chosen) .. ":" .. tostring(why) end }
      for k, v in pairs(spec or {}) do s[k] = v end
      if not ov:chooseMenuItem(s) then return false end
    end })
  o:_watchMenus({ T.O.menuTests.newWindow })
  T.front(o)
  return o
end

-- Right on the stepper, as the captured key delivers it.
function T.right()
  S.epoch += 1
  T.call("key", S.captured["Right"])
end
"##;

fn run(scenario: &str) {
    run_with("windows", MI, scenario)
}

fn run_mac(scenario: &str) {
    run_with("macos", MI, scenario)
}

// ---------------------------------------------------------------------------------------------
// The item is chosen when a test sees the menu, and counts once the menu has taken it.
// ---------------------------------------------------------------------------------------------

/// The opener is clicked at the press; nothing more until newWindow sees the popup; then the item,
/// at its offset from the popup's corner, after asking the popup's own window whether it is drawn
/// there — on the tick it is seen, with no wait. The popup gone after the click is the item taken.
#[test]
fn the_item_is_chosen_on_the_tick_a_test_sees_the_menu() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        assert(#S.clicks == 1 and T.clickAt(1) == "362,64", "the opener: " .. T.clickAt(1))
        assert(S.speech[#S.speech].text == "Load preset, activated")
        T.tick(3)
        assert(#S.clicks == 1, "no menu, no item: nothing is clicked on a timer")
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 2 and T.clickAt(2) == "372,100", "the menu's corner + (10,20): " .. T.clickAt(2))
        local asked = S.ownsAsked[#S.ownsAsked]
        assert(asked[1] == 300 and asked[2] == 372 and asked[3] == 100, "asked of the menu's window")
        assert(T.open() == true, "and the menu counts as open, the keys its")
        assert(T.count("[menu item] 'Avenger': 'Load preset' — test 'newWindow' sees the menu at "
          .. "362,80 120x200 (window id 300) (its window id 300 is asked what is drawn there); "
          .. "choosing item (10,20) at (372,100)") == 1, T.dump())
        T.closePopups()
        T.tick(2)
        assert(#S.clicks == 2, "clicked once")
        assert(T.open() == false, "closed")
        assert(T.count("[menu item] 'Avenger': 'Load preset' chosen — clicked 1 time(s) at (372,100); "
          .. "the menu has closed") == 1, T.dump())
        assert(T.said() == "Load preset, activated", "nothing else is said: " .. T.said())
    "##);
}

/// JUCE ignores a mouse-up that comes too soon after its popup opened. The same menu still there
/// on the next tick is a click it did not take: the item is clicked again — and not on an answer
/// asked on the click's own tick, which a second test on the list gives. After the last retry the
/// user hears that the menu did not take the choice.
#[test]
fn a_click_the_menu_did_not_take_is_made_again_and_then_said() {
    run(r##"
        local S = T.S
        -- A second test answering the same menu without an id, on the same round.
        local same = { name = "same rect", cheap = true, test = function(_, answer)
          local w = S.windows[2]
          answer(w and { x = w.x, y = w.y, w = w.w, h = w.h } or false)
        end }
        local o = T.header({ T.O.menuTests.newWindow, same })
        T.press(o)
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 2, "one click on the tick the menu is seen, whatever else answers: " .. #S.clicks)
        -- Taken on the second click: chosen.
        T.tick(1)
        assert(#S.clicks == 3 and T.clickAt(3) == "372,100", "again: " .. T.clickAt(3))
        assert(T.count("[menu item] 'Avenger': 'Load preset' — test 'newWindow' still sees the menu at "
          .. "362,80 120x200 a tick after the item's click, which it did not take: clicking (372,100) "
          .. "again, 2 of 4") == 1, T.dump())
        T.closePopups()
        T.tick(2)
        assert(T.count("chosen — clicked 2 time(s) at (372,100); the menu has closed") == 1, T.dump())
        -- Never taken: four clicks, then said.
        T.press(o)
        T.popup(301, 362, 80, 120, 200)
        T.run(1500)
        assert(#S.clicks == 3 + 1 + 4, "the opener and four clicks on the item: " .. #S.clicks)
        assert(S.speech[#S.speech].text == "Load preset: the menu did not take the choice", T.said())
        assert(T.count("not chosen — the menu was still there after 4 click(s) on the item at (372,100)") == 1, T.dump())
    "##);
}

/// `retries = 0` for a menu whose item keeps it open: chosen at its click, and never clicked again.
/// Another window in the menu's place after the click is the item taken as well.
#[test]
fn retries_zero_is_chosen_at_the_click_and_another_window_in_its_place_is_taken() {
    run(r##"
        local S = T.S
        S.zoom = "80%"
        local o = T.stepper({ retries = 0 })
        T.right()
        T.popup(300, 121, 72, 60, 250)
        T.tick(1)
        assert(#S.clicks == 2 and S.done[1] == "true:nil", "chosen at the click: " .. tostring(S.done[1]))
        T.tick(3)
        assert(#S.clicks == 2, "not clicked again")
        T.closePopups()
        T.tick(2)
        -- Followed (the default): a dialog of the process in the list's place is the choice taken.
        local p = T.stepper({})
        S.epoch += 1
        S.captured["Right"]()
        T.popup(310, 121, 72, 60, 250)
        T.tick(1)
        assert(#S.done == 0, "not yet: " .. tostring(S.done[1]))
        T.closePopups()
        T.popup(311, 300, 200, 400, 300)
        T.tick(1)
        assert(S.done[1] == "true:nil", tostring(S.done[1]))
        assert(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at") == 1
          and T.count("test 'newWindow' sees another window in its place, at 300,200 400x300") == 1, T.dump())
    "##);
}

/// Of the windows that appeared, the largest is the menu: JUCE draws a popup's shadow as windows
/// of their own on Windows, thin strips around it.
#[test]
fn the_largest_window_that_appeared_is_the_menu() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        T.popup(301, 482, 80, 8, 200)     -- the right shadow
        T.popup(300, 362, 80, 120, 200)   -- the menu
        T.popup(302, 362, 280, 120, 8)    -- the bottom shadow
        T.tick(1)
        assert(T.clickAt(2) == "372,100", T.clickAt(2))
        assert(S.ownsAsked[#S.ownsAsked][1] == 300)
    "##);
}

/// Something drawn over the item: nothing clicked, said like any covered control, logged — and on
/// a retry the same.
#[test]
fn a_covered_item_is_not_clicked() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        S.owns = function(id) return id ~= 300 end
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 1, "only the opener")
        assert(S.speech[#S.speech].text == "something else is covering Load preset", T.said())
        assert(T.count("not chosen — (372,100) is inside the menu's window id 300, but another window is drawn there") == 1, T.dump())
        -- Covered only on the retry: the first click was made, the second is not.
        T.closePopups()
        T.tick(2)
        S.owns = true
        T.press(o)
        T.popup(301, 362, 80, 120, 200)
        T.tick(1)
        S.owns = function(id) return id ~= 301 end
        local clicks = #S.clicks
        T.tick(1)
        assert(#S.clicks == clicks, "not clicked again")
        assert(S.speech[#S.speech].text == "something else is covering Load preset", T.said())
    "##);
}

/// No test sees a menu for as long as the tests are asked on every tick after the press: nothing
/// is clicked, the user hears it once, and a menu that appears after that is another press's.
#[test]
fn no_menu_seen_in_time_chooses_nothing_and_says_so() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        T.run(7950)
        assert(#S.clicks == 1 and T.said() == "Load preset, activated", "still waiting: " .. T.said())
        T.run(150)
        assert(S.speech[#S.speech].text == "Load preset: no menu was seen, nothing was chosen", T.said())
        assert(T.count("not chosen — no menu test saw a menu within 8000 ms of the press") == 1, T.dump())
        T.popup(300, 362, 80, 120, 200)
        T.run(1500)
        assert(#S.clicks == 1, "a menu after that is not chosen from")
    "##);
}

/// The user presses one of the overlay's own keys before any menu is seen: the item is not chosen
/// and nothing is said — the key speaks for itself.
#[test]
fn an_own_key_first_drops_the_item_quietly() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        T.tick(1)
        T.tab()
        assert(T.count("not chosen — one of the overlay's own keys came first, with no menu open") == 1, T.dump())
        T.popup(300, 362, 80, 120, 200)
        T.run(1500)
        assert(#S.clicks == 1, "not clicked")
        assert(T.count("no menu was seen") == 0, "and not said")
    "##);
}

/// A second press of the same control while its menu is still awaited — a double Return, a held
/// arrow — clicks nothing: the first press stands, newWindow keeps its list, and the menu that
/// press opens is chosen in once. A stepper's second press says nothing of its own.
#[test]
fn a_second_press_while_the_menu_is_awaited_clicks_nothing() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        T.press(o)
        assert(#S.clicks == 1, "the opener once: " .. #S.clicks)
        assert(T.count("[menu item] 'Avenger': 'Load preset' pressed again while its menu is still being "
          .. "waited for — the opener is not clicked again; the first press stands") == 1, T.dump())
        assert(T.count("another press came first") == 0 and T.count("own keys came first") == 0, T.dump())
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 2 and T.clickAt(2) == "372,100", "chosen in the first press's menu: " .. T.clickAt(2))
        T.closePopups()
        T.tick(2)
        -- A stepper: Right twice, one opener click, one announcement.
        S.zoom = "80%"
        local st = T.stepper({})
        local said = #S.speech
        T.right()
        T.right()
        assert(#S.clicks == 3, "one opener click for two presses: " .. #S.clicks)
        T.popup(310, 121, 72, 60, 250)
        T.tick(1)
        assert(#S.clicks == 4)
        S.zoom = "85%"
        T.closePopups()
        T.run(600)
        local says = 0
        for i = said + 1, #S.speech do
          if S.speech[i].text == "Zoom, 85%, slider" then says += 1 end
        end
        assert(says == 1, "said once: " .. T.said(said + 1))
        -- The dropped press hears "busy" at once; the first, "chosen" once the list has taken it.
        assert(S.done[1] == "false:busy" and S.done[2] == "true:nil", table.concat(S.done, " "))
    "##);
}

/// A test that sees a menu but does not say where it is — nativePopup — hands the keys to the menu
/// as always, and chooses nothing; when the menu closes, the user hears why.
#[test]
fn a_menu_that_does_not_say_where_it_is_chooses_nothing() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.nativePopup })
        T.press(o)
        S.native = true
        T.tick(1)
        assert(T.open() == true and #S.clicks == 1, "open, and only the opener clicked")
        assert(T.count("— test 'nativePopup' sees a menu but does not say where it is; waiting for one that does") == 1, T.dump())
        T.tick(3)
        assert(T.count("does not say where it is; waiting") == 1, "said once")
        S.native = false
        T.tick(2)
        assert(T.open() == false)
        assert(S.speech[#S.speech].text == "Load preset: the menu does not say where it is, nothing was chosen", T.said())
    "##);
}

/// A cheap test that sees the menu without saying where does not hold back a test that does:
/// while an item waits, every test on the list is asked.
#[test]
fn a_cheap_test_that_does_not_say_where_holds_back_none_that_does() {
    run(r##"
        local S = T.S
        S.hit = nil
        local placing = { name = "module's rect", test = function(_, answer) answer(S.hit or false) end }
        local o = T.header({ T.O.menuTests.nativePopup, placing })
        T.press(o)
        S.native = true
        S.hit = { x = 362, y = 80, w = 120, h = 200 }
        T.tick(1)
        assert(T.clickAt(2) == "372,100", "placed by the test that says where: " .. T.clickAt(2))
    "##);
}

/// An item that falls outside the menu seen is not clicked, and not given up either: a later
/// answer may be the menu itself (a tooltip is a window too, on a Mac). Here it never fits, and
/// when the menu closes the user hears that the item is not where it should be.
#[test]
fn an_item_outside_the_menu_is_not_clicked() {
    run(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow }, { 10, 20 })
        T.press(o)
        T.popup(300, 362, 80, 8, 12)   -- a tip under the pointer, first
        T.tick(1)
        assert(#S.clicks == 1, "(372,100) is outside 362,80 8x12")
        assert(T.count("lands at (372,100), outside it; not clicking") == 1, T.dump())
        T.popup(301, 362, 80, 120, 200) -- then the menu, larger
        T.tick(1)
        assert(T.clickAt(2) == "372,100", "chosen in the menu: " .. T.clickAt(2))
        -- An item that fits no menu at all:
        local p = T.header({ T.O.menuTests.newWindow }, { 500, 20 })
        T.closePopups()
        T.tick(2)
        T.press(p)
        T.popup(400, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 3, "not clicked: " .. #S.clicks)
        T.closePopups()
        T.tick(2)
        assert(S.speech[#S.speech].text == "Load preset is not where it should be", T.said())
        assert(T.count("the item, at (862,100) of 362,80 120x200, is outside the menu that was seen") == 1, T.dump())
    "##);
}

/// In a scaled overlay the opener is placed like any `at`, through the frame and the scale, and
/// the item's offset from the menu's corner is a distance: scaled, never framed — at the factor the
/// OPENER was placed with, which a factor gone by the time the menu is seen does not take away.
#[test]
fn a_scaled_overlay_scales_the_opener_and_the_items_offset() {
    run(r##"
        local S = T.S
        S.k = 1.6
        local o = T.header({ T.O.menuTests.newWindow }, { 10, 20 }, function() return S.k end)
        T.press(o)
        assert(T.clickAt(1) == "519,72", "100 + 1.6*262 = 519.2, 50 + 1.6*14 = 72.4: " .. T.clickAt(1))
        T.popup(300, 519, 90, 190, 320)
        T.tick(1)
        assert(T.clickAt(2) == "535,122", "519 + 16, 90 + 32: " .. T.clickAt(2))
        T.closePopups()
        T.tick(2)
        -- The factor gone between the press and the menu: the press's factor places the item.
        T.press(o)
        S.k = nil
        T.popup(301, 519, 90, 190, 320)
        T.tick(1)
        assert(#S.clicks == 4 and T.clickAt(4) == "535,122", "placed at the press's 1.6: " .. T.clickAt(4))
        T.closePopups()
        T.tick(2)
        -- A frame moves the opener and not the item's offset.
        S.k = 2
        local f = T.O.new("Framed")
        f:frame(function() return 3, 4 end)
        f:scale(function() return S.k end)
        f:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = { 10, 20 } })
        f:_watchMenus({ T.O.menuTests.newWindow })
        T.front(f)
        T.press(f)
        assert(T.clickAt(5) == "627,82", "100 + 3 + 2*262, 50 + 4 + 2*14: " .. T.clickAt(5))
        T.popup(302, 600, 100, 190, 320)
        T.tick(1)
        assert(T.clickAt(6) == "620,140", "600 + 2*10, 100 + 2*20, no frame: " .. T.clickAt(6))
    "##);
}

/// A `menuItem` function is handed the overlay, the menu and the factor the opener was placed with,
/// and answers screen pixels from the menu's corner — not scaled again; nil places nothing and
/// says so. `false` passes over a window that is not its menu (a tooltip) and waits for the next;
/// with nothing but such windows, the user hears no menu it could be chosen in was seen. A
/// right-button opener is clicked with the right button.
#[test]
fn a_menu_item_function_and_a_right_button_opener() {
    run(r##"
        local S = T.S
        local o = T.O.new("Avenger")
        o:addHotspotButton({ label = "Redo list", at = { 305, 14 }, button = "right", opensMenu = true })
        o:addHotspotButton({ label = "Zoom 55", at = { 21, 14 },
          menuItem = function(ov, menu, k)
            S.menuSeen, S.kSeen = menu, k
            if S.noItem then return nil end
            if menu.h < 30 then return false, "too small" end
            return { 5, 11.5 * 1 + 6 }
          end })
        o:_watchMenus({ T.O.menuTests.newWindow })
        T.front(o)
        T.press(o)
        assert(S.clicks[1].button == "right" and T.clickAt(1) == "405,64", "right-clicked: " .. T.clickAt(1))
        T.closePopups()
        o.focus = 2
        o:_syncNativeKeys()
        S.epoch += 1
        S.captured["Return"]()
        assert(T.clickAt(2) == "121,64" and S.clicks[2].button == nil, "the zoom field, left: " .. T.clickAt(2))
        T.popup(299, 125, 70, 20, 10)     -- a tooltip first
        T.tick(1)
        assert(#S.clicks == 2, "not chosen in the tooltip")
        assert(T.count("— test 'newWindow' sees a window at 125,70 20x10, which its menuItem function says "
          .. "is not its menu (too small); waiting for the next answer") == 1, T.dump())
        T.popup(300, 121, 72, 60, 250)
        T.tick(1)
        assert(S.menuSeen.x == 121 and S.menuSeen.kind == "window" and S.menuSeen.window == 300)
        assert(S.kSeen == nil, "no scale, no factor")
        assert(T.clickAt(3) == "126,89.5", "no scale: the plain sum, 72 + 17.5: " .. T.clickAt(3))
        -- An item function with no answer.
        T.closePopups()
        T.tick(2)
        S.noItem = true
        S.epoch += 1
        S.captured["Return"]()
        T.popup(301, 121, 72, 60, 250)
        T.tick(1)
        assert(#S.clicks == 4, "only the opener")
        assert(S.speech[#S.speech].text == "Zoom 55 is not available now", T.said())
        assert(T.count("the item cannot be placed now: its menuItem answered nil, not {dx, dy}") == 1, T.dump())
        -- Only windows that are not its menu, until the tests stop looking.
        T.closePopups()
        T.tick(2)
        S.noItem = false
        S.epoch += 1
        S.captured["Return"]()
        T.popup(302, 125, 70, 20, 10)
        T.run(8200)
        assert(#S.clicks == 5, "nothing chosen")
        assert(S.speech[#S.speech].text == "Zoom 55: no menu it can be chosen in was seen, nothing was chosen", T.said())
        -- In a scaled overlay the function gets the factor, and its answer is not scaled again.
        T.closePopups()
        T.tick(2)
        local sc = T.O.new("Scaled")
        sc:scale(function() return 2 end)
        sc:addHotspotButton({ label = "Item", at = { 21, 14 },
          menuItem = function(_, menu, k) S.kSeen = k; return { 16, 32.4 } end })
        sc:_watchMenus({ T.O.menuTests.newWindow })
        T.front(sc)
        T.press(sc)
        T.popup(303, 140, 90, 100, 200)
        T.tick(1)
        assert(S.kSeen == 2 and T.clickAt(7) == "156,122", "140 + 16, 90 + 32.4 rounded: " .. T.clickAt(7))
    "##);
}

/// A menu the plug-in paints inside its own window, seen by a module's own test that answers its
/// rectangle: the item is checked as any click of the overlay is — inside the plug-in, and the
/// plug-in's window drawn there — and refused, and said, when something else is.
#[test]
fn a_menu_painted_inside_the_plugin_is_checked_like_any_click() {
    run(r##"
        local S = T.S
        S.hit = nil
        local painted = { name = "painted menu", cheap = true,
          test = function(_, answer) answer(S.hit) end }
        local o = T.header({ painted })
        T.press(o)
        S.hit = { x = 362, y = 80, w = 120, h = 200 }
        T.tick(1)
        assert(T.clickAt(2) == "372,100", T.clickAt(2))
        local asked = S.ownsAsked[#S.ownsAsked]
        assert(asked[1] == 7, "asked of the plug-in's own window, as every click is")
        assert(T.count("(drawn inside the plug-in, checked like any click of the overlay)") == 1, T.dump())
        S.hit = nil
        T.tick(2)
        -- Refused: something else is drawn over the plug-in there.
        T.press(o)
        S.owns = false
        S.hit = { x = 362, y = 80, w = 120, h = 200 }
        T.tick(1)
        assert(#S.clicks == 3, "only the opener: " .. #S.clicks)
        assert(S.speech[#S.speech].text == "something else is covering Load preset", T.said())
        assert(T.count("not chosen — refused, and said") == 1, T.dump())
    "##);
}

/// On a Mac the window list numbers windows as the window server does, which is not a host.window
/// id: the menu's window is asked about by that number (ownsPoint's `listed`), and refused when
/// another window is drawn over the item.
#[test]
fn on_a_mac_the_menus_window_is_asked_about_by_its_number_in_the_window_list() {
    run_mac(r##"
        local S = T.S
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(T.clickAt(2) == "372,100", T.clickAt(2))
        local asked = S.ownsAsked[#S.ownsAsked]
        assert(asked[1] == 300 and asked[2] == 372 and asked[3] == 100 and asked.listed == true,
          "asked by its number in the window list")
        assert(T.count("(window id 300) (its window 300 of the window list is asked what is drawn there)") == 1, T.dump())
        T.closePopups()
        T.tick(2)
        T.press(o)
        S.owns = function(id) return id ~= 301 end
        T.popup(301, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 3 and S.speech[#S.speech].text == "something else is covering Load preset", T.said())
        assert(T.count("(372,100) is inside the menu's window 301 of the window list, but another window is drawn there") == 1, T.dump())
    "##);
}

/// A stepper whose step is a choice from the plug-in's list: `chooseMenuItem` opens it and chooses,
/// `onDone` hears it was chosen once the list has taken it, and the stepper announces what its
/// value became.
#[test]
fn choose_menu_item_from_a_stepper() {
    run(r##"
        local S = T.S
        S.zoom = "80%"
        local o = T.stepper({})
        assert(S.holding["Right"], "the stepper holds Right")
        T.right()
        assert(T.clickAt(1) == "121,64", T.clickAt(1))
        T.popup(300, 121, 72, 60, 250)
        T.tick(1)
        assert(T.clickAt(2) == "126,158.5", "item 7 of the list: " .. T.clickAt(2))
        assert(#S.done == 0, "not chosen until the list has taken it")
        S.zoom = "85%"
        T.closePopups()
        T.tick(2)
        assert(S.done[1] == "true:nil", tostring(S.done[1]))
        assert(S.speech[#S.speech].text == "Zoom, 85%, slider", T.said())
        -- The opener no control's `at` names is marked in the calibration shot once it has been used.
        assert(o:calibrationShot() == true)
        local marks = S.shots[#S.shots - 1].marks
        assert(#marks == 1 and marks[1][1] == 121 and marks[1][2] == 64, "the zoom field's crosshair")
        assert(T.count("[calibrate]" .. string.format("  %2d %-24s screen (%d,%d)  pixel %s  [menu opener]",
          1, "Zoom", 121, 64, "20,20,20")) == 1, T.dump())
        -- No menu tests: not opened, said, and the step reports it.
        local p = T.O.new("Bare")
        local r = nil
        p:addCustomButton({ label = "x", onActivate = function(ov)
          r = ov:chooseMenuItem({ label = "Zoom", at = { 21, 14 }, menuItem = { 1, 1 } })
        end })
        T.front(p)
        p:activate(1)
        assert(r == false and #S.clicks == 2, "not opened")
        assert(S.speech[#S.speech].text == "Zoom is not available now", T.said())
    "##);
}

/// A spec that cannot work raises at the call: no `at`, a `menuItem` that is neither {dx, dy} nor a
/// function, a `button` the click does not know, a `retries` that is no whole number from 0. An
/// overlay not in front opens nothing and tells onDone. An opener that cannot be placed, or is
/// refused, is said, tells onDone, and is no press: the tests are not set running, newWindow takes
/// no list, and no menu shot is used up.
#[test]
fn choose_menu_item_checks_its_spec_and_its_opener_first() {
    run(r##"
        local S = T.S
        local o = T.O.new("Avenger")
        o:scale(function() return S.k end)
        o:_watchMenus({ T.O.menuTests.newWindow })
        local function raises(spec, text)
          local ok, err = pcall(function() o:chooseMenuItem(spec) end)
          assert(not ok and string.find(tostring(err), text, 1, true), tostring(err))
        end
        raises({ menuItem = { 1, 2 } }, "chooseMenuItem takes { label, at, menuItem")
        raises({ at = { 1, 2 }, menuItem = 5 }, "`menuItem` must be {dx, dy} or a function")
        raises({ at = { 1, 2 }, menuItem = { 1, 2 }, button = "secondary" }, "`button` must be")
        raises({ at = { 1, 2 }, menuItem = { 1, 2 }, retries = -1 }, "`retries` must be a whole number")
        local done = {}
        local function spec(at)
          return { label = "Zoom", at = at, menuItem = { 1, 1 },
            onDone = function(_, chosen, why) done[#done + 1] = tostring(chosen) .. ":" .. why end }
        end
        assert(o:chooseMenuItem(spec({ 21, 14 })) == false and done[1] == "false:inactive")
        T.front(o)
        T.calibrate(true)
        local lists = S.windowLists
        S.k = nil
        assert(o:chooseMenuItem(spec({ 21, 14 })) == false and done[2] == "false:unplaced", tostring(done[2]))
        assert(S.speech[#S.speech].text == "Zoom is not available now", T.said())
        S.k = 1
        assert(o:chooseMenuItem(spec({ 2000, 14 })) == false and done[3] == "false:refused", tostring(done[3]))
        assert(#S.clicks == 0 and S.windowLists == lists, "no click, and newWindow took no list")
        assert(T.count("[menu item] 'Avenger': 'Zoom' clicked its opener") == 0, T.dump())
        assert(S.snaps == 0 and #S.asyncs == 0, "no menu shot taken")
        -- The same for a hotspot whose opener cannot be placed.
        local h = T.header({ T.O.menuTests.newWindow }, { 10, 20 }, function() return S.k end)
        S.k = nil
        T.press(h)
        assert(#S.clicks == 0 and S.windowLists == lists, "no press for a hotspot that cannot be placed")
        assert(S.speech[#S.speech].text == "Load preset is not available now", T.said())
    "##);
}

/// The ways a waiting item ends without a word, each logged and each told to onDone: the overlay
/// leaving the front, another press, the overlay on another window when the menu is seen — and
/// when the tests stop looking with the overlay on another window, nothing about a window the user
/// has left is said.
#[test]
fn the_quiet_endings_are_logged_and_told_to_on_done() {
    run(r##"
        local S = T.S
        S.zoom = "80%"
        local o = T.stepper({})
        o:addHotspotButton({ label = "Redo list", at = { 305, 14 }, button = "right", opensMenu = true })
        -- Left the front.
        T.right()
        o:_deactivate()
        assert(S.done[1] == "false:left", tostring(S.done[1]))
        assert(T.count("not chosen — the overlay left the front first") == 1, T.dump())
        -- Another press.
        T.front(o)
        o.focus = 1
        o:_syncNativeKeys()
        T.right()
        o.focus = 2
        o:_syncNativeKeys()
        S.epoch += 1
        S.captured["Return"]()
        assert(S.done[2] == "false:press", tostring(S.done[2]))
        assert(T.count("not chosen — another press came first") == 1, T.dump())
        -- On another window when the menu is seen.
        o.focus = 1
        o:_syncNativeKeys()
        T.right()
        local was = S.origin
        S.origin = { id = 8, class = was.class, app = was.app, client = was.client, bounds = was.bounds }
        T.popup(300, 121, 72, 60, 250)
        T.tick(1)
        assert(S.done[3] == "false:moved", tostring(S.done[3]))
        T.closePopups()
        T.tick(2)
        -- On another window when the tests stop looking: moved, not said.
        S.origin = was
        local said = #S.speech
        T.right()
        S.origin = { id = 8, class = was.class, app = was.app, client = was.client, bounds = was.bounds }
        T.run(8200)
        assert(S.done[4] == "false:moved", tostring(S.done[4]))
        for i = said + 1, #S.speech do
          assert(not string.find(S.speech[i].text, "no menu was seen", 1, true), T.said(said + 1))
        end
        assert(T.count("not chosen — the overlay is on another window now") == 2, T.dump())
    "##);
}

/// A control that chooses a menu item on an overlay with no menu tests: reported when it binds,
/// and at the press it opens nothing — a menu nobody can see would keep every key.
#[test]
fn a_menu_item_with_no_tests_is_reported_and_opens_nothing() {
    run(r##"
        local S = T.S
        local o = T.O.new("Avenger")
        o:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = { 10, 20 } })
        o:attach({ class = "nothing" }, {})
        assert(T.count("chooses a menu item, but the overlay lists no menu tests (`menus`)") == 1, T.dump())
        assert(T.count("opens a menu, but the overlay lists no menu tests") == 0, "one finding, not two")
        T.front(o)
        o:activate(1)
        assert(#S.clicks == 0, "not opened")
        assert(S.speech[#S.speech].text == "Load preset is not available now", T.said())
    "##);
}

/// In a calibrating run the item is photographed with its menu, a crosshair on the point about to
/// be clicked: the picture is asked off the event loop on the tick a test sees the menu — that pass
/// cannot wait for it — and the item is clicked in its answer, after it; the PNG is written on the
/// pass after that. A retry is not photographed again.
#[test]
fn the_menu_item_shot_is_asked_off_the_loop_and_the_item_clicked_once_it_is_in() {
    run(r##"
        local S = T.S
        T.calibrate(true)
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        local snaps, asked = S.snaps, #S.asyncs
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(S.snaps == snaps, "no picture taken on the event loop")
        assert(#S.asyncs == asked + 1, "one picture asked for the item: " .. #S.asyncs - asked)
        local r = S.asyncs[#S.asyncs]
        assert(r.region[1] == 350 and r.region[2] == 68 and r.region[3] == 494 and r.region[4] == 292,
          "the menu, 12 round")
        assert(#S.clicks == 1, "the item waits for its picture: " .. T.clickAt(2))
        T.tick(1)
        assert(#S.clicks == 2 and T.clickAt(2) == "372,100", "clicked once the picture is in: " .. T.clickAt(2))
        local i = #S.order
        while S.order[i] ~= "click" do i -= 1 end
        assert(S.order[i - 1] == "snapshotAsync", table.concat(S.order, " "))
        T.runDue()
        local shot
        for _, s in ipairs(S.shots) do
          if string.find(s.path, "menu%-item") then shot = s end
        end
        assert(shot and shot.path == "C:/modules/overlay-runtime/calibration/Avenger-Load-preset-menu-item.png", shot and shot.path)
        assert(shot.snapshot and shot.snapshot.region[1] == 350 and shot.snapshot.region[4] == 292,
          "the menu, 12 round")
        assert(#shot.marks == 1 and shot.marks[1][1] == 372 and shot.marks[1][2] == 100)
        assert(T.count("[calibrate] 'Avenger' 'Load preset': menu item shot, (350,68)-(494,292), "
          .. "the item marked at (372,100) -> C:/modules/overlay-runtime/calibration/Avenger-Load-preset-menu-item.png (true)") == 1, T.dump())
        -- A retry is not photographed again.
        asked = #S.asyncs
        T.tick(1)
        assert(#S.clicks == 3 and #S.asyncs == asked and S.snaps == snaps, "one shot per press")
    "##);
}

/// The overlay leaves the front while the item's picture is being taken: the pick is over, and the
/// answer clicks nothing — the picture is still written, the crosshair where the item would have been.
#[test]
fn an_item_whose_pick_ended_while_its_picture_was_taken_is_not_clicked() {
    run(r##"
        local S = T.S
        T.calibrate(true)
        local o = T.header({ T.O.menuTests.newWindow })
        T.press(o)
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        assert(#S.clicks == 1, "the opener only")
        o:_deactivate()
        T.tick(2)
        assert(#S.clicks == 1, "an item clicked after the overlay left: " .. T.clickAt(2))
        assert(T.count("[menu item] 'Avenger': 'Load preset' not chosen — the overlay left the front first") == 1, T.dump())
        assert(T.count("[calibrate] 'Avenger' 'Load preset': menu item shot, (350,68)-(494,292), "
          .. "the item marked at (372,100)") == 1, T.dump())
    "##);
}

/// The popup of a plug-in whose menu takes the front — a window of the same process: the overlay
/// holds its place over it, and the item is chosen there, asked of that window.
#[test]
fn the_item_is_chosen_while_the_overlay_holds_its_place_over_the_menu() {
    run(r##"
        local S = T.S
        S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
        T.turn()
        local o = T.O.new("Avenger")
        o:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = { 10, 20 } })
        o:attachEmbedded({ hosts = { T.HOST }, control = "Qt%d+.-QWindowIcon" },
          { slot = "com.platform.vps-avenger", menus = { T.O.menuTests.newWindow } })
        T.runDue() -- its first evaluation, on the module's next turn
        assert(o.active, T.dump())
        T.press(o)
        assert(T.clickAt(1) == "362,64", T.clickAt(1))
        T.popup(900, 400, 180, 180, 220)
        T.show(T.POPUP)
        assert(o.active, "held over its menu")
        assert(T.clickAt(2) == "410,200", "chosen in the menu that took the front: " .. T.clickAt(2))
        assert(S.ownsAsked[#S.ownsAsked][1] == 900)
        assert(T.count("came to the front after 'Load preset' was pressed and a test sees a menu") == 1, T.dump())
    "##);
}

/// A hold over a menu that is a window in front ends with the plug-in having the keyboard again
/// before any answer placed the item: the menu is over, and the user hears why nothing was chosen.
#[test]
fn a_hold_that_ends_before_the_item_is_placed_says_why() {
    run(r##"
        local S = T.S
        S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
        T.turn()
        local o = T.O.new("Avenger")
        o:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = { 500, 20 } })
        o:attachEmbedded({ hosts = { T.HOST }, control = "Qt%d+.-QWindowIcon" },
          { slot = "com.platform.vps-avenger", menus = { T.O.menuTests.newWindow } })
        T.runDue() -- its first evaluation, on the module's next turn
        T.press(o)
        T.popup(900, 400, 180, 180, 220)
        T.show(T.POPUP)
        assert(o.active and #S.clicks == 1, "held, and the item outside the menu not clicked")
        T.closePopups()
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active, T.dump())
        assert(T.count("the plug-in has the keyboard again") == 1, T.dump())
        assert(S.speech[#S.speech].text == "Load preset is not where it should be", T.said())
        assert(T.count("not chosen — the item, at (900,200) of 400,180 180x220, is outside the menu that was seen") == 1, T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// Two small building blocks a box drawn inside a plug-in needs.
// ---------------------------------------------------------------------------------------------

/// `O:resume(false)`: every activation starts at the first control, the same window or not — a
/// box drawn inside the plug-in comes back on the same window each time. Without it, the same
/// window resumes where the user was.
#[test]
fn resume_false_starts_every_activation_at_the_first_control() {
    run(r##"
        local S = T.S
        local function box(resume)
          local o = T.O.new(resume and "Resumes" or "Box")
          o:addStaticText({ label = "Avenger asks", text = function() return "Are you sure?" end })
          o:addCustomButton({ label = "Yes", onActivate = function() end })
          o:addCustomButton({ label = "No", onActivate = function() end })
          if not resume then o:resume(false) end
          o:attach({ class = "#32770" }, {})
          return o
        end
        local b = box(false)
        local r = box(true)
        T.runDue() -- its first evaluation, on the module's next turn
        assert(b.active and r.active, T.dump())
        b.focus, r.focus = 3, 3
        T.show(T.OTHER)
        assert(not b.active and not r.active)
        T.show(T.FX)
        assert(b.active and r.active)
        assert(b.focus == 1, "starts at its first stop: " .. b.focus)
        assert(r.focus == 3, "the default resumes: " .. r.focus)
        local ok = pcall(function() b:resume(true) end)
        assert(not ok, "set before binding")
    "##);
}

/// `pollWhen`: a binding's `pollMatch` poll rechecks the overlay only while it answers true — the
/// rest of the time it costs one call, not a context match. One that raises is said once and
/// rechecked as if it were not there; one that is not a function raises at the bind.
#[test]
fn poll_when_limits_the_match_poll_to_when_the_module_expects_a_change() {
    run(r##"
        local S = T.S
        S.want, S.gated = false, 0
        local o = T.O.new("Box")
        o:addCustomButton({ label = "Yes", onActivate = function() end })
        o:gate(function() S.gated += 1; return false end)
        o:attach({ class = "#32770" }, { slot = "box", pollMatch = 500,
          pollWhen = function() if S.raise then error("broken") end return S.want end })
        local g = S.gated
        T.poll(500)
        T.poll(500)
        assert(S.gated == g, "not rechecked while the module expects nothing: " .. (S.gated - g))
        S.want = true
        T.poll(500)
        assert(S.gated == g + 1, "rechecked while it does")
        S.raise = true
        T.poll(500)
        T.poll(500)
        assert(S.gated == g + 3, "a pollWhen that raises is rechecked as before")
        assert(T.count("[poll] 'Box': pollWhen failed:") == 1, T.dump())
        local ok, err = pcall(function()
          T.O.new("Bad"):attach({ class = "#32770" }, { pollMatch = 500, pollWhen = true })
        end)
        assert(not ok and string.find(tostring(err), "`pollWhen` must be a function", 1, true), tostring(err))
    "##);
}
