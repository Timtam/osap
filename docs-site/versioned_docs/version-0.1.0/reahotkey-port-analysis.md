# Porting ReaHotkey Overlays as Modules: What It Takes to Run Them on macOS

*Principal-architect analysis (multi-agent: 3 framework agents + 3 overlay-inventory groups + synthesis, against the real ReaHotkey codebase under `c:/scripts/ReaHotkey`). Additional basis: [architecture-feasibility-study.md](architecture-feasibility-study.md) + [host-api-capability-catalog.md](host-api-capability-catalog.md). As of 2026-06-21.*

The most important observation first: our capability catalog did **not** emerge in a vacuum — it was already cut as a ReaHotkey vertical slice. `host.overlay`, `host.speech`, `host.sound`, `host.hotkey`, `host.window`, `host.input`, `host.screen`, `host.ocr` all already carry an MVP checkmark. This analysis confirms the cut against the real source mechanisms in ReaHotkey and sharpens the point where the effort estimate truly hinges: the separation of **framework** and **content**.

## 1. How ReaHotkey Works

Audio plugins (VST/AU) render their GUI themselves (OpenGL/Canvas/JUCE/VSTGUI) and expose **no usable accessibility tree**. ReaHotkey's core idea inverts this: **instead of repairing the missing a11y tree, it builds its own virtual, self-voicing control tree *on top of* the plugin** and speaks directly to the screen reader API. No MSAA/UIA tree — a parallel-running, declarative "shadow UI."

Building blocks:
- **Declarative control tree** (`Lib/AccessibilityOverlay.ahk`): `AccessibilityControl` (root, global registry) → `AccessibilityOverlay` (container + navigation machine: children, focus index, tab routing, hotkey dispatch). Families `FocusableControl` (focus + speak) and `ActivatableControl` (+ activation). Lifecycle hooks (PreExec/PostExec/Pre/PostSpeech) and a `States` map (state → spoken text).
- **Six interaction backends** (the only OS-bound layer): **Custom** (callbacks), **Hotspot** (coordinate click + `PixelGetColor` for state), **Graphical** (`ImageSearch` reads state; also slider via `MouseClickDrag`), **Native** (Win32 `ControlGetHwnd`/`ControlFocus`/`ControlClick`), **UIA** (`UIA.FindElement`→`SetFocus`/`Click`), **OCR** (Tesseract/UWP reads a text region). Plus **PassThrough** (routes hotkeys through to the host app).
- **Detection** (`Lib/ReaHotkey.ahk`): 500-ms state machine + detection cascade — host mode via `WinActive` (reaper.exe/Ableton), plugin control via `WinGetControls` + regex, then plugin-specific `CheckerFunction` (title regex, REAPER FX `ListViewGetContent`, UIA, image fallback). **All coordinates are compensated relative to the plugin control's position** (`ControlGetPos` → offset applied to every coordinate).
- **Output** (`Speak()`): fallback chain JAWS-COM → NVDA controller DLL → SAPI. Sound feedback today is *not* in the core, but per-plugin via `SoundPlay`.

**In one sentence:** ReaHotkey is a declarative engine for virtual, self-voicing controls whose values and actions are coupled to a non-accessible plugin via six interchangeable backends — coordinate-relative calibrated, with screen reader speech output.

## 2. The Decisive Separation: Framework vs. Content

**This is the crux of the effort estimate. Anyone who fails to draw it cleanly estimates wrong by orders of magnitude.**

### Framework = build ONCE, then portable

Pure logic, zero OS binding, translates 1:1 into our `host.overlay` runtime + Luau:
- The **control taxonomy** (Button/Toggle/Checkbox/Edit/ListBox/StaticText/Tab/TabControl + all backend variants).
- The **focus/activate lifecycle model** (hooks, `States` map → spoken text, "same control → speak state only").
- **Navigation/tab routing/hotkey dispatch**, **speech composition** ("Label Type Value State Hotkey"), speech queue.
- The **coordinate compensation *model*** (offset = plugin control origin + relative coordinate). The math is portable; only the origin source changes (Win32 `ControlGetPos` → macOS `AXFrame`).
- The **detection *strategy*** as a multi-stage pipeline (Title → AX/Control → ListView/Table → Image/OCR).

This is the stable skeleton — built once as a capability-gated host API + Luau builder, for Windows *and* macOS.

### Content = recalibrate PER PLUGIN for macOS (NOT portable)

Scales with the **number of plugins**, not with the codebase:
- **All pixel coordinates** (Hotspot X/Y, OCR regions, slider start/end, tab click points) — measured per plugin, valid only for the exact Windows build/skin/scaling.
- **Reference images** (Graphical/FindImage PNGs) and **pixel colors** (Hotspot On/Off/Checked).
- **Detection constants**: title regex, Win32 class names (`^VSTGUI[0-9A-F]+$`, `JUCE_…`), the `reaperPluginHostWrapProc1` heuristic, `SysListView321` reading.

**Why this *re-incurs* on macOS rather than just being "adapted":**
1. **AU vs. VST3** — different binary, different window layout, different pixel geometry. Coordinates not transferable.
2. **REAPER hosting is different** — the native/bridged mode distinction does not exist on macOS (AU/VST3 run in-process as `NSView`); the `reaperPluginHostWrapProc1` heuristic has no equivalent.
3. **No `ahk_class`/`ClassNN`** — macOS has an AX role tree; the class-name-based detection must be re-measured per plugin against AX role/identifier.
4. **Retina/points-vs-pixels** invalidates coordinates + image assets.
5. **The REAPER FX list is not a `SysListView32`** → replace the detection signal with AX table or OCR.

**Consequence:** Framework = finite, one-time investment. Content = a linear function over the plugin count, in large part *manual measurement on the Mac* + *permission authorization*. "Port of ReaHotkey" = **port one framework + re-measure N plugins.** The second number dominates from the second dozen overlays onward.

## 3. Minimal Host API — Reconciliation with Our Catalog

Coverage nearly complete (✅ = catalog MVP, ○ = catalog post-MVP):

| Capability | Namespace | In catalog |
|---|---|---|
| Virtual control tree + navigation | `host.overlay` | ✅ MVP |
| Screen reader / speech output | `host.speech` (tts-rs) | ✅ MVP |
| Sound feedback | `host.sound` | ✅ MVP |
| Global + contextual hotkeys | `host.hotkey` | ✅ MVP |
| Window/control detection + geometry | `host.window` | ✅ MVP |
| Coordinate click/mouse/drag/scroll | `host.input` | ✅ MVP |
| Keypress / pass-through | `host.input` | ✅ MVP |
| Image search (state from frame) | `host.screen` | ✅ MVP |
| OCR (text region) | `host.ocr` | ✅ MVP |
| Pixel/color | `host.screen.pixel` | ✅ MVP |
| UIA/AX element (Native + UIA backend) | `host.a11y` | ○ post-MVP |
| Package resources | `host.resource` | ✅ MVP |

Observations:
- The **6 backends map exactly onto capability combinations** (catalog §3 — confirmed against source code).
- **Four of the six backends (Custom, Hotspot, Graphical, OCR) need *no* `host.a11y`.** This is why the MVP gets by without the fiddliest capability. Only Native + UIA need `host.a11y` — correctly marked as post-MVP in the catalog.
- **`host.speech` is the only capability whose *semantic model* changes on macOS** — decision made (study §11.7): tts-rs/AVFoundation.
- **We pull sound feedback into the core** (`host.sound`), unlike ReaHotkey (scattered per-plugin).

Minimal set: `overlay + speech + sound + resource + hotkey + window + input + screen + ocr` — word-for-word catalog §6.

## 4. macOS Feasibility per Capability

| Capability | Windows source | macOS API | Status | Permission |
|---|---|---|---|---|
| Overlay runtime | pure logic | pure logic (Rust+Luau) | **full** | — |
| Speech ⚠️ | NVDA DLL / JAWS / SAPI | AVSpeechSynthesizer (tts-rs) | **full, model change** | — |
| Sound | `SoundPlay` | AVAudioPlayer / NSSound | **full** | — |
| Hotkey (context. + global) ⚠️ | `#HotIf WinActive` + `Hotkey` | CGEventTap + context gate | **to be built anew** | Input Monitoring + Accessibility |
| Window/detection ⚠️ | `WinGetControls`/`ControlGetClassNN` | NSWorkspace + AXUIElement; CGWindowList | **limited** | Accessibility (+ Screen Rec. for title) |
| Control geometry | `ControlGetPos` | `AXUIElement` `kAXFrame` | **full** | Accessibility |
| Click/drag/move | `Click`/`MouseClickDrag` | `CGEventCreateMouseEvent` | **full** | Accessibility |
| Keypress/scroll | `Send`/Wheel | `CGEventCreateKeyboardEvent`/`…ScrollWheelEvent` | **full** (keycode map needed) | Accessibility |
| Image search ⚠️ | `ImageSearch` (builtin) | ScreenCaptureKit frame + own template match | **to be built anew** | Screen Recording |
| Pixel/color | `PixelGetColor` | SCK/`CGDisplayCreateImage` crop + readout | **limited** | Screen Recording |
| OCR | Tesseract.exe / UWP | Apple Vision `VNRecognizeTextRequest` | **full** (better than Tesseract) | (uses Screen Rec.) |
| UIA/AX element ⚠️ | `UIA.FindElement` | `AXUIElement` (`kAXChildren`/`kAXPress`) | **limited** | Accessibility |

### Critical Points

**(a) Screen reader output — the biggest conceptual difference.** Windows speaks *to a foreign screen reader*. macOS has no direct counterpart for this. **Strategy A (default): AVSpeechSynthesizer/tts-rs** — always works, own voice/rate (UX deviation), no special privilege; `stopSpeaking` replaces `cancelSpeech`. **Strategy B: NSAccessibility announcement to VoiceOver** — VO voice/settings, but only when VO is running, requires a real NSAccessibility element, and no guarantee of reliable interruption (a killer for a tuner with 4 announcements/s). Decided: Strategy A (study §11.7); the self-voicing ReaHotkey architecture fits "the app voices itself" anyway.

**(b) Coordinate/image fragility** — not an API problem (CGEvent+SCK+Vision cover everything), but a **content problem** (§2): re-capture all constants per plugin on the Mac, including Retina.

**(c) Plugin detection/hosting** — harder: no `ahk_class`, AU instead of VST, no REAPER bridge windows, FX list is no ListView. On macOS, AX is often empty for audio plugins (JUCE/u-he → empty `AXGroup`) → **Vision OCR and template matching become relatively more important** (good for our OCR/Hotspot MVP). Detection order may need to be prioritized platform-specifically.

**(d) Permissions are hard gates** — TCC: Accessibility, Screen Recording, Input Monitoring, none grantable programmatically. CGEventTap is *silently* disabled on self-update (study P0-2 → health watchdog mandatory); Screen Recording is re-prompted *weekly* under Sequoia (P0-9). An onboarding/operations problem to solve before the first overlay.

## 5. What an Overlay Looks Like as a Luau Module (Serum2)

```lua
-- module: overlays/serum2/init.luau
-- module.toml: [capabilities].require = ["overlay","speech","sound",
--   "window","input","screen","ocr","hotkey","resource"]
local M = {}

function M.detect(host)
  local w = host.window.find{ titleRegex = "^VSTGUI%x+$" }        -- Win: VSTGUI class
  if not w then return nil end
  local fx = w:listViewContent("SysListView321")                  -- Win: ListView; macOS: AX table
  if fx and not fx:match("Serum 2") then return nil end
  return w                                                          -- = AXFrame/ControlPos source
end

function M.build(host, win)
  local ov = host.overlay.new("Serum 2", { anchor = win })         -- relativeTo = pluginControl
  ov:addCustomButton{ label = "Main Menu", hotkey = "Ctrl+M",
    onActivate = function() host.input.click(1058, 4, {relativeTo="pluginControl"}) end }
  ov:addCustomButton{ label = "Save Preset As", hotkey = "Ctrl+S",
    onActivate = function() host.input.click(910, 4, {relativeTo="pluginControl"}) end }
  ov:addHotspotButton{ label = "Previous Preset", at = {928, 4}, hotkey = "Alt+P" }
  ov:addHotspotButton{ label = "Next Preset",     at = {968, 4}, hotkey = "Alt+N" }
  ov:addOCRButton{ label = "Preset", region = {540,13,608,23}, engine = "best", hotkey = "Alt+M" }
  return ov
end

return M
```

The **module is almost nothing but content** — coordinates, regions, labels, hotkeys, one detection rule. All the OS mechanics live under `host.*`, platform-identical. For macOS, ideally only the calibrated numbers change (coordinates + OCR region + detection against AX instead of ListView).

## 6. Bare-Minimum Scope & Order

**(a) First: the overlay runtime framework** as a host capability (Win+macOS behind traits): `host.overlay` (taxonomy, lifecycle, navigation, speech composition, coordinate compensation) + `host.speech` (tts-rs) + `host.sound` (core) + `host.hotkey` with context gate (macOS: CGEventTap + frontmost check + re-enable watchdog) + `host.window` (detection/geometry) + `host.input` + `host.screen` (image search incl. macOS SCK+NCC) + `host.ocr` (Vision) + `host.resource`. = exactly catalog §6.

**(b) Then: ONE overlay end-to-end on Win AND macOS — simplest first:**
1. **FabFilter** — *one* HotspotButton, no OCR/Custom/UIA. Validates the complete coordinate-compensation pipeline (AXFrame→click). The "Hello World" of the platform.
2. **GTune** — OCR vertical slice: 2 OCR regions + Custom + 400-ms auto-report timer. Validates `host.ocr` (Vision) + `host.timer` + periodic capture (4 Hz → performance stress test).
3. **ZebraLegacy** — simplest u-he overlay: only Hotspot + 1 OCR, no Custom/Toggle/pixel color. Demonstrates "multiple plugins/one file."
4. **Serum2** — the "rounded" MVP: Hotspot + Custom + OCR + host-specific detection (§5).

Once FabFilter + GTune run on both platforms, the platform is **proven**.

**(c) Then: broader porting.** Early bundles: u-he family **Diva/Hive2/Repro/ZebraLegacy** — structurally almost identical (logo/prev/next/save hotspots + 1 OCR preset + optionally 1 pixel toggle). **A single generic, parameterized "u-he header" module could cover all four** → lowers content effort. **Sforzando** (the standalone variant gets by without UIA).

**The hardest — deliberately later:**
- **Engine 2** — the only overlay with **native Win32 operation** (`ControlClick "Button2"`) + REAPER ListView + host branching. Needs `host.a11y`.
- **Raum** (UIA detection, Qt6) — AX detection; Qt a11y behaves differently on macOS.
- **Zampler** — largest Hotspot/OCR tree, fragile state logic (mouse wheel, click-then-OCR-reread, 3-tab state).
- **Komplete Kontrol / Kontakt 7+8** — UIA+Hotspot+OCR+Graphical mixed, high content volume.
- **Dubler 2 family** — very involved, deliberately deferred (NOT blocked): image-search/hardcoded-coordinate heavy, **absolute** screen coordinates, plus advanced audio routing via ASIO/BASSASIO (per the client also available for macOS — so not a hard blocker, but advanced) and `MSXML2` XML (trivially replaceable). High content effort → to the end.

## 7. Biggest Risks / Open Questions

1. **Coordinate-based interaction is platform/version fragile — the dominant risk.** Re-capture all constants on the Mac, break on every plugin update. **Open:** accept pure re-measurement, or build tooling (an **"overlay calibrator"** that interactively captures coordinates/regions/colors/images on the Mac and stores them as assets)? Without such a tool, content maintenance becomes a permanent cost factor.
2. **macOS plugin window embedding + AU-vs-VST.** REAPER loads AU/VST3 in-process as `NSView`. **Open + to verify early (FabFilter phase):** Is the `AXFrame` of the plugin `NSView` reliably readable through REAPER (for the compensation offset)? This is the foundation of *all* coordinate-relative overlays.
3. **Screen reader output strategy.** Decided (AVSpeechSynthesizer), tradeoff accepted (own voice, no braille). **Open:** Is that enough for blind musicians accustomed to VoiceOver? Otherwise retrofit the announcement path (with interruption risk).
4. **OCR/image vs. native AX.** Where a plugin delivers real AX values, AX is more robust/faster/lower-permission than OCR (no Retina problem). **Recommendation:** the MVP stays OCR/Hotspot, but `host.a11y` as the *next* capability right after the vertical slice — **not at the end**.
5. **Permissions onboarding is operationally critical.** Three TCC gates + CGEventTap silent disable (P0-2) + Sequoia weekly re-prompt (P0-9). **To solve before the first overlay:** health watchdog (`tapIsEnabled`/`tapEnable`, 5 s), permission persistence across updates (stable team/bundle ID), guided one-time onboarding flow.
6. **Performance budget capture+OCR** (study 10.2). GTune's 4-Hz auto-report = 4 captures + OCR/s *per active overlay*; under SCK more expensive than Windows `PixelGetColor`. **Open:** a hard budget (max. capture frequency, ROI size).

## Bottom Line

The port is **feasible and well-prepared** — the capability catalog is cut exactly for this vertical slice, and four of the six backends (Custom/Hotspot/Graphical/OCR) get by without `host.a11y`. The **framework part is a one-time, finite investment**; the work that grows linearly with the plugin count is **per-plugin content re-measurement on the Mac** + **macOS permission/update onboarding**. Order: build the runtime → **FabFilter** (coordinate vertical slice) → **GTune** (OCR/timer) → u-he bundle → later Engine2/Raum/Zampler/Dubler2. Two things to verify *before* everything else: **(1)** the `AXFrame` offset of a REAPER-hosted plugin `NSView` and **(2)** a viable TCC onboarding incl. CGEventTap watchdog.
