-- Demonstrates host.screen: screen size, pixel color, and image search.
--
-- In a timer's handler rather than at the top level: a handler is where a call can wait
-- for what it asked, and a module's top level is not (see "Where it waits" in
-- docs/api/screen.md). Of the calls below, `pixel` and `snapshot` wait here for the screen and
-- `imageSearch` for the image worker while the event loop goes on — a pixel of the snapshot
-- touches no screen and answers at once — and `size` runs on the event loop wherever it is called.

host.timer.after(0, function()
  local size = host.screen.size()
  host.log.info(string.format("Screen size: %dx%d", size.w, size.h))

  local p = host.screen.pixel(0, 0)
  host.log.info(string.format("Pixel @ 0,0   = %s (r=%d g=%d b=%d)", p.hex, p.r, p.g, p.b))

  local c = host.screen.pixel(math.floor(size.w / 2), math.floor(size.h / 2))
  host.log.info("Pixel @ center = " .. c.hex)

  -- One picture of the top-left corner, waited for in this handler, then read without touching
  -- the screen again.
  local snap, why = host.screen.snapshot({ region = { 0, 0, 64, 64 } })
  if snap then
    local corner = host.screen.pixel(0, 0, { snapshot = snap })
    host.log.info("Snapshot " .. tostring(snap) .. ": pixel @ 0,0 = " .. corner.hex)
    snap:release()
  else
    host.log.info("Snapshot: none (" .. why .. ")")
  end

  -- Image search for a bundled probe over a small region. The 8x8 magenta probe is
  -- almost certainly not on screen, so this proves the decode+capture+match pipeline
  -- runs end to end (returns nil). Point it at a real plugin asset to find a control.
  local hit = host.screen.imageSearch("assets/probe.png", { region = { 0, 0, 400, 400 } })   -- waits here
  if hit then
    host.log.info(string.format("imageSearch: found probe at %d,%d (%dx%d)", hit.x, hit.y, hit.w, hit.h))
  else
    host.log.info("imageSearch: probe not found in region (pipeline OK)")
  end

  host.speech.output(string.format("Screen is %d by %d.", size.w, size.h), { interrupt = true })
end)
