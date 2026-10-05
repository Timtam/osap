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

A control is spoken as `"label, type[, value]"`, and **every kind is a focus stop**: static text is Tab-reachable and read aloud, it simply has no activation.

A control's hooks are the module's code, and the runtime calls them guarded — and in one of three places, which decides whether a hook may wait for a read there: see [Where a hook runs](#where-a-hook-runs). A `text` — and a tab control's `current` and `verticalName` — that raises gives **no value**: the control is announced as if the hook had answered `nil`, the read before a stepper's step or activation is `nil`, and a stepper's watch takes it as nothing read ([`O:watch`](#o-watch)). It is logged once per control and hook, however often the control is announced: `[overlay] '<label>': its text raised: <message>` (`its current raised`, `its verticalName raised`). A `when` that raises hides that one control, logged once as ``<overlay>: '<label>' — its `when` failed (<message>); hiding just this control``.

```luau
local O = host.require("com.platform.overlay")
```

Source: `modules/overlay-runtime/src/main.luau`.

## What to declare {#declare}

Not a capability — a **dependency**:

```toml
dependencies = ["com.platform.overlay"]
```

An `"overlay"` entry under `[capabilities] require` is a leftover from when the overlay was part of the host; `host.overlay` no longer exists. See [what that list is and is not](./index.md#capabilities).

## Where a hook runs {#where-a-hook-runs}

Every callback of a module runs as a [handler](../module-runtime-and-lifecycle.md#handlers) of the host, one at a time per module, and the runtime calls an overlay's hooks inside them. A hook runs in one of three places, and the place decides what it may do:

| Hook | Runs | May it wait for a read? |
|---|---|---|
| a control's `text`; a tab control's `current` and `verticalName` | in the handler of the key, timer or window event that announces the control | yes |
| a control's `onActivate`; a stepper's `onStep` | in the handler of the key or hotkey that pressed it | yes |
| a stored [`identify`](#o-attachembedded) — one whose verdict is kept — asked for a control it has no verdict for | in the handler of the event that resolves the overlay's origin; in the arbiter's call, a plain call, when that is the first to resolve it — an overlay on a `slot` brought to the front by the arbiter choosing again, after another claimant left | yes, but not in the arbiter's call |
| `when`; an `identify` asked at every evaluation (`cacheIdentity = false`, or a `control` function's where the DAW gives no panel); a binding's `present`; a [menu test](#o-menutests) | in a coroutine of the runtime's own, one per scan or tick | no: it must answer at once |
| a `menuItem` function and the `onDone` of [`O:chooseMenuItem`](#o-choosemenuitem) | inside the menu tick's coroutine when a test answers at once; otherwise in the handler of the test's callback, and `onDone` also in a key's handler, or in the arbiter's call as the overlay leaves the front | no: it must answer at once |
| the overlay's own [`onActivate` and `onDeactivate`](#o-onactivate) | on a `slot`, in the arbiter's call, a plain call; without one, in the handler of whatever looked at the overlay again — a window or focus event, a `pollMatch` poll, the runtime's own timers (the first evaluation of a binding, the check 250 ms after it came to the front), the menu tick | no: on a slot nothing can wait there, and either way the overlay is half way into or out of the front |
| a [library](#o-gate)'s `gate` | in the handler of whatever looked at the overlay again, outside any scan | yes, but a read with a callback keeps the module's keys free |

**"Wait for a read"** is [`host.ocr.recognize`](ocr.md#host-ocr-recognize) without a callback: where the table says yes, it stops the handler until the reading comes, and only the overlay's module waits — its next key runs after it ([Where it waits](ocr.md#where-it-waits)). With a callback it waits nowhere, and is fine in every row. What the rows that say no rule out: a `when` is asked for every control on every Tab and a menu test on every tick, and a module that waited there would be busy all the time.

**Where it may wait**, the overlay checks afterwards that what it was doing still stands. A sentence notes where the overlay is before its control's first hook runs — in front, in which stay, on which window, the focus and the control, the keys of its own, the announcements, the menus opened — and after each of `text`, `current` and `verticalName` it asks whether that still holds, as [`O:stillHere`](#o-here) does with `focus`, `keys` and `said`, and a menu opened since (a menu that was open before the hook does not count against it). When it no longer holds, nothing of the sentence is said, the log says `[read] '<label>' not spoken after its text: <why>` (`after its current`, `after its verticalName`) in `stillHere`'s words, and an OCR button's click is not made: `[read] '<label>': not clicking — <why>`. Without a wait nothing changes between the two, and the sentence is said as it always was. A module's own `onActivate` or `onStep` that reads and then acts — as VPS Avenger's preset steps read the name, then click — takes [`O:here()`](#o-here) before the read and asks `O:stillHere` after it, before it acts: the runtime cannot check that for it.

**Where it must not wait**, the hooks run in a coroutine the runtime makes: one for a whole scan — every `when` of a Tab's search for the next control, every claimant of a shared hotkey, every test a menu tick asks — and each hook inline inside it; a hook asked on its own, such as the `when` of the control Return presses, has one to itself. That is one coroutine per scan or per lone hook. To the host it is a coroutine the module made: `host.ocr.recognize` without a callback does not wait there, but holds the event loop for the read and says so in the log ([Where it waits](ocr.md#where-it-waits)). A pixel, a snapshot, an image search and [`host.ocr.recognize`](ocr.md#host-ocr-recognize) with a callback answer at once and are fine. A `coroutine.yield` of the hook's own is ended with the error `a hook asked on every scan yielded; it must return at once`: a `when` asked on its own then hides its control, as a `when` that raises does; inside a scan the yield reaches past that guard, and the scan — and with it the key that started it — ends with that error. A menu test that yields has failed, as one that raises has: the tick goes on, the tests after it are asked on the next tick, and the test itself is asked in a coroutine of its own from then on, where it fails alone.

**The overlay's own origin is resolved before every such coroutine**, outside it, so a stored `identify` asked for it runs in the handler. One reached inside the coroutine all the same — another overlay's origin asked by a `when`, say, after a plug-in made its control anew — is not asked there: that evaluation finds no control (not counted towards the eight of [`attachEmbedded`](#o-attachembedded)), nothing of it is kept for the [epoch](timer.md#host-epoch), and the overlay looks again on its module's next turn, once per epoch, through a recheck of its own. Logged once per control: `attachEmbedded: [<class>] id=<id>, identify is not asked inside a scan of the overlay's hooks — '<overlay>' looks again on its next turn`.

**The first evaluation of a binding** runs on the module's next turn, about 15 ms after it was made ([`host.timer.after(0)`](timer.md#host-timer-after)), not inside the call: a binding is made at a module's top level, where nothing can wait. An overlay whose window or plug-in is in front at load comes up a tick later.

**Coming to the front** resolves the origin once more, guarded: a `control` function that raises there is logged, `[overlay] <label>: its origin raised as it came to the front: <message>`, and the overlay still comes up whole — its keys taken, its scope its window — as on a window whose handle it does not know.

**Before a press**, only a stepper's value is read, because its announcement waits for that value to change. No other control's `text` is asked before its `onActivate` runs; a module that needs what its `text` works out asks for it in `onActivate` itself.

```luau
-- Asked on every scan, in the runtime's coroutine: one pixel, at once.
ov:addCustomButton({
  label = "Close library browser",
  when = function(o)
    local c = o:origin().client
    local p = host.screen.pixel(c.x + 640, c.y + 12)
    return p ~= nil and p.r > 200
  end,
  onActivate = function(o)
    local x, y = o:toScreen(640, 12)
    if x then host.input.click(x, y) end
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

Creates a new overlay object. `label: string?` (defaults to `"Overlay"`).

Returns an `Overlay` (metatable-backed table) with fields `label`, `controls = {}`, `focus = 0`, `active = false`, `hoverToRead = false`, `contexts = {}`, `activeCtx = false`, and internal registration/key/trigger state.

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

## O\:addStaticText(label) {#o-addstatictext}

Appends a static text control: Tab-reachable and read aloud on focus, but with no activation (Enter does nothing). `label: string`.

Returns the control table `{ kind = "static", label = label }`.

`labelOrOpts` may also be a **table** — `{ label, text, when }` — and that form is the one worth knowing. `text(overlay) -> string?` is appended to the label each time the control is announced, which turns a caption into a **read-out**: a value the user wants to check is then read by arriving at it, with nothing pressed and nothing changed. The alternative would be a button, and a button that only reports is a promise of an action it does not perform. `text` runs in the handler of the key or event that announces the control, and may read there ([Where a hook runs](#where-a-hook-runs)).

```luau
-- A caption...
ov:addStaticText("Cinematic Studio Strings")

-- ...and a read-out, re-read on every announcement.
ov:addStaticText({
  label = "Battery",
  text = function() return valueRightOf("Battery Level") or "not shown" end,
})
```

## O\:addHotspotButton(opts) {#o-addhotspotbutton}

Appends a button that, when activated, clicks a fixed origin-relative point. `opts: { label: string, at: {number, number} | ((overlay) -> {number, number}?), points: {{number, number}}?, settle: number?, text: ((overlay) -> string?)?, ocrLabel: {number, number, number, number} | ((overlay) -> {…}?)?, hotkey: string?, hotkeyKeepsFocus: boolean?, rawOrigin: boolean?, fromRight: boolean?, button: ("left" | "right" | "middle")?, opensMenu: boolean?, menuItem: {number, number} | ((overlay, menu, factor) -> {number, number} | false | nil)?, retries: number?, when: ((overlay) -> boolean)? }` — `at` is `{x, y}` relative to the origin; `hotkey` is an optional global activation hotkey spec (e.g. `"Alt+P"`); `opensMenu` says the click puts a menu on screen (see [`O.menuTests`](#o-menutests)).

Returns `{ kind = "hotspot", label, ocrLabel, text, at, points, settle, hotkey, hotkeyKeepsFocus, opensMenu, rawOrigin, fromRight, button, menuItem, retries, when }`. On activate it clicks `(origin.x + at[1], origin.y + at[2])` — through the frame and the scale, when the overlay has them — logs `<overlay>: '<label>' activated, clicking (x,y)`, and speaks `"label, activated"`.

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

A `when` predicate must return exactly `true`; anything else counts as hidden. A hidden control is not Tab-reachable, its hotkey does nothing, and it does not claim a key combination that a visible sibling wants. A `when` runs in the runtime's own coroutine, once per scan, and must answer at once: no read that waits ([Where a hook runs](#where-a-hook-runs)).

A control's `hotkey` also **moves the overlay's focus** to that control, silently, before activating it — ReaHotkey's `TriggerHotkey` does the same. So after Ctrl+L for "Load instrument" and a dialog closed again, the overlay comes back on "Load instrument", and Tab carries on from there. The control then holds the keys a focused control holds (Space and Return for a button). `hotkeyKeepsFocus = true`, accepted by every constructor that takes a `hotkey`, opts out: Melodyne's "Menu bar" uses it, because focus on a button would take Space, which is Melodyne's play/stop. This changed on 2026-09-19 and affects every overlay with hotkeys on Windows as well (Kontakt, Komplete Kontrol, Soundiron, u-he).

Space or Return on a focused control that has **hidden itself** since focus reached it acts on the visible control that shares its hotkey — Kontakt's "Switch to classic view" and "Switch to play view" share Alt+V and swap places on every press. With no such control, the overlay says "… is not available now" instead of doing nothing without a word.

Two controls **share** a hotkey when their specs name the same key on this platform, however they are written: the runtime compares them through [`host.keys.normalize`](keys.md#host-keys-normalize), so `"Cmd+S"` beside an inherited `"Ctrl+S"` is one claim on every platform (both are the Ctrl role, which is Command on a Mac). The key is spoken on focus in the platform's words through [`host.keys.describe`](keys.md#host-keys-describe) — `"Alt+V"` is "Option+V" on a Mac, and a tab's `hotkeyLabel` is said the same way when it is a key spec. A hotkey the host would refuse — one that is not a key spec, one the system keeps for itself, a modifier tap, or a key the platform has no code for (the reasons [`host.keys.check`](keys.md#host-keys-check) marks "raises") — is reported when the overlay binds, naming the control, left out of the `[keys] … holds:` line with its reason instead of being registered, and not spoken when the control or tab is focused.

## Bindings — O.window / O.embedded / \:with / O.hosts / O\:bind {#bindings}

**Signatures:**
`O.window(matcher, opts?) -> Binding` · `O.embedded(spec, opts?) -> Binding` · `Binding:with(over) -> Binding` · `O.hosts(...) -> {matcher}` · `O:bind(binding, opts?) -> Overlay`

A **binding** is an inert value saying *where* an overlay lives: which window or embedded control, on which `slot`, at which `specificity`, with `pollMatch` / `menus`. Declare it once and reuse it, instead of writing a factory function that rebuilds an attach spec per attachment — the reason such factories appear is that a caller mutates the table and the next caller needs a clean copy. `O.window` takes what [`attach`](#o-attach) takes, and `O.embedded` what [`attachEmbedded`](#o-attachembedded) takes — so an embedded binding with no control of its own for a platform gets the DAW's plug-in panel there, as `attachEmbedded` describes, and every overlay bound to one `O.embedded` value shares the control it resolves, once per epoch, and on a panel its identity verdict.

`:with(over)` returns a **copy** with `over` merged into the target (`control`, `identify`, `title`, …); `over.opts` merges into the options. `O.hosts(...)` concatenates host lists and skips `nil`, so an optional host (a plugin that may not be installed) can be listed inline: a table with an array part is a list of entries, any other non-empty table is one entry — an entry may carry nothing but a platform block, as Logic's does in [daw-hosts](../daw-hosts.md) — and an empty table adds nothing. `O:bind(binding, opts)` attaches, with `opts` overriding the binding's own options.

```luau
local HOSTED = O.embedded({
    hosts = O.hosts(daw.all, kk and kk.standaloneWindow),
    control = "Qt%d+.-QWindowIcon",
    identify = identifyHost,
    cacheIdentity = false,
}, { slot = SLOT, menus = { O.menuTests.nativePopup, O.menuTests.accessibility } })

base:bind(HOSTED, { specificity = O.layer.base })
-- `macos = false`: the dialog is Kontakt's own, never a plug-in panel, so on a Mac — where a
-- plain pattern that finds nothing would take the DAW's panel — the binding does not apply.
dialog:bind(HOSTED:with({ control = { windows = "^NIChildWindow%x+$", macos = false },
                          identify = present, opts = { menus = false } }),
            { specificity = O.layer.dialog })
```

One overlay object takes **one** binding: an object is pinned to a single slot and its match poll belongs to that join. Two places to live means two overlay objects sharing a build function.

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

## O.memoByEpoch(fn) {#o-memobyepoch}

**Signature:** `O.memoByEpoch(fn: (...) -> value) -> (...) -> value`

Memoizes `fn` for the current [epoch](timer.md#host-epoch) only: until the epoch turns over the first call asks `fn` and every later call gets that answer back, whatever arguments it is given; the first call in a new epoch asks again. The epoch turns over on an OS event, a `host.timer.after` coming due, a read's or search's answer, `host.window.focus` and the module's own input — **not** on a [`host.timer.every`](timer.md#host-timer-every) tick, so a poll on `every`, the overlay's menu tick among them, gets the same answer on tick after tick until one of those happens. For a question whose answer holds only until the world moves — a reading of the screen, a walk of the accessibility tree — asked by several controls in one announcement, or by a [scale](#o-scale) factor at every coordinate one calibration shot places.

The same rule as [`O.memoByOrigin`](#o-memobyorigin): a **non-nil** answer is kept; `nil` means "could not tell yet" and is asked again at the next call; `false` is kept. Only the current epoch's answer is held — one slot, not a table — so nothing accumulates, and a stale answer cannot be read after the epoch has turned over. The arguments are not part of the key: a function asked about two different things in one epoch is two memos. `fn` is not guarded: what it raises comes out of the memo's call, and nothing is kept. A hit costs one call of [`host.epoch`](timer.md#host-epoch) and a comparison.

```luau
-- ON:EAR's accessibility tree, read once per epoch however many read-outs ask about it.
local tree = O.memoByEpoch(function() return host.element.rawDump(ov:hwnd()) end)

-- A zoom read off the plug-in's zoom field, for the overlay's scale: once per epoch, not once per
-- coordinate. nil (nothing read) is asked again, and places nothing meanwhile.
local zoomOf = O.memoByEpoch(function(origin) return readZoom(origin) end)
```

## O\:origin() / O\:hwnd() {#o-origin}

**Signature:** `O:origin() -> Control | Window | nil` · `O:hwnd() -> number | nil`

The active context's coordinate window — the plugin control when embedded, the window when standalone — and its handle. `nil` while the overlay is not active. This is the module's handle on the thing it overlays; use it instead of reaching into `activeCtx`.

```luau
-- `:hwnd()` -- the window handle, for asking the accessibility layer about it:
onActivate = function(o)
  local p = host.element.locate(o:hwnd(), "", host.element.type.Edit)
  if p then host.input.click(p.x, p.y) end
end,

-- `:origin()` -- the context itself, whose `client` rect every authored coordinate is
-- measured against. ON:EAR works a point in its design back out of where a word was read:
local function designY(overlay, screenY)
  local o = overlay:origin()
  if not (o and o.client and o.client.h > 0) then return nil end  -- not active: no answer
  return (screenY - o.client.y) / (o.client.h / DESIGN_H)
end
```

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

### Windows

An embedded plug-in's control is borderless, so its `client` and `bounds` are the same size and either answers alike. A standalone window's are not: a Kontakt 8 window is 1026 wide around 1010 of plug-in, and the content is the 1010.

### macOS

A DAW's plug-in panel ([`attachEmbedded`](#o-attachembedded-macos)) has `client` equal to `bounds`, so the two agree there. A standalone window's `client` is derived from its own geometry rather than read ([window table shapes](window.md#table-shapes), macOS): a titled window's content below its title bar, and the frame itself for a borderless one — its content size is then the frame's.

## O\:frame(fn) {#o-frame}

**Signature:** `O:frame(fn: (origin) -> (number, number)) -> Overlay`

Shifts the overlay's whole coordinate frame: `fn` returns `dx, dy`, resolved per active control — a host version's content shift, or a nested plugin's inner origin. Controls marked `rawOrigin` opt out, and so does a landmark-anchored overlay while its landmark is on screen ([`O:landmark`](#o-gate)): it places from the landmark's corner. Called before the overlay is bound; afterwards it raises. A second call replaces the first.

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

## O\:scale(fn, opts?) {#o-scale}

**Signature:** `O:scale(fn: (origin) -> number?, opts: { about: {number, number}? }?) -> Overlay`

Scales every authored coordinate of the overlay by the factor `fn` answers, for a plug-in that zooms its whole interface. VPS Avenger is the case it was built for: it zooms from 50 to 200 %, and by the avenger_control project's account a point measured at 50 % from its corner lands at *zoom / 50* times that distance. Every coordinate goes through one formula,

```text
screen = origin + frame + k × (authored − about)
```

rounded once, to the nearest whole unit — a pixel on Windows, a point on macOS — with a half rounded up: `origin` is the client-area top-left as above, `frame` what [`O:frame`](#o-frame) answers (unscaled), `k` what `fn(origin)` answers now, and `about` the authored point that stays where the frame puts it — `{0, 0}` unless `opts.about` says otherwise. `about` is for a plug-in that scales around something other than its corner: ARC ON:EAR scales by its window's height about its design's centre line, which is `about = {960, 0}` with a frame of half the window's width. Two cases do not fit the formula as written: `fromRight` is `origin + frame's dx + content width − k × at[1]` across (a distance from the right edge, which `about` has nothing to say about), with `y` as above; and a landmark-anchored overlay ([`O:landmark`](#o-gate) with `anchor = true`), while its landmark is on screen, is `landmark's corner + k × (authored − about)`, with no frame.

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

### Windows

A coordinate is a physical pixel ([Coordinates](index.md#coordinates)), so a plug-in drawn at 150 % display scaling is 1.5 times the size it has at 100 %, at the same zoom. The factor has to carry that: one learned from what is on screen — a width, a height, the distance between two things read by OCR — already does; one computed from the plug-in's zoom alone does not, and nothing in the host says which scaling a window is drawn at.

### macOS

A coordinate is a point, and a Retina display draws two pixels per point, so the factor is the same on a Retina display as on one without: a zoom read off the plug-in, divided by the zoom the coordinates were measured at, is the whole factor there.

## O\:toScreen(x, y, opts?) / O\:toScreenRect(r, opts?) {#o-toscreen}

**Signature:** `O:toScreen(x: number, y: number, opts: { rawOrigin: boolean? }?) -> (number, number) | (nil, string)` · `O:toScreenRect(r: {number, number, number, number}, opts: { rawOrigin: boolean?, whole: boolean? }?) -> {number, number, number, number} | (nil, string)`

Where an authored point, or an authored rectangle `{x1, y1, x2, y2}`, lands on screen now: what an `at` or a `region` of the overlay would click or read, through the same origin, frame and [scale](#o-scale), for a module's own code — a custom button's `onActivate`, a stepper's `onStep`, a `text` function that reads by OCR. `opts.rawOrigin = true` places it as a `rawOrigin` control would: in the origin's own pixels.

Returns `nil` and a reason when the overlay is not active (no origin), and when its scale has no factor now; a caller tests the first value. It raises what the overlay's [frame](#o-frame) function raises: the factor is asked guarded, the frame is not. In an overlay without a scale it is the plain sum, `origin + frame + authored`, unrounded — whole numbers in, whole numbers out. It clicks and reads nothing itself, and costs one origin resolution (memoised per [epoch](timer.md#host-epoch) for an embedded overlay) plus the frame's and the factor's functions.

`opts.whole = true` makes the rectangle one that [`host.ocr.recognize`](ocr.md#host-ocr-recognize) and [`host.screen.snapshotAsync`](screen.md#host-screen-snapshotasync) take as it is: whole numbers, not empty. Each corner is cut toward zero — `134.5` becomes `134`, `-965.5` becomes `-965` — which is the pixels the overlay's own read of a `region` covers; and when nothing is left once cut — corners turned around, or less than one across or down — the answer is `nil, "empty on screen"`. An overlay with a fractional frame places fractions otherwise, and `recognize` refuses them at the call. In a scaled overlay the corners are whole numbers already, and `whole` changes nothing but the empty case. Any value but `true` leaves the corners as placed. `whole` came with version 0.2 of the runtime: a module that uses it depends on `"com.platform.overlay >= 0.2"`.

```luau
-- A stepper that turns a knob by dragging it a few authored units, at whatever zoom it is drawn.
onStep = function(dir, o)
  local x1, y1 = o:toScreen(262, 140)
  local x2 = o:toScreen(262 + 6 * dir, 140)
  if x1 and x2 then host.input.drag(x1, y1, x2, y1) end
end,

-- A read-out that reads its own region: the handler that announces it waits for the reading.
text = function(o)
  local r = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
  return r and host.ocr.recognize(r).text or nil
end,

-- The same region read with a callback: the press returns at once, and the value is said later.
onActivate = function(o)
  local r = o:toScreenRect({ 34, 7, 214, 22 }, { whole = true })
  if not r then return end   -- no origin, no factor, or nothing of it on screen
  host.ocr.recognize(r, function(reading)
    if reading.status == "text" then host.speech.output(reading.text, { interrupt = true }) end
  end)
end,
```

### Windows

The answer is in physical pixels, as every coordinate is.

### macOS

The answer is in points, as every coordinate is; a factor that is not a whole number can put a point between two pixels of a Retina display, and it is rounded to the nearest whole point like any other.

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

Appends a button that runs a Luau callback on activation. `opts: { label: string, onActivate: (overlay) -> (), text: ((overlay) -> string?)?, typeLabel: string?, editable: boolean?, opensMenu: boolean?, hotkey: string?, when: ((overlay) -> boolean)? }` — `onActivate` receives the overlay itself (so a header control can reach the active context).

`onActivate` is called **guarded**. A control whose action throws says so rather than going silent: the host used to catch the error at the key dispatch and write a line, while the person who pressed it heard nothing at all — which is indistinguishable from a control that quietly did its job. It runs in the handler of the key or hotkey, and may read there; one that reads and then acts checks that the overlay is still where it was before it acts ([Where a hook runs](#where-a-hook-runs)).

`text` is announced between the label and the type word, and is how a control that ACTS can also say what it would act on. It is not read before `onActivate` runs: only a stepper's value is.

`typeLabel` overrides the word for what this control *is*. The kind a control is built from and the thing it stands for are not always the same: a custom button that puts the caret into a text field is an edit box to everyone using it, and announcing "button" describes the implementation to somebody with no interest in it — and says the wrong thing about what typing will do next.

`editable` says that activating this control leaves the caret in a text field, so **Space belongs to the application** from then on. Return still activates, because it is the only way back into the field after tabbing away. This is the same rule an `addOCREdit` gets by virtue of its kind; the flag is for controls that reach their field some other way (a coordinate, an accessibility element) and would otherwise swallow the space in "White 80s".

Returns `{ kind = "custom", label, onActivate, text, typeLabel, editable, opensMenu, hotkey, when }`.

```luau
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

## O\:addStepper(opts) {#o-addstepper}

Appends a **value changed with Left and Right**, where the module knows how to change it. `opts: { label: string, text: (overlay) -> string?, onStep: (dir: number, overlay) -> false?, onActivate: ((overlay) -> ())?, settle: number?, typeLabel: string?, hotkey: string?, hotkeyKeepsFocus: boolean?, when: ((overlay) -> boolean)? }`.

Announced as a slider, because that is what it is to the person using it; how it is driven underneath is the module's problem rather than theirs.

Use it where `addSlider` cannot serve. That one finds its thumb by matching an image, which needs a template captured for one plug-in at one size — no use for a rotary drawn as an arc, a bar with two handles, or anything in a window the user can resize. What such a control does have is a value printed beside it and something that moves it, and that is all a stepper is.

- `onStep(dir, overlay)` — `dir` is `-1` for Left and `+1` for Right. Called guarded, in the handler of the arrow key, where it may read ([Where a hook runs](#where-a-hook-runs)); one that raises says `"<label> could not be moved"`. It returns nothing, or `false` for a press that moved nothing and has nothing of its own to say — one that came while an earlier step is still being carried out, whose own announcement is on its way: nothing is watched or announced for it.
- `onActivate` — optional, and what a **press** means. Without it, Space and Return are still captured on a stepper (it is not an inert control) and then do nothing at all, which is a promise without an action. ON:EAR's two use it for "double-click to put this back to its default", which is one keystroke instead of twenty.
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

## O\:afterIdle(key, ms, fn) {#o-afteridle}

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

## O.doubleClick(x, y) {#o-doubleclick}

Two clicks at the same point, far enough apart in time to **be** a double-click. A module-level function rather than a method — it touches no overlay state.

Worth having in one place because the same fact is needed from both sides. A plug-in that resets a control to its default on a double-click is offering a genuinely useful gesture — one keystroke instead of twenty steps — and reaching it means sending two clicks close enough together. That same fact is a hazard everywhere else, which [`O:afterIdle`](#o-afteridle) exists to avoid.

The gap is ninety milliseconds, and the number that matters is that it is **not zero**. Two clicks sent back to back go out in the same instant, and an application deciding whether it has seen a double-click is looking at the interval between two presses — an interval of zero reads as easily as one press with a stutter as it does as two. Ninety is plainly two, and comfortably inside any double-click time a system uses.

```luau
-- ON:EAR's Tone and Width both reset to their default on a double-click, which is one
-- keystroke against twenty steps. Aimed at where the handle IS, not at the middle of the
-- track: the two handles stop about ten per cent apart, so the middle is on the far side
-- of the fold and belongs to the other one.
onActivate = function()
  local x, y = screenPoint(widthHandleX(widthNow()), 869)
  if x then O.doubleClick(x, y) end
end,
```

## O\:watch(spec) {#o-watch}

Waits for something to **change**, rather than for a length of time. `spec: { read: (overlay) -> any, was: any?, done: ((now: any, was: any) -> boolean)?, every: number?, within: number?, onDone: ((now, was) -> ())?, onGiveUp: ((now, was) -> ())? }`.

Almost every `host.timer.after` in a module is a guess about how long an application takes, and a guess is wrong in both directions. Too short and the value read back is the one from *before* the action — a real value announced with confidence for the wrong moment, which is how ON:EAR's speaker grid came to name the previously loaded speaker each time a tile was pressed. Too long and everything feels slow.

- `read` — what to look at. **`nil` means "cannot read right now", which is not the same as "unchanged" and never counts as done.** Treating those as the same thing is the specific bug this replaces.
- `was` — the reading from before the action. Omit it and `read` is called once to get it, which is only correct if nothing has happened yet.
- `done(now, was)` — defaults to "it changed".
- `every` — how often to look, default 100 ms. Each look costs whatever `read` costs, and a screen pixel is a compositor frame, so this is not free.
- `within` — give up after this long and call `onGiveUp` with the last reading. Not an error: a radio button that was already chosen is *meant* not to change.

Bound to the overlay. It stops the moment the overlay is no longer active or the window it was watching is no longer in front — a callback arriving after the world has moved is the mistake that pressing the key again cannot undo.

```luau
self:watch({
  read = function() return valueBelow("Width") end,
  was = before,
  within = 600,
  onDone = say,
  onGiveUp = say,   -- it did not move, and that is worth saying too
})
```

## O\:here() / O\:stillHere(mark, opts?) {#o-here}

**Signature:** `O:here() -> Mark` · `O:stillHere(mark: Mark, opts: { focus: boolean?, keys: boolean?, said: boolean?, menus: boolean?, place: boolean? }?) -> true | (false, string)`

Whether the overlay is still where it was, for an answer that comes later and acts on what it learned — a [`host.ocr.recognize`](ocr.md#host-ocr-recognize) callback, a timer. `here` notes where the overlay is now, as a mark; `stillHere` says whether that still holds: `true`, or `false` and why. They are the checks the runtime makes itself before it says a value read off the screen and before an OCR button clicks ([`O:addOCRButton`](#o-addocrbutton)), with the reasons in the same words as its `[read]` lines.

A mark notes the stay in front (every time the overlay comes to the front is a new one), the window — its origin's handle, [`O:hwnd()`](#o-origin) — the focus and the control on it, and how many keys of the overlay's own, announcements and menu openings it has seen. It is opaque: its fields are the runtime's. A mark taken while the overlay is not active never holds, except with `place = false`. `stillHere` asks, in this order, and answers the first that no longer holds:

| What | Asked | Why, when it no longer holds |
|---|---|---|
| The overlay is active | always, unless `place = false` | `the overlay is no longer active` |
| The same stay: it has not left the front since, not even to come back to the same window | always, unless `place = false` | `the overlay left the front and came back since` |
| The same window: the same handle | always, unless `place = false` | `the overlay is on another window now` |
| The same focus, and the same control on it | `focus = true` | `the focus has moved on` |
| No key of the overlay's own since | `keys = true` | `a later key reached the overlay` |
| No announcement of the overlay's since | `said = true` | `a later announcement was made` |
| No menu open over the overlay now ([`O:menuOpen()`](#o-menuopen)) | `menus = true` | `a menu is open over the overlay` |
| No menu opened over it since, not even one closed again | `menus = true` | `a menu opened over the overlay since` |

- **A key of the overlay's own** is one that reached it — Tab, Shift+Tab, Return, Space, an arrow it holds, the tab keys, a control's or a tab's hotkey — whatever it did, nothing included. A key that went to the plug-in is not one, nor is a calibration key.
- **An announcement** is the overlay saying a control as a whole — the focus arriving on it, the overlay's arrival, a control said again once a press settled (a toggle, a stepper, an OCR control's activation) — whether or not it was said in the end. What the overlay says about an action — `"<label>, activated"`, a tab switched with Left or Right — is not one.
- **A menu** is one the overlay's [menu tests](#o-menutests) see; without menu tests none ever opens.
- **`place = false`** leaves the first three out, for an action that stays right while another window of the same application is in front and the overlay may not be active (see macOS below). The code then checks its target itself. The other options are asked as given.

No timer and no [epoch](timer.md#host-epoch) decides anything here: each count changes only when its event happens. What a mark does not see: typing that went to the plug-in, the plug-in redrawing, and a window moved or resized, which is the same window. Coordinates worked out before the wait are worked out again after it ([`O:toScreen`](#o-toscreen)).

`stillHere` raises, naming the caller's line, when `mark` is not one from `O:here()`, when it is a mark of another overlay, when `opts` is not a table, and for an option it does not have — a misspelled option would otherwise check nothing at all. It never raises otherwise. Both run where they are called, on the main thread: `here` costs a small table and one origin resolution (memoised per epoch for an embedded overlay), `stillHere` a few comparisons and, unless `place = false`, one more origin resolution. Neither touches the screen.

They came with version 0.2 of the runtime: a module that calls them says so, `dependencies = ["com.platform.overlay >= 0.2"]`, and an older runtime then fails its load instead of its first call.

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
      local still, why = o:stillHere(mark, { keys = true })
      if not still then return host.log.info("Bank: not clicking, " .. why) end
      if not samePlace(o:toScreenRect(BANK, { whole = true }), r) then return end -- it moved
      if reading.status ~= "text" then return end
      local w = reading.words[1]
      host.input.click(w.x + 2, w.y + math.floor(w.h / 2))
    end)
  end,
})
```

### Windows

The window is the origin's window handle: the plug-in's own child window in a DAW, its window standalone.

### macOS

The window is the origin's `id`: on a DAW's plug-in panel, the plug-in window's. A plug-in's menu can be a window of its own that comes to the front, and the overlay may not be active while it is; an action that belongs to the press before it — a click into that menu — asks `{ keys = true, place = false }` and checks its target itself, for instance that the same application is in front ([`host.window.foreground()`](window.md#host-window-foreground)).

## O\:addOCRButton(opts) {#o-addocrbutton}

Appends a button whose label/value is read live by OCR over a region; activating re-reads it then clicks the region centre. `opts: { label: string, region: {number, number, number, number} | ((overlay) -> {number, number, number, number}?), text: ((overlay) -> string?)?, fallback: string?, hotkey: string?, hotkeyKeepsFocus: boolean?, readOnly: boolean?, opensMenu: boolean?, when: ((overlay) -> boolean)? }` — `region` is `{x1, y1, x2, y2}` origin-relative; `readOnly` re-reads on activation and never clicks; `opensMenu` says the click puts a menu on screen (see [`O.menuTests`](#o-menutests)).

Returns `{ kind = "ocr", label, region, hotkey, hotkeyKeepsFocus, readOnly, opensMenu, when, text, fallback }`. It is announced as `"<label>[, <text>], button[, <key>], <value>"` — the key its `hotkey`, said in the platform's words when focus arrives on the control (not when it is activated), and the value what the region reads: its text; `fallback` when nothing is drawn there or the recogniser read nothing, and `"no text"` when there is no `fallback`; `"cannot be read now"` when the read failed — and a `readOnly` one without the word "button": a control that cannot be pressed does not describe itself as one.

**Read off the event loop.** The region is read with [`host.ocr.recognize`](ocr.md#host-ocr-recognize) and a callback at every announcement — arriving on the control, and activating it — in the user's language, and the announcement is said when the answer comes: one sentence, as late as the recognition makes it, while the keys, the timers and the speech of the whole application go on. A value read as several lines is said as one, the lines joined with a space.

**Activating** asks for the read, and clicks the region centre once the answer has come and the sentence has been said — the order the two had while the read held the event loop. So the value said is the one from before the click, however long the picture takes; the sentence does not cut off what the click opens (a screen reader's announcement of a dialog or a field); and an `opensMenu` press counts at its click (see [`O.menuTests`](#o-menutests)), so the menu it opens comes after its value. The click is made only while the overlay is still where the key was pressed — active, on the same window, not having left the front since, the control still in its place — and no other key of the overlay's own has arrived since: a user who pressed on has moved on, and a click landing after that, a menu opening over the control they went to, is the mistake pressing again cannot undo. A click not made writes `[read] '<label>': not clicking — <why>`. An OCR edit field (`addOCREdit`, Komplete Kontrol's "Save as") puts the caret into its field on focus by a click made the same way: after its sentence, and only while the focus is still on it and no key of the overlay's own has come since (`[read] '<label>': the focus moved on before the answer; not clicking into the field`).

The read is made in the VM of the module that owns the overlay, so it is that module's and counts toward its 16 reads waiting or running at once. Its `key` is one per overlay — `"com.platform.overlay focus 1"`, `"… 2"` and so on, one per overlay in the order they first read — so a later announcement's read replaces one that has not started recognising yet; a module does not use keys that begin `"com.platform.overlay "` for its own reads.

The sentence is said only while it is still about what is in front of the user: the answer is not superseded (`"stale"`, `newer`); the overlay is still active, on the same window, and has not left the front since; the focus is where it was; no key of the overlay's own has arrived since and no other announcement of the overlay has begun; and no menu counts as open over it ([`O:menuOpen()`](#o-menuopen)), nor has one opened since the read was asked — a menu the plug-in put up itself, after a choice in which the value read is the one from before it. Otherwise nothing is said for it. No timer decides any of this: an answer is spoken when it comes, or dropped. A key that goes to the plug-in itself never reaches the overlay and does not drop an answer. A read refused at the call is said at once, with the static label and `"cannot be read now"` as the value. A region with nothing of it on screen — corners turned around — is not read and is said as an empty read is. The same rule keeps a late announcement from taking the place of a key's: a toggle's announcement once its state has settled after a press ([`O:addHotspotToggle`](#o-addhotspottoggle), [`O:addGraphicalToggle`](#o-addgraphicaltoggle)) and the overlay's announcement on arrival are not made when a key of the overlay's own came after the press or the arrival, which has said where the user is (`[watch] <overlay>: '<label>' not announced — a later key reached the overlay`, `[activate] '<overlay>': a key reached it before its arrival was announced; not announcing the arrival`).

**The log.** Each answer writes one line: `[read] '<label>' = "<text>" (region x1,y1 wxh, <ms> ms, <n> words[, <k> placed by estimate], <language>)` — the region as authored; `<ms>` from the announcement to the answer; `<n>` the words the recogniser placed, and `<k> placed by estimate` the words whose place the host estimated, which are the second recogniser's answer — on Windows, and on a Mac where the download carries it — the way a lone digit is read; and the language the read was made in, left out for a reading never recognised — then `: nothing drawn there`, `: the recogniser read nothing`, `: the read failed (<error>)` or `: not recognised` for a reading that is not text; then `; spoken`, or `; not spoken:` and why — `a later announcement's read replaced it before it was recognised`, `a later announcement asked for another read`, `the overlay is no longer active`, `the overlay left the front and came back since`, `the overlay is on another window now`, `the focus has moved on`, `a later key reached the overlay`, `a later announcement was made`, `a menu is open over the overlay` or `a menu opened over the overlay since`. A read refused at the call writes `[read] '<label>' could not be read: <error>`, and a region empty on screen `[read] '<label>' not read: its region is empty on screen (x1,y1 to x2,y2)`.

- `text(overlay)` is announced between the label and the type word: what a read-out's value *is*, where that changes (Melodyne's inspector box holds whichever parameter the tool owns).
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
-- overlay the region follows the zoom; `fallback` says an empty field in the plug-in's terms.
ov:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, readOnly = true,
  fallback = "no preset name" })

-- A region that depends on what the module knows: the value box of whichever slot is shown.
ov:addOCRButton({ label = "Slot value", readOnly = true,
  region = function(o)
    local slot = o.state.slot
    return slot and { 40 + 60 * slot, 300, 90 + 60 * slot, 316 } or nil   -- nil: "cannot be read now"
  end })
```

### Windows

The read is the one [`host.ocr.recognize`](ocr.md#windows) makes for any module: through the module's capture source, then `Windows.Media.Ocr` with the second recogniser beside it for a lone digit — the recognisers, the small-text treatment and the language (the first language in the user's Windows list that OCR has installed) that the read on the event loop used here before. One difference in the language: the user's list is read when the application starts, so a change to it in Windows Settings is used from the next start, where the read on the event loop asked Windows at every read. A read of one of sforzando's read-outs took 22–42 ms (measured on the event loop, a debug build on a six-core desktop), so the sentence comes about as soon after the key as it did — later by about one turn of the loop, which is a timer message of the lowest priority every 15–16 ms, and by any recognition of another read already running, which is not interrupted; an activation's click, after the sentence, by as much. A region's corners are cut toward zero, as the read on the event loop read them, so an overlay with a fractional [frame](#o-frame) reads the pixels it always did.

### macOS

The pipeline [`host.ocr.recognize`](ocr.md#macos) uses on a Mac: Vision's retry ladder, and the second recogniser beside it where the download carries it, with an Intel Mac's checked first pass. The language is the user's — the first of their preferred languages that Vision reads, then English — where the read on the event loop without `lang` handed Vision none, and so Vision's own default; for a user whose first language is not English, a value or label is now read with that language. How long the sentence takes is the recognition's, measured with the read on the event loop and Vision alone, before the second recogniser answered on a Mac: 25–56 ms for sforzando's read-outs on a Mac mini M1, 115–164 ms after more than 5 s without a read, and 143–786 ms on an Intel MacBook Air (2020) — see [`host.ocr.recognize`](ocr.md#macos). While it runs, the keyboard's event tap, the timers and every other key go on; on that Air each such read used to hold them up for as long.

## O\:addGraphicalToggle(opts) {#o-addgraphicaltoggle}

Appends a toggle whose on/off state is read by image-matching its region against an "on" and "off" template; activating clicks the region centre and re-reads the new state after ~150 ms, unless a key of the overlay's own came meanwhile. `opts: { label: string, region: {number, number, number, number}, onImage: string?, offImage: string?, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative; `onImage`/`offImage` are template image paths.

Returns `{ kind = "gtoggle", label, region, onImage, offImage, hotkey }`. Spoken state is `"on"` / `"off"` (omitted when neither template matches).

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

## O\:addHotspotToggle(opts) {#o-addhotspottoggle}

Appends a toggle whose on/off state is read from a **single pixel** at its click point (ReaHotkey's `HotspotToggleButton`) — cheap (one screen touch), where a region scan would cost ~16 ms *per pixel*. The pixel is compared to an on/off reference colour and the **nearest** wins. Activating clicks the point (toggling it) and then [waits for the state to actually change](#o-watch) before announcing it, giving up after ~900 ms — and does not announce it when a key of the overlay's own came meanwhile (see [`O:addOCRButton`](#o-addocrbutton)). A control that redraws in thirty milliseconds is announced in thirty; one that takes half a second is announced correctly instead of early; one that does not change at all — the already-chosen member of a radio group — is announced at the deadline, which is the truth about it. `opts: { label: string, at: {number, number}, onColor: {number, number, number}, offColor: {number, number, number}, hotkey: string?, rawOrigin: boolean? }` — `at` is `{x, y}` origin-relative; `onColor`/`offColor` are `{r, g, b}`.

`at` may also be a **function** of the overlay, for a control whose position depends on what is on screen right now; returning `nil` yields no point and the click is skipped rather than guessed. `onColor`/`offColor` may each be a **list** of `{r, g, b}`, because one state can legitimately look several ways. A reading that resembles neither closely enough — measured against how far apart the references are — reports **no state at all** and logs why, rather than announcing whichever is nearer. Announcing the opposite of the truth is the worst thing this system can do to somebody who cannot check it against the screen; saying nothing is merely unhelpful.

`text` is honoured here as everywhere, and is how a toggle can report something the state alone does not say — ON:EAR's panel switches use it for "unavailable", because announcing "off" for a switch that will not respond is the same failure as announcing the wrong state.

Like `addHotspotButton`, the click is refused if another window is drawn over the point (see [`host.window.ownsPoint`](window#host-window-ownspoint)).

Returns `{ kind = "hotspottoggle", label, at, onColor, offColor, text, hotkey, rawOrigin }`. Spoken state is `"on"` / `"off"` (omitted when the pixel can't be read). Prefer this over `addGraphicalToggle` when the control has a distinct lit/unlit colour (an indicator LED, a lit ⏻ icon) — it needs no template images and is a fraction of the cost.

```luau
ov:addHotspotToggle({
  label = "Spot 1 Mic",
  at = { -138, 366 },
  onColor = { 180, 165, 230 }, -- lit (accent)
  offColor = { 78, 79, 85 },   -- unlit (dim grey)
})
```

## O\:focusNext() {#o-focusnext}

Moves focus to the next control (wrapping) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. No-op when there are no controls. Returns nothing. On a [pass-through](#o-addpassthrough) stop it steps through the plug-in's own controls instead, as Tab does there, and `focusPrev` back.

Tab is already bound to this by the runtime, so a module calls it only to give the ring a second, more natural key.

```luau
-- ON:EAR's five preset slots are a row, so Alt+Right should walk them too.
host.hotkey.register("Alt+Right", function()
  if ov.active then ov:focusNext() end   -- a global key: only steer an overlay that is up
end)
```

## O\:focusPrev() {#o-focusprev}

Moves focus to the previous control (wrapping) and speaks it. No-op when empty. Returns nothing. On a [pass-through](#o-addpassthrough) stop whose walk has gone into the plug-in it steps one element back, as Shift+Tab does there — and on the element the walk went in on, back out onto the stop.

```luau
-- The mirror of the above, and worth having for the wrap: with nothing focused yet this
-- lands on the LAST control, because the end of the ring is where you look when you suspect
-- you missed something.
host.hotkey.register("Alt+Left", function()
  if ov.active then ov:focusPrev() end
end)
```

## O\:addPassThrough(opts) {#o-addpassthrough}

**Signature:** `O:addPassThrough(opts: { label: string?, when: ((overlay) -> boolean)?, readUnnamed: boolean? }?) -> Control` · `O:enableUiaPassThrough(opts?) -> Overlay`

Appends a stop that hands Tab to the plug-in's own focusable elements, for a plug-in window that does not move focus on Tab by itself. In Kontakt standalone it is the only way into Kontakt's native controls, and Kontakt's standalone overlay ends with it, as "Kontakt controls". Like every control it is added before the overlay binds; afterwards it raises.

- `label` — what the stop is called; `"Plugin controls"` when left out.
- `when` — as for every control: the stop is in the ring only while it returns exactly `true`.
- `readUnnamed` — read a stop that has no name off the screen, below. On unless it is `false`; `nil` and anything else leave it on.

Returns the control table, `{ kind = "passthrough", label, when, readUnnamed }`. `O:enableUiaPassThrough(opts)` is the old spelling: it adds the same control with the same `opts`, and returns the overlay.

**Tab and Shift+Tab.** Arriving on the stop says `"<label>, Tab to step through them"`. Tab from there moves the plug-in's own keyboard focus one element on with [`host.element.focusStep`](./element.md#host-element-focusstep), and the screen reader announces the element from the focus event; each further Tab moves on one element, and Shift+Tab one back. Once every element has been visited, Tab goes on to the overlay's next control without touching the plug-in's focus; Shift+Tab on the element Tab went in on comes back out onto the stop; Shift+Tab on the stop itself goes back through the overlay's controls, never into the plug-in. Arriving on the stop again starts the walk afresh. How a walk is counted — in steps, against a ring that may change size as it is walked — is under [`host.element.focusStep`](./element.md#host-element-focusstep). While the stop has the focus, Space and Return are not the overlay's: they reach the plug-in's element, which is where a check box is ticked or a list opened.

**What is spoken.** A stop with a name is the screen reader's: the overlay says nothing about it. A stop whose name has no visible character — empty, or made only of white space, control characters and characters that show nothing, such as a no-break space, a zero-width space or a byte-order mark — is announced by the screen reader as its role and little more, so with `readUnnamed` the overlay reads what the plug-in drew there:

- The rectangle `focusStep` gives for the element (`bounds`), widened to whole numbers — the near edges down, the far edges up — and cut to the rectangle of the overlay's origin, the plug-in window (its `bounds`, see [`O:origin()`](#o-origin)): what lies beyond is another window's. Nothing is read when there is no rectangle, when it lies outside the window, or when what is left is under 2 across or down. It is not cut to a scroll view: an element scrolled out of its list's view is read where it lies (see [`host.element.focusStep`](./element.md#host-element-focusstep)).
- It is read with [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) and a callback, in the user's language. The picture is taken at the step, normally within one screen frame of it, so it shows what the plug-in has drawn by then. The read is made in the VM of the module that owns the overlay (the runtime is a `code_module`), so it is that module's: it counts toward the module's 16 reads waiting or running at once, and the module's next `host.input.*` or `host.window.focus` call waits up to 50 ms for its picture, as after any read. Its `key` is one per overlay, so a later step's read replaces one that has not started recognising yet; the keys are `"com.platform.overlay passthrough 1"`, `"… 2"` and so on, one per overlay in the order they first read, and a module does not use keys that begin `"com.platform.overlay "` for its own reads.
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

### Windows

`bounds` is the element's UI Automation rectangle, read once, right after the focus has landed: a scroll the plug-in animates after that is not in it (see [`host.element.focusStep`](./element.md#host-element-focusstep)). The capture is a fixed ~17 ms screen frame through the standard path, and the recognition 4–6 ms for a small read-out, tens to hundreds of milliseconds for a large region (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)).

With **Speak through the screen reader** ticked in the Application settings tab, as it is by default, and a screen reader running — NVDA, say — the line goes to the screen reader, without cutting off what it is saying: when its announcement of the element has begun, the line waits behind it. When the answer comes first, which a small element's read can, what becomes of the line once the screen reader handles the focus event is the screen reader's decision. With the box unticked, or no screen reader running, the line goes to the plain voice — a second voice beside the screen reader, which can talk over its announcement (see [`host.speech.output`](./speech.md#host-speech-output)).

### macOS

`bounds` is the element's Accessibility frame as the walk read it, before the focus moved. The recognition is Vision's, with its retry ladder abandoned after 250 ms (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)).

The line goes to the platform's own voice unless **Speak through VoiceOver** is ticked in the Application settings tab (see [`host.speech.output`](./speech.md#host-speech-output)): through the platform's own voice it is a second voice beside VoiceOver, and can overlap VoiceOver's announcement of the element rather than follow it; through VoiceOver, whether it waits behind that announcement is VoiceOver's decision.

## O\:activate(index) {#o-activate}

Activates the control at `index` (defaults to the focused control). `index: number?`. Behaviour by kind: `hotspot` clicks `at` and speaks `"label, activated"` — and, with `menuItem`, chooses that item once a menu test sees the menu (see [`O:chooseMenuItem`](#o-choosemenuitem)); `custom` calls `onActivate(self)`; `ocr` reads the region again, off the event loop, says it, and then clicks the region centre, unless the overlay or the user has moved on by then (see [`O:addOCRButton`](#o-addocrbutton)); `gtoggle` clicks the region centre and re-reads state after ~150 ms. A control that cannot be placed now (a [scale](#o-scale) with no factor, a region function with no answer) clicks nothing and says so: `"<label> is not available now"`, or for an OCR control the `"cannot be read now"` of its announcement. No-op for `static` or a missing control. Returns nothing.

```luau
-- Space and Return are bound to this by the runtime, so a module needs it only to press a
-- control from somewhere else. Melodyne swallows the first key a freshly focused window
-- receives, which makes a "do that again" key worth having.
host.hotkey.register("Ctrl+Alt+Return", function()
  if ov.active then ov:activate() end   -- no index: whatever is focused
end)

-- With an index, counted in `ov.controls`:
-- ov:activate(1)
```

## O\:attach(matcher, opts) {#o-attach}

Binds the overlay as a **standalone** context: active while a window matching `matcher` is the foreground/active window — or while the overlay holds its place over its own menu, a window of the same application in front after one of its controls opened it, in which one of its tests sees a menu (see [`O.menuTests`](#o-menutests)) — with coordinates relative to that window's client area. `matcher` is a window matcher passed to `host.window.test`; `opts: { hoverToRead: boolean?, menus: {MenuTest}?, slot: string?, specificity: number?, pollMatch: number?, pollWhen: ((overlay) -> boolean)? }?` — `menus` is described under [`O.menuTests`](#o-menutests), and `slot`, `specificity`, `pollMatch` and `pollWhen` as for [`attachEmbedded`](#o-attachembedded). The overlay is first evaluated on the module's next turn after it is bound, not inside the call ([Where a hook runs](#where-a-hook-runs)).

While active, the overlay captures and suppresses the navigation keys, scoped to its own window (so `Alt+Tab` and menus pass through natively): `Tab` / `Shift+Tab` move between controls, `Return` and `Space` activate the focused control, and — when the overlay has a tab control — `Left`/`Right`, `Ctrl+Tab`/`Ctrl+Shift+Tab` and `Ctrl+<n>` drive it (the first two are Control+Tab on a Mac, see [below](#o-attach-macos)). `Space` is released while an editable field (an `ocredit` control) is focused, so a literal space can be typed into it. On activation the overlay starts at its first control (and any tab control at its first tab) when a **genuinely new** window opened, but resumes the last-focused control when the *same* still-open window merely regained the foreground (`Alt+Tab` out and back); the two are told apart by the window's identity (its HWND). `hoverToRead` (default `false`) moves the mouse onto an OCR control on focus (some UIs only reveal values on hover). Registers the foreground/focus trigger once. Returns nothing.

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

### Windows {#o-attach-windows}

The tab keys are the ones written: `Ctrl+Tab` and `Ctrl+Shift+Tab` cycle the tabs, and `Ctrl+1` to `Ctrl+9` go to tab 1 to 9.

### macOS {#o-attach-macos}

`Ctrl+1` to `Ctrl+9` are Command-1 to Command-9, because a spec's `Ctrl` is Command on a Mac ([the key spec](./keys.md#key-spec-string-format)). The two cycling keys are picked per platform instead: they are Control-Tab and Control-Shift-Tab (`Meta+Tab`, `Meta+Shift+Tab`), because Command-Tab is the application switcher, which the system keeps for itself.

## O\:attachEmbedded(spec, opts) {#o-attachembedded}

Binds the overlay as an **embedded** context: active while keyboard focus is inside a plug-in hosted in a DAW. Coordinates are relative to the plug-in's control — its own child window on Windows, the DAW's plug-in panel on a Mac (below) — so the same regions work standalone and embedded, and in every DAW.

`spec: { hosts: {Entry}?, host: Entry?, control: string | { windows: Control?, macos: Control?, linux: Control? } | ((activeWindow, hostPanel) -> control?)?, identify: ((control) -> boolean?)?, cacheIdentity: boolean?, present: ((control) -> boolean)? }` where `Control` is `string | ((activeWindow, hostPanel) -> control?) | false`.

`hosts` is the list of DAW entries the plug-in can be embedded in (falls back to `{ spec.host }`): window matchers carrying what each DAW knows about its plug-in windows. A plug-in module passes `daw.all` from `com.platform.daw-hosts` and names no DAW itself — see [daw-hosts](../daw-hosts.md) for the entry format and for adding a DAW. The first entry whose matcher takes the window in front is the one used; its `chrome` classes and its `pluginOrigin` apply.

`control` says where the plug-in is, per platform, resolved in three steps:

1. **The module's own.** A string is a Luau pattern matched against the class names of the window's child controls (`host.window.controls()`) and of the focus chain (`host.window.focusChain()`); the first that matches and passes `identify` is the control. A function is called as `(activeWindow, hostPanel)` and returns a control table or `nil` (see [macOS](#o-attachembedded-macos)). A string that is not in an OS-keyed table applies on every platform.
2. **The DAW's plug-in panel**, when the module gives nothing for this platform — no entry, or a pattern of which no control both matched and passed `identify` — **and** the window's entry has a `pluginOrigin` on this platform **and** the binding has an `identify`. The panel says where *a* plug-in is and nothing about *which*, so a binding without `identify` never takes it.
3. **Nothing**: the binding is inert on this platform. It says so once at bind time, as `[<label>] embedded binding is inactive on <os>: <why>` — the module declared `false` for the platform; or neither the module (no pattern for the platform) nor any of its hosts (no `pluginOrigin` there) gives a control; or only the panel is on offer and the binding has no `identify`. It registers no trigger and asks nothing afterwards. The line is decided over all of a binding's hosts at once, so a binding with `daw.all` — where some entries have an origin — never says it for an entry that has none; the runtime says that instead, once per entry and VM, the first time such an entry's window is in front: `[overlay] '<title>' is a plug-in window of <daw>, whose daw-hosts entry gives no pluginOrigin on macos — no plug-in panel there, …`.

`false` for a platform is the module's word that the binding does not apply there, not even through the panel: Kontakt's "Content Missing" dialog overlay is `{ windows = "^NIChildWindow%x+$", macos = false }`. A pattern that looks like a Win32 class — no `AX` and no `/` in it — is reported once per pattern on a Mac, saying what the binding does then: takes the panel, or stays inert because it has no `identify`, or because none of its hosts gives a `pluginOrigin` there; written as `{ windows = … }` it is not tried there at all.

`identify(control)` is an optional confirmation callback (UIA / OCR / image search) — needed because a class pattern often matches any plug-in (the `Plugin<pointer>` class that several vendors' plug-ins share) and a panel matches every one. `true` matches; any other value except `nil` — `false` included — does not, and is kept as the verdict. `nil` means "cannot tell yet" (UIA not ready the instant a plug-in appears) and is not kept: the control does not match now and `identify` is called again at the next recheck — every focus event, and every `pollMatch` tick — up to 8 times in a row; the eighth `nil` in a row is kept as "no", as `false` would be. Each call runs on the event loop and costs whatever `identify` does (a UIA search of an Electron window was measured at 0.65–0.88 s), so an `identify` that means "no" returns `false`, not nothing. A kept verdict's `identify` runs in the handler of the event that resolves the origin, where it may wait for a read — unless the arbiter's call is the first to resolve it, bringing an overlay on a `slot` to the front after another claimant left: that is a plain call, where it cannot; one reached inside a scan of the overlay's hooks is not asked there, and is asked on the module's next turn ([Where a hook runs](#where-a-hook-runs)). One that raises is no verdict: "cannot tell yet", asked again at the next recheck and not counted towards the 8, logged once per control as `attachEmbedded: [<class>] id=<id>, identify raised: <message> — taken as could not tell yet, and not counted — '<overlay>'`.

An `identify` that reads the screen **with a callback** — [`host.ocr.recognize`](ocr.md#host-ocr-recognize), as sforzando's does on a panel (below) — answers `nil` until its answer lands and then calls [`host.window.recheck`](window.md#host-window-recheck); `false` would be kept. Its answer has to land within those 8 evaluations: every recheck in between counts one, so under a `pollMatch` of 500 ms that is about four seconds of rechecks and nothing else happening, and a read that lands later finds "no" kept already — per control on Windows, for the rest of the stay on a Mac. A read that saw nothing at all (`"blank"`, `"none"`, `"failed"`) is no answer: leave the verdict `nil` so the next evaluation reads again, and keep `false` for text that is not yours. An async read inside a `control` **function** instead — Kontakt's — is not counted by the runtime at all: the function returns `nil` until its read lands, and the function bounds its own retries.

How long a verdict is kept is per platform — per control on Windows, per stay in the panel on a Mac — see the sections below. `cacheIdentity = false` keeps nothing: `identify` is called at every evaluation, for a verdict that depends on more than the control (Kontakt's: "is a Komplete Kontrol wrapped around it?"), so such an `identify` has to be cheap — and it must answer at once: it runs in the runtime's own coroutine, as a `when` does ([Where a hook runs](#where-a-hook-runs)), and so does the `identify` of a `control` function's control where the DAW gives no panel, which is asked at every evaluation too. One that raises is "no" for that evaluation, logged once per control: `… identify raised: <message> — taken as no this time — '<overlay>'`. `present(control)`, when given, is asked at every evaluation as well and never kept — for a condition that changes while the same control stays focused — in the runtime's own coroutine, so it must answer at once too. One that raises does not match, and is logged once per binding as `attachEmbedded: '<overlay>': its binding's present raised: <message> — taken as not matching`; the evaluation that asked it goes on to its end, so an overlay in front leaves it.

`opts: { hoverToRead?, menus?, slot: string?, specificity: number?, pollMatch: number?, pollWhen: ((overlay) -> boolean)? }` — `hoverToRead`, `menus` and the navigation / focus-reset behaviour are as in `attach`. With `slot` the overlay joins the host **arbiter** for that slot at `specificity` (a base and the overlays inheriting it pass the same slot; the most-specific *matching* one is active — see `host.arbiter`); `pollMatch` (ms) additionally re-checks the match on a recurring timer, for matches that change with no window event (a library landmark appearing inside an already-focused plugin). `pollWhen`, with `pollMatch`, is asked on each of its ticks, with the overlay, and the match is re-checked only when it answers `true`: for a match that can change without a window event only while the module expects it to — VPS Avenger's warning box, looked for only after a Save or Initialize of its own. Every other tick then costs one call of it rather than a context match (the focus chain is read, and on a Mac in Logic that is the slow accessibility). One that raises is logged once, `[poll] '<overlay>': pollWhen failed: …`, and the match is re-checked as if it were not there; anything but a function raises when the overlay is bound. The poll's interval and its one timer per interval are unchanged by it. Returns nothing. The overlay is evaluated on the module's next turn after it is bound, like any binding ([Where a hook runs](#where-a-hook-runs)) — and an overlay that is already bound (to its standalone window, say) is evaluated again after this call, once, so a plug-in already in front is recognised without waiting for a window or focus event, which on a Mac may never come.

The control is resolved once per [`host.epoch()`](timer.md#host-epoch) per `spec` table, shared by every overlay bound to that table, so the controls, the focus chain and `identify` are asked once per event however many overlays and coordinates want the answer; on a panel the verdict is kept per `spec` table as well, so an overlay that asks while another bound to the same table is outranked does not ask again.

As with `attach`, the overlay also stays active while it holds its place over its own menu — a window of the same application in front after one of its controls opened it, in which one of its tests sees a menu (see [`O.menuTests`](#o-menutests)). When that ends, the match above decides again, exactly as for any other window change; if the keyboard is still in the menu's own window and that window is not shown, the window of the press is brought back first, once.

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
  identify = function(ctrl)
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
        host.window.recheck()
      end)
    end
    return nil                                     -- not false: false is kept for the stay
  end,
}, { menus = { O.menuTests.nativePopup, O.menuTests.newWindow } })
```

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

**The gate is geometric.** The `chrome` classes are Win32 vocabulary and match nothing here, so the runtime asks where the focused element is. The focus chain has to **end in the window in front**: the window in front and the focused element are two separate questions to the system, and a chain that ends in another window — another application's, read in between — says nothing about this one. On such a chain, or an **empty** one (the backend saying it could not read where the keyboard is, as while the application is not answering), the keyboard is not inside, and the origin is not asked for either. Otherwise the window itself as the focused element — all a view that publishes nothing leaves on the chain — counts as inside, and so does an element whose centre lies inside the panel; an element whose centre lies outside the panel is the DAW's own chrome and the overlay stays out, and so is an element that has no rectangle (the host gives every element one; a control a module made may not). The accessibility focus on a menu item outside the panel takes the overlay out as any element outside it does.

**A verdict is kept for a stay.** One REAPER FX chain window shows whichever FX is selected in its list, in the same window — possibly at the same rectangle, since REAPER sizes the window to the plug-in — so a panel's verdict cannot be kept per window. Whether Logic's plug-in window can be switched to another insert from its header is not measured yet. The rule assumes that what changes the plug-in shown is the DAW's own chrome, and that the keyboard has to be there to use it. So a verdict is kept for a **stay**: the keyboard inside one window's panel, from the evaluation that first finds it there until anything else is observed — the keyboard on the DAW's chrome, another window in front, an empty chain or one that is another window's, or the window's title or client size changed. The next time the keyboard is inside, `identify` is asked again, once. It is observed at every evaluation, and by a watcher of the runtime's own on every focus and activation event in each VM that has a binding that can take a panel, so an overlay outranked on its slot — which skips its own rechecks — does not bring back a verdict from before the visit. Moving the window keeps the stay. The verdict is kept for the stay **and** the control's place in the window, so a control a function moves within one stay is asked about again. The eighth `nil` in a row is kept as "no" for the rest of the stay.

What the stay cannot see: a plug-in changed with the keyboard still inside it keeps the old verdict until the keyboard next leaves; and a visit to the DAW's chrome that no evaluation *samples* is not seen either. A focus change on a Mac only marks the focus as changed, and the round that follows reads the chain as it is by then, while moving into a view that publishes nothing raises no notification at all — so Shift+Tab to REAPER's FX list, Down, Tab back, all done before a busy event loop gets to its queue, reads as one round with the keyboard inside, and the old verdict goes on over the newly selected plug-in. `host.window.recheck`, and so the focus key's "already inside" press, is one more evaluation: it undoes an `identify` that could not tell yet, not a verdict the stay keeps.

The verdict is logged when it differs from the one logged last for that window and place, naming the overlay that asked: `attachEmbedded: [host-panel] panel of '<title>' id=<id> at <dx>,<dy> <w>x<h> from its content origin, identify=<verdict> — '<label>'`. One line per VM, when the first binding with no control of its own for the platform is bound, says that it takes the panel: `[overlay] on macos an embedded binding with no control of its own takes the DAW's plug-in panel … '<label>' is the first here`.

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

**Signature:** `menus = { test, … }` in the options of `O:attach`, `O:attachEmbedded`, `O.window` and `O.embedded`, where each `test` is `(o: Overlay, answer: (seen: any) -> ()) -> ()` or `{ name: string?, cheap: boolean?, test: (o, answer) -> (), pressed: ((o) -> ())?, forget: ((o) -> ())? }` · the building blocks `O.menuTests.nativePopup`, `O.menuTests.newWindow`, `O.menuTests.accessibility`, `O.menuTests.accessibilityAfterPress`

The tests that tell an overlay its plug-in has a menu open, listed in its `menus` option; while a menu is open, the keys belong to it. The overlay gives up its per-control hotkeys — a registered hotkey outranks the application, so `Alt+M` would otherwise re-fire the control under the menu — and tells the key hook through [`host.keys.menuOpen`](keys.md#host-keys-menuopen) to let the navigation keys it captures (`Tab`, `Return`, `Space` and whatever the focused control holds) through to the application. **How a menu is seen is the module's decision**: the module names the tests, and the runtime runs them. ReaHotkey does the same with one fixed check, `WinExist("ahk_class #32768")`; a plug-in that paints its own menu needs a check of its own, and only its module knows which.

A test answers one question — *is a menu open over this overlay's plug-in right now?* — by calling `answer(seen)` exactly once per call, at once or later. Any value other than `nil` and `false` counts as seen, so a search's hit can be handed straight on. An answer that is a table with numbers `x`, `y`, `w` and `h` (`w` and `h` above 0) also says **where** the menu is, on screen, which is what an item is chosen in ([`O:chooseMenuItem`](#o-choosemenuitem)): it has to be the whole menu, since an item outside it is not clicked. `newWindow` answers with one; a search's hit is such a table too, but only of what its template shows — hand one on as the menu's rectangle only when the template is the whole menu, and otherwise answer the menu's own rectangle worked out from it. `o` is the overlay, so `o:origin()` is the plug-in's window or control. A synchronous check answers before it returns; a test that starts [`host.screen.imageSearchAsync`](screen.md#host-screen-imagesearchasync), `matchCellsAsync` or `snapshotAsync` answers from the callback, which the runtime receives on the main thread like every callback. A bare function is a test named `test <n>` after its place in the list and is not cheap. The table form adds:

- `name` — what the log calls it. Default `test <n>`.
- `cheap = true` — may run on every tick even when nothing was pressed (see below). Default `false`.
- `pressed(o)` — called when a control with `opensMenu` is activated, before it acts: for a test that compares against that moment.
- `forget(o)` — called when the user presses one of the overlay's own keys (a navigation key or a hotkey) while no menu counts as open — so the last press opened nothing that took the keys; while an item waits for its menu ([`O:chooseMenuItem`](#o-choosemenuitem)), once the key has run, and not for a second press of the same control — when the overlay leaves the front, and when the plug-in has the keyboard again after the overlay held its place over a menu that was a window in front (see below): a comparison against that press ends there.

`opensMenu = true` on a control ([`addHotspotButton`](#o-addhotspotbutton), [`addCustomButton`](#o-addcustombutton), [`addOCRButton`](#o-addocrbutton), `addGraphicalButton`) says that activating it puts a menu on screen. It makes the tests run on every tick for a while, calls their `pressed`, and in a calibrating run takes the [menu shots](calibrating.md#host-calibrating). It never counts a menu as open by itself.

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
- **The keyboard left in the menu's own window.** A plug-in can hide its menu's window instead of closing it, and the keyboard can stay in that window while nobody can see it. Komplete Kontrol in REAPER is the case this was built for: after `Escape` the first key was lost, and the overlay came back only about four seconds later, when REAPER's FX window was in front again — most likely because KK only hid its menu's window and the key went into it; that has not been measured. [`host.window.active()`](window.md#host-window-active) cannot show that state: it reports a hidden window as none, and answers from what was read in the current epoch. [`host.window.foreground()`](window.md#host-window-foreground) can: the foreground window, shown or not, read afresh. So when a hold ends because no test sees the menu any more, the runtime asks it, and when the foreground window is the **menu's own window** — the first one the hold was over after the press, of the press's process — and it is **not shown**, the runtime brings back the window that was in front at the press with [`host.window.focus`](window.md#host-window-focus) — **once per press**, whatever the answer — and asks the binding's own match again at once; from there it is as above. First it checks that the window of the press is still listed by [`host.window.list`](window.md#host-window-list) for its process with its class, because a menu item can close it and Windows reuses a gone window's id; `list` lists titled windows only, so an untitled window of the press is never brought back. If it is not listed, or the listing fails, nothing is brought back, and the log says so. Nothing is announced: the log line gives the platform's answer, `accepted` or `declined`, and whether the overlay is back is the match's to say. On Windows an accepted request is completed when REAPER's thread gets to it, so the match asked at once most likely still finds the hidden window in front — nothing, to `active()` — and the overlay leaves the front, unless the word still says the menu is open; REAPER's foreground event, when the change is made, brings it back with the keyboard in the plug-in, through an activation that speaks its control. Nothing is brought back when the foreground window is shown, or is any other window — a dialog the menu led to, a submenu that is a window of its own, another of the same application, the window of the press, another application's — or when there is none, or it cannot be read (the call raises, or on a Mac the application does not answer); the reading is then logged instead, once per press. The menu's own window getting the keyboard back after a dialog it led to has closed does count: it is still the menu's window, not shown. The step runs when the hold ends: with `newWindow` that is the tick that closes the menu, at its second miss; an overlay with `pollMatch` can reach it on its poll's first miss, while the word still says open, because the poll asks the tests itself and a hold needs a test seeing the menu there and then — `shown = false` is what keeps one missed reading of a menu that is still up from bringing anything back.

No timer decides any of it: the window in front, the press, the tests' answers and the window that gets the keyboard do. With nothing in front — the moment between a popup closing and its owner coming back — a hold over a menu the tests still see stands until a window is. Only an overlay with menu tests holds its place.

`menus = true` **raises** when the overlay is bound, with a message naming the building blocks. So does anything that is not a list of tests, a list with a gap in it (a `nil` before another entry, which is what a misspelt name leaves), and a `pressed` or `forget` that is not a function; reading a name that `O.menuTests` does not have raises as well. `menus = false`, `{}` or no `menus` means no tests: nothing runs, and a control declared with `opensMenu` on such an overlay is reported when the overlay binds.

The log lines start with `[menu]`: which test saw a menu first, which control's press it followed (or that none did), and the keyboard focus at that moment ([`host.window.focusChain`](window.md#host-window-focuschain)`()[1]`); the keys that went through to an open menu; whether the menu was still seen two ticks after an `Escape` went through; a test that has not answered for 20 ticks; and, when the menu closed, how long it counted as open. For a menu that is a window in front: `… came to the front after '<control>' was pressed and a test sees a menu — the overlay holds its place while it is up, holding no keys`; `the plug-in has the keyboard again` when a hold ends with the binding matching; `no longer holding its place — in front now: …` when it ends any other way, naming what [`host.window.active()`](window.md#host-window-active) answers — which can be a window read earlier in the epoch that has hidden since, so it does not say where the keyboard was; `no test sees the menu any more; the keyboard goes to id=<id> of pid <pid>, shown` (or `not shown`, or `no window the platform names`, or `a window that could not be read (<reason>)`) `, and the menu's window '<title>' (<exe>) is id=<id> of pid <pid> — nothing is brought back`, which does, when a hold ends with no test seeing the menu and nothing is brought back; and `no test sees the menu any more, but its window '<title>' (<exe>) still gets the keyboard and is not shown — bringing back '<title>' (<exe>), where '<control>' was pressed: accepted` — or `declined`, followed by the reason when the call raised — or, when the window of the press is not found, `… is not shown — the window of the press, '<title>' (<exe>), is not listed for its process with its class (gone, untitled, or its id now another window's), so nothing is brought back`, or `… could not be looked for (<reason>), so nothing is brought back`. The overlay's `[keys] … gave up its per-control hotkeys (menu open)` and `… took back …` lines mark the hand-over itself.

The building blocks are tests like any other, to be listed where they see a plug-in's menus:

| Block | Sees | Cost | Cheap |
| --- | --- | --- | --- |
| `O.menuTests.nativePopup` | A menu the operating system drew, open in the application in front. | One flag read, [`host.keys.nativeMenuOpen`](keys.md#host-keys-nativemenuopen). | yes |
| `O.menuTests.newWindow` | A window of the plug-in's process that was not there when a control with `opensMenu` was activated: a popup menu drawn as a window of its own. The process is settled at that press and kept. It compares only after such a press, and stops when the windows that appeared have all gone, at `forget`, and at the next press, which starts a new comparison. While it compares, **any** new window of that process counts — a dialog as well as a menu, whether it takes the front or not; one that takes the front keeps the overlay holding its place, with no keys, until it is gone (above). A menu no control of the overlay opened is not seen. It answers with the menu: `{ kind = "window", id, window, layer, class, x, y, w, h }` of the **largest** window that appeared (a toolkit can draw a popup's shadow as windows of their own, thin strips round it), the first the window list names of two the same size; `id` as [`windowsOf`](window.md#host-window-windowsof) lists it — which [`host.window.ownsPoint`](window.md#host-window-ownspoint) takes with `listed = true` — and `window` the same window as a `host.window` id where there is one (see the platform sections). | One [`host.window.windowsOf`](window.md#host-window-windowsof) of one process per tick while it compares, nothing otherwise. | yes |
| `O.menuTests.accessibility` | An element of the Menu type anywhere in the plug-in's accessibility tree, [`host.element.find(origin, "", host.element.type.Menu)`](element.md#host-element-find). An element that stays in the tree while nothing is drawn counts as a menu for as long as it is there. | A walk of the plug-in's whole tree: measured at 50–194 ms on Windows. Answered once per [epoch](timer.md#host-epoch), which a tick does not turn over, so a tick can get the previous tick's answer. | no |
| `O.menuTests.accessibilityAfterPress` | What `accessibility` sees, asked only from the activation of a control with `opensMenu` until that press is done with: the menu it opened was seen and has closed (answered no twice in a row), one of the overlay's own keys arrived with no menu open, the overlay left the front, or the plug-in has the keyboard again after the overlay held its place over a menu that was a window in front. For a plug-in whose tree can hold a Menu element while nothing is open: such an element keeps the overlay out only until the user leaves the plug-in and comes back. A menu no control of the overlay opened is not seen. | The same walk from a press until it is done with, nothing otherwise. | no |

A menu the plug-in paints inside its own window, with no element for it, is seen by none of them — the window list does not change and nothing is announced. That is what a module's own test is for: take the menu shots in a calibrating run, then test a pixel or an image that only an open menu has.

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

-- A module's own test, one pixel: a plug-in whose list, while it is open, has a white border
-- at content point (412, 140) — the kind of point read off the menu shots. A pixel read costs
-- one compositor frame (~16.7 ms on Windows), so it is not declared cheap: every tick after a
-- press and while the list is open, once in eight ticks otherwise.
local list = {
  name = "preset list",
  test = function(o, answer)
    local c = o:origin().client
    local p = host.screen.pixel(c.x + 412, c.y + 140)
    answer(p ~= nil and p.r > 200 and p.g > 200 and p.b > 200)
  end,
}

-- And an image, answered from the search's callback: a hit counts as seen, nil as not.
local fileMenu = {
  name = "file menu",
  test = function(o, answer)
    local b = o:origin().bounds
    host.screen.imageSearchAsync(host.path("images/FileMenu.png"),
      { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
      function(hit) answer(hit) end)
  end,
}

ov:attachEmbedded(spec, { menus = { O.menuTests.nativePopup, list, fileMenu } })
```

### Windows {#o-menutests-windows}

`nativePopup` is true while the foreground thread is in menu mode — a `#32768` popup menu, a menu bar, a window's system menu. A menu open in another application does not count. `newWindow` compares visible top-level windows of the process, tooltips left out, so a popup the toolkit draws as a tool window of its own counts; its answer's `window` is the popup's HWND, which is what every `host.window` call names, and its `id` the same number, so [`host.window.ownsPoint`](window.md#host-window-ownspoint) can be asked about it either way. `accessibility` and `accessibilityAfterPress` are a UI Automation search of the plug-in control's subtree for any element of the Menu type, with no visibility test: a Qt menu is seen while it is up, and so is any menu a plug-in keeps in its tree while hidden.

Holding its place: a window is "in front" when it is the foreground window. A `#32768` menu and a menu bar's menus do not become it, so an overlay never needs to hold its place for them; a toolkit's popup that takes the foreground does — Komplete Kontrol's in REAPER (2026-09-26). That window is a full-screen one of `reaper.exe`, and after `Escape` Komplete Kontrol most likely only hides it: no foreground event came for about four seconds, which a destroyed window would most likely have caused — not measured. While a hidden window is the foreground window, [`host.window.foreground()`](window.md#host-window-foreground) answers it with `shown = false` — hidden, minimised and cloaked all count as not shown — and `host.window.active()` answers `nil` once the epoch has turned over; until then it can still answer the window it read while that was shown. The window of the press is brought back with `SetForegroundWindow`, whose `true` means Windows accepted the request: the change is made when REAPER's thread gets to it, with a foreground event, and it does not put the keyboard on any particular control. Where the keyboard is afterwards is REAPER's to decide: in the plug-in, the overlay is in front with its keys; on the FX window itself or its FX list, which the [chrome gate](#o-attachembedded) counts as REAPER's own, it is out until the keyboard reaches the plug-in. The process is the window's `app.pid`: a bridged plug-in's popup is a window of the bridge process, so it is held when the window in front at the press belongs to the bridge as well (REAPER's own bridge window), and not when the bridged plug-in is drawn inside the host's FX window — the overlay then leaves the front for it as it always did.

### macOS {#o-menutests-macos}

`nativePopup` counts the frontmost application's `AXMenuOpened` / `AXMenuClosed` notifications: a menu counts as open from its opening notification until its closing one, however long it stays up, and the count is cleared when another application comes to the front (see [`host.keys.nativeMenuOpen`](keys.md#host-keys-nativemenuopen)). Holding its place asks [`host.window.active()`](window.md#host-window-active) for the window in front, so it applies when a plug-in's popup becomes the application's focused window. A popup that does not become it leaves the overlay's window in front, and holding its place has no part in what happens then: the binding's own match decides — on the DAW's plug-in panel of [`attachEmbedded`](#o-attachembedded-macos), or a function's control, the accessibility focus on a menu item outside the panel takes the overlay out, as any element outside it does. `host.window.active()` answers nil while the frontmost application has no window (the Finder after a click on the desktop): a hold over a menu the tests still see stands through that, and ends once they stop seeing it. The window that gets the keyboard is the frontmost application's focused window, and [`host.window.foreground()`](window.md#host-window-foreground) reports it as shown unless it is minimised, so a focused menu window brings nothing back here. While the application does not answer, `foreground()` is `nil`, and nothing is brought back either: the runtime does not act on a state it could not read, and does not call `host.window.focus` on an application that is not answering. The reading itself is still taken once at each hold that ends with no test seeing the menu: two accessibility reads of the frontmost application, which against one that has just stopped answering can take up to the one-second timeout once, on the main thread. If the window of the press is brought back, `host.window.focus` raises it and activates its application, and `true` means raised and in front, not that the keyboard is in it (see [`host.window.focus`](window.md#host-window-focus)). The panel and the function form of `attachEmbedded` read an empty focus chain as "not in the plug-in", so the overlay is back once the chain names something inside the panel. `newWindow` compares on-screen windows by owning process with no filter at all, so a tooltip or a window at any level counts as well; an `NSMenu` sits at window level 101. Its answer's `id` is the window server's `CGWindowID`, a numbering `host.window` ids are not in, so its `window` is `nil`; what is drawn over the menu is asked by that `id`, with [`host.window.ownsPoint`](window.md#host-window-ownspoint)'s `listed = true`. `accessibility` and `accessibilityAfterPress` are special-cased by the host to "is a menu open in this application", without descending the menu bar. An application the host has found not answering is left alone for 5 seconds, and in that time the answer is no: a menu only these tests see then counts as closed after two ticks, and as open again once the application answers. The one-miss rule does not cover that.

## O\:chooseMenuItem(spec) — choosing an item in a plug-in's menu {#o-choosemenuitem}

**Signature:** `O:chooseMenuItem(spec: { label: string?, at: {number, number} | ((overlay) -> {number, number}?), fromRight: boolean?, rawOrigin: boolean?, button: ("left" | "right" | "middle")?, menuItem: {number, number} | ((overlay, menu, factor) -> {number, number} | false | nil), retries: number?, onDone: ((overlay, chosen: boolean, why: string?) -> ())? }) -> boolean` · the control form: `menuItem` on [`addHotspotButton`](#o-addhotspotbutton)

Opens a plug-in's own popup menu and chooses an item in it: clicks the opener at `at`, clicks the item once one of the overlay's [menu tests](#o-menutests) **sees** the menu and says where it is, and counts it as chosen once the menu is seen to have taken it. A JUCE plug-in's menus are popups it draws itself — windows of their own, with no item anybody can press through the accessibility layer; VPS Avenger's MENU, zoom list and redo list are such popups by the avenger_control project's account. The obvious alternative, a hotspot with `points = { opener, item }`, does not work: every click of a sequence asks [`host.window.ownsPoint`](window.md#host-window-ownspoint) of the plug-in's window first, a popup is a window of its own over it, so the item's click is refused as covered; and the `settle` between the two would be a timer deciding that the menu is up by then.

The method is for a module's own code — a stepper whose step is a choice from a list, as Avenger's zoom is. A hotspot with `menuItem` does the same as a control: its `at` (with its `button`) is the opener, its `menuItem` the item, its `retries` the retries, and it has no `onDone`.

`label` names the choice in what is said and logged; `"Menu item"` when it is not given.

What happens, in order:

1. **The opener** is placed like a hotspot's `at` — origin, frame, [scale](#o-scale), `fromRight`, `rawOrigin` — and checked like any hotspot's click (inside the origin, nothing else drawn there). One that cannot be placed — an `at` function that answers `nil`, or no factor — is **said**, `"<label> is not available now"`, unlike a hotspot's silent `nil`; one that is refused is said as the check says it. Either way nothing more happens: `onDone(overlay, false, "unplaced" | "refused")`, and the method returns `false`.
2. **The press.** Only then does it count as the press of a control with `opensMenu`: the tests run on every tick from here, `newWindow` takes the window list it compares against, and in a calibrating run the [menu shots](calibrating.md#host-calibrating) are taken. An item the overlay was still waiting to choose for **another** control is dropped (logged). The opener is clicked with `button` (left unless given), and the method returns `true`; the item is chosen later.
3. **The menu.** Each answer of a test that sees a menu is looked at, as it arrives, on the tick it arrives or in a test's callback. One that says where the menu is — a table with numbers `x`, `y`, `w`, `h`, as `newWindow` answers — places the item. A table `menuItem` is the item's offset from the menu's top-left corner, a **distance**: scaled at the factor the opener was placed with (the popup is drawn at the zoom it opened at, and a factor read off the plug-in's screen may not be readable while the popup covers it), never framed. A `menuItem` function is called with the overlay, that table and that factor (`nil` in an overlay without a scale), and answers the offset in **screen pixels** — it works from the menu's rectangle, which is in screen pixels, so its answer is not scaled again, as a `dragBy` function's is not; `false` says "this window is not my menu" (a tooltip that appeared first), and the next answer is looked at; `nil` is an item that cannot be placed. An answer that sees a menu without saying where — `nativePopup`, the accessibility tests — places nothing, and is logged once; another test on the list may still say where, and while an item waits every test on the list is asked, a cheap one that sees the menu holding back none.
4. **The checks.** The item must lie inside the menu's rectangle; if it does not, it is not clicked, the log says where it fell, and the next answer is looked at — a later answer can be the menu itself where this one was something else that appeared. Then what is drawn there: a menu that is a window is asked [`host.window.ownsPoint`](window.md#host-window-ownspoint) about *itself* — by its `host.window` id (`window`, which `newWindow` gives on Windows), or by its `id` in the window list's numbering with `listed = true` (macOS) — and a `false` refuses the click; a menu with no `kind` — a rectangle a module's own test answered, for a menu painted inside the plug-in — gets the check every click of the overlay gets.
5. **The click**, at once, in that same pass: no wait of any kind stands between the menu being seen and the item being clicked.
6. **Taken, or clicked again.** A menu does not always take a click that comes right after it opened: JUCE's popup ignores a mouse-up in its first moments (a quarter of a second in its source as it was read for this, not measured against any JUCE a plug-in here ships), and the first tick that lists a new window can come sooner. So the item keeps waiting after its click. An answer to a question asked on a **later** tick that still shows the same menu — the same window, at the same place and size — is a click the menu did not take: the item is checked again and clicked again, at most `retries` more times (default 3; a count of answers, not a time). The menu closing — for the menu tests, two answers in a row that do not see it — an answer that shows another window in its place, or the overlay leaving the front or moving on after the click, is the choice **taken**: `onDone(overlay, true)`. `retries = 0` is for a menu whose item keeps it open (a tick box in a menu), where a second click would be a second choice: chosen at the click, and not followed. The menu then closes the way any menu does for the menu tests, and the keys come back to the overlay with it.

A second call for the **same** `label` while its item is still waiting for its menu — a double Return, a held arrow on a stepper — clicks nothing: `"… pressed again while its menu is still being waited for — the opener is not clicked again; the first press stands"` in the log, `onDone(overlay, false, "busy")` for the second call, and `false` from it. The first press is not disturbed: one of the overlay's own keys that is such a repeat does not count as the user moving on either.

Nothing more is clicked, the item is dropped, `onDone(overlay, false, why)` is called, and:

| When | `why` | Said | Logged (`[menu item] '<overlay>': '<label>' not chosen — …`) |
| --- | --- | --- | --- |
| no test saw a menu within 8 seconds of the press — the time the tests run on every tick after a press, a bound on how long a press waits for its menu, not a guess that it is up | `due` | `"<label>: no menu was seen, nothing was chosen"` | `no menu test saw a menu within 8000 ms of the press` |
| the menu was seen only by tests that do not say where, by then or when it closes | `due`, `closed` | `"<label>: the menu does not say where it is, nothing was chosen"` | `test '<name>' sees a menu but does not say where it is, …` |
| the `menuItem` function took no window seen for its menu, by then or when it closes | `due`, `closed` | `"<label>: no menu it can be chosen in was seen, nothing was chosen"` | `its menuItem function took no window seen for its menu (the last at …)` |
| the item fell outside every menu seen, by then or when it closes | `due`, `closed` | `"<label> is not where it should be"` | `the item, at (x,y) of x,y wxh, is outside the menu that was seen` |
| the menu closed before any of that | `closed` | `"<label>: the menu closed before anything was chosen"` | `the menu closed before a test placed the item` |
| the item cannot be placed: `menuItem` raised or answered `nil` or no `{dx, dy}` | `unplaced` | `"<label> is not available now"` | `the item cannot be placed now: <why>` |
| another window is drawn over the item, at its click or a retry | `covered` | `"something else is covering <label>"` | `(x,y) is inside the menu's window …, but another window is drawn there` |
| a menu drawn inside the plug-in, and the overlay's own check refused the item | `refused` | what that check says | `refused, and said` |
| the menu was still there after the last retry | `untaken` | `"<label>: the menu did not take the choice"` | `the menu was still there after <n> click(s) on the item at (x,y)` |
| one of the overlay's own keys arrived with no menu open, the overlay left the front, another control's press came, or the overlay is on another window by the time the menu is seen or the 8 seconds are over | `key`, `left`, `press`, `moved` | nothing: the user moved on, and the key says what it does | `one of the overlay's own keys came first, …`, `the overlay left the front first`, `another press came first`, `the overlay is on another window now` |

At the call itself `why` is `inactive` (the overlay is not in front: nothing said), `no tests` (no menu tests: `"<label> is not available now"`), `unplaced` or `refused` (step 1), or `busy` (a repeat, above). The method **raises** at the call for a `spec` that is not a table or has no `at` of `{x, y}` or a function, a `menuItem` that is neither `{dx, dy}` nor a function, a `button` other than `"left"`, `"right"` or `"middle"`, and a `retries` that is not a whole number from 0: a mistake that would otherwise show only after the menu had opened.

A menu that stays open after its item was refused keeps the keys, as any open menu does: `Escape` goes to it, and a module that knows how its plug-in's popup closes can close it in `onDone` — [`O:menuOpen`](#o-menuopen) says whether a menu still counts as open. A menu that appears after the item was dropped is an ordinary menu: nothing is chosen in it.

The log says every step: `[menu item] '<overlay>': '<label>' opening its menu, clicking (x,y)`; `… clicked its opener — the item is chosen when a menu test sees the menu, for up to 8000 ms`; `… — test 'newWindow' sees the menu at 362,80 120x200 (window id 300) (its window id 300 is asked what is drawn there); choosing item (10,20) at (372,100)`; `… — test 'newWindow' still sees the menu at … a tick after the item's click, which it did not take: clicking (372,100) again, 2 of 4`; `… chosen — clicked 1 time(s) at (372,100); the menu has closed`; or a `not chosen` line from the table. In a calibrating run, a press that was photographed also writes the **menu item shot** — the menu with a crosshair on the item, taken just before its first click (see [calibrating](calibrating.md#host-calibrating)) — and the calibration shot marks the opener of every label this method has been called with, at the `at` it was last given, as `[menu opener]`, after the overlay's controls.

**Cost.** The opener's placement and click, one origin resolution at the press; per answer that sees a menu, the item's placement and one `ownsPoint`; per retry, the same again and one click; a calibrating run adds one capture before the item's first click. Nothing is asked of the screen that the menu tests were not asking anyway.

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
```

### Windows

A popup drawn as a top-level window of the plug-in's process (for a plug-in in a DAW, the DAW's process, unless the DAW bridges it) is what `newWindow` sees; its answer's `window` is that window's HWND, and the item is checked with `ownsPoint` against it — the window manager's hit test, taken up to the top-level window on both sides, which sees through a window that lets clicks pass. A toolkit can draw a popup's drop shadow as separate windows beside it, which is why the largest new window is taken as the menu.

### macOS

`newWindow` lists windows as the window server numbers them (`CGWindowID`), which is not the numbering of `host.window` ids, so its answer has no `window`, and the item is checked by the menu's `id` with `ownsPoint`'s `listed = true`: the window server's own hit test, with `nil` — no answer, which lets the click go — where it names no window, a window of VoiceOver's own, or one of this application's. The menu's rectangle and the click are both in points. A tooltip under the pointer is a window of the process as well, and nothing filters it: the largest new window is the menu, and an item that falls outside a smaller one that came first waits for the next answer (step 4).

## O\:menuOpen() {#o-menuopen}

**Signature:** `O:menuOpen() -> boolean`

Whether a menu counts as open over the overlay now: one of its [menu tests](#o-menutests)' word is that one is. `false` for an overlay with no menu tests, and while it is not in front. For a module's own code that tidies up after a menu of its own — closing a popup a choice left open by clicking its opener again — which must not open one that has gone. A table lookup.

```luau
-- VPS Avenger (modules/vps-avenger, shortened): a choice that could not be made leaves the popup
-- open; the zoom field closes the zoom list, as a second click on it does in the plug-in.
onDone = function(o, chosen, why)
  if not chosen and why == "unplaced" and o:menuOpen() then
    local x, y = o:toScreen(21, 14)
    if x then host.input.click(x, y) end
  end
end,
```

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

## O\:gate(fn) / O\:landmark(image) {#o-gate}

`gate(fn)` sets an extra activation condition ANDed onto the context match:
`fn(origin)` (origin = the active context's coordinate window/control) returns
whether the overlay should be active. `landmark(image)` is the common case — a gate
satisfied only while `image` (an **absolute** path, via `host.path`) is found within
the active context's region. A derived / library overlay uses a landmark to take
over from its base only when its product wordmark is on screen. Both return the
overlay (chainable).

On an overlay bound to an arbiter slot, the gate's answer is what the slot is decided on:
context match AND gate is reported to the arbiter as this claim's `matching`, and specificity
only ranks the claims that are matching. So a dialog overlay takes the slot exactly while its
gate says the dialog is there — see [host.arbiter](arbiter#matches).

**A gate whose condition can change without a window event needs `pollMatch`.** Gates are otherwise re-evaluated only when a window is activated or focused, which is enough for a dialog — opening and closing one *is* a window event — and not enough for anything that appears and disappears *inside* a window that never changes. ON:EAR's chooser panels are exactly that, and without the poll the overlay kept the arbiter slot after its panel had closed: a ring for a panel that was no longer on screen, which somebody who cannot see it has no way to escape. See `pollMatch` under `O:attach` below.

**Coordinate anchoring (opt-in):** pass `landmark(image, { anchor = true })` and, while
the landmark is on screen, the overlay's control coordinates resolve **relative to the
landmark's top-left** instead of the plugin's client area (a `rawOrigin` control still
uses the client). So an overlay whose landmark sits *in the same UI panel as its
controls* (a library's wordmark above its mixer) stays correctly positioned regardless
of host, window size, or plugin chrome/browser state — the landmark and the controls
move together. Author such controls relative to the wordmark (coordinates may be
negative if a control sits above/left of it). WITHOUT `anchor`, `landmark` is a pure
gate and controls stay client-relative — the right choice when the image is used only
to tell the overlay apart from a sibling on a shared slot (a dialog's header image).

```luau
-- A gate: sforzando inside REAPER is active only while the keyboard is in the PLUG-IN.
-- REAPER's own chrome answers with a focus chain several elements deep; the plug-in
-- exposes nothing, so focus reaches the window and stops. Depth is the discriminator.
inDaw:gate(function()
  local chain = host.window.focusChain()
  return not chain or #chain <= 1
end)

-- A landmark, anchoring: this library's controls are authored relative to its wordmark,
-- so they follow it wherever Kontakt draws the panel.
ov:landmark(host.path("images/MimiPage/Wordmark.png"), { anchor = true })
```

## O\:typingWhen(fn) {#o-typingwhen}

`fn() -> boolean`. While it returns true, the overlay **holds no keys at all** — not the navigation keys, not Space, not Return.

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

## O\:onActivate(fn) / O\:onDeactivate(fn) {#o-onactivate}

**Signature:** `O:onActivate(fn: (overlay) -> ()) -> Overlay` · `O:onDeactivate(fn: (overlay) -> ()) -> Overlay`

Hooks the module runs each time the overlay comes to the front and each time it leaves it. Neither may wait for a read ([Where a hook runs](#where-a-hook-runs)): an overlay on a `slot` runs them in the arbiter's call, a plain call of the host's where nothing can wait — `host.ocr.recognize` without a callback holds the event loop there — and any overlay is half way into or out of the front while they run. A hook that needs a read starts it with a callback. `onActivate` runs after the overlay has taken its keys and before its first control is announced (350 ms later), so the origin is known and a hook can prepare what that announcement reads; it is called guarded — a hook that raises is logged, `[overlay] <label>: onActivate failed: …`, and the overlay comes up all the same. `onDeactivate` runs after the keys have been handed back and the menu state dropped, and is not guarded: what it raises leaves the overlay's deactivation into the window event that caused it — the overlay is inactive and its keys are released by then — and the host logs it there. Both are set before the overlay is bound; afterwards they raise. One of each per overlay: a second call replaces the first.

"Comes to the front" is the overlay's activation, not a window event: an overlay that holds its place over its own menu stays active, and comes to the front again only after it has left it — so a hook that forgets a per-window reading here forgets it at every return to the plug-in, Alt+Tab included.

```luau
-- A scanning dialog says when a scan it committed to has gone (modules/komplete-kontrol does
-- this, keeping the flag in a field of its own; a module's state belongs in `o.state`).
scan:onDeactivate(function(o)
  if o.state.scanCommitted then host.speech.output("Scanning stopped", { interrupt = true }) end
  o.state.scanCommitted = false
end)

-- A module that reads the plug-in's zoom once per stay in front forgets it on every return.
ov:onActivate(function(o) o.state.zoom = nil end)
```

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
