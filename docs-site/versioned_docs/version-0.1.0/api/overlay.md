---
title: "host.overlay (O) — self-voicing overlay control layer"
sidebar_position: 6
---

The overlay API is exposed as `host.overlay` (aliased `O` throughout). `O.new` returns an `Overlay` object whose methods are called with `:`. Controls are added to a virtual tree, navigated by keyboard, and spoken as `"label, type[, value]"`. All control coordinates are **origin-relative**: the origin is the client-area top-left (in screen pixels) of the active context's coordinate window — the plugin window when standalone, or the embedded plugin's child control when hosted in a DAW — re-resolved per call so it tracks window moves; `(0, 0)` when unattached.

Source: `C:/scripts/operating-system-automation-platform/crates/host/src/overlay_prelude.luau`.

Control **kinds**: `static`, `hotspot`, `custom`, `ocr`, `gtoggle`. The spoken type label is `""` (static), `"button"` (hotspot/custom/ocr), `"toggle button"` (gtoggle). `static` controls are skipped during navigation.

## O.new(label)

Creates a new overlay object. `label: string?` (defaults to `"Overlay"`).

Returns an `Overlay` (metatable-backed table) with fields `label`, `controls = {}`, `focus = 0`, `active = false`, `naturalNav = false`, `hoverToRead = false`, `contexts = {}`, `activeCtx = false`, and internal registration/key/trigger state.

```lua
local ov = O.new("My Plugin")
```

## O:addStaticText(label)

Appends a non-focusable static text control (skipped when navigating). `label: string`.

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

Moves focus to the next focusable control (wrapping, skipping `static`) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. No-op when there are no controls. Returns nothing.

## O:focusPrev()

Moves focus to the previous focusable control (wrapping, skipping `static`) and speaks it. No-op when empty. Returns nothing.

## O:activate(index)

Activates the control at `index` (defaults to the focused control). `index: number?`. Behaviour by kind: `hotspot` clicks `at` and speaks `"label, activated"`; `custom` calls `onActivate(self)`; `ocr` re-reads then clicks the region centre; `gtoggle` clicks the region centre and re-reads state after ~150 ms. No-op for `static` or a missing control. Returns nothing.

```lua
ov:activate()      -- activate focused control
ov:activate(3)     -- activate the 3rd control
```

## O:setControls(controls)

Replaces the entire control set at runtime: unregisters/re-registers hotkeys if active and resets focus to the first focusable control. `controls: {Control}` (array of control tables). Used by container overlays when the detected library changes. Returns nothing.

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

## O.container(spec)

Creates a **container overlay** bound to an embedded plugin that hosts swappable sub-overlays (e.g. Kontakt hosting a sample library). It attaches to the plugin via `attachEmbedded`, then watches for libraries registered under a `contract` (through `host.providers`) and mounts the matching one's controls after its own generic `header`. Library overlays are plain **data** (no Lua crosses module VMs); the container interprets them.

`spec` fields:
- `label: string?` — overlay label (default `"Plugin"`).
- `hosts` / `host` / `control` / `identify` — passed through to `attachEmbedded` (the host matchers, the control-class pattern, and the per-control identity check).
- `variants: {{ name: string, offset: {number, number}, identify: ((control) -> boolean)? }}?` — per-version parameters; the first variant whose `identify` passes (or has none) supplies the coordinate `offset` applied to library coordinates, so libraries auto-adapt without knowing the plugin version. Cached per control id. Defaults to a single variant built from `spec.label` / `spec.offset` / `spec.identify`.
- `offset: {number, number}?` — default coordinate shift when no `variants` given (default `{0, 0}`).
- `contract: string` — the provider contract key under which library descriptors are registered.
- `header: {Control}?` — the generic, always-present control set mounted before any library's controls (default `{}`).
- `naturalNav: boolean?`, `hoverToRead: boolean?` — forwarded to the embedded attach.
- `pollMs: number?` — landmark-detection poll interval in ms (default `500`).

Returns the configured `Overlay` (container). On creation it starts a self-rescheduling **library-detection poll** (image-matching each descriptor's `image` landmark inside the plugin rect; sticky on the current library) and a **menu-watch** (UIA control type `50009` = Menu) that, under `naturalNav`, tells the key hook to let captured nav keys pass through to a plugin's own (non-Win32) menu.

### Library descriptor data format

A provider registered under the container's `contract` returns plain data of the shape:

```lua
{
  provides = "kontakt-library",       -- the contract string
  libraries = {
    {
      name = "Session Strings",
      vendor = "Native Instruments",
      image = "session_strings_landmark.png",  -- landmark for detection
      controls = {
        -- coordinates are origin-relative (base coords; the container
        -- shifts them by the active variant's offset before mounting)
        { kind = "static",  label = "Articulation" },
        { kind = "hotspot", label = "Legato", at = { 100, 200 } },
        { kind = "custom",  label = "Reset", hotkey = "Alt+R" },
        { kind = "ocr",     label = "Dynamics", region = { 40, 60, 180, 84 } },
        { kind = "gtoggle", label = "Reverb", region = { 300, 60, 330, 90 },
          onImage = "rev_on.png", offImage = "rev_off.png" },
      },
    },
  },
}
```

Each library's `controls[].kind` is one of `static`, `hotspot`, `custom`, `ocr`, `gtoggle` (same shapes as the `O:add*` builders), with coordinates relative to the plugin origin. The container flattens `libraries` from all providers under the contract, detects the loaded one by image-matching its `image` landmark in the plugin rect, then mounts `header` followed by the library's offset-shifted controls and announces `"<label>, <library name>"` (or `"<label>, no library"` when none matches).

```lua
local ov = O.container({
  label = "Kontakt",
  hosts = { { title = "REAPER" } },
  control = "Plugin",
  contract = "kontakt-library",
  header = { { kind = "custom", label = "Browse", onActivate = browse } },
  variants = {
    { name = "Kontakt 8", offset = { 0, 0 },  identify = isK8 },
    { name = "Kontakt 7", offset = { 0, -24 }, identify = isK7 },
  },
  naturalNav = true,
})
```

