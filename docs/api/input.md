---
title: "host.input — mouse and keyboard"
sidebar_position: 6
toc_max_heading_level: 2
---

Synthesises mouse and keyboard input: moving and clicking at screen coordinates, dragging, scrolling, sending a key combination, and typing text.

It is what performs every activation an overlay makes. Most modules never touch it, because the overlay's own control kinds click their coordinates for them, so you come here for the gesture that is not one of those — Kontakt's instrument-editor wrench at a fixed offset from the right edge of the window, ON:EAR's tone knob nudged half a wheel notch because a whole one moves it five of its own units, Melodyne's menu bar opened a few pixels above the client origin.

All of it is blind clicking at screen coordinates, and **nothing here asks what is drawn under the point** — except a [`click`](#host-input-click) given the `window` it is for — which is why modules bring their window forward first, and why the overlay runtime refuses a point falling outside its own window. Coordinates are device pixels on Windows and points on macOS (see [`host.screen.size`](./screen.md#host-screen-size)), and every one is converted to a whole number by cutting toward zero without an error. So a centre computed as `x + w / 2` lands on the whole pixel toward zero, which for a negative coordinate — a monitor left of or above the primary — is the opposite direction from `//`; where the nearest pixel matters, round with `math.floor(v + 0.5)`.

Dragging is deliberately not a press, a warp and a release: the movement is paced over sixteen injected steps and blocks the calling thread for about sixty milliseconds on Windows, because a control that reads the *speed* of a gesture answers an instantaneous jump with an enormous change.

**Acting does not wait for a read.** Every call on this page but `cursorPos` and `held` acts at once — a click with `via = "noactivate"` is handed at once to the thread that makes it — also while the calling module has a [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) or a [`host.screen`](./screen.md#with-a-callback) call given a callback still out — and so does [`host.window.focus`](./window.md#host-window-focus). A picture that is not taken yet when the input is made may show the screen after it. So a module that reads a field and then clicks it clicks once the read is answered: after the call returns in a handler, where it [waits](./ocr.md#where-it-waits), or in its callback. Whether an input could be in a picture is what the picture's `inputEpoch` says, against [`host.inputEpoch()`](./timer.md#host-inputepoch) as it stood before the input — except after a [`post`](#host-input-post), which does not turn it over, and after a `host.window.focus`, which turns it over only once the window has come forward: after those two, compare the picture's `time` with `host.now()` noted after the act ([`host.inputEpoch`](./timer.md#host-inputepoch)).

When a module acts while the capture thread has yet to take a picture it asked for that is meant to show the screen as it was at the call, the log says so, once a session per module, naming the call that acted. Those pictures are a text read's, but not one of a snapshot, whose picture is the snapshot's; a plain `snapshot`'s or `profile`'s with a callback — and the points of a `pixel` or `pixels` given one, read on that thread too —, and a timed one's whose `at` had already passed at the call; and the first picture of a change wait, which is its baseline, unless the wait has a `from` that holds every watched region:

`[<module>] acted (host.input.click) while a picture it asked for was not taken yet; that picture may show the input — act once the picture is answered: after the call returns in a handler, or in its callback`

A picture asked for with an `at` still to come at the call is meant to come after whatever the module does next, and is not counted; nor are a change wait's rounds after its baseline, which are there to see what the module does, nor a picture already taken whose answer is not back yet — a text read's while it is being recognised, say. Nothing here waits: the note costs a lookup of the module's requests and, while one is out, a look at the capture thread's queue under its lock, which that thread never holds across a capture.

```luau
-- Read the field, then click it: in the read's callback the picture is of the field as it was
-- before the click, however long the capture took.
host.ocr.recognize({ 400, 300, 520, 320 }, function(r)
  if r.status == "text" then host.speech.output(r.text) end
  host.input.click(460, 310)
end)
```

Sending a shortcut is the call that catches authors out. On Windows the synthesised key inherits whatever the user is still physically holding, so a key sent from inside a hotkey callback arrives with that hotkey's modifiers attached — which is why the overlay waits for the modifiers to come up before it sends anything.

**What each call answers** is under its **Returns**, and every error value it can give is in the table under its **Errors** ([Reading an Errors table](./errors.md#reading-an-errors-table)). Every call that acts answers `true` when its input was made and `false, err` when a part of it was not: what Windows did not take or a Mac would not make, and for [`click`](#host-input-click) and [`post`](#host-input-post) also a window that refuses. `cursorPos` and `held` answer what they read. `true` says the input was made, never what a program did with it — and on Windows input that the system drops on its way into a program of higher rights is counted as made, since Windows does not say so. **What a failure leaves pressed**: on a Mac nothing, since a call makes every event before it posts the first. On Windows a call whose events Windows stopped taking part-way sends the releases of what it pressed again, each on its own; only when Windows refuses that second time too does a key or a button stay down. A [`click`](#host-input-click)'s or a [`drag`](#host-input-drag)'s button left so answers `failed` with `the button is still down`, and the host holds it for the module as a [`mouseDown`](#host-input-mousedown)'s: [`held`](#host-input-held) lists it, and the host lets go of it when the module stops. A [posted click](#host-input-click-windows) refused after its press leaves the press posted without a release, which the error's `detail` says. A message is what the log shows, and it may change between versions; the code does not.

**Two raises any call here can meet** are said once, here. A module that did not declare `input` ([below](#declare)) raises at its first reach for `host.input`, naming the capability. And any call raises `stopped by the host: …` when its module has run past its [limits](../module-runtime-and-lifecycle.md#limits). The rest each entry lists under **Raises**. **A coordinate** is read as a number — a string that reads as one is taken too — and cut toward zero, as above; one that is neither, NaN, or outside the 32-bit range raises.

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

### Returns {#host-input-cursorpos-returns}

- `{ x, y }` — a new table at every call, in screen coordinates: device pixels on Windows, points on a Mac. Where the system does not give the position — on Windows while another desktop has the input, the lock screen or a UAC prompt — it is `{ x = 0, y = 0 }`, with a line in the log that calls it a guess: once a session on Windows, each time on a Mac. The build for other systems always answers `0, 0`.

### Errors {#host-input-cursorpos-errors}

None. It never answers an error value.

It never raises; anything passed to it is ignored.

## host.input.move(x, y) {#host-input-move}

Moves the mouse cursor to the given screen coordinates.

**Signature:** `host.input.move(x: number, y: number) -> (boolean, Error?)`

Both arguments are integers. Answers `true` once the pointer is there, `false, err` when the system did not put it there ([Returns](#host-input-move-returns)).

```luau
host.input.move(960, 540)
```

While a button is held, a move is a **drag**: a knob or a slider that holds the press follows the pointer. That is how a press, a glide and a release are composed from [`mouseDown`](#host-input-mousedown), moves and [`mouseUp`](#host-input-mouseup) — one move per step the module decides, each sent at once and not paced.

```luau
-- A knob turned by holding its press: down, a few points up, let go where the moves left it.
local x, y = 1276, 852
host.input.mouseDown(x, y)
for step = 1, 5 do host.input.move(x, y - step) end
host.input.mouseUp(x, y - 5)
```

### Returns {#host-input-move-returns}

- `true` — the pointer was put there, or moved there as motion. The build for other systems answers `true` and moves nothing.
- `false, err` — it was not; `err.code` is under Errors. The pointer stays where it was.

### Errors {#host-input-move-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `failed` | return | Without a button held: `SetCursorPos` refused, with its error — another desktop has the input, the lock screen or a UAC prompt, say. | `Windows did not put the pointer there (error …)` | Windows |
| `failed` | return | With a button held: `SendInput` did not take the motion, with its error. | `Windows did not take the input (error …)` | Windows |
| `failed` | return | The system would not make the movement's event. Nothing was sent, and the log says where. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was sent` | macOS |

**Raises** for a mistake in the call: an `x` or `y` that is not a coordinate (see the top of the page).

### Windows {#host-input-move-windows}

Without a button held, the pointer is **placed** (`SetCursorPos`): no event goes through the system's input, so no low-level hook or raw input sees it. A control that holds a press and follows the pointer sat still through such placements — ON:EAR's Width slider, measured — which is why a move with a button held is motion instead.

While a button is held — one a module pressed with `mouseDown` and has not let go of, or one down on the desktop (the high bit of `GetAsyncKeyState`: the user's own hand on the mouse) — the move is **one injected absolute motion event** instead: `SendInput` with `MOUSEEVENTF_MOVE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_ABSOLUTE`, the point placed over the whole virtual desktop at the middle of its pixel's share of the 0–65,535 axis, so that it lands on that pixel on any monitor; a point off the desktop goes to its nearest edge, where the system keeps the pointer. The event carries no button: the system puts the held button into the window's `WM_MOUSEMOVE` (`MK_LBUTTON`, `MK_RBUTTON`) itself, as for a hand on the mouse, so a control holding the press sees motion it follows. Absolute rather than relative, because a relative move goes through the pointer's speed and "Enhance pointer precision" and lands somewhere else. One event per call, not paced and not marked against coalescing — `drag` paces — costing one `SendInput`. A low-level mouse hook sees it as injected motion, which a placement never was.

### macOS {#host-input-move-macos}

A real move event is posted — or a **drag** event of the held button when one is currently held, which is also what the implicit move inside `mouseDown` and `mouseUp` does.

So the press-then-glide-then-release gesture, composed from `mouseDown`, moves and `mouseUp`, is a genuine drag on both platforms: a control that follows the pointer while its button is held follows it on a Mac and on Windows alike. Where the pace of the movement matters — a control that reads the *speed* of a drag — use `host.input.drag`, which paces it on both.

## host.input.click(x, y, opts?, cb?) {#host-input-click}

Clicks at `(x, y)`, through the system's input or, experimentally, without bringing the window it clicks to the front.

**Signature:** `host.input.click(x: number, y: number, opts: { button?: "left" | "right" | "middle", window?: number, via?: "input" | "post" | "noactivate", move?: boolean }?, cb: ((made: boolean, err: Error?) -> ())?) -> (boolean, Error?)`

`x`/`y` are integers. The optional `opts.button` selects the button; it is matched case-insensitively, with `"right"` and `"middle"` recognized and anything else (including omitting `opts`) defaulting to **left**.

It answers `true` when the click was made — with `via = "noactivate"`, when it was handed to the thread that makes it — and `false, err` when it was not ([Returns](#host-input-click-returns)). `true` never says that the program did anything with the click: no program answers one. Without `window` a click is never refused, so a module that ignores the answer works as it always did.

- **`window`** — the window the click is for: an id from [`host.window`](./window.md) or an overlay's `O:hwnd()`, never a number you made up. With it the click is made only when the point is in that window, or in a window inside it, and nothing else is drawn over it there; otherwise nothing is clicked, and the answer is `false, err` with `gone`, `outside` or `covered` — and on Windows `denied`, when the window's program runs with higher rights than this application and would never get the click ([Errors](#host-input-click-errors)). Each refusal is a line in the log: `[input] click at 940,310 for window 2755348 (via = "post") refused: something else is drawn over the point`.
- **`via`** — how the click is made, chosen for each call:
  - `"input"`, the default: the pointer moves to the point and the click goes through the system's input, exactly as without `via`. A click into a window that is not in front brings it to the front — the system activates a window that is clicked — so a page that clicks into a plug-in window loses the keyboard to it and has to be brought back: the round trip, which a screen reader announces as the focus leaving and coming back.
  - `"post"`, which needs `window`: the move, the press and the release are posted to the window — as messages to it on Windows, as events handed to its program on a Mac — at the point, the pointer moved there first. The host brings nothing to the front, and the window in front keeps the keyboard. Whether the program reacts is its own business, and so is what it does then: most toolkits read a click from what is posted, one that reads the mouse some other way never sees it, and one can bring its own window forward on a click — REAPER brought its FX window forward on Windows, and a JUCE plug-in asks for its DAW's activation on a Mac (see the platform blocks).
  - `"noactivate"`, which needs `window`, on Windows only — a Mac answers `false, err`, code `not_supported`, message `not supported on this platform yet`, as it is a Windows window style: the window's top-level window is given a style that keeps a click from bringing it to the front, and is then clicked through the system's input, as `"input"` clicks. Only a window another window owns gets the style — a plug-in's floating window — never a program's main window. The style is changed and the click made on a thread of the host's, after the call returns; the window keeps the style until another window with a title comes to the front, the module stops or the application ends, and then has it taken off again; a run that ends without taking it off — a crash — takes it off at its next start.
- **`move`** — only with `via = "post"`, and **experimental**, for the HTML probe's comparison of the two (`tools/html-probe`, W10 on both platforms): `true` by default, the pointer put at the point before the click — on a Mac without an event of its own; on Windows `SetCursorPos` also makes Windows send the window under the pointer a `WM_MOUSEMOVE` of its own, besides the posted one; `false` leaves the pointer where it is. Some toolkits ask where the real pointer is while they handle a click — JUCE on Windows turns a press into a drag to it — so a posted click without the pointer can be lost; whether any plug-in needs this is what the probe measures, and the option goes again unless one does.
- **`cb`** — called once, on a later tick of the event loop, with `(true)` or `(false, err)`: for `"input"` and `"post"` the answer the call returned, for `"noactivate"` whether the click was made on its thread — what goes wrong on that thread reaches the callback only, and without one the log alone (Windows). It runs as an event of the module, after whatever the module is busy with, and not at all when the module is disabled, reloaded or removed first; an error in it is reported as any callback's is.

**What the program sees** of a posted click: a move, a press and a release at the point, in its own coordinates whatever its display scaling; the button, with the modifiers as each platform's block says; no double click is made of two calls, though a program that times clicks itself may take two quick ones for one. **Not for** what goes on inside the plug-in after the click — a menu (JUCE closes its own menus when its DAW is not in front, and a system menu needs its window in front), a text field (keys go to the window in front), a dialog, a drag: those take the plug-in to the front, the round trip.

**Cost:** on Windows `"input"` microseconds, and with `window` a few more local calls that ask nothing of any program — three for the point, and the rights of the program the click goes into, read from its process (this application's own once); `"post"` the same and three posted messages — it never waits for the other program; `"noactivate"` the checks on the event loop, then the rest on the host's `window-style` thread, where the first click for a window waits up to 1000 ms for the window's program to answer, and then for the style change, which has no bound (see Windows). On a Mac `"input"` the same microseconds, and with `window` the window server's hit test; `"post"` on the event loop: a question to the system's privacy database, one read of the window server's list of windows on screen, the hit test, at most accessibility reads of the window's place — a control's rectangle, two attributes; a window's content, measured once and remembered, mostly before its id reached the module (`host.window` measures it to give the window's `client`), but measured again on every click while it cannot be measured, a read of the window's children and one of each — each read bounded at one second as every accessibility read is, and none asked of a program that did not answer a moment ago, though one that stops answering at the click can hold the event loop for several seconds once (a read that times out is asked again attribute by attribute) and is then refused at once for five seconds; and three events handed to the program, which never wait for it (see macOS). Every way that makes its click turns over [`host.inputEpoch()`](./timer.md#host-inputepoch) at the call, and a click refused at the call turns over nothing.

```luau
host.input.click(300, 200)                       -- left click
host.input.click(300, 200, { button = "right" }) -- right click

-- A page's "Next preset": the plug-in's arrow, clicked without taking the page away, on Windows
-- and on a Mac. Needs "input" and "window" in the manifest; `plugin` is the plug-in window's table.
local ok, err = host.input.click(x, y, { window = plugin.id, via = "post" })
if not ok then
  host.log.info("no click without the front (" .. err .. "): the plug-in window has to come forward")
end

-- The same with the style, its outcome told on a later tick.
host.input.click(x, y, { window = plugin.id, via = "noactivate" }, function(made, err)
  if not made then host.speech.output("Not clicked: " .. err) end
end)
```

### Returns {#host-input-click-returns}

- `true` — the click was made, through the system's input or posted. With `via = "noactivate"`: it was handed to the host's `window-style` thread, which makes it after the call returns, and only a callback hears what came of it. `true` never says that the program did anything with the click.
- `false, err` — the click was not made; `err.code` is one of the codes of the first table below. Without `window` a click is never refused, though the system can fail to take it. Through the system's input, a press Windows took without its release has the release sent again on its own; refused that second time too, the button is still down, and it is held for the module from then on, as one pressed with [`mouseDown`](#host-input-mousedown) — for `"noactivate"` once the outcome has come back. A posted click refused at its release leaves its move and its press posted, and the press has no release: its `detail` says so, and the program may take the button for down until a release of the user's or of another click. A button held with `mouseDown` stays held when the click was not made, and with `"noactivate"` until the click is made, on the tick its outcome comes back.
- With a callback: the call answers as above, at once, and `cb(true)` or `cb(false, err)` comes once, on a later tick. For `"input"` and `"post"`, and for a `"noactivate"` refused at the call, it is the answer the call gave; for a `"noactivate"` handed over, what came of it on the thread, with a code of either table. It does not come when the module is disabled, reloaded or removed first.

### Errors {#host-input-click-errors}

None of these is raised. **At the call**, refused before anything is clicked — returned, and handed to the callback too:

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `gone` | both | With `window`: no window has that id now — on a Mac only with `"post"`, as `"input"` there clicks for an unknown id. On Windows, for `"noactivate"`, also its top-level window gone; on a Mac, also a window number the window server has given another program's window since. | `the window is gone` | both |
| `outside` | both | With `window`: the point is not in the window's rectangle — on a Mac a control's own, for a control's id. On a Mac only with `"post"`. | `the point is not in the window` | both |
| `outside` | both | `"post"`: the point is on the window's frame or title bar, not in its client area, where a real click is something else. | `the point is on the window's frame, not in its client area` | both |
| `covered` | both | With `window`: another window is drawn over the point. On a Mac only a window known to be drawn there ([macOS](#host-input-click-macos)). | `something else is drawn over the point` | both |
| `denied` | both | `"post"`: the window's program runs with higher rights (error 5). `"input"` with `window`: the program of the window the click would go into runs with higher rights than this application — an elevated one — so Windows would drop the click on its way in; asked before the pointer moves, and nothing is clicked. | `the window's program runs with higher rights` | Windows |
| `failed` | both | `"post"`: Windows refused a message with another error, its number in the message. What was posted before it — the move, or the move and the press — stays posted, and the error's `detail` says which: a refused post is not tried again, since what refused it — a window gone, a full message queue — refuses a second one. | `the window did not take the message (error …)` | Windows |
| `refused` | both | `"noactivate"`: the window's top-level window is a program's main window — nothing owns it, or it is marked for the taskbar — which never gets the style. | `the window belongs to a program's main window, which never gets the no-activate style: only a window another window owns — a plug-in's floating window — does` | Windows |
| `cancelled` | both | `"noactivate"`: the application has begun to end. | `the application is ending` | Windows |
| `denied` | both | `"post"`: this application has no Accessibility grant, without which the window server drops posted events without a word. Asked first. | `this application may not post events: Accessibility is not granted to it (System Settings > Privacy & Security > Accessibility)` | macOS |
| `hidden` | both | `"post"`: the window could not be paired with a number in the window server's list — for a control, the window it is in. | `the window has no number in the window server's list` | macOS |
| `hidden` | both | `"post"`: the window is not on screen: minimised, hidden, or on another Space. | `the window is not on screen` | macOS |
| `timeout` | both | `"post"`: the window's place could not be read from its program, or the program did not answer a moment ago. | `the window's program did not answer` | macOS |
| `not_supported` | both | `"post"`: this macOS lacks `CGEventSetWindowLocation`, which gives a posted click its place in the window. Nothing was posted. | `this macOS cannot give a posted click its place in the window (CGEventSetWindowLocation is missing)` | macOS |
| `failed` | both | `"post"`: the system would not make one of the three events. Nothing was posted. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was posted` | macOS |
| `not_supported` | both | `"noactivate"` on a Mac, as it is a Windows window style; `"post"` and `"noactivate"` on the build for other systems. | `not supported on this platform yet` | macOS, other |
| `failed` | both | `"input"`: `SetCursorPos` refused, with its error — another desktop has the input, say. Nothing is clicked. | `Windows did not put the pointer there (error …)` | Windows |
| `failed` | both | `"input"`: `SendInput` did not take the press and the release, with its error. A press it took has its release sent again on its own, which Windows took: nothing is left down. | `Windows did not take the input (error …)` | Windows |
| `failed` | both | `"input"`: Windows took the press and refused its release, with the click and again on its own. The button is still down, and held for the module ([`held`](#host-input-held)), which the host lets go of when the module stops. | `Windows took the press but not its release (error …): the button is still down` | Windows |
| `failed` | both | `"input"`: the system would not make one of the click's three events. Nothing was sent. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was sent` | macOS |

**After the hand-over** of a `"noactivate"` click, on the host's `window-style` thread: the call has answered `true`, so these reach the **callback only** — without one, the log alone. Nothing is clicked.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `gone` | callback | The window went before the thread came to it, or its handle names another window now. | `the window is gone` | Windows |
| `cancelled` | callback | The application began to end before the style was given. | `the application is ending` | Windows |
| `denied` | callback | The window's program runs with higher rights: it refused the question asked first, or the style (error 5). | `the window's program runs with higher rights` | Windows |
| `timeout` | callback | The program did not answer the question asked before its style is changed: within 1000 ms, or 500 once the application is ending. | `the window's program did not answer within … ms` | Windows |
| `failed` | callback | That question failed with another error. | `the window's program did not answer (error …)` | Windows |
| `failed` | callback | The file of the windows to give their style back could not be written; the message names it, with the system's words. The style is not given. | `the list of windows to give their style back could not be written (…)` | Windows |
| `failed` | callback | Windows refused the style with another error, and the window does not hold it. | `the window did not take the style (error …)` | Windows |
| `refused` | callback | The style was set, and the window does not hold it: its program set it back, or never took it. | `the window did not keep the no-activate style` | Windows |
| `failed` | callback | The host's thread for it could not be started, is gone, or failed in this click: a fault of the host's, with more in the log. | `the host could not start its window-style thread: …`, `the host's window-style thread is gone`, `the host's thread for it failed: …` | Windows |
| `failed` | callback | The style was given, and `SetCursorPos` refused to put the pointer at the point: nothing is clicked. | `Windows did not put the pointer there (error …)` | Windows |
| `failed` | callback | The style was given, and `SendInput` did not take the press and the release; a press it took has its release sent again on its own, which Windows took. | `Windows did not take the input (error …)` | Windows |
| `failed` | callback | The style was given, and Windows took the press and refused its release, twice. The button is still down, and held for the module from this tick, or let go of at once when the module has stopped running meanwhile. | `Windows took the press but not its release (error …): the button is still down` | Windows |

**Not answered**: input Windows drops on its way into a program of higher rights, when the click has no `window`. `SendInput` counts it as taken, so such a click answers `true`: only a click given a `window` can be asked about, and is refused for it, `denied`.

**Raises** for a mistake in the call, each naming the option: an `x` or `y` that is not a coordinate (see the top of the page); `opts` that is neither a table nor `nil`; a `window` that is not a whole number; a `via` other than `"input"`, `"post"` and `"noactivate"`; a `via` other than `"input"` without a `window`; a `move` that is not a boolean, or `move = false` with a way other than `"post"`; a `cb` that is neither a function nor `nil`. A `button` that is not one of the three names is left, never raised for, and so is any other key of `opts`.

### Windows {#host-input-click-windows}

**`via = "input"`.** Whatever the user is **physically holding is inherited** by the injected click, so one posted from a hotkey callback while Alt is still down is an Alt+click. The overlay runtime waits for the modifiers to come up before clicking for exactly that reason; a module clicking from its own hotkey handler needs the same care.

**`window`.** The window `WindowFromPoint` names at the point has to be `window` or a window inside it (`IsChild`); `outside` — `the point is not in the window` — goes by `window`'s own rectangle. With `"input"`, the program of that window must run with no higher rights than this application: the integrity level of its process (`OpenProcess`, the token's `TokenIntegrityLevel`) no higher than this application's, unless this application holds UIAccess. Otherwise Windows drops the click on its way in (the User Interface Privilege Isolation) while `SendInput` counts it as taken, so it is refused — `denied`, as a posted click is — and the pointer does not move. A program whose level cannot be read — one of the system's protected processes, say — is not refused, and its click may be dropped as before. `WindowFromPoint` does not ask another program's windows whether they let a click through, so a transparent window a toolkit lays over its content — JUCE's over an OpenGL editor — is taken for the window drawn there.

**`via = "post"`.** `WM_MOUSEMOVE` and the button's down and up messages, posted with `PostMessageW` back to back to the window `WindowFromPoint` names at the point — a child window, the plug-in's own, where the plug-in has one: nothing of the click reaches the DAW around it. The point is converted to that window's client coordinates for its DPI mode: a per-monitor aware window reads physical pixels, a system-aware or unaware one its own, scaled by its DPI over its monitor's. With `move` (the default) the pointer is put at the point first with `SetCursorPos`, so that `GetCursorPos` and `GetMessagePos` agree with the click; the move makes Windows send the window under the pointer a `WM_MOUSEMOVE` of its own as well, before the posted one, which `move = false` does not. Nothing goes through the system's input: no hit test is asked of the program, no `WM_MOUSEACTIVATE` is sent, nothing is activated, no low-level hook sees it. The `wParam` carries the button and `MK_CONTROL` and `MK_SHIFT` as `GetAsyncKeyState` has them; every other modifier the program reads only from the keyboard — as for every click, wait for a hotkey's modifiers to come up. Refused as well ([Errors](#host-input-click-errors)): a point on the window's frame or title bar, where a real click is another message; a program of higher rights — an elevated one, error 5, into which an injected click fails without saying so; and any other error `PostMessageW` answers, by its number. One line in the log per posted click, naming the window it went into and its DPI mode — where a click landing beside its target shows, as it would for a window that is not per-monitor aware on a scaled monitor:

`[input] posted click for window 2755348 ('#32770'): into 3017492 ('JUCE_18f3a2b4c1d') at client 120,45; that window is per-monitor v2, monitor 150 %; pointer moved: yes`

**`via = "noactivate"`.** The top-level window of `window` is given the extended style `WS_EX_NOACTIVATE`, with which the system does not activate a window that is clicked, and the click is made with `SetCursorPos` and `SendInput` as `"input"` makes it. Only an **owned** top-level window gets it — a plug-in's floating window, which its DAW's window owns. One that nothing owns or that is marked for the taskbar (`WS_EX_APPWINDOW`) — a program's main window, a standalone instrument's, a plug-in docked in its DAW's window — is refused at the call (`refused` — `the window belongs to a program's main window, which never gets the no-activate style: …`), since it could lose its taskbar button and its place in Alt+Tab while it held the style. Changing another program's window waits for that program, so it is done on the host's `window-style` thread, never on the event loop: the program is asked first whether it answers (`WM_NULL`, up to 1000 ms), then the style is set with `SetWindowLongPtrW` — which waits for the program **without a bound**: one that answers the question and then hangs holds the thread, and every click and give-back after it, until it answers — and read back: a style that is on is held, whatever the call answered. The click follows on the same thread. **The style is held** — taken off right after the click, the change would reach the program before the click does, and the click would bring the window forward after all — until another window comes to the front, the module that clicked stops (disabled, reloaded, rolled back or stopped by the host) or the application ends, and is then taken off on the same thread, after every click still on its way. Another window means one with a title: the host has no window event for a window without one, so an untitled window in front ends nothing until a titled one follows it. At the end the application hands that thread a last give-back and waits for it up to 2 seconds in all, each program having half a second to answer; a window not given its activation back by then — its program did not answer, or hangs as the style changes — keeps its line in the file below, and the next start gives it back. From the moment the application begins to end, no window is given the style: a click still on its way is refused with `cancelled` — `the application is ending`. A window that has the style of its own is clicked as it is and left alone. A program that brings its window forward itself on a click still can: the click is real input, which gives it the right to.

Every window given the style is written to `held-window-styles.txt` beside the application **before** it is given — its handle, its process and when that process started, its class — and the file is rewritten as the style is taken off, and deleted when no window holds it. A run that ends without taking it off again, a crash or a kill, takes it off at the next start, before any module can click, wherever the window is still the one named and still has the style; until then that window does not come to the front when clicked (the keyboard and the DAW's own commands still bring it forward), and closing it in the DAW ends that too. Refused on the thread, for the callback only — without one it is in the log alone — for the reasons of the second table under [Errors](#host-input-click-errors): the window gone meanwhile, its program not answering the question or of higher rights, the file above not written, the style not taken or not kept, the application ending, and the host's own thread. Each refusal is a line in the log as well, callback or not — `[input] click at 940,310 for window 2755348 (via = "noactivate") refused: the window did not keep the no-activate style` — and so is what went right:

`[input] window 2755348 ('#32770') of process 4242 given the no-activate style for clicks that leave the window in front where it is`

`[input] clicked window 2755348 ('#32770') at 940,310 without activating it`

`[input] window 2755348 ('#32770'): the no-activate style given back — another window came to the front`

### macOS {#host-input-click-macos}

**`via = "input"`.** The event's modifier flags are **cleared**, so a click is a bare click whatever is held — the same treatment `send` gives a key, and the same for `scroll`, `drag` and the `mouseDown`/`mouseUp` pair. Added when the shortcut that puts the keyboard back into a plug-in was found to click while its five-key chord was still down. What such a click would have done was not measured: by the AppKit convention a Control-modified left click is a secondary click, and the chord in question holds Control among four others, so a context menu is the likely reading rather than an observed one.

**`window`** is checked as [`host.window.ownsPoint`](./window.md#host-window-ownspoint) checks a point: only a window known to be drawn over the point refuses, with `"something else is drawn over the point"` — VoiceOver's windows and this application's own other than a page are not taken for one. With `"input"` that is all; `"post"` asks more of the window (below).

**`via = "post"`.** The move, the press and the release are `CGEvent`s handed to the window's process with `CGEventPostToPid`, back to back. That way passes the window server's routing by — no hit test of its own, no activation, no pointer moved — so each event says where it goes: the window's number in the window server (its `CGWindowID`: the window's own, or for an id of something inside a window, that window's), in the two public fields `kCGMouseEventWindowUnderMousePointer` and `kCGMouseEventWindowUnderMousePointerThatCanHandleThisEvent` and in the private one AppKit's own events carry it in (51); and the point's place in that window, from its top-left corner, set with the private `CGEventSetWindowLocation`. AppKit in the program hands the events to that window, and the window to the view under the point, while the window's application stays in the background. The events' modifier flags are cleared, as for every click here, so a click from a hotkey's callback is a bare click; each is marked as the host's own in `kCGEventSourceUserData`. With `move` (the default) the pointer is warped to the point first with `CGWarpMouseCursorPosition`, so that a program asking where the pointer is (`[NSEvent mouseLocation]`) finds it on the click. The warp posts no event of its own — the program sees the posted move only — and after a warp macOS holds the mouse's own movement off the pointer for about a quarter of a second. The host joins the two again at once (`CGAssociateMouseAndMouseCursorPosition`, as SDL, GLFW and Wine do after their warps), which gives the user's mouse the pointer back straight away while this application is the active one — Apple describes the call for an application in the foreground. A posted click is mostly made while it is not (a page described `activates = false`, an overlay's key with the DAW in front), and what the call does then is not measured: the user's mouse may stall for that quarter of a second after each such click (TODO.md, MW21). `move = false` leaves the pointer where it is.

Refused as on Windows where Windows has the case — the window gone (no window the host knows by that id, or its number now belongs to another program's window), the point outside its rectangle (a control's own for a control's id) or on its frame (a window's title bar or border, where a click would move or zoom the window), something drawn over it (the hit test of `window` above) — and for reasons of the Mac's own, each in the table under [Errors](#host-input-click-errors). One of them needs its reason here: `CGEventPostToPid` answers nothing, and without the Accessibility privilege the window server drops the events without a word, so the host asks `CGPreflightPostEventAccess` first and answers `denied` rather than `true`; the start block's `hid post events` line names the same privilege. One line in the log per posted click, naming the process and the window it went into, and the place in the window the events carry — where a click landing beside its target shows:

`[input] posted click for window 12: into window number 8812 of process 4242 (Logic Pro X) at 240,160 from the window's top-left corner (340,210 on the screen); pointer moved: yes`

**What the program then does is its own, and three things decide it here.** AppKit's first-mouse rule: a press into a window that is not its application's key window may make it the key window there — the application still not the active one — and reaches the view only when that view takes the first mouse; a view that does not (many custom views, SwiftUI's tap gestures, the web view of a page) may get nothing of that click. Measured on the CI's Mac runners (macOS 14 to 26, `page-check`'s `posted_unkeyed`): two clicks into a page whose window did not have the keyboard, with another application in front, reached nothing, and the page's window did not become the key window; clicks into the same page while it had the keyboard landed where they were aimed. Whether a later click lands while the application stays in the background is not measured: the work this rests on saw such clicks dropped, a custom view's and a SwiftUI tap's, and needed a step of its own first that brought the application forward or gave it the focus — so posting twice is no remedy to count on; the CI's `page-check` records what a page's web view does with two (`posted_unkeyed`). JUCE takes the first mouse, reads the click's place from the event rather than from the pointer, and **brings its own application forward on a press**: a JUCE view's mouse-down asks for its application's activation (`activateIgnoringOtherApps:`) whenever that application is not the active one — for a plug-in, its DAW — so a posted click into a JUCE plug-in may bring the DAW to the front by the plug-in's doing, not the host's; whether macOS 14 and later grants a request that no real click came with is not measured. And a plug-in drawn by another process — an Audio Unit its DAW runs out of process — is a view of that process's inside the DAW's window, which the events handed to the DAW's process may not reach. Nothing of this has run in a plug-in yet: the CI's `page-check` posts clicks into a page of the application's own on every Mac runner, and a session in Logic is to say what plug-ins do (TODO.md, MW21). Not measured either: a program that reads the mouse elsewhere than from its events (a game, a canvas engine).

**`via = "noactivate"`** answers `false, err` — `not_supported`, `not supported on this platform yet` — and so does the callback: it is a Windows window style. A module written for both platforms takes `"post"` or the round trip here.

## host.input.drag(x1, y1, x2, y2, opts?) {#host-input-drag}

Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases — with real, paced movement in between.

**Signature:** `host.input.drag(x1: number, y1: number, x2: number, y2: number, opts: { button?: "left" | "right" | "middle" }?) -> (boolean, Error?)`

All four coordinates are integers. `opts.button` behaves exactly as in `click` (defaults to left). Answers `true` once the drag is made, `false, err` when a part of it was not ([Returns](#host-input-drag-returns)).

```luau
host.input.drag(100, 100, 400, 300)
host.input.drag(100, 100, 400, 300, { button = "middle" })
```

A drag is not a press, a warp and a release. On Windows the pointer is moved with injected move events rather than `SetCursorPos` — over the virtual desktop, on any monitor — interpolated over sixteen steps with a few milliseconds between them, because two kinds of control tell the difference: one that watches for motion while its button is held sees injected events and can miss a warp entirely, and one that reads the *speed* of a drag answers an instantaneous jump with an enormous change. macOS posts a drag event between press and release, which it has always done. The pacing blocks the calling thread for about sixty milliseconds, which is only ever paid when somebody deliberately drags something.

If a control needs a sustained press before it will react at all, use `mouseDown`/`mouseUp` with a timer instead — that is why those exist separately.

### Returns {#host-input-drag-returns}

- `true` — the press, the movement and the release were made. The build for other systems answers `true` and makes nothing.
- `false, err` — a part of it was not; `err.code` is under Errors. On a Mac nothing is sent at all. On Windows a press that was not taken presses nothing, and a step not taken ends the movement there and is followed by the release all the same, sent again on its own when Windows does not take it; refused that second time too, the button is still down, and the error says so: it is held for the module from then on, as one pressed with [`mouseDown`](#host-input-mousedown). A button held with `mouseDown` stays held when the drag answered `false`.

### Errors {#host-input-drag-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `failed` | return | `SetCursorPos` refused to put the pointer at the start: nothing is pressed. | `Windows did not put the pointer there (error …)` | Windows |
| `failed` | return | `SendInput` did not take the press, a step or the release, with its error — the first one it did not take. After a press it took, the release is sent all the same, and again on its own when Windows does not take it. | `Windows did not take the input (error …)` | Windows |
| `failed` | return | Windows refused the release, with the drag and again on its own: the button is still down, and held for the module ([`held`](#host-input-held)), which the host lets go of when the module stops. Said before a step not taken. | `Windows took the press but not its release (error …): the button is still down` | Windows |
| `failed` | return | The system would not make one of the drag's four events. Nothing was sent. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was sent` | macOS |

**Raises** for a mistake in the call: a coordinate that is not one (see the top of the page); `opts` that is neither a table nor `nil`. A `button` that is not one of the three names is left, never raised for.

### Windows {#host-input-drag-windows}

The pointer is moved with injected absolute move events over the virtual desktop — the event a [`move`](#host-input-move) with a button held sends, so a drag lands where it is asked to on any monitor — rather than `SetCursorPos`, interpolated over sixteen steps a few milliseconds apart. The pacing blocks the calling thread for about sixty milliseconds, paid only when something is deliberately dragged.

### macOS {#host-input-drag-macos}

A single `Dragged` event is posted between press and release — no interpolation. That asymmetry is deliberate: a scrollbar needs one intermediate event and gets it. A control that only reacts to *continuous* movement would work on Windows and not here, and nothing has yet exercised the difference on a Mac.

## host.input.scroll(x, y, notches) {#host-input-scroll}

Moves to `(x, y)` and scrolls the mouse wheel by `notches`, which may be fractional.

**Signature:** `host.input.scroll(x: number, y: number, notches: number) -> (boolean, Error?)`

`x` and `y` are integers. `notches` is in wheel notches and **need not be a whole number**; positive scrolls up, negative scrolls down. Answers `true` once the wheel's event is made, `false, err` when it was not ([Returns](#host-input-scroll-returns)).

A notch is 120 units to the operating system, and a control decides for itself what one is worth — ON:EAR's tone knob moves by five of its own units per notch, which is as fine as this could ask for while notches were whole. Half a notch is 60 units, and an application that scales its step by the delta moves by half as much. One that rounds to its own interval instead simply does not move and reports the same value back, which is the honest outcome and says to stop asking for halves.

```luau
host.input.scroll(960, 540, -3)   -- down three notches
host.input.scroll(960, 540, 0.5)  -- half a notch up, where the control is finer than a notch
```

### Returns {#host-input-scroll-returns}

- `true` — the pointer was put at the point and the wheel turned there. It says nothing of how far the control moved. The build for other systems answers `true` and scrolls nothing.
- `false, err` — it was not; `err.code` is under Errors. On Windows a pointer not put at the point scrolls nothing.

### Errors {#host-input-scroll-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `failed` | return | `SetCursorPos` refused to put the pointer at the point: nothing is scrolled. | `Windows did not put the pointer there (error …)` | Windows |
| `failed` | return | `SendInput` did not take the wheel's event, with its error. | `Windows did not take the input (error …)` | Windows |
| `failed` | return | The system would not make the movement to the point. Nothing was sent. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was sent` | macOS |
| `failed` | return | The system would not make the wheel's event. Nothing was sent, and the log says where. | `CGEventCreateScrollWheelEvent2 returned NULL; nothing was sent` | macOS |

**Raises** for a mistake in the call: an `x` or `y` that is not a coordinate (see the top of the page); a `notches` that is neither a number nor a string that reads as one.

### Windows {#host-input-scroll-windows}

The unit is native: 120 units to a notch is what `WHEEL_DELTA` counts, so a fractional request is expressed exactly.

### macOS {#host-input-scroll-macos}

Expressed in Quartz **lines**, which cannot be smaller than one. A caller asking for less than a line gets the smallest whole line rather than nothing — a control that needs a half-step will not get one here until this asks Quartz in pixel units instead. The conversion logs the first scroll of a run, and after that every request that is not a whole notch, saying what was asked for and what was sent; the difference shows up as evidence rather than as a mystery.

## host.input.send(combo) {#host-input-send}

Sends a keyboard shortcut by pressing the modifiers, tapping the key, and releasing in reverse order.

**Signature:** `host.input.send(combo: string) -> (boolean, Error?)`

`combo` is a [key spec](./keys.md#key-spec-string-format), read by the same parser as a hotkey or a capture: `+`-separated parts, the final part being the key and any leading parts modifiers (whitespace around parts is trimmed, case does not matter). The modifiers are the four roles: `ctrl` (also `control`, `cmd`, `command`), `alt` (`option`), `shift` and `win` (`super`, `meta`). On a Mac `ctrl` is Command and `win` is Control, so `host.input.send("Ctrl+S")` is Ctrl+S on Windows and Command+S on a Mac, the application's own Save on both. For the Mac's Control key, write `Meta`. The key is one of the [key names](./keys.md#key-spec-string-format) — letters, digits, `F1`–`F24`, fourteen named keys and the numeric keypad's sixteen (`Numpad0`–`Numpad9`, `NumpadEnter`, `NumpadAdd`, `NumpadSubtract`, `NumpadMultiply`, `NumpadDivide`, `NumpadDecimal`); there is no punctuation, `Insert` or media keys, and there is no separate key-down or key-up. A spec that does not parse raises ([Raises](#host-input-send-errors)); what only the moment decides — on a Mac, a key the keyboard layout cannot type — is answered ([Returns](#host-input-send-returns)). It turns over [`host.inputEpoch()`](./timer.md#host-inputepoch) when a key event went out — after a send that answered `true`, and after one Windows took part of and answered `false` — and not for one that raised or that answered `false` with nothing sent: a refused send acted on nothing. The counter moves once the events are handed to the system, before the program in front has read them. Until 2026-10-24 it moved at the call, before anything was checked.

The key goes through the same keyboard hook as a real one, so a combination that any module has [captured](./keys.md#host-keys-capture) is caught and suppressed again — sending the key you captured does not pass it on. See [`post`](#host-input-post) for that.

**It ends no count of quick repeats**, unless a capture or a hotkey takes it. Its events carry a mark the hook and the tap know a sent key by, and a key-down with it is left out of what ends a count ([`press.count`](./hotkey.md#quick-repeats-once-twice-three-times)), so a callback can send a key at every press of its hotkey or captured key and the next press counts on. The mark changes nothing else: a capture or a hotkey takes the key as it would a typed one — a press of its own, counted as a typed one is, which ends the count when it is of another key — and to a key a busy module let go of it is a key that reached the program ([`host.keys`](./keys.md)).

```luau
host.input.send("Ctrl+S")         -- the application's Save, on either platform
host.input.send("Ctrl+Shift+Esc")

-- A key the layout may not type on a Mac: say so rather than nothing.
local ok, err = host.input.send("Ctrl+F21")
if not ok and err.code == host.errors.NOT_SUPPORTED then
  host.speech.output("This key cannot be sent here.")
end

-- A picture cached against host.inputEpoch() is read again only after a key that went out.
local before = host.inputEpoch()
if host.input.send("Ctrl+S") then
  assert(host.inputEpoch() ~= before)   -- sent: the counter moved
end
```

### Returns {#host-input-send-returns}

- `true` — the key was sent, with its modifiers. It says nothing of what the program in front did with it.
- `false, err` — it was not; `err.code` is one of the codes below. On a Mac nothing is sent. On Windows a key or a modifier Windows took the press of has its release sent again on its own; only one Windows refuses that second time too stays down, and the log's line says how many of the releases went. The host holds no record of keys, so nothing lets go of such a key later.

### Errors {#host-input-send-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_supported` | return | No key of the current keyboard layout types the key, or macOS has no key code for it (`F21`–`F24`). The message names the spec, its virtual-key code and why. Nothing is sent. | `'…' (VK …): …` | macOS |
| `failed` | return | The system would not make the key-down's or the key-up's event: both are made before either is sent, so nothing is sent. | `CGEventCreateKeyboardEvent returned NULL for '…' (keycode …)` | macOS |
| `failed` | return | `SendInput` did not take every event — the modifiers' presses, the key's press and release, the modifiers' releases, in one call — with its error. The releases of what it took the press of are sent on their own. | `Windows did not take the input (error …)` | Windows |
| `not_supported` | return | The build for other systems, which has no input. | `not supported on this platform yet` | other |

**Not answered**: a key Windows drops on its way into a program of higher rights, which `SendInput` counts as taken.

**Raises** for a mistake in the call, naming the part: a `combo` that is not a string (a number is read as its digits: `5` is the key 5); an empty combo; an unknown modifier; an unknown key; a modifier tap (`"Ctrl tap"`), which is something to capture, not a key to send.

### Windows {#host-input-send-windows}

Each role is the key of its name. The modifiers are synthesised as real key presses around the key — pressed in the order Ctrl, Alt, Shift, Win, whatever order the spec wrote them in, and released in reverse — and whatever the user is **physically holding is inherited**. Called from a hotkey callback while Alt is still down, `host.input.send("Escape")` arrives as Alt+Escape — which is why `host.keys.modifiersDown()` exists and why modules defer a synthesised key until the user has let go.

Every key is sent by **virtual-key code only**: `SendInput` with scan code 0, without `KEYEVENTF_SCANCODE` and without the extended-key flag — so the arrows, Home, End, Page Up/Down and Delete go out as their non-extended forms. The keypad's keys go as their own virtual keys (`VK_NUMPAD0`–`VK_NUMPAD9`, `VK_ADD`, `VK_SUBTRACT`, `VK_MULTIPLY`, `VK_DIVIDE`, `VK_DECIMAL`), whatever Num Lock says; its Enter, which has no virtual key of its own, is the one key sent with the extended-key flag: `VK_RETURN` extended, as the keypad sends it, which is how a program tells it from the main Enter. Every press and release goes in one `SendInput` call, which Windows does not interleave with other input, so there is no hold time. Every event carries a mark in its `dwExtraInfo`, which the hook knows a sent key by. An application that reads scan codes, DirectInput or Raw Input — many games — may ignore the key, or read it as a different one.

### macOS {#host-input-send-macos}

`Ctrl` is Command, `Alt` Option, `Shift` Shift and `Win` (`Meta`) Control, so `"Ctrl+C"` copies and `"Meta+C"` is Control+C. The modifiers are set as flags on the event, and setting them **replaces the whole set**, so anything the user is holding is stripped and the same call delivers a bare Escape. No modifier key event is posted at all, so an application that watches for physical modifier presses sees an unmodified key.

The deferral dance is therefore unnecessary for keys here, and since 2026-09-03 not for clicks either: a synthesised click has its flags cleared the same way (see `click`).

A letter is the key that types it under the current keyboard layout — with Command held, when the combo holds Command — as on Windows: `"Ctrl+Z"` is the key labelled Z on a German Mac as on a US one, so `host.input.send("Ctrl+Z")` is Undo on both. Digits and the named keys are US keyboard positions (see [the key spec](./keys.md#key-spec-string-format)). The keypad's keys are its own key codes, and their events carry the keypad's flag (`kCGEventFlagMaskNumericPad`) beside the modifiers', as the keyboard's own events do. Every event carries a mark in its event-source user data (`kCGEventSourceUserData`), which the tap knows a sent key by.

## host.input.text(text) {#host-input-text}

Types a Unicode string as synthetic keystrokes.

**Signature:** `host.input.text(text: string) -> (boolean, Error?)`

`text` is sent character-by-character as Unicode input, not as key presses. Answers `true` once every character is sent, `false, err` when the typing stopped before the end ([Returns](#host-input-text-returns)).

Its characters carry no mark, as [`send`](#host-input-send)'s keys do: each is a key that reached the program, and ends a count of quick repeats ([`press.count`](./hotkey.md#quick-repeats-once-twice-three-times)) as a typed key does. A callback that has to count on sends its key with `send`.

Control characters are sent as characters too: `"\n"` is the character U+000A and `"\t"` is U+0009, **not** a press of Enter or Tab, and whether an application treats those characters like the keys is up to the application. Press keys with [`send`](#host-input-send): `host.input.send("Enter")`.

```luau
host.input.text("Hello, world!")
host.input.send("Enter")   -- not "\n"
```

### Returns {#host-input-text-returns}

- `true` — every character was sent. An empty `text` sends nothing and answers `true`; so does the build for other systems.
- `false, err` — the typing stopped before the end: the characters before it were sent, and the message says how many. On a Mac a character's two events are made before either is sent, so none is half typed. On Windows a character counts as typed once Windows took its press — the press is what delivers it —, and its release is sent again on its own; a character outside the basic plane is two units, and one whose second unit Windows did not take arrives as its first half alone, which the count leaves out.

### Errors {#host-input-text-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `failed` | return | `SendInput` took only part of the text's events — every character's press and release, in one call — with its error. | `Windows did not take the input after … of … characters (error …)` | Windows |
| `failed` | return | The system would not make a character's event: the characters before it were sent. The log has the same line. | `CGEventCreateKeyboardEvent returned NULL while typing; … of … characters were sent` | macOS |

**Not answered**: text Windows drops on its way into a program of higher rights, which `SendInput` counts as taken.

**Raises** for a mistake in the call: a `text` that is not a string (a number is typed as its digits).

### Windows {#host-input-text-windows}

Each UTF-16 unit is sent as a `KEYEVENTF_UNICODE` event (virtual key `VK_PACKET`), down and up, every unit of the text in one `SendInput`. An application that reads scan codes, DirectInput or Raw Input — many games — does not see it.

### macOS {#host-input-text-macos}

Each character is posted as a keyboard event pair with key code 0 and the character as its Unicode payload, with the modifier flags cleared, so the keyboard layout plays no part.

## host.input.mouseDown(x, y, opts?) {#host-input-mousedown}

**Signature:** `host.input.mouseDown(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> (boolean, Error?)`

Presses a mouse button at `(x, y)` and **leaves it down**. This is the half of a gesture that `click` and `drag` cannot express, because both of those press and release in the same breath: a control that only reveals itself while the button is *sustained* — the flyout of extra tools behind Melodyne's tool icon is the case that prompted this API, a gesture that later turned out to be undocumented and was replaced by a key — never appears for either of them. It is split into two calls rather than offered as a timed variant deliberately: holding, gliding and settling is the better part of a second, and a call that slept for that long would sleep in the event pump, stopping speech, hotkeys and detection with it. Split, the caller composes the gesture out of `host.timer` callbacks and the pump keeps running between the parts. There is no screen read here and nothing is asked about what is under the point, so the call itself is cheap.

While your module runs, nothing releases the button for you. A held button belongs to the whole desktop, not to your module, so **every** continuation must end in a `mouseUp` — including the ones that give up because the overlay moved to another window mid-gesture. Pin the window handle before the press and re-check it in each callback, exactly as the timed sequences elsewhere in the runtime do; `opts.button` must match on the way up as it did on the way down. A `mouseUp`, a `drag` or a `click` through the system's input of the same button ends the hold; a posted click (`via = "post"`) does not.

The host releases a button your module holds only when the module stops running: when it is disabled, reloaded, [stopped by the host](../module-runtime-and-lifecycle.md#limits) (a callback that ran too long or out of memory), when its load is rolled back, and when the application ends. It releases it where the pointer is at that moment, so the pointer does not move, and writes a line such as `[input] [com.tool.guard-probe] released the left mouse button it had pressed with host.input.mouseDown: it was stopped by the host` — or, when the system did not take the release, `… could not release the left mouse button …` with why. The release is the desktop's, like the press: it lets go of the button for whatever program is under the pointer. A module that goes on after such a release — enabled again, with a gesture's end still to come — asks [`host.input.held()`](#host-input-held) which buttons are still its own before it lets go.

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

### Returns {#host-input-mousedown-returns}

- `true` — the button is down at the point, and noted as this module's held one ([`held`](#host-input-held)) until its `mouseUp`. The build for other systems answers `true`, presses nothing and notes the button all the same.
- `false, err` — nothing was pressed, and nothing is noted: there is nothing to let go of. `err.code` is under Errors.

### Errors {#host-input-mousedown-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `failed` | return | `SetCursorPos` refused to put the pointer at the point: nothing is pressed. | `Windows did not put the pointer there (error …)` | Windows |
| `failed` | return | `SendInput` did not take the press, with its error. | `Windows did not take the input (error …)` | Windows |
| `failed` | return | The system would not make the movement or the press: both are made before either is sent, so nothing was sent. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was sent` | macOS |

**Raises** for a mistake in the call: an `x` or `y` that is not a coordinate (see the top of the page); `opts` that is neither a table nor `nil`. A `button` that is not one of the three names is left, never raised for.

### Windows {#host-input-mousedown-windows}

The pointer is placed with `SetCursorPos` and then the button-down event is sent. As with `host.input.move` without a button held, the placement is no input: no low-level hook or raw input sees the pointer arrive. The press is noted, so the moves after it are motion ([`move`](#host-input-move)) until its `mouseUp`, a `drag` of the same button, or a `click` of it through the system's input (`via = "input"` or `"noactivate"`); a posted click (`via = "post"`) goes past the system's input and leaves the note.

### macOS {#host-input-mousedown-macos}

A movement event is posted before the press — or a **drag** event if a button is already held — so the press lands on a control that has been told the pointer is there.

## host.input.mouseUp(x, y, opts?) {#host-input-mouseup}

**Signature:** `host.input.mouseUp(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> (boolean, Error?)`

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

### Returns {#host-input-mouseup-returns}

- `true` — the pointer moved to the point and the button was released there; it is no longer in [`held`](#host-input-held). The build for other systems answers `true` and releases nothing.
- `false, err` — a part of it was not made, and **the button may still be down**: it stays in `held()`, so the module can let go of it again, and the host lets go of it when the module stops, as of every button it holds. On Windows a movement not taken is followed by the release on its own, where the pointer is, and the answer is `false` all the same: the release did not land where it was asked, and a button left in `held()` that is already up costs at most a second release at the module's stop, where one taken for up while still down would stay down for the user. On a Mac nothing was sent. `err.code` is under Errors.

### Errors {#host-input-mouseup-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `failed` | return | `SendInput` did not take the movement or the release, with its error; a release not taken with the movement is sent again on its own. | `Windows did not take the input (error …)` | Windows |
| `failed` | return | The system would not make the movement or the release: both are made before either is sent, so nothing was sent. | `CGEventCreateMouseEvent(type …) returned NULL; nothing was sent` | macOS |

**Raises** for a mistake in the call: an `x` or `y` that is not a coordinate (see the top of the page); `opts` that is neither a table nor `nil`. A `button` that is not one of the three names is left, never raised for.

### Windows {#host-input-mouseup-windows}

The motion event a `host.input.move` with a button held sends, to `(x, y)`, then the button-up event, in one `SendInput` — no `SetCursorPos`. So press, glide and release are one drag on Windows as on a Mac: a control that followed the pointer through the moves gets the release where it is. The host's own release at a disable is the same, at the pointer's place: motion of no distance, then the up. Where the pace of the movement matters, `host.input.drag` paces it.

### macOS {#host-input-mouseup-macos}

The move before the release is posted as a **drag** event, because the backend sees a button still held. That is what makes "press here, glide there, let go" a genuine drag on this platform: the control has followed the pointer the whole way and the release lands on the entry it was over.

## host.input.held() {#host-input-held}

**Signature:** `host.input.held() -> { "left" | "right" | "middle" }`

Lists the mouse buttons **this module** pressed with [`mouseDown`](#host-input-mousedown) and has not let go of. They come in the order it pressed them, `{}` for none — never `nil` — in a new table at every call. It is the host's own record: a `mouseUp` or a `drag` of a button takes it off, as does a `click` of it through the system's input (`via = "input"`, or `"noactivate"` once the click is made, on the tick its outcome comes back), though not a posted one (`via = "post"`), which leaves the button held; a `click` or a `drag` whose release Windows refused twice puts its button on it, as the button is still down (`the button is still down`); and the host empties it as it lets go of the module's buttons itself — when the module is disabled, reloaded, [stopped by the host](../module-runtime-and-lifecycle.md#limits) or its load is rolled back ([`mouseDown`](#host-input-mousedown)).

It is **not the desktop's state**, and asks the system nothing: a button the user holds is not in it, nor another module's, and a button the user let go of with the real mouse while the module held it stays in it until the module's `mouseUp`. It acts on nothing — no input is made, and [`host.inputEpoch()`](./timer.md#host-inputepoch) stays as it was. It takes no arguments and costs a look at a table, on the event loop.

What it is for: a held gesture whose end the module can learn of **after a disable and an enable** — a trip's hand-over tells its `onEnd` then ([`trip:handOver`](./overlay.md#trip-handover)). The host let go of the button at the disable; a `mouseUp` after it would release a button nobody holds, over whatever lies under the pointer. So the end lets go once, and only where the record still has the button:

```luau
-- Every end of a held gesture calls this; once is enough, and after a disable the host has let go.
local down = true
local function letGo(x, y)
  if not down then return end
  down = false
  if table.find(host.input.held(), "right") then host.input.mouseUp(x, y, { button = "right" }) end
end
```

### Returns {#host-input-held-returns}

- A list of button names, `"left"`, `"right"` or `"middle"` — in the order this module pressed them, a new table at every call.
- `{}` — this module holds none. Never `nil`.

### Errors {#host-input-held-errors}

None. It never answers an error value.

It never raises; anything passed to it is ignored.

### Windows {#host-input-held-windows}

The host's record, as above: the system is not asked, so a button down on the desktop that this module did not press is not listed. (What a `move` asks besides the desktop's state is the backend's own note of its presses, which is not this.)

### macOS {#host-input-held-macos}

The host's record, as on Windows: the session's button state is not asked.

## host.input.post(id, key) {#host-input-post}

**Signature:** `host.input.post(id: number, key: string) -> (boolean, Error?)`

Delivers a single key to **one window** rather than to whatever has focus. Reach for it when a hotkey has to act *and* still hand the application a key: our own hotkey registration and key capture see synthesised input exactly as they see real input, so `host.input.send` from inside a hotkey callback re-triggers the handler that sent it, forever. A posted key goes to the addressed window (or process) without travelling past that machinery, and is seen by nobody else. `id` is the handle from `host.window.*` or the overlay's `O:hwnd()` — never a number you constructed. `key` is a single [key name](./keys.md#key-spec-string-format) (`A`–`Z`, `0`–`9`, `F1`–`F24`, `Space`, `Enter`/`Return`, `Esc`/`Escape`, `Tab`, `Backspace`, `Delete`/`Del`, the arrows, `Home`, `End`, `PageUp`, `PageDown`, and the keypad's `Numpad0`–`Numpad9`, `NumpadEnter`, `NumpadAdd`, `NumpadSubtract`, `NumpadMultiply`, `NumpadDivide` and `NumpadDecimal`, all matched case-insensitively); **there are no modifiers here** — `"Ctrl+S"` is not a key name and raises, as does any name the table does not know. It answers `true` once the key is posted, or `false, err` when it was not — the window gone, a program of higher rights, a key the layout cannot type ([Returns](#host-input-post-returns)).

Unlike every other call in `host.input` that acts, a posted key does **not** turn over `host.inputEpoch()`, nor `host.epoch()`. So the host's own per-epoch answers are not re-asked either: a `host.window.focusChain()`, `active()` or `host.element.find` that was already asked in this epoch gives the same answer after the post — the one from before the key (see [`host.window`](./window.md)). Anything memoised against that counter — a view state read from pixels, a probe of which sub-tool is active — will not be re-read because you posted a key, so re-read it explicitly after a post sequence, which is what the Melodyne overlay does when it checks with the uncached probe what its presses actually reached.

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

Where the answer matters — a window gone, or a program of higher rights, gets nothing:

```luau
local ok, err = host.input.post(hwnd, "F5")
if not ok then
  host.log.info("F5 not posted: " .. err)
end
```

### Returns {#host-input-post-returns}

- `true` — the key-down and the key-up were posted. It says nothing of whether the program read them.
- `false, err` — the key was not posted, or on Windows only its key-down was: a key-up Windows refuses comes after a key-down it took, and is not tried again, since what refused it — a window gone, a full message queue — refuses a second one. On a Mac both events are made before either is posted, so nothing is posted. `err.code` is one of the codes below. The build for other systems posts nothing, and says so: `false, err`, `not_supported` (until 2026-10-24 it answered `true`).

### Errors {#host-input-post-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `gone` | return | The handle names no window: `0`, or one Windows refuses as no window's (error 1400). | `the window is gone` | Windows |
| `denied` | return | The window's program runs with higher rights (error 5). | `the window's program runs with higher rights` | Windows |
| `failed` | return | Windows refused the post with another error, its number in the message. | `the window did not take the message (error …)` | Windows |
| `not_supported` | return | The key has no key code here: `F21`–`F24`, or a letter no key of the current keyboard layout types. The message names the key, its virtual-key code and why. | `key '…' (VK …): …` | macOS |
| `gone` | return | The handle is not one this backend issued — stale, or a number made up — so there is no process to post to. | `key_post: … is not a handle this backend issued, so there is no process to post '…' to` | macOS |
| `failed` | return | The system would not make the key-down's or the key-up's event. Nothing is posted. | `CGEventCreateKeyboardEvent returned NULL for '…' (keycode …)` | macOS |
| `not_supported` | return | The build for other systems, which has no window to post to. Nothing is posted. | `not supported on this platform yet` | other |

**Raises** for a mistake in the call: an `id` that is neither a number nor a string that reads as one; a `key` that is not one of the key names, with `unknown key '…'` — a combination such as `"Ctrl+S"` included.

### Windows {#host-input-post-windows}

Genuinely per-window: `WM_KEYDOWN`/`WM_KEYUP` are posted to the handle you passed with `PostMessageW`, with the scan code packed into the `lParam` because some applications read that rather than the virtual key — Melodyne decodes keys itself rather than leaving it to the defaults. The scan code is the one `MapVirtualKeyW` gives the key (`MAPVK_VK_TO_VSC_EX`), and a key it gives as extended is posted with the extended-key bit (24) of the `lParam` set, as the keyboard delivers it: the keypad's Divide, which without it is the main row's key of the same scan code (`/` on a US layout, `-` on a German one). The keypad's Enter is posted as `VK_RETURN` with that bit, as the keypad's own key is delivered. The arrows, Home, End, Page Up and Down and Delete go on the scan codes Windows gives them, the keypad's, without it. Posting is also the path most likely to be seen at all here: Melodyne runs its own message pump with its own accelerator table, so a posted `WM_KEYDOWN` reaches `TranslateAccelerator` where a sent one would bypass it. A post Windows refuses is answered, and the key-up is not posted after a refused key-down: `gone` — `the window is gone` (the handle names no window), `denied` — `the window's program runs with higher rights` (error 5), `failed` — `the window did not take the message (error N)`. A post Windows took says nothing about whether the program read it. A posted key exists only as a window message: the keyboard state is not changed, and an application that reads the keyboard through DirectInput or Raw Input — as many games do — never sees it.

### macOS {#host-input-post-macos}

There is no per-window message queue to post into, so this is one step coarser — the event is handed to the window's **process** with `CGEventPostToPid` and lands wherever that application routes it, which need not be the window you addressed. Three consequences a caller can observe: the handle must be one this backend issued, and a stale or foreign number answers `false, err` — `gone`, `key_post: … is not a handle this backend issued, so there is no process to post '…' to` — instead of quietly doing nothing; a key name the shared table accepts can still fail here, because it has to map on to a Quartz keycode as well — a letter maps to the key that types it under the current keyboard layout (see [the key spec](./keys.md#key-spec-string-format)) — and one that does not (`F21`–`F24`, or a letter no key of the layout types) answers `false, err` — `not_supported`, `key '…' (VK …): …`, naming the key and its VK code — and an event the system would not make `failed`, `CGEventCreateKeyboardEvent returned NULL for '…' (keycode …)`; and the modifier flags on the event are **cleared**, not inherited, so a key posted from inside a hotkey callback while its combination is still physically held arrives bare — a key of the keypad with the keypad's flag (`kCGEventFlagMaskNumericPad`) alone, as the keyboard's own event has it. The first post of a run writes a line to the log saying it is routing per process — the line to suspect if a posted key ever surfaces in the wrong window of the same application.
