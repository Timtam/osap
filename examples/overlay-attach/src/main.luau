-- A context-bound overlay (ReaHotkey-style): it activates automatically while a
-- Notepad window is focused and deactivates when you switch away. While active,
-- Tab / Shift+Tab move between controls and Enter or Space activates the focused
-- one; the per-control hotkeys (Alt+1/3) fire from anywhere it is active.
--
-- Alt, not Ctrl+Alt: Ctrl+Alt is AltGr on Windows, where a layout types a character
-- with it (AltGr+3 is a German layout's ³), and a hotkey takes that character from the
-- user everywhere for as long as it is held (host.keys.check says `altgr`). A control's
-- hotkey is held only while its overlay is active, so Alt+1 and Alt+3 are taken only
-- while a Notepad window is in front. On a Mac they are Option+1 and Option+3, which
-- type a character in most layouts (`composes`), taken the same way.

local O = host.require("com.platform.overlay")
local ov = O.new("Notepad")

ov:addStaticText("Notepad helper")

ov:addCustomButton{
  label = "Say hello",
  hotkey = "Alt+1",
  onActivate = function() host.speech.output("Hello from the Notepad overlay!", { interrupt = true }) end,
}

ov:addOCRButton{
  label = "Read top of the window",
  region = { 0, 0, 700, 60 },
  hotkey = "Alt+3",
}

-- Bind to any window whose title contains "Notepad".
ov:attach({ title = { contains = "Notepad" } })

host.speech.output("Notepad overlay loaded. Focus a Notepad window to activate it.", { interrupt = true })
