---
title: "Overlay — keyboard controls with spoken labels"
sidebar_position: 21
toc_max_heading_level: 2
---

An overlay is a list of controls a module defines over a plug-in window. The user moves through it with Tab; each control is spoken as its label, what kind of control it is, and its current value; and pressing one acts on the plug-in — a click at a coordinate, a key, a drag, whatever that control was built to do. It exists because the plug-in draws its own interface, which a screen reader cannot see into. It is **not a host capability but a module** — declare `com.platform.overlay` under `dependencies` in `module.toml`, never under `[capabilities] require`, and pull it in with `host.require`.

What comes back is very nearly the whole of most modules: Impact Soundworks' Juggernaut is a table of measured coordinates and two library overlays of a caption and one `addOCRButton` each. How a control reads its value is the cost you are choosing — a hotspot toggle samples a single pixel, which is one compositor frame (~16.7 ms on Windows through the standard path); a graphical toggle image-matches templates over a region; OCR is slower than either, which is why `ocrLabel` belongs on controls whose name really does change with the loaded patch and not on every control.

One plug-in is usually several overlays rather than one. Kontakt declares one per cell of version-by-environment, and overlays sharing an arbiter slot rank by `O.layer`, so Komplete Kontrol's chrome yields to the Kontakt inside it, that to the library loaded in that, and all three to a modal while it is up.

Running through every method here is one rule: **announce what the application did, not what the module intended.** `O:watch` waits for a value to change instead of guessing a delay, a pixel that resembles neither reference reports no state at all rather than the nearer guess, and a click is refused when another window is drawn over the point.

All control coordinates are **origin-relative**: the origin is the client-area top-left of the active context's coordinate window — the plug-in window when standalone, or the embedded plug-in's child control when hosted in a DAW (on a Mac, the DAW's plug-in panel: see [`attachEmbedded`](#o-attachembedded-macos)) — re-resolved per call so it tracks the window as it moves, and `(0, 0)` when the overlay is not attached. An overlay can shift that frame ([`O:frame`](#o-frame)) and, for a plug-in that zooms its whole interface, scale every authored coordinate ([`O:scale`](#o-scale)).

A control is spoken as `"label, type[, value]"`, and **every kind is a focus stop**: static text is Tab-reachable and read aloud; Space and Return do nothing on it, and its hotkey or [`O:activate`](#o-activate) says it again.

A control's hooks are the module's code, and the runtime calls them guarded — and in one of three places, which decides whether a hook may wait for a read there: see [Where a hook runs](#where-a-hook-runs). A `text` — and a tab control's `current` and `verticalName` — that raises gives **no value**: the control is announced as if the hook had answered `nil`, the read before a stepper's step or activation is `nil`, and a stepper's watch takes it as nothing read ([`O:watch`](#o-watch)). It is logged once per control and hook, however often the control is announced: `[overlay] '<label>': its text raised: <message>` (`its current raised`, `its verticalName raised`). A `when` that raises hides that one control, logged once as ``<overlay>: '<label>' — its `when` failed (<message>); hiding just this control``.

```luau
local O = host.require("com.platform.overlay")
```

Source: `modules/overlay-runtime/src/main.luau`, and its reasons in `modules/overlay-runtime/src/errors.luau`.

:::caution Since version 0.11: error values, not words
Every answer of the runtime's that says why something was not done is an [error value](./errors.md#error-values), as the host's are: what `O:chooseMenuItem` and its `onDone`, `O:inPlugin`, `O:pageBack`, `O:pageKey`, `trip:afterMenu`, `trip:handOver`, `O:stillHere`, `O:toScreen`, `O:toScreenRect`, `onGiveUp`, `onBack`, a hand-over's `onEnd` and the calibration and capture methods answer — and `onShown` and `onClose` hand on the page's own. (`O.doubleClick` answers as `host.input.click` does: a double-click whose second half the host did not send answers `covered`.) Compare its `code` with a constant of [`O.errors`](#o-errors), which holds the runtime's eight codes and every one of `host.errors`'s: `err.code == O.errors.BUSY`. Its message is the sentence the runtime logs, and `tostring` and `..` give it. **Code written against the words of 0.10 — `why == "busy"`, a table keyed by `"due"` — no longer matches, and says nothing.** A dependency is a lower bound only, so a module that asks for `"com.platform.overlay >= 0.10"` loads against this runtime all the same; one that compares codes asks for `>= 0.11`.
:::

**Reading an entry.** Every entry ends with **Returns** and **Errors**, before its Windows and macOS blocks ([Reading an Errors table](./errors.md#reading-an-errors-table)). The runtime's own messages are its reasons in `errors.luau`, word for word; a message may change between versions, and the code does not. An error the runtime only hands on — a capture's, a focus's, a page's, a hotkey registration's, a click's — is the host's own table, and its row links to that call's Errors. The runtime hands each on through one helper of its own, `handOn`, which names the entry and the host call, and the docs check reads those and holds the rows to them, both ways. It also follows a host call's error value through each of the runtime's functions — the call's second value, or a callback's second parameter — and fails one that leaves by `return`, a hook or a callback without `handOn`, and a call whose whole answer leaves at once or is packed into a table; an error stored in a table's field or an upvalue and handed on from another function it does not follow. A hook's error values are rows of the call whose spec holds the hook, under "arrives" `hook onDone`, `hook onGiveUp` and so on.

**The runtime makes no partial answer of its own.** Where a call of the runtime answers an error value, its first value is `false` or `nil`, and nothing of what was asked was done — but for [`O:captureAll`](#o-captureall), which answers `false, err` for the first shot it could not write, having written the others. The one partial answer a module meets through it is the host's: a page's `onShown` is handed [`page:show`](./gui.md#page-show)'s `true, err` as it came — the page is in front, but over the plug-in's window ([`O:addPage`](#o-addpage-errors)).

**Two raises any method can meet**, said once here. A binding's `control` function and the overlay's [`O:frame`](#o-frame) function are called unguarded wherever the overlay works out its origin or places a coordinate, so what they raise comes out of the method that asked — `O:origin`, `O:hwnd`, `O:here`, `O:toScreen` and every other that places. And the host's guard: `stopped by the host: …`, when the module has run too long ([the limits](../module-runtime-and-lifecycle.md#limits)). The capability gate is not met: the runtime is a code module, and its own calls of the host are judged by its own manifest ([`[capabilities]`](../module-package-format.md#capabilities)), while a module's hooks call the host under the module's.

## What to declare {#declare}

Not a capability — a **dependency**:

```toml
dependencies = ["com.platform.overlay"]
```

An `"overlay"` entry under `[capabilities] require` is a leftover from when the overlay was part of the host; `host.overlay` no longer exists. See [what that list is and is not](./index.md#capabilities).

## Where a hook runs {#where-a-hook-runs}

Every callback of a module runs as a [handler](../module-runtime-and-lifecycle.md#handlers) of the host, one at a time per module, and the runtime calls an overlay's hooks inside them. A hook runs in one of three places, and the place decides what it may do:

| Hook | Runs | May it wait for a read or a capture? |
|---|---|---|
| a control's `text`; a tab control's `current` and `verticalName` | in the handler of the key, timer or window event that announces the control | yes |
| a control's `onActivate`; a stepper's `onStep` | in the handler of the key or hotkey that pressed it | yes |
| a [menu test](#o-menutests)'s `before` | in the handler of the press — the key, hotkey or trip that opened the menu — before the press counts | yes — `O.menuTests.drawn`'s picture before the click — except for a press made from an `onDone` inside the menu tick's coroutine, where a read raises |
| a stored [`identify`](#o-attachembedded) — one whose verdict is kept — asked for a control it has no verdict for | in the handler of the event that resolves the overlay's origin; in the arbiter's call, a plain call, when that is the first to resolve it — an overlay on a `slot` brought to the front by the arbiter choosing again, after another claimant left | yes, but not in the arbiter's call |
| `when`; an `identify` asked at every evaluation (`cacheIdentity = false`, or a `control` function's where the DAW gives no panel); a binding's `present`; a [menu test](#o-menutests) | in a coroutine of the runtime's own, one per scan or tick | no: it must answer at once, and reads no screen — a pixel a `when` needs is a [probe](#o-probe)'s |
| a [menu test](#o-menutests)'s `chained` | where the answer of the test that placed an item with a `submenu` runs — in a coroutine of its own inside the menu tick's work, or in the handler of the test's callback — before the item is clicked | no: it answers through its `done`, at once or from a callback |
| a `menuItem` function (a `submenu`'s too) and the `onDone` of [`O:chooseMenuItem`](#o-choosemenuitem) | inside the menu tick's coroutine when a test answers at once; otherwise in the handler of the test's callback, and `onDone` also in a key's handler, in the arbiter's call as the overlay leaves the front, or — in a calibrating run, on a press whose item is photographed — in the handler of the [menu item shot](#o-calibrating)'s answer, right after the item's click | no: it must answer at once; a [probe](#o-probe)'s reader answers what stands, and reads nothing on the tick or in either callback |
| the overlay's own [`onActivate`](#o-onactivate) and [`onDeactivate`](#o-ondeactivate) | on a `slot`, in the arbiter's call, a plain call; without one, in the handler of whatever looked at the overlay again — a window or focus event, a `pollMatch` poll, the runtime's own timers (the first evaluation of a binding, the check 250 ms after it came to the front), the menu tick | no: on a slot nothing can wait there, and either way the overlay is half way into or out of the front |
| a [library](#o-gate)'s `gate`, a [landmark](#o-landmark) among them | in the handler of whatever looked at the overlay again, outside any scan | yes, but a read with a callback keeps the module's keys free |
| a [page](#o-addpage)'s `describe` | in the `addPage` or `pageAgain` call | no: it only describes |
| a page's `open` | in the page key's handler, or in the page's `onReady` handler when the page loaded after the key; with `openLater`, its `answer` from wherever the module calls it — at once, or a read's callback | yes |
| a page's `onMessage`, `onReady` and `onClose` | as the page's own events, handlers of the module (a message and `onClose("user")` in the interactive lane) | yes |
| a [trip](#o-inplugin)'s `act`, and the `fn` of its [`afterMenu`](#trip-aftermenu) and [`afterDialog`](#trip-afterdialog) | on the module's next turn after the arrival ([`host.timer.after(0)`](timer.md#host-timer-after)); the arbiter's call that sees the arrival only schedules it | yes |
| a trip's `after` and `onGiveUp` | on the module's next turn after `done` or the end, or in the page key's handler | yes |
| [`O:afterDialog`](#o-afterdialog)'s `fn` | on the module's next turn after the overlay is in front again after the dialog ([`host.timer.after(0)`](timer.md#host-timer-after)), or in the timer of its 8-second bound | yes |
| a [hand-over](#trip-handover)'s `onKey` | in the handler of the key it captured, the plug-in's window in front | yes |
| a hand-over's `ends` | on the module's next turn after the arrival that follows its end key ([`host.timer.after(0)`](timer.md#host-timer-after)), as a wait's `fn` | yes — it answers through its `answer`, at once or from a read's callback |
| a hand-over's `onEnd` | after its own end: where `ends` answered `true` — the arrival's handler, or a read's callback — or, after an `onKey` that answered `true` and with no `ends`, in the arrival's handler on the module's next turn; after any other end, where the trip's `onGiveUp` or `after` runs, just before it; never after a `handOver` that answered false | yes |
| a page's `onShown` and `onBack` | in the handler that showed or hid the page | yes |

**"Wait for a read"** is [`host.ocr.recognize`](ocr.md#host-ocr-recognize) without a callback, and **"a capture"** a [`host.screen.snapshot`](screen.md#host-screen-snapshot), [`pixel`](screen.md#host-screen-pixel) or [`pixels`](screen.md#host-screen-pixels) of the screen, or a [`profile`](screen.md#host-screen-profile), [`cells`](screen.md#host-screen-cells), [`matchCells`](screen.md#host-screen-matchcells), [`template{ capture }`](screen.md#host-screen-template), snapshot's `crop`, [`imageSearch`](screen.md#host-screen-imagesearch), [`imageSearchAll`](screen.md#host-screen-imagesearchall), [`save`](screen.md#host-screen-save) or [`saveMarked`](screen.md#host-screen-savemarked) without a callback: where the table says yes, it stops the handler until the reading, the picture, the colours or the region's answer come, and only the overlay's module waits — its next key runs after it ([Where it waits](ocr.md#where-it-waits), and for the screen [its own](screen.md#where-it-waits)). With a callback a read waits nowhere, and is fine in every row; so is a `pixel` or `pixels` of a snapshot, which reads no screen. What the rows that say no rule out: a `when` is asked for every control on every Tab and a menu test on every tick, and a module that waited there would be busy all the time.

**Where it may wait**, the overlay checks afterwards that what it was doing still stands. A sentence notes where the overlay is before its control's first hook runs — in front, in which stay, on which window, the focus and the control, the keys of its own, the announcements, the menus opened — and after each of `text`, `current` and `verticalName` it asks whether that still holds, as [`O:stillHere`](#o-stillhere) does with `focus`, `keys` and `said`, and a menu opened since (a menu that was open before the hook does not count against it). When it no longer holds, nothing of the sentence is said, the log says `[read] '<label>' not spoken after its text: <why>` (`after its current`, `after its verticalName`) in `stillHere`'s words, and an OCR button's click is not made: `[read] '<label>': not clicking — <why>`. Without a wait nothing changes between the two, and the sentence is said as it always was. A module's own `onActivate` or `onStep` that reads and then acts — as VPS Avenger's preset steps read the name, then click — takes [`O:here()`](#o-here) before the read and asks [`O:stillHere`](#o-stillhere) after it, before it acts: the runtime cannot check that for it. The runtime does the same around its own reads of a pixel before a click: a [hotspot toggle](#o-addhotspottoggle)'s press, and a [graphical toggle](#o-addgraphicaltoggle)'s `reveal` probe.

**Where it must not wait**, the hooks run in a coroutine the runtime makes: one for a whole scan — every `when` of a Tab's search for the next control, every claimant of a shared hotkey, every test a menu tick asks — and each hook inline inside it; a hook asked on its own, such as the `when` of the control Return presses, has one to itself. That is one coroutine per scan or per lone hook. To the host it is a coroutine the module made: `host.ocr.recognize` without a callback does not wait there, and raises — the hook then raises with it ([Where it waits](ocr.md#where-it-waits)). Nor does such a hook read the screen itself: the `host.screen` calls that wait in a handler — `snapshot`, `pixel`, `pixels`, `profile`, `cells`, `matchCells`, `template{ capture }`, `crop`, the searches, `save`, `saveMarked` — raise there without a callback, as `recognize` does, and the hook raises with them ([Where it waits](screen.md#where-it-waits)); a `pixel` or `pixels` of a snapshot is answered at once. What a `when` needs of the screen is declared as a [probe](#o-probe): the runtime reads it in the handler before the scan, one capture for all the overlay's probes and only after something acted (or at every Tab: for a probe declared so, and while a change the module [expects](#o-expectchange) has not been read), and the `when` answers from that reading. A call with a callback — [`host.ocr.recognize`](ocr.md#host-ocr-recognize), and every [`host.screen`](screen.md#with-a-callback) call given one — asks and returns at once, and is fine there. A `coroutine.yield` of the hook's own is ended with the error `a hook asked on every scan yielded; it must return at once`: a `when` asked on its own then hides its control, as a `when` that raises does; inside a scan the yield reaches past that guard, and the scan — and with it the key that started it — ends with that error. A menu test that yields has failed, as one that raises has: the tick goes on, the tests after it are asked on the next tick, and the test itself is asked in a coroutine of its own from then on, where it fails alone.

**The overlay's own origin is resolved before every such coroutine**, outside it, so a stored `identify` asked for it runs in the handler. One reached inside the coroutine all the same — another overlay's origin asked by a `when`, say, after a plug-in made its control anew — is not asked there: that evaluation finds no control (not counted towards the eight of [`attachEmbedded`](#o-attachembedded)), nothing of it is kept for the [epoch](timer.md#host-epoch), and the overlay looks again on its module's next turn, once per epoch, through a recheck of its own. Logged once per control: `attachEmbedded: [<class>] id=<id>, identify is not asked inside a scan of the overlay's hooks — '<overlay>' looks again on its next turn`.

**The first evaluation of a binding** runs on the module's next turn, about 15 ms after it was made ([`host.timer.after(0)`](timer.md#host-timer-after)), not inside the call: a binding is made at a module's top level, where nothing can wait. An overlay whose window or plug-in is in front at load comes up a tick later.

**A page's hooks** all run under `pcall`; one that raises is logged as `[page] '<overlay>': <hook> failed: <message>`. A trip's `act` or a wait's `fn` that raises gives the trip up (`failed`); an `after` or `onGiveUp` that raises does not stop the page from coming back — what it posted before the raise is in the page. Each is held to the [limits](../module-runtime-and-lifecycle.md#limits) every callback is held to, and a wait inside one counts on no clock. The arbiter's calls stay plain calls: bringing an overlay in or out of the front only records and schedules for its page.

**Coming to the front** resolves the origin once more, guarded: a `control` function that raises there is logged, `[overlay] <label>: its origin raised as it came to the front: <message>`, and the overlay still comes up whole — its keys taken, its scope its window — as on a window whose handle it does not know. On a `slot` its [probes](#o-probe) are not read there, where nothing can wait: a `when` asked as it comes forward answers from the last reading, and the check 350 ms later, before the arrival is said, reads them and moves the focus off a control they now hide.

**Before a press**, only a stepper's value is read, because its announcement waits for that value to change. No other control's `text` is asked before its `onActivate` runs; a module that needs what its `text` works out asks for it in `onActivate` itself.

```luau
-- One pixel of the plug-in's own, read for the `when` below in the handler before each scan.
local browser = ov:probe("browser", { { 640, 12 } })

-- Asked on every scan, in the runtime's coroutine: it answers from the probe, at once.
ov:addCustomButton({
  label = "Close library browser",
  when = function()
    local c = browser()
    return c ~= nil and c[1].r > 200
  end,
  onActivate = function(o)
    local x, y = o:toScreen(640, 12)
    if not x then return end
    -- The click shuts the browser: the probe is read at every key until it says so, so a Tab that
    -- comes before the plug-in has redrawn does not keep this control in the ring.
    o:expectChange("browser")
    host.input.click(x, y)
  end,
})

-- Asked as the control is announced, in the handler: it may wait for a read.
ov:addStaticText({
  label = "Preset",
  text = function(o)
    local r = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
    return r and host.ocr.recognize(r).text or nil
  end,
})
```

## O.new(label) {#o-new}

**Signature:** `O.new(label: string?) -> Overlay`

Creates a new overlay object, with no controls and no binding: it does nothing until it is bound ([`O:attach`](#o-attach), [`O:attachEmbedded`](#o-attachembedded), [`O:bind`](#o-bind)). `label` is the name announced when the overlay comes up, and the name every log line of it carries; `"Overlay"` when it is `nil`.

```luau
-- sforzando's standalone overlay: the object, its controls, then where it lives.
local O = host.require("com.platform.overlay")
local ov = O.new("sforzando")   -- the name announced when the overlay comes up
ov:addOCRButton({ label = "Instrument", region = { 90, 22, 200, 36 }, opensMenu = true })
ov:addOCRButton({ label = "Polyphony", region = { 486, 40, 516, 70 }, opensMenu = true })
-- `menus`: how sforzando's own pop-up list is seen, so the keys belong to it while it is open.
ov:attach({ title = { contains = "sforzando" }, windows = { class = "PLGWindowClass" } },
  { menus = { O.menuTests.nativePopup, O.menuTests.newWindow } })
```

Costs one table, on the event loop; nothing is asked of the host.

### Returns {#o-new-returns}

An `Overlay`, a table with a metatable, every time. Its fields a module reads are `label`, `controls` (the controls in the order they were added), `focus` (the index of the focused control, `0` before the ring is entered), `active` (whether it is in front), `hoverToRead`, [`state`](#o-state), [`away`](#o-away) and, once it has a page, `page`. The fields that begin with `_`, `contexts` and `activeCtx` are the runtime's.

### Errors {#o-new-errors}

None. It never answers an error value.

It never raises: `label` is kept as it is given, and only `nil` is replaced.

## O\:addStaticText(label) {#o-addstatictext}

**Signature:** `O:addStaticText(labelOrOpts: string | { label: string, text: ((overlay) -> string?)?, when: ((overlay) -> boolean)?, hotkey: string?, hotkeyKeepsFocus: boolean? }) -> Control`

Appends a static text control: Tab-reachable and read aloud on focus. Space and Return do nothing on it; its `hotkey` and [`O:activate`](#o-activate) say it again.

`labelOrOpts` may also be a **table** — `{ label, text, when, hotkey, hotkeyKeepsFocus }` — and that form is the one worth knowing. `text(overlay) -> string?` is appended to the label each time the control is announced, which turns a caption into a **read-out**: a value the user wants to check is then read by arriving at it, with nothing pressed and nothing changed. The alternative would be a button, and a button that only reports is a promise of an action it does not perform. `text` runs in the handler of the key or event that announces the control, and may read there ([Where a hook runs](#where-a-hook-runs)).

`hotkey` (since version 0.4.1 of the runtime) gives a read-out a key of its own: pressing it moves the focus to the control, as any control's hotkey does, and says it — `"<label>, <text>"`, interrupting what is being said, without the key's name, which is said only when focus arrives by Tab. `hotkeyKeepsFocus = true` says it and leaves the focus where it was. Space and Return are never taken while the focus is on a static text, with or without a hotkey: they stay the application's. A module that sets `hotkey` depends on `"com.platform.overlay >= 0.4.1"`; an older runtime ignores it.

```luau
-- A caption...
ov:addStaticText("Cinematic Studio Strings")

-- ...and a read-out, re-read on every announcement.
ov:addStaticText({
  label = "Battery",
  text = function() return valueRightOf("Battery Level") or "not shown" end,
})

-- A read-out with a key: Alt+B says "Battery, 80 per cent" from anywhere in the overlay.
ov:addStaticText({
  label = "Battery",
  hotkey = "Alt+B",
  text = function() return valueRightOf("Battery Level") or "not shown" end,
})
```

### Returns {#o-addstatictext-returns}

The control table, appended to `o.controls`: `{ kind = "static", label = label }` for a string, and `{ kind = "static", label, text, when, hotkey, hotkeyKeepsFocus }` for a table, `hotkeyKeepsFocus` made a boolean. `when` is the control's own ANDed with an enclosing [`O:group`](#o-group)'s.

### Errors {#o-addstatictext-errors}

None. It never answers an error value.

**Raises** for a control added after the overlay is bound: `overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`. Nothing else is checked at the call. A field of the wrong kind is reported when the overlay binds, in a line `[overlay] <overlay>: <finding>` that names the control, and the control is kept.

## O\:addHotspotButton(opts) {#o-addhotspotbutton}

**Signature:** `O:addHotspotButton(opts) -> Control`, where `opts: { label: string, at: {number, number} | ((overlay) -> {number, number}?), points: {{number, number}}?, settle: number?, text: ((overlay) -> string?)?, ocrLabel: {number, number, number, number} | ((overlay) -> {…}?)?, hotkey: string?, hotkeyKeepsFocus: boolean?, rawOrigin: boolean?, fromRight: boolean?, button: ("left" | "right" | "middle")?, opensMenu: boolean?, menuItem: {number, number} | ((overlay, menu, factor) -> {number, number} | false | nil)?, retries: number?, when: ((overlay) -> boolean)? }`

Appends a button that, when activated, clicks a fixed origin-relative point. `at` is `{x, y}` relative to the origin; `hotkey` is an optional global activation hotkey spec (e.g. `"Alt+P"`); `opensMenu` says the click puts a menu on screen (see [`O.menuTests`](#o-menutests)).

On activate it clicks `(origin.x + at[1], origin.y + at[2])` — through the frame and the scale, when the overlay has them — logs `<overlay>: '<label>' activated, clicking (x,y)`, and speaks `"label, activated"`.

`button` is the mouse button of that click: `"right"` for a control a right click opens (VPS Avenger's redo list is a right click on UNDO), `"middle"`, or `"left"` — the default, and what [`host.input.click`](input.md#host-input-click) makes of any other value, which the overlay reports when it binds. It applies to `at` only, not to a `points` sequence.

`menuItem` makes the click at `at` open a menu and **chooses an item in it**: the item's offset from the menu's top-left corner, clicked when one of the overlay's menu tests sees the menu and says where it is, and clicked again while the menu is seen not to have taken it, up to `retries` more times (default 3) — see [`O:chooseMenuItem`](#o-choosemenuitem), which this is the control form of, for all of it. It implies `opensMenu`, needs `at` (a `points` sequence cannot open one) and menu tests on the binding; without tests the overlay reports the control when it binds, and pressing it opens nothing and says `"<label> is not available now"`. Pressed again while its menu is still awaited, it clicks nothing (logged); the first press stands.

Two checks come before the click. The point must fall inside the origin's own frame, and — where the platform can say — the window it belongs to must be the one actually **drawn** at that point; see [`host.window.ownsPoint`](window#host-window-ownspoint). A coordinate inside our rectangle can still be covered by a notification or another application, and a click that lands somewhere unknown while the overlay announces "activated" is a press with no way to tell where it went.

`text` is announced between the label and the word "button", and is how a control that acts can also say something about itself — whether it is available, or what it would act on.

`fromRight` measures `at[1]` from the coordinate window's **right edge** instead of its left — the click lands at `origin.x + width − at[1]` (ReaHotkey's `ControlX + ControlWidth − N`). Use it for plugin UI laid out from the right, so the target stays correct whatever the plugin's width is. Also accepted by [`addHotspotToggle`](#o-addhotspottoggle).

`points` replaces `at` with a **click sequence** — `points = {{x1,y1},{x2,y2},…}`, with `settle` ms between each (default 400) — for UI where reaching a control means getting there first: switch to its tab, then its sub-tab, then click it. Each step re-resolves the origin and goes through the overlay's own coordinate resolution, so a frame offset, `rawOrigin` or landmark anchoring applies exactly as for a single point. Making that one control keeps every action self-contained, so a user who cannot see a tab structure never has to navigate it.

`at` may also be a **function** `(overlay) -> {x, y} | nil`, for a control whose position depends on what is focused right now — one overlay serving several versions of a plugin whose chrome moved between them. Returning `nil` (the version isn't known yet) skips the click rather than guessing, without a word. What it returns is authored like a table: framed and scaled in an overlay that has a frame or a scale, unless the control is `rawOrigin`. Also accepted by `addHotspotToggle`.

In an overlay with [`O:scale`](#o-scale) whose factor cannot be told now, the click is not made and the control says `"<label> is not available now"`, with a `[place]` line in the log; the same holds for every step of a `points` sequence, which then ends where it is.

```luau
ov:addHotspotButton({ label = "Play", at = { 120, 40 }, hotkey = "Alt+P" })
-- 352 px in from the right edge, 87 px down — Kontakt's instrument arrows:
ov:addHotspotButton({ label = "Previous instrument", at = { 352, 87 }, fromRight = true, rawOrigin = true })

-- VPS Avenger's header: UNDO clicked, and its redo list opened with the right button. The list is
-- a popup of its own, so the keys go to it while a menu test sees it.
ov:addHotspotButton({ label = "Undo", at = { 305, 14 } })
ov:addHotspotButton({ label = "Redo list", at = { 305, 14 }, button = "right", opensMenu = true })
-- MENU, then its first item — chosen once a menu test sees the menu, 5 across and 14 down from its
-- corner (both scaled with the overlay; the numbers here are illustrative, not measured).
ov:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = { 5, 14 } })
-- A menu of tick boxes stays open after a choice: a second click would untick it again.
ov:addHotspotButton({ label = "Sync to host", at = { 400, 14 }, menuItem = { 5, 30 }, retries = 0 })
```

### Returns {#o-addhotspotbutton-returns}

The control table, appended to `o.controls`: `{ kind = "hotspot", label, ocrLabel, text, at, points, settle, hotkey, hotkeyKeepsFocus, opensMenu, rawOrigin, fromRight, button, menuItem, retries, when }`, with `opensMenu` true whenever `menuItem` is given and `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addhotspotbutton-errors}

None. It never answers an error value. A press it cannot make — the point outside the origin's window, another window drawn there, no factor from the scale — is said and logged at the press, as above, and a choice its `menuItem` could not make is said and logged as [`O:chooseMenuItem`](#o-choosemenuitem-errors)'s are; the module is handed none of them.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil` (Luau's own `attempt to index nil with 'label'`). Nothing else is checked at the call: a missing `at`, a `menuItem` without `at` or without menu tests, a `button` it does not know and the like are reported when the overlay binds, each in a line `[overlay] <overlay>: <finding>` that names the control. An `at` function is called unguarded at the press: what it raises ends that key's handler, and the host logs it.

## O\:group(pred, build) {#o-group}

**Signature:** `O:group(pred: (overlay) -> boolean, build: (overlay) -> ()) -> Overlay`

Adds everything `build` adds under a shared condition: `pred` is ANDed onto each control's own `when`, and groups nest. Prefer it over repeating the same `when` on every control of a set — forgetting one is silent.

```luau
ov:group(bare, function(ov)
    ov:addCustomButton({ label = "Load instrument", hotkey = "Ctrl+L", onActivate = load })
    ov:group(isVersion("Kontakt 7"), function(ov)
        ov:addHotspotButton({ label = "Library on/off", at = { 231, 19 }, hotkey = "Alt+L" })
    end)
end)
```

A `when` predicate must return exactly `true`; anything else counts as hidden. A hidden control is not Tab-reachable, its hotkey does nothing, and it does not claim a key combination that a visible sibling wants. A `when` runs in the runtime's own coroutine, once per scan, and must answer at once: no read that waits, and no read of the screen — a pixel it needs is a [probe](#o-probe)'s ([Where a hook runs](#where-a-hook-runs)).

A control's `hotkey` also **moves the overlay's focus** to that control, silently, before activating it — ReaHotkey's `TriggerHotkey` does the same. So after Ctrl+L for "Load instrument" and a dialog closed again, the overlay comes back on "Load instrument", and Tab carries on from there. The control then holds the keys a focused control holds (Space and Return for a button). `hotkeyKeepsFocus = true`, accepted by every constructor that takes a `hotkey`, opts out: Melodyne's "Menu bar" uses it, because focus on a button would take Space, which is Melodyne's play/stop. This changed on 2026-09-19 and affects every overlay with hotkeys on Windows as well (Kontakt, Komplete Kontrol, Soundiron, u-he).

A control's hotkey **hands its press on** (since version 0.12 of the runtime): the host's count of the key's quick repeats, `press.count` ([Quick repeats](hotkey.md#quick-repeats-once-twice-three-times)), reaches a custom button's or a stepper's `onActivate(overlay, press)` — 1 for a first press, 2 for a second quick one — once the hotkey's modifiers are up. The runtime registers its controls' hotkeys with a count at every press (`taps = "every"`): the first press of a double one has acted by the time the second comes, so a control does on 1 what is right whether or not a 2 follows. Space, Return and [`O:activate`](#o-activate) without a press hand `onActivate` none. A module that reads it depends on `"com.platform.overlay >= 0.12"`.

Space or Return on a focused control that has **hidden itself** since focus reached it acts on the visible control that shares its hotkey — Kontakt's "Switch to classic view" and "Switch to play view" share Alt+V and swap places on every press. With no such control, the overlay says "… is not available now" instead of doing nothing without a word.

Two controls **share** a hotkey when their specs name the same key on this platform, however they are written: the runtime compares them through [`host.keys.normalize`](keys.md#host-keys-normalize), so `"Cmd+S"` beside an inherited `"Ctrl+S"` is one claim on every platform (both are the Ctrl role, which is Command on a Mac). The key is spoken on focus in the platform's words through [`host.keys.describe`](keys.md#host-keys-describe) — `"Alt+V"` is "Option+V" on a Mac, and a tab's `hotkeyLabel` is said the same way when it is a key spec. A hotkey the host would refuse — one that is not a key spec, one the system keeps for itself, a modifier tap, or a key the platform has no code for (the reasons [`host.keys.check`](keys.md#host-keys-check) marks "raises") — is reported when the overlay binds, naming the control, left out of the `[keys] … holds:` line with its reason instead of being registered, and not spoken when the control or tab is focused. A hotkey the host claims but cannot hold at that moment — another module, program or the system holds it ([`host.hotkey.register`](hotkey.md#host-hotkey-register)'s `id, err`) — is kept, since the host tries the claim again, and listed in that line as not held now, with the error's message.

```luau
ov:group(function(o) return o.state.editorOpen == true end, function(g)
  g:addHotspotButton({ label = "Close editor", at = { 600, 12 }, hotkey = "Alt+E" })
end)
```

### Returns {#o-group-returns}

The overlay itself, so calls chain.

### Errors {#o-group-errors}

None. It never answers an error value.

**Raises** what `build` raises — it is called at once, unguarded — and so a control `build` adds after the overlay is bound raises there. `pred` is not checked at the call: anything but a function is a `when` that hides every control of the group, logged once per control when it is first asked (``<overlay>: '<label>' — its `when` failed (<message>); hiding just this control``).

## O\:probe(name, points, opts?) {#o-probe}

**Signature:** `O:probe(name: string, points: { {number, number} } | ((overlay) -> { {number, number} }?), opts: { read: ((colours: { Colour }, last: any, points: { {number, number} }) -> any)?, rawOrigin: boolean?, everyEpoch: boolean? }?) -> () -> any`, where `Colour` is [`host.screen.pixel`](screen.md#host-screen-pixel)'s

The pixels a `when` answers from, which the runtime reads in the handler before a scan. A `when` runs where nothing can wait, on every scan, and reads no screen there ([Where a hook runs](#where-a-hook-runs)); a probe is points of the plug-in that the runtime reads for it — every probe of the overlay that is due, with one [`host.screen.pixels`](screen.md#host-screen-pixels) — in the handler before a scan, and the `when` asks the probe's **reader** what was read. A ReaHotkey `PixelGetColor` in a visibility check is a probe here. Since version 0.5.0 of the runtime: a module that declares one depends on `"com.platform.overlay >= 0.5"`.

**The points** are a list of `{x, y}` in the overlay's own coordinates, placed as [`O:toScreen`](#o-toscreen) places them — through the frame and the scale, or as a `rawOrigin` control's with `opts.rawOrigin = true` — or a function of the overlay answering a list of screen points, asked each time the probe could be due, for points that move: measured from the right edge, after a scroll. Each point is rounded to the nearest whole pixel. A function that answers `nil`, and a point that cannot be placed now (no origin, no [scale](#o-scale) factor), read nothing: the probe keeps its value. A function that raises reads nothing either, nor does a list placed through a [frame](#o-frame) function that raises, logged once per message: `[probe] '<overlay>' '<name>': its points raised: <message> — not read`. An origin that raises reads no probe of the overlay, and says nothing here: the hook that asks the origin next raises with it and is logged as it always is. An empty list reads no pixel and is still handed to `read`.

**When it is read.** A probe is **due** when its key has changed since it was last read: the window the overlay is on (its origin's `id`), [`host.inputEpoch()`](timer.md#host-inputepoch) — which the module's own input, a window coming forward and a controller press turn over, and the user's own keys do not — the check 250 ms after an overlay of the module came to the front (the runtime's look again, which a reading taken while the screen still showed the window before needs), and its points on screen. A probe declared with **`everyEpoch = true`** is due in every new [epoch](timer.md#host-epoch) as well — at each Tab, each key of the overlay's and the other events the epoch lists — for what the user's own keys change in the plug-in, which turns over no input epoch: ON:EAR's speaker grid, which the user's typing in its search field refills while the Tab ring is elsewhere. It costs one capture per Tab. A probe whose change the module [expects](#o-expectchange) is due in every new epoch too, until a reading of the change is kept. A due probe is read:

- before a scan in a handler: a Tab's or Shift+Tab's search for the next control, a press of Return or Space (the `when` of the control pressed, and of those sharing its hotkey), a control's hotkey (its claimants'), the check 350 ms after the overlay came to the front, a calibration shot;
- when its reader is called in a handler outside a scan — in a `text`, an `onActivate`, a timer's callback;
- before the overlay's control hotkeys are registered again.

So a Tab after an act reads once, and Tabs in a row read nothing (an `everyEpoch` probe once each, and one whose change is expected once each until the change is read). It is **not** read inside a scan (a `when`, a `present`, a menu test: the reader answers what stands); on the menu tick, nor in anything the tick runs, such as the hotkeys taken back as a menu closes; nor in what a menu test's answer goes on to do when it comes in a callback of its own, or the answer of a calibrating run's [menu item shot](#o-calibrating) — the item clicked, a pick's `onDone`, the hotkeys taken back — since a reading right after the item's click would be of the menu still drawn; where nothing can wait (the arbiter's call that brings an overlay on a `slot` to the front, its `onActivate` and `onDeactivate`, a module's top level); nor while the overlay is not in front. The probes due in one handler are read in one call, in the order they were declared.

**A reading is of the screen as it is at that key.** A plug-in that has not redrawn by the first key after an act is read as it was, and once per act, that reading stands until the next act: no time decides when a redraw is done. An act of the module's that is meant to change what a probe shows says so with [`O:expectChange`](#o-expectchange), before it, and the probe is then read at each key until a reading shows the change. Without it, a Tab pressed quickly after a click that shuts a panel can keep the panel's controls in the ring until the next act. What the user's own keys change in the plug-in turns over no input epoch either, and is read after the next act, or at every Tab with `everyEpoch`.

**`read(colours, last, points)`** says what a reading means: `colours` one [`Colour`](screen.md#host-screen-pixel) per point, in order; `last` the probe's value before this reading; `points` the screen points read. Its answer is the probe's value. **`nil` is a reading it does not recognise**, which is not kept: the value stays as it was, and the probe is due again at the first scan in a later [epoch](timer.md#host-epoch) — not again in the same one, so a plug-in covered by another window costs one capture per key, not one per hook. Without `read` the value is the list of colours itself. Each probe's value is set before the next one's `read` runs, so a `read` may call an earlier probe's reader for what it holds now; a reader called inside a `read` reads nothing. A `read` runs in the handler, outside the scan, and should not wait; one that raises counts as `nil`, logged once per probe: `[probe] '<overlay>' '<name>': its read raised: <message> — the last value stands`.

**The reader**, `reader()`, answers the probe's value, or `nil` before a reading was first kept. The value outlives a stay in front and a change of window: an overlay that comes to the front on another window answers the last value until the first scan there reads it.

**A value that changed** — compared with `==`, and a list of colours colour by colour — registers the overlay's control hotkeys again, since the controls whose `when` asks the probe may have shown or hidden, and a hidden control claims no combination ([`O:group`](#o-group)); the `[keys] '<overlay>' holds: …` line says so when what is held changes.

**A capture that fails** reads nothing: every probe that was due keeps its value and is due again in a later epoch, logged once until a read succeeds: `[probe] '<overlay>': <n> probe(s) not read — <reason>` ([failure reasons](screen.md#failure-reasons)).

**A reading the overlay has left** is not kept: when the overlay is no longer where it was as the read was asked — out of the front, in another stay there, on another window, as [`O:stillHere`](#o-stillhere) asks without options — nothing of it is kept and no hotkey is registered, logged each time: `[probe] '<overlay>': <n> probe(s) read and not kept — <why>` in `stillHere`'s words. The probes stay due, and the next scan where the overlay is reads them.

**Cost.** Declaring one costs nothing. A scan with nothing due works out each probe's points — its function called, or its list placed — and compares a key. A scan with probes due adds one `host.screen.pixels` over all their points, which the handler [waits](screen.md#where-it-waits) for while the `screen-capture` thread reads them — the module's keys queue behind it, the event loop goes on: one capture of the box around them when they lie within 2,000,000 pixels of each other (see [its Cost](screen.md#host-screen-pixels)).

```luau
-- Kontakt 8's view, from one pixel below its top bar, measured from the right edge: 26 in the
-- play view (its scroll bar's track), 86 in the classic one (a button of the rack's header).
-- Within 10, not more: the scroll bar's thumb, 3 px to the right, reads 71, and must be neither.
local view = ov:probe("view", function(o)
  local origin = o:origin()
  local c = origin and origin.client
  if not c then return nil end -- no window now: nothing is read, the last view stands
  return { { c.x + c.w - 15, c.y + 59 } }
end, { read = function(colours)
  local v = (colours[1].r + colours[1].g + colours[1].b) / 3
  if math.abs(v - 26) <= 10 then return "play" end
  if math.abs(v - 86) <= 10 then return "classic" end
  return nil -- another picture: the last view stands, and the next scan looks again
end })

ov:addCustomButton({
  label = "Switch to classic view",
  hotkey = "Alt+V",
  when = function() return view() == "play" end,
  onActivate = function(o)
    -- Kontakt may redraw after the next key has come: the view is read at every key until it
    -- reads "classic" (O:expectChange).
    o:expectChange("view")
    host.input.send("F10")
  end,
})
```

### Returns {#o-probe-returns}

The probe's **reader**, a function of no arguments: `reader()` answers the probe's value — what `read` answered for the last reading kept, or the list of colours without `read` — or `nil` before a reading was first kept. Called in a handler outside a scan, it reads the overlay's due probes first; inside a scan, or inside a `read`, it answers what stands.

### Errors {#o-probe-errors}

None. It never answers an error value. Neither does its reader: a capture that fails, a reading `read` does not recognise and a reading the overlay has left keep the value that stands, and the log says why (above).

**Raises** for a mistake in the call: a probe declared after the overlay is bound (`overlay '<label>': probe must be called BEFORE binding it — the binding is what starts it`); a name that is not a non-empty string (`overlay '<label>': probe takes a name, got <type>`) or is one of the overlay's probes already (`overlay '<label>': a probe named '<name>' is declared already`); `points` that is neither a list of `{x, y}` (`… probe '<name>': points[<i>] is not {x, y}`) nor a function; `opts` that is not a table; an option other than `read`, `rawOrigin` and `everyEpoch`; and a `read` that is not a function.

### Windows {#o-probe-windows}

Read through `host.screen.pixels`: a `BitBlt` of the box around the points through the standard path, about one compositor frame (16.7 ms) for points close together; under a declared `capture = "duplication"` a 1×1 piece of one duplication request each, which the `screen-capture` thread waits for up to 250 ms ([`host.screen.pixels`](screen.md#host-screen-pixels)). Coordinates are device pixels.

### macOS {#o-probe-macos}

Read through `host.screen.pixels`: one capture of the box at point resolution on the `screen-capture` thread — one ScreenCaptureKit capture, measured at 36 to 91 ms — afresh on every call, with colours downsampled from a Retina display, so a `read` compares with a tolerance. Coordinates are points. Without the Screen Recording permission the colours are the wallpaper's: a `read` that recognises only the plug-in's own colours answers `nil`, and the last value stands.

## O\:expectChange(name, ...) {#o-expectchange}

**Signature:** `O:expectChange(name: string, ...: string) -> ()`

Says that what the module is about to do will change what the [probes](#o-probe) named show: a view toggle's key, a click that opens or shuts a panel. A probe is read once after an act, at the first key that scans, and a plug-in may not have redrawn by then; that reading would stand until the next act. With this, each probe named is due in every new [epoch](timer.md#host-epoch) — read before the scan of each Tab, each key of the overlay's and the other handlers the probe lists — until a reading of one of them is kept that differs from the value that probe held at the call. A key that comes before the redraw reads the screen as it was, and the next key reads again. No time decides anything: the redraw is waited for only by reading again at the next key. Since version 0.5.0 of the runtime: a module that calls it depends on `"com.platform.overlay >= 0.5"`.

**It reads nothing itself.** The value each probe holds at the call is what a later reading must differ from, so it is called before the act, or in the same handler right after it, before anything reads the probes. A probe with nothing read yet has `nil` there, and the first reading kept ends it.

**When it ends.** At the first kept reading of any probe named that differs from that probe's value at the call — compared as a [value that changed](#o-probe) is — the change counts as read for every probe named: each is due again only when its key changes, and a probe read in that capture stands under its key, so the Tabs after it read nothing (one named that was not read in it, its points not placed then, is read once more at the next key). A reading `read` does not recognise, a capture that fails and a reading the overlay left are not kept, and end nothing. A probe named again in a later call takes the later one's value and ends with the later one. The overlay on another window than at the call (another origin `id`) ends it at the next read there; an overlay with no origin at the call looks for the change on any window. An act that changed nothing — a key the plug-in did not take — leaves the probes due at every key until the overlay is on another window, or a later call names them.

**Cost.** The call costs a look at the overlay's origin and a table. Each key while the change is not read costs one [`host.screen.pixels`](screen.md#host-screen-pixels) over the points of the overlay's due probes, as a probe's read does — a wait of the key's handler, not of the event loop: usually one or two keys after the act. Once the change is read, Tabs in a row cost nothing again.

```luau
-- Kontakt 8's view toggle: F10, which Kontakt may draw after the next key has come.
ov:addCustomButton({
  label = "Switch to classic view",
  hotkey = "Alt+V",
  when = function() return view() == "play" end,
  onActivate = function(o)
    o:expectChange("view", "editor") -- both read from one picture, until either shows the change
    host.input.send("F10")
  end,
})
```

### Returns {#o-expectchange-returns}

Nothing.

### Errors {#o-expectchange-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: no name (`overlay '<label>': expectChange takes the names of one or more of its probes`), a name that is not a string (`overlay '<label>': expectChange takes probe names, got <type>`), and a name that is not one of the overlay's probes (`overlay '<label>': expectChange: no probe named '<name>'`). It may be called before or after the overlay is bound.

### Windows {#o-expectchange-windows}

Nothing of its own. Each read while the change is not read is the probe's: about one compositor frame (16.7 ms) for points close together ([`O:probe`](#o-probe)).

### macOS {#o-expectchange-macos}

Nothing of its own. Each read while the change is not read is the probe's: one capture of the box around the points, afresh on every read ([`O:probe`](#o-probe)), which took 36 to 91 ms through ScreenCaptureKit where it was measured.

## Bindings — where an overlay lives {#bindings}

A **binding** is an inert value saying *where* an overlay lives: which window or embedded control, on which `slot`, at which `specificity`, with `pollMatch` / `menus`. Declare it once and reuse it, instead of writing a factory function that rebuilds an attach spec per attachment — the reason such factories appear is that a caller mutates the table and the next caller needs a clean copy. [`O.window`](#o-window) takes what [`attach`](#o-attach) takes, and [`O.embedded`](#o-embedded) what [`attachEmbedded`](#o-attachembedded) takes — so an embedded binding with no control of its own for a platform gets the DAW's plug-in panel there, as `attachEmbedded` describes, and every overlay bound to one `O.embedded` value shares the control it resolves, once per epoch, and on a panel its identity verdict. [`:with`](#binding-with) derives a variant, [`O.hosts`](#o-hosts) lists the hosts, and [`O:bind`](#o-bind) attaches an overlay by one. In their signatures, `AttachOpts` is the options table `attach` and `attachEmbedded` take: `{ hoverToRead, menus, slot, specificity, pollMatch, pollWhen }`.

One overlay object takes **one** binding: an object is pinned to a single slot and its match poll belongs to that join. Two places to live means two overlay objects sharing a build function.

**Never in this application's own windows.** When the window in front is one of the application's own — the module manager, a dialog of the host's: a [window table](./window.md#window-table) whose `own` is `true` — every overlay lets go at the window or focus event that brings it, and asks nothing there: no context, no `identify`, no gate, no menu test, and nothing of the window that has just lost the foreground. One on a `slot` reports to the arbiter that it does not match, before the test that would skip an outranked overlay's recheck, so one outranked until then says so at its own recheck too; one without a slot leaves the front. Whatever its matcher says: a standard dialog of the application is a `#32770`, as REAPER's FX window is. The window in front is read once per [epoch](./timer.md#host-epoch) for every overlay of the module, from `host.window.active()`, which keeps its answer for the epoch anyway.

The overlays recheck one after another, and the arbiter elects again at every report. So when a slot's winner lets go first, the arbiter can elect a claim whose last report still says it matches — made in the other program's window by an overlay outranked since, which has skipped its rechecks — before that overlay's own recheck has run. An overlay the arbiter calls to come up while one of the application's own windows has the keyboard does not come up: it reports that it does not match instead, and the arbiter elects again at once, so none comes up there. With no window shown in front it reads [`host.window.foreground()`](./window.md#host-window-foreground)'s `own` for this, each time it is called to come up: the hidden window the tray icon puts in front for its menu on Windows is no window to `active()`. There the rechecks run as they do with nothing in front, and only an overlay called to come up sees that the keyboard is the application's.


**Leaving the front.** Every time an overlay leaves the front, one line says what is in front now and which test failed, after a semicolon:

```text
[deactivate] 'VPS Avenger' — in front now: 'Inst 1' (Logic Pro); the keyboard is on [AXButton//] at 410,55, outside the plug-in's panel (the DAW's own controls)
```

The reason is the first test that said no, in these words (`<…>` is filled in):

| Test | Reason |
|---|---|
| this application's own window in front | `one of this application's own windows is in front ('<title>')`; the overlay's own page: `its page is in front` |
| a window binding | `no window is in front` · `'<title>' is not a window its binding matches` |
| an embedded binding's window | `no window is in front` · `'<title>' is not a plug-in window of the binding's hosts` |
| the module's pattern | `no control matches '<pattern>'` · `identify says control [<class>] id=<id> is not the plug-in` · `identify could not tell yet about control [<class>] id=<id> (<n> of 8)` · `identify raised about control [<class>] id=<id>` · `identify is not asked inside a scan of the overlay's hooks` |
| a `control` function | `the module's control function found no control` · `the keyboard is outside what the module's control function returned` |
| `identify` asked at every evaluation | `identify, asked at every evaluation, says no` |
| `present` | `its binding's present says no` |
| the gate | `its gate says no`; a [landmark](#o-landmark): `its landmark is not seen` |
| its menu's window ([holding its place](#o-menutests)) | `its place over its menu's window is no longer held; <the binding's reason>` |
| a [trip](#o-inplugin)'s held place | `the plug-in's window held for the trip is not in front` |
| none of the above (a binding with no context left) | `nothing it is bound to matches` |
| the arbiter, while the overlay still matched | `another claim on slot '<slot>' was chosen: <module> at specificity <n>` (the claim [`host.arbiter.participants`](arbiter.md#host-arbiter-participants) names active, which the host marks before it calls the old one off) · with no claim active: `the arbiter called it off on slot '<slot>' while it still matched` |

An overlay with two bindings — sforzando's standalone window and its embedded binding — says both reasons, the first binding's first, joined by `; `. A reason is made of what the test had in hand (the title, the focused element and its place), kept as values and put into words only for this line, and an evaluation that matches makes none. Nothing is asked of the host for a reason, but for the arbiter's: that one calls [`host.arbiter.participants`](arbiter.md#host-arbiter-participants) once, when the line is written.

### Windows {#bindings-windows}

An embedded binding's reasons include the DAW's chrome classes: `the keyboard is on the DAW's own control [<class>]` (REAPER's FX list, `SysListView32`). The panel's reasons below do not occur, since no daw-hosts entry gives a panel on Windows.

### macOS {#bindings-macos}

An embedded binding on the DAW's plug-in panel ([`attachEmbedded`](#o-attachembedded-macos)) says where the keyboard was found: `the keyboard is on [<AXRole/AXSubrole/AXIdentifier>] at <x>,<y>, outside the plug-in's panel (the DAW's own controls)` (the focused element's centre, in screen points) · `the focused element [<class>] has no rectangle` · `the focus chain ends in another window than '<title>'` · `where the keyboard is in '<title>' cannot be read (an empty focus chain), and no stay is in progress there` · `where the keyboard is in '<title>' cannot be read (an empty focus chain), and the DAW's entry gives this binding no plug-in panel there` · `where the keyboard is in '<title>' cannot be read (an empty focus chain), and the module's control function has found no control with the keyboard in it in this stay` · `the DAW's entry gives no plug-in panel in '<title>'`. And what `identify` said of the panel: `identify says the panel shows another plug-in` · `identify could not tell yet (<n> of 8)` · `identify could not tell 8 times: no for this stay` · `identify is not asked inside a scan of the overlay's hooks` · `identify raised`.

## O.window(matcher, opts?) {#o-window}

**Signature:** `O.window(matcher, opts: AttachOpts?) -> Binding`

A binding to a **standalone** window: what [`O:attach(matcher, opts)`](#o-attach) does when an overlay is bound by it. `matcher` is a window matcher for [`host.window.test`](window.md#host-window-test), and `opts` the options `attach` takes (`hoverToRead`, `menus`, `slot`, `specificity`, `pollMatch`, `pollWhen`). Nothing is checked or asked here: the binding is a value, and what is wrong with it shows when an overlay is bound by it.

```luau
local STANDALONE = O.window({ app = { exe = { contains = "sforzando" } } },
  { menus = { O.menuTests.nativePopup, O.menuTests.newWindow } })
ov:bind(STANDALONE)
```

### Returns {#o-window-returns}

A new `Binding`: `{ target = matcher, opts = opts or {} }` with the binding metatable, every time. `matcher` and `opts` are held as given, not copied.

### Errors {#o-window-errors}

None. It never answers an error value.

It never raises.

## O.embedded(spec, opts?) {#o-embedded}

**Signature:** `O.embedded(spec, opts: AttachOpts?) -> Binding`

A binding to a plug-in **embedded** in a DAW: what [`O:attachEmbedded(spec, opts)`](#o-attachembedded) does when an overlay is bound by it. `spec` is the spec `attachEmbedded` takes (`hosts`, `control`, `identify`, `cacheIdentity`, `present`), and every overlay bound to the same value shares the control it resolves, once per epoch, and on a panel its identity verdict.

```luau
local HOSTED = O.embedded({
    hosts = O.hosts(daw.all, kk and kk.standaloneWindow),
    control = "Qt%d+.-QWindowIcon",
    identify = identifyHost,
    cacheIdentity = false,
}, { slot = SLOT, menus = { O.menuTests.nativePopup, O.menuTests.accessibility } })
```

### Returns {#o-embedded-returns}

A new `Binding`: `{ target = spec, opts = opts or {} }` with the binding metatable, every time. `spec` and `opts` are held as given, not copied — the table is what the shared resolution is keyed by.

### Errors {#o-embedded-errors}

None. It never answers an error value.

It never raises.

## Binding\:with(over) {#binding-with}

**Signature:** `Binding:with(over: { [string]: any, opts: AttachOpts? }?) -> Binding`

A **copy** of the binding with `over` merged into its target (`control`, `identify`, `title`, …), and `over.opts` merged into its options. The binding it is called on is not changed. A copy of an `O.embedded` binding is a new spec table, so its overlays do not share the control the original's resolve.

```luau
-- `macos = false`: the dialog is Kontakt's own, never a plug-in panel, so on a Mac — where a
-- plain pattern that finds nothing would take the DAW's panel — the binding does not apply.
dialog:bind(HOSTED:with({ control = { windows = "^NIChildWindow%x+$", macos = false },
                          identify = present, opts = { menus = false } }),
            { specificity = O.layer.dialog })
```

### Returns {#binding-with-returns}

A new `Binding` of the same kind (window or embedded): its target a shallow copy of the original's with `over`'s fields set over it, and its options a shallow copy with `over.opts`'s set over them. `nil` for `over` gives a plain copy.

### Errors {#binding-with-errors}

None. It never answers an error value.

**Raises** for an `over` that is neither a table nor `nil`, and an `over.opts` that is not a table (Luau's own `pairs` error).

## O.hosts(...) {#o-hosts}

**Signature:** `O.hosts(...: { Entry } | Entry | nil) -> { Entry }`

Concatenates host lists and skips `nil`, so that an optional host, a plug-in that may not be installed, can be listed inline. A table with an array part is a list of entries. Any other non-empty table is one entry; an entry may carry nothing but a platform block, as Logic's does in [daw-hosts](../daw-hosts.md). An empty table adds nothing. Every argument is looked at, a `nil` in the middle included.

```luau
local hosts = O.hosts(daw.all, kk and kk.standaloneWindow)   -- kk may not be installed
```

### Returns {#o-hosts-returns}

A new list of the entries, in the order given; `{}` when there are none.

### Errors {#o-hosts-errors}

None. It never answers an error value.

It never raises: an argument that is not a table is skipped like `nil`.

## O\:bind(binding, opts?) {#o-bind}

**Signature:** `O:bind(binding: Binding, opts: AttachOpts?) -> Overlay`

Attaches the overlay by `binding`: [`O:attach`](#o-attach) for an `O.window` binding, [`O:attachEmbedded`](#o-attachembedded) for an `O.embedded` one, with `opts` set over the binding's own options for this overlay alone (the binding is not changed). From here the overlay is live: what must be declared before binding — controls, probes, a frame, a scale, a gate, a page — raises after it.

```luau
base:bind(HOSTED, { specificity = O.layer.base })
dialog:bind(HOSTED, { specificity = O.layer.dialog })
```

### Returns {#o-bind-returns}

The overlay itself, so calls chain.

### Errors {#o-bind-errors}

None. It never answers an error value.

**Raises** what [`O:attach`](#o-attach-errors) and [`O:attachEmbedded`](#o-attachembedded-errors) raise for its options, and Luau's own `attempt to index nil with 'opts'` for a `binding` that is `nil`.

## O.layer {#o-layer}

The specificity ladder within a slot, named: `chrome` (a host's own frame), `base` (the plugin's generic header), `content` (what is loaded inside it right now), `dialog` (a modal that must own the keyboard). Pass `specificity = O.layer.content` rather than a bare number.

```luau
-- One arbiter slot, four overlays, most specific match wins: Komplete Kontrol's own chrome
-- yields to the Kontakt loaded inside it, that yields to the library loaded inside THAT,
-- and all three yield to a modal while it is up.
kk:bind(HOSTED, { specificity = O.layer.chrome })         -- the host's own frame
kontakt:bind(HOSTED, { specificity = O.layer.base })      -- the plug-in's generic header
library:bind(HOSTED, { specificity = O.layer.content })   -- what is loaded in it right now
contentMissing:bind(DIALOG, { specificity = O.layer.dialog })
```

### Returns {#o-layer-returns}

A table of four numbers, the same table every time in a VM: `chrome = 10`, `base = 20`, `content = 30`, `dialog = 40`. Spaced by ten, so a layer nobody has met yet can go between two of them. It is a plain table, not frozen: writing to it changes it for every overlay in the VM, so a module reads it and never writes it.

## O.errors {#o-errors}

**Signature:** `O.errors: { [string]: string }`

The codes of the runtime's error values, as constants: its own eight, and every code of [`host.errors`](./errors.md#host-errors) through it, so `O.errors.NO_MENU` is `"no_menu"` and `O.errors.BUSY` is `host.errors.BUSY`. A name that is neither raises and lists the runtime's — `O.errors has no 'NO_MENNU'; its codes are NOT_IN_FRONT, MOVED_ON, UNPLACED, CLOSED, NO_MENU, NO_SUBMENU, UNFIT_SUBMENU, CLASH and every one of host.errors's` — so a misspelt constant fails where it is written, not as a comparison that is never true. Read-only: assigning raises. Since version 0.11 of the runtime: a module that reads it depends on `"com.platform.overlay >= 0.11"`.

| code | constant | meaning |
|---|---|---|
| `not_in_front` | `NOT_IN_FRONT` | What this needs in front is not: the overlay, the page, or the plug-in's window. |
| `moved_on` | `MOVED_ON` | The user moved on since: a key, another press, another window or application, the focus, a later announcement, a menu opened. |
| `unplaced` | `UNPLACED` | It cannot be placed now: no origin, no factor from the [scale](#o-scale), or a placing function that answered nothing or raised. |
| `closed` | `CLOSED` | What it was for was closed: the menu, or the page. |
| `no_menu` | `NO_MENU` | No menu it could choose in was seen before the bound. |
| `no_submenu` | `NO_SUBMENU` | The submenu did not open. |
| `unfit_submenu` | `UNFIT_SUBMENU` | A submenu opened, and none it could choose in was seen. |
| `clash` | `CLASH` | A control of the page's overlays has that hotkey. |

The runtime's other answers carry host codes — `busy`, `gone`, `refused`, `outside`, `covered`, `not_found`, `taken`, `invalid`, `not_supported`, `not_ready`, `cancelled`, `failed` — each entry below says which. Every error value the runtime makes comes from one of its own reasons, in `modules/overlay-runtime/src/errors.luau`: a code, a message, and the entries that give it. One it only hands on — a capture's, a page's, a focus's, a hotkey registration's — is the host's own table, as the host answered it.

```luau
local opened, err = o:chooseMenuItem({ label = "Zoom", at = { 21, 14 }, menuItem = { 5, 5 },
  onDone = function(_, chosen, later)
    if not chosen and later.code == O.errors.NO_MENU then host.log.info("Zoom: " .. later) end
  end })
if not opened and err.code == O.errors.BUSY then return end   -- the last choice is still being made
```

### Returns {#o-errors-returns}

The table itself, its constants strings. `pairs(O.errors)` gives the runtime's eight; the host's are reached by name.

## O.memoByOrigin(fn, opts?) {#o-memobyorigin}

**Signature:** `O.memoByOrigin(fn: (origin, ...) -> value, opts: { key: ((origin) -> any)? }?) -> (origin, ...) -> value`

Memoizes a per-window property that does not change while that window exists — which plugin version it is, whether it is a host container, where its content begins.

One rule: a **non-nil** result is cached; **`nil` means "cannot tell yet" and is not cached**. Identity is usually resolved through UIA, which is not ready the instant a plugin window appears, and pinning that first answer is how an overlay ends up permanently convinced it is driving a different plugin version. `false` is a real answer and *is* cached.

`opts.key(origin)` extends the cache key past the window handle — for a property that survives a window *move* but not a *resize*.

It keeps the answers for the **64 windows (keys) asked about most recently** and forgets the one asked about least recently when a 65th comes: a window that closed is never asked about again, and the application runs for days. A window forgotten and asked about again has `fn` called for it once more. A hit costs a table lookup; a new key at a full memo costs one pass over its 64 entries.

The same holds for the verdicts of an embedded binding's `identify` on Windows (see [`attachEmbedded`](#o-attachembedded); on a Mac's plug-in panel a verdict is kept for a stay instead): kept for the 64 controls asked about most recently, and a `nil` from `identify` is not a verdict — the control is asked again at the next recheck, up to 8 times in a row, with one `attachEmbedded: … identify could not tell yet` line per control; the eighth `nil` in a row is kept as "no", as `false` would be, with one line saying so.

```luau
local variantOf = O.memoByOrigin(function(ctrl)
    local i = host.element.findAny(ctrl.id, VERSIONS, { U.Window, U.Pane })
    return i and VERSIONS[i] or nil   -- nil: UIA not ready, ask again next time
end)
```

### Returns {#o-memobyorigin-returns}

The memo, a function `(origin, ...) -> value`. It answers what `fn(origin, ...)` answered for that window (and `opts.key`'s value), from the memo when a non-`nil` answer is kept — only the first value `fn` answered is kept and handed back. An `origin` that is `nil` or has no `id` is not memoised: `fn` is asked every time, and its answers come back as they are.

### Errors {#o-memobyorigin-errors}

None. It never answers an error value.

It never raises at the call: nothing is checked there. The memo raises what `fn` or `opts.key` raises — neither is guarded, and nothing is kept for that call — and Luau's own error at its first call when `fn` is not a function.

## O.memoByEpoch(fn) {#o-memobyepoch}

**Signature:** `O.memoByEpoch(fn: (...) -> value) -> (...) -> value`

Memoizes `fn` for the current [epoch](timer.md#host-epoch) only: until the epoch turns over the first call asks `fn` and every later call gets that answer back, whatever arguments it is given; the first call in a new epoch asks again. The epoch turns over on an OS event, a `host.timer.after` coming due, a read's or search's answer, `host.window.focus` and the module's own input — **not** on a [`host.timer.every`](timer.md#host-timer-every) tick, so a poll on `every`, the overlay's menu tick among them, gets the same answer on tick after tick until one of those happens. For a question whose answer holds only until the world moves — a reading of the screen, a walk of the accessibility tree — asked by several controls in one announcement, or by a [scale](#o-scale) factor at every coordinate one calibration shot places.

The same rule as [`O.memoByOrigin`](#o-memobyorigin): a **non-nil** answer is kept; `nil` means "could not tell yet" and is asked again at the next call; `false` is kept. An answer is kept under the epoch it was given in, read when `fn` returns, not under the one it was asked in: an `fn` that waits — a [`host.ocr.recognize`](ocr.md#host-ocr-recognize) without a callback — lets other events turn the epoch over meanwhile, and its answer is then kept for the epoch it came back in. Only the current epoch's answer is held — one slot, not a table — so nothing accumulates, and a stale answer cannot be read after the epoch has turned over. The arguments are not part of the key: a function asked about two different things in one epoch is two memos. `fn` is not guarded: what it raises comes out of the memo's call, and nothing is kept. A hit costs one call of [`host.epoch`](timer.md#host-epoch) and a comparison.

```luau
-- ON:EAR's accessibility tree, read once per epoch however many read-outs ask about it.
local tree = O.memoByEpoch(function() return host.element.rawDump(ov:hwnd()) end)

-- A zoom read off the plug-in's zoom field, for the overlay's scale: once per epoch, not once per
-- coordinate. nil (nothing read) is asked again, and places nothing meanwhile.
local zoomOf = O.memoByEpoch(function(origin) return readZoom(origin) end)
```

### Returns {#o-memobyepoch-returns}

The memo, a function `(...) -> value`. Within one epoch it answers the first non-`nil` value `fn` answered in it, whatever it is handed; until `fn` has answered one, every call asks `fn` again. Only the first value `fn` answers is kept and handed back, every time.

### Errors {#o-memobyepoch-errors}

None. It never answers an error value.

It never raises at the call: nothing is checked there. The memo raises what `fn` raises — `fn` is not guarded, and nothing is kept for that call — and Luau's own error at its first call when `fn` is not a function.

## O\:origin() {#o-origin}

**Signature:** `O:origin() -> Control | Window | nil`

The active context's coordinate window — the plug-in control when embedded (on a Mac, the DAW's [plug-in panel](#o-attachembedded-macos)), the window when standalone. This is the module's handle on the thing it overlays; use it instead of reaching into `activeCtx`. It is resolved at the call — once per [epoch](timer.md#host-epoch) per binding for an embedded overlay, which keeps the answer for the epoch; a window query for a standalone one ([`host.window.active`](window.md#host-window-active), which keeps its own answer for the epoch).

```luau
-- `:origin()` -- the context itself, whose `client` rect every authored coordinate is
-- measured against. ON:EAR works a point in its design back out of where a word was read:
local function designY(overlay, screenY)
  local o = overlay:origin()
  if not (o and o.client and o.client.h > 0) then return nil end  -- not active: no answer
  return (screenY - o.client.y) / (o.client.h / DESIGN_H)
end
```

### Returns {#o-origin-returns}

- A [control table](window.md#table-shapes) (`id`, `class`, `bounds`, `client`, and on a Mac's panel `stay`) for an embedded overlay, or a [window table](window.md#window-table) for a standalone one.
- `nil` while the overlay is not active, while its context finds nothing now, and for the rest of an epoch in which its origin raised as it came to the front.

### Errors {#o-origin-errors}

None. It never answers an error value. An origin it cannot find now is `nil`.

**Raises** what the binding's `control` function raises, which is called unguarded (see the top of this page). `identify` is guarded.

## O\:hwnd() {#o-hwnd}

**Signature:** `O:hwnd() -> number | nil`

The handle of [`O:origin()`](#o-origin): its `id`. On Windows the plug-in's own child window when embedded and the window standalone; on a Mac's plug-in panel, the plug-in window's id. One origin resolution, as `O:origin()` costs.

```luau
-- `:hwnd()` -- the window handle, for asking the accessibility layer about it:
onActivate = function(o)
  local p = host.element.locate(o:hwnd(), "", host.element.type.Edit)
  if p then host.input.click(p.x, p.y) end
end,
```

### Returns {#o-hwnd-returns}

The origin's `id`, a number; `nil` whenever `O:origin()` is `nil`.

### Errors {#o-hwnd-errors}

None. It never answers an error value.

**Raises** what [`O:origin()`](#o-origin) raises.

## O.contentSize(origin) {#o-contentsize}

**Signature:** `O.contentSize(origin: Control | Window | nil) -> (number, number)`

The width and height of an origin's **content**, which is what coordinates are measured against — each on its own: the width is `client.w` when that is above 0 and `bounds.w` otherwise, the height `client.h` or else `bounds.h`, and `0` where neither is there; `0, 0` for `nil`. Anything measured from the right edge (`fromRight`) or sized from the window has to use this. A function of the runtime, not a method; it reads the table it is handed and asks the host nothing.

```luau
-- A factor for a plug-in whose design is 1010 wide, from the width it is drawn at now.
ov:scale(function(o)
  local w = O.contentSize(o)
  return w > 0 and w / 1010 or nil
end)
```

### Returns {#o-contentsize-returns}

Two numbers, the width and the height: each the `client`'s when it is above 0, else the `bounds`'s, else `0`; `0, 0` for `nil` or `false`. Never `nil`. It raises only for an argument it cannot index, such as a number.

### Windows {#o-contentsize-windows}

An embedded plug-in's control is borderless, so its `client` and `bounds` are the same size and either answers alike. A standalone window's are not: a Kontakt 8 window is 1026 wide around 1010 of plug-in, and the content is the 1010.

### macOS {#o-contentsize-macos}

A DAW's plug-in panel ([`attachEmbedded`](#o-attachembedded-macos)) has `client` equal to `bounds`, so the two agree there. A standalone window's `client` is derived from its own geometry rather than read ([window table shapes](window.md#table-shapes), macOS): a titled window's content below its title bar, and the frame itself for a borderless one — its content size is then the frame's.

## O\:frame(fn) {#o-frame}

**Signature:** `O:frame(fn: (origin) -> (number, number)) -> Overlay`

Shifts the overlay's whole coordinate frame: `fn` returns `dx, dy`, resolved per active control — a host version's content shift, or a nested plugin's inner origin. Controls marked `rawOrigin` opt out, and so does a landmark-anchored overlay while its landmark is on screen ([`O:landmark`](#o-landmark)): it places from the landmark's corner. Called before the overlay is bound; afterwards it raises. A second call replaces the first.

The shift is **not scaled**: in an overlay with [`O:scale`](#o-scale) it is added to the origin first, and the scaled part of a coordinate after it — the frame is where the part of the geometry that does not zoom goes: an offset of the plug-in's own corner from the origin (VPS Avenger's -2, +2 on a DAW's panel), or a centring (ARC ON:EAR's half width). Never the DAW's own chrome: daw-hosts has put the origin below it already ([daw-hosts](../daw-hosts.md)), and DAW geometry in a plug-in's module would have to be written again for every DAW. `fn` is called at every resolution, with the origin, and is not guarded: an error in it raises out of whatever was placing the control — [`O:toScreen`](#o-toscreen) included.

```luau
-- A Kontakt nested inside Komplete Kontrol: the origin is KK's container control, but every
-- coordinate is authored against Kontakt's own client area. Shift the frame by the
-- difference, and fall back to a measured constant while the nested control cannot be
-- enumerated -- returning nothing here would leave every coordinate unresolvable.
ov:frame(function(kkCtrl)
  local k = findNestedKontakt(kkCtrl)
  if not (k and k.client and kkCtrl.client) then
    return 198, 222   -- KK's browser is covering it; measured offset
  end
  return k.client.x - kkCtrl.client.x, k.client.y - kkCtrl.client.y
end)
```

### Returns {#o-frame-returns}

The overlay itself, so calls chain.

### Errors {#o-frame-errors}

None. It never answers an error value.

**Raises** for a call after the overlay is bound: `overlay '<label>': frame must be called BEFORE binding it — the binding is what starts it`. `fn` is not checked at the call; what it raises later, or a non-function's call error, comes out of whatever was placing a coordinate then.

## O\:scale(fn, opts?) {#o-scale}

**Signature:** `O:scale(fn: (origin) -> number?, opts: { about: {number, number}? }?) -> Overlay`

Scales every authored coordinate of the overlay by the factor `fn` answers, for a plug-in that zooms its whole interface. VPS Avenger is the case it was built for: it zooms from 50 to 200 %, and by the avenger_control project's account a point measured at 50 % from its corner lands at *zoom / 50* times that distance. Every coordinate goes through one formula,

```text
screen = origin + frame + k × (authored − about)
```

rounded once, to the nearest whole unit — a pixel on Windows, a point on macOS — with a half rounded up: `origin` is the client-area top-left as above, `frame` what [`O:frame`](#o-frame) answers (unscaled), `k` what `fn(origin)` answers now, and `about` the authored point that stays where the frame puts it — `{0, 0}` unless `opts.about` says otherwise. `about` is for a plug-in that scales around something other than its corner: ARC ON:EAR scales by its window's height about its design's centre line, which is `about = {960, 0}` with a frame of half the window's width. Two cases do not fit the formula as written: `fromRight` is `origin + frame's dx + content width − k × at[1]` across (a distance from the right edge, which `about` has nothing to say about), with `y` as above; and a landmark-anchored overlay ([`O:landmark`](#o-landmark) with `anchor = true`), while its landmark is on screen, is `landmark's corner + k × (authored − about)`, with no frame.

It applies to `at` (a table or a function's answer), `points`, `region` (a table or a function's answer), `ocrLabel`, a tab's `at`, a `reveal` probe, a slider's `from` and `to` (and so the size of one arrow-key step, one percent of the track as drawn), a graphical button's `clickOffset` and a fixed `dragBy` (distances: scaled, not framed and not moved by `about`), a menu item's offset from its menu's corner when it is a table ([`O:chooseMenuItem`](#o-choosemenuitem): a distance too, at the factor its opener was placed with), the points of [`O:toScreen`](#o-toscreen) and `captureRegion`, and every crosshair of a calibration shot. It does **not** apply to a `rawOrigin` control, which is in the origin's own pixels — neither framed nor scaled — to a `dragBy` function's or a `menuItem` function's answer, or anything else worked out from a search's hit or a menu's rectangle, which are screen pixels already, or to template images, which are matched at their own `scales`.

`fn` is called with the origin at **every** resolution — once per point or region a control places, so a calibration shot calls it at least once per control and once more for each region, name region and menu opener it lists — and its answer is not kept by the runtime. A factor that costs something to learn, such as a zoom read off the plug-in's own caption, is memoised by the module: [`O.memoByEpoch`](#o-memobyepoch) keeps it until the epoch turns over. It is called guarded. Anything but a number above 0 and below infinity — `nil`, which is how a factor says "cannot tell now", a raise, a string, 0, a negative number, NaN — is **no factor**: nothing the overlay places is clicked or read. A control that would have clicked says `"<label> is not available now"` and logs `[place] '<overlay>': '<label>' cannot be placed now — <why>; nothing clicked` — a slider's arrow keys and a graphical button's press included; an OCR control announces `"cannot be read now"` in place of its value; a toggle announces no state; a tab stays where it was and says `"<tab> tab is not available now"`. Never 1: a factor guessed at 1 is a click at the wrong place, said with the confidence of the right one.

The factor is logged when it **changes** — `[scale] '<overlay>': factor 1.6000 about (0,0)`, or `[scale] '<overlay>': no factor now — <why>; nothing it places is clicked or read until it answers one` — never per resolution.

An overlay that does not call it is placed exactly as before: no rounding, and nothing in this section applies. Called before the overlay is bound; afterwards it raises, and a `fn` that is not a function or an `about` that is not two numbers raises at the call. A second call replaces the first.

A function rather than a design size to compare with the window's: neither overlay that scales today has a size to compare. Avenger's factor is its zoom **read** off its own zoom field — its width has an offset nobody has measured, and a zoom worked out from the width reads 80 % as 75 % — and ON:EAR's is its height alone, its width being free. A design size is one line inside `fn`; a read zoom is no design size at all.

```luau
-- A plug-in whose layout is written at 50 % of its zoom, the zoom read off its zoom field once
-- per epoch. `readZoom` is the module's own (an OCR read); nil until it has one, and then nothing
-- is clicked at a guessed size. On a Mac the zoom is the whole factor; on Windows the display's
-- scaling multiplies it and has to be learned from the screen (see Windows below) — VPS Avenger's
-- module learns it from where its header is drawn. `displayFactor` is the module's own too.
local zoomOf = O.memoByEpoch(function(origin) return readZoom(origin) end)
ov:scale(function(origin)
  local zoom = zoomOf(origin)
  return zoom and zoom / 50 * displayFactor(origin) or nil
end)

-- ARC ON:EAR (modules/ik-on-ear/src/geometry.luau): the window scales by its height and centres
-- the design across, so the design's centre line is put at half the window's width.
ov:frame(function(o) return o.client.w / 2, 0 end)
ov:scale(function(o) return o.client.h > 0 and o.client.h / 1009 or nil end, { about = { 960, 0 } })
```

### Returns {#o-scale-returns}

The overlay itself, so calls chain.

### Errors {#o-scale-errors}

None. It never answers an error value. A factor that cannot be had later is no error of this call: each call that places answers it or says it — [`O:toScreen`](#o-toscreen-errors) with `unplaced`, a control with `"<label> is not available now"`.

**Raises** for a mistake in the call: a call after the overlay is bound (`overlay '<label>': scale must be called BEFORE binding it — the binding is what starts it`), a `fn` that is not a function (`overlay '<label>': scale takes a function(origin) -> factor, got <type>`), and an `about` that is not two numbers (``overlay '<label>': scale's `about` must be {x, y}, got <type>``).

### Windows {#o-scale-windows}

A coordinate is a physical pixel ([Coordinates](index.md#coordinates)), so a plug-in drawn at 150 % display scaling is 1.5 times the size it has at 100 %, at the same zoom. The factor has to carry that: one learned from what is on screen — a width, a height, the distance between two things read by OCR — already does; one computed from the plug-in's zoom alone does not, and nothing in the host says which scaling a window is drawn at.

### macOS {#o-scale-macos}

A coordinate is a point, and a Retina display draws two pixels per point, so the factor is the same on a Retina display as on one without: a zoom read off the plug-in, divided by the zoom the coordinates were measured at, is the whole factor there.

## O\:toScreen(x, y, opts?) {#o-toscreen}

**Signature:** `O:toScreen(x: number, y: number, opts: { rawOrigin: boolean? }?) -> (number, number) | (nil, Error)`

Where an authored point lands on screen now: what an `at` of the overlay would click, through the same origin, frame and [scale](#o-scale), for a module's own code — a custom button's `onActivate`, a stepper's `onStep`, a hook that clicks what it read. `opts.rawOrigin = true` places it as a `rawOrigin` control would: in the origin's own pixels, neither framed nor scaled. Its rectangle twin is [`O:toScreenRect`](#o-toscreenrect).

In an overlay without a scale it is the plain sum, `origin + frame + authored`, unrounded — whole numbers in, whole numbers out. In a scaled one it is the scale's formula, rounded once to the nearest whole unit. The factor is asked guarded; the [frame](#o-frame) function is not, and what it raises comes out of this call. It clicks and reads nothing itself, and costs one origin resolution (memoised per [epoch](timer.md#host-epoch) for an embedded overlay) plus the frame's and the factor's functions, on the event loop.

```luau
-- A stepper that turns a knob by dragging it a few authored units, at whatever zoom it is drawn.
onStep = function(dir, o)
  local x1, y1 = o:toScreen(262, 140)
  local x2 = o:toScreen(262 + 6 * dir, 140)
  if x1 and x2 then host.input.drag(x1, y1, x2, y1) end
end,

-- A point that cannot be placed now is logged, not guessed.
onActivate = function(o)
  local x, y = o:toScreen(120, 40)
  if x == nil then
    local err = y   -- on failure the second value is the error value
    host.log.info("[my-module] Play not clicked: " .. err)
    return
  end
  host.input.click(x, y)
end,
```

### Returns {#o-toscreen-returns}

- `x, y` — the point on screen, two numbers.
- `nil, err` — it cannot be placed now; `err.code` is `unplaced`. The error is the **second** value, where `y` would be, so a caller tests `x` and reads the error from the second place.

### Errors {#o-toscreen-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `unplaced` | return | The overlay is not active, or its context finds no origin now (no plug-in control, no window in front). | `the overlay has no origin now` | both |
| `unplaced` | return | A scaled overlay, without `rawOrigin`: the scale's function raised. Its message is the raise's. | `its scale function raised: …` | both |
| `unplaced` | return | A scaled overlay, without `rawOrigin`: the scale's function answered `nil`, which is how it says "cannot tell now". | `its scale function answered nil` | both |
| `unplaced` | return | A scaled overlay, without `rawOrigin`: the scale's function answered something other than a number above 0 and below infinity — a string, 0, a negative number, NaN. | `its scale function answered …, not a factor above 0` | both |

**Raises** for a mistake in the call: an `x` or `y` that is not a number (Luau's own arithmetic error). And what the frame function raises.

### Windows {#o-toscreen-windows}

The answer is in physical pixels, as every coordinate is.

### macOS {#o-toscreen-macos}

The answer is in points, as every coordinate is; a factor that is not a whole number can put a point between two pixels of a Retina display, and it is rounded to the nearest whole point like any other.

## O\:toScreenRect(r, opts?) {#o-toscreenrect}

**Signature:** `O:toScreenRect(r: {number, number, number, number}, opts: { rawOrigin: boolean?, whole: boolean? }?) -> {number, number, number, number} | (nil, Error)`

Where an authored rectangle `{x1, y1, x2, y2}` lands on screen now: what a `region` of the overlay would read, each corner placed as [`O:toScreen`](#o-toscreen) places a point — through the origin, the frame and the [scale](#o-scale), or in the origin's own pixels with `opts.rawOrigin = true`. For a module's own code: a `text` function that reads by OCR, a read with a callback, a snapshot of a part of the plug-in. It clicks and reads nothing itself, and costs what `O:toScreen` costs.

`opts.whole = true` makes the rectangle one that [`host.ocr.recognize`](ocr.md#host-ocr-recognize) and [`host.screen.snapshot`](screen.md#host-screen-snapshot) take as it is: whole numbers, not empty. Each corner is cut toward zero — `134.5` becomes `134`, `-965.5` becomes `-965` — which is the pixels the overlay's own read of a `region` covers; and when nothing is left once cut — corners turned around, or less than one across or down — the answer is `nil, err`, `outside`. An overlay with a fractional frame places fractions otherwise, and `recognize` refuses them at the call. In a scaled overlay the corners are whole numbers already, and `whole` changes nothing but the empty case. Any value but `true` leaves the corners as placed, turned around or not. `whole` came with version 0.2 of the runtime: a module that uses it depends on `"com.platform.overlay >= 0.2"`.

```luau
-- A read-out that reads its own region: the handler that announces it waits for the reading.
text = function(o)
  local r = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
  return r and host.ocr.recognize(r).text or nil
end,

-- The same region read with a callback: the press returns at once, and the value is said later.
onActivate = function(o)
  local r, err = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
  if not r then   -- no origin, no factor, or nothing of it on screen
    if err.code == O.errors.OUTSIDE then host.speech.output("The name is not on screen") end
    return
  end
  host.ocr.recognize(r, function(reading)
    if reading.status == "text" then host.speech.output(reading.text, { interrupt = true }) end
  end)
end,
```

### Returns {#o-toscreenrect-returns}

- `{x1, y1, x2, y2}` — a new table, the corners on screen. With `whole`, whole numbers with `x2 > x1` and `y2 > y1`; without it, as placed.
- `nil, err` — it cannot be placed now (`unplaced`), or with `whole` nothing of it is left on screen (`outside`).

### Errors {#o-toscreenrect-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `unplaced` | return | The overlay is not active, or its context finds no origin now. | `the overlay has no origin now` | both |
| `unplaced` | return | A scaled overlay, without `rawOrigin`: the scale's function raised. | `its scale function raised: …` | both |
| `unplaced` | return | A scaled overlay, without `rawOrigin`: the scale's function answered `nil`. | `its scale function answered nil` | both |
| `unplaced` | return | A scaled overlay, without `rawOrigin`: the scale's function answered something other than a number above 0 and below infinity. | `its scale function answered …, not a factor above 0` | both |
| `outside` | return | With `whole = true`: nothing is left once the corners are cut — corners turned around, or less than one across or down. | `empty on screen` | both |

**Raises** for a mistake in the call: an `r` that is not a table of four numbers (Luau's own indexing or arithmetic error). And what the frame function raises.

### Windows {#o-toscreenrect-windows}

The corners are in physical pixels, as every coordinate is.

### macOS {#o-toscreenrect-macos}

The corners are in points, as every coordinate is; a factor that is not a whole number can put a corner between two pixels of a Retina display, and it is rounded to the nearest whole point like any other.
## O.state {#o-state}

A free-form table on every overlay for the owning module's own state, so it does not have to squat in the runtime's reserved `_`-prefixed fields.

```luau
-- Where the Width handle was left, so the next step continues from there instead of
-- re-reading a value the drag has not settled to yet. On the overlay rather than in a
-- file-level local: one module can build more than one overlay from the same file -- ON:EAR
-- builds its Settings window from this one -- and a shared upvalue would answer for the
-- wrong window.
ov:addStepper({
  label = "Width",
  text = function() return valueBelow("Width") or "not shown" end,
  onStep = function(dir, o)
    o.state.widthLastX = (o.state.widthLastX or widthHandleX(widthNow())) + dir * STEP_PX
    dragWidthTo(o.state.widthLastX)
  end,
})
```

### Returns {#o-state-returns}

A table, `{}` when the overlay is made, the same one for the overlay's life: the runtime never reads, writes or clears it, through every stay in front, every window and every page round trip.

## O.stateGeneration() {#o-stategeneration}

**Signature:** `O.stateGeneration() -> number`

A count that goes up by one 250 ms after an overlay of this VM comes to the front, once per arrival: the runtime's "look again", for a reading of the screen taken in the first moments after a focus change, which can still show the window that was in front before. A module that keeps a screen reading of its own keys it on this count — read again when the count has moved — so the reading taken while the screen still showed the old window is replaced exactly once. A reading taken afresh each time needs nothing: the look again rechecks the overlay, and its probes are read again by it ([`O:probe`](#o-probe)).

A function of the runtime, not a method, and a read of one number: no host call, on the event loop.

```luau
-- A per-window reading of the plug-in's view, read again once after each arrival.
local seen = { gen = -1, view = nil }
local function viewNow(o)
  if seen.gen ~= O.stateGeneration() then
    seen.gen, seen.view = O.stateGeneration(), readView(o)   -- readView is the module's own
  end
  return seen.view
end
```

### Returns {#o-stategeneration-returns}

A whole number, `0` when the runtime is loaded into the VM, one more at each look again of any overlay of the VM — counted only while that overlay is still in front 250 ms after it came. It never goes down, and it never raises.

## ocrLabel — reading a control's name off the screen {#ocrlabel}

Any hotspot or hotspot-toggle may carry `ocrLabel = {x1, y1, x2, y2}` (origin-relative): the control's spoken **name** is then read by OCR from that region instead of announced from the static `label`, which becomes the fallback for when OCR reads nothing.

Use it where the plugin itself changes what a control means. A sample library can put one mixer strip in a fixed place whose five channels are microphone positions for one patch and orchestral sections for another — same buttons, same pixels, different names. A fixed label is then confidently wrong, which is worse than being slow: it tells a user who cannot see the screen that they toggled something they did not.

Costs one OCR read per focus, so put it on controls whose name genuinely varies, not on every control. The name is read off the event loop, as an OCR button's value is (see [`O:addOCRButton`](#o-addocrbutton)): the announcement is one sentence, said when the answer comes and only while it is still about what is in front of the user, and each answer writes a line `[read] '<label>' name = "<text>" (region x1,y1 wxh, <ms> ms, <n> words, <language>)` with the same parts and endings as the value's line. A name read as nothing, or not read, leaves the static `label`; one read as several lines is said as one, the lines joined with a space.

Like a `region`, it may be a function of the overlay answering the corners, and it is scaled in an overlay with [`O:scale`](#o-scale). A name region that cannot be placed now — the function answers `nil`, or the scale has no factor — is not read, and the static `label` is announced, as when OCR reads nothing.

```luau
ov:addHotspotToggle({
    label = "Spot 1 Mic",              -- fallback if OCR reads nothing
    at = { -130, 350 },                -- the power icon
    ocrLabel = { -154, 358, -106, 376 }, -- the caption printed under it
    onColor = { 180, 165, 230 },
    offColor = { 77, 79, 85 },
})
```

## O\:addCustomButton(opts) {#o-addcustombutton}

**Signature:** `O:addCustomButton(opts) -> Control`, where `opts: { label: string, onActivate: (overlay, press: { count: number }?) -> (), text: ((overlay) -> string?)?, typeLabel: string?, editable: boolean?, opensMenu: boolean?, hotkey: string?, hotkeyKeepsFocus: boolean?, when: ((overlay) -> boolean)? }`

Appends a button that runs a Luau callback on activation. `onActivate` receives the overlay itself (so a header control can reach the active context), and, when the control's `hotkey` pressed it, that press: `press.count` is the key's count of quick repeats, 1, 2, 3 ([`O:group`](#o-group), since version 0.12 of the runtime). Space and Return hand it none.

`onActivate` is called **guarded**. A control whose action throws says so rather than going silent: the host used to catch the error at the key dispatch and write a line, while the person who pressed it heard nothing at all — which is indistinguishable from a control that quietly did its job. It runs in the handler of the key or hotkey, and may read there; one that reads and then acts checks that the overlay is still where it was before it acts ([Where a hook runs](#where-a-hook-runs)).

`text` is announced between the label and the type word, and is how a control that ACTS can also say what it would act on. It is not read before `onActivate` runs: only a stepper's value is.

`typeLabel` overrides the word for what this control *is*. The kind a control is built from and the thing it stands for are not always the same: a custom button that puts the caret into a text field is an edit box to everyone using it, and announcing "button" describes the implementation to somebody with no interest in it — and says the wrong thing about what typing will do next.

`editable` says that activating this control leaves the caret in a text field, so **Space belongs to the application** from then on. Return still activates, because it is the only way back into the field after tabbing away. This is the same rule an `addOCREdit` gets by virtue of its kind; the flag is for controls that reach their field some other way (a coordinate, an accessibility element) and would otherwise swallow the space in "White 80s".

```luau
-- Once reads the bank's name, twice quickly reads its whole path. `bankName` and `bankPath` are
-- the module's own.
ov:addCustomButton({
  label = "Bank",
  hotkey = "Alt+B",
  onActivate = function(o, press)
    local twice = press ~= nil and press.count == 2
    host.speech.output(twice and bankPath(o) or bankName(o))
  end,
})

-- Komplete Kontrol's library browser hides the instrument loaded behind it, so the overlay
-- offers a way to close it -- and offers it only while one is actually open.
ov:addCustomButton({
  label = "Close library browser",
  hotkey = "Alt+L",
  when = function(o) return browserToggle(o:hwnd()) ~= nil end,
  onActivate = function(o)
    local p = browserToggle(o:hwnd())   -- a UIA point, re-resolved at press time
    if p then host.input.click(p.x, p.y) end
  end,
})
```

### Returns {#o-addcustombutton-returns}

The control table, appended to `o.controls`: `{ kind = "custom", label, onActivate, text, typeLabel, editable, opensMenu, hotkey, hotkeyKeepsFocus, when }`, `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addcustombutton-errors}

None. It never answers an error value. An `onActivate` that raises is logged, `<overlay>: '<label>' failed while activating: <message>`, and said, `"<label> could not be used"`; the module is handed nothing.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil` (Luau's own `attempt to index nil with 'label'`). Nothing else is checked at the call; a field of the wrong kind is reported when the overlay binds.

## O\:addStepper(opts) {#o-addstepper}

**Signature:** `O:addStepper(opts) -> Control`, where `opts: { label: string, text: (overlay) -> string?, onStep: (dir: number, overlay) -> false?, onActivate: ((overlay, press: { count: number }?) -> ())?, settle: number?, typeLabel: string?, hotkey: string?, hotkeyKeepsFocus: boolean?, when: ((overlay) -> boolean)? }`

Appends a **value changed with Left and Right**, where the module knows how to change it.

Announced as a slider, because that is what it is to the person using it; how it is driven underneath is the module's problem rather than theirs.

Use it where `addSlider` cannot serve. That one finds its thumb by matching an image, which needs a template captured for one plug-in at one size — no use for a rotary drawn as an arc, a bar with two handles, or anything in a window the user can resize. What such a control does have is a value printed beside it and something that moves it, and that is all a stepper is.

- `onStep(dir, overlay)` — `dir` is `-1` for Left and `+1` for Right. Called guarded, in the handler of the arrow key, where it may read ([Where a hook runs](#where-a-hook-runs)); one that raises says `"<label> could not be moved"`. It returns nothing, or `false` for a press that moved nothing and has nothing of its own to say — one that came while an earlier step is still being carried out, whose own announcement is on its way: nothing is watched or announced for it.
- `onActivate` — optional, and what a **press** means. Without it, Space and Return are still captured on a stepper (it is not an inert control) and then do nothing at all, which is a promise without an action. ON:EAR's two use it for "double-click to put this back to its default", which is one keystroke instead of twenty. Called `(overlay, press)`, `press` the hotkey's when its `hotkey` pressed it, as a custom button's ([`O:addCustomButton`](#o-addcustombutton)).
- `settle` — how long to wait **at most** for the value to change before announcing it anyway, in milliseconds, default 600. Not how long to wait: see [`O:watch`](#o-watch). A step that goes through a menu — [`O:chooseMenuItem`](#o-choosemenuitem), whose item is clicked only once the menu is seen — needs a longer one. It is read after each `onStep` returns, from the returned control's `settle` field, so an `onStep` that knows nothing will change — a zoom already at its last step — sets it short on the control before it returns, and the value is said at once.

`text` is read once before each step and each press — the value the announcement waits to see change — and again until it does. What is announced afterwards always comes from reading `text` again, never from what the step intended. A control that reports its own intention rather than the application's state is the failure this project keeps returning to. A `text` that raises reads as nothing: the watch waits until `settle` and the value is said without it (see the top of this page).

`hotkey` and `hotkeyKeepsFocus` are taken as by every constructor (see [`O:group`](#o-group)); the hotkey activates the stepper as Return does.

```luau
ov:addStepper({
  label = "Tone",
  when = function() return element("Tone") ~= nil end,
  text = function() return valueBelow("Tone") or "not shown" end,
  onStep = function(dir, o)
    local x, y = o:toScreen(290, 780)   -- a design point, through the overlay's scale
    if x then host.input.scroll(x, y, dir * 0.5) end
  end,
})

-- A step that is carried out through the plug-in's own list: a press while the last one is still
-- under way sends nothing and says nothing of its own; at either end nothing is opened, and the
-- value is said at once. `busy`, `atEnd` and `openAndChoose` are the module's own.
local zoom
zoom = ov:addStepper({
  label = "Zoom",
  settle = 3000,
  text = function(o) return readZoomText(o) end,
  onStep = function(dir, o)
    if busy() then return false end
    if atEnd(dir) then zoom.settle = 1; return end
    zoom.settle = 3000
    if not openAndChoose(o, dir) then return false end
  end,
})
```

### Returns {#o-addstepper-returns}

The control table, appended to `o.controls`: `{ kind = "stepper", label, text, onStep, onActivate, settle, typeLabel, hotkey, hotkeyKeepsFocus, when }`, `hotkeyKeepsFocus` made a boolean. Its `settle` is read at every step, so a module sets it on this table.

### Errors {#o-addstepper-errors}

None. It never answers an error value. An `onStep` that raises is logged, `<overlay>: '<label>' failed while stepping: <message>`, and said, `"<label> could not be moved"`; an `onActivate` that raises is said `"<label> could not be used"`.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil` (Luau's own `attempt to index nil with 'label'`). Nothing else is checked at the call.

## O\:addSlider(opts) {#o-addslider}

**Signature:** `O:addSlider(opts) -> Control`, where `opts: { label: string, region: {number, number, number, number} | ((overlay) -> {…}?), thumbImage: string | { string }, from: number, to: number, hotkey: string?, hotkeyKeepsFocus: boolean?, tolerance: number?, scales: { number }?, when: ((overlay) -> boolean)? }`

Appends a **graphical slider** — ReaHotkey's `GraphicalHorizontalSlider` — whose value is where its thumb is drawn. The thumb is found by an [image search](screen.md#host-screen-imagesearch) of `thumbImage` inside `region`, and its centre against `from` (0 %) and `to` (100 %), two x coordinates in the overlay's own, gives a percentage. `thumbImage` may be a list, for a thumb drawn one way at rest and another while it is dragged; every path is absolute ([`host.path`](path.md#host-path)). `tolerance` and `scales` are the search's, a per-control override of the overlay's defaults (a tolerance of 20 and ten scales from 0.67 to 2). It is announced `"<label>, slider"`, and its percentage after that, as a value: `"<n> percent"`, or `"not found"` when the thumb is not in the region.

**Left and Right** move it one percent of the track as drawn ([`O:adjust`](#o-adjust)): the thumb is dragged that far, looked for again 25 ms later, and when the reading has not moved, dragged one pixel further, up to 24 times, until the percentage changes or the track's end is reached; then the new percentage is said. Every step after a search goes on only while the overlay is where the key was pressed — active, in the same stay, on the same window ([`O:stillHere`](#o-stillhere) without options); otherwise the log says `[overlay] <overlay>: '<label>' not going on <after what> — <why>`. The point each drag ends at is checked as every click of the overlay is, and a drag refused there ends the step. The region, `from` and `to` are scaled in an overlay with [`O:scale`](#o-scale); the template is not.

**Cost.** One image search per announcement, with a callback. A step costs one search, then one before and one after each drag — each a capture and a match, which the handler waits for: the arrow key's, then the timer's.

```luau
-- A mic blend slider: a thumb on a track 40 to 240 across (the numbers are illustrative).
ov:addSlider({
  label = "Mic blend",
  region = { 30, 300, 250, 320 },
  thumbImage = host.path("images/Blend/Thumb.png"),
  from = 40, to = 240,
})
```

### Returns {#o-addslider-returns}

The control table, appended to `o.controls`: `{ kind = "slider", label, region, thumbImage, from, to, hotkey, hotkeyKeepsFocus, tolerance, scales, when }`.

### Errors {#o-addslider-errors}

None. It never answers an error value. A thumb not found is said, `"<label>, not found"`, and a track that cannot be placed now is said `"<label> is not available now"`.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil`. Nothing else is checked at the call: no `region`, no `thumbImage`, a missing `from` or `to`, a `to` not right of `from`, and a path that is not absolute are reported when the overlay binds.

## O\:adjust(dir) {#o-adjust}

**Signature:** `O:adjust(dir: number) -> ()`

One step of the **focused** control's value, as Left (`-1`) and Right (`+1`) make it: a [slider](#o-addslider)'s drag, or a [stepper](#o-addstepper)'s `onStep(dir, overlay)` and the announcement of what the value became. Left and Right are bound to it by the runtime while a slider or a stepper is focused, so a module calls it only to step from another key. Nothing happens when the focused control is hidden, is of another kind, or is a stepper without `onStep`.

```luau
-- A second pair of keys for the focused value, for a plug-in whose arrows the user wants.
host.hotkey.register("Alt+Up", function() if ov.active then ov:adjust(1) end end)
host.hotkey.register("Alt+Down", function() if ov.active then ov:adjust(-1) end end)
```

### Returns {#o-adjust-returns}

Nothing. What the step did is said, and logged where it was not made.

### Errors {#o-adjust-errors}

None. It never answers an error value.

**Raises** only for a `dir` that is not a number on a slider (Luau's own arithmetic error). A stepper's `onStep` and `text` run guarded, and a stepper is handed `dir` as given.

## O\:afterIdle(key, ms, fn) {#o-afteridle}

**Signature:** `O:afterIdle(key: any, ms: number, fn: (overlay) -> ()) -> ()`

Runs `fn` **once**, `ms` after the last call carrying the same `key`. Every call restarts the clock, so a run of keystrokes produces exactly one action at the end of it rather than one per press.

The reason is a specific failure, and it was found from both directions on one control. Two clicks close together in time and space are a **double-click**, and on ON:EAR's Width slider a double-click resets it — so arrow keys pressed quickly enough were two clicks two pixels apart, and the slider jumped back to its default in the middle of an adjustment. Waiting out the double-click time between presses instead would make holding an arrow useless. So the presses accumulate in the module's own state, and the gesture goes out once they stop.

`key` names the thing being coalesced, so one overlay can have several in flight without them interfering.

Bound to the overlay: a pending action is dropped if the overlay is no longer active, or if the window it was aimed at is no longer in front. A click that lands after the world has moved is the mistake that pressing the key again cannot undo. Without a timer available there is nothing to coalesce with, so `fn` runs immediately — the alternative would be never running it at all.

```luau
-- Each press moves a pending position; the click goes out when the presses stop.
onStep = function(dir, o)
  widthPendingX = (widthPendingX or currentHandleX()) + dir * STEP_PX
  o:afterIdle("width", 260, function()
    local x, y = screenPoint(widthPendingX, 869)
    widthPendingX = nil
    if x then host.input.click(x, y) end
  end)
end,
```

### Returns {#o-afteridle-returns}

Nothing. `fn(overlay)` runs later, as a handler of the timer's, or never: when a later call with the same `key` came, or the overlay is not active on the same window by then.

### Errors {#o-afteridle-errors}

None. It never answers an error value. An `fn` that raises is logged, `<overlay>: coalesced '<key>' failed: <message>`.

It never raises for `key` or `fn`: an `fn` that is not a function makes the call do nothing. An `ms` that is not a number of milliseconds raises as [`host.timer.after`](timer.md#host-timer-after) raises for it.

## O.doubleClick(x, y, opts?, cb?) {#o-doubleclick}

**Signature:** `O.doubleClick(x: number, y: number, opts: { button: ("left" | "right" | "middle")? }?, cb: ((ok: boolean, err: Error?) -> ())?) -> true | (false, Error)`

A double-click at the point, made by the host as **one gesture**. It is [`host.input.click`](input.md#host-input-click) with `clicks = 2`, which makes both clicks on a thread of its own. A module-level function rather than a method — it touches no overlay state, and checks nothing about the point: place and check it first, as any click of a module's own (`O:toScreen`, [`host.window.ownsPoint`](window.md#host-window-ownspoint)).

Worth having in one place because the same fact is needed from both sides. A plug-in that resets a control to its default on a double-click is offering a genuinely useful gesture — one keystroke instead of twenty steps — and reaching it means sending two clicks close enough together, which the system takes for one double-click. That same fact is a hazard everywhere else, which [`O:afterIdle`](#o-afteridle) exists to avoid.

The gesture, since version 0.15 of the runtime: the first press — the pointer moved to the point — released 20 ms later, and 60 ms after that the second press, released 20 ms later, the presses about 80 ms apart. On a Mac the second pair carries a click state of 2, the first of 1, as a double-click of a mouse does; Windows counts the two clicks itself. Before 0.15 the runtime sent two plain clicks 90 ms apart on a timer of its own: on a Mac those were two single clicks, each with a click state of 1, and VPS Avenger in Logic took no reset from them (the third Mac test, 2026-10-25). The timing is the VPS Avenger control tool's, whose double-clicks Avenger takes, and no turn of the event loop that runs long stretches it: the waits are on the host's thread.

- `opts.button` — the button **both** clicks are made with: `"left"` (the default), `"right"` or `"middle"`. VPS Avenger resets Pan and Random Pitch with a double-click of the right button.
- `cb` — called once, as a handler of its own, where a read may wait, with the gesture's answer once both clicks have gone out: `cb(true)`, the place to start watching for what the double-click changed rather than a guess at when it is over. Or `cb(false, err)`: the host did not send the second click — the window at the point changed after the first, `covered` — or the system did not take one of them, or the host refused the double-click at the call.

**It answers** as `host.input.click` does — `true`, or `false` and the host's error value — since version 0.11 of the runtime; before, both answers were dropped. `true` says the host has the gesture; what came of it is `cb`'s. No `window` is given, so nothing is refused for its window at the call (Returns, below).

**Cost.** One [`host.input.click`](input.md#host-input-click) call: at the call its checks, on a Mac the window server's hit test at the point; then about 100 ms on the host's `input-gesture` thread, with nothing on the event loop. Nothing is read.

```luau
-- ON:EAR's Tone and Width both reset to their default on a double-click, which is one
-- keystroke against twenty steps. Aimed at where the handle IS, not at the middle of the
-- track: the two handles stop about ten per cent apart, so the middle is on the far side
-- of the fold and belongs to the other one.
onActivate = function()
  local x, y = screenPoint(widthHandleX(widthNow()), 869)
  if x then O.doubleClick(x, y) end
end,

-- A pan knob that resets on a double-click of the right button, and the knob read once both
-- clicks are out: `before` is a snapshot the module took before the double-click, so the change
-- wait compares with the knob as it was.
O.doubleClick(x, y, { button = "right" }, function(ok, err)
  if not ok then return host.log.info("the pan knob was not reset: " .. err) end
  host.screen.snapshot({ region = knobBox, change = { from = before, timeout = 2000 } }, function(snap)
    if snap then sayPan(readPan(snap)) snap:release() end
  end)
end)
```

### Returns {#o-doubleclick-returns}

- `true` — the host has the double-click, and makes it after the call returns; its answer goes to `cb`: `cb(true)` once both clicks are out, or `cb(false, err)`.
- `false, err` — the host refused it at the call, and nothing was clicked; `err` is the host's own table ([`host.input.click`](input.md#host-input-click-returns)), and the log says `overlay: a double-click at (<x>,<y>) was not made — <message>`. With a `cb`, it hears the same, `cb(false, err)`, on a later turn as a handler of its own, from the host. So a `cb` that tests `ok` before watching for a change does not watch for one that was never asked for.
- A `cb(false, err)` after `true` may come after the first click was made: the second not sent (`covered`), or one of its events not taken. The program has had one click then, which a reset by double-click does not take.

### Errors {#o-doubleclick-errors}

None of these is raised. Each is handed on as the host answered it.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [host.input.click's errors](input.md#host-input-click-errors) | both | A double-click through the system's input given no `window`, the host's: refused at the call — at the call and to `cb` — or, after its hand-over, the window at the point changed after the first click (`covered`, the second not sent), a press or a release the system did not take or make, or the host's thread for it — to `cb` only. | the host's | both |

**Raises** for a mistake in the call, before anything is clicked: an `opts` that is not a table or `nil` (`O.doubleClick: opts must be a table { button = … } or nil, got string`), a `button` it does not know (`O.doubleClick: opts.button must be "left", "right" or "middle", got x`), and a `cb` that is neither a function nor `nil` (`O.doubleClick: the callback (fourth argument) must be a function or nil, got number`). What `host.input.click` raises for `x` and `y` comes out of the call. `cb` is not guarded: what it raises is reported as any callback's of the host's is.

### Windows {#o-doubleclick-windows}

The host places the pointer at the point and sends the press and the release of the button, each a `SendInput` of its own, twice ([`host.input.click`](input.md#host-input-click-windows), `clicks = 2`); Windows counts the two as a double-click when they come within its double-click time and rectangle, as from a mouse — and 80 ms is inside any double-click time a user sets. ON:EAR's resets go this way.

### macOS {#o-doubleclick-macos}

The second pair carries a click state of 2 and the first of 1, as a mouse's double-click does, which AppKit hands the program as each event's `clickCount` ([`host.input.click`](input.md#host-input-click-macos), `clicks = 2`); the VPS Avenger control tool posts the same, with the same timing. That Avenger takes it from this application as it takes the tool's is not measured yet.

## O\:watch(spec) {#o-watch}

**Signature:** `O:watch(spec) -> any`, where `spec: { read: (overlay) -> (any, any?), was: any?, done: ((now: any, was: any, n: number) -> boolean)?, every: number?, within: number?, onDone: ((now, was, n) -> ())?, onGiveUp: ((now, was, n) -> ())? }`

Waits for something to **change**, rather than for a length of time.

Almost every `host.timer.after` in a module is a guess about how long an application takes, and a guess is wrong in both directions. Too short and the value read back is the one from *before* the action — a real value announced with confidence for the wrong moment, which is how ON:EAR's speaker grid came to name the previously loaded speaker each time a tile was pressed. Too long and everything feels slow.

- `read` — what to look at. **`nil` means "cannot read right now", which is not the same as "unchanged" and never counts as done.** Treating those as the same thing is the specific bug this replaces. An optional second value names the **reading** the value came from — anything compared with `==`, typically a number the read's callback counts up — for a `read` that hands back the last answer of a read with a callback, which several looks can share: a look that names the same reading as the last one read is no new reading, however many looks that read `nil` came in between. Without it every look is a reading, which is right for a read that waits (a pixel, a text read without a callback).
- `was` — the reading from before the action. Omit it and `read` is called once to get it, which is only correct if nothing has happened yet; a reading it names is then the one the first look is compared with.
- `done(now, was, n)` — defaults to "it changed". Asked once per **new** reading that is not `nil`, with `n`, how many readings in a row have read `now`, this one included: 1 the first time. A look that reads `nil` neither counts nor breaks the run; a reading of another value starts it again at 1. So `done` can wait for a value read twice — a plug-in that draws something else on its way to the new value (VPS Avenger's loading display before the preset's name) shows it once, and the name again and again.
- `every` — how often to look, default 100 ms. Each look costs whatever `read` costs, and a screen pixel is a compositor frame, so this is not free.
- `within` — give up after this long and call `onGiveUp` with the last reading, default 900 ms; counted in whole `every`s. Not an error: a radio button that was already chosen is *meant* not to change.
- `onDone(now, was, n)` and `onGiveUp(now, was, n)` get the count as `done` does; `onGiveUp`'s `n` is 0 when the last look read `nil`.

Each end writes one line: `[watch] <label>: changed after <ms> ms — <was> -> <now>` or `… gave up after …`, with ` (<n> readings alike)` after it when `n` is more than 1.

Bound to the overlay. It stops the moment the overlay is no longer active or the window it was watching is no longer in front — a callback arriving after the world has moved is the mistake that pressing the key again cannot undo. It ends **unreported** then: neither `onDone` nor `onGiveUp` is called. So a watch from before a [page](#o-addpage) came to the front ends with nothing said; a [trip](#o-inplugin)'s act takes its own watches, and one whose overlay leaves — a sibling box taking the slot — ends unreported too, while the act must still call `trip:done`.

```luau
self:watch({
  read = function() return valueBelow("Width") end,
  was = before,
  within = 600,
  onDone = say,
  onGiveUp = say,   -- it did not move, and that is worth saying too
})
```

A name read with a callback, one read out at a time: the watch looks every 100 ms, a read answers in 130 to 200 on a Mac, so the reading's number tells the watch which looks share an answer. The name counts once two readings agree, or at once when it is a name the module knows.

```luau
-- VPS Avenger, after ◀ or ▶ (modules/vps-avenger/src/main.luau, shortened).
local last, seq, reading = nil, 0, false
local function nameNow(o)
  if not reading then
    reading = true
    host.ocr.recognize(o:toScreenRect(NAME), { key = "name" }, function(r)
      reading = false
      if r.status == "text" then last, seq = r.text, seq + 1 end
    end)
  end
  if last then return last, seq end   -- the value, and the reading it came from
end

o:watch({
  read = nameNow,
  was = before,
  done = function(now, was, n) return now ~= was and (n >= 2 or catalog[now]) end,
  within = 2000,
  onDone = function(now) host.speech.output(now, { interrupt = true }) end,
  onGiveUp = function(now, was, n)
    if now ~= was and n < 2 and not catalog[now] then now = nil end   -- read once: not a name
    host.speech.output(now or "the preset name cannot be read now", { interrupt = true })
  end,
})
```

### Returns {#o-watch-returns}

The reading it compares with: `spec.was` when given, otherwise what `read` answered at the call (`nil` when it raised). Nothing when `spec.read` is not a function: no watch is started. What comes later goes to `onDone` or `onGiveUp`, each called at most once, and neither when the watch ends unreported.

### Errors {#o-watch-errors}

None. It never answers an error value. A look that could not read is `nil` from `read`, and a watch that ends without a change is `onGiveUp`, not an error.

**Raises** only for a `spec` it cannot index: `nil` gives Luau's own `attempt to index nil with 'read'`. Every hook is called guarded: a `read` that raises reads `nil`, a `done` that raises is not done, and what `onDone` or `onGiveUp` raises is dropped without a line.

## O\:here() {#o-here}

**Signature:** `O:here() -> Mark`

Notes where the overlay is now, as a **mark**, for an answer that comes later and acts on what it learned — a [`host.ocr.recognize`](ocr.md#host-ocr-recognize) callback, a timer. [`O:stillHere(mark)`](#o-stillhere) then says whether that still holds. The two are the checks the runtime makes itself before it says a value read off the screen and before an OCR button clicks ([`O:addOCRButton`](#o-addocrbutton)), with the messages in the same words as its `[read]` lines.

A mark notes the stay in front (every time the overlay comes to the front is a new one — a quiet arrival in a [trip](#o-inplugin) included), the window — its origin's handle, [`O:hwnd()`](#o-hwnd) — the focus and the control on it, and how many keys of the overlay's own, announcements begun and said, and menu openings it has seen. A mark taken while the overlay is not active never holds, except with `place = false`.

It runs where it is called, on the main thread, and costs a small table and one origin resolution (memoised per [epoch](timer.md#host-epoch) for an embedded overlay). It touches no screen. It came with version 0.2 of the runtime: a module that calls it depends on `"com.platform.overlay >= 0.2"`, and an older runtime then fails its load instead of its first call.

### Returns {#o-here-returns}

A mark, a new table every time. It is opaque: its fields are the runtime's, and a module only hands it to `O:stillHere` of the same overlay.

### Errors {#o-here-errors}

None. It never answers an error value.

**Raises** only what the origin's resolution raises (see the top of this page).

## O\:stillHere(mark, opts?) {#o-stillhere}

**Signature:** `O:stillHere(mark: Mark, opts: { focus: boolean?, keys: boolean?, said: boolean?, spoken: boolean?, menus: boolean?, place: boolean? }?) -> true | (false, Error)`

Whether what a [mark](#o-here) noted still holds: `true`, or `false` and an error value. It asks, in the order of the Errors table, and answers the first check that no longer holds. The first three are always asked, unless `place = false`; the others only with their option:

- **`focus = true`** — the same focus, and the same control on it.
- **`keys = true`** — no key of the overlay's own since. **A key of the overlay's own** is one that reached it — Tab, Shift+Tab, Return, Space, an arrow it holds, the tab keys, a control's or a tab's hotkey — whatever it did, nothing included. A key that went to the plug-in is not one, nor is a calibration key.
- **`said = true`** — no announcement of the overlay's since. **An announcement** is the overlay saying a control as a whole — the focus arriving on it, the overlay's arrival, a control said again once a press settled (a toggle, a stepper, an OCR control's activation) — whether or not it was said in the end. What the overlay says about an action — `"<label>, activated"`, a tab switched with Left or Right — is not one.
- **`spoken = true`** — no announcement of the overlay's *said* since: handed to speech, at once for a control with nothing to read, and for an OCR control when its read answers. So from when an announcement begins until then, `said` has moved and `spoken` has not, and an announcement that is dropped in the end is never said. Something a module says after the overlay's own sentence, queued behind it, waits for `spoken`; `said` would let it go ahead of a sentence still waiting for its read. A graphic control's value, said after its name, is part of the same announcement and not counted again.
- **`menus = true`** — no menu open over the overlay now ([`O:menuOpen()`](#o-menuopen)), and none opened over it since, not even one closed again. **A menu** is one the overlay's [menu tests](#o-menutests) see; without menu tests none ever opens.
- **`place = false`** leaves the first three out, for an action that stays right while another window of the same application is in front and the overlay may not be active (see macOS below). The code then checks its target itself. The other options are asked as given.

No timer and no [epoch](timer.md#host-epoch) decides anything here: each count changes only when its event happens. What a mark does not see: typing that went to the plug-in, the plug-in redrawing, and a window moved or resized, which is the same window. Coordinates worked out before the wait are worked out again after it ([`O:toScreen`](#o-toscreen)).

It runs where it is called, on the main thread, and costs a few comparisons and, unless `place = false`, one origin resolution. It touches no screen. It came with version 0.2 of the runtime, and `spoken` with 0.4.1: a module that calls it depends on `"com.platform.overlay >= 0.2"` (`>= 0.4.1` for `spoken`).

```luau
-- Return on "Bank" reads the bank's name and clicks it, but only while the overlay is where the
-- key was pressed, no other key of its own came since, and the window has not moved — the words
-- are where the picture saw them. Needs "ocr", "input" and "log" in the manifest.
local BANK = { 300, 20, 400, 36 }
local function samePlace(a, b) return a and b and a[1] == b[1] and a[2] == b[2] end
ov:addCustomButton({
  label = "Bank",
  onActivate = function(o)
    local mark = o:here()
    local r = o:toScreenRect(BANK, { whole = true })
    if not r then return end
    host.ocr.recognize(r, function(reading)
      local still, err = o:stillHere(mark, { keys = true })
      if not still then
        -- Out of the front: the user is elsewhere, and nothing is said. Moved on: logged.
        if err.code == O.errors.NOT_IN_FRONT then return end
        return host.log.info("Bank: not clicking, " .. err)
      end
      if not samePlace(o:toScreenRect(BANK, { whole = true }), r) then return end -- it moved
      if reading.status ~= "text" then return end
      local w = reading.words[1]
      host.input.click(w.x + 2, w.y + math.floor(w.h / 2))
    end)
  end,
})
```

### Returns {#o-stillhere-returns}

- `true` — every check asked still holds.
- `false, err` — the first check that no longer holds; `err.code` is `not_in_front` or `moved_on`, and its message the runtime's `[read]` words for it.

### Errors {#o-stillhere-errors}

None of these is raised. Each comes as an error value where "arrives" says. They are listed in the order they are asked.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not active now. Asked unless `place = false`. | `the overlay is no longer active` | both |
| `moved_on` | return | Another stay: it left the front since, even if it came back to the same window. Asked unless `place = false`. | `the overlay left the front and came back since` | both |
| `moved_on` | return | Another window: the origin's handle is not the mark's. Asked unless `place = false`. | `the overlay is on another window now` | both |
| `moved_on` | return | With `focus = true`: the focus moved, or another control is at the focused index. | `the focus has moved on` | both |
| `moved_on` | return | With `keys = true`: a key of the overlay's own reached it since. | `a later key reached the overlay` | both |
| `moved_on` | return | With `said = true`: an announcement of the overlay's began since. | `a later announcement was made` | both |
| `moved_on` | return | With `spoken = true`: an announcement of the overlay's was said since. | `a later announcement was spoken` | both |
| `moved_on` | return | With `menus = true`: a menu counts as open over the overlay now. | `a menu is open over the overlay` | both |
| `moved_on` | return | With `menus = true`: a menu opened over the overlay since, open or closed again now. | `a menu opened over the overlay since` | both |

**Raises** for a mistake in the call, naming the caller's line: a `mark` that is not one from `O:here()` (`overlay '<label>': stillHere takes a mark from O:here(), got <type>`), a mark of another overlay (`overlay '<label>': stillHere was handed a mark of overlay '<other>'`), an `opts` that is not a table (`overlay '<label>': stillHere's options are a table, got <type>`), and an option it does not have (`overlay '<label>': stillHere has no option '<name>' (it has focus, keys, said, spoken, menus and place)`) — a misspelled option would otherwise check nothing at all. It never raises otherwise, but for the origin's resolution (see the top of this page).

### Windows {#o-stillhere-windows}

The window is the origin's window handle: the plug-in's own child window in a DAW, its window standalone.

### macOS {#o-stillhere-macos}

The window is the origin's `id`: on a DAW's plug-in panel, the plug-in window's. A plug-in's menu can be a window of its own that comes to the front, and the overlay may not be active while it is; an action that belongs to the press before it — a click into that menu — asks `{ keys = true, place = false }` and checks its target itself, for instance that the same application is in front ([`host.window.foreground()`](window.md#host-window-foreground)).

## O\:addOCRButton(opts) {#o-addocrbutton}

**Signature:** `O:addOCRButton(opts) -> Control`, where `opts: { label: string, region: {number, number, number, number} | ((overlay) -> {number, number, number, number}?), text: ((overlay) -> string?)?, value: ((overlay, text: string) -> string?)?, fallback: string?, hotkey: string?, hotkeyKeepsFocus: boolean?, readOnly: boolean?, opensMenu: boolean?, when: ((overlay) -> boolean)? }`

Appends a button whose label/value is read live by OCR over a region; activating re-reads it then clicks the region centre. `region` is `{x1, y1, x2, y2}` origin-relative; `readOnly` re-reads on activation and never clicks; `opensMenu` says the click puts a menu on screen (see [`O.menuTests`](#o-menutests)).

It is announced as `"<label>[, <text>], button[, <key>], <value>"` — the key its `hotkey`, said in the platform's words when focus arrives on the control (not when it is activated), and the value what the region reads: its text; `fallback` when nothing is drawn there or the recogniser read nothing, and `"no text"` when there is no `fallback`; `"cannot be read now"` when the read failed — and a `readOnly` one without the word "button": a control that cannot be pressed does not describe itself as one.

**Read off the event loop.** The region is read with [`host.ocr.recognize`](ocr.md#host-ocr-recognize) and a callback at every announcement — arriving on the control, and activating it — in the user's language, and the announcement is said when the answer comes: one sentence, as late as the recognition makes it, while the keys, the timers and the speech of the whole application go on. A value read as several lines is said as one, the lines joined with a space.

**Activating** asks for the read, and clicks the region centre once the answer has come and the sentence has been said — the order the two had while the read held the event loop. So the value said is the one from before the click, however long the picture takes; the sentence does not cut off what the click opens (a screen reader's announcement of a dialog or a field); and an `opensMenu` press counts at its click (see [`O.menuTests`](#o-menutests)), so the menu it opens comes after its value. The click is made only while the overlay is still where the key was pressed — active, on the same window, not having left the front since, the control still in its place — and no other key of the overlay's own has arrived since: a user who pressed on has moved on, and a click landing after that, a menu opening over the control they went to, is the mistake pressing again cannot undo. A click not made writes `[read] '<label>': not clicking — <why>`. An OCR edit field (`addOCREdit`, Komplete Kontrol's "Save as") puts the caret into its field on focus by a click made the same way: after its sentence, and only while the focus is still on it and no key of the overlay's own has come since (`[read] '<label>': the focus moved on before the answer; not clicking into the field`).

The read is made in the VM of the module that owns the overlay, so it is that module's and counts toward its 16 reads waiting or running at once. Its `key` is one per overlay — `"com.platform.overlay focus 1"`, `"… 2"` and so on, one per overlay in the order they first read — so a later announcement's read replaces one that has not started recognising yet; a module does not use keys that begin `"com.platform.overlay "` for its own reads.

The sentence is said only while it is still about what is in front of the user: the answer is not superseded (`"stale"`, `newer`); the overlay is still active, on the same window, and has not left the front since; the focus is where it was; no key of the overlay's own has arrived since and no other announcement of the overlay has begun; and no menu counts as open over it ([`O:menuOpen()`](#o-menuopen)), nor has one opened since the read was asked — a menu the plug-in put up itself, after a choice in which the value read is the one from before it. Otherwise nothing is said for it. No timer decides any of this: an answer is spoken when it comes, or dropped. A key that goes to the plug-in itself never reaches the overlay and does not drop an answer. A read refused at the call is said at once, with the static label and `"cannot be read now"` as the value. A region with nothing of it on screen — corners turned around — is not read and is said as an empty read is. The same rule keeps a late announcement from taking the place of a key's: a toggle's announcement once its state has settled after a press ([`O:addHotspotToggle`](#o-addhotspottoggle), [`O:addGraphicalToggle`](#o-addgraphicaltoggle)) and the overlay's announcement on arrival are not made when a key of the overlay's own came after the press or the arrival, which has said where the user is (`[watch] <overlay>: '<label>' not announced — a later key reached the overlay`, `[activate] '<overlay>': a key reached it before its arrival was announced; not announcing the arrival`).

**The log.** Each answer writes one line: `[read] '<label>' = "<text>"[, said as "<value>"] (region x1,y1 wxh, <ms> ms, <n> words[, <k> placed by estimate], <language>)` — the region as authored; `<ms>` from the announcement to the answer; `<n>` the words the recogniser placed, and `<k> placed by estimate` the words whose place the host estimated, which are the second recogniser's answer — on Windows, and on a Mac where the download carries it — the way a lone digit is read; and the language the read was made in, left out for a reading never recognised — then `: nothing drawn there`, `: the recogniser read nothing`, `: the read failed (<error>)` or `: not recognised` for a reading that is not text; then `; spoken`, or `; not spoken:` and why — `a later announcement's read replaced it before it was recognised`, `a later announcement asked for another read`, `the overlay is no longer active`, `the overlay left the front and came back since`, `the overlay is on another window now`, `the focus has moved on`, `a later key reached the overlay`, `a later announcement was made`, `a menu is open over the overlay` or `a menu opened over the overlay since`. A read refused at the call writes `[read] '<label>' could not be read: <error>`, and a region empty on screen `[read] '<label>' not read: its region is empty on screen (x1,y1 to x2,y2)`.

- `text(overlay)` is announced between the label and the type word: what a read-out's value *is*, where that changes (Melodyne's inspector box holds whichever parameter the tool owns).
- `value(overlay, text)` says what the text read means, where the module knows how the recogniser misreads the plug-in's text: VPS Avenger says its blank preset, "Init Preset", for what the recogniser makes of it at 50 % ("hit Presst"). It is asked with the text as read, its lines joined with a space, once the reading is text, and what it answers is said in the text's place; `nil`, or anything but a string with something in it, says the text as read. It runs in the handler of the read's answer and must answer at once: it reads nothing. One that raises is logged once per control (`[overlay] '<label>': its value raised: …`), and the text is said as read. The `[read]` line keeps the text as read and adds `, said as "<value>"` where the two differ. It came with version 0.5.6 of the runtime: a module that gives it depends on `"com.platform.overlay >= 0.5.6"`.
- `fallback` says what an empty region means in the plug-in's terms (`"no value"`), instead of "no text", which describes the recogniser.
- `region` may be a **function** of the overlay answering the corners, for a region that moves with something the module knows. It is asked at every read. `nil` from it — or no factor from the overlay's [scale](#o-scale) — reads nothing: the control announces `"cannot be read now"` in place of a value, logs `[read] '<label>' not read: <why>`, and a click it would have made is not made. A region in a scaled overlay is scaled corner by corner, and its `[read]` line adds where it was read: `… at x,y wxh on screen`.

```luau
-- u-he draws its whole interface itself: the preset name exists nowhere but on screen.
-- Focusing reads it; pressing opens u-he's own preset menu, and `opensMenu` makes the
-- overlay's menu tests look for it at once (see O.menuTests).
ov:addOCRButton({ label = "Preset menu", region = { 480, 20, 720, 48 },
  hotkey = "Alt+M", opensMenu = true })

-- `readOnly`: focusing re-reads the articulation, but a press would drop the user into a
-- list no screen reader can follow -- so this one announces and never clicks.
ov:addOCRButton({ label = "Articulation", region = { -115, 114, 165, 152 },
  readOnly = true, hotkey = "Alt+B" })

-- A preset name, the first control, so it is what the overlay says on arrival. In a scaled
-- overlay the region follows the zoom; `fallback` says an empty field in the plug-in's terms,
-- and `value` a reading the module knows for a misread of the blank preset.
ov:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true,
  fallback = "no preset name",
  value = function(_, text)
    if string.lower(text) == "hit presst" then return "Init Preset" end
    return nil
  end })

-- A region that depends on what the module knows: the value box of whichever slot is shown.
ov:addOCRButton({ label = "Slot value", readOnly = true,
  region = function(o)
    local slot = o.state.slot
    return slot and { 40 + 60 * slot, 300, 90 + 60 * slot, 316 } or nil   -- nil: "cannot be read now"
  end })
```

### Returns {#o-addocrbutton-returns}

The control table, appended to `o.controls`: `{ kind = "ocr", label, region, hotkey, hotkeyKeepsFocus, readOnly, opensMenu, when, text, value, fallback }`, `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addocrbutton-errors}

None. It never answers an error value. A read that failed or was refused is said as `"cannot be read now"`, and its error's message is in the `[read]` line (above); a click not made is logged.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil` (Luau's own `attempt to index nil with 'label'`). Nothing else is checked at the call: a missing `region`, or one that is neither four numbers nor a function, is reported when the overlay binds.

### Windows {#o-addocrbutton-windows}

The read is the one [`host.ocr.recognize`](ocr.md#host-ocr-recognize-windows) makes for any module: through the module's capture source, then `Windows.Media.Ocr` with the second recogniser beside it for a lone digit — the recognisers, the small-text treatment and the language (the first language in the user's Windows list that OCR has installed) that the read on the event loop used here before. One difference in the language: the user's list is read when the application starts, so a change to it in Windows Settings is used from the next start, where the read on the event loop asked Windows at every read. A read of one of sforzando's read-outs took 22–42 ms (measured on the event loop, a debug build on a six-core desktop), so the sentence comes about as soon after the key as it did — later by about one turn of the loop, which is a timer message of the lowest priority every 15–16 ms, and by any recognition of another read already running, which is not interrupted; an activation's click, after the sentence, by as much. A region's corners are cut toward zero, as the read on the event loop read them, so an overlay with a fractional [frame](#o-frame) reads the pixels it always did.

### macOS {#o-addocrbutton-macos}

The pipeline [`host.ocr.recognize`](ocr.md#host-ocr-recognize-macos) uses on a Mac: Vision's retry ladder, and the second recogniser beside it where the download carries it, with an Intel Mac's checked first pass. The language is the user's — the first of their preferred languages that Vision reads, then English — where the read on the event loop without `lang` handed Vision none, and so Vision's own default; for a user whose first language is not English, a value or label is now read with that language. How long the sentence takes is the recognition's, measured with the read on the event loop and Vision alone, before the second recogniser answered on a Mac: 25–56 ms for sforzando's read-outs on a Mac mini M1, 115–164 ms after more than 5 s without a read, and 143–786 ms on an Intel MacBook Air (2020) — see [`host.ocr.recognize`](ocr.md#host-ocr-recognize-macos). While it runs, the keyboard's event tap, the timers and every other key go on; on that Air each such read used to hold them up for as long.

## O\:addOCREdit(opts) {#o-addocredit}

**Signature:** `O:addOCREdit(opts) -> Control`, where `opts: { label: string, region: {number, number, number, number} | ((overlay) -> {…}?), hotkey: string?, hotkeyKeepsFocus: boolean?, when: ((overlay) -> boolean)? }`

Appends an **edit field read by OCR** — ReaHotkey's `OCREdit`, for a plug-in's own text field that publishes nothing: Komplete Kontrol's "Save as". It is announced `"<label>, edit, <value>"`, the value read off `region` as an [OCR button](#o-addocrbutton)'s is, off the event loop, with the same endings (`"no text"`, `"cannot be read now"`). Once its sentence is said, the region's centre is clicked, so the caret lands in the plug-in's field and the user can type — only while the focus is still on the control and no key of the overlay's own has come since (`[read] '<label>': the focus moved on before the answer; not clicking into the field`). Return reads it again and clicks the centre again, which is the way back into the field after tabbing away; Space is not taken while it is focused, so a space can be typed. It takes no `text`, `value`, `fallback`, `readOnly` or `opensMenu`.

```luau
-- A save dialog drawn by the plug-in: the name field, then the button that saves (the numbers
-- are illustrative).
ov:addOCREdit({ label = "Preset name", region = { 120, 60, 420, 84 } })
ov:addHotspotButton({ label = "Save", at = { 380, 120 } })
```

### Returns {#o-addocredit-returns}

The control table, appended to `o.controls`: `{ kind = "ocredit", label, region, hotkey, hotkeyKeepsFocus, when }`, `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addocredit-errors}

None. It never answers an error value. A read that failed is said as `"cannot be read now"`, and a click not made is logged.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil`. A missing `region` is reported when the overlay binds.

## O\:addGraphicalToggle(opts) {#o-addgraphicaltoggle}

**Signature:** `O:addGraphicalToggle(opts) -> Control`, where `opts: { label: string, region: {number, number, number, number} | ((overlay) -> {…}?), onImage: string | { string } | nil, offImage: string | { string } | nil, hotkey: string?, hotkeyKeepsFocus: boolean?, hint: { on: string?, off: string? }?, reveal: { at: {number, number}, closedColor: {number, number, number}, settle: number? }?, tolerance: number?, scales: { number }?, when: ((overlay) -> boolean)? }`

Appends a toggle whose on/off state is read by image-matching its region against an "on" and "off" template; activating clicks the region centre and re-reads the new state after ~150 ms, unless a key of the overlay's own came meanwhile. `region` is `{x1, y1, x2, y2}` origin-relative; `onImage`/`offImage` are template image paths, each one path or a list of them for a state drawn more than one way. `hint` is what is said after the state when a press turned it to that state; `tolerance` and `scales` are the search's.

Spoken state is `"on"` / `"off"` (omitted when neither template matches).

A `reveal` probe, `{ at = {x, y}, closedColor = {r, g, b}, settle = ms }` — a point whose colour says the panel the toggle lives in is collapsed, and which opens it — is read before the toggle's state is read and before its click. A pixel there within 30 of `closedColor` (the three channels' differences added up) is closed; anything else, or a pixel that cannot be read, lets the state read or the click go ahead at once. Whatever the probe reads, what follows it is made only while the overlay is still where the probe was read — active, in the same stay in front, on the same window, as [`O:stillHere`](#o-stillhere) asks without options. Closed, the probe is clicked — like a hotspot's click, not outside the origin's window or over another window's pixel — and the state read or the click follows `settle` ms later (250 by default), again only while the overlay is still where the probe was read. Where it is not, nothing more is read or clicked for that press or announcement, and the log says `[overlay] <overlay>: '<label>' not going on after reading its panel's probe — <why>` (`after opening its panel`, once `settle` is over), in `stillHere`'s words.

```luau
-- Soundiron's shared FX rack. The reverb bypass is a bar with a legend rather than a lit
-- dot, so its state is matched against two captured templates instead of one pixel.
-- Paths must be ABSOLUTE -- `host.path` resolves them under this module's own root.
ov:addGraphicalToggle({
  label = "Reverb",
  region = { -91, 108, -71, 148 },   -- anchored to the library wordmark, hence negative x
  onImage = host.path("images/FxRack/ReverbOn.png"),
  offImage = host.path("images/FxRack/ReverbOff.png"),
  hotkey = "Alt+B",
})
```

### Returns {#o-addgraphicaltoggle-returns}

The control table, appended to `o.controls`: `{ kind = "gtoggle", label, region, onImage, offImage, hotkey, hotkeyKeepsFocus, hint, reveal, tolerance, scales, when }`, `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addgraphicaltoggle-errors}

None. It never answers an error value. A search that could not be made — the capture failed, or too many were waiting — says no state, and the log says why: `[gtoggle] '<label>': could not look (<message>), so no value is said`.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil`. Nothing else is checked at the call: no `region`, neither image, a path that is not absolute, a half-declared `reveal` and a `hint` keyed by anything but `on` and `off` are reported when the overlay binds.

## O\:addGraphicalButton(opts) {#o-addgraphicalbutton}

**Signature:** `O:addGraphicalButton(opts) -> Control`, where `opts: { label: string, image: string | { string }, region: {number, number, number, number} | ((overlay) -> {…}?) | nil, hotkey: string?, hotkeyKeepsFocus: boolean?, opensMenu: boolean?, dragBy: {number, number} | ((overlay, hit) -> (number?, number?, string?))?, clickOffset: {number, number}?, state: ((overlay, hit) -> string?)?, unavailable: ((overlay) -> string?)?, rawOrigin: boolean?, tolerance: number?, scales: { number }?, when: ((overlay) -> boolean)? }`

Appends a button that is **found rather than placed** — ReaHotkey's `GraphicalButton`: `image` is searched for inside `region` (the whole origin when there is no `region`), and activating clicks the centre of where it was found, never the middle of the region. For a control with no coordinate to write down: Kontakt's "Performance | FX Rack" tabs sit at the foot of each instrument's own interface, at a height that depends on the instrument. Every path is absolute ([`host.path`](path.md#host-path)), and may be a list of variants.

- **Announced** `"<label>, button"`, and after it, from a search with a callback: nothing when it is found, `"not found"` when it is not, or what `state(overlay, hit)` answers for the hit (worked out from where it was found, a pixel beside it, say). `unavailable(overlay)`, when it answers a string, is said in place of a search, for a button that is there but cannot act now. A search that could not be made says nothing more, and is logged.
- **Activating** says `"<label>, activated"` at the press, then searches, with a callback; the answer is used only while the overlay is still active on the same window. Not found, it says `"<label> not found"`; a search that could not be made, `"<label> could not be looked for, try again"`. Found, it clicks the hit's centre — or `clickOffset`, an authored distance from the hit's corner (scaled in a scaled overlay) — after the check every click of the overlay gets.
- **`dragBy`** drags from that point instead of clicking, for a handle: `{dx, dy}` authored (and scaled), or a function of the overlay and the hit answering `dx, dy` in screen pixels, or nothing and a sentence to say instead (`"<label> cannot go further"` when it gives none). The drag's end is not checked: a resize drag is meant to end outside the window it started in.
- **`rawOrigin`** places `region` in the origin's own pixels, as on a hotspot. `tolerance` and `scales` are the search's.

A search that misses is logged with what it searched, `[gbutton] '<label>' not found: searched (x1,y1)-(x2,y2) for <n> template(s) at <m> scale(s)`, and in a calibrating run the picture it searched is written once per control, `calibration/miss-<label>.png`.

**Cost.** One image search per announcement and per press, each a capture and a match off the event loop.

```luau
-- Kontakt's FX Rack tab, wherever the instrument's interface puts it.
ov:addGraphicalButton({
  label = "FX Rack",
  image = host.path("images/Rack/FxRackTab.png"),
  rawOrigin = true,
})
```

### Returns {#o-addgraphicalbutton-returns}

The control table, appended to `o.controls`: `{ kind = "gbutton", label, region, image, hotkey, hotkeyKeepsFocus, opensMenu, dragBy, clickOffset, state, unavailable, rawOrigin, tolerance, scales, when }`, `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addgraphicalbutton-errors}

None. It never answers an error value. Not found and could not look are said, as above.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil`. Nothing else is checked at the call: no `image`, a path that is not absolute and a `dragBy` that is neither `{dx, dy}` nor a function are reported when the overlay binds. A `dragBy` function and `unavailable` are called unguarded: what they raise ends the handler they run in, and the host logs it; `state` is guarded.

## O\:addHotspotToggle(opts) {#o-addhotspottoggle}

Appends a toggle whose on/off state is read from a **single pixel** at its click point (ReaHotkey's `HotspotToggleButton`) — cheap (one screen touch), where a region scan would cost ~16 ms *per pixel*. The pixel is compared to an on/off reference colour and the **nearest** wins. Activating clicks the point (toggling it) and then [waits for the state to actually change](#o-watch) before announcing it, giving up after ~900 ms — and does not announce it when a key of the overlay's own came meanwhile (see [`O:addOCRButton`](#o-addocrbutton)). A control that redraws in thirty milliseconds is announced in thirty; one that takes half a second is announced correctly instead of early; one that does not change at all — the already-chosen member of a radio group — is announced at the deadline, which is the truth about it. `opts: { label: string, at: {number, number} | ((overlay) -> {number, number}?), onColor: {number, number, number} | { {number, number, number} }, offColor: (likewise), text: ((overlay) -> string?)?, ocrLabel: {number, number, number, number}?, hint: { on: string?, off: string? }?, hotkey: string?, hotkeyKeepsFocus: boolean?, rawOrigin: boolean?, fromRight: boolean?, when: ((overlay) -> boolean)? }` — `at` is `{x, y}` origin-relative; `onColor`/`offColor` are `{r, g, b}`.

`at` may also be a **function** of the overlay, for a control whose position depends on what is on screen right now; returning `nil` yields no point and the click is skipped rather than guessed. `onColor`/`offColor` may each be a **list** of `{r, g, b}`, because one state can legitimately look several ways. A reading that resembles neither closely enough — measured against how far apart the references are — reports **no state at all** and logs why, rather than announcing whichever is nearer. Announcing the opposite of the truth is the worst thing this system can do to somebody who cannot check it against the screen; saying nothing is merely unhelpful.

`text` is honoured here as everywhere, and is how a toggle can report something the state alone does not say — ON:EAR's panel switches use it for "unavailable", because announcing "off" for a switch that will not respond is the same failure as announcing the wrong state.

Like `addHotspotButton`, the click is refused if another window is drawn over the point (see [`host.window.ownsPoint`](window#host-window-ownspoint)).

The pixel before the click — the one the log line after the press compares with the pixel after it — is read where the key was pressed, and the click is made only while the overlay is still there: active, in the same stay in front, on the same window ([`O:stillHere`](#o-stillhere) without options). Otherwise nothing is clicked or announced, and the log says `[overlay] <overlay>: '<label>' not clicking after reading its pixel — <why>`, in `stillHere`'s words. The point is asked again after the read, before the click: another window drawn over it meanwhile refuses the click, as above.

Spoken state is `"on"` / `"off"` (omitted when the pixel can't be read). Prefer this over `addGraphicalToggle` when the control has a distinct lit/unlit colour (an indicator LED, a lit ⏻ icon) — it needs no template images and is a fraction of the cost.

```luau
ov:addHotspotToggle({
  label = "Spot 1 Mic",
  at = { -138, 366 },
  onColor = { 180, 165, 230 }, -- lit (accent)
  offColor = { 78, 79, 85 },   -- unlit (dim grey)
})
```

### Returns {#o-addhotspottoggle-returns}

The control table, appended to `o.controls`: `{ kind = "hotspottoggle", label, ocrLabel, text, at, onColor, offColor, hotkey, hotkeyKeepsFocus, hint, rawOrigin, fromRight, when }`, `hotkeyKeepsFocus` made a boolean.

### Errors {#o-addhotspottoggle-errors}

None. It never answers an error value. A pixel that cannot be read says no state, and a click refused or not made is said or logged, as above.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil`. Nothing else is checked at the call: no `at`, a missing colour and a colour that is not `{r, g, b}` are reported when the overlay binds. An `at` function is called unguarded at the press, as a hotspot's is.

## O\:addTabControl(opts) {#o-addtabcontrol}

**Signature:** `O:addTabControl(opts) -> { control: Control, onTab: (self, n: number) -> (() -> boolean) }`, where `opts: { label: string?, tabs: { { label: string, at: {number, number}?, hotkey: string?, hotkeyLabel: string? } }, typeLabel: string?, current: ((overlay) -> number?)?, verticalStep: ((overlay, tab: number, dir: number) -> string?)?, verticalName: ((overlay, tab: number) -> string?)?, hotkeyKeepsFocus: boolean?, onSelect: ((overlay, tab: number) -> ())?, when: ((overlay) -> boolean)? }`

Appends a **tab control** — ReaHotkey's `TabControl`: one focus stop that stands for a row of tabs, of which only the **current** tab's controls are in the ring. It is announced `"<tab> tab selected, tab control"` (`typeLabel` in place of "tab control"), and a tab's `hotkeyLabel` after it, in the platform's words when it is a key spec. Choosing a tab clicks its `at` — through the frame and the scale, so the plug-in's own tab follows — and says `"<tab> tab selected"`; a tab whose point cannot be placed now stays unchosen, the tab before it current, and says `"<tab> tab is not available now"`.

- **The keys.** While the tab control is focused, Left and Right choose the previous and next tab, wrapping ([`O:switchTab`](#o-switchtab)). From anywhere in the overlay, Ctrl+Tab and Ctrl+Shift+Tab choose the next and previous and put the focus on the tab control ([`O:cycleTab`](#o-cycletab)), and Ctrl+1 to Ctrl+9 choose tab 1 to 9 ([`O:gotoTab`](#o-gototab)) — on a Mac Control-Tab, Control-Shift-Tab and Command-1 to 9 ([`O:attach`](#o-attach-macos)). A tab's own `hotkey` chooses it from anywhere, once its modifiers are let go. Ctrl+1 to Ctrl+9 and a tab's hotkey move the focus to the tab control, unless it has `hotkeyKeepsFocus`: then the tab is chosen and said, and the focus stays where it was. These keys are taken only by an overlay that has a tab control, and act on its first one.
- **The tabs' controls.** The handle's [`onTab(n)`](#tabs-ontab) answers a `when` that is true while tab `n` is current: give it to each control of that tab, added after this call, and they are in the ring and hold their hotkeys only then. Combined with a control's own condition through [`O:group`](#o-group).
- **`current(overlay)`** — where the plug-in is, asked each time the tab control is announced: an index of a tab makes it the current one; anything else leaves the one the control last chose. For a plug-in whose own keys change the tab.
- **`verticalStep(overlay, tab, dir)`** — a second axis: Up (`-1`) and Down (`+1`), taken while the tab control is focused, step something within the current tab, and what it answers is said; `nil` says nothing ([`O:stepTabVertical`](#o-steptabvertical)). **`verticalName(overlay, tab)`**, when it answers a string, is said in place of the tab's label: what the tab is called now. Melodyne's tool bar uses both for the tools stacked behind each icon.
- **`onSelect(overlay, tab)`** — after a tab was chosen and said, guarded: for a module that must not talk over that announcement.

On a new window the tab control starts on tab 1; on the same window again it keeps its tab ([`O:attach`](#o-attach)). `current` and `verticalName` run in the handler of the key or event that announces the control, where they may read ([Where a hook runs](#where-a-hook-runs)); one that raises gives no value.

```luau
-- A plug-in with two pages of controls, chosen by tabs it draws at the top.
local tabs = ov:addTabControl({
  label = "Pages",
  tabs = { { label = "Main", at = { 40, 12 } }, { label = "Effects", at = { 110, 12 } } },
})
ov:addHotspotButton({ label = "Power", at = { 60, 80 }, when = tabs:onTab(1) })
ov:addHotspotButton({ label = "Reverb", at = { 60, 80 }, when = tabs:onTab(2) })
```

### Returns {#o-addtabcontrol-returns}

A handle, not the control: `{ control = <the control table>, onTab = <function> }`. The control table, appended to `o.controls`, is `{ kind = "tabcontrol", label, tabs, typeLabel, current, verticalStep, verticalName, hotkeyKeepsFocus, onSelect, when }`, with `label` `""` and `tabs` `{}` when not given. `handle:onTab(n)` answers a new function of no arguments, `true` while tab `n` is current and `false` otherwise.

### Errors {#o-addtabcontrol-errors}

None. It never answers an error value. A tab's hotkey the host refuses, or claims and cannot hold now, is logged, `[keys] '<overlay>': tab '<tab>' hotkey <spec> is not registered (<reason>)` or `… is not held now: <message>`.

**Raises** for a control added after the overlay is bound (`overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`) and for an `opts` that is `nil`. Nothing else is checked at the call. A `verticalStep` that raises says nothing; `onSelect` is guarded and logged, `[overlay] <overlay>: tab onSelect failed: <message>`.

## tabs\:onTab(n) {#tabs-ontab}

**Signature:** `tabs:onTab(n: number) -> (() -> boolean)`, where `tabs` is the handle [`O:addTabControl`](#o-addtabcontrol) returns

A `when` for the controls of tab `n` of that tab control: a function of no arguments that answers `true` while tab `n` is the current one. Give it to each control of the tab, added after the tab control, and they are in the ring and hold their hotkeys only while the tab is current; with a control's own condition, through [`O:group`](#o-group). The current tab is the tab control's own: the one it last chose, or the one its `current` named when it was last announced. Tab controls nest: a control of a tab of a nested tab control is gated on the outer tab's `onTab` and on its own, one `O:group` inside the other.

Asked wherever its control's `when` is — on every scan, in the runtime's coroutine ([Where a hook runs](#where-a-hook-runs)) — it reads one number and calls nothing of the host's.

```luau
local tabs = ov:addTabControl({
  label = "Pages",
  tabs = { { label = "Main", at = { 40, 12 } }, { label = "Effects", at = { 110, 12 } } },
})
ov:group(tabs:onTab(2), function(fx)   -- in the ring only while Effects is the current tab
  fx:addHotspotButton({ label = "Reverb", at = { 60, 80 } })
  fx:addHotspotButton({ label = "Delay", at = { 60, 110 } })
end)
```

### Returns {#tabs-ontab-returns}

A new function of no arguments, which answers `true` while tab `n` is current and `false` otherwise, never `nil`. Each call makes a new one. An `n` that is no tab's index, and a call with a dot — `tabs.onTab(2)`, where `n` is then `nil` — answer one that is always `false`.

### Errors {#tabs-ontab-errors}

None. It never answers an error value.

It never raises: `n` is not checked, and the function it answers only compares.

## O\:switchTab(dir) {#o-switchtab}

**Signature:** `O:switchTab(dir: number) -> ()`

Chooses the tab `dir` steps from the current one, wrapping — `-1` the previous, `+1` the next — on the **focused** [tab control](#o-addtabcontrol), and says it, as Left and Right do there. Nothing happens unless the focused control is a tab control with tabs.

```luau
host.hotkey.register("Alt+PageDown", function() if ov.active then ov:switchTab(1) end end)
```

### Returns {#o-switchtab-returns}

Nothing.

### Errors {#o-switchtab-errors}

None. It never answers an error value. A tab that cannot be placed now is said, `"<tab> tab is not available now"`, and the tab before it stays.

**Raises** for a `dir` that is not a number, when the focused control is a tab control (Luau's own arithmetic error).

## O\:cycleTab(dir) {#o-cycletab}

**Signature:** `O:cycleTab(dir: number) -> ()`

Chooses the tab `dir` steps from the current one, wrapping, on the overlay's **first** [tab control](#o-addtabcontrol), from anywhere in the overlay, and moves the focus onto the tab control — what Ctrl+Tab and Ctrl+Shift+Tab do. Nothing happens on an overlay without a tab control, or one with no tabs.

```luau
host.hotkey.register("Ctrl+Alt+T", function() if ov.active then ov:cycleTab(1) end end)
```

### Returns {#o-cycletab-returns}

Nothing.

### Errors {#o-cycletab-errors}

None. It never answers an error value.

**Raises** for a `dir` that is not a number, on an overlay with a tab control (Luau's own arithmetic error).

## O\:gotoTab(n, keepFocus?) {#o-gototab}

**Signature:** `O:gotoTab(n: number, keepFocus: boolean?) -> ()`

Chooses tab `n` of the overlay's **first** [tab control](#o-addtabcontrol) and says it — what Ctrl+`n` and a tab's own hotkey do. The focus moves onto the tab control unless `keepFocus` is true or the tab control has `hotkeyKeepsFocus`; the tab is chosen and said either way. Nothing happens for an `n` outside `1` to the number of tabs, or on an overlay without a tab control.

```luau
-- A key of the module's own for the second tab, leaving the focus where it is.
host.hotkey.register("Ctrl+Alt+2", function() if ov.active then ov:gotoTab(2, true) end end)
```

### Returns {#o-gototab-returns}

Nothing.

### Errors {#o-gototab-errors}

None. It never answers an error value. A tab that cannot be placed now is said, and the tab before it stays.

**Raises** for an `n` that cannot be compared with numbers (Luau's own error), on an overlay with a tab control.

## O\:stepTabVertical(dir) {#o-steptabvertical}

**Signature:** `O:stepTabVertical(dir: number) -> ()`

One step along the second axis of the overlay's **first** [tab control](#o-addtabcontrol): its `verticalStep(overlay, current, dir)` is called, guarded, and a string it answers is said, interrupting — what Up and Down do while the tab control is focused. Nothing happens without a tab control or without `verticalStep`.

```luau
host.hotkey.register("Alt+Down", function() if ov.active then ov:stepTabVertical(1) end end)
```

### Returns {#o-steptabvertical-returns}

Nothing.

### Errors {#o-steptabvertical-errors}

None. It never answers an error value.

It never raises: `verticalStep` is guarded, and one that raises says nothing.

## O\:focusNext() {#o-focusnext}

**Signature:** `O:focusNext() -> ()`

Moves focus to the next control (wrapping) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. No-op when there are no controls. On a [pass-through](#o-addpassthrough) stop it steps through the plug-in's own controls instead, as Tab does there, and `focusPrev` back.

Tab is already bound to this by the runtime, so a module calls it only to give the ring a second, more natural key.

```luau
-- ON:EAR's five preset slots are a row, so Alt+Right should walk them too.
host.hotkey.register("Alt+Right", function()
  if ov.active then ov:focusNext() end   -- a global key: only steer an overlay that is up
end)
```

### Returns {#o-focusnext-returns}

Nothing. The control it reaches is said; with no control shown, nothing is.

### Errors {#o-focusnext-errors}

None. It never answers an error value.

It never raises for a hook that raises: every `when` and `text` it asks runs guarded. It raises only for a `when` that yields inside the scan for the next control, `a hook asked on every scan yielded; it must return at once` ([Where a hook runs](#where-a-hook-runs)).

## O\:focusPrev() {#o-focusprev}

**Signature:** `O:focusPrev() -> ()`

Moves focus to the previous control (wrapping) and speaks it. No-op when empty. On a [pass-through](#o-addpassthrough) stop whose walk has gone into the plug-in it steps one element back, as Shift+Tab does there — and on the element the walk went in on, back out onto the stop.

```luau
-- The mirror of the above, and worth having for the wrap: with nothing focused yet this
-- lands on the LAST control, because the end of the ring is where you look when you suspect
-- you missed something.
host.hotkey.register("Alt+Left", function()
  if ov.active then ov:focusPrev() end
end)
```

### Returns {#o-focusprev-returns}

Nothing. The control it reaches is said.

### Errors {#o-focusprev-errors}

None. It never answers an error value.

**Raises** only for a `when` that yields inside the scan, as `focusNext` does.

## O\:addPassThrough(opts) {#o-addpassthrough}

**Signature:** `O:addPassThrough(opts: { label: string?, when: ((overlay) -> boolean)?, readUnnamed: boolean? }?) -> Control`

Appends a stop that hands Tab to the plug-in's own focusable elements, for a plug-in window that does not move focus on Tab by itself. In Kontakt standalone it is the only way into Kontakt's native controls, and Kontakt's standalone overlay ends with it, as "Kontakt controls". Like every control it is added before the overlay binds; afterwards it raises.

- `label` — what the stop is called; `"Plugin controls"` when left out.
- `when` — as for every control: the stop is in the ring only while it returns exactly `true`.
- `readUnnamed` — read a stop that has no name off the screen, below. On unless it is `false`; `nil` and anything else leave it on.

[`O:enableUiaPassThrough(opts)`](#o-enableuiapassthrough) is the old spelling.

**Tab and Shift+Tab.** Arriving on the stop says `"<label>, Tab to step through them"`. Tab from there moves the plug-in's own keyboard focus one element on with [`host.element.focusStep`](./element.md#host-element-focusstep), and the screen reader announces the element from the focus event; each further Tab moves on one element, and Shift+Tab one back. Once every element has been visited, Tab goes on to the overlay's next control without touching the plug-in's focus; Shift+Tab on the element Tab went in on comes back out onto the stop; Shift+Tab on the stop itself goes back through the overlay's controls, never into the plug-in. Arriving on the stop again starts the walk afresh. How a walk is counted — in steps, against a ring that may change size as it is walked — is under [`host.element.focusStep`](./element.md#host-element-focusstep). While the stop has the focus, Space and Return are not the overlay's: they reach the plug-in's element, which is where a check box is ticked or a list opened.

**What is spoken.** A stop with a name is the screen reader's: the overlay says nothing about it. A stop whose name has no visible character — empty, or made only of white space, control characters and characters that show nothing, such as a no-break space, a zero-width space or a byte-order mark — is announced by the screen reader as its role and little more, so with `readUnnamed` the overlay reads what the plug-in drew there:

- The rectangle `focusStep` gives for the element (`bounds`), widened to whole numbers — the near edges down, the far edges up — and cut to the rectangle of the overlay's origin, the plug-in window (its `bounds`, see [`O:origin()`](#o-origin)): what lies beyond is another window's. Nothing is read when there is no rectangle, when it lies outside the window, or when what is left is under 2 across or down. It is not cut to a scroll view: an element scrolled out of its list's view is read where it lies (see [`host.element.focusStep`](./element.md#host-element-focusstep)).
- It is read with [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) and a callback, in the user's language. The picture is taken at the step, normally within one screen frame of it, so it shows what the plug-in has drawn by then. The read is made in the VM of the module that owns the overlay (the runtime is a `code_module`), so it is that module's: it counts toward the module's 16 reads waiting or running at once, and a `host.input.*` or `host.window.focus` call the module makes before its picture is taken does not wait for it and is noted in the log, as with any read ([host.input](./input.md)). Its `key` is one per overlay, so a later step's read replaces one that has not started recognising yet; the keys are `"com.platform.overlay passthrough 1"`, `"… 2"` and so on, one per overlay in the order they first read, and a module does not use keys that begin `"com.platform.overlay "` for its own reads.
- When the answer is text, rows with nothing but white space are skipped, and the first **two** of the rest, top to bottom, each trimmed and joined with `", "`, are spoken with `interrupt = false`: the line does not cut off what is being said. Whether it then waits for the screen reader's own announcement of the element or is said beside it is the speech path's — see the platform sections. Further rows are dropped. Nothing is spoken for a region that is blank, one the recogniser read nothing in or only white space, or a read that failed.
- It is spoken only while nothing the overlay can see has moved on: the answer is not superseded (`newer`); the overlay is still active, on the same window, and has not left the front since, not even to come back to that window; the stop still has the overlay's focus and the keyboard is still in the plug-in; and no further Tab or Shift+Tab has reached the stop since the step. There is no deadline: an answer is spoken whenever it comes, as long as all of that holds, and dropped when any of it does not.
- Two things the overlay does not see. The checks are made when the answer arrives, not when the line is heard: a line queued behind a long announcement is said when that ends, after a Tab that came in between, and nothing takes it back. And a key that goes to the plug-in's element itself — Space, Return, an arrow key — never reaches the overlay, so a reading taken before such a key can be spoken after it, and says what the element showed before the key.

**Cost.** One OCR read per unnamed stop landed on; none for a stop with a name, or with `readUnnamed = false`. On the event loop it costs queueing the read and, when the answer comes, the checks above. Those resolve the overlay's origin afresh, because the answer's arrival turns the [epoch](./timer.md#host-epoch) over: for a standalone overlay that is one [`host.window.active`](./window.md#host-window-active) query, which touches no screen; for an embedded one it is the full resolve of the plug-in's control — measured at about 11 ms for Kontakt's in a DAW — unless something else in the same epoch has resolved it already. The capture and the recognition run on the two OCR threads, and a Tab does not wait for them; what they cost is under each platform below. The step itself costs what `focusStep` costs.

**The log.** Every step writes a line — `[passthrough] Synth: Tab -> stop 5 of 12 '', 3 into the lap` — and, with `readUnnamed`, every unnamed stop one more, saying what became of it, with the element's control type (the `ctype` of [`host.element.focusStep`](./element.md#host-element-focusstep)) and the text quoted up to 80 bytes. The lines below are made up, in the real format:

```text
[passthrough] Synth: stop 5 of 12 (type 50000) has no name; read at 812,240 64x22 in 38 ms: "Init Patch", spoken
[passthrough] Synth: stop 6 of 12 (type 50033) has no name; read at 812,270 200x90 in 41 ms: "Filter, Cutoff" (2 of 3 rows), not spoken: a later step asked for another read
[passthrough] Synth: stop 7 of 12 (type 50000) has no name; read at 790,300 30x30 in 25 ms: nothing drawn there
[passthrough] Synth: stop 8 of 12 (type 50000) has no name and no rectangle; nothing to read
```

`read at` gives the region read, in screen coordinates, and `in` the time from the step to the answer; `(2 of N rows)` counts the rows that were not blank. After `read at …`, the other endings are `the recogniser read nothing`, `no text (only white space)`, `no text (<status>)` for a status this page does not list, `the read failed (<error>)` and `not recognised, a later step's read replaced it`; and `"…", not spoken:` followed by `a later step asked for another read`, `the overlay is no longer active`, `the overlay left the front and came back since`, `the overlay is on another window now`, `the focus has left the pass-through`, `the keyboard has left the plugin's controls` or `a later key moved on`. With nothing read, the line ends `its rectangle x,y wxh is outside the plugin's window`, `… is under 2 across or down`, or `… is under 2 across or down inside the plugin's window (x,y wxh)` when the window's edge is what left too little — each followed by `, nothing to read` — or `its rectangle x,y wxh could not be read: <error>` when the read was refused at the call.

```luau
-- Kontakt standalone's header, last stop: Kontakt's own controls, unnamed ones read aloud.
ov:addPassThrough({ label = "Kontakt controls" })

-- A plug-in whose unnamed elements are images with no text in them: reading them would only
-- ever say nothing, so leave them to the screen reader.
ov:addPassThrough({ label = "Synth controls", readUnnamed = false })
```

### Returns {#o-addpassthrough-returns}

The control table, appended to `o.controls`: `{ kind = "passthrough", label, when, readUnnamed }`, `label` `"Plugin controls"` when not given and `readUnnamed` a boolean.

### Errors {#o-addpassthrough-errors}

None. It never answers an error value. A step the plug-in did not take, and an unnamed stop that could not be read, are logged (above); nothing is handed to the module.

**Raises** for a control added after the overlay is bound: `overlay '<label>': adding a control must be called BEFORE binding it — the binding is what starts it`. `opts` may be `nil`.

### Windows {#o-addpassthrough-windows}

`bounds` is the element's UI Automation rectangle, read once, right after the focus has landed: a scroll the plug-in animates after that is not in it (see [`host.element.focusStep`](./element.md#host-element-focusstep)). The capture is a fixed ~17 ms screen frame through the standard path, and the recognition 4–6 ms for a small read-out, tens to hundreds of milliseconds for a large region (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)).

With **Speak through the screen reader** ticked in the Application settings tab, as it is by default, and a screen reader running — NVDA, say — the line goes to the screen reader, without cutting off what it is saying: when its announcement of the element has begun, the line waits behind it. When the answer comes first, which a small element's read can, what becomes of the line once the screen reader handles the focus event is the screen reader's decision. With the box unticked, or no screen reader running, the line goes to the plain voice — a second voice beside the screen reader, which can talk over its announcement (see [`host.speech.output`](./speech.md#host-speech-output)).

### macOS {#o-addpassthrough-macos}

`bounds` is the element's Accessibility frame as the walk read it, before the focus moved. The recognition is Vision's, with its retry ladder abandoned after 250 ms (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)).

The line goes to the platform's own voice unless **Speak through VoiceOver** is ticked in the Application settings tab (see [`host.speech.output`](./speech.md#host-speech-output)): through the platform's own voice it is a second voice beside VoiceOver, and can overlap VoiceOver's announcement of the element rather than follow it; through VoiceOver, whether it waits behind that announcement is VoiceOver's decision.

## O\:enableUiaPassThrough(opts?) {#o-enableuiapassthrough}

**Signature:** `O:enableUiaPassThrough(opts: { label: string?, when: ((overlay) -> boolean)?, readUnnamed: boolean? }?) -> Overlay`

The old spelling of [`O:addPassThrough`](#o-addpassthrough), from when the pass-through was a mode: it adds the same control with the same `opts`. New code calls `addPassThrough`.

```luau
ov:enableUiaPassThrough({ label = "Kontakt controls" })   -- the same as ov:addPassThrough{…}
```

### Returns {#o-enableuiapassthrough-returns}

The overlay itself, not the control.

### Errors {#o-enableuiapassthrough-errors}

None. It never answers an error value.

**Raises** for a call after the overlay is bound: `overlay '<label>': enableUiaPassThrough must be called BEFORE binding it — the binding is what starts it`.

## O\:activate(index, press?) {#o-activate}

**Signature:** `O:activate(index: number?, press: { count: number }?) -> ()`

Activates the control at `index` (defaults to the focused control). `press` is handed on to the control's `onActivate`: a control's hotkey hands its own press here ([`O:group`](#o-group)), and a module may hand one it got from a key of its own; nothing reads it but `onActivate`. Behaviour by kind: `hotspot` clicks `at` and speaks `"label, activated"` — and, with `menuItem`, chooses that item once a menu test sees the menu (see [`O:chooseMenuItem`](#o-choosemenuitem)); `custom` calls `onActivate(self, press)`; `ocr` reads the region again, off the event loop, says it, and then clicks the region centre, unless the overlay or the user has moved on by then (see [`O:addOCRButton`](#o-addocrbutton)); `gtoggle` clicks the region centre and re-reads state after ~150 ms; `static` says the control again, interrupting, as its hotkey does (see [`O:addStaticText`](#o-addstatictext)); `ocredit` reads and clicks as `ocr` does ([`O:addOCREdit`](#o-addocredit)); `gbutton` searches, then clicks or drags where it was found ([`O:addGraphicalButton`](#o-addgraphicalbutton)); `hotspottoggle` clicks its point and says its state once it has changed ([`O:addHotspotToggle`](#o-addhotspottoggle)); `stepper` runs its `onActivate(self, press)`, and says the value it then reads. A slider, a tab control and a pass-through stop do nothing here. A control that cannot be placed now (a [scale](#o-scale) with no factor, a region function with no answer) clicks nothing and says so: `"<label> is not available now"`, or for an OCR control the `"cannot be read now"` of its announcement. A hidden control asked by its `index` does nothing; the focused one hidden since acts on the visible control that shares its hotkey, or says `"<label> is not available now"` ([`O:group`](#o-group)). No-op for a missing control.

```luau
-- Space and Return are bound to this by the runtime, so a module needs it only to press a
-- control from somewhere else. Melodyne swallows the first key a freshly focused window
-- receives, which makes a "do that again" key worth having.
host.hotkey.register("Ctrl+Alt+Return", function()
  if ov.active then ov:activate() end   -- no index: whatever is focused
end)

-- With an index, counted in `ov.controls`:
-- ov:activate(1)

-- A key of the module's own that presses control 3 as its hotkey would, count and all.
host.hotkey.register("Ctrl+Alt+3", function(press)
  if ov.active then ov:activate(3, press) end
end)
```

### Returns {#o-activate-returns}

Nothing. What the press did is said, and what it could not do is said and logged.

### Errors {#o-activate-errors}

None. It never answers an error value.

**Raises** only what a control's `at` function raises — called unguarded, as `O:frame`'s is — and for a `when` that yields inside the scan for the control that shares a hotkey. A control's `onActivate`, `text` and `when` run guarded: one that raises is logged and said (`"<label> could not be used"` for `onActivate`).

## O\:attach(matcher, opts) {#o-attach}

**Signature:** `O:attach(matcher, opts: { hoverToRead: boolean?, menus: {MenuTest}?, slot: string?, specificity: number?, pollMatch: number?, pollWhen: ((overlay) -> boolean)? }?) -> ()`

Binds the overlay as a **standalone** context: active while a window matching `matcher` is the foreground/active window — or while the overlay holds its place over its own menu, a window of the same application in front after one of its controls opened it, in which one of its tests sees a menu (see [`O.menuTests`](#o-menutests)) — with coordinates relative to that window's client area. `matcher` is a window matcher passed to [`host.window.test`](window.md#host-window-test), which checks it at every evaluation: a key no matcher has, or one of the wrong type, raises at the overlay's first evaluation, and the log writes it as the error of the handler that ran the evaluation, the first time it comes — `[timer] [com.example.synth] …host.window.test: matcher has no key 'class'; …` for the first evaluation, which a timer of the runtime's runs, `[window trigger]` or `[focus change]` for a later one — and the module's error dialog shows it once ([Errors are contained](../module-runtime-and-lifecycle.md#runtime-model)) ([Matchers](window.md#matchers)); `opts: { hoverToRead: boolean?, menus: {MenuTest}?, slot: string?, specificity: number?, pollMatch: number?, pollWhen: ((overlay) -> boolean)? }?` — `menus` is described under [`O.menuTests`](#o-menutests), and `slot`, `specificity`, `pollMatch` and `pollWhen` as for [`attachEmbedded`](#o-attachembedded). The overlay is first evaluated on the module's next turn after it is bound, not inside the call ([Where a hook runs](#where-a-hook-runs)).

While active, the overlay captures and suppresses the navigation keys, scoped to its own window (so `Alt+Tab` and menus pass through natively): `Tab` / `Shift+Tab` move between controls, `Return` and `Space` activate the focused control, and — when the overlay has a tab control — `Left`/`Right`, `Ctrl+Tab`/`Ctrl+Shift+Tab` and `Ctrl+<n>` drive it (the first two are Control+Tab on a Mac, see [below](#o-attach-macos)). `Space` is released while an editable field (an `ocredit` control) is focused, so a literal space can be typed into it. On activation the overlay starts at its first control (and any tab control at its first tab) when a **genuinely new** window opened, but resumes the last-focused control when the *same* still-open window merely regained the foreground (`Alt+Tab` out and back); the two are told apart by the window's identity (its HWND). `hoverToRead` (default `false`) moves the mouse onto an OCR control on focus (some UIs only reveal values on hover). Registers the foreground/focus trigger once.

```luau
-- Melodyne's standalone window, by executable and window class.
ov:attach({
  app = { exe = { contains = "Melodyne" } },
  windows = { class = "GNWindowDoc" },
}, {
  -- Melodyne has a native menu bar: while one of its menus is open the keys have to reach
  -- it rather than steer the overlay, and a menu the system draws is what nativePopup sees.
  menus = { O.menuTests.nativePopup },
})

-- Sharing an arbiter slot with other overlays over the same window, and re-checking the
-- gate on a timer because a panel can open and close with no window event at all:
-- settings:attach(on_ear, { menus = { O.menuTests.nativePopup, O.menuTests.newWindow },
--                           slot = ON_EAR_SLOT, specificity = O.layer.dialog, pollMatch = 500 })
```

### Returns {#o-attach-returns}

Nothing. The overlay is bound: what must be declared before binding raises from here on. Whatever the declaration gets wrong that does not raise — a control with no point, a hotkey the host refuses, a `specificity` without a `slot`, a relative image path — is reported now, one line each, `[overlay] <overlay>: <finding>`.

### Errors {#o-attach-errors}

None. It never answers an error value.

**Raises** for a mistake in the options, as the overlay is bound: `menus = true` (``overlay '<label>': `menus = true` no longer says how a menu is seen — …``, naming the building blocks), a `menus` that is not a list of tests or has a gap in it, a test's `before`, `pressed`, `forget` or `chained` that is not a function ([`O.menuTests`](#o-menutests)); a `pollWhen` that is not a function (``overlay '<label>': `pollWhen` must be a function(overlay) -> boolean, got <type>``); and, for an overlay with a [page](#o-addpage), its page key being the hotkey of one of its controls or tabs, or a second page in its group.

### Windows {#o-attach-windows}

The tab keys are the ones written: `Ctrl+Tab` and `Ctrl+Shift+Tab` cycle the tabs, and `Ctrl+1` to `Ctrl+9` go to tab 1 to 9.

### macOS {#o-attach-macos}

`Ctrl+1` to `Ctrl+9` are Command-1 to Command-9, because a spec's `Ctrl` is Command on a Mac ([the key spec](./keys.md#key-spec-string-format)). The two cycling keys are picked per platform instead: they are Control-Tab and Control-Shift-Tab (`Meta+Tab`, `Meta+Shift+Tab`), because Command-Tab is the application switcher, which the system keeps for itself.

## O\:attachEmbedded(spec, opts) {#o-attachembedded}

Binds the overlay as an **embedded** context: active while keyboard focus is inside a plug-in hosted in a DAW. Coordinates are relative to the plug-in's control — its own child window on Windows, the DAW's plug-in panel on a Mac (below) — so the same regions work standalone and embedded, and in every DAW.

`spec: { hosts: {Entry}?, host: Entry?, control: string | { windows: Control?, macos: Control?, linux: Control? } | ((activeWindow, hostPanel) -> control?)?, identify: ((control, again: () -> ()) -> boolean?)?, cacheIdentity: boolean?, present: ((control) -> boolean)? }` where `Control` is `string | ((activeWindow, hostPanel) -> control?) | false`.

`hosts` is the list of DAW entries the plug-in can be embedded in (falls back to `{ spec.host }`): window matchers carrying what each DAW knows about its plug-in windows. A plug-in module passes `daw.all` from `com.platform.daw-hosts` and names no DAW itself — see [daw-hosts](../daw-hosts.md) for the entry format and for adding a DAW. The first entry whose matcher takes the window in front is the one used; its `chrome` classes and its `pluginOrigin` apply. An entry is tested by its matcher keys alone, the entry's own fields left out ([the entry](../daw-hosts.md#entry)), since 0.12.1: any other key in it raises from [`host.window.test`](window.md#host-window-test) at the overlay's first evaluation, and the log writes it as the error of the handler that ran the evaluation, the first time it comes — `[timer] [com.example.synth] …host.window.test: matcher has no key 'pluginOrgin'; …` for the first evaluation, which a timer of the runtime's runs, `[window trigger]` or `[focus change]` for a later one — and the module's error dialog shows it once ([Errors are contained](../module-runtime-and-lifecycle.md#runtime-model)).

`control` says where the plug-in is, per platform, resolved in three steps:

1. **The module's own.** A string is a Luau pattern matched against the class names of the window's child controls (`host.window.controls()`) and of the focus chain (`host.window.focusChain()`); the first that matches and passes `identify` is the control. A function is called as `(activeWindow, hostPanel)` and returns a control table or `nil` (see [macOS](#o-attachembedded-macos)). A string that is not in an OS-keyed table applies on every platform.
2. **The DAW's plug-in panel**, when the module gives nothing for this platform — no entry, or a pattern of which no control both matched and passed `identify` — **and** the window's entry has a `pluginOrigin` on this platform **and** the binding has an `identify`. The panel says where *a* plug-in is and nothing about *which*, so a binding without `identify` never takes it.
3. **Nothing**: the binding is inert on this platform. It says so once at bind time, as `[<label>] embedded binding is inactive on <os>: <why>` — the module declared `false` for the platform; or neither the module (no pattern for the platform) nor any of its hosts (no `pluginOrigin` there) gives a control; or only the panel is on offer and the binding has no `identify`. It registers no trigger and asks nothing afterwards. The line is decided over all of a binding's hosts at once, so a binding with `daw.all` — where some entries have an origin — never says it for an entry that has none; the runtime says that instead, once per entry and VM, the first time such an entry's window is in front: `[overlay] '<title>' is a plug-in window of <daw>, whose daw-hosts entry gives no pluginOrigin on macos — no plug-in panel there, …`.

`false` for a platform is the module's word that the binding does not apply there, not even through the panel: Kontakt's "Content Missing" dialog overlay is `{ windows = "^NIChildWindow%x+$", macos = false }`. A pattern that looks like a Win32 class — no `AX` and no `/` in it — is reported once per pattern on a Mac, saying what the binding does then: takes the panel, or stays inert because it has no `identify`, or because none of its hosts gives a `pluginOrigin` there; written as `{ windows = … }` it is not tried there at all.

`identify(control)` is an optional confirmation callback (UIA / OCR / image search) — needed because a class pattern often matches any plug-in (the `Plugin<pointer>` class that several vendors' plug-ins share) and a panel matches every one. `true` matches; any other value except `nil` — `false` included — does not, and is kept as the verdict. `nil` means "cannot tell yet" (UIA not ready the instant a plug-in appears) and is not kept: the control does not match now and `identify` is called again at the next recheck — every focus event, and every `pollMatch` tick — up to 8 times in a row; the eighth `nil` in a row is kept as "no", as `false` would be. Each call runs on the event loop and costs whatever `identify` does (a UIA search of an Electron window was measured at 0.65–0.88 s), so an `identify` that means "no" returns `false`, not nothing. A kept verdict's `identify` runs in the handler of the event that resolves the origin, where it may wait for a read — unless the arbiter's call is the first to resolve it, bringing an overlay on a `slot` to the front after another claimant left: that is a plain call, where it cannot; one reached inside a scan of the overlay's hooks is not asked there, and is asked on the module's next turn ([Where a hook runs](#where-a-hook-runs)). One that raises is no verdict: "cannot tell yet", asked again at the next recheck and not counted towards the 8, logged once per control as `attachEmbedded: [<class>] id=<id>, identify raised: <message> — taken as could not tell yet, and not counted — '<overlay>'`.

**Asking again.** A kept verdict's `identify` is handed a second argument, `again`: a function that asks every overlay of this module bound to this `spec` table to evaluate once more, on the module's next turn (`host.timer.after(0)`, about 15 ms) — once however often it is called in a turn, and the same function every time. An `identify` that reads the screen **with a callback** — [`host.ocr.recognize`](ocr.md#host-ocr-recognize), as sforzando's does on a panel (below) — answers `nil` until its answer lands and calls `again()` from the callback; `false` would be kept. The evaluation that follows asks `identify` once more, and it answers from what its read found. That is the way to be picked up as soon as the answer is in: otherwise the next evaluation is whatever window or focus event comes next, and on a Mac, with the keyboard in a view that publishes nothing, there may be none. [`host.window.recheck`](window.md#host-window-recheck) did this before `again` existed (0.7), as a round of every module's overlays; it works as before, and is for a change a module made on screen itself. Each `nil` still counts towards the 8, so an `identify` that calls `again` after every read that saw nothing is bounded too. `again` is not handed to an `identify` asked at every evaluation (below), which answers at once anyway; one written for both ignores a missing `again`. Called after the keyboard has left, it brings nothing up: the evaluation finds what is in front then. Its answer has to land within those 8 evaluations: every recheck in between counts one, so under a `pollMatch` of 500 ms that is about four seconds of rechecks and nothing else happening, and a read that lands later finds "no" kept already — per control on Windows, for the rest of the stay on a Mac. A read that saw nothing at all (`"blank"`, `"none"`, `"failed"`) is no answer: leave the verdict `nil` so the next evaluation reads again, and keep `false` for text that is not yours. An async read inside a `control` **function** instead — Kontakt's — is not counted by the runtime at all: the function returns `nil` until its read lands, and the function bounds its own retries.

How long a verdict is kept is per platform — per control on Windows, per stay in the panel on a Mac — see the sections below. `cacheIdentity = false` keeps nothing: `identify` is called at every evaluation, for a verdict that depends on more than the control (Kontakt's: "is a Komplete Kontrol wrapped around it?"), so such an `identify` has to be cheap — and it must answer at once: it runs in the runtime's own coroutine, as a `when` does ([Where a hook runs](#where-a-hook-runs)), and so does the `identify` of a `control` function's control where the DAW gives no panel, which is asked at every evaluation too. One that raises is "no" for that evaluation, logged once per control: `… identify raised: <message> — taken as no this time — '<overlay>'`. `present(control)`, when given, is asked at every evaluation as well and never kept — for a condition that changes while the same control stays focused — in the runtime's own coroutine, so it must answer at once too. One that raises does not match, and is logged once per binding as `attachEmbedded: '<overlay>': its binding's present raised: <message> — taken as not matching`; the evaluation that asked it goes on to its end, so an overlay in front leaves it.

`opts: { hoverToRead?, menus?, slot: string?, specificity: number?, pollMatch: number?, pollWhen: ((overlay) -> boolean)? }` — `hoverToRead`, `menus` and the navigation / focus-reset behaviour are as in `attach`. With `slot` the overlay joins the host **arbiter** for that slot at `specificity` (a base and the overlays inheriting it pass the same slot; the most-specific *matching* one is active — see `host.arbiter`); `pollMatch` (ms) additionally re-checks the match on a recurring timer, for matches that change with no window event (a library landmark appearing inside an already-focused plugin). `pollWhen`, with `pollMatch`, is asked on each of its ticks, with the overlay, and the match is re-checked only when it answers `true`: for a match that can change without a window event only while the module expects it to — VPS Avenger's warning box, looked for only after a Save or Initialize of its own. Every other tick then costs one call of it rather than a context match (the focus chain is read, and on a Mac in Logic that is the slow accessibility). One that raises is logged once, `[poll] '<overlay>': pollWhen failed: …`, and the match is re-checked as if it were not there; anything but a function raises when the overlay is bound. The poll's interval and its one timer per interval are unchanged by it. The overlay is evaluated on the module's next turn after it is bound, like any binding ([Where a hook runs](#where-a-hook-runs)) — and an overlay that is already bound (to its standalone window, say) is evaluated again after this call, once, so a plug-in already in front is recognised without waiting for a window or focus event, which on a Mac may never come.

The control is resolved once per [`host.epoch()`](timer.md#host-epoch) per `spec` table, shared by every overlay bound to that table, so the controls, the focus chain and `identify` are asked once per event however many overlays and coordinates want the answer; on a panel the verdict is kept per `spec` table as well, so an overlay that asks while another bound to the same table is outranked does not ask again.

As with `attach`, the overlay also stays active while it holds its place over its own menu — a window of the same application in front after one of its controls opened it, in which one of its tests sees a menu (see [`O.menuTests`](#o-menutests)). When that ends, the match above decides again, exactly as for any other window change; if the keyboard is still in the menu's own window and that window is not shown, the window of the press is brought back first, once — and so it is when the dialog an item of its menu opened has gone and the keyboard went to another shown window of the same application that is no plug-in window of the binding, the DAW's main window after its file panel closed (since 0.13).

```luau
-- sforzando (modules/sforzando, shortened): its own window class on Windows, the DAW's plug-in
-- panel on a Mac, in every DAW daw-hosts has an entry for. "Plugin<pointer>" is shared by several
-- vendors' plug-ins and a panel holds whatever plug-in is shown, so `identify` says which.
local daw = host.require("com.platform.daw-hosts")
-- The wordmark relative to the plug-in's top-left; the Mac build lays its header out a few points
-- differently, which is sforzando's own difference and not a DAW's.
local WORDMARK = host.os.pick { windows = { 605, 33, 762, 70 }, macos = { 597, 29, 770, 74 } }
local pending, seen = nil, {}   -- the read out, and its answer: one window is in front at a time

ov:attachEmbedded({
  hosts = daw.all,
  control = { windows = "^Plugin%x+$" },   -- no macos entry: the panel there
  identify = function(ctrl, again)
    if host.element.find(ctrl.id, "PlogueXMLGUI", host.element.type.Pane) then return true end
    local x, y = math.floor(ctrl.client.x), math.floor(ctrl.client.y)   -- a panel's may carry a fraction
    local region = { x + WORDMARK[1], y + WORDMARK[2], x + WORDMARK[3], y + WORDMARK[4] }
    if ctrl.stay == nil then                       -- a control of its own: waits for the read, once per control
      local r = host.ocr.recognize(region, { lang = "en" })
      return string.find(string.lower(r.text), "sforzando", 1, true) ~= nil
    end
    local key = ctrl.id .. "|" .. ctrl.stay        -- the panel: once per stay, off the event loop
    if seen[key] ~= nil then return seen[key] end
    if pending ~= key then
      pending = key
      host.ocr.recognize(region, { lang = "en", key = "wordmark" }, function(r)
        if pending == key then pending = nil end
        if r.status ~= "text" then return end       -- nothing read: asked again next time
        seen = { [key] = string.find(string.lower(r.text), "sforzando", 1, true) ~= nil }
        again()                                    -- look again now that the answer is in
      end)
    end
    return nil                                     -- not false: false is kept for the stay
  end,
}, { menus = { O.menuTests.nativePopup, O.menuTests.newWindow } })
```

### Returns {#o-attachembedded-returns}

Nothing. The overlay is bound, as by [`O:attach`](#o-attach-returns), and the declaration's findings are reported now, together with the line that says a binding is inactive on this platform when it is.

### Errors {#o-attachembedded-errors}

None. It never answers an error value. A binding that cannot apply on this platform is no error: it says so once, `[<label>] embedded binding is inactive on <os>: <why>`, and asks nothing afterwards.

**Raises** for a mistake in the options, as [`O:attach`](#o-attach-errors) does. `spec` is not checked at the call. An `identify` and a `present` that raise are guarded and logged (above); a `control` function is not guarded, and what it raises comes out of whatever resolved the origin then (see the top of this page).

### Windows {#o-attachembedded-windows}

`control` matches a child window class: a DAW hosts a plug-in in a child window, and that window has a class name to match on. Candidates come from `host.window.controls()` plus the focus chain, and the entry's `chrome` classes keep the overlay out while the focused control is one of the DAW's own (REAPER's FX list, its buttons). A verdict of `identify` is kept per control handle, for the 64 controls asked about most recently, as in [`O.memoByOrigin`](#o-memobyorigin); the eighth `nil` in a row is kept for as long as that control is.

No daw-hosts entry has a `pluginOrigin` for Windows, so the panel of step 2 never applies there: every binding finds its own control or nothing, as it always has. An entry must not get one: every binding whose own class pattern found nothing would take the panel instead.

### macOS {#o-attachembedded-macos}

A plug-in inside a DAW publishes no control of its own, as far as anything has been measured: in REAPER, Kontakt 7's accessibility elements are direct children of the FX window, with nothing that *is* the plug-in; sforzando in REAPER publishes nothing at all, and VPS Avenger nothing in REAPER or in Logic. So the control is the **DAW's plug-in panel**, built by the runtime from the window's entry: `pluginOrigin(window)` gives `{ dx, dy }` from the window's content origin, and the panel reaches the window's right and bottom edges. It is a control table like one from `host.window.controls()`:

```luau
{ id = <the window's id>, app = <its application>, class = "host-panel",
  bounds = <the panel>, client = <the panel>, stay = <a number, see below> }
```

The window's `id`, so element queries still ask the right window; `bounds` and `client` both the panel. Every authored coordinate, `rawOrigin`, `fromRight` and `O.contentSize` behave against it as against a child control on Windows. `pluginOrigin` is called once per window and client size per VM — it may walk the window's accessibility tree — and a divider dragged inside a window that keeps its size is followed only after the window is resized, or closed and opened again as a new window (whether a DAW reuses the window it closed is not measured). A `pluginOrigin` that raises or answers anything but two numbers — `nil` included, which is how an origin says it could not look this time — gives that window no panel, with one line per window and size (`[overlay] the DAW entry's pluginOrigin for '…' (id=…, <w>x<h>) … — no plug-in panel there`), and is asked again at the next evaluation. An origin that leaves the panel no width or height (past the window's right or bottom edge) gives no panel either, silently.

**The gate is geometric.** The `chrome` classes are Win32 vocabulary and match nothing here, so the runtime asks where the focused element is. The focus chain has to **end in the window in front**: the window in front and the focused element are two separate questions to the system, and a chain that ends in another window — another application's, read in between — says nothing about this one. On such a chain the keyboard is not inside, and the origin is not asked for either. An **empty** chain — the backend saying it could not read where the keyboard is — is not inside either, and begins nothing, but it keeps a stay in its window (below): the system answers so while a view that publishes nothing has the keyboard, and Logic does after every click into a plug-in's own drawn view. Otherwise the window itself as the focused element — all a view that publishes nothing leaves on the chain — counts as inside, and so does an element whose centre lies inside the panel; an element whose centre lies outside the panel is the DAW's own chrome and the overlay stays out, and so is an element that has no rectangle (the host gives every element one; a control a module made may not). The accessibility focus on a menu item outside the panel takes the overlay out as any element outside it does.

**A verdict is kept for a stay.** One REAPER FX chain window shows whichever FX is selected in its list, in the same window — possibly at the same rectangle, since REAPER sizes the window to the plug-in — so a panel's verdict cannot be kept per window. Whether Logic's plug-in window can be switched to another insert from its header is not measured yet. The rule assumes that what changes the plug-in shown is the DAW's own chrome, and that the keyboard has to be there to use it. So a verdict is kept for a **stay**: the keyboard inside one window's panel, from the evaluation that first finds it there until anything else is observed — the keyboard on the DAW's chrome, another window in front, a chain that is another window's or an element without a rectangle, or the window's title or client size changed. The next time the keyboard is inside, `identify` is asked again, once. **An empty chain with the same window in front keeps the stay:** it observes nowhere else, and the panel the stay began with stands for the DAW's — the origin is not asked, `identify` answers from the verdict kept for the stay (or is asked, once, when it could not tell yet), and a `control` function is not asked: the control it last returned in the stay with the keyboard inside is the answer, moved with the window, with no keyboard test — and where it returned none in the stay, or one the keyboard was outside, the overlay is not up there and the empty chain does not bring it up. On a Mac an empty chain is also the host's busy quarantine, in which a function that looks for its plug-in's elements would find none, and a function that counts its misses (Kontakt's) would count those against the plug-in. A function whose own read with a callback lands while the chain is empty is asked again at the first evaluation whose chain is readable. The first evaluation that does so in a stay says it once: `[overlay] '<label>': where the keyboard is in '<title>' cannot be read (an empty focus chain) — the stay goes on there`. An empty chain begins a stay in one case only: in the focus round a module's own click asked for with [`host.window.recheck { entered = … }`](window.md#host-window-recheck), with the window it names in front and its point inside this binding's panel of that window — daw-hosts' focus key, after its click into the plug-in, when Logic reports no focus at all (since 0.13; the second Mac test of VPS Avenger, 2026-10-24, where nothing came up until the window was dragged). `identify` is asked then, as at any stay's start, and the line says it: `[overlay] '<label>': <module>'s click into '<title>' at <x>,<y> begins a stay — <DAW> reports no focus after it`. Any other empty chain begins none, so an overlay is not brought up on a reading nobody took: after a visit to the DAW's chrome, in another window, or in this one resized or retitled meanwhile, the overlay comes up at the first evaluation whose chain is readable. A `control` function is not asked on the click's round either, since what it returns is tested against where the keyboard is. It is observed at every evaluation, and by a watcher of the runtime's own on every focus and activation event in each VM that has a binding that can take a panel, so an overlay outranked on its slot — which skips its own rechecks — does not bring back a verdict from before the visit. Moving the window keeps the stay. The verdict is kept for the stay **and** the control's place in the window, so a control a function moves within one stay is asked about again. The eighth `nil` in a row is kept as "no" for the rest of the stay.

What the stay cannot see: a plug-in changed with the keyboard still inside it keeps the old verdict until the keyboard next leaves; and a visit to the DAW's chrome that no evaluation *samples* is not seen either. A focus change on a Mac only marks the focus as changed, and the round that follows reads the chain as it is by then, while moving into a view that publishes nothing raises no notification at all — so Shift+Tab to REAPER's FX list, Down, Tab back, all done before a busy event loop gets to its queue, reads as one round with the keyboard inside, and the old verdict goes on over the newly selected plug-in. `host.window.recheck`, and so the focus key's "already inside" press, is one more evaluation: it undoes an `identify` that could not tell yet, not a verdict the stay keeps. And a visit to the DAW's chrome made while the DAW is in the host's busy quarantine (the five seconds after it did not answer an accessibility question: [`host.window.focusChain`](window.md#host-window-focuschain)) reads as an empty chain too, which keeps the stay: the overlay then holds its keys over the DAW's own controls until the quarantine ends, when the host runs a focus round whose evaluation reads the chain again ([`host.window.focusChain`](window.md#host-window-focuschain-macos), "Read again when the quarantine ends") — or, while the DAW goes on not answering, until the first such round it answers. Before the empty chain kept the stay (0.7), every click into VPS Avenger's view in Logic ended it — the first Mac test of Avenger's page, 2026-10-14 — and took the overlay down with a choice of its menu still waiting.

The verdict is logged when it differs from the one logged last for that window and place, naming the overlay that asked: `attachEmbedded: [host-panel] panel of '<title>' id=<id> at <dx>,<dy> <w>x<h> from its content origin, identify=<verdict> — '<label>'`. One line per VM, when the first binding with no control of its own for the platform is bound, says that it takes the panel: `[overlay] on macos an embedded binding with no control of its own takes the DAW's plug-in panel … '<label>' is the first here`.

**The stay is held by a page.** While an overlay's [page](#o-addpage) holds the place over the window the stay is in — from the page key's show putting the page in front, through every trip, until away ends — nothing that would end a stay ends it: the page in front, the plug-in's window, and a window of its application on a trip or a way back keep it, so no `identify` is asked within a trip or between trips. The plug-in's window back with another title ends it (and a trip under way, `"gone"`); a new size during a trip is a zoom step, whose size the stay takes, and the panel is worked out again for it; a new size outside a trip, or a way back that did not reach the plug-in's window within 8 seconds, ends it.

`stay` on the panel is a number per VM that changes when a stay ends; two VMs' numbers are unrelated. It is there for a module whose own evidence is costly and whose `identify` is asked at every evaluation (a `cacheIdentity = false` one, Kontakt's): it keys that evidence on the stay, the way the runtime keys a verdict.

**A function for the platform** is called as `(activeWindow, hostPanel)`, where `hostPanel` is the panel above (with its `stay`) or `nil` when the window's entry has no `pluginOrigin`, and returns the control or `nil`. It is for a plug-in that knows its own corner better than the DAW's origin does, or that publishes a container of its own: Kontakt finds its FILE button and hands back the panel from its own corner. The control needs `bounds`: the keyboard has to be inside the DAW's panel **and** inside what the function returned. Not their union — the DAW's panel is the DAW's word for where its chrome ends — so a control that begins above or left of the DAW's panel has a strip in which the keyboard reads as the DAW's chrome; a DAW origin that far off is a wrong entry. What it returns gets the panel's `stay`, and its verdict is kept for the stay as the panel's is; where there is no panel, `identify` is asked at every evaluation. A `macos` pattern — `"^AXGroup/"` — matches the accessibility role, subrole and identifier a control's `class` is here (`AXRole/AXSubrole/AXIdentifier`), for a plug-in that does publish its own container.

```luau
-- Kontakt 7 in a DAW (modules/kontakt, detect.onHostPanel, much shortened): the DAW's panel, from
-- Kontakt's own corner when its FILE button, with LIBRARY beside it, is near where Kontakt's
-- geometry puts it. The module keeps what it found per stay, and reads the top row as text when
-- no button is published.
control = {
  windows = "Qt%d+.-QWindowIcon",
  macos = function(active, panel)
    if not panel then return nil end
    local B = host.element.type.Button
    local p = host.element.locate(active.id, "FILE", B)
    local q = p and host.element.locate(active.id, "LIBRARY", B)
    if not (q and q.x - p.x >= 35 and q.x - p.x <= 90) then return nil end
    local dx, dy = p.x - (panel.client.x + 175), p.y - (panel.client.y + 19)
    if math.abs(dx) > 24 or math.abs(dy) > 24 then return nil end   -- some other plug-in's FILE
    local x, y = panel.client.x + dx, panel.client.y + dy
    local w = active.client.x + active.client.w - x
    local h = active.client.y + active.client.h - y
    return { id = panel.id, app = panel.app, class = panel.class, variant = "Kontakt 7",
             bounds = { x = x, y = y, w = w, h = h }, client = { x = x, y = y, w = w, h = h } }
  end,
}
```

## O.menuTests — seeing a plug-in's menus {#o-menutests}

**Signature:** `menus = { test, … }` in the options of `O:attach`, `O:attachEmbedded`, `O.window` and `O.embedded`, where each `test` is `(o: Overlay, answer: (seen: any) -> ()) -> ()` or `{ name: string?, cheap: boolean?, test: (o, answer) -> (), before: ((o, press: Press) -> any)?, pressed: ((o, press: Press, looked: any) -> ())?, forget: ((o) -> ())?, chained: ((o, press: Press, menu: Menu, done: (ok: boolean, why: string?) -> ()) -> ())? }`, `Press = { label: string, x: number?, y: number?, button: string?, parent: { x: number, y: number, w: number, h: number }? }` and `Menu` a test's answer that says where the menu is · the building blocks `O.menuTests.nativePopup`, `O.menuTests.newWindow`, `O.menuTests.accessibility`, `O.menuTests.accessibilityAfterPress`, and `O.menuTests.drawn(spec: { name: string?, region: (o, press: Press) -> ({ x1, y1, x2, y2 }?, { w, h }?), minW: number?, minH: number? }) -> test`

The tests that tell an overlay its plug-in has a menu open, listed in its `menus` option; while a menu is open, the keys belong to it. The overlay gives up its per-control hotkeys — a registered hotkey outranks the application, so `Alt+M` would otherwise re-fire the control under the menu — and tells the key hook through [`host.keys.menuOpen`](keys.md#host-keys-menuopen) to let the navigation keys it captures (`Tab`, `Return`, `Space` and whatever the focused control holds) through to the application. **How a menu is seen is the module's decision**: the module names the tests, and the runtime runs them. ReaHotkey does the same with one fixed check, `WinExist("ahk_class #32768")`; a plug-in that paints its own menu needs a check of its own, and only its module knows which.

A test answers one question — *is a menu open over this overlay's plug-in right now?* — by calling `answer(seen)` exactly once per call, at once or later. Any value other than `nil` and `false` counts as seen, so a search's hit can be handed straight on. An answer that is a table with numbers `x`, `y`, `w` and `h` (`w` and `h` above 0) also says **where** the menu is, on screen, which is what an item is chosen in ([`O:chooseMenuItem`](#o-choosemenuitem)): it has to be the whole menu, since an item outside it is not clicked. `newWindow` answers with one; a search's hit is such a table too, but only of what its template shows — hand one on as the menu's rectangle only when the template is the whole menu, and otherwise answer the menu's own rectangle worked out from it. `o` is the overlay, so `o:origin()` is the plug-in's window or control. A synchronous check answers before it returns; a test that passes a [`host.screen`](screen.md#with-a-callback) call a callback — an `imageSearch`, a `matchCells`, a `pixel` or a `snapshot` — answers from the callback, which the runtime receives on the main thread like every callback. A bare function is a test named `test <n>` after its place in the list and is not cheap. The table form adds:

- `name` — what the log calls it. Default `test <n>`.
- `cheap = true` — may run on every tick even when nothing was pressed (see below). Default `false`.
- `before(o, press)` — called at a press **before it counts**, in the press's handler, where a read may wait ([Where a hook runs](#where-a-hook-runs)): for a test that needs a picture of the moment before the click (`drawn`). What it returns is handed to `pressed` as its third argument. Since a read waited, the press is checked afterwards as after a calibrating run's menu shot: an overlay that is no longer where the press was clicks nothing (`not_in_front` or `moved_on`), and an opener's point that is no longer its own is refused (`outside` or `covered`) — see [`O:chooseMenuItem`](#o-choosemenuitem), step 2. A press made where a read cannot wait — in an `onDone`, which runs in the menu tick's coroutine — gets a `before` whose read raises there; `drawn` catches that and arms nothing.
- `pressed(o, press, looked)` — called when a control with `opensMenu` is activated, before it acts: for a test that compares against that moment. `press` is `{ label, x?, y?, button? }` — the control's or the choice's label, and for an opener with a point (a hotspot, [`O:chooseMenuItem`](#o-choosemenuitem)) that point on screen and its button — and `looked` what the test's own `before` returned.
- `forget(o)` — called when the user presses one of the overlay's own keys (a navigation key or a hotkey) while no menu counts as open — so the last press opened nothing that took the keys; while an item waits for its menu ([`O:chooseMenuItem`](#o-choosemenuitem)), once the key has run, and not for a second press of the same control — when the overlay leaves the front, and when the plug-in has the keyboard again after the overlay held its place over a menu that was a window in front (see below): a comparison against that press ends there.
- `chained(o, press, menu, done)` — called when the item this test placed opens a **submenu** ([`O:chooseMenuItem`](#o-choosemenuitem)'s `submenu`, step 7), **before** the item is clicked: for a test that has to get ready to see a popup that opens from a menu it already sees. `press` is `{ label, x, y, button = "left", parent }` — the item's point on screen, and `parent` the menu's rectangle as this test answered it — and `menu` that answer itself. It runs where the test's answer runs — in the menu tick's coroutine for an answer given at once, in the menu tick's work after a callback otherwise — and **must not wait**: it calls `done(true)` once it is ready, at once or from a callback, or `done(false, why)` when it cannot follow a submenu, and the item is then not clicked. A second `done` is logged and ignored; a hook that raises or yields counts as `done(false, <its error>)`; one that has not answered when the press's 8 seconds are over ends the choice, and `onDone` hears `not_ready` — `closed` if the menu closed meanwhile; its `done` after that clicks nothing. `newWindow` and `drawn` have one (see [Following a submenu](#o-menutests-submenu)); a test without one is not told, and its next answer that is not the menu is taken for the submenu.

`opensMenu = true` on a control ([`addHotspotButton`](#o-addhotspotbutton), [`addCustomButton`](#o-addcustombutton), [`addOCRButton`](#o-addocrbutton), `addGraphicalButton`) says that activating it puts a menu on screen. It makes the tests run on every tick for a while, calls their `pressed`, and in a calibrating run takes the [menu shots](#o-calibrating). It never counts a menu as open by itself.

**The runtime has no timer that decides whether a menu is open.** Each test has a *word*: from the answer that saw a menu, it says one is open, and it stops saying so once it has answered no **twice in a row** — one missed read is not a close, because an answer can be a moment old (the accessibility walk can hand a tick the previous tick's answer) or catch a menu mid-redraw. A menu counts as open while any test's word is that it is, and closes when none says so any more. When it opens the overlay hands its keys over once, and when it closes it takes them back once, however long the menu stays up. **A menu no test sees gets no pass-through**: the overlay keeps its keys while it is up, exactly as if there were no menu. Nothing below the runtime decides it by time either: the host's `nativeMenuOpen` counts a menu open until it closes (see [macOS](#o-menutests-macos)).

**When the tests run** sets only how often they are asked, never what they decide. The runtime ticks every 150 ms, on the main thread. On each tick, for each overlay in front:

- every test runs for 8 seconds after a control with `opensMenu` was activated, and for as long as a menu counts as open;
- otherwise a test declared `cheap` runs on every tick, and each of the others once 8 ticks (about 1.2 s) have passed since it was last asked, so a menu opened some other way — the user's screen reader, a key that went to the plug-in — is still noticed;
- while the word of a `cheap` test is that a menu is open, the others are not asked at all: they could only agree — except while an item waits to be chosen, or to be seen taken ([`O:chooseMenuItem`](#o-choosemenuitem)): then every test is asked, since the one that says where the menu is need not be the cheap one that sees it.

**A test runs in the runtime's own coroutine**, with the other tests of its tick, and must answer at once or from a callback: it is asked on every tick, and a read there would keep the module busy all the time ([Where a hook runs](#where-a-hook-runs)). One that yields has failed, as one that raises has — said once, and counted as not seeing a menu — and is asked in a coroutine of its own from then on, so that it fails alone: the tests asked after it on that tick, of its overlay and of the others, are asked on the next.

**A test that has not answered yet** is not asked again until it does. It holds up none of the other tests, and its word stands meanwhile, so a slow test does not make a menu it sees flicker. There is no timeout: a test that never answers again after seeing a menu keeps that menu open until the overlay leaves the front, and the log names a test whose answer has been outstanding for 20 ticks. A test that raises has answered no, and a second answer to one question is ignored; each is logged once per test. When the overlay leaves the front every word is dropped and the state starts again from nothing; an answer that arrives later for a question asked before that is ignored.

**One timer per module.** The overlays of a module share it, and while a menu counts as open over any of them, every one of them that is in front gives up its hotkeys: the pass-through is one flag for the module, and it lets every capture in the window it counts for through, another module's included (see [`host.keys.menuOpen`](keys.md#host-keys-menuopen)).

**A menu that is a window in front.** Some menus are windows of their own that come to the front — Komplete Kontrol's own menu in REAPER is a popup titled "Komplete Kontrol", a window of `reaper.exe`. The overlay's binding no longer matches then (its window is not in front), but it does not leave the front: it **holds its place** over its own menu. Activating a control with `opensMenu` records the window in front and its process — the *press*.

- **On the menu.** A window in front that is not the one of the press, but belongs to the same process, is held when one of the overlay's tests sees a menu **there and then**: the tests are asked at that moment, so `newWindow` sees the popup at once, and only an answer that sees a menu counts. The *word* does not: it says "open" for one more miss after a menu has closed, and a dialog an item of a native menu opened — Komplete Kontrol standalone's Preferences, which has an overlay of its own — is no menu of this overlay's; the overlay leaves the front for it as it always did. A test that answers only later (from a callback) cannot hold a window, so a module whose menu is a window lists a synchronous test that sees it: `newWindow`. Held, the overlay stays in front but holds **no keys at all** while that window is: no navigation key, no hotkey, and no say in the pass-through flag ([`host.keys.menuOpen`](keys.md#host-keys-menuopen)), so the window's keys are its own — whatever it leads to: a dialog opened from the menu may have an overlay of its own. Its tests run on, and each window event asks them again. Its origin stays the plug-in as it was at the press, and a [gate](#o-gate) is not asked meanwhile. It keeps its claim on its arbiter slot, so an overlay of the same or a lower specificity on that slot does not win a window the menu opened until the hold ends.
- **When the hold ends** — the window in front is another application's (`Alt+Tab`), the window of the press, a window of the same application in which no test sees a menu now, or nothing once the tests no longer see the menu — the binding's own match decides, **exactly as for any other window change**. That is ReaHotkey's rule as well: its plug-in context is REAPER's FX window in front with the plug-in's control found in it. With the keyboard in the plug-in the overlay is still in front and has its keys again: its navigation keys at once, scoped to its window, and its hotkeys when its tests stop seeing the menu, as for any menu; the tests forget the press there, so a window the menu opened that is still up is no longer a menu, and for `newWindow` the hotkeys are back by the next tick — until then the navigation keys pass through. With the keyboard anywhere else — on REAPER's FX window itself, which its [chrome gate](#o-attachembedded) counts as REAPER's own — the overlay leaves the front, and comes back through an ordinary activation, which speaks its control, once the keyboard is in the plug-in. A gate is asked again. A window that comes up with no press behind it, or once the press is forgotten — the overlay's own next key with no menu open, leaving the front, the plug-in having the keyboard again after a hold, or the next press — is never held.
- **The keyboard left in the menu's own window.** A plug-in can hide its menu's window instead of closing it, and the keyboard can stay in that window while nobody can see it. Komplete Kontrol in REAPER is the case this was built for: after `Escape` the first key was lost, and the overlay came back only about four seconds later, when REAPER's FX window was in front again — most likely because KK only hid its menu's window and the key went into it; that has not been measured. [`host.window.active()`](window.md#host-window-active) cannot show that state: it reports a hidden window as none, and answers from what was read in the current epoch. [`host.window.foreground()`](window.md#host-window-foreground) can: the foreground window, shown or not, read afresh. So when a hold ends because no test sees the menu any more, the runtime asks it, and when the foreground window is the **menu's own window** — the first one the hold was over after the press, of the press's process — and it is **not shown**, the runtime brings back the window that was in front at the press with [`host.window.focus`](window.md#host-window-focus) — **once per press**, whatever the answer — and asks the binding's own match again at once; from there it is as above. First it checks that the window of the press is still listed by [`host.window.list`](window.md#host-window-list) for its process with its class, because a menu item can close it and Windows reuses a gone window's id; `list` lists titled windows only, so an untitled window of the press is never brought back. If it is not listed, or the listing fails, nothing is brought back, and the log says so. Nothing is announced: the log line gives the platform's answer, `accepted` or `declined`, and whether the overlay is back is the match's to say. On Windows an accepted request is completed when REAPER's thread gets to it, so the match asked at once most likely still finds the hidden window in front — nothing, to `active()` — and the overlay leaves the front, unless the word still says the menu is open; REAPER's foreground event, when the change is made, brings it back with the keyboard in the plug-in, through an activation that speaks its control. Nothing is brought back when the foreground window is shown, or is any other window — a dialog the menu led to, a submenu that is a window of its own, another of the same application, the window of the press, another application's — or when there is none, or it cannot be read (the call raises, or on a Mac the application does not answer); the reading is then logged instead, once per press. The menu's own window getting the keyboard back after a dialog it led to has closed does count: it is still the menu's window, not shown. **After a dialog** (since 0.13) one other window counts as well: the window that appeared after the press — a dialog an item of its menu opened, the first window held over after that item was clicked by [`O:chooseMenuItem`](#o-choosemenuitem) (or a hotspot's `menuItem`), Logic's "Load Preset..." panel from VPS Avenger's MENU; never a menu's own window — has gone, it is listed for its process no more, and in that same change the window that gets the keyboard is a shown window of the same process other than the press's and the dialog's that the binding's DAW entries do not take as a plug-in window: the DAW's main window, where Logic leaves the keyboard when its file panel closes (the second Mac test of VPS Avenger, 2026-10-24, where the overlay went down there until the focus key). The window of the press is then brought back the same way, once per press, only while it is listed with its class, its title and its size as at the press; the log says `[menu] '<label>': the window that appeared after '<control>' was pressed, '<title>' (<exe>), has gone, and the keyboard is in '<title>' (<exe>) of the same application, which is no plug-in window — bringing back …`, or `… — but the window that appeared is still listed, so nothing is brought back`. Only a binding of `attachEmbedded` can tell a plug-in window, so only its overlays take this step. The step runs when the hold ends: with `newWindow` that is the tick that closes the menu, at its second miss; an overlay with `pollMatch` can reach it on its poll's first miss, while the word still says open, because the poll asks the tests itself and a hold needs a test seeing the menu there and then — `shown = false` is what keeps one missed reading of a menu that is still up from bringing anything back.

No timer decides any of it: the window in front, the press, the tests' answers and the window that gets the keyboard do. With nothing in front — the moment between a popup closing and its owner coming back — a hold over a menu the tests still see stands until a window is. Only an overlay with menu tests holds its place.

**While away.** An overlay whose group is [away](#o-away) — in front quietly during a trip of its page — runs its tests and holds its place over its menu as above, but holds no keys before, during or after: when the hold ends, nothing is taken back, and a menu closing hands nothing back either. Its keys come back when away ends. A trip waits for such a menu with [`trip:afterMenu`](#trip-aftermenu).

`menus = true` **raises** when the overlay is bound, with a message naming the building blocks. So does anything that is not a list of tests, a list with a gap in it (a `nil` before another entry, which is what a misspelt name leaves), and a `before`, `pressed`, `forget` or `chained` that is not a function; reading a name that `O.menuTests` does not have raises as well. `menus = false`, `{}` or no `menus` means no tests: nothing runs, and a control declared with `opensMenu` on such an overlay is reported when the overlay binds.

The log lines start with `[menu]`: which test saw a menu first, which control's press it followed (or that none did), and the keyboard focus at that moment ([`host.window.focusChain`](window.md#host-window-focuschain)`()[1]`); the keys that went through to an open menu; whether the menu was still seen two ticks after an `Escape` went through; a test that has not answered for 20 ticks; and, when the menu closed, how long it counted as open. For a menu that is a window in front: `… came to the front after '<control>' was pressed and a test sees a menu — the overlay holds its place while it is up, holding no keys`; `the plug-in has the keyboard again` when a hold ends with the binding matching; `no longer holding its place — in front now: …` when it ends any other way, naming what [`host.window.active()`](window.md#host-window-active) answers — which can be a window read earlier in the epoch that has hidden since, so it does not say where the keyboard was; `no test sees the menu any more; the keyboard goes to id=<id> of pid <pid>, shown` (or `not shown`, or `no window the platform names`, or `a window that could not be read (<reason>)`) `, and the menu's window '<title>' (<exe>) is id=<id> of pid <pid> — nothing is brought back`, which does, when a hold ends with no test seeing the menu and nothing is brought back; and `no test sees the menu any more, but its window '<title>' (<exe>) still gets the keyboard and is not shown — bringing back '<title>' (<exe>), where '<control>' was pressed: accepted` — or `declined`, followed by the reason when the call raised — or, when the window of the press is not found, `… is not shown — the window of the press, '<title>' (<exe>), is not listed for its process with its class (gone, untitled, or its id now another window's), so nothing is brought back`, or `… could not be looked for (<reason>), so nothing is brought back`. The overlay's `[keys] … gave up its per-control hotkeys (menu open)` and `… took back …` lines mark the hand-over itself.

The building blocks are tests like any other, to be listed where they see a plug-in's menus:

| Block | Sees | Cost | Cheap |
| --- | --- | --- | --- |
| `O.menuTests.nativePopup` | A menu the operating system drew, open in the application in front. | One flag read, [`host.keys.nativeMenuOpen`](keys.md#host-keys-nativemenuopen). | yes |
| `O.menuTests.newWindow` | A window of the plug-in's process that was not there when a control with `opensMenu` was activated: a popup menu drawn as a window of its own. The process is settled at that press and kept. It compares only after such a press, and stops when the windows that appeared have all gone, at `forget`, and at the next press, which starts a new comparison. While it compares, **any** new window of that process counts — a dialog as well as a menu, whether it takes the front or not; one that takes the front keeps the overlay holding its place, with no keys, until it is gone (above). A menu no control of the overlay opened is not seen. It answers with the menu: `{ kind = "window", id, window, layer, class, x, y, w, h }` of the **largest** window that appeared (a toolkit can draw a popup's shadow as windows of their own, thin strips round it), the first the window list names of two the same size; `id` as [`windowsOf`](window.md#host-window-windowsof) lists it — which [`host.window.ownsPoint`](window.md#host-window-ownspoint) takes with `listed = true` — and `window` the same window as a `host.window` id where there is one (see the platform sections). | One [`host.window.windowsOf`](window.md#host-window-windowsof) of one process per tick while it compares, nothing otherwise; one more as it gets ready to follow a submenu. | yes |
| `O.menuTests.accessibility` | An element of the Menu type anywhere in the plug-in's accessibility tree, [`host.element.find(origin, "", host.element.type.Menu)`](element.md#host-element-find), asked with a callback: the test answers when the tree has, on a later turn. An element that stays in the tree while nothing is drawn counts as a menu for as long as it is there. A walk the plug-in did not answer ([not known](element.md#asking-without-waiting)) is answered with the test's last answer: no miss, and no menu that was not seen. | A walk of the plug-in's whole tree, measured at 50–194 ms — on Windows off the event loop, on the host's accessibility thread. Answered once per [epoch](timer.md#host-epoch), which a tick does not turn over, so a tick can get the previous tick's answer. | no |
| `O.menuTests.accessibilityAfterPress` | What `accessibility` sees, asked only from the activation of a control with `opensMenu` until that press is done with: the menu it opened was seen and has closed (answered no twice in a row; a walk that was not answered counts as neither), one of the overlay's own keys arrived with no menu open, the overlay left the front, or the plug-in has the keyboard again after the overlay held its place over a menu that was a window in front. For a plug-in whose tree can hold a Menu element while nothing is open: such an element keeps the overlay out only until the user leaves the plug-in and comes back. A menu no control of the overlay opened is not seen. | The same walk from a press until it is done with, nothing otherwise. | no |
| `O.menuTests.drawn{ region }` | A filled area of the rectangle `region` names for the press, changed since the press and read the same twice in a row: a popup painted inside the plug-in ([its entry](#o-menutests-drawn)). Answers `{ x, y, w, h, print }` with no `kind`; `print` says how the popup's lines read, so that one drawn exactly as at an item's click is told from one drawn again (since 0.13). A menu no control of the overlay opened is not seen. | One capture at the press, waited for; then one profile with a callback a tick while it compares, two while it follows a submenu; nothing otherwise. | no |

A menu the plug-in paints inside its own window, with no element for it, is seen by none of the first four — the window list does not change and nothing is announced. [`drawn`](#o-menutests-drawn) sees it by its pixels, with no template, where the module says it is drawn; a module's own test can instead test a pixel or an image that only an open menu has, read off the menu shots of a calibrating run.

**[`O.menuTests.drawn{ region, … }`](#o-menutests-drawn)** makes a test for a popup **painted** inside the plug-in: how it sees one, when it has gone, what it costs and what it logs are in its own entry.

### Following a submenu {#o-menutests-submenu}

An item that opens a submenu ([`O:chooseMenuItem`](#o-choosemenuitem)'s `submenu`) is clicked once the test that placed it is ready to see the submenu: its `chained` hook is called first, and the item clicked on its `done(true)`.

- **`newWindow`** takes the window list again — the menu's window is in it now — and from then on answers the **largest window not in that list** as the submenu, in the same form as any menu; while there is none, the menu's own window as it stands, while it is listed; with both gone, `false`, and it stops comparing. Its `chained` answers `done(false, "it is not comparing")` when it is not comparing (no press of the overlay's own opened the menu). Logged: `newWindow: follows the submenu of '<label>' — a window that appears from now on, its menu the window <id>`, and at the end `newWindow: the menu and its submenu have gone` (or `the menu has gone, and no submenu was seen`). Cost: one window list of one process, as on every tick.
- **`drawn`** asks the module's `region(o, press)` again, with `press.parent`: the rectangle the submenu is drawn in, which should leave the menu out — a rectangle inside the menu, give or take two pixels, is never taken for its submenu anyway. It takes two [profiles](screen.md#host-screen-profile) **with callbacks**, of that rectangle and of the menu's own rectangle as it answered it, and calls `done(true)` once both are in: `drawn: follows the submenu of '<label>': looked for in x1,y1-x2,y2, its menu at x,y wxh`. With no menu seen by it, `done(false, "it has not seen the menu the item is in")`; with no rectangle, `done(false, "no rectangle for the submenu of '<label>'")`; a picture that could not be taken, `done(false, <why>)`. From then on, two profiles a tick, with callbacks, and once both are in:
  - the submenu's rectangle is compared as a press's is, by the same rules — changed, filled, seen alike twice, still there, gone (`a submenu wxh at x,y, alike at readings …`, `the submenu wxh at x,y has gone at reading n (…) — its menu is followed`) — and its rectangle is answered while it is seen;
  - otherwise the menu is answered while it is **still there by its rows**: gone once the rows whose fill changed since its profile before the item's click come to the smallest popup's height in pixels **and** fewer than 70 % of its rows read as they did. Its columns are left out: the pointer on the item highlights its row, a band of the menu's rows at a highlight's contrast, which moves the mean of every column of the menu past 6 and would read the menu as gone by the rules of a press. One band of rows is fewer than the smallest popup's height; a menu that closes changes nearly all of them;
  - both gone: `false`, and it stops comparing until the next press (`the submenu and its menu have gone (…)`, or `the menu of '<label>' has gone before a submenu was seen (…)`).

  No submenu seen within 8 seconds of the pictures before the item's click: the submenu is no longer looked for (`no submenu of '<label>' within 8000 ms — its menu is followed alone`), and the menu is followed alone, one profile a tick, until it goes — its word is the menu's, so a module can still close it with a key while it counts open. A reading that cannot be taken keeps the word (`a reading of the submenu of '<label>' could not be taken: <why>`). Cost: two captures with callbacks before the item's click (36–91 ms each on a Mac, about 16.7 ms on Windows' standard path, off the event loop), then two captures a tick while it follows, and in Luau at most 1024 comparisons for the submenu and 256 for the menu's rows a tick; one capture a tick once the submenu is no longer looked for.
- `nativePopup`, `accessibility` and `accessibilityAfterPress` have no `chained`: they say no rectangle, which places no item anyway.

**A choice is followed only by the test that placed its item.** With `newWindow` and `drawn` on one list, both can see one popup — its window, and the area it covers with its shadow — and answer rectangles a little apart: the one that placed the item is the one whose later answers say whether the menu took the click (step 6 of [`O:chooseMenuItem`](#o-choosemenuitem)), and the other's count only towards the menu's word. It is that test itself, not its name: two tests on a list may share one — two `drawn` without a `name` are both called `drawn` — and only the one that placed the item is followed.

```luau
-- sforzando: a native popup on Windows, a window of its own on a Mac.
ov:attach(sforzando, { menus = { O.menuTests.nativePopup, O.menuTests.newWindow } })

-- Kontakt in a DAW: its Qt menus are Menu elements, but its tree can keep one while nothing
-- is open, so the walk is asked only after a press of the snapshot dropdown or VIEW.
kontakt:attachEmbedded(spec, { menus = { O.menuTests.nativePopup, O.menuTests.accessibilityAfterPress } })

-- Komplete Kontrol in a DAW: its own menu is a popup window of the DAW's process that comes to
-- the front. newWindow sees it, so the overlay holds its place while it is up, holding no keys;
-- once it has closed, the binding's own match decides.
kk:attachEmbedded(spec, { slot = SLOT,
  menus = { O.menuTests.nativePopup, O.menuTests.accessibility, O.menuTests.newWindow } })

-- VPS Avenger: JUCE popups, windows in one DAW and painted inside the editor in another. `drawn`
-- looks where each press's popup is painted: below the zoom field for the zoom list, round the
-- point of a right click for a tab's menu (modules/vps-avenger/src/popups.luau has every one), at
-- 50 % from Avenger's corner and placed at the zoom read (`factorOf` is the module's own); nil for
-- a press that opens none of these.
local function popupRegion(o, press)
  local k = factorOf(o:origin())
  if not k then return nil end
  if press.label == "Zoom" then return o:toScreenRect({ 12, 18, 72, 262 }), { 20 * k, 34.5 * k } end
  if press.button == "right" and press.x then
    return { press.x - 120 * k, press.y + k, press.x + 120 * k, press.y + 145 * k }, { 30 * k, 23 * k }
  end
  return nil
end
avenger:bind(binding, { menus = { O.menuTests.newWindow, O.menuTests.drawn({ region = popupRegion }) } })

-- A module's own test, one pixel: a plug-in whose list, while it is open, has a white border
-- at content point (412, 140) — the kind of point read off the menu shots. A test must not wait,
-- so it reads that point with a callback, off the event loop, and answers from the callback.
-- Each ask is a read on the screen-capture thread (~16.7 ms on Windows), so it is not declared
-- cheap: every tick after a press and while the list is open, once in eight ticks otherwise.
local list = {
  name = "preset list",
  test = function(o, answer)
    local c = o:origin().client
    host.screen.pixel(c.x + 412, c.y + 140, nil, function(p)
      answer(p ~= nil and p.r > 200 and p.g > 200 and p.b > 200)   -- nil: not read this time
    end)
  end,
}

-- And an image, answered from the search's callback: a hit counts as seen, nil as not.
local fileMenu = {
  name = "file menu",
  test = function(o, answer)
    local b = o:origin().bounds
    host.screen.imageSearch(host.path("images/FileMenu.png"),
      { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
      function(hit) answer(hit) end)
  end,
}

ov:attachEmbedded(spec, { menus = { O.menuTests.nativePopup, list, fileMenu } })
```

### Windows {#o-menutests-windows}

`nativePopup` is true while the foreground thread is in menu mode — a `#32768` popup menu, a menu bar, a window's system menu. A menu open in another application does not count. `newWindow` compares visible top-level windows of the process, tooltips left out, so a popup the toolkit draws as a tool window of its own counts; its answer's `window` is the popup's HWND, which is what every `host.window` call names, and its `id` the same number, so [`host.window.ownsPoint`](window.md#host-window-ownspoint) can be asked about it either way. `accessibility` and `accessibilityAfterPress` are a UI Automation search of the plug-in control's subtree for any element of the Menu type, with no visibility test: a Qt menu is seen while it is up, and so is any menu a plug-in keeps in its tree while hidden. A JUCE submenu is a window of its own as its menu is: `newWindow`, which placed the item, follows it, and the largest window new since its `chained` is the submenu.

Holding its place: a window is "in front" when it is the foreground window. A `#32768` menu and a menu bar's menus do not become it, so an overlay never needs to hold its place for them; a toolkit's popup that takes the foreground does — Komplete Kontrol's in REAPER (2026-09-26). That window is a full-screen one of `reaper.exe`, and after `Escape` Komplete Kontrol most likely only hides it: no foreground event came for about four seconds, which a destroyed window would most likely have caused — not measured. While a hidden window is the foreground window, [`host.window.foreground()`](window.md#host-window-foreground) answers it with `shown = false` — hidden, minimised and cloaked all count as not shown — and `host.window.active()` answers `nil` once the epoch has turned over; until then it can still answer the window it read while that was shown. The window of the press is brought back with `SetForegroundWindow`, whose `true` means Windows accepted the request: the change is made when REAPER's thread gets to it, with a foreground event, and it does not put the keyboard on any particular control. Where the keyboard is afterwards is REAPER's to decide: in the plug-in, the overlay is in front with its keys; on the FX window itself or its FX list, which the [chrome gate](#o-attachembedded) counts as REAPER's own, it is out until the keyboard reaches the plug-in. The process is the window's `app.pid`: a bridged plug-in's popup is a window of the bridge process, so it is held when the window in front at the press belongs to the bridge as well (REAPER's own bridge window), and not when the bridged plug-in is drawn inside the host's FX window — the overlay then leaves the front for it as it always did.

### macOS {#o-menutests-macos}

`nativePopup` counts the frontmost application's `AXMenuOpened` / `AXMenuClosed` notifications: a menu counts as open from its opening notification until its closing one, however long it stays up, and the count is cleared when another application comes to the front (see [`host.keys.nativeMenuOpen`](keys.md#host-keys-nativemenuopen)). Holding its place asks [`host.window.active()`](window.md#host-window-active) for the window in front, so it applies when a plug-in's popup becomes the application's focused window. A popup that does not become it leaves the overlay's window in front, and holding its place has no part in what happens then: the binding's own match decides — on the DAW's plug-in panel of [`attachEmbedded`](#o-attachembedded-macos), or a function's control, the accessibility focus on a menu item outside the panel takes the overlay out, as any element outside it does. `host.window.active()` answers nil while the frontmost application has no window (the Finder after a click on the desktop): a hold over a menu the tests still see stands through that, and ends once they stop seeing it. The window that gets the keyboard is the frontmost application's focused window, and [`host.window.foreground()`](window.md#host-window-foreground) reports it as shown unless it is minimised, so a focused menu window brings nothing back here. While the application does not answer, `foreground()` is `nil`, and nothing is brought back either: the runtime does not act on a state it could not read, and does not call `host.window.focus` on an application that is not answering. The reading itself is still taken once at each hold that ends with no test seeing the menu: two accessibility reads of the frontmost application, which against one that has just stopped answering can take up to the one-second timeout once, on the main thread. If the window of the press is brought back, `host.window.focus` raises it and sets its focus flag — at once while its application is the active one, otherwise once the workspace announces that application's activation — and `true` never means that the keyboard is in it (see [`host.window.focus`](window.md#host-window-focus)). An empty focus chain begins no stay on the panel and the function form of `attachEmbedded`, so after a menu's window that ended the stay the overlay is back once the chain names something inside the panel; with the plug-in's window in front all along, an empty chain keeps the stay and the overlay stays up. `newWindow` compares on-screen windows by owning process with no filter at all, so a tooltip or a window at any level counts as well; an `NSMenu` sits at window level 101. Its answer's `id` is the window server's `CGWindowID`, a numbering `host.window` ids are not in, so its `window` is `nil`; what is drawn over the menu is asked by that `id`, with [`host.window.ownsPoint`](window.md#host-window-ownspoint)'s `listed = true`. `accessibility` and `accessibilityAfterPress` are special-cased by the host to "is a menu open in this application", without descending the menu bar. An application the host has found not answering is left alone for 5 seconds, and in that time the answer is no: a menu only these tests see then counts as closed after two ticks, and as open again once the application answers. The one-miss rule does not cover that.

## O.menuTests.drawn(spec) {#o-menutests-drawn}

**Signature:** `O.menuTests.drawn(spec: { name: string?, region: (o: Overlay, press: Press) -> ({ x1, y1, x2, y2 }?, { w, h }?), minW: number?, minH: number? }) -> test`, with `Press` and `test` as [O.menuTests](#o-menutests) has them

Makes a [menu test](#o-menutests) for a popup **painted** inside the plug-in: VPS Avenger's in Logic, which JUCE draws as a child of the plug-in's editor — no window of the process comes, no notification is posted, and the plug-in publishes no element (the first Mac test of its page, 2026-10-14). At each press `region(o, press)` names the rectangle on screen that press's popup is painted in — corners, cut to whole pixels — and may answer, second, the smallest popup it takes there, `{ w, h }` in screen pixels (otherwise `minW` × `minH`, default 12 × 12); no rectangle arms nothing. Its `before` takes the rectangle's row and column [profile](screen.md#host-screen-profile) then, waited for: the picture before the click. On each tick after the press it profiles the rectangle again, **with a callback**, and compares the two in Luau — at most 256 rows and 256 columns, every line up to 256 and an even spread of that many beyond, never the pixels:

- a row or column has **changed** where its luminance mean differs by 6 or more or a channel's mean by 10 or more (its fill), or where only its darkest or lightest pixel does, by 40 or more (its extremes);
- the change's **span** runs from the first to the last line that changed either way; it is a popup when it is at least the smallest popup and **filled**: half the rows and half the columns inside it, at least, changed their fill. A ring drawn round something — VoiceOver's cursor — changes the extremes of every line it crosses, and the fill of none but its own, and is no popup;
- it is **seen** when the same span — each edge within 2 pixels — comes twice in a row and 90 % of the lines compared read alike in the two: the answer is `{ x, y, w, h }`, the span on screen, with **no `kind`**, so an item is chosen in it as in a menu drawn inside the plug-in ([`O:chooseMenuItem`](#o-choosemenuitem), step 4). A flash gone before its second reading and an animation that never reads alike are no menu;
- it is **still there** while both hold, of the lines whose fill changed at the opening: those that no longer read as they did at the sighting are fewer than a popup — under the smallest popup's height in rows, or under its width in columns, each compared line counting for the pixels between it and the next — **and** 70 % of them still read as they did. A row under the pointer, highlighted, passes both: one band of rows, fewer than the smallest popup's height (a menu of only two rows may fail the second). A popup that goes fails one of them: every line it covered changes back, a popup's worth however much else of the span reads as it did — whatever the press changed that outlasts the popup, such as a tab the same right click selected under the rectangle — and, where it differed from what lies under it in fewer lines than that, most of its own lines. While it is there the same rectangle is answered, so a click the menu did not take is clicked again — with `print`, how the lines whose fill changed at the opening read in that reading, their means rounded: a popup drawn exactly as at the item's click is the click still pending, not one the menu did not take (since 0.13; [step 6](#o-choosemenuitem));
- it is **gone** otherwise: the answer is `false`, and the test compares nothing more until the next press, so the plug-in redrawn under the popup — a new zoom, a new tab — is no new popup.

No timer decides any of it: readings are counted, and the one bound is the 8 seconds the tests are asked on every tick after a press — a `drawn` test that has seen nothing by then stops comparing (`no popup of '<label>' within 8000 ms`). It stops at `forget` and at the next press as well. It is not cheap: asked every tick for 8 seconds after a press and while the menu counts as open; when it is not comparing it answers `false` at once and reads nothing. **Cost:** one capture at the press, waited for — before the click, so a press takes one capture longer to click (about 16.7 ms through the standard path on Windows, 36–91 ms on a Mac); then one capture a tick while it compares, reduced on the image worker, and at most 1024 comparisons of a line's six numbers in Luau (its mean, its three channels' means, its darkest and its lightest pixel): 512 for its change from the press's picture, and 512 more for how alike it reads with the reading before or with the sighting. Its log lines start with `[menu] '<overlay>': drawn:` — `armed on '<label>' in x1,y1-x2,y2 (wxh), a popup at least wxh`, `nothing armed on '<label>' — <why>`, `a change of wxh at x,y at reading n, not alike yet` (or `not filled — no menu`, or `smaller than a popup … — no menu`; the first five of a press), `a popup wxh at x,y, alike at readings n-1 and n`, `the popup wxh at x,y has gone at reading n (c columns and r rows not as they were, a popup at least wxh; a of its l lines as they were)`, `nothing has changed in … after 20 readings of '<label>'` — which says no popup opened — and `a reading of '<label>' could not be taken: <why>` (a failed reading answers what the test said last); those of a submenu are under [Following a submenu](#o-menutests-submenu). It sees what is drawn in the rectangle whoever draws it: a window of another process put over the plug-in there is a popup to it as well, and the item's own check then refuses the click as covered, the host's hit test naming that window's application.

It follows a submenu of what it saw as well, with its `chained`: [Following a submenu](#o-menutests-submenu). Since version 0.8 of the runtime, and its `chained` since 0.10.

```luau
-- A plug-in whose preset list is painted below its preset field, at most 240 x 300 there: the
-- rectangle in the overlay's coordinates, placed through the origin and the scale at each press.
local LIST = { 40, 30, 280, 330 }
local presets = O.menuTests.drawn({
  name = "preset list",
  region = function(o, press)
    if press.label ~= "Preset" then return nil end -- another press opens no list here
    return o:toScreenRect(LIST, { whole = true }), { 120, 40 } -- the smallest list it takes
  end,
})
ov:attach(synth, { menus = { O.menuTests.newWindow, presets } })
```

### Returns {#o-menutests-drawn-returns}

A menu test: a table `{ name, cheap = false, test, before, pressed, forget, chained }`, to be listed in an overlay's `menus` as any test is. `name` is `tostring(spec.name)`, or `"drawn"`. Each call makes a test of its own, which keeps what a press armed for each overlay it serves; the same test may be listed for several overlays.

### Errors {#o-menutests-drawn-errors}

None. It never answers an error value. A reading that could not be taken is logged (`a reading of '<label>' could not be taken: <why>`), and the test answers what it said last.

**Raises** at the call, naming the caller's line: a `spec` that is not a table, or whose `region` is not a function (`O.menuTests.drawn takes { name?, region = function(o, press) -> { x1, y1, x2, y2 }?, minW?, minH? }`), and a `minW` or `minH` that is not a number above 0 (``O.menuTests.drawn: `minW` must be a number above 0, got 0``). Nothing else is checked: a key it does not have is not read. `region` runs guarded at each press, so one that raises arms nothing, logged `nothing armed on '<label>' — its region function raised: <message>`.

### Windows {#o-menutests-drawn-windows}

`drawn` reads the screen through the module's [source](screen.md#which-picture-a-read-sees), in device pixels: a JUCE popup that is a top-level window over the plug-in is in its picture too, so with `newWindow` on the same list both see it — `newWindow` on the tick it is listed, `drawn` two readings later — and the choice follows `newWindow`, which placed its item.

### macOS {#o-menutests-drawn-macos}

`drawn`'s pictures are in points, the downsampled values of a Retina display, so its thresholds are on averages that hardly move with the scale; without the Screen Recording permission every picture is the wallpaper, which never changes, and its log says `nothing has changed …` at the twentieth reading. VoiceOver's cursor is in its picture — a ring, which is not filled, and no popup. A submenu of a popup painted inside the plug-in — VPS Avenger's in Logic — is painted as well, and `drawn` follows it beside its menu (two pictures a tick, in points).

## O\:chooseMenuItem(spec) — choosing an item in a plug-in's menu {#o-choosemenuitem}

**Signature:** `O:chooseMenuItem(spec: { label: string?, at: {number, number} | ((overlay) -> {number, number}?), fromRight: boolean?, rawOrigin: boolean?, button: ("left" | "right" | "middle")?, menuItem: {number, number} | ((overlay, menu, factor) -> {number, number} | false | nil), submenu: { menuItem: {number, number} | ((overlay, menu, factor) -> {number, number} | false | nil) }?, retries: number?, onDone: ((overlay, chosen: boolean, err: Error?) -> ())? }) -> true | (false, Error)` · the control form: `menuItem` on [`addHotspotButton`](#o-addhotspotbutton), which takes no `submenu`

Opens a plug-in's own popup menu and chooses an item in it: clicks the opener at `at`, clicks the item once one of the overlay's [menu tests](#o-menutests) **sees** the menu and says where it is, and counts it as chosen once the menu is seen to have taken it. A JUCE plug-in's menus are popups it draws itself, with no item anybody can press through the accessibility layer — windows of their own, or painted inside the editor when the plug-in gives them a parent component: VPS Avenger's MENU, zoom list, redo list and right-click menus in Logic, which `newWindow` cannot see and `drawn` can. The obvious alternative, a hotspot with `points = { opener, item }`, does not work: every click of a sequence asks [`host.window.ownsPoint`](window.md#host-window-ownspoint) of the plug-in's window first, a popup is a window of its own over it, so the item's click is refused as covered; and the `settle` between the two would be a timer deciding that the menu is up by then.

The method is for a module's own code — a stepper whose step is a choice from a list, as Avenger's zoom is. A hotspot with `menuItem` does the same as a control: its `at` (with its `button`) is the opener, its `menuItem` the item, its `retries` the retries, and it has no `onDone` and no `submenu`. Since version 0.10.0 of the runtime the method takes `submenu`, an item that opens a submenu chosen in it (step 7): a module that uses it depends on `"com.platform.overlay >= 0.10"`.

`label` names the choice in what is said and logged; `"Menu item"` when it is not given.

What happens, in order:

1. **The opener** is placed like a hotspot's `at` — origin, frame, [scale](#o-scale), `fromRight`, `rawOrigin` — and checked like any hotspot's click (inside the origin, nothing else drawn there). One that cannot be placed — an `at` function that answers `nil`, or no factor — is **said**, `"<label> is not available now"`, unlike a hotspot's silent `nil`; one that is refused is said as the check says it. Either way nothing more happens: the method answers `false` and an error value — `unplaced`, or `outside` or `covered` by the check that refused — and `onDone(overlay, false, err)` hears the same one.
2. **The press.** Only then does it count as the press of a control with `opensMenu`: the tests run on every tick from here, `newWindow` takes the window list it compares against, a test's `before` reads what it needs (`drawn`'s picture of where the popup will be), and in a calibrating run the [menu shots](#o-calibrating) are taken; those reads are waited for before the click: an overlay that is no longer where the press was by then clicks nothing — the method answers `false, err` with [`O:stillHere`](#o-stillhere)'s code and message (`not_in_front`, `moved_on`), and `onDone` hears it — and the opener is checked again (refused: `outside` or `covered`). An item the overlay was still waiting to choose for **another** control is dropped (logged). The opener is clicked with `button` (left unless given), and the method returns `true`; the item is chosen later.
3. **The menu.** Each answer of a test that sees a menu is looked at, as it arrives, on the tick it arrives or in a test's callback. One that says where the menu is — a table with numbers `x`, `y`, `w`, `h`, as `newWindow` answers — places the item. A table `menuItem` is the item's offset from the menu's top-left corner, a **distance**: scaled at the factor the opener was placed with (the popup is drawn at the zoom it opened at, and a factor read off the plug-in's screen may not be readable while the popup covers it), never framed. A `menuItem` function is called with the overlay, that table and that factor (`nil` in an overlay without a scale), and answers the offset in **screen pixels** — it works from the menu's rectangle, which is in screen pixels, so its answer is not scaled again, as a `dragBy` function's is not; `false` says "this window is not my menu" (a tooltip that appeared first), and the next answer is looked at — a second value beside it, a few words or an error value, only goes into the log line that says so, once per window; `nil` is an item that cannot be placed, and a value beside it is not read. An answer that sees a menu without saying where — `nativePopup`, the accessibility tests — places nothing, and is logged once; another test on the list may still say where, and while an item waits every test on the list is asked, a cheap one that sees the menu holding back none.
4. **The checks.** The item must lie inside the menu's rectangle; if it does not, it is not clicked, the log says where it fell, and the next answer is looked at — a later answer can be the menu itself where this one was something else that appeared. Then what is drawn there: a menu that is a window is asked [`host.window.ownsPoint`](window.md#host-window-ownspoint) about *itself* — by its `host.window` id (`window`, which `newWindow` gives on Windows), or by its `id` in the window list's numbering with `listed = true` (macOS) — and a `false` refuses the click; a menu with no `kind` — a rectangle a module's own test answered, for a menu painted inside the plug-in — gets the check every click of the overlay gets.
5. **The click**, at once, in that same pass: no wait of any kind stands between the menu being seen and the item being clicked.
6. **Taken, or clicked again.** A menu does not always take a click that comes right after it opened: JUCE's popup ignores a mouse-up in its first moments (a quarter of a second in its source as it was read for this, not measured against any JUCE a plug-in here ships), and the first tick that lists a new window can come sooner. So the item keeps waiting after its click. An answer **of the test that placed the item** to a question asked on a **later** tick that still shows the same menu — the same window, at the same place and size — is a click the menu did not take: the item is checked again and clicked again, at most `retries` more times (default 3; a count of answers, not a time). Two things say otherwise first (since 0.13). **A popup drawn exactly as at the click** — `drawn`'s answer carries `print`, how the popup's lines read, and that is the same as in the answer the item was clicked on; or a reading of it that could not be taken — is the click **still pending**: the plug-in has stopped drawing while it acts on the click, as VPS Avenger does while it opens Logic's file panel (the second Mac test, 2026-10-24, where its item was clicked four times). It is waited for, never clicked again, up to the bound the press's look keeps (8 s from the opener's click); still drawn so at that bound, the click was not taken: `untaken`, as after the last retry. The log says `… — test 'drawn' sees the menu drawn as at the item's click: the click is waited for, not made again` once. Only a popup drawn again and still open — a row under the pointer highlighted — is a click not taken, and the click made again on it is the one a popup frozen from then on is compared with: a plug-in that took that click and then stopped drawing is not clicked once more for each reading. An answer without `print` (`newWindow`'s, a module's own) is followed by its rectangle, as before. And **a window of the plug-in's process over the item's point** that [`host.window.list`](window.md#host-window-list) did not list just before the click — a dialog the item opened, however soon — is the item **taken**, whatever is still drawn under it: `… chosen — clicked n time(s) at (x,y); a new window of the plug-in's process, '<title>' (id <id>), is over the item's point`. The process is the overlay's control's, or the window in front's; one listing just before the item's click, and one before each click that would be made again. An item clicked on an answer that came from a callback — between two ticks, as `drawn`'s do — counts as clicked on the next tick, so the reading asked right after the click, of a popup the plug-in has not closed yet, never clicks it again. Another test's answers say nothing about the click. The menu closing — for the menu tests, two answers in a row that do not see it — an answer that shows another window in its place, or the overlay leaving the front or moving on after the click, is the choice **taken**: `onDone(overlay, true)`. `retries = 0` is for a menu whose item keeps it open (a tick box in a menu), where a second click would be a second choice: chosen at the click, and not followed. The menu then closes the way any menu does for the menu tests, and the keys come back to the overlay with it.
7. **A submenu** (`submenu = { menuItem }`): the item is one that opens another popup — VPS Avenger's Copy Sample to, whose twelve slots are a submenu — and the choice is made in that popup.
   - The item is placed and checked as in steps 3 and 4, and **not clicked at once**: the test that placed it is told first through its `chained` hook ([Following a submenu](#o-menutests-submenu)), and the item is clicked once that answers `done(true)` — after the checks a click made after a wait gets: the overlay still where the press was, nothing drawn over the item. While the hook is getting ready, the tests' answers count for nothing. A hook that answers `done(false, why)`, raises or yields places nothing: `unplaced`, the item not clicked. A test with no hook goes on at once.
   - The item is clicked **once**. It counts as **taken** when the test that placed it answers a **new popup**: a rectangle that is not the menu's own nor inside it (each edge within two pixels). Its answers of the menu itself are waited through, and the item is **never clicked again**: a toolkit that ignored the click opens its submenu on the item's hover as well (JUCE does), and a second click while the submenu is drawing could close it and open it again under a reading. So the item has no retries; the bound ends it.
   - The bound starts again at the item's click: no new popup within 8 seconds of it, or the menu closing first, is `no_submenu`; new popups that the submenu's item could not be chosen in — `submenu.menuItem` answered `false` for each, or the item fell outside each — are `unfit_submenu`.
   - The submenu's item is then placed by `submenu.menuItem` in the new popup — a table at the opener's factor, a function handed the popup and that factor, as in step 3 — checked, clicked and followed as in steps 4 to 6, `retries` counting the submenu item's clicks. Place it from the **submenu's own corner**, as a table does, not from the item that opened it: a toolkit puts a submenu where it fits, as it does a menu. JUCE puts it beside its item with its top at the item's top when the item lies in the upper half of the area the menu may use — the plug-in, for a popup painted inside it, otherwise the display it is on — but with its **bottom** at the item's bottom in the lower half, so that it opens upward, and pushes it inside that area at its edges (`calculateWindowPos` in `juce_PopupMenu.cpp`, read in its source). A row counted from the item is right only in the first case. The submenu closing and **its menu left open** — the test that placed the item answering the menu alone, on a tick after the submenu's item was clicked — is the click **untaken**: a toolkit closes a menu and its submenus together on a choice (JUCE's `dismissMenu` walks up to the first menu), so a menu left means the click chose nothing.

A second call for the **same** `label` while its item is still waiting for its menu — a double Return, a held arrow on a stepper — clicks nothing: `"… pressed again while its menu is still being waited for — the opener is not clicked again; the first press stands"` in the log, `false, err` from the second call, `busy`, and `onDone(overlay, false, err)` for it. The first press is not disturbed: one of the overlay's own keys that is such a repeat does not count as the user moving on either.

A choice that fails after the press clicks nothing more: the item is dropped, `onDone(overlay, false, err)` is called with an error value, what the user hears is said, and the log says `[menu item] '<overlay>': '<label>' not chosen — <its message>`. One that fails at the call answers `false, err`, and `onDone` hears the same error value. Every one is under Errors, with what is said.

A menu that stays open after its item was refused keeps the keys, as any open menu does: `Escape` goes to it, and a module that knows how its plug-in's popup closes can close it in `onDone` — [`O:menuOpen`](#o-menuopen) says whether a menu still counts as open. A menu that appears after the item was dropped is an ordinary menu: nothing is chosen in it.

The log says every step: `[menu item] '<overlay>': '<label>' opening its menu, clicking (x,y)`; `… clicked its opener — the item is chosen when a menu test sees the menu, for up to 8000 ms`; `… — test 'newWindow' sees the menu at 362,80 120x200 (window id 300) (its window id 300 is asked what is drawn there); choosing item (10,20) at (372,100)`; `… — test 'newWindow' still sees the menu at … a tick after the item's click, which it did not take: clicking (372,100) again, 2 of 4`; `… chosen — clicked 1 time(s) at (372,100); the menu has closed`; or a `not chosen` line from the table. With a `submenu`: `… item (10,20) at (372,100) opens a submenu — the test is told first`, `… clicking item (10,20) at (372,100) once (…); its submenu is chosen in once test '<name>' sees a new popup, for up to 8000 ms`, `… test '<name>' sees a new popup at x,y wxh after the item's click: the item is taken, and its submenu is chosen in`, `… choosing the submenu's item (dx,dy) at (x,y) (…)`. In a calibrating run, a press that was photographed also writes the **menu item shot** — the menu with a crosshair on the item, taken before its first click, which waits for it (see [`O.calibrating`](#o-calibrating)), and with a `submenu` the submenu's as `<overlay>-<control>-submenu-item.png` before its first click — and the calibration shot marks the opener of every label this method has been called with, at the `at` it was last given, as `[menu opener]`, after the overlay's controls.

**Cost.** The opener's placement and click, one origin resolution at the press; per answer that sees a menu, the item's placement and one `ownsPoint`; per retry, the same again and one click; a calibrating run adds one capture before the item's first click. Nothing is asked of the screen that the menu tests were not asking anyway — but for a `submenu`, what the test that follows it takes ([Following a submenu](#o-menutests-submenu)): `newWindow` one window list, `drawn` two captures before the item's click and two a tick while it follows.

```luau
-- A stepper over a plug-in's zoom list: each step opens the list and chooses the entry beside the
-- current one; the stepper then reads the zoom field again and says what it became. `zoomIndex`
-- and `readZoomText` are the module's own; the numbers are illustrative.
local ENTRIES, PITCH = 21, 11.5   -- rows in the list, 11.5 apart at 50 %
ov:addStepper({
  label = "Zoom",
  settle = 3000,                                  -- at most this long for the new zoom to show
  text = function(o) return readZoomText(o) end,
  onStep = function(dir, o)
    local i = zoomIndex(o)                        -- which of the 21 entries is the current zoom
    if not i or not (i + dir >= 1 and i + dir <= ENTRIES) then return end  -- nothing beside it
    local opened = o:chooseMenuItem({
      label = "Zoom",
      at = { 21, 14 },
      -- The list is drawn at the zoom it opened at, so its rows are PITCH times the factor the
      -- zoom field was clicked with; the answer is in screen pixels from the list's corner.
      menuItem = function(_, menu, k)
        if not k then return nil end
        return { 12 * k, (6 + PITCH * (i - 1 + dir)) * k }
      end,
    })
    if not opened then return false end             -- a repeat, or refused and said: nothing to watch
  end,
})

-- A drum slot's menu, opened with the right button, and its Copy Sample to: a submenu of the twelve
-- slots, the target chosen in it. Points from Avenger's corner at 50 %, as the module's tables have
-- them; `row` places an item from the corner, inside the popup it is handed. The submenu opens
-- upward from an item low in the plug-in, so `slot` counts its rows from the submenu's own top:
-- the first row's middle half a row below it, the rows 11.5 apart.
local function row(item)
  return function(o, menu, k)
    local x, y = o:toScreen(item[1], item[2])
    if not (x and k) then return nil end
    if y < menu.y or y >= menu.y + menu.h then return false, "its rows do not reach the item's" end
    return { math.min(menu.w - 1, math.max(0, x - menu.x)), y - menu.y }
  end
end
local function slot(n)
  return function(o, menu, k)
    local x = o:toScreen(393, 0)
    if not (x and k) then return nil end
    local dy = (5.75 + 11.5 * (n - 1)) * k
    if dy >= menu.h then return false, "too short for the slot's row" end
    return { math.min(menu.w - 1, math.max(0, x - menu.x)), dy }
  end
end
o:chooseMenuItem({
  label = "A drum slot's menu",
  at = { 282, 64.5 }, button = "right",              -- slot 2's name
  menuItem = row({ 323, 130.5 }),                    -- Copy Sample to
  submenu = { menuItem = slot(5) },                  -- slot 5
  retries = 3,                                       -- the submenu item's; the item is clicked once
  onDone = function(_, chosen, err)
    if not chosen and (err.code == O.errors.NO_SUBMENU or err.code == O.errors.UNFIT_SUBMENU) and o:menuOpen() then
      host.input.send("Escape")                      -- the menu, and any submenu, still up
    end
  end,
})
```

### Returns {#o-choosemenuitem-returns}

- `true` — the opener was placed, checked and clicked; the item is chosen later. `onDone(overlay, chosen, err)` is called once, when the choice is over: `chosen = true` and no error value once the menu took the item, or `chosen = false` and an error value.
- `false, err` — nothing was clicked. `onDone(overlay, false, err)` has heard the same error value already, inside the call, before it returned.

`onDone` is called guarded: one that raises is logged, `[menu item] '<overlay>': '<label>' onDone failed: <message>` after the press.

### Errors {#o-choosemenuitem-errors}

None of these is raised. Each comes as an error value where "arrives" says. At the call, `return`: `onDone` hears the same one. After the press, `hook onDone` only. "Said" is what the user hears; where nothing is said, the user moved on or the overlay was not in front, and a key says what it does.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | At the call, the overlay is not in front. Nothing is said. | `the overlay is not in front` | both |
| `busy` | return | A repeat of the same `label` while its item still waits for its menu — a double Return, a held arrow on a stepper. The first press stands. Nothing is said. | `pressed again while its menu is still being waited for — the opener is not clicked again; the first press stands` | both |
| `unplaced` | return | The opener cannot be placed: its `at` function answered `nil`, or the scale has no factor (step 1). Said: `"<label> is not available now"`. | `'…' cannot be placed now — …; nothing clicked` | both |
| `outside` | return | The opener lies outside the origin's window — at the call, or again after a picture before the press. Said: `"<label> is not where it should be"`. | `'…' resolves to (…,…), outside its origin's window (…,…) …x… — not clicking` | both |
| `covered` | return | Another window is drawn over the opener, likewise, or — on Windows — the window is gone. Said: `"something else is covering <label>, or its window is gone"`. | `'…' at (…,…) is inside our window's frame but another window is drawn there, or the window is gone — not clicking` | both |
| `not_in_front` | return | The overlay is no longer active once a picture before the press is in (step 2). Nothing is said. | `the overlay is no longer active` | both |
| `moved_on` | return | Likewise, the overlay left the front and came back, or is on another window. Nothing is said. | `the overlay left the front and came back since`, `the overlay is on another window now` | both |
| `no_menu` | hook onDone | No test saw a menu within 8 seconds of the press — the time the tests run on every tick after a press, a bound on how long a press waits for its menu, not a guess that it is up. Said: `"<label>: no menu was seen, nothing was chosen"`. | `no menu test saw a menu within … ms of the press` | both |
| `no_menu` | hook onDone | At the bound, the menu was seen only by tests that do not say where it is. Said: `"<label>: the menu does not say where it is, nothing was chosen"`. | `test '…' sees a menu but does not say where it is, and no test that does saw it` | both |
| `closed` | hook onDone | The same, when the menu closes before the bound. Said likewise. | `test '…' sees a menu but does not say where it is, and no test that does saw it` | both |
| `no_menu` | hook onDone | At the bound, the `menuItem` function took no window seen for its menu (it answered `false` for each). Said: `"<label>: no menu it can be chosen in was seen, nothing was chosen"`. | `its menuItem function took no window seen for its menu (the last at …)` | both |
| `closed` | hook onDone | The same, when the menu closes before the bound. Said likewise. | `its menuItem function took no window seen for its menu (the last at …)` | both |
| `no_menu` | hook onDone | At the bound, the item fell outside every menu seen; the message gives where it fell, `(x,y) of x,y wxh`. Said: `"<label> is not where it should be"`. | `the item, at …, is outside the menu that was seen` | both |
| `closed` | hook onDone | The same, when the menu closes before the bound. Said likewise. | `the item, at …, is outside the menu that was seen` | both |
| `closed` | hook onDone | The menu closed before a test placed the item, and none of the above was seen. Said: `"<label>: the menu closed before anything was chosen"`. | `the menu closed before a test placed the item` | both |
| `unplaced` | hook onDone | The item cannot be placed: `menuItem` (or `submenu.menuItem`) raised or answered `nil` or no `{dx, dy}`, no factor was known at the press, or a `chained` hook could not follow its submenu (`test '<name>' cannot follow its submenu: <why>`). Said: `"<label> is not available now"`. | `the item cannot be placed now: …` | both |
| `covered` | hook onDone | Another window is drawn over the item, at its click or a retry, asked of the menu's own window by its `host.window` id — the `window` of `newWindow`'s answer on Windows — or, on Windows, that window is gone. Said: `"something else is covering <label>, or its window is gone"`. | `(…,…) is inside the menu's window id …, but another window is drawn there, or that window is gone` | both |
| `covered` | hook onDone | The same, asked by the menu's `id` in the window list's numbering, for an answer of `kind = "window"` with no `window` — `newWindow`'s on a Mac. Said likewise. | `(…,…) is inside the menu's window … of the window list, but another window is drawn there, or that window is gone` | both |
| `outside` | hook onDone | A menu drawn inside the plug-in (an answer with no `kind`): the overlay's own check found the item outside the origin's window. Said as that check says it, and logged `not chosen — refused, and said`. | `'…' resolves to (…,…), outside its origin's window (…,…) …x… — not clicking` | both |
| `covered` | hook onDone | The same, another window drawn over the item, or the window gone. | `'…' at (…,…) is inside our window's frame but another window is drawn there, or the window is gone — not clicking` | both |
| `refused` | hook onDone | The menu was still there after the last retry: it did not take the choice — or, since 0.13, a popup `drawn` sees was still drawn as at the item's click at the bound (a click pending that never came to anything). Said: `"<label>: the menu did not take the choice"`. | `the menu was still there after … click(s) on the item at (…,…)` | both |
| `refused` | hook onDone | With a `submenu`: the submenu's menu answered alone after the submenu's item was clicked (step 7). Said likewise. | `the submenu has closed and its parent menu is still there after … click(s) on the item at (…,…)` | both |
| `no_submenu` | hook onDone | With a `submenu`: no new popup within 8 seconds of the item's click. Said: `"<label>: its submenu did not open, nothing was chosen in it"`. | `no submenu was seen within … ms of the item's click` | both |
| `no_submenu` | hook onDone | With a `submenu`: the menu closed after the item's click, before a new popup was seen. Said likewise. | `the menu closed after the item's click, and no submenu was seen` | both |
| `unfit_submenu` | hook onDone | With a `submenu`: new popups were seen, and `submenu.menuItem` answered `false` for every one, by the bound or the menu's close. Said: `"<label>: no submenu it can be chosen in was seen, nothing was chosen in it"`. | `its submenu's menuItem function took no popup seen after the item's click (the last at …)` | both |
| `unfit_submenu` | hook onDone | With a `submenu`: new popups were seen, and the submenu's item fell outside every one. Said: `"<label> is not where it should be in its submenu, nothing was chosen in it"`. | `the submenu's item, at …, is outside the submenu that was seen` | both |
| `not_ready` | hook onDone | With a `submenu`: the bound came while the test that placed the item was still getting ready to follow it (`chained`). Said: `"<label>: its submenu cannot be followed now, nothing was chosen"`. | `the bound came while its test was getting ready to follow its submenu; nothing was clicked in it` | both |
| `closed` | hook onDone | With a `submenu`: the menu closed while that test was getting ready. Said: `"<label>: the menu closed before anything was chosen"`. | `the menu closed while its test was getting ready to follow its submenu; nothing was clicked in it` | both |
| `moved_on` | hook onDone | One of the overlay's own keys arrived with no menu open, before the item was clicked. Nothing is said. | `one of the overlay's own keys came first, with no menu open` | both |
| `moved_on` | hook onDone | The overlay left the front before the item was clicked. Nothing is said. | `the overlay left the front first` | both |
| `moved_on` | hook onDone | Another control's press came first: a new press drops the item the last one waited to choose. Nothing is said. | `another press came first` | both |
| `moved_on` | hook onDone | The overlay is on another window by the time the menu is seen or the 8 seconds are over. Nothing is said. | `the overlay is on another window now` | both |

Once the item has been clicked, every other ending — the menu closed, another window in its place, the overlay leaving the front or moving on — is the choice taken: `onDone(overlay, true)`, with no error value.

**Raises** for a mistake in the call, before anything is clicked: a `spec` that is not a table, has no `at` of `{x, y}` or a function, or no `menuItem` (``overlay '<o>': chooseMenuItem takes { label, at, menuItem, … }, with `at` {x, y} or a function(overlay)``); a `menuItem` that is neither `{dx, dy}` nor a function; a `submenu` that is not `{ menuItem = … }` with such a `menuItem` (``overlay '<o>': chooseMenuItem's `submenu` must be { menuItem = {dx, dy} or a function(overlay, menu, factor) }, got …``); a `button` other than `"left"`, `"right"` or `"middle"`; and a `retries` that is not a whole number from 0 — a mistake that would otherwise show only after the menu had opened. It raises too on an overlay **with no menu tests** — ``overlay '<o>': chooseMenuItem needs menu tests to see the menu with — `menus` on its binding`` — in front or not, since its declaration decides, not the moment: no menu could ever be seen. Before version 0.11 of the runtime it answered `false` there and said `"<label> is not available now"`. An `at` function is called unguarded: what it raises comes out of the call.

### Windows {#o-choosemenuitem-windows}

A popup drawn as a top-level window of the plug-in's process (for a plug-in in a DAW, the DAW's process, unless the DAW bridges it) is what `newWindow` sees; its answer's `window` is that window's HWND, and the item is checked with `ownsPoint` against it — the window manager's hit test, taken up to the top-level window on both sides, which sees through a window that lets clicks pass. A toolkit can draw a popup's drop shadow as separate windows beside it, which is why the largest new window is taken as the menu. A submenu of such a popup is another window of the same process: `newWindow` takes the largest window new since its `chained` as the submenu (step 7).

### macOS {#o-choosemenuitem-macos}

`newWindow` lists windows as the window server numbers them (`CGWindowID`), which is not the numbering of `host.window` ids, so its answer has no `window`, and the item is checked by the menu's `id` with `ownsPoint`'s `listed = true`: the window server's own hit test, with `nil` — no answer, which lets the click go — where it names no window, a window of VoiceOver's own, or one of this application's. The menu's rectangle and the click are both in points. A tooltip under the pointer is a window of the process as well, and nothing filters it: the largest new window is the menu, and an item that falls outside a smaller one that came first waits for the next answer (step 4). A popup painted inside the plug-in — VPS Avenger's in Logic, where no window comes for it — is `drawn`'s, a rectangle in points with no `kind`, and its item is checked against the plug-in's own window as any click of the overlay is: the plug-in's window answers for every point of it. Its submenu is painted too: `drawn` follows it, the submenu's item checked the same way.

## O\:menuOpen() {#o-menuopen}

**Signature:** `O:menuOpen() -> boolean`

Whether a menu counts as open over the overlay now: one of its [menu tests](#o-menutests)' word is that one is. `false` for an overlay with no menu tests, and while it is not in front. For a module's own code that tidies up after a menu of its own — closing a popup a choice left open by clicking its opener again, or with Escape — which must not open one that has gone, nor send a key to the plug-in once it has. A table lookup.

```luau
-- VPS Avenger (modules/vps-avenger, shortened): a choice that could not be made leaves the popup
-- open; the zoom field closes the zoom list, as a second click on it does in the plug-in.
onDone = function(o, chosen, err)
  if not chosen and err.code == O.errors.UNPLACED and o:menuOpen() then
    local x, y = o:toScreen(21, 14)
    if x then host.input.click(x, y) end
  end
end,
```

### Returns {#o-menuopen-returns}

`true` while a menu counts as open over the overlay — one of its tests' word is that one is; `false` otherwise: always with no menu tests, and from the moment the overlay leaves the front, which drops every word, until a test sees a menu again.

### Errors {#o-menuopen-errors}

None. It never answers an error value.

It never raises.

## O\:afterDialog(fn) {#o-afterdialog}

**Signature:** `O:afterDialog(fn: (overlay, seen: boolean) -> ()) -> true | (false, Error)`

"The dialog that comes next, then the plug-in again, then `fn`" — for a press of the overlay's own, outside any trip of its page, as [`trip:afterDialog`](#trip-afterdialog) is for a trip: an item of the plug-in's menu that opens a file dialog the user works in, and the module that says what it did once the plug-in is back. VPS Avenger's ring chooses Load preset in its MENU, the user loads a preset in the DAW's open panel, and the ring says the preset's name once Avenger is in front again. Since version 0.14 of the runtime; a module that calls it depends on `"com.platform.overlay >= 0.14"`.

- **Called while the overlay is in front** — in a control's `onActivate` before the click that opens the dialog, or in [`O:chooseMenuItem`](#o-choosemenuitem)'s `onDone` once the item was taken. It notes the overlay's own window and its application: the window in front, or the window the press was made in where `onDone` comes while the overlay holds its place over the press's menu or over the dialog that covered the item, or as the overlay leaves the front for that dialog. With `retries` at its default, `onDone(true)` comes in any of these ways, and the call notes the same window in each.
- **The dialog** is the first window of that application, other than the window noted, that is in front at the call or comes to the front from then on — whether the overlay [holds its place](#o-menutests) over it or leaves the front for it — unless it is the window the overlay held its place over for its press's menu **before** an item of that menu was clicked, or the window in front when the item was clicked, which is the menu, not a dialog. It is decided at each recheck of the overlay, which every window and focus event asks. Once it was seen, **nothing bounds the wait**: the user's time in a file dialog is theirs.
- **`fn(overlay, true)`** runs once the overlay is in front again on the window noted, not away and holding its place over nothing — the hold over the dialog ended with the plug-in's window in front, or an ordinary activation there after the overlay left the front for it — on the module's next turn, as a handler, where a read may wait, and the overlay is asked again there: in front on that window still, or the look waits for the next recheck.
- **`fn(overlay, false)`** runs when no dialog came within **8 seconds** of the call — a bound on looking, as for a press's menu, never a guess that it has come — whether the overlay is in front then or not; `o.active` says. The module may read all the same: an item that opened nothing still did what it did.
- **Dropped**, `fn` never called, with a line in the log: when the overlay comes to the front on another window after the dialog was seen, and when another `afterDialog` call of the overlay begins a wait. A disable stops the module's timers; a reload ends the VM, and no `fn` runs.
- Nothing is said. The log says `[menu] '<overlay>': waits for the dialog that comes next — a window of pid N other than '<title>' (<exe>) — for up to 8000 ms`, `… a dialog came forward — '<title>' (<exe>), window id X of pid N, M ms after the wait began`, `… '<title>' (<exe>) is in front again after the dialog, M ms after the wait began`, `… no dialog came within 8000 ms`, `… in front on '<title>' (<exe>) after the dialog, not on '<title>' (<exe>) — the wait for it is dropped` and `… the wait for a dialog since M ms ago is dropped — another begins`.

**Cost.** One timer for the bound, and, while a wait stands, one [`host.window.active()`](window.md#host-window-active) at the call and at each recheck of the overlay — memoised per [epoch](timer.md#host-epoch). Nothing is read of the screen; what `fn` reads is the module's.

```luau
-- In a choice's onDone: the plug-in's Load item opens the DAW's file panel; once the plug-in is
-- back, its preset's name is read and said. NAME is the name's rectangle, in the overlay's coordinates.
onDone = function(o, chosen)
  if not chosen then return end
  local ok, err = o:afterDialog(function(o2, seen)
    if not o2.active then return end
    local r = o2:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
    if not r then return end
    host.ocr.recognize(r, {}, function(reading)
      if reading.status ~= "text" then return end
      host.speech.output((seen and "Loaded " or "No dialog came; the preset is ") .. reading.text)
    end)
  end)
  if not ok then host.log.info("not waited for: " .. tostring(err)) end
end,
```

### Returns {#o-afterdialog-returns}

- `true` — the wait has begun; `fn` is called once, `true` after the dialog or `false` at the bound, unless the wait is dropped (above).
- `false, err` — nothing waits, and `fn` is never called: see Errors.

### Errors {#o-afterdialog-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not in front — nor leaving the front in a choice's `onDone` — its group is away (its page in front, a trip under way), or no window of an application is in front to note. | `the overlay is not in front` | both |

**Raises** for a mistake in the call, naming the caller's line: an `fn` that is not a function (`overlay '<label>': afterDialog takes a function(overlay, seen), got <type>`). `fn` runs guarded: one that raises is logged, `[menu] '<overlay>': afterDialog's fn failed: <message>`.

### Windows {#o-afterdialog-windows}

The window noted is a top-level window — the plug-in's FX window in front, or the one the press was made in, never its child window the overlay's origin is — and the dialog a top-level window of the same process: a DAW's file dialog is the DAW's. A modal dialog that a DAW opens for a plug-in in a bridge process belongs to the bridge, and is no window of the application noted; that has not been met.

### macOS {#o-afterdialog-macos}

The window noted is the plug-in's window, which [`host.window.active()`](window.md#host-window-active) answers as the frontmost application's focused window, and the dialog another window of that application: Logic's "Load Preset..." panel is Logic's. A sandboxed DAW's open panel can belong to another process (`com.apple.appkit.xpc.openAndSavePanelService`), which is then no window of the application noted, and no dialog is seen: `fn(overlay, false)` at the bound, the module reading all the same. The second Mac test's log names Logic's panel as a window of Logic's own.

## O\:resume(on) {#o-resume}

**Signature:** `O:resume(on: boolean) -> Overlay`

Whether the overlay, coming to the front again on the window it was last in front on, resumes on the control the user was on (`true`, the default: `Alt+Tab` out and back should not throw them back to the start), or starts on its first control as it does on a new window (`false`). For a box drawn **inside** a plug-in's window, whose origin is that window every time it appears: VPS Avenger's warning box comes up for Initialize and again for Save, and resuming would put the second on the button the first was answered with — a user who counts stops from the box's text would land on the other button. Called before the overlay is bound; afterwards it raises. A second call replaces the first.

```luau
local box = O.new("Warning box")
box:addStaticText({ label = "Avenger asks", text = boxText })
box:addHotspotButton({ label = "Yes", rawOrigin = true, at = yesPoint })
box:addHotspotButton({ label = "No", rawOrigin = true, at = noPoint })
box:resume(false)   -- every box starts on its own text
box:bind(BINDING, { specificity = O.layer.dialog })
```

### Returns {#o-resume-returns}

The overlay itself, so calls chain. Anything but `false` is taken as `true`.

### Errors {#o-resume-errors}

None. It never answers an error value.

**Raises** for a call after the overlay is bound: `overlay '<label>': resume must be called BEFORE binding it — the binding is what starts it`.

## O\:gate(fn) {#o-gate}

**Signature:** `O:gate(fn: (origin) -> boolean) -> Overlay`

Sets an extra activation condition ANDed onto the context match: `fn(origin)` — `origin` the active context's coordinate window or control, as [`O:origin()`](#o-origin) answers it — says whether the overlay should be active, and only an answer of exactly `true` counts. A derived or library overlay uses one to take over from its base only when what it is for is on screen; [`O:landmark`](#o-landmark) is the common case, an image. It is asked at every evaluation after the context matched, except while the overlay holds its place over its own menu's window, in the handler of whatever looked at the overlay again ([Where a hook runs](#where-a-hook-runs)).

On an overlay bound to an arbiter slot, the gate's answer is what the slot is decided on: context match AND gate is reported to the arbiter as this claim's `matching`, and specificity only ranks the claims that are matching. So a dialog overlay takes the slot exactly while its gate says the dialog is there — see [host.arbiter](arbiter#matches).

**A gate whose condition can change without a window event needs `pollMatch`.** Gates are otherwise re-evaluated only when a window is activated or focused, which is enough for a dialog — opening and closing one *is* a window event — and not enough for anything that appears and disappears *inside* a window that never changes. ON:EAR's chooser panels are exactly that, and without the poll the overlay kept the arbiter slot after its panel had closed: a ring for a panel that was no longer on screen, which somebody who cannot see it has no way to escape. See `pollMatch` under [`O:attach`](#o-attach).

One gate per overlay: a second call replaces the first, and so does a landmark, which is a gate. Called before the overlay is bound.

```luau
-- A gate: sforzando inside REAPER is active only while the keyboard is in the PLUG-IN.
-- REAPER's own chrome answers with a focus chain several elements deep; the plug-in
-- exposes nothing, so focus reaches the window and stops. Depth is the discriminator.
inDaw:gate(function()
  local chain = host.window.focusChain()
  return not chain or #chain <= 1
end)
```

### Returns {#o-gate-returns}

The overlay itself, so calls chain.

### Errors {#o-gate-errors}

None. It never answers an error value. A gate that says no is logged where the overlay leaves the front: `its gate says no`.

**Raises** for a call after the overlay is bound: `overlay '<label>': gate must be called BEFORE binding it — the binding is what starts it`. `fn` is not checked at the call, and is called unguarded: what it raises ends the evaluation that asked it, and the handler it ran in, and the host logs it.

## O\:landmark(image, opts?) {#o-landmark}

**Signature:** `O:landmark(image: string, opts: { anchor: boolean? }?) -> Overlay`

A [gate](#o-gate) satisfied only while `image` — an **absolute** path, via [`host.path`](path.md#host-path) — is found within the active context's region. A derived or library overlay uses one to take over from its base only when its product wordmark is on screen. The search is made with a callback, off the event loop: the gate answers the last verdict and asks for a new one, one search out at a time, and a verdict that changes looks at the overlay again at once. Once found, only the last hit and 6 pixels round it are searched, and a miss there widens the next search to the whole region; while a claim of at least the overlay's rank holds its slot, it searches nothing and answers its last verdict.

**Coordinate anchoring (opt-in):** pass `landmark(image, { anchor = true })` and, while the landmark is on screen, the overlay's control coordinates resolve **relative to the landmark's top-left** instead of the plugin's client area (a `rawOrigin` control still uses the client). So an overlay whose landmark sits *in the same UI panel as its controls* (a library's wordmark above its mixer) stays correctly positioned regardless of host, window size, or plugin chrome/browser state — the landmark and the controls move together. Author such controls relative to the wordmark (coordinates may be negative if a control sits above/left of it). WITHOUT `anchor`, `landmark` is a pure gate and controls stay client-relative — the right choice when the image is used only to tell the overlay apart from a sibling on a shared slot (a dialog's header image).

It replaces a gate set before it, and a gate set after it replaces the landmark. Its log lines start with `[landmark] <overlay>:` — a hit and the region searched, a search that could not be made (`could not look (<message>); the last verdict (…) stands …`), and a near miss: a full search that missed, made again at three times the tolerance and hit there, which says the template has drifted.

```luau
-- A landmark, anchoring: this library's controls are authored relative to its wordmark,
-- so they follow it wherever Kontakt draws the panel.
ov:landmark(host.path("images/MimiPage/Wordmark.png"), { anchor = true })
```

### Returns {#o-landmark-returns}

The overlay itself, so calls chain.

### Errors {#o-landmark-errors}

None. It never answers an error value. A search that could not be made keeps the last verdict, and says so once per spell in the log.

**Raises** for a call after the overlay is bound: `overlay '<label>': landmark must be called BEFORE binding it — the binding is what starts it`. A path that is not absolute is not raised: it is reported when the overlay binds (`landmark image is not an absolute path — use host.path()`).

## O\:typingWhen(fn) {#o-typingwhen}

**Signature:** `O:typingWhen(fn: (overlay) -> boolean) -> Overlay`

While `fn` answers exactly `true`, the overlay **holds no keys at all** — not the navigation keys, not Space, not Return. It is asked, with the overlay, every time the overlay works out which keys it holds: at every focus move, as it comes to the front, and at every evaluation while it is in front — a window or focus event, a `pollMatch` tick.

Letting named keys through one at a time patches a hole whose shape is not known: any key nobody thought of stays swallowed, and a swallowed key in a text box is indistinguishable, from the keyboard, from the application having frozen. The condition is supplied by the module because only the module can tell — ON:EAR's answer is that its accessibility tree goes dark exactly while the caret is in its search box, which its gate is measuring anyway.

The way back is the application's own: Tab moves focus out of its editor, the condition goes false on the next check, and the keys come back. Nothing here can lock, because while it is on there is nothing left to lock with; the worst it can do is go inert, which announces itself the moment Tab does not move the ring.

```luau
-- ON:EAR's accessibility tree goes dark exactly while the caret is in a text field, and the
-- gate is measuring that anyway -- so the same flag decides whether to hold any keys at all.
local settingsDark = false
settings:gate(function(origin)
  if host.element.locate(origin.id, "Global Settings", host.element.type.Text) then
    settingsDark = false
    return true
  end
  settingsDark = true   -- nothing answers: the device-name field has the caret
  return true           -- absence of an answer is not an answer, so the last one stands
end)
settings:typingWhen(function() return settingsDark end)
```

### Returns {#o-typingwhen-returns}

The overlay itself, so calls chain. A second call replaces the first; `nil` takes the condition away.

### Errors {#o-typingwhen-errors}

None. It never answers an error value. Each change is logged: `<overlay>: a text field has the keyboard — holding no keys until it gives it back (<n> key(s) handed back)`, and `… the text field gave the keyboard back — navigation keys taken again (…)`.

It never raises: it may be called before or after the overlay is bound, and `fn` runs guarded — one that raises counts as not typing.

## O\:onActivate(fn) {#o-onactivate}

**Signature:** `O:onActivate(fn: (overlay) -> ()) -> Overlay`

A hook the module runs each time the overlay comes to the front; [`O:onDeactivate`](#o-ondeactivate) is its pair. It may not wait for a read ([Where a hook runs](#where-a-hook-runs)): an overlay on a `slot` runs it in the arbiter's call, a plain call of the host's where nothing can wait — `host.ocr.recognize` without a callback raises there — and any overlay is half way into the front while it runs. A hook that needs a read starts it with a callback. It runs after the overlay has taken its keys and before its first control is announced (350 ms later), so the origin is known and a hook can prepare what that announcement reads. Set before the overlay is bound; one per overlay, a second call replacing the first.

"Comes to the front" is the overlay's activation, not a window event: an overlay that holds its place over its own menu stays active, and comes to the front again only after it has left it — so a hook that forgets a per-window reading here forgets it at every return to the plug-in, Alt+Tab included.

**Paired, and held while away.** The two alternate: `onActivate` is called only for an overlay the module has not been told is active, `onDeactivate` only for one it has. While the overlay's group is [away](#o-away) — its page in front, or a trip of its page under way — neither is called: the overlay leaves the front for the page, comes up quietly in a trip, gives the slot to a sibling box and takes it back, and the module hears nothing. On the turn after away ends they are settled: an overlay the module was told is active that is not in front gets `onDeactivate`, and one in front it was not told of — a sibling that came up during a trip and is still up — gets `onActivate`. So a page round trip, a trip, and a box during one are invisible to these hooks; the user leaving the page for another application is one `onDeactivate`, at that settle. A module disabled while away gets its deferred `onDeactivate` at the settle after the first event once it is enabled again.

```luau
-- A module that reads the plug-in's zoom once per stay in front forgets it on every return.
ov:onActivate(function(o) o.state.zoom = nil end)
```

### Returns {#o-onactivate-returns}

The overlay itself, so calls chain.

### Errors {#o-onactivate-errors}

None. It never answers an error value. The hook is called guarded: one that raises is logged, `[overlay] <label>: onActivate failed: …`, and the overlay comes up all the same.

**Raises** for a call after the overlay is bound: `overlay '<label>': onActivate must be called BEFORE binding it — the binding is what starts it`. `fn` is not checked at the call.

## O\:onDeactivate(fn) {#o-ondeactivate}

**Signature:** `O:onDeactivate(fn: (overlay) -> ()) -> Overlay`

A hook the module runs each time the overlay leaves the front, the pair of [`O:onActivate`](#o-onactivate), with the same rules: it may not wait for a read, it is not called while the overlay's group is away, and the two alternate. It runs after the keys have been handed back and the menu state dropped. Set before the overlay is bound; one per overlay, a second call replacing the first.

```luau
-- A scanning dialog says when a scan it committed to has gone (modules/komplete-kontrol does
-- this, keeping the flag in a field of its own; a module's state belongs in `o.state`).
scan:onDeactivate(function(o)
  if o.state.scanCommitted then host.speech.output("Scanning stopped", { interrupt = true }) end
  o.state.scanCommitted = false
end)
```

### Returns {#o-ondeactivate-returns}

The overlay itself, so calls chain.

### Errors {#o-ondeactivate-errors}

None. It never answers an error value.

**Raises** for a call after the overlay is bound: `overlay '<label>': onDeactivate must be called BEFORE binding it — the binding is what starts it`. The hook itself is **not** guarded where the overlay leaves the front: what it raises leaves the overlay's deactivation into the window event that caused it — the overlay is inactive and its keys are released by then — and the host logs it there. At the settle after away it is guarded, logged `[overlay] <label>: onDeactivate failed: <message>`.

## O\:addPage(spec) — an overlay's page {#o-addpage}

**Signature:** `O:addPage(spec: { describe: (on) -> Page, key: string, open: (overlay, answer: ((false?) -> ())?) -> false?, openLater: boolean?, onMessage: (overlay, value) -> (), notHere: string?, onReady: ((overlay) -> ())?, onClose: ((overlay, how: string, err: Error?) -> ())?, onShown: ((overlay, ok: boolean, err: Error?) -> ())?, onBack: ((overlay, ok: boolean, err: Error?) -> ())?, prime: { x: number, y: number }? }) -> Page`

Gives the overlay a [page](gui.md#host-gui-webview) — an HTML page beside the plug-in, for what a Tab ring does badly — and makes the runtime hold the overlay's place while the page is in front. Without it, an overlay lets go for a page as for any other window, and every action of the page's in the plug-in is a round trip the module writes by hand ([Building a page](../building-a-page.md#step-5)). With it, the runtime shows the page from a key, keeps the overlay **away** while the page is in front ([`o.away`](#o-away)), takes the page's actions into the plug-in and brings the page back ([`O:inPlugin`](#o-inplugin)), and goes back to the plug-in on the same key ([`O:pageBack`](#o-pageback)). The page and its messages are the module's; the runtime names no plug-in and no DAW.

Called at the module's top level, **before the overlay is bound** — afterwards it raises. Since version 0.6 of the runtime: a module that calls it depends on `"com.platform.overlay >= 0.6"`, and declares `gui` itself, since its own `describe` calls [`host.gui.webview`](gui.md#host-gui-webview).

**At the call:**

1. `describe(on)` is called once, and must answer `host.gui.webview{ … }` with `onMessage = on.message`, `onReady = on.ready` and `onClose = on.close` — the runtime's callbacks for the page's three events. It is the module's function because a relative `file` resolves against the folder of the code that calls `host.gui.webview`. Describing builds nothing (gui.md).
2. It is checked: the answer must be a page — `show`, `hide`, `post`, `isShown`, `prepare`, `close` and `window` — and `describe` must have read all three fields of `on`.
3. `o.page` is set to the page, and the first page of a VM registers the runtime's page trigger: every window and focus event, one callback of each kind per VM.
4. `key` is registered with [`host.hotkey.register`](hotkey.md#host-hotkey-register), for the module's life — the **page key**.

**The group.** The page is shared by every overlay of this VM on the overlay's arbiter slot — a dialog-layer box over the same plug-in is in it — or by the overlay alone when it has no slot. When each overlay of the group is bound, it raises for the page key being, by [`host.keys.normalize`](keys.md#host-keys-normalize), the hotkey of one of its controls or tabs, and for a second page in the group.

| Field | What it is |
|---|---|
| `describe(on)` | Describes the page and returns it (above). |
| `key` | The page key, below. |
| `open(o)` | Runs before the key's show, in the key's handler, where a read may wait: it reads what the page shows, then posts the state, then the focus. `false` shows nothing — it has said why itself. |
| `openLater` | `true` hands `open` a second argument, `answer`, and the show waits for it instead of for `open`'s return (since 0.14; below, **A later answer**). Default `false`. |
| `onMessage(o, m)` | Every message from the page, as a handler, in the interactive lane. Nothing is filtered: a trip and the way back refuse themselves when the page is not in front. |
| `notHere` | What the key says where the overlay is not in front. Default `"<label> is not in front; its page opens from there."` |
| `onReady(o)`, `onClose(o, how, err)` | The module's own hooks, after the runtime's part: `onClose` hands on the page's own `how` (`"user"` or `"failed"`) and, beside `"failed"`, its error value ([`host.gui.webview`](gui.md#host-gui-webview)). |
| `onShown(o, ok, err)` | After every show the runtime made at the key, [`page:show`](gui.md#page-show)'s answer as it came: `true`; `true, err` — in front over the rectangle to avoid, `too_large`, a partial answer; or `false, err` — but for `not_ready`, which the runtime answers itself (step 4, below). The module plays its sound there. Without it, a `false` is said in the runtime's words. |
| `onBack(o, ok, err)` | After the way back (below): `true`, or `false` and its error value. Without it, a `false` is said in the runtime's words. |
| `prime` | The page's neutral point, in the overlay's coordinates, for a DAW whose [daw-hosts entry](../daw-hosts.md#entry) says it drops the first click (`swallowsFirstClick`, reserved): one click there at each trip's arrival, before `act`. No entry sets it today. |

**The page key**, a handler of the module's in the interactive lane. A foreground event of the page that never came is made up for first (it has been seen not to come): when the runtime's show asked for the place and the page has the keyboard, the group lets go now. Then the first case that applies:

1. **A trip is under way:** given up (`cancelled`), its `onGiveUp` run in this handler, and the page shown — even one in front already, since a focus still on its way could undo it. A trip whose work is done runs its back step now.
2. **The page has the keyboard:** the way back, [`O:pageBack()`](#o-pageback).
3. **A way back is under way and the plug-in's window has not come yet** (Logic's main window in front): the page shown again, beside the plug-in's window.
4. **The overlay is in front.** With a menu open over it — counted open, or held over its own popup — nothing is shown: `"<label>: a menu is open; close it, then press the key again."` Otherwise the place is noted — the window in front, the control the overlay is active on, the DAW's entry — `open` runs, the place is checked again ([`O:stillHere`](#o-stillhere), `place`), and the page is shown with `{ focus = true, avoid = <the window's bounds> }`. A page not built yet — `show`'s `not_ready` — says "Building the page." and shows it from its `onReady` — `open` asked afresh — only while the overlay is still on that control. A show Windows declines (`refused`) keeps the place asked for: it may come a moment later.
5. **The overlay is not in front, and the window in front is its plug-in's** (since 0.13): a plug-in window of the page overlay's [embedded binding](#o-attachembedded), whose DAW entry gives the [plug-in's panel](#o-attachembedded-macos) there, and the binding's `identify` — asked in this handler, as at a stay's start, where a read may wait — says the panel shows this plug-in. Where the keyboard is is not asked: in Logic it reads as nowhere while a plug-in's own drawn view has it. The group comes up on that panel through a hold of the key's, as a trip's arrival does — the window, the panel, the binding's `present` and the overlay's gate — and the page opens as in case 4. A binding without `identify`, or with a `control` function, does not open the page this way. The log says `[page] '<overlay>': the page key with the overlay not in front, in its plug-in's window '<title>': the overlay comes up on the DAW's panel there, and the page opens`, or, where the overlay did not come up on it, `… '<title>' is its plug-in's window, and the overlay did not come up on its panel — not opened` before case 6. The hold goes when away ends, and at once when nothing was shown. On Windows no DAW entry gives a panel, so this case never applies there.
6. **Otherwise:** `notHere` is said, interrupting, and logged with what is in front.

```luau
-- Nothing to write: a module whose binding has `identify` gets case 5 as it is.
local o = O.new("Synth")
o:attachEmbedded({ hosts = daw.all, control = { windows = "^JUCE_PlugIn" },
  identify = function(panel) return readWordmark(panel) == "Synth" end })
o:addPage({ key = "Ctrl+Shift+Alt+F1", describe = describe, open = open, onMessage = onMessage })
```

The order is the one [Step 5](../building-a-page.md#step-5) asks of a page: the state, then the error or the focus, then `show`.

**A later answer** (`openLater = true`, since version 0.14 of the runtime; a module that sets it depends on `"com.platform.overlay >= 0.14"`). `open(o, answer)` runs where it always does, and may return at once, its questions asked with callbacks — the reads in parallel instead of one after another in the key's handler — and call `answer(v)` once they are in: `answer()` (or `answer(nil)`) shows the page, `answer(false)` shows nothing, as `open`'s return would. Called at once inside `open`, it is what `open`'s return was; `open` returning `false` is `answer(false)`. When it comes, the place is checked again as it is after a waited `open` ([`O:stillHere`](#o-stillhere), `place`) — the overlay still in front in the same stay on the same window — and the page is shown with `{ focus = true, avoid = <the window's bounds> }`, `onShown` hearing the show's answer as at the key. **Nothing is shown**, and the log says so, when the page key was pressed again before the answer (each press asks `open` afresh, with an `answer` of its own; an earlier one's is then too late), when the page came to the front or a trip began meanwhile, and when the place no longer holds. A second call of the same `answer` is logged and ignored. **Nothing bounds the wait**: an `open` that never answers shows nothing, and the next key asks afresh — and a case-5 press's hold of the panel (above) stays until away ends or the key is pressed again. The module's other keys and events run meanwhile: the key's handler has returned. `open` that raises is logged, `[page] '<overlay>': open failed: <message>`, and its `answer` counts for nothing. Log lines: `[page] '<overlay>': open answers later — the show waits for its answer`, `… open answered N ms after the key`, and `… and the key was pressed again since — not shown`.

On Windows the show that follows a later answer is the same `page:show` as at the key; one Windows declines (`refused`) keeps the place asked for, as at the key.

```luau
-- The page's state read off one picture, its questions asked at once with callbacks.
o:addPage({
  describe = describe, key = "Ctrl+Shift+Alt+F1", onMessage = onMessage,
  openLater = true,
  open = function(o, answer)
    local r = o:toScreenRect({ 0, 0, 600, 400 }, { whole = true })
    local snap = r and host.screen.snapshot({ region = r })
    if not snap then return false end -- nothing to read: nothing shown
    host.ocr.recognize({ name = "preset", region = o:toScreenRect({ 34, 7, 214, 22 }) }, { snapshot = snap },
      function(_, byName)
        snap:release()
        o.page:post({ kind = "state", preset = byName.preset.text })
        o.page:post({ kind = "focus", id = "preset" })
        answer() -- the page is shown now
      end)
  end,
})
```

**Preparing.** On the module's turn after the overlay first comes to the front, not away, the page is built hidden ([`page:prepare()`](gui.md#page-prepare)), and the log carries the time of the call (`[page] '<overlay>': prepared: building (the call took N ms)`). A prepare that fails is not asked again from there: the key builds the page from then on. After `onClose("failed")` the next arrival prepares again.

**A page that came forward another way** — the module's own `page:show`, an app page reached with Alt+Tab — holds no place: the overlay lets go as for any window, its `onDeactivate` included, a trip is refused (`not_found`, `no place is held: the page came forward another way`), and the log says `[page] '<overlay>': the page came to the front without the page key — nothing held`.

**Cost.** One hotkey for the module's life, and one window and one focus callback per VM. Nothing is built until the first prepare. No timer, no poll, and nothing read while no page is shown; each window and focus event costs a VM with a page one `host.window.active()` per page.

**Thread.** Everything runs on the event loop, in the module's handlers ([Where a hook runs](#where-a-hook-runs)): the arbiter's calls that bring the overlay in or out only record and schedule.

```luau
-- module.toml: dependencies = ["com.platform.overlay >= 0.6"]
--              [capabilities] require = ["gui", "screen", "ocr", "input", "speech", "log"]
local O = host.require("com.platform.overlay")
local ov = O.new("Synth")
-- … the ring's controls …
ov:addPage({
  describe = function(on)
    return host.gui.webview({ file = "ui/index.html", title = "Synth page",
      onMessage = on.message, onReady = on.ready, onClose = on.close })
  end,
  key = host.os.pick({ windows = "Ctrl+Shift+Alt+F1", macos = "Ctrl+Shift+Win+F1" }),
  notHere = "The Synth window is not in front. Open it, then press the key again.",
  open = function(o)
    o.page:post({ kind = "state", preset = o.state.preset })
    o.page:post({ kind = "focus", id = "preset" })
  end,
  onMessage = function(o, m)
    if m.action == "back" then o:pageBack() end
  end,
})
ov:bind(BINDING, { slot = "com.example.synth", specificity = O.layer.base, menus = { O.menuTests.newWindow } })
```

### Returns {#o-addpage-returns}

The page — what `describe` answered, `host.gui.webview`'s page object — which is `o.page` from now on. It is not built yet: the first prepare or the page key builds it.

**Its `onShown` can hear the page in front with an error beside it.** `onShown(o, true, err)`, `err.code` `too_large`, is [`page:show`](gui.md#page-show)'s partial answer handed on: the page is in front with the keyboard, but over part of the plug-in's window, there being no room beside it. It is the one partial answer a module meets through the runtime. Test `ok` for whether the page is up, and `err` for whether it is where it was asked to be:

```luau
onShown = function(o, ok, err)
  if not ok then
    -- not shown: `err` says why (the page's own error, not_ready never reaches here)
    return host.speech.output("The page did not open: " .. err)
  end
  if err then host.log.info("[synth] the page is in front, over the plug-in: " .. err) end   -- too_large
end,
```

### Errors {#o-addpage-errors}

None of these is raised. Each comes as an error value to one of the page's hooks; the call itself answers none.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [page:show's errors](gui.md#page-show-errors) | hook onShown | After the runtime's show at the page key, `page:show`'s answer as it came: `false, err`, or `true, err` with `too_large` (Returns, above). Every one but `not_ready`, which the runtime answers itself — it says "Building the page." and shows the page from `onReady`, whose answer `onShown` then hears. Without `onShown`, a `false` is said in the runtime's words. | the host's | both |
| `gone` | hook onBack | The way back — [`O:pageBack`](#o-pageback), the page key in the page — found the plug-in's window gone; `err.detail` says how. The page stays. Without `onBack`, `"<label>: the plug-in's window is gone."` is said. | `the plug-in's window is gone` | both |
| any of [host.window.focus's errors](window.md#host-window-focus-errors) | hook onBack | The way back: the focus of the held control was not accepted, and its own error value is handed on — `refused`, or `gone`, `hidden` or `not_supported` where it says more. The page stays. | the host's | both |
| `refused` | hook onBack | The way back: that focus raised, or answered `false` with no error value; `err.detail` is the raise's message. Without `onBack`, `"<label>: the plug-in's window would not come to the front."` is said. | `the plug-in's window was not brought forward` | both |
| any of [host.gui.webview's errors](gui.md#host-gui-webview-errors) | hook onClose | The page failed: `onClose(o, "failed", err)`, the page's own error value beside `"failed"`. `onClose(o, "user")` has none. | the host's | both |

The way back's refusals that ask nothing of the system — a trip under way, the page without the keyboard, no place held — are `O:pageBack`'s answers, and `onBack` does not hear them. The page key the host claims but cannot hold now is no error of the call: the claim is kept and tried again by the host, and one line says so, `[page] '<overlay>': the page key <key> is not registered: <message>`; [`O:pageKey`](#o-pagekey) can try another key.

**Raises** for a mistake in the call, naming the caller's line: a call after binding (`overlay '<label>': addPage must be called BEFORE binding it — the binding is what starts it`), a field it does not have, a missing field or one of the wrong type, a `prime` that is not `{ x, y }`, a `describe` that raised, answered no page or did not read all of `on`, a second page on the same overlay (`overlay '<label>': a second page — one page to a group`), and a `key` the host refuses for a hotkey ([`host.keys.check`](keys.md#host-keys-check)'s `parse`, `reserved`, `no_keycode`, `tap`: `overlay '<label>': addPage's key <key> is refused for a hotkey (<reason>)`). As each overlay of the group is bound, it raises for the page key being the hotkey of one of its controls or tabs, and for a second page in the group (**The group**, above).

### Windows {#o-addpage-windows}

The place's control is the plug-in's own child window. The way back and every trip ask for that control with [`host.window.focus`](window.md#host-window-focus), which brings its window forward and lands the keyboard in it.

### macOS {#o-addpage-macos}

The place's control is the DAW's [plug-in panel](#o-attachembedded-macos), whose id is its window's: the way back and a trip ask for the window. While a page has the keyboard the DAW is not the active application (unless the page is described `activates = false`), and Logic may hide its plug-in windows then (gui.md, macOS). The page key's chord is a Mac's own: pick one with [`host.os.pick`](os.md#host-os-pick).

## O\:inPlugin(spec) — a trip into the plug-in {#o-inplugin}

**Signature:** `O:inPlugin(spec: { label: string, act: (overlay, trip) -> (), after: (overlay, result) -> string?, onGiveUp: ((overlay, err: Error) -> string?)? }) -> true | (false, Error)`

One action of the page's in the plug-in — a **trip**: the plug-in's window comes forward, `act` runs once it is there, and the page comes back after. Reads alone are a trip as well. Called from the page's message, on any overlay of the page's group.

- `act(o, trip)` runs as a handler on the module's next turn after the arrival, where a read may wait, with `o` the group's overlay active on the plug-in — quietly: nothing said, no key held, no `onActivate`.
- `trip:done(result)` ends the work; `after(o, result)` then posts the state, then the error or the focus, and the page comes back. `after` may return a short sentence, said only when the page does not come back.
- `onGiveUp(o, err)` posts the error of a trip that ended without `done`, handed its error value (the ends, below), and may return words, said likewise.

**Refused** with `false, err` — nothing asked of the system but the focus, one line logged, `[page] '<overlay>': trip '<label>' not started — <word>` (`busy`, `not in front`, `nothing held`, `gone`, `declined`) — in the order of the Errors table. A module posts nothing for `busy` and `not_in_front`, which the runtime has logged; for the others it posts an error to its page, and for `gone` its "plug-in window closed" sentence. A foreground event of the page that never came is made up for before `not_found` is decided, as at the key.

**Started:** `true`. The plug-in's control was asked to the front and accepted — accepted, not arrived. The trip waits for the plug-in's window **at most 8 seconds** (`TRIP_LOOK_MS`, a bound on looking, as [`MENU_LOOK_MS`](#o-choosemenuitem) is for a menu, never a guess that it has come).

**The arrival** is the plug-in's window in front and an overlay of the group active on the held control, through a **held context** placed first in the group's contexts for the trip: the window in front, the held control in it, the binding's `present` and the overlay's [gate](#o-gate), and nothing else — no question of where the keyboard is, no DAW chrome gate, no `identify`. The keyboard on REAPER's FX list or Logic's header does not keep a trip out. It is the group's: an overlay outside the group sharing the binding neither uses it nor sees its answer. The arrival is checked again on the module's turn before `act`: the trip still the same, the plug-in's window with the keyboard ([`foreground()`](window.md#host-window-foreground), afresh), the overlay still on it.

**The trip object** handed to `act` and to the waits' functions:

- [`trip.label`](#trip-label) and [`trip.live`](#trip-live) — true until the trip ended or its work was done.
- [`trip.overlay`](#trip-overlay) — the group's overlay active on the held control as it is asked: a sibling box that took the slot meanwhile is the one to act through (`trip.overlay:chooseMenuItem{ … }`).
- [`trip:inFront()`](#trip-infront) — the check before every click and every read of the plug-in after a wait; false ends the trip.
- [`trip:done(result)`](#trip-done) — the work is over, and the back step follows.
- [`trip:afterMenu(fn)`](#trip-aftermenu) — the menu the next click opens, then `fn`.
- [`trip:afterDialog(fn)`](#trip-afterdialog) — the dialog that comes next, then the plug-in again, then `fn`.
- [`trip:handOver(spec)`](#trip-handover) — the keyboard handed to the plug-in itself — the user types into its own text field or chooses in its own browser — until the key that ends it, the module's word that it is over, or any end of the trip; since version 0.9 of the runtime.
- **One wait at a time.** `afterMenu`, `afterDialog` and `handOver` raise, naming the caller's line, while another wait or a hand-over is pending, while the trip is not acting (before its arrival, during a wait, after `done` or its end) — they are for `act`, the waits' functions and their callbacks — and `afterMenu` on an overlay with no menu tests.

**The ends.** The page's window and focus events decide, and `trip:inFront()` decides on the module's own check. `onGiveUp(o, err)` is handed the error value of the end — each is a `hook onGiveUp` row under Errors, with the stages it ends — and the log says `[page] '<overlay>': trip '<label>' given up — <word>, after N ms in <stage>`, the word the row names.

**Not ends:** another window of the plug-in's application on the way (Logic's main window first, REAPER's main window); a sibling overlay of the group taking the slot (a dialog-layer box, quiet, and `trip.overlay` names it); a popup the menu tests see, over which the overlay holds its place with no keys; the dialog a wait is for; any other window of the plug-in's application while acting — the act's next `trip:inFront()` decides. A [hand-over](#trip-handover) ends with the trip on every end above, its keys back at that moment and its `onEnd` told. An act whose callbacks never come — a module's bug, a [watch](#o-watch) whose overlay left — is ended by the page key alone: `[page] '<overlay>': trip '<label>' given up at the key after N ms in <stage>`.

**Gone.** Windows: the plug-in's window is not listed for its process ([`host.window.list`](window.md#host-window-list): closed, or hidden), or — with it in front — the held control is not among its controls (the plug-in unloaded). macOS: only while the plug-in's application is the active one — a DAW may hide its plug-in windows while it is not — the window in front with another title, or another window of its application in front and the plug-in's window not listed; and a focus declined with nothing of the application listed any more (it quit). Checked at the start, at the way back, and at every window event of a trip that names the plug-in's window or its application.

**The back step**, on the turn after `done` (or in the page key's handler): `after(o, result)` runs — always, so the page has the result whenever it next shows — the trip ends, and the page comes back **unless the window in front is another application's, or one of this application's that is not the page** (with nothing in front, it comes back). It is shown with `{ focus = true, avoid = <the plug-in window's bounds now> }`, so beside the plug-in after a zoom step resized it. A page that does not come back says the runtime's sentence and `after`'s words; with the plug-in's window in front then, the group's keys come back, with nothing announced, and a show Windows grants a moment later takes the group away again. **A give-up** is the same with `onGiveUp(o, err)` in place of `after` — at the key it always shows.

**Reads inside a trip.** `act` is a handler with the plug-in in front and the group active: it takes its own marks and watches, reads with a callback or waits for a read, and asks `trip:inFront()` after every wait before it clicks or reads the plug-in. A change caused by a click is waited for from the click's own picture ([`host.screen.snapshot`](screen.md#host-screen-snapshot) with a callback, the click in its answer, then the change wait from there, at most 2 seconds; [`O:watch`](#o-watch) chains longer ones). After a page shown over the plug-in, the arrival's log line says how much the two overlap: whether the first picture can still show the page is not known.

**What is said.** Nothing on a trip that goes well — the screen reader says the plug-in's window and the page coming back. Given up with no page shown: `"<label>: another window came to the front; nothing more was done."` or `"<label>: the page was closed; nothing more was done."`, behind the screen reader, with `onGiveUp`'s words after. A page that does not come back: `"The page did not come to the front; if it does not come by itself, the page key brings it."` (declined, `refused`) or `"The page did not come to the front: <its message>."`, interrupting, with `after`'s or `onGiveUp`'s words. Every line of the runtime's is under `[page] '<overlay>': …`: each show and its answer, away beginning and ending and why, each trip's start, arrival — `trip '<label>' in the plug-in after N ms; the keyboard on <class>` — end and duration, each dialog's pid and title.

**Thread and cost.** All on the event loop, in the module's handlers. A trip costs one `host.window.focus`, one timer for its bound, and, while it waits, a look at each window or focus event; nothing is polled.

```luau
-- In the page's message: one action, the click and the read inside the trip.
local NAME, NEXT = { 34, 7, 214, 22 }, { 230, 14 }
local started, err = ov:inPlugin({
  label = "Next preset",
  act = function(o, trip)
    local region = o:toScreenRect(NAME, { whole = true })
    if not region then return trip:done({ error = "the name cannot be placed now" }) end
    host.screen.snapshot({ region = region }, function(before)
      if not trip:inFront() then return end -- false ended the trip already
      local x, y = o:toScreen(NEXT[1], NEXT[2])
      if not x or host.window.ownsPoint(o:hwnd(), x, y) == false then
        return trip:done({ error = "something covers Next" })
      end
      host.input.click(x, y)
      host.screen.snapshot({ region = region, change = { from = before, timeout = 2000 } },
        function(after, err2)
          if not trip:inFront() then return end
          if not after then return trip:done({ error = "the name cannot be read now (" .. tostring(err2) .. ")" }) end
          host.ocr.recognize(region, { snapshot = after }, function(r)
            trip:done({ name = r.status == "text" and r.lines[1] and r.lines[1].text or nil })
          end)
        end)
    end)
  end,
  after = function(o, result)
    o.page:post({ kind = "state", preset = result.name })
    if result.error then o.page:post({ kind = "error", text = "Next preset: " .. result.error }) end
    return result.name and ("Preset " .. result.name) or nil -- said only if the page does not come back
  end,
  onGiveUp = function(o, gaveUp)
    o.page:post({ kind = "error", text = "Next preset: " .. gaveUp })
  end,
})
-- `busy` and `not_in_front` are the runtime's to log; the others the page shows.
if not started and err.code ~= O.errors.BUSY and err.code ~= O.errors.NOT_IN_FRONT then
  ov.page:post({ kind = "error", text = "Next preset: " .. err })
end
```

### Returns {#o-inplugin-returns}

- `true` — started: the plug-in's control was asked to the front and accepted — accepted, not arrived. `act` runs once it is there, and the trip ends with one of two hooks, once: `after(o, result)` once `trip:done(result)` was called, or `onGiveUp(o, err)` when it ended without `done`.
- `false, err` — not started; nothing was asked of the system but, for the last three rows at the call, the focus.

### Errors {#o-inplugin-errors}

None of these is raised. Each comes as an error value where "arrives" says: at the call `return`, in the order they are decided; after a start, `hook onGiveUp`, the trip given up.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `busy` | return | A trip of the group is under way — one at a time; the second message is dropped. Logged `busy`. | `a trip of the page is under way` | both |
| `not_in_front` | return | The page does not have the keyboard ([`foreground()`](window.md#host-window-foreground), read afresh): a message sent before the user went elsewhere. Logged `not in front`. | `the page does not have the keyboard` | both |
| `not_found` | return | No place is held: the page came forward another way, not by the page key. Logged `nothing held`. | `no place is held: the page came forward another way` | both |
| `gone` | return | The plug-in's window is gone (**Gone**, above), or on a Mac a focus was declined with nothing of its application listed any more; `err.detail` says how. Logged `gone`. | `the plug-in's window is gone` | both |
| any of [host.window.focus's errors](window.md#host-window-focus-errors) | return | The focus of the held control was not accepted, and its own error value is handed on — `refused` (`declined by the system`), or `gone`, `hidden` or `not_supported` where it says more. Logged `declined`. | the host's | both |
| `refused` | return | That focus raised, or answered `false` with no error value; `err.detail` is the raise's message. Logged `declined`. | `the plug-in's window was not brought forward` | both |
| `moved_on` | hook onGiveUp | A window of another application came forward, or one of this application's that is not the page, in every stage but after `done`; or `trip:inFront()` found that window in front. Log word `another application`. | `a window of another application came to the front, or one of this application's that is not the page` | both |
| `cancelled` | hook onGiveUp | The page key, in every stage but after `done` — then the back step runs at once instead. `onGiveUp` runs in the key's handler, and the page is shown. Logged `given up at the key`. | `the page key gave the trip up` | both |
| `closed` | hook onGiveUp | The page closed — its close box, its failure, the module's `page:close()` or `O:pageAgain()`, a disable: the page not shown at an event — in every stage. Log word `page closed`. | `the page was closed` | both |
| `gone` | hook onGiveUp | The plug-in's window gone (**Gone**, above), in every stage but after `done`; `err.detail` says how. Log word `gone`. | `the plug-in's window is gone` | both |
| `not_in_front` | hook onGiveUp | On the way: the plug-in's window not in front within 8 seconds of the focus. Log word `not in front`. | `the plug-in's window did not come within … ms` | both |
| `not_in_front` | hook onGiveUp | Acting or waiting: `trip:inFront()` false, the window in front not another application's — the plug-in's window without the keyboard, or no overlay of the group on the held control. Log word `not in front`. | `the plug-in's window does not have the keyboard` | both |
| `taken` | hook onGiveUp | On the way, or in a wait whose condition holds: the plug-in's window in front and another module's overlay at least as specific holding the slot, decided on the turn after its event. Log word `outranked`. | `another module's overlay holds the plug-in's window` | both |
| `failed` | hook onGiveUp | Acting: `act`, a wait's `fn` or the prime raised — or a hand-over's `ends`, `onKey` or `onEnd` ([`trip:handOver`](#trip-handover-errors)). The message names it: `act failed`, `afterMenu failed`, `afterDialog failed`, `the prime failed`, `ends failed`, `onKey failed`, `onEnd failed`; `err.detail` is the raise's message. Log word `failed`. | `… failed` | both |

**Raises** for a mistake in the call, naming the caller's line: a `spec` that is not a table, a field it does not have, a missing or empty `label`, an `act` or `after` that is not a function, an `onGiveUp` that is neither a function nor `nil`, and an overlay whose group has no page (`overlay '<label>': inPlugin — its group has no page (O:addPage)`). `act`, `after`, `onGiveUp` and the waits' functions run guarded: one that raises is logged, `[page] '<overlay>': <hook> failed: <message>`, and `act` or a wait's `fn` gives the trip up (`failed`, above).

### Windows {#o-inplugin-windows}

The trip asks for the plug-in's **control**, never its FX window: [`host.window.focus`](window.md#host-window-focus) of a control brings its window forward and lands the keyboard in the control, putting it there at most twice where the application left it elsewhere (HW1). A trip arrives whether or not that lands.

### macOS {#o-inplugin-macos}

The trip asks for the plug-in's window (the panel's id is its window's), and the [stay](#o-attachembedded-macos) is held across it: no `identify` within a trip, nor between trips while the page is up. A window of the DAW's with the plug-in window's title that comes forward under another id is logged, and the trip waits for the held id (`… came forward as id Y, not the held id X — waiting for X`). With `activates = false` (gui.md) whether the plug-in's window raises a foreground event at all is unmeasured; without one, the trip ends by its bound. A sandboxed DAW's open panel may belong to another process, which ends a dialog wait as another application's window (`moved_on`): the log names the pid.

## trip.label {#trip-label}

**Signature:** `trip.label: string`

The [trip](#o-inplugin)'s label, `spec.label` of the `O:inPlugin` that started it: what every `[page]` line of the trip names (`trip '<label>' …`) and what the runtime's sentences about it begin with (`"<label>: the page was closed; nothing more was done."`).

```luau
act = function(o, trip)
  host.log.info("[my-module] " .. trip.label .. ": in the plug-in")
end,
```

### Returns {#trip-label-returns}

The string the trip was started with, for its whole life, read from the trip itself with no call. Writing it is not refused: the trip's later lines and sentences then say what was written.

## trip.live {#trip-live}

**Signature:** `trip.live: boolean`

Whether the [trip](#o-inplugin) is still under way: `true` from its start until its work is done — [`trip:done`](#trip-done) — or it is given up, by any of its ends, by [`trip:inFront()`](#trip-infront) answering false, or by a [`trip:handOver`](#trip-handover-errors) that answered `moved_on` or `not_in_front`. A function that runs after a wait — a read's callback, a hand-over's `onEnd`, a module's own timer — reads it to know whether the trip is still its to act in. Unlike `trip:inFront()` it asks nothing of the system and ends nothing.

```luau
local handed, err = trip:handOver({ kind = "browse", onEnd = onEnd })
if not handed and trip.live then trip:done({ error = "the keyboard: " .. err }) end -- not live: given up
```

### Returns {#trip-live-returns}

`true` or `false`, read from the trip itself with no call. Once `false`, never `true` again: the back step or the give-up follows on the module's next turn, or in the page key's handler.

## trip.overlay {#trip-overlay}

**Signature:** `trip.overlay: Overlay`

The overlay to act through: the one of the page's group active on the plug-in's held control, and not holding its place over a menu, as it is read. A sibling box that took the slot meanwhile — a dialog-layer overlay — is the one in front then, so after a wait a module acts through `trip.overlay`, not the overlay its act was handed (`trip.overlay:chooseMenuItem{ … }`).

```luau
trip:afterDialog(function(o, t, seen)
  if not t:inFront() then return end
  local now = t.overlay   -- a dialog-layer box, if one took the slot while the dialog was up
  t:done({ saved = seen, on = now.label })
end)
```

### Returns {#trip-overlay-returns}

An overlay, read afresh at each read: the group's overlay active on the held control; when none is, the one the trip last arrived with or waited for a menu through — at first the overlay whose `O:inPlugin` started it. Never `nil`.

## trip\:inFront() {#trip-infront}

**Signature:** `trip:inFront() -> boolean`

The check a [trip](#o-inplugin)'s act makes before every click and every read of the plug-in after a wait: true while the trip is live, the plug-in's window has the keyboard ([`foreground()`](window.md#host-window-foreground), read afresh) and an overlay of the group is on the held control, not held over a menu. **False ends the trip**: given up on the module's next turn, `onGiveUp` hearing `moved_on` when the window in front is another application's or one of this application's that is not the page, and `not_in_front` otherwise. Another window's foreground event waits behind a handler that waits ([handlers](../module-runtime-and-lifecycle.md#handlers)), so only this fresh reading knows the user went elsewhere meanwhile. An act just returns after a false. Each false is logged: `[page] '<overlay>': trip '<label>': its act asked, and it is not in the plug-in — <what is in front>`.

Costs one `host.window.foreground()` read, which asks the system afresh, on the event loop.

```luau
host.screen.snapshot({ region = region }, function(before)
  if not trip:inFront() then return end   -- false ended the trip already: onGiveUp says why
  host.input.click(x, y)
end)
```

### Returns {#trip-infront-returns}

`true` or `false`. After the trip ended or its work was done, `false`, and nothing more happens.

### Errors {#trip-infront-errors}

None. It never answers an error value. The error value of the end it causes goes to `onGiveUp` ([`O:inPlugin`](#o-inplugin-errors)), and to a hand-over's `onEnd`.

It never raises.

## trip\:done(result) {#trip-done}

**Signature:** `trip:done(result: any) -> ()`

The work of a [trip](#o-inplugin) is over: the back step runs on the module's next turn — `after(o, result)`, then the page back — so `done` may be called from anywhere: a read's callback, `chooseMenuItem`'s `onDone`, a plain call. A pending wait is dropped, and a hand-over's keys go back at once (its `onEnd` hears `"done"` in the back step). Logged `[page] '<overlay>': trip '<label>' done after N ms`.

```luau
host.ocr.recognize(region, { snapshot = after }, function(r)
  trip:done({ name = r.status == "text" and r.text or nil })
end)
```

### Returns {#trip-done-returns}

Nothing. `result` is handed to `after` as given.

### Errors {#trip-done-errors}

None. It never answers an error value.

It never raises. A second call, or one after the trip ended, is logged and ignored: `[page] '<overlay>': trip '<label>': done after the trip ended — ignored`.

## trip\:afterMenu(fn) {#trip-aftermenu}

**Signature:** `trip:afterMenu(fn: (overlay, trip, seen: boolean) -> ()) -> true | (false, Error)`

"The menu my next click opens, then `fn`." A press of a control with `opensMenu` on `trip.overlay`, so called **before** the click: its [menu tests](#o-menutests) run on every tick from here and `newWindow` takes its list. `fn(o, trip, true)` runs as a handler once a test of the group has seen a menu, then seen it closed, and the trip is on the plug-in again; `fn(o, trip, false)` when no test saw a menu within `MENU_LOOK_MS` (8 seconds). Once a menu was seen, nothing bounds the wait: the user's time in a menu is theirs. While it waits, the trip is not acting: a second wait raises, and `fn` is where the act goes on.

```luau
-- In a trip's act: the plug-in's preset menu, opened by a click, and the plug-in again after it.
local ok, err = trip:afterMenu(function(o2, t2, seen)
  if not seen then return t2:done({ error = "the menu did not open" }) end
  t2:done({})
end)
if not ok then return trip:done({ error = "the menu cannot be opened now: " .. err }) end
host.input.click(x, y)   -- only after a true: the press counts from the call
```

### Returns {#trip-aftermenu-returns}

- `true` — the press counts, and the wait has begun: make the click now.
- `false, err` — nothing is to be clicked, and no wait has begun.

### Errors {#trip-aftermenu-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | `trip.overlay` is not in front at the call — a sibling box has the slot, say. | `'…' is not in front` | both |
| `not_in_front` | return | A calibrating run's picture or a menu test's `before` was taken before the press, and by then the overlay is no longer active. | `the overlay is no longer active` | both |
| `moved_on` | return | Likewise, the overlay left the front and came back, or is on another window, by then. | `the overlay left the front and came back since`, `the overlay is on another window now` | both |

**Raises**, naming the caller's line: another wait of the trip pending (`trip '<label>': afterMenu while a <kind> wait is pending — one wait at a time`), its hand-over under way, the trip not acting — before its arrival, during a wait, after `done` or its end (`trip '<label>': afterMenu is for the trip's act and its continuations, while it is acting (it is <stage>)`) — an `fn` that is not a function, and `trip.overlay` with no menu tests (``trip '<label>': afterMenu needs menu tests on overlay '<overlay>' — `menus` on its binding``). `fn` runs guarded: one that raises gives the trip up, `failed` (`afterMenu failed`).

## trip\:afterDialog(fn) {#trip-afterdialog}

**Signature:** `trip:afterDialog(fn: (overlay, trip, seen: boolean) -> ()) -> ()`

"The dialog that comes next, then the plug-in again, then `fn`." From the call on, a window of the plug-in's application other than its window that comes forward is the dialog — decided on the module's turn after its event, once every overlay of the VM has looked at it — unless it is the window a menu was held over for the trip's last press or a menu still counts open over the group. `fn(o, trip, true)` once the plug-in's window is in front again with the trip arrived; `fn(o, trip, false)` when no dialog came within 8 seconds. Once a dialog was seen, nothing bounds the wait: the user's time in a file dialog is theirs. Each dialog is logged with its pid and title.

```luau
-- In a trip's act: the plug-in's Save button opens a file dialog; the user saves in it.
trip:afterDialog(function(o2, t2, seen)
  t2:done({ saved = seen })
end)
host.input.click(x, y)
```

### Returns {#trip-afterdialog-returns}

Nothing: the wait always begins. What follows comes to `fn`.

### Errors {#trip-afterdialog-errors}

None. It never answers an error value. A dialog that did not come is `fn(o, trip, false)`; a window of another application coming forward meanwhile gives the trip up ([`O:inPlugin`](#o-inplugin-errors)).

**Raises**, naming the caller's line, as `afterMenu` does: another wait pending, a hand-over under way, the trip not acting, and an `fn` that is not a function (`trip '<label>': afterDialog takes a function(overlay, trip, seen), got <type>`). `fn` runs guarded: one that raises gives the trip up, `failed` (`afterDialog failed`).

## trip\:handOver(spec) — handing the keyboard to the plug-in {#trip-handover}

**Signature:** `trip:handOver(spec: { kind: "typing" | "browse" | "keys", keys: { string }?, onKey: ((overlay, trip, key: string, mods, press: { count: number }) -> boolean?)?, ends: ((overlay, trip, info, answer: (over: boolean) -> ()) -> ())?, onEnd: (overlay, trip, how: "ended" | "done" | "given_up", info, err: Error?) -> (), swallow: { keys: { string }, say: string }? }) -> true | (false, Error)`, where `info = { key: string?, passed: { string }, ms: number }`

A wait of a [trip](#o-inplugin) in which the user works in the plug-in **itself**: types a new name into its own text field, chooses in its own preset browser, or presses keys the module answers one by one. The runtime holds only the keys that end it — and the keys the module asks it to swallow (`swallow`, below) — and while it holds them the plug-in has the rest of the keyboard; nothing is recorded of what the user types. The page stays where the trip left it — behind the plug-in, not hidden — the group stays [away](#o-away) and quiet, and nothing is said but `swallow`'s sentence. Since version 0.9 of the runtime: a module that calls it depends on `"com.platform.overlay >= 0.9"`; `swallow` since version 0.16, `">= 0.16"`.

| kind | What it holds | `keys` | `onKey` | `ends` | Its own end |
|---|---|---|---|---|---|
| `"typing"` | the end keys, captured, and the module's [menu flag](keys.md#host-keys-menuopen) set, so they **go through** to the plug-in | the end keys; default `{ "Return", "Escape" }` | not taken | optional | an end key gone through, then `ends` answering true |
| `"browse"` | the same | default `{ "Return" }` — Escape and every other key are the plug-in's | not taken | optional | the same |
| `"keys"` | the keys named, captured for `onKey`; no flag | **required**, the keys `onKey` is handed | **required** | not taken | `onKey` answering true |

Called from `act`, a wait's function or one of their callbacks, while the trip is acting, with the plug-in's window in front.

**It answers** `true` once the keys are held, or `false` and an error value (Returns and Errors, below). After `moved_on` or `not_in_front` the trip is given up already — `trip.live` is false — and its `onGiveUp` posts it, handed the same error value: a module just returns. After any other, the trip goes on, and the module ends it with an error, `trip:done({ error = … })`; nothing is held.

**At the call**, in this order: what [`host.keys.passedThrough`](keys.md#host-keys-passedthrough) holds from before is drained and dropped, logged — a Return that reached a menu earlier is no end of this one; the module's [scope](keys.md#host-keys-scope) is pinned to the plug-in's window; the keys are captured; for `"typing"` and `"browse"` the flag is set; the VM's menu tick is started if no overlay with menu tests has started it; the keys of `swallow` are captured after them; one line is logged: `[page] '<overlay>': trip '<label>' hands the keyboard over (typing): Return, Escape let through to window id X, the module's scope pinned there; it ends on one of them, the page key or another application — nothing bounds it` — with `swallow`, `…, the module's scope pinned there; Tab, Shift+Tab swallowed there; it ends on …`. A capture that is refused releases what was captured, sets the scope back as the end does — with nothing else of the VM in front — and writes no flag; the refused one registered nothing ([`capture`](keys.md#host-keys-capture)). One line says so: `… the keyboard is not handed over — <key> is not captured: <message>`.

**Why two captures and a flag.** While a module's menu flag counts for the window in front the hook lets every captured key through to it and records it for `passedThrough`, together with every unmodified Return and Escape; and a flag counts only while its module holds a capture, for the window its captures are scoped to ([`menuOpen`](keys.md#host-keys-menuopen)). The captures make the flag count, the scope keeps it to the plug-in's window, and what goes through is recorded — which is how the runtime knows the user pressed Return in the plug-in's own field without taking the key from it.

**The event.** The record is drained by the VM's menu tick (every 150 ms), which hands the keys it holds to the hand-over and decides nothing by time. The first end key is the event: logged (`… Return went through to the plug-in — the hand-over's end is looked at`), and the end is looked at where the trip can arrive — on the module's next turn, the plug-in's window in front (afresh) and an overlay of the group on the held control, as a wait's arrival. A key that is not an end key is logged as gone through, and is no end. For `"keys"` the event is `onKey` answering `true`; it is asked for each captured key, as a handler, with the plug-in's window in front, and a key pressed with another window in front, or after the event, is not handed to it. It is handed the key's `mods` and `press` as the capture's callback got them — `press.count`, the key's count of quick repeats ([`host.keys.capture`](keys.md#host-keys-capture)), since version 0.12 of the runtime.

**`swallow = { keys, say }` — keys that do nothing**, since version 0.16 of the runtime, for a key the plug-in must not get while the user works in it: VPS Avenger on Windows switches its oscillators on Tab, so its renames, its browsers and its knobs held swallow Tab and Shift+Tab. `keys` is a list of 1 to 8 key specs, none of them one of the hand-over's own `keys`; `say` is the module's sentence of how the hand-over ends (`"Return confirms, Escape cancels."`). Of every kind, the keys are captured with [`menu = "taken"`](keys.md#host-keys-capture), so the hook takes them while the flag lets every other captured key of the module through: they reach neither the plug-in nor the record of keys gone through — so never `info.passed` — nor `onKey`, and they are no event. At the first of them pressed with the plug-in's window in front, `say` is said, interrupting — once in the hand-over, however many follow; the hand-over says nothing at its start. Each is logged: `[page] '<overlay>': trip '<label>': Tab swallowed — it does nothing and never reaches the plug-in: said "Return confirms, Escape cancels."`, later `…, said once already`; one whose callback runs with another window in front says nothing (`… Tab swallowed with … in front — it does nothing; nothing said`). Every end of the hand-over gives them back with its other keys, in the same moment; the end's line names them (`… Return, Escape released with Tab, Shift+Tab, swallowed, the menu flag false, …`). A menu the system drew in the plug-in's application still gets them, as it gets every captured key.

**A key taken after all.** When a `"typing"` or `"browse"` end key reaches its capture's callback instead — the flag did not count for the window in front, because something cleared it — it never reached the plug-in, so it is no event: the log says so (`… Return was taken, not let through — the menu flag did not count for the window in front (…)`), the scope and the flag are set again with the plug-in's window in front, and `"<label>: Return did not reach the plug-in; press it again."` is said behind the screen reader.

**`ends(o, trip, info, answer)`** — the module's word on whether the event ended the work: its field closed again, its browser done. It runs as a handler where a read may wait, and answers by calling `answer(true)` (over) or `answer(false)` (it goes on), at once or from a read's callback; a second answer, or one after the trip ended, is logged and ignored. Without `ends` the event is the end. Meanwhile the keys stay held: an end key that comes while `ends` is asked is kept, and is the next event at once if the answer is no. An `ends` that raises gives the trip up, `failed`. An `ends` that never answers is a stuck act: the page key ends it.

**`onEnd(o, trip, how, info, err)`** — told **exactly once**, on every end of a hand-over that answered `true`, after the keys are back; a `handOver` that answered false held nothing, and its `onEnd` is never called. `how` is a status word, `"ended"`, `"done"` or `"given_up"`; for `"given_up"` the trip's give-up's error value comes fifth — the same table [`onGiveUp`](#o-inplugin) is handed, one of the `hook onEnd` rows under Errors — and for the other two nothing does:

| how | when | where it runs |
|---|---|---|
| `"ended"` | `ends` answered true, or `onKey` did, or — with no `ends` — the event came (`info.key` the end key) | the trip acting: in the handler or the callback that answered `ends` — the arrival's own, or a read's callback — and after `onKey`, or with no `ends`, in the arrival's handler on the module's next turn; the module reads, then calls `trip:done(result)` |
| `"done"` | the module's own `trip:done` while the hand-over holds | in the back step's handler, before `after` |
| `"given_up"` | any end of the trip without `done`, `err` beside it | before `onGiveUp`, in the place each Errors row names |

Before version 0.11 of the runtime `how` named each give-up in a word of its own (`"page key"`, `"another application"`, …) and `onEnd` had no fifth argument.

`info.key` is the end key on `"ended"` and nil on every other end; `info.passed` every key the record held while the hand-over held its keys, in order — for `"keys"`, every key `onKey` was handed; `info.ms` the time since the call. An `onEnd` that raises is logged, `[page] '<overlay>': onEnd failed: <message>`.

**Keys back on every path, at once.** Whatever ends the trip, the hand-over's keys go back in that moment — in the give-up's own handler, in `done`, at its own end; plain calls only, so in the arbiter's call and a trigger alike: the captures released, the flag what the VM's overlays not away say (false while away), the scope every window again unless an overlay of the module that is not away is in front, whose pin stands, and what the record still holds dropped (logged `… went through after the end: …`). One line says it: `[page] '<overlay>': trip '<label>': the hand-over ends — <word>, after N ms; Return, Escape released, the menu flag false, the scope every window`. While a hand-over holds the module's flag and scope, the runtime writes neither of them anywhere else: an overlay leaving the front, the menu tick's state, a hold over a menu's window leave them as the hand-over set them.

**Not ends:** a window of the plug-in's own application — Logic's main window, REAPER's, a dialog of the DAW's — logged once per hand-over (`… a window of <app> came forward during the hand-over — not an end`); the page itself coming forward; a sibling overlay of the group taking the slot; a key that is not an end key; and **time**: nothing bounds a hand-over. The user's way back is the page key.

**Disable, enable, reload.** A disable takes the module's captures out of the hook and forgets its scope and flag ([`scope`](keys.md#host-keys-scope)), and hides the page; the hand-over's own record still says it holds. Enabled again, the captures are back in every window, with no flag, and the **first** of the menu tick (within 150 ms), a window or focus event, or a captured key ends the trip, `closed`, and releases them: at most the one key pressed in that moment is taken, and the log names it. A reload ends the VM, and the host drops its captures, its scope and its flag with it; no `onEnd` runs then, as no code of the VM's runs any more.

**Cost.** The captures — two for `"typing"`, and one per key of `swallow` — the flag, one tick of the VM's one 150 ms timer, which drains the record, and nothing read of the screen: what the module's own hooks read is theirs. Everything on the event loop, in the module's handlers.

```luau
-- In a trip's act: a field the plug-in edits in place on a double-click, renamed by the user and
-- read back. FIELD is the field's rectangle, in the overlay's coordinates.
local FIELD = { 34, 7, 214, 22 }
local region = o:toScreenRect(FIELD, { whole = true })
host.screen.snapshot({ region = region }, function(before)
  if not trip:inFront() then return before and before:release() end
  if not before then return trip:done({ error = "the field cannot be read now" }) end
  local x, y = o:toScreen(123, 14)
  O.doubleClick(x, y, nil, function(ok)
    if not ok then
      before:release()
      return trip:done({ error = "the field was not double-clicked" })
    end
    -- The plug-in's editor, seen open: a box over a tenth of the field that stands still.
    local open = { from = before, timeout = 2000, settle = 50, minPixels = 400 }
    host.screen.snapshot({ region = region, change = open }, function(editor, _, seen)
      before:release()
      if not trip:inFront() then return editor and editor:release() end
      if not (editor and seen.changed and seen.settled) then
        if editor then editor:release() end
        return trip:done({ error = "the editor did not open" })
      end
      editor:release()
      local handed, err = trip:handOver({
        kind = "typing", -- Return keeps the name, Escape the old one: both go through to the plug-in
        -- Tab would leave the field in this plug-in: it does nothing, and says once how to end.
        swallow = { keys = { "Tab", "Shift+Tab" }, say = "Return keeps the name, Escape cancels." },
        ends = function(o2, t2, info, answer)
          -- Over once the field no longer shows the editor (fieldClosed: the module's own read).
          host.screen.snapshot({ region = region }, function(now)
            if not t2:inFront() then return now and now:release() end
            answer(now ~= nil and fieldClosed(now))
            if now then now:release() end
          end)
        end,
        onEnd = function(o2, t2, how, info)
          if how ~= "ended" then return end -- onGiveUp posts the rest
          if not t2:inFront() then return end
          host.ocr.recognize(region, {}, function(r)
            t2:done({ name = r.status == "text" and r.text or nil, cancelled = info.key == "Escape" })
          end)
        end,
      })
      if not handed and trip.live then trip:done({ error = "the keyboard: " .. err }) end -- not live: given up
    end)
  end)
end)
```

And `"keys"`: a knob the plug-in turns only while its button is held, pressed and held through the hand-over — PageUp and PageDown a point each, Return letting go and reading it, Escape letting go — every end letting go once, and not where the host let go already at a disable ([`host.input.held`](input.md#host-input-held)):

```luau
-- In a trip's act. KNOB is the knob's middle and its box, in the overlay's coordinates; readKnob is
-- the module's own reader of a picture of it.
local KNOB, BOX = { 120, 80 }, { 104, 64, 136, 96 }
local x, y = o:toScreen(KNOB[1], KNOB[2])
if not x then return trip:done({ error = "the knob cannot be placed now" }) end
local dy, down = 0, false
local function letGo()
  if not down then return end
  down = false
  if table.find(host.input.held(), "left") then host.input.mouseUp(x, y + dy) end
end
local handed, err = trip:handOver({
  kind = "keys",
  keys = { "PageUp", "PageDown", "Return", "Escape" }, -- captured: none of them reaches the plug-in
  onKey = function(_, _, key)
    if key == "Return" or key == "Escape" then
      letGo()
      return true -- the end: onEnd(…, "ended", …) on the next turn
    end
    dy += key == "PageUp" and -1 or 1
    host.input.move(x, y + dy) -- with the button held, a drag
    return false
  end,
  onEnd = function(o2, t2, how, info)
    letGo() -- first, on every end
    if how ~= "ended" or not t2:inFront() then return end -- onGiveUp posts the rest
    if info.key == "Escape" then return t2:done({}) end
    host.screen.snapshot({ region = o2:toScreenRect(BOX, { whole = true }) }, function(snap)
      if not t2:inFront() then return snap and snap:release() end
      t2:done({ value = snap and readKnob(snap) or nil })
      if snap then snap:release() end
    end)
  end,
})
if not handed then
  if trip.live then trip:done({ error = "the keyboard: " .. err }) end -- not live: given up, onGiveUp posts it
  return
end
host.input.mouseDown(x, y) -- after the hand-over, so that one refused leaves no button down
down = true
```

### Returns {#trip-handover-returns}

- `true` — the keys are held, and the hand-over has begun; `onEnd` will be told once, on whatever ends it.
- `false, err` — nothing is held, nothing was registered, and `onEnd` is never called. For `moved_on` and `not_in_front` the trip itself is given up already, and `err` is the error value its `onGiveUp` is handed.

### Errors {#trip-handover-errors}

None of these is raised. Each comes as an error value where "arrives" says: at the call `return`, in the order they are decided; after a `true`, `hook onEnd` with `how = "given_up"`, the same error value the trip's `onGiveUp` hears just after.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `busy` | return | A hand-over of another page of this VM still holds the module's keys: one at a time per VM, since the flag, the scope and the record are the module's. Another page of the VM comes to the front only after that trip has ended (another application's window), so this is a guard for events that cross, not a state a module reaches in order. | `trip '…' of this module holds the module's keys` | both |
| `moved_on` | return | [`trip:inFront()`](#trip-infront) answered false at the call, which has given the trip up already, with a window of another application in front, or one of this application's that is not the page: the give-up's own error value. Before version 0.12.1 of the runtime, `not_in_front`. | `a window of another application came to the front, or one of this application's that is not the page` | both |
| `not_in_front` | return | `trip:inFront()` answered false at the call, which has given the trip up already, with any other window in front, or none: the give-up's own error value. | `the plug-in's window does not have the keyboard` | both |
| any of [host.keys.capture's errors](keys.md#host-keys-capture-errors) | return | A key — one of `keys` or of `swallow` — could not be captured: no keyboard hook, or on a Mac no event tap without the Accessibility grant (`denied`). The host registered nothing for it, and what was captured before it is released. | the host's | both |
| `failed` | return | A capture raised — a key `normalize` read and `capture` did not: the host's own failure. What was captured before it is released. | `capturing … raised: …` | both |
| `cancelled` | hook onEnd | The page key, in every state of the hand-over. `onEnd` runs in the key's handler, before `onGiveUp`; then the page is shown. | `the page key gave the trip up` | both |
| `moved_on` | hook onEnd | A window of another application came forward, or one of this application's that is not the page — or `trip:inFront()` found one in front, inside a hook or a callback of the module's. On the next turn, before `onGiveUp`; the page is not shown over it. | `a window of another application came to the front, or one of this application's that is not the page` | both |
| `closed` | hook onEnd | The page closed — its close box, its failure, the module's `page:close()` or `O:pageAgain()`, a disable, seen at the first tick, event or captured key after the enable. Before `onGiveUp`, where it was seen — the page's close, `O:pageAgain()`, a window or focus event, a captured key — and on the module's next turn when the menu tick saw it first. | `the page was closed` | both |
| `gone` | hook onEnd | The plug-in's window gone ([the trip's rule](#o-inplugin)); `err.detail` says how. On the next turn, before `onGiveUp`. | `the plug-in's window is gone` | both |
| `taken` | hook onEnd | An end key with another module's overlay at least as specific holding the slot. On the next turn, before `onGiveUp`. | `another module's overlay holds the plug-in's window` | both |
| `not_in_front` | hook onEnd | `trip:inFront()` false inside a hook or a callback of the module's, the window in front not another application's. On the next turn, before `onGiveUp`. | `the plug-in's window does not have the keyboard` | both |
| `failed` | hook onEnd | `ends` or `onKey` raised, or `onEnd` raised on `"ended"` — which is then not told again — or the trip's act raised after the hand-over began. The message names it, `ends failed`, `onKey failed`, `onEnd failed`, `act failed`; `err.detail` is the raise's message. On the next turn, before `onGiveUp`. | `… failed` | both |

**Raises** for a mistake in the call, naming the caller's line: a `spec` that is not a table or has a field it does not have; a `kind` that is none of the three; an `onEnd` that is not a function; `keys` that is not a list of 1 to 8 key specs, names a key [`host.keys.normalize`](keys.md#host-keys-normalize) cannot read, or one key twice; `onKey` missing for `"keys"` or given for another kind; `ends` given for `"keys"`, or not a function; a `swallow` that is not a table or has a field other than `keys` and `say`, whose `keys` is not a list of 1 to 8 key specs, names a key `normalize` cannot read, names one key twice or names one of the hand-over's own `keys` — a key is held or swallowed, not both — or whose `say` is not a string or is empty; and, as [`trip:afterMenu`](#trip-aftermenu-errors) does, a wait or a hand-over of this trip pending — `ends` asking for another one included — or the trip not acting.

### Windows {#trip-handover-windows}

The scope is the window in front at the call, the plug-in's FX window or the DAW window it is docked in, and the low-level hook lets a captured end key through to it while the flag counts, records it, and logs once per reason and window `captured Return (vk 0x0d/m0) was let through to the application: … a menu declared open with host.keys.menuOpen(true) by a module scoped to the window in front` ([`capture`](keys.md#host-keys-capture), its Windows block); the keypad's Enter is recorded as `"Return"`. A screen reader's hook after the host's sees the key as usual. Whether a DAW hands Return to a plug-in's text field or takes it for itself — REAPER has a per-FX "Send all keyboard input to plug-in" — is the DAW's; it has not been measured with an overlay (TODO.md, VPS Avenger).

### macOS {#trip-handover-macos}

The scope is the frontmost application's window in front, read by accessibility at the call, and the event tap lets the end key through unmodified and records it, the keypad's Enter as `"Return"`. VoiceOver's own chords never reach the tap. Whether a plug-in's text field in Logic gets Return and Escape through the tap is not measured yet (TODO.md, VPS Avenger); the log's `went through` against `was taken` lines answer it. Without the Accessibility grant `capture` answers `nil` and its error value, `denied`, and the hand-over answers that error value.

## O\:holdKeys(spec) {#o-holdkeys}

**Signature:** `O:holdKeys(spec: { keys: { string }, onKey: (overlay, key: string, mods, press) -> boolean?, onEnd: (overlay, how: string, info: table) -> (), swallow: { keys: { string }, say: string }? }) -> true | (false, Error)`

Takes a few keys for the module while the overlay is in front, outside any trip of its page — for what one press of a control begins and later keys finish. VPS Avenger's ring holds a knob pressed and turns it with Page Up and Page Down until Return, as the avenger_control tool does. Since version 0.13 of the runtime; a module that calls it depends on `"com.platform.overlay >= 0.13"`.

- **The keys are the hold's, over every claim of the overlay's own.** A key of `keys` reaches `onKey`, whether the overlay captured it before — Return, which then does not activate the focused control, Space, an arrow a focused control takes — or a control claims it as its `hotkey` — and stays the hold's when the overlay captures it again while the hold stands, Return as the focus moves to another button. The overlay's other keys work as ever: Tab moves the focus — unless `swallow` names it, below — and a control's hotkey that is not one of `keys` activates its control. The module's scope is pinned to the window in front, as at an activation.
- **`onKey(overlay, key, mods, press)`** runs as a handler of the module's, where a read may wait, with the spec as `keys` gave it, the modifiers held (`mods`; `nil` for a key that came as a control's hotkey) and the host's [count of the key's quick repeats](hotkey.md#host-hotkey-register) (`press`). Answering `true` ends the hold.
- **`onEnd(overlay, how, info)`** is told exactly once, whatever ends the hold, before its keys go: `"ended"`, after `onKey` answered `true` (`info.key`, the key), or after `onKey` raised (`info.failed`, its message); `"left"`, when the overlay leaves the front — another window, the page key's page, the module disabled — (`info.why`, the reason its `[deactivate]` line gives), or when it stays in front but on another window it also matches, a second instance of the plug-in, whose recheck finds its origin changed (`info.why`: `the overlay is in front on another window (<id>), not on the one its hold began on (<id>)`) — the hold's keys are scoped to the window it began on and would never come from the other, and what the module holds down belongs to the first; `"replaced"`, when another `holdKeys` call of the overlay begins a hold (`info` empty). Its keys go after it — one that raises is logged, and the keys go all the same — and the overlay, while it is still in front, takes its own captures of them again, since the host keeps one capture of a key for a module ([`host.keys.capture`](keys.md#host-keys-capture)): Return activates the focused control again, Tab moves the focus again. After `"left"` with the front the overlay lets every key go anyway.
- **`swallow = { keys, say }`**, since version 0.16 of the runtime (`">= 0.16"`): keys that do nothing while the hold lasts, as a trip's [hand-over](#trip-handover) swallows them — the same fields and rules. They are the hold's over every claim of the overlay's own, as `keys` are — Tab moves no focus — and are handed to nobody: not `onKey`, not the plug-in, also while a menu flag lets the module's other keys through ([`menu = "taken"`](keys.md#host-keys-capture)). At the first of them `say` is said, interrupting, once per hold; each is logged, `[keys] '<overlay>': Tab swallowed by its hold — it does nothing: said "…"`, later `…, said once already`. Every end gives them back with `keys`: the overlay takes its own captures of them again while it is in front, so Tab moves the focus again.
- Nothing is said but `swallow`'s sentence: the module says what its hold is for. The log says `[keys] '<overlay>' holds PageUp, PageDown, Return, Escape for its module until its onKey ends it` — with `swallow`, `…; Tab, Shift+Tab swallowed meanwhile` — and `[keys] '<overlay>': its hold of … ends (<how>)`.

```luau
-- In a control's onActivate: press the knob and hold it, step it with Page Up and Page Down, let go on Return.
onActivate = function(o)
  local x, y = o:toScreen(120, 340)
  host.input.mouseDown(x, y)
  local dy = 0
  local ok, err = o:holdKeys({
    keys = { "PageUp", "PageDown", "Return", "Escape" },
    -- Tab does nothing while the knob is held, and says once how the hold ends.
    swallow = { keys = { "Tab", "Shift+Tab" }, say = "Return confirms, Escape cancels." },
    onKey = function(o, key)
      if key == "PageUp" or key == "PageDown" then
        dy += key == "PageUp" and -1 or 1
        host.input.move(x, y + dy) -- the button held: a drag of the knob
        return
      end
      return true -- Return or Escape: over
    end,
    onEnd = function(o, how) host.input.mouseUp(x, y + dy) end,
  })
  if not ok then
    host.input.mouseUp(x, y)
    host.log.info("not held: " .. tostring(err))
  end
end,
```

### Returns {#o-holdkeys-returns}

- `true` — every key is captured, and the hold has begun.
- `false, err` — nothing is held: see Errors.
- `onKey`'s answer: `true` ends the hold; anything else keeps it.

### Errors {#o-holdkeys-errors}

None of these is raised. Each comes at the call; what was captured before is released.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not in front, or its group is away (its page in front, a trip under way). | `the overlay is not in front` | both |
| any of [host.keys.capture's errors](keys.md#host-keys-capture-errors) | return | A key — one of `keys` or of `swallow` — could not be captured: no keyboard hook, or on a Mac no event tap without the Accessibility grant (`denied`). The host registered nothing for it. | the host's | both |
| `failed` | return | A capture raised — a key `normalize` read and `capture` did not: the host's own failure. | `capturing … raised: …` | both |

**Raises** for a mistake in the call, naming the caller's line: a `spec` that is not a table or has a field it does not have; an `onKey` or an `onEnd` that is not a function; `keys` that is not a list of 1 to 8 key specs, names a key [`host.keys.normalize`](keys.md#host-keys-normalize) cannot read, or names one key twice; a `swallow` that [`trip:handOver`](#trip-handover-errors) would raise for — not a table, a field other than `keys` and `say`, `keys` not a list of 1 to 8 key specs, a key `normalize` cannot read, one key twice or one of `keys`, a `say` that is not a string or is empty. All of them before anything is captured, also with the overlay not in front.

### Windows {#o-holdkeys-windows}

The keys are captured by the low-level hook, scoped to the window in front, as the overlay's own keys are ([`host.keys.capture`](keys.md#host-keys-capture), its Windows block): a screen reader's hook after the host's never sees them. A key a control claims as its `hotkey` is a `RegisterHotKey` claim, which comes before the hook; the overlay hands it to `onKey` itself.

### macOS {#o-holdkeys-macos}

The keys are captured by the event tap, scoped to the frontmost application's window in front, as the overlay's own keys are. VoiceOver's own chords never reach the tap, so a key with VoiceOver's modifiers held is VoiceOver's. Without the Accessibility grant `capture` answers `nil` and its error value, `denied`, and `holdKeys` answers that error value.

## O.away {#o-away}

**Signature:** `o.away` → `boolean`

Whether the overlay's group is **away**: true from the moment the page took the front from the overlay's window, through every trip, until away ends. A field, to be read only; false on an overlay with no page.

While away, no overlay of the group holds a key of any kind — no hotkey, no captured key, no menu flag, no key scope (the page key is the module's, not the overlay's, and stays) — nothing of it is announced, and its `onActivate` and `onDeactivate` wait ([`O:onActivate`](#o-onactivate)). "Active" keeps its one meaning, holding the slot: an overlay of the group can be in front quietly during a trip, and the arbiter elects as it always does.

**Away begins** in the overlay's leaving the front, for whatever reason it leaves, when the runtime's show at the key asked for the place, the page's window is the one in front, and the overlay was last on the place's control. **Away ends** at the plug-in's window (the user came back, or the way back arrived) — a page over the plug-in, shown and not in front, is hidden then, one beside it is left — at another window of the plug-in's application while the page is in front, at another application's window or one of this application's own, when the page is closed (seen at the next event), and 8 seconds after a way back that did not reach the plug-in's window. As it ends, every overlay of the group in front takes its keys back and says its arrival, and on the next turn each one's hooks are settled: one the module was told is active that is not in front gets `onDeactivate`, one in front the module was not told of — a sibling that came up during a trip — gets `onActivate`. Logged: `[page] '<overlay>': away ends — <why>; in front: <window>`.

A module reads it where it would otherwise take the overlay for gone: a background watcher that may run only while the page is hidden checks `not o.away`.

```luau
-- The plug-in's level, read for the ring every second — not while the page is in front.
host.timer.every(1000, function()
  if ov.active and not ov.away then readLevel(ov) end
end)
```

### Returns {#o-away-returns}

`true` or `false`, never `nil`: `false` from `O.new` on, and on an overlay with no page. The same value on every overlay of the group. A module reads it and never writes it: the runtime decides by it whether the overlay holds any key.

### Windows {#o-away-windows}

As above.

### macOS {#o-away-macos}

As above; the [stay](#o-attachembedded-macos) is held while away.

## O\:pageBack() {#o-pageback}

**Signature:** `O:pageBack() -> true | (false, Error)`

The way back from an overlay's [page](#o-addpage) to the plug-in: what the page key does in the page, and what a page's own "Back" button calls. It is refused first, with nothing asked of the system, while a trip is under way, unless the page has the keyboard, and when no place is held. Then the held control is asked to the front with [`host.window.focus`](window.md#host-window-focus) **first**, while the page still has the front — refused there, the page stays — and only then the page is hidden. Accepted, the group stays away until the plug-in's window comes: another window of the plug-in's application on the way is waited past (Logic's main window first), for at most 8 seconds, after which away ends where the user is. The page's close box, Alt+F4 in it, and Command-W on a Mac are a way back too: the host has hidden the page and given the front back, and the runtime asks for the held control.

`onBack(o, ok, err)`, the page's hook, hears every way back the runtime takes — this call's and the page key's — that got as far as the focus: `true` once the page is hidden, or `false` and the error value. Without `onBack`, a `false` there is said: `"<label>: the plug-in's window would not come to the front."` or `"<label>: the plug-in's window is gone."`. A way back refused before the focus is logged, `[page] '<overlay>': the way back not taken — <word>`, and `onBack` does not hear it.

It runs where it is called, on the event loop, and costs one system call, the focus.

```luau
-- The page's own "Back" button.
onMessage = function(o, m)
  if m.action == "back" then
    local ok, err = o:pageBack()
    if not ok and err.code ~= O.errors.BUSY and err.code ~= O.errors.NOT_IN_FRONT then
      o.page:post({ kind = "error", text = "Back: " .. err })
    end
  end
end
```

### Returns {#o-pageback-returns}

- `true` — the held control was asked to the front and accepted, and the page is hidden. `onBack(o, true)` has run.
- `false, err` — the page stays where it is.

### Errors {#o-pageback-errors}

None of these is raised. Each comes as an error value where "arrives" says, in the order they are decided. The first three ask nothing of the system and do not reach `onBack`; the last three do, `onBack(o, false, err)` with the same error value ([`O:addPage`](#o-addpage-errors)).

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `busy` | return | A trip of the page is under way. Logged `busy`. | `a trip of the page is under way` | both |
| `not_in_front` | return | The page does not have the keyboard ([`foreground()`](window.md#host-window-foreground), read afresh). Logged `not in front`. | `the page does not have the keyboard` | both |
| `not_found` | return | No place is held: the page came forward another way, not by the page key. Logged `nothing held`. | `no place is held: the page came forward another way` | both |
| `gone` | return | The plug-in's window is gone, by the trip's rule ([`O:inPlugin`](#o-inplugin)); `err.detail` says how. | `the plug-in's window is gone` | both |
| any of [host.window.focus's errors](window.md#host-window-focus-errors) | return | The focus of the held control was not accepted, and its own error value is handed on — `refused`, or `gone`, `hidden` or `not_supported` where it says more. | the host's | both |
| `refused` | return | That focus raised, or answered `false` with no error value; `err.detail` is the raise's message. | `the plug-in's window was not brought forward` | both |

**Raises** only for an overlay whose group has no page: `overlay '<label>': pageBack — its group has no page (O:addPage)`. `onBack` runs guarded, logged `[page] '<overlay>': onBack failed: <message>`.

### Windows {#o-pageback-windows}

The held control is the plug-in's child window, and the focus lands the keyboard in it ([`host.window.focus`](window.md#host-window-focus), HW1). The page's own hand-back gives the keyboard back to the control it took the front from as well ([`page:hide`](gui.md#page-hide)).

### macOS {#o-pageback-macos}

The held control is the DAW's panel, whose id is its window's. Where VoiceOver lands after the way back is unmeasured; with the keyboard on Logic's header the overlay does not come up, and the stay ends there.

## O\:pageKey(spec) {#o-pagekey}

**Signature:** `O:pageKey(spec: string) -> true | (false, Error)`

Registers another page key for an overlay's [page](#o-addpage) and gives up the old one, for a setting. Refused, the old key stays registered. **It never answers a partial answer:** a key the host claims but cannot hold now — [`host.hotkey.register`](hotkey.md#host-hotkey-register)'s `id, err` — is let go again at once, and the call answers `false, err` with register's error value, since a page key that does nothing is no key; logged `[page] '<overlay>': the page key stays <old>: <spec> is not held — <message>`. Accepted, it logs `[page] '<overlay>': the page key is <spec> now, was <old>`.

It runs where it is called, on the event loop, and costs one [`host.keys.check`](keys.md#host-keys-check) of the spec (remembered for the VM's life) and one registration.

```luau
-- A setting for the page key.
host.settings.onChange("pageKey", function(new)
  local ok, err = ov:pageKey(new)
  if not ok then
    if err.code == O.errors.CLASH then host.speech.output("That key belongs to a control of the overlay.") end
    host.log.info("the page key stays as it was: " .. err)
  end
end)
```

### Returns {#o-pagekey-returns}

- `true` — the new key is registered and held, and the old one is given up.
- `false, err` — the old key stays; nothing of the new one is kept.

### Errors {#o-pagekey-errors}

None of these is raised. Each comes as an error value where "arrives" says, in the order they are decided.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `invalid` | return | [`host.keys.check`](keys.md#host-keys-check) says `parse`: the spec does not read as a key. | `… is not a key spec` | both |
| `invalid` | return | It says `tap`: a modifier on its own, which a hotkey cannot be. | `… is a modifier tap, which only host.keys.capture can watch` | both |
| `taken` | return | It says `reserved`: the system keeps the combination. | `… is kept by the system` | both |
| `not_supported` | return | It says `no_keycode`: this platform gives the key no key code of its own — `F21`–`F24` on a Mac, which have none, and `NumpadEnter` on Windows, which has Enter's. | `… has no key code of its own on this platform` | both |
| `clash` | return | The key is, by [`host.keys.normalize`](keys.md#host-keys-normalize), the hotkey of a control or a tab of an overlay of the group; the message names it, `control '<label>'` or `tab '<label>'`, and the overlay. | `… is the hotkey of … of overlay '…'` | both |
| `failed` | return | The registration raised — a spec `check` read and `register` did not: the host's own failure. | `registering … raised: …` | both |
| any of [host.hotkey.register's errors](hotkey.md#host-hotkey-register-errors) | return | The host will not hold it now: another program, module or the system holds it (`taken`), the layout types no such letter (`not_supported`), and the others register answers. Whether register answered `nil, err` or `id, err`, nothing is kept. | the host's | both |

**Raises** for a mistake in the call: a `spec` that is not a string (`overlay '<label>': pageKey takes a key spec, a string, got number`) — before version 0.11 that answered `false, "parse"` — and an overlay whose group has no page (`overlay '<label>': pageKey — its group has no page (O:addPage)`).

## O\:pageAgain() {#o-pageagain}

**Signature:** `O:pageAgain() -> Page`

Closes an overlay's [page](#o-addpage) and describes it again, for a setting fixed per page object (`activates`, `window`): a trip under way is given up (`closed`), [`page:close()`](gui.md#page-close) gives the front back first where the page was in front, `describe` is called again with `addPage`'s checks, and `o.page` is the new page. Away ends by what is in front. The new page is prepared at once where the overlay is in front and not away, otherwise at its next arrival.

It runs where it is called, on the event loop, and costs a page closed and described.

```luau
host.settings.onChange("pageBeside", function()
  ov:pageAgain()   -- describe reads the setting, and the new page takes it
end)
```

### Returns {#o-pageagain-returns}

The new page, which `o.page` is from now on. The old one is closed.

### Errors {#o-pageagain-errors}

None. It never answers an error value. A trip it gives up hears `closed` in its `onGiveUp` ([`O:inPlugin`](#o-inplugin-errors)).

**Raises** for an overlay whose group has no page (`overlay '<label>': pageAgain — its group has no page (O:addPage)`), and as `addPage` raises for its `describe`: one that raised, answered no page, or did not read all of `on`. A `describe` that raises leaves the old page closed and `o.page` the old object.

## O.calibrating() {#o-calibrating}

**Signature:** `O.calibrating() -> boolean`

Whether calibration is switched on: `true` while **Calibration keys and pictures in overlays, for writing an overlay** is ticked in the overlay runtime's own settings — in the [module manager](../module-manager.md#settings), the Installed list, **Overlay runtime**, **Settings…** — and `false` otherwise, which is how it starts. It never raises and never answers `nil`.

It exists so that a measuring instrument is not also a feature. The runtime arms its [calibration keys](../building-an-overlay.md#calibrator), takes its menu shots and saves the picture a failed image search looked at only while it is on, so a normal user never has those combinations taken away from their plug-in or pictures written behind their back; and a module gates measurements that are too expensive to run for somebody who is not measuring — a capture per selection change is an instrument, not a behaviour.

**One switch for every module built on the runtime.** It is the runtime's own setting, `calibrate`, stored under the runtime's id `com.platform.overlay` in `settings.toml` beside the application ([a shared runtime's settings](settings.md)), whichever module's code asks. A module asks this function; it does not define a setting of its own for it.

**It takes effect immediately**, with no reload. Each module built on the runtime keeps the value in a variable of its runtime's code, which the setting's [`onChange`](settings.md#host-settings-onchange) updates, and this call only reads it: no host call, on whichever thread runs the module's Luau (the event loop), in well under a microsecond — cheap enough to ask at every measurement rather than once at load, which is what makes a change reach code that has already loaded. The dialog's OK reaches every module in the turn after it, about 15 ms later, as one of the module's [handlers](../module-runtime-and-lifecycle.md#handlers): behind a handler of that module that waits for a read, so until then that module still answers the old value. What is decided by an answer follows it from then on — the menu shots, the menu item shot, the picture of a missed search and a module's own instruments. **The three keys are taken when an overlay comes to the front**: switched on, they arm the next time an overlay comes to the front; switched off, an overlay already in front keeps them until it leaves, and the next one to come to the front does not take them. While the module manager is in front to be ticked in, no overlay is.

The log says each step. A run that starts with calibration stored on says so at its first `define`, before any overlay comes to the front: `[settings] com.platform.overlay calibrate is true as stored (its default is false)`. The host writes `[settings] com.platform.overlay calibrate is now true` (or `false`) when OK changed it; each module that has made overlays writes `[calibrate] on — <n> overlay(s) here, <m> in front; the keys arm the next time an overlay comes to the front` (`[calibrate] off — …; the one in front keeps its keys until it leaves`, or `…; no overlay holds the keys`); and the first overlay of a module to come to the front with calibration on writes `[calibrate] armed — <the S key> shot, +T template, +V verify (active overlay)` once, and again after calibration has been switched off and on.

**Without a window to click in** — a headless run — there is no dialog: write the setting into `settings.toml` beside the application while the application is not running (it replaces the file whole when it saves), and start it.

```toml
[modules."com.platform.overlay".settings]
calibrate = true
```

```luau
local O = host.require("com.platform.overlay")

-- A module's own instrument: what a read of the header said, written down only for somebody
-- measuring. Asked when the answer arrives, not once at load, so ticking the setting takes effect
-- at the next answer.
local function headerRead(reading)
  if O.calibrating() then
    host.log.info("[my-plugin] header: " .. tostring(reading.text))
  end
end
```

`O.calibrating` came with version 0.4 of the runtime: a module that asks it depends on `"com.platform.overlay >= 0.4"`, and an older runtime then fails its load instead of its first call.

**The calibration and capture methods** an overlay has — [`calibrationShot()`](#o-calibrationshot), [`calibrationCrop(size?)`](#o-calibrationcrop) and [`calibrationVerify()`](#o-calibrationverify), which the three calibration keys call on the overlay in front, and [`captureControl`](#o-capturecontrol), [`captureRegion`](#o-captureregion), [`captureFull`](#o-capturefull) and [`captureAll`](#o-captureall), which a module's own measuring calls — answer why they did not, since version 0.11 of the runtime; before, a bare `false`. The keys drop the answer, each method having logged what it did. Each is an entry below.

**The menu shots.** While calibration is on, activating an overlay control declared with `opensMenu` saves three pictures — no crosshairs — of the overlay's origin whole: the plug-in's control when it is embedded, the window with its frame when it is standalone (the origin's `bounds`, see [`O:origin()`](#o-origin)). The rectangle is read once, just before the control acts, and all three pictures use it, so they line up pixel for pixel; the part of a popup that reaches past it is not in them. **One screen capture is all that stands between the key and the click**: the first picture is a [snapshot](screen.md#host-screen-snapshot) taken on the press — on the `screen-capture` thread, which the press's handler [waits](screen.md#where-it-waits) for — and kept. The control acts only while the overlay is still where the press was made once the picture is in — active, in the same stay in front, on the same window ([`O:stillHere`](#o-stillhere) without options); otherwise nothing is clicked or chosen, and the log says why: `[calibrate] '<overlay>' '<control>': not acting after the menu shot — <why>`, in `stillHere`'s words. A single point is also asked again before its click, as after any read. A press that does not act counts for nothing: its picture is let go unwritten, the later two are not asked for, and it ends no item the last press waits to choose, sets no [menu test](#o-menutests) running and is not one of the control's two photographed openings (below). Nothing else is looked up on the press — no control's visibility or point. The later two are taken off the event loop at their time, counted from just before the click ([`host.screen.snapshot`](screen.md#taken-at-a-set-time) with a callback and `at`) and each is written when it arrives; the first is written after the second of them, not straight after the click, which is when the menu's first answer, its window coming to the front and the user's first key arrive. So a key pressed while the menu opens waits for no capture and no PNG, and one pressed at about 600 or 1500 ms waits for one or two PNGs being written. The log lines come in the order the pictures are written — 600 ms, 1500 ms, then the one of the press:

| File, under `modules/overlay-runtime/calibration/` | Taken |
| --- | --- |
| `<overlay>-<control>-menu-before.png` | just before the control acts |
| `<overlay>-<control>-menu-after-600.png` | about 600 ms after |
| `<overlay>-<control>-menu-after-1500.png` | about 1500 ms after |

`<overlay>` and `<control>` are the two labels with every run of characters other than letters and digits turned into `-`. A later press of the same control is numbered — `<overlay>-<control>-2-menu-before.png`, then `-3-` — counting from the files already on disk, so nothing is overwritten, across restarts too. **Only the first two openings of each control are photographed** — per overlay, until the module is reloaded: what a menu looks like open does not change on the tenth opening, and a session of days calibrating wrote three PNGs at every one. The third opening writes one `[calibrate]` line saying the later ones are not photographed (`… menu shots were taken of its first 2 openings; later ones are not photographed until the module is reloaded`), and nothing after that; the control acts as always. Each picture writes a `[calibrate]` log line with its rectangle, where the plug-in's content begins on screen (`content at (x,y)`, the point the overlay's coordinates count from, so a pixel in the picture is at content point *picture x + left − content x*), its absolute path and `(true)`, or `(false: …)` with the reason it was not saved — a capture refused for the [snapshot budget](screen.md#host-screen-snapshot) included; a picture that fails stops neither the control nor the other two. The later two are taken whatever happened in between, including when the overlay has left the front; a press on an overlay whose window is not known writes one line saying so and no pictures. The timing is part of the measurement only: nothing in the runtime reads these pictures.

They are what a module's own [menu test](#o-menutests) is written from. A menu a plug-in paints inside its own window has no window, no accessibility element and no notification, and the difference between the "before" picture and the "after" ones is the pixels that only an open menu has.

**The menu item shot.** A control that chooses an item in its menu ([`O:chooseMenuItem`](#o-choosemenuitem), or a hotspot with `menuItem`) clicks the item on the pass in which a test sees the menu, so the pictures above — the later two taken 600 and 1500 ms after the press — show the menu already closed. On a press that was photographed (the first two of each control), the item is photographed as well: one [snapshot with a callback](screen.md#snapshot-with-a-callback) of the menu's rectangle as the test gave it, 12 pixels round, asked on the pass in which the test sees the menu — which cannot wait for a picture — and the item's first click is made once the picture is in, after the checks a click made again gets: the item still waiting for its menu, the overlay still on its window, nothing drawn over the item. On those presses the item is clicked about one tick later than on the others. The picture is written on the pass after its answer, with a crosshair on the point clicked, whether the click was made or not. A click the menu did not take and that is made again is not photographed again:

| File, under `modules/overlay-runtime/calibration/` | Taken |
| --- | --- |
| `<overlay>-<control>-menu-item.png` | before the item is first clicked, which waits for it |
| `<overlay>-<control>-submenu-item.png` | for an item with a `submenu`, of the submenu with its item, before that is first clicked, which waits for it |

numbered like the other pictures (`<overlay>-<control>-2-menu-item.png`, …). Its log line gives the rectangle captured, the item's point and the path: `[calibrate] '<overlay>' '<control>': menu item shot, (x1,y1)-(x2,y2), the item marked at (x,y) -> <path> (true)` (`submenu item shot` for the submenu's) — or `(false: …)` with the reason, and the click is made either way; a picture that cannot be asked for gives that line its reason, and the item is clicked at once. It is the picture an item's offset is measured from: the offset is from the menu's top-left corner, which is the picture's corner plus 12.

```luau
-- Nothing to call: the control only has to say that it opens a menu.
ov:addHotspotButton({ label = "Preset menu", at = { 412, 118 }, hotkey = "Alt+M", opensMenu = true })
-- With calibration on, Alt+M on the overlay "Diva" writes
--   Diva-Preset-menu-menu-before.png
--   Diva-Preset-menu-menu-after-600.png
--   Diva-Preset-menu-menu-after-1500.png
-- and the next press Diva-Preset-menu-2-menu-before.png and its two companions. The log says
--   [calibrate] 'Diva' 'Preset menu': menu shot just before the control acts,
--     (120,90)-(1120,742), content at (120,90) -> C:\…\Diva-Preset-menu-menu-before.png (true)
```

### Returns {#o-calibrating-returns}

`true` while calibration is switched on, `false` otherwise — never `nil`. A read of a variable: it never raises, and asks the host nothing.

### Windows {#o-calibrating-windows}

`settings.toml` is next to the `.exe`. The keys are Ctrl+Alt+Shift+S, T and V.

The capture before the click goes through the module's [source](screen.md#which-picture-a-read-sees), on the `screen-capture` thread: through the standard path about one compositor frame (~16.7 ms) for a plug-in-sized rectangle, and the turn of the event loop that hands it to the press. Writing a PNG is the larger cost of a shot — several times larger in a debug build than in a release build — and it runs on the image worker when a picture is written, while the runtime's handler waits for it and the module's keys wait behind it: never between the key and the click, and not before the 600 ms picture has arrived. The menu item shot is one capture of the same kind, of the menu's rectangle, taken on the `screen-capture` thread between the menu being seen and the item's click, which waits for it, and its PNG is written on the pass after the click.

### macOS {#o-calibrating-macos}

`settings.toml` is in the folder that holds the `.app` (see [`host.settings.set`](settings.md#host-settings-set) for a translocated copy). The keys are Command+Option+Shift+S, T and V — a spec's Ctrl is Command there — off Control+Option, which is VoiceOver's.

The capture before the click is taken by ScreenCaptureKit on the `screen-capture` thread, which the press waits for, at the display's own resolution, and kept at up to five times the bytes of the rectangle (see [`host.screen.snapshot`](screen.md#host-screen-snapshot)), for the 1.5 seconds until the last picture has arrived; the pictures of a maximised window held at once come close to a module's snapshot budget, and a capture the budget refuses is written as `(false: …)`. The menu item shot is one more capture, of the menu's rectangle only, taken by ScreenCaptureKit on the `screen-capture` thread while the item's click waits for it, and held until the next pass writes it. Without the Screen Recording permission the pictures show the wallpaper.

## O\:calibrationShot() {#o-calibrationshot}

**Signature:** `O:calibrationShot() -> true | (false, Error)`

What the calibration key Ctrl+Alt+Shift+S does on the overlay in front ([calibration keys](../building-an-overlay.md#calibrator)): a picture of the coordinate window with a **crosshair on every control where it will actually click**, and the pixel read there, and the same picture without the crosshairs. It writes `calibration/<overlay>.png` and `calibration/<overlay>-clean.png` under the overlay runtime's own folder (`modules/overlay-runtime/calibration/`), the overlay's label with every run of characters other than letters and digits turned into `-`; a later shot of the same overlay is numbered, `<overlay>-2.png`, counting from the files already on disk. The log says the origin, the rectangle captured and the path, then which context the overlay took as its origin and what else was on offer, then one line per control: its number, label, screen point and pixel, or ``hidden (its `when` is false)``, or `no position`. The pixels under every crosshair are read in one capture; a read that fails leaves every pixel `?` and says why first.

**A scaled overlay's calibration shot** ([`O:scale`](#o-scale)). Its first line adds the factor its crosshairs were placed with and the frame, `… scale 1.6000 about (0,0), frame (0,0) -> …`, or `no scale now (<why>)`; each control's line adds every region it reads as the screen rectangle, `region (x1,y1)-(x2,y2)` (and `ocrLabel (…)`), since the authored numbers are not where it reads. A control with no factor now has no crosshair and is listed with `no position`. In any overlay, the line of a control that chooses a menu item ends `then item (dx,dy) of its menu` (or `then an item of its menu`, for a `menuItem` function): the opener is what the crosshair marks, and the item is in the menu item shot. An opener a module's own code has asked [`O:chooseMenuItem`](#o-choosemenuitem) to click — a stepper's, which no control's `at` names — is marked too once it has been asked for, at the `at` it was last given, after the controls, in label order: `  n <label> screen (x,y)  pixel r,g,b  [menu opener]` (or `-- <label> no position  [menu opener]` when it cannot be placed now).

**Cost.** One capture of the crosshairs' pixels, two captures and two PNGs written — the handler waits for each, on the `screen-capture` thread and the image worker — and, for the context lines, the window's controls, its focus chain and a dump of the origin's accessibility tree.

```luau
-- A module's own key for a shot, beside the calibration key, while it is being written.
host.hotkey.register("Ctrl+Alt+Shift+F12", function()
  if not ov.active then return end
  local ok, err = ov:calibrationShot()
  if not ok then host.log.info("[my-module] no shot: " .. err) end
end)
```

### Returns {#o-calibrationshot-returns}

- `true` — the marked picture was written. The clean one's answer is not checked.
- `false, err` — the marked picture was not written, or nothing could be placed.

### Errors {#o-calibrationshot-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `unplaced` | return | The overlay has no origin now — it is not active, or its context finds nothing; logged `[calibrate] no active origin`. | `the overlay has no origin now` | both |
| any of [host.screen.saveMarked's errors](screen.md#host-screen-savemarked-errors) | return | The marked picture was not taken or the file not written. | the host's | both |

**Raises** what `host.screen.saveMarked` raises for its arguments, which the runtime makes itself: nothing for a mistake of the module's, since it takes none.

**Raises where it cannot wait**: called with an origin where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the `host.screen.pixels` inside it raises, `host.screen.pixels cannot wait here: …` — or `host.screen.saveMarked`, with no crosshair to read — and nothing is written; see [Where it waits](screen.md#where-it-waits). The calibration key calls it in a handler.

## O\:calibrationCrop(size?) {#o-calibrationcrop}

**Signature:** `O:calibrationCrop(size: number?) -> true | (false, Error)`

What the calibration key Ctrl+Alt+Shift+T does: crops `size` pixels (28 by default) square around the **focused** control's point and writes it as a template, `calibration/<label>.png` under the overlay runtime's folder, the control's label with every character other than a letter or a digit turned into `-`. The point is where the calibration shot puts its crosshair: a hotspot's `at`, a slider's thumb or a graphical button's hit where they are found, otherwise its region's middle. A later crop of the same label **replaces** the file. The overlay must be active, as for the capture methods: an overlay not in front would be cropped from the origin it last had (logged `[calibrate] the overlay is not active, so nothing is cropped`; before version 0.12.1 of the runtime it was cropped all the same). Logged `[calibrate] cropped <label> around (x,y) -> <path> (<answer>)`.

```luau
local ok, err = ov:calibrationCrop(40)   -- a wider template around the focused control
if not ok then host.log.info("[my-module] not cropped: " .. err) end
```

### Returns {#o-calibrationcrop-returns}

- `true` — the template was written.
- `false, err` — it was not.

### Errors {#o-calibrationcrop-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not active. | `the overlay is not active` | both |
| `not_found` | return | No control is focused. | `no control is focused` | both |
| `unplaced` | return | The focused control has no point now: none to place, a scale with no factor, a region function with no answer. | `the focused control has no position to crop around` | both |
| any of [host.screen.save's errors](screen.md#host-screen-save-errors) | return | The picture was not taken or the file not written. | the host's | both |

**Raises** for a `size` that is not a number (Luau's own arithmetic error), and what an `at` function raises, as at a press.

**Raises where it cannot wait**: called with a point to crop around where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the `host.screen.save` inside it raises, `host.screen.save cannot wait here: …`, and nothing is written; see [Where it waits](screen.md#where-it-waits). The calibration key calls it in a handler.

## O\:calibrationVerify() {#o-calibrationverify}

**Signature:** `O:calibrationVerify() -> number | (nil, Error)`

What the calibration key Ctrl+Alt+Shift+V does: counts every match of the **focused** control's template across the coordinate window — its `onImage` (or `offImage`), or else the crop [`calibrationCrop`](#o-calibrationcrop) wrote for its label, `not_found` when there is none — with a tolerance of 8. Exactly one is what a control wants before it clicks blindly; two means it will one day click the wrong one, and none that it never fires. Logged `[calibrate] <label>: <n> match(es) for <template>`, then one line per hit.

```luau
local n, err = ov:calibrationVerify()
if n == nil then host.log.info("[my-module] not counted: " .. err)
elseif n ~= 1 then host.log.info("[my-module] the template matches " .. n .. " times") end
```

### Returns {#o-calibrationverify-returns}

- A number from `0` — the matches counted. `0` is an answer: the search was made and found none.
- `nil, err` — nothing was counted.

### Errors {#o-calibrationverify-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_found` | return | No control is focused. | `no control is focused` | both |
| `unplaced` | return | The overlay has no origin now. | `the overlay has no origin now` | both |
| `not_found` | return | The control has no `onImage` or `offImage`, and no crop of its label is in the runtime's `calibration/` folder; the message names the label and the path. Logged `[calibrate] <label>: no matches counted — no crop of it at <path>`. Before version 0.12.1 of the runtime the search raised for it. | `no crop of '…' was written: … is not there` | both |
| any of [host.screen.imageSearchAll's errors](screen.md#host-screen-imagesearchall-errors) | return | The search could not be made — logged `[calibrate] <label>: no matches counted for <template> — <message>`, not as "0 match(es)". | the host's | both |

**Raises** what `host.screen.imageSearchAll` raises for a template it cannot load — a control's own `onImage` or `offImage` that is missing inside a package ([Return or raise](errors.md#return-or-raise)). A crop is calibration output, not part of the runtime's package, and its absence is answered.

**Raises where it cannot wait**: called with a template to count where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the `host.screen.imageSearchAll` inside it raises, `host.screen.imageSearchAll cannot wait here: …`, and nothing is counted; see [Where it waits](screen.md#where-it-waits). The calibration key calls it in a handler.

## O\:captureControl(label, path) {#o-capturecontrol}

**Signature:** `O:captureControl(label: string, path: string) -> true | (false, Error)`

Writes the live pixels of the control labelled `label` — the first with a `region` — to `path`, an absolute file path, as a PNG: its region placed now, through the origin, the frame and the scale. For re-capturing an image control's on and off templates against the real plug-in. The overlay must be active, so the origin is the plug-in in front. Logged `captureControl '<label>' region=(x1,y1,x2,y2) -> <path> ok=<answer>`.

```luau
ov:captureControl("Reverb", host.path("images/FxRack/ReverbOn.png"))
```

### Returns {#o-capturecontrol-returns}

- `true` — the picture was written.
- `false, err` — it was not.

### Errors {#o-capturecontrol-errors}

None of these is raised. Each comes as an error value where "arrives" says, in the order they are decided.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not active. | `the overlay is not active` | both |
| `unplaced` | return | The control's `region` is a function, and it raised. | `its … function raised: …` | both |
| `unplaced` | return | The control's `region` function answered `nil`. | `its … function answered nil` | both |
| `unplaced` | return | The control's `region` function answered something other than four numbers. | `its … function answered …, not {x1, y1, x2, y2}` | both |
| `unplaced` | return | A scaled overlay with no origin now. | `the overlay has no origin now` | both |
| `unplaced` | return | A scaled overlay whose scale function raised. | `its scale function raised: …` | both |
| `unplaced` | return | A scaled overlay whose scale function answered `nil`. | `its scale function answered nil` | both |
| `unplaced` | return | A scaled overlay whose scale function answered no factor above 0. | `its scale function answered …, not a factor above 0` | both |
| `not_found` | return | No control with that label has a `region`. | `no control labelled '…' with a region` | both |
| any of [host.screen.save's errors](screen.md#host-screen-save-errors) | return | The picture was not taken or the file not written. | the host's | both |

**Raises** what `host.screen.save` raises for a `path` it does not take.

**Raises where it cannot wait**: called with a region to write where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the `host.screen.save` inside it raises, `host.screen.save cannot wait here: …`, and nothing is written; see [Where it waits](screen.md#where-it-waits).

## O\:captureRegion(region, path) {#o-captureregion}

**Signature:** `O:captureRegion(region: {number, number, number, number}, path: string) -> true | (false, Error)`

Writes an authored rectangle `{x1, y1, x2, y2}` of the overlay — placed as [`O:toScreenRect`](#o-toscreenrect) places it, without `whole` — to `path`, an absolute file path, as a PNG: a wider shot for calibration or inspection. The overlay must be active. Logged `captureRegion (x1,y1,x2,y2) -> <path> ok=<answer>`.

```luau
ov:captureRegion({ 0, 0, 400, 120 }, host.path("calibration/header.png"))
```

### Returns {#o-captureregion-returns}

- `true` — the picture was written.
- `false, err` — it was not.

### Errors {#o-captureregion-errors}

None of these is raised. Each comes as an error value where "arrives" says, in the order they are decided.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not active. | `the overlay is not active` | both |
| `unplaced` | return | A scaled overlay with no origin now. | `the overlay has no origin now` | both |
| `unplaced` | return | A scaled overlay whose scale function raised. | `its scale function raised: …` | both |
| `unplaced` | return | A scaled overlay whose scale function answered `nil`. | `its scale function answered nil` | both |
| `unplaced` | return | A scaled overlay whose scale function answered no factor above 0. | `its scale function answered …, not a factor above 0` | both |
| any of [host.screen.save's errors](screen.md#host-screen-save-errors) | return | The picture was not taken or the file not written — corners turned around included, which read as an empty region (`failed`, `screen capture failed`). Fractional corners are cut toward zero and the picture written. | the host's | both |

**Raises** for a `region` that is not a table of four numbers (Luau's own error), and what `host.screen.save` raises for a `path` it does not take.

**Raises where it cannot wait**: called with the overlay active where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the `host.screen.save` inside it raises, `host.screen.save cannot wait here: …`, and nothing is written; see [Where it waits](screen.md#where-it-waits).

## O\:captureFull(path) {#o-capturefull}

**Signature:** `O:captureFull(path: string) -> true | (false, Error)`

Writes the **whole** plug-in in front — the origin's client area's corner, as large as its `bounds` — to `path`, an absolute file path, as a PNG: for locating controls and working out coordinates and offsets again. The overlay must be active. Logged `captureFull client=(x,y) bounds=<w>x<h> -> <path> ok=<answer>`.

```luau
ov:captureFull(host.path("calibration/whole.png"))
```

### Returns {#o-capturefull-returns}

- `true` — the picture was written.
- `false, err` — it was not.

### Errors {#o-capturefull-errors}

None of these is raised. Each comes as an error value where "arrives" says, in the order they are decided.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not active. | `the overlay is not active` | both |
| `unplaced` | return | It has no origin now, or one without a `client` and `bounds`. | `the overlay has no origin now` | both |
| any of [host.screen.save's errors](screen.md#host-screen-save-errors) | return | The picture was not taken or the file not written. | the host's | both |

**Raises** what `host.screen.save` raises for a `path` it does not take.

**Raises where it cannot wait**: called with an origin where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the `host.screen.save` inside it raises, `host.screen.save cannot wait here: …`, and nothing is written; see [Where it waits](screen.md#where-it-waits).

## O\:captureAll(dir) {#o-captureall}

**Signature:** `O:captureAll(dir: string) -> true | (false, Error)`

A shot of the whole plug-in's content and a small one round every positioned control, into the folder `dir` (an absolute path), so coordinates worked out again can be checked against the live plug-in in one go: `content-full.png`, the authored rectangle `{0, 0, 640, 430}` — in an overlay without a scale, a pixel at `(x, y)` in it is the authored content point `(x, y)` — and `ctl-<label>.png` for each control with a label, a table `at` or a table `region`, and no `rawOrigin`, 68 by 52 authored units round its point (the label's characters other than letters and digits turned into `_`). Each is taken as [`O:captureRegion`](#o-captureregion) takes it. **Every shot is tried**; one that is not written stops none of the others. The overlay must be active.

```luau
local ok, err = ov:captureAll(host.path("calibration/all"))
if not ok then host.log.info("[my-module] not every shot was written: " .. err) end
```

### Returns {#o-captureall-returns}

- `true` — every shot was written.
- `false, err` — at least one was not; `err` is the **first** that was not written, as `captureRegion` gave it. The others were tried, and those that could be written were.

### Errors {#o-captureall-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_in_front` | return | The overlay is not active; nothing is tried. | `the overlay is not active` | both |
| any of [O:captureRegion's errors](#o-captureregion-errors) | return | The first shot that was not written: its placement in a scaled overlay with no factor or no origin, or the picture or the file. | the runtime's or the host's | both |

**Raises** what `host.screen.save` raises for a `dir` that makes a `path` it does not take.

**Raises where it cannot wait**: called with the overlay active where a read cannot wait — outside every handler, in a function the host calls and waits for, or in a coroutine of the module's own — the first `host.screen.save` inside it raises, `host.screen.save cannot wait here: …`, and no shot after it is tried; see [Where it waits](screen.md#where-it-waits).

## Plugin base + library overlays (the cell model) {#plugin-base-library-overlays}

A plugin is not one overlay. It is one overlay per **cell** — per combination of things
that are fixed for as long as the window exists: which version it is, and where it runs
(embedded in a host, wrapped in another plugin, standalone). Each cell gets its own
binding, its own coordinates, and its own control set, and the arbiter decides between
them once. Use a `when` predicate only for what changes *while* an overlay is active
(which tab is front, whether a browser is open, whether a library is loaded) — the
arbiter cannot re-elect on those without tearing the overlay down, which a
screen-reader user hears.

Kontakt is the worked example: `{Kontakt 7, Kontakt 8} × {in a DAW, in Komplete Kontrol,
standalone}` = six overlays, declared by looping over a table of cells
(`modules/kontakt/src/cells.luau`). They share one arbiter slot per environment, so
Komplete Kontrol's own chrome (`O.layer.chrome`), a Kontakt header (`base`), a loaded
library (`content`) and a modal dialog (`dialog`) compose without knowing about each
other.

A sample-library module depends on `com.platform.kontakt` and calls
`kontakt.library(name, landmark, build)`, which builds one inheriting overlay per cell —
each carrying that cell's header plus the library's own controls, gated on the landmark
and anchored to it. `build(ov)` runs once per cell, so it must only add controls:

```luau
local kontakt = host.require("com.platform.kontakt")
kontakt.library("Cinematic Studio Strings", host.path("images/CSS/Product.png"), function(ov)
  ov:addStaticText("Cinematic Studio Strings")
  for i, fallback in ipairs({ "Spot 1 Mic", "Spot 2 Mic", "Main Mic", "Room Mic", "Mix" }) do
    local x = -130 + (i - 1) * 40
    ov:addHotspotToggle({
      label = fallback,
      at = { x, 350 },
      ocrLabel = { x - 24, 358, x + 24, 376 },
      onColor = { 180, 165, 230 },
      offColor = { 77, 79, 85 },
    })
  end
end)
```

Splitting a module across files is what keeps this readable: see
[`host.include`](./include.md#host-include). Kontakt separates detection,
the cell matrix, the per-version geometry, what a control does, and what a cell contains.

See [Nested overlays design](../nested-overlays-design.md).
