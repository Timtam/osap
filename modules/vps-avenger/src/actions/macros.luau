-- The macros' actions (the plan's slice 5), for the page's trips and the ring alike: specs for
-- src/act.luau, as src/actions/osc.luau's are. The avenger_control tool's names, arguments and
-- points (its actions.py, clicker and header_map.py, src/tables/macros.luau), its defects fixed:
--
--   select_macro_tab        the tab, after the tabs are counted from their captions — tab 3's point
--                           lies 14 from the "+" of two tabs, so a tab past the count is never
--                           clicked; the tab shown, its names, buttons and amounts read
--   add_macro_tab           "+", where the count puts it, none at four; the captions read
--   delete_macro_tab        macro tab 1's menu with the right button, its item; none at one tab
--   toggle_macro_button     the button, then its share of highlight and blue read — never turned
--                           over from memory, as the tool did when its read was unsure
--   macro_reset_mode        a knob's field's menu, its reset mode: VPS Avenger shows the mode only
--                           in that menu, so the menu taking the click is all there is to read
--   macro_button_menu_item  a button's menu, its item once (a tick: a second click would undo it);
--                           then the buttons' change waited for and both read again — Special
--                           Switch may turn the other one off
--   reset_macro_amount      a double-click on the knob; the knob reader's amount after it, said
--                           against the tool's default, 0 (Act.resetAnswer)
--
-- Builders as osc.luau's: `(args, label, known) -> spec | nil, why`, `known` the values last read
-- in this window, for words (a name, the tab shown for the page's focus) and never a decision.

local M = host.include("src/tables/macros.luau")
local Knob = host.include("src/read/knob.luau")
local Act = host.include("src/act.luau")

local A = {}

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
end

local function box(p, size)
  return { p[1] - size[1] / 2, p[2] - size[2] / 2, p[1] + size[1] / 2, p[2] + size[2] / 2 }
end

local function plural(n, word) return n .. " " .. word .. (n == 1 and "" or "s") end

-- The tabs' captions and the selected tab's band.
local TABS = { M.TAB_SPANS[1][1], M.TAB_SPANS[1][2], M.TAB_SPANS[4][3], M.TAB_SPANS[1][4] }

-- The macro tabs read, or nil and why.
local function counted(vb)
  local n = vb.macroCount
  if not n then return nil, "the macro tabs cannot be counted now" end
  return n
end

-- The tab shown, for the page's focus: as read after, before, or last; 1 when none was.
local function shown(vb, va, known)
  return (va and va.macroSelected) or vb.macroSelected or (known and known.macroSelected) or 1
end

-- A slot's name as last read — the five fields, knobs first — or the slot's own words.
local function nameOf(slot, vb, known, fallback)
  local labels = (vb and vb.macroLabels) or (known and known.macroLabels)
  local name = labels and labels[slot]
  return (name and name ~= "") and name or fallback
end

function A.select_macro_tab(args, label)
  local i = whole(args.index)
  if not i then return nil, "no macro tab was named" end
  return {
    label = label,
    part = "macros",
    text = true,
    textAfter = true,
    decide = function(vb)
      local n, why = counted(vb)
      if not n then return nil, why end
      if i < 1 or i > n then return nil, string.format("macro tab %d is not there; VPS Avenger shows %d", i, n) end
      return { at = { M.TAB_X[i], M.TAB_Y } }
    end,
    watch = function() return { TABS } end,
    same = function(vb, va) return va.macroSelected ~= nil and va.macroSelected == vb.macroSelected end,
    answer = function(_, va, how)
      local t = va and va.macroSelected
      if not t then return { error = "the macro tab shown cannot be read now" } end
      if how == "unchanged" then return { note = "unchanged, macro tab " .. t .. " is shown", said = "macro tab " .. t .. ", unchanged" } end
      return { said = "macro tab " .. t, focus = "macro-tab-" .. t }
    end,
  }
end

function A.add_macro_tab(_, label)
  return {
    label = label,
    part = "macros",
    text = true,
    textAfter = true,
    decide = function(vb)
      local n, why = counted(vb)
      if not n then return nil, why end
      -- The tool's focus at the limit: the fourth tab.
      if n >= 4 then return nil, "VPS Avenger has 4 macro tabs, the most it takes", "macro-tab-4" end
      return { at = { M.PLUS_X_BY_TAB_COUNT[n], M.TAB_Y } }
    end,
    watch = function() return { TABS } end,
    same = function(vb, va) return va.macroCount ~= nil and va.macroCount == vb.macroCount end,
    answer = function(_, va, how)
      local n = va and va.macroCount
      if not n then return { error = "the macro tabs cannot be counted now" } end
      if how == "unchanged" then return { note = "unchanged, " .. plural(n, "macro tab"), said = plural(n, "macro tab") .. ", unchanged" } end
      return { said = plural(n, "macro tab"), focus = "macro-tab-" .. n }
    end,
  }
end

function A.delete_macro_tab(_, label)
  local x = M.TAB_X[1]
  return {
    label = label,
    part = "macros",
    text = true,
    textAfter = true,
    decide = function(vb)
      local n, why = counted(vb)
      if not n then return nil, why end
      -- The tool's focus at the limit: the only tab.
      if n <= 1 then return nil, "VPS Avenger keeps its only macro tab", "macro-tab-1" end
      return { at = { x, M.TAB_Y }, menu = { item = { x, M.DELETE_Y }, retries = 3 } }
    end,
    watch = function() return { TABS } end,
    same = function(vb, va) return va.macroCount ~= nil and va.macroCount == vb.macroCount end,
    answer = function(_, va, how)
      local n = va and va.macroCount
      if not n then return { error = "Delete chosen; the macro tabs cannot be counted now" } end
      if how == "unchanged" then return { note = "unchanged, " .. plural(n, "macro tab"), said = plural(n, "macro tab") .. ", unchanged" } end
      return { said = plural(n, "macro tab"), focus = n > 1 and "macro-delete-last" or "macro-tab-1" }
    end,
  }
end

function A.toggle_macro_button(args, label, known)
  local i = whole(args.index)
  if not (i and i >= 1 and i <= 2) then return nil, "no macro button was named" end
  return {
    label = label,
    part = "macros",
    -- The names, for the button's own in what is said; the count with them, for the tab shown.
    text = true,
    decide = function() return { at = { M.BUTTON_X[i], M.BUTTON_Y } } end,
    watch = function() return { M.BUTTON_BOUNDS[i] } end,
    same = function(vb, va)
      local a = va.macroButtons and va.macroButtons[i]
      return a ~= nil and a == (vb.macroButtons and vb.macroButtons[i])
    end,
    answer = function(vb, va, how)
      local name = nameOf(3 + i, vb, known, "macro button " .. i)
      local a = va and va.macroButtons and va.macroButtons[i]
      if a == nil then return { error = name .. " cannot be read now" } end
      local words = name .. " " .. (a and "ON" or "OFF")
      if how == "unchanged" then return { note = "unchanged, " .. words, said = words .. ", unchanged" } end
      return { said = words, focus = string.format("macro-%d-slot-%d-on-off", shown(vb, va, known), i + 3) }
    end,
  }
end

-- The reset modes' words, as the page's menu has them.
local MODES = { normal = "Normal Mode", snap = "Snap At Center" }
A.MODES = MODES

function A.macro_reset_mode(args, label, known)
  local i, mode = whole(args.index), args.mode
  if not (i and i >= 1 and i <= 3) then return nil, "no macro knob was named" end
  if not MODES[mode] then return nil, tostring(mode) .. " is no reset mode of a macro's" end
  local field = M.FIELDS["macro" .. i]
  return {
    label = label,
    part = "macros",
    macroCount = known and known.macroCount,
    decide = function()
      return { at = { field[1], field[2] },
        menu = { item = { M.RESET_MODE_X[i], M.RESET_MODE_Y[mode] }, retries = 3, read = false } }
    end,
    same = function() return true end,
    answer = function(vb)
      return { said = MODES[mode] .. " chosen", focus = string.format("macro-%d-slot-%d-menu", shown(vb, nil, known), i) }
    end,
  }
end

-- The button menu's items, as the page's menu has them.
local BUTTON_ITEMS = { special = "Macro Button Special Switch", trigger = "Trigger Mode" }

function A.macro_button_menu_item(args, label, known)
  local i, item = whole(args.index), args.item
  if not (i and i >= 1 and i <= 2) then return nil, "no macro button was named" end
  if not BUTTON_ITEMS[item] then return nil, tostring(item) .. " is no item of a macro button's menu" end
  return {
    label = label,
    part = "macros",
    macroCount = known and known.macroCount,
    decide = function()
      return { at = { M.BUTTON_X[i], M.BUTTON_Y },
        menu = { item = { M.BUTTON_MENU_X[i], M.BUTTON_MENU_Y[item] }, retries = 0 } }
    end,
    -- Chosen at the item's click, which Avenger acts on after it: both buttons' change waited for
    -- from the picture before the right click, and read once they changed or at the bound. Trigger
    -- Mode may show nothing at all; the read at the bound is then what the page gets.
    watch = function() return { M.BUTTON_BOUNDS[1], M.BUTTON_BOUNDS[2] } end,
    same = function(vb, va)
      local a, b = va.macroButtons, vb.macroButtons
      return a ~= nil and b ~= nil and a[1] == b[1] and a[2] == b[2]
    end,
    answer = function(vb, va)
      return { said = BUTTON_ITEMS[item] .. " chosen",
        focus = string.format("macro-%d-slot-%d-menu", shown(vb, va, known), i + 3) }
    end,
  }
end

function A.reset_macro_amount(args, label, known)
  local i = whole(args.index)
  if not (i and i >= 1 and i <= 3) then return nil, "no macro knob was named" end
  local p = { M.FIELDS["macro" .. i][1], M.AMOUNT_Y }
  return {
    label = label,
    part = "macros",
    macroCount = known and known.macroCount,
    decide = function() return { at = p, double = true } end,
    -- The knob's picture (Knob.BOX): the reader looks within 28 of the project's pixels (28 points
    -- at 50 %).
    watch = function() return { box(p, Knob.BOX.macro) } end,
    same = function(vb, va)
      local a = va.macroAmounts and va.macroAmounts[i]
      return a ~= nil and a == (vb.macroAmounts and vb.macroAmounts[i])
    end,
    -- Said against the knob's default, the tool's (M.AMOUNT_DEFAULT; Act.resetAnswer).
    answer = function(vb, va)
      local function amount(v) return v and v.macroAmounts and v.macroAmounts[i] end
      return Act.resetAnswer(label, nameOf(i, vb, known, "macro knob " .. i), function(a) return a .. " percent" end,
        amount(vb), amount(va), M.AMOUNT_DEFAULT, string.format("macro-%d-slot-%d-reset-amount", shown(vb, va, known), i))
    end,
  }
end

return A
