---
title: "host.settings — per-module settings"
sidebar_position: 15
toc_max_heading_level: 2
---

Typed, validated settings, declared by the module and edited by the user in the module manager. For the few choices a module should not make on somebody's behalf — Komplete Kontrol has exactly one, an opt-out for automatically closing KK's library browser, and the overlay runtime one, the calibration switch every overlay module asks ([`O.calibrating`](overlay.md#o-calibrating)). Held per module and persisted in `settings.toml` beside the application (for where that is on each platform, see the platform sections of [`set`](#host-settings-set)), so they survive a restart. There is no fallback location: if that folder cannot be written, settings do not persist, and the log says so once.

`define` is the load-bearing call: it fixes the setting's kind from its default and carries the label, the bounds and the permitted choices, and **the module manager builds the settings dialog out of precisely that** — one native checkbox, number field or dropdown per setting, for a screen reader to read. Everything else works only on what was defined: `get`, `set` and `onChange` raise for a key that never was, and `onChange` fires for the dialog as well as for `set`, so nothing has to poll its own settings.

A module sees only its own store, keyed by module id, which also means a code module's settings stay under the id that defined them rather than under whichever VM its code happens to be running in. **So a shared runtime's settings are one set for every game built on it**: a setting its code defines is stored under the runtime's id and appears under the runtime in the module manager, whichever game module's VM ran the `define`, and a `set` from one game changes it for all. A choice that must differ per game — which key repeats the menu, say — is defined by the game module's own entry and handed to the runtime (see [a runtime shared by game modules](./require.md#a-runtime-shared-by-game-modules)). `host.config` is the same table under a second name.

All four calls work on an in-memory store on the main thread, in microseconds; only the save that follows a change touches the disk, on the next pass of the loop (see [`set`](#host-settings-set)).

**No call here answers an [error value](./errors.md#error-values).** Each does what it was asked or raises, and its entry's **Errors** says what raises. Two raises can come from any of them ([Return or raise](./errors.md#return-or-raise)): reaching `host.settings` or `host.config` from code whose module did not declare `settings` raises `module '…' used host.settings without declaring it — …` ([What to declare](#declare)), and a module that has run too long is stopped with `stopped by the host: …` ([the limits](../module-runtime-and-lifecycle.md#limits)).

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

A persisted value that differs from the default is written to the log the first
time a run defines it, once per module and key however many VMs define it:
`[settings] com.platform.overlay calibrate is true as stored (its default is false)`.
So a log says which settings were not at their default from the start, before
anything the module does with them.

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

### Returns {#host-settings-define-returns}

- The setting's current value, a `boolean`, `number` or `string`: the stored value when one of the default's kind is stored, otherwise `default`, which is stored then. It is not checked against `min`, `max` or `oneOf` (above).

### Errors {#host-settings-define-errors}

None. It never answers an error value.

**Raises** for a mistake in the call, and then nothing is defined or stored: a `key` that is neither a string nor a number (a number is read as its digits); a `default` that is not a boolean, a number or a string (`settings value must be a boolean, number, or string`); a string that is not valid UTF-8; an `opts` that is neither a table nor `nil`; and a mistake inside `opts`, each message beginning `host.settings.define:`:

- a key other than `label`, `min`, `max` and `oneOf` — `host.settings.define: 'mx' is not an option; the options are label, min, max and oneOf`;
- a `label` that is not a string — `label is a string, not a number`;
- a `min` or `max` that is not a number, or is NaN — `min is a number, not a string`, `max is a number, not NaN` — or that bounds a setting whose default is not a number — `max bounds a number setting, and 'mode' is a string`;
- a `min` above the `max` — `min 5 is above max 1, so no value could be set`; the two equal are a range of one value;
- a `oneOf` that is not a table — `oneOf is a list of strings, not a string`; one with an entry that is not a string — `oneOf's entry 2 is a string, not a number`, and a hole the list's length reaches, as in `{ "fast", nil, "safe" }`, is such an entry: `oneOf's entry 2 is a string, not a nil`; one with keys beside its list, a hole past the list's length among them (`t = { "fast" }; t[3] = "safe"`) — `oneOf is a list of strings, and has keys of its own beside them`; an empty one — `oneOf lists no choice`; and one on a setting whose default is not a string — `oneOf lists the choices of a string setting, and 'speed' is a number`.

Until 2026-10-24 none of these raised. A number `label` was taken as its digits, a `min` or `max` string that reads as a number as that number (`min = "0"` bounded the setting at 0), and number entries of `oneOf` as their digits; an empty `oneOf` set no choices, the keys beside its list and the entries past a hole were dropped, and every other mistake above was left without a word.

```luau
-- A misspelt bound raises at the define, rather than leaving the setting unbounded.
local ok, why = pcall(host.settings.define, "speed", 1.0, { mx = 4 })
-- ok == false; why: "host.settings.define: 'mx' is not an option; the options are label, min, max and oneOf"
```

## host.settings.get(key) {#host-settings-get}

**Signature:** `host.settings.get(key: string) -> boolean | number | string`

Returns the current stored value of a previously-defined setting
(`boolean | number | string`). Raises an error if `key` was never `define`d, or
if it has no stored value.

```luau
if host.settings.get("mode") == "safe" then ... end
```

### Returns {#host-settings-get-returns}

- The stored value of `key`, a `boolean`, `number` or `string`: the one `define` returned, or what a `set` or the settings dialog stored since.

### Errors {#host-settings-get-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `key` this module never `define`d (`setting 'speeed' was not defined`); a `key` that is neither a string nor a number. A guard no defined key reaches raises too: a key with no stored value (`setting '…' has no value`), which `define` rules out by storing one.

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

The modules' part of the file is the application's copy, read at start; the `[app]` table — the application's own switches — is taken from the file as it stands at each save, since the Application settings tab writes those to the file itself. So a switch changed in that tab stays as it was set when a module's setting is saved after it. The file is replaced whole on every save: the store is written to a temporary file of its own beside `settings.toml` (named `settings.<random>.toml.tmp`), flushed to disk, and renamed over `settings.toml`. A crash in the middle leaves the old file or the new one, never a cut-off one, and so does a power cut on a volume that can flush. At worst a temporary file is left behind; nothing reads it, and the application removes it at its next start once it is a minute old. A save that fails leaves `settings.toml` as it was and is logged once per session. On a volume that cannot flush, the save goes through without the flush, and that is logged once per session too; a power cut right after such a save can lose it.

### Returns {#host-settings-set-returns}

- Nothing: no value comes back. When `set` returns, the value is in the store, and the `onChange` callbacks registered from this module's own VM have run; the others run after it ([`onChange`](#host-settings-onchange)).

### Errors {#host-settings-set-errors}

None. It never answers an error value.

**Raises** for a mistake in the call, and then nothing is stored and no callback runs:

- a `key` this module never `define`d — `setting 'speeed' was not defined`;
- a `value` that is not a boolean, a number or a string — `settings value must be a boolean, number, or string` — or a string that is not valid UTF-8;
- a `value` of another kind than the default's — `setting 'mode': expected Str, got Number`, the kinds being `Bool`, `Number` and `Str`;
- a NaN, for any number setting, bounded or not — `setting 'speed': must be a number, not NaN`;
- a number below `min` or above `max` — `setting 'speed': must be >= 0.25`, `setting 'speed': must be <= 4`;
- a string that is not one of `oneOf` — `setting 'mode': must be one of ["fast", "safe"]`;
- a `key` that is neither a string nor a number.

**An error in an `onChange` callback does not come out of `set`.** It is reported as that callback's error, in the log and in the Module error dialog, and `set` goes on to the next callback. A save that fails does not raise either: it leaves `settings.toml` as it was and is logged, as above.

### Windows {#host-settings-set-windows}

`settings.toml` is next to the `.exe`. The rename replaces the old file in one step, even while another program has it open (a second instance reading it, a virus scanner, the search indexer, a sync client): `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, and when that is refused, a POSIX-semantics rename (`SetFileInformationByHandle`), which is what `std::fs::rename` does. The flush is `FlushFileBuffers`.

### macOS {#host-settings-set-macos}

`settings.toml` is in the folder that holds the `.app` — the original `.app` when macOS runs a translocated copy of it and can say where the original is; otherwise the copy's folder, where nothing can be saved (see [building on macOS](../building-on-macos.md#translocation)). The flush is `fcntl(F_FULLFSYNC)`, which APFS and HFS+ support and a network share (SMB, NFS) may not; there the save goes through unflushed, as described above. After the rename the folder itself is flushed as well, since on macOS a rename is only on disk once its folder is. The file is created with mode 0666 less the umask (usually `rw-r--r--`), so other accounts on the Mac can read it.

## host.settings.onChange(key, callback) {#host-settings-onchange}

**Signature:** `host.settings.onChange(key: string, callback: (new: boolean | number | string, old: (boolean | number | string)?) -> ()) -> nil`

Registers `callback` to run whenever this setting is set (via `set` or the
settings GUI). Returns `nil`. Multiple callbacks may be registered per key.

- `key: string` — the setting to watch: one this module has `define`d before the call. A key
  never defined raises, `setting 'speeed' was not defined`, as `get` and `set` do — registered,
  its callback would never fire — so `define` comes first.
- `callback: (new, old) -> ()` — called with the new value and the previous
  stored value.

It fires on **every** `set`, including one that stores the value already there,
so compare `new` with `old` if only a real change matters. The settings dialog's OK
applies every field, changed or not, so it fires the callbacks of every setting the
dialog shows. A value the dialog hands over that its setting does not take is not stored and
fires nothing; the log says so (`<module id> <key>: the value … from the dialog was not taken:
…`). Unlike the module's other callbacks, these also fire while the module
is disabled — the dialog can be opened for a disabled module too — but not while the
application has [stopped it](../module-runtime-and-lifecycle.md#limits): then they do not
run; the values are stored, and turning the module on again builds it afresh, so it reads
them as they are then. `old` is not `nil` in
practice: `define` stores the default when nothing is stored, so the first change
after load reports the default (or the persisted value) as `old`, never `nil`.

**When it runs.** A callback registered from your own module runs inside your
`host.settings.set`, before `set` returns. One registered from another module's VM — a
dependent running your `code_module`'s code, say — runs after `set` has returned, as one of
that module's callbacks, one at a time with its others: later in the same turn of the event
loop — or in the next turn when `set` ran late in a turn, after the callbacks that waited for
their modules had run: inside another such `onChange`, in a window trigger's report of the window
already in front ([`initial = true`](window.md#host-window-ontrigger)), or in the round a
[`host.window.recheck()`](window.md#host-window-recheck) asked for. A change made in the settings
dialog reaches every callback in the turn after OK, about 15 ms later — never inside the dialog's
own event — and a disabled module's in that turn too.

**A callback called for the dialog or for another module's `set` is a
[handler](../module-runtime-and-lifecycle.md#handlers)** of its module: while the module is busy —
one of its handlers [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read — it
waits behind it and runs once the module is free, in the order its events came, never dropped; and
it may wait itself, for a [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) without a callback.
Your own, run inside your `set`, is a plain call: it runs at once, also while a handler of your
module waits, and a read there without a callback cannot wait, and raises
([Where it waits](./ocr.md#where-it-waits)), as a `host.screen` call that would wait does
([its own](./screen.md#where-it-waits)).

```luau
-- Module A defines "lang" and watches it.
host.settings.onChange("lang", function(new) host.log.info("A heard " .. new) end)
host.hotkey.register("Ctrl+Alt+L", function()
  host.settings.set("lang", "de") -- "A heard de" is logged here, inside set
  host.log.info("set returned")   -- then this; a dependent's callback for "lang" runs after it
end)
```

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

### Returns {#host-settings-onchange-returns}

- Nothing: no value comes back. The callback is registered, and fires from the next change on.

### Errors {#host-settings-onchange-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `key` this module never `define`d (`setting 'speeed' was not defined`); a `key` that is neither a string nor a number; a `callback` that is not a function. An error the callback raises when it fires does not come back here: it is reported as a callback error, in the log and in the Module error dialog.

## host.config.define(key, default, opts?) {#host-config-define}

**Signature:** the same as [`host.settings.define`](#host-settings-define)'s.

`host.config` is the **same table** as `host.settings`, a catalog-compatibility alias, so each of its four calls is the `host.settings` call of the same name: the same function, the same store, the same settings dialog. It needs the `settings` capability; nothing declares `config`, and reaching `host.config` without `settings` raises `module '…' used host.config without declaring it — add "settings" …`.

```luau
local speed = host.config.define("speed", 1.0) -- the same call as host.settings.define
```

### Returns {#host-config-define-returns}

As [`host.settings.define`](#host-settings-define-returns).

### Errors {#host-config-define-errors}

None. It never answers an error value.

**Raises** as [`host.settings.define`](#host-settings-define-errors) does, with the same messages.

## host.config.get(key) {#host-config-get}

**Signature:** the same as [`host.settings.get`](#host-settings-get)'s.

The same function as `host.settings.get`, under the second name of the table ([`host.config.define`](#host-config-define)).

```luau
if host.config.get("mode") == "safe" then ... end -- the same as host.settings.get("mode")
```

### Returns {#host-config-get-returns}

As [`host.settings.get`](#host-settings-get-returns).

### Errors {#host-config-get-errors}

None. It never answers an error value.

**Raises** as [`host.settings.get`](#host-settings-get-errors) does, with the same messages.

## host.config.set(key, value) {#host-config-set}

**Signature:** the same as [`host.settings.set`](#host-settings-set)'s.

The same function as `host.settings.set`, under the second name of the table ([`host.config.define`](#host-config-define)).

```luau
host.config.set("speed", 1.5) -- identical to host.settings.set("speed", 1.5)
```

### Returns {#host-config-set-returns}

As [`host.settings.set`](#host-settings-set-returns).

### Errors {#host-config-set-errors}

None. It never answers an error value.

**Raises** as [`host.settings.set`](#host-settings-set-errors) does, with the same messages.

## host.config.onChange(key, callback) {#host-config-onchange}

**Signature:** the same as [`host.settings.onChange`](#host-settings-onchange)'s.

The same function as `host.settings.onChange`, under the second name of the table ([`host.config.define`](#host-config-define)): a callback registered through either name fires for a change made through either.

```luau
host.config.onChange("speed", function(new) host.log.info("speed is now " .. new) end)
```

### Returns {#host-config-onchange-returns}

As [`host.settings.onChange`](#host-settings-onchange-returns).

### Errors {#host-config-onchange-errors}

None. It never answers an error value.

**Raises** as [`host.settings.onChange`](#host-settings-onchange-errors) does, with the same messages.
