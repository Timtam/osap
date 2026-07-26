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
    library = { 231, 19 },
    viewMenu = { 288, 19 },
    -- ReaHotkey uses 815 with the library browser open and 440 with it closed, telling
    -- them apart through the plugin's UIA tree — which an embedded Kontakt does not have.
    -- So this is only offered where UIA resolves the real button: the standalone header.
    shop = { 440, 19 },
    prevMulti = { 704, 53 },
    nextMulti = { 722, 53 },
  },
  ["Kontakt 8"] = {
    fileMenu = { 86, 19 }, -- the KONTAKT wordmark
    prevMulti = { 722, 53 },
    nextMulti = { 742, 53 },
  },
}

-- File-menu item click points, relative to the FILE-MENU BUTTON: the menu drops from that
-- button, so one set of offsets holds for both versions even though the button itself sits
-- ~90 px apart in Kontakt 7 and 8. Measured on the live 8.11 menu (all three confirmed
-- working); the item order is the same in Kontakt 7.
local MENU_ITEM = {
  ["Load..."] = { 14, 65 },
  ["Save multi as..."] = { 14, 121 },
  ["Reset multi"] = { 14, 176 },
}

-- The instrument / multi / snapshot arrows (ReaHotkey's MoveToPluginInstrumentButton /
-- MultiButton / SnapshotButton). The multi pair is the one that moved between versions,
-- so it is looked up in GEOMETRY per focused Kontakt; the rest are identical in both.
local ARROWS = {
  { label = "Previous instrument", at = { 352, 87 }, fromRight = true, hotkey = "Ctrl+P" },
  { label = "Next instrument", at = { 332, 87 }, fromRight = true, hotkey = "Ctrl+N" },
  { label = "Previous multi", key = "prevMulti", hotkey = "Ctrl+Shift+P" },
  { label = "Next multi", key = "nextMulti", hotkey = "Ctrl+Shift+N" },
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
  MENU_ITEM = MENU_ITEM,
  ARROWS = ARROWS,
  K7_CONTROLS = K7_CONTROLS,
  UIA_FILE_MENU = UIA_FILE_MENU,
}
