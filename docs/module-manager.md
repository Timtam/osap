---
title: Module Manager
sidebar_position: 3
---

# Module Manager

The platform runs as a **system-tray app**: a single resident process that hosts
every installed module (one Luau VM each) and shares one event loop. The tray
icon opens a native, screen-reader-friendly window for managing modules — install,
enable/disable, configure, update, reload, and uninstall — without restarting.

All controls are **native OS controls** (real checkboxes, list, buttons, dialogs)
so a screen reader (NVDA/JAWS) exposes their state and labels correctly. Dialogs
are read silently by the screen reader — the app never speaks over it.

> Closing the window **hides it to the tray**; modules keep running. Use the
> tray icon's **Quit** to actually exit.

## Installed tab

Lists every loaded module with a native checkbox. Below the list: **Settings…**,
**Reload**, and **Uninstall** (the last two act on the selected module).

### Enable / disable

Unchecking a module disables it **at runtime** — its hotkeys, captured keys, and
window triggers are revoked immediately, its overlays deactivated. Re-checking
re-registers them. The enabled set is **persisted** (next to the executable, in
`settings.toml`), so a disabled module stays disabled across restarts.

### Settings…

Opens a per-module dialog built from the settings the module declared via
[`host.settings.define`](api/resource-settings-modules.md): a native control per
setting (checkbox / number field / dropdown / text), each labelled for the screen
reader. Changes are validated and persisted; a module can react live via
`host.settings.onChange`.

### Reload

Rebuilds the selected module's VM **in place** from its source directory — edit a
module's code and reload it in the running app, no restart, without disturbing the
other modules. Useful while **developing** a module.

- A broken `module.toml` leaves the running module intact (the manifest is
  re-read before anything is torn down).
- A broken rebuild reports the error and leaves the module inactive until you fix
  it and reload again; persisted settings are preserved.
- Other modules that **inherit this module's code** (`code_module` dependents)
  keep their old copy until restarted — the reload dialog names them.

### Reload everything: Ctrl+Shift+Win+Alt+F5

The same rebuild for **every** loaded module at once, on a **system-wide** key — it
works from inside the plugin you are testing, so you never have to leave it, find the
tray, and come back. The result is **spoken** ("11 modules reloaded", or the names of
the ones that failed), because the window you are looking at is not ours and there is
nothing on screen to check.

Modules are rebuilt **dependencies first**: a module copies its code dependencies'
functions when it is built, so the other order would hand a module the old code and
the change would only appear after a restart — the exact thing this key exists to
avoid. One module failing does not stop the rest; it is named in the announcement and
the reason is in the log.

The combination is awkward on purpose: it rebuilds every VM in the application, and it
has to be impossible to hit by accident while working in a plugin. If something else on
the system already owns it, the app logs that at startup and carries on without it.

### Uninstall

Removes the selected module's files and disables it immediately. **Dependency-safe:**
if another loaded module (transitively) depends on it, the uninstall is blocked
and names the dependents. After a successful uninstall you're offered to also
remove dependencies it pulled in that nothing else needs (orphan cleanup).

## Browse tab

Searches the public module ecosystem — GitHub repositories tagged with the module
topic — and installs by `owner/repo`. There is **no central registry**; discovery
is by topic (HFS-style).

Before installing, the module's **requested capabilities** are shown for review
(default-deny — a module only gets what it asks for). Installing pulls the **whole
dependency tree**: each declared dependency is resolved to its repo via the topic
search and fetched too. A freshly installed module is **hot-loaded** into the
running app — usable without a restart.

## Updates tab

**Check for updates** compares each remotely-installed module against its upstream
`module.toml` `version` (semver). An update is offered **only when the version is
bumped** — commits *between* releases don't trigger one. Each entry shows the
transition, e.g. `v0.1.0 → v0.2.0`. **Update selected** re-fetches and reinstalls;
modules that inherit an updated module's code pick up the change on the next start
(the message names them — *restart to apply*).

## Conflict detection

Two modules from different authors can end up wanting the same **global hotkey**.
When that happens, the clash is detected at registration and surfaced in an
accessible dialog naming **both** modules and the combo — so you can see *why* a
hotkey isn't working, instead of it silently failing. First-come keeps the hotkey;
the later module still loads (with that one binding inactive). Disable one of them
(then restart) to switch. (Captured keys are *not* flagged — window-scoped
overlays legitimately share keys like Tab/Enter for their own windows.)

## Crash protection

A bug in a module — a Luau error or even a Rust panic in one of its callbacks —
**never takes down the app or the other modules**. The faulting callback is
caught and isolated, and the concrete Luau error (with its traceback) is surfaced
in an accessible dialog, attributed to the module, so the author can debug it. The
same error is also written to the log file (`automation-platform.log`, next to the
executable).

## Headless mode

Setting `AUTOMATION_PLATFORM_HEADLESS=1` runs without the tray window — it loads
the given modules and (for a module with hotkeys/keys/triggers) runs the event
loop, useful for testing/automation. Module activity is written to the log file
rather than the screen. A module with no triggers loads and exits; a trigger
module runs until the process is killed.

## CLI

A few operations are also available from the command line (they act on the
portable modules directory, not a running instance):

```text
automation-platform search <query>      # search the module topic on GitHub
automation-platform install <owner/repo># install + its dependency tree (capability review)
automation-platform list                # list installed modules
automation-platform update              # update modules whose version was bumped
automation-platform uninstall <id>      # dependency-safe uninstall + orphan cleanup
automation-platform <dir> [<dir> …]     # run the given module directories (dev)
```
