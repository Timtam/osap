-- The page's reads when its key opens it: what Avenger shows, read off ONE picture of it.
--
--   1. In the page's `open` — the runtime's page key, a handler, while the header overlay is
--      active on an identified Avenger and nothing covers it (docs/api/overlay.md, O:addPage) — or
--      in a trip's continuation after Initialize: one snapshot of the plug-in's panel
--      (host.screen.snapshot, waited for).
--   2. Every question of the avenger_control project's classifiers (read/strip, centre, drumsq,
--      drums, sample, macros, expansion) asked of that picture. For the page's key (Open.read with
--      `done`, the runtime's page answered later: O:addPage's `openLater`, overlay runtime 0.14) all
--      at once with callbacks, and beside them the text read of what needs none of their answers —
--      the preset's name, the browser's header's name, the macro tabs' captions and names (the
--      database's checked: THE DATABASE FIRST, below) — so that neither waits for the other (the
--      second Mac test's 26 questions one after another took about 390 ms before the text read
--      began: A19). Without
--      `done` — Initialize's read in a trip's continuation — each waited for in turn, and that text
--      read with the rest. Each question is the image worker's work on the picture already taken.
--   3. Then the text read of what the answers place, of the same picture, with a named region for
--      each thing to read — the OSC tabs' names (one region per tab, the count from the dots; none
--      of a covered strip: read/strip.luau, A COVERED STRIP), the browser's view's caption where its
--      icon tells neither state, and with the Drums tab selected the sample browser's rows where
--      EDITOR shows no tab of the editor's (the strip, the central tab and the editor's pixels
--      decided before it when the dots gave the count) — waited for in the handler of the last
--      answer, and the knob reader over a few hundred points of the picture (read/knob.luau). The
--      drum slots' names are not read: they are Avenger's fixed roles (read/drums.luau, THE NAMES).
--      Nothing of the expansion browser below its header is read as text (read/browser.luau): its
--      state is the header's, the picture test's and the user's word (Br.state).
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
-- One press at a time: without `done` everything is waited for in the one handler, and the module's
-- next key waits in its mailbox meanwhile (docs/module-runtime-and-lifecycle.md, handlers); with
-- it, a second press reads afresh, its first text read superseding the earlier one's under their
-- one key (docs/api/ocr.md), and the runtime shows the latest press's answer alone (page.luau).

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

-- Whether a menu counts open over overlay `o` (O:menuOpen: a table lookup), false where it cannot
-- be asked: a read of the strip made then is covered (read/strip.luau, A COVERED STRIP).
function Open.menuOpen(o)
  local ok, open = pcall(function() return o and o.menuOpen and o:menuOpen() end)
  return ok and open == true
end

-- Where the dots give no count (read/strip.luau, by their spacing): the count the user confirmed
-- for this preset in window `id` (the page's Confirm OSC Count, the project's
-- action_confirm_osc_count: state.luau, S.oscConfirmed), else the preset database's for a preset
-- `place` (DB.place's) places by its words as they stand — not by similarity, and not after Load
-- or Save as, where the name may be the user's own file's — as the project's tool prefers its
-- database's count (its actions.py, _refresh_osc_panel_from_screen). The count and where it came
-- from (Open.FROM's key, its words for the log), or nil. Never shown as read: the page says a
-- confirmed one so, and the database's as the count alone (state.luau, `count_from`; the
-- maintainer's decision of 2026-10-26, src/presets.luau).
Open.FROM = { confirmed = "as confirmed for this preset", database = "by the preset database" }

-- Whether `place` (DB.place's) is one the database's tabs may be taken from: placed by its words as
-- they stand, not by similarity, and not after Load or Save as, with a tab in its file.
local function trusted(place)
  return place ~= nil and place.osc ~= nil and place.how ~= "fuzzy" and not place.own
    and #place.osc >= 1
end

function Open.countElsewhere(id, preset, place)
  local n = S.oscConfirmed(id, preset)
  if n then return n, "confirmed" end
  if trusted(place) then return math.min(8, #place.osc), "database" end
  return nil
end

-- ── The names from the database ──────────────────────────────────────────────
-- THE DATABASE FIRST (the maintainer's decision of 2026-10-31): the OSC tabs' names and the macros'
-- names — and the macro tabs' count — are the preset database's wherever it places the preset Avenger
-- shows as `trusted` takes a place (by its words as they stand, not after Load or Save as), as the
-- project's tool names them (its _current_macro_info, _osc_state). The screen is read for them ONCE
-- PER PAGE KEY, as a check: a read that is not near the database's name leaves that name marked,
-- ", from the database" (src/presets.luau, WHAT IT MAY DO), and the marked name is what the page and
-- the ring say until the preset may have changed (Open.keptNames, Open.keptLabels); an action of the
-- page's or the ring's reads no name. The evidence: the developer's Mac test of 2026-10-31 misread
-- the tabs' names under the level meter ("Penc" four times, "diy" for "Arp") and the macros' dot as
-- a word, and the second Windows session of the same day put an "o" before every macro name
-- ('o Flanger', 'Clearw') and read no macro count at all ('UACRO 1') — where the database had every
-- one of them. This reverses 0.14's order for the ring's Oscillator stepper, Mute and Solo and the
-- knobs' sentences, which named a tab by the name a read took off the screen first (wordsFor). Where
-- the database does not place the preset — Init Preset, a user's own preset, no database installed,
-- a count of tabs the database does not have, tabs or macros this session edited (S.tabsEdited,
-- S.macrosEdited) — the names are read as before.

-- The database's OSC tabs' names for `place`, a strip of `count` tabs in window `window` under the
-- preset's name `preset`, or nil where they are not taken (above).
function Open.dbOsc(place, count, window, preset)
  if not (count and trusted(place) and math.min(8, #place.osc) == count) then return nil end
  if S.tabsEdited(window, preset) then return nil end
  return place.osc
end

-- The database's macro tabs for `place` in window `window` under `preset` — a list of five names
-- each — or nil where they are not taken (above).
function Open.dbMacros(place, window, preset)
  if not (place and place.macros and #place.macros >= 1 and place.how ~= "fuzzy" and not place.own) then
    return nil
  end
  if S.macrosEdited(window, preset) then return nil end
  return place.macros
end

-- The macros' names `known` keeps from the page key's check, where they were taken for the preset
-- `preset` on macro tab `tab` and the preset has not changed since (`macroLabelsOf`, kept with
-- them, as the OSC tabs' names are: Open.keptNames) — the names, marks and all — else nil.
function Open.keptLabels(known, preset, tab)
  local of = known and known.macroLabelsOf
  if not (of and preset ~= nil and of.preset == preset and of.tab == tab and of.at == S.changes()
    and known.macroLabels) then return nil end
  return known.macroLabels
end

-- The tabs' names `known` (state.luau's values) keeps from an earlier read, where they were read
-- under the preset's name `preset` — a name, never nil — over `count` tabs, and since the preset
-- last may have changed (`oscNamesOf`, kept with them; S.changes): the names, else nil. The values
-- keep a name across ◀ and ▶, Load and Undo, which replace only the preset's (S.set); a name read
-- under another preset's name, or before a tab was added or deleted, is another tab's, and one read
-- before ▶ and ◀ back to the same name may be a name the file does not have.
function Open.keptNames(known, preset, count)
  local of = known and known.oscNamesOf
  if not (of and preset ~= nil and of.preset == preset and of.count == count and of.at == S.changes()
    and known.oscNames) then return nil end
  return known.oscNames
end

-- The same, where they were read off the screen — not the database's (THE DATABASE FIRST, above:
-- `oscNamesOf.db`): what a rename of one tab keeps of the others, after which the database's names
-- name none of the preset's tabs (state.luau, S.tabsEdited).
function Open.keptReadNames(known, preset, count)
  local names = Open.keptNames(known, preset, count)
  if names and known.oscNamesOf.db then return nil end
  return names
end

-- The words for the `count` OSC tabs of read `v` (`v.oscWords`), which the module's own sentences
-- name a tab by (src/actions/osc.luau, Osc.oscName: the ring's Oscillator stepper, Mute and Solo,
-- a knob's sentence, an action's answer), as the page names its tabs. Where the preset database's
-- names are taken (Open.dbOsc: THE DATABASE FIRST, above) the database's — as the page key's check
-- left them, marked where it disagreed, while window `window`'s values keep them under the same
-- preset's name `preset` and count (Open.keptNames) — whatever a read took off the screen: the
-- order of 0.14, the screen's name first, reversed by the maintainer's decision of 2026-10-31.
-- Elsewhere the name this read took off the screen, else the one the window's values keep from an
-- earlier read; else a hole, said as Avenger's "OSC i". Said as they are; a name of the database's
-- is marked only where the page key's read disagreed with it (src/presets.luau). The ring reads no
-- text of the strip (ring.luau), so these are its names; nothing more is read or looked up for
-- them: the place is the one the read places anyway, and the rest is kept already.
local function wordsFor(v, count, preset, place, window)
  if not count then return nil end
  local kept = Open.keptNames(S.values(window), preset, count)
  local db = Open.dbOsc(place, count, window, preset)
  local out = {}
  for i = 1, count do
    local read = v.oscNames and v.oscNames[i]
    if db then
      out[i] = (kept and kept[i]) or db[i]
    else
      out[i] = read or (kept and kept[i]) or nil
    end
  end
  return out
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
-- picture the reads were answered from; and "browser-unsettled" once per zoom where the caption,
-- the header and the picture test together settled nothing (read/browser.luau, Br.state) — the
-- pictures the second Mac test's plan asked for.
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

-- The OSC tabs' names into `v.oscNames`, from `texts` (tab -> text read, nil for none), each
-- cleaned the project's way; and in `v.oscNamesOf` what they were taken under: the preset's name
-- `preset` (nil: not known), the count and S.changes() (Open.keptNames). Where the database's names
-- are taken (`db`, Open.dbOsc's: THE DATABASE FIRST, above) they are the names, and the text read —
-- the page key's check — only marks one it is not near (R.snap), ", from the database". Elsewhere
-- the names read, checked against the database's names of the preset's tabs (`vocab`, or nil) — in
-- the database's spelling where the read was near it, said in the log and nowhere else
-- (src/presets.luau, WHAT IT MAY DO).
local function oscNames(texts, count, vocab, v, preset, db)
  v.oscNames = {}
  -- `db`: the names are the database's (Open.keptReadNames).
  v.oscNamesOf = { preset = preset, count = count, at = S.changes(), db = db ~= nil or nil }
  local words = {}
  for i = 1, count do
    local raw = texts[i]
    local clean = raw and R.tidy(raw) ~= "" and R.cleanOscName(raw, i) or nil
    local cleaned = clean and clean ~= R.tidy(raw) and (" -> '" .. clean .. "'") or ""
    if db then
      local name = db[i]
      if clean then
        local _, snapped, ratio = R.snap(clean, name)
        if snapped or ratio == 1 then
          v.oscNames[i] = name
          words[#words + 1] = string.format("%d '%s'%s, the database's '%s' (%.2f alike)", i, R.forLog(raw), cleaned,
            name, ratio)
        else
          v.oscNames[i] = name .. ", " .. DB.MARK
          words[#words + 1] = string.format("%d '%s'%s, not the database's '%s' (%.2f alike) — the database's, marked",
            i, R.forLog(raw), cleaned, name, ratio or 0)
        end
      else
        v.oscNames[i] = name
        words[#words + 1] = string.format("%d '%s', the database's (not read)", i, name)
      end
    elseif clean then
      local name, snapped, ratio = R.snap(clean, vocab and vocab[i])
      v.oscNames[i] = name
      words[#words + 1] = string.format("%d '%s'%s%s", i, R.forLog(raw), cleaned,
        snapped and string.format(" -> '%s', the database's (%.2f alike)", name, ratio) or "")
    else
      words[#words + 1] = string.format("%d not read", i)
    end
  end
  log("strip: the tabs' names " .. table.concat(words, "; "))
end

-- The same names from the database alone, where an action reads none (THE DATABASE FIRST): the page
-- key's checked names that window `window` keeps for the same preset and count (Open.keptNames),
-- marks and all, else the database's `db`. Logged in one line.
local function oscNamesKept(count, db, v, preset, window)
  local kept = Open.keptNames(S.values(window), preset, count)
  v.oscNames = {}
  v.oscNamesOf = { preset = preset, count = count, at = S.changes(), db = true }
  for i = 1, count do v.oscNames[i] = (kept and kept[i]) or db[i] end
  log(string.format("strip: the tabs' names not read — the preset database's%s: %s",
    kept and ", as the page key's read checked them" or "", table.concat(v.oscNames, ", ")))
end

-- The text regions that need no question's answer — the preset's name, the browser's view and its
-- header's name, the macro tabs' captions and the five names of the tab shown (THE DATABASE FIRST:
-- read at the page key as the check) — appended to `entries`. Not the central tabs' captions, read
-- until 0.16 for a log line alone: the central tab is its bottom strip's pixels (read/centre.luau),
-- and the captions read 'EDITOR ARP ORU SQ STEP SQ' on Windows (mt-verified, finding 13).
local function firstEntries(pl, entries)
  local nameRect = pl:rect(G.NAME_REGION)
  if nameRect then entries[#entries + 1] = { name = "preset", region = nameRect } end
  -- The browser's header's name; its view's caption only in the text read after the questions,
  -- where the view's icon told neither state (read/expansion.luau, THE VIEW'S ICON).
  Expansion.entries(pl, entries, false)
  Macros.entries(pl, entries)
  return entries
end

-- The work once every question has answered: the text read, the decisions, the pictures. Runs in
-- the handler the questions were waited for in — or, with Open.read's `done`, the handler of the
-- last answer — where the text read waits too; what `ctx.early` read already is not read again.
local function decide(ctx)
  local pl, snap, got, opts = ctx.pl, ctx.snap, ctx.got, ctx.opts
  local v = { zoom = opts.zoom }
  local marks = { strip = {}, macros = {}, knob = {}, centre = {}, drumsq = {}, browser = {}, drums = {},
    sample = {}, ["sq-controls"] = {} }

  -- How many oscillators the dots say, which places the tabs' names. With a count from the dots the
  -- strip is decided before the text read, which then asks the sample browser's rows where the
  -- central tab reads EDITOR on the Drums tab; a count from elsewhere keeps the strip's decision
  -- after it. A covered strip (read/strip.luau, A COVERED STRIP) — no dot at OSC 1's place, a menu
  -- counting open over the overlay, MENU's items' words in its text — decides nothing: only a count
  -- that does not come from the screen, and no text of the strip is asked.
  local count, dots, dotsRead = Strip.count(pl, got, log, snap)
  local cover = Strip.coverOf(pl, dots, dotsRead, ctx.menuOpen)
  if cover then count = nil end
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

  -- The text read of the picture: what the early read (ctx.early) did not read already — and the
  -- browser's view's caption where its icon tells neither state. The drum slots' names are not read
  -- (read/drums.luau, THE NAMES: Avenger's twelve fixed roles).
  local entries = firstEntries(pl, {})
  local look = Expansion.look(pl, got)
  if look.icon == nil then Expansion.captionEntry(pl, entries) end
  if cover then
    -- Nothing of a covered strip is read as text.
  elseif count then
    for i = 1, count do
      local r = pl:rect(T.tabSpan(i, count, T.NAME_TOP, T.NAME_BOTTOM))
      if r then entries[#entries + 1] = { name = "osc " .. i, region = r } end
    end
  else
    local r = pl:rect(T.AREA)
    if r then entries[#entries + 1] = { name = "strip", region = r } end
  end
  if centreEarly and Sm.applies("browser", v) then
    local r = pl:rect(Sm.fieldRect("browser"))
    if r then entries[#entries + 1] = { name = "browser", region = r } end
  end
  local byName = {}
  local rest = entries
  if ctx.early then
    rest = {}
    for k, r in pairs(ctx.early) do byName[k] = r end
    for _, e in ipairs(entries) do
      if ctx.early[e.name] == nil then rest[#rest + 1] = e end
    end
  end
  if #rest > 0 then
    local _, names = host.ocr.recognize(rest, { snapshot = snap, lang = H.lang() })
    for k, r in pairs(names or {}) do byName[k] = r end
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

  -- The preset's name, the project's way with its blank preset and a bullet word before it, and
  -- where the database has it. A name read only in boxes the recogniser guessed is none, unless the
  -- database has it as it stands (main.luau, nameOf: PaddleOCR alone invented every one).
  local name = textOf("preset")
  if name then
    local taken, init = R.presetName(name)
    if init and taken ~= name then log(string.format("the name read '%s' is taken as Avenger's blank preset, %s", name, taken)) end
    if R.guessedOnly(byName["preset"]) and not DB.catalogName(taken) then
      log(string.format("the name read '%s' was read only in boxes the recogniser guessed, and is no catalog name "
        .. "as it stands — not taken", name))
    else
      v.preset = taken
    end
  end
  local place = nil
  if v.preset and DB.present then
    local ok, p = pcall(DB.place, v.preset)
    if ok then place = p else log("the preset database failed: " .. tostring(p)) end
  end
  if place then v.expansion = place.expansion end

  -- The strip: the dots' count checked by the names read in its tabs, a count from elsewhere where
  -- the dots gave none, the names, then the rest. MENU's items' words in the strip's text are a
  -- cover found late: what was decided off the picture is dropped again.
  local texts = {}
  if count then
    for i = 1, count do texts[i] = textOf("osc " .. i) end
  end
  if not cover then
    local word = Strip.menuWords(textOf("strip"))
    for i = 1, count or 0 do word = word or Strip.menuWords(texts[i]) end
    if word then
      cover = "its text holds MENU's item '" .. word .. "'"
      count, texts, early = nil, {}, false
      for _, k in ipairs({ "central", "drumSq" }) do v[k] = nil end
      for _, k in ipairs(Sm.KEYS) do v[k] = nil end
      centreEarly = false
    end
  end
  if cover then Strip.covered(v, cover, log) end
  if count then
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
  if not count and not cover then
    local strip = byName["strip"]
    local found
    count, found = Strip.countFromText(strip and strip.status == "text" and strip.text or "")
    log(string.format("strip: the tabs read whole%s — %s", #found > 0 and (" name OSC " .. table.concat(found, ", ")) or "",
      strip and (count and (count .. " oscillator" .. (count == 1 and "" or "s")) or "no count") or "not asked"))
    if count then texts = Strip.namesFromWords(pl, strip and strip.words, count) end
  end
  v.oscCount = count
  if count and not cover then
    local preset = v.preset or opts.preset
    oscNames(texts, count, place and place.osc, v, preset, Open.dbOsc(place, count, ctx.window, preset))
  end
  if not early and not cover then Strip.decide(pl, snap, got, count, v, log, marks.strip) end
  if v.selectedType == "drums" then
    if not centreEarly then
      centre()
      log("centre: the sample browser's rows are not read — the strip's count was not the dots'")
    end
    Dr.decide(pl, got, v, log, marks.drums)
    if Sm.shown(v) then Sm.decideText(byName, v, log, nil) end
  end
  middles(pl, marks)

  -- The macros: the count, the tab shown, its names, its buttons, its knobs — the count and the
  -- names the database's where it has the preset, the captions and names read as their check (THE
  -- DATABASE FIRST); and what the names were taken under (`macroLabelsOf`, Open.keptLabels).
  local dbTabs = Open.dbMacros(place, ctx.window, v.preset)
  local counted = Macros.count(byName, v, dbTabs)
  Macros.selected(pl, got, v.macroCount, v, log, marks.macros)
  Macros.labels(pl, byName, v, dbTabs and v.macroSelected and dbTabs[v.macroSelected] or nil, counted, log)
  v.macroLabelsOf = { preset = v.preset, tab = v.macroSelected, at = S.changes() }
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
  -- header's name — one the catalog or the roster has, that expansion open; the picture test of its
  -- title band and the categories' scrollbar (read/expansion.luau, E.look), of the grids asked with
  -- the rest; and the user's word, the grid confirmed in this window. FAV by its star column where an
  -- expansion reads open; and with FAV on, the stars of the rows this session's own clicks placed.
  local reads = { ["expansion view"] = byName["expansion view"], ["expansion name"] = byName["expansion name"] }
  local seen, why = Br.state(reads, look, { window = ctx.window, zoom = opts.zoom, expected = ctx.opts.expected })
  log("expansion: " .. tostring(why))
  Br.gateLine("the page key", { window = ctx.window, pl = pl, byName = reads, look = look, st = seen, words = why,
    zoom = opts.zoom }, "no click — the state posted to the page")
  for _, p in ipairs({ { X.VIEW_TEXT[1], X.VIEW_TEXT[2] }, { X.VIEW_TEXT[3], X.VIEW_TEXT[4] }, { X.NAME_TEXT[1], X.NAME_TEXT[2] },
    { X.NAME_TEXT[3], X.NAME_TEXT[4] } }) do
    local x, y = pl:point(p[1], p[2])
    marks.browser[#marks.browser + 1] = { x = x, y = y }
  end
  if not seen then marks["browser-unsettled"] = marks.browser end
  if seen then
    v.expansionOpen = seen.open
    v.selectedExpansion = seen.open and (seen.name or "") or ""
    if not seen.open then
      v.expansionView, v.expansionViewFrom = seen.view, seen.viewFrom
      S.forgetBrowserAt(ctx.window, "the page key reads the grid")
    elseif seen.name and seen.name ~= ctx.opts.expected then
      S.forgetBrowserAt(ctx.window, "the page key reads " .. seen.name)
    end
    -- The name read in the header, or the one at its place in the database's list where the header
    -- did not read it (Br.state): its lists either way, and posted as any name is.
    if seen.name then
      local ok, lists = pcall(DB.lists, seen.name)
      if ok and lists then
        v.lists = lists
        if place and place.expansion == seen.name then v.loadedIndex = place.index - 1 end
      end
    end
  end
  Expansion.favorite(pl, got, v, log, marks.browser, v.expansionOpen == true)
  if v.lists and v.favoriteView then
    local rows = Br.shownRows(pl, ctx.window, v.selectedExpansion, v.lists)
    if rows then
      Br.stars(snap, pl, rows, marks.browser)
      v.favoriteStates = Br.starStates(rows)
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
-- at the key; expected = the expansion last read open in this window, or nil; window = the id of
-- the window the values are kept by (the page's; Open.window() where not given); preset = the name
-- last read, which the OSC tabs' names are kept as read under (`oscNamesOf`) where the picture's
-- own name reads nothing, or nil }. Answers the values read — nil where a read failed — or nil,
-- why nothing was read for the log, and the sentence to say (Open.NO_FACTOR,
-- NO_PICTURE, FAILED; nil when the user moved on). `values.zoom` is always there, and `values.cut`
-- is the screen's size when the picture was cut at its edge.
--
-- With `done`, the questions and the first text read asked at once with callbacks (above, 2), and
-- `done(values)` — or `done(nil, why, sentence)` — once, on a later turn as the handler of the last
-- answer, or at once where nothing could be asked (no factor, no picture, the user moved on); it
-- returns nothing then. The picture is let go once every answer is in; a module disabled meanwhile
-- hears nothing more.
function Open.read(o, opts, done)
  local t0 = host.now()
  -- Nothing read: answered, or handed to `done` at once.
  local function none(why, words)
    if done then return done(nil, why, words) end
    return nil, why, words
  end
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then
    return none("the plug-in cannot be placed now (no factor)", Open.NO_FACTOR)
  end
  local region, cut = pictureRegion(origin.client)
  -- Corners past what this module's snapshots may hold are answered nil and why; guarded all the
  -- same, for a call the host refuses as a mistake (docs/api/screen.md).
  local taken, snap, why = pcall(host.screen.snapshot, { region = region })
  if not (taken and snap) then
    return none("no picture of " .. corners(region) .. ": " .. tostring(taken and why or snap), Open.NO_PICTURE)
  end
  local fine, gone = o:stillHere(opts.mark)
  if not fine then
    snap:release()
    return none("the user moved on while the picture was taken — " .. tostring(gone))
  end
  if cut then
    log(string.format("the picture: %s, cut at the edge of the %dx%d screen — the panel ends at %d,%d; what lies "
      .. "past the edge is not read", corners(region), cut.w, cut.h, math.ceil(origin.client.x + origin.client.w),
      math.ceil(origin.client.y + origin.client.h)))
  end
  local ctx = { pl = P.placer(origin, k, opts.zoom), snap = snap, opts = opts, got = { why = {} }, t0 = t0,
    tSnap = host.now(), cut = cut, window = opts.window or Open.window(), menuOpen = Open.menuOpen(o) }
  local all = {}
  Strip.ask(ctx.pl, all)
  Centre.ask(ctx.pl, all)
  DrumSq.ask(ctx.pl, all)
  Dr.ask(ctx.pl, all)
  Sm.ask(ctx.pl, all)
  Macros.ask(ctx.pl, all)
  Expansion.ask(ctx.pl, all)
  -- An answer `value` (or nil and `reason`) of request `req`, as the classifiers take it.
  local function took(req, ok, value, reason)
    if not ok then
      ctx.got.why[req.name] = tostring(value)
    elseif value ~= nil then
      local made, got = pcall(function() return req.kind == "cells" and P.grid(req, value) or value end)
      if made then ctx.got[req.name] = got else ctx.got.why[req.name] = tostring(got) end
    else
      ctx.got.why[req.name] = reason
    end
  end
  -- Request `req` asked of the snapshot, with callback `cb` (nil: waited for).
  local function ask(req, cb)
    if req.kind == "cells" then
      return host.screen.cells({ region = req.rect, cols = req.cols, rows = req.rows, predicate = req.pred,
        snapshot = snap }, cb)
    end
    return host.screen.profile({ region = req.rect, axes = req.axes or "columns", snapshot = snap }, cb)
  end
  local asked = 0
  -- The decisions, once every answer is in; the picture let go; the times in the log.
  local function finish()
    ctx.tPixels = ctx.tPixels or host.now()
    local ok, v = pcall(decide, ctx)
    snap:release()
    if not ok then
      log("the reads failed: " .. tostring(v))
      return nil, "the reads failed", Open.FAILED
    end
    if ctx.early then
      log(string.format("%d questions of one picture; the picture %d ms after the key, the questions %d ms and the "
        .. "first text read %d ms beside them, the rest of the text %d ms, the rest %d ms", asked, ctx.tSnap - t0,
        ctx.tAnswers - ctx.tSnap, (ctx.tEarly or ctx.tSnap) - ctx.tSnap, (ctx.tText or ctx.tPixels) - ctx.tPixels,
        host.now() - (ctx.tText or ctx.tPixels)))
    else
      log(string.format("%d questions of one picture; the picture %d ms after the key, the questions %d ms, the "
        .. "text %d ms, the rest %d ms", asked, ctx.tSnap - t0, ctx.tPixels - ctx.tSnap, (ctx.tText or ctx.tPixels)
          - ctx.tPixels, host.now() - (ctx.tText or ctx.tPixels)))
    end
    return v
  end
  if not done then
    -- Each asked of what the picture holds of it, and waited for; guarded, so that every answer is
    -- counted whatever it holds.
    for _, req in ipairs(all) do
      if not cut or cutTo(req, region) then
        asked += 1
        took(req, pcall(ask, req, nil))
      else
        ctx.got.why[req.name] = "past the edge of the screen"
      end
    end
    return finish()
  end
  -- With `done`: every question and the first text read asked at once, each counted out until its
  -- answer comes; the rest in the handler of the last.
  local out, over = 0, false
  local function settle()
    if out > 0 or over then return end
    over = true
    ctx.tAnswers = host.now()
    done(finish())
  end
  for _, req in ipairs(all) do
    if not cut or cutTo(req, region) then
      asked += 1
      out += 1
      local okA, err = pcall(ask, req, function(value, reason)
        took(req, true, value, reason)
        out -= 1
        settle()
      end)
      if not okA then
        took(req, false, err)
        out -= 1
      end
    else
      ctx.got.why[req.name] = "past the edge of the screen"
    end
  end
  local early = firstEntries(ctx.pl, {})
  ctx.early = {}
  if #early > 0 then
    out += 1
    local okT, errT = pcall(host.ocr.recognize, early, { snapshot = snap, lang = H.lang(), key = "vps-avenger page" },
      function(_, byName)
        ctx.early = byName or {}
        ctx.tEarly = host.now()
        out -= 1
        settle()
      end)
    if not okT then
      log("the first text read was not asked: " .. tostring(errT))
      out -= 1
    end
  end
  settle()
end

-- ── A part of Avenger, read again ────────────────────────────────────────────
-- What an action of the page or the ring reads before its click and after it (src/act.luau): only
-- the part it acts on, off a picture of that part. Each a rectangle at 50 % from Avenger's corner,
-- and what is read in it:
--   strip    the OSC tabs, Drums and Trig: the count, the tab selected, every Mute and Solo, the
--            Drums tab's switches and mode (read/strip.luau)
--   knobs    the strip, and the selected oscillator's knobs (read/knob.luau)
--   tab      the strip, the knobs, and with the Drums tab the central tab, DRUM SQ and the drumkit
--            table: what a choice of tab reads after its click
--   drums    the strip, and with the Drums tab the drumkit table (read/drums.luau): every slot's
--            Mute, Solo, Lock and Loop and the slot selected — no slot's name, which is a fixed role
--            (read/drums.luau, THE NAMES, since 0.16)
--   editor   the strip, the central tab, the drumkit table, and with the Drums tab and EDITOR the
--            sample editor (read/sample.luau): its tab shown, the editor or the sample browser in its
--            place, the tab's switch
--   central  the strip, and the central tab and DRUM SQ with any tab selected (`anyTab`), the
--            sample editor where it reads the Drums tab and EDITOR: what a choice of central tab
--            reads after its click (src/actions/drums.luau)
--   drumsq   the central tab and DRUM SQ with any tab of the strip selected, and no strip — the
--            page shows DRUM SQ whatever the strip shows (src/actions/drumsq.luau)
--   macros   the macro panel: the tab shown, its buttons and its knobs' amounts; the tabs counted
--            and the five names read with `text`
--   browser  the expansion browser (read/browser.luau, Br.part): the view's caption and the header's
--            name as text, the picture test, FAV's pixels; with "presets", the stars of the rows this
--            session's own clicks placed where FAV reads on (`expected` in the opts: the expansion
--            known open)
-- `fields` (Open.part's opts) names what is read as text besides: the sample editor's menus' fields,
-- the envelope's preset name and the sample browser's rows (read/sample.luau), DRUM SQ's LENGTH and SPEED
-- (read/drumsq.luau) — each asked only where the pixels read before the text read say it is shown.
Open.PARTS = {
  strip = { rect = { 230, 18, 640, 145 }, strip = true },
  knobs = { rect = { 230, 18, 640, 182 }, strip = true, knobs = true },
  tab = { rect = { 230, 18, 640, 268 }, strip = true, knobs = true, centre = true, drums = true },
  drums = { rect = { 230, 18, 640, 232 }, strip = true, drums = true },
  editor = { rect = { 230, 18, 640, 425 }, strip = true, centre = true, drums = true, editor = true },
  central = { rect = { 230, 18, 640, 425 }, strip = true, centre = true, anyTab = true, editor = true },
  drumsq = { rect = { 236, 236, 640, 430 }, centre = true, anyTab = true },
  macros = { rect = { 700, 425, 871, 535 }, macros = true },
  browser = { rect = X.BROWSER, browser = true },
}

-- The keys of state.luau's values each part reads, whether it finds them or not: what it does not
-- find is not kept from an earlier read (S.part). The OSC names and the macro tabs' count and names
-- only where the part's text is read; the words for the tabs (wordsFor) and the preset's name they
-- were taken for with every read of the strip, since they need no text; and how the selection's read
-- went (`selectionState`: read/strip.luau, S.decide) and whether the strip was covered
-- (`stripCovered`), which the ring and the page keep the Drums tab's controls by (ring.luau,
-- state.luau's S.drumsKept).
local STRIP_KEYS = { "oscCount", "oscCountFrom", "oscMute", "oscSolo", "selectedType", "selectedIndex", "drumsMute",
  "drumsSolo", "trigMute", "trigSolo", "drumPattern", "drumLock", "drumMode", "oscWords", "oscWordsOf",
  "selectionState", "stripCovered" }
Open.STRIP_KEYS = STRIP_KEYS

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
-- preset's name last read, whose tabs' names in the database are the names' vocabulary and, text
-- read or not, the words for the tabs (wordsFor), fields = a list of the fields to read as text
-- where they are shown (above), window = the id of the window the values are kept by
-- (Open.window() where not given), whose names kept are words too, readCount = the macro tabs'
-- captions read for the count even where the database has it, screenNames = the OSC tabs' names
-- read even where the database has them (src/act.luau) }. Where the preset database has the preset,
-- the names are its own and are not read (THE DATABASE FIRST, above). Answers the values read
-- (state.luau's), and the keys the part read — a key of the list missing from the values was not
-- found. Nothing clicks; nothing but the snapshot is read. The strip, the central tab, DRUM SQ and
-- the sample editor's pixels are decided before the part's text is read, so that the text read asks
-- a field only where it is shown. `menuOpen` (opts): a menu counts open over the overlay the read is
-- made for (Open.menuOpen) — the strip read then is covered, as one with no dot at OSC 1's place or
-- with MENU's items' words in its names (read/strip.luau, A COVERED STRIP): nothing of it decided,
-- no name of it read, only a count from elsewhere.
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
  local cover = nil
  if part.strip then
    read(STRIP_KEYS)
    local dots, dotsRead
    count, dots, dotsRead = Strip.count(pl, got, log, snap)
    cover = Strip.coverOf(pl, dots, dotsRead, opts.menuOpen)
    if cover then count = nil end
    if count then
      Open.compareCount(count, window, opts.preset, place)
    else
      count, from = Open.countElsewhere(window, opts.preset, place)
      if count then log(string.format("strip: %d oscillators, %s", count, Open.FROM[from])) end
    end
    v.oscCount, v.oscCountFrom = count, from
    if cover then
      Strip.covered(v, cover, log)
    else
      Strip.decide(pl, snap, got, count, v, log, marks.strip)
    end
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

  -- The database's names where they are taken (THE DATABASE FIRST): the OSC tabs' — unless the
  -- action compares what an edit of the tabs did to them (`screenNames`, an OSC tab's menu) — and the
  -- macros'; then none of those is read.
  local dbNames = part.strip and count and not cover and not opts.screenNames
    and Open.dbOsc(place, count, window, opts.preset) or nil
  local dbTabs = part.macros and Open.dbMacros(place, window, opts.preset) or nil

  -- The one text read of the part's picture, where anything of it is asked: the macro tabs'
  -- captions where the count is not the database's, or the action counts them (`readCount`, Add and
  -- Delete macro tab), and the names where they are not.
  local byName = {}
  local entries = {}
  if opts.text then
    if part.strip and count and not dbNames and not cover then
      for i = 1, count do
        local r = pl:rect(T.tabSpan(i, count, T.NAME_TOP, T.NAME_BOTTOM))
        if r then entries[#entries + 1] = { name = "osc " .. i, region = r } end
      end
    end
    if part.macros then Macros.entries(pl, entries, dbTabs == nil or opts.readCount == true, dbTabs == nil) end
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
    if opts.text and count and dbNames then
      read({ "oscNames", "oscNamesOf" })
      oscNamesKept(count, dbNames, v, opts.preset, window)
    elseif opts.text and count and not cover then
      read({ "oscNames", "oscNamesOf" })
      local texts = {}
      local word = nil
      for i = 1, count do
        local r = byName["osc " .. i]
        texts[i] = r and r.status == "text" and R.tidy(r.text) or nil
        word = word or Strip.menuWords(texts[i])
      end
      -- The dots' count checked by the names, as at the page key: a short one is not taken, and
      -- the strip is decided again by the count from elsewhere, or none. MENU's items' words in them
      -- are a cover found late: nothing of the strip decided stands.
      local past = from == nil and Strip.namesPast(texts, count)
      if word then
        cover = "its names hold MENU's item '" .. word .. "'"
        count, from = Open.countElsewhere(window, opts.preset, place)
        v.oscCount, v.oscCountFrom = count, from
        Strip.covered(v, cover, log)
        v.central, v.drumSq = nil, nil
        for _, k in ipairs(Sm.KEYS) do v[k] = nil end
        kit = false
      elseif past then
        log(string.format("strip: a tab's name reads OSC %d — the dots' count of %d is short, and not taken", past, count))
        Strip.unread(v)
        count, from = Open.countElsewhere(window, opts.preset, place)
        v.oscCount, v.oscCountFrom = count, from
        Strip.decide(pl, snap, got, count, v, log, marks.strip)
      else
        oscNames(texts, count, place and place.osc, v, opts.preset)
      end
    end
    v.oscWords, v.oscWordsOf = wordsFor(v, v.oscCount, opts.preset, place, window), opts.preset
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
    if kit then Dr.decide(pl, got, v, log, marks.drums) end
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
    -- The preset's name the read was taken for, for an action's fact (src/actions/macros.luau).
    v.macrosOf = opts.preset
    local counted = nil
    if opts.text then
      read({ "macroCount", "macroLabels", "macroLabelsOf" })
      -- An action that counts the tabs (`readCount`) takes the captions read first, the database's
      -- where they read none; every other the database's first (Macros.count).
      if opts.readCount and dbTabs then
        counted = Macros.count(byName, v, nil)
        if v.macroCount == nil then counted = Macros.count(byName, v, dbTabs) end
      else
        counted = Macros.count(byName, v, dbTabs)
      end
    else
      v.macroCount = opts.macroCount or (dbTabs and math.min(4, #dbTabs)) or nil
    end
    Macros.selected(pl, got, v.macroCount, v, log, marks.macros)
    local tab = v.macroSelected
    local db5 = dbTabs and tab and dbTabs[tab] or nil
    local kept = db5 and Open.keptLabels(S.values(window), opts.preset, tab) or nil
    if opts.text then
      Macros.labels(pl, byName, v, db5, counted, log, kept)
      v.macroLabelsOf = { preset = opts.preset, tab = tab, at = S.changes() }
    else
      -- Counted before, not read here: not part of what this read says. The database's names, where
      -- it has them, are words for the action's answer (a button's name), cost nothing, and are not
      -- part of it either: no key of theirs.
      v.macroCount = nil
      if db5 then
        v.macroLabels = {}
        for i = 1, 5 do v.macroLabels[i] = (kept and kept[i]) or db5[i] end
      end
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
