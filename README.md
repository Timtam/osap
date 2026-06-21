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

Try the global-hotkey demo (stays resident): `cargo run -p app -- modules/hotkey`, then press **Ctrl+Alt+H** to hear it speak. Ctrl+C to quit.

Try the first self-voicing **overlay** (stays resident): `cargo run -p app -- modules/overlay`, then navigate with **Ctrl+Alt+Left/Right** and activate with **Ctrl+Alt+Enter** (or the per-control hotkeys **Ctrl+Alt+1/2/3**).

## Structure

- `crates/module-manifest` — `module.toml` parsing + module loading (dev: unpacked directory).
- `crates/host` — Luau runtime + `host` API (`log`/`speech`/`sound`/`hotkey`/`window`/`os`/`screen`/`ocr`/`input`/`overlay`/`path`/`resource`). The OS is reached through a `Backend` trait (`crates/host/src/backend/`, one impl per platform; Windows real, others stub). The OS-gated window matcher (`find`/`findAll`) is a Luau prelude.
- `crates/app` — binary `automation-platform`, loads & runs a module.
- `modules/hello`, `modules/hotkey`, `modules/window`, `modules/window-trigger`, `modules/screen`, `modules/ocr`, `modules/input`, `modules/sound`, `modules/overlay`, `modules/overlay-attach`, `modules/keys` — example modules (speak-once; global-hotkey trigger; window/os inspection; foreground-change trigger; screen capture & image search; OCR; mouse/keyboard input; audio playback; self-voicing overlay; context-bound overlay; low-level key capture).
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
