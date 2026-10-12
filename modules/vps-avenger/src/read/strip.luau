-- The oscillator strip, read off the page's picture: how many oscillators there are, which tab is
-- selected, and Mute and Solo of every tab; on the Drums tab, its pattern switch, its lock and
-- which of its two modes is chosen.
--
-- SOURCE: the avenger_control project's classifiers (its clicker/__init__.py: the OSC tabs' dots,
-- the selected tab's dark strip, the lit button, the green lock and the brighter caption), taken
-- over with its developer's permission (NOTICE): its colour tests and its numbers as it has them,
-- asked as shares of one picture rather than counted pixel by pixel (read/picture.luau). What it
-- counted in its own pixels is compared in its own pixels: a share times the pixels the project's
-- capture held there.

local P = host.include("src/read/picture.luau")
local T = host.include("src/tables/strip.luau")
local D = host.include("src/tables/drums.luau")

local S = {}

-- ── The colour tests ─────────────────────────────────────────────────────────
-- The project's, written as host.screen.predicate takes them, and checked by it when the module
-- loads. ACTIVE is a lit Mute, Solo or pattern switch: red — or blue in the same way, since the
-- project tested its pixels in both orders of the channels, not sure which its captures had (its
-- red_rgb and red_bgr). BRIGHT is (r + g + b) / 3 above 140, a button lit by its brightness.
-- DOT is the green or cyan dot every OSC tab has near its left edge; LOCK the lock's strong green.
S.ACTIVE = host.screen.predicate(
  "(r >= 126 and r - g >= 13 and r - b >= -7) or (b >= 126 and b - g >= 13 and b - r >= -7)")
S.BRIGHT = host.screen.predicate("r + g + b >= 421")
S.DOT = host.screen.predicate("g >= 170 and b >= 140 and r >= 75 and g - r >= 45 and g - b >= 10")
S.LOCK = host.screen.predicate("g >= 211 and g - r >= 81 and g - b >= 81")

-- A button is lit when a share of its pixels passes ACTIVE or BRIGHT: 22 %, and at least 18 of
-- the project's pixels.
local ACTIVE_SHARE = 0.22
local ACTIVE_FLOOR = 18

-- A dot, in the project's pixels at its scale s: at least max(4, 4·s²) of them, at most max(8,
-- 10·s) across. (It also took a dot as at most max(6, 8·s) tall, which a run over one row of
-- cells cannot see; the dots' band is 7 tall at 50 % anyway.)
local function dotLimits(s)
  return math.max(4, P.round(4 * s * s)), math.max(8, P.round(10 * s))
end

-- Of DRUMKIT's and SEQUENCE's captions, the one whose bright, unsaturated pixels are 8 brighter
-- on average than the other's. Its pixels: (r + g + b) / 3 above 95, and their channels less than
-- 130 apart.
local MODE_MARGIN = 8
local TEXT_LUMA = 95
local TEXT_SPREAD = 130
-- The points a caption is read at, at most.
local MODE_POINTS = 160

-- The OSC tabs' captions read as text, where the dots gave no count: "OSC 3", "0SC 3" or "OSG 3".
-- The highest number read is the count.
function S.countFromText(text)
  local t = string.upper(tostring(text or ""))
  local found, hi = {}, 0
  local at = 1
  while true do
    local a, b, n = string.find(t, "[O0]S[CG]%s*([1-8])", at)
    if not a then break end
    local before = a > 1 and string.sub(t, a - 1, a - 1) or ""
    local after = string.sub(t, b + 1, b + 1)
    if not string.find(before, "[%w]") and not string.find(after, "[%w]") then
      local v = tonumber(n)
      found[#found + 1] = v
      if v > hi then hi = v end
    end
    at = b + 1
  end
  if hi == 0 then return nil, found end
  return hi, found
end

-- The OSC tabs' names from the strip read whole as text (where the dots gave no count, and the
-- count came from the text): of each of `count` tabs, the words whose middles lie in it, joined;
-- nil for a tab with none.
function S.namesFromWords(pl, words, count)
  local out = {}
  for i = 1, count do
    local r = pl:rect(T.tabSpan(i, count, T.NAME_TOP, T.NAME_BOTTOM))
    local parts = {}
    for _, w in ipairs(words or {}) do
      local mx = w.x + w.w / 2
      if r and mx >= r[1] and mx < r[3] then parts[#parts + 1] = w.text end
    end
    if #parts > 0 then out[i] = table.concat(parts, " ") end
  end
  return out
end

-- ── What is asked ────────────────────────────────────────────────────────────

-- Mute and Solo of every tab lie in one row: from the leftmost Mute there can be (eight tabs) to
-- Trig's Solo, BUTTON_STATE tall round MUTE_Y.
local ROW = {
  T.tabX(1, 8) + T.MUTE_X_OFFSET - T.BUTTON_STATE[1] / 2, T.MUTE_Y - T.BUTTON_STATE[2] / 2,
  T.TRIG_SOLO[1] + T.BUTTON_STATE[1] / 2, T.MUTE_Y + T.BUTTON_STATE[2] / 2,
}
-- The selection is looked for from the panel's left to Trig's right.
local SELECTION = { T.PANEL_LEFT, T.SELECTION_TOP, T.TRIG_SELECTION[2], T.SELECTION_BOTTOM }

-- The band a tab's dark strip is read in: the bottom rows of SELECTION, in pixels of the picture.
local function band(pl)
  local r = pl:rect(SELECTION)
  if not r then return nil end
  local rows = P.bottomRows(pl, r[4] - r[2])
  return { r[1], r[4] - rows, r[3], r[4] }
end

function S.ask(pl, out)
  local function add(req) if req then out[#out + 1] = req end end
  add(P.columns("strip dots", pl:rect({ T.PANEL_LEFT, T.DOT_TOP, T.NAME_RIGHT, T.SELECTION_BOTTOM }), S.DOT))
  -- Every Mute and Solo is as tall as the row: one row of cells.
  local row = pl:rect(ROW)
  add(P.cells("strip active", row, S.ACTIVE, pl.k, 1))
  add(P.cells("strip bright", row, S.BRIGHT, pl.k, 1))
  local pattern = pl:box(D.PATTERN, D.PATTERN_STATE)
  add(P.cells("pattern active", pattern, S.ACTIVE, pl.k))
  add(P.cells("pattern bright", pattern, S.BRIGHT, pl.k))
  add(P.cells("lock", pl:box(D.MODE_LOCK, D.LOCK_STATE), S.LOCK, pl.k))
  add(P.profile("strip band", band(pl)))
end

-- ── The answers ──────────────────────────────────────────────────────────────

-- How many oscillators the dots say, or nil; and the dots, for the log and the picture.
function S.count(pl, got, log)
  local g = got["strip dots"]
  if not g then
    log("strip: the dots were not read (" .. tostring(got.why["strip dots"]) .. ")")
    return nil, {}
  end
  local minPx, maxW = dotLimits(pl.s)
  local dots, words = {}, {}
  for _, run in ipairs(g:runs()) do
    local w, px = (run.to - run.from) * pl.t, run.pixels * pl.t * pl.t
    local ok = px >= minPx and w <= maxW
    if ok then dots[#dots + 1] = run end
    words[#words + 1] = string.format("x %d-%d %.1f px (the project's %.1f of at least %d, %.1f across of at most %d)%s",
      run.from, run.to, run.pixels, px, minPx, w, maxW, ok and "" or " — no dot")
  end
  local count = #dots > 0 and math.min(8, #dots) or nil
  log(string.format("strip: %d dot%s%s — %s", #dots, #dots == 1 and "" or "s",
    #words > 0 and (": " .. table.concat(words, "; ")) or "",
    count and (count .. " oscillator" .. (count == 1 and "" or "s")) or "no count"))
  return count, dots
end

-- Box `size` round table point `p`: lit or not by ACTIVE and BRIGHT in grids `ga` and `gb`, or
-- nil; and the log's words.
local function lit(pl, ga, gb, p, size)
  local r = pl:box(p, size)
  local a = ga and r and ga:share(r)
  local b = gb and r and gb:share(r)
  if a == nil or b == nil then return nil, "not read" end
  local n = pl:toolPixels(r)
  local thr = math.max(ACTIVE_FLOOR, P.round(n * ACTIVE_SHARE))
  local on = a * n >= thr or b * n >= thr
  return on, string.format("%s red, %s bright (the project's %.0f and %.0f of %.0f, lit from %d) %s",
    P.pct(a), P.pct(b), a * n, b * n, n, thr, on and "ON" or "OFF")
end
S.lit = lit

-- The mean brightness of a caption's text pixels, from points of the picture; nil with none.
local function captionLuma(pl, snap, rect)
  if not rect then return nil, 0, 0 end
  local pts = P.spread(rect, MODE_POINTS)
  local ok, colours = pcall(host.screen.pixels, pts, { snapshot = snap })
  if not ok or type(colours) ~= "table" then return nil, #pts, 0 end
  local sum, n = 0, 0
  for _, c in ipairs(colours) do
    local luma = (c.r + c.g + c.b) / 3
    local spread = math.max(c.r, c.g, c.b) - math.min(c.r, c.g, c.b)
    if luma > TEXT_LUMA and spread < TEXT_SPREAD then
      sum += luma
      n += 1
    end
  end
  return n > 0 and sum / n or 0, #pts, n
end

-- Everything else of the strip, once the count is known (nil when neither the dots nor the text
-- gave one): into `v`, the read values. `marks` collects the points for the calibration picture.
function S.decide(pl, snap, got, count, v, log, marks)
  local ga, gb = got["strip active"], got["strip bright"]
  local words = {}
  local function mark(p) local x, y = pl:point(p[1], p[2]); marks[#marks + 1] = { x = x, y = y } end
  -- Mute and Solo of every OSC tab, then of Drums and Trig.
  if count then
    v.oscMute, v.oscSolo = {}, {}
    for i = 1, count do
      local x = T.tabX(i, count)
      local mp, sp = { x + T.MUTE_X_OFFSET, T.MUTE_Y }, { x + T.SOLO_X_OFFSET, T.MUTE_Y }
      local m, mw = lit(pl, ga, gb, mp, T.BUTTON_STATE)
      local s, sw = lit(pl, ga, gb, sp, T.BUTTON_STATE)
      v.oscMute[i], v.oscSolo[i] = m, s
      words[#words + 1] = string.format("OSC %d Mute %s, Solo %s", i, mw, sw)
      mark({ x, T.TAB_Y })
      mark(mp)
      mark(sp)
    end
  end
  local function one(label, p, key)
    local on, w = lit(pl, ga, gb, p, T.BUTTON_STATE)
    v[key] = on
    words[#words + 1] = label .. " " .. w
    mark(p)
  end
  one("Drums Mute", T.DRUMS_MUTE, "drumsMute")
  one("Drums Solo", T.DRUMS_SOLO, "drumsSolo")
  one("Trig Mute", T.TRIG_MUTE, "trigMute")
  one("Trig Solo", T.TRIG_SOLO, "trigSolo")
  log("strip: " .. table.concat(words, "; "))

  -- The selected tab: the darkest bottom strip of the OSC tabs, Drums and Trig.
  local prof = got["strip band"]
  if prof and count then
    local means, best = {}, nil
    local function tab(label, kind, index, x1, x2)
      local a = pl:point(x1, T.SELECTION_TOP)
      local b = pl:point(x2, T.SELECTION_TOP)
      local m = P.bandMean(prof, a, b)
      means[#means + 1] = string.format("%s %s", label, m and string.format("%.1f", m) or "not read")
      if m and (not best or m < best.mean) then best = { kind = kind, index = index, mean = m } end
    end
    for i = 1, count do
      local r = T.tabSpan(i, count, T.SELECTION_TOP, T.SELECTION_BOTTOM)
      tab("OSC " .. i, "osc", i, r[1], r[3])
    end
    tab("Drums", "drums", 1, T.DRUMS_SELECTION[1], T.DRUMS_SELECTION[2])
    tab("Trig", "trig", 1, T.TRIG_SELECTION[1], T.TRIG_SELECTION[2])
    if best then v.selectedType, v.selectedIndex = best.kind, best.index end
    log(string.format("strip: the tabs' bottom strips, darkest selected: %s — %s", table.concat(means, ", "),
      best and (best.kind == "osc" and ("OSC " .. best.index) or (best.kind == "drums" and "Drums" or "Trig"))
        or "none read"))
  else
    log("strip: the selected tab is not read (" .. (count and tostring(got.why["strip band"]) or "no count") .. ")")
  end

  -- The Drums tab's own: the pattern switch, the lock, the mode.
  if v.selectedType == "drums" then
    local on, w = lit(pl, got["pattern active"], got["pattern bright"], D.PATTERN, D.PATTERN_STATE)
    v.drumPattern = on
    mark(D.PATTERN)
    local lock = got["lock"]
    local lr = pl:box(D.MODE_LOCK, D.LOCK_STATE)
    local share, _, any = nil, nil, nil
    if lock and lr then share, _, any = lock:share(lr) end
    if share ~= nil then v.drumLock = any end
    mark(D.MODE_LOCK)
    local drum, n1, k1 = captionLuma(pl, snap, pl:rect({ D.MODE_DRUM_SELECTION[1], D.MODE_SELECTION_TOP,
      D.MODE_DRUM_SELECTION[2], D.MODE_SELECTION_BOTTOM }))
    local sq, n2, k2 = captionLuma(pl, snap, pl:rect({ D.MODE_SQ_SELECTION[1], D.MODE_SELECTION_TOP,
      D.MODE_SQ_SELECTION[2], D.MODE_SELECTION_BOTTOM }))
    -- The project's rule as it has it: DRUMKIT's caption the brighter means SEQUENCE is chosen.
    if drum and sq then
      if drum >= sq + MODE_MARGIN then
        v.drumMode = "sq"
      elseif sq >= drum + MODE_MARGIN then
        v.drumMode = "drum"
      end
    end
    mark(D.MODE_DRUM)
    mark(D.MODE_SQ)
    log(string.format("strip: Drums — pattern %s; lock %s green (the project's %.0f pixels) %s; DRUMKIT's "
      .. "caption %s over %d of %d points, SEQUENCE's %s over %d of %d — mode %s", w, P.pct(share),
      share and share * pl:toolPixels(lr) or 0, share == nil and "not read" or (any and "ON" or "OFF"),
      drum and string.format("%.1f", drum) or "not read", k1, n1, sq and string.format("%.1f", sq) or "not read",
      k2, n2, v.drumMode or "not read"))
  end
end

return S
