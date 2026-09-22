-- The game-controller half of the probe.
--
-- Nobody who writes this code has a controller at the desk, so everything the gamepad backend
-- promises about timing and background delivery is, until a tester runs this, a promise. This
-- file turns a session with a pad into numbers in the log the tester sends anyway:
--
--   * the ready line says how many pads were found and how each source came up;
--   * every event is logged with its `age` — how late our callback ran after the press was
--     detected — so "is it fast enough with the game in front" is read off the log instead of
--     asked of somebody who cannot see a stopwatch;
--   * a second key reads out what every pad is doing, so a blind tester can check that a press
--     was SEEN without the probe talking over the game on every press.
--
-- Read-only like the rest of the probe: it listens, it never sends anything to a pad or a game.
-- `maxAge` is set far out on purpose. A normal module wants a late press dropped; the probe
-- wants it LOGGED, because a late press is exactly the measurement.
--
-- The button and stick listeners exist only while a pad is connected. The probe is loaded in
-- EVERY test session, and a button or stick listener is what makes the host poll a pad every
-- 4 ms on Windows and hold a latency-critical activity against App Nap on macOS; a session
-- about Kontakt must be measured under the same conditions the shipped application runs in.
-- Only the `connected`/`disconnected` pair stays for the whole run, and costs a look at the
-- empty XInput slots every two seconds.

local M = {}

-- Ctrl+Shift+F10 on Windows, Command+Shift+F10 on a Mac (a spec's Ctrl is Command there), off
-- Control+Option for the reason the main key is: that pair belongs to VoiceOver.
M.KEY = "Ctrl+Shift+F10"

local LATE = 60000 -- ms; see the note above

local seen = { presses = 0, last = nil, pads = {} }

local function log(fmt, ...)
  host.log.info("[probe] " .. string.format(fmt, ...))
end

local function say(text)
  host.speech.output(text, { interrupt = true })
end

local function onButton(e)
  if e.type == "down" then
    seen.presses = seen.presses + 1
    seen.last = { label = e.label, button = e.button, pad = e.pad, time = e.time, age = e.age }
  end
  log(
    "gamepad pad %d %s %s (%s), age %d ms, time %d%s",
    e.pad, e.type, e.button, e.label, math.floor(e.age), math.floor(e.time),
    e.synthetic and ", synthetic (the pad went away while it was held)" or ""
  )
end

-- The status table in one line, keys sorted so two logs can be compared line by line.
local function statusLine()
  local s = host.gamepad.status()
  local keys = {}
  for k in pairs(s) do
    keys[#keys + 1] = k
  end
  table.sort(keys)
  local parts = {}
  for _, k in ipairs(keys) do
    parts[#parts + 1] = k .. " " .. tostring(s[k])
  end
  return table.concat(parts, "; ")
end

-- What goes into the ready line: the count, and the one or two sources that decide whether a
-- press can be seen at all on this platform.
local function summary()
  local pads = host.gamepad.list()
  local s = host.gamepad.status()
  local parts = {}
  for _, k in ipairs({ "xinput", "arrival", "gamecontroller", "background" }) do
    if s[k] then
      parts[#parts + 1] = k .. " " .. s[k]
    end
  end
  return string.format("gamepad: %d pad(s) (%s)", #pads, table.concat(parts, ", "))
end

local function readOut()
  local pads = host.gamepad.list()
  log("gamepad read-out: %s", statusLine())
  if #pads == 0 then
    say("No game controller is connected.")
    log("gamepad read-out: no pad connected")
    return
  end
  local spoken = {}
  for _, pad in ipairs(pads) do
    local s = host.gamepad.state(pad.index)
    local held = {}
    if s then
      for _, name in ipairs(pad.buttons) do
        if s.buttons[name] then
          held[#held + 1] = name
        end
      end
      log(
        "gamepad read-out: pad %d %s (%s, %s, %s): holding [%s]; left %.2f, %.2f; right %.2f, %.2f; triggers %.2f, %.2f",
        pad.index, pad.name, pad.id, pad.family, pad.source, table.concat(held, " "),
        s.axes.left_x or 0, s.axes.left_y or 0, s.axes.right_x or 0, s.axes.right_y or 0,
        s.axes.left_trigger or 0, s.axes.right_trigger or 0
      )
    end
    -- Positional names, spoken with spaces: "dpad down" rather than a screen reader spelling
    -- out an underscore.
    local words = {}
    for i, name in ipairs(held) do
      words[i] = (string.gsub(name, "_", " "))
    end
    spoken[#spoken + 1] = string.format(
      "Pad %d, %s: %s.", pad.index, pad.name,
      #words == 0 and "nothing held" or ("holding " .. table.concat(words, ", "))
    )
  end
  if seen.last then
    local ago = (host.now() - seen.last.time) / 1000
    spoken[#spoken + 1] = string.format(
      "%d presses seen. The last was %s on pad %d, %.0f seconds ago, reported %d milliseconds after it was detected.",
      seen.presses, seen.last.label, seen.last.pad, ago, math.floor(seen.last.age)
    )
  else
    spoken[#spoken + 1] = "No press seen yet."
  end
  say(table.concat(spoken, " "))
end

-- The tokens of the button and stick listeners while any pad is connected, and the pads that
-- are. See the note at the top of this file.
local values = nil
local present = {}

local function onAxis(e)
  log("gamepad pad %d axis %s %.2f (raw %.2f), age %d ms", e.pad, e.axis, e.value, e.raw, math.floor(e.age))
end

local function listenToValues()
  if values then
    return
  end
  values = {
    host.gamepad.on("down", onButton, { maxAge = LATE }),
    host.gamepad.on("up", onButton, { maxAge = LATE }),
    -- A coarse step: every movement of a stick in 0.05 steps is a hundred lines a second and
    -- says nothing a quarter step does not. No `maxAge`: an axis value is never dropped.
    host.gamepad.on("axis", onAxis, { step = 0.25 }),
  }
  log("gamepad: listening to buttons and sticks while a pad is connected")
end

local function stopListeningToValues()
  if not values then
    return
  end
  for _, token in ipairs(values) do
    host.gamepad.off(token)
  end
  values = nil
  log("gamepad: no pad connected; button and stick listeners removed")
end

-- Starts listening and calls `ready(summary)` once the sources have had a moment to act on it:
-- registering a listener wakes the Windows pad thread, which then starts probing (and polling,
-- once a pad is there and the button listeners follow), and a ready line written in the same
-- breath would report the state from before. 300 ms is far more than that takes and far less
-- than anybody waits for the line.
function M.start(ready)
  local ok, err = pcall(function()
    host.gamepad.on("connected", function(e)
      log(
        "gamepad pad %d connected: %s (%s, family %s, source %s)%s, age %d ms",
        e.pad, e.info.name, e.info.id, e.info.family, e.info.source,
        e.synthetic and ", already there when the probe started" or "", math.floor(e.age)
      )
      if not e.synthetic then
        say(string.format("%s connected as pad %d.", e.info.name, e.pad))
      end
      present[e.pad] = true
      listenToValues()
    end)
    host.gamepad.on("disconnected", function(e)
      log("gamepad pad %d disconnected: %s, age %d ms", e.pad, e.info.name, math.floor(e.age))
      say(string.format("Pad %d disconnected.", e.pad))
      present[e.pad] = nil
      if next(present) == nil then
        stopListeningToValues()
      end
    end)
  end)
  if not ok then
    -- The Rust side has already written "gamepad watcher failed" with the reason; this line
    -- ties it to the probe, and the ready line below still comes, so the rest of the probe is
    -- not lost with the controllers.
    log("gamepad watcher failed: %s", tostring(err))
    ready("gamepad: unavailable (" .. tostring(err) .. ")")
    return
  end
  host.hotkey.register(M.KEY, function()
    local fine, why = pcall(readOut)
    if not fine then
      log("gamepad read-out FAILED: %s", tostring(why))
      say("The game controller read-out failed; the log has the reason.")
    end
  end)
  host.timer.after(300, function()
    local fine, text = pcall(summary)
    ready(fine and text or ("gamepad: status unavailable (" .. tostring(text) .. ")"))
    log("gamepad status: %s", statusLine())
  end)
end

return M
