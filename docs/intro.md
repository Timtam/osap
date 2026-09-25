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

- **Modules** are Luau scripts, each in its own VM, that may call only the host
  namespaces their manifest declares. All of them run on one thread, with no time
  or memory limit; the slow work of `host.ocr.read` and of the asynchronous
  screen searches runs on threads of the host's own and answers in a callback
  (see [Threads](module-runtime-and-lifecycle.md#threads)). A module can
  **depend on** others: a `code_module` dependency's code runs inside the
  dependent's VM so its functions are reachable via `host.require` (the
  inheritance model); a plain data dependency exports only data.
- The **host API** (`host.*`) exposes the primitives — window/control
  introspection, input, screen capture, image search and grid signatures, OCR,
  UI Automation, game controllers, speech, hotkeys, timers, persistence —
  directly scriptable from Luau.
- The **overlay runtime** is itself a module (`com.platform.overlay`, a
  `code_module`): depend on it and `host.require` it to build self-voicing,
  navigable accessible overlays on top of those primitives (the ReaHotkey class),
  including **nested plugin-in-plugin** overlays (a DAW hosting Komplete Kontrol
  hosting Kontakt hosting a sample library).

## Where to start

- **[Module Manager](module-manager.md)** — install, configure, update, and reload
  modules from the tray.
- **[Module package format](module-package-format.md)** — the `module.toml` manifest
  (entry, capabilities, dependencies, `code_module`) and where modules are found.
- **[API Reference](api/index.md)** — every `host.*` function + the overlay API; if you
  are porting from another tool, start with
  [Coming from another tool](api/index.md#coming-from).
- **[Architecture feasibility study](architecture-feasibility-study.md)** — the
  design rationale.
- **[Nested overlays design](nested-overlays-design.md)** — the plugin-in-plugin
  overlay model.
- **[ReaHotkey port analysis](reahotkey-port-analysis.md)** — the source we port.

This documentation describes the **current** build; there is one version of it.
The Windows package the build workflow makes carries the same pages offline, built
from the same commit as the application beside them.
