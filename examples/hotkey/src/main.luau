-- Registers a global hotkey that speaks when pressed. The first real "trigger":
-- the module stays resident and reacts to a system-wide key combination.

host.log.info("Hotkey demo loaded.")

-- Control+Alt+H on Windows. A hotkey is held system-wide, so on a Mac the demo picks a function
-- key: written once, "Ctrl+Alt+H" would be Option+Command+H there, which is Hide Others in the
-- application menu of most applications, taken from all of them while the demo runs.
-- Shift+Command+F7 is off that menu, and off Control+Option, VoiceOver's layer ("Meta+Alt" in a
-- spec). Like every F-key on a Mac it needs "Use F1, F2, etc. as standard function keys" turned
-- on, or fn held down.
local KEY = host.os.pick {
  windows = "Ctrl+Alt+H",
  macos = "Cmd+Shift+F7",
  linux = "Ctrl+Alt+H",
}
local count = 0
host.hotkey.register(KEY, function()
  count = count + 1
  host.speech.output("Hotkey pressed " .. count .. (count == 1 and " time." or " times."), { interrupt = true })
  host.log.info("Hotkey fired (" .. count .. ").")
end)

-- Said in this platform's words: "Control+Alt+H" on Windows, "Shift+Command+F7" on a Mac.
host.speech.output("Hotkey demo ready. Press " .. host.keys.describe(KEY) .. ".", { interrupt = true })
