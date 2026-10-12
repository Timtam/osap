-- DRUM SQ, read off the page's picture while it is the central tab: whether PATTERN CHAIN and AUTO
-- FOLLOW are on, which of the patterns B, C and D are on, and which pattern is selected.
--
-- SOURCE: the avenger_control project's detect_central_drum_sq_pattern_states and
-- detect_central_drum_sq_selected_pattern (its clicker/__init__.py), and its check that the
-- picture is not implausibly dark (actions.py, _refresh_drum_sq_pattern_states), taken over with
-- its developer's permission (NOTICE): its boxes, colour tests and numbers, as shares and column
-- profiles of one picture (read/picture.luau).

local P = host.include("src/read/picture.luau")
local C = host.include("src/tables/centre.luau")
local S = host.include("src/read/strip.luau")

local Q = {}

-- PATTERN CHAIN is on when its box's mean brightness is 90 or more, AUTO FOLLOW at 100.
local CHAIN_MEAN = 90
local FOLLOW_MEAN = 100
-- Darker than this everywhere, the picture is not DRUM SQ's, and nothing is taken from it.
local DARK_MEANS = 20
-- A selected pattern that is switched off has an outline brighter than its inside: by at least 4,
-- and 5 more than any other switched-off pattern's. A selected pattern that is on: the brightest
-- outline of those on, at least 80, and 8 brighter than the next.
local OFF_SCORE, OFF_MARGIN = 4, 5
local ON_BORDER, ON_MARGIN = 80, 8
-- The outline is the project's two pixels round its box.
local BORDER_PIXELS = 2

-- The patterns' row: from PATTERN CHAIN's box's left to D's right, as tall as the selected box.
local function rowOf()
  local chain = C.SQ_PATTERN_PLAYER
  local d = C.SQ_PATTERNS.d
  local h = C.SQ_PATTERN_SELECTED[2]
  return { chain[1] - C.SQ_PATTERN_PLAYER_STATE[1] / 2, d[2] - h / 2, d[1] + C.SQ_PATTERN_SELECTED[1] / 2, d[2] + h / 2 }
end
Q.ROW = rowOf()

-- The row as eight tall: the boxes of PATTERN CHAIN, AUTO FOLLOW and the patterns' states.
local BAND8 = { Q.ROW[1], C.SQ_PATTERNS.a[2] - C.SQ_PATTERN_STATE[2] / 2, Q.ROW[3],
  C.SQ_PATTERNS.a[2] + C.SQ_PATTERN_STATE[2] / 2 }

-- The selected boxes' row in pixels of the picture, and its outline's width there.
local function selectedRow(pl)
  local a, d = C.SQ_PATTERNS.a, C.SQ_PATTERNS.d
  local half = C.SQ_PATTERN_SELECTED[1] / 2
  local r = pl:rect({ a[1] - half, Q.ROW[2], d[1] + half, Q.ROW[4] })
  local b = math.max(1, math.floor(BORDER_PIXELS / pl.t + 0.5))
  return r, b
end

function Q.ask(pl, out)
  local function add(req) if req then out[#out + 1] = req end end
  local row = pl:rect(Q.ROW)
  add(P.cells("sq active", row, S.ACTIVE, pl.k))
  add(P.cells("sq bright", row, S.BRIGHT, pl.k))
  add(P.profile("sq band", pl:rect(BAND8)))
  local r, b = selectedRow(pl)
  if r and r[4] - r[2] > 2 * b then
    add(P.profile("sq top", { r[1], r[2], r[3], r[2] + b }))
    add(P.profile("sq inner", { r[1], r[2] + b, r[3], r[4] - b }))
    add(P.profile("sq bottom", { r[1], r[4] - b, r[3], r[4] }))
  end
end

-- The sum of brightness, and the pixel count, of profile `prof` between screen x1 and x2.
local function sumOf(prof, x1, x2)
  if not prof or x2 <= x1 then return 0, 0 end
  local m, cols = P.bandMean(prof, x1, x2)
  if not m then return 0, 0 end
  return m * cols * prof.h, cols * prof.h
end

-- DRUM SQ's state into `v.drumSq`, when the central tab read is DRUM SQ.
function Q.decide(pl, got, v, log, marks)
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

  -- Which pattern is selected: each box's outline against its inside, from three profiles.
  local top, inner, bottom = got["sq top"], got["sq inner"], got["sq bottom"]
  local _, b = selectedRow(pl)
  local scores, borders, active, words = {}, {}, {}, {}
  for _, key in ipairs(C.SQ_PATTERN_ORDER) do
    local p = C.SQ_PATTERNS[key]
    local at = { p[1], p[2] + C.SQ_PATTERN_SELECTED_Y_ADJUST }
    local on = S.lit(pl, ga, gb, at, C.SQ_PATTERN_SELECTED)
    local r = pl:box(at, C.SQ_PATTERN_SELECTED)
    if r and top and inner and bottom and on ~= nil then
      local ts, tn = sumOf(top, r[1], r[3])
      local bs, bn = sumOf(bottom, r[1], r[3])
      local ls, ln = sumOf(inner, r[1], r[1] + b)
      local rs, rn = sumOf(inner, r[3] - b, r[3])
      local is, inn = sumOf(inner, r[1] + b, r[3] - b)
      local bn2 = tn + bn + ln + rn
      if bn2 > 0 and inn > 0 then
        local border, inside = (ts + bs + ls + rs) / bn2, is / inn
        scores[key], borders[key], active[key] = border - inside, border, on
        words[#words + 1] = string.format("%s outline %.1f, inside %.1f, mean %.1f%s", key, border, inside,
          (ts + bs + ls + rs + is) / (bn2 + inn), on and ", on" or "")
      end
    end
    mark(at)
  end
  -- Highest first, and of two alike the one first in A to D, as the project's stable sort has it.
  local function ranked(values, wanted)
    local out = {}
    for i, key in ipairs(C.SQ_PATTERN_ORDER) do
      if values[key] ~= nil and active[key] == wanted then out[#out + 1] = { key = key, v = values[key], i = i } end
    end
    table.sort(out, function(x, y)
      if x.v ~= y.v then return x.v > y.v end
      return x.i < y.i
    end)
    return out
  end
  local selected, confidence = nil, 0
  local off = ranked(scores, false)
  if off[1] then
    confidence = off[1].v - (off[2] and off[2].v or -999)
    if off[1].v >= OFF_SCORE and confidence >= OFF_MARGIN then selected = off[1].key end
  end
  if not selected then
    local on = ranked(borders, true)
    if on[1] then
      confidence = on[1].v - (on[2] and on[2].v or -999)
      if on[1].v >= ON_BORDER and confidence >= ON_MARGIN then selected = on[1].key end
    end
  end

  v.drumSq = { states = states, selected = selected }
  if chainMean then v.drumSq.chain = chainMean >= CHAIN_MEAN end
  if followMean then v.drumSq.follow = followMean >= FOLLOW_MEAN end
  log(string.format("drum sq: PATTERN CHAIN mean %s (on from %d), AUTO FOLLOW %s (on from %d); %s; selected: %s "
    .. "— %s by %.1f", chainMean and string.format("%.1f", chainMean) or "not read", CHAIN_MEAN,
    followMean and string.format("%.1f", followMean) or "not read", FOLLOW_MEAN, table.concat(stateWords, "; "),
    #words > 0 and table.concat(words, "; ") or "not read", selected or "none", confidence))
end

return Q
