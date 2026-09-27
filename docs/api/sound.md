---
title: "host.sound — audio playback"
sidebar_position: 16
toc_max_heading_level: 2
---

Plays an audio file shipped inside the module. For the places where a sound is faster than a sentence and does not have to be listened to all the way through.

It is not part of the speech queue: the call returns immediately, the sound finishes on its own, and it neither interrupts what is being spoken nor waits for it. The path is package-relative and resolved against the root of the module whose code makes the call — the opposite of the image templates in the overlay runtime, where a relative path is rejected as an error.

The failures are all quiet. No audio device, a missing file, an undecodable one: each is logged and returns normally, so a sound that does not play cannot take a key handler down with it — and cannot announce itself either. Nothing under `modules/` reaches for this yet; every overlay in the project speaks instead.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["sound"]
```

See [what that list is and is not](./index.md#capabilities).

## host.sound.play(path) {#host-sound-play}

Plays an audio file from the module's package directory, fire-and-forget.

**Signature:** `host.sound.play(path: string) -> nil`

`path` is resolved relative to the root of the module whose code makes the call (see [the path rule](./index.md#paths)); an absolute path is used as it stands, and `..` is not removed. The audio output device is opened lazily on first use; if no output device is available, or the file is missing or cannot be decoded, the failure is logged and the call still returns `nil` (no error is raised). Playback is detached, so the call returns immediately and the sound finishes on its own. Returns `nil`. Runs on the event loop: it opens the file and reads its header there, and the sound is decoded and played on the audio system's own thread.

**The output follows the device.** The output is opened on the system's default output device, and opened afresh — with one `[sound]` line — at the next play after either of these: its stream reported an error (the device was unplugged, switched off or taken away), or the default output device is another one than when it was opened. A sound still playing on the old output stops with it. Every play asks the system which device is the default: one query of the audio service, on the event loop.

```luau
host.sound.play("assets/ding.wav")
```

### Windows

The output is a WASAPI shared-mode stream (through `cpal`) on the default render device, and a device that is removed or invalidated is reported as a stream error. After the machine resumes from sleep, or the session is connected to the console or a remote client again, the output is let go and opened at the next sound, not before: nothing is held open for a sound that never comes.

### macOS

The output is a Core Audio stream (through `cpal`) on the default output device. It is opened again at the next play when its stream reported an error or the default output device is another one, as on Windows. After the Mac wakes from sleep, or this user's session is back at the console after fast user switching, the output is let go and opened at the next sound, not before, as on Windows.
