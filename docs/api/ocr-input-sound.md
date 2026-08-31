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

On a backend OCR failure the call raises a Lua error **on Windows**; see the platform sections below, because macOS never raises here and a failure is indistinguishable from an empty region.

```luau
local res = host.ocr.recognize({ region = { x1 = 100, y1 = 200, x2 = 500, y2 = 240 }, lang = "en" })
print(res.text)
for _, word in ipairs(res.words) do
  print(word.text, word.x, word.y, word.w, word.h)
end
```

### Windows

A failed capture and an unavailable OCR language both come back as errors and are **raised into Lua**, so `pcall` is a meaningful guard.

For a region of 400x200 or less a second recogniser runs alongside the system one and its answer is used when the system engine returns nothing — which is the lone-digit case, the thing `Windows.Media.Ocr` refuses. That fallback recognises without locating, so when it answers it sets `text` and leaves **`words` empty**.

### macOS

**Nothing in this path returns an error.** A failed capture, a refused recognition request and an unknown language code all log and return `{ text = "", words = {} }`. A `pcall` guard around this call is dead code here, and a broken Screen Recording permission is indistinguishable from a genuinely empty region — the only symptom is a read-out that is permanently blank.

There is no second engine: the dependency is compiled for Windows only. Small text is carried by a retry ladder — tightened crop, then the whole region, then bigger and faster — abandoned after 250 ms, and `text` and `words` always agree with each other.

So the same call fails in **opposite shapes**: empty `words` with real `text` on Windows for a lone digit, and empty `text` with real `words` here when the ladder runs out of budget.

## host.ocr.recognizeMany(opts)

Recognizes several regions from **one** screen capture — on Windows. macOS does not implement it and falls back to one capture per region; the platform sections below say what that costs.

**Signature:** `host.ocr.recognizeMany(opts: { regions: { x1, y1, x2, y2 }[], lang?: string }) -> { { text: string, words: {…}, error?: string } }[]`

Returns one entry per region, in the order given, each shaped like `host.ocr.recognize`'s result — with word boxes still in absolute screen coordinates relative to **that** region. A region that could not be read comes back as `{ text = "", words = {}, error = "…" }` rather than as a hole, so `results[2]` is always the second region's answer.

**Why:** recognition is cheap and the capture is not. Measured on the reference machine, a 67×13 read-out recognizes in 4–6 ms while the capture underneath costs a fixed ~17 ms compositor frame whatever its size — so two adjacent read-outs, read one after the other, spend two thirds of their time photographing the screen twice. A watcher polling two boxes at 120 ms measured 44 ms per tick with two calls and 27 ms with one.

The regions are **not** merged into a single wider recognition, and that is the point of the call rather than an oversight: the fallback to the neural recognizer fires per region and only for a region that came back empty, and a merged strip is never empty — so a value the primary recognizer dropped would stay dropped while its neighbour came through. Only the capture is shared.

```luau
local r = host.ocr.recognizeMany({ regions = {
  { x1 = 220, y1 = 61, x2 = 287, y2 = 74 },
  { x1 = 300, y1 = 61, x2 = 367, y2 = 74 },
} })
host.log.info(r[1].text .. " / " .. r[2].text)
```

### Windows

As documented: one capture of the bounding box of every region, cropped per region — falling back to one capture each when a region is degenerate or the capture came back clipped.

### macOS

**Not implemented — the shared default applies, which is one full capture per region**, each a separate round trip at a separate instant. The promise this call exists to make is therefore not kept here: two read-outs that must agree with each other, a note name and its cent offset say, can come from different moments and contradict each other.

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

### Windows

The pointer is placed, and nothing is told that it moved. A control watching for motion while its button is held can miss the move entirely.

### macOS

A real move event is posted — or a **drag** event when a button is currently held, which is also what the implicit move inside `mouseDown` and `mouseUp` does.

So the press-then-glide-then-release gesture, composed from `mouseDown`, a timer and `mouseUp`, is a genuine drag here and can be an invisible warp on Windows: a slider that follows the pointer on a Mac may not move at all there. Where the movement itself is the point, use `host.input.drag`, which paces it on both.

## host.input.click(x, y, opts?)

Moves to `(x, y)` and synthesizes a mouse click there.

**Signature:** `host.input.click(x: number, y: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

`x`/`y` are integers. The optional `opts.button` selects the button; it is matched case-insensitively, with `"right"` and `"middle"` recognized and anything else (including omitting `opts`) defaulting to **left**. Returns `nil`.

```luau
host.input.click(300, 200)                       -- left click
host.input.click(300, 200, { button = "right" }) -- right click
```

## host.input.drag(x1, y1, x2, y2, opts?)

Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases — with real, paced movement in between.

A drag is not a press, a warp and a release. On Windows the pointer is moved with injected move events rather than `SetCursorPos`, interpolated over sixteen steps with a few milliseconds between them, because two kinds of control tell the difference: one that watches for motion while its button is held sees injected events and can miss a warp entirely, and one that reads the *speed* of a drag answers an instantaneous jump with an enormous change. macOS posts a drag event between press and release, which it has always done. The pacing blocks the calling thread for about sixty milliseconds, which is only ever paid when somebody deliberately drags something.

If a control needs a sustained press before it will react at all, use `mouseDown`/`mouseUp` with a timer instead — that is why those exist separately.

### Windows

The pointer is moved with injected move events rather than `SetCursorPos`, interpolated over sixteen steps a few milliseconds apart. The pacing blocks the calling thread for about sixty milliseconds, paid only when something is deliberately dragged.

### macOS

A single `Dragged` event is posted between press and release — no interpolation. That asymmetry is deliberate: a scrollbar needs one intermediate event and gets it. A control that only reacts to *continuous* movement would work on Windows and not here, and nothing has yet exercised the difference on a Mac.

**Signature:** `host.input.drag(x1: number, y1: number, x2: number, y2: number, opts: { button?: "left" | "right" | "middle" }?) -> nil`

All four coordinates are integers. `opts.button` behaves exactly as in `click` (defaults to left). Returns `nil`.

```luau
host.input.drag(100, 100, 400, 300)
host.input.drag(100, 100, 400, 300, { button = "middle" })
```

## host.input.scroll(x, y, amount)

Moves to `(x, y)` and scrolls the mouse wheel by `notches`, which may be fractional.

**Signature:** `host.input.scroll(x: number, y: number, notches: number) -> nil`

`x` and `y` are integers. `notches` is in wheel notches and **need not be a whole number**; positive scrolls up, negative scrolls down. Returns `nil`.

A notch is 120 units to the operating system, and a control decides for itself what one is worth — ON:EAR's tone knob moves by five of its own units per notch, which is as fine as this could ask for while notches were whole. Half a notch is 60 units, and an application that scales its step by the delta moves by half as much. One that rounds to its own interval instead simply does not move and reports the same value back, which is the honest outcome and says to stop asking for halves.

### Windows

The unit is native: 120 units to a notch is what `WHEEL_DELTA` counts, so a fractional request is expressed exactly.

### macOS

Expressed in Quartz **lines**, which cannot be smaller than one. A caller asking for less than a line gets the smallest whole line rather than nothing — a control that needs a half-step will not get one here until this asks Quartz in pixel units instead. The conversion logs the first scroll of a run, and after that every request that is not a whole notch, saying what was asked for and what was sent; the difference shows up as evidence rather than as a mystery.

```luau
host.input.scroll(960, 540, -3)   -- down three notches
host.input.scroll(960, 540, 0.5)  -- half a notch up, where the control is finer than a notch
```

## host.input.send(combo)

Sends a keyboard shortcut by pressing the modifiers, tapping the key, and releasing in reverse order.

**Signature:** `host.input.send(combo: string) -> nil`

`combo` is a single string of `+`-separated parts, with the final part being the key and any leading parts being modifiers (whitespace around parts is trimmed). Recognized modifiers (case-insensitive): `ctrl`/`control`, `alt`/`option`, `shift`, and `win`/`super`/`cmd`/`command`/`meta`. An empty combo or an unknown modifier raises a Lua error. Returns `nil`.

```luau
host.input.send("Ctrl+S")
host.input.send("Ctrl+Shift+Esc")
```

### Windows

The modifiers are synthesised as real key presses around the key, and whatever the user is **physically holding is inherited**. Called from a hotkey callback while Alt is still down, `host.input.send("Escape")` arrives as Alt+Escape — which is why `host.keys.modifiersDown()` exists and why modules defer a synthesised key until the user has let go.

### macOS

The modifiers are set as flags on the event, and setting them **replaces the whole set**, so anything the user is holding is stripped and the same call delivers a bare Escape. No modifier key event is posted at all, so an application that watches for physical modifier presses sees an unmodified key.

The deferral dance is therefore unnecessary for *keys* here — but not for clicks: a synthesised click carries no flag-clearing of its own, so one posted while the user holds Alt is an Alt+click on both platforms.

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

