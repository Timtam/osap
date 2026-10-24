---
title: "host.sound — audio playback"
sidebar_position: 16
toc_max_heading_level: 2
---

Plays an audio file shipped inside the module. For the places where a sound is faster than a sentence and does not have to be listened to all the way through.

It is not part of the speech queue: the call returns immediately, the sound finishes on its own, and it neither interrupts what is being spoken nor waits for it. The path is package-relative and resolved against the root of the module whose code makes the call — the opposite of the image templates in the overlay runtime, where a relative path is rejected as an error.

A sound that cannot play is answered, not raised, so it cannot take a key handler down with it: no audio device, and a file outside the module's package that is missing or does not decode, come back as `false` and an [error value](./errors.md), and the log says so too. The one exception is a mistake in the module itself: a sound file **inside** its package that is missing or does not decode raises, as a mistyped template path does ([Return or raise](./errors.md#return-or-raise)). The VPS Avenger module plays its page's sounds through it.

**What the call answers** is under its **Returns**, and what it can fail with under its **Errors** ([Reading an Errors table](./errors.md#reading-an-errors-table)). A message may change between versions; its code does not. Two raises are not repeated at the entry. One is the capability gate, for a module that did not declare `sound`. The other is `stopped by the host: …`, once the module has run too long ([the limits](../module-runtime-and-lifecycle.md#limits)).

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

**The output follows the device.** The output is opened on the system's default output device, and opened afresh — with one `[sound]` line — at the next play after either of these: its stream reported an error (the device was unplugged, switched off or taken away), or the default output device is another one than when it was opened. A sound still playing on the old output stops with it. Every play asks the system which device is the default: one query of the audio service, on the event loop.

```luau
local ok, err = host.sound.play("assets/ding.wav")  -- raises if the package has no such sound
if not ok then host.log.info("no ding: " .. err) end -- no output device, say
```

```luau
-- `chosen` is a full path the user typed in, outside the package: a missing file is answered.
local ok, err = host.sound.play(chosen)
if not ok and err.code == host.errors.NOT_FOUND then
  host.speech.output("That sound file is not there")
end
```

### Returns {#host-sound-play-returns}

- `true` — the file was opened, its header read, and the sound handed to the output. It plays on its own from there. Whether it is heard to the end is not reported: a sound still playing when the output is opened afresh stops with it.
- `false, err` — nothing was played. `err.code` is one of the codes under Errors, and the log has the message too, as a `[sound]` line.

### Errors {#host-sound-play-errors}

None of these is raised. Each comes as the second value. The file is checked first and the output after it, so a file that cannot play is answered for the file even when there is no output device either.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `not_found` | return | A file outside the module's package that is not there. The message has the path as it was resolved, and the system's words. | `cannot play '…': …` | both |
| `invalid` | return | A file outside the package that is there and is not a sound it decodes. It decodes WAV, FLAC, Ogg Vorbis and MP3. The message has the decoder's words. | `cannot play '…': …` | both |
| `failed` | return | A file outside the package that is there and cannot be opened: no right to read it, a folder, another program holding it. | `cannot play '…': …` | both |
| `unavailable` | return | The file is a sound, and no output device could be opened: the system has none, or none would open. `err.detail` has the audio system's words for the default device that would not open, and is `nil` when there is no device at all; the `[sound]` line has them after the message. Nothing is held open: the next sound tries again. | `no audio output device` | both |

**Raises** for a mistake in the call:

- a `path` that is neither a string nor a number. A number is taken as its digits, so `5` is the file `5`.
- a file **inside** the module's package that is missing, cannot be opened or does not decode. Inside means the path resolved under the module's root, with `..` taken into account; an absolute path into the package, such as one from [`host.path`](./path.md#host-path), is inside too. It raises with the same `cannot play '…': …`, whatever the output device: a sound the module ships is part of it, and a mistyped name is a mistake to fix ([Return or raise](./errors.md#return-or-raise)).

### Windows {#host-sound-play-windows}

The output is a WASAPI shared-mode stream (through `cpal`) on the default render device, and a device that is removed or invalidated is reported as a stream error. After the machine resumes from sleep, or the session is connected to the console or a remote client again, the output is let go and opened at the next sound, not before: nothing is held open for a sound that never comes.

### macOS {#host-sound-play-macos}

The output is a Core Audio stream (through `cpal`) on the default output device. It is opened again at the next play when its stream reported an error or the default output device is another one, as on Windows. After the Mac wakes from sleep, or this user's session is back at the console after fast user switching, the output is let go and opened at the next sound, not before, as on Windows.
