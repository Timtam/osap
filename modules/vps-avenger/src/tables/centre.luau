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
