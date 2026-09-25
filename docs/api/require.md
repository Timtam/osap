---
title: "host.require — a declared dependency"
sidebar_position: 12
toc_max_heading_level: 2
---

Returns what a dependency declared in your manifest exports.

For a `code_module` — the overlay runtime, Kontakt, daw-hosts — that dependency's source has already been evaluated **inside your own VM**, so what comes back has its functions intact. This is how every module obtains `O`, and how Cinematic Studio Series inherits Kontakt's framework and calls `kontakt.library`.

**A `code_module`'s source runs more than once.** It is also a module in its own right: it is loaded first, gets its own VM, and its entry file runs there; then the same source is evaluated again inside the VM of every module that depends on it — under `dependencies` or `optional_dependencies`, directly or through another `code_module`. So the overlay runtime also runs in Cinematic Studio Series' VM, which depends on it only through Kontakt. A runtime that three game modules depend on, with nothing depending on them in turn, therefore runs its top level four times, and anything that top level does — registering a hotkey, arming a timer, capturing a key, speaking — happens four times, in four VMs, each belonging to a different module. Four hotkey registrations of one combination are four claims competing with each other (see [`host.hotkey`](./hotkey.md)).

Work that must happen once per application goes into a function named `activate` in the table the entry returns. The host calls it once, after the entry has run, in the module's **own** VM only — never when the source is evaluated as a dependency. That is how the overlay runtime and Kontakt set up what exists once per application. Work that belongs to each dependent — a game's triggers, polls and keys — goes into a function the dependent calls from its own entry; see [a runtime shared by game modules](#a-runtime-shared-by-game-modules).

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

**Signature:** `host.require(id: string) -> any`

Returns the object that dependency `id` exported, where `id` is a module declared
in this module's manifest `dependencies`. Raises `required module '<id>' is not loaded or
exports nothing` when there is nothing to return — use
[`host.tryRequire`](./tryrequire.md#host-tryrequire) for an optional dependency that may be
absent. **A `code_module` this module does not declare is not evaluated into its VM.**
Requiring one anyway reaches only the legacy data path below, and a runtime whose table holds
functions has no data export there, so that raises the message above although the runtime is
loaded: list it under `dependencies` (or `optional_dependencies`). `id` is a string; a number
is read as its digits, and anything else raises.

**Cost.** For a `code_module`, a lookup in this VM's table of evaluated dependencies, so the
same live table comes back on every call. For a legacy data dependency, the data is converted
into a fresh Luau table on every call, at a cost that grows with its size: call it once and
keep the result. Both on the main thread, like every Lua call.

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

### What a code module must not do {#code-module-rules}

- **No top-level `...`.** In a dependent's VM the entry file becomes the body of a function
  that receives `host`, so `...` at its top level does not compile there. It does compile in
  the module's own VM, so the mistake shows only once a dependent loads — as the dependent's
  load error, `error running dependency '<runtime>' of '<game>'`.
- **Do not return the `host` table** as the exported table. A dependency's host carries its
  own capabilities, and handing it over would hand them to every dependent, so the
  dependent's load fails with `dependency '<dep>' of '<id>' returns its own host table …`.
  Export functions that use `host` instead.
- **Disabling the code module stops none of its dependents.** Each dependent runs its own copy
  of the code, and what that copy registers belongs to the dependent: unticking a shared
  runtime in the module manager stops only what runs in the runtime's own VM (its `activate`),
  and every game built on it goes on. Untick the game modules to stop them.
- **Its settings are one set for every dependent.** A setting the runtime's code defines is
  stored under the runtime's id, whichever game's VM defines it, so every game shares it and
  it appears under the runtime in the module manager. A per-game choice is defined by the game
  module's own entry (see [`host.settings`](./settings.md)).

## A runtime shared by game modules {#a-runtime-shared-by-game-modules}

One reader for many games: a `code_module` runtime holds all the code, and each game is a small module that holds only its data and one call.

Each game module — generated by a converter, say — is the same two lines. The runtime exports `start(pack)`; each game module's entry reads its pack and calls `start`
**once, in its own VM**. So everything `start` registers — the window trigger, the poll, the
hotkey, a controller listener — belongs to that game module: it stops while the game module is
disabled, and it is registered afresh when the game module is reloaded. The runtime's top
level registers nothing, because it also runs in the runtime's own VM, where there is no pack.

The runtime, `menu-runtime/module.toml` (a `[screen]` table here applies to every game built
on it; see [the manifest](../module-package-format.md#screen-and-why-it-is-declared-per-module)):

```toml
id = "com.example.menu-runtime"
name = "Menu reader runtime"
version = "1.0.0"
code_module = true

[capabilities]
require = ["window", "screen", "speech", "timer", "hotkey"]
```

`menu-runtime/src/main.luau`:

```luau
-- Runs in the runtime's own VM and in every game module's VM, so the top level only
-- defines: nothing here registers a trigger, a timer or a key.
local M = {}

-- Called once by each game module's entry, in that game's VM, with that game's pack.
function M.start(pack)
  host.screen.predicate(pack.predicate)       -- a bad pack fails the game's load, here
  local poll, key, since, last

  local function readMenu(w)
    if since and host.now() - since < 1000 then return end   -- one read in flight
    since = host.now()
    host.screen.matchCellsAsync({
      region = { window = w, fraction = pack.region },
      cols = pack.cols, rows = pack.rows, predicate = pack.predicate,
    }, pack.states, function(m)
      since = nil
      local name = m and m.similarity >= pack.minScore
        and m.similarity - m.runnerUp >= pack.margin and m.name or nil
      if name and name ~= last then host.speech.output(name) end
      last = name
    end)
  end

  local function arrive()
    if not poll then
      poll = host.timer.every(100, function()
        local w = host.window.active()
        if w and host.window.test(pack.window, w) then readMenu(w) end
      end)
    end
    -- Claimed on the next pass, not now: the game this one replaces gives the key back in
    -- this same activation, and a claim made before it has would be a conflict.
    host.timer.after(0, function()
      local w = host.window.active()
      if not key and w and host.window.test(pack.window, w) then
        key = host.hotkey.register(pack.repeatKey, function()
          if last then host.speech.output(last) end
        end)
      end
    end)
  end

  local function leave()
    if poll then host.timer.cancel(poll); poll = nil end
    if key then host.hotkey.unregister(key); key = nil end
  end

  -- Every window that comes forward, and the one already in front at load: the game's own
  -- window starts the reader, any other window stops it.
  host.window.onTrigger({}, { initial = true }, function(win)
    if host.window.test(pack.window, win) then arrive() else leave() end
  end)
end

return M
```

A game module, `my-game/module.toml`. It declares only what its **own** entry calls — here
nothing, since `host.require` and `host.include` need no capability; the runtime's code is
judged by the runtime's manifest wherever it runs:

```toml
id = "com.example.my-game"
name = "My Game menu reader"
version = "1.0.0"
dependencies = ["com.example.menu-runtime >= 1.0"]
```

`my-game/src/main.luau`, the same two lines for every game:

```luau
local runtime = host.require("com.example.menu-runtime")
runtime.start(host.include("data/gamepack.luau"))
```

`my-game/data/gamepack.luau`, the converter's output — a file that returns one table:

```luau
return {
  window = { app = { exe = "mygame.exe" } },
  region = { 0.10, 0.20, 0.45, 0.80 },
  cols = 10, rows = 36,
  predicate = "red >= 80 && red*10 >= green*13",
  states = { { name = "Start", cells = "00ff…" }, { name = "Options", cells = "ff00…" } },
  minScore = 0.97, margin = 0.01,
  repeatKey = "Ctrl+Shift+F11",
}
```

The pack can be JSON instead — `runtime.start(host.json.decode(host.resource.read("data/pack.json")))`
— and the game module then declares `resource` for its own `host.resource.read`. Read it in the
game module, not in the runtime: a relative path in the runtime's code resolves under the
runtime's folder ([the path rule](./index.md#paths)). A converter may write either file with a
UTF-8 byte-order mark; the host skips one at the start of a `.luau` file, of `module.toml`, and
of the text `host.json.decode` is given.

**Keys with many games on one runtime.** A combination can be held by only one registration at
a time, and every game module's VM runs its own `start`, so registering the key in `start`
unconditionally makes one claim per game. The game module that loaded first then holds it — its
callback runs, in its own VM with its own pack, whichever game is in front — and every other
game module opens a **Binding conflict** dialog at start naming the two modules — once, until
that module is disabled and enabled again (see
[a registration is a claim](./hotkey.md#a-registration-is-a-claim-not-a-guarantee)).
Between modules the host's rule decides: a module that holds a combination keeps it, and
otherwise the one that loaded first gets it. The overlay runtime's press-time choice does not
help here — it picks among the controls of one overlay, in one module's VM, and games are
separate modules.

What works for any number of games is the pattern above: hold the key only while the game is
in front. `onTrigger({}, …)` hears every window that comes forward, and one activation is
handed to every enabled module, in load order, before any timer runs. So the game being left
releases its key during the activation, and the game arriving claims it from
`host.timer.after(0, …)`, which runs on a later pass of the loop, once every module has been
told. Claiming inside the trigger would find the key still held whenever the arriving game
loaded before the one being left, and put up the conflict dialog; the key would still pass to
it a moment later, when the other game let go. The window already in front at start is handled
the same way, through `initial = true`. A key the user chooses per game — a string setting the
game module defines, see [`host.settings`](./settings.md) — avoids the contest as well, as long
as no two games are given the same one.

The same goes for [`host.keys.capture`](./keys.md#host-keys-capture): several captures of one
key open no dialog, but each press goes to the earliest capture still standing, so capture
while the game is in front and release when it leaves. A
[controller listener](./gamepad.md#host-gamepad-on) does not look at which window is in front
either: add it in `arrive` and remove it with `off` in `leave`, or test the window inside it.
