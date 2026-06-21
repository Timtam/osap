-- Demonstrates host.screen: screen size, pixel color, and image search.

local size = host.screen.size()
host.log.info(string.format("Screen size: %dx%d", size.w, size.h))

local p = host.screen.pixel(0, 0)
host.log.info(string.format("Pixel @ 0,0   = %s (r=%d g=%d b=%d)", p.hex, p.r, p.g, p.b))

local c = host.screen.pixel(math.floor(size.w / 2), math.floor(size.h / 2))
host.log.info("Pixel @ center = " .. c.hex)

-- Image search for a bundled probe over a small region. The 8x8 magenta probe is
-- almost certainly not on screen, so this proves the decode+capture+match pipeline
-- runs end to end (returns nil). Point it at a real plugin asset to find a control.
local hit = host.screen.imageSearch("assets/probe.png", { region = { 0, 0, 400, 400 } })
if hit then
  host.log.info(string.format("imageSearch: found probe at %d,%d (%dx%d)", hit.x, hit.y, hit.w, hit.h))
else
  host.log.info("imageSearch: probe not found in region (pipeline OK)")
end

host.speech.output(string.format("Screen is %d by %d.", size.w, size.h), { interrupt = true })
