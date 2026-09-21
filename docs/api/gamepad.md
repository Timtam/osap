---
title: "host.gamepad — watching game controllers"
sidebar_position: 3.5
toc_max_heading_level: 2
---

A game played with a controller has no keyboard focus to hang an overlay on and usually no accessibility tree either, so the press itself is the only event there is. This namespace hands that press to a module without taking anything from the game: which button went down on which pad, where the sticks are, and when a pad arrives or leaves. A module reading a game's menu listens for the D-pad, looks at the screen the moment after, and says what the cursor moved to.

**Nothing here looks at which window is in front.** A listener gets every matching event the platform delivers while its module is enabled, whatever application has the focus; whether the platform delivers input while this application is in the background is in the platform sections of [`on`](#host-gamepad-on). Deciding that a press is meant for your game is the module's job: check `host.window.active()` in the callback, or register the listener when your game comes forward and remove it with [`off`](#host-gamepad-off) when it leaves.

**Which pads are seen depends on the platform**: see the platform sections of [`list`](#host-gamepad-list).

**It observes and nothing more.** Every press reaches the game as well as your module, and nothing here can stop that — on Windows that would take a filter driver and a virtual controller, on macOS seizing the device away from the game, and neither fits an application that installs by unzipping. There is therefore no claim and no conflict the way `host.hotkey` has them: every enabled module with a matching listener gets every event. A command of your own belongs on the keyboard, through `host.hotkey`; a button combination on the pad would reach the game too.

**Names are positional.** `south` is the bottom face button whatever is printed on it — A on an Xbox pad, Cross on a PlayStation pad, B on a Nintendo pad — so a module written for "the confirm button" works on every family the platform reports (on Windows, that is XInput pads only). What is printed on the pad arrives as `label`, in English, for speech. The full list and a table for porting from SDL, pygame and XInput are [at the end](#button-and-axis-names).

**Values.** Sticks run from -1 to 1 with `y` positive **down**, as in SDL and on the screen; triggers run from 0 to 1. `state()` gives the raw values. Listeners get dead-zoned values instead — radially over the two halves of a stick, with Microsoft's defaults (0.24 left, 0.27 right, 0.12 triggers) — and the hub also turns each stick direction and each trigger into a button of its own (`left_stick_down`, `right_trigger`), pressed at half deflection and released below 0.35 (triggers: 0.12 and 0.08), so a menu can be walked with the stick the same way as with the D-pad.

**The event says when to look, not what the game drew.** Each event carries `time` — milliseconds on the `host.now()` clock when the press was detected — and `age`, how long ago that was when your callback ran. The game draws the moved cursor one or more of its own frames *after* the press, so a screen read made at once usually sees the old cursor. Search in a short burst until the picture has changed, as the example under [`on`](#host-gamepad-on) does. For the same reason a press turns over `host.inputEpoch()` — the game's screen does change — but it turns over at the press, before the game has redrawn: do not cache a reading of the game's screen against `inputEpoch` alone.

**Holding a button.** There are no chords. To act on a long press, start a timer on `down` and ask `state()` when it fires; the example under [`state`](#host-gamepad-state) does exactly that.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["gamepad"]
```

See [what that list is and is not](./index.md#capabilities).

## host.gamepad.list() {#host-gamepad-list}

**Signature:** `host.gamepad.list()` → `{ PadInfo }`

The pads connected now, sorted by `index`; an empty table when there are none. The first call anywhere in the application starts watching and raises an error on a platform with no controller support.

```
PadInfo = { index = 1, id = "xinput:0", name = "Xbox controller",
            family = "xbox" | "playstation" | "nintendo" | "generic",
            source = "xinput" | "gamecontroller", mapped = true,
            vendor = number?, product = number?,
            buttons = { "south", "east", ... }, axes = { "left_x", ... } }
```

`index` runs from 1 to 8, lowest free first, and stays with a pad while it is connected; a pad that is unplugged frees its number for the next one. `id` says where the platform sees this connection, not which device it is — see the platform notes below — so do not use it to recognise a pad you have seen before. `buttons` lists every button the pad can report, the derived stick and trigger buttons included.

```luau
for _, pad in ipairs(host.gamepad.list()) do
  host.speech.output(string.format("Pad %d: %s", pad.index, pad.name))
end
```

### Windows

Xbox-type pads, through XInput, which allows at most four; their `family` is `xbox`. Their `id` is XInput's slot, `xinput:0` to `xinput:3`: a different pad plugged into the same slot gets the same `id`. `guide` is in `buttons` only when the system's XInput has the extended entry point that reports it. `vendor` and `product` are the pad's USB ids where XInput's extended capabilities call answers for the slot, and `nil` otherwise. PlayStation, Nintendo and other controllers are not listed on Windows unless they present themselves through XInput.

The first call waits up to 250 ms for the first reading of the four slots, so a pad that is plugged in is normally already listed. When no module listens for `down`, `up` or `axis`, `list()` asks for a fresh reading, waits up to 50 ms for it, and keeps the pads read for five seconds afterwards.

### macOS

The controllers the GameController framework supports, with two sticks and two triggers; `family` comes from the controller's own description. Their `id` is `gc:` and a number counted up for every connection, so the same pad unplugged and plugged back in gets a new one. The framework finds controllers on its own schedule, so a pad attached at launch can appear a moment after the first call — listen for [`connected`](#host-gamepad-on) as well, which also announces the pads already there. The log records the application's bundle identifier when watching starts, and warns when there is none: run the application from its `.app`.

## host.gamepad.state(pad) {#host-gamepad-state}

**Signature:** `host.gamepad.state(pad: number)` → `PadState?`

What pad number `pad` is doing now, or `nil` when there is no pad with that index. Values are raw — no dead zone — so a module that wants its own curve has the original numbers.

```
PadState = { pad = 1, buttons = { south = false, dpad_down = true, ... },
             axes = { left_x = 0.01, left_y = -0.98, ... }, time = 81234 }
```

`buttons` names every button the pad has, `false` included, so asking about a released button gives `false` rather than `nil`. `time` is when the pad last reported anything.

A long press, the way to do what a chord would: act only if `start` is still down 800 ms later.

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

While a `down`, `up` or `axis` listener exists, `state()` answers from the same readings, every 4 ms, that those listeners get. Otherwise — no listener, or only `connected` and `disconnected` ones, which are served by a look every two seconds — it reads the pad afresh (waiting up to 50 ms) and keeps it read for five seconds, so calling it repeatedly does not start and stop the reading each time.

### macOS

Answered from the last values GameController delivered, which it does whenever anything on the pad changes.

## host.gamepad.on(event, callback, opts?) {#host-gamepad-on}

**Signature:** `host.gamepad.on(event: "down" | "up" | "axis" | "connected" | "disconnected", callback: (e: PadEvent) -> (), opts: { pad: number?, buttons: {string}?, axes: {string}?, deadzone: number?, step: number?, maxAge: number? }?)` → `token: number`

Calls `callback` for every matching event while the module is enabled, until [`off`](#host-gamepad-off) or until the module is reloaded. Returns a token for `off`.

```
down / up = { type = "down", pad = 1, button = "dpad_down", label = "Down",
              time = 81234, age = 9, synthetic = false }
axis      = { type = "axis", pad = 1, axis = "left_y", value = -0.73, raw = -0.80,
              time = 81240, age = 3, synthetic = false }
connected / disconnected = { type = "connected", pad = 1, info = PadInfo,
                             time = 80012, age = 0, synthetic = false }
```

The options, each for the events it makes sense for — any other key, and any unknown button or axis name, raises an error rather than leaving a listener that never fires:

| Option | Events | Meaning |
|---|---|---|
| `pad` | all | Only this pad index. |
| `buttons` | `down`, `up` | Only these buttons, by name. |
| `axes` | `axis` | Only these axes, by name. |
| `deadzone` | `axis` | Replaces the default dead zone for every axis of this listener; `0` switches it off. At least 0 and below 1. |
| `step` | `axis` | Deliver a value only when it moved at least this far since the last one this listener got. Default `0.05`. Coming to rest (`0`) and full deflection are always delivered. |
| `maxAge` | `down`, `up` | Milliseconds. A press older than this when the application gets to it is not delivered — a press acted on seconds late reads the wrong screen. Default `1000`. |

An `up` reaches a listener only if that listener saw its `down`: the release of a press that was dropped for `maxAge`, that happened before the listener was registered, or that happened while the module was disabled is not delivered, so a module never hears a release of something it never saw pressed. An `up` is never dropped for its own age — the module that got the press needs the release. `axis` values are never dropped for age either: each one is where the stick is now.

`synthetic` is `true` for events the application made up: the `up` sent for every button still held when a pad disappears, before its `disconnected`, and the `connected` a listener receives for each pad that was already there when it was registered, or that arrived while its module was disabled. A listener hears `connected` once per pad per connection, whichever of the two told it first.

`axis` events are sent at most once per pass of the application's loop per axis, carrying the latest value; `down` and `up` keep their order. The two halves of a stick share one dead zone, so moving one can change the other's value — pushing `left_x` back to the middle while `left_y` rests just inside the dead zone takes `left_y` to 0 as well — and a listener gets an event for each half whose value changed, the other half included.

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

While a `down`, `up` or `axis` listener exists, XInput pads are read every 4 ms, so a press is detected up to 4 ms after it happened and a press shorter than that can be missed. With only `connected` and `disconnected` listeners the four slots are looked at every two seconds, and with no listener nothing is read at all. While anybody listens, a new pad is also looked for as soon as Windows announces a HID device arriving. The pads are read the same way whichever application is in front. Windows opens Xbox Game Bar on the Guide button by default (Settings, Gaming), which takes the focus from the game.

### macOS

GameController delivers every change as it happens, on a queue of its own. Input reaches an application that is not in front only after it asks, with `shouldMonitorBackgroundEvents`; the application asks when the first controller connects and logs what macOS answered. While a `down`, `up` or `axis` listener exists it also holds a latency-critical `NSProcessInfo` activity, which is the request macOS offers against App Nap. `guide` is listed when the controller reports a Home button, but macOS may keep that button for itself.

## host.gamepad.off(token) {#host-gamepad-off}

**Signature:** `host.gamepad.off(token: number)` → `boolean`

Removes the listener `on` returned `token` for, and says whether there was one. A token of another module is left alone. Safe to call from inside the listener itself — the usual way to listen for exactly one event.

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

When the last `down`, `up` or `axis` listener goes, the App Nap activity is ended.

## host.gamepad.status() {#host-gamepad-status}

**Signature:** `host.gamepad.status()` → `{ [string]: string }`

What the controller watching is doing, as readable text, for a diagnostic line in a log. `watcher` is `"not started"` until the first `list`, `state` or `on`, `"running"` after that, or `"failed: "` and the reason. Calling this does not start anything.

```luau
local s = host.gamepad.status()
host.log.info("gamepad " .. s.watcher .. ", " .. (s.mode or s.background or ""))
```

### Windows

Also `xinput` (which DLL, and whether the Guide button can be read), `arrival` (whether Windows agreed to announce devices being plugged in), `timer` (whether the 4 ms period is a high-resolution one) and `mode` (parked, probing every two seconds, or polling every 4 ms).

### macOS

Also `bundle` (the application's bundle identifier), `gamecontroller` (how many controllers were known at start), `background` (what macOS reported after the background request) and `activity` (whether the latency-critical activity is held).

## Button and axis names {#button-and-axis-names}

Every name a filter accepts, beside what SDL3, pygame and XInput call it and what is printed on each family's pads.

| Ours | SDL3 | pygame `controller` | pygame 2 `joystick`, Xbox pad | XInput | Xbox | PlayStation | Nintendo |
|---|---|---|---|---|---|---|---|
| `south` | `SDL_GAMEPAD_BUTTON_SOUTH` | `CONTROLLER_BUTTON_A` | button 0 | `XINPUT_GAMEPAD_A` | A | Cross | B |
| `east` | `..._EAST` | `CONTROLLER_BUTTON_B` | button 1 | `XINPUT_GAMEPAD_B` | B | Circle | A |
| `west` | `..._WEST` | `CONTROLLER_BUTTON_X` | button 2 | `XINPUT_GAMEPAD_X` | X | Square | Y |
| `north` | `..._NORTH` | `CONTROLLER_BUTTON_Y` | button 3 | `XINPUT_GAMEPAD_Y` | Y | Triangle | X |
| `back` | `..._BACK` | `CONTROLLER_BUTTON_BACK` | button 6 | `XINPUT_GAMEPAD_BACK` | View | Share | Minus |
| `guide` | `..._GUIDE` | `CONTROLLER_BUTTON_GUIDE` | button 10 | (0x0400, extended call only) | Xbox | PS | Home |
| `start` | `..._START` | `CONTROLLER_BUTTON_START` | button 7 | `XINPUT_GAMEPAD_START` | Menu | Options | Plus |
| `left_stick` | `..._LEFT_STICK` | `CONTROLLER_BUTTON_LEFTSTICK` | button 8 | `XINPUT_GAMEPAD_LEFT_THUMB` | Left stick | L3 | Left stick |
| `right_stick` | `..._RIGHT_STICK` | `CONTROLLER_BUTTON_RIGHTSTICK` | button 9 | `XINPUT_GAMEPAD_RIGHT_THUMB` | Right stick | R3 | Right stick |
| `left_shoulder` | `..._LEFT_SHOULDER` | `CONTROLLER_BUTTON_LEFTSHOULDER` | button 4 | `XINPUT_GAMEPAD_LEFT_SHOULDER` | LB | L1 | L |
| `right_shoulder` | `..._RIGHT_SHOULDER` | `CONTROLLER_BUTTON_RIGHTSHOULDER` | button 5 | `XINPUT_GAMEPAD_RIGHT_SHOULDER` | RB | R1 | R |
| `dpad_up` … `dpad_right` | `..._DPAD_UP` … | `CONTROLLER_BUTTON_DPAD_UP` … | hat 0 (y is +1 for up) | `XINPUT_GAMEPAD_DPAD_UP` … | Up … | Up … | Up … |
| `misc1` | `..._MISC1` | — | — | — | Share | Mute | Capture |
| `right_paddle1`, `left_paddle1`, `right_paddle2`, `left_paddle2` | `..._RIGHT_PADDLE1` … | — | — | — | P1, P3, P2, P4 | | |
| `touchpad` | `..._TOUCHPAD` | — | — | — | | Touchpad | |

The buttons the application derives from the analog values: `left_trigger`, `right_trigger`, and `left_stick_up`, `left_stick_down`, `left_stick_left`, `left_stick_right` with the same four for `right_stick_`. A diagonal holds two of them, like a D-pad.

| Axis | SDL3 | pygame `controller` | pygame 2 `joystick`, Xbox pad | XInput |
|---|---|---|---|---|
| `left_x`, `left_y` | `SDL_GAMEPAD_AXIS_LEFTX`, `..._LEFTY` | `CONTROLLER_AXIS_LEFTX`, `..._LEFTY` | axis 0, axis 1 (down positive, as ours) | `sThumbLX`, `sThumbLY` (up positive — ours is down positive) |
| `right_x`, `right_y` | `..._RIGHTX`, `..._RIGHTY` | `CONTROLLER_AXIS_RIGHTX`, `..._RIGHTY` | axis 3, axis 4 | `sThumbRX`, `sThumbRY` (up positive) |
| `left_trigger`, `right_trigger` | `..._LEFT_TRIGGER`, `..._RIGHT_TRIGGER` | `CONTROLLER_AXIS_TRIGGERLEFT`, `..._TRIGGERRIGHT` | axis 2, axis 5 (−1 released, 1 pulled) | `bLeftTrigger`, `bRightTrigger` (0–255) |

The `joystick` column is pygame 2's own documented numbering for an Xbox 360 pad, which is what an XInput pad reads as through the SDL2 that pygame 2 ships with. It is not a rule for other pads: pygame 1 (SDL 1.2) put both triggers on one axis 2 and the right stick on axes 4 and 3, and a PlayStation or Nintendo pad numbers its buttons differently again. For those, print the numbers from the pad in hand, or port through `pygame._sdl2.controller`, whose names are positional like ours.

`button7` and the HID axis names `x`, `y`, `z`, `rx`, `ry`, `rz`, `slider` and `dial` are also accepted in a filter. They belong to controllers nobody has mapped, which no platform reports yet.
