-- What a recogniser makes of Avenger's text, read the avenger_control project's way: its helpers
-- for misreads it measured — the blank preset, the OSC tabs' captions, the macro tabs' count, the
-- default macro names — taken over with its developer's permission (NOTICE), and the check of a
-- name read against the preset database's; and this module's own for what the developer's Mac
-- test and the second Windows session of 2026-10-31 found: a lone bullet word before a name, the
-- look-alike letters of other scripts, words a recogniser only guessed. Plain work on strings, at
-- once and anywhere: nothing here reads the screen, and only R.presetName writes to the log.

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

-- Cyrillic and Greek letters a recogniser gives for the Latin letter they look like — the ones
-- Unicode's confusables list (UTS #39) maps to one Latin letter, of the shapes Avenger's names
-- use: Vision read Avenger's grey default "Macro 3" as "Масrо 3", Cyrillic М, а and о, under Init
-- Preset in the developer's Mac test of 2026-10-31, four times, and no name could be compared with
-- it (mt-verified, finding 12). Folded before any comparison of a reading (R.snap, R.oneOf,
-- R.roughlySame, R.macroTabCount, R.cleanOscName); a name is never said folded.
local CONFUSABLE = {
  -- Cyrillic
  ["а"] = "a", ["е"] = "e", ["о"] = "o", ["р"] = "p", ["с"] = "c", ["у"] = "y", ["х"] = "x",
  ["і"] = "i", ["ј"] = "j", ["ѕ"] = "s", ["һ"] = "h", ["ԁ"] = "d", ["ӏ"] = "l",
  ["А"] = "A", ["В"] = "B", ["Е"] = "E", ["К"] = "K", ["М"] = "M", ["Н"] = "H", ["О"] = "O", ["Р"] = "P",
  ["С"] = "C", ["Т"] = "T", ["Х"] = "X", ["У"] = "Y", ["І"] = "I", ["Ј"] = "J", ["Ѕ"] = "S",
  -- Greek
  ["Α"] = "A", ["Β"] = "B", ["Ε"] = "E", ["Ζ"] = "Z", ["Η"] = "H", ["Ι"] = "I", ["Κ"] = "K", ["Μ"] = "M",
  ["Ν"] = "N", ["Ο"] = "O", ["Ρ"] = "P", ["Τ"] = "T", ["Υ"] = "Y", ["Χ"] = "X",
  ["α"] = "a", ["ι"] = "i", ["ν"] = "v", ["ο"] = "o", ["ρ"] = "p", ["υ"] = "u",
}

-- `s` with those letters as the Latin ones; a string with none comes back as it is.
function R.foldConfusables(s)
  s = tostring(s or "")
  if not string.find(s, "[\205-\212]") then return s end
  return (string.gsub(s, "[\192-\255][\128-\191]*", function(ch) return CONFUSABLE[ch] end))
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
  local x, y = string.lower(R.foldConfusables(R.tidy(a))), string.lower(R.foldConfusables(R.tidy(b)))
  if x == "" or y == "" then return false end
  if x == y then return true end
  return R.ratio(x, y) >= R.SAME_RATIO
end

-- A lone bullet-like word a recogniser reads before a name: "o", "O", "°", "•", "·" or "*" and a
-- space, which the second Windows session (2026-10-31) read before every name, preset and macro
-- alike ('o VO Hey Mr Robo', 'o Flanger') — as the project drops "•", "·" and "*" from an OSC tab's
-- name (its _clean_osc_ocr_name; R.cleanOscName). Not "0": a user's own preset may begin with one
-- ("0 dB Lead"). None of the catalog's 18,446 preset names, and no macro or OSC name of it, begins
-- with one of these and a space (avw2-verified, R6), so only a user's own name "o …" loses it.
local LONE_BULLETS = { ["o"] = true, ["O"] = true, ["°"] = true, ["•"] = true, ["·"] = true, ["*"] = true }

-- `text` (tidied) without such a word before it, and the word dropped, or nil where there was none.
function R.withoutBullet(text)
  local t = R.tidy(text)
  local first, rest = string.match(t, "^(%S+) (.+)$")
  if first and LONE_BULLETS[first] then return rest, first end
  return t, nil
end

-- The texts R.presetName has dropped a bullet word from, so that the log says it once per text:
-- one line a reading would be one line every tenth of a second while a watch is out. At most 64.
local bulletsSaid, bulletsCount = {}, 0

-- `text` as a preset's name: "Init Preset" for a reading of it (R.isInitPreset), and whether it
-- was taken so; any other text as it is — without a lone bullet word before it (R.withoutBullet),
-- the log saying so once per text, so that the next session's log says whether the factor's
-- correction made it unnecessary.
function R.presetName(text)
  if R.isInitPreset(text) then return R.INIT_PRESET, true end
  local name, dropped = R.withoutBullet(text)
  if not dropped then return text, false end
  local t = tostring(text)
  if not bulletsSaid[t] then
    if bulletsCount >= 64 then bulletsSaid, bulletsCount = {}, 0 end
    bulletsSaid[t], bulletsCount = true, bulletsCount + 1
    host.log.info(string.format("[avenger] the name read '%s' begins with a lone '%s' — taken as '%s'", R.forLog(t),
      dropped, name))
  end
  if R.isInitPreset(name) then return R.INIT_PRESET, true end
  return name, false
end

-- Whether every word of reading `res` (host.ocr.recognize's) is one the recogniser only guessed
-- (`approx`: shared out by the text's length, docs/api/ocr.md) — on a Mac PaddleOCR's text alone,
-- where Vision read nothing: every one of the 28 such readings in this module's regions in the
-- developer's Mac test of 2026-10-31 was an invention ("PY" for a preset's name). False for a
-- reading with no words, and for one with any word located.
function R.guessedOnly(res)
  local words = type(res) == "table" and res.words
  if type(words) ~= "table" or #words == 0 then return false end
  for _, w in ipairs(words) do
    if not w.approx then return false end
  end
  return true
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
  local compact = string.upper((string.gsub(R.foldConfusables(clean), "[^A-Za-z0-9|Il]", "")))
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
-- (macro_collector.py), except that a reading with nothing like "macro" is no count — the project
-- answered 1 for it — and that a word of five letters that is "macro" with ONE letter misread, a
-- zero for an O no misread at all, counts as it: the second Windows session (2026-10-31) read the
-- caption "UACRO 1" in all four of its reads, and the count, the Macro tab and the database's names
-- were gone with it (avw2-verified, A2). The database's count comes first where it has the preset
-- (read/macros.luau); this is the count where it has none.
local function macroLike(word)
  if #word ~= 5 then return false end
  local off = 0
  for i = 1, 5 do
    if string.sub(word, i, i) ~= string.sub("macro", i, i) then off += 1 end
  end
  return off <= 1
end

function R.macroTabCount(text)
  local norm = string.gsub(string.lower(R.foldConfusables(R.tidy(text))), "0", "o")
  local hi, seen = 0, false
  -- Each word of letters, and the digit after it, alone or glued on ("macro2").
  for word, n in string.gmatch(norm, "(%a+)%s*(%d?)") do
    if macroLike(word) then
      seen = true
      local k = tonumber(n)
      if k and k >= 1 and k <= 4 then hi = math.max(hi, k) end
    end
  end
  if hi > 0 then return hi end
  if seen then return 1 end
  return nil
end

-- Avenger's own name for macro field `i` (1 to 3 the knobs, 4 and 5 the buttons) of macro tab
-- `tab`, the one it shows where the sound designer named none: the project's _current_macro_label
-- (actions.py) — "Macro (tab − 1) × 3 + i" for a knob, "MacroBtn (tab − 1) × 2 + i − 3" for a
-- button. The names a read is compared with where the preset database has none (read/macros.luau):
-- Init Preset's, and a user's own preset's that kept them.
function R.defaultMacroName(tab, i)
  if i <= 3 then return string.format("Macro %d", (tab - 1) * 3 + i) end
  return string.format("MacroBtn %d", (tab - 1) * 2 + i - 3)
end

-- How alike a name read has to be to the database's to be taken for it: the preset database's
-- (presets.luau, THRESHOLD), the project's DEFAULT_MATCH_THRESHOLD.
R.SNAP_RATIO = 0.72

-- A name read compared with the name the preset database has in its place (`expected`, or nil):
-- the database's spelling when the two are the same words, case and spacing aside; the database's
-- name, and `true`, when they are R.SNAP_RATIO alike or more; otherwise the name as read. The flag
-- and the ratio are for the log: the name is shown as it comes back, unmarked (src/presets.luau,
-- WHAT IT MAY DO).
function R.snap(read, expected)
  if not expected or expected == "" then return read, false, nil end
  local a, b = string.lower(R.foldConfusables(R.tidy(read))), string.lower(R.foldConfusables(R.tidy(expected)))
  if a == b then return R.tidy(expected), false, 1 end
  local r = R.ratio(a, b)
  if r >= R.SNAP_RATIO then return R.tidy(expected), true, r end
  return read, false, r
end

-- Which of `words` (key -> the words a field shows) reading `read` is, case and spacing aside: the
-- key of the most alike, when it is R.SNAP_RATIO alike or more — a menu's field ("1l8" is 1/8's), a
-- speed. The key and the ratio; nil and the best ratio for the log otherwise.
function R.oneOf(read, words)
  local a = string.lower((string.gsub(R.foldConfusables(R.tidy(read)), " ", "")))
  if a == "" then return nil, 0 end
  local best, ratio = nil, 0
  for key, w in pairs(words) do
    local r = R.ratio(a, string.lower((string.gsub(w, " ", ""))))
    if r > ratio or (r == ratio and best ~= nil and key < best) then best, ratio = key, r end
  end
  if best and ratio >= R.SNAP_RATIO then return best, ratio end
  return nil, ratio
end

-- A note value read — "1/8", "1/16T" — as a key of `words` ("1_8", "1_16t"): its slash read as
-- l, I, | or nothing, its T in either case; else R.oneOf's. The key and how it was taken, or nil.
function R.noteValue(read, words)
  local s = string.gsub(R.tidy(read), " ", "")
  local digits, t = string.match(s, "^1[/lI|\\]?(%d+)([tT]?)$")
  if digits then
    local key = "1_" .. digits .. string.lower(t)
    if words[key] then return key, 1 end
  end
  return R.oneOf(read, words)
end

return R
