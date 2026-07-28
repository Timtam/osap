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
-- replacing the anchor, and no page here is that shape. It was nearly extended to search for a
-- moving probe by image, for this very tab row, and that was dropped for the same reason it is
-- unused — with one overlay per page there is nothing to reveal. The overlay gated on the
-- wordmark would deactivate at the instant the reveal succeeded.
--
-- ── Two products got here late, and the reason is worth keeping ───────────────
-- Adey and Mimi Page were shipped without their reverbs and with a comment explaining that the
-- FX rack "does not exist" in this version of the libraries. It did. Their windows were 679 px
-- tall and the tab row sits at y 675..694, so every shot stopped just above the evidence, and a
-- picture that stops above a thing cannot be cited for its absence. Making the window taller
-- put the row on screen at the exact y ReaHotkey has always published. Both retracted comments
-- are kept in place below rather than deleted, because the mistake is more instructive than the
-- fix: the shot was real, the reading of it was careful, and the conclusion was still wrong.
--
-- ── Why none of ReaHotkey's coordinates could be carried over ─────────────────
-- Its numbers are plugin-absolute with a per-host offset table, and the shot shows why that
-- cannot survive: with Kontakt's browser open the instrument GUI is pushed ~355 px right, so
-- its FX-rack probe for Voices Of Gaia (155,582) lands in Kontakt's browser panel instead.
-- Everything below is measured from a landmark that moves with the panel it is printed on.

local kontakt = host.require("com.platform.kontakt")
local function img(rel) return host.path(rel) end

-- Measured on Voices Of Gaia (Kontakt 8 in REAPER, browser open, 1010x679 of content).
-- ── Mimi Page Light & Shadow: the reverb IS there, and this block twice said otherwise ──
-- First it said the FX rack "does not exist here", on the strength of a shot in which no tab row
-- appears. Then, once Adey disproved that, it said the reverb was probably there but would not
-- be written until measured. Both are now settled by a picture: with the window made taller the
-- tab row is at y 675..694, exactly where Adey's is, and the FX rack behind it is the same page
-- down to the pixel — tile at y 383, reverb at y 500, the same offset between them.
--
-- So the reverb is ported, and NOT from this table: it lives on the shared FXRACK overlay below,
-- because measuring it is what proved the page carries no product identity at all. Refusing to
-- write it until there was a shot was right; what the shot then showed was that the entry did
-- not belong here in the first place.
--
-- What stays here is this library's own page, and the one thing on it a blind user cannot
-- otherwise reach:
--
-- Rather than ship a control that clicks into a keyboard, this overlay offers the one piece of
-- state in that GUI a blind user cannot get at any other way: the articulation field, which
-- reads "AH (LIGHT)" and is the only thing saying which vowel and dynamic layer is loaded.
-- That is BEYOND ReaHotkey, and deliberately so — it is a substitution for a control that
-- cannot be ported, not an expansion of scope. `readOnly`, because activating it would open
-- the dropdown, and a dropdown a screen reader cannot read is a worse place to be than the
-- page you started on — and a `readOnly` control is announced with no control type at all,
-- rather than as a "button" it would be unable to honour.
-- The wordmark is MASKED to its lettering, and that is not a refinement — without it this
-- overlay does not appear at all on a second patch of the same library.
--
-- Loading "Mimi Page Phrases 140BPM" produced no overlay whatsoever. The wordmark was on screen,
-- in the same place, and the template still missed: of 17480 pixels, 29 were outside tolerance,
-- 27 of them in one 8x7 corner. Reading the actual values explained it — the dark ARTWORK behind
-- the lettering is a uniform ~20 brighter in that patch (39 against 59, 44/63, 45/60), while the
-- lettering itself is identical to the byte (255 against 255). The library redraws its backdrop
-- slightly differently per patch and leaves the wordmark alone.
--
-- So the template keeps only what actually identifies the library: pixels brighter than 180,
-- which is 6705 of 17480 — text and nothing else. Unique across 28 shots (it matches four, all
-- of them Mimi Page, including both captures of the patch that started this) without depending
-- on a backdrop that was never the point. Contrast the earlier attempt at masking Kontakt's
-- FX-rack tab, where the same idea left 78 opaque pixels and matched several hundred places:
-- the technique is only as good as what survives it.
--
-- The threshold is 180 and not 150 because the first attempt SHIPPED BROKEN and was reported
-- broken. At 150 the mask kept 9111 pixels, of which exactly ONE — an antialiased edge — was
-- 22 away against a tolerance of 20, and one pixel is the whole difference between an overlay
-- and no overlay. It read as verified because the tool used to check it sampled ~600 of those
-- 9111. A subsampled comparison is a safe FILTER, since missing pixels can only add false
-- candidates, and a worthless VERDICT. It now confirms every candidate against every opaque
-- pixel, which is what the runtime does.
--
-- Two things follow. Artwork wordmarks are landmarks for the TEXT printed on them, not for the
-- picture; and a landmark that stops matching is a SILENT failure — no overlay, no message, no
-- log line saying why — which is why this one had to be reported by the user rather than noticed
-- here. Worth a look at the other libraries' wordmarks before it happens again.
local MIMI = {
  name = "Mimi Page Light & Shadow",
  -- "MIMI PAGE / LIGHT & SHADOW", the artwork wordmark, masked to its lettering (see above).
  landmark = "images/MimiPage/Wordmark.png",
  articulation = { -115, 114, 165, 152 },
}

-- ── Voice Of Wind Adey: the FX page IS there, and this comment used to deny it ──
-- What stood here was: "the panel ends at y 570 with the keyboard below and there is no tab
-- row, so ReaHotkey's rack probe at y 663 has nothing to probe." Every word of that was read
-- off a calibration shot, and the conclusion was still wrong. The window was 664x679 of
-- content and the tab row sits at y 675..694 — INSIDE the instrument's own GUI, below the
-- bottom of a window that short. It was never absent, only cut off, and "there is no tab row"
-- is a claim a picture that stops above it cannot support.
--
-- ReaHotkey's y 663 + the Kontakt 8 offset of 29 = 692 lands in that row. Its number had not
-- drifted; nothing about this library had changed. Three explanations came before that one —
-- the library's own strip, drifted coordinates, a changed Kontakt frame — and the measurement
-- that settled it was making the window taller and looking again.
--
-- What follows is therefore the reverb ReaHotkey ships, ported. The articulation read-out below
-- stays gone for its own unrelated reason:
--
-- ── The articulation, an OCR target in CURSIVE ───────────────────────────────
-- The substitute control was the articulation, here "Legato - Ah": the vowel
-- within the loaded patch, which nothing else states (Kontakt's own rack names the PATCH,
-- "Adey Legato", which is different information).
--
-- And it did not survive the listening test. That text is set in a HANDWRITTEN SCRIPT, both
-- our engines are trained on print (WinRT and the embedded PaddleOCR), and what came back was
-- unusable. So the control is GONE rather than shipped with a caveat: a confident wrong read
-- tells a user who cannot check it that a different vowel is loaded than the one they are
-- hearing, and no amount of documentation reaches someone in the middle of playing.
--
-- The overlay stays, because detection is worth having on its own — it announces the library
-- and its landmark is verified — and because whatever replaces the read belongs on this
-- control when it exists. Two candidates, in order of how likely they are to actually work:
-- template-matching the FINITE set of articulation strings (this text is rendered from a fixed
-- font every time, so a picture of each vowel would be exact where OCR is hopeless), or an OCR
-- engine that handles handwriting, addressed only where a module asks for it.
--
-- ReaHotkey has a single "Voices of Wind Collection" overlay; ours is per product, like Audio
-- Imperia's eleven. Adey's landmark is 200x165 of watercolour artwork, verified not to match
-- either sibling — but see the note on those two for why that is the shape NOT to copy.
local ADEY = {
  name = "Voice Of Wind Adey",

  -- PERFORMANCE PAGE. Landmark: the watercolour wordmark, matched at (247,187) in the tall shot.
  performance = {
    landmark = "images/VoiceOfWindAdey/Wordmark.png",
    articulation = { -192, 267, 32, 305 },
  },

}

-- ── Voices of Wind Audrey and Kimba: the NAME is the landmark, not the picture ──
-- These two share their whole GUI with Adey, wordmark included: "Voice of Wind" is drawn
-- identically on all three and only the name below it differs — "Audrey" in blue italic,
-- "Kimba" in dark navy. So a landmark cut around the shared line would match all three, and one
-- cut around the artwork would depend on the artwork.
--
-- Adey's is the second of those, 200x165 of pure watercolour, and it works: checked, it matches
-- neither sibling. But it is the shape that had just failed on Mimi Page, where a backdrop
-- redrawn ~20 brighter for a different patch cost the overlay entirely, and Adey's has no
-- lettering to fall back to. So these two are cut around the NAME ALONE and masked to its
-- strokes — 2007 px for Audrey (blue dominant over red and green by more than 80; the text
-- reads (91,117,243)), 2201 for Kimba (darker than 100 against pale blue). Each matches exactly
-- once across 30 shots, and neither can be broken by repainting the picture behind it.
--
-- Their reverb needs nothing declared here: the FX rack page is the shared one below.
--
-- The articulation IS declared, and that is a difference from Adey rather than an inconsistency
-- with it. Adey's reads in a handwritten script that defeated both OCR engines, and the control
-- was removed rather than shipped wrong. These two draw the same information in a plain
-- sans-serif — "Legato - Ah" — which is what OCR is for. Same library family, different GUI
-- generation, and the deciding factor is legibility rather than which product it is.
local AUDREY = {
  name = "Voice Of Wind Audrey",
  landmark = "images/VoiceOfWindAudrey/Wordmark.png",  -- found at (569,412)
  articulation = { 26, 88, 181, 116 },
  fxRackTab = { -120, 255, 60, 310 },                  -- tab found at (-87, 273)
}

local KIMBA = {
  name = "Voice Of Wind Kimba",
  landmark = "images/VoiceOfWindKimba/Wordmark.png",   -- found at (609,425)
  articulation = { -14, 75, 141, 103 },
  fxRackTab = { -160, 240, 20, 300 },                  -- tab found at (-127, 260)
}

-- ── The FX rack page belongs to no product, so neither does its overlay ───────
-- This was written first as ADEY.fxRack, gated on one tile of the FX-slot collage on that page,
-- and the reasoning looked sound: Voices Of Gaia's tile does not match Adey's page, so the
-- collages must be per product. Then Mimi Page's FX rack was opened and our overlay announced
-- "Voice Of Wind Adey, FX rack" over an instrument rack that plainly read "Mimi Page Legato".
--
-- The collage is IDENTICAL on both. So is everything else: measured, the tile sits at y 383 in
-- both shots and the reverb at y 500, giving the same tile-to-reverb offset of (-84, +117) to
-- the pixel; the only difference between the two captures is the 346 px the open browser shifts
-- everything, and two reverb settings the user can change. The controls were right the whole
-- time. Only the NAME was wrong, and a name is not a small thing when it is the one sentence
-- spoken to someone who cannot see which library is loaded.
--
-- One comparison was mistaken for a rule. Gaia differs because it is a DIFFERENT LAYOUT — a
-- 7x17 reverb template at another offset — not because collages are per product. So this page
-- gets one overlay for every product that draws it, named for what it can actually establish:
-- Soundiron, the FX rack. Which product is loaded is not on this page, and Kontakt's own
-- instrument header — already read by the header overlay above — is where that answer lives.
--
-- It covers Voices of Wind Audrey and Kimba too, unshot and unmeasured, for the same reason it
-- covers Mimi Page: nothing here is per product.
--
-- Nor is anything here per PATCH, which had to be checked rather than assumed — the very next
-- thing that happened after this overlay was written was a second patch of Mimi Page failing to
-- match its Performance-page wordmark at all. This page came through byte-identical: tile at
-- (761,383), reverb at (677,500), Performance tab at (354,605), the same numbers the first
-- patch gives. The difference is what the two pages are made of. A Performance page is the
-- library's own backdrop with a wordmark printed on it, and the backdrop is redrawn per patch;
-- an FX rack is flat Kontakt chrome with fixed slot graphics on it. Being 80% artwork did this
-- template no harm, because it is artwork nobody repaints.
local FXRACK = {
  name = "Soundiron FX rack",
  landmark = "images/FxRack/Tile.png",

  -- The reverb insert's bypass, as two templates rather than Gaia's single pixel — this switch
  -- is a red bar with an ON/OFF legend, not a lit dot, and it is one ReaHotkey already
  -- publishes. Its own ReverbOn.png matched our capture at (331,500), exactly inside the search
  -- rect it declares (x 324..344, y 491..531), which is as direct a confirmation of a ported
  -- coordinate as this project has had: two tools, two capture paths, one pixel.
  --
  -- ReverbOff was NOT verifiable when this was written — there was no shot of the reverb
  -- switched off, and there could not be, since switching it off needs this control. It was
  -- shipped with a safe failure mode (a graphical toggle whose templates both miss reports
  -- NOTHING rather than guessing) and confirmed on the first press: the switch's mean red fell
  -- from 173 to 77 while the delay bypass beside it stayed at ~95, so the change was the
  -- control and not the lighting.
  reverb = { -91, 108, -71, 148 },
  reverbOn = "images/FxRack/ReverbOn.png",
  reverbOff = "images/FxRack/ReverbOff.png",
}

-- The two tabs, FOUND rather than placed — and shared by both Adey pages, so they are declared
-- once here rather than per product.
--
-- Found, because the row moves in both axes: its y follows the height of whatever GUI the
-- library draws above it, its x follows whether Kontakt's browser is open. A fixed point cannot
-- address it, which is why ReaHotkey's probe reads the right colour in a window of the right
-- size and lands one row below the tab in ours.
--
-- One template each, because the row is drawn identically for every Soundiron product measured
-- so far: Adey's and Gaia's crops of it differ by nothing at all once aligned — 0 pixels over
-- tolerance at a 2-px x shift, which was the entire reason two separate templates appeared to
-- be needed. Cutting them 2 px apart is not evidence of two things.
--
-- And each template is a STATE as much as a place. An unselected tab is filled with a fixed
-- grey ramp (100, 91, 89, 87, 86, 84, 83, 81, 79, 77, 75, 74, 72, 64); the selected one is
-- filled with its own page's background instead, so it matches nothing. That means each button
-- is found exactly when it is the one you can usefully press — the FX-rack tab is invisible to
-- the search once you are on the FX rack — and "not found" is the honest report for a tab you
-- are already standing on. ReaHotkey encodes the same fact as an equality test against
-- 0x565656, one sample of that ramp.
local TABS = {
  fxRack      = "images/fxrack-tab.png",
  performance = "images/performance-tab.png",
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
      -- The way to the FX rack, added before anything on that page has been measured — which
      -- is the order this project works in rather than an oversight. The page cannot be
      -- photographed until someone can reach it, and the person who has to reach it cannot use
      -- a mouse. So the switch comes first and the reverb follows from the picture it makes
      -- possible, exactly as it went for Voices Of Gaia and then for Adey.
      --
      -- Measured on the 1010x965 shot: wordmark at (493,398), tab at (482,675).
      ov:addGraphicalButton({
        label = "FX rack page",
        image = img(TABS.fxRack),
        region = { -40, 260, 140, 315 },   -- tab found at (-11, 277), 126x20
        scales = { 1.0 },
        hotkey = "Alt+R",
      })
    end)

    kontakt.library(ADEY.name, img(ADEY.performance.landmark), function(ov)
      ov:addStaticText(ADEY.name)
      -- The articulation read-out lived here and was removed; see the note above. Its measured
      -- region is kept, because it is the region whichever mechanism replaces OCR will need:
      -- ADEY.performance.articulation = { -192, 267, 32, 305 }, landmark-relative.
      --
      -- The region is small and wordmark-relative on purpose. Kontakt loads several instruments
      -- into one rack and each draws its OWN tab row: a shot with two of them has a second row
      -- 557 px above this one, and a search across the whole plug-in would find whichever came
      -- first rather than this instrument's. Anchoring to this instrument's wordmark and looking
      -- only just below it cannot pick up the neighbour's.
      ov:addGraphicalButton({
        label = "FX rack page",
        image = img(TABS.fxRack),
        region = { -140, 470, 20, 525 },   -- tab found at (-111, 488), 126x20
        scales = { 1.0 },                  -- fixed-size chrome; multi-scale invents hits
        hotkey = "Alt+R",                  -- same key as Gaia's, and the same journey
      })
    end)

    for _, lib in ipairs({ AUDREY, KIMBA }) do
      kontakt.library(lib.name, img(lib.landmark), function(ov)
        ov:addStaticText(lib.name)
        -- `readOnly` for the same reason as Mimi Page's: activating it would open a dropdown no
        -- screen reader can read, which is a worse place to be than the page you started on.
        ov:addOCRButton({
          label = "Articulation",
          region = lib.articulation,
          readOnly = true,
          hotkey = "Alt+B",
        })
        ov:addGraphicalButton({
          label = "FX rack page",
          image = img(TABS.fxRack),
          region = lib.fxRackTab,
          scales = { 1.0 },
          hotkey = "Alt+R",
        })
      end)
    end

    -- The FX rack page, ONE overlay for every product that draws it. Two-overlay-per-library
    -- shape as with Gaia, and for the same reason — the artwork wordmark that identifies a
    -- library is not drawn on this page, so an overlay gated on it would deactivate at the
    -- moment of arrival. What is different here is that the second overlay is shared; see the
    -- note on FXRACK.
    kontakt.library(FXRACK.name, img(FXRACK.landmark), function(ov)
      ov:addStaticText("Soundiron, FX rack")
      ov:addGraphicalToggle({
        label = "Reverb",
        region = FXRACK.reverb,
        onImage = img(FXRACK.reverbOn),
        offImage = img(FXRACK.reverbOff),
        hotkey = "Alt+B",
      })
      ov:addGraphicalButton({
        label = "Performance page",
        image = img(TABS.performance),
        -- Tab found at (-407, 222), 126x20. The left edge is -415 and not a round number more:
        -- that is exactly the plug-in's own left edge, and a search rect that starts outside the
        -- window is a rect the capture has to clamp or refuse.
        region = { -415, 205, -260, 260 },
        scales = { 1.0 },
        hotkey = "Alt+R",
      })
    end)

    host.log.info(
      "Soundiron: Gaia (2 pages) + Adey + Mimi Page + Audrey + Kimba + a shared FX rack ready."
    )
  end,
}
