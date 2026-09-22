-- Does the capability gate hold?
--
-- Run by CI, headless, as the only module. It prints one verdict line: `CAPABILITY PROBE: ok`
-- when every route was refused, or a line naming each route that got through. The build fails
-- on the second.
--
-- It exists because a hole here is silent by nature. `host.include` handed included files the
-- UNGATED table for as long as includes existed, and nothing noticed: every module that used
-- the route was entitled to what it reached, so nothing ever failed. It took an adversarial
-- review of an unrelated commit to find it. A unit test could not have: the defect was not
-- inside a function but between two of them, in which table one handed the other.

local holes = {}

-- `f` must be REFUSED. Reaching the namespace is the finding.
local function denied(route, f)
  if pcall(f) then
    table.insert(holes, route)
  end
end

return {
  activate = function()
    denied("host.speech from the entry point", function() return host.speech.output end)
    denied("host.screen from the entry point", function() return host.screen.pixel end)
    -- A namespace with free members: `host.keys.normalize`, `describe` and `check` need no
    -- capability, and the table that hands them out must not hand out the capture beside them.
    denied("host.keys.capture from the entry point", function() return host.keys.capture end)

    -- The module's own second file: same package, same manifest, and until 2026-09-03 a
    -- complete bypass, because `include` handed it the whole host table.
    for _, route in ipairs(host.include("src/included.luau")) do
      table.insert(holes, route)
    end

    if #holes == 0 then
      host.log.info("CAPABILITY PROBE: ok — every undeclared namespace was refused")
    else
      host.log.info("CAPABILITY PROBE: HOLE — reached " .. table.concat(holes, ", "))
    end

    -- A second, unrelated property, checked here because this is the only module CI runs
    -- headless and the check costs one line: does `host.timer` fire at all in that mode?
    --
    -- It did not, for two separate reasons, and neither failed loudly: `Manager::run` did
    -- not even enter the event loop for a module whose only reason to be alive was a timer,
    -- and the Windows loop then sat in a blocking `GetMessageW` that nothing ever woke. A
    -- module logged its `activate` line and went silent for ten seconds. Since headless is
    -- the ONLY way module Luau is ever run on a Mac here, that made both CI jobs blind to
    -- everything a real overlay does.
    --
    -- Reported from inside the callback on purpose: if timers die again, this line is simply
    -- absent, and the job fails on the missing line rather than passing quietly.
    host.timer.after(200, function()
      host.log.info("HEADLESS TIMER: fired — host.timer runs without a window")
    end)
  end,
}
