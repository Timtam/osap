---
title: "host.tryRequire — an optional dependency"
sidebar_position: 19
toc_max_heading_level: 2
---

The same as [`host.require`](./require.md#host-require), for something listed under `optional_dependencies`: it returns `nil` when the module is absent instead of raising.

For a dependency that improves a module without being needed by it. Kontakt asks for Komplete Kontrol so that it can recognise itself nested inside a standalone KK window, and runs perfectly well when KK is not installed.

**`host.tryRequire` answers no [error value](./errors.md#error-values).** Its `nil` says only that the dependency is not there, and its entry's **Errors** says when it raises. Nothing here is gated, so the one other raise is the guard's ([Return or raise](./errors.md#return-or-raise)): a module that has run too long is stopped with `stopped by the host: …` ([the limits](../module-runtime-and-lifecycle.md#limits)).

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.tryRequire(id) {#host-tryrequire}

**Signature:** `host.tryRequire(id: string) -> any?`

Like [`host.require`](./require.md#host-require), but returns **nil** instead of raising when `id`
isn't loaded — for an **optional dependency** (an id declared in the manifest's
`optional_dependencies`, which is loaded, and for a `code_module` evaluated into this VM,
only when it is actually present). The module adapts to whether the dependency is there.

- `id: string` — a module id, typically one from this module's `optional_dependencies`.
- Cost and thread are those of [`host.require`](./require.md#host-require): a table lookup on
  the main thread for a `code_module`, a fresh copy of the data on every call for a legacy
  data dependency.

```luau
-- Kontakt optionally uses Komplete Kontrol to detect itself hosted in a standalone KK
-- window; if KK isn't installed, `kk` is nil and that detection is simply off.
local kk = host.tryRequire("com.platform.komplete-kontrol")
if kk and kk.standaloneWindow then
  hosts[#hosts + 1] = kk.standaloneWindow -- reuse KK's own exported window matcher
end
```

### Returns {#host-tryrequire-returns}

- For a `code_module` this module declares, under `optional_dependencies` or `dependencies`, and that is loaded: the value its entry file returned, evaluated in this VM — the same live value at every call, functions intact.
- For a legacy data dependency that is loaded: a fresh Luau copy of the plain data its entry returned, at every call.
- `nil` alone — the id is neither evaluated into this VM nor has a data export. The module is not installed or did not load; or it is a `code_module` this module does not declare, whose code is evaluated only into modules that list it, and a table of functions has no data export; or its entry returned no table of plain data. List a `code_module` under `optional_dependencies` (or `dependencies`) for it to come back with its functions.

As with [`host.require`](./require.md#host-require-returns), the data path does not read the manifest: a data export comes back for any module that has loaded, declared here or not.

### Errors {#host-tryrequire-errors}

None. It never answers an error value. Where [`host.require`](./require.md#host-require-errors) raises that there is nothing to return, this answers `nil`.

**Raises** for a mistake in the call: an `id` that is neither a string nor a number (a number is read as its digits), or a string that is not valid UTF-8.

A dependency whose code fails as it is evaluated into this VM does not raise here, nor in `host.require`: it fails this module's load, before its entry runs (`error running dependency '…' of '…'`).
