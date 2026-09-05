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

-- The questions this platform can be ASKED rather than told — everything the log could settle
-- on its own instead of a person working through a checklist. Read-only, all of it.
local answers = host.include("src/answers.luau")

-- A run counter, so pressing the key twice does not overwrite the first answer with the
-- second — and so the log and the image can be matched up by eye.
-- Where the numbering starts, which is NOT zero.
--
-- It was, and the pictures are named `probe-N.png` from it, so the first press of every
-- launch wrote `probe-1.png` over the one from the launch before. That is not a corner case
-- on the machine this is aimed at: setting up a new Mac means granting a permission, quitting,
-- reopening — macOS hands a new permission only to a process that started after it — and the
-- tester's own protocol has him press this key in more than one step. The picture is half of
-- what he sends back, and it is the half that cannot be reconstructed from anything else.
--
-- So the count resumes rather than restarts: the first free name wins. `host.resource.exists`
-- reads from the module's own folder, which is exactly where `host.screen.save` writes.
local run = (function()
  if not (host.resource and host.resource.exists) then
    return 0
  end
  -- Bounded: a folder somebody has filled should not make the module hang at load.
  for n = 1, 500 do
    if not host.resource.exists(string.format("probe-%d.png", n)) then
      return n - 1
    end
  end
  return 500
end)()

local function say(text)
  host.speech.output(text, { interrupt = true })
end

local function line(fmt, ...)
  host.log.info(string.format(fmt, ...))
end

-- Do the plugin's own control names survive into what macOS publishes?
--
-- The single question a Kontakt or Komplete Kontrol window is being probed to answer, and it
-- decides an architecture: on Windows a plugin's parts are matched by the internal names Qt
-- gives them, and whether those reach `AXIdentifier` decides whether the nested-overlay
-- design ports or has to be rebuilt out of image templates. sforzando could not answer it —
-- it is not a Qt application.
--
-- The answer is already in the dump above, spread across three hundred lines. Counting it
-- here is the difference between a question somebody has to reconstruct and one the log
-- states. The backend composes the class as `AXRole/AXSubrole/AXIdentifier` (see
-- `join_class`), so the identifier is the third field and an absent subrole leaves `//`.
--
-- macOS only, and not out of caution: on Windows this is already known — UIA publishes an
-- AutomationId and the shipped modules match on it — so the count would be noise beside an
-- answer nobody is waiting for.
local function identifierSummary(label, elements)
  if not (host.os and host.os.is and host.os.is("macos")) then
    return
  end
  local withId, seen, sample = 0, {}, {}
  for _, e in ipairs(elements) do
    local ident = string.match(e.class or "", "^[^/]*/[^/]*/(.+)$")
    if ident and ident ~= "" then
      withId = withId + 1
      if not seen[ident] then
        seen[ident] = true
        if #sample < 8 then
          sample[#sample + 1] = ident
        end
      end
    end
  end
  if withId == 0 then
    -- "of the N returned", not "of N". The dump above is bounded by nodes and by time, and it
    -- says on its own line whether it was cut short — so this sentence must not imply it saw
    -- the whole tree. Reading "NONE publish an AXIdentifier" off a fragment is the one wrong
    -- answer this press can give that would send the whole nested-overlay design to image
    -- matching for no reason.
    line(
      "  tree %s — NONE of the %d element(s) returned publish an AXIdentifier. If the dump "
        .. "line above says it was complete, a matcher here has only role and geometry to "
        .. "work with",
      label, #elements
    )
  else
    line(
      "  tree %s — %d of the %d element(s) returned publish an AXIdentifier; first few: %s",
      label, withId, #elements, table.concat(sample, ", ")
    )
  end
end

-- The accessibility tree of one element, bounded by what a log is worth reading.
local function dumpTree(label, id)
  local ok, elements = pcall(function()
    return host.element.rawDump(id)
  end)
  if not ok or not elements then
    line("  tree %s: unavailable (%s)", label, tostring(elements))
    return 0
  end
  if #elements == 0 then
    -- An empty table is not an empty tree. `host.element.rawDump` answers with one when the
    -- handle is not live or the application is in the busy quarantine, and a Lua table is
    -- truthy either way, so the guard above cannot catch it. Printing the identifier verdict
    -- here would state "NONE publish an AXIdentifier" about a walk that never happened — the
    -- one wrong answer this press can give that would send the whole nested-overlay design to
    -- image matching for nothing. The backend now says which of the two it was on its own
    -- line, immediately above this one.
    line(
      "  tree %s — NOTHING CAME BACK. That is not the same as an empty tree: see the "
        .. "dump(...) line above for whether it was walked at all. No conclusion is drawn "
        .. "from this.",
      label
    )
    return 0
  end
  line("  tree %s — %d element(s)", label, #elements)
  for _, e in ipairs(elements) do
    -- The class is the join key a matcher is written against, so it is printed exactly as
    -- the backend composes it, with nothing trimmed or prettified. The rectangle is the
    -- other half: it is what an author actually needs, and it measures things nothing else
    -- can — the window's own close button, for instance, says how tall the title bar is,
    -- and that is the difference between a Windows-authored coordinate landing on a control
    -- and landing a title bar above it.
    local b = e.bounds or { x = 0, y = 0, w = 0, h = 0 }
    line(
      "    %s depth=%d type=%d class='%s' name='%s' at %d,%d %dx%d",
      string.rep(" ", math.min(e.depth or 0, 12)),
      e.depth or 0,
      e.ctype or -1,
      e.class or "",
      e.name or "",
      b.x, b.y, b.w, b.h
    )
  end
  identifierSummary(label, elements)
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

  line(
    "note: tree rectangles below are SCREEN coordinates; subtract the content origin "
      .. "(%d,%d) to get what a module would author",
    win.client.x, win.client.y
  )
  -- Said before the work, not after it.
  --
  -- Everything below is silent until the run finishes, and on a large plug-in that is a long
  -- time to sit in front of an application that appears to have stopped. Somebody who cannot
  -- see a spinner has no way to tell work from a hang, and force-quitting is the reasonable
  -- response to a hang — which would lose the press this whole session is for.
  say("Recording. This can take a few seconds on a large plugin.")

  local total = dumpTree(string.format("window '%s'", win.title or ""), win.id)

  -- The loop is bounded, and it was not.
  --
  -- Each `dumpTree` is bounded by the host — 2000 nodes and five seconds — but the NUMBER of
  -- them was not bounded by anything: `host.window.controls()` returns up to 256 surfaces on
  -- macOS, and `AXGroup` and `AXUnknown` both count as surfaces, which is what a custom-drawn
  -- toolkit publishes for nearly every node it does not map. Worst case that is 257 walks of
  -- five seconds each — twenty minutes of frozen application and a log large enough to rotate
  -- the session away — for a press whose only spoken confirmation comes at the end.
  --
  -- It has never been exercised: the only Mac this has ever run on could not run a plug-in
  -- with any sub-surfaces at all (`surfaces inside it: 0`), and Kontakt is the first tree with
  -- them. Same shape as the fault fixed in the backend an hour earlier, one level up: that one
  -- bounded a walk, and nothing bounded the loop around it.
  --
  -- The surfaces are also descendants of the window, so in the good case their subtrees are
  -- already in the dump above; these exist for the case where that one was truncated, or where
  -- a surface turns out to be a separate hierarchy. That is worth some duplication, not
  -- unbounded duplication.
  local DUMP_BUDGET_MS = 20000
  local MAX_SURFACES = 24
  local started = host.now()
  local dumped, skipped = 0, 0
  for i, c in ipairs(controls) do
    if dumped >= MAX_SURFACES or (host.now() - started) > DUMP_BUDGET_MS then
      skipped = skipped + 1
    else
      total = total + dumpTree(string.format("surface %d [%s]", i, c.class or ""), c.id)
      dumped = dumped + 1
    end
  end
  if skipped > 0 then
    line(
      "  trees: %d of %d surface(s) dumped, %d SKIPPED after %d ms — this recording is "
        .. "partial by design rather than by accident, and the window's own tree above is "
        .. "the complete one to read",
      dumped, #controls, skipped, host.now() - started
    )
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

  answers.all(line, win)

  line("===== end of probe %d =====", run)
  line("(the timer line above arrives a second later, after this marker — its ABSENCE means "
    .. "the timer never fired, which is itself the answer)")
  say(string.format(
    "Probe %d written. %d surfaces, %d accessibility elements, %d words.",
    run, #controls, total, (ok and res and #res.words) or 0
  ))
end)

host.log.info(string.format("[probe] ready — press %s over the window you want recorded", KEY))
