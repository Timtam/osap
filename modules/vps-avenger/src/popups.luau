-- Where Avenger draws its popups, for the runtime's drawn test (O.menuTests.drawn,
-- docs/api/overlay.md): the rectangle a press's popup will be painted in, and the smallest popup
-- that counts there.
--
-- WHY. The first Mac test of the page (2026-10-14) clicked the zoom field twice and no menu test saw
-- the list in 8 s: no window of Logic's process came on screen for newWindow, and no AXMenuOpened
-- came for the system's menus, which the host saw for Logic's own menus all along. In current JUCE
-- that is a popup given a parent component — painted as a child of the plug-in's own editor, with
-- no window and no notification (juce_PopupMenu.cpp, MenuWindow) — and the avenger_control
-- project's numbers fit that: every popup row it clicks is written from Avenger's corner, and the
-- macro button menu's item lies 26 right of button 1's click but only 7 right of button 2's,
-- pushed back inside Avenger's right edge. What Logic does was not seen: no picture was taken after
-- either click (not a calibrating run). So the drawn test looks where the popup will be, and its
-- log lines say which it was: `drawn: a popup …`, or `drawn: nothing has changed …` (no popup
-- opened). newWindow stays on the list for a DAW that draws the popups as windows (JUCE's other
-- case, and most likely Windows'), and a choice follows the test that placed its item.
--
-- THE REGIONS, at 50 % from Avenger's corner as every table of this module, cut to Avenger's 871 x
-- 561 and placed at the factor of the press: STARTING VALUES worked out from the project's points,
-- not seen. A popup's whole rectangle has to lie in its region — the item is placed in the area the
-- test sees, and a popup cut by its region's edge is taken as smaller than it is — so they are
-- generous:
--   the zoom list   below the zoom field, 21 rows of 11.5 from y 18, as wide as the field
--   MENU's popup    below MENU, its four rows from y 18
--   the redo list   below UNDO, as many rows as there are steps to redo
--   a right click   for every menu a right click opens (the oscillator, Drums and Trig tabs, macro
--                   tab 1, the macro knobs' fields, the macro buttons): from 120 left of the click
--                   to 120 right of it, from just below it to 145 below — JUCE opens a popup at
--                   the pointer and pushes it back inside its parent at the edges
-- The menu shots of a calibrating run show where each popup really is, and the pictures the Avenger
-- developer is asked for settle them (TODO.md, "The first Mac test of the Avenger page", item 5).

local G = host.include("src/geometry.luau")
local H = host.include("src/header.luau")

local Pp = {}

-- { rect at 50 %, the smallest popup at 50 % { w, h } }. The smallest is two rows of the zoom
-- list's 11.5 tall and 30 across for a menu, and three rows for the zoom list, which zoomEntry
-- (main.luau) refuses below that anyway.
Pp.ZOOM_LIST = { rect = { 12, 18, 72, 262 }, min = { 20, 34.5 } }
Pp.MENU = { rect = { 245, 18, 335, 78 }, min = { 30, 23 } }
Pp.REDO = { rect = { 290, 18, 430, 200 }, min = { 30, 23 } }
-- From the click, at 50 %.
Pp.AT_CLICK = { rect = { -120, 1, 120, 145 }, min = { 30, 23 } }

-- Avenger's whole interface at 50 %, which a popup with a parent component cannot leave.
local AVENGER = { 0, 0, G.WIDTH_AT_50, G.HEIGHT_AT_50 }

-- Which popup a press opens, by the label the ring and the page press it with (main.luau,
-- actions.luau): the zoom field's, MENU's items, UNDO's right click. Every other press with a
-- point and the right button is a right click's menu (actions/osc.luau, actions/macros.luau).
local BY_LABEL = {
  Zoom = Pp.ZOOM_LIST,
  ["Load preset"] = Pp.MENU,
  ["Save preset"] = Pp.MENU,
  ["Save preset as"] = Pp.MENU,
  ["Initialize preset"] = Pp.MENU,
  ["Redo list"] = Pp.REDO,
  Redo = Pp.REDO,
}

local function cut(a, b)
  local r = { math.max(a[1], b[1]), math.max(a[2], b[2]), math.min(a[3], b[3]), math.min(a[4], b[4]) }
  if r[3] - r[1] < 2 or r[4] - r[2] < 2 then return nil end
  return r
end

-- The drawn test's `region`: for `press` ({ label, x?, y?, button? }) on overlay `o`, the
-- rectangle on screen its popup is painted in, and the smallest popup there in screen pixels; nil
-- for a press that opens none of these (nothing is armed then, and the log says so).
function Pp.region(o, press)
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return nil end
  local whole = G.placeRect(origin, k, AVENGER)
  if not whole then return nil end
  local kind = BY_LABEL[press.label]
  local r
  if kind then
    r = G.placeRect(origin, k, kind.rect)
  elseif press.button == "right" and press.x and press.y then
    kind = Pp.AT_CLICK
    local a = kind.rect
    r = { math.floor(press.x + a[1] * k), math.floor(press.y + a[2] * k), math.ceil(press.x + a[3] * k),
      math.ceil(press.y + a[4] * k) }
  else
    return nil
  end
  r = r and cut(r, whole)
  if not r then return nil end
  return r, { kind.min[1] * k, kind.min[2] * k }
end

-- The drawn test the header overlay's binding lists (main.luau).
Pp.spec = { name = "drawn", region = Pp.region }

-- A `menuItem` for a right click's menu (O:chooseMenuItem): the item at table point `item` —
-- the project's figures, from Avenger's corner, at the factor the opener was clicked at — and only
-- in a popup whose rows reach it. Across, at the item's own x, the project's, kept inside what was
-- seen: not the middle of the area seen, which holds whatever else the same right click changed —
-- a tab it selected, under the region — and can lie beside the popup. A popup whose rows do not
-- reach the item's is not this one (`false`): a tooltip, a small window of the DAW's that newWindow
-- saw first; the runtime looks at the next answer, and says at the bound that none was a menu to
-- choose in. `pick.seen` notes that the item was placed in a popup (main.luau, closeLeftOpen). The
-- log gives the popup's place in the table's units, so the items can be written from the popup's
-- corner once one has been seen.
function Pp.row(label, item, pick)
  return function(o, m, k)
    local origin = o:origin()
    if not (k and origin) then return nil end
    local cx, cy = G.place(origin, k, 0, 0)
    local x, y = G.place(origin, k, item[1], item[2])
    -- Said once per window by the runtime, with this reason.
    if y < m.y or y >= m.y + m.h then
      return false, string.format("its rows do not reach the item's row at %.1f", item[2])
    end
    pick.seen = true
    local at = string.format("(%.1f,%.1f) %.1fx%.1f from Avenger's corner in 50 %% units", (m.x - cx) / k,
      (m.y - cy) / k, m.w / k, m.h / k)
    local dx = math.max(0, math.min(m.w - 1, x - m.x))
    host.log.info(string.format("[avenger] '%s': the popup at %s; the item %.1f below its top and %.1f from "
      .. "its left edge, at the item's own x%s", label, at, (y - m.y) / k, dx / k,
      dx ~= x - m.x and ", kept inside the area seen" or ""))
    return { dx, y - m.y }
  end
end

return Pp
