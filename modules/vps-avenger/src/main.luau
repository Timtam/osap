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
--                      marked, when the read fails or disagrees (presets.luau); unavailable
--                      where the name read is Init Preset, which Avenger steps no preset from
--   Preset info        only with the preset database: the preset's expansion, category,
--                      oscillator tabs and macro names, said plainly (presets.luau)
--   Load, Save,        MENU's four items, each chosen in Avenger's own menu once a menu test sees
--   Save as,           it (O:chooseMenuItem); Save's and Initialize's warning box is an overlay of
--   Initialize         its own (dialog.luau); after Load's and Save as's file dialog, the name
--                      read and said once Avenger is in front again (O:afterDialog)
--   Undo, Redo list    UNDO clicked, or right-clicked for its redo list, whose keys are the list's
--                      while a menu test sees it
--   Zoom               a stepper over Avenger's zoom list: the entry beside the current one is
--                      chosen, and what the zoom field then reads is what is said
--
-- and after it the oscillator strip and the macros (ring.luau): the tab selected, its Mute and
-- Solo, Add oscillator, the selected oscillator's knobs reset, the macro tab, its knobs reset and
-- its buttons — and on the Drums tab its pattern, mode and lock, the slot selected and its Mute and
-- Solo, and the central tab, with DRUM SQ's patterns once DRUM SQ reads as the central tab — each
-- saying what was read, and clicked as the page's trips click them (act.luau)
--
-- at every zoom from 50 to 200 %, in every DAW daw-hosts has an entry for. Turning knobs from the
-- ring and the browser are later stages (TODO.md, "VPS Avenger"); the sample editor is the page's
-- only. Plug-in only: the project supports no standalone Avenger, and neither does this module yet
-- (TODO.md).
--
-- And the project's panel as a page of this module's own (page.luau, ui/), on the overlay
-- runtime's page support (O:addPage): its key on an identified Avenger reads Avenger off one
-- picture (read/open.luau) and shows the page beside it; the header's actions are trips into
-- Avenger made with this file's own clicks, reads and choices (actions.luau), the renames and
-- Browse Presets hand-overs of the keyboard to Avenger (handover.luau), and the knobs turned with
-- a press held through a hand-over of the keys (amount.luau); the page's other actions come in
-- later stages, and say so until then. While the page is hidden, DRUM SQ's watcher says the pattern
-- AUTO FOLLOW moves to (watch.luau).
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
-- The project's reading of what a recogniser makes of Avenger's text.
local R = host.include("src/read/ocr.luau")
local Page = host.include("src/page.luau")
local Ring = host.include("src/ring.luau")
-- What the page is told; here only that the preset may have changed (S.presetMayHaveChanged).
local S = host.include("src/state.luau")
-- Where Avenger's popups are painted, for the drawn menu test (below, the binding).
local Popups = host.include("src/popups.luau")

-- The arbiter slot the header and the warning box share: the box, at the dialog layer, takes the
-- keyboard from the header while it is up.
local SLOT = "com.platform.vps-avenger"

-- ── Where Avenger is ─────────────────────────────────────────────────────────
-- Every DAW daw-hosts knows, and no DAW named (docs/daw-hosts.md): on a Mac the DAW's plug-in
-- panel — REAPER's FX chain and floating window, Logic's plug-in window — which daw-hosts places,
-- and on Windows Avenger's own child window.
--
-- That child's class is not a JUCE window's, as this module first assumed: the first Avenger seen
-- in a Windows DAW (2026-10-30, the VST3 in REAPER's FX chain, the window probe's lines) had
-- KalliGuiWin18360-000000000D72C850, inside REAPER's reaperPluginHostWrapProc, and the keyboard
-- in it, so "^JUCE_%x+$" never matched and the overlay never came up. Only the prefix is matched:
-- the number and the hex after it look like a build and an address, which a second load or
-- release need not repeat. Any plug-in made with the same toolkit has such a window, which is why
-- `identify` — the header read, header.luau — says whose it is, there as on a Mac's panel.
local BINDING = O.embedded({
  hosts = daw.all,
  control = { windows = "^KalliGuiWin" },
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
-- click DID — the name after it — and only the module's own code can watch for that; UNDO's,
-- because an undo may change the preset, and only the module's own code can forget then what it
-- read of it; the redo list's right click, because the module has to know that it opened a popup
-- (header.luau, `carried`). The checks a hotspot's click gets from the runtime are made here the same way: the
-- point placed through the overlay's frame and scale (nothing clicked without a factor), inside the
-- plug-in, and not under another window (host.window.ownsPoint: "a module only needs it directly
-- when it clicks a coordinate itself", docs/api/window.md). Each refusal is said, as the runtime
-- says its own — unless `quiet`, for a click that tidies up after something the runtime has
-- already said, and for a trip of the page's, which shows the refusal in the page instead (it is
-- handed back with the `false`). A click from a hotkey would inherit held modifiers on Windows;
-- these controls have none, and Return and Space arrive with none.
--
-- Screen point (x, y) checked as above — what the browser clicks where it READ a word, never a table
-- point (src/actions/browser.luau): x and y, or nil and what was said (or would have been, `quiet`).
local function pointAt(o, label, x, y, quiet)
  local function refuse(said)
    if not quiet then host.speech.output(said, { interrupt = true }) end
    return nil, said
  end
  local origin = o:origin()
  local b = origin and origin.bounds
  if not (b and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h) then
    host.log.info(string.format("[avenger] '%s' resolves to (%d,%d), outside the plug-in — not clicking", label, x, y))
    return refuse(label .. " is not where it should be")
  end
  local ok, mine = pcall(host.window.ownsPoint, origin.id, x, y)
  if ok and mine == false then
    host.log.info(string.format("[avenger] '%s' at (%d,%d): another window is drawn there, or the window is gone — not clicking", label, x, y))
    return refuse("something else is covering " .. label .. ", or its window is gone")
  end
  return x, y
end

-- Where table point `at` lies on screen, checked as above: x and y, or nil and what was said (or
-- would have been, `quiet`).
local function pointOf(o, label, at, quiet)
  local x, y = o:toScreen(at[1], at[2])
  if not x then
    host.log.info(string.format("[avenger] '%s' cannot be placed now — %s; nothing clicked", label, tostring(y)))
    local said = label .. " is not available now"
    if not quiet then host.speech.output(said, { interrupt = true }) end
    return nil, said
  end
  return pointAt(o, label, x, y, quiet)
end

-- A click at screen point (x, y), checked by pointAt: true, or false and what was said.
local function clickPoint(o, label, x, y, button, quiet)
  local px, py = pointAt(o, label, x, y, quiet)
  if not px then return false, py end
  host.log.info(string.format("[avenger] '%s': clicking (%d,%d)%s", label, px, py,
    button and (" with the " .. button .. " button") or ""))
  if button then
    host.input.click(px, py, { button = button })
  else
    host.input.click(px, py)
  end
  return true
end

-- O.doubleClick at screen point (x, y) with `button` (nil: the left), its answers as this file's: false
-- and what was said where the host refused it at the call — nothing goes out, and `after` is not
-- called — otherwise true, and `after(true)`, as a handler, once both clicks have gone out, or
-- `after(false, said)` where the gesture stopped after the call: since overlay runtime 0.15 the
-- host makes the double-click as one gesture on a thread of its own (host.input.click's `clicks =
-- 2`), and its callback's `false, err` may come from either half — the window at the point changed
-- before the second (`covered`), or a click the system did not take; the host's `[input]
-- double-click` line says which. Each refusal is said unless `quiet`.
local function doubled(label, x, y, button, quiet, after)
  local function refuse(err)
    local said = label .. " was not clicked: " .. tostring(err)
    if not quiet then host.speech.output(said, { interrupt = true }) end
    return said
  end
  local refused = false
  local ok, err = O.doubleClick(x, y, button and { button = button } or nil, function(ok2, err2)
    -- A refusal at the call was answered there; the callback hears it too, once, and is not followed.
    if refused then return end
    if ok2 == false then
      host.log.info(string.format("[avenger] '%s': the double-click was not made whole — %s", label, tostring(err2)))
      return after(false, refuse(err2))
    end
    after(true)
  end)
  if ok == false then
    refused = true
    return false, refuse(err)
  end
  return true
end

-- A double-click at screen point (x, y), checked by pointAt, both clicks left (doubled): true and
-- `after(ok, said)` later, or false and what was said.
local function doublePoint(o, label, x, y, quiet, after)
  local px, py = pointAt(o, label, x, y, quiet)
  if not px then return false, py end
  host.log.info(string.format("[avenger] '%s': double-clicking (%d,%d)", label, px, py))
  return doubled(label, px, py, nil, quiet, after)
end

local function clickAt(o, label, at, button, quiet)
  local x, y = pointOf(o, label, at, quiet)
  if not x then return false, y end
  host.log.info(string.format("[avenger] '%s': clicking (%d,%d)%s", label, x, y,
    button and (" with the " .. button .. " button") or ""))
  if button then
    host.input.click(x, y, { button = button })
  else
    host.input.click(x, y)
  end
  return true
end

-- A double-click of the module's own, placed and checked as clickAt's click is (the oscillator's
-- and the macro knobs' resets, src/act.luau): both clicks with `button` (doubled), and
-- `after(true)`, as a handler, once the second has gone out — where the change is watched for from
-- — or `after(false, said)`; false and what was said where nothing went out.
local function doubleAt(o, label, at, button, quiet, after)
  local x, y = pointOf(o, label, at, quiet)
  if not x then return false, y end
  host.log.info(string.format("[avenger] '%s': double-clicking (%d,%d)%s", label, x, y,
    button and (" with the " .. button .. " button") or ""))
  return doubled(label, x, y, button, quiet, after)
end

-- ── A popup left open ────────────────────────────────────────────────────────
-- A choice the runtime gave up on AFTER its popup was seen (`pick.seen`, set by the item's
-- placing) — it could not be placed, something covers the item, the item fell outside it, or the
-- popup did not take the click — leaves Avenger's popup open, and while a menu test sees it the
-- keys are the popup's: the Zoom control's Left and Right would go to the list, and the user was
-- only told the choice failed. So it is closed, and only while the menu still counts as open, so
-- that a popup already gone is not opened again. How it is closed goes by JUCE's behaviour, read in
-- its source — Avenger's popups behave JUCE-like on a Mac (popups.luau), and on Windows, where
-- Avenger's toolkit is its own (KalliGui, 2026-10-30), none of it is measured:
--   * MENU's and the zoom field's (`opener`): the way the project closes the zoom list, by
--     clicking the opener again. A JUCE popup is modal, and a click on the component it was
--     opened from dismisses it later and goes no further (juce_PopupMenu.cpp, MenuWindow::
--     inputAttemptWhenModal);
--   * a right click's (no `opener`): with Escape, which the modal popup takes (its keyPressed
--     dismisses it). Not with a click: a popup opened at a point rather than from a component is
--     dismissed by a click anywhere outside it AND the click goes on (juce_Component.cpp: "If
--     processing the input attempt has exited the modal loop, we'll allow the event to be
--     delivered") — on the tab it selects, the button it toggles, the field it edits, unread.
--     Which of the two Avenger's right clicks open is not known; Escape is right for both.
-- Quiet: the runtime has said what went wrong, and the menu tests say whether the popup has gone
-- (`[menu] … the menu has closed`). Whether either closes Avenger's popups is not measured
-- (TODO.md). A submenu that did not open (`no_submenu`, a drum slot's Copy Sample to), one its
-- target could not be chosen in (`unfit_submenu`) and one the drawn test could not get ready to
-- follow (`not_ready`) leave the menu up: Escape too, which JUCE takes for the whole of a menu and
-- its submenus (its dismissMenu walks up to the first menu; read in its source, not measured).
-- By the code of O:chooseMenuItem's error value (docs/api/overlay.md): `refused` is a popup that
-- did not take the click, `outside` and `covered` an item the overlay's own check refused.
local CLOSE_AFTER = { unplaced = true, covered = true, outside = true, refused = true, no_menu = true,
  no_submenu = true, unfit_submenu = true, not_ready = true }

local function closeLeftOpen(w, pick, err, label, opener)
  if not (pick.seen and CLOSE_AFTER[err.code] and w:menuOpen()) then return end
  if opener == nil then
    host.log.info(string.format("[avenger] '%s': its popup was left open (%s) — pressing Escape to close it",
      label, err.code))
    host.input.send("Escape")
    return
  end
  host.log.info(string.format("[avenger] '%s': its popup was left open (%s) — clicking its opener "
    .. "again to close it", label, err.code))
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
-- The readings after the click are made with a callback (host.ocr.recognize), one out at a time:
-- the watch looks every 100 ms and is handed the last answer that belongs to this press. The press
-- waits for the one before the click, because it has to be the name before the click and a read
-- that answers later cannot be: one read per press, as the Preset read-out makes one per focus.
-- Only this module waits for it; the user can bring another window to the front meanwhile, so the
-- press looks again after the read (O:stillHere) and clicks nothing in a place it has left.
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
-- `init`: whether the latest name read is Init Preset (below), or nil for not known.
local names = { gen = 0, reading = false, text = nil, textGen = -1, seq = 0, last = nil, pending = nil, init = nil }

-- ── ◀ and ▶ on Init Preset ───────────────────────────────────────────────────
-- Avenger steps no preset from its blank preset: ◀ and ▶ change nothing there, and the
-- avenger_control project's tool has no case for it — it clicks and waits for a change that never
-- comes (its actions.py, _step_preset). So where the name read is Init Preset the two say they are
-- unavailable, the runtime's way for a custom button (its `text`, announced with the label, as
-- ON:EAR's panel buttons say what blocks them), and a press clicks nothing and says why — the
-- first Mac test of the page, 2026-10-14 (TODO.md, item 3). Decided by the latest read, never by
-- a time: the press's own read of the name, or, when that reads nothing, the latest name this
-- module read (`names.init`) — by a press, a watch, the Preset read-out or the page. That claim is
-- forgotten where the preset may have changed with nothing read after it — a choice from MENU,
-- Undo, the redo list, the overlay leaving the front, and the same clicks and choices made by a
-- trip of the page's (actions.luau) — where the preset database's position is forgotten too;
-- until a read, ◀ and ▶ claim nothing, and a press reads. The page says it in its own words
-- (state.luau).
local AT_INIT = "Avenger steps no preset from Init Preset — load a preset first"

-- Name `t`, as read: the last one read, and whether it is Init Preset. `t` as it came.
local function noted(t)
  names.last = t
  names.init = t == R.INIT_PRESET
  return t
end

-- The preset may have changed, and nothing has read it since: and with it everything the ring
-- read of the oscillators and the macros (ring.luau), and the OSC tabs' names a read keeps
-- (state.luau, S.presetMayHaveChanged).
local function unseen()
  names.init = nil
  Ring.forget()
  S.presetMayHaveChanged()
end

-- The name last read, for the reads that place it in the database (read/open.luau) — nil from the
-- moment the preset may have changed with nothing read since (`unseen`): the second Mac test's ring
-- placed "Init Preset" right after Resurrection was loaded from MENU. Nil too while a step's watch
-- is out (`names.pending`), from its click until the name after it has settled: the ring's strip
-- read in that time was placed by the name before, and said the preset before's tabs for the rest
-- of the stay (the review of 2026-10-26; ring.luau reads its strip again once a name is known).
-- `names.last` itself stays, for the watch's `was`.
local function lastName()
  return names.init ~= nil and names.pending == nil and names.last or nil
end

local function tidy(text)
  local t = string.gsub(text or "", "%s+", " ")
  t = string.gsub(t, "^ ", "")
  return (string.gsub(t, " $", ""))
end

-- A name read, the project's way: what its recogniser made of Avenger's blank preset ("hit
-- Presst" at 50 %) is "Init Preset", said and compared as that, and the log says it was taken so.
local function asName(t)
  local name, init = R.presetName(t)
  if init and name ~= t then
    host.log.info(string.format("[avenger] the name read '%s' is taken as Avenger's blank preset, %s", t, name))
  end
  return name
end

-- The name as read now — the press's handler waits for the reading — or nil.
local function nameNow(o)
  local r = o:toScreenRect(G.NAME_REGION)
  if not r then return nil end
  local ok, res = pcall(host.ocr.recognize, r, { lang = H.lang() })
  local t = ok and res and tidy(res.text) or ""
  if t == "" then return nil end
  return noted(asName(t))
end

-- The last name read for press `gen` and which reading it came from (`names.seq`, counted up for
-- every name a read's answer brings), and a new read started when none is out. The watch looks
-- more often than a read answers — every 100 ms against 130 to 200 for a read on a Mac — so the
-- same answer is handed to several looks, and the reading's number is what tells the watch it is
-- the same one (O:watch counts readings, not looks).
local function nameAfter(o, gen)
  if gen ~= names.gen then return nil end
  if not names.reading then
    local r = o:toScreenRect(G.NAME_REGION)
    if r then
      names.reading = true
      local ok = pcall(host.ocr.recognize, r, { lang = H.lang(), key = "vps-avenger name" }, function(res)
        names.reading = false
        if gen ~= names.gen or res.status ~= "text" then return end
        local t = tidy(res.text)
        if t ~= "" then
          t = noted(asName(t))
          names.text, names.textGen = t, gen
          names.seq += 1
        end
      end)
      if not ok then names.reading = false end
    end
  end
  if names.textGen ~= gen then return nil end
  return names.text, names.seq
end

-- A name the watch read after a click counts once it settles: read twice in a row, or — with
-- the preset database — a catalog name as it stands (presets.luau, DB.catalogName), which counts
-- at its first reading. Avenger shows a loading display while it loads a preset, and a read of it
-- comes out as some other text each time ("cYanmaGentaYellowb", "PY", "M": the first Mac test of
-- the page, 2026-10-14, where the first reading that differed was said as the preset in seven of
-- ten steps), so two readings alike are the name. A loading display read alike twice in a row would
-- still pass; that log had none. `n` is O:watch's count of readings in a row.
local function settledName(now, n)
  return n >= 2 or DB.catalogName(now)
end

-- What the watch's bound leaves of reading `now`, read `n` times in a row, after `before`: nil for
-- a name that changed and never settled — said as not read, not as the preset — and `now` otherwise.
local function keptAtBound(label, now, before, n)
  if now == nil or now == before or settledName(now, n) then return now end
  host.log.info(string.format("[avenger] '%s': '%s' was read once and is no catalog name — not taken", label, now))
  return nil
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

-- The name before a press's click, read while the press waits — and whether the overlay is still
-- where the press was made: `false` and nothing more when it is not, said in the log.
local function nameBefore(o, label)
  local mark = o:here()
  local read = nameNow(o)
  local fine, why = o:stillHere(mark)
  if not fine then
    host.log.info(string.format("[avenger] '%s': not going on after the name was read — %s", label, tostring(why)))
    return false
  end
  return true, read
end

local function presetStep(ov, label, at, dir)
  ov:addCustomButton({
    label = label,
    -- What the latest read says, at once: announcing reads nothing.
    text = function() return names.init and "unavailable on Init Preset" or nil end,
    onActivate = function(o)
      -- A knob the ring holds (ring.luau, Ring.stillWorking): nothing clicked.
      if Ring.stillWorking(label) then return end
      local here, read = nameBefore(o, label)
      if not here then return end
      if names.init then
        host.log.info(string.format("[avenger] '%s': %s Init Preset, which Avenger steps no preset from — "
          .. "nothing clicked", label, read and "the name reads" or "the name read last was"))
        host.speech.output(label .. ": " .. AT_INIT, { interrupt = true })
        return
      end
      local was = read or names.last
      if not clickAt(o, label, at) then return end
      -- Another preset: what the ring read of its oscillators and macros is not Avenger's any more,
      -- nor the tabs' names a read keeps.
      Ring.forget()
      S.presetMayHaveChanged()
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
        done = function(now, before, n) return before ~= nil and now ~= before and settledName(now, n) end,
        within = NAME_WITHIN,
        onDone = function(now)
          settled(gen)
          host.speech.output(decided(step, gen, now, false) or now, { interrupt = true })
        end,
        onGiveUp = function(now, before, n)
          settled(gen)
          -- Overtaken by a later press, whose own watch says the name: this one has read nothing
          -- since (nameAfter), and a word of its own at its bound would come after that name and
          -- say it cannot be read — ▶ ▶ within half a second on a Mac, where two readings alike
          -- keep a watch open longer.
          if gen ~= names.gen then return end
          now = keptAtBound(label, now, before, n)
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

-- For a trip of the page's (actions.luau): the name after a click of the module's own, watched as
-- a step's is — read again and again until it differs from `was` and has settled, up to
-- NAME_WITHIN — and `onEnd(now, before, changed)` once, with the last name read (nil: none, or one
-- that never settled). The watch ends unreported where the overlay leaves (O:watch); the trip's own
-- ends cover that. `label` names the action in the log.
local function watchName(o, was, onEnd, label)
  names.gen += 1
  local gen = names.gen
  o:watch({
    read = function(w) return nameAfter(w, gen) end,
    was = was,
    done = function(now, before, n) return before ~= nil and now ~= before and settledName(now, n) end,
    within = NAME_WITHIN,
    onDone = function(now, before) onEnd(now, before, true) end,
    onGiveUp = function(now, before, n) onEnd(keptAtBound(label or "the page", now, before, n), before, false) end,
  })
end

-- For Browse Presets (src/handover.luau): the name read until it settles, with nothing to differ
-- from — the user chose in Avenger's own browser, and it may be the preset that was there — up to
-- NAME_WITHIN, and `onEnd(now, n)` once: the name, read `n` times in a row, or nil (none, or one
-- that never settled), as watchName's bound leaves it.
local function nameSettles(o, onEnd, label)
  names.gen += 1
  local gen = names.gen
  o:watch({
    read = function(w) return nameAfter(w, gen) end,
    done = function(now, _, n) return settledName(now, n) end,
    within = NAME_WITHIN,
    onDone = function(now, _, n) onEnd(now, n) end,
    onGiveUp = function(now, _, n) onEnd(keptAtBound(label or "the page", now, nil, n), n) end,
  })
end

-- ── Preset info ──────────────────────────────────────────────────────────────
-- Only with the preset database: the project's Info. Pressed, it reads the name as a step does
-- before its click — the press waits for it, one read per press — and says what the database has on
-- it (presets.luau, DB.info): its expansion, category, oscillator tabs and macro names, plainly,
-- with "from the database" only after a name the database guessed (presets.luau, WHAT IT MAY DO).
-- A button rather than a read-out: the whole of it is a long sentence nobody wants on every Tab
-- past it.
local function presetInfo(ov)
  ov:addCustomButton({
    label = "Preset info",
    onActivate = function(o)
      local here, read = nameBefore(o, "Preset info")
      if not here then return end
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
-- Chosen in Avenger's own menu, a popup it draws itself — painted inside Avenger's editor in
-- Logic, as a JUCE popup given a parent is (a Mac finding), possibly a window of its own elsewhere
-- (popups.luau; unmeasured on Avenger's Windows toolkit, KalliGui): MENU is clicked, and the item
-- once a menu test (below, the binding) sees the popup and says where it is — never after a wait
-- — and the choice counts once the popup has taken it (O:chooseMenuItem).
--
-- The item is placed from Avenger's corner, where the project's figures are, across the middle of
-- the popup's own width, in screen pixels at the factor MENU was clicked at (the runtime hands it
-- over); and only in a popup that opened where those figures hold — its top below MENU's middle
-- and above Load's — since JUCE moves a popup that has no room below its opener (JUCE-like
-- behaviour; unmeasured on KalliGui), and a row counted
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

-- The items that open a file dialog, clicked once (O:chooseMenuItem's `retries = 0`): Avenger's
-- popup stays drawn, frozen, while the dialog opens — the second Mac test clicked Load preset four
-- times, and a second click lands in the dialog. The runtime waits on a popup frozen as at the
-- click since 0.13, but a click it follows and still finds so at the press's bound is given up as
-- not taken, and the popup is then closed by a click on MENU (closeLeftOpen), which would land in
-- the dialog too; so these are chosen at their click, as the project's tool clicks them once.
local ONE_CLICK = { ["Load preset"] = true, ["Save preset as"] = true }

-- After Load and Save as, the name (the second Mac test's finding 18: the ring said nothing of the
-- preset loaded): the file dialog the item opens is the user's, and once the plug-in is in front
-- again after it — the overlay runtime's O:afterDialog (0.14), the overlay holding its place over
-- the dialog or coming back to the window it was chosen in — the name is watched as the page's Load
-- watches it (watchName: read until it differs from `was` and has settled, up to NAME_WITHIN) and
-- said in the page's words, "Load preset: <name>", or that it cannot be read. No dialog within the
-- runtime's bound: the name all the same, with that said. A wait the runtime refuses says nothing
-- more than the press did.
local function nameAfterDialog(w, label, was)
  local ok, err = w:afterDialog(function(o, seen)
    if not o.active then
      host.log.info(string.format("[avenger] '%s': %s, and the overlay is not in front — the name is not read", label,
        seen and "back after the dialog" or "no file dialog came"))
      return
    end
    watchName(o, was, function(now)
      local said
      if now == nil then
        said = label .. ": the preset name cannot be read now"
      else
        said = label .. ": " .. now .. (seen and "" or " — no file dialog was seen")
      end
      host.log.info(string.format("[avenger] '%s': the name after the dialog — %s", label, said))
      host.speech.output(said, { interrupt = true })
    end, label)
  end)
  if not ok then
    host.log.info(string.format("[avenger] '%s': the name is not watched after the dialog — %s", label, tostring(err)))
  end
end

-- `arms`: the item opens Avenger's warning box, which is looked for once the choice has been
-- taken.
local function menuEntry(ov, label, item, arms)
  ov:addCustomButton({
    label = label,
    onActivate = function(o)
      if Ring.stillWorking(label) then return end
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
      -- The name before the choice, for the watch after Load's and Save as's dialog.
      local was = names.last
      local opened = o:chooseMenuItem({
        label = label,
        at = G.MENU,
        menuItem = menuItemAt(label, item, pick),
        retries = ONE_CLICK[label] and 0 or nil,
        onDone = function(w, chosen, err)
          if chosen then
            -- Every item may load, rename, overwrite or blank the preset: the database's position
            -- is not Avenger's any more, nor the latest name read.
            DB.forget(label .. " chosen", OWN_NAME[label])
            unseen()
            if arms then D.arm(id, label) end
            if ONE_CLICK[label] then nameAfterDialog(w, label, was) end
            return
          end
          closeLeftOpen(w, pick, err, label, G.MENU)
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
-- wherever the plug-in put the popup — below the field, or moved up because it had no room there, which
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
      if Ring.stillWorking("Zoom") then return false end
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
        onDone = function(w, chosen, err)
          if chosen then
            H.stepped(w:origin() or origin, z)
            return
          end
          -- A press dropped while this one's list is awaited ends nothing of this one's.
          if err.code == O.errors.BUSY then return end
          H.zoomOver()
          closeLeftOpen(w, pick, err, "Zoom", G.ZOOM_FIELD)
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
-- empty read is the recogniser's failure — Avenger always shows a name — and is said as that; a
-- reading of the blank preset the project knows for a misread is said as "Init Preset" (`value`),
-- and kept as the latest name read while the overlay is in front, for ◀ and ▶.
header:addOCRButton({ label = "Preset", region = G.NAME_REGION, readOnly = true, fallback = "cannot be read now",
  value = function(o, text)
    local name = R.presetName(text)
    if o.active then noted(name) end
    return name
  end })
presetStep(header, "Previous preset", G.PREV, -1)
presetStep(header, "Next preset", G.NEXT, 1)
if DB.present then presetInfo(header) end
menuEntry(header, "Load preset", G.LOAD, false)
menuEntry(header, "Save preset", G.SAVE, true)
menuEntry(header, "Save preset as", G.SAVE_AS, false)
menuEntry(header, "Initialize preset", G.INITIALIZE, true)
-- UNDO, clicked as a hotspot clicks and said as one says its click; an undo may take the preset
-- back — after Initialize, to the one before — so what this module read of it is forgotten.
header:addCustomButton({
  label = "Undo",
  onActivate = function(o)
    if Ring.stillWorking("Undo") then return end
    if not clickAt(o, "Undo", G.UNDO) then return end
    DB.forget("Undo")
    unseen()
    host.speech.output("Undo, activated", { interrupt = true })
  end,
})
-- The redo list is Avenger's own popup, which the project found VoiceOver could step through with
-- the arrow keys: opened with a right click on UNDO, and while a menu test sees it the keys are the
-- list's (`opensMenu`: the runtime's press, which the menu tests compare against — newWindow's
-- window list, drawn's picture of the list's rectangle — comes before the click). Whether NVDA
-- reads it, and whether VoiceOver does from here — a list painted inside Avenger publishes no
-- element, so most likely not — is a session's question
-- (TODO.md). UNDO's crosshair in the header picture of a calibrating run (header.luau) is both
-- controls' point.
header:addCustomButton({
  label = "Redo list",
  opensMenu = true,
  onActivate = function(o)
    if Ring.stillWorking("Redo list") then return end
    if clickAt(o, "Redo list", G.UNDO, "right") then
      H.popup(o:origin())
      -- What the list redoes is chosen in the list, which this module does not read.
      DB.forget("the redo list opened")
      unseen()
      host.speech.output("Redo list, activated", { interrupt = true })
    end
  end,
})
zoomStepper(header)
-- The oscillator strip and the macros, after Zoom (ring.luau).
Ring.build(header, { lastName = lastName })
-- Away from the overlay, the preset can be changed in ways it does not see — the DAW's own
-- preset menu, a file dialog, another project: the database's position goes with it.
header:onDeactivate(function()
  H.left()
  -- A watch out now ends without a word (O:watch looks only while the overlay is up).
  names.pending = nil
  DB.forget("the overlay left the front")
  unseen()
end)
-- ── The page ─────────────────────────────────────────────────────────────────
-- Its key, its settings and its description (page.luau), added before the header is bound, as
-- O:addPage asks: the zoom as the header read it; the rest is read off one picture when the key is
-- pressed (read/open.luau). The header's actions in the page use this file's own clicks, reads and
-- choices (actions.luau), and the tables of the ring's.
Page.start({
  header = header,
  zoomOf = function(origin) return (H.zoom(origin)) end,
  DB = DB,
  ring = {
    clickAt = clickAt,
    pointOf = pointOf,
    pointAt = pointAt,
    clickPoint = clickPoint,
    doublePoint = doublePoint,
    nameNow = nameNow,
    lastName = lastName,
    noteName = noted,
    unseen = unseen,
    atInit = function() return names.init == true end,
    watchName = watchName,
    settledName = nameSettles,
    menuItemAt = menuItemAt,
    zoomEntry = zoomEntry,
    closeLeftOpen = closeLeftOpen,
    doubleAt = doubleAt,
    forgetRing = Ring.forget,
    -- For the DRUM SQ watcher (src/watch.luau): the ring's action out, its values, its merge.
    ringBusy = Ring.busy,
    ringValues = Ring.values,
    ringSeen = Ring.seen,
    boxOffScreen = boxOffScreen,
    OWN_NAME = OWN_NAME,
    ZOOM_SETTLE = ZOOM_SETTLE,
  },
})

-- Avenger's popups — MENU, the zoom list, the redo list, the tabs', knobs' and buttons' menus — are
-- drawn by Avenger itself, and two tests see them, each saying where so an item can be chosen in
-- one: `drawn`, a popup painted inside Avenger's editor where popups.luau says it is drawn — what
-- the first Mac test of the page found in Logic, where no window of the process came for the zoom
-- list (TODO.md, item 5) — and `newWindow`, a popup that is a window of the DAW's process (JUCE's
-- other case, and possibly Windows', where Avenger's toolkit is KalliGui and its popups are
-- unmeasured; a DAW that runs plug-ins apart is the popup's owner then, unmeasured). A choice is
-- followed by the test that placed its item. Not `nativePopup`: Avenger draws its menus itself
-- rather than as the system's — on a Mac no system menu came for them — and a test that says only
-- "a menu is open" places no item.
header:bind(BINDING, { specificity = O.layer.base,
  menus = { O.menuTests.newWindow, O.menuTests.drawn(Popups.spec) } })

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
