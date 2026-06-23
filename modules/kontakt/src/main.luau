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

-- ── Komplete Kontrol nesting ──────────────────────────────────────────────────
-- KK can itself host a Kontakt instance (DAW → KK → Kontakt → library). KK and
-- Kontakt share the Qt control class and differ only by UIA name. EnumChildWindows
-- enumerates the nested Kontakt control too, but ReaHotkey anchors a nested library
-- on KK's control and crosses KK's chrome with a fixed content offset — we do the
-- same, so the SAME library layout (authored in the bare-Kontakt frame) lands
-- correctly without the library knowing anything about KK.
local function isKK(c)
  return host.uia.find(c.id, "Komplete Kontrol", 50032) or host.uia.find(c.id, "Komplete Kontrol", 50033)
end
-- KK chrome offset, from KK's control top-left to where Kontakt's content begins.
-- TUNE against the real plugin (ReaHotkey uses ~190,111 for a library under KK); if
-- the library overlay is shifted on screen, adjust these two numbers.
local KK_OFFSET = { 190, 111 }
-- Does the active host window contain a control passing `test` (a KK or a Kontakt
-- container)? The two compete: a bare Kontakt defers to a wrapping KK, and a KK
-- only counts as a host when it actually contains a Kontakt.
local function hasControl(test)
  for _, c in ipairs(host.window.controls()) do
    if string.match(c.class, "Qt%d+QWindowIcon") and test(c) then return true end
  end
  return false
end
local function kkPresent() return hasControl(isKK) end
local function kontaktPresent() return hasControl(anyKontakt) end

-- Resolve a control's content offset (cached per control id — a UIA probe is costly
-- and the host is fixed for a given window): KK's chrome offset when anchored on a
-- KK container, otherwise the bare Kontakt version offset (K7 = 0, K8 = +29).
local offsetCache = {}
local function resolveOffset(c)
  local off = offsetCache[c.id]
  if off == nil then
    if isKK(c) then
      off = KK_OFFSET
      host.log.info(string.format(
        "Kontakt nested in Komplete Kontrol — anchoring on KK, content offset (%d, %d)", off[1], off[2]
      ))
    else
      off = { 0, 0 }
      for _, v in ipairs(VARIANTS) do
        if v.identify(c) then
          off = v.offset
          break
        end
      end
    end
    offsetCache[c.id] = off
  end
  return off[1], off[2]
end

-- The plugin control to anchor on: prefer a KK container that is hosting Kontakt
-- (so the KK offset places the library), else a bare Kontakt with no KK wrapping it.
local function identifyHost(c)
  if isKK(c) then return kontaktPresent() end
  if anyKontakt(c) then return not kkPresent() end
  return false
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

-- Adds the generic, library-independent header (UIA-driven, robust across versions)
-- to `ov` with the functional builders — the same way any overlay is built. The
-- onActivate closures take the ACTIVE overlay `o` (passed at activation), so they
-- work unchanged in a derived/library overlay too.
local function addHeader(ov)
  ov:addCustomButton({
    label = "Kontakt file menu",
    hotkey = "Alt+F",
    onActivate = function(o)
      if uiaClick(pluginHwnd(o), "Kontakt File Menu", BUTTON) then
        host.keys.menuOpen(true) -- snappier than waiting for the menu-watch poll
      end
    end,
  })
  ov:addCustomButton({
    label = "Load instrument",
    hotkey = "Ctrl+L",
    onActivate = function(o)
      local hwnd = pluginHwnd(o)
      if uiaClick(hwnd, "Kontakt File Menu", BUTTON) then
        host.timer.after(200, function() uiaClick(hwnd, "Load...", MENUITEM) end)
      end
    end,
  })
end

-- The embedded-attach spec shared by the base and every library overlay. A
-- library's landmark is now an overlay-level gate (ov:landmark), not part of the
-- spec, so this is identical for the base and every library.
local function spec()
  return {
    hosts = daw.all,
    -- NI Qt plugin control class (e.g. "Qt652QWindowIcon{GUID}1"). Loose match; the
    -- UIA name (identifyHost) disambiguates Kontakt — and a KK that hosts it — from
    -- other NI Qt plugins, and anchors a KK-nested Kontakt on the KK container.
    control = "Qt%d+QWindowIcon",
    identify = identifyHost,
    -- identifyHost is RELATIONAL (a bare Kontakt defers to a wrapping KK, a KK only
    -- counts when it hosts Kontakt), so it must NOT be cached per HWND — KK's
    -- container HWND is stable while a Kontakt is loaded into it.
    cacheIdentity = false,
  }
end

return {
  -- Standalone side effect, run only in Kontakt's own VM: the base overlay.
  activate = function()
    local ov = O.new("Kontakt")
    addHeader(ov)
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 0, naturalNav = true })
    ov:_startMenuWatch()
    host.log.info("Kontakt base overlay ready.")
  end,

  -- Called from a sample-library module (this file runs in its VM): builds an
  -- inheriting overlay that wins over the base when `landmark` is on screen inside
  -- Kontakt. The library author adds its own controls in `build(ov)` with the same
  -- functional builders used everywhere — ov:addStaticText / addHotspotButton /
  -- addGraphicalToggle / addCustomButton — on top of the inherited header; no
  -- declarative control tables. Controls are authored in the Kontakt 7 coordinate
  -- frame; the per-control offset adapts them to the focused Kontakt version.
  --
  -- `landmark` (and any control on/offImage) MUST be an ABSOLUTE path (host.path) —
  -- imageSearch runs in Kontakt's identity scope, so a relative path would resolve
  -- under Kontakt's root, not the library's.
  library = function(name, landmark, build)
    local ov = O.new("Kontakt — " .. (name or "library"))
    addHeader(ov)
    ov._originOffset = resolveOffset
    if build then build(ov) end
    -- Match = Kontakt focused (the embedded context) AND this library's landmark
    -- visible (re-checked on the poll below, since a library loads/unloads inside
    -- Kontakt with no window event).
    if landmark then ov:landmark(landmark) end
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 1, naturalNav = true, pollMatch = 500 })
    ov:_startMenuWatch()
    host.log.info("Kontakt library overlay ready: " .. (name or "library"))
    return ov
  end,
}
