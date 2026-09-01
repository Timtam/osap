---
title: "host.sound — a noise instead of a sentence"
sidebar_position: 10
toc_max_heading_level: 2
---

For playing an audio file shipped inside the module, where a sound says it faster than a sentence would and does not have to be listened to all the way through.

It is not part of the speech queue: the call returns immediately, the sound finishes on its own, and it neither interrupts what is being spoken nor waits for it. The path is package-relative and resolved against the calling module's own root — the opposite of the image templates in the overlay runtime, where a relative path is rejected as an error.

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

`path` is resolved relative to the calling module's package root. The audio output device is opened lazily on first use; if no output device is available, or the file is missing or cannot be decoded, the failure is logged and the call still returns `nil` (no error is raised). Playback is detached, so the call returns immediately and the sound finishes on its own. Returns `nil`.

```luau
host.sound.play("assets/ding.wav")
```
