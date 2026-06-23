-- Cinematic Studio Strings — a Kontakt sample-library overlay that INHERITS the
-- Kontakt module. It depends on com.platform.kontakt (so Kontakt's framework is
-- evaluated in this VM) and registers an inheriting overlay that takes over from
-- Kontakt's base whenever this library's landmark is on screen. Controls are
-- authored in the Kontakt 7 frame; Kontakt applies the version offset. Ported
-- from ReaHotkey's CinematicStudioSeries overlay.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

return {
  -- Standalone side effect (this module's own VM): register the library overlay.
  -- Controls are built functionally on top of Kontakt's inherited header.
  activate = function()
    kontakt.library("Cinematic Studio Strings", img("images/CinematicStudioStrings/Product.png"), function(ov)
      ov:addStaticText("Cinematic Studio Strings")
      ov:addHotspotButton({ label = "Spot 1 Mic", at = { 35, 481 } })
      ov:addHotspotButton({ label = "Spot 2 Mic", at = { 75, 481 } })
      ov:addHotspotButton({ label = "Main Mic", at = { 115, 481 } })
      ov:addHotspotButton({ label = "Room Mic", at = { 155, 481 } })
      ov:addGraphicalToggle({
        label = "Mix Position",
        region = { 188, 473, 220, 489 },
        onImage = img("images/CinematicStudioStrings/MixOn.png"),
        offImage = img("images/CinematicStudioStrings/MixOff.png"),
      })
    end)
  end,
}
