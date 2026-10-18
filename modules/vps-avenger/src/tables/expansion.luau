-- VPS Avenger's expansion browser, in its upper left: the Expansion View control (which closes an
-- open expansion), the grid or list of expansions, and an open expansion's categories and presets.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, and the regions and boxes its
-- clicker reads the browser in (clicker/__init__.py), taken over with its developer's permission
-- (NOTICE): at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's own
-- interface; the numbers as the project wrote them, starting values nobody here has checked
-- (geometry.luau, "Where the numbers come from"). Not taken over: the project's scrollbar tables,
-- a page for each view and roster size, and its tables of where a category's presets lie once
-- scrolled, measured for one roster and one set of category sizes — the browser's actions read the
-- grid and the lists and click the words read (src/actions/browser.luau).
--
-- WORKED OUT HERE, not the project's (the plan for the browser, §2.1): the browser's picture, the
-- columns' text regions, the star column, the lists' wheel points and scrollbar tracks, and the
-- grid's columns per view — each from the project's numbers beside it, and STARTING VALUES until
-- a picture of the browser settles them (TODO.md, D14 and D15).

local T = {}

-- The browser's whole picture, for its actions: the Expansion View control and its caption, FAV's
-- box, the area, both scrollbars (x 12 and 228) and the ninth category row (226 ± 10).
T.BROWSER = { 5, 40, 233, 240 }

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

-- An open expansion's two lists as text, each a region of its own: the host joins engine lines that
-- overlap vertically into one row (docs/api/ocr.md), so a category and a preset at the same height
-- would come back as one line of one region. The categories right of their scrollbar at x 12, the
-- nine rows of 20 from y 66; the presets left of their stars at x 220, the fourteen rows of 12.5
-- from y 60; the stars' column round x 220, as wide as a star's box, from the first star's top — FAV's
-- box above it reaches y 55, and a grid cell that held part of it would count FAV's light as the
-- first row's star.
T.CATEGORY_TEXT = { 16, 56, 70, 236 }
T.PRESET_TEXT = { 72, 53, 212, 229 }
T.STAR_COLUMN = { 215, 55.5, 225, 229 }

-- The three lists the browser moves: the grid of expansions, an open expansion's categories and its
-- presets — each with its text region (`rect`), where the wheel is turned (the list's middle, away
-- from both scrollbars) and its scrollbar's track, clicked at its end to move the list a page: x,
-- and y at the top (`up`) and the bottom (`down`) — the project's track ends (its EXPANSION_SCROLL_*,
-- EXPANSION_CATEGORY_SCROLL_* and EXPANSION_PRESET_SCROLL_*, the grid's from its List view's).
T.LISTS = {
  grid = { rect = T.AREA, wheel = { 117, 140 }, track = { x = 228, up = 58, down = 222 } },
  categories = { rect = T.CATEGORY_TEXT, wheel = { 42, 146 }, track = { x = 12, up = 66, down = 218 } },
  presets = { rect = T.PRESET_TEXT, wheel = { 142, 141 }, track = { x = 228, up = 58, down = 227 } },
}

-- The grid's columns in view `view`: the middle of each (`xs`), half a column's width (`half`)
-- and the distance from one row of tiles to the next (`pitch`). List: one column at the project's
-- x, its rows 36 apart (its List view's y cycle); 8 per Row: the project's tiles, from x 23 every
-- 27.5, rows 20 apart; 1 to 7 per Row: the area split evenly, as the project places their tiles (its
-- expansion_cell_pos), as many rows as columns. nil for a view Avenger does not have.
function T.columns(view)
  if view == "List" then
    return { xs = { T.LIST_X }, half = (T.AREA[3] - T.AREA[1]) / 2, pitch = 36 }
  end
  if view == "8 per Row" then
    local xs = {}
    for c = 0, 7 do xs[c + 1] = T.GRID8_X0 + c * T.GRID8_X_STEP end
    return { xs = xs, half = T.GRID8_X_STEP / 2, pitch = T.GRID8_Y_STEP }
  end
  local n = tonumber(string.match(tostring(view or ""), "^(%d) per Row$"))
  if not (n and n >= 1 and n <= 7) then return nil end
  local w = (T.AREA[3] - T.AREA[1]) / n
  local xs = {}
  for c = 0, n - 1 do xs[c + 1] = T.AREA[1] + (c + 0.5) * w end
  return { xs = xs, half = w / 2, pitch = (T.AREA[4] - T.AREA[2]) / n }
end

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

return T
