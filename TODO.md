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
- [x] **OCR engine — dual-engine pipeline (embedded PaddleOCR fallback):** the weakness Sforzando exposed — `Windows.Media.Ocr` is fast (~5 ms warm) but **blind to isolated single glyphs** (reads "128" but not a lone "1") — is fixed by a second in-process engine. A **PaddleOCR recognition model via ONNX Runtime** (`ort`, recognition-only, English PP-OCR mobile) is **embedded in the binary** (`crates/host/models/`, `include_bytes!` — portable, no external file) as a **fallback**: for a small region it runs **concurrently** with WinRT (`backend::paddle_ocr`) and its read is used only when WinRT returns empty, so a lone digit costs ≈ max(winrt, paddle) ≈ 15 ms (not their sum) while the common multi-char case stays on WinRT. Shared **content-tight preprocessing** (`tighten`: crop to the glyph + upscale) was the real unlock — it fixed both engines. The model **preloads at startup** off the hot path (`backend::warmup_ocr`). Engine chosen by a measured spike (pure-Rust `ocrs` vs `ort`+PaddleOCR on real crops; PaddleOCR won on speed + single-digit accuracy). WinRT stays the primary + the only multi-word/detection path. **The macOS half of this plan was not built** (noted 2026-09-02): PaddleOCR was to be the cross-platform single-glyph specialist, but `ort` sits under `cfg(windows)` and `mod paddle_ocr` is `#[cfg(windows)]`, so Vision carries the small-text case alone there. That is deliberate and documented in `backend/macos/ocr.rs` — and Vision is given everything that helps: full backing-resolution capture, `.accurate`, language correction off, minimum text height zero, and a second untightened pass on the same "only when empty" rule. See [[ocr-engine-architecture]]. ✓ (2026-06-21, interactively confirmed)
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
- [x] **A module excluded by `supported_os` has a row now** (2026-09-01). The Installed list
      was built from what LOADED, so a module skipped for this platform vanished: no row, no
      settings, no way to remove it — in the one window somebody who cannot see the screen
      depends on. It is remembered when it is skipped and listed with the reason in the row's
      own text, because a screen reader reads the row and nothing else.
  - The backlog entry said Settings, Reload **and** Uninstall should all refuse. Uninstall
      should not: it removes files by id, needs nothing loaded, and is exactly what somebody
      who installed it past the warning came to the window for. So Settings and Reload refuse
      by name, the checkbox puts itself back, and Uninstall works — skipping the revoke step,
      since there are no registrations to revoke.
  - The list now reports its own contents to the log — how many rows, and how many of them are
      for modules this platform will not run. The one place a module can be reached from is
      also the one place whose emptiness is invisible to the person it matters to, so the count
      belongs in the log a tester sends rather than only on a screen they cannot read.
  - Confirmed with NVDA (2026-09-01): the row reads, the checkbox is gone, Space does nothing
      and says nothing, Reload is unavailable, Uninstall works.
  - **Four attempts failed on one wrong assumption**, and it is worth keeping: a wx event is
      consumed by calling `event.skip(false)`, NOT by declining to call `skip`. Without it the
      binder passes the event on, the native tree cycles its checkbox, and the accessibility
      layer announces the change before any handler here runs — so every correction afterwards
      is either silent (a lie) or a list rebuild (which re-announces the row). Setting the
      state-image index to 0 removes the picture only; the control keeps cycling underneath.
      Both routes need suppressing: `on_key_down` for Space, and `on_mouse_left_down` with a
      `TVM_HITTEST` for `TVHT_ONITEMSTATEICON`.
  - The answer came from the user pointing at **rabbit**, which had solved the same problem
      against the same control, with the reasoning written beside it. Worth looking there first
      for anything wx-or-native-control shaped: guessing in front of a blind tester costs his
      time, and a search costs mine.
- [ ] **Module manager follow-ups:** CLI/IPC control surface; a **native macOS/GTK checkbox path** (`TVS_CHECKBOXES` is Windows-only — non-Windows currently shows no checkboxes). Out-of-process only for the untrusted-native-FFI tier. See [docs/module-runtime-and-lifecycle.md](docs/module-runtime-and-lifecycle.md).
  - Lifted out of the completed entries below, where they were easy to lose:
  - [x] **Hotkey conflicts are resolved at registration only** — fixed 2026-09-03, and the
      entry understated it. The skipped registration was not merely inactive: `host.hotkey.register`
      returned *before* inserting it, so the callback was dropped and the module got the id `0`.
      There was nothing to revive, which is why it took a restart. Liveness is now DERIVED by
      `refresh_hotkeys` from the enabled set, the way `refresh_captured` has always derived the
      captured-key set — the earliest claim among enabled modules holds the combination, and
      there is no stored decision left to go stale. `apply_enabled` recomputes instead of
      re-registering blind (it discarded the result with `let _ =`), every release path
      refreshes, `hotkey_owner` and `is_enabled` are gone with their last callers, and the
      conflict message no longer tells the user to restart.
    - **The reload is where this wanted to go wrong, twice.** First: `purge_module` must NOT
      refresh, because both its callers are the reload path and promoting a waiting claim
      mid-rebuild hands a module its own key away. Keeping the refresh out closed the window
      — and not the outcome, which was the second mistake and only surfaced when the review
      was done by hand after the workflow failed. Ids are monotonic and never reused, so the
      rebuilt module registers with a HIGHER id than the standing claim and "lowest id wins"
      still gave the key away, permanently, for the session. The rule is `(module_idx, id)`
      now — load order between modules, registration order within one — which survives a
      reload because a module keeps its index. At start-up the two readings agree, so nothing
      changes for the ordinary case.
    - Also from that review: a module that HAD a hotkey and is reloaded WITHOUT one registers
      nothing, so nothing triggered a refresh and the combination its purge released at the
      OS was left held by nobody. `reload_module` refreshes at the end of the operation now.
    - Measured end to end, not reasoned: two modules contesting Ctrl+Alt+H, the loser now gets
      a real id (3, not 0) and its claim stands; when the holder releases the key, the log
      shows it passing over within the same second. With the first module persisted off, the
      second holds the key at load and no conflict is reported. Four unit tests pin the rule
      itself, including the case that regressed.
    - **Five more findings from an adversarial review, all on the conflict MESSAGE**, and one
      of them overturned the rule again:
      - A module that is *holding* a key now keeps it. Ordering by `(module_idx, id)` alone
        let a lower-indexed module take a live key away, and the review showed the case is
        real rather than theoretical: overlays register control hotkeys on activation and
        release them on deactivation, `Alt+B` is a control in more than one shipped overlay,
        and a plug-in switch can have the arriving overlay register while the leaving one is
        still holding. Incumbency is derived from `live`, so no stored state came back.
      - A loser was told "module A already uses it" even when A's own registration had been
        refused by the OS — `want` is computed from claims, not from what was granted. Only
        reported when the winner is actually live now.
      - With three claimants both losers were told to disable the holder and promised the key
        would pass to *them*; it passes to the next in load order. The message no longer
        promises who gets it, only who has it and what makes it move.
      - The dedup key carried no owner, so the corrected message naming the new holder was
        swallowed — a user could carry out an instruction that was never retracted.
      - `docs/module-manager.md` still said "(then restart)", contradicting the API reference.
    - **Not verified by me, and both are things the owner can do in seconds:** the
      manager-checkbox path (`set_enabled` → `apply_enabled` → `refresh_hotkeys`, same
      plumbing, rule unit-tested for that exact transition — driving a tree control's checkbox
      from a script was not worth the machinery), and the reload itself, which needs the
      reload hotkey pressed. Six unit tests cover the rule including the reload ordering; what
      is unmeasured is the plumbing around those two triggers.
  - **Decided, so it is not re-opened: the holder is not remembered across a restart.**
      Raised by the owner from the test itself — re-enabling the module that used to hold a
      combination does not take it back, which is the rule working (whoever holds it keeps
      it; loading earlier is not a reason to interrupt a module that is using something).
      What that leaves is a wrinkle: at start-up nobody holds anything, so load order decides
      again and the first module has it back. Persisting the holder would have made the
      toggle a durable control over who owns a shared key; the owner's answer was that this
      is solving the wrong problem. **The problem is that two modules want one combination at
      all** — the same thing NVDA add-ons have always had — and a user settles it by turning
      off what they do not need, which persists on its own. Load order therefore stays the
      tie-break, and the restart behaviour is documented rather than fixed.
  - [ ] **Application-side hotkey remapping — the feature the conflict actually points at.**
      Not urgent; written down while the reasoning is fresh. The user should be able to move
      a module's key rather than choose between two modules.
      - The shape: the host already records every claim — `hotkeys` holds the spec, the
        owning module and whether it is live, and `refresh_hotkeys` already knows who lost to
        whom. So it can offer "module A wants Ctrl+Win+Alt+F8, which module B is using" and
        let the user bind A's to Ctrl+Win+F8 instead. The remap is stored per **(module id,
        requested spec)**, so a module's other keys are untouched, and the next time A asks
        for Ctrl+Win+Alt+F8 the host registers Ctrl+Win+F8 for it — silently, from then on.
      - Where it is stored: the settings, not an environment variable. Whether the manager
        gets a column, a dialog on the conflict itself, or a settings pane is a UI question
        that should be answered by whoever is looking at that window.
      - **The hard part is not the binding, it is what the module SAYS.** An overlay
        announces its own keys ("press Alt+B"), and a control must never claim something it
        cannot honour — so a remapped module that still announces the spec it asked for is
        lying to somebody who cannot check. `host.hotkey.register` would have to report the
        **effective** spec back, and the overlay runtime would have to announce that rather
        than the authored string. That is the real work, and it reaches into every overlay
        that names a key.
      - Conflict detection then has to run against effective specs, not requested ones, or a
        remap could quietly collide with a third module. And a replacement can itself be
        unavailable — held by another application, or unparseable — which needs the same
        honest refusal the OS-conflict path already gives.

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
- [x] **Packaging bundles the screen-reader clients** (already done; the entry was stale).
      `package.ps1` copies `nvdaControllerClient64.dll`, `SAAPI64.dll` and `DirectML.dll`, and
      **throws** naming any that is missing, with what each one costs — because every one of
      them fails silently and differently: without the speech clients the overlay runs and says
      nothing, without DirectML small text stops being read, and neither looks like a missing
      file to whoever is testing.
- [x] **Logging off stdout:** diagnostics now go to a file `<exe_dir>/automation-platform.log` (portable — next to the binary) via `host::logging`, not stdout/stderr (a screen reader reads the focused terminal). ✓ (2026-06-21)

## macOS (2026-08-13, written blind)

The backend, the packaging and the documentation exist; nothing has ever run on a Mac.
See [docs/macos-port.md](docs/macos-port.md) for the decisions and
[docs/building-on-macos.md](docs/building-on-macos.md) for how to build it.

- [x] **The CI build is green** (2026-09-01). It had been failing on every push since 31
      August — not on the code: the job never started, because the account's Actions were
      blocked on billing. With that cleared it links on a real Mac and its smoke run loads
      every module headless, which turned it into the first real verification the macOS half
      has had: macOS 15.7.9 on a Mac mini, Accessibility and Screen Recording granted, every
      module loading with capability enforcement on, under the new `element` and `arbiter`
      names, with no refusals and the arbiter registering its slots. Compiling was all this
      could say before.
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
- [x] **The blank-region guard is on both platforms now** (2026-08-31). macOS had noticed the
      identical fact — one flat colour, nothing to crop to — logged it at *trace*, asked TCC
      whether it was a permission problem, and handed the rectangle to Vision anyway.
  - The worse case escalated rather than stopping, and it is the realistic shape: an **empty
      well**, a dark value field set into lighter chrome, is found by the crop, so `cropped`
      is true, the ladder opens, and it ends at the `FAST` character model reading an empty
      box — a model with no linguistic validation and no way to answer "nothing".
      `ink_inside_panel` used to fold "not a panel" and "a panel with nothing on it" into one
      `None`; it now answers three ways, and the third marks the plan blank.
  - A blank plan short-circuits before anything is rendered, so an empty region costs no
      recognition at all rather than up to four.
  - Blind code, and the probe already measures exactly this: its regions 1 and 2 are proved
      flat and region 3 straddles an edge, so the next log from a Mac says whether Vision was
      inventing on that input in the first place.
- [x] **The seven first-session measurements** — taken (2026-09-03), on a 2015 Intel
      MacBook Air, macOS 12.7.6, VoiceOver running, all three permissions granted, the
      build signed as "OSAP Local Signing" so the grants survive a rebuild. In the order the
      doc lists them:
  1. **Does anything appear** — yes: bundle, menu-bar item, environment block, three clean
      sessions, no module refused, no crash.
  2. **Coordinates on Retina** — HALF. The pointer's position and the window's AX frame
      agree, and a capture comes back at 1.00x — but this machine has a backing scale of
      1.00, so the 2.00x case the question was really about is still unmeasured. Needs a
      Retina Mac or an external display at a scaled resolution.
  3. **Is a capture real** — yes: `probe-1.png` is a faithful 1013x580 screenshot, 16 ms.
  4. **Does the tap suppress** — yes: "the event tap suppressed its first key (vk 0x09)",
      which is Tab, and Input Monitoring reported granted.
  5. **Does OCR read plugin text** — yes: 58 words from REAPER's FX window; the three
      sforzando read-outs read `empty`, `64`, `DEF`.
  6. **`_AXUIElementGetWindow`** — indirectly: the frame lines ("content starts 28pt below
      the frame top") show the window pairing holds. No log line names the private call
      itself, so this is inferred from the geometry working, not read off a measurement.
  7. **The pump budget** — measured, and it is the number this port most needed. An ordinary
      Tab step costs **90–250 ms on the event thread**: the synchronous Vision read of the
      read-out (`announce 248` for the 123x23 pt Instrument field). Windows pays 23–34 ms
      for the same step. The tap SURVIVED that — not one re-enable in minutes of navigation.
      It was switched off three times in the session, each during a stall of over a second
      (five F6 presses at 1.0–1.5 s each, and the probe's 2.4 s), and the watchdog had it
      back within about two seconds every time. So: the main thread suffices, the ceiling is
      roughly a second, and the two things that crossed it are named below.
- [x] **Does Apple Vision read a lone digit?** Yes (2026-09-03). The probe of REAPER's FX
      window returned `'1'`, `'2'` and `'0%'` as tokens of their own, so the second OCR engine
      does NOT have to become cross-platform for that reason. What the same session left open
      is narrower and is the next item.
- [ ] **PB RANGE reads nothing at the value 1 in sforzando standalone; POLY. at 1 reads
      fine.** Reported by the tester, and the module predicted it about itself: its macOS
      region for the pitchbend field `{576, 38, 607, 60}` was flagged in its own comment as
      "the one most likely to want a nudge", because the probe's box for RANGE overlapped
      DEF's by three points. Since Vision reads lone digits, the region is the suspect, not
      the recogniser. One probe of the standalone with PB RANGE set to 1 settles it — a state
      the tester CAN produce, through our own menu — and no log line for it exists yet,
      because the overlay logs a read's timing, never its text.
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
  - [ ] Cut a new docs version on each `engine_api` bump, so a module author reads the reference for *their* target version rather than the newest one. **The premature 0.1.0 snapshot was removed** (2026-08-31): it had been cut when the site was set up, before any release and before `engine_api` had ever moved, so the dropdown offered two stands of the same version and the archived one had quietly stopped being true — 14 overlay entries against the live 25, 6 UIA entries against 12, missing `O:watch`, `O:addStepper`, `O:group` and `O.layer` entirely. A version gets frozen when somebody is actually running it.
  - [x] **Every API entry has a worked example, the way AutoHotkey's reference does** (2026-08-31). Not a
        signature restated in prose but the call in use, in a `luau` block, taken from a real
        module where possible so it moves when the module does. Asked for on 2026-08-31, and it
        is a reading-order argument as much as a teaching one: a screen reader delivers a code
        block in one piece, while three paragraphs of prose leave the reader assembling the call
        from memory. Counted the same day, and the first count was wrong in a way
        worth recording: it grepped for ```` ```luau ```` fences only, while over half the
        reference was labelled ```` ```lua ````, so it reported a gap of 32 where the real one
        — entries with **no code block at all** — is **8**: six in `overlay.md` (`O.layer`,
        `O:frame`, `O.state`, `O:focusNext`, `O:focusPrev`, `O:gate`/`O:landmark`), plus
        "Key spec string format" and `host.uia.rawDump`. Separately, entries whose only block
        was a signature or a return shape rather than a use — an audit found ten more of those
        in `overlay.md` alone, including one whose entire body was `--[[ ... ]]`. All 85 entries
        across the six reference pages now carry one, taken from a real module wherever a real
        module uses the call.
  - [x] **Named per-OS sections wherever behaviour actually differs** (2026-08-31), the way the
        BASS reference does it. Twenty-five candidate differences were collected from the two
        backends and then adversarially checked; nineteen stood as written, four needed
        rewriting and **two were wrong** — one claimed a Windows bound that only applies to half
        the functions it named, and one described a failure the cited code already mitigates. A
        platform note claiming a difference that does not exist is worse than a missing one,
        because it teaches the reader to distrust the real ones. What shipped: 22 Windows
        sections and 26 macOS ones. Three sentences of general prose contradicted them and were
        reconciled — "all coordinates are screen pixels", "`id` fields are native window
        handles", and "on a backend OCR failure the call raises a Lua error" were each true of
        Windows only.
  - [x] **Every code block is now labelled `luau`, and highlighted** (2026-08-31). Prism ships
        no `luau` grammar, and an unregistered language is not an error there but SILENCE — 52
        blocks rendered as plain grey text while the mislabelled `lua` ones were coloured, and
        the two are indistinguishable in the source, so nothing ever reported it. A swizzled
        `prism-include-languages` aliases `luau` onto the Lua grammar; all 111 blocks in
        `docs/` now carry the language the platform actually runs. Verified against a file
        that was not edited, so the highlighting is the alias's doing and not the relabelling.

## Shared components, pulled into the runtime (2026-08-31)

Asked for after ON:EAR: go back over the modules and centralise what several of them had each
built for themselves.

- [x] **The stepper announcement was written twice.** There are two ways to move a stepper —
      pressing it and stepping it with Left/Right — and each had its own copy of the same
      fifteen lines: read the printed value, watch until it changes, give up at the control's
      settle time, announce whichever happened. Identical, and therefore one edit away from
      quietly disagreeing. Now `announceWhenChanged`, once.
- [x] **`O.doubleClick(x, y)` and `Overlay:afterIdle(key, ms, fn)`.** Both were ON:EAR's, and
      both are the same fact from opposite sides: two clicks close together in time and space
      are a double-click. A plug-in that resets a control to its default on one is offering a
      real gesture worth reaching — one keystroke against twenty steps — and everywhere else it
      is a hazard, which is how the Width slider came to reset itself mid-adjustment when the
      arrow keys were pressed quickly. `afterIdle` coalesces a burst of presses into one action
      and, unlike the hand-rolled version it replaces, drops a pending action when the overlay
      is no longer active or its window is no longer in front. ON:EAR now uses both and keeps
      neither implementation. Verified that both cross into another module's VM as functions.
- [x] **sforzando's `inDaw:gate` does not need `pollMatch`** — checked rather than assumed.
      The concern was a gate whose condition can change with no window event, but this one asks
      about the focus chain, and `_recheck` is driven by focus events as well as activation
      (`modules/overlay-runtime/src/main.luau:3022-3023`). Closed, not fixed.
- [ ] **The guessed settles in Kontakt, Komplete Kontrol and Melodyne** still wait a fixed
      number of milliseconds where `Overlay:watch` would wait for the thing itself — Kontakt has
      four (250/250/900), Komplete Kontrol a 250/600/1000 chain. Not migratable from here: each
      one needs the plug-in running to see what it actually waits for, and a wait shortened
      against a guess is a value announced from before the action.
- [ ] **A coordinate-scaling helper is still deferred.** ON:EAR is the only module that scales
      its authored coordinates to the window's drawn size, so there is one instance and nothing
      to generalise from. It becomes worth doing when a second plug-in needs it.

## The reference reads like one now (2026-08-31)

Asked for after reading it: it did not feel like an API reference, and there was no index of
what the platform offers. Both true, and underneath them a defect nobody could have seen.

- [x] **The anchors were unusable, and displayed correctly the whole time.** Docusaurus derives
      a heading's anchor from its text, and on `O:addStaticText(label)` it produced `olabel` —
      the method name simply gone. Three entries collapsed onto `oopts`, `oopts-1`, `oopts-2`,
      numbered in document order, so adding an entry renumbered the ones after it. Every link
      to an overlay function was therefore either meaningless or on a timer, which is why two
      of mine broke earlier in the same session and I patched the symptom both times without
      asking why. The headings rendered perfectly, so nothing looked wrong. Every entry now
      carries an explicit `{#anchor}` derived from its own name — `#o-addstatictext`,
      `#host-window-ownspoint` — and Docusaurus honours those verbatim.
- [x] **The contents list on each page was half unusable, for the same reason.** Reported from
      the page rather than the source: the link list at the foot of the overlay page read
      `O(label)`, `O(opts)` three times over, and `O() / O()`. Same cause as the anchors — a
      colon glued to a word is directive syntax, so `:addStaticText` is consumed before the
      heading's value is taken, and that value feeds both the contents list and the anchor. The
      heading itself renders from something else, which is why the page looked right and its
      own navigation did not. Escaping the colon leaves the rendering identical and stops the
      parse; the twenty affected headings are all in `overlay.md`, and the generator maintains
      the escape.
- [x] **And the contents lists carried forty-seven "Windows"/"macOS" rows.** The per-OS blocks
      are level-3 headings, so `ocr-input-sound` listed the pair six times with nothing to say
      which call each belonged to. `toc_max_heading_level: 2` on every reference page, so the
      contents list is the list of entries and nothing else.
- [x] **An index of all 87 entries**, grouped by namespace, each with the one-line summary its
      page already carried. The sidebar had listed six pages named things like
      `speech-hotkey-keys-timer-log`, so finding `host.timer.after` meant guessing which bundle
      it was in.
- [x] **Both are generated** (`tools/api-index.py`) and checked in CI. A hand-kept list of 87
      entries is wrong within a month, and an index missing the function somebody is looking
      for reads as "this platform does not have it". Verified the check fails: adding an
      unstamped heading exits 1 and names both files.
  - Docusaurus has no ready-made reference theme for an API like this — the plugins that exist
      read OpenAPI, which describes HTTP endpoints. What makes a reference read as one is the
      index, the stable anchors and the per-entry shape, not a stylesheet.
- [x] **The pages are one per namespace now** (2026-08-31). `window.md` no longer holds
      `host.os` and the matcher grammar; `host.path` and `host.resource` are two pages because
      they are two namespaces. Nineteen pages, sorted alphabetically by the title the
      navigation shows.

## Agreed next, in this order (2026-08-31)

- [x] **1. Enforce the capability list, scoped per module** (2026-08-31). Each module's `host` table carries
      only the namespaces it declares. The mechanism already exists and is not yet used for
      this: `build_dep_host` gives a code dependency its own table with the owner's underneath
      by metatable, and `eval_on_host` wraps each chunk as `function(host) … end` so it captures
      that table rather than a global. So permission follows the module that WROTE the code,
      while ownership stays with the VM that runs it — the overlay runtime declares `ocr` and
      may use it; Kontakt, which depends on it, does not get `ocr` from that.
  - Two details decide whether it works: a permitted namespace must be bound to the OWNER's
      table, so a hotkey the dependency registers still belongs to the owner and dies with it;
      and the metatable's `__index` must refuse a namespace the dependency did not ask for,
      because falling through to the owner would hand back exactly what was just denied.
  - A denial should raise naming the module and the capability, not evaluate to `nil` — the
      symptom of `nil` is "attempt to index a nil value" three frames away.
  - **Built as a VIEW, not a trimmed table** — the first attempt cut the namespaces out of the
      module's own host, and that table is also what a dependency's fall-through binds to, so a
      dependency lost a binding it was entitled to whenever its DEPENDENT had not declared the
      same thing. sforzando showed it at once: the runtime declares `timer` and may use it,
      sforzando does not, and `host.timer.every` came back nil three frames from anything that
      could say why. The full table now stays whole and is only ever a source of bindings.
  - The prelude runs BEFORE gating, on the whole table, because it extends the host
      (`host.os.pick`, the matchers); gating first would put its additions on the view where a
      dependency cannot see them.
  - **The declarations are now exactly what is used**, and much smaller for it: `audio-imperia`
      went from ten to three, `u-he` from ten to two, `impact-soundworks` from eleven to two,
      while the overlay runtime carries the broad set because its code is what does the work.
      That is the payoff — a library used to declare nine capabilities it never names, because
      the runtime read a pixel on its behalf and there was no way to say so.
  - Static analysis got them close but not right, and the difference is instructive: six
      modules needed `window` back although their own files never write `host.window`. **A
      callback you register is your code** — the gate and the matcher an overlay supplies are
      called during a focus change, and the access is correctly attributed to the module that
      supplied them. The enforcement itself settled the final set, in two rounds.
  - Seven manifests under-declared and broke, which was the point: `melodyne` (keys, log,
      timer), `sforzando` (uia), `kontakt` (path), `overlay-runtime` (path, resource, arbiter),
      `examples/hello` (path), `tools/inspect` (uia). Over-declaration stays legal.
- [x] **2. Renamed `host.uia` to `host.element`** (2026-08-31). Both platforms already use
      the word — `IUIAutomationElement`, `AXUIElement` — so it is the shared vocabulary rather
      than a neutral invention, and it drops a vendor name from an API whose purpose is to
      abstract over both.
  - Renamed: the Luau name (113 uses across 21 files), the capability string in 7 manifests,
      and the ten `Backend` trait methods that carry it. **Not** renamed: the word UIA in prose,
      where it means Microsoft's API and is correct, and `crates/host/src/backend/uia.rs`,
      which is the Windows implementation and is named after what it implements.
- [x] **3. The reference read like a loose collection of leaflets** (2026-08-31). Two things,
      both mine:
  - **The pages are sorted alphabetically now**, by the title the navigation actually shows —
      every `host.*` page in order, then the two that are not namespaces. They had sat in the
      order I happened to think of them, which is no order at all to somebody looking for one.
  - **The titles and introductions were too pleased with themselves.** `host.speech — the only
      way out` and `host.sound — a noise instead of a sentence` are essay titles. Every page now
      opens by saying what the namespace covers — `host.speech — spoken output`, "Recognises
      text in a screen region" — with the measurements and the reasoning following, which is
      the order somebody arriving at a reference needs them in. Eleven openings said what their
      subject MEANT before saying what it did; a reader at `host.ocr` already knows they want
      text off the screen.

## One page per feature (2026-08-31)

The six pages of the reference grouped namespaces for no reason anybody could state —
`speech-hotkey-keys-timer-log` was five features in a filename. Asked for: one page per
feature, each saying what that part of the API is for and what to declare in `module.toml`.

- [x] **Eighteen pages, one per feature**, each with a title that says what it is for
      (`host.screen — what the plug-in looks like`), an introduction grounded in what the
      modules actually do with it, and a **What to declare** box carrying the manifest line.
      The introductions carry the measured costs, because choosing between the tree, an image
      and OCR is the decision every control makes once: a screen touch is a compositor frame
      whatever it reads, a 53-element accessibility walk is ~30 ms, a full-window template
      search measured twelve seconds.
- [x] **The capability list is described honestly, once**, on the index. It is not validated
      and **not enforced** — every module gets the whole `host` table whatever it declares —
      and it cannot be a security claim, being a self-report from exactly the party a reader
      has no reason to trust. What it does do is reach the log and the install dialog, which
      is the only reason to keep it accurate.
- [x] **Twenty-one public names had no entry at all.** Six were found by reading
      (`host.os.pick`, `host.resource.exists`, `host.now`, `host.inputEpoch`,
      `host.calibrating`, `host.arbiter`), and fifteen more by the coverage check written
      afterwards. Every one had been passed over by the same faulty test — "does a module
      other than the runtime use it?" — which is a statement about today's callers rather than
      about what the API is. The overlay runtime is a module like any other and will be
      maintained separately; nothing it can reach is private.
- [x] **The check that would have caught them.** `check-docs.ps1` asked only whether everything
      documented exists, which catches rot and never a gap. `api-index.py` now also asks the
      other way: every `host.*` the host registers must have an entry. It caught `host.moduleId`
      in one of the new examples — an API I had invented while writing the arbiter page.
  - Its allowlist is derived rather than kept by hand, after the hand-kept version reported
      `host.calibrating` — perfectly real — as a name the host does not provide. A checker that
      cries wolf gets switched off.
- [x] **The drafts were adversarially checked before they shipped**, and most needed
      correcting: an example that read a private local of the runtime and would have raised
      when pasted, a measurement claimed for both platforms that was Windows-only, several
      platform sections that described a difference the code does not have. One was marked
      "this is the one that must not ship".
- [x] **`host.uia` was renamed to `host.element`** (2026-08-31) — see "Agreed next".
- [x] **A page with no "What to declare" box is now caught** (2026-09-01). The boxes were
      written by the one-time split rather than generated, so a new page could be added
      without one and nothing would say so. `api-index.py --check` now fails on any reference
      page missing `{#declare}`, which is the same shape as the coverage check above: ask the
      question the writer will forget, not the one they just answered.
- [x] **The last grouped page was split** (2026-09-01) — `modules.md` held `host.require`,
      `host.tryRequire` and `host.include` under one filename, which is the grouping the whole
      section was meant to end. Three pages now, one namespace each, and the navigation is
      alphabetical throughout: `host.include`, `host.require`, `host.tryRequire` sit where a
      reader looking for them would look.
  - The paragraph about VM isolation moved to `host.require`, which is the call it explains:
      what crosses a module boundary depends on the dependency kind, and a `code_module`
      brings its functions with it because its source is evaluated in the dependent's VM.

## Found while documenting (2026-08-31)

Three defects that surfaced only because somebody wrote down what the code claims. None was
reported by a test, because none of them fails loudly.

- [x] **Three of the four modifier taps were dead on Windows** (2026-08-31), and the way this
      was nearly recorded is the more useful half. An audit reported that `MASK_TAP` "is
      implemented only in the macOS tap", and a grep for `MASK_TAP` in `windows.rs` agreed —
      no hits. Both were wrong: the Windows hook implements taps, it just spelled the mask
      `0x10` instead of naming the constant, so neither a reader nor a search could find it.
      The claim went into a platform note and into this file as fact before the code was
      opened, which is the exact failure the platform notes were adversarially checked to
      avoid.
  - The real defect, once the code was read, is narrower and real: only Alt was armed
      (`vk == 0x12 || 0xA4 || 0xA5`), so `"Ctrl tap"`, `"Shift tap"` and `"Cmd tap"` parsed,
      returned a token, and could never fire. All four are armed now, the mask is named rather
      than spelled, and a second modifier pressed on top drops the arm the way macOS already
      did — without that, releasing Alt while Ctrl was still held fired a tap the user never
      made.
- [x] **`recognizeMany` keeps its promise on macOS now** (2026-09-01). It had no override, so
      the trait default applied: one full capture per region, each a separate round trip at a
      separate moment — which is the failure the call was added to prevent, since its whole
      purpose is that two values a module compares come from the same instant.
  - The crop reuses `render`'s clipping rather than `CGImageCreateWithImageInRect`. That API is
      the obvious tool and is deliberately avoided in this file: its rectangle is documented in
      the image's own coordinate space, a convention nobody here can test.
  - Falls back to one capture each wherever the shortcut cannot be trusted — fewer than two
      regions, a degenerate one, a failed capture, or one that came back a different size than
      asked for, which means it was clipped at a screen edge and every offset would point
      somewhere else.
- [x] **The reference contained examples that raise if you copy them** (2026-08-31). Seven
      lines in one file: `host.window.focused()` twice, a binding that has never existed, and
      `host.log("…")` five times, where `host.log` is a table carrying only `info`, so the call
      raises "attempt to call a table value". Nothing had noticed, because prose and code rot in
      the same silence. Fixed, and `check-docs.ps1` now checks every `host.*` name the
      documentation calls against what the host actually registers — it passes on the reference
      and fails with exit 1 on a reintroduced `host.window.focused`, which is how it was
      verified. Worth wiring into CI.

## Further open points (from the feasibility study)

- [ ] Define hard **performance/footprint budgets** (hotkey latency in ms, baseline RAM, default binary size) — study §10.2.
- [ ] Clarify **macOS distribution/MDM details** (Developer ID/notarization flow, possibly PPPC profiles for enterprise) — study §10.4.
- [ ] Work out the **native-FFI security model** (out-of-process sandbox per OS, trust store/signature flow, macOS FFI policy) — study §11.1–11.2.

## ReaHotkey port — prerequisites & tooling

Foundation: [docs/reahotkey-port-analysis.md](docs/reahotkey-port-analysis.md).

- [x] **Pre-Flight 1** — verified 2026-09-03: the AX frame of REAPER's FX window and of
      sforzando standalone both resolve, with the content inset (28 pt below the frame top)
      reported per window class. Original text follows. Verify the window/plugin frame on macOS. [VOCR](docs/prior-art-vocr.md) demonstrates: the AX frame (`AXPosition`/`AXSize`) of the focused app window is reliable even for non-accessible plugin windows and serves as the capture/coordinate origin. What remains to be checked: sub-view frame of an *embedded* plugin vs. floating window (REAPER can show plugins as floating windows → simplest case).
- [x] **Pre-Flight 2** — verified 2026-09-03: all three grants present in the tester's
      session and surviving a rebuild under the local signing identity. Original text
      follows. macOS TCC onboarding (Accessibility + Screen Recording, Input Monitoring only with CGEventTap). **Hotkey strategy per VOCR:** registered hotkeys via Carbon `RegisterEventHotKey` + context scopes → **no** CGEventTap/Input Monitoring/silent-disable needed. CGEventTap + health watchdog (`tapIsEnabled`/`tapEnable`) only if suppression/remapping/hotstrings are required. Permission persistence across self-updates (stable team/bundle ID). Study P0-2 + P0-9.
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

## Speech through prism (2026-09-01)

Replacing the Windows half of `speech/mod.rs` — `tts-rs` over Tolk — with a from-source,
statically linked binding to [prism](https://github.com/ethindp/prism). Windows only; macOS
keeps `voiceover.rs`, which is better than prism's VoiceOver backend. The measurements, the
design and the reasoning are in [docs/prism-speech-design.md](docs/prism-speech-design.md).

- [x] **Step 1 — the crate and the build** (2026-09-01). `crates/prism-sys`, prism as a
      submodule pinned to `f237af6` (v0.18.2, the commit the smoke test ran against), and a
      `build.rs` that compiles it in 36 s. It asserts its own outcome rather than trusting
      its arguments: every `-D` must come back from `CMakeCache.txt` with a real type (an
      unknown option is a *warning* and exit 0 in CMake, so a renamed one would silently
      build a different library — proven by feeding it a name that was real at v0.17.0), the
      built backend set must equal the requested one, and `prism.lib` must claim the dynamic
      C runtime (two CRTs in one process is two heaps, announced only by an `LNK4098`).
  - The backend-set check first passed on a lie: CMake reconfigures in place and leaves the
      targets it no longer builds behind as directories, so it was reading everything ever
      built here. A changed option set now discards the build and install trees first, and
      the fingerprint is recorded only after the build succeeds.
  - Nine backends: `nvda`, `jaws`, `zoom_text`, `sapi`, `onecore`, `zdsr`, `pc_talker`,
      `boy_pc_reader`, `sense_reader`. Out: `uia` (returns success when nobody is listening),
      `system_access` and `window_eyes` — the only two prism marks `LEGACY`, so the legacy
      path goes with them. Losing System Access is a deliberate, stated regression.
- [x] **Step 2 — bindings, link flags, smoke tests** (2026-09-01). bindgen against the
      installed header (with `-DPRISM_STATIC`, or every declaration comes back
      `__declspec(dllimport)`), `static:+whole-archive=prism`, the `/delayload:` flags and
      the three vendor import stubs, and five smoke tests. `build.rs` finds Visual Studio's
      libclang itself when the shell has not, the way `run-dev.ps1` already does — with no
      fallback to a checked-in bindings file, because a stale one cannot catch what it exists
      to catch.
  - **Both traps fired on the way, exactly as predicted.** Without the delay-load flags the
      test binary did not start: `0xC0000135`, before `main`. And `-tests` was the wrong
      qualifier — it covers integration tests and leaves the lib's own unit-test binary to
      die at start-up. Without `+whole-archive` the backend list came back **empty**, which
      is the silent mute this whole guard exists for.
  - **The empty-string guard had a hole the test found and reading did not**: a NUL is not
      whitespace, so `" "` survived `trim`, was then filtered down to an empty string and
      handed to prism anyway. Stripped before the check now, not after.
  - `PRISM_BACKEND_*` are `UINT64_C()` macros that bindgen cannot evaluate, so backends are
      opened by name through `prism_registry_id` — the names the smoke test pins.
      `PrismBackendFeature` is blocked outright: its members are 64-bit, its underlying type
      is 4 bytes, and its last member silently evaluates to 0.
- [x] **Step 2a — the application binary delay-loads what prism imports** (2026-09-01).
      Cargo does not propagate a link argument to a dependent's executable the way it
      propagates a native library, so `crates/app/build.rs` has to say it again — but not
      keep a second copy of the DLL names: prism-sys publishes them as `cargo:delayload=…`,
      which cargo hands to a **direct** dependent as `DEP_PRISM_DELAYLOAD`. That is the only
      reason `app` names prism-sys in its manifest at all. Verified with `cargo build -v`:
      all three `/delayload:` arguments reach the executable's link.
- [x] **Step 3 — `speech/prism.rs`** (2026-09-01): one owning thread, backends opened by
      name in order and never `create_best`, a speech engine opened lazily on the first line
      that needs one (opening OneCore was measured at 2.9 s, which at start-up is 2.9 s of
      frozen application), one-strike demotion, the 300 ms stall deadline enforced from
      `refused()` on the event-loop side, and a `rearm` that replaces the whole worker
      because prism's binding to a dead screen reader is fixed for the instance's lifetime.
      Compiled but not yet spoken through — `Speech` still routes Windows lines to `tts`.
  - A compiler warning about a pointless variable turned out to be pointing at a real bug:
      the worker returned on failure with lines still in its channel, each of which had been
      counted in `pending` when it was handed over. `is_speaking` would then have answered
      yes for the rest of the session and every shutdown would have sat out its full fifteen
      seconds. It hands them all back now, to be said the other way.
- [x] **Step 4 and 5 — wired in, with the switch** (2026-09-01). `Speech` tries prism first,
      `pump()` speaks the refusals through the other path, `is_speaking` counts its own
      outstanding lines. New switch **"Speak through the screen reader"**, Windows only,
      **default on** — it is the way out rather than the way in. The `tolk` feature left
      `tts` in the same commit: with it on, the fallback would have been the very screen
      reader that had just stopped answering.
  - **Eight defects found by an adversarial review before the user ever heard it**, five of
      them ending in silence or in the wrong words. Fixed: the headless loop never called
      `Speech::pump()`, where the whole stall deadline lives, so a wedged screen reader could
      never be detected there — every event loop now has an `on_tick` hook; the 2.9-second
      synthesiser open happened on the first line rather than at start-up, inside the very
      call the deadline was meant to bound; `pending` stayed set after a stall, so every later
      shutdown would have waited its full fifteen seconds; the worker could exit with the
      channel still live, dropping a line into nobody's hands; the deadline covered only
      `speak`, not the opens before it; a wedged worker replayed its whole backlog on
      unblocking, announcing controls from minutes ago; and — the worst of them — an
      interrupting line discarded everything queued **after** it as well as before, so the
      overlay's name-then-value pair lost the name and spoke a value belonging to nothing.
  - Not fixed, and deliberate: a screen reader that is merely SLOW rather than wedged finishes
      after the deadline has already handed the same words to the fallback, and the line is
      heard twice. The call cannot be cancelled, so the choice is between hearing something
      twice and risking not hearing it at all.
- [x] **The startup report no longer guesses at the screen reader** (2026-09-01). It asked
      whether `nvdaControllerClient64` or `SAAPI64` was loaded in this process — a fair proxy
      only while Tolk loaded them on demand. The first run through prism printed "none
      detected (speech falls back to SAPI)" two lines above "speaking through NVDA". Removed
      rather than repaired: the speech layer's own line says which backend will speak, which
      is the question worth answering.
  - `run-dev.ps1` stopped the running copy AFTER building, so a rebuild failed with "failed
      to remove file … Zugriff verweigert" — which looks nothing like "the app is running".
      Moved above the build, the order `package.ps1` already used and documented.
- [x] **The screen reader comes back by itself** (2026-09-02), which is what the first live
      test asked for: quitting NVDA moved speech to WinRT correctly, but getting it back meant
      restarting the application. One searcher now looks every 3 seconds for the first minute
      and every 30 after, silent unless it finds one, stopped while the setting is off and
      stopped when its channel closes.
  - **The first version was wrong in a way the log made obvious**: it keyed the eager tier off
      when the worker had started rather than when the path was lost, so the first attempt
      waited thirty seconds. It did work — `back to NVDA` thirty seconds after the deadline
      fired — and from a keyboard that is indistinguishable from not working. Two tests now
      cover it, one of which simulates a loss and requires the pickup within five seconds; it
      takes 0.08 s.
  - **One searcher rather than one per attempt**, because the cost is not where it looked: a
      sweep of the seven screen-reader backends costs 86 ms, of which 62 is `prism_init`. The
      searcher keeps its library open and sleeps between looks.
  - **Losing NVDA does not return an error — the call blocks.** The 300 ms deadline is
      therefore the normal way a loss is noticed, and the abandoned thread its normal price. A
      log line now records whether such a call ever comes back at all, because the design
      should not have to assume.
  - It rests on a measured property rather than a hope: prism refuses a backend whose reader
      is not running — with NVDA up, the other six answered `BACKEND_NOT_AVAILABLE`. A smoke
      test now asserts the invariant, because a retry that adopted a dead backend would be
      silence, which is the one outcome this design exists to prevent.
  - Measured in passing: NVDA reports `braille=true` and `output=true`, so the braille step
      has something real to switch on.
- [x] **The headless loop is missing more than speech** (2026-09-03) — and it was missing
      more than the two calls. Three defects, stacked so that each hid the next, separated by
      experiment because reading could not tell them apart.
  1. **`Manager::run` would not start a loop at all** for a module whose only reason to be
      alive was a timer: the guard asked for a hotkey, a captured key or a window trigger, and
      otherwise waited for speech and returned. The condition was right when written —
      `host.timer` fired from the GUI tick only — and stopped being right when the tick grew a
      headless counterpart. Armed timers and outstanding image searches now count.
  2. **The Windows loop then sat in a blocking `GetMessageW`.** No window of ours, nobody
      typing at it, so no message ever arrived and the tick never came. It waits with a 15 ms
      timeout now and drains with `PeekMessageW` — the shape the macOS loop already had
      (`CFRunLoop::run_in_mode`, 0.015) and the GUI path gets from `timer.start(15, …)`.
  3. **`on_tick` really was missing the two calls**, which is where this item started.
  - Measured at each step, and the order matters because defect 1 made the first measurement
      worthless: a module arming `host.timer.every(500, …)` logged `activate` and nothing for
      ten seconds — which proved only that the loop was never entered. With the guard fixed
      and the blocking wait still in place, the loop was entered and no timer fired in four
      seconds. With all three fixed, `every(500)` fires seven times in four seconds.
  - **This is why it mattered more than it read.** Headless is the only way module Luau is
      ever run on a Mac here (the macOS job's "Load the modules once" step), and it is how the
      Windows job runs the capability probe. Both jobs were blind to everything an overlay
      actually does. `tools/capability-probe` now reports its verdict from inside a timer
      callback, so a regression is a MISSING line and the job fails on it rather than passing
      quietly.
  - Still not done, and deliberately: the headless tick has none of the GUI path's phase
      instrumentation (which splits an iteration into events/timers/images and logs anything
      over 250 ms). Worth adding when there is a reason to look at it.
- [x] **Step 6 — application speech separated from module speech** (2026-09-02).
      `host.speech` still always speaks, through SAPI if that is what is there — whoever
      installed and enabled a module decided that. The two places the application speaks on
      its own behalf now go through `Shared::announce`, which **shows** rather than speaks.
  - The rule came out simpler than it was designed, because the owner pointed out the thing
      the design had missed: a screen reader reads a notification out anyway. So it is not
      "speak when a screen reader is running, show otherwise" — it is show, and speak only
      where there is nothing to show it in. On Windows that means the application never
      speaks on its own behalf at all; on macOS, where `show_balloon` answers false, it may,
      and then only with VoiceOver actually running.
  - The tray icon is the only thing that can show a notification and it lives in `gui.rs`,
      so the host is handed a way to reach it — the mirror of the `announce` callback that
      already went the other way. Headless installs none, and the rule still holds.
  - The reload's "Reloading modules" line is gone. It existed so that silence would not read
      as "the key did nothing", which only works if it arrives at once — and it was written
      for a channel where it did. Windows shows notifications when it is ready to, so both
      turned up late and together: two interruptions where one would do, and no reassurance
      either way. Confirmed live before removing it.
- [x] **Step 7 — packaging and licences** (2026-09-02). `nvdaControllerClient64.dll` and
      `SAAPI64.dll` are gone from the staged build: prism reaches NVDA over raw RPC with
      stubs compiled in, and is linked statically, so nothing beside the executable is for
      speech any more. Only `DirectML.dll` remains.
  - **The repository had no LICENSE at all** while every crate declared `GPL-3.0-or-later`.
      It has one now, and the ZIP carries a `licences/` folder: our GPL-3, prism's MPL-2.0
      and NOTICE, prism's whole `LICENSES/` tree, and a README naming the pinned URL, commit
      and tag — because MPL-2.0 §3.2 requires telling whoever receives the executable where
      the covered source is. prism's own NOTICE is not a sufficient attribution list: it
      omits highway (Apache-2.0, whose §4(d) has a real propagation requirement) and NVGT
      (Zlib), so the tree ships rather than a summary of it. The SHA is read from the
      submodule at package time rather than written down, so it cannot drift.
  - The tester README claimed "Speech goes through NVDA or System Access if one of them is
      running", which had been false since System Access was compiled out. Verified by
      running `package.ps1 -NoBuild -NoZip` and reading what came out.
  - `architecture-feasibility-study.md` still named prism as the road not taken, in two
      places. Noted rather than rewritten — it is a dated design document — and the note says
      what actually decided it, which was not the braille the study expected.
## Is Vision blind to a lone glyph? (2026-09-02)

Nobody has asked, and the answer decides whether macOS has an OCR gap.

WinRT is blind to isolated single glyphs — it reads "128" and not a lone "1" — which is the
entire reason the PaddleOCR fallback exists on Windows. macOS does not have that fallback,
by a documented decision, and compensates by giving Vision the best possible input. Whether
that is *enough* is unknown, because no Mac has ever been asked.

- [ ] **The canary is Melodyne's note field.** It is readable on Windows only because the
      second engine fires, and `modules/melodyne/src/main.luau` documents at length what
      happened when a change made the primary non-empty and the fallback stopped firing. If
      Vision reads it, the question is closed and the plan's macOS half can be struck for
      good. If it does not, macOS needs the fallback after all — and that means `ort` on the
      Mac, which also means the universal build grows a per-architecture dependency it does
      not have today.
- [ ] **Not answerable from here, and not answerable by the probe either.** OCR needs
      something on screen to read, and this project cannot put a known lone digit there — nor
      ask a blind tester to produce one. It is a question for the test round, on a plug-in
      that has such a field, not for a synthetic check.

## The first Mac session — what it left open (2026-09-03)

Three sessions, one tester, one plugin. Everything that was settled is ticked where it was
asked; this is what was NOT, plus what the session found that nobody had asked.

- [ ] **The startup announcement is silent on every Mac, by default.** The tester: "I don't
      hear the spoken notification that Automation Platform is running in the menu bar".
      The log explains it completely: `announce()` speaks only when `via_screen_reader()` is
      true, which on macOS is `voiceover_speech() && is_running()` — and "Speak through
      VoiceOver" is off by default (`settings on: dock_while_open`, nothing else). The comment
      in `gui.rs` says the host "may still speak this one"; it cannot, with the defaults. The
      fix is the plain voice for this one line when there is no balloon, which is what the
      comment meant. Not done in this round — the owner chose F6 and the busy-skip first.
- [ ] **F6 into the plugin: rewritten, and unrun.** Five of five presses failed in the
      session, with the backend reporting every request accepted and the focus chain still
      three deep — REAPER keeps its keyboard on the FX list, and the plugin's view exposes no
      element to hand it to. `daw-hosts` now clicks three points inside the plugin's panel
      when the chain says the keyboard is on REAPER's own chrome, reads the chain again a
      quarter of a second later on a timer (a posted click has not been processed when the
      call returns; the first draft read it in the same breath and would have raced it), and
      announces only what that reading supports. Two readings of the chain an adversarial
      review caught as wrong: an EMPTY chain is a failed read, not "inside", and a chain that
      does not end in our window belongs to another application. No click is sent on either,
      because a click on that basis lands in whatever is actually in front. The click point
      is derived from the probe (content 240,52 plus the measured FX-list width, memoised per
      window). A third finding, from the review's critic rather than its reviewers: the click
      was posted while the hotkey's four modifiers were still physically held, and macOS
      mouse events inherited them — a Control-modified left click is a secondary click, so
      the shortcut would have opened a context menu. Mouse events now have their flags
      cleared in the backend, as key events have since Melodyne. And the backend's focus
      chain now answers EMPTY when the focus could not be read, instead of substituting the
      window and looking exactly like "inside". Whether REAPER moves its focus on the click,
      only the next session can say.
  - **Known limitation, and it needs a second plug-in to settle.** "The keyboard is on the
      host's chrome" is read from the chain being deeper than the window itself, which is
      only right for a plug-in whose view exposes nothing — sforzando, as measured. A plug-in
      that exposes its own controls would put the focus several elements deep INSIDE itself,
      be read as chrome, get clicked at its corner and be reported as a failure that is not
      one. The REAPER matcher is title-based, so every plug-in takes this path. Distinguishing
      the two needs an accessible plug-in on the tester's machine to look at first.
- [ ] **Each F6 press cost a second and switched the tap off twice.** `host.window.find`
      lists every window, `enumerate_windows` asked every application, one of them was not
      answering, and the loop never consulted the busy quarantine that
      `frontmost_window_element` has used since it was introduced. It does now — and the
      review was right that this is narrower than it reads: the quarantine lasts five
      seconds, his presses were 16–51 s apart, so it would have spared none of them. The fix
      that would have: `find` carries an `app` clause (`exe`, `bundleId`), and
      `runningApplications()` answers name and bundle without one accessibility call, so
      `list` could ask only the applications a matcher names. That needs the filter to reach
      the backend (`host.window.list(filter?)`), which is API surface, and is not done blind.
      When an application IS in the quarantine, `find` is blind to its windows for five
      seconds; the log now names the application it did not ask, and the shortcut says "could
      not find a plugin window", which is what it knows.
- [ ] **`window.active()` is the biggest thing blocking the pump, and it is not what was
      fixed.** Found on a second, systematic reading of the same log. It stalled the pump
      **fourteen times**, worst case **2605 ms**; `enumerate_windows`, which the F6 work
      guarded, stalled it six times. And the worst one had nothing to do with F6: it came
      while the tester was simply using the plug-in, a moment after a menu closed
      (`pid 1218 answered the frontmost-window question after 2455 ms`). This is the question
      every overlay asks on every tick.
  - The mechanism is written in `frontmost_window_element`'s own comment: "an application
      that is not answering charges the messaging timeout three times over". It reads
      `AXFocusedWindow`, then `AXMainWindow`, then `AXWindows` — and the `is_busy` gate is
      only at the TOP. When the first read times out it quarantines the application, and the
      two fall-backs then ask the application we just decided not to ask. The log shows
      exactly that shape: `timed out reading AXFocusedWindow after 1s`, then an answer at
      2455 ms.
  - **Not fixed blind, because the obvious fix has a user-visible cost.** Re-checking
      `is_busy` between the fall-backs caps the worst case at one timeout instead of three —
      but it returns `None`, and `None` means "no active window", which is what an overlay
      gates on: it would deactivate for up to the five-second quarantine rather than stall
      for two seconds. Which of those a blind user would rather have is not answerable from
      here. Three candidates, in the order I would try them: re-check between the fall-backs
      AND serve the last known window while the application is quarantined; or cap the total
      by lowering the messaging timeout for this one question; or get the question off the
      thread that carries the event tap, which is the structural answer and the large one.
- [ ] **Timers run about 5% slow on that machine, and a settle is a deadline.** Measured by
      the probe: `10 x 100 ms took 1050 ms (shortest 105, longest 105)`, so a 900 ms watch
      deadline is really about 945 ms there. Nothing is broken; it is a number modules with
      settles and deadlines are written against, and every one of those was tuned on Windows.
      Worth a look before blaming a module for missing a window that was never open long
      enough.
- [ ] **Vision costs the same whether the region is tiny or the whole window.** From the
      same probe: a 1013x580 pt read took 730 ms and finished, while a **64x16 pt** read was
      abandoned at its 457 ms deadline. Region size is not the lever — the per-request cost
      is — which is why an ordinary Tab step costs 90-250 ms there against 23-34 ms on
      Windows. It also means an overlay that reads three small fields pays three times, and
      the way out is fewer requests rather than smaller ones: one read of a region covering
      several fields, split afterwards by position. Before building that, measure whether one
      request over a region holding three read-outs really costs less than three requests.

- [ ] **Retina is still unmeasured.** The tester's Air has a backing scale of 1.00. The
      coordinate agreement shown by the probe is real and answers nothing about 2.00x.
- [ ] **The arm64 half of the universal build has never run.** The session ran the x86_64
      slice (no Rosetta). The CI's `lipo` check proves both slices exist, not that the arm64
      one launches.
- [ ] **Qt object names in `AXIdentifier`** — still open, and it needs NO code. The class
      string the backend publishes is already `AXRole/AXSubrole/AXIdentifier` (see
      `join_class`, whose own doc gives `"AXGroup//NI.Kontakt.Main"` as its example), and the
      probe prints it verbatim for every element. One probe run over a Kontakt or Komplete
      Kontrol window answers it. The tester probed sforzando, which is not a Qt application;
      he thinks he can install one.
- [ ] **`host.element.rawDump` did not return on Windows, twice, over a WinUI window.** Found
      while self-testing the probe's new answers: the run reached "note: tree rectangles
      below are SCREEN coordinates" — printed immediately before the dump — and produced
      nothing further in 40 s, twice, over a WhatsApp window (`WinUIDesktopWin32WindowClass`,
      four surfaces including a `Chrome_WidgetWin_0`). The same probe completes on macOS. It
      is wrapped in `pcall`, so an error would have been logged; nothing was, which points at
      a hang rather than a failure. Not chased — it is the Windows side and it did not block
      what was being tested. Worth its own look: a probe that stops before its own answers is
      an instrument that lies by omission.
- [ ] **The VoiceOver transport** — still unmeasured, because the switch was off. Ask the
      tester to turn "Speak through VoiceOver" on for the next round; the `voiceover.sdef`
      he sent confirms the `output` command exists.
  - **The permission that would have swallowed that measurement is now asked for**
      (2026-09-04). Every line of that transport is an Apple Event, which needs the
      **Automation** grant — a fourth permission, in its own pane, refused by default, and
      refused SILENTLY: TCC declines the event, so VoiceOver says nothing and the setting
      looks broken. We knew about it only as a sentence in a failure message, after the fact.
      `AEDeterminePermissionToAutomateTarget` both asks and prompts depending on one flag, so
      the check and the request are the same call; it is made when the switch goes on, and
      the session header reports the status without prompting. Prompted by VOCR 3.0, whose
      permission wizard does the same thing the harder way (a fake Apple Event to raise the
      dialog). Written blind, type-checked, never run.
- [x] **The pump line spoke of Windows on a Mac.** "past ~300 ms Windows stops waiting for
      our keyboard hook" appeared 34 times in a macOS log. It names the platform's own
      hazard now (the tap being switched off). Noted because it cost reading time before it
      was recognised as wording.
- [x] **`docs/macos-port.md` claimed the tap lives on its own thread.** It does not
      (`tap.rs`: main thread), and the session showed the main thread suffices. Corrected.
- Observed and left alone, because they behaved: REAPER and sforzando each refused the focus
  observer when first seen (busy at startup) and were subscribed later on the next
  activation; seven `Return` presses reached the plugin while its menu was open, which is the
  menu pass-through working; `ownsPoint` answered `nil` throughout, by design.

## An error dialog nobody can get back to (2026-09-03)

Reported from a real session: a module-error dialog appeared with, from the user's side, no
title and no taskbar entry. Two separate defects behind one window, and the second is the
serious one.

- [x] **The dialog has no taskbar button, on either platform.** Fixed on Windows, verified by
      measurement: the error report is now a top-level `Frame` (`report_error` in
      `crates/host/src/gui.rs`), unowned and not a tool window, which is what earns a taskbar
      button. Escape closes it, bound on the text control because key events do not propagate
      to a frame. A second report appends to the same window rather than opening another, and
      a report after it was closed opens a fresh one. **macOS is written but unverified** —
      it promotes the agent to a regular application while the window is open, the same way
      the manager window does, and demotes again on close unless the manager is still open.
      The original diagnosis follows, for the record: `modal_message` built a
      `wxDialog` (`crates/host/src/gui.rs`), and a dialog never gets a taskbar button — only
      a frame does. Its parent is the manager window, which in a tray application is normally
      hidden, so there is no button for the parent either. Alt+Tab away from it and there is
      nothing to come back to: the modal is still there, holding the application, unreachable.
  - **macOS is worse, not better.** The application ships as an agent with no Dock icon at
      all — `gui.rs` says so where it builds the menu-bar item — so there is not even a Dock
      tile to click. The existing `dock_while_open` setting is the machinery that already
      solves this for the manager window; a dialog needs the same treatment.
  - Three ways out, and the choice is a product decision rather than a technical one:
      show the manager window (which has a taskbar button, and on macOS a Dock icon while
      open) whenever a dialog is queued and let the modal ride on it; make the message a
      **notification** instead, the way `Shared::announce` now does for the application's own
      announcements, keeping a dialog only for things that need an answer; or build it as a
      frame rather than a dialog, which gets a button but stops being modal.
- [ ] **The title may be fine and read as missing — and the window it was asked about no
      longer exists.** The question was whether a screen reader gets the accessible NAME of
      the report, since `report_callback_error` does pass "Module error: <id>". That report is
      a frame now, with `set_name` on its text control, so the question needs re-asking
      against the new window rather than answered from the old one. Two things to check with
      NVDA in one sitting: does the FIRST report read its own title, and does a SECOND one —
      which retitles the window "Automation Platform — N problems" and puts each report's own
      heading at the top of its own entry — read the new report rather than the old text.
- [ ] **The error window on macOS is written and unrun.** It compiles and links (the macOS CI
      builds `gui.rs`), and nothing beyond that is known. Three things only a Mac can answer:
      does the agent promotion actually produce a Dock icon and an app-switcher entry for it;
      does VoiceOver read the text control when it opens; and does the refcount hold — open
      the manager, make a module fault, close the manager, and check the error window still
      has its Dock icon. That last one is the bug this window was written to prevent, so it is
      the one worth doing first.
- [ ] **A failing code dependency reports once per dependent.** `report_load_failure` pushes
      straight onto the error queue rather than through `queue_dialog`, so it is not deduped:
      if `com.platform.overlay` ever failed to load, each of the eight modules that depend on
      it would queue its own report. They land in one window in one tick, and each carries its
      own heading, so it is legible rather than eight windows — but eight entries saying the
      same thing is still the wrong shape. Found by the completeness critic of the 2026-09-03
      review; left standing because the fix (dedupe on the CAUSE rather than the dependent)
      needs a think about what a user most needs to be told.

## A module was blamed for a capability it never used (2026-09-03)

The error dialog said `com.platform.impact-soundworks` used `host.log` without declaring it.
It does not: the string does not appear anywhere in its 65 lines.

- [x] **Found and fixed — it was the platform accusing a module of its own doing.** The owner
      supplied the one fact that turned two hours of reading into five minutes: `window_prelude`
      was the top frame. That is not a module at all; it is the Luau prelude the host injects
      into every VM, and `W._dispatchFocus` logs there when a dispatch runs slow.
  - The mechanism: the prelude is loaded against the WHOLE host table, deliberately, because
      it extends it. But its functions looked `host` up as a GLOBAL, and they run later — by
      which time the global has become the module's gated view. So the platform's own
      diagnostic was charged to whichever module's VM it happened to fire in.
  - It is worse than a wrong name in a dialog. The refusal is a raised error, thrown into the
      middle of the focus dispatch it was measuring — so the dispatch it was diagnosing did
      not finish, and a module that had done nothing wrong got a dialog about it.
  - The fix is that the prelude now holds the whole table as an **upvalue**: it is wrapped in
      `function(host)` and called with the ungated table. That covers the two `host.log` calls
      and anything anybody adds there later, which is the point — the next diagnostic written
      into the prelude would have walked into the same trap.
  - Proven rather than reasoned: with `SLOW_DISPATCH_MS` temporarily set to 0, so every focus
      dispatch logs, 44 dispatch lines and 0 refusals — including from the modules that do not
      declare `log`.
  - Worth remembering as a rule: **host code that runs inside a module VM must not be subject
      to that module's declarations.** The prelude was the only such code; if more is ever
      added, it needs the same treatment.

## macOS speech — the plan, and why prism is not part of it (2026-09-02)

Asked whether prism should replace `tts` on macOS as well, so that the dependency goes
entirely. The answer is no, but the cleanup behind the question is worth doing — with our own
wrapper rather than with prism.

- [x] **The measurement that settles it was already sitting in CI.** The start-up timing added
      for the Windows work ran on a real Mac: run 33620074214, HEAD `8e44699`, macos-15-intel —
      `[speech] the speech engines took 28 ms to open`. Against the **2872 ms** that justified
      removing `tts` on Windows, that is a hundredfold difference, and the reason is structural
      rather than luck: the AVFoundation constructor registers a delegate class and allocates a
      synthesiser, where the WinRT one built a whole `MediaPlayer`. **The argument that carried
      Windows does not exist here.**
- [x] **And prism would be worse, not better.** `vendor/source/backends/avspeech.cpp:165-176`
      requests Personal Voice authorization inside `initialize()` and waits on a semaphore for
      up to **120 seconds**; prism's own `doc/src/api/backend-notes.md:13` says so. That is a
      permission dialog at start-up, in front of somebody who cannot see it to dismiss it —
      exactly the failure the VoiceOver setting was shaped to avoid. The Windows escape (open
      it on a background thread) does not port either: every avspeech call goes through
      `dispatch_sync` to the main queue, which during start-up is not running yet.
- [ ] **The cleanup is still worth doing, for a different reason.** `tts` is the sole reason
      the abandoned `objc 0.2` / `cocoa-foundation` generation is in a build that otherwise
      uses `objc2` throughout — two Objective-C runtimes in one process, 11 crates that
      nothing else needs. It also does `version_parts[1].parse().unwrap()` on the macOS version
      string inside our start-up path (`tts-0.26.3/src/lib.rs:347`), so a change in Apple's
      format is an application that does not start, on the platform nobody here can run. And
      both of its macOS backends register the same delegate class name, so a second `Tts` in
      one process fails — macOS structurally cannot re-arm its plain voice the way the Windows
      fallback thread does.
- [x] **Built** (2026-09-04), written blind and never run. `speech/avspeech.rs` over
      `objc2-avf-audio` 0.3.2, which was verified to expose all six symbols the plan named —
      `requestPersonalVoiceAuthorization` and `personalVoiceAuthorizationStatus` included —
      before a line was written. `tts` is gone from the tree entirely. The synthesiser is
      built on the worker's FIRST line rather than at start-up, so a session that says
      nothing pays nothing; Personal Voice is asked for only when somebody chooses one, which
      is the whole difference from prism. `engines()` and `use` answer for real on macOS now:
      `voiceover` plus every installed voice by its AVFoundation identifier, and choosing a
      Personal Voice is what asks for it.
  - **The check crate now borrows the WHOLE speech module**, not its two macOS leaves. It
      could not before — `speech/mod.rs` pulled in `tts`, which does not build for that
      target from here — so the wiring was the one part never checked at home. Borrowing it
      caught a `self.tts` that the rewrite had missed on the first pass, in the path that
      speaks lines VoiceOver turned down. That would have reached CI, not a person, but it is
      exactly the class of thing that used to reach the tester.
  - Unrun, and these are what the next session settles: does the system voice speak at all,
      does a chosen voice change what is heard, and does the Personal Voice consent dialog
      appear when — and only when — one is chosen.
- [x] **The original plan, kept for the record:** our own `speech/avspeech.rs`, in the shape and size of `voiceover.rs`, over
      `objc2-avf-audio` (0.3.2, the same generation as the `objc2` crates already here — and
      `AVSpeechSynthesizer` lives in AVFAudio). The decisive advantage over prism is not the
      dependency count: it is that `crates/macos-check` can compile it **from Windows**, which
      is the only compiler this project can run at home. A CMake C++ build on the Mac could
      not be checked here at all.
  - **Personal Voice belongs in it, and is the reason to write it rather than to keep `tts`.**
      It is not out of reach for a hand-written wrapper: `requestPersonalVoiceAuthorization`
      grants it, and the voices then appear in `AVSpeechSynthesisVoice.speechVoices()`. What
      prism gets wrong is *when* — in `initialize()`, at start-up. Ours would ask only when
      somebody chooses Personal Voice, and never otherwise. Verify first that
      `objc2-avf-audio` exposes the authorization call and the voice traits; that check costs
      one `cargo check --target aarch64-apple-darwin`.
- [x] **`host.speech` grew a way to see and choose what speaks** (2026-09-02).
      `engines()` lists what could speak and what can right now — id, name, whether it is the
      user's own reader, whether it is available. `use(id)` chooses one for the calling module
      VM and refuses honestly when the engine is not there; `engine()` reports the choice.
      `use(nil)` returns to the ordinary path. macOS answers an empty list and `false` for
      now, and the shape was chosen so VoiceOver, the system voice and Personal Voice join it
      without changing anything.
  - **An adversarial review argued against selection and was overruled, correctly.** Its case
      was that two engines are two queues and `interrupt` cannot cross between them. True —
      but `interrupt` never crossed engine boundaries in the first place, and a screen-reader
      user already lives with another application talking over their reader. The review had
      mistaken a property of our process for a property of the world.
  - **Three findings on the way, all one cause**: a prism library is safe to open once and
      keep, and unsafe to open and close repeatedly. It crashed (OneCore probed from a second
      library), it prevented a process from exiting (a library opened on the event-loop
      thread), and it hung (a fresh library per query). The question is now answered by a
      worker that already holds one, and a chosen engine is another long-lived worker rather
      than something opened per call.
- [ ] **`host.speech` — what is left of the idea.** The macOS half: VoiceOver, the system
      voice and Personal Voice as three entries in `engines()`, and `use` answering for real
      there. It waits on the objc2 wrapper, which waits on the Mac tester. Also unbuilt, and
      deliberately: a way to send DIFFERENT text to a braille display than to the ear, which
      would be an option on `output` rather than a namespace of its own — worth building only
      when a module needs it, and a 40-character braille line suggests one eventually will.
- [x] **Order of operations — answered by the first session** (2026-09-03): the plain voice
      is fine. The tester heard the overlay throughout (and noted that with VoiceOver on
      system TTS it does not even interrupt us). The wrapper is therefore tidying plus
      Personal Voice, not a fix, and it waits on one more measurement: the VoiceOver
      transport, which this round could not take because the switch was off.
  - prism on macOS only becomes reasonable if upstream drops the blocking Personal Voice wait
      from `initialize()` and the unconditional `dispatch_sync` to main in `speak()`. Both are
      changes to their source, not configuration, so it is not a decision this project can
      take on its own.
  - Still unmeasured, and cheap to get: the first `speak()` on macOS — the equivalent of the
      567 ms measured on Windows. `crates/host/src/speech/**` already triggers the macOS
      workflow, so the number would arrive on the next push.

- [x] **Windows CI — built** (2026-09-03). The open question answered itself: a stock
      `windows-latest` is currently the `windows-2025-vs2026` image, so it compiles C++23
      with `<expected>` and `<flat_set>` without a custom label. `.github/workflows/windows-build.yml`
      builds, tests, builds the docs, packages the artifact — and, since 2026-09-03, runs the
      capability probe headless. As predicted, `Swatinem/rust-cache` does not cache a
      workspace crate's `OUT_DIR`, so prism gets its own cache key on the pinned SHA.
- [x] **Step 8 — `tts` is gone from the Windows build** (2026-09-02), and the reasoning
      that nearly stopped it is the useful part. It was rejected on half a measurement:
      opening a prism speech engine costs seconds (OneCore 3.5 s, SAPI 2.0 s) and the
      fallback is needed exactly when a screen reader has just gone — against which `tts`
      looked instant and already there.
  - The owner asked the question that was missing: *does `tts` take just as long?* It does,
      and worse — **2872 ms of BLOCKING start-up** in the running application, measured, for
      a voice that with a screen reader present never says anything, plus 567 ms for its
      first line. Not cheaper than prism: more expensive, and charged to everybody at every
      start rather than to one person once.
  - The fallback is now a second prism worker with its own library on its own thread
      (`speech/fallback.rs`), opening in the background and queueing what arrives meanwhile.
      `Speech::new` measures **0 ms**. Two threads and two libraries on purpose: the
      screen-reader path is usually lost by the stall deadline, so its thread is wedged, and
      anything sharing it would be wedged too. The screen-reader worker no longer opens
      speech engines at all.
  - Removing the crate broke the build on `OcrResult::Lines`, which needs the
      `Foundation_Collections` feature of the `windows` crate — used here without being
      declared, because `tts` enabled it on the same crate and cargo unified the features. A
      borrowed feature is a dependency you do not know you have until the lender leaves.
- [x] **Step 9 — braille** (2026-09-02). What is spoken now goes to a braille display as
      well, through `prism_backend_output`, which speaks and writes in one call. Switch
      **"Also send what is said to a braille display"**, Windows only, default on — the
      alternative being that a braille reader gets nothing from the overlays at all, which is
      what this application did until now while its own documentation claimed otherwise.
  - **No `host.braille` namespace**, asked for and argued against. There is no separate
      braille channel on macOS to have one for: VoiceOver brailles whatever it is told to
      say. An API that did something on one platform and nothing on the other would be making
      a promise it cannot keep. A backend without braille answers `output` by speaking, so it
      is a strict superset rather than a fork in the road. If a module ever needs DIFFERENT
      text on the display than in the ear, that is an option on the existing call, not a
      namespace of its own.
  - **prism issue #68 was checked rather than trusted.** `prism_backend_output` used to
      return `INVALID_UTF8` for well-formed UTF-8 depending on the byte-length parity of a
      line containing a multi-byte character, and it shipped across four releases. Closed
      against v0.16.7, and this is v0.18.2 — the reporter's sweep now runs as a smoke test,
      because German umlauts are two-byte characters and half the announcements would have
      failed. 24 strings, all accepted.
  - **Verified on a real display** (2026-09-02, by the owner). Not only that the call is
      made and accepted, which is all the smoke test can show, but that the announcements
      actually arrive on the braille line.

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
