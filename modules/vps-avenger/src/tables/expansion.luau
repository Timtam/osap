-- VPS Avenger's expansion browser, in its upper left: the Expansion View control (which closes an
-- open expansion), the grid or list of expansions, and an open expansion's categories and presets.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, and the regions and boxes its
-- clicker reads the browser in (clicker/__init__.py), taken over with its developer's permission
-- (NOTICE): at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's own
-- interface; the numbers as the project wrote them, starting values nobody here has checked
-- (geometry.luau, "Where the numbers come from"). Not taken over: the project's scrollbar tables,
-- a page for each view and roster size, which the plan for the browser leaves behind for a read
-- of the grid (TODO.md, "VPS Avenger, the page").

local T = {}

-- The Expansion View control at (15, 50): it opens the view menu while the browser shows the
-- expansions, and closes the open expansion while one is open.
T.VIEW = { 15, 50 }
T.CLOSE = { 15, 50 }

-- What the control reads as: one of these while the browser is closed; an icon while an
-- expansion is open. Read from (9, 43), 78 across and 16 down.
T.VIEW_OPTIONS = {
  "List",
  "1 per Row",
  "2 per Row",
  "3 per Row",
  "4 per Row",
  "5 per Row",
  "6 per Row",
  "7 per Row",
  "8 per Row",
}
T.VIEW_TEXT = { 9, 43, 9 + 78, 43 + 16 }

-- The view menu: List at y 60, then 1 to 8 per Row from y 74, 11.5 apart, at x 27.
T.VIEW_MENU_X = 27
T.VIEW_MENU_LIST_Y = 60
T.VIEW_MENU_GRID_FIRST_Y = 74
T.VIEW_MENU_Y_PITCH = 11.5

-- The browser's area, from (10, 56) to (225, 224); its text is read over the whole of it.
T.AREA = { 10, 56, 225, 224 }

-- The list view's column, and the 8 per Row grid: the first tile at (23, 68), 27.5 across and 20
-- down from one to the next.
T.LIST_X = 117.5
T.LIST_Y = { 73, 109, 145, 181, 217 }
T.GRID8_X0 = 23
T.GRID8_X_STEP = 27.5
T.GRID8_Y0 = 68
T.GRID8_Y_STEP = 20

-- An open expansion: nine category rows from (42, 66), 20 apart; fourteen preset rows from
-- (142, 60), 12.5 apart, with each row's favourite star at x 220; FAV, the favourites view, at
-- (212, 50).
T.CATEGORY_X = 42
T.CATEGORY_Y0 = 66
T.CATEGORY_Y_STEP = 20
T.CATEGORY_VISIBLE_ROWS = 9
T.PRESET_X = 142
T.PRESET_FAVORITE_X = 220
T.PRESET_Y0 = 60
T.PRESET_Y_STEP = 12.5
T.PRESET_VISIBLE_ROWS = 14
T.FAVORITE_VIEW = { 212, 50 }
-- FAV is read in a box 25 by 10 round its point; a preset's star in one 9 by 9.
T.FAVORITE_VIEW_STATE = { 25, 10 }
T.FAVORITE_STATE = { 9, 9 }

-- Where row `view` of the view menu is (the project's expansion_view_menu_pos), or nil for a view
-- Avenger does not have.
function T.viewMenuPoint(view)
  for i, v in ipairs(T.VIEW_OPTIONS) do
    if v == view then
      if i == 1 then return { T.VIEW_MENU_X, T.VIEW_MENU_LIST_Y } end
      return { T.VIEW_MENU_X, T.VIEW_MENU_GRID_FIRST_Y + (i - 2) * T.VIEW_MENU_Y_PITCH }
    end
  end
  return nil
end

-- Category row `index` (counted from 0) of `total`, as Avenger shows them once its list is
-- scrolled to the top (index under 9) or to the bottom (the project's expansion_category_pos).
function T.categoryPoint(index, total)
  index, total = math.max(0, math.floor(index)), math.max(1, math.floor(total))
  local row = index
  if index >= T.CATEGORY_VISIBLE_ROWS then
    row = T.CATEGORY_VISIBLE_ROWS - 1 - math.max(0, total - 1 - index)
  end
  row = math.clamp(row, 0, T.CATEGORY_VISIBLE_ROWS - 1)
  return { T.CATEGORY_X, T.CATEGORY_Y0 + row * T.CATEGORY_Y_STEP }
end

-- Preset row `index` (counted from 0) of the fourteen shown, or nil past them (the project's
-- expansion_preset_pos).
function T.presetPoint(index)
  index = math.floor(index)
  if index < 0 or index >= T.PRESET_VISIBLE_ROWS then return nil end
  return { T.PRESET_X, T.PRESET_Y0 + index * T.PRESET_Y_STEP }
end

return T
