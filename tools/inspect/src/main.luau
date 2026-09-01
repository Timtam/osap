-- OCR window inspector (dev tool): Ctrl+Alt+I OCRs the focused window's full
-- client area and logs every recognized word with its CLIENT-relative
-- coordinates, for calibrating overlay regions and revealing where hardcoded
-- (e.g. ReaHotkey) coordinates land. With AUTOMATION_PLATFORM_OCR_DEBUG=1 the
-- captured image is also saved next to the executable.

host.hotkey.register("Ctrl+Alt+I", function()
  local win = host.window.active()
  if not win then
    host.speech.output("No active window.", { interrupt = true })
    return
  end
  local b = win.bounds
  local res = host.ocr.recognize({ region = { b.x, b.y, b.x + b.w, b.y + b.h } })
  host.log.info(string.format(
    "INSPECT '%s' [%s] clientOrigin=(%d,%d) frame=%dx%d words=%d",
    win.title, win.class, win.client.x, win.client.y, b.w, b.h, #res.words
  ))
  for _, wd in ipairs(res.words) do
    host.log.info(string.format(
      "  '%s' @ client (%d,%d) %dx%d",
      wd.text, wd.x - win.client.x, wd.y - win.client.y, wd.w, wd.h
    ))
  end
  host.speech.output(
    string.format("Inspected %s, %d words logged.", win.title, #res.words),
    { interrupt = true }
  )
end)

-- Ctrl+Alt+C: dump the focused window's child controls (class + screen geometry +
-- client origin), for detecting plugins embedded in a host (DAW) window and
-- finding their coordinate origin.
host.hotkey.register("Ctrl+Alt+C", function()
  local win = host.window.active()
  if not win then
    host.speech.output("No active window.", { interrupt = true })
    return
  end
  local controls = host.window.controls()
  host.log.info(string.format(
    "CONTROLS '%s' [%s] clientOrigin=(%d,%d) controls=%d",
    win.title, win.class, win.client.x, win.client.y, #controls
  ))
  for _, c in ipairs(controls) do
    host.log.info(string.format(
      "  [%s] window (%d,%d) %dx%d  client (%d,%d)",
      c.class, c.bounds.x, c.bounds.y, c.bounds.w, c.bounds.h, c.client.x, c.client.y
    ))
  end
  host.speech.output(
    string.format("%s, %d child controls logged.", win.title, #controls),
    { interrupt = true }
  )
end)

-- Ctrl+Alt+U: dump the UIA tree (raw view — named elements + Qt window panes) of
-- every Qt-class control in the focused window, with depth / ControlType / ClassName
-- / Name. For finding the identity of a plugin rendered inside another's Qt window
-- (e.g. a Kontakt nested in Komplete Kontrol's own QWindowIcon).
host.hotkey.register("Ctrl+Alt+U", function()
  local win = host.window.active()
  if not win then
    host.speech.output("No active window.", { interrupt = true })
    return
  end
  local function dumpOne(label, id)
    host.log.info(string.format("UIA DUMP %s id=%d", label, id))
    for _, e in ipairs(host.element.dump(id)) do
      host.log.info(string.format("  type=%-5d class='%s' name='%s'", e.ctype, e.class, e.name))
    end
  end
  -- the active (host) window first, then each Qt plugin control
  dumpOne(string.format("window '%s'", win.title), win.id)
  local dumped = 1
  for _, c in ipairs(host.window.controls()) do
    if string.match(c.class, "Qt%d") then
      dumpOne(string.format("control [%s]", c.class), c.id)
      dumped = dumped + 1
    end
  end
  host.speech.output(string.format("UIA dumped for %d roots.", dumped), { interrupt = true })
end)
