# Architecture and Feasibility Design: Cross-Platform Automation Platform (AHK/Keyboard-Maestro Successor)

*Principal architect synthesis from domain research (10 dimensions) and critical review. As of: 2026-06-21.*

---

## 1. Executive Summary

**The undertaking is feasible — but not as "AHK for all platforms" with a uniform feature set.** It is feasible as a cross-platform automation platform with *honestly tiered capabilities*, whose foundational architectural principle is not "parity" but "runtime capability detection." The uncomfortable truth: a central AHK core feature — reading foreign windows and controls (WinGet/ControlGet) — has **no equivalent in principle under Wayland** (no global window introspection by design) and on **macOS** is reachable only with persistent permission friction (four separate TCC authorizations, none of them grantable programmatically). In addition, three realities collide hard with naive expectations: (1) macOS permits the product **only outside the App Store** (the App Sandbox forbids the accessibility APIs entirely); (2) the global event tap on macOS is *silently* disabled on every self-update/re-signing — without a health watchdog the product appears "broken after update"; (3) the Windows behavioral profile (global hook + input injection + screen capture + FFI) is exactly the heuristic signature of stalkerware/RATs and, without code signing and AV whitelisting, will land in quarantine on first launch for many users. Windows reaches full AHK parity; macOS reaches functional Keyboard-Maestro parity at the cost of onboarding friction; Linux/X11 is high but **dying** (GNOME 50, Fedora 43, Ubuntu 25.10 remove X11 in 2025/26); Linux/Wayland is fundamentally constrained and fragmented. Whoever accepts this tiered capability model and builds capability detection as the architectural bracket from day 1 gets a viable product. Whoever promises "works the same everywhere" builds a reputation risk.

---

## 2. Recommended Tech Stack

### Main recommendations at a glance

| Building block | Main recommendation | Alternative (briefly) |
|---|---|---|
| **Host language** | **Rust** | C++ — only with a mandatory C++-only core SDK (not the case here) |
| **Script runtime** | **Luau** (via `mlua`, feature `luau`) | QuickJS-ng (via `rquickjs`) — if talent pool/JS syntax matters more than performance |
| **GUI toolkit** | **wxDragon** (Rust→wxWidgets, native controls) | Slint+AccessKit / Tauri (Webview) — if rich text/hypertext or a pure Rust build are a priority |
| **a11y foundation** | **Native OS a11y** (MSAA/UIA, NSAccessibility) via wxWidgets standard controls | AccessKit (only needed for custom-renderer toolkits) |
| **Screen-reader output** | **tts-rs** (`tts` crate, pure Rust) → `host.speech` | SRAL/prism (C++ FFI) — upgrade if macOS braille is needed |
| | *Since 2026-09: Windows took the upgrade. See [prism-speech-design.md](prism-speech-design.md).* | |
| **OCR** | **Native-first**: Apple Vision (mac), Windows.Media.Ocr (Win) | **PP-OCRv5 via RapidOCR-ONNX** as Linux default + determinism fallback |
| **Capture** | Win: WGC; mac: ScreenCaptureKit; Linux: X11 XShm + Wayland Portal/PipeWire | — |
| **Image search** | Custom **SIMD-NCC** (lightweight) | OpenCV `matchTemplate` as an optional plugin |
| **Input/Hooks** | **Direct OS FFI per platform** (no universal wrapper) | `enigo` (send only), `rdev` fork (listen/grab) as a transitional measure |
| **FFI/native libs** | `libloading` 0.8 + `libffi` 5.x | — |
| **Self-update** | **Velopack** + custom Ed25519/TUF signature layer | `self_update` + `cargo-dist` |

### Host language: Rust (clear recommendation)

**Rust wins over C++** for four reasons, in this order: (1) memory and thread safety *without GC* — critical for a long-running, privileged daemon with global event hooks (Microsoft/Google: ~70% of CVEs are memory-safety bugs); (2) a crate ecosystem mature for exactly this domain in *one* build graph (`windows`/`windows-sys` ^0.62 officially from Microsoft, `uiautomation` ^0.24, the `objc2` family, `x11rb`/`wayland-client`/`evdev`); (3) Cargo as a reproducible cross-build instead of CMake/vcpkg/Conan fragmentation; (4) with **espanso** there exists a fully Rust-built precedent for *exactly this product class* (launcher-daemon-worker, trait backends per OS).

**Honest caveat** (from the review): The "reproducible Cargo cross-build" advantage is partly relativized by the heavy native deps (OCR, CV, libffi) — see Sections 8 and 9. C++ remains sensible only if a vendor-specific C++-only SDK were central; that is not the case here.

### Script runtime: Luau (answer to "Lua-fast + modern")

The user's question "a language as fast as Lua, but with modern features" has a concrete answer in 2026: **Luau** (luau-lang/luau, MIT). It is Lua re-implemented in C++ (its own register VM, inline caching, fastcall builtins) and delivers exactly the sought profile: gradual/inferred types, clean OOP/closures *and* production-grade sandbox primitives (no FS/OS functions in the stdlib, hard memory limits, interruptible loops — by design). This makes it the only language that combines performance, modern features, and built-in sandboxing in one MIT embeddable — ideal for an AHK/KM successor that executes untrusted user code.

**Performance clarification (review-corrected):** Luau is *only in interpreter mode* on par with the LuaJIT *interpreter*. The LuaJIT *JIT* remains clearly faster; Luau native codegen is ~1.6x behind. For automation glue (hotkey snippets, UI control) this is irrelevant — the expensive operations are OS calls, not VM cycles. The original "close to LuaJIT" promise was too optimistic and is deliberately corrected here.

**Alternative QuickJS-ng** (JS/ES2025, via `rquickjs`): the strongest alternative when a broad talent pool and familiar syntax matter more than the last bit of performance — AHK users are imperative-familiar, JS lowers the barrier to entry. Trade-off: slower than Luau, more sandboxing effort of your own.

**Open decision (review):** `mlua` advertises async/Tokio but is historically deadlock-prone there; the `mluau` fork *removed async for that reason*. **Recommendation:** do *not* expose async in the module programming model (synchronous model with worker thread pool in the host), which settles the mlua-vs-mluau question in favor of the more stable variant. Non-default: PyO3/CPython (untrusted not sandboxable, heavy), Rhai (AST walker, ~2x slower than CPython), RustPython/Starlark/Wren (too immature/limited).

### GUI toolkit: wxDragon (native wxWidgets controls)

**Client decision (2026-06-21), backed by hands-on experience:** He has already shipped screen-reader-accessible wxDragon apps for Windows + macOS (e.g. [rabbit](https://github.com/timtam/rabbit) — a REAPER accessibility installer in Rust + wxDragon, tested against NVDA/JAWS/Narrator/VoiceOver, with a `crates/` workspace + Fluent i18n + signed CI). **wxDragon** (Rust binding to wxWidgets, MIT/Apache; wxWidgets under the wxWindows Licence — uncritical for open source) uses real OS controls whose accessibility the operating system provides automatically: Windows MSAA/IAccessible via `wxAccessible`, macOS NSAccessibility via the native Cocoa controls.

Deliberately accepted trade-offs: (a) a mandatory C++/CMake toolchain (+ LLVM `libclang`) and a vendored wxWidgets build, binaries ~3–15 MB — no pure Rust build; uncritical, since the native-FFI decision requires a C build chain anyway. (b) The OS-inherited a11y applies only to *native* controls — owner-drawn/custom widgets (e.g. dark-mode `wxCheckBox`, `wxGrid`) are not automatically accessible and are avoided or worked around individually. (c) wxDragon is pre-1.0 with a small maintainer circle → version pinning + custom screen-reader smoke tests.

Runtime UI from Luau: viable via programmatic creation and `XmlResource::load_from_string` (Luau generates XRC).

**Rejected alternatives:** *Slint + AccessKit* (pure Rust, enforcement via DSL — but AccessKit has real gaps for rich text/hypertext, native a11y only emulated) and *Tauri/Webview* (most mature ARIA a11y, but web look + larger runtime). Both remain fallback options should rich-text editing become a core feature.

### OCR strategy: hybrid (native-first, ONNX fallback)

There is no engine that is simultaneously optimally "efficient AND exact" on all three platforms. Stable internal trait abstraction `recognize(image, langs) -> [TextBox{text, bbox, confidence}]` with:
- **Default = native engine** (energy-efficient, GPU/NPU, no model download, top quality): macOS = **Apple Vision** (`objc2-vision`), Windows = **Windows.Media.Ocr** (`windows-rs`).
- **Fallback + Linux default = PP-OCRv5 via RapidOCR-ONNX** (Rust: `oar-ocr`/`paddle-ocr-rs` on `ort`/onnxruntime). CER ~0.10 (better than Tesseract ~0.18), 40+ languages incl. CJK, Apache-2.0, delivers *deterministic, reproducible* results across platforms.
- Engine overridable per script (`force_engine="onnx"`, `pin_model_version`) for portable modules. Tesseract only as an optional plugin for exotic languages. Cloud OCR never default (data privacy).

### Vision/Capture, Input, FFI

- **Capture** (uniform frame format BGRA8 + stride + DPI/scale + monitor origin): Win = **Windows.Graphics.Capture** (crate `windows-capture`), Desktop Duplication as a monitor fallback, PrintWindow as legacy. mac = **ScreenCaptureKit** (CGWindowList has been removed since macOS 15). Linux = X11 XShm + Wayland `ashpd` (Portal) + `pipewire-rs`.
- **Image search:** custom **SIMD-NCC + image pyramid + ROI** as a lightweight default (or FFI to `Fastest_Image_Pattern_Matching`, BSD-2); OpenCV `matchTemplate` only as an optional plugin (too heavy for "lightweight").
- **Input/Hooks:** **direct OS FFI per platform** instead of trusting wrappers. Win = `SendInput` + `SetWindowsHookEx(WH_KEYBOARD_LL/_MOUSE_LL)` + `RegisterHotKey`. mac = custom **CGEventTap FFI** (`.defaultTap` for suppression/remapping) with a re-enable watchdog. Linux = X11 (XTEST/XRecord) + Wayland (`libei`/Portal for sending, `uinput` as a root bypass). `enigo` (send only) and `rdev` forks as a transitional measure.
- **FFI:** `libloading` 0.8 (module loading) + `libffi` 5.x (generic C-call engine, CIF at runtime) = AHK `DllCall`/ctypes equivalent. No custom JIT FFI (LuaJIT level) in v1.

---

## 3. Overall Architecture

### Layered model (boxes and relationships)

The system is a **Cargo workspace** with a multi-process runtime model.

**Processes (top level, three boxes):**
- **`Launcher`** — starts/monitors the other processes, coordinates self-update and restart (espanso pattern with exit codes 50/51 for a targeted restart). Relationship: *spawns and monitors* `Daemon` and `Worker(s)`.
- **`Daemon`** (privileged, always-on) — contains the **hot path**: global hotkey/event listener, event matching, capability registry. Runs in the **main thread with an OS-native run loop** (NSApplication/CFRunLoop on macOS, message loop on Windows — both mandatorily main-thread-bound). Relationship: *holds* the OS backend layer; *delegates* only hits/actions (not every event) via IPC to workers.
- **`Worker(s)`** (isolated, untrusted) — execute script modules (Luau VM) and possibly native FFI modules. Relationship: *receive* actions from the daemon via IPC, *call back* into the host API.

**IPC edge** between the processes: Named Pipes (Windows) / Unix domain sockets, serialization via `serde`/`bincode`, **semver-versioned together with the host API** (handshake with version reconciliation on connect).

**Within the Daemon/Core (layers, top to bottom):**

1. **Core engine (OS-agnostic)** — event dispatcher, module lifecycle manager, config/state store, **capability registry** (the central bracket). Calls downward into the OS backend abstraction via traits.
2. **OS backend abstraction (traits)** — `WindowProvider`, `ElementProvider`, `Injector`, `HotkeyListener`, `CaptureProvider`, `ClipboardProvider`, `OcrEngine`. Implemented by one backend per OS via `#[cfg(target_os)]`: `WindowsBackend` (windows-rs/UIA), `MacBackend` (objc2/AX/CGEventTap/SCK), `LinuxX11Backend` (x11rb/AT-SPI/XTEST), `LinuxWaylandBackend` (libei/Portal/uinput/AT-SPI). Each backend *fills* the capability registry with what it can really do on *this* machine.
3. **Script runtime** — Luau VM (in the worker). Consumes the **host API** (curated, capability-gated surface), *never* the OS backends directly.
4. **Module/plugin system** — two classes: **script modules** (Luau, in-VM sandboxed, default) and **native FFI modules** (libffi, gated, signature-required, in an isolated worker). Every module has a **manifest** (declares `engine_api` range + required capabilities).
5. **GUI** — Slint, runs in the main-thread run loop *together with the hotkey listener*. Exposes its own a11y tree via AccessKit (provider side — separate from the foreign-app introspection!).
6. **Update** — Velopack + signature layer. Steers the launcher (coordinated restart), verifies Ed25519-signed manifests.

**Threading rule:** main/GUI thread with OS run loop for hotkey listener + GUI + CGEventTap (all run-loop-bound); Tokio worker thread pool for script/IO work. **Hot-path events stay in the daemon** — never per event across the process boundary (latency).

### Process/isolation model

**Multi-process is the foundational model** (not single-process): (1) crash isolation — a crashing user module does not take down the global hotkey listener; (2) clean permission boundaries (macOS TCC per bundle/process); (3) privilege separation (privileged `uinput` helper on Linux separated from the script worker). **Trade-off:** multi-process IPC and hotkey latency pull in opposite directions → mitigation: event detection/matching in the daemon, only hits to the worker. The final worker granularity hangs on the performance budget. **Refinement (decided):** script (Luau) modules are *not* one-process-per-module — one daemon process loads all enabled modules concurrently, one lightweight VM each, with dynamic enable/disable; only the untrusted-native-FFI tier runs out-of-process. See [module-runtime-and-lifecycle.md](module-runtime-and-lifecycle.md).

---

## 4. Feature × Platform Feasibility Matrix

Values: **Full** / **Limited** / **Impossible**.

| Capability | Windows | macOS | Linux (X11) | Linux (Wayland) |
|---|---|---|---|---|
| **Enumerate windows** | Full — EnumWindows/Win32 | Full — CGWindowList (title needs screen-recording perm.) | Full — EWMH `_NET_CLIENT_LIST` | Limited — only compositor-specific (wlr/ext-foreign-toplevel; GNOME only via shell ext.); no standard |
| **Read controls** | Full — UIA + MSAA fallback | Limited — AX API + TCC accessibility; Electron delivers an empty AX tree; no mature Rust crate | Full — AT-SPI2 (D-Bus) | Limited — AT-SPI2 runs, but no surface mapping/global geometry |
| **Image search (ImageSearch)** | Full — WGC capture + NCC | Limited — SCK + screen-recording perm. (weekly re-prompt from Sequoia) | Full — XShm, consent-free | Limited — only Portal+PipeWire with consent; no free polling |
| **OCR** | Full — Windows.Media.Ocr / ONNX | Limited — Vision API top, but capture needs perm. | Full — ONNX on XShm frame | Limited — engine ok, image acquisition Portal-gated |
| **Color/pixel detection** | Full — from cached frame | Limited — like capture (TCC) | Full — XGetImage | Limited — like capture |
| **Input simulation** | Full — SendInput | Limited — CGEventTap/Accessibility + input monitoring; Secure Input blocks | Full — XTEST | Limited — libei/Portal (prompt) or uinput (root) |
| **Global hotkeys** | Full — RegisterHotKey/WH_KEYBOARD_LL | Limited — CGEventTap (permissions + silent-disable race) | Full — XGrabKey | Limited/Impossible — only GlobalShortcuts portal (KDE good, Hyprland broken, GNOME/wlroots partly without) |
| **FFI / native libs** | Full — LoadLibrary/libffi | Limited — Hardened Runtime blocks unsigned dylibs without `disable-library-validation` | Full — dlopen | Full — dlopen (display server irrelevant) |
| **GUI / accessibility** | Full — AccessKit→UIA | Full — AccessKit→NSAccessibility | Full — AccessKit→AT-SPI | Limited — a11y keyboard/shortcuts compositor-dependent (fully only from GNOME 48) |
| **Self-update** | Full — Velopack + signature | Full — Velopack/Sparkle (notarization mandatory, signature stability critical) | Limited — tarball+updater (AppImage self-update barely, Flatpak sandbox breaks core function) | Limited — like X11 |

**Core message:** Windows = consistently Full. macOS = consistently "Limited" *only because of permissions* (functionally present). Wayland = structurally limited for *all* automation core features — and "Linux/Wayland" is really ≥4 targets (GNOME/KDE/Hyprland/wlroots) each with its own capability matrix.

---

## 5. Accessibility-by-Design

**Principle: the unsafe variant must not even be expressible — a11y goes from opt-in to opt-out.** With the wxDragon choice, enforcement shifts from a toolkit DSL (Slint) into the **Luau GUI API layer**: the script-facing binding makes the accessible label a mandatory argument and sets it on the native control; owner-drawn/custom widgets are not even exposed without an explicit accessible name. Native standard controls deliver the OS a11y automatically. Conceptually still four mechanisms (formulated below against the original Slint example, transferable in spirit to the API binding):

1. **Accessible name = mandatory constructor argument** of every interactive widget. A button/input without `accessible-label` (or without an explicit `accessible-label: ""` *plus* a `decorative` marker with justification) produces a **compile error**. The escape hatch is mandatory, otherwise developers circumvent the rule with dummy strings.
2. **Default role automatically** — no optional `role`; every widget gets the correct AccessKit role by construction.
3. **Focus/tab order** is *derived* from the layout tree; all interactive elements are keyboard-focusable by default.
4. **WCAG contrast check** in the layout compiler (4.5:1 text / 3:1 large type, AA) for static colors; for dynamic/theme colors *at runtime*.

**Concretely:** every custom widget *must* write `Role` + `Name` + `Actions` into the AccessKit push tree. ID stability across frames is mandatory (otherwise the screen-reader focus breaks). **Warning:** AccessKit delivers *no* automatic a11y — it is only as complete as the toolkit fills the tree.

---

## 6. API Versioning & Self-Update

**Strict separation:** (A) host binary delivery ≠ (B) module API contract.

### (B) API versioning (module protection)
- **Manifest field:** `engine_api: ">= X, < Y"` (SemVer range). The engine exposes `api_version` as an integer level (Neovim style) *and* a semantic `Major.Minor`.
- **SemVer policy:** additive changes = **minor**; breaking changes = **new major**, never silently. Incompatible modules are **rejected on load instead of crashing** (capability check before execution).
- **Adapters/shims:** at least N=2–3 major versions in parallel via a shim/adapter layer; clear deprecation policy. **Manifest V2→V3 lesson:** hard sunsets without a long overlap window destroy the ecosystem.
- **Script vs. native:** script modules = simple contract (no ABI risk); native FFI modules = strict ABI contract with an exact `api_version` match (OBS model).
- **CI enforcement:** API snapshot/contract tests (`cargo-semver-checks`/`cargo-public-api`) as a CI gate.

### (A) Self-update with per-OS signature
- **Base:** **Velopack** (MIT, all three OSes, delta updates). Additionally **custom Ed25519-signed release manifests**; from broader distribution **full TUF** (key rotation, rollback/freeze protection).
- **Windows:** OV code signing (Authenticode); EXE replacement via rename/restart.
- **macOS (critical):** Developer ID + **notarization** + stapled ticket. **Signature stability is operationally critical:** re-signing can break TCC permissions *and* CGEventTap → preserve a stable bundle ID/team ID across updates.
- **Linux:** signed tarball + custom updater; Flatpak optional, but its sandbox breaks the core function.

**Common thread:** self-update couples directly with the most fragile macOS mechanism — without a stable signature + health watchdog + permission persistence, this is a "broken after update" reputation killer.

---

## 7. Security Model

**Principle: modules = code execution. Default-deny, tiered trust levels, capability gating.**

### Three trust levels
1. **Script modules (default, Luau)** — in-VM sandboxed. See *only* the curated **host API**, never OS backends directly.
2. **Host-API capability gating** — every OS automation function (hotkeys, input injection, capture, foreign-window introspection, native FFI) is a **gated capability** that the user must grant explicitly per module (the manifest declares required capabilities, default-deny). This is the actual security boundary.
3. **Native FFI modules (highest risk)** — in-process there is no effective sandbox. Therefore: gated `native`/`ffi` capability; *signed/trusted* modules (Ed25519 + trust store) without a prompt; *untrusted* native modules in a **separate worker process with an OS sandbox** (Windows AppContainer/Job Object, macOS Seatbelt, Linux Landlock+seccomp+namespaces).

**WASM/WASI** as an optional "safe tier" for pure logic — but **no substitute** for native FFI (WASM cannot call native libs directly).

### macOS dilemma
Notarization → Hardened Runtime → Library Validation blocks unsigned user dylibs → opt-out (`disable-library-validation`) opens a dylib-hijacking vector. **Clean resolution:** default = *only script/WASM modules*; native FFI only as a gated, signed, out-of-process-isolated capability (on macOS, possibly only modules signed with your own team ID).

### Anti-abuse
The product *is* potentially a keylogger/RAT framework. Key management, revocation, default-deny capabilities, and a module trust store are not only security but **trustworthiness toward AV/EDR**.

---

## 8. Biggest Risks & Open Decisions (prioritized)

**P0 — Existential / scope-defining:**
1. **Foreign-window introspection has no equivalent on Wayland** (by design), on macOS only with persistent friction → "parity" untenable; capability detection + an honest feature matrix mandatory.
2. **CGEventTap silent-disable race × self-update** = "broken after update" → stable signature + 5s health watchdog (`tapIsEnabled`/`tapEnable`) + permission persistence mandatory.
3. **macOS App Sandbox forbids AX APIs entirely** → the Mac App Store is permanently excluded as a distribution channel. Only Developer ID + notarization + non-sandboxed.
4. **Windows behavioral profile = stalkerware signature** → without code signing + AV-vendor whitelisting + reputation building, the product lands in quarantine on first launch. Go-to-market risk.

**P1 — Promise conflicts:**
5. **"Extremely lightweight" × OCR+ImageSearch+FFI incompatible** in the default binary (onnxruntime + ~50–80 MB OCR models + OpenCV + libffi build chains) → these features as *optional, loadable, signed plugins*.
6. **License risks:** Slint (GPLv3 contagion when statically linked into closed source!), Tesseract/Leptonica/tessdata provenance, OpenCV-Contrib, PaddleOCR models → a consolidated license-compatibility matrix for the entire dependency closure *before* finalization.
7. **Code-signing cost/logistics:** Apple Developer (99 USD/year); Windows OV/EV with **HSM/token requirement since 2023** (complicates CI signing); macOS notarization needs Apple-hardware runners. → start with **OV**, EV only on real AV/EDR quarantine.

**P2 — Platform/detail:**
8. **Wayland compositor fragmentation** makes "Linux" ≥4 parallel products (GNOME/KDE/Hyprland/wlroots), each with its own test load.
9. **macOS Sequoia weekly re-prompt** (screen recording) permanently degrades *all* pixel-based features.
10. **Missing performance budgets** block worker granularity, OpenCV-vs-SIMD, ONNX bundling.
11. **Energy/background on macOS unplanned:** App Nap, QoS, SMAppService (login-item approval from Ventura).
12. **Further gaps:** GDPR/telemetry; IME/keyboard layouts (AZERTY/Dvorak/dead keys/CJK); config/state migration; observability/debugging across process boundaries.

---

## 9. Recommended Approach / MVP Cut

**Guiding principle: walking skeleton on the easiest platform, capability detection from day 1, heavy features as later plugins.**

- **Phase 0 — Foundational decisions** (before any line of product code): fix the eight "must-answer" points together (plugin model, performance budgets, capability matrix, macOS distribution, security model, license matrix, signing logistics, runtime/async).
- **Phase 1 — Walking skeleton (Windows-first):** global hotkey → Luau script in the worker → SendInput action, through the full layering (Launcher/Daemon/Worker, IPC, capability registry, backend trait) + Slint GUI with enforced a11y. Proves the multi-process model, IPC versioning, and the host-API capability boundary end-to-end.
- **Phase 2 — macOS (biggest risk first):** Developer ID + notarization + Hardened Runtime + non-sandboxed; CGEventTap with re-enable watchdog; 4-permission onboarding; SMAppService; self-update with a *stable* signature (smoke test: an update must not break permissions).
- **Phase 3 — Capture/OCR/image search as plugins:** native engines (WGC/SCK, Windows.Media.Ocr/Vision) + ONNX fallback, *loadable*. DPI/scale first-class.
- **Phase 4 — Linux (Wayland-first, X11 as a bonus):** Tier-1 = KDE + GNOME, best-effort = Hyprland/wlroots; libei/Portal + AT-SPI + ScreenCast; uinput fallback as "degraded, needs root".
- **Phase 5 — Native FFI + module registry:** `DllCall` equivalent (libloading+libffi) as a gated, signed, out-of-process-isolated capability; trust store + signature infrastructure.

**Platform order:** Windows (validates the architecture cheaply) → macOS (decides viability) → Linux (highest fragmentation, last).

---

## 10. Recommended Next Clarification Questions

1. **Plugin model (most consequential single decision):** native dylibs (real `DllCall`) or a pure embedded script/WASM VM? Determines the entire macOS security/notarization profile. *Recommendation: script/WASM as default, native FFI only gated/signed/isolated.*
2. **Hard performance/footprint budgets:** target values for hotkey latency (ms), baseline RAM consumption, default binary size? OCR/ImageSearch/FFI in the default binary or as plugins?
3. **Parity promise / capability matrix:** Wayland introspection accepted as a hard gap? Which compositors Tier-1 (KDE/GNOME) vs. best-effort (Hyprland/wlroots)?
4. **Distribution channels:** Developer ID/tarball instead of Mac App Store / Flatpak-Snap confirmed? **Enterprise/MDM need** (PPPC profiles could grant macOS TCC in advance — a strong B2B differentiator)?
5. **License / closed source:** product closed source? Then clarify Slint's GPLv3/commercial (vs. permissive egui).
6. **Script language final:** Luau (best performance+sandbox) or QuickJS-ng (familiar JS syntax, larger talent pool)? Expose async in the module model? *Recommendation: Luau, synchronous model.*

---

## 11. Decisions Made (2026-06-21) and Their Consequences

Bindingly chosen:
- Host: **Rust** · scripting language: **Luau** (`mlua`/`luau`, synchronous model) · GUI: **wxDragon** (native wxWidgets controls, confirmed by the client through hands-on experience)
- Plugin model: **native FFI as a gated capability** (not only script/WASM)
- Scope: **Windows + macOS first**, Linux later
- License: **open source** (Slint GPLv3 thereby unproblematic)

Consequences that are no longer optional now:

1. **Native FFI moves security to the front.** Because untrusted modules may load native code, the following must exist from the early walking skeleton: (a) an out-of-process sandbox worker (Windows: AppContainer/Job Object with a restricted token; macOS: `sandbox_init`/Seatbelt profile), (b) a module trust store with Ed25519 signature verification (trusted modules without a prompt, untrusted only in the sandbox worker), (c) a default-deny capability manifest. The `native`/`ffi` capability is the highest-tier one and requires explicit, persistent user approval per module.

2. **The macOS FFI policy must be decided explicitly.** Hardened Runtime + Library Validation block unsigned user dylibs. Recommended policy: on macOS, *no* arbitrary user dylibs in the main process; native modules there are either (a) distributed signed/notarized with your own team ID, or (b) loaded exclusively in the separate sandbox helper — avoid `disable-library-validation` in the main process (opens dylib hijacking). This keeps the notarization of the core product clean.

3. **AV/EDR risk rises through FFI.** Hook + injection + capture + FFI together = a strong stalkerware heuristic. Plan countermeasures now: OV code signing from the start, reputation building, AV-vendor whitelisting submission; open source additionally helps here (auditable code).

4. **Choose the open-source license of the host.** Slint statically linked ⇒ the host must be GPLv3-compatible. Important: the script/IPC boundary separates modules as independent works — third-party script modules can carry their own license; the host's GPL does not force a module license. For native FFI modules, the linking/derived-work question must be reexamined → another reason to load native modules in the separate helper (IPC boundary) rather than in the main process.

5. **Linux later ≠ Linux ignored.** Continue designing the backend traits for 4 targets (Win/macOS/X11/Wayland); the Linux backends remain `unimplemented` for now, and the capability registry reports them cleanly as "not available". This keeps the later Linux entry additive instead of an architectural break.

6. **The wxDragon GUI choice has two architectural consequences.** (a) The wxWidgets GUI event loop occupies the main thread with its own run loop — the global hotkey listener or macOS `CGEventTap` (both run-loop-bound) share the same thread/the same loop ⇒ GUI + input listener live in the same process/main thread (daemon or a dedicated GUI+input process). (b) a11y-by-design is enforced in the Luau API layer (a mandatory label sets the native control name), not via a compile-time DSL.

7. **Screen-reader/speech output = tts-rs** (`tts` crate, pure Rust, permissive), encapsulated as a `host.speech` capability — module authors speak via `host.speech.output(text)`, without DLL wiring. Windows: Tolk→NVDA/JAWS (incl. braille) or WinRT; macOS: AVFoundation/AVSpeechSynthesizer (direct TTS, no braille routing — macOS offers no clean public API for it; prism/SRAL would have the same limit). Replaces prism (C++23/FFI) and ReaHotkey's NVDA-Controller/SAPI path and resolves the open macOS speech question. The backend remains swappable behind the host API (SRAL/prism or an NSAccessibility announcement path as an upgrade, should macOS braille become a hard requirement).

> **Windows took that upgrade in September 2026**, and for reasons this paragraph did not
> anticipate. It was not braille that decided it — though NVDA turns out to offer that too —
> but that the Tolk path threw away every failure, so a screen reader closing mid-session
> left the application silent with nothing in the log, and that it spoke through the
> speakers at anybody running this without a screen reader. macOS keeps `voiceover.rs`,
> which is better than prism's own VoiceOver backend. See
> [prism-speech-design.md](prism-speech-design.md).

With this, the open points 1, 5, 6 and the macOS speech question from Section 10 are decided; what remains open is primarily the **performance/footprint budgets** (10.2) and the **macOS distribution/MDM details** (10.4).

---

*Methodology: synthesis from 10-dimension domain research + critical review. Where reports contradicted each other, the demonstrably more realistic statement was adopted and marked — in particular: macOS AX crate maturity (custom build needed), Luau performance (interpreter, not JIT level), Linux strategy (Wayland-first, X11 is sunset), and the incompatibility of "lightweight" with heavy feature deps in the default binary.*
