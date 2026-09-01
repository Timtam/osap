---
title: "host.timer — waiting without blocking"
sidebar_position: 11
toc_max_heading_level: 2
---

Timers are how a module waits without blocking. `after` covers the settling time an application needs before its new state can be read back — Kontakt clicks a header toggle and re-reads the panel 250 ms later — while `every` is for something that changes with no event at all, which is why Melodyne polls its read-out strip and why a library overlay polls for its landmark. A recurring timer is also re-armed across a disable, which a self-rescheduling `after` chain is not.

**Neither is a thread.** Both are drained by the application's own loop, on the same main thread as speech, hotkeys and the arbiter, so a slow callback delays all of them: the pump logs any iteration over 250 ms, and past roughly 300 ms Windows stops waiting for the keyboard hook and delivers the key without us.

`host.epoch` is what an expensive reading should be memoized against instead of a clock. It moves whenever an OS event, a timer, or the module's own synthesised input could have changed the screen, so a cached answer is free within one pass and always re-taken in the next — which "it was fresh 50 ms ago" cannot promise.

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

Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick. The callback runs once with no arguments (only if the module is still enabled at fire time) and is then discarded. There is no returned handle and no way to cancel an individual timer.

```luau
host.timer.after(500, function()
  host.speech.output("Half a second later")
end)
```

---

## host.timer.every(ms, callback) {#host-timer-every}

**Signature:** `host.timer.every(ms: number, callback: () -> ())` → `nil`

Schedules a **recurring** callback to fire approximately every `ms` milliseconds, driven from the event-loop tick. Unlike a self-rescheduling `host.timer.after` chain, a recurring timer is **re-armed even while the owning module is disabled** (the callback is only *invoked* while enabled), so a poll resumes on re-enable instead of dying. There is no returned handle or per-timer cancel; it is released when the module is reloaded/unloaded. Use it for polling that must survive a disable/enable cycle — e.g. an overlay watching for a landmark to appear.

```luau
host.timer.every(150, function()
  -- e.g. re-check whether a library landmark is on screen
end)
```

---

## host.epoch() {#host-epoch}

**Signature:** `host.epoch() -> number`

A counter that changes whenever the world may have: an OS event dispatched into a module (hotkey, key, window activation, focus change), a timer firing, an async image result arriving, or the module itself driving input (click, move, drag, scroll, key send, typing).

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

Milliseconds since the application started, monotonic, so it cannot go backwards in the middle of a measurement.

It exists for one thing: measuring your own hot paths. Every performance question in this project up to its arrival had to be answered from Rust or from log timestamps a second apart, neither of which can say what a single focus step cost.

```luau
local t0 = host.now()
local value = readTheSlowThing()
host.log.info(("read took %d ms"):format(host.now() - t0))
```

## host.inputEpoch() {#host-inputepoch}

**Signature:** `host.inputEpoch() -> number`

A counter that turns over only when something **acted** on the screen: input this platform drove, or a window coming forward.

`host.epoch` also moves on timer ticks and on the user's own keystrokes, which is right for "re-resolve where the plug-in is" and far too eager for "what does this pixel say". A screen read costs a fixed compositor frame, so a property that only an action can change should be cached against this one instead.

```luau
-- Re-read only when something could actually have moved it.
if cachedAt ~= host.inputEpoch() then
  cached, cachedAt = host.screen.pixel(x, y), host.inputEpoch()
end
```
