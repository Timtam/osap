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
- [x] **OCR engine — dual-engine pipeline (embedded PaddleOCR fallback):** the weakness Sforzando exposed — `Windows.Media.Ocr` is fast (~5 ms warm) but **blind to isolated single glyphs** (reads "128" but not a lone "1") — is fixed by a second in-process engine. A **PaddleOCR recognition model via ONNX Runtime** (`ort`, recognition-only, English PP-OCR mobile) is **embedded in the binary** (`crates/host/models/`, `include_bytes!` — portable, no external file) as a **fallback**: for a small region it runs **concurrently** with WinRT (`backend::paddle_ocr`) and its read is used only when WinRT returns empty, so a lone digit costs ≈ max(winrt, paddle) ≈ 15 ms (not their sum) while the common multi-char case stays on WinRT. Shared **content-tight preprocessing** (`tighten`: crop to the glyph + upscale) was the real unlock — it fixed both engines. The model **preloads at startup** off the hot path (`backend::warmup_ocr`). Engine chosen by a measured spike (pure-Rust `ocrs` vs `ort`+PaddleOCR on real crops; PaddleOCR won on speed + single-digit accuracy). WinRT stays the primary + the only multi-word/detection path. **The macOS half of this plan was not built** (noted 2026-09-02; since 2026-10-02 the Mac package carries the recogniser and measures it beside Vision, see "macOS (2026-08-13, written blind)"): PaddleOCR was to be the cross-platform single-glyph specialist, but `ort` sits under `cfg(windows)` and `mod paddle_ocr` is `#[cfg(windows)]`, so Vision carries the small-text case alone there. That is deliberate and documented in `backend/macos/ocr.rs` — and Vision is given everything that helps: full backing-resolution capture, `.accurate`, language correction off, minimum text height zero, and a second untightened pass on the same "only when empty" rule. See [[ocr-engine-architecture]]. ✓ (2026-06-21, interactively confirmed)
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

- [x] **The neural recogniser in the Mac package, measured beside Vision** (2026-10-02, step a of
      the plan for faster reads on Intel Macs; nothing that is read or said changed). ONNX Runtime
      1.22.0, Microsoft's universal dylib, in `Contents/Frameworks`, opened at run time (`ort` with
      `load-dynamic`); the model in `Contents/Resources`; `licences/` and `ocr-pictures/` beside
      the `.app` (`package-macos.sh --onnxruntime`). tract was measured as the other engine and not
      taken: the same reads on all 55 inputs, but 7.6 times ONNX Runtime's time, and its build
      script needs a C compiler for each Apple target, so `check-macos.ps1` would break. Every small
      read hands the recogniser its content crop as a **shadow** (until 2026-10-03, below), at
      utility, never waited for, and on the recognise thread one pass of the fast level over it; both
      are only compared with the answer and counted (`ocr/shadow.rs`, a line every 200th read and
      at exit, a trace line for each disagreement, `ocr-shadow-*` pictures with the debug switch).
      `ocr-bench`: 28 drawn pictures and 13 real Windows captures (`--pictures`), each with the
      answers that are right; eleven ways of reading a small region — six candidates, the rest
      for comparison — and a closing line naming the cheapest candidate that reads right; Vision beside the recogniser, sustained load, the recogniser's
      and the fast level's first passes, pauses, a second language; `--paddle` and
      `--paddle-probe`, on Windows too. CI fetches and checks the dylib, refuses an executable that
      links it, runs two short runs without it and with the wrong half on macos-15, probes it on
      macOS 14 and 26, and runs `--paddle` on Windows. Step b (warm-up, turns) waits for the Air's
      numbers below; step c (reading on Intel Macs with it) was built on 2026-10-03, below.
- [x] **The Mac reads with the neural recogniser, by Windows' rule** (2026-10-03, the maintainer's
      decisions of that day; built and type-checked for both Apple targets, nothing of it run on a
      Mac). Wherever the recogniser has loaded, a small read hands it the content crop before
      Vision's first pass, at the reading thread's quality of service, and its text answers where
      Vision's accurate ladder — the tight crop, the whole region, the enlarged crop within the
      budget, no fast rung — read nothing: no words, the crop as the box, approximate boxes in
      `read` (`ocr/merge.rs`, which a test holds to `windows.rs`'s `recognize_image`, unchanged). On
      an Intel Mac the fast level reads first and answers where the recogniser reads the same, with
      the recogniser's spacing (`+3ct` read `+3 ct`). Every wait for the recogniser ends within the
      ladder's 250 ms, the check's within its fast pass's own time. The fast rung stays only where
      the recogniser is not there. With the debug switch on every read's line says who answered
      (`answered by the fast level and Paddle agreeing`, `Vision`, `Paddle alone`, `nobody`) and the
      wait for the recogniser in Windows' words; the cost and slow-read lines say it always, and the
      counts line (every 200th read and at exit) counts each. `ocr-bench`: `prod` reads as the
      application on that Mac, `old` as before. After the review of the same day: a request Vision
      refuses is answered by nothing and the recogniser is not used, as Windows' read fails then;
      the check agrees only where the recogniser reads a space wherever the fast level does (`1 23`
      and `12 3` would otherwise have become `1 2 3`); a poll's read that made way for a key press's
      takes only an answer already there; the fast pass for the counts comes after the answer, so
      the budget no longer sets any time aside; the check has a log fragment of its own (`+ waited
      <ms>ms for paddle's check (which read the same|which read otherwise|which had nothing|which
      had not answered by then)`), so that `which answered` is Windows' wait alone; an Intel Mac
      warms the fast level up after the accurate one; and the log says once a thread at which class
      the recogniser runs its regions. What it rests on, CI run 37089023246: the check 11–27 ms
      where one accurate pass over a value field took 220–470 on the Intel runner; sforzando's 1x
      "0" read by the recogniser in 4–7 ms where the ladder read nothing (Intel) or "o" (macOS 26,
      the fast rung); and its inventions on the drawn level meter and speaker ("l.", "D", ")"),
      which Windows answers alike.
- [ ] **The session at the Intel Air** (the plan's a6), the recogniser answering now: the new zip,
      the measuring script unattended; then with "Save the images OCR was given" on, one round —
      sforzando's Instrument, Polyphony and Pitchbend (Pitchbend brought to 1 by a fixed key
      sequence that lands there from any start, so nobody has to confirm the value; the picture is
      checked afterwards), VPS Avenger's preset name and its header at the smallest zoom, Kontakt's
      header; no Melodyne, which does not run on a Mac — then the switch off, and the log, the bench
      file and the pictures sent. It has to show: **which way the reads were answered, and how
      often** — every read's line (`answered by …`, there while the switch is on) and the counts
      line at exit (`answered by the fast level and Paddle agreeing in …, by Vision in …, by Paddle
      alone in …, by nobody in …`); **every reading the recogniser gave alone**, each kept as an
      `ocr-shadow-paddle-alone-*` picture — a digit gained (Pitchbend at 1, a lone digit;
      sforzando's Tune and Trans "0" no module reads, so they stay the bench's real captures), or
      text invented on a real plug-in's meter, symbol or empty field, which none of the round's
      fields should give; how often it came too late — `paddle-late` pictures, and in the per-read
      lines `which had not answered by then` after `for paddle` at the end of the ladder and after
      `for paddle's check` at the Intel check; whether the check's answers are right on real 2x
      text, spacing included, and whether one unchanged field is answered now by the check and now
      by the ladder (its per-read lines, read again and again): where the two give a dash or a quote
      in different forms, a polling module would hear a change that is none; the class the log names
      for the event loop's regions (`ocr: the neural recogniser runs the regions the event loop asks
      for at …`); the fast level's warm-up line; the event loop's slow reads (the slow-read lines)
      against the last session's 143–786 ms, the first of the session among them; and **the quit**
      with the recogniser loaded and used: `ocr: released the neural recogniser's session and ONNX
      Runtime's environment …` among the log's last lines, and no "quit unexpectedly" (the item on
      aborting at exit below). It decides b1 and b2, and whether anything of the new reading
      changes.
- [x] **The ONNX Runtime archive's SHA-256, pinned** (2026-10-03). GitHub lists no digest for
      `onnxruntime-osx-universal2-1.22.0.tgz` (its release API gives `digest: null`); the macOS
      workflow's first fetch from Microsoft's release address computed
      `cfa6f6584d87555ed9f6e7e8a000d3947554d589efe3723b8bfa358cd263d03c`, and that is
      `sha256=` in `tools/onnxruntime-mac.txt`. An archive that differs is refused, and so is a pin
      file without a SHA-256, whose error names the archive's own for a new version's pin.
- [ ] **Every process that loaded ONNX Runtime aborted at exit on a Mac** (CI run 37081778332,
      2026-10-03): the library loaded and read on macOS 14, 15 and 26, arm64 and Intel, and then
      each such process died with "libc++abi: terminating due to uncaught exception of type
      std::__1::system_error: mutex lock failed: Invalid argument" (signal 6) — the bench's
      children and `--paddle-probe`; by the same teardown the application would at every quit
      once its warm-up had loaded the recogniser, with macOS's "quit unexpectedly" and a crash
      report. Cause: ONNX Runtime 1.22 keeps its environment in a static
      `unique_ptr` (`OrtEnv::p_instance_`, registered with `atexit` when the dylib is opened) and
      `ort` never releases it; `exit()` destroys the logging's function-local mutex
      (`DefaultLoggerMutex`, registered later, when the environment is made) first, then that
      `unique_ptr`, whose `~LoggingManager` locks the dead mutex. The same stack is in
      microsoft/onnxruntime#25038 (1.22.0, macOS 15.5); 1.23.0 keeps the environment in a plain
      pointer instead. `--paddle-probe` never starts the recogniser's thread and aborted all the
      same, so that thread is not the cause. **Fixed, not yet seen on a Mac:** every process
      releases the session, then the environment, before `exit()` (`paddle_ocr::release`, after
      everything that can be inside ONNX Runtime has been closed and waited for, at most half a
      second): the end of `run`, `ocr-bench`'s every process before `main` hands its code to
      `exit`, and `main`'s guard (`ReleaseAtExit`) on every other way out of `main`, a panic
      included; a process that cannot release ends through `_exit` with its status. An `atexit`
      backstop, registered right after the environment is made, ends an `exit()` that begins
      unreleased with `_exit(0)` and a line on standard error. CI now fails any step whose process
      read and then ended by a signal or with a status other than 0, says "died of signal N at exit
      after reading" (or "hung at exit after reading", for a child its two-minute limit ended)
      apart from "did not load", and fails on the backstop's line. **The next
      macOS run must show:** `newer-macos` (14, 26): `--paddle-probe` says `ok` and ends with 0;
      `ocr-bench` on macos-15, macos-26 and macos-15-intel: no `libc++abi` line, the probes `one
      pass went through` for `paddle-raw` and `paddle-crop`, `paddle | the neural recogniser in
      this process: the session was made …`, the `neural recogniser, …` first passes measured, and
      the run ending with 0 after `done` — the first end, on a Mac, of a process that made the
      environment itself and released it; the copies as before, ending with 0. **Not seen by CI,
      only on a tester's Mac:** the application's own quit with the recogniser loaded — the tray's
      Quit, File > Quit and Command-Q end the main loop and go through `run`'s end, which no CI
      step reaches (its application runs are ended by a signal, which runs no teardown): the log's
      last lines should hold `ocr: released the neural recogniser's session and ONNX Runtime's
      environment before the process ends`, and macOS should show no "quit unexpectedly" and write
      no crash report. The backstop's route, AppKit's own `terminate:`, is cancelled while the
      module window exists (next item), so it is not expected to be taken; its line in a tester's
      output would say otherwise.
- [ ] **On a Mac, the Dock's Quit and a logout, restart or shutdown do not end the application**
      (read in wxWidgets 3.3.2's sources, 2026-10-03; not seen on a Mac). The quit Apple Event and
      AppKit's `terminate:` both ask wxWidgets first (`OSXOnShouldTerminate`), which asks the first
      top-level window to close: the module window, whose close handler always refuses, since
      closing it only hides it (`gui.rs`). So nothing ends, and the window is hidden. A logout
      waits on this application; whether macOS then cancels the logout or ends the process is for
      a Mac to show. For a VoiceOver user that is a restart that stalls. The tray's Quit, File >
      Quit and Command-Q are not affected. wxdragon 0.9.16 has no binding for
      `wxEVT_QUERY_END_SESSION`. One way: `NSWorkspaceWillPowerOffNotification`, observed beside
      the other NSWorkspace notifications in `backend/macos/system.rs`, sets a flag that makes the
      window's close handler quit (`begin_quit`, `exit_main_loop`) instead of refusing. Windows
      asks every top-level window to close at a logout as well (`wxApp::OnQueryEndSession`,
      `src/msw/app.cpp`); whether it is held there too is not looked at here.
- [ ] **The neural recogniser around macOS 13.3.** Microsoft builds 1.22 for 13.3, and the
      application does not open the library on an older macOS at all (`RUNTIME_MACOS` in
      `backend/paddle_ocr.rs`; the log says "not available" with the version, and Vision reads
      alone), rather than trusting `dlopen` with `RTLD_NOW` to refuse it before any of its
      initialisers runs. Not seen: whether 13.3 to 13.x loads it — no runner is older than macOS
      14 — and whether `minos` is 13.3 in both slices, which the build job prints and warns about
      when it is not. CI shows the refusal path only for a missing dylib and for one without this
      Mac's half (macos-15).
- [ ] **Windows stays on `ort` 2.0.0-rc.10.** Cargo allows one package with
      `links = "onnxruntime"` in the whole project, across every target, so the Mac's `ort` is the
      same version as Windows'. The way on: both together to rc.13, the Mac with its `api-22`
      feature; rc.13 needs Rust 1.88.
- [ ] **The dylib on a tester's Mac: first start, translocation, quarantine.** Whether the bundled
      dylib loads in a download opened with its quarantine flag still set, and after "Open
      Anyway": it may stay in quarantine. Does loading then fail quietly — the log's "not
      available" line, Vision alone — or does Gatekeeper put up a dialog? An unexpected dialog
      would be bad for a VoiceOver user. The library is opened a few seconds after start, by
      `paddle-warm-up`, unprompted. The macos-15 leg answers one half of it: a copy whose library
      alone carries the quarantine flag runs `--paddle-probe` within two minutes or warns
      (`ocr-bench-macos-15-copies`); a whole download quarantined and opened by "Open Anyway" stays
      the tester's. And whether `strip -x` and the ad-hoc signature keep it loadable on a real Mac;
      CI's `--paddle-probe` answers that for its own runners only.
- [ ] **The recogniser on Retina pictures.** The crop rule in points (`Tighten::for_scale`) and
      `paddle_pre::MAX_ASPECT` = 12 are measured on drawn pictures and Windows-rendered lines only
      (all four margin and enlargement settings read alike; every cut of a line up to 14.6 times
      as wide as tall read right, the first misread at 14.9): confirm on the tester's real 2x
      captures and macOS-rendered text. And whether `ink_lines`, with the margin as the gap, tells
      a rule under a word from a second line on real captures: `field-empty`'s dotted rule touches
      its descenders and measures as one line.
- [ ] **The recogniser beside Vision on the Air's cores, and its session time and memory there.**
      The bench's threads section (Vision beside it at two priorities, four blocks of sustained
      load) and its `paddle-first` processes answer it; in sessions, the counts line (`ocr: the
      neural recogniser beside Vision (Paddle) …`) counts the Vision passes it ran beside, and the
      slow-read line names it. Its warm-up, on `paddle-warm-up` at utility, is let go at the moment
      the recognise thread's own warm-up pass begins (both wait for the first Vision warm-up), and
      that pass is on the 5-second hang clock: whether the recogniser's warm-up should wait for the
      recognise thread's too — a second opener, and a case for a session whose recognise thread
      never warms — is for the bench's `paddle-first:with-vision` row to say.
- [ ] **The fast level's first pass in a cold process, and the fast level and the recogniser after
      pauses**: on the CI's Macs from the next run, on the Air from its session.
- [ ] **Revision 2: not taken for the lone digit** (2026-10-03: the recogniser answers it, by
      Windows' rule); `rev2-tight` and `rev3>rev2` stay in the bench for comparison. Apple lists it
      as deprecated from macOS 15; macOS 26.6.2 still offers it. Its trial in a process of its own
      stays. macOS 12 has no revision 3, and the application reads with revision 2 there already.
- [ ] **The counts from real sessions** — who answered each read, the recogniser's readings beside
      the ladder's, and where a fast pass ran beside the ladder, the fast level against both — the
      Mac's counterpart of O15. An Intel Mac's agreed answers meet no accurate pass in a session,
      since none runs then: a value the fast level and the recogniser both read wrong alike shows
      only on the bench's pictures, on an Intel read whose recogniser answered after the check had
      stopped waiting or read the value spaced apart, and on Apple silicon's counted fast pass
      (`fast = Paddle but not the ladder`, `ocr-shadow-fast-paddle-wrong-*`); 300 agreed reads there
      without a wrong one would put the rate below 1 % with 95 % certainty. The `ocr-shadow-*`
      pictures tell a gained digit from an invention.
- [ ] **The recogniser alone after an empty Vision ladder answers, as on Windows** (decided
      2026-10-03), its inventions included: on the drawn pictures without text it read "l." and "l"
      on a level meter and "D" and ")" on a speaker on every CI Mac, and alike on Windows ("ll.",
      ".", "O", ")"); no minimum score separates that from a right lone digit (0.55, sforzando's
      TUNE); and it misreads a word cut at half its height ("Veleeity") where Vision misreads it
      too. Whether it invents on a real plug-in is for the Air's `ocr-shadow-paddle-alone-*`
      pictures; CI warns on the drawn pictures in every run (`pipeline | the application's reading
      (prod) invents text on: …`).
- [ ] **The fast against the accurate level on a Neural Engine** (the Mac mini): whether Apple
      silicon gets the Intel check too. It reads by Windows' rule over the accurate ladder now, and
      on its recognise thread makes the fast pass for the counts that answer this.
- [ ] **The Intel check's wait, and the event loop's longest read — the maintainer's yes needed.**
      The check waits for the recogniser no longer than the fast pass took (`merge::check_wait`): on
      CI's Intel runner the recogniser took 4–25 ms a field and the fast pass 11–27, so it should
      mostly be in time; the Air's per-read lines say how often it was not (`for paddle's check
      (which had not answered by then)`), and the counts line's `fast = Paddle = the ladder` and
      `fast = Paddle but not the ladder` count, on an Intel Mac, the checks that agreed too late or
      spaced the value apart. A read that fails the check climbs the accurate ladder after one fast
      pass and at most as long again; no wait for the recogniser goes past the ladder's 250 ms, and
      the fast rung is gone where the recogniser is asked. So the event loop's longest read can be
      up to two fast passes longer than before, where the check fails and the budget is spent by the
      ladder — about 22–54 ms warm on CI's Intel runner, against the 220–470 ms an agreed check
      saves — which breaks "never longer than today's worst case" by that much. More on top, none of
      it measured yet: a session's first check on a thread, now that an Intel Mac warms the fast
      level after the accurate one (the fast warm-up line says what that first pass cost; 54–75 ms
      cold in CI's processes of their own); the recogniser beside Vision's passes at the reader's
      own class rather than at utility; and `recognizeMany`, which pays it once a region. The
      check's time also counts against the budget, so a failed check leaves the enlarged rung less
      of it (on an Intel Mac the first accurate pass alone spends most of it). CI measures the
      failed check's cost in every run: `prod` against `old` on `lone-1@2x`, `sfz-tune@1x`,
      `sfz-trans@1x`, `field-empty@2x`, `sfz-polyphony@1x` and `mel-cents@1x`, where the fast level
      and the recogniser disagree. If the maintainer does not take it, one condition keeps the event
      loop at its old bound: no check on it (`!ladder.on_event_loop` in `Rungs::merged`), and
      `recognize` on an Intel Mac reads at the accurate ladder's speed again. Whether the tap
      notices is for the Air.
- [ ] **The enlarged rung still invents on macOS 26** (CI run 37089023246: text on the drawn level
      meter and speaker). The decision kept the accurate ladder as it was, and the recogniser
      answers only where the ladder read nothing, so an invention of that rung stands. To be looked
      at with the Air's and the Mac mini's pictures.
- [ ] **Large regions with the fast level**: nothing checks them, since the recogniser reads one
      line only.
- [ ] **The tester's real 2x captures, and how their answers were checked.** The real crops in the
      repository are Windows 1x only, their answers read off the picture by a model and held
      against the plug-in's value range and against what Windows.Media.Ocr and the recogniser read
      (`tools/ocr-fixtures/real.toml`); `k8-voices` is misread by both Windows recognisers, and
      today's ladder on a Mac may well misread it too (a warning in CI).
- [ ] **Licences in the Windows package**: ONNX Runtime linked in and the model embedded, without
      their notices (`package.ps1` ships the GPL text and prism's). A task of its own.
- [ ] **The bench's rounds**, set after the first CI run of this size so that the Intel Mac stays
      under 30 minutes (`Plan::speed_pictures`, `samples`); every section prints its time. The CI
      job's limit is 45 minutes until then. The pipeline's verdicts lean on the engine's control
      (`prod-b`), which runs in another section at another time: should the pipeline's verdicts
      look noisy, a control of its own (prod under another name, about a minute more) is the next
      step.
- [x] **`paddle_pre::fold`'s dashes** (decided 2026-10-03): every dash U+2010 to U+2015 and the
      minus U+2212 are one hyphen, the en dash U+2013 among them — the form Vision is likeliest to
      give for a minus. A value's minus comes out of either recogniser as any of them, so a Vision
      "–12" and a Paddle "-12" agree.
- [ ] **What the recogniser costs a session**: one recognition beside every small read, now at the
      reading thread's quality of service (which class, the log says once a thread; the event loop's
      has not been seen on a real Mac), on the Air's cores — is Vision clearly slower beside it (the
      slow-read line's `the neural recogniser beside it`; the bench's sustained blocks ask at the
      measuring thread's priority now)? — the Apple-silicon recognise thread's counted fast pass,
      made after the answer (5 to 12 ms on the arm64 runners), and whether the debug pictures'
      limits (8 a region, 64 regions and 32 MiB a list, two lists) suit a test round.
- [ ] **Name the neural recogniser's model exactly.** `crates/host/models/NOTICE.txt` says what is
      known: an English recognition model of PaddleOCR's PP-OCR mobile series, converted to ONNX
      with paddle2onnx from a PaddlePaddle 3 program (its graph's names say so). Which model, and
      from which release, was not recorded when it came in (2026-06-21); the notice should say.

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
- [x] **Coordinate scaling is in the runtime (2026-09-28).** Deferred until a second plug-in
      needed it, and VPS Avenger is that plug-in: it zooms from 50 to 200 %. `O:scale(fn, {
      about })` puts every authored coordinate of an overlay through origin + frame + k *
      (authored - about), rounded once; ON:EAR moved onto it with its points unchanged (held to
      its old arithmetic to the pixel in `crates/host/src/overlay_scale_tests.rs`). See "VPS
      Avenger, stage 1: the runtime's building blocks" below.

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

- [x] **Answered on pictures by the first `ocr-bench` CI run** (2026-10-02): Vision's first
      accurate pass reads nothing on a drawn lone digit on every CI Mac, at one of its two scales
      (2x on the Intel runner and macOS 15, 1x on macOS 26), and the ladder's pass over the whole
      region reads it — two passes, 919 ms on the Intel runner — where revision 2 reads it in
      one. So macOS has a cost there rather than a gap; the neural
      recogniser is in the Mac package now, measured beside Vision before anything uses it (see
      "macOS (2026-08-13, written blind)"). The canary below was Melodyne's note field, and
      Melodyne does not run on a Mac (`supported_os = ["windows"]`).
- [x] **The canary is Melodyne's note field.** It is readable on Windows only because the
      second engine fires, and `modules/melodyne/src/main.luau` documents at length what
      happened when a change made the primary non-empty and the fallback stopped firing. If
      Vision reads it, the question is closed and the plan's macOS half can be struck for
      good. If it does not, macOS needs the fallback after all — and that means `ort` on the
      Mac, which also means the universal build grows a per-architecture dependency it does
      not have today.
- [x] **Not answerable from here, and not answerable by the probe either** — answered by
      `ocr-bench`, which puts known lone digits in front of Vision without a screen: drawn ones,
      and real captures cut on Windows (`crates/host/bench-data/ocr/real`). OCR needs
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
plug-in overlay declares nothing and reads through GDI exactly as before. Committed in
dee4164.

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

**Measured with the game** (2026-09-25, by the external developer on his machine, build
c724ab7 — since the history rewrite of 2026-09-25 that commit is 6413849 — plus
`tools/capture-liveness-dxgi`, the DXGI variant of `tools/capture-liveness`; full screen
1280x1024, AMD Radeon integrated graphics): through desktop duplication the game read LIVE — 40 of 40 and 39 of 40
reads gave different pictures, where the standard GDI path had given 40 identical ones
("Frozen"). Each 1280x1024 read took 6-7 ms, the slowest 11 ms — a whole
`host.screen.profile` call over the client area (`axes = "columns"`), column reduction
included, against about 23 ms for the same call through GDI; how it splits between capture
and reduction was not measured; the Direct3D device took 32 ms to create, `DuplicateOutput`
0 ms, the first picture came 4-16 ms after opening; after 30 s without a read the duplication
was released and the next read reopened it without trouble. The hook key (Ctrl+Shift+F7)
arrived with the game in front. So desktop duplication is the working source for such games;
`docs/api/screen.md` and `docs/screen-frame-sharing-design.md` say so.

- [x] **Step 0 — is OUR GDI path frozen for the game at all?** Yes. Measured 2026-09-21 on
      the developer's machine with `tools/capture-liveness` (its fourth version: forty
      `host.screen.profile` reads of the window in front, a quarter of a second apart, through
      the standard GDI path): the game full screen at 1280x1024, 40 of 40 reads the same
      picture ("Frozen") while the game was moving, about 23 ms per read. His own finding had
      come from his C# reader's GDI capture; ours is a screen-DC BitBlt of the composed image
      and reads the game frozen as well. So duplication is the fix, not a speed feature, and
      `fallback = "none"` keeps its reason; M4 measured it live. The present mode
      (PresentMon) was not taken; it stays open as M4b.
- [ ] **M1** A read's cost inside the app: the pump round trip, small regions and full screen,
      with the observation line's duplication clause. (Test-build figures above.) In the
      application (2026-09-25, the developer's machine, integrated AMD Radeon, the
      fullscreen game running): a whole `profile` call over 1280x1024 through duplication
      (`axes = "columns"`, column reduction included) 6-7 ms, the slowest 11 ms; the
      capture's share was not measured — the observation line's duplication clause of such a
      run would show it. Small regions in the application are still to be taken.
- [ ] **M2** Device creation and `DuplicateOutput` inside the app, the memory the driver adds,
      and above all whether the first picture after opening arrives at once and is complete.
      Explain the 3.5-4.3 s device creations above — cold driver, the new executable, or a
      debug build — and whether the app sees them. In the application on the developer's
      machine (2026-09-25, M4, AMD Radeon integrated GPU): device 32 ms, `DuplicateOutput`
      0 ms, the first picture 4-16 ms after opening, with the game moving; no multi-second
      creation there. Completeness was not checked by eye. The memory the driver adds, the
      3.5-4.3 s cases and a static screen (M16) are still open.
- [ ] **M3** The same RGB bytes from both paths on Kontakt, Melodyne and the desktop, so
      existing templates would keep matching (temporarily add `[screen] capture =
      "duplication"` to their manifests; read the first-read comparison line).
- [x] **M4** With our implementation, the game is frozen through GDI (step 0) and **live
      through duplication** — measured 2026-09-25 on the developer's machine with build
      c724ab7 (now 6413849) and a duplication variant of `capture-liveness` (`[screen] capture =
      "duplication"`, `fallback = "none"`, so a read duplication does not answer counts as
      failed instead of going through GDI): in two runs 40 of 40 and 39 of 40 reads gave
      different pictures ("Live"), where GDI had given 40 identical. The timings — each read,
      the device, the first picture, the reopening after 30 s idle — are in the record above.
      Ctrl+Shift+F7, taken through the keyboard hook, arrived
      while the game was in front; whether the tool's registered Ctrl+Shift+F8 was pressed is
      not reported (a registered Ctrl+Shift+F11 arrived on 2026-09-21). Duplication is the
      working source for this game; the gate before handing the developer the build for his
      menu reader is passed.
- [ ] **M4b** The game's present mode (PresentMon) with and without a duplication held: does
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
      read) right? The chance run above saw 33 ms per read. With the game of M4 in front on
      an integrated AMD Radeon: 6-7 ms per 1280x1024 read (a whole `host.screen.profile` call
      over the client area), the slowest 11 ms, well inside the pump's 60 ms. Whether that
      game holds the GPU at 100 % is not known: its GPU load was not recorded.
- [ ] **M10** Several monitors, negative coordinates, mixed DPI: coordinates and pixels match
      GDI; a region across two outputs is stitched correctly (the code path is unit-tested,
      never run on two monitors).
- [ ] **M11** The cost of reopening after the 30 s idle release (the device is kept), and
      after the device's own release at 5 minutes without a read. Seen in the application
      (2026-09-25, M4): after 30 s without a read the duplication was released and the next
      read reopened it without trouble; what the reopening cost was not reported on its own.
      The device's own release at 5 minutes is still unseen.
- [ ] **M12** Whether duplication works at all on a GitHub Windows runner (Basic Display
      Adapter) — the CI live-test step prints it.
- [ ] **M13** With OBS display capture and his C# reader running, `DuplicateOutput` gets
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
- [x] `tools/capture-liveness-dxgi` — the DXGI variant of `tools/capture-liveness` that the
      2026-09-25 measurement was made with — is committed (2026-09-25). It goes to a tester
      on its own, because `package.ps1` ships only `modules/`.
- [ ] Later (design step 9): `Req::WaitChange` — a dirty-rectangle watch for short-lived help
      bubbles and for announcing a toggle when it actually repaints; Windows.Graphics.Capture
      per window if M14, or another game, shows duplication missing a game's frames (M4: this
      game reads live); rotated monitors; the
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
      pygame and a pad; the game-menu reader port is where it gets checked.
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
      right_paddle2, left_paddle1, left_paddle2); `age` with the App Nap activity held; and
      what the `beginActivityWithOptions` call costs the event loop in the `on` that brings
      the first `down`/`up`/`axis`/`chord` listener (`docs/api/gamepad.md` says only that it
      is made there).
- [ ] **Not built this round** (design steps 5, 7, 8): the Raw Input HID source for
      PlayStation, Nintendo and generic pads and its `gamepad_hid` switch in
      `appcfg::SWITCHES` — with the corrected precedent: SDL's RAWINPUT driver keeps only
      XInput-capable (`IG_`) devices, so background delivery of DS4/DualSense reports through
      `RIDEV_INPUTSINK` has no SDL backing and needs its own spike with a DS4 or DualSense,
      HID `ReadFile` (non-exclusive, never writing) being the fallback before SDL3. Then
      `O:onGamepad` in the overlay runtime (register on activation, release on deactivation).
      Later: an SDL `gamecontrollerdb.txt` import, an IOHIDManager source on macOS for pads
      GameController does not list, a capture triggered by DXGI's next frame after a press,
      and waking the manager window's pump at once on a press instead of at its next 15 ms
      tick — only if the probe's `age` lines call for it. (The macOS CI job now fails on
      "gamepad watcher failed" like the Windows probe step.) Chords, once on this list, are
      built — see the next item.
- [x] **Button combinations as triggers** (2026-09-25): `host.gamepad.on("chord", cb,
      { buttons = { … }, pad?, maxAge?, exact?, holdMs? })`, promised to the game-module
      port and decided with the hotkey block ("gamepad chords as triggers for game
      modules"); documented under "Button combinations" in `docs/api/gamepad.md`. The rule
      is pure code in `backend/gamepad/chord.rs` (borrowed into `crates/macos-check` with the
      rest of the backend), the listener side in `gamepad_api.rs`. It fires at a press of a
      member after which the whole set is held on one pad, judged by the held set every
      button event now carries (`PadEvent::held`) — so a button already down at connect, at
      un-park or before the listener existed counts. A member cannot go down again without
      going up, so that fires once per holding by itself; the one repeat left, two members
      found down in one poll, is stopped by the report number the events share
      (`PadEvent::report`). No "wait for the release" flag: a release the parked Windows
      source never reported would have left it set and the next completion silent. A
      broadcast like every other pad event, not the design's earliest-registration-wins
      claim: nothing is consumed, the `down`s of the same presses are delivered as well, and
      one module's combination must not silence another's. `exact` counts derived buttons;
      `holdMs` is fired by the tick (`fire_pad_tick`) against the hub's state then, in the
      interactive OCR lane like `on_gamepad`. Review round (2026-09-25): the tick leaves a
      hold alone while a release of its set is still in the hub's queue (`Hub::pad_now`), so
      a member let go and pressed again during a slow callback no longer fires the broken
      hold; a member let go after `holdMs` but before the tick holds the hold out and it
      fires on that tick instead of being dropped; disabling a module cancels its holds at
      once (`refresh_gamepad`), not on the next tick; two directions of one stick axis in a
      set raise, and so does `maxAge` below 1 (it would drop every press); `maxAge` and
      `holdMs` are documented as whole milliseconds capped at 2^32-1. The probe logs
      left_shoulder + right_shoulder.
      Unit-tested, and started headless on a padless machine (2026-09-25): a chord listener
      registers, every raise case raises with its message, `off` answers true then false,
      and a chord listener alone keeps the headless loop running. No pad has pressed one;
      the live checks are the next item.
- [ ] **Combinations with a real pad** (Windows first, then the next Mac session): the
      probe's `chord left_shoulder+right_shoulder` line comes once per holding, beside the
      two `down` lines and with a `time` equal to the second one's; pressed within one 4 ms
      poll it comes with the first `down` in name order; a resting stick or trigger never
      blocks an `exact` combination (stick drift past 0.5 would — check how often a real
      stick gets there); a `holdMs` combination arrives about `holdMs` + one tick after the
      completing press, not at all when a member is let go early, and once (after its `up`)
      when a member is let go just after `holdMs`; whether the probe's `age` lines ever show
      the 100 ms+ stall during a hold that the queue check guards against; a combination still
      fires after its module was disabled and enabled again with the set let go meanwhile
      (the pad thread parks while nobody listens); on macOS the same, and whether
      GameController reports two buttons pressed together in one change or two.
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
- [x] **Screen snapshots**: an explicit frame handle — one capture, then several `pixel` reads
      and searches against it — idea B of `docs/screen-frame-sharing-design.md`. Built
      2026-09-26: `host.screen.snapshot`, read by every screen call given `{ snapshot = s }`, and
      `host.screen.pixels` for several points from one capture — see "Screen snapshots, the
      synchronous core" below; the same day `host.screen.snapshotAsync` (off the event loop, at a
      set time, or as a change wait) and `host.ocr.read` on a snapshot — see "Snapshots taken
      off the event loop, and change waits". `host.screen.cells` (built 2026-09-22, "Grid cells and window
      regions" below) covers the block-statistics half.

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
- [x] **A `"<modifier> tap"` hotkey — and `F21`–`F24` on macOS — is reported as held by
      "another application".** `host.hotkey.register` returns an id because `key_spec` parses
      the spec; the platform refuses it later in `refresh_hotkeys`, `report_os_conflict`'s
      dialog blames another application, and the claim is retried on every change to the
      enabled set. Raise at `register` for a spec the platform can never hold.
      Since 2026-09-22 the same dialog also answers a macOS hotkey on a letter that no key of
      the current keyboard layout types: `register_on_key` (`backend/macos/hotkey.rs`) says so
      in its error, and the log has it, but the dialog blames another application. That one
      can become holdable after a layout switch, so it wants a reason of its own in the
      dialog (the backend's error classified, not matched as text) rather than a raise.
  - Fixed (2026-09-22, d2b132f): `host.hotkey.register` raises for a tap, for a key the
      platform has no code for (F21–F24 on macOS) and for a chord the system keeps, before
      anything is claimed (`backend::hotkey_claim_for`); and a letter no key of the current
      macOS layout types is parked instead of refused (`backend/macos/hotkey.rs`, `register`)
      and registered as soon as a layout that types it is selected, so neither reaches the
      dialog. The macOS half has not run on a Mac: see "Verify on a Mac (letters by layout)".
- [ ] **A capture whose hook or tap could not be installed stays registered.**
      `host.keys.capture` pushes its entry and refreshes the captured set before
      `watch_keys()`; when that raises (macOS without the Accessibility grant), the token is
      lost with the error, and the key is suppressed and dispatched once a later capture
      installs the tap. Register the entry only after `watch_keys()` succeeded.
- [x] **The keyboard hook is installed once and never checked.** Windows documents that a
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
      Fixed (2026-09-27), after a session whose captured keys stopped: installed again after
      every resume and unlock, and when a raw-input witness sees it miss three key-downs in a row
      — see "The keyboard hook over days of uptime (2026-09-27)".
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

Since 2026-09-27 only a macOS package whose executable CI reused carries `build-info.txt`;
see "The downloads (2026-09-27)" below.

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
- [ ] The macOS job's module load prints the same: the executable's own commit when the job
      built it, and the one `build-info.txt` beside the `.app` names when it reused one.
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
critique). Built and unit-tested, and exercised by headless runs of the application
(2026-09-22): `encode` and its errors, `cancel` (twice, with `nil`, from inside its own poll
and within the same tick), `initial` reporting the window really in front, a late
registration's report and the input epoch turning with it. Nothing of it has run with a
window of the application open, or in a shipped module.

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
- [x] **Re-install a keyboard hook Windows removed.** Built 2026-09-27 (end of this item).
      Until then the only signal was a
      `WM_HOTKEY` the hook should have seen, and with the hook on its own thread several
      harmless cases look the same: a hook running late, whose `WM_HOTKEY` now usually arrives
      first (the two come from two threads); an elevated window in front, which would need an
      integrity-level check of the foreground process to rule out; a hotkey granted between
      the hook's lookup and Windows' own check. A re-install also puts our hook in front of a
      screen reader's in the chain, mid-session. Worth building if a removal is ever seen in a
      log (captured keys dead, "arrived through RegisterHotKey" lines in front of ordinary
      windows).
      Built (2026-09-27), after a session ten hours old whose captured keys stopped: the witness is
      raw input compared with the hook's calls rather than `WM_HOTKEY`, so none of the three
      cases above reaches it, and the re-install's place ahead of a screen reader's hook is the
      order the application has when it starts after the screen reader, and is said in the log
      — see "The keyboard hook over days of uptime (2026-09-27)".
- [ ] **Live (Windows), with NVDA:**
  - Heard with NVDA before d2b132f was committed: overlay navigation, NVDA's own keys,
      Alt+letter hotkeys, the F5 and F6 keys and a second start. Which of the points below
      that covered in detail was not written down, so they stay open;
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
  - WinUAE in front: a registered hotkey fires. In the developer's game (a native remake,
      not WinUAE) both ways arrive while it is in front: a registered hotkey
      (Ctrl+Shift+F11, 2026-09-21) and a key taken through the keyboard hook — the key
      `tools/capture-liveness-dxgi` captures through it (`host.keys.capture`, Ctrl+Shift+F7,
      2026-09-25). A registered hotkey matched in the hook was not what that run measured.
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
`lib.rs`, the worker job in `image_search.rs`. Built and unit-tested, and exercised by headless
runs of the application (2026-09-22) against the desktop: the predicate's canonical form and
its errors, the same region's cells twice with equal hex, `matchCells` answering index 1 at
similarity 1.0, a 5x20 region giving 720 hex digits, a window region resolving, an empty
client area answered `nil, reason`, and `matchCellsAsync` answering. Nothing of it has run
with a window of the application open, or against a game.

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
- [x] **The other calls take the window Region form too** (2026-09-25): `profile`,
      `imageSearch`, `imageSearchAsync`, `imageSearchEach` and its entries' `within`,
      `imageSearchAll`, `imageSearchMulti`, `save`, `saveMarked`, `template{ capture = ... }`,
      `host.ocr.recognize` and each `recognizeMany` region read it through the one reader
      (`region_lua::read_loose` hands the window form to `region_lua::read`), and `pixel` takes
      a point `{ window, fraction = { x, y } }` (the region's start formula,
      `region::resolve_point`). Decided per call: their corners stay loose, exactly as every
      module in `modules/` and `tools/` passes them, but a table that is neither form (`{}`, a
      `{ x, y, w, h }` bounds table) and a region that is not a table now raise, naming the
      call and the argument, where they read the whole primary screen without a word. A
      minimised window is answered in each call's failure shape. Unit-tested (`region_lua`,
      `region`, `ocr_wiring_tests`, the template tests); the bindings' wiring is held by
      `every_region_goes_through_the_one_reader`.
- [x] **Failure reasons** (2026-09-25): a read that gets no picture says why — `pixel`,
      `profile`, `imageSearch` and `template` add a reason after their `nil`, `save` and
      `saveMarked` after `false`, `imageSearchMulti` as a third value, `imageSearchAll` after
      its empty list, and the async callbacks as a second argument; the cells calls carry the
      real cause instead of one fixed text. The words are `Fallback::describe`'s and the
      standard path's, listed in `screen.md` "Failure reasons" and held to it by
      `every_reason_a_module_is_told_is_listed` and `the_hosts_own_reasons_are_listed`. A
      successful read still returns exactly one value.
- [x] **`[screen]` unknown keys** (2026-09-25): a key the host does not know (`captur`) is
      carried by the manifest reader and logged in one line naming it and the module; the
      manifest still loads. A value of the wrong type fails the manifest, as before.
- [ ] **Live, not yet seen:** a declaring module under `fallback = "none"` hearing the reasons
      from `pixel`, `profile` and `imageSearchEach` (switch the setting off: every read must say
      `it is switched off in Application settings`); the `[screen]` unknown-key line in the
      log of a real module; `pixel`, `profile`, `save` and `imageSearchEach` with a window
      region of a real window, and a minimised one; that a `fallback = "none"` module's first
      synchronous read in an `onTrigger` callback (`{ initial = true }` included) answers
      `it is still opening` while the prewarm's opening runs, and its first `matchCellsAsync`
      or `imageSearchAsync` waits the opening out (`screen.md`, "Nothing waits for that
      opening"); macOS: none of the window forms of these calls has run on a Mac (the
      arithmetic is shared and unit-tested; the client rectangle there is derived, see the
      macOS item above).
- [x] **Snapshots:** a `snapshot` key for the cells calls once snapshots exist. A region not
      wholly inside the snapshot must answer `nil, "the region is not inside the snapshot"`
      rather than be clipped, which would change every block. Built 2026-09-26: `opts.snapshot`
      on `cells`, `matchCells` and `matchCellsAsync`, exactly so (`read_cells_opts` in lib.rs;
      the reduction reads the region where it lies in the snapshot, `cells::of_sub`).
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

- [x] **Migrate the Melodyne selection watcher** to `host.ocr.recognize` with a callback and
      `key = "selection"` (B4, 2026-10-05): one read out at a time (`host.ocr.pending`), its answer
      checked against the mark taken when it was asked (`O:stillHere`) and `nativeMenuOpen`, and the
      tool switch's hold against the time it was asked. Its NVDA check is the step's own ("NVDA
      check of step B4", below).
- [ ] **Migrate the overlay runtime's `speakControl`**: the name spoken at once, the OCR value
      appended when it arrives, and the callback returning on `newer` (the focus moved) or on a
      changed pinned window. Only an `ocrLabel` control waits for its read before speaking. The
      input barrier is what keeps read-then-click right for the `opensMenu` buttons (sforzando,
      u-he, Soundiron, Impact Soundworks) and the Komplete Kontrol OCR edit field; an NVDA test
      with each of them is the gate.
- [ ] **Kontakt's file-menu read** waits in its timer's handler since B1b+M, and keeps building its
      own rows (`ocrRows`) until the reading's rows are compared with it on both platforms.
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
      user-initiated quality of service the recognise thread asks for changes anything. (macOS:
      since 2026-09-27 a latency-critical activity against App Nap is held while keys are
      captured or a controller is listened to — `backend/macos/activity.rs`, see "macOS over
      days of uptime"; the question stands for the hours in which nothing is captured.)
- [ ] **O19** — the cost of deciding the small-text path by the content crop instead of the
      region's size (up to 1 MP), and whether it changes any read in the repo's modules.
- [ ] **Mac, first run:** a `read` from a headless probe — the capture from the `screen-capture`
      thread (ScreenCaptureKit's answer while the pump is not the thread waiting), rows from
      Vision's observations, `languages()` and `resolveLanguage("de")`.
- [ ] **Not built yet from the design:** the capture-probe `read` line and its CI assertion
      (informational on Windows first; the probe reads a snapshot with `read` since 2026-09-26,
      but not the live screen); `expect`. Snapshots on the `screen-capture` thread are built
      (2026-09-26, "Snapshots taken off the event loop, and change waits"); the log line for a
      `recognize` that held the event loop is built; the await form is to be a mailbox per module
      instead (not built yet), with no task API for modules (2026-10-04, "Text recognition off
      the event loop").
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
- [x] **The older calls' region arithmetic can overflow** (found beside the `read` review, older
      than it; fixed 2026-09-25): `read_region` and `recognizeMany` in `lib.rs` computed
      `(x2 - x1).max(0)` in `i32`, so one region from x = -2e9 to 2e9 panicked in a debug build
      (on the event loop) and became an empty region in a release one; the bounding box that
      `ocr_regions` (Windows, `backend/windows.rs`) and `recognize_regions` (macOS) read several
      regions with added and subtracted their edges in `i32` the same way, so two regions at
      x = -2e9 and x = 2e9 panicked too. Now in 64 bits: `region::loose_corners` for the corners
      (`read_region` is gone), keeping what the release build gave, and `region::bounding_box`
      for the box, which answers a box that does not fit the coordinate range by reading each
      region on its own. Unit-tested (`loose_corners_never_wrap`,
      `a_bounding_box_that_does_not_fit_is_none`); the macOS half is type-checked only.

## Speech, Luau sources and triggers, from the proof-of-concept audit (2026-09-25)

Found while checking the documentation against what an external developer needs for a shared
game runtime; built and unit-tested. The byte-order mark, the JSON duplicate-key rule and
`onTrigger` ran in a headless run of the application (2026-09-25); the speech changes have not
run with a screen reader.

- [x] **`host.speech.output(text, {})` queued instead of interrupting.** The binding read
      `t.get::<bool>("interrupt").unwrap_or(true)`, and mlua reads `nil` as `false`, so the
      default applied only when `opts` itself was left out: `{}` and `{ interrupt = nil }`
      queued, and a runtime passing `{ interrupt = pack.interrupt }` would lag behind fast
      navigation. Now `opt_bool` in `lib.rs`: absent or `nil` is `true`, `true`/`false` are
      themselves, anything else raises (`host.speech.output: interrupt is true or false, not a
      number`). `host.keys.check`'s `layout` is read the same strict way — `{ layout = "no" }`
      used to count as `true` without a word. Tests: `source_and_option_tests`
      (`speech_interrupts_unless_told_not_to`, `a_boolean_option_is_strict`). Every call in
      `modules/` and `tools/` passes a literal or a boolean expression, so none changes.
- [x] **A UTF-8 byte-order mark made a `.luau` file fail to load** (Luau: "Unicode character
      U+feff"); .NET's `Encoding.UTF8` and PowerShell 5's `Out-File -Encoding utf8` write one.
      `read_luau_source` in `lib.rs` drops one leading mark for a module's entry file, a code
      dependency's entry and every `host.include`; a second mark, or one further in, is left for
      Luau. `module.toml` (the TOML parser) and `host.json.decode` already skipped one; both
      are pinned by tests now (`a_manifest_with_a_byte_order_mark_loads` in module-manifest).
      `host.resource.read` still returns the file as it is, mark included — documented.
- [x] **`host.window.onTrigger(matcher, cb)` raised "attempt to index function value"**, and a
      callback that was not a function was stored and failed only when the window came
      forward. The prelude now takes the two-argument form, and raises at the caller's line for
      a callback that is not a function, a matcher or `opts` that is neither a table nor `nil`;
      `onFocus` checks its callback too. After review, `onTrigger` also raises for a matcher key
      whose wrong type changed its meaning: an `app`, `os` or platform block that is not a
      table, an `app` inside a platform block that is not a table, a `where` that is not a
      function — `{ app = "game.exe" }` used to fire for every window. `find`, `findAll` and
      `test` do not check (documented in window.md, Matchers). Tests:
      `initial_trigger_tests::the_options_can_be_left_out`, `a_bad_registration_raises_at_once`,
      `test_takes_a_misshapen_matcher_as_window_md_says`.
- [x] **JAWS refused a line with prism error 9 (INTERNAL) and OneCore spoke for about 90 s
      while JAWS kept running**, although `speech.md` promised the screen reader back "within
      three seconds of coming back", and the log said "Usually this means the screen reader was
      closed". What prism offers to tell the two cases apart is `IS_SUPPORTED_AT_RUNTIME`, which
      creates a backend and asks without initialising it (JAWS: its `JFWUI2` window and the
      class factory; NVDA: its RPC endpoint). Built (`speech/prism.rs`): a look that opens
      nothing asks every reader that; while one is running the searcher keeps looking every
      3 s for as long as it takes, and only when none is does it slow to 30 s after a minute
      (`next_look`, `Search`, pure and tested). The refusal line names the reader, prism's error
      in prism's own words (`prism error 9: Internal backend error`, from `prism_error_string`),
      which call it was (speech alone, or speech and braille in one `output`), that the line and
      the ones after it go to the plain voice, and the schedule; the stall line says the same.
      The searcher logs its first finding (`JAWS is running but would not open (…)` or `no
      screen reader is running`), each change, the pace dropping, and `back to …`. Also: the
      same searcher runs when no screen reader was running at start, so one started later is
      used without a restart; it stops at its next look once "Speak through the screen reader"
      is unticked (the design doc said it did, and it did not) and a new one goes out when the
      setting is on again (`searcher_due`, tested); lines a replaced worker still hands back are
      no longer dropped with its channel (`leftovers`). The look's cost is estimated (about
      twice the measured 24 ms sweep), not measured. `docs/api/speech.md` states the schedule.
- [x] **Review of the recovery (2026-09-25).** Four faults found and fixed in
      `speech/prism.rs` and `speech/mod.rs`:
  - A searcher that found a reader which then refused a line before the event loop had seen
    the path healthy (a line handed over in the same pass — the flapping error-9 state) left
    the path with the plain voice for the rest of the session: `searcher_due` took "a
    searcher was sent" for "it is still looking". It now counts one as looking only while
    it has no reader in hand; a searcher stores `healthy` before releasing `reader`, and
    `retry_if_due` reads `reader` (Acquire) first.
  - Ticking "Speak through the screen reader" off and on with nothing said in between
    looked only at the sleeping searcher's next look, up to 30 s later, although the page, the
    log and the design doc said "at once": only `Speech::say` watched the tick. `pump` now
    watches it on every pass. A replaced searcher asks before every look whether anybody is
    still listening, so it neither looks again nor logs a find.
  - A JAWS running and refusing at start-up was logged as "no screen reader is running", and
    on opening as "JAWS was started after the application". The start-up line now says
    `no screen reader would open`, the search's first look is always logged, and its find
    reads `JAWS is open now; speaking through it from now on`.
  - The strict `interrupt`/`layout` reads, the two byte-order-mark reads in `populate_vm`,
    the searcher's stop, the read order in `retry_if_due` and the tick in `pump` were not
    pinned where they are wired: `option_and_speech_wiring_tests` in `lib.rs`.
- [x] **Decided (2026-09-25): the 3-second pace is bounded.** Only a reader whose runtime
      check looks at the reader itself (NVDA, JAWS, ZoomText, PC-Talker, Sense Reader) keeps
      it, for five minutes from the look that first saw it running and refusing, then every
      10 s (about 0.5% of one core, estimated). ZDSR and Boy PC Reader, whose "running" is a
      process-list match that includes their background services, get the pace of a search
      that found none: 3 s for the first minute, then 30 s. `speech/prism.rs` (`next_look`,
      `CHECKED_BY_THE_READER`, `Search::news_since`) with tests; `speech.md` and
      `prism-speech-design.md` say the same. The elevated-JAWS case stays unverified.
- [ ] **Live, Windows with JAWS: the recovery.** Only a real JAWS can say whether a JAWS in that
      state counts as running (window and class factory present) — if it does, the log shows
      `JAWS is running but would not open (…)` and it is used within about 3 s of opening
      again; if not, the log shows `no screen reader is running` and the 30-second pace after a
      minute applies, which would be worth knowing — and which error opening it gave meanwhile.
      Also: whether `output`'s braille half (`BrailleString` through `RunFunction`) is what
      failed; JAWS answers INTERNAL for either half, and the log cannot tell them apart.
      Two more that only a machine with JAWS installed can show: the runtime check
      (`CoGetClassObject` for the JAWS API class, then `FindWindow(JFWUI2)`) now runs on every
      look that opens nothing, and again in each new prism context after a searcher is
      replaced; prism-sys's cross-context safety test has only run where that class is not
      registered, so this path, of the same kind as the earlier OneCore crash, has never run.
      And a JAWS that opens but refuses every line makes every line start a new searcher (about
      60 ms to open prism), writes `JAWS refused a line` and `back to JAWS` per line, and when
      only braille failed says each line twice; the search no longer ends stuck on it, but
      whether it happens is for this test.
- [ ] **Consider: speech and braille as two calls.** prism's NVDA, JAWS and ZDSR `output` is
      speak then braille (PC-Talker's too while a braille display is connected), and a failure
      of either demotes the whole screen-reader path — so a braille
      failure costs the user their screen reader's voice as well, and the line is said twice.
      Calling `speak` and then `prism_backend_braille` ourselves would keep the voice when only
      braille fails and let the log name the half. A design change to the one-strike rule;
      decide before building.
- [ ] **Live, Windows with NVDA: nothing changed for the ordinary path.** Start with NVDA
      running, quit and restart NVDA: `back to NVDA` within about 3 s, as before; start the
      application without a screen reader and start NVDA afterwards: `no screen reader would
      open`, then `no screen reader is running`, then `NVDA is open now` within the first
      minute's 3-second pace. Untick and tick "Speak through the screen reader" while it
      searches, with nothing said in between: the log says it stopped (when a look fell between
      the two), and ticking looks at once (`trying the screen reader again`).

## The next steps of the API work: snapshots, own input, listening keys (2026-09-22)

Steps 1 to 5 of the API work are built — the sections above, from "Desktop duplication" to
"Text recognition off the event loop". What the maintainer decided for after them (2026-09-21),
in this order, and what is not built:

- [x] **Step 6 — snapshots with change waits.** Built 2026-09-26. One captured frame, with the
      time it was taken, that `pixel`, the cells calls, the image searches and `host.ocr.read`
      can all read, so that a state detected and the text read after it come from one picture;
      `host.screen.pixels` for many points of one frame; a `snapshot` key on the cells calls
      ("Snapshots" under grid cells above); a snapshot taken a set time after an input, for a
      help bubble that is on screen only for a moment (a game-menu reader reads 30, 80, 150 and
      250 ms after a controller press, today with `host.timer.after` and one read each); and a
      wait until a region changes, which for a module that is not an overlay does not exist
      today (`O:watch` is the overlay's). Snapshots are taken on the one `screen-capture`
      thread `host.ocr.read` already uses (decided: one capture thread for OCR and snapshots,
      recognition on its own). Takes up "Screen snapshots" in the porting review, "Snapshots"
      under grid cells and the snapshot part of "Not built yet from the design" under text
      recognition; the dirty-rectangle `Req::WaitChange` under desktop duplication is the
      Windows way to a change wait through duplication; the change waits built poll instead, on
      every path, and the engine is unchanged (S7′ decides whether it is needed).
      **Both halves are built** (2026-09-26): the synchronous core ("Screen snapshots, the
      synchronous core" below) — the handle and its budget, `host.screen.pixels`, and the
      `snapshot` key on `pixel`, `profile`, the five image searches, `template{ capture }`, the
      cells calls, `save` and `saveMarked` — and the asynchronous half ("Snapshots taken off the
      event loop, and change waits" below) — `host.screen.snapshotAsync` on the `screen-capture`
      thread, as a lane beside the OCR captures with the input barrier over both, at once, at a
      set time or as a change wait, and `host.ocr.read` on a snapshot. What is left are the
      measurements and live checks listed in those two sections.
- [ ] **Step 7 — own input tagged, scan codes, hold, new key names** (`host.input.send`; item
      e1 of the small-API design as the critique revised it). Not built:
  - **Own input tagged.** Every `SendInput` of the host — keys and mouse — carries a tag in
      `dwExtraInfo`, and every event the host posts on macOS a value in
      `kCGEventSourceUserData`. The keyboard hook and the event tap let a tagged event through
      without capturing it, arming a tap or counting it as a screen reader's modifier, so a
      module never captures a key it sent itself, and a sent Insert or Numpad0 cannot make the
      hook believe a screen reader's modifier is held. The screen-reader test becomes: the
      modifier physically held (updated from untagged events only), or the key down and not
      held by us; the modifier mask drops a modifier only we hold.
  - **Scan codes:** `host.input.send(combo, { scan = true })` sends `KEYEVENTF_SCANCODE` with
      the scan code `MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC_EX)` gives for the foreground
      thread's layout, plus `KEYEVENTF_EXTENDEDKEY` when it carries the E0 prefix — for games
      that read DirectInput or Raw Input and ignore virtual keys. Accepted and ignored on macOS,
      where a posted key code is already a key position. The default path stays as it is, so
      Kontakt and Melodyne receive exactly what they receive today.
  - **Hold:** `host.input.send(combo, { hold = ms })`, 1 to 10000. The downs go out in one
      `SendInput` batch, the ups at their deadline from a release thread of their own (a heap
      and a condition variable), independent of the event loop. Reference counts per virtual
      key, so only the last holder sends the up; purge, disable, reload and exit release at
      once; a crash cannot release, which the docs must say. On macOS the key goes down now
      and up later, with the modifiers as flags on both events.
  - **New key names:** `Insert`/`Ins`; `Numpad0`–`Numpad9`, `NumpadMultiply`, `NumpadAdd`,
      `NumpadSubtract`, `NumpadDecimal`, `NumpadDivide`; punctuation by position under the W3C
      `KeyboardEvent.code` names — `Minus`, `Equal`, `BracketLeft`, `BracketRight`,
      `Backslash`, `Semicolon`, `Quote`, `Backquote`, `Comma`, `Period`, `Slash`,
      `IntlBackslash`. The internal id stays the US-layout virtual key. Windows: `capture`
      matches the twelve positional keys by `kb.scanCode` and the hook reports the internal
      id; `send` converts through `MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, foreground HKL)`
      and `post` through the target thread's layout; `host.hotkey.register` refuses the
      positional names. macOS accepts them all. No one-character aliases (`+` cannot be a key
      in a `+`-joined grammar); text goes through `host.input.text`.
  - **Tests:** name round trips and `every_key_the_shared_parser_accepts_is_mapped`; the
      scan-code table; a tagged event passes a capture; the screen-reader test ignores keys
      only we hold; reference counting; release on purge and disable; `register` refusing the
      positional names on Windows.
  - **Open questions, for a Mac and for the screen readers:** `IntlBackslash` and the ISO key
      swap; whether games that read `GCKeyboard` or IOHID see posted keys; how NVDA and JAWS
      treat an injected Insert or Numpad0; auto-repeat of a held key.
- [ ] **`keyDown`, `keyUp` and `releaseHeld`** (item e2): later, when a module needs to hold a
      key across calls. The same reference counts, release thread and releases as `hold`, with
      `maxHold` 10000 ms by default; named `releaseHeld`, not `releaseAll`, beside
      `host.keys.releaseAll`.
- [ ] **`host.keywatch` — a keyboard observer that only listens** (decided 2026-09-21: later;
      item d of the small-API design). Today `host.keys.capture` always takes the key away, and
      there is no listen-only mode for ordinary keys. A namespace shaped like
      `host.gamepad.on`/`off`, behind a capability of its own, `"keywatch"` — not folded into
      `keys`, so the install review can tell "swallows Tab for its own interface" from "hears
      the keys you press" — with a window or process scope required, named keys only,
      synthesised keys left out by default, and no key ever logged. Windows: Raw Input for the
      keyboard (usage page 0x01, usage 0x06, `RIDEV_INPUTSINK`) on a `keywatch` thread with a
      message-only window, registered only while an enabled module listens; a
      `WH_KEYBOARD_LL` hook that never swallows as the fallback. macOS: a listen-only event tap
      on a thread of its own; the window scope holds per process only there, and Secure Event
      Input hides password fields. Spike first: an elevated window in front (UIPI); whether a
      key a low-level hook swallows — ours or a screen reader's — still reaches Raw Input;
      `RIDEV_INPUTSINK` on a message-only window; `hDevice` and `ExtraInformation` of injected
      keys; on macOS the target pid, VoiceOver and the injected source.

## Screen snapshots, the synchronous core (2026-09-26)

The first half of step 6 is built: `host.screen.snapshot` takes one picture on the event loop, and
every screen call given `{ snapshot = s }` reads that picture instead of the screen — `pixel`, the
new `host.screen.pixels`, `profile`, the five image searches (the two asynchronous ones on the
image worker, without a capture), `template{ capture, snapshot }`, the three cells calls, `save`
and `saveMarked` (snapshot.rs, backend/frame.rs, docs/api/screen.md "Reading a snapshot"). A read
of a region cuts it to the snapshot; a point, a template and a grid of cells need theirs wholly
inside. Snapshots are charged against 128 MiB per module VM and 512 MiB in all. The live paths
of the older calls are the code they were, the profile loop and `imageSearchAll`'s scan moved
into `profile.rs` and `template::find_all_in` and held to copies of the old code by tests. The
asynchronous half is built too: "Snapshots taken off the event loop, and change waits" below.
What only a person, a Mac or a measurement can settle:

- [ ] **S1** — a `BitBlt` of a box gives exactly `GetPixel`'s colour at every point on screen:
      `pixels` reads through a capture, `pixel` through `GetPixel`. Written as the ignored test
      `snapshot_live_pixels_equal_getpixel` (windows.rs, on the test window `dxgi_live` paints;
      `cargo test -p host snapshot_live -- --ignored --nocapture`), not run yet. Also whether a
      `BitBlt` of a box that reaches off every monitor reads black there (`pixels` never asks
      for a point on no monitor; `pixel` and the docs say region reads are black there).
- [ ] **S2** — `pixels` of eight close points costs one screen touch (one `pixel`'s 16.7 ms on
      the standard path; the same ignored test prints both), and what points spread over the
      screen cost at 1080p and 4K: one capture per 2048x976 tile they reach
      (`frame::plan_points`), two on a 1080p primary screen and up to six at 4K. A headless run
      on this machine's 1080p screen is recorded under "Screen snapshots, the synchronous core"
      below; the per-capture times at 4K are not measured.
- [ ] **S4** — two snapshots of a still screen are byte-identical within one path, and a GDI
      snapshot against a duplication one of the same region: the basis of a change wait's
      `tolerance`. Written as the ignored test `snapshot_live_still_frames_are_identical`
      (windows.rs), not run yet.
- [x] **S9** — collector pacing: `snapshot::tests::unreleased_10hz_poll_stays_under_the_cap`
      takes 300 unreleased snapshots in one VM three ways and never raises: 2 MiB snapshots in a
      bare VM (heap 0.3 MB) and in one holding about 25 MB of live strings, and 8 MiB snapshots
      in the latter. At most two were held at once every time (4.0 and 16.0 MiB), so the
      collector steps alone keep a poll that forgets `release()` far under the budget. What a
      call costs apart from its (fake) capture — the reservation, the collector's step and the
      handle — release build, three runs, 2026-09-26: bare VM 0.11–0.21 ms on average, at most
      1.1 ms; 25 MB heap 1.9–2.3 ms on average with 2 MiB snapshots, 3.1–3.2 ms with 8 MiB ones,
      at most 6.5 ms. The debug build: 0.24 ms and 3.3–3.6 ms on average, at most 5.8 ms. The
      step is clamped to the heap's size, since Luau pays at most one heap's worth of debt per
      step call anyway (`gc_kbytes`). Not measured: a heap of many small tables, which the
      collector traverses rather than skips as it does strings, and a heap of 100 MB or more.
- [ ] **S10** — the PNG encode of `save` from a snapshot, against a live `save` of the region.
- [ ] **Mac, first run (S8, S11):** a `snapshot` and `pixels` from a headless probe on a Retina
      Mac — the backing image a snapshot keeps (`NativeImage`) and its `crop`, drawn into a new
      bitmap (never run); `via` reporting `screencapturekit` or `coregraphics`; what 30 held
      snapshots of a window cost in memory against the fivefold estimate; before macOS 15.2,
      whether the older functions' best-resolution grab (a snapshot) and their nominal one (every
      other read) give the same point values; `pixels` reading black for a point off the desktop
      and for one in the gap between two displays of different sizes (`capture::pixels` asks
      `display_at` for each point), and whether a capture `frame::plan_points` plans across such
      a gap comes back whole — a clipped one fails the whole call, where the same points read one
      `pixel` at a time would each be answered.
- [ ] **Decided while planning, for the maintainer to confirm:** the region of `snapshot` and
      `crop` is required and read strictly (no whole-screen default); a template cut from a
      snapshot needs its region wholly inside; `pixels` reads its points strictly (whole
      numbers, where `pixel(x, y)` cuts a fraction); macOS snapshots keep the backing image
      (up to five times the memory, for OCR on a snapshot as sharp as a live read); the pixels
      an asynchronous search keeps of a released snapshot are not charged; a search on a
      snapshot held while its module was disabled is made again on the same snapshot. Those two
      together mean a disabled module's held searches keep their snapshots' pixels, uncharged,
      for as long as it stays disabled: bounded in number (nothing new is queued meanwhile), not
      in time.
- [ ] **Snapshot reads in the observation line.** `profile`, `cells` and `matchCells` on a
      snapshot do their reduction on the event loop, and it is not timed: the observation line
      counts screen touches, and a read of a snapshot touches none. So a module that reduces a
      large snapshot often stalls the event loop without the log saying so. A field of its own
      for them (`snap_us`, say) would.
- [x] **capture-probe** has its snapshot lines (2026-09-26): a `snapshot`, `pixels` of three
      points beside `pixel` of the same points, from the screen and from the snapshot, a `save`
      of the snapshot, a plain `snapshotAsync`, a change wait on a still screen and a
      `host.ocr.read` on the snapshot, and the line `snapshot callbacks N of M answered` before
      "still alive after capturing", which the macOS jobs require to say N = M and the Windows
      job warns about. The two `snapshotAsync` requests are asked first, whatever the snapshot on
      the pump does, and the read of that snapshot only when it exists; M counts only the asks
      that did not raise (3, or 2 without the snapshot). A snapshot that returns nil on the
      pump, and a call that raised (`THREW`, checked before the count), are errors of their own
      on the Macs, so neither is ever reported as a callback that did not come (2026-09-26). No
      CI run has taken a snapshot on a Mac yet.
- [x] **Headless check of `pixels` after the tile plan** (debug build, 1920x1080, 2026-09-26):
      three close points took 18 ms (one capture), two far apart 33 ms (two 1x1 captures), the
      four corners and the centre 136 ms and a 32x32 grid of 1024 points over the whole screen
      216 ms (two captures each, one per row of tiles; a debug build, whose per-pixel work grows
      with the area, and release times are not measured). Every colour equalled a snapshot of
      the screen taken just before, and a point on no monitor read black beside one read as
      the snapshot reads it.

## Snapshots taken off the event loop, and change waits (2026-09-26)

The second half of step 6 is built: `host.screen.snapshotAsync` takes a snapshot on the
`screen-capture` thread `host.ocr.read` photographs on — at once, at a set time (`at`, on
`host.now()`'s clock), or as a change wait (`change`: round after round until at least
`minPixels` of the watched pixels differ from `from`, or from the wait's first picture, by more
than `tolerance`; optionally until they stand still for `settle`; at most 2 s) — and
`host.ocr.read { snapshot = s }` recognises a snapshot without a capture (ocr/change.rs,
ocr/snap_queue.rs, ocr/service.rs, snapshot.rs; docs/api/screen.md "host.screen.snapshotAsync",
docs/api/ocr.md "On a snapshot"). One capture thread for both, one priority rule, one input
barrier; the DXGI engine is unchanged and change waits poll on every path
(docs/screen-frame-sharing-design.md, section 6). The callback is `cb(snap, why, info)`, exactly
once, and dropped for a module disabled, reloaded or removed first. Built with unit tests of
the wait, the lane, the scheduler's two new calls, the service (a test of the capture
loop on a stepped clock among them) and the Luau side. What only a person, a Mac or a measurement
can settle:

- [ ] **S3** — change-wait rounds through the standard path: their pacing (a GDI capture costs a
      compositor frame, so rounds run back to back) and the CPU the `screen-capture` thread uses
      during a 2 s wait, on an idle desktop and with a game in front; and how late the thread
      wakes for a round or an `at` on Windows — a condition variable's timeout follows the system
      timer, 15.6 ms at the default resolution, and the docs say "up to one timer tick". Whether a
      background application's timer is coarsened further.
- [ ] **S5 (Mac)** — ScreenCaptureKit in a loop: whether two captures of a still screen are
      identical (the basis of `tolerance` there), what a capture costs per round and in CPU,
      whether every call returns a new composition, and whether the screen-recording indicator in
      the menu bar (and the title-bar icon of Apple forum thread 732962) flickers or stays lit
      during a wait — which a blind tester would never hear and everybody else would see. Decides
      `MIN_ROUND_MACOS` (33 ms, a guess) and whether macOS needs a default tolerance of its own.
- [ ] **S6** — with the external developer's game, through desktop duplication, which reads it
      live: from a D-pad press to the first changed picture (`e.time` against `snap.time`), how
      long a help bubble stays up, which `watch` region catches the cursor without the game's own
      animation, and which reads the bubble more reliably: timed pictures (30, 80, 150,
      250 ms), a change wait with `settle = 0` — which answers the first frame of a bubble that
      fades in, which the check for the bubble may then reject — or one with a `settle` of
      30–50 ms, which the docs' example uses. And whether two quick D-pad presses make the menu
      example say the item before the real one (the second press's wait compares with a `from`
      that predates the first press's move) with `settle = 50`, and whether its second picture,
      150 ms after the answer, corrects it.
- [ ] **S7′** — a change wait through desktop duplication: the cost of a polling round (a round
      that finds no newly composed frame compares the kept one again), and whether a game
      redraws its whole window every frame — which decides whether a dirty-rectangle wait
      (`capture_next`, the reserved `Req::WaitChange`) would help at all.
- [ ] **S12** — the game's own frame times (PresentMon) while a 2 s standard-path change wait
      captures back to back beside it.
- [ ] **S13** — whether `BitBlt` from two or three threads at once (the event loop, the image
      worker, `screen-capture`) overlaps or serialises, now that a change wait captures beside
      the other two.
- [ ] **Live test, written and not run:** `snapshot_live_change_wait_sees_repaint` (windows.rs;
      `cargo test -p host snapshot_live -- --ignored --nocapture`): a change wait over a test
      window sees a second window appear 100 ms later, `changed`, within 100–400 ms.
- [ ] **Mac, first run of the asynchronous half:** a `snapshotAsync` and a change wait from a
      headless probe — `capture::frame` called on the `screen-capture` thread (ScreenCaptureKit's
      answer while the pump is not the thread waiting), `poll` skipping the flat-picture watch,
      `via` reporting; `host.ocr.read` on a snapshot reading the kept backing image
      (`Shot::Shared` from the `NativeImage`, cut by `render`), a snapshot whose region reached
      off the desktop included; the rebase when ScreenCaptureKit and Core Graphics alternate;
      `capture::display_id_at` keeping the regions of two displays out of one capture (never
      run: on a Mac with a Retina and a non-Retina display, two requests whose regions lie one on
      each should be two captures, and a change wait on each should come back at its own
      display's scale); what a change wait holds — its newest picture with the backing image, the
      baseline and a still spell's first picture as point-sized copies — against the 7-fold
      charge. The capture probe's macOS jobs require every snapshot callback it asked for.
- [ ] **NVDA and JAWS with a real module:** a callback spoken promptly after a D-pad press
      (press, change, speech), and a click held behind a plain `snapshotAsync` by the input
      barrier (read, then act), under load — Melodyne's selection watcher polling beside it.
- [ ] **Decided while planning, for the maintainer to confirm:** the callback is `cb(snap, why, info)`, not `cb(snap, info)` with `info.error`; a newer
      request with the same key answers the older one `nil` and a reason even when its picture was
      already taken (no `newer` as `host.ocr.read` has); `at` is a `host.now()` time, not a delay;
      the input barrier covers plain requests and a change wait's first picture without `from` or
      `at`, not timed requests or waits with `from`; `host.ocr.read` on a snapshot cuts each region
      to the snapshot, and a region with nothing in it fails alone; `recognize` and
      `recognizeMany` raise for a `snapshot` key instead of ignoring it; `snapshotAsync`'s keys are
      their own (a text read with the same key never replaces one); per module the oldest request
      is ended, in the whole application the newest refused; the change wait lives in
      `ocr/change.rs`, beside the thread that drives it.
- [x] **Headless check of the asynchronous half** (debug build, 1920x1080, standard path,
      2026-09-26): a plain `snapshotAsync` of 300x100 was answered 24 ms after the call, its
      picture taken 7 ms after it; `at = host.now() + 150` was taken 170 ms after the call; a
      change wait of 250 ms on a still region ended at 253 ms with 14 pictures, one of 200 ms
      against a snapshot with 12, both unchanged; the capture probe's change wait of 300 ms took
      18 pictures, 17.2 ms a round — the standard path's compositor frame, back to back; a text
      read of a snapshot answered in 23–151 ms, a region cut to the snapshot read as the part
      inside it and one wholly outside failed with the reason; a key replaced the older request;
      of twenty plain requests and five change waits asked in one callback, eight requests and
      one wait were ended by the module's limits and the rest answered with a picture; 33
      callbacks for 33 requests; every mistake raised with its message; the probe's three
      snapshot callbacks came. Release times are not measured.
- [ ] **Decided while fixing the review of the asynchronous half (2026-09-26), for the
      maintainer to confirm:** a change that undoes itself before a `settle` spell ends — a
      flash, a cursor that moved off and back — is no change, and the wait looks on (`changed` is
      never said of a picture equal to the baseline); `snap.inputEpoch` is `host.inputEpoch()`
      once the picture was taken, not at the call, for `snapshotAsync` read on the
      `screen-capture` thread from a mirror the event loop updates before it acts; a standard
      round is ONE capture — the most urgent request's, shared by the due requests inside it or
      within a 2,000,000-pixel union of it, on one display — and the rest go next, so an urgent
      picture never waits behind other modules' large captures; a change wait keeps its baseline
      and a still spell's first picture as point-sized copies of the watched box and its newest
      picture cut to its region, and is charged 3 × `w * h * 4` on Windows and 7 × on macOS
      (was 3 × and 15 ×); a request ended by a key or a limit gives its bytes back at once; a
      newer keyed request made by a callback in the same delivery batch supersedes an older answer
      in it; a wait with a `from` that cannot be its baseline (not holding every watched region,
      or taken another way than the module reads) holds input like one without; a read of a
      snapshot counts the part its regions cover in the 256 MB picture budget; desktop
      duplication falling back plans the standard captures of a round as a standard round is.
- [x] **Review of the asynchronous half, fixed** (2026-09-26): the sixteen findings of the
      adversarial review — the points above, the docs of `inputEpoch`, of the effective size limits (a
      change wait at most 11,184,810 pixels on Windows and 4,793,490 points on macOS in an
      otherwise empty budget, a plain request 33,554,432 pixels or 6,710,886 points), of
      `info.frames` for a request ended early, of the exit and of 251 rounds; the bubble example
      checks the picture for the bubble; a panic anywhere in a round's own code answers every
      request of it; new tests for each, the ones the review found would not fail when their fix
      was reverted included.
- [ ] **Decided while fixing the final reviews of step 6 (2026-09-26), for the maintainer to
      confirm:** the budget for a window region is answered, not raised — `snapshot`, a
      snapshot's `crop` and `snapshotAsync` answer `nil` and `at the window's current size this
      module's snapshots would hold …` (or the application's sentence), as the size limit is
      answered for a window region, and log it for `snapshotAsync` once per module every 10 s;
      corners still raise. So a poll of a window maximised on a 4K display (a change wait, the
      kept `last` and a poll in flight come to about 158 MiB of full-window pictures) no longer
      puts an error dialog over the game from a controller callback. A `minPixels` larger than
      the pixels a change wait watches raises when the region and every `watch` region are
      corners, and is answered `at the window's current size the change wait watches … pixels,
      fewer than its minPixels …` when a window decides it — before, such a wait ran to its
      timeout every time, and a `watch` of one pixel with the default `minPixels = 4` was one.
      A request the application's limit refuses ends none of its module's requests: the limit
      is counted as it stands once the module's own oldest has ended, and when it refuses all
      the same, nothing is ended; a refused request gives its charge back at once. The docs'
      menu example waits with `settle = 50` and reads once more 150 ms after the answer, gated on
      the game being in front; the bubble example uses `settle = 40`.
- [x] **Final reviews of step 6, fixed** (2026-09-26): the unit tests that assumed Windows'
      least time between rounds (8 ms, where macOS has 33 ms) or its estimate factor (1, where
      macOS reserves five times the pixels) derive them now — `over_budget_…`,
      `process_cap_spans_vms`, `reservation_refunds_when_the_capture_fails`,
      `a_failed_capture_fails_plain_and_steps_waits` and
      `a_stream_of_interactive_rounds_lets_an_aged_ocr_capture_through`; the whole host test
      suite was also run on Windows with macOS's values of `min_round`, `first_choice_via` and
      `ESTIMATE_FACTOR` put in by hand, and passed. The docs: what `settle` does for a bubble,
      the burst of presses, the strictly read corners of `snapshotAsync` and its `watch`, what
      `crop` raises for, which `snapshotAsync` holds input, `host.epoch` turning over for text
      reads and snapshot answers, the priority a `snapshotAsync` inherits, macOS's rebase
      between ScreenCaptureKit and Core Graphics, a `from` kept whole when it is exactly the
      watched part, and that nothing but a newer key or a disable cancels a request.
- [ ] **Later, not built:** a dirty-rectangle change wait through desktop duplication
      (`capture_next`, `Req::WaitChange`) after S7′; change waits through an `SCStream` on macOS
      after S5; `info.first`, the first changed picture kept beside the settled one; `s:bytes()`,
      after the template work's R0; `host.screen.diff(a, b)`; a tile for `pixel` on Windows; the
      image worker's captures on `screen-capture`; a capture triggered on the input thread itself,
      at the press (after S6); a content cache for text recognition, keyed by a hash of the
      recogniser's input and `lang`; a call that cancels a module's pending `snapshotAsync` by
      its key without asking again (now only a newer request with the key, which is itself
      taken, or disabling the module ends one).

## Menus: each module names its tests, and no timer decides (2026-09-26)

The maintainer's decisions of 2026-09-26: the host's core API holds only general primitives, and
no timed option decides UI state. A menu counts as open while a test says so and closed when none
does; time may only set how often the tests run. How a plug-in's menus are seen is its module's
decision, hooked into one interface the overlay runtime provides.

- [x] **Removed, and why.** The central detection every `menus = true` overlay got, and every
      elapsed-time decision about whether a menu is open:
  - `MENU_HOLD_MS` (8 s) and the hold: a control with `opensMenu` counted as a menu for 8 s
      unless a detector confirmed it, with a 300 ms grace after a Return or Escape
      (`MENU_CLOSE_GRACE_MS`) and its confirm logic. It gave the keys to a menu that was not
      there for up to 8 s, and took them back from one still being read.
  - `MENU_SEEN_CAP_MS` (30 s): a menu the accessibility check had seen for 30 s stopped
      counting — half-applied, so from then on the keys flipped every 1.2 s (Kontakt's snapshot
      menu on Cinematic Studio Strings: 23 flips, 17 hotkeys registered again each time).
  - The hand-over at the press itself (`host.keys.menuOpen(true)` and the hotkeys given up
      before any check had looked), and the 600 ms restore for an overlay with no watch.
  - `menus = true`: it raises at bind now, naming the building blocks.
- [x] **What replaced it** — `modules/overlay-runtime`, the Menus section;
      `docs/api/overlay.md#o-menutests`. `menus = { test, … }` on a binding; a test is
      `function(o, answer)` that answers once per question, at once or from an async callback
      (any value but nil and false is "seen"), or `{ name, cheap, test, pressed, forget }`.
      Each test has a word: "open" from the answer that saw a menu until it has answered no
      twice in a row (one missed read is not a close); the menu is open while any test's word
      says so. A test whose answer is outstanding is not asked again, holds up none of the
      others, and its word stands meanwhile — no timeout. Cadence only: every tick for 8 s after
      an `opensMenu` press and while a menu is open, otherwise cheap tests every tick and each
      other test once 8 ticks have passed since it was last asked; while a cheap test sees the
      menu the others are not asked. While a menu is open over any overlay of a module, every
      one of its overlays in front gives up its hotkeys. Building blocks
      `O.menuTests.nativePopup`, `O.menuTests.newWindow` (the process and the window list taken
      at the press; ends when its windows have gone, at the overlay's own next key with no menu
      open, and when the overlay leaves the front), `O.menuTests.accessibility` and
      `O.menuTests.accessibilityAfterPress` (the same walk, only from a press until that press
      is done with), both for the transition. `[menu]` log lines: which test saw it, the
      keyboard focus then, the keys that went through, whether it was still seen two ticks
      after an Escape, a test that has not answered for 20 ticks. In a calibrating run an
      `opensMenu` press saves `<overlay>-<control>-menu-before.png`, `-menu-after-600.png`,
      `-menu-after-1500.png` (docs/api/overlay.md, O.calibrating). Scenarios against a scripted host in
      `crates/host/src/overlay_menu_tests.rs`: 46 since the "holding its place" and menu-shot
      items further down. Host bug fixed on the way: a captured Return or Escape was recorded
      twice (the Windows hook and the macOS tap noted it in two branches).
- [x] **Which module lists which tests** (from the logs and the modules' own records):
  - Kontakt in a DAW: nativePopup + accessibilityAfterPress (see the snapshot-bar item below).
      Standalone: nativePopup + accessibility (the accessibility check saw every Kontakt menu on
      Windows since Phase A, and Kontakt 8 standalone's File menu, opened with VoiceOver, on a
      Mac). Kontakt in Komplete Kontrol: nativePopup + accessibility + newWindow — the walk asks
      KK's tree there (the nested Kontakt is a leaf), for KK's own menu.
  - Komplete Kontrol in a DAW: nativePopup + accessibility + newWindow (no log says which sees
      KK's own menu). Standalone: nativePopup (its menu bar is the window's Win32 menu bar).
  - sforzando, all three bindings: nativePopup (Windows; ReaHotkey needs nothing but `#32768`)
      + newWindow (macOS, 2026-09-18: a level-101 window for each of the six lists opened).
  - u-he: nativePopup (the preset menu is a native menu, verified live). Melodyne: nativePopup.
  - ARC ON:EAR (main window, settings, choosers): nativePopup + newWindow. Not accessibility:
      on 2026-08-31 it ran every tick after each press and saw none of the menus.
- [ ] **Menus only the removed hold ever covered — they keep the overlay's keys now, until a
      test sees them:**
  - Kontakt 7 standalone on macOS, File and View menus (2026-09-18: "the hold ended 300 ms
      after Escape went through to a menu no detector ever saw — and the plugin's window list
      did not change"). Drawn inside the window: phase 2.
  - ARC ON:EAR's Device list on Windows (2026-08-31, three presses, each "no detector ever saw
      one" — before the window-list detector existed). A JUCE list is a window of its own, so
      newWindow should see it: a live check. (The Settings gear, declared `opensMenu`, opens a
      panel, not a menu; the hold gave the keys away for its length there, and no longer does.)
  - Kontakt nested in Komplete Kontrol: its own snapshot dropdown (a leaf to accessibility; no
      log of it exists). Probably hold-only; phase 2 covers it with the bare Kontakt's picture.
- [ ] **Phase 2 — picture tests for self-drawn menus: macOS only (decided 2026-09-27).** On
      Windows every module's menus are already seen by a real test, measured with the shots and
      logs of 2026-09-27: sforzando's three lists are native `#32768` menus (`nativePopup`, and
      `newWindow` sees them too), Komplete Kontrol's menu in a DAW is a popup window
      (`newWindow`), and Kontakt's Qt menus, the full snapshot menu of an instrument with
      snapshots and the empty one of an instrument without, are seen by
      `accessibilityAfterPress`. So no picture test on Windows. On macOS no test sees Kontakt's
      File and View menus (drawn inside the window): build a picture test for them from shots
      taken ON A MAC at the next session (Retina, points, macOS rendering differ from the
      Windows shots), and list them in that session's plan. sforzando on macOS: `newWindow`
      saw its lists in the 09-18 session; confirm there.
  - How to take the shots: tick "Calibration keys and pictures in overlays" under Installed →
      Overlay runtime → Settings… (no reload). Then press the control, wait two
      seconds without a key, and close the menu with Escape; the log's
      `[calibrate] … menu shot` lines name the three files in
      `modules/overlay-runtime/calibration/`. Kontakt 8 in REAPER, classic view: Tab to
      "Snapshot menu" and Return (not Alt+M: see the next item), on an instrument with
      snapshots. File menu: Alt+F (Kontakt file menu) in a standalone Kontakt — in a DAW no
      control leaves that menu open (Load, Save and Reset drive it themselves). View menu: Alt+V
      in a standalone Kontakt 7 (Kontakt 8 has no View menu). sforzando: Return on Instrument,
      Polyphony and Pitchbend range. LAST, Cinematic Studio Strings, which has no snapshots:
      "Snapshot menu" and Return once. If the overlay is silent afterwards (no hotkeys, Tab
      going to Kontakt), switch to another application and back (Alt+Tab, then Alt+Tab again).
- [ ] **Kontakt 7 in a DAW on a Mac, anchored by its header text — unverified (2026-09-27).**
      A tester's log of that day had Kontakt 7 in REAPER's FX chain publish no accessibility
      element at all (no FILE button, "0 focusable candidates" on three presses of Cmd+Shift+F6
      over 73 s), so the in-DAW overlay never came, while the same Mac's standalone Kontakt 7
      published its header half a minute after starting. `detect.onHostPanel` now reads a small
      region around where FILE belongs in the DAW's plug-in panel (daw-hosts' origin, see "Plug-in
      modules in every DAW entry" below) when no FILE button with LIBRARY beside it is published
      (`no FILE button of Kontakt 7's found by accessibility in '…' — reading FILE LIBRARY at …`),
      and anchors on FILE with LIBRARY to its right (`geometry.headerWords`) within 24 points of
      where Kontakt's geometry puts it — no title, no band of the window. A read that sees
      nothing is asked again, up to eight evaluations of a stay. At the next Mac session: Kontakt 7 in REAPER's
      FX chain, in a floating window and in Logic; the log's `[kontakt] Kontakt 7 in '…' (…): its
      header text FILE at x,y puts Kontakt's corner dx,dy from the DAW's plug-in origin, which is
      ox,oy from the content origin` line should give a corner within a few points of 0,0, and an
      origin of about 240,52 in the chain (where REAPER's own layout put the panel on that Mac:
      272,112 in a window whose content began at 32,60), 0,22 floating and 2,88 in Logic.
      Compare it with the button's corner when Kontakt does publish (`FILE at … puts Kontakt's
      corner`). When Kontakt's corner is more than 24 points from the DAW's origin the read
      finds nothing (`no FILE LIBRARY row where Kontakt 7's would be`), and then it is the
      origin in daw-hosts that is wrong. The first word read is FILE's centre, measured at
      (176,19) on the Windows shot against the authored {175,19}.
- [ ] **REAPER's floating plug-in windows on a Mac — origin 0,22 unverified (2026-09-27).**
      Cmd+Shift+F6 and every macOS host matcher took REAPER's FX chain only (`FX: …`); the same
      log had it pressed five times over a floating VPS Avenger, answering "could not find a
      plugin window". `daw-hosts` now matches both (`^%u[%u%d]*i?: `, the format first:
      `VST3i: …`), and a floating window's plug-in origin is 0,22 — from the probe's toolbar
      elements (content y 1–22) and the same plug-in's text in Logic, not from a picture. Check
      with sforzando floated out of its chain: the log line `attachEmbedded: [host-panel] panel
      of 'VST3i: sforzando …' id=… at 0,22 …, identify=true` and its read-outs landing; and
      Cmd+Shift+F6 over a floating window.
- [ ] **Logic Pro as a host — built from the probe, unverified (2026-09-27).** The same log has
      Cmd+Shift+F6 pressed three times over a Logic plug-in window, which no matcher knew.
      `daw-hosts` now has a `logic` entry, in `all`, which the embedded bindings and the focus
      key both search: bundle `com.apple.logic10`, subrole AXDialog, a non-blank title (Logic
      titles the window by the channel strip, "Inst 1", not by the plug-in). Its
      `logicPluginOrigin` puts the plug-in 2 in and 88 down from the window's FRAME — the bottom
      edge of the header's elements plus 4 when the window publishes them, counting only
      elements that begin above where the plug-in would (so a plug-in's own published elements
      are not taken for Logic's header), 88 when not — and logs "[daw-hosts] Logic plug-in window
      '…': the plug-in starts at …". Every plug-in module is recognised there without naming
      Logic (see "Plug-in modules in every DAW entry" below). At the next Mac session, with VPS
      Avenger, sforzando or Kontakt 7 in Logic:
  - Cmd+Shift+F6 from the Logic main window: the plug-in window comes forward and its title
      is said; the log's origin line should read 2,88 (header's bottom edge) while the host
      still reports content == frame for that window — with Kontakt 7 too, whose FILE button
      would have moved it about 30 points down had it been counted as header.
  - Whether the keyboard reaches the plug-in's view at all after that (Logic's key commands
      may keep it), and what the focus chain reads (one deep = the window, which the embedded
      gate counts as inside).
  - Which of Logic's other windows are titled AXDialogs too: the focus key takes the first it
      finds, and every embedded binding takes the DAW's panel in them and asks its `identify`
      once per stay there (sforzando: one wordmark read, off the event loop). With Logic's toolbar
      hidden (the window's toolbar button), where the plug-in starts.
  - Ableton Live and MainStage on a Mac: no entry yet — each is one entry in daw-hosts, with a
      measured origin, and nothing else.
- [ ] **Kontakt's classic-view hotkeys came late** (2026-09-26 log): after switching Kontakt 8 to
      classic view, the rack controls' hotkeys (Alt+M/P/N, Ctrl+P/N, Ctrl+Shift+P/N, Alt+E/8/9)
      were not registered for about a minute — no "is now held" line between the switch and the
      next re-sync. Cause not yet traced; the log shows only that nothing re-synced them.
- [ ] **Kontakt's snapshot bar and the accessibility walk.** On 2026-09-26 a Menu element
      appeared in an embedded Kontakt 8's tree when the snapshot dropdown was pressed on
      Cinematic Studio Strings and was still there 66 s later, across a switch to another
      application. With no cap, the plain accessibility test would count it as an open menu for
      as long as it exists, and the backstop would see it again after every return. So Kontakt
      in a DAW lists accessibilityAfterPress: the element is counted only from a press of the
      snapshot dropdown (or VIEW) until that press is done with. If it stays after the press,
      the overlay is out — its hotkeys given up, Tab and Return going to Kontakt — until the
      user switches to another application and back, which ends the press. Escape (never
      captured) closes a real menu and brings the keys back sooner. Disabling a module is not
      needed; done anyway, it has to be the module that owns the overlay in front — for
      Cinematic Studio Strings that is Cinematic Studio Series, not Kontakt, because the arbiter
      drops only the overlays of the disabled module itself. The live check decides what the
      element is — the `[menu]` line names the keyboard focus, and one after an Escape says
      whether it was still seen — and phase 2's picture test replaces the walk either way.
- [ ] **Live checks, Windows with NVDA:** u-he Alt+M preset menu (`[menu] … test 'nativePopup'
      sees it`, one gave up / took back pair); Komplete Kontrol standalone Alt+F/E/V/C/H, and
      Edit → Preferences: no `holds its place` line, and Tab moves in the Preferences overlay; KK
      in a DAW, Space or Alt+M on "Komplete Kontrol menu" — the log says `… came to the front
      after 'Komplete Kontrol menu' was pressed and a test sees a menu — the overlay holds its
      place` and `test 'newWindow' sees it`, no `[deactivate]`; arrows and Return walk KK's menu;
      after Escape, when the menu has closed, one `… its window 'Komplete Kontrol' (reaper.exe)
      still gets the keyboard and is not shown — bringing back 'FX: …' (reaper.exe), where
      'Komplete Kontrol menu' was pressed: accepted` (or `declined`, the likely answer: next
      item), then most likely `no longer holding its place — in front now: nothing` and a
      `[deactivate]` at once — Windows makes the change when REAPER's thread gets to it — and an
      `[activate]` that speaks the overlay's control once REAPER has made it with the keyboard in
      KK (made inside the call instead: `the plug-in has the keyboard again` and one `took
      back`); the first key after that (Tab) speaks the overlay's next control, with no second
      `bringing back` line (the same for Kontakt inside KK). No `bringing back` line: the
      `no test sees the menu any more; the keyboard goes to …, and the menu's window … is id=…
      of pid … — nothing is brought back` line says where the keyboard was when the menu closed,
      and that is the finding; the `in front now:` line names host.window.active()'s answer,
      which can be kept from earlier in the epoch, and says nothing about it. A submenu, or
      Escape twice: the same; if KK's submenus are windows of their own, the reading line names
      the submenu's window, which is not the menu's own and brings nothing back (widening that is
      the maintainer's call). An item of KK's menu that opens a dialog: the overlay holds its
      place while the dialog is up; after it closes, a `bringing back` line only if KK's hidden
      menu window gets the keyboard back, otherwise the reading line and the ordinary match (back
      once the keyboard is in KK);
      then keep only the tests the `[menu]` lines named for KK; Melodyne's
      menu bar, no flicker; Kontakt 8 in REAPER: Ctrl+L still loads, the snapshot menu on an
      instrument with snapshots takes arrows and Return (`test 'accessibilityAfterPress' sees
      it`), and on Cinematic Studio Strings the focus and Escape lines say whether keys reach
      Kontakt at all, and Alt+Tab away and back gives the overlay its keys again; sforzando's
      three lists (nativePopup expected); ON:EAR's Device list and the four settings lists
      (newWindow expected: `newWindow: a window appeared`).
- [ ] **The foreground lock will most likely decline the step's SetForegroundWindow** (the
      review of 2026-09-27; not measured). The step runs on a tick 150–300 ms after an Escape
      that went to REAPER, and Windows lets a process change the foreground only if it is the
      foreground process, was started by it, received the last input event, or no window is in
      the foreground — none of which is likely to hold for this host then (the Escape went to
      REAPER, and the hidden menu window is the foreground window). If the NVDA test logs
      `declined`, the maintainer decides between (a) `host.window.focus` doing what AutoHotkey's
      WinActivate does (AttachThreadInput with the foreground window's thread around
      SetForegroundWindow) — a host change; and (b) no refocus: while foreground() names the
      press's hidden menu window, the runtime treats that as the plug-in's context, its keys held
      and scoped to that window — a new runtime decision that needs no foreground rights.
- [ ] **Live checks, macOS:** sforzando's lists still seen by newWindow, standalone and in REAPER,
      and whether a list's window becomes the focused window (a `holds its place` line; no
      `bringing back` line is expected after it closes: foreground() reports a focused window as
      shown unless it is minimised, and the `the keyboard goes to …` line says what it answered)
      or leaves REAPER's in front — then the binding's own match decides, and in REAPER the
      DAW's plug-in panel's geometric gate takes the overlay out when VoiceOver's focus is on a
      list item outside sforzando's panel (a `[deactivate]` line; whether the keys still reach the
      list then is the question); Command+Tab to the Finder with
      a list up: the overlay holds no keys while newWindow still sees the list, and leaves (`no
      longer holding its place — in front now: nothing`) once it does not — does the list close
      when REAPER goes to the background?; Kontakt 8 standalone's File menu by accessibility;
      Kontakt 7 standalone's File and View menus now keep the overlay's keys until phase 2
      (expected) — so with one of them open, Return re-presses "Kontakt file menu" (or "View
      menu") under it instead of choosing an item, and only VoiceOver's own VO+Space chooses one.
- [ ] **host.window.foreground() on a Mac, never run.** Whether AXFocusedWindow can name a
      window that is ordered out (not on screen) — `shown` would still say true, since only
      AXMinimized is read and the window server is not asked; whether its `id` equals
      `active().id` for the same window (both intern the element, so it should); and what the two
      reads cost. The probe (Cmd+Shift+F9) logs a `foreground:` line beside the window it
      probes, saying whether it is that window; and every hold over a menu's window that ends
      with no test seeing the menu logs the runtime's `the keyboard goes to …` line.
- [ ] **A busy Mac application reads as "no menu" to the accessibility tests.** The host leaves
      an application that did not answer alone for 5 s (`BUSY_PENALTY`, `backend/macos/ax.rs`)
      and `find` answers false meanwhile, so a menu only the accessibility tests see (a
      standalone Kontakt 8's File menu) closes after two ticks and opens again afterwards.
      Documented under overlay.md's macOS notes; telling "could not ask" from "no menu" needs a
      host change — the maintainer's decision.
- [x] **Decided 2026-09-26: the host's 60 s rule is gone.** On macOS `native_menu_open` treated a
      menu depth that had not moved for 60 s as closed (`STUCK_MENU_MS`, `backend/macos/watch.rs`),
      and the event tap's own pass-through followed it — a timed decision about UI state. Removed:
      a menu counts as open from its `AXMenuOpened` until its `AXMenuClosed`, and the count is
      cleared when another application comes to the front. docs/api/keys.md
      (`host.keys.nativeMenuOpen`, macOS) and overlay.md's macOS block say so.
  - [ ] **Only a Mac can confirm:** that `AXMenuOpened` / `AXMenuClosed` arrive in balanced
      pairs in REAPER, Kontakt 8 and sforzando — open and close a menu bar menu, a submenu, and
      one closed with Escape, and look for a "the menu in pid … closed" line after each "a menu
      opened in pid …"; and what a lost close would cost: the tap lets captured keys through and
      the overlay's hotkeys stay given up until the user switches to another application and
      back, which clears the count (say so in the protocol, as the way out).
- [x] **Decided 2026-09-26: a test that saw a menu and then never answers keeps it open** until
      the overlay leaves the front: its word stands while its answer is outstanding, which is
      what keeps a slow picture test from flickering, and only a timeout could tell "slow" from
      "never" — the kind of timer the decisions rule out. The log names such a test after 20
      ticks. The building blocks always answer; a module's own async test must answer from every
      path of its callback.
- [x] **Decided 2026-09-26: `menus = true` raises at bind**, with a message naming the building
      blocks: a module still written with it fails to load rather than running with no menu
      tests.
- [x] **A menu that is a window in front: the overlay holds its place** (2026-09-26, from the
      maintainer's NVDA test of phase 1). Komplete Kontrol in REAPER: Space on "Komplete Kontrol
      menu" opened KK's menu as a popup window of reaper.exe titled "Komplete Kontrol"; it came
      to the front, so the overlay left the front (`[deactivate] … in front now: 'Komplete
      Kontrol' (reaper.exe)`) and its tests — which run only for an overlay in front — never saw
      the menu. When the menu closed, REAPER left the keyboard on its FX window, which the
      embedded chrome gate counts as REAPER's, so the overlay stayed out, hotkeys and all, until
      the user left and came back two minutes later. ReaHotkey follows "REAPER's FX window is
      active" there. Now (`modules/overlay-runtime`, the Menus section, "holding its place";
      docs/api/overlay.md#o-menutests): after a press of an `opensMenu` control, a window in
      front that belongs to the process that was in front at the press (not that window itself)
      is held while the overlay's tests see a menu — asked there and then, so newWindow sees the
      popup at once — and the overlay stays in front holding no keys at all (no navigation key,
      no hotkey, no say in the pass-through flag: a dialog the menu leads to may be another
      overlay's, and the host hands a captured key to the first module holding it), its tests
      running. When the hold ends, the binding's own match decides, as for any other window
      change (ReaHotkey's rule: REAPER's FX window active with the plug-in's control in it); with
      the keyboard in the plug-in its navigation keys come back at once and its hotkeys when the
      tests stop seeing the menu. Another application, or a window of the same one no test takes
      for a menu, ends it as before. No timer: the window in front, the press, the tests and the
      keyboard decide. Process, not
      owner: the window table has no owner, and a plug-in's popups are windows of its process on
      both platforms. 8 scenarios in `overlay_menu_tests.rs` bind the overlay for real
      (`attachEmbedded` against a scripted FX window, through the arbiter); 34 targeted mutants
      of the runtime, the phase-1 ones included, each fail at least one scenario.
  - [x] **Review fixes, the same day.** The hold needs a test to see a menu AT THAT MOMENT, not
      the word: the word says "open" for a miss after a menu has closed, so a dialog that an item
      of a native menu opened (KK standalone, Edit → Preferences) was held, and when the word
      closed two ticks later the leaving overlay unpinned the Preferences overlay's key scope.
      When the plug-in has the keyboard again after a hold, the tests forget the press, so a
      window the menu opened that stays up no longer keeps the keys passing through. Nothing in
      front holds only a menu the tests still see (a Command+Tab to an application with no window
      kept the overlay in front). The hotkeys come back with their `took back` line. The "before"
      menu shot is written after the other two, not straight after the click. (A claim on the
      window of the press, with a "back on its window" state and an empty-chain rule built on it,
      came with these fixes; removed on 2026-09-27, next item.)
  - [x] **Simplified, and one step beyond ReaHotkey** (2026-09-27, the maintainer's decisions
      after his NVDA test and a diagnostic run). KK's menu is an invisible full-screen Qt window
      of reaper.exe that KK most likely only HIDES after Escape (the likeliest reading of the
      log, not measured: no foreground event came for ~4 s): newWindow saw it gone and the menu
      closed, the claim's logic saw another window in front and let the overlay go, the user's
      first key (Tab) was lost, and the overlay came back ~4 s later with REAPER's FX window.
      (The log's `in front is id=…` on that tick was host.window.active()'s answer kept from an
      earlier epoch, not a reading: a tick does not turn the epoch over, and active() reports a
      hidden window as none. The focus chain, read fresh, was empty, which on Windows means the
      foreground window was hidden or absent.) ReaHotkey has no return handling: its plug-in
      context is REAPER's `#32770` FX window active with the plug-in control found after
      `reaperPluginHostWrapProc1` (`Lib/ReaHotkey.ahk`, GetPluginControl / ManageState).
      Removed: the claim ("home"), "back on its window", `attachEmbedded`'s
      `origin({ anyFocus = true })`, `keyboardAround`, `_menuHomeLeft`, `_menuBackOn`, the
      empty-chain rule, and their docs, log lines and scenarios. Kept: the hold (no keys at all,
      a test seeing the menu there and then); when it ends, the ordinary match decides. Added,
      first version: bring back the window of the press when the window held over was still in
      front with an empty focus chain. The review found it could only fire on a stale reading
      (see above), and that the scripted host was green only because it reported hidden windows
      as in front and kept nothing for the epoch; replaced the same day, next item.
  - [x] **host.window.foreground(), and the step after the hold on it** (2026-09-27, the
      maintainer's approval of the review's proposal). A general host primitive: the window that
      gets the keyboard now, `{ id, pid, shown }`, hidden or not, read afresh on every call and
      never from the epoch's store; `nil` when there is none or it cannot be read. Windows:
      GetForegroundWindow, its process, and shown = visible, not minimised, not cloaked (DWM), all
      local. macOS: the frontmost application's AXFocusedWindow (no fall-backs) and its
      AXMinimized, `nil` without asking while the application is in the not-answering quarantine
      or when a read times out. Stub: nil. Capability `window`.
      docs/api/window.md#host-window-foreground, with `active()`'s Windows block corrected (it was
      "always current"; it is the epoch's answer, and a hidden foreground window is nil) and
      `focus()`'s (`true` is Windows accepting the request, not the keyboard in it). The runtime's
      step now fires only when, as a hold ends because no test sees the menu, foreground() names
      the menu's own window — the one the hold started over after the press, of the press's
      process — with `shown = false`; the focus chain is not read. The window of the press must
      still be listed for its process with its class (Windows reuses handles), or nothing is
      brought back and the log says so. A dialog the menu led to never triggers it; the menu's own
      window getting the keyboard back after such a dialog does. The scripted host in
      overlay_menu_tests.rs now answers as the real one: a hidden window is never active();
      active() and focusChain() are kept per epoch, turned over by events, keys, timer.after,
      snapshot answers and focus() but not by the tick; an empty chain on Windows only for a
      hidden or absent foreground window; foreground() fresh; and a Mac mode. 50 scenarios, among
      them the KK case from the diagnostic run, every excluded case, the handle-reuse check and
      the Mac's unreadable state; 37 targeted mutants each fail at least one, and the first
      version's runtime fails 10 of the 50 scenarios.
  - [x] **Review fixes of that step, the same day.** When the step reaches its reading and
      brings nothing back, the reading is logged, once per press (`… the keyboard goes to id=…
      of pid …, shown` / `not shown` / `no window the platform names` / `a window that could not
      be read (…)`, `and the menu's window … is id=… of pid … — nothing is brought back`): the
      `in front now:` line names active()'s possibly kept answer, and three different states
      had given the same log. The scripted host completes an accepted host.window.focus later,
      with its foreground event (`T.land`), as Windows does for another thread's window, and at
      once only where a scenario says so; the KK case now asserts `[deactivate] … 'nothing'` and
      then the `[activate]` that speaks. A window of the press that is not listed is "not listed
      for its process with its class (gone, untitled, or its id now another window's)" — list()
      lists titled windows only — and a listing that raises is said with its reason. A
      pollMatch poll can bring the window back at its first miss, before the tick closes the
      menu (a scenario, and overlay.md). The trait's default `foreground_window` is gone, so a
      backend without one does not compile, and the Windows reading is `foreground_of(hwnd)`,
      tested read-only on this session's hidden top-level windows and on handles that are no
      window. window.md: what foreground() names and where keys do not go to it (hotkeys and
      hooks, a UWP app's frame, macOS panels that do not activate their application), the
      macOS run-loop freshness and the quarantine a timed-out read sets; active()'s "hidden"
      per platform, a minimised Mac window read as nil; focus()'s `true` is the request
      accepted, the change made later on the window's own thread. "KK only hides its menu's
      window" is hedged everywhere as the likeliest reading.
- [x] **The menu shots no longer delay the click** (2026-09-26: ~700 ms per shot in a debug
      build; Diva's preset menu felt slow). The press takes one snapshot of the origin's own
      rectangle (`bounds`) and nothing else — no PNG, no control's point or `when` — and the
      control acts; its PNG is written once the later two are in, not straight after the click,
      when the menu's first answer and the user's first key arrive. The later two are
      `snapshotAsync` with `at`, captured off the event loop and written when they arrive. The pictures now cover the
      origin whole (for a window, its frame too) rather than the content rectangle grown by the
      controls' points, and the log line says where the content begins. docs/api/overlay.md, O.calibrating.
- [ ] **Accessibility context checks block the event loop while Kontakt loads** (2026-09-26,
      NVDA test of phase 1). After Kontakt's Load dialog the overlay came back only after ~4 s:
      `uia.findAny(Komplete Kontrol) blocked the pump for 2075 ms`, `uia.findAny(Kontakt 8/Kontakt
      7) blocked the pump for 1006 ms`, `[recheck] 'Kontakt 8' 3083 ms (context 3083, gate 0)`,
      `[pump] one iteration took 4372 ms (… window-activate 3097 + focus-change 1231 …)`.
      Kontakt's identity checks (`isKK`, `variantIndex`, relational, not cached) ask UI
      Automation on the event loop while Kontakt is busy loading. Moving those checks off the
      event loop is its own task; not done here.
- [ ] **Kontakt still writes the flag itself:** `openFileMenu` calls `host.keys.menuOpen(true)`
      after opening the File menu by name, and the runtime overwrites it on its next tick
      (≤150 ms) with its tests' answer. Unchanged behaviour, but a claim with no test behind it.
      Likewise `verifyMenuOpened` says "Snapshot menu did not open" from one accessibility check
      350 ms after the click: an announcement, not menu state, but a fixed delay deciding what
      is said; phase 2's picture test is the better witness for it too.

## The downloads (2026-09-27)

Three things about a Build run's downloads, from the maintainer's first look at them:

- [x] **The macOS download had no documentation.** The macOS job now builds the site as the
      Windows one does, and `package-macos.sh` runs `docs-offline.ps1` under the runner's
      PowerShell 7, the conversion `package.ps1` runs. The script no longer assumes Windows: it
      makes `-Out` absolute and writes its paths with forward slashes (its output on Windows is
      byte for byte what it was, under pwsh 7 and under 5.1). A reused executable's package gets
      this commit's documentation as it gets this commit's modules ("The documentation, into the
      reused package"). "Zip the download" refuses a package without `docs/index.html`.
- [x] **The macOS download was a zip inside a zip**, because GitHub zipped the uploaded zip
      again, and Archive Utility did not unpack the inner one properly. The zip `ditto` makes is
      now the artifact itself: `upload-artifact@v7` with `archive: false`, named
      `automation-platform-<version>-<commit>-macos.zip` (and `name` set to the same, because
      `overwrite` deletes by `name`, not by the file). `newer-macos` fetches it with
      `download-artifact@v8` and `skip-decompress: true`: without that, v8 unzips anything served
      as `application/zip`, which this is, with an unzip that keeps no permissions. The reuse step
      fetches it through the REST API (`gh api .../artifacts/<id>/zip`) rather than
      `gh run download`, which unzips with its own unzip, and builds past a download of the older
      `-macos` kind. The Windows download stays the folder that GitHub zips: one zip already.
- [x] **`build-info.txt` is gone from every package but a macOS one whose executable CI reused.**
      The executable knows its own commit, and both packaging scripts package the executable of
      their own checkout. The Windows job checks the README's `Build:` line instead of the file.
      The reuse step takes a fresh package's executable commit from the run that made it (checked
      against that package's README `Build:` line) and a reused package's from `binary=`, and a
      re-run that reuses its own first attempt now leaves no file. The log header's "no
      build-info.txt beside the .app" line is gone: every fresh macOS package would have said it.
- [x] **A `build-info.txt` left over from an older download is not used.** Unpacking a new
      download over an older folder, to keep `settings.toml` and the log, used to replace the
      file; now that fresh packages have none, the old one stays, and it would have named the
      old build in the log header and the Modules window's title. The application takes the file
      only when its `binary=` line (which the reuse step always writes) names the executable's
      own commit, `-modified` or not; otherwise the header names the executable's commit and the
      next line says the file is left over (`build_info::read`, unit-tested).
- [x] **A re-run that finds its own executable in a later run's package** (run B reused run
      A's executable, then "Re-run all jobs" on A) removed B's file but left B's README naming
      build B. The reuse step now sets the README's `Build:` line back to this run's commit in
      that case too.

Checked here: the workflows parse and every `run:` block passes `bash -n` or the PowerShell
parser; `check-macos.ps1`; the `build_info` and `portable` unit tests, compiled on their own;
`package.ps1 -NoBuild -NoZip` with placeholder binaries (no build-info.txt, docs present, the new
README check passes and fails for a wrong commit); `package-macos.sh --no-build --no-zip` on
Linux with and without a `pwsh` (docs and the README's line about them, or neither); the reuse
step, extracted from the workflow and run under `bash -e` with gh and ditto stood in for, through
seven cases (a fresh earlier package, a reused one, a README naming another commit, a re-run of
itself, the older `-macos` kind, a failed download, none at all); its artifact query through
gh's own jq against the real runs. Only a run shows:

- [ ] The first run with this change builds the Mac executable: the newest earlier macOS
      download is of the older `-macos` kind, and the reuse step says so and builds. "Build
      the documentation" passes on the Mac, the Package step prints `Docs: N page(s), N
      rewritten` from pwsh, and "Zip the download" lists `docs` and no `build-info.txt`, then
      writes `automation-platform-<version>-<commit>-macos.zip`.
- [ ] The run's page lists that file as the macOS artifact, and downloading it in a browser
      gives the zip itself: a double-click in Finder makes the `AutomationPlatform` folder, with
      the `.app`, `docs/index.html`, `modules` and `README.txt`, and nothing else zipped inside.
- [ ] `newer-macos` on 14 and 26: the download step reports a raw file (not an extraction), the
      unpack step finds the executable runnable, no inner zip, the documentation, and the
      signature verifying (a warning otherwise), and the log header names this run's build.
- [ ] Windows: the Package step's README check passes, the download has no `build-info.txt`,
      and the capability step's log header still names this run's commit.
- [ ] The first Luau-only push after that reuses: "reused the build from …", "The documentation,
      into the reused package" runs, "Zip the download" lists `build-info.txt`, `newer-macos`'s
      "Unpack it the way a tester does" prints it with `commit=` and `binary=`, and the
      downloaded README's `Build:` line and the log header say both commits, with no line after
      the header saying the file was not used.
- [ ] "Re-run all jobs" on a run: the macOS upload replaces the first attempt's file (overwrite
      by the file's name), and `newer-macos` downloads the new one.

## The keyboard hook over days of uptime (2026-09-27)

A session started at about three in the morning was used ten hours later: the overlay activated
and registered its captured keys (`captured set: vk 0x09/m0 … vk 0x09/m1 …`), and not one Tab
or Shift+Tab reached it — no `[keys] dispatch` line at all — while every `RegisterHotKey` hotkey
still worked; a restart fixed it. A `WH_KEYBOARD_LL` hook Windows removed without a word ("on
Windows 7 and later, the hook is silently removed without being called", Microsoft's
`LowLevelKeyboardProc` page) fits that, and nothing installed it again. But the log does not prove
it: had the hook been gone and a held hotkey (Alt+M, Alt+S …) been pressed, `settle_hotkey` would
have written "arrived through RegisterHotKey, not through the keyboard hook", and there is no such
line. A screen reader's modifier the hook had recorded as held fits every line as well:
`SCREEN_READER_MOD_DOWN` is set and cleared only by the hook seeing Insert go down and up, and the
hook is not called for keys going to NVDA's menu or dialogs (UIAccess, one integrity level above
ours) — NVDA+N, then Insert released in the menu, left it set, and from then on every captured key
was let through to the plugin and every hook hotkey passed on to `RegisterHotKey` as "expected",
with no line; a restart cleared it. A capture scope pinned to a window no longer in front, or a
menu flag left on, fit too. The application is expected to run for hours, if not days, so all of
them are handled below. Asked of the maintainer: was a hotkey pressed while Tab was dead, and was
an NVDA menu or dialog (NVDA+N, NVDA+F7, NVDA+Ctrl+G …) or an elevated window used just before?

- [x] **Windows: installed again when it is known to be lost.** A thread `keyboard-watch`
      (`backend/hook_watch_thread.rs`) with a message-only window of its own registers for
      suspend/resume (`RegisterSuspendResumeNotification` with a callback — a message-only
      window gets no broadcast `WM_POWERBROADCAST`) and for session changes
      (`WTSRegisterSessionNotification`), and on `PBT_APMRESUMEAUTOMATIC`, `WTS_SESSION_UNLOCK`,
      `WTS_CONSOLE_CONNECT` and `WTS_REMOTE_CONNECT` posts `WM_APP_REHOOK` to the hook's thread.
      That thread installs the new hook first and then takes the old one out — it handles no
      message in between, so no key is handled twice and none passes with no hook of ours —
      forgets what the old one saw go by (a screen reader's modifier held, a pending modifier
      tap, the modifier record late calls are judged by) when Windows had removed it, and keeps
      everything the application set; the hotkeys' record of held keys carries over, since a key-up lost with the old hook
      costs nothing there (`hook_carry_over_tests`). It writes no file: the outcome goes back to
      the watch, which writes the line.
- [x] **Windows: a hook removed while the machine stayed on is noticed.** The same thread
      registers the keyboard as raw input (`RIDEV_INPUTSINK`, without `RIDEV_NOLEGACY` or
      `RIDEV_NOHOTKEYS`) and compares each physical key-down — a device handle (injected input
      has none), a make, a real key — with when the hook was last called
      (`backend/hook_watch.rs`, pure, with tests): each key-down is judged by whether the hook
      was called since the key-down before it, with a second of slack, so a late hook and a
      late witness both count as alive. Three in a row that it was not called for, in front of
      a window the hook can see — the desktop with the keyboard is ours, the foreground
      process's integrity level is not above ours (an elevated program, a UIAccess screen
      reader's dialogs) — install it again. Counted, never timed; each re-install the witness
      asks for doubles the count for the next one, up to 320, until the hook confirms it by
      being called. Each re-install
      writes one `keys` line: the reason (`resumed`, `unlocked`, `the hook stopped seeing keys
      the system delivered: N key-downs …`), whether the old hook was still installed or
      Windows had already removed it (`UnhookWindowsHookEx` answering
      `ERROR_INVALID_HOOK_HANDLE`), and that it is first in the chain again; a failed one the
      `SetWindowsHookExW` error. No crate: `multiinput` busy-waits its thread (a one-nanosecond
      sleep in a loop, for days) and owns its window. Docs: `docs/api/keys.md` (When Windows
      drops the hook), `hotkey.md`, `timer.md`.
- [x] **macOS: the event tap over days** (`backend/macos/tap.rs`). The `TapDisabledBy*` line was
      written inside the callback, where a file write just after a wake can get the tap
      switched off again: now counted there and written by the run-loop observer. A tap whose
      mach port is invalid, or that stays off when re-enabled, is created again — the watchdog
      only ever re-enabled, which does nothing to a dead port and would have announced a repair
      every two seconds. After `NSWorkspaceDidWakeNotification`,
      `NSWorkspaceSessionDidBecomeActiveNotification` and the distributed
      `com.apple.screenIsUnlocked`, the tap is checked at once and the result logged.
      Type-checked only (`cargo check --target aarch64-apple-darwin -p macos-check`, with and
      without `--tests`).
- [x] **Windows: a screen reader's modifier recorded as held is forgotten when the keyboard goes
      where the hook is not called** (review of the above, finding 1). The watch also registers
      `EVENT_SYSTEM_FOREGROUND` (out of context, on its own thread, so an OCR call on the pump
      cannot delay it): when a window whose integrity level is above ours — or unreadable —
      comes to the front, and at every session change and suspend/resume, the backend forgets
      `SCREEN_READER_MOD_DOWN` and `TAP_ARMED` (`forget_keys_held_out_of_sight`); said once per
      session with how long ago the modifier was seen going down. `hook_watch::keys_go_unseen`.
      Rejected alternative: "held only while its auto-repeat keeps arriving" — pressing a second
      key stops the modifier's repeat, so NVDA+Up, Up with Insert held would lose the second
      press.
- [x] **Windows: why a captured key was let through is logged** — screen reader's modifier
      recorded as held (with its age in ms) or physically down, out of scope (both window
      handles), menu mode, `menuOpen(true)`; once per reason and window in front, reset when the
      captured set empties (`report_capture_passes`). The next "overlay dead, hotkeys alive" log
      tells these apart from a removed hook.
- [x] **Windows: a captured key-up follows a key-down that reached the application**
      (`swallow_captured`: `GetAsyncKeyState` in the hook does not yet include the event, so for
      a key-up it says whether the key-down got through). Before, the key-down that made the fifth
      miss reached the plugin, the new hook swallowed its key-up, and Tab or Space stayed held
      system-wide for as long as the capture stood.
- [x] **Windows: the watch cannot be switched off by one lost reply.** The witness raises its bar
      when it asks, not when the reply comes; a witness request is posted again while one is
      pending; a reply the hook's thread cannot post is kept in an atomic and read at the next
      key-down or change (`LOST_REPLY`); an undecodable reply still clears the request.
- [x] **Windows: no two hooks of ours in the chain.** `hook_watch::swap` (tested with a fake chain):
      new first, then old out; old refusing with another error than 1404 → the new one is taken
      out again (`Outcome::Reverted`); only if that fails too do both stay, the old one kept and
      tried again at every later swap. What the old hook recorded is forgotten only when Windows
      had removed it (`Outcome::forget_seen_keys`).
- [x] **Windows: a refused session registration is retried at the 1st, 2nd, 4th, 8th … key-down**
      (`hook_watch::retry_at`), not at every one.
- [x] **macOS: a tap that comes back off is not re-created every two seconds.** An invalid port is
      always re-created; one that stays off is re-created once, and a new one that is off too is
      left alone (`STAYS_OFF`) until it reports itself on or a wake/unlock/session-active asks;
      the old port's retain is released after `invalidate`. The module comment no longer claims
      every macOS failure is visible: a valid, enabled tap that never fires (Input Monitoring)
      has no witness.
- [x] **Decided (maintainer, 2026-09-27):** every resume, unlock and connect installs the hook
      again whether or not it was lost (ahead of NVDA's each time, which is the order a host
      started after the screen reader has anyway); and three counted misses, not five, make a
      re-install (`MISSES_TO_REHOOK`), so three real keystrokes at most reach the plugin first.
- [x] **Windows: keys typed into this application's own windows are not counted as missed, and
      what the hook recorded is forgotten when one of them comes to the front** (2026-10-05).
      Tab pressed in the module manager installed the hook again, the lines saying 3, then 6, 12
      and 24 key-downs had reached raw input and not the hook, with no call in between: Windows
      does not call a process's low-level keyboard hook for keys going to its own windows (the
      HTML probe page showed the same; Microsoft documents nothing, the mechanism is unknown). A
      key-down raw input reports with `RIM_INPUT` — this process had the foreground when it was
      pressed, the key's own report rather than a later foreground query — is not counted
      (`hook_watch::counts`), as keys for a window of a higher integrity level or another
      desktop already were not; the re-install line keeps what D1 added (foreground count, window
      in front, hook entries), for any other case. When one of this application's windows comes
      to the front, the watch has the backend forget the screen reader's modifier, the pending
      modifier tap, the captured keys held and the keys kept back (`hook_watch::front_unseen`,
      `hook_watch_thread::front_came`), with the line `a window the keyboard hook is not called
      for came to the front (one of this application's own: …) while the keyboard hook had a
      screen reader's modifier recorded as held … forgotten`, once per session like the others.
      The "arrived through RegisterHotKey" line names this application's windows among its
      reasons. Tests: `hook_watch` (the counting rule, a replay of that session, the window in
      front) and `hook_carry_over_tests` (the records forgotten). Docs: keys.md, hotkey.md.
- [ ] **Live (Windows), with NVDA:**
  - **first, on the build that was running on 2026-09-27** (it settles what happened): NVDA
      started before the application, an overlay active (Komplete Kontrol in REAPER), press
      NVDA+N, Escape, then Tab. If Tab reaches the plugin instead of the overlay until an NVDA
      key is pressed in REAPER, the stuck modifier record was the cause. On the new build: Tab
      works at once, and the log has one `a window the keyboard hook is not called for came to
      the front … recorded as held … forgotten` line;
  - NVDA+Space in an overlay: one `captured Space (vk 0x20/m0) was let through to the
      application: the hook has a screen reader's modifier … recorded as held — it saw one go
      down N ms ago` line with a small N, and NVDA leaves focus mode as before;
  - the watch's start line: `Raw input yes; session notifications yes; suspend/resume
      notifications yes; foreground events yes`, and the process's level (`0x2000` for an
      ordinary start);
  - a **sleep/resume** cycle: after waking, one `the keyboard hook was installed again
      (resumed: …)` line, saying whether the old hook was still installed or already removed —
      the first real answer to whether a sleep loses the hook; captured Tab works in an overlay
      straight away; NVDA+T, NVDA+Space and browse-mode arrows behave as before;
  - a **lock/unlock** (Windows+L, then the PIN): one `unlocked:` line, and typing the PIN adds
      no `the hook stopped seeing keys` line. The unlock line — and the `[system] the session
      was unlocked` line beside it (see "Uptime" below) — is also the only proof that
      `WM_WTSSESSION_CHANGE` reaches a message-only window; if neither comes, the session
      registration moves to the watch's hidden top-level window;
  - a **forced hook timeout, on a test machine only — never on the maintainer's**: suspend the
      whole process (Process Explorer, Suspend), type about fifteen keys in Notepad — each
      waits a second for the hook — and resume it, then type a few more keys. Expected: one
      `the hook stopped seeing keys the system delivered: 3 key-downs …` line saying Windows had
      already removed the old hook, and a captured Tab working again after it. (Whether it
      comes from the queued key-downs or from the next few depends on whether Windows still
      delivers the timed-out calls after the resume: late calls count as the hook alive.)
      Microsoft does not say after how many timeouts it removes a hook, so if no line comes,
      type more keys while suspended. After the line, open Notepad and hold nothing: typing
      Tab there must give one tab per press (a Tab left held by the re-install would repeat);
  - **NVDA restarted after the app**, then twenty browse-mode arrows on a web page: no `the
      hook stopped seeing keys` line. That settles what Microsoft does not document: whether
      raw input sees a key a hook ahead of ours swallows. If the line does appear, once, with
      "the old hook was still installed", it does — then say so in `hook_watch.rs` and
      `docs/api/keys.md` (the effect is one re-install that puts our hook ahead of NVDA's, as at
      start). The same question for the other programs that install key-swallowing hooks when
      they get focus: a **Remote Desktop client** full-screen with "apply Windows key
      combinations: on the remote computer", a **VM console** (Hyper-V, VMware or VirtualBox)
      and an **AutoHotkey script** started after the app — twenty keys in each, and no `stopped
      seeing keys` line. If one appears, the host's hotkeys would fire inside that window after
      it (our hook first again, hotkeys ignore the scope): document it, or leave those windows
      out of the count;
  - Tab pressed twenty times in the module manager, and in a module's Settings… dialog: no
      `the hook stopped seeing keys` line; then back into an overlay, a captured Tab works at
      once;
  - twenty keys typed in an **elevated** command prompt and in NVDA's settings dialog: no
      `the hook stopped seeing keys` line;
  - a **Remote Desktop** connection to the session: one `connected to a remote client` line,
      and captured keys work over the connection. Keys arriving over RDP may carry no device
      handle in raw input, which leaves the witness deaf there (harmless: connect and unlock
      still install the hook again); note whether a forced timeout over RDP gives a `stopped
      seeing keys` line.
- [ ] **Next Mac session:**
  - sleep the Mac and wake it: `the Mac woke from sleep: the event tap was checked and is valid
      and enabled` (or a repair line), and a captured Tab works at once;
  - lock the screen (Control+Command+Q) and unlock it: `the screen was unlocked: …`;
  - switch to another user and back: `this user's session became active again: …`;
  - a main-thread stall of over a second (a probe OCR loop) while pressing keys: `the system
      disabled the event tap N time(s) because a callback took longer …` appears after the
      stall, and the tap works right after;
  - whether the event tap is called for keys going to this application's own windows, which
      the Windows hook is not (2026-10-05): with a module that captures a key for every window
      (no `host.keys.scope`), press that key in the module manager. The module's callback running
      and the manager not getting the key say it is. Its place (`kCGHIDEventTap`, where keys enter
      the window server, before it is decided which application gets them) says it should be.
      Nothing on a Mac counts missed keys, so no re-creation follows either way; if it is not, a
      key held across a switch to the manager stays recorded as held (`held_back`, `TAP_ARMED`),
      and the tap needs the forgetting the Windows watch does;
  - withdraw Accessibility from the application while it runs, then give it back. Not known
      which the system does: switch the tap off (re-enable lines, then at most one `created
      again … The new tap is off as well` line and silence), or invalidate its port (`the event
      tap had to be created again … and could not be` once, then `the event tap was created
      again` after the grant is back). Either way no line every two seconds.

## Uptime: sleep, lock, display changes, the log and what grows (2026-09-27)

Two read-only audits listed what breaks or grows while the application runs for days (system
events and time; resources over hours and days). The keyboard hook is the section above; the
macOS items of the audits (VoiceOver re-arming, ScreenCaptureKit's back-off, App Nap, pid reuse,
the macOS system events) are not in this list. Done for Windows and the shared code:

- [x] **System events** (`system_events.rs`, `backend/hook_watch_thread.rs`). The keyboard
      watch's thread now starts with the event loop (`Backend::watch_system`), whether or not a
      hook is ever installed, and its session and suspend/resume registrations — not made twice
      — also push `Sleep`/`Wake`/`Lock`/`Unlock`/`Connected`/`Disconnected` for the event loop;
      raw input, the foreground events and the witness are armed only when the hook is. A second,
      hidden top-level window on that thread (never shown, `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`)
      hears the broadcasts a message-only window does not: `WM_DISPLAYCHANGE`, `WM_DPICHANGED`,
      `WM_SETTINGCHANGE` with `SPI_SETWORKAREA` or `SPI_SETLOGICALDPIOVERRIDE`, and `TaskbarCreated`
      (log only: wxWidgets' `wxTaskBarIcon` answers it and adds the icon again, checked in
      `src/msw/taskbar.cpp`). The pump delivers each batch through `HostEvents::on_system`: one
      `[system]` line per event (how long the machine slept, the session was locked), both epochs
      turned over, the window in front reported again as an activation and a focus round after a
      resume, unlock or connect, a focus round after a display/scale/work-area change, the
      `[env]` display lines again (now with a `monitors` line: each monitor's effective DPI —
      `system dpi` is fixed for a session), desktop duplication reset, and the audio output let
      go after a resume or connect. No host.* call; docs: `window.md` (onTrigger/onFocus,
      Windows), `timer.md` (epoch, inputEpoch). **A storm is bounded** (after the review): an
      event equal to the one queued just before it is folded into it, a full queue drops its
      oldest rather than its newest, the display-family lines (display, scale, work area,
      taskbar) are written once and then counted per kind through `logging::Repeats` (a line a
      minute at most, flushed by the tick), and the `[env]` display lines are written again only
      when they changed. Since the merge with the macOS group (below) the same queue, event
      type, batch and plan serve both platforms.
- [x] **The log rotates within the session** (`logging.rs`, `Sink`): a byte count kept beside the
      file, past 8 MB the file goes to `.log.1` and the new one begins with a `[host] log
      continued` line, the build and the settings; a refused rename keeps writing and is tried
      again after another 512 KB, with a `could not be moved` line at each try (a reader that
      holds the file without sharing its deletion, such as `Get-Content -Wait`, keeps it growing
      until it lets go; copy-and-truncate was rejected, it can lose a line another copy
      appends). After the review, an opening that fails is tried again after another 512 KB
      rather than at every line, and a rename that went through is remembered, so the file that
      opens begins with the continuation lines. `file-rotate` 0.8 checked and not used (loses every line while a
      rename fails, `assert!`s inside the logger, no continuation hook, brings chrono). Docs:
      `log.md`. Tests in `logging.rs`.
- [x] **A second copy writing the same log** (found in the review of the merge with the macOS
      changes). A headless run beside the application — exempt from the instance guard on
      purpose — opens the same file and counts only its own lines, so it can move the file
      first; the application's handle then follows it to `.log.1`. At the application's own 8 MB
      its rename would have put the headless run's new file over `.log.1`, where its own lines
      had gone. Now a rotation first checks that the path still names its open file
      (`Fs::still_names`, through `same-file` 1.0.6, already in the lock file under walkdir);
      when it does not, nothing is moved, and the copy goes on in the new file after a `the log
      had been moved away before this session moved it` line and the continuation lines. Left:
      the headless run moving its own new file over `.log.1` while the application still writes
      there — that needs 8 MB written by the headless run. Test in `logging.rs`
      (`a_log_another_copy_moved_is_not_moved_again`, on the real filesystem).
- [x] **A continued file names the app folder** (same review). The continuation lines repeat
      the header's `app folder` line (kept from the header, so no second write probe), after the
      `translocated` line on macOS, whose text refers to it. Five lines at most. Docs: `log.md`,
      whose macOS section now carries the `translocated` line; `building-on-macos.md` and
      `macos-permissions.md` ask for `.log.1` as well, since the first block is there after a
      rotation.
- [x] **A module error repeated is written once, then counted** (`report_callback_error`,
      `logging::Repeats`): per module, context and message; one summary a minute at most, flushed
      by the tick when the error stops; forgotten on disable/enable, and (after the review) on a
      reload and a rolled-back hot-load. A full table (512 lines) forgets the line seen longest
      ago instead of refusing new ones, so a steady error is still counted while a changing one
      floods. Docs: `module-runtime-and-lifecycle.md`, `log.md`.
- [x] **A hotkey another program holds is logged once** (`refresh_hotkeys`, `os_refused`): the
      retries at every register and release are counted, and the `is now held` line says how
      often it was refused. Docs: `hotkey.md`.
- [x] **The `captured set` line** (trace or calibrate) is written once per tick and only when it
      changed, instead of once per captured key.
- [x] **Desktop duplication** (`backend/dxgi.rs`): hangs count within ten minutes, not for the
      session; stopped, it is tried again on a fresh thread after 10 minutes, doubling to an hour
      (never while its old thread is still inside the driver); a resume, unlock or connect
      clears the back-off, the hang count and the doubling and tries a stopped one at once. A
      display or scale change alone (after the merge review) does so at most once in ten
      minutes, only when there are hangs to forget or duplication has stopped, and keeps the
      doubling and the back-off (`dxgi::after_system_event`): at every flap of a display whose
      link keeps dropping it had wiped the hang count, so a driver hanging at each change was
      never stopped. The `Disabled` reason changed wording (`screen.md`, Failure reasons).
      Tests beside the existing ones. Cross-reference: M5. After the review, the doubling is
      forgiven when a fresh engine answers 100 reads in time without a hang, and when the
      switch is turned off and on.
- [x] **`host.sound` follows the device** (`sound.rs`): the stream is built with rodio's mixer
      and cpal (rodio's re-export) with an error callback; the output is opened again after a
      stream error, when the default device changed, and after a resume or connect. Docs:
      `sound.md`.
- [x] **Menu shots: the first two openings of each control** per overlay and VM; the third says
      so once (`overlay-runtime`, `_menuShots`; test in `overlay_menu_tests.rs`). Docs:
      `overlay.md` (O.calibrating). Counted per overlay and control object (weak keys), not per label, so
      overlays that share a label keep their own counts.
- [x] **`host.ocr.read` keys are forgotten** once no read with them waits (`ocr/lua.rs`,
      `forget_settled`), as `snapshotAsync`'s are. Docs: `ocr.md`.
- [x] **`imageSearchAsync`, `imageSearchEach`, `matchCellsAsync`: at most 64 waiting per module**;
      the 65th ends the module's oldest with `too many image searches waiting for this module
      (64); the oldest was ended`, which the worker then skips (`image_search.rs`). Docs:
      `screen.md`. First 16; raised after the review, because a sample-library VM's landmark
      gates all ask in one poll (about 14 waiting counted for Soundiron, the header's reads on
      top) and the cap would have ended the same gates' searches every poll. The overlay
      runtime's landmark gate, value reads and `gbutton` activation, and Kontakt's resize-grip
      check, now read `cb(nil, reason)` as "could not look" — the last verdict stands — instead
      of "not there".
- [x] **One neural recogniser thread** (`backend/paddle_ocr.rs`, `ask`) instead of a thread per
      small region: still handed every small region before `Windows.Media.Ocr` starts, still used
      only when that reads nothing; a region no longer wanted is cancelled (skipped, or stopped
      before the session). Docs: `ocr.md` (Windows). After the review: the thread's loop is kept
      going by an outer `catch_unwind`, and if the thread ever ends its queue is answered with
      nothing and no region is queued for it again (`ALIVE`, `orphan_all`), so a synchronous
      `host.ocr.recognize` cannot wait for ever on the event loop.
- [x] **Screen-reader workers left in a call that never returns are bounded** (`speech/prism.rs`):
      counted, the stall line says how many, and at four still waiting no new worker is started
      until one comes back. Docs: `speech.md`.
- [x] **The overlay runtime's memos are bounded** (`memoByOrigin`, the embedded binding's
      identity cache: the 64 keys used last), and an `identify` that answers `nil` is asked
      again instead of being stored as `false` for the life of the window — up to 8 times in a
      row per control (after the review), then taken as no, so a plain-Lua `identify` that says
      no by returning nothing does not run its UIA or OCR call at every recheck. Docs:
      `overlay.md`.
- [x] **Hotkey ids past 0xBFFF: no change.** Measured on Windows 11 26220 that `RegisterHotKey`
      grants ids up to 0x7FFFFFFE; said at `next_id` in `lib.rs`.
- [ ] **Audit items neither uptime group took** (listed after the merge review; the system
      audit's and the resource audit's numbers):
  - **System audit P2-11, both platforms: the single-instance listener gives up** after 20
      accept errors in a row (`instance.rs`, `failures >= 20`); a later second start then waits
      10 s and gives up, until a restart. Bind again with a back-off (1 s, doubling to a
      minute), said once; test `serve` over an iterator of accept results.
  - **System audit P3, `Instant` across a suspend:** on macOS `Instant` stops while the Mac
      sleeps, so a `host.timer.after` deadline stretches by the sleep; on both platforms an
      `every` timer fires once after a resume and does not catch up (`timers.rs`,
      `t.next = now + t.interval`). Measure Windows' clock across one sleep, then state
      `host.now()`'s and the timers' behaviour per platform in `timer.md`. Duplication's hang
      clock runs on it too; the `[system]` storm counts are handled at a wake since the merge
      review.
  - **System audit P3, Windows power throttling for the hook thread and the pump:** neither
      opts out (`SetThreadInformation(ThreadPowerThrottling)`, or the whole process with
      `SetProcessInformation(ProcessPowerThrottling)`); the gamepad thread does
      (`gamepad/win_thread.rs`). O18 asks the same for the OCR threads; one measurement.
  - **System audit P3, `CoInitializeEx` on every UIA call** (`uia.rs`, `automation`), never
      balanced: a per-thread counter that wraps after about 4 billion calls. Initialise once
      per thread.
  - **System audit P3, nothing brings the application back after a reboot** (Windows
      Update): no `RegisterApplicationRestart`, no autostart. Optional; an Application
      settings switch if done.
  - **Resource audit 11: the template cache has no byte cap**, only 64 entries
      (`TEMPLATE_CACHE_CAP`, `lib.rs`), so a module with many whole-window templates holds 64
      of them; `template.rs`'s cache of scaled variants has a byte cap that could be copied.
- [ ] **Live (Windows), with NVDA:**
  - the start line: `[system] sleep, lock, connections and display changes are heard on the
      keyboard watch's thread: session notifications yes; suspend/resume notifications yes;
      display, scale, work-area and taskbar broadcasts yes`;
  - **sleep and wake**: `[system] the machine is going to sleep — heard at …, written … later`
      and `the machine woke from sleep (asleep for N min); …`, both written after the wake (the
      first line without the "heard at" part if the loop turned before the machine went down);
      compare the length with `powercfg /sleepstudy` or the System event log; the active overlay is looked at again
      at once (Tab works, NVDA speaks), and a `desktop duplication is asked again` line only if a
      module reads through duplication;
  - **Windows+L and unlock**: `the screen was locked`, `the screen was unlocked (locked for N
      s); …`;
  - **display scale 100 % → 125 %** (Settings, Display): `the display scale changed` (perhaps
      with `(N notifications)`) and the `[env] monitors: … 120 dpi (125%)` line; overlay clicks
      still land on target. If no `[system]` line comes, a hidden window does not get
      `WM_DPICHANGED` or `SPI_SETLOGICALDPIOVERRIDE` — say so here; a resolution change should
      give `the display configuration changed` all the same;
  - **the taskbar set to hide automatically**, then back within ten minutes: one `the work
      area changed` line (perhaps with `(N notifications)`), and for the change back only a
      count, `the same work-area change came 1 more time(s) in the last N s`, a minute after the
      first line at the earliest (the display family is counted, not written again, within ten
      minutes); a second full line would mean the counting does not hold;
  - **Explorer restarted** (Task Manager, Restart): `the taskbar was created again` and the tray
      icon back, reachable with the keyboard;
  - **a Remote Desktop connection** to the session: `connected to a remote client (away for
      …)`;
  - **switch user** to another account and back: `the session was switched away from the
      console`, then `the session is back at the console (away for …)` and the overlay looked at
      again;
  - **the log past 8 MB** (trace on for a working day, or a copy of a large log placed as
      `automation-platform.log` before starting): `.log.1` appears, the new file begins with
      `log continued`, and opening the log in a viewer that locks it gives a `could not be
      moved` line every 512 KB while it is open, no lost lines, and the move at the first try
      after the viewer is closed;
  - **a display link that keeps dropping** (a TV or AV receiver in standby on HDMI, if one is at
      hand): one `the display configuration changed` line, then `the same display change came N more
      time(s)` at most once a minute, and `[env]` lines only when the displays really changed.
      With a module reading through desktop duplication meanwhile: at most one `desktop
      duplication is asked again after the system event: … (after a display or scale change,
      at most once in 10 minutes; …)` line in ten minutes, and if duplication hangs at the
      changes, `has taken more than 2 s (1 of 3 …)` up to `(3 of 3 …)` and one `stopped
      answering` line, then no `[capture]` line per flap until its wait is over;
  - **`examples/sound`**: make another device the default and play: one `[sound] … opened again`
      line and the sound on the new device; unplug a USB audio device while a sound plays: `the
      audio output reported an error`, and the next sound plays on the new default;
  - **a module whose poll raises every 500 ms**: one full line, then `the same timer error came
      N more time(s) in the last 60 s` once a minute, and nothing after it is fixed; reloaded
      with the error still there, the next one is written in full again;
  - **Melodyne's selection watcher with trace on**: Process Explorer shows one `paddle-ocr`
      thread whatever the rate, the CPU of the process stays near the WinRT cost alone, and the
      `waited … for paddle` lines appear only for regions WinRT read nothing in;
  - **a 24-hour headless soak** with a timer-only module (the resource audit's plan): private
      bytes, handles, threads and GDI/USER objects flat, and the log's size;
  - **hotkey ids past 0xBFFF on Windows 10** (not measured there): a headless probe that
      registers id 0xC001 on Ctrl+Alt+Shift+Win with the unassigned virtual key 0xE8 and
      unregisters it at once.
- [ ] **Next Mac session:** `examples/sound` with headphones unplugged while it plays, and with
      the output switched in Control Center: `[sound] the audio output reported an error` or
      `… opened again`, and the next sound audible (cpal's `DeviceIsAlive` listener, from its
      source; never seen live — which is why `sound.md` does not describe it).
- [ ] **`host.sound` on rodio 0.21** (checked from its source 2026-09-27, not taken in the uptime
      change): `OutputStreamBuilder::with_error_callback` and `open_default_stream` do what
      `sound.rs`'s `build`/`stream` do by hand. Moving means cpal 0.15.3 → 0.16 on both
      platforms (the macOS half only verifiable on a Mac), `Sink::connect_new` instead of
      `try_new`, `log_on_drop(false)`, and keeping the default-device check (its
      `OutputStream` does not name its device). Also removes cpal 0.15.3's leaked event handle
      per failed output, if 0.16 fixed that (not checked).

## macOS over days of uptime (2026-09-27)

From the two uptime audits of 2026-09-27 (system events and time; resource growth): the macOS
half. Everything below is type-checked only (`cargo check --target aarch64-apple-darwin -p
macos-check`, with and without `--tests`); the pure parts run under `cargo test` on Windows
(`backend/macos/backoff.rs`, `backend/macos/handle_table.rs`, `speech/vo_park.rs`,
`speech/pace.rs`, `system_events.rs` with its `what_each_event_sets_off` table), and where they are called
from is checked on the source (`lib.rs`, `macos_uptime_wiring_tests`).

- [x] **System events reach the host** (audit P1-2, macOS). `backend/macos/system.rs` hears the
      workspace's will-sleep, did-wake, screens-did-sleep/-wake and session resign/become-active
      notifications, the distributed `com.apple.screenIsLocked`/`…Unlocked` (CoreFoundation
      centre, delivered immediately in the background), and display reconfiguration
      (`CGDisplayRegisterReconfigurationCallback`: `NSApplicationDidChangeScreenParameters…`
      needs a running `NSApplication`, which a headless run has not). Each is queued with the
      wall-clock time it was heard (the host's queue, `system_events::push`) and delivered first
      in the pump's drain as one batch through `HostEvents::on_system` — since the merge, the
      Windows shape (see the merge item below). The host writes one `[system]` line per event
      (`system_events.rs`: how long the sleep, the lock or the absence lasted, by the wall clock,
      since `Instant` stops while a Mac sleeps; "heard at …, written N later" when the line is
      only written after the wake; "how long it slept is not known" when the sleep notice was
      itself heard only as the Mac woke; a display change once, then counted, with at most one
      line a minute, the count flushed from the tick), turns both epochs over, hands it to
      speech and does the rest of the host's plan. Before handing the batch over (since the
      merge review; it came after at first) the backend checks the event tap at once after a
      wake, an unlock or the session coming back (`tap::recheck_soon` — the tap's own
      subscription from 25713e5 moved into `system.rs`, so each notification is registered
      once), brings a ScreenCaptureKit back-off's next probe forward after a wake, an unlock,
      the session coming back, the displays waking or a display change
      (`SystemEvent::rechecks_capture`), and once per batch with an event that may have changed
      what is in front reads the frontmost application again (an unannounced one is taken over,
      and announced as an activation only when the host does not report the window in front
      itself after the batch — so after the displays wake or a display change, not after a
      wake, an unlock or the session coming back). The `[env] display N` lines after a display
      change are the host's, written when they differ from the last ones
      (`MacBackend::display_environment`). No host.* call, no setting.
- [x] **One system-event design for both platforms** (merge of the Windows and macOS uptime
      groups, 2026-09-27). The two groups had built the event type twice (`system_events.rs`
      with a batch, and `backend::SystemEvent` delivered one at a time with its time) and the
      `[system]` line twice. Now: one `SystemEvent` in `system_events.rs` (`Sleep`, `Wake`,
      `Lock`, `Unlock`, `Connected`/`Disconnected { remote }` — macOS fast user switching is the
      console kind —, `DisplaysAsleep`/`DisplaysAwake` from macOS only, `DisplaysChanged`, and
      `Scale`/`WorkArea`/`TaskbarCreated` from Windows only); one queue (`push`/`take`: repeats
      folded, the oldest dropped when full, and now one line when it did — the macOS queue's
      line); one shape, `HostEvents::on_system(Vec<Stamped>)`; one `Since::plan` with the Windows
      group's storm counting (`logging::Repeats`, flushed by `log_housekeeping` — macOS's own
      minute counter and `flush` are gone) and the macOS group's wall-clock rules (the late note,
      "not known" for a sleep heard at the wake, nothing on a clock set back, days in a length;
      the Windows `(at …)` stamp is folded into the late note); one set of rules on the event
      (`front_may_have_changed`, `reports_front`, `reopens_audio`, `rewrites_display_lines`,
      `rechecks_capture` — Windows' `resets_capture` and macOS' `brings_capture_probe_forward`
      were the same question —, `retries_screen_reader`), from which each line's tail is
      written, so a line cannot claim what the host does not do. New on macOS by the merge: the
      window in front is reported again as an activation after a wake, an unlock or the session
      coming back, and the audio output is let go after a wake or the session coming back (as
      on Windows); the focus round after an event is the host's (the backend's
      `mark_focus_dirty` after it is gone); the `[env]` display lines go through
      `Backend::display_environment` (the backend's own last-written copy is gone). Docs:
      `window.md`, `timer.md`, `sound.md`.
- [x] **The merge's review, fixed** (2026-09-27). On macOS the backend's part of a batch —
      the tap check, the ScreenCaptureKit probe brought forward, the front application read
      again — runs before the host hears it, as Windows resets duplication first: after it,
      the host's reactivation captured while ScreenCaptureKit still skipped, and an application
      that came to the front while the screen was locked was activated twice in one drain
      (`watch::recheck_front(announce)`; `queue::taken_over` still arms the focus ladder for an
      untitled window; wiring test `system_events_are_delivered_first_and_acted_on`). On
      Windows a display or scale change alone no longer resets desktop duplication at every
      flap (the Desktop duplication item under Uptime). A wake writes the display-family counts
      still owed and forgets them (`Repeats::take_all`): on macOS the process clock they run on
      stops while the Mac sleeps. A start event folded in the queue is stamped by its last
      notice. Docs: `window.md` (onTrigger, and onFocus's untitled-window rounds on macOS,
      written down now), `timer.md` (how often the epoch turns), `screen.md`.
- [x] **VoiceOver comes back after a refusal** (audit P1-3). `speech/vo_park.rs`: only
      `errAEEventNotPermitted` (-1743, by the Apple Event's code or `(-1743)` in `osascript`'s
      output) parks the path until the setting is ticked again; anything else is tried again on
      prism's schedule (`speech/pace.rs`, moved out of `prism.rs` and shared: 3 s for five
      minutes then 10 s while VoiceOver runs, 3 s for a minute then 30 s while it does not,
      counted from the first refusal), and at once when VoiceOver's pid changes (asked before
      every line anyway) or on a wake, an unlock or the session coming back. A look lets one line
      through and closes the gate behind it until VoiceOver has answered; lines said meanwhile go
      to the system voice at once. A refusal whose `osascript` child had to be stopped at its 5 s
      limit puts its run of refusals on the slow pace (10 s / 30 s) at once. The log: one line
      per run of refusals, one when the pace changes, one when a refusal of another kind comes
      (by its codes, or its words when it has none), one when VoiceOver takes a line again.
- [x] **The VoiceOver transport, bounded** (review of the above, finding 1 and 7). Lines handed
      over in the same opening of the gate as a refused one are not offered (`voiceover.rs`:
      each line carries the opening it was handed over in; the worker says them through the
      system voice at once instead of paying the refusal again, one after another). After the
      Apple Event is refused, `osascript` follows only where it can help (`vo_park::after_event`):
      not after -1743 (refused the same way), not after -600 (it would start VoiceOver), and after
      -1744 once per arming — start-up, each tick of the setting — since it waits on the
      Automation question for up to 5 s; after that each line asks
      `AEDeterminePermissionToAutomateTarget(…, askUserIfNeeded: false)` first, on the speech
      thread, and goes to the system voice at once while the question is open. The Apple Event
      is given up for `osascript` for the session only when `osascript` then took the same line
      after a refusal that was not about permission (`vo_park::event_broken`) — a child that took
      a line after -1744 means the question was answered meanwhile. A refusal of a line handed
      over before the setting was ticked is not held against the path the tick re-armed.
- [x] **ScreenCaptureKit backs off instead of switching off** (audit P1-4).
      `backend/macos/backoff.rs`: after a capture not answered within 1.5 s it is not asked for
      30 s, then one probe while the reads go to Core Graphics; each probe not answered within
      the deadline doubles the wait, to 10 minutes; captures that time out together count once;
      only an answer within the deadline (a picture or an error) ends it. Review fixes (findings
      2, 3, 3b–3d): the probe is **not waited for** where an older capture function exists
      (`capture::sck_probe` — its handler reports to `Backoff::probe_answered`, and a probe with
      no word after twice the deadline is written off at the next `attempt`, which also frees a
      probe whose caller unwound); only a macOS without the older functions waits for it, having
      nothing else to answer the read. A wake, an unlock, the session coming back, the displays
      waking or a display change no longer END the back-off — they bring its next probe forward,
      once per step and not sooner than 30 s after the timeout that began the step, and the
      level stays (`Backoff::clear`). The back-off's lock is never held while a line is written.
      Only `capture.rs`'s ScreenCaptureKit switch changed.
- [x] **App Nap** (audit P2-7). `backend/macos/activity.rs`: one
      `UserInitiatedAllowingIdleSystemSleep | LatencyCritical` activity for the application,
      held while keys are captured (`tap::set_captured_keys`, non-empty — the host's notion of
      an active overlay) or a controller's buttons or axes are listened to (the gamepad's own
      activity folded in; its `activity` status says which). Info.plist (`package-macos.sh`)
      checked: `LSUIElement`, no `NSAppSleepDisabled`, and deliberately none (a comment there
      now says why).
- [x] **pid-keyed caches forget an application that quits** (audit P2-10, resource audit 4).
      `NSWorkspaceDidTerminateApplicationNotification` (in `system.rs`) → at the next drain
      `watch::forget_process` (the observer and its run-loop source, a busy retry, a
      written-off verdict), `ax::forget_process` (`EXE_BY_PID`, `BUNDLE_BY_PID`, `BUSY_UNTIL`,
      the remembered front window) and `handles::forget_pid` → `ax::forget_handles`
      (`INSET_BY_HANDLE`, `RING_REPORTED`, which moved to module level). Up to 1024 quits per
      turn of the loop are queued; past that the log says so (review finding 8).
- [x] **The handle table drops windows that are gone** (resource audit 3). Each entry knows the
      window it lies in (`handle_table.rs`, pure and generic: `Kind::Window` is its own owner,
      `Kind::In(window)` a control; `window_controls` passes its window, the focus chain adopts
      its links to the window at its end). A sweep (at 4096 entries, then at twice what it
      kept) still drops dead processes and asks each window's application once for its role —
      `kAXErrorInvalidUIElement` is gone, with every control inside it; no answer keeps it — at
      most 256 windows and 150 ms a sweep, busy applications skipped. Handles stay monotonic, so
      the contract is unchanged. Review fixes (findings 4, 9, 11): the sweep runs from the pump
      after the drain (`handles::sweep_if_due` in `pump_pending`), no longer inside the intern
      that crossed the size — that was an activation, a control walk's 300 ms budget or the
      focus chain; a window paired with a `CGWindowID` that `CGWindowListCopyWindowInfo(All)`
      still lists is kept without asking its application (`handle_table::needs_asking`);
      `ax::window_id` fills in only the id (`handles::set_window_id`) instead of re-interning,
      which turned a control into a window; a process is "exited" only on `ESRCH`, not on
      `EPERM` (another user's process).
- [x] **The headless loop drains an autorelease pool per turn** (resource audit 10).
- [ ] **Left open by the review** (2026-09-27), each waiting for what a Mac shows:
  - **Which VoiceOver refusals reach the host at all.** The Apple Event is sent `NoReply`, so
      only macOS's refusal to send it comes back (-1743, -1744, -600, -609 …); VoiceOver's own
      refusal — "Allow VoiceOver to be controlled with AppleScript" unticked — and a VoiceOver too
      busy to answer are not seen, and such a line is lost silently, neither VoiceOver nor the
      system voice saying it (as before this change). Through `osascript` (after the transport
      switched) they do come back. Measure first (live check below); if lines are lost, ask for a
      reply with a short timeout on a thread that can receive it, or probe with `osascript` once
      per look.
  - **An application quitting without the workspace's notice** (review finding 8). Not
      documented to be posted for background-only applications and menu-bar agents; if the live
      check below shows one missing, key `EXE_BY_PID`/`BUNDLE_BY_PID` by (pid, start time)
      (`proc_pidinfo(PROC_PIDTBSDINFO)`'s `pbi_start_tvsec`) or compare it on a cache hit.
  - **One "invalid element" drops a window** (review finding 10, not changed): an element
      reference an application has invalidated does not become valid again — a window it rebuilt
      is a new element and gets a new handle anyway — so a second sweep's agreement would only
      delay the reclaim by a doubling. Paired windows the window server still lists are no longer
      asked at all. Revisit if the handle-table live check shows an open plug-in's overlay
      starting over after a sweep.
  - **How long a Mac slept, from the kernel** (review finding 12, not changed): `kern.sleeptime`
      / `kern.waketime` would measure a sleep whose notice came late, but after Power Nap's dark
      wakes they give the last stretch only, not the night. The line now says "not known"
      instead of "asleep for 0 s"; compare the figures with `pmset -g log` before using them.
- [ ] **Next Mac session** (all of it written blind):
  - the start: `system events: … — yes; the screen lock — yes; display changes — yes`;
  - **sleep and wake** with an overlay active: `[system] the machine is going to sleep` (possibly
      written only after the wake, then with "— heard at …, written … later"), `[system] the
      machine woke from sleep (asleep for …)` — compare with `pmset -g log` — then `the Mac woke
      from sleep: the event tap was checked and is valid and enabled`, and captured Tab working
      at once. A `how long it slept is not known` line means the sleep notice was heard only at
      the wake: note how often. New by the merge: the window in front is reported again as an
      activation after the wake (the overlay resumes rather than starting over), and
      `examples/sound` after the wake opens the output afresh (`[sound] … opened again` is not
      expected; the sound simply plays);
  - **lock and unlock** (Control+Command+Q, then the password): `[system] the screen was locked`,
      `[system] the screen was unlocked (locked for …)`, the tap line after it — and whether the
      lock notices arrive at all while this accessory application is in the background. No
      spurious `loginwindow … no activation said so — …` line after the unlock, and an active
      overlay resumes where it was rather than starting over — since the merge the host reports
      the window in front again as an activation after the unlock, as on Windows;
  - **an application brought to the front while the screen is locked** (in Terminal
      `sleep 20; open -a TextEdit`, lock at once, unlock after half a minute): one `TextEdit
      (pid …) is in front, and no activation said so — taken over now; the system event reports
      its window` line, and ONE activation of TextEdit's window: a module's `onTrigger` callback
      for it that writes a line (`host.log.info`) writes it once. Twice would mean the
      workspace announced it as well, as Windows can — say so here;
  - **fast user switching** to another account and back: `session was switched away from the
      console` and `session is back at the console (away for …)`;
  - **displays**: let them sleep (`displays went to sleep` / `displays woke`); change the
      resolution or scale in System Settings, or plug in or unplug an external display:
      one `[system] the display configuration changed` line, and further changes within ten
      minutes counted: `the same display change came N more time(s) in the last … s`, at most
      once a minute — several full lines within a minute would mean the counting does not hold —
      followed by the `[env] display N` lines when the arrangement differs, and an overlay's
      clicks still landing on target. A display change before a sleep and another at the wake
      (an external display): the one at the wake written in full, after a `the same display
      change came N more time(s) within … s of its last line, before the machine slept` line
      if some were still counted; whether the callback fires in a headless run too;
  - **VoiceOver off and on with Command+F5** in the middle of a session with "Speak through
      VoiceOver" ticked: lines while it is off go to the system voice; after it is on again the
      next line reaches VoiceOver (and the braille display) without touching the setting — a
      `VoiceOver started (pid …)` or `runs as a new process` line if a refusal had parked the
      path;
  - **VoiceOver with "Allow VoiceOver to be controlled with AppleScript" unticked**: either one
      `VoiceOver would not take a line (…)` line with the error code, the system voice speaking,
      a `still would not take a line … every 10 s` line after five minutes, and after ticking it
      again a `VoiceOver took a line again` line within 10 s — or **no refusal at all**: VoiceOver
      silent AND no system voice, lines lost (see "Left open" above). Which one it is, and which
      code comes back if any;
  - **the Automation question left unanswered** (this application's Automation entry reset with
      `tccutil reset AppleEvents com.automationplatform.app`, the setting ticked, the dialog not
      answered): how long the
      first line waits (expected: up to 5 s, one `osascript did not come back within 5 s` in the
      refusal line), that later lines are not delayed (each look asks macOS without a dialog:
      `the system says this application may not send VoiceOver Apple Events yet — the Automation
      question has not been answered [-1744]`), whether the dialog comes back on its own, and
      that allowing it brings VoiceOver back within 10 s;
  - **ScreenCaptureKit after a wake** with an OCR overlay in use: whether any capture times out
      (a `did not answer … within 1500 ms … not asked for 30 s` line), then the wake's `… is
      asked again at the next capture rather than at the end of its … back-off` line, and a
      `ScreenCaptureKit answered a probe within its deadline` line bringing it back — or a `did
      not answer the capture that asked it again after its back-off` line doubling it. No read
      should wait 1.5 s for a probe on a macOS that still has the older functions. When a
      back-off was already running from 30 s or more before the sleep, the `… is asked again at
      the next capture` line comes BEFORE the wake's `[system]` line, and the reactivation's
      first capture is the probe (its answer line right after the wake), not a Core Graphics
      read with the probe only at the next poll;
  - **App Nap**: an overlay active (keys captured), the application in the background for ten
      minutes: Activity Monitor's Energy tab reads App Nap: No and Preventing Sleep: No for it,
      `pmset -g assertions` lists nothing of it, and the Mac still sleeps when idle. Then with no
      overlay active for ten minutes: App Nap: Yes is expected; press a captured key after an
      overlay activates again and look for a `reached the event tap late` line — that line is
      also the answer to whether the tap with nothing captured suffers from a nap;
  - **an application quitting**: open and quit sforzando (or any observed application) twice:
      `stopped observing … : it quit` each time, and the second launch observed afresh (`observing
      focus in …`), not skipped as known. Also a **menu-bar agent** (one with no Dock icon)
      quitting, with detailed (trace) logging ticked: whether a `pid … quit: forgotten` trace line appears for it at
      all — if not, the workspace does not report it (see "Left open");
  - **the handle table**: a long session opening and closing plugin windows until a `handle table
      swept: … window(s) gone of N asked, M kept by the window list` line appears — how many
      were dropped, how long it took, whether the window list could be read (without Screen
      Recording too), and that an open plugin's overlay did not start over after it;
  - Terminal's "Secure Keyboard Entry" on while an overlay is active (audit P3, not built): what
      the log says — nothing is written for Secure Event Input yet.

## A download opened with its quarantine flag still set (2026-09-27)

A tester unzipped the whole macOS download and the application ran, but the Installed modules
tab was empty. The likeliest cause is App Translocation: a quarantined application opened from
the Finder (Open on the context menu, or Open Anyway) is run from a read-only copy of the `.app`
alone at a random path under `/private/var/folders/…/AppTranslocation/`, so the folder the
application thought it was in held no `modules`, no settings and no room for a log.

- [x] **The folder around the original `.app`.** `portable::base_dir` asks the Security
      framework's `SecTranslocateIsTranslocatedURL` and `SecTranslocateCreateOriginalPathForURL`
      about the running `.app` and, for a translocated copy, uses the folder around the original
      for `modules/`, `settings.toml`, the log and `build-info.txt`. Both functions are declared
      only in `SecTranslocate.h` in Apple's open-source Security project and bound by no crate in
      the lock file, so they are found with `dlopen`/`dlsym` (as Dolphin and DuckStation do) and
      never linked: a macOS without them still starts. Decided by the first call, which is the
      launcher's module list, so nothing reads the folder before it; nothing in it logs or
      panics, because the log's path and the panic hook both depend on it. Any answer short of
      another `.app` leaves the folder what it was before. The rule is pure and unit-tested on
      Windows (`portable::place`); the question is type-checked by `macos-check`, and two
      macOS-only tests ask the real framework in the macOS CI job's `cargo test`.
- [x] **Said in the log.** A `translocated:` line in the header (after `executable`) and in the
      `[env]` block: `no`, `YES — …` naming the copy and the original and repeating the `xattr`
      fix, or `not known (why)`; a `no` for an `.app` under `AppTranslocation` is not believed
      and reads `YES` with that reason. A new header line `modules folder <path>: N folders`
      (`empty`, `does not exist` or `could not be read (…)`: only the state, because a start with
      module folders on the command line does not read it), and the `no modules to load` line
      names the folder it read, what was there, and for a missing one that an application moved
      away from its folder finds none.
- [x] Both macOS smoke runs in CI warn unless the log says `translocated: no` (they start the
      executable directly, so they are never translocated): the first real call of the two
      functions, on macOS 15, 14 and 26.

Checked here: `cargo test -p host -p module-manifest` (the new `portable` tests: the translocated
case, a trailing slash, an answer that is not another `.app`, a failed or impossible question
with and without the translocated path shape, a `no` under `AppTranslocation`, a loose binary,
the modules folder's states);
`cargo check --target aarch64-apple-darwin -p macos-check`, with and without `--tests`; the
workflow parses and its two changed `run:` blocks pass `bash -n`; `package-macos.sh` passes
`bash -n`. Only a Mac can show:

- [ ] **Next Mac session: open a fresh download WITHOUT removing the quarantine flag.**
      1. Download the zip with a browser and unzip it with Archive Utility
         (`xattr -l AutomationPlatform.app` lists `com.apple.quarantine`). Rename the unzipped
         folder to `Test Ü`: a space and a non-ASCII letter, the one part of the path handling
         nobody has seen on a Mac (Core Foundation may hand the original's path back
         decomposed). Move the folder into the home folder as README step 1 says: left in
         Downloads, reading `modules` beside the original would raise the Downloads permission
         dialog, which a translocated run never did before.
      2. Skip the README's `xattr` line and open the `.app` with Open on the Finder's context
         menu (on macOS 15 and later: the Open Anyway button in Privacy & Security).
      3. **First check the log's `executable` line** names
         `/private/var/folders/…/AppTranslocation/…`. By published accounts only a Finder move of
         the `.app` itself ends translocation, not a move of the folder around it; if the line
         names `~/Test Ü/…` instead, moving the folder ended it, this pass tested nothing, and
         a tester who followed README step 1 was never translocated either. Then start over
         with a new download whose zip is moved into a new folder in the home folder and
         unzipped there, so that nothing unzipped is moved.
      4. Expected: the Installed modules tab lists the shipped modules, and
         `automation-platform.log` is in `~/Test Ü` beside the original `.app`, not in
         `~/Library/Application Support/AutomationPlatform/`. The header has `translocated: YES
         — … The original is /Users/…/Test Ü/AutomationPlatform.app …`, `app folder
         /Users/…/Test Ü (writable)` and `modules folder /Users/…/Test Ü/modules: N folders`,
         and the `[env]` block repeats the `translocated` line. If it says `YES` with a reason
         instead, the reason is the finding: which function failed, with its POSIX error. If the
         original is found but no module loads, or its path looks garbled, repeat once in a
         folder with a plain name, to tell the encoding apart from the rest.
      5. Change a setting in Application settings: it lands in `~/Test Ü/settings.toml`. Quit and
         open it the same way again: a second `translocated: YES` with a different copy path and
         the same original.
      6. Quit, run the `xattr` line and open it: the setting is still there and the line says
         `translocated: no`.
- [ ] **After that, not before** (by published accounts, moving the `.app` with the Finder ends
      translocation for it for good): make a new, empty folder `Moved` in the home folder — not
      the Desktop, which macOS guards with a permission dialog (README step 1), so that its
      `modules` would read `could not be read (Operation not permitted)` — drag the `.app` alone
      into it and open it. Expected: `modules folder /Users/…/Moved/modules: does not exist` in
      the header, and `no modules to load — /Users/…/Moved/modules does not exist. Installed
      modules are read only from beside the application, so one moved away from the folder it
      came in finds none. …`. Put it back.
- [x] **A log that starts a new file within the session** (the uptime change's rotation, merged
      with this one): the new file's continuation lines repeat the header's `translocated`
      line after the build, and then the `app folder` line it refers to, so a file sent on its
      own still says why it is in the folder it is in. Rotation renames within the folder the
      log was opened in, which is the resolved one; nothing in it asks the Security framework
      again.

## The application asks its own way into the Screen Recording list (2026-09-27)

At a first setup the application appeared in the Accessibility list by itself and not in the
Screen Recording one, which a blind tester then had to add by hand (padlock, +, the .app). Built
blind, compile-checked for macOS, not yet run on a Mac (docs/macos-permissions.md, "How the
application gets into the Screen Recording list"). Screen Recording is asked for once per run by
itself, on the first tick at which Accessibility is granted — after the window and its
announcement, never on top of the Accessibility dialog (`perm::pump`) — with
`CGRequestScreenCaptureAccess` alone; on macOS 12 followed by ScreenCaptureKit's shareable
content (5 s bound, own thread) and one point through the looked-up older functions, a second
apart. The Permissions page's button climbs the same ladder one request per press
(`enrol::screen_step`, tested on Windows) and never opens the pane on a press that has just
asked — it says so instead; the press after the last request opens the pane. The log ends every
request with what is left to do, including `tccutil reset ScreenCapture <bundle id>` for an
entry left by an earlier build.

Input Monitoring now reads its own state: `IOHIDCheckAccess` for listening, through
`objc2-io-kit`. Every build before called it with request type 0, which is the posting side, so
"Input Monitoring follows Accessibility" — and the first session's "Input Monitoring reported
granted" — were Accessibility read a second way. The old reading stays in the log as `hid post
events`. Input Monitoring's button asks with `CGRequestListenEventAccess`, then
`IOHIDRequestAccess`, one per press; nothing asks for it at start. The startup block gained
`launched by`; the minimum macOS is 12.3 (ScreenCaptureKit is a required load, and it arrived
in 12.3); Info.plist carries `NSInputMonitoringUsageDescription`. What only a Mac can answer:

- [ ] **Does the application land in the Screen Recording list by itself — on the oldest and the
      newest macOS available?** Each run from a clean state for this one permission: a fresh
      user account, or `tccutil reset ScreenCapture com.automationplatform.app`. Always with the
      bundle id — a bare `tccutil reset ScreenCapture` takes the permission from every
      application on that Mac. The .app opened from the Finder or with `open` (`launched by:
      launchd`), never from its binary in a terminal.
  1. Accessibility missing too (also `tccutil reset Accessibility com.automationplatform.app`):
     one dialog at start, and the log says `screen recording: NOT granted, and not asked for
     yet`. Grant Accessibility; within about a second the log says `asked with
     CGRequestScreenCaptureAccess (now that Accessibility is granted)` — and on macOS 12 the
     ScreenCaptureKit line and the one-point line, a second apart each. Then: is
     AutomationPlatform in the Screen Recording list (the tester opens the pane and reads it
     out), and how many dialogs came up.
  2. Reset Screen Recording only, launch with Accessibility granted: the startup announcement
     says a dialog may come, then the same lines with `(at start, once the application was
     up)`, and the same two questions.
  3. On 13 or later, with the entry still missing after 1 or 2: press "Open the Screen
     Recording settings". Expected: no pane; the page says macOS has been asked; the log has
     `asking a second way` and the ScreenCaptureKit line. Did a dialog come, and is the entry
     there now? Press again: the pane opens (`settings pane: opened`).
  If the entry is missing on a version, the lines say which requests were made there and what
  each answered; if it is there, the log cannot say which of them put it there, and that is
  enough.
- [ ] **What ScreenCaptureKit answers while the permission is missing**, per version: the line
      names domain and code (expected `-3801`, "declined"), or `did not answer within 5000 ms`.
      A timeout on a version means the older functions carried the request there.
- [ ] **Two or three dialogs on macOS 12?** There the automatic request makes all three
      requests, a second apart. If the tester hears more than one dialog, the capture requests
      move to the button there too (`enrol::screen_step`). And if the documented request turns
      out to enrol on 12 now that it waits for Accessibility and for the GUI — the 12.7.6
      finding was made with a request at the same instant as the Accessibility prompt, before
      AppKit — the extra requests can go.
- [ ] **Every start while Screen Recording is switched off.** The automatic request runs again
      on each start while it is missing; with the entry present and off, does each start put a
      dialog up? (ScreenCaptureKit has been reported to prompt on every call on macOS 14.6 and
      15; the automatic request is the documented one alone there, so this is about that one.)
      If it does, remember in the settings that the request was made for this build and make it
      once — decided after measuring, not before.
- [ ] **A new build over an old, granted entry.** Every CI download is ad-hoc signed and so a new
      identity; the previous build's entry stays in the list, switched on. On the oldest and the
      newest macOS: does the new build's request prompt, change the entry, or do nothing, and
      what does the list read? The log's last Screen Recording line tells the tester to run
      `tccutil reset ScreenCapture com.automationplatform.app`: does the next start's request
      then add the entry again, or does it — as reported after "−" (forums thread 818415) —
      need the Mac restarted first?
- [ ] **Does the key tap need Input Monitoring at all?** It is created active, the kind
      Accessibility governs, and it suppressed keys in the first session (12.7.6) — whether
      Input Monitoring was granted then is unknown, because the line that said so read the
      posting side. Read the new `input monitoring` line on each Mac next to `the event tap
      suppressed its first key`: if the tap suppresses while the line reads `unknown` or
      `denied`, it is not needed, and Input Monitoring should stop being `blocking` — a
      `denied` reading puts the startup window in front of the user today. Until measured it
      stays blocking, as it was.
- [ ] **Input Monitoring's button.** With the `input monitoring` line `unknown` (or after
      `tccutil reset ListenEvent com.automationplatform.app`): press "Open the Input Monitoring
      settings". The log's `input monitoring: asked with CGRequestListenEventAccess` line gives
      the answer and the state before and after; unknown to denied means the entry exists. Did
      a dialog come, and is the entry in the list? If not, press again (`IOHIDRequestAccess
      (listen)`), same questions; a third press opens the pane. On macOS 26.6 another project
      saw the first request raise nothing and the second work.
- [ ] **`NSInputMonitoringUsageDescription`.** Added to Info.plist on the strength of one forum
      report (thread 809431) that `IOHIDCheckAccess` and `IOHIDRequestAccess` for listening
      answer `denied` without it, whatever the switch says. Unverified: if the `input
      monitoring` line reads `denied` on a Mac where the switch is on, the key did not help.
- [x] **App Nap while the request waits for Accessibility** (found when this was merged with
      the uptime change, whose App Nap activity was held only while keys are captured or a
      controller is listened to): `perm::pump` looks at Accessibility once a second, and the
      grant is made in System Settings with the application's window covered, the case App
      Nap stretches timers for. The activity now has a third reason, `activity::SETUP`, held
      from the first look that finds Accessibility missing until the request is made (by the
      pump or the button); the first hold is written per reason, so the first overlay's `…
      while keys are captured …` line is still written in full. After the merge's review:
      - that reason alone begins the activity **without `LatencyCritical`**
        (`UserInitiatedAllowingIdleSystemSleep` only): it is held for as long as Accessibility
        is missing — the whole session when it is never granted, as after an ad-hoc rebuild —
        and a look once a second needs App Nap kept away, not the most accurate timers.
        Captured keys or a controller make it latency-critical, and releasing them makes it
        plain again; the new activity is begun before the old one is ended;
      - a reason that joins an activity already held gets its line too (`… (already held while
        …)`), so a module that captures keys at load no longer hides the setup wait's line, and
        a controller or the setup wait holding it first no longer hides the first capture's;
      - the controller's `activity` status names what still holds the activity instead of
        always saying keys.
      The rules are in `activity_reasons.rs`, pure and tested on Windows; `activity.rs` begins
      and ends what they decide.
- [ ] **Does the request follow the grant within about a second, napped or not?** With
      Accessibility missing, leave the application's window covered by System Settings for a
      minute or more before switching Accessibility on. Expected: `App Nap: holding an
      activity (not latency-critical) while Screen Recording's own request waits for
      Accessibility to be granted` near the start (`a latency-critical activity … (already held
      while keys are captured)` when a module captured keys at load), Activity Monitor's Energy
      tab reading App Nap: No while it waits, the `asked with CGRequestScreenCaptureAccess (now
      that Accessibility is granted)` line within about a second of the switch, and App Nap:
      Yes again some minutes later with no overlay active. If the request comes late while the
      activity is plain, `SETUP` belongs among the latency-critical reasons
      (`activity_reasons.rs`, `LATENCY_CRITICAL`).

## Unnamed pass-through stops, read by OCR (2026-09-27)

A Mac tester's log of Kontakt 7 standalone had the pass-through's ring mostly made of stops with
no name (`[passthrough] Kontakt 7: Tab -> stop 15 of 29 '', …`; FILE, LIBRARY, VIEW, SHOP,
Search, Brand, Sound Type and Character had one), so the screen reader said a role and little
else. Built, and tested against the scripted host (`overlay_passthrough_tests.rs`), not yet run
on either system: `host.element.focusStep` hands back the landed element's `bounds`, and
`Overlay:addPassThrough` (`readUnnamed`, on by default) reads the rectangle of a stop whose name
has no visible character with `host.ocr.read`, cut to the plug-in window, and says its first two
rows queued (`interrupt = false`) — only while nothing has moved on, with no timer. One
`[passthrough] … (type N) has no name; …` line per such stop says what it read and what became
of it. Named stops are unchanged. What only the real systems can answer:

- [ ] **Windows, NVDA, Kontakt 7 and 8 standalone.** Walk "Kontakt controls" once each: which
      stops have no name there at all (the `-> stop N of M ''` lines — the Mac count need not
      hold on Windows, whose ring is scoped to the content area), what each reads as and its
      `(type N)`, and what is heard: NVDA's own announcement first and the text after it, or the
      text cut off. The read can answer before NVDA has handled the focus event (a small
      element's read answers in a few tens of milliseconds), and whether NVDA's announcement
      then cancels the queued line is unknown. If it does, the answer is found in the order of
      events, not in a delay. Also listen for a value said twice: an unnamed edit field or combo
      box whose value NVDA announces itself, and whose text the overlay then reads out again.
- [ ] **Are the rectangles where the element is drawn — UIA's on Windows, Accessibility's on the
      Mac?** Switch on **Calibration keys and pictures in overlays** under Installed → Overlay
      runtime → Settings… (no reload); walk a few unnamed stops, and on one of them press the calibration
      shot key — Ctrl+Alt+Shift+S, Command+Option+Shift+S on a Mac — which saves a picture of
      the plug-in window into the module's `calibration/` folder. Send that PNG back with the
      log's `read at x,y wxh` lines: the regions are compared against the picture here, which
      cannot be done by ear. A stop that logs `no rectangle` is one the platform gave no
      rectangle for; one that logs `outside the plugin's window` has a rectangle, and it lies
      outside the window — a row scrolled out of view, say.
- [ ] **Does a list scroll when the focus reaches one of its rows, and is the row read where it
      is?** On Windows `bounds` is read once, right after the focus has landed, so a scroll the
      list animates after that is not in it; on a Mac `bounds` is from before the focus moved,
      and the ring admits rows below their list's viewport (the Mac mini session above: 16 of
      2708 presets exposed). On both systems, in Kontakt 8 standalone, Tab into the preset list
      and on through its rows until the stops leave the list, and send the log. A row read
      where it was, not where it is, shows as a `read at` region at the row's old place, a
      spoken text that is another row's or another control's, or a line that ends `outside the
      plugin's window`. That decides whether `bounds` needs a viewport cut — the macOS
      `focus_step` viewport filter in the Mac mini item above, and the same on Windows if its
      lists show it too.
- [ ] **The next Mac session, VoiceOver, Kontakt 7 standalone.** The order and the overlap of
      the two announcements: with Speak through VoiceOver off the text is the system voice, a
      second voice that can talk over VoiceOver's announcement of the element; with it on,
      whether VoiceOver queues it behind its own is VoiceOver's decision. Both ways, once,
      noted by ear.
- [ ] **What a large unnamed element reads as** — a whole list, a browser pane, a scroll area.
      Two rows of a list may be noise rather than a name, and a host row joins what stands side
      by side, so two rows of a wide element can be a long line said on every landing. The
      `(2 of N rows)`, the region's size and the `(type N)` in the log say how much there was
      and of what kind. Whether that calls for a limit on length, size or role is decided on
      the log, not in advance.
- [ ] **A key that goes to the plug-in, straight after Tab.** Space or an arrow key on an unnamed
      stop, pressed before its text is heard: the overlay does not see such a key, so the text
      read before it can be said after it — after a dropdown's list has been announced, or with
      the value it had before the arrow. Note whether that happens and whether it misleads.
- [ ] **The cost, and Tab held down.** The `in N ms` of every outcome line, on both systems, for
      a button and for a list. With Tab held down, the older reads end as `not spoken: a later
      step asked for another read`, as `not recognised, a later step's read replaced it`, or —
      when the next stop has a name, so no newer read was asked for — as `not spoken: a later
      key moved on`; all three are right, and nothing is said about a stop already left. What
      the log cannot show is the speech queue: the checks are made when an answer arrives, so a
      line accepted then waits behind whatever is being said and is heard after any Tab that
      came in between, with nothing to take it back. Listen for text heard a stop or two late.
      If it is, what would fix it is general, not the overlay's: a way to have a queued line
      dropped when a newer one of the same kind is said — a host speech question, to be
      decided then.

## Plug-in modules in every DAW entry (2026-09-27)

The maintainer's decision of that day: what a DAW is — which of its windows are plug-in windows,
its own chrome, and on a Mac where a plug-in starts inside its window — lives in daw-hosts entries
and nowhere else, so adding a DAW is adding its entries and every plug-in module is then
recognised in it; no plug-in module names a DAW or needs the window's title to name the plug-in.
Built, reviewed on 2026-09-28 and corrected after that review, and tested against the scripted
host (`crates/host/src/overlay_host_panel_tests.rs`, 30 scenarios: the runtime's resolution, the
gate and the stay, and daw-hosts, sforzando and Kontakt loaded for real in a REAPER window floated
out of its chain, a REAPER FX chain and a Logic window titled "Inst 1"); not yet run on either
system.

- [x] **The entry** (`modules/daw-hosts`; docs/daw-hosts.md): a window matcher with `chrome`,
      `pluginOrigin` (a function per platform — macOS for REAPER and Logic), `focusTarget` and
      `daw`. The focus key searches the entries of `all` that can match on the platform it runs
      on — the list the overlays bind with; its own list (`pluginWindows`) and the exports
      `reaperPluginOrigin` / `logicPluginOrigin` are gone. On Windows it therefore also finds
      REAPER's bridged plug-in windows and Ableton's VST2 and `#32770` plug-in windows, which its
      own list did not know; REAPER's `#32770` entry keeps the key off REAPER's other dialogs by
      the plug-in windows' titles (`focusTarget`). Its origin is asked only on a readable chain,
      kept per window and size, and one that fails is said ("could not tell where the plugin is
      in …"), not clicked on and not kept. REAPER's origin takes the FX list by where it is (the
      leftmost surface left of 240 that does not span the window), and answers nothing — not the
      authored 240 — when the host could not read the window's surfaces.
- [x] **The DAW's plug-in panel** (`modules/overlay-runtime`, attachEmbedded;
      docs/api/overlay.md): a binding with no control of its own for the platform — no entry, or
      a pattern of which no control matched and passed `identify` — takes the panel built from
      the window's entry's `pluginOrigin`, when it has an `identify`; otherwise it is inert and
      one line says why, and an entry without an origin says so once when its window is in
      front. `false` for a platform declines even the panel; a function entry is handed it.
      Geometric gate, on a chain that has to end in the window in front (and the origin is not
      asked for on one that does not, or is empty). A verdict on a panel is kept for a STAY (the
      keyboard inside one window's panel, until it is seen anywhere else), per binding value and
      with the overlay named in its line, and a watcher per VM so an overlay outranked on its
      slot cannot bring back a verdict from before a visit to the FX list. A binding that joins
      an overlay already bound is evaluated at once. Windows is unchanged for the overlays: no
      entry has an origin there.
- [x] **The modules.** sforzando: one embedded binding for both platforms; the macOS-only
      "sforzando in a DAW" overlay, its REAPER bundle-and-title matcher, its frame from REAPER's
      geometry and its focus-chain-depth gate are gone; on a panel its wordmark is read off the
      event loop, and a read that saw nothing is no answer. Kontakt: the in-DAW cells' Mac control
      is the panel, moved to Kontakt's own corner by its FILE button with LIBRARY beside it (or
      Kontakt 7's header text) within 24 points of where its geometry puts it — Kontakt 8's own
      "Kontakt File Menu" within 110 across — no title check, with the relational checks as
      before; a miss is asked again up to eight evaluations of a stay. Content Missing and the
      in-Komplete-Kontrol cells are declared absent on a Mac (`macos = false`). Komplete Kontrol,
      u-he, Melodyne, ARC ON:EAR and the Kontakt library modules named no DAW and are unchanged.
- [ ] **sforzando in a DAW on a Mac — the wordmark, never read there.** The binding used to be
      recognised in REAPER by the window's title; now by a `PlogueXMLGUI` pane (nothing of
      sforzando's is published on a Mac, so in practice not) or by the "sforzando" wordmark read
      off the event loop at 597..770 x 29..74 of the panel — the Windows region (605..762 x
      33..70) grown by 8 and 4 points for the Mac build, which moved the read-outs by up to 7 and
      4. At the next session, sforzando in REAPER (chain and floating) and in Logic: the
      `attachEmbedded: [host-panel] panel of '…' … identify=true — 'sforzando'` line, the overlay
      coming up and its read-outs landing. `identify=false` with sforzando plainly on screen means
      the region misses the wordmark: send the log and a screenshot of the window, and the region
      is measured from it.
- [ ] **Kontakt 8 in a DAW on a Mac** is recognised only when it publishes a button named "Kontakt
      File Menu" within 110 points across and 24 up or down of the authored {86,19}, which was
      written as a click point, not measured as a centre; otherwise `publishes no button named
      Kontakt File Menu — no Kontakt 8 there yet`, once per window, and no overlay. Check with
      Kontakt 8 in REAPER and in Logic, and note the anchor line's corner: its first number is
      how far the authored point is from the wordmark's centre, which every header coordinate of
      Kontakt 8 on a Mac is then off by.
- [ ] **The stay, live.** In REAPER's FX chain with sforzando and another FX: select the other FX
      in the list and Tab (or Cmd+Shift+F6) back into the plug-in — the overlay must not come up
      over it (one `identify=false` line); select sforzando again — it comes up. In Logic, change
      the plug-in shown from the window's header, if Logic's window can do that. Whether moving
      between the list and the plug-in fires the focus notifications the stay is ended by. And
      the same switch done quickly — Shift+Tab, Down, Tab in a second, with Kontakt and its
      libraries enabled so the event loop is busy: a focus change on a Mac only marks the focus
      dirty and the next round reads the chain as it is then, so a visit no round samples keeps
      the old verdict over the new plug-in (the runtime's "the stay" says so). If that happens,
      the fix is a general one in the host — counting the notifications merged into a round —
      not a timer.
- [ ] **A fresh plug-in window on a Mac: the first verdict.** Open sforzando and a Kontakt 7 in a
      new window and go straight in: the first `identify=` line should say `true`. A "no" that
      comes before the plug-in has painted is kept for the whole stay — only nothing-read is
      asked again, and Cmd+Shift+F6 with the keyboard already inside is one more evaluation, not
      a fresh stay. If a wrong first "no" shows up, what would undo it is a general "look again
      afresh" round (a `host.window.recheck` that ends every stay), which nobody has needed yet.
- [ ] **A DAW origin more than 19 points too low hides Kontakt's top row.** The runtime gates the
      keyboard on the DAW's panel as well as on Kontakt's own corner, so with Logic's estimated 88
      too far down, the keyboard on FILE or LIBRARY reads as Logic's header and the overlay drops.
      Kontakt's anchor line gives the corner: a second number below -19 means the origin in
      daw-hosts is what to correct.
- [ ] **What the evidence costs in a window that is not the plug-in's.** With no title to go by,
      every plug-in window in every DAW is asked once per stay, in Kontakt's VM and each library
      module's (five today): an accessibility search for FILE (and LIBRARY beside it) and one for
      "Kontakt File Menu", up to eight times while they find nothing, and header reads off the
      loop; sforzando one wordmark read off the loop. Look at the `[recheck]` and `[poll]` lines
      around a return into sforzando or VPS Avenger with Kontakt and its libraries enabled.
- [ ] **The focus key on Windows, with NVDA.** It now searches every entry, so it also finds a
      bridged REAPER plug-in window (`REAPERb32host`), Ableton's VST2 plug-in windows and any
      `#32770` of Ableton's process. Ctrl+Shift+Win+Alt+F6 over an Ableton VST2 plug-in window,
      over a bridged REAPER one, and in Ableton with a file dialog open and no plug-in window:
      which window it takes and what it says (for the last, the dialog's title, which the entry
      allows because a plug-in's own dialogs are such windows).
- [ ] **sforzando in Ableton Live on Windows.** Every page now says the `Plugin<hex>` class is one
      several vendors' plug-ins share (ReaHotkey matches sforzando, Engine 2 and Zampler by it, in
      REAPER and in Ableton); this platform has only ever seen it in REAPER. Check that the
      overlay comes up over sforzando in Ableton, which is what makes a Windows DAW entry need no
      more than a matcher and its chrome.
- [ ] **Modules that would bind a panel on a Mac only by override.** u-he's binding has no
      `identify`, so on a Mac it would stay inert by design (its window class was its whole
      identity); if u-he is ever ported it needs one that answers on the panel. Komplete Kontrol's
      `HOSTED` binding, with "Load modules not meant for this system", takes the DAW's panel on a
      Mac when its `identify` finds a "Komplete Kontrol" element in the window, with coordinates
      measured on Windows — nobody has seen what Komplete Kontrol publishes inside a Mac DAW
      beyond the third session's "nothing".
- [ ] **Logic's origin line repeats.** `[daw-hosts] Logic plug-in window '…': the plug-in starts
      at …` comes once per window and size in every module that binds with `daw.all` —
      sforzando, Kontakt and each of its library modules — identical each time, and once more
      from the focus key. Expected: daw-hosts' own VM evaluates no overlay, so a line limited to
      it would come only when the key is pressed.

## VPS Avenger, stage 1: the runtime's building blocks (2026-09-28)

The maintainer's decisions of that day: our own module for VPS Avenger, every feature the
avenger_control project's tool had, and stage 1 now — the header (preset name, previous/next,
MENU with Load, Save, Save as and Initialize, Undo, the redo list, zoom) at every zoom, in every DAW
daw-hosts knows. Its general parts live in the overlay runtime, built and tested against the
scripted host (`crates/host/src/overlay_scale_tests.rs`, 16 scenarios;
`crates/host/src/overlay_menu_item_tests.rs`, 24), and documented in docs/api/overlay.md; nothing
of it has run on either system.

- [x] **Coordinate scaling** — `O:scale(fn, { about })`, `O:toScreen` / `O:toScreenRect`. One
      formula for every authored coordinate: `at` (table or function), `points`, `region` and
      `ocrLabel` (both may now be functions of the overlay), `fromRight` (a distance from the right
      edge), tab points, `reveal` probes, a slider's `from`/`to` and step, a graphical button's
      `clickOffset` and fixed `dragBy`, a menu item's offset, calibration crosshairs.
      `rawOrigin` is neither framed nor scaled. The frame is unscaled and composes first. A
      factor the module cannot tell yet (`nil`, a raise, not a number above 0) places nothing and
      says so ("… is not available now" — a slider's arrow keys too — "cannot be read now");
      logged when it changes. An overlay without it is placed exactly as before — no rounding (a
      scenario holds that). Every kind of coordinate above has a scenario of its own.
- [x] **ARC ON:EAR on the runtime's scale.** `geometry.luau` is now the model put on each of its
      four overlays (a frame of half the window's width, a factor of height / 1009 about (960, 0));
      every `scaled(x, y)` is a plain `{x, y}`, its own clicks and reads go through `toScreen` /
      `toScreenRect`, and the chooser rows — points worked out from the tree — are `rawOrigin`.
      The runtime's arithmetic is its old arithmetic in the same order; the tests hold every design
      point it clicks (29, in all four overlays, over 429 window sizes and places) and every point
      and rectangle its own code placed (fractions of a pixel included) to the old formula.
      `supported_os` stays windows.
- [x] **Choosing an item in a plug-in's own menu** — `menuItem` on `addHotspotButton` (with `button
      = "right"` for a menu a right click opens), and `O:chooseMenuItem(spec)` for a module's own
      code (a stepper over Avenger's zoom list). The opener is placed and checked, then counts as a
      press, then is clicked; the item is clicked on the answer of a menu test that sees the menu
      and says where it is (`newWindow` answers with the largest window that appeared: `{ kind =
      "window", id, window, x, y, w, h, … }`), at its offset from the menu's corner — a table scaled
      at the factor the opener was placed with, a function's answer in screen pixels (`false`: not
      this window, the next answer) — inside it, and with `host.window.ownsPoint` asked of the
      menu's own window: by its HWND on Windows, by its window-list number on a Mac (`ownsPoint`'s
      new `listed` option). Chosen only once the menu has taken it: the same menu still there on a
      later tick is clicked again, up to `retries` more times (default 3, `retries = 0` for a menu
      that stays open), then said ("… the menu did not take the choice"). A second press of the same
      control while its menu is awaited clicks nothing. While an item waits every menu test is
      asked, a cheap one that sees the menu holding back none. Nothing clicked, logged and said when
      no test saw a menu within the 8 s the tests run after a press, when the menu does not say
      where it is, when the function took no window for its menu, when the item falls outside it,
      when it is covered, or when it cannot be placed; dropped quietly on an own key, leaving the
      front, another press or another window. In a calibrating run the menu is photographed with a
      crosshair on the item (`…-menu-item.png`), and the calibration shot marks an opener a
      stepper's `chooseMenuItem` clicks (`[menu opener]`).
- [x] **Small building blocks the module needed** — `O:menuOpen()` (whether a menu counts as open,
      for closing a popup a choice left open); `O:resume(false)` (every activation starts at the
      first control: a box drawn inside a plug-in comes back on the same window); `pollWhen` on a
      binding with `pollMatch` (the poll rechecks only while the module expects a change); a
      stepper's `onStep` returning `false` (a press that moved nothing says nothing of its own),
      and its `settle` read at each step, documented.
- [x] **Documentation** (docs/api/overlay.md): `O:scale`, `O:toScreen`, `O:chooseMenuItem`,
      `menuItem`, `retries` and `button`, region functions, what a menu test's answer can say,
      `O:menuOpen`, `O:resume`, `pollWhen`; and what stage 1 uses that had no entry:
      `O.memoByEpoch`, `O.contentSize`, `O:onActivate` / `O:onDeactivate`, `addOCRButton`'s `text`,
      `fallback` and `when`, `addHotspotButton`'s whole option list. docs/api/window.md:
      `ownsPoint`'s `listed`. The stale runtime comment that `ownsPoint` was not implemented on
      macOS is corrected.
- [ ] **ON:EAR, live on Windows.** The tests hold the arithmetic; nothing has run. The main
      window, one chooser and the settings panel at two window sizes (maximised and a small one):
      the calibration shot (Ctrl+Alt+Shift+S) of each, crosshairs where they were before, and Tone,
      Width and a tile press still landing. Its log gains `[scale] 'ARC ON:EAR': factor …` lines,
      one per resize. One change that is not arithmetic: with no height at all (a window
      minimised to nothing while in front) a press now says "… is not available now", where the
      old `at` function answered nil and the press said nothing.
- [ ] **Avenger's popups against the item choice — Windows.** Is the MENU popup a window of the
      DAW's process that `newWindow` sees (the `newWindow: a window appeared …` line and the menu
      shots)? Is the largest new window the menu, or does JUCE's drop shadow come out larger (it
      should not: strips beside it)? How many clicks the item needs: the log's `clicking (x,y)
      again, n of 4` lines. JUCE's quarter of a second after its popup opens is from its source as
      read for this, not measured against the JUCE Avenger ships; if JUCE ignores more than three
      clicks, the count is too small — detection all the same, never a wait. `ownsPoint` against
      the popup's HWND: `true` expected.
- [ ] **Avenger's popups — macOS, in Logic and in REAPER.** The same questions, plus: at which
      window level; whether the popup becomes the application's focused window (then the overlay
      holds its place over it, as tested); whether a tooltip appears under the parked pointer first
      (an item outside it, or a window the module's function passes over, waits for the next
      answer); what `ownsPoint` with `listed` answers for the popup's own number — `true` expected,
      never run.
- [ ] **Template images do not follow `O:scale`.** A graphical toggle's, graphical button's or
      slider's templates and a landmark are matched at their own `scales`. Stage 1 matches no image;
      a later stage that does, at a zoom other than the one the template was cut at, needs its
      `scales` worked out from the factor.
- [ ] **Stage 2 documentation, before Avenger's later stages rely on it.** Undocumented in
      docs/api/overlay.md and not used by stage 1: `addTabControl` (a tab's `at`, `hotkey` and
      `hotkeyLabel`; `typeLabel`, `current`, `verticalStep` / `verticalName`,
      `hotkeyKeepsFocus`, `onSelect`) and `gotoTab` / `cycleTab` / `switchTab` /
      `stepTabVertical`; `addSlider`; `addOCREdit`; `addGraphicalButton` (`dragBy`,
      `clickOffset`, `state`, `unavailable`, `rawOrigin`); `O.stateGeneration`; `O:adjust`; the
      calibration helpers `captureControl`, `captureRegion`, `captureFull`, `captureAll`. A tab
      control whose tabs are read off the screen (the analysis's C1) is stage 2 work as well.

## VPS Avenger, stage 1: the module (2026-09-28)

The module itself, `com.platform.vps-avenger` (`modules/vps-avenger`), on the runtime's building
blocks above: VPS Avenger's header at every zoom from 50 to 200 %, in every DAW daw-hosts knows,
from the avenger_control project's coordinate tables, taken as starting values with permission and
with that source named in `geometry.luau`. Built and tested against the scripted host
(`crates/host/src/overlay_avenger_tests.rs`, 20 scenarios: a REAPER window floated out of its chain,
a REAPER FX chain and Logic windows titled "Inst 1" on a Mac, Avenger's JUCE child in REAPER on
Windows at 100, 110, 125 and 150 % display scaling); nothing of it has met a real Avenger. The Mac
tester's stage-1 instructions are a text of their own, not in the repository.

- [x] **The module.** Bound with `daw.all`: on a Mac the DAW's plug-in panel, on Windows Avenger's
      own child window by the shape of a JUCE class (`^JUCE_%x+$`). Which plug-in and which zoom
      are one read of Avenger's header off the event loop (`header.luau`): MENU and UNDO in
      capitals, in boxes the recogniser located, in the table's proportion from Avenger's corner,
      and a factor that fits them say it is Avenger (never the window title; half an answer is read
      again, never a "no"); the zoom field's caption — left of where the preset's name begins — is
      the zoom, and the factor is zoom / 50 times a display factor: 1 on a DAW's panel, and on
      Windows what the header's own size gives (1.25, 1.5 …); an unread caption leaves the size's
      step on a panel and, on Windows, a factor with no zoom, which the Zoom control will not step
      from. A new size of the child window the module holds as Avenger's (a zoom step on Windows)
      is Avenger's still. Avenger's corner is the panel's + `ORIGIN_DIFF` (-2, +2), on a DAW's
      panel only. The ring: Preset (read; spoken on arrival), Previous and Next preset (the new name
      once it is read on screen, "unchanged" when it is not, and never a name before the watch is
      over when nothing was read before the click), Load, Save, Save as and Initialize (chosen in
      Avenger's MENU once `newWindow` sees it and counted once the popup took it; Save and
      Initialize refused, and said, where the warning box would open off the screen), Undo, Redo
      list (a right click; the keys are the list's while it is seen), Zoom (a stepper over
      Avenger's zoom list, which says the zoom the field then reads, and sends nothing while a step
      is under way). A popup a choice left open is closed with a second click on its opener.
      Avenger's warning box after Initialize or Save is an overlay of its own at the dialog layer
      (`dialog.luau`), looked for only while expected (`pollWhen`), starting on its own text every
      time (`resume(false)`): its text, YES and NO clicked where they are read, given up only on two
      readings without them. A stay that the module's own popup or zoom step ended keeps the
      overlay in front — once, and at the size it was read at unless the zoom list opened. In a
      calibrating run: `modules/vps-avenger/calibration/header-<zoom>pct-<w>x<h>.png` (where the
      overlay clicks the zoom field, ◀, ▶, MENU and UNDO, and where the words were read) and
      `dialog-<choice>-<zoom>pct.png`, beside the runtime's calibration, menu and menu item shots.
- [ ] **Stage 1 on the Mac tester's Avenger** — asynchronous: a CI build and written instructions,
      and the tester sends back the log, both calibration folders and his notes. With
      **Calibration keys and pictures in overlays** ticked under Installed → Overlay runtime →
      Settings… (no reload):
      1. **Logic**, Avenger at whatever zoom it is at: into its window, Command-Shift-F6. Heard:
         "Preset, <name>". Log: `attachEmbedded: [host-panel] panel of 'Inst 1' … identify=true
         — 'VPS Avenger'` and `[avenger] 'Inst 1' (<w>x<h>): VPS Avenger, the zoom field reads
         <z> % — zoom <z> % (read), factor …, display factor 1.00. MENU read at (…), UNDO at (…):
         (dx,dy) and (dx,dy) from where the table puts them …`. Picture:
         `header-<z>pct-<w>x<h>.png`. Then Command-Option-Shift-S: `VPS-Avenger.png` and
         `VPS-Avenger-clean.png` in the runtime's folder.
      2. Tab once round the ring: Preset, Previous preset, Next preset, Preset info, Load preset,
         Save preset, Save preset as, Initialize preset, Undo, Redo list, Zoom. Preset info is there
         because the build ships the preset database (`modules/vps-avenger-presets`, below);
         without it the ring has the other ten.
      3. **Next preset**, Return twice, then **Previous preset** once: the new name each time,
         `[watch] VPS Avenger: changed after …`. An "unchanged" at the first press is the swallowed
         first click (below). With the database: the name as before when it is the neighbour the
         database expected, and the database's name after it, marked "from the database", when
         the read failed or disagreed; the lines `[avenger] 'Next preset': the database's
         neighbour: '…'` and `read '…', the database's neighbour exactly` (or `… not the
         database's neighbour`) show whether Avenger steps through the catalog's order. Then
         **Preset info**: the expansion, category, oscillators and macros, each marked.
      4. **Zoom**, Left one step at a time to 50 %, then Right back: "Zoom, 75 percent, slider" and
         so on, and `[avenger] 'Zoom': 80 % to 75 %, entry 6 … at y … by the list's height, … by
         the table`; the header picture at each zoom; `VPS-Avenger-Zoom-menu-item.png` (the first
         two steps). At 50 %: what "Preset" says, and whether the overlay comes up again after
         Command-Tab away and back and Command-Shift-F6 — a new stay, so the header is read at
         50 % (OCR at 50 %, below).
      5. **Load preset**: the file dialog opens; Escape. `VPS-Avenger-Load-preset-menu-*.png`, and
         `[avenger] 'Load preset': MENU's popup at (…) … from Avenger's corner in 50 % units`.
      6. **Initialize preset**: "Avenger asks, Warning, …" (every box starts on its text); Tab
         twice to **No**, Return; the header is back on "Initialize preset". `Avenger's warning box
         is up … YES read at …` and `dialog-Initialize-preset-<z>pct.png`. The same with **Save
         preset**, **No**.
      7. **Undo**; **Redo list**, then Down and Up — does VoiceOver read the list? — and Escape.
      8. **REAPER**, a floating window and an FX chain: 1, 3 and one zoom step there and back.
- [ ] **The origin difference (-2, +2)** (`geometry.luau`, `ORIGIN_DIFF`): the (dx, dy) of the
      header lines at two zooms in Logic and in REAPER settle it. The same at both zooms and in
      both DAWs is an error in daw-hosts' origins, which Kontakt and sforzando on a Mac panel share:
      it moves into daw-hosts' Logic and REAPER entries and out of this module. Growing with the
      zoom is the project's tables: the difference goes into the tables at 50 %, scaled, and then
      on Windows too — Avenger's own child window, which today gets none; the Windows log line
      already gives the offsets with and without it. Near 0 is the constant right. It answers
      Logic's 88 against the project's 90, and REAPER's 22. The other way, if the two do not
      settle it: anchor Avenger's corner on where MENU and UNDO are READ, with the factor known,
      as Kontakt anchors on its FILE button — and keep the constant only for the first read's band.
- [ ] **OCR of Avenger's header and name at 50 %.** The project's recogniser read "Init Preset" as
      "hit Presst" at 50 %; the tester's good reads were at 80 %. A MENU or UNDO the recogniser
      does not read (one wrong character is accepted) leaves the overlay down at that zoom — the log
      says `not VPS Avenger — no MENU was read` or `… — read again`; a zoom field it does not read
      leaves the zoom `seen from the header's size, not read`. On Windows, captions only in the
      neural fallback's shared-out boxes are no evidence (`read only in boxes the recogniser
      guessed`) and are read again. If 50 % fails, the header band is read enlarged, or the zoom
      told apart some other way — decided on the pictures.
- [ ] **A misread zoom caption on Windows.** On a DAW's panel a caption misread as another step
      never fits (the display factor is 1). On Windows the display factor is learned from the
      header, and a misread can land on a quarter step that fits within 3 %: 60 % read as "50%" at
      150 % display is 1.8 against 1.75. The factor is then 3 % off and the zoom wrong. A host
      primitive for the display scaling a window is drawn at (a general one, `GetDpiForWindow` on
      Windows, 1 on a Mac, where coordinates are points) would make the display factor known there
      as it is on a panel. Build it once an Avenger on Windows shows the case.
- [ ] **Avenger's popups as windows** (MENU, the zoom list, the redo list): the `newWindow: a window
      appeared …` lines; whether one comes to the front (`came to the front after …`: the overlay
      holds its place over it); how many clicks an item took (`clicking … again, n of 4`); whether
      the zoom list's row by its own height agrees with the table's row (the two numbers of the
      `'Zoom'` line, within 3 points); whether MENU's popup opens below MENU. Then the items are
      placed from the popup's corner (the runtime's item above) and the placement from Avenger's
      corner goes. Whether the redo list answers VoiceOver's and NVDA's arrow keys with the keys
      handed to it. **Whether Logic runs Avenger in a process of its own**: `newWindow` lists the
      windows of the process in front at the press, so if Logic hosts the plug-in apart, its
      popups belong to that process and every MENU, zoom and redo press ends with "no menu was
      seen" — the `newWindow: a window appeared` lines present in REAPER and missing in Logic answer
      it; the tester text asks for that sentence.
- [ ] **A popup a choice left open is closed with a second click on its opener.** The project
      closes the zoom list that way; that MENU's popup closes the same way is JUCE's modal popup,
      not measured. The `[avenger] '…': its popup was left open (…) — clicking its opener again`
      line and the `[menu] … the menu has closed` after it show whether it did.
- [ ] **The zoom list at high zooms on a small screen.** At 150–200 % on a MacBook the list may not
      fit below the field. Moved, it is placed by its own height; cut short with scroll arrows it
      is refused (`cut short, so it scrolls; nothing chosen`) and closed, which leaves no way down
      from that zoom through the overlay — measure first, then read the list's visible rows.
- [ ] **The warning boxes.** Inside Avenger, as the project says, or windows? Initialize's YES and
      NO against the table (the offsets in the log line). Save's box: whether Save always opens
      one (and not a file dialog for a factory preset), its words and its YES and NO. A box that is
      a window leaves the header holding its place over it with no keys and would have its buttons
      refused as covered; the box overlay then needs the box's window as its origin.
- [ ] **A warning box off the screen.** Save and Initialize are refused when the box's band, placed
      at the zoom, reaches past the primary display (`would lie at …, past the screen's …`): the
      host names no other display's size, so a window on a second display is not checked at all,
      and one reaching down onto a display below the primary would be refused wrongly. Whether the
      box is drawn in Avenger's middle, and how far down, is the project's figure only.
- [ ] **A DAW window narrower than Avenger.** The header band is sized by Avenger's width at the
      panel's size, its width or its height in Avenger's proportions (871 by 561 at 50 %, the
      height from the tester's log, not measured on its own). Whether any DAW keeps its plug-in
      window narrower than a large Avenger, and what the band then misses, is not measured.
- [ ] **Load's and Save as's file dialog on a Mac.** A window of its own, which the overlay leaves
      the front for (or holds its place over with no keys, while `newWindow` still compares
      against the MENU press) — or a sheet on Logic's plug-in window, whose fields the runtime's
      geometric gate would read as inside the plug-in's panel, so the overlay would keep Tab and
      Return over the dialog. Which of the two decides whether the gate has to tell a sheet apart —
      the runtime's question, for every module on a panel, not this module's.
- [ ] **Logic's keyboard with Avenger in front**: whether Tab, Return, Space, Left and Right reach
      the overlay in Logic's plug-in window at all (the `[keys]` lines); whether Logic takes the
      first click after it comes forward for itself (Next preset says "unchanged" the first time);
      Link mode — a window switched to another insert in place should end the stay by the visit to
      Logic's header or by its new title, and the header read then says `not VPS Avenger`. A stay
      carried over after our own popup that shows another plug-in after all is overturned by two
      reads (`not VPS Avenger after all`), but the runtime keeps a panel's verdict for the stay:
      the overlay stays up, placing nothing, until the stay ends.
- [ ] **Windows, if an Avenger is ever there**: REAPER's FX chain and floating window, Ableton.
      Whether Avenger's editor is a `JUCE_<hex>` child and the only one (the calibration shot's
      roster), the header line at 100 and 150 % display scaling (`display factor 1.50`) and its
      offsets with and without the (-2, +2), and NVDA in the redo list.
- [ ] **◀, ▶ and the redo list's right click are the module's own clicks**, with the runtime's
      checks repeated (placed through the frame and scale, inside the plug-in, `ownsPoint`),
      because a hotspot has no hook for what its click changed or that it opened a popup. A hotspot
      that says what its click changed once it has (a read-back through `O:watch`) would be a
      runtime building block for every plug-in; lift it when a second module needs the same.
- [ ] **Hotkeys.** Stage 1 has none: which keys Logic and REAPER leave to a plug-in window is not
      measured. Previous and next preset on keys first, once the Logic keyboard question is
      answered.
- [ ] **A zoom chosen directly.** The project's tool sets a zoom in one action (`set_zoom`); the
      Zoom control steps one entry at a time, so 50 to 200 % is twenty steps. Home and End, or a
      first step to 100 %, choose that entry of the list in one pick.
- [ ] **Stage 2 — oscillators and macros.** Runtime, documented first: the tab control with tabs
      read off the screen (the documentation item above), coarse steps on a stepper (Page Up and
      Down), a right-button double-click, a public "focus this control", a toggle by the share of
      matching pixels in a box (`host.screen.cells`). Module: the OSC tabs (their count from the
      green dots, their names by OCR), mute and solo, the tab menu; the knobs as steppers — a short
      drag per step, Avenger's double-click reset on Return — with their **values always in the
      ring** (the maintainer's decision), so the pointer-angle reader is built (a runtime building
      block over `snapshot` and `pixels`) whatever OSARA's and Logic's parameter lists offer; the
      macros (tabs, three knobs and two buttons each, names by OCR, the reset-mode and button
      menus). Renaming presets, oscillators and macros with `typingWhen` once Avenger's editor can
      be detected, and only after measuring whether `host.input.text` or a letter at a time through
      `host.input.send` reaches a JUCE editor at all (the project found neither synthetic keys nor
      the clipboard arrive).
- [ ] **Stage 3 — drums, the sample editor and Drum SQ**: twelve slots with their columns and menus,
      the kit, the sample editor's tabs, knobs and menus, Drum SQ's length, speed, patterns and
      menus, and the playing pattern said as it changes, on a poll cadence while the Drum SQ tab is
      seen. The sample editor sits low (y 407 at 50 %), off a small screen above about 70 %.
- [ ] **Stage 4 — the expansion browser**: a list control in the runtime (items from a function,
      first-letter jumps, "n of N"); expansions, categories and presets read by OCR and clicked by
      the word read, not by the project's tables for one library's size; the wheel, measured;
      **free-text search in a native field from `host.gui`** (the maintainer's decision).
- [x] **The preset database** (the maintainer's decisions of 2026-09-28): the module may orient
      itself on the avenger_control project's database, as the project's tool does, and says
      whatever it did not read off the screen marked "from the database". The developer has
      given the database without restriction. `tools/avenger-presets/convert.py` (stdlib Python, its
      own tests beside it) turns the project's three exports into the data module
      `com.platform.vps-avenger-presets` (`modules/vps-avenger-presets`): an index and one file per
      expansion — 124 expansions, 17,571 presets, their categories, oscillator tab names and macro
      names, Avenger's default names marked; 129 files, 2.66 MB of data, from the export of
      2026-09-26. **An allowlist**: only the rosters "Complete" and "Factory Standard" are read,
      and the lists of what individual people own, and the expansions only such a list has, are
      dropped unread, counted and never named. A code module whose reader runs in VPS Avenger's VM,
      an optional dependency (`host.tryRequire`): without it the module is stage 1 exactly. With it
      (`src/presets.luau`): a name read is snapped to the catalog as the tool does it (exact, then
      any case, then similarity 0.72 in the known expansion and then everywhere, where the closest
      name also has to stand out from every other by 0.05; ´ read as '; "Init Preset" and a
      catalog name with more after it never; after Load and Save as, the same words only until a
      step shows another name), ◀ and ▶ add the database's neighbour, marked, when the read-back
      failed or disagrees — the screen's words first, as read when the catalog has them only by
      similarity (the neighbour rule in `presets.luau`; no timer decides) — and **Preset info**
      says the expansion, category, oscillators and macros, each part marked. Tested against the
      scripted host with a small fixture catalog, never the real data (`overlay_avenger_tests.rs`,
      7 more scenarios); the converter by its own 7 tests on a made-up export.
- [x] **The database's licence** (2026-10-02). There is none to wait for: the avenger_control tool
      has not been published and has no licence, and its developer gave the database without
      restriction, to be integrated. The data module is GPL-3.0-or-later like the rest
      (`convert.py` writes it so); its `NOTICE` names where the data comes from.
- [ ] **The Mac package writes no licence text at all** — not the GPL, and not where the source
      is (the Windows package's `licences\README.txt` does both). Not the database's matter any
      more, but noticed with it.
- [x] **Credits in the catalog** (2026-10-02). Two expansions, "The Collection" 1 and 2, name their
      categories after producers — VPS's published credits, as Avenger's own browser shows them,
      not anything about who owns what — and Preset info says them ("category …"). The
      maintainer's call: the presets stay exactly as they are.
- [ ] **New expansions.** The data is the export of 2026-09-26: an expansion released since is not
      in it, and neither are its presets: they are said as read, with no neighbour, and Preset
      info says they are not in the database — unless one stands out as like a catalog name, which
      is then said as that name, marked (29 % of 279 catalog names tried with their own expansion
      left out, 71 % without the 0.05 margin; `presets.luau`, SNAPPING). A new export from the project's developer → `convert.py <folder> --version
      <next>`, and commit the data module (`tools/avenger-presets/README.md`, "When a new export
      arrives"). Nothing checks for one by itself.
- [ ] **Whether Avenger's ◀ and ▶ walk the catalog's order** — its lists are category block after
      category block, not alphabetical, and that is the order the tool steps through; nobody has
      compared it with Avenger. The step lines in the tester's log (`the database's neighbour` and
      `read '…', the database's neighbour exactly` or `not the database's neighbour`) answer it,
      and whether Avenger's own search, open, makes ◀ and ▶ walk its results instead (the manual
      says so since 1.8.0: every step then disagrees, which is what it should say). If that turns
      out to be too much to hear, the review's suggestion: after a step whose read is another
      catalog name, stop saying the neighbour (log it) until a read equals it again — state, no
      timer; not built, because the decision is to say the neighbour when the read disagrees.
- [ ] **What the database costs on a real machine.** Measured only in a debug test build against
      the real data: the index read and indexed at the first ◀, ▶ or Preset info in 130 to 160 ms
      (a first Preset info 150 to 190 in all), a name compared with the whole catalog — one it does
      not have, the user's own preset — 150 to 300 ms for ten made-up names (the 0.05 margin's
      share about a tenth), on the event loop (`presets.luau`, COST; answers are kept, so once per
      name). ◀ and ▶ do that after their click is sent, so it delays what is said, not the step.
      The shipped release build is not measured: the `[avenger] the preset database: … read and
      indexed in … ms` line on the Mac, and a Next onto a preset of the user's own. If it shows, a
      bigram index for the search over the whole catalog.
- [ ] **Which expansions the user has, in Avenger's order.** Not in the catalog, deliberately (the
      owner lists stay out): at an expansion's first and last preset there is no neighbour, and
      Avenger goes on into the user's own next expansion. Stage 4 reads the expansion list off the
      browser. The user's own presets and expansions are not in the catalog either: said as read,
      unless one stands out as like a catalog name (then marked, as a new expansion's above); after
      Load and Save as, as read until a step shows another name, as the tool skips its snap there.
- [ ] **The Preset read-out says what the recogniser read**, not the catalog's spelling: it is the
      runtime's OCR read-out, which has no hook to change its text, and Preset info, ◀ and ▶ are
      where the catalog's spelling is said. A general `format` for an OCR read-out would do it;
      not built, as nothing here needed the runtime changed.
- [ ] **Undo does not forget the database's position** — Undo is the runtime's hotspot, which tells
      the module nothing of its click (the project's tool forgets its position there: "Undo can
      revert a preset change"). A MENU item chosen, the redo list opened and the overlay leaving
      the front do forget it. A stale position shows where a read fails: a step whose name before
      the click cannot be read takes its neighbour from it, and Preset info whose read fails speaks
      from it — marked, and replaced by the next name read. Two ways to forget it, both the
      maintainer's call: Undo as the module's own button (as the redo list is), which loses its
      crosshair in the runtime's calibration shot and its "not available" answer, both stage 1's
      and pinned by its scenarios; or a general after-click hook on the runtime's hotspot button.
      Whether Avenger's undo can change the preset at all is not known.
- [ ] **A step overtaken by another press says "the preset name cannot be read now"** when its watch
      runs out, two seconds after the later step said its name (found while adding the database;
      stage-1 behaviour, kept as it was): the overtaken watch reads nothing more (`nameAfter`) and
      gives up with nothing. An overtaken watch should say nothing — `gen ~= names.gen` in its
      `onGiveUp` — which the database's rule already does for its own part (`decided`).
- [ ] **How long Preset info is**: about 220 characters on average over the catalog, up to 440,
      "from the database" three or four times. Tabs and macros nobody named are said with
      Avenger's own captions ("OSC 3", "MacroBtn 1"), as the tool's Info shows them; saying
      "buttons unnamed" instead, or one marker for the whole answer, is a choice for after the
      tester has heard it (the reader's `default` flag has what it needs).
- [ ] **What else the catalog gives, later**: the oscillator and macro names as the vocabulary a
      stage-2 read of the tabs is checked against (five or eight words per preset); stage 4's
      lists of categories and presets, and free-text search over all 17,571 names.
- [ ] **Everything the project's tool does** is the goal: what is left of its actions after stages
      1 to 4, then what it never reached (ARP, Step SQ, the effects, the mixer, the modulation
      envelopes, zones). And a standalone Avenger, if there is one and it is wanted: an `O.window`
      binding beside the embedded one, as sforzando has.

## VoiceOver's AppleScript box, caught (2026-10-01)

The Mac session of 2026-10-01 (an Intel MacBook Air, macOS 15.8, Retina 2x, VoiceOver running):
every permission granted, "Speak through VoiceOver" on, VoiceOver Utility's "Allow VoiceOver to be
controlled with AppleScript" unticked — and silence. Each line is an Apple Event sent without
waiting for a reply, so VoiceOver dropping it could not be told from success: the log said
`speech is going to VoiceOver`, the first line "took 0 ms", and no refusal ever came, so the
fallback for refusals never spoke. The maintainer's decision: catch it; the user is never left in
silence.

- [x] **Built — not yet run on a Mac.** The rules are pure and tested (`speech/vo_script.rs`), the
      reads and the question are `speech/voiceover.rs`. Read on the event loop, asking VoiceOver
      nothing: whether `/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled` exists (a
      `stat`), and below macOS 15 `SCREnableAppleScript` in `com.apple.VoiceOver4/default`
      (CFPreferences). Then, on the speech thread, `get text under cursor of vo cursor` through
      `osascript` — only with Automation granted, only while VoiceOver runs (`is running`, and the
      `tell` compiled by `run script` only after it, so nothing starts VoiceOver). -1708 means not
      allowed, over the reads too; an answer means allowed only where the reads cannot say, never
      over a file that is not there (a `get` may be answered with the box unticked, and a wrong
      "allowed" is the silence); anything else leaves it to the reads. -1744 holds the sentence
      back only while the switch's own Automation request is out
      (`perm::voiceover_automation_asking`), since the system answers -1744 before anybody asked
      too. Asked at start, when the setting is ticked (now watched in `pump` too, as on Windows),
      for every new VoiceOver process, and when VoiceOver Utility quits (the backend's quit notice,
      by bundle id or `VoiceOver Utility.app`, both logged); while not allowed, the reads are
      looked at again on the refusals' pace, and VoiceOver is asked again when they change.
      Looked after while the setting is on or a module chose `voiceover`. While not allowed every
      line goes to the system voice, lines that waited behind the question included; the user
      hears the sentence once per episode — not over the Automation dialog the switch puts up —
      and VoiceOver says "Automation Platform now speaks through VoiceOver." once VoiceOver itself
      answered. `[env] voiceover applescript` (from the reads alone), and a plain line on the
      Permissions page. Nothing writes the box.
- [ ] **The next Mac session verifies**, each from the log:
      - **VoiceOver's answer with the box unticked — the question that decides the design:** the
        raw line `VoiceOver, asked through AppleScript (…), answered: …`. -1708, as the sources
        say for VoiceOver's own commands: as designed. `answered`: the `get` is not gated — safe
        now, because an answer never counts over the file, but useless; replace it by something
        VoiceOver gates that says nothing — `output ""` if an empty `output` does not cut off
        what VoiceOver is saying, or the first line after each trigger sent through `osascript`,
        which waits for VoiceOver's reply. Anything else that means "unticked" (-1728, -10000)
        goes into `vo_script::classify`, with that log as its source;
      - **the file:** does `… is there` / `… is not there` in `[env] voiceover applescript` follow
        the box on 15.8, and on 12.7.6 and 14.5? Or does it say `could not be looked at (…)` —
        then `/private/var/db/Accessibility` cannot be searched by a user, and VoiceOver's answer
        alone decides;
      - **the key:** what `SCREnableAppleScript` reads on 15.8, where it is not counted, and on
        12.7.6 and 14.5, where it is — does it follow the box?
      - **with the box ticked:** `answered`, and `the first AppleScript question to VoiceOver took
        … ms`; not -1728 with nothing under the VoiceOver cursor (then the question needs another
        property);
      - **the question is silent:** VoiceOver says nothing and its cursor does not move when it is
        asked, at start and right after VoiceOver Utility quits;
      - **VoiceOver Utility quitting:** a `VoiceOver Utility quit (bundle id …, bundle …)` line,
        which settles the id, then `asking VoiceOver whether it accepts AppleScript (VoiceOver
        Utility quit; …)`. And does the look every 3 s find the box ticked before that, as soon as
        the administrator's password has been given?
      - **heard:** at start with the box unticked, the startup announcement and then the sentence,
        once, in the system voice; after ticking the box, "Automation Platform now speaks through
        VoiceOver." in VoiceOver's voice, and sforzando's read-outs through VoiceOver;
      - **VO-Fn-F8** opens VoiceOver Utility on the tester's laptop, as the sentence says;
      - **a new build with the setting on, Automation never asked and the box unticked:** the
        sentence comes at start with no Automation dialog; the dialog comes with the first line
        after the box is ticked;
      - **the setting ticked with Automation not answered yet:** the sentence is not said over
        macOS's dialog, and comes within a few seconds of answering it;
      - whether the Automation dialog the `osascript` rung puts up outlives the child stopped at
        5 s. It is not counted as "on screen" (`ask_voiceover`): only a line handed to VoiceOver
        puts it, so only while the reads do not say "not allowed".
- [ ] **Later, once the answers are in:**
      - whether `host.speech.engines()` should report `voiceover` unavailable while VoiceOver does
        not accept AppleScript (today `available` means running);
      - a line handed to VoiceOver before the box was found unticked is lost, sent without a reply:
        unticking the box mid-session loses the lines until VoiceOver Utility quits. The
        maintainer's call: while lines go to VoiceOver, `stat` the file every 10 s (microseconds
        each) and take it disappearing as a trigger, which caps that loss at 10 s;
      - whether the sentence is said again when VoiceOver Utility quits with the box still
        unticked — today once per episode, and an interrupting line cuts it off;
      - with Automation refused (-1743) the sentence names only the box; whether it should name
        Privacy & Security, Automation as well.

## Text recognition on the Mac, measured: `ocr-bench` (2026-10-01)

Why a small read-out on the tester's Intel MacBook Air (macOS 15.8, Retina) took 143 to 786 ms,
against 22 to 42 ms on the Windows machine and 25 to 56 ms on the warm Mac mini M1: the logs
point at Vision without a Neural Engine, a second ladder pass on some fields, the first pass on a
thread, and two Vision passes at once — and none of it is measured on its own. The instrument for
that is built; what the application reads and says is unchanged.

- [x] **Built, never run on a Mac.** `automation-platform ocr-bench [--quick] [--quiet]
      [--long-idle] [--capture-ms N] [--out FILE] [--summary FILE]` (`crates/host/src/ocr/bench.rs`,
      the pure half with its tests; `backend/macos/ocr/bench.rs`, the Vision half, a child of
      `ocr.rs`); ten pictures carried in the executable, drawn by `tools/ocr-fixtures/make.py`
      (sforzando-like fields at 1x and 2x, the digits about 11 px tall at 1x as in sforzando's own,
      and a line on the pass-through path); docs/building-on-macos.md, "Measuring text
      recognition". CI: the job `ocr-bench` in macos-build.yml runs the downloaded zip on macos-15,
      macos-26 and macos-15-intel, keeps each leg's output as an artifact, and runs the tester's
      script with `--help`; the import check also refuses `_MLAllComputeDevices`, Core ML's device
      classes and Vision's compute-stage names. The tester: `measure-text-recognition.command`
      beside the `.app` (quiet, a sound for done and another for stopped, the file shown in
      Finder) and README step 7. In `ocr.rs`, `run_vision` was split into `new_request` and
      `perform`, the crop margin became `content_margin`, `upscale_for` goes through
      `upscale_toward`, and two per-thread cells only the benchmark touches were added: a pass
      counter, and a revision override for its revision-2 pipeline round. Nothing the application
      does changed.
- [x] **The review's findings, fixed before the first run** (2026-10-01): the engine's order is
      shuffled over every cell each round instead of rotated, so no variant always follows the
      same one; `prod-b`, a control, says when a run is too noisy; the verdict is an exact
      Mann-Whitney test at p < 0.01 plus the 10 %/5 ms gap (no verdicts in `--quick`), and a cell's
      time cap leaves out its first pass; every line is written the moment it is known, the
      variants that call what no Mac has run (revisions, compute devices) are tried in a process of
      their own first, and the summary page gets each table when its section ends; one uncounted
      first-pass process, the warm-up order turned each round, three rounds; the pipeline counts
      the ladder's budget from a pretend 50 ms capture (`--capture-ms`); every measuring thread
      asks for user-initiated and the run says what it got; the timed pass includes making the
      request, as the application's does; the pictures are decoded once into a bitmap; the
      fields are redrawn at sforzando's digit height (11 px at 1x; they were 8); idle passes on the
      known and on a fresh thread, and `--long-idle` for minutes; the tester's script no longer
      assigns zsh's read-only `status`, and finds a running application with `pgrep -f` (`pgrep
      -x` compares a name cut to 16 characters). Not done: a Mac-side `#[test]` that decodes the
      pictures, because the build job's tests gate the download and the benchmark already says
      which picture it could not use; widening README step 2 to the whole folder, because step 7
      runs the script through `zsh`, which Gatekeeper does not check.
- [x] **When the CI job runs** (the maintainer, 2026-10-01): on every build with Rust changes.
      That is the job's `if:` line as built: every run that built the executable, and every run
      started by hand. A run that reuses an earlier executable measures nothing new and skips it.
- [x] **Read** (2026-10-02, CI run 36972644006): every leg finished, in 226 to 408 s. No CI Mac
      offers Vision a Neural Engine. On the Intel runner one accurate pass over a small field
      costs 262 to 312 ms, the fast level 12 to 14 ms and it reads the fields and the line right,
      but not a lone digit; a lone digit takes today's ladder two passes (919 ms), revision 2 one
      (427 ms), likewise on the arm64 runners. Two passes at once take 2.3 times as long each on
      the Intel runner, with no gain in throughput (1.8 and 2 times on the arm64 ones). The first
      pass is a cost per process, not per thread, and the warm-up over words makes the first real
      pass as fast as a warm one. What the next run is read for is in "macOS (2026-08-13, written
      blind)".
- [x] **What to read from the first CI run** — the summary page has the tables, the job log every
      line, the artifact `ocr-bench-<runner>.txt` the whole output. Compare within one leg, never
      milliseconds between legs:
      - Did every leg finish? The Intel leg is the first time CI executes the x86_64 slice at all;
        one that dies is a crash to fix before the tester runs it. A `probe |` line that says a
        process died names the call.
      - `compute devices`: does Vision offer text recognition a Neural Engine or a GPU in the arm64
        VMs, and anything but the CPU on the Intel runner?
      - `pipeline`: Vision passes a read for `lone-1` and `field-empty`. 1.0 means the ladder did not
        climb on these pictures; more means it did, and that many passes is the cost. The `rev2`
        rows beside them: whether revision 2 reads them in fewer passes or more.
      - `control`: if prod-b came out "clearly" anything, no verdict of that leg counts.
      - `switching`: prod in the engine against prod pass after pass; 15 % or more apart means the
        engine's milliseconds carry the cost of switching models.
      - `engine` verdicts against `prod`: which variants are "clearly faster" and still read every
        picture right — above all `rev2` and `cpu`/`gpu` on the Intel leg, and `mth-0.25`,
        `reuse`, `target-48` and `fast` everywhere. Only data: the fast model's place last in the
        ladder is a decision, not a finding.
      - `first passes`: the first real pass after the former bars warm-up against the one after
        the application's word warm-up and the one with none — did the bars warm the recogniser
        at all, and does the line of words?
      - `threads`: the first pass on a fresh thread, the process warm — as dear as a first pass in a
        process (a cost per thread) or as cheap as a warm one? And two threads at once against one.
      - `idle`: 2, 10 and 30 s on the known thread and on a fresh one, against warm.
      - Whether `prod` reads every picture right on every leg (a leg says so as a warning).
      - The quality of service each section's thread had (`this thread at …`).
      - How long each leg took, for the decision above.
- [ ] **Revision 2's reading is not decided by these pictures.** They are imitations drawn by
      FreeType; whether revision 2 reads real plug-in text as well as revision 3 needs real
      read-outs (OCR debug captures from a session), whatever the benchmark's `rev2` rows say.
- [ ] **The tester's Intel Air, then an Apple-silicon Mac**: `zsh
      ~/AutomationPlatform/measure-text-recognition.command`, the application quit, on mains power;
      send the `ocr-bench-N.txt` Finder shows. Only the Air gives a 10 W laptop's milliseconds, and
      only the M1 the figures with a Neural Engine. Read the same lines. Once that has run,
      `--long-idle` on the Air, unattended, for the minutes of idle.
- [ ] **What it does not answer**: capture on a Retina screen, heat over a session, the load of a
      DAW, real plug-in text. The application's own log answers the benchmark's questions on real
      fields since the change below (Vision passes per read, two passes at once, the first pass
      per process against per thread).

## Text recognition on the Mac: the logs, the warm-up, focus reads off the loop (2026-10-01)

The maintainer's decisions on the assessment of why a read on the tester's Intel MacBook Air took
ten times what it takes on Windows: build the items that change nothing that is read — logs that
let the next ordinary session answer the open questions, a warm-up over real words that is also a
self-test, a warm-up on the recognise thread, the measured costs in the docs — and move the
overlay runtime's focus reads off the event loop. Not now: anything that makes Intel faster
(revision 2, the budget before the second rung, the fast model earlier) waits for the first
`ocr-bench` CI numbers; no recognition setting was changed.

- [x] **The cost lines** (`backend/macos/ocr.rs`; their wording and arithmetic in `ocr/cost.rs`,
      tested on Windows). The first recognition in the process and the first on each thread — one
      line for a recognition that is both — with how long Vision had been idle before it in the
      process and on that thread, or that a pass of another thread was running when it began, and
      whether another pass ran beside it — the old line called the first on every thread "Vision's
      model load". A recognition of 100 ms or more, at most one line per region every 10 s on each
      thread: its capture, and each Vision pass with its rung (`as captured`, `tight crop`, `whole
      region`, `enlarged`, `fast model`), its time, its words, and `another pass beside it` when
      another pass ran in the process at any moment of it (a count of the passes in Vision, around
      `run_vision`); for a `host.ocr.read` the 100 ms are the recognition's alone (`picture taken
      apart`, its capture in the `[ocr]` line). The `[ocr] … waited` line names its regions.
      `[env]` names the processor (`cpu`, on Windows too: CPUID's brand string,
      `GetLogicalProcessorInformation`, `GetActiveProcessorCount`; on a Mac the `_max` counts) and
      the revision a request carries (`vision`).
- [x] **The warm-up reads a line of printed words** — `line@1x`, the benchmark's picture, so its
      "word" warm-up is the application's — and says whether Vision read it right, how many of its
      words it read right, or that it read nothing (`… but read NOTHING in its test line …`: a
      recogniser answering empty, as a report of macOS 27 has it; only that is an alarm). Its
      boxes are placed in the picture, so the words come back in reading order. Its thread is named
      `ocr-warm-up`. The recognise thread makes the same pass once it has read the languages and
      the `ocr-warm-up` pass has ended (`cost::Gate`, no timer), in the language of its reads,
      before its first job and on the hang clock of one: a warm-up that hangs has reads refused
      with the reason after 5 s (`OcrWorker::warm_up`; nothing on Windows). One after the other,
      so the first line times a first pass in the process and the second a first pass on another
      thread of a warm process. The bars are the benchmark's alone now.
- [x] **Docs**: docs/api/ocr.md's macOS sections give the measured costs per machine (Mac mini M1,
      the Intel Air, the CI's arm64 runners), the warm-up and the cost lines; ocr.rs no longer
      cites Apple guidance of "around 32 px" (no Apple source was found; Apple documents only the
      relative `minimumTextHeight`), and the warm-up's timings are the measured ones — of the
      warm-up over bars, said so; what the logs only suggest (two passes for the Instrument field,
      a second pass for a lone digit) is said as probable, with the line that will tell.
- [x] **The overlay runtime's focus reads off the event loop.** An OCR control's value region and
      a control's `ocrLabel` are one `host.ocr.read` per announcement (one picture for both), under
      one key per overlay (`"com.platform.overlay focus N"`), in the user's language; the
      announcement is one sentence — name, text, type word, key, value, a toggle's state and hint,
      as before — said when the answer comes, and only while it is still true: not `"stale"` or
      `newer`, the overlay active, on the same window, in the same stay, the focus where it was, no
      key of the overlay's own since (`ownKey` counts them), no other announcement since, no menu
      open over it now or opened since (`_menuChange` counts them). `"blank"`/`"none"` say the
      `fallback` or "no text", as an empty read did; `"failed"` says "cannot be read now"; a read
      refused at the call is said at once without a value; a region turned around is not read and
      says what an empty read said; lines are said joined with a space. Activating an OCR button
      clicks after the sentence, once the answer is in — the order it had on the loop: the value
      from before the click, the sentence not cutting off what the click opens, an `opensMenu`
      press counted at its click so its menu comes after the value — and only while the overlay is
      where the key was pressed and no key of its own came since (logged when not). An OCREdit's
      focus click likewise, after its sentence, while the focus is still on it. A toggle's watch
      and the arrival announce nothing over a key of the overlay's own that came after them.
      Corners are cut toward zero as `recognize` read them. Every answer is one `[read]` line with
      what became of it, the words the recogniser placed and those placed by estimate (Windows's
      second recogniser) counted apart, and the language. Scenarios:
      `overlay_focus_read_tests.rs`; the other harnesses answer these reads by themselves
      (`T.autoFocusReads`). ik-on-ear's "Virtual speaker", a static text whose `text` read a fixed
      region with `recognize`, is a read-only OCR button now (`fallback = "not shown"`).
- [ ] **The next ordinary Mac session** (sforzando on the Intel Air, with this build) answers, from
      the log alone: how many passes the Instrument field takes (`ocr: a slow read … tight crop …;
      whole region …`); whether passes ran at once (`another pass beside it`); whether the
      expensive first read is per process, per thread or after a pause (the two warm-up lines, the
      first-recognition lines with their idle times); which processor the Air has (`cpu`); the
      revision (`vision`); whether both warm-ups read their test line.
- [ ] **Whether the recognise thread's warm-up earns its cost**: it runs after the warm-up on
      `ocr-warm-up` has ended, and reads asked in the first seconds wait for both — up to about
      1.8 s on the Air for the first, and for the second as long again if a first pass is a cost
      per thread. Its line's time answers that: dear, and it is worth its wait; as cheap as a warm
      pass, and the first pass is a cost per process — then it can go again, and the event loop's
      first synchronous read needs no warm-up of its own either. `ocr-bench threads` asks the same
      on fixed pictures.
- [ ] **The focus reads off the loop, live**: on the Air — the sentence as late as the read, but no
      stall of the keys and no tap switched off by OCR (the `[read]` line's `ms` against the
      `[focus]` line's); on Windows with NVDA — Tab and Return through sforzando's and a Kontakt
      library's OCR buttons, the sentence as before, nothing said twice, Return's sentence before
      what its click opens (sforzando's menus: the value, then the menu), Komplete Kontrol's "Save
      as" with the caret in the field after its sentence. And whether any answer is dropped that
      should have been said (`not spoken: …`), or a click (`not clicking — …`). On the Air at
      plug-in entry: an overlay brought to the front by the landmark poll asks its arrival's read
      from a timer, so as a background read — does it wait behind the detection reads of entry
      (up to 4.8 s there) before the arrival is said? The `[read]` line's `ms` says.
- [ ] **A key between an OCR button's press and its answer drops the press's click** (logged).
      Chosen because a click after the user moved on — a menu opening over the control they went
      to — cannot be undone by pressing again; the other choice, the image button's, clicks while
      the overlay is on the same window. The price: Return twice inside one read (on the Air,
      within half a second) clicks once. For the maintainer to confirm, or to turn the other way.
- [ ] **A press's watch and the arrival over a key**: a hotspot toggle's or graphical toggle's
      announcement after a press, and the arrival's, are not made when a key of the overlay's own
      came after them; a stepper's (`announceWhenChanged`) still is, while the focus is on it —
      with a held arrow it says each value it sees settle, as it always did. Whether held keys
      there want the same rule is open.
- [ ] **The language of a focus read on a Mac** is now the user's first one Vision reads, where
      `recognize` gave Vision none. Whether `de-DE` reads a plug-in's English labels and values
      ("64", "DEF", "-3.0 dB") as Vision's default did is not measured: no tester has had German
      first (the Air resolves to `en-US`). On Windows both were the user's first installed OCR
      language.
- [ ] **Not measured, and so not in the API doc**: ScreenCaptureKit's capture on Apple silicon
      (macOS 15.2 and later; the M1 session captured with `CGWindowListCreateImage`); any Apple
      silicon newer than the M1 (an M2 figure exists only in an earlier tester's log that is not
      here); the idle curve past the M1's 5 s and the Air's minutes (`ocr-bench --long-idle`); two
      Vision callers at once on a real Mac beyond the Air's few readings (1.2 to 2 times); the Air
      under `ocr-bench`. Measured on the CI's virtual Macs since (2026-10-02), and not yet on a real
      one: the warm-up over words (0.6 to 0.9 s on the arm64 runners, 1.4 to 4.1 s on the Intel
      one, the line read each time) and two passes at once (1.8 to 2.3 times as long each).
      "One accurate pass about 150 ms" on the Air is worked out (a read less its capture), not
      timed; the slow-read lines time it.
- [ ] **The modules' own synchronous reads, not moved** — each is not a focus announcement the
      runtime makes, or needs more than the runtime's pattern:
      - `modules/kontakt/src/actions.luau:265`, `invokeMenuItem`: reads the open file menu to find
        a row, then clicks it, in a timer. Read-then-click, not an announcement; moving it puts the
        click in the read's callback, with guards of its own (the menu still open, the overlay on
        the same window). Worth doing; a change of its own.
      - `modules/vps-avenger/src/main.luau:170`, `nameNow`: the preset's name before a ◀ or ▶
        click, which has to be the name before the click (the module's own comment); a read whose
        callback makes the click would do it, and changes when the click is made.
      - `modules/sforzando/src/main.luau:186`: the wordmark on Windows's own-control path, inside
        an `identify` that answers a boolean, once per control; the Mac's panel path is already off
        the loop (`wordmarkOnPanel`).
      - `modules/ik-on-ear/src/browsers.luau:421` (the search field, an editable custom button) and
        `modules/melodyne/src/main.luau:860` (`readPosition`, which parses what it reads and reads
        again): `text` providers, which the runtime calls synchronously for their string. Off the
        loop they need the runtime to read a region for them with the focus read, or an
        asynchronous provider — a decision about the runtime's API. (ik-on-ear's virtual speaker
        was one too; a plain read of a fixed region, it is a read-only OCR button now.)
      - `modules/ik-on-ear/src/browsers.luau:749` (`relearnGrid`), and in
        `modules/melodyne/src/main.luau` the reading after a tool switch (`reportFieldsAfterSwitch`,
        through `readSelection`) and the calibration diagnostics (the transport in
        `measureNoteArea`): helpers whose callers want the answer at once; each would be
        restructured around a callback. Melodyne's read-out watcher and its time signature
        (`withBeatsPerBar`) read with a callback since B4.

## Text recognition off the event loop: steps 1 to 3, and a mailbox per module (2026-10-04)

The maintainer's decisions of 2026-10-03 and -04: no text recognition may burden the event loop.
Steps 1 to 3 were built around an explicit await form for modules, `host.task.run(fn)`. Then the
maintainer chose a mailbox per module instead ("way B"): every module does one thing after
another on the main thread, and a `recognize` without a callback is to wait holding only its own
module. So no task API for modules ships at all; it was trimmed out before main ("step S"). What
steps 1 to 3 built stays: the coroutine machinery inside the host (crates/host/src/task.rs,
task_shim.luau), which every callback is to run in and which only the tests reach today
(`task::table`, built in test builds alone); `reading.time` and `reading.inputEpoch` and
`host.ocr.pending(key)` (ocr/lua.rs, docs/api/ocr.md); the loop guard (loop_guard.rs) and its CI
line; and the overlay runtime's building blocks (modules/overlay-runtime, version 0.2.0;
docs/api/overlay.md). Since step B1b+M every handler waits: `host.ocr.recognize(what, opts?, cb?)`
is the one call — with a callback the former `read`, without one a wait that holds only its own
module — and the blocking call is left only where it cannot wait, with one log line per module and
kind of place that says how long the first held the loop.

- [x] **The machinery** — tasks are coroutines the host resumes in the delivery of text readings;
      owned by the VM that runs them, with the priority of the dispatch that started them; dropped
      on disable (after the arbiter's re-election), reload (a second pass after the purge's
      onDeactivate, which also ends the old bug of a timer it armed firing into the old VM) and
      rollback; 16 task stretches on the event loop's stack at once at most (tested in a debug
      build on half the main thread's 1 MiB); module code that resumes, closes or yields a task
      through to the host handled as task.rs says, in the real host and in the scripted test host
      alike, whose scenarios also fail on an error a task ended with. No module can start a task:
      the host table has no entry for it, and the tests reach the machinery through `task::table`
      (the global `task` in task_tests.rs, `T.task` in the scripted host).
- [x] **The two waits** over the read service, with `key`, `lang` as a list, `skipped` and
      `"stale"`; where they cannot wait — in the application everywhere, as no module starts a
      task — the old blocking call, with one log line per module that says how long it held the
      event loop, and a summary per module at exit. The three messages for the places that cannot
      wait are written and tested — they name the callback form — and the scripted test host
      raises them already; the application raises none.
- [x] **`reading.time`, `reading.inputEpoch`, `host.ocr.pending(key)`.**
- [x] **The loop guard** — a debug build panics, a release build logs
      `[loop] text recognition on the event loop`, for a recognition on the loop outside the one
      exception, the legacy call; CI fails on the line after the capture probe, on both systems.
- [ ] **Never run on a Mac** (for the next Mac session; macOS was type-checked only): the guard's
      places in `backend/macos/ocr.rs` (`capture_for_read`, `recognise_shot`, `frames_for_round`,
      `shot_of`, `warm_up_recognise`, `run_vision`) and `paddle_ocr::ask_with`, and that nothing
      but the legacy call reaches them from the main thread — the macOS CI job's capture-probe log
      says so first; the `inputEpoch` and `time` a Mac read carries; a wait end to end on a Mac
      (with the handlers that wait).
- [x] **Step 3, the runtime's building blocks** (runtime 0.2.0): `O:here()` and
      `O:stillHere(mark, opts)` with `focus`, `keys`, `said`, `menus` and `place = false`, the
      reasons in the words of the `[read]` lines; `O:toScreenRect(r, { whole = true })`; a control's
      `text`, `current` or `verticalName` that raises is logged once per control and hook and has
      no value (a stepper's watch no longer compares error messages). `speakControl`, the OCR
      button's click and the OCREdit's click check through the same mark, on the same conditions as
      before. The scripted test host records every member it lacks (`S.missing`) and fails a
      scenario that leaves one, or a hook's raise line, at its end
      (crates/host/src/overlay_runtime_marks_tests.rs).
- [ ] **NVDA check of step 3, before it is committed** — nothing should sound different, because
      the announcement and both OCR clicks were rewritten onto the mark: Tab through an overlay
      with an OCR value (sforzando's read-outs), a press on an OCR button, an OCREdit (Komplete
      Kontrol's "Save as"). sforzando's part passed (2026-10-04); Komplete Kontrol's "Save as" is
      still to be tried — its scan at start takes long.
- [x] **Step K, the key scope and the menu flag per module** (the mailbox plan's first step,
      b0-final.md): `host.keys.scope` and `menuOpen` are the calling module's own (the VM's:
      a code dependency's count for the module that depends on it), kept by the host
      (crates/host/src/captures.rs) and handed to the Windows hook and the Mac tap beside the
      captures, which carry their module now. One decision for both, `capture_decision` in
      backend/mod.rs: the earliest capture whose module is scoped to the window in front, or to
      every window, takes the key; a menu flag counts for its module's window, and only while
      that module holds a capture. The hook queues the module, whether the key is the keyboard's
      auto-repeat (Windows: a bit per key, cleared by its key-up and with what the hook forgets;
      Mac: the event's autorepeat field) and the window in front; `on_key` runs that module's own
      capture and drops the key with a line when it is gone or the scope moved. A disable, a
      reload and a rolled-back hot-load drop the scope and the flag after the arbiter's
      re-election. `passedThrough` is per module. On a Mac a scope the frontmost application
      does not name is the window the tap last saw in front, no longer every window. Fixes the
      late `_deactivate` that set back another module's scope and flag. Tested with two modules'
      VMs over the stub backend, one of them busy in a handler (key_scope_tests.rs).
- [ ] **NVDA check of step K, before it is committed** — Kontakt with a plug-in menu: Tab and
      Enter reach the menu; Melodyne as always; switching between two windows with overlays of
      different modules, Kontakt and Melodyne, and Tab, Space and the arrows in each.
- [ ] **Step K never ran on a Mac** (type-checked only): the tap deciding per module, and still
      cheap enough — no `the system disabled the event tap` line while Tab, Space and the arrows
      go through an overlay; a scope the frontmost application does not name, pinned to the window
      the tap last saw (`key scope: the frontmost application … so the scope is the window the
      event tap last saw in front`); the autorepeat field marking a held arrow's repeats; Return
      and Escape in Kontakt's menu still in the runtime's `[menu]` lines (`passedThrough` per
      module). The Windows NVDA list above, in the Mac session. And a plug-in window opened in a
      DAW that is busy loading it: after the log's `key scope: … the window the event tap last saw
      in front` line, Tab must reach the overlay. The tap's window is not updated while an
      application comes to the front without answering, so that fallback can pin the window that
      was in front before; the next real answer then moves the tap away from the pin, and every key
      of the overlay goes to the application until the overlay comes to the front again. If that
      happens: a fallback that heals — the frontmost application's remembered window, the one the
      overlay matched on — is the maintainer's call (b0-final.md chose the tap's window).
- [ ] **The Mac event tap writes log lines inside its callback** (found reviewing K; older than
      K): `report_gate_pass` formats and writes a line there, at most once every 3 s per reason,
      and the first suppression once per session — while the callback's own comment says a file
      write there is the delay that gets the tap switched off. Hand both to the run-loop observer,
      as `report_disabled` is (an atomic slot for the key, the mask and the reason).
- [x] **Step B1a, every callback a handler of its module** (b0-final.md): each event a module
      hears — hotkey, captured key, controller event, timer, window trigger, the report of the
      window in front, focus change, the answer of a read, an image search or a snapshot, a
      setting's `onChange` from the dialog or another module's `set` — goes through its mailbox
      (crates/host/src/mailbox.rs) and runs as a coroutine of the host (task.rs, the machinery
      renamed to handlers), one at a time per module; the synchronous places stay plain calls.
      Nothing waits yet: `recognize` takes the blocking call in a handler too, so in the
      application only a setting ever waits, for one tick. The rules for a busy module are built
      and tested through a wait point only the tests have: events queued in order and run in a
      tick phase of their own with a 15 ms budget, taking turns; a focus change and an axis folded
      into the one queued last, a held key's repeats folded, 256 inputs at most; a key or hotkey
      to its registration, or the same module's new one of the same key while its scope allows;
      an answer in the mailbox still `pending` and judged `newer` when it runs; a queued `after`
      collected once and cancellable, `every` skipped; a disable keeping a setting's `onChange`;
      the input epoch turned once on arrival, the initial report's eagerly; a trigger made after
      an activation arrived not reached by it; `[dispatch]` without the time parked;
      `coroutine.running()` not nil in a callback, and a callback's own `coroutine.yield` raising
      at the yield with the message in module-runtime-and-lifecycle.md. The scripted test host
      drives the real mailbox (`T.call`), its Lua copy of the task rules gone.
- [ ] **NVDA check of step B1a, before it is committed** — nothing may sound different: Tab
      through Kontakt with a CSS library, through Melodyne and through ON:EAR; one hotkey of each;
      a setting changed in a module's Settings dialog.
- [ ] **Step B1a never ran on a Mac** (type-checked only): every callback a handler there too —
      no `the system disabled the event tap` line while Tab and the arrows go through an overlay,
      and no `[pump]` line naming `queued module events`. The cost of one event as a handler is
      printed (`HANDLER COST:`) by the release tests of the Windows CI and, since the review of
      2026-10-04, of the macOS job on Apple Silicon; the Intel Mac runner runs no tests — measure
      it there (b0-final.md asks for it: a test binary built for x86_64 and run there, or a line
      of `ocr-bench`, which all three runners run), report only, a limit after both are known.
- [x] **Step B2, the read service for handlers that wait** (b0-final.md). A read a handler waits
      for becomes interactive when an event in the interactive lane queues behind it — a key, a
      hotkey, a controller event, a window or focus change, or a timer or an answer one of those
      asked for (`mailbox::enqueue` → `task::promote` → `Scheduler::promote`) — and the
      handler goes on in that lane; a job being recognised runs on as it started (ocr.md). The
      hang answer: once the recogniser has answered no region for `HANG`, the delivery's sweep
      (`Service::hang_sweep`, `Scheduler::cancel_all`) answers every read still out `"failed"`
      with the existing reason, once per hang, a callback's and a waiting handler's alike; a late
      answer finds nobody, `pending` is false again, new reads stay refused until the recogniser
      answers. Tested on held recognitions, never timed (sched.rs, service.rs, task_tests.rs).
      Nothing changes in normal use: no handler waits yet.
- [ ] **Step B2 never ran on a Mac** (type-checked only): the reads asked while Vision's first
      request hangs at start now end `"failed"` after 5 s (`ocr: the N read(s) waiting for the
      text recogniser were answered "failed": …`) rather than wait for it.
- [x] **A promoted handler's timers** (found building B2; settled in the review of
      2026-10-04): a handler raised because a key queued behind its poll's read keeps its lane —
      the read it waits for and every read it waits for after it are interactive (`raised`, the
      second read b0-design.md raises it for), but a `host.timer.after` or a read with a callback
      it arms afterwards is in the lane it began in. A poll chained through `after` stays a poll
      (task_tests.rs, the read with a callback after the key).
- [x] **Step B3, the overlay runtime for handlers** (b0-final.md; runtime 0.3.0). A sentence takes
      its mark before its control's `text`, `current` or `verticalName` and asks it after each
      (`[read] '<label>' not spoken after its <hook>: <why>`; an OCR button's click is not made
      then). The hooks asked on every scan or tick — `when`, an `identify` asked at every
      evaluation, `present`, the menu tests — run in ONE coroutine of the runtime's own per
      scan (`noWait`, `scanPass`), only around the search loops and the menu tests' asking, never
      around what is said; the overlay's origin is resolved before each, outside it. A stored
      `identify` reached inside one is not asked there, not counted, not kept for the epoch, and
      asked on the module's next turn through `host.timer.after(0)`. A binding's first
      evaluation runs on the next tick. Only a stepper's `text` is read before a press (ON:EAR's
      tiles call `freshGrid` themselves). `identify` and `present` guarded; `_activate` resolves
      the origin guarded. Tests in the scripted host (overlay_handler_tests.rs, and ON:EAR's tile
      in overlay_scale_tests.rs); the scenarios that bind now run the first tick before they
      look. Heard: Enter on ON:EAR's "Search" no longer reads first.
- [ ] **NVDA check of step B3, before it is committed** — ON:EAR: Enter on "Search" (the
      click comes at once), the tiles, "Grid down" twice; in REAPER with Kontakt in front, reload
      all modules (Ctrl+Alt+Shift+Win+F5): the overlay speaks again; Tab through Melodyne and
      through Kontakt with a CSS library: as before.
- [ ] **Step B3 never ran on a Mac with a plug-in**: an overlay whose plug-in is in front at
      load — sforzando in REAPER, Kontakt 7 in Logic — comes up on the first tick after its
      binding rather than inside it, which the scripted host holds to; nothing else of B3 differs
      between the platforms.
- [x] **Step B1b+M, handlers wait and one call reads** (b1b-final.md, with the maintainer's
      change of 2026-10-05). Every handler waits: the switch `HANDLERS_WAIT` is gone, and
      `host.ocr.recognize(what, opts?, cb?)` is the one call — with a callback the former `read`
      (`ocr::lua::read`), without one a wait in its handler (the shim's `start`, task.rs), a list
      answering `(list, byName)` in both forms, an explicit `nil` callback raising. Where it cannot
      wait, the blocking call (`legacy_read` in lib.rs) answers the same reading as a wait, with
      `skipped`, through the same `normalise`, its language resolved as a read's; a `key` or a
      `snapshot` there raises; one log line per module and kind of place, and the summary at exit.
      `host.ocr.read` and `recognizeMany` are gone, and `recognize` checks its arguments as every
      host function does. A poll that waited with keys behind it is said once per module and place, a
      handler busy 30 s with keys behind it once (`[<module>] has been busy for …`), the inputs
      dropped at the limit at the first and then at most every 10 s. The guard: a stop names the
      keys that waited in the stopped module's mailbox (`… key(s) that waited for it were dropped
      with the stop: …`, and "Dropped:" in the dialog); a memory error handing a reading over is the
      stop's, not a module error (`handover_failed`). The tree: the overlay runtime (0.3.1),
      kontakt, sforzando, vps-avenger, melodyne and ik-on-ear (each a patch version up), the tools;
      a check after the wait in ON:EAR's tile, Avenger's preset step and info, and Kontakt's file
      menu (the program in front, before the read and after it; Escape only to the same window);
      `lang = "en"` for Kontakt's menu and sforzando's wordmark. Tests (task_tests.rs,
      mailbox_tests.rs, key_scope_tests.rs, the scripted host's scenarios), docs (ocr.md rewritten;
      the busy-module rules on the hotkey, keys, timer, window, settings, arbiter, gamepad and
      screen pages and in module-runtime-and-lifecycle.md, "A handler waits"). After the review
      (b1b-review.md): a read that failed is no verdict — sforzando's own control answers nil
      ("could not tell yet") rather than a `false` kept for good, ON:EAR keeps its grid to be read
      again, and Kontakt says "Menu could not be read" with the reason in the log; ON:EAR's Grid
      down and up say nothing once another window came to the front during their quarter second or
      their read; the scripted host's waits answered at once (`T.answerWaits`) check the call as
      the host does and are refused where the host could not wait; the probe's batching times are
      named from asking to the answer.
- [ ] **NVDA check of step B1b+M, before it is committed** — b1b-final.md section 6, at the reads'
      own speed: Melodyne, Kontakt (Ctrl+L, Ctrl+S, Ctrl+R) and sforzando as before — Kontakt works
      (the maintainer, 2026-10-05); ON:EAR and Avenger once a device and the plug-in are at hand
      again. The steps that needed a read slowed on purpose — a window switch, a key or another
      module's hotkey while a read is out — are the automated tests' (task_tests.rs,
      key_scope_tests.rs, mailbox_tests.rs, the scripted host's scenarios, on held reads); by hand
      only where a read is slow by itself. Two modules in one FX chain are Avenger and Kontakt —
      Melodyne's overlay is for the standalone Melodyne only: a preset step in Avenger and at once
      to Kontakt and Tab there; when Avenger's capture took that Tab and let it go meanwhile,
      Kontakt's capture gets it, as if Avenger had never captured it (`went to com.platform.kontakt,
      the next module that captures it in the window it was pressed in` in the log; the maintainer's
      answer of 2026-10-05 to pk-final.md's question a), and only with no module capturing Tab there
      does it go to the DAW (`was passed on to the program in front`). Then a quarter of an hour on
      a release build with no `[guard] … stopped` line, bringing the `[cpu]` lines, the poll lines
      and every `could not wait here`.
- [ ] **Step B1b+M never ran on a Mac** (type-checked only): Kontakt's Alt+V, load and save read
      the file menu in English and click the right entry, and a Cmd+Tab away during a read slow
      enough to try it clicks nothing and sends no Escape; Avenger's preset steps and "Preset info";
      sforzando recognised on the Intel Air, freshly loaded too; keys pressed in another window
      during a read; no `the system disabled the event tap` line while a module waits, no
      `[guard] … stopped`, no `could not wait here` but the example's; and the reads asked while
      Vision's first request hangs at start end `"failed"` after 5 s.
- [x] **Step B4, Melodyne's polls with a callback** (b0-final.md, B4; 2026-10-05). The read-out
      watcher and the note area's time signature read with `host.ocr.recognize` and a callback, so
      a tick ends at once and Melodyne's own keys no longer wait behind a poll that asks eight
      times a second. One read out at a time under a key (`host.ocr.pending`). An answer is about
      the moment it was asked in: dropped, and no baseline, when the overlay has left, gone to
      another window or come back in another stay since (`O:stillHere`), or a menu is open; the tool
      switch's hold is decided by the time of the question, and the first answer to a read asked
      after the switch (after a variant's last press) is the new tool's baseline, said nothing —
      with reads slower than the hold, the read asked before the switch is held but holds the old
      tool's values, and the next one used to be said as a change (found reviewing B4). Each box's
      text on one line, and a box that failed or went stale is no reading rather than an empty box.
      A move in the note area is said when the time signature answers, with no frame taken
      meanwhile; the memo is written by the answer, so a read that failed or never answered is asked
      again. Melodyne 0.1.2, on `com.platform.overlay >= 0.2`; scenarios in
      overlay_melodyne_tests.rs.
- [ ] **NVDA check of step B4, before it is committed**: the same sentences as before — a note
      walked with the arrows and its cents changed, a move in the note area said under a tool with
      no read-out (under Time, "right, one beat" or "right, one step"), a tool switch whose name
      the read-outs do not talk over and after which no sentence about the new tool's read-outs
      comes; Tab, the arrows and Alt+F answer as before. An answer that comes after the tool
      switch's hold, and the keys that no longer wait behind the watcher, are the tests'
      (overlay_melodyne_tests.rs).
- [ ] **Melodyne's reading after a tool switch still waits** (found building B4):
      `reportFieldsAfterSwitch` reads both boxes and the wide field in its `host.timer.after`
      handler, for the log only, so Melodyne's keys wait behind those two reads after every tool
      switch, for as long as two reads take, and when keys waited the log's `a timer waited` line
      names its place. B4 left it as the plan has it (log only); with a callback it
      would hold nothing.
- [x] **Melodyne's analysis read-out, back, and the note area's defects** (the maintainer's
      decisions of 2026-10-05; inv-1.md, inv-3.md). The read-out dropped by accident on 2026-08-13
      returns as a static text "Analysis" at the end of the Tab ring, Alt+A, saying the pass or
      "idle" from the looks, reading nothing itself. A look every 500 ms while the overlay is in
      front takes a picture of the disc's place with `snapshotAsync` (key, `at`) and decides from
      its pixels — the caption band halving what is under it, 13 rows, and the rim around the
      centre the band gives — then reads the pass name from that picture with a callback; a look is
      also made as the overlay comes to the front, so the read-out has its answer by the arrival
      sentence. "Busy: <pass>" once, after the arrival sentence has been said (not only begun:
      `O:stillHere` `spoken`, runtime 0.4.1), and not after the read-out has said the pass;
      "Analysis finished" once, after two looks in a row without the disc in the same stay. The note area takes its frames the same way (profile of
      the snapshot in the callback, every check asked again, the snapshot released on every path),
      tests each frame for the disc and compares nothing while it is up or an analysis is known;
      frames carry a mark and go when the gate closes or the stay changes; a re-baseline forgets
      the note's position and the bar lines; its picture holds the disc's place too, for windows
      wider than about 1790 px. The read-outs' silence belongs to a stay and is the picture's to
      say: the watcher takes one `snapshotAsync` picture of the strip per tick and reads from it
      which boxes are drawn, whether each holds a value (ink in three rows or more against its own
      fill — a value 8-9 rows, the dash one, Fade's empty box none, on the calibration captures) and
      the text; an empty read of a value is not silence, and Fade, Time and Note Separation are. No
      live capture on the watcher's tick any more; `boxDrawn` reads the watcher's memo. Log lines:
      every sentence of the note area with its evidence, the gate opening and closing, the analysis
      found and finished, and every hundred frames their cost. A static text takes a `hotkey`
      (runtime 0.4.1). Melodyne 0.1.4.
- [ ] **NVDA check of the analysis read-out, before it is committed**: load a file into Melodyne
      and hear "Busy: <pass>" after the arrival sentence, never over it, also when coming back with
      the focus on "Inspector"; Alt+A says the pass while it runs and "idle" after; coming back
      onto "Analysis" says the pass once, with no "Busy" after it; "Analysis finished" once; no
      "left/right, … step" while the disc is up; under Time, and under Fade, a note moved with
      Ctrl+Right is still said, and under Pitch with a note selected nothing of the note area is;
      Space plays with the focus on "Analysis".
- [ ] **Melodyne's disc at another window size**: the disc and its band were measured on one
      capture of a 962x660 window, centred 4 px above the client area's middle, and the band is
      looked for 8 rows up and down. A calibration shot of an analysis in a window of another size,
      and one of another pass (Percussive, Melodic), would say whether the centre and the caption
      hold; the log's `analysis found` line names the pass when it does.
- [ ] **What the note area costs the event loop now**: the capture is off it, and since
      `host.screen.profile` takes a callback (below) so is the reduction of its 910x522 profile;
      the profile's tables, the comparison and `bodyBetween`'s small row profiles of a move are
      not. The `[melodyne] note area: 100 frames, …` line says how much, and the `[observe]` line's
      `profile with a callback` clause what the tables and the worker's reduction took: 46 ms a
      frame in the debug build of 2026-10-05 (reduction 32, tables 4.3), where a debug test build
      of 2026-10-09 put the reduction at 30-35 ms — the image worker's now — and the columns'
      tables at 0.5-0.8 ms. One calibration stay on a release build gives the real figure. A
      picture identical to the last profiled one is still not profiled (Melodyne 0.1.5, a change
      wait against it); whether Melodyne's idle note area is byte-identical between frames is what
      the line's count says. Open: skipping byte-equal rows in `change.rs`'s `differs` (24-28 ms
      debug, 0.5 ms release for an unchanged 910x522 picture on the capture thread), and the
      opt-level of Luau and the host in dev builds.
- [x] **`host.screen.profile` with a callback** (the maintainer's decision of 2026-10-09, in place
      of a separate `profileAsync`): `profile(opts, cb)` returns at once; a region's picture is
      taken on `screen-capture` as a plain or timed `snapshotAsync`'s is (`at`, the input barrier
      without it), a snapshot's is the snapshot, held until it is reduced; the reduction runs on the
      image worker; only the tables are built on the event loop, as `cb(profile)` runs as a handler
      of its module, with the picture's `time` and `inputEpoch`, or `cb(nil, reason)`. It is one of
      the module's snapshot requests (16 per VM, 64 in all) and charged against the snapshot
      budget until its answer is handed over; its `key` is its own, and a newer one ends an older
      one wherever it is, the worker skipping a reduction it has not begun. New
      `host.screen.pending(key)`, for `snapshotAsync` and `profile` alike. Dropped with a disabled,
      reloaded or stopped module. Without a callback the call is unchanged. Melodyne's note area
      keeps its picture (the disc in the frame, a move's body, the unchanged-picture skip) and has
      it profiled with a callback, one frame out at a time. Docs: screen.md, ocr.md,
      module-runtime-and-lifecycle.md, timer.md. Not run live yet: the NVDA check of Melodyne 0.1.5
      below covers it.
- [x] **Melodyne's pass name, its note steps under Time, and pictures that did not change** (the
      maintainer's live session of 2026-10-05). The pass name is read inside the disc's rim —
      columns -50..+49 and -49..+49 of the centre in one call, `lang = "en"` — where the old crop
      took the right rim, lighter than the filled band, as an "l" ("Detectionl"). The centre is
      measured from the rim's crossings on twelve rows clear of the band (481.0 and 480.85 on both
      captures from guesses 4 px off), since two pixels left of it both crops read "Polyphonic
      Detectior"; the `analysis found` line says how far it was from the window's middle. A read
      counts only when both crops read the same Latin letters and spaces (accented ones too), and
      a name is the pass once two looks in a row have read it, so a pass that follows another is
      taken at its second look; the first counted read asks for the next picture at once, once per
      analysis. A read answered "failed" with no language where English is not among the
      recogniser's languages turns the reads to the user's language. "Busy" and Alt+A wait for a
      taken name. A note move is measured from the two halves of one frame, seen at 3 rows of ink
      and sized at half each half's peak (the October note's texture made them 34 columns for a
      20 px move): the halves touch (the move is their distance), or the note's body lies between
      them in its colour on its own rows (their width), or the page does (their distance);
      otherwise the words both readings agree on, else the direction alone. Halves that overlap are
      no rigid move: the ink's flow gives the direction, or the sentence is "moved". A frame whose
      cursor moved is no edit; full-height bar lines that moved with a scroll are no cursor, so an
      edit that made Melodyne scroll is measured against the view's move. Half a move holds the
      baseline for one frame, and a picture identical to the one before lets the hold go. Words:
      whole beats when the time signature is read; else the learned step — kept as a part of the
      bar, so it follows the zoom, learned from moves of a sixteenth of a bar or more — counted in
      beats when it is one, else in steps; under half the unit "one fine step". No fractions of a
      bar: the October note's halves measure 21 and 23 px for a 20 px move. Not a regression of
      bc7d61d: at a bar of 80 px the notes changed 3-7 rows, under the coarse 8. The calibration
      transport read takes a callback. Scenarios in overlay_melodyne_tests.rs; Python mirrors on
      the calibration captures in the session's scratch. Melodyne 0.1.5.
- [x] **NVDA check of Melodyne 0.1.5, before it is committed** (2026-10-09, done; the steps
      under Time are the open item below): load a file and hear "Busy:
      Polyphonic Detection" — no misreading — and Alt+A say the same name; under Time at the zoom
      of 2026-10-05 (bars about 80 px), Ctrl+Right says "right, one beat" where Melodyne shows a
      time signature and "right, one step" where it shows a dash, Ctrl+Alt+Right "right, one fine
      step", Ctrl+Left "left, one beat" (or "left, one step"); walking notes with plain Left and
      Right says nothing of the note area. The `[melodyne] note area: 100 frames, N of them the
      picture before` line says how often a picture was the same, and `analysis found` how far the
      disc's centre was from the window's middle.
- [ ] **Melodyne's steps under Time are still inconsistent** (NVDA check of 2026-10-09; deferred
      by the maintainer on 2026-10-10: the core application comes first, module details later).
      The analysis read-out is right: the pass was found after two looks agreed (8 reads not
      taken, 24 over the whole pass), and "Analysis finished" came once. Under Time, presses
      meant as the same Ctrl+Right were said as "one beat", "2 beats", "one step" and "one
      fine step" within seconds of each other (log 1791597119-159: 24 sentences, runs 0-4,
      arrivals 1-24 columns, frame age 87-165 ms). Which key each sentence answered is not in the
      log, so the next look needs the key beside the sentence, and the two calibration shots of
      the item below. The callback profile costs the loop 4.0-4.6 ms of tables per frame in a
      debug build (reduction 31-37 ms on the image worker).
- [ ] **Melodyne's note steps on a picture after the analysis**: the step measurement was tried on
      notes of calibration/Melodyne-3-clean.png and on the October note of Melodyne-5-clean.png,
      drawn during the analysis, moved in Python; there is no picture of the document of
      2026-10-05 after its analysis. Two calibration shots under Time, the note selected, before
      and after one Ctrl+Right at a bar of about 80 px, would show its halves at half their peak
      and whether the cursor stays where it was.
- [x] **Diagnostics for the dead arrows and the hook re-installs** (inv-4.md, D1-D4, and inv-1.md's
      profile split). A re-install for missed key-downs says for how many this process had the
      foreground (`RIM_INPUT`), the window in front when the watch asked, and how often the hook was
      entered since the first missed key-down — the key before the one the witness first found the
      hook silent at — with how long before that key it was last called. With trace on, the first
      five arrow presses let through after each change of the captured set while a module's scope is
      pinned to the window in front (not a held arrow's repeats) are written with the window in
      front and its thread's focus, active and capture windows. The deactivate line names the
      foreground when nothing is in front; the `[observe]` line splits a profile into capture,
      reduction and tables, snapshots included; every `dispatch` line says how long the key waited in
      the queue. Docs: keys.md, screen.md.
- [x] **A module's setting no longer puts back an Application settings switch on disk.** The host
      saved its whole store as read at start, so the next module setting or enable after a switch
      ticked in the Application settings tab wrote the old switch back (`flush_if_dirty`); the
      `[app]` table is now taken from the file at every save (`Store::save_keeping_app_on_disk`).
      And a module setting stored with another value than its default is written to the log the
      first time a run defines it: `[settings] com.platform.overlay calibrate is true as stored (its
      default is false)`, which the log header said of calibration while it was an application
      switch. Docs: settings.md, overlay.md.
- [x] **Komplete Kontrol defines its setting as it loads** (found moving calibration into the
      runtime). The open item here said its Settings… stayed greyed until a reload: the Installed
      tab enables Settings… from the settings a module had defined when the list was built, at
      start and on a reload, and Komplete Kontrol was read as defining its one only when its
      overlay first came to the front. That came from reading the code and was never seen live,
      and the reading was wrong: the define was in the module's `activate`, which the loader runs
      as the module loads (`populate_vm`), before the list is built. It now defines it at the top
      level, as the runtime does calibration, and keeps the value in a local that `define` and its
      `onChange` set; only its own VM reads it, in the overlays `activate` makes. Its code runs in
      Kontakt's VM too, and the setting stays one, under Komplete Kontrol's id. Komplete Kontrol
      0.1.1; test in `overlay_host_panel_tests.rs`. If the button is ever seen greyed, the cause is
      not where the setting is defined.
- [x] **The slow-reads switch removed; a key a busy module let go of goes to the next module or to
      the program** (the maintainer's decisions of 2026-10-05; pk-built.md). The Application tab's
      "Slow every text read by 2 seconds, for testing" is gone, and with it what only it used: the
      switch that is never stored (`Switch::persist`, `persists`), the delivery's hold (`hand_over`,
      `held`, `fire_at`) and its docs — settings should change what a user can tell, and nobody
      delays their own feedback; slow reads stay the automated tests' (held reads,
      `task::test_wait`). A key or a hotkey press that waited in its busy module's mailbox, whose
      registration the module released meanwhile and did not make again, was kept from the program
      in front for nothing: it is sent to that program as it was pressed — the same key, a modifier
      the press had and nobody holds now pressed around it, one the user holds left alone, none left
      down, a held key's folded repeats once more — while the window it was pressed in is still in
      front, the key itself is not held, no modifier and no screen reader's key is held that the
      press was made without, and no key-down has reached the program since the press
      (`backend::pass_on_strokes`, pure); and not while a hotkey of the application holds the
      combination, which the system would hand it to (`captures::pass_on`). Otherwise dropped, with
      the reason in the line (`captures::released_line`). Sent marked, so the hook or the tap lets
      it through first and no capture takes it again: Windows one `SendInput` with `dwExtraInfo`
      set, the scan code and extended flag the press had, a part taken released at once
      (`left_down`); macOS `CGEventPost` from a private event source with `kCGEventSourceUserData`
      set, on the press's keycode, the modifiers as `FlagsChanged` events and as flags
      (`keys::pass_on_events`). Held is the system's word or the hook's or tap's record of a
      key-down it swallowed (`KEPT_BACK`, the tap's suppressed bits). The order: the hook and the
      tap count every key-down they let go on (`LET_THROUGH`), marked ones and modifiers not, each
      captured key and hotkey press carries the count of its press (`Pressed::seq`), and a key is
      sent only while the count has not moved — else it would follow a key typed after it, a Delete
      on the line a Down moved to (the review of 2026-10-05, pk-review.md F1; a refusal the decision
      did not name, read from its "as if the module had never captured it"). The Mac asks the window
      in front afresh for it (F2). Asked again the same day (pk-final.md, question a), the
      maintainer put another module first: such a key or hotkey press goes to the next module that
      captures the combination in the window it was pressed in — the earliest capture of an enabled
      module scoped to that window or to everywhere, the hook's own rule (`captures::offered_to`,
      pure; `backend::in_scope`, shared with `capture_decision`) — through that module's mailbox as
      its own key, under its rules, and only with none to the program; the modules that let go of it
      travel with it (`Event::Key::let_go`) and never get it again, so it is at one place at a time
      and moves on at most once per module. F1 and F5 stay as built. A key whose capture went before
      it reached its free module is dropped as before. Tests: backend/mod.rs, windows.rs,
      macos/keys.rs, captures.rs (`offered_to` against `capture_decision` for every window and every
      set let go), key_scope_tests.rs (a busy module's keys and a hotkey, released before delivery,
      over the stub backend that records what it would send; two modules in one window, the first
      busy and letting go, the second free or busy or letting go as well); docs: keys.md, hotkey.md,
      module-runtime-and-lifecycle.md, ocr.md, module-manager.md.
- [ ] **NVDA check of this, before it is committed** — the Application tab: no "Slow every text
      read" switch any more, and its first sentence "Settings for the application itself. Each takes
      effect when its label says, and is remembered."; Kontakt and Melodyne as before. A key passed
      on needs a module busy while its overlay lets the key go, which cannot be set up by hand; when
      the log has `was passed on to the program in front`, the program had the key — NVDA said what
      it did with it — and a `was dropped: its registration was released meanwhile, and …` line
      names why one was not: `a key typed after it has reached the program first` when anything was
      typed between the press and its turn, `the screen reader's key is held down now` with Insert
      or Caps Lock held, `a hotkey holds its combination now`. A `went to …, the next module that
      captures it in the window it was pressed in` line says that another module, which captures the
      key there, had it instead (Avenger and Kontakt in one FX chain, in the check of B1b+M above),
      and no `was passed on` line follows for that key. Bring any of them.
- [ ] **The pass-on never ran on a Mac** (type-checked only): a key passed on arrives as pressed —
      Command+key as Command+key, with no modifier left down after it (a letter typed next is a
      letter, not a shortcut); the tap lets the posted events through (no capture fires for them, no
      `tap: captured` trace, no modifier tap from the posted `FlagsChanged` events, and the mark in
      `kCGEventSourceUserData` survives `CGEventPost` to the tap); a program takes the
      `FlagsChanged` events `CGEventCreateKeyboardEvent` plus `CGEventSetType` makes — if not, the
      flags on the key's own events alone, as `key_send` sends; and the HID system's key state
      (`CGEventSourceKeyState`) reports a key the tap swallowed as down while it is held. Posted
      from a private event source: two keys passed on one after the other (a held arrow's press and
      its repeat; Cmd+V and then Tab) both arrive, with no `the key is still held down` or `Cmd is
      held down now` line, and a letter typed while one is posted stays a letter; and the program
      still reads the posted modifiers. The window in front is asked afresh: a window that opened in
      front between the press and its turn — a dialog of the same application — gets nothing, the
      key is dropped (`no longer in front`). The order: a key typed between the press and its turn
      drops it (`a key typed after it has reached the program first`), nothing typed lets it go; a
      hotkey press passed on after its registration went is passed on, not dropped as overtaken —
      the tap counts the hotkey's own key-down before Carbon hands the press over (queue.rs,
      `push_hotkey`). A key whose combination a registered hotkey holds is not sent (`a hotkey holds
      its combination now`). The keypad's Enter: held, it is not passed on, and passed on it arrives
      as the keypad's Enter, not Return. Two modules in one window: a key a busy module let go of,
      which the other module captures there, goes to that module and is not posted (`went to …, the
      next module that captures it in the window it was pressed in`, no `was passed on` line) — the
      window the tap noted at the press is what decides, as for every capture. Provoked only where a
      module is busy while its overlay lets a key go: the log line is the evidence, or a probe step
      written for it.
- [ ] **The rest, as a mailbox per module** (b0-final.md): B5, the example (`examples/ocr`)
      reading in a callback, the tools, and CI failing on `could not wait here` from then on; step
      11, the waits of a read moved off the loop and the input barrier removed; and step 12, the
      places that still cannot wait raising, once B5 is in and that CI line has stayed green since.
- [x] **The review of K, B1a, B2 and B3** (2026-10-04, three reviews; kb-final.md). The overlays
      of one module share its key scope and menu flag: the one that comes to the front pins the
      scope, only the last to leave sets it back, and the flag is then what the ones in front say
      — Komplete Kontrol's standalone overlay, back before its Preferences overlay left, had its
      keys captured in every window. An overlay releases its keys before it unpins and pins before
      it captures. A hotkey for a busy module notes the window the hook or the tap compares keys
      with (`key_front`), not the Mac's accessibility question; its line says it waited only when
      it did. A held key's second hold keeps a repeat of its own; a controller button's release is
      never dropped at the limit; a queued event is pushed in the borrow that counted it; the
      budget's `[pump]` line comes once every 10 s with a count. A menu test that yields fails
      alone, and the tick goes on. An origin that raises as an overlay comes to the front is
      unknown for that epoch, so the activation is not stopped half way by the same raise. Windows
      says when `scope(true)` finds no window in front. Every
      non-test source file is searched for a module callback called outside its mailbox. Docs:
      keys.md, overlay.md's "Where a hook runs" (the `menuItem` and `onDone` row, the arbiter's
      call, `recognize` without "without a callback"), settings.md, timer.md, module-manager.md,
      building-an-overlay.md's anchors.
- [ ] **The HTML probe's page callbacks, when it is merged** (found reviewing B1a): its
      `onReady`, `onMessage` and `onClose` (webview_lua.rs) call the module's callback themselves.
      Through the mailbox, as `Event::Page` — `onMessage` an input — with `open` checking the
      module and the page's build. The source test in lib.rs that looks for a module's callback
      called outside its mailbox (`…_only_at_the_synchronous_places`) fails until they are. With
      it, docs/api/gui.md: its link to `#host-ocr-read` goes to `#host-ocr-recognize`, and the rules
      for a busy module (b1b-plan.md, section 4) — a page's events wait while a handler of its
      module waits for a reading, and run in order after it; a read in `onMessage` makes a waiting
      poll read of the same module an interactive one. And the lane of a page message after the
      user's activation, still open, can be heard then.

Follow-ups this work found and did not take on:

- [x] **The host's own `open` and `discard` of a busy module's events, run** (found reviewing
      B1a; done in B1b+M): each arm is one function the host and the tests' holders both run —
      `open_pad_in`, `open_hotkey_in`, `open_image_in` and `discard_image_in`, `open_snapshot_in`,
      beside the setting's `open_on_change` — with a busy-module test per arm (mailbox_tests.rs,
      key_scope_tests.rs).
- [ ] **A module disabled at load evaluates its overlays only at the next window or focus event
      after it is enabled** (found reviewing B3): the first evaluation of a binding is a one-shot
      `host.timer.after(0)`, which a disabled module does not get. Not heard: a module is enabled
      in the manager's window, and coming back to the plug-in is the event. Registering the
      runtime's trigger with `initial = true` would evaluate every overlay twice at every load;
      an `initial` trigger that only evaluates an overlay never evaluated would not.

- [ ] **A control that is a child and on the focus chain is asked about twice** (found building
      B3): `attachEmbedded`'s pattern search walks `host.window.controls()` and then the focus
      chain, so a focused plug-in control is met twice per evaluation. With a verdict kept that
      costs nothing; while `identify` answers nil, its count goes up by two, and the eighth "could
      not tell" comes after four evaluations. Asking each control id once per evaluation would
      keep the count to what the docs say.
- [ ] **The synchronous `host.screen` captures** (the maintainer's decision 3, the next work), and
      with them a `read` of a snapshot that was taken on the event loop. Perhaps later
      `snapshotAsync` as a wait, which would let Avenger's preset step be written in a line.
      A condition from step B3: where such a wait cannot wait — in the runtime's coroutine
      for the hooks asked on every scan, in a coroutine of the module's own, where Luau cannot
      stop — it captures on the event loop as today, for good, and never raises: Melodyne's
      `when` reads pixels (screen-frame-sharing-design.md, section 6).
- [ ] **A capture that hangs on Windows.** The read service's hang clock runs only while a
      recognition runs; on the Mac `SCK_TIMEOUT` bounds a capture, but the GDI capture has no
      bound. Since B1b+M a module waits for its reads, so a capture that never returns leaves it
      deaf — its keys wait behind the handler, and after 30 s the `has been busy` line says so —
      where it used to hold the whole application. Decide whether the hang clock covers the
      capture too.
- [ ] **A handler that reads again at once after `"failed"`** (B1b+M): during a hang a read asked
      again fails at once and is handed over on the next turn, so a handler that reads in a loop
      until it gets text turns once a tick and keeps its module busy for as long as the recogniser
      does not answer. docs/api/ocr.md says to return or ask again from a timer; decide whether the
      host should brake it too.
- [ ] **`O:stillHere` does not ask which program is in front** (found reviewing B1b+M): while an
      overlay holds its place over its own menu (`menus`, `opensMenu`), its origin is frozen, so a
      mark taken before a wait does not see a window switch during it. No handler in the tree waits
      over a held menu — Avenger never chooses a menu entry after a wait, and Kontakt checks the
      program in front itself — but a module that does would click into the new window. A check of
      the process in front in `stillHere` itself would close it.
- [ ] **Kontakt's check of the program in front, measured** (B1b+M): `invokeMenuItem` compares
      `host.window.active().app.pid` before the 250 ms, before the read and after it, taking a Qt
      menu for the same process. That a Qt menu answers with Kontakt's process on Windows and on a
      Mac, inside a DAW and standalone, is reasoned, not seen: the NVDA check above and the Mac
      session show it when load, save and reset still click.
- [ ] **UIA on a thread of its own** (a multithreaded apartment, as Microsoft's UI Automation
      threading notes recommend), its calls as waits. Measure first.
- [ ] **Luau's interrupt against a module that never ends.** Decide first whether there may be
      a limit at all.
- [ ] **A real thread per module** ("way A"), only if measurements after the mailbox and the
      `host.screen` waits ask for it.
- [ ] **An OCR stepper waits for a whole recognition before its step.** Photograph the value
      before the step instead of recognising it.
- [ ] **Three existing timers decide what is said** (the review's F12): ON:EAR's look after a tile
      click (18 times 80 ms, then "nothing at that position"), Avenger's `NAME_WITHIN` (2 s) and the
      runtime watch's deadline. They are no recognition on the loop and not part of this work, and
      not new named exceptions.
- [ ] **A read or a timer that a disabled module asks for** — from an `onChange` in the settings
      dialog — still arrives after a quick enable. For tasks this is closed (a wait in a disabled
      module ends the task), and a disable now drops what its own `onDeactivate` asked for.
- [ ] **Kontakt's 250 ms** before its file-menu read, to be replaced by the menu tests' word.
      Needs Mac data first.
- [ ] **A host form for "English where installed, otherwise the user's language".** Until it
      exists, Kontakt's header and file-menu reads and sforzando's wordmark (on the panel and on its
      own control) ask for `lang = "en"`, and on a Windows with no English text recognition
      installed every one of them is answered `"failed"`: Kontakt's load, save and reset say "Menu
      could not be read", with the reason in the log, and sforzando on Windows is found by its UIA
      pane alone. A list, `{ "en", (host.ocr.languages())[1] }`, would read there in the user's
      language, but `languages()` can hold the event loop up to 50 ms in the first moments after the
      start.
- [ ] **`LADDER_BUDGET`**, to be judged again now that it no longer protects the event loop.
- [ ] **Whether `ocr-warm-up` is still needed.** Measure first.
- [ ] **`host.window.focus` and `inputEpoch`** (a review of steps 1 to 3): `focus` turns over
      `host.epoch` only, and `inputEpoch` turns over when the window has come forward and the loop
      has heard of it — so a reading taken right after a `focus` can carry the value from before
      it, which docs/api/timer.md now says. Turning it over in `focus` itself (before the act, as
      `host.input.*` does) would re-read what Kontakt and Melodyne cache against it once more after
      a focus, so it is the maintainer's call, not this build's. `host.input.post` turning over
      neither counter is deliberate (docs/api/input.md, for Melodyne's variant presses) and stays.
- [ ] **A reload's second pass drops only timers, text reads, snapshots and tasks** (a review of
      steps 1 to 3): what the old VM's `onDeactivate` registers during the reload — a hotkey, a
      captured key, an `onChange`, a controller listener, an image search, an arbiter claim —
      survives the purge, keeps the old VM in memory, and some of it can call into it. The fix is
      the whole of `purge_module`'s registration purge in one helper, called before and after the
      `onDeactivate` calls; docs/module-runtime-and-lifecycle.md says what is dropped twice today.

## A module that runs too long or uses too much memory is stopped (2026-10-04)

G1 and G2 of the guard (a-final.md 5.1, g12-design.md; built in g1-built.md and g2-built.md, and
changed after the three reviews in g12-final.md). Every VM has a memory limit and every callback a
time limit; a module past either is stopped until the next start.

- [x] **`[limits] memory_mib`** (module-manifest): whole MiB, default 256, 256 to 2048; outside
      that the manifest fails like a bad `id` (start-up, hot-load, install review, `.zip`); a
      wrong type fails as `invalid type`; an unknown key in `[limits]` is named in the log. The
      install review says an amount above the default (for a `code_module`: in every VM its code
      runs in), the update review a changed one (not a reason for a review); an install whose
      unpacked `[limits]` differs from the reviewed one fails naming `[limits]`. Docs:
      module-package-format.md, whose message and log lines tests hold word for word.
- [x] **The VM's limit is the sum** over the code that runs in it (`populate_vm`:
      `collect_code_deps` first, then `vm_guard::describe`), set before any module code runs;
      every VM comes from `vm_guard::new_vm`, with 256 MiB at once (a source test holds it). A test
      sums it from manifests on disk: the diamond's shared dependency once, a data dependency, an
      absent optional one and one for another system not at all.
- [x] **The time limit** (vm_guard.rs, thread_cpu.rs): 2 s of the event loop's processor time
      (`GetThreadTimes` / `thread_info`, read by the `vm-guard` watchdog every 50 ms) or 10 s on
      the wall clock, counted per slice from the outermost entry — a handler's delivery
      (`task::start_thread`, `task::resume_parked`: one entry from the stretch that starts or
      resumes it until it ends or waits, its own `coroutine.yield`s answered inside it, so
      `while true do pcall(coroutine.yield) end` is one callback), `call_guarded`, a load step —
      so waiting never counts. mlua's interrupt is installed once per VM, saved and switched off;
      past a budget the watchdog writes it back into the VMs on the entry stack, once per slice
      (again each look while the round waits). A VM inside another's callback that has run less
      than half the budget that ran out itself lets the round pass (the caller takes it when the
      call returns); the VM that takes it samples its stack for 10 ms and at least 16 times, and
      the innermost function in every sample is the loop. That is a library's code — stopping the
      library and every VM that runs it in one step — only for a loop in Luau that spent the
      processor time with no host call running; the wall clock, a slow host call and the host's
      own Luau stop only the VM's module; so does a callback that returns before its samples are
      complete (one long host call, then its end), stopped as its entry ends. A load stops nobody
      else. The outer callback's clocks restart. The stop is sticky: every safepoint raises again.
- [x] **The stop** (stops.rs): when the outermost entry ends, `stops::mark` turns the modules off,
      gives their keys back, releases the mouse buttons they hold with `mouseDown` (a per-module
      record; also on disable, reload, rollback and exit), drops the keys and hotkey presses
      queued during the stall (Windows: `KEY_QUEUE`, `HOTKEY_QUEUE`, and a `WM_HOTKEY` stamped
      during the stall when it arrives, `DROP_OS_UNTIL`) — but answers the application's own
      reload key — and writes the `[guard]` and `[keys]` lines; each step that touches the OS is
      caught on its own, so the records are made whatever panics. Trips a panic left in the guard
      are marked at the end of the turn. `stops::settle` at the end of the turn does the rest of
      disabling and queues the dialog, the sentence (at most 255 characters, Windows'
      notification) and the rows' notes (callback, host call, place). `settings.toml` keeps it
      on; ticking it builds it afresh (`toggle_module`, `reload_module`) and turns it on; ticking
      the library of a library's stop turns the whole group on; a reload keeps it off and its
      report counts it. Details… in the manager shows the limit, its parts and the memory in use.
- [x] **`host_call!` in every binding** (one atomic swap in, one store out), so the messages name
      the host call the time ran out in; a source test holds every closure Luau can call to it,
      and another every call from the host into Lua to the known places. G3 needs the same
      marker.
- [x] **mlua pinned to `=0.11.6`** (host and module-manifest's dev-dependency), with
      `vm_guard_tests::mlua_pin`: the lock's versions, the field NULL in a new VM,
      `set_interrupt` writing that field and nothing else, mlua leaving it alone afterwards
      (`resume_error` included), the saved pointer calling the closure and its error ending a loop
      and a coroutine, the closure seeing the interrupted coroutine, no call for the collector's
      steps (armed, raising, a whole cycle of `gc_step` and allocations from Rust), the memory
      limit, and the error variants the guard matches. They run in both CI jobs' `cargo test`.
- [x] **The `[cpu]` lines**: the watchdog charges the loop's processor time to the VM on top at
      each look; a module above 10 % of a minute, one callback of 250 ms, and a summary per module
      at exit. A measurement; it changes nothing.
- [ ] **NVDA test of G1 and G2** with `tools/guard-probe`, `-lib`, `-b`, `-c`
      (`.\run-dev.ps1 -Build -Tools -Only guard-probe,guard-probe-lib,guard-probe-b,guard-probe-c`;
      `-Build`, or an older build without the guard starts): the keys and what to expect are in
      `tools/guard-probe/src/main.luau` (TESTS 0 to 18; every key ends by itself, so a build
      without the guard never hangs on one). Then the real modules on `.\run-dev.ps1 -Build
      -Release`: nothing may change, no `[guard] … stopped` line may appear, and the `[cpu]` lines
      and each module's load time go back here.
- [ ] **Set the `[cpu]` thresholds** (`CPU_LINE_PERCENT` 10 %, `CPU_LINE_CALLBACK` 250 ms, both
      provisional in vm_guard.rs) after the first measurements.
- [ ] **Decide: loads are held to the same 2 s / 10 s.** A legitimately slow load would now fail
      ("stopped while loading"). Check the first build's `[cpu]` lines for load costs, Avenger's
      above all.
- [ ] **Decide: data-only dependents of a looping library stay on.** Today every library is a
      `code_module`, so a loop in daw-hosts' own code turns off daw-hosts and nearly every overlay
      at once; a module that depends on one for data only would not run its code, and stays on.
- [ ] **Decide (changed after the reviews): turning a stopped module on builds it afresh**, as
      a reload does, instead of turning on the same VM as the stop left it — which the G1 build
      did, with the stopped VM's owed `onDeactivate` run first. A stop ends a callback anywhere:
      the overlay runtime's `_deactivate` sets `active = false` before it unregisters its hotkeys,
      so a stop between the two left hotkeys registered that the same VM, turned on again, gave
      back to the OS while the overlay was not in front — swallowing keys in every program. The
      owed `onDeactivate` is gone with it (the rebuilt VM has no overlay active), and so is the
      design's alternative test `an_overlay_activated_again_without_a_deactivate_comes_up_whole`.
- [ ] **Decide: ticking the library of a library's stop turns the whole group on** (each built
      afresh), and ticking one of the others turns on only it. The alternative was a tick per row;
      for the overlay runtime that is about a dozen.
- [ ] **Decide: a stopped module's settings do not run** — they are stored, wait in its mailbox
      rather than be refused by the stopped VM, and go with it when it is built afresh, which
      reads them as they are then; **a reload keeps it off** (built as designed).
- [ ] **Decide: controller presses during a stall** that ends in a stop: dropped like keys, or
      delivered late as now?
- [ ] **Decide: the budgets in a debug build.** The host's own work counts on a callback's
      clocks, and a debug build runs it many times slower (one full-region image match took 12 s
      there), so a callback the release build finishes in a fraction of a second can be stopped
      under `run-dev.ps1` without `-Release`. Built: the same 2 s / 10 s in both, and the docs and
      the probe say to judge real modules on a release build. Alternative: larger budgets under
      `debug_assertions`.
- [ ] **Keys pressed a moment before the stall** that waited in the same batch as the event that
      stalled are dropped with the stall's (`drop_queued_input` takes the whole queue); the docs
      say so. Dropping only keys stamped after the slice began would need the hook's `kb.time` in
      `Taken` and the slice's start in tick terms.
- [ ] **A memory error while `open` builds a handler's arguments** is reported as an error, not
      a stop; the module is stopped at its next event. Only a module that keeps its VM full and
      catches the error meets it (G1 open point).
- [ ] **mlua's nil error for a memory error**: Luau calls a protected call's error handler for a
      memory error too, and mlua's then returns nothing, so the error reaching the host is at times
      `RuntimeError("<nil>")` (seen through a Rust callback, in five runs of six of the pin test on
      Windows). The guard takes a nil error from a VM with less than 1/64 of its limit free for a
      memory error. Recheck when mlua is unpinned.
- [ ] **A sleep in the middle of a callback**: whether the wall clock (`Instant`) counts the
      time the machine was asleep was not measured; if it does, a callback running at the moment
      of a sleep would be stopped on waking. Rare (callbacks last milliseconds); the `[system]`
      resume line beside a `[guard]` stop would show it.
- [ ] **G3, a call that never returns** (a-final.md 5.1): the slice-age check, the hook's
      pass-through flag, the speech sender, a second start that offers to end the frozen copy.
      It builds on `vm_guard`'s `slice_seq`, `slice_start_ns` and `call` (the watchdog already
      arms such a slice and logs it; nothing fires until the call returns).
- [ ] **Stop by yield** in a handler instead of the sticky raise (a-guard.md 2.4, Y8), if a
      module's `pcall` seeing the stop ever matters.
- [ ] **A memory stop's sticky raise in a VM that is still full** can come as Luau's own
      out-of-memory rather than the stop's text, until the settle collects the VM's garbage: the
      module is stopped either way, and only an outer callback of the same VM sees the other text.
      Collecting at the stop itself would cost the event loop a full collection at once.
- [ ] **`guard-bench`** (g12-design.md 8), not built: the time from arming to the stop, idle and
      under load, as a packaged command the Intel Mac runner (which runs no tests) can run, with a
      `GUARD BENCH:` line in CI.
- [ ] **`HANDLER COST` on the Mac**: every stretch is an entry of the guard, now with the time
      limit's bookkeeping. On Windows, release, four runs: the figures are in
      module-runtime-and-lifecycle.md; the Apple Silicon CI job prints it too, and
      `GUARD ENTRY COST:` beside it.
- [ ] **macOS, unverified:**
      1. Keys typed during a 2 s stall reach the application once the tap times out, and the key
         queue `drop_queued_input` drains is empty. Probe TEST 12 in a Mac session.
      2. A Carbon hotkey pressed during a stall comes after it, or not at all. If it comes, it is
         delivered, not dropped; the drop rule would need its event time.
      3. `thread_cpu.rs`'s `thread_info` on the loop thread from the watchdog's: the test runs on
         Apple Silicon only, and the Intel runner runs no tests.
      4. The time from arming to the stop under load on an Intel Mac (`guard-bench`).
      5. The stop's sentence (`announce`: a notification where there is one, else speech when a
         screen reader runs), the Details… button and the row's note — wx controls like the
         others — never seen on a Mac.
      6. The sentence against VoiceOver reading the error window: on a Mac the sentence is
         speech (AVSpeech or VoiceOver's announcement) while VoiceOver reads the window that took
         the focus, so two voices may speak at once. The window says everything the sentence says.
      The memory limit is mlua's allocator, the same on both.

## The application's own keys, and what reload-all rereads (2026-10-10, from the maintainer)

- [ ] **A key that quits the application.** Windows: Ctrl+Shift+Alt+Win+Q, beside the reload key
      (Ctrl+Shift+Win+Alt+F5). macOS: a chord of the same family, chosen with the same care as
      the reload key (Cmd+Shift+F5, see RELOAD_HOTKEY_MACOS in crates/host/src/lib.rs):
      Control-Option chords are dead under VoiceOver, Cmd+Shift+Q is the system's Log Out and
      Cmd+Option+Shift+Q logs out without asking, so neither may be used; measure the candidate
      on the Mac before it is fixed. It quits the same way the tray's Exit does (modules
      deactivated, the recognisers released, the log closed), says one short sentence before it
      goes, and is listed beside the reload key in the log's start lines, the manager and the
      docs.
- [ ] **Reload-all must reread the manifests and resolve the module set again.** Observed by the
      maintainer: after editing a module's module.toml (its dependencies, for example) and
      pressing the reload key, the module dies with an error that does not say why.
      reload_module (lib.rs) rereads the module's own manifest, but reload_everything orders the
      rebuild by the dependency lists from BEFORE the reload, rebuilds only the modules that
      were loaded (a dependency added on disk is not discovered), and other state derived from
      the manifests at start (the memory limit summed per VM, capability grants, the code
      dependencies copied into a VM, settings schemas, the manager's rows) may stay stale. Find
      which of these produced the error, then make reload-all do what a restart does for the
      manifests: read every manifest first, resolve dependencies and order on the new graph,
      load new modules and drop removed ones, and when a manifest is wrong, say which module and
      which entry, spoken and in the error window.

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
