-- The controls a Kontakt overlay carries, per CELL. Called by every overlay this module
-- builds — the plain header ones and each library's — so a library inherits exactly the
-- header its cell should have.
--
-- Every coordinate here is a plain number from the cell's own geometry table. Nothing asks
-- which Kontakt it is on, because the cell already is one.

local A = host.include("src/actions.luau")
local geo = host.include("src/geometry.luau")

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
  -- The view toggle, by Kontakt's OWN keyboard shortcut (F10) rather than through its
  -- menu: the menu entry reads "Switch to Classic View   F10", and a shortcut the
  -- application defines cannot drift the way a measured menu row can. Kontakt 8 only —
  -- Kontakt 7 has no view modes, and the button did nothing there.
  --
  -- Two controls, one key: F10 toggles, so the label has to say which way it will go.
  -- Whichever is currently visible owns Alt+V — the runtime resolves a shared hotkey to its
  -- visible claimant at press time, which is exactly this case.
  --
  -- (Kontakt 8's own "Play View" header button turns the modern view ON; it never toggles,
  -- which is why there was no way back to the classic one without this.)
  if cell.hasViewToggle then
    ov:addCustomButton({
      label = "Switch to classic view",
      hotkey = "Alt+V",
      when = cell.inModern,
      onActivate = function() host.input.send("F10") end,
    })
    ov:addCustomButton({
      label = "Switch to play view",
      hotkey = "Alt+V",
      when = cell.inClassic,
      onActivate = function() host.input.send("F10") end,
    })
  end
  -- The classic view's own arrows, offered only while that view is up. Their coordinates
  -- are right — verified against a calibration shot of the classic header — but in the
  -- modern view the same points fall inside the loaded library's UI, where one of them sat
  -- on Cinematic Studio Strings' "Sustain": pressing Ctrl+P changed the articulation
  -- instead of the instrument. ReaHotkey gates these the same way, through a UIA check we
  -- cannot make embedded; the view probe (a pixel in Kontakt's own chrome) stands in.
  ov:group(cell.inClassic, function(ov)
    for _, h in ipairs(cell.header.arrows) do
      ov:addHotspotButton({
        label = h.label,
        at = h.at or { h.fromRight, h.y },
        fromRight = h.fromRight ~= nil,
        rawOrigin = true,
        hotkey = h.hotkey,
      })
    end
    -- The snapshot controls reveal the bar themselves when they need to (see
    -- actions.snapshotAction), so they are offered whenever the view is up rather than
    -- appearing and disappearing with it.
    --
    -- A version whose bar has not been measured simply does not offer them: that row
    -- carries a save and a delete icon, so an approximate coordinate there does not miss
    -- harmlessly, it overwrites or deletes one of the user's snapshots.
    for _, h in ipairs(cell.header.snapshots or {}) do
      ov:addCustomButton({
        label = h.label,
        hotkey = h.hotkey,
        onActivate = A.snapshotAction(cell, h.fromRight, h.y),
      })
    end
  end)
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
