-- The ring's oscillator strip and macros (the plan's slice 5, §2.5): controls after Zoom, each
-- saying the latest read of its part of Avenger in this stay, and acting through the same specs as
-- the page's trips (src/actions/osc.luau, src/actions/macros.luau, src/act.luau):
--
--   Oscillator            a stepper over OSC 1 to n, Drums and Trig: the tab read as selected
--   Mute, Solo            the selected tab's, on or off as read; Return clicks it
--   Add oscillator        Return adds one, none at eight
--   the Drums tab's group (the plan's slice 8), shown only once the strip read in this stay reads the
--   Drums tab selected, through the specs of src/actions/drums.luau:
--     Drum pattern        on or off as read; Return clicks it
--     Drum mode           a stepper over DRUMKIT and SEQUENCE: the mode read
--     Drum lock           locked or unlocked as read; Return clicks LOCK
--     Drum slot           a stepper over the twelve slots: the slot read as selected, by its
--                         factory name — the ring reads no text of the table (the slots' names are
--                         twelve text regions, too slow on an Intel Mac to read on a Tab)
--     Slot mute, Slot solo   the selected slot's, on or off as read; Return clicks it
--     Central tab         a stepper over EDITOR, ARP, DRUM SQ and STEP SQ: the central tab read
--     and in it DRUM SQ's group (the plan's slice 8, its other half), shown only once a read in this
--     stay reads DRUM SQ as the central tab, through the specs of src/actions/drumsq.luau — the
--     ring reads no text, so not LENGTH or SPEED:
--       Pattern chain, Auto follow   on or off as read; Return clicks it
--       Pattern           a stepper over A and the patterns read on: the pattern read as selected
--       Pattern B, Pattern C, Pattern D   on or off as read; Return turns it on or off
--   Level, Pan, Transpose, Fine tune, Random pitch   the selected oscillator's knobs as read;
--                         Return resets the knob with a double-click (the right button for Pan and
--                         Random Pitch). Turning them is the page's (the plan's slice 7,
--                         src/amount.luau); the ring's is TODO.md's ("The ring's knobs turned")
--   Macro tab             a stepper over the macro tabs: the tab read as shown, of how many
--   Add macro tab, Delete last macro tab
--   Macro 1 to 3          a knob's name and amount as read; Return resets it
--   Macro button 1, 2     a button's name, on or off as read; Return clicks it
--
-- A part is read when one of its controls is first announced or pressed in a stay — the overlay's
-- onActivate may not wait for a read (docs/api/overlay.md, "Where a hook runs") — and again after
-- each of the ring's own actions on it, whose read after the click is what the control says next.
-- Forgotten when the overlay leaves the front, where the preset may have changed with nothing read
-- since (main.luau's `unseen`, ◀ and ▶), and at every trip of the page's, which acts on Avenger
-- while the ring is away. One action at a time: a press while one is out sends nothing, and says
-- nothing of its own; the one out says what it read.
--
-- The tabs' menus, the macro knobs' reset modes and the macro buttons' menus are the page's only
-- (the plan's question Q5-2): a stepper over their items would have no value read to say. So are a
-- drum slot's menu for the same reason; the drumkit's Back and Next, which would have nothing short
-- to say after a step, the ring reading no names; and a slot's knobs, Browse and Edit Sample, a
-- trip's hand-overs and the sample editor's. So are DRUM SQ's pattern and sequence menus, SPEED and
-- LENGTH, whose values are text, and its knobs; and the whole sample editor, whose controls lie on
-- four tabs, its menus, fields and browser among them, where a stepper over its tabs would say a tab
-- and offer nothing on it.
--
-- The DRUM SQ watcher (src/watch.luau) reads this stay's values (Ring.values), waits while an action
-- of the ring's is out (Ring.busy), and writes what it read into them (Ring.seen).

local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local Open = host.include("src/read/open.luau")
local Act = host.include("src/act.luau")
local Osc = host.include("src/actions/osc.luau")
local Macros = host.include("src/actions/macros.luau")
local Drums = host.include("src/actions/drums.luau")
local Sq = host.include("src/actions/drumsq.luau")
local D = host.include("src/tables/drums.luau")
local C = host.include("src/tables/centre.luau")

local Ring = {}

local R = nil -- main.luau's ring helpers (Ring.build)

-- What was read in this stay: the values, and which parts. `gen` counts the actions, so that an
-- action that ends after the cache was forgotten writes nothing into the new one. `centralAt`: when
-- the central tab was last read into it (host.now).
local cache = { values = {}, parts = {}, gen = 0 }
local pending = nil -- the gen of the action out, or nil

function Ring.forget()
  cache = { values = {}, parts = {}, gen = cache.gen + 1 }
  pending = nil
end

-- Whether an action of the ring's is out.
function Ring.busy()
  return pending ~= nil
end

-- This stay's values where they hold the central tab, and when it was read; nil otherwise.
function Ring.values()
  if cache.values.central == nil then return nil end
  return cache.values, cache.centralAt
end

local function noteCentral(keys)
  if table.find(keys, "central") then cache.centralAt = host.now() end
end

-- What another read of this module's made of `keys` (the DRUM SQ watcher's), merged as a part's read
-- is, but no part marked read: the controls read their part again when they first need it.
function Ring.seen(keys, values)
  for _, k in ipairs(keys) do cache.values[k] = values[k] end
  noteCentral(keys)
end

local log = function(s) host.log.info("[avenger] [ring] " .. s) end

local function say(words)
  host.speech.output(words, { interrupt = true })
end

-- What part `name`'s read gives, merged: each key it read replaced, as state.luau's S.part does.
local function merge(name, keys, values)
  for _, k in ipairs(keys) do cache.values[k] = values[k] end
  noteCentral(keys)
  cache.parts[name] = true
  -- The tab part reads the strip and the knobs too; the knobs, the drums and the central parts the
  -- strip.
  if name == "tab" then cache.parts.strip, cache.parts.knobs = true, true end
  if name == "knobs" or name == "drums" or name == "central" then cache.parts.strip = true end
  -- The central part reads DRUM SQ as the drumsq part does, and the strip besides.
  if name == "central" then cache.parts.drumsq = true end
end

-- Part `name` read now, unless it was in this stay — in the handler that asks, where the picture
-- and its questions wait. The values, or nil when nothing could be read.
local function ensure(o, name, text)
  if cache.parts[name] and (not text or cache.parts[name .. " text"]) then return cache.values end
  if pending then return nil end
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return nil end
  local pl = P.placer(origin, k, (H.zoom(origin)))
  local region = Open.partRegion(pl, name)
  if not region then return nil end
  local mark = o:here()
  local ok, snap = pcall(host.screen.snapshot, { region = region })
  if not (ok and snap) then
    log("the " .. name .. " could not be pictured: " .. tostring(snap))
    return nil
  end
  local gen = cache.gen
  local okR, v, keys = pcall(Open.part, snap, name, { pl = pl, text = text, preset = R.lastName(),
    macroCount = cache.values.macroCount })
  snap:release()
  if not okR then
    log("the " .. name .. " could not be read: " .. tostring(v))
    return nil
  end
  if gen ~= cache.gen or not o:stillHere(mark) then return nil end
  merge(name, keys, v)
  if text then cache.parts[name .. " text"] = true end
  return cache.values
end

-- Action `spec` from the ring: run, its read merged, and what it read said — unless `quiet` (a
-- stepper, whose own announcement says the value). `label` begins a refusal. Whether done came
-- before Act.run returned with something to say: a refusal, said at once.
local function run(o, label, spec, quiet)
  cache.gen += 1
  local gen = cache.gen
  pending = gen
  local early = true
  local refused = false
  Act.run(o, spec, function(result)
    if pending == gen then pending = nil end
    if not result or gen ~= cache.gen then return end
    if result.part then
      merge(spec.part, result.part.keys, result.part.values)
      if spec.textAfter then cache.parts[spec.part .. " text"] = true end
    end
    local words = result.sentence or (result.error and (label .. ": " .. result.error))
    if early and words then refused = true end
    if quiet and not refused then return end
    words = words or result.said or (result.note and (label .. ": " .. result.note))
    if words then say(words) end
  end)
  early = false
  return refused
end

-- The selected tab's words, and the spec builders that act on it.
local function selectedTab(v)
  if v.selectedType == "osc" and v.selectedIndex then return "osc", v.selectedIndex end
  if v.selectedType == "drums" or v.selectedType == "trig" then return v.selectedType end
  return nil
end

-- A spec, or nil and why said: the builder's refusal.
local function built(label, build, args)
  local spec, why = build(args, label, cache.values)
  if not spec then
    say(label .. ": " .. tostring(why))
    return nil
  end
  return spec
end

local function busy(label)
  if pending then
    log(string.format("'%s': an action of the ring is still out — this press is not sent", label))
    return true
  end
  return false
end

-- ── The oscillator strip ─────────────────────────────────────────────────────

local function oscillatorStepper(ov)
  local ctl
  ctl = ov:addStepper({
    label = "Oscillator",
    settle = 3000,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "strip")
      if not v then return "not read" end
      return Osc.tabWords(v) or "not read"
    end,
    onStep = function(dir, o)
      if busy("Oscillator") then return false end
      local v = ensure(o, "strip")
      local n = v and v.oscCount
      local kind, index = nil, nil
      if v then kind, index = selectedTab(v) end
      if not (n and kind) then
        say(n and "Oscillator: the selected tab cannot be read now" or "Oscillator: the oscillator tabs cannot be counted now")
        return false
      end
      local at = kind == "osc" and index or (kind == "drums" and n + 1 or n + 2)
      local to = at + dir
      if to < 1 or to > n + 2 then
        ctl.settle = 1
        return
      end
      ctl.settle = 3000
      local spec
      if to <= n then
        spec = built("Oscillator", Osc.select_osc_tab, { index = to })
      elseif to == n + 1 then
        spec = built("Oscillator", Osc.select_drums_tab, {})
      else
        spec = built("Oscillator", Osc.select_trig_tab, {})
      end
      if not spec then return false end
      if run(o, "Oscillator", spec, true) then return false end
    end,
  })
end

-- Mute or Solo of the selected tab.
local function switch(ov, title)
  local lower = string.lower(title)
  ov:addCustomButton({
    label = title,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "strip")
      if not v then return "not read" end
      local kind, index = selectedTab(v)
      local on
      if kind == "osc" then
        local list = v[lower == "mute" and "oscMute" or "oscSolo"]
        on = list and list[index]
      elseif kind then
        on = v[kind .. title]
      end
      if on == nil then return "not read" end
      return (kind == "osc" and ("OSC " .. index) or (kind == "drums" and "Drums" or "Trig")) .. ", " .. (on and "on" or "off")
    end,
    onActivate = function(o)
      if busy(title) then return end
      local v = ensure(o, "strip")
      local kind, index = nil, nil
      if v then kind, index = selectedTab(v) end
      if not kind then return say(title .. ": the selected tab cannot be read now") end
      local build = Osc["toggle_" .. kind .. "_" .. lower]
      local spec = built(title, build, { index = index })
      if spec then run(o, title, spec) end
    end,
  })
end

local function addOscillator(ov)
  ov:addCustomButton({
    label = "Add oscillator",
    onActivate = function(o)
      if busy("Add oscillator") then return end
      local spec = built("Add oscillator", Osc.add_osc, {})
      if spec then run(o, "Add oscillator", spec) end
    end,
  })
end

-- A knob of the selected oscillator, its value as read; Return resets it.
local function oscKnob(ov, label, key)
  local field = Osc.CONTROLS[key].field
  ov:addCustomButton({
    label = label,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "knobs")
      if not v then return "not read" end
      if v.selectedType ~= "osc" then
        return v.selectedType and "no oscillator selected" or "not read"
      end
      local a = v.osc and v.osc[field]
      if a == nil then return "not read" end
      return string.sub(Osc.CONTROLS[key].words(a), #Osc.CONTROL_NAMES[key] + 2)
    end,
    onActivate = function(o)
      if busy(label) then return end
      local spec = built(label, Osc.reset_osc_control, { control = key })
      if spec then run(o, label, spec) end
    end,
  })
end

-- ── The Drums tab ────────────────────────────────────────────────────────────

-- A button of the group: `text(v)` its words from the part `part` read, `act(v)` the builder and
-- its arguments, or nil and why — said.
local function drumButton(ov, label, part, text, act)
  ov:addCustomButton({
    label = label,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, part)
      if not v then return "not read" end
      return text(v)
    end,
    onActivate = function(o)
      if busy(label) then return end
      local v = ensure(o, part)
      if not v then return say(label .. ": VPS Avenger cannot be read now") end
      local build, args, why = act(v)
      if not build then return say(label .. ": " .. why) end
      local spec = built(label, build, args)
      if spec then run(o, label, spec) end
    end,
  })
end

-- A stepper of the group over `list`: `at(v)` the place in it read (or nil and why), `words(v)` what
-- it says, `step(to)` the builder and its arguments for entry `to`.
local function drumStepper(ov, label, part, count, at, words, step)
  local ctl
  ctl = ov:addStepper({
    label = label,
    settle = 3000,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, part)
      if not v then return "not read" end
      return words(v)
    end,
    onStep = function(dir, o)
      if busy(label) then return false end
      local v = ensure(o, part)
      local now, why = nil, "VPS Avenger cannot be read now"
      if v then now, why = at(v) end
      if not now then
        say(label .. ": " .. why)
        return false
      end
      local to = now + dir
      if to < 1 or to > count then
        ctl.settle = 1
        return
      end
      ctl.settle = 3000
      local build, args = step(to)
      local spec = built(label, build, args)
      if not spec then return false end
      if run(o, label, spec, true) then return false end
    end,
  })
end

-- ── DRUM SQ ──────────────────────────────────────────────────────────────────

-- On or off as read, or "not read".
local function onOff(on)
  if on == nil then return "not read" end
  return on and "on" or "off"
end

-- A and the patterns read on, in order.
local function patternsOn(d)
  local list = { "a" }
  for i, p in ipairs({ "b", "c", "d" }) do
    if d.states and d.states[i] then list[#list + 1] = p end
  end
  return list
end

-- Pattern: a stepper over A and the patterns read on, from the pattern read as selected.
local function patternStepper(ov)
  local ctl
  ctl = ov:addStepper({
    label = "Pattern",
    settle = 3000,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "drumsq")
      local d = v and v.drumSq
      if not (d and d.selected) then return "not read" end
      local list = patternsOn(d)
      local at = table.find(list, d.selected)
      if not at then return string.upper(d.selected) .. ", not on" end
      return string.upper(d.selected) .. ", " .. at .. " of " .. #list
    end,
    onStep = function(dir, o)
      if busy("Pattern") then return false end
      local v = ensure(o, "drumsq")
      local d = v and v.drumSq
      local list = d and patternsOn(d)
      local at = list and d.selected and table.find(list, d.selected)
      if not at then
        say(d and d.selected and "Pattern: the pattern selected is not one read on"
          or "Pattern: the selected pattern cannot be read now")
        return false
      end
      local to = at + dir
      if to < 1 or to > #list then
        ctl.settle = 1
        return
      end
      ctl.settle = 3000
      local spec = built("Pattern", Sq.select_central_drum_sq_pattern, { pattern = list[to] })
      if not spec then return false end
      if run(o, "Pattern", spec, true) then return false end
    end,
  })
end

local function drumSqGroup(ov)
  ov:group(function() return cache.values.central == "drum_sq" end, function(q)
    drumButton(q, "Pattern chain", "drumsq", function(v)
      return onOff(v.drumSq and v.drumSq.chain)
    end, function() return Sq.central_drum_sq_pattern_player, {} end)
    drumButton(q, "Auto follow", "drumsq", function(v)
      return onOff(v.drumSq and v.drumSq.follow)
    end, function() return Sq.central_drum_sq_auto_follow, {} end)
    patternStepper(q)
    for i, p in ipairs({ "b", "c", "d" }) do
      drumButton(q, "Pattern " .. string.upper(p), "drumsq", function(v)
        return onOff(v.drumSq and v.drumSq.states and v.drumSq.states[i])
      end, function() return Sq.toggle_central_drum_sq_pattern, { pattern = p } end)
    end
  end)
end

local MODE_ORDER = { "drum", "sq" }
local MODE_WORDS = { drum = "DRUMKIT", sq = "SEQUENCE" }

-- The slot read as selected, or nil and why.
local function selectedSlot(v)
  if v.kitSelected then return v.kitSelected end
  return nil, "the selected slot cannot be read now"
end

-- A slot's Mute or Solo, the selected slot's.
local function slotSwitch(ov, label, button)
  local key = Drums.BUTTONS[button][1]
  drumButton(ov, label, "drums", function(v)
    local s = selectedSlot(v)
    if not s then return "the selected slot cannot be read now" end
    local on = v[key] and v[key][s]
    return D.PART_LABELS[s] .. ", " .. (on == nil and "not read" or (on and "on" or "off"))
  end, function(v)
    local s, why = selectedSlot(v)
    if not s then return nil, nil, why end
    return Drums.toggle_drumkit_button, { index = s, button = button }
  end)
end

local function drumsGroup(ov)
  ov:group(function() return cache.values.selectedType == "drums" end, function(g)
    drumButton(g, "Drum pattern", "strip", function(v)
      if v.drumPattern == nil then return "not read" end
      return v.drumPattern and "on" or "off"
    end, function() return Drums.toggle_drum_pattern, {} end)
    drumStepper(g, "Drum mode", "strip", #MODE_ORDER, function(v)
      if v.drumMode == nil then return nil, "the drum mode cannot be read now" end
      return table.find(MODE_ORDER, v.drumMode)
    end, function(v) return v.drumMode and MODE_WORDS[v.drumMode] or "not read" end,
    function(to) return Drums.select_drum_mode, { mode = MODE_ORDER[to] } end)
    drumButton(g, "Drum lock", "strip", function(v)
      if v.drumLock == nil then return "not read" end
      return v.drumLock and "locked" or "unlocked"
    end, function() return Drums.select_drum_mode, { mode = "lock" } end)
    drumStepper(g, "Drum slot", "drums", #D.PART_LABELS, selectedSlot, function(v)
      local s = v.kitSelected
      if not s then return "not read" end
      return D.PART_LABELS[s] .. ", " .. s .. " of " .. #D.PART_LABELS
    end, function(to) return Drums.select_drumkit_part, { index = to } end)
    slotSwitch(g, "Slot mute", "mute")
    slotSwitch(g, "Slot solo", "solo")
    drumStepper(g, "Central tab", "central", #C.TAB_ORDER, function(v)
      if v.central == nil then return nil, "the central tab cannot be read now" end
      return table.find(C.TAB_ORDER, v.central)
    end, function(v) return v.central and Drums.CENTRAL_WORDS[v.central] or "not read" end,
    function(to) return Drums.select_central_tab, { tab = C.TAB_ORDER[to] } end)
    drumSqGroup(g)
  end)
end

-- ── The macros ───────────────────────────────────────────────────────────────

local function macroTabStepper(ov)
  local ctl
  ctl = ov:addStepper({
    label = "Macro tab",
    settle = 3000,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "macros", true)
      if not (v and v.macroCount) then return "not read" end
      if not v.macroSelected then return "of " .. v.macroCount .. ", the one shown not read" end
      return v.macroSelected .. " of " .. v.macroCount
    end,
    onStep = function(dir, o)
      if busy("Macro tab") then return false end
      local v = ensure(o, "macros", true)
      if not (v and v.macroCount and v.macroSelected) then
        say(v and v.macroCount and "Macro tab: the macro tab shown cannot be read now"
          or "Macro tab: the macro tabs cannot be counted now")
        return false
      end
      local to = v.macroSelected + dir
      if to < 1 or to > v.macroCount then
        ctl.settle = 1
        return
      end
      ctl.settle = 3000
      local spec = built("Macro tab", Macros.select_macro_tab, { index = to })
      if not spec then return false end
      if run(o, "Macro tab", spec, true) then return false end
    end,
  })
end

local function macroButtonOf(ov, label, build, args)
  ov:addCustomButton({
    label = label,
    onActivate = function(o)
      if busy(label) then return end
      local spec = built(label, build, args)
      if spec then run(o, label, spec) end
    end,
  })
end

-- Macro knob `i`: its name and amount as read; Return resets it.
local function macroKnob(ov, i)
  local label = "Macro " .. i
  ov:addCustomButton({
    label = label,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "macros", true)
      if not v then return "not read" end
      local name = v.macroLabels and v.macroLabels[i]
      local a = v.macroAmounts and v.macroAmounts[i]
      return (name and (name .. ", ") or "") .. (a and (a .. " percent") or "amount not read")
    end,
    onActivate = function(o)
      if busy(label) then return end
      local spec = built(label, Macros.reset_macro_amount, { index = i })
      if spec then run(o, label, spec) end
    end,
  })
end

-- Macro button `i`: its name, on or off as read; Return clicks it.
local function macroButton(ov, i)
  local label = "Macro button " .. i
  ov:addCustomButton({
    label = label,
    text = function(o)
      if pending then return nil end
      local v = ensure(o, "macros", true)
      if not v then return "not read" end
      local name = v.macroLabels and v.macroLabels[3 + i]
      local on = v.macroButtons and v.macroButtons[i]
      return (name and (name .. ", ") or "") .. (on == nil and "not read" or (on and "on" or "off"))
    end,
    onActivate = function(o)
      if busy(label) then return end
      local spec = built(label, Macros.toggle_macro_button, { index = i })
      if spec then run(o, label, spec) end
    end,
  })
end

-- Adds the controls to overlay `ov`, after the header's own. `ring`: main.luau's helpers.
function Ring.build(ov, ring)
  R = ring
  oscillatorStepper(ov)
  switch(ov, "Mute")
  switch(ov, "Solo")
  addOscillator(ov)
  drumsGroup(ov)
  oscKnob(ov, "Level", "volume")
  oscKnob(ov, "Pan", "pan")
  oscKnob(ov, "Transpose", "transpose")
  oscKnob(ov, "Fine tune", "fine_tune")
  oscKnob(ov, "Random pitch", "random_pitch")
  macroTabStepper(ov)
  macroButtonOf(ov, "Add macro tab", Macros.add_macro_tab, {})
  macroButtonOf(ov, "Delete last macro tab", Macros.delete_macro_tab, {})
  for i = 1, 3 do macroKnob(ov, i) end
  for i = 1, 2 do macroButton(ov, i) end
end

return Ring
