-- The page's reads when its key opens it: what Avenger shows, read off ONE picture of it.
--
--   1. In the page's `open` — the runtime's page key, a handler, while the header overlay is
--      active on an identified Avenger and nothing covers it (docs/api/overlay.md, O:addPage) — or
--      in a trip's continuation after Initialize: one snapshot of the plug-in's panel
--      (host.screen.snapshot, waited for).
--   2. Every question of the avenger_control project's classifiers (read/strip, centre, drumsq,
--      macros, expansion) asked of that picture, each waited for in turn: the runtime shows the
--      page once `open` has returned, so the answers have to be in by then. Each is the image
--      worker's work on the picture already taken, and one event-loop turn to hand it over.
--   3. Then ONE text read of the same picture, with a named region for each thing to read — the
--      preset's name, the browser's view, the OSC tabs' names (one region per tab, the count from
--      the dots), the macro tabs' captions and the five names of the tab shown, the central tabs'
--      captions — and the knob reader over a few hundred points of the picture (read/knob.luau).
--      Only when the browser's caption reads no view, a second text read of the browser's area, to
--      tell an open expansion — the one region the project read only then, and the largest by
--      far, too slow on an Intel Mac to read every time.
--   4. The values (state.luau's), each nil where it was not read; the log with every share, count,
--      angle and word, and the times; in a calibrating run, marked pictures of the parts read.
--
-- Nothing here clicks, and nothing reads the screen after the snapshot: every later read is of
-- the picture, so every value comes from the same moment. The plug-in's coordinates are placed
-- with the overlay's origin and factor as they were at the key (read/picture.luau, `placer`).
--
-- The picture is the panel as far as the screen shows it: a panel that reaches past the primary
-- display's right or bottom edge is cut there (`pictureRegion`), and what lies past the edge is
-- not read. Every question is asked of the part of its region the picture holds, and a box that
-- lies partly past the edge is not read (read/picture.luau, `Grid:share`).
--
-- One press at a time, without a flag: everything is waited for in the one handler, and the
-- module's next key waits in its mailbox meanwhile (docs/module-runtime-and-lifecycle.md,
-- handlers).

local O = host.require("com.platform.overlay")
local G = host.include("src/geometry.luau")
local H = host.include("src/header.luau")
local DB = host.include("src/presets.luau")
local R = host.include("src/read/ocr.luau")
local P = host.include("src/read/picture.luau")
local T = host.include("src/tables/strip.luau")
local Strip = host.include("src/read/strip.luau")
local Centre = host.include("src/read/centre.luau")
local DrumSq = host.include("src/read/drumsq.luau")
local Macros = host.include("src/read/macros.luau")
local Expansion = host.include("src/read/expansion.luau")
local Knob = host.include("src/read/knob.luau")

local Open = {}

-- The parts of Avenger a calibrating run photographs, at 50 % from its corner: the strip with the
-- Drums tab's switches, the macro panel, the oscillator's knobs, the central tabs and DRUM SQ's
-- row. One picture of each per zoom in a session, of the picture the reads were answered from.
Open.PICTURES = {
  strip = { 230, 18, 640, 145 },
  macros = { 700, 425, 871, 535 },
  knob = { 320, 46, 372, 182 },
  centre = { 240, 236, 400, 252 },
  drumsq = { 330, 250, 490, 268 },
}
Open.PICTURE_ORDER = { "strip", "macros", "knob", "centre", "drumsq" }

local log = function(s) host.log.info("[avenger] [read] " .. s) end

-- What the user is told when the key's reads show no page, by why. "The user moved on" is not
-- said: the user is somewhere else by then, and hears that place.
Open.NO_FACTOR = "The page did not open: VPS Avenger's size cannot be told now."
Open.NO_PICTURE = "The page did not open: VPS Avenger could not be read now."
Open.FAILED = "The page did not open: reading VPS Avenger failed."

-- The pictures written this session, by name.
local shots = {}

local function pictures(pl, snap, marks, zoom)
  if not O.calibrating() then return end
  for _, area in ipairs(Open.PICTURE_ORDER) do
    local name = string.format("%s-%dpct", area, zoom or 0)
    local rect = pl:rect(Open.PICTURES[area])
    if rect and not shots[name] then
      shots[name] = true
      local path = host.path("calibration/" .. name .. ".png")
      local list = marks[area] or {}
      -- With a callback: the picture is written on the image worker, the page is not held for it.
      -- Guarded: a measuring instrument must not take the reads it measures down with it.
      local ok, err = pcall(host.screen.saveMarked, path, { snapshot = snap, region = rect, marks = list },
        function(saved, why)
          host.log.info(string.format("[avenger] [calibrate] %s picture, (%d,%d)-(%d,%d): %d mark%s -> %s (%s)", area,
            rect[1], rect[2], rect[3], rect[4], #list, #list == 1 and "" or "s", path,
            saved and "true" or ("false: " .. tostring(why))))
        end)
      if not ok then
        host.log.info(string.format("[avenger] [calibrate] %s picture not asked for: %s", area, tostring(err)))
      end
    end
  end
end

-- The OSC tabs' names into `v.oscNames` and `v.oscSnapped`, from `texts` (tab -> text read, nil
-- for none), each cleaned the project's way and checked against the database's names of the
-- preset's tabs (`vocab`, or nil).
local function oscNames(texts, count, vocab, v)
  v.oscNames, v.oscSnapped = {}, {}
  local words = {}
  for i = 1, count do
    local raw = texts[i]
    if raw and R.tidy(raw) ~= "" then
      local clean = R.cleanOscName(raw, i)
      local name, snapped, ratio = R.snap(clean, vocab and vocab[i])
      v.oscNames[i], v.oscSnapped[i] = name, snapped
      words[#words + 1] = string.format("%d '%s'%s%s", i, R.forLog(raw), clean ~= R.tidy(raw) and (" -> '" .. clean .. "'") or "",
        snapped and string.format(" -> '%s', the database's (%.2f alike)", name, ratio) or "")
    else
      words[#words + 1] = string.format("%d not read", i)
    end
  end
  log("strip: the tabs' names " .. table.concat(words, "; "))
end

-- The work once every question has answered: the text read, the decisions, the pictures. Runs in
-- the handler the questions were waited for in, where the text read waits too.
local function decide(ctx)
  local pl, snap, got, opts = ctx.pl, ctx.snap, ctx.got, ctx.opts
  local v = { zoom = opts.zoom }
  local marks = { strip = {}, macros = {}, knob = {}, centre = {}, drumsq = {}, expansion = {} }

  -- How many oscillators the dots say, which places the tabs' names.
  local count = Strip.count(pl, got, log)

  -- The one text read, of the picture.
  local entries = {}
  local nameRect = pl:rect(G.NAME_REGION)
  if nameRect then entries[#entries + 1] = { name = "preset", region = nameRect } end
  Expansion.entries(pl, entries)
  Macros.entries(pl, entries)
  local centralRect = pl:rect(Centre.ROW)
  if centralRect then entries[#entries + 1] = { name = "central tabs", region = centralRect } end
  if count then
    for i = 1, count do
      local r = pl:rect(T.tabSpan(i, count, T.NAME_TOP, T.NAME_BOTTOM))
      if r then entries[#entries + 1] = { name = "osc " .. i, region = r } end
    end
  else
    local r = pl:rect(T.AREA)
    if r then entries[#entries + 1] = { name = "strip", region = r } end
  end
  local byName = {}
  if #entries > 0 then
    local _, names = host.ocr.recognize(entries, { snapshot = snap, lang = H.lang() })
    byName = names or {}
  end
  ctx.tText = host.now()
  local function textOf(name)
    local r = byName[name]
    return r and r.status == "text" and R.tidy(r.text) or nil
  end
  local said = {}
  for _, e in ipairs(entries) do
    local r = byName[e.name]
    said[#said + 1] = string.format("%s %s", e.name, r and (r.status == "text" and ("'" .. R.forLog(r.text) .. "'")
      or (r.status .. (r.error and (": " .. r.error) or ""))) or "not answered")
  end
  log("words: " .. table.concat(said, "; "))

  -- The preset's name, the project's way with its blank preset, and where the database has it.
  local name = textOf("preset")
  if name then
    local taken, init = R.presetName(name)
    if init and taken ~= name then log(string.format("the name read '%s' is taken as Avenger's blank preset, %s", name, taken)) end
    v.preset = taken
  end
  local place = nil
  if v.preset and DB.present then
    local ok, p = pcall(DB.place, v.preset)
    if ok then place = p else log("the preset database failed: " .. tostring(p)) end
  end
  if place then v.expansion = place.expansion end

  -- The strip: a count from the text where the dots gave none, the names, then the rest.
  local texts = {}
  if count then
    for i = 1, count do texts[i] = textOf("osc " .. i) end
  else
    local strip = byName["strip"]
    local found
    count, found = Strip.countFromText(strip and strip.status == "text" and strip.text or "")
    log(string.format("strip: the tabs read whole%s — %s", #found > 0 and (" name OSC " .. table.concat(found, ", ")) or "",
      count and (count .. " oscillator" .. (count == 1 and "" or "s")) or "no count"))
    if count then texts = Strip.namesFromWords(pl, strip and strip.words, count) end
  end
  v.oscCount = count
  if count then oscNames(texts, count, place and place.osc, v) end
  Strip.decide(pl, snap, got, count, v, log, marks.strip)
  local central = textOf("central tabs")
  if central then log("centre: the captions read '" .. central .. "'") end
  if v.selectedType == "drums" then
    Centre.decide(pl, got, v, log, marks.centre)
    if v.central == "drum_sq" then DrumSq.decide(pl, got, v, log, marks.drumsq) end
  end

  -- The macros: the count, the tab shown, its names, its buttons, its knobs.
  local counted = Macros.count(byName, v)
  Macros.selected(pl, got, v.macroCount, v, log, marks.macros)
  local vocab = place and place.macros and v.macroSelected and place.macros[v.macroSelected] or nil
  Macros.labels(byName, v, vocab, counted, log)
  Macros.buttons(pl, got, v, log, marks.macros)
  v.macroAmounts = {}
  local amounts = {}
  for i = 1, 3 do
    local a, w = Knob.macro(pl, snap, i, marks.macros)
    v.macroAmounts[i] = a
    amounts[#amounts + 1] = string.format("knob %d %s", i, w)
  end
  log("macros: " .. table.concat(amounts, "; "))
  Knob.oscMiddles(pl, marks.knob)
  if v.selectedType == "osc" then
    v.osc = {}
    log("oscillator: " .. Knob.osc(pl, snap, v.osc, marks.knob))
  end

  -- The browser: its view, or what its area says when the caption reads none; FAV.
  local viewText = textOf("expansion view")
  v.expansionView = Expansion.parseView(viewText)
  if v.expansionView then
    v.expansionOpen, v.selectedExpansion = false, ""
    log(string.format("expansion: the view reads '%s' — %s, no expansion open", tostring(viewText), v.expansionView))
  else
    local area = Expansion.area(pl)
    local seen, why = nil, "no area"
    if area then
      local r = host.ocr.recognize({ name = "expansion area", region = area }, { snapshot = snap, lang = H.lang() })
      local roster = ctx.opts.roster
      seen, why = Expansion.fromArea(r and r.status == "text" and r.text or "", DB, roster)
    end
    log(string.format("expansion: the view reads %s — the area read: %s", viewText and ("'" .. viewText .. "'") or "nothing",
      tostring(why)))
    if seen then
      v.expansionOpen = seen.open
      v.selectedExpansion = seen.open and (seen.name or "") or ""
      if seen.name then
        local ok, lists = pcall(DB.lists, seen.name)
        if ok and lists then
          v.lists = lists
          if place and place.expansion == seen.name then v.loadedIndex = place.index - 1 end
        end
      end
    end
  end
  Expansion.favorite(pl, got, v, log, marks.expansion)

  pictures(pl, snap, marks, opts.zoom)
  v.cut = ctx.cut
  return v
end

-- The part of the plug-in's panel (client rectangle `c`) the picture holds, in whole pixels: all
-- of it, less what lies past the primary display's right or bottom edge while Avenger's corner is
-- on that display — the only display the host names (host.screen.size), as main.luau's check of
-- the warning box takes it. Nothing is drawn there to read, and a Mac's snapshot of Avenger at
-- 190 % and more would pass what one snapshot may hold (docs/api/screen.md, the budget: 6,710,886
-- points in a module that holds no other). The rectangle, and the screen's size when it was cut.
local function pictureRegion(c)
  local r = { math.floor(c.x), math.floor(c.y), math.ceil(c.x + c.w), math.ceil(c.y + c.h) }
  local ok, s = pcall(host.screen.size)
  if not (ok and type(s) == "table" and type(s.w) == "number" and type(s.h) == "number") then return r, nil end
  if r[1] < 0 or r[2] < 0 or r[1] >= s.w or r[2] >= s.h then return r, nil end
  if r[3] <= s.w and r[4] <= s.h then return r, nil end
  return { r[1], r[2], math.min(r[3], s.w), math.min(r[4], s.h) }, s
end

-- Request `req` cut to `view`, the picture's rectangle cut at the screen's edge: its grid's cells
-- as many per pixel as before, at least one each way, and its boxes past the edge not read
-- (`req.view`); nil when nothing of it is in the picture.
local function cutTo(req, view)
  local r = req.rect
  local c = { math.max(r[1], view[1]), math.max(r[2], view[2]), math.min(r[3], view[3]), math.min(r[4], view[4]) }
  if c[3] <= c[1] or c[4] <= c[2] then return nil end
  if req.kind == "cells" then
    req.cols = math.clamp(math.floor(req.cols * (c[3] - c[1]) / (r[3] - r[1]) + 0.5), 1, 256)
    req.rows = math.clamp(math.floor(req.rows * (c[4] - c[2]) / (r[4] - r[2]) + 0.5), 1, 256)
  end
  req.rect = c
  req.view = view
  return req
end

local function corners(r)
  return string.format("(%d,%d)-(%d,%d)", r[1], r[2], r[3], r[4])
end

-- Reads Avenger for the page, in a handler where a read may wait: overlay `o` is the header
-- overlay, active on an identified Avenger. `opts`: { zoom = the zoom read, or nil; mark = o:here()
-- at the key; roster = the database's expansions, or nil }. Answers the values read — nil where a
-- read failed — or nil, why nothing was read for the log, and the sentence to say (Open.NO_FACTOR,
-- NO_PICTURE, FAILED; nil when the user moved on). `values.zoom` is always there, and `values.cut`
-- is the screen's size when the picture was cut at its edge.
function Open.read(o, opts)
  local t0 = host.now()
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then
    return nil, "the plug-in cannot be placed now (no factor)", Open.NO_FACTOR
  end
  local region, cut = pictureRegion(origin.client)
  -- Guarded: corners past what this module's snapshots may hold raise (docs/api/screen.md).
  local taken, snap, why = pcall(host.screen.snapshot, { region = region })
  if not (taken and snap) then
    return nil, "no picture of " .. corners(region) .. ": " .. tostring(taken and why or snap), Open.NO_PICTURE
  end
  local fine, gone = o:stillHere(opts.mark)
  if not fine then
    snap:release()
    return nil, "the user moved on while the picture was taken — " .. tostring(gone)
  end
  if cut then
    log(string.format("the picture: %s, cut at the edge of the %dx%d screen — the panel ends at %d,%d; what lies "
      .. "past the edge is not read", corners(region), cut.w, cut.h, math.ceil(origin.client.x + origin.client.w),
      math.ceil(origin.client.y + origin.client.h)))
  end
  local ctx = { pl = P.placer(origin, k, opts.zoom), snap = snap, opts = opts, got = { why = {} }, t0 = t0,
    tSnap = host.now(), cut = cut }
  local all = {}
  Strip.ask(ctx.pl, all)
  Centre.ask(ctx.pl, all)
  DrumSq.ask(ctx.pl, all)
  Macros.ask(ctx.pl, all)
  Expansion.ask(ctx.pl, all)
  -- Each asked of what the picture holds of it, and waited for; guarded, so that every answer is
  -- counted whatever it holds.
  local asked = 0
  for _, req in ipairs(all) do
    if not cut or cutTo(req, region) then
      asked += 1
      local ok, value, reason
      if req.kind == "cells" then
        ok, value, reason = pcall(host.screen.cells, { region = req.rect, cols = req.cols, rows = req.rows,
          predicate = req.pred, snapshot = snap })
      else
        ok, value, reason = pcall(host.screen.profile, { region = req.rect, axes = "columns", snapshot = snap })
      end
      if not ok then
        ctx.got.why[req.name] = tostring(value)
      elseif value ~= nil then
        local made, got = pcall(function() return req.kind == "cells" and P.grid(req, value) or value end)
        if made then ctx.got[req.name] = got else ctx.got.why[req.name] = tostring(got) end
      else
        ctx.got.why[req.name] = reason
      end
    else
      ctx.got.why[req.name] = "past the edge of the screen"
    end
  end
  ctx.tPixels = host.now()
  local ok, v = pcall(decide, ctx)
  snap:release()
  if not ok then
    log("the reads failed: " .. tostring(v))
    return nil, "the reads failed", Open.FAILED
  end
  log(string.format("%d questions of one picture; the picture %d ms after the key, the questions %d ms, the "
    .. "text %d ms, the rest %d ms", asked, ctx.tSnap - t0, ctx.tPixels - ctx.tSnap, (ctx.tText or ctx.tPixels)
      - ctx.tPixels, host.now() - (ctx.tText or ctx.tPixels)))
  return v
end

-- ── A part of Avenger, read again ────────────────────────────────────────────
-- What an action of the page or the ring reads before its click and after it (src/act.luau): only
-- the part it acts on, off a picture of that part. Each a rectangle at 50 % from Avenger's corner,
-- and what is read in it:
--   strip    the OSC tabs, Drums and Trig: the count, the tab selected, every Mute and Solo, the
--            Drums tab's switches and mode (read/strip.luau)
--   knobs    the strip, and the selected oscillator's knobs (read/knob.luau)
--   tab      the strip, the knobs, and with the Drums tab the central tab and DRUM SQ
--   macros   the macro panel: the tab shown, its buttons and its knobs' amounts; the tabs counted
--            and the five names read with `text`
Open.PARTS = {
  strip = { rect = { 230, 18, 640, 145 }, strip = true },
  knobs = { rect = { 230, 18, 640, 182 }, strip = true, knobs = true },
  tab = { rect = { 230, 18, 640, 268 }, strip = true, knobs = true, centre = true },
  macros = { rect = { 700, 425, 871, 535 }, macros = true },
}

-- The keys of state.luau's values each part reads, whether it finds them or not: what it does not
-- find is not kept from an earlier read (S.part). The OSC names and the macro tabs' count and names
-- only where the part's text is read.
local STRIP_KEYS = { "oscCount", "oscMute", "oscSolo", "selectedType", "selectedIndex", "drumsMute",
  "drumsSolo", "trigMute", "trigSolo", "drumPattern", "drumLock", "drumMode" }

-- The picture's rectangle for part `name` with placer `pl`, cut at the primary display's edge as the
-- key's picture is (pictureRegion), and to the plug-in's panel; nil when nothing of it is on screen.
function Open.partRegion(pl, name)
  return Open.region(pl, Open.PARTS[name].rect)
end

-- The same for any table rectangle `rect` (the renames' fields, read/editor.luau).
function Open.region(pl, rect)
  local r = pl:rect(rect)
  if not r then return nil end
  local c = pl.origin.client
  r = { math.max(r[1], math.floor(c.x)), math.max(r[2], math.floor(c.y)), math.min(r[3], math.ceil(c.x + c.w)),
    math.min(r[4], math.ceil(c.y + c.h)) }
  local cut = pictureRegion({ x = r[1], y = r[2], w = r[3] - r[1], h = r[4] - r[2] })
  if cut[3] - cut[1] < 2 or cut[4] - cut[2] < 2 then return nil end
  return cut
end

-- Reads part `name` off snapshot `snap` (of Open.partRegion's rectangle), in a handler where its
-- questions wait. `opts`: { pl = the placer the picture was taken with, text = read the part's text
-- too, macroCount = the macro tabs counted before, for a part read without its text, preset = the
-- preset's name last read, whose tabs' names in the database are the names' vocabulary }. Answers
-- the values read (state.luau's), and the keys the part read — a key of the list missing from the
-- values was not found. Nothing clicks; nothing but the snapshot is read.
function Open.part(snap, name, opts)
  local part = Open.PARTS[name]
  local pl = opts.pl
  local view = { snap.x, snap.y, snap.x + snap.w, snap.y + snap.h }
  local all = {}
  if part.strip then Strip.ask(pl, all) end
  if part.centre then
    Centre.ask(pl, all)
    DrumSq.ask(pl, all)
  end
  if part.macros then Macros.ask(pl, all) end
  local got = { why = {} }
  for _, req in ipairs(all) do
    if cutTo(req, view) then
      local ok, value, reason
      if req.kind == "cells" then
        ok, value, reason = pcall(host.screen.cells, { region = req.rect, cols = req.cols, rows = req.rows,
          predicate = req.pred, snapshot = snap })
      else
        ok, value, reason = pcall(host.screen.profile, { region = req.rect, axes = "columns", snapshot = snap })
      end
      if not ok then
        got.why[req.name] = tostring(value)
      elseif value ~= nil then
        local made, g = pcall(function() return req.kind == "cells" and P.grid(req, value) or value end)
        if made then got[req.name] = g else got.why[req.name] = tostring(g) end
      else
        got.why[req.name] = reason
      end
    else
      got.why[req.name] = "outside the part's picture"
    end
  end
  local place = nil
  if opts.preset and DB.present then
    local ok, p = pcall(DB.place, opts.preset)
    if ok then place = p end
  end
  local v, keys = {}, {}
  local function read(list) for _, k in ipairs(list) do keys[#keys + 1] = k end end
  local marks = { strip = {}, macros = {}, knob = {}, centre = {}, drumsq = {} }
  local count = nil
  if part.strip then
    read(STRIP_KEYS)
    count = Strip.count(pl, got, log)
    v.oscCount = count
  end
  -- The one text read of the part's picture, where it is asked.
  local byName = {}
  if opts.text then
    local entries = {}
    if part.strip and count then
      for i = 1, count do
        local r = pl:rect(T.tabSpan(i, count, T.NAME_TOP, T.NAME_BOTTOM))
        if r then entries[#entries + 1] = { name = "osc " .. i, region = r } end
      end
    end
    if part.macros then Macros.entries(pl, entries) end
    if #entries > 0 then
      local _, names = host.ocr.recognize(entries, { snapshot = snap, lang = H.lang() })
      byName = names or {}
    end
  end
  if part.strip then
    if opts.text and count then
      read({ "oscNames", "oscSnapped" })
      local texts = {}
      for i = 1, count do
        local r = byName["osc " .. i]
        texts[i] = r and r.status == "text" and R.tidy(r.text) or nil
      end
      oscNames(texts, count, place and place.osc, v)
    end
    Strip.decide(pl, snap, got, count, v, log, marks.strip)
  end
  if part.knobs then
    read({ "osc" })
    if v.selectedType == "osc" then
      v.osc = {}
      log("oscillator: " .. Knob.osc(pl, snap, v.osc, marks.knob))
    end
  end
  if part.centre then
    read({ "central", "drumSq" })
    if v.selectedType == "drums" then
      Centre.decide(pl, got, v, log, marks.centre)
      if v.central == "drum_sq" then DrumSq.decide(pl, got, v, log, marks.drumsq) end
    end
  end
  if part.macros then
    read({ "macroSelected", "macroButtons", "macroAmounts" })
    local counted = nil
    if opts.text then
      read({ "macroCount", "macroLabels", "macroSnapped" })
      counted = Macros.count(byName, v)
    else
      v.macroCount = opts.macroCount
    end
    Macros.selected(pl, got, v.macroCount, v, log, marks.macros)
    if opts.text then
      local vocab = place and place.macros and v.macroSelected and place.macros[v.macroSelected] or nil
      Macros.labels(byName, v, vocab, counted, log)
    else
      -- Counted before, not read here: not part of what this read says.
      v.macroCount = nil
    end
    Macros.buttons(pl, got, v, log, marks.macros)
    v.macroAmounts = {}
    local amounts = {}
    for i = 1, 3 do
      local a, w = Knob.macro(pl, snap, i, marks.macros)
      v.macroAmounts[i] = a
      amounts[#amounts + 1] = string.format("knob %d %s", i, w)
    end
    log("macros: " .. table.concat(amounts, "; "))
  end
  return v, keys
end

return Open
