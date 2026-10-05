# Module Runtime & Lifecycle

*Drafted 2026-06-21, brought in line with the host 2026-09-22. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md) (§3 process/isolation model) and [host-api-capability-catalog.md](host-api-capability-catalog.md).*

> **Status:** the model below is built — see the [Module Manager](module-manager.md) guide for
> what the user sees (enable/disable, per-module settings, install/browse, version-based
> updates, hotkey-conflict detection, in-place reload, crash reporting). Each VM has a memory
> limit and each callback a time limit, and a module that goes past either is stopped
> ([below](#limits)). Where the original draft promised more than was built — a VM dropped on
> disable, automatic disabling of a faulting module, a scriptable control surface — this page
> now says what happens instead. The CLI offers search/install/list/update/uninstall;
> enable/disable/reload are in the manager window (and a system-wide key reloads everything),
> not a scriptable IPC.

## Principle

**One platform process hosts many modules concurrently — not one OS process per module.** Every loaded module lives in the same process; the host multiplexes events (hotkeys, captured keys, window triggers, timers, image results, controller events) to them on one thread. Modules can be enabled and disabled at runtime.

## Why not process-per-module

- **Hot-path latency:** the global hotkey / window-trigger dispatch must live in one place (the event loop) for low latency. Fanning every event out to N processes via IPC is the wrong shape (feasibility study §3 trade-off).
- **Cost:** each process means its own memory + VM + startup latency; dozens of small modules would be wasteful.
- **What it gives up:** isolation. The draft assumed the Luau VM would contain a misbehaving module on its own. It does for memory and for time — every VM has a memory limit and every callback a time limit, and a module that goes past either is stopped ([below](#limits)) — but not for everything: a callback holds the whole application for up to its 2 seconds before it is stopped, a host call that never returns holds it until it returns, long but finite work under the limit holds it while it runs (see [Runtime model](#runtime-model)), and no limit covers a crash in the host. The capability list catches a module reaching for a namespace by accident; it is not a boundary against a module that is written to misbehave.

## Runtime model

- **One thread for module code.** The event loop runs on the main thread: the Win32 message loop (the wxWidgets one while the module manager exists), or the macOS run loop, driven by a 15 ms tick. Every module callback and every host call a module makes run there, one at a time, and so do the dispatch of hotkeys, captured keys, window events and controller events and, on macOS, the event tap. Some host calls hand their work to a thread of the host's own and return at once; their answer comes back to a callback on the event loop, on a later tick — or, for a text read without a callback, to the handler that waits for it. Which calls do that, and which hold the loop until they return, is under [Threads](#threads).
- **One Luau VM per module**, all in the same process: globals and state are separate between modules, and each can be enabled and disabled on its own.
- **A callback that runs too long is stopped.** A callback runs to its end on the one thread, so while it runs it stalls the whole application: every module, speech, every captured key and hotkey waiting for its callback, and on macOS the event tap, which the system switches off once it stops answering for about 300 ms. One that has not returned after 2 seconds of the event loop's processor time, or 10 seconds in all, is stopped and its module turned off ([below](#limits)), so a loop without end costs about 2 seconds of silence rather than the application. Below that limit nothing interrupts it — computing statistics over thousands of pixels in Luau holds everything for as long as it takes. Keep callbacks short, leave pixel work to the host's reductions ([`profile`](api/screen.md#host-screen-profile), [`cells`](api/screen.md#host-screen-cells), image search), and poll with the forms that answer in a callback ([`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) given one, [`matchCellsAsync`](api/screen.md#host-screen-matchcellsasync), [`imageSearchAsync`](api/screen.md#host-screen-imagesearchasync), [`imageSearchEach`](api/screen.md#host-screen-imagesearcheach), [`snapshotAsync`](api/screen.md#host-screen-snapshotasync)).
- **Errors are contained, not acted on.** A Luau error or a Rust panic in a callback is caught; the rest of the application goes on. The error is written to the log the first time it comes — per module, kind of callback and message — and then counted, with one line a minute at most saying how often it came again (a poll that fails every 500 ms wrote 170 000 identical lines a day); it is shown in an accessible dialog once per module and kind of callback. The log's count starts again when the module is reloaded or disabled and enabled, so the first error after a fix is written in full; the dialog shows again after the module is disabled and enabled. Nothing disables a module that keeps failing — but one whose callback runs too long, or runs out of memory without catching it, is stopped ([below](#limits)).
- Loading a module runs its entry file, which **registers** hotkeys, captured keys, window triggers, overlays, timers and listeners with the host. The host keeps them keyed by owning module, so each can be revoked or skipped per module.
- **A `code_module` runs more than once.** It is loaded as a module in its own right, with its own VM, and its source is evaluated again inside the VM of every module that depends on it, directly or through another `code_module`; each copy's registrations belong to the VM it ran in. Work meant to happen once goes into an `activate` function in the table the entry returns — see [`host.require`](api/require.md).

### A callback that runs too long, or a module that uses too much memory {#limits}

Two limits hold every module: a callback may run for 2 seconds of the event loop's processor time or 10 seconds in all, and a VM may use a set amount of memory. A module that goes past either is **stopped**: turned off until the next start, with a spoken sentence, a dialog, a log line and a note in its row of the module manager. Neither limit can be changed in the application's settings; the memory a module may use can be raised in its manifest.

**Time.** Two clocks count each callback, from the moment the host calls into the module's Luau until the call returns to the host: the event loop's **processor time**, at most **2 seconds**, and the **wall clock**, at most **10 seconds**.

- The call that counts is the outermost: a handler's run (a hotkey's, a key's, a timer's, a window trigger's, a read's callback — see [Each callback is a handler](#handlers)), an arbiter's `onActivate` or `onDeactivate` the host runs on its own, or a step of loading a module (a `code_module`'s code, the entry file, `activate`). A callback that runs inside another module's — an `onDeactivate` run by another module's `host.arbiter.setMatching` — counts on the clocks of the one that began. A handler that waits at a wait point stops its clocks there and starts them afresh when it goes on, so waiting never counts. Its own `coroutine.yield`, which the host answers at once by raising at the yield, is no wait: the clocks run on through it, so `while true do pcall(coroutine.yield) end` is stopped like any other loop.
- Processor time grows only while the event loop really computes. Waiting for another program — an accessibility answer, a capture — and waiting for the processor behind other programs do not count on it, but they do count on the wall clock. Under full load a loop collects processor time slowly, and then the wall clock decides, at 10 seconds.
- A thread of the host's own (`vm-guard`) looks at both clocks every 50 ms. Its first look at a callback — up to 50 ms after the callback began — takes the processor-time baseline, so the event loop makes no system call per callback, and what ran before that look is not counted; a limit is met up to one look late; and the module then samples where it is for 10 ms (below). So a loop is stopped after 2.0 to about 2.15 seconds of processor time, or 10.0 to about 10.1 seconds on the wall clock.
- The stop comes at the module's next safepoint — a loop's back edge, a call, a return, a step of the pattern matcher: at once in a loop of Luau, at the next step of the pattern matcher in a long `string.find`, and after a host call returns when the time ran out inside one. **A host call that never returns is not stopped**: it holds the application until it returns, and the module is stopped then (TODO.md has the second stage that will cover it).
- A module's `pcall` sees the stop as an error with the text `stopped by the host: this callback ran too long, and the module is turned off`, and sees it again at every safepoint after it, in a loop around it, in its error handler, in its own coroutines and metamethods — nothing it does runs on. Code after the stop does not run, so cleanup after a loop does not happen; the host takes back what it holds for the module itself (below).
- **Who takes the stop.** The module whose Luau runs when the time is up — unless it runs inside another module's callback that began the count, and its own call began less than half the budget ago, on the wall clock: 1 second when the processor time ran out, 5 when the wall clock did. Then the time is mostly its caller's: it runs on, and the caller takes the stop as soon as that call returns. A quick `onDeactivate` is not stopped for the loop of the module whose `setMatching` ran it; one that loops itself is stopped once its call is that old, even when its caller had begun to look where it was (below). The time the messages give is the count of the callback that began, of which the stopped one ran at least half; the host call they name is the one running when the time ran out, and only when it was the stopped module's.
- **Whose code it is.** Before it stops, the module that takes the stop looks where it is at each safepoint for 10 ms, and at least 16 times: the innermost function that is in every look is the loop, where a helper the loop calls comes and goes. When that function is in a library's file — a `code_module` whose code runs inside its dependents' VMs, such as the overlay runtime — **and** the loop spent the 2 seconds of processor time in Luau itself, the library and every module whose VM runs its code are stopped in the same step, so a loop in the overlay runtime costs one stall and not one per overlay. A `code_module`'s own code is a library's code wherever it runs: a loop in its own hotkey, in its own VM, stops it and every module that runs its code too. In every other case only the module that took the stop is stopped: when the loop is in the module's own code, or in a library's helper it calls; when the wall clock decided; when the time ran out in a host call or just after one — a whole-window image match, an accessibility walk into a plug-in that hangs, made by the library's code for one module — and when the loop is in the host's own Luau. The place in the messages is still the loop's line, in the library's file if it is there. A callback that returns before its 10 ms of looking are up — one long host call, and then its end — is stopped all the same, its module alone. A module that depends on a library only for data (`code_module = false`) does not run its code, and is not stopped with it.
- The callback that called into the stopped one, in another module, goes on with fresh clocks of its own: the stopped one's call ends in it as a failed call (`host.arbiter.setMatching` returns, and the stopped module's `onDeactivate` simply did not finish).
- **The host's own work counts.** A host call's time is the callback's: in a debug build of the application, where the host runs many times slower than released — one full-region image match took twelve seconds there — a callback the release build runs in a fraction of a second can be stopped. Judge real modules on a release build (`.\run-dev.ps1 -Build -Release`).
- **Loads are held to the same limits.** A module whose entry file or `activate` runs past them does not load ("A module did not load" at start-up, "Reload failed" for a reload), and nothing else is stopped: `stopped while loading: its entry file ran for 2 seconds of processor time without returning, at src/main.luau line 3`.

**Memory.** Every VM has a memory limit: 256 MiB for each module whose code runs in it — the module itself and each `code_module` it depends on, each once — or what a module's manifest asks for instead with `[limits] memory_mib` (256 to 2048). How the sum is made, what counts and what does not: [module-package-format.md](module-package-format.md#limits). Real modules use a few MB.

The allocation that would go past the limit fails, with Luau's `not enough memory`. Caught by the module's own `pcall`, that is the module's business: it goes on, and so does its VM. What it allocated before the error stays until the collector reaches it — Luau has no emergency collection — so another large allocation in the same callback can meet the limit again; when a callback returns with less than an eighth of its VM's limit free — or less than 2 MiB, in a VM so small that an eighth is less — the host collects the VM's garbage at once, so the next callback has its room back. **Not caught** — the error reaches the host at the end of a callback, of a plain call such as an arbiter's `onDeactivate`, or of a step of loading — it **stops the module**. So does a run-time error with no value (`nil`) from a VM that has less than a sixty-fourth of its limit free — or less than 256 KiB, in a VM so small that a sixty-fourth is less: mlua at times makes one of a memory error that met the limit inside a host call. A memory stop takes only the module whose VM ran out of memory: memory cannot be laid at the door of the code that holds it, so a library whose code runs in the VM is not stopped with it. Luau does not say where it ran out of memory, so the messages name the module and the callback, not a line — unless the error came through a host call, whose traceback is shown. A module that runs out while it loads does not load, and nothing else is stopped: `stopped while loading: its entry file needed more than the 256 MiB of memory it may use`.

**What a stop does**, either way:

- The module is turned off at once, as soon as the callback that the event loop began has returned — before anything else of the event loop runs. What reads the enabled flag skips it from then on, in the rest of the same dispatch too.
- Its captured keys, hotkeys and controller demand go back to the program in front at the same moment. A mouse button it pressed with [`host.input.mouseDown`](api/input.md#host-input-mousedown) and had not released is released where the pointer is.
- **Keys pressed while the callback held the application are dropped**, with a `[keys]` line: on Windows, the captured keys and hotkey presses that waited for the event loop when the stop came — pressed during the stall, or a moment before it while the event loop worked through the keys and events that came with the one that stalled — and a `RegisterHotKey` press made during the stall even when it arrives after it, by its time stamp, so that whichever module takes the key next does not get ten Tab presses at once. A key the event loop had taken out of its queue before the stall began still arrives, late. The application's own reload key is not dropped: pressed during the stall, it reloads every module after the stop. On macOS the event tap runs on the stalled thread itself, and the system switches it off after about 300 ms, so keys typed during the stall go to the program in front and none wait to be dropped (not checked on a Mac yet). Controller presses are not dropped. A stall that ends without a stop drops nothing: the keys typed ahead arrive as before.
- **Keys that waited for it are dropped too.** A module can be stopped while one of its handlers waits for a read — its `onDeactivate` looped meanwhile, say, or a library whose code runs in it did. The keys, hotkey presses and controller buttons that waited in its mailbox behind that handler ([A handler waits](#a-handler-waits)) are dropped with the stop, and said in a `[keys]` line of their own, `[com.example.game] 3 key(s) that waited for it were dropped with the stop: Tab ×2, Space`, and in the dialog as `Dropped: 3 keys that waited for it: Tab ×2, Space.`
- The log line below is written.
- At the end of that turn of the event loop the rest of what disabling does follows — its overlays lose their slots, its text reads, snapshots, handlers and queued events are dropped, its key scope and menu flag are forgotten — then the dialog, the spoken sentence, and the note in its row of the module manager. After a memory stop its VM's garbage is collected then too.
- It stays off **until the next start**: `settings.toml` keeps it on, so the next start loads it, and ticking it in the module manager turns it on again at once — **built afresh**, as a reload builds it, not picked up where the stop left it: a stop ends a callback anywhere, an overlay's `onDeactivate` after it marked itself inactive and before it gave its keys back, say, and the same VM turned on again would carry that on. It runs its entry file and `activate` again and reads its settings as they are now. Ticking the library of a library's stop turns on every module the stop took with it, each built afresh. A module that cannot be built afresh stays off, and the error window says why. A reload keeps a stopped module off, and the reload key's report counts the stopped modules that stay off. To keep it off after a restart as well, tick it and untick it.

The words, for a module whose hotkey loops in a function `spin` of its own, holding the left mouse button down, while the user pressed Tab twice and Space. Spoken — through the screen reader, as the application's other announcements are, or shown as a notification where there is one. A notification holds 255 characters on Windows, so the sentence is made to fit: when the whole of it is longer, its second half names the error window for the rest, and then its first half counts the modules of a library's stop instead of naming them, says "ran too long", and leaves out the callback it ran inside. It always names the module, the callback, the host call and the place.

```
Stopped Game helper: its hotkey callback ran for 2 seconds of processor time without returning, at src/main.luau line 40. Its keys go to the program in front again; it stays off until you turn it on in the module manager.
```

The dialog, titled `Module stopped: com.example.game`, in the same window as the application's other error reports. It may come at the same moment as the sentence, and the screen reader may read the window rather than the sentence — so the dialog says everything the sentence says:

```
Stopped Game helper: its hotkey callback ran for 2 seconds of processor time without returning, at src/main.luau line 40.

What ran too long: Game helper's hotkey callback.
How long: 2.0 s of processor time, 2.1 s in all. A callback may take 2 s of processor time or 10 s in all.
In a host call: no, it was running Luau code.
Where, innermost first:
  com.example.game/src/main.luau:40: in function 'spin'
  com.example.game/src/main.luau:44: in function <com.example.game/src/main.luau:43>
Turned off: Game helper (com.example.game).
Released: the left mouse button it was holding down.
Dropped: 3 keys pressed while the application waited: Tab ×2, Space.

Its keys go to the program in front again. It stays off until you turn it on again in the module manager's Installed list, or start the application again; turned on, it is built afresh, as a reload builds it. To keep it off after a restart too, tick it and untick it. If it happens again, send this text to the module's author.
```

The log lines, written when it is turned off:

```
[guard] [com.example.game] stopped: its hotkey callback ran 2003 ms of processor time (2110 ms in all) without returning, past the processor-time limit; in Luau code; at com.example.game/src/main.luau:40; off until the next start: com.example.game; released: left mouse button of Game helper; dropped: 3 key(s), 0 hotkey press(es); frames: com.example.game/src/main.luau:40: in function 'spin' | com.example.game/src/main.luau:44: in function <com.example.game/src/main.luau:43>
[keys] 3 key(s) and 0 hotkey press(es) made while Game helper's hotkey callback held the application were dropped, as the module was stopped: Tab ×2, Space
```

And its row in the module manager reads `Game helper  v1.2.0   (com.example.game) — stopped: its hotkey callback ran too long, at src/main.luau:40`, unticked. The frames name each module's file by the module's id and the path in its folder; `[C]` is a function of Luau's own library, and `(host)` a part of the host's own Luau.

When the wall clock decides, inside a host call — the row then reads `— stopped: its hotkey callback ran too long, in host.screen.pixel, at src/main.luau:71`:

```
Stopped Game helper: its hotkey callback ran for 10 seconds without returning, in host.screen.pixel, at src/main.luau line 71. Its keys go to the program in front again; it stays off until you turn it on in the module manager.
```

A callback that ran inside another module's callback says so — here an arbiter's `onDeactivate`, run when another module's hotkey took its slot:

```
Stopped Synth overlay: its arbiter onDeactivate callback, run during Game helper's hotkey callback, ran for 2 seconds of processor time without returning, at src/main.luau line 22. Its keys go to the program in front again; more in the error window.
```

Game helper, whose hotkey ran it, goes on with fresh clocks: only Synth overlay is stopped.

A slow host call that a library's code made for one module — a whole-window image match the overlay kit ran in Synth overlay's hotkey, past the wall clock — stops only that module. The place is the library's line, named as the library's file:

```
Stopped Synth overlay: its hotkey callback ran for 10 seconds without returning, in host.screen.imageSearchMulti, at Overlay kit's src/main.luau line 1595. Its keys go to the program in front again; it stays off until you turn it on in the module manager.
```

Its row reads `— stopped: its hotkey callback ran too long, in host.screen.imageSearchMulti, at Overlay kit's src/main.luau:1595`.

A loop in a library's own code that spent the processor time, run in Synth overlay's hotkey callback, stops the library and the two modules that run its code. Said in full, this sentence would be longer than a notification holds, so it counts them:

```
Stopped Overlay kit and 2 modules that run its code: its code ran too long in Synth overlay's hotkey callback, at src/main.luau line 9. Their keys go to the program in front again; they stay off until you turn them on in the module manager.
```

The dialog, titled `Modules stopped: com.example.kit and 2 that run its code`:

```
Stopped Overlay kit and the 2 modules that run its code, Sampler overlay and Synth overlay: its code ran for 2 seconds of processor time without returning in Synth overlay's hotkey callback, at src/main.luau line 9.

What ran too long: Overlay kit's code, in Synth overlay's hotkey callback.
How long: 2.0 s of processor time, 2.1 s in all. A callback may take 2 s of processor time or 10 s in all.
In a host call: no, it was running Luau code.
Where, innermost first:
  com.example.kit/src/main.luau:9: in function 'spin'
  com.example.synth/src/main.luau:41: in function <com.example.synth/src/main.luau:40>
Turned off: Overlay kit (com.example.kit), Sampler overlay (com.example.sampler), Synth overlay (com.example.synth).

Their keys go to the program in front again. They stay off until you turn them on again in the module manager's Installed list, or start the application again; ticking Overlay kit turns them all on again, each built afresh, as a reload builds it. To keep one off after a restart too, tick it and untick it. If it happens again, send this text to the module's author.
```

The library's row reads `— stopped: its code ran too long in Synth overlay's hotkey callback, at src/main.luau:9`, and each of the others `— stopped with Overlay kit: its code ran too long in Synth overlay's hotkey callback, at src/main.luau:9`.

The library's own VM looping in its own hotkey is the same rule — its code, wherever it runs:

```
Stopped Overlay kit and 2 modules that run its code: its code ran too long in its own hotkey callback, at src/main.luau line 30. Their keys go to the program in front again; they stay off until you turn them on in the module manager.
```

Its row reads `— stopped: its code ran too long in its own hotkey callback, at src/main.luau:30`, and the others' `— stopped with Overlay kit: its code ran too long in Overlay kit's hotkey callback, at src/main.luau:30`.

When the library was off already, the modules that run its code are stopped without it:

```
Stopped 2 modules that run Overlay kit's code: Overlay kit's code ran too long in Synth overlay's hotkey callback, at src/main.luau line 9. Their keys go to the program in front again; they stay off until you turn them on in the module manager.
```

The dialog is titled `Modules stopped: com.example.sampler and com.example.synth, which run Overlay kit's code`, and asks you to tick each of them.

```luau
-- Stopped: a loop that never ends. About 2 seconds of silence, then the sentence above.
host.hotkey.register("Ctrl+Alt+Win+1", function()
  local n = 0
  while true do n += 1 end
end)

-- Not stopped: a second of arithmetic holds the application for that second, then says "finished".
host.hotkey.register("Ctrl+Alt+Win+8", function()
  local started, n = host.now(), 0
  while host.now() - started < 1000 do n += 1 end
  host.speech.output("finished")
end)
```

The words of a memory stop, for a module that keeps adding to a table in a hotkey's callback. Spoken:

```
Stopped Game helper: its hotkey callback needed more than the 256 megabytes of memory it may use. Its keys go to the program in front again, and it stays off until you turn it on in the module manager or start the application again.
```

The dialog, titled `Module stopped: com.example.game`:

```
Stopped Game helper: its hotkey callback needed more than the 256 megabytes of memory it may use.

What used too much memory: Game helper's hotkey callback.
How much: more than its 256 MiB — all of it its own — with 255.9 MiB in use when it was stopped.
Where: not known. Luau does not say where it ran out of memory.
Turned off: Game helper (com.example.game).

Its keys go to the program in front again. It stays off until you turn it on again in the module manager's Installed list, or start the application again; turned on, it is built afresh, as a reload builds it. To keep it off after a restart too, tick it and untick it. If it happens again, send this text to the module's author.
```

The log line, written when it is turned off:

```
[guard] [com.example.game] stopped: its hotkey callback needed more than the 256 MiB of memory its VM may use (255.9 MiB in use); where is not known; off until the next start: com.example.game
```

And its row in the module manager reads `Game helper  v1.2.0   (com.example.game) — stopped: its hotkey callback needed more than 256 MiB of memory`, unticked. Released buttons and dropped keys add their lines as for a time stop.

A memory stop of a callback that ran inside another module's callback — an arbiter's `onDeactivate` of a module that declares 300 MiB and runs a library's code, run when another module's hotkey took its slot, which ran out reading its presets through a host call. The sentence:

```
Stopped Synth overlay: its arbiter onDeactivate callback, run during Game helper's hotkey callback, needed more than the 556 megabytes of memory it may use. Its keys go to the program in front again; more in the error window.
```

The dialog, with the traceback mlua kept of the host call it ran out in — the chunks are named by their files' paths there:

```
Stopped Synth overlay: its arbiter onDeactivate callback, run during Game helper's hotkey callback, needed more than the 556 megabytes of memory it may use.

What used too much memory: Synth overlay's arbiter onDeactivate callback, run during Game helper's hotkey callback.
How much: more than its 556 MiB — 300 of its own (set in its module.toml) and 256 for Overlay kit (com.example.kit), whose code runs in it — with 555.9 MiB in use when it was stopped.
Where, as far as it is known:
  stack traceback:
  [C]: in function 'read'
  [string "C:\AutomationPlatform\modules\synth\src\main.luau"]:12: in function 'loadPresets'
Turned off: Synth overlay (com.example.synth).

Its keys go to the program in front again. It stays off until you turn it on again in the module manager's Installed list, or start the application again; turned on, it is built afresh, as a reload builds it. To keep it off after a restart too, tick it and untick it. If it happens again, send this text to the module's author.
```

Game helper, whose hotkey ran it, goes on: only Synth overlay is stopped, and its row reads `— stopped: its arbiter onDeactivate callback needed more than 556 MiB of memory`. A module that ran out in Luau's own code — `string.rep`, a table growing — has no such traceback, and its dialog says `Where: not known.`

```luau
-- Caught: the module's own business. The callback says so and goes on; nothing is stopped.
host.hotkey.register("Ctrl+Alt+Win+2", function()
  local ok = pcall(function()
    local t = {}
    -- Each string different: Luau keeps one copy of equal strings.
    while true do t[#t + 1] = string.rep("x", 1048576) .. #t end
  end)
  host.speech.output(ok and "done" or "too big, skipped")
end)
-- Not caught: the same loop without the pcall stops the module, with the words above.
```

What a stopped module is, apart from being off: no code of it runs — a call into its VM is refused, a setting's `onChange` included, which waits in its mailbox and goes with the VM when the module is built afresh; an arbiter claim it held loses its slot at once, and its `onDeactivate` is not run: the module built afresh starts with no overlay active ([`host.arbiter`](api/arbiter.md#host-arbiter-setmatching)).

**The guard's own lines**, besides the stop's, all marked `[guard]`:

```
[guard] [com.example.melodyne] memory limit 512 MiB: 256 of its own, 256 for com.platform.overlay
[guard] [com.example.game] a callback is past its limit (2003 ms of processor time, 2049 ms in all): the module whose code it runs is stopped at its next Luau safepoint, unless the callback returns first
[guard] [com.example.game] the callback past its limit returned before it was stopped; nothing was stopped
[guard] the watchdog thread could not start (…): no callback can be stopped for running too long
[guard] mlua did not install its interrupt; this VM cannot be stopped for running too long
```

The first is written for every VM as it is built. The second is written by the watchdog the moment a callback goes past a limit, with `, in host.x` when a host call was running then; it names the module whose callback began, and the stop's own line follows when the module is stopped. A host call that never returns leaves this line alone in the log — the module is stopped only when the call comes back. The third follows it when the callback returned first — between two looks, or while the module was still looking where it was — and nothing was stopped. The last two say the guard cannot stop a callback for time at all: no watchdog thread (the system would not start one; memory is still limited), or a VM without mlua's interrupt (that VM only).

**What each module takes of the event loop** is measured too, and changes nothing: every 50 ms the watchdog charges the event loop's processor time since its last look to the module whose code runs at that moment. A module that took 10 % or more of a minute gets a `[cpu]` line for it, and one callback of 250 ms or more of processor time gets a line of its own, each at most once a minute; at exit a line per module says what it took over the whole run. The two thresholds are provisional, to be set after the first measurements. A step of loading counts as a callback here.

```
[cpu] [com.example.game] used 7.9 s of the event loop's processor time in the last 60 s (13 %); its longest callback took 0.6 s. A measurement for module authors and for deciding on threads per module; it changes nothing
[cpu] [com.example.game] one callback took 0.6 s of the event loop's processor time. A measurement for module authors and for deciding on threads per module; it changes nothing
[cpu] [com.example.game] 41.2 s of the event loop's processor time over 2 h 10 min of running (0.5 %); its longest callback took 0.6 s
```

The longest callback is measured between the watchdog's looks, so a callback shorter than two looks (100 ms) shows as `none of its callbacks ran across two looks of the watchdog`, and a longer one up to 100 ms short.

## Threads {#threads}

Module code runs on the event loop and nowhere else, so a module never needs a lock, and runs a second callback of its own only while one of its handlers waits — in the three ways [below](#a-handler-waits). What the host does for it runs in one of two places: on a thread of the host's own, with the answer handed back to the loop, or on the loop itself, which then waits until the call returns. The API pages give each call's cost; this is the overview.

**Handed to a thread of the host's own.** The call itself costs the event loop what it takes to check its arguments — for `matchCellsAsync` that includes decoding every state it is given, and for the image searches loading any template file that is not cached yet — plus, in a module that reads through desktop duplication, the comparison described at the end of the next list. The work goes on elsewhere.

| Thread | What runs there | How the answer comes back |
|---|---|---|
| `screen-capture` | The picture of every [`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) given a callback or waiting in a handler, taken at the call — except a read of a snapshot, which is not photographed — and every picture of [`host.screen.snapshotAsync`](api/screen.md#host-screen-snapshotasync): at once, at a set time, or each round of a change wait. One capture at a time, in the order the [screen page](api/screen.md) gives. | `snapshotAsync`'s `cb` on the loop, on the next turn after its picture; a text read's answer through `ocr-recognise`. |
| `ocr-recognise` | The recognition of those pictures, one job at a time. On macOS it first makes one warm-up pass of Vision, once the languages are read and the warm-up on a thread of its own (`ocr-warm-up`) has ended; reads asked meanwhile wait for it, on the 5-second hang clock of a job (see [`host.ocr.recognize`](api/ocr.md#macos)). | `cb` on the loop, or the handler that waits going on there, on the next turn after the recognition finished. |
| `paddle-ocr` | The neural recogniser, one thread for the application, one region at a time in the order they came. Every region of up to 400x200 (pixels on Windows, points on macOS) is handed to it the moment it is there — on macOS once the blank guard has found something in it — beside the system recogniser, and its answer is used only when the system recogniser read nothing (see [`host.ocr.recognize`](api/ocr.md#windows)); a region whose answer is no longer wanted is cancelled. On macOS, where the download carries it, the system recogniser is Vision's accurate ladder, each region runs at the quality of service of the thread that asked for it, and on an Intel Mac it also checks Vision's fast level (see [`host.ocr.recognize`](api/ocr.md#macos)). `recognize` where it [cannot wait](api/ocr.md#where-it-waits) hands its small regions to it too. | To the recognition that asked. One on the event loop, where `recognize` cannot wait, waits on the loop for it only when the system recogniser read nothing — on Windows as long as the recognition takes, on macOS no longer than what is left of the ladder's 250 ms, and on an Intel Mac for its check no longer than the fast pass took. |
| The image worker — one thread for every module | [`imageSearchAsync`](api/screen.md#host-screen-imagesearchasync), [`imageSearchEach`](api/screen.md#host-screen-imagesearcheach) and [`matchCellsAsync`](api/screen.md#host-screen-matchcellsasync): the capture, the reduction and the matching. The worker serves as one batch every request waiting when it becomes free and those that arrive in the 5 ms after it took the first, with one capture per distinct region and source, and the matching is spread across the processor's cores. A duplication wait or a large match holds up every request queued behind it, whichever module asked. | `cb` on the loop, on a later tick. |
| `dxgi-capture` (Windows) | Desktop duplication, for a module that declares `[screen] capture = "duplication"`. A read through it may wait for this thread: one on the event loop up to about 60 ms, one on the image worker or `screen-capture` up to 250 ms. No read waits while duplication is overdue or is being left alone after a failure, and an event-loop read does not wait when the loop's share of waiting is spent, or while duplication is opening — except the first event-loop read of an opening that was not started ahead of time (see [Which picture a read sees](api/screen.md#which-picture-a-read-sees)). | The read that asked. |
| `keyboard-hook` (Windows) | The low-level keyboard hook. It matches captured keys and granted hotkeys and swallows them without waiting for the event loop, so a busy loop never delays typing in another application. | The key's callback on the loop. |
| `gamepad` (Windows) | Reading XInput pads: every 4 ms while a pad is connected and either a `down`, `up`, `axis` or `chord` listener of an enabled module exists or a `list()` or `state()` came in the last five seconds; otherwise a look at the four slots every two seconds while anything listens or such a call is that recent, and nothing at all while nothing does (see [`host.gamepad`](api/gamepad.md)). On macOS, GameController delivers changes on a queue of its own. | The listener on the loop. |
| Speech (`prism-speech` and `prism-voice` on Windows, `voiceover` and `avspeech` on macOS) | Talking to the screen reader or the voice. [`host.speech.output`](api/speech.md#host-speech-output) hands the line over and returns. | — |
| Audio output | Playing what [`host.sound.play`](api/sound.md#host-sound-play) opened. | — |

The host keeps a few more threads that no module call reaches: the module manager's GitHub searches and installs, the listener that lets a second start bring this copy's manager forward, the warm-ups of the recognition models at start (`ocr-warm-up` on macOS, and the neural recogniser's, `paddle-warm-up` there), and `vm-guard`, which every 50 ms reads the wall clock and the event loop's processor time, arms the stop of a callback past its limit, and writes the `[cpu]` lines ([above](#limits)). It never runs module code: the stop itself happens on the event loop.

**On the event loop, which waits until the call returns.** Everything else a module calls. The ones that take time:

- **Reading the screen synchronously:** [`pixel`](api/screen.md#host-screen-pixel), [`pixels`](api/screen.md#host-screen-pixels), [`snapshot`](api/screen.md#host-screen-snapshot), [`profile`](api/screen.md#host-screen-profile), [`imageSearch`](api/screen.md#host-screen-imagesearch), [`imageSearchMulti`](api/screen.md#host-screen-imagesearchmulti), [`imageSearchAll`](api/screen.md#host-screen-imagesearchall), [`cells`](api/screen.md#host-screen-cells), [`matchCells`](api/screen.md#host-screen-matchcells), [`template`](api/screen.md#host-screen-template) with `capture` (and with `file`, which decodes the PNG), [`save`](api/screen.md#host-screen-save) and [`saveMarked`](api/screen.md#host-screen-savemarked). A read given a [snapshot](api/screen.md#reading-a-snapshot) captures nothing and costs only its own work. On Windows every other one of them but `template` with `file` is at least one capture: about 16.7 ms through the standard path; through desktop duplication about 0.3–5 ms on an idle desktop (more with a game in front — see **Cost** under [Which picture a read sees](api/screen.md#which-picture-a-read-sees)) with a wait for its answer of at most about 60 ms; a read duplication does not answer in that time is then read the standard way as well, unless the module declares `fallback = "none"`.
- **Recognising text where it cannot wait:** [`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) without a callback outside a handler — at a module's top level, in an arbiter's `onActivate` or `onDeactivate`, in a coroutine of the module's own ([Where it waits](api/ocr.md#where-it-waits)) — the capture and every recognition, with no time limit. The log says how long a module's first one of each kind of place held the loop, and at exit how often and how long in all. In a handler it waits instead, and holds only its module ([A handler waits](#a-handler-waits)).
- **Asking another application:** [`host.element`](api/element.md) — accessibility queries into another process, from about 30 ms to several hundred — and the [`host.window`](api/window.md) queries.
- **Acting:** [`host.input`](api/input.md) and [`host.window.focus`](api/window.md#host-window-focus). Each first waits up to 50 ms for the calling module's pending pictures on `screen-capture` to be taken — its `host.ocr.recognize`s', its plain `snapshotAsync`s' and the first picture of a change wait without `at` whose baseline that picture is — and a `drag` paces its movement for about 60 ms on Windows.
- **Reading the module's own files:** [`host.include`](api/include.md), [`host.resource.read`](api/resource.md#host-resource-read), and evaluating a `code_module` dependency at load.
- **First calls that start something:** the first [`host.gamepad`](api/gamepad.md) `list`, `state` or `on` starts the controller watching, and on Windows waits up to 250 ms for the first reading (on macOS it does not wait); the first granted hotkey or capture on Windows starts the keyboard hook's thread and waits for it to answer; `host.sound.play` opens the audio device the first time, and opens the file and starts its decoder every time; in the first moments after the application starts, [`host.ocr.languages`](api/ocr.md#host-ocr-languages), `resolveLanguage`, and `recognize` where it cannot wait, wait up to 50 ms for the language list.
- **Asking for a controller's state:** on Windows, while no `down`, `up`, `axis` or `chord` listener of an enabled module exists, a [`host.gamepad`](api/gamepad.md) `list()` or `state()` made more than five seconds after the last one waits up to 50 ms for a fresh reading.
- **In a module that reads through desktop duplication**, its first read after it was built — and each read after that until duplication has answered once — makes a comparison of the two ways of reading for the log, on the event loop, even when the read itself is one of the asynchronous calls above: a duplication read of up to about 60 ms and, when it answers, one standard capture — of the first of the call's regions that has a rectangle at the call (a window region whose window's client area is empty has none), or of the client area of the window in front when that region is too small to tell the two apart.

Pure computation — [`host.json`](api/json.md), [`host.keys.normalize`](api/keys.md#host-keys-normalize) and the like, [`host.timer`](api/timer.md) arming — costs what the work does and no more.

### Each callback is a handler {#handlers}

Every callback the host calls for an event runs as a coroutine of the host — a **handler** of its module: a hotkey's, a captured key's, a controller listener's, a timer's, a window trigger's (an activation and the report of the window already in front), an `onFocus`, the callback of a text read, an image search or a snapshot, and a setting's `onChange` called for the settings dialog or for another module's `set`. A module's handlers run one after another: the next one starts when the last has returned — after its waits, for one that [waits](#a-handler-waits) — and no handler of any module starts while another is running. A handler costs the event loop about 1 µs per event, where the plain call a callback was before cost about 0.1 µs — 1.08 to 1.15 µs against 0.07 µs over four runs of 20 000 events, in a release build on Windows, in a VM of the guard, whose bookkeeping of the [limits](#limits) each handler goes through — 0.13 to 0.14 µs for each entry and its end, of which a handler has two, its delivery and its stretch; the CI prints both figures on Windows and on an Apple Silicon Mac (`HANDLER COST:`, `GUARD ENTRY COST:`).

A few callbacks are plain calls, as before, and are no handlers: an arbiter's `onActivate` and `onDeactivate` (and the overlay runtime's on top of them), your own `onChange` called from your own [`host.settings.set`](api/settings.md#host-settings-onchange), and the top level of your module, of a `code_module` and of an [included](api/include.md) file.

In a handler, `coroutine.running()` returns the handler's coroutine, not `nil`, and `coroutine.isyieldable()` is `true`. A `coroutine.yield` there that reaches the host waits for nothing: it raises where it was made, so a `pcall` around it catches it and the callback goes on — on the same clocks ([the limits](#limits)) — with this message:

```
this callback runs as a coroutine of the host, and a coroutine.yield in it cannot wait for one of your own callbacks — wrap the code that yields in coroutine.wrap
```

Coroutines of your own work as they always did: the yields of a coroutine you made go to your own `resume`.

```luau
-- A generator of the module's own: its yields go to the module's call, not to the host.
local nextName = coroutine.wrap(function()
  for _, name in ipairs({ "Volume", "Pan", "Mute" }) do
    coroutine.yield(name)
  end
end)
host.hotkey.register("Ctrl+Alt+N", function()
  host.speech.output(nextName() or "done")
end)
```

### A handler waits {#a-handler-waits}

[`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) without a callback **waits** in a handler for its reading: the handler stops at the call, and the event loop goes on with everything else — other modules' keys and handlers, speech, the event tap. Only its own module waits. It is **busy** until the handler has gone on and returned, and its later events wait in its mailbox meanwhile, in the order they came, and then run one after another:

- **Keys, hotkey presses and controller buttons.** Up to 256 wait; the next is dropped, with a `[keys]` line at the first and then at most every 10 seconds while the module stays busy, naming the kind of handler it waited in: `[com.example.game] 4 key(s) or button(s) pressed while it was busy were dropped: 256 were already waiting (the module waited for timer, 2140 ms by the first of them)`. A held key's repeats fold into one while the last of that key waiting is a repeat, so a hold keeps one step; two presses never fold. A controller button's release is never dropped — a listener that heard the press hears the release — and an axis replaces the last one waiting for the same listener and axis.
- **A key runs on the registration it was pressed for.** When the module made that registration again meanwhile — an overlay that went and came back — the key runs on the new one, and the log says so: `[com.example.game] Tab, pressed while the module was busy, went to the module's current registration of it`. A captured key does not run on a capture its module has [scoped](api/keys.md#host-keys-scope) to another window since, and a hotkey does not run once another module holds it: such a press is dropped and said. When the module released the registration meanwhile and holds none for that combination any more, the press goes where it would have gone had the module never taken it: to the next module that [captures](api/keys.md#host-keys-capture) it in the window it was pressed in, through that module's mailbox as that module's key, and said: `[com.example.game] Tab, pressed while the module was busy, went to com.example.synth, the next module that captures it in the window it was pressed in: its registration was released meanwhile`. With no such module, it goes to the program in front — sent as it was pressed, while the window it was pressed in is still in front and no key typed after it has reached the program first — and is said: `[com.example.game] Tab, pressed while the module was busy, was passed on to the program in front: its registration was released meanwhile`; otherwise it is dropped, and the line says why (see the top of [`host.keys`](api/keys.md)). A key goes to another module only that way: never while its own module holds it, and never back to a module that let go of it.
- **Timers.** An [`every`](api/timer.md#host-timer-every) does not wait: its turn is skipped while its module is busy, and it comes round again at its interval. An [`after`](api/timer.md#host-timer-after) that comes due waits, and runs late, never early; it can be cancelled until it runs.
- **Everything else waits and is never dropped**: window activations and the report of the window already in front, focus changes — folded into one, which asks the focus as it is when it runs — the answers of reads, image searches and snapshots, and a setting's `onChange` called for the dialog or for another module's `set`.

A handler that has kept its module busy for 30 seconds with keys waiting behind it is said once, with where it waits; a timer's handler that waited with keys behind it is said once per place — both lines are under [Where it waits](api/ocr.md#where-it-waits). The waiting itself counts on no clock of the [limits](#limits).

**Three ways into a waiting module.** While a handler waits, the module's code runs in only three ways, each a plain call: its overlays' arbiter [`onActivate` and `onDeactivate`](api/arbiter.md#host-arbiter-register), run at once when another module's `host.arbiter.setMatching` or `unregister` moves them; its own `onChange` run by a `host.settings.set` that code makes; and that code resuming the waiting handler itself, with a thread it kept from `coroutine.running()`. The last ends the wait: the call raises in the handler with `host.ocr.recognize: this callback's coroutine was resumed by the module's own code, and so the host runs it no more; a callback's coroutine is the host's to resume`, the handler goes on inside the call that resumed it, and the module is free for its next event. Nothing else of the module runs before the handler has returned.

**One module per plug-in.** A module runs one handler at a time, so a port that keeps every plug-in in one module has one plug-in's read hold the keys of another in the next window ([One module per plug-in](building-an-overlay.md#one-module-per-plug-in)).

The rule for what may end a wait: **a wait is ended by the host** — a thread of its own, such as the text recogniser handing back a reading, or state it keeps — **never by another event of the same module**. That event would be next in line behind the very handler waiting for it, so a "wait until my key comes up" built on the module's own key callbacks could never end.

## Lifecycle / states

- **Installed** — a folder under `modules/` beside the application (see [where modules are found](module-package-format.md#where-modules-are-found)).
- **Loaded** — its VM built and its entry file run, then the returned `activate` (if any). Every installed module that its `supported_os` allows is loaded at start-up, **disabled ones included**.
- **Enabled** — the user wants it active; persisted in `settings.toml` beside the application. Its callbacks are delivered.
- **Disabled** — still loaded. Its OS hotkeys are released (and may pass to another module's standing claim), its captured keys leave the suppression set and its [key scope and menu flag](api/keys.md#host-keys-scope) are forgotten — after the `onDeactivate` below, which would set them again; enabled again, its captures are back, in every window and with no menu, until it sets them again — its controller demand is withdrawn, and its overlays lose any contested slot — an overlay that held one gets its `onDeactivate`, so it can tear down. Its other callbacks are not called, with one exception: a `host.settings.onChange` callback still fires when its setting changes, and the Settings dialog in the module manager works for a disabled module too. Nothing else is undone: the **VM, its globals, its recurring timers and its listeners all survive**, recurring timers keep being re-armed (a one-shot `after` that comes due meanwhile is discarded uncalled), and an image search that was answered while it was off is searched again when it is enabled. Enabling it again resumes delivery; the entry file does **not** run again. Its text reads and snapshots waiting for their pictures are dropped — after the `onDeactivate` above, so what that started goes too — and enabling it again does not bring them back: their callbacks never run, so a flag set before a read stays set. Use [`host.ocr.pending`](api/ocr.md#host-ocr-pending) rather than a flag.

- **Stopped** — disabled by the host because a callback ran too long or ran its VM out of memory, or because a library whose code runs in its VM looped ([above](#limits)), until the next start or until it is ticked in the module manager. Everything said of Disabled holds, and five things differ: no code of it runs at all, its `onChange` callbacks included; an overlay that held a slot loses it without its `onDeactivate`; ticking it builds it afresh, as a reload does, so its entry file runs again and what waited for the old VM goes with it; `settings.toml` keeps it on, so the next start loads it; and a reload keeps it off.

Two consequences follow. A module that starts disabled has still run its entry file and `activate`, so top-level side effects happen anyway — speech (which is not gated on the enabled state), synthesised input, sounds, `host.settings.define`. And a module-level cache survives a disable and enable; only a reload clears it.

Transitions: *enable* / *disable* flip the flag and recompute what is held at the OS, as above. *Reload* builds a **new VM in place** from the module's folder and runs its entry again — what the old VM had registered, read, armed or started is dropped first; its timers, text reads, snapshots waiting for their pictures, and its key scope and menu flag are dropped once more after the `onDeactivate` it runs as it goes, so that none of those it started then reaches the old VM (what else that `onDeactivate` registers — a hotkey, a captured key, a listener — is not dropped a second time); every module that depends on it through `dependencies` — directly or through other modules, of any kind — is rebuilt after it, in dependency order, so they take up its new code (a module that reaches it only through `optional_dependencies` is not). A broken `module.toml` leaves the running module untouched; a broken entry leaves the module inactive until the next successful reload.

## Enable/disable interface

- **Persistence:** the enabled state of every module is stored in `settings.toml` beside the application.
- **Control surfaces:** the module manager window (tray icon), with a native checkbox per module, Settings, Reload and Uninstall; a system-wide key that reloads every module; and the CLI for search, install, list, update and uninstall. There is no IPC for enabling, disabling or reloading from a script.
- **Conflict handling:** two modules claiming the same **hotkey** are reported in an accessible dialog naming both, and the combination passes to the waiting module when the holder lets go. Captured keys are not reported: a captured key is delivered to one module only — see [`host.keys`](api/keys.md).

## Isolation tiers

- **Script (Luau) modules** — the only tier that exists: in-process, one VM each, capability-gated, toggled at runtime.
- **Native FFI / untrusted modules** — the draft's out-of-process tier. Not built: no module can load native code.
