---
title: "host.gamepad — watching game controllers"
sidebar_position: 3.5
toc_max_heading_level: 2
---

A game played with a controller has no keyboard focus to hang an overlay on and usually no accessibility tree either, so the press itself is the only event there is. This namespace hands that press to a module without taking anything from the game: which button went down on which pad, which buttons are held together, where the sticks are, and when a pad arrives or leaves. A module reading a game's menu listens for the D-pad, looks at the screen the moment after, and says what the cursor moved to.

**Nothing here looks at which window is in front.** A listener gets every matching event the platform delivers while its module is enabled, whatever application has the focus; whether the platform delivers input while this application is in the background is in the platform sections of [`on`](#host-gamepad-on). Deciding that a press is meant for your game is the module's job: check `host.window.active()` in the callback, or register the listener when your game comes forward and remove it with [`off`](#host-gamepad-off) when it leaves.

**Which pads are seen depends on the platform**: see the platform sections of [`list`](#host-gamepad-list).

**It observes and nothing more.** Every press reaches the game as well as your module, and nothing here can stop that — on Windows that would take a filter driver and a virtual controller, on macOS seizing the device away from the game, and neither fits an application that installs by unzipping. There is therefore no claim and no conflict the way `host.hotkey` has them: every enabled module with a matching listener gets every event. A command of your own can be a keyboard hotkey, through `host.hotkey`, or a [button combination](#button-combinations) on the pad — which reaches the game too, and has a hold time for exactly that reason.

**Names are positional.** `south` is the bottom face button whatever is printed on it — A on an Xbox pad, Cross on a PlayStation pad, B on a Nintendo pad — so a module written for "the confirm button" works on every family the platform reports (on Windows, that is XInput pads only). What is printed on the pad arrives as `label`, in English, for speech. The full list, the labels and a table for porting from SDL, pygame and XInput are [at the end](#button-and-axis-names).

**Values.** Sticks run from -1 to 1 with `y` positive **down**, as in SDL and on the screen; triggers run from 0 to 1. `state()` gives the raw values. Listeners get dead-zoned values instead — radially over the two halves of a stick, with Microsoft's defaults (0.24 left, 0.27 right, 0.12 triggers) — and the application also turns each stick direction and each trigger into a button of its own (`left_stick_down`, `right_trigger`), pressed at half deflection and released below 0.35 (triggers: 0.12 and 0.08), so a menu can be walked with the stick the same way as with the D-pad.

**The event says when to look, not what the game drew.** Each event carries `time` — milliseconds on the `host.now()` clock when the application detected the press — and `age`, how long ago that was when your callback ran. The game draws the moved cursor one or more of its own frames *after* the press, so a screen read made at once usually sees the old cursor. Search in a short burst until the picture has changed, as the example under [`on`](#host-gamepad-on) does. For the same reason a delivered press turns over `host.inputEpoch()` — the game's screen does change — but it turns over at the press, before the game has redrawn: do not cache a reading of the game's screen against `inputEpoch` alone.

**Where it runs.** Every function here is called on the application's event loop, like the rest of `host`, and every callback runs there too — never on the thread or queue that reads the pads. A callback that takes long holds up everything else the loop does, as any callback does. A [`host.ocr.read`](./ocr.md#host-ocr-read) asked for from a callback of this namespace — `down`, `up`, `axis`, `connected`, `disconnected` and every combination, one held for its `holdMs` included — counts as one somebody is waiting for and goes ahead of reads asked for by polls; so does one asked for from a [`host.timer.after`](./timer.md#host-timer-after) or image-search callback armed there, which inherits it (see **Order and priority** under [`host.ocr.read`](./ocr.md#host-ocr-read)). The one exception is the `connected` a listener receives for a pad that was already there or arrived while its module was disabled (`synthetic = true`): a read asked for from it waits with the polls.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["gamepad"]
```

See [what that list is and is not](./index.md#capabilities).

## host.gamepad.list() {#host-gamepad-list}

**Signature:** `host.gamepad.list()` → `{ PadInfo }`

The pads connected now, sorted by `index`; an empty table when there are none.

```
PadInfo = { index = 1, id = "xinput:0", name = "Xbox controller",
            family = "xbox" | "playstation" | "nintendo" | "generic",
            source = "xinput" | "gamecontroller", mapped = true,
            vendor = number?, product = number?,
            buttons = { "south", "east", ... }, axes = { "left_x", ... } }
```

`index` runs from 1 to 8, lowest free first, and stays with a pad while it is connected; a pad that is unplugged frees its number for the next one, and a ninth pad is not watched (the log says so once). `id` says where the platform sees this connection, not which device it is — see the platform notes below — so do not use it to recognise a pad you have seen before. `buttons` lists every button the pad can report, the derived stick and trigger buttons included. `mapped` is `true` for every pad a platform reports today.

**The first call** of `list`, [`state`](#host-gamepad-state) or [`on`](#host-gamepad-on) anywhere in the application starts watching the pads, and **raises** when that fails — on a platform without controller support (`"game controllers are not supported on linux"`), or when the watcher cannot start; every later call raises with the same reason. Apart from that, `list` does not raise.

```luau
for _, pad in ipairs(host.gamepad.list()) do
  host.speech.output(string.format("Pad %d: %s", pad.index, pad.name))
end
```

### Windows

Xbox-type pads, through XInput, which allows at most four. Every XInput pad is reported the same way, because XInput gives no name: `name` is `"Xbox controller"`, `family` is `xbox` and the labels are Xbox ones — a third-party pad in XInput mode, an 8BitDo for instance, is announced with A, B, X and Y whatever is printed on it. Their `id` is XInput's slot, `xinput:0` to `xinput:3`: a different pad plugged into the same slot gets the same `id`. `guide` is in `buttons` only when the system's XInput has the extended entry point that reports it. `vendor` and `product` are the pad's USB ids where XInput's extended capabilities call answers for the slot, and `nil` otherwise. PlayStation, Nintendo and other controllers are not listed on Windows unless they present themselves through XInput. Where no XInput DLL can be loaded at all, watching still starts — `status().watcher` is `"running"` and `status().xinput` says `unavailable` — and `list()` is always empty.

The first call blocks the event loop until the first reading of the four slots is done, or 250 ms at most, so a pad that is plugged in is normally already listed. After that, `list()` answers from what the pad thread last read, except when nothing keeps it reading — no enabled module listens for `down`, `up`, `axis` or `chord`, and no `list()` or `state()` came in the last five seconds: then it asks for a fresh reading and blocks the loop until it comes, up to 50 ms, and the pads stay read for five seconds afterwards. While they are read, a connected pad is read every 4 ms, and a pad being plugged in is found when Windows announces it, or at the latest by a look at the empty slots every two seconds.

### macOS

The controllers the GameController framework supports, with two sticks and two triggers; `family` comes from the controller's own description, `name` is the name the framework gives it, and `vendor` and `product` are always `nil`. Their `id` is `gc:` and a number counted up for every connection, so the same pad unplugged and plugged back in gets a new one. The first call does not wait: the framework finds controllers on its own schedule, so a pad attached at launch can appear a moment after the first call — listen for [`connected`](#host-gamepad-on) as well, which also announces the pads already there. The log records the application's bundle identifier when watching starts, and warns when there is none: run the application from its `.app`.

## host.gamepad.state(pad) {#host-gamepad-state}

**Signature:** `host.gamepad.state(pad: number)` → `PadState?`

What pad number `pad` is doing now, or `nil` when there is no pad with that index. A `pad` that is not a whole number from 1 to 8 gives `nil` too. Values are raw — no dead zone — so a module that wants its own curve has the original numbers.

```
PadState = { pad = 1, buttons = { south = false, dpad_down = true, ... },
             axes = { left_x = 0.01, left_y = -0.98, ... }, time = 81234 }
```

`buttons` names every button the pad has, `false` included, so asking about a released button gives `false` rather than `nil`. A button already held when the pad was connected is `true` here, although no `down` was ever sent for it. `time` is when the pad last reported anything.

**Raises** when `pad` is not a number — `nil`, a table, a string that does not read as a number (`"2"` is taken as 2) — and, like `list`, when watching the pads cannot start; this call starts it too.

A long press of one button — a [combination](#button-combinations) has `holdMs` for this, a single button does not: act only if `start` is still down 800 ms later.

```luau
host.gamepad.on("down", function(e)
  host.timer.after(800, function()
    local s = host.gamepad.state(e.pad)
    if s and s.buttons.start then
      host.speech.output(currentMenuText())
    end
  end)
end, { buttons = { "start" } })
```

### Windows

While the pad is read every 4 ms — an enabled module listens for `down`, `up`, `axis` or `chord`, or a `list()` or `state()` came in the last five seconds — `state()` answers at once from the last reading, the same one those listeners get. Otherwise — no listener, or only `connected` and `disconnected` ones, which are served by a look every two seconds — it asks for a fresh reading, blocks the event loop until it comes (up to 50 ms), and keeps the pad read for five seconds, so calling it repeatedly does not start and stop the reading each time.

### macOS

Answered at once from the last values GameController delivered, which it does whenever anything on the pad changes.

## host.gamepad.on(event, callback, opts?) {#host-gamepad-on}

**Signature:** `host.gamepad.on(event: "down" | "up" | "axis" | "chord" | "connected" | "disconnected", callback: (e: PadEvent) -> (), opts: { pad: number?, buttons: {string}?, axes: {string}?, deadzone: number?, step: number?, maxAge: number?, exact: boolean?, holdMs: number? }?)` → `token: number`

Calls `callback` for every matching event while the module is enabled, until [`off`](#host-gamepad-off) or until the module is reloaded. While the module is disabled the listener is kept and not called. Returns a token for `off`: a whole number, counted up across the application. `"chord"` is a combination of buttons held together, with rules of its own under [Button combinations](#button-combinations).

```
down / up = { type = "down", pad = 1, button = "dpad_down", label = "Down",
              time = 81234, age = 9, synthetic = false }
axis      = { type = "axis", pad = 1, axis = "left_y", value = -0.73, raw = -0.80,
              time = 81240, age = 3, synthetic = false }
chord     = { type = "chord", pad = 1, buttons = { "left_shoulder", "right_shoulder" },
              labels = { "LB", "RB" }, time = 81234, age = 4, synthetic = false }
connected / disconnected = { type = "connected", pad = 1, info = PadInfo,
                             time = 80012, age = 0, synthetic = false }
```

The options, each for the events it makes sense for:

| Option | Events | Meaning |
|---|---|---|
| `pad` | all | Only this pad index: a whole number from 1 to 8. |
| `buttons` | `down`, `up`, `chord` | For `down` and `up`: only these buttons, by name. For `chord`: the combination, which is required — at least two names, none twice. |
| `axes` | `axis` | Only these axes, by name. |
| `deadzone` | `axis` | Replaces the default dead zone for every axis of this listener; `0` switches it off. At least 0 and below 1. |
| `step` | `axis` | Deliver a value only when it moved at least this far since the last one this listener got. From 0 to 2; default `0.05`. Coming to rest (`0`) and full deflection are always delivered. |
| `maxAge` | `down`, `up`, `chord` | Whole milliseconds, 1 or more. A press older than this when the application gets to it is not delivered — a press acted on seconds late reads the wrong screen. Default `1000`. |
| `exact` | `chord` | `true`: the combination counts only while no other button is held on the pad. Default `false`. |
| `holdMs` | `chord` | Whole milliseconds, 0 or more, the whole combination has to stay held before it fires. Default `0`, at the press. |

`maxAge` and `holdMs` count whole milliseconds: a fraction is cut off (`holdMs = 0.5` fires at the press, `maxAge = 1.9` is 1), and a value above 4294967295 (about 49.7 days, `math.huge` included) counts as 4294967295. `maxAge` below 1 raises instead of being cut to 0, because every press is some time old when it is delivered, so 0 would drop them all.

**What raises**, at the call, so a listener that could never fire is never registered: an `event` that is not one of the six; a `callback` that is not a function; `opts` that is neither a table nor `nil`; an option that is not one of this event's (`maxAge` on `axis`, `deadzone` on `down`, a misspelt name); an option name that is not a string; `pad` that is not a whole number from 1 to 8; `buttons` or `axes` that is not a list of names, an empty list, or an unknown name among them — label spellings such as `"A"`, `"Cross"` or `"LB"` are not names; `deadzone` below 0 or from 1 up; `step` outside 0 to 2; `maxAge` below 1, `holdMs` negative, or either not a number; `exact` that is not `true` or `false`; the [combination rules](#button-combinations) for `buttons` on `chord`; and, as for `list`, watching the pads failing to start. Nothing else raises, and nothing about the pads raises later: a listener for a pad that is not there simply waits.

**Cost.** `on` checks its options and registers the listener on the event loop, and never waits for a press or for a pad to appear. The first call of `list`, `state` or `on` in the application also starts watching the pads; what that and every later call cost is in the platform sections below.

An `up` reaches a listener only if that listener saw its `down`: the release of a press that was dropped for `maxAge`, that happened before the listener was registered, or that happened while the module was disabled is not delivered, so a module never hears a release of something it never saw pressed. A button that was already held when the pad was connected — or, on Windows, when the pads started being read again after nobody listened — gives neither a `down` nor an `up`. An `up` is never dropped for its own age — the module that got the press needs the release. `axis` values are never dropped for age either: each one is where the stick is now.

`synthetic` is `true` for events the application made up: the `up` sent for every button still held when a pad disappears, before its `disconnected`, and the `connected` a listener receives for each pad that was already there when it was registered, or that arrived while its module was disabled. A listener hears `connected` once per pad per connection, whichever of the two told it first.

**Order.** Each pass of the application's loop takes what the pads reported since the last one and delivers it in the order it was detected — presses, releases and connections in order, then the latest value of each axis that moved, by pad and axis — and each event to the listeners in the order `on` was called, across modules. `axis` values therefore come at most once per pass per axis. The two halves of a stick share one dead zone, so moving one can change the other's value — pushing `left_x` back to the middle while `left_y` rests just inside the dead zone takes `left_y` to 0 as well — and a listener gets an event for each half whose value changed, the other half included. In a pass, the pads come after hotkeys, keys and windows coming forward, and before a change of focus is reported; `connected` replays and held combinations come later in the same pass, after the timers.

**`inputEpoch`** turns over when a `down`, `up` or `chord` is delivered to at least one listener — once for everything delivered together. Axis values, connections, and presses no listener received turn over only `host.epoch`, and only when some callback ran.

Reading a menu: after each press, search in a burst until the cursor is somewhere new, for at most 150 ms after the press. `lastHit` is where the cursor was found last time; anything found somewhere else is new.

```luau
local lastHit = nil

local function lookAfter(e, before, onNew)
  local deadline = host.now() + 150 - e.age
  local function step()
    host.screen.imageSearchAsync(CURSOR_TEMPLATES, { region = MENU_REGION, tolerance = 8 }, function(hit)
      if hit and (not before or hit.y ~= before.y) then
        onNew(hit)
      elseif host.now() < deadline then
        step() -- straight back to the worker, without waiting a tick
      end
    end)
  end
  step()
end

local token = host.gamepad.on("down", function(e)
  lookAfter(e, lastHit, function(hit)
    lastHit = hit
    host.speech.output(itemAt(hit.y))
  end)
end, { buttons = { "dpad_up", "dpad_down", "left_stick_up", "left_stick_down" } })
```

A help bubble that appears after a press is the same call with `before = nil`: anything found is new.

### Windows

**Detection.** While a `down`, `up`, `axis` or `chord` listener of an enabled module exists (or for five seconds after a `list()` or `state()`), connected XInput pads are read every 4 ms, on a thread of the application's own, with a high-resolution timer. Where Windows gives no high-resolution timer — before Windows 10 version 1803 — or refuses to arm it, they are read about every 15.6 ms instead, the system's tick; `status().timer` says which, and the log says so once. `time` is the reading that saw the change, so it comes up to one reading period after the press itself, and `age` counts from that reading; a press shorter than the period can be missed altogether. With only `connected` and `disconnected` listeners the four slots are looked at every two seconds, and with no listener nothing is read at all. While anybody listens, a new pad is also looked for as soon as Windows announces a HID device arriving. The pads are read the same way whichever application is in front.

**Delivery.** In the application as it normally runs, events wait for the next 15 ms tick of the module manager's window (a wxWidgets timer, running whether the window is shown or not), so a callback runs up to about 15 ms after detection when the loop is not busy with something else. In a headless run a press, a release or a connection wakes the loop at once; axis values alone do not, and wait for the loop's next 15 ms turn.

**Cost of the call.** The first call of `list`, `state` or `on` in the application blocks the event loop until the first reading of the four slots is done, 250 ms at most (see [`list`](#host-gamepad-list)). Every other `on` costs microseconds and never blocks: when the new listener changes how hard the pads are read, the pad thread is told with a posted message.

Windows opens Xbox Game Bar on the Guide button by default (Settings, Gaming), which takes the focus from the game.

### macOS

The first call does not wait for a controller (see [`list`](#host-gamepad-list)). The `on` that brings the first `down`, `up`, `axis` or `chord` listener of an enabled module also asks macOS, on the event loop, for the activity described below.

GameController delivers every change as it happens, on a queue of its own; `time` is when that delivery came. The event loop picks it up on its next turn — the manager window's 15 ms wxWidgets tick, or in a headless run a 15 ms turn of the CoreFoundation run loop — so at most about 15 ms later when the loop is not busy with something else. Input reaches an application that is not in front only after it asks, with `shouldMonitorBackgroundEvents`; the application asks when the first controller connects and logs what macOS answered. While a `down`, `up`, `axis` or `chord` listener exists it also holds a latency-critical `NSProcessInfo` activity, which is the request macOS offers against App Nap. `guide` is listed when the controller reports a Home button, but macOS may keep that button for itself.

## Button combinations {#button-combinations}

A combination fires once when every button of a set is held together on one pad. `host.gamepad.on("chord", callback, { buttons = { … } })` calls `callback` at the press that completes the set, whatever order the buttons went down in, and again only after a button of the set has been let go and the set is completed anew.

```luau
-- LB + RB (L1 + R1 on a PlayStation pad) reads out the whole menu while the game is in front.
-- Needs "gamepad", "window", "ocr" and "speech" in the manifest.
local GAME = { app = { name = "mygame" } }
local MENU = { 0.25, 0.20, 0.75, 0.85 } -- where the game draws its menu: fractions of its window

host.gamepad.on("chord", function(e)
  local w = host.window.active()
  if not (w and host.window.test(GAME, w)) then return end
  host.ocr.read({ window = w, fraction = MENU }, { key = "menu" }, function(r)
    if r.status == "text" then
      host.speech.output(r.text)
    end
  end)
end, { buttons = { "left_shoulder", "right_shoulder" } })
```

The rules:

- **Held is what counts, not the order of the presses.** A button of the set that was already down before — when the pad was plugged in, when the listener was registered, while the module was disabled — counts as held, although it never gave a `down` of its own. Only a press of a button *in* the set completes it: pressing another button while the whole set is already held does not fire.
- **Holding it does not repeat it.** Further presses while the set stays held do nothing. Letting go of any button of the set arms it again, and pressing that button again fires again; releasing another button does neither.
- **One pad.** All of the set has to be held on the same pad; `pad` restricts the listener to one index. A pad that goes away is forgotten, and the next pad given its index starts armed.
- **The presses are still presses.** A combination consumes, replaces and delays nothing: `down` and `up` listeners get each of its presses and releases as usual, and so does the game. The chord is delivered in the same pass as the `down` of its completing press, at that event's place in the [order](#host-gamepad-on) — to the listeners in the order `on` was called, so a `down` listener registered before the chord listener hears the press first. Two listeners for the same buttons, in one module or two, both fire; so does a listener for a larger set that contains a smaller one, when its own last button goes down.
- **Several buttons in one reading.** When one reading of the pad (see the platform sections) finds several buttons gone down, their `down` events come in the order of the [names table](#button-and-axis-names), each judged by everything held after that reading, so the chord comes with the first of them that is in the set.
- **`exact = true`** fires only while no other button is held on the pad — at the completing press, and for a hold also when the hold is judged (see `holdMs`). Derived buttons count: a stick pushed past half way or a trigger pulled past its threshold is a button held. A completion refused for this does not fire later, when the other button is let go; let go of a button of the set and press it again.
- **`holdMs`** fires only after the whole set has stayed held that long after the completing press, by the times the pad reported: a button of the set let go before then cancels it, and so does disabling the module. The hold is judged on the application's tick, after that pass's presses and releases have been delivered, so the chord normally comes up to about 15 ms after `holdMs` has passed, and later when the loop is busy; `time` is still the completing press and `age` is at least `holdMs`. A button of the set let go *after* `holdMs` had passed, but before the tick got to the hold, does not cancel it: the chord comes in the same pass as that button's `up`, after it, and `exact` then refuses it only if a button outside the set was held right after that release. Nor does the tick fire a hold ahead of a release it has not delivered yet: while a release of a button of the set is still waiting to be delivered — the loop was busy — the hold waits for it, and that release's time decides as above. Use it for a combination the game also answers to — the game sees the quick press, and only a long one reaches your module as well.
- **`maxAge`** applies to the completing press, as for `down`: one that is older when the application gets to it is not delivered, and does not fire later at another press while the set stays held. For a hold, the chord is also dropped when the hold ended more than `maxAge` before the tick that would have fired it.
- **A disabled module** gets no combination, and a hold not fired yet is cancelled the moment the module is disabled. Enabled again, it gets the next completion — whatever was pressed and let go meanwhile — but not one that happened while it was disabled.
- **Derived buttons** can be part of a set: `{ "left_trigger", "right_trigger" }` is both triggers pulled past their threshold. Two directions of one stick axis — `left_stick_up` with `left_stick_down`, `left_stick_left` with `left_stick_right`, and the same pairs of the right stick — are never held together, so a set with both of a pair raises; a diagonal, such as `left_stick_up` with `left_stick_left`, is two held buttons.
- **A button the pad does not have** makes a set that never completes on that pad, and nothing raises, because another pad may have it: `buttons` in the pad's [`PadInfo`](#host-gamepad-list) lists what it can report. On Windows that leaves out `misc1`, the paddles, `touchpad` and every `buttonN`, and `guide` where XInput's extended entry point is missing.

`buttons` in the event is the set in the order the listener named it; `labels` is what is printed on those buttons on this pad, in the same order. `time` is when the completing press was detected — the same `time` as its `down` — and `age` counts from it. `synthetic` is always `false`. A delivered combination turns over `host.inputEpoch()`, like a press.

**Cost.** Judging a combination takes microseconds per button event, on the event loop; a hold that is running or waiting for the tick is looked at on every 15 ms tick, which also takes microseconds.

**What raises**, besides what raises for every event under [`on`](#host-gamepad-on): no `buttons` (and so an `opts` left out), fewer than two names in it, a name twice, two directions of one stick axis, an unknown name, `exact` that is not a boolean, `holdMs` negative or not a number, and the options of other events.

A combination the game uses too, held for 800 ms and only on its own:

```luau
host.gamepad.on("chord", function(e)
  host.speech.output(table.concat(e.labels, " and ") .. ": reader settings")
  openReaderSettings()
end, { buttons = { "back", "start" }, holdMs = 800, exact = true })
```

### Windows

A reading is one poll of the pad, every 4 ms while the listener exists (about 15.6 ms without a high-resolution timer; see [`on`](#host-gamepad-on)), so two buttons pressed within the same poll arrive together.

### macOS

A reading is one call of GameController's value-changed handler, which comes on its queue whenever an element of the pad changes; the application reads the whole pad at each call. Whether two buttons pressed together arrive in one call or in two decides only which of their `down` events the combination comes with, not whether it comes.

## host.gamepad.off(token) {#host-gamepad-off}

**Signature:** `host.gamepad.off(token: number)` → `boolean`

Removes the listener `on` returned `token` for, and says whether there was one. It answers `false` for a token already removed, never issued, or another module's — which is left alone. Safe to call from inside the listener itself — the usual way to listen for exactly one event. Costs microseconds and never starts watching the pads.

**Raises** when `token` is not a number, `off(nil)` included.

```luau
local token
token = host.gamepad.on("connected", function(e)
  host.speech.output(e.info.name .. " connected")
  host.gamepad.off(token)
end)
```

Register on your overlay's activation and call `off` on its deactivation, and the pad is only listened to while your overlay is the one in front.

### Windows

When the last listener goes, the pad thread stops reading and waits without a timer.

### macOS

When the last `down`, `up`, `axis` or `chord` listener goes, the App Nap activity is ended.

## host.gamepad.status() {#host-gamepad-status}

**Signature:** `host.gamepad.status()` → `{ [string]: string }`

What the controller watching is doing, as readable text, for a diagnostic line in a log. `watcher` is `"not started"` until the first `list`, `state` or `on`, `"running"` after that, or `"failed: "` and the reason. The other keys are the platform's, below, and appear once watching has started. Calling this does not start anything, and it never raises.

```luau
local s = host.gamepad.status()
host.log.info("gamepad " .. s.watcher .. ", " .. (s.mode or s.background or ""))
```

### Windows

Also `xinput` (which DLL, and whether the Guide button can be read — or `unavailable` and why, when no XInput DLL could be loaded: the watcher runs, and no pad is ever listed), `arrival` (whether Windows agreed to announce devices being plugged in), `timer` (whether the 4 ms period is a high-resolution one) and `mode` (parked, probing every two seconds, or polling every 4 ms). `thread` appears only when the pad thread has stopped — it could not create its window, or it failed — and says why; nothing is read from then on.

### macOS

Also `bundle` (the application's bundle identifier), `gamecontroller` (how many controllers were known at start), `background` (what macOS reported after the background request) and, once a `down`, `up`, `axis` or `chord` listener has existed, `activity` (whether the latency-critical activity is held).

## Button and axis names {#button-and-axis-names}

Every name a filter accepts, beside what SDL3, pygame and XInput call it. The order of the rows is the order in which several buttons found down in one reading are reported.

| Ours | SDL3 | pygame `controller` | pygame 2 `joystick`, Xbox pad | XInput |
|---|---|---|---|---|
| `south` | `SDL_GAMEPAD_BUTTON_SOUTH` | `CONTROLLER_BUTTON_A` | button 0 | `XINPUT_GAMEPAD_A` |
| `east` | `..._EAST` | `CONTROLLER_BUTTON_B` | button 1 | `XINPUT_GAMEPAD_B` |
| `west` | `..._WEST` | `CONTROLLER_BUTTON_X` | button 2 | `XINPUT_GAMEPAD_X` |
| `north` | `..._NORTH` | `CONTROLLER_BUTTON_Y` | button 3 | `XINPUT_GAMEPAD_Y` |
| `back` | `..._BACK` | `CONTROLLER_BUTTON_BACK` | button 6 | `XINPUT_GAMEPAD_BACK` |
| `guide` | `..._GUIDE` | `CONTROLLER_BUTTON_GUIDE` | button 10 | (0x0400, extended call only) |
| `start` | `..._START` | `CONTROLLER_BUTTON_START` | button 7 | `XINPUT_GAMEPAD_START` |
| `left_stick` | `..._LEFT_STICK` | `CONTROLLER_BUTTON_LEFTSTICK` | button 8 | `XINPUT_GAMEPAD_LEFT_THUMB` |
| `right_stick` | `..._RIGHT_STICK` | `CONTROLLER_BUTTON_RIGHTSTICK` | button 9 | `XINPUT_GAMEPAD_RIGHT_THUMB` |
| `left_shoulder` | `..._LEFT_SHOULDER` | `CONTROLLER_BUTTON_LEFTSHOULDER` | button 4 | `XINPUT_GAMEPAD_LEFT_SHOULDER` |
| `right_shoulder` | `..._RIGHT_SHOULDER` | `CONTROLLER_BUTTON_RIGHTSHOULDER` | button 5 | `XINPUT_GAMEPAD_RIGHT_SHOULDER` |
| `dpad_up`, `dpad_down`, `dpad_left`, `dpad_right` | `..._DPAD_UP` … | `CONTROLLER_BUTTON_DPAD_UP` … | hat 0 (y is +1 for up) | `XINPUT_GAMEPAD_DPAD_UP` … |
| `misc1` | `..._MISC1` | — | — | — |
| `right_paddle1`, `left_paddle1`, `right_paddle2`, `left_paddle2` | `..._RIGHT_PADDLE1` … | — | — | — |
| `touchpad` | `..._TOUCHPAD` | — | — | — |
| `left_trigger`, `right_trigger` | (an axis there) | (an axis there) | (an axis there) | `bLeftTrigger`, `bRightTrigger` past 30 |
| `left_stick_up`, `_down`, `_left`, `_right`; the same four for `right_stick_` | — | — | — | — |

The last two rows are the buttons the application derives from the analog values, in that order: pressed at half deflection (triggers 0.12), released below 0.35 (triggers 0.08). A diagonal holds two stick directions, like a D-pad.

What `label` (and a combination's `labels`) says for each button, by `family`:

| Ours | Xbox | PlayStation | Nintendo | generic |
|---|---|---|---|---|
| `south`, `east`, `west`, `north` | A, B, X, Y | Cross, Circle, Square, Triangle | B, A, Y, X | South button, East button, West button, North button |
| `back`, `start`, `guide` | View, Menu, Xbox | Share, Options, PS | Minus, Plus, Home | Back, Start, Guide |
| `left_stick`, `right_stick` | Left stick, Right stick | L3, R3 | Left stick, Right stick | Left stick, Right stick |
| `left_shoulder`, `right_shoulder` | LB, RB | L1, R1 | L, R | Left shoulder, Right shoulder |
| `left_trigger`, `right_trigger` | LT, RT | L2, R2 | ZL, ZR | Left trigger, Right trigger |
| `misc1` | Share | Mute | Capture | Misc |
| `right_paddle1`, `right_paddle2`, `left_paddle1`, `left_paddle2` | P1, P2, P3, P4 | Right function, Right back, Left function, Left back | Right paddle, Right paddle 2, Left paddle, Left paddle 2 | Right paddle 1, Right paddle 2, Left paddle 1, Left paddle 2 |
| `dpad_up` … `dpad_right` | Up, Down, Left, Right | the same | the same | the same |
| `left_stick_up` … `right_stick_right` | Left stick up … Right stick right | the same | the same | the same |
| `touchpad` | Touchpad | Touchpad | Touchpad | Touchpad |

The labels are English on purpose; translate them in the module if it speaks another language, and match on the name, never on the label. Which families a platform reports is in the platform sections of [`list`](#host-gamepad-list).

| Axis | SDL3 | pygame `controller` | pygame 2 `joystick`, Xbox pad | XInput |
|---|---|---|---|---|
| `left_x`, `left_y` | `SDL_GAMEPAD_AXIS_LEFTX`, `..._LEFTY` | `CONTROLLER_AXIS_LEFTX`, `..._LEFTY` | axis 0, axis 1 (down positive, as ours) | `sThumbLX`, `sThumbLY` (up positive — ours is down positive) |
| `right_x`, `right_y` | `..._RIGHTX`, `..._RIGHTY` | `CONTROLLER_AXIS_RIGHTX`, `..._RIGHTY` | axis 3, axis 4 | `sThumbRX`, `sThumbRY` (up positive) |
| `left_trigger`, `right_trigger` | `..._LEFT_TRIGGER`, `..._RIGHT_TRIGGER` | `CONTROLLER_AXIS_TRIGGERLEFT`, `..._TRIGGERRIGHT` | axis 2, axis 5 (−1 released, 1 pulled) | `bLeftTrigger`, `bRightTrigger` (0–255) |

The `joystick` column is pygame 2's own documented numbering for an Xbox 360 pad, which is what an XInput pad reads as through the SDL2 that pygame 2 ships with. It is not a rule for other pads: pygame 1 (SDL 1.2) put both triggers on one axis 2 and the right stick on axes 4 and 3, and a PlayStation or Nintendo pad numbers its buttons differently again. For those, print the numbers from the pad in hand, or port through `pygame._sdl2.controller`, whose names are positional like ours.

`button7` (any `button` and a number from 1) and the HID axis names `x`, `y`, `z`, `rx`, `ry`, `rz`, `slider` and `dial` are also accepted in a filter. They belong to controllers nobody has mapped, which no platform reports; such a button's label is `Button 7`.
