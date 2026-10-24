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
-- DOT is the green or cyan dot every OSC tab has near its left edge while the oscillator plays;
-- LOCK the lock's strong green.
S.ACTIVE = host.screen.predicate(
  "(r >= 126 and r - g >= 13 and r - b >= -7) or (b >= 126 and b - g >= 13 and b - r >= -7)")
S.BRIGHT = host.screen.predicate("r + g + b >= 421")
local PLAYING = "g >= 170 and b >= 140 and r >= 75 and g - r >= 45 and g - b >= 10"
S.DOT = host.screen.predicate(PLAYING)
S.LOCK = host.screen.predicate("g >= 211 and g - r >= 81 and g - b >= 81")

-- The dot in its two other colours, which the project counted none of (this module's, measured):
-- SILENT pink, a muted oscillator's, or every other tab's while one is soloed — its core about
-- (248, 138, 175) on `OSC Tabs/Mutesolo/Avenger 100, 8 OSCs OSC 3 Mute.png` and `… OSC 8 Mute.png`
-- — and SOLO orange, the soloed oscillator's, about (246, 192, 151) on `… OSC 8 Solo.png`. None of
-- the three passes another's colour. DOT_ANY is any of them: the dots counted.
local SILENT = "r >= 190 and r - g >= 70 and b - g >= 15"
local SOLO = "r >= 190 and r - b >= 60 and g - b >= 20 and r - g >= 25"
S.DOT_SILENT = host.screen.predicate(SILENT)
S.DOT_SOLO = host.screen.predicate(SOLO)
S.DOT_ANY = host.screen.predicate("(" .. PLAYING .. ") or (" .. SILENT .. ") or (" .. SOLO .. ")")

-- The colour of a dot's middle pixel `c` ({ r, g, b }) by the three tests above, written out again:
-- "playing", "silent", "solo", or nil.
function S.dotColour(c)
  local r, g, b = c.r, c.g, c.b
  if g >= 170 and b >= 140 and r >= 75 and g - r >= 45 and g - b >= 10 then return "playing" end
  if r >= 190 and r - g >= 70 and b - g >= 15 then return "silent" end
  if r >= 190 and r - b >= 60 and g - b >= 20 and r - g >= 25 then return "solo" end
  return nil
end

-- A button is lit when a share of its pixels passes ACTIVE or BRIGHT: 22 %, and at least 18 of
-- the project's pixels.
local ACTIVE_SHARE = 0.22
local ACTIVE_FLOOR = 18

-- A dot, in the project's pixels at its scale s: at least max(4, 4·s²) of them, at most max(8,
-- 10·s) across and max(6, 8·s) tall — its detect_visible_osc_tab_count's limits.
local function dotLimits(s)
  return math.max(4, P.round(4 * s * s)), math.max(8, P.round(10 * s)), math.max(6, P.round(8 * s))
end

-- THE COUNT BY SPACING. Each dot's middle gives its tab's number, (x − the first dot's middle) /
-- the tabs' step + 1, by the step of the count (T.STEP_BY_COUNT) nearest the dots' middle gap and
-- within a quarter of it; one dot alone is one oscillator. Every dot has to land within a quarter
-- of a step of a whole number from 1 to the count, one dot to a tab, or the dots give no count:
-- a dot too many, or a step that fits no count, is a misread, and nothing is clicked by it.
-- Measured: 38.0 to 38.5 apart with eight tabs, 147 with two
-- (`OSC Tabs/Avenger 100, 8 OSCs, OSC 1 selected.png`, `… 2 OSCs, OSC 1 selected.png`);
-- neighbouring steps differ by at least 3 units.
-- A dot that was not found leaves the spacing, and the count, as they are.
local SPACING_SLACK = 0.25

-- THE SELECTED TAB, by the project's darkest bottom strip with a margin of this module's own: the
-- darkest has to lead the next by TAB_LEAD at least, and by no less than the others differ among
-- themselves — one stands out, the rest are alike. A read while Avenger redraws, every tab's strip
-- brighter by its own amount, is then not read (the second Mac test of the page, 2026-10-24: Trig
-- read selected by 7.3 over tabs 21 apart). Measured on the ten `OSC Tabs/Avenger 100, …` pictures,
-- the band ending at T.SELECTION_BAND_BOTTOM: leads of 16.0 to 19.0 over spreads of at most 2.3;
-- with Avenger's corner 0.75 of a unit low 5.0 to 6.0 over at most 0.7, and 0.75 high 14.6 to 19.3
-- over at most 11.1.
local TAB_LEAD = 5

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

-- The band a tab's dark strip is read in: as many rows as the project's bottom rows of SELECTION,
-- above T.SELECTION_BAND_BOTTOM, in pixels of the picture.
local function band(pl)
  local r = pl:rect(SELECTION)
  if not r then return nil end
  local rows = P.bottomRows(pl, r[4] - r[2])
  local _, bottom = pl:point(T.PANEL_LEFT, T.SELECTION_BAND_BOTTOM)
  return { r[1], bottom - rows, r[3], bottom }
end

function S.ask(pl, out)
  local function add(req) if req then out[#out + 1] = req end end
  -- The dots' band, whose every dot-coloured pixel is read off one grid (P.litCells, P.lit): every
  -- dot whole, as the project's count saw it.
  add(P.litCells("strip dots", pl:rect(T.DOT_BAND), S.DOT_ANY))
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

-- The count from dots `dots` (each with `mid`, its middle's screen x, left to right) by the
-- spacing, or nil and why; each dot gets its tab's number (`tab`).
local function bySpacing(pl, dots)
  local x0 = pl:exact(T.DOT_MIDDLE, 0)
  local n, step
  if #dots == 1 then
    n, step = 1, T.STEP_BY_COUNT[1][1]
  else
    local gaps = {}
    for i = 2, #dots do gaps[#gaps + 1] = (dots[i].mid - dots[i - 1].mid) / pl.k end
    table.sort(gaps)
    local m = #gaps
    local gap = m % 2 == 1 and gaps[(m + 1) // 2] or (gaps[m // 2] + gaps[m // 2 + 1]) / 2
    for c, st in ipairs(T.STEP_BY_COUNT) do
      if not step or math.abs(st[1] - gap) < math.abs(step - gap) then n, step = c, st[1] end
    end
    if math.abs(step - gap) > SPACING_SLACK * step then
      return nil, string.format("their middle gap of %.1f fits no count's step (the nearest %d, %d tabs')", gap, step, n)
    end
  end
  local taken = {}
  for _, d in ipairs(dots) do
    local at = (d.mid - x0) / (step * pl.k) + 1
    local tab = math.floor(at + 0.5)
    if math.abs(at - tab) > SPACING_SLACK or tab < 1 or tab > n or taken[tab] then
      for _, e in ipairs(dots) do e.tab = nil end
      return nil, string.format("the dot at x %.1f lands at tab %.2f of %d, %d apart", d.mid, at, n, step)
    end
    taken[tab], d.tab = true, tab
  end
  return n
end

-- How many oscillators the dots say, by their spacing, or nil; and the dots, for the log and the
-- picture: { mid (the middle's screen x), tab (its number, where the count was read), colour
-- (S.dotColour's, of its middle pixel, or nil) }, off snapshot `snap`, where their pixels' colours
-- are read. Every dot's colour counts: a muted tab's dot is pink, and while one tab is soloed every
-- other tab's is (SILENT, above).
function S.count(pl, got, log, snap)
  local g = got["strip dots"]
  if not g then
    log("strip: the dots were not read (" .. tostring(got.why["strip dots"]) .. ")")
    return nil, {}
  end
  local pixels, why = nil, "no picture to read the dots' colours in"
  if snap then pixels, why = P.lit(g, snap, S.dotColour) end
  if not pixels then
    log("strip: the dots were not read (" .. tostring(why) .. ")")
    return nil, {}
  end
  local colourAt = {}
  for _, q in ipairs(pixels) do colourAt[q[2] * 1048576 + q[1]] = q[3] end
  local left, right = g.rect[1], g.rect[3]
  local minPx, maxW, maxH = dotLimits(pl.s)
  local dots, words = {}, {}
  for _, c in ipairs(P.components(P.runsOf(pixels))) do
    local w, h, px = (c.x2 - c.x1) * pl.t, (c.y2 - c.y1) * pl.t, c.pixels * pl.t * pl.t
    -- A part the band's edge cuts is no dot whose size is known.
    local cut = c.x1 <= left or c.x2 >= right
    local ok = not cut and px >= minPx and w <= maxW and h <= maxH
    if ok then
      -- Its colour, the pixel's in the middle of its middle row.
      local row = c.runs[(#c.runs + 1) // 2]
      local mid = colourAt[row[1] * 1048576 + (row[2] + row[3] - 1) // 2]
      dots[#dots + 1] = { mid = (c.x1 + c.x2) / 2, colour = mid and S.dotColour(mid) }
    end
    words[#words + 1] = string.format("x %d-%d y %d-%d, %d px (the project's %.1f of at least %d, %.1f across of at "
      .. "most %d, %.1f tall of at most %d)%s", c.x1, c.x2, c.y1, c.y2, c.pixels, px, minPx, w, maxW, h, maxH,
      cut and " — cut by the band's edge" or (ok and "" or " — no dot"))
  end
  local count, spacing = nil, nil
  if #dots > 0 then count, spacing = bySpacing(pl, dots) end
  local said = {}
  for _, d in ipairs(dots) do
    said[#said + 1] = string.format("%.1f%s %s", d.mid, d.tab and (" tab " .. d.tab) or "", d.colour or "colour not read")
  end
  log(string.format("strip: %d dot%s%s%s — %s", #dots, #dots == 1 and "" or "s",
    #words > 0 and (": " .. table.concat(words, "; ")) or "", #said > 0 and ("; the dots " .. table.concat(said, ", ")) or "",
    count and (count .. " oscillator" .. (count == 1 and "" or "s") .. " by their spacing")
      or (spacing and ("no count: " .. spacing) or "no count")))
  return count, dots
end

-- What Strip.decide read by a count that turned out short (read/open.luau): the tabs' Mute and
-- Solo, and the selection where it was an OSC tab's — Drums' and Trig's places are their own.
function S.unread(v)
  v.oscMute, v.oscSolo = nil, nil
  if v.selectedType == "osc" then v.selectedType, v.selectedIndex = nil, nil end
end

-- Whether the tabs' names `names` (tab -> text read) say that `count` is short: a name read as
-- "OSC m" with m past it is Avenger's own name for a tab the count does not have. The highest such
-- m, or nil.
function S.namesPast(names, count)
  local past = nil
  for _, text in pairs(names) do
    local m = S.countFromText(text)
    if m and m > count and (not past or m > past) then past = m end
  end
  return past
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

  -- The selected tab: the darkest bottom strip of the OSC tabs, Drums and Trig, where it stands out
  -- (TAB_LEAD).
  local prof = got["strip band"]
  if prof and count then
    local means, ranked = {}, {}
    local function tab(label, kind, index, x1, x2)
      local a = pl:point(x1, T.SELECTION_TOP)
      local b = pl:point(x2, T.SELECTION_TOP)
      local m = P.bandMean(prof, a, b)
      means[#means + 1] = string.format("%s %s", label, m and string.format("%.1f", m) or "not read")
      if m then ranked[#ranked + 1] = { kind = kind, index = index, mean = m, label = label } end
    end
    for i = 1, count do
      local r = T.tabSpan(i, count, T.SELECTION_TOP, T.SELECTION_BOTTOM)
      tab("OSC " .. i, "osc", i, r[1], r[3])
    end
    tab("Drums", "drums", 1, T.DRUMS_SELECTION[1], T.DRUMS_SELECTION[2])
    tab("Trig", "trig", 1, T.TRIG_SELECTION[1], T.TRIG_SELECTION[2])
    table.sort(ranked, function(a, b) return a.mean < b.mean end)
    local verdict
    if #ranked < 2 then
      verdict = "none read"
    else
      local best = ranked[1]
      local lead, spread = ranked[2].mean - best.mean, ranked[#ranked].mean - ranked[2].mean
      if lead >= TAB_LEAD and lead >= spread then
        v.selectedType, v.selectedIndex = best.kind, best.index
        verdict = string.format("%s, by %.1f, the others within %.1f", best.label, lead, spread)
      else
        verdict = string.format("none: %s by %.1f, the others within %.1f", best.label, lead, spread)
      end
    end
    log(string.format("strip: the tabs' bottom strips, darkest selected: %s — %s", table.concat(means, ", "), verdict))
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

-- ── Under a Solo ─────────────────────────────────────────────────────────────
-- Avenger lights every other tab's Mute while a tab's Solo is on, in the same colour as a Mute
-- pressed (the second Mac test's pictures, `Oscillators/…`): a Mute read under a Solo says nothing
-- of its own. So while any Solo reads on, the Mute of a tab whose own Solo does not is said as
-- "silenced by OSC 1's Solo" — never "on" — and an explicit Mute there is told only once the Solo
-- is off. The tabs whose Solo reads on, by their labels ("OSC 1", "Drums", "Trig"), in the strip's
-- order — empty where none does or none was read.
function S.soloBy(v)
  local out = {}
  for i, on in pairs(v.oscSolo or {}) do
    if on == true then out[#out + 1] = i end
  end
  table.sort(out)
  for k, i in ipairs(out) do out[k] = "OSC " .. i end
  if v.drumsSolo == true then out[#out + 1] = "Drums" end
  if v.trigSolo == true then out[#out + 1] = "Trig" end
  return out
end

-- "silenced by OSC 1's Solo" for tab `kind` ("osc", "drums" or "trig"; `index` an OSC tab's) where
-- another tab's Solo reads on and its own does not; nil otherwise.
function S.silencedBy(v, kind, index)
  local own = kind == "osc" and v.oscSolo and v.oscSolo[index] or (kind == "drums" and v.drumsSolo)
    or (kind == "trig" and v.trigSolo)
  if own == true then return nil end
  local by = S.soloBy(v)
  if #by == 0 then return nil end
  if #by == 1 then return "silenced by " .. by[1] .. "'s Solo" end
  return "silenced by the Solos of " .. table.concat(by, ", ", 1, #by - 1) .. " and " .. by[#by]
end

return S
