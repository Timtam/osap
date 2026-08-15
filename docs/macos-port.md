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

**AX is already top-left, and so is CoreGraphics.** `kAXPositionAttribute`,
`CGDisplayBounds`, `CGEvent` locations and `CGWarpMouseCursorPosition` all measure from the
top-left of the primary display, y growing downward — the same frame Windows uses. There is
**no global flip** anywhere in this backend, and adding one would mirror every window
rectangle and every click.

Exactly two bottom-left sources exist, and each is converted in the one function that reads
it: `NSScreen`'s frames, which are therefore **kept out of the geometry path entirely** in
favour of `CGDisplayBounds`, and Vision's normalised boxes.

## `client_*` on a platform that has no client area

Windows distinguishes a window's frame from its content, and every overlay coordinate in
every module is relative to the content origin. macOS accessibility has no such
distinction: `AXPosition`/`AXSize` describe the whole window, title bar included, and there
is no attribute that reports the content rect.

> **Measured 2026-08-15, and it puts this rule in doubt.** Sforzando standalone, probed on
> a Mac: the three read-outs the Windows module authors coordinates for were found by OCR at
> `dx` of −4, +7, −5 and `dy` of **+32, +31, +32**. The horizontal layout is identical and
> the vertical is off by one constant — a title bar. So `client == frame` is what stands
> between a module written once and a module written twice, and the next probe (which now
> reports every element's rectangle, including the window's own close button) should be able
> to derive that constant rather than guess it. The rule below is what the code does today,
> not what it should do once that measurement lands.

So the rule here is **`client` equals the frame, always** — no heuristic, no guessing at a
title-bar height. It is predictable, which matters more than it looks: macOS module
variants have not been written yet, so they will be authored against whatever this reports,
and a rule that is consistently "including the title bar" costs an author nothing, whereas
one that is right for borderless plugin windows and subtly wrong for titled ones would
produce a module whose coordinates are all correct and another whose coordinates are all
shifted, with nothing to tell them apart.

## What `class` says

`class` is the string every module's plugin detection matches against — on Windows it is
the window class, matched with patterns like `"Qt%d+.-QWindowIcon"`. macOS has nothing of
the sort, so the backend composes one, and it must be the **same** string everywhere it
appears or a module will match against something the diagnostic dump never shows:

```
AXRole/AXSubrole/AXIdentifier      e.g.  AXWindow/AXStandardWindow/   AXGroup//NI.Kontakt.Main
```

Both separators always, a missing part empty — so a pattern can anchor on them and an
element with nothing to say still yields `"//"`, which can be matched, rather than a nil
that cannot. None of the three parts is ever localised, which the title and the role
*description* both are. `uia_raw_dump` — how anyone will discover what a plugin's tree
actually contains — prints the identical string in its own `class` slot, so an author can
paste what a dump shows straight into a matcher.

A matcher can also name one part on its own: `macos = { axRole = "AXWindow" }`,
`axSubrole`, `axIdentifier`. The prelude splits the composed string; the backend publishes
one field.

Whether the Qt object names the Windows modules match on (`FileTypeSelector`,
`WhatsNewScreen`) survive into `AXIdentifier` is unknown: they reach Windows through Qt's
Windows accessibility provider, and the macOS bridge is a different one. This is the single
biggest open question for the "port the ReaHotkey overlays" goal, and the dump is the only
instrument that can answer it — which is why it ships in the first build rather than as a
diagnostic afterthought.

## Where the event tap lives

On the **main run loop**, with a watchdog — and this is the decision most likely to have to
change on real hardware, so it is worth stating the argument on both sides.

macOS switches off an event tap whose callback does not answer quickly enough, and it does
not switch it back on by itself. The main run loop is also the pump, and the pump routinely
spends a long time inside a cross-process accessibility call: one of the state probes the
modules use costs 60–300 ms by design. So the tap can be disabled during ordinary use.

What makes that survivable rather than fatal is that the disable is *observable*. The tap
re-enables itself from a run-loop observer, the pump asks again on every tick, both are
rate-limited to one cheap query every couple of seconds, and every re-enable is logged. The
failure is therefore a second or two of lost keys with a line in the log naming it — not a
platform that quietly stops answering.

Its own thread would remove the mechanism entirely, and that is the escalation if a first
session shows the tap dying repeatedly. It is not free: the tap's callback would then be
forbidden from touching accessibility at all, and the scope gate currently asks which window
is frontmost. That has to become a cached value maintained by the activation observer before
the thread can move. Doing the harder thing first, blind, to fix a problem that may not
occur, is how a port acquires bugs nobody can find.

Either way the state is already shared rather than thread-local — atomics and one mutex — so
the move costs no rewrite.

## How "is a menu open" is answered

Not by asking. It is precomputed: applications post `AXMenuOpened` and `AXMenuClosed` as
their menus track, the per-application observer already listening for focus changes hears
them too, and the question reduces to reading a counter. That matters because the question
is asked from inside the key path on every captured keystroke, where an accessibility
traversal would be catastrophic.

The design note that this could ride on `NSMenu`'s tracking notifications was wrong, and
the implementation says so: those are posted on the *in-process* notification centre and
know only about our own menus, never a plugin's. The accessibility notifications are the
only cross-process signal that is not a traversal.

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
8. **Does Vision read a lone digit** — the one measurement that decides whether the second
   OCR engine has to become cross-platform. Windows runs a neural fallback specifically
   because the system engine refuses single digits, and that fallback is a Windows-only
   dependency, so on macOS Vision carries the case alone. Ten minutes with one request
   against the crops the Windows work already produced.
9. **Do Qt object names survive into `AXIdentifier`** — see the `class` section. If they do
   not, the modules that navigate by them need a different anchor on macOS.

## Two things that were nearly built wrong

Worth recording, because both were caught by review rather than by a compiler and both
would have been invisible until a Mac was in the room.

**A global coordinate flip.** The first draft of the contract called for flipping Cocoa's
bottom-left origin at the backend boundary. That is true of `NSScreen` and of Vision, and
false of everything else this backend touches — accessibility geometry and CoreGraphics
events are already top-left. A blanket flip would have mirrored every window rectangle and
every click, and a partial one that forgot Vision would have left OCR boxes mirrored, which
is invisible on single-line text and wrong on everything else.

**The first OCR call killing the keyboard.** Vision loads its model on the first request —
routinely half a second to two seconds — and OCR runs synchronously on the pump thread. On
a shared run loop that alone would have exceeded the tap's tolerance and disabled key
capture permanently, mid-session, in a way that reads as "it worked and then stopped". The
tap living on its own thread removes the mechanism; warming Vision on a background thread
at startup, as the Windows backend already does for its second engine, removes the stall.
