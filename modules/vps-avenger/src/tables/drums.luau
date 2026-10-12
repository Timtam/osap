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

-- The drumkit's Back and Next.
T.KIT_BACK = { 617, 225 }
T.KIT_NEXT = { 623, 225 }

-- The twelve slots, from y 49 down in steps of 15.5; the slot's name is clicked at x 282.
T.PART_X = 282
T.PART_FIRST_Y = 49
T.PART_Y_STEP = 15.5

-- A slot's menu opens at x 323; its items lie these far below the slot's row, and a submenu's
-- targets at x 393, 11.5 apart.
T.MENU_X = 323
T.SUBMENU_X = 393
T.SUBMENU_Y_STEP = 11.5
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

-- Target `target` of item `item`'s submenu (drumkit_submenu_item_point), or nil.
function T.submenuItemPoint(index, item, target)
  local p = T.menuItemPoint(index, item)
  if not p then return nil end
  return { T.SUBMENU_X, p[2] + (slot(target) - 1) * T.SUBMENU_Y_STEP }
end

return T
