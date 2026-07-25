---
title: "host.overlay (O) — self-voicing overlay control layer"
sidebar_position: 6
---

The overlay API is exposed as `host.overlay` (aliased `O` throughout). `O.new` returns an `Overlay` object whose methods are called with `:`. Controls are added to a virtual tree, navigated by keyboard, and spoken as `"label, type[, value]"`. All control coordinates are **origin-relative**: the origin is the client-area top-left (in screen pixels) of the active context's coordinate window — the plugin window when standalone, or the embedded plugin's child control when hosted in a DAW — re-resolved per call so it tracks window moves; `(0, 0)` when unattached.

Source: `C:/scripts/operating-system-automation-platform/crates/host/src/overlay_prelude.luau`.

Control **kinds**: `static`, `hotspot`, `custom`, `ocr`, `gtoggle`. The spoken type label is `""` (static), `"button"` (hotspot/custom/ocr), `"toggle button"` (gtoggle). **Every kind is a focus stop** — `static` text is Tab-reachable and read aloud, it just has no activation (Enter does nothing on it).

## O.new(label)

Creates a new overlay object. `label: string?` (defaults to `"Overlay"`).

Returns an `Overlay` (metatable-backed table) with fields `label`, `controls = {}`, `focus = 0`, `active = false`, `hoverToRead = false`, `contexts = {}`, `activeCtx = false`, and internal registration/key/trigger state.

```lua
local ov = O.new("My Plugin")
```

## O:addStaticText(label)

Appends a static text control: Tab-reachable and read aloud on focus, but with no activation (Enter does nothing). `label: string`.

Returns the control table `{ kind = "static", label = label }`.

```lua
ov:addStaticText("Mixer section")
```

## O:addHotspotButton(opts)

Appends a button that, when activated, clicks a fixed origin-relative point. `opts: { label: string, at: {number, number}, hotkey: string?, rawOrigin: boolean?, fromRight: boolean? }` — `at` is `{x, y}` relative to the origin; `hotkey` is an optional global activation hotkey spec (e.g. `"Alt+P"`).

Returns `{ kind = "hotspot", label, at, hotkey, rawOrigin, fromRight }`. On activate it clicks `(origin.x + at[1], origin.y + at[2])` and speaks `"label, activated"`.

`fromRight` measures `at[1]` from the coordinate window's **right edge** instead of its left — the click lands at `origin.x + width − at[1]` (ReaHotkey's `ControlX + ControlWidth − N`). Use it for plugin UI laid out from the right, so the target stays correct whatever the plugin's width is. Also accepted by [`addHotspotToggle`](#oaddhotspottoggleopts).

```lua
ov:addHotspotButton({ label = "Play", at = { 120, 40 }, hotkey = "Alt+P" })
-- 352 px in from the right edge, 87 px down — Kontakt's instrument arrows:
ov:addHotspotButton({ label = "Previous instrument", at = { 352, 87 }, fromRight = true, rawOrigin = true })
```

## O:addCustomButton(opts)

Appends a button that runs a Luau callback on activation. `opts: { label: string, onActivate: (overlay) -> (), hotkey: string? }` — `onActivate` receives the overlay itself (so a header control can reach the active context).

Returns `{ kind = "custom", label, onActivate, hotkey }`.

```lua
ov:addCustomButton({
  label = "Next library",
  onActivate = function(self) --[[ ... ]] end,
})
```

## O:addOCRButton(opts)

Appends a button whose label/value is read live by OCR over a region; activating re-reads it then clicks the region centre. `opts: { label: string, region: {number, number, number, number}, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative.

Returns `{ kind = "ocr", label, region, hotkey }`. When focused/spoken it appends the OCR text (or `"no text"`) as the value.

```lua
ov:addOCRButton({ label = "Patch", region = { 200, 12, 360, 32 } })
```

## O:addGraphicalToggle(opts)

Appends a toggle whose on/off state is read by image-matching its region against an "on" and "off" template; activating clicks the region centre and re-reads the new state after ~150 ms. `opts: { label: string, region: {number, number, number, number}, onImage: string?, offImage: string?, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative; `onImage`/`offImage` are template image paths.

Returns `{ kind = "gtoggle", label, region, onImage, offImage, hotkey }`. Spoken state is `"on"` / `"off"` (omitted when neither template matches).

```lua
ov:addGraphicalToggle({
  label = "Mix",
  region = { 50, 50, 80, 70 },
  onImage = "mix_on.png",
  offImage = "mix_off.png",
})
```

## O:addHotspotToggle(opts)

Appends a toggle whose on/off state is read from a **single pixel** at its click point (ReaHotkey's `HotspotToggleButton`) — cheap (one screen touch), where a region scan would cost ~16 ms *per pixel*. The pixel is compared to an on/off reference colour and the **nearest** wins. Activating clicks the point (toggling it) then re-reads the state after ~150 ms. `opts: { label: string, at: {number, number}, onColor: {number, number, number}, offColor: {number, number, number}, hotkey: string?, rawOrigin: boolean? }` — `at` is `{x, y}` origin-relative; `onColor`/`offColor` are `{r, g, b}`.

Returns `{ kind = "hotspottoggle", label, at, onColor, offColor, hotkey, rawOrigin }`. Spoken state is `"on"` / `"off"` (omitted when the pixel can't be read). Prefer this over `addGraphicalToggle` when the control has a distinct lit/unlit colour (an indicator LED, a lit ⏻ icon) — it needs no template images and is a fraction of the cost.

```lua
ov:addHotspotToggle({
  label = "Spot 1 Mic",
  at = { -138, 366 },
  onColor = { 180, 165, 230 }, -- lit (accent)
  offColor = { 78, 79, 85 },   -- unlit (dim grey)
})
```

## O:focusNext()

Moves focus to the next control (wrapping) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. No-op when there are no controls. Returns nothing.

## O:focusPrev()

Moves focus to the previous control (wrapping) and speaks it. No-op when empty. Returns nothing.

## O:activate(index)

Activates the control at `index` (defaults to the focused control). `index: number?`. Behaviour by kind: `hotspot` clicks `at` and speaks `"label, activated"`; `custom` calls `onActivate(self)`; `ocr` re-reads then clicks the region centre; `gtoggle` clicks the region centre and re-reads state after ~150 ms. No-op for `static` or a missing control. Returns nothing.

```lua
ov:activate()      -- activate focused control
ov:activate(3)     -- activate the 3rd control
```

## O:setControls(controls)

Replaces the entire control set at runtime: unregisters/re-registers hotkeys if active and resets focus to the first focusable control. `controls: {Control}` (array of control tables — the low-level form the `addX` builders produce; prefer the builders). Returns nothing.

## O:attach(matcher, opts)

Binds the overlay as a **standalone** context: active while a window matching `matcher` is the foreground/active window, with coordinates relative to that window's client area. `matcher` is a window matcher passed to `host.window.test`; `opts: { hoverToRead: boolean? }?`.

While active, the overlay captures and suppresses the navigation keys, scoped to its own window (so `Alt+Tab` and menus pass through natively): `Tab` / `Shift+Tab` move between controls, `Return` and `Space` activate the focused control, and — when the overlay has a tab control — `Left`/`Right`, `Ctrl+Tab`/`Ctrl+Shift+Tab` and `Ctrl+<n>` drive it. `Space` is released while an editable field (an `ocredit` control) is focused, so a literal space can be typed into it. On activation the overlay starts at its first control (and any tab control at its first tab) when a **genuinely new** window opened, but resumes the last-focused control when the *same* still-open window merely regained the foreground (`Alt+Tab` out and back); the two are told apart by the window's identity (its HWND). `hoverToRead` (default `false`) moves the mouse onto an OCR control on focus (some UIs only reveal values on hover). Registers the foreground/focus trigger once. Returns nothing.

```lua
ov:attach({ title = "MySynth" })
```

## O:attachEmbedded(spec, opts)

Binds the overlay as an **embedded** context: active while keyboard focus is inside a plugin control hosted in a DAW. Coordinates are relative to that control's client area, so the same regions work standalone and embedded.

`spec: { hosts: {Matcher}?, host: Matcher?, control: string, identify: ((control) -> boolean)? }` — `hosts` is the list of acceptable DAW host-window matchers (falls back to `{ spec.host }`); `control` is a Luau pattern matched against candidate child/focus-chain control class names; `identify(control)` is an optional confirmation callback (UIA / OCR / image search), cached per control HWND, used because a host's plugin control class often matches any plugin (e.g. REAPER's `Plugin<ptr>`). Candidates come from `host.window.controls()` plus the `host.window.focusChain()`.

`opts: { hoverToRead?, slot: string?, specificity: number?, pollMatch: number? }` — `hoverToRead` and the navigation / focus-reset behaviour are as in `attach`. With `slot` the overlay joins the host **arbiter** for that slot at `specificity` (a base and the overlays inheriting it pass the same slot; the most-specific *matching* one is active — see `host.arbiter`); `pollMatch` (ms) additionally re-checks the match on a recurring timer, for matches that change with no window event (a library landmark appearing inside an already-focused plugin). Returns nothing.

```lua
ov:attachEmbedded({
  hosts = { { title = "REAPER" }, { title = "Cubase" } },
  control = "Plugin",
  identify = function(c) return host.uia.find(c.id, "MySynthGUI", 0) end,
})
```

## O:gate(fn) / O:landmark(image)

`gate(fn)` sets an extra activation condition ANDed onto the context match:
`fn(origin)` (origin = the active context's coordinate window/control) returns
whether the overlay should be active. `landmark(image)` is the common case — a gate
satisfied only while `image` (an **absolute** path, via `host.path`) is found within
the active context's region. A derived / library overlay uses a landmark to take
over from its base only when its product wordmark is on screen. Both return the
overlay (chainable).

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

```lua
ov:landmark(host.path("images/MyLib/wordmark.png"))
```

## O:derive(name, build)

The **composition** primitive. Derives a child overlay from this one: the child
inherits this overlay's controls (a fresh copy), coordinate frame, interaction
flags, and activation context, and competes on the **same arbiter slot one
specificity higher** (so the child wins over its parent when both match).
`build(child)` adds the child's own controls (with the functional builders) and a
more-specific gate (`child:landmark(...)`). Recursive: a sub-library derives a
library, each level a full overlay the arbiter assembles — React-portal style. The
parent must already be attached (so its context + slot exist to inherit). Returns
the child overlay.

```lua
local child = base:derive("Sub-library", function(c)
  c:landmark(host.path("sub-wordmark.png"))
  c:addHotspotButton({ label = "Extra control", at = { 40, 200 } })
end)
```

## O:addSlot(name) / O:fill(name, build)

Named **slots** let a base overlay reserve a position for child controls instead of
forcing them to append at the end. `addSlot(name)` adds an invisible placeholder
(skipped in navigation — an unfilled slot is 0 focus stops); a derived / library
overlay calls `fill(name, build)`, and `build(slot)` adds controls with the usual
builders — they land **at the slot's position**, so a base of `header + slot +
footer` becomes `header + the child's controls + footer`. `fill` is a no-op if no
such slot exists (e.g. already filled). Both return the overlay (chainable).

```lua
-- base:
ov:addStaticText("Header")
ov:addSlot("body")
ov:addStaticText("Footer")
-- a derived overlay fills the slot — its controls go between Header and Footer:
child:fill("body", function(s)
  s:addHotspotButton({ label = "Item", at = { 40, 200 } })
end)
```

## O:replace(label, build)

Overrides an inherited control: replaces the first control labelled `label` with the
(first) control `build` adds, leaving the rest of the tree intact — use it in a
derived overlay to change one inherited control without rebuilding the others. No-op
if no control matches. Returns the overlay (chainable).

```lua
child:replace("Load instrument", function(o)
  o:addCustomButton({ label = "Load instrument", onActivate = function(ov) --[[ custom ]] end })
end)
```

## Plugin base + library overlays (inheritance model)

A plugin like Kontakt is a **base overlay** (its generic header) plus separate
**library overlays** that inherit it and take over when their landmark is on
screen — they compete on one arbiter `slot` by `specificity` (see `host.arbiter`).
Everything is built with the **same functional builders** used above; there is no
declarative control table.

The Kontakt framework lives in the `com.platform.kontakt` module. A library module
depends on it and calls `kontakt.library(name, landmark, build)`, which returns an
overlay that already carries Kontakt's inherited header, embedding, and version
offset plus the `landmark` gate; `build(ov)` adds the library's own controls
functionally:

```lua
local kontakt = host.require("com.platform.kontakt")
kontakt.library("Cinematic Studio Strings", host.path("images/CSS/Product.png"), function(ov)
  ov:addStaticText("Cinematic Studio Strings")
  ov:addHotspotButton({ label = "Spot 1 Mic", at = { 35, 481 } })
  ov:addGraphicalToggle({
    label = "Mix Position", region = { 188, 473, 220, 489 },
    onImage = host.path("images/CSS/MixOn.png"), offImage = host.path("images/CSS/MixOff.png"),
  })
end)
```

Under the hood the base + each library are `O.new` + the functional builders +
`attachEmbedded` (with `slot` / `specificity` / `pollMatch`) + `ov:landmark(...)`; a
further sub-library uses `O:derive` (above). See
[Nested overlays design](../nested-overlays-design.md). (The earlier single
`O.container` + `host.providers` data model has been removed.)

