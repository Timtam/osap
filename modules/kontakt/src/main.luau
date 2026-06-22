-- Kontakt container overlay. Attaches to Kontakt embedded in a DAW, detects the
-- loaded sample library (any module registered under the "kontakt.library"
-- contract via host.providers), and renders its controls after a generic header.
--
-- Library controls are authored in the Kontakt 7 coordinate frame. Kontakt 8
-- shifts the instrument content down by 29 px. The container detects which
-- version is in front (UIA name) and applies that variant's offset, so a library
-- never needs to know whether it runs in Kontakt 7 or 8 — its coordinates adapt.

local daw = host.require("com.platform.daw-hosts")

-- UIA control-type ids.
local BUTTON, MENUITEM = 50000, 50011

-- A Kontakt version's identity (UIA name + Window/Pane control type).
local function isKontakt(version)
  return function(c)
    return host.uia.find(c.id, version, 50032) or host.uia.find(c.id, version, 50033)
  end
end

-- The focused Kontakt plugin window handle, from the overlay's active context.
local function pluginHwnd(ov)
  local ctrl = ov.activeCtx and ov.activeCtx.origin()
  return ctrl and ctrl.id
end

-- Find a UIA element by name + type within the plugin and click its centre.
local function uiaClick(hwnd, name, ctype)
  if not hwnd then return false end
  local p = host.uia.locate(hwnd, name, ctype)
  if p then
    host.input.click(p.x, p.y)
    return true
  end
  return false
end

-- Generic, library-independent Kontakt controls (always present, before the
-- library's). UIA-driven (robust across versions), ported from ReaHotkey's
-- Kontakt header. Instrument/snapshot navigation + save follow once validated.
local header = {
  {
    kind = "custom",
    label = "Kontakt file menu",
    hotkey = "Alt+F",
    onActivate = function(ov)
      if uiaClick(pluginHwnd(ov), "Kontakt File Menu", BUTTON) then
        host.keys.menuOpen(true) -- snappier than waiting for the menu-watch poll
      end
    end,
  },
  {
    kind = "custom",
    label = "Load instrument",
    hotkey = "Ctrl+L",
    onActivate = function(ov)
      local hwnd = pluginHwnd(ov)
      if uiaClick(hwnd, "Kontakt File Menu", BUTTON) then
        -- Let the menu render, then click "Load…".
        host.timer.after(200, function() uiaClick(hwnd, "Load...", MENUITEM) end)
      end
    end,
  },
}

host.overlay.container({
  label = "Kontakt",
  hosts = daw.all,
  -- NI Qt plugin control class (e.g. "Qt652QWindowIcon{GUID}1"). Loose match; the
  -- UIA name disambiguates Kontakt from other NI Qt plugins (e.g. Komplete Kontrol).
  control = "Qt%d+QWindowIcon",
  variants = {
    { name = "Kontakt 8", offset = { 0, 29 }, identify = isKontakt("Kontakt 8") },
    { name = "Kontakt 7", offset = { 0, 0 }, identify = isKontakt("Kontakt 7") },
  },
  contract = "kontakt.library",
  header = header,
  naturalNav = true,
})

host.log.info("Kontakt container overlay ready.")
