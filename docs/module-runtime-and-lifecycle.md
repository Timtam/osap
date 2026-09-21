# Module Runtime & Lifecycle

*Drafted 2026-06-21, brought in line with the host 2026-09-21. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md) (§3 process/isolation model) and [host-api-capability-catalog.md](host-api-capability-catalog.md).*

> **Status:** the model below is built — see the [Module Manager](module-manager.md) guide for
> what the user sees (enable/disable, per-module settings, install/browse, version-based
> updates, hotkey-conflict detection, in-place reload, crash reporting). Where the original
> draft promised more than was built — a memory-limited, interruptible sandbox, a VM dropped on
> disable, automatic disabling of a faulting module, a scriptable control surface — this page
> now says what happens instead. The CLI offers search/install/list/update/uninstall;
> enable/disable/reload are in the manager window (and a system-wide key reloads everything),
> not a scriptable IPC.

## Principle

**One platform process hosts many modules concurrently — not one OS process per module.** Every loaded module lives in the same process; the host multiplexes events (hotkeys, captured keys, window triggers, timers, image results, controller events) to them on one thread. Modules can be enabled and disabled at runtime.

## Why not process-per-module

- **Hot-path latency:** the global hotkey / window-trigger dispatch must live in one place (the event loop) for low latency. Fanning every event out to N processes via IPC is the wrong shape (feasibility study §3 trade-off).
- **Cost:** each process means its own memory + VM + startup latency; dozens of small modules would be wasteful.
- **What it gives up:** isolation. The draft assumed the Luau VM would contain a misbehaving module on its own; the host sets **no memory limit and no interrupt** on any VM, so it does not (see [Runtime model](#runtime-model)). The capability list catches a module reaching for a namespace by accident; it is not a boundary against a module that is written to misbehave.

## Runtime model

- **One thread.** The event loop runs on the main thread: the Win32 message loop, or the macOS run loop, driven by a 15 ms tick. Every module callback, every host call, hotkey dispatch and the keyboard hook share it. The asynchronous image searches (`imageSearchAsync`, `imageSearchEach`) capture and match on a worker thread, and speech engines, desktop duplication and the controller reader have threads of their own; everything else a module calls — OCR, pixel reads, profiles, the synchronous searches, accessibility queries — runs on the event loop and holds it until it returns.
- **One Luau VM per module**, all in the same process: globals and state are separate between modules, and each can be enabled and disabled on its own.
- **Nothing interrupts a callback.** No VM has a memory limit or an instruction-count interrupt. A callback that loops without end — or runs long, such as computing statistics over thousands of pixels in Luau — stalls the whole application: every module, speech, and the keyboard hook, which on Windows lets keys past once it is held for about 300 ms. Keep callbacks short, and leave pixel work to the host's reductions ([`profile`](api/screen.md#host-screen-profile), image search).
- **Errors are contained, not acted on.** A Luau error or a Rust panic in a callback is caught; the rest of the application goes on. The error is written to the log every time and shown in an accessible dialog once per module and kind of callback (again after the module is disabled and enabled). Nothing disables a module that keeps failing.
- Loading a module runs its entry file, which **registers** hotkeys, captured keys, window triggers, overlays, timers and listeners with the host. The host keeps them keyed by owning module, so each can be revoked or skipped per module.
- **A `code_module` runs more than once.** It is loaded as a module in its own right, with its own VM, and its source is evaluated again inside the VM of every module that depends on it, directly or through another `code_module`; each copy's registrations belong to the VM it ran in. Work meant to happen once goes into an `activate` function in the table the entry returns — see [`host.require`](api/require.md).

## Lifecycle / states

- **Installed** — a folder under `modules/` beside the application (see [where modules are found](module-package-format.md#where-modules-are-found)).
- **Loaded** — its VM built and its entry file run, then the returned `activate` (if any). Every installed module that its `supported_os` allows is loaded at start-up, **disabled ones included**.
- **Enabled** — the user wants it active; persisted in `settings.toml` beside the application. Its callbacks are delivered.
- **Disabled** — still loaded. Its OS hotkeys are released (and may pass to another module's standing claim), its captured keys leave the suppression set, its controller demand is withdrawn, and its overlays lose any contested slot — an overlay that held one gets its `onDeactivate`, so it can tear down. Its other callbacks are not called, with one exception: a `host.settings.onChange` callback still fires when its setting changes, and the Settings dialog in the module manager works for a disabled module too. Nothing else is undone: the **VM, its globals, its recurring timers and its listeners all survive**, recurring timers keep being re-armed (a one-shot `after` that comes due meanwhile is discarded uncalled), and an image search that was answered while it was off is searched again when it is enabled. Enabling it again resumes delivery; the entry file does **not** run again.

Two consequences follow. A module that starts disabled has still run its entry file and `activate`, so top-level side effects happen anyway — speech (which is not gated on the enabled state), synthesised input, sounds, `host.settings.define`. And a module-level cache survives a disable and enable; only a reload clears it.

Transitions: *enable* / *disable* flip the flag and recompute what is held at the OS, as above. *Reload* builds a **new VM in place** from the module's folder and runs its entry again; every module that depends on it through `dependencies` — directly or through other modules, of any kind — is rebuilt after it, in dependency order, so they take up its new code (a module that reaches it only through `optional_dependencies` is not). A broken `module.toml` leaves the running module untouched; a broken entry leaves the module inactive until the next successful reload.

## Enable/disable interface

- **Persistence:** the enabled state of every module is stored in `settings.toml` beside the application.
- **Control surfaces:** the module manager window (tray icon), with a native checkbox per module, Settings, Reload and Uninstall; a system-wide key that reloads every module; and the CLI for search, install, list, update and uninstall. There is no IPC for enabling, disabling or reloading from a script.
- **Conflict handling:** two modules claiming the same **hotkey** are reported in an accessible dialog naming both, and the combination passes to the waiting module when the holder lets go. Captured keys are not reported: a captured key is delivered to one module only — see [`host.keys`](api/keys.md).

## Isolation tiers

- **Script (Luau) modules** — the only tier that exists: in-process, one VM each, capability-gated, toggled at runtime.
- **Native FFI / untrusted modules** — the draft's out-of-process tier. Not built: no module can load native code.
