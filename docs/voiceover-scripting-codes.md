---
title: VoiceOver's scripting codes
sidebar_position: 9
---

# What VoiceOver's Apple Event codes are, and why they are written down here

Speech on macOS currently reaches VoiceOver by launching `/usr/bin/osascript` once per line
with `tell application "VoiceOver" to output "…"`. The alternative is to build that Apple
Event ourselves and send it in-process — no child process, no AppleScript component, and an
explicit timeout on the reply.

What blocked it was terminology. VoiceOver does not ship its scripting definition as a file
inside its bundle, so it cannot be copied off a Mac we do not have, and only the Xcode
command line tools' `sdef` can extract it. On 2026-08-20 the macOS tester installed those
tools and ran

```bash
sdef /System/Library/CoreServices/VoiceOver.app > voiceover.sdef 2>&1
```

on macOS 12.7.6, and sent back 14 KB of dictionary. This page records the parts that matter,
so nobody has to ask for it again. Apple's file itself is not copied into this repository.

## The one command we need

| | |
| --- | --- |
| Suite | `VOAS` — "VoiceOver Suite" |
| Command | `output`, code **`VOASoutp`** |
| Event class | `VOAS` |
| Event ID | `outp` |
| Direct parameter | the text to say (`keyDirectObject`, `----`); it also accepts an *outputable* object |
| Optional parameter | `with`, keyword **`with`**, of enumeration `spel` |
| Target | bundle identifier `com.apple.VoiceOver` |

So the in-process form is: build the event with
`NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID`,
attach the string with `setParamDescriptor_forKeyword` under `keyDirectObject`, and send it
with `sendEventWithOptions_timeout_error` — which returns a `Result` and takes a timeout, so
a wedged VoiceOver cannot park a thread the way a child process could.

**This is not a decision to build it.** The rule set when the measurement was designed still
stands: if the milliseconds in the log track the length of the line, `output` blocks until
the phrase has been spoken, and no change of transport buys anything. Only a fixed cost above
roughly 100 ms on the oldest machine we support justifies the rewrite. These codes were the
expensive half of finding out *how*; the numbers are still the half that decides *whether*.

## The `with` parameter, which is more interesting than it looks

`with` takes enumeration `spel`, and it has exactly two values:

| Name | Code | Meaning |
| --- | --- | --- |
| alphabetic spelling | `alpS` | spell it out, letter by letter |
| phonetic spelling | `phoS` | spell it phonetically — alpha, bravo, charlie |

That is a real capability for this project rather than a curiosity. An overlay reads values
off a plugin's screen with OCR, and OCR is least reliable exactly where being wrong matters
most: a preset name, a file name, a serial number. Being able to ask VoiceOver to spell one
back — on demand, as a second key on the same control — is something the platform's own voice
cannot do at all.

## The rest of the suite, for later

Recorded because obtaining it cost a tester session, not because anything uses it yet.

| Command | Code | Note |
| --- | --- | --- |
| `perform command` | `VOASperC` | runs a VoiceOver command by name |
| `perform action` | `VOASpera` | |
| `press` / `release` | `VOASpres` / `VOASrele` | |
| `click` | `VOASclik` | |
| `select` | `VOASsele` | |
| `move` | `coremove` | moves the VoiceOver cursor |
| `open` | `VOASopen` | a menu or a resource |
| `close menu` | `VOASclos` | |
| `grab screenshot` | `VOASshot` | screenshots the VoiceOver cursor, returns a path |
| `save` | `VOASsave` | saves the last phrase |
| `copy to pasteboard` | `VOAScopy` | copies the last phrase — the same one-utterance limit as VO-Shift-C |
| `quit` | `aevtquit` | the standard one |

The application object also exposes `vo cursor` (`vocu`), `commander` (`cmmd`) and
`mouse cursor` (`mocu`) as read-only properties, with the Cocoa class `SCRWorkspace`.

**One caution about `perform command`.** It looks like the answer to "we cannot claim
VoiceOver's keys" — we could ask VoiceOver to do the navigating instead. It is not: that
would still be VoiceOver's cursor moving through a window with nothing accessible in it,
which is the situation the overlays exist to escape. Worth knowing about, not worth building
on.
