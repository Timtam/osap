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

-- Runs `steps` one event-loop tick apart, then `done`.
--
-- The probe used to do everything inside its hotkey callback, and that callback runs on the
-- thread that also carries macOS's event tap. On the Mac mini it took 1.2 s, and both times the
-- system switched the tap off: the chord's own key-ups were waiting behind it. A tick apart,
-- the run loop turns between steps and the tap is answered each time. Each step is timed on a
-- line of its own, so the next log says which one is heavy on which machine.
--
-- A step's `run` is handed `resume`; one that returns `answers.WAIT` calls it itself when its
-- own timers are done. The first link is 20 ms, not 0: a zero-delay timer set from a hotkey
-- callback is already due when the same pump iteration fires its timers, while one set from a
-- timer callback waits for the next tick — which is what every later link relies on.
--
-- `token` is the run's entry in `busy` below. Each step stamps it, so "stuck" means no step for
-- two minutes rather than a run slower than that in all; and a run a newer press has given up
-- on (`token.dropped`) stops at its next step instead of weaving into the new one.
local function runSteps(steps, done, token)
  if not (host.timer and host.timer.after) then
    -- Nothing to spread them over: all at once, as the probe always used to.
    for _, s in ipairs(steps) do
      local ok, err = pcall(s.run, function() end)
      if not ok then line("  %s: FAILED (%s)", s.name, tostring(err)) end
    end
    done()
    return
  end
  local i = 0
  local advance
  local function schedule()
    host.timer.after(0, advance)
  end
  advance = function()
    if token then
      if token.dropped then return end
      token.since = host.now()
    end
    i = i + 1
    local s = steps[i]
    if not s then
      done()
      return
    end
    local t0 = host.now()
    local resumed, waiting = false, false
    local function resume()
      if resumed then return end
      resumed = true
      if waiting then schedule() end
    end
    local ok, r = pcall(s.run, resume)
    line("probe step '%s': %d ms", s.name, host.now() - t0)
    if not ok then line("  %s: FAILED (%s)", s.name, tostring(r)) end
    if ok and r == answers.WAIT and not resumed then
      waiting = true -- `resume` schedules the next step
    else
      schedule()
    end
  end
  host.timer.after(20, advance)
end

-- The run in progress, so a second press cannot interleave a second run with it — two PROBE
-- sections, two pictures and two summaries woven together. Not permanent: a run that has made
-- no step for two minutes is taken to be stuck (a timer that never fired), and the next press
-- starts afresh, says so, and stops the old one at its next step.
local busy = nil
local STUCK_MS = 120000

host.hotkey.register(KEY, function()
  if busy and host.now() - busy.since < STUCK_MS then
    say(string.format("Still recording probe %d.", busy.run))
    return
  end
  if busy then
    line("PROBE %d made no progress for %d ms — giving it up and starting a new one",
      busy.run, host.now() - busy.since)
    busy.dropped = true
    busy = nil
  end
  run = run + 1
  local win = host.window.active()
  if not win then
    say("Nothing is focused, so there is nothing to probe.")
    line("PROBE %d: no active window", run)
    return
  end
  -- The window's identity, geometry, surfaces and focus chain are read NOW, at the press, and
  -- the tree dumps and name lookups that follow go by that window's id. The picture, the OCR
  -- reads, the pointer and the window list are read when their own step runs, inside the
  -- rectangle read here — so something raised over the plug-in in the meantime (a notification,
  -- another application, a dialog of the plug-in itself) is what they record. The picture
  -- step says so in the log when the window in front is no longer this one.
  local thisRun = run

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
  -- Frame against content. On Windows these differ by the border and title bar. On macOS
  -- there is no client rect to ask for and the backend derives one (`content_rect` in
  -- `backend/macos/ax.rs`): a titled window's content view, or failing that twice the centre
  -- offset of its own close button; a window without a title bar has content == frame, and
  -- that is right. When the derivation finds neither, the backend reports content == frame
  -- for a titled window too, and that is one title bar wrong — two Kontakt dumps in the
  -- tester's third session were exactly that. The note further down says so on the line an
  -- author would otherwise take the rectangles from.
  line(
    "frame: %d,%d %dx%d    content: %d,%d %dx%d",
    win.bounds.x, win.bounds.y, win.bounds.w, win.bounds.h,
    win.client.x, win.client.y, win.client.w, win.client.h
  )

  local controls = host.window.controls()
  -- The count is a bounded walk's result, and the Mac backend says on a line of its own when it
  -- was cut short (and only then, or when the walk was slow). Pointing at that line rather than
  -- repeating a bare number: this one is spoken aloud as the confirmation that the press
  -- landed, and a partial count sounds exactly like a complete one.
  line(
    "surfaces inside it: %d (on a Mac: cut short if a window_controls line above says STOPPED, and 0 can mean the application was not answering; nothing below depth 8, or past 256 children of one node, is looked at)",
    #controls
  )
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

  -- The subrole is the second field of the class the backend composes (`join_class`:
  -- `AXRole/AXSubrole/AXIdentifier`), and these three are the ones `content_rect` treats as
  -- titled — the same set, so the caveat fires exactly where the derivation was attempted.
  local subrole = string.match(win.class or "", "^[^/]*/([^/]*)/") or ""
  local titled = subrole == "AXStandardWindow" or subrole == "AXDialog"
    or subrole == "AXSystemDialog"
  local contentIsFrame = win.client.x == win.bounds.x and win.client.y == win.bounds.y
    and win.client.w == win.bounds.w and win.client.h == win.bounds.h
  local caveat = ""
  if host.os.is("macos") and titled and contentIsFrame then
    caveat = " — content == frame on a titled window: the title-bar inset could not be "
      .. "derived, so authoring from these rectangles would land one title bar too high"
  end
  line(
    "note: tree rectangles below are SCREEN coordinates; subtract the content origin "
      .. "(%d,%d) to get what a module would author%s",
    win.client.x, win.client.y, caveat
  )
  -- Said before the work, not after it.
  --
  -- Everything below is silent until the run finishes, and on a large plug-in that is a long
  -- time to sit in front of an application that appears to have stopped. Somebody who cannot
  -- see a spinner has no way to tell work from a hang, and force-quitting is the reasonable
  -- response to a hang — which would lose the press this whole session is for.
  say("Recording. This can take a few seconds on a large plugin.")

  local total, words = 0, 0
  local steps = {}
  steps[#steps + 1] = { name = "window tree", run = function()
    total = total + dumpTree(string.format("window '%s'", win.title or ""), win.id)
  end }

  -- The loop is bounded, and it was not.
  --
  -- Each `dumpTree` is bounded by the host — 2000 nodes and five seconds — but the NUMBER of
  -- them was not bounded by anything: `host.window.controls()` returns up to 256 surfaces on
  -- macOS, and `AXGroup` and `AXUnknown` both count as surfaces, which is what a custom-drawn
  -- toolkit publishes for nearly every node it does not map. Worst case that is 257 walks of
  -- five seconds each — twenty minutes of frozen application and a log large enough to rotate
  -- the session away — for a press whose only spoken confirmation comes at the end.
  --
  -- The surfaces are also descendants of the window, so in the good case their subtrees are
  -- already in the dump above; these exist for the case where that one was truncated, or where
  -- a surface turns out to be a separate hierarchy. That is worth some duplication, not
  -- unbounded duplication. One surface per step, so a large plug-in's many small trees are no
  -- longer one long stall.
  local DUMP_BUDGET_MS = 20000
  local MAX_SURFACES = 24
  local started
  local dumped, skipped = 0, 0
  for i, c in ipairs(controls) do
    steps[#steps + 1] = { name = string.format("surface %d tree", i), run = function()
      started = started or host.now()
      if dumped >= MAX_SURFACES or (host.now() - started) > DUMP_BUDGET_MS then
        skipped = skipped + 1
        return
      end
      total = total + dumpTree(string.format("surface %d [%s]", i, c.class or ""), c.id)
      dumped = dumped + 1
    end }
  end

  -- A picture of the window, next to the application. The one artefact a sighted helper can
  -- check in seconds, and the only way to tell a real capture from the wallpaper that macOS
  -- hands back when Screen Recording was never granted.
  steps[#steps + 1] = { name = "picture", run = function()
    local front = host.window.active()
    if not (front and front.id == win.id) then
      line(
        "  NOTE: the window in front is no longer the one being recorded (now '%s') — the "
          .. "picture and the text read below show what covers its rectangle, not it",
        tostring(front and front.title or "nothing")
      )
    end
    if skipped > 0 then
      line(
        "  trees: %d of %d surface(s) dumped, %d SKIPPED after %d ms — this recording is "
          .. "partial by design rather than by accident, and the window's own tree above is "
          .. "the complete one to read",
        dumped, #controls, skipped, host.now() - (started or host.now())
      )
    end
    local shot = string.format("probe-%d.png", thisRun)
    local saved = host.screen.save(shot, {
      region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
    })
    line("capture: %s (%dx%d points requested)", saved and shot or "FAILED", win.bounds.w, win.bounds.h)
  end }

  local read
  steps[#steps + 1] = { name = "text", run = function()
    local ok, res = pcall(function()
      return host.ocr.recognize({
        region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
      })
    end)
    if ok and res then
      read = res.words
      words = #res.words
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
  end }
  steps[#steps + 1] = { name = "text against elements", run = function(resume)
    if not read then return end
    -- Its own hops, one lookup a tick; the chain goes on when it calls back.
    answers.textAgainstElements(line, win, read, resume)
    return answers.WAIT
  end }

  for _, s in ipairs(answers.steps(line, win)) do
    steps[#steps + 1] = s
  end

  -- Held from here, not from the press: the part above is synchronous, so nothing can
  -- interleave with it, and a throw in it would otherwise leave the key refusing for two
  -- minutes.
  local token = { run = thisRun, since = host.now() }
  busy = token
  runSteps(steps, function()
    line("===== end of probe %d =====", thisRun)
    -- Only its own: a run that was given up on must not release the guard of the one that
    -- replaced it.
    if busy == token then busy = nil end
    say(string.format(
      "Probe %d written. %d surfaces, %d accessibility elements, %d words.",
      thisRun, #controls, total, words
    ))
  end, token)
end)

-- The game controllers have a file of their own (src/gamepad.luau), and the ready line waits
-- for them: it reports how many pads were found and how each source came up, which is only
-- true once the sources have acted on the listeners that file registers.
local gamepad = host.include("src/gamepad.luau")
gamepad.start(function(pads)
  host.log.info(string.format(
    "[probe] ready — press %s over the window you want recorded, %s for the game controllers; %s",
    KEY, gamepad.KEY, pads
  ))
end)
