---
title: "host.resource / host.path / host.settings / host.config / host.require / host.providers"
sidebar_position: 5
---

These functions cover package-relative file access, per-module settings, and
cross-module dependencies. Each module runs in its own isolated Luau VM. *How
much* crosses a module boundary depends on the dependency kind: a
[**`code_module`**](../module-package-format.md) dependency has its *code*
evaluated inside the dependent's VM, so its **functions** are reachable via
`host.require` (the inheritance model — base/library modules work this way); a
legacy (non-code) dependency exports only plain data (booleans, numbers, strings,
and tables thereof) via `host.require` / `host.providers`, since raw values,
closures, and userdata can't otherwise cross VMs.

Paths are resolved relative to the *calling module's own root directory* (the
unpacked package), which the host tracks per module — a module never sees
another module's settings or files unless data is deliberately exported.

## host.path(rel)

Resolves a package-relative path to an **absolute** filesystem path string
(`string`), joining `rel` onto the calling module's root and absolutising it
(so the path stays valid even when handed to another module with a different
working directory). It does not check that the file exists.

```luau
local img = host.path("assets/kontakt.png") -- "C:\...\modules\my-mod\assets\kontakt.png"
host.providers("plugin-overlay") -- e.g. pass img to a container module
```

## host.resource.read(rel)

Reads a package-relative file as a UTF-8 string (`string`) from the calling
module's root; raises a Luau error if the file is missing or not valid UTF-8.

```luau
local json = host.resource.read("data/layout.json")
local layout = parse(json)
```

## host.settings.define(key, default, opts?)

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

## host.settings.get(key)

Returns the current stored value of a previously-defined setting
(`boolean | number | string`). Raises an error if `key` was never `define`d, or
if it has no stored value.

```luau
if host.settings.get("mode") == "safe" then ... end
```

## host.settings.set(key, value)

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

## host.settings.onChange(key, callback)

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

## host.config.get / host.config.set / host.config.define / host.config.onChange

`host.config` is the **same table** as `host.settings` (a catalog-compatibility
alias). `host.config.get(key)`, `host.config.set(key, value)`,
`host.config.define(...)`, and `host.config.onChange(...)` are identical to their
`host.settings.*` counterparts above.

```luau
host.config.set("speed", 1.5) -- identical to host.settings.set("speed", 1.5)
```

## host.require(id)

Returns the object that dependency `id` exported, where `id` is a module declared
in this module's manifest `dependencies`. Raises an error if `id` is not loaded or
exported nothing.

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

## host.providers(contract)

Decentralised discovery: returns an array (`{ table }`) of the export tables of
**every** loaded module whose exports declare `provides == contract`. No central
registry — a container module finds its extension modules by contract string.
The result is live: a hot-loaded provider appears on the next call. Data only.

- `contract: string` — the contract id to match against each module's exported
  `provides` field.
- Returns: a sequence of export tables (empty if no module provides the contract).

```luau
-- an extension module's entry returned `return { provides = "kontakt-overlay", name = "...", regions = {...} }`
for _, ext in ipairs(host.providers("kontakt-overlay")) do
  registerOverlay(ext.name, ext.regions)
end
```

