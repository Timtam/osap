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

> **On macOS this is not yet true of the module list.** wxWidgets draws its tree
> control itself there rather than using a real one, so VoiceOver sees a single
> opaque element and there are no checkboxes to tick. The list is therefore
> read-only for now: enabling and disabling a module happens through the same
> mechanism as before, but not from that list. Everything else in the window —
> the tabs, the buttons, the dialogs, the settings fields — is native and behaves
> as described. See [macos-port.md](macos-port.md).

> Closing the window **hides it to the tray**; modules keep running. Use the
> tray icon's **Quit** to actually exit.

The window's title ends with the version and the build, for example
`Automation Platform — Modules (0.1.0, build 6c95c8b)`, so the build can be heard without
opening the log: on Windows a screen reader reads it whenever the window gets focus. The log
names the same build at the start of every session (`[host] version 0.1.0, build 6c95c8b`);
quote it when you report a problem.

## Installed tab

Lists every loaded module with a native checkbox. Below the list: **Settings…**,
**Reload**, and **Uninstall** (the last two act on the selected module).

### Enable / disable

Unchecking a module disables it **at runtime** — its hotkeys are released at the
OS, its captured keys stop being suppressed, its overlays deactivate (an active
overlay's `onDeactivate` runs once, so it can tear down), and its callbacks
(triggers, timers, keys, controller events) are not called any more — except
`host.settings.onChange`, which still fires when a setting changes. Re-checking
delivers them again. Disabling does not unload the module: its VM and
everything it registered stay, and its entry file does not run again when it is
re-enabled (see [the lifecycle](module-runtime-and-lifecycle.md#lifecycle--states)).
The enabled set is **persisted** in `settings.toml` beside the application (next
to the `.exe` on Windows, in the folder that holds the `.app` on macOS), so a
disabled module stays disabled across restarts — and is still loaded at start-up,
with its callbacks switched off.

### Settings…

Opens a per-module dialog built from the settings the module declared via
[`host.settings.define`](api/settings.md#host-settings-define): a native control per
setting (checkbox / number field / dropdown / text), each labelled for the screen
reader. Changes are validated and persisted; a module can react live via
`host.settings.onChange`. **OK applies every field**, changed or not, so every
setting's `onChange` callbacks fire — and the button works for a disabled module
too, whose `onChange` callbacks then fire as well.

### Reload

Rebuilds the selected module's VM **in place** from its source directory — edit a
module's code and reload it in the running app, no restart, without disturbing the
modules that do not depend on it. Its state starts afresh: the new VM runs the entry
file again. Useful while **developing** a module.

- A broken `module.toml` leaves the running module intact (the manifest is
  re-read before anything is torn down).
- A broken rebuild reports the error and leaves the module inactive until you fix
  it and reload again; persisted settings are preserved.
- Every module that depends on this one through `dependencies` — directly, or
  through other modules — is **rebuilt with it**, after it, in dependency order, so
  a module holding a copy of its code takes up the new one; the result names them.
  A module that reaches it only through `optional_dependencies` keeps the old copy
  until it is reloaded itself.

### Reload everything: Ctrl+Shift+Win+Alt+F5, or Command-Shift-F5 on a Mac

> Not the same chord translated: the Windows one would carry Control-Option on a Mac, which
> is VoiceOver's own modifier and never reaches an application while VoiceOver keeps its
> default setting. Either way it uses an F-key, so on a Mac it needs "Use F1, F2, etc. as
> standard function keys" turned on in System Settings or the `fn` key held down as well.

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

**One repository is one module**, with its `module.toml` at the repository root.
It is installed as a plain folder, `modules/<repository name>/` beside the
application. A pack of many modules cannot be installed from one repository: put
each module's folder directly under `modules/` instead, and it loads at the next
start. A folder there whose `module.toml` does not parse is skipped without a
message. See [where modules are found](module-package-format.md#where-modules-are-found).

Before installing, the module's **requested capabilities** are shown for review
(default-deny — a module only gets what it asks for). Installing pulls the **whole
dependency tree**: each declared dependency is resolved to its repo via the topic
search and fetched too. A freshly installed module is **hot-loaded** into the
running app — usable without a restart.

A module whose manifest declares `supported_os` without this system is **flagged in that
same review** — *"it can be installed, but it will not be loaded here"* — and installed
anyway if you say so. It then names itself in the log at every start instead of loading.
Omitting the field means no claim and no exclusion; see
[module-package-format.md](module-package-format.md).

## Updates tab

**Check for updates** compares each remotely-installed module against its upstream
`module.toml` `version` (semver). An update is offered **only when the version is
bumped** — commits *between* releases don't trigger one. Each entry shows the
transition, e.g. `v0.1.0 → v0.2.0`. **Update selected** re-fetches and reinstalls,
then reloads the updated module together with every module that depends on it
through `dependencies`, directly or not, as **Reload** does; the message names them. (A version that is not
semver on either side falls back to comparing the latest commit.)

## The Installed list is a different control on each platform

Windows uses a native tree with real OS checkboxes (`wxTreeCtrl` + `TVS_CHECKBOXES`); macOS
uses a `wxCheckListBox`, which there is a real `NSTableView` with a checkbox column.

Not a whim. Off Windows, wxWidgets compiles its whole accessibility layer out
(`include/wx/chkconf.h`) and its tree control is a scrolled window that paints its own rows —
so VoiceOver did not read that list badly, it skipped the control entirely. One control for
both platforms was built and tried first: `wxCheckListBox` on Windows is an owner-drawn
listbox, and the accessible wxWidgets supplies for it reports the checkbox correctly but no
item positions and no selected state. With NVDA that was worse than the tree. Giving up the
platform that works to fix the one that does not is the wrong trade, so both stay, behind one
seam that shows the rest of the window nothing but a row index.

## Application settings tab

The platform's own settings — the ones about the application rather than about any module.
On every system:

- **Detailed (trace) logging** — takes effect immediately.
- **Save the images OCR was given** — takes effect immediately.
- **Calibration keys in overlays** — reload modules to apply.
- **Load modules not meant for this system** (their `supported_os` excludes it) — restart to
  apply.
- **Run without a window** — restart to apply.

On Windows only:

- **Speak through the screen reader** (NVDA, JAWS…) instead of a separate voice — on by
  default.
- **Also send what is said to a braille display** — on by default.
- **Let modules that ask for it read the screen through the graphics card** — desktop
  duplication, for the modules that declare `[screen] capture = "duplication"` (see
  [Which picture a read sees](api/screen.md#which-picture-a-read-sees)). On by default; it does
  nothing for a module that did not ask. Turning it off makes those modules read the standard
  way (or get no picture, under `fallback = "none"`); turning it off and on again gives
  duplication another try after it stopped answering.

On macOS only:

- **Speak through VoiceOver** instead of a separate voice.
- **Offer my Personal Voice to modules** — asks macOS for permission when ticked.
- **Show a Dock icon while the module manager is open**, so Command-Tab can reach it — on by
  default.

**A setting that does not exist on this system is not shown.** The VoiceOver one appears on a
Mac and nowhere else, because a checkbox somebody can tick and that then changes nothing is
the same broken promise as a control announcing an action it cannot perform.

A tab rather than a dialog, and **each change applies as it is made**. There is no OK button,
for the same reason the module checkboxes in the Installed list have none: the change is the
action. A modal would mean opening it, changing something, confirming, and only then finding
out what it did; a tab is somewhere you can be, tick something, hear the result, and untick
it.

Every label says **when its setting takes effect** — immediately, after reloading modules, or
after a restart — because a setting that appears to do nothing is worse than one that admits
it needs a restart. Under each checkbox is a sentence saying what the setting is for.
Changes are written to `settings.toml` beside the application, so they survive a restart.

These were environment variables, and a variable is the wrong shape for them: it has to be
decided before the process starts, cannot be changed while it runs, is invisible to anybody
who did not set it, and has to be typed into a terminal to be used at all. Somebody who needs
trace logging for one keystroke should be able to switch it on, do the thing, and switch it
off again.

The variables still work, for the launches that have no window to click in — a CI job running
headless, a tester told to start with tracing on. A variable that is set **forces** its
setting on for that run; its checkbox is shown ticked and disabled, with the reason in its
name, rather than accepting a click it cannot honour. The log's session header lists every
setting that is on and which of the two turned it on.

## Conflict detection

Two modules from different authors can end up wanting the same **global hotkey**.
Only one module can hold a combination at a time, so the clash is surfaced in an
accessible dialog naming **both** modules and the combo — you can see *why* a
hotkey isn't working instead of it silently failing.

Both modules stay loaded, and the one that missed out keeps a standing **claim**.
The module that loaded first holds the combination, and a module that is already
holding one keeps it. **The claim is honoured the moment the combination is free** —
disable, uninstall or reload the module holding it and the hotkey passes over by
itself. Nothing needs restarting. (Before 2026-09-03 it did: a claim that lost was
not recorded at all, so there was nothing left to hand the key to.)

(Captured keys are *not* flagged, and they are not shared either. While any enabled
module captures a key, the hook suppresses it for the whole application — within the
limits [`host.keys`](api/keys.md) describes: the capture scope, an open menu, and on
Windows a held screen-reader modifier let it through — and each press it swallows goes
to **one** callback: the earliest capture of that key still standing
among enabled modules. Nothing routes it by which window is in front, and a module
that captured the same key later simply never hears it — without a dialog. Overlays
avoid the clash by capturing only while they are active; see
[`host.keys`](api/keys.md).)

## Crash protection

A bug in a module — a Luau error or even a Rust panic in one of its callbacks —
**never takes down the app or the other modules**. The faulting callback is
caught and isolated, and the concrete Luau error (with its traceback) is surfaced
in an accessible dialog, attributed to the module, so the author can debug it —
once per module and kind of callback, until the module is disabled and enabled
again. Every occurrence is also written to the log file
(`automation-platform.log` beside the application, or in
`%LOCALAPPDATA%\AutomationPlatform` / `~/Library/Application Support/AutomationPlatform`
when that folder cannot be written). A module that keeps failing is **not**
disabled automatically.

What this does not cover is a callback that never returns. There is no time or
memory limit on a module's code, so a loop that does not end holds the one thread
everything runs on — every module, speech and the keyboard hook — until the
application is ended.

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
