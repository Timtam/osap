---
title: "host.ocr — text recognition"
sidebar_position: 8
toc_max_heading_level: 2
---

Recognises text in a screen region. Use it for what a plug-in paints rather than publishes: a menu row, a live read-out, a field whose accessibility element disappears exactly when it is needed.

It is the last of the three ways of reading a plug-in to reach for, and the right one more often than that suggests — an embedded Kontakt's file menu carries no accessibility information whatsoever, so its rows are read and matched by name rather than clicked at a measured offset (an offset slipped by one row once and overwrote the user's default multi), and ON:EAR drops its accessibility tree the moment the caret enters its search box.

It is also the most expensive of the three, and the cost is nearly all capture rather than recognition: a 67x13 read-out recognises in 4–6 ms while the capture under it is the same fixed ~17 ms frame as any other. That is why two values which have to agree with each other are read with one `recognizeMany` — measured at 27 ms a tick against 44 for two separate calls.

Ask for the region you actually want and no wider. The regions of one call are deliberately recognised separately, because the second engine that rescues a lone digit on Windows runs only for a small region the primary one returned nothing for, and Melodyne's first version merged its two read-outs into a single strip and lost the note name entirely.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["ocr"]
```

See [what that list is and is not](./index.md#capabilities).

## host.ocr.recognize(opts?) {#host-ocr-recognize}

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

## host.ocr.recognizeMany(opts) {#host-ocr-recognizemany}

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
