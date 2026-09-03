-- An included file is the module's own code and gets the module's own permissions — no more.
-- The manifest declares `log` only, so both of these must be refused here exactly as they are
-- in the entry point.
local holes = {}
local function denied(route, f)
  if pcall(f) then
    table.insert(holes, route)
  end
end

denied("host.speech from an included file", function() return host.speech.output end)
denied("host.screen from an included file", function() return host.screen.pixel end)

return holes
