---
title: "host.resource — bundled files"
sidebar_position: 13
toc_max_heading_level: 2
---

Reads a file bundled inside your own module, and asks whether one is there.

For manifests and data files rather than for images: `read` decodes as UTF-8 and raises when the file is missing or is not text, so a PNG is an error rather than a string. Paths are relative to the root of the module whose code makes the call — see [the path rule](./index.md#paths).

**Read-only and text-only.** These two calls are all there is: nothing reads a file as bytes, and nothing writes one (the only files a module can write are the PNGs of [`host.screen.save`](./screen.md#host-screen-save)). Persist a value with [`host.settings`](./settings.md), which holds booleans, numbers and strings; ship binary data such as template pixels as JSON numbers or a Luau table, and decode it with [`host.json.decode`](./json.md#host-json-decode) or `host.include`.

**Not a sandbox.** The path is joined onto the module's root as written: `..` is not removed and an absolute path is used as it stands, so either reaches any file the user can read. Treat a path taken from a data file as you would any other path.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["resource"]
```

See [what that list is and is not](./index.md#capabilities).

## host.resource.read(rel) {#host-resource-read}

**Signature:** `host.resource.read(rel: string) -> string`

Reads a package-relative file from the calling module's root and returns it as a UTF-8 string; raises a Luau error if the file is missing or not valid UTF-8.

```luau
local layout = host.json.decode(host.resource.read("data/layout.json"))
host.log.info("layout has " .. #layout.controls .. " controls")
```

**Cost.** The whole file is read synchronously, on the main thread, every time: nothing is cached, and every callback, hotkey and speech line waits until the read has finished. Read a data file once, at load, and keep what it gives.

**What raises.** The error is the operating system's own message, with no path in it — `The system cannot find the file specified. (os error 2)` on an English Windows (in the language of Windows elsewhere), `No such file or directory (os error 2)` on macOS, `stream did not contain valid UTF-8` for a file that is not text — so wrap the call in `pcall` and name the file yourself when the message is for a person. A `rel` that is neither a string nor a number raises too.

**The text comes back exactly as it is on disk**, a byte-order mark at the start included. [`host.json.decode`](./json.md#host-json-decode) skips one; code of your own that compares the first characters of the text has to skip it itself.

A data file is read by the module that ships it. In a `code_module` dependency, `resource` resolves under the dependency's own root — even while its code runs in your VM — so a game module that feeds a shared runtime reads its own file and hands the runtime the decoded table — see [`host.json.decode`](./json.md#host-json-decode). Handing the runtime a relative path instead would make it read from its own folder.

### Windows

`/` and `\` both separate folders, and names are compared without case, as the file system does: `"Data\\Pack.json"` (a Luau string, so the `\` is written twice) finds `data/pack.json`. The same holds for `exists`.

### macOS

Only `/` separates folders. A `\` is part of a file name here, so `"data\\pack.json"` looks for a file of that name in the module's root, does not find `data/pack.json`, and raises; write `/` for code that runs on both. Whether names are compared without case depends on the volume: the default one does, a case-sensitive one does not. The same holds for `exists`, which answers `false` for such a path.

## host.resource.exists(rel) {#host-resource-exists}

**Signature:** `host.resource.exists(rel: string) -> boolean`

Whether something exists at `rel` under this module's own root, without reading it. **A folder counts**: `host.resource.exists("images")` is `true` when there is an `images` folder, so a check for a file whose name could also be a folder's cannot tell the two apart. `false` also when the answer cannot be had — a path the system refuses to look at, say — and the call never raises for that; it raises only for a `rel` that is neither a string nor a number.

One question to the file system, on the main thread; usually microseconds, and never a read of the file.

Its use is naming a file that must not overwrite an earlier one: the overlay runtime's calibrator numbers each captured template by counting up until it finds a name nobody has taken, so a second measuring session does not quietly replace the first one's evidence.

```luau
local n = 1
while host.resource.exists(("images/shot-%d.png"):format(n)) do n += 1 end
host.screen.save(("images/shot-%d.png"):format(n), { region = r })
```
