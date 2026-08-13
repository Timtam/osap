-- One key that says everything there is to know about the window in front.
--
-- This exists for a person who cannot see the screen, on a machine nobody else can touch.
-- Asking such a tester "what does the plugin look like, and does the overlay find it?" asks
-- them to describe something they have no way to observe. So instead: they put a plugin in
-- front, press one key, and send us a folder. Everything we would have asked about is in it.
--
-- It gathers, for the focused window and every surface inside it:
--   * its identity as the platform sees it — title, class, application, and on macOS the
--     bundle identifier, which is what a matcher will have to be written against;
--   * its geometry, frame and content, which is what every authored coordinate is relative
--     to and the thing most likely to be quietly wrong on a Retina display;
--   * its accessibility tree, which is the only way anyone will learn what the macOS side
--     actually publishes — and the single thing standing between us and a working matcher;
--   * a PNG of it, which proves the screen can be read at all (without Screen Recording,
--     macOS does not fail a capture, it hands back a picture of the wallpaper);
--   * what OCR reads in it, with coordinates.
--
-- Nothing here interprets. It records.

local KEY = host.os.pick {
  -- Control+Option is VoiceOver's own modifier and everything on it belongs to VoiceOver,
  -- so the Mac gets Command+Shift instead.
  macos = "Cmd+Shift+F9",
  windows = "Ctrl+Shift+F9",
  linux = "Ctrl+Shift+F9",
}

-- A run counter, so pressing the key twice does not overwrite the first answer with the
-- second — and so the log and the image can be matched up by eye.
local run = 0

local function say(text)
  host.speech.output(text, { interrupt = true })
end

local function line(fmt, ...)
  host.log.info(string.format(fmt, ...))
end

-- The accessibility tree of one element, bounded by what a log is worth reading.
local function dumpTree(label, id)
  local ok, elements = pcall(function()
    return host.uia.rawDump(id)
  end)
  if not ok or not elements then
    line("  tree %s: unavailable (%s)", label, tostring(elements))
    return 0
  end
  line("  tree %s — %d element(s)", label, #elements)
  for _, e in ipairs(elements) do
    -- The class is the join key a matcher is written against, so it is printed exactly as
    -- the backend composes it, with nothing trimmed or prettified.
    line(
      "    %s depth=%d type=%d class='%s' name='%s'",
      string.rep(" ", math.min(e.depth or 0, 12)),
      e.depth or 0,
      e.ctype or -1,
      e.class or "",
      e.name or ""
    )
  end
  return #elements
end

host.hotkey.register(KEY, function()
  run = run + 1
  local win = host.window.active()
  if not win then
    say("Nothing is focused, so there is nothing to probe.")
    line("PROBE %d: no active window", run)
    return
  end

  line("===== PROBE %d =====", run)
  line("platform: %s", host.os.current)
  line(
    "window: title='%s' class='%s'",
    win.title or "",
    win.class or ""
  )
  line(
    "app: name='%s' exe='%s' bundleId='%s' pid=%d",
    win.app.name or "",
    win.app.exe or "",
    win.app.bundleId or "",
    win.app.pid or 0
  )
  -- Frame against content. On Windows these differ by the border and title bar; on macOS
  -- they are currently reported as the same rectangle, and whether that is right is one of
  -- the things this probe exists to settle.
  line(
    "frame: %d,%d %dx%d    content: %d,%d %dx%d",
    win.bounds.x, win.bounds.y, win.bounds.w, win.bounds.h,
    win.client.x, win.client.y, win.client.w, win.client.h
  )

  local controls = host.window.controls()
  line("surfaces inside it: %d", #controls)
  for i, c in ipairs(controls) do
    line(
      "  [%d] class='%s' frame %d,%d %dx%d  content %d,%d %dx%d",
      i, c.class or "",
      c.bounds.x, c.bounds.y, c.bounds.w, c.bounds.h,
      c.client.x, c.client.y, c.client.w, c.client.h
    )
  end

  -- The focus chain says whether the keyboard is inside a plugin surface or on the host's
  -- own chrome, which is the gate every embedded overlay passes through.
  local chain = host.window.focusChain()
  line("focus chain: %d deep (nearest first)", #chain)
  for i, c in ipairs(chain) do
    line("  [%d] class='%s' %d,%d %dx%d", i, c.class or "", c.bounds.x, c.bounds.y, c.bounds.w, c.bounds.h)
  end

  local total = dumpTree(string.format("window '%s'", win.title or ""), win.id)
  for i, c in ipairs(controls) do
    -- Every surface, not only the ones matching a toolkit pattern: on a platform where
    -- nobody yet knows what the classes look like, filtering by a guess would hide the
    -- answer.
    total = total + dumpTree(string.format("surface %d [%s]", i, c.class or ""), c.id)
  end

  -- A picture of the window, next to the application. The one artefact a sighted helper can
  -- check in seconds, and the only way to tell a real capture from the wallpaper that macOS
  -- hands back when Screen Recording was never granted.
  local shot = string.format("probe-%d.png", run)
  local saved = host.screen.save(shot, {
    region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
  })
  line("capture: %s (%dx%d points requested)", saved and shot or "FAILED", win.bounds.w, win.bounds.h)

  local ok, res = pcall(function()
    return host.ocr.recognize({
      region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
    })
  end)
  if ok and res then
    line("ocr: %d word(s)", #res.words)
    for _, wd in ipairs(res.words) do
      line(
        "  '%s' @ content (%d,%d) %dx%d",
        wd.text, wd.x - win.client.x, wd.y - win.client.y, wd.w, wd.h
      )
    end
  else
    line("ocr: failed (%s)", tostring(res))
  end

  line("===== end of probe %d =====", run)
  say(string.format(
    "Probe %d written. %d surfaces, %d accessibility elements, %d words.",
    run, #controls, total, (ok and res and #res.words) or 0
  ))
end)

host.log.info(string.format("[probe] ready — press %s over the window you want recorded", KEY))
