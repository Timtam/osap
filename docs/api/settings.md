---
title: "host.settings — per-module settings"
sidebar_position: 15
toc_max_heading_level: 2
---

Typed, validated settings, declared by the module and edited by the user in the module manager. For the few choices a module should not make on somebody's behalf — Komplete Kontrol has exactly one, an opt-out for automatically closing KK's library browser. Held per module and persisted in `settings.toml` beside the application (for where that is on each platform, see the platform sections of [`set`](#host-settings-set)), so they survive a restart. There is no fallback location: if that folder cannot be written, settings do not persist, and the log says so once.

`define` is the load-bearing call: it fixes the setting's kind from its default and carries the label, the bounds and the permitted choices, and **the module manager builds the settings dialog out of precisely that** — one native checkbox, number field or dropdown per setting, for a screen reader to read. Everything else works only on what was defined: `get` raises for a key that never was, and `onChange` fires for the dialog as well as for `set`, so nothing has to poll its own settings.

A module sees only its own store, keyed by module id, which also means a code module's settings stay under the id that defined them rather than under whichever VM its code happens to be running in. `host.config` is the same table under a second name.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["settings"]
```

See [what that list is and is not](./index.md#capabilities).

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

Neither value is checked against `min`, `max` or `oneOf` here: those are enforced
on `set` and in the settings dialog only. A persisted value of the right kind is
returned as it was stored, so after you narrow a `oneOf` list or tighten a range,
a value stored under the old schema comes back unchanged; and a `default` outside
its own bounds is accepted. Check the returned value yourself if that matters.

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

### Windows

`settings.toml` is next to the `.exe`.

### macOS

`settings.toml` is in the folder that holds the `.app`.

## host.settings.onChange(key, callback) {#host-settings-onchange}

Registers `callback` to run whenever this setting is set (via `set` or the
settings GUI). Returns `nil`. Multiple callbacks may be registered per key.

- `key: string` — the setting to watch.
- `callback: (new, old) -> ()` — called with the new value and the previous
  stored value.

It fires on **every** `set`, including one that stores the value already there,
so compare `new` with `old` if only a real change matters. The settings dialog's OK
applies every field, changed or not, so it fires the callbacks of every setting the
dialog shows. Unlike the module's other callbacks, these also fire while the module
is disabled — the dialog can be opened for a disabled module too. `old` is not `nil` in
practice: `define` stores the default when nothing is stored, so the first change
after load reports the default (or the persisted value) as `old`, never `nil`.

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
