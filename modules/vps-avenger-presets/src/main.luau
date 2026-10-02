-- VPS Avenger's preset database: the reader of this module's data.
--
-- The data is the catalog of VPS Avenger's expansions, the commercial ones as the catalog lists
-- them — for every expansion its categories and presets, for every preset its oscillator tabs'
-- names and its macro tabs' names —
-- written by tools/avenger-presets/convert.py from the avenger_control project's database, whose
-- developer gave it without restriction (NOTICE). README.md describes the files.
--
-- WHY A CODE MODULE. The module that uses the data (com.platform.vps-avenger, as an optional
-- dependency) cannot read these files itself: a relative path in ITS code resolves under its own
-- folder, and where this folder is — beside it, or in a modules/ folder beside that — is the
-- host's business (docs/module-package-format.md). Evaluated in the dependent's VM, this file
-- keeps this module's identity, so host.resource reads from HERE wherever it runs
-- (docs/api/require.md), and the one piece of code that knows the format is the one that ships
-- with the data. A plain data export (host.require of a module that returns tables) would copy
-- all of it into a fresh table on every call, and read all of it at every start.
--
-- WHEN IT READS. Never at load: this file also runs in this module's own VM, where nobody asks
-- it anything. The index is read at the first M.index() and kept; an expansion's file at the
-- first M.expansion() that names it, and kept — at most all 124, 2.3 MB of text. Each read is
-- one host.resource.read and one host.json.decode on the caller's thread, the main one: the
-- index is 0.3 MB, an expansion's file 5 to 110 KB. A file that cannot be read or does not
-- decode is not read again until the module using it is reloaded (all of this state lives in
-- that module's VM), and every call after says why.

local M = {}

-- The format this reader knows. convert.py's FORMAT; a file of another format is refused.
M.FORMAT = 1

local index, indexWhy = nil, nil
local entries = nil -- expansion name -> its entry in the index
local files = {} -- expansion name -> its decoded file, or { why = "…" }

local function readJson(rel)
  local ok, text = pcall(host.resource.read, rel)
  if not ok then return nil, string.format("%s cannot be read: %s", rel, tostring(text)) end
  local fine, v = pcall(host.json.decode, text)
  if not fine then return nil, string.format("%s is not JSON: %s", rel, tostring(v)) end
  if type(v) ~= "table" then return nil, rel .. " holds no object" end
  if v.format ~= M.FORMAT then
    return nil, string.format("%s is format %s, and this reader reads format %d", rel, tostring(v.format), M.FORMAT)
  end
  return v
end

-- The index: { format, exported, factoryStandard = { name, … }, expansions = { { name, file,
-- presets = { name, … } }, … } }, the expansions in the catalog's order and each one's presets
-- in its own list's order — the order of its category blocks, which is not alphabetical. The
-- live table: a caller that changes it changes it for every later call. nil and why when it
-- cannot be read.
function M.index()
  if index then return index end
  if indexWhy then return nil, indexWhy end
  local v, why = readJson("data/index.json")
  if v and type(v.expansions) ~= "table" then v, why = nil, "data/index.json lists no expansions" end
  if not v then
    indexWhy = why
    return nil, why
  end
  local byName = {}
  for _, e in ipairs(v.expansions) do
    if type(e) ~= "table" or type(e.name) ~= "string" or type(e.file) ~= "string" or type(e.presets) ~= "table" then
      indexWhy = "data/index.json has an expansion without a name, a file or presets"
      return nil, indexWhy
    end
    byName[e.name] = e
  end
  index, entries = v, byName
  return v
end

-- Expansion `name`'s file: { format, name, categories = { { name, start, count }, … }, presets =
-- { { name, osc = { … }, macros = { { … }, … } }, … } } — presets in the index's order, so the
-- i-th of one is the i-th of the other. nil and why for a name the index does not have, or a
-- file that cannot be read.
function M.expansion(name)
  local kept = files[name]
  if kept then
    if kept.why then return nil, kept.why end
    return kept
  end
  local _, why = M.index()
  if not entries then return nil, why end
  local e = entries[name]
  if not e then return nil, string.format("the index has no expansion '%s'", tostring(name)) end
  local v, bad = readJson(e.file)
  if v and (type(v.presets) ~= "table" or #v.presets ~= #e.presets or type(v.categories) ~= "table") then
    v, bad = nil, string.format("%s does not hold the %d presets the index lists", e.file, #e.presets)
  end
  files[name] = v or { why = bad }
  return v, bad
end

-- The category of preset `i` of expansion file `x`: its name, the preset's place in it (1-based)
-- and how many it holds; nil when no category's block holds `i`.
function M.category(x, i)
  for _, c in ipairs(x.categories) do
    if c.count > 0 and i >= c.start and i < c.start + c.count then
      return c.name, i - c.start + 1, c.count
    end
  end
  return nil
end

-- The oscillator tabs of preset `i` of `x`, in order: { { name = "Bass", default = false }, … }.
-- A tab the sound designer did not name — "" in the file — comes back with Avenger's own caption
-- for it, "OSC <n>", and `default = true`.
function M.oscillators(x, i)
  local out = {}
  local p = x.presets[i]
  for n, name in ipairs(p and p.osc or {}) do
    if name == "" then
      out[n] = { name = "OSC " .. n, default = true }
    else
      out[n] = { name = name, default = false }
    end
  end
  return out
end

-- The macro tabs of preset `i` of `x`: { { knobs = { three }, buttons = { two } }, … }, each name
-- { name, default } as above. Avenger's own names count across the tabs: knob k of tab t is
-- "Macro <3(t-1)+k>", button b "MacroBtn <2(t-1)+b>".
function M.macros(x, i)
  local out = {}
  local p = x.presets[i]
  for t, tab in ipairs(p and p.macros or {}) do
    local knobs, buttons = {}, {}
    for slot = 1, 5 do
      local name = tab[slot] or ""
      local default
      if slot <= 3 then
        default = "Macro " .. ((t - 1) * 3 + slot)
      else
        default = "MacroBtn " .. ((t - 1) * 2 + slot - 3)
      end
      local item = name == "" and { name = default, default = true } or { name = name, default = false }
      if slot <= 3 then knobs[#knobs + 1] = item else buttons[#buttons + 1] = item end
    end
    out[t] = { knobs = knobs, buttons = buttons }
  end
  return out
end

return M
