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

-- ── The instrument header ────────────────────────────────────────────────────
-- The two versions lay this out DIFFERENTLY, which cost a long detour: ReaHotkey keeps a
-- separate copy of these numbers per version, we shared one set, and Kontakt 8's numbers
-- were the ones that survived. Measured against a calibration shot of each:
--
--   Kontakt 8, classic view — three stacked rows
--     y 53   Multi Rack: the multi name and its arrows, at a fixed offset from the LEFT
--     y 87   instrument name, its arrows, the camera, the info button
--     y 118  the snapshot bar, once the camera has revealed it
--
--   Kontakt 7, rack view — two rows, and no multi row at all
--     y 19   the top bar: FILE LIBRARY VIEW [multi ▲▼] … SHOP
--     y 61   instrument name, its arrows, the camera, the info button
--
-- So Kontakt 8's y 87 lands two rows low in Kontakt 7 (on "Max: 128" / "Memory"), and its
-- multi coordinate — 704 px from the LEFT — falls outside a Kontakt 7 window that is only
-- 649 px wide. Both were live: the arrows changed nothing and the camera probe read the
-- wrong row, which is what "Snapshots not available" was really reporting.
--
-- `arrows` and `snapshots` are the control rows; `camera` is the button that reveals the
-- snapshot bar (actions.snapshotAction presses it and re-checks). A missing entry means
-- "not measured for this version" and the controls are simply not offered — see header.
-- CAMERA_LIT is the brightness above which that button counts as LIT — the bar is already
-- open, so pressing it again would CLOSE it. Measured in both states on both versions, and
-- they agree: the glyph reads 169 (Kontakt 7) / 170 (Kontakt 8) at rest and 255 when the
-- bar is open, so the midpoint separates them with 42 to spare either way.
--
-- It was 150, from a reading of the dark PANEL AROUND the glyph rather than the glyph, and
-- both versions therefore reported "already open" whenever the bar was shut: the reveal
-- was skipped and the snapshot targets were clicked against whatever occupies that row
-- with the bar closed — the instrument's own UI. It only ever appeared to work because the
-- bar happened to be open during testing, which is the failure mode this whole file exists
-- to avoid: a number inferred from a neighbouring measurement rather than taken from the
-- thing itself.
local CAMERA_LIT = 212

local HEADER = {
  ["Kontakt 8"] = {
    camera = { fromRight = 305, y = 87, litAbove = CAMERA_LIT },
    arrows = {
      { label = "Previous instrument", fromRight = 352, y = 87, hotkey = "Ctrl+P" },
      { label = "Next instrument", fromRight = 332, y = 87, hotkey = "Ctrl+N" },
      { label = "Previous multi", at = { 722, 53 }, hotkey = "Ctrl+Shift+P" },
      { label = "Next multi", at = { 742, 53 }, hotkey = "Ctrl+Shift+N" },
    },
    -- The bar is not in the header until the camera reveals it. With it up, ReaHotkey's
    -- numbers are right: the name field and its dropdown begin where 588 lands, and the
    -- arrows measured at (607, 117) and (625, 117) against 405 / 389 at y 118.
    snapshots = {
      { label = "Snapshot menu", fromRight = 588, y = 118, hotkey = "Alt+M", opensMenu = true },
      { label = "Previous snapshot", fromRight = 405, y = 118, hotkey = "Alt+P" },
      { label = "Next snapshot", fromRight = 389, y = 118, hotkey = "Alt+N" },
    },
  },
  ["Kontakt 7"] = {
    -- Measured from calibration shots 649 px wide: the arrow glyphs centre on x 312 and
    -- 331, the camera on x 359, all on row y 58–64; with the bar open, its dropdown
    -- triangle is at x 70 and its arrows at x 260 and 279, on row y 85–91.
    camera = { fromRight = 290, y = 61, litAbove = CAMERA_LIT },
    arrows = {
      { label = "Previous instrument", fromRight = 337, y = 61, hotkey = "Ctrl+P" },
      { label = "Next instrument", fromRight = 318, y = 61, hotkey = "Ctrl+N" },
      -- No multi pair. ReaHotkey's (704, 53) points past the right edge of this window,
      -- and what it was measured against is not in the shot — the multi selector here is
      -- the ▲▼ in the TOP bar (around x 324, y 14 and 25), which is a different control in
      -- a different place. Switching multi unloads what is loaded, so this stays absent
      -- until it is measured rather than inferred: a control that fires something else is
      -- worse than one that is missing.
    },
    -- Everything here is measured from the RIGHT even though the dropdown sits at the far
    -- left of the row, because opening Kontakt's library browser widens the window to the
    -- LEFT: the rack keeps its size and its distance from the right edge, so every
    -- right-anchored number survives that and every absolute one does not. (It is what
    -- moves ReaHotkey's SHOP coordinate from 440 to 815.) The dropdown lands at 579 here
    -- against Kontakt 8's 588 — the two layouts differ by row, not by column.
    snapshots = {
      { label = "Snapshot menu", fromRight = 579, y = 88, hotkey = "Alt+M", opensMenu = true },
      { label = "Previous snapshot", fromRight = 389, y = 88, hotkey = "Alt+P" },
      { label = "Next snapshot", fromRight = 370, y = 88, hotkey = "Alt+N" },
      -- Deliberately NOT offered, and the reason these were measured rather than derived:
      -- the same row carries save (fromRight 344) and delete (321). A guess extrapolated
      -- from Kontakt 8 put "Next snapshot" at 374 — 30 px from overwriting a snapshot.
    },
  },
}

-- Single-click header controls Kontakt 7 has and Kontakt 8 does not (its chrome moved them
-- into menus). `uia` is the button's UIA name, used where the plugin is accessible.
--
-- `opensMenu`: VIEW drops a menu, and a control that opens one has to say so — that is what
-- hands the navigation keys to the menu. The file menu has always declared it; VIEW did not,
-- and on a Mac nothing else would have noticed its menu at all.
-- `uiaOnly`: no coordinate is right for SHOP in both of Kontakt's layouts — it moves with the
-- library browser, 440 from the left with the browser closed and 815 with it open, the second
-- measured on a Mac — so when it cannot be found by name the press is refused out loud rather
-- than aimed at where it sits in one layout of the two. `shop` stays in GEOMETRY all the same:
-- the standalone header offers a control only where the geometry has its key.
local K7_CONTROLS = {
  { label = "Library on/off", key = "library", uia = "LIBRARY", hotkey = "Alt+L", embedded = true },
  { label = "View menu", key = "viewMenu", uia = "VIEW", hotkey = "Alt+V", opensMenu = true },
  { label = "Shop", key = "shop", uia = "SHOP", hotkey = "Alt+S", uiaOnly = true },
}

-- The UIA name of the file-menu button, which is version-specific.
local UIA_FILE_MENU = { ["Kontakt 7"] = "FILE", ["Kontakt 8"] = "Kontakt File Menu" }

return {
  GEOMETRY = GEOMETRY,
  HEADER = HEADER,
  K7_CONTROLS = K7_CONTROLS,
  UIA_FILE_MENU = UIA_FILE_MENU,
}
