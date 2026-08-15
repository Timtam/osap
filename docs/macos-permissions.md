# macOS permissions

*Three switches decide whether this application can do anything at all. None can be granted
by the application. Two of them, when missing, do not produce an error — they produce
plausible wrong answers, which for someone who cannot see the screen is the worst possible
failure. This page is what to grant, how to check, and what each absence looks like.*

## The short version

**System Settings → Privacy & Security**, then:

| Permission | Needed for | What it looks like when missing |
| --- | --- | --- |
| **Accessibility** | reading and clicking anything | every plugin looks empty; no overlay ever activates |
| **Screen Recording** | capture, image search, OCR | **captures silently return the desktop wallpaper** — never an error |
| **Input Monitoring** | intercepting and suppressing keys | overlay keys reach the plugin instead of the overlay |

The pane is called **System Settings → Privacy & Security** on Ventura and later, and
**System Preferences → Security & Privacy → Privacy** on Monterey and earlier. The log names
whichever one this machine actually has.

**On Monterey, expect no Screen Recording prompt.** The application asks for the permission
at launch, and asking is what normally enrols it in that list — but on macOS 12 the request
frequently raises no dialog and adds no entry. Measured, and reported independently by a
second person for a different permission on the same OS version. Add it by hand: unlock the
padlock, press **+**, choose `AutomationPlatform.app`.

After granting any of them, **quit and reopen the application**. macOS only hands a newly
granted permission to a process that started *after* it was granted; the running one keeps
the old answer until it is restarted. This is the single most common reason a permission
appears not to have worked.

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
Tab, the arrows, Space inside an overlay. Global shortcuts do **not** need it: those are
registered through an older, narrower mechanism precisely so the main interaction keeps
working before the fussiest permission has been granted. So an application with
Accessibility but not Input Monitoring is half-alive: shortcuts open overlays, and
navigating inside one leaks keys to the plugin.

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
first. Two things in this project sit there, and neither is an overlay shortcut:

- the **calibrator's** `Ctrl+Alt+Shift+S/T/V`, which only exist in a run started with
  `AUTOMATION_PLATFORM_CALIBRATE=1`, and
- the application's own **reload-everything** key, `Ctrl+Shift+Win+Alt+F5`. It also uses an
  F-key, so it additionally needs "Use F1, F2, etc. as standard function keys" turned on, or
  the `fn` key held down.

The overlays themselves use `Alt+<key>`, `Ctrl+<key>` and `Ctrl+Shift+<key>`, and their
macOS question is a different one:

- `Alt` becomes **Option**, which on a Mac is the layer that types accented characters. That
  is harmless for a shortcut the platform consumes, but a combination it fails to claim
  types a character into the plugin rather than doing nothing.
- `Ctrl` becomes **Control**, and `Control+<letter>` on macOS is the text-editing layer
  inherited from emacs — `Control+A`, `Control+E`, `Control+N`, `Control+P` all mean
  something inside any text field. A registered global shortcut wins, but it takes the key
  away from the plugin's own text entry while the overlay is active, and the macOS
  convention for a command would be `Command+<letter>` anyway.

So macOS module variants want their own key choices rather than the Windows ones. The
mechanism exists — a module can ask which platform it is on with `host.os.is("macos")` and
register accordingly — and the choice has not been made yet.

## Where this is not the whole story

The application is not sandboxed and is not distributed through the App Store — the
accessibility APIs are forbidden inside the App Sandbox, so that channel is permanently
closed to it. See [architecture-feasibility-study.md](architecture-feasibility-study.md) §6
for what proper distribution requires.
