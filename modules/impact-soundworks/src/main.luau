-- Impact Soundworks — Juggernaut, as two overlays rather than one.
--
-- Ported from ReaHotkey's ImpactSoundworks.ahk (under
-- Includes/Overlays/KontaktKompleteKontrol/), which registers Juggernaut twice: once for
-- the BASS patch and once for DRUMS & FX. They are not variants of a layout, they are
-- different instruments sharing a name — a bass synth with a step sequencer, and an
-- eight-channel drum mixer — so each gets its own overlay, told apart by its own wordmark.
--
-- ── The landmark ─────────────────────────────────────────────────────────────
-- ReaHotkey identifies these by the loaded patch's NAME as Kontakt prints it in the
-- instrument header ("BASS Juggernaut"), in Kontakt's own font. That detects the patch
-- perfectly well and anchors nothing: it sits in Kontakt's chrome, not in the library's
-- panel, so a coordinate measured from it moves whenever Kontakt's own layout does.
--
-- The library's artwork says the same thing in its own panel: the upper word of the
-- wordmark is "BASS" in one and "DRUMS/FX" in the other, over identical "JUGGERNAUT"
-- lettering. So that word is the landmark. Verified both ways — each fails to match in the
-- other patch's shot, which is what keeps the two overlays off each other's window.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

-- Measured from calibration shots of both patches (Kontakt 8, browser open — the state
-- that would break a plugin-relative coordinate and leaves an anchored one alone).
--
-- `preset` is the blue name field, in wordmark-relative coordinates, cropped short of the
-- up/down arrows at its right end: ReaHotkey's OCRButton CLICKS the region it reads, which
-- opens the preset browser, and a click that landed on an arrow would step the preset
-- instead — changing the sound of a control that was only meant to report it.
local PRODUCTS = {
  {
    name = "Juggernaut Bass",
    landmark = "images/Juggernaut/Bass.png", -- "BASS", 76x30 at (391,53)
    preset = { 321, 5, 462, 33 }, -- the blue field, x 706..899 in that shot
  },
  {
    name = "Juggernaut Drums & FX",
    landmark = "images/Juggernaut/DrumsFX.png", -- "DRUMS/FX", 118x27 at (371,57)
    preset = { 131, 1, 308, 29 }, -- x 496..725
  },
}

return {
  -- Standalone side effect (this module's own VM): one library overlay per patch.
  activate = function()
    for _, p in ipairs(PRODUCTS) do
      kontakt.library(p.name, img(p.landmark), function(ov)
        ov:addStaticText(p.name)
        -- ReaHotkey's OCRButton: reads the preset name aloud on focus, and clicking it
        -- opens the preset browser. The name is the only thing in this UI that says which
        -- of the two hundred presets is loaded, and nothing else exposes it.
        ov:addOCRButton({ label = "Preset", region = p.preset, hotkey = "Alt+P" })
      end)
    end
  end,
}
