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

-- Activate while the sforzando window is focused; deactivate on leaving.
-- naturalNav = Tab / Shift+Tab / arrows / Enter (captured + suppressed), so it
-- feels like a normal accessible control — just like ReaHotkey.
ov:attach(sforzando, { naturalNav = true })
