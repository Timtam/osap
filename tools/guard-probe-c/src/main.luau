-- Guard probe C — part of tools/guard-probe (see the test list there). It runs Guard probe
-- library's code and never loops itself: when the library's code loops in Guard probe B's VM
-- (TEST 13), it is stopped with the library all the same, because the same code runs in it.

local lib = host.require("com.tool.guard-probe-lib")

return {
  activate = function()
    host.hotkey.register("Ctrl+Alt+Shift+Win+0", function()
      host.speech.output("Guard probe C is on")
    end)

    host.log.info("[guard-probe-c] loaded with " .. lib.name() .. ": Ctrl+Alt+Shift+Win+0")
  end,
}
