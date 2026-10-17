-- The sample editor, read off a picture while the Drums tab is selected and EDITOR is the central
-- tab (read/open.luau's `editor` part, the page key's read and `central`'s): which of its four tabs
-- is shown, whether Avenger shows the editor or the sample browser in its place, Amp's, Pitch's or
-- EQ's switch on the tab shown, and — where an action names them — its fields as text: a menu's
-- field (Play Mode, Grid, an EQ band's type, Loop-Stretch), the envelope's preset name, the
-- browser's first rows.
--
-- SOURCE: the avenger_control project's detect_sample_editor_enable_state (its clicker/
-- __init__.py), taken over with its developer's permission (NOTICE): its blue test, its box and its
-- threshold — 1.5 % of the box's pixels, at least 2 — as the share of one grid. The project read
-- nothing else of the editor: it took the tab it clicked last for the one shown, the switch it
-- flipped in memory, and the browser for shown after a row's click. Here the tab is read by the
-- project's own rule for the central tab (read/centre.luau) — the darkest bottom strip, clearly
-- the darkest — applied to the editor's four tabs, a rule new on these tabs whose numbers a
-- calibrating run settles (TODO.md, D6); the browser by its rows' words; the fields as text.
--
-- In two steps, as read/open.luau calls them: the pixels before the part's one text read
-- (Sm.decidePixels), so that the text read asks the browser's words only where no tab reads, and
-- the words after it (Sm.decideText).

local P = host.include("src/read/picture.luau")
local C = host.include("src/tables/centre.luau")
local R = host.include("src/read/ocr.luau")

local Sm = {}

-- The project's blue of a switch that is on.
Sm.BLUE = host.screen.predicate("r >= 100 and g >= 125 and b >= 155 and b - r >= 15 and b - g >= 8")

-- The darkest tab has to be this much darker than the next, as the central tabs' rule has it.
local MARGIN = 4
-- A switch is on with 1.5 % of its box's pixels blue, and at least 2 of the project's pixels.
local BLUE_SHARE = 0.015
local BLUE_FLOOR = 2

-- The values the part reads, found or not; the fields only where they were asked.
Sm.KEYS = { "editorTab", "editorView", "editorOn" }
Sm.FIELD_KEYS = { "editorFields" }

-- The tabs' words, and the tabs a field is on: nil for any tab.
Sm.TAB_WORDS = { sample = "Sample", amp = "Amp", pitch = "Pitch", eq = "EQ" }
local FIELD_TABS = {
  play_mode = { sample = true },
  grid = { amp = true, pitch = true },
  eq1_type = { eq = true },
  eq2_type = { eq = true },
  envelope_name = { amp = true, pitch = true },
}

-- A field's rectangle at 50 %, by its key: a menu's field, the envelope's name, the browser's rows.
function Sm.fieldRect(key)
  if key == "envelope_name" then return C.ENVELOPE_NAME end
  if key == "browser" then return C.BROWSER_HEAD end
  return C.VALUES[key] and C.valueRect(key) or nil
end

-- A field's name in the text read: its key with spaces.
function Sm.regionName(key)
  return (string.gsub(key, "_", " "))
end

function Sm.ask(pl, out)
  local r = pl:rect(C.EDITOR_TABS_ROW)
  if r then
    local rows = P.bottomRows(pl, r[4] - r[2])
    out[#out + 1] = P.profile("editor tabs band", { r[1], r[4] - rows, r[3], r[4] })
  end
  local req = P.cells("editor switch", pl:rect(C.SWITCHES), Sm.BLUE, pl.k)
  if req then out[#out + 1] = req end
end

-- Whether the sample editor is read in `v`: on the Drums tab with EDITOR as the central tab.
function Sm.shown(v)
  return v.selectedType == "drums" and v.central == "editor"
end

-- Whether field `key` is asked in the text read of a picture whose pixels read `v`: true, or false
-- and why, for the log.
function Sm.applies(key, v)
  if not Sm.shown(v) then return false, "the sample editor is not shown" end
  if key == "browser" then
    if v.editorTab then return false, "the editor reads its " .. Sm.TAB_WORDS[v.editorTab] .. " tab" end
    return true
  end
  if v.editorTab == nil then return false, "no tab of the editor reads shown" end
  local tabs = FIELD_TABS[key]
  if tabs and not tabs[v.editorTab] then return false, "the " .. Sm.TAB_WORDS[v.editorTab] .. " tab is shown" end
  return true
end

-- The tab shown, the view by it, and the tab's switch, into `v`: only where Sm.shown(v). `marks`
-- collects the points for the calibration picture.
function Sm.decidePixels(pl, got, v, log, marks)
  local function mark(p) local x, y = pl:point(p[1], p[2]); marks[#marks + 1] = { x = x, y = y } end
  local prof = got["editor tabs band"]
  local ranked, words = {}, {}
  for _, key in ipairs(C.EDITOR_TAB_ORDER) do
    local p = C.EDITOR_TABS[key]
    local x1 = pl:point(p[1] - C.TAB_SIZE[1] / 2, p[2])
    local x2 = pl:point(p[1] + C.TAB_SIZE[1] / 2, p[2])
    local m = prof and P.bandMean(prof, x1, x2)
    if m then ranked[#ranked + 1] = { key = key, mean = m } end
    words[#words + 1] = string.format("%s %s", key, m and string.format("%.1f", m) or "not read")
    mark(p)
  end
  table.sort(ranked, function(a, b) return a.mean < b.mean end)
  local margin = nil
  if #ranked > 0 then
    margin = (#ranked > 1 and ranked[2].mean or 999) - ranked[1].mean
    if margin >= MARGIN then v.editorTab = ranked[1].key end
  end
  log(string.format("editor: the tabs' bottom strips %s; the darkest %s by %s — %s", prof and table.concat(words, ", ")
    or ("not read (" .. tostring(got.why["editor tabs band"]) .. ")"), ranked[1] and ranked[1].key or "none",
    margin and string.format("%.1f", margin) or "nothing", v.editorTab or ("none taken, under " .. MARGIN)))
  if v.editorTab then v.editorView = "editor" end

  -- The tab's switch: EQ's, or the envelope's for Amp and Pitch; Sample has none.
  local tab = v.editorTab
  if tab == "amp" or tab == "pitch" or tab == "eq" then
    local p = tab == "eq" and C.EQ_ENABLE_STATE or C.ENABLE_STATE
    local size = tab == "eq" and C.EQ_ENABLE_STATE_SIZE or C.ENABLE_STATE_SIZE
    local at = { p[1] + C.ENABLE_STATE_CENTER_ADJUST[1], p[2] + C.ENABLE_STATE_CENTER_ADJUST[2] }
    local g, r = got["editor switch"], pl:box(at, size)
    local share = g and r and g:share(r)
    mark(at)
    if share == nil then
      log(string.format("editor: %s's switch not read (%s)", Sm.TAB_WORDS[tab],
        g and "its box is not in the picture" or tostring(got.why["editor switch"])))
      return
    end
    local n = pl:toolPixels(r)
    local thr = math.max(BLUE_FLOOR, P.round(n * BLUE_SHARE))
    local on = share * n >= thr
    v.editorOn = { tab = tab, on = on }
    log(string.format("editor: %s's switch %s blue (the project's %.0f of %.0f, on from %d) %s", Sm.TAB_WORDS[tab],
      P.pct(share), share * n, n, thr, on and "ON" or "OFF"))
  end
end

-- After the text read (`byName`, the part's answers by region; nil where none was made): the view
-- by the browser's words where no tab read, and the fields asked (`fields`, keys) that applied —
-- a menu's field snapped to its menu's words (tables/centre.luau, VALUE_WORDS), the item's key;
-- anything else as read. Into `v.editorFields` where any field was asked.
function Sm.decideText(byName, v, log, fields)
  local function textOf(key)
    local r = byName and byName[Sm.regionName(key)]
    if not r then return nil, "not asked" end
    if r.status ~= "text" then return nil, tostring(r.status) end
    local t = R.tidy(r.text)
    if t == "" then return nil, "nothing read" end
    return t
  end
  if Sm.shown(v) and v.editorTab == nil then
    local t, why = textOf("browser")
    if t then
      local low = string.lower(t)
      for _, w in ipairs(C.BROWSER_WORDS) do
        if string.find(low, string.lower(w), 1, true) then
          v.editorView = "browser"
          break
        end
      end
    end
    log(string.format("editor: no tab reads shown; the browser's rows %s — %s", t and ("'" .. R.forLog(t) .. "'")
      or tostring(why), v.editorView == "browser" and "the sample browser" or "neither the editor nor the browser read"))
  end
  if not fields then return end
  v.editorFields = {}
  local words = {}
  for _, key in ipairs(fields) do
    if key ~= "browser" then
      local t, why = textOf(key)
      local vocab = C.VALUE_WORDS[key]
      if t and vocab then
        local item, ratio = R.noteValue(t, vocab)
        v.editorFields[key] = item
        words[#words + 1] = string.format("%s '%s' -> %s (%.2f alike)", key, R.forLog(t), item or "none of its items",
          ratio or 0)
      else
        v.editorFields[key] = t
        words[#words + 1] = string.format("%s %s", key, t and ("'" .. R.forLog(t) .. "'") or tostring(why))
      end
    end
  end
  if #words > 0 then log("editor: the fields " .. table.concat(words, "; ")) end
end

return Sm
