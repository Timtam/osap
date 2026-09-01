---
title: "Overlay — the control ring"
sidebar_position: 1
toc_max_heading_level: 2
---

An overlay is the thing this platform exists to produce: a ring of controls laid over a plug-in no screen reader can read, which the user reaches with Tab and which speaks each control as label, type and value. It is **not a host capability but a module** — declare `com.platform.overlay` under `dependencies` in `module.toml`, never under `[capabilities] require`, and pull it in with `host.require`.

What comes back is very nearly the whole of most modules: Impact Soundworks' Juggernaut is a table of measured coordinates and two library overlays of a caption and one `addOCRButton` each. How a control reads its value is the cost you are choosing — a hotspot toggle samples a single pixel, which is one compositor frame (~16.7 ms on Windows); a graphical toggle image-matches templates over a region; OCR is slower than either, which is why `ocrLabel` belongs on controls whose name really does change with the loaded patch and not on every control.

One plug-in is usually several overlays rather than one. Kontakt declares one per cell of version-by-environment, and overlays sharing an arbiter slot rank by `O.layer`, so Komplete Kontrol's chrome yields to the Kontakt inside it, that to the library loaded in that, and all three to a modal while it is up.

Running through every method here is one rule: **announce what the application did, not what the module intended.** `O:watch` waits for a value to change instead of guessing a delay, a pixel that resembles neither reference reports no state at all rather than the nearer guess, and a click is refused when another window is drawn over the point.

All control coordinates are **origin-relative**: the origin is the client-area top-left of the active context's coordinate window — the plug-in window when standalone, or the embedded plug-in's child control when hosted in a DAW — re-resolved per call so it tracks the window as it moves, and `(0, 0)` when the overlay is not attached.

A control is spoken as `"label, type[, value]"`, and **every kind is a focus stop**: static text is Tab-reachable and read aloud, it simply has no activation.

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

## O.new(label) {#o-new}

Creates a new overlay object. `label: string?` (defaults to `"Overlay"`).

Returns an `Overlay` (metatable-backed table) with fields `label`, `controls = {}`, `focus = 0`, `active = false`, `hoverToRead = false`, `contexts = {}`, `activeCtx = false`, and internal registration/key/trigger state.

```luau
-- sforzando's standalone overlay: the object, its controls, then where it lives.
local O = host.require("com.platform.overlay")
local ov = O.new("sforzando")   -- the name announced when the overlay comes up
ov:addOCRButton({ label = "Instrument", region = { 90, 22, 200, 36 }, opensMenu = true })
ov:addOCRButton({ label = "Polyphony", region = { 486, 40, 516, 70 }, opensMenu = true })
-- `menus = true`: while sforzando's own pop-up list is open, the keys belong to it.
ov:attach({ title = { contains = "sforzando" }, windows = { class = "PLGWindowClass" } },
  { menus = true })
```

## O\:addStaticText(label) {#o-addstatictext}

Appends a static text control: Tab-reachable and read aloud on focus, but with no activation (Enter does nothing). `label: string`.

Returns the control table `{ kind = "static", label = label }`.

`labelOrOpts` may also be a **table** — `{ label, text, when }` — and that form is the one worth knowing. `text(overlay) -> string?` is appended to the label each time the control is announced, which turns a caption into a **read-out**: a value the user wants to check is then read by arriving at it, with nothing pressed and nothing changed. The alternative would be a button, and a button that only reports is a promise of an action it does not perform.

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

Appends a button that, when activated, clicks a fixed origin-relative point. `opts: { label: string, at: {number, number}, hotkey: string?, rawOrigin: boolean?, fromRight: boolean? }` — `at` is `{x, y}` relative to the origin; `hotkey` is an optional global activation hotkey spec (e.g. `"Alt+P"`).

Returns `{ kind = "hotspot", label, at, text, hotkey, rawOrigin, fromRight, opensMenu, when }`. On activate it clicks `(origin.x + at[1], origin.y + at[2])` and speaks `"label, activated"`.

Two checks come before the click. The point must fall inside the origin's own frame, and — where the platform can say — the window it belongs to must be the one actually **drawn** at that point; see [`host.window.ownsPoint`](window#host-window-ownspoint). A coordinate inside our rectangle can still be covered by a notification or another application, and a click that lands somewhere unknown while the overlay announces "activated" is a press with no way to tell where it went.

`text` is announced between the label and the word "button", and is how a control that acts can also say something about itself — whether it is available, or what it would act on.

`fromRight` measures `at[1]` from the coordinate window's **right edge** instead of its left — the click lands at `origin.x + width − at[1]` (ReaHotkey's `ControlX + ControlWidth − N`). Use it for plugin UI laid out from the right, so the target stays correct whatever the plugin's width is. Also accepted by [`addHotspotToggle`](#o-addhotspottoggle).

`points` replaces `at` with a **click sequence** — `points = {{x1,y1},{x2,y2},…}`, with `settle` ms between each (default 400) — for UI where reaching a control means getting there first: switch to its tab, then its sub-tab, then click it. Each step re-resolves the origin and goes through the overlay's own coordinate resolution, so a frame offset, `rawOrigin` or landmark anchoring applies exactly as for a single point. Making that one control keeps every action self-contained, so a user who cannot see a tab structure never has to navigate it.

`at` may also be a **function** `(overlay) -> {x, y} | nil`, for a control whose position depends on what is focused right now — one overlay serving several versions of a plugin whose chrome moved between them. Returning `nil` (the version isn't known yet) skips the click rather than guessing. Also accepted by `addHotspotToggle`.

```luau
ov:addHotspotButton({ label = "Play", at = { 120, 40 }, hotkey = "Alt+P" })
-- 352 px in from the right edge, 87 px down — Kontakt's instrument arrows:
ov:addHotspotButton({ label = "Previous instrument", at = { 352, 87 }, fromRight = true, rawOrigin = true })
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

A `when` predicate must return exactly `true`; anything else counts as hidden. A hidden control is not Tab-reachable, its hotkey does nothing, and it does not claim a key combination that a visible sibling wants.

## Bindings — O.window / O.embedded / \:with / O.hosts / O\:bind {#bindings}

**Signatures:**
`O.window(matcher, opts?) -> Binding` · `O.embedded(spec, opts?) -> Binding` · `Binding:with(over) -> Binding` · `O.hosts(...) -> {matcher}` · `O:bind(binding, opts?) -> Overlay`

A **binding** is an inert value saying *where* an overlay lives: which window or embedded control, on which `slot`, at which `specificity`, with `pollMatch` / `menus`. Declare it once and reuse it, instead of writing a factory function that rebuilds an attach spec per attachment — the reason such factories appear is that a caller mutates the table and the next caller needs a clean copy.

`:with(over)` returns a **copy** with `over` merged into the target (`control`, `identify`, `title`, …); `over.opts` merges into the options. `O.hosts(...)` concatenates host lists and skips `nil`, so an optional host (a plugin that may not be installed) can be listed inline. `O:bind(binding, opts)` attaches, with `opts` overriding the binding's own options.

```luau
local HOSTED = O.embedded({
    hosts = O.hosts(daw.all, kk and kk.standaloneWindow),
    control = "Qt%d+.-QWindowIcon",
    identify = identifyHost,
    cacheIdentity = false,
}, { slot = SLOT, menus = true })

base:bind(HOSTED, { specificity = O.layer.base })
dialog:bind(HOSTED:with({ control = "^NIChildWindow%x+$", identify = present,
                          opts = { menus = false } }), { specificity = O.layer.dialog })
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

```luau
local variantOf = O.memoByOrigin(function(ctrl)
    local i = host.uia.findAny(ctrl.id, VERSIONS, { U.Window, U.Pane })
    return i and VERSIONS[i] or nil   -- nil: UIA not ready, ask again next time
end)
```

## O\:origin() / O\:hwnd() {#o-origin}

**Signature:** `O:origin() -> Control | Window | nil` · `O:hwnd() -> number | nil`

The active context's coordinate window — the plugin control when embedded, the window when standalone — and its handle. `nil` while the overlay is not active. This is the module's handle on the thing it overlays; use it instead of reaching into `activeCtx`.

```luau
-- `:hwnd()` -- the window handle, for asking the accessibility layer about it:
onActivate = function(o)
  local p = host.uia.locate(o:hwnd(), "", host.uia.type.Edit)
  if p then host.input.click(p.x, p.y) end
end,

-- `:origin()` -- the context itself, whose `client` rect every authored coordinate is
-- measured against. ON:EAR scales all of its to the size the window is really drawn at:
local function screenPointOf(overlay, dx, dy)
  local o = overlay:origin()
  if not (o and o.client and o.client.h > 0) then return nil end  -- not active: no point
  local k = o.client.h / DESIGN_H
  return math.floor(o.client.x + o.client.w / 2 + k * (dx - DESIGN_W / 2) + 0.5),
    math.floor(o.client.y + k * dy + 0.5)
end
```

## O\:frame(fn) {#o-frame}

**Signature:** `O:frame(fn: (origin) -> (number, number)) -> Overlay`

Shifts the overlay's whole coordinate frame: `fn` returns `dx, dy`, resolved per active control — a host version's content shift, or a nested plugin's inner origin. Controls marked `rawOrigin` opt out.

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

Costs one OCR read per focus, so put it on controls whose name genuinely varies, not on every control.

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

`onActivate` is called **guarded**. A control whose action throws says so rather than going silent: the host used to catch the error at the key dispatch and write a line, while the person who pressed it heard nothing at all — which is indistinguishable from a control that quietly did its job.

`text` is announced between the label and the type word, and is how a control that ACTS can also say what it would act on.

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

Appends a **value changed with Left and Right**, where the module knows how to change it. `opts: { label: string, text: (overlay) -> string?, onStep: (dir: number, overlay) -> (), onActivate: ((overlay) -> ())?, settle: number?, typeLabel: string?, when: ((overlay) -> boolean)? }`.

Announced as a slider, because that is what it is to the person using it; how it is driven underneath is the module's problem rather than theirs.

Use it where `addSlider` cannot serve. That one finds its thumb by matching an image, which needs a template captured for one plug-in at one size — no use for a rotary drawn as an arc, a bar with two handles, or anything in a window the user can resize. What such a control does have is a value printed beside it and something that moves it, and that is all a stepper is.

- `onStep(dir, overlay)` — `dir` is `-1` for Left and `+1` for Right. Called guarded.
- `onActivate` — optional, and what a **press** means. Without it, Space and Return are still captured on a stepper (it is not an inert control) and then do nothing at all, which is a promise without an action. ON:EAR's two use it for "double-click to put this back to its default", which is one keystroke instead of twenty.
- `settle` — how long to wait **at most** for the value to change before announcing it anyway. Not how long to wait: see [`O:watch`](#o-watch).

What is announced afterwards always comes from reading `text` again, never from what the step intended. A control that reports its own intention rather than the application's state is the failure this project keeps returning to.

```luau
ov:addStepper({
  label = "Tone",
  when = function() return element("Tone") ~= nil end,
  text = function() return valueBelow("Tone") or "not shown" end,
  onStep = function(dir)
    local x, y = screenPoint(290, 780)
    if x then host.input.scroll(x, y, dir * 0.5) end
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

## O\:addOCRButton(opts) {#o-addocrbutton}

Appends a button whose label/value is read live by OCR over a region; activating re-reads it then clicks the region centre. `opts: { label: string, region: {number, number, number, number}, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative.

Returns `{ kind = "ocr", label, region, hotkey }`. When focused/spoken it appends the OCR text (or `"no text"`) as the value.

```luau
-- u-he draws its whole interface itself: the preset name exists nowhere but on screen.
-- Focusing reads it; pressing opens u-he's own preset menu, which `opensMenu` hands the
-- keys to.
ov:addOCRButton({ label = "Preset menu", region = { 480, 20, 720, 48 },
  hotkey = "Alt+M", opensMenu = true })

-- `readOnly`: focusing re-reads the articulation, but a press would drop the user into a
-- list no screen reader can follow -- so this one announces and never clicks.
ov:addOCRButton({ label = "Articulation", region = { -115, 114, 165, 152 },
  readOnly = true, hotkey = "Alt+B" })
```

## O\:addGraphicalToggle(opts) {#o-addgraphicaltoggle}

Appends a toggle whose on/off state is read by image-matching its region against an "on" and "off" template; activating clicks the region centre and re-reads the new state after ~150 ms. `opts: { label: string, region: {number, number, number, number}, onImage: string?, offImage: string?, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative; `onImage`/`offImage` are template image paths.

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

Appends a toggle whose on/off state is read from a **single pixel** at its click point (ReaHotkey's `HotspotToggleButton`) — cheap (one screen touch), where a region scan would cost ~16 ms *per pixel*. The pixel is compared to an on/off reference colour and the **nearest** wins. Activating clicks the point (toggling it) and then [waits for the state to actually change](#o-watch) before announcing it, giving up after ~900 ms. A control that redraws in thirty milliseconds is announced in thirty; one that takes half a second is announced correctly instead of early; one that does not change at all — the already-chosen member of a radio group — is announced at the deadline, which is the truth about it. `opts: { label: string, at: {number, number}, onColor: {number, number, number}, offColor: {number, number, number}, hotkey: string?, rawOrigin: boolean? }` — `at` is `{x, y}` origin-relative; `onColor`/`offColor` are `{r, g, b}`.

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

Moves focus to the next control (wrapping) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. No-op when there are no controls. Returns nothing.

Tab is already bound to this by the runtime, so a module calls it only to give the ring a second, more natural key.

```luau
-- ON:EAR's five preset slots are a row, so Alt+Right should walk them too.
host.hotkey.register("Alt+Right", function()
  if ov.active then ov:focusNext() end   -- a global key: only steer an overlay that is up
end)
```

## O\:focusPrev() {#o-focusprev}

Moves focus to the previous control (wrapping) and speaks it. No-op when empty. Returns nothing.

```luau
-- The mirror of the above, and worth having for the wrap: with nothing focused yet this
-- lands on the LAST control, because the end of the ring is where you look when you suspect
-- you missed something.
host.hotkey.register("Alt+Left", function()
  if ov.active then ov:focusPrev() end
end)
```

## O\:activate(index) {#o-activate}

Activates the control at `index` (defaults to the focused control). `index: number?`. Behaviour by kind: `hotspot` clicks `at` and speaks `"label, activated"`; `custom` calls `onActivate(self)`; `ocr` re-reads then clicks the region centre; `gtoggle` clicks the region centre and re-reads state after ~150 ms. No-op for `static` or a missing control. Returns nothing.

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

Binds the overlay as a **standalone** context: active while a window matching `matcher` is the foreground/active window, with coordinates relative to that window's client area. `matcher` is a window matcher passed to `host.window.test`; `opts: { hoverToRead: boolean? }?`.

While active, the overlay captures and suppresses the navigation keys, scoped to its own window (so `Alt+Tab` and menus pass through natively): `Tab` / `Shift+Tab` move between controls, `Return` and `Space` activate the focused control, and — when the overlay has a tab control — `Left`/`Right`, `Ctrl+Tab`/`Ctrl+Shift+Tab` and `Ctrl+<n>` drive it. `Space` is released while an editable field (an `ocredit` control) is focused, so a literal space can be typed into it. On activation the overlay starts at its first control (and any tab control at its first tab) when a **genuinely new** window opened, but resumes the last-focused control when the *same* still-open window merely regained the foreground (`Alt+Tab` out and back); the two are told apart by the window's identity (its HWND). `hoverToRead` (default `false`) moves the mouse onto an OCR control on focus (some UIs only reveal values on hover). Registers the foreground/focus trigger once. Returns nothing.

```luau
-- Melodyne's standalone window, by executable and window class.
ov:attach({
  app = { exe = { contains = "Melodyne" } },
  windows = { class = "GNWindowDoc" },
}, {
  -- Melodyne has a native menu bar: while a menu is open the captured navigation keys
  -- have to reach it rather than steer the overlay.
  menus = true,
})

-- Sharing an arbiter slot with other overlays over the same window, and re-checking the
-- gate on a timer because a panel can open and close with no window event at all:
-- settings:attach(on_ear, { menus = true, slot = ON_EAR_SLOT,
--                           specificity = O.layer.dialog, pollMatch = 500 })
```

## O\:attachEmbedded(spec, opts) {#o-attachembedded}

Binds the overlay as an **embedded** context: active while keyboard focus is inside a plugin control hosted in a DAW. Coordinates are relative to that control's client area, so the same regions work standalone and embedded.

`spec: { hosts: {Matcher}?, host: Matcher?, control: string, identify: ((control) -> boolean)? }` — `hosts` is the list of acceptable DAW host-window matchers (falls back to `{ spec.host }`); `control` is a Luau pattern matched against candidate child/focus-chain control class names; `identify(control)` is an optional confirmation callback (UIA / OCR / image search), cached per control HWND, used because a host's plugin control class often matches any plugin (e.g. REAPER's `Plugin<ptr>`). Candidates come from `host.window.controls()` plus the `host.window.focusChain()`.

`opts: { hoverToRead?, slot: string?, specificity: number?, pollMatch: number? }` — `hoverToRead` and the navigation / focus-reset behaviour are as in `attach`. With `slot` the overlay joins the host **arbiter** for that slot at `specificity` (a base and the overlays inheriting it pass the same slot; the most-specific *matching* one is active — see `host.arbiter`); `pollMatch` (ms) additionally re-checks the match on a recurring timer, for matches that change with no window event (a library landmark appearing inside an already-focused plugin). Returns nothing.

```luau
-- REAPER names every plug-in window "Plugin<pointer>", so the class alone matches ANY
-- plug-in: `identify` is what confirms this one is sforzando (its GUI is a UIA pane).
local daw = host.require("com.platform.daw-hosts")
ov:attachEmbedded({
  hosts = daw.all,
  control = { windows = "^Plugin%x+$" },   -- OS-keyed; see below
  identify = function(ctrl)
    return host.uia.find(ctrl.id, "PlogueXMLGUI", host.uia.type.Pane) ~= nil
  end,
}, { slot = SLOT, specificity = O.layer.base, menus = true })
```

### Windows

`control` matches a child window class, and the example above is the whole mechanism: a DAW hosts a plug-in in a child window, and that window has a class name to match on.

### macOS

There is no equivalent child control to match. A binding whose `control` table carries no entry for the running platform is **inert rather than broken** -- it logs and does nothing, and the module keeps working through whatever other bindings it has. That is why sforzando ships a separate standalone binding for the Mac rather than relying on this one.

## O\:gate(fn) / O\:landmark(image) {#o-gate}

`gate(fn)` sets an extra activation condition ANDed onto the context match:
`fn(origin)` (origin = the active context's coordinate window/control) returns
whether the overlay should be active. `landmark(image)` is the common case — a gate
satisfied only while `image` (an **absolute** path, via `host.path`) is found within
the active context's region. A derived / library overlay uses a landmark to take
over from its base only when its product wordmark is on screen. Both return the
overlay (chainable).

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
  if host.uia.locate(origin.id, "Global Settings", host.uia.type.Text) then
    settingsDark = false
    return true
  end
  settingsDark = true   -- nothing answers: the device-name field has the caret
  return true           -- absence of an answer is not an answer, so the last one stands
end)
settings:typingWhen(function() return settingsDark end)
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
[`host.include`](modules#host-include). Kontakt separates detection,
the cell matrix, the per-version geometry, what a control does, and what a cell contains.

See [Nested overlays design](../nested-overlays-design.md).
