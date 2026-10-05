-- Guard probe library — part of tools/guard-probe (see the test list there). A code module: its
-- code runs in the VM of every module that depends on it — Guard probe B's and Guard probe C's —
-- so it adds its 256 MiB to their limit, and a loop in its own code stops all three (TEST 13).
-- A loop in B's code that calls ready() is B's (TEST 16), and pixelForever's slow host calls are
-- one window's matter, not the library's: they stop only B (TEST 17).

local M = {}

function M.name()
  return "Guard probe library"
end

-- A helper with a short loop of its own: most of the time a loop that calls it spends is in it.
function M.ready()
  local s = 0
  for i = 1, 20 do s += i end
  return false
end

-- A loop in the library's own file that would run 30 seconds: the stop is laid at its door.
function M.spin()
  local started, n = os.clock(), 0
  while os.clock() - started < 30 do for _ = 1, 100000 do n += 1 end end
  return n
end

-- A loop over a host call that waits for the screen, made by the library for the module that
-- calls it.
function M.pixelForever()
  local started = os.clock()
  while os.clock() - started < 30 do local c = host.screen.pixel(0, 0) end
end

return M
