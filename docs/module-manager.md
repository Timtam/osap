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

## Starting it again brings the window forward

Only one copy runs per user (on Windows, per user and session). Starting the application
while it is already running does not start a second copy: the running one shows its module
window and brings it to the front, and the new start ends without writing anything to the log.
The running copy logs `[instance] another start of the application asked for the module
window; showing it` (on Windows with the new start's process id after "application", as
`(pid …)`). This holds for any second start, from any folder: two copies in two folders would
still fight over the same hotkeys and keyboard hook.

- On Windows, when one of the manager's messages or dialogs is open at that moment, the dialog
  is brought to the front rather than the window behind it, so the keyboard lands where an
  answer is expected.
- A copy that is shutting down says so from the moment **Quit** is chosen. The new start waits
  for it, up to 10 seconds, and then starts normally.
- When the new start cannot show the running copy's window, it does not start either. It shows
  a message saying why, and leaves one line in the running copy's log, marked `[instance] a
  second copy of the application (pid …) did not start`, with the reason and the lock. The
  message is a system message box, read by the screen reader when it appears; nothing is
  spoken. The reasons:
  - the running copy did not respond for 10 seconds (the message says how to end it: Task
    Manager on Windows, Activity Monitor on a Mac);
  - it was still shutting down after 10 seconds (start it again in a moment);
  - it is a different version that does not understand the request (this is said at once,
    without the wait; quit it from its tray icon or menu-bar item first);
  - on Windows, it could not be confirmed to be the same user's copy (see the next point).
- A copy started as administrator (Windows) and one started normally are still one copy:
  - Started as administrator while a normal copy runs, the start is handed to the normal copy,
    which shows its window. To run as administrator, quit the running copy first.
  - Started normally while a copy runs as administrator, the start finds the administrator
    copy's lock, although Windows does not let it open that lock. It hands over when Windows
    lets it confirm that the running copy is the same user's; otherwise it waits 10 seconds and
    ends with the message that another copy is running that it could not confirm as yours. It
    never starts a second host beside it.
- On macOS, opening the `.app` again from the Finder, Launchpad, Spotlight or the Dock never
  starts a second copy in the first place; macOS tells the running one instead, which shows
  its window the same way. The guard matters for the bare binary (`run-dev.sh`) and for
  anything that starts the executable inside the bundle directly.
- Headless runs are not part of this. They neither take the lock nor ask for the window, so a
  headless run and a windowed one can run side by side; see [Headless mode](#headless-mode).
- Neither is a build from before this rule: it takes no lock and looks for none, so it runs
  beside a newer copy, started before it or after, and the two compete for hotkeys as two
  programs would. Quit it from its tray icon.

How it works: the lock is wxWidgets' single-instance checker (a named mutex on Windows, a lock
file on macOS), and the request travels over a named pipe (Windows) or a Unix-domain socket
(macOS). The names contain the user's SID and session id (Windows) or user id (macOS). The
Windows pipe admits only that user and the system, and the new start checks, before it asks
anything, that the copy answering runs as the same user in the same session. The macOS lock
file and socket are in `~/Library/Application Support/AutomationPlatform` (in the user's
temporary folder only when that path would be too long for a socket), and each side checks the
other's user id. A lock file left behind by a crash is removed by wxWidgets at once when the
process it names no longer exists. When that process id has since been reused by another
program, the lock file is taken over after 10 seconds in which nothing answered, provided the
process is not running this application.

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

Only a module that is already loaded can be reloaded: Reload never looks for new folders
under `modules/`. A module folder copied in by hand is loaded at the next start after
**Quit**; one installed from the Browse tab is loaded at once.

- A broken `module.toml` leaves the running module intact (the manifest is
  re-read before anything is torn down).
- A broken rebuild reports the error and leaves the module inactive until you fix
  it and reload again; persisted settings are preserved.
- Every module that depends on this one through `dependencies` — directly, or
  through other modules — is **rebuilt with it**, after it, in dependency order, so
  a module holding a copy of its code takes up the new one; the result names them.
  A module that reaches it only through `optional_dependencies` keeps the old copy
  until it is reloaded itself.

### Reload everything: Ctrl+Alt+Shift+Win+F5, or Command-Shift-F5 on a Mac

> Not the same chord translated: the Windows one would carry Control-Option on a Mac, which
> is VoiceOver's own modifier and never reaches an application while VoiceOver keeps its
> default setting. So the host picks one chord per platform, `Ctrl+Shift+Win+Alt+F5` and
> `Cmd+Shift+F5` in the [key spec](api/keys.md#key-spec-string-format), and its log names it
> `Ctrl+Alt+Shift+Win+F5` on Windows and `Shift+Cmd+F5` on a Mac. Either way it uses an
> F-key, so on a Mac it needs "Use F1, F2, etc. as standard function keys" turned on in
> System Settings or the `fn` key held down as well.

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
each module's folder directly under `modules/` instead, then **Quit** from the tray
icon and start the application again — the folders load at that start. Closing the
window only hides it, and starting the application while it runs only brings the
window forward ([above](#starting-it-again-brings-the-window-forward)), so neither
reads the new folders; nor does **Reload**, which rebuilds modules already loaded. A
folder there whose `module.toml` does not parse is skipped, with a line in the log
saying why; one whose name starts with `.` is skipped without one. When two folders
carry the same module id, the first one the file system lists is loaded and the other
is skipped without a line in the log — keep one folder per id. See
[where modules are found](module-package-format.md#where-modules-are-found).

**Search** follows GitHub's result pages, 100 results each, up to GitHub's own ceiling of
1000 results. What you type is sent as written — `c++` and `a & b` are searched for, not
mangled — and narrows the module topic.

**Install selected** first works out everything the install would **add**, and writes
nothing while it does: the module you chose and, recursively, every dependency it declares
that is not installed yet. Each dependency id is resolved to a repository through the topic
search — the most-starred repository that declares that id wins; its manifests are read in
star order until the id turns up, and a further page of results is fetched only when the ones
already fetched are used up — and the chosen module's **optional dependencies** are resolved
too, with what they need. The status line says *Checking what owner/repo needs…* meanwhile. A
required dependency that cannot be found fails the install there, with its id in the message;
nothing has been written.

A module is installed into the folder named after its repository, so two modules can ask for
one folder. The plan is refused — before the review — when a module it adds would go into a
folder that holds an installed module with another id, or when two of its modules would share
a folder; folder names are compared without case, as Windows and macOS compare them. An
optional module in that position is left out and named, with the reason, instead. The message
names the module in the way: uninstall it first. A folder under `modules/` that is not a
readable module at all — somebody's work in progress, say — is never replaced either; the
install stops before anything is downloaded and asks for the folder to be moved away.

Then a **review** dialog lists every module the install adds, numbered, each with its name,
version, id, the repository it comes from, why it is there (*the module you chose*, *needed
by …*, *optional for …*) and, one per line, what its **capabilities** let it do — *read text
on the screen (ocr)*, *speak through your screen reader (speech)*, and so on; a module that
asks for none says so. The text is in a read-only field that has the focus when the dialog
opens, so the screen reader reads it at once, and it can be read again line by line. Optional
modules are listed in their own section after the required ones, and an optional dependency
that cannot be installed is named with the reason. The buttons are **Install** and
**Cancel** — or, when there are optional modules, **Install with the optional modules**,
**Install without them** and **Cancel**. Escape and the close box cancel, and no button is
the default, so Enter in the text installs nothing. Cancelling leaves nothing on disk. The
modules already running keep running while the review is open; an error one of them hits
meanwhile is held back and shown when the review closes, so the error window does not take
the focus away from the text being read.

What a module's author wrote — its name, the systems it claims, a capability name the
application does not know, and a reason an optional module cannot be installed — is shown on
one line: line breaks, other control characters and text-direction controls become spaces, and
it is cut to at most 100 characters (300 for a reason). A name cannot add lines of its own to
the review.

Each module is **pinned** to the commit whose `module.toml` the review showed, and that commit
is what is downloaded — a repository that changes between the review and the download does not
change what is installed. Dependencies are written before the module that needs them, so a
download that fails part-way leaves nothing installed that cannot load — though the
dependencies already written stay installed.

Each module is unpacked into a staging folder beside the others, `modules/.staging-…`, and
checked there: it must be a readable module, and its `module.toml` must equal the one reviewed
in every field. Only then is the folder of the same name, if there is one, moved aside and the
new one moved into its place; the old one is deleted with the staging folder. So a module that
fails anywhere on the way — a download that breaks off, an archive that breaks a rule, a
manifest that differs — leaves nothing of itself behind, and the version installed before it,
if any, exactly as it was; the message says *Nothing in modules/… was changed*. A staging
folder left behind — by a crash, or because a file in it could not be deleted, which the log
then names — is skipped at start-up like every folder whose name starts with `.`, and can be
deleted by hand. A freshly installed module is **hot-loaded** into the running app — usable
without a restart — and its new dependencies with it.

A module whose manifest declares `supported_os` without this system is **flagged in that
same review** — *"it can be installed, but it will not be loaded here"* — and installed
anyway if you say so. It then names itself in the log at every start instead of loading.
Omitting the field means no claim and no exclusion; see
[module-package-format.md](module-package-format.md).

The archive is unpacked by the application's own loop, not by a general unzip: every name in
it must be a plain relative path inside the repository's one folder that Windows can create —
no `..` that leads out of it, no drive, no absolute path, no `:` or Windows device name, no
two names that are one file when case is ignored — and it may have at most 10,000 members,
which together may unpack to at most 256 MB. The rules are listed in
[module-package-format.md](module-package-format.md#archives).

Every request to GitHub gives up rather than hang: 15 seconds each to look the name up, to
connect and to send the request, 30 seconds for GitHub to start answering, and 60 seconds for
a whole answer to arrive — 10 minutes for a module's archive; one that takes longer to arrive fails.
A stalled connection ends in an error message, and the buttons come back. The limits are
fixed; there is no setting for them.

## Updates tab

**Check for updates** compares each remotely-installed module against its upstream
`module.toml` `version` (semver). An update is offered **only when the version is
bumped** — commits *between* releases don't trigger one. Each entry shows the
transition, e.g. `v0.1.0 → v0.2.0`. (A version that is not semver on either side falls back
to comparing the latest commit.)

**Update selected** first compares the new version with the installed one. When the new
version asks for a **capability** the installed one did not, starts using a module it did not
— as a dependency or an optional dependency that is already installed, whose code will then
run for it — or needs a module that is **not installed yet**, a review dialog shaped like the
install review lists exactly that: the new capabilities, the modules it starts using with
their capabilities, and each module to be installed with it. **Update** applies it; **Cancel**,
Escape and the close box change nothing. An update that asks for nothing new is applied
without a question. Capabilities or dependencies the new version *drops* are not asked about.

Like an install, the update is pinned to the commit the comparison read, and the modules it
newly needs are installed and hot-loaded before the updated module is reloaded — together with
every module that depends on it through `dependencies`, directly or not, as **Reload** does;
the message names them.

An update that fails leaves the installed version in place, as a failed install does (see the
staging folder above). On Windows a folder cannot be moved while a file in it is open, so an
update fails, changing nothing, while the running module holds one of its files open — a
sound it is playing, say; it can be tried again once that is over.

A module installed from a branch whose name holds characters a URL does not carry as they
are — `fix#12`, say — is checked for updates with the branch percent-encoded in the address.

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
everything runs on — every module, speech, every key and hotkey callback, and on
macOS the event tap — until the application is ended. (The Windows keyboard hook
has a thread of its own, so typing elsewhere goes on, and captured keys stay
swallowed, their callbacks never coming.)

## Headless mode

Setting `AUTOMATION_PLATFORM_HEADLESS=1` runs without the tray window — it loads
the given modules and (for a module with hotkeys/keys/triggers) runs the event
loop, useful for testing/automation. Module activity is written to the log file
rather than the screen. A module with no triggers loads and exits; a trigger
module runs until the process is killed.

A headless run is exempt from the [one-copy rule](#starting-it-again-brings-the-window-forward):
CI and the development tools start one beside a running application on purpose. It does not
stop a windowed copy from starting and is not stopped by one, and its log says
`[instance] headless: not guarded against a second copy, and not visible to one`. Two hosts
side by side still compete for any hotkey they both register, so a headless tool should not
claim a combination the running application's modules use.

## CLI

A few operations are also available from the command line (they act on the
portable modules directory, not a running instance):

```text
automation-platform search <query>      # search the module topic on GitHub
automation-platform install <owner/repo># review every module it adds, then install them
automation-platform list                # list installed modules
automation-platform update              # update bumped modules; asks when one asks for more
automation-platform uninstall <id>      # dependency-safe uninstall + orphan cleanup
automation-platform <dir> [<dir> …]     # run the given module directories (dev)
```

The last line starts the application with those folders instead of the installed modules, and
only when no copy is running: with one in the tray, starting the application without one of
the commands above — with folders or without — brings that copy's window forward and ends, and
the folders are ignored ([one copy](#starting-it-again-brings-the-window-forward)). A headless
start (`AUTOMATION_PLATFORM_HEADLESS=1`) loads its folders beside the running copy.

The `search`, `install`, `list`, `update` and `uninstall` commands are not starts of the
application: they run and end beside a running copy, and do not ask it anything. What they
change in the modules folder reaches the running copy only when it next starts — it goes on
with the modules it loaded, and does not load one installed this way until then.
