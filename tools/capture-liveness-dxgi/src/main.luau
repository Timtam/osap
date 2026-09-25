-- The capture liveness measurement through DXGI Desktop Duplication (see module.toml): the same
-- forty reads and the same verdict as tools/capture-liveness, whose comments below still apply,
-- with other keys so both can be loaded at once and compared on the same game — Ctrl+Shift+F7
-- through the keyboard hook, Ctrl+Shift+F8 as a registered hotkey. The first duplicated read
-- opens the duplication, which can take seconds, so the key wakes it right away and the ten
-- seconds of the countdown cover the opening.
--
-- Does this application see a LIVE picture of the window in front, or a frozen one?
--
-- Some games present their frames in a way the ordinary Windows screen read (GDI) does not
-- follow: the read keeps returning the same old picture, or black, while the game plainly moves.
-- Everything built on top — image search, OCR, pixel colours — then answers about a picture
-- that is no longer on the screen, and nothing says so. This tool asks the question directly.
--
-- Press the key, bring the game to the front within ten seconds while something in it MOVES (a
-- title screen animating, a demo running), and keep your hands off until it speaks again — twenty
-- seconds or more, since a read of a large window costs time of its own. The ten seconds are
-- there so the key can also be pressed OUTSIDE the game: some games keep every key to themselves.
-- It names the window it watches, so a measurement of the wrong window is heard at once. It reads the window forty times, a quarter of a
-- second apart, and counts how many different pictures came back:
--   * many different pictures  -> the capture is live;
--   * one picture, forty times -> frozen, IF the game was moving (a still menu also gives one);
--   * black                    -> the capture sees nothing of the game at all.
-- The log gets one line per read (its fingerprint and how long the read took) and the verdict.
--
-- Each read is one host.screen.profile over the window's content: one screen capture, reduced to
-- per-column statistics. The fingerprint is built from those statistics, so any visible change
-- in the window changes it, and the cost of a read is the cost of one capture.
--
-- Two keys start it, and they reach this application by two different ways, so that pressing
-- each once INSIDE the game also answers a second question: which way does the game leave open?
--   * the first goes through the application's own keyboard hook (host.keys.capture);
--   * the second is a registered hotkey (host.hotkey.register), the way ordinary global keys work.
-- A game or emulator can switch registered hotkeys off while it is in front (WinUAE registers its
-- keyboard as raw input with RIDEV_NOHOTKEYS), or sit in front of our hook with one of its own.
-- The log names the way and the window in front for every key that arrives. Where the hook is
-- refused the first key falls back to a registered hotkey.
-- The modifiers still reach the game; only the F7 / F8 is taken.

-- Desktop duplication exists on Windows only, so this module does too (module.toml).
local KEY = "Ctrl+Shift+F7"
local KEY_REGISTERED = "Ctrl+Shift+F8"
local COUNTDOWN_MS = 10000

local READS = 40
local EVERY_MS = 250

local function say(text)
  host.speech.output("Duplication. " .. text, { interrupt = true })
end

local function line(fmt, ...)
  host.log.info(string.format("[liveness-dxgi] " .. fmt, ...))
end

-- A number that changes when the picture changes. Column means are fractional; rounding them to
-- a tenth keeps capture noise out while any real change moves at least one column.
local function fingerprint(cols)
  local h, bright = 0, 0
  for i, m in ipairs(cols.mean) do
    h = (h * 31 + math.floor(m * 10 + 0.5) + i) % 2147483647
    bright = bright + cols.max[i]
  end
  return h, bright == 0
end

local running, counting = false, false

local function describe(win)
  if not win then return "nothing" end
  return string.format("'%s' (%s)", tostring(win.title), tostring(win.app and win.app.exe or "?"))
end

local function watch(via)
  local win = host.window.active()
  if not (win and win.client and win.client.w > 0 and win.client.h > 0) then
    say("Nothing is in front to watch.")
    return
  end
  running = true
  local c = win.client
  local region = { c.x, c.y, c.x + c.w, c.y + c.h }
  line("watching '%s' (%s), content %d,%d %dx%d, %d reads every %d ms, started by the %s",
    tostring(win.title), tostring(win.app and win.app.exe or "?"), c.x, c.y, c.w, c.h,
    READS, EVERY_MS, via)
  say("Watching " .. tostring(win.title) .. ". Hands off until I speak again.")

  local seen, distinct, black, failed, n = {}, 0, 0, 0, 0
  local slowest, total = 0, 0
  local function read()
    n = n + 1
    local t0 = host.now()
    local p, why = host.screen.profile({ region = region, axes = "columns" })
    local ms = host.now() - t0
    total = total + ms
    if ms > slowest then slowest = ms end
    if not p then
      failed = failed + 1
      line("read %d: FAILED after %d ms (%s)", n, ms, tostring(why))
    else
      local fp, isBlack = fingerprint(p.columns)
      if isBlack then black = black + 1 end
      if not seen[fp] then
        seen[fp] = true
        distinct = distinct + 1
      end
      line("read %d: fingerprint %d%s, %d ms", n, fp, isBlack and " (black)" or "", ms)
    end
    if n < READS then
      host.timer.after(EVERY_MS, read)
      return
    end
    running = false
    local verdict
    if failed == READS then
      verdict = "The capture failed every time."
    elseif black == READS - failed then
      verdict = "Black: the capture sees nothing of this window."
    elseif distinct <= 1 then
      verdict = "Frozen: the same picture every time. If the game was moving, the capture is stuck."
    else
      verdict = string.format("Live: %d different pictures in %d reads.", distinct, READS - failed)
    end
    line("%s (%d distinct, %d black, %d failed; reads took %d ms on average, the slowest %d ms)",
      verdict, distinct, black, failed, math.floor(total / READS), slowest)
    say(verdict)
  end
  read()
end

-- Every key that arrives is logged with its way and the window in front at that moment; that line
-- is the answer to "does this game let our keys through". Then the countdown, then the reads.
local function start(key, via)
  line("%s arrived through the %s while %s was in front", host.keys.describe(key, { style = "short" }),
    via, describe(host.window.active()))
  if running then
    say("Still watching.")
    return
  end
  if counting then
    say("Already counting.")
    return
  end
  counting = true
  -- Wakes the duplication now, so it has the countdown to open; the answer does not matter.
  host.screen.pixel(0, 0)
  say("Ten seconds. Bring the game to the front, then hands off.")
  host.timer.after(COUNTDOWN_MS, function()
    counting = false
    watch(via)
  end)
end

local how = "keyboard hook"
local ok, err = pcall(host.keys.capture, KEY, function() start(KEY, "keyboard hook") end)
if not ok then
  line("the keyboard hook is not available (%s); using a registered hotkey instead", tostring(err))
  how = "registered hotkey"
  host.hotkey.register(KEY, function() start(KEY, "registered hotkey") end)
end
local okReg, errReg = pcall(host.hotkey.register, KEY_REGISTERED,
  function() start(KEY_REGISTERED, "registered hotkey") end)
if not okReg then
  line("%s could not be registered (%s)", host.keys.describe(KEY_REGISTERED, { style = "short" }),
    tostring(errReg))
end

host.log.info(string.format(
  "[liveness-dxgi] ready — press %s (%s) or %s (registered hotkey), then bring a moving game to the front",
  host.keys.describe(KEY, { style = "short" }), how,
  host.keys.describe(KEY_REGISTERED, { style = "short" })))
