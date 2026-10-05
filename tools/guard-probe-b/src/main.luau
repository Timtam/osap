-- Guard probe B — part of tools/guard-probe (see the test list there). It declares 300 MiB and
-- runs Guard probe library's code, so its VM may use 556 MiB. It holds the slot "guard-probe" at
-- specificity 1; Guard probe takes it at 2, and then B's onDeactivate runs out of memory (TEST 3),
-- or at 3, and then it loops (TEST 10). Its Ctrl+Alt+Win+6 loops in the library's code (TEST 13),
-- its Ctrl+Alt+Shift+Win+6 loops in its own code, asking the library each time (TEST 16), and its
-- Ctrl+Alt+Shift+Win+7 has the library read a pixel over and over (TEST 17). Every loop ends by
-- itself after 30 seconds, every fill after 600 MB, saying so — on a build without the guard.

local lib = host.require("com.tool.guard-probe-lib")
local SLOT = "guard-probe"

-- Each string different: Luau keeps one copy of equal strings.
local function fill()
  local t = {}
  for i = 1, 600 do
    t[#t + 1] = string.rep("x", 1048000) .. i
  end
  host.speech.output("No stop: this build has no memory limit")
end

local function spin()
  local started, n = os.clock(), 0
  while os.clock() - started < 30 do for _ = 1, 100000 do n += 1 end end
  host.speech.output("No stop: this build has no time limit")
end

-- The module's own loop; the library's helper it calls runs a short loop of its own each time.
local function poll()
  local started = os.clock()
  while os.clock() - started < 30 do if lib.ready() then break end end
  host.speech.output("No stop: this build has no time limit")
end

return {
  activate = function()
    host.settings.define("chime", false, { label = "Chime" })
    host.settings.onChange("chime", function(new)
      host.speech.output("Guard probe B heard the chime: " .. (new and "on" or "off"), { interrupt = false })
    end)

    local claim = host.arbiter.register(SLOT, 1, function()
      local chime = host.settings.get("chime") and "on" or "off"
      host.speech.output("Guard probe B has the slot, chime " .. chime, { interrupt = false })
    end, function()
      -- Only when Guard probe takes the slot: its own stop, a reload or unticking it must not run
      -- it out of memory or into a loop.
      local winner = host.arbiter.winnerSpecificity(SLOT)
      if winner == 2 then
        fill()
      elseif winner == 3 then
        spin()
      end
      host.speech.output("Guard probe B gave up the slot", { interrupt = false })
    end)
    host.arbiter.setMatching(SLOT, claim, true)

    host.hotkey.register("Ctrl+Alt+Win+9", function()
      host.speech.output("Guard probe B is on")
    end)

    host.hotkey.register("Ctrl+Alt+Shift+Win+2", function()
      fill()
    end)

    host.hotkey.register("Ctrl+Alt+Win+6", function()
      lib.spin()
    end)

    host.hotkey.register("Ctrl+Alt+Shift+Win+6", function()
      poll()
    end)

    host.hotkey.register("Ctrl+Alt+Shift+Win+7", function()
      lib.pixelForever()
    end)

    host.log.info("[guard-probe-b] loaded with " .. lib.name() .. ": Ctrl+Alt+Win+6 and 9, Ctrl+Alt+Shift+Win+2, 6 and 7")
  end,
}
