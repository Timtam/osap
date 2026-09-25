---
title: "host.path — module-relative paths"
sidebar_position: 11
toc_max_heading_level: 2
---

Turns a name relative to your own module's directory into an absolute filesystem path.

**The absolute part is the point, and it is the mistake authors make.** Every image a control matches against — a landmark, an on/off template, a slider thumb — has to be an absolute path, and the overlay runtime refuses a relative one with an error saying so. The reason is [the path rule](./index.md#paths): a relative path resolves against the root of the module whose code makes the call, and the search is made by the overlay runtime's code — or, for a library inheriting Kontakt, by Kontakt's — so a relative path would be looked for in that module's folder and not in the library's.

It is also what lets an image cross a module boundary at all. Cinematic Studio Series passes `host.path` results into `kontakt.library`, and they still resolve on the other side.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["path"]
```

See [what that list is and is not](./index.md#capabilities).

## host.path(rel) {#host-path}

**Signature:** `host.path(rel: string) -> string`

Resolves a package-relative path to an **absolute** filesystem path string
(`string`), joining `rel` onto the root of the module whose code makes the call
and absolutising it (so the path stays valid even when handed to another module
with a different working directory). It does not check that the file exists, and
it does not confine the result to the module: `"../x"` names a file beside the
module folder, and an absolute `rel` comes back as it is.

String work on the main thread, with no file-system access: microseconds, and it never
blocks. It raises only for a `rel` that is neither a string nor a number (a number is read as
its digits). Folders may be separated by `/` on every platform.

```luau
local img = host.path("assets/kontakt.png") -- "C:\...\modules\my-mod\assets\kontakt.png"
```

### Windows

Making the path absolute also resolves `..` in it, so `host.path("../x")` comes back
without the `..`, naming the file beside the module folder directly. `/` and `\` both
separate folders, and the result is written with `\`.

### macOS

The `..` is left in the returned path, and the file system resolves it when the path
is used. Only `/` separates folders: a `\` is part of a file name here, so a path written
with `\` names a different file than on Windows.
