-- VPS Avenger's Drums tab: its pattern switch, its two modes and the lock between them, the
-- drumkit's Back and Next, and the drumkit table of twelve slots.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, taken over with its developer's
-- permission (NOTICE): at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's
-- own interface; the numbers as the project wrote them, starting values nobody here has checked
-- (geometry.luau, "Where the numbers come from"). One name is spelt as Avenger and this module's
-- page spell it, "Snare 2", where the project wrote "Snare2".

local T = {}

-- The pattern switch at (560, 62), read in a box 10 by 10.
T.PATTERN = { 560, 62 }
T.PATTERN_STATE = { 10, 10 }

-- DRUMKIT and SEQUENCE, the two modes, with LOCK between them, at y 135. The mode chosen is read
-- from the two captions' brightness, from y 132 to 138: DRUMKIT's from x 556 to 588, SEQUENCE's
-- from 595 to 627. LOCK is read in a box 10 by 10 for its green light.
T.MODE_DRUM = { 574, 135 }
T.MODE_LOCK = { 592, 135 }
T.MODE_SQ = { 613, 135 }
T.LOCK_STATE = { 10, 10 }
T.MODE_SELECTION_TOP = 132
T.MODE_SELECTION_BOTTOM = 138
T.MODE_DRUM_SELECTION = { 556, 588 }
T.MODE_SQ_SELECTION = { 595, 627 }

-- The drumkit's Back and Next, ◀ and ▶: the project's (617, 225) and (623, 225) put Back on ◀'s
-- flat edge, half a point from the gap between them; here the middles of both, measured on
-- `Drum SQ/Avenger 100% Pattern On.png` — ◀ from 614 to 617.5, ▶ from 621.5 to 625, both from 222
-- to 226.5.
T.KIT_BACK = { 616, 224.5 }
T.KIT_NEXT = { 623, 224.5 }

-- The twelve slots, from y 49 down in steps of 15.5; the slot's name is clicked at x 282.
T.PART_X = 282
T.PART_FIRST_Y = 49
T.PART_Y_STEP = 15.5

-- A slot's menu opens at x 323; its items lie these far below the slot's row, and a submenu's
-- targets at x 393, 11.5 apart.
T.MENU_X = 323
T.SUBMENU_X = 393
T.SUBMENU_Y_STEP = 11.5
-- The project clicks target t at its item's row plus (t − 1) × 11.5: right only for a submenu that
-- opens DOWNWARD, its first row beside its item. JUCE puts a submenu's top at its item's only for an
-- item in the upper half of the plug-in; in the lower half it puts the submenu's BOTTOM at the
-- item's, so the submenu opens upward, and it pushes it inside at the edges (juce_PopupMenu.cpp,
-- calculateWindowPos, read in its source) — on a Mac, where Avenger paints its popups inside
-- itself, Copy Slot to on slot 10 opens upward. So a target is placed from the submenu's own top:
-- its first row's middle this far below it, the project's place where the submenu opens downward —
-- its top at its item's top, less JUCE's border, taken as nothing here: half a row above the item's
-- middle. A starting value, which the submenu's calibration picture settles (`-submenu-item.png`).
T.SUBMENU_FIRST_ROW = T.SUBMENU_Y_STEP / 2
T.MENU_Y_OFFSET = {
  lock_all = 6,
  unlock_all = 18,
  save_drumkit_as = 43,
  copy_sample_to = 66,
  swap_sample_with = 78,
  copy_slot_to = 100,
  swap_slot_with = 112,
  remove_sample = 126,
}

-- The columns of a slot's row.
T.COLUMN_X = {
  slot = 282,
  menu = 282,
  loop = 325,
  browse = 345,
  edit_sample = 370,
  back = 389,
  next = 401,
  lock = 417,
  mute = 429,
  solo = 441,
  volume = 458,
  pan = 488,
  pitch = 518,
  routing = 595,
}

-- ── Where a slot's state shows (this module's, worked out from the points above) ──
-- STARTING VALUES worked out from the project's points, as popups.luau's regions are — nobody has
-- seen them on an Avenger; the page's reads log what they find and a calibrating run pictures it
-- (read/open.luau, `drums-<zoom>pct.png`).
-- A slot's Mute, Solo, Lock and Loop are read in a box 13 by 8 round their point: the project's
-- OSC_BUTTON_STATE, which its detect_drumkit_button_state reads a slot's button in.
T.BUTTON_STATE = { 13, 8 }
-- Half a slot's row, either way of its point.
T.SLOT_HALF = T.PART_Y_STEP / 2
-- The name column: the slots' name cells, right of the piano keys the rows are labelled with (B, A#,
-- … C, from 243.5 to 269), to just before Loop's box (325 − 13/2). Measured on
-- `Drum SQ/Avenger 100% Pattern On.png`: the keys end at 269, the cells start at 269.5 and the names
-- at 273.5. From 248 the selection's background measured the keys — the black ones dark, so five
-- rows stood out alike — and the names' text read took the black keys' "#" (the second Mac test of
-- the page, 2026-10-24).
T.NAME_COLUMN = { 271, 318 }
-- The rows of a slot's band above and below its name, its background only: from 4.5 to 7.75 points
-- from the row's middle, each way.
T.NAME_ROWS = { 4.5, 7.75 }
-- The table from the first row's top to the last row's bottom.
T.TABLE_TOP = T.PART_FIRST_Y - T.SLOT_HALF
T.TABLE_BOTTOM = T.PART_FIRST_Y + 11 * T.PART_Y_STEP + T.SLOT_HALF

-- The items of a slot's menu in its order, with the page's words for them (ui/panel.js); the four
-- that open a submenu of the twelve slots.
T.MENU_ITEMS = {
  { "lock_all", "Lock All" },
  { "unlock_all", "Unlock All" },
  { "save_drumkit_as", "Save Drumkit as" },
  { "copy_sample_to", "Copy Sample to" },
  { "swap_sample_with", "Swap Sample with" },
  { "copy_slot_to", "Copy Slot to" },
  { "swap_slot_with", "Swap Slot with" },
  { "remove_sample", "Remove Sample" },
}
T.SUBMENU_ITEMS = { copy_sample_to = true, swap_sample_with = true, copy_slot_to = true, swap_slot_with = true }

-- The slots' names in Avenger's factory drumkit, top to bottom.
T.PART_LABELS = {
  "Crash",
  "Open Hi-Hat",
  "Ride",
  "Closed Hi-Hat 2",
  "Percussion 2",
  "Closed Hi-Hat 1",
  "Percussion 1",
  "Snare 2",
  "Clap",
  "Snare 1",
  "Rimshot",
  "Bassdrum",
}

local function slot(index) return math.clamp(math.floor(index), 1, #T.PART_LABELS) end

-- Slot `index`'s name (the project's drumkit_part_point).
function T.partPoint(index)
  return { T.PART_X, T.PART_FIRST_Y + (slot(index) - 1) * T.PART_Y_STEP }
end

-- Column `column` of slot `index`'s row (drumkit_column_point), or nil for a column the table
-- does not have.
function T.columnPoint(index, column)
  local x = T.COLUMN_X[column]
  if not x then return nil end
  return { x, T.PART_FIRST_Y + (slot(index) - 1) * T.PART_Y_STEP }
end

-- Item `item` of slot `index`'s menu (drumkit_menu_item_point), or nil.
function T.menuItemPoint(index, item)
  local dy = T.MENU_Y_OFFSET[item]
  if not dy then return nil end
  return { T.MENU_X, T.PART_FIRST_Y + (slot(index) - 1) * T.PART_Y_STEP + dy }
end

-- Target `target` of a slot menu's submenu: { x from Avenger's corner, y below the submenu's own
-- top } — the project's drumkit_submenu_item_point, counted from the submenu rather than from its
-- item (above).
function T.submenuTarget(target)
  return { T.SUBMENU_X, T.SUBMENU_FIRST_ROW + (slot(target) - 1) * T.SUBMENU_Y_STEP }
end

-- The middle of slot `index`'s row.
function T.rowY(index)
  return T.PART_FIRST_Y + (slot(index) - 1) * T.PART_Y_STEP
end

-- Slot `index`'s name as a rectangle: the name column, 7 above and below the row's middle — the
-- text read, and Browse's watched box.
function T.nameRect(index)
  local y = T.rowY(index)
  return { T.NAME_COLUMN[1], y - 7, T.NAME_COLUMN[2], y + 7 }
end

-- Slot `index`'s whole row, from the name column to Pitch's knob.
function T.rowRect(index)
  local y = T.rowY(index)
  return { T.NAME_COLUMN[1], y - T.SLOT_HALF, T.COLUMN_X.pitch + 8, y + T.SLOT_HALF }
end

-- A drum knob's box, the knob's point ± (8, 7.75): a calibrating run's picture of it, and a
-- reset's watched box.
function T.knobBox(index, control)
  local p = T.columnPoint(index, control)
  if not p then return nil end
  return { p[1] - 8, p[2] - T.SLOT_HALF, p[1] + 8, p[2] + T.SLOT_HALF }
end

-- A column's box down the whole table, `w` across round its x.
function T.columnRect(column, w)
  local x = T.COLUMN_X[column]
  return { x - w / 2, T.TABLE_TOP, x + w / 2, T.TABLE_BOTTOM }
end

return T
