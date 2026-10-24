-- The page's reads when its key opens it: what Avenger shows, read off ONE picture of it.
--
--   1. In the page's `open` — the runtime's page key, a handler, while the header overlay is
--      active on an identified Avenger and nothing covers it (docs/api/overlay.md, O:addPage) — or
--      in a trip's continuation after Initialize: one snapshot of the plug-in's panel
--      (host.screen.snapshot, waited for).
--   2. Every question of the avenger_control project's classifiers (read/strip, centre, drumsq,
--      drums, sample, macros, expansion) asked of that picture, each waited for in turn: the runtime
--      shows the page once `open` has returned, so the answers have to be in by then. Each is the
--      image worker's work on the picture already taken, and one event-loop turn to hand it over.
--   3. Then ONE text read of the same picture, with a named region for each thing to read — the
--      preset's name, the browser's view, the OSC tabs' names (one region per tab, the count from
--      the dots), the macro tabs' captions and the five names of the tab shown, the central tabs'
--      captions, and with the Drums tab selected the twelve drum slots' names, DRUM SQ's LENGTH where
--      it is the central tab and the sample browser's rows where EDITOR shows no tab of the editor's
--      (the strip, the central tab and the editor's pixels decided before the text read when the
--      dots gave the count) — and the knob reader over a few hundred points of the picture
--      (read/knob.luau).
--      Only when neither the browser's caption nor its header's name settles whether an expansion
--      is open, a second text read of the browser's area — the one region the project read only
--      then, and the largest by far, too slow on an Intel Mac to read every time — and with an
--      expansion open, of its presets' column.
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
local Dr = host.include("src/read/drums.luau")
local Sm = host.include("src/read/sample.luau")
local C = host.include("src/tables/centre.luau")
local X = host.include("src/tables/expansion.luau")
local Br = host.include("src/read/browser.luau")
local S = host.include("src/state.luau")

local Open = {}

local log = function(s) host.log.info("[avenger] [read] " .. s) end

-- ── The count from elsewhere ─────────────────────────────────────────────────
-- The window a read's values are kept by (state.luau): the one in front, as the page's key takes
-- it (page.luau) — on Windows the DAW's window that holds the plug-in, not the plug-in's own child
-- window the overlay's origin is (docs/api/overlay.md, O:origin) — or nil. The page's reads are
-- handed theirs (Open.read's and Open.part's `window`); every other read takes it so.
function Open.window()
  local ok, w = pcall(host.window.active)
  return ok and type(w) == "table" and w.id or nil
end

-- Where the dots give no count (read/strip.luau, by their spacing): the count the user confirmed
-- for this preset in window `id` (the page's Confirm OSC Count, the project's
-- action_confirm_osc_count: state.luau, S.oscConfirmed), else the preset database's for a preset
-- `place` (DB.place's) places by its words as they stand — not by similarity, and not after Load
-- or Save as, where the name may be the user's own file's — as the project's tool prefers its
-- database's count (its actions.py, _refresh_osc_panel_from_screen). The count and where it came
-- from (Open.FROM's key), or nil. Each is marked as such where it is shown (state.luau,
-- `count_from`), never as read.
Open.FROM = { confirmed = "as confirmed for this preset", database = "by the preset database" }

function Open.countElsewhere(id, preset, place)
  local n = S.oscConfirmed(id, preset)
  if n then return n, "confirmed" end
  if place and place.osc and place.how ~= "fuzzy" and not place.own and #place.osc >= 1 then
    return math.min(8, #place.osc), "database"
  end
  return nil
end

-- Where the dots' count `count` and the count from elsewhere disagree, the log says both: the dots'
-- is taken, since it is what the clicks are placed by.
function Open.compareCount(count, id, preset, place)
  local n, from = Open.countElsewhere(id, preset, place)
  if n and n ~= count then
    log(string.format("strip: the dots read %d oscillators, %d %s — the dots' count is taken", count, n, Open.FROM[from]))
  end
end

-- The parts of Avenger a calibrating run photographs, at 50 % from its corner: the strip with the
-- Drums tab's switches, the macro panel, the oscillator's knobs, the central tabs, DRUM SQ's row,
-- the drumkit table, the sample editor ("sample": the renames' pictures are "editor-…"), DRUM
-- SQ's controls and the expansion browser — marked at the corners of the view's caption and the
-- header's name it is told by, FAV's point and the stars read (the browser's own actions write
-- "browser-grid-…" and "browser-open-…"). One picture of each per zoom in a session, of the
-- picture the reads were answered from; and "browser-unsettled" once per zoom where the header,
-- the caption and the area together settled nothing (read/browser.luau, Br.state) — the pictures
-- the second Mac test's plan asked for.
Open.PICTURES = {
  strip = { 230, 18, 640, 145 },
  macros = { 700, 425, 871, 535 },
  knob = { 320, 46, 372, 182 },
  centre = { 240, 236, 400, 252 },
  drumsq = { 330, 250, 490, 268 },
  drums = { 230, 40, 640, 232 },
  sample = { 240, 252, 630, 420 },
  ["sq-controls"] = { 430, 405, 620, 428 },
  browser = X.BROWSER,
  ["browser-unsettled"] = X.BROWSER,
}
Open.PICTURE_ORDER = { "strip", "macros", "knob", "centre", "drumsq", "drums", "sample", "sq-controls", "browser",
  "browser-unsettled" }

-- Where the sample editor's and DRUM SQ's controls are, marked in their calibration pictures
-- whether or not they were read: the editor's tabs, switches, fields and knobs; LENGTH, SPEED and
-- DRUM SQ's knobs.
local function middles(pl, marks)
  local function mark(list, p) local x, y = pl:point(p[1], p[2]); list[#list + 1] = { x = x, y = y } end
  local editor, sq = marks.sample, marks["sq-controls"]
  for _, key in ipairs(C.EDITOR_TAB_ORDER) do mark(editor, C.EDITOR_TABS[key]) end
  mark(editor, C.ENABLE_STATE)
  mark(editor, C.EQ_ENABLE_STATE)
  for _, key in ipairs({ "play_mode", "grid", "eq1_type", "eq2_type", "loop_stretch" }) do mark(editor, C.VALUES[key]) end
  for _, key in ipairs({ "volume", "pitch", "pan", "spike", "delay", "lowpass", "highpass", "rate" }) do
    mark(editor, C.EDITOR_CONTROLS[key])
  end
  for _, p in ipairs({ C.SQ_CONTROLS.vel_amount, C.SQ_LENGTH, C.SQ_CONTROLS.shuffle, C.SQ_SPEED }) do mark(sq, p) end
end

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
    -- A picture whose marks were not gathered (browser-unsettled, where the browser was settled)
    -- is not written.
    if rect and marks[area] and not shots[name] then
      shots[name] = true
      local path = host.path("calibration/" .. name .. ".png")
      local list = marks[area]
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
  local marks = { strip = {}, macros = {}, knob = {}, centre = {}, drumsq = {}, browser = {}, drums = {},
    sample = {}, ["sq-controls"] = {} }

  -- How many oscillators the dots say, which places the tabs' names. With a count from the dots the
  -- strip is decided before the text read, which then asks the drum slots' names when it reads the
  -- Drums tab selected; a count from elsewhere keeps the strip's decision after it.
  local count = Strip.count(pl, got, log, snap)
  local early = count ~= nil
  if early then Strip.decide(pl, snap, got, count, v, log, marks.strip) end
  -- The central tab, DRUM SQ and the sample editor's pixels, on the Drums tab; decided before the
  -- text read when the strip was, which then asks DRUM SQ's LENGTH where it reads DRUM SQ and the
  -- sample browser's rows where it reads EDITOR with no tab of the editor's.
  local function centre()
    Centre.decide(pl, got, v, log, marks.centre)
    if v.central == "drum_sq" then DrumSq.decide(pl, got, v, log, marks.drumsq) end
    if Sm.shown(v) then Sm.decidePixels(pl, snap, got, v, log, marks.sample) end
  end
  local centreEarly = early and v.selectedType == "drums"
  if centreEarly then centre() end

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
  local slotNames = early and v.selectedType == "drums"
  if slotNames then Dr.entries(pl, entries) end
  if centreEarly and Sm.applies("browser", v) then
    local r = pl:rect(Sm.fieldRect("browser"))
    if r then entries[#entries + 1] = { name = "browser", region = r } end
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

  -- The strip: the dots' count checked by the names read in its tabs, a count from elsewhere where
  -- the dots gave none, the names, then the rest.
  local texts = {}
  if count then
    for i = 1, count do texts[i] = textOf("osc " .. i) end
    local past = Strip.namesPast(texts, count)
    if past then
      log(string.format("strip: a tab's name reads OSC %d — the dots' count of %d is short, and not taken", past, count))
      count, texts, early = nil, {}, false
      Strip.unread(v)
    else
      Open.compareCount(count, ctx.window, v.preset, place)
    end
  end
  if not count then
    count, v.oscCountFrom = Open.countElsewhere(ctx.window, v.preset, place)
    if count then log(string.format("strip: %d oscillators, %s", count, Open.FROM[v.oscCountFrom])) end
  end
  if not count then
    local strip = byName["strip"]
    local found
    count, found = Strip.countFromText(strip and strip.status == "text" and strip.text or "")
    log(string.format("strip: the tabs read whole%s — %s", #found > 0 and (" name OSC " .. table.concat(found, ", ")) or "",
      strip and (count and (count .. " oscillator" .. (count == 1 and "" or "s")) or "no count") or "not asked"))
    if count then texts = Strip.namesFromWords(pl, strip and strip.words, count) end
  end
  v.oscCount = count
  if count then oscNames(texts, count, place and place.osc, v) end
  if not early then
    Strip.decide(pl, snap, got, count, v, log, marks.strip)
    if v.selectedType == "drums" and not slotNames then
      log("drums: the slots' names are not read — the strip's count was not the dots'")
    end
  end
  local central = textOf("central tabs")
  if central then log("centre: the captions read '" .. central .. "'") end
  if v.selectedType == "drums" then
    if not centreEarly then
      centre()
      log("centre: the sample browser's rows are not read — the strip's count was not the dots'")
    end
    Dr.decide(pl, got, slotNames and byName or nil, v, log, marks.drums)
    if Sm.shown(v) then Sm.decideText(byName, v, log, nil) end
  end
  middles(pl, marks)

  -- The macros: the count, the tab shown, its names, its buttons, its knobs.
  local counted = Macros.count(byName, v)
  Macros.selected(pl, got, v.macroCount, v, log, marks.macros)
  local vocab = place and place.macros and v.macroSelected and place.macros[v.macroSelected] or nil
  Macros.labels(byName, v, vocab, counted, log)
  Macros.buttons(pl, got, v, log, marks.macros)
  local amounts
  v.macroAmounts, amounts = Knob.macros(pl, snap, marks.macros)
  for i = 1, 3 do amounts[i] = string.format("knob %d %s", i, amounts[i]) end
  log("macros: " .. table.concat(amounts, "; "))
  Knob.oscMiddles(pl, marks.knob)
  if v.selectedType == "osc" then
    v.osc = {}
    log("oscillator: " .. Knob.osc(pl, snap, v.osc, marks.knob))
  end

  -- The browser (read/browser.luau, Br.state's order): the view's label where Avenger draws it; the
  -- header's name — none read there, the grid in the view stored, one the catalog or Avenger's own
  -- list has, that expansion open; and only where neither settles it, a second text read of the
  -- area, the one region the project read only then, and the largest by far (the expansion known
  -- open in this window scored first, the grid told by the catalog's expansions and Avenger's own
  -- list). FAV by its star column where an expansion reads open; and with an open expansion named,
  -- the category its preset rows read as and, with FAV on, their stars — from the presets' column,
  -- of the same picture.
  local reads = { ["expansion view"] = byName["expansion view"], ["expansion name"] = byName["expansion name"] }
  local seen, why = Br.state(reads, ctx.opts.expected)
  local areaRead = nil
  if not seen then
    local area = Expansion.area(pl)
    if area then
      areaRead = host.ocr.recognize({ name = "browser area", region = area }, { snapshot = snap, lang = H.lang() })
      reads["browser area"] = areaRead
      seen, why = Br.state(reads, ctx.opts.expected)
    end
  end
  log("expansion: " .. tostring(why))
  for _, p in ipairs({ { X.VIEW_TEXT[1], X.VIEW_TEXT[2] }, { X.VIEW_TEXT[3], X.VIEW_TEXT[4] }, { X.NAME_TEXT[1], X.NAME_TEXT[2] },
    { X.NAME_TEXT[3], X.NAME_TEXT[4] } }) do
    local x, y = pl:point(p[1], p[2])
    marks.browser[#marks.browser + 1] = { x = x, y = y }
  end
  if not seen then marks["browser-unsettled"] = marks.browser end
  if seen then
    v.expansionOpen = seen.open
    v.selectedExpansion = seen.open and (seen.name or "") or ""
    if not seen.open then v.expansionView, v.expansionViewFrom = seen.view, seen.viewFrom end
    -- Named by what it lists, the database's naming — unless it is the one this window had open by
    -- the word clicked, and the database's scoring only says it still is. A name read in the header
    -- is read.
    v.selectedFromDatabase = seen.score ~= nil and seen.name ~= nil
      and not (seen.name == ctx.opts.expected and not ctx.opts.expectedFromDatabase) or nil
    if seen.name then
      local ok, lists = pcall(DB.lists, seen.name)
      if ok and lists then
        v.lists = lists
        if place and place.expansion == seen.name then v.loadedIndex = place.index - 1 end
      end
    end
  end
  Expansion.favorite(pl, got, v, log, marks.browser, v.expansionOpen == true)
  if v.lists then
    local column = areaRead and Br.column(areaRead, pl:rect(X.PRESET_TEXT))
    if not column then column = Br.read(snap, pl, { "presets" })["presets"] end
    local lines = Br.lines(Br.rows(column), v.lists.presets, v.lists.presetCategories, "preset rows")
    v.categoryShown = Br.categoryOf(lines, v.lists.presetCategories)
    if v.favoriteView then
      Br.stars(snap, pl, lines, marks.browser)
      v.favoriteStates = Br.starStates(lines)
    end
  end

  pictures(pl, snap, marks, opts.zoom)
  v.cut = ctx.cut
  if ctx.cut then v.offScreen = Open.offScreen(pl, Open.PAGE_PARTS) end
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
-- at the key; expected = the expansion last read open in this window, or nil; expectedFromDatabase =
-- whether only the database named it; window = the id of the window the values are kept by (the
-- page's; Open.window() where not given) }. Answers the values read — nil where a
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
  -- Corners past what this module's snapshots may hold are answered nil and why; guarded all the
  -- same, for a call the host refuses as a mistake (docs/api/screen.md).
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
    tSnap = host.now(), cut = cut, window = opts.window or Open.window() }
  local all = {}
  Strip.ask(ctx.pl, all)
  Centre.ask(ctx.pl, all)
  DrumSq.ask(ctx.pl, all)
  Dr.ask(ctx.pl, all)
  Sm.ask(ctx.pl, all)
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
        ok, value, reason = pcall(host.screen.profile, { region = req.rect, axes = req.axes or "columns", snapshot = snap })
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
--   tab      the strip, the knobs, and with the Drums tab the central tab, DRUM SQ and the drumkit
--            table, its names read with `text` (`kitNames`): the page's Drums tab reads them after
--            its click
--   drums    the strip, and with the Drums tab the drumkit table (read/drums.luau): every slot's
--            Mute, Solo, Lock and Loop and the slot selected; the twelve names read with `text`
--            (`kitNames`)
--   editor   the strip, the central tab, the drumkit table (its names with `text`), and with the
--            Drums tab and EDITOR the sample editor (read/sample.luau): its tab shown, the editor
--            or the sample browser in its place, the tab's switch
--   central  the strip, and the central tab and DRUM SQ with any tab selected (`anyTab`), the
--            sample editor where it reads the Drums tab and EDITOR: what a choice of central tab
--            reads after its click (src/actions/drums.luau)
--   drumsq   the central tab and DRUM SQ with any tab of the strip selected, and no strip — the
--            page shows DRUM SQ whatever the strip shows (src/actions/drumsq.luau)
--   macros   the macro panel: the tab shown, its buttons and its knobs' amounts; the tabs counted
--            and the five names read with `text`
--   browser  the expansion browser (read/browser.luau, Br.part): the view's caption as text and
--            FAV's pixels; with the field "area", the area read where the caption reads no view, for
--            an open expansion; with "presets", the preset rows and their stars where FAV reads on
--            (`expected` in the opts: the expansion known open, whose lists the rows are placed in)
-- `fields` (Open.part's opts) names what is read as text besides: the sample editor's menus' fields,
-- the envelope's preset name and the browser's rows (read/sample.luau), DRUM SQ's LENGTH and SPEED
-- (read/drumsq.luau) — each asked only where the pixels read before the text read say it is shown.
Open.PARTS = {
  strip = { rect = { 230, 18, 640, 145 }, strip = true },
  knobs = { rect = { 230, 18, 640, 182 }, strip = true, knobs = true },
  tab = { rect = { 230, 18, 640, 268 }, strip = true, knobs = true, centre = true, drums = true, kitNames = true },
  drums = { rect = { 230, 18, 640, 232 }, strip = true, drums = true, kitNames = true },
  editor = { rect = { 230, 18, 640, 425 }, strip = true, centre = true, drums = true, kitNames = true, editor = true },
  central = { rect = { 230, 18, 640, 425 }, strip = true, centre = true, anyTab = true, editor = true },
  drumsq = { rect = { 236, 236, 640, 430 }, centre = true, anyTab = true },
  macros = { rect = { 700, 425, 871, 535 }, macros = true },
  browser = { rect = X.BROWSER, browser = true },
}

-- The keys of state.luau's values each part reads, whether it finds them or not: what it does not
-- find is not kept from an earlier read (S.part). The OSC names and the macro tabs' count and names
-- only where the part's text is read.
local STRIP_KEYS = { "oscCount", "oscCountFrom", "oscMute", "oscSolo", "selectedType", "selectedIndex", "drumsMute",
  "drumsSolo", "trigMute", "trigSolo", "drumPattern", "drumLock", "drumMode" }

-- ── A part past the screen's edge ────────────────────────────────────────────
-- The second Mac test's window reached below its 1080-point screen, and what lay there read "not
-- read": a part whose rectangle leaves the screen is said as that, with the way to bring it back —
-- in the page's error region (page.luau), and by the ring's controls and every action's refusal
-- (ring.luau, src/act.luau). Identification is not affected. Each part's words, and whether they
-- take "are"; the parts the page names, the browser and then the panel's from the top.
local PART_WORDS = {
  strip = { "the oscillator strip" }, knobs = { "the oscillator's knobs", true },
  tab = { "the selected tab's controls", true }, drums = { "the drum slots", true }, editor = { "the sample editor" },
  central = { "the central panel" }, drumsq = { "DRUM SQ" }, macros = { "the macros", true },
  browser = { "the expansion browser" },
}
Open.PAGE_PARTS = { "browser", "strip", "central", "macros" }

-- Whether part `name`'s rectangle with placer `pl` reaches past the primary display's bottom and its
-- right edge, while Avenger's corner is on that display (pictureRegion's rule) — as far as the
-- plug-in's panel goes; false, false where that cannot be told.
local function leaves(pl, name)
  local r = pl:rect(Open.PARTS[name].rect)
  local c = pl.origin and pl.origin.client
  local ok, s = pcall(host.screen.size)
  if not (r and c and ok and type(s) == "table" and type(s.w) == "number" and type(s.h) == "number") then
    return false, false
  end
  if c.x < 0 or c.y < 0 or c.x >= s.w or c.y >= s.h then return false, false end
  return math.min(r[4], c.y + c.h) > s.h, math.min(r[3], c.x + c.w) > s.w
end

-- What parts `names` that leave the screen say, in one sentence that begins in lower case, or nil
-- where none does: "the macros are below the screen; move the plug-in's window up".
function Open.offScreen(pl, names)
  local groups, order = {}, {}
  local up, left = false, false
  for _, n in ipairs(names) do
    local below, right = leaves(pl, n)
    if below or right then
      local key = (below and "b" or "") .. (right and "r" or "")
      if not groups[key] then
        groups[key] = {}
        order[#order + 1] = key
      end
      table.insert(groups[key], n)
      up, left = up or below, left or right
    end
  end
  if #order == 0 then return nil end
  local clauses = {}
  for _, key in ipairs(order) do
    local list = groups[key]
    local words = {}
    for i, n in ipairs(list) do words[i] = PART_WORDS[n][1] end
    local joined = #words == 1 and words[1] or (table.concat(words, ", ", 1, #words - 1) .. " and " .. words[#words])
    local plural = #list > 1 or PART_WORDS[list[1]][2] == true
    local where = key == "b" and "below the screen" or (key == "r" and "past the screen's right edge"
      or "below the screen and past its right edge")
    clauses[#clauses + 1] = joined .. (plural and " are " or " is ") .. where
  end
  return table.concat(clauses, "; ") .. "; move the plug-in's window "
    .. (up and left and "up and left" or (up and "up" or "left"))
end

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
  -- Cut where the panel is — by Avenger's corner, not the part's: a part wholly below the screen
  -- has its own corner past it, and is not on another display for that.
  local _, s = pictureRegion(c)
  if s then r = { r[1], r[2], math.min(r[3], s.w), math.min(r[4], s.h) } end
  if r[3] - r[1] < 2 or r[4] - r[2] < 2 then return nil end
  return r
end

-- Reads part `name` off snapshot `snap` (of Open.partRegion's rectangle), in a handler where its
-- questions wait. `opts`: { pl = the placer the picture was taken with, text = read the part's text
-- too, macroCount = the macro tabs counted before, for a part read without its text, preset = the
-- preset's name last read, whose tabs' names in the database are the names' vocabulary, fields = a
-- list of the fields to read as text where they are shown (above), window = the id of the window
-- the values are kept by (Open.window() where not given) }. Answers the values read
-- (state.luau's), and the keys the part read — a key of the list missing from the values was not
-- found. Nothing clicks; nothing but the snapshot is read. The strip, the central tab, DRUM SQ and
-- the sample editor's pixels are decided before the part's text is read, so that the text read asks
-- the slots' names only where the strip reads the Drums tab selected, and a field only where it is
-- shown.
function Open.part(snap, name, opts)
  local part = Open.PARTS[name]
  local pl = opts.pl
  if part.browser then return Br.part(snap, pl, opts) end
  local view = { snap.x, snap.y, snap.x + snap.w, snap.y + snap.h }
  local all = {}
  if part.strip then Strip.ask(pl, all) end
  if part.centre then
    Centre.ask(pl, all)
    DrumSq.ask(pl, all)
  end
  if part.drums then Dr.ask(pl, all) end
  if part.editor then Sm.ask(pl, all) end
  if part.macros then Macros.ask(pl, all) end
  local got = { why = {} }
  for _, req in ipairs(all) do
    if cutTo(req, view) then
      local ok, value, reason
      if req.kind == "cells" then
        ok, value, reason = pcall(host.screen.cells, { region = req.rect, cols = req.cols, rows = req.rows,
          predicate = req.pred, snapshot = snap })
      else
        ok, value, reason = pcall(host.screen.profile, { region = req.rect, axes = req.axes or "columns", snapshot = snap })
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
  local marks = { strip = {}, macros = {}, knob = {}, centre = {}, drumsq = {}, drums = {}, editor = {} }
  local count, from = nil, nil
  local window = opts.window or Open.window()
  if part.strip then
    read(STRIP_KEYS)
    count = Strip.count(pl, got, log, snap)
    if count then
      Open.compareCount(count, window, opts.preset, place)
    else
      count, from = Open.countElsewhere(window, opts.preset, place)
      if count then log(string.format("strip: %d oscillators, %s", count, Open.FROM[from])) end
    end
    v.oscCount, v.oscCountFrom = count, from
    Strip.decide(pl, snap, got, count, v, log, marks.strip)
  end
  if part.centre then
    read({ "central", "drumSq" })
    if v.selectedType == "drums" or part.anyTab then
      Centre.decide(pl, got, v, log, marks.centre)
      if v.central == "drum_sq" then DrumSq.decide(pl, got, v, log, marks.drumsq) end
    end
  end
  if part.editor then
    read(Sm.KEYS)
    if Sm.shown(v) then Sm.decidePixels(pl, snap, got, v, log, marks.editor) end
  end
  local kit = part.drums and v.selectedType == "drums"
  local names = opts.text and part.kitNames

  -- The fields asked, each only where what was just read shows it: the editor's on their tab,
  -- DRUM SQ's where it is the central tab. Those not asked, in one log line.
  local editorFields, sqFields, skipped = {}, {}, {}
  local askedEditor = false
  for _, key in ipairs(opts.fields or {}) do
    if DrumSq.FIELDS[key] then
      if part.centre then
        read({ DrumSq.FIELD_KEYS[key] })
        if v.central == "drum_sq" then
          sqFields[#sqFields + 1] = key
        else
          skipped[#skipped + 1] = string.format("'%s' not asked — DRUM SQ is not the central tab read", key)
        end
      end
    elseif part.editor then
      if key ~= "browser" then askedEditor = true end
      local fine, why = Sm.applies(key, v)
      if fine then
        editorFields[#editorFields + 1] = key
      else
        skipped[#skipped + 1] = string.format("'%s' not asked — %s", key, tostring(why))
      end
    end
  end
  if askedEditor then read(Sm.FIELD_KEYS) end
  if #skipped > 0 then log("fields: " .. table.concat(skipped, "; ")) end

  -- The one text read of the part's picture, where anything of it is asked.
  local byName = {}
  local entries = {}
  if opts.text then
    if part.strip and count then
      for i = 1, count do
        local r = pl:rect(T.tabSpan(i, count, T.NAME_TOP, T.NAME_BOTTOM))
        if r then entries[#entries + 1] = { name = "osc " .. i, region = r } end
      end
    end
    if part.macros then Macros.entries(pl, entries) end
    if kit and names then Dr.entries(pl, entries) end
  end
  for _, key in ipairs(editorFields) do
    local r = pl:rect(Sm.fieldRect(key))
    if r then entries[#entries + 1] = { name = Sm.regionName(key), region = r } end
  end
  for _, key in ipairs(sqFields) do
    local r = pl:rect(C.valueRect(key))
    if r then entries[#entries + 1] = { name = DrumSq.FIELDS[key], region = r } end
  end
  if #entries > 0 then
    local _, answers = host.ocr.recognize(entries, { snapshot = snap, lang = H.lang() })
    byName = answers or {}
  end
  if part.strip then
    if opts.text and count then
      read({ "oscNames", "oscSnapped" })
      local texts = {}
      for i = 1, count do
        local r = byName["osc " .. i]
        texts[i] = r and r.status == "text" and R.tidy(r.text) or nil
      end
      -- The dots' count checked by the names, as at the page key: a short one is not taken, and
      -- the strip is decided again by the count from elsewhere, or none.
      local past = from == nil and Strip.namesPast(texts, count)
      if past then
        log(string.format("strip: a tab's name reads OSC %d — the dots' count of %d is short, and not taken", past, count))
        Strip.unread(v)
        count, from = Open.countElsewhere(window, opts.preset, place)
        v.oscCount, v.oscCountFrom = count, from
        Strip.decide(pl, snap, got, count, v, log, marks.strip)
      else
        oscNames(texts, count, place and place.osc, v)
      end
    end
  end
  if part.knobs then
    read({ "osc" })
    if v.selectedType == "osc" then
      v.osc = {}
      log("oscillator: " .. Knob.osc(pl, snap, v.osc, marks.knob))
    end
  end
  if part.drums then
    read(Dr.KEYS)
    if names then read(Dr.NAME_KEYS) end
    if kit then Dr.decide(pl, got, names and byName or nil, v, log, marks.drums) end
  end
  if part.editor and Sm.shown(v) then
    local fields = {}
    for _, key in ipairs(editorFields) do
      if key ~= "browser" then fields[#fields + 1] = key end
    end
    Sm.decideText(byName, v, log, askedEditor and fields or nil)
  end
  if #sqFields > 0 then DrumSq.fields(byName, v, log, sqFields) end
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
    local amounts
    v.macroAmounts, amounts = Knob.macros(pl, snap, marks.macros)
    for i = 1, 3 do amounts[i] = string.format("knob %d %s", i, amounts[i]) end
    log("macros: " .. table.concat(amounts, "; "))
  end
  return v, keys
end

return Open
