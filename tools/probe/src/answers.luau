-- What this platform can be ASKED, as opposed to what it can be told.
--
-- The rest of the probe records what a window IS. This part puts questions to the platform
-- itself — the ones nobody here can put to it, because the macOS half of this project was
-- written on Windows, blind, and a capability written on one platform and assumed on the other
-- is discovered by a blind tester at the worst possible moment. The point is that the answers
-- arrive in a log the tester was sending anyway, instead of on a checklist somebody has to
-- work through by hand.
--
-- **Everything here is read-only, and that is a rule rather than a preference.** This runs over
-- somebody's real plugin in their real session, and they cannot see what it did. Nothing below
-- clicks, drags, scrolls, types or moves focus. Where a question can only be answered by acting
-- on the interface — what a scroll notch is worth to a control, whether a drag needs the
-- movement in between — it is not asked here at all, and TODO.md says so instead of pretending
-- the probe covers it.

local M = {}

local function floor(v)
  return math.floor(v)
end

-- Every check is wrapped: one that throws must not take the rest of the probe with it. The
-- probe is the tester's only instrument, and half an answer beats a dead key.
local function guard(line, what, fn)
  local ok, err = pcall(fn)
  if not ok then
    line("  %s: FAILED (%s)", what, tostring(err))
  end
end

-- == Do the two coordinate systems agree? =====================================
--
-- The cheapest question here and the one with the largest blast radius. Window geometry comes
-- from the accessibility API; a click is posted into the window server's global space. On
-- Windows those are the same numbers and nobody has ever had to think about it. On a Retina
-- Mac they need not be, and `backend/macos/mod.rs` opens with the rule that keeps them the
-- same — points, never pixels, origin top-left.
--
-- If that rule is broken, nothing reports it. A click simply lands at half or double the
-- intended distance from the origin, on Retina machines only, and the overlay announces
-- "activated" either way. Reading the pointer costs nothing and is the only side-effect-free
-- way to see the input layer's own opinion of where it is: `cursor_pos` builds a null CGEvent
-- and reads its location, which is the very space a click is posted into.
function M.pointerSpace(line, win)
  local cur = host.input and host.input.cursorPos and host.input.cursorPos()
  if not cur then
    line("  pointer: host.input.cursorPos unavailable")
    return
  end
  local b = win.bounds
  local inside = cur.x >= b.x and cur.x < b.x + b.w and cur.y >= b.y and cur.y < b.y + b.h
  line(
    "  pointer at %d,%d — %s the probed window (%d,%d %dx%d)",
    cur.x, cur.y,
    inside and "INSIDE" or "outside",
    b.x, b.y, b.w, b.h
  )
  if not inside then
    -- Not a failure: the pointer is wherever the tester left it, and a keyboard-only session
    -- leaves it anywhere at all. Said plainly so the line is not read as a fault.
    line("     (only meaningful if the pointer was resting over the window — it may not have been)")
  else
    -- The number that matters. Halfway across the window in AX points should be halfway across
    -- it in click points; a factor of two here IS the Retina bug, visible before any click.
    line(
      "     relative to content origin: %d,%d of %dx%d",
      cur.x - win.client.x, cur.y - win.client.y, win.client.w, win.client.h
    )
  end
end

-- == Who is drawn at a point? =================================================
--
-- `host.window.ownsPoint` exists so that a control can decline to click a coordinate that is
-- inside its window's frame but covered by something else — a notification, a tooltip, another
-- application raised over it. Windows answers it; macOS inherits the trait default and answers
-- `nil`, which every caller reads as permission.
--
-- So there are two things worth recording, and the second is the useful one. Whether the
-- binding answers at all, and — whatever it answers — what the window list says is actually
-- lying over the plugin. The second is the data any future macOS implementation gets judged
-- against, and it costs one enumeration.
function M.ownsPoint(line, win)
  if not (host.window and host.window.ownsPoint) then
    line("  ownsPoint: not bound in this build")
    return
  end
  local c = win.client
  local cx, cy = c.x + floor(c.w / 2), c.y + floor(c.h / 2)
  local function ask(what, x, y)
    local ok, v = pcall(host.window.ownsPoint, win.id, x, y)
    line(
      "  ownsPoint(%s at %d,%d) = %s",
      what, x, y,
      ok and (v == nil and "nil (this platform cannot say — read as permission)" or tostring(v))
        or ("ERROR " .. tostring(v))
    )
  end
  ask("window centre", cx, cy)

  -- The negative control, and it has to be EARNED rather than assumed. The first version of
  -- this asked about the top-left of the display, on the reasoning that the probed window is
  -- almost never there — and the very first run answered `true`, correctly, because the window
  -- under test began at -8,-8 and genuinely covered the corner. A control that the subject can
  -- legitimately pass is not a control at all. So: a point provably OUTSIDE the window's own
  -- frame, or none, said out loud.
  local b = win.bounds
  local scr = (host.screen and host.screen.size and host.screen.size()) or { w = 0, h = 0 }
  local ox, oy
  if b.x + b.w + 8 < scr.w then
    ox, oy = b.x + b.w + 8, b.y + floor(b.h / 2)
  elseif b.x - 8 > 0 then
    ox, oy = b.x - 8, b.y + floor(b.h / 2)
  elseif b.y + b.h + 8 < scr.h then
    ox, oy = b.x + floor(b.w / 2), b.y + b.h + 8
  end
  if ox then
    -- An implementation that answers `true` here is answering `true` to everything, which is
    -- the failure that would let every covered click through while looking like it checked.
    ask("outside the window", ox, oy)
  else
    line("  ownsPoint: the window covers the display — no point outside it to test against")
  end

  -- What the enumeration thinks covers the centre. Two limits, and the second is the one worth
  -- remembering: `host.window.list` is not in z-order on macOS — it follows the running
  -- applications — so this cannot say who is on TOP; and overlapping is not the same question
  -- as what a click would HIT. A window can be drawn over a control and still let the click
  -- through, which is exactly what a screen reader's cursor ring does. So this list is
  -- evidence to design against, never a verdict to act on.
  local ok, all = pcall(host.window.list)
  if not ok or not all then
    line("  ownsPoint: window list unavailable (%s)", tostring(all))
    return
  end
  local n = 0
  for _, w in ipairs(all) do
    local b = w.bounds
    if b and cx >= b.x and cx < b.x + b.w and cy >= b.y and cy < b.y + b.h then
      n = n + 1
      line(
        "     overlapping the centre: pid=%d '%s' class='%s' %d,%d %dx%d%s",
        (w.app and w.app.pid) or 0, w.title or "", w.class or "",
        b.x, b.y, b.w, b.h,
        (w.app and win.app and w.app.pid == win.app.pid) and "  <- ours" or ""
      )
    end
  end
  line("  ownsPoint: %d window(s) overlap the centre (not z-ordered on every platform)", n)
end

-- == Does the recogniser invent text? =========================================
--
-- The worst thing this system can do to somebody who cannot check it against the screen is to
-- read them something that was never there. On Windows it did: an empty search box and five
-- tiles whose captions were clipped away all came back as the same invented string, from six
-- different places, because a recognition-only network has no way to answer "nothing here".
-- The Windows fix is a guard that refuses to hand a flat rectangle to a recogniser at all.
--
-- The macOS path notices the same fact — one flat colour, nothing to crop to — and then hands
-- the rectangle over anyway. Whether Apple Vision invents on that input is not knowable from
-- the code: it runs a text DETECTOR first, which can legitimately find nothing, but no
-- confidence floor is applied to what it does return. So it is a measurement, and this is it.
--
-- Two things make the measurement worth trusting:
--
--   * The region is proved uniform rather than assumed. `host.screen.profile` reports, per row
--     and per column, the minimum and maximum luminance of the actual pixels. A row where
--     those are equal is one luminance end to end; a band of such rows all carrying the SAME
--     value is a rectangle that is provably flat, in one capture. Guessing at a blank corner
--     would have made a negative result meaningless.
--   * It is done twice, on bands of different colour. One invented string proves little on its
--     own; the same string out of two different blank rectangles is the exact fingerprint the
--     Windows failure left, and one log settles it without a second round trip to the tester.
--
-- The probe's existing whole-window OCR cannot see any of this. A plugin window is far larger
-- than the size below which the backend does its content-cropping and its retry ladder, so the
-- whole escalation path — including the fast, least-trusted model at the end of it — is never
-- reached by the probe as it stands.

-- How wide a candidate strip is. The first version of this searched the FULL window width for
-- a uniform row and found none at all — over 1920 points of a real interface, essentially no
-- row is one colour end to end, so the test never measured anything. Narrow enough to be flat
-- in ordinary chrome, wide enough that a word would have fitted in it if there had been one.
local STRIP_W = 64
-- Below this there is nothing a recogniser would look at anyway, and a two-row sliver of
-- gradient can be flat by accident.
local MIN_BAND = 8
-- Several places across the window, because one strip can land on the busiest column there is.
local STRIP_AT = { 0.12, 0.30, 0.50, 0.70, 0.88 }

-- Runs of rows within ONE strip that are each a single flat luminance and all carry the same
-- value. Each row having min == max proves that row is flat across the strip; requiring the
-- same value across the run proves the rows agree with each other.
local function flatRuns(prof)
  local rows = prof.rows
  if not (rows and rows.min and rows.max) then return {} end
  local runs, i, n = {}, 1, #rows.min
  while i <= n do
    if rows.min[i] == rows.max[i] then
      local lum, j = rows.min[i], i
      while j + 1 <= n and rows.min[j + 1] == rows.max[j + 1] and rows.min[j + 1] == lum do
        j = j + 1
      end
      runs[#runs + 1] = { first = i, last = j, lum = lum, height = j - i + 1 }
      i = j + 1
    else
      i = i + 1
    end
  end
  return runs
end

-- Reads a rectangle back and says whether every pixel in it is the one luminance. The strip
-- search proves each ROW is flat across 64 points; this re-reads the exact rectangle that is
-- about to be recognised, on both axes, so the claim rests on the same pixels the recogniser
-- will see rather than on a neighbouring capture.
local function confirmFlat(rect, lum)
  local p = host.screen.profile({ region = rect, axes = "both" })
  if not p or not p.columns or not p.rows then return false, "profile returned nothing" end
  for _, axis in ipairs({ p.columns, p.rows }) do
    for k = 1, #axis.min do
      if axis.min[k] ~= axis.max[k] or axis.min[k] ~= lum then
        return false,
          string.format("pixel %d reads %d..%d, expected %d", k, axis.min[k], axis.max[k], lum)
      end
    end
  end
  return true
end

local function readRegion(line, what, rect)
  local ok, res = pcall(function()
    return host.ocr.recognize({ region = rect })
  end)
  if not ok or not res then
    line("     %s: OCR failed (%s)", what, tostring(res))
    return nil
  end
  local words, texts = res.words or {}, {}
  line("     %s: %d word(s)%s", what, #words, #words == 0 and "  <- nothing invented" or "")
  for _, wd in ipairs(words) do
    -- Verbatim, with coordinates. A string here IS the finding, and paraphrasing it would
    -- destroy the one comparison that matters: whether two different blanks read the same.
    line("        '%s' at %d,%d %dx%d", wd.text, wd.x, wd.y, wd.w, wd.h)
    texts[#texts + 1] = wd.text
  end
  return table.concat(texts, " ")
end

function M.blankOcr(line, win)
  local c = win.client
  if c.w < 100 or c.h < 60 then
    line("  blank OCR: window too small to carve a test region out of")
    return
  end

  -- One capture per strip. A screen touch costs a compositor frame whatever its size, so this
  -- is five frames — worth it once, on a key the tester pressed deliberately.
  local cands = {}
  for _, frac in ipairs(STRIP_AT) do
    local w = math.min(STRIP_W, c.w - 4)
    local x1 = math.max(c.x, math.min(c.x + floor(c.w * frac) - floor(w / 2), c.x + c.w - w))
    local prof = host.screen.profile({
      region = { x1, c.y, x1 + w, c.y + c.h },
      axes = "rows",
    })
    if prof and prof.rows then
      for _, r in ipairs(flatRuns(prof)) do
        if r.height >= MIN_BAND then
          r.x, r.w = x1, w
          r.y = prof.y + r.first - 1
          r.red = (prof.rows.r and prof.rows.r[r.first]) or -1
          r.green = (prof.rows.g and prof.rows.g[r.first]) or -1
          r.blue = (prof.rows.b and prof.rows.b[r.first]) or -1
          r.limit = prof.y + prof.h
          cands[#cands + 1] = r
        end
      end
    end
  end

  line("  blank OCR: %d flat region(s) found in %d strip(s) of %d points",
    #cands, #STRIP_AT, STRIP_W)
  if #cands == 0 then
    line("     nothing provably uniform here — no measurement made rather than a guessed one")
    return
  end
  table.sort(cands, function(a, b) return a.height > b.height end)

  -- Two of DIFFERENT colour, because the fingerprint being looked for is the same invented
  -- string arriving out of two unlike places.
  local picked = { cands[1] }
  for k = 2, #cands do
    if cands[k].lum ~= cands[1].lum then
      picked[2] = cands[k]
      break
    end
  end

  local readings = {}
  for idx, band in ipairs(picked) do
    local h = math.min(20, band.height)
    local rect = { band.x, band.y, band.x + band.w, band.y + h }
    line(
      "  region %d: %d,%d %dx%d, luminance %d, colour %d,%d,%d (flat run was %d row(s) tall)",
      idx, band.x, band.y, band.w, h, band.lum,
      floor(band.red + 0.5), floor(band.green + 0.5), floor(band.blue + 0.5), band.height
    )
    local flat, why = confirmFlat(rect, band.lum)
    if not flat then
      -- Something moved on screen between the two captures, or the run was not what it looked
      -- like. Either way the region has stopped being proof of anything, so no reading is taken
      -- from it: a measurement whose premise failed is worse than none at all.
      line("     not uniform on re-reading (%s) — skipped rather than reported", tostring(why))
    else
      readings[idx] = readRegion(line, string.format("region %d, proved flat", idx), rect)
    end
  end

  if readings[1] and readings[2] and readings[1] ~= "" and readings[1] == readings[2] then
    line(
      "  blank OCR: THE SAME STRING CAME BACK FROM TWO DIFFERENT BLANK RECTANGLES ('%s') — "
        .. "that is invention, not a faint reading",
      readings[1]
    )
  end

  -- One more region, deliberately NOT flat: the boundary where the tallest run ends. A region
  -- with a real edge in it but no text takes a different path through the backend — the crop
  -- finds something, so the retry ladder is allowed to escalate, ending at the fastest and
  -- least trusted model. A perfectly flat rectangle never reaches that rung, so testing only
  -- flat ones would leave the worse case unmeasured.
  local band = picked[1]
  local edge = band.y + band.height
  if edge + 8 < band.limit then
    local y1 = math.max(band.y, edge - 8)
    local rect = { band.x, y1, band.x + band.w, y1 + 16 }
    line("  region 3: %d,%d %dx16 — straddles the edge of region 1, so it is NOT flat",
      band.x, y1, band.w)
    readRegion(line, "region 3, an edge but no text", rect)
  end
end

-- == Does a timer actually tick at the rate everything assumes? ===============
--
-- Every wait in the overlay runtime is built on `host.timer`, and the one that matters most —
-- the watch that waits for a value to change before announcing it — polls at 100 ms and gives
-- up at 900. Those are wall-clock numbers on Windows. On macOS the same timers are drained by
-- the GUI toolkit's own tick, and nothing has ever confirmed that cadence there.
--
-- It matters in a specific and quiet way. If a tick is really 400 ms, a watch's "900 ms" is
-- nearer three seconds, and a control that did change reports late while one that did not
-- holds the keyboard's attention for far longer than intended. Neither looks like a timing
-- fault from the outside; both feel like the plugin being slow.
--
-- Ten hops of 100 ms, measured against the clock. Read-only, costs a second, and its ABSENCE
-- from the log is itself an answer — it would mean the timer never fired at all.
function M.timerCadence(line, hops)
  if not (host.timer and host.timer.after and host.now) then
    line("  timers: host.timer or host.now unavailable")
    return
  end
  hops = hops or 10
  local t0, prev = host.now(), host.now()
  local worst, best, n = 0, math.huge, 0
  local function hop()
    local now = host.now()
    local d = now - prev
    prev = now
    n = n + 1
    if d > worst then worst = d end
    if d < best then best = d end
    if n < hops then
      host.timer.after(100, hop)
    else
      line(
        "  timers: %d x 100 ms took %d ms (shortest %d, longest %d) — a 900 ms watch deadline "
          .. "is really about %d ms here",
        hops, host.now() - t0, best, worst, floor(9 * ((host.now() - t0) / hops))
      )
    end
  end
  host.timer.after(100, hop)
end

-- == What does asking "which window is in front" actually cost? ================
--
-- The biggest single stall in the first tester's log, and it was not the one that was fixed.
-- `window.active()` blocked the event loop fourteen times, worst case 2605 ms — during
-- ordinary use, a moment after a plugin menu closed.
--
-- Since then the backend has changed twice, and this measurement is what says whether the
-- changes did anything. It now stops asking an application that has just failed to answer
-- (it used to charge the messaging timeout three times over per question), and while an
-- application is in that penalty box it serves the window that application last described,
-- rather than reporting no active window at all.
--
-- ONE ASK PER TICK, and that is the whole point of the rewrite. The host answers this
-- question from a per-tick cache — the same overlay asking twice in one tick costs one OS
-- call, which is the cache doing its job — so forty asks in a row, as this used to do,
-- measured thirty-nine cache hits and one real call. It reported a beautiful distribution of
-- zeroes and would have confirmed any fix at all, including one that made things worse. Each
-- ask now sits in its own timer hop, so each one crosses a tick boundary and reaches the
-- backend.
--
-- Two things to know when reading the result. A fast ask may still be a cache hit if another
-- module asked first in the same tick, so a low median is weak evidence; the TAIL is the
-- finding, because nothing in the cache can invent a slow answer. And an ask served from the
-- backend's memory of a wedged application is fast BY DESIGN — the log line "serving the
-- window it last described" is what tells those apart, so read this next to the log rather
-- than on its own.
function M.activeWindowCost(line, hops)
  if not (host.window and host.window.active and host.now and host.timer and host.timer.after) then
    line("  window.active cost: host.window.active, host.now or host.timer unavailable")
    return
  end
  hops = hops or 40
  local times = {}
  local worst, best, total = 0, math.huge, 0
  local n = 0
  local function ask()
    local t0 = host.now()
    host.window.active()
    local d = host.now() - t0
    times[#times + 1] = d
    total = total + d
    if d > worst then worst = d end
    if d < best then best = d end
    n = n + 1
    if n < hops then
      -- Longer than one tick on purpose. A delay under the tick length can come due inside
      -- the same pump iteration that scheduled it, which would put two asks in one epoch and
      -- put the cache back in the measurement.
      host.timer.after(20, ask)
      return
    end
    table.sort(times)
    local median = times[floor((#times + 1) / 2)]
    local p90 = times[math.max(1, floor(#times * 0.9))]
    line(
      "  window.active cost: %d asks, one per tick — best %d ms, median %d ms, 90th %d ms, "
        .. "worst %d ms, %d ms in total",
      hops, best, median, p90, worst, total
    )
    -- The tail named separately, because the tail is the finding. A single ask over 300 ms is
    -- already past what the event tap tolerates.
    local slow = 0
    for _, d in ipairs(times) do
      if d >= 100 then slow = slow + 1 end
    end
    line(
      "  window.active cost: %d of %d asks took 100 ms or more%s",
      slow, hops,
      slow == 0
        and "  <- no stall seen in this run; check the log for \"serving the window it last "
          .. "described\" before reading that as healthy"
        or "  <- this is the stall"
    )
  end
  host.timer.after(20, ask)
end

-- == Is a big OCR read cheaper than several small ones? ========================
--
-- Measured on the first tester's machine, from two lines in the same log: a 1013x580 pt read
-- finished in 730 ms, while a 64x16 pt read was ABANDONED at its 457 ms deadline. So cost is
-- not proportional to area — it is close to flat per request — and that is why an ordinary
-- Tab step costs 90-250 ms there against 23-34 ms on Windows.
--
-- If that holds, an overlay reading three small fields should read ONE region covering all
-- three and split it by position afterwards. That would be a real change to the runtime, so
-- it wants a measurement rather than an inference from two incidental log lines.
--
-- Reads the window's own content area once, then three small strips inside it, and reports
-- both — best of three rounds each, because one pair of timings on a machine this variable is
-- a single sample.
--
-- It DOES interpret, and the sentence that used to say otherwise was the problem: the verdict
-- underneath compared the two totals with no margin at all, so five per cent read as "worth
-- building". The rule it applies now is written into the line it prints, so a reader can
-- disagree with the threshold instead of only with the conclusion.
function M.ocrBatching(line, win)
  if not (host.ocr and host.ocr.recognize and win and win.client) then
    line("  ocr batching: host.ocr or the window's client area unavailable")
    return
  end
  local c = win.client
  if c.w < 240 or c.h < 120 then
    line("  ocr batching: the window is too small to split meaningfully (%dx%d)", c.w, c.h)
    return
  end
  -- A band across the top of the content, where a plugin's read-outs usually sit, and three
  -- strips inside that same band. Same pixels either way, so the comparison is honest.
  local band = { c.x, c.y, c.x + c.w, c.y + math.min(120, floor(c.h / 4)) }
  local third = floor((band[3] - band[1]) / 3)

  -- Three rounds, and the CHEAPEST of each kept. One pair of timings is a single sample on a
  -- machine where an ordinary Tab step was measured anywhere between 90 and 250 ms, and
  -- scheduling can only ever inflate a sample, never deflate it — so the floor is the honest
  -- estimator for "what does this cost when nothing is in the way".
  local ROUNDS = 3
  local oneMs, manyMs = math.huge, math.huge
  local oneWords, manyWords, failed = 0, 0, 0
  for _ = 1, ROUNDS do
    local t0 = host.now()
    local okOne, one = pcall(function() return host.ocr.recognize({ region = band }) end)
    local ms = host.now() - t0
    if ms < oneMs then
      oneMs = ms
      oneWords = (okOne and one and one.words and #one.words) or 0
    end

    local roundMs, roundWords = 0, 0
    for i = 0, 2 do
      local strip = { band[1] + i * third, band[2], band[1] + (i + 1) * third, band[4] }
      local t1 = host.now()
      local ok, res = pcall(function() return host.ocr.recognize({ region = strip }) end)
      roundMs = roundMs + (host.now() - t1)
      if ok and res and res.words then
        roundWords = roundWords + #res.words
      else
        failed = failed + 1
      end
    end
    if roundMs < manyMs then
      manyMs = roundMs
      manyWords = roundWords
    end
  end

  -- The number that actually decides, rather than the two it is derived from. Batching would
  -- replace up to three reads with one, so what it saves is two EXTRA requests — and that is
  -- the figure to compare against the cost of building it.
  local extra = floor((manyMs - oneMs) / 2)
  line(
    "  ocr batching: one %dx%d region took %d ms and read %d word(s); the same band as three "
      .. "strips took %d ms and read %d word(s); best of %d round(s)%s",
    band[3] - band[1], band[4] - band[2], oneMs, oneWords,
    manyMs, manyWords, ROUNDS,
    failed > 0 and string.format(" (%d strip(s) failed or timed out)", failed) or ""
  )
  -- The rule is stated so it can be disagreed with, which is the whole difference between a
  -- measurement and a verdict.
  --
  -- It used to be `manyMs > oneMs`: ANY difference at all read as "worth building". The first
  -- run of this on a real Mac returned 354 ms against 374 — five per cent — and duly declared
  -- a change to the runtime worth making, days before a session that could not be repeated.
  -- An instrument whose threshold is zero cannot tell its finding from its noise, and this one
  -- was about to cost a feature nobody needed.
  --
  -- WORTH_MS is what one extra Vision request has to cost before removing two of them is worth
  -- a change to shared runtime code. Fifty, because an announcement on that machine already
  -- costs 90 to 250 ms: saving a hundred is something a person notices, and saving twenty is
  -- not. A guess, said out loud, with the number beside it so the next reader can pick another.
  local WORTH_MS = 50
  line(
    "  ocr batching: an extra read costs about %d ms — %s",
    extra,
    extra >= WORTH_MS
      and ("that is more than the " .. WORTH_MS
        .. " ms this is worth building for, so batching a row of read-outs into one region "
        .. "would pay for itself")
      or ("that is under the " .. WORTH_MS
        .. " ms this would be worth building for, so leave the runtime alone — three reads "
        .. "are barely dearer than one here, and the per-request cost is not the thing to fix")
  )
end

-- == Does the module-error window work on a Mac? ==============================
--
-- Not a measurement the probe can take: it needs a person to say what they heard and whether
-- the window could be reached. So the probe does the one part a person cannot do reliably —
-- provoke a real module error at a known moment — and the briefing asks the three questions.
--
-- A real error rather than a simulated one: this raises it from inside a timer callback,
-- which is the path `report_callback_error` actually takes, so what appears is the genuine
-- article and not something shaped like it.
--
-- Deliberately LAST in the run, and behind its own delay: it puts a window on screen, and
-- everything above should be in the log before anything steals the focus.
function M.provokeErrorWindow(line)
  if not (host.timer and host.timer.after) then
    line("  error window: host.timer unavailable, cannot provoke one")
    return
  end
  line("  error window: raising a real module error in three seconds — expect a window titled")
  line("  error window: 'Module error: com.tool.probe'. Three things to report:")
  line("     1. does it have a Dock icon and an entry in the app switcher (Cmd-Tab)")
  line("     2. does VoiceOver read the message when it opens")
  line("     3. with the module manager ALSO open, close the manager — does the error window")
  line("        keep its Dock icon (that is the refcount, and it is the bug this was written")
  line("        to prevent)")
  -- Three seconds, not one. Two of the answers above run on timer hops of their own — the
  -- timer cadence takes about a second, and window.active cost about 1.2, because it now
  -- deliberately crosses a tick boundary per ask — and a window appearing on screen while
  -- their lines are still arriving makes the run harder to read and harder to describe.
  -- Raising the error does NOT stop them (a callback error is logged and shown, the module
  -- stays loaded), so this is about the order things reach the tester, not about losing them.
  host.timer.after(3000, function()
    error("this is the probe, raising an error on purpose so the error window can be looked at")
  end)
end

-- The whole section, in the order the answers are worth reading.
function M.all(line, win)
  line("----- what this platform answers -----")
  guard(line, "pointer space", function() M.pointerSpace(line, win) end)
  guard(line, "ownsPoint", function() M.ownsPoint(line, win) end)
  guard(line, "blank OCR", function() M.blankOcr(line, win) end)
  guard(line, "timers", function() M.timerCadence(line) end)
  guard(line, "window.active cost", function() M.activeWindowCost(line) end)
  guard(line, "ocr batching", function() M.ocrBatching(line, win) end)
  -- Last, because it raises a window: everything above wants to be in the log first.
  guard(line, "error window", function() M.provokeErrorWindow(line) end)
end

return M
