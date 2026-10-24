-- HTML page probe (dev tool, Windows and macOS): the maintainer's NVDA test of `host.gui.webview`,
-- stage 1 of the HTML plan of 2026-09-28, and the Mac tester's VoiceOver test of the same pages on
-- a Mac (stage 4). It answers the questions W1–W10 and MW1–MW21 in TODO.md — whether the screen
-- reader finds its way into a page shown beside a plug-in window, which way of pressing a key
-- gives the page the front, what a click into the plug-in and back costs, which keys reach the
-- page — for a person who cannot see the screen: everything it observes goes into the log, and
-- one key says what happened since it was last pressed.
--
-- It binds nothing to a plug-in and adds no overlay. Test it on a plug-in without one (ReaEQ in
-- REAPER). Its keys on Windows, all free there with NVDA — the four-modifier chord the platform's
-- own reload (F5) and plug-in focus key (F6) use, one function key further along each:
--   Ctrl+Shift+Win+Alt+F7  the page beside the plug-in window in front, the way every hotkey
--                          arrives: matched in the keyboard hook (W2 a). Pressed while the page
--                          is in front, it goes back to the plug-in (W7) — there it arrives
--                          through RegisterHotKey, the hook being out of the chain while a
--                          window of this application is in front.
--   Ctrl+Shift+Win+Alt+F8  the same, registered with { foreground = true }: Windows delivers it
--                          as WM_HOTKEY (W2 b).
--   Ctrl+Shift+Win+Alt+F9  says what happened since it was last pressed, and counts anew.
--   Ctrl+Shift+Win+Alt+F10 remembers the mouse pointer's point in the plug-in window as a target
--                          for W10 (below) — put the pointer on the control first, with NVDA's
--                          "move mouse to navigator object" or a module's overlay.
-- On a Mac the four-modifier chord holds Control and Option together, VoiceOver's own modifier,
-- which takes it before any application sees it (the third Mac session); so the Mac's keys are
-- Command-Shift chords, as the platform's own are there (Command-Shift-F5 and F6), clear of the
-- window probe's F9 and F10, which ship in the Mac's download:
--   Command-Shift-F7   the page beside the plug-in window in front, with the keyboard (MW1a);
--                      pressed in a page, back to the plug-in (MW3). Every hotkey on a Mac is the
--                      system's own delivery, so `{ foreground = true }`, and W2 with it, asks
--                      nothing there.
--   Command-Shift-F8   the same page described `activates = false` (MW1b) instead: a panel that
--                      takes the keyboard while the DAW stays the active application. Pressed in a
--                      page, back to the plug-in.
--   Command-Shift-F11  says what happened since it was last pressed, and counts anew.
--   Command-Shift-F12  a captured key, not a hotkey: the page F7 shows, beside the plug-in window
--                      without the keyboard (MW2), and pressed again while it is shown so, its live
--                      region updated (MW16). Pressed in a page it must not fire — no capture
--                      applies while a window of this application has the keyboard (MW16) — and
--                      the page gets it as a key instead.
--   Command-Shift-F4   W10's target, as Windows' F10 (below): the pointer's point in the plug-in
--                      window remembered — put the pointer on the control first, with VoiceOver's
--                      "move the mouse to the VoiceOver cursor" (VO-Command-F5) or a module's
--                      overlay.
-- And one setting, off by default: the tool page comes up by itself when a plug-in window comes
-- to the front from another window (W2 c) — never the application window, which is reached as any
-- program's window is.
--
-- W10 (clicking without the round trip, from the page): the page's "Read the plug-in's elements"
-- lists what the accessibility tree has of the plug-in (UIA on Windows, AX on a Mac); then a way —
-- nothing (only watch), the round trip, a posted click, a posted click with the pointer left where
-- it is, and on Windows the no-activate style and an accessibility press — and 1, 10 or 20 times;
-- "Try it". Each attempt puts the pointer on the target first for the ways that move it, reads the
-- plug-in near the target before and after, takes a baseline picture, acts, waits for the picture
-- to change, and says what happened without cutting the screen reader off: whether another window
-- came forward, whether the pixels at the target changed, the text before and after, and the
-- element's state where it is readable. The round trip goes as W3's, each step on the event that
-- ends the one before. F7 or F8 gives a run up. On a Mac (MW21) the page offers the ways a Mac has
-- — the no-activate style is a Windows window style, and `host.element.press` answers "not
-- supported on this platform yet" there — and its target key is Command-Shift-F4, the window probe
-- holding Command-Shift-F10.
--
-- Two pages from the same files: the tool page of W1–W9, in no list of windows, and the same page
-- in an application window (`window = "app"`), in Alt+Tab and on the taskbar, for W6 again — the
-- maintainer's decision of 2026-10-11 that a module may have a real window. The page's button
-- "Show as an application window" shows the other beside the plug-in window and then hides the
-- tool page, so the front goes from page to page and never through the plug-in window; there,
-- "Show as a tool window" goes back the same way. The keys work on the one shown last. "Back to
-- the plug-in" hides the tool page and leaves the application window open behind the plug-in
-- window, as any program's window stays: Alt+Tab, or F7, brings it back.
--
-- On a Mac a third page from the same files: the tool page described `activates = false`
-- (experimental), titled "HTML probe without activation", which only Command-Shift-F8 shows. A
-- key shows its page and leaves the others as they are, so this one can take the keyboard with
-- the tool page shown beside the plug-in window without it (Command-Shift-F12) — a window of the
-- application's own that AppKit may give the keyboard to as this one leaves (MW1b).
--
-- The tool page is prepared — built, hidden — the first time a plug-in window comes to the
-- front, so WebView2's cold start (W8) is over before the first key; `page:prepare()` again after
-- a close. The application window is built at the first switch to it, and the page without
-- activation at its first key, so a run that does not need them does not wait for their builds
-- too.
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
-- keeps the page when Windows declines (2026-10-11). 4: the page in an application window as well,
-- for W6 again (2026-10-11). 5: W10, the ways of acting in the plug-in without the round trip
-- (host.input.click's via = "post"/"noactivate" and host.element.press), measured from the page
-- (2026-10-11). 6: the probe on a Mac, for MW1–MW19 (2026-10-12), made the same day on another
-- line of work. 7: the two together, where those lines met (2026-10-12): W10 on Windows, the
-- Mac's keys and its page without activation on a Mac. 8: W10 on a Mac as well, with the posted
-- clicks a Mac has now (2026-10-16, MW21).
local REVISION = 8

-- The keys, each platform's (above). F8 is Windows' own delivery there (W2 b) and the page
-- without activation on a Mac (MW1b). W10's target key is F10 on Windows and F4 on a Mac, where
-- the window probe holds F10; the captured key is a Mac's alone.
local KEY_SHOW = host.os.pick({ windows = "Ctrl+Shift+Win+Alt+F7", macos = "Cmd+Shift+F7" })
local KEY_SHOW_FG = host.os.pick({ windows = "Ctrl+Shift+Win+Alt+F8", macos = "Cmd+Shift+F8" })
local KEY_REPORT = host.os.pick({ windows = "Ctrl+Shift+Win+Alt+F9", macos = "Cmd+Shift+F11" })
local KEY_TARGET = host.os.pick({ windows = "Ctrl+Shift+Win+Alt+F10", macos = "Cmd+Shift+F4" })
local KEY_UNFOCUSED = host.os.pick({ macos = "Cmd+Shift+F12" })

-- W10's ways on this platform, as the page's Way select offers them (its options with these
-- values; the others are taken out as it loads): on a Mac the posted clicks and the round trip,
-- not the no-activate style, a Windows window style, nor the accessibility press, which a Mac does
-- not have yet.
local W10_WAYS = host.os.pick({
  windows = { "watch", "round", "post", "post-stay", "noactivate", "press" },
  macos = { "watch", "round", "post", "post-stay" },
})

-- How a show key's press arrives, in the results' words: on Windows F7's is matched in the
-- keyboard hook and F8's left to Windows (W2); on a Mac every hotkey is the system's own, and F8
-- names the page it shows.
local HOW = host.os.pick({
  windows = { F7 = "keyboard hook", F8 = "WM_HOTKEY" },
  macos = { F7 = "hotkey", F8 = "page without activation" },
})

-- The modifiers the page reports of a key, in this platform's words for the W4 lines: the DOM's
-- `ctrlKey`, `altKey`, `shiftKey` and `metaKey` are the keys so labelled, which on a Mac are
-- Control, Option, Shift and Command — in the order Apple prints them.
local MOD_WORDS = host.os.pick({
  windows = { ctrl = "Ctrl+", alt = "Alt+", shift = "Shift+", meta = "Win+" },
  macos = { ctrl = "Control+", alt = "Option+", shift = "Shift+", meta = "Cmd+" },
})

-- What a show's `true` says. On Windows the page is in front when the call returns, so one that
-- is not is a result. On a Mac it says that the front was asked for: this application's activation
-- comes on the event loop after the callback, and so does AppKit making the page's window the key
-- window — unless this application is the active one already, or the page is the one described
-- `activates = false`. There a page not yet in front is the expected answer, and the page's own
-- focus line (`… the page has the focus N ms after …`) says whether it came (MW1a).
local SHOW_ASKS = host.os.pick({ windows = false, macos = true })

local AUTO = "autoShow"
host.settings.define(AUTO, false, {
  label = "Show the tool page by itself when a plug-in window comes to the front from another window (test W2 c)",
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
  since = { results = {}, buttons = { ["click-plugin"] = 0, back = 0, switch = 0, announce = 0 }, keys = {}, keyCount = 0 }
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

-- Each page's buttons, counted over the whole session as their messages arrive, beside the page's
-- own count of its clicks: a press that arrived twice leaves the two apart (W5). Per page, as the
-- page counts its own.
local function buttonCounts()
  return { ["click-plugin"] = 0, back = 0, switch = 0, announce = 0 }
end

-- The pages, each with its kind, its title, its words for the results, whether it is built and
-- loaded — until then a plug-in window coming to the front prepares the tool page — and its
-- buttons' counts. Described below; their callbacks close over them.
local tool = { kind = "tool", title = "HTML probe", words = "tool page", ready = false, actions = buttonCounts(), gaps = buttonCounts() }
local app = { kind = "app", title = "HTML probe window", words = "application window", ready = false, actions = buttonCounts(), gaps = buttonCounts() }
-- On a Mac, the tool page described `activates = false` (MW1b); none on Windows, where the
-- option changes nothing.
local noActivation = host.os.pick({
  macos = { kind = "tool", title = "HTML probe without activation", words = "page without activation", activates = false,
    ready = false, actions = buttonCounts(), gaps = buttonCounts() },
})
local pages = { tool, app, noActivation }
-- The page the keys and buttons work on: the one shown last.
local cur = tool
-- The page F7 shows, and the captured key: the tool page, or the application window once a switch
-- has made it the page in use — never the page without activation, which only F8 shows on a Mac.
-- The automatic show (W2 c) shows it while it is the tool page.
local keyed = tool

-- The plug-in window the page was last shown beside, as a window table.
local plugin = nil
-- A show that answered `not_ready`: shown again once the page is ready.
local pending = nil
-- The show whose "page has the focus" is still to come.
local measuring = nil
-- The last press of one of the probe's keys or buttons, for how long after it each thing happened.
local pressedAt = host.now()
-- W3's round trip while it is under way: "forward" (the plug-in window asked to the front),
-- "click" (clicked, its picture asked for), "watch" (the page back in front, watched for losing
-- it again). nil when none is. A second "Click into the plug-in" while one is under way is not a
-- second round trip (see onMessage).
local w3 = nil
-- Whether "the page covers part of the plug-in window" has been said, for the shows that answer
-- so in a row.
local overSaid = false
-- W3's last click, which the next one keeps away from (see backgroundPoint).
local lastClick = nil
-- W7: after "back to the plug-in", the window whose keyboard is to be looked at once it is in
-- front.
local landing = nil

-- W10: the remembered target (a point in the plug-in window's client area, as an offset so it
-- survives the window moving), the plug-in's elements as the last read found them (name, type and
-- a point of their own, each an offset too), and the run under way — nil when none is. A second
-- "Try it" while one runs is ignored.
local w10 = { target = nil, elements = {}, run = nil }

-- Keys that are only modifiers: logged, not counted — pressed in the page, they are the report
-- key's own, whose F9 RegisterHotKey takes there (the keyboard hook is out of the chain while the
-- page is in front), and on a Mac whose F11 the system's hotkey delivery takes. Nor the key the
-- hook presses to mask a chord it swallowed, which has no virtual key of its own (`Unidentified`
-- to the page): it reaches the page only when the hook swallowed a chord in the moment it was
-- going out.
local NOT_COUNTED = { Control = true, Shift = true, Alt = true, Meta = true, OS = true, AltGraph = true, Unidentified = true }

-- Whether `w` is a plug-in window of a DAW daw-hosts knows: an entry's matcher takes it, and the
-- entry's `focusTarget` too where it has one (REAPER's matcher takes every dialog of its process).
-- The matcher is the entry without its own fields (`daw.matcher`), which host.window.test raises for.
local function isPluginWindow(w)
  if not w then
    return false
  end
  for _, entry in ipairs(daw.all) do
    if host.window.test(daw.matcher(entry), w) and (entry.focusTarget == nil or host.window.test(entry.focusTarget, w)) then
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
  for _, rec in ipairs(pages) do
    if rec.page and rec.page.window and f.id == rec.page.window then
      return rec == cur and "the page" or ("the " .. rec.words)
    end
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
-- beside it, a maximised window say: the partial answer, `true` with an error value beside it
-- (`too_large`, docs/api/gui.md). Said once for a run of such shows, and in words for the result.
local function overWords(err)
  if err == nil then
    overSaid = false
    return ""
  end
  if not overSaid then
    overSaid = true
    -- Not over the screen reader saying the page, which comes to the front with this show.
    mention("The page covers part of the plug-in window: there is no room beside it.")
  end
  return " (over part of the plug-in window: no room beside it)"
end

-- Shows page `rec` beside `p`, and makes it the page in use. `via` names the way it was asked
-- for; `t0` is when, and `after` what happened at `t0`, for the result. `replaces`: the page it is
-- shown in place of (the switch), hidden only once this one is shown — out of the front by then,
-- it hands nothing on — and kept, the page in use still, when this one is not shown. A show that
-- succeeds ends one still waiting for its build.
local function showBeside(rec, p, via, t0, after, replaces)
  local wasShown = rec.page:isShown()
  local ok, err = rec.page:show({ focus = true, avoid = p.bounds })
  local ms = host.now() - t0
  if ok then
    pending = nil
    cur = rec
    if rec ~= noActivation then
      keyed = rec
    end
    if replaces and replaces ~= rec and replaces.page:isShown() then
      replaces.page:hide()
    end
    measuring = { via = via, t0 = t0, after = after }
    local over = overWords(err)
    local front = frontWords()
    if front == "the page" then
      result("%s: accepted, %d ms after %s; in front: the page%s", via, ms, after, over)
    elseif SHOW_ASKS then
      result("%s: the front asked for, %d ms after %s; in front so far: %s%s", via, ms, after, front, over)
    else
      result("%s: ACCEPTED BUT NOT IN FRONT, %d ms after %s; in front: %s%s", via, ms, after, front, over)
    end
  elseif err.code == host.errors.NOT_READY then
    pending = { via = via, t0 = t0, after = after, plugin = p, rec = rec, replaces = replaces }
    log("%s: the page is being built; it is shown when it is ready", via)
    say("Building the page.")
  else
    -- A switch declined: the other page is on screen without the keyboard, beside the one in
    -- use, and goes again unless it was open before.
    if replaces and replaces ~= rec and not wasShown and rec.page:isShown() then
      rec.page:hide()
    end
    result("%s: not shown, %s, %d ms after %s; in front: %s", via, tostring(err), ms, after, frontWords())
    say("Page not shown: " .. tostring(err))
  end
end

-- W7: back to the plug-in — its window to the front first, while the page still is, then the
-- tool page out of sight; the application window stays open behind it, as any program's window
-- does. Where the keyboard lands is looked at once the window is in front. Declined, the page
-- stays where it is, in front, rather than leave the keyboard to whichever window Windows picks:
-- F7 and Alt+F4 still work there. A switch still waiting for its build is given up: the way back
-- is the newer wish.
local function backToPlugin(via)
  w3 = nil
  pending = nil
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
  landing = { via = via, id = p.id, t0 = t0 }
  if cur == app then
    result("%s: back to the plug-in accepted; the application window stays open; in front now: %s", via, frontWords())
    return
  end
  cur.page:hide()
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

-- W3's click on Windows: a point of the plug-in window's own background — inside its client area,
-- and at least CLEAR pixels from its edges and from every control in it — with the words for the
-- log line, or nil and the words for why there is none.
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
local function noBackground()
  return nil, "it has no spot of its own background clear of its controls" .. (lastClick and " and away from the last click" or "")
end
local function backgroundPoint(w)
  local c = w.client
  if not c or c.w < 2 * CLEAR or c.h < 2 * CLEAR then
    return noBackground()
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
  if not best then
    return noBackground()
  end
  return best, string.format("its own background, %d px from its nearest control or edge", bestRoom)
end

-- W3's click on a Mac (MW4): a point of the plug-in window's title bar — half way down it, across
-- its middle, clear of the window's three buttons on its left — with the words for the log line,
-- or nil where the window has no title bar, its content beginning at its frame (Logic's plug-in
-- windows, as the host reports them). Not the window's own background, as on Windows:
-- `controls()` lists containers only on a Mac, never a button, a slider or a field, so the
-- background cannot be told from the controls on it, and a click there could turn a knob or open
-- a menu. One click on a title bar brings its window forward, which it is already, and does
-- nothing else. Never where the last W3 click was: two round trips close together would be a
-- double click, which zooms a window there, so a click within APART of the last one along the
-- bar goes twice that further along it, or back.
local BAR_BUTTONS = 80
local BAR_LEAST = 16
local function titleBarPoint(w)
  local b, c = w.bounds, w.client
  local bar = (b and c) and (c.y - b.y) or 0
  if bar < BAR_LEAST or b.w < BAR_BUTTONS + CLEAR + 4 * APART then
    return nil, "it has no title bar of its own to click"
  end
  local lo, hi = b.x + BAR_BUTTONS, b.x + b.w - 1 - CLEAR
  local x = math.clamp(b.x + b.w // 2, lo, hi)
  if lastClick and math.abs(x - lastClick.x) < APART then
    x = if x + 2 * APART <= hi then x + 2 * APART else x - 2 * APART
  end
  return { x = x, y = b.y + bar // 2 }, "its title bar"
end

-- The click of the round trip, each platform's (above).
local clickSpot = host.os.pick({ windows = backgroundPoint, macos = titleBarPoint })

-- MW4, a Mac's: whether the page's level follows the keyboard — floating while the page has it,
-- the normal level otherwise — so that the plug-in window, once it has the keyboard, is drawn over
-- a page that covers part of it. Beside the plug-in window, where a show with `avoid` puts the page
-- whenever there is room, the level changes nothing a click or a read of the plug-in window meets,
-- and it is not seen here: the log says so. Over part of it — a window with no room beside it —
-- the window server is asked at the middle of the part the page covers, once the plug-in window is
-- in front: `false` is the page, or something else, still drawn over it there (the host's log
-- names that window's application and level). The page's place is its window table, listed among
-- this application's windows; this application's process is the one the window in front belongs
-- to when the page's "Click into the plug-in" arrives (`ownPid`).
local ownPid = nil
local function levelSeen(w)
  local id, pg = cur.page.window, nil
  if ownPid and id then
    for _, x in ipairs(host.window.list({ pids = { ownPid } })) do
      if x.id == id then
        pg = x
        break
      end
    end
  end
  if not pg then
    log("MW4: the %s's place is not known, so whether its level follows the keyboard is not seen", cur.words)
    return
  end
  local a, b = pg.bounds, w.bounds
  local x1, y1 = math.max(a.x, b.x), math.max(a.y, b.y)
  local x2, y2 = math.min(a.x + a.w, b.x + b.w), math.min(a.y + a.h, b.y + b.h)
  if x1 >= x2 or y1 >= y2 then
    log("MW4: the %s is beside the plug-in window, so whether its level follows the keyboard is not seen", cur.words)
    return
  end
  local x, y = (x1 + x2) // 2, (y1 + y2) // 2
  local owns = host.window.ownsPoint(w.id, x, y)
  if owns == false then
    result("MW4: the plug-in window is in front, and something is STILL DRAWN OVER it at %d,%d, where the %s covers it", x, y, cur.words)
  elseif owns then
    result("MW4: the plug-in window is in front, and drawn over the %s where that covers it, at %d,%d", cur.words, x, y)
  else
    log("MW4: the window server could not say which window is drawn at %d,%d, where the %s covers the plug-in window", x, y, cur.words)
  end
end
local seeLevel = host.os.pick({ macos = levelSeen })

-- W3: into the plug-in and back, as an overlay's action on a page would be — and every step on
-- the event that says the one before is done. The plug-in window is asked to the front — in front
-- of the page, which may be over part of it (no room beside a maximised window); when it is there
-- (its foreground event, or a focus event that finds it with the keyboard: below), one click goes
-- onto its own background — on a Mac its title bar (above) — or none where it has no such spot —
-- then the round trip is the change of windows alone — and a picture of the plug-in is asked for,
-- off the event loop, as an overlay reads after its click; in that picture's answer, once the
-- click has gone through the system, the page is asked back. Then whether it keeps the front: a
-- blur of the page's with another window in front says when it does not.
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
  if seeLevel then
    -- The page pressed in has the keyboard: the window in front is this application's.
    local f = host.window.foreground()
    if f and f.own then
      ownPid = f.pid
    end
  end
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
  if seeLevel then
    seeLevel(w)
  end
  local spot, where = clickSpot(w)
  if not spot then
    log("W3: the plug-in window came to the front %d ms after the press; %s, so nothing is clicked — the round trip is the change of windows alone", host.now() - w3.t0, where)
  elseif host.window.ownsPoint(w.id, spot.x, spot.y) == false then
    -- Nothing clicked through another window; the round trip goes on without the click, so the
    -- page still comes back.
    result("W3: something else is drawn over the plug-in window at %d,%d; no click", spot.x, spot.y)
    say("Something covers the plug-in window; no click.")
    spot = nil
  else
    host.input.click(spot.x, spot.y)
    lastClick = spot
    log("W3: the plug-in window came to the front %d ms after the press; clicked at %d,%d (%s)", host.now() - w3.t0, spot.x, spot.y, where)
  end
  w3.clicked = spot ~= nil
  local c = w.client
  local region = { c.x, c.y, c.x + math.clamp(c.w, 1, 400), c.y + math.clamp(c.h, 1, 300) }
  host.screen.snapshot({ region = region, key = "w3" }, function(snap, why)
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
    local ok, note = cur.page:show({ focus = true, avoid = now })
    local front = frontWords()
    if ok then
      w3.stage = "watch"
      w3.backAt = host.now()
      measuring = { via = "W3 back", t0 = t, after = "the page was asked back" }
      if SHOW_ASKS and front ~= "the page" then
        -- On a Mac the front only asked for (SHOW_ASKS): the page's focus line follows.
        result(
          "W3: the page asked back%s, %d ms after the press; in front so far: %s%s; %s.",
          w3.clicked and "" or " (no click)", host.now() - w3.t0, front, overWords(note), kept
        )
      else
        result(
          "W3: the page came back%s, accepted, %d ms after the press; in front: %s%s; %s. It has kept the front so far.",
          w3.clicked and "" or " (no click)", host.now() - w3.t0, front, overWords(note), kept
        )
      end
    else
      result("W3: the page did not come back: %s, %d ms after the press; in front: %s; %s", tostring(note), host.now() - w3.t0, front, kept)
      w3 = nil
    end
  end)
end

-- ── W10: acting in the plug-in without the round trip ──────────────────────────

-- A reading's text, or "" when it read nothing: what the before/after reads compare.
local function readingText(r)
  if r and r.status == "text" then
    return r.text
  end
  return ""
end

-- Short enough to say and to read in the log.
local function short(s)
  if #s > 40 then
    return s:sub(1, 40) .. "…"
  end
  return s
end

-- What an element says about its own state, where it is readable, handed to `done` — the plug-in
-- window's title is tried as the container (its UIA root carries it), so a Window/Pane named for
-- it is the subtree searched; "—" when nothing answers. Asked with a callback, so the event loop
-- never waits for the plug-in's answer; at once for the point, which has no element to ask about.
local function stateWords(p, el, done)
  if not el then
    done("—")
    return
  end
  host.element.stateProbe(p.id, p.title or "", el.name, el.ctype, function(st)
    if not st then
      done("—")
    elseif st.toggle ~= nil then
      done(st.toggle == 1 and "on" or "off")
    elseif st.checked then
      done("checked")
    else
      done(string.format("state 0x%x", st.legacyState))
    end
  end)
end

-- A box of about ±64 px around the target, inside the plug-in window's client area: what the
-- reads read and the picture watches.
local function w10Region(x, y, p)
  local c = p.client
  local x1 = math.max(c.x, x - 64)
  local y1 = math.max(c.y, y - 64)
  local x2 = math.min(c.x + c.w, x + 64)
  local y2 = math.min(c.y + c.h, y + 64)
  if x2 - x1 < 2 then
    x2 = math.min(c.x + c.w, x1 + 2)
  end
  if y2 - y1 < 2 then
    y2 = math.min(c.y + c.h, y1 + 2)
  end
  return { x1, y1, x2, y2 }
end

-- The chosen target as a screen point now (offsets kept, so a moved window is followed), the
-- element it is where it is one, and its words for the result.
local function w10Resolve(p)
  local run = w10.run
  local c = p.client
  if run.target == "point" then
    if not w10.target then
      return nil
    end
    return c.x + w10.target.dx, c.y + w10.target.dy, nil, "the remembered point"
  end
  local i = tonumber(string.match(tostring(run.target), "^el:(%d+)$"))
  local el = i and w10.elements[i]
  if not el then
    return nil
  end
  local words = string.format("%q", el.name)
  if (el.alike or 1) > 1 then
    -- The press acts on the first of them the plug-in lists, which need not be this one.
    words = string.format("%s (one of %d alike)", words, el.alike)
  end
  return c.x + el.dx, c.y + el.dy, el, words
end

-- Ends the run, saying `why` when it ended for a reason (a gone window or target) rather than
-- having finished.
local function w10Finish(why)
  w10.run = nil
  if why then
    result("W10: %s", why)
    say(why)
  end
end

-- The ways that put the pointer on the target as they click. An attempt with one of them puts it
-- there before anything is read or pictured, so a control that lights up under the pointer is lit
-- in the baseline too, and only what the click did tells the two pictures apart.
local MOVES_POINTER = { round = true, post = true, noactivate = true }

-- How long a changed picture has to stand still before the change wait answers it.
local SETTLE_MS = 150

-- What the change wait saw, in words. `info.waited` counts from the wait's start — the act just
-- before it — to the picture it answered with, the stretch that picture stood still included.
local function pictureWords(info)
  if not (info and info.changed == true) then
    return "no change"
  end
  if info.settled then
    return string.format("changed, settled %d ms after the wait began (the last %d ms of them still)", info.waited or 0, SETTLE_MS)
  end
  return string.format("changed, still changing when the wait ended after %d ms", info.waited or 0)
end

-- The act of `run`'s way at (x, y): nothing, the round trip, a posted click, a posted click with
-- the pointer left where it is, the no-activate style, or an accessibility press. `done(made,
-- why)` ends it — at once for the ones that act on the event loop, in a callback for the ones a
-- thread makes. The round trip is W3's, every step on the event that says the one before is done:
-- the plug-in window asked to the front here, the click on its foreground event (onTrigger, below),
-- `done` after the click, and the page back once the change wait has answered (w10Attempt) — so the
-- click never lands after the page is back, and the next attempt starts with the page in front.
local function w10Act(run, x, y, el, p, done)
  local way = run.way
  if way == "watch" then
    done(true, nil)
  elseif way == "round" then
    if not host.window.focus(p.id) then
      done(false, "the plug-in would not come to the front")
      return
    end
    run.forward = { id = p.id, x = x, y = y, done = done, t0 = host.now() }
  elseif way == "post" then
    host.input.click(x, y, { window = p.id, via = "post" }, done)
  elseif way == "post-stay" then
    host.input.click(x, y, { window = p.id, via = "post", move = false }, done)
  elseif way == "noactivate" then
    host.input.click(x, y, { window = p.id, via = "noactivate" }, done)
  elseif way == "press" then
    if not el then
      done(false, "the accessibility press needs an element, not the point")
      return
    end
    host.element.press(p.id, el.name, el.ctype, done)
  else
    done(false, "unknown way")
  end
end

-- One attempt, every step on the event that ends the one before (no timer): the pointer to the
-- target for the ways that move it, the element's state, the before read, a baseline picture, the
-- act, a wait for the picture to change, the page back for the round trip, the state and the read
-- after — then the result, and the next repetition if any. Nothing here interrupts what the screen reader is saying. A run given up
-- meanwhile (F7, F8) leaves its answers to an attempt nobody waits for: each step asks first
-- whether its run is still the one under way.
local function w10Attempt()
  local run = w10.run
  if not run then
    return
  end
  local function live()
    return w10.run == run
  end
  local p = refreshed(plugin)
  if not p then
    return w10Finish("the plug-in window is gone")
  end
  local x, y, el, label = w10Resolve(p)
  if not x then
    return w10Finish("the target is gone")
  end
  run.label = label
  run.otherFront = false
  if MOVES_POINTER[run.way] then
    host.input.move(x, y)
  end
  local region = w10Region(x, y, p)
  stateWords(p, el, function(beforeState)
    if not live() then
      return
    end
    host.ocr.recognize(region, { key = "w10" }, function(rBefore)
      if not live() then
        return
      end
      local beforeText = readingText(rBefore)
      host.screen.snapshot({ region = region, key = "w10" }, function(base)
        if not live() then
          if base then
            base:release()
          end
          return
        end
        -- Kept on the run as well, for a round trip given up while it waits for the plug-in window:
        -- nothing else lets go of it then (onShowKey).
        run.base = base
        local frontBefore = frontWords()
        local change = { timeout = 1500, settle = SETTLE_MS }
        if base then
          change.from = base
        end
        w10Act(run, x, y, el, p, function(made, why)
          if not live() then
            if base then
              base:release()
            end
            return
          end
          host.screen.snapshot({ region = region, key = "w10", change = change }, function(snap, _why, info)
            if base then
              base:release()
            end
            if not live() then
              if snap then
                snap:release()
              end
              return
            end
            -- The round trip's last step: the page back, once the click has shown in the picture or
            -- the wait gave up. A page that does not come back ends the run after this attempt.
            if run.way == "round" then
              local ok, note = cur.page:show({ focus = true, avoid = (refreshed(p) or p).bounds })
              if not ok then
                run.stop = "the page did not come back: " .. tostring(note)
              end
            end
            local frontAfter = frontWords()
            local record = function(afterState, afterText)
              if not live() then
                return
              end
              run.n += 1
              if made then run.stats.made += 1 end
              local picChanged = info ~= nil and info.changed == true
              if picChanged then run.stats.pic += 1 end
              local textChanged = beforeText ~= afterText
              if textChanged then run.stats.text += 1 end
              local kept = not run.otherFront and frontBefore == "the page" and frontAfter == "the page"
              if kept then run.stats.front += 1 end
              local line = string.format(
                "W10 %s on %s (%d/%d): %s; front: %s; picture: %s; text: %s; state: %s",
                run.way, run.label, run.n, run.reps,
                made and "made" or ("NOT made (" .. tostring(why) .. ")"),
                kept and "the page throughout" or ("changed — now " .. frontAfter),
                pictureWords(info),
                textChanged and string.format("%q to %q", short(beforeText), short(afterText)) or "unchanged",
                beforeState == afterState and beforeState or (beforeState .. " to " .. afterState)
              )
              if run.reps == 1 then
                result("%s", line)
              else
                log("%s", line)
              end
              if run.n < run.reps and not run.stop then
                w10Attempt()
                return
              end
              w10.run = nil
              if run.reps > 1 then
                local summary = string.format(
                  "W10 %s on %s x%d: made %d, picture changed %d, text changed %d, front kept %d",
                  run.way, run.label, run.n, run.stats.made, run.stats.pic, run.stats.text, run.stats.front
                )
                result("%s", summary)
                mention(summary)
              else
                mention(line)
              end
              if run.stop then
                result("W10: %s", run.stop)
                mention(run.stop)
              end
            end
            -- The state after, then the read after: the result once both have answered.
            stateWords(refreshed(p) or p, el, function(afterState)
              if not live() then
                if snap then
                  snap:release()
                end
                return
              end
              if snap then
                host.ocr.recognize(region, { snapshot = snap, key = "w10after" }, function(rAfter)
                  snap:release()
                  record(afterState, readingText(rAfter))
                end)
              else
                record(afterState, "")
              end
            end)
          end)
        end)
      end)
    end)
  end)
end

-- The target key (F10 on Windows, F4 on a Mac): remember the pointer's point in the plug-in window
-- in front (or the one remembered), as an offset into its client area, and say so. The pointer is put there by the tools the maintainer
-- already has — NVDA's "move mouse to navigator object", a module's overlay.
local function rememberTarget()
  pressedAt = host.now()
  local a = host.window.active()
  if isPluginWindow(a) then
    plugin = a
  else
    plugin = refreshed(plugin)
  end
  if not plugin then
    say("No plug-in window is in front.")
    log("W10: %s pressed, but no plug-in window is in front or remembered", host.keys.describe(KEY_TARGET, { style = "short" }))
    return
  end
  local c = plugin.client
  local pt = host.input.cursorPos()
  w10.target = { dx = pt.x - c.x, dy = pt.y - c.y }
  log(
    "W10: target remembered at %d,%d on screen (%d,%d in the plug-in window '%s' client area)",
    pt.x, pt.y, w10.target.dx, w10.target.dy, plugin.title or ""
  )
  say(string.format("Target remembered: %d, %d in the plug-in window.", w10.target.dx, w10.target.dy))
end

-- The named UIA elements of plug-in window `p` in its dump `nodes` with a rectangle inside its
-- client area (at most 60), each as an offset so a moved window is followed, posted to the page
-- `rec` that asked for its Target select.
local function listElements(rec, p, nodes)
  local c = p.client
  local out = {}
  for _, e in ipairs(nodes) do
    local b = e.bounds
    if e.name ~= "" and b and b.w > 0 and b.h > 0
      and b.x >= c.x - 1 and b.y >= c.y - 1
      and b.x + b.w <= c.x + c.w + 1 and b.y + b.h <= c.y + c.h + 1 then
      out[#out + 1] = { name = e.name, ctype = e.ctype, dx = b.x + b.w // 2 - c.x, dy = b.y + b.h // 2 - c.y }
      if #out >= 60 then
        break
      end
    end
  end
  w10.elements = out
  -- Elements that share their name and type: a press finds the first of them the plug-in lists
  -- (host.element.press, as find), which need not be the one chosen and watched here — so each is
  -- marked in the list, and how many there are is in the log.
  local alike = {}
  for _, el in ipairs(out) do
    local k = el.name .. "\0" .. el.ctype
    alike[k] = (alike[k] or 0) + 1
  end
  local shared = 0
  local items = {}
  for i, el in ipairs(out) do
    el.alike = alike[el.name .. "\0" .. el.ctype]
    local label = string.format("%s (type %d)", el.name, el.ctype)
    if el.alike > 1 then
      shared += 1
      label = string.format("%s, one of %d alike: a press acts on the first of them", label, el.alike)
    end
    items[i] = { value = "el:" .. i, label = label }
  end
  rec.page:post({
    kind = "w10elements",
    target = w10.target and string.format("%d, %d", w10.target.dx, w10.target.dy) or nil,
    elements = items,
  })
  log(
    "W10: %d named element(s) of the plug-in inside its client area read; %d of them share their name and type with another, and a press acts on the first of those the plug-in lists",
    #out, shared
  )
  mention(string.format("%d elements read.", #out))
end

-- The page's "Read the plug-in's elements": the remembered plug-in window's dump, listed for the
-- page `rec` (listElements). Asked with a callback, so the event loop never waits for the plug-in's
-- answer; a dump that is not all of the tree says why in the log.
-- Said when a W10 request finds no plug-in window remembered: the target key by its function key,
-- as the report names F7.
local function noTarget()
  say(string.format(
    "No plug-in window remembered. Press %s with the pointer on the control first.",
    host.os.pick({ windows = "F10", macos = "F4" })
  ))
end

local function readElements(rec)
  local p = refreshed(plugin)
  if not p then
    noTarget()
    return
  end
  host.element.dump(p.id, function(nodes, why)
    if why then
      log("W10: the plug-in's elements are not all of its tree: %s", tostring(why))
    end
    listElements(rec, p, nodes)
  end)
end

-- The page's "Try it": the chosen way on the chosen target, the chosen number of times — one run
-- at a time.
local function tryW10(m)
  if w10.run then
    log("W10: 'Try it' while a run is under way — ignored")
    return
  end
  if not refreshed(plugin) then
    noTarget()
    return
  end
  local reps = tonumber(m["repeat"]) or 1
  if reps ~= 1 and reps ~= 10 and reps ~= 20 then
    reps = 1
  end
  w3 = nil
  pressedAt = host.now()
  w10.run = {
    way = tostring(m.way or "watch"),
    target = tostring(m.target or "point"),
    reps = reps,
    n = 0,
    stats = { made = 0, pic = 0, text = 0, front = 0 },
  }
  log("W10: try '%s' on target '%s', %d time(s)", w10.run.way, w10.run.target, reps)
  w10Attempt()
end

-- W9: the live region of page `rec` and the host's speech at the same moment.
local function announce(rec)
  local n = rec.actions.announce
  rec.page:post({ kind = "live", text = string.format("Live region update %d.", n) })
  host.speech.output(string.format("Host speech %d.", n), { interrupt = true })
  log("W9: live region update %d posted and host speech %d said in the same callback", n, n)
end

-- W6 again: the other page in place of this one, beside the plug-in window — shown first, while
-- this application has the front, and this one hidden after, so the front goes from page to page
-- and NVDA never hears the plug-in window between the two (W3's round trip). The host gives the
-- other page's front back to where this one came from, the plug-in window. Not built yet, the
-- other is shown once it is ready, and this one stays until then.
local function switchWindow()
  local p = refreshed(plugin)
  if not p then
    say("No plug-in window remembered.")
    log("the page's button: switch, but no plug-in window is remembered")
    return
  end
  local t0 = host.now()
  pressedAt = t0
  w3 = nil
  landing = nil
  -- From either tool page to the application window, whose label the button has there.
  local to = cur == app and tool or app
  log("the page's button: the %s in place of the other", to.words)
  showBeside(to, p, "switch to the " .. to.words, t0, "the page's button", cur)
end

-- A key as the page named it, in words: the space bar is " " there.
local function keyName(key)
  if key == " " then
    return "Space"
  end
  return tostring(key)
end

-- What page `rec` sent.
local function onMessage(rec, m)
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
    local w = MOD_WORDS
    local mods = (m.ctrl and w.ctrl or "") .. (m.alt and w.alt or "") .. (m.shift and w.shift or "") .. (m.meta and w.meta or "")
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
    local actions, gaps = rec.actions, rec.gaps
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
      announce(rec)
    elseif m.action == "switch" then
      switchWindow()
    end
  elseif kind == "w10" then
    -- `do` and `repeat` are keywords, so the page's fields are read by name. A way this platform's
    -- page does not offer still runs, and answers what the host answers for it here.
    if m["do"] == "read" then
      readElements(rec)
    elseif m["do"] == "try" then
      tryW10(m)
    else
      log("page: an unknown W10 request '%s'", tostring(m["do"]))
    end
  elseif kind == "loaded" then
    -- Which page it is, for the label of its switch button; and W10's part, hidden until this
    -- says it is offered, with this platform's ways and the name of its target key.
    rec.page:post({
      kind = "window",
      window = rec.kind,
      w10 = true,
      ways = W10_WAYS,
      targetKey = host.keys.describe(KEY_TARGET),
    })
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

-- Page `rec` is ready: shown, if a show waited for it; said otherwise, for the tool page, so F7
-- need not be guessed at.
local function onReady(rec)
  rec.ready = true
  log("the %s is ready, %d ms after the last press", rec.words, host.now() - pressedAt)
  if pending and pending.rec == rec then
    local p = pending
    pending = nil
    showBeside(rec, refreshed(p.plugin) or p.plugin, p.via .. " (after the build)", p.t0, p.after, p.replaces)
  elseif rec == tool then
    result("the page is ready: prepared, not shown")
    mention("HTML probe ready.")
  else
    result("the %s is ready: prepared, not shown", rec.words)
  end
end

-- onClose(how, err): "user", or "failed" with the error value that says what failed.
local function onClose(rec, how, err)
  if how == "failed" then
    rec.ready = false
  end
  local words = err and (how .. " — " .. err) or how
  if rec == tool then
    result("the page was closed: %s", words)
  else
    result("the %s was closed: %s", rec.words, words)
  end
end

for _, rec in ipairs(pages) do
  rec.page = host.gui.webview({
    file = "ui/index.html",
    title = rec.title,
    -- The tool page by default, as any module's page is that does not say.
    window = rec == app and "app" or nil,
    -- Said only for the page without activation; every other page is described as before.
    activates = rec.activates,
    onMessage = function(m)
      onMessage(rec, m)
    end,
    onReady = function()
      onReady(rec)
    end,
    onClose = function(how, err)
      onClose(rec, how, err)
    end,
  })
end

-- ── The keys ──────────────────────────────────────────────────────────────────

-- The plug-in window a page goes beside: the one in front, or the one remembered — remembered as
-- it is now. nil, and said, when there is none. `via` names the key, for the log.
local function pluginFor(via)
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
    return nil
  end
  log(
    "%s pressed; in front: %s; the plug-in window '%s' at %d,%d %dx%d",
    via, frontWords(), plugin.title or "", plugin.bounds.x, plugin.bounds.y, plugin.bounds.w, plugin.bounds.h
  )
  return plugin
end

-- The two show keys: the page beside the plug-in window in front, or — pressed in the page —
-- back to the plug-in. `key` names the key, `how` the way its press arrives outside the page; in
-- the page both arrive through RegisterHotKey on Windows, the keyboard hook being out of the chain
-- there. `which` is the page the key shows — F7's is the page F7 shows (`keyed`) — and the other
-- pages are left as they are. A press ends a W3 round trip still being watched, and gives up a
-- W10 run under way.
local function onShowKey(key, how, which)
  local t0 = host.now()
  pressedAt = t0
  w3 = nil
  if w10.run then
    log("W10: the run of '%s' given up by %s after %d of %d attempt(s)", w10.run.way, key, w10.run.n, w10.run.reps)
    -- Every other step still on its way lets go of the baseline as it comes; a round trip waiting
    -- for the plug-in window has none on its way.
    if w10.run.forward and w10.run.base then
      w10.run.base:release()
    end
    w10.run = nil
  end
  local f = host.window.foreground()
  if f and cur.page.window and f.id == cur.page.window and cur.page:isShown() then
    backToPlugin(key .. ", pressed in the page")
    return
  end
  local via = string.format("%s (%s)", key, how)
  landing = nil
  if not pluginFor(via) then
    return
  end
  showBeside(which or keyed, plugin, via, t0, "the key's callback")
end

host.hotkey.register(KEY_SHOW, function()
  onShowKey("F7", HOW.F7)
end)
if noActivation then
  host.hotkey.register(KEY_SHOW_FG, function()
    onShowKey("F8", HOW.F8, noActivation)
  end)
else
  host.hotkey.register(KEY_SHOW_FG, function()
    onShowKey("F8", HOW.F8)
  end, { foreground = true })
end

-- MW2 and MW16, a Mac's: the captured key. From the plug-in window it shows the page F7 shows
-- beside it without the keyboard — raised above the ordinary windows, the plug-in window keeping
-- the keyboard, where a screen reader that says nothing is the answer (MW2) — and pressed again
-- while the page is shown so, it updates the page's live region, which the screen reader reads or
-- does not while the page is not the key window (MW16): nothing is said with it, so what is heard
-- is the page's. No capture applies while a window of this application has the keyboard, so
-- pressed in a page it goes to the page as a key (its W4 line) and this never runs; if it does,
-- that is a result, and nothing else happens.
local liveWhileOut = 0
local function onUnfocusedKey()
  local t0 = host.now()
  pressedAt = t0
  local f = host.window.foreground()
  if f and f.own then
    result("MW16: the captured key FIRED while a window of this application had the keyboard; in front: %s", frontWords())
    say("The captured key fired in a window of this application.")
    return
  end
  w3 = nil
  landing = nil
  local rec = keyed
  local via = "F12 (captured, without the keyboard)"
  if rec.page:isShown() then
    liveWhileOut += 1
    rec.page:post({ kind = "live", text = string.format("Live region update %d, while the page does not have the keyboard.", liveWhileOut) })
    result("MW16: live region update %d posted to the %s, which does not have the keyboard; in front: %s", liveWhileOut, rec.words, frontWords())
    return
  end
  if not pluginFor(via) then
    return
  end
  local ok, err = rec.page:show({ focus = false, avoid = plugin.bounds })
  local ms = host.now() - t0
  if ok then
    result("%s: the %s shown, %d ms after the key's callback; in front: %s", via, rec.words, ms, frontWords())
  elseif err.code == host.errors.NOT_READY then
    log("%s: the %s is being built; it is not shown by itself when it is ready", via, rec.words)
    say("Building the page. Press the key again once it is ready.")
  else
    result("%s: the %s not shown, %s, %d ms after the key's callback; in front: %s", via, rec.words, tostring(err), ms, frontWords())
    say("Page not shown: " .. tostring(err))
  end
end

if KEY_UNFOCUSED then
  -- A capture answers nil and an error value on a Mac while the event tap cannot be made (no
  -- Accessibility grant), and captures nothing: said, and the rest of the probe goes on.
  local token, err = host.keys.capture(KEY_UNFOCUSED, onUnfocusedKey)
  if not token then
    log("%s could not be captured: %s", host.keys.describe(KEY_UNFOCUSED, { style = "short" }), tostring(err))
  end
end

host.hotkey.register(KEY_REPORT, function()
  local parts = {}
  if not reported then
    reported = true
    parts[1] = string.format(
      "HTML probe revision %d. The page's button Show as an application window puts it in a window of its own.",
      REVISION
    )
    if noActivation then
      parts[1] ..= string.format(
        " %s shows it without activation, and %s shows it without the keyboard.",
        host.keys.describe(KEY_SHOW_FG), host.keys.describe(KEY_UNFOCUSED)
      )
    end
  end
  if not cur.ready then
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
  if w10.run then
    parts[#parts + 1] = string.format(
      "A W10 run is under way, %d of %d done%s; F7 gives it up.",
      w10.run.n, w10.run.reps,
      w10.run.forward and ", waiting for the plug-in window to come to the front" or ""
    )
  end
  local b = since.buttons
  parts[#parts + 1] = string.format(
    "Since the last report: buttons: click into the plug-in %d, back %d, window switch %d, announce %d. Key presses the page saw: %d%s.",
    b["click-plugin"], b.back, b.switch, b.announce,
    since.keyCount,
    #since.keys > 0 and (": " .. table.concat(since.keys, ", ")) or ""
  )
  say(table.concat(parts, " "))
  log("the report was said; counting anew")
  fresh()
  landing = nil
end)

host.hotkey.register(KEY_TARGET, function()
  rememberTarget()
end)

-- ── The windows: W2 c, W3, W7 ─────────────────────────────────────────────────

-- W10's round trip: the plug-in window it asked for is in front — the click now, and then the
-- change wait (w10Act). `id` is the window in front; `how` says how that was told, for the log. A
-- foreground change tells it, and on a Mac, where the DAW is the active application already, a
-- focus event alone (as for W3, onFocus below). Whether it clicked.
local function w10Arrived(id, how)
  local fw = w10.run and w10.run.forward
  if not (fw and id == fw.id) then
    return false
  end
  w10.run.forward = nil
  host.input.click(fw.x, fw.y)
  log("W10: the plug-in window %s %d ms after it was asked; clicked at %d,%d", how, host.now() - fw.t0, fw.x, fw.y)
  fw.done(true, nil)
  return true
end

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
  -- A W10 run watches whether another window came forward while it acted: any window that is not
  -- one of our pages (the plug-in a round trip brings forward among them).
  if w10.run then
    local ours = false
    for _, rec in ipairs(pages) do
      if rec.page and rec.page.window and w.id == rec.page.window then
        ours = true
      end
    end
    if not ours then
      w10.run.otherFront = true
    end
    if w10Arrived(w.id, "came to the front") then
      return
    end
  end
  if w3 and w3.stage == "forward" and w.id == w3.plugin.id then
    w3Arrived(w)
    return
  end
  checkLanding()
  if not isPluginWindow(w) then
    return
  end
  -- The first plug-in window of the session, or the first since the tool page failed: it is built
  -- now, hidden, so that WebView2's cold start is over before the first key. Not the application
  -- window, which only W6 again needs: its build would hold the event loop in every run (W8).
  if not tool.ready then
    local ok, err = tool.page:prepare()
    if not ok and err.code ~= host.errors.NOT_READY then
      log("the page could not be prepared: %s", tostring(err))
    end
  end
  if not host.settings.get(AUTO) then
    return
  end
  if previous == w.id then
    return
  end
  for _, rec in ipairs(pages) do
    if rec.page.window ~= nil and previous == rec.page.window then
      return
    end
  end
  -- The application window is reached as any program's window is: by itself it would come at
  -- every return to the plug-in window from another desktop or program, take the keyboard from
  -- the plug-in, and leave the place and size the user gave it for the one beside the plug-in.
  -- The page F7 shows, so never the page without activation, which only F8 shows on a Mac.
  if keyed == app then
    log(
      "automatic show: none — the application window is the %s, and it comes only when asked for",
      keyed == cur and "page in use" or "page F7 shows"
    )
    return
  end
  plugin = w
  pressedAt = host.now()
  showBeside(keyed, w, "automatic show", pressedAt, "the plug-in window came forward")
end)

-- The keyboard moving inside a window: W7's landing, when the window's foreground event came
-- before its control had the keyboard. The first focus event after an answer ends the watch.
-- And a round trip's plug-in window, W3's or W10's, come to the front with no foreground event to
-- say so: on a Mac only an application's activation is one, so a window of the application that is
-- the active one already — REAPER's, the page described `activates = false` having had the keyboard
-- only lent — is told by a focus event alone, as is the plug-in window focused after its
-- application's activation named another of its windows first. The trip goes on once the plug-in
-- window has the keyboard.
host.window.onFocus(function()
  if w10.run and w10.run.forward then
    local f = host.window.foreground()
    if f and w10Arrived(f.id, "has the keyboard, told by a focus event,") then
      return
    end
  end
  if w3 and w3.stage == "forward" then
    local f = host.window.foreground()
    if f and f.id == w3.plugin.id then
      local a = host.window.active()
      log("W3: the plug-in window has the keyboard, told by a focus event, %d ms after the press", host.now() - w3.t0)
      w3Arrived(a and a.id == f.id and a or refreshed(w3.plugin) or w3.plugin)
      return
    end
  end
  checkLanding()
  if landing and landing.last then
    landing = nil
  end
end)

host.settings.onChange(AUTO, function(v)
  log("automatic show is now %s", v and "on" or "off")
end)

-- What the keys and the switch do here, in this platform's words.
local START = host.os.pick({
  windows = { f8 = "it (WM_HOTKEY)", lists = "in Alt+Tab and on the taskbar (W6 again)", captured = "" },
  macos = {
    f8 = "it without activation (activates = false, MW1b)",
    lists = "in Command-Tab and the Dock (MW10, MW11)",
    captured = "; %s, captured, shows it without the keyboard (MW2) and then updates its live region (MW16)",
  },
})
log(
  "revision %d ready — %s shows the page (%s), %s shows %s, %s says what happened%s; "
    .. "the page's button 'Show as an application window' shows it in a window of its own, %s, "
    .. "and 'Show as a tool window' there goes back; automatic show is %s%s",
  REVISION,
  host.keys.describe(KEY_SHOW, { style = "short" }),
  HOW.F7,
  host.keys.describe(KEY_SHOW_FG, { style = "short" }),
  START.f8,
  host.keys.describe(KEY_REPORT, { style = "short" }),
  string.format(", %s remembers the pointer's point in the plug-in for W10", host.keys.describe(KEY_TARGET, { style = "short" })),
  START.lists,
  host.settings.get(AUTO) and "on" or "off",
  KEY_UNFOCUSED and string.format(START.captured, host.keys.describe(KEY_UNFOCUSED, { style = "short" })) or ""
)
