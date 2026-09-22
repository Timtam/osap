-- Demonstrates host.keys: low-level key capture WITH suppression. Toggle with
-- Ctrl+Alt+O. While on, Tab and Escape are intercepted (spoken, and NOT passed
-- to the focused app); while off, they work normally.

local TOGGLE = "Ctrl+Alt+O"
local capturing = false

host.hotkey.register(TOGGLE, function()
  capturing = not capturing
  if capturing then
    -- Plain Tab and Shift+Tab are captured; Alt+Tab is NOT (different combo),
    -- so window switching keeps working.
    host.keys.capture("Tab", function() host.speech.output("Tab intercepted", { interrupt = true }) end)
    host.keys.capture("Shift+Tab", function() host.speech.output("Shift Tab intercepted", { interrupt = true }) end)
    host.keys.capture("Escape", function() host.speech.output("Escape intercepted", { interrupt = true }) end)
    host.speech.output("Key capture on. Plain Tab, Shift Tab and Escape are intercepted; Alt Tab still works.", { interrupt = true })
  else
    host.keys.releaseAll()
    host.speech.output("Key capture off.", { interrupt = true })
  end
end)

host.speech.output("Key capture demo. Press " .. host.keys.describe(TOGGLE)
  .. " to toggle Tab and Escape capture.", { interrupt = true })
