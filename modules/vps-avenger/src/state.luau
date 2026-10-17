-- What the page is told about Avenger: per plug-in window, the values READ off it, and the state
-- the tool's page reads (ui/panel.js, applyState) built from them under the tool's own keys — the
-- port of its state_snapshot (the avenger_control project's dispatcher.py).
--
-- ONLY WHAT WAS READ. The tool kept 56 values it assumed until something said otherwise — one
-- oscillator, macro tab 1, every switch off — and its page showed them as if they were Avenger's.
-- Here a value nobody has read is not there: it is left out of the state, or a hole in a list,
-- and the page says it as "not read". A state goes whole, so a key that is missing from it was not
-- read. Values that come from the preset database rather than the screen are marked as such: the
-- expansions list (`expansions_from_database`), the expansion the preset is in
-- (`expansion_from_database`), an open expansion's categories and presets
-- (`expansion_lists_from_database`), and a name read that the database's spelling replaced
-- (`names_from_database`, `labels_from_database`).
--
-- The values are read/open.luau's, all of them at the page key: the preset's name and the zoom,
-- the browser's view, an open expansion and FAV; the oscillator strip — the count, the names, the
-- tab selected, Mute and Solo, on the Drums tab its switches and mode, and the selected
-- oscillator's knobs; the central tab with the Drums tab, and DRUM SQ on it; the macro tabs, the
-- tab shown, its names, buttons and knobs. What the page shows and nothing reads yet — the drum
-- slots, the sample editor, DRUM SQ's length — is left out, and the page says so.

local S = {}

local R = host.include("src/read/ocr.luau")

-- One table per plug-in window, by its id: { values = { key -> value }, at = when they were read
-- (host.now), facts = { key -> value } }. At most WINDOWS_MAX, the oldest dropped: one window is in
-- front at a time. `facts` are what an action of this session did that no read can show — the OSC
-- tab Copy was chosen for (`oscCopied`, its name as read, or "OSC <i>"), which the page's Paste
-- follows: Avenger's own clipboard cannot be read. A read of the whole of Avenger keeps them.
local WINDOWS_MAX = 8
local byWindow, order = {}, {}

-- Why the page's ◀ and ▶ cannot step: VPS Avenger steps no preset from its blank preset, which
-- the tool did not know (main.luau, "◀ and ▶ on Init Preset"). The state says it
-- (`preset_steps_unavailable`, a key of the port's) where the preset read is Init Preset, and the
-- page's buttons are then aria-disabled with it in their help (ui/panel.js); a press is answered
-- with it in the error region (actions.luau).
S.AT_INIT = "VPS Avenger steps no preset from Init Preset; load a preset first"

local function stepsWhy(v)
  if v.preset ~= nil and R.isInitPreset(v.preset) then return S.AT_INIT end
  return nil
end

local function forWindow(id)
  local st = byWindow[id]
  if st then return st end
  st = { values = {}, at = nil, facts = {} }
  byWindow[id] = st
  order[#order + 1] = id
  if #order > WINDOWS_MAX then byWindow[table.remove(order, 1)] = nil end
  return st
end

-- Window `id`'s values as read just now: `values` replaces what was read there before, keys
-- absent from it included — a value a read did not find is not kept from an earlier read.
function S.read(id, values)
  local st = forWindow(id)
  st.values = values
  st.at = host.now()
end

-- What a trip read again in window `id` — the preset's name after a step, the zoom after a choice
-- of size — over what was read before: `fields` replace those keys alone (a false one clears its
-- key), and the rest stays as it was read, with its time.
function S.set(id, fields)
  local st = forWindow(id)
  for k, v in pairs(fields) do
    if v == false then st.values[k] = nil else st.values[k] = v end
  end
end

-- What an action read again of one part of Avenger in window `id` (read/open.luau, Open.part): each
-- of `keys` replaced by its value in `values`, nil included — what the part's read did not find is
-- not kept from an earlier read — and the rest as it was read, with its time.
function S.part(id, keys, values)
  local st = forWindow(id)
  for _, k in ipairs(keys) do st.values[k] = values[k] end
end

-- A fact of this session in window `id` (above): `value`, or nil to forget it.
function S.note(id, key, value)
  forWindow(id).facts[key] = value
end

-- What was last read in window `id`, as it stands (nil: nothing yet) — for words, never changed
-- through this.
function S.values(id)
  local st = id and byWindow[id] or nil
  return st and st.values
end

-- When window `id`'s values were read (host.now), or nil: nothing read there yet.
function S.at(id)
  local st = id and byWindow[id] or nil
  return st and st.at
end

-- Why ◀ and ▶ cannot step from the preset read in window `id` (S.AT_INIT), or nil: they can, or
-- nothing was read there.
function S.stepsUnavailable(id)
  local st = id and byWindow[id] or nil
  return st and stepsWhy(st.values) or nil
end

-- A list of `n` entries from `src`, holes where nothing was read: the page reads a hole as "not
-- read". nil for no `n`.
local function list(src, n)
  if not n then return nil end
  local out = {}
  for i = 1, n do
    if src then out[i] = src[i] end
  end
  return out
end

-- Whether any entry of the first `n` of `marks` is true.
local function anyOf(marks, n)
  for i = 1, n or 0 do
    if marks and marks[i] then return true end
  end
  return false
end

-- osc_panel: the tool's keys for the strip, the selected oscillator's knobs, the Drums tab, the
-- central tab and DRUM SQ; and `copied`, the OSC tab a Copy of this session was chosen for.
local function oscPanel(v, facts)
  local n = v.oscCount
  local out = {
    count = n,
    copied = facts and facts.oscCopied or nil,
    names = list(v.oscNames, n),
    mute_states = list(v.oscMute, n),
    solo_states = list(v.oscSolo, n),
    selected_type = v.selectedType,
    selected_index = v.selectedType == "osc" and v.selectedIndex or nil,
    drums_mute = v.drumsMute,
    drums_solo = v.drumsSolo,
    trig_mute = v.trigMute,
    trig_solo = v.trigSolo,
    drum_pattern = v.drumPattern,
    drum_lock = v.drumLock,
    drum_mode = v.drumMode,
    central_panel_tab = v.central,
  }
  if n and anyOf(v.oscSnapped, n) then out.names_from_database = list(v.oscSnapped, n) end
  if v.selectedType == "drums" then
    out.selected_label = "Drums"
  elseif v.selectedType == "trig" then
    out.selected_label = "Trig"
  elseif v.selectedType == "osc" and v.oscNames then
    out.selected_label = v.oscNames[v.selectedIndex]
  end
  -- The knobs show the selected oscillator's controls only: its place in each list, the others
  -- holes.
  if v.osc and n and v.selectedIndex and v.selectedIndex <= n then
    local i = v.selectedIndex
    local function at(x)
      local t = {}
      t[i] = x
      return t
    end
    out.volume_values = at(v.osc.volume)
    out.pan_values = at(v.osc.pan)
    out.transpose_values = at(v.osc.transpose)
    out.fine_tune_values = at(v.osc.fineTune)
    out.random_pitch_values = at(v.osc.randomPitch)
  end
  local sq = v.drumSq
  if sq then
    out.drum_sq_auto_follow = sq.follow
    out.drum_sq_pattern_chain = sq.chain
    out.drum_sq_pattern_selected = sq.selected
    out.drum_sq_pattern_states = list(sq.states, 3)
  end
  return out
end

-- macro_panel: the tool's keys for the macros. Only the tab shown is read; the others' names are
-- holes.
local function macroPanel(v)
  local sel = v.macroSelected
  local out = {
    count = v.macroCount,
    selected_index = sel,
    button_states = list(v.macroButtons, 2),
    amount_values = list(v.macroAmounts, 3),
  }
  if sel then
    local labels = list(v.macroLabels, 5)
    out.tabs = {}
    out.tabs[sel] = labels
    out.selected_labels = labels
    if anyOf(v.macroSnapped, 5) then out.labels_from_database = list(v.macroSnapped, 5) end
  end
  return out
end

-- The state the page reads, for window `id` (nil: no window yet, nothing read). `roster` is the
-- database's list of expansions, or nil. `opts.age`: the state is answered from earlier reads
-- (the page reloaded itself), so it says how many seconds ago they were made.
function S.snapshot(id, roster, opts)
  local st = id and byWindow[id] or nil
  local v = st and st.values or {}
  local out = {
    preset = v.preset,
    preset_steps_unavailable = stepsWhy(v),
    zoom = v.zoom,
    expansion_view = v.expansionView,
    expansion_open = v.expansionOpen,
    selected_expansion = v.selectedExpansion,
    expansion_favorite_view = v.favoriteView,
    osc_panel = oscPanel(v, st and st.facts),
    macro_panel = macroPanel(v),
  }
  if v.expansion then
    out.expansion = v.expansion
    out.expansion_from_database = true
  end
  if v.lists then
    out.expansion_categories = v.lists.categories
    out.expansion_presets = v.lists.presets
    out.expansion_preset_categories = v.lists.presetCategories
    out.expansion_lists_from_database = true
    out.loaded_expansion_preset_index = v.loadedIndex
  end
  if roster then
    out.expansions = roster
    out.expansions_from_database = true
  end
  if opts and opts.age and st and st.at then
    out.read_age_seconds = math.floor((host.now() - st.at) / 1000)
  end
  return out
end

return S
