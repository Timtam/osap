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
-- Each one's STATE, read off its own icon.
--
-- These announced their name and nothing else, which for a collapse is a poor bargain: the
-- only way to learn that a panel was open was to close it. Asked for directly, and it is the
-- same rule the read-only OCR control and the plug-in size read-out came from — a control must
-- not be less informative than what it does.
--
-- UIA has nothing to offer here. All three come back as plain Buttons (type 50000) with no
-- toggle pattern and a fixed name, so the state is not in the tree. It IS in the icon: Kontakt
-- draws the indicator lighter while its panel is open. Measured on two shots of the same
-- library differing only in the side pane, at the same point relative to the bottom-left
-- corner: 202 closed against 240 open, held over six pixels of one column rather than a single
-- antialiased edge.
--
-- Anchored to the BOTTOM of the plug-in, so the probe is computed per read rather than written
-- down: the status bar sits at the foot of a window whose height these very controls change.
local STATUS_TOGGLES = {
  -- Per VIEW, because the two views draw this bar differently and the difference is not a
  -- detail: play view uses a darker background with brighter glyphs and drops two buttons from
  -- the right, which moves the keyboard toggle. Reading a classic reference in play view is how
  -- the first version came to announce "collapsed" for a panel it had never measured.
  --
  -- Within a view: x from the LEFT edge when positive, from the RIGHT when negative; y always
  -- from the BOTTOM, because the bar hangs at the foot of a window whose height these controls
  -- change. `open`/`shut` are that icon's own two shades, measured per toggle — assuming the
  -- side pane's pair would serve all three was wrong, the info pane runs the other way round
  -- (its probe lands on the glyph rather than the fill) and is 109 apart rather than 38.
  -- Both views, and the play pair proves these buttons WORK there — which nearly got them
  -- hidden. Between two play-view shots only the keyboard icon had changed, so they looked
  -- inert; pressing Alt+1 on purpose took the plug-in from 1010 to 649 wide. The evidence for
  -- "does nothing" was really evidence that nobody had tried.
  { label = "Side pane", hotkey = "Alt+1",
    classic = { probe = { 9, -12 }, open = 240, shut = 202 },
    play    = { probe = { 9, -12 }, open = 236, shut = 188 },
    name = "Side Pane (F1): Toggles the visibility of the side pane." },
  -- Classic only, and for a reason that took a dedicated shot to establish rather than for want
  -- of trying. In play view the button WORKS — pressing it changes 3134 px, all of them in one
  -- band above the status bar — but its icon does not move: the two captures are pixel-identical
  -- across the whole bar. Kontakt simply does not indicate this one's state there.
  --
  -- Nor does the pane itself offer a probe. Only 8% of that band changes, so what appears is
  -- HINT TEXT rather than a panel, and its content depends on whatever the mouse is over. There
  -- is nothing stable to read, which is a different answer from "not measured yet" and worth
  -- the distinction: this one is closed, not open.
  { label = "Info pane", hotkey = "Alt+2",
    classic = { probe = { 43, -16 }, open = 131, shut = 240 },
    name = "Info Pane (F9): Toggles the visibility of the information hint." },
  -- Which state is which was settled by looking for the keyboard rather than by assuming
  -- brighter means open — the info pane runs the other way round, so that assumption had
  -- already been wrong once. The band above the bar is 55% near-white in one shot and 0% in
  -- the other. Its play-view pair, 236 against 188, is the same pair the side pane shows
  -- there: one palette per view, not one per button.
  { label = "Keyboard panel", hotkey = "Alt+3", uiaState = true,
    classic = { probe = { -86, -12 }, open = 240, shut = 202 },
    play    = { probe = { -58, -12 }, open = 236, shut = 188 },
    name = "Keyboard (F3): Toggles the visibility of the Keyboard panel, which shows the virtual keyboard." },
}

local PANEL_TOLERANCE = 12

-- "expanded"/"collapsed" and the verb for pressing it, or nil when the reading is neither.
--
-- Nil is the common case rather than a failure, and checking 27 shots is what showed it. The
-- play view lays its status bar out differently — the same three points read 236, 99 and 24
-- there instead of 240, 131 and 240 — and once the plug-in is grown past its host window the
-- bottom rows are not Kontakt at all, and all three read 51. Neither is a state, and saying so
-- is the whole point: a wrong "expanded" costs the user the panel.
local function panelState(ov, t, cell)
  local classic = not (cell and cell.inClassic) or cell.inClassic(ov)
  local ref = classic and t.classic or t.play
  -- No reference for this toggle in this view means NO STATE, not a nearby one. Refusing beats
  -- a close-enough number here because these controls act on the panel: a wrong reading is not
  -- a wrong word, it is a lost panel. Measured in play view, the classic points read 236, 236
  -- and 24 against 240, 131 and 240 — near enough that two of the three would have answered
  -- confidently from a layout no reference had ever seen.
  if not ref then return nil end
  local o = ov:origin()
  if not (o and o.client) then return nil end
  local w, h = O.contentSize(o)
  if w == 0 or h == 0 then return nil end
  local px = ref.probe[1] >= 0 and ref.probe[1] or (w + ref.probe[1])
  local p = host.screen.pixel(o.client.x + px, o.client.y + h + ref.probe[2])
  if not p then return nil end
  local v = (p.r + p.g + p.b) / 3
  if math.abs(v - ref.open) <= PANEL_TOLERANCE then return "expanded", "collapse" end
  if math.abs(v - ref.shut) <= PANEL_TOLERANCE then return "collapsed", "expand" end
  -- The pixel says nothing — the plug-in has been grown past its host window, or this is a
  -- layout no reference covers. Ask UIA, but only here: it is the same raw tree walk as
  -- pluginLocate, 60-300 ms against a pixel's 17, and far too slow to put on every focus step.
  -- Only the Keyboard toggle answers (measured); the other two carry no state at all.
  if t.uiaState then
    local checked = A.uiaChecked(t.name, host.uia.type.Button)
    if checked == true then return "expanded", "collapse" end
    if checked == false then return "collapsed", "expand" end
  end
  -- Also nil once the plug-in is grown past its host window: the bottom rows are then not
  -- Kontakt at all and every probe reads the same 51.
  return nil
end

local function addStatusToggles(ov, cell)
  for _, t in ipairs(STATUS_TOGGLES) do
    ov:addCustomButton({
      label = t.label,
      hotkey = t.hotkey,
      text = function(o)
        local state, action = panelState(o, t, cell)
        if not state then return nil end
        return state .. ", press to " .. action
      end,
      onActivate = function(o)
        -- Says so when the name does not resolve, rather than going quiet: these live in
        -- Kontakt's own chrome, which a DAW-embedded instance does not always expose.
        if not A.uiaClick(nil, t.name, host.uia.type.Button) then
          host.speech.output(t.label .. " not found", { interrupt = true })
          return
        end
        -- Report the state it MOVED TO, once the panel has finished appearing. A toggle that
        -- acts silently leaves the user to press it again to find out what happened.
        if host.timer then
          host.timer.after(250, function()
            local state = panelState(o, t, cell)
            if state then host.speech.output(t.label .. " " .. state, { interrupt = true }) end
          end)
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
-- The wrench's centre, measured from the RIGHT edge because Kontakt's instrument panel has a
-- fixed width and hangs there: 650 px in for the button's left edge at 1010 and at 664 px of
-- window width alike, so 632 for its middle.
local WRENCH_FROM_RIGHT, WRENCH_Y = 632, 88

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
-- KONTAKT'S INSTRUMENT-EDITOR SECTIONS, found by their captions.
--
-- Five stacked blocks that fold away — Instrument Buses, Insert Effects, Send Effects, Main
-- Effects, Modulation — each with a small button to the left of its name. Without these the
-- editor is a wall with nothing openable in it from a keyboard.
--
-- FOUND rather than placed, because they move: opening one pushes every block below it down by
-- however tall its contents are. Measured, "Modulation" sits at y 513 in one shot and y 649 in
-- another of the same view. And found by the CAPTION rather than the button, because the button
-- carries no text and looks identical on all five, while the caption is Kontakt's own chrome and
-- the same on every library. Masked to its lettering, which is the rule this project arrived at
-- the hard way: 163-241 dark pixels each, matching all four edit shots at 1010 and 664 px of
-- width, and nothing else across 45 captures.
--
-- UIA offers nothing here, and that is measured rather than assumed — a raw walk in this very
-- view lists Kontakt's header and status bar and stops. The editor is not in the tree at all.
--
-- The STATE comes from one pixel rather than a second template per block. The fold button shows
-- a triangle when shut and a cross when open, and at 18 px left and 9 px below the caption's
-- corner that reads 225 against 71 — 154 apart, identical across all four shut blocks in a shot
-- that has one open. A second template would answer the same question for the price of a second
-- full-region search.
local EDIT_SECTIONS = {
  { label = "Instrument buses", image = "images/edit/instrument-buses.png", hotkey = "Alt+Q" },
  { label = "Insert effects",   image = "images/edit/insert-effects.png",   hotkey = "Alt+W" },
  { label = "Send effects",     image = "images/edit/send-effects.png",     hotkey = "Alt+Y" },
  { label = "Main effects",     image = "images/edit/main-effects.png",     hotkey = "Alt+X" },
  { label = "Modulation",       image = "images/edit/modulation.png",       hotkey = "Alt+M" },
}

-- From the caption's top-left corner: where the fold button is, and where to read its glyph.
local FOLD_CLICK = { -19, 8 }
local FOLD_PROBE = { -18, 9 }
local FOLD_SHUT, FOLD_OPEN = 225, 71

local function addEditSections(ov, cell)
  for _, sec in ipairs(EDIT_SECTIONS) do
    ov:addGraphicalButton({
      label = sec.label,
      when = cell.inEdit,
      rawOrigin = true,
      image = host.path(sec.image),
      scales = { 1.0 },
      clickOffset = FOLD_CLICK,
      state = function(_, hit)
        local p = host.screen.pixel(hit.x + FOLD_PROBE[1], hit.y + FOLD_PROBE[2])
        if not p then return nil end
        local v = (p.r + p.g + p.b) / 3
        -- Refuses between the two, as every state probe in this file does: these controls fold
        -- a panel away, so a wrong answer costs the user the panel rather than a word.
        if math.abs(v - FOLD_SHUT) <= 40 then return "collapsed, press to expand" end
        if math.abs(v - FOLD_OPEN) <= 40 then return "expanded, press to collapse" end
        return nil
      end,
      hotkey = sec.hotkey,
    })
  end
end

local function addResizeControls(ov, cell)
  -- The size itself, readable by arriving at it. Asked for directly, and the reason is the
  -- one this whole thread has been about: the two controls below CHANGE the size and say so,
  -- but until now there was no way to ask what it is — only to alter it and listen. A value
  -- you can only learn by changing it is not a value you can check.
  -- Offered in EVERY view, unlike the two controls below it. Gating it on the classic view was
  -- a reflex — they are gated, so it was too — and wrong: this reports, it does not resize. A
  -- number that is true in both views should be readable in both, and the first thing anyone
  -- wanted it for was to check whether a control in the OTHER view had done anything.
  ov:addStaticText({
    label = "Plugin size",
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
  -- KONTAKT'S OWN INSTRUMENT EDITOR, by COORDINATE rather than by image.
  --
  -- It was found by template with a border mask, and the mask was the mistake. Its opaque ring
  -- sat on the outer 3 px of the crop, which is not the button but what is BESIDE it — and that
  -- is the library's own coloured strip: red on Voice Of Wind Adey, black on Voices Of Gaia. So
  -- it worked on the libraries it had been cut from and reported "not found" everywhere else,
  -- which is what a user hit on the first library that had never been tried. Measured, 220 of
  -- its 312 compared pixels fail on another library and every one of them is in the outer two
  -- rings; the innermost line matches perfectly. The old comment claimed the border was
  -- "identical pixel for pixel across every variant seen", and it was — every variant seen had
  -- been the same library.
  --
  -- Re-cutting the ring further in would work and is still the wrong answer, because there is
  -- nothing here to search FOR. The wrench is Kontakt's own chrome at a fixed place on a panel
  -- of fixed width that hangs off the right edge of the window: the template matched 650 px
  -- from the right at 1010 px of width, and 650 px from the right at 664. A coordinate is
  -- exact, costs no capture, and cannot be broken by whatever a library paints beside it.
  --
  -- Its STATE now comes from the view probe rather than from the button's own appearance, which
  -- is the other thing the image did badly: two templates, one per state, each library-specific.
  ov:addCustomButton({
    label = "Instrument editor",
    hotkey = "Alt+E",
    when = cell.inClassic,
    text = function(o)
      if not cell.inEdit then return nil end
      return cell.inEdit(o) and "open, press to close" or "closed, press to open"
    end,
    onActivate = function(o)
      local origin = o:origin()
      if not (origin and origin.client) then return end
      local w = O.contentSize(origin)
      if w <= WRENCH_FROM_RIGHT then
        host.speech.output("The instrument editor button is off screen", { interrupt = true })
        return
      end
      host.input.click(origin.client.x + w - WRENCH_FROM_RIGHT, origin.client.y + WRENCH_Y)
    end,
  })
  -- Offered only while that view is up. The coordinates are right for it, but in the other
  -- view the same points fall inside whatever library is loaded — one of them sat on
  -- Cinematic Studio Strings' "Sustain", so Ctrl+P changed the articulation instead of the
  -- instrument. ReaHotkey gates them the same way (`If This.GetPluginView() = "rack"`).
  -- `inRack`, not `inClassic`: the instrument EDITOR also answers "classic" to the view probe,
  -- and every one of these addresses a rack it has replaced. Offering them there was the
  -- defect — a control that announces itself and then clicks into someone else's panel.
  ov:group(cell.inRack, function(ov)
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
  addStatusToggles(ov, cell)
  addResizeControls(ov, cell)
  addEditSections(ov, cell)
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
