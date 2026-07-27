-- Announces the title of every window you switch to. Demonstrates the window
-- trigger layer (auto-activation) — the foundation for plugin overlays.

host.log.info("Window trigger demo loaded. Switch windows to hear announcements.")

-- An empty matcher matches every foreground window. Narrow it with e.g.
-- { app = { exe = "reaper.exe" } } or { windows = { class = { regex = "^VSTGUI" } } }.
host.window.onTrigger({}, { on = "activate" }, function(win)
  host.log.info("Foreground -> " .. win.title)
  host.speech.output(win.title, { interrupt = true })
end)

host.speech.output("Window trigger ready. Switch windows.", { interrupt = true })
