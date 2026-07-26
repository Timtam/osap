-- The controls a Kontakt overlay carries, per CELL. Called by every overlay this module
-- builds — the plain header ones and each library's — so a library inherits exactly the
-- header its cell should have.
--
-- Every coordinate here is a plain number from the cell's own geometry table. Nothing asks
-- which Kontakt it is on, because the cell already is one.

local A = host.include("src/actions.luau")
local geo = host.include("src/geometry.luau")
local U = host.uia.type

-- Komplete Kontrol's chrome comes from the Komplete Kontrol MODULE, not from a copy here:
-- it owns those controls, and it is a code module, so its exported part crosses into this
-- VM. The in-kk cell is only declared when KK is loaded (see cells.luau), so `kk.chrome`
-- is always there when this runs.
local kk = host.tryRequire("com.platform.komplete-kontrol")

-- The Kontakt header for a bare plugin in a DAW. ReaHotkey's set: the file-menu actions,
-- the instrument / multi / snapshot arrows, and — on Kontakt 7 — the single-click chrome
-- buttons Kontakt 8 moved into menus.
--
-- All of it by COORDINATE, because a DAW-embedded Kontakt 8 exposes no accessible content
-- whatsoever: its UIA element is a leaf even when queried directly, so ReaHotkey's
-- UIA-first path is dead there and only its fallback remains. (Kontakt 7 does expose a
-- tree — Qt 6.5.3 against Kontakt 8's 6.6.1 — and the actions still try UIA first.)
local function addPluginHeader(ov, cell)
  ov:addCustomButton({
    label = "Load instrument",
    hotkey = "Ctrl+L",
    onActivate = function(o) A.invokeMenuItem(cell, o, "Load...") end,
  })
  ov:addCustomButton({
    label = "Save multi as",
    hotkey = "Ctrl+S",
    onActivate = function(o) A.invokeMenuItem(cell, o, "Save multi as...") end,
  })
  ov:addCustomButton({
    label = "Reset multi",
    hotkey = "Ctrl+R",
    onActivate = function(o) A.invokeMenuItem(cell, o, "Reset multi") end,
  })
  -- The arrows sit in the PLUGIN CONTROL's own frame, so `rawOrigin`: they must skip both
  -- the content offset and a library overlay's landmark anchoring, exactly as ReaHotkey
  -- measures them off the plugin control whatever library is loaded. `fromRight` ones are
  -- anchored to its right edge (ReaHotkey's ControlWidth − N).
  for _, h in ipairs(geo.ARROWS) do
    ov:addHotspotButton({
      label = h.label,
      at = h.at or cell.geometry[h.key],
      fromRight = h.fromRight,
      rawOrigin = true,
      hotkey = h.hotkey,
    })
  end
  for _, h in ipairs(geo.K7_CONTROLS) do
    -- Only those Kontakt 7 offers embedded, and only where the cell has the coordinate:
    -- Kontakt 8's geometry simply has no entry, so the loop skips it without a version
    -- test. Shop is absent on purpose — its x depends on whether the library browser is
    -- open, which ReaHotkey tells apart through the plugin's UIA tree, and an embedded
    -- Kontakt has none. With the browser open (its default) the closed-state coordinate
    -- lands in the browser's own chrome.
    if h.embedded and cell.geometry[h.key] then
      ov:addHotspotButton({
        label = h.label,
        at = cell.geometry[h.key],
        rawOrigin = true,
        hotkey = h.hotkey,
      })
    end
  end
end

-- The standalone header: ReaHotkey's Kontakt StandaloneHeader. A standalone Kontakt IS
-- accessible, so the file menu is genuinely usable here — the menu watch hands Tab and the
-- arrows straight to the opened Qt menu — and the Kontakt 7 extras resolve by UIA name.
local function addStandaloneHeader(ov, cell, isLibrary)
  ov:addCustomButton({
    label = "Kontakt file menu",
    hotkey = "Alt+F",
    onActivate = function(o) A.openFileMenu(cell, o) end,
  })
  for _, h in ipairs(geo.K7_CONTROLS) do
    if cell.geometry[h.key] then
      ov:addCustomButton({
        label = h.label,
        hotkey = h.hotkey,
        onActivate = A.headerAction(cell, h.uia, h.key),
      })
    end
  end
  -- Tab / Shift+Tab step through Kontakt's OWN focusable controls (its window does not
  -- move focus on Tab by itself) — ReaHotkey's standalone Tab pass-through. It REPLACES
  -- the overlay's own Tab navigation, so it belongs only to the plain header, whose job is
  -- precisely to hand you on to the plugin. On a LIBRARY overlay it would take Tab away
  -- from the very controls that overlay exists to offer: they would be present and
  -- unreachable, which reads as the library overlay not being there at all.
  if not isLibrary then ov:enableUiaPassThrough() end
end

-- Everything a cell's overlay carries before a library adds its own controls.
-- `isLibrary` marks the overlay as one a sample library is building on top of.
local function add(ov, cell, isLibrary)
  if cell.where == "in-kk" then
    kk.chrome(ov)
  elseif cell.where == "standalone" then
    addStandaloneHeader(ov, cell, isLibrary)
  else
    addPluginHeader(ov, cell)
  end
end

return { add = add }
