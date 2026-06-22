-- Kontakt framework, as a code module (com.platform.kontakt). It provides:
--   • a BASE overlay — the generic, library-independent header (open the file
--     menu, load an instrument) — active whenever Kontakt is focused in a DAW.
--     Created by activate(), which the host calls only in Kontakt's own VM.
--   • library(spec) — called by a sample-library module that DEPENDS on Kontakt
--     (so this file is evaluated in that library's VM): it builds an INHERITING
--     overlay (the same header plus the library's controls) that takes over from
--     the base whenever the library's landmark is on screen.
-- Base and every library overlay compete on one arbiter slot; the library (higher
-- specificity) wins when its landmark is present, otherwise the base. Library
-- controls are authored in the Kontakt 7 coordinate frame; Kontakt 8 shifts
-- content down 29 px and the per-control origin offset adapts them automatically.

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")

local SLOT = "com.platform.kontakt"
local BUTTON, MENUITEM = 50000, 50011

-- A Kontakt version's identity (UIA name + Window/Pane control type) + the content
-- offset its libraries need.
local function isKontakt(version)
  return function(c)
    return host.uia.find(c.id, version, 50032) or host.uia.find(c.id, version, 50033)
  end
end
local VARIANTS = {
  { name = "Kontakt 8", offset = { 0, 29 }, identify = isKontakt("Kontakt 8") },
  { name = "Kontakt 7", offset = { 0, 0 }, identify = isKontakt("Kontakt 7") },
}
local function anyKontakt(c)
  for _, v in ipairs(VARIANTS) do
    if v.identify(c) then return true end
  end
  return false
end
-- Resolve a control's version offset (cached per control id — a UIA probe is
-- costly and the version is fixed for a given window).
local offsetCache = {}
local function resolveOffset(c)
  local off = offsetCache[c.id]
  if off == nil then
    off = { 0, 0 }
    for _, v in ipairs(VARIANTS) do
      if v.identify(c) then
        off = v.offset
        break
      end
    end
    offsetCache[c.id] = off
  end
  return off[1], off[2]
end

-- The focused Kontakt plugin window handle, from an overlay's active context.
local function pluginHwnd(ov)
  local ctrl = ov.activeCtx and ov.activeCtx.origin()
  return ctrl and ctrl.id
end
local function uiaClick(hwnd, name, ctype)
  if not hwnd then return false end
  local p = host.uia.locate(hwnd, name, ctype)
  if p then
    host.input.click(p.x, p.y)
    return true
  end
  return false
end

-- A fresh copy of the generic, library-independent header (UIA-driven, robust
-- across versions), so each overlay owns its own control tables.
local function header()
  return {
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
          host.timer.after(200, function() uiaClick(hwnd, "Load...", MENUITEM) end)
        end
      end,
    },
  }
end

-- The embedded-attach spec shared by the base and every library overlay. `present`
-- is the library's landmark gate (nil for the base, always present when Kontakt
-- is focused).
local function spec(present)
  return {
    hosts = daw.all,
    -- NI Qt plugin control class (e.g. "Qt652QWindowIcon{GUID}1"). Loose match; the
    -- UIA name (anyKontakt) disambiguates Kontakt from other NI Qt plugins.
    control = "Qt%d+QWindowIcon",
    identify = anyKontakt,
    present = present,
  }
end

return {
  -- Standalone side effect, run only in Kontakt's own VM: the base overlay.
  activate = function()
    local ov = O.new("Kontakt")
    ov:setControls(header())
    ov:attachEmbedded(spec(nil), { slot = SLOT, specificity = 0, naturalNav = true })
    ov:_startMenuWatch()
    host.log.info("Kontakt base overlay ready.")
  end,

  -- Called from a sample-library module (this file runs in its VM): an inheriting
  -- overlay that wins over the base when `lib.image` is on screen inside Kontakt.
  library = function(lib)
    local ov = O.new("Kontakt — " .. (lib.name or "library"))
    local controls = header()
    for _, c in ipairs(lib.controls or {}) do
      controls[#controls + 1] = c
    end
    ov:setControls(controls)
    ov._originOffset = resolveOffset
    -- Match = Kontakt focused AND this library's landmark visible. Checked fresh
    -- (a library loads/unloads inside Kontakt with no window event), and pcall-
    -- guarded so a bad image path skips rather than breaking detection.
    local function landmark(c)
      if not lib.image or not c.client or not c.bounds then return false end
      local region =
        { c.client.x, c.client.y, c.client.x + c.bounds.w, c.client.y + c.bounds.h }
      local ok, res = pcall(host.screen.imageSearch, lib.image, { region = region })
      return ok and res ~= nil
    end
    ov:attachEmbedded(
      spec(landmark),
      { slot = SLOT, specificity = 1, naturalNav = true, pollMatch = lib.pollMs or 500 }
    )
    ov:_startMenuWatch()
    host.log.info("Kontakt library overlay ready: " .. (lib.name or "library"))
    return ov
  end,
}
