-- Cinematic Studio Series — Kontakt sample-library overlays that INHERIT the Kontakt
-- module. It depends on com.platform.kontakt (so Kontakt's framework is evaluated in this
-- VM) and registers one inheriting overlay per PRODUCT, each taking over from Kontakt's
-- base header whenever that product's landmark is on screen.
--
-- Ported from ReaHotkey's CinematicStudioSeries.ahk (under
-- ReaHotkey/Includes/Overlays/KontaktKompleteKontrol/): the mixer channel toggles, nothing
-- else. No articulation grid, reverb, section faders or performance toggles — those were an
-- over-derivation and are gone.
--
-- ── Why a product is a DATA ROW ───────────────────────────────────────────────
-- ReaHotkey carries THREE coordinate tables per product — Kontakt7XOffset,
-- Kontakt8XOffset, KompleteKontrolXOffset — because a library's controls sit at a different
-- place in each host. All three vanish here: the overlay is landmark-anchored, so its
-- coordinates are relative to the product's own wordmark, and the wordmark and the mixer
-- move together. One row per product, and the six cells of the Kontakt matrix
-- ({Kontakt 7, Kontakt 8} × {in a DAW, in Komplete Kontrol, standalone}) come out of
-- kontakt.library() for free.
--
-- That is also what makes the rest of ReaHotkey's library overlays cheap to add: Audiobro,
-- Impact Soundworks, Soundiron and most of Audio Imperia are this shape with different
-- numbers and a different wordmark.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

-- ── The products ─────────────────────────────────────────────────────────────
-- `mixer` is the row of channel buttons, each with a power (⏻) icon: clicking it toggles
-- the channel, and its state is read from a SINGLE pixel on that icon (ReaHotkey's
-- HotspotToggleButton). So a screen-reader user hears every channel's state, not just one.
--
-- `at` is the FIRST channel's icon relative to the wordmark's top-left, `pitch` the step to
-- the next. `mix` is a separate control for a product whose mix switch is not a power icon.
local PRODUCTS = {
  {
    name = "Cinematic Studio Strings",
    landmark = "images/CinematicStudioStrings/Product.png",
    -- MEASURED off a full-plugin capture of the live 8.11 UI, and verified live in every
    -- host. The ⏻ icons sit at wordmark x {-130,-90,-50,-10,30} and y +350. An earlier
    -- reading (x -138…, y +366) was 16 px low — on the LABEL row under the icons, where the
    -- sampled pixel is text on background: three of the five read near-black (4,9,22) and
    -- so always reported "off", while the clicks missed the buttons entirely. The colours
    -- had been measured on the real icons, which is why they never looked wrong.
    verified = true,
    mixer = {
      at = { -130, 350 },
      pitch = 40,
      on = { 180, 165, 230 }, -- CSS's accent lilac, lit
      off = { 77, 79, 85 }, -- dim grey, unlit
      -- WHAT these five are depends on the loaded instrument, which is CSS's own design:
      -- a single-section patch ("CSS 1st Violins") puts the MIC POSITIONS here, while "CSS
      -- Full Ensemble" puts the SECTIONS here instead (Viol. 1, Viol. 2, Violas, Cellos,
      -- Basses). Same buttons, same pixels, different meaning — hence `ocrLabel` below.
      channels = { "Spot 1 Mic", "Spot 2 Mic", "Main Mic", "Room Mic", "Mix Position" },
    },
  },
  {
    name = "Cinematic Studio Brass",
    landmark = "images/CinematicStudioBrass/Product.png",
    -- NOT measured — DERIVED, and the derivation is written down so it can be checked
    -- instead of trusted. ReaHotkey puts Brass's mics at plugin-frame (46, 465), (96, 465)
    -- and (146, 465), and its mix switch at (186,476)-(206,488), in the Kontakt 7 frame.
    -- Its Strings mics are at (35, 481)… in that same frame, and we measured those at
    -- wordmark-relative (-130, 350) — so in ReaHotkey's frame this wordmark sits at about
    -- (165, 131), and subtracting that gives the numbers below.
    --
    -- The one assumption is that Brass's wordmark sits at the same offset from its mixer as
    -- Strings' does. Plausible (same vendor, same UI family) and unverified — which is
    -- exactly the difference that has repeatedly been between a working control and a
    -- silent one. One calibration shot with Brass loaded settles it; until then a wrong
    -- click here toggles a mic channel, which is audible and reversible.
    verified = false,
    mixer = {
      at = { -119, 334 },
      pitch = 50,
      on = { 246, 175, 172 }, -- ReaHotkey's 0xF6AFAC — Brass's accent, lit
      off = { 115, 113, 112 }, -- 0x737170, unlit
      channels = { "Close Mic", "Main Mic", "Room Mic" },
    },
    -- Brass's mix position is NOT on the mixer row (wordmark y +345 against the mics' +334)
    -- and is not a power icon, so it is read by IMAGE as ReaHotkey does it, not by pixel.
    mix = {
      label = "Mix Position",
      region = { 21, 345, 41, 357 },
      onImage = "images/CinematicStudioBrass/MixOn.png",
      offImage = "images/CinematicStudioBrass/MixOff.png",
    },
  },
}

-- Turns one row into controls. Nothing here knows which product it is building.
local function build(product)
  return function(ov)
    ov:addStaticText(product.name)
    local m = product.mixer
    for i, fallback in ipairs(m.channels) do
      local x = m.at[1] + (i - 1) * m.pitch
      ov:addHotspotToggle({
        label = fallback,
        at = { x, m.at[2] },
        -- The NAME comes off the screen where the product prints it: the caption under each
        -- icon is the only place that says what the channel currently IS. The static label
        -- is the fallback when OCR reads nothing, so a failed read never leaves a control
        -- nameless — it then names the mic set, which is what a single-section patch shows.
        ocrLabel = { x - 24, m.at[2] + 8, x + 24, m.at[2] + 26 },
        onColor = m.on,
        offColor = m.off,
      })
    end
    if product.mix then
      ov:addGraphicalToggle({
        label = product.mix.label,
        region = product.mix.region,
        onImage = img(product.mix.onImage),
        offImage = img(product.mix.offImage),
      })
    end
  end
end

return {
  -- Standalone side effect (this module's own VM): one library overlay per product.
  activate = function()
    for _, product in ipairs(PRODUCTS) do
      kontakt.library(product.name, img(product.landmark), build(product))
      -- Said out loud in the log rather than left to the comments: an unverified product's
      -- coordinates are a hypothesis, and the next person to see it misbehave should not
      -- have to read this file to learn that.
      if not product.verified then
        host.log.info(string.format(
          "%s: coordinates are DERIVED from ReaHotkey's, not measured — verify with the "
            .. "calibrator before trusting them",
          product.name
        ))
      end
    end
  end,
}
