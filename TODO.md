# Backlog / To-do

Project backlog for the cross-platform automation platform.
Architecture and feasibility foundation: [docs/architecture-feasibility-study.md](docs/architecture-feasibility-study.md).

## Implementation

- [x] **Walking Skeleton (Slice 1):** Cargo workspace (`crates/module-manifest`, `host`, `app`) + Luau embedding (mlua/luau, `error-send` feature) + `host` API (`log`/`speech`/`path`/`resource`) + module loading (`module.toml` + entry). `cargo run -p app -- modules/hello` loads the example module and speaks via tts-rs. ✓ (2026-06-21)
- [x] **Slice 2:** `host.hotkey` + event loop. Windows backend (Win32 `RegisterHotKey` + `GetMessage` loop via `windows-sys`) behind a platform-gated interface (seed of the OS-backend abstraction); a global hotkey triggers a Luau callback. Example: `modules/hotkey` (Ctrl+Alt+H → speaks). macOS Carbon `RegisterEventHotKey` to follow. ✓ (2026-06-21)
- [x] **Slice 3:** `host.window` (`list`/`active`, OS-gated declarative `find`/`findAll` matcher via a Luau prelude) + `host.os`. Windows backend (`EnumWindows`/`GetForegroundWindow` + title/class/pid/exe/bounds). Example: `modules/window`. ✓ (2026-06-21)
- [x] **Slice 4:** OS-backend trait abstraction (`backend::Backend`, Lua-agnostic). Windows impl (window enumeration + hotkeys) and a stub for other platforms; the host bridges OS events to Luau via a `HostEvents` dispatcher. Prepares the macOS backend as a second impl. ✓ (2026-06-21)
- [x] **Slice 5:** `host.window.onTrigger(matcher, {on=…}, cb)` — window event layer. Windows `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` feeds foreground changes into the event loop (woken via `PostThreadMessage`), dispatched to OS-gated matchers (Luau prelude). Example: `modules/window-trigger`. ✓ (2026-06-21, interactively confirmed: window titles announced on switching)
- [x] **Slice 6:** `host.screen` — `size`/`pixel` (GDI) + `imageSearch` (GDI BitBlt capture + PNG-template match, template alpha as mask, OS-gated region). Example: `modules/screen`. ✓ (2026-06-21)
- [x] **Slice 7:** `host.ocr` — text recognition via `Windows.Media.Ocr` (WinRT). `recognize({region, lang})` → `{text, words=[{text,x,y,w,h}]}` (boxes in screen coords). macOS Apple Vision / ONNX to follow. Example: `modules/ocr`. ✓ (2026-06-21)
- [x] **Slice 8:** `host.input` — `cursorPos`/`move`/`click`/`drag`/`scroll` (SetCursorPos + SendInput) + `send`(combo)/`text`(unicode). Windows backend. Verified non-destructively (cursor move + restore). Example: `modules/input`. ✓ (2026-06-21)
- [x] **Slice 9:** `host.sound` (rodio, fire-and-forget) + `host.overlay` runtime (Luau prelude): self-voicing control tree (StaticText/Hotspot/Custom/OCR), keyboard navigation + speech composition, built on the primitives. First overlay runs end-to-end. Examples: `modules/sound`, `modules/overlay`. ✓ (2026-06-21) — **ReaHotkey overlay-MVP capability set complete.**
- [x] **Slice 10:** Overlay auto-activation. `host.hotkey.unregister` (backend `UnregisterHotKey`) + `host.window.test` (expose the matcher predicate); `overlay:attach(matcher)` activates an overlay (registers hotkeys + announces) while a matching window is focused and deactivates on leaving — ReaHotkey-style context binding (wires onTrigger + host.overlay). Example: `modules/overlay-attach`. ✓ (2026-06-21)
- [x] **Slice 11:** `host.keys` — low-level keyboard hook (`WH_KEYBOARD_LL`) with **suppression**. `capture(name, cb)`/`release(name)`/`releaseAll()`; captured keys are swallowed (not passed to the focused app) and dispatched as `on_key` with `{shift,ctrl,alt}`. Example: `modules/keys` (toggle Tab/Escape capture with Ctrl+Alt+O). ✓ (2026-06-21)
- [ ] **Follow-up:** let `overlay:attach` opt into natural navigation (`host.keys` Tab/arrows/Enter) instead of the Ctrl+Alt hotkeys, for true-plugin contexts (where Tab isn't needed for text editing).
- [x] **Slice 13 — module-package loader:** ZIP + extract-on-install. `LoadedModule::load()` accepts a dir (dev) or a `.zip`; zips extract to a content-addressed cache `<temp>/automation-platform-modules/<id>/<version>-<hash>/` (cache-hit skips re-extraction); resources resolve against the extracted root. ✓ (2026-06-21)
- [x] **Slice 12 — module manager (concurrent multi-module runtime):** load multiple modules in one process (one Luau VM each, per-module root), sharing one `Backend`/TTS/audio + one event loop with **central event routing** (hotkeys/keys/window-triggers tagged by owning module; `mlua::Lua` is clonable so callbacks dispatch to the right VM). `app <dir1> <dir2> …`. Dispatcher honors a per-module `enabled` flag. ✓ (2026-06-21)
- [x] **Slice 14 — wxDragon GUI integration (event-loop coexistence):** resident modules open a native wxDragon window; wxWidgets owns the message loop and our OS events (hotkeys via a message-only window + `WM_HOTKEY`; foreground + low-level-key hooks) are drained from a wx `Timer` tick via `Backend::pump_pending`. New `host::gui`; `Manager::run` hands the loop to wx (or `run_event_loop` when `AUTOMATION_PLATFORM_HEADLESS=1`). App embeds a Windows manifest (Common Controls v6 + DPI) so wxWidgets uses themed controls and no startup warning. Litmus test passed: window open **and** Ctrl+Alt+H still speaks. ✓ (2026-06-21)
- [x] **Slice 15 — tray module-manager window:** the resident app is now a system-tray manager (wxTaskBarIcon: Show / Quit, double-click to open; window starts hidden, closing hides to tray). The window lists loaded modules with **native checkboxes** (`wxTreeCtrl` + `TVS_CHECKBOXES` via raw Win32, for correct screen-reader/UIA toggle exposure — generic/owner-drawn controls were rejected as not accessible enough, see [[accessibility-native-controls]]). Toggling a checkbox enables/disables the module at runtime: `Shared::set_enabled` (un)registers its OS hotkeys + recomputes the captured-key set; the dispatcher already skips disabled modules. ✓ (2026-06-21)
- [x] **Persist the enabled set:** the manager remembers disabled modules across runs via a portable `<exe_dir>/disabled-modules.txt` (one module id per line; `host::config`). A persisted disabled state is re-applied right after the module's entry loads (revoking its OS registrations). ✓ (2026-06-21)
- [ ] **Module manager follow-ups:** reload; CLI/IPC control surface; conflict detection (duplicate hotkeys / same captured key in two modules); a **native macOS/GTK checkbox path** (`TVS_CHECKBOXES` is Windows-only — non-Windows currently shows no checkboxes). Out-of-process only for the untrusted-native-FFI tier. See [docs/module-runtime-and-lifecycle.md](docs/module-runtime-and-lifecycle.md).
- [ ] Finalize the host's license (currently the placeholder `GPL-3.0-or-later`; a permissive one might be more flexible for a module ecosystem).
- [ ] **Packaging:** release builds must bundle the screen-reader client DLLs the `tolk` feature deploys next to the binary (`nvdaControllerClient64.dll`, `SAAPI64.dll`), so `host.speech` routes to NVDA/JAWS on end-user machines.
- [x] **Logging off stdout:** diagnostics now go to a file `<exe_dir>/automation-platform.log` (portable — next to the binary) via `host::logging`, not stdout/stderr (a screen reader reads the focused terminal). ✓ (2026-06-21)

## Documentation

- [ ] **API-version-specific, web-based documentation** (delivery presumably via **GitHub Pages**).
  - Versioned reference for the module/script API: one separate documentation state per API version (matching `engine_api` / `api_version`, see study §6), so that module authors see the reference matching *their* target version.
  - Version switcher on the site (docs.rs / Docusaurus versioned-docs style).
  - **Single Source of Truth:** generate the documentation from the API definition where possible (capability catalog / host-API schema), so that reference and implementation do not diverge.
  - Hosting via GitHub Pages; build in CI on every API release.

## Further open points (from the feasibility study)

- [ ] Define hard **performance/footprint budgets** (hotkey latency in ms, baseline RAM, default binary size) — study §10.2.
- [ ] Clarify **macOS distribution/MDM details** (Developer ID/notarization flow, possibly PPPC profiles for enterprise) — study §10.4.
- [ ] Work out the **native-FFI security model** (out-of-process sandbox per OS, trust store/signature flow, macOS FFI policy) — study §11.1–11.2.

## ReaHotkey port — prerequisites & tooling

Foundation: [docs/reahotkey-port-analysis.md](docs/reahotkey-port-analysis.md).

- [ ] **Pre-Flight 1:** Verify the window/plugin frame on macOS. [VOCR](docs/prior-art-vocr.md) demonstrates: the AX frame (`AXPosition`/`AXSize`) of the focused app window is reliable even for non-accessible plugin windows and serves as the capture/coordinate origin. What remains to be checked: sub-view frame of an *embedded* plugin vs. floating window (REAPER can show plugins as floating windows → simplest case).
- [ ] **Pre-Flight 2:** macOS TCC onboarding (Accessibility + Screen Recording, Input Monitoring only with CGEventTap). **Hotkey strategy per VOCR:** registered hotkeys via Carbon `RegisterEventHotKey` + context scopes → **no** CGEventTap/Input Monitoring/silent-disable needed. CGEventTap + health watchdog (`tapIsEnabled`/`tapEnable`) only if suppression/remapping/hotstrings are required. Permission persistence across self-updates (stable team/bundle ID). Study P0-2 + P0-9.
- [ ] **Overlay calibrator tool:** interactive Mac tool that records coordinates/regions/pixel colors/reference images and stores them as module assets — otherwise re-measuring content per plugin becomes a permanent cost factor.
- [ ] Concretize the **capture+OCR performance budget** (max capture frequency, ROI size) — GTune polls at 4 Hz; more expensive under ScreenCaptureKit than Windows `PixelGetColor`. Part of study §10.2.
- [ ] **MVP vertical-slice order:** runtime framework → FabFilter (1 hotspot) → GTune (OCR+timer) → u-he bundle (Diva/Hive2/Repro/Zebra, possibly 1 generic "u-he header" module) → later Engine2/Raum/Zampler/KompleteKontrol. **Dubler 2 family = very involved** (image-/coordinate-heavy; advanced audio routing via ASIO/BASSASIO — per the client also available for Mac), deliberately deferred.

## Dev tools

- [ ] **Window/control inspector** (built-in dev tool, à la AHK *Window Spy* and like ReaHotkeys *Overlay Designer* — but **inspection only, without the design/code-generation aspect**). Shows live, for the window/element under the cursor or the focused one:
  - **Window:** title, app (`name`/`bundleId`/`exe`/`pid`), OS parameters (Windows: Win32 class; macOS: AX role/subrole/identifier), `bounds`, id.
  - **Control/element under the cursor:** class (`ClassNN`) or AX role/subrole/identifier/value, geometry **relative to the window/plugin control** (for coordinate calibration), AX-tree path.
  - **Mouse:** position absolute + relative to the focused window/plugin control; **pixel color** under the cursor.
  - Live update on mouse movement (Window Spy style) + hotkey to "freeze"/copy the values.
  - **Purpose:** delivers exactly the parameters that the WindowMatcher ([docs/window-matching.md](docs/window-matching.md)) needs per OS, as well as coordinates/colors for calibration. Cross-platform (Win + macOS).
  - Built on the first-class primitives themselves (`host.window`/`host.a11y`/`host.screen`/cursor) → dogfooding the API.
  - **Related:** the *overlay calibrator* (above) builds on this — it records the inspected coordinates/regions/colors/images and stores them as module assets.
