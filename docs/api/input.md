---
title: "host.input — pressing things"
sidebar_position: 6
toc_max_heading_level: 2
---

How a module actually presses the thing it has just described: the synthetic mouse and keyboard behind every activation an overlay performs. Most modules never touch it, because the overlay's own control kinds click their coordinates for them, so you come here for the gesture that is not one of those — Kontakt's instrument-editor wrench at a fixed offset from the right edge of the window, ON:EAR's tone knob nudged half a wheel notch because a whole one moves it five of its own units, Melodyne's menu bar opened a few pixels above the client origin.

All of it is blind clicking at screen coordinates, and **nothing here asks what is drawn under the point** — which is why modules bring their window forward first, and why the overlay runtime refuses a point falling outside its own window.

Dragging is deliberately not a press, a warp and a release: the movement is paced over sixteen injected steps and blocks the calling thread for about sixty milliseconds on Windows, because a control that reads the *speed* of a gesture answers an instantaneous jump with an enormous change.

Sending a shortcut is the call that catches authors out. On Windows the synthesised key inherits whatever the user is still physically holding, so a key sent from inside a hotkey callback arrives with that hotkey's modifiers attached — which is why the overlay waits for the modifiers to come up before it sends anything.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["input"]
```

See [what that list is and is not](./index.md#capabilities).

## host.input.cursorPos() {#host-input-cursorpos}

Returns the current mouse cursor position in screen coordinates.

**Signature:** `host.input.cursorPos() -> { x: number, y: number }`

Takes no arguments. Returns a table `{ x, y }`.

```luau
local p = host.input.cursorPos()
print(p.x, p.y)
```

## host.input.move(x, y) {#host-input-move}

Moves the mouse cursor to the given screen coordinates.

**Signature:** `host.input.move(x: number, y: number) -> nil`

Both arguments are integers. Returns `nil`.

```luau
host.input.move(960, 540)
```

### Windows

The pointer is placed, and nothing is told that it moved. A control watching for motion while its button is held can miss the move entirely.

### macOS

A real move event is posted — or a **drag** event when a button is currently held, which is also what the implicit move inside `mouseDown` and `mouseUp` does.

So the press-then-glide-then-release gesture, composed from `mouseDown`, a timer and `mouseUp`, is a genuine drag here and can be an invisible warp on Windows: a slider that follows the pointer on a Mac may not move at all there. Where the movement itself is the point, use `host.input.drag`, which paces it on both.

## host.input.click(x, y, opts?) {#host-input-click}

Moves to `(x, y)` and synthesizes a mouse click there.

**Signature:** `host.input.click(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

`x`/`y` are integers. The optional `opts.button` selects the button; it is matched case-insensitively, with `"right"` and `"middle"` recognized and anything else (including omitting `opts`) defaulting to **left**. Returns `nil`.

```luau
host.input.click(300, 200)                       -- left click
host.input.click(300, 200, { button = "right" }) -- right click
```

## host.input.drag(x1, y1, x2, y2, opts?) {#host-input-drag}

Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases — with real, paced movement in between.

A drag is not a press, a warp and a release. On Windows the pointer is moved with injected move events rather than `SetCursorPos`, interpolated over sixteen steps with a few milliseconds between them, because two kinds of control tell the difference: one that watches for motion while its button is held sees injected events and can miss a warp entirely, and one that reads the *speed* of a drag answers an instantaneous jump with an enormous change. macOS posts a drag event between press and release, which it has always done. The pacing blocks the calling thread for about sixty milliseconds, which is only ever paid when somebody deliberately drags something.

If a control needs a sustained press before it will react at all, use `mouseDown`/`mouseUp` with a timer instead — that is why those exist separately.

### Windows

The pointer is moved with injected move events rather than `SetCursorPos`, interpolated over sixteen steps a few milliseconds apart. The pacing blocks the calling thread for about sixty milliseconds, paid only when something is deliberately dragged.

### macOS

A single `Dragged` event is posted between press and release — no interpolation. That asymmetry is deliberate: a scrollbar needs one intermediate event and gets it. A control that only reacts to *continuous* movement would work on Windows and not here, and nothing has yet exercised the difference on a Mac.

**Signature:** `host.input.drag(x1: number, y1: number, x2: number, y2: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

All four coordinates are integers. `opts.button` behaves exactly as in `click` (defaults to left). Returns `nil`.

```luau
host.input.drag(100, 100, 400, 300)
host.input.drag(100, 100, 400, 300, { button = "middle" })
```

## host.input.scroll(x, y, amount) {#host-input-scroll}

Moves to `(x, y)` and scrolls the mouse wheel by `notches`, which may be fractional.

**Signature:** `host.input.scroll(x: number, y: number, notches: number) -> nil`

`x` and `y` are integers. `notches` is in wheel notches and **need not be a whole number**; positive scrolls up, negative scrolls down. Returns `nil`.

A notch is 120 units to the operating system, and a control decides for itself what one is worth — ON:EAR's tone knob moves by five of its own units per notch, which is as fine as this could ask for while notches were whole. Half a notch is 60 units, and an application that scales its step by the delta moves by half as much. One that rounds to its own interval instead simply does not move and reports the same value back, which is the honest outcome and says to stop asking for halves.

### Windows

The unit is native: 120 units to a notch is what `WHEEL_DELTA` counts, so a fractional request is expressed exactly.

### macOS

Expressed in Quartz **lines**, which cannot be smaller than one. A caller asking for less than a line gets the smallest whole line rather than nothing — a control that needs a half-step will not get one here until this asks Quartz in pixel units instead. The conversion logs the first scroll of a run, and after that every request that is not a whole notch, saying what was asked for and what was sent; the difference shows up as evidence rather than as a mystery.

```luau
host.input.scroll(960, 540, -3)   -- down three notches
host.input.scroll(960, 540, 0.5)  -- half a notch up, where the control is finer than a notch
```

## host.input.send(combo) {#host-input-send}

Sends a keyboard shortcut by pressing the modifiers, tapping the key, and releasing in reverse order.

**Signature:** `host.input.send(combo: string) -> nil`

`combo` is a single string of `+`-separated parts, with the final part being the key and any leading parts being modifiers (whitespace around parts is trimmed). Recognized modifiers (case-insensitive): `ctrl`/`control`, `alt`/`option`, `shift`, and `win`/`super`/`cmd`/`command`/`meta`. An empty combo or an unknown modifier raises a Lua error. Returns `nil`.

```luau
host.input.send("Ctrl+S")
host.input.send("Ctrl+Shift+Esc")
```

### Windows

The modifiers are synthesised as real key presses around the key, and whatever the user is **physically holding is inherited**. Called from a hotkey callback while Alt is still down, `host.input.send("Escape")` arrives as Alt+Escape — which is why `host.keys.modifiersDown()` exists and why modules defer a synthesised key until the user has let go.

### macOS

The modifiers are set as flags on the event, and setting them **replaces the whole set**, so anything the user is holding is stripped and the same call delivers a bare Escape. No modifier key event is posted at all, so an application that watches for physical modifier presses sees an unmodified key.

The deferral dance is therefore unnecessary for *keys* here — but not for clicks: a synthesised click carries no flag-clearing of its own, so one posted while the user holds Alt is an Alt+click on both platforms.

## host.input.text(text) {#host-input-text}

Types a Unicode string as synthetic keystrokes.

**Signature:** `host.input.text(text: string) -> nil`

`text` is sent character-by-character as Unicode input. Returns `nil`.

```luau
host.input.text("Hello, world!")
```

## host.input.mouseDown(x, y, opts?) {#host-input-mousedown}

**Signature:** `host.input.mouseDown(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

Presses a mouse button at `(x, y)` and **leaves it down**. This is the half of a gesture that `click` and `drag` cannot express, because both of those press and release in the same breath: a control that only reveals itself while the button is *sustained* — the flyout of extra tools behind Melodyne's tool icon is the case that prompted this API, a gesture that later turned out to be undocumented and was replaced by a key — never appears for either of them. It is split into two calls rather than offered as a timed variant deliberately: holding, gliding and settling is the better part of a second, and a call that slept for that long would sleep in the event pump, stopping speech, hotkeys and detection with it. Split, the caller composes the gesture out of `host.timer` callbacks and the pump keeps running between the parts. There is no screen read here and nothing is asked about what is under the point, so the call itself is cheap.

Nothing releases the button for you. A held button belongs to the whole desktop, not to your module, so **every** continuation must end in a `mouseUp` — including the ones that give up because the overlay moved to another window mid-gesture. Pin the window handle before the press and re-check it in each callback, exactly as the timed sequences elsewhere in the runtime do; `opts.button` must match on the way up as it did on the way down.

```luau
-- Press the tool icon and hold it, so a flyout that only exists while the button is down has
-- time to appear; then glide onto the entry and let go. (Shaped after the press-and-hold
-- gesture the Melodyne overlay used, removed from modules/melodyne/src/main.luau in 83886d0
-- once its sub-tools turned out to be reachable by a documented key instead.)
local origin = o:origin()
local drag = tool.variants[target - 1].drag
local x, y = origin.client.x + tool.at[1], origin.client.y + tool.at[2]
local pinned = o:hwnd()
host.input.mouseDown(x, y)
host.timer.after(350, function()
  -- A gesture spanning a second can outlive the window it started in: release either way.
  if not (o.active and o:hwnd() == pinned) then return host.input.mouseUp(x, y) end
  host.input.move(x, y + drag)
  host.timer.after(150, function() host.input.mouseUp(x, y + drag) end)
end)
```

### Windows

The pointer is placed with `SetCursorPos` and then the button-down event is sent. As with `host.input.move`, the placement is not motion: nothing is told the pointer arrived, so a control that highlights or hit-tests on movement is pressed without ever having been hovered.

### macOS

A movement event is posted before the press — or a **drag** event if a button is already held — so the press lands on a control that has been told the pointer is there.

## host.input.mouseUp(x, y, opts?) {#host-input-mouseup}

**Signature:** `host.input.mouseUp(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

Releases a mouse button at `(x, y)`, ending a press begun with `mouseDown`. The coordinates are the point the gesture *ends* on, which is usually not the point it started on: a press-hold-glide-release picks its entry by where the release happens. `opts.button` selects which button is released, matched case-insensitively with `"right"` and `"middle"` recognized and anything else — including omitting `opts` — meaning **left**; release the button you actually pressed, since nothing pairs the two calls up for you.

Treat this as the mandatory end of every path through a held gesture, not just the successful one. The release is what returns the desktop to a sane state, so the abort branches (`overlay no longer active`, `different window`) have to call it too, and a module that leaves a button down leaves it down for the user.

```luau
-- The glide and its tail, as the removed Melodyne gesture had them: every step re-checks the
-- window and lets go back at the press point if it has changed, and only the settle at the
-- end releases where the glide actually finished.
local GLIDE_STEPS, GLIDE_MS, SETTLE = 12, 20, 150
local drag = tool.variants[target - 1].drag
local step = 0
local glide
glide = function()
  if not (o.active and o:hwnd() == pinned) then return host.input.mouseUp(x, y) end
  step = step + 1
  host.input.move(x, y + math.floor(drag * step / GLIDE_STEPS))
  if step < GLIDE_STEPS then
    host.timer.after(GLIDE_MS, glide)
  else
    host.timer.after(SETTLE, function() host.input.mouseUp(x, y + drag) end)
  end
end
glide()
```

### Windows

`SetCursorPos` and then the button-up event. Because the intermediate `host.input.move` calls are warps rather than motion, a control that follows the pointer only while it sees movement can sit still through the whole glide and receive a release it has no path to. Where the movement itself is the point, `host.input.drag` paces it.

### macOS

The move before the release is posted as a **drag** event, because the backend sees a button still held. That is what makes "press here, glide there, let go" a genuine drag on this platform: the control has followed the pointer the whole way and the release lands on the entry it was over.

## host.input.post(id, key) {#host-input-post}

**Signature:** `host.input.post(id: number, key: string) -> nil`

Delivers a single key to **one window** rather than to whatever has focus. Reach for it when a hotkey has to act *and* still hand the application a key: our own hotkey registration and key capture see synthesised input exactly as they see real input, so `host.input.send` from inside a hotkey callback re-triggers the handler that sent it, forever. A posted key goes to the addressed window (or process) without travelling past that machinery, and is seen by nobody else. `id` is the handle from `host.window.*` or the overlay's `O:hwnd()` — never a number you constructed. `key` is a single [key name](./keys.md#key-spec-string-format) (`A`–`Z`, `0`–`9`, `F1`–`F24`, `Space`, `Enter`/`Return`, `Esc`/`Escape`, `Tab`, `Backspace`, `Delete`/`Del`, the arrows, `Home`, `End`, `PageUp`, `PageDown`, all matched case-insensitively); **there are no modifiers here** — `"Ctrl+S"` is not a key name and raises, as does any name the table does not know.

Unlike every other call in `host.input` that acts, a posted key does **not** turn over `host.inputEpoch()`. Anything memoised against that counter — a view state read from pixels, a probe of which sub-tool is active — will not be re-read because you posted a key, so re-read it explicitly after a post sequence, which is what the Melodyne overlay does when it checks with the uncached probe what its presses actually reached.

```luau
-- modules/melodyne/src/main.luau: Melodyne's sub-tools are reached by pressing the tool's
-- function key REPEATEDLY. Posted, not sent, because our own capture would catch it back.
local hwnd = o:hwnd()
local sent = 0
local step
step = function()
  if not (o.active and o:hwnd() == hwnd) then return end  -- the window changed under us
  host.input.post(hwnd, tool.key)                         -- e.g. "F5"
  sent = sent + 1
  if sent < presses then host.timer.after(PRESS_GAP, step) end
end
host.timer.after(PRESS_GAP, step)
```

### Windows

Genuinely per-window: `WM_KEYDOWN`/`WM_KEYUP` are posted to the handle you passed with `PostMessageW`, with the scan code packed into the `lParam` because some applications read that rather than the virtual key — Melodyne decodes keys itself rather than leaving it to the defaults. Posting is also the path most likely to be seen at all here: Melodyne runs its own message pump with its own accelerator table, so a posted `WM_KEYDOWN` reaches `TranslateAccelerator` where a sent one would bypass it. A handle whose window is gone is not reported: the post goes nowhere and the call returns normally.

### macOS

There is no per-window message queue to post into, so this is one step coarser — the event is handed to the window's **process** with `CGEventPostToPid` and lands wherever that application routes it, which need not be the window you addressed. Three consequences a caller can observe: the handle must be one this backend issued, and a stale or foreign number raises an error naming it instead of quietly doing nothing; a key name the shared table accepts can still fail here, because it has to map on to a Quartz keycode as well, and one that does not raises an error naming the key and its VK code; and the modifier flags on the event are **cleared**, not inherited, so a key posted from inside a hotkey callback while its combination is still physically held arrives bare. The first post of a run writes a line to the log saying it is routing per process — the line to suspect if a posted key ever surfaces in the wrong window of the same application.
