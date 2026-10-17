-- The page's renames and Browse Presets (the plan's slice 6): the avenger_control tool's rename and
-- return modes, as hand-overs of the overlay runtime's (trip:handOver, docs/api/overlay.md). The
-- user works in Avenger's own editor or browser, and what Avenger shows after it is READ: this
-- module records, sends and types no key (the maintainer's decision of 2026-10-12).
--
--   start_rename      the field double-clicked where the tool double-clicks it — the preset's name,
--   rename_osc        the OSC tab, the macro knob's or button's field — and Avenger's editor seen
--   rename_macro      open in a picture of the field (read/editor.luau); the keyboard handed over,
--                     nothing said — the tool says nothing either, and the screen reader hears the
--                     editor (VoiceOver: "text, editing") — until Return or Escape has gone through
--                     to it and the field reads closed again; then the name read off a picture: the
--                     preset's, the OSC tab's (with the strip), the macro field's (with the macro
--                     panel). After Escape the old name is read again. A name the user typed is
--                     posted as read: the preset's is never snapped to the database's (the tool's
--                     skip_db_snap), and nothing is written to the database (the tool's set_osc_name)
--   browse_expansion_presets   the tool's sentence, the keyboard handed over with Return alone,
--                     and once Return has gone through, the name read until it settles
--   drumkit_column {index, column = "browse"}   a drum slot's Browse (the plan's slice 8): the
--                     strip read first, nothing clicked off the Drums tab; the keyboard handed over
--                     with Return alone, Browse clicked, the tool's sentence #13; once Return has
--                     gone through, a change wait on the slot's name from its picture before the
--                     click, and the slot's name read and compared with the one read off that picture
--   sample_browser_focus {index}   a row of the sample browser (the plan's slice 8, its other half),
--                     as a slot's Browse: the sample editor's part read first, nothing clicked unless
--                     it shows the browser and a slot read as selected, which the row loads into
--   sample_editor_panel_button {button = menu | preset}   the envelope's Menu and Preset on Amp or
--                     Pitch: the keyboard handed over with Return alone, nothing said (the tool said
--                     nothing); after Return the envelope's name read
--   sample_editor_field {field}   a field of the sample editor, as a rename: double-clicked on its
--                     own tab only, Avenger's editor seen open and closed, its value read back
--
-- What the tool did that is not ported: it recorded the keys typed to know the new name (its
-- panel.py) — the macro fields' only source of their names there — ignored Return for a second
-- after the editor opened, slept a quarter of a second after it, and fell back after five minutes;
-- here the name is read off the screen, the editor's close is a reading, and the page key is the
-- user's way back. Its OSC tab got a click of its own before the double-click: kept where the strip
-- reads another tab selected, its sleep a change wait. Its preset's name got a priming click: not
-- kept, since the trip brought Avenger's window forward (a DAW that drops the first click gets the
-- runtime's `prime`, from its daw-hosts entry). Its Windows panel read the preset's name after
-- every kind of rename; here each kind reads its own field. Its rename dialog was dead code, and is
-- left out of the page.

local G = host.include("src/geometry.luau")
local H = host.include("src/header.luau")
local S = host.include("src/state.luau")
local R = host.include("src/read/ocr.luau")
local P = host.include("src/read/picture.luau")
local Open = host.include("src/read/open.luau")
local Ed = host.include("src/read/editor.luau")
local T = host.include("src/tables/strip.luau")
local M = host.include("src/tables/macros.luau")
local D = host.include("src/tables/drums.luau")
local Osc = host.include("src/actions/osc.luau")
local Drums = host.include("src/actions/drums.luau")
local Sa = host.include("src/actions/sample.luau")
local C = host.include("src/tables/centre.luau")
local Act = host.include("src/act.luau")

local Hv = {}

local E = nil -- what actions.luau hands over (Hv.start)

-- The tool's sentence for its return mode (#12): the page's title is "Avenger Control".
Hv.BROWSE = "Browse presets in Avenger. To return to Avenger Control, press Return."
-- The tool's sentence for a drum slot's Browse (#13).
Hv.BROWSE_SLOT = "Use the Avenger browser. To return to Avenger Control, press Return."
-- The page's words, in its error region but the one said while the user is in Avenger.
Hv.NOT_OPEN = "VPS Avenger's editor did not open; nothing was renamed"
Hv.STILL_OPEN = "VPS Avenger's editor is still open: Return keeps the name, Escape cancels."
Hv.NOT_READ = "the new name cannot be read now"
Hv.NO_KEYBOARD = "the keyboard cannot be handed to VPS Avenger now"

local function log(fmt, ...)
  host.log.info(("[avenger] [rename] " .. fmt):format(...))
end

-- Every picture `held` keeps, let go: one held past its action holds the module's budget.
local function release(held)
  for k, s in pairs(held) do
    s:release()
    held[k] = nil
  end
end

-- The overlay's placer now — its origin, factor and zoom (read/picture.luau) — or nil.
local function placer(o)
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return nil end
  return P.placer(origin, k, (H.zoom(origin)))
end

-- The text of table rectangle `rect` in snapshot `snap`, tidied, or nil: the name read back, in a
-- handler, where the read waits.
local function textIn(pl, snap, rect)
  local r = pl:rect(rect)
  if not r then return nil end
  local ok, res = pcall(host.ocr.recognize, { name = "the field renamed", region = r }, { snapshot = snap, lang = H.lang() })
  if not (ok and type(res) == "table" and res.status == "text") then return nil end
  local t = R.tidy(res.text)
  return t ~= "" and t or nil
end

-- Part `part` read before the double-click, as src/act.luau reads one (Act.partBefore).
local function partBefore(pl, part, text, fields)
  return Act.partBefore(pl, part, text, log, fields)
end

-- ── A rename in Avenger's own editor ─────────────────────────────────────────
-- One flow for the three, in a trip's act, where reads wait:
--   1. a picture of the field (with a callback), then — the trip still in front — the double-click
--      at its point (main.luau's doubleAt);
--   2. after the second click, a change wait on the field from that picture: the editor open, or
--      "did not open" in the error region and the trip over, nothing handed over;
--   3. its edges read off the two pictures (read/editor.luau), the pictures let go, and the
--      keyboard handed over (`typing`: Return and Escape go through to Avenger and end it);
--   4. `ends`: a picture of the part read after; its field closed, or unsure, is the end — the read
--      decides — and still open, a change wait from it for Avenger's repaint; still open after that,
--      said, and the hand-over goes on;
--   5. `onEnd`, on its own end: the name read off that picture (F.read), and the trip done with it.
--      On any other end, the pictures go, and the trip's give-up says the rest (actions.luau, go).
-- `F`: kind ("preset", "osc", "macro": the log's, and the calibration pictures'), label (the
-- double-click's words), pl (the placer of the read before), at and field (the field's point and
-- rectangle, in table points at 50 %), after (the part read after, Open.PARTS' name, or nil for the
-- field's own picture), first (called before the double-click, or nil), part (the part as last
-- read before the double-click, { keys, values }, posted with a trip over before the hand-over, or
-- nil), and read(o, trip, now, pl, info) -> the trip's result, `now` the picture after (nil: none)
-- and `info` the hand-over's, its `key` Return or Escape.
function Hv.rename(o, trip, F)
  -- The trip over before the hand-over, with the part as last read (F.part) for the page.
  local function over(result)
    result.part = result.part or F.part
    return trip:done(result)
  end
  local pl = F.pl
  local at = Ed.place(pl, F.field)
  local afterRegion = at and (F.after and Open.partRegion(pl, F.after) or at.picture)
  if not afterRegion then
    log("'%s': nothing clicked — the field is not on the screen", F.kind)
    return over({ error = "that part of VPS Avenger is not on the screen" })
  end
  local held = {}   -- the pictures held: before, open, now
  local refs = nil  -- the field's edges before the double-click and with the editor open

  -- The field in picture `snap`: "closed", "open" or "unsure" — unsure without references, and the
  -- read after decides.
  local function verdict(snap)
    if not refs then
      log("'%s': its edges cannot be compared (not read with the editor open) — taken as closed: the read "
        .. "decides", F.kind)
      return "unsure"
    end
    local now, why = Ed.edges(snap, at)
    if not now then
      log("'%s': its edges cannot be compared (%s) — taken as closed: the read decides", F.kind, tostring(why))
      return "unsure"
    end
    local v = Ed.classify(F.kind, now, refs.before, refs.open)
    if v == "unsure" then log("'%s': taken as closed: the read decides", F.kind) end
    return v
  end

  local function ends(_, t2, info, answer)
    local asked = pcall(host.screen.snapshot, { region = afterRegion }, function(snap, why)
      if not t2:inFront() then
        if snap then snap:release() end
        return
      end
      if not snap then
        log("'%s': no picture after %s (%s) — the read decides", F.kind, tostring(info.key), tostring(why))
        return answer(true)
      end
      if held.now then held.now:release() end
      held.now = snap
      if verdict(snap) ~= "open" then return answer(true) end
      -- Avenger may not have repainted yet: its repaint is waited for, from the picture that shows
      -- the editor.
      local waited = pcall(host.screen.snapshot, { region = afterRegion, change = Ed.wait(snap, at) },
        function(again, why2)
          if not t2:inFront() then
            if again then again:release() end
            return
          end
          if not again then
            log("'%s': no picture after the editor's wait (%s) — the read decides", F.kind, tostring(why2))
            return answer(true)
          end
          if verdict(again) ~= "open" then
            held.now:release()
            held.now = again
            return answer(true)
          end
          -- Neither picture is held while the user types on.
          again:release()
          release(held)
          log("'%s': the editor is still open after %s — the keyboard stays VPS Avenger's", F.kind, tostring(info.key))
          host.speech.output(Hv.STILL_OPEN, { interrupt = false })
          answer(false)
        end)
      if not waited then answer(true) end
    end)
    if not asked then answer(true) end
  end

  local function onEnd(o2, t2, how, info)
    if how ~= "ended" or not t2:inFront() then return release(held) end
    Ed.shot(F.kind, "end", held.now, at, pl.zoom)
    local okR, result = pcall(F.read, o2, t2, held.now, pl, info)
    release(held)
    if not (okR and type(result) == "table") then
      log("'%s': the read after failed: %s", F.kind, tostring(result))
      result = { error = "reading VPS Avenger failed" }
    end
    t2:done(result)
  end

  -- The editor seen open, or not: the double-click's change wait from the field's picture before it.
  local function afterDouble()
    local asked, err = pcall(host.screen.snapshot, { region = at.picture, change = Ed.wait(held.before, at) },
      function(snap, why, info)
        if not trip:inFront() then
          if snap then snap:release() end
          return release(held)
        end
        local open = Ed.isOpen(snap, info)
        Ed.logOpen(F.kind, open, info, why)
        if not open then
          if snap then snap:release() end
          release(held)
          return over({ error = Hv.NOT_OPEN })
        end
        held.open = snap
        -- Where the keyboard is in the platform's words: a later, better test of "open" (U8).
        local okC, chain = pcall(host.window.focusChain)
        local first = okC and type(chain) == "table" and chain[1] or nil
        log("'%s': the keyboard after the double-click: %s", F.kind,
          first and tostring(first.class or first.role) or "nothing the platform reports")
        Ed.shot(F.kind, "before", held.before, at, pl.zoom)
        Ed.shot(F.kind, "open", held.open, at, pl.zoom)
        local before, whyB = Ed.edges(held.before, at)
        local opened, whyO = Ed.edges(held.open, at)
        if before and opened then
          refs = { before = before, open = opened }
        else
          log("'%s': the field's edges cannot be read: %s", F.kind, tostring(whyB or whyO))
        end
        -- The edges are what is kept: no picture is held while the user types.
        release(held)
        if not trip:inFront() then return end
        local handed, whyH = trip:handOver({ kind = "typing", ends = ends, onEnd = onEnd })
        if handed or whyH == "not in front" then return end
        log("'%s': the keyboard was not handed over (%s); VPS Avenger's editor is open", F.kind, tostring(whyH))
        over({ error = Hv.NO_KEYBOARD .. "; its editor is still open" })
      end)
    if not asked then
      release(held)
      log("'%s': no change wait after the double-click: %s", F.kind, tostring(err))
      over({ error = Hv.NOT_OPEN })
    end
  end

  local asked, err = pcall(host.screen.snapshot, { region = at.picture }, function(snap, why)
    if not snap then
      log("'%s': nothing clicked — no picture of the field: %s", F.kind, tostring(why))
      return over({ error = "VPS Avenger cannot be read now" })
    end
    held.before = snap
    if not trip:inFront() then return release(held) end
    if F.first then F.first() end
    local clicked, said = E.ring.doubleAt(o, F.label, F.at, nil, true, afterDouble)
    if not clicked then
      release(held)
      return over({ sentence = said })
    end
  end)
  if not asked then
    log("'%s': nothing clicked — no picture of the field: %s", F.kind, tostring(err))
    over({ error = "VPS Avenger cannot be read now" })
  end
end

-- start_rename: the name read before (as ◀ and ▶ read it), then the shared flow at the tool's point;
-- the database's position forgotten before the double-click (the tool's _clear_anchor). After it, a
-- name roughly the same as before (the tool's _names_roughly_same), and the old one after Escape,
-- keeps the database's place; another is the user's own, posted as read with no expansion.
function Hv.renamePreset(o, trip, forget)
  local was = E.ring.nameNow(o) or E.ring.lastName()
  if not trip:inFront() then return end
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  Hv.rename(o, trip, {
    kind = "preset", label = "Renaming the preset", pl = pl, at = G.NAME, field = G.NAME_REGION,
    first = function() forget("renaming from the page") end,
    read = function(_, _, now, pl2, info)
      local text = now and textIn(pl2, now, G.NAME_REGION)
      if not text then
        log("'preset': %s — nothing read", tostring(info.key))
        return { error = Hv.NOT_READ }
      end
      local name = R.presetName(text)
      E.ring.noteName(name)
      local kept = info.key == "Escape" or (was ~= nil and R.roughlySame(name, was))
      log("'preset': %s — read '%s'%s", tostring(info.key), name,
        kept and (", the name before ('" .. tostring(was) .. "') kept") or ", the user's own")
      if kept then return { set = E.named(name), focus = "rename-preset-button" } end
      return { set = { preset = name, expansion = false }, focus = "rename-preset-button" }
    end,
  })
end

-- The shared flow on OSC tab `i` of `n`, `before` the strip as last read before the double-click.
local function renameTab(o, trip, i, n, pl, label, before)
  local span = T.tabSpan(i, n, T.NAME_TOP, T.NAME_BOTTOM)
  Hv.rename(o, trip, {
    kind = "osc", label = label, pl = pl, at = { T.tabX(i, n), T.TAB_Y }, field = span, after = "strip",
    part = before,
    read = function(_, _, now, pl2, info)
      if not now then return { part = before, error = Hv.NOT_READ } end
      local va, keys = Open.part(now, "strip", { pl = pl2, preset = E.ring.lastName() })
      local name = textIn(pl2, now, span)
      log("'osc %d': %s — %s", i, tostring(info.key), name and ("read '" .. name .. "'") or "nothing read")
      local part = { keys = keys, values = va }
      if not name then return { part = part, error = Hv.NOT_READ } end
      local w = E.plugin()
      local known = S.values(w and w.id) or {}
      local names, snapped = {}, {}
      for j = 1, va.oscCount or n do
        names[j] = known.oscNames and known.oscNames[j]
        snapped[j] = known.oscSnapped and known.oscSnapped[j]
      end
      names[i], snapped[i] = name, false
      va.oscNames, va.oscSnapped = names, snapped
      keys[#keys + 1] = "oscNames"
      keys[#keys + 1] = "oscSnapped"
      return { part = part, focus = "osc-rename-" .. i }
    end,
  })
end

-- rename_osc {index}: the strip read first — the count from the dots places the tab, and a tab past
-- it, or no count, is refused with nothing clicked (osc.luau's `counted`). A tab the strip does not
-- read as the one selected is selected first, as the tool clicks it before its double-click: one
-- click, and its change waited for and the selection read again (src/act.luau, select_osc_tab's
-- spec on the strip) in the place of the tool's 0.12 s — the tab's selection lies across the field's
-- picture, and a double-click that selected it would show the open wait a change that is no editor.
-- Then the shared flow on the tab, its picture before taken once the selection has been seen. After
-- it the strip is read again and the tab's name off the same picture: posted as read, never
-- snapped, the other tabs' names as last read.
function Hv.renameOsc(o, trip, i)
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local vb, keysB, err = partBefore(pl, "strip")
  if not trip:inFront() then return end
  if not vb then return trip:done({ error = err }) end
  local n, why = Osc.counted(vb, i)
  if not n then
    log("'osc %d': nothing clicked — %s", i, why)
    return trip:done({ part = { keys = keysB, values = vb }, error = why })
  end
  local label = "Renaming OSC " .. i
  if vb.selectedType == "osc" and vb.selectedIndex == i then
    return renameTab(o, trip, i, n, pl, label, { keys = keysB, values = vb })
  end
  local sel = Osc.select_osc_tab({ index = i }, label)
  sel.part = "strip"
  local decide, clicked = sel.decide, false
  sel.decide = function(v)
    local click, refusal = decide(v)
    clicked = click ~= nil
    return click, refusal
  end
  log("'osc %d': the strip reads %s selected — OSC %d is selected first", i, Osc.tabWords(vb) or "no tab", i)
  Act.run(o, sel, function(result)
    if not trip:inFront() then return end
    -- Nothing clicked: refused, as the double-click would have been.
    if not clicked or result.sentence then
      return trip:done({ part = result.part, error = result.error, sentence = result.sentence })
    end
    local va = result.part and result.part.values
    log("'osc %d': after its selection — %s; the double-click next", i,
      tostring(result.said or result.note or result.error))
    renameTab(o, trip, i, va and Osc.counted(va, i) or n, pl, label, result.part)
  end, trip)
end

-- rename_macro {index}: the macro panel read first with its words — the tab shown and its five
-- names — then the shared flow on field `index` (1 to 3 the knobs', 4 and 5 the buttons'; the tool's
-- _macro_field_name). After it the panel is read again and the field's name off the same picture,
-- posted as read; the page's focus back on the field's Rename button where the tab shown was read.
function Hv.renameMacro(o, trip, index)
  local field = M.FIELD_ORDER[index]
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local vb, keysB, err = partBefore(pl, "macros", true)
  if not trip:inFront() then return end
  if not vb then return trip:done({ error = err }) end
  local before = { keys = keysB, values = vb }
  local focus = vb.macroSelected and string.format("macro-%d-slot-%d-rename", vb.macroSelected, index) or nil
  Hv.rename(o, trip, {
    kind = "macro", label = "Renaming macro field " .. index, pl = pl, at = M.FIELDS[field],
    field = M.FIELD_NAMES[field], after = "macros", part = before,
    read = function(_, _, now, pl2, info)
      if not now then return { part = before, error = Hv.NOT_READ } end
      local va, keys = Open.part(now, "macros", { pl = pl2, macroCount = vb.macroCount })
      local name = textIn(pl2, now, M.FIELD_NAMES[field])
      log("'macro %s': %s — %s", field, tostring(info.key), name and ("read '" .. name .. "'") or "nothing read")
      local labels, snapped = {}, {}
      for j = 1, 5 do
        labels[j] = vb.macroLabels and vb.macroLabels[j]
        snapped[j] = vb.macroSnapped and vb.macroSnapped[j]
      end
      if name then labels[index], snapped[index] = name, false end
      va.macroCount, va.macroLabels, va.macroSnapped = vb.macroCount, labels, snapped
      for _, k in ipairs({ "macroCount", "macroLabels", "macroSnapped" }) do keys[#keys + 1] = k end
      local part = { keys = keys, values = va }
      if not name then return { part = part, error = Hv.NOT_READ } end
      return { part = part, focus = focus }
    end,
  })
end

-- ── Browse Presets ───────────────────────────────────────────────────────────
-- A trip with no click: the preset may change while the user browses, so the database's position is
-- forgotten first and a give-up posts the preset as not read (actions.luau, go). The name as it
-- stands is read once with a callback, for the log only (U5); the keyboard is handed over with
-- Return alone — Escape and the arrows are Avenger's browser's — and the tool's sentence said
-- behind the screen reader. Once Return has gone through, the name is read until two readings agree
-- or it is a catalog name (main.luau), in the place of the tool's quarter-second sleep, and posted
-- with the database's place, as the tool snaps it.
function Hv.browse(o, trip, forget)
  forget("browsing presets in VPS Avenger")
  if not trip:inFront() then return end
  local start = {}
  local r = o:toScreenRect(G.NAME_REGION)
  if r then
    pcall(host.ocr.recognize, r, { lang = H.lang(), key = "vps-avenger browse" }, function(res)
      if res.status == "text" then start.name = R.tidy(res.text) end
    end)
  end
  local handed, why = trip:handOver({
    kind = "browse",
    onEnd = function(o2, t2, how)
      if how ~= "ended" or not t2:inFront() then return end
      E.ring.settledName(o2, function(name, n)
        host.log.info(string.format("[avenger] [browse] the name at the start %s, after Return %s (read %d times)",
          start.name and ("'" .. start.name .. "'") or "not read", name and ("'" .. name .. "'") or "not read", n or 0))
        if name == nil then return t2:done({ error = "the preset name cannot be read now" }) end
        t2:done({ set = E.named(name), focus = "browse-expansion-presets-button" })
      end, "Browse Presets")
    end,
  })
  if handed then
    host.speech.output(Hv.BROWSE, { interrupt = false })
    return
  end
  if why == "not in front" then return end
  trip:done({ error = Hv.NO_KEYBOARD })
end

-- ── A drum slot's Browse, and a row of the sample browser ────────────────────
-- A trip whose act opens Avenger's sample browser on slot `i`, or one of its rows, and hands the
-- keyboard over, as Browse Presets does — Return alone held, Escape and the arrows the browser's —
-- and once Return has gone through, reads the slot's name. In order:
--   1. a part read first, and nothing clicked where it refuses: the strip off the Drums tab
--      (Drums.refusal) for a slot's Browse; the sample editor's part for a row, unless it shows the
--      sample browser and a slot read as selected, which the row loads into (Sa.browserRefusal);
--   2. the slot's name pictured and HELD through the hand-over — one row of the name column, as
--      the knobs' hand-over holds the knob's box (src/amount.luau): the baseline of the one change
--      wait after Return — and its text read off that picture with a callback, while the user
--      browses (one region, nothing waits for it);
--   3. the hand-over, before the click, so that a hand-over refused clicks nothing;
--   4. the click at the tool's point — Browse (345, the slot's row), or the row (294, 277 + 10 per
--      row) — and the slot's name noted as not read in the state, which a give-up then posts, and
--      the read after Return overwrites;
--   5. the tool's sentence #13, behind the screen reader;
--   6. Return through: a change wait on the name from the held picture, then the part read with its
--      text — the drumkit table, and for a row the sample editor with it, whose view Avenger may
--      change once a sample is loaded — and the slot's name said as read, "unchanged" where it reads
--      as it read before. By the names, as Back and Next decide: the name's box holds the band the
--      selected slot is drawn in (read/drums.luau), and the tool took every click in a slot's row
--      for its selection, so Browse on a slot not selected repaints the box whatever is chosen.
--      Only where either name was not read do the pixels decide: "unchanged" where Avenger drew no
--      change. Every picture is let go on every path.
-- No change wait sees the browser open, as the rename's does: whether a second Browse closes an
-- open browser is not known, so a change in the central area would not say "open". Where none
-- opened the hand-over is harmless: Return goes to Avenger and ends it. The log says what was
-- clicked, and the developer's pictures say what Avenger does (TODO.md, D3).
--
-- Steps 2 to 6 for slot `F.i`: `F` = { i, label (the click's words), at (the click, at 50 %), focus
-- (the page's element after it), before ({ keys, values } as read in step 1), after (the part read
-- after Return), fields (its fields, or nil), pl, log(fmt, …) }.
local function browseName(o, trip, F)
  local i, pl, slog, before = F.i, F.pl, F.log, F.before
  local role = D.PART_LABELS[i]
  local region = Open.region(pl, D.nameRect(i))
  if not region then
    slog("nothing clicked — the slot's name is not on the screen")
    return trip:done({ part = before, error = "that part of VPS Avenger is not on the screen" })
  end
  local okS, snap, whyS = pcall(host.screen.snapshot, { region = region })
  if not (okS and snap) then
    slog("nothing clicked — no picture of its name: %s", tostring(okS and whyS or snap))
    return trip:done({ part = before, error = "VPS Avenger cannot be read now" })
  end
  local held = { before = snap }
  if not trip:inFront() then return release(held) end
  -- The name before, off the held picture, answered while the user browses: the read keeps the
  -- pixels it needs whatever becomes of the picture.
  local start = {}
  pcall(host.ocr.recognize, { name = "slot " .. i, region = region }, { snapshot = snap, lang = H.lang(),
    key = "vps-avenger browse slot" }, function(res)
    local t = type(res) == "table" and res.status == "text" and R.tidy(res.text) or ""
    if t ~= "" then start.name = t end
    start.answered = true
  end)

  -- The slot's name read after Return, off a picture of the part read after, and the trip done
  -- with it; `changed` the change wait's verdict on the name's box, which decides only where a name
  -- is missing.
  local function readAfter(t2, changed)
    local partRegion = Open.partRegion(pl, F.after)
    local okP, pic = false, nil
    if partRegion then okP, pic = pcall(host.screen.snapshot, { region = partRegion }) end
    if not t2:inFront() then
      if okP and pic then pic:release() end
      return
    end
    if not (okP and pic) then
      slog("no picture after Return: %s", tostring(pic))
      return t2:done({ part = before, error = role .. "'s name cannot be read now" })
    end
    local okR, va, keys = pcall(Open.part, pic, F.after, { pl = pl, text = true, preset = E.ring.lastName(),
      fields = F.fields })
    pic:release()
    if not okR then
      slog("the read after Return failed: %s", tostring(va))
      return t2:done({ part = before, error = "reading VPS Avenger failed" })
    end
    local part = { keys = keys, values = va }
    local name = va.kitNames and va.kitNames[i]
    local was = start.name
    if name and was then changed = name ~= was end
    slog("after Return — %s, %s by %s (before: %s)", name and ("'" .. name .. "'") or "no name read",
      changed and "changed" or "unchanged", (name and was) and "the names" or "the pixels",
      was and ("'" .. was .. "'") or (start.answered and "no name read" or "not answered yet"))
    if not name then return t2:done({ part = part, error = role .. "'s name cannot be read now" }) end
    if not changed then return t2:done({ part = part, note = "unchanged, " .. role .. " reads " .. name }) end
    t2:done({ part = part, said = role .. " reads " .. name, focus = F.focus })
  end

  local function onEnd(_, t2, how)
    if how ~= "ended" or not t2:inFront() then return release(held) end
    local asked, errW = pcall(host.screen.snapshot, { region = region, change = { from = held.before,
      timeout = Act.CHANGE_WITHIN, tolerance = 16, minPixels = 4, settle = 50 } }, function(s, _, info)
      release(held)
      if s then s:release() end
      if not t2:inFront() then return end
      readAfter(t2, type(info) == "table" and info.changed == true)
    end)
    if not asked then
      release(held)
      slog("no change wait after Return: %s", tostring(errW))
      readAfter(t2, false)
    end
  end

  local handed, whyH = trip:handOver({ kind = "browse", onEnd = onEnd })
  if not handed then
    release(held)
    if whyH == "not in front" then return end
    slog("nothing clicked — the keyboard was not handed over (%s)", tostring(whyH))
    return trip:done({ part = before, error = Hv.NO_KEYBOARD })
  end
  local clicked, said = E.ring.clickAt(o, F.label, F.at, nil, true)
  -- Refused: done ends the hand-over, and onEnd lets the picture go.
  if not clicked then return trip:done({ part = before, sentence = said }) end
  -- What the browser does to the slot's name, not read yet: posted as not read by a give-up.
  local w = E.plugin()
  if w then
    local known = S.values(w.id) or {}
    local names = {}
    for j = 1, #D.PART_LABELS do names[j] = known.kitNames and known.kitNames[j] end
    names[i] = nil
    S.part(w.id, { "kitNames" }, { kitNames = names })
  end
  host.speech.output(Hv.BROWSE_SLOT, { interrupt = false })
end

-- drumkit_column {index, column = "browse"}: slot `i`'s Browse.
function Hv.browseSlot(o, trip, i, label)
  local function slog(fmt, ...)
    host.log.info(("[avenger] [browse] 'slot %d': " .. fmt):format(i, ...))
  end
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local vb, keysB, err = Act.partBefore(pl, "strip", nil, function(fmt, ...) slog(fmt, ...) end)
  if not trip:inFront() then return end
  if not vb then return trip:done({ error = err }) end
  local before = { keys = keysB, values = vb }
  local why, focus = Drums.refusal(vb)
  if why then
    slog("nothing clicked — %s", why)
    return trip:done({ part = before, error = why, focus = focus })
  end
  browseName(o, trip, { i = i, label = label, at = D.columnPoint(i, "browse"), focus = "drumkit-browse-" .. i,
    before = before, after = "drums", pl = pl, log = slog })
end

-- sample_browser_focus {index}: row `index` of the sample browser, which loads into the slot read as
-- selected — the tool clamped the row and took the browser for shown.
function Hv.browseSample(o, trip, index, label)
  local function slog(fmt, ...)
    host.log.info(("[avenger] [browse] 'sample browser row %d': " .. fmt):format(index, ...))
  end
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local fields = { "browser" }
  local vb, keysB, err = Act.partBefore(pl, "editor", nil, function(fmt, ...) slog(fmt, ...) end, fields)
  if not trip:inFront() then return end
  if not vb then return trip:done({ error = err }) end
  local before = { keys = keysB, values = vb }
  local why, focus = Sa.browserRefusal(vb)
  if why then
    slog("nothing clicked — %s", why)
    return trip:done({ part = before, error = why, focus = focus })
  end
  browseName(o, trip, { i = vb.kitSelected, label = label, at = C.browserItemPoint(index),
    focus = "sample-browser-item-" .. index, before = before, after = "editor", fields = fields, pl = pl, log = slog })
end

-- ── The envelope's Menu and Preset ───────────────────────────────────────────
-- sample_editor_panel_button {button = menu | preset} on the Amp or the Pitch tab: the tool's return
-- mode, with no sentence of its own (it said none). The editor's part read first, with the
-- envelope's name, and nothing clicked unless Amp or Pitch is shown (Sa.envelopeRefusal); the
-- envelope pictured and HELD — its Preset field and the envelope above the fields — the baseline of
-- the change wait after Return; the keyboard handed over with Return alone, Avenger's popup or
-- preset list the user's; the button clicked. Once Return has gone through — its end looked at with
-- the plug-in in front, so a Save dialog Menu led to is waited past (trip:handOver) — the change
-- wait from the held picture, and the part read again with the envelope's name: the answer as Back
-- and Next's (actions/sample.luau, Sa.envelopeAnswer).
function Hv.browseEnvelope(o, trip, button, label)
  local function slog(fmt, ...)
    host.log.info(("[avenger] [browse] 'envelope %s': " .. fmt):format(button, ...))
  end
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local fields = { "browser", "envelope_name" }
  local vb, keysB, err = Act.partBefore(pl, "editor", nil, function(fmt, ...) slog(fmt, ...) end, fields)
  if not trip:inFront() then return end
  if not vb then return trip:done({ error = err }) end
  local before = { keys = keysB, values = vb }
  local why, focus = Sa.envelopeRefusal(vb)
  if why then
    slog("nothing clicked — %s", why)
    return trip:done({ part = before, error = why, focus = focus })
  end
  local n, a = C.ENVELOPE_NAME, C.ENVELOPE_AREA
  local region = Open.region(pl, { math.min(n[1], a[1]), math.min(n[2], a[2]), math.max(n[3], a[3]), math.max(n[4], a[4]) })
  local partRegion = Open.partRegion(pl, "editor")
  if not (region and partRegion) then
    slog("nothing clicked — the envelope is not on the screen")
    return trip:done({ part = before, error = "that part of VPS Avenger is not on the screen" })
  end
  local okS, snap, whyS = pcall(host.screen.snapshot, { region = region })
  if not (okS and snap) then
    slog("nothing clicked — no picture of the envelope: %s", tostring(okS and whyS or snap))
    return trip:done({ part = before, error = "VPS Avenger cannot be read now" })
  end
  local held = { before = snap }
  if not trip:inFront() then return release(held) end
  local focusAfter = "sample-editor-" .. tostring(vb.editorTab) .. "-" .. button

  local function readAfter(t2, changed)
    local okP, pic = pcall(host.screen.snapshot, { region = partRegion })
    if not t2:inFront() then
      if okP and pic then pic:release() end
      return
    end
    if not (okP and pic) then
      slog("no picture after Return: %s", tostring(pic))
      return t2:done({ part = before, error = "the " .. Sa.envelopeWords(vb) .. " cannot be read now" })
    end
    local okR, va, keys = pcall(Open.part, pic, "editor", { pl = pl, preset = E.ring.lastName(), fields = fields })
    pic:release()
    if not okR then
      slog("the read after Return failed: %s", tostring(va))
      return t2:done({ part = before, error = "reading VPS Avenger failed" })
    end
    -- The names decide where both were read, as Back and Next's `same`; the pixels otherwise.
    local was = vb.editorFields and vb.editorFields.envelope_name
    local now = va.editorFields and va.editorFields.envelope_name
    if was and now then changed = was ~= now end
    slog("after Return — %s, %s (before: %s)", now and ("'" .. now .. "'") or "no name read",
      changed and "changed" or "unchanged", was and ("'" .. was .. "'") or "no name read")
    local result = Sa.envelopeAnswer(vb, va, changed and "changed" or "unchanged", focusAfter)
    result.part = { keys = keys, values = va }
    t2:done(result)
  end

  local function onEnd(_, t2, how)
    if how ~= "ended" or not t2:inFront() then
      release(held)
      -- What the user did in Avenger's popup or list is not read: the envelope's name posted so.
      if how ~= "ended" then
        local w = E.plugin()
        if w then S.part(w.id, { "editorFields" }, {}) end
      end
      return
    end
    local asked, errW = pcall(host.screen.snapshot, { region = region, change = { from = held.before,
      timeout = Act.CHANGE_WITHIN, tolerance = 16, minPixels = 4, settle = 50 } }, function(s, _, info)
      release(held)
      if s then s:release() end
      if not t2:inFront() then return end
      readAfter(t2, type(info) == "table" and info.changed == true)
    end)
    if not asked then
      release(held)
      slog("no change wait after Return: %s", tostring(errW))
      readAfter(t2, false)
    end
  end

  local handed, whyH = trip:handOver({ kind = "browse", onEnd = onEnd })
  if not handed then
    release(held)
    if whyH == "not in front" then return end
    slog("nothing clicked — the keyboard was not handed over (%s)", tostring(whyH))
    return trip:done({ part = before, error = Hv.NO_KEYBOARD })
  end
  local clicked, said = E.ring.clickAt(o, label, C.PANEL[button], nil, true)
  if not clicked then return trip:done({ part = before, sentence = said }) end
end

-- ── A field of the sample editor typed into ──────────────────────────────────
-- sample_editor_field {field}: the editor's part read first, and nothing clicked unless the field's
-- own tab is shown (Sa.fieldRefusal) — Gain and Range share a point, as Start and the EQ's first
-- band lie apart only on their tabs; then the shared flow of a rename on the field (Hv.rename), its
-- picture the field's box: the double-click where the tool double-clicks it, Avenger's editor seen
-- open, the keyboard handed over with nothing said, the field seen closed, and its value READ off
-- that picture, never recorded from the keys.
function Hv.renameSample(o, trip, field, label)
  local pl = placer(o)
  if not pl then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local vb, keysB, err = partBefore(pl, "editor", nil, { "browser" })
  if not trip:inFront() then return end
  if not vb then return trip:done({ error = err }) end
  local before = { keys = keysB, values = vb }
  local why, focus = Sa.fieldRefusal(vb, field)
  if why then
    log("'sample %s': nothing clicked — %s", field, why)
    return trip:done({ part = before, error = why, focus = focus })
  end
  local words = Sa.FIELD_WORDS[field]
  local rect = C.fieldRect(field)
  local focusAfter = Sa.fieldFocus(field, vb)
  Hv.rename(o, trip, {
    kind = "sample", label = label, pl = pl, at = C.PANEL[field], field = rect, part = before,
    read = function(_, _, now, pl2, info)
      local text = now and textIn(pl2, now, rect)
      log("'sample %s': %s — %s", field, tostring(info.key), text and ("read '" .. text .. "'") or "nothing read")
      if not text then return { part = before, error = words .. " cannot be read now" } end
      if info.key == "Escape" then return { part = before, note = "Escape: " .. words .. " reads " .. text, focus = focusAfter } end
      return { part = before, said = words .. " reads " .. text, focus = focusAfter }
    end,
  })
end

-- What actions.luau hands over: { ring = main.luau's helpers, named = (name) -> the state's fields
-- with the database's place, plugin = () -> the window the page was shown beside }.
function Hv.start(env)
  E = env
end

return Hv
