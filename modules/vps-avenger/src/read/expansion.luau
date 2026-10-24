-- The expansion browser, read off the page's picture: its view (List, or 1 to 8 per Row) while it
-- shows the expansions, where Avenger draws it; whether an expansion is open instead, and which;
-- and whether the favourites view (FAV) is on.
--
-- SOURCE: the avenger_control project's detect_expansion_state and its helpers (its actions.py)
-- and its detect_expansion_favorite_view (clicker/__init__.py), taken over with its developer's
-- permission (NOTICE): the view's caption read as text — an open expansion shows an icon there —
-- and, where it reads no view, the browser's text compared with the preset database; FAV from its
-- brightest pixel and its bright ones. Three differences: the expansion an action expects is scored
-- the project's way, by its presets and its categories, and any other only by its presets, which
-- the database's index holds — every expansion's categories would mean reading every expansion's
-- file — and a text two expansions' presets are named in as much names neither, where the project
-- took the first of its roster; the grid is told by the catalog's expansions and Avenger's own list
-- read this session, not by the roster the page shows, which may hold none of the names a scrolled
-- grid shows; and a text that settles nothing is "not read", where the project kept what it had
-- last known. Since the second Mac test of the page (2026-10-24) the browser is first told by the
-- open expansion's name in its header (src/read/browser.luau, Br.state), and FAV by its star column.

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

-- Words in an open expansion's categories and presets, of which a text has to hold two to be an
-- open expansion's (the project's markers); an expansion's name has to be this long to count as
-- seen in the browser's grid.
local OPEN_MARKERS = {
  "arp", "bass", "bells", "chords", "drums", "effects", "instruments", "leads", "mallets",
  "strings", "guitar", "synth", "seq", "pad",
}
local NAME_MIN = 5
E.NAME_MIN = NAME_MIN
-- What an open expansion has to score (DB.scoreOpen, DB.openExpansion): one preset name, the
-- project's least.
local OPEN_SCORE = 6

function E.ask(pl, out)
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

-- The browser's area, for the read made when the caption reads no view.
function E.area(pl)
  return pl:rect(X.AREA)
end

-- What the browser's text `text` (its area, or an open expansion's two columns, read) says, with the
-- preset database `DB`, the expansion an action expects open (`expected`, or nil) and the names a
-- grid is told by (`names`: { names, folds } — the catalog's expansions and Avenger's own list):
-- { open = true, name = the expansion, score } for an open expansion the database recognises — the
-- one expected where it scores, else the one whose presets the text names most; { open = true } where
-- two expansions' presets it names as much — which of them is open the text does not say; { open =
-- false } for the grid of expansions — one of `names` named in it; { open = true } for an open
-- expansion the database does not recognise — two of the markers; or nil, nothing settled. And the
-- log's words.
function E.fromArea(text, DB, expected, names)
  local t = DB.fold(text)
  if t == "" then return nil, "nothing read" end
  if DB.present then
    if expected and expected ~= "" then
      local okS, score = pcall(DB.scoreOpen, t, expected)
      if okS and score >= OPEN_SCORE then
        return { open = true, name = expected, score = score }, string.format("%s, the one expected, by what it lists (%d)",
          expected, score)
      end
    end
    local ok, n, sc, other = pcall(DB.openExpansion, t)
    if not ok then return nil, "the preset database failed: " .. tostring(n) end
    if n and other then
      return { open = true }, string.format("two expansions list what was read: %s and %s, by their presets named (%d each)",
        n, other, sc)
    end
    if n then return { open = true, name = n, score = sc }, string.format("%s, by its presets named (%d)", n, sc) end
  end
  for k, f in ipairs(names and names.folds or {}) do
    if #f >= NAME_MIN and string.find(t, f, 1, true) then
      return { open = false }, string.format("the grid of expansions: '%s' named", names.names[k])
    end
  end
  local hits = {}
  for _, m in ipairs(OPEN_MARKERS) do
    if string.find(t, m, 1, true) then hits[#hits + 1] = m end
  end
  if #hits >= 2 then
    return { open = true }, "an open expansion the database does not name: " .. table.concat(hits, ", ")
  end
  return nil, "nothing it settles"
end

return E
