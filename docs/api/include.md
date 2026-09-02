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
- Executed **once per VM**; further includes of the same file return the same value. A module split across files must not re-run side effects per include. (A code module still runs once per dependent VM, and so do its includes.)
- Paths are relative to the module root. `..` and absolute paths are **rejected** — this executes code, so escaping the module directory is not a nuisance but a hole.
- An include **cycle** raises an error naming the file rather than overflowing the stack.
- Reported line numbers match the file.
