-- The expansion browser's actions (the plan's slice 9), for the page's trips: by the avenger_control
-- tool's names and arguments (its actions.py), the browser read off a picture of it
-- (src/read/browser.luau) before every click and after it, and every click on the words READ — the
-- middle of a tile's or a row's words, a row's star at its own height — never on the tool's table
-- point for an index. The tool clicked its tables' points for the page it believed the grid on, and
-- scrolled by its tables measured for one roster and one set of category sizes; it then wrote the
-- state it assumed: the view chosen, the expansion opened, the first category selected, the preset
-- loaded (its name taken from the database). Here:
--
--   set_expansion_view {view}   through src/act.luau: the view field opened with the LEFT button, the
--                         view's row chosen in its painted list (popups.luau, Pp.VIEW_LIST), and the
--                         view stored as set once the list took it — the tool's AVGExpansionView —
--                         since this Avenger draws no view's label to read (src/read/browser.luau,
--                         Br.view); nothing clicked where an expansion reads open — the same point
--                         would close it — or where nothing tells whether one is, nor where a view's
--                         label reads the one asked
--   confirm_expansion_view {view}   no trip: the view stored as confirmed (the tool's
--                         action_confirm_expansion_view), nothing clicked
--   close_expansion       through src/act.luau: the field clicked only where the header reads an
--                         expansion open — the tool clicked it whatever was shown, and with the grid
--                         shown that click opens the view list; then the header read again
--   toggle_expansion_favorite_view {enabled}   through src/act.luau: FAV clicked, its pixels read
--                         before and after, and with FAV on the rows' stars
--   open_expansion {index, page_start, visible_index}   the grid read; the name looked for in it, the
--                         list moved a step at a time where it is not there (B.seek); its words
--                         clicked; then the columns read, and the expansion they list scored (the
--                         tool's page position is only the first direction to look in)
--   select_expansion_category {index, total}   the categories read and looked for as the grid is, the
--                         row clicked, then the preset rows read and placed in the database's lists
--   select_all_expansion_presets   no click: the name and the rows read (the tool read the name)
--   go_to_loaded_preset_category   no trip: the database's category for the name last read, as the
--                         tool's, marked as the database's on the page (B.goToCategory)
--   select_expansion_preset, load_expansion_preset, mark_expansion_preset_favorite {preset_index, …}
--                         the rows read; another category's, the target's category found and clicked
--                         first; the row looked for; then a click, a double-click, or a click on its
--                         star — the name watched after the first two, the star read after the third
--   read_expansion_list   the port's own: Avenger's grid read from its top to its end, kept for the
--                         session (S.avengerList) and shown in place of the database's roster
--   align_open_expansion  Correct open Expansion (the plan's slice 10): no click — the name and ONE
--                         picture of the caption, the header's name and the columns read with no
--                         expansion expected, so the one the page had cannot score first; what that
--                         does not settle is "cannot be told now" (the tool read twice, 0.45 s apart,
--                         over its roster)
--
-- MOVING A LIST (B.step, B.seek). The maintainer's decision (TODO.md, stage 4): the lists are read and
-- clicked by the word and moved by the wheel, measured. A step is one wheel notch at the list's
-- middle, then a change wait from the picture last read, watching the list's top and bottom quarter
-- bands — away from the pointer in its middle, whose hover over a tile is not the list moving — with
-- a still spell of SETTLE, so a list that scrolls smoothly is answered once it stands still. Where a
-- notch moves nothing while the way is not known yet, one click on the scrollbar's track toward the
-- step is tried, the tool's measured way, and whichever moved the list is kept for the session
-- (`means`); where both move nothing, the list has stopped moving that way. Only where the list's
-- order is known — Avenger's list once read, an expansion's categories and presets; never the
-- roster, whose order against Avenger's nobody has measured (TODO.md, D17) — do two steps running
-- whose names went the other way turn that list's wheel round for the session, once a search (a
-- Mac's natural scrolling may reverse an injected wheel: nobody has measured it). A search reads
-- after every step until the name is read or the list stops, then goes the other way from where it
-- is. Not read at either end, the name is said not to be in the list only where the list moved and
-- the name's place in an order known lies away from the places read; otherwise it is a name that
-- cannot be read now, and a list that did not move is said as that. No timer decides anything: every
-- answer is a change wait's, bounded by STEP_WITHIN, and the search by GUARD steps a way, which only
-- a picture that never stands still reaches.
--
-- Nothing is taken for done: what the browser shows after a click is what is posted, and where it was
-- not read it is said as that. The database names an open expansion by what its columns list and
-- places the rows in its lists, and says so ("from the database"); it never decides that a click did
-- something.

local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local X = host.include("src/tables/expansion.luau")
local Br = host.include("src/read/browser.luau")
local Open = host.include("src/read/open.luau")
local S = host.include("src/state.luau")
local DB = host.include("src/presets.luau")
local Drums = host.include("src/actions/drums.luau")
local Act = host.include("src/act.luau")

local B = {}

local E = nil -- what actions.luau hands over (B.start)

local function log(fmt, ...)
  host.log.info(("[avenger] [browser] " .. fmt):format(...))
end

-- How long a step's change wait looks for the list to move, at most — the longest one change wait may
-- wait (docs/api/screen.md) — and how long it must have stood still to count: one or two of Avenger's
-- frames, the host's still spell (docs/api/screen.md, `settle`). Starting values (TODO.md, D13).
B.STEP_WITHIN = 2000
B.SETTLE = 60
-- How long a click's change is looked for: as src/act.luau's.
local CHANGE_WITHIN = 2000
-- Steps a way a search takes at most: a bound on looking, as MENU_LOOK_MS is the runtime's.
B.GUARD = 256

-- The page's word for "every category" (the tool's _ALL_PRESET_CATEGORY).
B.ALL = "__all__"

-- The page's words.
local OPEN_NOW = "an expansion is open in VPS Avenger; press Close Expansion to close it first"
local NONE_OPEN = "no expansion is open in VPS Avenger"
local VIEW_UNREAD = "whether an expansion is open in VPS Avenger cannot be read now"

-- Every value of the browser's (state.luau's): a read that shows the grid, or another expansion,
-- says each of them.
local KEYS = { "expansionView", "expansionViewFrom", "expansionOpen", "selectedExpansion", "lists", "loadedIndex",
  "favoriteView", "favoriteStates", "categoryShown", "pageCategory", "selectedFromDatabase" }

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
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

function B.set_expansion_view(args, label)
  local view = args.view
  local at = type(view) == "string" and X.viewMenuPoint(view)
  if not at then return nil, tostring(view) .. " is not one of VPS Avenger's views" end
  return {
    label = label,
    part = "browser",
    fields = { "area" },
    decide = function(vb)
      if vb.expansionOpen == true then return nil, OPEN_NOW end
      if vb.expansionOpen == nil then return nil, VIEW_UNREAD end
      -- A view's label read as the one asked; a view stored is clicked all the same, as the tool did.
      if vb.expansionView == view and vb.expansionViewFrom == nil then return nil, "unchanged, Expansion View " .. view end
      return { at = X.VIEW, menu = { button = "left", item = at, retries = 3, read = false } }
    end,
    answer = function(_, _, how)
      if how ~= "chosen" then return { error = "the view was not chosen" } end
      Br.storeView(view, "set")
      return { part = { keys = { "expansionView", "expansionViewFrom" }, values = { expansionView = view,
        expansionViewFrom = "set" } }, said = viewWords(view, "set") }
    end,
  }
end

-- confirm_expansion_view {view}: the page's Confirm Expansion View — no trip, no click.
function B.confirmView(args)
  local view = args.view
  if not (type(view) == "string" and X.viewMenuPoint(view)) then
    return nil, tostring(view) .. " is not one of VPS Avenger's views"
  end
  Br.storeView(view, "confirmed")
  return { expansionView = view, expansionViewFrom = "confirmed" }
end

function B.close_expansion(_, label, known)
  return {
    label = label,
    part = "browser",
    fields = { "area" },
    expected = known and known.selectedExpansion or nil,
    decide = function(vb)
      -- With the grid shown, this point opens the view list: the tool clicked it all the same.
      if vb.expansionOpen == false then return nil, NONE_OPEN end
      if vb.expansionOpen ~= true then return nil, VIEW_UNREAD end
      return { at = X.CLOSE }
    end,
    watch = function() return { X.VIEW_TEXT, X.NAME_TEXT, X.AREA } end,
    same = function(_, va) return va.expansionOpen ~= false end,
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
    decide = function(vb)
      if vb.expansionOpen == false then return nil, NONE_OPEN end
      if vb.expansionOpen ~= true then return nil, VIEW_UNREAD end
      if want ~= nil and vb.favoriteView == want then return nil, "unchanged, FAV " .. (want and "ON" or "OFF") end
      return { at = X.FAVORITE_VIEW }
    end,
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

-- Where the browser is on overlay `o` now: { pl = the placer, region = its picture's rectangle },
-- or nil and the page's error.
local function placed(o)
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return nil, "VPS Avenger's size cannot be told now" end
  local pl = P.placer(origin, k, (H.zoom(origin)))
  local region = Open.region(pl, X.BROWSER)
  if not region then return nil, "that part of VPS Avenger is not on the screen" end
  return { pl = pl, region = region }
end

-- One picture of the browser, waited for: the snapshot, or nil and the page's error.
local function picture(ctx)
  local ok, snap, why = pcall(host.screen.snapshot, { region = ctx.region })
  if ok and snap then return snap end
  log("no picture of the browser: %s", tostring(ok and why or snap))
  return nil, "VPS Avenger cannot be read now"
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

-- ── Moving a list ────────────────────────────────────────────────────────────

-- How each list moves this session, once seen: "wheel" or "track"; nil while not known. And each
-- list's wheel, 1 or -1, turned round where the names read showed it moving the other way.
local means = {}
local sign = { grid = 1, categories = 1, presets = 1 }
local LIST_WORDS = { grid = "grid of expansions", categories = "list of categories", presets = "list of presets" }

-- The top and bottom quarter of list `list`'s region, the bands a step watches.
local function bands(list)
  local r = X.LISTS[list].rect
  local q = (r[4] - r[2]) / 4
  return { { r[1], r[2], r[3], r[2] + q }, { r[1], r[4] - q, r[3], r[4] } }
end

-- One step of list `list` ("grid", "categories", "presets") toward `dir` ("down": toward later entries;
-- "up"), from picture `from`, which it takes over: the wheel, or the track where the wheel moved
-- nothing (above). `done(picture or nil, moved, sentence?)` with the picture after it — the one the
-- list stood still in — or nil where none came; with the sentence where its point could not be
-- clicked. Nothing where the trip ended meanwhile.
function B.step(o, trip, ctx, list, dir, from, label, done)
  local L = X.LISTS[list]
  local watch = onScreen(ctx, bands(list))
  local t0 = host.now()
  local function track(pic, after)
    local at = { L.track.x, dir == "down" and L.track.down or L.track.up }
    local ok, said = E.ring.clickAt(o, label, at, nil, true)
    if not ok then
      pic:release()
      return done(nil, false, said)
    end
    waitFrom(trip, ctx, pic, watch, B.STEP_WITHIN, B.SETTLE, after)
  end
  local function said(s, moved, how)
    log("step %s in the %s: %s, %d ms", dir, LIST_WORDS[list], moved and ("moved by the " .. how) or how,
      host.now() - t0)
    done(s, moved)
  end
  if means[list] == "track" then
    return track(from, function(s, changed)
      if not s then return done(nil, false) end
      said(s, changed, changed and "track" or "unchanged by the track — at its end")
    end)
  end
  local ok, why = E.ring.wheelAt(o, label, L.wheel, (dir == "down" and -1 or 1) * sign[list], true)
  if not ok then
    from:release()
    return done(nil, false, why)
  end
  waitFrom(trip, ctx, from, watch, B.STEP_WITHIN, B.SETTLE, function(s, changed)
    if not s then return done(nil, false) end
    if changed then
      if means[list] == nil then
        means[list] = "wheel"
        log("the %s moves by the wheel — the wheel from here on", LIST_WORDS[list])
      end
      return said(s, true, "wheel")
    end
    if means[list] == "wheel" then return said(s, false, "unchanged by the wheel — at its end") end
    track(s, function(s2, changed2)
      if not s2 then return done(nil, false) end
      if changed2 then
        means[list] = "track"
        log("the wheel moved nothing in the %s; its scrollbar's track did — the track from here on", LIST_WORDS[list])
        return said(s2, true, "track")
      end
      said(s2, false, "unchanged by the wheel and the track — at its end")
    end)
  end)
end

-- An entry found by its name in list `spec.list`, moved a step at a time where it is not read: `spec`
-- = { list, label, pic = the picture read last (taken over), entries = what it read, read(s) ->
-- entries | nil (s a picture after a step; nil: not read), match(entry) -> boolean, place(entry) ->
-- its place in the list's order or nil, target = the place looked for, or nil, ordered = true where
-- that order is Avenger's own (read at any time while the search runs), hint = "down" | "up" | nil,
-- said = a sentence said, behind the screen reader, at the first step, onFound(entry, picture,
-- entries), onNotFound(word, picture or nil, sentence?) }. `word`, both ends reached: "absent" (the list
-- moved, and the target's place is not in an order known or lies away from the places read — more
-- than one place beyond them), "missed" (the list moved, and the order puts the target among the
-- places read or next to them: a name not read), "still" (the list moved neither way); or "guard"
-- (GUARD steps a way), "unread", "refused" (with the sentence of the point refused). The direction:
-- toward the target's place against the places read; where none is read, the hint; else down. In an
-- order known, two steps running whose places went the other way turn the list's wheel round for the
-- session, once a search, and the search starts again from where the list is.
function B.seek(o, trip, ctx, spec)
  local pic, entries = spec.pic, spec.entries
  local dir, turned, steps, told = nil, false, 0, false
  local moved, contrary, flipped = false, 0, false
  local seenLo, seenHi = nil, nil
  local function find(list)
    for _, e in ipairs(list) do
      if spec.match(e) then return e end
    end
    return nil
  end
  local function span(list)
    local lo, hi = nil, nil
    for _, e in ipairs(list) do
      local p = spec.place(e)
      if p then
        lo = lo and math.min(lo, p) or p
        hi = hi and math.max(hi, p) or p
      end
    end
    return lo, hi
  end
  -- The places read so far, over the whole walk.
  local function saw(list)
    local lo, hi = span(list)
    if lo then
      seenLo = seenLo and math.min(seenLo, lo) or lo
      seenHi = seenHi and math.max(seenHi, hi) or hi
    end
  end
  saw(entries)
  -- Why the target was not read once both ends were reached.
  local function ended()
    if not moved then return "still" end
    local t = spec.target
    if spec.ordered and t and seenLo and t >= seenLo - 1 and t <= seenHi + 1 then return "missed" end
    return "absent"
  end
  local go
  go = function()
    local e = find(entries)
    if e then return spec.onFound(e, pic, entries) end
    if dir == nil then
      local lo, hi = span(entries)
      local why
      if spec.target and lo and spec.target > hi then
        dir, why = "down", string.format("its place %d lies after the %d to %d read", spec.target, lo, hi)
      elseif spec.target and lo and spec.target < lo then
        dir, why = "up", string.format("its place %d lies before the %d to %d read", spec.target, lo, hi)
      elseif spec.hint then
        dir, why = spec.hint, "the first place to look"
      else
        dir, why = "down", "nothing to go by"
      end
      log("'%s': not read in the %s — looking %s (%s)", spec.label, LIST_WORDS[spec.list], dir, why)
    end
    if steps >= B.GUARD then
      log("'%s': still moving after %d steps — nothing more was looked at", spec.label, B.GUARD)
      return spec.onNotFound("guard", pic)
    end
    if spec.said and not told then
      told = true
      host.speech.output(spec.said, { interrupt = false })
    end
    steps += 1
    local before = span(entries)
    B.step(o, trip, ctx, spec.list, dir, pic, spec.label, function(s, didMove, sentence)
      pic = s
      if sentence then return spec.onNotFound("refused", nil, sentence) end
      if not s then return spec.onNotFound("unread", nil) end
      if not didMove then
        if turned then return spec.onNotFound(ended(), pic) end
        turned, steps = true, 0
        dir = dir == "down" and "up" or "down"
        log("'%s': the %s moves no further — looking %s", spec.label, LIST_WORDS[spec.list], dir)
        return go()
      end
      moved = true
      local next = spec.read(s)
      if not trip:inFront() then
        s:release()
        return
      end
      if not next then return spec.onNotFound("unread", s) end
      saw(next)
      entries = next
      local after = span(next)
      -- The wheel turned round only by an order known, and only by two steps running that went the
      -- other way: one misread row placed far off is not the list moving back.
      if spec.ordered and means[spec.list] == "wheel" and before and after and after ~= before then
        contrary = (dir == "down") ~= (after > before) and contrary + 1 or 0
        if contrary >= 2 and flipped then
          contrary = 0
          log("'%s': the places read in the %s went the other way again — the wheel not turned round twice in one "
            .. "search", spec.label, LIST_WORDS[spec.list])
        elseif contrary >= 2 then
          flipped, contrary = true, 0
          sign[spec.list] = -sign[spec.list]
          log("'%s': the wheel moved the %s %s two steps running, where it was turned %s (the places read %d, then %d) — "
            .. "turned round for the session, and the search started again from here", spec.label, LIST_WORDS[spec.list],
            dir == "down" and "up" or "down", dir, before, after)
          -- The ends met so far were met the other way round: the walk starts again from here.
          dir, turned, steps = nil, false, 0
        end
      end
      go()
    end)
  end
  go()
end

-- ── What the reads say ───────────────────────────────────────────────────────

-- The values a read of the browser says, from its state `st` (Br.state's): the grid — every open
-- expansion's value gone; an expansion open — the one the read names, with its lists from the database
-- where it is another than the one `known` had (marked as named by the database), or none. And the
-- keys they are posted under.
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
    v.selectedFromDatabase = true
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

-- The preset rows of reading `r` placed in `lists`, with their stars off picture `s` where FAV reads
-- on (`fav`); and the category they read as.
local function presetRows(s, pl, r, lists, fav, marks)
  local lines = Br.lines(Br.rows(r), lists.presets, lists.presetCategories, "preset rows")
  if fav then Br.stars(s, pl, lines, marks) end
  return lines, Br.categoryOf(lines, lists.presetCategories)
end

-- The words for a search that did not find `name` in VPS Avenger's `list` ("list", "list of
-- categories", "list of <category>"), by B.seek's word: "is not in" only for a list that moved and
-- an order that does not put the name where the list was read; a name not read where it should have
-- been, or in a list that did not move, is said as not read.
local function notFound(word, name, list)
  if word == "guard" then return name .. " cannot be found now" end
  if word == "unread" then return "VPS Avenger's browser cannot be read now" end
  if word == "still" then return name .. " was not read, and VPS Avenger's " .. list .. " did not move" end
  if word == "missed" then return name .. " cannot be read in VPS Avenger's " .. list .. " now" end
  return name .. " is not in VPS Avenger's " .. list
end

-- The stars read in `lines` over the ones `known` read with FAV on in the same expansion.
local function starsOver(lines, known, expansion)
  local before = known.selectedExpansion == expansion and known.favoriteView and known.favoriteStates or nil
  return Br.starStates(lines, before)
end

-- The calibration picture of the grid in view `view`, marked at every tile's point and where a step
-- turns the wheel and clicks the track.
local function gridPicture(s, ctx, view, tiles)
  local marks = {}
  for _, t in ipairs(tiles) do marks[#marks + 1] = { x = t.x, y = t.y } end
  local L = X.LISTS.grid
  for _, p in ipairs({ L.wheel, { L.track.x, L.track.up }, { L.track.x, L.track.down } }) do
    local x, y = ctx.pl:point(p[1], p[2])
    marks[#marks + 1] = { x = x, y = y }
  end
  Br.picture(string.format("browser-grid-%s-%dpct", (string.gsub(string.lower(view), " ", "-")), ctx.pl.zoom or 0), s,
    ctx.region, marks)
end

-- The calibration picture of an open expansion, marked at the rows, the stars and FAV.
local function openPicture(s, ctx, lines, marks)
  local list = table.clone(marks or {})
  for _, e in ipairs(lines or {}) do list[#list + 1] = { x = e.x, y = e.y } end
  Br.picture(string.format("browser-open-%dpct", ctx.pl.zoom or 0), s, ctx.region, list)
end

-- ── The trips ────────────────────────────────────────────────────────────────

-- The first look of a trip: the browser pictured and read for `which` (Br.read's names), its state
-- with `expected`; the picture held. nil where the trip ended or is done (`trip:done` called).
local function look(o, trip, which, expected, label)
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
  local byName = Br.read(snap, ctx.pl, which)
  if not trip:inFront() then
    snap:release()
    return nil
  end
  local st, words = Br.state(byName, expected)
  log("'%s': %s", label, words)
  ctx.snap, ctx.byName, ctx.st = snap, byName, st
  return ctx
end

-- An expansion of Avenger's own list's that a read of the grid does not have (`tiles`), or nil.
local function foreign(tiles)
  if not S.avengerList then return nil end
  local names = Br.names()
  for _, t in ipairs(tiles) do
    if t.how ~= "as read" and not names.own[t.key] then return t.name end
  end
  return nil
end

local function droppedWords(name)
  return "VPS Avenger lists " .. name .. ", which this session's list does not have; the database's list is shown "
    .. "again — read Avenger's list again"
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

-- open_expansion: in trip `trip`'s act on overlay `o`, a handler where reads wait, and `trip:inFront()`
-- after every wait — false has ended the trip, and its give-up says the rest.
function B.open(o, trip, args, name, label, known)
  local ctx = look(o, trip, { "caption", "name", "area" }, known.selectedExpansion, label)
  if not ctx then return end
  local st, pl = ctx.st, ctx.pl
  local function refuse(why, part)
    ctx.snap:release()
    log("'%s': nothing clicked — %s", label, why)
    trip:done({ part = part, error = why })
  end
  if not st then return refuse(VIEW_UNREAD) end
  if st.open then return refuse(OPEN_NOW, partOf(st, known)) end
  -- The view the words are placed by is the one its label reads now, or else the one stored — set
  -- from the page or confirmed there (read/browser.luau, Br.view): a grid in another view would split
  -- its names across the wrong columns, which is why the page says how the view came to be.
  local view = st.view
  if not view then return refuse(VIEW_UNREAD, partOf(st, known)) end
  local tiles = Br.tiles(ctx.byName["browser area"], view, pl)
  if Br.named(tiles) == 0 then return refuse("the names in VPS Avenger's browser cannot be read now", partOf(st, known)) end
  local dropped = nil
  local spec = nil
  local function dropOn(list)
    local x = foreign(list)
    if x then
      dropped = x
      S.dropAvengerList()
      -- The order the search goes by is no longer Avenger's own.
      if spec then spec.ordered = false end
      log("'%s': the grid shows %s, which Avenger's list read this session does not have — the list dropped", label, x)
    end
  end
  dropOn(tiles)
  -- The order the search goes by: Avenger's own list where it was read — an order known — else the
  -- list shown, the roster, which only says which way to look first: how Avenger orders the
  -- expansions against the catalog nobody has measured (TODO.md, D17).
  local order = {}
  local ordered = S.avengerList ~= nil
  local own = (S.avengerList and S.avengerList.names) or E.shown() or {}
  for i, n in ipairs(own) do
    local f = DB.fold(n)
    if order[f] == nil then order[f] = i end
  end
  local key = DB.fold(name)
  local start = whole(args.page_start)
  log("'%s': the tool's place, page start %s, tile %s — the first place to look is the grid as it stands", label,
    tostring(args.page_start), tostring(args.visible_index))
  local function finish(result)
    if dropped then
      if result.error or result.note then
        log("'%s': %s", label, droppedWords(dropped))
      else
        result.note = droppedWords(dropped)
      end
    end
    trip:done(result)
  end

  -- After the click: the caption, the header's name and the columns read, FAV, and the expansion the
  -- header names — or, where it names none, the one the columns list.
  local function readOpen(s)
    local byName = Br.read(s, pl, { "caption", "name", "categories", "presets" })
    local st2, words = Br.state(byName, name)
    log("'%s': after the click, %s", label, words)
    local r = { st = st2 }
    if st2 and st2.open then
      local marks = {}
      r.fav = Br.fav(s, pl, marks)
      local ok, lists = pcall(DB.lists, st2.name or name)
      if st2.name and ok and lists then
        r.lists = lists
        r.lines, r.category = presetRows(s, pl, byName["presets"], lists, r.fav, marks)
      end
      openPicture(s, ctx, r.lines, marks)
    end
    return r
  end
  local function opened(r, changed)
    local st2 = r and r.st
    if not st2 then return finish({ error = "VPS Avenger's browser cannot be read now" }) end
    if st2.open == false then
      log("'%s': the grid still shown (%s)", label, changed and "the picture changed" or "unchanged")
      return finish({ part = partOf(st2, known), error = "nothing VPS Avenger shows changed" })
    end
    if not st2.name then
      return finish({ part = partOf(st2, known), error = DB.present and "VPS Avenger shows an expansion open; which one "
        .. "cannot be read now" or ("VPS Avenger shows an expansion open where " .. name .. " was clicked; which one "
        .. "cannot be read without the preset database") })
    end
    local v = { expansionOpen = true, selectedExpansion = st2.name, lists = r.lists, favoriteView = r.fav,
      categoryShown = r.category, pageCategory = r.category, loadedIndex = loadedIn(E.ring.lastName(), st2.name) }
    if r.fav and r.lines then v.favoriteStates = Br.starStates(r.lines) end
    if st2.name ~= name then
      v.selectedFromDatabase = st2.score ~= nil or nil
      return finish({ part = { keys = KEYS, values = v }, error = "VPS Avenger opened " .. st2.name
        .. (st2.score ~= nil and ", by the presets it lists (from the database)" or ", as its header reads") })
    end
    finish({ part = { keys = KEYS, values = v }, focus = "open-expansion-name" })
  end
  local function click(t, pic)
    log("'%s': clicking '%s' at (%d,%d), the middle of its words (read '%s', %s)", label, t.name, t.x, t.y, t.read, t.how)
    local ok, said = E.ring.clickPoint(o, label, t.x, t.y, nil, true)
    if not ok then
      pic:release()
      return finish({ sentence = said })
    end
    lookOn(trip, ctx, pic, { X.VIEW_TEXT, X.NAME_TEXT, X.AREA }, readOpen, function(r) return r.st ~= nil and r.st.open == true end,
      opened)
  end
  gridPicture(ctx.snap, ctx, view, tiles)
  spec = {
    list = "grid",
    label = label,
    pic = ctx.snap,
    entries = tiles,
    read = function(s)
      local b = Br.read(s, pl, { "area" })
      local t = Br.tiles(b["browser area"], view, pl)
      dropOn(t)
      return t
    end,
    match = function(t) return t.key == key end,
    place = function(t) return order[t.key] end,
    target = order[key],
    ordered = ordered,
    hint = start and (start > 0 and "down" or "up") or nil,
    said = "Looking for " .. name .. " in VPS Avenger's list.",
    onFound = click,
    onNotFound = function(word, pic, sentence)
      if pic then pic:release() end
      if sentence then return finish({ sentence = sentence }) end
      -- The name is one of Avenger's own list, still held: it is there, and was not read.
      if word == "absent" and spec.ordered then word = "missed" end
      log("'%s': %s not found (%s) — nothing clicked", label, name, word)
      finish({ error = notFound(word, name, "list") })
    end,
  }
  B.seek(o, trip, ctx, spec)
end

-- Category `target` (its place in `lists.categories`, from 1) found in Avenger's list of categories
-- from picture `pic` (taken over) and its categories read, `cats`, and clicked; then `next(byName,
-- changed, s)` with the picture after the click — held, the caller's — read for the caption and the
-- columns, looked on from while its preset rows do not read as the target's.
local function chooseCategory(o, trip, ctx, label, lists, target, pic, cats, next, onFail)
  local name = lists.categories[target]
  B.seek(o, trip, ctx, {
    list = "categories",
    label = label,
    pic = pic,
    entries = Br.placed(cats),
    read = function(s)
      local b = Br.read(s, ctx.pl, { "categories" })
      return Br.placed(Br.lines(Br.rows(b["categories"]), lists.categories, nil, "categories"))
    end,
    match = function(e) return e.index == target end,
    place = function(e) return e.index end,
    target = target,
    ordered = true,
    onFound = function(e, s)
      log("'%s': clicking '%s' at (%d,%d), the middle of its words (read '%s', %s)", label, e.name, e.x, e.y, e.read, e.how)
      local ok, said = E.ring.clickPoint(o, label, e.x, e.y, nil, true)
      if not ok then
        s:release()
        return onFail({ sentence = said })
      end
      lookOn(trip, ctx, s, { X.CATEGORY_TEXT, X.PRESET_TEXT }, function(s2)
        local b = Br.read(s2, ctx.pl, { "caption", "name", "categories", "presets" })
        local _, c = presetRows(s2, ctx.pl, b["presets"], lists, false)
        return { byName = b, category = c }
      end, function(r) return r.category == name end, function(r, changed, s2)
        if not r then return onFail({ error = "the presets VPS Avenger lists cannot be read now" }) end
        next(r.byName, changed, s2)
      end, true)
    end,
    onNotFound = function(word, s, sentence)
      if s then s:release() end
      if sentence then return onFail({ sentence = sentence }) end
      log("'%s': %s not found among the categories (%s) — nothing clicked", label, name, word)
      onFail({ error = notFound(word, name, "list of categories") })
    end,
  })
end

-- Why nothing is done in an open expansion, from the first look's state `st`: nil where the
-- expansion `expansion` (the one known open) reads open, or an expansion reads open and is not named.
local function notOpen(st, expansion)
  if not st then return VIEW_UNREAD end
  if st.open ~= true then return NONE_OPEN end
  if st.name and st.name ~= expansion then
    return "VPS Avenger shows another expansion, " .. st.name .. ", by the presets it lists (from the database)"
  end
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

-- select_expansion_category, as B.open: the categories read, the target looked for and clicked, then
-- the preset rows read and placed, which say whether VPS Avenger took it.
function B.category(o, trip, target, label, known)
  local lists, expansion = known.lists, known.selectedExpansion
  local name = lists.categories[target]
  local ctx = look(o, trip, { "caption", "name", "categories", "presets" }, expansion, label)
  if not ctx then return end
  local why = notOpen(ctx.st, expansion)
  if why then
    ctx.snap:release()
    log("'%s': nothing clicked — %s", label, why)
    return trip:done({ part = partOf(ctx.st, known), error = why })
  end
  local cats = Br.lines(Br.rows(ctx.byName["categories"]), lists.categories, nil, "categories")
  if #Br.placed(cats) == 0 then
    ctx.snap:release()
    return trip:done({ error = "the categories in VPS Avenger's browser cannot be read now" })
  end
  chooseCategory(o, trip, ctx, label, lists, target, ctx.snap, cats, function(byName, changed, s)
    local fav = Br.fav(s, ctx.pl)
    local lines, shown = presetRows(s, ctx.pl, byName["presets"], lists, fav)
    s:release()
    local v = { categoryShown = shown, favoriteView = fav, favoriteStates = fav and starsOver(lines, known, expansion) or nil }
    local keys = { "categoryShown", "favoriteView", "favoriteStates" }
    local result = { part = { keys = keys, values = v } }
    if shown == name then
      v.pageCategory = name
      keys[#keys + 1] = "pageCategory"
      return trip:done(result)
    end
    if shown == nil then
      result.note = name .. " chosen; which presets VPS Avenger lists cannot be read now"
      return trip:done(result)
    end
    log("'%s': the rows read as %s's after %s was clicked (%s)", label, shown, name, changed and "changed" or "unchanged")
    result.error = "VPS Avenger lists the presets of " .. shown .. " (by their names, from the database)"
    trip:done(result)
  end, function(result) trip:done(result) end)
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
-- ("mark"), as B.open: the rows read; where they are another category's (or none is placed), the
-- target's category found and clicked first; the row looked for; then its click, its double-click,
-- or its star's click. `forget` (actions.luau's go) is told before a click that may load a preset.
function B.preset(o, trip, forget, kind, arg, label, known)
  local lists, expansion = known.lists, known.selectedExpansion
  local target, cat = arg.target, arg.category
  local pname = lists.presets[target]
  local was = nil
  if kind ~= "mark" then
    was = E.ring.nameNow(o) or E.ring.lastName()
    if not trip:inFront() then return end
  end
  local ctx = look(o, trip, { "caption", "name", "categories", "presets" }, expansion, label)
  if not ctx then return end
  local pl = ctx.pl
  local fav = Br.fav(ctx.snap, pl)
  local why = notOpen(ctx.st, expansion)
  if not why and kind == "mark" then
    if fav == false then why = "the favorites view is off in VPS Avenger; turn FAV on first" end
    if fav == nil then why = "the favorites view cannot be read now" end
  end
  if why then
    ctx.snap:release()
    log("'%s': nothing clicked — %s", label, why)
    return trip:done({ part = partOf(ctx.st, known), error = why })
  end
  local page = arg.keep and B.ALL or cat
  local readRows = function(s)
    local b = Br.read(s, pl, { "presets" })
    return Br.placed(Br.lines(Br.rows(b["presets"]), lists.presets, lists.presetCategories, "preset rows"))
  end
  -- The browser's values after the row's action: the category shown, the page's, FAV and the stars.
  local function browserPart(lines)
    local v = { categoryShown = cat, pageCategory = page, favoriteView = fav }
    if fav then v.favoriteStates = starsOver(lines or {}, known, expansion) end
    return { keys = { "categoryShown", "pageCategory", "favoriteView", "favoriteStates" }, values = v }
  end

  -- Select and Load: the name watched as after ◀ and ▶, then the strip and the macros read with their
  -- text (the tool's refresh), and with FAV on the rows' stars once more.
  local function afterName(now, before, changed)
    -- The watch's end comes on a later turn: the strip and the macros are read only on Avenger.
    if not trip:inFront() then return end
    if now == nil then return trip:done({ part = browserPart(nil), error = "the preset name cannot be read now" }) end
    local part = browserPart(nil)
    local function read(name, text)
      local v, keys = Act.partBefore(pl, name, text, function(fmt, ...) log(fmt, ...) end)
      if v then part = merged(part, { keys = keys, values = v }) end
    end
    read("strip", true)
    if not trip:inFront() then return end
    read("macros", true)
    if not trip:inFront() then return end
    if fav then
      local s = picture(ctx)
      if s then
        local lines = readRows(s)
        if trip:inFront() then Br.stars(s, pl, lines) end
        s:release()
        if not trip:inFront() then return end
        part.values.favoriteStates = starsOver(lines, known, expansion)
      end
    end
    local loaded = loadedIn(now, expansion)
    part = merged(part, { keys = { "loadedIndex" }, values = { loadedIndex = loaded } })
    local note = nil
    if not changed and before ~= nil and now == before then note = "unchanged, " .. now end
    trip:done({ set = E.named(now), part = part, note = note })
  end

  local function act(row, pic)
    if kind == "mark" then
      Br.stars(pic, pl, { row })
      local before = row.star
      local x = math.floor(pl:exact(X.PRESET_FAVORITE_X, 0) + 0.5)
      log("'%s': clicking %s's star at (%d,%d), its row's height (read '%s', %s)", label, row.name, x, row.y, row.read,
        row.how)
      local ok, said = E.ring.clickPoint(o, label, x, row.y, nil, true)
      if not ok then
        pic:release()
        return trip:done({ sentence = said })
      end
      -- The star's box on screen, the change wait's watch.
      local k = pl.k
      local box = { math.floor(x - X.FAVORITE_STATE[1] / 2 * k), math.floor(row.y - X.FAVORITE_STATE[2] / 2 * k),
        math.ceil(x + X.FAVORITE_STATE[1] / 2 * k), math.ceil(row.y + X.FAVORITE_STATE[2] / 2 * k) }
      local deadline = host.now() + CHANGE_WITHIN
      local function round(from)
        waitFrom(trip, ctx, from, { box }, math.max(1, deadline - host.now()), 0, function(s, changed)
          if not s then return trip:done({ part = browserPart(nil), error = "the star cannot be read now" }) end
          local again = { index = row.index, x = row.x, y = row.y }
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
          part.values.favoriteStates = part.values.favoriteStates or {}
          part.values.favoriteStates[row.index - 1] = again.star
          local words = again.star and "a favorite" or "not a favorite"
          if before ~= nil and again.star == before then return trip:done({ part = part, note = "unchanged, " .. words }) end
          if not changed then return trip:done({ part = part, note = "unchanged, " .. words }) end
          trip:done({ part = part, said = pname .. ", " .. words })
        end)
      end
      return round(pic)
    end
    -- Whatever a click on a row does in Avenger — select, load — may change the preset.
    forget("the page's " .. label)
    log("'%s': %s %s at (%d,%d), the middle of its words (read '%s', %s)", label,
      kind == "load" and "double-clicking" or "clicking", row.name, row.x, row.y, row.read, row.how)
    pic:release()
    local ok, said
    if kind == "load" then
      ok, said = E.ring.doublePoint(o, label, row.x, row.y, true, function(okD, saidD)
        if okD then return E.ring.watchName(o, was, afterName, label) end
        if trip:inFront() then trip:done({ sentence = saidD }) end
      end)
    else
      ok, said = E.ring.clickPoint(o, label, row.x, row.y, nil, true)
      if ok then E.ring.watchName(o, was, afterName, label) end
    end
    if not ok then return trip:done({ sentence = said }) end
  end

  local function rowsFrom(pic, rows)
    B.seek(o, trip, ctx, {
      list = "presets",
      label = label,
      pic = pic,
      entries = rows,
      read = readRows,
      match = function(e) return e.index == target end,
      place = function(e) return e.index end,
      target = target,
      ordered = true,
      onFound = act,
      onNotFound = function(word, s, sentence)
        if s then s:release() end
        if sentence then return trip:done({ sentence = sentence }) end
        log("'%s': %s not found among the rows (%s) — nothing clicked", label, pname, word)
        trip:done({ part = browserPart(nil), error = notFound(word, pname, "list of " .. cat) })
      end,
    })
  end

  local rows = Br.placed(Br.lines(Br.rows(ctx.byName["presets"]), lists.presets, lists.presetCategories, "preset rows"))
  local shown = Br.categoryOf(rows, lists.presetCategories)
  if shown == cat then return rowsFrom(ctx.snap, rows) end
  -- The rows are another category's, or none was placed: the target's category first — the read
  -- decides, not the page's flag (the tool clicked it only where the page showed every category).
  log("'%s': the rows read as %s's, %s is in %s — its category first", label, tostring(shown), pname, cat)
  local cats = Br.lines(Br.rows(ctx.byName["categories"]), lists.categories, nil, "categories")
  local ci = table.find(lists.categories, cat)
  if not ci then
    ctx.snap:release()
    return trip:done({ error = pname .. " has no category in the database" })
  end
  chooseCategory(o, trip, ctx, label, lists, ci, ctx.snap, cats, function(byName, _, s)
    local lines = Br.placed(Br.lines(Br.rows(byName["presets"]), lists.presets, lists.presetCategories, "preset rows"))
    local now = Br.categoryOf(lines, lists.presetCategories)
    if now ~= cat then
      s:release()
      return trip:done({ part = { keys = { "categoryShown" }, values = { categoryShown = now } },
        error = now and ("VPS Avenger lists the presets of " .. now .. " (by their names, from the database)")
          or (cat .. " chosen; which presets VPS Avenger lists cannot be read now") })
    end
    rowsFrom(s, lines)
  end, function(result) trip:done(result) end)
end

-- select_all_expansion_presets: no click — the name read, then the browser's rows, and the page's
-- category "every one".
function B.all(o, trip, label, known)
  local name = E.ring.nameNow(o)
  if not trip:inFront() then return end
  local expansion = known.selectedExpansion
  local ctx = look(o, trip, { "caption", "name", "categories", "presets" }, expansion, label)
  if not ctx then return end
  local st = ctx.st
  local why = notOpen(st, expansion)
  if why then
    ctx.snap:release()
    return trip:done({ set = E.named(name), part = partOf(st, known), error = why })
  end
  local lists = known.lists
  local fav = Br.fav(ctx.snap, ctx.pl)
  local v = { pageCategory = B.ALL, favoriteView = fav, loadedIndex = loadedIn(name, expansion) }
  if lists then
    local lines, shown = presetRows(ctx.snap, ctx.pl, ctx.byName["presets"], lists, fav)
    v.categoryShown = shown
    if fav then v.favoriteStates = starsOver(lines, known, expansion) end
  end
  ctx.snap:release()
  trip:done({ set = E.named(name), part = { keys = { "pageCategory", "favoriteView", "loadedIndex", "categoryShown",
    "favoriteStates" }, values = v }, focus = "expansion-category-all" })
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
    return nil, "the preset loaded is in another expansion, " .. p.expansion .. ", from the database"
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

-- align_open_expansion: no click. The header's name says which expansion is open where it is read
-- as one (read/browser.luau, Br.state); where it is not, every read after the page key and every
-- action of the browser's scores the expansion the page has open first, and takes it on one preset
-- name its columns share (read/expansion.luau, E.fromArea), so an expansion that opened in its place
-- stays named read after read; this read expects none, and the database names what the columns list over its whole catalog
-- (the tool scored its roster) — or, where two expansions list what was read as much, names neither,
-- and which one is open cannot be read (the tool took the first). The name read first, as All reads
-- it (the tool placed the preset loaded in what it found); then the caption and the columns, once —
-- the tool's 0.45 s second read goes: every trip's first read comes at the same arrival. Posted:
-- what the read says; with an expansion named, its lists, the rows' category, FAV and, with FAV on,
-- the stars; the page's own category only where the expansion is another than the page had (the
-- tool cleared its "All" only then). Nothing settled: every value of the browser's posted as not
-- read, never kept.
function B.align(o, trip, label, known)
  local name = E.ring.nameNow(o)
  if not trip:inFront() then return end
  local ctx = look(o, trip, { "caption", "name", "categories", "presets" }, nil, label)
  if not ctx then return end
  local st, s, pl = ctx.st, ctx.snap, ctx.pl
  local set = E.named(name)
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
    return trip:done({ set = set, part = partOf(st, known), error = "VPS Avenger shows an expansion open; which one cannot be "
      .. (DB.present and "read now" or "read without the preset database") })
  end
  local y = st.name
  local had = known.expansionOpen == true and known.selectedExpansion ~= "" and known.selectedExpansion or nil
  local marks = {}
  local fav = Br.fav(s, pl, marks)
  local v = { expansionOpen = true, selectedExpansion = y, favoriteView = fav, loadedIndex = loadedIn(name, y),
    -- Named by what it lists, the database's naming — unless the header read its name, or the page had
    -- it open by the word clicked, as the key's read keeps it (read/open.luau).
    selectedFromDatabase = st.score ~= nil and not (y == known.selectedExpansion and not known.selectedFromDatabase)
      or nil }
  local keys = { "expansionView", "expansionViewFrom", "expansionOpen", "selectedExpansion", "lists", "loadedIndex",
    "favoriteView", "favoriteStates", "categoryShown", "selectedFromDatabase" }
  local okL, lists = pcall(DB.lists, y)
  local lines = nil
  if okL and lists then
    v.lists = lists
    lines, v.categoryShown = presetRows(s, pl, ctx.byName["presets"], lists, fav, marks)
    if fav then v.favoriteStates = starsOver(lines, known, y) end
  end
  if y ~= had then
    v.pageCategory = v.categoryShown
    keys[#keys + 1] = "pageCategory"
    log("'%s': the page had %s; %s by what it lists (%d)", label, had or "no expansion open", y, st.score or 0)
  end
  openPicture(s, ctx, lines, marks)
  s:release()
  local words = y .. (v.selectedFromDatabase and ", from the database" or "")
  trip:done({ set = set, part = { keys = keys, values = v },
    note = (y == had and "unchanged, " or "corrected to ") .. words })
end

-- ── Avenger's own list ───────────────────────────────────────────────────────

-- How sure a tile's name is, where the reads of one place agree as often on two: the same words,
-- then the same ignoring case, then similarity, then a text kept as read.
local SURE = { exact = 4, folded = 3, fuzzy = 2, ["as read"] = 1 }

-- Avenger's list gathered from reads of its grid taken a step apart from its top down. `add(tiles)`
-- — one read's tiles, in reading order — places them by where their names were read before: each
-- name read before says where the read begins, and the place most of them say is taken (on a tie,
-- the one nearest where the read before began), so a tile misread in the overlap places nothing
-- wrong. It answers "merged"; "empty" (no tile: nothing learnt); "gap" (no name of it read before:
-- the grid may have moved past a part of the list unread); or "backward" (it begins before the read
-- before it: the grid moved up). `result()`: the names, each place's the one most of its reads read
-- (the surer on a tie), and their marks — true where that is the database's spelling of a read that
-- took similarity — or nil and why: a place read as two names as often and as surely, or two places
-- read as one name (Avenger lists each expansion once, so one of them was misread). And, for the
-- log, every place whose reads did not agree.
function B.gather()
  local places, at = {}, {}
  local from = nil
  local g = {}
  function g.add(tiles)
    if #tiles == 0 then return "empty" end
    local d = 0
    if #places > 0 then
      local counts = {}
      for j, t in ipairs(tiles) do
        for place in pairs(at[t.key] or {}) do counts[place - j] = (counts[place - j] or 0) + 1 end
      end
      local best, n = nil, 0
      for k, c in pairs(counts) do
        local off, bestOff = math.abs(k - (from - 1)), best and math.abs(best - (from - 1))
        if c > n or (c == n and (off < bestOff or (off == bestOff and k > best))) then best, n = k, c end
      end
      if best == nil then return "gap" end
      if best + 1 < from then return "backward" end
      d = best
    end
    for j, t in ipairs(tiles) do
      local place = d + j
      local p = places[place] or {}
      places[place] = p
      local v = p[t.key]
      if not v then
        v = { n = 0, name = t.name, how = t.how }
        p[t.key] = v
      elseif SURE[t.how] > SURE[v.how] then
        v.name, v.how = t.name, t.how
      end
      v.n += 1
      at[t.key] = at[t.key] or {}
      at[t.key][place] = true
    end
    from = d + 1
    return "merged"
  end
  function g.result()
    local names, marks, seen, told = {}, {}, {}, {}
    for i, p in ipairs(places) do
      local reads = {}
      for key, v in pairs(p) do reads[#reads + 1] = { key = key, v = v } end
      table.sort(reads, function(a, b)
        if a.v.n ~= b.v.n then return a.v.n > b.v.n end
        if SURE[a.v.how] ~= SURE[b.v.how] then return SURE[a.v.how] > SURE[b.v.how] end
        return a.key < b.key
      end)
      local best, other = reads[1].v, reads[2] and reads[2].v
      if other and other.n == best.n and SURE[other.how] == SURE[best.how] then
        return nil, string.format("place %d read as %s and as %s as often", i, best.name, other.name), told
      end
      if other then
        local all = {}
        for _, r in ipairs(reads) do
          all[#all + 1] = string.format("'%s' %d time%s", r.v.name, r.v.n, r.v.n == 1 and "" or "s")
        end
        told[#told + 1] = string.format("place %d read as %s — %s taken", i, table.concat(all, ", "), best.name)
      end
      local key = reads[1].key
      if seen[key] then return nil, string.format("places %d and %d both read as %s", seen[key], i, best.name), told end
      seen[key] = i
      names[i] = best.name
      marks[i] = best.how == "fuzzy" or nil
    end
    return names, marks, told
  end
  return g
end

-- read_expansion_list: Avenger's grid moved to its top with change waits only, then read down a step
-- at a time to its end, each read placed in the list by B.gather. Kept only where both ends were
-- reached, the reads at both read something, no read lost its place, and every place told one name
-- (S.setAvengerList); otherwise nothing is kept, and the answer says which.
function B.list(o, trip, label, known)
  local ctx = look(o, trip, { "caption", "name", "area" }, known.selectedExpansion, label)
  if not ctx then return end
  local st, pl = ctx.st, ctx.pl
  local function refuse(why, part)
    ctx.snap:release()
    log("'%s': nothing moved — %s", label, why)
    trip:done({ part = part, error = why })
  end
  if not st then return refuse(VIEW_UNREAD) end
  if st.open then return refuse(OPEN_NOW, partOf(st, known)) end
  -- The view read now, never one remembered (as B.open).
  local view = st.view
  if not view then return refuse(VIEW_UNREAD, partOf(st, known)) end
  host.speech.output("Reading the expansions VPS Avenger lists.", { interrupt = false })
  local list = B.gather()
  local restarted = false
  -- What the read after the last step added (B.gather's answer).
  local last = nil
  local GAP = "a part of the list could not be read"
  local function incomplete(why, sentence, words)
    log("'%s': the list not read whole (%s) — nothing kept", label, why)
    if sentence then return trip:done({ sentence = sentence }) end
    trip:done({ error = (words or "the list could not be read to its end") .. "; "
      .. (S.avengerList and "the list shown stays" or "the database's list stays") })
  end
  local readTiles = function(s)
    local b = Br.read(s, pl, { "area" })
    return Br.tiles(b["browser area"], view, pl)
  end
  local down
  local function up(pic, n)
    if n >= B.GUARD then
      pic:release()
      return incomplete("still moving up after " .. B.GUARD .. " steps")
    end
    B.step(o, trip, ctx, "grid", "up", pic, label, function(s, moved, sentence)
      if sentence then return incomplete("refused", sentence) end
      if not s then return incomplete("no picture") end
      if moved then return up(s, n + 1) end
      log("'%s': at the top after %d step%s", label, n, n == 1 and "" or "s")
      local tiles = readTiles(s)
      if not trip:inFront() then
        s:release()
        return
      end
      last = list.add(tiles)
      if last == "empty" then
        s:release()
        return incomplete("nothing read at the top", nil, GAP)
      end
      down(s, 0)
    end)
  end
  down = function(pic, n)
    if n >= B.GUARD then
      pic:release()
      return incomplete("still moving down after " .. B.GUARD .. " steps")
    end
    B.step(o, trip, ctx, "grid", "down", pic, label, function(s, moved, sentence)
      if sentence then return incomplete("refused", sentence) end
      if not s then return incomplete("no picture") end
      if not moved then
        s:release()
        if last == "empty" then return incomplete("nothing read at the end", nil, GAP) end
        local names, marks, told = list.result()
        for _, line in ipairs(told) do log("'%s': %s", label, line) end
        if not names then return incomplete(marks, nil, GAP) end
        log("'%s': at the end — %d expansion%s: %s", label, #names, #names == 1 and "" or "s", table.concat(names, ", "))
        S.setAvengerList(names, marks)
        return trip:done({ part = partOf(st, known), note = string.format("VPS Avenger lists %d expansion%s; this list is "
          .. "VPS Avenger's for this session", #names, #names == 1 and "" or "s"), focus = "expansion-search-input" })
      end
      local tiles = readTiles(s)
      if not trip:inFront() then
        s:release()
        return
      end
      last = list.add(tiles)
      if last == "empty" then log("'%s': a step down read no tile — the next read places the list", label) end
      if last == "gap" then
        s:release()
        log("'%s': a read that overlaps nothing read before — a part of the list may lie unread here", label)
        return incomplete("a read placed nowhere", nil, GAP)
      end
      if last == "backward" then
        if restarted or means.grid ~= "wheel" then
          s:release()
          return incomplete("the grid moved up where it was moved down")
        end
        restarted = true
        sign.grid = -sign.grid
        log("'%s': the wheel moved the grid up, where it was turned down — turned round for the session, and the "
          .. "list read again from its top", label)
        list, last = B.gather(), nil
        return up(s, 0)
      end
      down(s, n + 1)
    end)
  end
  gridPicture(ctx.snap, ctx, view, Br.tiles(ctx.byName["browser area"], view, pl))
  up(ctx.snap, 0)
end

-- What actions.luau hands over: { ring = main.luau's helpers, named = (name) -> the preset's set,
-- plugin = () -> the window the page was shown beside, shown = () -> the list of expansions last
-- posted }.
function B.start(env)
  E = env
end

-- How each list was seen to move this session, and its wheel's sign: for the log's readers, and the
-- tests.
function B.ways() return means, sign end

return B
