-- One way for every click of the oscillator strip's and the macros' actions (the plan's slice 5),
-- whichever asks for it — a trip of the page's (actions.luau, O:inPlugin) or a control of the ring
-- (main.luau) — so that both click where the tables put a point and say only what was read after:
--
--   1. BEFORE. One picture of the part the action acts on (read/open.luau, Open.PARTS), waited for
--      where the action runs — a trip's act, a control's press: handlers, where reads wait — and
--      read with pixels only (Open.part), with its text where the click needs a count or a name.
--   2. DECIDE. The action's own `decide` places the click from that read, or refuses it, and a
--      refusal clicks nothing: no count read, an index past the count, the eighth oscillator, an
--      oscillator's knob while Drums is selected. A refusal at a limit keeps the tool's focus
--      there (the eighth tab, the fourth macro tab, the only one).
--   3. CHECK. Still where the action began: `trip:inFront()` in a trip (false has ended it), the
--      overlay's own O:stillHere in the ring.
--   4. CLICK. Left or right (main.luau's clickAt, placed and checked as the ring's own clicks), a
--      double-click (doubleAt, O.doubleClick, its button and its callback after the second click),
--      or an item of the right click's menu, chosen once a menu test sees the popup
--      (O:chooseMenuItem; popups.luau places the item).
--   5. CHANGE. A change wait from the picture of step 1 — after the double-click's second click,
--      after the menu took its item — on the part's rectangle or the watched boxes in it, at most
--      Act.CHANGE_WITHIN (host.screen.snapshot's `change`); then the part read again off the picture
--      it answered with. A read that shows nothing of what the action changes changed, on a picture
--      that changed — the pointer's hover drawn over a button first — looks on from that picture,
--      within the same bound: readings, not a sleep. At the bound "unchanged" is said, and what was
--      read is what the page shows.
--   6. DONE. `done(result)` once, with what was read; nil where the user moved on, and nothing is
--      said then. Every snapshot is released on every path.
--
-- A menu whose item changes nothing Avenger shows — a macro knob's reset mode, an OSC tab's Copy —
-- has no change wait: the menu taking the click is all there is to read (`menu.read = false`).
-- One whose item counts as chosen at its click (`retries = 0`, a macro button's tick) has its
-- change waited for from the picture before the right click all the same: Avenger acts on the
-- click after it has gone out, and a picture taken at once would show the buttons from before.
-- An item that opens a submenu (`menu.submenu`, a drum slot's Copy Sample to) is chosen in it by the
-- runtime (O:chooseMenuItem's `submenu`); one that opens a file dialog (`menu.dialog`, Save Drumkit
-- as) waits for the dialog, which is the user's (trip:afterDialog), in the place of the change wait,
-- and with `dialog = "read"` (DRUM SQ's sequence menu) reads the part once more after it. A menu
-- opened with the LEFT button (`menu.button`: the sample editor's Grid, Play Mode, an EQ band's
-- type, Loop-Stretch, DRUM SQ's SPEED) is a field with its popup attached, as the zoom field's is:
-- a failed choice that leaves it open is closed by clicking that field again, a right click's with
-- Escape (main.luau, closeLeftOpen). `fields` names what the part reads as text besides, where it is
-- shown (read/open.luau): a menu's field, DRUM SQ's LENGTH and SPEED, the envelope's name.
-- Nothing is ever flipped from memory: the tool turned a macro button's state over when its read
-- was unsure; here an unsure read is "cannot be read now". And where nothing reads what the action
-- changes — a drum knob's reset — `same` is left out, and the change wait's own verdict on the
-- watched boxes is the answer: "changed" or, at its bound, "unchanged", with no looking on.

local O = host.require("com.platform.overlay")
local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local Open = host.include("src/read/open.luau")
local Pp = host.include("src/popups.luau")

local Act = {}

local R = nil -- main.luau's ring helpers (Act.start)
local Header = nil -- the header overlay, the one with menu tests (Act.start)

-- How long a change is looked for after the click, at most — the longest one change wait may wait
-- (docs/api/screen.md) and the project's measure of Avenger's redraw, about 150 ms and up to 500,
-- with room for a Mac's capture. A bound on looking: a change that comes is answered when it comes.
Act.CHANGE_WITHIN = 2000
-- What counts as changed in a change wait: a channel 16 apart, in 4 pixels — the host's defaults.
local TOLERANCE, MIN_PIXELS = 16, 4

-- Why a choice in Avenger's menu was not made, by the code of O:chooseMenuItem's error value
-- (docs/api/overlay.md), in the page's words; another code says its message.
local NOT_CHOSEN = {
  no_menu = "no menu was seen; nothing was chosen",
  closed = "the menu closed before anything was chosen",
  unplaced = "the menu cannot be placed now; nothing was chosen",
  covered = "something else is covering the menu, or it is gone; nothing was chosen",
  outside = "the menu could not be clicked; nothing was chosen",
  refused = "the menu did not take the choice",
  no_submenu = "its submenu did not open; nothing was chosen in it",
  unfit_submenu = "no submenu it can be chosen in was seen; nothing was chosen in it",
  not_ready = "its submenu cannot be followed now; nothing was chosen",
  not_in_front = "VPS Avenger is not in front; nothing was chosen",
  moved_on = "something else came first; nothing was chosen",
  gone = "VPS Avenger's window is gone; nothing was chosen",
  busy = "the last choice is still being made",
}

function Act.notChosen(err)
  return NOT_CHOSEN[err.code] or ("nothing was chosen (" .. tostring(err) .. ")")
end

-- Where no menu can be seen: a trip's overlay is the warning box while the box holds the slot (a
-- Save or Initialize waiting for it), and the box has no menu tests, so O:chooseMenuItem and
-- trip:afterMenu raise there — a mistake in the call, by the runtime's rule. Nothing is chosen on
-- it, and the page is told so. Only the header has the menu tests (main.luau, its binding).
Act.NO_MENU_HERE = "the menu cannot be seen here; nothing was chosen"

function Act.menuHere(o)
  return o == Header
end

local function corners(r)
  return string.format("(%d,%d)-(%d,%d)", r[1], r[2], r[3], r[4])
end

-- The watched boxes of an action, table rectangles, as screen rectangles cut to the picture's
-- `region`; nil — the whole region — when it names none, or none lies in the region.
local function watched(pl, rects, region)
  if not rects then return nil end
  local out = {}
  for _, t in ipairs(rects) do
    local r = pl:rect(t)
    if r then
      r = { math.max(r[1], region[1]), math.max(r[2], region[2]), math.min(r[3], region[3]), math.min(r[4], region[4]) }
      if (r[3] - r[1]) * (r[4] - r[2]) >= MIN_PIXELS then out[#out + 1] = r end
    end
  end
  return #out > 0 and out or nil
end

-- Action `spec` on overlay `o`, in a handler where reads wait; `trip` when a trip of the page's
-- runs it. `spec`:
--   label     the action's words, in the log and as the menu's label
--   part      Open.PARTS' name, read before the click and after it
--   text, textAfter   the part's text read before the click, after it
--   fields    the fields read as text before and after the click, where they are shown (Open.part)
--   expected  the expansion known open, for the `browser` part's reads (read/browser.luau, Br.part)
--   decide(vb) -> click | nil, why, focus?   from the read before; nil refuses with `why`, and
--             `focus` the page's element the refusal leaves the focus on. A click is
--             { at = {x, y}, button = "right"?, double = true?, menu = { item = {x, y},
--             button = "left"?, retries = n?, read = false?, submenu = { x, below, row, first }?,
--             dialog = true | "read"? }? }, points at 50 % from Avenger's corner — a menu's
--             `button`: its opener's, the right one unless it says "left"; `read = false`: nothing
--             to read after it; `submenu`: the item opens a submenu, its item chosen there, at x
--             from Avenger's corner and `below` from the submenu's own top (popups.luau,
--             Pp.submenuRow); `dialog`: the item opens a file dialog, waited for in a trip, nothing
--             read after it — with "read", the part read once more when Avenger is in front again
--   macroCount   the macro tabs counted before, where the part's text is not read (the tab shown)
--   watch(vb, click) -> { rect }?   the boxes the change is looked for in (default: the part)
--   same(vb, va) -> boolean?   nothing this action changes changed; nil — or no `same` — where
--             nothing read says it: the change wait's verdict decides
--   answer(vb, va, how) -> result   how: "changed", "unchanged" (at the bound), "chosen" (a menu
--             that changes nothing shown, or a file dialog seen), "no dialog" (none within the
--             runtime's bound) or "unread" (no picture after the click)
-- `done(result)`: { part = { keys, values }?, error = a reason?, sentence = a whole sentence (a
-- click refused, as the ring's clicks say it)?, note = a reason?, focus = an id of the page's?, said =
-- the ring's words? } — `part` is filled in with the last read when the answer has none — or nil:
-- the user moved on.
function Act.run(o, spec, done, trip)
  local label = spec.label
  local function log(fmt, ...)
    host.log.info(("[avenger] [act] '%s': " .. fmt):format(label, ...))
  end
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then
    log("nothing clicked — VPS Avenger cannot be placed now (no factor)")
    return done({ error = "VPS Avenger's size cannot be told now" })
  end
  local pl = P.placer(origin, k, (H.zoom(origin)))
  local region = Open.partRegion(pl, spec.part)
  if not region then
    log("nothing clicked — the %s is not on the screen", spec.part)
    return done({ error = "that part of VPS Avenger is not on the screen" })
  end
  local mark = o:here()
  -- Still where the action began, after a wait; said in the log when not. A trip's check ends the
  -- trip when it fails (its give-up says the rest); the ring hears nothing.
  local function here(what)
    if trip then return trip:inFront() end
    local fine, why = o:stillHere(mark)
    if not fine then log("not going on %s — %s", what, tostring(why)) end
    return fine
  end
  local function movedOn()
    if not trip then done(nil) end
  end
  local okS, snap, whyS = pcall(host.screen.snapshot, { region = region })
  if not (okS and snap) then
    log("nothing clicked — no picture of %s: %s", corners(region), tostring(okS and whyS or snap))
    return done({ error = "VPS Avenger cannot be read now" })
  end
  local readOpts = { pl = pl, text = spec.text, preset = R.lastName(), macroCount = spec.macroCount,
    fields = spec.fields, expected = spec.expected }
  local okB, vb, keysB = pcall(Open.part, snap, spec.part, readOpts)
  if not okB then
    snap:release()
    log("nothing clicked — the read before failed: %s", tostring(vb))
    return done({ error = "reading VPS Avenger failed" })
  end
  if not here("after the read before the click") then
    snap:release()
    return movedOn()
  end
  local before = { keys = keysB, values = vb }
  local okD, click, refusal, refusalFocus = pcall(spec.decide, vb)
  if not okD then
    snap:release()
    log("nothing clicked — its decision failed: %s", tostring(click))
    return done({ part = before, error = "something went wrong in this module" })
  end
  if not click then
    snap:release()
    log("nothing clicked — %s", tostring(refusal))
    return done({ part = before, error = refusal, focus = refusalFocus })
  end

  local afterOpts = { pl = pl, text = spec.textAfter, preset = readOpts.preset,
    macroCount = vb.macroCount or spec.macroCount, fields = spec.fields, expected = spec.expected }
  local deadline = nil
  local last = nil -- { keys, values } of the latest read after the click

  local function finish(how)
    local va = last and last.values or nil
    local okA, result = pcall(spec.answer, vb, va, how)
    if not okA or type(result) ~= "table" then
      log("its answer failed: %s", tostring(result))
      result = { error = "something went wrong in this module" }
    end
    result.part = result.part or last or before
    log("%s — %s", how, tostring(result.error or result.note or result.said or "done"))
    done(result)
  end

  -- One change wait from picture `from`, then the part read off what it answered.
  local function round(from)
    local left = math.floor(math.min(Act.CHANGE_WITHIN, deadline - host.now()))
    if left < 1 then
      if from ~= snap then from:release() end
      snap:release()
      return finish("unchanged")
    end
    local change = { from = from, watch = watched(pl, spec.watch and spec.watch(vb, click) or nil, region),
      timeout = left, tolerance = TOLERANCE, minPixels = MIN_PIXELS }
    local asked, err = pcall(host.screen.snapshot, { region = region, change = change }, function(s, why, info)
      if from ~= snap then from:release() end
      if not s then
        snap:release()
        log("no picture after the click: %s", tostring(why))
        if not here("after the click") then return movedOn() end
        return finish("unread")
      end
      if not here("after the change wait") then
        s:release()
        snap:release()
        return movedOn()
      end
      local okR, va, keys = pcall(Open.part, s, spec.part, afterOpts)
      if not okR then
        s:release()
        snap:release()
        log("the read after the click failed: %s", tostring(va))
        return done({ part = before, error = "reading VPS Avenger failed" })
      end
      if not here("after the read after the click") then
        s:release()
        snap:release()
        return movedOn()
      end
      last = { keys = keys, values = va }
      local changed = info and info.changed == true
      -- Nothing read says whether what the action changes changed: the change wait's verdict.
      local okSame, same = true, nil
      if spec.same then okSame, same = pcall(spec.same, vb, va) end
      if okSame and same == nil then
        s:release()
        snap:release()
        return finish(changed and "changed" or "unchanged")
      end
      same = okSame and same == true
      if same and changed and host.now() < deadline then
        log("the picture changed and what this action changes did not, %d ms after the click — looking on "
          .. "from that picture", Act.CHANGE_WITHIN - (deadline - host.now()))
        return round(s)
      end
      s:release()
      snap:release()
      finish(same and "unchanged" or "changed")
    end)
    if not asked then
      if from ~= snap then from:release() end
      snap:release()
      log("no change wait: %s", tostring(err))
      finish("unread")
    end
  end

  local function watchFromNow()
    deadline = host.now() + Act.CHANGE_WITHIN
    round(snap)
  end

  if click.menu then
    local menu = click.menu
    -- A file dialog after the choice is waited for in a trip (trip:afterDialog): none without one.
    if menu.dialog and not trip then
      snap:release()
      log("nothing clicked — its file dialog is waited for only in a trip of the page's")
      return done({ part = before, error = "that is chosen from the page" })
    end
    if not Act.menuHere(o) then
      snap:release()
      log("nothing chosen — overlay '%s' has no menu tests", tostring(o.label))
      return done({ part = before, error = Act.NO_MENU_HERE })
    end
    local pick = {}
    local left = menu.button == "left"
    local opened = o:chooseMenuItem({
      label = label,
      at = click.at,
      button = left and "left" or "right",
      menuItem = Pp.row(label, menu.item, pick),
      submenu = menu.submenu and { menuItem = Pp.submenuRow(label, menu.submenu) } or nil,
      retries = menu.retries,
      onDone = function(m, chosen, err)
        if not chosen then
          -- A right click's popup is closed with Escape, a left opener's by clicking the opener
          -- again (main.luau).
          if err.code ~= O.errors.BUSY then R.closeLeftOpen(m, pick, err, label, left and click.at or nil) end
          snap:release()
          return done({ part = before, error = Act.notChosen(err) })
        end
        if menu.dialog then
          snap:release()
          -- The dialog is the user's: the trip goes on once Avenger is in front again, and with
          -- "read" the part is read once more there, for what the dialog did to it.
          return trip:afterDialog(function(_, t2, seen)
            log("%s", seen and "its file dialog came and went" or "no file dialog came within the bound")
            if menu.dialog == "read" then
              local okS, s, whyS2 = pcall(host.screen.snapshot, { region = region })
              if not t2:inFront() then
                if okS and s then s:release() end
                return
              end
              if okS and s then
                local okR, va, keys = pcall(Open.part, s, spec.part, afterOpts)
                s:release()
                if okR then
                  last = { keys = keys, values = va }
                else
                  log("the read after the dialog failed: %s", tostring(va))
                end
                if not t2:inFront() then return end
              else
                log("no picture after the dialog: %s", tostring(okS and whyS2 or s))
              end
            end
            finish(seen and "chosen" or "no dialog")
          end)
        end
        if menu.read == false then
          snap:release()
          return finish("chosen")
        end
        watchFromNow()
      end,
    })
    if opened then H.popup(origin) end
    return
  end
  local ok, said
  if click.double then
    -- The change watched for once the second click has gone out; one that was not made ends it.
    ok, said = R.doubleAt(o, label, click.at, click.button, true, function(okD, saidD)
      if okD then return watchFromNow() end
      snap:release()
      if not here("after the double-click") then return movedOn() end
      done({ part = before, sentence = saidD })
    end)
  else
    ok, said = R.clickAt(o, label, click.at, click.button, true)
  end
  if not ok then
    snap:release()
    return done({ part = before, sentence = said })
  end
  if not click.double then watchFromNow() end
end

-- Part `part` read before an action of a trip's own that is no click of Act.run's — a rename's
-- double-click (src/handover.luau), a knob's press (src/amount.luau) — as Act.run reads one: its
-- picture waited for, its pixels (and with `text` its words, with `fields` those fields where they
-- are shown) asked of it, the picture let go. The values and their keys, or nil, nil and the page's
-- error; `log(fmt, …)` the caller's log.
function Act.partBefore(pl, part, text, log, fields)
  local region = Open.partRegion(pl, part)
  if not region then return nil, nil, "that part of VPS Avenger is not on the screen" end
  local okS, snap, why = pcall(host.screen.snapshot, { region = region })
  if not (okS and snap) then
    log("'%s': nothing clicked — no picture of it: %s", part, tostring(okS and why or snap))
    return nil, nil, "VPS Avenger cannot be read now"
  end
  local okR, v, keys = pcall(Open.part, snap, part, { pl = pl, text = text, preset = R.lastName(), fields = fields })
  snap:release()
  if not okR then
    log("'%s': nothing clicked — the read before failed: %s", part, tostring(v))
    return nil, nil, "reading VPS Avenger failed"
  end
  return v, keys
end

-- What main.luau hands over: its clicks, its reads of the name and its tidying up after a popup.
function Act.start(ring, header)
  R, Header = ring, header
end

return Act
