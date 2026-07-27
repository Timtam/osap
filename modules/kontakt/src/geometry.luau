-- Kontakt's header geometry, per version — ReaHotkey's coordinate fallbacks, which it
-- keeps in two separate overlay classes (Includes/Overlays/Kontakt7.ahk, Kontakt8.ahk)
-- with the numbers hard-coded in each. Pure data: no host calls, no state.
--
-- All coordinates are relative to the PLUGIN CONTROL's own top-left. `fromRight` entries
-- are measured from its right edge instead (ReaHotkey's ControlX + ControlWidth − N).

-- Chrome positions that differ between the versions.
local GEOMETRY = {
  ["Kontakt 7"] = {
    fileMenu = { 175, 19 }, -- the "FILE" button
    prevMulti = { 704, 53 },
    nextMulti = { 722, 53 },
    library = { 231, 19 },
    viewMenu = { 288, 19 },
    -- ReaHotkey uses 815 with the library browser open and 440 with it closed, telling
    -- them apart through the plugin's UIA tree — which an embedded Kontakt does not have.
    -- So this is only offered where UIA resolves the real button: the standalone header.
    shop = { 440, 19 },
  },
  ["Kontakt 8"] = {
    fileMenu = { 86, 19 }, -- the KONTAKT wordmark
    prevMulti = { 722, 53 },
    nextMulti = { 742, 53 },
  },
}

-- The instrument and multi arrows of the CLASSIC view's header (ReaHotkey's
-- MoveToPluginInstrumentButton / MultiButton). Verified against a calibration shot of the
-- live 8.11 classic header: the instrument pair sits at (658, 89) and (677, 89) and the
-- multi pair at (725, 54) and (744, 54) — ReaHotkey's numbers, to within a pixel or two.
--
-- They exist ONLY in the classic view. In the modern one the same coordinates fall inside
-- whatever library is loaded (one landed on Cinematic Studio Strings' "Sustain"), so they
-- are gated on the view rather than offered unconditionally — see header.luau.
--
-- The snapshot trio needed one more step to verify: its bar is not in the header until the
-- camera button reveals it. With the bar up, ReaHotkey's numbers are right again — the name
-- field and its dropdown begin where -588 lands, and the arrows measured at (607, 117) and
-- (625, 117) against its -405 / -389 at y+118.
local ARROWS = {
  { label = "Previous instrument", at = { 352, 87 }, fromRight = true, hotkey = "Ctrl+P" },
  { label = "Next instrument", at = { 332, 87 }, fromRight = true, hotkey = "Ctrl+N" },
  { label = "Previous multi", key = "prevMulti", hotkey = "Ctrl+Shift+P" },
  { label = "Next multi", key = "nextMulti", hotkey = "Ctrl+Shift+N" },
}

-- The snapshot bar's own controls. The bar is not in the header until Kontakt's camera
-- button reveals it, so each of these reveals it first and then clicks — see
-- actions.snapshotAction, which also knows where that camera is.
local SNAPSHOTS = {
  { label = "Snapshot menu", at = { 588, 118 }, fromRight = true, hotkey = "Alt+M" },
  { label = "Previous snapshot", at = { 405, 118 }, fromRight = true, hotkey = "Alt+P" },
  { label = "Next snapshot", at = { 389, 118 }, fromRight = true, hotkey = "Alt+N" },
}

-- Single-click header controls Kontakt 7 has and Kontakt 8 does not (its chrome moved them
-- into menus). `uia` is the button's UIA name, used where the plugin is accessible.
local K7_CONTROLS = {
  { label = "Library on/off", key = "library", uia = "LIBRARY", hotkey = "Alt+L", embedded = true },
  { label = "View menu", key = "viewMenu", uia = "VIEW", hotkey = "Alt+V" },
  { label = "Shop", key = "shop", uia = "SHOP", hotkey = "Alt+S" },
}

-- The UIA name of the file-menu button, which is version-specific.
local UIA_FILE_MENU = { ["Kontakt 7"] = "FILE", ["Kontakt 8"] = "Kontakt File Menu" }

return {
  GEOMETRY = GEOMETRY,
  ARROWS = ARROWS,
  SNAPSHOTS = SNAPSHOTS,
  K7_CONTROLS = K7_CONTROLS,
  UIA_FILE_MENU = UIA_FILE_MENU,
}
