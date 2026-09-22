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

Speaks `text`; `opts.interrupt` defaults to `true` (omitting `opts` also means interrupt). Output is **not** echoed to the console — a screen reader reading the terminal would double the speech. The call itself does not check whether the module is enabled: a disabled module's callbacks are not called, but its entry file still runs at load, and a line spoken there is heard.

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

## host.speech.engines() {#host-speech-engines}

**Signature:** `host.speech.engines()` → `{ { id: string, name: string, screenReader: boolean, available: boolean } }`

Everything that could speak on this machine, and whether it can right now.

`id` is the only field to compare — lower case, no punctuation, stable across a screen reader renaming itself between versions. `name` is what the thing calls itself and is meant to be **said**, not parsed. `screenReader` distinguishes the user's own reader — their voice, their rate, their reading order, their braille display — from a plain speech engine. `available` means it could take a line this second, which for a screen reader means it is running.

```luau
for _, e in host.speech.engines() do
  if e.available and e.screenReader then
    host.log.info("could speak through " .. e.name)
  end
end
```

The answer is a snapshot from a moment ago and a fresh look is started behind it, so the call costs microseconds rather than the 35 ms a look takes — every callback, captured key and hotkey waits for the event loop, and a look while a speech engine is opening was measured at 2.7 seconds. For "which screen readers are running" a moment-old answer is the right kind: it changes when somebody starts or quits one, not between two lines of Luau.

**It can be empty just after start-up, and that is not the same as "nothing can speak here."** The first snapshot is taken behind the scenes rather than at launch, deliberately — a session that says nothing should not pay for opening a synthesiser — so a module that asks immediately may be told there is nothing. Measured: on one macOS machine the list arrived after 282 ms, while on one Windows machine two consecutive runs took 500 ms and **5500 ms** before a plain voice could be chosen. A module that picks a voice should keep looking rather than ask once and believe the answer; `tools/speech-probe` is written that way and says how long it waited.

### Windows

Nine entries once the list has filled, always the same nine, because they are what the application was compiled to reach: `nvda`, `jaws`, `zoomtext`, `zdsr`, `pctalker`, `boypcreader`, `sensereader` — and `sapi` and `onecore`, which are Windows itself and therefore always available. Before it has filled there are none; see above.

### macOS

`voiceover` first, then every installed system voice by its AVFoundation identifier —
`com.apple.voice.compact.en-GB.Daniel` and its like. Those identifiers are stable across
sessions, which is what a module storing a choice needs.

`voiceover` is the odd one and the only one with `screenReader` true: it is not a voice but a
**transport**, handing the line to the user's own reader, at their rate, in their reading
order, and to their braille display — none of which a voice can do. It reports `available`
only while VoiceOver is running.

**Every voice in this list is usable, Personal ones included.** macOS only shows a Personal
Voice at all once it has been authorised, so its presence *is* its availability — an entry
here marked unavailable would describe a state that cannot occur. Getting one into the list
is what the "Offer my Personal Voice to modules" switch does.

---

## host.speech.use(id) {#host-speech-use}

**Signature:** `host.speech.use(id: string?)` → `boolean`

Chooses what speaks for **this module**. `nil` returns it to the ordinary path.

```luau
-- A module that wants its own voice, so its announcements are not mistaken for the reader's
if not host.speech.use("sapi") then
  host.log.info("SAPI is not available here; staying with the usual voice")
end
```

Returns `false` when the engine is not there — unknown to this build, or a screen reader that is not running. It refuses rather than accepting and quietly speaking somewhere else, because a call that reports success and does something different is a promise not kept.

**The choice belongs to the module VM**, which is the unit this API can honestly promise. `host.speech` falls through to the VM owner rather than to the module that defined the code, so a `use` written inside a shared framework belongs to whichever module inherited it — and that is the module somebody installed and enabled, which makes it the right owner of the decision.

Two things worth knowing before reaching for it. The first line after a choice may wait for the engine to open, measured at 2.0 s for SAPI and 3.5 s for OneCore; that happens on the new voice's own thread, so nothing else waits with it and the lines queue rather than being lost. And `interrupt` only ever applied to the engine being addressed — two engines are two queues, so a module speaking through its own voice will not cut off what the overlay is saying through the user's. That is the same everyday situation as another application talking over a screen reader, not a new one.

A chosen engine that later stops answering falls through to the ordinary path rather than to silence.

### Windows

Any id from `host.speech.engines()` that reports `available`.

### macOS

Any id from `host.speech.engines()` that reports `available`, including `voiceover`.

There is no per-voice worker here, unlike Windows: a voice is a property of each utterance
rather than of the synthesiser, so choosing one costs nothing and the first line after a
choice is not slower than any other.

**A Personal Voice appears here only once it is allowed.** macOS does not let an application
see that such a voice exists until the user has permitted that application to use it, so
this list cannot offer one as a choice-that-asks: its presence is already the permission. The
asking happens on the **"Offer my Personal Voice to modules"** switch in the Application
settings, which is a deliberate act by the person at the keyboard rather than something a
module can trigger. Needs macOS 14 or later; on anything older the API does not exist and
nothing here pretends otherwise.

---

## host.speech.engine() {#host-speech-engine}

**Signature:** `host.speech.engine()` → `string?`

The id this module chose, or `nil` when it is on the ordinary path.

```luau
local mine = host.speech.engine()
host.log.info(mine and ("speaking through " .. mine) or "speaking the usual way")
```

It answers what was **chosen**, not what is currently speaking: a choice that cannot be honoured falls through silently to the ordinary path, and this call still reports the choice. What is actually speaking is a question for the log, which names the voice as it opens.

