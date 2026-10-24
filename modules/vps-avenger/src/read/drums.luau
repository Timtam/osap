-- The drumkit table of the Drums tab, read off a picture (read/open.luau's `drums` part, and the
-- page key's read while the strip reads the Drums tab selected): every slot's Mute, Solo, Lock and
-- Loop, which slot is selected, and the twelve slots' names.
--
-- SOURCE: the avenger_control project's detect_drumkit_button_state (its clicker/__init__.py),
-- taken over with its developer's permission (NOTICE): a slot's button is lit by the project's own
-- test for a lit button — 22 % and at least 18 of its pixels red or bright, in a box 13 by 8 round
-- the button's point (read/strip.luau, S.lit) — asked here as the shares of two grids over the
-- table. The project read Mute, Solo and Lock so, and Loop not at all, flipping it from memory: here
-- Loop is read by the same test, a starting value (TODO.md, D4). It never read which slot is
-- selected, nor any slot's name — its page showed SELECTED on the last slot it clicked and the
-- factory names, changed only by what its menu choices were assumed to do. Here the names are
-- read as text, and the selection by a rule of this module's own (below), which a calibrating
-- run's picture and the log settle with the Avenger developer (TODO.md, D2).
--
-- THE SELECTION. Avenger draws its selected slot differently, or its sample editor could not show
-- "the selected slot"; how, nobody here has seen. So the rule looks only for ONE row that stands
-- out among the twelve, lighter or darker, by the background of the name column above and below
-- the name (tables/drums.luau, NAME_ROWS): of the twelve means, the one that differs most from
-- their median is taken when it differs by at least SELECTED_MARGIN and the next most by at most
-- half of that. Anything else — rows too alike, or two that stand out (zebra rows, a hover) — is
-- "not read", and the log says why.

local P = host.include("src/read/picture.luau")
local T = host.include("src/tables/drums.luau")
local Strip = host.include("src/read/strip.luau")
local R = host.include("src/read/ocr.luau")

local Dr = {}

-- How far the selected slot's background must stand out from the median of the twelve: the
-- project's margin between the two mode captions, as a starting value.
local SELECTED_MARGIN = 8
-- How much less the next one may stand out: half.
local NEXT_SHARE = 0.5

-- The values of a part with the table (state.luau's), found or not; the names only where the
-- part's text is read.
Dr.KEYS = { "kitMute", "kitSolo", "kitLock", "kitLoop", "kitSelected" }
Dr.NAME_KEYS = { "kitNames" }

-- The four buttons of a slot's row: the values' key, the column, the log's word.
local BUTTONS = {
  { "kitMute", "mute", "Mute" },
  { "kitSolo", "solo", "Solo" },
  { "kitLock", "lock", "Lock" },
  { "kitLoop", "loop", "Loop" },
}

-- Lock to Solo in one grid, Loop in another: their boxes, down the whole table.
local LEDS = { T.COLUMN_X.lock - T.BUTTON_STATE[1] / 2, T.TABLE_TOP, T.COLUMN_X.solo + T.BUTTON_STATE[1] / 2,
  T.TABLE_BOTTOM }
local LOOP = T.columnRect("loop", T.BUTTON_STATE[1])
local NAMES = { T.NAME_COLUMN[1], T.TABLE_TOP, T.NAME_COLUMN[2], T.TABLE_BOTTOM }

-- The grids' cells are two table points across: a slot's box, 13 by 8, covers about 7 by 4 of
-- them, which its share weighs by how much of each it covers — and the 48 shares of the table look
-- at a quarter of the cells that one-point cells would make them.
local CELL = 2

function Dr.ask(pl, out)
  local function add(req) if req then out[#out + 1] = req end end
  local leds, loop = pl:rect(LEDS), pl:rect(LOOP)
  add(P.cells("slot leds active", leds, Strip.ACTIVE, CELL * pl.k))
  add(P.cells("slot leds bright", leds, Strip.BRIGHT, CELL * pl.k))
  add(P.cells("slot loop active", loop, Strip.ACTIVE, CELL * pl.k))
  add(P.cells("slot loop bright", loop, Strip.BRIGHT, CELL * pl.k))
  -- Its rows are what is used: the background above and below each name.
  add(P.profile("slot names", pl:rect(NAMES), "both"))
end

-- The twelve slots' names as regions of the one text read, into `entries`.
function Dr.entries(pl, entries)
  for i = 1, #T.PART_LABELS do
    local r = pl:rect(T.nameRect(i))
    if r then entries[#entries + 1] = { name = "slot " .. i, region = r } end
  end
end

-- The background of slot `i`'s name column, above and below its name, from the profile's rows; nil
-- where the profile does not hold it.
local function background(pl, prof, i)
  local y = T.rowY(i)
  local a1 = select(2, pl:point(T.NAME_COLUMN[1], y - T.NAME_ROWS[2]))
  local a2 = select(2, pl:point(T.NAME_COLUMN[1], y - T.NAME_ROWS[1]))
  local b1 = select(2, pl:point(T.NAME_COLUMN[1], y + T.NAME_ROWS[1]))
  local b2 = select(2, pl:point(T.NAME_COLUMN[1], y + T.NAME_ROWS[2]))
  local m1, n1 = P.rowMean(prof, a1, a2)
  local m2, n2 = P.rowMean(prof, b1, b2)
  if not (m1 and m2) then return nil end
  return (m1 * n1 + m2 * n2) / (n1 + n2)
end

-- The selected slot by the rule above, or nil and why; and the log's words.
local function selected(pl, prof)
  if not prof then return nil, "not read (no profile of the name column)" end
  local means, words = {}, {}
  for i = 1, #T.PART_LABELS do
    local m = background(pl, prof, i)
    means[i] = m
    words[#words + 1] = string.format("%d %s", i, m and string.format("%.1f", m) or "not read")
    if not m then return nil, "not read (slot " .. i .. " is not in the picture)", table.concat(words, ", ") end
  end
  local sorted = table.clone(means)
  table.sort(sorted)
  local median = (sorted[6] + sorted[7]) / 2
  -- The one that stands out most, and how far the next does.
  local best, second = nil, 0
  for i, m in ipairs(means) do
    local d = math.abs(m - median)
    if not best then
      best = { i = i, d = d }
    elseif d > best.d then
      second, best = best.d, { i = i, d = d }
    else
      second = math.max(second, d)
    end
  end
  local list = table.concat(words, ", ")
  if best.d < SELECTED_MARGIN then
    return nil, string.format("too close: slot %d stands out by %.1f, under %d", best.i, best.d, SELECTED_MARGIN), list
  end
  if second > best.d * NEXT_SHARE then
    return nil, string.format("two alike: slot %d stands out by %.1f and another by %.1f", best.i, best.d, second), list
  end
  return best.i, string.format("slot %d, by %.1f from the median %.1f (the next by %.1f)", best.i, best.d, median, second),
    list
end

-- The table into `v`, off the answers `got` (and the text read, `byName`, or nil where the part's
-- text was not read): only where the strip reads the Drums tab selected. `marks` collects the points
-- for the calibration picture.
function Dr.decide(pl, got, byName, v, log, marks)
  local function mark(p) local x, y = pl:point(p[1], p[2]); marks[#marks + 1] = { x = x, y = y } end
  local words = {}
  for _, b in ipairs(BUTTONS) do v[b[1]] = {} end
  for i = 1, #T.PART_LABELS do
    local parts = {}
    for _, b in ipairs(BUTTONS) do
      local ga = b[2] == "loop" and got["slot loop active"] or got["slot leds active"]
      local gb = b[2] == "loop" and got["slot loop bright"] or got["slot leds bright"]
      local p = T.columnPoint(i, b[2])
      local on, w = Strip.lit(pl, ga, gb, p, T.BUTTON_STATE)
      v[b[1]][i] = on
      parts[#parts + 1] = b[3] .. " " .. w
      mark(p)
    end
    words[#words + 1] = string.format("slot %d %s", i, table.concat(parts, ", "))
  end
  log("drums: " .. table.concat(words, "; "))
  local s, why, means = selected(pl, got["slot names"])
  v.kitSelected = s
  log(string.format("drums: the name column's background %s — selected: %s", means or "not read", tostring(why)))
  for i = 1, #T.PART_LABELS do mark({ T.NAME_COLUMN[1] + 4, T.rowY(i) }) end
  if byName then
    v.kitNames = {}
    local said = {}
    for i = 1, #T.PART_LABELS do
      local r = byName["slot " .. i]
      local text = r and r.status == "text" and R.tidy(r.text) or ""
      if text ~= "" then v.kitNames[i] = text end
      said[#said + 1] = string.format("%d %s", i, text ~= "" and ("'" .. R.forLog(text) .. "'")
        or (r and r.status or "not answered"))
    end
    log("drums: the slots' names " .. table.concat(said, "; "))
  end
end

return Dr
