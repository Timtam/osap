---
title: Overview
slug: /
sidebar_position: 1
---

# OS Automation Platform

An accessible, cross-platform (Windows + macOS first) OS-automation platform in
the class of AutoHotkey / Keyboard Maestro, built in **Rust** with **Luau**
modules. Its first product goal is porting [ReaHotkey](reahotkey-port-analysis.md)'s
accessible plugin overlays as macOS-capable modules.

- **Modules** are sandboxed Luau scripts, each in its own VM (capability
  isolation). A module can **depend on** others: a `code_module` dependency's code
  runs inside the dependent's VM so its functions are reachable via `host.require`
  (the inheritance model); a plain data dependency exports only data.
- The **host API** (`host.*`) exposes the primitives — window/control
  introspection, input, screen capture + image search, OCR, UI Automation,
  speech, hotkeys, timers, persistence — directly scriptable from Luau.
- The **overlay runtime** is itself a module (`com.platform.overlay`, a
  `code_module`): depend on it and `host.require` it to build self-voicing,
  navigable accessible overlays on top of those primitives (the ReaHotkey class),
  including **nested plugin-in-plugin** overlays (a DAW hosting Komplete Kontrol
  hosting Kontakt hosting a sample library).

## Where to start

- **[Module Manager](module-manager.md)** — install, configure, update, and reload
  modules from the tray.
- **[Module package format](module-package-format.md)** — the `module.toml` manifest
  (entry, capabilities, dependencies, `code_module`).
- **[API Reference](api/window.md)** — every `host.*` function + the overlay API.
- **[Architecture feasibility study](architecture-feasibility-study.md)** — the
  design rationale.
- **[Nested overlays design](nested-overlays-design.md)** — the plugin-in-plugin
  overlay model.
- **[ReaHotkey port analysis](reahotkey-port-analysis.md)** — the source we port.

This documentation is **versioned**: use the version dropdown (top-right) to read
the reference for a specific release.
