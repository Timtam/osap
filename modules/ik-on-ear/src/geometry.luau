-- Where a coordinate written in this module actually lands.


-- == The parts of this window that are only a picture =========================
--
-- Everything above this line came out of the accessibility tree. Everything below it is drawn
-- by JUCE and reaches the tree as, at most, a label sitting next to the thing it names - the
-- switches, the seven assignment buttons and the curve check-marks are not in it at all.
--
-- Which raises the question the tester answered by dragging the window about: what happens to
-- a coordinate when the window is resized. Measured, from two probes of the same window at
-- 1920x1009 and at 826x481, ON:EAR scales its whole interface **uniformly by the height** and
-- **centres it horizontally**. Not a reflow, and not two independent axes. The model
--
--     scale = content height / 1009
--     x     = content width / 2 + scale * (design x - 960)
--     y     = scale * design y
--
-- predicts 43 of the 47 elements the two probes share to within one and a half pixels. The
-- four it misses are the title bar, the system menu, and the minimise and close buttons -
-- drawn by Windows rather than by JUCE, and so rightly not scaling with the rest.
--
-- So nothing below is a pixel. Every number is a coordinate in the frame it was measured in,
-- resolved against whatever size the window happens to be now. A fixed offset from a nearby
-- label would have been no better, because the offset scales too.
local DESIGN_W, DESIGN_H = 1920, 1009

local function scaleOf(o)
  if not (o and o.client and o.client.h and o.client.h > 0) then return nil end
  return o.client.h / DESIGN_H
end

local function scaled(dx, dy)
  return function(self)
    local o = self:origin()
    local k = scaleOf(o)
    if not k then return nil end
    return {
      math.floor(o.client.w / 2 + k * (dx - DESIGN_W / 2) + 0.5),
      math.floor(k * dy + 0.5),
    }
  end
end

-- The same model, in screen coordinates, for the one thing here that has to be read rather
-- than pressed. An OCR region cannot be a function in the runtime, so this control does its
-- own reading instead of pretending its region is fixed.
local function screenRect(o, x1, y1, x2, y2)
  local k = scaleOf(o)
  if not k then return nil end
  local cx, cy = o.client.x, o.client.y
  local half = o.client.w / 2
  return {
    math.floor(cx + half + k * (x1 - DESIGN_W / 2) + 0.5), math.floor(cy + k * y1 + 0.5),
    math.floor(cx + half + k * (x2 - DESIGN_W / 2) + 0.5), math.floor(cy + k * y2 + 0.5),
  }
end

-- Absolute screen point for a design coordinate, for a control that does its own clicking.
-- Takes the overlay because two of them share this window now.
local function screenPointOf(overlay, dx, dy)
  local o = overlay:origin()
  local k = scaleOf(o)
  if not k then return nil end
  return math.floor(o.client.x + o.client.w / 2 + k * (dx - DESIGN_W / 2) + 0.5),
    math.floor(o.client.y + k * dy + 0.5)
end

local function screenPoint(dx, dy)
  return screenPointOf(ov, dx, dy)
end

return {
  DESIGN_W = DESIGN_W,
  DESIGN_H = DESIGN_H,
  scaleOf = scaleOf,
  scaled = scaled,
  screenRect = screenRect,
  screenPointOf = screenPointOf,
}
