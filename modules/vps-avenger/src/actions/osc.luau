-- The oscillator strip's actions (the plan's slice 5), for the page's trips and the ring alike: each
-- a spec for src/act.luau — what is read before the click, where the click goes, what is watched,
-- and what the read after it says. By the avenger_control tool's names and arguments (its
-- actions.py), with its points (its clicker and header_map.py, src/tables), and its defects fixed:
-- nothing is assumed, and every answer is what was read.
--
--   select_osc_tab        the tab, placed by the count the dots read; the selection band read
--   add_osc               "Add OSC", none at eight; the count and the names read
--   select_drums_tab, select_trig_tab   the tab; the selection, Drums' switches, the central tab
--                         and DRUM SQ read
--   toggle_osc_mute/_solo, toggle_drums_mute/_solo, toggle_trig_mute/_solo   the LED; its share
--                         of red or brightness read
--   osc_menu_item         the tab's menu with the right button, its item at the tool's row: Copy
--                         (noted for Paste: Avenger's clipboard cannot be read), Paste,
--                         Initialize, Delete (none with one oscillator); the names, the dots and
--                         the knobs read, and the last three noted as an edit of the tabs, whose
--                         database names then name none (state.luau, S.tabsEdited)
--   drums_menu_item, trig_menu_item   Initialize in the tab's menu; the strip read
--   reset_osc_control     a double-click on the selected oscillator's knob — with the right
--                         button for Pan and Random Pitch, as the tool does — never while Drums or
--                         Trig is selected (the tool clicked there all the same); the knob read,
--                         and said against the tool's default (Act.resetAnswer)
--
-- Each builder is `(args, label, known) -> spec | nil, why`: `args` the page's (or the ring's
-- own), `label` the action's words, `known` the values last read in this window (state.luau's,
-- for words only: a name, never a decision). nil refuses before anything is read.

local T = host.include("src/tables/strip.luau")
local OSC = host.include("src/tables/osc.luau")
local Strip = host.include("src/read/strip.luau")
local Open = host.include("src/read/open.luau")
local Knob = host.include("src/read/knob.luau")
local Act = host.include("src/act.luau")

local A = {}

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
end

-- A box `size` round table point `p`, as a rectangle.
local function box(p, size)
  return { p[1] - size[1] / 2, p[2] - size[2] / 2, p[1] + size[1] / 2, p[2] + size[2] / 2 }
end

-- The tabs' row: every tab's caption, dot and selection band, Drums and Trig.
local TABS_ROW = { T.PANEL_LEFT, T.MUTE_Y - T.BUTTON_STATE[2] / 2, T.TRIG_SELECTION[2], T.SELECTION_BOTTOM }
-- The selected oscillator's knobs.
local KNOBS = Open.PICTURES.knob

-- Whether `a` and `b` are the same value, lists and tables compared entry by entry.
local function same(a, b)
  if a == b then return true end
  if type(a) ~= "table" or type(b) ~= "table" then return false end
  for k, v in pairs(a) do
    if not same(v, b[k]) then return false end
  end
  for k in pairs(b) do
    if a[k] == nil then return false end
  end
  return true
end

-- Whether every one of `keys` reads the same in `vb` and `va`.
local function sameKeys(vb, va, keys)
  if not va then return false end
  for _, k in ipairs(keys) do
    if not same(vb[k], va[k]) then return false end
  end
  return true
end

local STRIP = { "oscCount", "oscMute", "oscSolo", "selectedType", "selectedIndex", "drumsMute", "drumsSolo",
  "trigMute", "trigSolo", "drumPattern", "drumLock", "drumMode" }

local function plural(n, word) return n .. " " .. word .. (n == 1 and "" or "s") end

-- The count the dots read, and `i` within it — or nil and why.
local function counted(vb, i)
  local n = vb.oscCount
  if not n then return nil, "the oscillator tabs cannot be counted now" end
  if i and (i < 1 or i > n) then
    return nil, string.format("OSC %d is not there; VPS Avenger shows %s", i, plural(n, "oscillator"))
  end
  return n
end

-- OSC tab `i` in words, from the values `...` in turn — reads of this moment: an action's read
-- after its click and the one before it, or what the ring read in its stay, never a window's values
-- kept from earlier (state.luau: a step replaces only the name) — as the page names its tabs: the
-- name one of them read off the screen (`oscNames`, in the database's spelling where the read was
-- close to it), else a read's words for the tab (`oscWords`, read/open.luau's wordsFor: a name the
-- window keeps from a read under the same preset's name and count, else the preset database's for
-- the preset shown), else Avenger's own "OSC i". The ring reads no text of the strip on a step
-- (ring.luau), so there the names kept and the database's are what the avenger_control developer
-- asked for (his report of 2026-10-26: the Oscillator stepper said only "OSC 1", "OSC 2" where the
-- page shows the names). Said as it is, with no "from the database": the maintainer's decision of
-- 2026-10-26, first for these names and the same day for everything the database gives — that is
-- what the database is there for — in place of the mark of 2026-09-28 (src/presets.luau).
local function oscName(i, ...)
  for k = 1, select("#", ...) do
    local v = select(k, ...)
    local n = v and v.oscNames and v.oscNames[i]
    if n and n ~= "" then return n end
  end
  for k = 1, select("#", ...) do
    local v = select(k, ...)
    local n = v and v.oscWords and v.oscWords[i]
    if n and n ~= "" then return n end
  end
  return "OSC " .. i
end

-- The tab the strip reads as selected in `v`, in words (an OSC tab by oscName, from `v` and then
-- `...`), or nil.
local function tabWords(v, ...)
  if not v then return nil end
  if v.selectedType == "osc" and v.selectedIndex then return oscName(v.selectedIndex, v, ...) end
  if v.selectedType == "drums" then return "Drums" end
  if v.selectedType == "trig" then return "Trig" end
  return nil
end

local function sameSelection(vb, va)
  return va.selectedType ~= nil and va.selectedType == vb.selectedType and va.selectedIndex == vb.selectedIndex
end

-- What a choice of tab says: the tab read as selected after it.
local function selectedAnswer(vb, va, how)
  local tab = tabWords(va, vb)
  if not tab then return { error = "the selected tab cannot be read now" } end
  if how == "unchanged" then return { note = "unchanged, " .. tab .. " is selected", said = tab .. " is selected, unchanged" } end
  return { said = tab .. " selected" }
end

function A.select_osc_tab(args, label)
  local i = whole(args.index)
  if not i then return nil, "no oscillator tab was named" end
  return {
    label = label,
    part = "tab",
    decide = function(vb)
      local n, why = counted(vb, i)
      if not n then return nil, why end
      return { at = { T.tabX(i, n), T.TAB_Y } }
    end,
    watch = function() return { TABS_ROW, KNOBS } end,
    same = sameSelection,
    answer = selectedAnswer,
  }
end

-- Drums or Trig, at its tab's point; with `names`, the part's text read after the click — the Drums
-- tab's slots' names, which the page shows (the ring leaves them off: ring.luau).
local function fixedTab(x, names)
  return function(_, label)
    return {
      label = label,
      part = "tab",
      textAfter = names,
      decide = function() return { at = { x, T.TAB_Y } } end,
      watch = function() return { TABS_ROW, KNOBS } end,
      same = sameSelection,
      answer = selectedAnswer,
    }
  end
end
A.select_drums_tab = fixedTab(T.DRUMS_TAB_X, true)
A.select_trig_tab = fixedTab(T.TRIG_TAB_X)

function A.add_osc(_, label)
  return {
    label = label,
    part = "tab",
    textAfter = true,
    decide = function(vb)
      local n, why = counted(vb)
      if not n then return nil, why end
      -- The tool's focus at the limit: the eighth tab.
      if n >= 8 then return nil, "VPS Avenger has 8 oscillators, the most it takes", "osc-tab-8" end
      return { at = { T.ADD_X, T.TAB_Y } }
    end,
    watch = function() return { TABS_ROW } end,
    same = function(vb, va) return va.oscCount ~= nil and va.oscCount == vb.oscCount end,
    answer = function(_, va, how)
      local n = va and va.oscCount
      if not n then return { error = "the oscillator tabs cannot be counted now" } end
      if how == "unchanged" then return { note = "unchanged, " .. plural(n, "oscillator"), said = plural(n, "oscillator") .. ", unchanged" } end
      return { said = plural(n, "oscillator"), focus = "osc-tab-" .. n }
    end,
  }
end

-- A Mute or Solo LED: `key` the values' list or switch, `title` its word, `point(vb)` where it is
-- (nil and why where it cannot be placed), `index` an OSC tab's, said by its name (oscName, from
-- the reads after the click and before it), `kind` the tab's ("osc", "drums", "trig"). A Mute read
-- while another tab's Solo is on is said as silenced by it (Strip.silencedBy), its tabs named so.
local function toggle(label, key, index, title, point, kind)
  local function valueOf(v)
    if not v then return nil end
    if index then return v[key] and v[key][index] end
    return v[key]
  end
  return {
    label = label,
    part = "strip",
    decide = function(vb)
      local p, why = point(vb)
      if not p then return nil, why end
      return { at = p }
    end,
    watch = function(_, click) return { box(click.at, T.BUTTON_STATE) } end,
    same = function(vb, va)
      local a = valueOf(va)
      return a ~= nil and a == valueOf(vb)
    end,
    answer = function(vb, va, how)
      local a = valueOf(va)
      local who = index and oscName(index, va, vb)
      local named = (who and (who .. " ") or "") .. title
      if a == nil then return { error = (who and (who .. "'s ") or "") .. title .. " cannot be read now" } end
      local silenced = title == "Mute" and Strip.silencedBy(va, kind, index, function(j)
        return oscName(j, va, vb)
      end)
      local words = named .. (silenced and (", " .. silenced) or (" " .. (a and "ON" or "OFF")))
      if how == "unchanged" then return { note = "unchanged, " .. words, said = words .. ", unchanged" } end
      return { said = words }
    end,
  }
end

local function oscToggle(kind)
  local key, title, dx = kind == "mute" and "oscMute" or "oscSolo", kind == "mute" and "Mute" or "Solo",
    kind == "mute" and T.MUTE_X_OFFSET or T.SOLO_X_OFFSET
  return function(args, label)
    local i = whole(args.index)
    if not i then return nil, "no oscillator tab was named" end
    return toggle(label, key, i, title, function(vb)
      local n, why = counted(vb, i)
      if not n then return nil, why end
      return { T.tabX(i, n) + dx, T.MUTE_Y }
    end, "osc")
  end
end
A.toggle_osc_mute = oscToggle("mute")
A.toggle_osc_solo = oscToggle("solo")

local function fixedToggle(key, title, p, kind)
  return function(_, label)
    return toggle(label, key, nil, title, function() return p end, kind)
  end
end
A.toggle_drums_mute = fixedToggle("drumsMute", "Mute", T.DRUMS_MUTE, "drums")
A.toggle_drums_solo = fixedToggle("drumsSolo", "Solo", T.DRUMS_SOLO, "drums")
A.toggle_trig_mute = fixedToggle("trigMute", "Mute", T.TRIG_MUTE, "trig")
A.toggle_trig_solo = fixedToggle("trigSolo", "Solo", T.TRIG_SOLO, "trig")

-- What an OSC tab's menu changes: its names, the dots, the switches and the selected tab's knobs —
-- the names read before the click as after it (`text`), so that a Paste or an Initialize that
-- changes nothing VPS Avenger shows can be said as that.
local MENU_KEYS = { "oscCount", "oscNames", "oscMute", "oscSolo", "selectedType", "selectedIndex", "osc" }

function A.osc_menu_item(args, label)
  local i, item = whole(args.index), args.item
  if not i then return nil, "no oscillator tab was named" end
  if not T.MENU_ITEM_Y[item] then return nil, tostring(item) .. " is no item of an oscillator's menu" end
  return {
    label = label,
    part = "tab",
    -- Copy keeps the name it copied, read before; Paste and Initialize compare the names before
    -- with the names after; Delete counts the dots.
    text = item ~= "Delete",
    textAfter = item ~= "Copy",
    decide = function(vb)
      local n, why = counted(vb, i)
      if not n then return nil, why end
      if item == "Delete" and n == 1 then return nil, "VPS Avenger deletes no oscillator when it has one" end
      local x = T.menuX(i, n)
      return { at = { x, T.TAB_Y }, menu = { item = { x, T.MENU_ITEM_Y[item] }, retries = 3, read = item ~= "Copy" } }
    end,
    watch = function() return { TABS_ROW, KNOBS } end,
    same = function(vb, va)
      if item == "Delete" then return va.oscCount ~= nil and va.oscCount == vb.oscCount end
      return sameKeys(vb, va, MENU_KEYS)
    end,
    answer = function(vb, va, how)
      if item == "Copy" then
        local name = oscName(i, vb)
        return { facts = { oscCopied = name }, said = "Copy chosen for " .. name }
      end
      -- Paste, Initialize and Delete, once chosen, edit the preset's tabs — whatever the read after
      -- shows: the database's names of them name no tab of this preset from now on
      -- (state.luau, S.tabsEdited), by the name the read before was taken for.
      local facts = { oscEdited = { preset = vb.oscWordsOf } }
      if item == "Delete" then
        local n = va and va.oscCount
        if not n then return { facts = facts, error = "Delete chosen; the oscillator tabs cannot be counted now" } end
        if how == "unchanged" then return { facts = facts, note = "unchanged, " .. plural(n, "oscillator") } end
        return { facts = facts, focus = "osc-menu-" .. math.min(n, i), said = plural(n, "oscillator") }
      end
      if how == "unchanged" then
        return { facts = facts, note = item .. " chosen; nothing VPS Avenger shows changed" }
      end
      return { facts = facts, said = item .. " chosen" }
    end,
  }
end

-- Initialize in the Drums or the Trig tab's menu.
local function tabMenu(x, what)
  return function(args, label)
    local item = args.item or "Initialize"
    if item ~= "Initialize" then return nil, tostring(item) .. " is no item of the " .. what .. " tab's menu" end
    return {
      label = label,
      part = "tab",
      decide = function() return { at = { x, T.TAB_Y }, menu = { item = { x, T.DRUMS_TRIG_MENU_Y }, retries = 3 } } end,
      same = function(vb, va)
        return sameKeys(vb, va, STRIP) and sameKeys(vb, va, { "central", "drumSq" })
      end,
      answer = function(_, _, how)
        if how == "unchanged" then return { note = "Initialize chosen; nothing VPS Avenger shows changed" } end
        return { said = "Initialize chosen" }
      end,
    }
  end
end
A.drums_menu_item = tabMenu(T.DRUMS_TAB_X, "Drums")
A.trig_menu_item = tabMenu(T.TRIG_TAB_X, "Trig")

-- The selected oscillator's controls a double-click resets, by the page's names: the knob, the
-- reader's value, its words, and the value's words alone (`value`, for a reset's answer). Pan is
-- Level's knob and Random Pitch Fine Tune's, with the right button (OSC.RIGHT_BUTTON).
local function percent(v) return v .. " percent" end
local function semitones(v) return v .. " semitones" end
local function cents(v) return v .. " cents" end
local CONTROLS = {
  volume = { field = "volume", value = percent, words = function(v) return "Volume " .. percent(v) end },
  pan = { field = "pan", value = tostring, words = function(v) return "Pan " .. v end },
  transpose = { field = "transpose", value = semitones, words = function(v) return "Transpose " .. semitones(v) end },
  fine_tune = { field = "fineTune", value = cents, words = function(v) return "Fine Tune " .. cents(v) end },
  random_pitch = { field = "randomPitch", value = tostring, words = function(v) return "Random Pitch " .. v end },
}
A.CONTROLS = CONTROLS
A.CONTROL_NAMES = { volume = "Volume", pan = "Pan", transpose = "Transpose", fine_tune = "Fine Tune",
  random_pitch = "Random Pitch" }

-- A reset: the double-click, then the knob read and said against the control's default (the tool's,
-- OSC.DEFAULTS; Act.resetAnswer): reset, already at its default, not its default, unchanged, or not
-- read.
function A.reset_osc_control(args, label)
  local key = args.control
  local c, p = CONTROLS[key], OSC.CONTROLS[key]
  if not (c and p) then return nil, tostring(key) .. " is no control of an oscillator's" end
  local function valueOf(v) return v and v.osc and v.osc[c.field] end
  return {
    label = label,
    part = "knobs",
    decide = function(vb)
      if vb.selectedType == "drums" then return nil, "the Drums tab is selected; it has no oscillator controls" end
      if vb.selectedType == "trig" then return nil, "the Trig tab is selected; it has no oscillator controls" end
      if vb.selectedType ~= "osc" then return nil, "the selected tab cannot be read now" end
      return { at = p, double = true, button = OSC.RIGHT_BUTTON[key] and "right" or nil }
    end,
    -- The knob's picture (Knob.BOX): the reader looks within 16 of the project's pixels (16 points
    -- at 50 %).
    watch = function() return { box(p, Knob.BOX.osc) } end,
    same = function(vb, va)
      local a = valueOf(va)
      return a ~= nil and a == valueOf(vb)
    end,
    answer = function(vb, va)
      return Act.resetAnswer(label, A.CONTROL_NAMES[key], c.value, valueOf(vb), valueOf(va), OSC.DEFAULTS[key],
        "osc-reset-" .. string.gsub(key, "_", "-"))
    end,
  }
end

-- The tab read as selected, in words (the ring's Oscillator stepper says it), and an OSC tab's name
-- (its Mute and Solo, the knobs' sentences: ring.luau, src/amount.luau), from reads of this moment.
A.tabWords = tabWords
A.oscName = oscName
-- The count the dots read, and an index within it (the OSC tab's rename, src/handover.luau).
A.counted = counted

return A
