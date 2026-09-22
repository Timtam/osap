# macOS permissions

*Four switches now, and none can be granted by the application. Three of them, when
missing, do not produce an error — they produce plausible wrong answers, which for someone
who cannot see the screen is the worst possible failure. The count grew with Automation,
whose absence is the quietest of the lot: the setting reads as on and nothing happens. This
page is what to grant, how to check, and what each absence looks like.*

## The short version

**System Settings → Privacy & Security**, then:

| Permission | Needed for | What it looks like when missing |
| --- | --- | --- |
| **Accessibility** | reading and clicking anything | every plugin looks empty; no overlay ever activates |
| **Screen Recording** | capture, image search, OCR | **captures silently return the desktop wallpaper** — never an error |
| **Input Monitoring** | intercepting and suppressing keys | overlay keys reach the plugin instead of the overlay |
| **Automation** → VoiceOver | speaking *through* VoiceOver | the setting is on and the overlay still speaks in its own voice |

Only the first two need granting for the platform to work at all; **Automation** is asked
for separately, and only if you switch on "Speak through VoiceOver". **Input Monitoring normally follows the Accessibility
grant** rather than needing one of its own — observed across three sessions on one machine:
it read *unknown* before Accessibility was granted, *granted* immediately afterwards without
that pane ever being opened, and denied again once a rebuild invalidated the Accessibility
grant. It is listed because it can be switched off independently, and because if it ever
disagrees with Accessibility, that disagreement is itself the finding.

The pane is called **System Settings → Privacy & Security** on Ventura and later, and
**System Preferences → Security & Privacy → Privacy** on Monterey and earlier. The log names
whichever one this machine actually has.

**On Monterey, expect no Screen Recording prompt.** The application asks for the permission
at launch, and asking is what normally enrols it in that list — but on macOS 12 the request
frequently raises no dialog and adds no entry. Measured, and reported independently by a
second person for a different permission on the same OS version. Add it by hand: unlock the
padlock, press **+**, choose `AutomationPlatform.app`.

Whether the application has to be restarted afterwards depends on the permission, and both
halves were measured on a Mac mini with macOS 14.5 on 2026-09-18:

- **Accessibility takes effect in the running application**, within a few seconds. Press
  *Re-check now* on the Permissions page to confirm it; no restart.
- **Screen Recording does not.** macOS hands it only to a process that started *after* it was
  granted, so **quit and reopen the application** after granting it. The running one keeps
  the old answer; this is the most common reason a permission appears not to have worked.
- **Input Monitoring** usually follows Accessibility by itself. If *Re-check now* still shows
  it missing, quit and reopen — this one has not been measured either way.
- **Automation** (VoiceOver speech) needs no restart.

## Checking, without being able to see

The application asks the system what it has been granted and writes the answer to its log
at every startup, in a block near the top:

```
[env] accessibility: trusted
[env] screen recording: granted
[env] display 1: 1512x982 pt, scale 2.0 (primary)
[env] voiceover: running
[env] bundle: com.automationplatform.app
```

That block is the first thing to read when something does not work, and the first thing to
send. It sits beside the `.app` as `automation-platform.log`, or in
`~/Library/Application Support/AutomationPlatform/` if the folder beside the app was not
writable — the log's own header says which one it chose.

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

**Input Monitoring** is needed only for keys the overlay *takes away* from the plugin —
and, as above, is usually granted implicitly with Accessibility, since a process trusted for
Accessibility is allowed to listen to events. What follows is what its absence would mean if
it ever were absent on its own:
Tab, the arrows, Space inside an overlay. Global shortcuts do **not** need it: those are
registered through an older, narrower mechanism precisely so the main interaction keeps
working before the fussiest permission has been granted. So an application with
Accessibility but not Input Monitoring is half-alive: shortcuts open overlays, and
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

The remedy is to make the record match the application again: **remove the entry from the
list (select it, press −), then add it back**, and restart the application. Toggling the
switch off and on is usually not enough — the stale requirement stays attached to the entry.

`./macos-signing-identity.sh` stops it recurring.

## Permissions and rebuilds

macOS records these against an application's **code identity**, not against its path or its
name. An ad-hoc signature — `codesign -s -`, the default here — derives that identity from
the contents of the binary, so **every rebuild is a different application** and all three
permissions are forgotten. Measured on a tester's second run: everything back to "NOT
granted", and the probe reporting no focused window because accessibility reads were being
refused. On Monterey that also means re-adding the app to Screen Recording by hand, since no
prompt appears there.

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
  exist in a run started with `AUTOMATION_PLATFORM_CALIBRATE=1`.
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
