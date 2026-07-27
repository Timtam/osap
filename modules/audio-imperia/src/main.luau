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
-- One segment of a product's mixer selector. `y0`/`y1` are the row, shared by every segment
-- of that product; `x0`/`x1` are this segment's own box. The label suffix and the three
-- template paths follow from the segment's name, which is ReaHotkey's own naming.
local function mix(product, name, x0, x1, y0, y1)
  local base = "images/" .. product .. "/" .. name .. "Mix"
  return {
    label = name .. " Mix",
    region = { x0, y0, x1, y1 },
    on = { base .. "On.png" },
    off = { base .. "Off1.png", base .. "Off2.png" },
  }
end

local PRODUCTS = {
  {
    name = "Areia",
    landmark = "images/Areia/Product.png",
    -- MEASURED off a calibration shot of "Areia - 03 6 Celli - Performance Legato" in
    -- Kontakt 8 with the browser open. Wordmark at (972,55); the mixer row is y 224–247 and
    -- the two segments span x 410–489 and 491–574; the mic-blend track runs x 410–574 at
    -- y 166–180 with the thumb centred on (492,172).
    verified = true,
    -- The Close/Mid/Far mic blend. `from`/`to` are the thumb CENTRE at each end of its
    -- travel — the track's ends inset by the thumb's radius — which is what makes the
    -- reading a percentage of the blend rather than of the bar. Checked against the shot:
    -- the thumb sat on "Mid" and the arithmetic returns exactly 50.
    -- TWO thumb templates, which is what ReaHotkey's EZMixerOff/EZMixerOn pair is for and
    -- the reason a single one failed live: once the slider has been touched, the knob is
    -- drawn slightly differently — measured, up to 27 per channel — and our matcher wants
    -- every pixel within tolerance. No single tolerance fixes that: 27 is needed to accept
    -- the second state, and at 30 the template already matches twice inside the track,
    -- at 35 seven hundred times. So: one template per state.
    --
    -- `ThumbHeld.png` was cropped from a calibration shot whose crosshair lay across the
    -- knob, so the 32 pixels it covered are stored TRANSPARENT — the matcher treats those
    -- as wildcards, which is the honest encoding for "we could not see this pixel".
    slider = {
      label = "Close/Mid/Far",
      region = { -570, 105, -390, 133 },
      thumb = { "images/Areia/Thumb.png", "images/Areia/ThumbHeld.png" },
      from = -554,
      to = -406,
    },
    mixes = {
      mix("Areia", "Classic", -562, -483, 169, 192),
      mix("Areia", "Modern", -481, -398, 169, 192),
    },
  },
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
      mix("Chorus", "Classic", -535, -494, 163, 186), -- "CLC"
      mix("Chorus", "Modern", -493, -452, 163, 186), -- "MDN"
      mix("Chorus", "ScottSmith", -451, -370, 163, 186), -- "Scott Smith"
    },
  },
}

local function build(product)
  return function(ov)
    ov:addStaticText(product.name)
    if product.slider then
      local sl = product.slider
      ov:addSlider({
        label = sl.label,
        region = sl.region,
        thumbImage = (function()
          local out = {}
          for i, t in ipairs(sl.thumb) do out[i] = img(t) end
          return out
        end)(),
        from = sl.from,
        to = sl.to,
      })
    end
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
