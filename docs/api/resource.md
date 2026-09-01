---
title: "host.resource — bundled files"
sidebar_position: 11
toc_max_heading_level: 2
---

Reads a file bundled inside your own module, and asks whether one is there.

For manifests and data files rather than for images: `read` decodes as UTF-8 and raises when the file is missing or is not text, so a PNG is an error rather than a string. Paths are relative to the calling module's own root — the unpacked package — which the host tracks per module.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["resource"]
```

See [what that list is and is not](./index.md#capabilities).

## host.resource.read(rel) {#host-resource-read}

Reads a package-relative file as a UTF-8 string (`string`) from the calling
module's root; raises a Luau error if the file is missing or not valid UTF-8.

```luau
local json = host.resource.read("data/layout.json")
local layout = parse(json)
```

## host.resource.exists(rel) {#host-resource-exists}

**Signature:** `host.resource.exists(rel: string) -> boolean`

Whether a file exists under this module's own root, without reading it.

Its use is naming a file that must not overwrite an earlier one: the overlay runtime's calibrator numbers each captured template by counting up until it finds a name nobody has taken, so a second measuring session does not quietly replace the first one's evidence.

```luau
local n = 1
while host.resource.exists(("images/shot-%d.png"):format(n)) do n += 1 end
host.screen.save(("images/shot-%d.png"):format(n), { region = r })
```
