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

-- OCR regions, relative to the window's CONTENT origin (x1, y1, x2, y2).
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
-- Worth re-checking on the next probe: the reported box for RANGE overlaps DEF's by three
-- points, so the pitchbend region is the one most likely to want a nudge.
-- Built fresh each time rather than shared, because two overlays now want them and a
-- control table is not something to alias between them.
-- `opensMenu` on all three because all three do: Enter on any of them opens sforzando's own
-- pop-up list. It is not decoration — it is what tells the runtime to stop owning the
-- navigation keys the moment the menu appears, so arrows and Enter reach the menu instead of
-- re-firing the control underneath and reopening it. Without it the macOS tester could move
-- through a menu and never commit a choice.
local function addReadOuts(o)
  o:addOCRButton({ label = "Instrument", opensMenu = true, region = host.os.pick {
    windows = { 90, 22, 200, 36 },
    macos = { 82, 20, 205, 43 },
  } })
  o:addOCRButton({ label = "Polyphony", opensMenu = true, region = host.os.pick {
    windows = { 486, 40, 516, 70 },
    macos = { 490, 38, 530, 58 },
  } })
  o:addOCRButton({ label = "Pitchbend range", opensMenu = true, region = host.os.pick {
    windows = { 576, 40, 602, 60 },
    macos = { 576, 38, 607, 60 },
  } })
end

addReadOuts(ov)

-- DAW host-window criteria come from the shared `daw-hosts` library module
-- (declared as a dependency in module.toml), so they aren't hardcoded here.
-- Sforzando's VST window class is "Plugin<instance pointer>" (host-independent),
-- so the same OCR regions apply relative to that control.
local daw = host.require("com.platform.daw-hosts")

-- Activate while the sforzando window is focused (standalone) or while it is
-- embedded in a DAW host window; deactivate on leaving. Navigation is Tab /
-- Shift+Tab / Enter (and Space) captured + suppressed, so it feels like ReaHotkey.
-- `menus = true`: while the plugin has a menu open, hand the captured navigation keys to
-- it instead of steering the overlay. Any overlay over a plugin that can open a menu needs
-- this; without it Tab and Enter are swallowed and the menu cannot be used at all.
ov:attach(sforzando, { menus = true })
-- Embedded in a DAW, by CONTROL — which is the Windows way, and only the Windows way. A
-- Mac has since been probed and the answer was that there is no control: REAPER lets its
-- accessibility tree show its own chrome and nothing of the plugin at all. The macOS case is
-- therefore a separate binding at the bottom of this file, against the FX window itself. An
-- OS-keyed table with no entry for the current platform makes this one inert rather than
-- wrong, which is what keeps the two from fighting over the same window.
ov:attachEmbedded({
  hosts = daw.all,
  control = { windows = "^Plugin%x+$" },
  -- "^Plugin%x+$" matches any REAPER plugin, so confirm it's actually sforzando:
  -- read the "sforzando" wordmark in the header (relative to the plugin control).
  -- ReaHotkey uses UIA (a Pane named "PlogueXMLGUI"); this OCR landmark is the
  -- UIA-free check (UIA-based identity to be added alongside).
  identify = function(ctrl)
    -- UIA (as ReaHotkey): sforzando's GUI is a Pane
    -- named "PlogueXMLGUI".
    if host.uia and host.uia.find(ctrl.id, "PlogueXMLGUI", host.uia.type.Pane) then
      return true
    end
    -- Fallback: read the "sforzando" wordmark in the header.
    local ox, oy = ctrl.client.x, ctrl.client.y
    local r = host.ocr.recognize({ region = { ox + 605, oy + 33, ox + 762, oy + 70 } })
    return string.find(string.lower(r.text or ""), "sforzando", 1, true) ~= nil
  end,
}, { menus = true })

-- ===== Embedded in a DAW, on macOS =====
--
-- The binding above cannot serve here, and the probe of a real REAPER session says why: its
-- FX window offers 22 accessibility elements and every one of them is REAPER's own chrome —
-- the FX list, the preset menu, Param, 2 out, UI, FX Active. Nothing whatsoever belongs to
-- sforzando. There is no control to attach to, which is exactly what the log has been saying
-- for weeks ("no control pattern for this platform").
--
-- So this attaches to the FX WINDOW and shifts its coordinate frame to where the plugin
-- actually sits inside it. Identity comes from the title, because REAPER writes the plugin's
-- name into it — `FX: Track 1 "sforzando"` — on both platforms, and because on this one it is
-- the only thing available.
if host.os.is("macos") then
  -- Measured from the probe capture rather than guessed: the plugin's dark panel begins at
  -- x=240 and y=52 relative to the window's content origin. An OCR landmark present in both
  -- the standalone and the embedded shot agreed to within one pixel.
  --
  -- Only the vertical is authored. REAPER's FX list can be dragged wider or narrower, so the
  -- horizontal is measured at run time from the one surface the accessibility tree does
  -- expose — that list — plus the gap between it and the plugin, which the same capture puts
  -- at nine points. The authored 240 stays as the answer for when the list cannot be read.
  local REAPER_TOP_INSET = 52
  local REAPER_LIST_GAP = 9
  local REAPER_LEFT_FALLBACK = 240

  -- Memoised per window. Reading the surface list is an accessibility traversal, and paying
  -- for one on every spoken read-out is not affordable on the machine this runs on. The cost
  -- of memoising is that dragging the FX list divider needs a window switch before the
  -- coordinates follow; that is the right trade for a divider nobody moves mid-session.
  local pluginOrigin = O.memoByOrigin(function(o)
    local dx = REAPER_LEFT_FALLBACK
    local surfaces = host.window.controls()
    local list = surfaces and surfaces[1]
    if list and list.bounds and o.client then
      dx = (list.bounds.x + list.bounds.w) - o.client.x + REAPER_LIST_GAP
    end
    host.log.info(("[sforzando] plugin origin inside the FX window: %d,%d"):format(dx, REAPER_TOP_INSET))
    return { dx, REAPER_TOP_INSET }
  end)

  local inDaw = O.new("sforzando in a DAW")
  addReadOuts(inDaw)
  inDaw:frame(function(o)
    local p = pluginOrigin(o)
    return p[1], p[2]
  end)
  -- Only while the keyboard is actually in the plugin. When it is in REAPER's own chrome the
  -- focus chain runs several elements deep — the probe recorded AXTable, then AXScrollArea,
  -- then the window — whereas the plugin exposes nothing at all, so focus inside it reaches
  -- the window and stops. Depth is therefore the discriminator this platform gives us, and
  -- it is the same question the Windows path asks with its list of chrome classes.
  inDaw:gate(function()
    local chain = host.window.focusChain()
    return not chain or #chain <= 1
  end)
  inDaw:attach({
    title = { contains = "sforzando" },
    macos = { app = { bundleId = "com.cockos.reaper" }, axSubrole = "AXStandardWindow" },
  }, { menus = true })
end
