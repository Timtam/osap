//! A menu item that opens a SUBMENU, chosen in it (`O:chooseMenuItem`'s `submenu = { menuItem }`,
//! the runtime's RG3 for VPS Avenger's Copy Sample to, Swap Sample with, Copy Slot to and Swap Slot
//! with, whose twelve slots are a submenu of a drum slot's menu), run for real against the scripted
//! host of `overlay_menu_tests.rs` with the drawn test's picture of `overlay_drawn_menu_tests.rs`.
//!
//! What it holds: the item that opens the submenu is placed as any item and clicked ONCE, after the
//! test that placed it was told through its `chained` hook — newWindow taking the window list again,
//! drawn picturing the submenu's rectangle and the menu with callbacks before the click — and never
//! clicked again while its menu is still answered; the item counts as taken only when that test
//! answers a NEW popup, which is not the menu nor a rectangle inside it, and the submenu's item is
//! placed there by `submenu.menuItem` at the opener's factor and chosen as any item is, its own
//! retries and all; a submenu that never comes within the bound from the item's click, or a menu
//! that closes first, is "no submenu", said, and one seen that its item cannot be chosen in "unfit
//! submenu"; a submenu that closes and leaves its menu open is the click untaken, but not on the
//! click's own tick; a highlighted row of the menu keeps it (drawn judges it by its rows only); a hook
//! that fails, raises or waits clicks nothing, one still getting ready at the bound is "unfollowed",
//! and a second `done` is ignored; an item ready after a wait is checked again before its click; a
//! user who moves on while the hook prepares clicks nothing; only the test that placed the item is
//! followed; a test with no hook follows by its next rectangle; a calibrating run photographs both
//! items; and a spec that cannot work raises.

use super::overlay_drawn_menu_tests::DR;
use super::overlay_menu_tests::run_with;

/// The helpers, added to the drawn test's.
const SM: &str = r##"
local S = T.S
-- A profile asked with a callback goes into S.order too, as "profile(cb)", beside the waited ones —
-- "profile(cb) child" and "profile(cb) parent" for a submenu's two (drawn's `chained`, and while it
-- follows one).
do
  local profile = rawget(T.host.screen, "profile")
  rawset(T.host.screen, "profile", function(opts, cb)
    if cb ~= nil then
      local tag = type(opts.key) == "string" and string.match(opts.key, " (%a+)$") or nil
      S.order[#S.order + 1] = "profile(cb)" .. ((tag == "child" or tag == "parent") and (" " .. tag) or "")
    end
    return profile(opts, cb)
  end)
end
-- A drawn test whose region is S.region for a press, and for a submenu's press — `press.parent`
-- the menu — the rectangle right of the menu, 60 above and below it (S.subRegion(q), when set),
-- that press noted in S.subPress.
function T.drawnSub(min)
  return T.O.menuTests.drawn({ region = function(_, press)
    if press.parent then
      S.subPress = press
      local q = press.parent
      if S.subRegion then return S.subRegion(q), min end
      return { q.x + q.w, q.y - 60, q.x + q.w + 160, q.y + q.h + 60 }, min
    end
    S.pressed = press
    return S.region, min
  end })
end
-- A choice of the overlay's own whose item opens a submenu (`submenu.menuItem` { 15, 25 } unless
-- `spec` says), its onDone noted in S.done as T.choose notes it.
function T.chooseSub(o, spec)
  local s = { submenu = { menuItem = { 15, 25 } } }
  for k, v in pairs(spec or {}) do s[k] = v end
  return T.choose(o, s)
end
-- An overlay with `tests`, scaled by `k` when given.
function T.overlayScaled(tests, k)
  local o = T.O.new("Avenger")
  if k then o:scale(function() return k end) end
  o:addCustomButton({ label = "Undo", onActivate = function() end })
  o:_watchMenus(tests)
  T.front(o)
  return o
end
-- The index of the first entry of S.order that is `what`, from `from` on; or nil.
function T.orderAt(what, from)
  for i = (from or 0) + 1, #S.order do
    if S.order[i] == what then return i end
  end
  return nil
end
-- The entries of S.order from `from` on, as one string.
function T.orderSince(from)
  local out = {}
  for i = from + 1, #S.order do out[#out + 1] = S.order[i] end
  return table.concat(out, ",")
end
-- Ticks, one at a time, until a log line holds `text` (at most `max` ticks, default 10): whether one
-- does.
function T.untilLogged(text, max)
  for _ = 1, max or 10 do
    if T.count(text) > 0 then return true end
    T.tick(1)
  end
  return T.count(text) > 0
end
-- The menu painted at 362,80-482,200 and seen by drawn, its item placed and the test told: the
-- tick on which `chained` asked its pictures, which come on the next.
function T.paintedMenuSeen()
  T.drawPopup(362, 80, 482, 200)
  T.ok(T.untilLogged("opens a submenu — the test is told first"), "the menu seen: " .. T.dump())
end
"##;

fn run(scenario: &str) {
    run_with("windows", &format!("{DR}\n{SM}"), scenario)
}

fn run_mac(scenario: &str) {
    run_with("macos", &format!("{DR}\n{SM}"), scenario)
}

// ---------------------------------------------------------------------------------------------
// A submenu that is a window of its own (newWindow).
// ---------------------------------------------------------------------------------------------

/// Windows: the menu appears, the item is placed at the opener's factor, newWindow's `chained` takes
/// the window list again, and the item is clicked once; the menu still there on later ticks is waited
/// through. A second window appears: the submenu, handed to `submenu.menuItem` with the opener's
/// factor, its item clicked and asked of its own window; both gone, the choice is made.
#[test]
fn a_submenu_window_is_chosen_in_once_its_parents_item_has_opened_it() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.O.menuTests.newWindow }, 2)
        T.chooseSub(o, { submenu = { menuItem = function(_, menu, k)
          S.subMenu, S.subK = menu, k
          return { 30, 50 }
        end } })
        -- The opener (262, 14) at 2 from (100, 50): (624, 78).
        T.ok(#S.clicks == 1 and T.clickAt(1) == "624,78", "the opener: " .. T.clickAt(1))
        T.popup(300, 624, 90, 240, 400)
        local lists = S.windowLists
        T.tick(1)
        -- The item (10, 20) at the opener's factor from the menu's corner: (644, 130).
        T.ok(#S.clicks == 2 and T.clickAt(2) == "644,130", "the item, once: " .. T.clickAt(2))
        T.ok(S.windowLists == lists + 2, "the tick's window list and the chained one's: " .. (S.windowLists - lists))
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' — test 'newWindow' sees the menu at 624,90 240x400 (window id 300); "
          .. "item (10,20) at (644,130) opens a submenu — the test is told first") == 1, T.dump())
        T.ok(T.count("[menu] 'Avenger': newWindow: follows the submenu of 'Zoom' — a window that appears from now on, "
          .. "its menu the window 300") == 1, T.dump())
        T.ok(T.count("'Zoom' — clicking item (10,20) at (644,130) once (its window id 300 is asked what is drawn there); "
          .. "its submenu is chosen in once test 'newWindow' sees a new popup, for up to 8000 ms") == 1, T.dump())
        T.tick(3)
        T.ok(#S.clicks == 2, "the menu still there is waited through, never clicked again: " .. #S.clicks)
        T.popup(301, 864, 120, 200, 300)
        T.tick(1)
        T.ok(S.subMenu and S.subMenu.id == 301 and S.subMenu.x == 864 and S.subK == 2, "the submenu, at the opener's factor")
        T.ok(#S.clicks == 3 and T.clickAt(3) == "894,170", "the submenu's item, in screen pixels: " .. T.clickAt(3))
        T.ok(S.ownsAsked[#S.ownsAsked][1] == 301, "asked of the submenu's own window")
        T.ok(T.count("'Zoom' — test 'newWindow' sees a new popup at 864,120 200x300 (window id 301) after the item's "
          .. "click: the item is taken, and its submenu is chosen in") == 1, T.dump())
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[menu] 'Avenger': newWindow: the menu and its submenu have gone") == 1, T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at (894,170); the menu has closed") == 1, T.dump())
        T.ok(S.done[1] == "true:nil" and #S.clicks == 3, tostring(S.done[1]))
        T.ok(T.said() == "", "nothing said: " .. T.said())
    "##);
}

/// The menu stays and nothing new comes: no submenu within 8000 ms of the item's click — the bound
/// starts again there — said, logged and told to onDone; the item clicked once in all that time.
#[test]
fn no_submenu_within_the_bound_is_said_and_the_parent_is_clicked_once() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.O.menuTests.newWindow })
        T.run(3000)
        T.chooseSub(o)
        T.run(3000)
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        T.ok(#S.clicks == 2 and T.clickAt(2) == "372,100", "the opener and the item: " .. T.clickAt(2))
        -- Past the press's own bound, within the item's.
        T.run(7950)
        T.ok(#S.done == 0 and #S.clicks == 2, "still waiting, clicked once: " .. #S.clicks)
        T.run(300)
        T.ok(S.done[1] == "false:no submenu", tostring(S.done[1]))
        T.ok(S.speech[#S.speech].text == "Zoom: its submenu did not open, nothing was chosen in it", T.said())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' not chosen — no submenu was seen within 8000 ms of the item's click") == 1,
          T.dump())
        T.ok(#S.clicks == 2, "the item never clicked again: " .. #S.clicks)
    "##);
}

/// The menu closes after the item's click with no submenu seen: "no submenu", in the words of a menu
/// that closed.
#[test]
fn the_parent_closing_without_a_submenu_is_no_submenu_not_chosen() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.O.menuTests.newWindow })
        T.chooseSub(o)
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        T.ok(#S.clicks == 2, "the item clicked")
        T.closePopups()
        T.tick(3)
        T.ok(T.count("[menu] 'Avenger': newWindow: the menu has gone, and no submenu was seen") == 1, T.dump())
        T.ok(T.count("'Zoom' not chosen — the menu closed after the item's click, and no submenu was seen") == 1, T.dump())
        T.ok(S.done[1] == "false:no submenu" and S.speech[#S.speech].text
          == "Zoom: its submenu did not open, nothing was chosen in it", tostring(S.done[1]))
    "##);
}

// ---------------------------------------------------------------------------------------------
// A submenu painted inside the plug-in (drawn).
// ---------------------------------------------------------------------------------------------

/// Mac: the menu painted and seen; `chained` asks the module's region with the press's `parent` and
/// pictures it and the menu with callbacks — the item is clicked only once both are in. The submenu
/// painted right of the menu is seen alike twice and its item clicked at its offset; while it is
/// followed, two pictures a tick; both repainted away, the choice is made.
#[test]
fn a_painted_submenu_is_followed_by_drawn() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayScaled({ T.drawnSub() })
        T.chooseSub(o)
        T.paintedMenuSeen()
        T.ok(T.count("[menu] 'Avenger': drawn: a popup 120x120 at 362,80, alike at readings") == 1, T.dump())
        T.ok(#S.clicks == 1, "the item waits for the hook's pictures: " .. #S.clicks)
        local p = S.subPress
        T.ok(p and p.label == "Zoom" and p.x == 372 and p.y == 100 and p.button == "left" and p.parent.x == 362
          and p.parent.y == 80 and p.parent.w == 120 and p.parent.h == 120, "the press with its menu")
        T.tick(1)
        T.ok(#S.clicks == 2 and T.clickAt(2) == "372,100", "the item, once both are in: " .. T.clickAt(2))
        local opener = T.orderAt("click")
        local child, parent = T.orderAt("profile(cb) child", opener), T.orderAt("profile(cb) parent", opener)
        local item = T.orderAt("click", opener)
        T.ok(child and parent and item and child < item and parent < item, "both pictures before the item's click: "
          .. T.orderSince(opener))
        T.ok(T.count("[menu] 'Avenger': drawn: follows the submenu of 'Zoom': looked for in 482,20-642,260, its menu at "
          .. "362,80 120x120") == 1, T.dump())
        T.drawPopup(482, 110, 582, 230)
        local asked = S.profilesAsked
        T.tick(1)
        T.ok(S.profilesAsked == asked + 2, "two pictures a tick while it follows: " .. (S.profilesAsked - asked))
        T.ok(T.untilLogged("— choosing the submenu's item (15,25) at (497,135)"), T.dump())
        T.ok(T.count("[menu] 'Avenger': drawn: a submenu 100x120 at 482,110, alike at readings") == 1, T.dump())
        T.ok(#S.clicks == 3 and T.clickAt(3) == "497,135", "the submenu's item: " .. T.clickAt(3))
        T.clearPopup()
        T.tick(4)
        T.ok(T.count("[menu] 'Avenger': drawn: the submenu and its menu have gone") == 1, T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at (497,135); the menu has closed") == 1, T.dump())
        T.ok(S.done[1] == "true:nil" and #S.clicks == 3, tostring(S.done[1]))
        asked = S.profilesAsked
        T.tick(10)
        T.ok(S.profilesAsked == asked, "nothing asked once both have gone")
    "##);
}

/// After the item's click its row is drawn highlighted — one band of the menu's rows, which moves
/// the mean of every column: the menu is still there, judged by its rows; no line says it has gone,
/// and the submenu that comes later is chosen in.
#[test]
fn a_highlighted_parent_item_keeps_the_parent() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.drawnSub() })
        T.chooseSub(o)
        T.paintedMenuSeen()
        T.tick(1)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        T.drawPopup(362, 95, 482, 106, { 90, 90, 200 })
        T.tick(4)
        T.ok(T.open() == true and T.count("has gone") == 0, "the menu still there: " .. T.dump())
        T.drawPopup(482, 110, 582, 230)
        T.ok(T.untilLogged("— choosing the submenu's item"), T.dump())
        T.ok(#S.clicks == 3 and T.clickAt(3) == "497,135", "chosen in the submenu: " .. T.clickAt(3))
        T.ok(T.count("has gone") == 0, T.dump())
    "##);
}

/// No submenu painted within the bound: "no submenu" at the bound from the item's click; drawn looks
/// for the submenu no longer, logged, and follows the menu alone — one picture a tick — so that it
/// still counts open, which a module closes with a key; once the menu goes, it closes.
#[test]
fn no_painted_submenu_within_the_bound_is_no_submenu_and_its_menu_followed_alone() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayScaled({ T.drawnSub() })
        T.chooseSub(o)
        T.paintedMenuSeen()
        T.tick(1)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        T.run(7800)
        T.ok(#S.done == 0, "still waiting")
        T.run(450)
        T.ok(S.done[1] == "false:no submenu" and S.speech[#S.speech].text == "Zoom: its submenu did not open, nothing was "
          .. "chosen in it", tostring(S.done[1]))
        T.ok(T.count("[menu] 'Avenger': drawn: no submenu of 'Zoom' within 8000 ms — its menu is followed alone") == 1, T.dump())
        T.tick(1)
        local asked = S.profilesAsked
        T.tick(1)
        T.ok(S.profilesAsked == asked + 1 and T.open() == true, "the menu alone, one picture a tick, still open: "
          .. (S.profilesAsked - asked))
        T.clearPopup()
        T.tick(4)
        T.ok(T.open() == false and T.count("drawn: the menu of 'Zoom' has gone before a submenu was seen") == 1, T.dump())
    "##);
}

/// A region for the submenu that holds the menu itself: a change inside the menu — its item's row
/// drawn as a band a popup tall — is seen in that region, and answered, but it lies inside the menu:
/// no submenu, nothing clicked, and "no submenu" at the bound.
#[test]
fn the_parent_inside_the_submenus_region_is_not_taken_for_a_submenu() {
    run(r##"
        local S = T.S
        S.subRegion = function(q) return { q.x - 10, q.y - 10, q.x + q.w + 160, q.y + q.h + 10 } end
        local o = T.overlayScaled({ T.drawnSub() })
        T.chooseSub(o)
        T.paintedMenuSeen()
        T.tick(1)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        T.drawPopup(362, 95, 482, 120, { 90, 90, 200 })
        T.ok(T.untilLogged("[menu] 'Avenger': drawn: a submenu 119x25 at 362,95"), "seen in the region: " .. T.dump())
        T.tick(2)
        T.ok(#S.clicks == 2 and T.count("sees a new popup at") == 0, "inside the menu: no submenu")
        T.run(8200)
        T.ok(S.done[1] == "false:no submenu" and #S.clicks == 2, tostring(S.done[1]))
    "##);
}

/// The submenu's item clicked, and the submenu closes alone, its menu left open: the click chose
/// nothing — "untaken", said, never "chosen".
#[test]
fn a_submenu_that_closes_alone_leaves_its_item_untaken() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.drawnSub() })
        T.chooseSub(o)
        T.paintedMenuSeen()
        T.tick(1)
        T.drawPopup(482, 110, 582, 230)
        T.ok(T.untilLogged("— choosing the submenu's item"), T.dump())
        T.ok(#S.clicks == 3, "the submenu's item: " .. #S.clicks)
        T.clearLast()
        T.tick(4)
        T.ok(S.done[1] == "false:untaken", tostring(S.done[1]))
        T.ok(S.speech[#S.speech].text == "Zoom: the menu did not take the choice", T.said())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' not chosen — the submenu has closed and its parent menu is still there "
          .. "after 1 click(s) on the item at (497,135)") == 1, T.dump())
        T.ok(T.count("chosen — clicked") == 0, T.dump())
    "##);
}

/// A toolkit that takes the submenu away the moment its item is clicked and its menu a moment later:
/// the reading asked on the click's own tick, which shows the menu alone, says nothing of the click;
/// the readings after it, with both gone, are the choice made — never "untaken".
#[test]
fn the_menu_alone_on_the_submenu_items_own_tick_is_not_untaken() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayScaled({ T.drawnSub() })
        T.chooseSub(o)
        T.paintedMenuSeen()
        T.tick(1)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        -- The submenu goes at its item's click, before that tick's reading is taken.
        local click = T.host.input.click
        rawset(T.host.input, "click", function(...)
          click(...)
          if #S.clicks == 3 then T.clearLast() end
        end)
        T.drawPopup(482, 110, 582, 230)
        T.ok(T.untilLogged("— choosing the submenu's item (15,25) at (497,135)"), T.dump())
        T.clearPopup()
        T.tick(4)
        T.ok(S.done[1] == "true:nil" and T.count("'Zoom' chosen — clicked 1 time(s) at (497,135); the menu has closed") == 1,
          tostring(S.done[1]) .. " " .. T.dump())
        T.ok(T.count("not chosen") == 0 and #S.clicks == 3, T.dump())
    "##);
}

/// The submenu still there on later ticks: its item is a click it did not take, clicked again up to
/// its retries — the submenu's, the menu's item having been clicked once — and then "untaken".
#[test]
fn a_submenu_item_not_taken_is_clicked_again_and_then_untaken() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.O.menuTests.newWindow })
        T.chooseSub(o, { retries = 2 })
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        T.popup(301, 482, 110, 100, 120)
        T.tick(1)
        T.ok(#S.clicks == 3 and T.clickAt(3) == "497,135", "the submenu's item: " .. T.clickAt(3))
        T.run(1500)
        T.ok(#S.clicks == 1 + 1 + 3, "the opener, the menu's item once, the submenu's three times: " .. #S.clicks)
        T.ok(T.clickAt(2) == "372,100" and T.clickAt(5) == "497,135", T.clickAt(5))
        T.ok(S.done[1] == "false:untaken" and T.count("not chosen — the menu was still there after 3 click(s) on the item "
          .. "at (497,135)") == 1, T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// The hook, and the user moving on.
// ---------------------------------------------------------------------------------------------

/// A `chained` hook that says it cannot follow, one that raises and one that waits: the item is not
/// clicked, "unplaced" with the hook's words, said as a control that cannot act.
#[test]
fn a_chained_hook_that_fails_clicks_no_parent() {
    for how in ["false", "raises", "yields"] {
        let s = r##"
            local S = T.S
            S.mayNotWait = true
            local how = "@HOW@"
            local own = { name = "own", cheap = true, test = function(_, answer) answer(S.hit or false) end,
              chained = function(_, _, _, done)
                if how == "false" then return done(false, "it cannot see a submenu") end
                if how == "raises" then error("boom", 0) end
                coroutine.yield()
                done(true)
              end }
            local o = T.overlayScaled({ own })
            T.chooseSub(o)
            S.hit = { x = 362, y = 80, w = 120, h = 200 }
            T.tick(1)
            T.ok(#S.clicks == 1, how .. ": only the opener: " .. #S.clicks)
            T.ok(S.done[1] == "false:unplaced" and S.speech[#S.speech].text == "Zoom is not available now",
              how .. ": " .. tostring(S.done[1]))
            local words = ({ ["false"] = "it cannot see a submenu", raises = "boom",
              yields = "a hook asked on every scan yielded; it must return at once" })[how]
            T.ok(T.count("'Zoom' not chosen — the item cannot be placed now: test 'own' cannot follow its submenu: " .. words) == 1,
              how .. ": " .. T.dump())
            T.tick(3)
            T.ok(#S.clicks == 1, how .. ": nothing clicked later")
        "##;
        run(&s.replace("@HOW@", how));
    }
}

/// drawn's pictures for the submenu come back after a wait, and by then the overlay is on another
/// window, or another window covers the item: the checks of a click made after a wait refuse it —
/// "moved", logged, or "refused", said — and the item is never clicked.
#[test]
fn a_parent_item_ready_after_a_wait_is_checked_again_before_its_click() {
    for how in ["moved", "covered"] {
        let s = r##"
            local S = T.S
            local how = "@HOW@"
            local o = T.overlayScaled({ T.drawnSub() })
            T.chooseSub(o)
            T.paintedMenuSeen()
            T.ok(#S.clicks == 1, how .. ": the hook's pictures are out, nothing clicked yet: " .. #S.clicks)
            if how == "moved" then
              S.origin = table.clone(S.origin)
              S.origin.id += 1
            else
              S.owns = false
            end
            T.tick(2)
            T.ok(#S.clicks == 1, how .. ": the item not clicked: " .. T.clickAt())
            if how == "moved" then
              T.ok(S.done[1] == "false:moved" and T.count("'Zoom' not chosen — the overlay is on another window now") == 1,
                how .. ": " .. tostring(S.done[1]) .. " " .. T.dump())
            else
              T.ok(S.done[1] == "false:refused" and T.count("'Zoom' not chosen — refused, and said") == 1,
                how .. ": " .. tostring(S.done[1]) .. " " .. T.dump())
            end
            T.ok(T.count("— clicking item") == 0, how .. ": " .. T.dump())
        "##;
        run_mac(&s.replace("@HOW@", how));
    }
}

/// A `chained` hook still getting ready when the press's bound comes: "unfollowed", said in its own
/// words — the menu was seen and its item placed, so never "no menu was seen"; its `done` coming
/// after that clicks nothing. The menu closing while it gets ready: "closed", in the words of a menu
/// that closed before anything was chosen.
#[test]
fn a_chain_still_getting_ready_at_the_bound_or_the_close_is_said_so() {
    for how in ["bound", "closed"] {
        let s = r##"
            local S = T.S
            local how = "@HOW@"
            local own = { name = "own", cheap = true, test = function(_, answer) answer(S.hit or false) end,
              chained = function(_, _, _, done) S.ready = done end }
            local o = T.overlayScaled({ own })
            T.chooseSub(o)
            S.hit = { x = 362, y = 80, w = 120, h = 200 }
            T.tick(1)
            T.ok(#S.clicks == 1 and S.ready ~= nil, how .. ": told, nothing clicked: " .. #S.clicks)
            if how == "bound" then
              T.run(8200)
              T.ok(S.done[1] == "false:unfollowed" and S.speech[#S.speech].text
                == "Zoom: its submenu cannot be followed now, nothing was chosen", how .. ": " .. tostring(S.done[1]) .. " " .. T.said())
              T.ok(T.count("'Zoom' not chosen — the bound came while its test was getting ready to follow its submenu; "
                .. "nothing was clicked in it") == 1, T.dump())
            else
              S.hit = nil
              T.tick(3)
              T.ok(S.done[1] == "false:closed" and S.speech[#S.speech].text == "Zoom: the menu closed before anything was chosen",
                how .. ": " .. tostring(S.done[1]) .. " " .. T.said())
              T.ok(T.count("'Zoom' not chosen — the menu closed while its test was getting ready to follow its submenu; "
                .. "nothing was clicked in it") == 1, T.dump())
            end
            S.ready(true)
            T.tick(2)
            T.ok(#S.clicks == 1 and T.count("— test 'own' is ready to follow the submenu, and the choice is over; nothing "
              .. "clicked") == 1, how .. ": " .. T.dump())
        "##;
        run(&s.replace("@HOW@", how));
    }
}

/// A `done` answered twice from a callback: the second is logged and ignored, the item clicked once.
#[test]
fn a_chained_done_answered_twice_clicks_the_parent_once() {
    run(r##"
        local S = T.S
        local own = { name = "own", cheap = true, test = function(_, answer) answer(S.hit or false) end,
          chained = function(_, _, _, done) S.ready = done end }
        local o = T.overlayScaled({ own })
        T.chooseSub(o)
        S.hit = { x = 362, y = 80, w = 120, h = 200 }
        T.tick(1)
        S.ready(true)
        S.ready(true)
        T.tick(2)
        T.ok(#S.clicks == 2 and T.clickAt(2) == "372,100", "the item, once: " .. #S.clicks)
        T.ok(T.count("'Zoom' — test 'own' answered `chained` twice; the second answer is ignored") == 1, T.dump())
    "##);
}

/// A new popup after the item's click that the submenu's item cannot be chosen in — its menuItem
/// answers `false` for it, or the item falls outside it — is "unfit submenu" at the bound or the
/// close, said as what was seen: never "its submenu did not open".
#[test]
fn a_submenu_seen_that_its_item_cannot_be_chosen_in_is_unfit() {
    for how in ["false", "outside"] {
        let s = r##"
            local S = T.S
            local how = "@HOW@"
            local o = T.overlayScaled({ T.O.menuTests.newWindow })
            T.chooseSub(o, { submenu = { menuItem = how == "false" and function() return false, "not its rows" end
              or { 500, 25 } } })
            T.popup(300, 362, 80, 120, 200)
            T.tick(1)
            T.ok(#S.clicks == 2, how .. ": the item: " .. #S.clicks)
            T.popup(301, 482, 110, 100, 120)
            T.tick(2)
            T.ok(#S.clicks == 2 and T.count("sees a new popup at 482,110 100x120") == 1, how .. ": seen, nothing chosen: " .. T.dump())
            if how == "false" then
              T.run(8200)
              T.ok(S.done[1] == "false:unfit submenu" and S.speech[#S.speech].text
                == "Zoom: no submenu it can be chosen in was seen, nothing was chosen in it", tostring(S.done[1]) .. " " .. T.said())
              T.ok(T.count("'Zoom' not chosen — its submenu's menuItem function took no popup seen after the item's click "
                .. "(the last at 482,110 100x120)") == 1, T.dump())
            else
              T.closePopups()
              T.tick(3)
              T.ok(S.done[1] == "false:unfit submenu" and S.speech[#S.speech].text
                == "Zoom is not where it should be in its submenu, nothing was chosen in it", tostring(S.done[1]) .. " " .. T.said())
              T.ok(T.count("'Zoom' not chosen — the submenu's item, at (982,135) of 482,110 100x120, is outside the submenu "
                .. "that was seen") == 1, T.dump())
            end
            T.ok(not string.find(T.said(), "its submenu did not open", 1, true) and T.count("no submenu was seen") == 0
              and #S.clicks == 2, how .. ": " .. T.said())
        "##;
        run(&s.replace("@HOW@", how));
    }
}

/// The user moves on while drawn's pictures for the submenu are out: the overlay leaves the front,
/// or another press comes. Logged, not said; when the pictures come, nothing is clicked in the menu.
#[test]
fn the_user_moving_on_while_the_chain_is_prepared_clicks_nothing() {
    for how in ["left", "press"] {
        let s = r##"
            local S = T.S
            local how = "@HOW@"
            local o = T.overlayScaled({ T.drawnSub() })
            T.chooseSub(o)
            T.paintedMenuSeen()
            T.ok(#S.clicks == 1, how .. ": the hook asked, nothing clicked yet: " .. #S.clicks)
            local said = #S.speech
            if how == "left" then
              o:_deactivate()
              T.ok(T.count("'Zoom' not chosen — the overlay left the front first") == 1, T.dump())
            else
              T.choose(o, { label = "MENU", at = { 300, 14 } })
              T.ok(T.count("'Zoom' not chosen — another press came first") == 1, T.dump())
            end
            local clicks = #S.clicks
            T.tick(2)
            T.ok(#S.clicks == clicks, how .. ": nothing clicked after: " .. T.clickAt())
            T.ok(T.count("[menu item] 'Avenger': 'Zoom' — test 'drawn' is ready to follow the submenu, and the choice is over; "
              .. "nothing clicked") == 1, how .. ": " .. T.dump())
            for i = said + 1, #S.speech do
              T.ok(not string.find(S.speech[i].text, "Zoom", 1, true), how .. ": said: " .. T.said(said + 1))
            end
            T.ok(S.done[1] == "false:" .. how, tostring(S.done[1]))
        "##;
        run(&s.replace("@HOW@", how));
    }
}

/// Only the test that placed the item is followed: newWindow placed it, and drawn — which sees the
/// painted menu and, after the click, a larger change beside it — answers rectangles that count for
/// nothing; with no new window, "no submenu" at the bound, the item clicked once.
#[test]
fn a_submenu_follows_only_the_test_that_placed_the_parent() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.O.menuTests.newWindow, T.drawnSub() })
        T.chooseSub(o)
        T.popup(300, 362, 80, 120, 200)
        T.drawPopup(362, 80, 482, 200)
        T.tick(1)
        T.ok(#S.clicks == 2 and T.count("follows the submenu") == 1 and T.count("newWindow: follows the submenu") == 1,
          "newWindow placed and follows it: " .. T.dump())
        T.drawPopup(482, 110, 582, 230)
        T.tick(6)
        T.ok(T.count("[menu] 'Avenger': drawn: a popup ") >= 1, "drawn sees a popup: " .. T.dump())
        T.ok(#S.clicks == 2 and T.count("sees a new popup at") == 0, "drawn's answers are not followed: " .. #S.clicks)
        T.run(8200)
        T.ok(S.done[1] == "false:no submenu" and #S.clicks == 2, tostring(S.done[1]))
    "##);
}

/// A `submenu` that cannot work raises at the call: not a table, a table with no `menuItem`, a
/// `menuItem` that is neither {dx, dy} nor a function. With `retries = 0` the submenu's item is
/// chosen at its click, and not followed; the menu's item is clicked once either way.
#[test]
fn choose_menu_item_checks_its_submenu() {
    run(r##"
        local S = T.S
        local o = T.overlayScaled({ T.O.menuTests.newWindow })
        for _, sub in ipairs({ 3, {}, { menuItem = "x" } }) do
          local ok, err = pcall(function()
            o:chooseMenuItem({ label = "Zoom", at = { 262, 14 }, menuItem = { 10, 20 }, submenu = sub })
          end)
          T.ok(not ok and string.find(tostring(err), "overlay 'Avenger': chooseMenuItem's `submenu` must be { menuItem = "
            .. "{dx, dy} or a function(overlay, menu, factor) }, got ", 1, true), tostring(err))
        end
        T.ok(#S.clicks == 0, "nothing clicked by a call that raised")
        T.chooseSub(o, { retries = 0 })
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        T.ok(#S.clicks == 2 and #S.done == 0, "the menu's item is not chosen at its click: " .. tostring(S.done[1]))
        T.popup(301, 482, 110, 100, 120)
        T.tick(1)
        T.ok(#S.clicks == 3 and S.done[1] == "true:nil", "the submenu's item, chosen at its click: " .. tostring(S.done[1]))
        T.tick(3)
        T.ok(#S.clicks == 3, "not followed")
    "##);
}

/// In a calibrating run both items are photographed before their clicks: the menu's item as
/// `<overlay>-<control>-menu-item.png`, the submenu's in its submenu as `…-submenu-item.png`.
#[test]
fn a_calibrating_run_shoots_the_parents_and_the_submenus_item() {
    run(r##"
        local S = T.S
        T.calibrate(true)
        local o = T.overlayScaled({ T.O.menuTests.newWindow })
        T.chooseSub(o)
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        T.ok(#S.clicks == 1, "the menu's item waits for its picture")
        T.tick(1)
        T.ok(#S.clicks == 2 and T.clickAt(2) == "372,100", "the menu's item, after its picture: " .. T.clickAt(2))
        T.popup(301, 482, 110, 100, 120)
        T.tick(1)
        T.ok(#S.clicks == 2, "the submenu's item waits for its picture")
        T.tick(1)
        T.ok(#S.clicks == 3 and T.clickAt(3) == "497,135", "the submenu's item, after its picture: " .. T.clickAt(3))
        T.runDue()
        local paths = {}
        for _, s in ipairs(S.shots) do
          if string.find(s.path, "menu%-item") then paths[#paths + 1] = s.path end
        end
        T.ok(#paths == 2 and paths[1] == "C:/modules/overlay-runtime/calibration/Avenger-Zoom-menu-item.png"
          and paths[2] == "C:/modules/overlay-runtime/calibration/Avenger-Zoom-submenu-item.png", table.concat(paths, " "))
        T.ok(T.count("[calibrate] 'Avenger' 'Zoom': submenu item shot, (470,98)-(594,242), the item marked at (497,135)") == 1,
          T.dump())
    "##);
}

/// A test of the module's own with no `chained` hook: told nothing, the item clicked at once, and its
/// next answer that is not the menu — nor inside it — is the submenu, chosen in.
#[test]
fn a_test_without_chained_follows_by_its_next_rectangle() {
    run(r##"
        local S = T.S
        local own = { name = "own", cheap = true, test = function(_, answer) answer(S.hit or false) end }
        local o = T.overlayScaled({ own })
        T.chooseSub(o)
        S.hit = { x = 362, y = 80, w = 120, h = 200 }
        T.tick(1)
        T.ok(#S.clicks == 2 and T.count("opens a submenu — the test has no `chained`: its next answer that is not this menu "
          .. "is the submenu") == 1, T.dump())
        S.hit = { x = 370, y = 90, w = 100, h = 40 }
        T.tick(2)
        T.ok(#S.clicks == 2, "a rectangle inside the menu is no submenu")
        S.hit = { x = 482, y = 110, w = 100, h = 120 }
        T.tick(1)
        T.ok(#S.clicks == 3 and T.clickAt(3) == "497,135", "chosen in it: " .. T.clickAt(3))
        S.hit = nil
        T.tick(3)
        T.ok(S.done[1] == "true:nil", tostring(S.done[1]))
    "##);
}
