-- Soundiron — ported from ReaHotkey's Soundiron.ahk (Includes/Overlays/KontaktKompleteKontrol).
--
-- Three products, one control each in ReaHotkey: a Reverb toggle. What makes this the last of
-- the sample libraries rather than the easiest is WHERE that toggle lives.
--
-- ── The toggle is on a page that replaces the one identifying the library ─────
-- The Reverb switch is Kontakt's FX-rack bypass for the library's reverb insert, and the FX
-- rack is one of two tabs at the bottom of the instrument ("Performance" | "FX Rack").
-- Performance is what you get, so ReaHotkey binds a ClickFXRack helper to the toggle twice —
-- before focus and before activation — to open the rack first. The runtime grew a `reveal`
-- hook for exactly that, and then a calibration shot showed it cannot be used here.
--
-- The FX-rack page does not EXPAND, it REPLACES: the artwork header carrying the product
-- wordmark is gone while the rack is up. So the landmark that says which library this is, and
-- anchors every coordinate to its panel, disappears at precisely the moment it is needed —
-- measured, the library overlay simply deactivated and Kontakt's own header took the slot
-- back. Measured further: of the library area, only Kontakt's top bar (y 0..37) and the strip
-- below the tab row (y 523..678) are pixel-identical across the two pages. Nothing
-- product-specific survives.
--
-- ── So: one overlay per PAGE, which is the Cerberus pattern ───────────────────
-- Each page gets an overlay gated on a landmark that is actually on that page, and offers
-- only the controls that are actually there. Both landmarks are verified to match exactly
-- once on their own page. It costs one keypress — you have to switch pages to reach the
-- reverb anyway — and in exchange no control is ever offered against a coordinate resolved
-- from something that is not on screen.
--
-- `reveal` stays in the runtime, so far unused: it is right for a panel that expands WITHOUT
-- replacing the anchor, and ReaHotkey's remaining numbers hinted at that shape. A shot of
-- Mimi Page settled it — see below, it is a third shape again.
--
-- ── Why none of ReaHotkey's coordinates could be carried over ─────────────────
-- Its numbers are plugin-absolute with a per-host offset table, and the shot shows why that
-- cannot survive: with Kontakt's browser open the instrument GUI is pushed ~355 px right, so
-- its FX-rack probe for Voices Of Gaia (155,582) lands in Kontakt's browser panel instead.
-- Everything below is measured from a landmark that moves with the panel it is printed on.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

-- Measured on Voices Of Gaia (Kontakt 8 in REAPER, browser open, 1010x679 of content).
-- ── Mimi Page Light & Shadow: ReaHotkey's control does not exist here ────────
-- ReaHotkey gives this product the same Reverb toggle, with an FX-rack probe at y 652. The
-- calibration shot says there is nothing of the sort: the instrument panel ends at y 570 with
-- the keyboard directly below, there is no "Performance | FX Rack" tab row at all, and no
-- scrollbar (the right edge is a uniform 0x1a top to bottom). So there is no FX page to open,
-- nothing to reveal, and no reverb to toggle — ReaHotkey's numbers describe a layout this
-- version of the library does not have, the same way its Kontakt coordinates had drifted.
--
-- Rather than ship a control that clicks into a keyboard, this overlay offers the one piece of
-- state in that GUI a blind user cannot get at any other way: the articulation field, which
-- reads "AH (LIGHT)" and is the only thing saying which vowel and dynamic layer is loaded.
-- That is BEYOND ReaHotkey, and deliberately so — it is a substitution for a control that
-- cannot be ported, not an expansion of scope. `readOnly`, because activating it would open
-- the dropdown, and a dropdown a screen reader cannot read is a worse place to be than the
-- page you started on — and a `readOnly` control is announced with no control type at all,
-- rather than as a "button" it would be unable to honour.
local MIMI = {
  name = "Mimi Page Light & Shadow",
  -- "LIGHT & SHADOW", the distinctive line of the artwork wordmark; verified to match once.
  landmark = "images/MimiPage/Wordmark.png",
  articulation = { -115, 114, 165, 152 },
}

local GAIA = {
  name = "Voices Of Gaia",

  -- PERFORMANCE PAGE. Landmark: the upper word of the artwork wordmark, our own crop.
  performance = {
    landmark = "images/VoicesOfGaia/Wordmark.png",
    -- The FX-rack tab, wordmark-relative and measured ON THIS PAGE — which matters more than
    -- it sounds, see the note on `performanceTab` below. Grey (81,81,81) here means the rack
    -- is closed; it goes dark once selected. (ReaHotkey compares against 0x575757 = (87,87,87)
    -- with EXACT equality and would no longer match; the number kept is the measured one.)
    fxRackTab = { -33, 450 },
  },

  -- FX RACK PAGE. Landmark: one of the library's own effect tiles. Vendor marks are no use
  -- here — a "SOUNDIRON" badge would match all three products — and one tile ("stalking
  -- tom") is drawn twice in the grid, so it could not anchor. This one appears once.
  fxRack = {
    landmark = "images/VoicesOfGaia/FxRackTile.png",
    -- The reverb insert's bypass switch: a small red power button on the slot's blue bezel.
    -- Tile-relative, and above/left of the tile, hence the negative numbers.
    --
    -- ONE PIXEL, not two image templates, and measured in both states to be sure it is safe:
    -- lit (215,21,20) against unlit (113,52,46) is 159 apart, and unlike Diva's toggle — one
    -- antialiased pixel of a glyph stroke, with nothing like it next door — the whole AREA
    -- separates: across 7x7 the red channel reads 139-255 lit and 92-146 unlit. So the
    -- cheaper control is also the better one here: a single screen touch instead of a region
    -- capture and two template matches, with no images to ship or to go stale.
    reverb = { -91, -270 },
    reverbOn = { 215, 21, 20 },
    reverbOff = { 113, 52, 46 },
    -- Back to the instrument's own page, so the round trip is possible without a mouse.
    --
    -- MEASURED ON THIS PAGE, because the tab row does not sit at the same height on both:
    -- on the Performance page the tabs are at y 504..522, on the FX rack page at y 497..512.
    -- The first version of this control was authored from the Performance-page shot and its
    -- click landed 9 px low, on background — the button did nothing at all. Worse, the same
    -- mistake made me MISREAD the tabs: measuring the Performance tab at y 513 on the FX page
    -- reads the background underneath it, which came out as (0,0,0) and looked like evidence
    -- that this tab never changes colour. It changes perfectly well — grey (79,79,79) when it
    -- is the one you can press, dark (14,10,5) when it is already the page you are on.
    --
    -- Which is the argument for one overlay per page, restated: every number is taken from a
    -- picture of the page it applies to, so a coordinate cannot quietly be off by the
    -- difference between two layouts.
    performanceTab = { -44, 139 },
  },
}

return {
  activate = function()
    -- The instrument's own page: identify the library, and offer the way into the rack.
    kontakt.library(GAIA.name, img(GAIA.performance.landmark), function(ov)
      ov:addStaticText(GAIA.name)
      -- Not merely the route to the reverb: this tab is mouse-only in a self-drawn Kontakt
      -- GUI, so without this control the FX rack has no keyboard route at all. It is also
      -- what made measuring the page possible — nobody could be asked to click it.
      ov:addHotspotButton({ label = "FX rack page", at = GAIA.performance.fxRackTab, hotkey = "Alt+R" })
    end)

    -- The FX rack page: the reverb, and the way back.
    kontakt.library(GAIA.name .. " FX rack", img(GAIA.fxRack.landmark), function(ov)
      ov:addStaticText(GAIA.name .. ", FX rack")
      -- Alt+B, not Alt+V: the declaration check caught Alt+V being Kontakt's own view toggle
      -- on the header this overlay inherits, which is exactly the collision that shipped
      -- undetected in Impact Soundworks. B for bypass, and free across the whole Kontakt tree.
      ov:addHotspotToggle({
        label = "Reverb",
        at = GAIA.fxRack.reverb,
        onColor = GAIA.fxRack.reverbOn,
        offColor = GAIA.fxRack.reverbOff,
        hotkey = "Alt+B",
      })
      ov:addHotspotButton({
        label = "Performance page",
        at = GAIA.fxRack.performanceTab,
        hotkey = "Alt+R",
      })
    end)

    kontakt.library(MIMI.name, img(MIMI.landmark), function(ov)
      ov:addStaticText(MIMI.name)
      ov:addOCRButton({
        label = "Articulation",
        region = MIMI.articulation,
        readOnly = true,
        hotkey = "Alt+B",
      })
    end)

    host.log.info("Soundiron: Voices Of Gaia (2 pages) + Mimi Page Light & Shadow ready.")
  end,
}
