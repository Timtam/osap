-- DRUM SQ's watcher (the plan's slice 8, its other half): with PATTERN CHAIN and AUTO FOLLOW on,
-- Avenger moves DRUM SQ's selected pattern by itself as a sequence plays, and the letter of the
-- pattern it moves to is said — while the page is hidden and the overlay is on Avenger, as the
-- avenger_control tool's watcher said it (its dispatcher.py, a thread that looked every 0.1 s).
--
-- WHAT DECIDES. Only readings: a pattern is said when the read of the moment shows another one
-- selected than the read before it, and nothing else is said. The cadence — the `drumSqCadence`
-- setting, 100 to 1000 ms, 250 by default — is how often a picture is taken, a sampling rate, never
-- a bound that decides anything; a turn whose last picture or question is still out is skipped
-- (host.screen.pending), and so is every turn while the module waits in a handler (host.timer).
--
--   ARMED     by the newest read of DRUM SQ in Avenger's window — the ring's of this stay, the page's
--             or a trip's (state.luau) — showing it the central tab with PATTERN CHAIN and AUTO
--             FOLLOW on; the pattern then selected is the one the next change is measured from — where
--             none reads selected, the first one its own reads find, taken without a word — so arming
--             says nothing. A turn that may not look (below) leaves it unarmed, and a read
--             newer than the one it was armed from arms it again: the pattern the page or the ring
--             just chose is not said again, as the tool reset its watcher's last pattern when its
--             panel showed. The tool armed itself from what its state assumed, and kept its state's
--             selection when a scan could not tell.
--   LOOKING   only while the header overlay is active on Avenger and not away (the page in front,
--             a trip, a hand-over: docs/api/overlay.md, O.away), the page not shown — not even
--             beside Avenger, where the tool's visible panel kept its watcher quiet too — no menu
--             over Avenger and no action of the ring's out: the module's own reads are the truth
--             then.
--   ONE PICTURE a turn, with a callback: the central tabs' row and the patterns' row, 381 by 40
--             pixels at 80 %. The central tab and DRUM SQ are asked of it with callbacks, each under
--             a key of its own, and the picture is let go at once — a read on the image worker keeps
--             the pixels it needs. Nothing waits on the event loop, and nothing loops over pixels in
--             Luau: four band means, three shares and the ranking.
--   DISARMED  by its own read where that shows otherwise: another central tab, PATTERN CHAIN or AUTO
--             FOLLOW off. That read is written into the state and the ring, so the page's next state
--             and the ring say it. A read that tells nothing — not DRUM SQ's, too dark, no pattern
--             standing out — says and writes nothing, and the watcher stays armed.
--   SAID      the letter, interrupting what the screen reader says, as the tool's VoiceOver output
--             did; and written into the state and the ring.
--
-- The selected pattern is looked for only among A and the patterns the same picture reads on (the
-- tool's restriction, from its state there: read/drumsq.luau, `onlyOn`).

local O = host.require("com.platform.overlay")
local H = host.include("src/header.luau")
local P = host.include("src/read/picture.luau")
local Open = host.include("src/read/open.luau")
local Centre = host.include("src/read/centre.luau")
local DrumSq = host.include("src/read/drumsq.luau")
local C = host.include("src/tables/centre.luau")

local W = {}

W.DEFAULT = 250
W.MIN, W.MAX = 100, 1000
-- The central tabs' row and the patterns' row, at 50 % from Avenger's corner.
W.RECT = { math.min(Centre.ROW[1], DrumSq.ROW[1]), math.min(Centre.ROW[2], DrumSq.ROW[2]),
  math.max(Centre.ROW[3], DrumSq.ROW[3]), math.max(Centre.ROW[4], DrumSq.ROW[4]) }
-- The picture's key, and its questions' prefix: each request out under its own.
W.PICTURE_KEY = "vps-avenger watch picture"
local QUESTION_KEY = "vps-avenger watch "
-- The minute summary's period.
local SUMMARY_MS = 60000

local E = nil -- what page.luau hands over (W.start)
local token = nil -- the timer's
local cadence = W.DEFAULT
local armed = false
local armedIn = nil -- the window it was armed in
local armedAt = nil -- the time of the read it was armed from, or of its own last write
local last = nil -- the pattern the next change is measured from
local failing = false -- the last read told nothing: only the first of a run is logged
local busyKeys = {} -- the keys of the questions out
local stats = nil

local function log(fmt, ...)
  host.log.info(("[avenger] [watch] " .. fmt):format(...))
end

local function resetStats()
  stats = { since = host.now(), pictures = 0, skipped = 0, picMs = 0, askMs = 0, said = 0, lastAt = nil, gaps = 0,
    gapMs = 0 }
end

-- The minute summary, for the developer's measure of what the watcher costs (TODO.md, D11).
local function summary()
  local now = host.now()
  if now - stats.since < SUMMARY_MS then return end
  if stats.pictures > 0 or stats.skipped > 0 then
    log("%d s: %d pictures (every %d ms, about every %s ms), %d skipped while the last was out, the picture %d ms and the "
      .. "questions %d ms on average, %d changes said", (now - stats.since) // 1000, stats.pictures, cadence,
      stats.gaps > 0 and tostring(stats.gapMs // stats.gaps) or "—", stats.skipped,
      stats.pictures > 0 and stats.picMs // stats.pictures or 0, stats.pictures > 0 and stats.askMs // stats.pictures or 0,
      stats.said)
  end
  resetStats()
end

-- Whether any of the watcher's requests is out: its picture, or one of its questions.
local function pending()
  if host.screen.pending(W.PICTURE_KEY) then return true end
  for key in pairs(busyKeys) do
    if host.screen.pending(key) then return true end
    busyKeys[key] = nil
  end
  return false
end

-- Whether the watcher may look now, and the window it looks in; false and why otherwise.
local function mayLook()
  local o = E.header
  if not (o.active and not o.away) then return false, "inactive" end
  local page = E.page()
  if page and page:isShown() then return false, "the page" end
  if o:menuOpen() then return false, "a menu" end
  if E.ringBusy() then return false, "the ring" end
  local win = host.window.active()
  if not win then return false, "no window" end
  return true, win
end

-- The newest read of DRUM SQ, and when it was made: the ring's of this stay where it is newer than
-- the state's.
local function newest(win)
  local values, at = E.ringValues()
  local stateAt = E.S.readAt(win.id)
  if values and at and (stateAt == nil or at >= stateAt) then return values, at end
  return E.S.values(win.id), stateAt
end

local function onDrumSq(v)
  return v and v.central == "drum_sq" and v.drumSq and v.drumSq.chain == true and v.drumSq.follow == true
end

-- What a read of the watcher's own says, written where the page and the ring read it — and its time
-- the one a newer read is measured from.
local function write(win, v)
  E.S.part(win.id, { "central", "drumSq" }, v)
  E.ringSeen({ "central", "drumSq" }, v)
  armedAt = host.now()
end

-- The answers to the picture's questions, decided: in the last question's callback.
local function decide(win, pl, got, lines, t0, tPic)
  local function buffered(s) lines[#lines + 1] = s end
  local v = {}
  Centre.decide(pl, got, v, buffered, {})
  if v.central == "drum_sq" then DrumSq.decide(pl, got, v, buffered, {}, { onlyOn = true }) end
  stats.askMs += host.now() - tPic
  if v.central == nil or (v.central == "drum_sq" and v.drumSq == nil) then
    if not failing then
      failing = true
      log("not read — nothing said, still armed: %s", table.concat(lines, "; "))
    end
    return
  end
  failing = false
  if v.central ~= "drum_sq" or v.drumSq.chain ~= true or v.drumSq.follow ~= true then
    armed = false
    write(win, v)
    local why = v.central ~= "drum_sq" and (string.upper((string.gsub(v.central, "_", " "))) .. " as the central tab")
      or (v.drumSq.follow ~= true and "AUTO FOLLOW off" or "PATTERN CHAIN off")
    log("disarmed — %s: %s", why, table.concat(lines, "; "))
    return
  end
  local selected = v.drumSq.selected
  if selected == nil or selected == last then return end
  -- Armed with no pattern read as selected: the first one read is what the next change is measured
  -- from, not a change.
  if last == nil then
    last = selected
    write(win, v)
    log("pattern %s read, none when armed — not said: %s", string.upper(selected), table.concat(lines, "; "))
    return
  end
  local before = last
  last = selected
  write(win, v)
  host.speech.output(string.upper(selected), { interrupt = true })
  stats.said += 1
  log("pattern %s (%s before), %d ms after the tick — said: %s", string.upper(selected), string.upper(before),
    host.now() - t0, table.concat(lines, "; "))
end

local shots = {} -- the calibration pictures written this session, by name

-- The calibration picture's marks: the central tabs and DRUM SQ's row.
local function marksOf(pl)
  local out = {}
  local function mark(p) local x, y = pl:point(p[1], p[2]); out[#out + 1] = { x = x, y = y } end
  for _, key in ipairs({ "editor", "arp", "drum_sq", "step_sq" }) do mark(C.TABS[key]) end
  mark(C.SQ_PATTERN_PLAYER)
  mark(C.SQ_AUTO_FOLLOW)
  for _, key in ipairs({ "a", "b", "c", "d" }) do mark(C.SQ_PATTERNS[key]) end
  return out
end

-- One turn of the timer.
local function tick()
  summary()
  local fine, win = mayLook()
  if not fine then
    armed = false
    return
  end
  if pending() then
    stats.skipped += 1
    return
  end
  local src, at = newest(win)
  if not onDrumSq(src) or (armed and (armedIn ~= win.id or (at ~= nil and armedAt ~= nil and at > armedAt))) then
    armed = false
    if not onDrumSq(src) then return end
  end
  if not armed then
    armed, armedIn, armedAt, failing = true, win.id, at, false
    last = src.drumSq.selected
    log("armed — DRUM SQ, PATTERN CHAIN and AUTO FOLLOW read on, pattern %s selected; every %d ms",
      last and string.upper(last) or "none read", cadence)
  end
  local o = E.header
  local origin = o:origin()
  local k = origin and H.factor(origin)
  if not (origin and k) then return end
  local pl = P.placer(origin, k, (H.zoom(origin)))
  local region = Open.region(pl, W.RECT)
  if not region then return end
  local t0 = host.now()
  if stats.lastAt then
    stats.gaps += 1
    stats.gapMs += t0 - stats.lastAt
  end
  stats.lastAt = t0
  stats.pictures += 1
  local asked, err = pcall(host.screen.snapshot, { region = region, key = W.PICTURE_KEY }, function(s, why)
    if not s then
      if not failing then
        failing = true
        log("no picture: %s", tostring(why))
      end
      return
    end
    local tPic = host.now()
    stats.picMs += tPic - t0
    local still, now = mayLook()
    if not (still and now.id == win.id and armed) then return s:release() end
    if O.calibrating() then
      local name = string.format("drumsq-watch-%dpct", pl.zoom or 0)
      if not shots[name] then
        shots[name] = true
        local path = host.path("calibration/" .. name .. ".png")
        pcall(host.screen.saveMarked, path, { snapshot = s, region = region, marks = marksOf(pl) }, function(saved, whyS)
          host.log.info(string.format("[avenger] [calibrate] the watcher's picture -> %s (%s)", path,
            saved and "true" or ("false: " .. tostring(whyS))))
        end)
      end
    end
    local all = {}
    Centre.ask(pl, all)
    DrumSq.ask(pl, all)
    local got, out, lines = { why = {} }, 0, {}
    local function answered()
      out -= 1
      if out > 0 then return end
      local again, w2 = mayLook()
      if not (again and w2.id == win.id and armed) then return end
      decide(win, pl, got, lines, t0, tPic)
    end
    for _, req in ipairs(all) do
      local key = QUESTION_KEY .. req.name
      local function take(value, why2)
        if value ~= nil then
          local made, g = pcall(function() return req.kind == "cells" and P.grid(req, value) or value end)
          if made then got[req.name] = g else got.why[req.name] = tostring(g) end
        else
          got.why[req.name] = why2
        end
        answered()
      end
      local okQ, errQ
      if req.kind == "cells" then
        okQ, errQ = pcall(host.screen.cells, { region = req.rect, cols = req.cols, rows = req.rows, predicate = req.pred,
          snapshot = s, key = key }, take)
      else
        okQ, errQ = pcall(host.screen.profile, { region = req.rect, axes = req.axes or "columns", snapshot = s,
          key = key }, take)
      end
      if okQ then
        out += 1
        busyKeys[key] = true
      else
        got.why[req.name] = tostring(errQ)
      end
    end
    -- The questions keep the pixels they need: the picture goes now.
    s:release()
    if out == 0 then
      out = 1
      answered()
    end
  end)
  if not asked then log("no picture asked: %s", tostring(err)) end
end

-- The cadence the setting asks, or the default with a line in the log.
local function cadenceOf(v)
  if type(v) == "number" and v == math.floor(v) and v >= W.MIN and v <= W.MAX then return v end
  log("drumSqCadence '%s' is no cadence from %d to %d ms; %d instead", tostring(v), W.MIN, W.MAX, W.DEFAULT)
  return W.DEFAULT
end

local function arm(v)
  if token then host.timer.cancel(token) end
  cadence = cadenceOf(v)
  token = host.timer.every(cadence, tick)
  resetStats()
end

-- What page.luau hands over, once its settings are defined: { header = the header overlay, page = ()
-- -> the page object or nil, ringBusy = () -> whether an action of the ring's is out, ringSeen =
-- (keys, values) -> (), ringValues = () -> the ring's values with `central` read and when, or nil,
-- S = state.luau }.
function W.start(env)
  E = env
  arm(host.settings.get("drumSqCadence"))
  host.settings.onChange("drumSqCadence", function(new, old)
    if new == old then return end
    arm(new)
    log("every %d ms", cadence)
  end)
end

return W
