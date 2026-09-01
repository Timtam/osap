---
title: "Module imports — host.require, tryRequire, include"
sidebar_position: 18
toc_max_heading_level: 2
---

Three ways to reach code that is not in the file you are writing.

`host.require` names a dependency declared in the manifest, and for a `code_module` — the overlay runtime, Kontakt, daw-hosts — that dependency's source has already been evaluated **inside your own VM**, so what comes back has its functions intact. This is how every module obtains `O`, and how Cinematic Studio Series inherits Kontakt's framework and calls `kontakt.library`.

`host.tryRequire` is the same call for something listed under `optional_dependencies`, returning `nil` when it is absent instead of raising: Kontakt asks for Komplete Kontrol so it can recognise itself nested inside a standalone KK window, and runs perfectly well when KK is not installed.

`host.include` is your own module's other files. ON:EAR splits into the application it lives in, its coordinate geometry, its tree reader and a file each for the browser and settings panels — every one evaluated once per VM and handed the same `host` as the file that included it.

Each module runs in its own isolated Luau VM, and **how much crosses a module boundary depends on the dependency kind.** A [`code_module`](../module-package-format.md) dependency has its *code* evaluated inside the dependent's VM, so its **functions** are reachable through `host.require` — that is the inheritance model every base and library module is built on. A legacy, non-code dependency exports only plain data (booleans, numbers, strings and tables of those), because raw values, closures and userdata cannot otherwise cross a VM boundary.

Paths resolve against the *calling module's own root* — the unpacked package — which the host tracks per module. A module never sees another module's settings or files unless something is deliberately exported.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.include(rel) {#host-include}

**Signature:** `host.include(rel: string) -> any`

Loads **another file of this module** and returns whatever that file returns — a table of functions, a table of constants, an overlay, anything. This is how a module becomes more than one file: shared helpers, reference data, one overlay per file.

Not to be confused with [`host.require`](#host-require), which imports a *different module* by id.

```luau
-- src/geometry.luau
return { GEOMETRY = { … }, ARROWS = { … } }

-- src/main.luau
local geo = host.include("src/geometry.luau")
for _, a in ipairs(geo.ARROWS) do … end
```

Semantics worth knowing:

- The included file sees the **same `host`** as the file that included it, handed in rather than read from the globals. So inside a **code module** — whose source is evaluated in each dependent's VM — an included file resolves paths, settings and resources against the **defining** module, exactly as its includer does.
- Executed **once per VM**; further includes of the same file return the same value. A module split across files must not re-run side effects per include. (A code module still runs once per dependent VM, and so do its includes.)
- Paths are relative to the module root. `..` and absolute paths are **rejected** — this executes code, so escaping the module directory is not a nuisance but a hole.
- An include **cycle** raises an error naming the file rather than overflowing the stack.
- Reported line numbers match the file.

## host.require(id) {#host-require}

Returns the object that dependency `id` exported, where `id` is a module declared
in this module's manifest `dependencies`. Raises an error if `id` is not loaded or
exported nothing — use [`host.tryRequire`](#host-tryrequire) for an optional
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

## host.tryRequire(id) {#host-tryrequire}

Like [`host.require`](#host-require), but returns **nil** instead of raising when `id`
isn't loaded — for an **optional dependency** (an id declared in the manifest's
`optional_dependencies`, which is loaded, and for a `code_module` evaluated into this VM,
only when it is actually present). The module adapts to whether the dependency is there.

- `id: string` — a module id, typically one from this module's `optional_dependencies`.
- Returns: the dependency's exported object (functions intact for a `code_module`) if it
  is loaded, otherwise `nil`.

```luau
-- Kontakt optionally uses Komplete Kontrol to detect itself hosted in a standalone KK
-- window; if KK isn't installed, `kk` is nil and that detection is simply off.
local kk = host.tryRequire("com.platform.komplete-kontrol")
if kk and kk.standaloneWindow then
  hosts[#hosts + 1] = kk.standaloneWindow -- reuse KK's own exported window matcher
end
```
