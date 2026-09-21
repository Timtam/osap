---
title: "host.json — reading JSON"
sidebar_position: 6.5
toc_max_heading_level: 2
---

Turns JSON text into Luau values.

For data files a module ships — a layout, a pack of signatures a converter wrote — and reads with [`host.resource.read`](./resource.md#host-resource-read). Luau has no JSON parser of its own, and a module that carried one would parse on every load in a scripting language what the host parses natively.

**Decode only.** There is no `encode`: nothing here turns a Luau value into JSON text, and no call writes a file anyway (see [`host.resource`](./resource.md)). Values a module has to keep go into [`host.settings`](./settings.md).

## What to declare {#declare}

Nothing — this is available to every module. It reads nothing but the string it is handed. See [what the capability list is and is not](./index.md#capabilities).

## host.json.decode(text) {#host-json-decode}

**Signature:** `host.json.decode(text: string) -> any`

Parses `text` as one JSON document and returns it as Luau values: an object becomes a table with string keys, an array a table indexed from 1, a string a string, a number a number, `true`/`false` booleans.

**`null` becomes `nil`.** An object member whose value is `null` is simply absent, and a `null` at the top level returns `nil`. A `null` inside an array leaves a hole — `[1, null, 3]` gives `t[1] == 1`, `t[2] == nil`, `t[3] == 3` — and the length operator is not reliable across a hole, so iterate such an array by a count you know rather than with `#` or `ipairs`. The reason for the choice: the alternative is a placeholder value, and any placeholder Luau can hold is truthy, so `if cfg.optional then` would take the branch for a value that is explicitly absent.

Tables come back plain, with no metatable, so a module may set its own. Numbers are Luau numbers (doubles): an integer beyond 2^53 loses precision, as it would anywhere in Luau.

A UTF-8 byte-order mark at the very start is skipped: JSON does not allow one, but older Notepad and PowerShell 5's `Out-File -Encoding utf8` write it, and the file reaches you from `host.resource.read` exactly as it is on disk. A document that is not valid JSON raises, naming where it went wrong — `host.json.decode: expected value at line 3 column 8` — so a hand-edited data file points at its own mistake.

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

### Windows

The same code on every platform: `serde_json` inside the host, with no call into the operating system.

### macOS

The same code as on Windows; nothing in it is platform-specific. Its unit tests are part of the `cargo test` run of the macOS CI job.
