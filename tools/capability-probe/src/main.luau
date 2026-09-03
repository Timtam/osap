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
  end,
}
