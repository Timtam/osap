---
title: "host.ocr + host.input + host.sound — recognition, input synthesis & audio"
sidebar_position: 3
---

These three namespaces cover screen text recognition (`host.ocr`), mouse/keyboard input synthesis (`host.input`), and audio playback (`host.sound`). All coordinates are screen pixels.

## host.ocr.recognize(opts?)

Recognizes text inside a screen region and returns the full text plus per-word bounding boxes.

**Signature:** `host.ocr.recognize(opts: { region?: { x1, y1, x2, y2 }, lang?: string }?) -> { text: string, words: { { text: string, x: number, y: number, w: number, h: number } } }`

`opts` is optional. The `region` is given as corner coordinates `{ x1, y1, x2, y2 }` (also accepted positionally as `{ [1]=x1, [2]=y1, [3]=x2, [4]=y2 }`); missing corners default to `x1=0, y1=0` and `x2/y2` = screen width/height, so omitting `region` scans the whole primary display. `lang` is an optional OCR language hint (e.g. `"en"`). The internal capture rectangle is `(x1, y1, x2-x1, y2-y1)`.

The returned table always has:
- `text` — the full recognized string for the region.
- `words` — an array; each entry is `{ text, x, y, w, h }` where `x`/`y` are the word's top-left in **absolute screen coordinates** (the region origin `x1,y1` is added back to the per-word offset), and `w`/`h` are the box size.

On a backend OCR failure the call raises a Lua error.

```luau
local res = host.ocr.recognize({ region = { x1 = 100, y1 = 200, x2 = 500, y2 = 240 }, lang = "en" })
print(res.text)
for _, word in ipairs(res.words) do
  print(word.text, word.x, word.y, word.w, word.h)
end
```

## host.input.cursorPos()

Returns the current mouse cursor position in screen coordinates.

**Signature:** `host.input.cursorPos() -> { x: number, y: number }`

Takes no arguments. Returns a table `{ x, y }`.

```luau
local p = host.input.cursorPos()
print(p.x, p.y)
```

## host.input.move(x, y)

Moves the mouse cursor to the given screen coordinates.

**Signature:** `host.input.move(x: number, y: number) -> nil`

Both arguments are integers. Returns `nil`.

```luau
host.input.move(960, 540)
```

## host.input.click(x, y, opts?)

Moves to `(x, y)` and synthesizes a mouse click there.

**Signature:** `host.input.click(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

`x`/`y` are integers. The optional `opts.button` selects the button; it is matched case-insensitively, with `"right"` and `"middle"` recognized and anything else (including omitting `opts`) defaulting to **left**. Returns `nil`.

```luau
host.input.click(300, 200)                       -- left click
host.input.click(300, 200, { button = "right" }) -- right click
```

## host.input.drag(x1, y1, x2, y2, opts?)

Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases.

**Signature:** `host.input.drag(x1: number, y1: number, x2: number, y2: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

All four coordinates are integers. `opts.button` behaves exactly as in `click` (defaults to left). Returns `nil`.

```luau
host.input.drag(100, 100, 400, 300)
host.input.drag(100, 100, 400, 300, { button = "middle" })
```

## host.input.scroll(x, y, amount)

Moves to `(x, y)` and scrolls the mouse wheel by `amount` notches.

**Signature:** `host.input.scroll(x: number, y: number, amount: number) -> nil`

All three arguments are integers. `amount` is in wheel notches (each notch is one `WHEEL_DELTA` of 120); positive scrolls up, negative scrolls down. Returns `nil`.

```luau
host.input.scroll(960, 540, -3) -- scroll down three notches
```

## host.input.send(combo)

Sends a keyboard shortcut by pressing the modifiers, tapping the key, and releasing in reverse order.

**Signature:** `host.input.send(combo: string) -> nil`

`combo` is a single string of `+`-separated parts, with the final part being the key and any leading parts being modifiers (whitespace around parts is trimmed). Recognized modifiers (case-insensitive): `ctrl`/`control`, `alt`/`option`, `shift`, and `win`/`super`/`cmd`/`command`/`meta`. An empty combo or an unknown modifier raises a Lua error. Returns `nil`.

```luau
host.input.send("Ctrl+S")
host.input.send("Ctrl+Shift+Esc")
```

## host.input.text(text)

Types a Unicode string as synthetic keystrokes.

**Signature:** `host.input.text(text: string) -> nil`

`text` is sent character-by-character as Unicode input. Returns `nil`.

```luau
host.input.text("Hello, world!")
```

## host.sound.play(path)

Plays an audio file from the module's package directory, fire-and-forget.

**Signature:** `host.sound.play(path: string) -> nil`

`path` is resolved relative to the calling module's package root. The audio output device is opened lazily on first use; if no output device is available, or the file is missing or cannot be decoded, the failure is logged and the call still returns `nil` (no error is raised). Playback is detached, so the call returns immediately and the sound finishes on its own. Returns `nil`.

```luau
host.sound.play("assets/ding.wav")
```

