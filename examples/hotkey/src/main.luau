-- Registers a global hotkey that speaks when pressed. The first real "trigger":
-- the module stays resident and reacts to a system-wide key combination.

host.log.info("Hotkey demo loaded.")

local count = 0
host.hotkey.register("Ctrl+Alt+H", function()
  count = count + 1
  host.speech.output("Hotkey pressed " .. count .. (count == 1 and " time." or " times."), { interrupt = true })
  host.log.info("Hotkey fired (" .. count .. ").")
end)

host.speech.output("Hotkey demo ready. Press control, alt, H.", { interrupt = true })
