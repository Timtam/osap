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
ov:addOCRButton({ label = "Instrument", region = host.os.pick {
  windows = { 90, 22, 200, 36 },
  macos = { 82, 20, 205, 43 },
} })
ov:addOCRButton({ label = "Polyphony", region = host.os.pick {
  windows = { 486, 40, 516, 70 },
  macos = { 490, 38, 530, 58 },
} })
ov:addOCRButton({ label = "Pitchbend range", region = host.os.pick {
  windows = { 576, 40, 602, 60 },
  macos = { 576, 38, 607, 60 },
} })

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
-- Embedded in a DAW: Windows only, for now. Nobody has probed a DAW-hosted sforzando on a
-- Mac, so there is no control pattern to write — and a guessed one would either never match
-- or match something else. An OS-keyed table with no entry for the current platform makes
-- the binding inert rather than wrong, which is the honest state until somebody measures it.
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
