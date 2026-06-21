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
