---
title: "host.path & host.resource — module files"
sidebar_position: 10
toc_max_heading_level: 2
---

`host.path` turns a name relative to your own module's directory into an absolute one; `host.resource.read` hands you a bundled text file as a string.

**The absolute part is the point, and it is the mistake authors make.** Every image a control matches against — a landmark, an on/off template, a slider thumb — has to be an absolute path, and the overlay runtime refuses a relative one with an error saying so, because the search runs in the identity scope of whichever module is running: for a library inheriting Kontakt that is Kontakt's root, not the library's. It is also what lets an image cross a module boundary at all, since Cinematic Studio Series passes `host.path` results into `kontakt.library` and they still resolve on the other side.

`host.resource.read` decodes as UTF-8 and raises when the file is missing or is not text, so it is for manifests and data files rather than for the PNGs.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["path"]
```

See [what that list is and is not](./index.md#capabilities).

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

## host.resource.exists(rel) {#host-resource-exists}

**Signature:** `host.resource.exists(rel: string) -> boolean`

Whether a file exists under this module's own root, without reading it.

Its use is naming a file that must not overwrite an earlier one: the overlay runtime's calibrator numbers each captured template by counting up until it finds a name nobody has taken, so a second measuring session does not quietly replace the first one's evidence.

```luau
local n = 1
while host.resource.exists(("images/shot-%d.png"):format(n)) do n += 1 end
host.screen.save(("images/shot-%d.png"):format(n), { region = r })
```
