---
title: "What has never run on a Mac"
sidebar_position: 12
---

Everything in this project is written on Windows, and the macOS half of it is written blind.
`check-macos.ps1` type-checks the backend from here and the `macOS build` workflow links it on a
real runner, but neither of those runs a line of it against a real application. The only person
who can do that is the tester, and his time is the scarcest thing this project has.

So this is the list of what he would find out. Everything here **compiles** on macOS; nothing
here has been **seen to work** there. It is not a list of suspected bugs — it is a list of
claims nobody has checked, which is a different and more honest thing.

Add to it in the same change that adds the behaviour. A capability that is written on one
platform and assumed on the other is exactly the kind of thing that gets discovered by a blind
tester at the worst possible moment, and the whole reason this file exists is so that the
discovery happens on purpose instead.

## Not implemented on macOS at all

| What | Where | What happens there today | What it would take |
|---|---|---|---|
| `host.window.ownsPoint` | `backend/mod.rs` default, Windows-only implementation | Returns `nil`, which every caller reads as permission — so a click is never refused for being covered, exactly as before the check existed | An `AXUIElementCopyElementAtPosition` or `CGWindowListCopyWindowInfo` answer, then a test: put a window over a plug-in and press an overlay control, and see whether it refuses |

## Implemented but never exercised

| What | Where | Why it is worth checking |
|---|---|---|
| `mouse_scroll` unit change | `backend/macos/input.rs` via `backend/mod.rs` | The trait now counts wheel units (120 to a notch) where it used to count notches, and the macOS side divides. A whole notch should behave exactly as before; a *fraction* of one — which ON:EAR's Tone knob asks for — becomes the smallest whole line, because Quartz counts lines and cannot express less. Worth confirming that a notch still moves what it used to |
| `mouse_drag` | `backend/macos/input.rs` | Windows now sends injected, interpolated movement over about sixty milliseconds; macOS has always posted a single drag event between press and release. That asymmetry is deliberate and untested — a control that needs the intermediate movement would work on Windows and not on macOS |
| Blank-region OCR | `backend/windows.rs` (`tighten`) | A region with no content now returns nothing instead of whatever the neural fallback invents. The macOS OCR path is separate; whether Apple Vision answers a flat rectangle with text is unknown, and it is the same failure — a module cannot tell an invented reading from a real one |
| `Overlay:watch` | `overlay-runtime` | Platform-independent Luau, but what it watches is not: the readings it waits on are pixels, OCR and accessibility values, all of which behave differently there. The toggle and stepper announcements now depend on it |
| `editable` / `typingWhen` | `overlay-runtime` | Both decide which keys the overlay releases, and the key path on macOS is a `CGEventTap` rather than a Win32 hook. A field that should receive a space, and an overlay that should hold nothing while a text field has the keyboard |
| Screen-reader modifier pass-through | `backend/windows.rs` hook | Windows-only, and by design: NVDA's modifier is Insert or CapsLock, neither of which is a Windows modifier, while VoiceOver's is Ctrl+Option — real modifiers that the tap already sees. Worth one check that VO commands do reach VoiceOver inside an overlay |

## Known not to apply

The ARC ON:EAR module declares `supported_os = ["windows"]`. Every `uia_*` call it uses *is*
implemented on macOS through the Accessibility API, and the names it matches on come from JUCE
rather than from Windows, so a port is plausible — but nobody has had the application in front
of a Mac, and its window matcher uses a Win32 class name. See its manifest for the reasoning.
