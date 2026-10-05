-- Sforzando overlay — ported from ReaHotkey's Sforzando standalone overlay
-- (Includes/Overlays/Sforzando.ahk). Detects the standalone sforzando window
-- and exposes its header read-outs as self-voicing OCR controls.
--
-- This is the first real ReaHotkey port: it exercises window detection →
-- context-bound overlay → OCR of real plugin UI → speech + navigation.

-- The standalone window. Title and executable are the same on both platforms — macOS calls
-- the binary inside the bundle "sforzando" too — so the shared half of this matcher needed
-- no change at all. Only the identity that has no shared spelling differs: a Win32 window
-- class on one side, an accessibility subrole and a bundle identifier on the other.
--
-- Both blocks come from a real window rather than from a guess. The macOS side was probed on
-- a 2015 MacBook Air running Monterey: `class` came back as `AXWindow/AXStandardWindow/` and
-- the application as `com.Plogue.sforzando`.
local sforzando = {
  title = { contains = "sforzando" },
  app = { exe = { contains = "sforzando" } },
  windows = { class = "PLGWindowClass" },
  macos = {
    axSubrole = "AXStandardWindow",
    app = { bundleId = "com.Plogue.sforzando" },
  },
}

local O = host.require("com.platform.overlay")
local ov = O.new("sforzando")

-- OCR regions, relative to sforzando's own top-left (x1, y1, x2, y2): the window's CONTENT
-- origin standalone, the plug-in's own top-left inside a DAW.
--
-- Two sets, and the reason is worth understanding before adding a third platform. The
-- coordinates are not per-platform because the platforms differ; they are per-platform
-- because SFORZANDO'S OWN LAYOUT differs by a few points between its two builds. After the
-- backend learned to subtract the title bar, the same three read-outs sat within 4 points
-- vertically and 7 horizontally of where the Windows numbers put them — close enough to
-- prove the coordinate systems agree, and not close enough to share a region that has to
-- clear a neighbouring label without clipping its own value.
--
-- The macOS numbers were measured, not adjusted: the probe reported "empty" at content
-- (86,26) 40x13, "64" at (493,43) 33x11 and "DEF" at (571,44) 32x12. Each region below
-- clears the label to its left — POLY. ends at 488, PB RANGE at 574 — and is generous
-- vertically, which costs nothing because the recogniser crops to the ink it finds.
--
-- **Re-checked, and it did want one** (2026-09-04). The tester reported that this field
-- reads as nothing when its value is 1, while POLY. at 1 reads fine — and the region was the
-- cause rather than the recogniser. A second probe, this time of the plug-in EMBEDDED in
-- REAPER, put the label and the value in the same row without any overlap once both were
-- converted to the plug-in's own coordinates: RANGE at x 535..566, DEF at x 572..599. So
-- there are six clear points between them, and the region below started at 576 — four points
-- INSIDE its own value.
--
-- That is invisible for DEF, which is 27 points wide and survives losing its left edge. For a
-- single left-aligned glyph it is fatal: a "1" occupies roughly 572..579, of which a region
-- starting at 576 sees three points, and three points of a digit is not a digit. It starts at
-- 569 now — three points clear of the label, three before the value.
--
-- The earlier note here read the standalone probe as putting RANGE's box at 574, overlapping
-- DEF at 571, which cannot both be true of non-overlapping text; the embedded measurement is
-- the cleaner one and is what these numbers come from. Still unrun: whether a 1 now reads.
-- `opensMenu` on all three because all three do: Enter on any of them opens sforzando's own
-- pop-up list. It is not decoration — it is what gives the newWindow menu test (below) the
-- window list from before the click, so the list's window is recognised when it appears and
-- the navigation keys go to it, instead of re-firing the control underneath and reopening it.
--
-- One overlay carries them for both of its places, standalone and inside a DAW, on both
-- platforms: the regions are relative to sforzando's own top-left, which is the window's
-- content origin standalone and the plug-in's control — or, on a Mac, the DAW's plug-in panel —
-- inside a DAW. There was a second overlay for REAPER on a Mac, with the same three read-outs
-- in a frame moved by REAPER's geometry; the runtime's panel is that frame now, for every DAW.
ov:addOCRButton({ label = "Instrument", opensMenu = true, region = host.os.pick {
  windows = { 90, 22, 200, 36 },
  macos = { 82, 20, 205, 43 },
} })
ov:addOCRButton({ label = "Polyphony", opensMenu = true, region = host.os.pick {
  windows = { 486, 40, 516, 70 },
  macos = { 490, 38, 530, 58 },
} })
ov:addOCRButton({ label = "Pitchbend range", opensMenu = true, region = host.os.pick {
  windows = { 576, 40, 602, 60 },
  macos = { 569, 38, 607, 60 },
} })

-- DAW host-window criteria come from the shared `daw-hosts` code module (declared as a
-- dependency in module.toml), so no DAW is named here: `daw.all` is every DAW it has an entry
-- for, and a DAW added there is one sforzando is recognised in.
local daw = host.require("com.platform.daw-hosts")

-- The "sforzando" wordmark in the header, relative to the plug-in's top-left: the identity
-- check for a plug-in that publishes nothing to ask (see identify below).
--
-- Windows: ReaHotkey's landmark region, which has done this since the first port. macOS: the
-- same region grown by 8 points left and right and by 4 above and below, because sforzando's
-- Mac build lays its header out a few points differently — the read-outs above sat within 7
-- points horizontally and 4 vertically of where the Windows numbers put them once both were
-- measured from the plug-in's own corner. That difference is sforzando's, not a DAW's, so it
-- stays here. The wordmark itself has not been read on a Mac: TODO.md, "sforzando in a DAW on
-- a Mac". A wider region costs nothing — the question is only whether the word is in it.
local WORDMARK = host.os.pick {
  windows = { 605, 33, 762, 70 },
  macos = { 597, 29, 770, 74 },
}

-- The wordmark on the DAW's plug-in panel, read OFF the event loop.
--
-- On a child control (Windows) the read below is the answer itself, asked once per control handle:
-- the handler that asked waits for the reading. The panel is asked once per STAY instead — every
-- time the keyboard comes back into a plug-in from the DAW's own controls or another window — and
-- in EVERY plug-in of every DAW, not only sforzando's, because a panel says nothing about which
-- plug-in it holds. On a Mac a read there is Vision with its retry ladder, up to 250 ms when
-- nothing is found — which is exactly the common case, another plug-in's graphics — so a wait for
-- it would hold the module's first key after coming into any plug-in. So it is asked with a
-- callback, and `identify` answers nil ("could not tell yet") until the answer lands, then asks
-- every overlay to look again; `false` would be kept for the stay.
--
-- Only a read that READ something is a verdict: text without the wordmark is another plug-in.
-- Nothing read — "blank", "none", a read that failed or was pushed out — is not a "no": a view
-- that has not painted yet looks exactly like that. It stays nil, and the next evaluation reads
-- again; the runtime takes IDENTIFY_UNDECIDED_MAX of them in one stay as no. Not asked again on a
-- timer: an evaluation is a focus event, the runtime's own recheck, or Cmd+Shift+F6.
--
-- One answer at a time — one window is in front at a time — for one window, panel size and stay.
local wordmark = { key = nil, value = nil, reading = false }
local function wordmarkOnPanel(ctrl)
  local key = string.format("%s|%dx%d|%s", tostring(ctrl.id), ctrl.client.w, ctrl.client.h, tostring(ctrl.stay))
  if wordmark.key ~= key then wordmark.key, wordmark.value, wordmark.reading = key, nil, false end
  if wordmark.value ~= nil then return wordmark.value end
  if wordmark.reading then return nil end
  wordmark.reading = true
  local ox, oy = math.floor(ctrl.client.x), math.floor(ctrl.client.y)
  host.ocr.recognize({ ox + WORDMARK[1], oy + WORDMARK[2], ox + WORDMARK[3], oy + WORDMARK[4] },
    { lang = "en", key = "sforzando-wordmark" }, function(r)
      -- Another window, size or stay by now: that one reads for itself.
      if wordmark.key ~= key then return end
      wordmark.reading = false
      if r.status ~= "text" then return end
      wordmark.value = string.find(string.lower(r.text or ""), "sforzando", 1, true) ~= nil
      host.window.recheck()
    end)
  return nil
end

-- Activate while the sforzando window is focused (standalone) or while it is
-- embedded in a DAW host window; deactivate on leaving. Navigation is Tab /
-- Shift+Tab / Enter (and Space) captured + suppressed, so it feels like ReaHotkey.
-- `menus`: how sforzando's pop-up list is seen, so the captured navigation keys go to it while
-- it is open instead of steering the overlay (see the overlay runtime's Menus section).
--   nativePopup  on Windows the list is a native popup menu — ReaHotkey's sforzando overlay
--                relies on nothing but its `#32768` check for it.
--   newWindow    on a Mac it is a window of its own at the menu level (101): the 2026-09-18
--                session saw one open and close for each of the six lists it opened, standalone
--                and inside REAPER, where an earlier session's accessibility check saw none.
-- The same two for both bindings, so a list is one declaration.
local MENUS = { O.menuTests.nativePopup, O.menuTests.newWindow }
ov:attach(sforzando, { menus = MENUS })
-- Embedded in a DAW. On Windows, by CONTROL: sforzando's VST window class is
-- "Plugin<instance pointer>" — a class several vendors' plug-ins share (ReaHotkey matches
-- sforzando, Engine 2 and Zampler by it, in REAPER and in Ableton; this platform has seen it in
-- REAPER only) — and the regions above apply relative to it.
-- On a Mac there is no such control — REAPER's FX window offers 22 accessibility elements and
-- every one of them is REAPER's own chrome, nothing of sforzando's — and the binding has no
-- macOS entry, so the overlay runtime gives it the DAW's plug-in panel instead: where the
-- window's daw-hosts entry says a plug-in begins (REAPER's FX chain or floating window, Logic's
-- plug-in window, …). The runtime also keeps the overlay out of the way while the keyboard is
-- on the DAW's own controls, by asking whether the focused element lies inside the panel — the
-- question this module used to ask itself, as "is the focus chain one link deep", for REAPER
-- alone.
ov:attachEmbedded({
  hosts = daw.all,
  control = { windows = "^Plugin%x+$" },
  -- Whose plug-in is it? "^Plugin%x+$" matches other vendors' plug-ins too, and the DAW's panel
  -- on a Mac any plug-in at all, so this is asked of both, and kept: per control on Windows, per
  -- stay inside the panel on a Mac (another plug-in selected in the same FX chain window is
  -- asked about afresh — see the runtime's "stay").
  identify = function(ctrl)
    -- UIA (as ReaHotkey): sforzando's GUI is a Pane named "PlogueXMLGUI". On a Mac nothing
    -- of sforzando's is published, and this asks the whole plug-in window for nothing.
    if host.element and host.element.find(ctrl.id, "PlogueXMLGUI", host.element.type.Pane) then
      return true
    end
    -- Otherwise the "sforzando" wordmark in the header, read where it would be: with a callback
    -- on the DAW's panel (above), and on a control of its own as the answer itself — a small
    -- region, read in English as the wordmark is written, once per control handle; the handler
    -- that asked waits for the reading, and nothing else does.
    if ctrl.stay ~= nil then return wordmarkOnPanel(ctrl) end
    local ox, oy = ctrl.client.x, ctrl.client.y
    local r = host.ocr.recognize({ ox + WORDMARK[1], oy + WORDMARK[2], ox + WORDMARK[3], oy + WORDMARK[4] },
      { lang = "en" })
    -- The panel's rule: only a read that READ something is a verdict. A capture that failed, a
    -- recogniser that hung, too many reads waiting, no English recogniser: each is a reading, not a
    -- raise, and `false` would be kept for this control for good. nil is "could not tell yet": the
    -- next evaluation asks again, and the runtime takes IDENTIFY_UNDECIDED_MAX of them in a row as
    -- no.
    if r.status ~= "text" then return nil end
    return string.find(string.lower(r.text), "sforzando", 1, true) ~= nil
  end,
}, { menus = MENUS })
