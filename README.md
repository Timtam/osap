# Automation Platform (working title)

> **Status: pre-alpha, and public mostly so that the macOS builds can run.** The Windows
> side runs the shipped overlays daily. The macOS port is being tested blind, with a remote
> VoiceOver user as the only tester, and does not yet work end to end there. The host API
> changes without notice, the module format with it, and **the name — of the project and of
> this repository — is a placeholder that will change.** Nothing here is a release; if you try
> it anyway, `TODO.md` and `docs/` say what is known not to work.

> **How it is written:** with AI assistance, throughout — and read, questioned and directed
> by a human with software-development experience. The code, the commit messages and the
> design documents are the product of that pairing, not of either half alone.

Cross-platform OS automation platform (AHK-/Keyboard-Maestro-class) with embeddable **Luau modules**. First concrete goal: make the [ReaHotkey](https://github.com/timtam/ReaHotkey) overlays macOS-capable; general automation (OCR, image detection, input, window control …) is the broader purpose and is scriptable from Luau **independently of any overlay**.

## Stack

Rust · Luau (`mlua`) · wxDragon (native GUI) · speech through the running screen reader (prism on Windows, VoiceOver on macOS) with the system voice as the fallback (`host.speech`) · native FFI (gated). Goal: **Windows + macOS first**, Linux later. License: **GPL-3.0-or-later** (`LICENSE`; the module manifests say the same).

## Build & Run

```sh
cargo build
cargo run -p app -- examples/hello
```

This loads the example module (`module.toml` → `src/main.luau`) and has it speak via the host API (tts-rs). The default module path is `examples/hello`.

> **Windows build note:** wxDragon compiles wxWidgets from source on first build. Set `LIBCLANG_PATH` to your VS/LLVM `bin` (e.g. `…\VC\Tools\Llvm\x64\bin`) and have Ninja on `PATH`. The first build takes several minutes; later builds reuse the artifacts.

> **Ready-made test builds:** every push to `main` that touches code runs the **Build** workflow (Actions tab), which builds Windows and macOS side by side. A run that succeeds carries both downloads, `automation-platform-<version>-<commit>-windows` and `automation-platform-<version>-<commit>-macos` (a universal `.app`, Intel and Apple silicon).
>
> **Which build someone tested:** the application names its build at the start of every session in `automation-platform.log` (`[host] version 0.1.0, build 6c95c8b`) and in the title of the module manager window (`Automation Platform — Modules (0.1.0, build 6c95c8b)`), and the package's `README.txt` names it near the top. It is the commit in the download's name, recorded by the packaging scripts in `build-info.txt` beside the executable (beside the `.app` on macOS). When the macOS job reused an earlier executable because no Rust had changed, the log adds `(binary built from <commit>)`; a build of your own from `target/` names the commit it was compiled from, with `-modified` when the working tree had uncommitted changes.

Resident modules (those that register hotkeys, capture keys, or watch windows) start a **tray-resident module manager** (wxDragon): a system-tray icon plus a window that lists the loaded modules with **native checkboxes** to enable/disable each at runtime. wxWidgets owns the event loop and our OS events are pumped alongside it, so hotkeys/keys/triggers keep working. The window starts hidden — **double-click the tray icon** to open it (on macOS it is a
menu-bar item and its menu has **Show module manager**); closing it hides it back to the tray, and the tray menu's **Quit** exits. Starting the application again while it runs does not start a second copy: the running one brings its module manager window forward (see [docs/module-manager.md](docs/module-manager.md)). Set `AUTOMATION_PLATFORM_HEADLESS=1` to run without any window (Ctrl+C to quit); headless runs are exempt from the one-copy rule, so a test or tool can run beside the application. Diagnostics are written to `automation-platform.log` next to the executable — next to the
`.app` on macOS, where "beside the executable" would mean inside the bundle (kept off the console, which a screen reader would otherwise read aloud). Module settings and enable/disable state are stored in `settings.toml`, also next to the executable — the app keeps its state portable (no `%APPDATA%`). Modules declare settings via `host.settings.define`; the manager's per-module **Settings…** dialog edits them with native controls.

Try the global-hotkey demo: `cargo run -p app -- examples/hotkey`, then press **Ctrl+Alt+H** (**Shift+Command+F7** on a Mac, with F-keys as standard function keys or fn held) to hear it speak — it works whether the window is open or another app is focused. Quit from the tray icon to exit.

Try a self-voicing **overlay**: `cargo run -p app -- examples/overlay-attach` — it activates while a **Notepad** window is focused. Navigate its controls with **Tab / Shift+Tab** and activate the focused one with **Enter** or **Space** (the per-control hotkeys **Ctrl+Alt+1/3** also work while it is active).

Load **several modules at once** (they share one process, TTS, and event loop): `cargo run -p app -- examples/hotkey examples/window-trigger`.

A module can be loaded either as an unpacked directory (dev) or as a **`.zip` package** (extracted on demand to a content-addressed cache): `cargo run -p app -- path/to/module.zip`.

## Structure

- `crates/module-manifest` — `module.toml` parsing + module loading (dev: unpacked directory).
- `crates/host` — **module manager** + Luau runtime + `host` API (`window`/`screen`/`ocr`/`element`/`input`/`keys`/`hotkey`/`gamepad`/`speech`/`sound`/`timer`/`settings`/`log`/`path`/`resource`/`json`/`os`/`require`/`include`/`arbiter`, listed in full in [docs/api/index.md](docs/api/index.md); the overlay is a module, `com.platform.overlay`, not a namespace). Loads many modules concurrently (one VM each; shared backend/TTS/audio + one event loop, central event routing). The OS is reached through a `Backend` trait (`crates/host/src/backend/`, one impl per platform: Windows, macOS, and a stub for everything else); the window matcher + overlay runtime are Luau preludes. OCR uses `Windows.Media.Ocr` (fast, native) with an **embedded PaddleOCR recognition model run via ONNX Runtime** (`backend::paddle_ocr`, model baked in with `include_bytes!`) as a concurrent fallback for text WinRT can't read — notably isolated single digits. Overlays attach to standalone plugin windows *or* to plugins **embedded in a DAW host window** (REAPER/Ableton), detected by a child-control class on the keyboard-**focus chain** (`host.window.controls` / `focusChain`, driven by an `EVENT_OBJECT_FOCUS` hook).
- `crates/app` — binary `automation-platform`, loads & runs one or more modules: `automation-platform <dir1> <dir2> …`. Embeds a Windows manifest (Common Controls v6 + DPI awareness) via `build.rs`.
- `crates/host/src/gui.rs` — tray-resident wxDragon module manager: a system-tray icon + a window listing modules with **native** (`wxTreeCtrl` + `TVS_CHECKBOXES`) checkboxes to enable/disable them, plus a **per-module settings dialog** (native, screen-reader-labeled controls); hands wxWidgets the event loop and drains our OS events via a `Timer` tick (event-loop coexistence).
- `modules/` — the **real** modules, the set you would actually run:
  - `overlay-runtime` (`com.platform.overlay`) — the overlay framework itself, a **code module**: its code is evaluated inside each dependent's VM, so dependents get its functions and not just its data.
  - `daw-hosts` — a **library module**: shared DAW host-window matchers, imported via `host.require`. Modules declare `dependencies` in `module.toml` and those are auto-loaded first.
  - `kontakt`, `komplete-kontrol` — the ReaHotkey plugin overlays: one overlay per cell of {Kontakt 7, Kontakt 8} × {bare in a DAW, nested in Komplete Kontrol, standalone}.
  - `cinematic-studio-series` — a sample-library overlay built on `kontakt` by inheritance.
  - `sforzando` — the first real ReaHotkey port: a self-voicing OCR overlay over the standalone sforzando window.
- `examples/` — the API demos, one per capability: `hello` (speak once), `hotkey`, `window`, `window-trigger`, `screen`, `ocr`, `input`, `sound`, `keys`, `settings`, `overlay-attach` (a context-bound overlay over Notepad). Kept out of `modules/` so that running everything in `modules/` means running the real thing.
- `tools/inspect` — OCR window-inspector dev tool: **Ctrl+Alt+I** logs every recognized word with client-relative coordinates. Largely superseded for overlay work by the built-in calibrator ("Calibration keys in overlays" in the Application settings tab).
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
