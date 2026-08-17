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
-- either of them somewhere it does not belong.
local reportFieldsAfterSwitch = nil

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
      local i = activeTool(o)
      if not i then return nil end
      -- The TOOL is re-detected every time, because its probe is a clean bright-against-grey
      -- reading and Melodyne's own F-keys change it behind our back.
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
      local seen = activeVariant(o, TOOLS[i])
      if seen > 1 then variant = seen end
      return i
    end,
    -- Down and Up step this tool's stacked variants, which is where they belong: a variant is a
    -- property of the tool, not a separate thing beside it. This was briefly a control of its
    -- own on Alt+V, which worked and said the wrong thing about the shape of the UI.
    verticalName = function(o, i)
      local tool = TOOLS[i]
      if not (tool and tool.variants) then return nil end
      variant = activeVariant(o, tool)
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
      return (target == 1 and tool.name or tool.variants[target - 1].name) .. " tool"
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

-- What the left field is showing, in the words of the tool that owns it. Nil when no tool
-- reads as lit, in which case the label alone is the honest answer.
local function inspectorOwner(o)
  local i = activeToolCached(o)
  if not i then return nil end
  local tool = TOOLS[i]
  local v = activeVariant(o, tool)
  if v > 1 and tool.variants and tool.variants[v - 1] then
    return tool.variants[v - 1].name
  end
  return tool.name
end

ov:addOCRButton({
  label = "Inspector",
  region = NOTE_REGION,
  readOnly = true,
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
  -- So a Tab landing here under Formant is landing on something Melodyne is not drawing, and
  -- "no value" is the honest thing to say: not silence, which would be indistinguishable from
  -- a broken overlay, and not a sentence about optical character recognition.
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
  -- Both, always, because both boxes are always drawn (see the control above). This briefly
  -- skipped the second read when it believed only the Pitch tool had a second field. It cost
  -- more than the OCR it saved: it also put a PIXEL PROBE of the tool bar into every tick, so a
  -- 120 ms watcher paid six screen reads at a compositor frame each every time the user pressed
  -- anything. The tool is asked once per CHANGE now, for the log line, and not per tick.
  local note = field(NOTE_REGION)
  local pitch = field(PITCH_REGION)
  return note .. "|" .. pitch, note, pitch
end

-- One reading per tool switch, written down and not spoken. See the tool bar's `onSelect`.
--
-- Timed to land after the strip has repainted and before the hold lifts, so it measures the new
-- tool rather than the old one and cannot itself become an announcement. It says nothing to the
-- user; it is here so that the question "what does THIS tool show in those two boxes" is
-- answered by using Melodyne, by whoever is already using it, rather than by anybody
-- describing a screen.
reportFieldsAfterSwitch = function(toolName)
  if not host.timer then return end
  host.timer.after(READOUT_HOLD_MS - 150, function()
    if not ov.active then return end
    if host.keys.nativeMenuOpen and host.keys.nativeMenuOpen() then return end
    local _, note, pitch = readSelection(ov)
    if note == nil then return end
    -- Four pixels, ~17 ms each, once per tool switch, in a callback that is already waiting
    -- for a repaint. Nowhere near the Tab path and nowhere near note-stepping.
    local edges = {}
    for i, e in ipairs(BOX_EDGES) do
      local p = at(ov, e[1], e[2])
      edges[i] = p and tostring(p.r) or "?"
    end
    -- The legend names the CONTRAST, not two magic numbers: the border read 157 in one capture
    -- and 146 in another, because Melodyne's greys move with its appearance setting (and with
    -- whatever it draws over itself — the first shot was taken behind a detection dialog). What
    -- does not move is that a border is far darker than the 191 of bare tool bar. A threshold
    -- written into this line would have been wrong within four days.
    host.log.info(("[melodyne] fields under tool=%s: left=%q right=%q  box borders %s (dark ~146-157 = box drawn, 191 = bare strip)")
      :format(tostring(toolName or "?"), trimmed(note), trimmed(pitch), table.concat(edges, " ")))
  end)
end

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
