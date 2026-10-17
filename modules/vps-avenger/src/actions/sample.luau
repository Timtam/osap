-- The sample editor's actions (the plan's slice 8, its other half), for the page's trips: each a
-- spec for src/act.luau, as drums.luau's are — what is read before the click, where the click goes,
-- what is watched, and what the read after it says. By the avenger_control tool's names and
-- arguments (its actions.py), with its points and menu rows (its clicker and header_map.py,
-- src/tables/centre.luau), and its defects fixed. The tool clicked the editor whatever was shown and
-- wrote, unread, the strip's tab as Drums, the view, the editor's tab and the selected slot after
-- every one of them. Here nothing is clicked unless the read before shows the Drums tab, EDITOR as
-- the central tab and the editor in its place — not the sample browser — and each control's own
-- tab: Gain and Range share a point, as Play Mode lies beside Amp's Strength, so a click on the
-- wrong tab is another control's. Every answer is what was read after:
--
--   select_sample_editor_tab {tab}   Sample, Amp, Pitch or EQ; the tab and its switch read after
--   toggle_sample_editor_enable {tab}   Amp's, Pitch's or EQ's switch on its own tab; read after
--                         (the tool flipped it in memory first)
--   sample_editor_play_mode, sample_editor_grid, sample_editor_eq_type {band}, sample_editor_loop_stretch
--                         {item}   a menu opened with the LEFT button on its field (O:chooseMenuItem,
--                         its popup below it, popups.luau), its item at the tool's row; the field
--                         read before and after as text, the item it reads said — and Loop-Stretch's
--                         slot's Loop read after it, which the tool wrote from the item
--   sample_editor_panel_button {button = back | next}   the envelope's Back and Next on Amp or
--                         Pitch; its preset's name read before and after (Menu and Preset are
--                         hand-overs, src/handover.luau)
--   reset_sample_editor_control {control}   a double-click on one of the eight knobs: no reader
--                         reads them, so the answer is the change wait's own (src/act.luau)
--
-- The knobs turned (sample_editor_control) are a press held through a hand-over of the keys
-- (src/amount.luau, Am.sample); a typed field (sample_editor_field), a browser row
-- (sample_browser_focus) and the envelope's Menu and Preset hand the keyboard to Avenger
-- (src/handover.luau). Each refuses as these do (Sa.refusal and the others below).
--
-- Each builder is `(args, label, known) -> spec | nil, why`, as drums.luau's: `known` what was last
-- read in this window, for words only. nil refuses before anything is read.

local C = host.include("src/tables/centre.luau")
local D = host.include("src/tables/drums.luau")
local Sm = host.include("src/read/sample.luau")
local Drums = host.include("src/actions/drums.luau")

local Sa = {}

local box, verdict, told, itemKey = Drums.box, Drums.verdict, Drums.told, Drums.itemKey

-- How value `a`, read before the click, and `b`, read after it, compare: as drums.luau's
-- `compared` for one entry.
local function one(a, b)
  if a ~= nil and b ~= nil then return a == b and "same" or "differ" end
  if a ~= nil or b ~= nil then return "once" end
  return "none"
end
Sa.one = one

-- ── Where nothing is clicked ─────────────────────────────────────────────────

-- Why nothing is clicked in the sample editor, and where the focus goes then; nil where the read
-- before shows the Drums tab, EDITOR as the central tab and the editor shown.
function Sa.refusal(vb)
  local why, focus = Drums.refusal(vb)
  if why then return why, focus end
  if vb.central == nil then return "the central tab cannot be read now" end
  if vb.central ~= "editor" then return "EDITOR is not the central tab; nothing was clicked", "central-tab-editor" end
  if vb.editorView == "browser" then
    return "the sample browser is shown, not the sample editor; nothing was clicked", "sample-browser-item-1"
  end
  if vb.editorView == nil then return "the sample editor cannot be read now" end
  return nil
end

-- Why nothing is clicked unless the editor's tab read is one of `wanted`; nil where it is.
function Sa.tabRefusal(vb, wanted)
  if vb.editorTab == nil then return "the sample editor's tab cannot be read now" end
  for _, t in ipairs(wanted) do
    if vb.editorTab == t then return nil end
  end
  local words = {}
  for i, t in ipairs(wanted) do words[i] = "the " .. Sm.TAB_WORDS[t] end
  return table.concat(words, " or ") .. " tab is not shown; nothing was clicked", "sample-editor-tab-" .. wanted[1]
end

-- The editor shown and its tab one of `tabs` (nil: any).
local function onTab(vb, tabs)
  local why, focus = Sa.refusal(vb)
  if why then return why, focus end
  if tabs then return Sa.tabRefusal(vb, tabs) end
  return nil
end

-- Why nothing is clicked in the sample browser: the browser shown, and a slot read as selected, which
-- the row loads into.
function Sa.browserRefusal(vb)
  local why, focus = Drums.refusal(vb)
  if why then return why, focus end
  if vb.central == nil then return "the central tab cannot be read now" end
  if vb.central ~= "editor" then return "EDITOR is not the central tab; nothing was clicked", "central-tab-editor" end
  if vb.editorView == "editor" then
    return "the sample editor is shown, not the sample browser; nothing was clicked", "sample-editor-tab-sample"
  end
  if vb.editorView == nil then return "the sample browser cannot be read now" end
  if vb.kitSelected == nil then return "the selected slot cannot be read now; nothing was clicked" end
  return nil
end

-- The envelope's buttons: on Amp or Pitch.
function Sa.envelopeRefusal(vb)
  return onTab(vb, { "amp", "pitch" })
end

-- ── The typed fields ─────────────────────────────────────────────────────────
-- Each on its tab: the tool checked only some of them, from memory.
Sa.FIELD_TABS = {
  start = { "sample" }, ["end"] = { "sample" }, gain = { "sample" }, speed = { "amp", "pitch" },
  strength = { "amp" }, range = { "pitch" },
  eq1_freq = { "eq" }, eq1_gain = { "eq" }, eq1_q = { "eq" }, eq2_freq = { "eq" }, eq2_gain = { "eq" }, eq2_q = { "eq" },
}
-- Their words, the page's labels.
Sa.FIELD_WORDS = {
  start = "Start", ["end"] = "End", gain = "Gain", speed = "Speed", strength = "Strength", range = "Range",
  eq1_freq = "Band 1 Frequency", eq1_gain = "Band 1 Gain", eq1_q = "Band 1 Q-Factor",
  eq2_freq = "Band 2 Frequency", eq2_gain = "Band 2 Gain", eq2_q = "Band 2 Q-Factor",
}

function Sa.fieldArg(args)
  local f = args.field
  if type(f) == "string" and Sa.FIELD_TABS[f] then return f end
  return nil, tostring(f) .. " is no field of the sample editor's"
end

function Sa.fieldRefusal(vb, field)
  return onTab(vb, Sa.FIELD_TABS[field])
end

-- The page's element for a typed field, on the tab read (the page's ids; the tool's for the EQ's
-- fields were no element of the page).
function Sa.fieldFocus(field, vb)
  local band, what = string.match(field, "^eq(%d)_(%a+)$")
  if band then return "sample-editor-eq-band-" .. band .. "-" .. what end
  if Sa.FIELD_TABS[field][1] == "sample" then return "sample-editor-sample-" .. field end
  return "sample-editor-" .. tostring(vb.editorTab) .. "-" .. field
end

-- ── The knobs ────────────────────────────────────────────────────────────────
Sa.CONTROL_WORDS = { volume = "Volume", pitch = "Pitch", pan = "Pan", spike = "Spike", delay = "Delay",
  lowpass = "Lowpass", highpass = "Highpass", rate = "Rate" }

-- A knob's key as the tool takes it: lower case, spaces to "_", Low-Pass and High-Pass without one.
function Sa.controlKey(control)
  local key = string.gsub(string.lower(tostring(control or "")), "^%s+", "")
  key = string.gsub(string.gsub(key, "%s+$", ""), " ", "_")
  if key == "low_pass" then key = "lowpass" elseif key == "high_pass" then key = "highpass" end
  if Sa.CONTROL_WORDS[key] then return key end
  return nil, tostring(control) .. " is no control of the sample editor's"
end

-- ── The actions ──────────────────────────────────────────────────────────────

-- The editor's part, with the browser's rows read where no tab reads (read/sample.luau), and `more`.
local function fieldsWith(more)
  local f = { "browser" }
  for _, k in ipairs(more or {}) do f[#f + 1] = k end
  return f
end

function Sa.select_sample_editor_tab(args, label)
  local tab = args.tab
  if not (type(tab) == "string" and C.EDITOR_TABS[tab]) then
    return nil, tostring(tab) .. " is no tab of the sample editor's"
  end
  local word = Sm.TAB_WORDS[tab]
  return {
    label = label,
    part = "editor",
    fields = fieldsWith(),
    decide = function(vb)
      local why, focus = Sa.refusal(vb)
      if why then return nil, why, focus end
      return { at = C.EDITOR_TABS[tab] }
    end,
    watch = function() return { C.EDITOR_TABS_ROW, C.SWITCHES } end,
    same = function(vb, va) return va.editorTab ~= nil and va.editorTab == vb.editorTab end,
    answer = function(_, va, how)
      local t = va and va.editorTab
      if t == nil then return { error = "the sample editor's tab cannot be read now" } end
      local w = Sm.TAB_WORDS[t]
      if how == "unchanged" then
        return { note = "unchanged, " .. w .. " is selected", said = w .. " is selected, unchanged" }
      end
      if t ~= tab then return { said = w .. " selected", note = word .. " was clicked; " .. w .. " reads selected" } end
      return { said = w .. " selected", focus = "sample-editor-tab-" .. tab }
    end,
  }
end

function Sa.toggle_sample_editor_enable(args, label)
  local tab = args.tab
  if not (tab == "amp" or tab == "pitch" or tab == "eq") then
    return nil, tostring(tab) .. " has no on/off switch in the sample editor"
  end
  local word = Sm.TAB_WORDS[tab]
  local at = tab == "eq" and C.PANEL.eq_enable or C.PANEL.enable
  local state = tab == "eq" and C.EQ_ENABLE_STATE or C.ENABLE_STATE
  local size = tab == "eq" and C.EQ_ENABLE_STATE_SIZE or C.ENABLE_STATE_SIZE
  local function onOf(v)
    local e = v and v.editorOn
    if e and e.tab == tab then return e.on end
    return nil
  end
  return {
    label = label,
    part = "editor",
    fields = fieldsWith(),
    -- On its own tab only: Amp's and Pitch's switch are clicked at the same point.
    decide = function(vb)
      local why, focus = onTab(vb, { tab })
      if why then return nil, why, focus end
      return { at = at }
    end,
    watch = function() return { box(state, { size[1] + 4, size[2] + 4 }) } end,
    same = function(vb, va)
      local a = onOf(va)
      return a ~= nil and a == onOf(vb)
    end,
    answer = function(_, va, how)
      local a = onOf(va)
      if a == nil then return { error = "the " .. word .. " switch cannot be read now" } end
      return told(word .. " " .. (a and "ON" or "OFF"), how, tab == "eq" and "sample-editor-eq-enable"
        or ("sample-editor-" .. tab .. "-enable"))
    end,
  }
end

-- What a menu's field reads after its item was chosen, in the page's words: `name` the field's
-- words ("Grid"), `words` its items' (tables/centre.luau, VALUE_WORDS), `item` the item chosen, `a`
-- the item the field reads after (nil: not read), `how` the change wait's, `focus` the page's
-- element, `extra` words added to what is said ("; Ride Loop ON"). Shared with DRUM SQ's SPEED
-- (actions/drumsq.luau).
function Sa.menuAnswer(name, words, item, a, how, focus, extra)
  local w = words[item]
  extra = extra or ""
  if a == item then
    if how == "unchanged" then return { note = "unchanged, " .. name .. " reads " .. w .. extra } end
    return { said = name .. " " .. w .. extra, focus = focus }
  end
  if a ~= nil then return { note = w .. " chosen; " .. name .. " reads " .. words[a] .. extra } end
  if how == "changed" then return { said = w .. " chosen; VPS Avenger redrew " .. name .. extra, focus = focus } end
  if how == "unchanged" then return { note = w .. " chosen; nothing VPS Avenger shows changed" } end
  return { error = w .. " chosen; " .. name .. " cannot be read now" }
end

-- A menu opened with the left button on field `o.field` (tables/centre.luau, VALUES), its item
-- `o.item` at `o.at`: `o.tabs` the tabs it is on (nil: any), `o.name` the field's words, `o.focus(vb)`
-- the page's element; `o.extra` = { watch(vb), compare(vb, va), words(vb, va) } for what else the
-- item changes — `compare` a `compared` of drums.luau's, or nil where there is nothing to compare.
local function valueMenu(label, o)
  local words = C.VALUE_WORDS[o.field]
  local function fieldOf(v) return v and v.editorFields and v.editorFields[o.field] end
  local extra = o.extra or {}
  return {
    label = label,
    part = "editor",
    fields = fieldsWith({ o.field }),
    decide = function(vb)
      local why, focus = onTab(vb, o.tabs)
      if why then return nil, why, focus end
      return { at = C.VALUES[o.field], menu = { button = "left", item = o.at, retries = 3 } }
    end,
    watch = function(vb)
      local list = { C.valueRect(o.field) }
      if extra.watch then
        for _, r in ipairs(extra.watch(vb)) do list[#list + 1] = r end
      end
      return list
    end,
    same = function(vb, va)
      local list = { one(fieldOf(vb), fieldOf(va)) }
      local c = extra.compare and extra.compare(vb, va)
      if c then list[#list + 1] = c end
      return verdict(list)
    end,
    answer = function(vb, va, how)
      return Sa.menuAnswer(o.name, words, o.item, fieldOf(va), how, o.focus(vb), extra.words and extra.words(vb, va))
    end,
  }
end

-- An item as the tool takes it, and whether `words` has it.
local function menuItem(args, words, what)
  local item = itemKey(args.item)
  if words[item] then return item end
  return nil, tostring(args.item) .. " is no " .. what
end

-- A note value as the tool takes it: lower case, "/" to "_", spaces gone ("1/8T" is "1_8t").
local function noteKey(v)
  local s = string.lower(tostring(v or ""))
  s = string.gsub(s, "%s+", "")
  return (string.gsub(s, "[/%-]", "_"))
end
Sa.noteKey = noteKey

function Sa.sample_editor_play_mode(args, label)
  local item, why = menuItem(args, C.VALUE_WORDS.play_mode, "Play Mode")
  if not item then return nil, why end
  return valueMenu(label, {
    field = "play_mode", item = item, tabs = { "sample" }, name = "Play Mode",
    at = { C.PLAY_MODE_MENU_X, C.PLAY_MODE_MENU_Y[item] },
    focus = function() return "sample-editor-sample-play-mode" end,
  })
end

function Sa.sample_editor_grid(args, label)
  local item = noteKey(args.item)
  if not C.VALUE_WORDS.grid[item] then return nil, tostring(args.item) .. " is no Grid" end
  return valueMenu(label, {
    field = "grid", item = item, tabs = { "amp", "pitch" }, name = "Grid",
    at = { C.GRID_MENU_X, C.GRID_MENU_Y[item] },
    focus = function(vb) return "sample-editor-" .. tostring(vb.editorTab) .. "-grid" end,
  })
end

function Sa.sample_editor_eq_type(args, label)
  local band = tonumber(args.band)
  if not (band == 1 or band == 2) then return nil, "no EQ band was named" end
  local item, why = menuItem(args, C.VALUE_WORDS.eq_type, "EQ type")
  if not item then return nil, why end
  return valueMenu(label, {
    field = "eq" .. band .. "_type", item = item, tabs = { "eq" }, name = "Band " .. band .. " Type",
    at = { C.EQ_TYPE_MENU_X[band], C.EQ_TYPE_MENU_Y[item] },
    focus = function() return "sample-editor-eq-band-" .. band .. "-type" end,
  })
end

function Sa.sample_editor_loop_stretch(args, label, known)
  local item, why = menuItem(args, C.VALUE_WORDS.loop_stretch, "Loop-Stretch mode")
  if not item then return nil, why end
  -- The slot read as selected before: its Loop is read after, never written from the item.
  local function loopOf(v, i) return i and v and v.kitLoop and v.kitLoop[i] end
  return valueMenu(label, {
    field = "loop_stretch", item = item, name = "Loop-Stretch",
    at = { C.LOOP_STRETCH_MENU_X, C.LOOP_STRETCH_MENU_Y[item] },
    focus = function() return "sample-editor-loop-stretch" end,
    extra = {
      watch = function(vb)
        local i = vb.kitSelected
        return i and { box(D.columnPoint(i, "loop"), D.BUTTON_STATE) } or {}
      end,
      -- Where no slot read as selected, its Loop is left out.
      compare = function(vb, va)
        local i = vb.kitSelected
        if not i then return nil end
        return one(loopOf(vb, i), loopOf(va, i))
      end,
      words = function(vb, va)
        local i = vb.kitSelected
        local on = loopOf(va, i)
        if on == nil then return nil end
        return "; " .. Drums.slotName(va, known, i) .. " Loop " .. (on and "ON" or "OFF")
      end,
    },
  })
end

-- The envelope's buttons the page sends.
Sa.PANEL_BUTTONS = { menu = true, preset = true, back = true, next = true }

function Sa.panelArg(args)
  local b = args.button
  if type(b) == "string" and Sa.PANEL_BUTTONS[b] then return b end
  return nil, tostring(b) .. " is no button of the envelope's"
end

-- The envelope's name, as read; its words, by the tab read.
local function envelopeOf(v) return v and v.editorFields and v.editorFields.envelope_name end
function Sa.envelopeWords(vb)
  return (vb and vb.editorTab == "pitch" and "Pitch" or "Amp") .. " Envelope"
end

-- What the envelope reads after Back, Next, Menu or Preset: its name, or the change wait's verdict
-- where none was read. Shared with the hand-over of Menu and Preset (src/handover.luau).
function Sa.envelopeAnswer(vb, va, how, focus)
  local envelope = Sa.envelopeWords(vb)
  local n = envelopeOf(va)
  if n then return told(envelope .. " reads " .. n, how, focus) end
  if how == "changed" then return { said = "VPS Avenger redrew the " .. envelope, focus = focus } end
  if how == "unchanged" then return { note = "nothing VPS Avenger shows changed" } end
  return { error = "the " .. envelope .. " cannot be read now" }
end

-- Back and Next (Menu and Preset are hand-overs: actions.luau sends them to src/handover.luau).
function Sa.sample_editor_panel_button(args, label)
  local button, why = Sa.panelArg(args)
  if not button then return nil, why end
  return {
    label = label,
    part = "editor",
    fields = fieldsWith({ "envelope_name" }),
    decide = function(vb)
      local no, focus = Sa.envelopeRefusal(vb)
      if no then return nil, no, focus end
      return { at = C.PANEL[button] }
    end,
    watch = function() return { C.ENVELOPE_NAME, C.ENVELOPE_AREA } end,
    same = function(vb, va) return verdict({ one(envelopeOf(vb), envelopeOf(va)) }) end,
    answer = function(vb, va, how)
      return Sa.envelopeAnswer(vb, va, how, "sample-editor-" .. tostring(vb.editorTab) .. "-" .. button)
    end,
  }
end

function Sa.reset_sample_editor_control(args, label)
  local c, why = Sa.controlKey(args.control)
  if not c then return nil, why end
  local word = Sa.CONTROL_WORDS[c]
  return {
    label = label,
    part = "editor",
    fields = fieldsWith(),
    decide = function(vb)
      local no, focus = Sa.refusal(vb)
      if no then return nil, no, focus end
      return { at = C.EDITOR_CONTROLS[c], double = true }
    end,
    watch = function() return { C.editorKnobBox(c) } end,
    -- No `same`: nothing reads the editor's knobs (the tool read none, the page shows none), so the
    -- change wait's verdict on the knob's box is the answer.
    answer = function(_, _, how)
      if how == "changed" then
        return { said = "VPS Avenger redrew the " .. word .. " knob", focus = "sample-editor-reset-" .. c }
      end
      if how == "unchanged" then return { note = "unchanged: VPS Avenger drew no change at the " .. word .. " knob" } end
      return { error = "the " .. word .. " knob cannot be read now" }
    end,
  }
end

return Sa
