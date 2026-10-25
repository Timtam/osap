-- The expansion browser, read off the page's picture: its view (List, or 1 to 8 per Row) while it
-- shows the expansions, where Avenger draws it; whether an expansion is open instead; and whether
-- the favourites view (FAV) is on.
--
-- SOURCE: the avenger_control project's detect_expansion_state (its actions.py) and its
-- detect_expansion_favorite_view (clicker/__init__.py), taken over with its developer's permission
-- (NOTICE): the view's caption read as text — an open expansion shows an icon there. The rest is
-- this module's own: since the second Mac test of the page (2026-10-24) the browser is told by the
-- open expansion's name in its header (src/read/browser.luau, Br.state), FAV by its star column;
-- since the third (2026-10-25) also by a picture test of the header's title band and of the
-- categories' scrollbar (E.look) — pixels only. Nothing below the header is read as text: the
-- project's reading of the area's words against the database — the grid's names, the categories,
-- the preset rows — read nothing at the browser's sizes, and the maintainer's decision of that day
-- leaves the lists to the preset database and the places to src/tables/pages.luau.

local P = host.include("src/read/picture.luau")
local X = host.include("src/tables/expansion.luau")

local E = {}

-- A star's bright pixels, (r + g + b) / 3 above 150 (src/read/browser.luau, Br.stars).
E.BRIGHT = host.screen.predicate("r + g + b >= 451")
-- FAV is told by the star column — this module's own: FAV's label is drawn the same in every state
-- (98, 110 and 98 light pixels of its 1000 on the second Mac test's pictures), where the project
-- read its brightest pixel and its bright ones. With FAV on, Avenger draws a star in every preset's
-- row: a marked one light, (r + g + b) / 3 of 150 or more (E.LIGHT; 322 such pixels in the column's
-- 6940, `Expansions/Avenger, 100, Open African House,Fav Stars On, Marked Presets 1,4,8,9.png`),
-- and an unmarked one dim, of 45 or more (E.DRAWN). With FAV off nothing of a star is drawn, and
-- only the selected preset's row is lit, all over (`… Fav Stars Off.png`). A star is looked for in
-- the MIDDLE of its box, STAR_MIDDLE across and down round the row's middle, off a grid of one cell
-- per pixel row of the column (at most 256): a star fills 0.15 to 0.45 of it (that grid's cells are
-- the column's width), the lit row 0.85 to 1, and with FAV off every other row none of it — also
-- with Avenger's corner placed 1.5 units off either way, where the lit row reaches 0.06 to 0.14
-- into the next row's whole 9-unit box, which read FAV on when the whole box was read. So FAV is
-- on where the column holds
-- light pixels, or a star's middle holds a share of drawn ones from STAR_DRAWN_MIN to STAR_DRAWN_MAX
-- — a star, not a lit row — and off where neither: with FAV on and none of the shown presets
-- marked, the dim stars still say it, a category of two rows with the first selected included.
-- Asked only while an expansion is open: the grid's tiles light the same column. The light pixels'
-- grid is of one column, four cells to a row of the presets, as Br.stars reads the stars
-- (src/read/browser.luau); the whole read is a few hundred turns.
E.LIGHT = host.screen.predicate("r + g + b >= 450")
E.DRAWN = host.screen.predicate("r + g + b >= 135")
local STAR_DRAWN_MIN, STAR_DRAWN_MAX = 0.05, 0.6
local STAR_MIDDLE = 5
local STAR_CELLS = 4

-- The shortest text of the header taken for an expansion's name by similarity (DB.nameIn).
local NAME_MIN = 5
E.NAME_MIN = NAME_MIN

-- The picture test of the grid against an open expansion (tables/expansion.luau, TITLE_BAND and
-- LIST_BAR, measured on the second Mac test's 22 pictures of the grid and 3 of an open expansion):
-- the title band's share of light pixels (E.BRIGHT) — 0.00 % on every picture of the grid, 3.0 to
-- 7.4 % with an expansion open — and the categories' scrollbar's share of pixels of r + g + b 180
-- or more (E.BAR) — 0.0 to 5.0 % on the grid, 26 to 36.5 % open. An expansion is open where the
-- title band holds TITLE_OPEN or more, whatever its name reads: an open expansion draws its title
-- there, and TITLE_OPEN is a sixth of the least measured ("Factory", seven letters at 50 %), so
-- that the catalog's shortest name, "PSY", should light it too — not measured. The band holds no
-- title where it is at TITLE_DARK or less, and the grid shows where the bar is then at BAR_DARK or
-- less as well; the bar, measured in 8 per Row only, never says open by itself. Not measured either:
-- a Retina capture, the views but 8 per Row, the search field open, the pointer over the header.
E.BAR = host.screen.predicate("r + g + b >= 180")
E.TITLE_OPEN = 0.005
E.TITLE_DARK = 0.002
E.BAR_DARK = 0.10

-- The picture test's grids: only their shares over the whole band and the whole bar are wanted, so
-- a few cells each — TITLE_CELLS across the band, one row; BAR_CELLS down the bar, one column — and
-- Luau's work on them is a few dozen turns (a cell a unit across, as the other grids are, would be a
-- thousand cells and some ten thousand turns a read). A cell's share is the host's, rounded to
-- 1/255 (docs/api/screen.md), so a band with no light pixel is 0 exactly, and one lit pixel in a
-- cell counts.
local TITLE_CELLS, BAR_CELLS = 24, 14

-- The browser's grids, asked of the page's picture with the rest (read/open.luau) and of the
-- browser's own (src/read/browser.luau): the title band, the categories' scrollbar, and FAV's star
-- column twice.
function E.ask(pl, out)
  local band = pl:rect(X.TITLE_BAND)
  if band then
    out[#out + 1] = { name = "title band", kind = "cells", rect = band, cols = math.min(TITLE_CELLS, band[3] - band[1]),
      rows = 1, pred = E.BRIGHT }
  end
  local bar = pl:rect(X.LIST_BAR)
  if bar then
    out[#out + 1] = { name = "list bar", kind = "cells", rect = bar, cols = 1, rows = math.min(BAR_CELLS, bar[4] - bar[2]),
      pred = E.BAR }
  end
  local r = pl:rect(X.STAR_COLUMN)
  if not r then return end
  out[#out + 1] = { name = "fav stars", kind = "cells", rect = r, cols = 1, rows = X.PRESET_VISIBLE_ROWS * STAR_CELLS,
    pred = E.LIGHT }
  out[#out + 1] = { name = "fav star shapes", kind = "cells", rect = r, cols = 1, rows = math.clamp(r[4] - r[2], 1, 256),
    pred = E.DRAWN }
end

-- How many of the column's rows hold a star drawn in the middle of their box, by grid `g` of
-- E.DRAWN, and of how many in the picture; nil where none is.
local function starsDrawn(pl, g)
  local sx = pl:exact(X.PRESET_FAVORITE_X, 0)
  local half = STAR_MIDDLE / 2 * pl.k
  local drawn, boxes = 0, 0
  for i = 0, X.PRESET_VISIBLE_ROWS - 1 do
    local _, y = pl:exact(0, X.PRESET_Y0 + i * X.PRESET_Y_STEP)
    local s = g:share({ math.floor(sx - half + 0.5), math.floor(y - half + 0.5), math.floor(sx + half + 0.5),
      math.floor(y + half + 0.5) })
    if s ~= nil then
      boxes += 1
      if s >= STAR_DRAWN_MIN and s <= STAR_DRAWN_MAX then drawn += 1 end
    end
  end
  if boxes == 0 then return nil end
  return drawn, boxes
end

-- FAV into `v.favoriteView`, read only while `open` (an expansion read open): nil otherwise.
function E.favorite(pl, got, v, log, marks, open)
  local x, y = pl:point(X.FAVORITE_VIEW[1], X.FAVORITE_VIEW[2])
  marks[#marks + 1] = { x = x, y = y }
  if not open then
    log("expansion: FAV not read — no expansion reads open")
    return
  end
  local g, d, r = got["fav stars"], got["fav star shapes"], pl:rect(X.STAR_COLUMN)
  local share, _, any = nil, nil, false
  if g and r then share, _, any = g:share(r) end
  local drawn, boxes = nil, nil
  if d then drawn, boxes = starsDrawn(pl, d) end
  if share == nil or drawn == nil then
    local missing = share == nil and "fav stars" or "fav star shapes"
    log("expansion: FAV not read (" .. tostring(got[missing] and "its column is not in the picture" or got.why[missing]) .. ")")
    return
  end
  v.favoriteView = any or drawn > 0
  log(string.format("expansion: FAV — the star column %s light (the project's %.0f pixels of %.0f), a star drawn in the "
    .. "middle of %d of the %d rows' boxes — %s", P.pct(share), share * pl:toolPixels(r), pl:toolPixels(r), drawn, boxes,
    v.favoriteView and "ON" or "OFF"))
end

-- The view's caption and the header's name as OCR entries.
function E.entries(pl, out)
  local r = pl:rect(X.VIEW_TEXT)
  if r then out[#out + 1] = { name = "expansion view", region = r } end
  r = pl:rect(X.NAME_TEXT)
  if r then out[#out + 1] = { name = "expansion name", region = r } end
end

-- The view a caption reads as, or nil: "List", or the first of 1 to 8 in a caption with "row" in
-- it (the project's _parse_expansion_view_status).
function E.parseView(text)
  local t = string.lower((string.gsub(tostring(text or ""), "%s+", " ")))
  if string.find(t, "list", 1, true) then return "List" end
  if not string.find(t, "row", 1, true) then return nil end
  for n = 1, 8 do
    if string.find(t, tostring(n), 1, true) then return n .. " per Row" end
  end
  return nil
end

-- What the picture test says, from the grids `got` (E.ask's names): { title, bar = the shares, nil
-- where not read; verdict = "open" (the title band lit), "grid" (no title, the bar dark), "dark" (no
-- title, the bar lit or not read), "between" (a few light pixels: neither), or nil (the band not
-- read); words, for the log }.
function E.look(pl, got)
  local function shareOf(name, rect)
    local g, r = got[name], pl:rect(rect)
    if not (g and r) then return nil end
    return (g:share(r))
  end
  local title, bar = shareOf("title band", X.TITLE_BAND), shareOf("list bar", X.LIST_BAR)
  local verdict = nil
  if title ~= nil then
    if title >= E.TITLE_OPEN then
      verdict = "open"
    elseif title <= E.TITLE_DARK then
      verdict = (bar ~= nil and bar <= E.BAR_DARK) and "grid" or "dark"
    else
      verdict = "between"
    end
  end
  local words = string.format("title band %s lit, list bar %s lit — %s", P.pct(title), P.pct(bar), ({
    open = "an expansion's title", grid = "the grid", dark = "no title, the bar not dark",
    between = "neither", })[verdict] or "not read")
  return { title = title, bar = bar, verdict = verdict, words = words }
end

return E
