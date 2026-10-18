-- The expansion browser read off a picture of it, for the browser's actions (src/actions/browser.luau,
-- and through src/act.luau its `browser` part) and the page key (read/open.luau): what the grid of
-- expansions shows, word by word, where each name lies; an open expansion's categories and preset
-- rows, placed in the preset database's lists; and the rows' favourite stars.
--
-- ONE TEXT READ PER PICTURE, with named regions (Br.read): the view's caption ("expansion view"),
-- the area ("browser area") where the grid is expected, and the two columns ("categories",
-- "presets") where an open expansion is — two regions, never one: the host joins engine lines that
-- overlap vertically into one row (docs/api/ocr.md), so a category and a preset at the same height
-- would come back as one line.
--
-- THE GRID (Br.tiles). Each word goes to the column its middle lies in (tables/expansion.luau,
-- T.columns); in a column, the words of one of the host's rows make a fragment; two fragments one
-- under the other join when the gap between them is smaller than the upper one's height — a name
-- written on two lines — and are two tiles otherwise. Each tile is named among the catalog's
-- expansions and Avenger's own list read this session (Br.names) by the preset database's rules
-- (DB.nameIn): the same words, the same ignoring case, or — for NAME_MIN characters or more — the
-- closest by similarity that stands out from every other name. A joined text that is no name, whose
-- fragments each are one, is two tiles. A fragment that is no name is kept as read where it is
-- NAME_MIN characters or more, and left out otherwise; both are logged. A tile's point is the middle
-- of its words' boxes — what is clicked, never the project's point for an index — and a word whose
-- box lies within a point of the area's top or bottom edge is left out: half a tile is never
-- clicked. The join's rule and the columns are starting values: every gap is logged (TODO.md, D14).
--
-- THE ROWS (Br.lines). One column's rows, each named among an expansion's categories or presets
-- (DB.lists); a name the list holds twice is placed by the category the other rows read as, and
-- not placed where that does not tell. A row's point is the middle of its words.
--
-- THE STARS (Br.stars). Two grids of one column over the star column, four cells to a row of the
-- fourteen (at most 64 cells), with the project's two tests — bright, and its blue — and for each row
-- read, the cells under its star's 9 by 9 box at the row's middle: the project's rule, 18 of its
-- pixels bright or 18 blue, as shares (read/picture.luau, toolPixels), as FAV's own counts are.
--
-- Nothing here clicks or waits for a change: every read is of the picture it is handed, on the
-- image worker and the recogniser, and Luau works on the answers only — the words, a grid of at most
-- 64 cells — never on the pixels.

local O = host.require("com.platform.overlay")
local H = host.include("src/header.luau")
local DB = host.include("src/presets.luau")
local P = host.include("src/read/picture.luau")
local R = host.include("src/read/ocr.luau")
local X = host.include("src/tables/expansion.luau")
local E = host.include("src/read/expansion.luau")
local S = host.include("src/state.luau")

local Br = {}

-- The shortest text taken for a name by similarity, and kept as read when it is none.
Br.NAME_MIN = E.NAME_MIN

-- The star column's cells per preset row.
local STAR_CELLS = 4
-- The project's rule for a filled star: 18 of its pixels in the star's box bright or blue.
local STAR_PIXELS = 18
-- The project's blue: b above 120, and above r and g by more than 10 (its _screen_region_stats).
Br.BLUE = host.screen.predicate("b >= 121 and b - r >= 11 and b - g >= 11")

local function log(fmt, ...)
  host.log.info(("[avenger] [browser] " .. fmt):format(...))
end

-- ── Reading ──────────────────────────────────────────────────────────────────

-- The regions read as text, by the names an action asks for them with: the entry's name and its
-- table rectangle.
Br.REGIONS = {
  caption = { "expansion view", X.VIEW_TEXT },
  area = { "browser area", X.AREA },
  categories = { "categories", X.CATEGORY_TEXT },
  presets = { "presets", X.PRESET_TEXT },
}

-- Rectangle `r` cut to snapshot `snap`'s; nil when they hardly overlap.
local function within(r, snap)
  local c = { math.max(r[1], snap.x), math.max(r[2], snap.y), math.min(r[3], snap.x + snap.w),
    math.min(r[4], snap.y + snap.h) }
  if c[3] - c[1] < 2 or c[4] - c[2] < 2 then return nil end
  return c
end

-- The regions `which` (Br.REGIONS' names) read off snapshot `snap` with placer `pl`, in ONE text
-- read, waited for: the readings by their entries' names, and how long it took. A region the picture
-- holds nothing of is not asked; a read that fails answers nothing, and the log says why.
function Br.read(snap, pl, which)
  local entries = {}
  for _, w in ipairs(which) do
    local spec = Br.REGIONS[w]
    local r = spec and pl:rect(spec[2])
    r = r and within(r, snap)
    if r then entries[#entries + 1] = { name = spec[1], region = r } end
  end
  if #entries == 0 then return {}, 0 end
  local t0 = host.now()
  local ok, list, byName = pcall(host.ocr.recognize, entries, { snapshot = snap, lang = H.lang() })
  local ms = host.now() - t0
  if not ok then
    log("the text read of %s failed: %s", table.concat(which, ", "), tostring(list))
    return {}, ms
  end
  local said = {}
  for _, e in ipairs(entries) do
    local r = byName and byName[e.name]
    said[#said + 1] = string.format("%s %s", e.name, r and (r.status == "text" and ("'" .. R.forLog(r.text) .. "'")
      or (tostring(r.status) .. (r.error and (": " .. r.error) or ""))) or "not answered")
  end
  log("read in %d ms: %s", ms, table.concat(said, "; "))
  return byName or {}, ms
end

-- The text of reading `r`, tidied; nil where nothing was read.
function Br.text(r)
  if not (r and r.status == "text") then return nil end
  local t = R.tidy(r.text)
  return t ~= "" and t or nil
end

-- The host's rows of reading `r`, each { words = { { text, x, y, w, h } } } top to bottom: its
-- `lines`, or — a reading that has words and no lines — its words gathered into rows that overlap
-- vertically by half the smaller height, as the host gathers engine lines.
local function rowsOf(r)
  if not (r and r.status == "text") then return {} end
  if r.lines and #r.lines > 0 then return r.lines end
  local out = {}
  for _, w in ipairs(r.words or {}) do
    local into = nil
    for _, row in ipairs(out) do
      local top, bottom = math.max(row.y, w.y), math.min(row.y + row.h, w.y + w.h)
      if bottom - top >= math.min(row.h, w.h) / 2 then into = row break end
    end
    if into then
      into.words[#into.words + 1] = w
    else
      out[#out + 1] = { y = w.y, h = w.h, words = { w } }
    end
  end
  table.sort(out, function(a, b) return a.y < b.y end)
  return out
end

-- ── The names ────────────────────────────────────────────────────────────────

-- What a grid's tiles are named among: the catalog's expansions, whatever the roster, and Avenger's
-- own list read this session — { names, folds, catalog = { [i] = true for a catalog name },
-- own = { [fold] = true for a name of Avenger's list } }, built again only when that list changes.
local namesFor, namesOf = nil, nil
function Br.names()
  local own = S.avengerList
  if namesOf and namesFor == own then return namesOf end
  local names, catalog, mine = {}, {}, {}
  local seen = {}
  local cn, cf = DB.expansionNames()
  for i, n in ipairs(cn or {}) do
    names[#names + 1] = n
    catalog[#names] = true
    seen[cf[i]] = true
  end
  for _, n in ipairs(own and own.names or {}) do
    local f = DB.fold(n)
    mine[f] = true
    if not seen[f] then
      seen[f] = true
      names[#names + 1] = n
    end
  end
  local list = DB.nameList(names)
  list.catalog, list.own, list.cache = catalog, mine, {}
  namesFor, namesOf = own, list
  return namesOf
end

-- Text `t` among `list` (DB.nameList's, with a `cache`), each text looked up once per list: the grid
-- read again after a step reads the same names again.
local function lookUp(list, t, min)
  local kept = list.cache[t]
  if kept then return kept.hit, kept.why end
  local hit, why = DB.nameIn(t, list, min)
  list.cache[t] = { hit = hit, why = why }
  return hit, why
end

-- ── The state ────────────────────────────────────────────────────────────────

-- What the browser shows, from readings `byName` (Br.read's) and the expansion an action expects
-- open (`expected`, or nil): the port of the project's detect_expansion_state, in its order —
--   1. the caption reads a view: the grid, no expansion open — { view, open = false };
--   2. the text (the area's, or the two columns' joined) names an expansion by what it lists — the
--      expected one first, scored by its presets and categories (DB.scoreOpen), then any by its
--      presets (DB.openExpansion): { open = true, name, score } — where two list it as much, which
--      one is open is not read: { open = true };
--   3. one of the catalog's expansions or of Avenger's own list is named in it: the grid —
--      { open = false };
--   4. two of the project's markers: an expansion open, which one not read — { open = true };
--   5. nil: not read — where the project kept what it had last known.
-- And the log's words.
function Br.state(byName, expected)
  local caption = Br.text(byName["expansion view"])
  local view = E.parseView(caption)
  if view then
    return { view = view, open = false }, string.format("the view reads '%s' — %s, no expansion open", caption, view)
  end
  local area = Br.text(byName["browser area"])
  local text = area
  if not text then
    local c, p = Br.text(byName["categories"]), Br.text(byName["presets"])
    if c or p then text = (c or "") .. " " .. (p or "") end
  end
  if not text then
    return nil, string.format("the view reads %s, and nothing else was read", caption and ("'" .. caption .. "'") or "nothing")
  end
  local seen, why = E.fromArea(text, DB, expected, Br.names())
  return seen, string.format("the view reads %s — %s", caption and ("'" .. caption .. "'") or "nothing", tostring(why))
end

-- ── The grid ─────────────────────────────────────────────────────────────────

local function union(a, b)
  return { math.min(a[1], b[1]), math.min(a[2], b[2]), math.max(a[3], b[3]), math.max(a[4], b[4]) }
end

-- The tiles of the grid read in reading `r` (the area's), in view `view`, with placer `pl`: each {
-- name = the name (the list's spelling, or as read), read = the words read, how = "exact" |
-- "folded" | "fuzzy" | "as read", ratio, key = its folded name, catalog = true for a catalog name,
-- x, y = the middle of its words on screen, box, column } — in reading order, rows top to bottom and
-- each row's columns left to right. Empty, with why, where nothing can be told.
function Br.tiles(r, view, pl)
  local cols = X.columns(view)
  if not cols then return {}, "no view to place the words by" end
  if not (r and r.status == "text") then return {}, "nothing read" end
  local k = pl.k
  local area = pl:rect(X.AREA)
  if not area then return {}, "no area" end
  local mids = {}
  for c, x in ipairs(cols.xs) do mids[c] = (pl:exact(x, 0)) end
  local half = cols.half * k
  local names = Br.names()
  local frags, cut, outside = {}, 0, 0
  for _, row in ipairs(rowsOf(r)) do
    local inRow = {}
    for _, w in ipairs(row.words or {}) do
      if w.y < area[2] + k or w.y + w.h > area[4] - k then
        cut += 1
      else
        local mx = w.x + w.w / 2
        local best, bd = nil, nil
        for c, m in ipairs(mids) do
          local d = math.abs(mx - m)
          if bd == nil or d < bd then best, bd = c, d end
        end
        if bd and bd <= half then
          local f = inRow[best]
          if not f then
            f = { words = {}, box = { w.x, w.y, w.x + w.w, w.y + w.h } }
            inRow[best] = f
            frags[best] = frags[best] or {}
            table.insert(frags[best], f)
          else
            f.box = union(f.box, { w.x, w.y, w.x + w.w, w.y + w.h })
          end
          f.words[#f.words + 1] = w.text
        else
          outside += 1
        end
      end
    end
  end
  local tiles, words = {}, {}
  local function add(name, read, box, c, hit)
    local t = { name = name, read = read, how = hit and hit.how or "as read", ratio = hit and hit.ratio or nil,
      key = DB.fold(name), catalog = hit ~= nil and names.catalog[hit.index] == true, box = box, column = c,
      x = math.floor((box[1] + box[3]) / 2 + 0.5), y = math.floor((box[2] + box[4]) / 2 + 0.5) }
    tiles[#tiles + 1] = t
    words[#words + 1] = string.format("col %d '%s'%s at (%d,%d)", c, R.forLog(read),
      hit and (hit.name ~= read and string.format(" -> '%s' (%s%s)", hit.name, hit.how,
        hit.how == "fuzzy" and string.format(" %.2f", hit.ratio) or "") or string.format(" (%s)", hit.how)) or " (no name, kept as read)",
      t.x, t.y)
  end
  for c = 1, #cols.xs do
    local list = frags[c] or {}
    local i = 1
    while i <= #list do
      local f, g = list[i], list[i + 1]
      local took = false
      local ft = table.concat(f.words, " ")
      if g then
        local gap, h = g.box[2] - f.box[4], f.box[4] - f.box[2]
        if gap < h then
          local joined = ft .. " " .. table.concat(g.words, " ")
          local hit = lookUp(names, joined, Br.NAME_MIN)
          words[#words + 1] = string.format("col %d '%s' over '%s', gap %.1f of %.1f — %s", c, R.forLog(ft),
            R.forLog(table.concat(g.words, " ")), gap, h, hit and "one name" or "no name joined")
          if hit then
            add(hit.name, joined, union(f.box, g.box), c, hit)
            i += 2
            took = true
          end
        end
      end
      if not took then
        local hit, why = lookUp(names, ft, Br.NAME_MIN)
        if hit then
          add(hit.name, ft, f.box, c, hit)
        elseif #ft >= Br.NAME_MIN then
          add(ft, ft, f.box, c, nil)
        else
          words[#words + 1] = string.format("col %d '%s' left out (%s)", c, R.forLog(ft), tostring(why))
        end
        i += 1
      end
    end
  end
  -- Reading order: the tiles by their middles' height, a new row wherever one lies more than half a
  -- `pitch` below the first of the row it would join — rows told from each other by the tiles, not
  -- by bins from the area's top, which a grid scrolled to any offset puts its rows across, where a
  -- word box a point taller moves a tile into the next row — then each row by column, and in one
  -- column by height.
  local apart = cols.pitch * k / 2
  table.sort(tiles, function(a, b)
    if a.y ~= b.y then return a.y < b.y end
    if a.column ~= b.column then return a.column < b.column end
    return a.x < b.x
  end)
  local byRow, top = {}, nil
  for _, t in ipairs(tiles) do
    if top == nil or t.y - top > apart then
      byRow[#byRow + 1] = {}
      top = t.y
    end
    t.row = #byRow
    table.insert(byRow[#byRow], t)
  end
  tiles = {}
  for _, row in ipairs(byRow) do
    table.sort(row, function(a, b)
      if a.column ~= b.column then return a.column < b.column end
      if a.y ~= b.y then return a.y < b.y end
      return a.x < b.x
    end)
    for _, t in ipairs(row) do tiles[#tiles + 1] = t end
  end
  if cut > 0 then words[#words + 1] = string.format("%d word%s at the area's edge left out", cut, cut == 1 and "" or "s") end
  if outside > 0 then words[#words + 1] = string.format("%d word%s between the columns left out", outside, outside == 1 and "" or "s") end
  log("the grid in %s: %d tile%s — %s", view, #tiles, #tiles == 1 and "" or "s", table.concat(words, "; "))
  return tiles
end

-- How many of `tiles` were named (not kept as read).
function Br.named(tiles)
  local n = 0
  for _, t in ipairs(tiles) do
    if t.how ~= "as read" then n += 1 end
  end
  return n
end

-- ── The rows ─────────────────────────────────────────────────────────────────

-- One column's rows read in reading `r`: { text, box } top to bottom, the words of each of the
-- host's rows joined.
function Br.rows(r)
  local out = {}
  for _, row in ipairs(rowsOf(r)) do
    local words, box = {}, nil
    for _, w in ipairs(row.words or {}) do
      words[#words + 1] = w.text
      local b = { w.x, w.y, w.x + w.w, w.y + w.h }
      box = box and union(box, b) or b
    end
    if box then out[#out + 1] = { text = table.concat(words, " "), box = box } end
  end
  return out
end

-- Reading `r` of the whole area with only the words whose middle lies across screen rectangle
-- `rect`'s x — one column of it — as a reading of its own; nil where none is left. The page key
-- reads the area alone, and its preset rows are the presets' column of it.
function Br.column(r, rect)
  if not (r and rect and r.status == "text") then return nil end
  local lines = {}
  for _, row in ipairs(rowsOf(r)) do
    local words = {}
    for _, w in ipairs(row.words or {}) do
      local mx = w.x + w.w / 2
      if mx >= rect[1] and mx < rect[3] then words[#words + 1] = w end
    end
    if #words > 0 then lines[#lines + 1] = { words = words } end
  end
  if #lines == 0 then return nil end
  return { status = "text", lines = lines }
end

-- `names` prepared for the lookup (DB.nameList), once per list.
local prepared = setmetatable({}, { __mode = "k" })
local function listOf(names)
  local l = prepared[names]
  if l then return l end
  l = DB.nameList(names)
  l.cache = {}
  prepared[names] = l
  return l
end

-- Rows `rows` (Br.rows') placed in `names` — an open expansion's categories or presets
-- (DB.lists) — each { name, read, index (in `names`, from 1), how, ratio, x, y, box }, or { read,
-- why } for a row that is no name of it. With `categories` (the presets' categories, DB.lists'
-- presetCategories), a name the list holds twice is placed in the category most of the other rows
-- read as; without, or where that does not tell, it is not placed. `what` names the list in the log.
function Br.lines(rows, names, categories, what)
  local list = listOf(names)
  local out, words = {}, {}
  local votes, twice = {}, {}
  for _, row in ipairs(rows) do
    local hit, why = lookUp(list, row.text, 4)
    local e = { read = row.text, box = row.box, x = math.floor((row.box[1] + row.box[3]) / 2 + 0.5),
      y = math.floor((row.box[2] + row.box[4]) / 2 + 0.5) }
    if hit then
      e.name, e.how, e.ratio = hit.name, hit.how, hit.ratio
      if #hit.indices == 1 then
        e.index = hit.index
        if categories then
          local c = categories[e.index]
          votes[c] = (votes[c] or 0) + 1
        end
      else
        e.indices = hit.indices
        twice[#twice + 1] = e
      end
    else
      e.why = why
    end
    out[#out + 1] = e
  end
  local most, mostN = nil, 0
  for c, n in pairs(votes) do
    if n > mostN or (n == mostN and most ~= nil and c < most) then most, mostN = c, n end
  end
  for _, e in ipairs(twice) do
    local pick = nil
    if most and categories then
      for _, i in ipairs(e.indices) do
        if categories[i] == most then
          if pick == nil then pick = i else pick = false end
        end
      end
    end
    if pick then e.index = pick else e.why = string.format("in the list %d times, not told apart", #e.indices) end
  end
  for _, e in ipairs(out) do
    if e.index then
      words[#words + 1] = string.format("'%s'%s %d at y %d", R.forLog(e.read),
        e.name ~= e.read and string.format(" -> '%s' (%s%s)", e.name, e.how, e.how == "fuzzy" and string.format(" %.2f", e.ratio) or "")
          or "", e.index, e.y)
    else
      words[#words + 1] = string.format("'%s' no name (%s)", R.forLog(e.read), tostring(e.why))
    end
  end
  log("the %s: %d row%s — %s", what or "rows", #out, #out == 1 and "" or "s", table.concat(words, "; "))
  return out
end

-- The entries of `lines` that were placed.
function Br.placed(lines)
  local out = {}
  for _, e in ipairs(lines) do
    if e.index then out[#out + 1] = e end
  end
  return out
end

-- The category most of the placed rows `lines` belong to (`categories`, DB.lists'
-- presetCategories), and how many of them do; nil for none.
function Br.categoryOf(lines, categories)
  local votes = {}
  for _, e in ipairs(lines) do
    local c = e.index and categories[e.index]
    if c and c ~= "" then votes[c] = (votes[c] or 0) + 1 end
  end
  local most, n = nil, 0
  for c, m in pairs(votes) do
    if m > n or (m == n and most ~= nil and c < most) then most, n = c, m end
  end
  return most, n
end

-- ── FAV and the stars ────────────────────────────────────────────────────────

-- Grid `req` (P.cells') asked of snapshot `snap`, waited for: the grid, or nil and why.
local function cellsOf(snap, req)
  local r = req and within(req.rect, snap)
  if not r then return nil, "outside the picture" end
  req.rect = r
  local ok, value, why = pcall(host.screen.cells, { region = r, cols = req.cols, rows = req.rows,
    predicate = req.pred, snapshot = snap })
  if not ok then return nil, tostring(value) end
  if value == nil then return nil, why end
  return P.grid(req, value)
end

-- FAV, read off snapshot `snap` as the page key reads it (read/expansion.luau, E.favorite): true,
-- false, or nil for not read. `marks` gets its point, for a calibration picture.
function Br.fav(snap, pl, marks)
  local asks = {}
  E.ask(pl, asks)
  local got = { why = {} }
  for _, req in ipairs(asks) do
    local g, why = cellsOf(snap, req)
    if g then got[req.name] = g else got.why[req.name] = why end
  end
  local v = {}
  E.favorite(pl, got, v, function(s) log("%s", s) end, marks or {})
  return v.favoriteView
end

-- The stars of rows `lines` (Br.lines'), read off snapshot `snap`: each placed row's `star` true or
-- false, nil where its box is not in the picture. Two grids of one column, waited for. `marks` gets
-- each star's point.
function Br.stars(snap, pl, lines, marks)
  local col = pl:rect(X.STAR_COLUMN)
  local rows = X.PRESET_VISIBLE_ROWS * STAR_CELLS
  local bright = cellsOf(snap, col and { rect = col, cols = 1, rows = rows, pred = E.BRIGHT })
  local blue = cellsOf(snap, col and { rect = col, cols = 1, rows = rows, pred = Br.BLUE })
  local sx = pl:exact(X.PRESET_FAVORITE_X, 0)
  local hw, hh = X.FAVORITE_STATE[1] / 2 * pl.k, X.FAVORITE_STATE[2] / 2 * pl.k
  local words = {}
  for _, e in ipairs(lines) do
    if e.index then
      local box = { math.floor(sx - hw + 0.5), math.floor(e.y - hh + 0.5), math.floor(sx + hw + 0.5),
        math.floor(e.y + hh + 0.5) }
      if marks then marks[#marks + 1] = { x = math.floor(sx + 0.5), y = e.y } end
      local bs = bright and bright:share(box)
      local ls = blue and blue:share(box)
      if bs == nil or ls == nil then
        words[#words + 1] = string.format("%d not read", e.index)
      else
        local n = pl:toolPixels(box)
        e.star = bs * n >= STAR_PIXELS or ls * n >= STAR_PIXELS
        words[#words + 1] = string.format("%d %s bright, %s blue (the project's %.0f and %.0f of %.0f) — %s", e.index,
          P.pct(bs), P.pct(ls), bs * n, ls * n, n, e.star and "a favorite" or "not")
      end
    end
  end
  log("the stars: %s", #words > 0 and table.concat(words, "; ") or "no row placed")
end

-- The stars of `lines` as favoriteStates' entries, merged over `into` (or a new table): the
-- preset's place from 0 -> the star read.
function Br.starStates(lines, into)
  local out = into and table.clone(into) or {}
  for _, e in ipairs(lines) do
    if e.index and e.star ~= nil then out[e.index - 1] = e.star end
  end
  return out
end

-- ── The `browser` part (src/act.luau) ────────────────────────────────────────
-- The browser read off picture `snap` for an action of Act.run's — the view chosen, Close, FAV —
-- with `opts` = { pl, expected = the expansion known open, or nil, fields = { "area" | "presets" } }:
-- FAV's pixels, the caption as text, and — where the caption reads no view — with "area" the area
-- read for an open expansion (Br.state), with "presets" the preset rows and their stars where FAV
-- reads on. The values (state.luau's) and the keys read: a caption that reads a view says the grid is
-- shown and every open expansion's value is gone with it; one that reads none says only what was read.
local OPEN_KEYS = { "lists", "loadedIndex", "favoriteView", "favoriteStates", "categoryShown", "pageCategory",
  "selectedFromDatabase" }

function Br.part(snap, pl, opts)
  local fields = {}
  for _, f in ipairs(opts.fields or {}) do fields[f] = true end
  local v, keys = {}, { "expansionView" }
  local fav = Br.fav(snap, pl)
  local which = { "caption" }
  if fields.presets and fav and opts.expected then which[#which + 1] = "presets" end
  local byName = Br.read(snap, pl, which)
  local caption = Br.text(byName["expansion view"])
  v.expansionView = E.parseView(caption)
  if v.expansionView then
    v.expansionOpen, v.selectedExpansion = false, ""
    for _, k in ipairs({ "expansionOpen", "selectedExpansion" }) do keys[#keys + 1] = k end
    for _, k in ipairs(OPEN_KEYS) do keys[#keys + 1] = k end
    log("the view reads '%s' — %s, no expansion open", tostring(caption), v.expansionView)
    return v, keys
  end
  v.favoriteView = fav
  keys[#keys + 1] = "favoriteView"
  if fields.area then
    local area = Br.read(snap, pl, { "area" })
    local st, why = Br.state({ ["expansion view"] = byName["expansion view"], ["browser area"] = area["browser area"] },
      opts.expected)
    log("%s", why)
    if st then
      v.expansionOpen = st.open
      v.selectedExpansion = st.open and (st.name or "") or ""
      keys[#keys + 1] = "expansionOpen"
      keys[#keys + 1] = "selectedExpansion"
      -- The grid told by its names with the caption unread: nothing of an open expansion is left.
      if not st.open then
        for _, k in ipairs(OPEN_KEYS) do
          if k ~= "favoriteView" then keys[#keys + 1] = k end
        end
        v.favoriteView = nil
      end
    end
  end
  if fav == false then
    keys[#keys + 1] = "favoriteStates"
  elseif fav and byName["presets"] then
    local okL, lists = pcall(DB.lists, opts.expected)
    if okL and lists then
      local lines = Br.lines(Br.rows(byName["presets"]), lists.presets, lists.presetCategories, "preset rows")
      Br.stars(snap, pl, lines)
      v.favoriteStates = Br.starStates(lines)
      keys[#keys + 1] = "favoriteStates"
    end
  end
  return v, keys
end

-- ── Calibration ──────────────────────────────────────────────────────────────

-- The pictures written this session, by name.
local shots = {}

-- In a calibrating run, picture `name` (".png" added) of snapshot `snap` over screen rectangle
-- `rect`, with crosshairs at `marks`, once a session — written with a callback, on the image worker.
function Br.picture(name, snap, rect, marks)
  if not O.calibrating() or shots[name] or not rect then return end
  shots[name] = true
  local path = host.path("calibration/" .. name .. ".png")
  local ok, err = pcall(host.screen.saveMarked, path, { snapshot = snap, region = rect, marks = marks or {} },
    function(saved, why)
      host.log.info(string.format("[avenger] [calibrate] %s picture, (%d,%d)-(%d,%d): %d mark%s -> %s (%s)", name,
        rect[1], rect[2], rect[3], rect[4], #(marks or {}), #(marks or {}) == 1 and "" or "s", path,
        saved and "true" or ("false: " .. tostring(why))))
    end)
  if not ok then host.log.info(string.format("[avenger] [calibrate] %s picture not asked for: %s", name, tostring(err))) end
end

return Br
