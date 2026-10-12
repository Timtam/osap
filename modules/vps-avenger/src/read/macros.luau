-- The macro panel, read off the page's picture: how many macro tabs there are and which is shown,
-- the shown tab's five names, its two buttons on or off, and its three knobs' amounts (those are
-- the knob reader's, read/knob.luau).
--
-- SOURCE: the avenger_control project's detect_macro_button_states and the search it makes over a
-- button (its clicker/__init__.py), and its count of the macro tabs from their captions (its
-- macro_collector.py), taken over with its developer's permission (NOTICE): its colour tests and
-- numbers, as shares of one picture (read/picture.luau). Two reads are this module's own, since
-- the project kept them in memory rather than reading them: which tab is shown — the darkest
-- bottom strip, as the project reads the oscillator and central tabs — and the five names, read
-- as text and checked against the preset database's (read/ocr.luau).

local P = host.include("src/read/picture.luau")
local M = host.include("src/tables/macros.luau")
local R = host.include("src/read/ocr.luau")

local Mc = {}

-- A button is on when its highlight — (r + g + b) / 3 above 100 — covers 3 % of it and at least
-- 30 of the project's pixels, or its blue covers 7 % and at least 55. Off is only taken when both
-- are under half of that (and under 8 at least); in between, the project kept what it had, and
-- this module says it was not read.
Mc.HIGHLIGHT = host.screen.predicate("r + g + b >= 301")
Mc.BLUE = host.screen.predicate("b >= 81 and b - r >= 9 and b - g >= 9")
local HIGHLIGHT_SHARE, HIGHLIGHT_FLOOR = 0.03, 30
local BLUE_SHARE, BLUE_FLOOR = 0.07, 55
local OFF_FLOOR = 8

-- The darkest tab has to be this much darker than the next to be taken as shown: the central
-- tabs' margin (read/centre.luau).
local TAB_MARGIN = 4

local TABS = { M.TAB_SPANS[1][1], M.TAB_SPANS[1][2], M.TAB_SPANS[4][3], M.TAB_SPANS[1][4] }

function Mc.ask(pl, out)
  local function add(req) if req then out[#out + 1] = req end end
  local b1, b2 = M.BUTTON_BOUNDS[1], M.BUTTON_BOUNDS[2]
  -- Both buttons are as tall as their row: one row of cells.
  local buttons = pl:rect({ b1[1], b1[2], b2[3], b2[4] })
  add(P.cells("macro highlight", buttons, Mc.HIGHLIGHT, pl.k, 1))
  add(P.cells("macro blue", buttons, Mc.BLUE, pl.k, 1))
  local r = pl:rect(TABS)
  if r then
    local rows = P.bottomRows(pl, r[4] - r[2])
    add(P.profile("macro tabs", { r[1], r[4] - rows, r[3], r[4] }))
  end
end

-- The two buttons into `v.macroButtons` (true, false, or nil where not read).
function Mc.buttons(pl, got, v, log, marks)
  local gh, gb = got["macro highlight"], got["macro blue"]
  local words = {}
  v.macroButtons = {}
  for i, b in ipairs(M.BUTTON_BOUNDS) do
    local r = pl:rect(b)
    local h = gh and r and gh:share(r)
    local u = gb and r and gb:share(r)
    if h ~= nil and u ~= nil then
      local n = pl:toolPixels(r)
      local hc, bc = h * n, u * n
      local hthr = math.max(HIGHLIGHT_FLOOR, P.round(n * HIGHLIGHT_SHARE))
      local bthr = math.max(BLUE_FLOOR, P.round(n * BLUE_SHARE))
      local on = hc >= hthr or bc >= bthr
      local off = hc <= math.max(OFF_FLOOR, P.round(hthr * 0.5)) and bc <= math.max(OFF_FLOOR, P.round(bthr * 0.5))
      if on then
        v.macroButtons[i] = true
      elseif off then
        v.macroButtons[i] = false
      end
      words[#words + 1] = string.format("button %d highlight %s, blue %s (the project's %.0f and %.0f of %.0f, on "
        .. "from %d or %d) %s", i, P.pct(h), P.pct(u), hc, bc, n, hthr, bthr,
        on and "ON" or (off and "OFF" or "neither, not read"))
    else
      words[#words + 1] = string.format("button %d not read", i)
    end
    local x, y = pl:point(M.BUTTON_X[i], M.BUTTON_Y)
    marks[#marks + 1] = { x = x, y = y }
  end
  log("macros: " .. table.concat(words, "; "))
end

-- The tab shown, of `count`, into `v.macroSelected`: the only one when there is one; the one with
-- the darkest bottom strip, by TAB_MARGIN, when there are more.
function Mc.selected(pl, got, count, v, log, marks)
  if not count then return end
  for i = 1, count do
    local x, y = pl:point(M.TAB_X[i], M.TAB_Y)
    marks[#marks + 1] = { x = x, y = y }
  end
  if count == 1 then
    v.macroSelected = 1
    return
  end
  local prof = got["macro tabs"]
  local ranked, words = {}, {}
  for i = 1, count do
    local span = M.TAB_SPANS[i]
    local x1 = pl:point(span[1], span[2])
    local x2 = pl:point(span[3], span[2])
    local m = prof and P.bandMean(prof, x1, x2)
    if m then ranked[#ranked + 1] = { i = i, mean = m } end
    words[#words + 1] = string.format("%d %s", i, m and string.format("%.1f", m) or "not read")
  end
  table.sort(ranked, function(a, b)
    if a.mean ~= b.mean then return a.mean < b.mean end
    return a.i < b.i
  end)
  local margin = ranked[1] and ((ranked[2] and ranked[2].mean or 999) - ranked[1].mean) or nil
  if margin and margin >= TAB_MARGIN then v.macroSelected = ranked[1].i end
  log(string.format("macros: the tabs' bottom strips %s — %s", table.concat(words, ", "),
    v.macroSelected and ("tab " .. v.macroSelected .. " shown, by " .. string.format("%.1f", margin))
      or "none taken as shown"))
end

-- The OCR entries of the macro panel: its tabs' captions, and the shown tab's five names.
function Mc.entries(pl, out)
  local r = pl:rect(M.TAB_TEXT)
  if r then out[#out + 1] = { name = "macro tabs", region = r } end
  for _, f in ipairs(M.FIELD_ORDER) do
    local fr = pl:rect(M.FIELD_NAMES[f])
    if fr then out[#out + 1] = { name = "macro " .. f, region = fr } end
  end
end

-- How many macro tabs the captions read say, into `v.macroCount`; the words for the log.
function Mc.count(byName, v)
  local tabs = byName["macro tabs"]
  local text = tabs and tabs.status == "text" and tabs.text or ""
  v.macroCount = R.macroTabCount(text)
  return string.format("tabs '%s' — %s", R.forLog(text),
    v.macroCount and (v.macroCount .. " tab" .. (v.macroCount == 1 and "" or "s")) or "no count")
end

-- The shown tab's five names into `v.macroLabels` (nil where not read) and `v.macroSnapped` (true
-- where the database's spelling replaced a near one). `vocab`: the database's five names of the
-- shown tab, or nil. `counted`: Mc.count's words, said in the same line.
function Mc.labels(byName, v, vocab, counted, log)
  local said = { counted }
  v.macroLabels, v.macroSnapped = {}, {}
  for i, f in ipairs(M.FIELD_ORDER) do
    local r = byName["macro " .. f]
    local t = r and r.status == "text" and R.tidy(r.text) or ""
    if t ~= "" then
      local name, snapped, ratio = R.snap(t, vocab and vocab[i])
      v.macroLabels[i] = name
      v.macroSnapped[i] = snapped
      said[#said + 1] = string.format("%s '%s'%s", f, t, snapped
        and string.format(" -> '%s', the database's (%.2f alike)", name, ratio) or "")
    else
      said[#said + 1] = string.format("%s not read (%s)", f, r and r.status or "not asked")
    end
  end
  log("macros: " .. table.concat(said, "; "))
end

return Mc
