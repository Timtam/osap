-- Audio Imperia — Kontakt sample-library overlays that INHERIT the Kontakt module, one
-- inheriting overlay per PRODUCT, each taking over from Kontakt's base header while that
-- product's wordmark is on screen.
--
-- Ported from ReaHotkey's AudioImperia.ahk (under
-- Includes/Overlays/KontaktKompleteKontrol/). Its eleven products are the largest block of
-- unported content there, and they are also the block this design fits best: each is a
-- MIXER SELECTOR — a segmented row of mutually exclusive mix presets — over a library whose
-- own wordmark sits in the same panel.
--
-- ── Why these coordinates are not ReaHotkey's ─────────────────────────────────
-- ReaHotkey carries three offset tables per vendor (Kontakt7 / Kontakt8 / KompleteKontrol)
-- and measures from the plugin's top-left. We anchor to the product's wordmark instead, so
-- all three tables collapse into one set of numbers that holds in every host and whatever
-- Kontakt's browser is doing.
--
-- The numbers below were measured from a calibration shot rather than converted: ReaHotkey's
-- plugin-frame values have drifted (its Cinematic Studio ones no longer land on anything),
-- and deriving one product's offsets from another's was tried on Cinematic Studio Brass and
-- came out 25 px wrong. Its IMAGE templates, on the other hand, still match exactly — the
-- Chorus wordmark matched with zero mismatched samples — so those are used as they are.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

-- ── The products ─────────────────────────────────────────────────────────────
-- `mixes` is the segmented selector. Each entry is read by IMAGE rather than by pixel, as
-- ReaHotkey does it: the difference between selected and not is a 1–2 px border and the
-- colour of the label text (measured: orange 255,121,0 against grey 115,115,115), and a
-- single-pixel probe on a line that thin is one repaint away from being wrong.
--
-- Each state may be drawn more than one way, hence the lists — ReaHotkey ships two "off"
-- variants per button, and a toggle that knows only one of them goes silent in the other.
--
-- `region` is the button's own box relative to the wordmark's top-left, which is also what
-- bounds the search: ReaHotkey's ClassicMixOff2 template matches a second time up in
-- Kontakt's own top bar, so an unscoped search would read the state off the wrong pixels.
local function chorusMix(name, x0, x1)
  return {
    label = name .. " Mix",
    region = { x0, 163, x1, 186 },
    on = { "images/Chorus/" .. name .. "MixOn.png" },
    off = { "images/Chorus/" .. name .. "MixOff1.png", "images/Chorus/" .. name .. "MixOff2.png" },
  }
end

local PRODUCTS = {
  {
    name = "Chorus",
    landmark = "images/Chorus/Product.png",
    -- MEASURED off a calibration shot of "03 Chorus - Women - Energetic Syllables" in
    -- Kontakt 8, with the library browser open (which is the case that would break a
    -- plugin-relative coordinate and leaves an anchored one untouched). The wordmark sat at
    -- (943,61) in that frame; the selector row runs y 224–247 there, i.e. wordmark +163 to
    -- +186, and the three segments span x 408–449, 450–491 and 492–573 → the numbers below.
    -- Read back live at the same time: CLC off, MDN off, Scott Smith on.
    verified = true,
    mixes = {
      chorusMix("Classic", -535, -494), -- "CLC"
      chorusMix("Modern", -493, -452), -- "MDN"
      chorusMix("ScottSmith", -451, -370), -- "Scott Smith"
    },
  },
}

local function build(product)
  return function(ov)
    ov:addStaticText(product.name)
    for _, m in ipairs(product.mixes) do
      local on, off = {}, {}
      for i, p in ipairs(m.on) do on[i] = img(p) end
      for i, p in ipairs(m.off) do off[i] = img(p) end
      ov:addGraphicalToggle({ label = m.label, region = m.region, onImage = on, offImage = off })
    end
  end
end

return {
  -- Standalone side effect (this module's own VM): one library overlay per product.
  activate = function()
    for _, product in ipairs(PRODUCTS) do
      kontakt.library(product.name, img(product.landmark), build(product))
      if not product.verified then
        host.log.info(string.format(
          "%s: coordinates are DERIVED, not measured — verify with the calibrator before "
            .. "trusting them",
          product.name
        ))
      end
    end
  end,
}
