-- The expansion browser, read off the page's picture: its view (List, or 1 to 8 per Row) while it
-- shows the expansions; whether an expansion is open instead, and which; and whether the
-- favourites view (FAV) is on.
--
-- SOURCE: the avenger_control project's detect_expansion_state and its helpers (its actions.py)
-- and its detect_expansion_favorite_view (clicker/__init__.py), taken over with its developer's
-- permission (NOTICE): the view's caption read as text — an open expansion shows an icon there —
-- and, where it reads no view, the browser's whole area read as text and compared with the preset
-- database; FAV from its brightest pixel and its bright ones. Two differences: an open expansion
-- is recognised by the presets of it the text names (the database's index), not also by its
-- categories, which would mean reading every expansion's file; and a text that settles nothing is
-- "not read", where the project kept what it had last known.

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

-- What the browser's area read as `text` says, with the preset database `DB`: { open = true,
-- name = the expansion } for an open expansion the database recognises; { open = false } for the
-- grid of expansions — an expansion of the roster `roster` named in it; { open = true } for an open
-- expansion it does not recognise — two of the markers; or nil, nothing settled. And the log's
-- words.
function E.fromArea(text, DB, roster)
  local t = string.lower((string.gsub(tostring(text or ""), "%s+", " ")))
  if t == "" or t == " " then return nil, "nothing read" end
  local name, score = nil, nil
  if DB.present then
    local ok, n, sc = pcall(DB.openExpansion, t)
    if ok then
      name, score = n, sc
    else
      return nil, "the preset database failed: " .. tostring(n)
    end
  end
  if name then return { open = true, name = name }, string.format("%s, by its presets named (%d)", name, score) end
  for _, e in ipairs(roster or {}) do
    local f = string.lower((string.gsub(e, "%s+", " ")))
    if #f >= NAME_MIN and string.find(t, f, 1, true) then
      return { open = false }, string.format("the grid of expansions: '%s' named", e)
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
