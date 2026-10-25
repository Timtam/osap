//! A popup PAINTED inside the plug-in, seen by the runtime's `O.menuTests.drawn` (the first Mac test
//! of the VPS Avenger page, 2026-10-14: in Logic Avenger's zoom list was neither a window of the
//! process nor a Cocoa menu, and no menu test saw it), run for real against the scripted host of
//! `overlay_menu_tests.rs`. The host's `host.screen.profile` is played here over a scripted picture
//! of flat rectangles — waited for in a handler, or with a callback answered on the next tick — so
//! what is checked is the test's own comparison: the rectangle the module names for a press, its
//! profile before the click, and every reading after it compared with that.
//!
//! What it holds: a filled area that reads the same twice in a row is the popup, and its item is
//! chosen in it; an animation that never reads alike, a flash gone before it does, and an outline
//! round something are no menu; nothing changed is logged at twenty readings; a highlighted row
//! keeps the popup, so a click it did not take is clicked again; the popup gone with the plug-in
//! redrawn under it is the item taken, clicked once; no reading is asked before a press, nor after
//! the popup has gone or the press's look ran out; a press with no rectangle, or where the picture
//! cannot wait, arms nothing and says so; the keys pass through while the popup is seen and come
//! back after it; and a choice is followed only by the test that placed its item. And RG4:
//! `O.doubleClick`'s button and its callback after the second click.

use super::overlay_menu_tests::run_with;

/// The helpers, added to the harness's `T`.
pub(super) const DR: &str = r##"
local S = T.S
S.clicks = {}
S.owns = true
S.ownsAsked = {}

rawset(T.host, "input", T.strict("host.input", {
  -- Answered as the host answers a click: true, or false and its error value (S.clickErr, for
  -- every click asked while it is set). A double-click (`clicks = 2`) as the host makes one on a
  -- thread of its own: its first click at the call, refused there with S.clickErr; its second, and
  -- the callback, 100 ms later, as long as the host's gesture takes — the second not sent and the
  -- callback told S.secondErr while that is set. Each of its clicks is an entry of S.clicks marked
  -- with its number, `double`. The callback of any click is answered once, on a later tick, as the
  -- host answers it.
  click = function(x, y, opts, cb)
    local double = opts and opts.clicks == 2
    S.clicks[#S.clicks + 1] = { x, y, button = opts and opts.button, double = double and 1 or nil }
    S.order[#S.order + 1] = "click"
    local function later(fn, ms) S.after[#S.after + 1] = { at = S.now + (ms or 0), fn = fn } end
    if S.clickErr then
      local err = S.clickErr
      if cb then later(function() cb(false, err) end) end
      return false, err
    end
    if double then
      later(function()
        local err = S.secondErr
        if err == nil then
          S.clicks[#S.clicks + 1] = { x, y, button = opts.button, double = 2 }
          S.order[#S.order + 1] = "click"
        end
        if cb then
          if err ~= nil then cb(false, err) else cb(true) end
        end
      end, 100)
    elseif cb then
      later(function() cb(true) end)
    end
    return true
  end,
}))
rawset(T.host.window, "ownsPoint", function(id, x, y, opts)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y, listed = opts and opts.listed }
  if type(S.owns) == "function" then return S.owns(id, x, y) end
  return S.owns
end)

-- ── The picture ──────────────────────────────────────────────────────────────
-- A flat background and rectangles over it, later ones on top: { x1, y1, x2, y2, c }, a pixel in
-- one when its corner is (x1 <= x < x2). `S.version` turns over at every change, which the
-- profiles' cache goes by.
S.drawn = { bg = { 40, 40, 40 }, rects = {} }
S.version = 0
S.profilesAsked, S.profilesWaited = 0, 0
S.profileOut = {}   -- { cb, value, why, key }: the answers of profiles with a callback, on the next tick
S.onProfileWait = nil -- function(): runs inside a waited profile, before it answers
S.profileFails = nil  -- the reason a profile with a callback answers instead of a picture, while set

local function changed() S.version += 1 end
function T.drawPopup(x1, y1, x2, y2, c)
  S.drawn.rects[#S.drawn.rects + 1] = { x1, y1, x2, y2, c or { 200, 200, 200 } }
  changed()
end
-- A ring `t` thick round x1,y1-x2,y2, as VoiceOver draws its cursor.
function T.drawOutline(x1, y1, x2, y2, t, c)
  c = c or { 230, 230, 60 }
  T.drawPopup(x1, y1, x2, y1 + t, c)
  T.drawPopup(x1, y2 - t, x2, y2, c)
  T.drawPopup(x1, y1, x1 + t, y2, c)
  T.drawPopup(x2 - t, y1, x2, y2, c)
end
function T.clearPopup()
  S.drawn.rects = {}
  changed()
end
-- The plug-in redrawn under the popup: another background.
function T.background(c)
  S.drawn.bg = c
  changed()
end
-- The last rectangle drawn goes, the ones under it stay.
function T.clearLast()
  table.remove(S.drawn.rects)
  changed()
end

local function colourAt(x, y)
  local d = S.drawn
  for i = #d.rects, 1, -1 do
    local r = d.rects[i]
    if x >= r[1] and x < r[3] and y >= r[2] and y < r[4] then return r[5] end
  end
  return d.bg
end

-- The profile host.screen.profile answers for `region` (corners) of the picture as it is now:
-- the host's statistics per column and per row, BT.601 luminance.
local cache = {}
local function profileOf(region)
  local key = S.version .. ":" .. table.concat(region, ",")
  if cache[key] then return cache[key] end
  local x1, y1, x2, y2 = region[1], region[2], region[3], region[4]
  local w, h = x2 - x1, y2 - y1
  local function axis(n) return { min = table.create(n, 255), max = table.create(n, 0), dark = table.create(n, 0),
    mean = table.create(n, 0), r = table.create(n, 0), g = table.create(n, 0), b = table.create(n, 0) } end
  local cols, rows = axis(w), axis(h)
  for j = 1, h do
    for i = 1, w do
      local c = colourAt(x1 + i - 1, y1 + j - 1)
      local l = 0.299 * c[1] + 0.587 * c[2] + 0.114 * c[3]
      local li = math.floor(l + 0.5)
      for _, a in ipairs({ { cols, i }, { rows, j } }) do
        local t, k = a[1], a[2]
        if li < t.min[k] then t.min[k] = li end
        if li > t.max[k] then t.max[k] = li end
        if l < 128 then t.dark[k] += 1 end
        t.mean[k] += l
        t.r[k] += c[1]
        t.g[k] += c[2]
        t.b[k] += c[3]
      end
    end
  end
  for i = 1, w do
    for _, f in ipairs({ "mean", "r", "g", "b" }) do cols[f][i] /= h end
  end
  for j = 1, h do
    for _, f in ipairs({ "mean", "r", "g", "b" }) do rows[f][j] /= w end
  end
  local p = { x = x1, y = y1, w = w, h = h, columns = cols, rows = rows, time = S.now, inputEpoch = 0 }
  cache[key] = p
  return p
end

rawset(T.host.screen, "profile", function(opts, cb)
  if cb == nil then T.screenWait("host.screen.profile") end
  assert(type(opts) == "table" and opts.axes == "both", "the drawn test profiles both axes")
  local r = opts.region
  assert(type(r) == "table" and #r == 4, "corners")
  for k = 1, 4 do assert(r[k] == math.floor(r[k]), "corner " .. k .. " is not a whole number: " .. tostring(r[k])) end
  assert(r[3] > r[1] and r[4] > r[2], "corners empty or turned around")
  S.profilesAsked += 1
  local value = profileOf(r)
  if cb == nil then
    assert(opts.key == nil, "a key only with a callback")
    S.profilesWaited += 1
    S.order[#S.order + 1] = "profile"
    if S.onProfileWait then S.onProfileWait() end
    return value
  end
  if opts.key then S.out[opts.key] = (S.out[opts.key] or 0) + 1 end
  if S.profileFails then
    S.profileOut[#S.profileOut + 1] = { cb = cb, why = S.profileFails, key = opts.key }
  else
    S.profileOut[#S.profileOut + 1] = { cb = cb, value = value, key = opts.key }
  end
end)

do
  local runDue = T.runDue
  T.runDue = function()
    runDue()
    local out = S.profileOut
    if #out == 0 then return end
    S.profileOut = {}
    S.epoch += 1
    for _, a in ipairs(out) do
      if a.key then S.out[a.key] -= 1 end
      T.call("answer", a.cb, a.value, a.why, { time = S.now })
    end
  end
end

-- ── The overlay ──────────────────────────────────────────────────────────────
-- Over S.origin (client 100,50 800x600), with `tests` its menu tests: `drawn` a drawn test whose
-- region is S.region for every press (nil: none), the press it was handed noted in S.pressed.
S.region = { 300, 80, 540, 280 }
function T.drawnTest(min)
  return T.O.menuTests.drawn({
    region = function(_, press)
      S.pressed = press
      return S.region, min
    end,
  })
end

-- A choice of the overlay's own, as a stepper's `onStep` makes it: in a key's handler, where the
-- picture before the press waits. Its onDone noted in S.done.
function T.choose(o, spec)
  S.done = S.done or {}
  local s = { label = "Zoom", at = { 262, 14 }, menuItem = { 10, 20 },
    onDone = function(_, chosen, err) S.done[#S.done + 1] = tostring(chosen) .. ":" .. tostring(err and err.code) end }
  for k, v in pairs(spec or {}) do s[k] = v end
  local out
  assert(T.call("key", function() out = o:chooseMenuItem(s) end) == "Ran")
  return out
end

function T.overlayWith(tests)
  local o = T.O.new("Avenger")
  o:addCustomButton({ label = "Undo", onActivate = function() end })
  o:_watchMenus(tests)
  T.front(o)
  return o
end

function T.popup(id, x, y, w, h)
  S.windows[#S.windows + 1] = { id = id, layer = 0, class = "JUCE_1d4a", x = x, y = y, w = w, h = h }
end
function T.closePopups() S.windows = { S.windows[1] } end

function T.clickAt(i)
  local c = S.clicks[i or #S.clicks]
  return c and (c[1] .. "," .. c[2]) or "none"
end

function T.said(from)
  local out = {}
  for i = from or 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

-- An assertion whose failure prints the whole log to the test's output: a Luau error message is
-- cut at about 500 bytes.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  error(tostring(what or "assertion failed"), 2)
end
"##;

fn run(scenario: &str) {
    run_with("windows", DR, scenario)
}

fn run_mac(scenario: &str) {
    run_with("macos", DR, scenario)
}

/// The press: the rectangle the module names, with the press it was handed — the label, the
/// opener's point and its button — profiled in the press's handler BEFORE the click, waited for.
/// Nothing on the next tick (no change); the popup painted inside the plug-in, a filled area,
/// read once (not alike yet) and again alike: SEEN, answered as the area it covers with no
/// `kind`, and the item chosen in it at its offset from that corner, checked as any click of the
/// overlay. The popup gone: the item taken, clicked once, the test comparing no more.
#[test]
fn a_popup_painted_inside_the_plugin_is_chosen_once_it_reads_the_same_twice() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayWith({ T.O.menuTests.newWindow, T.drawnTest() })
        local before = #S.order
        T.ok(T.choose(o, { button = "right" }) == true, "opened")
        T.ok(S.profilesWaited == 1 and S.order[before + 1] == "profile" and S.order[before + 2] == "click"
          and #S.order == before + 2, "the picture before the click: " .. table.concat(S.order, ","))
        T.ok(S.pressed.label == "Zoom" and S.pressed.x == 362 and S.pressed.y == 64 and S.pressed.button == "right",
          "the press handed to the region: " .. tostring(S.pressed.label) .. " " .. tostring(S.pressed.x))
        T.ok(T.count("[menu] 'Avenger': drawn: armed on 'Zoom' in 300,80-540,280 (240x200), a popup at least 12x12") == 1,
          T.dump())
        T.tick(2)
        T.ok(#S.clicks == 1, "no change, no item")
        T.drawPopup(362, 80, 482, 200)
        T.tick(1)
        T.ok(#S.clicks == 1, "the reading asked before the popup was drawn")
        T.tick(1)
        T.ok(#S.clicks == 1 and T.count("[menu] 'Avenger': drawn: a change of 120x120 at 362,80 at reading 3, not alike yet") == 1,
          T.dump())
        T.tick(1)
        T.ok(T.count("[menu] 'Avenger': drawn: a popup 120x120 at 362,80, alike at readings 3 and 4") == 1, T.dump())
        T.ok(#S.clicks == 2 and T.clickAt(2) == "372,100" and S.clicks[2].button == nil, "the item: " .. T.clickAt(2))
        T.ok(T.count("— test 'drawn' sees the menu at 362,80 120x120 (drawn inside the plug-in, checked like any click "
          .. "of the overlay); choosing item (10,20) at (372,100)") == 1, T.dump())
        T.ok(T.open() == true, "open: the keys are the popup's")
        T.clearPopup()
        T.tick(4)
        T.ok(#S.clicks == 2, "clicked once")
        T.ok(T.count("[menu] 'Avenger': drawn: the popup 120x120 at 362,80 has gone at reading ") == 1, T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at (372,100); the menu has closed") == 1, T.dump())
        T.ok(T.open() == false and S.done[1] == "true:nil", "closed, the keys back: " .. tostring(S.done[1]))
        local asked = S.profilesAsked
        T.tick(10)
        T.ok(S.profilesAsked == asked, "nothing asked once it has gone")
    "##);
}

/// What is no menu: an animation that never reads alike (a spinner growing every reading) — no
/// item, and at the end of the press's look "no menu was seen"; a flash, gone before its second
/// reading; and an outline round part of the plug-in (VoiceOver's cursor), whose lines change only
/// their extremes inside it.
#[test]
fn an_animation_a_flash_and_an_outline_are_no_menu() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        for i = 1, 60 do
          T.clearPopup()
          T.drawPopup(320, 90, 340 + (i % 7) * 20, 120 + (i % 5) * 20)
          T.tick(1)
        end
        T.ok(#S.clicks == 1, "no item clicked: " .. #S.clicks)
        T.ok(S.done[1] == "false:no_menu" and T.count("'Zoom' not chosen — no menu test saw a menu within 8000 ms") == 1,
          tostring(S.done[1]))
        -- Past the press's look, the test is asked once in eight ticks, and stops comparing then.
        T.tick(10)
        T.ok(T.count("[menu] 'Avenger': drawn: no popup of 'Zoom' within 8000 ms — not comparing until the next press") == 1,
          T.dump())
        T.ok(T.count("drawn: later changes of this press are not named") == 1, "the log names five changes: " .. T.dump())
        -- A flash.
        T.clearPopup()
        T.choose(o)
        T.tick(1)
        T.drawPopup(362, 80, 482, 200)
        T.tick(1)
        T.clearPopup()
        T.tick(3)
        T.ok(#S.clicks == 2 and T.open() == false, "a flash: the opener only")
        -- An outline.
        T.drawOutline(320, 100, 500, 260, 3)
        T.tick(4)
        T.ok(#S.clicks == 2 and T.open() == false, "an outline: nothing")
        T.ok(T.count("[menu] 'Avenger': drawn: a change of 180x160 at 320,100 at reading ") >= 1
          and T.count("not filled — no menu") >= 1, T.dump())
    "##);
}

/// Nothing changed — the popup never drawn, or a Mac's wallpaper without Screen Recording — is
/// said once, at the twentieth reading; and with no rectangle, nothing is armed or read.
#[test]
fn nothing_changed_is_logged_and_no_rectangle_arms_nothing() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.tick(10)
        T.ok(S.profilesAsked == 0, "nothing read before a press")
        T.choose(o)
        T.tick(25)
        T.ok(T.count("[menu] 'Avenger': drawn: nothing has changed in 300,80-540,280 after 20 readings of 'Zoom'") == 1,
          T.dump())
        T.tick(40)
        S.region = nil
        local asked = S.profilesAsked
        T.choose(o)
        T.tick(5)
        T.ok(S.profilesAsked == asked, "no rectangle: nothing read")
        T.ok(T.count("[menu] 'Avenger': drawn: nothing armed on 'Zoom' — no rectangle for 'Zoom'") == 1, T.dump())
        T.ok(#S.clicks == 2, "the opener is clicked all the same")
    "##);
}

/// A press made where a read cannot wait — a choice from the `onDone` of another, which runs in
/// the menu tick's pass — arms nothing, and says so; the opener is clicked as before. And a press
/// whose overlay left the front while the picture before it was taken clicks nothing.
#[test]
fn a_press_where_the_picture_cannot_wait_arms_nothing_and_one_that_moved_on_clicks_nothing() {
    run(r##"
        local S = T.S
        S.mayNotWait = true
        local o = T.overlayWith({ T.O.menuTests.newWindow, T.drawnTest() })
        T.choose(o, { label = "MENU", retries = 0, onDone = function(ov, chosen)
          if chosen then ov:chooseMenuItem({ label = "Again", at = { 300, 14 }, menuItem = { 5, 5 } }) end
        end })
        T.popup(300, 362, 80, 120, 200)
        T.tick(1)
        T.ok(T.count("[menu] 'Avenger': drawn: nothing armed on 'Again' — the picture before the press cannot be taken here:") == 1,
          T.dump())
        T.ok(T.clickAt() == "400,64", "the second opener clicked all the same: " .. T.clickAt())
        T.closePopups()
        T.tick(3)
        -- The overlay leaves the front while the picture is taken.
        local clicks = #S.clicks
        S.onProfileWait = function() o.active = false end
        S.done = {}
        local opened = nil
        T.ok(T.call("key", function()
          opened = o:chooseMenuItem({ label = "Zoom", at = { 262, 14 }, menuItem = { 10, 20 },
            onDone = function(_, chosen, err) S.done[#S.done + 1] = tostring(chosen) .. ":" .. tostring(err and err.code) end })
        end) == "Ran")
        T.ok(opened == false and S.done[1] == "false:not_in_front" and #S.clicks == clicks, "nothing clicked: " .. tostring(S.done[1]))
        T.ok(T.count("[menu] 'Avenger' 'Zoom': not acting after the picture before the press — the overlay is no longer active") == 1,
          T.dump())
    "##);
}

/// A click the popup did not take: the popup still there on a later reading — with its row under
/// the pointer highlighted, one band of its rows changed — is the same popup, the same rectangle,
/// and the item is clicked again; then gone with the plug-in redrawn under it, which is the item
/// taken, not clicked a third time and not a new popup.
#[test]
fn a_highlighted_row_keeps_the_popup_and_a_redraw_under_it_is_the_item_taken() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(3)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        T.drawPopup(362, 95, 482, 106, { 90, 90, 200 })
        T.tick(2)
        T.ok(#S.clicks == 3 and T.clickAt(3) == "372,100", "clicked again: " .. #S.clicks)
        T.ok(T.count("still sees the menu at 362,80 120x120 a tick after the item's click, which it did not take: "
          .. "clicking (372,100) again, 2 of 4") == 1, T.dump())
        T.clearPopup()
        T.background({ 70, 50, 30 })
        T.tick(4)
        T.ok(#S.clicks == 3, "not clicked again")
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 2 time(s) at (372,100); the menu has closed") == 1, T.dump())
        T.ok(T.count("drawn: a popup ") == 1, "the redraw is no new popup: " .. T.dump())
    "##);
}

/// The second Mac test of VPS Avenger (2026-10-24, its finding 7): Avenger stopped drawing while it
/// opened Logic's file panel, and its popup, frozen, read as a click it had not taken — the item was
/// clicked four times. A popup drawn exactly as at the item's click is the click still pending: it is
/// waited for and not clicked again, said once; once it goes the item is chosen, clicked once. Still
/// drawn so at the bound, the click was not taken.
#[test]
fn a_popup_drawn_as_at_the_click_is_the_click_pending_and_not_clicked_again() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(3)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        T.tick(6)
        T.ok(#S.clicks == 2, "not clicked again: " .. T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' — test 'drawn' sees the menu drawn as at the item's click: the "
          .. "click is waited for, not made again") == 1, T.dump())
        T.clearPopup()
        T.tick(4)
        T.ok(#S.clicks == 2 and T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at (372,100); the menu "
          .. "has closed") == 1, T.dump())
        -- Frozen until the bound.
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(70)
        T.ok(#S.clicks == 4, "the opener and the item once: " .. #S.clicks)
        T.ok(S.done[#S.done] == "false:refused" and T.count("not chosen — the menu was still there after 1 click(s) on the item at (372,100)") == 1, tostring(S.done[#S.done]))
    "##);
}

/// A click made again — the popup drawn again, its row under the pointer highlighted — is the one a
/// frozen popup is compared with from then on: a plug-in that took that click and stopped drawing
/// while it acts on it is not clicked a third and a fourth time. Once the popup goes the item is
/// chosen, clicked twice.
#[test]
fn a_popup_frozen_after_a_click_made_again_is_that_click_pending() {
    run_mac(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(3)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        T.drawPopup(362, 95, 482, 106, { 90, 90, 200 })
        T.tick(2)
        T.ok(#S.clicks == 3, "clicked again: " .. T.dump())
        T.tick(10)
        T.ok(#S.clicks == 3, "not clicked a third time: " .. T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' — test 'drawn' sees the menu drawn as at the item's click: the "
          .. "click is waited for, not made again") == 1, T.dump())
        T.clearPopup()
        T.tick(4)
        T.ok(#S.clicks == 3 and T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 2 time(s) at (372,100); the menu "
          .. "has closed") == 1, T.dump())
    "##);
}

/// A window of the plug-in's process that came over the item's point after its click is the item
/// taken — a dialog the item opened — though the popup under it was drawn again: not clicked again.
/// One away from the point is not; one listed as soon as the click returns is, since the windows are
/// listed before it.
#[test]
fn a_new_window_of_the_plugin_over_the_items_point_is_the_item_taken() {
    run(r##"
        local S = T.S
        local DIALOG = { id = 950, title = "Load Preset...", class = "#32770", app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 300, y = 60, w = 600, h = 400 }, client = { x = 300, y = 60, w = 600, h = 400 } }
        local FAR = { id = 951, title = "Elsewhere", class = "#32770", app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 1200, y = 600, w = 100, h = 100 }, client = { x = 1200, y = 600, w = 100, h = 100 } }
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(3)
        T.ok(#S.clicks == 2, "the item: " .. #S.clicks)
        -- Away from the point: the popup drawn again is a click not taken, as before.
        S.listed = { T.MAIN, T.FX, FAR }
        T.drawPopup(362, 95, 482, 106, { 90, 90, 200 })
        T.tick(2)
        T.ok(#S.clicks == 3, "clicked again: " .. T.dump())
        -- Over the point.
        S.listed = { T.MAIN, T.FX, FAR, DIALOG }
        T.drawPopup(362, 95, 482, 106, { 90, 200, 90 })
        T.tick(2)
        T.ok(#S.clicks == 3, "not clicked again: " .. T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 2 time(s) at (372,100); a new window of the "
          .. "plug-in's process, 'Load Preset...' (id 950), is over the item's point") == 1, T.dump())
    "##);
    // One that is listed as the item's click returns: the plug-in's windows are taken before it.
    run(r##"
        local S = T.S
        local DIALOG = { id = 950, title = "Load Preset...", class = "#32770", app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 300, y = 60, w = 600, h = 400 }, client = { x = 300, y = 60, w = 600, h = 400 } }
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        local click = T.host.input.click
        rawset(T.host, "input", { click = function(x, y, opts)
          local r, e = click(x, y, opts)
          if #S.clicks == 2 then S.listed = { T.MAIN, T.FX, DIALOG } end
          return r, e
        end })
        T.tick(3)
        T.ok(#S.clicks == 2 and S.listed ~= nil, "the item: " .. #S.clicks)
        T.drawPopup(362, 95, 482, 106, { 90, 200, 90 })
        T.tick(2)
        T.ok(#S.clicks == 2 and T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at (372,100); a new "
          .. "window of the plug-in's process, 'Load Preset...' (id 950), is over the item's point") == 1, T.dump())
    "##);
}

/// A choice follows only the test that placed its item. Both tests see the popup — newWindow its
/// window, drawn the painted area with its shadow, a pixel larger each way (cut at the rectangle's top) — and newWindow places
/// the item and clicks again a tick later; drawn's answer, a rectangle a little apart, does not
/// end the choice as "another window in its place".
#[test]
fn a_choice_follows_only_the_test_that_placed_its_item() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.O.menuTests.newWindow, T.drawnTest() })
        T.choose(o)
        T.popup(300, 362, 80, 120, 120)
        T.drawPopup(361, 79, 483, 201)
        T.tick(1)
        T.ok(#S.clicks == 2, "newWindow places the item")
        T.tick(1)
        T.ok(#S.clicks == 3, "and clicks again: " .. #S.clicks)
        T.closePopups()
        T.clearPopup()
        T.tick(1)
        T.ok(T.count("drawn: a popup 122x121 at 361,80") == 1, T.dump())
        T.tick(4)
        T.ok(T.count("sees another window in its place") == 0, T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 2 time(s) at (372,100); the menu has closed") == 1,
          T.dump())
    "##);
}

/// The popup gone while something else the press changed stays — a panel the same click selected,
/// under most of the rectangle: the popup's own lines, a popup's worth each way, no longer read as
/// at the sighting, which is the popup gone, though most of the lines of the span read as they did.
/// The item is clicked once, not again on the plug-in under the popup that has closed.
#[test]
fn a_popup_gone_beside_a_lasting_change_of_its_press_is_gone() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o, { menuItem = { 70, 20 } })
        T.drawPopup(300, 140, 540, 280, { 90, 90, 90 })
        T.drawPopup(362, 90, 402, 120)
        T.tick(3)
        T.ok(T.count("[menu] 'Avenger': drawn: a popup 240x190 at 300,90, alike at readings 1 and 2") == 1, T.dump())
        T.ok(#S.clicks == 2 and T.clickAt(2) == "370,110", "the item: " .. T.clickAt(2))
        T.clearLast()
        T.tick(4)
        T.ok(#S.clicks == 2, "not clicked again on the plug-in: " .. #S.clicks)
        T.ok(T.count("[menu] 'Avenger': drawn: the popup 240x190 at 300,90 has gone at reading 4 (40 columns and "
          .. "30 rows not as they were, a popup at least 12x12; 340 of its 410 lines as they were)") == 1, T.dump())
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 1 time(s) at (370,110); the menu has closed") == 1,
          T.dump())
        T.ok(T.open() == false, "the keys back")
    "##);
}

/// A popup that differs from what lies under it in few of its lines — bands of it, half its rows,
/// fewer than the smallest popup's height — goes all the same: most of the lines it changed no
/// longer read as they did.
#[test]
fn a_sparse_popup_goes_by_most_of_its_lines() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest({ 100, 100 }) })
        T.choose(o)
        for b = 0, 5 do T.drawPopup(362, 80 + b * 20, 482, 90 + b * 20) end
        T.tick(3)
        T.ok(T.count("[menu] 'Avenger': drawn: a popup 120x110 at 362,80, alike at readings 1 and 2") == 1, T.dump())
        T.ok(#S.clicks == 2, "the item")
        T.clearPopup()
        T.tick(4)
        T.ok(T.count("[menu] 'Avenger': drawn: the popup 120x110 at 362,80 has gone at reading 4 (120 columns and 60 "
          .. "rows not as they were, a popup at least 100x100; 0 of its 180 lines as they were)") == 1, T.dump())
        T.ok(#S.clicks == 2 and T.open() == false, "closed, clicked once")
    "##);
}

/// The smallest popup: a change smaller than it is no menu, whether the smallest is the test's own
/// default (12 x 12) or the one the region answers with its rectangle; the log says which.
#[test]
fn a_change_smaller_than_the_smallest_popup_is_no_menu() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        T.choose(o)
        T.drawPopup(400, 150, 410, 160)
        T.tick(4)
        T.ok(T.count("[menu] 'Avenger': drawn: a change of 10x10 at 400,150 at reading 1, smaller than a popup "
          .. "(12x12) — no menu") == 1, T.dump())
        T.ok(#S.clicks == 1 and T.open() == false, "no item, no menu")
        T.clearPopup()
        T.tick(60)
        -- Tall but narrow.
        T.choose(o)
        T.drawPopup(400, 100, 408, 160)
        T.tick(4)
        T.ok(T.count("[menu] 'Avenger': drawn: a change of 8x60 at 400,100 at reading 1, smaller than a popup "
          .. "(12x12) — no menu") == 1, T.dump())
        T.clearPopup()
        T.tick(60)
        local big = T.overlayWith({ T.drawnTest({ 150, 150 }) })
        T.choose(big)
        T.ok(T.count("drawn: armed on 'Zoom' in 300,80-540,280 (240x200), a popup at least 150x150") == 1, T.dump())
        T.drawPopup(362, 80, 482, 200)
        T.tick(4)
        T.ok(T.count("[menu] 'Avenger': drawn: a change of 120x120 at 362,80 at reading 1, smaller than a popup "
          .. "(150x150) — no menu") == 1, T.dump())
        T.ok(#S.clicks == 3 and T.open() == false, "no item in a change smaller than the region says")
    "##);
}

/// The guards of a press: one whose point another window covers once the picture before it has
/// come is refused and clicks nothing; a reading asked for a press that is over counts for nothing,
/// though it would read alike with that press's last; and a reading that could not be taken keeps
/// what the test said last, so the popup it sees stays open through it.
#[test]
fn a_press_covered_after_its_picture_a_late_reading_and_a_failed_one() {
    run(r##"
        local S = T.S
        local o = T.overlayWith({ T.drawnTest() })
        -- Covered meanwhile.
        S.onProfileWait = function()
          S.onProfileWait = nil
          S.owns = false
        end
        T.choose(o)
        T.ok(#S.clicks == 0 and S.done[1] == "false:covered", "refused, nothing clicked: " .. tostring(S.done[1]))
        T.ok(T.count("drawn: armed on") == 0, "nothing armed: " .. T.dump())
        S.owns = true
        -- A reading of the last press, answered after the next one.
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(2)
        T.ok(T.count("drawn: a change of 120x120 at 362,80 at reading 1, not alike yet") == 1, T.dump())
        T.choose(o, { label = "MENU", at = { 300, 14 } })
        T.tick(3)
        T.ok(#S.clicks == 2 and T.open() == false and T.count("drawn: a popup ") == 0,
          "the old press's reading took no item: " .. T.clickAt())
        -- The popup cleared only once the last press stopped comparing: its going would be a change
        -- of that press's picture.
        T.tick(60)
        T.clearPopup()
        T.tick(2)
        -- A reading that fails while the popup is seen.
        T.choose(o, { retries = 0 })
        T.drawPopup(362, 80, 482, 200)
        T.tick(3)
        T.ok(#S.clicks == 4 and T.open() == true, "chosen at the click, the popup still up")
        S.profileFails = "the capture failed"
        T.tick(4)
        T.ok(T.open() == true, "still open through the failed readings")
        T.ok(T.count("[menu] 'Avenger': drawn: a reading of 'Zoom' could not be taken: the capture failed") == 1, T.dump())
        S.profileFails = nil
        T.clearPopup()
        T.tick(4)
        T.ok(T.open() == false, "closed once it reads gone")
    "##);
}

/// Two tests of one name — two `drawn` tests without one, each its own rectangle — both see the
/// popup, the second cut short by its region: only the test that placed the item follows the choice,
/// not the other one of the same name, whose rectangle would read as another window in its place.
#[test]
fn a_choice_follows_the_test_that_placed_its_item_not_another_of_its_name() {
    run(r##"
        local S = T.S
        local function drawnIn(r) return T.O.menuTests.drawn({ region = function() return r end }) end
        local o = T.overlayWith({ drawnIn({ 300, 80, 540, 280 }), drawnIn({ 300, 80, 540, 180 }) })
        T.choose(o)
        T.drawPopup(362, 80, 482, 200)
        T.tick(3)
        T.ok(#S.clicks == 2 and T.count("drawn: a popup 120x100 at 362,80") == 1, "both see it: " .. T.dump())
        -- Drawn again — its row under the pointer highlighted — and still there: not taken.
        T.drawPopup(362, 95, 482, 106, { 90, 90, 200 })
        T.tick(2)
        T.ok(#S.clicks == 3 and T.count("sees another window in its place") == 0, "clicked again: " .. T.dump())
        T.clearPopup()
        T.tick(4)
        T.ok(T.count("[menu item] 'Avenger': 'Zoom' chosen — clicked 2 time(s) at (372,100); the menu has closed") == 1,
          T.dump())
    "##);
}

/// RG4: `O.doubleClick` with `button` — one double-click of the host's (`clicks = 2`), both clicks
/// with it — and the callback once, after the second click has gone out; without options, or with
/// the left button, a double-click of the left button; a button it does not know, options that are
/// not a table and a callback that is not a function raise, naming them, and click nothing. And
/// D41: it answers as the host's click does, `true` at the call and the gesture's answer to the
/// callback; a double-click the host refuses at the call answers its error there and the callback
/// hears the same table on a later turn; one whose second half the host did not send — the window at
/// the point changed — is `true` at the call and `covered` to the callback, handed on as it came.
#[test]
fn a_double_click_takes_a_button_and_calls_back_after_the_second_click() {
    run(r##"
        local S = T.S
        local after = nil
        T.ok(T.call("key", function()
          T.O.doubleClick(410, 120, { button = "right" }, function() after = #S.clicks end)
        end) == "Ran")
        T.ok(#S.clicks == 1 and S.clicks[1].button == "right" and S.clicks[1].double == 1 and after == nil,
          "one double-click asked of the host, its first click at the call, right")
        T.tick(1)
        T.ok(#S.clicks == 2 and S.clicks[2].button == "right" and S.clicks[2].double == 2 and S.clicks[2][1] == 410
          and after == 2, "the second, then the callback: " .. tostring(after))
        T.O.doubleClick(5, 6)
        T.tick(1)
        T.ok(#S.clicks == 4 and S.clicks[3].button == nil and S.clicks[4].button == nil and S.clicks[4].double == 2,
          "a double-click of the left button")
        T.O.doubleClick(5, 6, { button = "left" })
        T.tick(1)
        T.ok(#S.clicks == 6 and S.clicks[6].button == nil, "left is the host's default button")
        local ok, err = pcall(T.O.doubleClick, 1, 2, { button = "x" })
        T.ok(not ok and string.find(err, 'O.doubleClick: opts.button must be "left", "right" or "middle", got x', 1, true), err)
        ok, err = pcall(T.O.doubleClick, 1, 2, "right")
        T.ok(not ok and string.find(err, "O.doubleClick: opts must be a table { button = … } or nil, got string", 1, true), err)
        ok, err = pcall(T.O.doubleClick, 1, 2, nil, 5)
        T.ok(not ok and string.find(err, "the callback (fourth argument) must be a function or nil, got number", 1, true), err)
        T.ok(#S.clicks == 6, "nothing clicked by a call that raised")
        local answers = nil
        local calls = 0
        local first = table.pack(T.O.doubleClick(410, 120, nil, function(...) calls += 1; answers = table.pack(...) end))
        T.ok(first.n == 1 and first[1] == true, "true at the call")
        T.tick(1)
        T.ok(answers and answers[1] == true and answers[2] == nil, "the callback hears the gesture's answer")
        T.tick(2)
        T.ok(calls == 1, "called once: " .. calls)
        local covered = T.E("covered", "something else is drawn over the point")
        S.clickErr = covered
        answers = nil
        local ok2, err2 = T.O.doubleClick(410, 120, nil, function(...) answers = table.pack(...) end)
        T.ok(ok2 == false and rawequal(err2, covered) and #S.clicks == 9 and answers == nil,
          "the host's refusal at the call, and nothing called back yet: " .. T.what(err2))
        T.tick(1)
        T.ok(#S.clicks == 9 and answers and answers[1] == false and rawequal(answers[2], covered),
          "no second click, and the callback hears the same error value")
        T.ok(T.count("overlay: a double-click at (410,120) was not made — something else is drawn over the point") == 1,
          T.dump())
        S.clickErr = nil
        -- The second half not sent by the host: true at the call, `covered` to the callback.
        local cut = T.E("covered", "the window at the point changed after the first click: the second click was not sent")
        S.secondErr = cut
        answers = nil
        local ok3 = T.O.doubleClick(410, 120, { button = "right" }, function(...) answers = table.pack(...) end)
        T.ok(ok3 == true and #S.clicks == 10, "true at the call, the first click made")
        T.tick(1)
        T.ok(#S.clicks == 10 and answers and answers[1] == false and rawequal(answers[2], cut),
          "the second not made, and the callback hears the host's error value: " .. T.what(answers and answers[2]))
        S.secondErr = nil
    "##);
}
