---
title: "host.ocr — text recognition"
sidebar_position: 9
toc_max_heading_level: 2
---

Recognises text in a screen region. Use it for what a plug-in paints rather than publishes: a menu row, a live read-out, a field whose accessibility element disappears exactly when it is needed.

It is the last of the three ways of reading a plug-in to reach for, and the right one more often than that suggests — an embedded Kontakt's file menu carries no accessibility information whatsoever, so its rows are read and matched by name rather than clicked at a measured offset (an offset slipped by one row once and overwrote the user's default multi), and ON:EAR drops its accessibility tree the moment the caret enters its search box.

It is also the most expensive of the three. For a tiny read-out the cost is mostly capture — a 67x13 read-out recognises in 4–6 ms while the capture under it is the same fixed ~17 ms frame as any other — but recognition grows with the region and the amount of text in it: reading control labels on a focus step is estimated at 50–200 ms (a cost ranking in [the screen frame-sharing design](../screen-frame-sharing-design.md), not a recorded measurement). That is why two values which have to agree with each other are read with one `recognizeMany` — measured at 27 ms a tick against 44 for two separate calls.

**Both calls are synchronous.** Capture and recognition run on the event loop, the one thread that also carries speech, hotkeys, timers and the keyboard hook, and all of those wait until the call returns. There is no asynchronous form, and what else a call costs on each platform is in the platform sections of `recognize`. So OCR does not belong on every tick of a poll: detect that something changed with [`imageSearchAsync`](./screen.md#host-screen-imagesearchasync), [`imageSearchEach`](./screen.md#host-screen-imagesearcheach) or a [`profile`](./screen.md#host-screen-profile), and read the text only when it did. A callback that holds the loop too long costs more than the delay; see [A slow callback](./timer.md#a-slow-callback).

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

**Signature:** `host.ocr.recognize(opts: { region?: { x1, y1, x2, y2 }, lang?: string }?) -> { text: string, words: { { text: string, x: number, y: number, w: number, h: number } }, skipped: boolean, error?: string }`

`opts` is optional. The `region` is given as corner coordinates `{ x1, y1, x2, y2 }` (also accepted positionally as `{ [1]=x1, [2]=y1, [3]=x2, [4]=y2 }`), read exactly as the [region form](./screen.md#region-form) says: missing corners default to `x1=0, y1=0` and `x2/y2` = screen width/height, so omitting `region` scans the whole primary display, and `x2`/`y2` are exclusive — the capture rectangle is `(x1, y1, x2-x1, y2-y1)`. `lang` is the engine's own language identifier, not a portable hint, and leaving it out means something different on each platform — see [Recognition language](#recognition-language).

Synchronous: it runs on the event loop and blocks speech, hotkeys, timers and the keyboard hook until it returns (see the top of this page).

The returned table always has:
- `text` — the full recognized string for the region.
- `words` — an array; each entry is `{ text, x, y, w, h }` where `x`/`y` are the word's top-left in **absolute screen coordinates** (the region origin `x1,y1` is added back to the per-word offset), and `w`/`h` are the box size.
- `skipped` — `true` when the **blank guard answered instead of the engine**: the region was small enough to be cropped to its content, the crop found no ink, and the recogniser was never asked — so the empty `text` and `words` are the guard's answer, not a reading. `false` whenever the guard did not fire, which includes every other way of getting an empty result; those are different faults and the log names them.

On a backend OCR failure the call raises a Lua error **on Windows**; see the platform sections below, because macOS never raises here and a failure is indistinguishable from an empty region — unless `skipped` says so.

```luau
local res = host.ocr.recognize({ region = { x1 = 100, y1 = 200, x2 = 500, y2 = 240 } })
print(res.text)
for _, word in ipairs(res.words) do
  print(word.text, word.x, word.y, word.w, word.h)
end
if res.skipped then
  print("nothing drawn there: the recogniser was not asked")
end
```

### Windows

A failed capture and an unavailable OCR language both come back as errors and are **raised into Lua**, so `pcall` is a meaningful guard.

One exception, for a module that declared `[screen] capture = "duplication"` with `fallback = "none"` (see [Which picture a read sees](./screen.md#which-picture-a-read-sees)): when duplication has no picture to give — a fullscreen switch, a UAC prompt, the moment it is still opening — the call does **not** raise but returns `{ text = "", words = {}, skipped = false, error = "screen capture failed: …" }`, the shape `recognizeMany` uses for a failed region. For such a module a missing picture is routine, and a raised error would put a module-error dialog in front of the application it is reading. A region that can never be read — an empty one, or one larger than 40 million pixels — and a recognition that fails still raise.

```luau
-- In a module with [screen] capture = "duplication", fallback = "none":
local res = host.ocr.recognize({ region = { 300, 200, 620, 240 } })
if res.error then
  return   -- no picture this time; ask again on the next tick
end
host.speech.output(res.text)
```

`Windows.Media.Ocr` is asked synchronously — the call waits on its `RecognizeAsync` — and a new engine is created for every call, and for every region of a `recognizeMany`.

For a region of 400x200 or less a second recogniser runs alongside the system one and its answer is used when the system engine returns nothing — which is the lone-digit case, the thing `Windows.Media.Ocr` refuses. In that case the call waits for the second recogniser to finish, with no time limit. That fallback recognises without locating, so when it answers it sets `text` and leaves **`words` empty**. It ignores `lang` and reads Latin script only (see [Recognition language](#recognition-language)).

The blank guard is that same crop step, so it exists only for a region of 400x200 or less: when the tightened crop finds no content the call returns `{ text = "", words = {}, skipped = true }` before either recogniser runs. A larger region is never checked and always reaches the system engine, so `skipped` is `false` for it whatever it contains.

### macOS

**Nothing in this path returns an error.** A failed capture and a refused recognition request both log and return `{ text = "", words = {} }`. A `pcall` guard around this call is dead code here, and a broken Screen Recording permission is indistinguishable from a genuinely empty region — the only symptom is a read-out that is permanently blank.

There is no second engine: the dependency is compiled for Windows only. Small text is carried by a retry ladder — tightened crop, then the whole region, then bigger and faster — abandoned after 250 ms, and `text` and `words` always agree with each other.

A region with **nothing in it is not recognised at all**, on either platform. Two shapes count as nothing: one flat colour, and a filled panel with nothing drawn on it — a value field with no value. The second is the one that matters, because the crop finds the *well*, which used to open the retry ladder and end at the character model reading an empty box. A recogniser asked about a blank rectangle does not answer "nothing"; it answers whatever its network makes of noise, and neither a module nor the person listening can tell that from a reading.

That branch is the one `skipped` reports: `true` means Vision was not asked, and the log carries a line saying so at the same moment. Like Windows it applies only to a region of 400x200 or less — a larger one goes to Vision as captured, with no crop and so no guard. The capture failures above return empty with `skipped = false`, so the two empties this platform produces can be told apart from Lua even though neither raises.

So the same call fails in **opposite shapes**: empty `words` with real `text` on Windows for a lone digit, and empty `text` with real `words` here when the ladder runs out of budget.

## host.ocr.recognizeMany(opts) {#host-ocr-recognizemany}

Recognizes several regions from **one** screen capture, so that values which have to agree with each other come from the same instant.

**Signature:** `host.ocr.recognizeMany(opts: { regions: { x1, y1, x2, y2 }[], lang?: string }) -> { { text: string, words: {…}, skipped: boolean, error?: string } }[]`

Returns one entry per region, in the order given, each shaped like `host.ocr.recognize`'s result — word boxes in absolute screen coordinates (each region's own top-left is added to what the engine reported inside it), and `skipped` set per region. A region that could not be read comes back as `{ text = "", words = {}, skipped = false, error = "…" }` rather than as a hole, so `results[2]` is always the second region's answer. Inside each region table the [region form](./screen.md#region-form)'s defaults apply, but this call is stricter than `opts.region` elsewhere: a `regions` entry that is not a table raises, a missing `regions` raises, and the list ends at the first `nil`, so the regions after a hole are silently not read. One `lang` applies to every region. Synchronous, like `recognize`: every region is recognised before the call returns.

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

As documented: one capture of the bounding box of every region, cropped per region — falling back to one capture each when a region is degenerate or the capture came back clipped. A module that reads through [desktop duplication](./screen.md#which-picture-a-read-sees) gets each region read separately from one frame instead, because there the cost grows with the area and the bounding box would be the expensive way round.

### macOS

The same, with the crop done by drawing the enclosing capture into a smaller context and letting it clip rather than by `CGImageCreateWithImageInRect`, whose rectangle is documented in the image's own coordinate space — a convention this port has no way to test.

Each region then runs the identical pipeline a single `recognize` would, including the retry ladder and the blank guard, so a value read this way is the value that call would have given — and its `skipped` is the one that call would have set.

## Recognition language {#recognition-language}

What `lang` means to each platform's engine, and what leaving it out means.

`lang`, in both calls, is **not a hint**. It is the platform engine's own language identifier, passed to that engine unchanged, and the two engines use different lists. Leaving it out does not mean English or "whatever the text is": it means each platform's own default, which differs between the two (see below). A `lang` that is neither a string nor a number is ignored rather than refused.

Leave `lang` out unless the default reads your text wrongly, and when you do give one, give it per platform with [`host.os.pick`](./os.md#host-os-pick), so that each engine is handed a name from its own list. Tesseract's three-letter codes (`"eng"`, `"deu"`) are in neither list.

```luau
-- German text on a machine whose default language may be another one. Each platform gets its
-- own value: Windows a tag of its OCR list, macOS nothing here, so Vision keeps its default.
local LANG = host.os.pick { windows = "de-DE" }
local REGION = { 300, 200, 620, 240 }
local ok, res = pcall(host.ocr.recognize, { region = REGION, lang = LANG })
if not ok then
  -- Windows raises when the German OCR component is not installed: log that, and read
  -- with the user-profile languages instead.
  host.log.info("German OCR unavailable, reading with the default: " .. tostring(res))
  res = host.ocr.recognize({ region = REGION })
end
host.speech.output(res.text)
```

### Windows

A BCP-47 language tag (`"en-US"`, `"de-DE"`) of a language whose Windows OCR component is installed. The host builds a language from it and asks `Windows.Media.Ocr` for an engine for exactly that language, with no check beforehand, so a language whose component is not installed raises (`OCR failed: …`) from `recognize`, and becomes that region's `error` in `recognizeMany`.

Omitted, the engine is created from the **user-profile languages** (`TryCreateFromUserProfileLanguages`): the first language in the user's Windows language list that OCR supports. On a German Windows that is German, whatever language the application being read is in.

The second recogniser that answers for small regions (see `recognize`) ignores `lang` altogether. It is a Latin-script model: its character set has the German umlauts but no `ß`, the whole Greek alphabet in both cases, and symbols, but no Cyrillic or East Asian script.

### macOS

A Vision recognition-language identifier, as Vision lists them (`"en-US"`, `"de-DE"`), handed to the request as its only recognition language. Omitted, nothing is set and Vision's own default applies. If Vision refuses the request, that is logged once per session and the result is empty — `text = ""`, no error, no raise. What Vision does with an identifier it does not know has not been observed.
