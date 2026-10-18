-- The expansion browser, read off the page's picture: its view (List, or 1 to 8 per Row) while it
-- shows the expansions; whether an expansion is open instead, and which; and whether the
-- favourites view (FAV) is on.
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
-- last known.

local P = host.include("src/read/picture.luau")
local X = host.include("src/tables/expansion.luau")

local E = {}

-- FAV is on when its brightest pixel's (r + g + b) / 3 is 190 or more, or 105 of the project's
-- pixels are brighter than 150.
E.PEAK = host.screen.predicate("r + g + b >= 570")
E.BRIGHT = host.screen.predicate("r + g + b >= 451")
local FAV_BRIGHT = 105

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
  local r = pl:box(X.FAVORITE_VIEW, X.FAVORITE_VIEW_STATE)
  if not r then return end
  out[#out + 1] = P.cells("fav peak", r, E.PEAK, pl.k)
  out[#out + 1] = P.cells("fav bright", r, E.BRIGHT, pl.k)
end

-- FAV into `v.favoriteView`.
function E.favorite(pl, got, v, log, marks)
  local r = pl:box(X.FAVORITE_VIEW, X.FAVORITE_VIEW_STATE)
  local peak, bright = got["fav peak"], got["fav bright"]
  local ps, _, any = nil, nil, false
  if peak and r then ps, _, any = peak:share(r) end
  local bs = bright and r and bright:share(r)
  local x, y = pl:point(X.FAVORITE_VIEW[1], X.FAVORITE_VIEW[2])
  marks[#marks + 1] = { x = x, y = y }
  if ps == nil or bs == nil then
    log("expansion: FAV not read")
    return
  end
  local n = pl:toolPixels(r)
  v.favoriteView = any or bs * n >= FAV_BRIGHT
  log(string.format("expansion: FAV %s of its pixels at 190 or brighter (%s), %s brighter than 150 (the project's %.0f of "
    .. "%.0f, on from %d) — %s", P.pct(ps), any and "some" or "none", P.pct(bs), bs * n, n, FAV_BRIGHT,
    v.favoriteView and "ON" or "OFF"))
end

-- The view's caption as an OCR entry.
function E.entries(pl, out)
  local r = pl:rect(X.VIEW_TEXT)
  if r then out[#out + 1] = { name = "expansion view", region = r } end
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
