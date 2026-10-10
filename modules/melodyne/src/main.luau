-- Melodyne 5 — the first overlay in this project that is NOT a ReaHotkey port.
--
-- ReaHotkey has never carried one. The source is the user's own AutoHotkey draft, which is
-- itself built on the coordinates published by SIBIAC, Alexey Zhelezov's open-source NVDA
-- add-on (azslow.com), used with permission. SIBIAC pins everything to the client area of the
-- editor window (class GNWindowDoc), which is the same frame this project's controls are
-- authored in, so its numbers transfer unchanged — no offset, no rescaling.
--
-- Melodyne standalone only, for now. The plug-in inside a DAW is the same GUI in a host window
-- and should follow once the standalone is right.

local O = host.require("com.platform.overlay")

-- The standalone editor window. Class and executable from the draft.
local melodyne = {
  app = { exe = { contains = "Melodyne" } },
  windows = { class = "GNWindowDoc" },
}

-- ── The tool bar ──────────────────────────────────────────────────────────────
-- Six icons in a row, and one focus stop rather than six, because that is what it is: a single
-- setting with six values, not six buttons. Left and Right step it, which matches how it looks
-- and leaves Up and Down for the variants.
--
-- `at` is both where to CLICK to select the tool and where to LOOK to see whether it is the
-- active one — the same point, because the icon lights up.
--
-- `variants` are the extra tools stacked behind an icon, which Melodyne only reveals on a
-- press-and-hold. `drag` is how far below the icon that entry sits in the flyout, and `probes`
-- are points that read near-black ONLY while that variant is the live one. Both are SIBIAC's,
-- verified by the draft's author for the Time column.
-- Sub-tool names and press counts from Celemony's own manual, checked against the binary
-- rather than taken from the earlier draft: that list was short, because Fade and Sibilant
-- Balance are Melodyne 5 additions and any list inherited from a Melodyne 4 era necessarily
-- misses them. Celemony's spelling varies between its own pages (Time Tool vs Timing Tool,
-- Separation Type Tool vs Note Separation Type Tool), so these are the forms that appear most
-- often in the manual.
--
-- `press` is how many times the tool's function key selects that entry, which is Celemony's
-- documented route to the sub-tools. `probes` read near-black ONLY while that variant is live
-- and come from SIBIAC; the entries without probes are the ones nobody has mapped, and they
-- report the base tool rather than inventing a state.
local TOOLS = {
  { name = "Main", key = "F1", at = { 84, 59 }, variants = {
      { name = "Scroll", press = 2, probes = {} },
      { name = "Zoom", press = 3, probes = {} },
    } },
  { name = "Pitch", key = "F2", at = { 100, 60 }, variants = {
      { name = "Pitch Modulation", press = 2, probes = { { 107, 68 }, { 107, 62 } } },
      { name = "Pitch Drift", press = 3, probes = { { 111, 62 }, { 112, 69 } } },
    } },
  -- The only base tool with no sub-tools at all, and it is visible as well as documented:
  -- its icon carries no drop-down triangle.
  { name = "Formant", key = "F3", at = { 129, 59 } },
  { name = "Amplitude", key = "F4", at = { 153, 60 }, variants = {
      { name = "Fade", press = 2, probes = {} },
      { name = "Sibilant Balance", press = 3, probes = { { 151, 67 } } },
    } },
  { name = "Time", key = "F5", at = { 175, 60 }, variants = {
      { name = "Time Handle", press = 2, probes = {} },
      { name = "Attack Speed", press = 3, probes = { { 177, 62 }, { 177, 72 } } },
    } },
  { name = "Note Separation", key = "F6", at = { 200, 60 }, variants = {
      { name = "Separation Type", press = 2, probes = {} },
    } },
}

-- Active icons are near-white, inactive ones a light grey. NEAREST of the two rather than a
-- threshold or an equality, and the draft is explicit about why: the live values drift a few
-- units either way (active reads ~0xFCFCFC against a nominal 0xFDFDFD, inactive ~0xD9D9D9
-- against 0xDADADA), so an exact comparison misclassifies both.
--
-- Nearest-of-two refuses nothing, which is right here and wrong elsewhere in this project:
-- every icon is one or the other, so there is no third state to guard against. Where a probe
-- can land on something that is neither — Kontakt's view probe, its panel toggles — refusing is
-- the rule, because there a wrong answer costs the user a panel.
local ACTIVE, IDLE = { 253, 253, 253 }, { 218, 218, 218 }
local BLACK, WHITE = { 0, 0, 0 }, { 253, 253, 253 }

local function nearer(p, a, b)
  local da = math.abs(p.r - a[1]) + math.abs(p.g - a[2]) + math.abs(p.b - a[3])
  local db = math.abs(p.r - b[1]) + math.abs(p.g - b[2]) + math.abs(p.b - b[3])
  return da <= db
end

-- Every point of the tool bar that says something, in client coordinates: each tool's icon, then
-- each variant's probes. A tool's `lit` and a variant's `seen` say where their readings sit in it.
local TOOL_POINTS = {}
for _, tool in ipairs(TOOLS) do
  TOOL_POINTS[#TOOL_POINTS + 1] = tool.at
  tool.lit = #TOOL_POINTS
end
for _, tool in ipairs(TOOLS) do
  for _, v in ipairs(tool.variants or {}) do
    v.seen = {}
    for _, probe in ipairs(v.probes) do
      TOOL_POINTS[#TOOL_POINTS + 1] = probe
      v.seen[#v.seen + 1] = #TOOL_POINTS
    end
  end
end

-- The tool bar as it is now: a colour for every point of TOOL_POINTS, or nil when it could not
-- be read.
--
-- ONE `pixels` call, which is one capture: thirteen points in a strip 120 pixels wide. They used
-- to be a `pixel` each, and a pixel read costs a fixed compositor frame — around 17 ms, measured,
-- whatever its size — so the first look at the tool bar, which tool and then which of its
-- variants, could read seven of them: a tenth of a second between a Tab and its announcement.
local function readToolBar(ov)
  local o = ov:origin()
  if not (o and o.client) then return nil end
  local pts = {}
  for i, p in ipairs(TOOL_POINTS) do
    pts[i] = { o.client.x + p[1], o.client.y + p[2] }
  end
  return host.screen.pixels(pts)
end

-- Which tool a reading of the tool bar shows selected, or nil when none reads as lit.
--
-- Asked on arrival, so the tool bar reports where the APPLICATION is rather than where this
-- control last put it. That distinction is the whole point: Melodyne has F1 to F6 for the same
-- job, and a control that only remembers its own last move is confidently wrong the moment the
-- user reaches for the shortcut.
local function activeTool(seen)
  if not seen then return nil end
  for i, tool in ipairs(TOOLS) do
    if nearer(seen[tool.lit], ACTIVE, IDLE) then return i end
  end
  return nil
end

-- The tool bar, read at most once per input epoch.
--
-- Even one capture is a compositor frame, and the tool bar is asked about on every focus move
-- that announces it — which tool, which variant, and whose value the left read-out shows — so
-- they share the one reading.
--
-- Keyed on `inputEpoch` rather than `epoch`: which tool is selected changes only when
-- something ACTS — the user pressing a function key, or us clicking an icon — and that is
-- exactly what turns that counter over. An ordinary epoch would re-read on every question.
local toolBarMemo = { gen = -1, seen = nil }
local function toolBarCached(ov)
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if toolBarMemo.gen ~= gen then
    toolBarMemo.gen = gen
    toolBarMemo.seen = readToolBar(ov)
  end
  return toolBarMemo.seen
end

-- Which tool is active, from the reading of this input epoch.
local function activeToolCached(ov)
  return activeTool(toolBarCached(ov))
end

-- A read-out change that the user CAUSED by changing tool is not news.
--
-- The tool bar and the two read-outs are the same strip of pixels: pick a different tool and
-- the left box shows that tool's own value instead. The watcher at the bottom of this file saw
-- the box change and announced it — over the top of the tool name the user had just asked for,
-- with interrupt, so the answer to "which tool is this" was cut off by its own side effect.
--
-- A window rather than a flag, because the strip does not repaint within one tick and a
-- sub-tool is reached by a whole run of key presses. During it the caches keep updating, so
-- nothing is announced late either: the new values become the baseline silently, which is what
-- they are. Declared up here because the two things that arm it — the tool bar's `onSelect` and
-- the variant stepper — both come before the watcher that reads it.
local READOUT_HOLD_MS = 500
local holdUntil = 0
-- When the strip's latest change was asked for: the tool switch, or the last press of a variant's
-- run (the hold's own estimate of it). The first answer to a read asked after it becomes the
-- baseline unsaid — see the watcher. nil once that answer has come.
local rebaseFrom = nil
local function holdReadouts(ms)
  if not host.now then return end -- no clock, no window; better than a hold that never lifts
  holdUntil = host.now() + (ms or READOUT_HOLD_MS)
  rebaseFrom = holdUntil - READOUT_HOLD_MS
end

-- Assigned further down, once the field reader it needs exists. The tool bar is declared before
-- that reader and has to be able to call it, and a forward local is a smaller price than moving
-- either of them somewhere it does not belong. `forgetBoxes` is here for the same reason: the
-- read-out boxes belong beside the regions they measure, and everything that CHANGES which
-- boxes exist — the tool bar, the variant stepper — is declared before that point.
local reportFieldsAfterSwitch = nil
local forgetBoxes = nil

-- What we last DELIBERATELY selected, for the log line only.
--
-- The runtime hands the tab index to `onSelect`, which is better than any probe: we did not
-- infer the tool, we chose it. It can still go stale if the tool is changed inside Melodyne by
-- mouse, so the log says which of the two it is rather than presenting both as the same fact.
local selectedToolName = nil

-- The active tool ONLY if it is already known — never at the price of a fresh probe.
--
-- For the log, and the restraint is the point. Recording which tool a read-out belonged to is
-- how we will learn what the second field holds under the tools nobody has measured yet, but
-- the read-outs change while the user walks from note to note with the arrow keys, and a
-- capture of the tool bar at a compositor frame on that path would buy that knowledge with the
-- responsiveness of the thing being described. So: free when the epoch already asked, absent
-- otherwise. It fills in whenever the user has just been in the overlay, which is exactly when
-- somebody is looking at a read-out anyway.
local function activeToolIfKnown()
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if toolBarMemo.gen ~= gen then return nil end
  local i = activeTool(toolBarMemo.seen)
  return i and TOOLS[i] and TOOLS[i].name or nil
end

-- The tool this read-out belongs to, and HOW we know — measured, chosen, or not at all.
--
-- Three different facts wear the same word otherwise, and the log is where the difference will
-- matter: a probed name was read off the screen just now, a selected one is what we clicked and
-- could have been overridden inside Melodyne since, and "?" is the honest answer when neither
-- is available and the alternative would be a screen read on the note-stepping path.
local function toolForLog()
  local probed = activeToolIfKnown()
  if probed then return probed .. " (probed)" end
  if selectedToolName then return selectedToolName .. " (selected)" end
  return "?"
end

-- Which variant of a tool a reading of the tool bar shows live: the first whose probe points ALL
-- read near-black, else the base. Only meaningful while that tool's icon is lit, since the probes
-- read its glyph against the lit white box.
local function activeVariant(seen, tool)
  if not seen then return 1 end
  for i, v in ipairs(tool.variants or {}) do
    if #v.seen > 0 then
      local all = true
      for _, k in ipairs(v.seen) do
        if not nearer(seen[k], BLACK, WHITE) then all = false break end
      end
      if all then return i + 1 end
    end
  end
  return 1
end

-- The same, from the reading of this input epoch.
--
-- Three callers want it while the tool bar is being announced — the tab's `current`, its
-- `verticalName`, and the left read-out's owner name. Each probe point was a screen read at a
-- compositor frame of its own, and landing on the tool bar was measured at 89 to 110 ms of pure
-- announcement because of it; now they are points of the tool's own reading.
--
-- Keyed on the input epoch for the reason `activeToolCached` is: which variant is live changes
-- only when something ACTS, and that is exactly what turns the counter over. A user walking the
-- Tab ring does not turn it, so the walk is free after the first look.
local function activeVariantCached(ov, tool)
  return activeVariant(toolBarCached(ov), tool)
end

local ov = O.new("Melodyne")

-- 1-based position in the current tool's variant stack; 1 is the base tool.
local variant = 1
-- Which tool the tracker's position belongs to; a different tool means start at its base.
local lastTool = nil

-- ── Selecting a variant, by function key ──────────────────────────────────────
-- This was a press-and-hold-then-glide drag, copied from the draft. Celemony documents no such
-- gesture: searching the whole 280-page manual gives zero hits for "click and hold", "flyout"
-- and "popup", and the only third-party descriptions are of a standing drop-down menu, on an
-- older Melodyne, with nobody at all describing a DRAG. What Celemony does document is that
-- pressing a tool's function key REPEATEDLY steps through its sub-tools.
--
-- So: click the icon to land on the base tool, then post the function key (target - 1) times.
-- Starting from a known base makes it deterministic whichever way Melodyne's own cycling
-- wraps — a question the manual does not answer.
--
-- Posted rather than sent, because our own hotkey registration would otherwise catch the key
-- we just synthesised. And spaced out in time: how quickly two presses have to follow one
-- another to count as a repeat is not documented either, so this leaves a gap that is
-- comfortable rather than minimal, and reports what it actually reached.
local PRESS_GAP = 180

local function selectVariant(o, tool, target)
  local origin = o:origin()
  if not (origin and origin.client) then return end
  local hwnd = o:hwnd()
  -- Long enough to cover the whole press sequence and the repaint after it. A variant change
  -- moves the same strip the read-outs live in, and the announcement that matters is the one
  -- this function ends with — the sub-tool it actually reached.
  holdReadouts(PRESS_GAP * (target > 1 and target or 1) + READOUT_HOLD_MS)
  if forgetBoxes then forgetBoxes() end
  host.input.click(origin.client.x + tool.at[1], origin.client.y + tool.at[2])
  variant = 1
  if target <= 1 or not hwnd then return end
  local presses = tool.variants[target - 1].press - 1
  local sent = 0
  local step
  step = function()
    if not (o.active and o:hwnd() == hwnd) then return end
    host.input.post(hwnd, tool.key)
    sent = sent + 1
    if sent < presses then
      host.timer.after(PRESS_GAP, step)
    else
      -- Say what was REACHED, not what was asked for, wherever the probes can tell. Where they
      -- cannot — Scroll, Zoom, Fade, Time Handle, Separation Type have never been mapped — the
      -- log records the disagreement rather than the announcement claiming certainty.
      host.timer.after(PRESS_GAP, function()
        if not o.active then return end
        -- CHECKED against the probes, not CORRECTED by them. Overwriting the tracked position
        -- with the measurement is what broke stepping: the probes report the base tool most of
        -- the time even when the sub-tool was plainly reached, so every Down recomputed 1+1
        -- and the third entry was unreachable. Of the eight sub-tools only three were ever
        -- mapped by anyone, so a probe that says "1" means "nothing to see here" far more
        -- often than it means "still on the base tool".
        local seen = activeVariant(readToolBar(o), tool)
        if seen ~= target then
          host.log.info(("[melodyne] variant: asked for %d (%s), probes report %d")
            :format(target, tool.variants[target - 1].name, seen))
        end
      end)
    end
  end
  host.timer.after(PRESS_GAP, step)
  variant = target
end

do
  local tabs = {}
  for i, tool in ipairs(TOOLS) do
    -- "has vertical variations" flags the tools whose icon stacks extra tools behind a
    -- press-and-hold: without it a user who cannot see the flyout has no way to learn the axis
    -- exists. The F-key is not in the label — `hotkeyLabel` puts it in the announcement, which
    -- is the runtime's own way of teaching a shortcut.
    tabs[i] = {
      label = tool.name .. " tool"
        .. (tool.variants and ", has vertical variations" or ""),
      at = tool.at,
      -- The tab's OWN hotkey, which is ReaHotkey's Tab.SetHotkey and was already in this
      -- runtime: pressing it brings the tab to the front from anywhere in the overlay, moves
      -- focus onto it, and announces it. The first attempt here put six hidden buttons beside
      -- the tab control instead and they never fired — a hotkey resolves to its VISIBLE
      -- claimant, so a control hidden to keep it out of the Tab ring has no claim to anything.
      --
      -- It also settles the question the draft works hard on. There, the F-key had to be
      -- re-injected into Melodyne because the hotkey consumed it and Melodyne was the one doing
      -- the switching. Here the tab's own coordinate is clicked, so Melodyne never needs to see
      -- the key at all — and the quirk about the first keystroke after activation being
      -- swallowed stops mattering.
      hotkey = tool.key,
      hotkeyLabel = tool.key,
    }
  end
  ov:addTabControl({
    tabs = tabs,
    -- Not spoken — a tab control announces its current TAB and its `typeLabel`, never this — but
    -- every log line that names a control prints it, and an unnamed control logs as ''. Two
    -- separate investigations have had to work out which control that was.
    label = "Toolbox",
    -- Not "tab control": mechanically that is exactly what it is, and saying so tells the user
    -- the wrong thing about what they are looking at. Celemony calls it the Toolbox;
    -- "horizontal tool bar" names it and says which way the arrows go.
    typeLabel = "horizontal tool bar",
    -- F1 to F6 pick a tool WITHOUT dragging the user to the tool bar. Changing tool is
    -- something you do in the middle of editing notes, and being moved out of the Note Editor
    -- on every switch — then having to walk back — is worse than the switch is worth. Tab still
    -- reaches the bar when the bar is what you want.
    hotkeyKeepsFocus = true,
    -- The tool bar and the read-outs are the same strip: choosing a tool changes what the left
    -- box shows, and the watcher was announcing that over the top of the tool's own name. The
    -- answer to "which tool is this" was being cut off by its own side effect. So the read-outs
    -- are held for a moment here — and the index is recorded, because being TOLD which tool was
    -- selected beats probing the icons for it.
    onSelect = function(_, i)
      selectedToolName = TOOLS[i] and TOOLS[i].name or nil
      holdReadouts()
      -- Which boxes exist is a property of the tool, so the answer we are holding is stale from
      -- this instant. Not measured here: the watcher's first picture asked once the hold has lifted
      -- says, and until then both boxes count as drawn.
      if forgetBoxes then forgetBoxes() end
      -- And every tool switch is a MEASUREMENT of what that tool puts in the two boxes.
      --
      -- The watcher cannot supply this on its own: it reports what CHANGED, and a tool whose
      -- boxes are both empty changes nothing, so it never appears in the log at all. That is
      -- precisely how Time and Note Separation stayed unmeasured through a whole session of
      -- evidence. Here we know the tool by name because we just chose it, so one deliberate
      -- reading afterwards fills the gap — two OCRs, once per switch, off the note-stepping
      -- path entirely, and it needs nobody to look at a screen.
      if reportFieldsAfterSwitch then reportFieldsAfterSwitch(selectedToolName) end
    end,
    current = function(o)
      local i = activeToolCached(o)
      if not i then return nil end
      -- The TOOL is re-detected whenever anything ACTED, because its probe is a clean
      -- bright-against-grey reading and Melodyne's own F-keys change it behind our back. Not on
      -- every announcement, though: this runs each time the tool bar is spoken, and a capture is
      -- a compositor frame of silence after a Tab (one per icon measured 89 to 110 ms). The
      -- input epoch is exactly the right clock — it turns over when a key or a click happens,
      -- which is the only way the tool can have changed, and a walk around the Tab ring does
      -- not turn it.
      --
      -- The VARIANT is not, and the asymmetry is the point: a sub-tool probe only exists for
      -- three of the eight, so a reading of "base tool" is usually the absence of a probe
      -- rather than the presence of the base. It may upgrade what we think — if it positively
      -- recognises a sub-tool, that beats our record, and it catches one chosen inside Melodyne
      -- — but it may not reset us to the base.
      if lastTool ~= i then
        variant = 1
        lastTool = i
      end
      local seen = activeVariantCached(o, TOOLS[i])
      if seen > 1 then variant = seen end
      return i
    end,
    -- Down and Up step this tool's stacked variants, which is where they belong: a variant is a
    -- property of the tool, not a separate thing beside it. This was briefly a control of its
    -- own on Alt+V, which worked and said the wrong thing about the shape of the UI.
    verticalName = function(o, i)
      local tool = TOOLS[i]
      if not (tool and tool.variants) then return nil end
      -- CHECKED against the probes, never CORRECTED by them — the same rule `current` above
      -- argues for at length, and this function was quietly breaking it.
      --
      -- It assigned the raw probe reading straight into `variant`, which meant a probe that
      -- failed to recognise a sub-tool reset us to the base. And it runs on every announcement
      -- of this control, so Tabbing away and back was enough: pick the middle of three
      -- variants, leave, come back, and Down moved to the FIRST variant instead of the third,
      -- because our record had been overwritten by a reading that says "1" both when the base
      -- tool is live and when the probe simply cannot tell. The log says how often that is —
      -- "asked for 2 (Fade), probes report 1", over and over — since only three of the eight
      -- variant stacks have probe points at all.
      local seen = activeVariantCached(o, tool)
      if seen > 1 then variant = seen end
      if variant <= 1 then return nil end -- the base tool: the tab's own label is right
      return tool.variants[variant - 1].name .. " tool, has vertical variations"
    end,
    verticalStep = function(o, i, dir)
      local tool = TOOLS[i]
      if not (tool and tool.variants) then
        return (tool and tool.name or "This tool") .. " has no variants"
      end
      local count = #tool.variants + 1
      -- Clamped rather than wrapped: at either end the name is repeated instead of jumping to
      -- the far side, so holding the key cannot cycle past the edge unnoticed.
      local target = math.max(1, math.min(count, variant + dir))
      if target ~= variant then selectVariant(o, tool, target) end
      local reached = target == 1 and tool.name or tool.variants[target - 1].name
      -- Measured here as well as on a tab switch, because a VARIANT changes the strip exactly
      -- as a tool does — and the log had no line for any of them, which is why the Fade tool's
      -- read-outs were an open question after a session spent in it.
      if reportFieldsAfterSwitch then
        reportFieldsAfterSwitch(reached, PRESS_GAP * (target > 1 and target or 1) + READOUT_HOLD_MS - 150)
      end
      return reached .. " tool"
    end,
  })
end

-- Celemony's own name for the main editing area, where the note blobs live. A static text
-- rather than a control, because there is nothing here to press — it marks the place.
ov:addStaticText("Note Editor")

-- ── The menu bar ──────────────────────────────────────────────────────────────
-- Melodyne swallows Alt, F10 and even the WM_SYSCOMMAND that would normally open its menu bar,
-- so the only way in is a click on "File" followed by Escape: the click enters real Win32 menu
-- mode with the dropdown open, and Escape collapses the dropdown while leaving the bar focused.
--
-- On a bare ALT TAP, which is what every other application does and therefore the only thing a
-- user will try. It needed a new capability — the keyboard hook now recognises a modifier
-- pressed and released with nothing in between, and passes it through rather than swallowing
-- it, so Alt keeps working as a modifier. Melodyne itself does nothing with Alt, so nothing is
-- taken away.
--
-- A control as well, because a gesture nobody can see is a gesture nobody can find: this one is
-- in the Tab ring and says its own name, and the tap is the shortcut for it.
--
-- The CLICK stays, and that is a portability decision rather than a shrug. Melodyne's menu bar
-- is an ordinary Win32 menu — verified, its binary imports CreateMenu and GetMenuItemInfoW but
-- never TrackPopupMenu — so it can be enumerated and driven with WM_COMMAND, and that was built
-- and measured here at 184 entries. It was then removed: a posted WM_COMMAND has no counterpart
-- on macOS, which this project is meant to reach, whereas a click does. And it buys nothing —
-- once the bar is open the operating system's own accessibility reads it.
local function engageMenuBar(o)
  local origin = o:origin()
  if not (origin and origin.client) then return end
  -- "File" sits ABOVE the client origin: y is negative, which is legal here and was a real bug
  -- elsewhere in this project when a guard assumed otherwise.
  host.input.click(origin.client.x + 12, origin.client.y - 3)
  host.timer.after(150, function()
    if o.active then host.input.send("Escape") end
  end)
end

ov:addCustomButton({
  label = "Menu bar",
  hotkey = "Alt+F",
  -- A control's hotkey moves the overlay's focus to it, but not this one: focus on a button
  -- holds Space, and Space is Melodyne's play/stop. Alt+F from a read-out must leave the user
  -- on the read-out.
  hotkeyKeepsFocus = true,
  onActivate = engageMenuBar,
})

-- The read-outs in the tool strip (SIBIAC's Edit1 and Edit2). `readOnly`, because activating
-- them does nothing in Melodyne and a control must not announce itself as a button it cannot
-- honour — the rule this project arrived at through a "button" that only reported.
--
-- The LEFT one was called "Selected note", and that was true of one tool out of six. The strip
-- is tool-dependent: the left box holds whatever parameter the live tool owns, and the unit is
-- that tool's rather than the note's — per cent for modulation, drift and attack speed,
-- decibels for amplitude, cents for formants, a note name only under the pitch tools. So it
-- announces WHICH TOOL's value it is reading, which is what the label could never say.
--
-- SIBIAC's model says the number of boxes varies too — "zero, one or two parameter controls,
-- depending on the tool" — and this file deliberately holds no table about that. What varies in
-- every reading anyone has taken is the CONTENT: a tool with one parameter leaves the second
-- box saying nothing, and nothing is what gets announced. Whether the box itself is ever
-- undrawn has never actually been measured; see `BOX_EDGES` below, which will settle it from
-- ordinary use.
local NOTE_REGION = { 220, 61, 287, 74 }
local PITCH_REGION = { 300, 61, 367, 74 }

-- The RIGHT box's left border — one pixel that answers a question no amount of OCR can.
--
-- Everything ever observed about the second field was a reading of its CONTENT, and that is
-- the flaw at the bottom of this whole affair. Melodyne's "nothing to report" is a dash three
-- ink pixels wide on a single row; the recogniser drops it exactly as it drops bare tool bar.
-- So "the deviation read as empty" has never distinguished "there is no box here" from "the
-- box is here and holds a dash" — and a control was taken out of the Tab ring on the strength
-- of that distinction, which nobody had ever made.
--
-- The border makes it. Measured across row y=67 of calibration/Melodyne-clean.png: the strip
-- runs 191, each box is bordered by a solid 157 column (x=218 and x=289 for the left box, 298
-- and 369 for the right) and filled with 194. The column at x=298 holds 157 from y=62 to y=73
-- without a break. 157 against 191 is a wider separation than the tool icons' own probe, which
-- this file already trusts, and the FILL would not do: 194 against 191 is three units.
--
-- All FOUR borders, not just the right box's, because three different states have to be told
-- apart and two probes cannot do it:
--   157 157 157 157   both boxes drawn — the right one is present and merely empty
--   157 157 191 191   the right box is genuinely not there under this tool
--   none of them 157  the strip has moved; the window is a size these coordinates do not fit,
--                     and every reading in this file is suspect rather than just this one
-- That last row is the one a single probe would misreport as "the box is gone", and a wrong
-- fact recorded confidently is what this whole affair was.
--
-- Logged as raw numbers and NOT turned into a predicate. No frame anyone has captured shows
-- that box absent, so a gate would again be a claim without an observation — and a number in
-- the log can still be reinterpreted by somebody who later learns what it meant, which a
-- verdict cannot.
local BOX_EDGES = { { 218, 67 }, { 289, 67 }, { 298, 67 }, { 369, 67 } }

-- A WIDER look, for the log only, because our own window may be cutting the answer in half.
--
-- Melodyne's binary carries one inspector field that is not like the others:
-- `MUElementTimeInspector`, bound to `startTime`, with the entry mask 9999|00|00|000 —
-- bar|beat|sixteenth|ticks — and 100 design units wide where every other field in the family is
-- 72. Anchored like the pitch fields it would run from x218 to x318. That swallows the right-
-- hand border probe at x298 and the gap probe at x294, and it overflows the 67-pixel OCR window
-- at x220-287. In other words: if the Time tool DOES show its position, this module could not
-- have seen it, and every "the time tools show nothing" in this file would be an artefact of
-- where we chose to look.
--
-- So: one wide recognition and a row of raw greys across the whole strip, written to the log at
-- each tool switch and nowhere else. No control reads them, nothing is announced from them.
-- They are here to answer the question before anybody acts on an answer.
local WIDE_REGION = { 218, 61, 320, 74 }
local EDGE_ROW = { 212, 218, 250, 289, 294, 298, 310, 320, 330, 369, 375 }

-- Two of those four, used as a live test rather than a log entry: the right box's left border,
-- and a point in the GAP between the boxes that is bare tool bar whether or not the right box
-- exists (x290-297 lies past the left box's own border at 289).
--
-- Compared against each other, never against a fixed number. The same border read 157 on one
-- capture and 146 on another, the box fill 194 against 209 — Melodyne's greys move with its
-- appearance setting and with what the application draws over itself. What does not move is the
-- CONTRAST: 45 units apart when the box is there, 0 when the strip runs straight through. A
-- threshold would have been wrong within four days; a comparison cannot be.
-- Per box: its left border, and a point just outside it that is bare tool bar either way. For
-- the right box that is the gap between the two (x290-297, past the left box's own border at
-- 289); for the left box it is the run between the last tool icon and the box (x212-217).
local BOX_PROBES = {
  left = { edge = { 218, 67 }, gap = { 215, 67 } },
  right = { edge = { 298, 67 }, gap = { 294, 67 } },
}
local BOX_EDGE_CONTRAST = 20

-- Is a box drawn RIGHT NOW? What the watcher's latest picture of this stay showed, however old
-- it is.
--
-- This is the same question the removed `when` predicate answered by asking which tool was
-- selected — from a table of tool names taken out of five documentation figures, which left out
-- the Main tool and cost the user his cents read-out. The question was never wrong; the
-- instrument was. One pixel against another says what Melodyne is drawing, needs no list of
-- tools, and cannot go stale when Celemony adds one.
--
-- Read from the watcher's own pictures: every one of them holds both boxes' probes, and the
-- watcher notes what they show here (rememberBoxes), eight times a second while the overlay is in
-- front, at no cost of its own. NEVER from the screen here. This is a `when`, asked on every walk
-- of the Tab ring and as the overlay comes to the front, where a read of the screen holds the
-- event loop for a compositor frame and cannot wait for its picture; it used to read both points
-- live whenever the watcher's answer was half a second old — a menu open, the first moment of a
-- stay.
--
-- DRAWN when there is no picture to go by: before the watcher's first in a stay — the Melodyne
-- the overlay comes back to, or another of its windows, need not have the tool it left behind —
-- and after a tool switch until a picture asked once its hold has lifted (forgetBoxes), since which
-- boxes are drawn is a property of the TOOL. A control that quietly vanishes tells a user who
-- cannot see the screen nothing about why, and "there is a stop here that says no value" is a far
-- smaller failure than "the stop you were looking for is gone".
local boxMemo = nil
forgetBoxes = function()
  boxMemo = nil
end
-- What a picture asked at `asked`, in the stay `mark` notes, showed of the boxes, noted for
-- boxDrawn: `drawn` as boxesIn answers it. Not one asked before a tool switch's hold lifted: the
-- strip does not repaint within one tick (holdReadouts), so a picture asked just after the switch
-- can show the old tool's boxes as surely as one asked before it — and kept, it would hide a stop
-- the new tool draws until the next tick.
local function rememberBoxes(drawn, mark, asked)
  if asked ~= nil and asked < holdUntil then return end
  boxMemo = { left = drawn.left, right = drawn.right, mark = mark }
end
local function boxDrawn(which)
  return boxMemo == nil or not ov:stillHere(boxMemo.mark) or boxMemo[which]
end
local function leftBoxDrawn() return boxDrawn("left") end
local function rightBoxDrawn() return boxDrawn("right") end

-- What the left field is showing, in the words of the tool that owns it. Nil when no tool
-- reads as lit, in which case the label alone is the honest answer.
local function inspectorOwner(o)
  local i = activeToolCached(o)
  if not i then return nil end
  local tool = TOOLS[i]
  local v = activeVariantCached(o, tool)
  if v > 1 and tool.variants and tool.variants[v - 1] then
    return tool.variants[v - 1].name
  end
  return tool.name
end

ov:addOCRButton({
  label = "Inspector",
  region = NOTE_REGION,
  readOnly = true,
  -- Gated on its own box being drawn, like the second one. Under the FADE tool that box is
  -- drawn and EMPTY — measured: borders 157 and 194 fill, and not a mark inside it — which is
  -- Melodyne having a field with nothing to put in it (Celemony's own Note Inspector leaves
  -- fades out of its parameter list too). So the stop stays and says so. SIBIAC's model allows
  -- for ZERO controls under some tool, and this is what will hide the stop when that turns up,
  -- without anybody having to work out which tool it was.
  when = leftBoxDrawn,
  -- The plug-in's terms, not the recogniser's: a box that is genuinely empty is Melodyne
  -- having no value to show, and "no text" would report on us instead. The AutoHotkey draft
  -- this is ported from said exactly this, and it was right.
  fallback = "no value",
  -- Between the label and the value, so it reads "Inspector, Amplitude, -3.0 dB" — the unit
  -- alone would not say which parameter it belongs to, and the tool strip is a Tab away.
  text = inspectorOwner,
})
-- The second field, and it is ALWAYS there.
--
-- This control was briefly gated to the Pitch base tool, on the reasoning that the other tools
-- show a single value and the rest of that strip is bare tool bar. The user said otherwise from
-- memory — the deviation had been there under the Main tool too — and the picture settles it:
-- modules/overlay-runtime/calibration/Melodyne-clean.png, taken with the MAIN tool lit, shows
-- BOTH boxes drawn, each holding a "-". The calibration shot beside it puts a crosshair in the
-- middle of each, so these two regions are the two boxes and nothing else.
--
-- So the box does not come and go; its CONTENT does, and "-" is Melodyne's own way of saying it
-- has nothing to report. A control that disappears when the value is a dash would be a worse
-- lie than one that reads the dash out: the user would learn that the field exists only
-- sometimes, which is not true of anything on that tool bar.
--
-- The gate was half right, and the log from the day this overlay was written says which half:
-- 178 readings taken when the code read both regions unconditionally, 167 of them with a cents
-- value on the right. The eleven without are exactly the single-field tools — Formant, Amplitude,
-- Pitch Modulation, one Time sub-tool — and the Main tool appears three separate times with BOTH
-- fields. So the Pitch SUB-tools really are single-field, and the Main tool never was.
--
-- None of which needs to be in the code. A tool with one field leaves the second region bare, the
-- recogniser reads nothing there, and nothing is announced — the measurement does the work a
-- table of tool names would do worse, and it cannot go stale when Celemony moves something.
-- Time and Note Separation are still unmeasured; see the tool bar's `onSelect`.
ov:addOCRButton({
  label = "Pitch deviation",
  region = PITCH_REGION,
  readOnly = true,
  -- And this is the one that needs it most, because under a single-parameter tool the box is
  -- GENUINELY NOT THERE — measured within minutes of `BOX_EDGES` existing, which is the whole
  -- case for measuring instead of arguing:
  --
  --   fields under tool=Pitch:     left="Ab 4"    right="-1 ct"  box borders 146 146 146 146
  --   fields under tool=Formant:   left="0 ct"    right=""       box borders 146 146 191 191
  --   fields under tool=Amplitude: left="0.00 dB" right=""       box borders 146 146 191 191
  --
  -- Both boxes drawn under Pitch; under the single-parameter tools the right pair of borders
  -- reads bare tool bar. SIBIAC's "zero, one or two parameter controls" was right, and so was
  -- the reference document's mechanism — only its list of which tools left the Main tool out.
  --
  -- So there is no stop here when there is no box. The control is gated on the MEASUREMENT
  -- above, and that is the whole difference from the gate this file used to carry: that one
  -- asked which tool was selected and answered from a list; this one asks what Melodyne has
  -- drawn. A Tab stop for a field the application has taken away is a promise of something to
  -- read that is not there — and the user said so plainly, twice.
  when = rightBoxDrawn,
  -- Still needed with the gate in place: the box can be present and EMPTY. Nothing selected
  -- shows Melodyne's dash, which the recogniser drops, and "no value" is then the honest
  -- answer — not silence, and not a sentence about optical character recognition.
  fallback = "no value",
})

-- ── Announcing a new selection without being asked ────────────────────────────
-- Selecting a different note changes the read-out, and a user working in the editor should not
-- have to tab away to find out what they just landed on.
--
-- Watched rather than triggered by the keys that do it, which was the first idea and the wrong
-- one: those keys are Melodyne's, they pass through untouched by design, and adding a way to
-- observe them would put a second mechanism in the way of the first. Reading the result is both
-- simpler and truer — it catches a selection made with the mouse, or by Melodyne itself, which
-- watching our own keys never would.
--
-- ONE announcement for whatever fields are there. When the Pitch tool gives two, they change
-- together, so a watcher per field would say the note and then the deviation as two separate
-- interruptions for a single event.
--
-- The two regions are adjacent on the same line, so the CHANGE DETECTOR is a single read across
-- both of them. That keeps the steady state at one OCR per tick rather than two, and only when
-- it differs are the two read separately for the announcement.
-- 500 ms was the first guess and far too slow to follow a note-by-note walk. 120 is roughly a
-- key repeat, so a held arrow is tracked rather than sampled.
--
-- It is not free: one OCR plus the screen capture under it, and a capture costs a fixed
-- compositor frame (~17 ms, measured) whatever its size. So the watcher reports its own cost
-- every so often — how long its reads take from asking to the answer, on the recogniser that
-- Tab's own reads share — and the interval is a number to be chosen from evidence; going faster
-- still is a trade that should be made with the figure in view rather than by feel.
local WATCH_MS = 120
local watchReads, watchCost = 0, 0

-- ONE CACHE PER FIELD, because a change in one box is not news about the other.
--
-- Walking a note's pitch up in cents read the note name again at every step — "A4, +12 cents",
-- "A4, +13 cents", "A4, +14 cents" — when the only thing that had changed was the number. The
-- name is the part the user already knows; it is the part they are holding in their head while
-- they listen for the number. So each box is remembered on its own and only the boxes that
-- CHANGED are spoken: "A4, +12 cents", then "+13 cents", "+14 cents", and the note name comes
-- back exactly when the note does — "A#4, +0 cents".
--
-- Still ONE utterance per tick, both changed boxes in it. Two utterances for one event would be
-- two interruptions, which is what the single combined cache was avoiding when it caused this.
local lastLeft, lastRight = "", ""
-- Where the overlay was when the values above became the baseline: a mark of O:here(), or nil
-- before the first read of a stay. See the watcher at the bottom of this file.
local baseline = nil

-- Whitespace off both ends, so a stray space from the recogniser does not read as a change.
local function trimmed(v)
  return (tostring(v or ""):match("^%s*(.-)%s*$"))
end

-- Worth saying out loud. A dash is Melodyne's "no single value": it stands there when nothing
-- is selected (measured — see docs/melodyne-reference.md §6.1) and when SEVERAL notes are and
-- they disagree. Every width the recogniser might hand back, and the three-hyphen form too:
-- two Celemony figures and five pages say one dash, one English page says three, so the
-- reference document's own instruction is to tolerate both (§7).
--
-- So a value is what has a letter or a digit in it — a note name, cents, decibels, a position —
-- and nothing else is. That covers every dash above, and the recogniser catching a box while
-- Melodyne is still drawing it: on 2026-10-10 the first frames of a stay read "† -" and then a
-- lone dot in each box, and the dots were said, which the user heard as ", ".
local function speakable(v)
  return type(v) == "string" and v:find("%w") ~= nil
end

-- TWO reads, one per field, and one announcement.
--
-- This was briefly a single read across both fields, split afterwards by the OCR's own word
-- boxes -- cheaper by a whole screen capture per tick, measured at 19 ms against 31. It was
-- also wrong, and the log said so plainly: read "+36 Ct", words +36@319 Ct@342, note empty.
-- The note field returned nothing at all, while reading it on its own works.
--
-- The reason is in this project's OCR pipeline rather than in Melodyne. WinRT is the primary
-- engine and the embedded PaddleOCR is a fallback that runs only when WinRT comes back EMPTY
-- for a small region -- which is exactly the case the note field needs, and exactly why the
-- AutoHotkey draft reached for Tesseract with the note that UWP OCR could not read this small
-- text. Merge the two regions and WinRT is no longer empty, because the deviation reads fine;
-- the fallback never fires and the note is lost.
--
-- So the fallback is PER REGION, and merging regions disables it. The saving was real and it
-- cost the feature.
--
-- The regions, one per box, in screen coordinates: nil while the overlay has no origin.
local function selectionRegions(o)
  local origin = o:origin()
  if not (origin and origin.client) then return nil end
  local function box(region)
    return {
      origin.client.x + region[1], origin.client.y + region[2],
      origin.client.x + region[3], origin.client.y + region[4],
    }
  end
  -- The left box always; the right one only when it is DRAWN, and this time the condition is a
  -- measurement rather than a table of tool names.
  --
  -- What is MEASURED: the watcher's own instrumentation reports ~25-32 ms per tick while it
  -- read one box, ~40-55 while it read two with content in them, and 64-74 during a session
  -- spent in the single-field tools — where the second read lands on bare tool bar. Reading a
  -- box the application is not drawing costs something real, eight times a second, forever.
  --
  -- What is NOT measured, and what I asserted before the user rightly asked: WHY. The backend
  -- runs the neural recogniser concurrently and waits for it only when WinRT comes back empty,
  -- so the theory is that an empty region pays that wait — but the empty case is also the one
  -- where `tighten` has no content to crop to, and neither half was ever timed. The OCR line
  -- now reports both. Until it has said which, the honest statement is the first paragraph.
  --
  -- An earlier version of this file skipped the read too, and was wrong for a different reason:
  -- it asked which TOOL was selected and looked the answer up in a list. Skipping because we
  -- LOOKED and the box is not drawn is the same saving without the claim.
  if not boxDrawn("right") then return { box(NOTE_REGION) } end
  -- BOTH boxes, one screen touch. A list of regions in one host.ocr.recognize is photographed
  -- together and each region recognised out of that one picture — which is the whole cost here:
  -- the recognition measured 4-6 ms against a capture's fixed ~17. Two separate calls put this
  -- tick at 44 ms; this puts it at 27, and the two regions stay two regions, so the per-region
  -- fallback that once rescued a dropped note name is untouched.
  return { box(NOTE_REGION), box(PITCH_REGION) }
end

-- One box as the watcher compares it: the text on ONE line, or nil when the box could not be
-- read this time.
--
-- Nil is not "". A read that failed — the recogniser not answering, too many reads waiting — or
-- went stale says nothing about the box, and taking it for an empty one would make the read-outs
-- look silent, which switches the note-area measurement and its sentences on. One line, because
-- a reading's `text` joins its rows with "\n" and `trimmed` leaves the inside of a value alone: a
-- value the recogniser happened to split over two rows would read as a change made of a line
-- break.
local function field(r)
  if r == nil or r.status == "failed" or r.status == "stale" then return nil end
  return (r.text:gsub("%s+", " "))
end

-- What one read of those regions says: both boxes joined, for the change detector, each box on
-- its own, and whether the right one was read at all; nil when either could not be read.
local function selectionOf(list)
  local note = field(list[1])
  local pitch = ""
  if list[2] ~= nil then pitch = field(list[2]) end
  if note == nil or pitch == nil then return nil end
  return note .. "|" .. pitch, note, pitch, list[2] ~= nil
end

-- The strip the watcher photographs, in client coordinates: both boxes, their borders and the
-- bare tool bar beside each — every point BOX_PROBES and BOX_INK name.
local STRIP = { 212, 60, 372, 76 }

-- Which boxes are drawn in picture `snap` of the window whose client area `origin` names, { left,
-- right }, from the four points of BOX_PROBES in one `pixels` call; nil when the picture does not
-- hold them.
local function boxesIn(snap, origin)
  local pts = {}
  for _, which in ipairs({ "left", "right" }) do
    local p = BOX_PROBES[which]
    pts[#pts + 1] = { origin.client.x + p.edge[1], origin.client.y + p.edge[2] }
    pts[#pts + 1] = { origin.client.x + p.gap[1], origin.client.y + p.gap[2] }
  end
  local px = host.screen.pixels(pts, { snapshot = snap })
  if not px then return nil end
  return {
    left = (px[2].r - px[1].r) > BOX_EDGE_CONTRAST,
    right = (px[4].r - px[3].r) > BOX_EDGE_CONTRAST,
  }
end

-- WHETHER A BOX HOLDS A VALUE, from the picture rather than from the recogniser.
--
-- The recogniser cannot say. Melodyne's "-" is three ink pixels on one row, which it drops as it
-- drops an empty box — and as it drops a value now and then — so an empty read says nothing
-- about which of the three it was. Taking it for silence made a dropout under the Pitch tool
-- open the note area's gate; taking it for nothing at all kept the gate shut under Time, Note
-- Separation and Fade, whose boxes are empty or not drawn, which is where the note area is the
-- only answer there is. The pixels tell them apart. Measured on the calibration captures, in rows
-- 63-72 of the box, against the box's own fill (194 in one appearance, 209 in another):
--   a value ("0 ct", "0.00 dB")                    ink in 8 and 9 rows, as dark as 0
--   the dash (Melodyne-clean.png, both boxes)      one row, 94
--   the Fade tool's empty box (Melodyne-3-clean)   none
-- So a value is ink in three rows or more, ink being 40 or more below the fill: the box's own top
-- border, the nearest thing to ink in an empty box, is 13 to 23 below it. Measured against the
-- box's own fill, never a fixed grey (see BOX_PROBES). Every capture shows dark text on a light
-- box; a light-on-dark appearance has never been seen, and there only a value the recogniser read
-- would say the read-outs speak.
local INK_ROWS, INK_CONTRAST = 3, 40
local BOX_INK = { left = { 220, 63, 287, 73 }, right = { 300, 63, 367, 73 } }

-- The keys the two boxes' profiles go under: one each, since a newer profile with a key replaces
-- the one out before it.
local INK_KEYS = { left = "left box", right = "right box" }

-- Whether box `which` holds a value in picture `snap`, handed to `done`: true or false, nil when
-- the picture does not hold the box. A profile of its rows, ten rows of 67 pixels, reduced off the
-- event loop and answered as a handler of its own.
--
-- WITH A CALLBACK, for the reason the watcher's read has one. This runs in the answer to the
-- watcher's picture, a handler of this module's, and a handler that waits keeps Melodyne's own
-- keys waiting with it. A profile without a callback is to wait for the image worker, as every
-- `host.screen` call that captures or walks a region is to, and two of them eight times a second
-- would put Tab and the F-keys back behind the watcher.
local function inkIn(snap, origin, which, done)
  local b = BOX_INK[which]
  host.screen.profile({
    snapshot = snap, axes = "rows", key = INK_KEYS[which],
    region = {
      origin.client.x + b[1], origin.client.y + b[2],
      origin.client.x + b[3], origin.client.y + b[4],
    },
  }, function(p)
    if not (p and p.rows and p.rows.min and p.rows.max and #p.rows.max > 0) then
      return done(nil)
    end
    -- The fill is the lightest pixel of a row, in most rows: a value never fills a row.
    local lightest = table.clone(p.rows.max)
    table.sort(lightest)
    local fill = lightest[(#lightest + 1) // 2]
    local rows = 0
    for _, darkest in ipairs(p.rows.min) do
      if darkest <= fill - INK_CONTRAST then rows += 1 end
    end
    done(rows >= INK_ROWS)
  end)
end

-- What picture `snap` shows in the boxes `drawn` says are drawn, both asked at once and handed to
-- `done` once both have answered: { left, right }, a box that is not drawn holding none, or nil
-- when a drawn one could not be measured. At once when neither is drawn.
local function inksIn(snap, origin, drawn, done)
  local ink, out, failed = { left = false, right = false }, 0, false
  for _, which in ipairs({ "left", "right" }) do
    if drawn[which] then out += 1 end
  end
  if out == 0 then return done(ink) end
  for _, which in ipairs({ "left", "right" }) do
    if drawn[which] then
      inkIn(snap, origin, which, function(has)
        if has == nil then failed = true else ink[which] = has end
        out -= 1
        if out == 0 then done(not failed and ink or nil) end
      end)
    end
  end
end

-- Whether the read-outs say nothing at all, from one read and the picture it was read from: false
-- when a box shows a value, read or seen as ink, and true when none does; nil when that cannot be
-- told, which leaves the last answer standing. `ink` is what the picture showed of each box
-- (inkIn; a box that is not drawn holds none), nil when it could not be measured — and then only
-- dashes are silence, since an empty read is not an empty field.
local function silenceOf(left, right, both, ink)
  if speakable(left) or (both and speakable(right)) then return false end
  if ink then return not (ink.left or ink.right) end
  if left == "" or (both and right == "") then return nil end
  return true
end

-- Both boxes read now, the handler waiting for them: for the reading after a tool switch, which
-- is written to the log. The watcher at the bottom of this file asks with a callback instead.
local function readSelection(o)
  local regions = selectionRegions(o)
  if not regions then return nil end
  return selectionOf((host.ocr.recognize(regions)))
end

-- One reading per tool switch, written down and not spoken. See the tool bar's `onSelect`.
--
-- Timed to land after the strip has repainted and before the hold lifts, so it measures the new
-- tool rather than the old one and cannot itself become an announcement. It says nothing to the
-- user; it is here so that the question "what does THIS tool show in those two boxes" is
-- answered by using Melodyne, by whoever is already using it, rather than by anybody
-- describing a screen.
-- ── Position ─────────────────────────────────────────────────────────────────
--
-- The transport's position read-out in the top bar, measured in our own calibration capture:
-- field interior x408-530, y12-34, ink x451-488, glyphs 14 px tall — a third taller than the
-- tool-bar boxes, on a clear background with no neighbouring text. The easiest OCR target in
-- the whole window.
--
-- The one numeric time read-out in the whole application, and it took a differential pixel
-- measurement to find it — not in the note editor, where everybody looked, but in the transport.
--
-- The evidence, from the user's own session: walking notes with the arrow keys moves a
-- two-pixel, near-full-height line across the note area, and this field changes in lockstep
-- with it, backwards steps included —
--
--   x869-870 / x888-889   transport="38.1.3"
--   x892-893 / x888-889   transport="38.1.4"
--   x880-881 / x892-893   transport="38.1.2"   <- the line went back; so did the number
--   x893-894 / x907-908   transport="38.2.1"
--
-- So the line is Melodyne's position cursor, selecting a note takes the cursor to it, and the
-- transport prints where it is. Bar.beat.sixteenth, for tools that display nothing at all: Time,
-- Time Handle, Note Separation, Fade. It is the POSITION OF THE CURSOR, which is the selected
-- note's position while you are walking notes and the playhead's while you are playing — and
-- that is what the label says, because those are not the same claim.
--
-- Not in the selection watcher: it would speak on every arrow press, over the note name and the
-- cents. A stop you can reach when you want it.
local TRANSPORT_REGION = { 408, 12, 530, 34 }

-- Checked against the shape Melodyne prints, and re-read once when it does not fit.
--
-- The raw recogniser is not reliable enough here to speak unedited, and the log says so: over a
-- single playback it produced "42.2.2", "43.1.1", "44.1.2" — and also "441.1", "442.2", "42. I",
-- "..", and a lone ".". The field itself is not the problem; it is the largest, cleanest text in
-- the window. The reads land mid-repaint, because the number changes several times a second
-- while the transport runs.
--
-- So: three groups of digits with SOMETHING between them (the separator glyph is the first
-- thing lost, and which glyph it was does not matter), and beat and sixteenth within a bar's
-- range. One retry, because a second look costs one frame and mid-repaint failures do not
-- repeat. When it still does not fit, the raw text is announced as itself rather than dressed
-- up as a position — "441.1" spoken as "bar 441" would be a confident lie, and this whole
-- evening is a record of what those cost.
--
-- The range check also guards a case nobody has tested: Melodyne's ruler can be switched to
-- seconds, and "0:12.345" would parse as three groups just as happily. A sixteenth of 345 is
-- not a sixteenth, so it falls through to the raw reading instead of being mislabelled.
local function readPosition(o)
  local origin = o:origin()
  if not (origin and origin.client) then return nil end
  local function once()
    local r = host.ocr.recognize({
      region = {
        origin.client.x + TRANSPORT_REGION[1], origin.client.y + TRANSPORT_REGION[2],
        origin.client.x + TRANSPORT_REGION[3], origin.client.y + TRANSPORT_REGION[4],
      },
    })
    return trimmed(r and r.text)
  end
  local function musical(s)
    local bar, beat, six = s:match("^(%d+)%D+(%d+)%D+(%d+)$")
    if not bar then return nil end
    if tonumber(beat) > 16 or tonumber(six) > 16 then return nil end
    return ("bar %s, beat %s, sixteenth %s"):format(bar, beat, six)
  end
  local raw = once()
  local said = musical(raw)
  if not said then
    local again = once()
    said = musical(again)
    if not said then
      host.log.info(("[melodyne] position not in bar.beat.sixteenth form: %q then %q")
        :format(raw, again))
      return again ~= "" and again or (raw ~= "" and raw or nil)
    end
  end
  return said
end

ov:addStaticText({ label = "Position", text = readPosition })

-- ── What Melodyne is busy with ────────────────────────────────────────────────
-- Melodyne analyses audio before it can be edited, and while it does it draws a progress disc in
-- the middle of the editor with the name of the pass across it ("Polyphonic Detection"). For a
-- sighted user that is unmissable. Without it, a Melodyne that is merely busy sounds broken: the
-- arrow keys reach one note and no further, and nothing says why. This read-out was in the overlay
-- on the day it was written and went out by accident with an unrelated change that evening; the
-- maintainer missed it as "the loading indicator".
--
-- WHETHER the disc is up is decided from pixels, and only the pass name is read as text. The
-- recogniser drops out on a frame now and then, and an empty read is not an empty field: an
-- analysis declared finished on one empty read would be announced in the middle of it.
--
-- Measured on calibration/Melodyne-clean.png (962x660, the disc part filled, "Polyphonic
-- Detection"), against Melodyne-2/3/4-clean.png (the same window, no disc, notes everywhere):
--   * The disc is centred 4 px above the middle of the client area, at 481,326, radius 52. Its rim
--     is a ring two pixels wide that reads 71-137 against 181-250 just inside it and 188-250 just
--     outside, at all sixteen angles sampled.
--   * The caption band runs across the disc for 13 rows (320-332) and HALVES whatever is under it:
--     250 becomes 137 and 188 becomes 106. So the band's top row is 0.55-0.56 of the row two above
--     it, and its bottom row of the row two below it — in every column but where a descender (the
--     y of "Polyphonic") crosses the bottom row.
--   * The test is both: the band's two edges in at least 9 and 6 of 11 columns 8 px apart (found:
--     11 and 10), then the rim around the centre that band puts the disc at in at least 12 of 16
--     angles (found: 16). On the three captures without a disc the pair of edges was found NOWHERE
--     in the note area — at none of the 141,750 places the test was tried — and the rim at the
--     disc's own place scores 2 of 16.
--
-- Ratios and contrasts, never fixed greys, because Melodyne's greys move with its appearance
-- setting (see BOX_PROBES). And the band is looked for 8 rows up and down, because the one capture
-- with a disc has one window size and what centres the disc is not measured: the centre is taken
-- from the band that is found.
local DISC_DY = -4
local BAND_COLS = { -40, -32, -24, -16, -8, 0, 8, 16, 24, 32, 40 }
local BAND_ROWS = 13
local BAND_SEARCH = 8
local BAND_LOW, BAND_HIGH = 0.45, 0.72
local BAND_TOP, BAND_BOTTOM = 9, 6
local RIM_RADII = { 50, 51, 52, 53, 54 }
local RIM_INSIDE, RIM_OUTSIDE, RIM_CONTRAST, RIM_NEED = 44, 60, 40, 12
-- Sixteen angles, none of them level with the band, so no inside point lands in it.
local RIM_ANGLES = {}
for k = 0, 15 do
  local a = math.rad(11.25 + 22.5 * k)
  RIM_ANGLES[#RIM_ANGLES + 1] = { math.cos(a), math.sin(a) }
end

local function luminance(p) return 0.299 * p.r + 0.587 * p.g + 0.114 * p.b end
local function round(v) return math.floor(v + 0.5) end

-- How many of the sixteen angles show the rim around (cx, cy), in snapshot `snap`: nil when the
-- snapshot does not hold the points.
local function rimAround(snap, cx, cy)
  local pts = {}
  for _, a in ipairs(RIM_ANGLES) do
    for _, r in ipairs(RIM_RADII) do pts[#pts + 1] = { cx + round(r * a[1]), cy + round(r * a[2]) } end
    pts[#pts + 1] = { cx + round(RIM_INSIDE * a[1]), cy + round(RIM_INSIDE * a[2]) }
    pts[#pts + 1] = { cx + round(RIM_OUTSIDE * a[1]), cy + round(RIM_OUTSIDE * a[2]) }
  end
  local px = host.screen.pixels(pts, { snapshot = snap })
  if not px then return nil end
  local per, n = #RIM_RADII + 2, 0
  for i = 1, #RIM_ANGLES do
    local base = (i - 1) * per
    local rim = math.huge
    for j = 1, #RIM_RADII do rim = math.min(rim, luminance(px[base + j])) end
    local inside, outside = luminance(px[base + per - 1]), luminance(px[base + per])
    if rim <= math.min(inside, outside) - RIM_CONTRAST then n += 1 end
  end
  return n
end

-- Where the disc is in snapshot `snap`, for a disc expected centred on (cx, cy) on the screen: the
-- screen row its caption band begins at, false when there is no disc, and nil when the snapshot
-- does not hold the points. Several hundred points read from the snapshot, which touches no screen.
local function discIn(snap, cx, cy)
  local first, last = cy - 6 - BAND_SEARCH - 2, cy - 6 + BAND_SEARCH + BAND_ROWS + 1
  local pts = {}
  for y = first, last do
    for _, dx in ipairs(BAND_COLS) do pts[#pts + 1] = { cx + dx, y } end
  end
  local px = host.screen.pixels(pts, { snapshot = snap })
  if not px then return nil end
  local cols = #BAND_COLS
  local function lum(y, k) return luminance(px[(y - first) * cols + k]) end
  local function halved(band, beside)
    return beside > 0 and band / beside >= BAND_LOW and band / beside <= BAND_HIGH
  end
  for top = cy - 6 - BAND_SEARCH, cy - 6 + BAND_SEARCH do
    local bottom = top + BAND_ROWS - 1
    local upper, lower = 0, 0
    for k = 1, cols do
      if halved(lum(top, k), lum(top - 2, k)) then upper += 1 end
      if halved(lum(bottom, k), lum(bottom + 2, k)) then lower += 1 end
    end
    if upper >= BAND_TOP and lower >= BAND_BOTTOM then
      local rim = rimAround(snap, cx, top + 6)
      if rim == nil then return nil end
      if rim >= RIM_NEED then return top end
    end
  end
  return false
end

-- Where on the screen the disc's centre is expected, for an origin whose content is `cw` x `ch`.
local function discCentre(origin, cw, ch)
  return origin.client.x + cw // 2, origin.client.y + ch // 2 + DISC_DY
end

-- Where the disc's centre IS across, in snapshot `snap`, for a disc found about `cx` whose band's
-- middle row is `cy`: the screen column, or `cx` when the rim does not say.
--
-- Half the client width is where the centre was on the one window size measured, and the rim test
-- above finds the disc two pixels either side of it. The pass name's crop needs more: two pixels
-- left of the centre both crops read "Polyphonic Detectior", which passes every test a read is put
-- to. Its row is measured already — the band's own top (discIn); one row off, both crops read "o
-- honic Detecti" — and now its column is too. On twelve rows
-- clear of the band, 10 to 30 above and below its middle, the rim crosses twice, as far left of
-- the centre as right of it; each crossing is the darkness-weighted middle of the darkest pixel of
-- its side and the two either side of it, and the centre is the median of the rows' midpoints. On
-- both captures with a disc this put it at 481.0 and 480.85 from guesses up to 4 px off on either
-- side. More than 3 px from `cx` is something else dark beside the rim — a note seen through the
-- unfilled part of the pie — and the guess stands.
local CROSS_ROWS = { -30, -26, -22, -18, -14, -10, 10, 14, 18, 22, 26, 30 }
local RIM_R = 52
local function discX(snap, cx, cy)
  local pts, sides = {}, {}
  for _, dy in ipairs(CROSS_ROWS) do
    local ex = math.floor(math.sqrt(RIM_R * RIM_R - dy * dy))
    for _, from in ipairs({ cx - ex - 7, cx + ex - 6 }) do
      sides[#sides + 1] = { from = from, first = #pts + 1 }
      for x = from, from + 12 do pts[#pts + 1] = { x, cy + dy } end
    end
  end
  local px = host.screen.pixels(pts, { snapshot = snap })
  if not px then return cx end
  local function crossing(side)
    local lum, k, top = {}, 1, -math.huge
    for i = 1, 13 do
      lum[i] = luminance(px[side.first + i - 1])
      if lum[i] < lum[k] then k = i end
      top = math.max(top, lum[i])
    end
    local sum, weight = 0, 0
    for i = math.max(1, k - 2), math.min(13, k + 2) do
      sum += (side.from + i - 1) * (top - lum[i])
      weight += top - lum[i]
    end
    return weight > 0 and sum / weight or nil
  end
  local mids = {}
  for i = 1, #sides, 2 do
    local left, right = crossing(sides[i]), crossing(sides[i + 1])
    if left and right then mids[#mids + 1] = (left + right) / 2 end
  end
  if #mids == 0 then return cx end
  table.sort(mids)
  local n = #mids
  local m = n % 2 == 1 and mids[(n + 1) // 2] or (mids[n // 2] + mids[n // 2 + 1]) / 2
  if math.abs(m - cx) > 3 then return cx end
  return round(m)
end

-- The picture discIn needs around a centre: the band's columns and rows, and the rim's points out
-- to RIM_OUTSIDE, with a few pixels to spare.
local function discRegion(cx, cy)
  return { cx - 64, cy - 72, cx + 65, cy + 73 }
end

local function nativeMenuOpen()
  return host.keys.nativeMenuOpen ~= nil and host.keys.nativeMenuOpen() == true
end

-- Looked for twice a second, and only while the overlay is in front. An analysis lasts seconds to
-- minutes, so half a second says "finished" within a second of it, and each look costs one capture
-- of 129 x 145 pixels on the capture thread and a few hundred pixels read from it on the event
-- loop — well under a millisecond there. The pass name is read from the same picture.
local BUSY_MS = 500
local DISC_KEY = "analysis disc"
local PASS_KEY = "analysis pass"

-- The analysis seen in this stay: where the overlay was when its disc was first seen, the pass
-- name once taken (see countPass) with the looks that read each name, the last name read and how
-- many looks in a row read it, and the reads that were not taken, how far across from the window's
-- middle the disc's centre was measured, whether "Busy" was said, and how many looks in a row since
-- found no disc. nil when there is none.
local analysis = nil
-- Where the overlay was at the last look that found no disc: the read-out says "idle" after one.
local idleMark = nil
-- Where the overlay was as it came to the front: "Busy" waits until an announcement has been said
-- since — its own arrival sentence, or a key's.
local arrivalMark = nil

local function analysingHere()
  return analysis ~= nil and ov:stillHere(analysis.mark)
end

-- "Busy: <pass>", once per analysis, once its name is taken — and after the overlay's own arrival
-- sentence, never in front of it. Both are queued, so the one handed to speech first is heard
-- first; until an announcement of this stay has been SAID, the next look asks again. Said, not
-- begun: a sentence that reads its control — "Inspector", "Pitch deviation" — begins at once and
-- is said when its read answers, and "Busy" asked in between went out ahead of it.
local function sayBusy(a)
  if a.said or a.pass == nil or analysis ~= a or not analysingHere() then return end
  if arrivalMark and ov:stillHere(arrivalMark, { spoken = true }) then return end
  a.said = true
  host.speech.output("Busy: " .. a.pass, { interrupt = false })
end

-- The pass name, read from the look's own picture, with a callback. One read out at a time; an
-- empty or failed one says nothing, and the next look reads again.
--
-- The caption alone, without the rim. Melodyne cuts the caption at the disc: on both captures with
-- a disc (Melodyne-clean.png, Melodyne-5-clean.png) its ink runs from 50 px left of the centre to
-- 50 right of it, the band's whole inside, and the rim begins at 51 on the right and 52 on the
-- left. The rim is not dark like the band everywhere: on the right it reads 127-148, lighter than
-- the band where the pie has filled it (106), and the recogniser's upscaling makes it a bar after
-- the last letter — the "l" of "Detectionl", which Windows' recogniser answered for most of a live
-- analysis. The crop from rim to rim read neither capture right. Columns 50 left of the centre to
-- 49 right of it read "Polyphonic Detection" on both, in English and in German, and in 40 of 41
-- fill states of the disc made from the two captures. The centre is the measured one (discX).
--
-- In English, which is what Melodyne shows here: over those fill states the English recogniser
-- read the name in 40, the user's German one in 37. Where English cannot be read — a Windows
-- without its text recognition — the first read that says so (answered "failed" with no
-- language, and English not among the recogniser's languages) turns the reads to the user's own
-- language for the rest of the session. The languages are asked only then, after a read has
-- answered, when their list is known and asking does not wait. A Melodyne in another language
-- shows its own name; how the English recogniser reads its accents is not measured, and a name
-- that is not read is "busy", never a misreading.
--
-- And TWICE, in one call: the same crop and one a column narrower on the left. Windows' recogniser
-- answers a crop one pixel off with another misreading, while a name that is really there reads
-- the same in both — so a read whose two crops differ is not taken. Over the 41 fill states the two
-- agreed in 39, and every time on the right name. Each crop is a region of its own, so each keeps
-- the neural recogniser that answers when Windows' reads nothing — every exact read of the live
-- session came from it.
local PASS_COLS = { -50, -49 }
local passLang = "en"
-- Looks in a row that must have read a name before it is the pass: one agreeing read can still be
-- a misreading that both crops share, and a name the pass really has is read again from the next
-- picture.
local PASS_LOOKS = 2

-- Defined below, with the clock that makes it: the read of a first name asks for the next picture
-- at once.
local look

-- A name as Melodyne prints one: words of letters, one space between them, nothing else — letters
-- of the Latin script, which is what the English recogniser reads and what a translated Melodyne's
-- accented letters are. Not Luau's %a, which knows no letter past ASCII. A misreading that slipped
-- through comes out as "Pöbhffoiü@.petectionl".
local function passLike(text)
  if utf8.len(text) == nil then return false end
  local before = 32
  for _, c in utf8.codes(text) do
    local letter = (c >= 65 and c <= 90) or (c >= 97 and c <= 122)
      or (c >= 0xC0 and c <= 0x24F and c ~= 0xD7 and c ~= 0xF7)
    if not (letter or (c == 32 and before ~= 32)) then return false end
    before = c
  end
  return before ~= 32
end

-- One look's read, counted: a name both crops read the same, or a read not taken. A name is the
-- pass once PASS_LOOKS looks in a row have read it — a read not taken between them counts neither
-- way — so a pass that follows another is taken at its second read however long the one before it
-- ran, and a misreading both crops shared once never is. The log says when a name is taken and the
-- first read that was not; the finished line counts the rest. True when the read counted for a
-- name.
local function countPass(a, one, two)
  if one ~= two or not passLike(one) then
    a.rejected += 1
    if a.rejected == 1 then
      host.log.info(("[melodyne] analysis pass: not taken, read %q and %q"):format(one, two))
    end
    return false
  end
  a.looks[one] = (a.looks[one] or 0) + 1
  a.counted += 1
  if a.last == one then a.inRow += 1 else a.last, a.inRow = one, 1 end
  if a.inRow < PASS_LOOKS or one == a.pass then return true end
  host.log.info(a.pass == nil
    and ("[melodyne] analysis found: %q (%d looks read it, %d reads not taken; the disc's centre %+d px"
      .. " across from the window's middle)"):format(one, a.looks[one], a.rejected, a.offset)
    or ("[melodyne] analysis pass now %q, was %q (%d looks in a row)"):format(one, a.pass, a.inRow))
  a.pass = one
  return true
end

-- The first name an analysis reads is not said on its own, and the next look would be up to half a
-- second away — after the arrival sentence, which says "busy" for want of a name. So that read asks
-- for the next picture at once: once per analysis, after its first counted read, so a recogniser
-- that keeps answering new names never keeps the capture thread busy. `cx` is where the disc was
-- looked for, `mx` where its centre was measured.
local function askPass(snap, cx, mx, top, a)
  if host.ocr.pending(PASS_KEY) then return end
  a.offset = mx - cx
  local crops = {}
  for i, dx in ipairs(PASS_COLS) do crops[i] = { mx + dx, top, mx + 50, top + BAND_ROWS } end
  host.ocr.recognize(crops, { snapshot = snap, key = PASS_KEY, lang = passLang },
    function(list)
      local first = list[1]
      -- Only against a known list: before it is published, resolveLanguage answers nil for every
      -- language, and a read refused that early says nothing about English.
      if passLang and first and first.status == "failed" and first.lang == ""
        and #host.ocr.languages() > 0 and host.ocr.resolveLanguage(passLang) == nil then
        host.log.info("[melodyne] analysis pass: English is not read here; the user's language from now on")
        passLang = nil
      end
      local one, two = field(list[1]), field(list[2])
      if one == nil or two == nil then return end
      one, two = trimmed(one), trimmed(two)
      if one == "" and two == "" then return end
      if analysis ~= a or not ov:stillHere(a.mark) or nativeMenuOpen() then return end
      local counted = countPass(a, one, two)
      sayBusy(a)
      if counted and a.pass == nil and a.counted == 1 then look() end
    end)
end

-- A look that found the disc: the analysis of this stay, begun now when there was none.
local function discFound(mark)
  if not analysingHere() then
    analysis = { mark = mark, pass = nil, looks = {}, last = nil, inRow = 0, counted = 0,
      rejected = 0, offset = 0, said = false, misses = 0 }
  end
  analysis.misses = 0
  sayBusy(analysis)
  return analysis
end

-- A look that found no disc. "Analysis finished" needs TWO such looks in a row in the same stay:
-- one picture can be caught between Melodyne drawing the editor and drawing the disc over it, which
-- has not been seen but has not been ruled out either, and the second look costs half a second.
local function discMissing(mark)
  idleMark = mark
  local a = analysis
  if not (a and ov:stillHere(a.mark)) then
    analysis = nil
    return
  end
  a.misses += 1
  if a.misses < 2 then return end
  analysis = nil
  local same = a.pass and a.looks[a.pass] or 0
  host.log.info(("[melodyne] analysis finished: %s (%d looks read it, %d another name, %d reads not taken)")
    :format(a.pass and ("%q"):format(a.pass) or "no name was taken", same, a.counted - same,
      a.rejected))
  host.speech.output("Analysis finished", { interrupt = false })
end

-- One look: a picture of the disc's place, taken off the event loop at a set time — now — so that
-- it does not hold back Melodyne's own clicks, and decided in its callback.
look = function()
  if not ov.active then
    analysis, idleMark = nil, nil
    return
  end
  -- A menu is drawn over the editor, and the picture would be the menu's.
  if nativeMenuOpen() then return end
  local origin = ov:origin()
  if not (origin and origin.client) then return end
  local cw, ch = O.contentSize(origin)
  local cx, cy = discCentre(origin, cw, ch)
  local mark = ov:here()
  local at = host.now()
  host.screen.snapshotAsync({ region = discRegion(cx, cy), key = DISC_KEY, at = at }, function(snap)
    if not snap then return end
    -- Answered a moment later: still the same Melodyne in the same stay, and no menu over it.
    local top = nil
    if ov:stillHere(mark) and not nativeMenuOpen() then top = discIn(snap, cx, cy) end
    if top then askPass(snap, cx, discX(snap, cx, top + 6), top, discFound(mark)) end
    snap:release() -- the pass name's read keeps its own pixels until it is answered
    if top == false then discMissing(mark) end
  end)
end

host.timer.every(BUSY_MS, function()
  look()
end)

-- Arrival: the mark "Busy" waits behind, and a look at once rather than at the next tick of the
-- clock above, so that the read-out has an answer by the time the arrival sentence asks it, 350
-- ms on — where the focus comes back to it after Alt+A — instead of "not looked at yet".
ov:onActivate(function(o)
  arrivalMark = o:here()
  look()
end)

-- The same answer on demand: a stop at the end of the Tab ring, and Alt+A from anywhere. It says
-- what the looks above found in this stay and reads nothing itself, so it is never later than the
-- last look, half a second at most. A static text, so Space stays Melodyne's play/stop while the
-- focus is on it. Once it has said the pass, "Busy: <pass>" would say it again, and is not said.
ov:addStaticText({
  label = "Analysis",
  hotkey = "Alt+A",
  text = function(o)
    if analysingHere() then
      if analysis.pass then analysis.said = true end
      return analysis.pass or "busy"
    end
    if idleMark and o:stillHere(idleMark) then return "idle" end
    return "not looked at yet"
  end,
})

-- ── Where is the SELECTED note? ───────────────────────────────────────────────
--
-- The question everything else in the note area waits on. Melodyne draws a selected blob
-- differently from an unselected one, but nobody has measured how, and without knowing WHICH
-- blob is the selected one, any geometry we read describes some note rather than the note.
--
-- The measurement is differential, which is why it needs no idea of what "selected" looks like.
-- The user walks notes with the arrow keys; between two selections the only part of the picture
-- that changes is the blob that was selected and the one that no longer is. So: one column
-- profile of the note area per selection change, compared against the previous one, and the
-- columns that moved are logged. Over a walk down a phrase those runs march across the area and
-- draw the map by themselves.
--
-- One capture per selection change, ~17 ms, and the AREA IS FREE — a capture costs the same
-- fixed compositor frame at 890x520 as a single GetPixel does for one point. That is the whole
-- reason `host.screen.profile` exists.
--
-- Calibration only. This is an instrument, not a feature: it says nothing to the user, and a
-- measurement that runs when nobody is measuring is just a slower overlay.
--
-- ON ITS OWN CLOCK, and the first attempt got that wrong in a way worth writing down. It hung
-- off the read-out watcher's "something changed" branch, which sounds like the same event and is
-- not: under the Amplitude tool every note in the phrase read "0.00 dB", so the selection walked
-- the whole way and the read-out never moved — the measurement fired once, printed its baseline,
-- and never spoke again. The selection changing and the read-out changing are different facts,
-- and only one of them is what this measures.
-- Clamped to the window, because the capture is of the SCREEN.
--
-- These four numbers were read off one 962x660 capture, and left absolute they describe that
-- window and no other. On a smaller Melodyne the region runs past the client area and profiles
-- whatever is BEHIND it — another application's pixels, reported as Melodyne's note editor and
-- indistinguishable from it in the log. So the right edge and the bottom are trimmed to the
-- content the overlay actually has, and the log line says which columns it covered.
local NOTE_AREA = { 48, 118, 958, 640 }
-- Faster than the first guess, because the announcement is per FRAME and the user presses per
-- KEY. At 400 ms two quick presses land in one frame and are announced as one move of double the
-- size, which reads as "the values are not continuous" — and they were not. The capture is taken
-- on the capture thread and reduced on the image worker; what the event loop pays per frame is the
-- profile's tables and the comparison, which the log reports every hundred frames, and only under
-- the tools that have nothing else to say.
local NOTE_AREA_MS = 200
local AREA_KEY = "note area"
local lastAreaSize = nil
local CHANGE_THRESHOLD = 6 -- mean, of 255; below this is recogniser and compositor noise
local MIN_THRESHOLD = 8 -- INK: rows of a column, against a floor of about 19 and a blob of 30-57
-- What the read-outs LAST said, as opposed to what is remembered.
--
-- The caches above deliberately never forget: an empty OCR read is not an empty field, so a
-- value survives a recogniser drop-out. That is right for announcing and wrong for asking
-- "does this tool show anything at all" — once any tool had displayed a value the answer was
-- yes for ever, and the note-area measurement, which only runs when the answer is no, went
-- silent for a whole session without a line to say why.
--
-- It belongs to a stay, with the mark of the read that said it: nil at load and once the overlay
-- has left, and nil is "not silent". It started out true and outlived every departure, so the
-- first frames of a stay were measured before any read had answered, and a note already selected
-- on the way back opened the gate. What the boxes hold is the picture's to say (silenceOf, inkIn):
-- an empty read of a box with a value in it is not silence, and one of boxes that are empty, hold
-- a dash or are not drawn is. Fade's box is drawn and empty, and under Time and Note Separation
-- every read so far has been empty.
local readOutsSilent, silentMark = nil, nil
local function silentHere()
  return readOutsSilent == true and silentMark ~= nil and ov:stillHere(silentMark)
end
-- The note's last known position, as the centre of the columns ink arrived in.
local lastArrival = nil
local lastArea, lastAreaMin = nil, nil
-- The bar lines of the previous frame, so a scroll can be told from an edit.
local prevLines = nil
-- Where the overlay was when the frame above was taken: a frame is compared only with one of the
-- same stay. Frames used to survive a closed gate and a trip away, and the first frame of a stay
-- was once compared with one more than two minutes old.
local areaMark = nil
-- The picture the profile above was made from, and its region as text: the next picture is asked
-- to be compared with it (measureNoteArea). nil whenever there is no profile to compare with.
local areaSnap, areaSnapRegion = nil, nil
-- Whether the frame above is one the last frame was compared with instead of itself — half a move
-- drawn (measureMove) — so that the next one is not held as well.
local heldOnce = false

-- The picture above, let go.
local function dropAreaSnap()
  if areaSnap then areaSnap:release() end
  areaSnap, areaSnapRegion = nil, nil
end

-- Everything a frame is compared with, gone: the next frame is a baseline.
local function dropFrames()
  lastArea, lastAreaMin, lastAreaSize, lastArrival, prevLines, areaMark =
    nil, nil, nil, nil, nil, nil
  heldOnce = false
  dropAreaSnap()
end

-- The TIME SIGNATURE, read from the top bar, and it is what turns a measurement into a sentence.
--
-- "One step" says nothing a person can act on, and the user said so. The missing piece was in
-- the window all along, two fields right of the transport: Melodyne prints the key, the time
-- signature and the tempo up there — "Eb Major", "3/4", "~ 88.09". Three four. Which means the
-- third of a bar that this channel has been measuring all evening, over and over, at 0.30 to
-- 0.34, IS ONE BEAT. A quarter note. The measurement was right and the unit was mine.
--
-- Read at most once per input epoch: it changes when the document changes, not while somebody
-- is nudging notes.
--
-- With a callback, as the read-out watcher reads: this is asked from the note-area timer, and a
-- timer that waited for it held Melodyne's own keys until it answered. So the beats are handed
-- to `k` — at once while this input epoch's reading is known, and otherwise when the read
-- answers, as a callback of its own — and nil when no time signature was read. The memo is
-- written when the reading comes, under the epoch the question was asked in: a read that never
-- answers (the module disabled meanwhile) leaves the epoch unread rather than read as nothing,
-- and one that failed is asked again by the next move, since a recogniser that did not answer
-- has said nothing about the time signature.
local TIMESIG_REGION = { 622, 12, 668, 34 }
local TIMESIG_KEY = "time signature"
local sigMemo = { gen = -1, beats = nil }
local function withBeatsPerBar(o, k)
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if sigMemo.gen == gen then return k(sigMemo.beats) end
  local origin = o:origin()
  if not (origin and origin.client) then
    sigMemo.gen, sigMemo.beats = gen, nil
    return k(nil)
  end
  host.ocr.recognize({
    region = {
      origin.client.x + TIMESIG_REGION[1], origin.client.y + TIMESIG_REGION[2],
      origin.client.x + TIMESIG_REGION[3], origin.client.y + TIMESIG_REGION[4],
    },
  }, { key = TIMESIG_KEY }, function(r)
    local text = field(r)
    if text == nil then return k(nil) end
    local n = tonumber(trimmed(text):match("^(%d+)%s*[/l|]%s*%d+$"))
    -- A bar of one to sixteen beats, or we did not read a time signature.
    sigMemo.gen, sigMemo.beats = gen, (n and n >= 1 and n <= 16) and n or nil
    k(sigMemo.beats)
  end)
end

-- The STEP the user is actually making, learned from what he has been making.
--
-- Dividing by a bar looked principled and is not, live: the detector that found eight lines
-- cleanly on a still capture returns widths of 36, 47, 57, 110 and 136 pixels within one
-- session, so the divisor wanders. And the ratio it produces sits at about 0.30 — exactly
-- between a quarter and a third — so two hundredths of noise flip the name. A stable
-- measurement was being read out as "a quarter, a third, two thirds".
--
-- Melodyne moves a note by whole grid units, so the honest unit is the one he keeps using. The
-- median of the recent displacements IS that unit, whatever the grid is set to, and the
-- announcement becomes a count of steps: what he did, not what I inferred about the ruler behind
-- it. His own run bears it out — 33, 34, 35, 44 and then a 90, which is two.
--
-- Kept AS A PART OF THE BAR the move was measured against (barFor, which no longer takes a
-- bar half as wide again as the frame's others), so that the unit follows the zoom at once: a
-- step of 20 px learned at a bar of 80 is 30 px at a bar of 120. Kept in pixels, the steps
-- learned before a zoom were the wrong size for the next half-dozen presses, and a window of half
-- as much again of a beat around them, which the zoom was to wash through, kept a step of the
-- wrong size and refused every grid but the beat: a sixteenth was a fine step, an eighth flipped
-- between that and a beat.
local stepHistory = {}
local function median(t)
  if #t == 0 then return nil end
  local s = table.clone(t)
  table.sort(s)
  return s[math.floor(#s / 2) + 1]
end
local function remember(t, v)
  t[#t + 1] = v
  if #t > 12 then table.remove(t, 1) end
end

-- A move under a sixteenth of a bar teaches no step. A nudge must not teach the step size: the
-- median is what a PRESS OF THE GRID KEY does, and feeding it nudges would drag it down until a
-- real step counted as several. A nudge measured 1 to 4 px against bars of 102 to 136, where a
-- sixteenth is 6 to 8.5, and a nudge of 3 px measured up to 4.5 on the notes of the calibration
-- captures moved in Python at a bar of 80, where it is 5 — learned, at a twentieth, it was one step
-- and the nudges after it steps too. A grid of sixteenths in 4/4 is at the edge of this, and a
-- finer one is a nudge to it.
local STEP_FLOOR = 16

-- The unit a move is counted in, against bar `bar`: the step he has been making, else — before he
-- has made one — a beat, the bar over the time signature's beats. nil when there is neither, or
-- when it is under 4 pixels, where the picture's noise is as large as the unit.
local function unitFor(beats, bar)
  if not bar then return nil end
  local learned = median(stepHistory)
  local unit = learned and learned * bar or (beats and bar / beats) or nil
  if unit and unit < 4 then return nil end
  return unit
end

-- What a move of `px` pixels against bar `bar` is said as, after its direction: "one beat", "2
-- beats", "one step", "3 steps", "one fine step"; nil when there is nothing to measure it by. The
-- same for every move, whichever threshold of ink it was seen at. A move told for certain (`learn`)
-- and not under STEP_FLOOR is a step and is learned first, so the first one is one step of whatever
-- grid he uses; one whose size was not told for certain teaches nothing.
--
-- Whole beats first, when the time signature is read: a move within a quarter of a beat of one or
-- more beats is said in beats, whatever step was learned — two presses that landed in one frame,
-- or a first move of two beats, are "2 beats", not one step of a grid of two.
--
-- A move under half the unit is Melodyne's NUDGE: Ctrl+Alt is the same gesture with the grid
-- switched off, and it moves a few pixels where a grid step moves a beat. Rounding that to the
-- nearest number of steps gives zero, and the code once reported it as one — a fine nudge
-- announced as a whole beat. It gets its own word instead, and no quantity: at a few pixels the
-- measurement carries a quarter of its own size in noise, and a number built on that would be
-- decoration. Half the unit, not a number of pixels: the split was a fixed 12 px, measured at a
-- beat of 34, and at a bar of 80 px — a beat of 20 — it called every grid step a nudge or said
-- nothing at all.
--
-- Other steps are counted in beats when as many of them fit in a bar as the time signature says,
-- and otherwise in steps — never as a fraction of a bar. Fractions were said once, a quarter, then
-- a third, for six presses of one step; and on the note of the session of 2026-10-05, moved 20 px
-- in Python, its halves measure 21 px one way and 23 the other: 0.26 and 0.29 of a bar of 80, where
-- a quarter is 0.25 and a third 0.33. Counted against his own steps, a press is one step every
-- time.
local function moveWords(px, beats, bar, learn)
  if not bar then return nil end
  if learn and px >= bar / STEP_FLOOR then remember(stepHistory, px / bar) end
  if beats and bar / beats >= 4 then
    local n = px / (bar / beats)
    local whole = math.floor(n + 0.5)
    if whole >= 1 and math.abs(n - whole) <= 0.25 then
      return whole == 1 and "one beat" or (whole .. " beats")
    end
  end
  local unit = unitFor(beats, bar)
  if unit == nil then return px < bar / STEP_FLOOR and "one fine step" or nil end
  if px < unit / 2 then return "one fine step" end
  local n = math.max(1, math.floor(px / unit + 0.5))
  local word = (beats and math.abs(bar / unit - beats) <= 0.6) and "beat" or "step"
  return n == 1 and ("one " .. word) or (n .. " " .. word .. "s")
end

-- The bar lines, found in the same profile that finds the note.
--
-- They are the only ruler we have, and they are not optional: a note that moved eleven pixels
-- has moved nothing a person can act on, while "an eighth" is an answer. The spacing is NOT a
-- constant — measured in this window at 99, 110, 108, 113, 112, 117 and 131 pixels, because the
-- tempo varies — so it is measured every time rather than remembered, and the two lines nearest
-- the movement are the ones that scale it.
--
-- A bar line is a column whose mean sits well below its surroundings on BOTH sides. Comparing
-- against neighbours eight columns away rather than against a global background is what makes
-- it survive a scrolled view, a different zoom and a screenful of note blobs: blobs are wide and
-- lines are not, so a blob lowers its neighbours with it and a line does not.
local BAR_LINE_CONTRAST = 14
local function barLines(mean)
  local lines, last = {}, -99
  for i = 9, #mean - 8 do
    local m = mean[i]
    if (mean[i - 8] - m) > BAR_LINE_CONTRAST and (mean[i + 8] - m) > BAR_LINE_CONTRAST then
      -- One entry per line: the columns of a two-pixel line would otherwise both count.
      if i - last > 4 then lines[#lines + 1] = i end
      last = i
    end
  end
  return lines
end

-- The bar width AT a given column, from the two detected lines that bracket it.
--
-- Interpolating locally rather than averaging the lot is the whole point: an average across a
-- variable tempo would describe a bar that occurs nowhere, and the note is always between two
-- particular lines.
local function barWidthAt(lines, col)
  if #lines < 2 then return nil end
  for i = 1, #lines - 1 do
    if col >= lines[i] and col <= lines[i + 1] then
      return lines[i + 1] - lines[i]
    end
  end
  -- Outside the detected run of lines: the nearest gap is the best available scale, and saying
  -- so is better than refusing to speak about a note near the edge of the view.
  if col < lines[1] then return lines[2] - lines[1] end
  return lines[#lines] - lines[#lines - 1]
end

-- The width of a bar across one frame: the median gap between its bar lines; nil with fewer than
-- two.
local function barWidthOf(lines)
  if #lines < 2 then return nil end
  local gaps = {}
  for i = 2, #lines do gaps[#gaps + 1] = lines[i] - lines[i - 1] end
  return median(gaps)
end

-- The bar a move at column `col` is measured against: the one around it, unless that is half as
-- wide again as the frame's other bars, or narrower by as much — a line found where there is none,
-- or one missed: one session gave widths from 36 to 136 — and then theirs.
local function barFor(lines, col)
  local typical = barWidthOf(lines)
  if not typical then return nil end
  local here = barWidthAt(lines, col)
  if here and here >= typical / 1.5 and here <= typical * 1.5 then return here end
  return typical
end

-- By how many columns did the changed part of the picture MOVE?
--
-- The first attempt tracked the strongest run from frame to frame, and the log showed why that
-- cannot work: a Ctrl+Right produces eight or nine changed runs, not two, because the notes in a
-- phrase sit close enough that moving one disturbs the columns of its neighbours. The strongest
-- run then jumps between unrelated notes, and the difference between two of those is a number
-- about nothing — which is exactly what the user heard: a figure when nothing had moved, silence
-- when something had.
--
-- The question the picture can actually answer is not "where did this run go" but "what shift
-- best explains the change". So: over the span that changed, slide the previous minimum profile
-- against the current one and take the offset with the smallest mean difference. Several notes
-- moving together, a note passing its neighbours, a run splitting in two — all of it is one
-- shift, which is what it physically is.
--
-- ONLY the minimum channel. The mean sees the position cursor and barely sees a blob; the
-- cursor moves when the SELECTION moves, which is not an edit, and announcing it as one was the
-- other half of what the user reported.
-- Scored over the CHANGED columns only, and that is not a detail — it is the whole method.
--
-- The first version compared a window padded by sixty columns on each side, so more than nine
-- tenths of what it scored was unchanged surroundings. Those match perfectly at offset zero and
-- badly at every other offset, so zero won every time and the estimator was silent for a whole
-- session. The log said so in the plainest possible way: "shift 0 px (fit 2.1 against 2.1)" —
-- the same number twice, which can only mean the two comparisons were looking at the same
-- unchanging thing.
--
-- Comparing only the columns that CHANGED inverts it. At offset zero those columns are, by
-- definition, different — that is why they are in the list. At the true offset each of them
-- should hold what its neighbour one shift back held a moment ago. So a good fit is now a small
-- number where standing still is a large one, and the two are finally measuring different
-- things.
--
-- Searched to half a bar either way, which is what the 60 px it once was came to at a bar of 110.
local function bestShift(prevMin, minv, cols, maxShift)
  local function score(k)
    local sum, n = 0, 0
    for _, i in ipairs(cols) do
      local j = i - k
      if j >= 1 and j <= #prevMin then
        sum = sum + math.abs((minv[i] or 0) - (prevMin[j] or 0))
        n = n + 1
      end
    end
    if n < 4 then return math.huge end
    return sum / n
  end
  local best, bestScore = 0, math.huge
  for k = -maxShift, maxShift do
    local s = score(k)
    if s < bestScore then best, bestScore = k, s end
  end
  return best, bestScore, score(0)
end

-- Every sentence the note area says, with the evidence it was said on, in the log: runs, ink
-- arrivals, how old the frame was by then, and what the read-outs held. Speech is not logged, and
-- this channel has said things nobody could account for afterwards.
local function noteAreaSay(text, runs, arrived, frame)
  host.log.info(("[melodyne] note area said %q (runs %d, arrived %d, frame age %d ms, read-outs %q|%q)")
    :format(text, runs, arrived, math.floor(host.now() - frame.taken), lastLeft, lastRight))
  host.speech.output(text, { interrupt = true })
end

-- Ink ARRIVING in a column, or leaving it, in rows: three, a third of the eight the runs above are
-- built on.
--
-- How many rows a column changes by says how much ink the note carries at its edge more than how
-- far it moved: once a move passes the few pixels of an edge's taper, 20 px and 34 px change a
-- column alike (shapes from the calibration captures, moved in Python). The notes of August
-- carried 9 to 14 rows there; those of the session of 2026-10-05, at a bar of 80 px, 3 to 7, so
-- their grid steps were seen at three or not at all. And at eight the halves of a move are its
-- darkest columns, not its extent: a note of 24 px moved 34 showed two columns on each side. So a
-- move is seen at three — and sized at half its own peak (halves) — and the threshold decides
-- nothing about what it is called (moveWords).
-- Ctrl+Alt, Melodyne's Nudge — the same gesture with the grid off — moves a few pixels where a grid
-- step moves a beat.
--
-- AND IT HAS A FLOOR THAT NO THRESHOLD CAN LOWER. Asked to report a Ctrl+Alt nudge it answered
-- "nothing (0 arrived, 0 left)" four times running: not one column of the note area changed by
-- as much as three rows of ink between frames. The picture did not carry the edit at all.
--
-- The arithmetic says why. At the zoom in that session a beat was 34 pixels wide, so a nudge of
-- a hundredth of a beat is a third of a pixel and does not exist on screen; even a tenth of a
-- beat is 3.4 pixels, which is about where the one successful fine reading of the evening sat.
-- Lowering the threshold further would not find the movement, it would start reporting
-- anti-aliasing as motion. The remedy is not sensitivity but magnification: at twice the zoom the
-- same nudge is twice the pixels. This channel measures what the picture holds, and the user sets
-- that with the zoom. Nor does an idle picture change a column by three: of the 49 frames the fine
-- pass looked at in the session of 2026-10-05, 44 found nothing at three, four were the moves
-- pressed, and in one three columns gained ink and none lost it.
local ARRIVED = 3

-- FULL-HEIGHT COLUMNS ARE NOT NOTES, and skipping them is what the ink channel was for.
--
-- Melodyne's playback cursor is a line spanning the whole editor, measured at 470 to 516
-- dark rows against 29 to 57 for a note blob. While the transport runs it crosses the area
-- several times a second, arriving in a new column each frame — so the user pressed Play and
-- was read a sentence every 200 ms. I measured that separation on his own capture hours ago
-- and then wrote an arrival test that could not tell the two apart.
--
-- And a cursor that MOVED says the frame holds no edit. Selecting a note takes the cursor to it —
-- walking notes with the arrows moved it in lockstep with the transport read-out — while the
-- cursor does not follow an edit: twelve semitone moves and four time moves left the transport
-- unchanged. The selection's highlight is three to four rows of ink, so a walk from note to note
-- would otherwise read as a move from one to the other.
--
-- But the bar lines are full height too: 470 to 519 dark rows on every calibration capture, at
-- exactly the columns barLines finds. When the view scrolls they move with the music, and counted
-- as the cursor they silenced the edit that made Melodyne scroll. So a full-height column that
-- came or went counts as the cursor only where the view's own move does not explain it: one that
-- was there the view's move away, or beside it, or past the edge it scrolled in from.
local FULL_HEIGHT = 150

-- The two halves of whatever moved between two frames: the columns that gained ARRIVED rows of ink
-- or more, and the ones that lost as many — how many of each, the first and the last, and the
-- arrivals' own list — `size`, the same halves at half their own peak change, which a move is
-- measured from (stepOf), and how many full-height columns came or went that the view's move of
-- `shift` px (nil or 0 when it did not move) does not explain: the cursor.
--
-- Over the WHOLE profile, not over the runs. The runs are a summary written for the log —
-- grouped, gap-tolerant, one to three columns wide — and building a centroid inside them left
-- samples of two and five columns. Every column that changed enough counts here; it costs three
-- passes over 910 numbers.
--
-- At half the peak, because a note's ink is not even along it. At three rows a half takes in the
-- taper of the note's ends and, along a note drawn as a waveform, every column whose thickness the
-- move changed: the note of calibration/Melodyne-5-clean.png, moved 20 px either way in Python,
-- had halves 34.5 and 33 columns wide on average at three rows, which said "two beats", and 23 and
-- 21 at half their peaks, which said one; the notes of Melodyne-3-clean.png, 170 moves, said the
-- same at both.
local function halves(minv, prevMin, shift)
  local n = #minv
  local h = { cols = {}, cursor = 0, aN = 0, aMin = math.huge, aMax = -math.huge, aPeak = 0,
    dN = 0, dMin = math.huge, dMax = -math.huge, dPeak = 0 }
  local change, fullNow, fullWas = {}, {}, {}
  for i = 1, n do
    local now, was = minv[i] or 0, prevMin[i] or 0
    fullNow[i], fullWas[i] = now >= FULL_HEIGHT, was >= FULL_HEIGHT
    local c = (now < FULL_HEIGHT and was < FULL_HEIGHT) and now - was or 0
    change[i] = c
    if c >= ARRIVED then
      h.aN += 1
      h.aMin, h.aMax, h.aPeak = math.min(h.aMin, i), math.max(h.aMax, i), math.max(h.aPeak, c)
      h.cols[#h.cols + 1] = i
    elseif c <= -ARRIVED then
      h.dN += 1
      h.dMin, h.dMax, h.dPeak = math.min(h.dMin, i), math.max(h.dMax, i), math.max(h.dPeak, -c)
    end
  end
  local k = shift or 0
  local function explained(full, j)
    return k ~= 0 and (j < 1 or j > n or full[j] or full[j - 1] or full[j + 1]) == true
  end
  for i = 1, n do
    if fullNow[i] ~= fullWas[i] then
      local ok
      if fullWas[i] then ok = explained(fullNow, i + k) else ok = explained(fullWas, i - k) end
      if not ok then h.cursor += 1 end
    end
  end
  local s = { aN = 0, aSum = 0, aMin = math.huge, aMax = -math.huge,
    dN = 0, dSum = 0, dMin = math.huge, dMax = -math.huge }
  local ta, td = math.max(ARRIVED, h.aPeak / 2), math.max(ARRIVED, h.dPeak / 2)
  for i = 1, n do
    local c = change[i]
    if h.aN > 0 and c >= ta then
      s.aN += 1
      s.aSum += i
      s.aMin, s.aMax = math.min(s.aMin, i), math.max(s.aMax, i)
    elseif h.dN > 0 and -c >= td then
      s.dN += 1
      s.dSum += i
      s.dMin, s.dMax = math.min(s.dMin, i), math.max(s.dMax, i)
    end
  end
  h.size = s
  return h
end

-- How much redder than blue, on average, a row of the columns ink arrived in must have become to
-- be one of the note's rows. Melodyne draws a note orange and yellow, edged in red, on a page of
-- greys — where red and blue are equal — so red over blue is the note, the light middle of a blob
-- included, which the dark count misses.
local NOTE_COLOUR = 8

-- Whether the columns between the two halves of a move hold the note's body, on the note's own
-- rows: true, false, or nil when the pictures cannot say — no rows turned the note's colour, or
-- the halves have no columns between them. `now` is the frame's picture and `before` the one the
-- last profile was made from; three small profiles of them, the arrival columns' rows in each and
-- the columns between on the note's rows.
--
-- On the note's rows, because the whole column is no test: other notes at other pitches stand in
-- the same columns, and a blob's light middle has fewer dark pixels than the page's grid lines.
local function bodyBetween(h, dir, frame, now, before)
  if not (now and before) or now.released or before.released then return nil end
  local lo, hi
  if dir > 0 then lo, hi = h.dMax + 1, h.aMin - 1 else lo, hi = h.aMax + 1, h.dMin - 1 end
  if hi < lo then return nil end
  local x0 = frame.origin.client.x + NOTE_AREA[1] - 1
  local top, bottom = frame.origin.client.y + NOTE_AREA[2], frame.origin.client.y + frame.y2
  local block = { x0 + h.aMin, top, x0 + h.aMax + 1, bottom }
  local pn = host.screen.profile({ snapshot = now, region = block, axes = "rows" })
  local pb = host.screen.profile({ snapshot = before, region = block, axes = "rows" })
  if not (pn and pb and pn.rows and pb.rows and pn.rows.r and pb.rows.r) then return nil end
  if #pn.rows.r ~= #pb.rows.r then return nil end
  local first, last = nil, nil
  for i = 1, #pn.rows.r do
    if (pn.rows.r[i] - pn.rows.b[i]) - (pb.rows.r[i] - pb.rows.b[i]) >= NOTE_COLOUR then
      first, last = first or i, i
    end
  end
  if first == nil then return nil end
  local arrived, was = 0, 0
  for i = first, last do
    arrived += pn.rows.r[i] - pn.rows.b[i]
    was += pb.rows.r[i] - pb.rows.b[i]
  end
  arrived, was = arrived / (last - first + 1), was / (last - first + 1)
  local g = host.screen.profile({
    snapshot = now, region = { x0 + lo, top + first - 1, x0 + hi + 1, top + last }, axes = "rows",
  })
  if not (g and g.rows and g.rows.r and #g.rows.r > 0) then return nil end
  local gap = 0
  for i = 1, #g.rows.r do gap += g.rows.r[i] - g.rows.b[i] end
  return gap / #g.rows.r - was >= (arrived - was) / 2
end

-- Which way the ink flowed between two frames, in pixels: one-dimensional optical flow. Where a
-- profile shifts by a little, what changed in a column is minus the shift times the slope there,
-- so the shift that explains the change best is minus the sum of change times slope over the sum
-- of the slopes squared — over the columns that changed and their neighbours, the cursor's left
-- out. A witness of the direction for a change that is no rigid move (stepOf).
local function flowOf(minv, prevMin)
  local num, den = 0, 0
  for i = 2, #minv - 1 do
    local near = false
    for j = i - 1, i + 1 do
      local now, was = minv[j] or 0, prevMin[j] or 0
      if now >= FULL_HEIGHT or was >= FULL_HEIGHT then
        near = false
        break
      end
      if math.abs(now - was) >= ARRIVED then near = true end
    end
    if near then
      local slope = ((prevMin[i + 1] - prevMin[i - 1]) + (minv[i + 1] - minv[i - 1])) / 4
      num -= (minv[i] - prevMin[i]) * slope
      den += slope * slope
    end
  end
  return den > 0 and num / den or 0
end

-- WHICH WAY, and HOW FAR, from the two halves of one move: the direction (1 right, -1 left, 0 when
-- the pictures cannot say), the distance in pixels — or, when they cannot tell which it is, nil
-- and the two distances it may be — and how it was told, for the log.
--
-- Which way is the centroids': where the darkness appeared against where it left. Whatever the
-- magnitude, if a thing moved right then the columns that got darker lie to the right of the
-- columns that got lighter — eleven signs of eleven in the test that earned it, where a correlation
-- had aliased onto the neighbouring notes.
--
-- But only of a rigid move, whose halves lie apart: what arrived on one side, what left on the
-- other. Where they overlap, more changed than one note moving. A note moved in a legato phrase
-- stretches the note before it and squeezes the one after, and on the phrase of
-- calibration/Melodyne-5-clean.png modelled so, the centroids pointed the wrong way at every step
-- from 2 to 34 px while the ink's flow (flowOf) pointed the right way, by 1.6 to 3.4 px. So where
-- the halves overlap the flow says the direction, when it comes to a pixel or more, and otherwise
-- the sentence says only that something moved — as for a note 119 px long nudged by 3 and 4 px,
-- where it stayed under one. No distance there: what the halves add up to is the neighbours' as
-- much as the note's.
--
-- How far is not the distance between them, and that was the fine pass's mistake. For a note at
-- least as wide as its move, each half is the move's width and the note's body lies between them,
-- so the centroids are the NOTE'S WIDTH apart; for a narrower note, each half is the note and the
-- page lies between them, so the centroids are the move apart and the halves the note's width.
-- Both cases give the same two numbers, the other way round. At a bar of 80 px the fine pass said
-- 10 and 21 px for what may have been steps of 20 — or the note's width. So the picture between
-- the halves decides (bodyBetween), and where it cannot, the sentence says only what both numbers
-- agree on (sayMove). Both numbers come from the halves at half their peak (halves); whether they
-- overlap, from the halves at three rows.
--
-- Tried on three notes of calibration/Melodyne-3-clean.png and two beside others, moved in Python
-- by steps of a beat and nudges of 2 to 4 px, both ways, at beats of 20 and 36 px: 158 moves said
-- right, 6 by their direction alone and 6 as "moved" — nudges of a note 119 px long, whose halves
-- are scattered along it — and none wrong. Following the last arrival instead says a note's width
-- for a move back the other way, and about half a step for a nudge after a step. On the note of
-- the session of 2026-10-05, of which there is no picture after its analysis — its waveform, drawn
-- during the analysis, moved in Python — 20 px either way is one beat and 34 px two.
local function stepOf(h, frame, now, before, minv, prevMin)
  if not (h.aMax < h.dMin or h.dMax < h.aMin) then
    local flow = flowOf(minv, prevMin)
    local how = ("halves that overlap, the ink's flow %.1f px"):format(flow)
    if math.abs(flow) >= 1 then return flow > 0 and 1 or -1, nil, nil, how end
    return 0, nil, nil, how
  end
  local s = h.size
  local ac, dc = s.aSum / s.aN, s.dSum / s.dN
  local dir = ac > dc and 1 or -1
  local apart, wide = math.abs(ac - dc), (s.aN + s.dN) / 2
  if math.abs(apart - wide) < 1.5 then return dir, apart, nil, "the halves touch" end
  local body = bodyBetween(s, dir, frame, now, before)
  if body == true then return dir, wide, nil, "the note's body between the halves" end
  if body == false then return dir, apart, nil, "the page between the halves" end
  return dir, nil, { wide, apart }, "nothing between the halves to tell by"
end

-- SPOKEN, when nothing else in this window will say anything.
--
-- Under Time, Time Handle, Note Separation and Fade there is no read-out to change and no
-- number anywhere in the application: the position belongs to the cursor and the cursor does
-- not follow an edit (measured — twelve semitone moves and four time moves, transport
-- unchanged throughout). A blind user pressing Ctrl+Right there gets silence, which is
-- indistinguishable from a key that did nothing.
--
-- The condition is measured rather than assumed: we speak only when the read-outs themselves
-- are silent (silentHere), so the pitch, formant and amplitude tools — which do have a number
-- and do announce it — are never talked over.
--
-- Said once the time signature is known: at once when this input epoch has read it, and
-- otherwise when its read answers. Then the sentence is about a moment that has passed, so it is
-- said only about the Melodyne it was measured in — the overlay still in front, on the same
-- window, in the same stay — with no menu drawn over it, and while the read-outs are still silent:
-- a tool switched to one that has a number meanwhile is announced by the read-out watcher, and
-- this would talk over it. Nor once an analysis has begun since.
--
-- `px` is the move, or nil with `readings`, the two it may be: then the words both readings come
-- to, and the direction alone when they differ, which is still true where a guess is not. Nothing
-- to measure the move by says the direction alone as well, and a direction the pictures cannot
-- tell (`dir` 0) says "moved".
local function sayMove(o, dir, px, readings, bar, runs, arrived, frame)
  local side = dir > 0 and "right" or dir < 0 and "left" or "moved"
  withBeatsPerBar(o, function(beats)
    if not o:stillHere(frame.mark) then return end
    if nativeMenuOpen() then return end
    if not silentHere() or analysingHere() then return end
    local words
    if px then
      words = moveWords(px, beats, bar, true)
    elseif readings and dir ~= 0 then
      local one, two = moveWords(readings[1], beats, bar, false), moveWords(readings[2], beats, bar, false)
      if one == two then words = one end
    end
    if O.calibrating() then
      local unit = unitFor(beats, bar)
      host.log.info(("[melodyne] move of %s px, against a unit of %s px, bar %s px, time signature %s")
        :format(px and ("%.1f"):format(px)
          or (readings and ("%.1f or %.1f"):format(readings[1], readings[2]) or "?"),
          unit and ("%.1f"):format(unit) or "?", tostring(bar or "?"), tostring(beats or "?")))
    end
    noteAreaSay(words and (side .. ", " .. words) or side, runs, arrived, frame)
  end)
end

-- One frame's move, measured and said. `lines` are the frame's bar lines, `viewShift` how far they
-- moved since the last frame (nil when not known), `now` and `before` the two pictures compared.
--
-- Answers true when the frame is to be HELD: half a move, ink that arrived with none that left or
-- the other way round. Twenty frames of forty-nine once showed one half alone at eight rows — "0
-- arrived, 1 left" or the reverse — and a frame can land mid-redraw, the note gone from its old
-- place and not drawn yet in the new one. So the next frame is compared with the one before this
-- instead, and has both halves. Once: a change that is still half at the next frame is let go.
local function measureMove(o, frame, minv, prevMin, lines, viewShift, runs, now, before)
  local h = halves(minv, prevMin, viewShift)
  if h.cursor > 0 then
    if O.calibrating() then
      host.log.info(("[melodyne] move: nothing, the cursor moved (%d columns): the selection or the"
        .. " playhead, not an edit"):format(h.cursor))
    end
    return false
  end
  if h.aN == 0 or h.dN == 0 then
    local hold = (h.aN > 0 or h.dN > 0) and not heldOnce
    if O.calibrating() then
      host.log.info(("[melodyne] move: nothing (%d arrived, %d left)%s")
        :format(h.aN, h.dN, hold and "; the next frame is compared with the one before" or ""))
    end
    return hold
  end
  local s = h.size
  local ac = s.aSum / s.aN
  local dir, px, readings, how = nil, nil, nil, nil
  if viewShift and viewShift ~= 0 then
    -- The view moved under the note: the halves describe the scroll as much as the edit. What the
    -- note did is how far its arrival moved, less the view's own move.
    if lastArrival then
      local real = ac - lastArrival - viewShift
      if real ~= 0 then dir, px = real > 0 and 1 or -1, math.abs(real) end
    end
    how = ("the last arrival, less the view's move of %d px"):format(viewShift)
  else
    dir, px, readings, how = stepOf(h, frame, now, before, minv, prevMin)
  end
  local bar = barFor(lines, math.floor(ac))
  lastArrival = ac
  if O.calibrating() then
    -- The correlation, as a SECOND OPINION in the log and nothing more. It is the estimate that
    -- aliased onto the neighbouring notes, and a shift it agrees on is a small reassurance while a
    -- disagreement is worth seeing.
    local corr = "?"
    if bar then
      local k, fit, still = bestShift(prevMin, minv, h.cols, math.floor(bar / 2))
      if fit < math.huge then corr = ("%d px (fit %.1f against %.1f standing still)"):format(k, fit, still) end
    end
    local x = NOTE_AREA[1] - 1
    host.log.info((
      "[melodyne] move: %s %s px, by %s; %d arrived at x%d-%d, %d left at x%d-%d; at half their peak"
      .. " %d at x%d-%d and %d at x%d-%d; bar %s px; correlation %s"
    ):format(dir == nil and "nothing" or (dir > 0 and "right" or dir < 0 and "left" or "moved"),
      px and ("%.1f"):format(px) or (readings and ("%.1f or %.1f"):format(readings[1], readings[2]) or "?"),
      how or "?", h.aN, h.aMin + x, h.aMax + x, h.dN, h.dMin + x, h.dMax + x,
      s.aN, s.aMin + x, s.aMax + x, s.dN, s.dMin + x, s.dMax + x, tostring(bar or "?"), corr))
  end
  -- Under a pixel the picture holds no move — at a beat of 34 px a hundredth of a beat is a third of
  -- a pixel — and what is left at that size is anti-aliasing. Anything the picture does hold is
  -- said, if only as its direction.
  if not dir or (px and px < 1) then return false end
  sayMove(o, dir, px, readings, bar, runs, h.aN, frame)
  return false
end

-- The transport read-out beside a frame's runs, while calibrating: one read out at a time.
local TRANSPORT_KEY = "transport"

-- One frame of the note area, profiled: compared with the last one of the same stay, and what
-- moved is logged and, while the read-outs are silent, said. `frame` is what the tick knew when it
-- asked for the picture — its mark, the window's size, the region's clamped corners, the origin —
-- and when the picture was taken; `now` is the picture, and `before` the one the last profile was
-- made from, when it is of the same place on the screen (nil otherwise).
local function measureFrame(o, p, frame, now, before)
  if not (p and p.columns and p.columns.mean and p.columns.dark) then return end
  -- A window that changed size re-baselines instead of being diffed: the whole picture moved,
  -- and every column would be a run. The column COUNT cannot catch this by itself — it is
  -- clamped to the same numbers for every window at least this wide — so the size is kept. And a
  -- frame of another stay is no baseline either: what Melodyne showed before an excursion is not
  -- a fact about the one we came back to, and this instrument's whole premise is that only ONE
  -- thing changed between two frames.
  if lastAreaSize ~= frame.size or not (areaMark and o:stillHere(areaMark)) then
    dropFrames()
    lastAreaSize = frame.size
  end
  local origin, x2, y2 = frame.origin, frame.x2, frame.y2
  -- The INK COUNT is the working channel; the mean is kept only for the bar lines, which are
  -- full-height and show up in it plainly.
  --
  -- Measured on a real capture at threshold 190: background and grid lines about 19 rows per
  -- column, note blobs 29 to 57, a full-height cursor 470 to 516. Three classes, cleanly apart.
  -- The mean gave a blob four units out of 255 and the minimum gave it nothing at all, because
  -- in a busy picture nearly every column already holds something black and cannot go blacker.
  local mean, minv = p.columns.mean, p.columns.dark
  local prev, prevMin = lastArea, lastAreaMin
  lastArea, lastAreaMin, areaMark = mean, minv, frame.mark
  local lines = barLines(mean)
  if not (prev and prevMin) or #prev ~= #mean then
    prevLines, heldOnce = lines, false
    -- The region is in the line because it is a GUESS clamped to this window, and a wrong one
    -- is otherwise invisible: a profile of the wrong pixels looks exactly like a profile of the
    -- right ones.
    host.log.info(("[melodyne] note area: baseline, %d columns, client x %d-%d, y %d-%d, window %s")
      :format(#mean, NOTE_AREA[1], x2, NOTE_AREA[2], y2, tostring(lastAreaSize)))
    return
  end
  -- Contiguous runs of changed columns, so the log names REGIONS rather than a list of
  -- hundreds of indices. A gap of one column does not end a run: a blob's outline can leave a
  -- column untouched in the middle of a shape that plainly moved.
  -- TWO channels, because they see different things and the first evening proved it.
  --
  -- The column MEAN sees a full-height feature enormously — the position cursor moved it by 130
  -- to 180 of 255 — and a note blob hardly at all: fifteen dark rows out of five hundred and
  -- twenty-two shift a mean by about four, which is under any threshold worth having. That is
  -- exactly what a Ctrl+Right under the Time tool produced: deltas of six to nine, scattered,
  -- indistinguishable from noise, while the cursor and the transport both stood still.
  --
  -- The column MINIMUM is the opposite instrument. It ignores how TALL something is and reports
  -- only how dark the darkest pixel got, so a blob arriving in a column swings it hard while a
  -- one-pixel line among many dark pixels barely registers. Its own threshold is higher because
  -- a minimum is a single pixel and therefore noisy where a mean is not.
  --
  -- A column counts as changed when EITHER fires, and both peaks are logged, so the log says
  -- which channel saw it — which is the difference between "the note moved" and "the cursor
  -- moved" without anybody having to look.
  local runs, from, peak, peakMin, gap = {}, nil, 0, 0, 0
  for i = 1, #mean do
    local d = math.abs(mean[i] - prev[i])
    local dm = math.abs((minv[i] or 0) - (prevMin[i] or 0))
    if d >= CHANGE_THRESHOLD or dm >= MIN_THRESHOLD then
      if not from then from, peak, peakMin = i, 0, 0 end
      if d > peak then peak = d end
      if dm > peakMin then peakMin = dm end
      gap = 0
    elseif from then
      gap = gap + 1
      if gap > 1 then
        runs[#runs + 1] = { from = from, to = i - gap, peak = peak, peakMin = peakMin }
        from = nil
      end
    end
  end
  -- The run ends where its last CHANGED column was, not at the end of the array:  counts
  -- the unchanged columns already walked past, and charging them to the run reports it wider
  -- than it is — by one column in the ordinary case, which is the whole width of the feature
  -- these runs turned out to be.
  if from then runs[#runs + 1] = { from = from, to = #mean - gap, peak = peak, peakMin = peakMin } end
  -- A MASS change is the view moving, not a note moving, and it is the one case where the
  -- differential premise fails outright. It showed up the first evening: walking past the right
  -- edge made Melodyne scroll, and two frames reported 48 and 53 runs while the position jumped
  -- from bar 38 to bar 40. Everything after such a frame is measured against a picture of a
  -- different stretch of music, so the baseline is thrown away rather than believed.
  if #runs > 12 then
    host.log.info(("[melodyne] note area: %d run(s) — the view moved, re-baselining"):format(#runs))
    -- The positions go with the picture: the next arrival measured against a note's place in the
    -- old view would be said as a move of the view's size.
    lastArea, lastAreaMin, lastArrival, prevLines, heldOnce = nil, nil, nil, nil, false
    return
  end
  -- HOW FAR DID THE VIEW ITSELF MOVE? Everything above is window-relative.
  --
  -- Melodyne scrolls to follow a note being dragged out of view. When it does, the note and
  -- the whole picture move together, a window-relative measurement sees an enormous
  -- displacement, and it grows with every press — which is exactly what the user heard: 39,
  -- 39, 47, 59, 54, with the changed columns spread across two hundred pixels at once.
  --
  -- The bar lines are the anchor, and they are already found reliably (verified against a real
  -- capture: eight lines, no false positives, on a picture full of blobs). They belong to the
  -- MUSIC, so if they moved, the view moved by that much, and the note's real displacement is
  -- what is left after subtracting it. A scroll with no edit then measures zero, which is the
  -- truth, instead of the largest number of the session. Each line is matched with the nearest
  -- of the last frame's within half a bar, and the median of those offsets is the view's move:
  -- a line entering or leaving the view at the edge must not drag the estimate with it.
  local viewShift = nil
  if prevLines and #prevLines >= 2 and #lines >= 2 then
    local reach = (barWidthOf(lines) or 0) / 2
    local offs = {}
    for _, a in ipairs(lines) do
      local best, bestD = nil, math.huge
      for _, b in ipairs(prevLines) do
        local d = math.abs(a - b)
        if d < bestD then best, bestD = b, d end
      end
      if best and bestD <= reach then offs[#offs + 1] = a - best end
    end
    if #offs >= 2 then
      table.sort(offs)
      viewShift = offs[math.floor(#offs / 2) + 1]
    end
  end
  local held = false
  if silentHere() then
    held = measureMove(o, frame, minv, prevMin, lines, viewShift, #runs, now, before)
  end
  -- A held frame leaves the comparison with the frame before it, bar lines and all.
  if held then lastArea, lastAreaMin = prev, prevMin else prevLines = lines end
  heldOnce = held
  -- The runs are logged only while calibrating: during ordinary use this fires whenever a note
  -- moves, and a log that writes on every edit is a log nobody reads.
  if #runs == 0 or not O.calibrating() then return end
  -- Ranked by whichever channel saw the most, so a blob move is not buried under a cursor move.
  --
  -- WHAT the moving feature is, settled by correlation rather than by argument.
  --
  -- It is two pixels wide, very nearly full height, and 130 to 180 units of column mean darker
  -- than its surroundings — far darker than the bar lines, which the same profile shows at
  -- about 55. A line like that which jumps to the selected note is most likely Melodyne's
  -- playback position cursor, and "most likely" is not something to build a read-out on.
  --
  -- The transport read-out in the top bar prints that position as bar.beat.sixteenth. If it
  -- changes in lockstep with these runs, the line IS the cursor and its x is the cursor's
  -- position; if it sits still while the runs march, the line belongs to the selection instead.
  -- Either answer is worth having and neither requires anybody to look at a screen. One read per
  -- moved frame, only while calibrating — with a callback, since this is a poll's answer and a
  -- read that waited would hold Melodyne's keys behind it — and the runs logged with its answer.
  table.sort(runs, function(a, b)
    return math.max(a.peak, a.peakMin) > math.max(b.peak, b.peakMin)
  end)
  local parts = {}
  for i = 1, math.min(#runs, 6) do
    local r = runs[i]
    -- Client coordinates, not indices into the capture, so the numbers can be compared with
    -- every other coordinate in this file.
    -- BOTH peaks, because which channel saw it is the finding. A run with mean 0 and a large
    -- minimum is a note blob; a run with a large mean is something nearly full height, which so
    -- far has only ever been the position cursor.
    parts[#parts + 1] = string.format("x%d-%d(mean%d min%d)",
      NOTE_AREA[1] + r.from - 1, NOTE_AREA[1] + r.to - 1, math.floor(r.peak + 0.5), r.peakMin)
  end
  local line = ("[melodyne] note area: %d run(s) moved, strongest %s"):format(#runs, table.concat(parts, " "))
  if not origin.client or host.ocr.pending(TRANSPORT_KEY) then
    host.log.info(line .. "  transport not read")
    return
  end
  host.ocr.recognize({
    origin.client.x + TRANSPORT_REGION[1], origin.client.y + TRANSPORT_REGION[2],
    origin.client.x + TRANSPORT_REGION[3], origin.client.y + TRANSPORT_REGION[4],
  }, { key = TRANSPORT_KEY }, function(r)
    host.log.info(("%s  transport=%q"):format(line, trimmed(field(r) or "?")))
  end)
end

-- Whether the note area is measured now, and why not. It runs for TWO reasons, and neither of
-- them is "always". While calibrating it is an instrument and writes to the log. The rest of the
-- time it runs only when the read-outs are silent — which is measured, not assumed, and is exactly
-- the tools that have no number anywhere in the application. Under Pitch or Amplitude it does not
-- run at all, so the overlay does not pay a picture every 200 ms for a window that is already
-- answering. And never while Melodyne analyses: the disc and the waveform under it change on every
-- frame, and none of that is a note being moved.
local function noteAreaGate(o)
  if not o.active then return false, "the overlay is not in front" end
  if analysingHere() then return false, "Melodyne is analysing" end
  if O.calibrating() or silentHere() then return true end
  return false, "the read-outs are not silent"
end

-- The gate's last state, so the log has a line when it changes and not on every tick.
local gateOpen = false
local function logGate(open, why)
  if open == gateOpen then return end
  gateOpen = open
  host.log.info(("[melodyne] note area: gate %s, read-outs %q|%q"):format(
    open and (silentHere() and "opened" or "opened for calibration") or ("closed (" .. why .. ")"),
    lastLeft, lastRight))
end

-- Frames measured, how many of them were the picture the last profile was made from, the event
-- loop's time in their callbacks, and from asking to the answer, for a line every hundred frames.
local areaFrames, areaSame, areaLoop, areaWait = 0, 0, 0, 0

-- One frame: `loop` ms of the event loop in its callbacks, answered at `answered` — the picture's
-- callback for one like the last or one with the disc, the profile's for the others.
local function countFrame(loop, frame, same, answered)
  areaFrames += 1
  if same then areaSame += 1 end
  areaLoop += loop
  areaWait += answered - frame.asked
  if areaFrames < 100 then return end
  host.log.info(("[melodyne] note area: %d frames, %d of them the picture before, %d ms on the event"
    .. " loop for them (%.1f each), %.1f ms from asking to the answer on average")
    :format(areaFrames, areaSame, math.floor(areaLoop), areaLoop / areaFrames, areaWait / areaFrames))
  areaFrames, areaSame, areaLoop, areaWait = 0, 0, 0, 0
end

-- One tick of the note area: the picture is taken off the event loop, at a set time so that it
-- does not hold back Melodyne's own clicks, and looked at in the callback, after everything that
-- decided to take it has been asked again — it is about the moment it was asked in. Its profile is
-- reduced on the image worker, and the frame is measured when that answers, everything asked once
-- more: it was 32 of the 46 ms a frame cost the event loop in the debug build of 2026-10-05, the
-- reduction of 910 x 522 pixels, and only its tables, another 4 there, are the loop's now. The
-- picture stays the module's until then: the disc is looked for in it, and a move's body
-- (bodyBetween) is read from it and from the one before.
--
-- And COMPARED, on the capture thread, with the picture the last profile was made from: a picture
-- in which not one pixel changed is let go unprofiled, since its profile would be the last one's
-- and the comparison would find nothing — no reduction on the worker, and no tables or comparison
-- on the event loop. While nobody edits, nothing should change; the hundred-frame line counts how
-- often it did not. Exact, so it decides nothing the profile would not have: no tolerance, and one
-- pixel is a change. The wait's timeout of 1 ms is not a time anything is waited for: the picture
-- taken at `at` is the answer, different or not, and whether it is the same is its bytes against
-- the last picture's. What it costs instead is on the capture thread — the two pictures' bytes
-- compared when they are the same, measured at about half a millisecond in a release build and 24
-- to 28 in a debug one — and the last profiled picture, held while the gate is open.
local function measureNoteArea(o)
  local open, why = noteAreaGate(o)
  logGate(open, why)
  if not open then
    -- Forget whenever the gate closes, on the way out as for a tool with a number or an analysis:
    -- the next frame it opens on is a baseline, never a comparison across the gap.
    dropFrames()
    return
  end
  -- No frame while the time signature is being read for a move: that move is said when the read
  -- answers, and the next frame is compared with the one it was measured on — as when this tick
  -- waited for the read. One read out at a time, across a disable too, without a flag.
  if host.ocr.pending(TIMESIG_KEY) then return end
  -- One frame out at a time, from its picture to its profile's answer: the next one is compared
  -- with this one, so it waits. Across a disable too, which drops both, without a flag.
  if host.screen.pending(AREA_KEY) then return end
  if nativeMenuOpen() then return end
  local origin = o:origin()
  if not (origin and origin.client) then return end
  local cw, ch = O.contentSize(origin)
  local x2, y2 = math.min(NOTE_AREA[3], cw - 2), math.min(NOTE_AREA[4], ch - 2)
  if x2 - NOTE_AREA[1] < 40 or y2 - NOTE_AREA[2] < 40 then return end
  local frame = { mark = o:here(), size = cw .. "x" .. ch, origin = origin, x2 = x2, y2 = y2,
    asked = host.now() }
  local cx, cy = discCentre(origin, cw, ch)
  local area = {
    origin.client.x + NOTE_AREA[1], origin.client.y + NOTE_AREA[2],
    origin.client.x + x2, origin.client.y + y2,
  }
  -- The picture holds the disc's place as well as the note area. The note area stops 958 px in
  -- and 640 down, and in a window much larger than the 962 x 660 one measured — wider than about
  -- 1790 px, or taller than about 1150 — part of the disc lies outside it: the test below would
  -- find its points missing from a picture of the note area alone and let the frame through.
  -- Profiled is the note area alone.
  local disc = discRegion(cx, cy)
  local region = {
    math.min(area[1], disc[1]), math.min(area[2], disc[2]),
    math.max(area[3], disc[3]), math.max(area[4], disc[4]),
  }
  local regionText = table.concat(region, ",")
  local from = (areaSnap ~= nil and areaSnapRegion == regionText) and areaSnap or nil
  -- Whether the frame is still about the Melodyne it was asked for, asked again in each callback.
  local function fresh()
    return o:stillHere(frame.mark) and not nativeMenuOpen() and not host.ocr.pending(TIMESIG_KEY)
      and (noteAreaGate(o))
  end
  host.screen.snapshotAsync({
    region = region, key = AREA_KEY, at = frame.asked,
    change = from and { from = from, timeout = 1, tolerance = 0, minPixels = 1 } or nil,
  }, function(snap, _, info)
    if not snap then return end
    local began = host.now()
    if not fresh() then
      snap:release()
      if not noteAreaGate(o) then dropFrames() end
      return
    end
    -- The same picture as the one the last profile was made from, and that profile is still the
    -- one frames are compared with, of this stay and this window size. `rebased` is a comparison
    -- that was not made — the two pictures taken two different ways — and is no answer. The frame
    -- would have been compared with that profile and found nothing, which lets go of a held one.
    if from ~= nil and from == areaSnap and info ~= nil and info.changed == false
      and not info.rebased and lastAreaSize == frame.size and areaMark ~= nil
      and o:stillHere(areaMark) then
      snap:release()
      heldOnce = false
      countFrame(host.now() - began, frame, true, began)
      return
    end
    frame.taken = snap.time
    -- The analysis disc in this very frame — the looks above may not have seen it yet — and the
    -- frame is not compared at all. It is counted all the same: the look was the event loop's.
    if discIn(snap, cx, cy) then
      snap:release()
      dropFrames()
      countFrame(host.now() - began, frame, false, began)
      return
    end
    local loop = host.now() - began
    -- Then the profile of the note area, reduced on the image worker: columns only, since a note's
    -- identity is a horizontal position, and dark below 190, where Melodyne's light page separates
    -- the measured classes. Under the frame's key, so the next tick waits for it.
    host.screen.profile({ snapshot = snap, region = area, axes = "columns", dark = 190, key = AREA_KEY },
      function(p)
        local answered = host.now()
        if not fresh() then
          snap:release()
          if not noteAreaGate(o) then dropFrames() end
          return
        end
        -- The picture before only when it is of the same place on the screen: after the window
        -- moved the profiles still compare, the client area being the same, and the pictures do not.
        measureFrame(o, p, frame, snap, areaSnapRegion == regionText and areaSnap or nil)
        -- Kept while its profile is the one the next frame is compared with; let go otherwise, and
        -- with it the one before when there is nothing to compare with any more.
        if p ~= nil and p.columns ~= nil and lastAreaMin == p.columns.dark then
          dropAreaSnap()
          areaSnap, areaSnapRegion = snap, regionText
        else
          snap:release()
          if lastAreaMin == nil then dropAreaSnap() end
        end
        countFrame(loop + host.now() - answered, frame, false, answered)
      end)
  end)
end

reportFieldsAfterSwitch = function(toolName, afterMs)
  if not host.timer then return end
  -- The delay is a parameter because a VARIANT is not reached in one step: `selectVariant`
  -- clicks the icon and then posts the function key once per position, spaced out in time. A
  -- measurement taken at the tab-switch delay would photograph the strip halfway through that
  -- sequence and record the tool we were passing through.
  host.timer.after(afterMs or (READOUT_HOLD_MS - 150), function()
    if not ov.active then return end
    if host.keys.nativeMenuOpen and host.keys.nativeMenuOpen() then return end
    local _, note, pitch = readSelection(ov)
    if note == nil then return end
    -- The greys along the strip in one `pixels` call: one capture, once per tool switch, in a
    -- callback already waiting for a repaint — a `pixel` each was eleven compositor frames.
    -- Nowhere near the Tab path and nowhere near note-stepping.
    local origin = ov:origin()
    local seen = nil
    if origin and origin.client then
      local pts = {}
      for i, x in ipairs(EDGE_ROW) do
        pts[i] = { origin.client.x + x, origin.client.y + 67 }
      end
      seen = host.screen.pixels(pts)
    end
    local edges = {}
    for i, x in ipairs(EDGE_ROW) do
      edges[i] = string.format("%d:%s", x, seen and tostring(seen[i].r) or "?")
    end
    local wide = ""
    if origin and origin.client then
      local r = host.ocr.recognize({
        region = {
          origin.client.x + WIDE_REGION[1], origin.client.y + WIDE_REGION[2],
          origin.client.x + WIDE_REGION[3], origin.client.y + WIDE_REGION[4],
        },
      })
      wide = trimmed(r and r.text)
    end
    -- The name is what we ASKED FOR, and saying so is the point.
    --
    -- It is not a measurement. We click an icon or post a function key and write down what we
    -- intended; whether Melodyne agreed is a separate question, and the log already shows it
    -- disagreeing — there are lines reading `tool=Fade left="0.00 dB"`, which is Amplitude's
    -- value under a sub-tool that never took, and `tool=Note Separation` beside a note name and
    -- cents. Every conclusion drawn from those labels was drawn from something we told
    -- ourselves. So the label is marked, and the pixels beside it are what can be trusted.
    --
    -- The legend names the CONTRAST, not two magic numbers: the border read 157 in one capture
    -- and 146 in another, because Melodyne's greys move with its appearance setting (and with
    -- whatever it draws over itself — the first shot was taken behind a detection dialog). What
    -- does not move is that a border is far darker than the 191 of bare tool bar. A threshold
    -- written into this line would have been wrong within four days.
    host.log.info((
      "[melodyne] fields after asking for tool=%s: left=%q right=%q wide=%q  greys %s"
      .. "  (dark ~146-157 = box border, 191 = bare strip; wide covers x218-320 in case the"
      .. " time field is 100 units rather than 72)"
    ):format(tostring(toolName or "?"), trimmed(note), trimmed(pitch), wide,
      table.concat(edges, " ")))
  end)
end

-- The note-area measurement, on its own clock, while its gate is open (noteAreaGate).
--
-- Registered unconditionally because calibration can be switched on at any time (the overlay
-- runtime's setting, `O.calibrating()`) and the read-outs can fall silent at any time, and the
-- timer is the cheapest possible no-op while the gate is closed: a few comparisons per tick. The
-- log says when the gate opens and closes, and otherwise only what moved.
host.timer.every(NOTE_AREA_MS, function()
  measureNoteArea(ov)
end)

-- The watcher's reads go under one key, and one is out at a time: a tick asks only once the
-- last one has answered (host.ocr.pending), which holds across a disable too, where the answer is
-- dropped and a flag set before the read would stay set. Its picture goes under the same key, a
-- snapshot's own: a tick whose picture has not come yet replaces it with its own. The boxes'
-- profiles (inksIn) go under keys of their own, and a tick while one is out asks nothing either.
local WATCH_KEY = "selection"

host.timer.every(WATCH_MS, function()
  -- Not while a menu is up. A menu is drawn OVER the tool strip, so the read-out region stops
  -- showing the read-outs and starts showing menu entries -- the watcher then reports a
  -- "changed selection" made of whatever the menu happens to say, over and over as the user
  -- moves through it. The reading was never wrong about the pixels; it was wrong about what
  -- those pixels were.
  --
  -- The last good values stay as the baseline rather than being cleared, so closing the menu
  -- does not announce a change that never happened.
  if host.keys.nativeMenuOpen and host.keys.nativeMenuOpen() then return end
  if not ov.active then
    -- Forget on the way out. Whatever the boxes said when Melodyne went away is not a fact
    -- about the Melodyne we come back to, and holding it would mean the first real change
    -- after returning is measured against a stale reading.
    --
    -- Coming back is therefore SILENT about the selection: the read that re-establishes the
    -- baseline does not announce it, and the arrival announcement is the overlay's own. Two
    -- sentences at once, one of them unasked for, is worse than one. And the read-outs' silence
    -- goes with them: it is news about this stay only.
    baseline = nil
    lastLeft, lastRight = "", ""
    readOutsSilent, silentMark = nil, nil
    return
  end
  if host.ocr.pending(WATCH_KEY) then return end
  if host.screen.pending(INK_KEYS.left) or host.screen.pending(INK_KEYS.right) then return end
  local origin = ov:origin()
  if not (origin and origin.client) then return end
  local function onScreen(r)
    return {
      origin.client.x + r[1], origin.client.y + r[2],
      origin.client.x + r[3], origin.client.y + r[4],
    }
  end
  -- WITH A CALLBACK, and that is the point of it. This tick used to wait for its read, and a
  -- handler that waits keeps its module waiting with it: every key of this overlay's own — Tab,
  -- Alt+F, the arrows on the tool bar — queued behind a poll that asks eight times a second, for
  -- 25-74 ms of every 120, and for as long as a read takes when one is slow. Asked with a
  -- callback, the tick ends at once, the keys run, and the answer is a handler of its own when it
  -- comes.
  --
  -- And ONE PICTURE, taken off the event loop at a set time, which is the moment of the question:
  -- the boxes' borders, what each holds and the text in them are all read from it. Which boxes are
  -- drawn used to be two points of the live screen, a compositor frame on the event loop every half
  -- second for as long as the overlay was in front. What each box holds is profiled off the event
  -- loop too, beside the read.
  --
  -- That answer is about the moment it was asked in, so it is checked against that moment: the
  -- mark says whether the overlay is still in front, on the same window and in the same stay,
  -- and the time of the question decides the tool-switch hold.
  local mark = ov:here()
  local asked = host.now and host.now() or nil
  local strip = onScreen(STRIP)
  host.screen.snapshotAsync({ region = strip, key = WATCH_KEY, at = asked }, function(snap)
    if not snap then return end
    if not ov:stillHere(mark) or nativeMenuOpen() then
      snap:release()
      return
    end
    -- Which boxes are drawn, for this read and for the Tab ring's (boxDrawn), and what each holds.
    -- The right box is read only when it is drawn — reading bare tool bar cost the recogniser
    -- time eight times a second (see selectionRegions) — and is taken for drawn when the picture
    -- cannot say, as boxDrawn takes it.
    local drawn = boxesIn(snap, origin)
    if drawn then rememberBoxes(drawn, mark, asked) end
    local regions = { onScreen(NOTE_REGION) }
    if drawn == nil or drawn.right then regions[2] = onScreen(PITCH_REGION) end
    -- The read and the boxes' profiles are asked together, and the read is judged once all of them
    -- have answered, whichever comes last: asking the read only after the profiles had answered
    -- would put a turn of the event loop in front of every announcement.
    local list, ink, inked = nil, nil, drawn == nil
    local function judge()
      local combined, note, pitch, both = selectionOf(list)
      if combined == nil then return end
      -- Gone since, or another window, or the same window in another stay: what the boxes said
      -- then is not news about now, and it is no baseline either.
      if not ov:stillHere(mark) then return end
      -- A menu opened over the strip while the read was out: the picture may be the menu's.
      if host.keys.nativeMenuOpen and host.keys.nativeMenuOpen() then return end
      local left, right = trimmed(note), trimmed(pitch)
      -- An EMPTY READ IS NOT AN EMPTY FIELD, and this distinction is load-bearing.
      --
      -- The recogniser drops out on these small low-contrast boxes now and then: the log has "0 ct"
      -- and "" alternating within the same second with nothing on screen moving. Treating that as
      -- "the field went empty" and then as "it came back" would announce the same unchanged value
      -- over and over — precisely the noise these caches exist to remove. So "" means "no reading
      -- this tick": nothing is announced and nothing is forgotten. Whether the read-outs are
      -- silent is the picture's to say (silenceOf, inkIn), for the stay the read was asked in: a
      -- box with a value in it, read or not, is not silence, and boxes that are empty, hold a dash
      -- or are not drawn are.
      local silent = silenceOf(left, right, both, ink)
      if silent ~= nil then readOutsSilent, silentMark = silent, mark end
      local changedLeft = left ~= "" and left ~= lastLeft
      local changedRight = right ~= "" and right ~= lastRight
      if left ~= "" then lastLeft = left end
      if right ~= "" then lastRight = right end
      -- The first answer to a read asked after the strip's latest change is the new tool's values:
      -- a baseline, said nothing. The hold alone does not make one when a read takes longer than
      -- the hold: the read asked before the switch is answered after the hold and held, but its
      -- values are the OLD tool's, and the next read, asked after the hold, would say the new
      -- tool's values as a change. With reads faster than the hold this answer comes during it and
      -- is held anyway.
      local rebase = rebaseFrom ~= nil and asked ~= nil and asked > rebaseFrom
      if rebase then rebaseFrom = nil end
      if not (baseline and ov:stillHere(baseline)) then
        -- The first read of a stay establishes the baseline rather than announcing it: the
        -- overlay already says what is focused, and a second sentence on top of that is noise.
        --
        -- A mark rather than a flag, so the stay it belongs to is part of it: an excursion to
        -- another window that came and went between two ticks, which the forgetting above never
        -- saw, still starts a new baseline here instead of being measured against the old one.
        baseline = mark
        lastLeft, lastRight = left, right
        return
      end
      if not (changedLeft or changedRight) then return end
      -- Held when the question was asked before the hold lifted — during it, or just before the
      -- tool switch that set it — even if the answer comes after: the picture is of the moment it
      -- was asked in, not of the moment it arrived.
      local holding = asked ~= nil and asked < holdUntil
      -- One line per change, kept permanently: it is what turned "only the cents are announced"
      -- from three indistinguishable guesses into one reading. Written even while the announcement
      -- is being held back, because a tool switch is exactly when the boxes reveal what that tool
      -- puts in them — the held ticks are the evidence, not the noise.
      --
      -- Named left and right rather than note and deviation, because that is all we actually know:
      -- the left box holds a note name under the pitch tools and a level in decibels under
      -- Amplitude, and what the right one holds away from pitch is the open question this line
      -- exists to answer. Calling a field by the value one tool puts in it is the mistake that
      -- produced the gate this replaced.
      host.log.info(("[melodyne] selection: tool=%s left=%q right=%q%s")
        :format(toolForLog(), left, right, holding and "  (held: the tool was just changed)"
          or rebase and "  (held: the first reading since the tool was changed)" or ""))
      if holding or rebase then return end
      local parts = {}
      -- Only what changed, and a DASH is not spoken. Melodyne draws "-" in a box with nothing to
      -- report, so it is a real reading and belongs in the cache — but "minus" is not a thing a
      -- person needs to hear when they have just deselected a note.
      if changedLeft and speakable(left) then parts[#parts + 1] = left end
      if changedRight and speakable(right) then parts[#parts + 1] = right end
      if #parts > 0 then
        host.speech.output(table.concat(parts, ", "), { interrupt = true })
      end
    end
    if drawn then
      inksIn(snap, origin, drawn, function(answer)
        ink, inked = answer, true
        if list then judge() end
      end)
    end
    host.ocr.recognize(regions, { snapshot = snap, key = WATCH_KEY }, function(answer)
      if asked then
        watchReads = watchReads + 1
        watchCost = watchCost + (host.now() - asked)
        if watchReads >= 100 then
          host.log.info((
            "[melodyne] selection watch: %d reads, asked every %d ms at most, %.1f ms each on"
            .. " average from asking to the answer"
          ):format(watchReads, WATCH_MS, watchCost / watchReads))
          watchReads, watchCost = 0, 0
        end
      end
      list = answer
      if inked then judge() end
    end)
    snap:release() -- the read and the profiles keep their own pixels until they are answered
  end)
end)

-- `menus`: Melodyne has a native menu bar, and while one of its menus is open the keys have to
-- reach it rather than steer the overlay. A menu the system draws is what nativePopup sees; in
-- the logs Melodyne's menus took the keys at once and gave them back cleanly, which is that
-- every-tick answer and not the slow accessibility backstop.
ov:attach(melodyne, { menus = { O.menuTests.nativePopup } })

host.log.info(
  "Melodyne: standalone overlay ready — tool bar, variants, menu bar, both inspector "
    .. "read-outs, and the analysis read-out."
)
