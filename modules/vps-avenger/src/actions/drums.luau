-- The Drums tab's actions (the plan's slice 8, its drums), for the page's trips and the ring alike:
-- each a spec for src/act.luau, as osc.luau's are — what is read before the click, where the click
-- goes, what is watched, and what the read after it says. By the avenger_control tool's names and
-- arguments (its actions.py), with its points (its clicker and header_map.py, src/tables/drums.luau),
-- and its defects fixed: nothing is clicked unless the strip reads the Drums tab selected — the tool
-- clicked the drum coordinates whatever was shown, which over an oscillator tab lands on the
-- oscillator's knobs and filter — and every answer is what was read, never what the click was
-- assumed to do:
--
--   toggle_drum_pattern   the pattern switch; read after it
--   select_drum_mode      DRUMKIT, LOCK or SEQUENCE; the mode or the lock read after it (the tool
--                         wrote the mode before it read)
--   previous_drumkit, next_drumkit   the drumkit's Back and Next; the twelve slots' names read before
--                         and after, and the ones that changed said (the tool reset the names to the
--                         factory ones, or kept the old kit's)
--   select_drumkit_part   a slot's name; the slot read as selected after it (the tool took the slot
--                         clicked for the selected one)
--   toggle_drumkit_button   a slot's Mute, Solo, Lock or Loop; every slot's four read after it —
--                         a Solo that mutes the others shows so, one that does not shows that (the
--                         tool wrote every other slot muted, flipped Loop from memory)
--   drumkit_menu_item     a slot's menu with the right button, its item at the tool's row: Lock All
--                         and Unlock All (every lock read), Copy and Swap, whose four items open a
--                         submenu of the slots chosen in it (O:chooseMenuItem's `submenu`), the
--                         target placed from the submenu's own top, Remove Sample (the names read),
--                         and Save Drumkit as, whose file dialog is waited for (the tool wrote the
--                         locks and moved the names in memory, and clicked the submenu after 0.08 s
--                         whether or not anything had opened, at its item's row plus the target's,
--                         which a submenu JUCE opens upward does not have)
--   drumkit_column        a slot's Back, Next (its name read) and Edit Sample (the selection and the
--                         central tab read); its Browse is a hand-over (src/handover.luau)
--   reset_drumkit_control   a double-click on a slot's Volume, Pan or Pitch: no reader reads a drum
--                         knob, so the answer is the change wait's own (src/act.luau)
--   select_central_tab    EDITOR, ARP, DRUM SQ or STEP SQ, and the central tab read after it with
--                         any tab of the strip selected (the tool never read it after its click)
--
-- A slot's knobs turned (drumkit_control) are a press held through a hand-over of the keys
-- (src/amount.luau, Am.drum), which refuses as these do (A.refusal).
--
-- Each builder is `(args, label, known) -> spec | nil, why`, as osc.luau's: `known` what was last
-- read in this window, for words only — a slot's name. nil refuses before anything is read.

local D = host.include("src/tables/drums.luau")
local C = host.include("src/tables/centre.luau")
local Centre = host.include("src/read/centre.luau")
local DrumSq = host.include("src/read/drumsq.luau")

local A = {}

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
end

-- A box `size` round table point `p`, as a rectangle.
local function box(p, size)
  return { p[1] - size[1] / 2, p[2] - size[2] / 2, p[1] + size[1] / 2, p[2] + size[2] / 2 }
end
A.box = box

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
A.same = same

-- Whether every one of `keys` reads the same in `vb` and `va`.
local function sameKeys(vb, va, keys)
  if not va then return false end
  for _, k in ipairs(keys) do
    if not same(vb[k], va[k]) then return false end
  end
  return true
end
A.sameKeys = sameKeys

-- How list `a`, read before the click, and `b`, read after it, compare in entries `from` to `to`:
-- "differ" where one entry read both times differs; else "once" where one was read only one of
-- the two times — a text read that missed a name says nothing of it, and an OCR dropout is no
-- change; else "same" where they were read and alike, "none" where nothing was read.
local function compared(a, b, from, to)
  local both, once = 0, false
  for r = from, to do
    local x, y = a and a[r], b and b[r]
    if x ~= nil and y ~= nil then
      if x ~= y then return "differ" end
      both += 1
    elseif x ~= nil or y ~= nil then
      once = true
    end
  end
  if once then return "once" end
  return both > 0 and "same" or "none"
end
A.compared = compared

-- A `same` (src/act.luau) from a list of `compared`: false where anything differs, true where
-- everything was read alike, and nil otherwise — the change wait's verdict decides.
local function verdict(list)
  local sure = true
  for _, c in ipairs(list) do
    if c == "differ" then return false end
    if c ~= "same" then sure = false end
  end
  if sure then return true end
  return nil
end
A.verdict = verdict

-- Why nothing is clicked, and where the focus goes then, unless the strip read shows the Drums tab
-- selected; nil when it does.
function A.refusal(vb)
  if vb.selectedType == nil then return "the selected tab cannot be read now" end
  if vb.selectedType ~= "drums" then return "the Drums tab is not selected; nothing was clicked", "drums-tab" end
  return nil
end

-- A slot's index, 1 to 12, or nil.
local function slotArg(v)
  local i = whole(v)
  if i and i >= 1 and i <= #D.PART_LABELS then return i end
  return nil
end
A.slotArg = slotArg

-- Slot `i`'s name for words: as read in `v`, else as last read (`known`), else its factory name —
-- the slot's role in the kit.
function A.slotName(v, known, i)
  local n = v and v.kitNames and v.kitNames[i]
  if n and n ~= "" then return n end
  n = known and known.kitNames and known.kitNames[i]
  if n and n ~= "" then return n end
  return D.PART_LABELS[i]
end
local slotName = A.slotName

-- Whether `v` has any slot's name read.
local function namesRead(v)
  if not (v and v.kitNames) then return false end
  for i = 1, #D.PART_LABELS do
    if v.kitNames[i] then return true end
  end
  return false
end

-- A decide that refuses off the Drums tab, and clicks `at` (with `extra`'s fields) on it.
local function onDrums(at, extra)
  return function(vb)
    local why, focus = A.refusal(vb)
    if why then return nil, why, focus end
    local click = { at = at }
    for k, v in pairs(extra or {}) do click[k] = v end
    return click
  end
end

-- The two modes' captions and the lock between them; the name column down the whole table.
local MODE_BAND = { D.MODE_DRUM_SELECTION[1], D.MODE_SELECTION_TOP, D.MODE_SQ_SELECTION[2], D.MODE_SELECTION_BOTTOM }
local NAME_COLUMN = { D.NAME_COLUMN[1], D.TABLE_TOP, D.NAME_COLUMN[2], D.TABLE_BOTTOM }

-- The page's words for what a read after an action says, and its focus.
local function told(words, how, focus)
  if how == "unchanged" then return { note = "unchanged, " .. words, said = words .. ", unchanged" } end
  return { said = words, focus = focus }
end
A.told = told

function A.toggle_drum_pattern(_, label)
  return {
    label = label,
    part = "strip",
    decide = onDrums(D.PATTERN),
    watch = function() return { box(D.PATTERN, D.PATTERN_STATE) } end,
    same = function(vb, va) return va.drumPattern ~= nil and va.drumPattern == vb.drumPattern end,
    answer = function(_, va, how)
      local a = va and va.drumPattern
      if a == nil then return { error = "the pattern switch cannot be read now" } end
      return told("Pattern " .. (a and "ON" or "OFF"), how)
    end,
  }
end

-- DRUMKIT, LOCK and SEQUENCE, by the page's `mode`.
A.MODES = { drum = D.MODE_DRUM, lock = D.MODE_LOCK, sq = D.MODE_SQ }

function A.select_drum_mode(args, label)
  local mode = args.mode
  local at = type(mode) == "string" and A.MODES[mode] or nil
  if not at then return nil, tostring(mode) .. " is no drum mode" end
  return {
    label = label,
    part = "strip",
    decide = onDrums(at),
    watch = function() return { MODE_BAND, box(D.MODE_LOCK, D.LOCK_STATE) } end,
    same = function(vb, va)
      if mode == "lock" then return va.drumLock ~= nil and va.drumLock == vb.drumLock end
      return va.drumMode ~= nil and va.drumMode == vb.drumMode
    end,
    answer = function(_, va, how)
      if mode == "lock" then
        local a = va and va.drumLock
        if a == nil then return { error = "the lock cannot be read now" } end
        return told(a and "LOCKED" or "UNLOCKED", how)
      end
      local m = va and va.drumMode
      if m == nil then return { error = "the drum mode cannot be read now" } end
      local name = m == "sq" and "SEQUENCE" or "DRUMKIT"
      if how == "unchanged" then return { note = "unchanged, " .. name .. " is selected", said = name .. " is selected, unchanged" } end
      return { said = name .. " selected" }
    end,
  }
end

-- The drumkit's Back or Next: the names read before and after; what changed is said. In SEQUENCE
-- mode without the lock they step the sequence, which nothing reads (the tool's help says so).
local function kitStep(at)
  return function(_, label)
    return {
      label = label,
      part = "drums",
      text = true,
      textAfter = true,
      decide = onDrums(at),
      watch = function() return { NAME_COLUMN, MODE_BAND } end,
      -- Only the slots read both times count; one read only once leaves it to the change wait.
      same = function(vb, va) return verdict({ compared(vb.kitNames, va.kitNames, 1, #D.PART_LABELS) }) end,
      answer = function(vb, va, how)
        if not va then return { error = "the drumkit cannot be read now" } end
        if how == "unchanged" then
          if vb.drumMode == "sq" and not vb.drumLock then return { note = "the slots read as before; the sequence is not read" } end
          return { note = "nothing VPS Avenger shows changed" }
        end
        -- What changed: the slots read both times whose names differ; one read only after is not
        -- said to have changed.
        local changed = {}
        if namesRead(va) and namesRead(vb) then
          for i = 1, #D.PART_LABELS do
            local n, was = va.kitNames[i], vb.kitNames[i]
            if n and was and n ~= was then changed[#changed + 1] = D.PART_LABELS[i] .. " reads " .. n end
          end
        end
        if #changed == 0 then return { said = "VPS Avenger redrew the drumkit" } end
        local shown = {}
        for j = 1, math.min(3, #changed) do shown[j] = changed[j] end
        return { said = table.concat(shown, ", ") .. (#changed > 3 and (", and " .. (#changed - 3) .. " more") or "") }
      end,
    }
  end
end
A.previous_drumkit = kitStep(D.KIT_BACK)
A.next_drumkit = kitStep(D.KIT_NEXT)

function A.select_drumkit_part(args, label, known)
  local i = slotArg(args.index)
  if not i then return nil, "no drum slot was named" end
  return {
    label = label,
    part = "drums",
    decide = onDrums(D.partPoint(i)),
    watch = function() return { NAME_COLUMN } end,
    same = function(vb, va) return va.kitSelected ~= nil and va.kitSelected == vb.kitSelected end,
    answer = function(_, va, how)
      local s = va and va.kitSelected
      if s == nil then return { error = "the selected slot cannot be read now" } end
      local name = slotName(va, known, s)
      if how == "unchanged" then return { note = "unchanged, " .. name .. " is selected", said = name .. " is selected, unchanged" } end
      if s ~= i then return { said = name .. " selected", note = "slot " .. i .. " was clicked; " .. name .. " reads selected" } end
      return { said = name .. " selected" }
    end,
  }
end

-- A slot's four buttons: the values' key and the page's word.
A.BUTTONS = { mute = { "kitMute", "Mute" }, solo = { "kitSolo", "Solo" }, lock = { "kitLock", "Lock" },
  loop = { "kitLoop", "Loop" } }

function A.toggle_drumkit_button(args, label, known)
  local i = slotArg(args.index)
  if not i then return nil, "no drum slot was named" end
  local b = type(args.button) == "string" and A.BUTTONS[args.button] or nil
  if not b then return nil, tostring(args.button) .. " is no button of a drum slot's" end
  local key, title = b[1], b[2]
  local at = D.columnPoint(i, args.button)
  local function valueOf(v) return v and v[key] and v[key][i] end
  return {
    label = label,
    part = "drums",
    decide = onDrums(at),
    watch = function() return { box(at, D.BUTTON_STATE) } end,
    same = function(vb, va)
      local a = valueOf(va)
      return a ~= nil and a == valueOf(vb)
    end,
    answer = function(_, va, how)
      local name = slotName(va, known, i)
      local a = valueOf(va)
      if a == nil then return { error = name .. "'s " .. title .. " cannot be read now" } end
      return told(name .. " " .. title .. " " .. (a and "ON" or "OFF"), how, "drumkit-" .. args.button .. "-" .. i)
    end,
  }
end

-- The page's words for a slot menu's items, by the tool's keys.
A.ITEM_WORDS = {}
for _, e in ipairs(D.MENU_ITEMS) do A.ITEM_WORDS[e[1]] = e[2] end

-- An item as the tool takes it: lower case, spaces and dashes to "_".
local function itemKey(item)
  local s = string.lower(tostring(item or ""))
  s = string.gsub(s, "^%s+", "")
  s = string.gsub(s, "%s+$", "")
  return (string.gsub(s, "[ %-]", "_"))
end
A.itemKey = itemKey

local LED_KEYS = { "kitMute", "kitSolo", "kitLock", "kitLoop" }

function A.drumkit_menu_item(args, label, known)
  local i = slotArg(args.index)
  if not i then return nil, "no drum slot was named" end
  local item = itemKey(args.item)
  local words = A.ITEM_WORDS[item]
  if not words then return nil, tostring(args.item) .. " is no item of a drum slot's menu" end
  local sub = D.SUBMENU_ITEMS[item] == true
  local target = nil
  if sub then
    target = slotArg(args.target_index)
    if not target then return nil, "no target slot was named" end
    if target == i then return nil, "a slot is not copied or swapped with itself" end
  end
  local locks = item == "lock_all" or item == "unlock_all"
  local save = item == "save_drumkit_as"
  local names = not (locks or save)
  local withSettings = item == "copy_slot_to" or item == "swap_slot_with"
  local focus = "drumkit-menu-" .. i
  return {
    label = label,
    part = "drums",
    text = names,
    textAfter = names,
    decide = function(vb)
      local why, f = A.refusal(vb)
      if why then return nil, why, f end
      local at = D.menuItemPoint(i, item)
      local t = sub and D.submenuTarget(target)
      return { at = D.columnPoint(i, "menu"), button = "right", menu = {
        item = at, retries = 3, dialog = save or nil,
        submenu = t and { x = t[1], below = t[2], row = at[2], first = D.SUBMENU_FIRST_ROW } or nil,
      } }
    end,
    watch = function()
      if locks then return { D.columnRect("lock", D.BUTTON_STATE[1]) } end
      if sub then return { D.rowRect(i), D.rowRect(target) } end
      return { D.rowRect(i) }
    end,
    -- The locks, or the names (and with a slot's settings the LEDs) of the rows the item changes,
    -- each as kitStep compares the names (`compared`): any of them that differs is a change, one
    -- read only once or not at all leaves it to the change wait.
    same = function(vb, va)
      if locks then return verdict({ compared(vb.kitLock, va.kitLock, 1, #D.PART_LABELS) }) end
      local seen = {}
      for _, r in ipairs(sub and { i, target } or { i }) do
        seen[#seen + 1] = compared(vb.kitNames, va.kitNames, r, r)
        if withSettings then
          for _, k in ipairs(LED_KEYS) do seen[#seen + 1] = compared(vb[k], va[k], r, r) end
        end
      end
      return verdict(seen)
    end,
    answer = function(_, va, how)
      if save then
        if how == "chosen" then return { said = "Save Drumkit as chosen", focus = focus } end
        return { note = "no file dialog was seen" }
      end
      if not va then return { error = words .. " chosen; the drumkit cannot be read now" } end
      if how == "unchanged" then return { note = words .. " chosen; nothing VPS Avenger shows changed" } end
      if locks then
        local n, read = 0, 0
        for r = 1, #D.PART_LABELS do
          local a = va.kitLock and va.kitLock[r]
          if a ~= nil then
            read += 1
            if a then n += 1 end
          end
        end
        if read == 0 then return { error = words .. " chosen; the slots' locks cannot be read now" } end
        return { said = n .. " of 12 slots locked" .. (read < 12 and (", " .. (12 - read) .. " not read") or ""),
          focus = focus }
      end
      local function reads(r)
        local n = va.kitNames and va.kitNames[r]
        return n and (D.PART_LABELS[r] .. " reads " .. n) or nil
      end
      if item == "remove_sample" then
        return { said = reads(i) or "Remove Sample chosen; VPS Avenger redrew the slot", focus = focus }
      end
      if item == "copy_sample_to" or item == "copy_slot_to" then
        local r = reads(target)
        return { said = words .. " chosen" .. (r and (": " .. r) or ("; VPS Avenger redrew " .. D.PART_LABELS[target] .. "'s row")),
          focus = focus }
      end
      local a, b = reads(i), reads(target)
      if a and b then return { said = a .. ", " .. b, focus = focus } end
      return { said = words .. " chosen; VPS Avenger redrew both rows", focus = focus }
    end,
  }
end

-- A slot's columns the page sends: Browse (a hand-over), Back, Next and Edit Sample.
A.COLUMNS = { browse = true, back = true, next = true, edit_sample = true }

-- The slot and the column of a drumkit_column, or nil and why.
function A.columnArgs(args)
  local i = slotArg(args.index)
  if not i then return nil, "no drum slot was named" end
  local column = args.column
  if not (type(column) == "string" and A.COLUMNS[column]) then return nil, tostring(column) .. " is no column of a drum slot's" end
  return i, column
end

-- The central tabs' words.
A.CENTRAL_WORDS = { editor = "EDITOR", arp = "ARP", drum_sq = "DRUM SQ", step_sq = "STEP SQ" }

function A.drumkit_column(args, label, known)
  local i, column = A.columnArgs(args)
  if not i then return nil, column end
  local at = D.columnPoint(i, column)
  local focus = "drumkit-" .. string.gsub(column, "_", "-") .. "-" .. i
  if column == "edit_sample" then
    -- The sample editor's part: what Edit Sample shows — the selection, the central tab and the
    -- editor's tab, view and switch with it (read/sample.luau).
    return {
      label = label,
      part = "editor",
      fields = { "browser" },
      decide = onDrums(at),
      watch = function() return { NAME_COLUMN, Centre.ROW } end,
      same = function(vb, va) return sameKeys(vb, va, { "kitSelected", "central" }) end,
      answer = function(_, va, how)
        if not va then return { error = "VPS Avenger cannot be read now" } end
        local parts = {}
        parts[1] = va.kitSelected and (slotName(va, known, va.kitSelected) .. " selected")
          or "the selected slot cannot be read now"
        parts[2] = va.central and (A.CENTRAL_WORDS[va.central] .. " shown") or "the central tab cannot be read now"
        return told(table.concat(parts, "; "), how, focus)
      end,
    }
  end
  -- Back and Next: the slot's name read before and after.
  return {
    label = label,
    part = "drums",
    text = true,
    textAfter = true,
    decide = onDrums(at),
    watch = function() return { D.nameRect(i) } end,
    same = function(vb, va) return verdict({ compared(vb.kitNames, va.kitNames, i, i) }) end,
    answer = function(_, va, how)
      local role = D.PART_LABELS[i]
      local n = va and va.kitNames and va.kitNames[i]
      if not n then
        if how == "changed" then return { said = "VPS Avenger redrew " .. role .. "'s name", focus = focus } end
        if how == "unchanged" then return { note = "nothing VPS Avenger shows changed" } end
        return { error = role .. "'s name cannot be read now" }
      end
      return told(role .. " reads " .. n, how, focus)
    end,
  }
end

-- A slot's knobs, by the page's `control`, and their words.
A.CONTROL_WORDS = { volume = "Volume", pan = "Pan", pitch = "Pitch" }

function A.reset_drumkit_control(args, label, known)
  local i = slotArg(args.index)
  if not i then return nil, "no drum slot was named" end
  local control = args.control
  local Control = type(control) == "string" and A.CONTROL_WORDS[control] or nil
  if not Control then return nil, tostring(control) .. " is no control of a drum slot's" end
  return {
    label = label,
    part = "drums",
    decide = onDrums(D.columnPoint(i, control), { double = true }),
    watch = function() return { D.knobBox(i, control) } end,
    -- No `same`: nothing reads a drum knob (the tool read none, the page shows none), so the change
    -- wait's verdict on the knob's box is the answer.
    answer = function(_, va, how)
      local name = slotName(va, known, i)
      if how == "changed" then
        return { said = "VPS Avenger redrew " .. name .. "'s " .. Control .. " knob", focus = "drumkit-reset-" .. control .. "-" .. i }
      end
      if how == "unchanged" then return { note = "unchanged: VPS Avenger drew no change at " .. name .. "'s " .. Control .. " knob" } end
      return { error = name .. "'s " .. Control .. " knob cannot be read now" }
    end,
  }
end

function A.select_central_tab(args, label)
  local tab = args.tab
  local at = type(tab) == "string" and C.TABS[tab] or nil
  if not at then return nil, string.upper(tostring(tab)) .. " is not reachable from this page yet" end
  return {
    label = label,
    part = "central",
    -- What the tab shows read with it: DRUM SQ's LENGTH, the sample editor's tab and view.
    fields = { "sq_length", "browser" },
    decide = function() return { at = at } end,
    watch = function() return { Centre.ROW, DrumSq.ROW } end,
    same = function(vb, va) return va.central ~= nil and va.central == vb.central end,
    answer = function(_, va, how)
      local c = va and va.central
      if c == nil then return { error = "the central tab cannot be read now" } end
      local w = A.CENTRAL_WORDS[c]
      if how == "unchanged" then return { note = "unchanged, " .. w .. " is selected", said = w .. " is selected, unchanged" } end
      return { said = w .. " selected", focus = "central-tab-" .. tab }
    end,
  }
end

return A
