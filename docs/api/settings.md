---
title: "host.settings — per-module settings"
sidebar_position: 15
toc_max_heading_level: 2
---

Typed, validated settings, declared by the module and edited by the user in the module manager. For the few choices a module should not make on somebody's behalf — Komplete Kontrol has exactly one, an opt-out for automatically closing KK's library browser. Held per module and persisted in `settings.toml` beside the application (for where that is on each platform, see the platform sections of [`set`](#host-settings-set)), so they survive a restart. There is no fallback location: if that folder cannot be written, settings do not persist, and the log says so once.

`define` is the load-bearing call: it fixes the setting's kind from its default and carries the label, the bounds and the permitted choices, and **the module manager builds the settings dialog out of precisely that** — one native checkbox, number field or dropdown per setting, for a screen reader to read. Everything else works only on what was defined: `get` raises for a key that never was, and `onChange` fires for the dialog as well as for `set`, so nothing has to poll its own settings.

A module sees only its own store, keyed by module id, which also means a code module's settings stay under the id that defined them rather than under whichever VM its code happens to be running in. **So a shared runtime's settings are one set for every game built on it**: a setting its code defines is stored under the runtime's id and appears under the runtime in the module manager, whichever game module's VM ran the `define`, and a `set` from one game changes it for all. A choice that must differ per game — which key repeats the menu, say — is defined by the game module's own entry and handed to the runtime (see [a runtime shared by game modules](./require.md#a-runtime-shared-by-game-modules)). `host.config` is the same table under a second name.

All four calls work on an in-memory store on the main thread, in microseconds; only the save that follows a change touches the disk, on the next pass of the loop (see [`set`](#host-settings-set)).

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["settings"]
```

See [what that list is and is not](./index.md#capabilities).

## host.settings.define(key, default, opts?) {#host-settings-define}

**Signature:** `host.settings.define(key: string, default: boolean | number | string, opts: { label: string?, min: number?, max: number?, oneOf: { string }? }?) -> boolean | number | string`

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

**Signature:** `host.settings.get(key: string) -> boolean | number | string`

Returns the current stored value of a previously-defined setting
(`boolean | number | string`). Raises an error if `key` was never `define`d, or
if it has no stored value.

```luau
if host.settings.get("mode") == "safe" then ... end
```

## host.settings.set(key, value) {#host-settings-set}

**Signature:** `host.settings.set(key: string, value: boolean | number | string) -> nil`

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

The file is replaced whole on every save: the store is written to a temporary file of its own beside `settings.toml` (named `settings.<random>.toml.tmp`), flushed to disk, and renamed over `settings.toml`. A crash in the middle leaves the old file or the new one, never a cut-off one, and so does a power cut on a volume that can flush. At worst a temporary file is left behind; nothing reads it, and the application removes it at its next start once it is a minute old. A save that fails leaves `settings.toml` as it was and is logged once per session. On a volume that cannot flush, the save goes through without the flush, and that is logged once per session too; a power cut right after such a save can lose it.

### Windows

`settings.toml` is next to the `.exe`. The rename replaces the old file in one step, even while another program has it open (a second instance reading it, a virus scanner, the search indexer, a sync client): `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, and when that is refused, a POSIX-semantics rename (`SetFileInformationByHandle`), which is what `std::fs::rename` does. The flush is `FlushFileBuffers`.

### macOS

`settings.toml` is in the folder that holds the `.app`. The flush is `fcntl(F_FULLFSYNC)`, which APFS and HFS+ support and a network share (SMB, NFS) may not; there the save goes through unflushed, as described above. After the rename the folder itself is flushed as well, since on macOS a rename is only on disk once its folder is. The file is created with mode 0666 less the umask (usually `rw-r--r--`), so other accounts on the Mac can read it.

## host.settings.onChange(key, callback) {#host-settings-onchange}

**Signature:** `host.settings.onChange(key: string, callback: (new: boolean | number | string, old: (boolean | number | string)?) -> ()) -> nil`

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

A callback belongs to the VM it was registered from, and goes when that module is
reloaded. That matters for a `code_module`, whose code also runs inside every module
that depends on it (see [`host.require`](./require.md#host-require)): a callback it
registers there watches the code module's own setting, but is dropped when the
**dependent** is reloaded, and the rebuilt dependent registers it again. Reloading the
code module itself does not take them. The reload rebuilds each module that lists it in
`dependencies` next, so theirs go then; one that reaches it only through
`optional_dependencies`, and every dependent after a reload of the code module that
failed, keeps them until that module is reloaded itself (until then it still runs the old
code, which registered them).

```luau
host.settings.onChange("speed", function(new, old)
  host.log.info(("speed: %s -> %s"):format(tostring(old), tostring(new)))
end)
```

## host.config.get / host.config.set / host.config.define / host.config.onChange {#host-config-get}

**Signature:** the same as [`host.settings`](#host-settings-define)'s four: `host.config.define(key, default, opts?)`, `host.config.get(key)`, `host.config.set(key, value)`, `host.config.onChange(key, callback)`.

`host.config` is the **same table** as `host.settings` (a catalog-compatibility
alias). `host.config.get(key)`, `host.config.set(key, value)`,
`host.config.define(...)`, and `host.config.onChange(...)` are identical to their
`host.settings.*` counterparts above.

```luau
host.config.set("speed", 1.5) -- identical to host.settings.set("speed", 1.5)
```
