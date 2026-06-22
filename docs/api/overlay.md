---
title: "host.overlay (O) — self-voicing overlay control layer"
sidebar_position: 6
---

The overlay API is exposed as `host.overlay` (aliased `O` throughout). `O.new` returns an `Overlay` object whose methods are called with `:`. Controls are added to a virtual tree, navigated by keyboard, and spoken as `"label, type[, value]"`. All control coordinates are **origin-relative**: the origin is the client-area top-left (in screen pixels) of the active context's coordinate window — the plugin window when standalone, or the embedded plugin's child control when hosted in a DAW — re-resolved per call so it tracks window moves; `(0, 0)` when unattached.

Source: `C:/scripts/operating-system-automation-platform/crates/host/src/overlay_prelude.luau`.

Control **kinds**: `static`, `hotspot`, `custom`, `ocr`, `gtoggle`. The spoken type label is `""` (static), `"button"` (hotspot/custom/ocr), `"toggle button"` (gtoggle). **Every kind is a focus stop** — `static` text is Tab-reachable and read aloud, it just has no activation (Enter does nothing on it).

## O.new(label)

Creates a new overlay object. `label: string?` (defaults to `"Overlay"`).

Returns an `Overlay` (metatable-backed table) with fields `label`, `controls = {}`, `focus = 0`, `active = false`, `naturalNav = false`, `hoverToRead = false`, `contexts = {}`, `activeCtx = false`, and internal registration/key/trigger state.

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

Appends a button that, when activated, clicks a fixed origin-relative point. `opts: { label: string, at: {number, number}, hotkey: string? }` — `at` is `{x, y}` relative to the origin; `hotkey` is an optional global activation hotkey spec (e.g. `"Alt+P"`).

Returns `{ kind = "hotspot", label, at, hotkey }`. On activate it clicks `(origin.x + at[1], origin.y + at[2])` and speaks `"label, activated"`.

```lua
ov:addHotspotButton({ label = "Play", at = { 120, 40 }, hotkey = "Alt+P" })
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

Replaces the entire control set at runtime: unregisters/re-registers hotkeys if active and resets focus to the first focusable control. `controls: {Control}` (array of control tables). Used to swap a Kontakt base overlay for a library's header + controls. Returns nothing.

## O:show()

Always-on mode: immediately registers hotkeys and announces readiness over speech (`"<label> overlay ready. N controls. ..."`). Use this for an unattached overlay that should be live without a window trigger. Returns nothing.

```lua
ov:show()
```

## O:attach(matcher, opts)

Binds the overlay as a **standalone** context: active while a window matching `matcher` is the foreground/active window, with coordinates relative to that window's client area. `matcher` is a window matcher passed to `host.window.test`; `opts: { naturalNav: boolean?, hoverToRead: boolean? }?`.

`naturalNav` (default `false`) captures and suppresses `Tab` / `Shift+Tab` / `Return` for native-style navigation while the overlay window is focused; otherwise navigation uses the fallback hotkeys `Ctrl+Alt+Right` / `Ctrl+Alt+Left` / `Ctrl+Alt+Return`. `hoverToRead` (default `false`) moves the mouse onto an OCR control on focus (some UIs only reveal values on hover). Registers the foreground/focus trigger once. Returns nothing.

```lua
ov:attach({ title = "MySynth" }, { naturalNav = true })
```

## O:attachEmbedded(spec, opts)

Binds the overlay as an **embedded** context: active while keyboard focus is inside a plugin control hosted in a DAW. Coordinates are relative to that control's client area, so the same regions work standalone and embedded.

`spec: { hosts: {Matcher}?, host: Matcher?, control: string, identify: ((control) -> boolean)? }` — `hosts` is the list of acceptable DAW host-window matchers (falls back to `{ spec.host }`); `control` is a Luau pattern matched against candidate child/focus-chain control class names; `identify(control)` is an optional confirmation callback (UIA / OCR / image search), cached per control HWND, used because a host's plugin control class often matches any plugin (e.g. REAPER's `Plugin<ptr>`). Candidates come from `host.window.controls()` plus the `host.window.focusChain()`.

`opts` is the same `{ naturalNav?, hoverToRead? }` as `attach`. Returns nothing.

```lua
ov:attachEmbedded({
  hosts = { { title = "REAPER" }, { title = "Cubase" } },
  control = "Plugin",
  identify = function(c) return host.uia.find(c.id, "MySynthGUI", 0) end,
}, { naturalNav = true })
```

## Plugin base + library overlays (inheritance model)

A plugin like Kontakt is a **base overlay** (its generic header) plus separate
**library overlays** that inherit it and take over when their landmark is on
screen — they compete on one arbiter `slot` by `specificity` (see `host.arbiter`).
The base and each library are built with `O.new`, `attachEmbedded` (passing
`slot`, `specificity`, a fresh `present` landmark gate, and `pollMatch`), and
`setControls`. The Kontakt framework lives in the `com.platform.kontakt` module;
a library module depends on it and calls `kontakt.library({ name, image,
controls })`. See [Nested overlays design](../nested-overlays-design.md). (The
earlier single `O.container` + `host.providers` data model has been removed.)

