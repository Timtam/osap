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
  activate = function()
    kontakt.library({
      name = "Cinematic Studio Strings",
      vendor = "Cinematic Studio Series",
      image = img("images/CinematicStudioStrings/Product.png"),
      controls = {
        { kind = "static", label = "Cinematic Studio Strings" },
        { kind = "hotspot", label = "Spot 1 Mic", at = { 35, 481 } },
        { kind = "hotspot", label = "Spot 2 Mic", at = { 75, 481 } },
        { kind = "hotspot", label = "Main Mic", at = { 115, 481 } },
        { kind = "hotspot", label = "Room Mic", at = { 155, 481 } },
        {
          kind = "gtoggle",
          label = "Mix Position",
          region = { 188, 473, 220, 489 },
          onImage = img("images/CinematicStudioStrings/MixOn.png"),
          offImage = img("images/CinematicStudioStrings/MixOff.png"),
        },
      },
    })
  end,
}
