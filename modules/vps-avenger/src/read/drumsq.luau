-- DRUM SQ, read off the page's picture while it is the central tab: whether PATTERN CHAIN and AUTO
-- FOLLOW are on, which of the patterns B, C and D are on, and which pattern is selected; and where
-- an action names it, its SPEED as text, which the project never read (it wrote no speed).
--
-- SOURCE: the avenger_control project's detect_central_drum_sq_pattern_states and
-- detect_central_drum_sq_selected_pattern (its clicker/__init__.py), and its check that the
-- picture is not implausibly dark (actions.py, _refresh_drum_sq_pattern_states), taken over with
-- its developer's permission (NOTICE): its boxes, colour tests and numbers, as shares and column
-- profiles of one picture (read/picture.luau).

local P = host.include("src/read/picture.luau")
local C = host.include("src/tables/centre.luau")
local S = host.include("src/read/strip.luau")
local R = host.include("src/read/ocr.luau")

local Q = {}

-- PATTERN CHAIN is on when its box's mean brightness is 90 or more, AUTO FOLLOW at 100.
local CHAIN_MEAN = 90
local FOLLOW_MEAN = 100
-- Darker than this everywhere, the picture is not DRUM SQ's, and nothing is taken from it.
local DARK_MEANS = 20

-- THE PATTERN SELECTED, by a rule of this module's own: the project's outline against a box 9
-- tall put the outline on the dark rows between the boxes (the second Mac test of the page,
-- 2026-10-24: "none by 1069.8"). In each pattern's rows (tables/centre.luau, SQ_PATTERN_ROWS) its
-- LEAD is the mean of its two brightest rows over its median row: the selected one's bright top and
-- bottom rows over its fill. The selected one leads by at least SELECTED_LEAD, and every other by
-- at most OTHERS_SHARE of that. Measured on the nine `Drum SQ/Pattern Chain/…` pictures: the
-- selected one 37.8 to 71.5, the others 6.3 to 15.3 — the letter drawn in every box — and with A
-- selected and on, B to D off, those 0.40 of its lead at most; the same with Avenger's corner
-- placed 2 points up or down.
local SELECTED_LEAD = 20
local OTHERS_SHARE = 0.5

-- The patterns' row: from PATTERN CHAIN's box's left to D's right, as tall as the selected box and
-- the rows its selection is read in.
local function rowOf()
  local chain = C.SQ_PATTERN_PLAYER
  local d = C.SQ_PATTERNS.d
  local h = C.SQ_PATTERN_SELECTED[2]
  return { chain[1] - C.SQ_PATTERN_PLAYER_STATE[1] / 2, math.min(d[2] - h / 2, C.SQ_PATTERN_ROWS[1]),
    d[1] + C.SQ_PATTERN_SELECTED[1] / 2, math.max(d[2] + h / 2, C.SQ_PATTERN_ROWS[2]) }
end
Q.ROW = rowOf()

-- The row as eight tall: the boxes of PATTERN CHAIN, AUTO FOLLOW and the patterns' states.
local BAND8 = { Q.ROW[1], C.SQ_PATTERNS.a[2] - C.SQ_PATTERN_STATE[2] / 2, Q.ROW[3],
  C.SQ_PATTERNS.a[2] + C.SQ_PATTERN_STATE[2] / 2 }

-- Pattern `key`'s rows, read for the selection: a table rectangle.
local function rowsOf(key)
  local p, half = C.SQ_PATTERNS[key], C.SQ_PATTERN_ROWS_ACROSS / 2
  return { p[1] - half, C.SQ_PATTERN_ROWS[1], p[1] + half, C.SQ_PATTERN_ROWS[2] }
end

function Q.ask(pl, out)
  local function add(req) if req then out[#out + 1] = req end end
  local row = pl:rect(Q.ROW)
  add(P.cells("sq active", row, S.ACTIVE, pl.k))
  add(P.cells("sq bright", row, S.BRIGHT, pl.k))
  add(P.profile("sq band", pl:rect(BAND8)))
  for _, key in ipairs(C.SQ_PATTERN_ORDER) do add(P.profile("sq rows " .. key, pl:rect(rowsOf(key)), "both")) end
end

-- A pattern's lead (above) off its rows' profile `prof`, or nil where it was not read.
local function leadOf(prof)
  local rows = prof and prof.rows
  if not (rows and prof.h >= 3) then return nil end
  local means = table.create(prof.h)
  for i = 1, prof.h do means[i] = (rows.r[i] + rows.g[i] + rows.b[i]) / 3 end
  table.sort(means)
  local n = #means
  local median = n % 2 == 1 and means[(n + 1) // 2] or (means[n // 2] + means[n // 2 + 1]) / 2
  return (means[n] + means[n - 1]) / 2 - median
end

-- DRUM SQ's SPEED, read as text where an action names it (read/open.luau's `fields`): its key,
-- and its name in the text read. LENGTH is a knob with its caption right of it, and no length is
-- drawn anywhere (measured on `Drum SQ/Pattern Chain/01 …`): it is not read
-- (src/actions/drumsq.luau, Sq.length; TODO.md, D10).
Q.FIELDS = { sq_speed = "sq speed" }
Q.FIELD_KEYS = { sq_speed = "sqSpeed" }

-- The fields asked (`fields`, Q.FIELDS' keys) off the text read (`byName`): `v.sqSpeed` as the
-- page's key ("1_8"), nil where it was not read. Only where DRUM SQ is the central tab read.
function Q.fields(byName, v, log, fields)
  local words = {}
  for _, key in ipairs(fields) do
    local r = byName and byName[Q.FIELDS[key]]
    local t = r and r.status == "text" and R.tidy(r.text) or ""
    local value, how = nil, nil
    if t ~= "" then
      local ratio
      value, ratio = R.noteValue(t, C.VALUE_WORDS.sq_speed)
      how = string.format(" (%.2f alike)", ratio or 0)
    end
    v[Q.FIELD_KEYS[key]] = value
    words[#words + 1] = string.format("%s %s%s", Q.FIELDS[key], t ~= "" and ("'" .. R.forLog(t) .. "' -> "
      .. tostring(value or "not taken")) or (r and r.status or "not answered"), how or "")
  end
  log("drum sq: " .. table.concat(words, "; "))
end

-- DRUM SQ's state into `v.drumSq`, when the central tab read is DRUM SQ. With `opts.onlyOn`, the
-- selected pattern is looked for only among A and the patterns this picture reads on — the
-- project's restriction for its watcher (its dispatcher's detect_central_drum_sq_selected_pattern
-- with the patterns enabled), from this read rather than from memory (src/watch.luau).
function Q.decide(pl, got, v, log, marks, opts)
  local ga, gb, band = got["sq active"], got["sq bright"], got["sq band"]
  local function mark(p) local x, y = pl:point(p[1], p[2]); marks[#marks + 1] = { x = x, y = y } end
  local function meanOf(p, size, adjust)
    local r = pl:box({ p[1], p[2] + adjust }, size)
    return r and P.bandMean(band, r[1], r[3])
  end
  local chainMean = meanOf(C.SQ_PATTERN_PLAYER, C.SQ_PATTERN_PLAYER_STATE, C.SQ_PATTERN_PLAYER_STATE_Y_ADJUST)
  local followMean = meanOf(C.SQ_AUTO_FOLLOW, C.SQ_AUTO_FOLLOW_STATE, C.SQ_AUTO_FOLLOW_STATE_Y_ADJUST)
  mark(C.SQ_PATTERN_PLAYER)
  mark(C.SQ_AUTO_FOLLOW)
  local states, stateWords, means = {}, {}, {}
  for i, key in ipairs({ "b", "c", "d" }) do
    local p = C.SQ_PATTERNS[key]
    local at = { p[1], p[2] + C.SQ_PATTERN_STATE_Y_ADJUST }
    local on, w = S.lit(pl, ga, gb, at, C.SQ_PATTERN_STATE)
    states[i] = on
    stateWords[#stateWords + 1] = key .. " " .. w
    local m = meanOf(p, C.SQ_PATTERN_STATE, C.SQ_PATTERN_STATE_Y_ADJUST)
    if m then means[#means + 1] = m end
  end
  if chainMean then means[#means + 1] = chainMean end
  local brightest = 0
  for _, m in ipairs(means) do brightest = math.max(brightest, m) end
  if #means == 0 or brightest < DARK_MEANS then
    log(string.format("drum sq: not read — %s", #means == 0 and "nothing of it was read"
      or string.format("implausibly dark, its means at most %.1f", brightest)))
    return
  end

  -- Which pattern is selected: the one that leads (above) — with `onlyOn`, of A and the patterns
  -- read on in this picture; otherwise of all four.
  local allowed = { a = true, b = true, c = true, d = true }
  if opts and opts.onlyOn then
    allowed = { a = true, b = states[1] == true, c = states[2] == true, d = states[3] == true }
  end
  local words, best, second = {}, nil, nil
  for _, key in ipairs(C.SQ_PATTERN_ORDER) do
    local lead = leadOf(got["sq rows " .. key])
    words[#words + 1] = string.format("%s %s", key, lead and string.format("%.1f", lead) or "not read")
    if lead and allowed[key] then
      if not best or lead > best.lead then
        second, best = best, { key = key, lead = lead }
      elseif not second or lead > second.lead then
        second = { key = key, lead = lead }
      end
    end
    mark(C.SQ_PATTERNS[key])
  end
  local selected, why
  if not best then
    why = "not read"
  elseif best.lead < SELECTED_LEAD then
    why = string.format("none: %s leads by %.1f, under %d", best.key, best.lead, SELECTED_LEAD)
  elseif second and second.lead > best.lead * OTHERS_SHARE then
    why = string.format("none: %s leads by %.1f and %s by %.1f, over half of it", best.key, best.lead, second.key,
      second.lead)
  else
    selected = best.key
    why = second and string.format("%s, by %.1f, the next %s by %.1f", best.key, best.lead, second.key, second.lead)
      or string.format("%s, by %.1f, the only one looked at", best.key, best.lead)
  end

  v.drumSq = { states = states, selected = selected }
  if chainMean then v.drumSq.chain = chainMean >= CHAIN_MEAN end
  if followMean then v.drumSq.follow = followMean >= FOLLOW_MEAN end
  log(string.format("drum sq: PATTERN CHAIN mean %s (on from %d), AUTO FOLLOW %s (on from %d); %s; the patterns' "
    .. "leads %s; selected%s: %s", chainMean and string.format("%.1f", chainMean) or "not read", CHAIN_MEAN,
    followMean and string.format("%.1f", followMean) or "not read", FOLLOW_MEAN, table.concat(stateWords, "; "),
    table.concat(words, ", "), (opts and opts.onlyOn) and " among A and the patterns read on" or "", why))
end

return Q
