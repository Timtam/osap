-- HTML page probe (dev tool, Windows): the maintainer's NVDA test of `host.gui.webview`, stage 1
-- of the HTML plan of 2026-09-28. It answers the questions W1–W9 in TODO.md — whether NVDA finds
-- its way into a page shown beside a plug-in window, which way of pressing a key gives the page
-- the front, what a click into the plug-in and back costs, which keys reach the page — for a
-- person who cannot see the screen: everything it observes goes into the log, and one key says
-- what happened since it was last pressed.
--
-- It binds nothing to a plug-in and adds no overlay. Test it on a plug-in without one (ReaEQ in
-- REAPER). Its keys, all free on Windows with NVDA — the four-modifier chord the platform's own
-- reload (F5) and plug-in focus key (F6) use, one function key further along each:
--   Ctrl+Shift+Win+Alt+F7  the page beside the plug-in window in front, the way every hotkey
--                          arrives: matched in the keyboard hook (W2 a). Pressed while the page
--                          is in front, it goes back to the plug-in (W7) — there it arrives
--                          through RegisterHotKey, the hook being out of the chain while a
--                          window of this application is in front.
--   Ctrl+Shift+Win+Alt+F8  the same, registered with { foreground = true }: Windows delivers it
--                          as WM_HOTKEY (W2 b).
--   Ctrl+Shift+Win+Alt+F9  says what happened since it was last pressed, and counts anew.
-- And one setting, off by default: the page comes up by itself when a plug-in window comes to
-- the front from another window (W2 c).
--
-- The page is prepared — built, hidden — the first time a plug-in window comes to the front, so
-- WebView2's cold start (W8) is over before the first key; `page:prepare()` again after a close.
--
-- Every time is measured from the callback that started it — a key's callback runs at the host's
-- first tick after the press, up to about 15 ms later — and ends at the tick that hands on what
-- the page sent, so each is good to about 15 to 30 ms. Nothing here waits for a time: every
-- step follows an event (a window coming forward, a focus change, a picture's answer, a message).

local daw = host.require("com.platform.daw-hosts")

-- Which revision of the probe this is, said at the start of the first report of a run and in the
-- log at load, so a test run on an old build is heard at once. 2: after the first NVDA run
-- (2026-09-28) and its review. 3: the same probe on the host whose handlers wait and whose
-- keyboard hook leaves the chain while a window of this application, the page's included, is in
-- front, and whose page gives the front back to the window it came from as it goes; the way back
-- keeps the page when Windows declines (2026-10-11).
local REVISION = 3

local KEY_SHOW = "Ctrl+Shift+Win+Alt+F7"
local KEY_SHOW_FG = "Ctrl+Shift+Win+Alt+F8"
local KEY_REPORT = "Ctrl+Shift+Win+Alt+F9"

local AUTO = "autoShow"
host.settings.define(AUTO, false, {
  label = "Show the page by itself when a plug-in window comes to the front from another window (test W2 c)",
})

local function log(fmt, ...)
  host.log.info("[html-probe] " .. string.format(fmt, ...))
end

local function say(text)
  host.speech.output(text, { interrupt = true })
end

-- Said without cutting off what the screen reader is saying — a page just come to the front.
local function mention(text)
  host.speech.output(text, { interrupt = false })
end

-- ── What the report says ──────────────────────────────────────────────────────

-- What happened since the report key was last pressed: the results (short sentences, newest
-- last), the buttons pressed and the keys the page saw. The report says them and starts anew, so
-- each test is counted from the report before it.
local MAX_RESULTS = 8
local MAX_KEYS = 10
local since
local function fresh()
  since = { results = {}, buttons = { ["click-plugin"] = 0, back = 0, announce = 0 }, keys = {}, keyCount = 0 }
end
-- Whether a report has been said in this run: the first one starts with the revision.
local reported = false
fresh()

local function result(fmt, ...)
  local line = string.format(fmt, ...)
  log("RESULT %s", line)
  local r = since.results
  r[#r + 1] = line
  if #r > MAX_RESULTS then
    table.remove(r, 1)
  end
end

-- ── The page ───────────────────────────────────────────────────────────────────

local page -- set below; its callbacks close over it

-- The plug-in window the page was last shown beside, as a window table.
local plugin = nil
-- A show that answered "not ready yet": shown again once the page is ready.
local pending = nil
-- The show whose "page has the focus" is still to come.
local measuring = nil
-- The last press of one of the probe's keys or buttons, for how long after it each thing happened.
local pressedAt = host.now()
-- The page's buttons, counted over the whole session as their messages arrive, beside the page's
-- own count of its clicks: a press that arrived twice leaves the two apart (W5).
local actions = { ["click-plugin"] = 0, back = 0, announce = 0 }
local gaps = { ["click-plugin"] = 0, back = 0, announce = 0 }
-- W3's round trip while it is under way: "forward" (the plug-in window asked to the front),
-- "click" (clicked, its picture asked for), "watch" (the page back in front, watched for losing
-- it again). nil when none is. A second "Click into the plug-in" while one is under way is not a
-- second round trip (see onMessage).
local w3 = nil
-- Whether the page is built and loaded: until then a plug-in window coming to the front prepares
-- it.
local ready = false
-- Whether "the page covers part of the plug-in window" has been said, for the shows that answer
-- so in a row.
local overSaid = false
-- W3's last click, which the next one keeps away from (see backgroundPoint).
local lastClick = nil
-- W7: after "back to the plug-in", the window whose keyboard is to be looked at once it is in
-- front.
local landing = nil

-- Keys that are only modifiers: logged, not counted — pressed in the page, they are the report
-- key's own, whose F9 RegisterHotKey takes there (the keyboard hook is out of the chain while the
-- page is in front). Nor the key the hook presses to mask a chord it swallowed, which has no
-- virtual key of its own (`Unidentified` to the page): it reaches the page only when the hook
-- swallowed a chord in the moment it was going out.
local NOT_COUNTED = { Control = true, Shift = true, Alt = true, Meta = true, OS = true, AltGraph = true, Unidentified = true }

-- Whether `w` is a plug-in window of a DAW daw-hosts knows: an entry's matcher takes it, and the
-- entry's `focusTarget` too where it has one (REAPER's matcher takes every dialog of its process).
local function isPluginWindow(w)
  if not w then
    return false
  end
  for _, entry in ipairs(daw.all) do
    if host.window.test(entry, w) and (entry.focusTarget == nil or host.window.test(entry.focusTarget, w)) then
      return true
    end
  end
  return false
end

-- The remembered plug-in window as it is now (it may have moved), or nil when it is gone.
local function refreshed(w)
  if not w then
    return nil
  end
  for _, x in ipairs(host.window.list({ pids = { w.app.pid } })) do
    if x.id == w.id then
      return x
    end
  end
  return nil
end

-- The window in front, in words for the log.
local function frontWords()
  local f = host.window.foreground()
  if not f then
    return "nothing"
  end
  if page and page.window and f.id == page.window then
    return "the page"
  end
  if plugin and f.id == plugin.id then
    return "the plug-in window"
  end
  local a = host.window.active()
  if a and a.id == f.id then
    return string.format("'%s' (%s)", a.title or "", a.app and a.app.exe or "?")
  end
  return string.format("window %d of process %d", f.id, f.pid)
end

-- A show that answered `true`: whether it put the page over part of the plug-in window — no room
-- beside it, a maximised window say. Said once for a run of such shows, and in words for the
-- result.
local function overWords(note)
  if note ~= "over the rectangle to avoid" then
    overSaid = false
    return ""
  end
  if not overSaid then
    overSaid = true
    -- Not over NVDA saying the page, which comes to the front with this show.
    mention("The page covers part of the plug-in window: there is no room beside it.")
  end
  return " (over part of the plug-in window: no room beside it)"
end

-- Shows the page beside `p`. `via` names the way it was asked for; `t0` is when, and `after`
-- what happened at `t0`, for the result.
local function showBeside(p, via, t0, after)
  local ok, why = page:show({ focus = true, avoid = p.bounds })
  local ms = host.now() - t0
  local front = frontWords()
  if ok then
    measuring = { via = via, t0 = t0, after = after }
    local over = overWords(why)
    if front == "the page" then
      result("%s: accepted, %d ms after %s; in front: the page%s", via, ms, after, over)
    else
      result("%s: ACCEPTED BUT NOT IN FRONT, %d ms after %s; in front: %s%s", via, ms, after, front, over)
    end
  elseif why == "not ready yet" then
    pending = { via = via, t0 = t0, after = after, plugin = p }
    log("%s: the page is being built; it is shown when it is ready", via)
    say("Building the page.")
  else
    result("%s: not shown, %s, %d ms after %s; in front: %s", via, tostring(why), ms, after, front)
    say("Page not shown: " .. tostring(why))
  end
end

-- W7: back to the plug-in — its window to the front first, while the page still is, then the
-- page out of sight. Where the keyboard lands is looked at once the window is in front. Declined,
-- the page stays where it is, in front, rather than leave the keyboard to whichever window
-- Windows picks: F7 and Alt+F4 still work there.
local function backToPlugin(via)
  w3 = nil
  local p = refreshed(plugin)
  if not p then
    say("No plug-in window remembered.")
    log("%s: back to the plug-in, but no plug-in window is remembered", via)
    return
  end
  local t0 = host.now()
  local accepted = host.window.focus(p.id)
  if not accepted then
    landing = nil
    result("%s: back to the plug-in DECLINED; the page stays; in front: %s", via, frontWords())
    say("Could not switch to the plug-in.")
    return
  end
  page:hide()
  landing = { via = via, id = p.id, t0 = t0 }
  result("%s: back to the plug-in accepted; in front right after hiding the page: %s", via, frontWords())
end

-- W7: once the plug-in window is in front again, which control of it has the keyboard — REAPER
-- may leave it on its FX list (SysListView32) rather than in the plug-in. Asked on the window and
-- focus events, never after a time: the first answer is where it landed; the window's foreground
-- event can come before its control has the keyboard, so the first focus event after that answer
-- says once more where it is, if that differs, and ends the watch (as does leaving the window,
-- or any key of the probe's).
local function checkLanding()
  if not landing then
    return
  end
  local f = host.window.foreground()
  if not f or f.id ~= landing.id then
    if landing.last then
      landing = nil
    end
    return
  end
  local chain = host.window.focusChain()
  local classes = {}
  for _, c in ipairs(chain) do
    classes[#classes + 1] = tostring(c.class)
  end
  local inner = chain[1]
  local where
  if not inner then
    where = "nothing that could be read"
  elseif inner.id == landing.id then
    where = "the plug-in window itself (" .. tostring(inner.class) .. ")"
  else
    where = tostring(inner.class)
  end
  if where == landing.last then
    return
  end
  if landing.last then
    result("W7: then the keyboard moved on to %s, %d ms after", where, host.now() - landing.t0)
  else
    result("W7: %s: the keyboard landed on %s, %d ms after", landing.via, where, host.now() - landing.t0)
  end
  landing.last = where
  log("W7: the focus chain, innermost first: %s", table.concat(classes, " < "))
end

-- W3's click: a point of the plug-in window's own background — inside its client area, and at
-- least CLEAR pixels from its edges and from every control in it — or nil when there is none.
-- Never the title bar: two clicks there are a double click, which maximised the maintainer's FX
-- window on 2026-09-28 and left the page no room. Assumed, and checked by the result line (the
-- window's place and size before and after): a click on a dialog's own background reaches the
-- window's procedure as a click on nothing — not REAPER's FX list, where a double click opens the
-- plug-in, and not the plug-in, whose knobs it would turn; the margin keeps it off the narrow gaps
-- a window may use as a splitter. And never where the last W3 click was: at least APART pixels
-- away along one axis, far beyond the few pixels Windows pairs two clicks within, so two round
-- trips however close together are never a double click, whatever the window does with one. Of
-- the points on a grid, the one farthest from everything wins; the grid is coarse enough for this
-- to take a few milliseconds.
local CLEAR = 8
local APART = 16
local function backgroundPoint(w)
  local c = w.client
  if not c or c.w < 2 * CLEAR or c.h < 2 * CLEAR then
    return nil
  end
  local controls = host.window.controls(w)
  local step = math.max(16, math.min(c.w, c.h) // 32)
  local best, bestRoom = nil, CLEAR - 1
  for y = c.y + CLEAR, c.y + c.h - 1 - CLEAR, step do
    for x = c.x + CLEAR, c.x + c.w - 1 - CLEAR, step do
      local room = math.min(x - c.x, c.x + c.w - 1 - x, y - c.y, c.y + c.h - 1 - y)
      if lastClick and math.abs(x - lastClick.x) < APART and math.abs(y - lastClick.y) < APART then
        room = -1
      end
      for _, k in ipairs(controls) do
        local b = k.bounds
        local d = math.max(b.x - x, x - (b.x + b.w - 1), b.y - y, y - (b.y + b.h - 1), 0)
        if d < room then
          room = d
          if room <= bestRoom then
            break
          end
        end
      end
      if room > bestRoom then
        best, bestRoom = { x = x, y = y }, room
      end
    end
  end
  return best, bestRoom
end

-- W3: into the plug-in and back, as an overlay's action on a page would be — and every step on
-- the event that says the one before is done. The plug-in window is asked to the front — in front
-- of the page, which may be over part of it (no room beside a maximised window); when it is there
-- (its foreground event, below), one click goes onto its own background (above), or none where it
-- has no such spot — then the round trip is the change of windows alone — and a picture of the
-- plug-in is asked for, off the event loop, as an overlay reads after its click; in that
-- picture's answer, once the click has gone through Windows, the page is asked back. Then whether
-- it keeps the front: a blur of the page's with another window in front says when it does not.
local function clickIntoPlugin()
  local p = refreshed(plugin)
  if not p then
    say("No plug-in window remembered.")
    return
  end
  local t0 = host.now()
  pressedAt = t0
  landing = nil
  w3 = { stage = "forward", t0 = t0, plugin = p }
  local accepted = host.window.focus(p.id)
  log("W3: the plug-in window asked to the front: %s; in front right after asking: %s", accepted and "accepted" or "DECLINED", frontWords())
  if not accepted then
    result("W3: the plug-in window would not come to the front")
    say("Could not switch to the plug-in.")
    w3 = nil
  end
end

-- W3, the plug-in window `w` in front: the click, and the picture whose answer brings the page
-- back.
local function w3Arrived(w)
  w3.stage = "click"
  local spot, room = backgroundPoint(w)
  if not spot then
    log("W3: the plug-in window came to the front %d ms after the press; it has no spot of its own background clear of its controls%s, so nothing is clicked — the round trip is the change of windows alone", host.now() - w3.t0, lastClick and " and away from the last click" or "")
  elseif host.window.ownsPoint(w.id, spot.x, spot.y) == false then
    -- Nothing clicked through another window; the round trip goes on without the click, so the
    -- page still comes back.
    result("W3: something else is drawn over the plug-in window at %d,%d; no click", spot.x, spot.y)
    say("Something covers the plug-in window; no click.")
    spot = nil
  else
    host.input.click(spot.x, spot.y)
    lastClick = spot
    log(
      "W3: the plug-in window came to the front %d ms after the press; clicked at %d,%d (its own background, %d px from its nearest control or edge)",
      host.now() - w3.t0, spot.x, spot.y, room
    )
  end
  w3.clicked = spot ~= nil
  local c = w.client
  local region = { c.x, c.y, c.x + math.clamp(c.w, 1, 400), c.y + math.clamp(c.h, 1, 300) }
  host.screen.snapshotAsync({ region = region, key = "w3" }, function(snap, why)
    if snap then
      snap:release()
    end
    if not w3 or w3.stage ~= "click" then
      return
    end
    if not snap then
      log("W3: the picture after the click gave nothing (%s); the page is asked back all the same", tostring(why))
    end
    local t = host.now()
    -- The question W3 asks, which nobody can hear: did the click change the plug-in window?
    local before, now = w3.plugin.bounds, (refreshed(w) or w).bounds
    local kept
    if before.x == now.x and before.y == now.y and before.w == now.w and before.h == now.h then
      kept = "the plug-in window kept its place and size"
    else
      kept = string.format(
        "the plug-in window CHANGED from %d,%d %dx%d to %d,%d %dx%d",
        before.x, before.y, before.w, before.h, now.x, now.y, now.w, now.h
      )
    end
    local ok, note = page:show({ focus = true, avoid = now })
    local front = frontWords()
    if ok then
      w3.stage = "watch"
      w3.backAt = host.now()
      measuring = { via = "W3 back", t0 = t, after = "the page was asked back" }
      result(
        "W3: the page came back%s, accepted, %d ms after the press; in front: %s%s; %s. It has kept the front so far.",
        w3.clicked and "" or " (no click)", host.now() - w3.t0, front, overWords(note), kept
      )
    else
      result("W3: the page did not come back: %s, %d ms after the press; in front: %s; %s", tostring(note), host.now() - w3.t0, front, kept)
      w3 = nil
    end
  end)
end

-- W9: the live region and the host's speech at the same moment.
local function announce()
  local n = actions.announce
  page:post({ kind = "live", text = string.format("Live region update %d.", n) })
  host.speech.output(string.format("Host speech %d.", n), { interrupt = true })
  log("W9: live region update %d posted and host speech %d said in the same callback", n, n)
end

-- A key as the page named it, in words: the space bar is " " there.
local function keyName(key)
  if key == " " then
    return "Space"
  end
  return tostring(key)
end

local function onMessage(m)
  if type(m) ~= "table" then
    log("page sent %s", tostring(m))
    return
  end
  local kind = m.kind
  if kind == "focus" then
    if measuring then
      result("%s: the page has the focus %d ms after %s", measuring.via, host.now() - measuring.t0, measuring.after)
      measuring = nil
    end
    log("page: window focus, on %s", tostring(m.target))
  elseif kind == "blur" then
    local front = frontWords()
    log("page: window lost the focus, %d ms after the last press; in front: %s", host.now() - pressedAt, front)
    -- Lost the front only with another window in front: the page's own window takes the
    -- keyboard for a moment as the document gets it back, which blurs the document with the
    -- page still in front (2026-09-28, "lost the front again … in front: the page").
    if w3 and w3.stage == "watch" and front ~= "the page" then
      result("W3: the page lost the front again %d ms after it came back; in front: %s", host.now() - w3.backAt, front)
      w3 = nil
    end
  elseif kind == "focusin" then
    log("page: focus moved to %s", tostring(m.target))
  elseif kind == "key" then
    local mods = (m.ctrl and "Ctrl+" or "") .. (m.alt and "Alt+" or "") .. (m.shift and "Shift+" or "") .. (m.meta and "Win+" or "")
    log(
      "W4 key: %s %s%s (code %s)%s on %s",
      tostring(m.type), mods, keyName(m.key), tostring(m.code), m["repeat"] and ", repeating" or "", tostring(m.target)
    )
    if m.type == "keydown" and not NOT_COUNTED[m.key] then
      since.keyCount += 1
      table.insert(since.keys, mods .. keyName(m.key))
      if #since.keys > MAX_KEYS then
        table.remove(since.keys, 1)
      end
    end
  elseif kind == "action" then
    if actions[m.action] == nil then
      log("page: an unknown button '%s'", tostring(m.action))
      return
    end
    -- W5: counted as the messages arrive, beside the page's own count of its clicks. A message
    -- that arrives twice shows as a gap between the two numbers; each new gap is a result.
    actions[m.action] += 1
    since.buttons[m.action] += 1
    log(
      "page: button '%s' pressed — message %d here, the page's count %s, click detail %s (0: not a mouse click); released in the page since the last button: %s",
      m.action, actions[m.action], tostring(m.count), tostring(m.detail),
      type(m.releases) == "table" and #m.releases > 0 and table.concat(m.releases, ", ") or "nothing"
    )
    local gap = (tonumber(m.count) or 0) - actions[m.action]
    if gap ~= gaps[m.action] then
      gaps[m.action] = gap
      result("W5: the page counted %s presses of '%s', the probe %d messages", tostring(m.count), m.action, actions[m.action])
    end
    -- W5, a hint for the log and no result: the same button activated again with nothing released
    -- in the page since its last activation — on 2026-09-28 NVDA's activation (click detail 1),
    -- then the Enter itself (detail 0). It is not proof: an Enter that activates on its key-down
    -- and takes the page away sends its key-up to the window in front then, and NVDA's own
    -- activation releases a pointer each time, doubled or not. W5's answer is the counts.
    if m.previous == m.action and m.released == false then
      log(
        "W5 hint: '%s' activated again with no key-up or pointer release in the page since its last activation (click detail %s) — one press arriving twice, or a key-up that went to another window",
        m.action, tostring(m.detail)
      )
    end
    if m.action == "click-plugin" then
      -- Never a second round trip on top of one: in its first two steps the page is behind the
      -- plug-in window, where no person can press its button. Once the page is back, a press is a
      -- round trip of its own, which clicks elsewhere than the one before (backgroundPoint).
      if w3 and w3.stage ~= "watch" then
        result("W5: 'click-plugin' again while the round trip before it is under way — ignored")
        return
      end
      clickIntoPlugin()
    elseif m.action == "back" then
      backToPlugin("the page's button")
    elseif m.action == "announce" then
      announce()
    end
  elseif kind == "loaded" then
    log(
      "page: document loaded, has the focus: %s; code built from a string %s; RTCPeerConnection is %s; "
        .. "navigator.clipboard.writeText is %s",
      tostring(m.hasFocus), tostring(m.evalRuns), tostring(m.rtc), m.clipboardWrite and "there" or "not there"
    )
  elseif kind == "check" then
    log("page check: %s — %s", tostring(m.what), tostring(m.status))
  elseif kind == "policy" then
    log("page: the policy refused %s for %s", tostring(m.directive), tostring(m.blocked))
  else
    log("page: %s %s", tostring(kind), host.json.encode(m))
  end
end

page = host.gui.webview({
  file = "ui/index.html",
  title = "HTML probe",
  onMessage = onMessage,
  onReady = function()
    ready = true
    log("the page is ready, %d ms after the last press", host.now() - pressedAt)
    if pending then
      local p = pending
      pending = nil
      showBeside(refreshed(p.plugin) or p.plugin, p.via .. " (after the build)", p.t0, p.after)
    else
      -- Prepared, and nobody waiting for it: said, so F7 need not be guessed at.
      result("the page is ready: prepared, not shown")
      mention("HTML probe ready.")
    end
  end,
  onClose = function(why)
    if why == "failed" then
      ready = false
    end
    result("the page was closed: %s", tostring(why))
  end,
})

-- ── The keys ──────────────────────────────────────────────────────────────────

-- The two show keys: the page beside the plug-in window in front, or — pressed in the page —
-- back to the plug-in. `key` names the key, `how` the way its press arrives outside the page; in
-- the page both arrive through RegisterHotKey, the keyboard hook being out of the chain there. A
-- press ends a W3 round trip still being watched.
local function onShowKey(key, how)
  local t0 = host.now()
  pressedAt = t0
  w3 = nil
  local f = host.window.foreground()
  if f and page.window and f.id == page.window and page:isShown() then
    backToPlugin(key .. ", pressed in the page")
    return
  end
  local via = string.format("%s (%s)", key, how)
  landing = nil
  local a = host.window.active()
  if isPluginWindow(a) then
    plugin = a
  else
    plugin = refreshed(plugin)
    if plugin then
      log("%s: the window in front is not a plug-in window; the page goes beside the one remembered", via)
    end
  end
  if not plugin then
    log("%s: no plug-in window in front and none remembered; in front: %s", via, frontWords())
    say("No plug-in window is in front.")
    return
  end
  log(
    "%s pressed; in front: %s; the plug-in window '%s' at %d,%d %dx%d",
    via, frontWords(), plugin.title or "", plugin.bounds.x, plugin.bounds.y, plugin.bounds.w, plugin.bounds.h
  )
  showBeside(plugin, via, t0, "the key's callback")
end

host.hotkey.register(KEY_SHOW, function()
  onShowKey("F7", "keyboard hook")
end)
host.hotkey.register(KEY_SHOW_FG, function()
  onShowKey("F8", "WM_HOTKEY")
end, { foreground = true })

host.hotkey.register(KEY_REPORT, function()
  local parts = {}
  if not reported then
    reported = true
    parts[1] = string.format("HTML probe revision %d.", REVISION)
  end
  if not ready then
    parts[#parts + 1] = "The page is not ready yet."
  end
  for _, line in ipairs(since.results) do
    parts[#parts + 1] = line
  end
  if #since.results == 0 then
    parts[#parts + 1] = "No results since the last report."
  end
  if w3 and w3.stage == "forward" then
    parts[#parts + 1] = "W3 is still waiting for the plug-in window to come to the front."
  end
  local b = since.buttons
  parts[#parts + 1] = string.format(
    "Since the last report: buttons: click into the plug-in %d, back %d, announce %d. Key presses the page saw: %d%s.",
    b["click-plugin"], b.back, b.announce,
    since.keyCount,
    #since.keys > 0 and (": " .. table.concat(since.keys, ", ")) or ""
  )
  say(table.concat(parts, " "))
  log("the report was said; counting anew")
  fresh()
  landing = nil
end)

-- ── The windows: W2 c, W3, W7 ─────────────────────────────────────────────────

-- Every foreground change, with how long after the last press it came — what W2 and W3 read the
-- order of events from. W3's plug-in window arriving goes on with its round trip; W7's has its
-- keyboard looked at. And, with the setting on, the page for a plug-in window that came to the
-- front from somewhere other than the page itself or that same window: an arrival, not a return,
-- decided by the window that was in front before rather than by any time.
local lastFront = nil
host.window.onTrigger({}, function(w)
  if not w then
    return
  end
  local previous = lastFront
  lastFront = w.id
  log("in front: '%s' (%s), %d ms after the last press", w.title or "", w.app and w.app.exe or "?", host.now() - pressedAt)
  if w3 and w3.stage == "forward" and w.id == w3.plugin.id then
    w3Arrived(w)
    return
  end
  checkLanding()
  if not isPluginWindow(w) then
    return
  end
  -- The first plug-in window of the session, or the first since the page failed: the page is
  -- built now, hidden, so that WebView2's cold start is over before the first key.
  if not ready then
    local ok, why = page:prepare()
    if not ok and why ~= "not ready yet" then
      log("the page could not be prepared: %s", tostring(why))
    end
  end
  if not host.settings.get(AUTO) then
    return
  end
  if previous == w.id or (page.window ~= nil and previous == page.window) then
    return
  end
  plugin = w
  pressedAt = host.now()
  showBeside(w, "automatic show", pressedAt, "the plug-in window came forward")
end)

-- The keyboard moving inside a window: W7's landing, when the window's foreground event came
-- before its control had the keyboard. The first focus event after an answer ends the watch.
host.window.onFocus(function()
  checkLanding()
  if landing and landing.last then
    landing = nil
  end
end)

host.settings.onChange(AUTO, function(v)
  log("automatic show is now %s", v and "on" or "off")
end)

log(
  "revision %d ready — %s shows the page (keyboard hook), %s shows it (WM_HOTKEY), %s says what happened; automatic show is %s",
  REVISION,
  host.keys.describe(KEY_SHOW, { style = "short" }),
  host.keys.describe(KEY_SHOW_FG, { style = "short" }),
  host.keys.describe(KEY_REPORT, { style = "short" }),
  host.settings.get(AUTO) and "on" or "off"
)
