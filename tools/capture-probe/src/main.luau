-- Does a screen capture come back at all on this macOS, through which system function, and
-- how fast — and, above all, does ScreenCaptureKit's answer arrive while the pump thread is
-- the thread waiting for it?
--
-- Not for a tester — for CI. Screen capture moved to ScreenCaptureKit, whose rectangle capture
-- answers through a completion handler, and capture.rs waits for that handler on a channel.
-- Nothing Apple documents says which queue the handler runs on. If it needs the main thread,
-- every capture asked from the pump — every pixel read and every OCR read, which is most of
-- what an overlay does — would wait out its deadline and then fall back to functions a newer
-- macOS may no longer have. A runner cannot grant Screen Recording, but a refusal travels back
-- through the same handler as an image does, so "did anything come back within the deadline"
-- is answerable here, and it is the question that decides whether the next Mac session reads
-- the screen at all.
--
-- Everything it prints is prefixed, so the job asserts on lines rather than on someone reading
-- a log.

local MARK = "CAPTURE PROBE:"

local function report(fmt, ...)
  if select("#", ...) > 0 then
    host.log.info(MARK .. " " .. string.format(fmt, ...))
  else
    host.log.info(MARK .. " " .. fmt)
  end
end

local now = host.now or function() return 0 end

-- A pixel read answers in whatever shape the binding gives; described rather than assumed.
local function describe(v)
  if type(v) == "table" then
    local r = v.r or v[1]
    local g = v.g or v[2]
    local b = v.b or v[3]
    return string.format("%s,%s,%s", tostring(r), tostring(g), tostring(b))
  end
  return tostring(v)
end

-- Snapshots: answers that come back from the `screen-capture` thread in a callback — a plain
-- `snapshotAsync` and a change wait on a screen nobody touches, asked first and whatever else
-- happens, and a text read of the snapshot taken on the pump, asked only when that snapshot
-- exists — and that one picture read several ways. The job compares the callbacks that came with
-- the ones asked for: a missing one is a thread that never answered, never a snapshot that failed
-- on the pump, which has a line of its own, nor a call that raised, which says THREW and is not
-- counted as asked.
local SNAP_REGION = { 0, 0, 200, 60 }
local POINTS = { { 10, 10 }, { 100, 30 }, { 199, 59 } }
local answered = 0
local expected = 0

local function askTheThread()
  local t3 = now()
  local oka, aerr = pcall(host.screen.snapshotAsync, { region = SNAP_REGION }, function(s, why2, info)
    answered = answered + 1
    report("snapshotAsync answered after %d ms: %s (waited %d ms, %d picture(s))",
      now() - t3, s and tostring(s) or ("nil, " .. tostring(why2)), info.waited, info.frames)
    if s then s:release() end
  end)
  if oka then expected = expected + 1 else report("snapshotAsync THREW: %s", tostring(aerr)) end

  local t4 = now()
  local okw, werr = pcall(host.screen.snapshotAsync, { region = SNAP_REGION, change = { timeout = 300 } },
    function(s, why2, info)
      answered = answered + 1
      report("change wait answered after %d ms: %s, changed=%s settled=%s rebased=%s, %d picture(s), %.1f ms a round",
        now() - t4, s and "a picture" or ("nil, " .. tostring(why2)), tostring(info.changed), tostring(info.settled),
        tostring(info.rebased), info.frames, info.frames > 0 and info.waited / info.frames or 0)
      if s then s:release() end
    end)
  if okw then expected = expected + 1 else report("change wait THREW: %s", tostring(werr)) end
end

local function snapshotLines()
  askTheThread()

  local t = now()
  local ok, snap, why = pcall(host.screen.snapshot, { region = SNAP_REGION })
  if not ok then
    report("snapshot 200x60 THREW: %s", tostring(snap))
    return
  end
  if not snap then
    report("snapshot 200x60 returned nil in %d ms: %s", now() - t, tostring(why))
    return
  end
  report("snapshot 200x60 returned in %d ms: %s", now() - t, tostring(snap))

  local t1 = now()
  local okl, live, lwhy = pcall(host.screen.pixels, POINTS)
  if okl then
    report("pixels of 3 points from the screen in %d ms: %s", now() - t1, live and "3 colours" or ("nil, " .. tostring(lwhy)))
  else
    report("pixels from the screen THREW: %s", tostring(live))
  end
  local oks, fromSnap = pcall(host.screen.pixels, POINTS, { snapshot = snap })
  if not oks then
    report("pixels from the snapshot THREW: %s", tostring(fromSnap))
  else
    local agree = fromSnap ~= nil
    for i, p in ipairs(POINTS) do
      local okp, c = pcall(host.screen.pixel, p[1], p[2], { snapshot = snap })
      if not okp then
        report("pixel from the snapshot THREW: %s", tostring(c))
        agree = false
        break
      end
      if not (c and fromSnap and c.hex == fromSnap[i].hex) then agree = false end
    end
    report("pixels and pixel from the snapshot %s", agree and "agree" or "DIFFER")
  end

  local t2 = now()
  local oksv, saved, swhy = pcall(host.screen.save, "probe-snapshot.png", { snapshot = snap })
  if oksv then
    report("save of the snapshot returned in %d ms: %s%s", now() - t2, tostring(saved), swhy and (", " .. swhy) or "")
  else
    report("save of the snapshot THREW: %s", tostring(saved))
  end

  local t5 = now()
  local oko, oerr = pcall(host.ocr.read, SNAP_REGION, { snapshot = snap }, function(r)
    answered = answered + 1
    report("read of the snapshot answered after %d ms: %s, %d word(s)%s",
      now() - t5, r.status, #r.words, r.error and (", " .. r.error) or "")
  end)
  if oko then expected = expected + 1 else report("read of the snapshot THREW: %s", tostring(oerr)) end
  snap:release()
end

-- Calls `k` once the snapshot callbacks asked for have come, or five seconds have passed, after
-- saying how many came of how many.
local function afterSnapshots(k)
  local started = now()
  local function look()
    if answered >= expected or now() - started > 5000 then
      report("snapshot callbacks %d of %d answered", answered, expected)
      k()
    else
      host.timer.after(100, look)
    end
  end
  host.timer.after(100, look)
end

-- After the loop has turned once, so the capture is asked from the pump exactly as an overlay
-- would ask it, not from inside module loading.
host.timer.after(1500, function()
  report("asking from the pump thread")

  local t0 = now()
  local ok, px = pcall(host.screen.pixel, 10, 10)
  report("pixel(10,10) %s in %d ms: %s", ok and "returned" or "THREW", now() - t0, describe(px))

  local t1 = now()
  local ok2, res = pcall(host.ocr.recognize, { region = { 0, 0, 200, 60 } })
  if ok2 and type(res) == "table" then
    report("recognize 200x60 returned in %d ms: %d word(s), skipped=%s",
      now() - t1, #(res.words or {}), tostring(res.skipped))
  else
    report("recognize 200x60 THREW in %d ms: %s", now() - t1, tostring(res))
  end

  -- A second pixel read outside the tile, so the second capture's cost is measured too: the
  -- first one pays whatever ScreenCaptureKit costs to start.
  local t2 = now()
  local ok3, px2 = pcall(host.screen.pixel, 600, 400)
  report("pixel(600,400) %s in %d ms: %s", ok3 and "returned" or "THREW", now() - t2, describe(px2))

  snapshotLines()

  -- A line only a live process can write, after every call that could have stopped it — the
  -- snapshot callbacks included, which come from another thread.
  afterSnapshots(function()
    report("still alive after capturing")

    -- Late reads, for the Windows job's copy of this probe that declares desktop duplication.
    -- The reads above come while duplication is still opening, so they go the standard way;
    -- these come once it is open, and are the ones that show the path itself works. The
    -- macOS jobs stop the process at the line above, before these run.
    host.timer.after(5000, function()
      local t3 = now()
      local ok4, px3 = pcall(host.screen.pixel, 10, 10)
      report("late pixel(10,10) %s in %d ms: %s", ok4 and "returned" or "THREW", now() - t3, describe(px3))
      local t4 = now()
      local ok5, res2 = pcall(host.ocr.recognize, { region = { 0, 0, 200, 60 } })
      report("late recognize 200x60 %s in %d ms",
        (ok5 and type(res2) == "table") and "returned" or "THREW", now() - t4)
      -- One more turn of the loop, so the observation line counting these reads is written.
      host.timer.after(500, function()
        pcall(host.screen.pixel, 600, 400)
        report("late reads done")
      end)
    end)
  end)
end)
