---
title: "host.path — module-relative paths"
sidebar_position: 11
toc_max_heading_level: 2
---

Turns a name relative to your own module's directory into an absolute filesystem path.

**The absolute part is the point, and it is the mistake authors make.** Every image a control matches against — a landmark, an on/off template, a slider thumb — has to be an absolute path, and the overlay runtime refuses a relative one with an error saying so. The reason is the identity scope: the search runs as whichever module is running, which for a library inheriting Kontakt is Kontakt's root and not the library's.

It is also what lets an image cross a module boundary at all. Cinematic Studio Series passes `host.path` results into `kontakt.library`, and they still resolve on the other side.

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
