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
  needs Apple's std but neither an Apple linker nor a Mac; a clang compiles the one
  Objective-C file in it, objc2's exception helper, which wry needs
  ([building-on-macos.md](building-on-macos.md#checking-macos-code-from-a-windows-machine)).
  Run it with `.\check-macos.ps1`.
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

So the rule is: **a window without a title bar reports its frame as its content, and a
window with one has the bar subtracted.** Which sounds obvious and was not, because macOS
gives no way to ask how tall the bar is.

The window's own buttons answer it. Close, minimise and zoom sit vertically centred in the
bar, so twice their centre offset is the bar's height — measured on Sforzando standalone:
buttons at y 93, height 16, in a frame whose top is 87, giving 2 × 14 = **28 points**. When
a window does have a real content view, that is used instead; when it has neither, the frame
stands, which is right for the borderless windows Qt and JUCE draw for plugins.

The measurement that forced this is worth keeping, because it is the whole argument for the
port being one module rather than two. The three read-outs the Windows Sforzando module
authors coordinates for were found by OCR on macOS at `dx` of −4, +7, −5 and `dy` of **+32,
+31, +32** — the horizontal layout identical, the vertical off by one constant. With the bar
subtracted, a module's Windows coordinates land on the same controls on both platforms. (The
28 derived from the buttons against the 31–32 the OCR implied is the margin an authored
region carries around its glyph, not a disagreement.)

The answer is cached as the four edge offsets rather than as a rectangle, so it survives the
window being moved or resized, and a failed probe is deliberately *not* cached — a window
asked about before it has laid itself out would otherwise report its whole frame as content
for the rest of its life.

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

On a **thread of its own** (`key-tap`, at user-interactive priority, running a run loop of its
own), with a watchdog on that run loop — as the Windows keyboard hook has a thread of its own.
Until 2026-10 it lived on the main run loop, and that was the decision this section said was
most likely to have to change on real hardware. It did.

macOS switches off an event tap whose callback does not answer quickly enough, and it does not
switch it back on by itself. Worse, every key of the Mac waits for the tap, which sits at the
head of the HID taps: a tap on the main run loop held every key of the machine, VoiceOver's
included, for as long as the pump was busy — and the pump routinely spends a long time inside a
cross-process accessibility call, a second at a time against an application that does not
answer. The second Mac tester's log (2026-10-30) has 128 lines of keys that reached the tap late,
up to 1.4 s, and 68 switch-offs, each beside a stall of ours; the threshold was about a second,
not the 300 ms first assumed.

Now:

- **The callback decides alone**, from what the main thread told it: the captured set and the
  scopes (one mutex the main thread holds for a copy), the window in front, the menu and
  own-window gates and the layout's letters (atomics). It never waits for the main thread's work.
- **A key it takes is suppressed at once and queued**, behind one lock that the drain empties
  with one swap and notes its moment under (`tap_queue.rs`). While the main thread is busy the
  key waits there; it never reaches the application underneath, and the rest of the keyboard is
  not held at all. A key that waited 250 ms or more for its module is said in the log (`… captured
  key(s) waited for the event loop …`, at most once a second): what an overlay's user still feels.
- **The window server is asked which application is in front**, for each key the tap would take
  for a capture pinned to a window (`GetFrontProcess`, looked up by name): another application
  than the one the main thread last told the tap of means the user switched while the main
  thread was busy, and the key goes to that application. The first answer's time is logged, an
  answer of a millisecond or more is counted, and two of 250 ms or more in a row switch the
  question off until a wake, an unlock or the session back (`tap_front.rs`). It names an
  application, not a window: another window of the same application in front during a stall is
  not seen (TODO.md). The tap decides at the press, so a key pressed before an overlay coming up
  has set its captures goes to the plug-in, as on Windows.
- **A shortcut's counts are stamped by the tap**, at its key-down, and Carbon's delivery on the
  main thread claims the stamp (`tap_queue::Shortcuts`): keys typed after a shortcut during a
  stall are not counted into it.
- **Nothing on the tap's thread writes a line**: the log's lock is the main thread's. What a line
  needs is left in atomics and small queues, and an observer on the main run loop writes it.
- **The watchdog and a re-creation run on the tap's thread**, which owns the port's run loop: a
  two-second timer, and a source the main thread signals after a wake, an unlock or the session
  coming back.

What it does not remove: the overlay's own latency. Keys wait for their callbacks, and what an
overlay says waits for the main thread, as long as accessibility questions are asked there
(TODO.md, a macOS accessibility thread).

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
table. Modifiers are *roles* and map the Qt way (decided 2026-09-22): `Ctrl` → Command, `Alt`
→ Option, `Shift` → Shift, `Win` → Control, as Qt's `ControlModifier` is Command on macOS and
its `MetaModifier` Control. A module written for Windows therefore lands on the Mac's
counterpart (`"Ctrl+C"` copies on both), and a Mac author writes `Meta` (or `Win`) for the
Control key; `Cmd`/`Command` are spellings of the Ctrl role. One table in `keys.rs`,
`MODIFIER_KEYS`, turns a role into Carbon's hotkey bit, the event flag `key_send` sets and the
tap reads, the layout's modifier state and the modifier keys themselves, and it is tested on
Windows. A module that wants another combination on a Mac picks one with `host.os.pick`.

Letters are the exception to "one table", because on Windows they are not positions: a
letter virtual key follows the layout's labels, so `"Z"` is the key labelled Z on a German
keyboard. The macOS backend therefore asks the selected keyboard layout which key types each
letter (`backend/macos/layout.rs`: `TISCopyCurrentKeyboardLayoutInputSource`, falling back to
the ASCII-capable layout, and `UCKeyTranslate`), puts the answer in force in `keys.rs` for
Carbon registration, `key_send`, `key_post` and the event tap alike, and does it again when the
system posts `kTISNotifySelectedKeyboardInputSourceChanged`, moving the hotkeys on letters to
their new keys (one that cannot move is parked and tried again at the next change, as is one
registered while no key types its letter). It asks
twice, without and with Command held, because a layout such as "Dvorak – QWERTY ⌘" types
QWERTY while Command is down, and a combination that holds Command is looked up in that
second table. The US table stays the letters' fallback, and everything that is not a letter
stays positional. The same read serves `host.keys.check`'s `composes`: `check` takes it again
when it asks, so that it answers for the layout selected at that moment, and the input source
a key is translated through is asked what Option with a key types — the layout is read in one
place, through one set of declarations in `ffi.rs`, on the main thread only. The
choice of layout and the letter tables are plain code, tested on Windows
with described German, French, Dvorak, Dvorak – QWERTY ⌘ and Russian layouts; the reading of a
real one has not run yet.

## Which macOS API answers which trait method

| Trait method | macOS |
| --- | --- |
| `enumerate_windows`, `active_window` | `CGWindowListCopyWindowInfo` + `NSWorkspace.frontmostApplication`, paired with AX |
| `window_controls`, `window_focus_chain` | AX children walk / `kAXFocusedUIElementAttribute` up through `kAXParentAttribute` |
| the `uia_*` family | AX attribute reads over the same tree — see below |
| `screen_size`, `pixel`, `capture` | `CGDisplayBounds`, `CGDisplayCreateImageForRect` |
| `ocr` | Vision `VNRecognizeTextRequest` (per-word boxes via `boundingBox(for:)`) |
| mouse + `key_send` + `type_text` | `CGEvent` synthesis posted to `kCGHIDEventTap` |
| `mouse_click_posted` (`host.input.click{ via = "post" }`) | `CGEvent`s handed to the window's process with `CGEventPostToPid`, carrying the window's number and the private `CGEventSetWindowLocation` place in it |
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
   wallpaper. The backend judges it by evidence rather than by `CGPreflightScreenCaptureAccess`
   alone, which answered "not granted" for a whole session of captures that held other
   applications: ScreenCaptureKit's last answer to a capture (a picture, or a refusal), then
   the system's check where it says granted, then whether other applications' window names
   can be read ([macos-permissions.md](macos-permissions.md#how-screen-recording-is-judged)).
   It also checks captured frames for the telltale (several captures in a row coming back
   uniform), so the log says *permission* instead of *no match found* forty times. An
   application appears in that settings list only once it has asked, so it is asked for
   once per run — after Accessibility, never on top of its dialog — with the documented
   request; the Permissions page's button adds a real capture request, one request per
   press; see
   [macos-permissions.md](macos-permissions.md#how-the-application-gets-into-the-screen-recording-list).
3. **Input Monitoring** — optional: the event tap took the overlay's keys with Accessibility
   alone and Input Monitoring denied on macOS 26.6; the tap is the active kind, which
   Accessibility governs. Registered hotkeys deliberately do **not** need it: Carbon is used
   precisely so the core interaction works before the fussiest permission is granted — and a
   hotkey arriving while the tap has seen no key is how the application notices a tap that
   is not handed the keyboard, and counts Input Monitoring as needed. It is read with
   `IOHIDCheckAccess` for listening and not asked for at start; the Permissions page's button
   asks, with `CGRequestListenEventAccess` and then `IOHIDRequestAccess`.

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
   Settles Input Monitoring. (The tap's placement: its own thread since 2026-10, above.)
5. **Does OCR read plugin text** — the same regions the Windows modules read.
6. **`_AXUIElementGetWindow`** — whether the private pairing works, or the geometry
   fallback is carrying it.
7. **The pump budget** — macOS disables an event tap that is too slow (Windows used to drop
   keystrokes past ~300 ms while its keyboard hook shared the pump; the hook has a thread of
   its own now). Measured here at about a second by the second Mac tester's log, and the tap
   has a thread of its own as well since 2026-10.
8. **Does Vision read a lone digit** — the one measurement that decides whether the second
   OCR engine has to become cross-platform. Windows runs a neural fallback specifically
   because the system engine refuses single digits, and that fallback is a Windows-only
   dependency, so on macOS Vision carries the case alone. Ten minutes with one request
   against the crops the Windows work already produced. (Answered by `ocr-bench` in 2026-10:
   Vision is blind to a lone digit at times, and the fallback now runs on a Mac too, by
   Windows' rule, where the download carries it — see [`host.ocr.recognize`](api/ocr.md#host-ocr-recognize-macos).)
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
routinely half a second to two seconds — and the text reads then ran synchronously on
the pump thread. On
a shared run loop that alone would have exceeded the tap's tolerance and disabled key
capture, mid-session, in a way that reads as "it worked and then stopped". Warming Vision on
a background thread at startup, as the Windows backend already does for its second engine,
removes the stall. (An earlier draft of this paragraph said the tap "living on its own
thread removes the mechanism" when it did not live there. The first tester session showed the
main thread sufficed for ordinary use: the tap was never disabled during minutes of Tab
navigation, only by stalls of over a second. The second tester's log showed those stalls in
ordinary use, and since 2026-10 the tap does live on its own thread — see above.)

**`host.ocr.recognize` takes OCR off the pump thread.** It photographs on a thread named
`screen-capture` (the same `capture_backing` the synchronous calls use) and runs Vision on a
second one, `ocr-recognise`, which asks for the user-initiated quality of service and wraps
every job in an autorelease pool. The Vision warm-up keeps its own thread from
`MacBackend::new` (`ocr-warm-up`). The recognise thread also asks Vision which languages it reads
(`supportedRecognitionLanguages`, at the accurate and the fast level) and the system which ones
the user prefers (`NSLocale.preferredLanguages`), and a read's `lang` is matched against those;
then, once the `ocr-warm-up` pass has ended, it makes a warm-up pass of its own in that language,
before its first job and on the hang clock of one (2026-10-01: the first read on that thread had
cost 2.4 s on an Intel Air). One after the other, so that the first line times a first pass in
the process and the second one on another thread. Both read a line of printed words and say in
the log whether Vision read it. A read without a callback waits in its handler, and only its
module waits; where it cannot wait — a module's top level, an arbiter's callback, a coroutine
of the module's own — it raises, so no read runs on the pump thread any more
([Where it waits](api/ocr.md#where-it-waits)).

**The `host.screen` captures leave the pump thread too.** In a handler `snapshot`, `pixel`,
`pixels` and the calls that work over a region wait while the `screen-capture` thread or the
image worker captures ([Where it waits](api/screen.md#where-it-waits)); where such a call cannot
wait it raises, as `recognize` does, so the pump thread captures nothing. `MacBackend` has no read
of the screen on the calling thread any more — the `pixel()` tile that served `host.screen.pixel`
on the pump went with it — and the capture thread's and the image worker's captures ask the loop
guard first: one on the pump thread is a bug in the host — a panic in a debug build, and in a
release one the line `[loop] screen capture on the event loop`, which the macOS CI jobs fail on.
