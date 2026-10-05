-- Guard probe — the guard's two limits: memory (G1) and time (G2). Its partners are
-- tools/guard-probe-b, tools/guard-probe-c and tools/guard-probe-lib; all four are needed.
--
-- WHAT THIS IS FOR. Every module VM has a memory limit — 256 MiB per module whose code runs in
-- it, more where a manifest asks for it with [limits] memory_mib — and every callback a time
-- limit: 2 seconds of the event loop's processor time, or 10 seconds in all. A callback past
-- either stops its module until the next start: a sentence, the error window, a line in the log,
-- and the module's row in the manager unticked with a note. A loop in a library's own code stops
-- the library and every module that runs its code. None of that can be checked without a person
-- and a screen reader, which is what these keys are for.
--
-- HOW TO RUN IT. The four probe folders and nothing else, and -Build, so that the application is
-- built with the guard — without -Build, run-dev.ps1 starts whatever build is there already, and
-- an older one has no limits at all:
--
--     .\run-dev.ps1 -Build -Tools -Only guard-probe,guard-probe-lib,guard-probe-b,guard-probe-c
--
-- THE SAFE WAY OUT. Every key here ends by itself: a loop after 30 seconds, a memory test after
-- 300 MB (600 for Guard probe B). With the guard you never wait that long. On a build without
-- it you hear "No stop: this build has no time limit" or "No stop: this build has no memory
-- limit" instead of a stop, and the application answers again; quit it from the tray and start
-- it with -Build. If nothing answers for more than a minute, end automation-platform.exe in Task
-- Manager.
--
-- THE SENTENCE AND THE WINDOW come at the same moment. NVDA may read the error window as it
-- takes the focus and cut the sentence short — the window's first line and its last paragraph
-- say the same, keys included. The log's [speech] line has the sentence as it was handed over.
-- A slot's sentence ("… has the slot") a few milliseconds before a stop may be cut the same way:
-- the log's [speech] line has it too.
--
-- Guard probe B says "Guard probe B has the slot, chime off" as it loads. Then:
--
--   TEST 0 — the details, before anything is stopped.
--     In the manager's Installed list, select "Guard probe B" and press Details….
--       Expected: "Memory: its VM may use up to 556 MiB — 300 of its own (set in its
--       module.toml) and 256 for Guard probe library (com.tool.guard-probe-lib), whose code runs
--       in it. In use now: …". On "Guard probe library": "Its code also runs in the VMs of: Guard
--       probe B (com.tool.guard-probe-b), Guard probe C (com.tool.guard-probe-c)." On "Guard
--       probe": "… up to 256 MiB — all of it its own …".
--
--   TEST 1 — a callback that runs out of memory stops its module.
--     a) Press Ctrl+Alt+Win+0.                   Expected: "Guard probe is on".
--     b) Press Ctrl+Alt+Win+2.                   Expected, within a second or two: "Stopped
--        Guard probe: its hotkey callback needed more than the 256 megabytes of memory it may
--        use. Its keys go to the program in front again, and it stays off until you turn it on in
--        the module manager or start the application again." The error window opens with the
--        dialog's text ("What used too much memory: Guard probe's hotkey callback. … Where: not
--        known. …").
--     c) Press Ctrl+Alt+Win+0.                   Expected: nothing — it is off.
--     d) In the manager: the row reads "Guard probe  v0.1.0   (com.tool.guard-probe) —
--        stopped: its hotkey callback needed more than 256 MiB of memory", unticked. Details… on
--        it says "State: off — stopped by the application …".
--     e) Tick it.                                Expected: the note goes. It is built afresh, as a
--        reload builds it.
--     f) Press Ctrl+Alt+Win+0.                   Expected: "Guard probe is on".
--        FAILS AS: silence at b); "ran for 2 seconds" at b) — the time limit came first, the
--        memory limit did not work; "No stop: this build has no memory limit"; or Guard probe
--        still answering at c).
--
--   TEST 2 — caught by the module's own pcall, it is the module's business.
--     Press Ctrl+Alt+Win+8.                      Expected: "Caught it. Guard probe goes on."
--     Then Ctrl+Alt+Win+0.                       Expected: "Guard probe is on". Nothing stopped.
--
--   TEST 3 — out of memory inside another module's callback.
--     a) Press Ctrl+Alt+Win+4.                   Guard probe takes the slot from Guard probe B,
--        whose onDeactivate then runs out of memory.
--        Expected: "Guard probe has the slot", and "Stopped Guard probe B: its arbiter
--        onDeactivate callback, run during Guard probe's hotkey callback, needed more than the
--        556 megabytes of memory it may use. Its keys go to the program in front again; more in
--        the error window." (the two may come in either order).
--     b) Press Ctrl+Alt+Win+0.                   Expected: "Guard probe is on" — only B stopped.
--     c) Press Ctrl+Alt+Win+9.                   Expected: nothing — B is off.
--     d) Press Ctrl+Alt+Win+4 again.             Expected: "Guard probe gave up the slot".
--     e) Tick Guard probe B in the manager.      Expected: "Guard probe B has the slot, chime
--        off" — it is built afresh and wins the slot.
--
--   TEST 4 — a stopped module is built afresh when it is ticked, with its settings as they are.
--     a) Press Ctrl+Alt+Shift+Win+2 (B's own key, while B holds the slot).
--        Expected: "Stopped Guard probe B: its hotkey callback needed more than the 556
--        megabytes of memory it may use. Its keys go to the program in front again, and it stays
--        off until you turn it on in the module manager or start the application again."
--     b) Open Settings… on Guard probe B, tick "Chime", OK.  Expected: nothing.
--     c) Tick Guard probe B again.               Expected: "Guard probe B has the slot, chime
--        on" — built afresh, it reads the setting as it is now.
--        FAILS AS: "Guard probe B heard the chime" (the old VM's waiting callback ran), or
--        "chime off".
--
--   TEST 5 — "until the next start".
--     Stop Guard probe again (TEST 1 b, or TEST 6), leave it unticked, quit from the tray and
--     start again (with the run line above).
--     Expected: Ctrl+Alt+Win+0 says "Guard probe is on" — it is on after a restart.
--
--   TEST 6 — a loop stops its module after 2 seconds of processor time.
--     a) Press Ctrl+Alt+Win+1.                   Expected: about 2 seconds of silence (nothing
--        answers, the keyboard of the program in front works), then "Stopped Guard probe: its
--        hotkey callback ran for 2 seconds of processor time without returning, at src/main.luau
--        line 241. Its keys go to the program in front again; it stays off until you turn it
--        on in the module manager." The error window opens: "What ran too long: Guard probe's
--        hotkey callback. How long: 2.0 s of processor time … In a host call: no, it was running
--        Luau code. Where, innermost first: com.tool.guard-probe/src/main.luau:241: in function
--        'spin' …".
--     b) Press Ctrl+Alt+Win+0.                   Expected: nothing — it is off.
--     c) In the manager the row ends "— stopped: its hotkey callback ran too long, at
--        src/main.luau:241", unticked. Tick it.  Expected: the note goes.
--     d) Press Ctrl+Alt+Win+0.                   Expected: "Guard probe is on".
--        FAILS AS: "No stop: this build has no time limit" after 30 seconds, or the sentence
--        naming another module.
--
--   TEST 7 — work in C between Luau's checks: string.upper of 4 MB, over and over.
--     Press Ctrl+Alt+Win+3, then tick it again.  Expected: as TEST 6, at line 248.
--
--   TEST 8 — a loop over a host call: host.screen.pixel, which waits for the screen.
--     Press Ctrl+Alt+Win+7, then tick it again.  Expected: up to 10 seconds of silence — a capture
--        mostly waits for the display, which is not processor time, so the wall clock decides —
--        then "Stopped Guard probe: its hotkey callback ran for 10 seconds without returning, in
--        host.screen.pixel, at src/main.luau line 255. …". If the capture computes on this
--        machine, it may be "2 seconds of processor time" instead; either is right, the host call
--        is named in both. (Rarely the time runs out between two calls: then "in
--        host.screen.pixel" is missing.)
--
--   TEST 9 — a long callback that ends is not stopped.
--     Press Ctrl+Alt+Shift+Win+1.                Expected: one second of silence, then
--        "finished". Ctrl+Alt+Win+0: "Guard probe is on". Nothing stopped, no error window.
--
--   TEST 10 — a loop inside another module's callback stops that module, not the caller.
--     a) Press Ctrl+Alt+Shift+Win+4.             Guard probe takes the slot from Guard probe B,
--        whose onDeactivate then loops.
--        Expected: about 2 seconds of silence, "Guard probe has the slot, for the time test",
--        and "Stopped Guard probe B: its arbiter onDeactivate callback, run during Guard probe's
--        hotkey callback, ran for 2 seconds of processor time without returning, at
--        src/main.luau line 23. Its keys go to the program in front again; more in the
--        error window." (the two may come in either order).
--     b) Press Ctrl+Alt+Win+0.                   Expected: "Guard probe is on" — only B stopped.
--     c) Press Ctrl+Alt+Shift+Win+4 again.       Expected: "Guard probe gave up the slot".
--     d) Tick Guard probe B in the manager.      Expected: "Guard probe B has the slot, chime
--        …" — built afresh.
--
--   TEST 11 — a mouse button held down is let go of.
--     a) Open Notepad, type a few lines, and route the mouse pointer into its text (NVDA+Numpad
--        Slash routes it to the review cursor).
--     b) Press Ctrl+Alt+Win+5: the left button goes down where the pointer is, then the callback
--        loops.                                  Expected: as TEST 6; the error window has
--        "Released: the left mouse button it was holding down." The log has "[input]
--        [com.tool.guard-probe] released the left mouse button …: it was stopped by the host".
--     c) In Notepad, type and select with Shift+arrows.  Expected: both behave normally — no
--        button stuck down.
--     d) Tick Guard probe again.
--
--   TEST 12 — keys pressed while the application waited are dropped.
--     a) Press Ctrl+Alt+Win+1, and during the silence press Ctrl+Alt+Win+9 (Guard probe B's key)
--        twice.                                  Expected: after the sentence, Guard probe B says
--        nothing for those two presses. The error window has "Dropped: 2 hotkey presses made
--        while the application waited: Ctrl+Alt+Win+9 ×2." (or the log a "[keys] … reached the
--        application after it, and were dropped" line for them).
--     b) Press Ctrl+Alt+Win+9.                   Expected: "Guard probe B is on".
--     c) Tick Guard probe again.
--
--   TEST 13 — a loop in a library's own code stops the library and every module that runs it.
--     a) Press Ctrl+Alt+Win+6 (Guard probe B's key, which calls the library's spin).
--        Expected: about 2 seconds, then "Stopped Guard probe library and 2 modules that run its
--        code: its code ran too long in Guard probe B's hotkey callback, at src/main.luau line
--        23. Their keys go to the program in front again; they stay off until you turn
--        them on in the module manager." The error window names all three.
--     b) Ctrl+Alt+Win+9 and Ctrl+Alt+Shift+Win+0. Expected: nothing — B and C are off.
--     c) Ctrl+Alt+Win+0.                         Expected: "Guard probe is on" — it does not run
--        the library's code.
--     d) In the manager, three rows are unticked: the library's ends "— stopped: its code ran too
--        long in Guard probe B's hotkey callback, at src/main.luau:23", B's and C's
--        "— stopped with Guard probe library: its code ran too long in Guard probe B's hotkey
--        callback, at src/main.luau:23".
--     e) Tick the library's row only.            Expected: all three rows ticked again, "Guard
--        probe B has the slot, chime …", and B and C answer their keys again.
--
--   TEST 14 — your real modules: nothing changes.
--     Quit, and start a RELEASE build with your usual modules (.\run-dev.ps1 -Build -Release): a
--     debug build runs the host's own work many times slower — one full-region image match took
--     twelve seconds there — and that time counts on the callback's clocks. Use Kontakt with a
--     CSS library, Melodyne and ON:EAR as always: Tab through each, one hotkey each, change a
--     setting in a module's dialog.
--     Expected: nothing new is heard, and the log has no "[guard] … stopped" line. It has a
--     "[guard] … memory limit …" line per module at start, and "[cpu]" lines — a module above 10 %
--     of a minute, one callback over 250 ms, and a summary per module at exit. Send those lines:
--     they set the thresholds, and show how long each module's load takes.
--
--   TEST 15 — a callback that yields in a loop is one callback: stopped like any other loop.
--     Press Ctrl+Alt+Shift+Win+5.                Expected: as TEST 6, at line 262.
--        FAILS AS: the application not answering for 30 seconds, then "No stop: this build has
--        no time limit" — every yield started the clocks afresh.
--
--   TEST 16 — a module's own loop that calls a library's helper is the module's loop.
--     a) Press Ctrl+Alt+Shift+Win+6 (Guard probe B's key: it loops, asking the library each time).
--        Expected: "Stopped Guard probe B: its hotkey callback ran for 2 seconds of processor time
--        without returning, at src/main.luau line 30. Its keys go to the program in front
--        again; it stays off until you turn it on in the module manager."
--     b) Ctrl+Alt+Shift+Win+0.                   Expected: "Guard probe C is on" — the library
--        and C were not stopped.
--     c) Tick Guard probe B.
--        FAILS AS: "Stopped Guard probe library and 2 modules …".
--
--   TEST 17 — a slow host call that a library's code makes stops only the module it ran for.
--     a) Press Ctrl+Alt+Shift+Win+7 (Guard probe B's key: the library reads a pixel over and over).
--        Expected: up to 10 seconds, then "Stopped Guard probe B: its hotkey callback ran for 10
--        seconds without returning, in host.screen.pixel, at Guard probe library's src/main.luau
--        line 31. Its keys go to the program in front again; it stays off until you turn
--        it on in the module manager." (Or "ran for 2 seconds of processor time", then ending "…;
--        more in the error window.")
--     b) Ctrl+Alt+Shift+Win+0.                   Expected: "Guard probe C is on".
--     c) Tick Guard probe B.
--        FAILS AS: the library and C stopped too.
--
--   TEST 18 — the application's own reload key, pressed during a stall, is not dropped.
--     a) Press Ctrl+Alt+Win+1, and during the silence press Ctrl+Alt+Shift+Win+F5.
--        Expected: the stop's sentence, then every module reloads: "Guard probe B has the slot,
--        chime …", and the report "… modules reloaded; 1 stopped module stays off".
--     b) Tick Guard probe again.
--
-- Each stop leaves one line in the log, marked [guard], and a dropped key a line marked [keys].
-- The slot's sentences do not interrupt each other (interrupt = false), so every one is heard.

local SLOT = "guard-probe"
-- Two claims on B's slot: B's onDeactivate fills memory when the one of specificity 2 takes it
-- (TEST 3), and loops when the one of 3 does (TEST 10).
local memoryClaim, timeClaim
local memoryMatching, timeMatching = false, false

-- Keeps strings of about 1 MiB in a table: 300 of them, more than the 256 MiB the VM may use.
-- Each one different — Luau keeps one copy of equal strings, and a table of the same string
-- grows by a few bytes a turn.
local function fill()
  local t = {}
  for i = 1, 300 do
    t[#t + 1] = string.rep("x", 1048000) .. i
  end
  host.speech.output("No stop: this build has no memory limit")
end

-- A loop that would run 30 seconds, in a function of its own so the traceback names it; the
-- loop is on one line, so the stop is always at it.
local function spin()
  local started, n = os.clock(), 0
  while os.clock() - started < 30 do for _ = 1, 100000 do n += 1 end end
  host.speech.output("No stop: this build has no time limit")
end

-- Work in C between Luau's checks: each call is milliseconds with no Luau instruction in it.
local function upperForever()
  local s, started, n = string.rep("a", 4 * 1024 * 1024), os.clock(), 0
  while os.clock() - started < 30 do local u = string.upper(s) n += #u end
  host.speech.output("No stop: this build has no time limit")
end

-- A loop over a host call that waits for the screen.
local function pixelForever()
  local started = os.clock()
  while os.clock() - started < 30 do local c = host.screen.pixel(0, 0) end
  host.speech.output("No stop: this build has no time limit")
end

-- A callback that yields over and over: the host answers each yield at once, raising at it.
local function yieldForever()
  local started = os.clock()
  while os.clock() - started < 30 do pcall(coroutine.yield) end
  host.speech.output("No stop: this build has no time limit")
end

return {
  activate = function()
    memoryClaim = host.arbiter.register(SLOT, 2, function()
      host.speech.output("Guard probe has the slot", { interrupt = false })
    end, function()
      host.speech.output("Guard probe gave up the slot", { interrupt = false })
    end)
    timeClaim = host.arbiter.register(SLOT, 3, function()
      host.speech.output("Guard probe has the slot, for the time test", { interrupt = false })
    end, function()
      host.speech.output("Guard probe gave up the slot", { interrupt = false })
    end)

    host.hotkey.register("Ctrl+Alt+Win+0", function()
      host.speech.output("Guard probe is on")
    end)

    host.hotkey.register("Ctrl+Alt+Win+1", function()
      spin()
    end)

    host.hotkey.register("Ctrl+Alt+Win+2", function()
      fill()
    end)

    host.hotkey.register("Ctrl+Alt+Win+3", function()
      upperForever()
    end)

    host.hotkey.register("Ctrl+Alt+Win+4", function()
      memoryMatching = not memoryMatching
      host.arbiter.setMatching(SLOT, memoryClaim, memoryMatching)
    end)

    host.hotkey.register("Ctrl+Alt+Shift+Win+4", function()
      timeMatching = not timeMatching
      host.arbiter.setMatching(SLOT, timeClaim, timeMatching)
    end)

    host.hotkey.register("Ctrl+Alt+Win+5", function()
      local p = host.input.cursorPos()
      host.input.mouseDown(p.x, p.y)
      spin()
    end)

    host.hotkey.register("Ctrl+Alt+Shift+Win+5", function()
      yieldForever()
    end)

    host.hotkey.register("Ctrl+Alt+Win+7", function()
      pixelForever()
    end)

    host.hotkey.register("Ctrl+Alt+Win+8", function()
      local ok = pcall(fill)
      host.speech.output(ok and "It fit, which it should not" or "Caught it. Guard probe goes on.")
    end)

    -- One second of arithmetic: long, but it ends, so it is not stopped.
    host.hotkey.register("Ctrl+Alt+Shift+Win+1", function()
      local started, n = host.now(), 0
      while host.now() - started < 1000 do
        n += 1
      end
      host.speech.output("finished")
    end)

    host.log.info("[guard-probe] loaded: Ctrl+Alt+Win+0 to 8, Ctrl+Alt+Shift+Win+1, 4 and 5")
  end,
}
