-- Cinematic Studio Strings — a Kontakt sample-library overlay that INHERITS the
-- Kontakt module. It depends on com.platform.kontakt (so Kontakt's framework is
-- evaluated in this VM) and registers an inheriting overlay that takes over from
-- Kontakt's base whenever this library's landmark is on screen.
--
-- Ported from ReaHotkey's Cinematic Studio Strings overlay
-- (ReaHotkey/Includes/Overlays/KontaktKompleteKontrol/CinematicStudioSeries.ahk): its
-- mixer channel toggles, nothing else (no articulation grid, reverb, section faders or
-- performance toggles — those were an over-derivation and are gone).
--
-- ReaHotkey's own coordinates predate the Kontakt 8.11 layout and no longer land on
-- anything; the ones below were MEASURED off a full-plugin capture of the live UI, and are
-- relative to this library's landmark rather than to the plugin, so they hold across every
-- host and browser state.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

return {
  -- Standalone side effect (this module's own VM): register the library overlay.
  activate = function()
    kontakt.library("Cinematic Studio Strings", img("images/CinematicStudioStrings/Product.png"), function(ov)
      ov:addStaticText("Cinematic Studio Strings")
      -- The five MIXER channels, each with a power (⏻) icon: clicking it toggles the
      -- channel, and its on/off state is read from a SINGLE pixel on that icon
      -- (ReaHotkey's HotspotToggleButton — the CSS Brass overlay uses the same pixel
      -- toggle). So a screen-reader user hears every channel's state, not just one.
      --
      -- WHAT the five channels are depends on the loaded instrument, which is CSS's own
      -- design, not something we choose: a single-section patch (e.g. "CSS 1st Violins")
      -- puts the MIC POSITIONS here — Spot 1, Spot 2, Main, Room, Mix — while "CSS Full
      -- Ensemble" puts the SECTIONS here instead: Viol. 1, Viol. 2, Violas, Cellos,
      -- Basses. Same five buttons, same pixels, different meaning. The labels below name
      -- the mic set; on the Full Ensemble they are the wrong names for the right buttons.
      --
      -- Coordinates are relative to the "CINEMATIC STUDIO STRINGS" LANDMARK's top-left
      -- (the overlay is landmark-anchored — see O:landmark / originOf), so they hold
      -- regardless of host, Kontakt browser state, window size or version: the wordmark
      -- and the mixer move together.
      --
      -- MEASURED off a full-plugin capture rather than derived: the ⏻ icons sit at
      -- landmark x {-130,-90,-50,-10,30} (a 40 px pitch) and y = +350. The earlier values
      -- (x -138…, y +366) were 16 px too low — on the LABEL row under the icons, where
      -- the sampled pixel is text on background: three of the five read near-black
      -- (4,9,22) and so always reported "off", while the clicks missed the buttons.
      -- Lit, the ⏻ is CSS's accent lilac (180,165,230); unlit it is dim grey (77,79,85) —
      -- both confirmed at the corrected point, which is why the colours never looked wrong.
      local ON, OFF = { 180, 165, 230 }, { 77, 79, 85 }
      -- The NAME comes off the screen (`ocrLabel`), from the caption printed under each
      -- icon — the only place that says what the channel currently is. The static label is
      -- the fallback when OCR reads nothing, and names the mic set, which is what a
      -- single-section patch shows.
      for i, fallback in ipairs({ "Spot 1 Mic", "Spot 2 Mic", "Main Mic", "Room Mic", "Mix Position" }) do
        local x = -130 + (i - 1) * 40
        ov:addHotspotToggle({
          label = fallback,
          at = { x, 350 },
          ocrLabel = { x - 24, 358, x + 24, 376 },
          onColor = ON,
          offColor = OFF,
        })
      end

    end)
  end,
}
