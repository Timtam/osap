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

-- ── The panel, once ──────────────────────────────────────────────────────────
-- Audio Imperia's libraries share ONE panel, at the same place in the plugin. Measured
-- across five calibration shots (Areia, Chorus, Solo, Talos, Jaeger): the mixer selector
-- always occupies y 224–247, its segments start at x 410 and 491, and where a library has
-- the Close/Mid/Far mic blend its track runs x 410–574 at y 166–180. Every product's mix
-- label matched its template at exactly (420,232) or (501,232).
--
-- What DOES move is the wordmark — Chorus prints it at (943,61), Areia at (972,55) — and
-- since our coordinates are anchored to it, that is the one number each product has to
-- contribute. So the panel is written down once, in plugin coordinates, and each product
-- carries only the wordmark position it was measured at; the offsets our controls actually
-- use are the difference. That keeps six hand-subtracted numbers per product from being six
-- chances to typo, and it states the real structure: one panel, many wordmarks.
local PANEL = {
  mixRow = { 224, 247 },
  segments = { { 410, 489 }, { 491, 574 } }, -- a two-segment selector
  blend = {
    region = { 402, 160, 582, 188 }, -- where the thumb is searched, the full travel
    from = 418, -- thumb centre at 0 %: the track's left end inset by its radius
    to = 566, --   …and at 100 %
  },
}

-- The knob is the SAME across products — Areia's pair of templates matches Jaeger's and
-- Talos's at the identical position — so it is one asset, not one per library. Two of them
-- because the knob is drawn differently under the pointer; see the note in the slider.
local THUMB = { "images/Areia/Thumb.png", "images/Areia/ThumbHeld.png" }

-- The two-segment products' labels are rendered IDENTICALLY — ReaHotkey ships four
-- byte-identical copies of these six files, one per library, and they match each other's
-- shots at exactly (420,232) and (501,232). So there is one shared set, and a library that
-- draws its own (Chorus, whose narrow segments read "CLC" and "MDN") names its own folder.
--
-- This is also what let Dolce in without a second calibration shot. Its OWN templates no
-- longer match anything — ReaHotkey's are from an older skin — and reading both states off
-- the screen would have meant asking the user to click the very buttons this overlay exists
-- to make clickable. Reusing a sibling's templates removes the question.
local SHARED_MIX = "Mix"

-- One segment of a product's mixer selector, in PLUGIN coordinates; `rel` converts.
local function mix(product, name, seg)
  local base = "images/" .. product .. "/" .. name .. "Mix"
  return {
    label = name .. " Mix",
    box = { seg[1], PANEL.mixRow[1], seg[2], PANEL.mixRow[2] },
    on = { base .. "On.png" },
    off = { base .. "Off1.png", base .. "Off2.png" },
  }
end

-- A product with the standard two-segment panel: Classic Mix, Modern Mix, and optionally
-- the mic blend. `at` is where its wordmark was measured.
local function standard(name, at, opts)
  return {
    name = name,
    landmark = "images/" .. name .. "/Product.png",
    wordmark = at,
    verified = true,
    blend = (opts and opts.blend) == true,
    mixes = {
      mix(SHARED_MIX, "Classic", PANEL.segments[1]),
      mix(SHARED_MIX, "Modern", PANEL.segments[2]),
    },
  }
end


-- ── The wide panel ───────────────────────────────────────────────────────────
-- Glade and Nucleus use Audio Imperia's OTHER layout — a wider window (1353 px against
-- 1177) with Basic/Advanced tabs — and their mix selector is two large labelled boxes
-- rather than a segmented strip. ReaHotkey drives these two with plain hotspot buttons and
-- announces no state at all; we can do better from the same single shot, because both
-- boxes contain the word "Mix" and the ONLY difference between selected and not is how
-- brightly it is drawn. So one pair of templates — "Mix" dim and "Mix" lit — serves both
-- buttons, and since each search is scoped to its own box, where the word sits inside the
-- label does not matter.
--
-- That is what made these measurable from one picture. Cropping a per-button on AND off
-- would have meant asking the user to click the buttons this overlay exists to make
-- clickable, which for a blind user is circular.
local function wideMix(product, label, box)
  return {
    label = label,
    box = box,
    on = { "images/" .. product .. "/MixOn.png" },
    off = { "images/" .. product .. "/MixOff.png" },
  }
end

local PRODUCTS = {
  -- Areia carries the three-segment note only because it was the first measured; its own
  -- selector is the standard two. Wordmark from a shot of "03 6 Celli - Performance Legato".
  standard("Areia", { 972, 55 }, { blend = true }),
  -- "03 Chorus - Women - Energetic Syllables". Chorus is the exception: THREE segments
  -- (CLC / MDN / Scott Smith), so its boxes are its own rather than PANEL's pair.
  {
    name = "Chorus",
    landmark = "images/Chorus/Product.png",
    wordmark = { 943, 61 },
    verified = true,
    mixes = {
      mix("Chorus", "Classic", { 408, 449 }), -- "CLC"
      mix("Chorus", "Modern", { 450, 491 }), -- "MDN"
      mix("Chorus", "ScottSmith", { 492, 573 }), -- "Scott Smith"
    },
  },
  standard("Solo", { 959, 61 }),
  -- Dolce's panel is the standard one and its labels match the shared templates; only
  -- ReaHotkey's Dolce-specific images are stale. Wordmark from a shot of the loaded library.
  standard("Dolce", { 948, 65 }),
  standard("Talos", { 960, 62 }, { blend = true }),
  -- Jaeger's wordmark is OUR crop, not ReaHotkey's: its Product.png no longer matches the
  -- current UI at any tolerance we would dare use, while its mix templates still do. The
  -- library was re-skinned and ReaHotkey's landmark went with it.
  standard("Jaeger", { 966, 64 }, { blend = true }),

  -- Measured from a shot of "Nucleus - 01 10 Violas - Sustained". Its wordmark sits over
  -- library artwork, so the landmark is OUR crop of the glyphs (150x13 at 781,70) —
  -- ReaHotkey's includes the backdrop and matches nothing once the patch changes it.
  -- Two stacked boxes, no mic blend.
  {
    name = "Nucleus",
    landmark = "images/Nucleus/Product.png",
    wordmark = { 781, 70 },
    verified = true,
    mixes = {
      wideMix("Nucleus", "Classic Mix", { 455, 178, 589, 201 }),
      wideMix("Nucleus", "Modern Mix", { 455, 212, 589, 235 }),
    },
  },

  -- Measured from "02 Pyramid Instruments / 02 Low Strings" — the patch family ReaHotkey
  -- targets; Glade's "Designer" patch has no mixer at all. Side-by-side boxes plus the mic
  -- blend, whose track (455–591) and thumb (centre 523) check out: exactly 50 %, i.e. "Mid".
  {
    name = "Glade",
    landmark = "images/Glade/Product.png",
    wordmark = { 771, 60 },
    verified = true,
    wideBlend = {
      region = { -326, 108, -171, 136 },
      thumb = { "images/Glade/Thumb.png" },
      from = -307,
      to = -189,
    },
    mixes = {
      wideMix("Glade", "Classic Mix", { 430, 226, 527, 250 }),
      wideMix("Glade", "Modern Mix", { 522, 226, 630, 250 }),
    },
  },
}

-- Plugin coordinates → wordmark-relative, which is what the overlay's controls use.
local function rel(product, box)
  local wx, wy = product.wordmark[1], product.wordmark[2]
  return { box[1] - wx, box[2] - wy, box[3] - wx, box[4] - wy }
end

local function build(product)
  return function(ov)
    ov:addStaticText(product.name)
    if product.wideBlend then
      local b = product.wideBlend
      local thumb = {}
      for i, t in ipairs(b.thumb) do thumb[i] = img(t) end
      ov:addSlider({
        label = "Close/Mid/Far",
        region = b.region,
        thumbImage = thumb,
        from = b.from,
        to = b.to,
      })
    end
    if product.blend then
      local b = PANEL.blend
      local thumb = {}
      for i, t in ipairs(THUMB) do thumb[i] = img(t) end
      ov:addSlider({
        label = "Close/Mid/Far",
        region = rel(product, b.region),
        thumbImage = thumb,
        from = b.from - product.wordmark[1],
        to = b.to - product.wordmark[1],
      })
    end
    for _, m in ipairs(product.mixes) do
      local on, off = {}, {}
      for i, t in ipairs(m.on) do on[i] = img(t) end
      for i, t in ipairs(m.off) do off[i] = img(t) end
      ov:addGraphicalToggle({
        label = m.label,
        region = rel(product, m.box),
        onImage = on,
        offImage = off,
      })
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
