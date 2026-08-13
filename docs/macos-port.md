# The macOS backend

*How the second platform is built, and the decisions it rests on. Written while there was
no Mac in the room: everything here is either compiled, unit-tested, or explicitly marked
as unverified. See [architecture-feasibility-study.md](architecture-feasibility-study.md)
§3 for why macOS is second, and [prior-art-vocr.md](prior-art-vocr.md) for the project that
already proved this is possible.*

## What "blind implementation" means here

The port is written without a machine to run it on. That is a real constraint, not a
disclaimer, and it changes what counts as done:

- **Compiled, not written.** `crates/macos-check` type-checks the backend against the
  `aarch64-apple-darwin` target from Windows — `cargo check` stops before linking, so it
  needs Apple's std but neither an Apple linker nor a Mac. Run it with `.\check-macos.ps1`.
  It catches every misremembered signature and missing feature flag. It cannot catch a
  wrong constant, an inverted axis, or a call that is legal and does the wrong thing.
- **Signatures from the source, never from memory.** The `objc2` crates are on disk in the
  cargo registry, and every API used here was read there and compiled before it was used.
- **Everything that can be decided without a Mac is decided here**, in writing, so the first
  session on real hardware is spent measuring rather than designing.
- **Everything that cannot** is listed at the bottom, with the measurement that settles it.

## Units, and the one that would silently ruin everything

macOS has two length units and a display where they differ by a factor of two. A port that
mixes them does not crash; it clicks half a screen away, and on a Retina machine only.

The rule, single and absolute:

> **Everything that crosses the `Backend` trait boundary is in POINTS, top-left origin,
> primary display.**

That is the unit `AXPosition`/`AXSize` report and the unit `CGEvent` clicks land in, so it
costs nothing at either edge. It matches what the host already assumes — one coordinate,
one screen position — so no module and no overlay learns that macOS exists.

Two places pay for it:

- **`capture`** is asked for a rect in points and must return an image whose pixels *are*
  points. `CGDisplayCreateImageForRect` returns a backing-resolution image — 2× on Retina —
  so the backend downsamples to the requested size. Template matching then behaves exactly
  as it does on Windows, and a template captured on one Mac works on another with a
  different display.
- **`ocr`** does the opposite internally: it captures at full backing resolution, hands
  Vision the sharp image, and divides the resulting word boxes by the scale factor before
  returning them. Downsampling first would throw away exactly the detail that makes small
  plugin text readable, which is the whole reason OCR is in this platform.

The scale factor is read per capture rather than cached: a laptop docked to an external
display changes it mid-session, and a cached 2 would then be wrong for every click.

**AX is already top-left.** `kAXPositionAttribute` measures from the top-left of the
primary display, the same as Windows. `NSScreen`/`NSWindow` frames do not — they are
bottom-left — and Vision's normalised boxes are bottom-left too. Both are converted at the
point they are read, never later.

## Window handles

The host passes an opaque `isize` around, produced by the backend and handed back to it.
On Windows it is an `HWND`. macOS has no single equivalent, and the two candidates each
answer only half the question:

- a **`CGWindowID`** identifies a window for capture and enumeration, is stable for the
  window's life, and cannot be turned into an accessibility element by any public API;
- an **`AXUIElement`** can be read, focused and walked, and says nothing about where the
  window's pixels are.

So the backend keeps its **own table** and hands out tokens into it. One entry holds the
`CGWindowID`, the owning pid, and the `AXUIElement`, and a window keeps the same token for
as long as it exists — modules compare handles for identity, so a token that changed on
every enumeration would break plugin detection in a way that looks like flickering.

Pairing the two halves is the one genuinely awkward part. `_AXUIElementGetWindow` is a
private function that returns an element's `CGWindowID` directly; it is what every window
manager on the platform uses, and this application is distributed outside the App Store,
where using it costs nothing but a version risk. It is therefore tried first and **matched
by geometry and title as a fallback**, with the log recording which path answered — so if a
future macOS drops the private symbol, the log says so instead of the port simply going
quiet.

## Keys

`key_spec` in `backend/mod.rs` parses specs into `(vk, mask)` where `vk` is a **Win32
virtual-key code**. That stays: it is the platform-neutral currency the host and every
module already speak, and the alternative — teaching modules two key vocabularies — would
put a platform detail into every overlay.

The macOS backend translates at its own edge, Win32 VK → macOS virtual keycode, in one
table. Modifiers map by *position*, not by name: `Ctrl` → Control, `Alt` → Option, `Shift`
→ Shift, `Win`/`Cmd` → Command. A module written for Windows therefore keeps its finger on
the same physical key, which is what a ported overlay wants; a module that wants Command
asks for `Cmd`, which the parser has always accepted.

## Which macOS API answers which trait method

| Trait method | macOS |
| --- | --- |
| `enumerate_windows`, `active_window` | `CGWindowListCopyWindowInfo` + `NSWorkspace.frontmostApplication`, paired with AX |
| `window_controls`, `window_focus_chain` | AX children walk / `kAXFocusedUIElementAttribute` up through `kAXParentAttribute` |
| the `uia_*` family | AX attribute reads over the same tree — see below |
| `screen_size`, `pixel`, `capture` | `CGDisplayBounds`, `CGDisplayCreateImageForRect` |
| `ocr` | Vision `VNRecognizeTextRequest` (per-word boxes via `boundingBox(for:)`) |
| mouse + `key_send` + `type_text` | `CGEvent` synthesis posted to `kCGHIDEventTap` |
| `register_hotkey` | Carbon `RegisterEventHotKey` — no permission, cannot be silently disabled |
| `set_captured_keys` (capture **and suppress**) | `CGEventTap` with a re-enable watchdog |
| `watch_foreground` | `NSWorkspaceDidActivateApplicationNotification` + `AXObserver` |
| `key_post` | **no equivalent** — see below |

### The `uia_*` methods are questions, not APIs

They are named after Windows UI Automation because that is where they were born, but each
one asks something a macOS accessibility tree can also answer: *is this element present*,
*where would I click it*, *what does it say about its own state*, *step the focus*. The
macOS side implements the **question**. Where UIA and AX genuinely disagree — control-type
numbers, for instance — the backend maps the UIA type a module asked for onto the AX roles
that mean the same thing, and logs anything it could not map, so an unported module says
so in the log instead of silently finding nothing.

### `key_post` has no equivalent, and says so

Posting a key to a specific window without it entering the input queue is a Win32-ism
(`PostMessage` to an `HWND`). macOS has no supported equivalent; `AXUIElementPostKeyboardEvent`
is deprecated and unreliable in exactly the applications that need it. It returns an error
naming the limitation rather than pretending to work — a control that cannot honour its own
promise is worse than one that admits it.

## Permissions

Three, all granted by the user in System Settings, none grantable programmatically:

1. **Accessibility** — required for AX and for the event tap. Checked with
   `AXIsProcessTrusted`; the prompting variant is used once at startup.
2. **Screen Recording** — required for capture. Its failure mode is the dangerous one: a
   capture without permission does not error, it returns a picture of the desktop
   wallpaper. The backend preflights it and, if it cannot, checks a captured frame for the
   telltale (a capture of a region known to be over another application coming back
   uniform), so the log says *permission* instead of *no match found* forty times.
3. **Input Monitoring** — required for the event tap on some versions. Registered hotkeys
   deliberately do **not** need it: Carbon is used precisely so the core interaction works
   before the fussiest permission is granted.

All three are reported in the startup environment block, by name, with their state. A
tester who cannot see a dialog needs the log to say which switch is off.

## Speech

`tts` already speaks on macOS through AVFoundation, so speech works out of the box with no
screen reader running at all. VoiceOver is detected (bundle id `com.apple.VoiceOver`) and
reported, because whether it is running changes what the user hears and how they will
describe a problem.

Routing speech *through* VoiceOver — which is what gives braille output and the user's own
voice settings — needs AppleScript and the `apple-events` entitlement, as VOCR does. That
is deliberately not in this first pass: it is a distribution question (entitlement,
notarisation, a consent prompt) rather than a code one, and it can be added behind the same
`host.speech` surface without any module noticing.

## What is not done, and what would settle it

Every item here needs a Mac. They are listed in the order a first session should take them.

1. **Does anything appear at all** — the app launches, the tray icon exists, the log
   contains the environment block. Settles: bundle layout, permissions prompt, wxWidgets.
2. **Are the coordinates right on a Retina display** — click a known button through
   `host.input.click` and see whether it lands. Settles the points/pixels rule above, which
   is the assumption most likely to be wrong.
3. **Is a capture real** — capture a region over a plugin and compare it with a screenshot.
   Settles Screen Recording and the black-frame detection.
4. **Does the tap suppress** — a captured key must not reach the application underneath.
   Settles Input Monitoring and the tap's placement in the run loop.
5. **Does OCR read plugin text** — the same regions the Windows modules read.
6. **`_AXUIElementGetWindow`** — whether the private pairing works, or the geometry
   fallback is carrying it.
7. **The pump budget** — Windows drops keystrokes if one iteration exceeds ~300 ms; macOS
   disables an event tap that is too slow. The equivalent number here is unmeasured.
