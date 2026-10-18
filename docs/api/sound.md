---
title: "host.sound — audio playback"
sidebar_position: 16
toc_max_heading_level: 2
---

Plays an audio file shipped inside the module. For the places where a sound is faster than a sentence and does not have to be listened to all the way through.

It is not part of the speech queue: the call returns immediately, the sound finishes on its own, and it neither interrupts what is being spoken nor waits for it. The path is package-relative and resolved against the root of the module whose code makes the call — the opposite of the image templates in the overlay runtime, where a relative path is rejected as an error.

A sound that cannot play is answered, not raised, so it cannot take a key handler down with it: no audio device, and a file outside the module's package that is missing or does not decode, come back as `false` and an [error value](./errors.md), and the log says so too. The one exception is a mistake in the module itself: a sound file **inside** its package that is missing or does not decode raises, as a mistyped template path does ([Return or raise](./errors.md#return-or-raise)). The VPS Avenger module plays its page's sounds through it.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["sound"]
```

See [what that list is and is not](./index.md#capabilities).

## host.sound.play(path) {#host-sound-play}

Plays an audio file from the module's package directory, fire-and-forget.

**Signature:** `host.sound.play(path: string)` → `true`, or `false, Error`

`path` is resolved relative to the root of the module whose code makes the call (see [the path rule](./index.md#paths)); an absolute path is used as it stands, and `..` is not removed. The file is opened and its header read first, then the audio output device is opened — lazily, on first use, and again at the next sound when there was none — and the sound is handed to it. Playback is detached, so the call returns immediately and the sound finishes on its own. Runs on the event loop: it opens the file and reads its header there, and the sound is decoded and played on the audio system's own thread.

Returns `true` once the sound is handed to the output, or `false` and an error value, nothing played, each also written to the log as its message:
- `unavailable` — `no audio output device: …`: no output device could be opened. The next sound tries again.
- `not_found` — `cannot play '…': …`, with the path as it was resolved and the system's words: a file outside the module's package that is not there.
- `invalid` — `cannot play '…': …`, with the decoder's words: a file outside the package that is not a sound it decodes (WAV, FLAC, Ogg Vorbis, MP3).
- `failed` — `cannot play '…': …`: a file outside the package that is there and cannot be opened — no right to read it, a folder, another program holding it.

**A file inside the module's package** — the path resolved under its root, `..` taken into account — **that cannot be opened or decoded raises** with the same `cannot play '…': …`, whatever the output device: a sound the module ships is part of it, and a mistyped name is a mistake to fix. A `path` that is not a string raises too.

**The output follows the device.** The output is opened on the system's default output device, and opened afresh — with one `[sound]` line — at the next play after either of these: its stream reported an error (the device was unplugged, switched off or taken away), or the default output device is another one than when it was opened. A sound still playing on the old output stops with it. Every play asks the system which device is the default: one query of the audio service, on the event loop.

```luau
local ok, err = host.sound.play("assets/ding.wav")  -- raises if the package has no such sound
if not ok then host.log.info("no ding: " .. err) end -- no output device, say
```

### Windows {#host-sound-play-windows}

The output is a WASAPI shared-mode stream (through `cpal`) on the default render device, and a device that is removed or invalidated is reported as a stream error. After the machine resumes from sleep, or the session is connected to the console or a remote client again, the output is let go and opened at the next sound, not before: nothing is held open for a sound that never comes.

### macOS {#host-sound-play-macos}

The output is a Core Audio stream (through `cpal`) on the default output device. It is opened again at the next play when its stream reported an error or the default output device is another one, as on Windows. After the Mac wakes from sleep, or this user's session is back at the console after fast user switching, the output is let go and opened at the next sound, not before, as on Windows.
