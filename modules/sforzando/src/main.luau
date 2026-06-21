-- Sforzando overlay — ported from ReaHotkey's Sforzando standalone overlay
-- (Includes/Overlays/Sforzando.ahk). Detects the standalone sforzando window
-- and exposes its header read-outs as self-voicing OCR controls.
--
-- This is the first real ReaHotkey port: it exercises window detection →
-- context-bound overlay → OCR of real plugin UI → speech + navigation.

-- The standalone window: title "…sforzando", class PLGWindowClass, exe sforzando(.x64).exe.
local sforzando = {
  title = { contains = "sforzando" },
  app = { exe = { contains = "sforzando" } },
  windows = { class = "PLGWindowClass" },
}

local ov = host.overlay.new("sforzando")

-- OCR regions are relative to the plugin window's top-left (x1, y1, x2, y2),
-- as measured by ReaHotkey's overlay designer.
ov:addOCRButton({ label = "Instrument", region = { 90, 22, 200, 36 } })
ov:addOCRButton({ label = "Polyphony", region = { 486, 40, 516, 70 } })
ov:addOCRButton({ label = "Pitchbend range", region = { 576, 40, 602, 60 } })

-- DAW host windows that embed VST/AU plugins (host-window criteria from
-- ReaHotkey). Sforzando's VST window class is "Plugin<instance pointer>",
-- host-independent, so the same OCR regions apply relative to that control.
local dawHosts = {
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "#32770" } } },
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "REAPERb32host" } } },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "Vst3PlugWindow" } } },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "AbletonVstPlugClass" } } },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "#32770" } } },
}

-- Activate while the sforzando window is focused (standalone) or while it is
-- embedded in a DAW host window; deactivate on leaving. naturalNav = Tab /
-- Shift+Tab / Enter captured + suppressed, so it feels like ReaHotkey.
ov:attach(sforzando, { naturalNav = true })
ov:attachEmbedded({
  hosts = dawHosts,
  control = "^Plugin%x+$",
  -- "^Plugin%x+$" matches any REAPER plugin, so confirm it's actually sforzando:
  -- read the "sforzando" wordmark in the header (relative to the plugin control).
  -- ReaHotkey uses UIA (a Pane named "PlogueXMLGUI"); this OCR landmark is the
  -- UIA-free check (UIA-based identity to be added alongside).
  identify = function(ctrl)
    -- UIA (as ReaHotkey): sforzando's GUI is a Pane (50033 = UIA_PaneControlTypeId)
    -- named "PlogueXMLGUI".
    if host.uia and host.uia.find(ctrl.id, "PlogueXMLGUI", 50033) then
      return true
    end
    -- Fallback: read the "sforzando" wordmark in the header.
    local ox, oy = ctrl.client.x, ctrl.client.y
    local r = host.ocr.recognize({ region = { ox + 605, oy + 33, ox + 762, oy + 70 } })
    return string.find(string.lower(r.text or ""), "sforzando", 1, true) ~= nil
  end,
}, { naturalNav = true })
