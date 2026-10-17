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
-- Nothing is ever flipped from memory: the tool turned a macro button's state over when its read
-- was unsure; here an unsure read is "cannot be read now".

local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local Open = host.include("src/read/open.luau")
local Pp = host.include("src/popups.luau")

local Act = {}

local R = nil -- main.luau's ring helpers (Act.start)

-- How long a change is looked for after the click, at most — the longest one change wait may wait
-- (docs/api/screen.md) and the project's measure of Avenger's redraw, about 150 ms and up to 500,
-- with room for a Mac's capture. A bound on looking: a change that comes is answered when it comes.
Act.CHANGE_WITHIN = 2000
-- What counts as changed in a change wait: a channel 16 apart, in 4 pixels — the host's defaults.
local TOLERANCE, MIN_PIXELS = 16, 4

-- Why a choice in Avenger's menu was not made (O:chooseMenuItem's `why`), in the page's words.
local NOT_CHOSEN = {
  due = "no menu was seen; nothing was chosen",
  closed = "the menu closed before anything was chosen",
  unplaced = "the menu cannot be placed now; nothing was chosen",
  covered = "something else is covering the menu; nothing was chosen",
  refused = "the menu could not be clicked; nothing was chosen",
  untaken = "the menu did not take the choice",
  inactive = "VPS Avenger is not in front; nothing was chosen",
  ["no tests"] = "the menu cannot be seen here; nothing was chosen",
  busy = "the last choice is still being made",
}

function Act.notChosen(why)
  return NOT_CHOSEN[why] or ("nothing was chosen (" .. tostring(why) .. ")")
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
--   decide(vb) -> click | nil, why, focus?   from the read before; nil refuses with `why`, and
--             `focus` the page's element the refusal leaves the focus on. A click is
--             { at = {x, y}, button = "right"?, double = true?, menu = { item = {x, y},
--             retries = n?, read = false? }? }, points at 50 % from Avenger's corner — a menu's
--             `read = false`: nothing to read after it
--   macroCount   the macro tabs counted before, where the part's text is not read (the tab shown)
--   watch(vb, click) -> { rect }?   the boxes the change is looked for in (default: the part)
--   same(vb, va) -> boolean   nothing this action changes changed
--   answer(vb, va, how) -> result   how: "changed", "unchanged" (at the bound), "chosen" (a menu
--             that changes nothing shown) or "unread" (no picture after the click)
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
  local readOpts = { pl = pl, text = spec.text, preset = R.lastName(), macroCount = spec.macroCount }
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
    macroCount = vb.macroCount or spec.macroCount }
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
      local okSame, same = pcall(spec.same, vb, va)
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
    local pick = {}
    local opened = o:chooseMenuItem({
      label = label,
      at = click.at,
      button = "right",
      menuItem = Pp.row(label, click.menu.item, pick),
      retries = click.menu.retries,
      onDone = function(m, chosen, why)
        if not chosen then
          -- A right click's popup: closed with Escape, not a click on its opener (main.luau).
          if why ~= "busy" then R.closeLeftOpen(m, pick, why, label, nil) end
          snap:release()
          return done({ part = before, error = Act.notChosen(why) })
        end
        if click.menu.read == false then
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
    ok, said = R.doubleAt(o, label, click.at, click.button, true, watchFromNow)
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
-- picture waited for, its pixels (and with `text` its words) asked of it, the picture let go. The
-- values and their keys, or nil, nil and the page's error; `log(fmt, …)` the caller's log.
function Act.partBefore(pl, part, text, log)
  local region = Open.partRegion(pl, part)
  if not region then return nil, nil, "that part of VPS Avenger is not on the screen" end
  local okS, snap, why = pcall(host.screen.snapshot, { region = region })
  if not (okS and snap) then
    log("'%s': nothing clicked — no picture of it: %s", part, tostring(okS and why or snap))
    return nil, nil, "VPS Avenger cannot be read now"
  end
  local okR, v, keys = pcall(Open.part, snap, part, { pl = pl, text = text, preset = R.lastName() })
  snap:release()
  if not okR then
    log("'%s': nothing clicked — the read before failed: %s", part, tostring(v))
    return nil, nil, "reading VPS Avenger failed"
  end
  return v, keys
end

-- What main.luau hands over: its clicks, its reads of the name and its tidying up after a popup.
function Act.start(ring)
  R = ring
end

return Act
