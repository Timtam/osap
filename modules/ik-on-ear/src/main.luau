-- IK Multimedia ARC ON:EAR — the desktop application that manages the headphones.
--
-- A JUCE application, and the probe of a real session says something unusual for one: it
-- exposes 53 named accessibility elements with positions. Almost all of them are static
-- text, so nothing can be *invoked* through the accessibility layer — but every label, every
-- read-out and every value is there by name, and so is where it sits.
--
-- That shapes the whole module. Nothing here is an authored coordinate. The tree is read
-- once per window and indexed, and a control finds itself by the words next to it. An
-- authored rectangle would be a promise about a window size, and this one has a maximise
-- button.

-- Split across src/ rather than kept in one file, because it had grown to nineteen hundred
-- lines and three of those panels are independent of each other. What lives where: the window
-- it attaches to and the slot they share in app.luau, the coordinate frame in geometry.luau,
-- the one tree read in tree.luau, the two chooser panels in browsers.luau, the settings panel
-- in settings.luau. This file is the main window and the wiring.

local O = host.require("com.platform.overlay")
local app = host.include("src/app.luau")
local geo = host.include("src/geometry.luau")
local read = host.include("src/tree.luau")

local ov = O.new("ARC ON:EAR")
local on_ear, ON_EAR_SLOT = app.window, app.SLOT
local DESIGN_W, DESIGN_H = geo.DESIGN_W, geo.DESIGN_H
local scaleOf, scaled, screenRect, screenPointOf =
  geo.scaleOf, geo.scaled, geo.screenRect, geo.screenPointOf
local dumpFor = read.dumpFor

local tree = O.memoByEpoch(function() return dumpFor(ov) end)

-- Everything named exactly `name`, in the order the tree reports it.
local function named(name)
  local out = {}
  for _, n in ipairs(tree() or {}) do
    if n.name == name then out[#out + 1] = n end
  end
  return out
end

-- The first element named `name`.
local function element(name)
  return named(name)[1]
end

-- The text sitting immediately to the RIGHT of a label, on the same line.
--
-- This application writes a read-out as two elements — the word "Battery Level" and the word
-- "100%" — and the second has no name of its own that a module could ask for, because its
-- name IS the value. So it is found by geometry: same row, nearest to the right. That is the
-- only way to read a value whose text changes, and it is why the tree is worth reading at
-- all rather than authoring coordinates.
local function valueRightOf(labelName)
  local label = element(labelName)
  if not label then return nil end
  local lb = label.bounds
  local best, bestDx = nil, nil
  for _, n in ipairs(tree() or {}) do
    local b = n.bounds
    -- Same row: the candidate's vertical middle falls inside the label's box.
    local sameRow = (b.y + b.h / 2) >= lb.y and (b.y + b.h / 2) <= (lb.y + lb.h)
    -- Measured from where the label STARTS, not from where its box ends. The boxes overlap
    -- here — "SR: " is reported 36 wide ending at 1556, and "192.0 kHz" starts at 1538,
    -- eighteen points inside it — so anything anchored to the label's right edge finds
    -- nothing at all. That is what made the sample rate read as "not shown", and it is the
    -- same trap the sforzando module recorded: these rectangles track the widget, not the ink.
    local dx = b.x - lb.x
    if sameRow and dx > 0 and n.name ~= "" and n.name ~= labelName then
      if not bestDx or dx < bestDx then
        best, bestDx = n, dx
      end
    end
  end
  return best and best.name or nil
end

-- Whatever is printed UNDER a label, sharing its middle.
--
-- The window puts a caption over its value in three places — Tone, Width, and the headphone
-- panel — and in all three the value is centred on the caption rather than aligned with its
-- left edge, which is why they are matched on their middles. The headphone panel stacks three
-- of them (maker, model, impedance) and they come back in the order they are printed.
local function valuesBelow(labelName, tol)
  local l = element(labelName)
  if not l then return {} end
  local mid = l.bounds.x + l.bounds.w / 2
  local out = {}
  for _, n in ipairs(tree() or {}) do
    if n.name ~= labelName and n.bounds.y > l.bounds.y then
      local m = n.bounds.x + n.bounds.w / 2
      if math.abs(m - mid) <= (tol or 12) then out[#out + 1] = n end
    end
  end
  table.sort(out, function(a, z) return a.bounds.y < z.bounds.y end)
  return out
end

local function valueBelow(labelName, tol)
  local v = valuesBelow(labelName, tol)[1]
  return v and v.name or nil
end

-- A read-out that speaks a label and whatever stands beside it.
--
-- Static text rather than a button, deliberately. There is nothing to press here, and a
-- control that announces itself as a button and then does nothing is the broken promise this
-- project keeps coming back to. Arriving at it reads the value; that is the whole control.
local function readOut(label, spoken)
  ov:addStaticText({
    label = spoken or label,
    text = function()
      return valueRightOf(label) or "not shown"
    end,
  })
end

-- ── Which curve view is showing ──────────────────────────────────────────────
--
-- Two dots at the top left, one filled. Measured with both states on screen at once, which is
-- the only reason this can be claimed: the chosen one is (237,237,237) and the other
-- (82,82,82), at the same x, sixteen apart in y.
for _, view in ipairs({
  { label = "Space view", y = 28 },
  { label = "Frequency response view", y = 50 },
}) do
  ov:addHotspotToggle({
    label = view.label,
    at = scaled(232, view.y),
    onColor = { 237, 237, 237 },
    offColor = { 82, 82, 82 },
  })
end

-- ── What the window says about the device ────────────────────────────────────
readOut("Battery Level", "Battery")
readOut("SR: ", "Sample rate")

-- The EQ band. These three are the only Edit controls in the whole window, and their names
-- ARE their values — which is why they are read directly rather than through `valueRightOf`.
local function bandValue(label)
  local l = element(label)
  if not l then return nil end
  local lb = l.bounds
  for _, n in ipairs(tree() or {}) do
    if n.ctype == host.uia.type.Edit then
      local b = n.bounds
      local sameRow = (b.y + b.h / 2) >= lb.y and (b.y + b.h / 2) <= (lb.y + lb.h)
      if sameRow and b.x >= lb.x then return n.name end
    end
  end
  return nil
end

for _, band in ipairs({ "Freq", "Gain", "Q" }) do
  ov:addStaticText({
    label = band,
    text = function()
      return bandValue(band) or "not shown"
    end,
  })
end

-- ── The header ───────────────────────────────────────────────────────────────
--
-- Both open something over the window, which they have to declare: the moment a menu is up
-- this overlay must stop owning Tab and Return, or the thing it just opened cannot be used.
--
-- The device chooser is clicked on its own name rather than on the small chevron beside it —
-- the name is an element with a rectangle, the chevron is nine pixels of painted glyph.
ov:addCustomButton({
  label = "Device",
  typeLabel = "combo box",
  opensMenu = true,
  text = function()
    local n = element("ARC ON EAR")
    return n and n.name or nil
  end,
  onActivate = function()
    local n = element("ARC ON EAR")
    if not n then
      host.speech.output("the device name is not where it should be", { interrupt = true })
      return
    end
    host.input.click(n.bounds.x + n.bounds.w // 2, n.bounds.y + n.bounds.h // 2)
  end,
})

ov:addHotspotButton({ label = "Settings", at = scaled(1753, 42), opensMenu = true })

-- ── The five preset slots ────────────────────────────────────────────────────
--
-- Clickable, and the only interactive things in this window whose position the tree gives
-- away without a measurement: each slot's second line is an element of its own, sitting
-- inside the slot. Clicking it hits the slot.
--
-- They are told apart by their vertical order, not by their names, because all five are
-- called "Default preset" until somebody saves over one — and a module that picked the first
-- match would switch to preset 1 whichever slot the user asked for.
-- The slots are found as a COLUMN, not by size.
--
-- The first attempt took every text element on the right-hand side between 80 and 140 points
-- wide, which is also true of "Battery Level" and "192.0 kHz" — so the list came out seven
-- long and Preset 1 was the battery reading. Width was never the property that distinguished
-- them.
--
-- What does: the five slots are the only elements in this window that share an exact left
-- edge with four others. Group the right-hand elements by their x and take the largest
-- group, requiring at least three, so a pair of unrelated labels that happen to line up
-- cannot be mistaken for the column. Nothing here is a measured number — the grouping finds
-- the column wherever it is and however wide the window has been dragged.
local function presetSlots()
  local o = ov:origin()
  if not (o and o.client) then return {} end
  local columns = {}
  for _, n in ipairs(tree() or {}) do
    if n.ctype == host.uia.type.Text and n.name ~= ""
      and (n.bounds.x - o.client.x) > 1000
    then
      local col = columns[n.bounds.x]
      if not col then
        col = {}
        columns[n.bounds.x] = col
      end
      col[#col + 1] = n
    end
  end
  local best = {}
  for _, col in pairs(columns) do
    if #col > #best then best = col end
  end
  if #best < 3 then return {} end
  table.sort(best, function(a, b) return a.bounds.y < b.bounds.y end)
  return best
end

-- -- Which curves the graph is drawing -----------------------------------------
--
-- Four dots above the frequency plot, each drawn in the colour of the curve it turns on: a
-- filled disc when that curve is shown, an empty ring when it is not. The probe point is the
-- centre of the dot, which is the fill when on and the page behind it when off.
--
-- Pre and Post were captured filled, so both of their colours are measured. User Trim and
-- Virtual Speaker were captured empty, and their fill colours are therefore a guess - named
-- here as the two that are known, so that if either turns out to be drawn in a third colour
-- the runtime refuses to name its state and writes a line saying so, rather than announcing
-- the nearest of two colours neither of which it is.
local PAGE = { 24, 24, 24 }
local CURVE_BLUE, CURVE_ORANGE = { 62, 133, 202 }, { 207, 107, 78 }

local curves = {
  { label = "Pre curve", x = 158, on = CURVE_BLUE },
  { label = "Post curve", x = 224, on = CURVE_ORANGE },
  { label = "User trim curve", x = 296, on = { CURVE_BLUE, CURVE_ORANGE } },
  { label = "Virtual speaker curve", x = 389, on = { CURVE_BLUE, CURVE_ORANGE } },
}

for _, c in ipairs(curves) do
  ov:addHotspotToggle({
    label = c.label,
    at = scaled(c.x, 118),
    onColor = c.on,
    offColor = PAGE,
  })
end

-- -- FN Button Assignment -------------------------------------------------------
--
-- What the button on the headphones themselves does. Seven choices, exactly one of them
-- filled with the application orange - a radio group, so it is offered as seven switches of
-- which one says "on", which is what a radio group sounds like anyway.
--
-- Probed low inside the button rather than at its centre. The centre is where the icon and
-- the two lines of label are, and a single pixel there reads whichever stroke happens to
-- cross it; six pixels above the bottom edge is empty in every one of the seven, which was
-- checked against all seven rather than assumed from one.
--
-- The names are read off the buttons, not guessed: six of the seven appear in the probe's OCR
-- at exactly these x positions, and the seventh was read from the capture itself.
local FN_ON, FN_OFF = { 225, 98, 59 }, PAGE

local fnButtons = {
  { label = "FN Phase Align", x = 593 },
  { label = "FN Virtual Speaker", x = 715 },
  { label = "FN Mono", x = 836 },
  { label = "FN Dim", x = 958 },
  { label = "FN Mute", x = 1082 },
  { label = "FN LED Off", x = 1202 },
  { label = "FN Play Pause", x = 1328 },
}

for _, b in ipairs(fnButtons) do
  ov:addHotspotToggle({
    label = b.label,
    at = scaled(b.x, 589),
    onColor = FN_ON,
    offColor = FN_OFF,
  })
end

-- -- The five panel switches ----------------------------------------------------
--
-- Rebuilt after the first test, and the interesting part is what the test found. Two of the
-- five reported no state, and the refusal lines said what they had read: (119,119,119) and
-- (108,108,108), nowhere near either reference. Enlarging the reference list would have
-- silenced that. Looking at the five switches side by side instead showed why they differed,
-- and it was not a shade — it was a whole state nobody had accounted for.
--
-- A switch is a pill about 36 wide with a round knob at one end. It is drawn three ways:
--
--   off, live      a bright hollow ring on the LEFT over a (82,82,82) body
--   off, dimmed    a flat (71,71,71) knob on the LEFT over a (53,53,53) body
--   on             the body filled and the knob, light, on the RIGHT
--
-- Three of the five were dimmed, because this application greys a panel's contents until the
-- panel above it is switched on: Phase Align waits on Calibration, and Virtual Speakers and
-- Ambience wait on Studio Simulation. That is also the answer to the other thing the test
-- found — that stepping the speaker list did nothing. Nothing was wrong with the arrows.
-- The panel they sit in was switched off, and a greyed control ignores a click exactly as it
-- ignores a mouse.
--
-- Which is a thing this overlay has to SAY. Announcing "off" for a switch that will not
-- respond is the same failure as announcing the wrong state: the user presses it, hears "off"
-- again, and has no way to find out why. So availability is announced with the state.
--
-- The probe point moved too. It was on the knob, which is the one part of the pill that
-- MOVES between states and whose edge is antialiased. It is now 27 into the pill, three
-- quarters of the way across: the middle of the body when off and the middle of the knob when
-- on. Both are flat fills, and both were verified on all five.
local SWITCH_PROBE = 27

-- One pixel, four answers. The application draws a dimmed control by blending it halfway
-- into the page, so the same pixel that separates on from off also separates live from
-- dimmed.
--
-- Three of the four are measured off the capture. The fourth — a live switch that is on —
-- is derived rather than seen, because the capture caught no such switch: the dimmed orange
-- body (124,62,46) is the application orange (225,98,59) mixed half-and-half with the page
-- (24,24,24), the same factor on all three channels, and undoing that blend on the dimmed
-- knob (189,158,150) overshoots 255 on every channel. So the live knob is white. If it turns
-- out to be some other bright colour, this refuses to name the state and logs what it read,
-- which is how the three states above were found in the first place.
local SWITCH_LOOK = {
  { rgb = { 82, 82, 82 }, state = "off", live = true },
  { rgb = { 53, 53, 53 }, state = "off", live = false },
  { rgb = { 255, 255, 255 }, state = "on", live = true },
  { rgb = { 189, 158, 150 }, state = "on", live = false },
}

-- Nearest reference, but only when "nearest" means something.
--
-- Two guards, and they catch different mistakes. The MARGIN guard (twice as close to the
-- winner as to the runner-up) is what stops a pixel halfway between the two off-greys, 87
-- apart, from being called dimmed on a coin toss. The CEILING guard is what stops a pixel
-- that is nothing like any of them from being called anything: the page behind these panels
-- is (24,24,24), which is 87 from the nearest reference and further still from the rest, so
-- without a ceiling a probe that had drifted off the pill entirely would confidently report
-- a dimmed switch that was off.
local SWITCH_CEILING = 60

local function classifySwitch(p)
  if not p then return nil end
  local best, second
  for _, look in ipairs(SWITCH_LOOK) do
    local d = math.abs(p.r - look.rgb[1]) + math.abs(p.g - look.rgb[2]) + math.abs(p.b - look.rgb[3])
    if best == nil or d < best.d then
      second = best
      best = { d = d, look = look }
    elseif second == nil or d < second.d then
      second = { d = d, look = look }
    end
  end
  if best.d > SWITCH_CEILING then return nil end
  if second and best.d * 2 > second.d then return nil end
  return best.look
end

-- Each switch reads its own pixel at most once per pump epoch, so the state read, the
-- availability text and any gate that asks about the same switch share one screen touch
-- rather than paying for three. A screen touch costs a compositor frame whatever it reads.
local function switchReader(name, dx, dy)
  return O.memoByEpoch(function()
    local o = ov:origin()
    local k = scaleOf(o)
    if not k then return nil end
    local x = math.floor(o.client.x + o.client.w / 2 + k * (dx - DESIGN_W / 2) + 0.5)
    local y = math.floor(o.client.y + k * dy + 0.5)
    local px = host.screen.pixel(x, y)
    local look = classifySwitch(px)
    if not look then
      host.log.info(("[on-ear] '%s' at (%d,%d) reads %s — no state this resembles")
        :format(name, x, y, px and ("%d,%d,%d"):format(px.r, px.g, px.b) or "nothing"))
      return nil
    end
    return look
  end)
end

-- `needs`: the switch that has to be on before this one will answer. Someone who cannot see
-- the screen has no way to discover that from a control that only says "unavailable" — they
-- would have to switch things on one at a time until the right one freed it. Saying which one
-- turns a search into a single step.
--
-- Verified for Virtual Speakers, where the tester switched Studio Simulation on and watched it
-- come alive. The other two are read off the same rule: each dimmed switch sits inside the
-- panel whose own header switch is off, and this application dims a panel's contents while its
-- header is off. If either is wrong the switch is still genuinely unavailable and the sentence
-- is merely unhelpful, never false — and one test says so.
local switches = {
  { label = "Calibration", x = 370 + SWITCH_PROBE, y = 651 },
  { label = "Phase Align", x = 370 + SWITCH_PROBE, y = 968, needs = "Calibration" },
  { label = "Studio Simulation", x = 709 + SWITCH_PROBE, y = 651 },
  { label = "Ambience", x = 707 + SWITCH_PROBE, y = 968, needs = "Studio Simulation" },
  { label = "Virtual Speakers", x = 1048 + SWITCH_PROBE, y = 650, needs = "Studio Simulation" },
}

-- Kept by label so the Virtual Speakers panel can ask whether its own switch is reachable
-- before offering to press anything inside it.
local switchLook = {}

for _, sw in ipairs(switches) do
  local read = switchReader(sw.label, sw.x, sw.y)
  switchLook[sw.label] = read
  ov:addHotspotToggle({
    label = sw.label,
    at = scaled(sw.x, sw.y),
    -- The runtime reads the state from these two lists; the module reads availability from
    -- the same pixel. Both halves of the classification therefore come from one measurement,
    -- and cannot disagree about what is on the screen.
    onColor = { { 255, 255, 255 }, { 189, 158, 150 } },
    offColor = { { 82, 82, 82 }, { 53, 53, 53 } },
    text = function()
      local look = read()
      if look and not look.live then
        return sw.needs and ("needs " .. sw.needs .. " on") or "unavailable"
      end
      return nil
    end,
  })
end

-- ── The two things that shape the sound ──────────────────────────────────────
--
-- Steppers, driven by the wheel over the control itself. Neither is a control this overlay can
-- read a position out of — Tone is an arc and Width a bar with two handles — but both print
-- their value beside them, and what a stepper announces is that printed value read again
-- after the change, never the change that was asked for.
--
-- The wheel is a guess about how the application takes input, and the honest kind: if it is
-- wrong, the value simply does not move and the control says the same number back, which is
-- the truth. Nothing is claimed that was not read.
--
-- Both appear only while the panel above them is switched on, which is also when their labels
-- appear in the tree at all — so the same test serves for both.
ov:addStepper({
  label = "Tone",
  when = function() return element("Tone") ~= nil end,
  text = function() return valueBelow("Tone") or "not shown" end,
  -- Half a notch. One whole one moves this knob five of its own units, which is a big step to
  -- take blind when the value is the tone of everything you are about to listen to. If the
  -- application rounds the half away it will simply not move, and the control will say the
  -- same number back — which is the truth, and tells us to stop asking for halves.
  onStep = function(dir)
    local x, y = screenPoint(290, 780)
    if x then host.input.scroll(x, y, dir * 0.5) end
  end,
})

-- Width READS. It does not adjust, and that is a statement about this control rather than a
-- gap somebody forgot to fill.
--
-- Five mechanisms were tried against it, and the geometry behind them is measured rather than
-- supposed — three captures of the bar, at ten, forty-eight and a hundred per cent, agreeing
-- on where the handles sit to within a third of a pixel:
--
--   the wheel                 moves the Tone knob beside it perfectly; this one ignores it
--   an instant drag           moved it eighteen per cent for two pixels, then not at all
--   a paced drag from Lua     the timers proved far coarser than the pacing asked for
--   a paced drag from Rust    injected movement, real duration, three pixels of travel: nothing
--   click to focus, then      the arrow key posted straight to the window, past our own
--   an arrow key              hook: nothing
--
-- Everything checkable from outside is right, and it does not move. What this control wants
-- from a mouse is not discoverable by trying another variation of the same gesture, and every
-- variation costs a person a test round to find out.
--
-- So it says what it says, honestly, and the mouse is how it gets changed. A control that
-- announces itself as a slider and then ignores every key is a promise this project does not
-- make; static text is what this is, and static text is what it now claims to be. Should
-- somebody ever learn what it responds to, `addStepper` is still there and this becomes four
-- lines again.
ov:addStaticText({
  label = "Width",
  when = function() return element("Width") ~= nil end,
  text = function()
    local v = valueBelow("Width")
    return v and (v .. ", adjust with the mouse") or "not shown"
  end,
})

-- -- Choosing the speakers being simulated --------------------------------------
--
-- The name and its description are painted text with no element behind them, so they are read
-- from the picture. The read-out is a separate stop in the ring so that arriving at it says
-- what is loaded without changing anything, and the arrows either side step the list.
--
-- All four are dead while the panel is greyed, which is why they are custom buttons rather
-- than plain hotspots: a hotspot would click regardless, and this overlay does not press
-- things that cannot answer. They say so instead, and the switch above them is what fixes it.
-- What, if anything, is stopping this panel from answering — and the first version of this
-- had it wrong in a way worth recording.
--
-- It asked only whether the panel was DIMMED, and so the arrows announced themselves as usable
-- the moment Studio Simulation came on, while Virtual Speakers itself was still off. The
-- tester found it immediately: the buttons pressed, the log shows four clicks at (1072,914),
-- and nothing moved. Two conditions, not one — the panel has to be reachable AND its own
-- switch has to be on — and the single pixel above answers both.
--
-- Returns the reason it will not work, or nil when it will. An unreadable switch is its own
-- answer and NOT a reason to go ahead: if the pixel where that switch should be cannot be
-- recognised, the window is not where this module believes it is, and the arrow coordinates
-- rest on exactly the same belief.
local function speakerPanelBlocker()
  local read = switchLook["Virtual Speakers"]
  local look = read and read()
  if not look then return "cannot tell whether Virtual Speakers is on" end
  if not look.live then return "needs Studio Simulation on" end
  if look.state ~= "on" then return "needs Virtual Speakers on" end
  return nil
end

local function panelButton(label, dx, dy)
  ov:addCustomButton({
    label = label,
    -- Announced on ARRIVAL, not after a fruitless press. Hearing "unavailable" and having to
    -- press the button to find out what would fix it costs a step that the person tabbing has
    -- no way to skip.
    text = speakerPanelBlocker,
    onActivate = function()
      local why = speakerPanelBlocker()
      if why then
        host.speech.output(label .. " " .. why, { interrupt = true })
        return
      end
      local x, y = screenPoint(dx, dy)
      if not x then return end
      host.log.info(("[on-ear] '%s' clicking (%d,%d)"):format(label, x, y))
      host.input.click(x, y)
    end,
  })
end

ov:addStaticText({
  label = "Virtual speaker",
  -- Read in every state, including while the panel is greyed. An earlier version answered
  -- "unavailable" here, which was the wrong kind of caution: this control changes nothing, the
  -- name it reads is true whether or not the simulation is using it, and the probe's OCR
  -- picked the words out of the dimmed panel perfectly well. The switch two stops earlier
  -- already says whether it is in use; repeating that here would only cost the name.
  text = function()
    local o = ov:origin()
    local r = screenRect(o, 865, 876, 1065, 945)
    if not r then return "not shown" end
    local got = host.ocr.recognize({ region = r })
    local text = (got and got.text or ""):gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "")
    if text == "" then return "not shown" end
    return text
  end,
})

-- Measured from the capture rather than estimated off a thumbnail, after the first test
-- reported that they did nothing: the left chevron's strokes occupy x 852-857 and the right
-- one's 1070-1074, both spanning y 886-896. They were already right, which is how the panel
-- turned out to be the answer.
panelButton("Previous virtual speaker", 855, 891)
panelButton("Next virtual speaker", 1072, 891)

-- Deliberately NOT behind the gate above, and measured rather than assumed. In the capture the
-- whole Virtual Speakers panel was greyed — its header sits at brightness 119 against 232 for
-- a live one — and yet the glyphs of SELECT SPEAKERS have exactly the distribution of
-- SELECT HEADPHONE next door, whose panel was never dimmed: background at 24, text topping out
-- at 163 in both. The application keeps this one button alive, which is the sensible design,
-- because choosing the speakers is how you decide what to switch on. Gating it with the arrows
-- would have taken away the step that comes first.
ov:addHotspotButton({ label = "Select speakers", at = scaled(965, 970) })

-- What is loaded, in the three lines the panel prints: maker, model, impedance. Read from the
-- tree rather than from the picture, because once a headphone is chosen they are all there as
-- elements — "AKG", "K812", "36 Ohm" — and a reading that costs nothing is better than one
-- that costs a screen capture.
ov:addStaticText({
  label = "Headphone",
  text = function()
    local vs = valuesBelow("Headphone Model", 20)
    if #vs == 0 then return "none selected" end
    local parts = {}
    for _, v in ipairs(vs) do parts[#parts + 1] = v.name end
    return table.concat(parts, ", ")
  end,
})

ov:addHotspotButton({ label = "Select headphone", at = scaled(1306, 970) })

for i = 1, 5 do
  ov:addCustomButton({
    label = ("Preset %d"):format(i),
    -- Announced with the slot's own second line, so arriving at a preset says what is in it
    -- before anything is pressed. A slot list where every entry says only its number is a
    -- list you have to load from to read.
    text = function()
      local slot = presetSlots()[i]
      return slot and slot.name or "not shown"
    end,
    onActivate = function()
      local slot = presetSlots()[i]
      if not slot then
        -- Said rather than clicked. Not finding the slot means the window is not laid out
        -- the way this module believes, and clicking where it *should* be would be pressing
        -- something unknown on somebody's audio device.
        host.speech.output(("preset %d is not where it should be"):format(i))
        return
      end
      host.input.click(
        slot.bounds.x + slot.bounds.w // 2,
        slot.bounds.y + slot.bounds.h // 2
      )
    end,
  })
end

-- Something this window's tree carries whenever the tree works at all.
--
-- Used by every panel here to tell "the thing I am looking for is gone" apart from "the
-- application has stopped answering questions", which it does whenever a caret is in one of
-- its text fields. One spelling of one fact, because two spellings of one question is how the
-- gate and the Close check came to disagree earlier in this file.

-- The panels. Each attaches itself to the shared slot as it is included, and each outranks
-- this overlay, so whichever of them is on screen owns the window and this one stands down.
host.include("src/browsers.luau")
host.include("src/settings.luau")

ov:attach(on_ear, { menus = true, slot = ON_EAR_SLOT, specificity = O.layer.content })
