-- Cinematic Studio Strings — a Kontakt sample-library overlay that INHERITS the
-- Kontakt module. It depends on com.platform.kontakt (so Kontakt's framework is
-- evaluated in this VM) and registers an inheriting overlay that takes over from
-- Kontakt's base whenever this library's landmark is on screen.
--
-- FAITHFUL PORT of ReaHotkey's Cinematic Studio Strings overlay
-- (ReaHotkey/Includes/Overlays/KontaktKompleteKontrol/CinematicStudioSeries.ahk):
-- exactly its controls, behaviour-identical — the header, four mic HotspotButtons,
-- and the Mix Position graphical toggle. Nothing else (no articulation grid, reverb,
-- section faders or performance toggles — those were an over-derivation and are gone).
--
-- Coordinates are authored in the Kontakt CONTENT frame; the host applies the per-host
-- content offset (nested in KK: the dynamic KK→content offset). ReaHotkey authors these
-- in its own KK frame (base + KK offset 190,111); the values below are those positions
-- expressed in our content frame. They are PROVISIONAL starting points — to be pixel-
-- verified against the live CSS 8.11 UI (REAPER + KK + Kontakt) and adjusted onto the
-- actual controls, since ReaHotkey's mic-mixer coordinates predate the 8.11 layout.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

return {
  -- Standalone side effect (this module's own VM): register the library overlay.
  activate = function()
    kontakt.library("Cinematic Studio Strings", img("images/CinematicStudioStrings/Product.png"), function(ov)
      ov:addStaticText("Cinematic Studio Strings")
      -- ReaHotkey: four mic HotspotButtons (Spot 1/2, Main, Room). HotspotButton =
      -- click the point + announce "activated" — behaviour-identical to ReaHotkey.
      ov:addHotspotButton({ label = "Spot 1 Mic", at = { 27, 370 } })
      ov:addHotspotButton({ label = "Spot 2 Mic", at = { 67, 370 } })
      ov:addHotspotButton({ label = "Main Mic", at = { 107, 370 } })
      ov:addHotspotButton({ label = "Room Mic", at = { 147, 370 } })
      -- ReaHotkey: the Mix Position graphical toggle (state read by image-matching the
      -- MixOn/MixOff templates over its region; click to toggle). Default match scales
      -- (the templates are ReaHotkey's, not captured this session, so multi-scale helps).
      ov:addGraphicalToggle({
        label = "Mix Position",
        region = { 180, 362, 212, 378 },
        onImage = img("images/CinematicStudioStrings/MixOn.png"),
        offImage = img("images/CinematicStudioStrings/MixOff.png"),
      })
    end)
  end,
}
