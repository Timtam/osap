-- The expansion browser read off a picture of it, for the browser's actions (src/actions/browser.luau,
-- and through src/act.luau its `browser` part) and the page key (read/open.luau): whether it shows the
-- grid of expansions or an open expansion, and which; FAV; the favourite stars of the preset rows
-- this session's own clicks placed; the view list a Close may open, by its pixels (Br.listSeen);
-- and the gate's line every browser action and the page key log (Br.gateLine).
--
-- ONLY THE HEADER IS READ AS TEXT (the maintainer's decision of 2026-10-25, after the third Mac
-- test): the view's caption ("expansion view") and the open expansion's name ("expansion name"), in
-- one text read. Below it nothing is: the grid's names are about 5 pixels tall at 80 % on a 1x
-- display, the categories and the preset rows hardly more, and the avenger_control developer found
-- no recogniser that reads them — which is why the tool keeps the expansions' database. The lists
-- are the preset database's (modules/vps-avenger-presets), the places the tool's tables
-- (src/tables/pages.luau), as the tool goes through them; the header's names confirm a click.
--
-- THE STATE (Br.state). The caption, where Avenger draws a view's label there; the header's name,
-- looked up among the catalog's expansions and the roster's; a picture test of the header's title
-- band and of the categories' scrollbar, pixels only (read/expansion.luau, E.look); and the user's
-- word, Confirm Expansion View, kept for this window, view and zoom (state.luau, S.confirmGrid) —
-- the tool's action_confirm_expansion_view, which sets its AVGExpansionOpen to 0. Something drawn in
-- the header that no text is read in (`none`) is never the grid by itself, nor is a header with
-- nothing drawn (`blank`): the picture test or the user's word has to say so too. The user's word
-- settles only such a header, never one a text is read in.
--
-- THE VIEW. Where Avenger draws the view's label it is read; the second Mac test's Avenger draws an
-- icon (tables/expansion.luau), and the view is then the one set from the page or confirmed there —
-- the `expansionView` setting, as the project kept its AVGExpansionView (Br.view) — and is said as
-- that, never as read.
--
-- THE STARS (Br.stars). Two grids of one column over the star column, four cells to a row of the
-- fourteen (at most 64 cells), with the project's two tests — bright, and its blue — and for each row
-- placed, the cells under its star's 9 by 9 box at the row's middle: the project's rule, 18 of its
-- pixels bright or 18 blue, as shares (read/picture.luau, toolPixels), as FAV's own counts are. A
-- row is placed where this session's own clicks put its category's list (Br.shownRows).
--
-- Nothing here clicks or waits for a change: every read is of the picture it is handed, on the
-- image worker and the recogniser, and Luau works on the answers only — the words of the header,
-- grids of a few thousand cells — never on the pixels.

local O = host.require("com.platform.overlay")
local H = host.include("src/header.luau")
local DB = host.include("src/presets.luau")
local P = host.include("src/read/picture.luau")
local R = host.include("src/read/ocr.luau")
local X = host.include("src/tables/expansion.luau")
local E = host.include("src/read/expansion.luau")
local S = host.include("src/state.luau")
local Pg = host.include("src/tables/pages.luau")
local Pp = host.include("src/popups.luau")

local Br = {}

-- The shortest text taken for a name by similarity.
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

-- The header's regions read as text, by the names an action asks for them with: the entry's name
-- and its table rectangle.
Br.REGIONS = {
  caption = { "expansion view", X.VIEW_TEXT },
  name = { "expansion name", X.NAME_TEXT },
}

-- Rectangle `r` cut to snapshot `snap`'s; nil when they hardly overlap.
local function within(r, snap)
  local c = { math.max(r[1], snap.x), math.max(r[2], snap.y), math.min(r[3], snap.x + snap.w),
    math.min(r[4], snap.y + snap.h) }
  if c[3] - c[1] < 2 or c[4] - c[2] < 2 then return nil end
  return c
end

-- The header's regions read off snapshot `snap` with placer `pl`, in ONE text read, waited for: the
-- readings by their entries' names, and how long it took. A region the picture holds nothing of is
-- not asked; a read that fails answers nothing, and the log says why.
function Br.read(snap, pl)
  local entries = {}
  for _, w in ipairs({ "caption", "name" }) do
    local spec = Br.REGIONS[w]
    local r = pl:rect(spec[2])
    r = r and within(r, snap)
    if r then entries[#entries + 1] = { name = spec[1], region = r } end
  end
  if #entries == 0 then return {}, 0 end
  local t0 = host.now()
  local ok, list, byName = pcall(host.ocr.recognize, entries, { snapshot = snap, lang = H.lang() })
  local ms = host.now() - t0
  if not ok then
    log("the text read of the header failed: %s", tostring(list))
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

-- The browser's grids `names` (E.ask's) asked of snapshot `snap`, waited for: { [name] = grid,
-- why = { [name] = why } }.
local function gridsOf(snap, pl, names)
  local asks = {}
  E.ask(pl, asks)
  local got = { why = {} }
  for _, req in ipairs(asks) do
    if names[req.name] then
      local g, why = cellsOf(snap, req)
      if g then got[req.name] = g else got.why[req.name] = why end
    end
  end
  return got
end

-- The picture test of snapshot `snap` (read/expansion.luau, E.look): its title band and its list
-- bar, waited for.
function Br.look(snap, pl)
  return E.look(pl, gridsOf(snap, pl, { ["title band"] = true, ["list bar"] = true }))
end

-- ── The names ────────────────────────────────────────────────────────────────

-- What the header's name is looked up among: the catalog's expansions and the roster's — an
-- expansion a roster lists that the database has no data for included — built again only when
-- the roster changes.
local namesFor, namesOf = nil, nil
function Br.names()
  local roster = host.settings.get("roster")
  if namesOf and namesFor == roster then return namesOf end
  local names, seen = {}, {}
  local cn = DB.expansionNames()
  for _, n in ipairs(cn or {}) do
    local f = DB.fold(n)
    if not seen[f] then
      seen[f] = true
      names[#names + 1] = n
    end
  end
  for _, n in ipairs(DB.roster(roster) or {}) do
    local f = DB.fold(n)
    if not seen[f] then
      seen[f] = true
      names[#names + 1] = n
    end
  end
  local list = DB.nameList(names)
  list.cache = {}
  namesFor, namesOf = roster, list
  return namesOf
end

-- Text `t` among `list` (DB.nameList's, with a `cache`), each text looked up once per list.
local function lookUp(list, t, min)
  local kept = list.cache[t]
  if kept then return kept.hit, kept.why end
  local hit, why = DB.nameIn(t, list, min)
  list.cache[t] = { hit = hit, why = why }
  return hit, why
end

-- ── The view ─────────────────────────────────────────────────────────────────

-- How the view stored was last stored this session: { view, how = "set" (Set Expansion View chose
-- it in Avenger) or "confirmed" (Confirm Expansion View) }; nil — "stored" — for a view the setting
-- kept from before, or one changed since by anything else (Br.settingChanged).
Br.viewHow = nil

-- The view Avenger shows where its caption is not read: the setting's, and how it came to be.
function Br.view()
  local stored, h = host.settings.get("expansionView"), Br.viewHow
  return stored, (h and h.view == stored) and h.how or "stored"
end

-- The view stored: `view` (one of X.VIEW_OPTIONS), `how` "set" or "confirmed".
function Br.storeView(view, how)
  Br.viewHow = { view = view, how = how }
  if host.settings.get("expansionView") ~= view then host.settings.set("expansionView", view) end
  log("the view stored: %s, %s", view, how)
end

-- The setting changed to `new` (page.luau, its onChange): to anything but the view Br.storeView
-- stored — in the settings dialog, by Reset Saved Settings — and the view is the setting's from
-- then on, "stored", never "set" or "confirmed".
function Br.settingChanged(new)
  local h = Br.viewHow
  if h and h.view ~= new then Br.viewHow = nil end
end

-- ── The state ────────────────────────────────────────────────────────────────

-- What the browser shows, from the header's readings `byName` (Br.read's), the picture test `look`
-- (E.look's, or nil) and `scope` = { window = the plug-in window's id, zoom = the zoom read,
-- expected = the expansion known open there, or nil } — in this order:
--   1. the caption reads a view (an Avenger that draws its label): the grid — { view, open = false };
--   2. the header's name is one of the catalog's or the roster's (the same words, the same ignoring
--      case, or the closest by similarity that stands out): that expansion open — { open = true,
--      name };
--   3. the title band is lit: an expansion open, its name not read — { open = true }, or, where it
--      is the one this session's own click opened in this window and nothing read since names
--      another (state.luau, S.browserAt), that one, by its place in the database's list — { open =
--      true, name, byPlace = true };
--   4. nothing drawn in the header (`blank`) and no title in its band: the grid — { view = the one
--      stored, viewFrom, open = false };
--   5. nothing read in the header (`blank` or `none`), no title, and the categories' scrollbar dark:
--      the grid, by the picture;
--   6. a text in the header that is no expansion's name, where the picture does not say the grid:
--      an expansion open, its name not read (as in 3);
--   7. nothing read in the header (`blank` or `none`) and the grid confirmed by the user in this
--      window for this view and zoom (S.gridConfirmed): the grid. The user's word settles only a
--      header nothing was read in — never one a text was read in, which is no contradiction only
--      where nothing is (the third Mac test's report, §6 A);
--   8. nil: not read. `none` alone — something drawn there and no text recognised, a name the
--      recogniser missed — never says the grid, and Set Expansion View's click there would close
--      an expansion.
-- Steps 2, 3 and 6 forget a confirmation of the grid in the window. Each answer carries `from`
-- ("caption", "header", "picture", "confirmed", "header and picture"). And the log's words.
function Br.state(byName, look, scope)
  scope = scope or {}
  local caption = Br.text(byName["expansion view"])
  local view = E.parseView(caption)
  if view then
    return { view = view, open = false, from = "caption" }, string.format("the view reads '%s' — %s, no expansion open",
      caption, view)
  end
  local header = byName["expansion name"]
  local status = header and header.status or nil
  local name = Br.text(header)
  local verdict = look and look.verdict or nil
  local pic = look and look.words or "the picture not read"
  if name then
    local hit = lookUp(Br.names(), name, Br.NAME_MIN)
    if hit then
      S.forgetGrid(scope.window, "the header reads " .. hit.name)
      return { open = true, name = hit.name, from = "header" }, string.format("the header reads '%s' — %s open (%s)",
        R.forLog(name), hit.name, hit.how)
    end
  end
  local read = name and ("'" .. R.forLog(name) .. "', no expansion's name") or ("no name (" .. tostring(status) .. ")")
  local function openUnnamed(from)
    local why = string.format("the header reads %s; %s: an expansion open", read, pic)
    local at = S.browserAt(scope.window)
    if at and at.expansion and at.expansion == scope.expected then
      return { open = true, name = at.expansion, byPlace = true, from = from }, why .. " — " .. at.expansion
        .. ", the one opened here by its place in the list chosen"
    end
    return { open = true, from = from }, why .. " — which one cannot be read"
  end
  if verdict == "open" then
    S.forgetGrid(scope.window, "the title band is lit")
    return openUnnamed("picture")
  end
  local stored, how = Br.view()
  local closed = string.format("no expansion open, the view %s %s", tostring(stored), tostring(how))
  local nothing = not name and (status == "blank" or status == "none")
  if nothing and status == "blank" and (verdict == "grid" or verdict == "dark") then
    return { view = stored, viewFrom = how, open = false, from = "header and picture" }, string.format("the header reads "
      .. "%s; %s: %s", read, pic, closed)
  end
  if nothing and verdict == "grid" then
    return { view = stored, viewFrom = how, open = false, from = "picture" }, string.format("the header reads %s; %s: %s",
      read, pic, closed)
  end
  if name and verdict ~= "grid" then
    S.forgetGrid(scope.window, "the header reads a text, '" .. R.forLog(name) .. "'")
    return openUnnamed("header")
  end
  local confirmed = nothing and S.gridConfirmed(scope.window, stored, scope.zoom)
  if confirmed then
    return { view = stored, viewFrom = how, open = false, from = "confirmed" }, string.format("the header reads %s; %s; "
      .. "the grid %s here for %s at %s %%: %s", read, pic, confirmed, tostring(stored), tostring(scope.zoom), closed)
  end
  return nil, string.format("the header reads %s; %s — not taken as closed", read, pic)
end

-- The header's name region for the gate's line: its reading's status, its size and where it is.
function Br.cropWords(byName, pl)
  local r = pl:rect(X.NAME_TEXT)
  local reading = byName["expansion name"]
  local status = reading and (reading.status == "text" and ("text '" .. R.forLog(reading.text) .. "'") or reading.status)
    or "not read"
  if not r then return status .. ", not on the screen" end
  return string.format("%s %dx%d at %d,%d", status, r[3] - r[1], r[4] - r[2], r[1], r[2])
end

-- The state's words for the gate's line.
local function stateWords(st)
  if not st then return "not read" end
  if st.open == false then return "the grid" end
  if st.name then return "open '" .. st.name .. "'" .. (st.byPlace and ", by its place" or "") end
  return "open, which one not read"
end

-- What the last read of the `browser` part said (Br.part), for the gate's line of the action of
-- src/act.luau's that decides by it (src/actions/browser.luau, `gated`): { window, pl, byName,
-- look, st, words }. Taken once, by that line.
Br.lastPart = nil

-- The gate's line: what the browser's actions decide by — the window and where Avenger is, the
-- zoom, the view and how it came to be, the header's name region, the picture test, the grid's
-- confirmation, the state and what said it — and `decision`, the action's (or the page key's: its
-- read clicks nothing). `label` is the action's words; `g` = { window, pl, byName, look, st, words,
-- zoom? (the zoom read, where the placer has none) }. Once an action or a page key.
function Br.gateLine(label, g, decision)
  local pl, st = g.pl, g.st
  local ox, oy = pl:point(0, 0)
  local view, how = Br.view()
  local c = S.gridConfirmation(g.window)
  log("gate '%s': window %s at %d,%d, zoom %s %% (factor %.2f); view %s (%s); name crop %s; picture: %s; grid "
    .. "confirmed %s; state %s from %s (%s); %s", label, tostring(g.window), ox, oy, tostring(g.zoom or pl.zoom), pl.k,
    st and st.view or view, (st and st.from == "caption") and "read" or (how == "stored" and "as stored" or how),
    Br.cropWords(g.byName, pl), g.look and g.look.words or "not read",
    c and string.format("for %s at %s %%", tostring(c.view), tostring(c.zoom)) or "no", stateWords(st),
    st and st.from or "nothing", tostring(g.words), decision)
end

-- ── The view list after Close, by its pixels ─────────────────────────────────
-- Where nothing told whether an expansion was open, Close is clicked as the tool clicks it, and
-- with the grid shown that click opens the view list instead (src/actions/browser.luau,
-- B.listAfterClose). Nothing there is read as text — below the header nothing is (above): the list
-- is told by its PIXELS, as the overlay runtime's drawn test tells a popup painted inside a plug-in
-- (docs/api/overlay.md, O.menuTests.drawn), from profiles of its place (popups.luau, Pp.VIEW_LIST)
-- and of the browser right of it (Br.LIST_REST), off the picture before the click and one after:
--   * a line — a row or a column — has changed its FILL where its mean differs by FILL_MEAN or
--     more, or a channel's mean by FILL_CHANNEL or more: the drawn test's DRAWN_MEAN and
--     DRAWN_CHANNEL;
--   * the list is SEEN where the lines of its place whose fill changed span at least the smallest
--     list (Pp.VIEW_LIST.min, at the factor), at least half the rows and half the columns of that
--     span among them, and no line — no row, no column — of the browser right of its place changed
--     its fill: a list is painted over its place alone, while an expansion that closes changes the
--     whole browser — the grid's other tiles come where its presets were, which is why the drawn
--     test, armed on that click, would take the grid redrawn under the list's place for a popup;
--   * after its field's click it has GONE where its place no longer holds such a change against
--     the picture before Close.
-- STARTING VALUES: no picture of the view list over the grid has been seen yet (TODO.md, T3-4).
-- Pixels only, reduced on the image worker; Luau compares a few hundred numbers.
local FILL_MEAN, FILL_CHANNEL = 6, 10

-- The browser right of the view list's place: the grid's tiles from x 115 there, an open
-- expansion's presets.
Br.LIST_REST = { Pp.VIEW_LIST.rect[3] + 5, X.AREA[2], X.AREA[3], X.AREA[4] }

-- The profiles of the view list's place and of the browser right of it off snapshot `snap`, both
-- axes, waited for: { list, rest }, or nil and why.
function Br.listPlace(snap, pl)
  local out = {}
  for _, w in ipairs({ { "list", Pp.VIEW_LIST.rect }, { "rest", Br.LIST_REST } }) do
    local r = pl:rect(w[2])
    r = r and within(r, snap)
    if not r then return nil, "the list's " .. (w[1] == "list" and "place" or "surroundings") .. " are not in the picture" end
    local ok, p, why = pcall(host.screen.profile, { region = r, axes = "both", snapshot = snap })
    if not ok then return nil, tostring(p) end
    if not p then return nil, tostring(why) end
    out[w[1]] = p
  end
  return out
end

-- Whether line `i` of axes `a` and `b` differs in its fill.
local function fillChanged(a, b, i)
  return math.abs(a.mean[i] - b.mean[i]) >= FILL_MEAN or math.abs(a.r[i] - b.r[i]) >= FILL_CHANNEL
    or math.abs(a.g[i] - b.g[i]) >= FILL_CHANNEL or math.abs(a.b[i] - b.b[i]) >= FILL_CHANNEL
end

-- The lines of axis `a` whose fill changed against `b`'s: the first, the last, and how many.
local function changedSpan(a, b)
  local first, last, n = nil, nil, 0
  for i = 1, math.min(#a.mean, #b.mean) do
    if fillChanged(a, b, i) then
      first = first or i
      last = i
      n += 1
    end
  end
  return first, last, n
end

-- Whether profiles `p` and `q` are of the same rectangle, both axes.
local function comparable(p, q)
  return p ~= nil and q ~= nil and p.columns ~= nil and q.columns ~= nil and p.rows ~= nil and q.rows ~= nil
    and p.x == q.x and p.y == q.y and p.w == q.w and p.h == q.h
end

-- What profiles `now` (Br.listPlace's) show against `before`'s, at placer `pl`'s factor: "list"
-- (the view list painted over its place), "rest" (the browser right of it changed too: the browser
-- itself, no list), "none" (nothing like the list), or nil where the two cannot be compared; and
-- the log's words. Pure.
function Br.listSeen(before, now, pl)
  if not (before and now and comparable(before.list, now.list) and comparable(before.rest, now.rest)) then
    return nil, "the pictures before and after are not of the same place"
  end
  -- Either axis: the grid's few tiles there change the rows they cross by far more than the
  -- columns, whose mean they share with the whole height.
  if changedSpan(now.rest.columns, before.rest.columns) or changedSpan(now.rest.rows, before.rest.rows) then
    return "rest", "the browser right of the list's place changed as well — the browser itself, no list painted over it"
  end
  local c1, c2, cn = changedSpan(now.list.columns, before.list.columns)
  local r1, r2, rn = changedSpan(now.list.rows, before.list.rows)
  if not (c1 and r1) then return "none", "nothing changed in the list's place" end
  local w, h = c2 - c1 + 1, r2 - r1 + 1
  local mw, mh = Pp.VIEW_LIST.min[1] * pl.k, Pp.VIEW_LIST.min[2] * pl.k
  local words = string.format("a change of %dx%d at %d,%d in the list's place, %d of its columns and %d of its rows "
    .. "filled", w, h, now.list.x + c1 - 1, now.list.y + r1 - 1, cn, rn)
  if w < mw or h < mh then return "none", string.format("%s — smaller than the list (%gx%g)", words, mw, mh) end
  if cn < w / 2 or rn < h / 2 then return "none", words .. " — not filled" end
  return "list", words .. ", and nothing right of it — the view list"
end

-- ── FAV and the stars ────────────────────────────────────────────────────────

-- FAV, read off snapshot `snap` as the page key reads it (read/expansion.luau, E.favorite) — an
-- expansion is open where this is asked: true, false, or nil for not read. `marks` gets its point,
-- for a calibration picture.
function Br.fav(snap, pl, marks)
  local got = gridsOf(snap, pl, { ["fav stars"] = true, ["fav star shapes"] = true })
  local v = {}
  E.favorite(pl, got, v, function(s) log("%s", s) end, marks or {}, true)
  return v.favoriteView
end

-- The preset rows that window `window`'s lists show, where this session's own clicks placed them in
-- `expansion` (state.luau, S.browserAt): each { index = its place in the expansion's list (from 1,
-- `lists`' order), row (from 0), y = the row's middle on screen } — or nil where it is not known.
function Br.shownRows(pl, window, expansion, lists)
  local at = S.browserAt(window)
  if not (at and at.expansion == expansion and at.category and at.presets and lists) then return nil end
  local members = {}
  for i, c in ipairs(lists.presetCategories) do
    if c == at.category then members[#members + 1] = i end
  end
  local rows, adjust = Pg.presetRows(at.presets.page, #members)
  local out = {}
  for r = 0, X.PRESET_VISIBLE_ROWS - 1 do
    local place = rows[r]
    if place ~= nil and members[place + 1] then
      local p = Pg.presetPoint(r, adjust)
      local _, y = pl:exact(p[1], p[2])
      out[#out + 1] = { index = members[place + 1], row = r, y = math.floor(y + 0.5) }
    end
  end
  return out
end

-- The stars of rows `rows` ({ index, y }), read off snapshot `snap`: each row's `star` true or
-- false, nil where its box is not in the picture. Two grids of one column, waited for. `marks` gets
-- each star's point.
function Br.stars(snap, pl, rows, marks)
  local col = pl:rect(X.STAR_COLUMN)
  local cells = X.PRESET_VISIBLE_ROWS * STAR_CELLS
  local bright = cellsOf(snap, col and { rect = col, cols = 1, rows = cells, pred = E.BRIGHT })
  local blue = cellsOf(snap, col and { rect = col, cols = 1, rows = cells, pred = Br.BLUE })
  local sx = pl:exact(X.PRESET_FAVORITE_X, 0)
  local hw, hh = X.FAVORITE_STATE[1] / 2 * pl.k, X.FAVORITE_STATE[2] / 2 * pl.k
  local words = {}
  for _, e in ipairs(rows) do
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
  log("the stars: %s", #words > 0 and table.concat(words, "; ") or "no row placed")
end

-- The stars of `rows` as favoriteStates' entries, merged over `into` (or a new table): the preset's
-- place from 0 -> the star read.
function Br.starStates(rows, into)
  local out = into and table.clone(into) or {}
  for _, e in ipairs(rows or {}) do
    if e.index and e.star ~= nil then out[e.index - 1] = e.star end
  end
  return out
end

-- ── The `browser` part (src/act.luau) ────────────────────────────────────────
-- The browser read off picture `snap` for an action of Act.run's — the view chosen, Close, FAV —
-- with `opts` = { pl, expected = the expansion known open, or nil, fields = { "presets" }, window =
-- the plug-in window's id }: the header as text and the picture test (Br.state); FAV's star column
-- where an expansion reads open, and with "presets" the stars of the rows this session's clicks
-- placed. The values (state.luau's) and the keys read: a read of the grid says every open
-- expansion's value is gone with it; one that does not settle it says only what was read.
local OPEN_KEYS = { "lists", "loadedIndex", "favoriteView", "favoriteStates", "pageCategory", "selectedFromDatabase" }

function Br.part(snap, pl, opts)
  local fields = {}
  for _, f in ipairs(opts.fields or {}) do fields[f] = true end
  local v, keys = {}, { "expansionView", "expansionViewFrom" }
  local byName = Br.read(snap, pl)
  local look = Br.look(snap, pl)
  local st, why = Br.state(byName, look, { window = opts.window, zoom = pl.zoom, expected = opts.expected })
  log("%s", why)
  Br.lastPart = { window = opts.window, pl = pl, byName = byName, look = look, st = st, words = why }
  if not st then return v, keys end
  keys[#keys + 1] = "expansionOpen"
  keys[#keys + 1] = "selectedExpansion"
  if not st.open then
    S.forgetBrowserAt(opts.window, "the grid read")
    v.expansionView, v.expansionViewFrom = st.view, st.viewFrom
    v.expansionOpen, v.selectedExpansion = false, ""
    for _, k in ipairs(OPEN_KEYS) do keys[#keys + 1] = k end
    return v, keys
  end
  if st.name and st.name ~= opts.expected then S.forgetBrowserAt(opts.window, "the header reads " .. st.name) end
  v.expansionOpen, v.selectedExpansion = true, st.name or ""
  local fav = Br.fav(snap, pl)
  v.favoriteView = fav
  keys[#keys + 1] = "favoriteView"
  if fav == false then
    keys[#keys + 1] = "favoriteStates"
  elseif fav and fields.presets and st.name then
    local okL, lists = pcall(DB.lists, st.name)
    local rows = okL and lists and Br.shownRows(pl, opts.window, st.name, lists)
    if rows then
      Br.stars(snap, pl, rows)
      v.favoriteStates = Br.starStates(rows)
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
