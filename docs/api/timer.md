---
title: "host.timer — timers and change counters"
sidebar_position: 18
toc_max_heading_level: 2
---

Timers are how a module waits without blocking. `after` covers the settling time an application needs before its new state can be read back — Kontakt clicks a header toggle and re-reads the panel 250 ms later — while `every` is for something that changes with no event at all, which is why Melodyne polls its read-out strip and why a library overlay polls for its landmark. A recurring timer is also re-armed across a disable, which a self-rescheduling `after` chain is not.

**Neither is a thread.** Both are drained by the application's own loop, on the same main thread as speech, hotkeys and the arbiter, so a slow callback delays all of them: the pump logs any iteration over 250 ms, and what else it costs is platform-specific — see [A slow callback](#a-slow-callback). Nothing interrupts a callback that does not return: a loop that never ends stops the whole application.

**Resolution is the loop's tick, about 15 ms.** Timers are looked at once per pass of the loop, which runs every 15 ms. A timer fires on the first pass at or after its time, never sooner, and `every` re-arms from the moment that pass ran — so an interval rounds up to whole ticks: `every(20)` fires about every 30 ms, `every(16)` about every 30 ms rather than at 60 Hz, and nothing fires more often than once a tick. The schedule also drifts by whatever each pass was late.

**Neither returns anything.** There is no handle and no way to cancel a timer: an `after` runs once, an `every` runs until the module is reloaded or removed. While the module is disabled an `every` is kept and goes on being re-armed, with its callback skipped; an `after` that comes due then is discarded without being called.

`host.epoch` is what an expensive reading should be memoized against instead of a clock. It moves whenever an OS event, a one-shot `after` coming due, or the module's own synthesised input could have changed the screen, so a cached answer is free within one dispatch and re-taken after the next such event — which "it was fresh 50 ms ago" cannot promise. An `every` tick does not move it, so a poll is handed what was cached before the tick until something else moves it.

Where a wait is for a *value to change* rather than for a length of time, use the overlay's `O:watch` instead: a guessed delay is wrong in both directions.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["timer"]
```

See [what that list is and is not](./index.md#capabilities).

## host.timer.after(ms, callback) {#host-timer-after}

**Signature:** `host.timer.after(ms: number, callback: () -> ())` → `nil`

Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick — at the earliest on the first tick at or after that time, so `after(1)` and `after(10)` both wait for the next tick. The callback runs once with no arguments (only if the module is still enabled at fire time) and is then discarded. There is no returned handle and no way to cancel an individual timer: to call one off, have the callback check a flag of your own. `ms` must be a number of at least 0 (a fraction is cut to the whole number below); a negative one raises.

```luau
host.timer.after(500, function()
  host.speech.output("Half a second later")
end)
```

---

## host.timer.every(ms, callback) {#host-timer-every}

**Signature:** `host.timer.every(ms: number, callback: () -> ())` → `nil`

Schedules a **recurring** callback to fire approximately every `ms` milliseconds, driven from the event-loop tick, and rounded up to whole ticks as described at the top of this page (`ms` below 1 counts as 1). Unlike a self-rescheduling `host.timer.after` chain, a recurring timer is **re-armed even while the owning module is disabled** (the callback is only *invoked* while enabled), so a poll resumes on re-enable instead of dying. There is no returned handle or per-timer cancel; it runs until the module is reloaded or removed. **The only way to stop a poll is to return early**: gate the callback on the condition it is for — the window being in front, a flag of your own — and let it return at once when that does not hold.

The timer belongs to the module whose VM runs the call. A `code_module` runtime that calls `every` at its top level therefore arms one poll in its own VM and one in the VM of every module that depends on it — directly, or through another `code_module` — each owned, and disabled, with the module whose VM it is (see [`host.require`](./require.md#host-require)).

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

## A slow callback {#a-slow-callback}

What a callback that takes too long costs beyond delaying everything else on the loop depends on the platform.

### Windows

Past roughly 300 ms Windows stops waiting for the keyboard hook and delivers the key without us. Windows also documents that a low-level keyboard hook which keeps timing out can be removed without notice; the host installs its hook once and never checks it again, so after such a removal no key would be captured for the rest of the session and nothing would say why. That has not been observed here, but it is one more reason to keep every callback short.

---

## host.epoch() {#host-epoch}

**Signature:** `host.epoch() -> number`

A counter that changes whenever the world may have: an OS event dispatched into a module (hotkey, key, window activation, focus change, controller event), a one-shot [`after`](#host-timer-after) coming due, an async image result arriving, [`host.window.focus`](./window.md#host-window-focus), or the module itself driving input (click, move, drag, scroll, key send, typing). A [`host.timer.every`](#host-timer-every) tick does not move it, and neither does [`host.input.post`](./input.md#host-input-post).

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

## host.now() {#host-now}

**Signature:** `host.now() -> number`

Milliseconds since the application started, monotonic, so it cannot go backwards in the middle of a measurement. It is one clock for the whole application: every module counts from the same moment, and so does the `time` a [game-controller event](gamepad#host-gamepad-on) carries, so the two can be compared.

It exists for one thing: measuring your own hot paths. Every performance question in this project up to its arrival had to be answered from Rust or from log timestamps a second apart, neither of which can say what a single focus step cost.

```luau
local t0 = host.now()
local value = readTheSlowThing()
host.log.info(("read took %d ms"):format(host.now() - t0))
```

## host.inputEpoch() {#host-inputepoch}

**Signature:** `host.inputEpoch() -> number`

A counter that turns over only when something **acted** on the screen: input this platform drove, or a window coming forward.

`host.epoch` also moves when a one-shot timer comes due and on the user's own keystrokes, which is right for "re-resolve where the plug-in is" and far too eager for "what does this pixel say". A screen read costs a fixed compositor frame, so a property that only an action can change should be cached against this one instead.

```luau
-- Re-read only when something could actually have moved it.
if cachedAt ~= host.inputEpoch() then
  cached, cachedAt = host.screen.pixel(x, y), host.inputEpoch()
end
```
