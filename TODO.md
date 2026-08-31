# Backlog / To-do

Project backlog for the cross-platform automation platform.
Architecture and feasibility foundation: [docs/architecture-feasibility-study.md](docs/architecture-feasibility-study.md).

## Implementation

- [x] **Walking Skeleton (Slice 1):** Cargo workspace (`crates/module-manifest`, `host`, `app`) + Luau embedding (mlua/luau, `error-send` feature) + `host` API (`log`/`speech`/`path`/`resource`) + module loading (`module.toml` + entry). `cargo run -p app -- examples/hello` loads the example module and speaks via tts-rs. ✓ (2026-06-21)
- [x] **Slice 2:** `host.hotkey` + event loop. Windows backend (Win32 `RegisterHotKey` + `GetMessage` loop via `windows-sys`) behind a platform-gated interface (seed of the OS-backend abstraction); a global hotkey triggers a Luau callback. Example: `examples/hotkey` (Ctrl+Alt+H → speaks). macOS Carbon `RegisterEventHotKey` to follow. ✓ (2026-06-21)
- [x] **Slice 3:** `host.window` (`list`/`active`, OS-gated declarative `find`/`findAll` matcher via a Luau prelude) + `host.os`. Windows backend (`EnumWindows`/`GetForegroundWindow` + title/class/pid/exe/bounds). Example: `examples/window`. ✓ (2026-06-21)
- [x] **Slice 4:** OS-backend trait abstraction (`backend::Backend`, Lua-agnostic). Windows impl (window enumeration + hotkeys) and a stub for other platforms; the host bridges OS events to Luau via a `HostEvents` dispatcher. Prepares the macOS backend as a second impl. ✓ (2026-06-21)
- [x] **Slice 5:** `host.window.onTrigger(matcher, {on=…}, cb)` — window event layer. Windows `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` feeds foreground changes into the event loop (woken via `PostThreadMessage`), dispatched to OS-gated matchers (Luau prelude). Example: `examples/window-trigger`. ✓ (2026-06-21, interactively confirmed: window titles announced on switching)
- [x] **Slice 6:** `host.screen` — `size`/`pixel` (GDI) + `imageSearch` (GDI BitBlt capture + PNG-template match, template alpha as mask, OS-gated region). Example: `examples/screen`. ✓ (2026-06-21)
- [x] **Slice 7:** `host.ocr` — text recognition via `Windows.Media.Ocr` (WinRT). `recognize({region, lang})` → `{text, words=[{text,x,y,w,h}]}` (boxes in screen coords). macOS Apple Vision / ONNX to follow. Example: `examples/ocr`. ✓ (2026-06-21)
- [x] **Slice 8:** `host.input` — `cursorPos`/`move`/`click`/`drag`/`scroll` (SetCursorPos + SendInput) + `send`(combo)/`text`(unicode). Windows backend. Verified non-destructively (cursor move + restore). Example: `examples/input`. ✓ (2026-06-21)
- [x] **Slice 9:** `host.sound` (rodio, fire-and-forget) + `host.overlay` runtime (Luau prelude): self-voicing control tree (StaticText/Hotspot/Custom/OCR), keyboard navigation + speech composition, built on the primitives. First overlay runs end-to-end. Examples: `examples/sound`, `modules/overlay`. ✓ (2026-06-21) — **ReaHotkey overlay-MVP capability set complete.**
- [x] **Slice 10:** Overlay auto-activation. `host.hotkey.unregister` (backend `UnregisterHotKey`) + `host.window.test` (expose the matcher predicate); `overlay:attach(matcher)` activates an overlay (registers hotkeys + announces) while a matching window is focused and deactivates on leaving — ReaHotkey-style context binding (wires onTrigger + host.overlay). Example: `examples/overlay-attach`. ✓ (2026-06-21)
- [x] **Slice 11:** `host.keys` — low-level keyboard hook (`WH_KEYBOARD_LL`) with **suppression**. `capture(name, cb)`/`release(name)`/`releaseAll()`; captured keys are swallowed (not passed to the focused app) and dispatched as `on_key` with `{shift,ctrl,alt}`. Example: `examples/keys` (toggle Tab/Escape capture with Ctrl+Alt+O). ✓ (2026-06-21)
- [x] **Overlay natural navigation:** `overlay:attach(matcher, { naturalNav = true })` captures (+ suppresses) **Tab / Shift+Tab** for moving between controls and **Enter** to activate, via `host.keys` (modifier-exact, so Alt+Tab passes through); released when the overlay deactivates. Arrows are left for value changes (standard scheme). Activate/deactivate are silent (ReaHotkey-style). ✓ (2026-06-21)
- [x] **First real ReaHotkey port — Sforzando (`modules/sforzando`):** detect the standalone sforzando window (title/class/exe), attach a self-voicing overlay of 3 OCR read-outs (Instrument / Polyphony / Pitchbend range) with **window-relative** OCR regions (translated to screen via the active window's origin), navigated with Tab. Validates the whole stack on real plugin UI. ✓ (2026-06-21, interactively confirmed)
- [x] **OCR engine — dual-engine pipeline (embedded PaddleOCR fallback):** the weakness Sforzando exposed — `Windows.Media.Ocr` is fast (~5 ms warm) but **blind to isolated single glyphs** (reads "128" but not a lone "1") — is fixed by a second in-process engine. A **PaddleOCR recognition model via ONNX Runtime** (`ort`, recognition-only, English PP-OCR mobile) is **embedded in the binary** (`crates/host/models/`, `include_bytes!` — portable, no external file) as a **fallback**: for a small region it runs **concurrently** with WinRT (`backend::paddle_ocr`) and its read is used only when WinRT returns empty, so a lone digit costs ≈ max(winrt, paddle) ≈ 15 ms (not their sum) while the common multi-char case stays on WinRT. Shared **content-tight preprocessing** (`tighten`: crop to the glyph + upscale) was the real unlock — it fixed both engines. The model **preloads at startup** off the hot path (`backend::warmup_ocr`). Engine chosen by a measured spike (pure-Rust `ocrs` vs `ort`+PaddleOCR on real crops; PaddleOCR won on speed + single-digit accuracy). WinRT stays the primary + the only multi-word/detection path; macOS will use Apple Vision in that role with PaddleOCR as the cross-platform single-glyph specialist. See [[ocr-engine-architecture]]. ✓ (2026-06-21, interactively confirmed)
- [x] **Overlay UX — resume position, focus announcement, menu pass-through (ReaHotkey parity):** (1) the overlay **remembers the last-focused control** across deactivate/reactivate, so re-entering the window (Alt+Tab out and back) resumes where you were instead of at 0; (2) on regaining focus it **announces the current control after a short delay** (`host.timer.after`, ~350 ms) so it doesn't clash with the screen reader's window-title announcement; (3) **menu pass-through** — captured nav keys (Tab/Enter) are suppressed only while the overlay should own them: the scoped window is foreground (`host.keys.scope`, new `Backend::set_key_scope`) **and** no popup menu is open (the hook checks for a visible `#32768` window — ReaHotkey's `WinExist("ahk_class #32768")` / `GetContext`). So activating an OCR button that opens a context menu lets arrows/Enter drive the menu natively instead of re-clicking the button, and Tab resumes overlay navigation once the menu closes. Adds `host.timer` (one-shot callbacks fired from the loop tick). ✓ (2026-06-21, interactively confirmed)
- [x] **Embedded plugins — windows-in-windows detection (DAW-hosted VST/AU):** plugins hosted inside a DAW (REAPER / Ableton) render into a child control of the host's plugin window, so the standalone window matcher can't see them. New primitives: `host.window.controls(win?)` (child controls: class + screen geometry + client origin, via `EnumChildWindows`) and `host.window.focusChain()` (controls from the focused element up to its top-level window, via `GetGUIThreadInfo` + ancestor walk). The overlay gains `attachEmbedded({ hosts, control })`: active while the keyboard **focus is inside** a control whose class matches `control` (a Luau pattern, e.g. sforzando's host-independent `^Plugin%x+$`) within any of `hosts` (DAW host-window matchers taken from ReaHotkey), using that control's client area as the coordinate origin — so the *same* OCR regions work standalone and embedded. Activation is driven by a new **`EVENT_OBJECT_FOCUS`** hook (focusing into a plugin within an already-foreground host raises no foreground event — more general than ReaHotkey's REAPER-specific F6); keying on focus-in-plugin (not host-foreground) means the overlay doesn't capture the host's own navigation keys. The overlay runtime was refactored to a **multi-context model** (standalone + embedded side by side); sforzando attaches both. The inspector gains **Ctrl+Alt+C** to dump a window's child controls (calibration). See [[embedded-plugin-detection]]. ✓ (2026-06-22, interactively confirmed)
- [x] **Plugin identity (UIA + OCR/image checker):** `^Plugin%x+$` matches *any* REAPER plugin, so `attachEmbedded` takes a per-plugin `identify(control)` check (cached per control HWND, since the check is costly). Sforzando uses **UIA** — a Pane (ControlType `50033` = `UIA_PaneControlTypeId`) named `PlogueXMLGUI`, exactly as ReaHotkey — with an **OCR landmark** ("sforzando" wordmark) as the UIA-free fallback (image-search via `host.screen` is another option the same hook allows). New minimal UIA: `host.uia.find(hwnd, name, controlType)` (`backend::uia`, `windows` 0.58 `IUIAutomation` cached in a thread-local; `ElementFromHandle` + AND-condition on Name/ControlType + `FindFirst(Subtree)`; runs on the existing RoInitialize'd MTA thread). Verified: a non-sforzando plugin has no `PlogueXMLGUI` pane → identity rejects it. See [[embedded-plugin-detection]]. ✓ (2026-06-22, interactively confirmed)
- [x] **Module dependencies + library modules (ecosystem Phase 1):** a `module.toml` can declare `dependencies = ["<id>", …]`; the manager **auto-discovers** each among sibling module directories, loads them first (recursive, topological, cycle-detected), and a **library module** shares reusable data by **returning a table** from its entry, which dependents import via **`host.require("<id>")`** (data only — Lua functions can't cross module VMs, so exports are serialized through serde). First use: the DAW host-window criteria moved out of the app/plugin into a shared `modules/daw-hosts` library that `modules/sforzando` depends on — keeping the app **general-purpose (mechanism, not content)**. Groundwork for **GitHub-topic module discovery/install/update** (HFS-style: repos tagged with a topic, no central registry — ecosystem Phase 2) and **module-manager tabs** (browse/install/uninstall — Phase 3). See [[module-ecosystem]]. ✓ (2026-06-22, interactively confirmed)
- [x] **GitHub-topic module discovery + install/update (ecosystem Phase 2):** `host::registry` (unauthenticated GitHub REST via `ureq`): **search** repos tagged `MODULE_TOPIC` (a central working-title constant, `osap-module`), **install** by `owner/repo` (download the default-branch ZIP via `…/zipball/{branch}`, strip GitHub's wrapper folder, extract into a portable `modules/` dir next to the exe + record `.source.toml`), review the manifest's requested **capabilities** before confirming, **update** (compare the branch's latest commit SHA to the installed one), **list** / **uninstall**. Driven by CLI subcommands (`automation-platform search|install|update|list|uninstall`); the polished GUI tabs + capability-review dialog are **Phase 3**. **Dogfooded end-to-end** against a live tagged repo (`Timtam/osap-daw-hosts`): search → install (capability review) → list → update (SHA-detect → reinstall to the new version) → uninstall, all verified. (Note: GitHub's `commits/{branch}` SHA can lag a push by a few seconds, so an update check immediately after a push may need a retry.) See [[module-ecosystem]]. ✓ (2026-06-22)
- [x] **Lazy audio:** the rodio output device is opened only on the first `host.sound.play`, not at startup — this tool overlays audio software, so holding the device could break it (caused an audio-output loss when launching sforzando). ✓ (2026-06-21)
- [x] **Slice 13 — module-package loader:** ZIP + extract-on-install. `LoadedModule::load()` accepts a dir (dev) or a `.zip`; zips extract to a content-addressed cache `<temp>/automation-platform-modules/<id>/<version>-<hash>/` (cache-hit skips re-extraction); resources resolve against the extracted root. ✓ (2026-06-21)
- [x] **Slice 12 — module manager (concurrent multi-module runtime):** load multiple modules in one process (one Luau VM each, per-module root), sharing one `Backend`/TTS/audio + one event loop with **central event routing** (hotkeys/keys/window-triggers tagged by owning module; `mlua::Lua` is clonable so callbacks dispatch to the right VM). `app <dir1> <dir2> …`. Dispatcher honors a per-module `enabled` flag. ✓ (2026-06-21)
- [x] **Slice 14 — wxDragon GUI integration (event-loop coexistence):** resident modules open a native wxDragon window; wxWidgets owns the message loop and our OS events (hotkeys via a message-only window + `WM_HOTKEY`; foreground + low-level-key hooks) are drained from a wx `Timer` tick via `Backend::pump_pending`. New `host::gui`; `Manager::run` hands the loop to wx (or `run_event_loop` when `AUTOMATION_PLATFORM_HEADLESS=1`). App embeds a Windows manifest (Common Controls v6 + DPI) so wxWidgets uses themed controls and no startup warning. Litmus test passed: window open **and** Ctrl+Alt+H still speaks. ✓ (2026-06-21)
- [x] **Slice 15 — tray module-manager window:** the resident app is now a system-tray manager (wxTaskBarIcon: Show / Quit, double-click to open; window starts hidden, closing hides to tray). The window lists loaded modules with **native checkboxes** (`wxTreeCtrl` + `TVS_CHECKBOXES` via raw Win32, for correct screen-reader/UIA toggle exposure — generic/owner-drawn controls were rejected as not accessible enough, see [[accessibility-native-controls]]). Toggling a checkbox enables/disables the module at runtime: `Shared::set_enabled` (un)registers its OS hotkeys + recomputes the captured-key set; the dispatcher already skips disabled modules. ✓ (2026-06-21)
- [x] **Persist the enabled set:** the manager remembers disabled modules across runs (re-applied right after a module's entry loads, revoking its OS registrations). Originally a `disabled-modules.txt`; folded into the unified settings store below. ✓ (2026-06-21)
- [x] **Slice 16 — settings format + GUI:** `host.settings` (`define`/`get`/`set`/`onChange`, `host.config` alias) — modules declare typed, validated, auto-persisted settings; each module sees only its own. Unified portable store `host::settings` (`<exe_dir>/settings.toml`: per-module `enabled` flag + `settings` map; atomic write; corrupt-file quarantine; auto-migrates the old `disabled-modules.txt`). The tray manager has a **per-module Settings… dialog** with native, screen-reader-labeled controls (checkbox / number field / dropdown / text — each labeled by a leading `wxStaticText` + `set_name`, the only thing NVDA reliably reads; see [[accessibility-native-controls]]). Example: `examples/settings`. ✓ (2026-06-21)
  - Follow-up: optional advisory `[settings.<key>]` block in `module.toml` for pre-run GUI introspection (deferred); bump `engine_api` (additive).
- [ ] **A module excluded by `supported_os` is invisible in the manager.** The Installed list
      is built from what was loaded, so a module skipped for this platform has no row —
      somebody who installs past the "will not be loaded here" warning and then looks for it
      will not find it. A synthesised row needs Settings, Reload and Uninstall to refuse for
      it, in the one window a blind user depends on, which is why it was not done in the same
      change. The log names every exclusion at every start in the meantime.
- [ ] **Module manager follow-ups:** CLI/IPC control surface; a **native macOS/GTK checkbox path** (`TVS_CHECKBOXES` is Windows-only — non-Windows currently shows no checkboxes). Out-of-process only for the untrusted-native-FFI tier. See [docs/module-runtime-and-lifecycle.md](docs/module-runtime-and-lifecycle.md).
  - Lifted out of the completed entries below, where they were easy to lose:
  - [ ] **Hotkey conflicts are resolved at registration only.** A binding skipped because another module held the combo does not activate when that owner is later disabled (needs a restart), and `apply_enabled`'s re-register on enable neither conflict-checks nor surfaces a clash.
  - [ ] **`rollback_to` still has the active-overlay `onDeactivate` gap** that `purge_module` fixed for reload — it applies on uninstall and on a failed load.
  - [ ] **Versioned dependencies: no version SELECTION.** Install fetches the default branch's latest, so a `>= x.y` constraint is checked at load but never used to choose what to fetch; reload does not re-verify constraints; and the `"id >= x.y"` syntax is undocumented in the module.toml docs.
  - [x] **Hotkey conflict detection (bd53d02):** a cross-module global-hotkey clash is detected at registration (parsed `(vk,mask)` compare via `key_spec`, order/case-stable) and surfaced in the accessible no-TTS dialog naming both modules + the combo; first-come keeps it, the later module stays loaded with that binding inactive. An OS rejection (another app holds the combo) is reported distinctly; a malformed spec fails loudly with the real parse error. Captured-key duplicates are intentionally NOT flagged (window-scoped overlays legitimately share Tab/Return). Adversarially reviewed. **Follow-up:** dynamic re-resolution — a skipped/disabled binding doesn't activate when the owner is later disabled (needs a restart), and `apply_enabled`'s re-register on enable doesn't conflict-check/surface (silent). Conflicts are registration-time only.
  - [x] **In-place module reload (235f92d):** a "Reload" button rebuilds the selected module's VM in place from its source dir (edit a dev module + reload, no app restart, no loss of other modules' state). `populate_vm` (the VM build, extracted + shared with load) + `purge_module` (single-index registration teardown — like `rollback_to` but `== idx`, no vector truncation; fires an active overlay's `onDeactivate`) + `reload_module` (re-read manifest BEFORE purge so a broken manifest leaves the running module intact; store snapshot/rollback on a failed rebuild; fresh VM under the same index). Refreshes the row's settings/dependencies/label; a failed reload tells the user the module is now inactive. Adversarially reviewed (MEDIUM findings folded in). **Follow-ups:** live cascade to *dependents* (they hold a copy of the code in their own VM → still need a restart; = the "live hot-reload on update" item below); `rollback_to` (uninstall / load-failure) still has the active-overlay `onDeactivate` gap that `purge_module` now fixes.
- [x] **Module crash protection + accessible error dialog (runtime callbacks):** a faulting module's callback — a Lua error OR a re-raised Rust panic — no longer takes down the app or sibling VMs. The hotkey / key / timer (one-shot + recurring) / window-trigger (activate + focus) / settings-`onChange` / arbiter dispatch all run through `call_guarded`/`guard` (`catch_unwind` + Lua-error capture, `crates/host/src/lib.rs`); the concrete Luau error (mlua's message + traceback where present) is logged **and** surfaced — attributed to the faulting module id — in the manager's accessible no-TTS dialog (`modal_message`; see [[accessibility-native-controls]]). The tick that shows it is re-entrancy-guarded (modal dialogs run a nested wx loop), and the dialog is deduped per (id, context) and reset on enable/disable so a fixed-and-retried module re-surfaces. Unit-tested (catches both a Lua error and a Rust panic) + adversarially reviewed (re-entrancy / dedup-flood / borrow-safety fixes folded in).
- [x] **Module load + `activate` panic isolation (37abc02):** `load_module`'s fallible block AND its dependency-resolution closure now run under `catch_unwind`, so a Rust panic from a module's own code (entry eval, code-dep eval, `activate`, or dep resolution) becomes a load error routed through the existing `rollback_to` instead of aborting the app (matters most on GUI hot-load). The store half of rollback is completed too: `load_module` snapshots the module's `settings` entry before the fallible block and restores it on failure, so a partial load can't leave an orphan section or leak a value onto a later reinstall. Adversarially reviewed (4 lenses + verify); both LOW findings (deps-closure window + store rollback) folded in. Tested (load-isolation pattern + store snapshot/restore round-trip).
- [x] **Versioned dependencies (min-version constraints, 9ce375b):** a `module.toml` dependency may carry a space-separated semver requirement (`"com.x.y >= 1.2"`; a bare id is unchanged + backward-compatible). `module-manifest::dep_id`/`dep_constraint` split a spec; at LOAD the resolved dependency's version is verified against the requirement (`check_dep_version`) — an unsatisfied one fails the load with a clear error (`module 'A' requires 'B' >= 1.2, but the loaded version is v1.0`), a non-semver req/version is logged + skipped. The id-based machinery (`host.require`, sibling resolution, the install/uninstall graph) consumes the stripped ids stored on `Module`/`InstalledModule`. Verified headless (impossible constraint fails, satisfiable loads) + unit-tested (parser + version check). **Follow-ups:** install fetches the default-branch latest (no constraint-driven version *selection*); reload doesn't re-verify constraints; document the `"id >= x.y"` syntax in the module.toml docs.
- [x] **Version-based update detection (6ed6860):** `registry::update_available` now compares the upstream `module.toml` `version` (semver) to the installed version — only a version bump signals an update; commits *between* releases no longer do. Falls back to the commit-SHA comparison when either version isn't valid semver. The CLI + the tray Updates list show the `vX → vY` transition. (Install still fetches the latest by SHA; only the signal changed. Added the `semver` crate.)
- [x] **Live hot-reload on update (no restart):** updating or reloading a module now rebuilds its **transitive dependents** too, in dependency order, without a restart. A code module's source is copied into each dependent's VM when that dependent is built, so rebuilding only the changed module left every dependent running the OLD copy — which is why the manager used to say "restart to apply". `registry::reload_order` (pure + unit-tested, incl. a cycle) sorts the affected modules so a dependent is always rebuilt AFTER the dependency whose code it copies; `reload_module_tree` walks that order and reports per-module success/failure (a dependent that fails to rebuild is left inactive and named, rather than aborting the cascade). Wired into both entry points: the Reload button and the update flow, which now reloads in place instead of asking for a restart. The TODO's stated blocker ("needs a per-module unload that re-indexes") turned out not to apply — `reload_module` rebuilds in place under the SAME index, so nothing is dropped from the index-keyed vectors. ✓ (2026-07-26)
- [ ] Finalize the host's license (currently the placeholder `GPL-3.0-or-later`; a permissive one might be more flexible for a module ecosystem).
- [ ] **Packaging:** release builds must bundle the screen-reader client DLLs the `tolk` feature deploys next to the binary (`nvdaControllerClient64.dll`, `SAAPI64.dll`), so `host.speech` routes to NVDA/JAWS on end-user machines.
- [x] **Logging off stdout:** diagnostics now go to a file `<exe_dir>/automation-platform.log` (portable — next to the binary) via `host::logging`, not stdout/stderr (a screen reader reads the focused terminal). ✓ (2026-06-21)

## macOS (2026-08-13, written blind)

The backend, the packaging and the documentation exist; nothing has ever run on a Mac.
See [docs/macos-port.md](docs/macos-port.md) for the decisions and
[docs/building-on-macos.md](docs/building-on-macos.md) for how to build it.

- [ ] **Get the CI build green.** `.github/workflows/macos-build.yml` is the only place this
      code is ever linked — `check-macos.ps1` runs the compiler front end only, and `host`
      itself cannot be checked from Windows at all (`tts` pulls `objc_exception`, whose
      build script needs a C compiler). Everything below is downstream of that job passing.
- [x] **First working macOS overlay: Sforzando standalone** (2026-08-15). Written from what
      two probes measured — `AXWindow/AXStandardWindow/` + `com.Plogue.sforzando` for identity,
      and the title-bar derivation that brought the authored coordinates within a few points
      of the Windows ones. The three OCR regions are per-platform because sforzando's own two
      builds differ by a few points, not because the coordinate systems do. Untested on
      hardware; the pitchbend region is the one most likely to want a nudge.
- [ ] **An OCR button clicks without the covered-point check.** `refuseOutside` guards
      `hotspot`, `hotspottoggle`, the `points` sequence and an image-found handle, but the
      `ocr`/`ocredit` branch clicks the region centre unguarded
      (`modules/overlay-runtime/src/main.luau:1730`). No comment there claims it is deliberate,
      so it reads as an omission rather than a decision — and it is the one control kind that
      matters most here, because sforzando, the only overlay that activates on a Mac, is built
      entirely from OCR read-outs and OCR buttons. Whatever the check is worth, that path does
      not get it. The fix is not a one-liner to apply blind: `refuseOutside` also enforces
      "inside the origin's frame", and an OCR region is authored against the origin but found
      by recognition, so the frame half needs its own thought before either half is turned on
      for a path every working overlay uses.

- [x] **What has never run there is now asked by the probe, not by a person** (2026-08-31).
      There was briefly a page in `docs/api/` listing every capability that compiles on macOS
      and has never been seen to work. That was the wrong shape twice over: it is a to-do and
      not API documentation, and most of it was work a tester should never have been handed.
      The probe (`Cmd+Shift+F9`) now answers what a machine can answer, in the log the tester
      was sending anyway — all of it read-only, because it runs over somebody's real plugin
      and they cannot see what it did:
  - **Do the two coordinate spaces agree** — the pointer's own position against the window's
      accessibility frame. A factor of two here is the Retina bug, visible before any click.
  - **`ownsPoint`** at the window centre and at a point provably outside its frame, plus every
      window the enumeration says overlaps the centre. The negative control had to be earned:
      the first version asked about the display's top-left corner and got `true` — correctly,
      because the probed window began at `-8,-8`.
  - **Whether the recogniser invents text.** Five 64-point strips are profiled for runs of
      rows that are one flat luminance; two runs of *different* colour are re-read to prove the
      rectangle is uniform, then recognised, and every string is logged verbatim. The same
      string out of two unlike blanks is the exact fingerprint the Windows failure left. A
      third region straddles a flat run's edge, because a perfectly flat rectangle short-cuts
      before the retry ladder and would leave the worse case unmeasured.
  - **What a 100 ms timer really costs**, ten hops against the clock — the number every
      `watch` deadline is denominated in. Measured on Windows: 1098 ms for ten, longest hop
      131 ms.
  - Verified by running it: on Windows all three OCR regions return nothing, `ownsPoint`
      answers `true` inside and `false` outside, and the strip search finds 26 flat regions
      where the first version — searching the full window width — found none at all.
- [ ] **`window_owns_point` on macOS: the obvious implementation is a trap.** Reaching for
      `CGWindowListCopyWindowInfo` costs no new FFI, no new permission and no coordinate
      conversion, and it answers **the wrong question**: what is *drawn* at a point, not what a
      click would *hit*. Clicks are posted by location, so the window server's hit test decides
      — and a click-through window over the control (a screen reader's cursor ring, a system
      HUD, a notification banner) would make it answer a confident "somebody else owns this"
      for a press that would have worked. On a machine whose user navigates by that very ring,
      that is every control, refused, with a spoken excuse. Strictly worse than the `nil` it
      returns today.
      The API that answers the right question is
      `NSWindow.windowNumberAtPoint:belowWindowWithWindowNumber:` — "the frontmost window that
      would be hit by a mouseDown" — whose number is a `CGWindowID` directly comparable to
      `handles::Entry.window_id`. Its costs are real but bounded: `NSWindow` is not in the
      enabled `objc2-app-kit` features and would go into both manifests, it needs a
      `MainThreadMarker`, and its `NSPoint` is **bottom-left** where everything else in that
      backend is top-left.
  - Two things any implementation must not do, both found by checking a draft rather than by
      running it. It must never turn an entry it could not parse into a verdict about a
      different window — skipping an unreadable window answers from whatever lies behind it,
      and that is a `Some(...)` derived from a failure. And it must not fall back to walking
      `AXParent` for the window: `MESSAGING_TIMEOUT` is a second per read and the walk is up to
      twenty levels on the thread that carries the event tap, so the cure for "clicks land
      wrong" would be "hotkeys stop working".
  - Also blocked on this: `Entry.window_id` is **always 0** today — nothing interns a real one
      — and `ffi::ax_window_id` and `ax::window_id` are both dead code with no callers. Either
      would have to start being filled in first.
- [ ] **Most of the untested list is not a testing job at all — it is blocked.** Scroll, drag,
      `editable`, `typingWhen` and the stepper are used by exactly one module, `ik-on-ear`,
      which declares `supported_os = ["windows"]`. No module that loads on a Mac can reach any
      of those code paths, so nobody can exercise them there however willing. `Overlay:watch`
      is the same in practice: its callers are `addHotspotToggle` and the stepper, and the only
      overlay that activates on a Mac is sforzando, built entirely from OCR read-outs and OCR
      buttons. These come off the list when a macOS-reachable overlay uses them, not before.
- [ ] **The blank-region guard is Windows-only, and macOS escalates harder on the same input.**
      `tighten()`'s `blank` marker lives in `backend/windows.rs` and stops a flat rectangle ever
      reaching a recogniser. `macos/ocr.rs` notices the identical fact in `Plan::content`, logs
      it at *trace*, asks TCC whether it is a permission problem — and hands the rectangle to
      Vision anyway. Vision is likelier to decline than PaddleOCR was, because it runs a text
      *detector* first, but `run_vision` applies **no confidence filter at all** and sets
      `setMinimumTextHeight(0.0)`, which removes the size floor.
  - The worse case is not the flat rectangle. An empty *well* — a dark value field set into
      lighter chrome — is found by the crop, so `cropped` is true, the retry ladder opens, and
      it ends at the `FAST` character model over an empty box: the exact analogue of the
      Windows failure. That rung already writes "only the fast model read this" to the log, but
      **nothing marks the string untrusted on the way back to Lua**, so a module cannot decline
      it and the person hears it. Either discard that rung's result or carry the fact in
      `OcrText`.
  - The probe's third region exists to measure exactly this, so the next log says whether it is
      theoretical.

- [ ] **The seven first-session measurements** in docs/macos-port.md, in that order: does
      anything appear, are coordinates right on Retina, is a capture real, does the tap
      suppress, does OCR read plugin text, does `_AXUIElementGetWindow` work, what does the
      pump cost.
- [ ] **Does Apple Vision read a lone digit?** The one measurement that decides whether the
      second OCR engine has to become cross-platform: Windows runs a neural fallback
      precisely because the system engine refuses single glyphs, and that fallback is a
      Windows-only dependency. Ten minutes with one request against the crops the Windows
      spike already produced.
- [ ] **Do Qt object names survive into `AXIdentifier`?** The strings the Kontakt and
      Komplete Kontrol modules navigate by (`FileTypeSelector`, `WhatsNewScreen`) reach
      Windows through Qt's Windows accessibility provider; the macOS bridge is a different
      one. If they do not survive, those modules need a different anchor on macOS. The
      accessibility dump answers it.
- [x] **The macOS key vocabulary: keep the Windows one** (2026-08-20). The proposal was to
      translate everything to VOCR's `Cmd+Shift+Ctrl+<letter>` / `Cmd+Ctrl+<arrow>`. It was
      built on an assumption that did not survive being checked: a control's hotkey is
      registered in `_registerHotkeys` from `_activate` and dropped in `_unregisterHotkeys`
      from `_deactivate`, so it is claimed **only while that plugin's overlay is in front**,
      never all day. Most of the case went with that.
  - What is left is real but narrow. On macOS `Option` is part of generating characters, not
      only a modifier, and two bound keys sit on the two most-used dead keys — `Alt+E` is
      acute, `Alt+N` is tilde. `Ctrl+N` and `Ctrl+P` are the emacs next/previous-line
      bindings every text field honours. Both matter in exactly one place: a plugin's own
      search box, with the overlay active.
  - And it is not asymmetric. On Windows those same keys are just as unavailable while the
      overlay is active — `Alt+<letter>` is the menu-accelerator layer, `Ctrl+N`/`Ctrl+P` are
      New and Print. Nothing on macOS *takes* them from us either: `Ctrl+<letter>` and
      `Option+<letter>` alone are not VoiceOver's (its modifier is both together), and Carbon
      accepts them.
  - Against changing: one vocabulary, one set of documentation, and parity with ReaHotkey —
      which is where these users and these fingers come from. Per-platform keys fork every
      spoken announcement as well as every doc.
  - **So it stays, and the log decides rather than the argument.** A refused registration is
      already reported by name (`backend/macos/hotkey.rs:98`, "macOS refused the shortcut
      ..."). If a specific combination actually fails on a Mac, that one moves, with
      `host.os.pick`, which already exists. Nothing moves on a prediction.
  - **Borrowing VoiceOver's own keys is closed, permanently.** The probe came back with no
    tone at all, and four independent lines of evidence say no application can do better.
    Our tap is already `HIDEventTap` + `HeadInsertEventTap` + suppressing — the earliest
    point CoreGraphics offers; session taps, per-pid taps and Carbon `RegisterEventHotKey`
    are all strictly downstream, and root does not move a tap (the root rule is an access
    check at creation, not a priority). VoiceOver is not in the tap chain at all: head
    insertion would have put us ahead of any pre-existing tap, and VO-arrow keeps working
    inside secure password fields where TN2150 says no tap receives keys — so its handling
    lives in the window server, above the whole Quartz layer. Apple says so twice
    (developer.apple.com/forums/thread/727085, /776129), and **VOCR — same users, same
    problem — binds no Control-Option chord anywhere**: its shipped `Shortcuts.json` uses
    Carbon masks 4864 and 4352, and the Option bit 2048 never appears.
  - Ruled out with reasons, so nobody re-derives them: a **DriverKit virtual HID device**
    (the Karabiner architecture) is the only layer genuinely below VoiceOver, and it is
    disqualified three times over — Apple-granted entitlements plus notarisation plus a root
    daemon, macOS 13+ against a tester on 12.7.6, and it remaps globally rather than while
    one window is in front, which is the opposite of the requirement. Karabiner issue #1058
    is that experiment shipped: VoiceOver "almost unusable". **IOHIDManager** can observe
    without root but can only suppress a whole device, and observing without suppressing
    gives double navigation — worse than today. **hidutil** is usage-to-usage only: no
    chords, no window scoping, and a crash leaves a blind user with dead arrow keys.
  - **VoiceOver's own extension points do not help either.** VO-Tab ("ignore the next
    keypress") is one combination, user-initiated, with no API for an app to request it —
    six keystrokes per navigation step. The **Keyboard Commander** can run an AppleScript,
    but its modifier is Option (not Control-Option), it is global rather than window-scoped,
    and it eats the Option character layer system-wide. Worth documenting as an optional
    "summon the overlay" key for somebody who asks; never as navigation, never as a default.
  - Still open, and cheap: whether the probe's silence was VoiceOver or our own matcher.
    The conclusion does not rest on it, and it must not cost a tester session. The tap now
    traces keys it saw and nobody claimed, and lists every event tap on the session at
    startup — both arrive in a log the tester was sending anyway.
- [x] **A module that would not load took the whole application with it** (2026-08-20).
      Found by the tester crashing on launch: my own sforzando probe registered
      `onActivate` after the overlay was bound, the runtime rejects that, and the startup
      loop propagated it. `load_module` already rolls back every registration a partial load
      made — the comment there called startup different "by design", and the design was
      wrong. A blind user whose application vanishes is left with a process that is gone and
      a log somebody has to talk them through finding, while the one place that could
      disable the offending module is the window that never opened. Now: a log line, an
      accessible modal from the same queue the module-error dialog drains, and the rest of
      the modules load.
- [x] **A module's Luau now gets loaded on a Mac before a tester sees it** (2026-08-20).
      `cargo check` compiles macOS Rust; it cannot execute a line of Luau, and a module
      branch inside `host.os.is("macos")` never runs on the machine this is written on — so
      a module can load cleanly on Windows for weeks and fail on the first real launch. The
      macOS job now runs the packaged app headless once and fails on "did not load", and
      `modules/**` is in its trigger list.
- [ ] **The VoiceOver transport is unmeasured, and the measurement has to answer one thing
      first** (2026-08-19): does `output` return when VoiceOver has the text, or block until
      the phrase has been spoken? If it blocks, per-line time is speech duration and NO
      change of transport buys anything — not a kept-compiled `NSAppleScript`, not a raw
      Apple Event, not a helper process. The log now carries the character count next to the
      milliseconds precisely to tell those apart, plus a one-off bare-`osascript` baseline
      that separates the launch from everything after it. Decision rule, set before the
      numbers arrive so it is not argued afterwards: **ms tracks length → close the
      question. Length-independent and under ~50 ms on a 2015 Air → close the question.
      Length-independent and over ~100 ms → build the direct Apple Event**, with the codes
      read from the `sdef` the tester is being asked for, on the existing worker thread,
      sent with `sendEventWithOptions:timeout:`.
  - Ruled out on evidence, not taste: **`NSAppleScript` is not usable here.** objc2 binds
    `executeAndReturnError` / `executeAppleEvent:error:` as returning a NON-optional
    `Retained<…>`, while the real API returns nil on script error — objc2 0.6.4 panics on a
    nil return, so the expected first-run case (AppleScript control not enabled) would take
    the process down instead of falling back. It is also conventionally main-thread-only,
    and this main thread carries the keyboard.
  - **Braille is unverifiable with the tester we have** (2026-08-20): he has no display.
    Whether VoiceOver's `output` reaches braille at all is the strongest single argument for
    the whole module, and nobody has observed it. Not a gap in the software — a gap in what
    anyone has seen. Step 4 of the briefing now says so rather than asking a question that
    cannot be answered, and asks the two things that CAN be: is it his own voice, and does an
    announcement cut VoiceOver off or queue behind it.
  - **The `sdef` is in** (2026-08-20). The tester installed the command line tools for his
    own reasons and ran it: `output` is event class `VOAS`, event ID `outp`, the text as the
    direct parameter, targeting bundle id `com.apple.VoiceOver`. Written up in
    docs/voiceover-scripting-codes.md along with the rest of the suite, because obtaining it
    cost a session. **It is not a decision to build the direct event** — the rule stands that
    the milliseconds decide, and those still have not been measured because the speech switch
    has never been ticked on a Mac.
  - Found in the same dictionary and worth its own line: `output` takes an optional `with`
    parameter of enumeration `spel`, whose values are **alphabetic** (`alpS`) and **phonetic**
    (`phoS`) spelling. A control could offer "spell this back to me" as a second key — which
    matters most exactly where OCR is least trustworthy, on preset names and file names, and
    which the platform's own voice cannot do at all.
- [x] **Speech now goes through VoiceOver** (2026-08-19). `crates/host/src/speech/` — one
      place decides where an announcement comes out, so no call site had to learn about it.
      On macOS the default is `tell application "VoiceOver" to output …` through `osascript`,
      which gets the user's own voice, their rate and **their braille display**; on a worker
      thread, because each line costs a process launch plus an AppleScript compile and this
      event loop also carries the keyboard. A refusal — VoiceOver not running, AppleScript
      control not allowed — is not silence: the line comes back and the platform's own voice
      says it, the reason is logged once, and everything after goes straight to the fallback
      without paying for another launch. **Speak through VoiceOver** in the Application
      settings tab turns it on — off until asked for, because the first line through it makes
      macOS raise an Automation consent dialog, and a dialog nobody asked for in front of
      somebody who cannot see it is not how an application should introduce itself. Ticking
      it again also re-tries a path that had failed (2026-08-20). The
      file is `#[path]`-borrowed by `crates/macos-check`, so the compiler checks it from
      Windows. What is still unmeasured: whether VoiceOver's `output` interrupts its own
      speech or queues behind it. Only a Mac can say.
- [x] **The module list was unreachable on macOS** (2026-08-19). Worse than the note here
      said: VoiceOver did not see one opaque element, it skipped the control. Cause read
      out of the wxWidgets 3.3.2 tree this repo builds — `include/wx/chkconf.h:1499` makes
      `wxUSE_ACCESSIBILITY` MSW-only and hard-sets it to 0 everywhere else, and
      `wxTreeCtrl` off MSW is `wxGenericTreeCtrl`, a scrolled window that paints its rows.
      Nothing to land on.
  - Fixed with a **platform split** behind `InstalledList` (crates/host/src/gui.rs): the
    native tree with `TVS_CHECKBOXES` stays on Windows, `wxCheckListBox` on macOS, and
    every caller sees an index.
  - One control for both was built first and tried with NVDA. `wxCheckListBox` on Windows
    is an owner-drawn listbox; wxWidgets 3.3.2 added a `wxCheckListBoxAccessible` that
    reports the checkbox role and state correctly but no child count, no item locations and
    no selected state. Measurably worse than the tree, and the user said so, so both stay.
  - Falls out of it: `sync_checks` and its shadow vector are gone, so the failure it
    guarded against — one unanswerable reading disabling every module — cannot happen; and
    macOS gets working Settings/Reload/Uninstall buttons, which were inert there because
    `native_checkboxes::same()` always answered `false`.
  - Still to confirm on a Mac: the six questions U1–U6 (per-row state, whether one utterance
    carries name AND checkbox, whether Space actually toggles).
- [ ] **Left standing after the adversarial review** (2026-08-13), each because the fix
      wants a measurement more than it wants a decision:
  - `focus_step` can in the worst case cost thousands of cross-process round trips per Tab
    press — it re-enumerates the ring on every step, which is a Windows-side invariant, and
    it times itself. Measure before optimising; the answer may be that the trees are small.
  - `walk` bounds nodes and depth but not TIME, and each node is a synchronous round trip
    whose own ceiling is the messaging timeout. A time bound is the right shape, but the
    number to give it is unmeasured.
  - The content-view probe caps the top inset at 64 pt and looks at the first eight children
    only, so a window with a toolbar above its content derives the wrong client rect. It
    logs the inset it found, which is how the first session can check it.
  - ~~Creating an `AXObserver` for an application on its first activation is six synchronous
    round trips on the pump thread.~~ Addressed (2026-08-19): it now happens **after** the
    frontmost window has been resolved and queued, so up to a second and a half of
    subscribing no longer sits in front of the answer the user is waiting for. It buys
    nothing for the switch happening now — it is about noticing the next change. Still six
    round trips, still once per application, but off the critical path.
- [ ] **Developer ID + notarisation.** Ad-hoc signatures change on every rebuild, so every
      test build a tester receives asks for its permissions again. Survivable for testing,
      not for release.

- [x] **The UIA tree walk was six process crossings per element** (2026-08-31). Measured on a
      53-element JUCE window: `uia_raw_dump` cost **211 ms**, because it asked the target for
      a name, a class, a control type and a rectangle one property at a time, and the walker
      asked for each child and sibling on top. A UIA **cache request** names the properties
      and the scope once and `BuildUpdatedCache` fetches the lot in a single call; the walk
      is then local. Same window, same output: **about 30 ms**. The raw view is kept as the
      tree filter — this function exists to see into hosted fragments the condition-based
      views stop at — and the old walk stays as the fallback for a provider that answers a
      subtree request with only its root.
  - Worth knowing for the next module: this made a whole layer of compensation in
    modules/ik-on-ear disappear — a snapshot, a staleness rule and an exception for the two
    read-outs that change on their own. All of it existed to work around the defect, and
    removing it left the module simpler AND less able to be stale.

## Documentation

- [x] **API-version-specific, web-based documentation:** a versioned **Docusaurus** site in `docs-site/` renders `docs/` — the guides plus the `host.*` API reference in `docs/api/` — with a version switcher (first snapshot `versioned_docs/version-0.1.0`) and a GitHub Pages deploy workflow (`.github/workflows/deploy-docs.yml`). Includes the step-by-step **[Building an overlay](docs/building-an-overlay.md)** tutorial, written because the vocabulary (cell, overlay, layer, slot, landmark) had grown faster than anything explained it. ✓
  - [ ] **Single Source of Truth:** the API reference is still written by hand, so it can drift from the implementation. Generate what can be generated from the capability catalog / host-API surface.
  - [ ] Cut a new docs version on each `engine_api` bump, so a module author reads the reference for *their* target version rather than the newest one.
  - [ ] **Every API entry needs a worked example, the way AutoHotkey's reference does.** Not a
        signature restated in prose but the call in use, in a `luau` block, taken from a real
        module where possible so it moves when the module does. Asked for on 2026-08-31, and it
        is a reading-order argument as much as a teaching one: a screen reader delivers a code
        block in one piece, while three paragraphs of prose leave the reader assembling the call
        from memory. Counted the same day — `window.md`, `ocr-input-sound.md`,
        `resource-settings-modules.md` and `uia-screen.md` are already there; the gap is
        **`overlay.md` (5 examples across 25 entries)** and
        **`speech-hotkey-keys-timer-log.md` (1 across 13)**.

## Further open points (from the feasibility study)

- [ ] Define hard **performance/footprint budgets** (hotkey latency in ms, baseline RAM, default binary size) — study §10.2.
- [ ] Clarify **macOS distribution/MDM details** (Developer ID/notarization flow, possibly PPPC profiles for enterprise) — study §10.4.
- [ ] Work out the **native-FFI security model** (out-of-process sandbox per OS, trust store/signature flow, macOS FFI policy) — study §11.1–11.2.

## ReaHotkey port — prerequisites & tooling

Foundation: [docs/reahotkey-port-analysis.md](docs/reahotkey-port-analysis.md).

- [ ] **Pre-Flight 1:** Verify the window/plugin frame on macOS. [VOCR](docs/prior-art-vocr.md) demonstrates: the AX frame (`AXPosition`/`AXSize`) of the focused app window is reliable even for non-accessible plugin windows and serves as the capture/coordinate origin. What remains to be checked: sub-view frame of an *embedded* plugin vs. floating window (REAPER can show plugins as floating windows → simplest case).
- [ ] **Pre-Flight 2:** macOS TCC onboarding (Accessibility + Screen Recording, Input Monitoring only with CGEventTap). **Hotkey strategy per VOCR:** registered hotkeys via Carbon `RegisterEventHotKey` + context scopes → **no** CGEventTap/Input Monitoring/silent-disable needed. CGEventTap + health watchdog (`tapIsEnabled`/`tapEnable`) only if suppression/remapping/hotstrings are required. Permission persistence across self-updates (stable team/bundle ID). Study P0-2 + P0-9.
- [x] **Overlay calibrator (Windows):** armed with `AUTOMATION_PLATFORM_CALIBRATE=1` and captured (not registered), so the keys belong to whichever overlay is ACTIVE — the only thing that can answer where its own controls land, since the coordinate frame (origin, frame offset, `rawOrigin`, landmark anchoring) is its own. **Ctrl+Alt+Shift+S** writes a screenshot of the coordinate window with a crosshair on every control plus the pixel read there, and logs the active window, every control in enumeration order with the chosen origin marked, and the origin's named accessibility elements; **+T** crops a template around the focused control straight into the module; **+V** counts that template's matches. It replaced a workflow of three-to-six rounds of throwaway diagnostics per coordinate, and the roster/tree half exists because a wrong ORIGIN is invisible in a list of coordinates — they are all faithfully wrong together. ✓ (2026-07-27)
  - [ ] The macOS counterpart, once there is a macOS backend to calibrate against.
- [ ] Concretize the **capture+OCR performance budget** (max capture frequency, ROI size) — GTune polls at 4 Hz; more expensive under ScreenCaptureKit than Windows `PixelGetColor`. Part of study §10.2.
- [ ] **MVP vertical-slice order:** runtime framework → FabFilter (1 hotspot) → GTune (OCR+timer) → u-he bundle (Diva/Hive2/Repro/Zebra, possibly 1 generic "u-he header" module) → later Engine2/Raum/Zampler/KompleteKontrol. **Dubler 2 family = very involved** (image-/coordinate-heavy; advanced audio routing via ASIO/BASSASIO — per the client also available for Mac), deliberately deferred.

## ReaHotkey port — sample libraries (in progress)

ReaHotkey keeps 19 unported product overlays in `Includes/Overlays/KontaktKompleteKontrol/`
— `AudioImperia.ahk` (11 products), `Soundiron.ahk` (3), `ImpactSoundworks.ahk` (2),
`Audiobro.ahk` (1), `CinematicStudioSeries.ahk` (2) and `NoLibraryProduct.ahk` (a "None"
fallback). Each file is `#IncludeAgain`d three times, once per host, with a per-host offset
table; **all of those tables vanish for us** because our library overlays are
landmark-anchored, and `kontakt.library()` already produces the six cells of the Kontakt
matrix. This is the cheapest remaining content on the backlog: a product is a data row.

- [x] **Cinematic Studio Series restructured to a product table** — the module (renamed from
      `cinematic-studio-strings`, id `com.platform.cinematic-studio-series`) now declares
      products as rows and builds their controls from one function. Strings unchanged and
      still measured. ✓ (2026-07-27)
- [x] **Cinematic Studio Brass measured and verified** — four channels (Close/Main/Room/Mix)
      at wordmark x {-144,-94,-44,+6}, y +334, lit (234,167,164) against unlit (92,88,85).
      Two findings worth keeping: the derived x was 25 px out although the derived y was
      exact (the two products centre their mixer differently under the same wordmark — so a
      sibling's numbers give the SHAPE, never the offsets); and ReaHotkey's separate image
      toggle for Brass's mix position points at empty background in this UI, while Mix's own
      ⏻ sits on the mixer row and reads like the other three. ✓ (2026-07-27)
- **What is actually installed here** (checked against the NI registry, which is what
  settles the order — an overlay nobody can put on screen cannot be verified, and every
  failure in this tool is silent): Audio Imperia complete (Areia, Cerberus, Chorus, Dolce,
  Glade Studio, Jaeger, Nucleus, Solo, Talos), Soundiron (Voices Of Gaia, Mimi Page Light
  and Shadow, the three Voice Of Wind titles), Impact Soundworks Juggernaut, Cinematic
  Studio Brass + Strings. **Audiobro LA Scoring Strings 3 is NOT installed** — so the
  "smallest second vendor" is out.
- **How a product gets its numbers.** ReaHotkey's are plugin-relative and have drifted;
  ours are wordmark-relative. Deriving one product's offsets from a sibling's was tried on
  Brass and was 25 px out. The workflow instead: ONE calibration shot of Kontakt with the
  library loaded — Kontakt's own header is enough, the library overlay need not exist yet —
  and the wordmark is then located in that shot with ReaHotkey's own `Product.png` template,
  which gives every offset without a guess and without shipping a control that clicks
  somewhere unverified.
- [x] **Impact Soundworks / Juggernaut** ✓ (2026-07-27) — two OVERLAYS, not two variants of
      one: a bass synth with a step sequencer and an eight-channel drum mixer, sharing a
      name. ReaHotkey identifies them by the loaded patch's name as KONTAKT prints it, which
      detects fine and anchors nothing; the library's own artwork says the same thing inside
      its own panel, so the upper word of the wordmark ("BASS" / "DRUMS/FX") is the landmark,
      each verified to find nothing in the other patch's shot. The preset field's OCR region
      stops short of its up/down arrows on purpose — an OCRButton CLICKS what it reads, and a
      click on an arrow would step the preset instead of opening the browser.
      **Impact Soundworks complete.**
- [x] **Soundiron — Voices Of Gaia** ✓ (2026-07-28), and the design it forced is the point.
      ReaHotkey opens the FX rack first via `ClickFXRack`, bound to the toggle TWICE (before
      focus AND before activation) because a state read taken while the rack is closed is not
      "off", it is meaningless. The runtime grew `reveal` for exactly that — and a calibration
      shot then showed it cannot be used here: the FX-rack page does not EXPAND, it REPLACES,
      so the artwork wordmark that identifies the library and anchors every coordinate is gone
      precisely when it is needed. Measured: of the library area only Kontakt's top bar and
      the strip below the tabs are pixel-identical across the pages; nothing product-specific
      survives. So it is ONE OVERLAY PER PAGE (the Cerberus pattern), each gated on a landmark
      that is actually on its page, Alt+R switching both ways because only one is ever active.
      The reverb needs no image templates at all: lit (215,21,20) against unlit (113,52,46) is
      159 apart and the whole 7x7 area separates, so one pixel is safer AND cheaper than a
      region capture with two templates.
- [x] **Soundiron — Mimi Page Light & Shadow** ✓ (2026-07-28), and ReaHotkey's control for it
      could NOT be ported: there is no FX page in this version at all. The instrument panel
      ends at y 570 with the keyboard directly below, there is no tab row, and no scrollbar
      (the right edge is a uniform 0x1a top to bottom), so ReaHotkey's rack probe at y 652
      would land in the keyboard. A third layout, not the second — `reveal` is still unused.
      Instead of shipping a control that clicks into a keyboard, the overlay offers the one
      piece of state otherwise unreachable: the articulation field ("AH (LIGHT)"), which alone
      says which vowel and dynamic layer is loaded. Beyond ReaHotkey and deliberately so — a
      substitution for a control that cannot be ported, not an expansion of scope.
- [x] **A `readOnly` OCR control no longer announces itself as a "button"** ✓ — it suppressed
      the click but kept the word, which promises an affordance that is not there: you hear
      "button", press it, nothing happens, and the only honest reading of that is a broken
      tool. Announced like a static text now, with no control type. The third variant of one
      mistake in a day, after a hint pointing at a path that did not work and a toggle claiming
      a state it could not read — **a control must not claim anything about itself it cannot
      honour.** (Also worth keeping: `ocrLabel` was the wrong tool here. It reads a control's
      NAME off the screen and REPLACES the fixed one, for controls whose meaning the plugin
      changes — Cinematic Studio's mic switches. Here the name is fixed and the value moves.)
- [ ] **Soundiron — the Voices of Wind family** is installed (Adey / Audrey / Kimba as separate
      NI libraries, where ReaHotkey has one "Voices of Wind Collection" overlay) but unmeasured
      and deliberately not declared. Its ReaHotkey rack probe sits at y 663, which the Mimi
      Page finding makes suspect for the same reason.
- [x] **Runtime: `reveal` on a graphical toggle** ✓ — the point whose colour says a panel is
      shut and which also opens it, plus a settle delay. Asynchronous (we cannot sleep through
      ReaHotkey's 250 ms), so the continuation re-checks that the overlay is still active on
      the SAME window; fails OPEN, because a control that silently stops responding is worse
      than one that clicks an already-open panel. Unused so far — see above.
- [ ] **A tab row need not sit at the same height on every page it appears on.** Kontakt's
      does not: y 504..522 on the Performance page, y 497..512 on the FX rack. A click authored
      from the wrong page's shot landed 9 px low on background and did nothing — and the same
      slip produced a FALSE MEASUREMENT, reading the background under a tab and concluding the
      tab never changes colour. Worth remembering as a class: when two states of one UI are
      being measured, a coordinate taken from the other state's picture is not an
      approximation, it is a different place.
- [x] **Audio Imperia — module started, Chorus measured and verified** (`modules/audio-imperia`,
      `com.platform.audio-imperia`). Its wordmark templates still match the current UI
      exactly, unlike its coordinates. ✓ (2026-07-27)
- [x] **Areia measured and verified** — wordmark (972,55); mixer row +169..+192 with two
      segments at -562..-483 and -481..-398; mic-blend track -570..-390 at +105..+133 with
      the thumb's travel from -554 to -406. ✓ (2026-07-27)
- [x] **Audio Imperia — Areia, Chorus, Solo, Talos, Jaeger** (5 of 11). The panel turns out
      to be SHARED: measured across five shots, the mixer row is always y 224–247 with
      segments starting at x 410 and 491, and the mic blend's track is always x 410–574 at
      y 166–180 — every product's mix label matched at exactly (420,232) or (501,232). Only
      the wordmark moves, so the module states the panel once in plugin coordinates and each
      product contributes just the wordmark position it was measured at. The slider knob is
      one shared asset too: Areia's template pair matches Jaeger's and Talos's exactly.
      ✓ (2026-07-27)
- [x] **Audio Imperia — complete** (2026-07-27).
  - [x] **Dolce** ✓ (2026-07-27) — and the way it got in is the point. Its own templates are
        stale, and cropping fresh ones would have meant asking a blind user to click the very
        buttons this overlay exists to make clickable. The two-segment products' labels turn
        out to be rendered identically (ReaHotkey ships four BYTE-IDENTICAL copies), so a
        sibling's templates serve, the six files are now shared, and no second shot was
        needed. **Look for that before asking for a state change** — see the memory note.
  - [x] **Glade** and **Nucleus** ✓ (2026-07-27) — they use Audio Imperia's OTHER layout (a
        1353 px window with Basic/Advanced tabs) and their mix selector is two large labelled
        boxes rather than a segmented strip. Measured from shots of the patches that actually
        have it ("02 Pyramid Instruments / 02 Low Strings", "Nucleus - 01 10 Violas"); the
        Designer patches carry no mixer at all. Both landmarks are OUR crops of the wordmark
        glyphs — ReaHotkey's include the artwork behind them and stop matching when the patch
        changes it. Glade's blend track (455–591, thumb centre 523) reads exactly 50 %.
        ReaHotkey announces no state for these two; we do, from a single shot, because both
        boxes contain the word "Mix" and only its brightness differs — one template pair
        serves both buttons.
  - [x] **Cerberus — Epic Mixes variant** ✓ (2026-07-27), and WITHOUT the list control and
        control-swapping the backlog said it needed. Cerberus's panel depends on the loaded
        patch — an "Epic Mixes" one has three mic faders (C / F / R), an ordinary one two
        (C / M) — and ReaHotkey handles that by ASKING: a list box where the user declares
        which patch they loaded, which then replaces the control set. We do not have to ask.
        The letter row IS the difference, so it is the landmark: an Epic Mixes patch matches
        it and gets those three controls, anything else matches nothing and is offered
        nothing, rather than three buttons aimed at the wrong faders. It anchors them too, so
        their offsets are measured from themselves. Verified: the template matches exactly
        once in the whole plugin.
  - [x] **Cerberus — ordinary variant** ✓ (2026-07-27). Six mic faders (TD BC MS D W F) plus
        a separate C | M pair; the six grey labels are the landmark, verified to match once
        here and NOT at all in the Epic Mixes shot, which is what stops the two variants
        claiming each other's window. Only C and M are offered, as in ReaHotkey — the six
        drum mics beside them would be a fair addition but that is a scope decision, not a
        measurement.
  - **Audio Imperia is complete**: all eleven product overlays across two panel layouts and
    two Cerberus patch families.
  - [ ] **A list control and runtime control-swapping are no longer needed for Audio
        Imperia** — but Zampler's four `OCRListBox`es and Dubler's `PopulatedListBox` still
        want the list, and arrow-key stepping now exists (the slider owns Left/Right), so
        that is the natural place to build on when either is ported.

## Kontakt — open threads from the Soundiron port (2026-07-28)

- [ ] **The status-bar toggles must report their STATE, not just act.** Side pane, info pane
      and keyboard are toggles announced as plain buttons, so pressing one tells you nothing
      about which way it went — the same defect as a `readOnly` OCR control calling itself a
      button, and the same rule: a control must not be less informative than what it does.
      Wanted as checked/unchecked or expanded/collapsed. A cheap and reliable source is
      already to hand: each panel changes the PLUGIN WINDOW'S OWN SIZE, measured at 1010x679
      with the side pane open against 664x679 with it closed, and the keyboard changes the
      height the same way. So the state is readable from the origin rather than from a pixel
      or a UIA property — no new measurement, and nothing that drifts with a theme. Verify
      before building: a window resized for another reason must not read as a toggled panel.
- [ ] **In the instrument EDIT view the play-view overlay is shown.** The view probe reads
      Edit Mode as "classic" in a calibration shot (brightness 99), so this is not simply the
      probe being wrong — Edit Mode is a THIRD state the two-way classic/play question cannot
      express. The likely answer is the one the user proposed: its own overlay, which also
      gives the collapsed Insert / Send / Main Effects sections somewhere to live.
- [x] **ReaHotkey's Soundiron probes fit a TALLER Kontakt window.** CONFIRMED, 2026-07-28. The
      hypothesis was right in full: Alt+9 grew the window from 664x679 to 664x965 and the
      "Performance | FX Rack" row appeared at y 675..694 — ReaHotkey's published 663 plus the
      Kontakt 8 offset of 29 is 692, inside it. Adey now has both pages and its reverb; see
      below for what is still owed.

## Kontakt — growing the window was one-way (2026-07-28)

- [x] **FIXED by clamping, after three other routes were measured and failed.** Pressing
      "taller" twice took the plug-in from 679 to 965 px, and at 965 the grip was unreachable:
      the plug-in's view ended at y 1069, the host's own window content at ~1024, and the
      taskbar began at ~1030, so the grip — 21 px in from the plug-in's bottom-right corner —
      had passed under both. What did NOT work, each measured from window rectangles: resizing
      the host's FX window (it shrank to 812 and the plug-in stayed 965 — the host passes no
      size down); `SetWindowPos` on Kontakt's own Qt window (ignored, snapped straight back);
      a drag aimed where the grip should have been (it went to the taskbar). Kontakt resizes
      through that grip and nothing else. The fix is therefore to never let it leave the
      screen: `host.screen.workArea()` was added to the backend and the growth step is clamped
      to keep the grip inside it, refusing with a spoken reason when there is no room.
      Resize controls are also gated on classic view now (8 of 27 shots have a grip, all
      classic; 13 play-view shots at the same height have none).
- [ ] **The window currently open is still 965 px and still stuck.** The clamp prevents this
      state, it does not undo it. Recovering an already-stuck window needs the host window
      lifted so the corner clears the taskbar, the grip dragged, and the host put back — which
      our own app can do (it acts only while the plug-in has focus, so it owns the foreground)
      but an outside script cannot: `SetForegroundWindow` from a background process is refused
      by Windows, which is what stopped the attempt. Only worth building if the state recurs.

## Soundiron — owed after Adey's reverb (2026-07-28)

- [x] **Mimi Page's reverb — DONE, and it changed the design.** Its FX rack turned out to be
      Adey's page to the pixel (tile at y 383, reverb at y 500, the same (-84,+117) between
      them; the only difference between the two captures is the 346 px an open browser shifts
      everything). The overlay gated on that page's collage therefore announced "Voice Of Wind
      Adey, FX rack" over an instrument rack reading "Mimi Page Legato". Fixed by making it ONE
      shared overlay named "Soundiron, FX rack" — the page carries no product identity, so the
      overlay must not claim one. Which library is loaded is on Kontakt's own instrument header,
      already read by the header overlay.
- [x] **One shared Soundiron FX-rack overlay instead of one per product?** Answered by the
      above, and not as a matter of taste: per product was WRONG, not merely redundant. Voices
      Of Gaia keeps its own because it is a different layout (7x17 reverb template at another
      offset), which is also why its tile does not match Adey's — the one comparison that had
      been mistaken for evidence that collages are per product.
- [x] **Voices of Wind Audrey and Kimba — DONE, and cut a different way on purpose.** All three
      Wind products draw "Voice of Wind" identically and differ only in the name below it, so a
      landmark around the shared line would match all three and one around the artwork would
      depend on the artwork — which is exactly what had just failed on Mimi Page. Both are cut
      around the NAME alone and masked to its strokes: Audrey 2007 px (blue dominant by >80,
      text reads (91,117,243)), Kimba 2201 px (darker than 100 on pale blue). One exact match
      each across 30 shots. They also get the articulation read-out Adey cannot have — same
      field, drawn here in plain sans-serif instead of handwriting, so OCR is the right tool
      rather than the wrong one.
- [ ] **Adey's own landmark is still 200x165 of pure watercolour.** It works and is verified not
      to match its siblings, but it is the one shape this file now argues against, and it has no
      lettering to fall back on: the name "Adey" is pink script over pink watercolour, which is
      why the artwork route was taken in the first place. Re-cutting it around the name with a
      colour mask is the obvious try; if the contrast is too low, this is the library that
      justifies keeping an artwork landmark and saying so.
- [x] **Is the FX-slot artwork stable? YES — across products AND patches.** Checked on a
      second patch ("Mimi Page Phrases 140BPM") after the first patch of the same library had
      just broken the Performance-page wordmark, so the question was live. The whole FX-rack
      page is byte-identical: tile at (761,383), reverb at (677,500), Performance tab at
      (354,605), the same numbers Mimi Page Legato gives. Nothing here needs masking.
- [ ] **Artwork is not a landmark — check every library gated on one.** Two failures, each
      worse than the last, and both reported by the user rather than noticed here.

      Mimi Page: a second patch drew the backdrop ~20 brighter, putting 29 of 17480 pixels
      outside tolerance while the LETTERING stayed identical to the byte. Fixed by masking to
      pixels brighter than 180.

      Voices Of Gaia: a second singer RECOLOURS THE WHOLE GUI — gold for Francesca, silver for
      Bryn, same shapes throughout. 80% of the wordmark outside tolerance, and masking to the
      brightest makes it worse (1174 of 1174 fail), because the bright pixels are the recoloured
      ones. Fixed by anchoring on the red "SUSTAIN:" caption instead: of its 182 red-dominant
      pixels, zero differ across the two schemes.

      The rule that comes out of both: a landmark must be the part a vendor does not restyle —
      lettering rather than picture, and a fixed accent colour rather than a themed one. Still
      unchecked on a second patch: Audio Imperia's eleven, Cinematic Studio's two, and Adey,
      whose landmark is 33000 px of pure watercolour with no lettering to retreat to.
      Ranked by artwork share, worst first: Talos 94%, Nucleus 94%, CSS Strings 92%, Solo 87%,
      Chorus 83%, Dolce 81%, CSB 81%, Glade 77%, Areia 76%, Jaeger 42%.

      Note what does NOT need this: Gaia's FX-rack tile matches a Bryn patch unchanged, because
      that page is flat Kontakt chrome with fixed slot graphics rather than the singer artwork.
      The exposure is specific to a library's own themed page.
- [ ] **The hazard is halved, not gone.** A pump iteration still exceeds ~300 ms on a window
      switch. What is left is neither the OS calls (1 question per epoch reaches the system),
      nor Lua table conversion (0.0 ms at microsecond resolution across 5077 calls), nor
      pixels (zero), nor the arbiter — it is the COUNT of Lua/Rust crossings inside
      `pluginControl`. The next lever is fewer crossings there, not cheaper ones.
- [ ] **The runtime is a code module, so per-VM state multiplies.** The scene is computed
      once per VM per epoch, not once globally — three Kontakt-using modules, three
      computations. That is the price of the inheritance model, and it caps every
      module-level memo the same way. A host-side scene would not have it.
- [ ] **Instrumentation to keep.** The pump reports its phases and event multiplicity, each
      epoch its hit ratio and pixel count, each dispatch its size, each recheck its two
      halves. It exists because SEVEN reasoned guesses about this were wrong in a row
      (arbiter, timer resolution, uncoalesced events, template cache, table conversion,
      pixels, pluginLocate) and only measuring inside the loop ended it. One near-miss worth
      remembering: accumulating per-call time with `as_millis()` rounded every sub-millisecond
      cost to zero, and eighteen hundred zeroes summed to zero — a unit too coarse looks
      exactly like evidence of absence.

## AutoHotkey translation audit (2026-07-27)

A 14-agent audit of every ported literal against its ReaHotkey original, across five lenses
(class names, colours, coordinate origins, image/OCR semantics, keys and timing). 22
candidates, 8 adversarially verified, 3 confirmed. The rule it produced, worth keeping:
**a literal copied from AutoHotkey is a claim about AHK's semantics, not a value — port the
meaning, then re-derive the number.** Each defect was the same shape: a token valid in AHK's
frame of reference became invalid in ours, and the runtime answered with silence or a
plausible wrong action rather than an error.

- [x] **Komplete Kontrol's standalone menu bar was entirely unreachable** ✓ (2026-07-27) —
      its five hotspots are authored at client-relative `y = -10`, which is legitimate: a
      window's client origin sits BELOW its caption and native menu bar, so addressing that
      bar means a negative y. `pointInOrigin` began the legal rectangle AT the client origin,
      so all five were refused before clicking, saying "File menu is not where it should be"
      — a message that reads like version drift and points nowhere near the cause. Worse,
      the overlay's `RegisterHotKey` claims outrank the application, so Alt+F/E/V/C/H were
      swallowed too: the last route to that menu bar without us. Now tested against the
      window's FRAME, which also fixes an existing mismatch (client ORIGIN paired with frame
      SIZE, overshooting the right and bottom edges by a border width on any window that has
      one). The case the guard was written for still holds: Kontakt 7's arrow at x=704
      against a 649 px window is outside the frame too.
- [ ] **Verify KK standalone's `-10` itself against a real window** — the guard was the bug,
      but the literal is still unconfirmed: Komplete Kontrol's application is not installed
      here, so nobody has checked that its menu bar is genuinely non-client rather than a Qt
      widget inside the client area. If it is a widget, the guard fix stands and the module's
      y needs re-measuring rather than copying. Re-measure x = 16/52/83/138/194 at the same
      time.
- [x] **Impact Soundworks' invented Alt+P** ✓ (2026-07-27) — ReaHotkey binds NO key on
      either Juggernaut OCRButton; ours added Alt+P, which the inherited Kontakt header
      already owns for "Previous snapshot". A doubly-claimed spec resolves to the first
      VISIBLE claimant and the header is declared first, so the key stepped a snapshot —
      changing the loaded sound — instead of reading the preset, in 4 of 6 cells. Removed.
- [x] **An open plug-in menu now takes ALL of the overlay's keys, not just the navigation
      ones** ✓ (2026-07-27) — ReaHotkey's `SetHotkeyMode(…, 0)` means "this overlay owns no
      keys at all"; ours meant "let the nav keys through". The per-control hotkeys are
      `RegisterHotKey` claims, and those OUTRANK the application, so with Kontakt's file or
      snapshot menu up, Alt+P and Alt+M never reached the menu — the overlay ate them and
      re-ran its own action, acknowledging a keystroke the menu never received.
      Their REGISTRATION is dropped, not merely their effect: ignoring the press would still
      swallow the key, which is indistinguishable from a broken tool. Unregistered, it
      reaches the menu. Set eagerly at press time via a declared `opensMenu`, since the
      150 ms poll is too slow to be the only answer (ReaHotkey does the same at
      Kontakt8.ahk:97); the poll is what lowers it again, and an overlay that is not
      menu-watched gets a one-shot restore so a suspension can never strand.
      Declared on the controls that hand a menu to the USER — Kontakt's snapshot and file
      menus, KK's hamburger and its five standalone menu-bar buttons, u-he's logo and preset
      menus — but NOT on the three that drive a menu themselves and close it again.
- [x] **The shared-hotkey invariant is enforced when an overlay is DECLARED** ✓
      (2026-07-27) — two controls may share a spec only when their `when` predicates are
      mutually exclusive; a claimant with no `when` is visible always and therefore always
      overlaps. Verified against the defect it was written for by reintroducing Alt+P: it
      names both controls and fires on exactly the four cells where both exist, staying
      silent on the two in-KK cells where the Kontakt header is not present — which is
      independently the split the audit derived. Two `when`-gated claimants are deliberately
      NOT reported: whether two predicates are exclusive cannot be decided here, and guessing
      would flag every legitimate case (KK's Alt+M against Kontakt's, Alt+V's two view
      toggles). The general form of this is worth keeping in mind: a guard that can refuse an
      author's value should fail loudly at declaration, not quietly at press time — a blind
      user cannot tell "refused" from "not implemented".
- [ ] **14 of the 22 candidates were never verified** (budget cap), so they are not cleared,
      only unexamined. Full run under `subagents/workflows/wf_b83b2311-d3a/journal.jsonl`.

## ReaHotkey port — plug-in headers (u-he)

The first ports that are not sample libraries inside Kontakt. A u-he synth is an ordinary
VST with a window class of its own, which makes it the simplest case the runtime has: the
class IS the identity, so there is no landmark, no `identify`, and nothing to disambiguate.

- [x] **u-he — Diva, Hive 2, Repro-1, Zebra2, ZebraHZ from one table** (`modules/u-he`,
      `com.platform.u-he`). ReaHotkey's four files (`Diva.ahk`, `Hive2.ahk`, `Repro.ahk`,
      `ZebraLegacy.ahk`) are token for token the same file apart from a class, a name and
      six coordinates, so only the measurements live per synth. All five load clean.
      Coordinates are ReaHotkey's, still to be verified against calibration shots.
- [x] **Runtime: a hotspot toggle REFUSES to report an unrecognised pixel** — it used to
      answer "whichever reference is nearer", which always has an answer, including for a
      pixel resembling neither state. These probes are single pixels and Diva's lands on the
      antialiased edge of a glyph stroke, so a rescale or a font change would have it read
      background and announce a confident, wrong state — the worst thing this system can do
      to someone who cannot check it. The threshold is self-calibrating (half the distance
      between the two references: 109 apart on Diva, 334 on Cinematic Studio, so no fixed
      tolerance could serve both) and a refusal is logged. `onColor`/`offColor` now also take
      a LIST, as ReaHotkey's OnColors/OffColors always could.
- [x] **Runtime: calibration shots survive a restart, and record who has the keyboard** —
      the shot counter lived only in memory, so a restart between two shots overwrote the
      first (it ate the first Diva shot). It now counts what is on disk, via a new
      `host.resource.exists` (`read` cannot answer this for a PNG — it decodes as UTF-8 and
      fails either way). The context dump also prints the focus chain with the overlay's
      origin marked: every control here is driven by synthetic mouse clicks, so an overlay
      can be wholly correct while the plug-in never receives a key, and nothing said so.
- [ ] **Calibration shots land in `modules/overlay-runtime/calibration/`**, not under the
      module that declared the overlay — `host.path`/`host.screen.save` resolve against the
      module whose code is RUNNING, which for the shared calibrator is always the runtime.
      Harmless but wrong, and with nine modules everything piles into one directory. The doc
      comment above the calibration section claims otherwise.
- [x] **Runtime: a state `hint` on a toggle** — ReaHotkey's `ExecuteOnActivationPostSpeech`,
      which all three u-he browser toggles use for one sentence: switching the preset
      browser on is half the information, because nothing on screen says the arrow keys now
      pick presets. Keyed BY STATE (`hint = { on = "…" }`) and spoken on ACTIVATION only —
      a hint repeated on every Tab pass is noise that says nothing new. Works on both toggle
      kinds; the declaration check rejects it anywhere else, and rejects a mistyped key,
      because a hint that is never spoken fails silently.
- [x] **Diva measured and verified** ✓ (2026-07-27) — 1200x670, Diva 1.4.5. All five
      positions land where ReaHotkey says, and BOTH toggle colours read back exactly (unlit
      126,128,134 = 0x7E8086, lit 170,168,159 = 0xAAA89F), which also settles that AHK v2's
      PixelGetColor is plain RGB. One number corrected: ReaHotkey's preset region ends at
      y 44 and cuts the descenders off the name — fine for Tesseract, not for an OCR that
      crops to the glyphs and upscales first. Widened to y 20–48, inside the pill's own dark
      interior (y 16–54).
- [x] **The preset menu is a NATIVE context menu** ✓ (2026-07-27, live on Diva and Repro-1),
      which makes the OCR preset control the accessible path to every preset — a screen
      reader reads a real Windows menu without help. `menus = true` is therefore load-bearing
      here, not precautionary.
- [x] **ReaHotkey's "use the arrow keys to select a preset" is not true here** ✓ — with the
      PRESETS page open the arrows select nothing (and they are not ours: the runtime
      captures Left/Right only for a focused slider). Our hint names the preset menu instead.
      Repeating an instruction that silently fails is worse than saying nothing to someone
      who cannot see that it failed.
- [x] **All five synths validated live** ✓ (2026-07-27) — Diva, Repro-1, Hive 2, Zebra2 and
      ZebraHZ each detect, announce and operate. ReaHotkey's coordinates transfer unchanged
      for all of them, which is worth recording: the four files really were one file.
- [ ] **Only Diva has been measured against a calibration shot.** The other four are
      validated by USE, which is strong evidence for anything audible (the preset name reads
      back, prev/next change the sound, the toggle reports a state) and none at all for a
      control whose miss is silent — a Save that lands one button over does not announce
      itself. Worth one shot each when convenient.
- [ ] **Every u-he coordinate assumes the DEFAULT window size.** These plug-ins have a
      70–200 % zoom, and at any other setting the whole header scales. No landmark fixes
      that: anchoring moves a coordinate frame, it does not scale one. Nothing detects the
      condition today, so it would present as an overlay whose controls all miss.
- [ ] **Repro-5** — ReaHotkey registers only Repro-1's class but then image-checks which of
      the two is on screen before offering the browser toggle, which only makes sense if
      the class does not always discriminate them. Unsettleable here: Repro-5 is not
      installed, so its class, layout and toggle colours are all unmeasured. If one ever
      turns up under Repro-1's class, that toggle needs a Repro-1 landmark gate first.
- [ ] **u-he inside Komplete Kontrol** — these are NKS-ready and ReaHotkey supports them
      there. Unlike a nested Kontakt (a UIA leaf needing a content frame), a u-he plug-in
      keeps its own HWND, so `EnumChildWindows` would find it under a KK window and its own
      client rect is already the right origin — the work is the ARBITER: it has to join KK's
      shared slot above KK's chrome, and a ranking mistake there is only visible against a
      real window.
- [ ] **Two plug-ins of one family in one FX chain** are children of the same host window,
      so both overlays match it. The shared slot means the arbiter picks one rather than
      leaving two overlays fighting over Tab, but which one is arbitrary. The same ambiguity
      exists across modules (a Kontakt and a Diva in one chain) and is inherent in matching
      on the host window rather than on which plug-in has keyboard focus.
- [ ] **Raum** (free, installed) and **GTune** (free, installed as VST2) are the small
      remaining ReaHotkey plug-in overlays. GTune is the one that settles whether we carry a
      SELF-UPDATING read-out (it polls at 2.5 Hz); every future level/meter display wants
      that answer.

## ReaHotkey port — Kontakt (open threads)

The Kontakt overlays cover {Kontakt 7, Kontakt 8} × {bare in a DAW, nested in Komplete
Kontrol, standalone}, with Cinematic Studio Strings inheriting on top. What is left:

- [ ] **Kontakt 7's multi arrows.** Not offered. ReaHotkey's `(704, 53)` points 55 px past the right edge of a 649 px window, and the real selector is the ▲▼ pair in Kontakt 7's TOP bar (around x 324, y 14 and 25) — a different control elsewhere. Switching multi unloads the loaded instrument, so this wants measuring, not inferring.
- [ ] **Kontakt 7's single-instrument view is unverified.** `detect.isRackView` ports ReaHotkey's test (`GetPluginView`: find the SHOP button, its previous sibling is "VIEW" ⇒ rack) as a UIA lookup for the VIEW button, and deliberately FAILS OPEN — "the plugin does not answer UIA" counts as rack, because a wrong "no" silently hides all seven rack controls, which is the bug that made this necessary. Only ever seen in the rack state; nobody has looked at what Kontakt 7 shows in the single view.
- [ ] **Previous/Next snapshot cannot confirm they did anything.** They acknowledge the press, and the dropdown checks that a menu actually opened, but the arrows have no post-condition. Reading the snapshot name field by OCR before and after would both confirm the step and let the control announce the new snapshot's name — which is the announcement a screen-reader user actually wants. Needs one look at what Kontakt puts in that field when an instrument has no snapshots.
- [ ] **`daw-hosts` accepts any `#32770` of reaper.exe as a plugin host,** because REAPER's FX window is one. So is every dialog a natively-hosted plugin opens: Kontakt's "Content Missing" passed as a host, and the Qt window drawing it passed as the plugin, until `detect.inOwnDialog` shut that door for Kontakt specifically (by title, as ReaHotkey does). The same mechanism is still open for every other module on `daw.all` — Komplete Kontrol has its own dialogs. Unverified, but it is the same mechanism, not a different one.
- [ ] **Kontakt 8's view toggle could use UIA where it is reachable.** Standalone, a raw walk lists "Play View" as a real Button; we send F10 everywhere instead. F10 works and cannot drift the way a measured menu row can, so this is a nicety, not a fix.

## Dev tools

- [x] **OCR window inspector (first version):** `tools/inspect` — **Ctrl+Alt+I** OCRs the focused window's client area and logs every recognized word with its **client-relative coordinates** (+ saves the capture with `AUTOMATION_PLATFORM_OCR_DEBUG=1`). Calibrates overlay regions and reveals where hardcoded (e.g. ReaHotkey) coordinates land vs the real controls. Resolved the sforzando polyphony case (the region was correct; the failures were the hover scrub-value — fixed by `hoverToRead`-off — and UWP OCR being blind to *single* digits). ✓ (2026-06-21)
  - Follow-up: the single-digit OCR gap is now fixed by the embedded PaddleOCR fallback (see the OCR-engine entry above). Still open: label-relative OCR controls (find "POLY.", read right) to be robust against window-width shifts.
- [ ] **Window/control inspector (full):** smaller than it reads — the calibrator above now answers most of it for the ACTIVE overlay (window identity, the full control roster with geometry, the origin's accessibility tree, pixel reads at every control). What is genuinely missing is the *live, cursor-driven* half: the element under the cursor and a freeze key. Extend to the element under the cursor (AX/UIA role/value), live mouse position + pixel colour, freeze hotkey — à la AHK *Window Spy*, **inspection only**. Shows live, for the window/element under the cursor or the focused one:
  - **Window:** title, app (`name`/`bundleId`/`exe`/`pid`), OS parameters (Windows: Win32 class; macOS: AX role/subrole/identifier), `bounds`, id.
  - **Control/element under the cursor:** class (`ClassNN`) or AX role/subrole/identifier/value, geometry **relative to the window/plugin control** (for coordinate calibration), AX-tree path.
  - **Mouse:** position absolute + relative to the focused window/plugin control; **pixel color** under the cursor.
  - Live update on mouse movement (Window Spy style) + hotkey to "freeze"/copy the values.
  - **Purpose:** delivers exactly the parameters that the WindowMatcher ([docs/window-matching.md](docs/window-matching.md)) needs per OS, as well as coordinates/colors for calibration. Cross-platform (Win + macOS).
  - Built on the first-class primitives themselves (`host.window`/`host.a11y`/`host.screen`/cursor) → dogfooding the API.
  - **Related:** the *overlay calibrator* (above) builds on this — it records the inspected coordinates/regions/colors/images and stores them as module assets.
