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
--
-- The overlay runtime carries the model out now (O:scale, 2026-09-28). This module did it by
-- hand for a month — an `at` that was a function per control, and a screen-point helper beside
-- every click its own code made — and it was the one module that did, until VPS Avenger zoomed
-- from 50 to 200 % and made a second. In the runtime's terms the model is: the frame puts the
-- design's centre line at half the content's width (unscaled — that half is the window's, not
-- the design's), and every authored point scales by the height about that centre line,
-- (960, 0). Its arithmetic is this module's old arithmetic, in the same order, so every point
-- lands where it did (crates/host/src/overlay_scale_tests.rs holds the two to the pixel).
local DESIGN_W, DESIGN_H = 1920, 1009

-- The factor, from the content's height; nil while there is no height to read, which the
-- runtime takes as "place nothing".
local function scaleOf(o)
  if not (o and o.client and o.client.h and o.client.h > 0) then return nil end
  return o.client.h / DESIGN_H
end

-- Puts the model on overlay `ov`, and hands it back. Every overlay over this window takes it:
-- the main one and each panel drawn inside the window, because the panels scale with it too.
-- A control whose point is worked out from the accessibility tree rather than from the design
-- — a chooser row's probe — is `rawOrigin`: in the window's own pixels, neither framed nor
-- scaled.
local function apply(ov)
  ov:frame(function(o) return o.client.w / 2, 0 end)
  ov:scale(scaleOf, { about = { DESIGN_W / 2, 0 } })
  return ov
end

return {
  DESIGN_W = DESIGN_W,
  DESIGN_H = DESIGN_H,
  scaleOf = scaleOf,
  apply = apply,
}
