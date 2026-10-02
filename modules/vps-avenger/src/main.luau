-- VPS Avenger overlay — stage 1: the header.
--
-- VPS Avenger draws its whole interface itself and publishes not one accessibility element, in
-- REAPER or in Logic (the tester's probe of 2026-09-27), so everything here is a click at a point
-- and a text read of a region — the knowledge of where, from the avenger_control project
-- (geometry.luau). What this stage covers, on the header row Avenger always shows:
--
--   Preset             the preset's name, read; the first stop, so it is what arriving says
--   Previous / Next    ◀ and ▶, and then the NEW name — once it is on screen, read (O:watch);
--                      with the preset database installed, the database's neighbour as well,
--                      marked, when the read fails or disagrees (presets.luau)
--   Preset info        only with the preset database: the preset's expansion, category,
--                      oscillator tabs and macro names, each marked as the database's
--   Load, Save,        MENU's four items, each chosen in Avenger's own menu once a menu test sees
--   Save as,           it (O:chooseMenuItem); Save's and Initialize's warning box is an overlay of
--   Initialize         its own (dialog.luau)
--   Undo, Redo list    UNDO clicked, or right-clicked for its redo list, whose keys are the list's
--                      while a menu test sees it
--   Zoom               a stepper over Avenger's zoom list: the entry beside the current one is
--                      chosen, and what the zoom field then reads is what is said
--
-- at every zoom from 50 to 200 %, in every DAW daw-hosts has an entry for. Knob values, macros,
-- oscillators, drums and the browser are later stages (TODO.md, "VPS Avenger"). Plug-in only: the
-- project supports no standalone Avenger, and neither does this module yet (TODO.md).
--
-- Coordinates are written as the project wrote them, at 50 % from Avenger's corner; the overlay
-- scales them by the zoom READ off Avenger's zoom field (header.luau), and puts Avenger's corner
-- where geometry.luau says it is on the DAW's panel.

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")
local G = host.include("src/geometry.luau")
local H = host.include("src/header.luau")
local D = host.include("src/dialog.luau")
-- The preset database, an optional dependency: DB.present says whether it is installed.
local DB = host.include("src/presets.luau")

-- The arbiter slot the header and the warning box share: the box, at the dialog layer, takes the
-- keyboard from the header while it is up.
local SLOT = "com.platform.vps-avenger"

-- ── Where Avenger is ─────────────────────────────────────────────────────────
-- Every DAW daw-hosts knows, and no DAW named (docs/daw-hosts.md): on a Mac the DAW's plug-in
-- panel — REAPER's FX chain and floating window, Logic's plug-in window — which daw-hosts places,
-- and on Windows Avenger's own child window.
--
-- That child's class is a JUCE window's: "JUCE_" and a number in hex, which JUCE makes from the
-- clock when a plug-in's code first puts a window on the screen, so it differs from one plug-in —
-- and one load — to the next, and only the shape can be matched. How sure: it is JUCE's own
-- naming, the shape of ON:EAR's JUCE_1a05739a091 in this repository, and Avenger is a JUCE
-- plug-in by the project's account (it names JUCE's hover hit-testing); but nobody has looked at
-- an Avenger in a Windows DAW, so whether its editor is that child, and the only one, is a Windows
-- session's to say (TODO.md). Every JUCE plug-in has such a window, which is why `identify` — the
-- header read, header.luau — says whose it is, there as on a Mac's panel.
local BINDING = O.embedded({
  hosts = daw.all,
  control = { windows = "^JUCE_%x+$" },
  identify = H.identify,
}, { slot = SLOT })

-- Both overlays place their coordinates alike: Avenger's corner on the DAW's panel (the frame,
-- geometry.luau) and the zoom read off its header (the scale, header.luau).
local function placed(ov)
  ov:frame(G.frameOf)
  ov:scale(H.factor)
  return ov
end

-- ── A click of the module's own ──────────────────────────────────────────────
-- ◀ and ▶ are clicked by a custom button rather than a hotspot, because what matters is what the
-- click DID — the name after it — and only the module's own code can watch for that; the redo
-- list's right click, because the module has to know that it opened a popup (header.luau,
-- `carried`). The checks a hotspot's click gets from the runtime are made here the same way: the
-- point placed through the overlay's frame and scale (nothing clicked without a factor), inside the
-- plug-in, and not under another window (host.window.ownsPoint: "a module only needs it directly
-- when it clicks a coordinate itself", docs/api/window.md). Each refusal is said, as the runtime
-- says its own — unless `quiet`, for a click that tidies up after something the runtime has
-- already said. A click from a hotkey would inherit held modifiers on Windows; these controls have
-- none, and Return and Space arrive with none.
local function clickAt(o, label, at, button, quiet)
  local function refuse(said)
    if not quiet then host.speech.output(said, { interrupt = true }) end
    return false
  end
  local x, y = o:toScreen(at[1], at[2])
  if not x then
    host.log.info(string.format("[avenger] '%s' cannot be placed now — %s; nothing clicked", label, tostring(y)))
    return refuse(label .. " is not available now")
  end
  local origin = o:origin()
  local b = origin and origin.bounds
  if not (b and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h) then
    host.log.info(string.format("[avenger] '%s' resolves to (%d,%d), outside the plug-in — not clicking", label, x, y))
    return refuse(label .. " is not where it should be")
  end
  local ok, mine = pcall(host.window.ownsPoint, origin.id, x, y)
  if ok and mine == false then
    host.log.info(string.format("[avenger] '%s' at (%d,%d): another window is drawn there — not clicking", label, x, y))
    return refuse("something else is covering " .. label)
  end
  host.log.info(string.format("[avenger] '%s': clicking (%d,%d)%s", label, x, y,
    button and (" with the " .. button .. " button") or ""))
  if button then
    host.input.click(x, y, { button = button })
  else
    host.input.click(x, y)
  end
  return true
end

-- ── A popup left open ────────────────────────────────────────────────────────
-- A choice the runtime gave up on AFTER the popup was seen — it could not be placed, something
-- covers the item, the item fell outside it, or the popup did not take the click — leaves
-- Avenger's popup open, and while a menu test sees it the keys are the popup's: the Zoom control's
-- Left and Right would go to the list, and the user was only told the choice failed. So it is
-- closed the way the project closes the zoom list, by clicking its opener again — a JUCE popup is
-- modal, and a click outside it dismisses it — and only while the menu still counts as open, so
-- that a popup already gone is not opened again. Quiet: the runtime has said what went wrong, and
-- the menu tests say whether the popup has gone (`[menu] … the menu has closed`). Whether that
-- click closes MENU's popup as it closes the zoom list is not measured (TODO.md).
local CLOSE_AFTER = { unplaced = true, covered = true, untaken = true, due = true, refused = true }

local function closeLeftOpen(w, pick, why, label, opener)
  if not (pick.seen and CLOSE_AFTER[why] and w:menuOpen()) then return end
  host.log.info(string.format("[avenger] '%s': its popup was left open (%s) — clicking its opener "
    .. "again to close it", label, tostring(why)))
  clickAt(w, label, opener, nil, true)
end

-- ── The preset's name, after ◀ or ▶ ──────────────────────────────────────────
-- Said once it has CHANGED on screen: O:watch reads it again and again until it differs from the
-- name read just before the click, and says what it reads then. The project's tool says the name
-- its database predicts instead, without reading; this module says what the plug-in did. With
-- the preset database installed, the database's neighbour is said as well — marked, and only
-- when the read failed or disagrees with it (presets.luau, "THE NEIGHBOUR RULE"): the watch, and
-- what it waits for, are the same with it and without it.
--
-- The readings after the click are made off the event loop (host.ocr.read), one out at a time:
-- the watch looks every 100 ms and is handed the last answer that belongs to this press. The one
-- before the click is made on the loop, because it has to be the name before the click and a read
-- that answers later cannot be: one read per press, as the Preset read-out makes one per focus.
-- When that read gets nothing — small text at 50 % — the name before is the last one this module
-- read, and with none at all, nothing is taken for a change: the watch runs to its bound and says
-- the last name it read, without saying it is new. The first reading after the click is taken
-- right after it, before Avenger has redrawn, so "whatever differs from nothing" would be the OLD
-- name said as the new one.
--
-- NAME_WITHIN bounds the wait, it does not decide anything: the project measured Avenger's redraw
-- at about 150 ms, up to 500, and a read on a Mac adds up to a quarter of a second. A name that
-- has not changed by then is said as it is, "unchanged" — ◀ at the first preset, or a click the
-- DAW swallowed (Logic takes the first click after activation for itself, by the project's
-- account: unmeasured, TODO.md).
local NAME_WITHIN = 2000

-- `pending`: the press whose watch is still out, or nil once it has ended (or the overlay has
-- left the front, which ends a watch without a word). Only the database's rule asks it: a press
-- made while it is set may read the name from before that press's click (presets.luau).
local names = { gen = 0, reading = false, text = nil, textGen = -1, last = nil, pending = nil }

local function tidy(text)
  local t = string.gsub(text or "", "%s+", " ")
  t = string.gsub(t, "^ ", "")
  return (string.gsub(t, " $", ""))
end

-- The name as read now, on the event loop, or nil.
local function nameNow(o)
  local r = o:toScreenRect(G.NAME_REGION)
  if not r then return nil end
  local ok, res = pcall(host.ocr.recognize, { region = r, lang = H.lang() })
  local t = ok and res and tidy(res.text) or ""
  if t == "" then return nil end
  names.last = t
  return t
end

-- The last name read for press `gen`, and a new read started when none is out.
local function nameAfter(o, gen)
  if gen ~= names.gen then return nil end
  if not names.reading then
    local r = o:toScreenRect(G.NAME_REGION)
    if r then
      names.reading = true
      local ok = pcall(host.ocr.read, r, { lang = H.lang(), key = "vps-avenger name" }, function(res)
        names.reading = false
        if gen ~= names.gen or res.status ~= "text" then return end
        local t = tidy(res.text)
        if t ~= "" then names.text, names.textGen, names.last = t, gen, t end
      end)
      if not ok then names.reading = false end
    end
  end
  return names.textGen == gen and names.text or nil
end

-- With the database: what DB.after says for the watch's outcome, or nil for what the module says
-- without it. Only for the latest press — a watch another press has overtaken reads nothing more
-- (nameAfter) and runs out, and its outcome is no evidence about the preset now shown — and
-- guarded, because the watch swallows an error in its callbacks, and the announcement with it.
local function decided(step, gen, now, unchanged)
  if not step or gen ~= names.gen then return nil end
  local ok, said = pcall(DB.after, step, now, unchanged)
  if ok then return said end
  host.log.info("[avenger] the preset database's rule failed: " .. tostring(said) .. " — said without it")
  return nil
end

local function settled(gen)
  if names.pending == gen then names.pending = nil end
end

local function presetStep(ov, label, at, dir)
  ov:addCustomButton({
    label = label,
    onActivate = function(o)
      local read = nameNow(o)
      local was = read or names.last
      if not clickAt(o, label, at) then return end
      -- The name read before the click places the preset the step starts from; worked out once
      -- the click is sent, so that the first use's index load never holds the click back.
      local step = nil
      if DB.present then
        local ok, s = pcall(DB.before, label, read, dir, names.pending ~= nil)
        if ok then
          step = s
        else
          host.log.info("[avenger] the preset database's rule failed: " .. tostring(s) .. " — the step goes without it")
        end
      end
      names.gen += 1
      local gen = names.gen
      names.pending = gen
      o:watch({
        read = function(w) return nameAfter(w, gen) end,
        was = was,
        done = function(now, before) return before ~= nil and now ~= before end,
        within = NAME_WITHIN,
        onDone = function(now)
          settled(gen)
          host.speech.output(decided(step, gen, now, false) or now, { interrupt = true })
        end,
        onGiveUp = function(now, before)
          settled(gen)
          local said = decided(step, gen, now, now ~= nil and now == before)
          if said then
            host.speech.output(said, { interrupt = true })
          elseif now == nil then
            host.speech.output("the preset name cannot be read now", { interrupt = true })
          elseif now == before then
            host.speech.output(now .. ", unchanged", { interrupt = true })
          else
            host.speech.output(now, { interrupt = true })
          end
        end,
      })
    end,
  })
end

-- ── Preset info ──────────────────────────────────────────────────────────────
-- Only with the preset database: the project's Info. Pressed, it reads the name as a step does
-- before its click — on the event loop, one read per press — and says what the database has on
-- it (presets.luau, DB.info): its expansion, category, oscillator tabs and macro names, each part
-- marked as the database's. A button rather than a read-out: the whole of it is a long sentence
-- nobody wants on every Tab past it.
local function presetInfo(ov)
  ov:addCustomButton({
    label = "Preset info",
    onActivate = function(o)
      local read = nameNow(o)
      local ok, said = pcall(DB.info, read)
      if not ok then
        host.log.info("[avenger] 'Preset info' failed: " .. tostring(said))
        said = "the preset database cannot be read"
      end
      host.speech.output(said, { interrupt = true })
    end,
  })
end

-- ── MENU's items ─────────────────────────────────────────────────────────────
-- Chosen in Avenger's own menu, which JUCE draws as a popup window of its own: MENU is clicked,
-- and the item once the menu test (newWindow, below) sees the popup and says where it is — never
-- after a wait — and the choice counts once the popup has taken it (O:chooseMenuItem).
--
-- The item is placed from Avenger's corner, where the project's figures are, across the middle of
-- the popup's own width, in screen pixels at the factor MENU was clicked at (the runtime hands it
-- over); and only in a popup that opened where those figures hold — its top below MENU's middle
-- and above Load's — since JUCE moves a popup that has no room below its opener, and a row counted
-- from Avenger's corner is then another row. Such a popup is not chosen in, and the log says where
-- it was. MENU sits at the top of Avenger, whose own height lies below it, so that is not expected
-- to happen; the menu item shot of a calibrating run shows where the rows really are, and the log
-- line here gives the popup's place in the table's units, so the offsets can be written from the
-- popup's corner once it has been seen (TODO.md). A window under two rows tall is no menu of four
-- items — a tooltip under the pointer is a window too, on a Mac — and the runtime is told to look
-- at the next answer instead (`false`).
local function menuItemAt(label, item, pick)
  return function(o, m, k)
    local origin = o:origin()
    if not (k and origin) then return nil end
    if m.h < 2 * G.ZOOM_LIST_PITCH * k then return false, "under two rows tall" end
    pick.seen = true
    local cx, cy = G.place(origin, k, 0, 0)
    local _, yOpen = G.place(origin, k, G.MENU[1], G.MENU[2])
    local _, yFirst = G.place(origin, k, G.LOAD[1], G.LOAD[2])
    local _, y = G.place(origin, k, item[1], item[2])
    local at = string.format("(%.1f,%.1f) %.1fx%.1f from Avenger's corner in 50 %% units",
      (m.x - cx) / k, (m.y - cy) / k, m.w / k, m.h / k)
    if m.y < yOpen or m.y > yFirst then
      host.log.info(string.format("[avenger] '%s': MENU's popup opened at %s — not below MENU, where "
        .. "the table's rows are; nothing chosen", label, at))
      return nil
    end
    host.log.info(string.format("[avenger] '%s': MENU's popup at %s; the item %.1f below its top, "
      .. "across its middle", label, at, (y - m.y) / k))
    return { m.w / 2, y - m.y }
  end
end

-- Whether Avenger's warning box, opened by `label` now, would lie off the screen: the box's band
-- (geometry.luau), placed, against the primary display's size — the only one the host names —
-- and only while Avenger's corner is on that display. Avenger at 150 % and more is taller than a
-- 1080-point screen, and a Mac window can reach past its bottom edge; the box is drawn in
-- Avenger's middle, so it would be modal where no read of ours, and no key, reaches it. The band,
-- the zoom and the screen, or nil when it is on screen or cannot be told.
local function boxOffScreen(o)
  local origin = o:origin()
  local r = o:toScreenRect(G.DIALOG_BAND)
  local ok, s = pcall(host.screen.size)
  if not (origin and r and ok and type(s) == "table" and type(s.w) == "number" and type(s.h) == "number") then
    return nil
  end
  local c = origin.client
  if c.x < 0 or c.y < 0 or c.x >= s.w or c.y >= s.h then return nil end
  if r[3] <= s.w and r[4] <= s.h then return nil end
  return r, H.zoom(origin), s
end

-- What each item leaves on screen, for the preset database (presets.luau, DB.forget): after Load
-- and Save as, most likely the user's own file, whose name is not looked for in the catalog by
-- similarity (the project's tool skips its snap there too); after Initialize, Avenger's blank
-- preset; Save keeps the name, so whether it is the user's own stays as it was.
local OWN_NAME = { ["Load preset"] = true, ["Save preset as"] = true, ["Initialize preset"] = false }

-- `arms`: the item opens Avenger's warning box, which is looked for once the choice has been
-- taken.
local function menuEntry(ov, label, item, arms)
  ov:addCustomButton({
    label = label,
    onActivate = function(o)
      if arms then
        local r, zoom, s = boxOffScreen(o)
        if r then
          host.log.info(string.format("[avenger] '%s' not chosen: Avenger's warning box would lie at "
            .. "(%d,%d)-(%d,%d), past the screen's %dx%d", label, r[1], r[2], r[3], r[4], s.w, s.h))
          host.speech.output(string.format("%s: Avenger's warning box would open off the screen%s — "
            .. "zoom out first", label, zoom and (" at " .. zoom .. " percent") or ""), { interrupt = true })
          return
        end
      end
      local origin = o:origin()
      local id = origin and origin.id
      local pick = {}
      local opened = o:chooseMenuItem({
        label = label,
        at = G.MENU,
        menuItem = menuItemAt(label, item, pick),
        onDone = function(w, chosen, why)
          if chosen then
            -- Every item may load, rename, overwrite or blank the preset: the database's position
            -- is not Avenger's any more.
            DB.forget(label .. " chosen", OWN_NAME[label])
            if arms then D.arm(id, label) end
            return
          end
          closeLeftOpen(w, pick, why, label, G.MENU)
        end,
      })
      -- What a hotspot says on its click: MENU was clicked. What the item did is the plug-in's to
      -- show — a file dialog, or the warning box's own overlay.
      if opened then
        H.popup(origin)
        host.speech.output(label .. ", activated", { interrupt = true })
      end
    end,
  })
end

-- ── Zoom ─────────────────────────────────────────────────────────────────────
-- A stepper over Avenger's zoom list. Left and Right open the list and choose the entry beside the
-- current zoom — the zoom READ off the zoom field — and the stepper then says what the zoom field
-- reads once it shows another zoom: the runtime watches the stepper's text for that, up to
-- `settle`, and the text is what the field reads (header.luau reads it again after the choice,
-- until it changes). At 50 or 200 % there is nothing beside it, and nothing is opened; with the
-- zoom not known (the field unread on a Windows display whose scaling is not known) neither.
--
-- The entry is placed by the list's own height, not from Avenger's corner. The list has 21 equal
-- rows, drawn at the zoom it opened at (geometry.luau), so its rows are 11.5 × the factor the zoom
-- field was clicked at apart, and whatever the popup has beyond them is its border, the same above
-- and below: entry t lies (height − 21 rows) / 2 + (t − ½) rows down from the popup's top,
-- wherever JUCE put the popup — below the field, or moved up because it had no room there, which
-- at high zooms it will not. A popup too short to hold 21 rows scrolls, and a row counted in it
-- would be another zoom, so nothing is chosen in it; one under three rows tall is no zoom list at
-- all (a tooltip), and the runtime looks at the next answer. Where the project's figures put the
-- entry (from Avenger's corner) is logged beside it, to check the two against each other; x is the
-- middle of the popup.
--
-- One step at a time: a press while a step is still being carried out — its list not seen yet,
-- or the zoom field not read again since — sends nothing and says nothing of its own (`false`
-- from onStep); the step in flight is announced. Held down, the arrow moves the zoom one entry per
-- step the plug-in has taken, not one per repeat of the key.
local ZOOM_SETTLE = 3000
-- What `settle` is set to when a step opens nothing, so the stepper says the zoom at once rather
-- than after waiting for a change that is not coming (the runtime reads it at each step).
local ZOOM_AT_ONCE = 1

local function zoomEntry(t, from, to, pick)
  return function(o, m, k)
    local origin = o:origin()
    if not (k and origin) then return nil end
    local row = G.ZOOM_LIST_PITCH * k
    local n = #G.ZOOMS
    if m.h < 3 * row then return false, "under three rows tall" end
    pick.seen = true
    if m.h < (n - 0.5) * row or m.h > (n + 4) * row then
      host.log.info(string.format("[avenger] 'Zoom': the popup is %d tall where the list's %d rows are "
        .. "%.0f — %s; nothing chosen", m.h, n, n * row,
        m.h < (n - 0.5) * row and "cut short, so it scrolls" or "not the zoom list"))
      return nil
    end
    local dy = (m.h - n * row) / 2 + row * (t - 0.5)
    local cx, cy = G.place(origin, k, 0, 0)
    local _, ty = G.place(origin, k, G.ZOOM_LIST_FIRST[1], G.ZOOM_LIST_FIRST[2] + G.ZOOM_LIST_PITCH * (t - 1))
    host.log.info(string.format("[avenger] 'Zoom': %d %% to %d %%, entry %d of the list at (%.1f,%.1f) "
      .. "%.1fx%.1f from Avenger's corner in 50 %% units: at y %.0f by the list's height, %d by the table",
      from, to, t, (m.x - cx) / k, (m.y - cy) / k, m.w / k, m.h / k, m.y + dy, ty))
    return { m.w / 2, dy }
  end
end

local function zoomStepper(ov)
  local ctl
  ctl = ov:addStepper({
    label = "Zoom",
    settle = ZOOM_SETTLE,
    -- nil only while a step of ours waits for the zoom field to show the new zoom, which is what
    -- the runtime's watch waits through; otherwise the zoom, or that it cannot be read now. Asked
    -- first either way: asking is what reads the field again while a step waits.
    text = function(o)
      local origin = o:origin()
      local z = H.zoom(origin)
      if H.stepping(origin) then return nil end
      if z then return z .. " percent" end
      return "cannot be read now"
    end,
    onStep = function(dir, o)
      local origin = o:origin()
      if H.stepping(origin) then
        host.log.info("[avenger] 'Zoom': the last step still waits for the zoom field — this press is not sent")
        return false
      end
      local z = origin and H.zoom(origin) or nil
      local i = z and G.zoomIndex(z)
      local to = i and G.ZOOMS[i + dir]
      if not to then
        ctl.settle = ZOOM_AT_ONCE
        host.log.info(string.format("[avenger] 'Zoom': no step %s from %s — nothing opened",
          dir < 0 and "down" or "up", z and (z .. " %") or "a zoom that is not known"))
        return
      end
      ctl.settle = ZOOM_SETTLE
      local pick = {}
      local opened = o:chooseMenuItem({
        label = "Zoom",
        at = G.ZOOM_FIELD,
        menuItem = zoomEntry(i + dir, z, to, pick),
        onDone = function(w, chosen, why)
          if chosen then
            H.stepped(w:origin() or origin, z)
            return
          end
          -- A press dropped while this one's list is awaited ends nothing of this one's.
          if why == "busy" then return end
          H.zoomOver()
          closeLeftOpen(w, pick, why, "Zoom", G.ZOOM_FIELD)
        end,
      })
      -- Not opened: a press while the last one's list is still awaited ("pressed again", logged by
      -- the runtime), or one the runtime refused and said. Nothing is watched for it.
      if not opened then return false end
      H.popup(origin, true)
    end,
  })
  return ctl
end

-- ── The header overlay ───────────────────────────────────────────────────────
local header = placed(O.new("VPS Avenger"))
-- The first stop: arriving says the preset. Read, never clicked: pressing it reads it again. An
-- empty read is the recogniser's failure — Avenger always shows a name — and is said as that.
header:addOCRButton({ label = "Preset", region = G.NAME_REGION, readOnly = true, fallback = "cannot be read now" })
presetStep(header, "Previous preset", G.PREV, -1)
presetStep(header, "Next preset", G.NEXT, 1)
if DB.present then presetInfo(header) end
menuEntry(header, "Load preset", G.LOAD, false)
menuEntry(header, "Save preset", G.SAVE, true)
menuEntry(header, "Save preset as", G.SAVE_AS, false)
menuEntry(header, "Initialize preset", G.INITIALIZE, true)
header:addHotspotButton({ label = "Undo", at = G.UNDO })
-- The redo list is Avenger's own popup, which the project found VoiceOver could step through with
-- the arrow keys: opened with a right click on UNDO, and while a menu test sees it the keys are the
-- list's (`opensMenu`: the runtime's press, which newWindow compares against, comes before the
-- click). Whether NVDA reads it, and whether VoiceOver does from here, is a session's question
-- (TODO.md). UNDO's crosshair in a calibration shot is Undo's, the same point.
header:addCustomButton({
  label = "Redo list",
  opensMenu = true,
  onActivate = function(o)
    if clickAt(o, "Redo list", G.UNDO, "right") then
      H.popup(o:origin())
      -- What the list redoes is chosen in the list, which this module does not read.
      DB.forget("the redo list opened")
      host.speech.output("Redo list, activated", { interrupt = true })
    end
  end,
})
zoomStepper(header)
-- Away from the overlay, the preset can be changed in ways it does not see — the DAW's own
-- preset menu, a file dialog, another project: the database's position goes with it.
header:onDeactivate(function()
  H.left()
  -- A watch out now ends without a word (O:watch looks only while the overlay is up).
  names.pending = nil
  DB.forget("the overlay left the front")
end)
-- `newWindow`: Avenger's popups — MENU, the zoom list, the redo list — are JUCE windows of the
-- DAW's process (the process Avenger runs in; a DAW that runs plug-ins apart is the popup's owner
-- then, unmeasured), which is what this test sees, and says where, so an item can be chosen in
-- one. Not `nativePopup`: JUCE draws its menus itself rather than as the system's, and a test that
-- says only "a menu is open" places no item.
header:bind(BINDING, { specificity = O.layer.base, menus = { O.menuTests.newWindow } })

-- ── The warning box ──────────────────────────────────────────────────────────
-- Its own overlay on the same slot and binding, at the dialog layer: the box's text, YES and NO.
-- `pollMatch`: the box comes and goes inside Avenger with no window event, so its gate is asked
-- every half second — and only while a Save or Initialize of this module is waiting for it
-- (`pollWhen`, dialog.luau). `resume(false)`: the box comes up in the same window every time, so
-- the runtime would resume on the button the last box was answered with; every box starts on its
-- own text, and a user who counts stops from there lands where they expect.
local box = placed(O.new("VPS Avenger warning"))
D.build(box)
box:gate(D.gate)
box:resume(false)
box:bind(BINDING, { specificity = O.layer.dialog, pollMatch = 500, pollWhen = D.expecting })

host.log.info("[avenger] VPS Avenger overlays ready: the header, and Avenger's warning box.")
