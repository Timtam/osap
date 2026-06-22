# Prior Art: VOCR — what it shows us for the macOS path

*Analysis of [VOCR](https://github.com/chigkim/VOCR) (chigkim, Swift, macOS, GPL-3.0) as a proven template. As of 2026-06-21. Belongs to [reahotkey-port-analysis.md](reahotkey-port-analysis.md) + [host-api-capability-catalog.md](host-api-capability-catalog.md).*

VOCR makes non-accessible macOS UIs — **including plugins in REAPER and Logic** — accessible to VoiceOver users via OCR + Accessibility. That is exactly our macOS problem space, proven in production. Sources: `Accessibility.swift`, `AXUIElement Extension.swift`, `Navigation.swift`, `OCRTextSearch.swift`, `Shortcuts.swift`, `Utils.swift`, `VOCR.entitlements`.

## Techniques (concrete API recipes)

1. **Window frame & capture origin** (`Navigation.getWindow()`): `NSWorkspace.frontmostApplication` → focused window as `AXUIElement` → `AXPosition`/`AXSize` via `AXUIElementCopyAttributeValue` → `AXValueGetValue(…, .cgPoint/.cgSize)` → `CGRect`. `ScreenCapture.capture(rect:)` (CoreGraphics) captures **the entire window**. **The window origin is added to all OCR coordinates.** → The AX frame is reliably available even when the window content is not accessible.
2. **OCR**: Apple Vision (`VisionOCR.observations(in:)` → `VNRecognizeTextRequest`); results as `displayResults: [[Observation]]` (lines × words), line grouping via `lineBreakTolerance ≈ 0.01`, words sorted by `boundingBox.midX`.
3. **Coordinate mapping** (`convert2coordinates`): normalized Vision bbox → flip Y (`1 - maxY`) → `VNImagePointForNormalizedPoint` → **+ window origin** → screen pixels. Mouse via `CGWarpMouseCursorPosition`. Character level via `topCandidates(1)[0].boundingBox(for: range)`.
4. **Navigation/cursor model**: cursor `(l, w, c)` (line/word/character) over `displayResults`; `right/left/up/down`, "report coordinates", optionally carry the mouse along (`Settings.moveMouse`).
5. **Speech output — tiered** (`Accessibility.speak` / `speakWithSynthesizer`): VoiceOver is running (detection via `NSWorkspace.runningApplications`) → AppleScript (`say.scpt`) **to VoiceOver** (respects the VO voice **+ Braille**); otherwise `NSSpeechSynthesizer`; additionally `NSAccessibility.post(announcementRequested)`. Read the VoiceOver cursor via AppleScript (`VOCursor.scpt` → x,y,w,h).
6. **Global hotkeys** (`Shortcuts.swift`): Swift lib **`HotKey`** = Carbon `RegisterEventHotKey` (`carbonKeyCode`/`carbonModifiers`). **Context scopes** (`global`/`navigation`/`computerUse`) via register/unregister per state (`activateNavigationShortcuts()` etc.). Persistence in `UserDefaults`.
7. **Permissions/entitlements** (`VOCR.entitlements`): **non-sandboxed** (no `app-sandbox`), entitlements only `com.apple.security.automation.apple-events` (for VO AppleScript) + `device.camera`. Accessibility + Screen Recording at runtime via TCC (`AXIsProcessTrustedWithOptions`). → Developer ID, **no App Store**.

## What this means for US (mapped to open questions)

1. **Pre-flight 1 largely answered.** The AX window frame (`AXPosition`/`AXSize`) is the reliable origin source for capture + coordinate compensation — **even for inaccessible plugin windows**. Confirms `host.window` geometry. The only remaining open case is an *embedded* plugin sub-view vs. a **floating window** — and REAPER can show plugins as a floating window, which yields the simplest case (top-level AX window).
2. **`host.speech` is richer.** The VoiceOver-via-AppleScript path solves the Braille/VO settings question (analysis §7.3). We offer it as a **macOS backend behind `host.speech`**: with VoiceOver running → VO AppleScript (Braille + VO voice), otherwise AVFoundation/`tts-rs`. Cost: `apple-events` entitlement + user opt-in ("control VoiceOver via AppleScript") + AppleScript latency.
3. **`host.hotkey` de-risked → pre-flight 2 defused.** For *registered* hotkeys, Carbon `RegisterEventHotKey` is sufficient (no Input Monitoring, no CGEventTap silent disable). Context switch = re-register per scope (like VOCR). **CGEventTap only** for suppression/remapping/hotstrings — the health watchdog is then only relevant for that case, not for normal overlay hotkeys.
4. **OCR-centric approach validated.** capture-window → Vision → line/word grouping → coord mapping → CGWarp/Click = exactly `host.screen` + `host.ocr` + `host.input`. We adopt the normalized-bbox→pixel recipe 1:1.
5. **Navigation model as a template for a non-overlay feature.** VOCR's (line/word/character) cursor over OCR results is exactly what a **generic "OCR explorer"** (overlay-INDEPENDENT, directly from Luau) can offer in our system — a good early feature that demonstrates the first-class primitives.
6. **Distribution confirmed:** non-sandboxed + Developer ID; `apple-events` entitlement if we offer the VO speech path.

## Limits / not 1:1 transferable

- VOCR OCRs the **entire window** and allows free navigation; ReaHotkey overlays are **curated** (fixed controls/hotkeys per plugin). Both map onto the same primitives in our system: VOCR style = generic OCR explorer (`host.ocr` + navigation), ReaHotkey style = `host.overlay`.
- VOCR requires VoiceOver for the best speech path; our `host.speech` must also work **without** VoiceOver running (AVFoundation fallback).
- **License GPL-3.0:** The techniques used are standard Apple APIs (AX, Vision, CGEvent, Carbon HotKey) — freely reimplementable in Rust. We adopt concepts, **do not copy Swift code**.
