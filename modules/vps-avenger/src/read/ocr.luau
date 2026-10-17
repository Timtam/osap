-- What a recogniser makes of Avenger's text, read the avenger_control project's way: its helpers
-- for misreads it measured — the blank preset, the OSC tabs' captions, the macro tabs' count —
-- taken over with its developer's permission (NOTICE), and the check of a name read against the
-- preset database's. Plain work on strings, at once and anywhere: nothing here reads the screen.

local R = {}

-- Avenger's blank preset, as Avenger writes it.
R.INIT_PRESET = "Init Preset"

-- The readings the project took for "Init Preset" as they stand, folded: lower case, and nothing
-- but letters and digits.
local INIT_READINGS = {
  ["init"] = true,
  ["initpreset"] = true,
  ["initpresst"] = true,
  ["lnitpreset"] = true,
  ["lnitpresst"] = true,
  ["hitpreset"] = true,
  ["hitpresst"] = true,
}

-- How alike a reading with "preset" or "presst" in it has to be to "init preset" to be taken for
-- it: the project's 0.84.
local INIT_RATIO = 0.84

-- Ratcliff/Obershelp, as Python's difflib.SequenceMatcher(None, a, b).ratio() computes it for
-- strings under 200 characters (its junk heuristic is off there): the longest common block,
-- earliest in `a` and then in `b` among equals, then the same on either side of it; 2 × the
-- characters matched / both lengths. Over bytes, where difflib counts characters — the same for
-- the ASCII these readings are. The preset database's search has its own, made for comparing
-- one name with thousands (presets.luau); this one compares two short strings.
local function bytesOf(s)
  local t = table.create(#s)
  for k = 1, #s do t[k] = string.byte(s, k) end
  return t
end

local function longest(a, b, alo, ahi, blo, bhi)
  local bi, bj, bk = alo, blo, 0
  local prev = {}
  for i = alo, ahi do
    local cur = {}
    for j = blo, bhi do
      if a[i] == b[j] then
        local k = (prev[j - 1] or 0) + 1
        cur[j] = k
        if k > bk then bi, bj, bk = i - k + 1, j - k + 1, k end
      end
    end
    prev = cur
  end
  return bi, bj, bk
end

local function matched(a, b, alo, ahi, blo, bhi)
  if alo > ahi or blo > bhi then return 0 end
  local i, j, k = longest(a, b, alo, ahi, blo, bhi)
  if k == 0 then return 0 end
  return k + matched(a, b, alo, i - 1, blo, j - 1) + matched(a, b, i + k, ahi, j + k, bhi)
end

function R.ratio(x, y)
  local n = #x + #y
  if n == 0 then return 1 end
  return 2 * matched(bytesOf(x), bytesOf(y), 1, #x, 1, #y) / n
end

-- Whether `text` is a reading of "Init Preset", Avenger's blank preset: the project's
-- _looks_like_init_preset_ocr (its actions.py), which says that at 50 % on a display that is not
-- Retina its recogniser often read "Init Preset" as "hit Presst" or the like. The text with runs
-- of space as one and in lower case is taken when, with only its letters and digits left, it is
-- one of INIT_READINGS; or when it holds "preset" or "presst" and is INIT_RATIO alike to "init
-- preset". The project takes such a reading for the blank preset so that a fresh Avenger does not
-- inherit the preset it showed before; this module says and compares it as "Init Preset". Only
-- ASCII is folded; a byte outside it is kept as a letter, as Python keeps a letter outside ASCII.
function R.isInitPreset(text)
  local norm = string.lower((string.gsub(tostring(text or ""), "%s+", " ")))
  norm = string.gsub((string.gsub(norm, "^ ", "")), " $", "")
  if norm == "" then return false end
  local compact = (string.gsub(norm, "[^%w\128-\255]", ""))
  if INIT_READINGS[compact] then return true end
  if string.find(norm, "preset", 1, true) or string.find(norm, "presst", 1, true) then
    return R.ratio(norm, "init preset") >= INIT_RATIO
  end
  return false
end

-- Whether two names are roughly the same: the project's _names_roughly_same (its actions.py) —
-- lower case (ASCII only), runs of space as one, and 0.88 alike or more. Nothing is the same as
-- nothing: an empty name on either side is not.
R.SAME_RATIO = 0.88

function R.roughlySame(a, b)
  local x, y = string.lower(R.tidy(a)), string.lower(R.tidy(b))
  if x == "" or y == "" then return false end
  if x == y then return true end
  return R.ratio(x, y) >= R.SAME_RATIO
end

-- `text` as a preset's name: "Init Preset" for a reading of it (R.isInitPreset), and whether it
-- was taken so; any other text as it is.
function R.presetName(text)
  if R.isInitPreset(text) then return R.INIT_PRESET, true end
  return text, false
end

-- ── Text as the page's reads take it ─────────────────────────────────────────

-- Runs of space as one, none at either end: a reading's rows run together as one line.
function R.tidy(text)
  local t = string.gsub(tostring(text or ""), "%s+", " ")
  t = string.gsub(t, "^ ", "")
  return (string.gsub(t, " $", ""))
end

-- A reading for a log line: on one line, cut at 60 bytes on a character's edge.
function R.forLog(text)
  local s = R.tidy(text)
  if #s > 60 then s = string.gsub(string.sub(s, 1, 60), "[\192-\255][\128-\191]*$", "") .. "…" end
  return s
end

-- An OSC tab's caption as read, cleaned the project's way (its _clean_osc_ocr_name, actions.py):
-- spaces tidied, a leading bullet or asterisk dropped, and a reading of Avenger's own caption for
-- tab `index` — "OSC", "0SC 2", "OSC l", "QSC 2" — taken for it, "OSC <index>". A name of the
-- sound designer's own ("OSC Lead", "Oscar Bass") is left as it is: only what reads as OSC and a
-- number, or OSC alone, is taken for the caption.
local OSC_DIGITS = { I = "1", L = "1", ["|"] = "1", O = "0", S = "5", B = "8" }

local BULLETS = { "•", "·", "*", " " }

function R.cleanOscName(raw, index)
  local clean = R.tidy(raw)
  local cut = true
  while cut do
    cut = false
    for _, b in ipairs(BULLETS) do
      if string.sub(clean, 1, #b) == b then
        clean, cut = string.sub(clean, #b + 1), true
      end
    end
  end
  clean = R.tidy(clean)
  if index == nil then return clean end
  local idx = math.clamp(math.floor(index), 1, 8)
  local compact = string.upper((string.gsub(clean, "[^A-Za-z0-9|Il]", "")))
  if compact == "" then return clean end
  local function once(s, a, b)
    local i = string.find(s, a, 1, true)
    if not i then return s end
    return string.sub(s, 1, i - 1) .. b .. string.sub(s, i + #a)
  end
  compact = once(once(once(compact, "0SC", "OSC"), "QSC", "OSC"), "0S", "OS")
  if compact == "OSC" then return "OSC " .. idx end
  if string.sub(compact, 1, 3) == "OSC" then
    local suffix = string.sub(compact, 4)
    local normal = string.gsub(suffix, ".", function(ch) return OSC_DIGITS[ch] or ch end)
    if normal == tostring(idx) then return "OSC " .. idx end
    if #suffix <= 2 and not string.find(normal, "%a") and tonumber(normal) == idx then return "OSC " .. idx end
  end
  return clean
end

-- How many macro tabs Avenger shows, from its tabs' captions read as text: the highest "MACRO n"
-- from 1 to 4; 1 for "Macro" with no number read (the project's own case: "macro i" for "Macro
-- 1", which says the first tab is there). The project's _parse_macro_tab_count
-- (macro_collector.py), except that a reading with no "macro" at all is no count — the project
-- answered 1 for it.
function R.macroTabCount(text)
  local norm = string.lower(R.tidy(text))
  local hi = 0
  for n in string.gmatch(norm, "macro%s*([1-4])") do hi = math.max(hi, tonumber(n)) end
  if hi > 0 then return hi end
  if string.find(norm, "macro", 1, true) then return 1 end
  return nil
end

-- How alike a name read has to be to the database's to be taken for it: the preset database's
-- (presets.luau, THRESHOLD), the project's DEFAULT_MATCH_THRESHOLD.
R.SNAP_RATIO = 0.72

-- A name read compared with the name the preset database has in its place (`expected`, or nil):
-- the database's spelling when the two are the same words, case and spacing aside; the database's
-- name, and `true` — to be marked — when they are R.SNAP_RATIO alike or more; otherwise the name
-- as read. With the ratio for the log.
function R.snap(read, expected)
  if not expected or expected == "" then return read, false, nil end
  local a, b = string.lower(R.tidy(read)), string.lower(R.tidy(expected))
  if a == b then return R.tidy(expected), false, 1 end
  local r = R.ratio(a, b)
  if r >= R.SNAP_RATIO then return R.tidy(expected), true, r end
  return read, false, r
end

return R
