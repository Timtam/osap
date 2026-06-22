# Host API Capability Catalog (Draft)

*Status: Draft, 2026-06-21. Single source of truth for the module API. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md) and [module-package-format.md](module-package-format.md).*

This catalog is the **single machine-readable source** from which the following are generated: (a) the Luau type definitions for module authors, (b) the `[capabilities]` enum in the manifest, (c) the versioned web documentation (see [TODO](../TODO.md)). It is versioned under `engine_api` (additive change = minor, breaking = new major; study §6).

## 1. Model

**Design principle — primitives are first-class and overlay-independent.** Every capability (`host.ocr`, `host.screen`, `host.input`, `host.window`, `host.hotkey`, `host.speech` …) is **directly scriptable from Luau, without using `host.overlay`.** `host.overlay` is *one* optional high-level layer (for accessible, self-voicing overlays in the style of ReaHotkey) that builds on the same primitives — not a mandatory funnel. General automation (the AHK / Keyboard Maestro class) uses the primitives directly; the overlays are merely the first, not the only, use case.

- **Namespaced:** every capability is `host.<namespace>.<function>`.
- **Default-deny + manifest gating:** a module may only use a namespace if it is listed in `module.toml` under `[capabilities].require` and the user has granted it (study §7).
- **Risk tiers:**
  - **low** — only affects the module's own package / its own output → granted by default.
  - **medium** — reads/controls foreign windows, screen, input → one-time user grant per module; on macOS bound to TCC permissions.
  - **high** — arbitrary code execution (native FFI) → signature-required + out-of-process sandbox.
- **Capability detection:** every backend reports `host.<ns>.available()` per machine (e.g. global hotkeys under Wayland = no). Modules query instead of assuming.
- **Platform backends** sit behind traits (Win/macOS first); the Luau API is platform-identical.

## 2. Overview Matrix

Tier = risk level · MVP = required for the ReaHotkey vertical slice · Feasibility = Win / macOS.

| Namespace | Purpose | Tier | MVP | Win | macOS | macOS permission |
|---|---|---|---|---|---|---|
| `host.overlay` | Self-voicing virtual accessible-control runtime (ReaHotkey framework) | low¹ | ✅ | full | full | — (actions inherit permissions of the backends they use) |
| `host.speech` | Screen-reader / TTS output (tts-rs) | low | ✅ | full | full | — |
| `host.sound` | Audio asset playback | low | ✅ | full | full | — |
| `host.resource` | Read/resolve package resources, module config | low | ✅ | full | full | — |
| `host.hotkey` | Contextual + global hotkeys | medium | ✅ | full | limited | Input Monitoring + Accessibility |
| `host.window` | Window / control introspection | medium | ✅ | full | limited | Accessibility |
| `host.input` | Mouse / keyboard simulation | medium | ✅ | full | limited | Accessibility |
| `host.screen` | Capture, image search, pixel/color | medium | ✅ | full | limited | Screen Recording |
| `host.ocr` | Text recognition in a region/image | medium | ✅ | full | limited | (uses `screen`) |
| `host.a11y` | Accessibility elements of foreign apps (UIA/AX) | medium | ○ | full | limited | Accessibility |
| `host.gui` | Own accessible windows (wxDragon) | medium | ○ | full | full | — |
| `host.clipboard` | Read/write clipboard | medium | ○ | full | full | — |
| `host.app` | Detect/focus/launch running apps | medium | ○ | full | limited | (Automation/AppleEvents partly) |
| `host.ffi` | Load/call native libs | **high** | ○ | full | limited | (signature, no TCC) |
| `host.log` / `host.timer` | Logging, scheduling | low | ✅ | full | full | — |

¹ `host.overlay` itself only speaks + reacts to its own hotkeys (low); its *control actions* (click, image search, OCR, AX reading) use `input`/`screen`/`ocr`/`a11y` and inherit their tier.

## 3. ReaHotkey Backends → Capabilities

The 6 interaction backends of the overlays map exactly onto capability combinations:

| ReaHotkey backend | Required capabilities |
|---|---|
| **Custom** (callbacks) | `overlay` + arbitrary depending on the callback (`input`, `app`, …) |
| **Hotspot** (coordinate click) | `overlay` + `input` (+ `window` for the coordinate reference) |
| **Graphical** (state via image) | `overlay` + `screen` (image search) + `input` |
| **OCR** (read text) | `overlay` + `ocr` + `screen` |
| **Native** (Win32 control) | `overlay` + `window`/`a11y` |
| **UIA** (accessibility element) | `overlay` + `a11y` |
| *Output (all)* | `speech` + `sound` |
| *Detection (all)* | `window` (title/class/ListView) + `screen` (image fallback) |

## 4. Namespaces in Detail (Luau API Sketch)

> Signatures are a draft. `?` = optional. Paths are always **package-relative** (the host resolves them).

### host.overlay — the overlay runtime (optional high-level layer)
*One* optional high-level layer for accessible, self-voicing overlays (the ReaHotkey class) — builds on `input`/`screen`/`ocr`/`a11y`/`hotkey`. NOT required for general automation; the primitives are directly scriptable (design principle §1). Builds a tree of virtual, self-voicing controls on top of a foreign window.
```lua
local ov = host.overlay.new("Serum 2")           -- OverlayBuilder
ov:addStaticText("Serum 2")
ov:addHotspotButton{ label="Next Preset", at={968,4}, hotkey="Alt+N" }
ov:addCustomButton{ label="Main Menu", hotkey="Ctrl+M",
                    onActivate=function() host.input.click(1058,4) end }
ov:addOCRButton{ label="Preset", region={540,13,608,23}, engine="best", hotkey="Alt+M" }
ov:addGraphicalToggleButton{ label="Classic Mix", image="assets/images/serum2/preset.png" }
ov:addTabControl{ ... }  ;  ov:addUIAControl{ path=..., role="button" }
return ov
-- Navigation (Tab/arrows), focus announcement, activation, and coordinate compensation
-- relative to the detected plugin control are handled by the runtime.
```
Control types (from ReaHotkey's taxonomy): StaticText, Button/ToggleButton/Checkbox/Edit/ListBox, Tab/TabControl, plus the backend variants Hotspot*, Graphical*(+Slider H/V), Custom*, Native*, UIA*, OCRButton, PassThrough.

### host.speech — speech output (tts-rs)
```lua
host.speech.output("Preset: Init", { interrupt = true })
host.speech.stop()
host.speech.setRate(n) ; host.speech.setVoice(id) ; host.speech.voices()
```
Win: Tolk→NVDA/JAWS (+ Braille) or WinRT · macOS: AVFoundation (direct TTS). **macOS option (prior art [VOCR](prior-art-vocr.md)):** while VoiceOver is running, route output to VoiceOver via AppleScript (respects the VO voice + **Braille**; requires the `apple-events` entitlement + user opt-in), otherwise AVFoundation fallback — tiered behind `host.speech`.

### host.sound — audio feedback
```lua
local h = host.sound.play("assets/sounds/focus.ogg", { volume = 0.8 })
h:stop()
```

### host.resource — package resources
```lua
local bytes = host.resource.read("assets/data/profiles.json")  -- bytes/string from the package
local p     = host.path("assets/images/serum2/preset.png")     -- real path (escape hatch)
```

### host.settings — per-module settings (persisted)
```lua
-- declare + read in one line; the type is pinned from the default. opts (optional):
-- { label = "…", min = N, max = N, oneOf = { … } }. A persisted value wins over the default.
local rate = host.settings.define("speechRate", 50, { label = "Speech rate", min = 0, max = 100 })
local lang = host.settings.define("ocrLanguage", "eng", { label = "OCR language", oneOf = { "eng", "deu" } })
host.settings.set("imageSearch", true)        -- validated against the schema; auto-persisted
local on = host.settings.get("imageSearch")    -- errors if the key was never define()d
host.settings.onChange("speechRate", function(new, old) end)
-- host.config is an alias of host.settings (catalog-compat).
```
Each module sees only its own settings (keyed by module id; isolation is structural). Scalars only (boolean / number / string). Persisted in a portable `<exe_dir>/settings.toml` next to the executable — one record per module (a host-owned `enabled` flag + the `settings` map); supersedes the old `disabled-modules.txt` (auto-migrated). Auto-saved (coalesced to the event loop + on shutdown, atomic write). The tray manager renders a native, accessible settings form per module from the registered schema.

### host.hotkey — hotkeys with context
```lua
host.hotkey.register("Ctrl+Alt+P", fn, { context = "overlay:Serum 2" })  -- contextual
host.hotkey.register("F1", fn, { context = "global" })
```
macOS: **registered** hotkeys via Carbon `RegisterEventHotKey` (no Input Monitoring, no silent disable — prior art [VOCR](prior-art-vocr.md); context switch via re-register per scope). CGEventTap only for suppression/remapping/hotstrings (then Input Monitoring + health watchdog). Wayland: usually not available → check `available()`.

### host.window — find & read windows/controls
Window matching is **platform-gated** (parameters are named differently per OS — there is no universal "class"). Details + trigger system: [window-matching.md](window-matching.md).
```lua
local w = host.window.active()        -- { id, title, app={name,bundleId,exe,pid}, bounds }
local w = host.window.find{           -- declarative, OS-gated matcher
  title   = { regex = "^Serum 2" },
  windows = { class = { regex = "^VSTGUI%x+$" } },
  macos   = { axSubrole = "AXFloatingWindow", bundleId = "com.cockos.reaper" },
  where   = function(win) return ... end,        -- escape hatch (image/OCR/AX/ListView)
}
host.window.findAll(matcher) ; host.window.list()
host.window.onTrigger(matcher, { on = "activate" }, fn)   -- "trigger-like" auto-activation
local c = w:focusedControl() ; w:listViewContent("SysListView321")
host.os.current  -- "windows"|"macos"|"linux" for imperative OS gating
```
Win: Win32/UIA + `SetWinEventHook` · macOS: AXUIElement/CGWindowList + `NSWorkspace`/`AXObserver` (Accessibility permission).

### host.input — mouse/keyboard
```lua
host.input.click(x, y, { button="left", relativeTo="pluginControl" })
host.input.move(x, y) ; host.input.drag(x1,y1, x2,y2) ; host.input.scroll(x,y, dy)
host.input.send("^s") ; host.input.text("Hallo")
```
`drag` for GraphicalSlider (ReaHotkey `MouseClickDrag`), `scroll` for mouse-wheel-based plugins (Zampler). Win: SendInput · macOS: CGEvent (`…MouseEvent`/`…ScrollWheelEvent`, Accessibility permission).

### host.screen — capture / image search / pixel
```lua
local m = host.screen.imageSearch("assets/images/serum2/preset.png", {region={x1,y1,x2,y2}})
if m then host.input.click(m.x, m.y) end
local col = host.screen.pixel(x, y)
local img = host.screen.capture({x1,y1,x2,y2})
```
Win: Windows.Graphics.Capture + SIMD-NCC · macOS: ScreenCaptureKit (Screen Recording permission).

### host.ocr — text recognition
```lua
local r = host.ocr.recognize({ region={540,13,608,23}, engine="best", lang="eng" })
-- r = { text="Init", boxes={...} }
```
Native by default (Windows.Media.Ocr / Apple Vision), ONNX fallback (study §2). Replaces ReaHotkey's Tesseract-exe invocation.

### host.a11y — accessibility elements of foreign apps
```lua
local el = host.a11y.focused()        -- or host.a11y.fromWindow(w):path(1,3,2)
el:role() ; el:name() ; el:value() ; el:focus() ; el:invoke()
```
Win: IUIAutomation · macOS: AXUIElement. Replaces ReaHotkey's `UIA.ahk` passthrough.

### host.gui — own accessible windows (wxDragon)
```lua
local win = host.gui.window{ title="Einstellungen" }
win:button{ label="Speichern", accessibleLabel="Profil speichern", onClick=fn } -- label mandatory
```
Accessibility enforced via the API (mandatory label, study §5).

### host.ffi — native libs (gated, high)
```lua
local lib = host.ffi.load("foo")           -- from native/<platform>/, signature-required
lib:call("bar", { "Int", 42 }, "Int")
```
Out-of-process sandbox for untrusted modules (study §11.1). macOS: only own Team-ID-signed or in the helper.

### host.app / host.clipboard / host.log / host.timer
```lua
host.app.running() ; host.app.focus(pid) ; host.app.launch(path)
host.clipboard.read() ; host.clipboard.write(text)
host.log.info(msg)  -- from the worker via IPC to the host
host.timer.after(ms, fn)   -- synchronous model, scheduled host-side
```

## 5. Versioning & Generation

- The catalog is maintained as a **schema** (e.g. a Rust definition with an annotation per capability: tier, `since` version, platform feasibility).
- From it, the build generates: Luau types (`.d.luau`), the manifest capability enum, the adapter shims for older `engine_api` versions, and the versioned web documentation.
- Additive functions/namespaces = minor; removal/signature change = new major with shim overlap (study §6).

## 6. MVP Cut (ReaHotkey Vertical Slice)

The minimum required to run a Hotspot/OCR overlay (e.g. Serum2) end-to-end on Win **and** macOS:

**`overlay` + `speech` + `sound` + `resource` + `hotkey` + `window` + `input` + `screen` (image search) + `ocr`.**

Later: `a11y` (for Native/UIA overlays), `gui`, `ffi`, `clipboard`, `app`.

## 7. Reconciliation with the ReaHotkey Port Analysis (2026-06-21)

The deep-dive analysis ([reahotkey-port-analysis.md](reahotkey-port-analysis.md)) **confirmed** the MVP cut against the real source code and produced three refinements:

- **`host.input` extended with `drag` + `scroll`** (incorporated above) — GraphicalSlider uses `MouseClickDrag`, Zampler the mouse wheel.
- **`host.a11y` moved up:** still not needed in the MVP (4 of the 6 backends — Custom/Hotspot/Graphical/OCR — manage without it), but as the **next** capability right after the vertical slice, not at the end. Reason: macOS audio plugins sometimes deliver real AX values; where present, AX is more robust/faster/lower-permission than OCR (no Retina problem).
- **Detection order is platform-specific:** under macOS, AX is often empty for audio plugins (JUCE/u-he → empty `AXGroup`), which makes Vision OCR and template matching relatively more important than under Windows. The detection strategy stays portable, its prioritization does not.

Confirmed: the `host.speech` model switch on macOS (tts-rs/AVFoundation, study §11.7) and `host.sound` deliberately in the core (instead of per-plugin as in ReaHotkey).
