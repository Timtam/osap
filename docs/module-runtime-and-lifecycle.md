# Module Runtime & Lifecycle (draft)

*Draft, 2026-06-21. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md) (§3 process/isolation model) and [host-api-capability-catalog.md](host-api-capability-catalog.md).*

## Principle

**One platform process hosts many modules concurrently — not one OS process per module.** All *enabled* modules are loaded together; the host multiplexes events (hotkeys, window triggers, …) to them. Modules can be enabled/disabled at runtime. Out-of-process isolation is reserved for the high-risk tier (untrusted native FFI), not the default.

The current walking skeleton (one module per CLI invocation) is a scaffolding simplification, **not** the target model.

## Why not process-per-module

- **Hot-path latency:** the global hotkey / window-trigger dispatch must live in one place (the daemon's event loop) for low latency. Fanning every event out to N processes via IPC is the wrong shape (feasibility study §3 trade-off).
- **Cost:** each process means its own memory + VM + startup latency; dozens of small modules would be wasteful.
- **Safety doesn't require it for scripts:** the Luau VM sandbox already isolates script modules within the host (memory limits, no FS/OS stdlib, interruptible), and the host-API capability gating is the real security boundary (study §7). Script modules therefore do **not** need a separate OS process for safety.

## Runtime model

- The **daemon** process holds the shared event loop on the main thread (as in the walking skeleton: the Win32 message loop that dispatches `WM_HOTKEY`).
- A **Module Manager** owns the set of installed modules and their state; on startup it loads every *enabled* module.
- **One Luau VM per module** (not one shared VM): clean isolation of globals/state between modules, independent enable/disable, and a faulting module can't corrupt another. Luau VMs are lightweight, so this is cheap. All VMs live in the **same process**.
- Loading a module runs its entry, which **registers triggers** (hotkeys, window-activate triggers, overlays, automations) with the host. The host keeps a registry keyed by **owning module**, so every registration can be revoked when the module is disabled.

This decides the previously open "worker granularity" question (study §3) for script modules: **in-process, one VM per module.**

## Lifecycle / states

- **Installed** — present on disk (extracted to the module cache), not necessarily active.
- **Enabled** — the user wants it active; persisted in config.
- **Loaded / Active** — VM created, entry run, triggers registered.
- **Disabled** — triggers unregistered, hotkeys released, VM dropped.

Transitions: *enable* → load (create VM, run entry, register triggers); *disable* → unload (revoke all of the module's registrations, drop VM). *Reload* (e.g. after an update) = disable + enable.

## Dynamic enable/disable interface

- **Persistence:** the set of enabled modules is stored in config (the user profile).
- **Control surfaces:**
  - **GUI / tray menu** (wxDragon): list modules with on/off toggles, status, last error, and conflicts.
  - **CLI / IPC:** `list` / `enable <id>` / `disable <id>` / `reload <id>` / `status <id>` — so the GUI and scripts can drive the same manager.
- The **Module Manager** API: `list()`, `enable(id)`, `disable(id)`, `reload(id)`, `status(id)`. Because every host registration is tagged with its owning module, `disable` can cleanly revoke that module's hotkeys/triggers/overlays.
- **Conflict handling:** the manager detects and reports collisions (e.g. two modules requesting the same global hotkey) instead of letting a registration silently fail.

## Isolation tiers (reconciles with the security model, study §7 / §11)

- **Script (Luau) modules — DEFAULT:** in-process, one VM each, VM-sandboxed + capability-gated. Loaded together, toggled at runtime.
- **Native FFI / untrusted modules — high tier:** out-of-process sandboxed worker (one or pooled per the security policy). Only these get a separate process. The Module Manager still controls their lifecycle, via IPC.

## Implications for the implementation

- Extend the host's registration registries (currently the per-host hotkey table from Slice 2) to be **per-module**, so the manager can revoke a module's registrations on disable.
- The event loop becomes multi-module: one daemon loop, dispatch to the owning module's VM by registration id.
- A module crashing/erroring is contained to its VM; the manager surfaces the error and can auto-disable a repeatedly-faulting module.
