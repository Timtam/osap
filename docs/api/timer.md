---
title: "host.timer — timers and change counters"
sidebar_position: 18
toc_max_heading_level: 2
---

Timers are how a module waits without blocking. `after` covers the settling time an application needs before its new state can be read back — Kontakt clicks a header toggle and re-reads the panel 250 ms later — while `every` is for something that changes with no event at all, which is why Melodyne polls its read-out strip and why a library overlay polls for its landmark. A recurring timer is also re-armed across a disable, which a self-rescheduling `after` chain is not.

**Neither is a thread.** Both are drained by the application's own loop, on the same main thread as speech, hotkeys and the arbiter, so a slow callback delays all of them: the pump logs any iteration over 250 ms, and what else it costs is platform-specific — see [A slow callback](#a-slow-callback). Nothing interrupts a callback that does not return: a loop that never ends stops the whole application.

**Resolution is the loop's tick, about 15 ms.** Timers are looked at once per pass of the loop, which runs every 15 ms. A timer fires on the first pass at or after its time, never sooner, and `every` re-arms from the moment that pass ran — so an interval rounds up to whole ticks: `every(20)` fires about every 30 ms, `every(16)` about every 30 ms rather than at 60 Hz, and nothing fires more often than once a tick. The schedule also drifts by whatever each pass was late. Within one pass the one-shot timers that are due run first, then the recurring ones, each in the order they were armed.

**Both return a token, and [`cancel`](#host-timer-cancel) takes it back.** The token is a whole number, unique for the life of the application and never reused. An `after` runs once unless it is cancelled first; an `every` runs until it is cancelled or its module is reloaded or removed. While the module is disabled an `every` is kept and goes on being re-armed, with its callback skipped; an `after` that comes due then is discarded without being called. Throwing the token away is fine: nothing is stopped when it is collected.

`host.epoch` is what an expensive reading should be memoized against instead of a clock. It moves whenever an OS event, a one-shot `after` coming due, or the module's own synthesised input could have changed the screen, so a cached answer is free within one dispatch and re-taken after the next such event — which "it was fresh 50 ms ago" cannot promise. An `every` tick does not move it, so a poll is handed what was cached before the tick until something else moves it.

Where a wait is for a *value to change* rather than for a length of time, a guessed delay is wrong in both directions. An overlay has [`O:watch`](./overlay.md#o-watch) for it, which exists only on an overlay and runs only while that overlay is active. A module that is not an overlay — a game module on a shared runtime, say — has no change wait: it polls with [`every`](#host-timer-every), keeps one asynchronous read in flight (the example under [`matchCellsAsync`](./screen.md#host-screen-matchcellsasync)) and compares each reading with the last, or, after a press or an input it caused, reads in a burst until a reading differs or a deadline passes: either asking again straight from each asynchronous read's callback, as the menu example under [`host.gamepad.on`](./gamepad.md#host-gamepad-on) does, or with a few `after`s at growing delays.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["timer"]
```

See [what that list is and is not](./index.md#capabilities).

## host.timer.after(ms, callback) {#host-timer-after}

**Signature:** `host.timer.after(ms: number, callback: () -> ())` → `number`

Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick — at the earliest on the first tick at or after that time, so `after(1)` and `after(10)` both wait for the next tick, and an `after(0)` armed from inside a timer callback waits for the next tick too. The callback runs once with no arguments (only if the module is still enabled at fire time) and is then discarded. Returns the timer's token, for [`host.timer.cancel`](#host-timer-cancel). `ms` must be a number of at least 0 (a fraction is cut to the whole number below); a negative one raises. The callback runs with the priority of the dispatch that armed it — a re-read armed from a key press is still somebody waiting — which decides where a [`host.ocr.read`](./ocr.md#host-ocr-read) it asks for waits; an `every` callback always runs as background work.

Arming costs a registry slot and a list entry, and nothing else; the list is looked at once per tick.

```luau
local pending = host.timer.after(500, function()
  host.speech.output("Half a second later")
end)
-- Changed our mind before it came due:
host.timer.cancel(pending)
```

### Windows

Drained by the 15 ms tick of the module manager's window (a wxWidgets timer), and in a headless run by the loop's own 15 ms wait for messages.

### macOS

Drained by the same 15 ms wxWidgets tick; in a headless run by a 15 ms turn of the CoreFoundation run loop.

---

## host.timer.every(ms, callback) {#host-timer-every}

**Signature:** `host.timer.every(ms: number, callback: () -> ())` → `number`

Schedules a **recurring** callback to fire approximately every `ms` milliseconds, driven from the event-loop tick, and rounded up to whole ticks as described at the top of this page. `ms` must be a number of at least 0 (a fraction is cut to the whole number below, and an interval below 1 counts as 1); a negative one raises. Unlike a self-rescheduling `host.timer.after` chain, a recurring timer is **re-armed even while the owning module is disabled** (the callback is only *invoked* while enabled), so a poll resumes on re-enable instead of dying. Returns the timer's token; it runs until [`host.timer.cancel`](#host-timer-cancel) is called with it — from inside its own callback too — or until the module is reloaded or removed.

The timer belongs to the module whose VM runs the call. A `code_module` runtime that calls `every` at its top level therefore arms one poll in its own VM and one in the VM of every module that depends on it — directly, or through another `code_module` — each owned, and disabled, with the module whose VM it is (see [`host.require`](./require.md#host-require)). Each of those VMs gets its own token and can cancel its own poll, and only that one.

Arming costs a registry slot and a list entry, as for `after`; the list is looked at once per tick, and every tick the timer is due its callback runs on the main thread, so what it costs is what the callback does.

```luau
-- A poll that only works while the game is in front, and never has two searches in flight.
-- Besides "timer", this needs "window", "screen", "path" and "log" in the manifest.
local GAME = { title = { contains = "My Game" } }
local busy = false
host.timer.every(150, function()
  if busy then return end
  local win = host.window.active()
  if not win or not host.window.test(GAME, win) then return end
  local b = win.client
  busy = true
  host.screen.imageSearchAsync(host.path("images/menu-cursor.png"),
    { region = { b.x, b.y, b.x + 400, b.y + 300 }, tolerance = 8 },
    function(hit)
      busy = false
      if hit then host.log.info(("cursor at %d,%d"):format(hit.x, hit.y)) end
    end)
end)
```

### Windows

The same tick as [`after`](#host-timer-after): 15 ms, from the manager window's wxWidgets timer or the headless loop's wait.

### macOS

The same tick as [`after`](#host-timer-after): 15 ms, from the wxWidgets timer or, headless, a turn of the CoreFoundation run loop.

---

## host.timer.cancel(token) {#host-timer-cancel}

**Signature:** `host.timer.cancel(token: number?) -> boolean`

Stops the pending timer `token` and returns `true`, or returns `false` when there was nothing of yours to stop: `nil`, anything that is not a whole number, a token that was never handed out, a one-shot timer that has already fired, a timer that was already cancelled, and a timer that belongs to another module. It never raises, so a module can cancel "whatever poll is running" without testing for `nil` first.

- **Only your own.** A timer can be cancelled only from the module that owns it — the module whose VM armed it, which for a `code_module` runtime's code is the dependent it runs in. A token is a small number, and one module must not be able to stop another's poll by guessing it.
- **From inside a callback.** An `every` that cancels itself returns `true` and does not fire again. An `after` that cancels itself gets `false`: it has already fired.
- **In the same tick.** A callback that cancels another timer due in the same pass of the loop stops it before it runs, one-shot or recurring.
- **While disabled.** Cancelling works while the module is disabled, for both kinds.

A linear scan over the pending timers, on the main thread.

```luau
-- Read a game's menu only while the game is in front: the poll starts when the game is
-- (already) in front and cancels itself as soon as it is not. Needs "window" and "timer".
local GAME = { title = { contains = "My Game" } }
local poll
local function readMenu()
  -- one detection read of the menu
end
host.window.onTrigger(GAME, { initial = true }, function()
  if poll then return end
  poll = host.timer.every(100, function()
    local win = host.window.active()
    if not (win and host.window.test(GAME, win)) then
      host.timer.cancel(poll); poll = nil; return
    end
    readMenu()
  end)
end)
```

`initial = true` is what starts the poll for a game that is already in front when the module loads or is enabled — see [`onTrigger`](./window.md#host-window-ontrigger). After the first switch away the poll stops, and the next activation of the game starts it again.

### Windows

No operating-system call: the timer is taken out of the host's own list.

### macOS

No operating-system call; the same code as on Windows.

## A slow callback {#a-slow-callback}

What a callback that takes too long costs beyond delaying everything else on the loop depends on the platform.

### Windows

Captured keys and hotkeys wait: the keyboard hook swallows them on its own thread at once, and their callbacks run when the loop is free again, late by as long as it was busy. The user's typing elsewhere is not delayed, and a captured key does not slip through to the application. What the loop's stalls no longer reach is the hook itself: Windows documents that a low-level hook which keeps timing out can be removed without notice, and since its thread does nothing but answer it, that takes a machine too loaded to schedule it. The host never checks the hook again once it is installed, so after such a removal no key would be captured for the rest of the session; a hotkey press that then arrives through `RegisterHotKey` alone is written to the log.

### macOS

The system switches off an event tap whose thread stops answering, and keys go uncaptured until the host's watchdog notices and switches it back on; the watchdog logs each time it does.

---

## host.epoch() {#host-epoch}

**Signature:** `host.epoch() -> number`

A counter that changes whenever the world may have: an OS event dispatched into a module (hotkey, key, window activation, focus change, controller event), a one-shot [`after`](#host-timer-after) coming due, an async image result, a [text read](./ocr.md#host-ocr-read)'s readings or a [`snapshotAsync`](./screen.md#host-screen-snapshotasync) answer arriving (once for all that arrive together), [`host.window.focus`](./window.md#host-window-focus), or the module itself driving input (click, move, drag, scroll, key send, typing). A [`host.timer.every`](#host-timer-every) tick does not move it, and neither does [`host.input.post`](./input.md#host-input-post).

Memoize an expensive observation against it, so repeats within one dispatch are free while a genuinely new situation is always re-observed:

```luau
local cachedEpoch, cached = -1, nil
local function expensiveThing()
    local e = host.epoch()
    if e ~= cachedEpoch then
        cached, cachedEpoch = reallyWorkItOut(), e
    end
    return cached
end
```

Deliberately **not** time-based, and it does not advance on an idle tick. A stale answer here means acting on the wrong screen position, and "it was fresh 50 ms ago" is not a safety property.

### Windows

A counter in the host; reading it makes no system call.

### macOS

The same counter, moved by the same events as on Windows.

## host.now() {#host-now}

**Signature:** `host.now() -> number`

Milliseconds since the application started, monotonic, so it cannot go backwards in the middle of a measurement. It is one clock for the whole application: every module counts from the same moment, and so does the `time` a [game-controller event](gamepad#host-gamepad-on) carries, so the two can be compared.

It exists for one thing: measuring your own hot paths. Every performance question in this project up to its arrival had to be answered from Rust or from log timestamps a second apart, neither of which can say what a single focus step cost.

```luau
local t0 = host.now()
local value = readTheSlowThing()
host.log.info(("read took %d ms"):format(host.now() - t0))
```

### Windows

Rust's monotonic clock (`std::time::Instant`), read in the host; no module-visible system call.

### macOS

The same clock as on Windows, Rust's `std::time::Instant`.

## host.inputEpoch() {#host-inputepoch}

**Signature:** `host.inputEpoch() -> number`

A counter that turns over only when something **acted** on the screen: input this platform drove, a game-controller press, release or combination delivered to a [listener](./gamepad.md#host-gamepad-on), a window coming forward, or a window already in front being reported to an [`onTrigger { initial = true }`](./window.md#host-window-ontrigger) callback.

`host.epoch` also moves when a one-shot timer comes due and on the user's own keystrokes, which is right for "re-resolve where the plug-in is" and far too eager for "what does this pixel say". A screen read costs a fixed compositor frame, so a property that only an action can change should be cached against this one instead.

```luau
-- Re-read only when something could actually have moved it.
if cachedAt ~= host.inputEpoch() then
  cached, cachedAt = host.screen.pixel(x, y), host.inputEpoch()
end
```

### Windows

A counter in the host; reading it makes no system call.

### macOS

The same counter, moved by the same events as on Windows.
