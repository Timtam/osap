# Automation Platform (working title)

Cross-platform OS automation platform (AHK-/Keyboard-Maestro-class) with embeddable **Luau modules**. First concrete goal: make the [ReaHotkey](https://github.com/timtam/ReaHotkey) overlays macOS-capable; general automation (OCR, image detection, input, window control …) is the broader purpose and is scriptable from Luau **independently of any overlay**.

## Stack

Rust · Luau (`mlua`) · wxDragon (native GUI) · tts-rs (`host.speech`) · native FFI (gated). Goal: **Windows + macOS first**, Linux later. License: still open (manifests currently `GPL-3.0-or-later` as a placeholder).

## Build & Run

```sh
cargo build
cargo run -p app -- modules/hello
```

This loads the example module (`module.toml` → `src/main.luau`) and has it speak via the host API (tts-rs). The default module path is `modules/hello`.

> **Windows build note:** wxDragon compiles wxWidgets from source on first build. Set `LIBCLANG_PATH` to your VS/LLVM `bin` (e.g. `…\VC\Tools\Llvm\x64\bin`) and have Ninja on `PATH`. The first build takes several minutes; later builds reuse the artifacts.

Resident modules (those that register hotkeys, capture keys, or watch windows) start a **tray-resident module manager** (wxDragon): a system-tray icon plus a window that lists the loaded modules with **native checkboxes** to enable/disable each at runtime. wxWidgets owns the event loop and our OS events are pumped alongside it, so hotkeys/keys/triggers keep working. The window starts hidden — **double-click the tray icon** to open it; closing it hides it back to the tray, and the tray menu's **Quit** exits. Set `AUTOMATION_PLATFORM_HEADLESS=1` to run without any window (Ctrl+C to quit). Diagnostics are written to `automation-platform.log` next to the executable (kept off the console, which a screen reader would otherwise read aloud). Module settings and enable/disable state are stored in `settings.toml`, also next to the executable — the app keeps its state portable (no `%APPDATA%`). Modules declare settings via `host.settings.define`; the manager's per-module **Settings…** dialog edits them with native controls.

Try the global-hotkey demo: `cargo run -p app -- modules/hotkey`, then press **Ctrl+Alt+H** to hear it speak — it works whether the window is open or another app is focused. Quit from the tray icon to exit.

Try the first self-voicing **overlay** (stays resident): `cargo run -p app -- modules/overlay`, then navigate with **Ctrl+Alt+Left/Right** and activate with **Ctrl+Alt+Enter** (or the per-control hotkeys **Ctrl+Alt+1/2/3**).

Load **several modules at once** (they share one process, TTS, and event loop): `cargo run -p app -- modules/hotkey modules/window-trigger`.

A module can be loaded either as an unpacked directory (dev) or as a **`.zip` package** (extracted on demand to a content-addressed cache): `cargo run -p app -- path/to/module.zip`.

## Structure

- `crates/module-manifest` — `module.toml` parsing + module loading (dev: unpacked directory).
- `crates/host` — **module manager** + Luau runtime + `host` API (`log`/`speech`/`sound`/`hotkey`/`keys`/`window`/`os`/`screen`/`ocr`/`input`/`overlay`/`path`/`resource`/`settings`). Loads many modules concurrently (one VM each; shared backend/TTS/audio + one event loop, central event routing). The OS is reached through a `Backend` trait (`crates/host/src/backend/`, one impl per platform; Windows real, others stub); the window matcher + overlay runtime are Luau preludes.
- `crates/app` — binary `automation-platform`, loads & runs one or more modules: `automation-platform <dir1> <dir2> …`. Embeds a Windows manifest (Common Controls v6 + DPI awareness) via `build.rs`.
- `crates/host/src/gui.rs` — tray-resident wxDragon module manager: a system-tray icon + a window listing modules with **native** (`wxTreeCtrl` + `TVS_CHECKBOXES`) checkboxes to enable/disable them, plus a **per-module settings dialog** (native, screen-reader-labeled controls); hands wxWidgets the event loop and drains our OS events via a `Timer` tick (event-loop coexistence).
- `modules/hello`, `modules/hotkey`, `modules/window`, `modules/window-trigger`, `modules/screen`, `modules/ocr`, `modules/input`, `modules/sound`, `modules/overlay`, `modules/overlay-attach`, `modules/keys`, `modules/settings` — example modules (speak-once; global-hotkey trigger; window/os inspection; foreground-change trigger; screen capture & image search; OCR; mouse/keyboard input; audio playback; self-voicing overlay; context-bound overlay; low-level key capture).
- `docs/` — design documents (see below).
- `TODO.md` — backlog & implementation slices.

## Design Documentation (`docs/`)

- `architecture-feasibility-study.md` — stack decisions, feasibility matrix, risks.
- `host-api-capability-catalog.md` — the versioned host API as the single source of truth.
- `module-package-format.md` — package format (ZIP, TOML manifest, resource resolution).
- `module-runtime-and-lifecycle.md` — multi-module runtime (one process, many modules), lifecycle, dynamic enable/disable.
- `window-matching.md` — OS-gated window matcher + trigger system.
- `reahotkey-port-analysis.md` — port plan for the ReaHotkey overlays to macOS.
- `prior-art-vocr.md` — proven macOS techniques (VOCR).
