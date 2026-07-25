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
      -- The five mic on/off toggles (Spot 1/2, Main, Room, Mix): each clicks its power (⏻)
      -- icon to toggle the mic and reads on/off from a SINGLE pixel on that icon (ReaHotkey's
      -- HotspotToggleButton — the CSS Brass overlay uses the same pixel-toggle). So a screen-
      -- reader user hears every mic's state, not just Mix.
      --
      -- Coordinates are relative to the "CINEMATIC STUDIO STRINGS" LANDMARK's top-left (the
      -- overlay is landmark-anchored — see O:landmark / originOf), so they hold regardless of
      -- host, Kontakt browser state, window size or version: the wordmark and the mixer move
      -- together. Measured against the live CSS 8.11 UI: the ⏻ icons sit at landmark x
      -- {-138,-98,-58,-18,22} (a 40 px pitch), and the discriminating pixel is on the ring at
      -- y = +366. Lit, the ⏻ is CSS's accent lilac (≈180,165,230); unlit it is dim grey
      -- (≈78,79,85); nearest wins.
      local ON, OFF = { 180, 165, 230 }, { 78, 79, 85 }
      ov:addHotspotToggle({ label = "Spot 1 Mic",   at = { -138, 366 }, onColor = ON, offColor = OFF })
      ov:addHotspotToggle({ label = "Spot 2 Mic",   at = { -98, 366 }, onColor = ON, offColor = OFF })
      ov:addHotspotToggle({ label = "Main Mic",     at = { -58, 366 }, onColor = ON, offColor = OFF })
      ov:addHotspotToggle({ label = "Room Mic",     at = { -18, 366 }, onColor = ON, offColor = OFF })
      ov:addHotspotToggle({ label = "Mix Position", at = { 22, 366 }, onColor = ON, offColor = OFF })
    end)
  end,
}
