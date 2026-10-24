-- A module to try out the quick repeats of a key (`press.count`), the keypad's keys in the key
-- spec, the keys this application sends, and the report of the window already in front
-- (`onTrigger { initial = true }`): the live checks in TODO.md press its keys and listen. Every key
-- below says the count of its press, so one, two and three quick presses say 1, then 1 2, then
-- 1 2 3, and slow ones 1 each.
--
-- Its keys, Windows / Mac. On a Mac an F-key needs "Use F1, F2, etc. as standard function keys"
-- turned on, or fn held down. Command+Shift with F4 to F12 are the window probe's, the other
-- tools' and the host's own, so its Mac keys past F3 are Option+Command, without F5 (the
-- Accessibility Shortcuts). The log lists the hotkeys at load and each captured key as it is
-- captured, in this platform's words.
--
--   Ctrl+Alt+J / Cmd+Shift+F1     a hotkey: its count
--   Ctrl+Alt+K / Cmd+Shift+F2     a hotkey with taps = "final": "final" and the count, once, when
--                                 the presses stop or another key is pressed
--   Ctrl+Alt+L / Cmd+Shift+F3     a hotkey that sends F13 at every press, while its own keys are
--                                 still down: its count, which the key sent does not end
--   Ctrl+Alt+P / Cmd+Option+F1    the captures, in turn: on; Return without NumpadEnter, and a
--                                 hotkey on NumpadEnter; that hotkey alone; off
--   Ctrl+Alt+W / Cmd+Option+F2    reads the screen for two seconds, its handler waiting: presses
--                                 made meanwhile wait behind it, and keep their counts
--   Ctrl+Alt+NumpadAdd / Cmd+Shift+NumpadAdd             its count, and once the modifiers are
--                                                        up it sends Numpad5 (Numpad1 on a Mac)
--   Ctrl+Alt+NumpadSubtract / Cmd+Shift+NumpadSubtract   the same, sending NumpadEnter
--   Ctrl+Alt+NumpadMultiply / Cmd+Shift+NumpadMultiply   the same, posting NumpadDivide to the
--                                                        control that has the keyboard
--   Ctrl+Alt+D, R, V / Cmd+Option+F6, F7, F8   the same three, for a keyboard without a keypad
--
-- Captured while the captures are on, in every window: Ctrl+Alt+Y / Cmd+Option+F3 and Numpad1,
-- their count, said again at every repeat of a key held down; Return and NumpadEnter, their name
-- and count. The keypad's Enter is NumpadEnter's while that is captured, and Return's otherwise.
-- The hotkey on NumpadEnter says "hotkey", the name and the count; Windows refuses it, and that
-- is said.
-- Send the keypad's keys with the captures off: while they are on they take Numpad1 and
-- NumpadEnter sent, as they take typed ones.
--
-- Every window that comes to the front is said and logged, and so is the one in front at load, or
-- when the module is turned on (initial = true): a report heard twice would show.

local function key(windows, macos)
  return host.os.pick { windows = windows, macos = macos, linux = windows }
end

local COUNT = key("Ctrl+Alt+J", "Cmd+Shift+F1")
local FINAL = key("Ctrl+Alt+K", "Cmd+Shift+F2")
local SENDS = key("Ctrl+Alt+L", "Cmd+Shift+F3")
local CAPTURES = key("Ctrl+Alt+P", "Cmd+Option+F1")
local WAITS = key("Ctrl+Alt+W", "Cmd+Option+F2")
local CAPTURED = key("Ctrl+Alt+Y", "Cmd+Option+F3")
local SEND_DIGIT = { key("Ctrl+Alt+NumpadAdd", "Cmd+Shift+NumpadAdd"), key("Ctrl+Alt+D", "Cmd+Option+F6") }
local SEND_ENTER = { key("Ctrl+Alt+NumpadSubtract", "Cmd+Shift+NumpadSubtract"), key("Ctrl+Alt+R", "Cmd+Option+F7") }
local POST_DIVIDE = { key("Ctrl+Alt+NumpadMultiply", "Cmd+Shift+NumpadMultiply"), key("Ctrl+Alt+V", "Cmd+Option+F8") }
local NUMPAD1, NUMPAD_ENTER, RETURN = "Numpad1", "NumpadEnter", "Return"
-- The digit each platform's check sends: Numpad5 on Windows, which with Num Lock off is Clear and
-- still has to type 5; Numpad1 on a Mac, whose check names that key.
local DIGIT = key("Numpad5", NUMPAD1)

-- A count that goes on waits behind the one before it, so three quick presses are heard as 1, 2,
-- 3. A 1 starts afresh, and a held key's repeat, which carries its press's count again, cuts off
-- the one before it rather than pile up behind it.
local lastCount = 0
local function sayCount(n, words)
  host.speech.output(words and (words .. " " .. n) or tostring(n), { interrupt = n == 1 or n == lastCount })
  lastCount = n
end

local function announce(text)
  host.speech.output(text)
end

-- A hotkey, and its line in the log: the key in this platform's words, what it does, and what the
-- host answered when it does not hold the key now.
local function hotkey(spec, does, cb, opts)
  local id, err = host.hotkey.register(spec, cb, opts)
  local line = host.keys.describe(spec) .. ": " .. does
  if err then
    line ..= (id and " — claimed, not held now: " or " — not registered: ") .. tostring(err)
  end
  host.log.info(line)
  return id, err
end

hotkey(COUNT, "says its count", function(press)
  sayCount(press.count)
end)

hotkey(FINAL, "says final and its count once the presses stop", function(press)
  sayCount(press.count, "final")
end, { taps = "final" })

-- F13, a key no program here acts on, sent while the hotkey's own keys are down: on Windows it
-- goes with them, as Control+Alt+F13; a Mac sends it bare.
local SENT = "F13"
hotkey(SENDS, "sends " .. SENT .. " at every press and says its count", function(press)
  local ok, err = host.input.send(SENT)
  sayCount(press.count)
  if not ok then
    host.speech.output("not sent: " .. tostring(err), { interrupt = false })
  end
end)

-- Reads until two seconds have passed, each read waiting in this handler (no callback), so the
-- module is busy meanwhile. A read that fails ends it: one that came back at once would spin.
local WAIT_MS = 2000
local CORNER = { 0, 0, 200, 50 }
hotkey(WAITS, "reads the screen for two seconds", function()
  announce("Reading for two seconds.")
  local from, reads = host.now(), 0
  repeat
    local r = host.ocr.recognize(CORNER)
    reads += 1
    if r.status == "failed" then
      host.log.info("a read failed: " .. tostring(r.error))
      break
    end
  until host.now() - from >= WAIT_MS
  host.log.info(string.format("waited %d ms for %d read(s)", host.now() - from, reads))
end)

-- The captures, in turn: 1 all of them; 2 Return without NumpadEnter, so that Return takes both
-- Enters, and a hotkey on NumpadEnter, which the capture of Return takes from it; 3 that hotkey
-- alone, where the platform has it; 0 none.
local step = 0
local enterId -- the hotkey on NumpadEnter, while one is registered

-- A capture, and its line in the log, as a hotkey has.
local function capture(spec, words)
  local token, err = host.keys.capture(spec, function(_, press)
    sayCount(press.count, words)
  end)
  local name = host.keys.describe(spec)
  if token then
    host.log.info(name .. ": captured, says " .. (words and "its name and " or "") .. "its count")
  else
    host.log.info(name .. ": not captured: " .. tostring(err))
  end
  return token, err
end

-- Answers false and the error value when the keyboard hook or the event tap could not be put in.
local function captureAll(withNumpadEnter)
  local wanted = { { CAPTURED }, { NUMPAD1 }, { RETURN, host.keys.describe(RETURN) } }
  if withNumpadEnter then
    table.insert(wanted, { NUMPAD_ENTER, host.keys.describe(NUMPAD_ENTER) })
  end
  for _, c in ipairs(wanted) do
    local token, err = capture(c[1], c[2])
    if not token then
      host.keys.releaseAll()
      return false, err
    end
  end
  return true
end

-- Answers whether the hotkey on NumpadEnter is held now; the log says why it is not.
local function enterHotkey()
  local name = host.keys.describe(NUMPAD_ENTER)
  local id, err = hotkey(NUMPAD_ENTER, "says hotkey, its name and its count", function(press)
    sayCount(press.count, "hotkey " .. name)
  end)
  enterId = id
  return id ~= nil and err == nil
end

local STEPS = "the captures, in turn: on, Return without NumpadEnter, the hotkey on NumpadEnter alone, off"
hotkey(CAPTURES, STEPS, function()
  host.keys.releaseAll()
  if enterId then
    host.hotkey.unregister(enterId)
    enterId = nil
  end
  step = (step + 1) % 4
  local enter = host.keys.describe(NUMPAD_ENTER)
  if step == 1 or step == 2 then
    local ok, err = captureAll(step == 1)
    if not ok then
      step = 0
      announce("Keys cannot be captured: " .. tostring(err))
      return
    end
  end
  if step == 1 then
    announce("Captures on: " .. host.keys.describe(CAPTURED) .. ", " .. host.keys.describe(NUMPAD1) .. ", "
      .. enter .. " and " .. host.keys.describe(RETURN) .. ".")
  elseif step == 2 then
    local held = enterHotkey()
    announce(enter .. " not captured: " .. host.keys.describe(RETURN) .. " takes both. "
      .. (held and ("A hotkey on " .. enter .. ".") or (enter .. " is no hotkey here; the log says why.")))
  elseif step == 3 and enterHotkey() then
    announce("Captures off. Only the hotkey on " .. enter .. ".")
  else
    if enterId then
      host.hotkey.unregister(enterId)
      enterId = nil
    end
    step = 0
    announce("Captures off.")
  end
end)

-- A key sent from a hotkey's callback carries the modifiers still held on Windows (Control+Alt with
-- Numpad5 types no 5), and a program that asks the keyboard reads a posted one with them. So these
-- wait until the modifiers are up, asked every 15 ms for up to three seconds, and send nothing
-- after that. A press made while one waits only says its count.
local waiting = false
local function onceReleased(act)
  if waiting then
    return
  end
  waiting = true
  local polls = 0
  local function poll()
    if host.keys.modifiersDown() then
      polls += 1
      if polls < 200 then
        host.timer.after(15, poll)
      else
        waiting = false
        announce("Nothing sent: the modifiers were held down for three seconds.")
      end
      return
    end
    waiting = false
    act()
  end
  poll()
end

local function answered(verb, ok, err)
  if not ok then
    announce("Not " .. verb .. ": " .. tostring(err))
  end
end

-- Each of these on its keypad key and on one off the keypad.
local function trigger(specs, does, act)
  for _, spec in ipairs(specs) do
    hotkey(spec, "says its count, and once the modifiers are up " .. does, function(press)
      sayCount(press.count)
      onceReleased(act)
    end)
  end
end

trigger(SEND_DIGIT, "sends " .. DIGIT, function()
  answered("sent", host.input.send(DIGIT))
end)
trigger(SEND_ENTER, "sends NumpadEnter", function()
  answered("sent", host.input.send(NUMPAD_ENTER))
end)
-- On Windows a post goes to the one window it names, and a program's frame window does not hand a
-- key on to the control in it that has the keyboard: so the post goes to the innermost link of the
-- focus chain, or to the window in front when the chain is empty. A Mac posts to the application
-- of whichever it names.
trigger(POST_DIVIDE, "posts NumpadDivide to the control that has the keyboard", function()
  local to = host.window.focusChain()[1] or host.window.foreground()
  if not to then
    announce("No window in front to post to.")
    return
  end
  host.log.info(string.format("posts NumpadDivide to %d (%s)", to.id, tostring(to.class)))
  answered("posted", host.input.post(to.id, "NumpadDivide"))
end)

-- Every window in front, the one at load too. A second after each report it asks for a picture of
-- the window, through desktop duplication on Windows (module.toml). The host starts opening
-- duplication before the first callback, so by then the log has the opening ("desktop duplication
-- … opened") before "asks for its picture", and the picture waits for a frame and the first-read
-- comparison, not for the opening. Without that head start this read starts the opening and waits
-- for it, and the opening's line comes after. A picture asked in the callback itself could not
-- tell the two apart: it would wait for the rest of an opening begun just before it as for one it
-- began itself.
local PICTURE_AFTER_MS = 1000
local reports = 0
host.window.onTrigger({}, { initial = true }, function(w)
  reports += 1
  local n = reports
  host.speech.output("Trigger: " .. w.title, { interrupt = false })
  host.log.info(string.format("trigger %d: %s (%s)", n, w.title, w.app.name))
  host.timer.after(PICTURE_AFTER_MS, function()
    host.log.info(string.format("trigger %d: asks for its picture", n))
    host.screen.snapshot({ region = { window = w, fraction = { 0, 0, 0.5, 0.5 } } }, function(snap, err, info)
      if snap then
        snap:release()
      end
      host.log.info(string.format("trigger %d: its picture %s after %d ms", n,
        snap and "was taken" or ("failed (" .. tostring(err) .. ")"), info.waited))
    end)
  end)
end)

announce("Quick repeats test ready. " .. host.keys.describe(COUNT) .. " says its count, "
  .. host.keys.describe(CAPTURES) .. " turns the captures on. The log lists every key.")
