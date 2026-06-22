-- Cinematic Studio Series — Kontakt sample-library overlay, as DATA only (no Lua
-- crosses module VMs). The kontakt container detects the loaded library by its
-- landmark image and renders these controls. Coordinates are authored in the
-- Kontakt 7 frame; the container applies its per-host offset. Ported from
-- ReaHotkey's CinematicStudioSeries overlay. Depends on com.platform.kontakt.

local function img(rel) return host.path(rel) end

return {
  provides = "kontakt.library",
  libraries = {
    {
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
    },
  },
}
