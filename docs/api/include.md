---
title: "host.include — another file of your own module"
sidebar_position: 5
toc_max_heading_level: 2
---

Loads another file of the module you are writing and returns whatever it returns.

This is how a module becomes more than one file. ON:EAR splits into the application it lives in, its coordinate geometry, its tree reader, and a file each for the browser and settings panels. Every included file is evaluated once per VM and handed the **same `host`** as the file that included it, which is what keeps a code module's own files resolving against its own root rather than the dependent's.

Not to be confused with [`host.require`](./require.md#host-require), which imports a different module by id.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.include(rel) {#host-include}

**Signature:** `host.include(rel: string) -> any`

Loads **another file of this module** and returns whatever that file returns — a table of functions, a table of constants, an overlay, anything. This is how a module becomes more than one file: shared helpers, reference data, one overlay per file.


```luau
-- src/geometry.luau
return { GEOMETRY = { … }, ARROWS = { … } }

-- src/main.luau
local geo = host.include("src/geometry.luau")
for _, a in ipairs(geo.ARROWS) do … end
```

Semantics worth knowing:

- The included file sees the **same `host`** as the file that included it, handed in rather than read from the globals. So inside a **code module** — whose source is evaluated in each dependent's VM — an included file resolves paths, settings and resources against the **defining** module, exactly as its includer does.
- Executed **once per VM**; further includes of the same file return the same value. A module split across files must not re-run side effects per include. (A code module still runs once in its own VM and once in the VM of every module that depends on it, directly or through another code module, and so do its includes.)
- Paths are relative to the root of the module whose code makes the call — for a code module, its own root (see [the path rule](./index.md#paths)). Folders are separated by `/`. The path is tidied **as text** first — `.` parts dropped, each `..` removing the part before it — and must then still lie inside the module: `"../other/x.luau"`, `"src/../../x.luau"`, an absolute path (even one that points into the module), and an empty path **raise** `include '<rel>' resolves outside the module directory`. A `\` or a `:` anywhere in the path raises too, on every platform — `include '<rel>': '\' is not allowed in an include path — separate folders with '/'` — because on Windows they are a separator and a drive, and a path must mean the same thing everywhere. The check is there because this executes code. It is a check of the path **as text**, and the file system is not asked: a symbolic link (or, on Windows, a junction) inside the module that points elsewhere is followed, and the file it leads to runs.
- Spellings that tidy to the same path are **one include**: `"src/a.luau"`, `"./src/a.luau"`, `"src//a.luau"` and `"src/../src/a.luau"` run the file once and return the same value.
- Luau's own `require` is not a way to split a module: every file here is loaded under a name `require` does not accept, so a `require("./lib")` raises `require is not supported in this context`. Use `host.include`.
- An include **cycle** raises an error naming the file rather than overflowing the stack.
- Reported line numbers match the file.
- **What raises**, besides the path rules above: a file that cannot be read raises `include '<rel>': <the system's reason>` — `include 'data/pack.luau': The system cannot find the file specified. (os error 2)` on an English Windows, in the language of Windows elsewhere, `… No such file or directory (os error 2)` on macOS — and so does a file that is not UTF-8 (`stream did not contain valid UTF-8`); a syntax error in the file raises Luau's message with the file's line; an error the file raises while it runs comes back out of `host.include` as it is. A failed include is not remembered: the next call tries the file again. An argument that is neither a string nor a number raises.
- **Text encoding.** The file is UTF-8. One byte-order mark at its very start is skipped, so a file written by a tool that puts one there (.NET's `Encoding.UTF8`, PowerShell 5's `Out-File -Encoding utf8`) loads; the same holds for a module's entry file and a code dependency's. A second mark, or one anywhere else, is left for Luau, which rejects it (`Unicode character U+feff`).
- **Cost.** The first include of a file in a VM reads it and compiles it synchronously, on the main thread, and then runs it; the time grows with the file, and a module's entry file waits for it. Later includes of the same file in that VM are a table lookup. A code module's files are read and compiled once in each VM its code runs in.

### Windows

The include is remembered by its tidied path as text, so spellings the file system treats as one file but the text does not — `"Src/A.luau"` beside `"src/a.luau"`, or a short 8.3 name — are two includes, and the file runs twice in one VM.

### macOS

Checked the same way as on Windows: the path is tidied as text before it is compared, so `"../other/x.luau"` is rejected here too. The file system is usually case-insensitive, so as on Windows `"Src/A.luau"` and `"src/a.luau"` are two includes of one file.
