-- The page's knobs turned in Avenger (the plan's slice 7): the avenger_control tool's amount mode,
-- as a trip whose act presses the knob, HOLDS the button and hands the keyboard over with the
-- overlay runtime's `trip:handOver{ kind = "keys" }` (docs/api/overlay.md):
--
--   macro_amount {index}         a macro knob, at (736, 787 or 838; 480)
--   osc_control {control}        the selected oscillator's Level (Volume, and Pan with the right
--                                button), Transpose, or its third knob (Fine Tune, and Random or
--                                Alternate Pitch with the right button)
--   osc_filter_control {control} the 2-band filter's Low-Pass or High-Pass handle, along x
--   drumkit_control {index, control}   a drum slot's Volume, Pan or Pitch, on its row; nothing
--                                pressed off the Drums tab; no reader, Return claims nothing
--   sample_editor_control {control}   one of the sample editor's eight knobs; nothing pressed
--                                unless the sample editor is shown; no reader
--   central_drum_sq_control {control}   VEL AMOUNT or SHUFFLE; nothing pressed unless DRUM SQ is
--                                the central tab; no reader
--
-- Each is the same gesture, its spec built here: the oscillator strip read first where the action
-- needs an oscillator selected (nothing pressed on the Drums or the Trig tab, which have none: the
-- tool pressed nothing there either); a picture of the knob; the hand-over — PageUp, PageDown,
-- Return and Escape CAPTURED, so none of them reaches Avenger or the DAW; the press, with the
-- tool's button; and the tool's sentence, behind the screen reader. Then each PageUp or PageDown is
-- ONE move of the held pointer, one table point (one at 50 %) times the overlay's factor — up or
-- left, down or right — kept within the tool's travel: 106 points either way of the press, or the
-- filter's x from 368 to 509. A key at the limit still makes its move, to the limit, and plays
-- Slider.wav (behind the `sounds` setting). Return lets go of the button and READS the knob off a
-- picture, once Avenger has drawn what the keys did — a change wait from the picture before the
-- press — and the page comes back with what was read; Escape lets go of it and reads nothing: the
-- knob stays where the keys moved it, and the page says its value not read. The filter has no
-- reader (the tool read none, the page shows none): Return brings the page back claiming nothing.
--
-- EVERY END lets go of the button exactly once (`letGo`): Return and Escape at once, in their key's
-- handler; the page key, another window, the page closed, the plug-in's window gone, outranked and
-- a hook that raised first thing in `onEnd`. After a disable the host has let go already
-- (docs/api/input.md, mouseDown): the end that follows the enable finds the button off the host's
-- record (host.input.held) and sends nothing more. A reload or a stop by the host runs no code of
-- this VM's; the host lets go then.
--
-- What the tool did that is not ported (its panel.py and windows_panel.py): its key monitor only
-- observed, so PageUp, PageDown and Return reached Logic as well (Return moved the playhead); it
-- had no Escape; it slept 50 ms per key, waited 0.12 s after Return and gave up after five
-- minutes; it read the knob after Return without waiting for anything; and its Windows driver
-- dragged by warping the pointer, which a held knob does not follow — this host moves it as motion
-- while the button is held (docs/api/input.md, move). Its sentence #6 said "ammount", and #7 the
-- control's key ("fine_tune"): here "amount", and the key with a space. Its #9, a drum slot's
-- knob, named the slot by its factory name even where its own state had another: here the slot's
-- name as last read, and the factory name where none was. Its #10, the sample editor's knob, named
-- the slot it took as selected from its last click: here the slot read as selected before the
-- press, by its name as last read. Its #11, VEL AMOUNT's and SHUFFLE's, is as it was.

local O = host.require("com.platform.overlay")
local H = host.include("src/header.luau")
local S = host.include("src/state.luau")
local P = host.include("src/read/picture.luau")
local Open = host.include("src/read/open.luau")
local Knob = host.include("src/read/knob.luau")
local OSC = host.include("src/tables/osc.luau")
local M = host.include("src/tables/macros.luau")
local D = host.include("src/tables/drums.luau")
local Osc = host.include("src/actions/osc.luau")
local Drums = host.include("src/actions/drums.luau")
local Sa = host.include("src/actions/sample.luau")
local Sq = host.include("src/actions/drumsq.luau")
local C = host.include("src/tables/centre.luau")
local Act = host.include("src/act.luau")

local Am = {}

local E = nil -- what actions.luau hands over (Am.start)

-- The keys the hand-over captures: none of them reaches Avenger or the DAW.
Am.KEYS = { "PageUp", "PageDown", "Return", "Escape" }
-- The page's words for a hand-over refused, as the renames say it.
Am.NO_KEYBOARD = "the keyboard cannot be handed to VPS Avenger now"
-- Where the tool leaves the page's focus when the strip shows no oscillator selected.
local NO_OSC_FOCUS = "osc-tab-detail-group"
-- The filter's band, for a calibrating run's pictures (no reader reads it yet).
local FILTER_BAND = { 366, 50, 511, 74 }

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
end

-- Every picture `held` keeps, let go.
local function release(held)
  for k, s in pairs(held) do
    s:release()
    held[k] = nil
  end
end

-- What was last read in the window the page was shown beside (state.luau's), or an empty table.
local function lastRead()
  local w = E.plugin()
  return S.values(w and w.id) or {}
end

-- The selected OSC tab's name as last read, or the tool's "OSC <i>", for the sentence (#7, #8).
local function oscName(vb, known)
  local i = vb and vb.selectedIndex
  local name = i and known and known.oscNames and known.oscNames[i]
  if name and name ~= "" then return name end
  return "OSC " .. tostring(i or 1)
end

-- ── The specs ────────────────────────────────────────────────────────────────
-- Each `(args, label, known) -> spec | nil, why`, as osc.luau's builders are: `known` what was last
-- read in this window, for words only. A spec:
--   label, name   the action's words; the log's and the calibration pictures' name for the knob
--   at            the press, a table point at 50 %
--   button        "right" for Pan and Random Pitch (the tool's OSC_CONTROL_RIGHT_BUTTON), else nil
--   axis, lo, hi  "y" or "x", and the travel in table points
--   strip         a part read first — `before`, Open.PARTS' name, the oscillator strip unless it
--                 says, with its `fields` — and nothing pressed where `refuse(vb)` answers why (and
--                 the page's focus); by default unless an oscillator is selected
--   knob, box     Knob.read's `which` and its picture's table rectangle; nil for no reader
--   band          a calibrating run's picture where there is no reader
--   sentence(vb)  the tool's sentence; focus(vb) the page's element after it
--   merge(before, fields)   the part posted: `before` the strip read first ({ keys, values }, or
--                 nil), `fields` Knob.read's — nil: the knob's values not read
--   value(fields), words(v), unread()   the value read, in words, and the words for none

-- The fields of each oscillator knob, as Knob.read gives them.
local KNOB_FIELDS = {
  volume = { "volume", "pan" },
  pan = { "volume", "pan" },
  transpose = { "transpose" },
  fine_tune = { "fineTune", "randomPitch" },
  random_pitch = { "fineTune", "randomPitch" },
}

function Am.macro(args, label, known)
  local i = whole(args.index)
  if not (i and i >= 1 and i <= 3) then return nil, "no macro knob was named" end
  local which = "macro" .. i
  local at, box = Knob.place(which)
  local function shown() return (known and known.macroSelected) or 1 end
  -- The knob's name as last read on the tab shown, or the tool's "Macro <n>" (its
  -- _current_macro_label): for words only.
  local function name()
    local n = known and known.macroLabels and known.macroLabels[i]
    if n and n ~= "" then return n end
    return "Macro " .. ((shown() - 1) * 3 + i)
  end
  return {
    label = label, name = "macro " .. i, at = at, axis = "y", knob = which, box = box,
    lo = at[2] - M.AMOUNT_LIMIT, hi = at[2] + M.AMOUNT_LIMIT,
    sentence = function()
      return "Please press Page Up or Down to set the amount of the " .. name() .. " Slider. To Confirm, press Return."
    end,
    focus = function() return string.format("macro-%d-slot-%d-amount", shown(), i) end,
    -- The amounts as last read, this knob's replaced.
    merge = function(_, fields)
      local was, list = lastRead().macroAmounts or {}, {}
      for j = 1, 3 do list[j] = was[j] end
      list[i] = fields and fields.amount or nil
      return { keys = { "macroAmounts" }, values = { macroAmounts = list } }
    end,
    value = function(fields) return fields.amount end,
    words = function(v) return name() .. " " .. v .. " percent" end,
    unread = function() return name() .. "'s knob cannot be read now" end,
  }
end

-- An oscillator's knob or filter band: refused before anything is pressed unless the strip read
-- first shows an oscillator selected.
local function oscRefusal(vb)
  if vb.selectedType == "drums" then return "the Drums tab is selected; it has no oscillator controls", NO_OSC_FOCUS end
  if vb.selectedType == "trig" then return "the Trig tab is selected; it has no oscillator controls", NO_OSC_FOCUS end
  if vb.selectedType ~= "osc" then return "the selected tab cannot be read now" end
  return nil
end

function Am.osc(args, label, known)
  local key = args.control
  local c = type(key) == "string" and Osc.CONTROLS[key] or nil
  if not c then return nil, tostring(key) .. " is no control of an oscillator's" end
  local at, box = Knob.place(key)
  local spoken = (string.gsub(key, "_", " "))
  return {
    label = label, name = key, at = at, button = OSC.RIGHT_BUTTON[key] and "right" or nil, axis = "y",
    lo = at[2] - OSC.LIMIT, hi = at[2] + OSC.LIMIT, strip = true, knob = key, box = box,
    sentence = function(vb)
      return "Please press Page Up or Down to set the " .. spoken .. " of " .. oscName(vb, known)
        .. ". To Confirm, press Return."
    end,
    focus = function() return "osc-" .. (string.gsub(key, "_", "-")) end,
    -- The strip as read before the press, and the selected oscillator's knobs as last read where
    -- that read shows the same oscillator selected — this knob's values replaced.
    merge = function(before, fields)
      local vb, last = before.values, lastRead()
      local osc = {}
      if last.osc and last.selectedType == "osc" and last.selectedIndex == vb.selectedIndex then
        osc = table.clone(last.osc)
      end
      for _, f in ipairs(KNOB_FIELDS[key]) do osc[f] = fields and fields[f] or nil end
      local keys, values = table.clone(before.keys), table.clone(vb)
      keys[#keys + 1] = "osc"
      values.osc = osc
      return { keys = keys, values = values }
    end,
    value = function(fields) return fields[c.field] end,
    words = c.words,
    unread = function() return Osc.CONTROL_NAMES[key] .. " cannot be read now" end,
  }
end

function Am.filter(args, label, known)
  local key = args.control
  local at = type(key) == "string" and OSC.FILTERS[key] or nil
  if not at then return nil, tostring(key) .. " is no band of the 2-band filter" end
  local band = key == "low_pass" and "Low-Pass" or "High-Pass"
  return {
    label = label, name = key, at = at, axis = "x", lo = OSC.FILTER_X_MIN, hi = OSC.FILTER_X_MAX, strip = true,
    band = FILTER_BAND,
    sentence = function(vb)
      return "Please press Page Up to move " .. band .. " left, or Page Down to move " .. band .. " right for "
        .. oscName(vb, known) .. ". To Confirm, press Return."
    end,
    focus = function() return "osc-" .. (string.gsub(key, "_", "-")) end,
    -- The strip as read before the press: the page shows no value of the filter's.
    merge = function(before) return before end,
  }
end

-- A drum slot's knob: Volume, Pan or Pitch on its row, with the left button for all three (the
-- tool's), refused before anything is pressed unless the strip shows the Drums tab selected. No
-- reader reads a drum knob — the tool read none, the page shows none — so Return claims nothing,
-- as the filter's does; a calibrating run pictures the knob's box (D.knobBox).
function Am.drum(args, label, known)
  local i = Drums.slotArg(args.index)
  if not i then return nil, "no drum slot was named" end
  local key = args.control
  if not (type(key) == "string" and Drums.CONTROL_WORDS[key]) then return nil, tostring(key) .. " is no control of a drum slot's" end
  local at = D.columnPoint(i, key)
  return {
    label = label, name = "drum " .. i .. " " .. key, at = at, axis = "y", lo = at[2] - OSC.LIMIT,
    hi = at[2] + OSC.LIMIT, strip = true, refuse = Drums.refusal, band = D.knobBox(i, key),
    -- The tool's #9, with the slot's name as last read.
    sentence = function()
      return "Please press Page Up or Down to set " .. key .. " for " .. Drums.slotName(nil, known, i)
        .. ". To Confirm, press Return."
    end,
    focus = function() return "drumkit-" .. key .. "-" .. i end,
    merge = function(before) return before end,
  }
end

-- One of the sample editor's eight knobs, with the left button (the tool's), refused before anything
-- is pressed unless the sample editor is shown (Sa.refusal) — its knobs are those of the slot read
-- as selected. No reader, as for a drum slot's knob.
function Am.sample(args, label, known)
  local c, why = Sa.controlKey(args.control)
  if not c then return nil, why end
  local at = C.EDITOR_CONTROLS[c]
  local word = Sa.CONTROL_WORDS[c]
  return {
    label = label, name = "sample " .. c, at = at, axis = "y", lo = at[2] - OSC.LIMIT, hi = at[2] + OSC.LIMIT,
    strip = true, before = "editor", fields = { "browser" }, refuse = Sa.refusal, band = C.editorKnobBox(c),
    -- The tool's #10, with the slot read as selected by its name as last read.
    sentence = function(vb)
      local i = vb and vb.kitSelected
      local slot = i and Drums.slotName(vb, known, i) or "the selected slot"
      return "Please press Page Up or Down to set " .. word .. " for " .. slot .. ". To Confirm, press Return."
    end,
    focus = function() return "sample-editor-" .. c end,
    merge = function(before) return before end,
  }
end

-- VEL AMOUNT or SHUFFLE, with the left button, refused before anything is pressed unless DRUM SQ
-- reads as the central tab (Sq.refusal). No reader.
function Am.drumSq(args, label)
  local c, why = Sq.controlKey(args.control)
  if not c then return nil, why end
  local at = C.SQ_CONTROLS[c]
  local word = Sq.CONTROL_WORDS[c]
  return {
    label = label, name = "drum sq " .. c, at = at, axis = "y", lo = at[2] - OSC.LIMIT, hi = at[2] + OSC.LIMIT,
    strip = true, before = "drumsq", refuse = Sq.refusal, band = C.sqKnobBox(c),
    -- The tool's #11.
    sentence = function() return "Please press Page Up or Down to set " .. word .. ". To Confirm, press Return." end,
    focus = function() return "central-drum-sq-" .. (string.gsub(c, "_", "-")) end,
    merge = function(before) return before end,
  }
end

-- ── Calibration ──────────────────────────────────────────────────────────────

-- The pictures written this session, by name.
local shots = {}

-- In a calibrating run, `stage`'s picture ("before", "after") of knob `name` at `zoom`, once per
-- session, with the reader's marks; written on the image worker, waited for by nothing.
local function shot(name, stage, snap, region, marks, zoom)
  if not (snap and O.calibrating()) then return end
  local file = string.format("amount-%s-%s-%dpct", string.gsub(name, " ", "-"), stage, zoom or 0)
  if shots[file] then return end
  shots[file] = true
  local path = host.path("calibration/" .. file .. ".png")
  local ok, err = pcall(host.screen.saveMarked, path, { snapshot = snap, region = region, marks = marks or {} },
    function(saved, why)
      host.log.info(string.format("[avenger] [calibrate] the %s picture of '%s' -> %s (%s)", stage, name, path,
        saved and "true" or ("false: " .. tostring(why))))
    end)
  if not ok then host.log.info("[avenger] [calibrate] the knob's picture not asked for: " .. tostring(err)) end
end

-- ── The gesture ──────────────────────────────────────────────────────────────

-- Spec `spec`'s gesture in trip `trip` on overlay `o`: the trip's act, a handler where reads wait.
-- `trip:done` once, with what was read, or the trip's give-up says the rest (actions.luau, go).
function Am.run(o, trip, spec)
  local function log(fmt, ...)
    host.log.info(("[avenger] [amount] '%s': " .. fmt):format(spec.name, ...))
  end
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then
    log("nothing pressed — VPS Avenger cannot be placed now (no factor)")
    return trip:done({ error = "VPS Avenger's size cannot be told now" })
  end
  local pl = P.placer(origin, k, (H.zoom(origin)))

  -- 1. The strip (or the spec's part), where the action needs an oscillator selected or another
  -- part shown: nothing pressed otherwise.
  local vb, before = nil, nil
  if spec.strip then
    local keysB, err
    vb, keysB, err = Act.partBefore(pl, spec.before or "strip", nil, log, spec.fields)
    if not trip:inFront() then return end
    if not vb then return trip:done({ error = err }) end
    before = { keys = keysB, values = vb }
    local why, focus = (spec.refuse or oscRefusal)(vb)
    if why then
      log("nothing pressed — %s", why)
      return trip:done({ part = before, error = why, focus = focus })
    end
  end
  local function part(fields) return spec.merge(before, fields) end

  -- A calibrating run's picture of the band, where no reader reads the knob (the filter): taken,
  -- written and let go. Whether one was taken — a wait, after which the trip is asked again.
  local function bandShot(stage)
    if not O.calibrating() then return false end
    local r = Open.region(pl, spec.band)
    if not r then return false end
    local okS, snap = pcall(host.screen.snapshot, { region = r })
    if not (okS and snap) then return true end
    shot(spec.name, stage, snap, r, {}, pl.zoom)
    snap:release()
    return true
  end

  -- 2. The knob's picture before the press, held through the hand-over: the baseline of the one
  -- change wait after Return. Its value is read for the log.
  local held = {}
  local region = nil
  local valueBefore = "no reader"
  if spec.knob then
    region = Open.region(pl, spec.box)
    if not region then
      log("nothing pressed — the knob is not on the screen")
      return trip:done({ part = before, error = "that part of VPS Avenger is not on the screen" })
    end
    local okS, snap, why = pcall(host.screen.snapshot, { region = region })
    if not (okS and snap) then
      log("nothing pressed — no picture of the knob: %s", tostring(okS and why or snap))
      return trip:done({ part = before, error = "VPS Avenger cannot be read now" })
    end
    held.before = snap
    if not trip:inFront() then return release(held) end
    local marks = {}
    local okK, fields, words = pcall(Knob.read, pl, snap, spec.knob, marks)
    local v = okK and type(fields) == "table" and spec.value(fields) or nil
    valueBefore = v ~= nil and spec.words(v) or "not read"
    log("before the press — %s; %s", valueBefore, okK and tostring(words) or ("the read failed: " .. tostring(fields)))
    shot(spec.name, "before", snap, region, marks, pl.zoom)
  elseif spec.band then
    bandShot("before")
    if not trip:inFront() then return end
  end

  -- 3. Where the press goes, checked as every click of the module's is (main.luau's pointOf).
  local x0, y0 = E.ring.pointOf(o, spec.label, spec.at, true)
  if not x0 then
    release(held)
    return trip:done({ part = before, sentence = y0 })
  end

  -- The pointer's place along the travel, in table points: every point placed as the runtime places
  -- one (pl:point), so the start places exactly at the press.
  local start = spec.axis == "y" and spec.at[2] or spec.at[1]
  local pos = start
  local function place(p)
    if spec.axis == "y" then return pl:point(spec.at[1], p) end
    return pl:point(p, spec.at[2])
  end
  local buttonName = spec.button or "left"
  local opts = spec.button and { button = spec.button } or nil
  local down = false     -- the button is held: pressed by this gesture, not let go of yet
  local moved = false    -- a key moved the knob from where it was pressed
  local ended = nil      -- "Return" or "Escape", once onKey has had one
  local moves, clips, lastMoveAt = 0, 0, nil

  -- The button let go of, once, wherever the gesture ends — `why` for the log: the key, onEnd's
  -- `how`, or a give-up's message. After a disable the host has let go already, and its record says
  -- so: nothing is sent again.
  local function letGo(why)
    if not down then return end
    down = false
    local okH, list = pcall(host.input.held)
    if okH and type(list) == "table" and not table.find(list, buttonName) then
      log("%s — the host let go of the %s button already; not released again", why, buttonName)
      return
    end
    local x, y = place(pos)
    host.input.mouseUp(x, y, opts)
    log("%s — the %s button released at (%d,%d) after %d move%s (net %+d, %d at the limit)%s", why, buttonName, x, y,
      moves, moves == 1 and "" or "s", pos - start, clips,
      lastMoveAt and string.format(", the last %d ms before", host.now() - lastMoveAt) or "")
  end

  -- PageUp and PageDown: one move each; Return and Escape: the button let go of, and the end.
  local function onKey(_, _, key)
    if key == "Return" or key == "Escape" then
      letGo(key)
      ended = key
      return true
    end
    local to = pos + (key == "PageUp" and -1 or 1)
    local clipped = to < spec.lo or to > spec.hi
    if clipped then to = pos end
    pos = to
    moves += 1
    if not clipped then moved = true end
    lastMoveAt = host.now()
    local x, y = place(pos)
    host.input.move(x, y)
    if clipped then
      clips += 1
      E.sound("Slider")
      if clips == 1 then log("%s at the limit (%d) — Slider.wav; the next ones are counted", key, pos) end
    end
    return false
  end

  -- The knob read after Return, off picture `snap` (nil: none), and the trip done with it.
  local function readAfter(t2, snap, how, info)
    if not snap then
      log("no picture after Return — %s", tostring(info))
      return t2:done({ part = part(nil), error = spec.unread() })
    end
    local marks = {}
    local okK, fields, words = pcall(Knob.read, pl, snap, spec.knob, marks)
    shot(spec.name, "after", snap, region, marks, pl.zoom)
    snap:release()
    if not okK then
      log("the read after Return failed: %s", tostring(fields))
      fields, words = nil, nil
    end
    local v = fields and spec.value(fields)
    log("read %s (%s before)%s; %s", v ~= nil and spec.words(v) or "nothing", valueBefore, info and string.format(
      " — changed %s, settled %s, after %s ms in %s picture(s)", tostring(info.changed), tostring(info.settled),
      tostring(info.waited), tostring(info.frames)) or "", tostring(words))
    if v == nil then return t2:done({ part = part(fields), error = spec.unread() }) end
    local said = spec.words(v)
    if how == "unchanged" then
      return t2:done({ part = part(fields), note = "unchanged, " .. said, said = said .. ", unchanged" })
    end
    t2:done({ part = part(fields), said = said, focus = spec.focus(vb) })
  end

  -- What the keys did, not read (yet): the knob noted as not read in the state, so that a give-up
  -- posts it so, not as it was before the press.
  local function notRead()
    local w = E.plugin()
    local p = part(nil)
    if w and p then S.part(w.id, p.keys, p.values) end
  end

  local function onEnd(_, t2, how, _, err)
    -- First, before anything that can raise: every end lets go of the button.
    letGo(err and tostring(err) or how)
    if how ~= "ended" or not t2:inFront() then
      release(held)
      if moved and spec.knob then notRead() end
      return
    end
    if ended == "Escape" then
      release(held)
      return t2:done({ part = (moved and spec.knob) and part(nil) or nil, focus = spec.focus(vb) })
    end
    if not spec.knob then
      if spec.band and bandShot("after") and not t2:inFront() then return end
      return t2:done({ part = part(nil), focus = spec.focus(vb) })
    end
    -- Nothing moved: the knob as it is, read at once.
    if not moved then
      release(held)
      local asked, err = pcall(host.screen.snapshot, { region = region }, function(snap, why)
        if not t2:inFront() then
          if snap then snap:release() end
          return
        end
        readAfter(t2, snap, "read", snap == nil and why or nil)
      end)
      if not asked then readAfter(t2, nil, "read", err) end
      return
    end
    -- Moved: what Avenger drew of it, waited for from the picture before the press — the knob noted
    -- as not read first, which a give-up during the wait posts and the read after it overwrites.
    notRead()
    local asked, err = pcall(host.screen.snapshot, { region = region, change = { from = held.before,
      timeout = Act.CHANGE_WITHIN, tolerance = 16, minPixels = 4, settle = 50 } }, function(snap, why, info)
      release(held)
      if not t2:inFront() then
        if snap then snap:release() end
        return
      end
      if not snap then return readAfter(t2, nil, "unread", why) end
      local changed = type(info) == "table" and info.changed == true
      readAfter(t2, snap, changed and "changed" or "unchanged", info)
    end)
    if not asked then
      release(held)
      readAfter(t2, nil, "unread", err)
    end
  end

  -- 4. The keyboard, before the press: a hand-over refused leaves no button down.
  local handed, errH = trip:handOver({ kind = "keys", keys = Am.KEYS, onKey = onKey, onEnd = onEnd })
  if not handed then
    release(held)
    if not trip.live then return end -- given up already: onGiveUp posts it
    log("nothing pressed — the keyboard was not handed over (%s)", tostring(errH))
    return trip:done({ part = before, error = Am.NO_KEYBOARD })
  end

  -- 5. The press, held; then the tool's sentence, behind the screen reader.
  host.input.mouseDown(x0, y0, opts)
  down = true
  log("pressed (%d,%d) with the %s button; PageUp moves %s, PageDown %s, 1 point × %.2f each, %d..%d", x0, y0,
    buttonName, spec.axis == "y" and "up" or "left", spec.axis == "y" and "down" or "right", k, spec.lo, spec.hi)
  host.speech.output(spec.sentence(vb), { interrupt = false })
end

-- What actions.luau hands over: { ring = main.luau's helpers, plugin = () -> the window the page was
-- shown beside, sound = (name) -> (), the page's sounds behind its setting }.
function Am.start(env)
  E = env
end

return Am
