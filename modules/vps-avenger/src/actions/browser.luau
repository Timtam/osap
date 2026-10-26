-- The expansion browser's actions (the plan's slice 9), for the page's trips: by the avenger_control
-- tool's names and arguments (its actions.py), and since the maintainer's decision of 2026-10-25 as
-- the tool goes through the browser — every expansion, category and preset by its PLACE: the preset
-- database's lists (modules/vps-avenger-presets, the roster's order, an expansion's categories and
-- presets) and the tool's page and scroll tables (src/tables/pages.luau). Nothing below the browser's
-- header is read as text: the grid's names, the categories and the preset rows read nothing at
-- Avenger's sizes, as the tool's developer found. What a click did is read where it can be: the
-- header's name after an Open, the preset's name after Load (a Select is a preview, which loads
-- none: B.preset), a star's pixels after Mark, FAV's pixels, the header and a picture test of its
-- title band after Close, and where nothing told before Close, the view list's pixels
-- (src/read/browser.luau).
-- The tool wrote the state it assumed after every click; here only what was read is posted, and
-- what this session's own clicks did that nothing shows — the category clicked, the presets' page
-- scrolled to, the grid's page — is kept as facts by window (state.luau, S.browserAt) and forgotten
-- where a read says otherwise.
--
--   set_expansion_view {view}   through src/act.luau: the view field opened with the LEFT button, the
--                         view's row chosen in its painted list (popups.luau, Pp.VIEW_LIST), and the
--                         view stored as set once the list took it — the tool's AVGExpansionView —
--                         and the grid confirmed in this window for it, as the tool's action sets
--                         AVGExpansionOpen to 0; nothing clicked where an expansion reads open — the
--                         same point would close it — or where nothing tells whether one is, nor
--                         where a view's label reads the one asked
--   confirm_expansion_view {view}   no trip: the view stored as confirmed and the grid confirmed in
--                         this window for that view and its zoom (the tool's
--                         action_confirm_expansion_view: AVGExpansionOpen = 0), nothing clicked
--   close_expansion       through src/act.luau: the field clicked where the header reads an expansion
--                         open, and where nothing tells, as the tool clicked it whatever was shown —
--                         never where the browser reads closed, where that click opens the view list;
--                         then the header read again, and where nothing told, the view list told by
--                         its pixels and closed again where the click opened it (B.listAfterClose)
--   toggle_expansion_favorite_view {enabled}   through src/act.luau: FAV clicked, its pixels read
--                         before and after, and with FAV on the stars of the rows placed
--   open_expansion {index, …}   by its place (B.byPlace): the page's index into the roster it was
--                         posted, the view read or stored and the tool's page tables — the grid taken
--                         to the page, the tile clicked at the tool's point — CONFIRMED BY THE
--                         HEADER'S NAME after the click; another name is closed again
--   select_expansion_category {index, total}   the category's scrollbar end and its row by the tool's
--                         table (Pg.category), the row clicked, the presets' change watched for
--   select_all_expansion_presets   no click: the name read (the tool read the name)
--   go_to_loaded_preset_category   no trip: the database's category for the name last read, as the
--                         tool's (B.goToCategory)
--   select_expansion_preset, load_expansion_preset, mark_expansion_preset_favorite {preset_index, …}
--                         its category clicked first where this session's clicks did not leave it
--                         shown, the presets' scrollbar clicked to its page by the tool's measured
--                         table (Pg.presetPlace, Pg.presetScroll), then a click, a double-click, or a
--                         click on its star at the tool's row — Select a preview, the strip and the
--                         macros read after it and no name; Load's double-click confirmed by the
--                         preset's name watched after it; the star read after Mark's click
--   align_open_expansion  Correct open Expansion (the plan's slice 10): no click — the name and ONE
--                         picture of the header read with no expansion expected; what it does not
--                         settle is "cannot be told now" (the tool read the area twice, 0.45 s apart)
--
-- Every scrollbar click is followed by a change wait on the list's top and bottom quarter bands, at
-- most STEP_WITHIN, with a still spell of SETTLE: a click at a point that moved nothing is not made
-- again there (the list stands at that end). No timer decides anything; the waits are bounds on
-- looking.

local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local X = host.include("src/tables/expansion.luau")
local Br = host.include("src/read/browser.luau")
local Open = host.include("src/read/open.luau")
local S = host.include("src/state.luau")
local DB = host.include("src/presets.luau")
local Drums = host.include("src/actions/drums.luau")
local Act = host.include("src/act.luau")
local Pp = host.include("src/popups.luau")
local Pg = host.include("src/tables/pages.luau")

local B = {}

local E = nil -- what actions.luau hands over (B.start)

local function log(fmt, ...)
  host.log.info(("[avenger] [browser] " .. fmt):format(...))
end

-- How long a scrollbar click's change wait looks for the list to move, at most — the longest one
-- change wait may wait (docs/api/screen.md) — and how long it must have stood still to count: one or
-- two of Avenger's frames, the host's still spell (docs/api/screen.md, `settle`). Starting values
-- (TODO.md, the third Mac test's T3-4).
B.STEP_WITHIN = 2000
B.SETTLE = 60
-- How long a click's change is looked for: as src/act.luau's.
local CHANGE_WITHIN = 2000

-- The page's word for "every category" (the tool's _ALL_PRESET_CATEGORY).
B.ALL = "__all__"

-- The page's words.
local OPEN_NOW = "an expansion is open in VPS Avenger; press Close Expansion to close it first"
local NONE_OPEN = "no expansion is open in VPS Avenger"
local VIEW_UNREAD = "whether an expansion is open in VPS Avenger cannot be read now"

-- Every value of the browser's (state.luau's): a read that shows the grid, or another expansion,
-- says each of them.
local KEYS = { "expansionView", "expansionViewFrom", "expansionOpen", "selectedExpansion", "lists", "loadedIndex",
  "favoriteView", "favoriteStates", "pageCategory" }

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
end

-- The window the page was shown beside, by its id: what the browser's facts are kept by.
local function windowId()
  local w = E.plugin()
  return w and w.id
end

-- ── Through src/act.luau ─────────────────────────────────────────────────────

-- The view's words, with how it came to be where it was not read (Br.view).
local function viewWords(view, from)
  if from == "set" then return "Expansion View " .. view .. ", set" end
  if from == "confirmed" then return "Expansion View " .. view .. ", confirmed" end
  if from == "stored" then return "Expansion View " .. view .. ", as stored" end
  return "Expansion View " .. view
end
B.viewWords = viewWords

-- A `decide` of src/act.luau's for the browser, with the gate's line (Br.gateLine) of the read it
-- decides by — the `browser` part's (Br.part, Br.lastPart) — and its decision: the click's `gate`
-- words, or what it refuses with.
local function gated(label, decide)
  return function(vb)
    local click, why, focus = decide(vb)
    local g = Br.lastPart
    Br.lastPart = nil
    if g then
      Br.gateLine(label, g, click and ("go on, " .. (click.gate or "clicked at its point")) or ("nothing clicked — "
        .. tostring(why)))
    end
    return click, why, focus
  end
end

function B.set_expansion_view(args, label)
  local view = args.view
  local at = type(view) == "string" and X.viewMenuPoint(view)
  if not at then return nil, tostring(view) .. " is not one of VPS Avenger's views" end
  return {
    label = label,
    part = "browser",
    decide = gated(label, function(vb)
      if vb.expansionOpen == true then return nil, OPEN_NOW end
      if vb.expansionOpen == nil then return nil, VIEW_UNREAD end
      -- A view's label read as the one asked; a view stored is clicked all the same, as the tool did.
      if vb.expansionView == view and vb.expansionViewFrom == nil then return nil, "unchanged, Expansion View " .. view end
      return { at = X.VIEW, menu = { button = "left", item = at, retries = 3, read = false },
        gate = "its field opened with the left button and " .. view .. " chosen in its list" }
    end),
    answer = function(_, _, how)
      if how ~= "chosen" then return { error = "the view was not chosen" } end
      Br.storeView(view, "set")
      -- The grid shows in the view chosen, no expansion open: the tool's AVGExpansionOpen = 0.
      local id = windowId()
      local v = S.values(id)
      S.confirmGrid(id, view, v and v.zoom, "set")
      return { part = { keys = { "expansionView", "expansionViewFrom" }, values = { expansionView = view,
        expansionViewFrom = "set" } }, said = viewWords(view, "set") }
    end,
  }
end

-- confirm_expansion_view {view}: the page's Confirm Expansion View — no trip, no click. The view
-- stored as confirmed and, for window `id` at zoom `zoom`, the grid confirmed with it (S.confirmGrid):
-- the user's word over what was read before, as the tool's action_confirm_expansion_view sets
-- AVGExpansionView, AVGExpansionOpen = 0 and AVGSelectedExpansion = "". The part to post — { keys,
-- values }, every open expansion's value gone (state.luau, S.part) — or nil and why.
function B.confirmView(args, id, zoom)
  local view = args.view
  if not (type(view) == "string" and X.viewMenuPoint(view)) then
    return nil, tostring(view) .. " is not one of VPS Avenger's views"
  end
  if not id then return nil, "the page was not opened from VPS Avenger; press the page key there" end
  Br.storeView(view, "confirmed")
  -- What this session's clicks left there first, a confirmation before this one with it.
  B.forgetPlaces(id, "Confirm Expansion View")
  S.confirmGrid(id, view, zoom, "confirmed")
  log("the grid confirmed in window %s for %s at %s %%", tostring(id), view, tostring(zoom))
  return { keys = KEYS, values = { expansionView = view, expansionViewFrom = "confirmed", expansionOpen = false,
    selectedExpansion = "" } }
end

-- Close Expansion. With the grid shown, its point opens the view list instead, so where the browser
-- reads closed nothing is clicked; where it reads open, or where nothing tells — the header holds
-- something no name is read in, and the picture test settles nothing — it is clicked, as the tool
-- clicks it whatever is shown (the maintainer's decision of 2026-10-24), and the browser read again
-- after the change. Where nothing told, `unsure()` says so: the view list's place and the browser
-- right of it are profiled off the picture before the click (`pictured`, Br.listPlace), and the
-- trip looks for the view list the click may have opened by its pixels (B.listAfterClose, with
-- `listBefore()`).
function B.close_expansion(_, label, known)
  local unsure, listBefore = false, nil
  return {
    label = label,
    part = "browser",
    expected = known and known.selectedExpansion or nil,
    decide = gated(label, function(vb)
      if vb.expansionOpen == false then return nil, NONE_OPEN end
      unsure = vb.expansionOpen ~= true
      if unsure then log("'%s': nothing tells whether an expansion is open — clicked as the tool clicks it", label) end
      -- The grid's page stays as it was, as the tool keeps AVGExpansionPageStart; an open
      -- expansion's lists go with it.
      S.forgetBrowserAt(windowId(), "Close Expansion")
      return { at = X.CLOSE, gate = unsure and "clicked at its point as the tool clicks it — nothing tells" or nil }
    end),
    pictured = function(snap, pl)
      if not unsure then return end
      local p, why = Br.listPlace(snap, pl)
      listBefore = p
      if not p then log("'%s': the view list's place not profiled before the click — %s", label, tostring(why)) end
    end,
    unsure = function() return unsure end,
    listBefore = function() return listBefore end,
    watch = function() return { X.VIEW_TEXT, X.NAME_TEXT, X.AREA } end,
    -- Where nothing told, what changed is the answer: a list drawn over the grid reads as nothing
    -- settled, and is no reason to look on.
    same = function(_, va)
      if unsure then return nil end
      return va.expansionOpen ~= false
    end,
    answer = function(_, va)
      if va and va.expansionOpen == false then
        return { said = "closed; " .. viewWords(va.expansionView, va.expansionViewFrom), focus = "expansion-search-results" }
      end
      if va and va.expansionOpen == true then return { error = "VPS Avenger still shows the expansion open" } end
      return { error = VIEW_UNREAD }
    end,
  }
end

-- What the page sends as `enabled`: true, false, or nil where FAV was not read (the tool's forms).
local function flag(v)
  if v == nil then return nil end
  if type(v) == "boolean" then return v end
  if type(v) == "number" then return v ~= 0 end
  local s = string.lower(tostring(v))
  return s == "1" or s == "true" or s == "yes" or s == "on"
end

function B.toggle_expansion_favorite_view(args, label, known)
  local want = flag(args.enabled)
  return {
    label = label,
    part = "browser",
    fields = { "presets" },
    expected = known and known.selectedExpansion or nil,
    decide = gated(label, function(vb)
      if vb.expansionOpen == false then return nil, NONE_OPEN end
      if vb.expansionOpen ~= true then return nil, VIEW_UNREAD end
      if want ~= nil and vb.favoriteView == want then return nil, "unchanged, FAV " .. (want and "ON" or "OFF") end
      return { at = X.FAVORITE_VIEW }
    end),
    -- FAV's box, and the star column, which comes and goes with it.
    watch = function() return { Drums.box(X.FAVORITE_VIEW, X.FAVORITE_VIEW_STATE), X.STAR_COLUMN } end,
    same = function(vb, va) return va.favoriteView ~= nil and va.favoriteView == vb.favoriteView end,
    answer = function(_, va, how)
      local f = va and va.favoriteView
      if f == nil then return { error = "the favorites view cannot be read now" } end
      local words = "FAV " .. (f and "ON" or "OFF")
      if how == "unchanged" then return { note = "unchanged, " .. words } end
      return { said = words }
    end,
  }
end

-- ── Pictures and change waits ────────────────────────────────────────────────

-- Where the browser is on overlay `o` now: { pl = the placer, region = its picture's rectangle,
-- window = the plug-in window's id }, or nil and the page's error.
local function placed(o)
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return nil, "VPS Avenger's size cannot be told now" end
  local pl = P.placer(origin, k, (H.zoom(origin)))
  local region = Open.region(pl, X.BROWSER)
  if not region then return nil, "that part of VPS Avenger is not on the screen" end
  return { pl = pl, region = region, window = windowId() }
end

-- One picture of the browser, waited for: the snapshot, or nil and the page's error.
local function picture(ctx)
  local ok, snap, why = pcall(host.screen.snapshot, { region = ctx.region })
  if ok and snap then return snap end
  log("no picture of the browser: %s", tostring(ok and why or snap))
  return nil, "VPS Avenger cannot be read now"
end

-- The header read off picture `s` as text, the picture test, and the state they say with the
-- expansion `expected` (Br.state): the state, its words, the readings and the test.
local function stateOf(ctx, s, expected)
  local byName = Br.read(s, ctx.pl)
  local look = Br.look(s, ctx.pl)
  local st, words = Br.state(byName, look, { window = ctx.window, zoom = ctx.pl.zoom, expected = expected })
  return st, words, byName, look
end

-- Table rectangles `rects` on screen, cut to the browser's picture.
local function onScreen(ctx, rects)
  local out = {}
  for _, t in ipairs(rects) do
    local r = ctx.pl:rect(t)
    if r then
      local g = ctx.region
      r = { math.max(r[1], g[1]), math.max(r[2], g[2]), math.min(r[3], g[3]), math.min(r[4], g[4]) }
      if (r[3] - r[1]) * (r[4] - r[2]) >= 4 then out[#out + 1] = r end
    end
  end
  return out
end

-- A change wait on the browser from picture `from` — which it takes over, and lets go of once it has
-- answered — on screen rectangles `watch`, at most `within` ms, with a still spell of `settle`; then
-- `next(picture or nil, changed)` with the trip still in front. Nothing more where it is not: the
-- trip's give-up says the rest.
local function waitFrom(trip, ctx, from, watch, within, settle, next)
  local change = { from = from, watch = #watch > 0 and watch or nil, timeout = math.clamp(math.floor(within), 1, 2000),
    tolerance = 16, minPixels = 4, settle = settle > 0 and math.min(settle, math.floor(within)) or nil }
  local ok, err = pcall(host.screen.snapshot, { region = ctx.region, change = change }, function(s, why, info)
    from:release()
    if not trip:inFront() then
      if s then s:release() end
      return
    end
    if not s then log("no picture after the change wait: %s", tostring(why)) end
    next(s, type(info) == "table" and info.changed == true)
  end)
  if not ok then
    from:release()
    log("no change wait: %s", tostring(err))
    next(nil, false)
  end
end

-- After a click: change waits from picture `from` on table rectangles `rects`, CHANGE_WITHIN in all,
-- each picture read with `read(s) -> r`; a picture that changed where `taken(r)` says nothing this
-- action changes did — the pointer's hover drawn first — is looked on from, within the same bound, as
-- src/act.luau looks on. `next(r or nil, changed, s)` once: with `keep`, the last picture held for
-- the caller (`s`), otherwise let go.
local function lookOn(trip, ctx, from, rects, read, taken, next, keep)
  local deadline = host.now() + CHANGE_WITHIN
  local watch = onScreen(ctx, rects)
  local function round(pic)
    waitFrom(trip, ctx, pic, watch, math.max(1, deadline - host.now()), 0, function(s, changed)
      if not s then return next(nil, false, nil) end
      local r = read(s)
      if not trip:inFront() then
        s:release()
        return
      end
      if changed and not taken(r) and deadline - host.now() >= 1 then
        log("the picture changed and what this action changes did not, %d ms after the click — looking on from that "
          .. "picture", CHANGE_WITHIN - (deadline - host.now()))
        return round(s)
      end
      if keep then return next(r, changed, s) end
      s:release()
      next(r, changed, nil)
    end)
  end
  round(from)
end

-- ── The view list after Close (A10, part 3) ──────────────────────────────────
-- Where nothing told whether an expansion was open, Close was clicked as the tool clicks it; with
-- the grid shown that click opened the view list (popups.luau, Pp.VIEW_LIST). Nothing there is read
-- as text (the maintainer's decision of 2026-10-25): the list is told by its PIXELS
-- (read/browser.luau, Br.listSeen) — its place and the browser right of it, profiled off the
-- picture before the click (B.close_expansion's `pictured`) and off one after it, seen where a
-- filled change of a list's size came over its place and nothing right of it changed. Seen, it is
-- closed as a left opener's popup is, by a click on its field (main.luau, closeLeftOpen), and
-- looked at again after a change wait on its place: gone, no expansion was open, and the grid is
-- posted in the view stored; still there, said so. Not seen, the result of the click stands.
-- Readings only, every one bounded by a change wait's: no timer decides anything.

-- After Close where nothing told (B.close_expansion's `unsure`): `result` the click's, as src/act.luau
-- answered it, and `before` the profiles of the list's place before the click (its `listBefore()`);
-- the trip done with it, or with what the view list's pixels say.
function B.listAfterClose(o, trip, result, label, before)
  local values = result.part and result.part.values
  if values and values.expansionOpen == true then return trip:done(result) end
  if not before then
    log("'%s': the view list not looked for — its place was not profiled before the click", label)
    return trip:done(result)
  end
  local ctx = placed(o)
  if not ctx then return trip:done(result) end
  local snap = picture(ctx)
  if not trip:inFront() then
    if snap then snap:release() end
    return
  end
  if not snap then
    log("'%s': the view list not looked for — no picture after the click", label)
    return trip:done(result)
  end
  local now, why = Br.listPlace(snap, ctx.pl)
  if not trip:inFront() then
    snap:release()
    return
  end
  local seen, words = Br.listSeen(before, now, ctx.pl)
  log("'%s': the view list by its pixels — %s", label, now and words or ("its place not profiled: " .. tostring(why)))
  if seen ~= "list" then
    snap:release()
    return trip:done(result)
  end
  log("'%s': the click opened the view list — no expansion was open; its field clicked again to close it", label)
  local ok, said = E.ring.clickAt(o, label, X.VIEW, nil, true)
  if not ok then
    snap:release()
    return trip:done({ sentence = said })
  end
  local unread = "the click opened the view list, and whether it closed again cannot be read now"
  waitFrom(trip, ctx, snap, onScreen(ctx, { Pp.VIEW_LIST.rect }), CHANGE_WITHIN, B.SETTLE, function(s)
    if not s then
      log("'%s': no picture after the field's click", label)
      return trip:done({ error = unread })
    end
    local again, whyA = Br.listPlace(s, ctx.pl)
    s:release()
    if not trip:inFront() then return end
    local left, wordsA = Br.listSeen(before, again, ctx.pl)
    log("'%s': after its field's click, %s", label, again and wordsA or ("its place not profiled: " .. tostring(whyA)))
    if left == nil then return trip:done({ error = unread }) end
    if left == "list" then return trip:done({ error = "the click opened the view list, and it is still open" }) end
    local view, how = Br.view()
    log("'%s': the view list has gone; the grid in the view %s (%s)", label, tostring(view), tostring(how))
    trip:done({ part = { keys = KEYS, values = { expansionOpen = false, selectedExpansion = "", expansionView = view,
      expansionViewFrom = how } }, note = "no expansion was open; the view list the click opened is closed again" })
  end)
end

-- ── Scrollbars ───────────────────────────────────────────────────────────────

-- The top and bottom quarter of list `list`'s region, the bands a scrollbar click's change is
-- watched in — away from the pointer in the list's middle, whose hover is not the list moving.
local function bands(list)
  local r = X.LISTS[list].rect
  local q = (r[4] - r[2]) / 4
  return { { r[1], r[2], r[3], r[2] + q }, { r[1], r[4] - q, r[3], r[4] } }
end

local LIST_WORDS = { grid = "the grid's scrollbar", categories = "the categories' scrollbar",
  presets = "the presets' scrollbar" }

-- Clicks `clicks` (table points, the tool's) on list `list`'s scrollbar in turn, from picture `pic`
-- (taken over), each followed by a change wait on the list's bands: a click at a point that moved
-- nothing is not made again at that point, as the list stands at that end. `done(picture or nil,
-- sentence?)` with the last picture — the caller's — or nil where none came; with the sentence where
-- a point could not be clicked. Nothing where the trip ended meanwhile.
local function clickSeries(o, trip, ctx, list, pic, clicks, label, done)
  local watch = onScreen(ctx, bands(list))
  local i, still = 0, nil
  local function step()
    i += 1
    while clicks[i] and still and clicks[i][1] == still[1] and clicks[i][2] == still[2] do i += 1 end
    local c = clicks[i]
    if not c then return done(pic) end
    local ok, said = E.ring.clickAt(o, label, c, nil, true)
    if not ok then
      pic:release()
      return done(nil, said)
    end
    waitFrom(trip, ctx, pic, watch, B.STEP_WITHIN, B.SETTLE, function(s, changed)
      if not s then return done(nil) end
      pic = s
      still = (not changed) and c or nil
      log("'%s': %s clicked at %.1f — the list %s", label, LIST_WORDS[list], c[2], changed and "moved" or "stood still")
      step()
    end)
  end
  step()
end

-- ── What the reads say ───────────────────────────────────────────────────────

-- The values a read of the browser says, from its state `st` (Br.state's): the grid — every open
-- expansion's value gone; an expansion open — the one the read names, or the one at its place
-- where only that named it, with its lists from the database where it is another than the one
-- `known` had, or none. And the keys they are posted under.
local function partOf(st, known)
  local v = {}
  if not st then return nil end
  if st.open == false then
    v.expansionView, v.expansionViewFrom, v.expansionOpen, v.selectedExpansion = st.view, st.viewFrom, false, ""
    return { keys = KEYS, values = v }
  end
  v.expansionOpen = true
  v.selectedExpansion = st.name or ""
  if st.name and st.name == known.selectedExpansion then
    return { keys = { "expansionView", "expansionViewFrom", "expansionOpen", "selectedExpansion" }, values = v }
  end
  if st.name then
    local ok, lists = pcall(DB.lists, st.name)
    if ok and lists then v.lists = lists end
  end
  return { keys = KEYS, values = v }
end

-- `a`'s keys and values, then `b`'s over them.
local function merged(a, b)
  if not a then return b end
  if not b then return a end
  local keys, values, seen = {}, {}, {}
  for _, p in ipairs({ a, b }) do
    for _, k in ipairs(p.keys) do
      if not seen[k] then
        seen[k] = true
        keys[#keys + 1] = k
      end
      values[k] = p.values[k]
    end
  end
  return { keys = keys, values = values }
end

-- Where name `name` (read in Avenger) lies in expansion `e`'s list, from 0 — the page's
-- loaded_expansion_preset_index — or nil.
local function loadedIn(name, e)
  if not (name and e and e ~= "" and DB.present) then return nil end
  local ok, p = pcall(DB.place, name)
  if ok and p and p.expansion == e then return p.index - 1 end
  return nil
end

-- The stars read in `rows` over the ones `known` read with FAV on in the same expansion.
local function starsOver(rows, known, expansion)
  local before = known.selectedExpansion == expansion and known.favoriteView and known.favoriteStates or nil
  return Br.starStates(rows, before)
end

-- ── The trips ────────────────────────────────────────────────────────────────

-- The first look of a trip: the browser pictured, its header read and its picture tested, its state
-- with `expected`; the picture held. nil where the trip ended or is done (`trip:done` called).
local function look(o, trip, expected)
  local ctx, why = placed(o)
  if not ctx then
    trip:done({ error = why })
    return nil
  end
  local snap, whyS = picture(ctx)
  if not snap then
    trip:done({ error = whyS })
    return nil
  end
  -- The picture was waited for: nothing is read of a place the user has left.
  if not trip:inFront() then
    snap:release()
    return nil
  end
  local st, words, byName, test = stateOf(ctx, snap, expected)
  if not trip:inFront() then
    snap:release()
    return nil
  end
  ctx.snap, ctx.byName, ctx.st, ctx.look, ctx.words = snap, byName, st, test, words
  return ctx
end

-- The gate's line, once a trip (read/browser.luau, Br.gateLine): what the browser's actions decide
-- by, and `decision`, the action's.
local function gate(ctx, label, decision)
  Br.gateLine(label, ctx, decision)
end

-- The name the page means by `args.index` in the list of expansions it was last posted (page.luau),
-- or nil and why.
function B.openArg(args)
  local i = whole(args.index)
  local shown = E.shown()
  local name = i and shown and shown[i + 1]
  if not name then return nil, "no expansion was named" end
  return name
end

-- Where the grid was last put by place (B.byPlace), by window id: { view, page, count }; forgotten
-- where the expansion it opened is not confirmed, and where the user's own Browse Presets had
-- Avenger's browser (B.forgetPlaces).
local placedAt = {}

-- What this session's clicks left where in window `id` — the grid's page, the open expansion's
-- lists — forgotten, and with them the grid's confirmation there (state.luau, S.forgetGrid): the
-- user's own browsing in Avenger's browser may have opened an expansion, which nothing then read
-- would contradict where the header reads no name. Why in the log.
function B.forgetPlaces(id, why)
  if not id then return end
  if placedAt[id] then
    placedAt[id] = nil
    log("the grid's page is forgotten: %s", tostring(why))
  end
  S.forgetBrowserAt(id, why)
  S.forgetGrid(id, why)
end

-- ── Opening an expansion ─────────────────────────────────────────────────────
-- As the tool opens it: by its place in the list chosen (the page's index into the roster it was
-- posted), the view stored or read, and the tool's page tables (src/tables/pages.luau): the grid
-- taken to the expansion's page by the tool's clicks on its scrollbar, each followed by a change
-- wait on the grid — a click that moves nothing leaves the grid where that point puts it, and is
-- not made again — and the tile clicked at the tool's point. CONFIRMED BY THE HEADER'S NAME after
-- the click: the one asked, it is open; another, it is closed again and said so; an expansion open
-- whose name the header does not read — its title band lit — the one at the place clicked (its
-- place in the database's list names it), posted as any name is and the answer saying the header
-- did not read it. Where the grid's page is not known — none of this session's clicks placed it in
-- this window and view — it is taken to its top first, by the tool's clicks for its first page from
-- its last; then the page placed is kept, as the tool kept AVGExpansionPageStart, and forgotten
-- where the header does not confirm the expansion asked.
function B.byPlace(o, trip, ctx, spec)
  local name, view, label, index, list = spec.name, spec.view, spec.label, spec.index, spec.list
  local pic = ctx.snap
  local p = index and list and Pg.place(view, index, #list)
  local at = p and Pg.cell(view, p)
  if not at then
    pic:release()
    log("'%s': %s cannot be placed by its place %s of %d in the view %s — nothing clicked", label, name, tostring(index),
      list and #list or 0, view)
    return spec.finish({ part = spec.part, error = name .. " cannot be placed in VPS Avenger's browser by its place" },
      "nothing clicked — no place for it")
  end
  local key = ctx.window
  local was = key and placedAt[key]
  local from = (was and was.view == view and was.count == p.count) and was.page or nil
  local unknown = from == nil and p.count > 1
  local clicks = {}
  -- A grid of one page does not scroll: it stands at its top.
  if unknown then
    for _, c in ipairs(Pg.scroll(view, p.count - 1, 0, p.count)) do clicks[#clicks + 1] = c end
  end
  from = from or 0
  if p.page ~= from then
    for _, c in ipairs(Pg.scroll(view, from, p.page, p.count)) do clicks[#clicks + 1] = c end
  end
  spec.path = string.format("place %d of %d in the list chosen (page %d of %d, tile %d, %s)", index + 1, #list,
    p.page + 1, p.count, p.visible + 1, view)
  log("'%s': %s placed by its place %d of %d in the list chosen: page %d of %d, tile %d, in the view %s; %d click(s) on "
    .. "the scrollbar first%s", label, name, index + 1, #list, p.page + 1, p.count, p.visible + 1, view, #clicks,
    unknown and " (the grid's page not known: its top first)" or "")
  if #clicks > 0 and spec.said then host.speech.output(spec.said, { interrupt = false }) end
  if key then placedAt[key] = nil end
  clickSeries(o, trip, ctx, "grid", pic, clicks, label, function(s, sentence)
    if sentence then return spec.finish({ sentence = sentence }, "a scrollbar click refused") end
    if not s then return spec.finish({ error = "VPS Avenger's browser cannot be read now" }, "no picture after a scrollbar click") end
    if key then placedAt[key] = { view = view, page = p.page, count = p.count } end
    spec.click(at, s)
  end)
end

-- open_expansion: in trip `trip`'s act on overlay `o`, a handler where reads wait, and `trip:inFront()`
-- after every wait — false has ended the trip, and its give-up says the rest.
function B.open(o, trip, args, name, label, known)
  local ctx = look(o, trip, known.selectedExpansion)
  if not ctx then return end
  local st, pl = ctx.st, ctx.pl
  local function refuse(why, part)
    ctx.snap:release()
    gate(ctx, label, "nothing clicked — " .. why)
    trip:done({ part = part, error = why })
  end
  if not st then return refuse(VIEW_UNREAD) end
  if st.open then return refuse(OPEN_NOW, partOf(st, known)) end
  -- The view the place is placed by is the one its label reads now, or else the one stored — set
  -- from the page or confirmed there (read/browser.luau, Br.view): a grid in another view puts the
  -- expansion elsewhere, which is why the page says how the view came to be.
  local view = st.view
  if not view then return refuse(VIEW_UNREAD, partOf(st, known)) end
  gate(ctx, label, "go on, by its place")
  log("'%s': the page's place, page start %s, tile %s — the tool's tables place it again", label,
    tostring(args.page_start), tostring(args.visible_index))
  local spec = { name = name, view = view, label = label, index = whole(args.index), list = E.shown(),
    part = partOf(st, known), said = "Looking for " .. name .. " in VPS Avenger's list by its place." }
  local function finish(result, what)
    log("'%s': path %s, %s", label, spec.path or "none", what)
    trip:done(result)
  end
  spec.finish = finish
  local clicked = "nothing clicked"

  -- Not confirmed as the one asked — another named, the grid still shown or nothing read: the grid's
  -- page may not be the one this session placed it at (the user may have scrolled it), so it is not
  -- kept, and the next Open takes the grid to its top first rather than click the same wrong tile.
  local function unconfirmed()
    local k = ctx.window
    if k and placedAt[k] then
      placedAt[k] = nil
      log("'%s': the place is not confirmed — the grid's page is forgotten", label)
    end
  end
  -- The header reads `other`: Close clicked, and the browser read again after the change.
  local function closeAgain(other)
    log("'%s': the header reads %s, not %s — the place was another's; closed again", label, other, name)
    local words = "VPS Avenger opened " .. other .. " where " .. name .. " lies by its place in the list chosen"
    local ctx2 = placed(o)
    local before = ctx2 and picture(ctx2)
    if not trip:inFront() then
      if before then before:release() end
      return
    end
    if not before then return finish({ error = words .. ", and it could not be closed again" }, clicked .. ", " .. other
      .. " in the header") end
    local ok, said = E.ring.clickAt(o, label, X.CLOSE, nil, true)
    if not ok then
      before:release()
      return finish({ sentence = said }, clicked .. ", " .. other .. " in the header, Close refused")
    end
    lookOn(trip, ctx2, before, { X.VIEW_TEXT, X.NAME_TEXT, X.AREA }, function(s) return (stateOf(ctx2, s, nil)) end,
      function(st3) return st3 ~= nil and st3.open == false end, function(st3)
        if st3 and st3.open == false then
          return finish({ part = partOf(st3, known), error = words .. "; it was closed again" }, clicked .. ", " .. other
            .. " in the header, closed again")
        end
        finish({ error = words .. ", and it is still open" }, clicked .. ", " .. other .. " in the header, still open")
      end)
  end
  -- After the click: the header read and its picture tested, FAV where an expansion reads open.
  local function readOpen(s)
    local st2, words = stateOf(ctx, s, nil)
    log("'%s': after the click, %s", label, words)
    local r = { st = st2 }
    if st2 and st2.open then
      local marks = {}
      r.fav = Br.fav(s, pl, marks)
      Br.picture(string.format("browser-open-%dpct", pl.zoom or 0), s, ctx.region, marks)
    end
    return r
  end
  local function opened(r, changed)
    local st2 = r and r.st
    if not (st2 and st2.open and st2.name == name) then unconfirmed() end
    if not st2 then return finish({ error = "VPS Avenger's browser cannot be read now" }, clicked .. ", nothing read after") end
    if st2.open and st2.name and st2.name ~= name then return closeAgain(st2.name) end
    if st2.open == false then
      log("'%s': the grid still shown (%s)", label, changed and "the picture changed" or "unchanged")
      return finish({ part = partOf(st2, known), error = "nothing VPS Avenger shows changed" }, clicked
        .. ", the grid still shown")
    end
    local okL, lists = pcall(DB.lists, name)
    local v = { expansionOpen = true, selectedExpansion = name, lists = okL and lists or nil, favoriteView = r.fav,
      loadedIndex = loadedIn(E.ring.lastName(), name) }
    -- What this session's click opened, kept for the reads after it (Br.state): its lists' places
    -- not known yet — nothing below the header is read — so its first category's or preset's action
    -- clicks the category first.
    S.setBrowserAt(ctx.window, { expansion = name })
    if not st2.name then
      return finish({ part = { keys = KEYS, values = v }, focus = "open-expansion-name", note = "VPS Avenger shows an "
        .. "expansion open where " .. name .. " lies by its place in the list chosen; its name cannot be read in the "
        .. "header now" }, clicked .. ", an expansion open, its name not read in the header")
    end
    finish({ part = { keys = KEYS, values = v }, focus = "open-expansion-name" }, clicked .. ", the header confirms it")
  end
  spec.click = function(at, pic)
    local x, y = pl:point(at[1], at[2])
    log("'%s': clicking %s at (%d,%d), the tool's point for its place", label, name, x, y)
    S.forgetGrid(ctx.window, "Open clicks a tile")
    S.forgetBrowserAt(ctx.window, "Open clicks a tile")
    local ok, said = E.ring.clickPoint(o, label, x, y, nil, true)
    if not ok then
      pic:release()
      return finish({ sentence = said }, "the tile's click refused")
    end
    clicked = string.format("clicked (%d,%d)", x, y)
    Br.picture(string.format("browser-grid-%s-%dpct", (string.gsub(string.lower(view), " ", "-")), pl.zoom or 0), pic,
      ctx.region, { { x = x, y = y } })
    lookOn(trip, ctx, pic, { X.VIEW_TEXT, X.NAME_TEXT, X.AREA }, readOpen, function(r2)
      return r2.st ~= nil and r2.st.open == true
    end, opened)
  end
  B.byPlace(o, trip, ctx, spec)
end

-- ── An open expansion's lists ────────────────────────────────────────────────

-- Why nothing is done in an open expansion, from the first look's state `st`: nil where the
-- expansion `expansion` (the one known open) reads open, or an expansion reads open and is not named.
local function notOpen(st, expansion)
  if not st then return VIEW_UNREAD end
  if st.open ~= true then return NONE_OPEN end
  if st.name and st.name ~= expansion then return "VPS Avenger shows another expansion, " .. st.name end
  return nil
end

-- The category and its place for `index` (from 0) in the expansion `known` has open, or nil and why.
function B.categoryArg(args, known)
  local i = whole(args.index)
  if not i then return nil, "no category was named" end
  if not DB.present then return nil, "the preset database is not installed" end
  local lists = known and known.expansionOpen and known.selectedExpansion ~= "" and known.lists
  if not lists then return nil, "which expansion is open in VPS Avenger is not known" end
  if not lists.categories[i + 1] then return nil, "no category was named" end
  return i + 1
end

-- The presets of category `cat` in `lists`: their places in the expansion's list (from 1), in its
-- order — the order Avenger lists a category's presets in, as the tool counts them.
local function membersOf(lists, cat)
  local out = {}
  for i, c in ipairs(lists.presetCategories) do
    if c == cat then out[#out + 1] = i end
  end
  return out
end

-- Category `cat` of `lists` (the expansion `expansion`'s) clicked, from picture `pic` (taken over):
-- the categories' scrollbar first clicked to the end that shows it where this session's clicks did
-- not leave it there (Pg.category, Pg.categoryScroll), then its row; a change wait on the two lists
-- after it. `done(picture or nil, changed, sentence?)` — the picture the caller's — and the facts
-- (S.browserAt) say the category clicked and its presets at their top, as the tool takes them after
-- a category's click.
local function clickCategory(o, trip, ctx, label, expansion, lists, cat, pic, done)
  local ci = table.find(lists.categories, cat)
  local place = ci and Pg.category(ci - 1, #lists.categories)
  if not place then
    pic:release()
    return done(nil, false, nil, cat .. " is not one of " .. expansion .. "'s categories in the database")
  end
  local at = S.browserAt(ctx.window)
  if not (at and at.expansion == expansion) then at = { expansion = expansion } end
  local clicks = (place.at and at.categories ~= place.at) and Pg.categoryScroll(place.at) or {}
  if #clicks > 0 then
    log("'%s': the list of categories to its %s first, for %s, the %d of %d", label, place.at, cat, ci, #lists.categories)
  end
  clickSeries(o, trip, ctx, "categories", pic, clicks, label, function(s, sentence)
    if sentence then return done(nil, false, sentence) end
    if not s then return done(nil, false) end
    if place.at then at.categories = place.at end
    local x, y = ctx.pl:point(place.point[1], place.point[2])
    log("'%s': clicking %s at (%d,%d), the tool's row %d of the categories", label, cat, x, y, place.row + 1)
    local ok, said = E.ring.clickPoint(o, label, x, y, nil, true)
    if not ok then
      s:release()
      return done(nil, false, said)
    end
    at.category, at.presets = cat, { page = 0, first = 0, final = false }
    S.setBrowserAt(ctx.window, at)
    waitFrom(trip, ctx, s, onScreen(ctx, { X.CATEGORY_TEXT, X.PRESET_TEXT }), CHANGE_WITHIN, B.SETTLE, function(s2, changed)
      done(s2, changed)
    end)
  end)
end

-- select_expansion_category: the category clicked by its place (clickCategory), then FAV and, with
-- FAV on, the stars of its first page's rows. Nothing reads which presets Avenger lists: the change
-- after the click is all there is, and the category is the page's from then on.
function B.category(o, trip, target, label, known)
  local lists, expansion = known.lists, known.selectedExpansion
  local name = lists.categories[target]
  local ctx = look(o, trip, expansion)
  if not ctx then return end
  local why = notOpen(ctx.st, expansion)
  if why then
    ctx.snap:release()
    gate(ctx, label, "nothing clicked — " .. why)
    return trip:done({ part = partOf(ctx.st, known), error = why })
  end
  gate(ctx, label, "go on, by its place")
  local was = S.browserAt(ctx.window)
  local shownBefore = was and was.expansion == expansion and was.category == name
  clickCategory(o, trip, ctx, label, expansion, lists, name, ctx.snap, function(s, changed, sentence, refusal)
    if sentence then return trip:done({ sentence = sentence }) end
    if refusal then return trip:done({ error = refusal }) end
    if not s then return trip:done({ error = "VPS Avenger's browser cannot be read now" }) end
    local fav = Br.fav(s, ctx.pl)
    local v = { pageCategory = name, favoriteView = fav }
    local keys = { "pageCategory", "favoriteView", "favoriteStates" }
    if fav then
      local rows = Br.shownRows(ctx.pl, ctx.window, expansion, lists)
      if rows then
        Br.stars(s, ctx.pl, rows)
        v.favoriteStates = starsOver(rows, known, expansion)
      end
    end
    s:release()
    local result = { part = { keys = keys, values = v } }
    if not changed then
      log("'%s': nothing in the lists changed after %s's click%s", label, name, shownBefore and " — it was shown" or "")
      result.note = shownBefore and ("unchanged, " .. name)
        or (name .. " clicked; nothing VPS Avenger shows changed, and its presets cannot be read")
    end
    trip:done(result)
  end)
end

-- What a preset row's action is given: { target = its place in the list, from 1, category, keep =
-- whether the page shows every category } — or nil and why.
function B.presetArg(args, known)
  local i = whole(args.preset_index)
  local lists = known and known.lists
  if not (i and lists and lists.presets[i + 1]) then return nil, "no preset was named" end
  return { target = i + 1, category = lists.presetCategories[i + 1], keep = args.keep_preset_category == B.ALL }
end

-- select_expansion_preset ("select"), load_expansion_preset ("load"), mark_expansion_preset_favorite
-- ("mark"), as the tool clicks them: the preset's category clicked first where this session's clicks
-- did not leave it shown (the tool clicked it where the page showed every category, and assumed it
-- shown otherwise), the presets' scrollbar clicked to the preset's page by the tool's table, then
-- its row's click, its double-click, or its star's click. Load is confirmed by the preset's name
-- read after it, `forget` (actions.luau's go) told before its double-click; a name that is not the
-- preset asked is said, and where the lists stand is forgotten. Select is a PREVIEW: Avenger lets
-- the preset be heard and loads none — the header's name stays — so, as the tool's
-- action_select_expansion_preset, no name is read before or after it, nothing is forgotten, and
-- nothing is said of the name (the avenger_control developer's report of 2026-10-26, of the
-- "unchanged, <name>" a Select brought: "Das ist auch nicht nötig. Das kannst du rausnehmen.");
-- the presets' change is waited for, then the strip and the macros read again, the tool's refresh.
function B.preset(o, trip, forget, kind, arg, label, known)
  local lists, expansion = known.lists, known.selectedExpansion
  local target, cat = arg.target, arg.category
  local pname = lists.presets[target]
  local was = nil
  if kind == "load" then
    was = E.ring.nameNow(o) or E.ring.lastName()
    if not trip:inFront() then return end
  end
  local ctx = look(o, trip, expansion)
  if not ctx then return end
  local pl = ctx.pl
  local fav = Br.fav(ctx.snap, pl)
  local why = notOpen(ctx.st, expansion)
  if not why and kind == "mark" then
    if fav == false then why = "the favorites view is off in VPS Avenger; turn FAV on first" end
    if fav == nil then why = "the favorites view cannot be read now" end
  end
  if not why and (not cat or cat == "") then why = pname .. " has no category in the database" end
  local members = cat and membersOf(lists, cat) or {}
  local within = table.find(members, target)
  if not why and not within then why = pname .. " has no place in its category in the database" end
  if why then
    ctx.snap:release()
    gate(ctx, label, "nothing clicked — " .. why)
    return trip:done({ part = partOf(ctx.st, known), error = why })
  end
  gate(ctx, label, "go on, by its place")
  local page = arg.keep and B.ALL or cat
  local place = Pg.presetPlace(within - 1, #members)
  -- The browser's values after the row's action: the page's category, FAV and the stars.
  local function browserPart(rows)
    local v = { pageCategory = page, favoriteView = fav }
    if fav and rows then v.favoriteStates = starsOver(rows, known, expansion) end
    return { keys = { "pageCategory", "favoriteView", "favoriteStates" }, values = v }
  end
  -- Where the lists stand is not what this session's clicks left: the preset clicked was another.
  local function notWhereLeft(now)
    S.forgetBrowserAt(ctx.window, "the preset read after the click is " .. tostring(now) .. ", not " .. pname)
  end

  -- After Select and Load: the strip and the macros read with their text (the tool's
  -- _refresh_osc_panel_from_screen), and with FAV on the stars of the rows shown once more — off
  -- picture `s` (taken over) where the caller has one from after the click, else off one taken
  -- after those reads. The browser's part with them, or nil where the trip has left Avenger.
  local function refreshed(s)
    local part = browserPart(nil)
    local states, starred = nil, false
    -- The stars of the rows shown, off picture `pic`, which is let go of.
    local function stars(pic)
      local rows = Br.shownRows(pl, ctx.window, expansion, lists)
      if rows and trip:inFront() then Br.stars(pic, pl, rows) end
      pic:release()
      states, starred = rows and starsOver(rows, known, expansion) or nil, true
    end
    if s then
      if fav then stars(s) else s:release() end
      if not trip:inFront() then return nil end
    end
    local function read(name, text)
      local v, keys = Act.partBefore(pl, name, text, function(fmt, ...) log(fmt, ...) end)
      if v then part = merged(part, { keys = keys, values = v }) end
    end
    read("strip", true)
    if not trip:inFront() then return nil end
    read("macros", true)
    if not trip:inFront() then return nil end
    if fav and not starred then
      local p = picture(ctx)
      if p then
        stars(p)
        if not trip:inFront() then return nil end
      end
    end
    if starred then part.values.favoriteStates = states end
    return part
  end

  -- Load: the name watched as after ◀ and ▶, then the refresh.
  local function afterName(now, before, changed)
    -- The watch's end comes on a later turn: the strip and the macros are read only on Avenger.
    if not trip:inFront() then return end
    if now == nil then return trip:done({ part = browserPart(nil), error = "the preset name cannot be read now" }) end
    local part = refreshed(nil)
    if not part then return end
    local loaded = loadedIn(now, expansion)
    part = merged(part, { keys = { "loadedIndex" }, values = { loadedIndex = loaded } })
    local result = { set = E.named(now), part = part }
    if not changed and before ~= nil and now == before then
      result.note = "unchanged, " .. now
      log("'%s': the name stayed %s after the click", label, now)
    elseif loaded == target - 1 then
      log("'%s': the name read is %s — confirmed", label, now)
    else
      notWhereLeft(now)
      result.error = "VPS Avenger shows " .. now .. " where " .. pname .. " lies by its place in " .. cat
      log("'%s': the name read is %s, not %s — not the place asked", label, now, pname)
    end
    trip:done(result)
  end

  local function act(pic)
    local point, star = Pg.presetPoint(place.row, place.adjust)
    local x, y = pl:point(point[1], point[2])
    local row = { index = target, y = y }
    if kind == "mark" then
      local sx = pl:point(star[1], star[2])
      Br.stars(pic, pl, { row })
      local before = row.star
      log("'%s': clicking %s's star at (%d,%d), the tool's row %d of page %d of %s", label, pname, sx, y, place.row + 1,
        place.page + 1, cat)
      local ok, said = E.ring.clickPoint(o, label, sx, y, nil, true)
      if not ok then
        pic:release()
        return trip:done({ sentence = said })
      end
      -- The star's box on screen, the change wait's watch.
      local k = pl.k
      local box = { math.floor(sx - X.FAVORITE_STATE[1] / 2 * k), math.floor(y - X.FAVORITE_STATE[2] / 2 * k),
        math.ceil(sx + X.FAVORITE_STATE[1] / 2 * k), math.ceil(y + X.FAVORITE_STATE[2] / 2 * k) }
      local deadline = host.now() + CHANGE_WITHIN
      local function round(from)
        waitFrom(trip, ctx, from, { box }, math.max(1, deadline - host.now()), 0, function(s, changed)
          if not s then return trip:done({ part = browserPart(nil), error = "the star cannot be read now" }) end
          local again = { index = target, y = y }
          Br.stars(s, pl, { again })
          if not trip:inFront() then
            s:release()
            return
          end
          if changed and again.star ~= nil and again.star == before and deadline - host.now() >= 1 then
            log("the picture changed and the star did not — looking on from that picture")
            return round(s)
          end
          s:release()
          local part = browserPart(nil)
          if again.star == nil then
            return trip:done({ part = part, error = "the star cannot be read now" })
          end
          part.values.favoriteStates = starsOver({ again }, known, expansion)
          local words = again.star and "a favorite" or "not a favorite"
          if before ~= nil and again.star == before then return trip:done({ part = part, note = "unchanged, " .. words }) end
          if not changed then return trip:done({ part = part, note = "unchanged, " .. words }) end
          trip:done({ part = part, said = pname .. ", " .. words })
        end)
      end
      return round(pic)
    end
    if kind == "select" then
      -- A preview loads nothing (above): nothing is forgotten, and no name is watched for. The
      -- presets' list is watched from the picture before the click, Avenger drawing the row
      -- selected — a bound on looking, not a decision: unchanged at it (the row selected already),
      -- the refresh is made all the same, and nothing is said.
      log("'%s': clicking %s at (%d,%d), the tool's row %d of page %d of %s", label, pname, x, y, place.row + 1,
        place.page + 1, cat)
      local ok, said = E.ring.clickPoint(o, label, x, y, nil, true)
      if not ok then
        pic:release()
        return trip:done({ sentence = said })
      end
      return waitFrom(trip, ctx, pic, onScreen(ctx, { X.PRESET_TEXT }), CHANGE_WITHIN, B.SETTLE, function(s, changed)
        log("'%s': %s after the click", label, changed and "the presets' list changed"
          or "nothing in the presets' list changed")
        local part = refreshed(s)
        if part then trip:done({ part = part }) end
      end)
    end
    -- A double-click on a row loads the preset.
    forget("the page's " .. label)
    log("'%s': double-clicking %s at (%d,%d), the tool's row %d of page %d of %s", label, pname, x, y, place.row + 1,
      place.page + 1, cat)
    pic:release()
    local ok, said = E.ring.doublePoint(o, label, x, y, true, function(okD, saidD)
      if okD then return E.ring.watchName(o, was, afterName, label) end
      if trip:inFront() then trip:done({ sentence = saidD }) end
    end)
    if not ok then return trip:done({ sentence = said }) end
  end

  -- Its page: from where this session's clicks left the category's list, the tool's clicks.
  local function toPage(pic)
    -- With no window to keep it by, what the category's click just did.
    local at = S.browserAt(ctx.window) or { expansion = expansion, category = cat, presets = { page = 0, first = 0 } }
    local clicks = Pg.presetScroll(at.presets, place, #members)
    if #clicks > 0 then
      log("'%s': %s is the %d of %d in %s — page %d, row %d; %d click(s) on the presets' scrollbar", label, pname,
        within, #members, cat, place.page + 1, place.row + 1, #clicks)
    end
    clickSeries(o, trip, ctx, "presets", pic, clicks, label, function(s, sentence)
      if sentence then return trip:done({ sentence = sentence }) end
      if not s then return trip:done({ error = "VPS Avenger's browser cannot be read now" }) end
      at.presets = { page = place.page, first = place.first, final = place.final }
      S.setBrowserAt(ctx.window, at)
      act(s)
    end)
  end

  local at = S.browserAt(ctx.window)
  if at and at.expansion == expansion and at.category == cat and at.presets then return toPage(ctx.snap) end
  -- The category Avenger shows is not known from this session's clicks: its category first.
  log("'%s': %s is in %s, which this session's clicks did not leave shown — its category first", label, pname, cat)
  clickCategory(o, trip, ctx, label, expansion, lists, cat, ctx.snap, function(s, _, sentence, refusal)
    if sentence then return trip:done({ sentence = sentence }) end
    if refusal then return trip:done({ error = refusal }) end
    if not s then return trip:done({ error = "VPS Avenger's browser cannot be read now" }) end
    toPage(s)
  end)
end

-- select_all_expansion_presets: no click — the name read, FAV, and the page's category "every one".
function B.all(o, trip, label, known)
  local name = E.ring.nameNow(o)
  if not trip:inFront() then return end
  local expansion = known.selectedExpansion
  local ctx = look(o, trip, expansion)
  if not ctx then return end
  local st = ctx.st
  local why = notOpen(st, expansion)
  if why then
    ctx.snap:release()
    gate(ctx, label, "nothing clicked — " .. why)
    return trip:done({ set = E.named(name), part = partOf(st, known), error = why })
  end
  gate(ctx, label, "no click")
  local fav = Br.fav(ctx.snap, ctx.pl)
  local v = { pageCategory = B.ALL, favoriteView = fav, loadedIndex = loadedIn(name, expansion) }
  if fav and known.lists then
    local rows = Br.shownRows(ctx.pl, ctx.window, expansion, known.lists)
    if rows then
      Br.stars(ctx.snap, ctx.pl, rows)
      v.favoriteStates = starsOver(rows, known, expansion)
    end
  end
  ctx.snap:release()
  trip:done({ set = E.named(name), part = { keys = { "pageCategory", "favoriteView", "loadedIndex", "favoriteStates" },
    values = v }, focus = "expansion-category-all" })
end

-- go_to_loaded_preset_category: no trip, as the tool's — the category the database has for the name
-- last read, in the expansion open on the page. The page's category and the preset placed, or nil
-- and why (the tool's five reasons).
function B.goToCategory(known)
  if not DB.present then return nil, "the preset database is not installed" end
  local name = known and known.preset
  if not name then return nil, "the preset's name has not been read" end
  local ok, p = pcall(DB.place, name)
  if not (ok and p) then return nil, "the preset loaded is not in the database" end
  local open = known.selectedExpansion
  if open and open ~= "" and open ~= p.expansion then
    return nil, "the preset loaded is in another expansion, " .. p.expansion
  end
  local lists = open == p.expansion and known.lists or nil
  if not lists then
    local okL, l = pcall(DB.lists, p.expansion)
    lists = okL and l or nil
  end
  local cat = lists and lists.presetCategories[p.index]
  if not cat or cat == "" then return nil, "the preset loaded has no category in the database" end
  log("'Go to Category': %s is in %s's %s, from the database", name, p.expansion, cat)
  return { pageCategory = cat, loadedIndex = p.index - 1 }
end

-- ── Correct open Expansion ───────────────────────────────────────────────────

local CANNOT_TELL = "whether an expansion is open in VPS Avenger, and which, cannot be told now"

-- align_open_expansion: no click. The name read first, as All reads it (the tool placed the preset
-- loaded in what it found); then ONE picture of the browser, its header read and its picture tested
-- with no expansion expected — so an expansion the page has open by its place is not taken for the
-- one shown unless the header reads its name. The tool read the area's words twice, 0.45 s apart, and
-- scored its roster by them; nothing below the header is read here. Posted: what the read says; with
-- an expansion named, its lists and FAV, the page's own category only where the expansion is another
-- than the page had. Nothing settled: every value of the browser's posted as not read, never kept.
function B.align(o, trip, label, known)
  local name = E.ring.nameNow(o)
  if not trip:inFront() then return end
  local ctx = look(o, trip, nil)
  if not ctx then return end
  local st, s, pl = ctx.st, ctx.snap, ctx.pl
  local set = E.named(name)
  gate(ctx, label, "no click")
  if not st then
    s:release()
    return trip:done({ set = set, part = { keys = KEYS, values = {} }, error = CANNOT_TELL })
  end
  if st.open == false then
    s:release()
    return trip:done({ set = set, part = partOf(st, known), note = NONE_OPEN })
  end
  if not st.name then
    s:release()
    return trip:done({ set = set, part = { keys = KEYS, values = { expansionOpen = true, selectedExpansion = "" } },
      error = "VPS Avenger shows an expansion open; which one cannot be read in its header now" })
  end
  local y = st.name
  local had = known.expansionOpen == true and known.selectedExpansion ~= "" and known.selectedExpansion or nil
  local marks = {}
  local fav = Br.fav(s, pl, marks)
  local v = { expansionOpen = true, selectedExpansion = y, favoriteView = fav, loadedIndex = loadedIn(name, y) }
  local keys = { "expansionView", "expansionViewFrom", "expansionOpen", "selectedExpansion", "lists", "loadedIndex",
    "favoriteView", "favoriteStates" }
  local okL, lists = pcall(DB.lists, y)
  if okL and lists then v.lists = lists end
  if y ~= had then
    S.forgetBrowserAt(ctx.window, "Correct open Expansion reads " .. y)
    v.pageCategory = nil
    keys[#keys + 1] = "pageCategory"
    log("'%s': the page had %s; the header reads %s", label, had or "no expansion open", y)
  elseif fav then
    local rows = v.lists and Br.shownRows(pl, ctx.window, y, v.lists)
    if rows then
      Br.stars(s, pl, rows, marks)
      v.favoriteStates = starsOver(rows, known, y)
    end
  end
  Br.picture(string.format("browser-open-%dpct", pl.zoom or 0), s, ctx.region, marks)
  s:release()
  trip:done({ set = set, part = { keys = keys, values = v }, note = (y == had and "unchanged, " or "corrected to ") .. y })
end

-- What actions.luau hands over: { ring = main.luau's helpers, named = (name) -> the preset's set,
-- plugin = () -> the window the page was shown beside, shown = () -> the list of expansions last
-- posted }.
function B.start(env)
  E = env
end

return B
