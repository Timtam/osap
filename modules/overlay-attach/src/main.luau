-- A context-bound overlay (ReaHotkey-style): it activates automatically while a
-- Notepad window is focused and deactivates when you switch away. Its hotkeys
-- (Ctrl+Alt+1/3, Ctrl+Alt+Left/Right/Enter) are live only while active.

local O = host.require("com.platform.overlay")
local ov = O.new("Notepad")

ov:addStaticText("Notepad helper")

ov:addCustomButton{
  label = "Say hello",
  hotkey = "Ctrl+Alt+1",
  onActivate = function() host.speech.output("Hello from the Notepad overlay!", { interrupt = true }) end,
}

ov:addOCRButton{
  label = "Read top of the window",
  region = { 0, 0, 700, 60 },
  hotkey = "Ctrl+Alt+3",
}

-- Bind to any window whose title contains "Notepad".
ov:attach({ title = { contains = "Notepad" } })

host.speech.output("Notepad overlay loaded. Focus a Notepad window to activate it.", { interrupt = true })
