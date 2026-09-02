---
title: "host.tryRequire — an optional dependency"
sidebar_position: 19
toc_max_heading_level: 2
---

The same as [`host.require`](./require.md#host-require), for something listed under `optional_dependencies`: it returns `nil` when the module is absent instead of raising.

For a dependency that improves a module without being needed by it. Kontakt asks for Komplete Kontrol so that it can recognise itself nested inside a standalone KK window, and runs perfectly well when KK is not installed.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.tryRequire(id) {#host-tryrequire}

Like [`host.require`](./require.md#host-require), but returns **nil** instead of raising when `id`
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
