---
title: "host.input — mouse and keyboard"
sidebar_position: 6
toc_max_heading_level: 2
---

Synthesises mouse and keyboard input: moving and clicking at screen coordinates, dragging, scrolling, sending a key combination, and typing text.

It is what performs every activation an overlay makes. Most modules never touch it, because the overlay's own control kinds click their coordinates for them, so you come here for the gesture that is not one of those — Kontakt's instrument-editor wrench at a fixed offset from the right edge of the window, ON:EAR's tone knob nudged half a wheel notch because a whole one moves it five of its own units, Melodyne's menu bar opened a few pixels above the client origin.

All of it is blind clicking at screen coordinates, and **nothing here asks what is drawn under the point** — except a [`click`](#host-input-click) given the `window` it is for — which is why modules bring their window forward first, and why the overlay runtime refuses a point falling outside its own window. Coordinates are device pixels on Windows and points on macOS (see [`host.screen.size`](./screen.md#host-screen-size)), and every one is converted to a whole number by cutting toward zero without an error. So a centre computed as `x + w / 2` lands on the whole pixel toward zero, which for a negative coordinate — a monitor left of or above the primary — is the opposite direction from `//`; where the nearest pixel matters, round with `math.floor(v + 0.5)`.

Dragging is deliberately not a press, a warp and a release: the movement is paced over sixteen injected steps and blocks the calling thread for about sixty milliseconds on Windows, because a control that reads the *speed* of a gesture answers an instantaneous jump with an enormous change.

**Acting does not wait for a read.** Every call on this page but `cursorPos` acts at once — a click with `via = "noactivate"` is handed at once to the thread that makes it — also while the calling module has a [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) or a [`host.screen`](./screen.md#with-a-callback) call given a callback still out — and so does [`host.window.focus`](./window.md#host-window-focus). A picture that is not taken yet when the input is made may show the screen after it. So a module that reads a field and then clicks it clicks once the read is answered: after the call returns in a handler, where it [waits](./ocr.md#where-it-waits), or in its callback. Whether an input could be in a picture is what the picture's `inputEpoch` says, against [`host.inputEpoch()`](./timer.md#host-inputepoch) as it stood before the input — except after a [`post`](#host-input-post), which does not turn it over, and after a `host.window.focus`, which turns it over only once the window has come forward: after those two, compare the picture's `time` with `host.now()` noted after the act ([`host.inputEpoch`](./timer.md#host-inputepoch)).

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

## host.input.click(x, y, opts?, cb?) {#host-input-click}

Clicks at `(x, y)`, through the system's input or, experimentally, without bringing the window it clicks to the front.

**Signature:** `host.input.click(x: number, y: number, opts: { button?: "left" | "right" | "middle", window?: number, via?: "input" | "post" | "noactivate", move?: boolean }?, cb: ((made: boolean, why: string?) -> ())?) -> (boolean, string?)`

`x`/`y` are integers. The optional `opts.button` selects the button; it is matched case-insensitively, with `"right"` and `"middle"` recognized and anything else (including omitting `opts`) defaulting to **left**.

**Returns** `true` when the click was made — with `via = "noactivate"`, when it was handed to the thread that makes it — and `false` and the reason when it was not. `true` never says that the program did anything with the click: no program answers one. Without `window` a click is never refused, so a module that ignores the answer works as it always did.

- **`window`** — the window the click is for: an id from [`host.window`](./window.md) or an overlay's `O:hwnd()`, never a number you made up. With it the click is made only when the point is in that window, or in a window inside it, and nothing else is drawn over it there; otherwise nothing is clicked, and the answer is `false` and one of `"the window is gone"`, `"the point is not in the window"`, `"something else is drawn over the point"`. Each refusal is a line in the log: `[input] click at 940,310 for window 2755348 (via = "post") refused: something else is drawn over the point`.
- **`via`** — how the click is made, chosen for each call:
  - `"input"`, the default: the pointer moves to the point and the click goes through the system's input, exactly as without `via`. A click into a window that is not in front brings it to the front — the system activates a window that is clicked — so a page that clicks into a plug-in window loses the keyboard to it and has to be brought back: the round trip, which a screen reader announces as the focus leaving and coming back.
  - `"post"`, which needs `window`: the move, the press and the release are posted to the window — as messages to it on Windows, as events handed to its program on a Mac — at the point, the pointer moved there first. The host brings nothing to the front, and the window in front keeps the keyboard. Whether the program reacts is its own business, and so is what it does then: most toolkits read a click from what is posted, one that reads the mouse some other way never sees it, and one can bring its own window forward on a click — REAPER brought its FX window forward on Windows, and a JUCE plug-in asks for its DAW's activation on a Mac (see the platform blocks).
  - `"noactivate"`, which needs `window`, on Windows only — a Mac answers `false, "not supported on this platform yet"`, as it is a Windows window style: the window's top-level window is given a style that keeps a click from bringing it to the front, and is then clicked through the system's input, as `"input"` clicks. Only a window another window owns gets the style — a plug-in's floating window — never a program's main window. The style is changed and the click made on a thread of the host's, after the call returns; the window keeps the style until another window with a title comes to the front, the module stops or the application ends, and then has it taken off again; a run that ends without taking it off — a crash — takes it off at its next start.
- **`move`** — only with `via = "post"`, and **experimental**, for the HTML probe's comparison of the two (`tools/html-probe`, W10 on both platforms): `true` by default, the pointer put at the point before the click — on a Mac without an event of its own; on Windows `SetCursorPos` also makes Windows send the window under the pointer a `WM_MOUSEMOVE` of its own, besides the posted one; `false` leaves the pointer where it is. Some toolkits ask where the real pointer is while they handle a click — JUCE on Windows turns a press into a drag to it — so a posted click without the pointer can be lost; whether any plug-in needs this is what the probe measures, and the option goes again unless one does.
- **`cb`** — called once, on a later tick of the event loop, with `(true)` or `(false, why)`: for `"input"` and `"post"` the answer the call returned, for `"noactivate"` whether the click was made on its thread. It runs as an event of the module, after whatever the module is busy with, and not at all when the module is disabled, reloaded or removed first; an error in it is reported as any callback's is.

Options that do not fit raise, naming the option: a `via` other than the three, a `via` other than `"input"` without a `window`, `move = false` with another way, a `window` that is not a whole number, a callback that is not a function.

**What the program sees** of a posted click: a move, a press and a release at the point, in its own coordinates whatever its display scaling; the button, with the modifiers as each platform's block says; no double click is made of two calls, though a program that times clicks itself may take two quick ones for one. **Not for** what goes on inside the plug-in after the click — a menu (JUCE closes its own menus when its DAW is not in front, and a system menu needs its window in front), a text field (keys go to the window in front), a dialog, a drag: those take the plug-in to the front, the round trip.

**Cost:** on Windows `"input"` microseconds, and with `window` three more local calls that ask nothing of any program; `"post"` the same and three posted messages — it never waits for the other program; `"noactivate"` the checks on the event loop, then the rest on the host's `window-style` thread, where the first click for a window waits up to 1000 ms for the window's program to answer, and then for the style change, which has no bound (see Windows). On a Mac `"input"` the same microseconds, and with `window` the window server's hit test; `"post"` on the event loop: a question to the system's privacy database, one read of the window server's list of windows on screen, the hit test, at most accessibility reads of the window's place — a control's rectangle, two attributes; a window's content, measured once and remembered, mostly before its id reached the module (`host.window` measures it to give the window's `client`), but measured again on every click while it cannot be measured, a read of the window's children and one of each — each read bounded at one second as every accessibility read is, and none asked of a program that did not answer a moment ago, though one that stops answering at the click can hold the event loop for several seconds once (a read that times out is asked again attribute by attribute) and is then refused at once for five seconds; and three events handed to the program, which never wait for it (see macOS). Every way that makes its click turns over [`host.inputEpoch()`](./timer.md#host-inputepoch) at the call, and a click refused at the call turns over nothing.

```luau
host.input.click(300, 200)                       -- left click
host.input.click(300, 200, { button = "right" }) -- right click

-- A page's "Next preset": the plug-in's arrow, clicked without taking the page away, on Windows
-- and on a Mac. Needs "input" and "window" in the manifest; `plugin` is the plug-in window's table.
local ok, why = host.input.click(x, y, { window = plugin.id, via = "post" })
if not ok then
  host.log.info("no click without the front (" .. why .. "): the plug-in window has to come forward")
end

-- The same with the style, its outcome told on a later tick.
host.input.click(x, y, { window = plugin.id, via = "noactivate" }, function(made, why)
  if not made then host.speech.output("Not clicked: " .. why) end
end)
```

### Windows

**`via = "input"`.** Whatever the user is **physically holding is inherited** by the injected click, so one posted from a hotkey callback while Alt is still down is an Alt+click. The overlay runtime waits for the modifiers to come up before clicking for exactly that reason; a module clicking from its own hotkey handler needs the same care.

**`window`.** The window `WindowFromPoint` names at the point has to be `window` or a window inside it (`IsChild`); `"the point is not in the window"` goes by `window`'s own rectangle. `WindowFromPoint` does not ask another program's windows whether they let a click through, so a transparent window a toolkit lays over its content — JUCE's over an OpenGL editor — is taken for the window drawn there.

**`via = "post"`.** `WM_MOUSEMOVE` and the button's down and up messages, posted with `PostMessageW` back to back to the window `WindowFromPoint` names at the point — a child window, the plug-in's own, where the plug-in has one: nothing of the click reaches the DAW around it. The point is converted to that window's client coordinates for its DPI mode: a per-monitor aware window reads physical pixels, a system-aware or unaware one its own, scaled by its DPI over its monitor's. With `move` (the default) the pointer is put at the point first with `SetCursorPos`, so that `GetCursorPos` and `GetMessagePos` agree with the click; the move makes Windows send the window under the pointer a `WM_MOUSEMOVE` of its own as well, before the posted one, which `move = false` does not. Nothing goes through the system's input: no hit test is asked of the program, no `WM_MOUSEACTIVATE` is sent, nothing is activated, no low-level hook sees it. The `wParam` carries the button and `MK_CONTROL` and `MK_SHIFT` as `GetAsyncKeyState` has them; every other modifier the program reads only from the keyboard — as for every click, wait for a hotkey's modifiers to come up. Refused as well: a point on the window's frame or title bar, where a real click is another message (`"the point is on the window's frame, not in its client area"`); `"the window's program runs with higher rights"` (an elevated program, error 5 — an injected click into one fails without saying so); `"the window did not take the message (error N)"`. One line in the log per posted click, naming the window it went into and its DPI mode — where a click landing beside its target shows, as it would for a window that is not per-monitor aware on a scaled monitor:

`[input] posted click for window 2755348 ('#32770'): into 3017492 ('JUCE_18f3a2b4c1d') at client 120,45; that window is per-monitor v2, monitor 150 %; pointer moved: yes`

**`via = "noactivate"`.** The top-level window of `window` is given the extended style `WS_EX_NOACTIVATE`, with which the system does not activate a window that is clicked, and the click is made with `SetCursorPos` and `SendInput` as `"input"` makes it. Only an **owned** top-level window gets it — a plug-in's floating window, which its DAW's window owns. One that nothing owns or that is marked for the taskbar (`WS_EX_APPWINDOW`) — a program's main window, a standalone instrument's, a plug-in docked in its DAW's window — is refused at the call (`"the window belongs to a program's main window, which never gets the no-activate style: …"`), since it could lose its taskbar button and its place in Alt+Tab while it held the style. Changing another program's window waits for that program, so it is done on the host's `window-style` thread, never on the event loop: the program is asked first whether it answers (`WM_NULL`, up to 1000 ms), then the style is set with `SetWindowLongPtrW` — which waits for the program **without a bound**: one that answers the question and then hangs holds the thread, and every click and give-back after it, until it answers — and read back: a style that is on is held, whatever the call answered. The click follows on the same thread. **The style is held** — taken off right after the click, the change would reach the program before the click does, and the click would bring the window forward after all — until another window comes to the front, the module that clicked stops (disabled, reloaded, rolled back or stopped by the host) or the application ends, and is then taken off on the same thread, after every click still on its way. Another window means one with a title: the host has no window event for a window without one, so an untitled window in front ends nothing until a titled one follows it. At the end the application hands that thread a last give-back and waits for it up to 2 seconds in all, each program having half a second to answer; a window not given its activation back by then — its program did not answer, or hangs as the style changes — keeps its line in the file below, and the next start gives it back. From the moment the application begins to end, no window is given the style: a click still on its way is refused with `"the application is ending"`. A window that has the style of its own is clicked as it is and left alone. A program that brings its window forward itself on a click still can: the click is real input, which gives it the right to.

Every window given the style is written to `held-window-styles.txt` beside the application **before** it is given — its handle, its process and when that process started, its class — and the file is rewritten as the style is taken off, and deleted when no window holds it. A run that ends without taking it off again, a crash or a kill, takes it off at the next start, before any module can click, wherever the window is still the one named and still has the style; until then that window does not come to the front when clicked (the keyboard and the DAW's own commands still bring it forward), and closing it in the DAW ends that too. Refused on the thread, for the callback: `"the window is gone"`, `"the window's program did not answer within 1000 ms"`, `"the window's program did not answer (error N)"`, `"the window's program runs with higher rights"`, `"the window did not take the style (error N)"`, `"the window did not keep the no-activate style"`, `"the list of windows to give their style back could not be written (…)"` with the file and the system's words, and `"the application is ending"`. Each refusal is a line in the log as well, callback or not — `[input] click at 940,310 for window 2755348 (via = "noactivate") refused: the window did not keep the no-activate style` — and so is what went right:

`[input] window 2755348 ('#32770') of process 4242 given the no-activate style for clicks that leave the window in front where it is`

`[input] clicked window 2755348 ('#32770') at 940,310 without activating it`

`[input] window 2755348 ('#32770'): the no-activate style given back — another window came to the front`

### macOS

**`via = "input"`.** The event's modifier flags are **cleared**, so a click is a bare click whatever is held — the same treatment `send` gives a key, and the same for `scroll`, `drag` and the `mouseDown`/`mouseUp` pair. Added when the shortcut that puts the keyboard back into a plug-in was found to click while its five-key chord was still down. What such a click would have done was not measured: by the AppKit convention a Control-modified left click is a secondary click, and the chord in question holds Control among four others, so a context menu is the likely reading rather than an observed one.

**`window`** is checked as [`host.window.ownsPoint`](./window.md#host-window-ownspoint) checks a point: only a window known to be drawn over the point refuses, with `"something else is drawn over the point"` — VoiceOver's windows and this application's own other than a page are not taken for one. With `"input"` that is all; `"post"` asks more of the window (below).

**`via = "post"`.** The move, the press and the release are `CGEvent`s handed to the window's process with `CGEventPostToPid`, back to back. That way passes the window server's routing by — no hit test of its own, no activation, no pointer moved — so each event says where it goes: the window's number in the window server (its `CGWindowID`: the window's own, or for an id of something inside a window, that window's), in the two public fields `kCGMouseEventWindowUnderMousePointer` and `kCGMouseEventWindowUnderMousePointerThatCanHandleThisEvent` and in the private one AppKit's own events carry it in (51); and the point's place in that window, from its top-left corner, set with the private `CGEventSetWindowLocation`. AppKit in the program hands the events to that window, and the window to the view under the point, while the window's application stays in the background. The events' modifier flags are cleared, as for every click here, so a click from a hotkey's callback is a bare click; each is marked as the host's own in `kCGEventSourceUserData`. With `move` (the default) the pointer is warped to the point first with `CGWarpMouseCursorPosition`, so that a program asking where the pointer is (`[NSEvent mouseLocation]`) finds it on the click. The warp posts no event of its own — the program sees the posted move only — and after a warp macOS holds the mouse's own movement off the pointer for about a quarter of a second. The host joins the two again at once (`CGAssociateMouseAndMouseCursorPosition`, as SDL, GLFW and Wine do after their warps), which gives the user's mouse the pointer back straight away while this application is the active one — Apple describes the call for an application in the foreground. A posted click is mostly made while it is not (a page described `activates = false`, an overlay's key with the DAW in front), and what the call does then is not measured: the user's mouse may stall for that quarter of a second after each such click (TODO.md, MW21). `move = false` leaves the pointer where it is.

Refused, each as on Windows where Windows has it: `"the window is gone"` (no window the host knows by that id, or its number now belongs to another program's window), `"the point is not in the window"` (its rectangle — a control's own for a control's id), `"the point is on the window's frame, not in its client area"` (a window's title bar or border, where a click would move or zoom the window), `"something else is drawn over the point"` (the hit test of `window` above). And the Mac's own: `"this application may not post events: Accessibility is not granted to it (System Settings > Privacy & Security > Accessibility)"` — `CGEventPostToPid` answers nothing, and without that privilege the window server drops the events without a word, so the host asks `CGPreflightPostEventAccess` first and refuses rather than answering `true`; the start block's `hid post events` line names the same privilege; `"the window has no number in the window server's list"` (the window could not be paired with one — the window it is in, for a control); `"the window is not on screen"` (minimised, hidden, or on another Space); `"the window's program did not answer"` (its window's place could not be read, or it did not answer a moment ago); and `"this macOS cannot give a posted click its place in the window (CGEventSetWindowLocation is missing)"`. One line in the log per posted click, naming the process and the window it went into, and the place in the window the events carry — where a click landing beside its target shows:

`[input] posted click for window 12: into window number 8812 of process 4242 (Logic Pro X) at 240,160 from the window's top-left corner (340,210 on the screen); pointer moved: yes`

**What the program then does is its own, and three things decide it here.** AppKit's first-mouse rule: a press into a window that is not its application's key window may make it the key window there — the application still not the active one — and reaches the view only when that view takes the first mouse; a view that does not (many custom views, SwiftUI's tap gestures, the web view of a page) may get nothing of that click. Measured on the CI's Mac runners (macOS 14 to 26, `page-check`'s `posted_unkeyed`): two clicks into a page whose window did not have the keyboard, with another application in front, reached nothing, and the page's window did not become the key window; clicks into the same page while it had the keyboard landed where they were aimed. Whether a later click lands while the application stays in the background is not measured: the work this rests on saw such clicks dropped, a custom view's and a SwiftUI tap's, and needed a step of its own first that brought the application forward or gave it the focus — so posting twice is no remedy to count on; the CI's `page-check` records what a page's web view does with two (`posted_unkeyed`). JUCE takes the first mouse, reads the click's place from the event rather than from the pointer, and **brings its own application forward on a press**: a JUCE view's mouse-down asks for its application's activation (`activateIgnoringOtherApps:`) whenever that application is not the active one — for a plug-in, its DAW — so a posted click into a JUCE plug-in may bring the DAW to the front by the plug-in's doing, not the host's; whether macOS 14 and later grants a request that no real click came with is not measured. And a plug-in drawn by another process — an Audio Unit its DAW runs out of process — is a view of that process's inside the DAW's window, which the events handed to the DAW's process may not reach. Nothing of this has run in a plug-in yet: the CI's `page-check` posts clicks into a page of the application's own on every Mac runner, and a session in Logic is to say what plug-ins do (TODO.md, MW21). Not measured either: a program that reads the mouse elsewhere than from its events (a game, a canvas engine).

**`via = "noactivate"`** answers `false, "not supported on this platform yet"`, and so does the callback: it is a Windows window style. A module written for both platforms takes `"post"` or the round trip here.

## host.input.drag(x1, y1, x2, y2, opts?) {#host-input-drag}

Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases — with real, paced movement in between.

**Signature:** `host.input.drag(x1: number, y1: number, x2: number, y2: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

All four coordinates are integers. `opts.button` behaves exactly as in `click` (defaults to left). Returns `nil`.

```luau
host.input.drag(100, 100, 400, 300)
host.input.drag(100, 100, 400, 300, { button = "middle" })
```

A drag is not a press, a warp and a release. On Windows the pointer is moved with injected move events rather than `SetCursorPos`, interpolated over sixteen steps with a few milliseconds between them, because two kinds of control tell the difference: one that watches for motion while its button is held sees injected events and can miss a warp entirely, and one that reads the *speed* of a drag answers an instantaneous jump with an enormous change. macOS posts a drag event between press and release, which it has always done. The pacing blocks the calling thread for about sixty milliseconds, which is only ever paid when somebody deliberately drags something.

If a control needs a sustained press before it will react at all, use `mouseDown`/`mouseUp` with a timer instead — that is why those exist separately.

### Windows

The pointer is moved with injected move events rather than `SetCursorPos`, interpolated over sixteen steps a few milliseconds apart. The pacing blocks the calling thread for about sixty milliseconds, paid only when something is deliberately dragged.

### macOS

A single `Dragged` event is posted between press and release — no interpolation. That asymmetry is deliberate: a scrollbar needs one intermediate event and gets it. A control that only reacts to *continuous* movement would work on Windows and not here, and nothing has yet exercised the difference on a Mac.

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

`combo` is a [key spec](./keys.md#key-spec-string-format), read by the same parser as a hotkey or a capture: `+`-separated parts, the final part being the key and any leading parts modifiers (whitespace around parts is trimmed, case does not matter). The modifiers are the four roles: `ctrl` (also `control`, `cmd`, `command`), `alt` (`option`), `shift` and `win` (`super`, `meta`). On a Mac `ctrl` is Command and `win` is Control, so `host.input.send("Ctrl+S")` is Ctrl+S on Windows and Command+S on a Mac, the application's own Save on both. For the Mac's Control key, write `Meta`. The key is one of the [key names](./keys.md#key-spec-string-format) — letters, digits, `F1`–`F24` and fourteen named keys; there are no numpad keys, punctuation, `Insert` or media keys, and there is no separate key-down or key-up. An empty combo, an unknown modifier, an unknown key or a `"<modifier> tap"` raises a Lua error naming the part. Returns `nil`.

The key goes through the same keyboard hook as a real one, so a combination that any module has [captured](./keys.md#host-keys-capture) is caught and suppressed again — sending the key you captured does not pass it on. See [`post`](#host-input-post) for that.

```luau
host.input.send("Ctrl+S")         -- the application's Save, on either platform
host.input.send("Ctrl+Shift+Esc")
```

### Windows

Each role is the key of its name. The modifiers are synthesised as real key presses around the key — pressed in the order Ctrl, Alt, Shift, Win, whatever order the spec wrote them in, and released in reverse — and whatever the user is **physically holding is inherited**. Called from a hotkey callback while Alt is still down, `host.input.send("Escape")` arrives as Alt+Escape — which is why `host.keys.modifiersDown()` exists and why modules defer a synthesised key until the user has let go.

Every key is sent by **virtual-key code only**: `SendInput` with scan code 0, without `KEYEVENTF_SCANCODE` and without the extended-key flag — so the arrows, Home, End, Page Up/Down and Delete go out as their non-extended forms. Each press and release is its own `SendInput` call, back to back with no delay between them, so there is no hold time. An application that reads scan codes, DirectInput or Raw Input — many games — may ignore the key, or read it as a different one.

### macOS

`Ctrl` is Command, `Alt` Option, `Shift` Shift and `Win` (`Meta`) Control, so `"Ctrl+C"` copies and `"Meta+C"` is Control+C. The modifiers are set as flags on the event, and setting them **replaces the whole set**, so anything the user is holding is stripped and the same call delivers a bare Escape. No modifier key event is posted at all, so an application that watches for physical modifier presses sees an unmodified key.

The deferral dance is therefore unnecessary for keys here, and since 2026-09-03 not for clicks either: a synthesised click has its flags cleared the same way (see `click`).

A letter is the key that types it under the current keyboard layout — with Command held, when the combo holds Command — as on Windows: `"Ctrl+Z"` is the key labelled Z on a German Mac as on a US one, so `host.input.send("Ctrl+Z")` is Undo on both. Digits and the named keys are US keyboard positions (see [the key spec](./keys.md#key-spec-string-format)).

## host.input.text(text) {#host-input-text}

Types a Unicode string as synthetic keystrokes.

**Signature:** `host.input.text(text: string) -> nil`

`text` is sent character-by-character as Unicode input, not as key presses. Returns `nil`.

Control characters are sent as characters too: `"\n"` is the character U+000A and `"\t"` is U+0009, **not** a press of Enter or Tab, and whether an application treats those characters like the keys is up to the application. Press keys with [`send`](#host-input-send): `host.input.send("Enter")`.

```luau
host.input.text("Hello, world!")
host.input.send("Enter")   -- not "\n"
```

### Windows

Each UTF-16 unit is sent with `SendInput` as a `KEYEVENTF_UNICODE` event (virtual key `VK_PACKET`), down and up. An application that reads scan codes, DirectInput or Raw Input — many games — does not see it.

### macOS

Each character is posted as a keyboard event pair with key code 0 and the character as its Unicode payload, with the modifier flags cleared, so the keyboard layout plays no part.

## host.input.mouseDown(x, y, opts?) {#host-input-mousedown}

**Signature:** `host.input.mouseDown(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

Presses a mouse button at `(x, y)` and **leaves it down**. This is the half of a gesture that `click` and `drag` cannot express, because both of those press and release in the same breath: a control that only reveals itself while the button is *sustained* — the flyout of extra tools behind Melodyne's tool icon is the case that prompted this API, a gesture that later turned out to be undocumented and was replaced by a key — never appears for either of them. It is split into two calls rather than offered as a timed variant deliberately: holding, gliding and settling is the better part of a second, and a call that slept for that long would sleep in the event pump, stopping speech, hotkeys and detection with it. Split, the caller composes the gesture out of `host.timer` callbacks and the pump keeps running between the parts. There is no screen read here and nothing is asked about what is under the point, so the call itself is cheap.

While your module runs, nothing releases the button for you. A held button belongs to the whole desktop, not to your module, so **every** continuation must end in a `mouseUp` — including the ones that give up because the overlay moved to another window mid-gesture. Pin the window handle before the press and re-check it in each callback, exactly as the timed sequences elsewhere in the runtime do; `opts.button` must match on the way up as it did on the way down. A `mouseUp`, `click` or `drag` of the same button ends the hold.

The host releases a button your module holds only when the module stops running: when it is disabled, reloaded, [stopped by the host](../module-runtime-and-lifecycle.md#limits) (a callback that ran too long or out of memory), when its load is rolled back, and when the application ends. It releases it where the pointer is at that moment, so the pointer does not move, and writes a line such as `[input] [com.tool.guard-probe] released the left mouse button it had pressed with host.input.mouseDown: it was stopped by the host`. The release is the desktop's, like the press: it lets go of the button for whatever program is under the pointer.

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

### Windows

Genuinely per-window: `WM_KEYDOWN`/`WM_KEYUP` are posted to the handle you passed with `PostMessageW`, with the scan code packed into the `lParam` because some applications read that rather than the virtual key — Melodyne decodes keys itself rather than leaving it to the defaults. Posting is also the path most likely to be seen at all here: Melodyne runs its own message pump with its own accelerator table, so a posted `WM_KEYDOWN` reaches `TranslateAccelerator` where a sent one would bypass it. A handle whose window is gone is not reported: the post goes nowhere and the call returns normally. A posted key exists only as a window message: the keyboard state is not changed, and an application that reads the keyboard through DirectInput or Raw Input — as many games do — never sees it.

### macOS

There is no per-window message queue to post into, so this is one step coarser — the event is handed to the window's **process** with `CGEventPostToPid` and lands wherever that application routes it, which need not be the window you addressed. Three consequences a caller can observe: the handle must be one this backend issued, and a stale or foreign number raises an error naming it instead of quietly doing nothing; a key name the shared table accepts can still fail here, because it has to map on to a Quartz keycode as well — a letter maps to the key that types it under the current keyboard layout (see [the key spec](./keys.md#key-spec-string-format)) — and one that does not (`F21`–`F24`, or a letter no key of the layout types) raises an error naming the key and its VK code; and the modifier flags on the event are **cleared**, not inherited, so a key posted from inside a hotkey callback while its combination is still physically held arrives bare. The first post of a run writes a line to the log saying it is routing per process — the line to suspect if a posted key ever surfaces in the wrong window of the same application.
