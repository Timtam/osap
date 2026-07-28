-- The controls a Kontakt overlay carries, per CELL. Called by every overlay this module
-- builds — the plain header ones and each library's — so a library inherits exactly the
-- header its cell should have.
--
-- Every coordinate here is a plain number from the cell's own geometry table. Nothing asks
-- which Kontakt it is on, because the cell already is one.

local O = host.require("com.platform.overlay")
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
-- The instrument rack's own controls — the view toggle, the instrument and multi arrows,
-- the snapshot bar — which exist only while the rack view is up, and which the plugin
-- exposes no accessible element for in ANY host: a raw walk of a standalone Kontakt 8
-- lists its Header and Status bar buttons and nothing of the rack. So they are coordinates
-- everywhere, and the same coordinates everywhere, because the rack is laid out the same
-- whether the plugin is embedded in a DAW or running on its own.
--
-- ReaHotkey offers these only on its PLUGIN header and gives the standalone one just the
-- file menu plus a Tab pass-through of Kontakt's native focus ring. We can't inherit that
-- split: a LIBRARY overlay has to keep Tab for its own controls (see addStandaloneHeader),
-- so on a standalone Kontakt with a library loaded that pass-through is off, and without
-- these the whole rack is unreachable — which is exactly what the user found.
-- The wrench templates, resolved ONCE and reported, because whose module resolves them is
-- the open question. This file is `host.include`d by the Kontakt module, but the Kontakt
-- module is a CODE module — its source is evaluated inside every dependent library's VM. If
-- `host.path` answers with the VM OWNER's root rather than the defining module's, then inside
-- a library's VM these become <that library>/images/… , which do not exist, `load_template`
-- errors, and the whole search call fails before it starts. That would look exactly like what
-- was measured: the calibrator (running on Kontakt's own overlay) finds the button, while
-- focusing the same control on a LIBRARY overlay produces a "searching…" line and no result
-- at all. Logged once so the answer is a fact rather than another theory.
local editImagesCache
local function editImages()
  if editImagesCache then return editImagesCache end
  editImagesCache = {
    host.path("images/instrument-edit-frame.png"),
    host.path("images/instrument-edit-active-frame.png"),
  }
  host.log.info("[gbutton] wrench templates resolve to: " .. editImagesCache[1])
  return editImagesCache
end

-- Kontakt's own STATUS BAR toggles, by name.
--
-- All three are keyboard shortcuts Kontakt documents (F1, F9, F3) and all three are lost in a
-- DAW: they reach REAPER, not the plug-in. Which makes them unreachable rather than merely
-- inconvenient — there is no other way to fold Kontakt's side pane away from a keyboard.
--
-- Named, so they are clicked by name rather than measured: the calibrator's raw UIA walk lists
-- them under "Status bar", and the names it printed are used VERBATIM below because the
-- comparison is exact, description and full stop included.
--
-- The side pane is the one that prompted this, and for a reason worth recording: with it open
-- the instrument area starts ~355 px in, and ReaHotkey's Soundiron coordinates (x 155, 162,
-- 164) only make sense with it CLOSED. Every calibration shot so far has had it open, so those
-- libraries have never been seen in the layout their published coordinates were written for.
local STATUS_TOGGLES = {
  { label = "Side pane", hotkey = "Alt+1",
    name = "Side Pane (F1): Toggles the visibility of the side pane." },
  { label = "Info pane", hotkey = "Alt+2",
    name = "Info Pane (F9): Toggles the visibility of the information hint." },
  { label = "Keyboard panel", hotkey = "Alt+3",
    name = "Keyboard (F3): Toggles the visibility of the Keyboard panel, which shows the virtual keyboard." },
}

local function addStatusToggles(ov)
  for _, t in ipairs(STATUS_TOGGLES) do
    ov:addCustomButton({
      label = t.label,
      hotkey = t.hotkey,
      onActivate = function()
        -- Says so when the name does not resolve, rather than going quiet: these live in
        -- Kontakt's own chrome, which a DAW-embedded instance does not always expose.
        if not A.uiaClick(nil, t.name, host.uia.type.Button) then
          host.speech.output(t.label .. " not found", { interrupt = true })
        end
      end,
    })
  end
end

-- Kontakt's own RESIZE GRIP, dragged rather than clicked.
--
-- Kontakt's window is bigger than what a DAW shows of it, and there is no keyboard way to
-- change that: the grip is a mouse-only handle in the bottom-right corner. It matters beyond
-- comfort — ReaHotkey's Soundiron probes sit at y 652/663 plus a version offset, below our
-- 679 px of content in every configuration reachable so far, so a taller window is the one
-- untested place their reverb could be.
--
-- FOUND by template, because a resize grip is the clearest case for it: dragging the thing
-- MOVES it, so a fixed coordinate would be right exactly once.
local RESIZE_STEP = 220

-- Say what the resize actually did, and warn if it cost the grip.
--
-- Both halves were missing and both were reported as the same symptom: "no feedback". A
-- graphical button only speaks when its template is NOT found, so a resize that worked said
-- nothing at all — and for someone who cannot see the window, a successful action and a
-- no-op are then the same event. Silence after a deliberate press is the one outcome that
-- cannot be interpreted.
--
-- The grip warning is a report and not a remedy: Kontakt hides its grip once it is as big as
-- the screen allows, and resizes through nothing else, so a plug-in that reaches that size is
-- stuck big until the instrument is reloaded. The clamp above should make this unreachable —
-- this is what would notice if that rule were wrong on a display it was never measured on.
local function afterResize(ov, before)
  local o = ov:origin()
  if not (o and o.client) then return end
  local w, h = O.contentSize(o)
  if w == 0 or h == 0 then return end
  host.screen.imageSearchAsync({ host.path("images/resize-grip.png") }, {
    region = { o.client.x, o.client.y, o.client.x + w, o.client.y + h },
    tolerance = 20,
    scales = { 1.0 },
  }, function(hit)
    if not hit then
      host.log.info(string.format(
        "[resize] the grip is gone at %dx%d — Kontakt is at its maximum and cannot be made "
        .. "smaller again without reloading the instrument", w, h))
      host.speech.output(
        "Warning: the resize grip is gone. Kontakt is at its largest and cannot be made "
        .. "smaller again — reload the instrument to reset its size.", { interrupt = true })
      return
    end
    host.log.info(string.format("[resize] height %d -> %d px", before, h))
    if h == before then
      host.speech.output("Plugin height unchanged", { interrupt = true })
    else
      host.speech.output(string.format("Plugin height %d pixels", h), { interrupt = true })
    end
  end)
end

-- How much taller the plug-in may still be made before Kontakt reaches the size at which it
-- stops drawing its grip. nil when there is nothing to measure against.
--
-- Against the SCREEN, not the work area: the taskbar has no bearing on how big Kontakt will
-- make itself, and an auto-hiding one reports the full height anyway. Screen-relative with a
-- floor, so it adapts to a display this was never measured on.
local MIN_STEP = 40

local function growRoom(ov)
  local origin = ov:origin()
  local sz = host.screen.size and host.screen.size()
  if not (origin and origin.client and sz and sz.h > 0) then return nil end
  local _, h = O.contentSize(origin)
  if h == 0 then return nil end
  return (sz.h - math.max(120, math.floor(sz.h * 0.1))) - (origin.client.y + h)
end

-- CLASSIC VIEW ONLY, which is a measurement and not a guess about what Kontakt "probably"
-- does. Across 27 calibration shots the grip appears in exactly 8, and all 8 are classic view;
-- 13 play-view shots at the SAME 679 px of content have none. Play view is not resizable, so
-- offering a resize there is a button that can only ever report "not found".
--
local function addResizeControls(ov, cell)
  -- The size itself, readable by arriving at it. Asked for directly, and the reason is the
  -- one this whole thread has been about: the two controls below CHANGE the size and say so,
  -- but until now there was no way to ask what it is — only to alter it and listen. A value
  -- you can only learn by changing it is not a value you can check.
  ov:addStaticText({
    label = "Plugin size",
    when = cell.inClassic,
    text = function(o)
      local origin = o:origin()
      if not origin then return nil end
      local w, h = O.contentSize(origin)
      if w == 0 or h == 0 then return nil end
      local room = growRoom(o)
      return string.format("width %d, height %d pixels%s", w, h,
        (room ~= nil and room < MIN_STEP) and ", at maximum height" or "")
    end,
  })
  for _, r in ipairs({
    { label = "Increase plugin height", dy = RESIZE_STEP, hotkey = "Alt+9" },
    { label = "Decrease plugin height", dy = -RESIZE_STEP, hotkey = "Alt+8" },
  }) do
    ov:addGraphicalButton({
      label = r.label,
      when = cell.inClassic,
      rawOrigin = true,
      -- No region: the whole plug-in. A written-out rectangle went stale on the second press
      -- — dragging the grip made the window taller and carried the grip past a lower bound
      -- that had looked generous (664x679 became 664x965). The origin grows with the window.
      image = host.path("images/resize-grip.png"),
      scales = { 1.0 },
      -- STOP SHORT OF THE SCREEN, because the last possible step is the one that cannot be
      -- undone.
      --
      -- Kontakt stops DRAWING the grip once it is as big as the screen allows. Not clipped, not
      -- covered — absent: two captures of the plug-in's own bottom-right corner, at 1010x679 and
      -- at 1209x987, show the dotted triangle in the first and bare status bar in the second.
      -- And the grip is the only way Kontakt resizes: the host passes no size down, SetWindowPos
      -- on its window is ignored, and a drag aimed where the grip should be does nothing. So the
      -- step that reaches the maximum strands the plug-in big until the instrument is reloaded.
      --
      -- Two measurements say what that maximum is, and it is the SCREEN rather than anything of
      -- Kontakt's own: 1010x965 sitting at y 104 ends at 1069, and 1209x987 at y 58 ends at
      -- 1045, on a 1080 px screen — 11 and 35 px short of the bottom. It grows until it nearly
      -- touches, and hides the grip there.
      --
      -- So the rule is to keep a wide berth from the bottom of the SCREEN (not the work area:
      -- the taskbar is irrelevant to how big Kontakt will make itself, and an auto-hiding one
      -- reports the full height anyway). Screen-relative, with a floor, so it adapts to a
      -- display this was never measured on. The step is CLAMPED rather than refused while there
      -- is any room at all, because a partial grow is what the user actually wants — one press
      -- was already enough to bring the FX rack tab row into view, and the second press was
      -- pure damage.
      -- Announced as unavailable rather than merely declining when pressed. A button that
      -- reads as an ordinary button and then does nothing is exactly what this control type
      -- exists to prevent, and it is worse for someone who cannot see that nothing moved.
      unavailable = r.dy > 0 and function(o)
        local room = growRoom(o)
        if room ~= nil and room < MIN_STEP then
          return "at maximum height, unavailable"
        end
        return nil
      end or nil,
      dragBy = function(o)
        local origin0 = o:origin()
        local _, before = O.contentSize(origin0)
        host.timer.after(900, function() afterResize(o, before) end)
        if r.dy < 0 then return 0, r.dy end          -- shrinking cannot strand anything
        local room = growRoom(o)
        if room == nil then return 0, r.dy end       -- nothing to measure against
        -- A MINIMUM STEP, because a step too small to see is worse than a refusal. With only
        -- the raw clamp, a plug-in 2 px from the limit was dragged by 2 px: nothing moved,
        -- nothing was refused, and so nothing was said. Pressing a button and getting silence
        -- is the one outcome a blind user cannot interpret at all.
        if room < MIN_STEP then
          return nil, nil,
            "Plugin is already at the greatest height it can have without losing the "
            .. "resize handle"
        end
        -- The check stays even though the clamp should make it unreachable. It is the only
        -- thing that would notice this rule being wrong on someone else's display, and it
        -- costs one image search per press.
        return 0, math.min(r.dy, room)
      end,
      hotkey = r.hotkey,
    })
  end
end

local function addRackControls(ov, cell)
  -- The view toggle, by Kontakt's OWN keyboard shortcut (F10) rather than through its
  -- menu: the menu entry reads "Switch to Classic View   F10", and a shortcut the
  -- application defines cannot drift the way a measured menu row can. Kontakt 8 only —
  -- Kontakt 7 switches rack/single through its VIEW menu and has no such command, and the
  -- button did nothing there.
  --
  -- Two controls, one key, because the toggle has to say which way it will go. Whichever is
  -- currently visible owns Alt+V — the runtime resolves a shared hotkey to its visible
  -- claimant at press time, which is exactly this case.
  --
  -- Alt+V used to do nothing here while Enter on the same control worked, which is the fact
  -- that found the bug: same callback, same F10, one difference — the modifier still under
  -- the user's finger. A hotkey callback runs WHILE its combination is held, so the F10 it
  -- sent arrived as Alt+F10, which is not Kontakt's shortcut. The runtime now waits for the
  -- modifiers to come up first (see afterModifiersReleased), as ReaHotkey does with KeyWait.
  --
  -- F10 STAYS, deliberately. Kontakt's header does expose an accessible "Play View" button,
  -- and clicking that by name looked like the better answer — but F10 was already proven to
  -- work here and the UIA click was not, and changing both at once produced a state the user
  -- could not get out of with no way to tell which change caused it. Kontakt's own published
  -- shortcut is the more conservative dependency anyway: it cannot drift the way a measured
  -- coordinate can, and unlike the button it is documented to toggle both ways.
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
  -- KONTAKT'S OWN INSTRUMENT EDITOR, and the way back out of it. One control, both templates.
  --
  -- This is what ReaHotkey's Soundiron reverb toggles actually need: their template is a 9x19
  -- red bypass switch, a Kontakt FX-rack insert bypass, and the route to that rack is the
  -- wrench in the instrument's own header. Not the library's — Voices Of Gaia builds a
  -- "Performance | FX Rack" strip into its GUI and the older Soundiron libraries do not, which
  -- is why hunting for a tab row found one for Gaia and nothing for Adey or Mimi Page.
  --
  -- FOUND, not placed, and it has to be: the wrench belongs to an instrument row, so it moves
  -- with the browser's width, the rack's scroll and how many instruments are loaded. Kontakt
  -- exposes no accessible element for it (a raw walk of the classic view lists the Header and
  -- the Status bar and nothing of the rack), so an image is the only handle — which is exactly
  -- the control type ReaHotkey has for this and we had not ported.
  --
  -- MASKED TO THE BORDER, after four attempts at collecting appearances failed.
  --
  -- The wrench toggles, so Edit Mode draws it differently (blue glyph) — that was expected.
  -- What was not: it is drawn at least TWO ways within the classic view alone, and not subtly.
  -- Measured at the same pixels across two shots of the same library in the same view:
  -- background 248-250 against 226-237, glyph 24-48 against 81-95. The glyph is 60 apart,
  -- three times the match tolerance, so a template cut from one can never match the other.
  -- That is what "not found" kept reporting — correctly — after the coordinate frame and the
  -- scale had both already been fixed. Adding the second appearance did not help either: the
  -- next capture showed a third, so collecting them is a game with no end.
  --
  -- What IS invariant is the button's BORDER: identical pixel for pixel across every variant
  -- seen. Alpha 0 is a wildcard in this matcher, so the templates keep a 3-px opaque ring and
  -- wildcard everything inside it. One template per toggle state instead of one per appearance.
  --
  -- Verified rather than assumed, because a border-only template is generic enough to be a
  -- real false-positive risk: at scale 1, the classic frame matches at exactly one position in
  -- BOTH classic variants and nowhere in Edit Mode or the play view; the active frame matches
  -- exactly once in Edit Mode and nowhere else. Neither matches anything in the play view, so
  -- the control honestly reports "not found" there.
  --
  -- UNGATED, deliberately, though the view probe reads Edit Mode as "classic" and a gate would
  -- work. This is the way OUT of a mode a user cannot leave with the mouse, and an escape that
  -- can be hidden by a mis-read probe is not an escape. It costs one Tab stop in the play view,
  -- where neither template matches and it honestly says so.
  ov:addGraphicalButton({
    label = "Instrument edit view",
    -- `rawOrigin` like every other rack control: the rack is Kontakt's chrome, not the
    -- library's content, so it must not carry the cell's content offset. Without it the region
    -- shifted down by that offset and clipped the template's top rows — a correct image
    -- reported as "not found", which is the message this control type exists to make
    -- trustworthy and therefore the one most likely to be believed.
    rawOrigin = true,
    region = { 0, 40, 1010, 260 },
    image = editImages(),
    -- ONE SCALE, and not a micro-optimisation. The module-wide default searches ten scales
    -- from 0.67 to 2.0, right for a library's ARTWORK at whatever zoom the plug-in is set to
    -- and wrong for a fixed-size button in the host's chrome: measured, this template has ZERO
    -- strict matches in Edit Mode at scale 1, and the multi-scale search still reported a hit
    -- at a size the button is never drawn at. So it said "not found" when read and clicked
    -- something else when pressed — the worst possible pair. One scale makes "not found" mean it.
    scales = { 1.0 },
    hotkey = "Alt+E",
  })
  -- Offered only while that view is up. The coordinates are right for it, but in the other
  -- view the same points fall inside whatever library is loaded — one of them sat on
  -- Cinematic Studio Strings' "Sustain", so Ctrl+P changed the articulation instead of the
  -- instrument. ReaHotkey gates them the same way (`If This.GetPluginView() = "rack"`).
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
        -- The snapshot menu LEAVES a menu open for the user to walk. Declaring it hands the
        -- overlay's own hotkeys to that menu from the press onwards, instead of letting
        -- Alt+M re-press the camera behind it.
        opensMenu = h.opensMenu,
        onActivate = function(o)
          -- Acknowledge at PRESS time, as a hotspot control does. These act through a
          -- reveal-then-click sequence that can take half a second, and one that worked
          -- used to be completely silent.
          host.speech.output(h.label .. ", activated", { interrupt = true })
          A.snapshotAction(cell, h.fromRight, h.y, h.opensMenu)(o)
        end,
      })
    end
  end)
end

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
  addRackControls(ov, cell)
  addStatusToggles(ov)
  addResizeControls(ov, cell)
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
    -- Unlike the three actions above, which drive the menu themselves and close it again,
    -- this one hands it to the user — so the overlay stops owning keys while it is up.
    opensMenu = true,
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
  addRackControls(ov, cell)
  -- A last stop that hands Tab on to Kontakt's OWN focusable controls: its window does not
  -- move focus on Tab by itself, so without this its native UI is unreachable. ReaHotkey's
  -- standalone Tab pass-through, and like ReaHotkey's it is a CONTROL — Tab reaches it
  -- after ours, walks the plugin's ring, and comes back out.
  --
  -- Not on a LIBRARY overlay. There the plugin's own controls are the ones the library
  -- overlay exists to replace with named, reachable ones, and the sample library's UI is
  -- drawn in a Qt scene that exposes no focusable elements anyway.
  if not isLibrary then ov:addPassThrough({ label = "Kontakt controls" }) end
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
