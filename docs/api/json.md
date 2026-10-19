---
title: "host.json — reading and writing JSON"
sidebar_position: 6.5
toc_max_heading_level: 2
---

Turns JSON text into Luau values, and Luau values into JSON text.

`decode` is for data files a module ships — a layout, a pack of signatures a converter wrote — and reads with [`host.resource.read`](./resource.md#host-resource-read). Luau has no JSON parser of its own, and a module that carried one would parse on every load in a scripting language what the host parses natively.

`encode` is for a structured log line and for noticing a change: its keys are always sorted, so two equal tables give the same text, and `encode(state) ~= last` is a real test of whether anything changed. **It writes no file** — no call does (see [`host.resource`](./resource.md)). Values a module has to keep go into [`host.settings`](./settings.md), and a setting is something the user sees: a string setting appears in the module manager as a text field, so a JSON blob kept in one is a blob a screen-reader user will meet and read out. Keep what you persist in settings the user can understand.

All three are pure computation on the main thread, with no system call; their cost is the size of the value.

**No call here answers an [error value](./errors.md#error-values).** Each does what it was asked or raises, and its entry's **Errors** says what raises: text that is not JSON, a value JSON cannot hold, an option that does not exist. Nothing here is gated, so the one other raise is the guard's ([Return or raise](./errors.md#return-or-raise)): a module that has run too long is stopped with `stopped by the host: …` ([the limits](../module-runtime-and-lifecycle.md#limits)).

## What to declare {#declare}

Nothing — this is available to every module. Each call reads nothing but the value it is handed. See [what the capability list is and is not](./index.md#capabilities).

## host.json.decode(text) {#host-json-decode}

**Signature:** `host.json.decode(text: string) -> any`

Parses `text` as one JSON document and returns it as Luau values: an object becomes a table with string keys, an array a table indexed from 1, a string a string, a number a number, `true`/`false` booleans.

**`null` becomes `nil`.** An object member whose value is `null` is simply absent, and a `null` at the top level returns `nil`. A `null` inside an array leaves a hole — `[1, null, 3]` gives `t[1] == 1`, `t[2] == nil`, `t[3] == 3` — and the length operator is not reliable across a hole, so iterate such an array by a count you know rather than with `#` or `ipairs`. The reason for the choice: the alternative is a placeholder value, and any placeholder Luau can hold is truthy, so `if cfg.optional then` would take the branch for a value that is explicitly absent.

**A key written twice in one object is not an error: the last one wins.** `{ "a": 1, "a": 3 }` gives `a == 3`, and a later `"a": null` removes an earlier value. A converter that writes a key twice by mistake is therefore not told.

Tables come back plain, with no metatable, so a module may set its own — which also means an empty array `[]` comes back as an empty table that [`encode`](#host-json-encode) writes as `{}`. Numbers are Luau numbers (doubles): an integer beyond 2^53 loses precision, as it would anywhere in Luau. A number is read as the double nearest to what is written, as .NET, Python and JavaScript read it, so a fraction another tool wrote in its shortest form comes back as exactly the number that tool had — which is what keeps a [window region](./screen.md#region-form) stored as fractions on the same pixel. At most 127 arrays and objects can be nested inside each other; a deeper document raises.

A UTF-8 byte-order mark at the very start is skipped: JSON does not allow one, but older Notepad and PowerShell 5's `Out-File -Encoding utf8` write it, and the file reaches you from `host.resource.read` exactly as it is on disk. A document that is not valid JSON raises, naming where it went wrong — `host.json.decode: expected value at line 3 column 8` — so a hand-edited data file points at its own mistake ([Errors](#host-json-decode-errors)).

```luau
-- data/layout.json: { "controls": [ { "label": "Play", "x": 12, "y": 40 }, … ] }
local layout = host.json.decode(host.resource.read("data/layout.json"))
for _, c in ipairs(layout.controls) do
  host.log.info(("%s at %d,%d"):format(c.label, c.x, c.y))
end
```

```luau
-- A file that may be hand-edited: say which line is wrong instead of failing to load.
local ok, pack = pcall(host.json.decode, host.resource.read("data/gamepack.json"))
if not ok then
  host.log.info("gamepack.json is not valid JSON: " .. tostring(pack))
  pack = { signatures = {} }
end
```

### Returns {#host-json-decode-returns}

- The document as Luau values, as above: a table, a string, a number or a boolean.
- `nil` — the document is `null`. That is the one way it answers `nil`, and no error comes with it.

### Errors {#host-json-decode-errors}

None. It never answers an error value.

**Raises** for a mistake in the call. A message about the text names the line and column where it went wrong:

- text that is not valid JSON — `host.json.decode: expected value at line 3 column 8` and the like — text that is not UTF-8 among it;
- arrays and objects nested more than 127 deep — `host.json.decode: recursion limit exceeded at line … column …`;
- a number too large for a double, such as `1e400`: `host.json.decode: number out of range at line … column …`;
- a `text` that is neither a string nor a number. A number is read as its digits, so `host.json.decode(42)` is `42`.

### Windows {#host-json-decode-windows}

The same code on every platform: `serde_json` inside the host, with no call into the operating system.

### macOS {#host-json-decode-macos}

The same code as on Windows; nothing in it is platform-specific. Its unit tests are part of the `cargo test` run of the macOS CI job.

## host.json.encode(value, opts?) {#host-json-encode}

**Signature:** `host.json.encode(value: any, opts: { pretty: (boolean | number)? }?) -> string`

Writes `value` as JSON text. Compact by default; `pretty = true` indents by two spaces, and a whole number from 1 to 8 sets the indent width. Pretty lines end in a line feed, with none after the last one.

**The options are strict.** `pretty` is the only one, and `false` or `nil` means compact. Anything else raises before anything is written, a misspelt key included ([Errors](#host-json-encode-errors)).

- **Scalars.** `nil` is `null`, a boolean is itself, a string is a JSON string — it must be valid UTF-8, non-ASCII is written as it is and `/` is not escaped. A whole number up to 2^53 in size is written as an integer, `3` and not `3.0`, and `-0` as `0`; any other number in the shortest form that reads back as exactly the same number. NaN and infinity raise: JSON has no way to write them, and turning them into `null` would hide the bug that made them.
- **Tables.** An empty table is `{}`. A table whose keys are all strings is an object, its keys written in byte order. A table whose keys are all whole numbers from 1 up is an array, with holes written as `null` — `{ 1, nil, 3 }` is `[1,null,3]` — unless it is too sparse: when its largest index is above 10 **and** more than twice the number of entries, it raises rather than padding the gap with nulls. A table marked by [`host.json.array`](#host-json-array) is an array, `[]` when empty; the sparse rule applies to it too. Only a table's own keys and values are read: `__index`, `__iter`, `__len` and every other metamethod are ignored.
- **What raises** is under [Errors](#host-json-encode-errors). The message names the place in the value, and a refused number key suggests the fix: **ids used as keys should be `tostring(id)`**, which JSON keeps as an object key.

The same table in two places is not a cycle and is simply written twice. `decode(encode(x))` gives back a value equal to `x` — metatables aside — for anything `encode` accepts, and equal tables always give equal text. Read the other way round, `encode(decode(s))` is the canonical form of `s` — compact, keys sorted, whole numbers without a fraction — except that `null` members and trailing `null` elements disappear, `[]` comes back as `{}`, and an array that is mostly `null` raises as too sparse, because `decode` turned its nulls into holes.

```luau
-- A structured log line: one line, keys in a fixed order, easy to search for.
local hit = { x = 120, y = 48, score = 0.97, template = "menu-cursor" }
host.log.info("[menu] " .. host.json.encode(hit))
-- [menu] {"score":0.97,"template":"menu-cursor","x":120,"y":48}
```

```luau
-- Speak only when the menu actually changed. Items are keyed by name, not by a numeric id:
-- an id used as a key is spelt tostring(id).
local last
local function onMenuRead(items, cursor)
  local s = host.json.encode({ items = items, cursor = cursor })
  if s ~= last then
    last = s
    host.speech.output(items[cursor])
  end
end
```

### Returns {#host-json-encode-returns}

- The JSON text, a string: compact, or indented with `pretty`, its keys in byte order. Equal values give equal text.

### Errors {#host-json-encode-errors}

None. It never answers an error value.

**Raises** for a mistake in the call, before anything is written. The options:

- an `opts` that is neither a table nor `nil` — `host.json.encode: options are a table, not a string`;
- a key other than `pretty`, a misspelling included — `host.json.encode: 'prety' is not an option; the only one is pretty` — and a key that is not a string — `host.json.encode: option names are strings, not a number`;
- a `pretty` that is not `true`, `false`, `nil` or a whole number from 1 to 8: `0`, `9`, `2.5` and `"yes"` all raise `host.json.encode: pretty is true, false or an indent width from 1 to 8, not …`.

And the value, each message naming the place, `host.json.encode: … at value…`:

- NaN or an infinity — `host.json.encode: inf has no JSON form (JSON has no NaN or infinity) at value.x`;
- a string, or a key, that is not valid UTF-8;
- a table mixing string and number keys, a number key that is not a whole number of at least 1, and a boolean, table or function used as a key;
- an array too sparse to write, as above;
- a table marked by [`host.json.array`](#host-json-array) that has a string key;
- a function, thread, userdata (a template handle, say), `vector` or `buffer` anywhere in the value — `host.json.encode: a function has no JSON form at value.states[2].cels[1]`;
- a table that contains itself — `host.json.encode: a cycle — value.self is value again`;
- tables nested more than 127 deep, which is the most `decode` reads back.

### Windows {#host-json-encode-windows}

The same code on every platform: a walk over the value inside the host, printed by `serde_json`, with no call into the operating system.

### macOS {#host-json-encode-macos}

The same code as on Windows; nothing in it is platform-specific. Its unit tests are part of the `cargo test` run of the macOS CI job.

## host.json.array(t?) {#host-json-array}

**Signature:** `host.json.array(t: table?) -> table`

Marks `t` — or a new empty table when it is left out — to be written by [`encode`](#host-json-encode) as a JSON array, and returns it. It is needed for one case only: an **empty** list, which is otherwise indistinguishable from an empty object and is written as `{}`. A table with entries 1 to n is an array already.

The mark is a metatable, one per module VM, frozen so no module can change what it means, and not protected: `setmetatable`, `table.clone` and `table.freeze` all work on a marked table as on any other. `table.clone` keeps the mark; `setmetatable` replaces it, so the table is no longer marked. Marking a table that is already marked is harmless. It raises when `t` already has a metatable of its own (marking would replace it), when `t` is frozen (mark it before `table.freeze`), and when `t` is neither a table nor `nil` ([Errors](#host-json-array-errors)). A marked table must not have string keys: `encode` raises for one.

```luau
local names = { "REAPER", "Notepad" }
local found = host.json.array()
for _, name in ipairs(names) do
  if name:find("Kontakt", 1, true) then table.insert(found, name) end
end
host.log.info(host.json.encode({ found = found }))   -- {"found":[]} when nothing matched
```

### Returns {#host-json-array-returns}

- `t` itself, marked — the same table, not a copy — or, with no `t`, a new empty table, marked.

### Errors {#host-json-array-errors}

None. It never answers an error value.

**Raises** for a mistake in the call:

- a `t` with a metatable of its own — `host.json.array: the table already has a metatable, and marking it would replace that one`;
- a frozen `t` — `host.json.array: the table is frozen, so it cannot be marked; mark it before table.freeze`;
- a `t` that is neither a table nor `nil` — `host.json.array: expected a table or nothing, not a string`.

A `t` that is marked already comes back as it is, frozen or not, and raises for none of these.

### Windows {#host-json-array-windows}

A metatable set in the host; no call into the operating system.

### macOS {#host-json-array-macos}

The same code as on Windows.
