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
local TOOLS = {
  { name = "Main", key = "F1", at = { 84, 59 } },
  { name = "Pitch", key = "F2", at = { 100, 60 }, variants = {
      { name = "Pitch modulation", drag = 24, probes = { { 107, 68 }, { 107, 62 } } },
      { name = "Pitch drift", drag = 48, probes = { { 111, 62 }, { 112, 69 } } },
    } },
  { name = "Formant", key = "F3", at = { 129, 59 } },
  { name = "Amplitude", key = "F4", at = { 153, 60 }, variants = {
      { name = "Sibilant balance", drag = 48, probes = { { 151, 67 } } },
    } },
  { name = "Time", key = "F5", at = { 175, 60 }, variants = {
      { name = "Attack speed", drag = 48, probes = { { 177, 62 }, { 177, 72 } } },
    } },
  -- The draft flags this one as unverified: drag 24 is a guess at the next flyout slot, and
  -- SIBIAC never mapped its probes. Carried over with the doubt intact — with no probes the
  -- detection below simply reports the base tool rather than inventing a variant.
  { name = "Note separation", key = "F6", at = { 200, 60 }, variants = {
      { name = "Separation Type", drag = 24, probes = {} },
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

-- ── Selecting a variant: press, wait, glide, settle, release ──────────────────
-- Melodyne shows the flyout only while the icon is HELD, so a drag that presses and moves in
-- the same breath misses it — the draft records that as tried and not working.
--
-- Composed from timers rather than done in one blocking call, and that is the whole reason
-- `host.input.mouseDown` / `mouseUp` exist as separate primitives now. The gesture takes the
-- better part of a second; performing it synchronously would stop speech, hotkeys and detection
-- for that long. Between the steps the pump keeps running.
local HOLD, GLIDE_STEPS, GLIDE_MS, SETTLE = 350, 12, 20, 150

local function selectVariant(o, tool, target)
  local origin = o:origin()
  if not (origin and origin.client) then return end
  local x, y = origin.client.x + tool.at[1], origin.client.y + tool.at[2]
  if target == 1 then
    host.input.click(x, y)
    variant = 1
    return
  end
  local drag = tool.variants[target - 1].drag
  local pinned = o:hwnd()
  host.input.mouseDown(x, y)
  -- Every continuation re-checks that the overlay is still on the same window, exactly as the
  -- hotspot sequences and the panel reveal do: a gesture that spans a second can otherwise
  -- finish its glide inside whatever the user switched to.
  host.timer.after(HOLD, function()
    if not (o.active and o:hwnd() == pinned) then return host.input.mouseUp(x, y) end
    local step = 0
    local glide
    glide = function()
      if not (o.active and o:hwnd() == pinned) then return host.input.mouseUp(x, y) end
      step = step + 1
      host.input.move(x, y + math.floor(drag * step / GLIDE_STEPS))
      if step < GLIDE_STEPS then
        host.timer.after(GLIDE_MS, glide)
      else
        host.timer.after(SETTLE, function() host.input.mouseUp(x, y + drag) end)
      end
    end
    glide()
  end)
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
    -- Not "tab control": mechanically that is exactly what it is, and saying so tells the user
    -- the wrong thing about what they are looking at. Celemony calls it the Toolbox;
    -- "horizontal tool bar" names it and says which way the arrows go.
    typeLabel = "horizontal tool bar",
    current = function(o)
      local i = activeTool(o)
      if not i then return nil end
      -- Re-detected rather than remembered, so a variant the user picked inside Melodyne is
      -- reported too, not only one we selected ourselves.
      variant = activeVariant(o, TOOLS[i])
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

local function captureAltTap(o)
  o._altTap = host.keys.capture("Alt tap", function() engageMenuBar(o) end)
end
ov:onDeactivate(function(o)
  if o._altTap then
    host.keys.release(o._altTap)
    o._altTap = nil
  end
end)

-- ── What Melodyne is busy with ────────────────────────────────────────────────
-- Melodyne analyses audio before it can be edited, and while it does it shows a progress disc
-- in the middle of the editor with the name of the pass under it ("Polyphonic Detection"). For
-- a sighted user that is unmissable; without it, a Melodyne that is merely BUSY is
-- indistinguishable from one that is broken — which is exactly how it was reported: "either the
-- audio is still loading, or the hotkeys are not getting through".
--
-- The LABEL rather than the percentage, and that is a decision rather than a shortcut. The disc
-- fills as a pie, so a percentage means knowing its centre and radius; measured from the one
-- capture available, the grey-pixel search caught the waveform and the caption too and returned
-- a radius half again too large, and the ring samples run 90 to 250 because the caption crosses
-- them. Pinning that needs two captures at different points of the same pass, and inventing it
-- from one would be the same mistake this project keeps paying for.
--
-- Centred on the CLIENT AREA rather than written down: the disc sits in the middle of the
-- editor, so its position follows the window's size.
local BUSY_W, BUSY_H = 190, 26

local function busyText(o)
  local origin = o:origin()
  if not (origin and origin.client) then return nil end
  local w, h = O.contentSize(origin)
  if w == 0 or h == 0 then return nil end
  local cx, cy = origin.client.x + w // 2, origin.client.y + h // 2
  local r = host.ocr.recognize({
    region = { cx - BUSY_W // 2, cy - BUSY_H // 2, cx + BUSY_W // 2, cy + BUSY_H // 2 },
  })
  if not (r and r.text and r.text ~= "") then return nil end
  return r.text
end

ov:addCustomButton({
  label = "Analysis",
  hotkey = "Alt+A",
  text = function(o) return busyText(o) or "idle" end,
  -- Nothing to press. Announced as a plain read-out rather than a button, because a control
  -- must not offer an affordance it does not have.
  readOnly = true,
  onActivate = function(o)
    host.speech.output(busyText(o) or "Melodyne is idle", { interrupt = true })
  end,
})

-- Said on arrival, unprompted, because that is when it matters: a user tabbing into a Melodyne
-- that is still analysing needs to know before they wonder why nothing responds.
ov:onActivate(function(o)
  captureAltTap(o)
  host.timer.after(400, function()
    if not o.active then return end
    local busy = busyText(o)
    if busy then host.speech.output("Busy: " .. busy, { interrupt = false }) end
  end)
end)

-- The two read-outs in the tool strip (SIBIAC's Edit1 and Edit2). `readOnly`, because
-- activating them does nothing in Melodyne and a control must not announce itself as a button
-- it cannot honour — the rule this project arrived at through a "button" that only reported.
local NOTE_REGION = { 220, 61, 287, 74 }
local PITCH_REGION = { 300, 61, 367, 74 }

ov:addOCRButton({ label = "Selected note", region = NOTE_REGION, readOnly = true })
ov:addOCRButton({ label = "Pitch deviation", region = PITCH_REGION, readOnly = true })

-- ── Announcing a new selection without being asked ────────────────────────────
-- Selecting a different note changes both read-outs, and a user working in the editor should
-- not have to tab away to find out what they just landed on.
--
-- Watched rather than triggered by the keys that do it, which was the first idea and the wrong
-- one: those keys are Melodyne's, they pass through untouched by design, and adding a way to
-- observe them would put a second mechanism in the way of the first. Reading the result is both
-- simpler and truer — it catches a selection made with the mouse, or by Melodyne itself, which
-- watching our own keys never would.
--
-- ONE announcement for BOTH fields. They change together, so a watcher per field would say the
-- note and then say the deviation as two separate interruptions for a single event.
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
local watchRegion = { NOTE_REGION[1], NOTE_REGION[2], PITCH_REGION[3], PITCH_REGION[4] }
local lastSeen = nil

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
  local note, pitch = field(NOTE_REGION), field(PITCH_REGION)
  return note .. "|" .. pitch, note, pitch
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
    -- Forget on the way out, so returning to a Melodyne that changed while we were away
    -- announces the new selection instead of staying silent about it.
    lastSeen = nil
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
  if lastSeen == nil then
    -- The first read after arriving establishes the baseline rather than announcing it: the
    -- overlay already says what is focused, and a second sentence on top of that is noise.
    lastSeen = combined
    return
  end
  if combined == lastSeen then return end
  lastSeen = combined
  local parts = {}
  if note ~= "" then parts[#parts + 1] = note end
  if pitch ~= "" then parts[#parts + 1] = pitch end
  if #parts > 0 then
    -- One line per change, kept permanently: it is what turned "only the cents are announced"
    -- from three indistinguishable guesses into one reading.
    host.log.info(("[melodyne] selection: note=%q deviation=%q"):format(note, pitch))
    host.speech.output(table.concat(parts, ", "), { interrupt = true })
  end
end)

-- `menus = true`: Melodyne has a native menu bar, and while a menu is open the captured
-- navigation keys have to reach it rather than steer the overlay.
ov:attach(melodyne, { menus = true })

host.log.info("Melodyne: standalone overlay ready — tool bar, variants, menu bar, 2 read-outs.")
