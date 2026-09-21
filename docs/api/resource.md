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

Reads a package-relative file as a UTF-8 string (`string`) from the calling
module's root; raises a Luau error if the file is missing or not valid UTF-8.

```luau
local layout = host.json.decode(host.resource.read("data/layout.json"))
host.log.info("layout has " .. #layout.controls .. " controls")
```

A data file is read by the module that ships it. In a `code_module` dependency, `resource` resolves under the dependency's own root — even while its code runs in your VM — so a game module that feeds a shared runtime reads its own file and hands the runtime the decoded table — see [`host.json.decode`](./json.md#host-json-decode). Handing the runtime a relative path instead would make it read from its own folder.

## host.resource.exists(rel) {#host-resource-exists}

**Signature:** `host.resource.exists(rel: string) -> boolean`

Whether a file exists under this module's own root, without reading it.

Its use is naming a file that must not overwrite an earlier one: the overlay runtime's calibrator numbers each captured template by counting up until it finds a name nobody has taken, so a second measuring session does not quietly replace the first one's evidence.

```luau
local n = 1
while host.resource.exists(("images/shot-%d.png"):format(n)) do n += 1 end
host.screen.save(("images/shot-%d.png"):format(n), { region = r })
```
