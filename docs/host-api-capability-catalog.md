# Host API Capability Catalog (Draft)

*Status: design, 2026-06-21. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md) and [module-package-format.md](module-package-format.md).*

:::caution This is the plan, not the platform

**For what exists today, read the [API reference](api/) — it is written from the code.** This
catalogue is where the shape was decided at the start of the project, and much of it is still
ahead of the implementation. The plan stands; what follows describes where it is going.

- **Default-deny and manifest gating is built** (2026-08-31), and scoped per module rather
  than per VM: a namespace a module has not declared is absent from its `host` table, and
  reaching for it raises an error naming the module and the capability. Permission follows the
  module that *wrote* the code, so a code dependency is judged by its own manifest wherever it
  runs, while ownership stays with the VM. Names are still not validated against a known set —
  a manifest may declare `"telepathy"` and load — and a handful of namespaces are ungated
  because gating them would mean every manifest names them: `os`, `require`, `tryRequire`,
  `include`, `epoch`, `now`, `inputEpoch`, `calibrating`.
- **`host.<ns>.available()` is not built.** No namespace has one.

Four namespaces are planned rather than present: `host.gui`, `host.clipboard`, `host.app`
and `host.ffi`. The **Built** column below says which is which.

Two rows that used to be here have been removed, because both named something that does not
exist under that name. The accessibility namespace shipped as **`host.element`** — see the
[reference](api/element) — and **the overlay is not a host namespace at all** but a module,
`com.platform.overlay`, declared under `dependencies`. Its own entry in the reference is
[Overlay](api/overlay).

`host.element` is a placeholder rather than a settled name: UIA is what Windows calls its tree and
macOS calls the same thing something else, so the name states one platform's vendor term for
an API that abstracts over both.

:::

## 0. What the gate actually stops

Asked directly, after the Luau prelude was changed to hold the whole host table as an upvalue:
*if privileged Luau runs inside a module's VM holding the ungated table, can a module get at
it?* The honest answer needed measuring rather than reasoning, so a probe module was written
that declares `log` and nothing else and then tries seven ways to reach `speech` and `screen`.
All seven were refused (2026-09-03):

| Route | Outcome |
| --- | --- |
| `host.speech` directly | refused, naming the module and the capability |
| `_G.host.screen` | refused — the global *is* the gated view, not a second table |
| a scan of every global for a table with a `speech` key | nothing found |
| `debug.getupvalue` on a prelude function | **Luau does not have it** |
| `getfenv(1).host.speech` | refused — the environment is the gated view |
| `getmetatable(host)` | reachable, and useless: `__index` is a Rust function that performs the check, and the full table is held in Rust |
| a scan of `host.log`, a namespace it *does* have | nothing found |

Two properties do the work, and both are worth keeping true when this code is touched:

1. **The full table never becomes a Luau value a module can name.** The gated view's `__index`
   is a Rust closure; the table it reads from is captured there. Handing the metatable out is
   therefore harmless — which is why the probe checks the metatable's *contents* rather than
   its existence.
2. **Privileged Luau never indexes `host` with a name a module supplies**, and never stores
   `host` in anything it returns. The prelude is the only privileged Luau there is; if more is
   added, it inherits this obligation, because a single `host[name]` there would be a
   capability oracle for every module in the process.

### And what it does not stop

**A declared dependency widens what a module can cause.** A module that declares nothing can
depend on one that declares `screen`, and call its functions — which then act, correctly, with
`screen`. That is the inheritance model working as designed (permission belongs to the author
of the code, not to the VM it runs in), but it means a manifest describes *what this module's
own code touches*, not the full reach of installing it. The manager shows the whole dependency
tree at install time for exactly this reason.

One thing in that direction *is* refused, because it is never a design and always an accident:
a dependency that **returns its own host table** from its entry point fails to load, naming
both modules. Exporting a function that uses `screen` is the model; exporting the table hands
over every capability the dependency declares, in one object, to a module whose manifest names
none of them. The check is by identity and one level deep — a closure that returns the table
when called cannot be caught by any amount of scanning, which is the next paragraph's point.

**Privileged Luau has one obligation, and it is not enforceable.** The window prelude runs
inside every module VM holding the ungated table. Nothing can stop such code from handing that
table out; no mechanism exists, short of not writing it that way. So the rule is written down
as a test that asserts the leak
(`privileged_luau_that_hands_out_its_host_defeats_the_gate`), next to the one that checks the
prelude as it stands obeys it: **do not return the host table, and do not store it anywhere a
module can name.**

**And the gate is a guard rail, not a sandbox.** Modules are Luau in our own process; native
FFI is a planned capability that would run in it too. The gate reliably stops a module from
reaching a namespace by accident or by casual poking — which is what it is for, and what the
probe measures. It is not a defence against an author who is actively hostile, and nothing
in this design claims to be one. The manifest's real job is to be *readable before installing*:
it is a statement by the author, which the user weighs, and the runtime holds the author to.

This catalog is the **single machine-readable source** from which the following are generated: (a) the Luau type definitions for module authors, (b) the `[capabilities]` enum in the manifest, (c) the versioned web documentation (a project-backlog item). It is versioned under `engine_api` (additive change = minor, breaking = new major; study §6).

## 1. Model

**Design principle — primitives are first-class and overlay-independent.** Every capability (`host.ocr`, `host.screen`, `host.input`, `host.window`, `host.hotkey`, `host.speech` …) is **directly scriptable from Luau, without the overlay.** The overlay is *one* optional high-level layer — accessible, self-voicing control rings in the style of ReaHotkey — built on the same primitives rather than a mandatory funnel. It shipped as a module rather than a namespace, which is exactly what that independence looks like in practice. General automation (the AHK / Keyboard Maestro class) uses the primitives directly; the overlays are merely the first use case, not the only one.

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
**Built** = exists today, with an entry in the [API reference](api/).

| Namespace | Purpose | Built | Tier | MVP | Win | macOS | macOS permission |
|---|---|---|---|---|---|---|---|
| `host.speech` | Screen-reader / TTS output (tts-rs) | yes | low | ✅ | full | full | — |
| `host.sound` | Audio asset playback | yes | low | ✅ | full | full | — |
| `host.resource` | Read/resolve package resources, module config | yes | low | ✅ | full | full | — |
| `host.hotkey` | Contextual + global hotkeys | yes | medium | ✅ | full | limited | Input Monitoring + Accessibility |
| `host.window` | Window / control introspection | yes | medium | ✅ | full | limited | Accessibility |
| `host.input` | Mouse / keyboard simulation | yes | medium | ✅ | full | limited | Accessibility |
| `host.screen` | Capture, image search, pixel/color | yes | medium | ✅ | full | limited | Screen Recording |
| `host.ocr` | Text recognition in a region/image | yes | medium | ✅ | full | limited | (uses `screen`) |
| `host.gui` | Own accessible windows (wxDragon) | **no** | medium | ○ | full | full | — |
| `host.clipboard` | Read/write clipboard | **no** | medium | ○ | full | full | — |
| `host.app` | Detect/focus/launch running apps | **no** | medium | ○ | full | limited | (Automation/AppleEvents partly) |
| `host.ffi` | Load/call native libs | **no** | **high** | ○ | full | limited | (signature, no TCC) |
| `host.log` / `host.timer` | Logging, scheduling | yes | low | ✅ | full | full | — |

The overlay module itself only speaks and reacts to its own hotkeys, which is a low tier; its
*control actions* — click, image search, OCR, reading the accessibility tree — use `input`,
`screen`, `ocr` and `uia`, and inherit their tier.

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

*Sketches, and the word is load-bearing: these are the signatures as they were imagined, not
as they were built. Where a namespace exists, its [reference page](api/) is what it actually
offers, and the two differ in places. Some calls below sit inside a namespace that shipped and
were themselves never written — `host.speech.stop`, `setRate`, `setVoice` and `voices` are the
whole of that set today.*

> Signatures are a draft. `?` = optional. Paths are always **package-relative** (the host resolves them).

### The overlay — shipped as a module, not a namespace

*One* optional high-level layer for accessible, self-voicing overlays (the ReaHotkey class),
built on `input` / `screen` / `ocr` / `uia` / `hotkey`. It is **not** required for general
automation: the primitives are directly scriptable, which is the design principle in §1.

It is not on the host table. It is a module — `com.platform.overlay`, declared under
`dependencies` and pulled in with `host.require` — which is what that independence looks like
once it is built. For what it offers, see the [Overlay reference](api/overlay).

### host.speech — speech output (tts-rs)
```luau
host.speech.output("Preset: Init", { interrupt = true })
host.speech.stop()
host.speech.setRate(n) ; host.speech.setVoice(id) ; host.speech.voices()
```
Win: Tolk→NVDA/JAWS (+ Braille) or WinRT · macOS: AVFoundation (direct TTS). **macOS option (prior art [VOCR](prior-art-vocr.md)):** while VoiceOver is running, route output to VoiceOver via AppleScript (respects the VO voice + **Braille**; requires the `apple-events` entitlement + user opt-in), otherwise AVFoundation fallback — tiered behind `host.speech`.

> **This line has been overtaken** (2026-09-02). It was wrong when it was written: Tolk was reached through `tts-rs`, whose backend calls `Tolk_Speak` and never `Tolk_Output`, so no braille ever left this application on Windows. Windows speech now goes through prism instead — `tts-rs` is gone from that build entirely — and the braille is real: `prism_backend_output` speaks and writes the display in one call, and NVDA reports that it can. There is deliberately no separate braille API; see [prism-speech-design.md](prism-speech-design.md).

### host.sound — audio feedback
```luau
local h = host.sound.play("assets/sounds/focus.ogg", { volume = 0.8 })
h:stop()
```

### host.resource — package resources
```luau
local bytes = host.resource.read("assets/data/profiles.json")  -- bytes/string from the package
local p     = host.path("assets/images/serum2/preset.png")     -- real path (escape hatch)
```

### host.settings — per-module settings (persisted)
```luau
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
```luau
host.hotkey.register("Ctrl+Alt+P", fn, { context = "overlay:Serum 2" })  -- contextual
host.hotkey.register("F1", fn, { context = "global" })
```
macOS: **registered** hotkeys via Carbon `RegisterEventHotKey` (no Input Monitoring, no silent disable — prior art [VOCR](prior-art-vocr.md); context switch via re-register per scope). CGEventTap only for suppression/remapping/hotstrings (then Input Monitoring + health watchdog). Wayland: usually not available → check `available()`.

### host.window — find & read windows/controls
Window matching is **platform-gated** (parameters are named differently per OS — there is no universal "class"). Details + trigger system: [window-matching.md](window-matching.md).
```luau
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
```luau
host.input.click(x, y, { button="left", relativeTo="pluginControl" })
host.input.move(x, y) ; host.input.drag(x1,y1, x2,y2) ; host.input.scroll(x,y, dy)
host.input.send("^s") ; host.input.text("Hallo")
```
`drag` for GraphicalSlider (ReaHotkey `MouseClickDrag`), `scroll` for mouse-wheel-based plugins (Zampler). Win: SendInput · macOS: CGEvent (`…MouseEvent`/`…ScrollWheelEvent`, Accessibility permission).

### host.screen — capture / image search / pixel
```luau
local m = host.screen.imageSearch("assets/images/serum2/preset.png", {region={x1,y1,x2,y2}})
if m then host.input.click(m.x, m.y) end
local col = host.screen.pixel(x, y)
local img = host.screen.capture({x1,y1,x2,y2})
```
Win: Windows.Graphics.Capture + SIMD-NCC · macOS: ScreenCaptureKit (Screen Recording permission).

### host.ocr — text recognition
```luau
local r = host.ocr.recognize({ region={540,13,608,23}, engine="best", lang="eng" })
-- r = { text="Init", boxes={...} }
```
Native by default (Windows.Media.Ocr / Apple Vision), ONNX fallback (study §2). Replaces ReaHotkey's Tesseract-exe invocation.

### Accessibility elements of foreign apps — shipped as `host.element`

Windows: `IUIAutomationElement`. macOS: `AXUIElement`. Replaces ReaHotkey's `UIA.ahk`
passthrough, and is built: see the [reference](api/element).

The shipped surface is not the element-handle model sketched here — there are no element
objects with `:role()` / `:name()` / `:invoke()`. It answers questions about a window instead:
find an element by name and type, locate a point to click, probe a named element's state, dump
the tree. That difference is deliberate and measured — a handle whose properties are fetched
one at a time cost 211 ms on a 53-element window, against about 30 ms for a request that names
what it wants up front.

**The name is not settled.** `uia` is what Windows calls its tree; macOS calls the same thing
something else, so the namespace currently states one platform's vendor term for an API whose
whole purpose is to abstract over both.

### host.gui — own accessible windows (wxDragon)
```luau
local win = host.gui.window{ title="Einstellungen" }
win:button{ label="Speichern", accessibleLabel="Profil speichern", onClick=fn } -- label mandatory
```
Accessibility enforced via the API (mandatory label, study §5).

### host.ffi — native libs (gated, high)
```luau
local lib = host.ffi.load("foo")           -- from native/<platform>/, signature-required
lib:call("bar", { "Int", 42 }, "Int")
```
Out-of-process sandbox for untrusted modules (study §11.1). macOS: only own Team-ID-signed or in the helper.

### host.app / host.clipboard / host.log / host.timer
```luau
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
- **The accessibility namespace moved up:** not needed in the MVP (4 of the 6 backends — Custom/Hotspot/Graphical/OCR — manage without it), but as the **next** capability right after the vertical slice rather than at the end. Reason: macOS audio plugins sometimes deliver real AX values, and where they do, the tree is more robust, faster and lower-permission than OCR, with no Retina problem. *It shipped as `host.element`, and this call was right: it is now how every plug-in in the project is identified.*
- **Detection order is platform-specific:** under macOS, AX is often empty for audio plugins (JUCE/u-he → empty `AXGroup`), which makes Vision OCR and template matching relatively more important than under Windows. The detection strategy stays portable, its prioritization does not.

Confirmed: the `host.speech` model switch on macOS (tts-rs/AVFoundation, study §11.7) and `host.sound` deliberately in the core (instead of per-plugin as in ReaHotkey).
