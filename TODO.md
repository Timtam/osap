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
      - **Half of it exists now (2026-09-22).** `host.keys.normalize` is the effective-spec
        vocabulary: the host records, logs and reports every registration by it, and the
        overlay runtime keys its claims by it and announces keys through
        `host.keys.describe`. What is left is the remap itself, and handing the remapped spec
        back to the module (from `register`) so the runtime announces that one instead.

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
- [x] **The macOS key vocabulary: keep the Windows one** (2026-08-20). **Superseded on
      2026-09-22** by the maintainer's decision that a spec's modifiers are roles, the Qt way:
      on a Mac `Ctrl` is Command, `Alt` Option, `Win` Control (see "Modifiers are roles, the Qt
      way" below). The reasons: Qt does exactly this (`ControlModifier` is Command on macOS,
      `MetaModifier` Control), so it is the convention cross-platform developers already know;
      `Ctrl+Alt` no longer lands on VoiceOver's Control+Option layer, where the positional rule
      put every Ctrl+Alt key; an application shortcut a module sends works on both platforms
      (`host.input.send("Ctrl+C")` copies on a Mac too); and parity with ReaHotkey only ever
      concerned Windows, where nothing moves. What follows is the 2026-08-20 reasoning, kept for
      the record. The proposal was to
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
      ..."). If a specific combination actually fails on a Mac, that one moves. Nothing moves
      on a prediction. (This said "to a neutral token, not to a `host.os.pick`". The tokens
      `Mod` and `Global` were built on 2026-09-22 and removed again the same day with the
      roles; a module that wants another combination on a Mac picks one with `host.os.pick`.)
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
- [ ] **Kontakt 8's view toggle could use UIA where it is reachable.** Standalone, a raw walk lists "Play View" as a real Button; Windows sends F10 instead, which works there and cannot drift the way a measured menu row can. On a Mac F10 is not Kontakt's key (measured 2026-09-18: the key went out, the view stayed, macOS beeped), so the toggle reads Kontakt's menu by OCR (header.luau) — and there, pressing the published 'Play View' button by name would remove both the OCR dependency and the never-seen "Switch to Play View" wording. On Windows a nicety; on a Mac more than that, once a classic-view capture shows how that button behaves.

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

- [x] **The startup announcement is silent on every Mac, by default** — fixed 2026-09-04.
      `announce()` asked `via_screen_reader()`, which is `voiceover_speech() && is_running()`
      — the TRANSPORT switch, off by default. It now asks `a_reader_is_present()`, which is
      only the second half: is anybody listening. With the switch off and VoiceOver running,
      the line goes out through the system voice, which is what leaving that switch alone
      asks for. The rule it protects is unchanged: no reader running, nothing said.
      `via_screen_reader` is gone — the compiler pointed out it had no callers left, which is
      the whole finding: it had exactly one, and that one was asking it the wrong question.
      Original report follows. The startup announcement is silent on every Mac, by default. The tester: "I don't
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
- [x] **`window.active()` was the biggest thing blocking the pump** — fixed 2026-09-04, and
      the fix needed two rounds because the first one moved the stall rather than removing it.
      It had stalled the pump **fourteen times**, worst case **2605 ms**; the worst had
      nothing to do with F6 — it came while the tester was simply using the plug-in, a moment
      after a menu closed (`pid 1218 answered the frontmost-window question after 2455 ms`).
      This is the question every overlay asks on every tick.
  - **The cause, from `frontmost_window_element`'s own comment:** it read `AXFocusedWindow`,
      then `AXMainWindow`, then `AXWindows`, with the `is_busy` gate only at the TOP. The
      first read timing out quarantined the application, and the two fall-backs then asked
      the application we had just written down as not answering. `is_busy` is now re-checked
      between them (`window_after_fallbacks`), which separates the two cases exactly and
      needs nothing new to do it: an absent value does not set the quarantine and still falls
      through; a timed-out read does and stops there.
  - **And the quarantine now serves the last known window instead of "no window"**
      (`front_memory.rs`). Returning `None` is what every overlay gates its activation on, so
      a plug-in busy for a moment made the overlay switch itself off and on again. The rules
      that keep that from becoming a WRONG answer live in their own file so they can be
      executed rather than read: it is served only for the window the application itself last
      named, never past `STALE_LIMIT` (30 s, a guess, and the log says how old every served
      answer was), and an identity-only refresh keeps the snapshot's own timestamp. Ten unit
      tests, run on Windows through the `#[path]` borrow `keys.rs` established, and each one
      confirmed to fail when its rule is broken on purpose.
  - **The first version moved the stall one function along, and review caught it.** Serving a
      remembered window meant `host.window.active()` answered — so the overlay runtime went
      on to `host.window.controls()`, which walked up to 600 nodes into the same wedged
      application with no quarantine check at all, on the tick rather than on an app switch.
      The quarantine now also guards `win_info`, `window_controls` and `root_of` (which is
      the funnel for `find`, `find_any`, `locate`, `dump` and `focus_step`). Its own header
      had stated the principle three lines above the code that broke it.
  - **`foreground_window_id` deliberately does NOT serve memory.** Memory can answer "which
      window is in front"; it cannot answer what all four of that function's callers ask,
      which is a form of "what just changed" — `watch` calls it on an
      `AXFocusedWindowChanged` notification, where the remembered window is by definition the
      one that is no longer focused, and `tap::set_key_scope` FREEZES what it returns for as
      long as the overlay is active, so a stale answer there would not expire with the
      quarantine. A remembered handle also reached `queue::drain`, which resolves it on the
      pump thread with no gate at all.
- [ ] **Still open, and named rather than implied.**
  - **Candidate 2 is contradicted by a measurement already in this file, and is off the
      table.** The idea was to give this one question its own short messaging timeout —
      Apple's header allows it ("Setting the timeout on another accessibility object sets it
      only for that object"), so it looked cheap. But `MESSAGING_TIMEOUT`'s own doc records
      why it was RAISED from 0.25 s to 1.0 s: on the tester's 2015 Air with VoiceOver
      running, sforzando never once answered inside a quarter second, every read timed out,
      the application went into the penalty box and the overlay never saw a window it could
      plainly read. A short budget for the hot question would reproduce that exactly — and
      the memory added above does not save it, because an application that never answers a
      first time has nothing remembered to serve. The same probe measured an ordinary Tab
      step at 90-250 ms there against 23-34 ms on Windows, so a healthy answer on that
      machine can already approach the ~300 ms at which the tap is switched off. No timeout
      value fixes that.
  - **Which leaves candidate 3**, getting the question off the thread that carries the event
      tap. The structural answer and the large one, and now the only one. What was shipped
      bounds a wedged application to ONE timeout per five-second quarantine cycle; that is
      still ~1000 ms. Before starting it, read the next session's `window.active cost` line,
      which the probe can finally produce.
  - **`win_info` can still cost a timeout of its own on the healthy path.** `snapshot` is a
      batch, its fall-back is seven individual reads, and the quarantine is only consulted
      after them. The guard added there stops the SECOND timeout inside one `win_info`, not
      the first.
  - [x] **`note_foreground(0)` said "no window" where the truth was "not known"** — fixed
      2026-09-04. The third time this exact conflation has been found here, after
      `via_screen_reader` (which cost the tester his startup announcement) and
      `attribute_element_checked` (which made a timed-out read look like "nothing has
      focus"). `foreground_window_id` returns `Option<isize>` now: `Some(0)` is "it answered,
      it has no window", `None` is "it did not answer". Nobody writes the fabricated zero any
      more — `watch` leaves the tap holding what it had, `tap::set_key_scope` leaves its
      disagreement standing rather than resolving it wrongly, and the backend's key-scope
      snapshot says out loud when it falls back to a global scope.
    - And `active_window` now tells the tap what it just learned. The tap holds a NUMBER
      because resolving it inside the tap callback is the one cross-process call that must
      never happen there, and that number was told to it only by two notifications — so a
      notification arriving during a quarantine left it stale with nothing due to correct it
      while the user stays inside one plugin. The pump asks the same question every tick
      anyway; passing the answer on costs one atomic store, and only the FRESH arm does it (a
      remembered window is an answer about geometry, not about which window has the
      keyboard).
    - **Reasoned from the code, not observed.** The log line that would show it — "a key the
      overlay had claimed reached the application instead" — has not appeared in a tester's
      log. It is in the next session's list to watch for.
  - **`window_focus_chain` reads `AXFocusedUIElement` off the SYSTEM-WIDE element**, so it
      has no pid to gate on and its failure cannot arm the quarantine either (`element_pid`
      of the system-wide element is not an application's). During a quarantine `active()` now
      answers from memory while `focusChain()` still pays a full second and returns empty.
  - **`enumerate_windows` was not changed and now disagrees with `active_window`:** during a
      quarantine `find`/`list` are blind to that application's windows while `active()`
      returns one from memory, so a module can be told in the same tick that a window exists
      and cannot be found.
- [x] **The probe measured this question forty times and reached the backend once** — found
      2026-09-04 by the review, then measured rather than argued. `host.window.active()` is
      served from a per-tick cache in `lib.rs`, and the probe asked forty times inside one
      callback, which is one tick. The host's own accounting says it exactly: driving the old
      loop headless logs `epoch served 39 of 40 OS question(s) from cache (1 actually
      asked)`. It reported a distribution of zeroes and would have confirmed ANY change,
      including one that made things worse — and it was about to do that in the next session
      with the tester. Each ask now sits in its own timer hop; the same run logs no
      cache-served line at all, because the forty asks are in forty ticks. The output also
      says what it cannot tell apart: an ask served from the backend's memory is fast BY
      DESIGN, so the verdict line points at the log rather than declaring health.
## The instruments, audited on purpose (2026-09-05)

Three faults were found in instruments today and none in the product, so the next round was
aimed at the instruments themselves: what does each one MEASURE, against what it CLAIMS. Four
lenses, adversarially refuted, eleven findings standing. The three criticals are fixed.

- [x] **A dump that never ran printed as a measured zero, and the session's headline verdict
      was computed from it.** `dump()` returns an empty `Vec` before writing its own line when
      the handle is not live or the application is in the five-second busy quarantine — and
      **an empty Lua table is truthy**, measured: `ok=true type=table truthy=true count=0`. So
      `if not ok or not elements` could not catch it, the probe printed "0 element(s)", and
      `identifierSummary` went on to state "NONE of the 0 element(s) returned publish an
      AXIdentifier" — the sentence the file itself calls the one wrong answer that would send
      the nested-overlay design to image matching for nothing. Worse in combination with the
      20-second surface loop added an hour earlier: one timeout at surface 3 quarantines the
      application for 5 s, and every surface after it fabricates its own verdict. The backend
      now says `NOT WALKED` on its own line, and the probe draws no conclusion from an empty
      answer.
- [x] **"blank OCR: 0 word(s) — nothing invented" measured the host's own guard, not Vision.**
      The section deliberately picks provably flat rectangles; `ocr.rs` refuses to send a flat
      rectangle to Vision at all (`if plan.blank { return Ok(empty()) }`), so the harder it
      worked to prove the region uniform, the more certainly the recogniser was never asked.
      The guard's only trace was at TRACE level, off by default, so a remote log showed no
      sign of it and the section read as a pass. The guard now logs at line level and says
      what an empty result there does and does not mean. **I checked this instrument myself an
      hour before and called it sound** — the luminance readings do prove the capture was
      real, which is what I looked at; I never asked whether Vision was reached.
- [x] **The timer cadence measured the probe's own OCR.** `prev` was taken at scheduling time,
      so the first interval contained everything the hotkey callback still had to do — and it
      goes on to run the OCR batching comparison synchronously. The tester's log proves it:
      `10 x 100 ms took 1673 ms (shortest 104, longest 729)`, with an OCR read of 354 ms and
      three more totalling 374 two lines above. **354 + 374 = 728.** The verdict divided the
      total by ten and reported a 900 ms deadline as 1505 ms; the honest number was in the
      same line all along, `shortest 104`. My own `ROUNDS = 3` change earlier that day made it
      worse. Now: the first interval is not measured, and the verdict comes from the MEDIAN.
      Re-run here — `shortest 100 ms, median 115 ms, longest 141 ms`.
  - **The number reported to the user from this line was wrong**, and the correction matters:
      timers on that machine are not "dramatically slow". A 900 ms deadline is about 1035 ms
      there, not 1505.
- [x] **The macOS reuse path shipped an artifact with no probe in it.** It deletes
      `dist/.../modules` and repopulates from `modules/*/` only — `package-macos.sh` stages
      `tools/probe` on a separate line, and the reuse path did not. So a reused build (the
      30-second runs) advertised "today's modules" with the tester's only instrument missing,
      and nothing downstream noticed: the smoke step asserts that modules loaded, not that
      THAT one did. Fixed, and CI now names `com.tool.probe` in its assertion.
- [x] **The four remaining findings from that audit, built 2026-09-05.**
  - **The CI step that exists to catch a process abort could not see one.** `output` hands the
      utterance to a channel and returns; the AVFoundation call happens afterwards on the
      worker. So `SPEECH PROBE: done` was written BEFORE the selector that could abort was
      sent, `kill` and `wait` discard their status, and every grep is satisfied by lines
      already on disk — an aborted process left a log identical to a healthy one, and an abort
      is the single named failure the step was written for. The probe now reports again 1.5 s
      after speaking, both macOS steps ask `kill -0` before killing, and the module step also
      requires `listening for events`.
  - **The pump's stall breakdown had no term for key and hotkey dispatch**, so every one of
      the 31 stalls in the tester's log read `= 0x window-activate 0 + focus-change 0` — an
      equation that does not balance, printed by a line whose own comment says it exists
      because "one iteration took 729 ms names a symptom and no cause". It prints the
      remainder now, named as mostly key and hotkey dispatch.
  - **`0.0 ms inside the bindings` counted two of the six families in the same sentence** —
      and not `window.active`, the one binding with a stall line of its own. It says what it
      measures now: rebuilding Lua tables in `window.controls` and `window.focusChain`.
  - **`surfaces inside it: N` was a bounded walk's partial result printed as a fact**, and
      spoken aloud as the confirmation that the press landed. Four bounds can cut it and none
      was visible: the 256-surface stop never touches the budget, the depth cut returns
      silently, `CHILDREN_MAX` clips a child list, and the node/time notice hangs on the
      once-per-session flag an earlier walk will have spent. `window_controls` has the
      per-call line `dump()` was given, and the probe points at it.

- [x] **And the loop around that walk was bounded by nothing either** — fixed 2026-09-05, an
      hour after the walk itself, and found by searching the list rather than recalling it.
      The probe dumps the window's tree and then calls the same dump on EVERY surface
      `host.window.controls()` returned. Each dump is bounded; the number of them was not.
      `CONTROL_MAX` is 256 and `is_surface` counts `AXGroup` and `AXUnknown` — which is what a
      custom-drawn toolkit publishes for nearly everything it does not map — so the worst case
      was 257 walks of five seconds each: twenty minutes of frozen application, a log large
      enough to rotate the session away, and no spoken word until the end. A blind tester
      force-quits a frozen application, and that press is the one the session exists for.
      Never exercised, because the only Mac this has run on could not run a plug-in with any
      sub-surfaces (`surfaces inside it: 0`).
  - Bounded now by a wall clock across the whole phase and a cap on surfaces, with a line
      naming what was skipped, and a spoken line BEFORE the work so a tester can tell work
      from a hang. Verified end to end on a real window with six surfaces, then with the cap
      forced to two to watch the refusal appear.
- [x] **A macOS dump could not say it had been truncated** — fixed with it. The completeness
      notice lived inside `walk` behind a once-per-session flag, so the first walk to run out
      consumed it, and on a large plug-in the control walk (600 nodes, 300 ms) will do that
      before the dump is asked for. The dump's own line now says whether it stopped on nodes,
      on time, or not at all — the Windows side has done this per dump since the day before.
      The stake: the probe builds "N of M elements publish an AXIdentifier" from that dump, and
      that sentence decides whether the nested-overlay design ports or is rebuilt out of image
      matching. It now says "of the N returned" rather than implying it saw the whole tree.
- [x] **`probe-N.png` restarted at 1 on every launch and overwrote the last one** — fixed
      2026-09-05. Not a corner case on the machine this is aimed at: setting up a new Mac means
      granting a permission, quitting and reopening (macOS hands a new permission only to a
      process that started after it), and the protocol has him press the key in more than one
      step. The picture is half of what he sends back and the half nothing else can
      reconstruct. The count resumes from the first free name now; `resource` is declared for
      that one read.

- [x] **A macOS accessibility walk was bounded by nodes and by nothing else** — fixed
      2026-09-05, and found by asking what the next session's most valuable keypress points at.
      The probe over a Kontakt or Komplete Kontrol window is by a distance the largest tree
      this has ever dumped: sforzando's entire window came to **sixteen nodes**
      (`dump(6): 16 element(s), 16 node(s) visited`), against a budget of 2000. Every one of
      those nodes is a cross-process round trip whose own ceiling is the messaging timeout, so
      600 nodes was bounded at 600 seconds — which is not a bound. The Windows side learned
      exactly this the same morning, where a node budget still left one window taking thirty
      seconds.
  - Two deadlines, each with a reason rather than a number. `HOT_DEADLINE` is **300 ms**, and
      that is not a guess: it is the figure this file already names as the point past which
      macOS switches the event tap off, so a walk that crosses it has already cost the user
      their keyboard. `DUMP_DEADLINE` is 5 s, the same trade as Windows — a truncated dump is
      worth much less than a slow one, and nobody is waiting on the keyboard while a tester
      presses the probe key deliberately. The log now names WHICH bound stopped a walk,
      because a node budget reached wants a larger count and a deadline reached cannot be
      helped by any count.
  - **Extracted to `macos/budget.rs` and unit-tested here**, the third file to take that route
      after `keys.rs` and `front_memory.rs` — and the tests immediately failed. Not the
      expectations: the code. A walk whose node count landed exactly on zero consulted the
      clock on that last step and reported that it had run out of TIME when what ran out was
      nodes, which is the one attribution that sends the next reader the wrong way. Five tests,
      and the granularity of the 64-node clock check is now written down rather than assumed.

- [x] **The Retina audit found nothing, and that is the finding** (2026-09-04). Four lenses
      over the capture path, the OCR mapping, window and control geometry with mouse input, and
      the Luau side plus the documentation, each adversarially refuted. **No points-versus-pixels
      confusion survived.** The reason is in `capture.rs`'s own header: the destination bitmap
      is created at exactly the requested size in POINTS and the captured image is drawn into
      it, so `cap.w == w` and `rgba.len() == w * h * 4` are structural rather than hoped for,
      and "the scale factor never appears in the arithmetic, which is exactly why it cannot be
      got wrong". The one caller that wants the sharp image — OCR — gets the backing-store
      image plus the scale MEASURED from what came back rather than read from the display,
      because a laptop docked to an external display changes it mid-session.
  - **And the instrument for it already exists.** `first screen capture: 774x566 points at
      333,87 came back at 1.00x and took 20 ms` is in the tester's log today; on a Retina Mac
      that line says 2.00x, and it is derived from the returned image. Nothing needs building
      to answer the question — it needs one probe press and one line read out.
- [x] **A focus chain reported a window's frame as its client rect** — fixed 2026-09-04, and
      found by the audit above while looking for something else. `control_info` collapses
      client onto frame, correctly, for everything BELOW a window — its own comment says so —
      and `window_focus_chain` ends its chain with the AXWindow itself, which is the case the
      comment excludes. So `focusChain()`'s last link and `active_window()` described one
      window with a `y` a title bar apart, and the Windows backend, which fills a top-level
      window's client fields from `ClientToScreen`, disagreed with both.
  - Fixed now rather than filed, for a reason that is not its severity: it is a coordinate
      wrong by a CONSTANT, which is exactly what a points-versus-pixels fault looks like from
      the outside. Leaving a decoy of that shape in place for the one session that finally has
      a Retina display would have been the expensive choice.

## The Mac mini session with Kontakt 7 and 8 (2026-09-18)

The protocol was `docs/macos-session-mini-kontakt8.md` (the protocols are working papers and
are no longer in git — see .gitignore; they live in `docs/` on the maintainer's machine); the
machine the Mac mini M1 of the
third session (macOS 14.5, 1.00x), the build 839b211 from CI. Six analyses of the log, each
finding put to a refuter; what was fixed is in the commits of 2026-09-19. **Settled:**
Kontakt 8 inside REAPER anchors on its `Kontakt File Menu` button exactly where the Windows
geometry puts it (corner 238,52, centre 86,19), Load instrument and Save multi as work through
the menu read by OCR, sforzando works throughout, macOS 14 captures through the looked-up
legacy call, and Kontakt 8's own header File menu is an `AXMenuButton` whose menu a detector
sees. **Fixed:** the pass-through is a line, not a ring (Shift+Tab backs out, `[passthrough]`
per step, `[deactivate]` per loss); a hotkey moves the overlay's focus (`hotkeyKeepsFocus`
opts out; Melodyne's Menu bar does); by-name lookups inside REAPER fall back to the whole window
(`plugin_locate`); Info pane's Mac name; the view toggle goes through Kontakt's menu on a Mac;
F6 says the title only; Personal Voice unticks when not granted and says so in two sentences;
Accessibility no longer tells anyone to restart; the content rect of a window whose only
full-width child is a strip; busy subscriptions feed the quarantine; ~12,000 per-tick log lines
a session are gone.

Changes that reach **Windows** too, and want one check there: a hotkey now moves the focus
(Kontakt, Komplete Kontrol, Soundiron, u-he — ReaHotkey's own behaviour); Shift+Tab on Kontakt
standalone's "Kontakt controls" no longer enters Kontakt from its last element; Return on a
control that has hidden itself acts on its hotkey sibling or says "not available now"; the
`[image]`/`[poll]`/`[recheck]` log thresholds.

- [ ] **Why Kontakt 7 inside REAPER published nothing** (19 nodes, all REAPER's; in session
      three the same window had 47). Three candidates the log cannot separate: Kontakt 7 was the
      second NI Kontakt in that REAPER process, after a Kontakt 8 that published 73; the plugin
      format (VST3 or AU, unrecorded both times); a page drawn over its header. Measurement, two
      minutes: quit REAPER, start it fresh, insert Kontakt 7 FIRST and note VST3i/AUi, wait a
      minute, Cmd+Shift+F9 over the FX window BEFORE any F6 (F6's fallback click lands on
      Kontakt 7's logo button, top edge). Then Kontakt 8 into the same REAPER, Kontakt 7's window
      again, F9 again.
- [ ] **Kontakt 8's classic view has never been captured on a Mac.** Its view toggle now reads
      Kontakt's menu for "Switch to Classic View" / "Switch to Play View"; the second wording has
      never been seen on either platform, and whether the Windows view pixel reads classic on a
      Mac is unknown (a miss logs every word the menu read). One press, then Cmd+Shift+F9 in
      classic view: header composition, whether 'Play View' keeps its name, where the status bar
      sits, and whether the `[view]` line flips.
- [ ] **The Kontakt 8 status toggles' state on a Mac.** The Windows probe points miss by a point
      or two (the status bar ends 2 pt above the panel's bottom), and the buttons carry no
      `AXValue`; the Mac says "pressed" meanwhile. Re-measure the points from a Mac capture with
      VoiceOver's caption panel off.
- [ ] **Does Kontakt 8's standalone ring change size step to step?** 90 stops at entry; the tree
      changed while he walked it (78→77→75 nodes). The `[passthrough]` lines now give index and
      count per press. Also whether `AXFocused` on a preset row below its list's viewport
      scrolls the list (16 of 2708 presets are exposed, rows below the viewport are admitted).
      Only after that: a viewport filter for macOS `focus_step` — clip by ancestors that SCROLL
      (AXScrollArea, AXList, AXTable, AXOutline, a group with an AXScrollBar child), never by
      every AXGroup, ignoring ancestors with no rectangle.
- [ ] **The event tap on its own thread** — waits for the measurement, which is built
      (2026-09-19). Both switch-offs were the probe's ~1.2 s synchronous hotkey callback, not
      focus handling; the ~1 s focus stalls did not trip it because no key was waiting. The tap
      now notes every key-down that reaches it 250 ms or more after it was pressed, and the
      run-loop watchdog writes `N key press(es) reached the event tap late, the worst M ms …`
      at most once a second. Only if a session shows those lines in ordinary use is the move
      worth making (tap.rs's escape hatch, which needs queue.rs's pump-thread queues to become
      cross-thread). The timestamp's unit on Apple silicon is read two ways (`key_age.rs`,
      tested on Windows); if the line reports absurd ages, that reading is the first suspect.
- [x] **The probe in timer hops, with a busy guard** — 2026-09-19. The press-time part stays
      synchronous (window, surfaces, focus chain); every tree dump, the picture, the OCR, the
      text comparison and each answer run one event-loop tick apart, the three OCR-batching
      rounds each in its own; timer cadence and window.active cost AFTER the heavy steps, and
      the summary waits for them. `probe step 'X': N ms` per step. A second press while a run
      is going says "Still recording probe N"; a run that makes no step for two minutes is given up
      and stopped. The picture step notes when the window in front is no longer the one recorded.
- [ ] **CI signing with a stable identity — the maintainer's, at release.** Done by the
      maintainer, last, before the official release; not a task for a session. Only a tester
      who keeps grants across builds on their own Mac would gain before then — the borrowed Macs
      reset TCC on purpose. The recipe, so it cannot fail without a word: the p12 made with
      `openssl pkcs12 -export -legacy`; a keychain created, unlocked and put on the search list;
      a step after Package that fails when the secret is set and `codesign -dv` shows no
      `Authority=OSAP Local Signing`; the workflow file added to the reuse diff; the secret in an
      Environment limited to main. The key IS the privilege — anyone holding it ships a binary
      that inherits the grants.
- [x] **The menu watch's backstop honoured a menu for 150 ms of every 1.2 s** — 2026-09-19. A
      menu the accessibility check sees now keeps the check running every tick (each sighting
      extends `menuPlausibleUntil` by three ticks), so the keys stay with it while it is seen and
      come back once two ticks in a row no longer see it; a first sighting with no control's
      menu plausible, and the close, are logged once each. Capped at 30 s of continuous sighting,
      then back to the backstop cadence, because on Windows the check is a whole-tree walk and a
      plug-in whose tree always holds a menu element would otherwise walk every tick. To confirm
      on a Mac: open Kontakt 8's own File menu with VoiceOver from the pass-through for five
      seconds — one "sees a menu" line, one "has closed" line, and exactly one "gave up" /
      "took back" pair around them, not a pair every 1.2 s as before. One read that misses is
      tolerated (the keys stay with the menu), so a real close gives them back two ticks later.
- [ ] **`[observe] epoch served` is still a line per tick on a Mac** (3437 in the session): every
      steady epoch reaches the OS 2-5 times there. Line level only for `binding_us >= 1000`,
      pixels, or `reached_os >= 6`; replace `asked >= 1000` (which flags every long idle epoch,
      one key asked thousands of times) with a rate; and fix the comment at lib.rs:514-533 —
      image drains bump the epoch, recurring timers do not.
- [ ] **The pump line calls its residue "mostly key and hotkey dispatch"** and never measures it
      (`ev_counts.3` is documented as key-dispatch ms and written by nobody). Time on_hotkey and
      on_key, print keys/hotkeys and "outside the handlers" separately.
- [ ] **`host.element.locate` is not epoch-cached**, so on a miss each of the five VMs that carry
      Kontakt's code walks the window. macOS-only `located()` routing first; on every platform
      only after checking ON:EAR's polls on Windows don't start logging `[observe]` per tick.
- [ ] **Personal Voice after a grant listed 0 personal voices.** Ask whether that Mac has one
      recorded; if so, relaunch and read the startup voice line. And what the status reads right
      after refusing macOS's own dialog (the pump now unticks either way).

## Before the fourth macOS session (2026-09-12)

The protocol was `docs/macos-session-four.md`, replaced on 2026-09-20 by the compact
`docs/macos-test-protocol.md` (English) and `docs/macos-test-protocol.de.md` (German), which
cover both machines. Sixty-five risks were put to eight readers and
two refuters each; twenty-six survived. What was changed before the session is in the commits
of 2026-09-12. What was deliberately NOT changed, and why, is here — the reasoning is the
point: a session that cannot be repeated is not the place to land everything at once.

- [x] **The Kontakt 7 STANDALONE cell matches on macOS** — 2026-09-13, before the session after
      all: nothing in it needed the Mac first, because PROBE 2 of the third session measured the
      window whole (title exactly `Kontakt 7`, `AXWindow/AXDialog/`, FILE at content (176,19)
      against the authored (175,19)), and the window being NAMED "Kontakt 7" means the whole
      UIA-by-name path works there rather than falling back to coordinates. Shipped with the two
      things it needed: the standalone cell answers `inRack`/`inClassic`/`inEdit` false on macOS
      (Kontakt 7's `isRackView` asks whether VIEW exists, and PROBE 2 publishes VIEW while the
      window is all preset browser — six rack controls would have aimed into the search field),
      and macOS `focus_step` walks the whole window minus its title-bar buttons instead of the
      first child, which in that dump is Kontakt's logo, a button with no children. Kontakt 8
      standalone stays Windows-only: unmeasured.
      Open, and only the session answers them: whether Kontakt's Qt menus are readable by
      VoiceOver on a Mac; whether Kontakt's elements accept `AXFocused` at all (the same question
      F6 asks in the DAW step); and what the library overlays' 500 ms landmark polling costs on a
      Mac while a Kontakt window is in front — they bind to every cell, so this cell brings them
      along. Protocol four, step 5.
- [ ] **`Cmd+Shift+F5` sits one modifier from `Cmd+F5`, which switches VoiceOver off.** The
      protocol warns him and names the recovery. Moving the key would remove the hazard
      outright, but every candidate replacement is chosen from a shortcut table nobody here
      can read, so the move waits for a session that can check it.
- [ ] **`ownsPoint` refuses clicks on macOS and has never run there.** A wrong refusal costs a
      press and says so out loud ("something else is covering …"), which is a recorded failure
      rather than a silent one, so it ships as it is and the protocol names the sentence. If
      the session shows it refusing wrongly, the answer is to log the verdict and click
      anyway for a session, then re-arm with evidence.
- [ ] **The log's timestamps are whole seconds** (`logging.rs`, `epoch_secs`), so the one
      measurement step 3 exists for — did the Tab half a second after Return reach the overlay
      or the plugin — cannot be ordered from the log, only from the tester's ear. Milliseconds
      are a one-line change; not made mid-protocol because every line of every log the project
      has would change format in the same week a session runs.
- [x] **`prism-sys`' smoke tests crashed in CI** (`STATUS_STACK_BUFFER_OVERRUN`, run
      34638177265 on 2026-09-12 and run 35691602651 on 2026-09-22), both times passing on a
      re-run: cargo ran them on parallel threads, several opening every backend in turn, and
      prism's backends may not be used from two threads at once. They now run one at a time
      (a shared lock in `crates/prism-sys/tests/smoke.rs`, 2026-09-22).
## The third macOS session (2026-09-10)

The protocol is `docs/macos-session-three.md`; the results came back from a **different
machine** — a Mac mini M1 (`Macmini9,1`), macOS 14.5, arm64 native, no Rosetta, backing scale
1.00, run from the GitHub artifact (ad-hoc signed) by the tester over remote assistance on
somebody else's account. Twelve findings were drawn from the log and put to 28 refuters; seven
were wrong or too wide, and three of the corrections were defects in the backend nobody had
seen. The fixes are in the commits of 2026-09-10; what follows is what the session settled and
what it left open.

**Settled.**

- **Kontakt/KK on macOS publish no identifiers, and the walks were complete.** Kontakt 7's own
  tree is 34 elements (28 at depth 1), no `AXIdentifier` anywhere, a handful of names (FILE,
  LIBRARY, VIEW, SHOP, All Presets, the Brand/Sound Type/Character radio buttons, a Search
  text field, and a button titled literally `Hide <font color="#ffffff">2702 Presets</font>`),
  two scrollbars for the two lists, and none of the library tiles or preset rows. Inside REAPER
  the identical 34 appear as depth-1 children of REAPER's FX window, offset (+246,+210), with
  no container. Komplete Kontrol inside REAPER publishes nothing — 23 elements, all REAPER's,
  with KK's whole interface on screen per OCR. Kontakt standalone's window is subrole
  `AXDialog`, not `AXStandardWindow`. The design consequence is in
  `docs/nested-overlays-design.md`.
- **Both four-modifier chords registered and never arrived** (log 35/240/382/587; no
  "arrived" line; the probe's Cmd+Shift+F9 arrived four of four). Moved to Cmd+Shift-F5/F6 on
  macOS; registration warns about any chord on Control-Option while VoiceOver runs. The same
  F6 chord had worked on the tester's own Air in sessions one and two, and he confirmed it
  still does — the cause is machine-specific and NOT established; the VoiceOver modifier
  setting (Caps Lock lets Control-Option chords through) is the likeliest difference.
- **Launch 2 is the log the second session asked for**: with "Speak through VoiceOver" on, the
  start-up announcement went through VoiceOver (log 592-594). That question is closed.
- **The first launch spoke with the system voice because VoiceOver was running and the
  Automation grant was not yet there**; a sighted user without a screen reader hears nothing
  (`a_reader_is_present`, log 592). His question is answered by design, not by a change.

**Fixed 2026-09-10, from what the log showed.**

- [x] `content_rect` looked at 24 children and the title-bar buttons are the last ones: both
      Kontakt windows had content == frame, 28 pt wrong, every call. Every child now.
- [x] 2.0-2.3 s of every 3 s probe stall was `enumerate_windows` at the full timeout across
      every process; the tap was switched off four times. Bounded per application, hidden and
      background-only processes skipped, slow ones named; `find`/`findAll` ask only the
      applications a matcher names (`host.window.apps()`, `host.window.list({pids})`).
- [x] A busy refusal of the focus observer was retried only on the next application switch —
      sforzando's restarted process (all seven menus) and REAPER (all of step 7) never got one.
      Retried on a clock now, frontmost application only, never in quarantine.
- [x] The menu hold: `host.window.windowsOf(pid)` sees a popup that is a window of its own;
      `host.keys.passedThrough()` sees the Return/Escape that closed a menu nothing can see,
      and cuts the hold to 300 ms. The expiry line says whether the window list moved.
- [x] Personal Voice: denied and unsupported told apart, read live, one sentence in the log
      and in a dialog on macOS 14 too.
- [x] Five collapsed string literals; the probe's once-per-session error; the blank-OCR
      "measured" sentence (`OcrText.skipped`); the dump note about content == frame; the
      arbiter roster and idle observe lines (749 of 1985 lines); the read-out text logged.

**Open.**

- [ ] **Ask the tester**: the VoiceOver modifier setting on both Macs (VoiceOver Utility >
      General); whether `~/Library/Application Support/AutomationPlatform/` holds the log of
      the very first launch (the file he sent starts with Accessibility already granted, and
      the logger appends — the first launch wrote somewhere else or the folder was
      re-extracted); one Permissions-page row quoted back (he answered "yes." to the request
      for a quote).
- [ ] **The sforzando first-run dialog** is not accessible and sforzando refused accessibility
      for ~4 minutes after install (log 742-808). The overlay did NOT activate while the
      dialog was up — activation came the second the app first answered — so "DEF read as
      550." is still unexplained; the read-out text is logged now, so next time it will be.
      A probe press over that dialog would say what it is.
- [ ] **Step 7 was structurally dead**: no F6, and REAPER without an observer, so the in-DAW
      overlay (event-driven, no poll) could never re-evaluate. Both fixed; untested. The
      VOCR-click route still cannot wake it — a keyboard moved into a view exposing no AX
      element fires no notification (`daw-hosts` says so) — only F6's own recheck can.
- [ ] **CI artifacts are ad-hoc signed** (`package-macos.sh` falls back when no identity is
      in the keychain; the workflow has none). Every new download is a new identity and all
      four grants are lost. A self-signed certificate exported once and kept as a GitHub
      secret, imported by the workflow, would give artifacts one identity; needs the
      maintainer's hand.
- [ ] **ownsPoint is unanswerable on macOS** and nil is read as permission; unrelated
      windows (a Software Update alert, a 1892x1055 Safari window, the remote-assist window)
      overlapped every probed window's centre all session. Documented; not implemented.
- [ ] ~16 sub-threshold pump blocks per session on focus changes — `[recheck] 'sforzando'`
      re-asking `window.active` synchronously while Finder takes 106-229 ms to answer. Not
      reported by the 250 ms line; the front memory covers the busy case, not the slow one.
- [ ] The first Tab after a pause costs 134-166 ms against 30-58 otherwise, all in the value
      read: Vision goes cold after a few seconds idle. Measured, not addressed.
- [ ] The REAPER plugin-origin rule holds for Kontakt to 1-3 pt after the `content_rect`
      fix, with x consistently 2 pt left of the logo; whether `REAPER_LIST_GAP` (9) is 2 pt
      too wide for every plugin or REAPER's popup is inset cannot be told without a
      sforzando-in-REAPER probe from the same build.
- [x] Protocol step 5 asked for "a Dock icon for the error window" — a window cannot have
      its own on macOS; the application's icon while the window is up is the answer he gave.
      Closed 2026-09-20: the error window is answered (see below) and the step is gone from the
      protocol. Step 1 assumed a local build; the artifact route has its own text now.
- [x] `docs/macos-session-three.md` still names the old chords in its keys table; it is the
      protocol as sent and carries a note. Closed on 2026-09-20: the protocols left git, and the
      one that is current is `docs/macos-test-protocol.md`.
- [x] **F6 asks before it clicks** — 2026-09-11. It used to click three points inside the
      plugin panel's top-left corner, which for Kontakt 7 is its logo button (PROBE 3:
      AXButton at 349,341, the panel corner at 347,338). `host.element.focusWithin` sets
      `AXFocused` on the first focusable element inside the panel — the same API the
      standalone Tab pass-through uses — and the click remains only for a plugin that
      publishes nothing to ask (sforzando). Where nothing takes the focus and the plugin
      does publish elements, F6 now says so instead of clicking. Unverified on hardware.

## The second macOS session (2026-09-04)

The protocol is `docs/macos-session-two.md`; his answers and log came back the same evening.
What it confirmed, in his words and in the log: the pitchbend region fix ("Pitchbend now reads
correctly when set to 1"), F6 ("Keyboard focus arrived! Hurray!" — five of five failed last
time), the Personal Voice guard added hours earlier (`this macOS has no such API — it arrived
in macOS 14`, where without it the process would have aborted), `window.active` at 1-2 ms over
40 asks with no stall, and the module list reading one row per press with its state.

- [x] **The menu hold was a safety net doing a mechanism's job** — fixed 2026-09-04. He
      reported it without recognising it: "Pitchbend's menu doesn't track when I use arrows and
      Enter anymore ... inconsistent with the Polyphony menu, which still works". Not two
      menus — two durations. Every menu in his log printed `a control opened a menu and no
      detector ever saw one`: sforzando's popup is not an `AXMenu`, so nothing on that platform
      can see it and the 2500 ms stopwatch IS the mechanism there. Under it his Enter reached
      the menu; over it the overlay had taken Enter back and only VoiceOver's own VO+Space
      still committed — and a longer list read out by ear takes longer, which is why the longer
      menu was the one that broke. Two changes: detection now ENDS the hold the moment it has
      proved it can see this plugin's menu (it used to go on forcing "menu open" until the
      deadline even where the answer was known, which is what made lengthening the deadline
      unsafe), and the unconfirmed hold is 8000 ms, the same guess as `MENU_PLAUSIBLE_MS`
      because it answers the same question. The expiry line now says the hold RAN OUT, so a key
      arriving just after it is evidence the number is still too short.
- [x] **A switch that ticks and does nothing** — fixed 2026-09-04, on his own suggestion. "The
      checkbox appears to be ticked but nothing happens. Would be clearer to present a dialog
      saying this is unsupported and ideally, saying the minimum supported OS version." He is
      right: on and inert is indistinguishable from broken. Ticking Personal Voice on a macOS
      without the API now says so and names macOS 14.
- [x] **The arrival announcement cut off the sentence that brought him there** — fixed
      2026-09-04. "When speaking through VoiceOver, the first control that gets focus
      interrupts the notification of where keyboard focus has gone." Every announcement in the
      runtime interrupted, which is right for one a keypress asked for and wrong for arrival.
      `speakControl` takes a `queued` flag and the activation path uses it. Bounded honestly:
      `interrupt` governs OUR queue only — whether VoiceOver cuts itself off is VoiceOver's
      decision — so this fixes the case where both lines are still ours, which is this one.
- [ ] **The line the protocol told him to watch for cannot tell the bug from normal
      operation.** It fired eight times in his log, every time as `because a plugin menu is
      open`, which is by design. Only `because the scoped window is not frontmost` is the
      defect. Either split the two log lines or, in the next protocol, name the second half.
- [ ] **Timers are far worse than the 5% previously recorded:** `10 x 100 ms took 1673 ms
      (shortest 104, longest 729)`, so a 900 ms watch deadline is really about 1505 ms there,
      and a single hop can take 729 ms on its own. Every settle and deadline in every module
      was tuned on Windows. Measured twice in the same session (1673 and 1637 ms).
- [x] **Batching OCR reads is NOT worth building, and the instrument said the opposite** —
      corrected 2026-09-04, before the work was done rather than after. The numbers are `one
      774x120 region took 354 ms and read 28 word(s); the same band as three strips took 374 ms
      and read 26 word(s)`: three requests cost **5.6% more** than one, so an extra Vision
      request is worth about 10 ms, not the hundreds the earlier hypothesis ("Vision costs the
      same whether the region is tiny or the whole window") implied. Replacing three reads with
      one would save about 20 ms of an announcement that costs 90-250 ms on that machine.
  - **The verdict was `manyMs > oneMs`** — any difference at all, however small, read as "worth
      building", and the function's own doc comment two lines above claimed it interpreted
      nothing. So the instrument turned five per cent into a change to shared runtime code,
      days before a session that cannot be repeated, and I was about to make it. The same
      failure as the `window.active cost` verdict: a threshold that cannot tell the finding
      from the noise.
  - Fixed in the probe: best of three rounds rather than one sample, the per-extra-request cost
      reported as the figure that actually decides, and a stated threshold (50 ms) with the
      reasoning beside it so a reader can disagree with the rule rather than only the
      conclusion.
- [ ] **`enumerate_windows` is now the biggest stall on that machine** — 1291 ms and 1033 ms in
      the two probe runs, against `active_window`'s worst of 329 ms. It is the probe's own doing
      (`host.window.list`), so no shipped module pays it today, but the `find`-with-an-`app`
      clause noted elsewhere in this file is what would bound it.
- [ ] **The VoiceOver transport is free, and that answers the question set before the
      numbers:** `50 lines through VoiceOver: 0 ms on average, 0 at best, 0 at worst`, against
      `an osascript that talks to nobody took 212 ms`. So `output` returns when VoiceOver has
      the text rather than blocking until it is spoken, and no change of transport buys
      anything. Close the question.
- [ ] **Space in the module list needs VoiceOver interaction.** "Space in the row without VO
      interaction doesn't work ... I could VO interact, then VO+Space worked." Our own Space
      handler (written precisely so a VoiceOver user is not left reading a list he cannot
      change) is never reached, because VoiceOver takes Space first. He asks whether that is
      expected of an `NSTableView`; it needs Apple's own documentation read rather than a guess.
- [x] **The startup announcement now says what happened to it** — 2026-09-04. It could end
      three ways — shown as a notification, spoken, or deliberately dropped because nobody is
      listening — and the log recorded none of them, so a silence that was correct and a
      silence that was a fault looked identical. All three are logged now, and `Speech::say`
      names the transport it chose whenever that CHANGES (a line per line would bury a log an
      overlay writes to on every focus step).
  - **And two silent early returns were found on the way, which may be the whole fault.**
      `note_frontmost_before_gui` gives up without a word when the frontmost application is
      already this one — which is likelier on a relaunch — and `restore_frontmost_after_gui_start`
      then finds nothing to hand back and also returns in silence. The front is never given
      back, the agent stays in front of an application it never took the front from, and it has
      no window for a screen reader's cursor to land on. That is the exact shape of "the VO
      cursor ended up in no-mans-land", and it left no trace at all. Both say so now.
- [ ] **The failing launch itself is still unreproduced.**
      "On first launch the expected prompt spoke with system TTS and VO cursor remained in
      Finder. However on a second launch, I got no feedback when OSAP was set to speak through
      VoiceOver, and in that failed case the VO cursor ended up in no-mans-land." The log we
      have contains ONE session, in which the switch was still off at start-up and went on nine
      minutes later — so the case he describes is not in it. Ask for the log of a failing
      launch; and independently, make that launch explain itself, because the startup line
      records neither which transport it took nor whether anything accepted it.
- [ ] **Qt is still unanswered and no longer his problem.** Kontakt and Komplete Kontrol will
      not run on macOS 12.7.6, and the Qt applications he could think of are equally
      unsupported there. It moves to the next session's protocol instead, on a machine where
      they run.

- [x] **Personal Voice would have killed the application on the tester's Mac** — found
      2026-09-04 by an adversarial review of the test protocol, before he was asked to press
      it. `request_personal_voice` called
      `requestPersonalVoiceAuthorizationWithCompletionHandler` with no guard, twenty lines
      below `personal_status`, which guards its sibling class method and says exactly why:
      "a selector that does not exist is not a `None` but a dead process". The selector
      arrived in macOS 14; his machine is 12.7.6. And the path is the one the guard exists
      for — `personal_status` answers `None` where the API is missing, `Speech::pump` reads
      `None` as "nobody has been asked yet", and that arm is what sends
      `Job::AuthorisePersonal`. Guarded now, with a log line naming the version. The draft
      protocol had told him to tick that switch as an early step and called "it still works"
      a pass, which was a fact nobody had measured.
- [x] **The Mac we already own was being asked one question** — fixed 2026-09-04. The macOS
      job packages a bundle, links it, and loads every module once; that was all. Speech is
      the one new subsystem a runner can actually exercise, because every accessibility call
      needs a TCC grant a runner does not have and speaking needs none — and the macOS speech
      path had never run anywhere but a build machine. `tools/speech-probe` (loaded by path,
      deliberately NOT packaged: it speaks, and this application does not speak unprompted)
      walks the wiring: `engines()`, `use()`, `output()`, `engine()`. The job fails if it does
      not reach the end, if no voice was chosen, or if a call was refused or ignored.
  - The first thing it settled cost nothing to find: reading the existing CI log showed the
      new path already works there — `the speech engines took 0 ms to open`, `191 system
      voice(s) installed, 0 of them personal, read in 282 ms`. So the worst risk, an
      unrecognised selector aborting the process (objc2 0.6.4 emits no availability cfgs),
      is retired for macOS 15. The tester's 12.7.6 is still only covered by the
      `respondsToSelector` guards, which are runtime checks and hold regardless.
  - **And the workflow comment was wrong about its own runner.** It said Accessibility being
      ungranted means the event loop refuses to start. The log says `listening for events
      (headless)` and then `headless: running the CoreFoundation run loop`. Timers fire there,
      which is what makes the step above possible at all.
  - `tools/**` is now in the job's paths filter. It packages one tool and runs another and
      watched neither — the third time this exact gap has been found here.
- [x] **Two speech tests were flaky, and the same measurement explains both** — fixed
      2026-09-04, after one of them turned main red. Neither failure was caused by the change
      that exposed it, and both had been dismissed twice as "environmental, CI is green".
  - They waited for `!engines().is_empty()`, which is the wrong condition: the nine Windows
      entries appear together, but `sapi`/`onecore` are marked available only once the library
      has answered — so the list goes non-empty while nothing in it can speak. They wait for a
      usable voice now.
  - **They also starved each other.** Every `engines()` call queues a refresh on the same
      worker, so a poll loop generates the work it is waiting behind; two of them in parallel,
      with one opening SAPI (a look while an engine is opening was measured at 2.7 s), never
      converge. Measured: alone both pass in 4.7 s, together one spent its whole ten-second
      budget and found nothing. Serialised with a mutex rather than `--test-threads=1`, which
      would slow the other 49 tests for the sake of these two. The suite is now FASTER (10.1 s
      to 4.6 s) and passed five consecutive runs.
  - **And it came back the next day, because I fixed the condition and not the duration.**
      The wait was 10 s; on a CI runner the first of the two tests spent all ten and failed,
      while the second found the same voices 1.2 MICROSECONDS later. The first always pays the
      cold open — each test builds its own `Speech`, and the fallback worker opens the speech
      libraries before reading its first job (2.0 s for SAPI, 3.5 for OneCore on an idle
      machine, evidently far worse on a contended one). Thirty seconds now, and a failure
      prints how long it waited and what the list held, because Windows always has sapi and
      onecore: an empty list there is a worker that has not finished, not a machine that
      cannot speak.
  - **What actually failed on CI was neither**: the assertion `took < 5 ms` on a single
      wall-clock sample of a nine-element vector clone, which measured 6.4 ms on a shared
      runner where `cargo test` runs in parallel. It takes the cheapest of five reads now —
      scheduling can only inflate a sample, and the property still holds, because work moved
      back onto the caller's thread would cost 35 ms in every sample.
- [x] **`host.speech.engines()` can answer EMPTY for seconds after start-up**, and neither the
      docs nor anything else said so. The first snapshot is taken behind the scenes so that a
      session which says nothing pays nothing, and a module asking early is told there is
      nothing. Measured while writing the probe: 282 ms on a macOS runner, and on ONE Windows
      machine two consecutive runs at **500 ms and 5500 ms** before a plain voice could be
      chosen. No shipped module calls `use`/`engines`, so nothing is broken today; a module
      author who asks once would have been. `docs/api/speech.md` says it now, including that
      "nine entries, always the same nine" is only true once the list has filled.
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
- [x] **`host.element.rawDump` did not return on Windows** — fixed 2026-09-04, and the
      diagnosis was wrong twice before the measurement settled it. First guess: the Chromium
      subtree. Measured by timing the dump over all 21 top-level windows on this desktop,
      Chromium answered in 65-374 ms for up to 2876 elements — and the run stopped dead in
      front of a **wxWidgets** window and stayed there for minutes.
  - The cause: this platform set no accessibility call timeout at all, where the macOS
      backend has bounded every AX call since it was written and says why in its own header.
      UIA waits about two minutes for an application that is not answering.
  - The first fix was a silent no-op, and only the log line caught it: `CUIAutomation` does
      not implement `IUIAutomation2`, so asking it for the timeouts returns E_NOINTERFACE.
      Created from `CUIAutomation8` now, with a fall-back, and the timeout is announced once
      per thread.
  - A node budget bounds nodes, not time. With the timeout in, a WinUI window took thirty
      seconds instead of never — so the walk carries a five-second deadline too, and says in
      the log when a dump is PART of a tree rather than all of it. Verified by forcing the
      deadline to a millisecond and watching the line appear.
  - **What is left, measured and not fixed:** that WinUI window still costs ~23 s, and it is
      not in the walk — it is `BuildUpdatedCache(TreeScope_Subtree)`, which materialises the
      whole subtree in one call before the deadline can see anything. Caching per LEVEL
      (`TreeScope_Children`) would bound it and still keep most of the win the subtree cache
      was measured to bring (211 ms per walk down to one crossing instead of six per
      element). Worth doing when something other than a diagnostic needs that window.
- [ ] **The original note, for the record.** `host.element.rawDump` did not return on Windows, twice, over a WinUI window. Found
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
- [x] **The error window on macOS — confirmed on a Mac**, in the session before the Mac mini
      one of 2026-09-18 (reported by the tester; no log of that session is in this repository).
      All three answers came back good: the application is promoted while the window is up (the
      log says "activation policy set to regular (Dock icon, in the app switcher) … accepted:
      true", and back to
      accessory on close), VoiceOver reads the message, and the window keeps the application's
      Dock icon when the module manager is closed underneath it — the refcount, which is the
      bug it was written to prevent. Note for the wording: a window has no Dock icon of its own
      on this platform; what appears is the application's, for as long as the window is open.
      The probe no longer raises a module error on purpose (removed 2026-09-20): it put a
      window on screen and took the focus off the plug-in just recorded, once per launch, for
      questions that are now answered. `git show 228029d:tools/probe/src/answers.luau` has the
      code if it is ever wanted again.
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
  - **Three findings from the review, two of them structural.** `Speech::use_engine` and
      `say_for` went cross-platform while the LUA BINDINGS in `lib.rs` stayed
      `cfg(windows)` — so for one commit macOS listed voices no module could select and
      `host.speech.use` did not exist there, which is precisely the rule this project treats
      as the important one. And Personal Voice was unreachable by construction: the code asked
      for authorisation only when the chosen voice was already marked personal, but macOS does
      not put a Personal Voice into `speechVoices()` until it IS authorised. The circle is
      broken by a switch — "Offer my Personal Voice to modules" — which is a deliberate act by
      the person at the keyboard, the same shape as the VoiceOver Automation permission.
  - **And one that would have killed the application at launch.** `voiceTraits` is not
      available on every macOS this bundle promises to run on (12.0), the objc2 bindings carry
      no availability information whatsoever, and an unrecognised selector is not a `None` —
      it is an Objective-C exception through a Rust frame. Three sources gave three different
      answers for when that selector arrived (10.15, 13.0, 14.0), so the code asks the runtime
      with `respondsToSelector` instead of trusting any of them, and the answer goes in the
      log. The class-method check needed the METACLASS, which is its own trap: `responds_to`
      on a class object answers about instance methods and would have reported "no" for a
      selector that exists.
  - Unrun, and these are what the next session settles: does the system voice speak at all,
      does a chosen voice change what is heard, does the Personal Voice switch raise its
      dialog, and — from one log line — which macOS versions actually have `voiceTraits`.
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

## In-memory templates for image search (2026-09-21)

The design, what was built and why: [`docs/in-memory-templates-design.md`](docs/in-memory-templates-design.md).
Built this round, uncommitted, pure Rust plus bindings: the matcher moved to
`crates/host/src/template.rs` with a verbatim copy of the old one as the reference its
equivalence test compares against (step 1); a per-template cache of scaled variants, bounded
by the haystack before anything is built, 16 entries and 16 MiB (step 2); `catch_unwind` per
entry and per batch on the image worker (step 2); `host.screen.template` with `rgba`, `rgb`,
`capture` and `file` sources, a 32 MiB per-VM budget and the `VmOwner` owner/generation fix
(steps 4–5); `host.screen.imageSearchEach` and handles in every search (step 6); alpha forced
to 255 in the Windows capture; `host.json.decode`; the doc bugs in `resource.md`, `require.md`
and `screen.md`. Bindings and worker are in `crates/host/src/image_search.rs`.

- [x] **Step 3 waits for the developer's answer: what is one cell of his signatures?** A pixel
      at a fixed place, or the mean of a block. Until he says, none of this is built: sparse
      point templates, `lo`/`hi` ranges, `outside` checks, template-level `tolerance`,
      `maxMiss` and the ±2 px refinement after a first hit with misses, nor the
      `host.screen.cells{ region, cols, rows }` block-mean reducer that replaces them if a cell
      is a mean. `template::Body` has one variant so the sparse one is an addition, and
      `host.screen.template` refuses `points`, `tolerance` and `maxMiss` as unknown fields
      today, so adding them later changes no existing behaviour. If a cell is a mean, give the
      colour test a `kind` byte (critique issue 1) so a channel-difference predicate can follow
      without an API change.
  - Answered (2026-09-21): a cell is the share of a block's pixels that pass a colour test, so
      the reducer was built instead of the sparse templates: `host.screen.cells` (2026-09-22,
      "Grid cells and window regions" below). Points, ranges, `outside`, `maxMiss` and the
      refinement are not needed for it and are not built; a template colour test would reuse
      `cells::Predicate` (see the last item there).
- [ ] **Measure the variant cache (step 2).** `match_ms` from the `[image]` batch line on the
      Kontakt and Soundiron landmark polls and on the gtoggle scale ladder, before and after
      this change. Expected: scaled scans faster (probes again, no resize per call); unscaled
      ones unchanged. Not measured — it needs the real libraries on screen.
- [ ] **The live self-test (step 8), on Windows, needing nobody to set up a screen:** capture a
      24x24 patch from the middle of the manager window (reject flat patches with a `profile`
      min/max check), search a region 200 px larger on each side — the hit must be the capture
      origin, and `imageSearchAll` must find exactly one — and log and speak the results.
- [ ] **Confirm the owner fix live:** with a Kontakt library loaded and its overlay active,
      (a) disable the overlay runtime (`com.platform.overlay`) in the manager and re-enable it,
      and (b) disable the LIBRARY module itself and re-enable it. Both times the library's
      landmark gate must keep searching, and after (b) its overlay must come back only while
      that library is really loaded. Before the fix, (a) stopped the gate for good, silently;
      with the first version of the fix, (b) did, and left it claiming its last answer.
- [ ] **Confirm alpha on Windows:** `host.screen.save` of any region now writes a PNG whose
      alpha is 255 throughout (the measurement that prompted it found 255 already; the forcing
      is for the machines where it is not). Duplication now does the same off the desktop: the
      canvas starts as opaque black (`dxgi.rs`, `capture`), so both paths give the same bytes
      there (decided 2026-09-21). A point on no monitor reads black on both paths too
      (`colorref_rgb` maps CLR_INVALID to black; it used to be white). Unmeasured on a real
      multi-monitor setup.
- [ ] **Optional hardening: a time-based in-flight stamp in the overlay runtime** (critique R7).
      No longer needed for a disable: an answer that arrives while its owner is disabled is
      now held and the search sent again on re-enable (`image_search::Fate::Hold`), so the
      landmark gate's `pending` flag is always cleared by a callback while the VM lives. A stamp
      (`host.now()` instead of a boolean, as the `imageSearchEach` doc example does) would still
      guard against a callback lost some other way.
- [ ] **Call-level `tolerance` outside 0–255 silently becomes 0, an exact match.** Every search
      reads it through `image_search::read_tol`, unchanged on purpose because changing it
      changes what existing modules get; clamp numbers to 0–255 and raise on anything else,
      as its own step.
- [ ] **macOS, never run on a Mac:** the template and JSON code is platform-free and its unit
      tests run in the macOS CI job's `cargo test`, but nothing of `host.screen.template{
      capture = … }` or `imageSearchEach` has run against a real Mac screen. Whether the
      capture's downsample from backing pixels to points is a box average is NOT established
      (`capture.rs` asks for `CGInterpolationQuality::Low`; Apple does not specify the kernel):
      add a measurement to `tools/capture-probe` — a known 2x2 checker at backing resolution,
      read back at point resolution — before the docs say anything about it.
- [ ] **`host.resource.readBytes`**, so a packed binary template can be shipped as a file and
      handed to `rgba` without a detour through JSON numbers.
- [ ] **A `capture` template must stay a live read when DXGI capture lands**: never served from
      a frame cache (`docs/screen-frame-sharing-design.md` on calibration paths). Holds by
      construction after the merge with desktop duplication (2026-09-21): `template{ capture =
      … }` reads through the module's source on the event loop, every call, and duplication
      answers with the most recently composed frame, not a cached one. Unchecked on a real
      duplication module.
- [x] **Porting the capture feature onto this** (2026-09-21, the merge of both features). Its
      design edited image-search code that lives in `crates/host/src/image_search.rs`, not
      `lib.rs`, so its hunks were ported by hand: one `backend::CaptureFn` alias, the capture
      feature's (several regions and a `CaptureSource`); `ImageTask` carries `source`, set in
      `enqueue` from `capture_source::read_source`; `run_batch` keys a frame on `(region, source)`
      through `capture_source::frame_keys` and `capture_frames`; `imageSearch`,
      `imageSearchMulti` and `imageSearchAll` capture through `read_source`, which also makes
      the VM's once-per-VM comparison line; `imageSearchAsync` and `imageSearchEach` put the
      source into the task; and `template{ capture = … }` reads through it too, still live.
      Tested by `each_task_is_answered_from_a_frame_read_through_its_own_source`. Not yet seen
      in the application with a module that declares `[screen] capture = "duplication"`.

## Desktop duplication — what only a real machine can answer (2026-09-21)

Built: `[screen] capture = "duplication"` / `fallback = "none"` in `module.toml`, resolved per
VM (`crates/host/src/capture_source.rs`); the engine on its own `dxgi-capture` thread
(`crates/host/src/backend/dxgi.rs`), dormant unless a loaded module resolves to it; the
Application settings switch `desktop_duplication` (Windows, on by default); prewarm when a
declaring module's window trigger fires; shutdown on exit. Design: the revised design of the
2026-09-21 critique; API: `docs/api/screen.md#which-picture-a-read-sees`. Every verified
plug-in overlay declares nothing and reads through GDI exactly as before. Nothing is committed
until the maintainer confirms.

**Measured so far** (2026-09-21, reference machine: GTX 1060 6GB, 1920x1080 at 60 Hz, one
monitor; `cargo test -p host dxgi_ -- --ignored --nocapture`, a test build, not the app):
release build, median of 15 — 1x1 0.30 ms (GDI 15.35), 55x27 1.02 ms (GDI 15.83), 633x418
1.04 ms (GDI 15.68), 1920x1080 5.40 ms (GDI 28.27), every size byte-identical to GDI on the
ordinary desktop; `DuplicateOutput` 0-1 ms; first picture 9-12 ms after opening on a static
desktop; a 240x120 window of four known colours read identically by both paths. Direct3D
device creation 178-227 ms in seven runs (three test runs, four runs of the debug application
headless with `tools/capture-probe` and a late-reading variant) and **3505, 4271 and 3968 ms
in three runs of the debug test binary** straight after it was rebuilt; the cause is not
known. In the application: `pixel`, `profile`, `imageSearchAsync` (the capture build's worker
in `lib.rs`; since the merge the search paths live in `image_search.rs` and have not run
through duplication, see the port item under in-memory templates), `save`, `recognize` and
`recognizeMany` all answered through duplication under both fallbacks, the first-read
comparison line said the pictures agree, and under `fallback = "none"` the reads made while it
was still opening returned `nil` / the `error` table without raising. Those numbers are a
first data point for M1-M3 and M16, not their answer: the pump under a real overlay's load and
a GPU busy with a game are not in them.

**Seen by chance** (2026-09-21, same machine, after the review fixes): `dxgi_live` and
`dxgi_measure` ran while a D3D fullscreen game (SDL) was in front
(`SHQueryUserNotificationState` = 3). GDI read the test window's four colours, drawn topmost
over the game; duplication read a dark picture without them (mean luminance 14.7 against GDI's
105.6) — presumably the game, which suggests the display showed the game and not the composed
desktop GDI read. Duplication reads took about 33 ms at every size and GDI's full screen 240
ms, so the GPU was busy (M9). One run, not checked by looking at the screen: a lead for step 0
and M4, not an answer.

- [ ] **Step 0 — is OUR GDI path frozen for the game at all?** Nobody has measured it. His
      finding came from his Python reader, whose GDI path may be a window DC, PrintWindow or
      mss; ours is a screen-DC BitBlt of the composed image. Tool first, no sight needed: a
      probe module that logs `host.screen.profile` means of the game window every 250 ms while
      the title screen animates (changing = live, constant = frozen), plus the PresentMon
      present mode. If our GDI path is live, duplication is a speed feature and the
      `fallback = "none"` mode loses its reason.
- [ ] **M1** A read's cost inside the app: the pump round trip, small regions and full screen,
      with the observation line's duplication clause. (Test-build figures above.)
- [ ] **M2** Device creation and `DuplicateOutput` inside the app, the memory the driver adds,
      and above all whether the first picture after opening arrives at once and is complete.
      Explain the 3.5-4.3 s device creations above — cold driver, the new executable, or a
      debug build — and whether the app sees them.
- [ ] **M3** The same RGB bytes from both paths on Kontakt, Melodyne and the desktop, so
      existing templates would keep matching (temporarily add `[screen] capture =
      "duplication"` to their manifests; read the first-read comparison line).
- [ ] **M4** With our implementation, the game is frozen through GDI and live through
      duplication; its present mode (PresentMon) with and without a duplication held — does
      holding one force composition or add latency?
- [ ] **M5** Recovery after the game's fullscreen toggle, a resolution change, UAC, Win+L,
      sleep and resume, and a hot-plugged monitor; what the fallback returns meanwhile.
- [ ] **M6** NVDA Screen Curtain (and Magnifier colour filters): what GDI and duplication each
      return. NVDA documents that screenshots see black with the curtain on, so today's GDI
      path is affected already; the result could argue for duplication or against it. Part of
      the step-3 gate.
- [ ] **M7** HDR: duplication colours against GDI's.
- [ ] **M8** A hybrid (Optimus) laptop: `DXGI_ERROR_UNSUPPORTED`, a clean fallback, and a log
      line that names the remedy (Graphics settings, the application, Power saving).
- [ ] **M9** `Map` latency with a game holding the GPU at 100 %: are the pump's 60 ms, the
      worker's 250 ms and the 60 ms-per-300 ms budget (charged only beyond 5 ms per answered
      read) right? The chance run above saw 33 ms per read.
- [ ] **M10** Several monitors, negative coordinates, mixed DPI: coordinates and pixels match
      GDI; a region across two outputs is stitched correctly (the code path is unit-tested,
      never run on two monitors).
- [ ] **M11** The cost of reopening after the 30 s idle release (the device is kept), and
      after the device's own release at 5 minutes without a read.
- [ ] **M12** Whether duplication works at all on a GitHub Windows runner (Basic Display
      Adapter) — the CI live-test step prints it.
- [ ] **M13** With OBS display capture and his Python reader running, `DuplicateOutput` gets
      `NOT_CURRENTLY_AVAILABLE` and the fallback is clean.
- [ ] **M14** `WAIT_TIMEOUT` never hides a changed screen: a game whose frames bypass
      composition (independent flip, MPO) would be frozen for duplication too, which is the
      case for Windows.Graphics.Capture (design section 7).
- [ ] **M15** (WGC only) Whether an unpackaged app can turn the yellow border off —
      probably not: `IsBorderRequired` needs 10.0.20348+, a consent prompt and a package
      capability.
- [ ] **M16** The first frame after opening or reopening on a STATIC screen (a menu waiting
      for input): does a real picture arrive, or only pointer updates until something
      repaints? The engine takes a picture only from a frame whose `LastPresentTime` is not
      zero and otherwise answers `NoFrameYet` (then 250 ms of back-off). If this fails, hold
      the duplication while a declaring module's window is in front instead of releasing it
      after 30 s.
- [ ] **M17** A software, enlarged or coloured pointer: is it in the duplicated picture? The
      engine logs once when Windows shows a pointer that duplication does not report
      separately.
- [ ] **M18** Stability with RTSS/Afterburner, the Discord overlay or the Steam overlay, which
      inject on device creation.
- [ ] **M19** Whether GDI capture works at all in the GitHub runner's session. The CI live
      test and the second capture-probe run are informational (`::warning::`) until they have
      passed a few times; then make them assertions.
- [ ] Set the constants at the top of `dxgi.rs` from M1, M2, M9, M11 and M16 (they are the
      design's estimates).
- [ ] `tools/inspect` OCRs through GDI whatever the module being inspected declares, so an
      author inspecting a game that GDI reads frozen reads the frozen picture. Say so in its
      output, or let it take the source of the module it inspects.
- [ ] Later (design step 9): `Req::WaitChange` — a dirty-rectangle watch for short-lived help
      bubbles and for announcing a toggle when it actually repaints; Windows.Graphics.Capture
      per window if M4/M14 show duplication misses the game's frames; rotated monitors; the
      phase-2 question (duplication by default for every module), which needs M6 and a
      decision about holding one of a session's four duplication slots all day.
- [ ] macOS: `[screen]` is accepted and ignored there. Whether ScreenCaptureKit (or the
      CoreGraphics fallback) sees a game's frames the way duplication does on Windows — the
      design assumed it reads the compositor's output — has never been checked, so the API
      page says nothing about it.
- [x] Decided by the maintainer (2026-09-21): `GetPixel`'s `CLR_INVALID` off the desktop now
      decodes as black, like every region read there, on both paths (`colorref_rgb`).
- [ ] **A `template{ capture = … }` made at load in a duplication module is cut from the
      standard picture**, or is `nil` under `fallback = "none"`: nothing opens duplication at
      load (`prewarm_hook` runs only before a window trigger's callback, on purpose), and the
      first read of a session spends the pump's one wait on the comparison read, so the
      template's own read finds the engine still opening. A search read that falls back is
      corrected by the next poll; a template keeps its picture for the session, and where GDI
      reads the application frozen or black it then never matches, silently. The same holds
      for one made just after the window came forward, and for `host.screen.save`. The API
      page says so (`screen.md`, `host.screen.template` and `save`). Check it live with a
      declaring module, then decide: return `nil` from the constructor when the fallback was
      `Fallback::Opening` (the switched-off, too-large and rotated-monitor fallbacks must
      still read the standard way, or the template would be `nil` for good), or give this one
      read a longer wait on the pump (changes the pump budget, and still does not cover a cold
      driver open of seconds).
- [ ] In the application, not yet seen: the prewarm running before a declaring module's first
      trigger callback (headless runs have no window trigger); the per-module first-read
      comparison line; the settings checkbox's off-and-on replacing a stopped or stuck capture
      thread; the device released after 5 minutes idle; the image-search paths as ported into
      `image_search.rs` — `imageSearch`, `imageSearchMulti`, `imageSearchAll`,
      `imageSearchAsync`, `imageSearchEach` and `template{ capture = … }` — with a module that
      declares `[screen] capture = "duplication"` (one headless capture-probe run under each
      fallback).

## Game controllers (2026-09-21)

`host.gamepad` exists: the hub and its names (`backend/gamepad/mod.rs`, `names.rs`), the host
wiring (`gamepad_api.rs`: listeners, `pad_deliveries`, replays, demand, epochs, the
process-wide `host.now()` origin), the Windows XInput source (`win_thread.rs`, `xinput.rs`), the
macOS GameController source (`apple.rs`, type-checked through `crates/macos-check` only), the
reference page `docs/api/gamepad.md`, and the probe's controller half (`tools/probe/src/
gamepad.luau`, Ctrl+Shift+F10 / Cmd+Shift+F10). **Nobody has pressed a real pad against any of
it.** What runs is the fake-driven unit tests and a headless start on a padless Windows 11
machine: `xinput1_4.dll` with ordinal 100, a high-resolution timer, probing every 2 s with a
listener, parked 5 s after the last `list()`, a parked `list()` answered in 3 ms. After the
review round (2026-09-21) a unit test also runs the poll timer itself — armed one-shot the way
the thread arms it, inside `MsgWaitForMultipleObjectsEx` — and on that machine 25 of 25 fired,
4.35 ms apart on average, longest 4.77 ms; the XInput read it drives still has never run.
Everything below needs a controller in a hand.

- [ ] **Windows, with an Xbox pad and a game in front** (the design's step 0, now
      against the real code): does `XInputGetState` return live data while the game has the
      focus, headless and with the manager window? The probe logs every event with its `age`;
      a press that never appears while the game is in front is the one finding that sinks the
      Windows half.
- [ ] **The real poll period and its cost.** With trace on, the pad thread logs once a minute
      "last 60 s of polling: … intervals <=4.5 ms …, longest …, thread CPU … ms". Read it with
      the game in front, on AC and on battery, and once with the power-throttling opt-out
      disabled (comment out the `throttling(true)` call) to see whether the opt-out is doing
      anything.
- [ ] **`age` from press to callback**, GUI and headless, from the probe's per-event lines.
      The design's estimate is 2 ms detection plus up to one 15 ms tick in GUI mode.
- [ ] **Hot-plug and sleep/resume**: unplug with a button held (the probe must log synthetic
      ups, then `disconnected`), plug back in (logged at once through the arrival notice, or
      only by the 2 s probe — the log's timestamps say which), sleep and resume with a
      wireless pad. The arrival notice is now `RegisterDeviceNotificationW` for
      `GUID_DEVINTERFACE_HID` on the thread's message-only window (SDL's way), no longer Raw
      Input: check it fires for a wired Xbox 360 pad, an Xbox One/Series pad over USB and
      over Bluetooth — each should announce its `IG_` HID collection — and that the status
      line says `arrival HID device notifications`.
- [ ] **Guide**: whether ordinal 100 reports it on this Windows, and whether Xbox Game Bar
      opening takes the focus from the game in a way that matters to a module.
- [ ] **The thresholds feel right**: Microsoft's dead zones (0.24 / 0.27 / 0.12), stick
      directions at 0.5 released below 0.35, triggers at 0.12 released below 0.08. Tuned on
      paper only.
- [x] ~~Raw Input's cost with a PlayStation pad attached~~ — gone with Raw Input: the arrival
      notice is a device notification now, which delivers no reports (review round,
      2026-09-21). Step 5's HID source will bring the question back, for the pads it reads.
- [ ] **The stick halves as a pair**: a listener now gets the partner half of a stick when the
      radial dead zone moves it (x back to the middle takes a y resting inside the dead zone to
      0 with it). Check with a real stick that the extra events read as right rather than as
      noise in the probe's axis lines.
- [ ] **Steam and DS4Windows**: Steam's desktop configuration turning pad presses into
      keystrokes, and DS4Windows' exclusive mode hiding the device, both change what we see.
- [ ] **The pygame 2 `joystick` column** in `docs/api/gamepad.md` (buttons 0–10, hat 0, axes
      0–5 for an Xbox 360 pad) is copied from pygame's own documentation, not from a run with
      pygame and a pad; the GameMenuReader port is where it gets checked.
- [ ] **macOS, on the next Mac session, with a pad**: background delivery while an emulator is
      frontmost and we are an accessory app; the log line "background monitoring was …,
      requested, and macOS now reports …" (and that setting it on the first connect does not
      crash — SDL's reason for doing it there); no TCC prompt appears; the bundle-identifier
      line, and whether `run-dev.sh`'s bare binary sees any controller at all (Apple forum
      667832: an empty bundle identifier left `controllers` empty); GameController with
      VoiceOver on (the macOS 15.4 notes list controllers going unresponsive with VoiceOver
      as fixed — the tester always runs VoiceOver); whether GameController applies a dead zone
      of its own, so ours comes on top; what `buttonHome` does; the labels and positions per
      family — above all whether a Nintendo pad's `buttonA` is the bottom button; the Elite
      paddles' order (`paddleButton1…4` mapped to P1, P2, P3, P4 = right_paddle1,
      right_paddle2, left_paddle1, left_paddle2); and `age` with the App Nap activity held.
- [ ] **Not built this round** (design steps 5, 7, 8): the Raw Input HID source for
      PlayStation, Nintendo and generic pads and its `gamepad_hid` switch in
      `appcfg::SWITCHES` — with the corrected precedent: SDL's RAWINPUT driver keeps only
      XInput-capable (`IG_`) devices, so background delivery of DS4/DualSense reports through
      `RIDEV_INPUTSINK` has no SDL backing and needs its own spike with a DS4 or DualSense,
      HID `ReadFile` (non-exclusive, never writing) being the fallback before SDL3. Then
      `O:onGamepad` in the overlay runtime (register on activation, release on deactivation).
      Later: chords (left out of v1 on purpose — the game gets the chord too, and a hold is
      `down` + `host.timer.after` + `state()`), an SDL `gamecontrollerdb.txt` import, an
      IOHIDManager source on macOS for pads GameController does not list, and a capture
      triggered by DXGI's next frame after a press. (The macOS CI job now fails on "gamepad
      watcher failed" like the Windows probe step.)
- [x] **A comment to correct** (design side finding): `backend/windows.rs` said the low-level
      keyboard hook "runs on its own thread" while it ran on the pump thread, which was the only
      reason its thread-local queues worked. True now (2026-09-22): the hook has a thread of
      its own (`keyboard_hook_thread`), and the state it shares with the pump is behind locks —
      see "The keyboard hook on a thread of its own" below. The two other comments that still
      said the hook ran on the pump (`backend/gamepad/mod.rs` on the key queues, `dxgi.rs` on
      why pump reads wait with a deadline) were corrected in the merge review.

## From the porting review of the documentation (2026-09-21)

A game-module port and two outside readings of `docs/` found places where the documentation
promised more than the host does. The pages now describe what happens; what follows is what
was planned, or found, and is not built.

Planned — the API pages say these do not exist:

- [x] **Threaded OCR.** Built 2026-09-22 as `host.ocr.read` — see "Text recognition off the
      event loop" below; the modules that poll with the synchronous calls move one by one, with
      an NVDA test each. What it was: `host.ocr.recognize` and `recognizeMany` capture and recognise on the
      event loop (`lib.rs`, the `recognize` binding; `RecognizeAsync(…).get()` and the untimed
      join of the Paddle thread in `backend/windows.rs`; the synchronous `performRequests`
      ladder in `backend/macos/ocr.rs`), which holds the keyboard hook for the whole read —
      estimated at 50–200 ms for a control label in `docs/screen-frame-sharing-design.md`, a
      cost ranking rather than a recorded measurement. Move recognition to a worker with a callback form, the way
      `imageSearchAsync` did for search, and keep the synchronous call for one-shots.
- [x] **OCR language normalisation**, with the threaded OCR — built 2026-09-22 (`ocr/lang.rs`,
      `fluent-langneg`), for `read` and the two older calls; one engine per language is not kept
      yet (O1 below). What it was: the module names a language once
      (`"de"`), and the host maps it to each engine's identifier. Today `lang` goes to
      `Windows.Media.Ocr` and Vision unchanged (`docs/api/ocr.md`, Recognition language). On
      Windows, ask `OcrEngine::IsLanguageSupported` instead of letting `TryCreateFromLanguage`
      fail, and keep one engine per language instead of creating one per call.
- [x] **Measure: does either engine take a bare `"de"` or `"en"`?** Superseded 2026-09-22: the
      resolver hands each engine a tag from its own list, so a bare tag never reaches one; what
      Vision does with a tag it does not list stays open as O6 below. It was never observed — no module
      in the repo passes `lang`. Windows: `Language::CreateLanguage("de")` and
      `TryCreateFromLanguage` on a machine with the German OCR component; macOS:
      `setRecognitionLanguages(["de"])` against `supportedRecognitionLanguages`. The docs make
      no claim about it until then. On the Mac, also pass an identifier Vision does not list
      and see what comes back: `docs/api/ocr.md` says only that a refused request is logged
      once and returns empty (`backend/macos/ocr.rs`, `warn_once("ocr-perform", …)`).
- [x] **Cancellable timers.** `after` and `every` return a token, and `host.timer.cancel`
      takes it back. Built 2026-09-22 — see "Timers, JSON encode and the window already in
      front" below.
- [x] **`host.window.onTrigger` for the window already in front** (`initial = true` in `opts`).
      Built 2026-09-22, reported on the next tick rather than at once — see the same section.
- [x] **`host.json.encode`**, with `host.json.array` for the empty list. Built 2026-09-22; a
      table with both an array and a hash part raises, naming the place — see the same section.
- [ ] **Screen snapshots**: an explicit frame handle — one capture, then several `pixel` reads
      and searches against it — idea B of `docs/screen-frame-sharing-design.md`. Today on
      Windows every `host.screen.pixel` call is a screen read of its own (about 16.7 ms on the
      standard path), on macOS only reads inside the last 256×96 tile within 5 ms share one,
      and no call reads several points from one capture. `host.screen.cells`
      (built 2026-09-22, "Grid cells and window regions" below) covers the block-statistics half.

Found while documenting — the behaviour is written down now, and wants fixing:

- [x] **`host.include` does not reject `..` on macOS.** The check was
      `std::path::absolute(root.join(rel)).starts_with(root_abs)` (`lib.rs`,
      `install_include`); on Unix `absolute` keeps `..`, so `"../other/x.luau"` passes and runs
      code from outside the module, and a second spelling of one file is cached under a second
      key and runs twice in one VM. Normalise the path lexically before the check, on both
      platforms.
  - Fixed (2026-09-21): `include_target` (`lib.rs`) cleans the path with path-clean before the
      check, refuses `\` and `:` as text on every platform, and uses the cleaned path as the
      cache key. Unit-tested (`include_tests`) on Windows; the same function runs on macOS, where
      the tests have not run.
- [ ] **A `"<modifier> tap"` hotkey — and `F21`–`F24` on macOS — is reported as held by
      "another application".** `host.hotkey.register` returns an id because `key_spec` parses
      the spec; the platform refuses it later in `refresh_hotkeys`, `report_os_conflict`'s
      dialog blames another application, and the claim is retried on every change to the
      enabled set. Raise at `register` for a spec the platform can never hold.
      Since 2026-09-22 the same dialog also answers a macOS hotkey on a letter that no key of
      the current keyboard layout types: `register_on_key` (`backend/macos/hotkey.rs`) says so
      in its error, and the log has it, but the dialog blames another application. That one
      can become holdable after a layout switch, so it wants a reason of its own in the
      dialog (the backend's error classified, not matched as text) rather than a raise.
- [ ] **A capture whose hook or tap could not be installed stays registered.**
      `host.keys.capture` pushes its entry and refreshes the captured set before
      `watch_keys()`; when that raises (macOS without the Accessibility grant), the token is
      lost with the error, and the key is suppressed and dispatched once a later capture
      installs the tap. Register the entry only after `watch_keys()` succeeded.
- [ ] **The keyboard hook is installed once and never checked.** Windows documents that a
      low-level hook that keeps timing out can be removed without notice; `KEY_HOOK_INSTALLED`
      would stay true and every capture would stop for the session with nothing logged. Not
      observed; worth a check after a long pump stall.
      Since 2026-09-22 the hook has a thread of its own that does nothing but answer it
      (`keyboard_hook_thread` in `backend/windows.rs`), so the pump's stalls no longer reach
      it, and a timeout takes a machine too loaded to schedule that thread. There is one
      witness: a hotkey that arrives through `RegisterHotKey` although the hook holds it
      writes "arrived through RegisterHotKey, not through the keyboard hook" to the log, once
      per window in front (`settle_hotkey`) — which also happens, harmlessly, in front of an
      elevated window. Still nothing re-installs the hook; see "Re-install a keyboard hook
      Windows removed" in the section on hotkeys in the keyboard hook.
- [x] **A module without the `window` capability breaks window delivery for itself.**
      `on_window_activate`, `on_focus_change` and `window_has_triggers` (`lib.rs`) reach the
      prelude through `lua.globals().get("host")`, which after load is the module's GATED view
      (`populate_vm` sets it to the `gated_view`), so `host.window` raises for a module that
      did not declare `window`. For an overlay module that relies on the runtime's manifest
      (the old `examples/overlay-attach`, `require = ["speech"]`), `window_has_triggers` is
      false and the foreground watch is never started on its behalf, so the overlay never
      activates. And while any other module keeps the watch running, every enabled module
      without `window` — a plain hotkey module included — gets a "window trigger" and a
      "focus change" error on every foreground and focus change: logged each time, shown once.
      Dispatch through the ungated table (`host_m`), which the prelude is already bound to,
      and skip a module that registered no triggers. The docs tell every module to declare
      `window` until then (`docs/api/index.md`, What to declare).
      Fixed (2026-09-21): `install_window_prelude` keeps the whole window table in each VM's
      named registry, and `window_has_triggers`, `dispatch_activate` and `dispatch_focus`
      go through that; a VM with no triggers is skipped before anything is converted. The
      gate still holds for module code. Unit-tested on a bare VM (`capability_gate_tests`,
      `window_events_*`); `examples/overlay-attach` is back to `require = ["speech"]`, and
      the docs say a module declares `window` only for its own calls.
- [x] **A dependency's `host.settings.onChange` outlives its owner's reload.** Dependency code
      gets the dependency's own settings table (`build_dep_host`), so a callback it registers
      is stored under `(dep_idx, key)` (`lib.rs`, the `onChange` binding), while
      `purge_module(owner)` removes only `(owner, …)` entries. After the owner is reloaded, the
      old VM's callbacks stay registered, keep that VM alive, and fire beside the new ones
      whenever the dependency's setting changes. Purge by the VM that registered them, not by
      the settings owner.
      Fixed (2026-09-21): each entry records the module that owns the VM it came from
      (`image_search::vm_owner`), and `purge_module` and `rollback_to` drop by that
      (`purge_on_change`, `rollback_on_change`; `on_change_ownership_tests` call those two
      directly, including that the old VM is released).
- [x] **A failed reload left the old VM delivering window events.** `reload_module` purged
      the old VM's registrations but left the VM itself in `m.lua` with the enabled flag
      unchanged, so `on_window_activate` went on dispatching into its prelude, and an overlay
      in it could activate and register hotkeys again for a module reported as inactive.
      Fixed (2026-09-21): a failed rebuild puts an empty VM in its place, which the dispatch
      skips (`window_events_skip_a_vm_without_the_prelude`). Not unit-tested end to end:
      nothing builds a `Shared` in a test.
- [ ] **Stale code comments.** `Shared::report_conflict` (`lib.rs`) says every enabled module
      that captured a key is dispatched to — `on_key` dispatches to the first. The doc comment
      of `reload_module` (`lib.rs`) and the Reload button's comment in `gui.rs` say dependents
      keep the old copy until restarted — `reload_module_tree` rebuilds them. The comment on
      `GATED` lists `match` as a free namespace; there is no `host.match`.

Verify on a Mac — the pages state these from the code, or no longer state them:

- [x] **Key positions.** Every spec went through the US-layout keycode table in
      `backend/macos/keys.rs`, so on a German (QWERTZ) Mac `host.input.send("Cmd+Z")` would
      have arrived as Cmd+Y. Superseded (2026-09-22): letters follow the keyboard layout now;
      what to verify instead is under "Hotkeys in the keyboard hook, and letters by layout
      on macOS" below.
- [ ] **Auto-repeat of a captured key.** Holding a captured arrow should fire the callback at
      the keyboard's repeat rate, as it does on Windows; the event tap matches every key-down
      and nothing filters repeats. `docs/api/keys.md` states it for Windows only.
- [ ] **Auto-repeat of a hotkey.** `backend/macos/hotkey.rs` expects Carbon to fire once per
      press and marks it unverified on hardware; `docs/api/hotkey.md` states once-per-press for
      Windows only. One deliberate press-and-hold.
- [ ] **The settings save on a Mac.** `Store::save` now replaces `settings.toml` through
      `tempfile` (a temporary file beside it, created 0666 less the umask; `sync_all`, best
      effort; `std::fs::rename`) and then flushes the folder (`settings.rs`,
      `replace_file`); `docs/api/settings.md` says so for macOS from the code. Never run
      there: the first macOS CI run after 2026-09-21 executes `store_save_tests`, whose
      failure case relies on renaming a file over a folder being refused, and whose mode test
      compares the store with a plain `fs::write`; check it is green, and that a setting
      changed in the application survives a restart with no `settings.*.toml.tmp` left beside
      the `.app`. Also never tried: the application in a folder on an SMB share or an exFAT
      stick. `sync_all` is `fcntl(F_FULLFSYNC)` there, which fcntl(2) lists only for HFS,
      FAT, UDF and APFS; the save should go through unflushed with one "saved settings …
      without flushing them to disk first" line in the log, and not fail.
- Background delivery of a game controller is already listed under "Game controllers"
  above; `docs/api/gamepad.md` no longer promises it for macOS.

## Driver-based features — ideas, to be decided later (2026-09-21)

Nothing here is planned yet. Each needs a driver or a system extension, which a portable,
unzip-and-run application does not have, so each is a deliberate decision of its own.

- [ ] **Taking gamepad input away from the game** (`host.gamepad.capture`, the counterpart of
      `host.keys.capture`). Gamepad input is observe-only by design: XInput, Raw Input and Apple's
      GameController only read a device, and no operating system offers a hook through which one
      application removes a button press before another sees it — unlike the keyboard, where the
      low-level hook (Windows) and the event tap (macOS) can drop a key. An overlay driven by the
      gamepad would need what remapping tools do (DS4Windows, reWASD, Steam Input): hide the real
      pad from the game and give the game a virtual one that receives everything the overlay does
      not consume.
  - Windows: a filter driver (HidHide) plus a virtual-pad bus (ViGEmBus, no longer maintained):
        kernel drivers, an administrator install, and a game without a pad if we crash.
  - macOS: seizing the device (`kIOHIDOptionsTypeSeizeDevice`) takes the pad from the game
        entirely; handing the rest on needs a virtual HID device through DriverKit, which needs an
        Apple entitlement.
  - Until then the hub is observe-only and the API reserves nothing for it; a later capture layer
        would sit on the same event hub.
- [ ] **A virtual MIDI device** that sends and receives chosen MIDI messages. To check first:
      macOS can create virtual MIDI sources and destinations through CoreMIDI without any driver;
      Windows traditionally needed a loopback driver (loopMIDI / teVirtualMIDI), and Windows MIDI
      Services (Windows 11) is said to add app-to-app endpoints — not verified here.
- [ ] **A virtual USB/HID device**, to emulate hardware. Windows needs a virtual-HID driver (a UMDF
      driver, or a third-party bus); macOS needs a DriverKit system extension with Apple's
      entitlement. The largest of the three.

## One build run for both systems (2026-09-21)

`.github/workflows/build.yml` ("Build") is now the only build workflow a push triggers. It calls
`windows-build.yml` and `macos-build.yml`, both `on: workflow_call` now, as parallel jobs of one
run, and names the downloads `automation-platform-<version>-<commit>-windows` and `-macos` (the
version from `crates/app/Cargo.toml`, the commit's first seven characters; the naming job warns
when the host crate, the Info.plist fallback in `package-macos.sh` or the Windows manifest says
otherwise). The YAML parses, and the calls, inputs, permissions, `needs` and outputs were
checked by script. Written without a run, though, so these are open until the first push
shows them:

- [ ] The run starts at all. A call that grants less than the called workflow asks for, or
      passes an input it does not declare, is refused before any job runs.
- [ ] Both downloads are in the one run, named as above, and `newer-macos` finds the macOS one
      by that name.
- [ ] The macOS reuse finds the previous Build run's `-macos` download. The first Build run has
      none and builds, since runs of the old `macos-build.yml` are not looked at. After that, a
      Luau-only push should log "reused the build from …".
- [ ] Two pushes in quick succession each get a complete run with both downloads. Nothing
      cancels a run any more; the old macOS workflow cancelled a superseded one.
- [ ] "Re-run failed jobs" after a red import check or capture probe (both run after the macOS
      upload) gets past the upload, which now overwrites the download the first attempt left in
      the run instead of failing on its name. The same for re-running the Windows job.

The earlier runs of "Windows build" and "macOS build" keep their history and their downloads until
those expire. Nothing triggers the two files on their own any more.

## Build numbers (2026-09-21)

The executable carries the commit it was compiled from (`git-version`, in `crates/app`), and
each package carries the commit it was made from in `build-info.txt` beside the executable or
the `.app`, written by `package.ps1 -Commit` / `package-macos.sh --commit`, which CI passes from
the naming job in `build.yml`. The log header says `version 0.1.0, build <commit>`, plus
`(binary built from <commit>)` when the two differ, and the Modules window's title ends with
`(0.1.0, build <commit>)`. Checked locally (unit tests, a headless run, `package.ps1` into a
scratch folder); these only a real run can show:

- [ ] The Windows job's capability step prints `log header: ... build <this run's commit>` with
      no warning. A `-modified` warning there means the runner's checkout changed before the
      compile; the `git status` below it says which file.
- [ ] The macOS job's module load prints the same, read from `build-info.txt` beside the `.app`.
- [ ] The first Luau-only push after this: the reuse step says "the package is build X, its
      executable was built from Y", the log header says `build X (binary built from Y)`, and
      the downloaded README's `Build:` line says both.
- [ ] Whether a reused macOS download keeps the permissions a tester granted to the build it
      reuses. Never observed either way. The bundle is left untouched for that reason (only
      `build-info.txt` and `README.txt` beside it change); a file inside it would need a new
      signature, and an ad-hoc signature is a new identity.

## One running copy, and what the libraries say (2026-09-21)

- [x] **A second start shows the running copy's module window and exits** instead of starting a
      second host (`crates/host/src/instance.rs`). The lock is wxWidgets' single-instance checker
      through wxdragon (a mutex named with the user's SID and session id on Windows; a lock file
      named with the uid, in `~/Library/Application Support/AutomationPlatform`, on macOS). The
      request goes over an `interprocess` named pipe (Windows) or Unix-domain socket (macOS), not
      wxWidgets' IPC, whose Windows form is DDE and hangs on any hung top-level window. The
      running copy answers from a thread of its own and the window's timer tick shows the window;
      on macOS the reopen event asks for the same. Headless runs are exempt. A copy that is
      quitting answers `quitting` from the moment Quit is chosen and is waited for (10 s); a copy
      that holds the lock and never answers is left alone. Checked here: unit tests for the names,
      the decision, the protocol, a real pipe conversation between two claims, and wxWidgets'
      mutex seen by a second holder and gone after release; a headless run logs the exemption.
      The macOS half is type-checked through `macos-check`, its tests included.
- [x] **Hardened after review** (same day): quitting stops the listener and waits for it before
      the lock is released, and a new copy retries listening for 3 s — before, the listener
      thread kept a pipe instance open until the process ended, so a copy restarted at once
      could not listen and nothing could reach it (a unit test restarts a copy and has a third
      start find it). The pipe admits only the user and SYSTEM (it granted read to Everyone), at
      most 4 connections are answered at once, and a second start connects at identification
      level and checks the answering process's session and user before it sends anything or
      hands over the foreground. A mutex this process may not open (an elevated copy's) counts
      as held instead of letting the start run unguarded. A start that gives up shows a system
      message box saying why (not responding, still closing, a different version, not
      confirmed as this user's) and says so in the log with the same reason. Quit says
      `quitting` before the window loop ends, and a Manager that fails to start says it too. A
      panic in a start that may still be a second copy appends one line instead of opening a
      session in the running copy's log, and that start reads the settings without rewriting or
      quarantining them. macOS: the lock file and socket moved out of the per-user temporary
      folder, which macOS empties of files untouched for three days.
- [x] **What the libraries say reaches the log** (`crates/host/src/logging.rs`): a `log` backend,
      and tracing's `log` feature for ort (ONNX Runtime's own messages included), written as
      `[dep:<crate>]`. Warnings and errors always, info and debug while trace logging is on,
      never trace; 20 lines per library per minute, with a notice when the limit is reached and
      the count before the library's next line. Checked in a headless run: with trace on, the OCR
      warmup's ONNX Runtime session start writes 20 `[dep:ort]` lines and then the notice;
      without trace, no `[dep:` line at all.
- [x] **The log header's OS line** comes from `os_info`: `os windows x86_64 — Windows 10.0.26220
      (Windows 11 Professional) [64-bit]` instead of the `OS` variable's `Windows_NT`.

Only a real session can show these:

- [ ] Windows: a second start while the application runs brings the module window to the FRONT,
      with the screen reader's focus in it. The second copy hands its permission to take the
      foreground to the running one (`AllowSetForegroundWindow`) before it asks; a window that
      opens behind the current one means that did not arrive or was refused.
- [ ] Windows: a second start while one of the manager's message boxes is open brings the message
      forward (not the window behind it, which the message has disabled) and the screen reader's
      focus lands in the message (`GetLastActivePopup` in `show_manager`).
- [ ] Windows: Quit from the tray and start again at once. The new copy should wait for the old
      one to finish and then start, the log should show one new session header, and a third
      start should then find the new copy (no `cannot listen` note in the new session; a
      `listening for other starts after … ms` note is fine).
- [ ] Windows, elevation, both directions: (a) started as administrator while a normal copy runs,
      the start should hand over to the normal copy and exit; (b) started normally while an
      elevated copy runs, the start should find the lock held (`OpenMutexW` refused), and then
      either hand over — if the pipe's own security admits it and the elevated process's user can
      be read — or end after 10 s with the "could not confirm that it is yours" message. Two
      hosts must never run. Which of the two (b) does is not known.
- [ ] Windows: the message box of a start that gave up is read by NVDA when it appears, and is in
      front. Case (b) above is the easiest way to get one.
- [ ] macOS, packaged: opening the running `.app` again from the Finder, and clicking its Dock icon
      while the window is open, both show the module window (the reopen event). Not known:
      whether Launch Services sends the reopen event to an agent application (`LSUIElement`).
- [ ] macOS, bare binary (`run-dev.sh`): a second start finds the first through the socket in
      `~/Library/Application Support/AutomationPlatform`, and the window comes forward in front
      of the Terminal. `show_manager` activates the application; the second process gives it no
      permission of its own on macOS.
- [ ] macOS: `wxSingleInstanceChecker` works before `wxdragon::main` there as it does on Windows (a
      lock file is created, a second holder sees it), the folder is created readable by the user
      alone, and the lock file and the socket are gone after Quit. The take-over of a lock file
      left by a crash, whose pid now belongs to an unrelated process (told apart from a live copy
      by `proc_pidpath`), has never run.
- [ ] macOS: the alert of a start that gave up (`CFUserNotificationDisplayAlert`, shown before
      wxWidgets exists) appears in front and VoiceOver reads it.
- [ ] macOS: the OS line reads `os macos aarch64 — Mac OS 15.x [64-bit]` or similar (os_info reads
      SystemVersion.plist, and `sw_vers` behind it).

## Module installation and the registry, hardened (2026-09-21)

From the crate audit (section C) and the review of the install path. All in
`crates/host/src/registry.rs`, `crates/module-manifest/src/lib.rs` and the install and
update flows of `crates/host/src/gui.rs`.

- [x] **A module archive could write outside the module folder.** A member named
      `top/C:\evil1.txt` was written to `C:\evil1.txt`, and zip's own `extract` lets it out too.
      Both crates are on zip 8.6 with only `deflate-flate2-zlib-rs` (zip 2.4 and the bzip2, zstd
      and xz C libraries are gone), and `module_manifest::unpack_zip` is the one loop both the
      install and a `.zip` package go through: every part of `enclosed_name` a plain name, no
      absolute name or drive, the archive's one folder first and something after it, and a
      256 MB budget checked on declared sizes and again on the bytes written. Names are checked
      before the installed folder is touched. Tested with archives written in the test.
- [x] **`id`, `version` and `entry` built unchecked paths.** `ModuleManifest::parse` validates
      them wherever a manifest is read; documented in `docs/module-package-format.md#names`.
- [x] **Search mangled `c++` and `a & b`, stopped after 30 results, and could hang an
      install.** `.query()` encodes the terms, search follows pages of 100 up to GitHub's
      1000-result ceiling, and one agent carries connect, response and body timeouts (10 minutes
      for an archive). Dependency ids are resolved lazily in star order instead of reading every
      manifest under the topic. URLs tested through a ureq middleware, without a network.
- [x] **A `"` in a branch name lost the update source.** `.source.toml` is written with the
      TOML serializer.
- [x] **The `.zip` package cache key used `DefaultHasher`,** which is not stable across Rust
      releases; it is SHA-256 (sha2) now. The package is unpacked beside its cache folder and
      renamed into place, so a half-written folder is never taken for a finished one.
- [x] **Dependencies were installed and hot-loaded without their capabilities being shown.**
      `resolve_tree` works out every module an install adds, pinned to the commit its manifest
      was read at, and writes nothing; the review dialog lists each with its capabilities in
      sentences, and the optional modules with their own buttons; `install_resolved` downloads
      exactly those commits and installs no module whose downloaded manifest differs from the
      one reviewed. An update that adds a capability or a dependency is reviewed the same way
      (`resolve_update`); one that asks for nothing new is applied as before. The CLI shows the
      same text.
- [x] **Review round (2026-09-21).**
  - The review text is built from one-line text: a module's name (also as "needed by"), its
      `supported_os`, an unknown capability name and an optional module's failure reason go
      through `one_line` (controls, line and paragraph separators and bidi controls become
      spaces; cut to 100 characters, 300 for a reason), so a name cannot forge lines.
  - A failed install or update no longer costs the installed version: `install_one` unpacks
      into `modules/.staging-…` (tempfile), loads it, compares the whole manifest with the
      reviewed one (`PartialEq` on `ModuleManifest`), and only then renames the installed
      folder aside and the new one in. `installed_in` and `find_module_dir` skip dot-folders.
  - A plan is refused when a module would land in a folder (compared without case) that holds
      an installed module with another id, or that two planned modules share; an optional one
      is left out instead. The install also refuses, before downloading, a folder that is not a
      readable module.
  - Archive names Windows would change or merge are refused before anything is written:
      trailing `.` or space, `<>"|?*`, case-insensitive duplicates and file/folder clashes, the
      superscript COM/LPT device names; so are encrypted members, compression other than
      deflate and stored, more than 10,000 members, and names over 400 bytes or 32 parts.
  - Module errors wait while a review is open (`reviewing` in `gui.rs`), so the error window
      does not take the focus from it; the `screen` capability's phrase now says it can save
      screenshots anywhere; a branch goes into the raw path percent-encoded
      (percent-encoding), so a module from `fix#12` sees its updates; dependency ids follow the
      id rule; the dependency lookup fetches search pages one at a time; a folder under
      `modules/` whose manifest fails is named in the log with the reason; unpacked files keep
      owner read and write on macOS.
- [ ] **Live: one install with dependencies and one update against the real GitHub.** The new
      requests — `commits?sha=<branch>&per_page=1`, `raw…/<sha>/module.toml`, `zipball/<sha>` —
      are tested only against a table, and so is a percent-encoded branch in the raw path
      (`raw…/fix%2312/module.toml`), which raw.githubusercontent.com has to decode. Then the
      review dialog with NVDA: the text is read when it opens, Escape and the close box cancel,
      and every button reads its label. The review is shown from the timer tick with its
      re-entrancy guard lifted, so the modules keep running behind it; that nothing else stacks
      a dialog meanwhile is argued in the code, not observed, and so is that a module error
      raised during the review appears only after it closes.
- [ ] **Live (Windows): an update while the module holds a file open** — a sound playing. The
      rename of the installed folder is expected to fail and leave the module as it was, with
      the "is a file in it open?" message; not yet seen.
- [ ] **macOS: the review dialog with VoiceOver,** Escape and the close box mapping to the
      `ID_CANCEL` button there, the permission bits `unpack_zip` keeps
      (`(mode & 0o777) | 0o600`), and the rename swap of a module folder on APFS.
- [ ] **Report upstream:** zip 8.6 `ZipArchive::extract` writes `top/C:\x` outside the target
      folder on Windows, and cap-std 4.0.3 lets `sub/C:x` out of its directory (both from the
      crate audit).
- [x] **A failed install replaced the installed folder.** An archive that failed while
      unpacking, a manifest that differed from the review, or a download without `module.toml`
      left the user without the installed version. Each module is staged now (see the review
      round above); module discovery skips dot-folders.
- [ ] **A failed multi-module install is not rolled back.** Dependencies are written first, so a
      failure leaves nothing that cannot load, but it can leave dependencies nobody uses.
- [ ] **Leftover staging folders are not cleaned up.** A crash mid-install, or a staging folder
      whose removal failed (logged), leaves `modules/.staging-…` behind. Nothing loads it; it
      has to be deleted by hand. Removing old ones at start-up would need a way to tell them
      from an install running in another process (the CLI beside the manager).

## Timers, JSON encode and the window already in front (2026-09-22)

Step 3, group A of the API work (the revised designs (a), (b) and (c) of the small-API
critique). Built and unit-tested; nothing of it has run in the real application yet.

- [x] **`host.timer.cancel(token)`**, with `after` and `every` returning the token
      (`crates/host/src/timers.rs`). Only the owning module's timers — the VM's owner, as
      `host.gamepad.off` rules; `nil`, an unknown token, a fired or foreign one answer `false`,
      and nothing raises. A callback that cancels another timer due in the same tick stops it:
      the due timers are collected as tokens and looked up again before each call.
- [x] **`host.json.encode(value, { pretty })` and `host.json.array(t?)`** (`json.rs`). Decode
      stays plain. The array marker is one frozen, unprotected table per VM, so
      `setmetatable`, `table.clone` and `table.freeze` still work on a marked table. Keys
      sorted by bytes; whole numbers up to 2^53 as integers; NaN, infinity, functions,
      threads, userdata, vectors and buffers raise with a path (`value.states[2].cels[1]`);
      sparse and number-keyed tables raise suggesting `tostring(id)`; cycles and more than
      127 levels raise. Round trips checked against serde_json's canonical form.
- [x] **`onTrigger { initial = true }`**: the prelude primes the trigger and asks
      (`_requestInitial`); the tick — GUI and headless — asks for the foreground once and
      calls `_dispatchInitial` through the host's own window handle; enabling a module primes
      its `initial` triggers again; an activation first counts as the report, a re-enable's
      queued one included. The foreground is asked only when some module has an `initial`
      trigger waiting (`_wantsInitial`), and a window without a title is no window, as on
      activation. Before the first matching callback the input epoch turns over once per
      report, and `capture_source::prewarm_if_declared` opens duplication once per VM. The
      tick's own timing line counts the report (`initial window report`). The headless
      `on_tick` also drains `host.window.recheck` now, which it never did. The drain is a free
      function (`drain_initial`) tested with a fake foreground; `apply_enabled`'s request and
      the purge/rollback clean-up of the queue need a `Shared`, which no unit test builds.
- [ ] **Live, Windows: `onTrigger { initial = true }` with the game already in front.** Load
      (or reload) a module whose trigger asks for it while its window is in front: the callback
      must run once, on the first tick, and a module that reads through duplication must log
      the opening before its first read rather than pay it inside that read. Enabling the
      module from the manager reports the manager window (it is in front then), so that case
      fires only on the next activation of the game — worth hearing once with NVDA to be sure
      it reads as intended.
- [ ] **macOS, never run on a Mac: `onTrigger { initial = true }`** — the only report a game
      that is already frontmost at load gets there, asked through `ax::active_window()` from
      the tick. Check it with an application already in front at load, and with one that does
      not answer accessibility (the docs promise the five-second memory of
      `host.window.active()` and, with no answer, no call). Also: enabling such a module from
      the manager asks the frontmost application through accessibility, from the main thread,
      and at that moment that is our own manager; check that the answer comes at once rather
      than after a timeout on our own process. A module without an `initial` trigger does not
      ask.
- [ ] **No module uses the three yet.** The overlay runtime still works around the missing
      cancel with generation counters (`Overlay:afterIdle`), and checks the window in front at
      load itself (`_recheck` after registering). Moving it onto `cancel` and
      `initial = true` is a behaviour change for every overlay and needs its own live test.

## Keys written once, said per platform (2026-09-22)

The key block of the cross-platform review (`xplat-critique`, "Do now" 1, 2, 3, 5 and 7).
Built on the positional rule of 2026-08-20 with two neutral tokens, `Mod` and `Global`; both
were superseded the same day by modifier roles ("Modifiers are roles, the Qt way", below).
Everything else here stands.

- [x] **One reader of modifier names.** `modifier_mask` in `backend/mod.rs` feeds the shared
      parser, which both `key_send`s and the Windows hotkey conversion now use (the second
      Windows parser is gone). Unit-tested for both platforms on Windows, and the Windows
      conversions to `MOD_*` and to the keys `key_send` holds have tests of their own. The
      neutral tokens `Mod` and `Global` it was built with are gone again, and the keys that used
      them are back to literal specs or to their per-platform picks (see the roles section).
- [x] **`host.keys.normalize`**, and the overlay runtime keys its claim map, its
      duplicate-claim check and its shared-hotkey lookups by it, so `Cmd+S` beside an inherited
      `Ctrl+S` is one claim. `host.hotkey.register` records and reports the
      normalized spec, so the log and the dialogs name the chord it is on this platform. (The
      reload key's log line reads `Ctrl+Alt+Shift+Win+F5` now, where it read
      `Ctrl+Shift+Win+Alt+F5`.)
- [x] **`host.keys.describe`**, and the runtime says a focused control's hotkey and a tab's
      `hotkeyLabel` through it: Control, Alt, Shift, Windows on Windows; Control, Option, Shift,
      Command on a Mac, with Backspace as "Delete" and Delete as "Forward Delete" there.
- [x] **`host.keys.check`** with structural reasons only — `parse`, `reserved`, `voiceover`
      (the whole Control+Option layer), `no-keycode`, `tap`, and the informational `altgr`
      (Windows, `ToUnicodeEx`) and `composes` (macOS, `UCKeyTranslate`); `{ layout = false }`
      leaves the layout unasked. No list of screen-reader keys. `host.hotkey.register` raises,
      with the reason, for everything no system can hold — `parse`, `reserved`, a tap, and
      `no-keycode` on macOS — where a tap or an F21 used to register and then end in the
      "Binding unavailable" dialog blaming another application. A test holds `register`'s
      refusals and `check`'s reasons together, and one reads every key spec in the modules,
      tools and examples and checks none of them is refused on either platform. The overlay
      runtime skips a refused control or tab hotkey (asking `check` without the layout, once
      per spec), guards the registration, and does not announce the key. `normalize`,
      `describe` and `check` need no capability (`FREE_MEMBERS` in `lib.rs`; `check` also
      reads the character the layout types with a Ctrl+Alt or Option chord), for a
      dependency's code too.
- [x] **Docs and examples:** `examples/window` matches `axRole = "AXWindow"`,
      `examples/settings` offers `en`/`de`/`fr`, and `docs/api/os.md`, `keys.md` and
      `building-an-overlay.md` present `pick` for differences in the other program only.
      (`docs/api/element.md` already named the capability `element`.)
- [x] **Plain-string control patterns on macOS** are logged once per pattern by the runtime.
      Komplete Kontrol, Melodyne and u-he declare `supported_os = ["windows"]`; Kontakt's
      optional import of Komplete Kontrol answers nil on a Mac, which it handles.
- [ ] **macOS: `composes` has never run.** `layout::character` in `backend/macos/layout.rs`,
      the module the letters by layout (below) are read in as well: one read of the layout,
      taken at start, at each input-source change and again at every `check` that asks the
      layout (so neither a late nor a missing notification leaves the answer on the old layout,
      and a letter that read moved is re-registered on the pump's next turn), one set of Text
      Input Source and `UCKeyTranslate` declarations (`ffi.rs`), and the input source a key is
      translated through kept with it. Type-checked, linked by the
      macOS CI build, and called by nothing shipped, because the runtime asks `check` with
      `layout = false`. At the next Mac session: `host.keys.check("Alt+E")` on a US layout
      (expect `´`, a dead key), `Alt+L` on German (expect `@`), once right after switching the
      input source (the new layout's answer), and once with a Japanese input method selected
      (the ASCII-capable fallback).
- [ ] **macOS: the calibrator on Command+Option+Shift** is unmeasured through the event tap,
      and two of its keys are menu shortcuts in many Mac applications: Command+Option+Shift+V
      is "Paste and Match Style" and Command+Option+Shift+S is "Save As". Only while
      calibrating and an overlay is active, and the tap takes the key first, but worth hearing
      once.
- [ ] **macOS: how VoiceOver says the described keys** ("Shift+Command+F6": whether the `+`
      is read as "plus", and whether the order sounds natural).
- [ ] **macOS: the plain-pattern log line** for Kontakt's in-DAW patterns (`^NIChildWindow%x+$`)
      should appear once per pattern at start-up; not yet seen in a Mac log.
- [ ] **Windows (live, with NVDA): the announcements** now say "Control+L" where they said
      "Ctrl+L", a control or tab whose hotkey the host refuses no longer says its key, and a
      Ctrl+Alt key checked on a German layout reports `altgr` with the character (seen in a
      headless run: `@`, `€`, `²` for Q, E, 2). The words are unit-tested; how they sound is
      not heard yet.
- [x] **The capture callback's modifier table** (`{shift, ctrl, alt, win}`) and a neutral
      `mod`/`cmd` field (xplat-audit A4): settled by the roles (2026-09-22). Its fields are
      roles, so on a Mac `ctrl` is Command held and `win` Control held; `ctrl` is the neutral
      field.
- [ ] **To confirm (maintainer): `check` without the `keys` capability.** `normalize`,
      `describe` and `check` are in `FREE_MEMBERS`, and `check` is the one of the three that
      reads more than the string — the character the current layout types with a Ctrl+Alt or
      Option chord. Kept free so that any module can ask before it announces a key; to be moved
      behind `keys` if that read should need the declaration.
- [ ] **To decide: should `check` say that no key of the current layout types a letter?** On
      macOS such a hotkey is accepted and parked until a layout that types the letter is
      selected (see "macOS: letters follow the keyboard layout" below), and `check` stays
      structural: `no-keycode` is F21–F24 only, and `ok` is true for the letter. A
      layout-dependent reason would have to be one that does not refuse (`register` does not
      raise for it) and would be asked of the layout even with `layout = false`.
- [ ] **`examples/overlay-attach` and the README still use Ctrl+Alt+1 and Ctrl+Alt+3**
      (`examples/overlay-attach/src/main.luau`, `README.md`, the overlay demo). Ctrl+Alt is
      AltGr on Windows, where a layout can type a character with 1 or 3 (`check` says `altgr`).
      On a Mac they are Command+Option+1/3 since the roles, off VoiceOver's layer. A key the
      overlay captures only while it is active would suit an example better.
- [x] **Letters by layout on macOS** (decided 2026-09-21 for step 3; xplat critique issue 1).
      Built in step 3 as well: see "macOS: letters follow the keyboard layout" in the next
      section, and what to verify on a Mac there. Letters in sends follow the layout with it
      — on a German Mac `host.input.send("Ctrl+Z")` is Command+Z, Undo.
- [x] **Hotkeys matched in the low-level hook as well** (decided 2026-09-21 for step 3).
      Built in step 3 as well, only for hotkeys whose `RegisterHotKey` succeeded, with the hook
      on a thread of its own: see the next section. A spec `host.hotkey.register` refuses (a
      tap, `reserved`, F21–F24 on macOS) raises before anything is registered, so it is never
      filed for the hook either (`hotkey_claim_for` in `backend/mod.rs` decides both, refusal
      first, and is tested with the hook's table), and every spelling of a role reaches the
      hook through the same shared parser (`parse_spec`) as `RegisterHotKey`. The dead Ctrl+Shift+F10 that raised it
      most likely had another cause (a second running copy holding the key), so what the hook
      adds in practice is unmeasured.

## Hotkeys in the keyboard hook, and letters by layout on macOS (2026-09-22)

From the key block of step 3 (the maintainer's decisions of 2026-09-21, "Hotkeys that games
switch off" and "Keys"; the cross-platform critique's first issue).

- [x] **Windows: a granted hotkey is matched in the keyboard hook as well**, AutoHotkey's
      `#UseHook`. WinUAE registers its keyboard with `RIDEV_NOHOTKEYS`, which silences every
      `RegisterHotKey` of every program while it is in front. `register_hotkey` files a
      combination for the hook only after `RegisterHotKey` granted it (a refused one belongs to
      another program and is never taken); the hook swallows it and queues the id, the
      key-up goes through, repeats are swallowed without firing, and a press with Alt, Win or
      Ctrl+Shift held gets the masking key 0xE8. A late hook's press and the `WM_HOTKEY` Windows
      posted for it anyway are paired by timestamp and dispatched once (no cap near one
      timeout on how late a call may be — several queued events each wait theirs — and an
      injected event or a stamp ahead of the clock counts as on time). A late call is judged by
      the modifiers the hook saw go by (`hotkey_hook::Mods`), a call on time by
      `GetAsyncKeyState`. The auto-repeat threshold follows the keyboard delay and FilterKeys
      (`repeat_threshold`, read with every grant). Key-downs the captures take and every
      key-up reach the hotkeys' record of held keys, and a granted combination the hook lets
      through (screen reader's modifier, key held before the modifiers) is noted, so its
      `WM_HOTKEY` is expected; the "arrived through RegisterHotKey" line is written once per
      window in front. Pure parts in `backend/hotkey_hook.rs` with 31 tests, `file_if_granted`
      tested with a refusal and against the backend's own `parse_spec`; documented in
      `docs/api/hotkey.md` (Windows).
- [x] **The keyboard hook on a thread of its own** (`keyboard_hook_thread` in
      `backend/windows.rs`), at the highest normal priority, doing nothing but answering the
      hook; installed with the first granted hotkey or capture, at once, no longer deferred to
      the pump. With the hotkeys matched in the hook it was present in every session — the
      reload key and daw-hosts' F6 are granted at start — and on the pump it made all typing
      on the machine wait for every OCR call and long callback (pump iterations of 400 and
      729 ms measured). The captured-key and hotkey state it shares with the pump moved from
      thread-locals behind short locks. `docs/api` (hotkey, keys, timer, ocr, screen, speech),
      `module-runtime-and-lifecycle.md`, `module-manager.md` and the pump's overrun line say
      so.
- [ ] **Re-install a keyboard hook Windows removed.** Not built. The only signal is a
      `WM_HOTKEY` the hook should have seen, and with the hook on its own thread several
      harmless cases look the same: a hook running late, whose `WM_HOTKEY` now usually arrives
      first (the two come from two threads); an elevated window in front, which would need an
      integrity-level check of the foreground process to rule out; a hotkey granted between
      the hook's lookup and Windows' own check. A re-install also puts our hook in front of a
      screen reader's in the chain, mid-session. Worth building if a removal is ever seen in a
      log (captured keys dead, "arrived through RegisterHotKey" lines in front of ordinary
      windows).
- [ ] **Live (Windows), with NVDA:**
  - an overlay's Alt+letter control hotkey (Kontakt's Alt+V, Alt+M) fires once, and
      REAPER's menu bar does not open when Alt comes up — the masking key's whole job;
      that NVDA says nothing for the masking key, with and without "speak command keys";
  - daw-hosts' `Ctrl+Shift+Win+Alt+F6` (`Ctrl+Alt+Shift+Win+F6` in the log) and the reload key still
      work, and releasing them opens neither Start nor the Office app; with Alt+Shift as
      the language switch, an Alt+Shift hotkey does not switch the input language;
  - NVDA+Space, NVDA+Tab and a hotkey pressed with Insert held behave as before (they go
      to NVDA or through RegisterHotKey, and the log shows no "not through the keyboard
      hook" line for them);
  - a hotkey held down fires once; a hotkey pressed in front of an elevated window (Task
      Manager) still fires, with the "arrived through RegisterHotKey" line;
  - after a deliberate main-thread stall (a module OCR loop), a hotkey pressed during it
      fires once, when the stall ends; with the hook on its own thread no "not dispatched a
      second time" line is expected;
  - during the same stall, typing in another application is not delayed at all, and a
      captured Tab in an overlay is swallowed (it reaches the plug-in neither during nor after
      the stall) and moves the overlay's focus once the stall ends;
  - plain `v` typed during a stall, with Alt pressed before the stall ends: no Alt+V fires,
      and no masking key is sent;
  - NVDA started before the app (the usual order): an overlay hotkey pressed in NVDA's
      input help (NVDA+1) runs the hotkey instead of being described, an NVDA gesture
      assigned to the same chord (Input gestures) does nothing, and "speak command keys"
      does not announce it — as `docs/api/hotkey.md` says; after restarting NVDA, NVDA gets
      all three back;
  - with FilterKeys on and a two-second repeat delay, holding a hotkey fires it once, and
      the log's "counts as auto-repeat" line names about 2200 ms;
  - WinUAE in front: a registered hotkey fires. Whether it helps in the developer's game
      (the developer's game, a native remake, not WinUAE) is unmeasured; capture-liveness logs
      which way a press came.
- [x] **macOS: letters follow the keyboard layout.** `backend/macos/layout.rs` reads the
      selected layout (`TISCopyCurrentKeyboardLayoutInputSource`, the ASCII-capable one when
      it types fewer letters, `UCKeyTranslate` over 48 keys) at start and after
      `kTISNotifySelectedKeyboardInputSourceChanged` (CF distributed observer,
      DeliverImmediately; the callback only raises a flag, and the pump — or `host.keys.check`,
      which reads the layout again whenever it asks it — reads it on the main thread);
      `keys.rs` holds the letters in force for Carbon registration, `key_send`,
      `key_post` and the tap's `keycode_to_vk`; `hotkey::reregister_letters` moves the
      hotkeys on letters. Digits, F-keys, named keys and punctuation stay positional. The
      table logic is tested on Windows with described German, French, Dvorak and Russian
      layouts (`backend/macos/keys.rs`).
      Also since the review: a second table read with Command held (`UCKeyTranslate`
      modifier state 0x01), used for every combination that holds Command, for "Dvorak –
      QWERTY ⌘" (`keys::letters_for`, tested with a described one); a hotkey that cannot
      follow a layout change is parked (null reference) and tried again at every later
      change, where it used to be lost for the session; the `keyboard letters` line is
      written at start and when a letter moved, not on every input-source change.
      Since the merge review: a hotkey on a letter no key of the current layout types is
      parked when it is registered as well (`hotkey::register`, logged), where it went back to
      the host as an error, reached the user as the "Binding unavailable" dialog blaming another
      application, and was not tried again after a switch; `key_send`, `key_post` and Carbon
      say "no key of the current keyboard layout types this letter" for it
      (`keys::why_no_keycode`); and a first read of the layout that happens late — the backend
      created off the main thread, `check` reading it first — re-registers the hotkeys already
      on US positions.
- [ ] **Verify on a Mac (letters by layout):**
  - US layout: the `keyboard letters at start` line says every letter is on its US position,
      and hotkeys, sends and captures on letters behave exactly as before letters followed the
      layout;
  - German layout: the log's `keyboard letters at start` line names
      `com.apple.keylayout.German` and `Y=0x06 Z=0x10`; `host.input.send("Cmd+Z")` in
      TextEdit undoes; a hotkey on `"Cmd+Shift+Z"` and a capture of `"Z"` answer to the
      key labelled Z;
  - French (AZERTY): `A=0x0c M=0x29 Q=0x00 W=0x06 Z=0x0d` in the line, and `"Cmd+1"` is the
      key labelled 1 (`&` unshifted);
  - switching the input source while running (menu bar or Ctrl+Space): a `keyboard letters
      the selected input source changed` line arrives while the app is in the background,
      the hotkeys on letters log "follows the keyboard layout", and the moved key fires;
      Cmd+Y and Cmd+Z registered together swap keys without a refusal;
  - Russian selected: the line shows the ASCII-capable layout supplying the letters;
      an input method (Japanese) selected: the layout under it is read;
  - "Dvorak – QWERTY ⌘": the line reports a table "with Command held" that is the US one,
      `host.input.send("Cmd+Z")` undoes in TextEdit, and a hotkey on `"Cmd+Shift+Z"` answers
      to the QWERTY Z key while a capture of `"Meta+Z"` (Control+Z) answers to Dvorak's Z;
  - a hotkey parked by a layout that cannot hold it (another application holding the chord
      on the new key) comes back, with a "held again" line, when switching back;
  - with "Automatically switch to a document's input source" on and two applications on the
      same layout, switching between them writes no `keyboard letters` line;
  - that the TIS and UCKeyTranslate declarations in `backend/macos/ffi.rs` link and
      answer: every start reads the layout through them for the letters, and
      `host.keys.check` asks the same kept input source for `composes`; neither has run on a
      Mac yet;
  - a hotkey on a letter no key of the selected layout types (no shipped layout found that
      has one; a custom layout that drops a letter, made with Ukelele, would): `register`
      returns without a dialog, the log says it is not held, and selecting a layout that types
      the letter logs "held for the first time" and the hotkey fires;
  - `host.keys.check("Alt+E")` right after a switch the notification has not reached yet
      answers for the new layout, and when that moved a letter, the next pump turn logs the
      hotkeys following it; how long a `check` that reads the layout takes (never timed).

## Modifiers are roles, the Qt way (2026-09-22)

The maintainer's decision of 2026-09-22, which supersedes the positional rule of 2026-08-20 and
the `Mod`/`Global` tokens of the same morning.

- [x] **The rule.** A spec's modifiers are roles: Ctrl, Alt, Win, Shift. On Windows and Linux
      each is the key of its name. On macOS Ctrl is Command, Alt is Option, Win is Control and
      Shift is Shift, as Qt's `ControlModifier` is Command there and its `MetaModifier` Control.
      The spellings are the same on every platform: Ctrl/Control/Cmd/Command are the Ctrl role,
      Alt/Option the Alt role, Win/Super/Meta the Win role. So a Mac author writes `Meta` (or
      `Win`) for the Control key, and `"Cmd+S"` is Ctrl+S on Windows as well. Before, `Cmd` and
      `Command` were the Windows key there, taps included (`"Cmd tap"` was a Windows-key tap and
      is a Ctrl tap now); no shipped module reads either on Windows, only in a pick's `macos`
      entry. Other combinations on a Mac: `host.os.pick`.
- [x] **Built.** `backend/mod.rs` has `modifier_mask` (no platform in it), the role table
      `role_words(os)` (spec word, spoken and short words, the platform's order), and
      `MAC_VOICEOVER_LAYER` = Win+Alt. It also has the macOS reserved list in role terms
      (`"Ctrl+Q"` is Command+Q) and `capture_mods_fields`, the table a capture callback gets.
      `parse_key_spec`/`key_spec` lost their platform argument; `normalize`, `describe` and
      `check` keep it. `backend/macos/keys.rs` has `MODIFIER_KEYS`, the one table from a role to
      the Mac key: Carbon bit, Quartz flag, `UCKeyTranslate` state, and the key codes of both
      sides. Carbon registration (`mask_to_carbon`), `key_send` (`mask_to_cg_flags`), the event
      tap's match (`mask_of_cg_flags`), the tap's modifier keys (built from the table in
      `tap.rs`), the Command letter table (`letters_for`) and the layout read all go through it.
      The tap asserts at compile time that the flag numbers are the bindings' own. `Mod` and
      `Global` are gone from the grammar, the docs and the tests. The keys that used them went
      back to literal specs: calibration `Ctrl+Alt+Shift+S/T/V`, and `Ctrl+Shift+F9`/`F10`/`F11`
      for the probe and capture-liveness. daw-hosts' back-into-plugin key and the host's reload
      key went back to their per-OS picks (`Ctrl+Shift+Win+Alt+F6`/`F5` on Windows,
      `Cmd+Shift+F6`/`F5` on a Mac, which is Command+Shift under the roles as it was before).
      Windows answers are byte-identical for every key spec in `modules/`, `tools/` and
      `examples/`: `windows_answers_are_unchanged_for_every_shipped_key` holds a golden table
      taken before the change.
- [x] **Refused on a Mac by the new rule, and moved (to confirm, maintainer).** The overlay
      runtime's tab keys `Ctrl+Tab` and `Ctrl+Shift+Tab` became Command+Tab and
      Shift+Command+Tab, the application switcher (`reserved`). They are now
      `host.os.pick { windows = "Ctrl+Tab", macos = "Meta+Tab" }` (and the Shift form):
      Control+Tab, the key they were on a Mac before and the Mac's own next-tab key. No other
      shipped key is refused on a Mac. `no_shipped_key_is_refused` reads every spec, and checks
      a pick's `macos =` entry for macOS only.
- [ ] **Next Mac session: every overlay key with Ctrl in it moved to Command.** For each: with
      the overlay active it fires, the application does not also act on it, and VoiceOver says
      it the way `describe` does.
  - Kontakt (`modules/kontakt`): `Ctrl+L`, `Ctrl+S`, `Ctrl+R` (`header.luau`) are
      Command+L/S/R. Previous/next instrument, `Ctrl+P`/`Ctrl+N` (both layouts in
      `geometry.luau`), are Command+P/N. Previous/next multi, `Ctrl+Shift+P`/`Ctrl+Shift+N`,
      are Shift+Command+P/N. All were Control+… before. Command+S, +P and +N are the DAW's
      Save, Print and New, which the overlay takes from it while active.
  - u-he (`modules/u-he`, `supported_os = ["windows"]` today): `Ctrl+U` and `Ctrl+S` are
      Command+U and Command+S.
  - Overlay runtime: go to tab n, `Ctrl+1`…`Ctrl+9`, is Command+1…9. Tab cycling stays
      Control+Tab and Control+Shift+Tab (picked, above). Calibration `Ctrl+Alt+Shift+S/T/V` is
      Command+Option+Shift, as with `Mod` this morning and Control+Option+Shift before that.
  - Tools: `inspect`'s `Ctrl+Alt+I/C/U` are Command+Option+I/C/U, off VoiceOver's layer now;
      Command+Option+I opens many applications' developer tools. The probe's `Ctrl+Shift+F9`,
      and `Ctrl+Shift+F10`/`F11` for the probe's controller key and capture-liveness, stay
      Command+Shift: they were Command+Shift picks.
  - Examples: `hotkey`'s `Ctrl+Alt+H` would be Command+Option+H, Hide Others in most
      applications' menu and taken from all of them while registered, so the example picks
      `Cmd+Shift+F7` on a Mac (Windows keeps `Ctrl+Alt+H`): it fires with F-keys set as standard
      function keys, or with fn held, and says "Shift+Command+F7". `keys`' `Ctrl+Alt+O`,
      `settings`' `Ctrl+Alt+S` and `overlay-attach`'s `Ctrl+Alt+1`/`3` are Command+Option.
  - Unchanged on a Mac: every `Alt+…` key (Option, as before, in Komplete Kontrol, Kontakt,
      Melodyne, Soundiron and u-he), the reload key and daw-hosts' key (Command+Shift+F5/F6).
      `hotkey-test-a`/`b`'s `Ctrl+Alt+Win+F8` also stays: Command+Option+Control+F8, still on
      VoiceOver's layer, a test tool.
- [x] **Review follow-ups (2026-09-22).** A spec that spells the Ctrl role both ways
      (`"Control+Command+F"`, `"Ctrl+Cmd+F"`) is a parse error naming the two words and `Meta`,
      rather than one key a Mac author did not mean (Command+F instead of Control+Command+F); no
      shipped spec does it, and on Windows it was Ctrl+Win before the roles and plain Ctrl
      after. The "Binding conflict" and "Binding unavailable" dialogs say a Mac key in
      `describe`'s spoken words ("Control+Shift+F6", not "Meta+Shift+F6"); Windows keeps the
      spec there and the log keeps it everywhere. A capture of a combination the Mac keeps
      (`reserved`) is not refused, and the log says so once per chord
      (`reserved_capture_line`). Command+W stays off the reserved list: the list holds the
      system chords with no Windows counterpart, and Ctrl+W closes a window on Windows too
      (maintainer to confirm).
- [ ] **Mac: the review follow-ups.** A binding conflict on a Mac (two modules on
      `"Ctrl+Shift+F9"`) puts up a dialog VoiceOver reads as "Shift+Command+F9". A module that
      captures `"Ctrl+Q"` writes one `the captured key 'Cmd+Q' is one macOS keeps for itself`
      line, once however often focus moves. Whether the event tap actually receives, and can
      suppress, Command-Tab, Command-Space and Command-Q before the system acts on them has
      not been measured; the log line and `keys.md` say only that the tap is asked for them.
- [ ] **Mac: what only a Mac can prove about the roles.** Carbon registers `"Ctrl+Shift+F9"`
      as `cmdKey|shiftKey`, and it fires on Command+Shift+F9. The event tap matches a capture
      of `"Ctrl+1"` on Command+1, and the callback gets `mods.ctrl == true` and
      `mods.win == false`; `"Meta+Tab"` matches on Control+Tab, with `mods.win`.
      `host.input.send("Ctrl+C")` copies in TextEdit, and `"Meta+A"` moves to the start of the
      line. `"Ctrl tap"` fires on a bare Command press and `"Win tap"` on a bare Control press,
      right-hand keys included. VoiceOver speaks `describe("Ctrl+Shift+F9")` as
      "Shift+Command+F9" and `describe("Meta+Tab")` as "Control+Tab". Under "Dvorak – QWERTY
      ⌘", `"Ctrl+Z"` takes its letter from the Command table. The flag numbers are checked
      against the bindings at compile time; everything else only by the tests on Windows.
- [ ] **Windows, live:** nothing a module does changes, so a spot check is enough. Kontakt's
      Ctrl+L and the calibration keys still fire, and the log's key spellings are the ones
      before.

## Seen in the first session with the log backend (2026-09-22)

- [ ] **wxdragon warns once at start:** `[dep:wxdragon] warn: Warning: C++ returned invalid
      TreeItemId pointer 0x..., rejecting`, logged right after the installed list is filled. It
      was invisible before our dependencies' messages reached the log. Find which tree call
      returns the invalid item (the module manager's tree, gui.rs) and whether anything the user
      sees is missing because of it.

## Grid cells and window regions (2026-09-22)

Step 4 of the API work: `host.screen.predicate`, `cells`, `matchCells` and `matchCellsAsync`,
and the window-relative Region form, built to reproduce an outside game-menu reader's signatures
bit for bit. The rules are in `crates/host/src/cells.rs` and `region.rs`, the bindings in
`lib.rs`, the worker job in `image_search.rs`. Built and unit-tested; nothing of it has run in
the real application yet.

- [x] **`region.rs`:** `{ window = w, fraction = { x1, y1, x2, y2 } }` resolved from the window
      table's `client` with the reader's own formula in f64 (floor for starts, ceil for ends,
      then his pixel clamps), so it never raises for a fraction outside 0..1 or an end before
      its start; only non-finite numbers and wrong types raise (`region_lua.rs`).
      His eight bounds cases are a unit test.
- [x] **`cells.rs`:** the predicate string on winnow (C# as pasted, a canonical form, errors
      with the column in characters), multiplied out into integer conditions; blocks with the
      one-pixel minimum; the cell as his `Math.Round(255.0*m/t)` in f64 (`round_ties_even`),
      held to the exact integer half-even on all 582,266 midpoints up to 200,000 pixels; hex
      (the `hex` crate) as the only exchange format; similarity with an integer sum; items by
      name and the runner-up. All 2^24 colours are checked against a Rust copy of his condition.
- [x] **Golden test against his package** (`cells_golden_tests.rs`): the frame's checksum, both
      vectors' bounds, all 720 blocks of his tables (edges, counts, byte) and both hex strings
      reproduce. The package is a frame of a commercial game, so it lives only in
      `crates/host/tests-data-local/` (ignored by git); the test says it skipped where the
      folder is missing, which is every CI runner.
- [x] **Bindings:** options and states read strictly by serde through mlua and
      `serde_path_to_error` (paths 1-based); `nil, reason` for an empty client area, a failed
      capture and a capture of the wrong size; `matchCellsAsync` as `Job::Cells` on the image
      worker, sharing frames with the searches of its batch, read through the VM's capture
      source; a region that did not resolve is answered on the worker without a capture, so
      the callback always comes. `region.rs` and `cells.rs` are borrowed into `macos-check`.
- [x] **Review fixes:** `host.json.decode` reads a number as the nearest double (serde_json's
      `float_roundtrip`: the default parser read 10.5 % of the fractions k/W up to W = 2000 a
      unit in the last place off, which moved a window region's edge by a pixel); predicate
      sources up to 8192 bytes, above the longest canonical form (6916), so what `predicate`
      returns is always accepted again; parentheses at most 32 deep, because the parser
      recursed once per `(` on the event loop's 1 MB stack (508 of them, in 1024 bytes, needed
      2.1 MB in a debug build: a crash, not an error); a number with a leading zero is
      refused, since C and JavaScript can read `010` as octal; hex errors count characters,
      not bytes.
- [ ] **Live self-test on Windows, needing nobody to set up a screen:** `cells` of a fraction
      region of the manager window, taken twice, gives equal hex; `matchCells` against that hex
      and an all-zero state gives index 1 with similarity 1.0; `matchCellsAsync` gives the
      same; a 5x20 px region with a 10x36 grid returns 720 hex digits.
- [ ] **A `matchCellsAsync` held over a disable, live:** disable a module in the manager while
      its cells read waits, then enable it: the callback gets `nil, "the module was disabled
      while the read waited"` without a capture, and a poll gated on it asks again.
      `image_search::resend` is unit-tested; the path through `apply_enabled`, the worker and
      the drain has not run, and a headless run cannot disable a module.
- [ ] **End to end with the reader's author:** his reader and `cells` log hex for the same
      static menu at the same time, through the same source (`[screen] capture =
      "duplication"`, which his pack prefers). The golden test proves the reduction only; this
      is the only proof that the two capture paths hand over the same pixels.
- [ ] **Standard path against duplication:** cells of one static window through both sources.
      Unmeasured, which is why `screen.md` tells authors to record through the source they poll
      with.
- [ ] **Optional: states recorded at 100 % against 150 % scaling**, to put a number on what the
      docs say about games that are not DPI-aware.
- [ ] **macOS, never run on a Mac:** the unit tests run in the macOS CI job's `cargo test`, but
      no Mac capture has been reduced. Add to `tools/capture-probe`: the stability of the cells
      of a static window; a Retina display against 1x; whether drawing into the DeviceRGB
      context changes a known sRGB patch (it would move every cell near a colour test's edges).
      The client rectangle a window region resolves against is derived there, not read, and has
      not been checked against a game window either.
- [ ] **The other calls should take the window Region form too**, through `region_lua::read`
      and `Region::resolve` (`host.ocr.read` does since the merge with the OCR step):
      `host.screen.profile`, `imageSearch`, `imageSearchAsync`, `imageSearchEach` and its
      entries' `within`, `imageSearchAll`, `imageSearchMulti`, `save`, `saveMarked`,
      `template{ capture = ... }`, `host.ocr.recognize` and each `recognizeMany` region, and
      `pixel` as a point form (`{ window, fraction = { x, y } }`). They read corners loosely
      today; adopting the form means deciding per call whether their corners become strict too,
      which changes what existing modules get.
- [ ] **Snapshots:** a `snapshot` key for the cells calls once snapshots exist. A region not
      wholly inside the snapshot must answer `nil, "the region is not inside the snapshot"`
      rather than be clipped, which would change every block.
- [ ] **An in-repo benchmark of `cells::reduce`.** The 8 ns a pixel in `screen.md` was measured
      with a scratch crate that borrowed `cells.rs` (release build, i7-8700K); an `#[ignore]`
      test would keep the number true when the code changes.
- [ ] **Share the predicate with templates** when a template colour test is built:
      `cells::Predicate` is that test, and a second parser would be a second set of rules.

## Text recognition off the event loop (2026-09-22)

`host.ocr.read` is built: a `screen-capture` thread photographs at the call, an `ocr-recognise`
thread recognises, the answer is delivered on the loop (ocr/service.rs, ocr/lua.rs,
docs/api/ocr.md). Languages go through `fluent-langneg` on both platforms, for `read`,
`languages`, `resolveLanguage` and the `lang` of the two older calls. The Windows neural
recogniser still starts beside `Windows.Media.Ocr` for every small region, exactly as before.
What only a person, a Mac or a measurement can settle:

- [ ] **Migrate the Melodyne selection watcher** to `host.ocr.read` with `key = "selection"`,
      re-checking `ov.active` and `nativeMenuOpen` inside the callback. Needs an NVDA test: the
      same announcements, and the pump overrun lines gone.
- [ ] **Migrate the overlay runtime's `speakControl`**: the name spoken at once, the OCR value
      appended when it arrives, and the callback returning on `newer` (the focus moved) or on a
      changed pinned window. Only an `ocrLabel` control waits for its read before speaking. The
      input barrier is what keeps read-then-click right for the `opensMenu` buttons (sforzando,
      u-he, Soundiron, Impact Soundworks) and the Komplete Kontrol OCR edit field; an NVDA test
      with each of them is the gate.
- [ ] **Kontakt's file-menu read** moves to `read` with the other two, but keeps building its own
      rows (`ocrRows`) until `read`'s rows are compared with it on both platforms.
- [ ] **The language list at load:** in the first headless run the list was not known within the
      50 ms `languages()` waits, so a module asking in its top-level code got `{}` (it was known a
      moment later). Measure how long the recognise thread's first `AvailableRecognizerLanguages`
      and `GlobalizationPreferences.Languages` take; if it is routinely over 50 ms, publish the
      last session's list at start and replace it when the fresh one arrives.
- [ ] **O1** — WinRT engine creation against `RecognizeAsync` (decides whether the recogniser
      should keep one engine per language).
- [ ] **O3 / O16** — how long reads wait: before the capture, the capture, before the recogniser,
      per lane, p50 and p95, with Melodyne polling and while tabbing through an overlay; and the
      input barrier's waits. The log's "ocr: N region(s) … waited" line covers jobs over 100 ms.
- [ ] **O4** — a synchronous `recognize` inline against the same read queued and waited for.
- [ ] **O5** — confirmation only: Windows with the resolver hands `"de-DE"` for `"de"`; whether
      `TryCreateFromLanguage("de")` alone would have worked is now moot.
- [ ] **O6 (Mac)** — Vision's accurate and fast language lists on macOS 12, 14, 15 and 26, and what
      Vision does with a tag it does not list (the resolver never hands it one, but `recognize`
      does while the list is not known yet).
- [ ] **O7 (Mac)** — what one Vision pass costs on the recognise thread, against the pump thread.
- [ ] **O8 (Mac)** — the spelling `NSLocale.preferredLanguages` gives ("de-DE", "de", "de-Latn-DE")
      and that it resolves; and that asking for it off the main thread is fine.
- [ ] **O9** — the line separator in `OcrResult.Text()` (what `recognize` returns as `text`).
- [ ] **O10** — whether the GitHub Windows runner has an OCR language, which decides whether a CI
      probe can assert a `read`.
- [ ] **O11 / O12 / O17** — `OcrEngine.MaxImageDimension`; what `Cancel()` does after a timeout;
      how long a whole-screen read takes on the reference machine. Together they decide a 5 s
      guard on `RecognizeAsync` (`SetCompleted` once, never `get()`), which is not built: the
      recognise thread waits on `get()` as `recognize` does, and a region not answered for 5 s
      only makes new reads fail at once until one is (the guard counts from the last region
      answered, not from the start of the job, since the review of 2026-09-22).
- [ ] **O13** — the event loop's COM apartment. `ensure_winrt` logs once when a thread keeps an
      apartment it already had; a GUI session's log answers it.
- [ ] **O14 (Mac)** — how often the ladder's last rungs read something past 250 ms, now that they
      no longer hold the tap thread.
- [ ] **O15** — how often WinRT and the neural recogniser disagree on small regions both read.
- [ ] **O18** — whether the two threads are throttled when the application is in the background:
      Windows power throttling (EcoQoS) on battery, macOS App Nap; and on macOS whether the
      user-initiated quality of service the recognise thread asks for changes anything.
- [ ] **O19** — the cost of deciding the small-text path by the content crop instead of the
      region's size (up to 1 MP), and whether it changes any read in the repo's modules.
- [ ] **Mac, first run:** a `read` from a headless probe — the capture from the `screen-capture`
      thread (ScreenCaptureKit's answer while the pump is not the thread waiting), rows from
      Vision's observations, `languages()` and `resolveLanguage("de")`.
- [ ] **Not built yet from the design:** the capture-probe `read` line and its CI assertion
      (informational on Windows first); a log nudge for a module polling `recognize` from a
      timer; the await form (`host.task`); snapshots on the `screen-capture` thread; `expect`.
- [x] **At the merge with the cells step: one strict region reader.**
      `crates/host/src/region_lua.rs` is the one reader, used by the cells calls and by `read`;
      the rule for corners is `region::corners` in the pure `region.rs`, which the macOS check
      borrows. One rule, the stricter of the two on each point: corners are whole numbers (a
      fraction of a pixel raises instead of being cut toward zero), `{}` raises instead of
      meaning the whole display, and empty or turned-around corners raise instead of being
      answered `"failed"`. What a window does at run time is answered, not raised: a window
      region whose client area is empty is `nil, reason` from the cells calls and a `"failed"`
      reading with that reason from `read`, while the call's other regions are read. `read`
      takes `{ window, fraction }` as a bare region, as an entry's `region` and in a list,
      resolved at the call. Written once under `docs/api/screen.md#region-form`; `ocr.md`
      points there.
  - Review fixes, same day: the pixel limit follows the same rule — corners past it raise,
      a window region past it is answered (`nil, reason` from the cells calls, a `"failed"`
      reading from `read`, counted after the corners and in order), since its size is the
      window's; a `read` whose every region is unresolved is answered without the queue (no
      ticket, picture or language), superseding its key's waiting reads all the same; a
      `matchCellsAsync` held over a disable is answered `nil, reason` on enable instead of
      reading a rectangle worked out minutes earlier; a three-letter `lang` (`"eng"`) is a
      well-formed tag and is answered unavailable, not raised, as the docs now say.
- [ ] **`read` with a window region, live:** a minimised game's region answered `"failed"` with
      the client-area reason while the other regions of the same call are read, and a region
      that follows the window when it is resized — neither has run outside the unit tests.
- [ ] **Measure: do the neural recognitions pile up under `read`?** They still start beside
      `Windows.Media.Ocr` for every small region, on purpose. But `read` is no longer paced by the
      event loop: a 64-region call starts 64 of them at once, all queuing on the one session
      lock, and a region that really needs the fallback waits behind every one of them that will
      be thrown away. One debug headless run delivered a small region after about 9 s (not
      reproduced; 894 ms in the trace run). Measure the fallback's wait with many regions per
      call first. If it matters, a way that keeps the start parallel: hand the thread a flag, set
      when `Windows.Media.Ocr` answers or the blank guard fires, and have `paddle_ocr::recognize`
      return early when it is set — after the preprocessing, before it takes the session lock.
- [ ] **An exit that hung in a third-party audio DLL** (seen 2026-09-22, not OCR): one of five
      headless runs that exited on their own (a module that fails to load, so nothing is
      pending) never finished exiting. Our exit had completed; the last thread sat in
      `ExitProcess` → `LdrShutdownProcess` → `SS3DevProps.dll` (ASUS Sonic Suite 3, loaded
      through the audio stack once the OneCore voice opened) → `SleepEx`, in a loop, and
      `TerminateProcess` is refused for a process already exiting. The other four exited with
      code 0 in 1.5–4 s. The `read` build's own headless runs had all been stopped by PID, so
      they never reached this. Check whether the tray's Quit meets the same on this machine, and
      whether headless should open a voice at all when nothing is spoken.
- [ ] **The older calls' region arithmetic can overflow** (found beside the `read` review, older
      than it): `read_region` and `recognizeMany` in `lib.rs` compute `(x2 - x1).max(0)` in
      `i32`, so one region from x = -2e9 to 2e9 panics in a debug build (on the event loop) and
      becomes an empty region in a release one; the macOS `recognize_regions` adds and subtracts
      the edges of several regions in `i32` the same way. `read` works in 64 bits (`region.rs`
      `corners`, `Rect::union`); do the same there.

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
