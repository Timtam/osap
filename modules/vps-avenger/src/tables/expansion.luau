-- VPS Avenger's expansion browser, in its upper left: the Expansion View control (which closes an
-- open expansion), the grid or list of expansions, and an open expansion's categories and presets.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, and the regions and boxes its
-- clicker reads the browser in (clicker/__init__.py), taken over with its developer's permission
-- (NOTICE): at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's own
-- interface; the numbers as the project wrote them, starting values nobody here has checked
-- (geometry.luau, "Where the numbers come from"). The project's page and scroll tables, by which
-- the browser's actions click every expansion, category and preset by its place as the project
-- clicks them (the maintainer's decision of 2026-10-25), are in src/tables/pages.luau.
--
-- WORKED OUT HERE, not the project's (the plan for the browser, §2.1): the browser's picture, the
-- columns' regions a change is watched in, the star column, and the header's name and title band —
-- each from the project's numbers beside it or measured on the second Mac test's pictures, and
-- STARTING VALUES until a picture of the browser settles them (TODO.md, D15).

local T = {}

-- The browser's whole picture, for its actions: the Expansion View control and its caption, FAV's
-- box, the area, both scrollbars (x 12 and 228) and the ninth category row (226 ± 10).
T.BROWSER = { 5, 40, 233, 240 }

-- The Expansion View control at (15, 50): it opens the view menu while the browser shows the
-- expansions, and closes the open expansion while one is open.
T.VIEW = { 15, 50 }
T.CLOSE = { 15, 50 }

-- What the control reads as on an Avenger that draws the view's label there: one of these while
-- the browser is closed. Read from (9, 43), 78 across and 16 down. The second Mac test's Avenger
-- draws an icon there, open or closed (`Expansions/Avenger 100 closed.png` and every
-- `Avenger NN closed.png`): the view is then the one set or confirmed (src/read/browser.luau).
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

-- The open expansion's name, centred in the browser's header — this module's own, measured on the
-- second Mac test's pictures: "African House" from 69 to 122 at 100 %
-- (`Expansions/Avenger, 100, Open African House,Fav Stars Off.png`), "Factory" from 73 to 114 at
-- 50 % (`Expansions/VPS Avenger Open Expansion.png`), the title's glyphs from y 45 to 51.5 in both;
-- with no expansion open nothing light lies between 20 and 146 there at 50, 80 or 100 %, and the BPM
-- ramp starts at 147. Read from 22, clear of the back arrow left of it, to 144, and from 43 to 54:
-- 2.5 points of room below the glyphs for a descender, and clear of the grid's first row of tiles,
-- whose dark outlines begin at 55 to 56. Until the third Mac test it reached 58, and those outlines
-- were ink to the host's crop (docs/api/ocr.md, a pixel far from the mean of the crop's corners) on
-- 21 of the 22 closed pictures — the 21 `Avenger NN closed.png`, 50 to 200 %, and `VPS Avenger
-- Closed Expansion.png`; at 80 % in its bottom 3 rows of 24 — so the header over the grid read
-- `none`, never `blank`, and nothing was taken as closed. To 54 it is blank on all 22, and holds
-- the title's ink on the three open ones.
T.NAME_TEXT = { 22, 43, 144, 54 }

-- The picture test of the grid against an open expansion (src/read/expansion.luau, E.look) — this
-- module's own, measured on the same 22 closed pictures and the three open ones (`VPS Avenger Open
-- Expansion.png` at 50 %, the two `Avenger, 100, Open African House,…` at 100 %), with Avenger's
-- corner placed exactly and 2 pixels off either way:
--   TITLE_BAND  the header's band the open expansion's title is drawn in: its share of light
--               pixels, r + g + b of 451 or more — none in any closed picture, 3.0 % (50 %) and
--               7.4 % (100 %) open. Whatever the name reads, a lit title is an open expansion.
--   LIST_BAR    the categories' scrollbar's track left of them, round x 12: its share of pixels
--               with r + g + b of 180 or more — 0.0 to 5.0 % closed, where a dark box lies there,
--               and 26.0 to 36.5 % open. Measured in 8 per Row only, so it only confirms the grid
--               where the title is dark; it never says open by itself.
T.TITLE_BAND = { 22, 44, 144, 53 }
T.LIST_BAR = { 9, 70, 15, 210 }

-- The view menu: List at y 60, then 1 to 8 per Row from y 74, 11.5 apart, at x 27.
T.VIEW_MENU_X = 27
T.VIEW_MENU_LIST_Y = 60
T.VIEW_MENU_GRID_FIRST_Y = 74
T.VIEW_MENU_Y_PITCH = 11.5

-- The browser's area, from (10, 56) to (225, 224): the grid's tiles lie in it, and a change after a
-- click on its scrollbar is watched for in it. Nothing in it is read as text — the expansions' names
-- there are about 5 pixels tall at 80 % on a 1x display, and the avenger_control developer found no
-- recogniser that reads them (the maintainer's decision of 2026-10-25).
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

-- An open expansion's two lists, where a change after a click on them or their scrollbars is watched
-- for — never read as text: the categories right of their scrollbar at x 12, the nine rows of 20
-- from y 66; the presets left of their stars at x 220, the fourteen rows of 12.5 from y 60; and the
-- stars' column round x 220, as wide as a star's box, from the first star's top — FAV's box above it
-- reaches y 55, and a grid cell that held part of it would count FAV's light as the first row's star.
T.CATEGORY_TEXT = { 16, 56, 70, 236 }
T.PRESET_TEXT = { 72, 53, 212, 229 }
T.STAR_COLUMN = { 215, 55.5, 225, 229 }

-- The three lists the browser scrolls by their scrollbars (src/tables/pages.luau): the grid of
-- expansions, an open expansion's categories and its presets, each with the region its change is
-- watched in, top and bottom quarter (src/actions/browser.luau, `bands`).
T.LISTS = {
  grid = { rect = T.AREA },
  categories = { rect = T.CATEGORY_TEXT },
  presets = { rect = T.PRESET_TEXT },
}

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
