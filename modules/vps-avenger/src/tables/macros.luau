-- VPS Avenger's macro panel, in its lower right corner: up to four macro tabs, and on the tab
-- shown three knobs and two buttons, each with a name field above it.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, taken over with its developer's
-- permission (NOTICE): at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's
-- own interface; the numbers as the project wrote them, which it worked out from a screenshot at
-- 100 % — starting values nobody here has checked (geometry.luau, "Where the numbers come from").
-- The detection numbers are in the project's pixels (tables/osc.luau says what those are).
--
-- Two kinds of rectangles here are this module's own, worked out from the project's points, since
-- the project never read them: the name fields (FIELD_NAMES) and the tabs (TAB_SPANS). Both are
-- marked in the calibration pictures, which settle them.

local T = {}

-- The tabs, clicked at y 436: tab 1 at x 724 to tab 4 at 849. "+" (a new tab) sits right of the
-- last tab, where the count puts it; the last tab is deleted at y 484.
T.TAB_Y = 436
T.DELETE_Y = 484
T.TAB_X = { 724, 766, 807, 849 }
T.PLUS_X_BY_TAB_COUNT = { 752, 793, 834 }

-- The tabs' captions are read from (710, 429) to (870, 443).
T.TAB_TEXT = { 710, 429, 870, 443 }

-- The name fields, double-clicked to rename: the three knobs' at y 449, the buttons' at y 511.
T.FIELDS = {
  macro1 = { 736, 449 },
  macro2 = { 787, 449 },
  macro3 = { 838, 449 },
  btn1 = { 761, 511 },
  btn2 = { 813, 511 },
}
T.FIELD_ORDER = { "macro1", "macro2", "macro3", "btn1", "btn2" }

-- The knobs' amounts: dragged at y 480, at most 106 either way; read in a ring 35 round the knob's
-- middle, between 8 and 28 from it, and rounded to 10 %.
T.AMOUNT_Y = 480
T.AMOUNT_LIMIT = 106
T.AMOUNT_DETECT_RADIUS = 35
T.AMOUNT_DETECT_MIN_R = 8
T.AMOUNT_DETECT_MAX_R = 28
T.AMOUNT_DETECT_STEP = 10

-- The two buttons: clicked at y 523, x 761 and 813, and read over the whole button.
T.BUTTON_Y = 523
T.BUTTON_X = { 761, 813 }
T.BUTTON_BOUNDS = {
  { 736, 517, 786, 530 },
  { 787, 517, 838, 530 },
}
T.BUTTON_STATE_Y = 523.5
T.BUTTON_STATE_X = { 761, 812.5 }
T.BUTTON_STATE = { 13, 8 }

-- A knob's reset modes, chosen in its menu, and a button's menu items.
T.RESET_MODE_X = { 741, 792, 842 }
T.RESET_MODE_Y = { normal = 458, snap = 470 }
T.BUTTON_MENU_X = { 787, 820 }
T.BUTTON_MENU_Y = { special = 540, trigger = 551 }

-- ── This module's own ────────────────────────────────────────────────────────

-- A name field as a rectangle read for text: 25 either side of its point, about half the 51
-- between two knobs, and 6 above and below it; the buttons' as wide as the button below them.
T.FIELD_NAMES = {
  macro1 = { 711, 443, 761, 455 },
  macro2 = { 762, 443, 812, 455 },
  macro3 = { 813, 443, 863, 455 },
  btn1 = { 736, 505, 786, 517 },
  btn2 = { 787, 505, 838, 517 },
}

-- Tab i as a rectangle, for which one is selected: halfway to its neighbours' points, the first
-- and last as wide as the one beside them, over the captions' rows.
T.TAB_SPANS = {
  { 703, 429, 745, 443 },
  { 745, 429, 786.5, 443 },
  { 786.5, 429, 828, 443 },
  { 828, 429, 870, 443 },
}

return T
