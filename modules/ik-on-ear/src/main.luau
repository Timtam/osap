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

local O = host.require("com.platform.overlay")
local ov = O.new("ARC ON:EAR")

-- The window. Identified by the executable rather than the window class: the class is
-- `JUCE_1a05739a091`, and that hexadecimal tail is generated per build of the JUCE runtime —
-- matching on it would work today and break at the next update, silently, in the way that is
-- hardest to diagnose from the outside. The title carries a middle dot (ARC ON·EAR), so both
-- matches deliberately stop before it and stay ASCII.
local on_ear = {
  app = { exe = { contains = "ARC ON" } },
  windows = { class = { contains = "JUCE_" } },
}

-- One arbiter slot for this window. The main overlay and the chooser panels all match it,
-- and the most specific one that is actually on screen wins.
local ON_EAR_SLOT = "com.platform.ik-on-ear/window"

-- ── The accessibility tree, read once and indexed ────────────────────────────
--
-- Read fresh, every time, and no cache anywhere in this module.
--
-- It did have one, and the story is worth keeping because the lesson is not about this
-- module. A walk of this 53-element window cost **211 ms**, every Tab paid it before a word
-- was spoken, and the module grew a snapshot, a staleness rule and an exception for the two
-- read-outs that change on their own. All of that was compensation for a defect one layer
-- down: the backend was asking the target process for a name, a class, a type and a
-- rectangle one property at a time. Asking for them all at once — a UIA cache request —
-- brought the same walk to about **30 ms**, and every one of those compensations became a
-- complication with nothing left to justify it.
--
-- So they are gone, and what is left is simpler AND more correct: nothing here can be stale,
-- including the positions, which the cached version would have kept believing across a
-- window resize the overlay did not notice.
--
-- The timing stays. It is what turned "there is a noticeable delay" into a number, and it is
-- what will say so again if this module ever meets a window with ten times the elements.
local slowestTree = 0
local function dumpFor(overlay)
  local o = overlay:origin()
  if not (o and o.id) then return nil end
  local started = host.now()
  local dump = host.uia.rawDump(o.id)
  local took = host.now() - started
  if took > slowestTree then
    slowestTree = took
    host.log.info(("[on-ear] reading the accessibility tree took %d ms (%d elements)")
      :format(took, #(dump or {})))
  end
  return dump
end

-- Memoised per EPOCH, which is not the cache this file used to have and argued its way out
-- of. That one had a time-to-live, so it could hand back a picture of the window from before
-- something the user had just done. An epoch is one pump of the host: every read inside it
-- happens between two of the user's actions, so a shared answer cannot be stale, and the
-- second control to ask in the same breath asks for free.
--
-- It matters more now than it did. A single announcement in the chooser panel below wants the
-- selected model, its description and the row that is highlighted — three questions of the
-- same tree, and without this, three crossings of a process boundary to answer them.
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

-- == The parts of this window that are only a picture =========================
--
-- Everything above this line came out of the accessibility tree. Everything below it is drawn
-- by JUCE and reaches the tree as, at most, a label sitting next to the thing it names - the
-- switches, the seven assignment buttons and the curve check-marks are not in it at all.
--
-- Which raises the question the tester answered by dragging the window about: what happens to
-- a coordinate when the window is resized. Measured, from two probes of the same window at
-- 1920x1009 and at 826x481, ON:EAR scales its whole interface **uniformly by the height** and
-- **centres it horizontally**. Not a reflow, and not two independent axes. The model
--
--     scale = content height / 1009
--     x     = content width / 2 + scale * (design x - 960)
--     y     = scale * design y
--
-- predicts 43 of the 47 elements the two probes share to within one and a half pixels. The
-- four it misses are the title bar, the system menu, and the minimise and close buttons -
-- drawn by Windows rather than by JUCE, and so rightly not scaling with the rest.
--
-- So nothing below is a pixel. Every number is a coordinate in the frame it was measured in,
-- resolved against whatever size the window happens to be now. A fixed offset from a nearby
-- label would have been no better, because the offset scales too.
local DESIGN_W, DESIGN_H = 1920, 1009

local function scaleOf(o)
  if not (o and o.client and o.client.h and o.client.h > 0) then return nil end
  return o.client.h / DESIGN_H
end

local function scaled(dx, dy)
  return function(self)
    local o = self:origin()
    local k = scaleOf(o)
    if not k then return nil end
    return {
      math.floor(o.client.w / 2 + k * (dx - DESIGN_W / 2) + 0.5),
      math.floor(k * dy + 0.5),
    }
  end
end

-- The same model, in screen coordinates, for the one thing here that has to be read rather
-- than pressed. An OCR region cannot be a function in the runtime, so this control does its
-- own reading instead of pretending its region is fixed.
local function screenRect(o, x1, y1, x2, y2)
  local k = scaleOf(o)
  if not k then return nil end
  local cx, cy = o.client.x, o.client.y
  local half = o.client.w / 2
  return {
    math.floor(cx + half + k * (x1 - DESIGN_W / 2) + 0.5), math.floor(cy + k * y1 + 0.5),
    math.floor(cx + half + k * (x2 - DESIGN_W / 2) + 0.5), math.floor(cy + k * y2 + 0.5),
  }
end

-- Absolute screen point for a design coordinate, for a control that does its own clicking.
-- Takes the overlay because two of them share this window now.
local function screenPointOf(overlay, dx, dy)
  local o = overlay:origin()
  local k = scaleOf(o)
  if not k then return nil end
  return math.floor(o.client.x + o.client.w / 2 + k * (dx - DESIGN_W / 2) + 0.5),
    math.floor(o.client.y + k * dy + 0.5)
end

local function screenPoint(dx, dy)
  return screenPointOf(ov, dx, dy)
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

-- == The chooser panels ======================================================
--
-- Pressing "Select speakers" opens a Speaker Browser, and the tester found out the hard way
-- what that meant: it is not a window. It is a pane the application draws INSIDE its own
-- window, at (270,128) and 1380x799 of it, so nothing about the window changed, the main
-- overlay stayed active over a layout that was no longer there, and there was no way out of
-- it — the one control that closes it was not in any ring.
--
-- So it is its own overlay, on the shared slot, gated on the pane being present. That gives
-- three things at once: the main ring stands down while the panel is up, this ring appears
-- with Close as its first stop, and neither has to know anything about the other.
--
-- Written as a function because there are two of these. The Headphone Model panel has the
-- same Select button, and its chooser is almost certainly the same pane with a different
-- name — but "almost certainly" is not a measurement, so only the one that has been probed
-- is built here. The other is a handful of lines once there is a capture of it.
local function chooser(spec)
  local b = O.new(spec.name)

  -- The pane's presence, and nothing more, decides whether this overlay is live. Asked with
  -- a targeted UIA find rather than the whole-tree read the rest of this module uses: a gate
  -- is re-evaluated on every recheck, whether or not anybody pressed anything, and a tree
  -- walk on that path would be a cost this window pays continuously for a panel that is
  -- almost never open.
  -- VISIBLE, not merely present — and never guessed from silence.
  --
  -- Two bugs are answered here, found one after the other, and the second is the interesting
  -- one.
  --
  -- The first version asked whether an element named SpeakerBrowserWindow EXISTED. It kept
  -- answering yes long after the panel had closed — three "still on screen after Close" lines
  -- against probes of the same window listing no such element — and then claimed the window on
  -- every launch, so the application opened into a ring for a panel that was never there. A UIA
  -- provider may keep a hidden component in its tree, and this one does. `locate` is the call
  -- that knows the difference: it refuses a rectangle that has collapsed or gone off-screen.
  --
  -- Then clicking into the search field made the panel "disappear" while it was plainly still
  -- on screen. It had not: putting the keyboard focus in that text box makes ON:EAR drop its
  -- ENTIRE accessibility tree. The probe is unambiguous — 77 elements with the panel open, and
  -- 6 a moment later with the same panel open and the caret in Search, those 6 being the window
  -- and the Windows title bar. Nothing of the application's own is left to find.
  --
  -- Reading that as "the panel closed" is what did the damage: this overlay stood down and the
  -- window overlay took over, with all 31 of its controls underneath a panel that covers them.
  -- Pressing "FN Mute" would then have clicked a speaker tile.
  --
  -- So absence of an answer is not an answer. When the panel cannot be found, a second question
  -- decides which kind of silence it is: something that is in this window's tree whenever the
  -- tree works at all. If that is missing too, the application has stopped answering, nothing
  -- has been learned, and the previous belief stands. The extra question is asked only when it
  -- can change something — the panel was believed open and has just gone missing — so the
  -- steady state still costs one lookup.
  local TREE_SENTINEL = "Battery Level"

  local paneWasThere = nil
  -- Set by the gate below, and it is the same measurement twice over: this application stops
  -- answering questions about itself precisely while the caret is in its search box. The gate
  -- needs to know so it does not read silence as "the panel closed"; the keyboard needs to
  -- know so the overlay stops holding keys that belong to the typing.
  local treeDark = false

  local function readPane(origin)
    if not (origin and origin.id) then return false, "no window" end
    local at = host.uia.locate(origin.id, spec.pane, host.uia.type.Window)
    if at then
      treeDark = false
      return true, ("visible at (%d,%d)"):format(at.x, at.y)
    end
    if not paneWasThere then return false, "not there" end
    if host.uia.locate(origin.id, TREE_SENTINEL, host.uia.type.Text) then
      treeDark = false
      return false, "gone, and the rest of the tree is still readable"
    end
    treeDark = true
    return true, "unknown: the window is answering nothing at all, so the last answer stands"
  end

  -- Hold nothing while the search box has the keyboard. Naming individual keys — Space here,
  -- something else next time — leaves every key nobody thought of still swallowed.
  b:typingWhen(function() return treeDark end)

  b:gate(function(origin)
    local there, why = readPane(origin)
    if there ~= paneWasThere then
      paneWasThere = there
      host.log.info(("[on-ear] %s: '%s' %s — %s")
        :format(spec.name, spec.pane, there and "is on screen" or "is gone", why))
    end
    return there
  end)

  local btree = O.memoByEpoch(function() return dumpFor(b) end)

  local function bElement(name)
    for _, n in ipairs(btree() or {}) do
      if n.name == name then return n end
    end
    return nil
  end

  -- Whether the panel is on screen at all — the same question the gate asks, asked again
  -- here because a control that closes something should be able to check that it did.
  -- Literally the gate's own question. Asking it a different way was how the two came to
  -- disagree in the first place.
  local function paneThere()
    return (readPane(b:origin()))
  end

  -- Bring the window forward, then click.
  --
  -- Insurance, not a fix for anything observed: every click this project makes is a blind one
  -- at a screen coordinate, and nothing anywhere checks that our window is the one lying under
  -- that point — `refuseOutside` only asks whether the point falls inside our window's
  -- RECTANGLE, which says nothing about what is drawn on top of it. Focusing first costs
  -- nothing when we are already in front, which is the normal case.
  --
  -- It is not the whole answer either, since a window can sit on top without holding focus.
  -- That answer is to ask the OS which window owns the point, and it belongs in the runtime
  -- where every module's clicks would get it, not bolted onto this one.
  local function clickAt(x, y)
    local o = b:origin()
    if o and o.id and host.window and host.window.focus then host.window.focus(o.id) end
    host.input.click(x, y)
  end

  local function clickElement(n)
    if not n then return false end
    clickAt(n.bounds.x + n.bounds.w // 2, n.bounds.y + n.bounds.h // 2)
    return true
  end

  -- The value under a column heading. MODEL and DESCRIPTION are headings with their text
  -- directly beneath, sharing the heading's left edge — which is how they are found, rather
  -- than by a coordinate, because that survives the panel being laid out differently.
  local function underLabel(nodes, name)
    local head
    for _, n in ipairs(nodes) do
      if n.name == name then
        head = n
        break
      end
    end
    if not head then return nil end
    local best
    for _, n in ipairs(nodes) do
      if n.name ~= name and math.abs(n.bounds.x - head.bounds.x) <= 6
        and n.bounds.y > head.bounds.y
      then
        if best == nil or n.bounds.y < best.bounds.y then best = n end
      end
    end
    return best and best.name or nil
  end

  -- The bar along the bottom, read as whatever columns this panel happens to have. The
  -- speaker chooser shows a model and a description; the headphone chooser shows a
  -- manufacturer, a model, a type, an impedance and a safe headroom. Naming the headings in
  -- the spec is the difference between one function and two.
  local function selectionFrom(nodes)
    if not nodes then return nil end
    local parts = {}
    for _, heading in ipairs(spec.headings) do
      local v = underLabel(nodes, heading)
      if v then parts[#parts + 1] = v end
    end
    if #parts == 0 then return nil end
    return table.concat(parts, ", ")
  end

  -- Shared within an announcement.
  local function selection()
    return selectionFrom(btree())
  end

  -- Deliberately NOT shared. A watch that waits for a value to change cannot be served from
  -- a memo of what the value was, and an epoch may not turn over between two ticks of a
  -- timer — so this one always crosses the boundary and always sees now.
  local function selectionNow()
    return selectionFrom(dumpFor(b))
  end

  -- The value sitting to the right of a heading on the same line — how the headphone panel's
  -- Type and Impedance menus report what they are set to. Both of their values are named
  -- "All" when nothing is chosen, so they can only be told apart by which heading they sit
  -- beside, never by their own name.
  local function rightOf(labelName)
    local head = bElement(labelName)
    if not head then return nil end
    local best
    for _, n in ipairs(btree() or {}) do
      if n.name ~= labelName and n.bounds.x > head.bounds.x
        and math.abs((n.bounds.y + n.bounds.h / 2) - (head.bounds.y + head.bounds.h / 2)) < 20
      then
        if best == nil or n.bounds.x < best.bounds.x then best = n end
      end
    end
    return best
  end

  for _, dd in ipairs(spec.dropdowns or {}) do
    b:addCustomButton({
      label = dd.label,
      typeLabel = "combo box",
      -- Declared, because the overlay cannot tell: the moment a menu is up it must stop
      -- owning Tab and Return, or the menu it just opened cannot be used.
      opensMenu = true,
      when = function() return rightOf(dd.label) ~= nil end,
      text = function()
        local v = rightOf(dd.label)
        return v and v.name or nil
      end,
      onActivate = function()
        if not clickElement(rightOf(dd.label)) then
          host.speech.output(dd.label .. " is not where it should be", { interrupt = true })
        end
      end,
    })
  end

  -- A row's own fill says whether it is the filter in force: (46,46,46) when it is against
  -- (26,26,26) when it is not. Both measured, from two captures of this panel taken in the
  -- two states — which is the only reason this can be claimed at all, and why it was not
  -- claimed an hour ago when only one of them existed.
  --
  -- Probed 170 into the row, in the gap between the name and its count, where the fill runs
  -- flat for seventy-five pixels in both states. Anywhere over the text would have been a
  -- reading of a glyph stroke, which is neither state.
  local ROW_ON, ROW_OFF = { 46, 46, 46 }, { 26, 26, 26 }
  local ROW_PROBE_DX = 170

  local function rowPoint(r)
    local o = b:origin()
    if not (o and o.client and r) then return nil end
    return {
      r.bounds.x - o.client.x + ROW_PROBE_DX,
      r.bounds.y - o.client.y + r.bounds.h // 2,
    }
  end

  -- The rows the panel is actually showing, top to bottom.
  --
  -- This is the whole answer to a list that would not scroll past Creative, and the log spells
  -- it out: seventeen rows offered, then fifteen, then eight, then two, then none, with the
  -- last name always Creative. The count was falling because "Row 15" is not the fifteenth row
  -- ON SCREEN — it is the fifteenth maker in the list, and it exists as an element only while
  -- that maker is rendered. Twenty slots asking for "Row 1" to "Row 20" were therefore asking
  -- for Adam Audio to Creative and nothing else, for ever, however far the list travelled.
  --
  -- So the slots are positions and the rows are looked up by where they are: everything named
  -- "Row <n>" that the panel is showing, sorted down the screen. Slot one is whatever is at
  -- the top now.
  local visibleRows = O.memoByEpoch(function()
    local pane = bElement(spec.pane)
    local found = {}
    for _, n in ipairs(btree() or {}) do
      if string.match(n.name, "^Row %d+$") then
        local mid = n.bounds.y + n.bounds.h // 2
        if not pane or (mid >= pane.bounds.y and mid <= (pane.bounds.y + pane.bounds.h)) then
          found[#found + 1] = n
        end
      end
    end
    table.sort(found, function(a, z) return a.bounds.y < z.bounds.y end)
    return found
  end)

  -- Scrolling, for the two things here that hold more than they show.
  --
  -- The manufacturer list runs to far more makers than the rows the panel renders, and the
  -- tile grid to more headphones than its fifteen positions — and the bottom row of tiles has
  -- its printed name clipped away by the viewport, so even what IS shown cannot be read there.
  -- Neither is reachable by tabbing, because tabbing walks what is drawn.
  --
  -- Done with the wheel over the area itself, which is what a mouse would do, and each control
  -- says where the list has ARRIVED rather than only that it moved: after a scroll the rows
  -- and tiles hold different things, and "scrolled" on its own tells you nothing about what is
  -- now in front of you.
  --
  -- Deliberately not automatic, for now. Tabbing off the end and having the panel move
  -- underneath is a nicer thing to describe than to use without seeing it, and the runtime has
  -- no notion of "bring this control into view" to build it on honestly. That belongs in the
  -- runtime, where every module with a list longer than its window would get it.
  local function scroller(label, atx, aty, amount, after)
    b:addCustomButton({
      label = label,
      onActivate = function()
        local x, y = screenPointOf(b, atx, aty)
        if not x then return end
        host.log.info(("[on-ear] %s: %s at (%d,%d)"):format(spec.name, label, x, y))
        local o = b:origin()
        if o and o.id and host.window and host.window.focus then host.window.focus(o.id) end
        gridDirty = true
        host.input.scroll(x, y, amount)
        if not host.timer then return end
        host.timer.after(250, function()
          host.speech.output(after() or "no further", { interrupt = true })
        end)
      end,
    })
  end

  -- Out, first, and deliberately first. Someone who cannot see this panel and did not expect
  -- it needs the exit to be one keystroke away, not at the end of twenty-five stops.
  -- Says what HAPPENED, not what was attempted.
  --
  -- As a plain hotspot this said "Close, activated" the moment it clicked, which is a claim
  -- about the future. Being stuck in a panel is exactly the situation where the difference
  -- matters, and where somebody who cannot see the screen has nothing else to go on: a tool
  -- that reports success makes the next thing they doubt themselves. So it watches the panel
  -- for a second, and says so if it is still there.
  --
  -- It checks the pane directly rather than trusting this overlay to have noticed. The two are
  -- not the same question — the panel really did close once while this overlay carried on as
  -- though it had not — and this one is about the click.
  b:addCustomButton({
    label = "Close",
    onActivate = function()
      local x, y = screenPointOf(b, spec.close[1], spec.close[2])
      if not x then return end
      local q = host.screen.pixel(x, y)
      host.log.info(("[on-ear] '%s' Close clicking (%d,%d), pixel there %s")
        :format(spec.name, x, y, q and ("%d,%d,%d"):format(q.r, q.g, q.b) or "unreadable"))
      clickAt(x, y)
      if not host.timer then return end
      local tries = 12
      local function look()
        if not paneThere() then return end -- gone; the window overlay announces itself
        tries -= 1
        if tries <= 0 then
          host.log.info(("[on-ear] '%s' is still on screen after Close"):format(spec.pane))
          host.speech.output("the panel did not close", { interrupt = true })
          return
        end
        host.timer.after(100, look)
      end
      host.timer.after(100, look)
    end,
  })

  b:addStaticText({
    label = "Selected",
    text = function() return selection() or "not shown" end,
  })

  -- The search field. Activating drops the caret into it, so the next thing typed goes to the
  -- application and filters the grid — which is the one way to reach a named item here
  -- without pressing every tile to find out what it is.
  -- Found as "the edit box inside this panel", not by its name.
  --
  -- Its name was "Search" in the first capture and gone from the tree in the second, because
  -- that name is the PLACEHOLDER: once the field has content of its own, the placeholder is
  -- what it no longer says. A lookup by name therefore worked exactly until somebody used it.
  -- Its identity is that it is the one edit control in this pane, and the main window's three
  -- band edits are outside the pane's rectangle.
  -- The field is CLICKED at a measured coordinate and only READ through the tree, because
  -- the element is not reliably there.
  --
  -- Three captures of this panel: the first had `Search`, an Edit at (328,179) 242x44; the
  -- second and third had no edit control inside the pane at all. So it appears — most likely
  -- while it holds the keyboard focus it takes when the panel opens — and otherwise is simply
  -- absent. A lookup that finds it by name broke as soon as it was used, and a lookup that
  -- finds "the edit in this pane" breaks whenever it is not focused, which is most of the
  -- time. The coordinate is from the one capture that had it, and scales like everything else.
  local function searchField()
    local pane = bElement(spec.pane)
    if not pane then return nil end
    for _, n in ipairs(btree() or {}) do
      if n.ctype == host.uia.type.Edit
        and n.bounds.x >= pane.bounds.x and n.bounds.y >= pane.bounds.y
        and (n.bounds.x + n.bounds.w) <= (pane.bounds.x + pane.bounds.w)
        and (n.bounds.y + n.bounds.h) <= (pane.bounds.y + pane.bounds.h)
      then
        return n
      end
    end
    return nil
  end

  b:addCustomButton({
    label = "Search",
    -- An EDIT, and it must say so. It was announced as a button, which describes how it is
    -- built rather than what it is: someone hearing "button" has no reason to expect that
    -- pressing it puts a caret somewhere and that the next thing they type goes into the
    -- application.
    typeLabel = "edit",
    -- Which also has to be true of the KEYS, or the word is just a label on the wrong thing.
    -- Activating this leaves the caret in the application's text box, and the overlay was
    -- still holding Space — so a search for "White 80s" lost its space to a control press.
    -- `editable` is the statement the runtime already understood from an OCREdit, said by a
    -- control that reaches its field by coordinate instead of by region. Return stays ours,
    -- because it is the only way back into the field after tabbing away from it.
    editable = true,
    -- READ FROM THE PICTURE, not from the tree.
    --
    -- Through the tree it worked, but only afterwards: the tester's typing was announced back
    -- to him one speaker later, because putting the caret in this field is exactly what makes
    -- ON:EAR drop its accessibility tree — so while there is anything to read, there is
    -- nothing to read it with. The pixels do not care.
    text = function()
      local o = ov:origin() or b:origin()
      local rect = screenRect(o, 330, 186, 565, 214)
      if not rect then return nil end
      local got = host.ocr.recognize({ region = rect })
      local v = (got and got.text or ""):gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "")
      -- The placeholder is the word "Search" itself, which means empty, not content.
      if v == "" or v == "Search" then return nil end
      return v
    end,
    onActivate = function()
      -- The element when it is there (it is exact), the measured coordinate when it is not.
      if clickElement(searchField()) then return end
      local x, y = screenPointOf(b, spec.search[1], spec.search[2])
      if not x then return end
      host.log.info(("[on-ear] '%s' search clicking (%d,%d)"):format(spec.name, x, y))
      clickAt(x, y)
    end,
  })

  -- A button, not a toggle, and the distinction was measured rather than assumed. Its glyphs
  -- top out at exactly 163 — the same as SELECT SPEAKERS and SELECT HEADPHONE — while the
  -- category rows beside it reach 225. Those are two roles in this application's palette, a
  -- button and a list item, not two states of one control.
  --
  -- It is one end of a radio group though — Show All or one category, never both — and the
  -- second capture settled how to say so. Two captures in opposite states show this control
  -- looking identical in both, so it has no appearance of its own to read. What it does have
  -- is a complement: it is in force exactly when no category is. So the answer is derived from
  -- the rows, which do carry a measured state, rather than invented for this one.
  for _, f in ipairs(spec.filters) do
  b:addHotspotButton({
    label = f.label,
    at = scaled(f.at[1], f.at[2]),
    text = function()
      local sawRow = false
      local o = b:origin()
      if not (o and o.client) then return nil end
      -- The rows on screen, not the first eight in the model — and at most four of them.
      -- Each of these is a screen touch costing a compositor frame, and the question only
      -- needs an answer, not a census: one row reading "selected" settles it, and a filter
      -- that is in force is drawn at the top of the list where the first few are.
      local rows = visibleRows()
      for i = 1, math.min(#rows, 4) do
        local r = rows[i]
        local pt = r and rowPoint(r)
        if pt then
          local q = host.screen.pixel(o.client.x + pt[1], o.client.y + pt[2])
          if q then
            sawRow = true
            local don = math.abs(q.r - ROW_ON[1]) + math.abs(q.g - ROW_ON[2])
              + math.abs(q.b - ROW_ON[3])
            local doff = math.abs(q.r - ROW_OFF[1]) + math.abs(q.g - ROW_OFF[2])
              + math.abs(q.b - ROW_OFF[3])
            -- A category is in force, so this is not.
            if don < doff then return nil end
          end
        end
      end
      -- Silent when no row could be read at all: "in force" would then be a guess dressed as
      -- a reading, and the categories above will have said nothing either.
      if not sawRow then return nil end
      -- Only the FIRST filter is the "everything" one. Favorites is a filter in its own
      -- right, and no row being selected says nothing about whether it is the one in force.
      if f.label ~= spec.filters[1].label then return nil end
      return "in force"
    end,
  })
  end

  -- The category list, by slot rather than by name. The three in the capture are Studio
  -- Monitors, Hi-Fi and Multimedia, and hard-coding those three would be a module that
  -- quietly loses a category the next time IK ships one. The rows carry their own names, so
  -- the slots read them; a slot with no row is skipped in navigation rather than announced
  -- as an empty thing to press.
  for i = 1, spec.rows do
    -- Inside the panel, or it does not exist as far as this ring is concerned.
    --
    -- The tree reports rows the list has scrolled past: seventeen makers came back from a
    -- panel with room for fifteen, and the last two sat below its bottom edge. Their state
    -- probe read (4,4,4) — the dimmed main window showing through from behind — and refused,
    -- which is how they were noticed, but refusing to name a state is not the point here.
    -- Activating one would have clicked straight through the panel onto whatever the
    -- application has at that spot, which in this window is the Studio Simulation section.
    local function row()
      return visibleRows()[i]
    end
    -- A row's own accessible name is "Row 1"; the words are in its children, which share its
    -- top edge — the label at the row's left edge and the count to the right of it.
    -- The row's OWN children. Matching anything on the same line and to the right swept up
    -- whatever else the panel happened to have at that height — a tile caption, a column of
    -- the bar along the bottom — and read it out after the maker's name. Depth is the thing
    -- that says "belongs to this row"; position only says "is near it".
    local function parts()
      local r = row()
      if not r then return nil end
      local label, count
      for _, n in ipairs(btree() or {}) do
        if n.depth == r.depth + 1 and n.name ~= r.name
          and math.abs(n.bounds.y - r.bounds.y) <= 4
        then
          if math.abs(n.bounds.x - r.bounds.x) <= 4 then
            label = n.name
          elseif n.bounds.x > r.bounds.x then
            count = n.name
          end
        end
      end
      return label, count
    end
    -- A toggle rather than a button, now that the selected look has been measured. These are
    -- a radio group — one filter at a time — and a radio group read aloud is a set of things
    -- of which exactly one says "on". Activating clicks the same point that reports the
    -- state, which is what selects the row.
    b:addHotspotToggle({
      label = spec.rowLabel,
      when = function() return row() ~= nil end,
      at = function() return rowPoint(row()) end,
      onColor = ROW_ON,
      offColor = ROW_OFF,
      text = function()
        local label, count = parts()
        if not label then return nil end
        return count and (label .. " " .. count) or label
      end,
    })
  end

  -- A tile says whether it is the one that is loaded, out of its own pixels.
  --
  -- Measured: the selected tile is drawn with an orange border and a warm tint inside it,
  -- (67,39,26), flat for more than twenty pixels in from its left edge; every other tile has
  -- the panel's own ground, (24,24,24), at the same place. Ten pixels inside the border and
  -- halfway down is inside both, comfortably.
  --
  -- This is the only thing in this panel that can be checked without the accessibility tree,
  -- which is what makes it worth having: the tree goes dark whenever the caret is in the
  -- search field, and that is precisely when somebody is about to press a tile.
  local TILE_ON, TILE_OFF = { 67, 39, 26 }, { 24, 24, 24 }
  local TILE_PROBE_DX = -72
  local TILE_LIMIT = 30 -- half the distance between the two, as everywhere else here

  -- Whether there is a speaker at a position at all, from its left border.
  --
  -- The tint above cannot answer this: an empty position and a tile that is merely not
  -- selected are the same colour there. What only a real tile has is its outline — measured at
  -- (50,50,50) on the middle row, (47,47,47) on the bottom one and the selected tile's orange
  -- (241,101,35), against the panel's own (24,24,24) where there is nothing. Flat down the
  -- tile's whole height, so the vertical placement has room; two pixels wide, so the
  -- horizontal placement does not, which is why an unclear reading counts as PRESENT.
  --
  -- That direction is deliberate. Offering a position that turns out to be empty costs one
  -- press and an honest "nothing at that position"; hiding one that exists makes a speaker
  -- unreachable with no symptom at all.
  local TILE_EDGE_DX = -81 -- one pixel inside the left border, from the tile's centre
  local GROUND_NEAR, EDGE_FAR = 8, 30

  local function tileFilled(tx, ty)
    local x, y = screenPointOf(b, tx + TILE_EDGE_DX, ty)
    if not x then return true end
    local q = host.screen.pixel(x, y)
    if not q then return true end
    local d = math.abs(q.r - TILE_OFF[1]) + math.abs(q.g - TILE_OFF[2]) + math.abs(q.b - TILE_OFF[3])
    if d > EDGE_FAR then return true end
    if d < GROUND_NEAR then return false end
    return true
  end

  local function tileSelected(tx, ty)
    local x, y = screenPointOf(b, tx + TILE_PROBE_DX, ty)
    if not x then return nil end
    local q = host.screen.pixel(x, y)
    if not q then return nil end
    local don = math.abs(q.r - TILE_ON[1]) + math.abs(q.g - TILE_ON[2]) + math.abs(q.b - TILE_ON[3])
    local doff = math.abs(q.r - TILE_OFF[1]) + math.abs(q.g - TILE_OFF[2]) + math.abs(q.b - TILE_OFF[3])
    if math.min(don, doff) > TILE_LIMIT then return nil end
    return don < doff
  end

  -- How far the tile lattice has slid from where it was measured, and where a row's middle
  -- is because of it. Declared here rather than beside the reading that works it out, because
  -- everything that positions a tile is below this line and a value used before its own
  -- declaration is not a value at all — which took the whole ring down: one nil call inside a
  -- control's visibility test, and Tab stopped doing anything at all.
  local gridShift = 0

  local function rowY(r)
    return spec.tiles.rows[r] + gridShift
  end

  -- HOW MANY positions hold anything, asked once per epoch and in as few screen touches as
  -- possible.
  --
  -- A pixel read is a GDI screen touch costing a fixed ~16.7 ms whatever it reads — one
  -- compositor frame — and this project has measured that before. Fifteen of them, which is
  -- what a per-tile `when` would cost, is a quarter of a second added to every Tab, and
  -- getting a quarter of a second OFF each Tab was a whole afternoon earlier in this file.
  --
  -- A grid fills in order, so the answer is one number and it can be found by halving. The
  -- last position is checked first because the ordinary case is a full grid, and that case
  -- then costs a single read; only a filtered grid pays the four.
  local tileCount = #spec.tiles.cols * #spec.tiles.rows

  local function tileAt(i)
    local r = (i - 1) // #spec.tiles.cols + 1
    local c = (i - 1) % #spec.tiles.cols + 1
    return spec.tiles.cols[c], rowY(r)
  end

  local lastFilled = nil
  local filledCount = O.memoByEpoch(function()
    if tileFilled(tileAt(tileCount)) then
      return tileCount
    end
    local lo, hi = 0, tileCount -- lo is known filled (or none), hi known empty
    while hi - lo > 1 do
      local mid = (lo + hi) // 2
      if tileFilled(tileAt(mid)) then
        lo = mid
      else
        hi = mid
      end
    end
    if lo ~= lastFilled then
      lastFilled = lo
      host.log.info(("[on-ear] %s: %d of %d positions hold a speaker")
        :format(spec.name, lo, tileCount))
    end
    return lo
  end)

  -- What the list shows now, for the scroller to report: the name in its first row.
  -- A row's own child, and only that. Matching on position alone picked up the list's
  -- "MANUFACTURER" heading once the list had been scrolled far enough for a row to sit where
  -- the heading is, and reported it as a maker. Depth is what actually tells them apart: the
  -- heading is a sibling of the list, the name is a child of the row.
  local function rowNameAt(i)
    local r = visibleRows()[i]
    if not r then return nil end
    for _, n in ipairs(btree() or {}) do
      if n.depth == r.depth + 1 and n.name ~= r.name
        and math.abs(n.bounds.y - r.bounds.y) <= 4
        and math.abs(n.bounds.x - r.bounds.x) <= 4
      then
        return n.name
      end
    end
    return nil
  end

  -- BOTH ends, and logged. The tester reported the list moving by two or three makers and
  -- then stopping, and "now starting at X" alone cannot tell a list that has stopped scrolling
  -- from one that is scrolling by a smaller step than expected — the first name is the same
  -- either way if the step is small. The span says which.
  local function listSpan()
    local first, last
    -- Counted as well as read. "The list stopped" and "the list moved somewhere this module
    -- will not look" are the same silence from the keyboard, and the difference is whether
    -- the tree offered rows that the filter then dropped.
    local rows = visibleRows()
    for i = 1, #rows do
      local n = rowNameAt(i)
      if n then
        first = first or n
        last = n
      end
    end
    host.log.info(("[on-ear] %s: %d row(s) showing, %d named"):format(
      spec.name, #rows, (first and 1 or 0) + (last ~= first and 1 or 0)))
    if not first then
      -- Which of the two silences this is. A list that has genuinely reached its end and a
      -- window that has stopped answering both leave this empty, and the tester saw the
      -- scrolling stop after two steps with no way to tell which had happened.
      host.log.info(("[on-ear] %s: no rows to read after scrolling — the tree has %d element(s)")
        :format(spec.name, #(btree() or {})))
      return nil
    end
    host.log.info(("[on-ear] %s list now shows %s .. %s"):format(spec.name, first, last))
    if first == last then return "showing " .. first end
    return ("%s to %s"):format(first, last)
  end

  -- ONE notch. Three of them took the list from Behringer past Beyerdynamic, Blue and Bose
  -- to Clear Tune Monitors in a single press — this list moves by a large fraction of itself
  -- per notch, and a step that jumps further than a screenful is not a way through a list,
  -- it is a way past one. Pressing again is cheap; the makers skipped over are not.
  scroller(spec.rowLabel .. " list down", 466, 600, -1, listSpan)
  scroller(spec.rowLabel .. " list up", 466, 600, 1, listSpan)

  -- The printed names, and where the grid currently sits, from ONE reading of all of it.
  --
  -- Grid down broke both at once, and for the same reason: this grid scrolls by fractions of a
  -- row, so afterwards every measured coordinate in it is out of alignment. The names were
  -- looked for above where they now are, and the tint that reports which tile is loaded was
  -- read at a point still inside the tile that used to hold it — which is why a stale
  -- selection stayed lit.
  --
  -- Both are answered by finding out where the lattice actually is, and the recogniser can
  -- say: a region bigger than 400 by 200 skips the crop-and-upscale step entirely and goes
  -- straight to the primary engine at native resolution, which is the path that read every one
  -- of these names correctly in the probe. So the whole grid is read at once, and each word's
  -- POSITION says which tile it belongs to and how far the rows have moved.
  --
  -- It costs more than a per-tile crop and is asked far less often — once when the panel
  -- opens, and once after anything that can move or refill the grid — instead of once per Tab.
  -- It also does away with two workarounds: no glyph repair is needed, because at this size the
  -- K is read as a K, and no list of which rows print a name, because a row that prints none
  -- simply contributes no words.
  local NAME_HALF = 54
  local NAME_ABOVE = 70 -- a name's middle sits this far below its tile's middle
  local gridNames, gridDirty, gridAtCount = {}, true, -1

  local function relearnGrid()
    gridDirty = false
    gridNames = {}
    if not spec.tiles.name then return end
    local o = b:origin()
    local k = scaleOf(o)
    if not k then
      gridDirty = true
      return
    end
    local cols, rows = spec.tiles.cols, spec.tiles.rows
    local rect = screenRect(o, cols[1] - 95, rows[1] - 100, cols[#cols] + 95, rows[#rows] + 100)
    if not rect then
      gridDirty = true
      return
    end
    local started = host.now()
    local got = host.ocr.recognize({ region = rect })
    -- Words that sit under a column, gathered with where they sat.
    local hits = {}
    for _, wd in ipairs((got and got.words) or {}) do
      local wx, wy = wd.x + wd.w / 2, wd.y + wd.h / 2
      for c, cx in ipairs(cols) do
        local sx = screenPointOf(b, cx, rows[1])
        if sx and math.abs(wx - sx) <= NAME_HALF * k then
          hits[#hits + 1] = { c = c, x = wx, y = wy, text = wd.text }
        end
      end
    end
    -- Rows, by grouping those words down the screen. A caption is one line, so anything
    -- within half a line of another belongs with it.
    table.sort(hits, function(a, z) return a.y < z.y end)
    local bands = {}
    for _, h in ipairs(hits) do
      local band = bands[#bands]
      if band and math.abs(h.y - band.y) <= 12 * k then
        band.items[#band.items + 1] = h
      else
        bands[#bands + 1] = { y = h.y, items = { h } }
      end
    end
    -- How far the lattice has moved. Every row shifts together, so one band settles it, and
    -- the answer is folded into a single row's spacing because a scroll of two rows and a bit
    -- leaves the grid looking exactly like a scroll of the bit.
    local spacing = (#rows > 1) and (rows[2] - rows[1]) or 1
    local oy = o.client.y
    if bands[1] then
      local designY = (bands[1].y - oy) / k -- back into the frame the coordinates are in
      local centre = designY - NAME_ABOVE
      local off = (centre - rows[1]) % spacing
      if off > spacing / 2 then off = off - spacing end
      gridShift = off
    else
      gridShift = 0
    end
    -- Each band to the row it now lines up with, and each word to its tile.
    for _, band in ipairs(bands) do
      local designY = (band.y - oy) / k - NAME_ABOVE
      local r = math.floor((designY - rows[1] - gridShift) / spacing + 0.5) + 1
      if r >= 1 and r <= #rows then
        local bag = {}
        for _, h in ipairs(band.items) do
          bag[h.c] = bag[h.c] or {}
          table.insert(bag[h.c], h)
        end
        for c, items in pairs(bag) do
          table.sort(items, function(a, z) return a.x < z.x end)
          local words = {}
          for _, it in ipairs(items) do words[#words + 1] = it.text end
          gridNames[(r - 1) * #cols + c] = table.concat(words, " ")
        end
      end
    end
    local named = 0
    for _ in pairs(gridNames) do named = named + 1 end
    host.log.info(("[on-ear] %s: read the grid in %d ms — %d named tile(s), rows offset by %d")
      :format(spec.name, host.now() - started, named, math.floor(gridShift + 0.5)))
  end

  -- A name, re-reading the grid only when something could have changed it.
  --
  -- `gridDirty` is set by everything this module does that moves or refills the grid — a
  -- scroll, a tile press, a filter. The count of occupied positions is checked as well,
  -- because a category chosen from the list changes the grid without going through any of
  -- those, and a filter almost always changes how many tiles are filled. It is already
  -- measured and costs nothing to compare.
  local function tileNameOf(n)
    local count = filledCount()
    if gridDirty or count ~= gridAtCount then
      gridAtCount = count
      relearnGrid()
    end
    return gridNames[n]
  end

  -- The grid. Its tiles are pictures with no accessible element and no text of their own, so
  -- a tile can only be named by pressing it: the panel's MODEL line then says what was
  -- chosen, and this control says it out loud rather than making the user go and look.
  --
  -- That is an honest contract and not an especially nice one — browsing means selecting —
  -- so the search field above is the better route when the name is known. It is worth having
  -- anyway, because after a search that leaves one match, that match is tile one.
  --
  -- Measured off the capture: the tile outlines run 665-830, 859-1023, 1052-1217, 1246-1410
  -- and 1439-1604 across, and 182-346, 375-539, 568-733 down.
  for r, ty in ipairs(spec.tiles.rows) do
    for c, tx in ipairs(spec.tiles.cols) do
      local n = (r - 1) * #spec.tiles.cols + c
      b:addCustomButton({
        label = spec.tileLabel .. " " .. n,
        -- A position with nothing in it is not offered. Searching narrows this grid to a
        -- handful, and tabbing past a dozen buttons that do nothing to reach them is not a
        -- list — it is an obstacle course.
        when = function() return n <= filledCount() end,
        text = function()
          -- The headphone panel prints a model name under every tile; the speaker panel
          -- prints none. Where there is one it is read, so this grid can be BROWSED — the
          -- speaker grid can only be sampled, because there a tile has no name until it has
          -- been pressed, and pressing is selecting.
          local name = tileNameOf(n)
          local on = tileSelected(tx, rowY(r))
          if name and on then return name .. ", selected" end
          return name or (on and "selected" or nil)
        end,
        onActivate = function()
          local ry = rowY(r)
          local x, y = screenPointOf(b, tx, ry)
          if not x then return end
          -- Waits for THIS TILE to become the selected one, then reads the name.
          --
          -- Two earlier versions got this wrong in the same direction. The first waited a
          -- quarter of a second and read whatever was there, and announced the previous
          -- speaker. The second compared the name before and after the click — better, until
          -- the caret was in the search field, because then the tree is dark, "before" is
          -- nothing at all, and the first name that becomes readable counts as a change even
          -- when it is the stale one. Which is exactly what the tester heard.
          --
          -- The tile's own tint is not subject to any of that. It is the click's result, it
          -- cannot be stale, and it does not depend on the application answering questions.
          -- Only once it says this tile is loaded is the name worth reading.
          host.log.info(("[on-ear] '%s %d' clicking (%d,%d)"):format(spec.tileLabel, n, x, y))
          clickAt(x, y)
          if not host.timer then return end
          gridDirty = true
          local tries = 18
          local function look()
            if tileSelected(tx, ry) then
              host.speech.output(selectionNow() or "not shown", { interrupt = true })
              return
            end
            tries -= 1
            if tries <= 0 then
              -- Never became selected. After a search the grid holds fewer speakers than it
              -- has positions, so this is the ordinary answer for a position with nothing in
              -- it — and it is the only report of a search result this panel gives us.
              host.speech.output("nothing at that position", { interrupt = true })
              return
            end
            host.timer.after(80, look)
          end
          host.timer.after(80, look)
        end,
      })
    end
  end

  do
    local mid = spec.tiles.cols[(#spec.tiles.cols + 1) // 2]
    local midRow = spec.tiles.rows[(#spec.tiles.rows + 1) // 2]
    local function firstTile()
      gridDirty = true
      local n = tileNameOf(1)
      if n then return "now starting at " .. n end
      return ("%d position(s) hold something"):format(filledCount())
    end
    scroller("Grid down", mid, midRow, -1, firstTile)
    scroller("Grid up", mid, midRow, 1, firstTile)
  end

  -- A dialog outranks the window overlay underneath it, which is what makes the main ring
  -- stand down while this is up.
  --
  -- `pollMatch` is not optional here, and leaving it out cost a test round. An overlay's gate
  -- is normally re-evaluated when a window is activated or focused — which is enough for a
  -- dialog, because opening and closing one IS a window event. This panel is not a window. It
  -- appears and disappears inside a window that never changes, so closing it produced no event
  -- at all: the gate was never asked again, this overlay kept the slot, and the tester was
  -- left holding a ring for a panel that was no longer on screen. He could tell only because
  -- "Selected" started saying it could not read anything.
  --
  -- Which is the whole reason the option exists — "for matches that change with no window
  -- event" — and the failure it prevents is worse than a missed activation: an overlay that
  -- will not let go is one a person who cannot see the screen has no way to escape.
  b:attach(on_ear, {
    menus = true,
    slot = ON_EAR_SLOT,
    specificity = O.layer.dialog,
    -- 500, not less: the log times each of these gate evaluations at 26 to 53 ms, because a
    -- UIA search of a whole window's subtree is what it costs. Every other arbiter overlay in
    -- this project polls at 500 for the same reason. Making the search itself cheap — it is
    -- looking for a DIRECT CHILD of the window and walks the entire subtree to find it — is a
    -- backend job, noted with the rest of the centralisation work.
    pollMatch = 500,
  })
  host.log.info(("[on-ear] %s overlay ready."):format(spec.name))
  return b
end

-- The two panels. Everything above is the same building; these are the measurements that
-- differ, and the second one cost one capture rather than a second module.
local GRID_COLUMNS = { 747, 941, 1134, 1328, 1521 }

chooser({
  name = "Speaker Browser",
  pane = "SpeakerBrowserWindow",
  -- The centre of the Close button's outline (x 1481-1598, y 128-174), not the centre of the
  -- word inside it: the word's middle happens to fall in the gap between two letters, which
  -- is a fine place for a button and a poor place to describe one from.
  close = { 1539, 151 },
  search = { 449, 201 },
  filters = { { label = "Show all", at = { 331, 277 } } },
  headings = { "MODEL", "DESCRIPTION" },
  rowLabel = "Category",
  rows = 8,
  tileLabel = "Speaker",
  tiles = { cols = GRID_COLUMNS, rows = { 264, 457, 650 } },
})

chooser({
  name = "Headphone Browser",
  -- Plural, and the application means it: the speaker one is SpeakerBrowserWindow.
  pane = "HeadphonesBrowserWindow",
  -- Same panel rectangle, same Close, same search box, to the pixel.
  close = { 1539, 151 },
  search = { 449, 201 },
  filters = {
    { label = "Show all", at = { 341, 260 } },
    { label = "Favorites", at = { 343, 296 } },
  },
  headings = { "MANUFACTURER", "MODEL", "TYPE", "IMPEDANCE", "SAFE HEADROOM" },
  dropdowns = { { label = "Type" }, { label = "Impedance" } },
  -- Seventeen makers were on screen at once and the list scrolls, so the slots are for what
  -- is visible; the tree only offers the visible rows in any case.
  rowLabel = "Manufacturer",
  rows = 20,
  tileLabel = "Headphone",
  -- The same five columns as the speaker grid, to the pixel, pushed down by the row of filter
  -- menus above it.
  --
  -- `name` is the box the printed model name sits in, as offsets from the tile's centre, and
  -- it is deliberately nowhere near the tile's edges. The first attempt reached to 82 below
  -- centre and 78 either side, which is where the tile's own rounded frame is — and the OCR
  -- step that trims a crop to its content then reads the FRAME as the content and squeezes
  -- the words into an unreadable sliver. The tester's report was that it read "weird stuff",
  -- which is exactly what that looks like from the outside. The runtime has carried a note
  -- about this since Komplete Kontrol's save dialog: all four corners must land on flat
  -- ground.
  --
  -- The text itself measured at 65 to 74 below centre in every one of the ten tiles that has
  -- one, and never wider than 39 either side. The bottom row has no name at all — the grid's
  -- viewport clips it before the strip — so those three tiles simply read nothing, which is
  -- already what an empty read means here.
  -- `name` is the box the printed model name sits in, as offsets from the tile's centre, and
  -- the fifth entry is which ROWS actually print one. Both are measurements, not choices: the
  -- text sits 65 to 74 below centre in every tile that has one and is never wider than 39
  -- either side, and the bottom row's strip is clipped away by the grid's viewport, so there
  -- is nothing there to read.
  tiles = {
    cols = GRID_COLUMNS,
    rows = { 314, 507, 701 },
    name = true,
  },
})

-- Navigation is Tab / Shift+Tab / Enter, captured while this window is in front, as
-- everywhere else in this platform.
--
-- On a SLOT, which it did not need until this window turned out to have chooser panels. They
-- are not separate windows — each browser is a pane drawn inside this one, at (270,105) and
-- 1380x799 of it — so every overlay here matches the same window and they would otherwise all
-- be active at once, with several Tab rings fighting over one key and the 31 controls of this
-- one pointing at things a panel now covers. Pressing "FN Mute" would have clicked whatever
-- tile happened to be at that point.
--
-- The arbiter settles it: same slot, and a chooser is a `dialog` where this is `content`, so
-- while one is up it owns the window and this overlay stands down — which is also exactly
-- what it means to the person using it.
ov:attach(on_ear, { menus = true, slot = ON_EAR_SLOT, specificity = O.layer.content })
