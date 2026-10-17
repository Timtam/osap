-- VPS Avenger's central area: the central tabs, the sample editor and its browser (EDITOR with
-- the Drums tab), and DRUM SQ.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, and the sizes its clicker reads
-- the central tabs in (clicker/__init__.py), taken over with its developer's permission (NOTICE):
-- at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's own interface; the
-- numbers as the project wrote them, starting values nobody here has checked (geometry.luau,
-- "Where the numbers come from").

local T = {}

-- ── The central tabs ─────────────────────────────────────────────────────────
-- The four the project knows, at y 244; each is read in a box 36 by 10 round its point. Avenger
-- has nine more in the same row (the page's FX to SHAPER 1), which the project has no points for.
T.TABS = {
  editor = { 262, 244 },
  arp = { 300, 244 },
  drum_sq = { 338, 244 },
  step_sq = { 376, 244 },
}
T.TAB_ORDER = { "editor", "arp", "drum_sq", "step_sq" }
T.TAB_SIZE = { 36, 10 }

-- ── The sample editor ────────────────────────────────────────────────────────
-- Its eight knobs, at y 407.
T.EDITOR_CONTROLS = {
  volume = { 307, 407 },
  pitch = { 349, 407 },
  pan = { 393, 407 },
  spike = { 436, 407 },
  delay = { 479, 407 },
  lowpass = { 522, 407 },
  highpass = { 565, 407 },
  rate = { 607, 407 },
}
-- Its tabs, at y 260.
T.EDITOR_TABS = {
  sample = { 264, 260 },
  amp = { 300, 260 },
  pitch = { 336, 260 },
  eq = { 609, 260 },
}
-- The envelope's buttons and fields, and the EQ's.
T.PANEL = {
  menu = { 501, 261 },
  preset = { 539, 261 },
  back = { 579, 261 },
  next = { 585, 261 },
  enable = { 267, 371 },
  eq_enable = { 258, 370 },
  grid = { 321, 372 },
  speed = { 392, 372 },
  strength = { 559, 372 },
  range = { 609, 372 },
  eq1_freq = { 299, 370 },
  eq1_gain = { 344, 370 },
  eq1_q = { 385, 370 },
  eq1_type = { 429, 370 },
  eq2_freq = { 482, 370 },
  eq2_gain = { 527, 370 },
  eq2_q = { 568, 370 },
  eq2_type = { 612, 370 },
  start = { 303, 372 },
  ["end"] = { 357, 372 },
  gain = { 609, 372 },
}
-- Where the envelope's and the EQ's switch shows whether it is on: a box 5 by 5 there.
T.ENABLE_STATE = { 253.5, 371.5 }
T.EQ_ENABLE_STATE = { 253.5, 369.5 }
T.ENABLE_STATE_CENTER_ADJUST = { 0, 0 }
T.ENABLE_STATE_SIZE = { 5, 5 }
T.EQ_ENABLE_STATE_SIZE = { 5, 5 }
-- Play Mode and its menu.
T.PLAY_MODE = { 542, 372 }
T.PLAY_MODE_MENU_X = 554
T.PLAY_MODE_MENU_Y = { monophone = 382, polyphone = 393 }
-- Grid's menu.
T.GRID_MENU_X = 332
T.GRID_MENU_Y = {
  ["1_16"] = 382,
  ["1_8"] = 393.5,
  ["1_4"] = 405,
  ["1_16t"] = 416.5,
  ["1_8t"] = 428,
  ["1_4t"] = 439.5,
}
-- An EQ band's type menu: band 1's at x 442, band 2's at 625.
T.EQ_TYPE_MENU_X = { 442, 625 }
T.EQ_TYPE_MENU_Y = {
  peak = 380,
  low_shelf = 391.5,
  high_shelf = 403,
  low_pass = 414.5,
  high_pass = 426,
}
-- Loop-Stretch and its menu.
T.LOOP_STRETCH = { 262, 398 }
T.LOOP_STRETCH_MENU_X = 322
T.LOOP_STRETCH_MENU_Y = { off = 407, turntable = 419, stretch = 430 }
-- The sample browser's thirteen rows, from y 277 down in steps of 10.
T.BROWSER_ITEM_X = 294
T.BROWSER_ITEM_FIRST_Y = 277
T.BROWSER_ITEM_Y_STEP = 10
T.BROWSER_ITEM_COUNT = 13

-- Row `index` of the sample browser (the project's sample_browser_item_point).
function T.browserItemPoint(index)
  local i = math.clamp(math.floor(index), 1, T.BROWSER_ITEM_COUNT)
  return { T.BROWSER_ITEM_X, T.BROWSER_ITEM_FIRST_Y + (i - 1) * T.BROWSER_ITEM_Y_STEP }
end

-- ── Where the sample editor's and DRUM SQ's values are read ───────────────────
-- Worked out from the project's points, as popups.luau's regions are: STARTING VALUES nobody has
-- seen, which the calibration pictures `sample-<zoom>pct.png` and `sq-controls-<zoom>pct.png`
-- settle (TODO.md, D6 and D7). The project read none of these: it clicked the editor's tab, its
-- switches, its menus and DRUM SQ's fields and took what it clicked for what was there.
--
-- The editor's four tabs, each read in a box TAB_SIZE round its point, as the central tabs are:
-- from SAMPLE's left to EQ's right.
T.EDITOR_TAB_ORDER = { "sample", "amp", "pitch", "eq" }
T.EDITOR_TABS_ROW = { 246, 255, 627, 265 }
-- Both switches' state boxes, ENABLE_STATE_SIZE round their points: one grid for the two.
T.SWITCHES = { 251, 367, 256, 374 }
-- A menu's field, which shows the value it was set to: a box VALUE_BOX round its opener, the
-- tool's point; Loop-Stretch's cut on the left at the panel's edge, 244. Read as text, its name in
-- the text read the key's with spaces ("play mode", "sq length").
T.VALUE_BOX = { 40, 10 }
T.VALUES = {
  play_mode = T.PLAY_MODE,
  grid = { 321, 372 },
  eq1_type = { 429, 370 },
  eq2_type = { 612, 370 },
  loop_stretch = T.LOOP_STRETCH,
  sq_length = { 491, 416 },
  sq_speed = { 593, 416 },
}
-- A typed field (PANEL's start to eq2_q): a box FIELD_BOX round its point, the picture a rename's
-- flow sees Avenger's editor open in; the EQ's lie 41 to 45 apart.
T.FIELD_BOX = { 36, 10 }
-- The envelope's Preset field, between Menu (501) and Back (579), where its preset's name would be
-- drawn; the envelope above the fields' row; DRUM SQ's steps between its patterns and its knobs;
-- the sample browser's first three rows (277, 287, 297).
T.ENVELOPE_NAME = { 514, 256, 564, 266 }
T.ENVELOPE_AREA = { 246, 266, 627, 364 }
T.SQ_STEPS = { 246, 266, 627, 404 }
T.BROWSER_HEAD = { 252, 272, 338, 302 }
-- What the sample browser's rows say: the page's thirteen (ui/panel.js), its vocabulary.
T.BROWSER_WORDS = { "Crash", "Open Hi-Hat", "Ride", "Closed Hi-Hat 2", "Percussion 2", "Closed Hi-Hat 1",
  "Percussion 1", "Snare 2", "Clap", "Snare 1", "Rimshot", "Bassdrum", "Drum Loops" }
-- What the menus' fields may read, by the page's keys: its words for them.
T.VALUE_WORDS = {
  play_mode = { monophone = "Monophone", polyphone = "Polyphone" },
  grid = { ["1_16"] = "1/16", ["1_8"] = "1/8", ["1_4"] = "1/4", ["1_16t"] = "1/16T", ["1_8t"] = "1/8T",
    ["1_4t"] = "1/4T" },
  eq_type = { peak = "Peak", low_shelf = "Low-Shelf", high_shelf = "High-Shelf", low_pass = "Low-Pass",
    high_pass = "High-Pass" },
  loop_stretch = { off = "Off", turntable = "Turntable", stretch = "Stretch" },
  sq_speed = { ["1_16"] = "1/16", ["1_8"] = "1/8", ["1_16t"] = "1/16T", ["1_8t"] = "1/8T", ["1_32"] = "1/32",
    ["1_4"] = "1/4", ["1_32t"] = "1/32T", ["1_4t"] = "1/4T" },
}
T.VALUE_WORDS.eq1_type, T.VALUE_WORDS.eq2_type = T.VALUE_WORDS.eq_type, T.VALUE_WORDS.eq_type
-- A knob's box, for its reset's change wait and a calibrating run's pictures.
T.EDITOR_KNOB_BOX = { 20, 20 }
T.SQ_KNOB_BOX = { 20, 20 }
-- LENGTH: the project's points per four steps when it drags it, and what it takes its double-click
-- to set — 64, from which it drags down (its clicker).
T.SQ_LENGTH_STEP = 6.6
T.SQ_LENGTH_RESET = 64

local function boxAt(p, size)
  return { p[1] - size[1] / 2, p[2] - size[2] / 2, p[1] + size[1] / 2, p[2] + size[2] / 2 }
end

-- A menu's field, by its key in T.VALUES.
function T.valueRect(key)
  local r = boxAt(T.VALUES[key], T.VALUE_BOX)
  if key == "loop_stretch" then r[1] = math.max(r[1], 244) end
  return r
end

-- A typed field's box, by its key in T.PANEL.
function T.fieldRect(key)
  return boxAt(T.PANEL[key], T.FIELD_BOX)
end

function T.editorKnobBox(c)
  return boxAt(T.EDITOR_CONTROLS[c], T.EDITOR_KNOB_BOX)
end

function T.sqKnobBox(c)
  return boxAt(T.SQ_CONTROLS[c], T.SQ_KNOB_BOX)
end

-- ── DRUM SQ ──────────────────────────────────────────────────────────────────
-- VEL AMOUNT and SHUFFLE, dragged and reset at y 416; LENGTH and SPEED beside them.
T.SQ_CONTROLS = {
  vel_amount = { 443, 416 },
  shuffle = { 538, 416 },
}
T.SQ_LENGTH = { 491, 416 }
T.SQ_SPEED = { 593, 416 }
-- The SEQUENCE MENU and its items.
T.SQ_SEQUENCE_MENU = { 245, 256 }
T.SQ_SEQUENCE_MENU_X = 275
T.SQ_SEQUENCE_MENU_Y = { save_sequence_as = 275, import_midi = 289, export_to_midi = 300 }
-- PATTERN CHAIN and AUTO FOLLOW, at y 259, read in boxes 14 by 8 and 11 by 8 (moved down by 0).
T.SQ_PATTERN_PLAYER = { 343, 259 }
T.SQ_PATTERN_PLAYER_STATE = { 14, 8 }
T.SQ_PATTERN_PLAYER_STATE_Y_ADJUST = 0
T.SQ_AUTO_FOLLOW = { 356, 259 }
T.SQ_AUTO_FOLLOW_STATE = { 11, 8 }
T.SQ_AUTO_FOLLOW_STATE_Y_ADJUST = 0
-- The patterns A to D at y 259: whether one is on, read in a box 30 by 8; which one is selected,
-- in a box 30 by 9.
T.SQ_PATTERNS = {
  a = { 377, 259 },
  b = { 407, 259 },
  c = { 437, 259 },
  d = { 467, 259 },
}
T.SQ_PATTERN_ORDER = { "a", "b", "c", "d" }
T.SQ_PATTERN_STATE = { 30, 8 }
T.SQ_PATTERN_STATE_Y_ADJUST = 0
T.SQ_PATTERN_SELECTED = { 30, 9 }
T.SQ_PATTERN_SELECTED_Y_ADJUST = 0
-- The pattern menu and its items, and SPEED's menu.
T.SQ_PATTERN_MENU = { 489, 259 }
T.SQ_PATTERN_MENU_X = 489
T.SQ_PATTERN_MENU_Y = {
  copy = 269,
  paste = 280.5,
  copy_first_half = 292,
  copy_second_half = 303.5,
  paste_to_first_half = 315,
  paste_to_second_half = 326.5,
  clear = 338,
}
T.SQ_SPEED_MENU_X = 601
T.SQ_SPEED_MENU_Y = {
  ["1_16"] = 426,
  ["1_8"] = 437.5,
  ["1_16t"] = 449,
  ["1_8t"] = 460.5,
  ["1_32"] = 472,
  ["1_4"] = 483.5,
  ["1_32t"] = 495,
  ["1_4t"] = 506.5,
}

return T
