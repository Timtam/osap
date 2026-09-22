-- Demonstrates host.os and host.window (enumeration, active window, OS-gated matcher).

host.log.info("OS: " .. host.os.current .. "  (is windows = " .. tostring(host.os.is("windows")) .. ")")

local all = host.window.list()
host.log.info("Visible top-level windows: " .. #all)

local w = host.window.active()
if w then
  host.log.info(string.format(
    "Active: '%s'  class=%s  exe=%s  pid=%d  %dx%d @ %d,%d",
    w.title, w.class, w.app.exe, w.app.pid, w.bounds.w, w.bounds.h, w.bounds.x, w.bounds.y))
else
  host.log.info("Active: <none>")
end

-- Matcher demo: a Windows-only matcher (class contains "Window").
local winMatch = host.window.findAll({ windows = { class = { contains = "Window" } } })
host.log.info("Matcher [windows: class contains 'Window'] matched: " .. #winMatch)

-- OS-gating demo: a macOS-only matcher must match nothing on Windows. On a Mac `class` is the
-- whole AXRole/AXSubrole/AXIdentifier triple, so the role alone is matched with `axRole`.
local macOnly = host.window.findAll({ macos = { axRole = "AXWindow" } })
host.log.info("Matcher [macos-only] matched on this OS: " .. #macOnly .. " (expected 0 on Windows)")

host.speech.output(string.format("Detected %d windows on %s.", #all, host.os.current), { interrupt = true })
