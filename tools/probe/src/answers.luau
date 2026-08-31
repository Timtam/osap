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

-- The whole section, in the order the answers are worth reading.
function M.all(line, win)
  line("----- what this platform answers -----")
  guard(line, "pointer space", function() M.pointerSpace(line, win) end)
  guard(line, "ownsPoint", function() M.ownsPoint(line, win) end)
  guard(line, "blank OCR", function() M.blankOcr(line, win) end)
  guard(line, "timers", function() M.timerCadence(line) end)
end

return M
