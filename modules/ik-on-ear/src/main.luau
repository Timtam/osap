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

-- ── The accessibility tree, read once and indexed ────────────────────────────
--
-- `rawDump` walks the whole tree across a process boundary. That is affordable once when the
-- window comes forward, and not affordable per keystroke, so it is memoised per window —
-- and re-read whenever the values it holds might have moved, which for this application is
-- whenever anything changed at all. `memoByEpoch` is exactly that: one read per pump epoch,
-- shared by every control that asks in the same breath.
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
local function tree()
  local o = ov:origin()
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

-- Navigation is Tab / Shift+Tab / Enter, captured while this window is in front, as
-- everywhere else in this platform.
ov:attach(on_ear, { menus = true })
