---
title: "host.require — a declared dependency"
sidebar_position: 12
toc_max_heading_level: 2
---

Returns what a dependency declared in your manifest exports.

For a `code_module` — the overlay runtime, Kontakt, daw-hosts — that dependency's source has already been evaluated **inside your own VM**, so what comes back has its functions intact. This is how every module obtains `O`, and how Cinematic Studio Series inherits Kontakt's framework and calls `kontakt.library`.

Each module runs in its own isolated Luau VM, and **how much crosses a module boundary depends on the dependency kind.** A [`code_module`](../module-package-format.md) dependency has its *code* evaluated inside the dependent's VM, so its **functions** are reachable through `host.require` — that is the inheritance model every base and library module is built on. A legacy, non-code dependency exports only plain data (booleans, numbers, strings and tables of those), because raw values, closures and userdata cannot otherwise cross a VM boundary.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.require(id) {#host-require}

Returns the object that dependency `id` exported, where `id` is a module declared
in this module's manifest `dependencies`. Raises an error if `id` is not loaded or
exported nothing — use [`host.tryRequire`](./tryrequire.md#host-tryrequire) for an optional
dependency that may be absent.

- If `id` is a [**`code_module`**](../module-package-format.md), its code was
  evaluated inside *this* VM, so `host.require` returns the live table it returned
  — **functions intact**. You call its functions directly; this is how
  inheritance / library modules build on a base module.
- Otherwise (a legacy data dependency), it returns a fresh Luau value mirroring
  the plain **data** the dependency exported (functions can't cross a VM this way).

```luau
-- a code_module dependency exposing functions:
local kontakt = host.require("com.platform.kontakt")
kontakt.library({ name = "Cinematic Studio Strings", image = host.path("css.png") })

-- a legacy data dependency: `return { presets = {...}, version = 3 }`
local lib = host.require("com.example.preset-library")
for _, p in ipairs(lib.presets) do ... end
```
