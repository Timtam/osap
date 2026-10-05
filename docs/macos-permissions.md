# macOS permissions

*Four switches now, and none can be granted by the application. Three of them, when
missing, do not produce an error — they produce plausible wrong answers, which for someone
who cannot see the screen is the worst possible failure. The count grew with Automation,
whose absence is the quietest of the lot: the setting reads as on and nothing happens. Beside
them sits one switch of VoiceOver's own, which "Speak through VoiceOver" needs as well. This
page is what to grant, how to check, and what each absence looks like.*

## The short version

**System Settings → Privacy & Security**, then:

| Permission | Needed for | What it looks like when missing |
| --- | --- | --- |
| **Accessibility** | reading and clicking anything | every plugin looks empty; no overlay ever activates |
| **Screen Recording** | capture, image search, OCR | **captures silently return the desktop wallpaper** — never an error |
| **Input Monitoring** | intercepting and suppressing keys, if it is needed next to Accessibility (not measured yet) | overlay keys reach the plugin instead of the overlay |
| **Automation** → VoiceOver | speaking *through* VoiceOver | the setting is on and the overlay still speaks in its own voice |
| VoiceOver's own **"Allow VoiceOver to be controlled with AppleScript"** — in VoiceOver Utility, General, not in Privacy & Security | speaking *through* VoiceOver | VoiceOver drops every line; the application notices, speaks with the system voice instead, and says why once ([below](#voiceover-applescript)) |

Only the first two need granting for the platform to work at all; **Automation** is asked
for separately, and only if you switch on "Speak through VoiceOver". **Input Monitoring** is
not asked for at start: the key capture is the kind Accessibility governs, and whether this
application needs Input Monitoring next to it has not been measured yet. Its line in the log
and on the Permissions page reads Input Monitoring's own state, and *unknown* there means macOS
has not been asked. (Builds before 2026-09-27 read the posting side of the same check instead —
the Accessibility side — which is why their logs showed "Input Monitoring" following
Accessibility. The log now reports that one on a line of its own, `hid post events`.)

The pane is called **System Settings → Privacy & Security** on Ventura and later, and
**System Preferences → Security & Privacy → Privacy** on Monterey and earlier. The log names
whichever one this machine actually has. From macOS 15 the Screen Recording list is called
**Screen & System Audio Recording**.

**Grant Accessibility first.** Nothing works without it, and it is also what the Screen
Recording request waits for — see the next section.

Whether the application has to be restarted afterwards depends on the permission, and both
halves were measured on a Mac mini with macOS 14.5 on 2026-09-18:

- **Accessibility takes effect in the running application**, within a few seconds. Press
  *Re-check now* on the Permissions page to confirm it; no restart.
- **Screen Recording does not.** macOS hands it only to a process that started *after* it was
  granted, so **quit and reopen the application** after granting it. The running one keeps
  the old answer; this is the most common reason a permission appears not to have worked.
- **Input Monitoring**: if *Re-check now* still shows it missing after granting it, quit and
  reopen — this one has not been measured either way.
- **Automation** (VoiceOver speech) needs no restart, and neither does VoiceOver's AppleScript
  box: on 2026-10-01 it was ticked with VoiceOver and the application running, and the next
  lines came through VoiceOver.

## How the application gets into the Screen Recording list

macOS lists an application under Screen Recording only once it has **asked** for the
permission. Checking whether it has it is not asking, and enrols nothing. So the application
asks by itself, once per run:

- **as soon as it is up** — after its window and its startup announcement — when
  Accessibility is already granted;
- otherwise **the moment Accessibility is granted** while it runs (it looks once a second,
  and keeps App Nap from stretching that second while it waits: the grant is made with its
  window covered, which is when macOS naps an application without a Dock icon). It waits,
  and keeps App Nap away, for as long as Accessibility is missing — the whole session, when
  it is never granted — with an activity that is not latency-critical, so the waiting costs
  no more battery than keeping App Nap away does.
  Never while the Accessibility dialog may still be on screen: two system dialogs would stand
  on top of each other, and somebody listening hears one of them. On a first launch the order
  is therefore Accessibility, then Screen Recording.

That request is **`CGRequestScreenCaptureAccess`**, the documented one. It returns at once;
the dialog, where macOS shows one, is drawn by the system, and its button that opens the
settings leads to the list.

There is a second, different way to ask: a **capture request**. The application asks
ScreenCaptureKit for its list of what can be captured, on a thread of its own, and waits at
most five seconds for the answer — it is the first thing Apple's own capture sample does, and
that sample's documentation says the system prompts on its first run. Where ScreenCaptureKit
does not answer, the application also captures one point through the older Core Graphics
functions (found by name at run time, like every use of them here). On macOS 13 and later it
does not do that otherwise: macOS 15 added alerts of its own for those functions, and a second
dialog about the same thing is one too many.

| macOS | Asked by itself | The Screen Recording button, pressed |
| --- | --- | --- |
| 11, and 12.0 – 12.2 | nothing: the application does not open there. Its minimum is 12.3 (`LSMinimumSystemVersion`), because it needs ScreenCaptureKit, which arrived in 12.3 | — |
| 12.3 – 12.7 | the documented request; a second later the capture request; a second after that, one point through the older functions. On macOS 12 the documented request alone was measured to raise no dialog and add no entry. | opens the list, since the first request makes all three; pressed before it, it makes them |
| 13 and later | the documented request alone | first press: the capture request (and the older functions, only if ScreenCaptureKit does not answer); next press: opens the list |

### The Screen Recording button

**"Open the Screen Recording settings"** on the Permissions page makes the next request the
application has left in this run, one per press — and **a press that has just asked does not
open the settings**. The request may have put a dialog on screen, and a settings window opened
on top of it would take the focus, and VoiceOver, away from that dialog. The page says instead
that macOS has been asked, that the dialog's button leads to the list, and that pressing again
if no dialog came up asks the second way, or, once everything has been asked, opens the list.
With Screen Recording granted, the button opens the list at once. Pressed before the automatic
request — while Accessibility is still missing — its first press is the documented request, and
the automatic one is then not made.

None of this can report whether an entry was added — no public call answers that — so the
log ends with what is left to do. After the automatic request on macOS 13 and later:

```
screen recording: asked with CGRequestScreenCaptureAccess (now that Accessibility is granted); it answered not granted
screen recording: if a dialog came up, its button that opens the settings leads to the list. If none did, or the application is not in the list, press 'Open the Screen Recording settings' on the Permissions page: it asks a second way. Failing that: open System Settings > Privacy & Security > Screen Recording, press the + button under the list, …
```

and after the capture request:

```
screen recording: asking a second way (from the Permissions page): a capture request
screen recording: asked ScreenCaptureKit for what can be captured, and it answered 'declined' (… -3801: "…") — its word for not permitted, …
screen recording: if this application is not in the list: open System Settings > Privacy & Security > Screen Recording, press the + button under the list, …
```

If ScreenCaptureKit answers with a number of displays and windows instead — which it normally
does only for an application allowed to capture — the "NOT granted" before it may be the answer
the system gave the process when it started, which it keeps until the application is started
again.

### If the entry still does not appear

Add it by hand. It is the fallback, and it always works:

1. Open the Screen Recording list: **System Settings → Privacy & Security → Screen Recording**
   (Screen & System Audio Recording from macOS 15); on Monterey **System Preferences →
   Security & Privacy → Privacy → Screen Recording**, and unlock the padlock at the bottom
   first.
2. Press **+** under the list, choose `AutomationPlatform.app`, and switch it on.
3. Quit the application and open it again.

Before that, read the `launched by` line of the log's startup block. It should begin with
`launchd`: the application was opened with `open` or from the Finder. If it names a shell,
the application was started from a terminal, and macOS asks about — and lists — the
application responsible for it, which is that terminal. Quit it and open the `.app` with
`open` or from the Finder.

If the entry **is** in the list and switched on, and the log still says "NOT granted" in a run
started after that, the entry belongs to an earlier build — see
[below](#when-the-switch-is-on-and-the-application-still-cannot-use-it).

## Input Monitoring

It is **not asked for at start**, and nothing asks for it but its button: the key capture is
the kind Accessibility governs, and whether this application needs Input Monitoring next to
Accessibility has not been measured. If keys an overlay has claimed reach the plugin while
Accessibility is granted, press **"Open the Input Monitoring settings"** on the Permissions
page. Like the Screen Recording button, it asks one way per press, and a press that has just
asked leaves the settings shut:

- with Accessibility missing, it asks nothing, says to grant Accessibility first, and opens
  the list;
- *granted*, or *denied* — which means the application is in the list, switched off — it
  opens the list;
- *unknown*: the first press asks with `CGRequestListenEventAccess`, the second with IOKit's
  `IOHIDRequestAccess` for listening, and the third opens the list.

Unlike Screen Recording's, its state can be read, and the log writes it before and after each
request: *unknown* turning into *denied* is the sign that the application is now in the list,
switched off. If it is not in the list even after both, add it with **+** the same way as
above.

## Checking, without being able to see

The application asks the system what it has been granted and writes the answer to its log
at every startup, in a block near the top:

```
[env] accessibility: granted
[env] screen recording: granted
[env] input monitoring: unknown — macOS has not been asked (the Permissions page's button asks)
[env] hid post events: granted (IOHIDCheckAccess for posting events, the Accessibility side — …)
[env] display 1: 1512x982 pt, scale 2.0 (primary)
[env] voiceover: running
[env] voiceover applescript: not allowed (/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled is not there; SCREnableAppleScript: not set, not counted on macOS 15 and later)
[env] translocated: no
[env] bundle: com.automationplatform.app
[env] launched by: launchd — opened with `open` or from the Finder, …
```

That block is the first thing to read when something does not work, and the first thing to
send. It is written when the application starts; after 8 MB the log goes on in a new file, and
the part with the block is moved to `automation-platform.log.1` — send both — and after a
second 8 MB it is gone, until the application is reopened. It sits beside the `.app` as
`automation-platform.log`, or in
`~/Library/Application Support/AutomationPlatform/` if the folder beside the app was not
writable — the log's own header says which one it chose. A download opened without removing
its quarantine flag runs from a copy macOS made elsewhere; the log then goes beside the original
`.app` when macOS can say where that is, and `translocated` says `YES`, where both are, and why
if the original could not be found (see
[building-on-macos.md](building-on-macos.md#translocation)).

## Why each one is asked for

**Accessibility** is the whole platform. Everything the overlays do — read a control, find
a plugin's window, move focus, click a button by name — goes through the accessibility API,
and it is all-or-nothing. Without it, the application does not crash and does not complain:
every query returns nothing, so it behaves like a machine with no plugins open.

**Screen Recording** is needed because many plugin controls tell the accessibility API
nothing at all, and the only way to read them is to look at the pixels. The failure mode
here deserves emphasis: macOS does not refuse a capture from an unauthorised process. It
returns a picture of the desktop. Every image search then fails to match, every value reads
as empty, and nothing anywhere reports a permission problem. The application tries to detect
this and say so, but the check cannot be perfect — if captures are behaving oddly, check
this switch first.

**Input Monitoring** would matter only for keys the overlay *takes away* from the plugin —
Tab, the arrows, Space inside an overlay — and whether they need it next to Accessibility is
not known yet (see [above](#input-monitoring)). Global shortcuts do **not** need it: those are
registered through an older, narrower mechanism precisely so the main interaction keeps
working before the fussiest permission has been granted. So if it is needed, an application
with Accessibility but not Input Monitoring is half-alive: shortcuts open overlays, and
navigating inside one leaks keys to the plugin.

## Automation, and why it is the quietest of the four

The three above are about *observing* — reading other applications, capturing the screen,
seeing keys. **Automation is about telling an application to do something**, and it is the
one the VoiceOver transport needs, because every line it says arrives at VoiceOver as an
Apple Event.

It behaves unlike the others in the way that matters most: **its refusal does not look like
a refusal.** A missing Accessibility grant makes every plugin look empty, which is obvious.
A missing Screen Recording grant hands back the wallpaper, which is at least visible in a
capture. A missing Automation grant makes macOS decline the event — and the transport then
does what it does for any failure: it marks itself unhealthy and hands the line to the
overlay's own voice. So you still hear everything. It just does not come out of VoiceOver,
in your voice, at your rate, or on your braille display, and that reads as "the setting did
not take" rather than as a permission problem.

So it is asked for at the one moment where the question is obviously about what you just
did: when you switch **"Speak through VoiceOver"** on. macOS then puts up its own dialog
naming both applications. Two things follow from that:

- **VoiceOver has to be running when you switch it on.** The system will not ask about
  controlling an application that is not there, and the log says so rather than failing
  quietly: *"VoiceOver automation not requested: VoiceOver is not running"*. Switch it on
  again with VoiceOver up.
- **Switching it off and on again puts the question again**, unlike the other two, which ask
  once per session. That is deliberate: switching it off and on is exactly what somebody does
  when they are trying to fix this. Whether macOS actually shows the dialog a second time is
  its decision rather than ours — after a refusal it generally does not, which is why the log
  names the pane instead of relying on the dialog coming back.

The session header reports it as `voiceover automation`, asked without prompting — a log
line must never put a dialog on screen. `not asked yet` is the ordinary state until the
setting goes on. When it reads `REFUSED`, a `voiceover automation fix` line follows it with
the pane to open, the same way the other permissions do, and it names whichever pane this
machine actually has.

## VoiceOver's own switch: AppleScript {#voiceover-applescript}

"Speak through VoiceOver" needs one more thing, and it is not in Privacy & Security: VoiceOver's
own **"Allow VoiceOver to be controlled with AppleScript"**, a box in **VoiceOver Utility →
General**. VO-Fn-F8 opens VoiceOver Utility while VoiceOver runs (VO-F8 where the top row is
set to standard function keys; VO-Fn-8 works too). Changing the box asks for an administrator's
password.

Without it VoiceOver **takes every line and drops it**. Nothing is refused, so this is quieter
still than a missing Automation grant: on 2026-10-01 a tester had every permission granted and
the setting on, the log said `speech is going to VoiceOver`, and nothing was heard. The
application now finds out for itself, at start-up, when the setting is ticked, when VoiceOver
starts or restarts, and when VoiceOver Utility quits:

- it reads the file VoiceOver Utility writes when the box is ticked,
  `/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled`, and before macOS 15 the
  preference the box sets, `SCREnableAppleScript`;
- with the Automation permission granted, it asks VoiceOver one read-only question through
  `osascript` and waits for the answer. **-1708** ("doesn't understand") means unticked, even
  when the file is there. An answer means ticked only where the file cannot be looked at: the
  question is a read, which VoiceOver may answer with the box unticked, and taking that for
  "ticked" would bring the silence back.

While the box is unticked, **every line goes to the system voice**, and the user hears once,
in the system voice: *"Automation Platform cannot speak through VoiceOver. Turn on 'Allow
VoiceOver to be controlled with AppleScript' in VoiceOver Utility, General; VO-Fn-F8 opens it.
Until then, this voice speaks."* Once the box is ticked — the application looks every few seconds
while it is not, and at once when VoiceOver Utility quits — lines go to VoiceOver again, and when
VoiceOver itself answered, it says *"Automation Platform now speaks through VoiceOver."* No
restart. What is not caught: the lines said between unticking the box and quitting VoiceOver
Utility are lost. The details, with the schedule and the costs, are in
[`host.speech.output`](api/speech.md#host-speech-output).

**The application never changes this box**, and never writes the file or the preference. It is
the user's security setting: with it ticked, any application allowed to send VoiceOver Apple
Events can make VoiceOver say and do things.

The startup block reports it as `voiceover applescript` — `allowed`, `not allowed` or `unknown`,
and the file and the preference it comes from — from those alone; VoiceOver's own answer follows
in the `[speech]` lines. The Permissions page shows it below the four permissions in plain words
("the box is not ticked"), with VoiceOver's last answer, as a line of text and no button: it says
where the box is, and *Re-check now* reads it again.

## When the switch is on and the application still cannot use it

This happens, and it is not the user's mistake. macOS does not remember "AutomationPlatform
is allowed"; it remembers a description of the *signed identity* that was allowed. A build
signed differently from the one that was granted is a different application as far as that
record is concerned — while still appearing in the list, still ticked.

The log names the identity in play:

```
[env] signature: ad-hoc (no certificate) — this identity changes on every rebuild …
[env] signature cdhash: 8a3f…
```

Compare that `cdhash` between two runs. If it changed and the permissions stopped working,
that is the entire explanation, and there is nothing else to look for.

The remedy is to make the record match the application again. Quit the application and run,
in Terminal:

```
tccutil reset ScreenCapture com.automationplatform.app
```

`Accessibility` or `ListenEvent` in place of `ScreenCapture` does the same for the other two.
Always with the bundle id: without it, `tccutil reset` takes that permission from every
application on the Mac. Then open the application again; the entry is gone, and its own
request can add it anew — if none does, add it with **+** as above. Toggling the switch off
and on is usually not enough: the stale requirement stays attached to the entry. Removing the
entry with **−** and adding it back with **+** works as well, but after **−** macOS has been
reported to raise no dialog for that application's own request until the Mac is restarted
(Apple developer forums, thread 818415), so add it back by hand straight away.

The log's Screen Recording lines say this too, because with an ad-hoc signed build — every
download from CI — it is the common case: the previous build's entry is in the list, switched
on, and belongs to an application that no longer exists.

`./macos-signing-identity.sh` stops it recurring.

## Permissions and rebuilds

macOS records these against an application's **code identity**, not against its path or its
name. An ad-hoc signature — `codesign -s -`, the default here — derives that identity from
the contents of the binary, so **every rebuild is a different application** and all three
permissions are forgotten. Measured on a tester's second run: everything back to "NOT
granted", and the probe reporting no focused window because accessibility reads were being
refused. And whenever the requests [above](#how-the-application-gets-into-the-screen-recording-list)
do not put the new build into the Screen Recording list, that means adding it by hand again.

`./macos-signing-identity.sh` fixes it in a minute. It creates a local self-signed
certificate, after which the identity is "this bundle id, signed by this certificate" —
neither half of which changes when the code does. `package-macos.sh` picks it up by name and
uses it automatically. Grant the three permissions once more after the first build that uses
it, and they stay granted.

That is for local testing only. A Developer ID and notarisation are the answer for anything
distributed, and are a separate piece of work.

Keeping the bundle identifier stable is what makes even that much work, which is why it is
fixed in `package-macos.sh` and not to be edited casually.

## Keys, and where they land on a Mac

**Control-Option is VoiceOver's own modifier**, so anything on it belongs to VoiceOver
first — on any Mac whose VoiceOver modifier is the default. (VoiceOver Utility > General
offers Caps Lock instead, and a Mac set that way lets Control-Option chords through, which
is how one chord can work for a tester on his own Mac and beep on somebody else's.) Two of
this project's keys used to sit there, and the third Mac session measured what that costs:
registered on both launches, never delivered, VoiceOver's error sound on every press.

- The application's own **reload-everything** key is `Ctrl+Alt+Shift+Win+F5` on Windows
  and **Command-Shift-F5** on a Mac (`Cmd+Shift+F5`); the `daw-hosts` key that puts the
  keyboard back into a plugin window is **Command-Shift-F6** likewise (`Cmd+Shift+F6`, and
  `Ctrl+Shift+Win+Alt+F6` on Windows). Both use an F-key, so they additionally need "Use F1,
  F2, etc. as standard function keys" turned on, or the `fn` key held down.
- The **calibrator's** keys are `Ctrl+Alt+Shift+S/T/V`: **Command-Option-Shift** on a Mac,
  where a spec's Ctrl is Command, off Control-Option, and Ctrl+Alt+Shift on Windows. They only
  exist once "Calibration keys and pictures in overlays" is ticked in the overlay runtime's
  Settings… (Installed list, Overlay runtime), from the next time an overlay comes to the front.
- The log says once per chord — registered through Carbon or captured by the event tap —
  when a key is on VoiceOver's modifier while VoiceOver is running, and `host.keys.check`
  reports `"voiceover"` for any such chord whether VoiceOver is running or not.

The overlays themselves use `Alt+<key>`, `Ctrl+<key>` and `Ctrl+Shift+<key>`, and their
macOS question is a different one:

- `Alt` becomes **Option**, which on a Mac is the layer that types accented characters. That
  is harmless for a shortcut the platform consumes, but a combination it fails to claim
  types a character into the plugin rather than doing nothing.
- `Ctrl` becomes **Command**, which is the application-menu layer. An overlay key there
  takes that shortcut from the application for as long as the overlay is active: Kontakt's
  `Ctrl+S`, `Ctrl+P` and `Ctrl+N` are Command-S, Command-P and Command-N, which are the DAW's
  Save, Print and New everywhere else. The overlay runtime's go-to-tab keys `Ctrl+1` to
  `Ctrl+9` are Command-1 to Command-9, the Mac's own tab keys. Its tab-cycling keys are
  picked per platform and stay **Control-Tab** and Control-Shift-Tab (`Meta+Tab`), because
  Command-Tab is the application switcher.
- A combination the system keeps for itself — Command-Q, Command-H, Command-M, Command-Tab,
  Command-Space and the rest of the list under `host.keys.check` — is refused as a hotkey. A
  capture of one is not refused, and the log says so once per chord.

A module that wants another combination on a Mac picks one per platform with
`host.os.pick { windows = …, macos = … }`. In a spec the Mac's Control key is written `Meta`
(or `Win`).

## Where this is not the whole story

The application is not sandboxed and is not distributed through the App Store — the
accessibility APIs are forbidden inside the App Sandbox, so that channel is permanently
closed to it. See [architecture-feasibility-study.md](architecture-feasibility-study.md) §6
for what proper distribution requires.
