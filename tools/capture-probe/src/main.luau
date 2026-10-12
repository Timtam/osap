-- Does a screen capture come back at all on this macOS, through which system function, and
-- how fast, from asking to the answer — and, above all, does ScreenCaptureKit's answer arrive
-- while the capture thread waits for it?
--
-- Not for a tester — for CI. Screen capture moved to ScreenCaptureKit, whose rectangle capture
-- answers through a completion handler, and capture.rs waits for that handler on a channel.
-- Nothing Apple documents says which queue the handler runs on. Every read below is made in a
-- handler, as an overlay makes it: the handler waits while the `screen-capture` thread takes the
-- picture or the points, and the event loop never captures for it — a capture there would be the
-- loop guard's `[loop] screen capture on the event loop` line, which CI fails on. If the answer
-- does not come to the thread that waits, every pixel read and every OCR read, which is most of
-- what an overlay does, would wait out its deadline and then fall back to functions a newer macOS
-- may no longer have. A runner cannot grant Screen Recording, but a refusal travels back through
-- the same handler as an image does, so "did anything come back within the deadline" is
-- answerable here, and it is the question that decides whether the next Mac session reads the
-- screen at all.
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
-- happens, and a text read of the snapshot the timer's handler waited for, asked only when that
-- snapshot exists — and that one picture read several ways. The job compares the callbacks that
-- came with the ones asked for: a missing one is a thread that never answered, never a snapshot
-- that answered nil, which has a line of its own, nor a call that raised, which says THREW and is
-- not counted as asked.
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

  -- Without a callback, in this timer's handler: the handler waits for the picture, which the
  -- `screen-capture` thread takes, and the time is from asking to the answer — the thread's queue
  -- and the tick that hands it over included.
  local t = now()
  local ok, snap, why = pcall(host.screen.snapshot, { region = SNAP_REGION })
  if not ok then
    report("snapshot 200x60 THREW: %s", tostring(snap))
    return
  end
  if not snap then
    report("snapshot 200x60 answered nil after %d ms (from asking to the answer): %s", now() - t, tostring(why))
    return
  end
  report("snapshot 200x60 waited %d ms (from asking to the answer): %s", now() - t, tostring(snap))

  local t1 = now()
  local okl, live, lwhy = pcall(host.screen.pixels, POINTS)
  if okl then
    report("pixels of 3 points from the screen waited %d ms (from asking to the answer): %s", now() - t1,
      live and "3 colours" or ("nil, " .. tostring(lwhy)))
  else
    report("pixels from the screen THREW: %s", tostring(live))
  end
  -- One point read live — waited for, read as `pixel` reads it, at point resolution on a Mac — and
  -- from the snapshot, which a Mac takes at the display's own resolution and draws down: whether
  -- the two ways give the same colour. The region is the menu bar's corner, which does not move.
  local okv, liveOne = pcall(host.screen.pixel, POINTS[1][1], POINTS[1][2])
  local okt, snapOne = pcall(host.screen.pixel, POINTS[1][1], POINTS[1][2], { snapshot = snap })
  if not (okv and okt) then
    report("live pixel or the snapshot's THREW: %s", tostring(okv and snapOne or liveOne))
  else
    local same = liveOne ~= nil and snapOne ~= nil and liveOne.hex == snapOne.hex
    report("live pixel and the snapshot's at %d,%d %s: %s and %s", POINTS[1][1], POINTS[1][2],
      same and "agree" or "DIFFER", describe(liveOne), describe(snapOne))
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

  -- Waited for, as the image worker encodes and writes it.
  local t2 = now()
  local oksv, saved, swhy = pcall(host.screen.save, "probe-snapshot.png", { snapshot = snap })
  if oksv then
    report("save of the snapshot waited %d ms (from asking to the answer): %s%s", now() - t2, tostring(saved), swhy and (", " .. swhy) or "")
  else
    report("save of the snapshot THREW: %s", tostring(saved))
  end

  local t5 = now()
  local oko, oerr = pcall(host.ocr.recognize, SNAP_REGION, { snapshot = snap }, function(r)
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

-- Six points far enough apart to be one capture of their box, asked in one `pixels` wait.
local SIX = { { 10, 10 }, { 60, 20 }, { 110, 30 }, { 160, 40 }, { 199, 50 }, { 5, 59 } }

-- After the loop has turned once, so the capture is asked from a handler exactly as an overlay
-- would ask it, not from inside module loading. Every read below waits in this timer's handler
-- while the `screen-capture` thread takes it; its time is from asking to the answer.
host.timer.after(1500, function()
  report("asking in a handler")

  local t0 = now()
  local ok, px = pcall(host.screen.pixel, 10, 10)
  report("pixel(10,10) %s %d ms (from asking to the answer): %s", ok and "waited" or "THREW", now() - t0, describe(px))

  local t6 = now()
  local ok6, six, why6 = pcall(host.screen.pixels, SIX)
  report("pixels of 6 points %s %d ms (from asking to the answer): %s", ok6 and "waited" or "THREW", now() - t6,
    not ok6 and tostring(six) or (six and "6 colours" or ("nil, " .. tostring(why6))))

  -- Without a callback, in this timer's handler: the handler waits for the reading, and the
  -- time is the wait. A capture that failed is a reading with status "failed", not a raise.
  local t1 = now()
  local ok2, res = pcall(host.ocr.recognize, { region = { 0, 0, 200, 60 } })
  if ok2 and type(res) == "table" and res.status ~= "failed" then
    report("recognize 200x60 returned in %d ms: %d word(s), skipped=%s",
      now() - t1, #(res.words or {}), tostring(res.skipped))
  elseif ok2 and type(res) == "table" then
    report("recognize 200x60 answered a failed reading in %d ms: %s", now() - t1, tostring(res.error))
  else
    report("recognize 200x60 THREW in %d ms: %s", now() - t1, tostring(res))
  end

  -- A second pixel read, far from the first, so the second capture's cost is measured too: the
  -- first one pays whatever ScreenCaptureKit costs to start.
  local t2 = now()
  local ok3, px2 = pcall(host.screen.pixel, 600, 400)
  report("pixel(600,400) %s %d ms (from asking to the answer): %s", ok3 and "waited" or "THREW", now() - t2, describe(px2))

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
      report("late pixel(10,10) %s %d ms (from asking to the answer): %s", ok4 and "waited" or "THREW", now() - t3, describe(px3))
      local t4 = now()
      local ok5, res2 = pcall(host.ocr.recognize, { region = { 0, 0, 200, 60 } })
      local how = "THREW"
      if ok5 and type(res2) == "table" then
        how = res2.status == "failed" and ("answered a failed reading (" .. tostring(res2.error) .. ")") or "returned"
      end
      report("late recognize 200x60 %s in %d ms", how, now() - t4)
      -- One more read half a second on, and the line the Windows job waits for before it stops
      -- the process. These reads wait on the capture thread, so the observation line no longer
      -- counts them; the first-read comparison's `[capture]` line says duplication answered.
      host.timer.after(500, function()
        pcall(host.screen.pixel, 600, 400)
        report("late reads done")
      end)
    end)
  end)
end)
