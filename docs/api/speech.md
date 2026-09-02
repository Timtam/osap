---
title: "host.speech — spoken output"
sidebar_position: 17
toc_max_heading_level: 2
---

Speaks a string, through whichever screen reader or voice the platform is configured to use.

It is the whole of a module's output. A self-voicing overlay has no window and no visible state, so everything the user is meant to learn — the label of the control Tab just landed on, the state a toggle came back with, the fact that a menu item was not found — leaves through here.

`interrupt` is the choice between cutting off what is being said and queueing behind it, and it defaults to cutting off, which is right when the user has just moved and the previous sentence is now about the wrong control. Queueing is for the second half of one announcement: the overlay runtime speaks a control's name at once and appends an image-read value when it arrives, because reading that value costs a screen capture — measured at 20–42 ms per focus step — and nothing is gained by the user waiting in silence for it.

Where the words actually come out — a screen reader, a speech engine, or VoiceOver — is not the caller's choice and not something a module needs to know.

**Braille goes with it.** There is no separate call for a braille display, and that is deliberate rather than an omission: on macOS there is no separate channel to have one for, because VoiceOver brailles whatever it is told to say. A `host.braille` would do something on one platform and nothing on the other, which is an API making a promise it cannot keep. So a line that is spoken is also written to the display, wherever the screen reader has one — on Windows through a single call that does both, on macOS because it always did.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["speech"]
```

See [what that list is and is not](./index.md#capabilities).

## host.speech.output(text, opts?) {#host-speech-output}

**Signature:** `host.speech.output(text: string, opts: { interrupt: boolean? }?)` → `nil`

Speaks `text`; `opts.interrupt` defaults to `true` (omitting `opts` also means interrupt). Output is **not** echoed to the console — a screen reader reading the terminal would double the speech.

```luau
host.speech.output("Reverb enabled")
host.speech.output("loading...", { interrupt = false })  -- queue, don't cut off
```

**Where it comes out**, which the caller does not choose and does not need to know:

- **Windows** — the running screen reader if there is one (NVDA, JAWS, ZoomText, ZDSR, PC-Talker, Boy PC Reader or Sense Reader), otherwise OneCore or SAPI. Speech reaches them through prism, compiled into the application, so nothing has to be installed alongside it. What is spoken also reaches a **braille display**, unless **Also send what is said to a braille display** is unticked in the Application settings tab. If the screen reader stops answering, the plain voice takes over within 300 ms and the screen reader is picked up again on its own, within three seconds of coming back.
- **macOS** — the platform's own voice, unless **Speak through VoiceOver** is ticked in the Application settings tab. Ticked, the line goes to VoiceOver and arrives in the user's voice, at their rate, and **on their braille display**, which nothing else can do.

  It is off until somebody asks for it, and the reason is the permission rather than the feature: the first line through this path is an Apple Event, and the first Apple Event makes macOS put an Automation consent dialog on screen. On by default, that dialog appears at startup — before the user has asked for anything, about a thing they may not want, in front of a person who cannot see it to dismiss it. Ticking the box is the request, and that is the moment to ask.

  Two things then send a line to the platform's own voice anyway:
  - **VoiceOver is not running.** Checked before every line, and deliberately not remembered: VoiceOver started later in the session simply starts being used. The check is also why the overlay cannot *start* VoiceOver — `tell application "VoiceOver"` would launch it, and a tool that switches on a screen reader nobody asked for is not acceptable behaviour.
  - **VoiceOver refuses**, most often because AppleScript control is not allowed. The line comes back and is said by the fallback, the reason is logged **once**, and the path is parked for the session so no further line pays for a process launch that will fail. Ticking the setting again re-arms it.

  Either way the line is said, and unticking the switch turns the path off again for anyone who prefers a second, distinct voice.

`interrupt` governs the platform's own queue. On the VoiceOver path, whether an announcement also cuts off what VoiceOver is saying for its own reasons is VoiceOver's decision, not one this API can make.

This call never raises: a failing speech engine must not take a module's key handler down with it.

---
