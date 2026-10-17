-- DRUM SQ's actions (the plan's slice 8, its other half), for the page's trips and the ring alike:
-- each a spec for src/act.luau, as drums.luau's are, but one — LENGTH, a flow of its own below. By
-- the avenger_control tool's names and arguments (its actions.py), with its points and menu rows
-- (its clicker and header_map.py, src/tables/centre.luau), and its defects fixed: the tool clicked
-- DRUM SQ whatever the central tab showed — over the sample editor's tabs and envelope — and
-- answered most of them with nothing read. Here nothing is clicked unless the read before shows
-- DRUM SQ as the central tab, with any tab of the strip selected (the page shows DRUM SQ whatever the
-- strip shows), and every answer is what was read after:
--
--   central_drum_sq_pattern_player, central_drum_sq_auto_follow   PATTERN CHAIN, AUTO FOLLOW;
--                         read after
--   select_central_drum_sq_pattern {pattern}   A to D; the pattern read as selected after (the tool
--                         wrote it)
--   toggle_central_drum_sq_pattern {pattern}   B, C or D with the right button; its state read after
--   central_drum_sq_pattern_menu_item {item}   PATTERN MENU with the right button, its item at the
--                         tool's row: a COPY changes nothing Avenger shows, and is kept as a fact of
--                         the session, which the page's PASTE follows (state.luau, `sqCopied`) — the
--                         tool's page set it at the click, before anything was chosen; PASTE and CLEAR
--                         answered by the change wait on the steps, which nothing reads
--   central_drum_sq_speed {item}   SPEED, a menu opened with the LEFT button on its field, the field
--                         read before and after as text
--   central_drum_sq_sequence_menu_item {item}   SEQUENCE MENU with the right button; its item opens
--                         a file dialog, waited for, and DRUM SQ read once more after it
--   central_drum_sq_length {steps}   LENGTH double-clicked, its length read, then dragged from what
--                         it reads to the length asked and read again (the tool wrote the length
--                         asked, and dragged as if the double-click had set 64)
--   reset_central_drum_sq_control {control}   a double-click on VEL AMOUNT or SHUFFLE: the change
--                         wait's verdict, no value claimed
--
-- VEL AMOUNT and SHUFFLE turned (central_drum_sq_control) are a press held through a hand-over of the
-- keys (src/amount.luau, Am.drumSq), which refuses as these do (Sq.refusal).

local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local C = host.include("src/tables/centre.luau")
local Q = host.include("src/read/drumsq.luau")
local Open = host.include("src/read/open.luau")
local S = host.include("src/state.luau")
local Drums = host.include("src/actions/drums.luau")
local Sa = host.include("src/actions/sample.luau")

local Sq = {}

local E = nil -- what actions.luau hands over (Sq.start)

local box, verdict, told, itemKey = Drums.box, Drums.verdict, Drums.told, Drums.itemKey
local one = Sa.one

-- How long LENGTH's change is looked for after the double-click and after the drag: as Act.run's.
local CHANGE_WITHIN = 2000

-- Why nothing is clicked unless DRUM SQ reads as the central tab, and where the focus goes then.
function Sq.refusal(vb)
  if vb.central == nil then return "the central tab cannot be read now" end
  if vb.central ~= "drum_sq" then return "DRUM SQ is not the central tab; nothing was clicked", "central-tab-drum_sq" end
  return nil
end

local function onDrumSq(click)
  return function(vb)
    local why, focus = Sq.refusal(vb)
    if why then return nil, why, focus end
    return click
  end
end

local function sq(v) return v and v.drumSq end

-- A switch of the patterns' row: PATTERN CHAIN or AUTO FOLLOW, `key` its field of `drumSq`.
local function switch(at, size, key, words, focus)
  return function(_, label)
    local function valueOf(v) local d = sq(v); return d and d[key] end
    return {
      label = label,
      part = "drumsq",
      decide = onDrumSq({ at = at }),
      watch = function() return { box(at, size) } end,
      same = function(vb, va)
        local a = valueOf(va)
        return a ~= nil and a == valueOf(vb)
      end,
      answer = function(_, va, how)
        local a = valueOf(va)
        if a == nil then return { error = words .. " cannot be read now" } end
        return told(words .. " " .. (a and "ON" or "OFF"), how, focus)
      end,
    }
  end
end
Sq.central_drum_sq_pattern_player = switch(C.SQ_PATTERN_PLAYER, C.SQ_PATTERN_PLAYER_STATE, "chain", "PATTERN CHAIN",
  "central-drum-sq-pattern-player")
Sq.central_drum_sq_auto_follow = switch(C.SQ_AUTO_FOLLOW, C.SQ_AUTO_FOLLOW_STATE, "follow", "AUTO FOLLOW",
  "central-drum-sq-auto-follow")

function Sq.select_central_drum_sq_pattern(args, label)
  local p = args.pattern
  if not (type(p) == "string" and C.SQ_PATTERNS[p]) then return nil, tostring(p) .. " is no pattern of DRUM SQ's" end
  local word = "Pattern " .. string.upper(p)
  local function selectedOf(v) local d = sq(v); return d and d.selected end
  return {
    label = label,
    part = "drumsq",
    decide = onDrumSq({ at = C.SQ_PATTERNS[p] }),
    watch = function() return { Q.ROW } end,
    same = function(vb, va)
      local a = selectedOf(va)
      return a ~= nil and a == selectedOf(vb)
    end,
    answer = function(_, va, how)
      local s = selectedOf(va)
      if s == nil then return { error = "the selected pattern cannot be read now" } end
      local w = "Pattern " .. string.upper(s)
      if how == "unchanged" then
        return { note = "unchanged, " .. w .. " is selected", said = w .. " is selected, unchanged" }
      end
      -- With AUTO FOLLOW on and a sequence playing, the selection moves by itself.
      if s ~= p then return { said = w .. " selected", note = word .. " was clicked; " .. w .. " reads selected" } end
      return { said = w .. " selected", focus = "central-drum-sq-pattern-" .. p }
    end,
  }
end

-- B, C and D, in the order of `drumSq.states`.
local STATE_INDEX = { b = 1, c = 2, d = 3 }

function Sq.toggle_central_drum_sq_pattern(args, label)
  local p = args.pattern
  if p == "a" then return nil, "Pattern A cannot be switched on or off" end
  local i = type(p) == "string" and STATE_INDEX[p] or nil
  if not i then return nil, tostring(p) .. " is no pattern of DRUM SQ's" end
  local word = "Pattern " .. string.upper(p)
  local function stateOf(v) local d = sq(v); return d and d.states and d.states[i] end
  return {
    label = label,
    part = "drumsq",
    -- A plain right click, as the tool's: no menu opens on it.
    decide = onDrumSq({ at = C.SQ_PATTERNS[p], button = "right" }),
    watch = function() return { box(C.SQ_PATTERNS[p], C.SQ_PATTERN_STATE) } end,
    same = function(vb, va)
      local a = stateOf(va)
      return a ~= nil and a == stateOf(vb)
    end,
    answer = function(_, va, how)
      local a = stateOf(va)
      if a == nil then return { error = word .. " cannot be read now" } end
      return told(word .. " " .. (a and "ON" or "OFF"), how, "central-drum-sq-pattern-toggle-" .. p)
    end,
  }
end

-- The pattern menu's items, the page's words, and what a COPY copies.
Sq.PATTERN_ITEMS = { copy = "COPY", paste = "PASTE", copy_first_half = "COPY FIRST HALF",
  copy_second_half = "COPY SECOND HALF", paste_to_first_half = "PASTE TO FIRST HALF",
  paste_to_second_half = "PASTE TO SECOND HALF", clear = "CLEAR" }
local COPIES = { copy = "whole", copy_first_half = "first", copy_second_half = "second" }

function Sq.central_drum_sq_pattern_menu_item(args, label)
  local item = itemKey(args.item)
  local words = Sq.PATTERN_ITEMS[item]
  if not words then return nil, tostring(args.item) .. " is no item of the pattern menu" end
  local copy = COPIES[item]
  local focus = "central-drum-sq-pattern-menu"
  return {
    label = label,
    part = "drumsq",
    -- A COPY changes nothing Avenger shows: the menu taking it is all there is to read. PASTE is not
    -- refused without one — Avenger's own menu decides, and an item it does not take is "untaken".
    decide = onDrumSq({ at = C.SQ_PATTERN_MENU, button = "right", menu = {
      item = { C.SQ_PATTERN_MENU_X, C.SQ_PATTERN_MENU_Y[item] }, retries = 3, read = not copy } }),
    watch = function() return { C.SQ_STEPS } end,
    -- No `same`: nothing reads a step, so the change wait's verdict on the steps is the answer.
    answer = function(vb, _, how)
      if copy then
        local d = sq(vb)
        return { facts = { sqCopied = { pattern = d and d.selected, part = copy } }, said = words .. " chosen",
          focus = focus }
      end
      if how == "changed" then return { said = words .. " chosen; VPS Avenger redrew the pattern", focus = focus } end
      if how == "unchanged" then return { note = words .. " chosen; nothing VPS Avenger shows changed" } end
      return { error = words .. " chosen; DRUM SQ cannot be read now" }
    end,
  }
end

function Sq.central_drum_sq_speed(args, label)
  local item = Sa.noteKey(args.item)
  local words = C.VALUE_WORDS.sq_speed
  if not words[item] then return nil, tostring(args.item) .. " is no speed of DRUM SQ's" end
  return {
    label = label,
    part = "drumsq",
    fields = { "sq_speed" },
    decide = onDrumSq({ at = C.SQ_SPEED, menu = { button = "left", item = { C.SQ_SPEED_MENU_X, C.SQ_SPEED_MENU_Y[item] },
      retries = 3 } }),
    watch = function() return { C.valueRect("sq_speed") } end,
    same = function(vb, va) return verdict({ one(vb.sqSpeed, va.sqSpeed) }) end,
    answer = function(_, va, how)
      return Sa.menuAnswer("SPEED", words, item, va and va.sqSpeed, how, "central-drum-sq-speed")
    end,
  }
end

Sq.SEQUENCE_ITEMS = { save_sequence_as = "SAVE SEQUENCE AS", import_midi = "IMPORT MIDI",
  export_to_midi = "EXPORT TO MIDI" }

function Sq.central_drum_sq_sequence_menu_item(args, label)
  local item = itemKey(args.item)
  local words = Sq.SEQUENCE_ITEMS[item]
  if not words then return nil, tostring(args.item) .. " is no item of the sequence menu" end
  return {
    label = label,
    part = "drumsq",
    -- Each item opens a file dialog, the user's: waited for, and DRUM SQ read once more after it —
    -- IMPORT MIDI may change the patterns.
    decide = onDrumSq({ at = C.SQ_SEQUENCE_MENU, button = "right", menu = {
      item = { C.SQ_SEQUENCE_MENU_X, C.SQ_SEQUENCE_MENU_Y[item] }, retries = 3, dialog = "read" } }),
    answer = function(_, _, how)
      if how == "chosen" then return { said = words .. " chosen", focus = "central-drum-sq-sequence-menu" } end
      return { note = "no file dialog was seen" }
    end,
  }
end

-- VEL AMOUNT and SHUFFLE, by the tool's keys and aliases, and their words.
Sq.CONTROL_WORDS = { vel_amount = "VEL AMOUNT", shuffle = "SHUFFLE" }
local CONTROL_ALIASES = { vel = "vel_amount", velocity = "vel_amount", velocity_amount = "vel_amount" }

function Sq.controlKey(control)
  local key = string.gsub(string.lower(tostring(control or "")), "^%s+", "")
  key = string.gsub(string.gsub(key, "%s+$", ""), " ", "_")
  key = CONTROL_ALIASES[key] or key
  if Sq.CONTROL_WORDS[key] then return key end
  return nil, tostring(control) .. " is no control of DRUM SQ's"
end

function Sq.reset_central_drum_sq_control(args, label)
  local c, why = Sq.controlKey(args.control)
  if not c then return nil, why end
  local word = Sq.CONTROL_WORDS[c]
  return {
    label = label,
    part = "drumsq",
    decide = onDrumSq({ at = C.SQ_CONTROLS[c], double = true }),
    watch = function() return { C.sqKnobBox(c) } end,
    -- No `same`: nothing reads DRUM SQ's knobs.
    answer = function(_, _, how)
      local focus = "central-drum-sq-reset-" .. (string.gsub(c, "_", "-"))
      if how == "changed" then return { said = "VPS Avenger redrew " .. word, focus = focus } end
      if how == "unchanged" then return { note = "unchanged: VPS Avenger drew no change at " .. word } end
      return { error = word .. " cannot be read now" }
    end,
  }
end

-- ── LENGTH ───────────────────────────────────────────────────────────────────

-- The steps asked, as the tool takes them: a whole number rounded to four, 4 to 64 (30 is 32, 70
-- is 64); nil and why for no number — the tool took 32.
function Sq.lengthArg(args)
  local n = tonumber(args.steps)
  if not n then return nil, "no length was named" end
  return math.clamp(P.round(math.floor(n) / 4) * 4, 4, 64)
end

-- LENGTH's words, the page's (ui/panel.js, centralDrumSqLengthWords).
function Sq.lengthWords(steps)
  local beats = steps // 4
  return string.format("LENGTH %d BEAT%s, %d STEPS", beats, beats == 1 and "" or "S", steps)
end

-- Every picture `held` keeps, let go.
local function release(held)
  for k, s in pairs(held) do
    s:release()
    held[k] = nil
  end
end

-- central_drum_sq_length {steps}: in trip `trip`'s act on overlay `o`, a handler where reads wait,
-- and `trip:inFront()` after every wait — false has ended the trip, and its give-up says the rest:
--   1. DRUM SQ pictured and read with LENGTH, the picture held; nothing clicked off DRUM SQ;
--   2. LENGTH double-clicked where the tool double-clicks it, both clicks left;
--   3. a change wait on LENGTH from that picture, and LENGTH read again — what the double-click set,
--      which the tool took to be 64 and dragged from (taken so only where it cannot be read);
--   4. a drag from LENGTH's point by `C.SQ_LENGTH_STEP` points per four steps from what was read to
--      the length asked, down for a shorter one and up for a longer one (host.input.drag), none where
--      it reads the length asked already;
--   5. a change wait from the picture after the double-click, and LENGTH read once more: the answer.
-- The length is noted as not read in the state once the double-click has gone out, which a give-up
-- posts, and the read after the drag overwrites; where no read comes after, the part answered is the
-- last one read with its LENGTH taken out, never a LENGTH read before it last changed.
function Sq.length(o, trip, steps, label)
  local function log(fmt, ...)
    host.log.info(("[avenger] [length] " .. fmt):format(...))
  end
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return trip:done({ error = "VPS Avenger's size cannot be told now" }) end
  local pl = P.placer(origin, k, (H.zoom(origin)))
  local region = Open.partRegion(pl, "drumsq")
  local field = region and pl:rect(C.valueRect("sq_length"))
  if not (region and field) then return trip:done({ error = "that part of VPS Avenger is not on the screen" }) end
  local watch = { { math.max(field[1], region[1]), math.max(field[2], region[2]), math.min(field[3], region[3]),
    math.min(field[4], region[4]) } }
  local opts = { pl = pl, fields = { "sq_length" } }
  local held = {}   -- the pictures held: before, mid
  local words = {}  -- the log's, in order

  -- Part `drumsq` read off picture `s`: { keys, values }, or nil.
  local function readOff(s)
    local ok, v, keys = pcall(Open.part, s, "drumsq", opts)
    if ok then return { keys = keys, values = v } end
    log("the read failed: %s", tostring(v))
    return nil
  end
  local function stepsWords(n) return n and tostring(n) or "not read" end
  -- Part `p` (readOff's) without its LENGTH: read before the double-click or the drag, it says the
  -- rest of DRUM SQ as it is and LENGTH as not read.
  local function unread(p)
    if not p then return nil end
    local values = table.clone(p.values)
    values.sqLength = nil
    return { keys = p.keys, values = values }
  end

  -- 1. Before.
  local okS, snap, whyS = pcall(host.screen.snapshot, { region = region })
  if not (okS and snap) then
    log("nothing clicked — no picture of DRUM SQ: %s", tostring(okS and whyS or snap))
    return trip:done({ error = "VPS Avenger cannot be read now" })
  end
  held.before = snap
  local before = readOff(snap)
  if not trip:inFront() then return release(held) end
  if not before then
    release(held)
    return trip:done({ error = "reading VPS Avenger failed" })
  end
  local vb = before.values
  local why, focus = Sq.refusal(vb)
  if why then
    release(held)
    log("nothing clicked — %s", why)
    return trip:done({ part = before, error = why, focus = focus })
  end
  words[#words + 1] = "before " .. stepsWords(vb.sqLength)

  -- 6. The answer, from the read last made (`after`, or nil: none).
  local function answer(after, last)
    log("%s; after %s", table.concat(words, "; "), stepsWords(after and after.values.sqLength))
    local part = after or unread(last or before)
    local n = after and after.values.sqLength
    if n == nil then return trip:done({ part = part, error = "the length cannot be read now" }) end
    if n ~= steps then
      return trip:done({ part = part,
        note = "LENGTH reads " .. string.sub(Sq.lengthWords(n), 8) .. ", not " .. steps .. " steps" })
    end
    if vb.sqLength == steps then return trip:done({ part = part, note = "unchanged, " .. Sq.lengthWords(n) }) end
    trip:done({ part = part, said = Sq.lengthWords(n), focus = "central-drum-sq-length" })
  end

  -- A change wait on LENGTH from picture `from`, then `next(picture or nil, changed)` — the trip
  -- still in front.
  local function waitFrom(from, next)
    local asked, err = pcall(host.screen.snapshot, { region = region, change = { from = from, watch = watch,
      timeout = CHANGE_WITHIN, tolerance = 16, minPixels = 4 } }, function(s, whyW, info)
      if not trip:inFront() then
        if s then s:release() end
        return release(held)
      end
      if not s then log("no picture after the change wait: %s", tostring(whyW)) end
      next(s, type(info) == "table" and info.changed == true)
    end)
    if not asked then
      log("no change wait: %s", tostring(err))
      next(nil, false)
    end
  end

  -- 4 and 5: the drag from what the double-click left (`mid`, in steps), and the read after it.
  local function drag(middle, mid)
    local d = (mid - steps) / 4 * C.SQ_LENGTH_STEP
    if math.abs(d) <= 0.05 then
      words[#words + 1] = "no drag"
      release(held)
      return answer(middle)
    end
    local x0, y0 = E.ring.pointOf(o, label, C.SQ_LENGTH, true)
    local x1, y1 = nil, nil
    if x0 then x1, y1 = E.ring.pointOf(o, label, { C.SQ_LENGTH[1], C.SQ_LENGTH[2] + d }, true) end
    if not x1 then
      release(held)
      local said = x0 and y1 or y0
      log("%s; not dragged — %s", table.concat(words, "; "), tostring(said))
      return trip:done({ part = middle or unread(before), sentence = said })
    end
    host.input.drag(x0, y0, x1, y1)
    words[#words + 1] = string.format("dragged %.1f points (%d,%d)→(%d,%d)", d, x0, y0, x1, y1)
    if not trip:inFront() then return release(held) end
    waitFrom(held.mid, function(s)
      release(held)
      if not s then return answer(nil, middle) end
      local after = readOff(s)
      s:release()
      if not trip:inFront() then return end
      answer(after, middle)
    end)
  end

  -- 2 and 3: the double-click, and LENGTH read after it.
  local function afterDouble()
    -- What LENGTH is from here on is not known until it is read: a give-up posts it so.
    local w = E.plugin()
    if w then S.part(w.id, { "sqLength" }, {}) end
    waitFrom(held.before, function(s, changed)
      held.before:release()
      held.before = nil
      local middle = nil
      if s then
        held.mid = s
        middle = readOff(s)
        if not trip:inFront() then return release(held) end
      end
      local mid = middle and middle.values.sqLength
      words[#words + 1] = string.format("the double-click — %s, read %s", changed and "changed" or "unchanged",
        stepsWords(mid))
      -- No picture to wait from after it: the drag would have no baseline, and nothing is dragged.
      if not held.mid then return answer(nil) end
      if not mid then
        log("the length after the double-click is not read — taken as %d, the tool's", C.SQ_LENGTH_RESET)
        mid = C.SQ_LENGTH_RESET
      end
      drag(middle, mid)
    end)
  end

  local clicked, said = E.ring.doubleAt(o, label, C.SQ_LENGTH, nil, true, afterDouble)
  if not clicked then
    release(held)
    return trip:done({ part = before, sentence = said })
  end
end

-- What actions.luau hands over: { ring = main.luau's helpers, plugin = () -> the window the page was
-- shown beside }.
function Sq.start(env)
  E = env
end

return Sq
