-- VPS Avenger's oscillator strip: the OSC tabs, Drums and Trig, with Mute and Solo above each tab
-- and a small dot in each OSC tab.
--
-- SOURCE: the avenger_control project's clicker/header_map.py and the points its clicker reads
-- the strip at (clicker/__init__.py), taken over with its developer's permission (NOTICE): at 50 %
-- of Avenger's zoom, in points from the top-left corner of Avenger's own interface, as every
-- table of this module is (geometry.luau, "Where the numbers come from"). The numbers are the
-- project's as it wrote them; the names are its names in this module's case. None has been
-- checked against an Avenger here: they are starting values, which the page's reads log and a
-- calibrating run photographs (read/open.luau, `strip-<zoom>pct.png`).

local T = {}

-- The OSC tabs begin at x 244. Their names are read between y 28 and 42, up to x 536 — or 548
-- with eight oscillators, where no "Add OSC" tab is left right of them.
T.PANEL_LEFT = 244
T.NAME_RIGHT = 536
T.NAME_RIGHT_WITH_ADD = 548
T.NAME_TOP = 28
T.NAME_BOTTOM = 42

-- Where a tab's selection shows: y 30 to 40, across the tab — the OSC tabs as the count shares
-- the strip out, Drums from 551 to 601 and Trig from 602 to 631.
T.SELECTION_TOP = 30
T.SELECTION_BOTTOM = 40
-- Where the selected tab's dark strip is read: in the bottom rows above y 39.5 — this module's own,
-- half a point above the selection's bottom, which is the strip's own lower edge. Measured on
-- `OSC Tabs/Avenger 100, 8 OSCs, OSC 3 selected.png`: the selected tab leads by 16.8 there, by 5.4
-- half a point lower and by 0.1 a point lower. It holds with Avenger's corner placed half a point
-- off.
T.SELECTION_BAND_BOTTOM = 39.5
T.DRUMS_SELECTION = { 551, 601 }
T.TRIG_SELECTION = { 602, 631 }

-- The dots, one in each OSC tab, its left edge at the tab's left and its middle 3.25 right of it:
-- looked for from 3 below the selection's top (the project's OSC_TAB_SELECTION_TOP + 3.0) to its
-- bottom, and from 3 left of the panel's left — so that the first tab's dot is whole with Avenger's
-- corner placed 1.5 off — to the names' right with eight oscillators. Drums' and Trig's dots, from
-- 553 and 604, lie right of it. Measured on `OSC Tabs/Avenger 100, 8 OSCs, OSC 1 selected.png`.
T.DOT_TOP = T.SELECTION_TOP + 3
T.DOT_BAND = { T.PANEL_LEFT - 3, T.DOT_TOP, T.NAME_RIGHT_WITH_ADD, T.SELECTION_BOTTOM }
T.DOT_MIDDLE = T.PANEL_LEFT + 3.25

-- The tabs and their menus are clicked at y 35; Mute and Solo at y 26, 7 left and 7 right of the
-- tab's middle, each read in a box 13 wide and 8 tall. The ARP row is at y 62.
T.TAB_Y = 35
T.MENU_LEFT = 265
T.MUTE_Y = 26
T.MUTE_X_OFFSET = -7
T.SOLO_X_OFFSET = 7
T.BUTTON_STATE = { 13, 8 }
T.ARP_Y = 62

-- Add OSC, Drums and Trig, clicked at y 35.
T.ADD_X = 544
T.DRUMS_TAB_X = 576
T.TRIG_TAB_X = 616

-- The items of a tab's menu, opened with the right button at the tab's menu point (menuX, TAB_Y)
-- and clicked where the project's clicker clicks them: an OSC tab's at its menu point's x, Copy at
-- y 46, Paste at 57, Initialize at 69 and Delete at 80; the Drums and the Trig tab's one item,
-- Initialize, at their tab's x and y 69 (its click_osc_menu, click_drums_tab and click_trig_tab).
T.MENU_ITEM_Y = { Copy = 46, Paste = 57, Initialize = 69, Delete = 80 }
T.DRUMS_TRIG_MENU_Y = 69

-- Mute and Solo of Drums and of Trig: the points the project's clicker reads them at.
T.DRUMS_MUTE = { 569, 26 }
T.DRUMS_SOLO = { 583, 26 }
T.TRIG_MUTE = { 609, 26 }
T.TRIG_SOLO = { 623, 26 }

-- The whole strip as the project reads it for text: from (238, 24), 395 across and 24 down.
T.AREA = { 238, 24, 238 + 395, 24 + 24 }

-- The width of an OSC tab, and how far apart their menus are, by how many oscillators there are.
T.STEP_BY_COUNT = {
  { 293, 0 },
  { 146, 147 },
  { 97, 98 },
  { 73, 73 },
  { 58, 58 },
  { 48, 48 },
  { 41, 42 },
  { 38, 38 },
}

local function clampCount(count) return math.clamp(math.floor(count), 1, 8) end

-- The middle of OSC tab `index` of `count` (the project's osc_tab_x).
function T.tabX(index, count)
  count = clampCount(count)
  index = math.clamp(math.floor(index), 1, count)
  return T.PANEL_LEFT + (index - 0.5) * T.STEP_BY_COUNT[count][1]
end

-- Where OSC tab `index` of `count` opens its menu (the project's osc_menu_x).
function T.menuX(index, count)
  count = clampCount(count)
  index = math.clamp(math.floor(index), 1, count)
  return T.MENU_LEFT + (index - 1) * T.STEP_BY_COUNT[count][2]
end

-- The right edge the OSC tabs share out, for `count` of them.
function T.namesRight(count)
  return clampCount(count) == 8 and T.NAME_RIGHT_WITH_ADD or T.NAME_RIGHT
end

-- OSC tab `index` of `count` as a rectangle from `top` to `bottom`: the strip from the panel's
-- left to the names' right, shared out evenly (the project's tab_width).
function T.tabSpan(index, count, top, bottom)
  count = clampCount(count)
  local w = (T.namesRight(count) - T.PANEL_LEFT) / count
  local x1 = T.PANEL_LEFT + (index - 1) * w
  return { x1, top, x1 + w, bottom }
end

return T
