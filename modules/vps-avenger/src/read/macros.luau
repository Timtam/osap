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
-- as text where the preset database does not have the preset, and at the page key only, as a
-- check, where it does (Mc.count, Mc.labels: the database first, the maintainer's decision of
-- 2026-10-31, as the project names them).

local P = host.include("src/read/picture.luau")
local M = host.include("src/tables/macros.luau")
local R = host.include("src/read/ocr.luau")
local DB = host.include("src/presets.luau")

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

-- The OCR entries of the macro panel: its tabs' captions (`count`), and the shown tab's five names
-- (`names`) — each only where it is read (read/open.luau: where the preset database has the preset,
-- the names are its own, and the screen is read for them at the page key only, as a check).
function Mc.entries(pl, out, count, names)
  if count ~= false then
    local r = pl:rect(M.TAB_TEXT)
    if r then out[#out + 1] = { name = "macro tabs", region = r } end
  end
  if names == false then return end
  for _, f in ipairs(M.FIELD_ORDER) do
    local fr = pl:rect(M.FIELD_NAMES[f])
    if fr then out[#out + 1] = { name = "macro " .. f, region = fr } end
  end
end

local function tabs(n) return n .. " tab" .. (n == 1 and "" or "s") end

-- How many macro tabs there are, into `v.macroCount`: the preset database's where it has the
-- preset (`dbTabs`, its macro tabs, or nil) — the project's own source (its _current_macro_info),
-- and on Windows the only one there was: the caption read "UACRO 1" all through the second Windows
-- session (2026-10-31; mt-verified, finding 12) — else what the captions read (R.macroTabCount).
-- Where both are there and differ, the log says so. The words for the log.
function Mc.count(byName, v, dbTabs)
  local r = byName and byName["macro tabs"]
  local text = r and r.status == "text" and r.text or ""
  local read = r and R.macroTabCount(text) or nil
  local db = dbTabs and #dbTabs > 0 and math.min(4, #dbTabs) or nil
  v.macroCount = db or read
  local caption = r and string.format("tabs '%s' — %s", R.forLog(text), read and tabs(read) or "no count")
    or "tabs not read"
  if db then
    return string.format("%s; %s, the database's%s", caption, tabs(db),
      (read and read ~= db) and string.format(", where the caption reads %d — the database's taken", read) or "")
  end
  return caption
end

-- A dot word before a macro's name — "•", "·", "o", "O", "0" or "°" — is the pink dot each name field
-- has at its left: in the developer's Mac test of 2026-10-31 at 3.75 to 6.25 units from the field's
-- left edge (its macros picture at 80 %; the names begin at 15, "Soften To Pad" two pixels after the
-- dot), read as "•" or "0" on the Mac (mt-verified, finding 12) and as "o" before every name on
-- Windows. A first word that is one of those and whose box ends left of DOT_END from the field's left
-- is dropped by its place, the rest kept as read — never by cutting the region, which would cut
-- such a name. A box the recogniser only guessed has no place to go by: such a reading, and one
-- whose dot is no word of its own, loses only a lone bullet word and a space (R.withoutBullet).
local DOT_END = 7
local DOT_WORDS = { ["•"] = true, ["·"] = true, ["o"] = true, ["O"] = true, ["0"] = true, ["°"] = true }

-- The name read in field `f` (M.FIELD_ORDER's) from reading `r`, or nil: nothing read. And the
-- word dropped before it, or nil.
function Mc.fieldText(pl, f, r)
  if not (r and r.status == "text") then return nil end
  local words = r.words or {}
  local first = words[1]
  if first and #words > 1 and not first.approx and DOT_WORDS[R.tidy(first.text)] then
    local field = pl:rect(M.FIELD_NAMES[f])
    if field and type(first.x) == "number" and first.x + (first.w or 0) <= field[1] + DOT_END * pl.k then
      local rest = {}
      for j = 2, #words do rest[#rest + 1] = words[j].text end
      local t = R.tidy(table.concat(rest, " "))
      if t ~= "" then return t, R.tidy(first.text) end
    end
  end
  local t = R.tidy(r.text)
  if t == "" then return nil end
  return R.withoutBullet(t)
end

-- The shown tab's five names into `v.macroLabels` (nil where not known):
--   * where the preset database has the preset (`db`, its five names of the shown tab — the
--     maintainer's decision of 2026-10-31: the database first, as the project names them), its
--     names; read (`byName`, the page key's check) and the read not near the database's name, that
--     name MARKED, ", from the database" (src/presets.luau, WHAT IT MAY DO: a name the database
--     puts where the screen's read is not); not read, `kept[i]` — the page key's checked name,
--     marked or not — or the database's;
--   * elsewhere — Init Preset, a user's own preset, no database — as read, without the field's
--     dot (Mc.fieldText), in Avenger's own default's spelling where the read is near it (Avenger
--     shows "Macro 1" … "MacroBtn 2" where a name was never given: R.defaultMacroName), its look-
--     alike letters folded for the comparison (R.snap); and nil where nothing was read.
-- `counted`: Mc.count's words, said in the same line of the log.
function Mc.labels(pl, byName, v, db, counted, log, kept)
  local said = { counted }
  v.macroLabels = {}
  local tab = v.macroSelected
  for i, f in ipairs(M.FIELD_ORDER) do
    local r = byName and byName["macro " .. f]
    local t, dropped = Mc.fieldText(pl, f, r)
    local raw = r and r.status == "text" and R.forLog(r.text) or nil
    local how = dropped and string.format(" ('%s' before it dropped)", dropped) or ""
    if db and db[i] then
      local name = db[i]
      if t then
        local _, snapped, ratio = R.snap(t, name)
        if snapped or ratio == 1 then
          v.macroLabels[i] = name
          said[#said + 1] = string.format("%s '%s'%s, the database's '%s' (%.2f alike)", f, raw, how, name, ratio)
        else
          v.macroLabels[i] = name .. ", " .. DB.MARK
          said[#said + 1] = string.format("%s '%s'%s, not the database's '%s' (%.2f alike) — the database's, marked", f,
            raw, how, name, ratio or 0)
        end
      else
        v.macroLabels[i] = (kept and kept[i]) or name
        said[#said + 1] = string.format("%s '%s', the database's%s", f, v.macroLabels[i],
          r and string.format(" (not read: %s)", tostring(r.status)) or "")
      end
    elseif t then
      local default = tab and R.defaultMacroName(tab, i)
      local name, snapped, ratio = R.snap(t, default)
      v.macroLabels[i] = name
      said[#said + 1] = string.format("%s '%s'%s%s", f, raw, how, snapped
        and string.format(" -> '%s', Avenger's default (%.2f alike)", name, ratio) or "")
    else
      said[#said + 1] = string.format("%s not read (%s)", f, r and r.status or "not asked")
    end
  end
  log("macros: " .. table.concat(said, "; "))
end

return Mc
