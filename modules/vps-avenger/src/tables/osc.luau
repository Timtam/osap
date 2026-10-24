-- VPS Avenger's oscillator page: the knobs of the selected OSC tab's main controls, and its
-- 2-band filter.
--
-- SOURCE: the avenger_control project's clicker/header_map.py, taken over with its developer's
-- permission (NOTICE): at 50 % of Avenger's zoom, in points from the top-left corner of Avenger's
-- own interface; the numbers as the project wrote them, starting values nobody here has checked
-- (geometry.luau, "Where the numbers come from").
--
-- The detection numbers below are in the PROJECT'S PIXELS: its captures were one pixel per point
-- (read/picture.luau, "The project's pixels"), at the zoom of the moment. The knob reader converts
-- them; they are kept here as the project has them.

local T = {}

-- Three knobs down one column, at x 346: Level (volume, and pan with the right button) at y 68,
-- Transpose at y 114, Fine Tune (and random or alternate pitch with the right button) at y 160.
T.CONTROL_X = 346
T.CONTROL_Y = 68
T.CONTROLS = {
  volume = { 346, 68 },
  pan = { 346, 68 },
  transpose = { 346, 114 },
  fine_tune = { 346, 160 },
  random_pitch = { 346, 160 },
}
-- The controls pressed with the right button.
T.RIGHT_BUTTON = { pan = true, random_pitch = true }
-- How far an amount drag goes either way, in points at 50 %.
T.LIMIT = 106

-- The 2-band filter: Low-Pass at (508, 62), High-Pass at (369, 62), each moved along x from 368
-- to 509.
T.FILTERS = {
  low_pass = { 508, 62 },
  high_pass = { 369, 62 },
}
T.FILTER_X_MIN = 368
T.FILTER_X_MAX = 509

-- The knob reader's ring for these knobs, in the project's pixels at 50 %: looked for within 18
-- of the knob's middle, between 1.5 and 16 from it.
T.DETECT_RADIUS = 18
T.DETECT_MIN_R = 1.5
T.DETECT_MAX_R = 16

-- What a reading is rounded to: Volume to 10 %, Pan to 25 %, Transpose to 12 semitones, Fine Tune
-- to 25 cents.
T.VOLUME_STEP = 10
T.PAN_STEP = 25
T.TRANSPOSE_STEP = 12
T.FINE_TUNE_STEP = 25

-- Pan, and random or alternate pitch, are drawn as extra bright pixels beside the pointer: about
-- 12.5 of the project's pixels at 50 % for none, and 12.5 more for the whole way.
T.PAN_BASE_PIXELS = 12.5
T.PAN_FULL_PIXEL_DELTA = 12.5

return T
