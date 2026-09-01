---
title: "host.resource / host.path / host.settings / host.config / host.require / host.tryRequire"
sidebar_position: 5
toc_max_heading_level: 2
---

These functions cover package-relative file access, per-module settings, and
cross-module dependencies. Each module runs in its own isolated Luau VM. *How
much* crosses a module boundary depends on the dependency kind: a
[**`code_module`**](../module-package-format.md) dependency has its *code*
evaluated inside the dependent's VM, so its **functions** are reachable via
`host.require` (the inheritance model — base/library modules work this way); a
legacy (non-code) dependency exports only plain data (booleans, numbers, strings,
and tables thereof) via `host.require`, since raw values, closures, and userdata
can't otherwise cross VMs. A dependency may also be **optional**
(`optional_dependencies`): loaded only if present, and fetched with `host.tryRequire`,
which returns `nil` when it isn't installed.

Paths are resolved relative to the *calling module's own root directory* (the
unpacked package), which the host tracks per module — a module never sees
another module's settings or files unless data is deliberately exported.

## host.path(rel) {#host-path}

Resolves a package-relative path to an **absolute** filesystem path string
(`string`), joining `rel` onto the calling module's root and absolutising it
(so the path stays valid even when handed to another module with a different
working directory). It does not check that the file exists.

```luau
local img = host.path("assets/kontakt.png") -- "C:\...\modules\my-mod\assets\kontakt.png"
```

## host.resource.read(rel) {#host-resource-read}

Reads a package-relative file as a UTF-8 string (`string`) from the calling
module's root; raises a Luau error if the file is missing or not valid UTF-8.

```luau
local json = host.resource.read("data/layout.json")
local layout = parse(json)
```

## host.settings.define(key, default, opts?) {#host-settings-define}

Registers a setting `key` for this module, pins its kind from `default`, and
returns the **current effective value** (`boolean | number | string`).

- `key: string` — setting id (unique within the module).
- `default: boolean | number | string` — also fixes the setting's kind. Numbers
  with no fractional part are stored as integers, otherwise as floats; both are
  the `Number` kind in Luau.
- `opts: table?` — optional schema metadata:
  - `label: string` — display label in the manager (defaults to `key`).
  - `min: number`, `max: number` — inclusive numeric bounds (enforced on `set`).
  - `oneOf: { string }` — allowed string choices (enforced on `set`).

Returns a persisted value if one exists *and* its kind matches the default;
otherwise it writes `default` to the store and returns it. Other value types
(table, nil) raise an error.

```luau
local speed = host.settings.define("speed", 1.0, {
  label = "Playback speed", min = 0.25, max = 4.0,
})
local mode  = host.settings.define("mode", "fast", { oneOf = { "fast", "safe" } })
```

## host.settings.get(key) {#host-settings-get}

Returns the current stored value of a previously-defined setting
(`boolean | number | string`). Raises an error if `key` was never `define`d, or
if it has no stored value.

```luau
if host.settings.get("mode") == "safe" then ... end
```

## host.settings.set(key, value) {#host-settings-set}

Validates `value` against the setting's schema and writes it to the store
(persisted to disk on the next event-loop tick), then fires any registered
`onChange` callbacks. Returns `nil`.

- `key: string` — must be a previously `define`d setting.
- `value: boolean | number | string` — must match the setting's kind and satisfy
  its `min`/`max`/`oneOf` constraints, else an error is raised and nothing changes.

```luau
host.settings.set("speed", 2.0)
host.settings.set("mode", "safe")
```

## host.settings.onChange(key, callback) {#host-settings-onchange}

Registers `callback` to run whenever this setting changes (via `set` or the
settings GUI). Returns `nil`. Multiple callbacks may be registered per key.

- `key: string` — the setting to watch.
- `callback: (new, old) -> ()` — called with the new value and the previous
  value (the previous value is `nil` on the first set).

```luau
host.settings.onChange("speed", function(new, old)
  host.log.info(("speed: %s -> %s"):format(tostring(old), tostring(new)))
end)
```

## host.config.get / host.config.set / host.config.define / host.config.onChange {#host-config-get}

`host.config` is the **same table** as `host.settings` (a catalog-compatibility
alias). `host.config.get(key)`, `host.config.set(key, value)`,
`host.config.define(...)`, and `host.config.onChange(...)` are identical to their
`host.settings.*` counterparts above.

```luau
host.config.set("speed", 1.5) -- identical to host.settings.set("speed", 1.5)
```

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

