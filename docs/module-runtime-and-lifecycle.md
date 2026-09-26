# Module Runtime & Lifecycle

*Drafted 2026-06-21, brought in line with the host 2026-09-22. Belongs to [architecture-feasibility-study.md](architecture-feasibility-study.md) (§3 process/isolation model) and [host-api-capability-catalog.md](host-api-capability-catalog.md).*

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

- **One thread for module code.** The event loop runs on the main thread: the Win32 message loop (the wxWidgets one while the module manager exists), or the macOS run loop, driven by a 15 ms tick. Every module callback and every host call a module makes run there, one at a time, and so do the dispatch of hotkeys, captured keys, window events and controller events and, on macOS, the event tap. Some host calls hand their work to a thread of the host's own and return at once; their answer comes back to a callback on the event loop, on a later tick. Which calls do that, and which hold the loop until they return, is under [Threads](#threads).
- **One Luau VM per module**, all in the same process: globals and state are separate between modules, and each can be enabled and disabled on its own.
- **Nothing interrupts a callback.** No VM has a memory limit or an instruction-count interrupt. A callback that loops without end — or runs long, such as computing statistics over thousands of pixels in Luau — stalls the whole application: every module, speech, every captured key and hotkey waiting for its callback, and on macOS the event tap, which the system switches off once it stops answering for about 300 ms. Keep callbacks short, leave pixel work to the host's reductions ([`profile`](api/screen.md#host-screen-profile), [`cells`](api/screen.md#host-screen-cells), image search), and poll with the forms that answer in a callback ([`host.ocr.read`](api/ocr.md#host-ocr-read), [`matchCellsAsync`](api/screen.md#host-screen-matchcellsasync), [`imageSearchAsync`](api/screen.md#host-screen-imagesearchasync), [`imageSearchEach`](api/screen.md#host-screen-imagesearcheach), [`snapshotAsync`](api/screen.md#host-screen-snapshotasync)).
- **Errors are contained, not acted on.** A Luau error or a Rust panic in a callback is caught; the rest of the application goes on. The error is written to the log every time and shown in an accessible dialog once per module and kind of callback (again after the module is disabled and enabled). Nothing disables a module that keeps failing.
- Loading a module runs its entry file, which **registers** hotkeys, captured keys, window triggers, overlays, timers and listeners with the host. The host keeps them keyed by owning module, so each can be revoked or skipped per module.
- **A `code_module` runs more than once.** It is loaded as a module in its own right, with its own VM, and its source is evaluated again inside the VM of every module that depends on it, directly or through another `code_module`; each copy's registrations belong to the VM it ran in. Work meant to happen once goes into an `activate` function in the table the entry returns — see [`host.require`](api/require.md).

## Threads {#threads}

Module code runs on the event loop and nowhere else, so a module never needs a lock and never sees two of its callbacks at once. What the host does for it runs in one of two places: on a thread of the host's own, with the answer handed back to the loop, or on the loop itself, which then waits until the call returns. The API pages give each call's cost; this is the overview.

**Handed to a thread of the host's own.** The call itself costs the event loop what it takes to check its arguments — for `matchCellsAsync` that includes decoding every state it is given, and for the image searches loading any template file that is not cached yet — plus, in a module that reads through desktop duplication, the comparison described at the end of the next list. The work goes on elsewhere.

| Thread | What runs there | How the answer comes back |
|---|---|---|
| `screen-capture` | The picture of every [`host.ocr.read`](api/ocr.md#host-ocr-read), taken at the call — except a read of a snapshot, which is not photographed — and every picture of [`host.screen.snapshotAsync`](api/screen.md#host-screen-snapshotasync): at once, at a set time, or each round of a change wait. One capture at a time, in the order the [screen page](api/screen.md) gives. | `snapshotAsync`'s `cb` on the loop, on the next turn after its picture; a text read's through `ocr-recognise`. |
| `ocr-recognise` | The recognition of those pictures, one job at a time. On Windows, every region of up to 400x200 pixels also starts one neural recognition on a short-lived thread of its own, beside the system recogniser (see [`host.ocr.read`](api/ocr.md#windows)). | `cb` on the loop, on the next turn after the recognition finished. |
| The image worker — one thread for every module | [`imageSearchAsync`](api/screen.md#host-screen-imagesearchasync), [`imageSearchEach`](api/screen.md#host-screen-imagesearcheach) and [`matchCellsAsync`](api/screen.md#host-screen-matchcellsasync): the capture, the reduction and the matching. The worker serves as one batch every request waiting when it becomes free and those that arrive in the 5 ms after it took the first, with one capture per distinct region and source, and the matching is spread across the processor's cores. A duplication wait or a large match holds up every request queued behind it, whichever module asked. | `cb` on the loop, on a later tick. |
| `dxgi-capture` (Windows) | Desktop duplication, for a module that declares `[screen] capture = "duplication"`. A read through it may wait for this thread: one on the event loop up to about 60 ms, one on the image worker or `screen-capture` up to 250 ms. No read waits while duplication is overdue or is being left alone after a failure, and an event-loop read does not wait when the loop's share of waiting is spent, or while duplication is opening — except the first event-loop read of an opening that was not started ahead of time (see [Which picture a read sees](api/screen.md#which-picture-a-read-sees)). | The read that asked. |
| `keyboard-hook` (Windows) | The low-level keyboard hook. It matches captured keys and granted hotkeys and swallows them without waiting for the event loop, so a busy loop never delays typing in another application. | The key's callback on the loop. |
| `gamepad` (Windows) | Reading XInput pads: every 4 ms while a pad is connected and either a `down`, `up`, `axis` or `chord` listener of an enabled module exists or a `list()` or `state()` came in the last five seconds; otherwise a look at the four slots every two seconds while anything listens or such a call is that recent, and nothing at all while nothing does (see [`host.gamepad`](api/gamepad.md)). On macOS, GameController delivers changes on a queue of its own. | The listener on the loop. |
| Speech (`prism-speech` and `prism-voice` on Windows, `voiceover` and `avspeech` on macOS) | Talking to the screen reader or the voice. [`host.speech.output`](api/speech.md#host-speech-output) hands the line over and returns. | — |
| Audio output | Playing what [`host.sound.play`](api/sound.md#host-sound-play) opened. | — |

The host keeps a few more threads that no module call reaches: the module manager's GitHub searches and installs, the listener that lets a second start bring this copy's manager forward, and the warm-up of the recognition models at start.

**On the event loop, which waits until the call returns.** Everything else a module calls. The ones that take time:

- **Reading the screen synchronously:** [`pixel`](api/screen.md#host-screen-pixel), [`pixels`](api/screen.md#host-screen-pixels), [`snapshot`](api/screen.md#host-screen-snapshot), [`profile`](api/screen.md#host-screen-profile), [`imageSearch`](api/screen.md#host-screen-imagesearch), [`imageSearchMulti`](api/screen.md#host-screen-imagesearchmulti), [`imageSearchAll`](api/screen.md#host-screen-imagesearchall), [`cells`](api/screen.md#host-screen-cells), [`matchCells`](api/screen.md#host-screen-matchcells), [`template`](api/screen.md#host-screen-template) with `capture` (and with `file`, which decodes the PNG), [`save`](api/screen.md#host-screen-save) and [`saveMarked`](api/screen.md#host-screen-savemarked). A read given a [snapshot](api/screen.md#reading-a-snapshot) captures nothing and costs only its own work. On Windows every other one of them but `template` with `file` is at least one capture: about 16.7 ms through the standard path; through desktop duplication about 0.3–5 ms on an idle desktop (more with a game in front — see **Cost** under [Which picture a read sees](api/screen.md#which-picture-a-read-sees)) with a wait for its answer of at most about 60 ms; a read duplication does not answer in that time is then read the standard way as well, unless the module declares `fallback = "none"`.
- **Recognising text synchronously:** [`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) and [`recognizeMany`](api/ocr.md#host-ocr-recognizemany) — the capture and every recognition, with no time limit.
- **Asking another application:** [`host.element`](api/element.md) — accessibility queries into another process, from about 30 ms to several hundred — and the [`host.window`](api/window.md) queries.
- **Acting:** [`host.input`](api/input.md) and [`host.window.focus`](api/window.md#host-window-focus). Each first waits up to 50 ms for the calling module's pending pictures on `screen-capture` to be taken — its `host.ocr.read`s', its plain `snapshotAsync`s' and the first picture of a change wait without `at` whose baseline that picture is — and a `drag` paces its movement for about 60 ms on Windows.
- **Reading the module's own files:** [`host.include`](api/include.md), [`host.resource.read`](api/resource.md#host-resource-read), and evaluating a `code_module` dependency at load.
- **First calls that start something:** the first [`host.gamepad`](api/gamepad.md) `list`, `state` or `on` starts the controller watching, and on Windows waits up to 250 ms for the first reading (on macOS it does not wait); the first granted hotkey or capture on Windows starts the keyboard hook's thread and waits for it to answer; `host.sound.play` opens the audio device the first time, and opens the file and starts its decoder every time; in the first moments after the application starts, [`host.ocr.languages`](api/ocr.md#host-ocr-languages), `resolveLanguage`, and `recognize` or `recognizeMany` with a `lang`, wait up to 50 ms for the language list.
- **Asking for a controller's state:** on Windows, while no `down`, `up`, `axis` or `chord` listener of an enabled module exists, a [`host.gamepad`](api/gamepad.md) `list()` or `state()` made more than five seconds after the last one waits up to 50 ms for a fresh reading.
- **In a module that reads through desktop duplication**, its first read after it was built — and each read after that until duplication has answered once — makes a comparison of the two ways of reading for the log, on the event loop, even when the read itself is one of the asynchronous calls above: a duplication read of up to about 60 ms and, when it answers, one standard capture — of the first of the call's regions that has a rectangle at the call (a window region whose window's client area is empty has none), or of the client area of the window in front when that region is too small to tell the two apart.

Pure computation — [`host.json`](api/json.md), [`host.keys.normalize`](api/keys.md#host-keys-normalize) and the like, [`host.timer`](api/timer.md) arming — costs what the work does and no more.

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
