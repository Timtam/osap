-- A first self-voicing overlay built on host.overlay. Stays resident; navigate
-- with Ctrl+Alt+Left/Right, activate with Ctrl+Alt+Enter, or use the per-control
-- hotkeys Ctrl+Alt+1/2/3. (Non-destructive: no hotspot clicks in this demo.)

local ov = host.overlay.new("Demo")

ov:addStaticText("Demo overlay")

ov:addCustomButton{
  label = "Say hello",
  hotkey = "Ctrl+Alt+1",
  onActivate = function() host.speech.output("Hello from the overlay!", { interrupt = true }) end,
}

ov:addCustomButton{
  label = "Cursor position",
  hotkey = "Ctrl+Alt+2",
  onActivate = function()
    local p = host.input.cursorPos()
    host.speech.output(string.format("Cursor at %d, %d", p.x, p.y), { interrupt = true })
  end,
}

ov:addOCRButton{
  label = "Read top-left of screen",
  region = { 0, 0, 600, 60 },
  hotkey = "Ctrl+Alt+3",
}

ov:show()
