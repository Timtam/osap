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
  },
  ["Kontakt 8"] = {
    fileMenu = { 86, 19 }, -- the KONTAKT wordmark
  },
}

-- File-menu item click points, relative to the FILE-MENU BUTTON: the menu drops from that
-- button, so one set of offsets holds for both versions even though the button itself sits
-- ~90 px apart in Kontakt 7 and 8.
--
-- MEASURED off a photograph of the open menu, row by row. The earlier values were one row
-- out for two of the three, which was invisible for a long time because the menu's CONTENT
-- varies: with "New Leap preset" absent everything shifts up by one row and the old numbers
-- were right again. That is also why every one of these is verified by OCR before the click
-- lands (see actions.luau) — a one-row slip on "Reset multi" hits "Save as default multi",
-- which overwrites the user's default multi.
local MENU_ITEM = {
  ["Load..."] = { 14, 63 },
  ["Save multi as..."] = { 14, 143 },
  ["Reset multi"] = { 14, 199 },
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
  K7_CONTROLS = K7_CONTROLS,
  UIA_FILE_MENU = UIA_FILE_MENU,
}
