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

local function at(ov, x, y)
  local o = ov:origin()
  if not (o and o.client) then return nil end
  return host.screen.pixel(o.client.x + x, o.client.y + y)
end

-- Which tool Melodyne currently has selected, or nil when none reads as lit.
--
-- Asked on arrival, so the tool bar reports where the APPLICATION is rather than where this
-- control last put it. That distinction is the whole point: Melodyne has F1 to F6 for the same
-- job, and a control that only remembers its own last move is confidently wrong the moment the
-- user reaches for the shortcut.
local function activeTool(ov)
  for i, tool in ipairs(TOOLS) do
    local p = at(ov, tool.at[1], tool.at[2])
    if p and nearer(p, ACTIVE, IDLE) then return i end
  end
  return nil
end

-- Which tool is active, asked at most once per input epoch.
--
-- `activeTool` costs up to six pixel reads, and a pixel read costs a fixed compositor frame —
-- around 17 ms, measured, whatever its size. Two things now ask the question on every focus
-- move (the read-outs below, to know which of them exist), so asking it six times per move
-- would put a tenth of a second between a Tab and its announcement.
--
-- Keyed on `inputEpoch` rather than `epoch`: which tool is selected changes only when
-- something ACTS — the user pressing a function key, or us clicking an icon — and that is
-- exactly what turns that counter over. An ordinary epoch would re-read on every question.
local toolMemo = { gen = -1, index = nil }
local function activeToolCached(ov)
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if toolMemo.gen ~= gen then
    toolMemo.gen = gen
    toolMemo.index = activeTool(ov)
  end
  return toolMemo.index
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
local function holdReadouts(ms)
  if not host.now then return end -- no clock, no window; better than a hold that never lifts
  holdUntil = host.now() + (ms or READOUT_HOLD_MS)
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
-- the read-outs change while the user walks from note to note with the arrow keys, and six
-- screen reads at a compositor frame each on that path would buy that knowledge with the
-- responsiveness of the thing being described. So: free when the epoch already asked, absent
-- otherwise. It fills in whenever the user has just been in the overlay, which is exactly when
-- somebody is looking at a read-out anyway.
local function activeToolIfKnown()
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if toolMemo.gen ~= gen then return nil end
  local i = toolMemo.index
  return i and TOOLS[i] and TOOLS[i].name or nil
end

-- The tool this read-out belongs to, and HOW we know — measured, chosen, or not at all.
--
-- Three different facts wear the same word otherwise, and the log is where the difference will
-- matter: a probed name was read off the screen just now, a selected one is what we clicked and
-- could have been overridden inside Melodyne since, and "?" is the honest answer when neither
-- is available and the alternative would be six screen reads on the note-stepping path.
local function toolForLog()
  local probed = activeToolIfKnown()
  if probed then return probed .. " (probed)" end
  if selectedToolName then return selectedToolName .. " (selected)" end
  return "?"
end

-- Which variant of a tool is live: the first whose probe points ALL read near-black, else the
-- base. Only meaningful while that tool's icon is lit, since the probes read its glyph against
-- the lit white box.
local function activeVariant(ov, tool)
  for i, v in ipairs(tool.variants or {}) do
    if #v.probes > 0 then
      local all = true
      for _, probe in ipairs(v.probes) do
        local p = at(ov, probe[1], probe[2])
        if not (p and nearer(p, BLACK, WHITE)) then all = false break end
      end
      if all then return i + 1 end
    end
  end
  return 1
end

-- The same reading, asked at most once per input epoch per tool.
--
-- Three callers want it while the tool bar is being announced — the tab's `current`, its
-- `verticalName`, and the left read-out's owner name — and each probe point is a screen read at
-- a compositor frame. Landing on the tool bar was measured at 89 to 110 ms of pure announcement
-- because of it, which is a tenth of a second of silence after a Tab.
--
-- Keyed on the input epoch for the reason `activeToolCached` is: which variant is live changes
-- only when something ACTS, and that is exactly what turns the counter over. A user walking the
-- Tab ring does not turn it, so the walk is free after the first look.
local variantMemo = { gen = -1, tool = nil, index = 1 }
local function activeVariantCached(ov, tool)
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if variantMemo.gen ~= gen or variantMemo.tool ~= tool then
    variantMemo.gen, variantMemo.tool = gen, tool
    variantMemo.index = activeVariant(ov, tool)
  end
  return variantMemo.index
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
        local seen = activeVariant(o, tool)
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
      -- this instant. Not measured here: the next question asks, and it will come within
      -- milliseconds anyway.
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
      -- every announcement, though: this runs each time the tool bar is spoken, and six screen
      -- reads at a compositor frame each measured 89 to 110 ms of silence after a Tab. The
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

-- Is the right box drawn RIGHT NOW?
--
-- This is the same question the removed `when` predicate answered by asking which tool was
-- selected — from a table of tool names taken out of five documentation figures, which left out
-- the Main tool and cost the user his cents read-out. The question was never wrong; the
-- instrument was. One pixel against another says what Melodyne is drawing, needs no list of
-- tools, and cannot go stale when Celemony adds one.
--
-- Memoised per input epoch, like the tool probe: the box changes only when something ACTS, and
-- that is exactly what turns the counter over. So a whole Tab walk pays two screen reads at
-- most, and an idle watcher pays none.
--
-- Defaults to DRAWN when the pixels cannot be read. A control that quietly vanishes tells a
-- user who cannot see the screen nothing about why, and "there is a stop here that says no
-- value" is a far smaller failure than "the stop you were looking for is gone".
-- Held for a while, and NOT keyed on the input epoch.
--
-- Which boxes are drawn is a property of the TOOL, not of the selection — and the input epoch
-- turns over on every keystroke, so an epoch-keyed memo re-measured twice on every arrow press
-- while the user walked notes, which is the one path that must stay quick. A short time window
-- plus an explicit invalidation when we change tool matches what actually varies: free during a
-- walk, fresh immediately after a switch, and self-healing within half a second if the tool ever
-- changes without our seeing it.
local BOX_MEMO_MS = 500
local boxMemo = { left = { at = nil, drawn = true }, right = { at = nil, drawn = true } }
forgetBoxes = function()
  boxMemo.left.at, boxMemo.right.at = nil, nil
end
local function boxDrawn(o, which)
  local m, p = boxMemo[which], BOX_PROBES[which]
  local now = host.now and host.now() or nil
  if m.at and now and (now - m.at) < BOX_MEMO_MS then return m.drawn end
  local edge, gap = at(o, p.edge[1], p.edge[2]), at(o, p.gap[1], p.gap[2])
  m.drawn = (edge == nil or gap == nil) or (gap.r - edge.r) > BOX_EDGE_CONTRAST
  m.at = now
  return m.drawn
end
local function leftBoxDrawn(o) return boxDrawn(o, "left") end
local function rightBoxDrawn(o) return boxDrawn(o, "right") end

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
-- every so often — the interval is a number to be chosen from evidence, and going faster still
-- is a trade against the pump that should be made with the figure in view rather than by feel.
local WATCH_MS = 120
local watchTicks, watchCost = 0, 0

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
local haveBaseline = false

-- Whitespace off both ends, so a stray space from the recogniser does not read as a change.
local function trimmed(v)
  return (tostring(v or ""):match("^%s*(.-)%s*$"))
end

-- Worth saying out loud. A dash is Melodyne's "no single value": it stands there when nothing
-- is selected (measured — see docs/melodyne-reference.md §6.1) and when SEVERAL notes are and
-- they disagree. Every width the recogniser might hand back, and the three-hyphen form too:
-- two Celemony figures and five pages say one dash, one English page says three, so the
-- reference document's own instruction is to tolerate both (§7).
local function speakable(v)
  return v ~= "" and v ~= "-" and v ~= "–" and v ~= "—" and v ~= "--" and v ~= "---"
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
local function readSelection(o)
  local origin = o:origin()
  if not (origin and origin.client) then return nil end
  local function field(region)
    local r = host.ocr.recognize({
      region = {
        origin.client.x + region[1], origin.client.y + region[2],
        origin.client.x + region[3], origin.client.y + region[4],
      },
    })
    return (r and r.text) or ""
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
  local wantRight = boxDrawn(o, "right")
  if not wantRight then
    local note = field(NOTE_REGION)
    return note .. "|", note, ""
  end
  -- BOTH boxes, one screen touch. `recognizeMany` captures the rectangle that encloses them
  -- once and recognises each region out of that single frame — which is the whole cost here:
  -- the recognition measured 4-6 ms against a capture's fixed ~17. Two separate calls put this
  -- tick at 44 ms; this puts it at 27, and the two regions stay two regions, so the per-region
  -- fallback that once rescued a dropped note name is untouched.
  local rs = host.ocr.recognizeMany({
    regions = {
      { origin.client.x + NOTE_REGION[1], origin.client.y + NOTE_REGION[2],
        origin.client.x + NOTE_REGION[3], origin.client.y + NOTE_REGION[4] },
      { origin.client.x + PITCH_REGION[1], origin.client.y + PITCH_REGION[2],
        origin.client.x + PITCH_REGION[3], origin.client.y + PITCH_REGION[4] },
    },
  })
  local note = (rs[1] and rs[1].text) or ""
  local pitch = (rs[2] and rs[2].text) or ""
  return note .. "|" .. pitch, note, pitch
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
-- size, which reads as "the values are not continuous" — and they were not. A capture is 17 ms,
-- so 200 ms is under a tenth of the timer thread, and it is only paid under the tools that have
-- nothing else to say.
local NOTE_AREA_MS = 200
local lastAreaSize = nil
local CHANGE_THRESHOLD = 6 -- mean, of 255; below this is recogniser and compositor noise
local MIN_THRESHOLD = 8 -- INK: rows of a column, against a floor of about 19 and a blob of 30-57
-- What the LAST TICK actually read, as opposed to what is remembered.
--
-- The caches above deliberately never forget: an empty OCR read is not an empty field, so a
-- value survives a recogniser drop-out. That is right for announcing and wrong for asking
-- "does this tool show anything at all" — once any tool had displayed a value the answer was
-- yes for ever, and the note-area measurement, which only runs when the answer is no, went
-- silent for a whole session without a line to say why.
local readOutsSilent = true
-- The note's last known position, as the centre of the columns ink arrived in.
local lastArrival = nil
local lastArea, lastAreaMin = nil, nil
-- The bar lines of the previous frame, so a scroll can be told from an edit.
local prevLines = nil

-- A distance as a FRACTION OF A BAR, named when it is close to a simple one.
--
-- Melodyne moves a note by whole grid units, so pressing the key twice does the same thing
-- twice; the measurement of it carries about two pixels of noise. Read out to the percent that
-- became "33, 38, 31, 35, 33, 34" for six identical presses, which sounds random and is not.
-- The user called it inconsistent, and he was describing my arithmetic rather than his edit.
--
-- The measured ratios sat at 0.31 to 0.38 against a bar that stayed within 98 to 102 pixels, so
-- the honest reading is a third of a bar, six times. Naming the nearest simple fraction when the
-- measurement is within five hundredths of it says exactly that and stays silent about a
-- precision the picture does not carry. Anything further from a named fraction keeps the
-- percentage, because inventing a name for 0.44 would be worse than admitting the number.
--
-- Fractions of a BAR, still, never note values: those need a time signature, and
-- bar.beat.sixteenth tells us how the transport counts, not how many beats a bar holds.
local FRACTIONS = {
  { 1 / 16, "a sixteenth of a bar" }, { 1 / 12, "a twelfth of a bar" },
  { 1 / 8, "an eighth of a bar" }, { 1 / 6, "a sixth of a bar" },
  { 1 / 4, "a quarter of a bar" }, { 1 / 3, "a third of a bar" },
  { 1 / 2, "half a bar" }, { 2 / 3, "two thirds of a bar" },
  { 3 / 4, "three quarters of a bar" }, { 1, "a bar" },
}
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
local TIMESIG_REGION = { 622, 12, 668, 34 }
local sigMemo = { gen = -1, beats = nil }
local function beatsPerBar(o)
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  if sigMemo.gen ~= gen then
    sigMemo.gen = gen
    sigMemo.beats = nil
    local origin = o:origin()
    if origin and origin.client then
      local r = host.ocr.recognize({
        region = {
          origin.client.x + TIMESIG_REGION[1], origin.client.y + TIMESIG_REGION[2],
          origin.client.x + TIMESIG_REGION[3], origin.client.y + TIMESIG_REGION[4],
        },
      })
      local top = trimmed(r and r.text):match("^(%d+)%s*[/l|]%s*%d+$")
      local n = tonumber(top)
      -- A bar of one to sixteen beats, or we did not read a time signature.
      if n and n >= 1 and n <= 16 then sigMemo.beats = n end
    end
  end
  return sigMemo.beats
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
-- median of the recent displacements IS that unit, whatever the zoom and whatever the grid is
-- set to, and the announcement becomes a count of steps: what he did, not what I inferred about
-- the ruler behind it. His own run bears it out — 33, 34, 35, 44 and then a 90, which is two.
local stepHistory, barHistory = {}, {}
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

-- How big one press is, and in what: beats when the numbers agree on it, steps otherwise.
--
-- Both ingredients are noisy on their own — the displacement carries about two pixels, the bar
-- detector wandered between 35 and 136 in one session — so both are taken as MEDIANS of the
-- last dozen, where the noise cancels and a zoom change washes through within a few presses.
-- Then: how many steps fit in a bar, against how many beats the time signature says a bar
-- holds. When those agree, one step IS one beat, and the announcement can say so.
local function stepsFor(px, barW, beats)
  -- A nudge must not teach the step size: the median is what a PRESS OF THE GRID KEY does, and
  -- feeding it one-pixel moves would drag it down until a real step counted as several.
  if px >= 8 then remember(stepHistory, px) end
  if barW and barW > 0 then remember(barHistory, barW) end
  local unit = median(stepHistory)
  if not unit or unit < 4 then return nil, nil end
  -- A move well under one unit is Melodyne's NUDGE: Ctrl+Alt is the same gesture with the grid
  -- switched off, and it moves one to four pixels where a grid step moves thirty-four. Rounding
  -- that to the nearest number of steps gives zero, and the old code then reported it as one —
  -- a fine nudge announced as a whole beat. It gets its own word instead, and no quantity: at
  -- one to four pixels the measurement carries a quarter of its own size in noise, and a number
  -- built on that would be decoration.
  if px < unit * 0.5 then return 1, "fine step" end
  local n = math.max(1, math.floor(px / unit + 0.5))
  local bar = median(barHistory)
  if beats and bar and bar > unit then
    local perBar = bar / unit
    if math.abs(perBar - beats) <= 0.6 then return n, "beat" end
  end
  return n, "step"
end

local function asFraction(ratio)
  for _, f in ipairs(FRACTIONS) do
    if math.abs(ratio - f[1]) <= 0.05 then return f[2] end
  end
  return nil
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
local MAX_SHIFT = 60
local function bestShift(prevMin, minv, cols)
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
  for k = -MAX_SHIFT, MAX_SHIFT do
    local s = score(k)
    if s < bestScore then best, bestScore = k, s end
  end
  return best, bestScore, score(0)
end

local function measureNoteArea(o)
  -- Runs for TWO reasons, and neither of them is "always". While calibrating it is an
  -- instrument and writes to the log. The rest of the time it runs only when the read-outs are
  -- silent — which is measured, not assumed, and is exactly the tools that have no number
  -- anywhere in the application. Under Pitch or Amplitude it does not run at all, so the
  -- overlay does not pay a capture every 400 ms for a window that is already answering.
  local silent = readOutsSilent
  if not (host.calibrating or silent) then return end
  if not ov.active then
    -- Forget on the way out, exactly as the read-out watcher does and for the same reason: a
    -- frame from before an excursion is not a fact about the Melodyne we come back to, and this
    -- instrument's whole premise is that only ONE thing changed between two frames. Diffing
    -- across a trip to another window would report that trip as a note move.
    lastArea, lastAreaMin, lastAreaSize, lastArrival = nil, nil, nil, nil
    return
  end
  if host.keys.nativeMenuOpen and host.keys.nativeMenuOpen() then return end
  if not (host.screen and host.screen.profile) then return end
  local origin = o:origin()
  if not (origin and origin.client) then return end
  local cw, ch = O.contentSize(origin)
  if not (cw and ch) then return end
  local x2, y2 = math.min(NOTE_AREA[3], cw - 2), math.min(NOTE_AREA[4], ch - 2)
  if x2 - NOTE_AREA[1] < 40 or y2 - NOTE_AREA[2] < 40 then return end
  -- A window that changed size re-baselines instead of being diffed: the whole picture moved,
  -- and every column would be a run. The column COUNT cannot catch this by itself — it is
  -- clamped to the same numbers for every window at least this wide — so the size is kept.
  local size = cw .. "x" .. ch
  if lastAreaSize ~= size then
    lastArea, lastAreaMin, lastAreaSize, lastArrival = nil, nil, size, nil
  end
  local p = host.screen.profile({
    region = {
      origin.client.x + NOTE_AREA[1], origin.client.y + NOTE_AREA[2],
      origin.client.x + x2, origin.client.y + y2,
    },
    -- Columns only: a note's identity is a horizontal position, and asking for rows as well
    -- would double the reduction for something nothing reads.
    axes = "columns",
    -- 190 rather than the default: Melodyne's note area is a light page, and the measured
    -- classes above were separated at exactly this level.
    dark = 190,
  })
  if not (p and p.columns and p.columns.mean and p.columns.dark) then return end
  -- The INK COUNT is the working channel; the mean is kept only for the bar lines, which are
  -- full-height and show up in it plainly.
  --
  -- Measured on a real capture at threshold 190: background and grid lines about 19 rows per
  -- column, note blobs 29 to 57, a full-height cursor 470 to 516. Three classes, cleanly apart.
  -- The mean gave a blob four units out of 255 and the minimum gave it nothing at all, because
  -- in a busy picture nearly every column already holds something black and cannot go blacker.
  local mean, minv = p.columns.mean, p.columns.dark
  local prev, prevMin = lastArea, lastAreaMin
  lastArea, lastAreaMin = mean, minv
  if not (prev and prevMin) or #prev ~= #mean then
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
  -- Silence when nothing moved: this runs on a timer now, and a line every 400 ms saying that
  -- an idle screen is still idle would bury the ones that matter.
  if #runs == 0 then return end
  -- A MASS change is the view moving, not a note moving, and it is the one case where the
  -- differential premise fails outright. It showed up the first evening: walking past the right
  -- edge made Melodyne scroll, and two frames reported 48 and 53 runs while the position jumped
  -- from bar 38 to bar 40. Everything after such a frame is measured against a picture of a
  -- different stretch of music, so the baseline is thrown away rather than believed.
  if #runs > 12 then
    host.log.info(("[melodyne] note area: %d run(s) — the view moved, re-baselining"):format(#runs))
    lastArea, lastAreaMin = nil, nil
    return
  end
  -- Ranked by whichever channel saw the most, so a blob move is not buried under a cursor move.
  table.sort(runs, function(a, b)
    return math.max(a.peak, a.peakMin) > math.max(b.peak, b.peakMin)
  end)
  -- SPOKEN, when nothing else in this window will say anything.
  --
  -- Under Time, Time Handle, Note Separation and Fade there is no read-out to change and no
  -- number anywhere in the application: the position belongs to the cursor and the cursor does
  -- not follow an edit (measured — twelve semitone moves and four time moves, transport
  -- unchanged throughout). A blind user pressing Ctrl+Right there gets silence, which is
  -- indistinguishable from a key that did nothing.
  --
  -- The condition is measured rather than assumed: we speak only when the read-outs themselves
  -- said nothing this tick, so the pitch, formant and amplitude tools — which do have a number
  -- and do announce it — are never talked over.
  --
  -- What is said is a DISPLACEMENT, in bars, because that is what the picture actually
  -- supports. "Eleven pixels" is not an answer a musician can act on, and a note value would
  -- assume a time signature nobody has measured: bar.beat.sixteenth tells us how the transport
  -- counts, not how many beats a bar holds. A fraction of a bar assumes nothing.
  if readOutsSilent and #runs >= 1 then
    -- Every column that changed, and nothing else. See bestShift for why the surroundings must
    -- stay out of it.
    -- Only the columns where something ARRIVED — where the picture got DARKER than it was here.
    --
    -- The changed set holds both halves of a move: the columns the note came to, and the ones
    -- it left. Those two are mirror images, so a shifted comparison fits the first at +d and the
    -- second at -d about equally well, and which of them wins is a coin toss. That is exactly
    -- what the user heard: "left" for a Ctrl+Right and back again, with the fit looking healthy
    -- every time — 5 to 14 against 25 to 40 standing still. The estimator was measuring
    -- correctly and answering a question with two answers.
    --
    -- Where a dark thing IS now has one answer to "where was that darkness a moment ago". So the
    -- arrival columns alone are scored, and the sign follows from the physics rather than from
    -- whichever half of the change happened to be larger.
    -- A LOWER bar for "arrived" than for "changed at all", and deliberately so. The run list is
    -- built from either channel, so a run can exist because the mean moved while the minimum
    -- barely did; requiring the full change threshold again here left half the frames with fewer
    -- than four arrival columns and therefore silent. The user heard a value, two gaps, another
    -- value — correct numbers that sounded like noise because of what was missing between them.
    -- Ink ARRIVING in a column, in rows. A blob covers 30 to 57 rows where the page floor is
    -- about 19, so eight rows is a clear arrival and still well above the flicker of anti-
    -- aliasing on a redraw.
    local ARRIVED = 8
    local cols = {}
    for _, r in ipairs(runs) do
      for i = r.from, r.to do
        -- MORE ink than before. The comparison points the other way from the luminance one it
        -- replaces, and getting that backwards would invert every direction this speaks.
        -- Same exclusion as the centroid below: a full-height column is the playback cursor,
        -- not a note, and it must not enter the comparison either.
        local now, was = minv[i] or 0, prevMin[i] or 0
        if now < 150 and was < 150 and now >= was + ARRIVED then cols[#cols + 1] = i end
      end
    end
    -- Kept as a SECOND OPINION in the log and nothing more. It is the estimate that aliased,
    -- and a shift it agrees on is a small reassurance while a disagreement is worth seeing.
    local corr, sBest, sStill = bestShift(prevMin, minv, cols)
    -- WHERE THE DARKNESS APPEARED against WHERE IT LEFT — a sign that cannot alias.
    --
    -- The correlation above is the wrong instrument for this picture and the user's test proved
    -- it: three deliberate Ctrl+Rights, the runs marching right in every frame (327, 345, 380,
    -- 467), and the answers were -44, -26, +47. Notes in a phrase sit 8 to 40 pixels apart while
    -- the search window reaches 60, so a shifted copy fits the NEIGHBOURING note as well as the
    -- right one. A quasi-periodic picture has several equally good alignments and which wins is
    -- an accident; no threshold repairs that.
    --
    -- Centroids cannot make that mistake. Whatever the magnitude, if a thing moved right then
    -- the columns that got darker lie to the right of the columns that got lighter. It says
    -- nothing reliable about HOW FAR — for a move shorter than the note's own width the two
    -- centroids are a note-width apart, not a displacement apart — which is exactly why it is
    -- logged beside the correlation rather than spoken instead of it.
    -- Over the WHOLE profile, not over the runs.
    --
    -- The runs are a summary written for the log — grouped, gap-tolerant, one to three columns
    -- wide — and building a centroid inside them left samples of two and five columns. Two
    -- correct answers out of a sample that small is indistinguishable from two lucky guesses,
    -- and this estimate exists precisely because the last one was believed too early. Every
    -- column that changed enough counts here; it costs one pass over 887 numbers.
    local aSum, aN, dSum, dN = 0, 0, 0, 0
    -- FULL-HEIGHT COLUMNS ARE NOT NOTES, and skipping them is what the ink channel was for.
    --
    -- Melodyne's playback cursor is a line spanning the whole editor, measured at 470 to 516
    -- dark rows against 29 to 57 for a note blob. While the transport runs it crosses the area
    -- several times a second, arriving in a new column each frame — so the user pressed Play and
    -- was read a sentence every 200 ms. I measured that separation on his own capture hours ago
    -- and then wrote an arrival test that could not tell the two apart.
    local FULL_HEIGHT = 150
    for i = 1, #minv do
      local now, was = minv[i] or 0, prevMin[i] or 0
      if now < FULL_HEIGHT and was < FULL_HEIGHT then
        if now >= was + ARRIVED then aSum, aN = aSum + i, aN + 1
        elseif was >= now + ARRIVED then dSum, dN = dSum + i, dN + 1 end
      end
    end
    -- WHERE THE NOTE IS NOW, tracked from frame to frame — not where it is against where it was
    -- within one frame.
    --
    -- Requiring both halves in the same 200 ms picture cost twenty frames out of forty-nine, and
    -- every one of those silences reads "0 arrived, 1 left" or "1 arrived, 0 left". A frame lands
    -- mid-redraw: the note is already gone from the old place and not yet drawn in the new one,
    -- or the reverse. Half the answer is there and I threw it away.
    --
    -- The arrival columns ARE the note's new position, so successive arrivals are successive
    -- positions and their difference is the step. The user's own run says it plainly — arrivals
    -- at 379, 345, 311, 276, 242, 208, 174, 139, 105, 70, a difference of 34 every time. The
    -- departure side is kept only for the log, where it is a second witness.
    local arrivalX = aN > 0 and (aSum / aN) or nil
    local centroid = (arrivalX and lastArrival) and (arrivalX - lastArrival) or nil
    if arrivalX then lastArrival = arrivalX end
    if host.calibrating then
      host.log.info(("[melodyne] centroid says %s by %s px (%d arrived, %d left)")
        :format(
          centroid and (centroid > 0 and "right" or "left") or "nothing",
          centroid and string.format("%.0f", math.abs(centroid)) or "?", aN, dN))
    end
    -- SPOKEN AGAIN, on the centroid and not on the correlation, because a controlled test
    -- earned it: five Ctrl+Rights followed by five Ctrl+Lefts produced
    --
    --   right 13, right 27, right 56, right 22, right 22, right 21
    --   left 21, left 22, left 22, left 56, left 27
    --
    -- Eleven signs out of eleven, and the magnitudes come back MIRRORED — the same journey
    -- retraced. That symmetry is the part no estimator can fake by guessing, and it is why this
    -- is evidence where "two correct answers" was not.
    --
    -- A sample floor rather than a fit threshold: the failures that remain are frames with two
    -- or three changed columns, where a centroid is an average of almost nothing. Five columns
    -- on each side keeps every frame of the test above and drops exactly those.
    -- HOW FAR DID THE VIEW ITSELF MOVE? Everything above is window-relative, and that is the
    -- flaw underneath every version of this so far.
    --
    -- Melodyne scrolls to follow a note being dragged out of view. When it does, the note and
    -- the whole picture move together, a window-relative measurement sees an enormous
    -- displacement, and it grows with every press — which is exactly what the user heard: 39,
    -- 39, 47, 59, 54, with the changed columns spread across two hundred pixels at once. The
    -- earlier test that came back eleven-for-eleven and mirrored simply never scrolled.
    --
    -- The bar lines are the anchor, and they are already found reliably (verified against a real
    -- capture: eight lines, no false positives, on a picture full of blobs). They belong to the
    -- MUSIC, so if they moved, the view moved by that much, and the note's real displacement is
    -- what is left after subtracting it. A scroll with no edit then measures zero, which is the
    -- truth, instead of the largest number of the session.
    local lines = barLines(mean)
    local viewShift = nil
    if prevLines and #prevLines >= 2 and #lines >= 2 then
      -- Median of the nearest-neighbour offsets: a line entering or leaving the view at the
      -- edge must not drag the estimate with it.
      local offs = {}
      for _, a in ipairs(lines) do
        local best, bestD = nil, math.huge
        for _, b in ipairs(prevLines) do
          local d = math.abs(a - b)
          if d < bestD then best, bestD = b, d end
        end
        if best and bestD <= MAX_SHIFT then offs[#offs + 1] = a - best end
      end
      if #offs >= 2 then
        table.sort(offs)
        viewShift = offs[math.floor(#offs / 2) + 1]
      end
    end
    prevLines = lines
    local real = centroid and (centroid - (viewShift or 0)) or nil
    if host.calibrating then
      host.log.info(("[melodyne] view moved %s px, so the note moved %s px")
        :format(viewShift and tostring(viewShift) or "?",
          real and string.format("%.0f", real) or "?"))
    end
    -- SPOKEN. Earned on the ink channel by a controlled test, and by the same standard of
    -- evidence that the earlier attempt failed to meet:
    --
    --   right  +32 +26 +40 +41
    --   left   -41 -40 -26 -32 -36 -44 -41
    --
    -- Eleven signs, all correct, with the magnitudes mirrored — 32, 26, 40, 41 out and 41, 40,
    -- 26, 32 back, the same journey retraced. The raw runs corroborate it independently: 786,
    -- 819, 854, 888, 922 and exactly back down the same columns. And unlike every earlier
    -- version, this channel was measured against the picture BEFORE anything was built on it.
    --
    -- A magnitude floor rather than a sample floor. The ink channel reports single columns, so
    -- a run of five on each side — the rule the previous attempt used — would have silenced six
    -- of these eleven correct measurements and reproduced exactly the disconnected feel the user
    -- reported. Both sides must be present, since a centroid needs two, and the move must be
    -- worth mentioning.
    -- No departure requirement any more: a step is the distance between two positions, and both
    -- of those come from the arrival side alone.
    -- Two pixels, not eight: the fine nudge lives between one and four, and eight was silencing
    -- the whole of Ctrl+Alt. The run detection above is what keeps noise out — an idle picture
    -- produces no changed columns at all, so there is nothing here to be jittery about.
    local confident = real ~= nil and aN >= 1 and math.abs(real) >= 2
    local moved = real or 0
    if host.calibrating then
      host.log.info(("[melodyne] correlation says %d px (fit %.1f against %.1f standing still)")
        :format(corr, sBest, sStill))
    end
    if confident then
      -- Scaled where the note ARRIVED, since that is where it now is and the bars are not
      -- evenly spaced.
      local barW = barWidthAt(lines, math.floor(arrivalX or 1))
      local dir = moved > 0 and "right" or "left"
      local beats = beatsPerBar(o)
      local n, unit = stepsFor(math.abs(moved), barW, beats)
      if n then
        if host.calibrating then
          host.log.info(("[melodyne] spoke %d %s(s): %d px, bar %s px, time signature %s")
            :format(n, unit, math.abs(moved), tostring(barW), tostring(beats)))
        end
        local word = n == 1 and ("one " .. unit) or (n .. " " .. unit .. "s")
        host.speech.output(dir .. ", " .. word, { interrupt = true })
      elseif barW and barW > 0 then
        local ratio = math.abs(moved) / barW
        local pct = math.floor(ratio * 100 + 0.5)
        local named = asFraction(ratio)
        -- Both numbers, because the user heard the percentage SHRINK while the pixel step held
        -- steady at 33 to 35 — one grid step per press, six presses running. The bars are not
        -- equally wide (99 to 131 measured across one view), so the same distance is a smaller
        -- fraction of a wider bar, and the fraction moved while the edit did not. Whether that
        -- is Melodyne slowing down or my line detector calling two different things a bar is
        -- exactly what these two numbers side by side will show.
        if host.calibrating then
          host.log.info(("[melodyne] spoke %s: %d px against a bar of %d px (ratio %.3f) at x%d")
            :format(named or (pct .. "%"), math.abs(moved), barW, ratio,
              math.floor(arrivalX or 1) + NOTE_AREA[1] - 1))
        end
        host.speech.output(named and (dir .. ", " .. named) or ("%s, %d%% of a bar"):format(dir, pct),
          { interrupt = true })
      else
        -- Nothing to scale by at all: the direction is still true, and a pixel count is not.
        host.speech.output(dir, { interrupt = true })
      end
    end
  end
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
  -- Either answer is worth having and neither requires anybody to look at a screen. One OCR per
  -- moved frame, only while calibrating.
  local transport = ""
  if host.calibrating and origin.client then
    local tr = host.ocr.recognize({
      region = {
        origin.client.x + TRANSPORT_REGION[1], origin.client.y + TRANSPORT_REGION[2],
        origin.client.x + TRANSPORT_REGION[3], origin.client.y + TRANSPORT_REGION[4],
      },
    })
    transport = trimmed(tr and tr.text)
  end
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
  -- Logged only while calibrating: during ordinary use this fires whenever a note moves, and
  -- a log that writes on every edit is a log nobody reads.
  if host.calibrating then
    host.log.info(("[melodyne] note area: %d run(s) moved, strongest %s  transport=%q")
      :format(#runs, table.concat(parts, " "), transport))
  end
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
    -- Pixels at ~17 ms each, once per tool switch, in a callback already waiting for a repaint.
    -- Nowhere near the Tab path and nowhere near note-stepping.
    local edges = {}
    for i, x in ipairs(EDGE_ROW) do
      local p = at(ov, x, 67)
      edges[i] = string.format("%d:%s", x, p and tostring(p.r) or "?")
    end
    local origin = ov:origin()
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

-- The note-area instrument, on its own clock and only while calibrating.
--
-- Registered unconditionally because `host.calibrating` is read when the VM is built and the
-- timer is the cheapest possible no-op when it is off: one comparison per 400 ms. Silent unless
-- something actually moved, so an idle Melodyne writes nothing at all.
host.timer.every(NOTE_AREA_MS, function()
  measureNoteArea(ov)
end)

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
    -- sentences at once, one of them unasked for, is worse than one.
    haveBaseline = false
    lastLeft, lastRight = "", ""
    return
  end
  local began = host.now and host.now() or nil
  local combined, note, pitch = readSelection(ov)
  if began then
    watchTicks = watchTicks + 1
    watchCost = watchCost + (host.now() - began)
    if watchTicks >= 100 then
      host.log.info(("[melodyne] selection watch: %d ticks at %d ms, %.1f ms each on average")
        :format(watchTicks, WATCH_MS, watchCost / watchTicks))
      watchTicks, watchCost = 0, 0
    end
  end
  if combined == nil then return end
  local left, right = trimmed(note), trimmed(pitch)
  -- An EMPTY READ IS NOT AN EMPTY FIELD, and this distinction is load-bearing.
  --
  -- The recogniser drops out on these small low-contrast boxes now and then: the log has "0 ct"
  -- and "" alternating within the same second with nothing on screen moving. Treating that as
  -- "the field went empty" and then as "it came back" would announce the same unchanged value
  -- over and over — precisely the noise these caches exist to remove. So "" means "no reading
  -- this tick": nothing is announced, and nothing is forgotten either.
  readOutsSilent = not speakable(left) and not speakable(right)
  local changedLeft = left ~= "" and left ~= lastLeft
  local changedRight = right ~= "" and right ~= lastRight
  if left ~= "" then lastLeft = left end
  if right ~= "" then lastRight = right end
  if not haveBaseline then
    -- The first read after arriving establishes the baseline rather than announcing it: the
    -- overlay already says what is focused, and a second sentence on top of that is noise.
    haveBaseline = true
    return
  end
  if not (changedLeft or changedRight) then return end
  local holding = host.now ~= nil and host.now() < holdUntil
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
    :format(toolForLog(), left, right, holding and "  (held: the tool was just changed)" or ""))
  if holding then return end
  local parts = {}
  -- Only what changed, and a DASH is not spoken. Melodyne draws "-" in a box with nothing to
  -- report, so it is a real reading and belongs in the cache — but "minus" is not a thing a
  -- person needs to hear when they have just deselected a note.
  if changedLeft and speakable(left) then parts[#parts + 1] = left end
  if changedRight and speakable(right) then parts[#parts + 1] = right end
  if #parts > 0 then
    host.speech.output(table.concat(parts, ", "), { interrupt = true })
  end
end)

-- `menus = true`: Melodyne has a native menu bar, and while a menu is open the captured
-- navigation keys have to reach it rather than steer the overlay.
ov:attach(melodyne, { menus = true })

host.log.info(
  "Melodyne: standalone overlay ready — tool bar, variants, menu bar, and both inspector "
    .. "read-outs."
)
