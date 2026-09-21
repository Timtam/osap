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

  -- A line only a live process can write, after every call that could have stopped it.
  host.timer.after(1500, function()
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
