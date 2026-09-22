---
title: "host.require — a declared dependency"
sidebar_position: 12
toc_max_heading_level: 2
---

Returns what a dependency declared in your manifest exports.

For a `code_module` — the overlay runtime, Kontakt, daw-hosts — that dependency's source has already been evaluated **inside your own VM**, so what comes back has its functions intact. This is how every module obtains `O`, and how Cinematic Studio Series inherits Kontakt's framework and calls `kontakt.library`.

**A `code_module`'s source runs more than once.** It is also a module in its own right: it is loaded first, gets its own VM, and its entry file runs there; then the same source is evaluated again inside the VM of every module that depends on it — under `dependencies` or `optional_dependencies`, directly or through another `code_module`. So the overlay runtime also runs in Cinematic Studio Series' VM, which depends on it only through Kontakt. A runtime that three game modules depend on, with nothing depending on them in turn, therefore runs its top level four times, and anything that top level does — registering a hotkey, arming a timer, capturing a key, speaking — happens four times, in four VMs, each belonging to a different module. Four hotkey registrations of one combination are four claims competing with each other (see [`host.hotkey`](./hotkey.md)).

Work that must happen once goes into a function named `activate` in the table the entry returns. The host calls it once, after the entry has run, in the module's **own** VM only — never when the source is evaluated as a dependency. That is how the overlay runtime and Kontakt set up what exists once per application.

```luau
-- The entry file of a code_module runtime.
local M = {}
function M.describe(state) return "menu: " .. state end   -- for dependents: runs in their VM
function M.activate()
  -- Only in the runtime's own VM, once per load.
  host.log.info("[runtime] loaded")
end
return M
```

Each module runs in its own isolated Luau VM, and **how much crosses a module boundary depends on the dependency kind.** A [`code_module`](../module-package-format.md) dependency has its *code* evaluated inside the dependent's VM, so its **functions** are reachable through `host.require` — that is the inheritance model every base and library module is built on. A legacy, non-code dependency exports only plain data (booleans, numbers, strings and tables of those), because raw values, closures and userdata cannot otherwise cross a VM boundary.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.require(id) {#host-require}

Returns the object that dependency `id` exported, where `id` is a module declared
in this module's manifest `dependencies`. Raises an error if `id` is not loaded or
exported nothing — use [`host.tryRequire`](./tryrequire.md#host-tryrequire) for an optional
dependency that may be absent.

- If `id` is a [**`code_module`**](../module-package-format.md), its code was
  evaluated inside *this* VM, so `host.require` returns the live table it returned
  — **functions intact**. You call its functions directly; this is how
  inheritance / library modules build on a base module. Its code keeps its own
  identity while it runs in your VM: relative paths, resources, settings and
  includes resolve against the dependency (see [the path rule](./index.md#paths)),
  and its manifest decides what it may call. Hotkeys, timers, captures and
  callbacks it registers belong to your module, and go when your module is reloaded.
  That includes [`host.settings.onChange`](./settings.md#host-settings-onchange); only
  the setting it watches is the dependency's, so a callback it registers fires when the
  *dependency's* setting changes, and an error in it is reported under the dependency's
  id.
- Otherwise (a legacy data dependency), it returns a fresh Luau value mirroring
  the plain **data** the dependency exported (functions can't cross a VM this way).

```luau
-- a code_module dependency exposing functions:
local kontakt = host.require("com.platform.kontakt")
-- library(name, landmark, build): the landmark must be an ABSOLUTE path, because the search
-- runs in Kontakt's identity and a relative one would resolve under Kontakt's root.
kontakt.library("Cinematic Studio Strings", host.path("images/css.png"), function(ov)
  ov:addStaticText("Cinematic Studio Strings")
end)

-- a legacy data dependency: `return { presets = {...}, version = 3 }`
local lib = host.require("com.example.preset-library")
for _, p in ipairs(lib.presets) do ... end
```
