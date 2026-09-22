---
title: "host.ocr — text recognition"
sidebar_position: 9
toc_max_heading_level: 2
---

Recognises text in a screen region. Use it for what a plug-in or a game paints rather than publishes: a menu row, a live read-out, a field whose accessibility element disappears exactly when it is needed.

**Use [`host.ocr.read`](#host-ocr-read).** It photographs the region at the moment of the call, recognises it on a thread of its own, and hands the answer to a callback on the event loop, so the loop — which also carries speech, hotkeys, captured keys, timers and, on macOS, the event tap — never waits for a recogniser. The two older calls, [`recognize`](#host-ocr-recognize) and [`recognizeMany`](#host-ocr-recognizemany), are for code that cannot wait for a callback: they do the same work on the event loop and block it until they return.

It is the last of the three ways of reading a plug-in to reach for, and the right one more often than that suggests — an embedded Kontakt's file menu carries no accessibility information whatsoever, so its rows are read and matched by name rather than clicked at a measured offset (an offset slipped by one row once and overwrote the user's default multi), and ON:EAR drops its accessibility tree the moment the caret enters its search box.

It is also the most expensive of the three. For a tiny read-out the cost is mostly capture — a 67x13 read-out recognises in 4–6 ms while the capture under it is the same fixed ~17 ms frame as any other — but recognition grows with the region and the amount of text in it: reading control labels on a focus step is estimated at 50–200 ms (a cost ranking in [the screen frame-sharing design](../screen-frame-sharing-design.md), not a recorded measurement). That is why values which have to agree with each other are read in one call: every region of a call comes from one picture.

Ask for the region you actually want and no wider. The regions of one call are deliberately recognised separately, because the second engine that rescues a lone digit on Windows runs only for a small region the primary one returned nothing for, and Melodyne's first version merged its two read-outs into a single strip and lost the note name entirely.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["ocr"]
```

See [what that list is and is not](./index.md#capabilities).

## host.ocr.read(what, opts?, cb) {#host-ocr-read}

Photographs one region or several at the moment of the call, recognises them off the event loop, and calls `cb` with the answer.

**Signature:** `host.ocr.read(what: Region | Entry | { Region | Entry }, opts: { lang: string | { string }?, key: string? }?, cb)` → `nil`

- `Region` is the [region form](./screen.md#region-form), read strictly as it says there and by the same reader as `host.screen.cells`: corners `{ x1, y1, x2, y2 }` or `{ x1 = …, y1 = …, x2 = …, y2 = … }` in screen coordinates, `x2` and `y2` exclusive, all four of them whole numbers and not empty or turned around; or `{ window = w, fraction = { x1, y1, x2, y2 } }`, fractions of a window's client area, turned into a rectangle at the call. There is no default region: `{}` raises.
- `Entry` is `{ region = Region, name = string? }`. The name comes back on the reading, and indexes the second callback argument.
- A **list** is an array of regions and entries, up to 64. The regions of one call are photographed together, from one moment, and each is recognised on its own.
- `opts.lang` is a language tag like `"de"` or `"de-DE"`, or a list of them in order of preference; left out, the user's language. See [Recognition language](#recognition-language).
- `opts.key` names what the read is *for*, so that a newer read with the same key replaces an older one still waiting — see below.
- `cb` goes last; `opts` may be left out: `host.ocr.read(region, cb)`.

The callback gets one **reading** for a single region or entry, and `(list, byName)` for a list — even a list of one: `list` is an array of readings in the order given, and `byName` indexes the named ones. A reading is plain data:

| Field | |
|---|---|
| `name` | The entry's name; absent for a bare region. |
| `x`, `y`, `w`, `h` | The rectangle that was read, in screen coordinates; all four `0` for a window region that was not read (see `"failed"` below). |
| `status` | `"text"`, `"blank"`, `"none"`, `"failed"` or `"stale"` — see below. |
| `newer` | `true` when a newer read with the same `key` was asked for after this one. |
| `text` | The rows joined with `"\n"`; `""` unless `status` is `"text"`. |
| `lines` | The rows, top to bottom: `{ text, x, y, w, h, words }` each. |
| `words` | Every row's words in reading order: `{ text, x, y, w, h, approx }` each, in screen coordinates. `approx` is `true` for a box the host shared out by the text's length rather than one an engine located, and absent otherwise. |
| `lang` | The language the read resolved to, as the platform names it (`"de-DE"`), on every reading of a call that was recognised — `"failed"` ones included: a region the recogniser failed on, and a window region that was not read while the call's other regions were. `""` when nothing was recognised: the capture failed, no language asked for is available, the read was refused or pushed out (too many reads waiting, the recogniser not answering), the recogniser failed inside the application, or no region of the call could be read; and on a `"stale"` reading. |
| `error` | Why, when `status` is `"failed"`. |

What the statuses mean:

- `"text"` — something was read. `words` is never empty then, whichever recogniser answered.
- `"blank"` — the region has nothing drawn in it, and no recogniser was asked (see the platform sections).
- `"none"` — the recognisers ran and read nothing.
- `"failed"` — it could not be read: the capture failed, a window region was not read (below), no language asked for is available, too many reads were waiting, or the recogniser is not answering. `error` says which. A **window region is not read** when its client area is empty at the call — a minimised window; `error` is `"the window's client area is empty (0x0)"` — or when, at the window's size then, it would take the call past 40 million pixels (see **Limits**); the call's other regions are read all the same. When no region of a call can be read, nothing is photographed or recognised: the readings come on the next turn of the loop.
- `"stale"` — a newer read with the same `key` was asked for before this one was recognised, so it never was. `newer` is `true`.

`words` is non-empty exactly when `text` contains something other than white space. A **row** is the host's, not an engine's: engine lines that overlap vertically by at least half the smaller height are one row, left to right, their text joined with a space. That is what makes `lines` the same shape on both platforms — Vision returns two read-outs side by side as two lines a pixel apart, Windows as whatever its engine decided.

**When the picture is taken.** At the call, normally within one screen frame: a thread that does nothing but capture takes it, so a picture never waits behind a recognition. `host.input.*` and [`host.window.focus`](./window.md#host-window-focus), called by the same module afterwards, wait — up to 50 ms — until that module's pending pictures are taken, and those pictures go before every other read's meanwhile, so **read, then act** is safe: a read followed by a click sees the screen before the click. If the picture is not taken within 50 ms — a capture already running for another read takes longer — the input goes ahead and the log says so, once per module.

**Delivery.** The callback runs exactly once, on the event loop, on the next turn of the loop after the recognition finished (the loop turns every 15 ms), while the module is enabled. It is **dropped** — never called — when the module is disabled, reloaded or removed first, the way a one-shot [`host.timer.after`](./timer.md#host-timer-after) is: a callback that clicked or spoke minutes later, over whatever window was in front by then, would be worse than none. Enabled again, the module simply reads again. A callback that raises is reported like any other module callback error.

**`key`, `stale` and `newer`.** A read with a key supersedes the same module's earlier reads with that key that have **not started recognising yet**; those are answered `"stale"`. A read whose recognition had already started is answered with what it read, and `newer = true` if a newer read with its key was asked for meanwhile. So a poll slower than its own period still makes progress — every recognition that starts is delivered — and code that must not speak about a focus that has moved on returns on `newer`. Without a key a read is never superseded.

**Order and priority.** Answers for different keys may arrive out of the order they were asked in, because a read asked for while the host dispatches a key, a hotkey, a game-controller press, a window coming forward, a focus change or a trigger's report of the window in front is recognised before reads asked from a [`host.timer.every`](./timer.md#host-timer-every) poll or while the module loads. A [`host.timer.after`](./timer.md#host-timer-after) callback, an image search's callback and a read's own callback run with the priority of the dispatch that asked for them. A poll's read that has waited half a second goes first all the same — to be photographed and to be recognised — so a stream of key presses or controller events cannot hold it back for good. There is nothing to set: which is which is the host's to infer.

**What raises.** Only mistakes in the call, at the call: `what` not a table; a region the [region form](./screen.md#region-form)'s strict reading refuses — a corner missing (`{}` included) or not a whole number, corners empty or turned around, a mixed or unknown key, a window table without `client`, a fraction that is not a finite number; an entry without `region` or with an unknown field; more than 64 regions, or none; corners that cover more than 40 million pixels in one call together; a name used twice; an option other than `lang` and `key`; a `lang` that is not a well-formed tag or a list of them; a `key` that is not a non-empty string; `cb` not a function. Nothing that goes wrong routinely raises: it arrives as a reading with a `status`, so code that only looks at `.text` sees `""`. The callback never receives `nil`.

**Limits.** 64 regions and 40 million pixels per call. Corners past the pixel limit raise, as above. Window regions are counted after the corners, in the order given and at the size they resolve to at the call, because that size is the window's, not the module's: one that would take the call past the limit is answered `"failed"`, and the next may still fit, so the same call does not raise at full screen and work in a smaller window. 16 reads per module waiting or running at once: the 17th makes the oldest one without a key fail with `"too many reads waiting — put regions that belong together in one call (up to 64)"`, or fails itself when every waiting read has a key. 64 jobs waiting or running in the whole application, where a job is one picture and its recognition: reads that share one (below) count once, and the 65th distinct one fails at once. When the recogniser has answered no region for 5 seconds, new reads fail at once with `"the text recogniser has not answered a region for N s"` until it answers one — a long job whose regions keep coming back is not that. Two reads asking for the same thing (the same regions, language and way of capturing) before the first is photographed share one picture and one recognition, across modules too.

**Cost.** On the event loop: checking the arguments and queueing, microseconds; building the reading tables when the answer comes. Everything else is on the two OCR threads: the capture (a fixed ~17 ms frame on Windows, whatever the size) and the recognition (4–6 ms for a small read-out on Windows, tens to hundreds of milliseconds for a large region). A read that took 100 ms or more from the call to the answer gets one log line naming where the time went — waiting for the capture, the capture, waiting for the recogniser, the recognition — at most one per module every 10 seconds.

```luau
-- A game menu, read eight times a second; the item is said when it changes. The regions are
-- fractions of the game's client area, so they follow the window at any size and scaling.
-- Needs "ocr", "timer", "window" and "speech" in the manifest.
local GAME = { app = { name = "mygame" } }
local ITEM, VALUE = { 0.09, 0.08, 0.41, 0.11 }, { 0.42, 0.08, 0.5, 0.11 }
local last = ""
host.timer.every(125, function()
  local w = host.window.active()
  if not (w and host.window.test(GAME, w)) then last = ""; return end
  local menu = {
    { name = "item",  region = { window = w, fraction = ITEM } },
    { name = "value", region = { window = w, fraction = VALUE } },
  }
  host.ocr.read(menu, { key = "menu" }, function(list, byName)
    if byName.item.status ~= "text" then return end -- stale, blank, none, failed: the next tick asks again
    local now = byName.item.text .. " " .. byName.value.text
    if now ~= last then
      last = now
      host.speech.output(now, { interrupt = true })
    end
  end)
end)
```

```luau
-- Tab moves to the next field: its name is said at once, its value when the read arrives.
-- Needs "ocr", "keys" and "speech" in the manifest.
local FIELDS = {
  { name = "Cutoff", value = { 300, 200, 360, 216 } },
  { name = "Resonance", value = { 300, 230, 360, 246 } },
}
local at = 0
host.keys.capture("Tab", function()
  at = at % #FIELDS + 1
  local field = FIELDS[at]
  host.speech.output(field.name, { interrupt = true })
  host.ocr.read(field.value, { key = "value" }, function(r)
    if r.newer or r.status ~= "text" then return end -- Tab was pressed again meanwhile
    host.speech.output(r.text, { interrupt = false })
  end)
end)
```

### Windows

The picture comes from the module's [capture source](./screen.md#which-picture-a-read-sees). The standard way photographs the regions' bounding box once and cuts it up when that box is no larger than four times the regions' own area (or 65 536 pixels, whichever is larger) and no larger than 8 million pixels; otherwise, or when the box comes back clipped at a screen edge, each region is photographed on its own. Desktop duplication takes every region as a piece of one frame in one request. Under `fallback = "none"`, a picture duplication cannot give is a reading with `status = "failed"` and an `error` beginning `screen capture failed: desktop duplication could not answer`.

Each region is then recognised exactly as `recognize` recognises it. A region of 400x200 or less is cropped to its content and enlarged; a crop that finds no content is `"blank"`, and `Windows.Media.Ocr` is not asked. For such a small region the second, neural recogniser starts at the same moment as `Windows.Media.Ocr`, and its answer is used only when `Windows.Media.Ocr` reads nothing — the lone digit. That recogniser reads without locating, so its text's words get boxes shared out by length inside the content crop it read, marked `approx = true`. It is a Latin-script model and reads with its own character set whatever `lang` says (see [Recognition language](#recognition-language)). A larger region goes to `Windows.Media.Ocr` as captured, with no crop and so never `"blank"`.

A new `Windows.Media.Ocr` engine is created for every region. The two OCR threads are named `screen-capture` and `ocr-recognise`. At exit, a job stops before its next region, the exit waits up to a second for the two threads and then up to half a second for neural recognitions still running, and no neural recognition starts after that; no callback runs by then.

### macOS

The picture is taken at the display's full backing resolution — twice the points on a Retina display — through the same capture as `recognize`: one capture of the bounding box by the same rule as on Windows, and one per region when the box would be wasteful or comes back a different size than asked. Without the Screen Recording permission macOS answers with a picture of the wallpaper rather than an error, so such a read reads the wallpaper — usually `"none"` or `"blank"` — rather than failing; a capture that could not be taken at all is `"failed"`.

Each region then runs the same pipeline as `recognize`: Vision at the accurate level with language correction off, the content crop and the blank guard for a region of 400x200 points or less, and the retry ladder — the whole region, then enlarged, then the fast model — abandoned after 250 ms. The fast model is asked only for a language it reads. A read asked from a poll skips the enlarged and fast rungs while a read asked from a key press is waiting behind it. Exactly one language is handed to Vision: the one `lang` resolved to.

The recognise thread asks for the user-initiated quality of service.

## host.ocr.languages() {#host-ocr-languages}

The languages the platform's recogniser reads, the one a read without `lang` uses first.

**Signature:** `host.ocr.languages()` → `{ string }`

Tags as the platform spells them (`"de-DE"`, `"en-US"`, `"zh-Hans"`), each once. The list is read by the recognise thread as the first thing it does after the application starts; a call made before that has finished waits for it for up to 50 ms and then answers from what is known — `{}` — and the log says so, once. It is read again when a language did not resolve, at most every 30 seconds, so a language installed while the application runs is picked up without a restart.

Never raises. Costs a lock and a copy of a short list; the event loop waits only in the case above.

```luau
local langs = host.ocr.languages()
host.log.info("text recognition reads: " .. table.concat(langs, ", "))
```

### Windows

The OCR languages installed on this machine, from `OcrEngine.AvailableRecognizerLanguages`: a language whose optical character recognition component is installed. First comes the one a read without `lang` uses — the first language in the user's Windows language list (`GlobalizationPreferences.Languages`) that is installed, then English, then the first installed.

### macOS

What Vision reads at the accurate level on this macOS version (`supportedRecognitionLanguages`), whatever is installed: Vision's list is part of the system. First comes the first of the user's preferred languages (`NSLocale.preferredLanguages`) that Vision reads, then English, then Vision's first.

## host.ocr.resolveLanguage(lang?) {#host-ocr-resolvelanguage}

Which language a read with this `lang` would use here.

**Signature:** `host.ocr.resolveLanguage(lang: string | { string } | nil)` → `string?`

`lang` is what a read takes: a tag, a list of tags in order of preference, or `nil` for the user's language. Returns the platform's tag it resolves to — `"de"` answers `"de-DE"` where that is installed — or `nil` when nothing here reads it, or when the list is not known yet (see [`languages`](#host-ocr-languages)). Raises when `lang` is not a well-formed tag, a list of them or `nil`. Costs what `languages` does, plus the matching.

```luau
-- Asked a second after the module loads, not while it loads: in the first moments after the
-- application starts the list may not be known yet, and this would answer nil for German even
-- where it is installed. Needs "ocr" and "timer" in the manifest.
host.timer.after(1000, function()
  if #host.ocr.languages() == 0 then return end -- still not known, or nothing installed
  if not host.ocr.resolveLanguage("de") then
    host.log.info("German text recognition is not available; reading in "
      .. tostring(host.ocr.resolveLanguage(nil)))
  end
end)
```

### Windows

Resolves against the installed OCR languages. `nil` is the first language in the user's Windows language list that is installed, then English, then the first installed.

### macOS

Resolves against what Vision reads at the accurate level. `nil` is the first of the user's preferred languages that Vision reads, then English, then Vision's first.

## host.ocr.recognize(opts?) {#host-ocr-recognize}

Recognizes text inside a screen region and returns the full text plus per-word bounding boxes — **on the event loop, which waits until it returns.** Use [`read`](#host-ocr-read) unless the answer is needed before the calling function returns: an overlay's `text` provider, an `identify` that returns a boolean.

**Signature:** `host.ocr.recognize(opts: { region?: { x1, y1, x2, y2 }, lang?: string }?) -> { text: string, words: { { text: string, x: number, y: number, w: number, h: number } }, skipped: boolean, error?: string }`

`opts` is optional. The `region` is given as corner coordinates `{ x1, y1, x2, y2 }` (also accepted positionally as `{ [1]=x1, [2]=y1, [3]=x2, [4]=y2 }`), read exactly as the [region form](./screen.md#region-form) says: missing corners default to `x1=0, y1=0` and `x2/y2` = screen width/height, so omitting `region` scans the whole primary display, and `x2`/`y2` are exclusive — the capture rectangle is `(x1, y1, x2-x1, y2-y1)`. `lang` is a language tag and goes through the same matching as `read`'s (see [Recognition language](#recognition-language)); leaving it out keeps each platform's own default, which is not the same on the two.

Synchronous: the capture and the recognition run on the event loop and block speech, hotkey and key callbacks, timers and, on macOS, the event tap until the call returns — the capture a fixed ~17 ms on Windows, the recognition 4–6 ms for a small read-out and far more for a large region.

The returned table always has:
- `text` — the full recognized string for the region.
- `words` — an array; each entry is `{ text, x, y, w, h }` where `x`/`y` are the word's top-left in **absolute screen coordinates** (the region origin `x1,y1` is added back to the per-word offset), and `w`/`h` are the box size.
- `skipped` — `true` when the **blank guard answered instead of the engine**: the region was small enough to be cropped to its content, the crop found no ink, and the system recogniser was never asked (on Windows the second one has already started by then, and its answer is dropped; see the Windows section below) — so the empty `text` and `words` are the guard's answer, not a reading. `false` whenever the guard did not fire, which includes every other way of getting an empty result; those are different faults and the log names them.

A `lang` that no recogniser here reads is answered, not raised: `{ text = "", words = {}, skipped = false, error = "language: … is not available here (available: …)" }`. A `lang` that is not a well-formed tag raises. On a capture or recognition failure the call raises a Lua error **on Windows**; see the platform sections below, because macOS never raises here and a failure is indistinguishable from an empty region — unless `skipped` says so.

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

A failed capture comes back as an error and is **raised into Lua**, and so does a failed recognition, so `pcall` is a meaningful guard.

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

For a region of 400x200 or less a second recogniser runs alongside the system one and its answer is used when the system engine returns nothing — which is the lone-digit case, the thing `Windows.Media.Ocr` refuses. In that case the call waits for the second recogniser to finish, with no time limit; when the system engine did answer, the call returns at once and the second recogniser finishes on its own thread, unused. At exit the application waits up to half a second for any that are still running, and logs how long it waited, or that it gave up. That fallback recognises without locating, so when it answers it sets `text` and leaves **`words` empty** here (`read` gives the same text approximate word boxes instead). It ignores `lang` and reads Latin script only (see [Recognition language](#recognition-language)).

The blank guard is that same crop step, so it exists only for a region of 400x200 or less: when the tightened crop finds no content the call returns `{ text = "", words = {}, skipped = true }` without asking the system engine. The second recogniser has already been started by then — it starts before the crop — and finishes on its own thread with its answer unused. A larger region is never checked and always reaches the system engine, so `skipped` is `false` for it whatever it contains.

### macOS

**Nothing in this path returns an error.** A failed capture and a refused recognition request both log and return `{ text = "", words = {} }`. A `pcall` guard around this call is dead code here, and a broken Screen Recording permission is indistinguishable from a genuinely empty region — the only symptom is a read-out that is permanently blank. The one `error` is the unavailable `lang` above.

There is no second engine: the dependency is compiled for Windows only. Small text is carried by a retry ladder — tightened crop, then the whole region, then bigger and faster — abandoned after 250 ms, and `text` and `words` always agree with each other.

A region with **nothing in it is not recognised at all**, on either platform. Two shapes count as nothing: one flat colour, and a filled panel with nothing drawn on it — a value field with no value. The second is the one that matters, because the crop finds the *well*, which used to open the retry ladder and end at the character model reading an empty box. A recogniser asked about a blank rectangle does not answer "nothing"; it answers whatever its network makes of noise, and neither a module nor the person listening can tell that from a reading.

That branch is the one `skipped` reports: `true` means Vision was not asked, and the log carries a line saying so at the same moment. Like Windows it applies only to a region of 400x200 or less — a larger one goes to Vision as captured, with no crop and so no guard. The capture failures above return empty with `skipped = false`, so the two empties this platform produces can be told apart from Lua even though neither raises.

So the same call fails in **opposite shapes**: empty `words` with real `text` on Windows for a lone digit, and empty `text` with real `words` here when the ladder runs out of budget. `read` has neither: its `words` are non-empty exactly when its `text` is.

## host.ocr.recognizeMany(opts) {#host-ocr-recognizemany}

Recognizes several regions from **one** screen capture, so that values which have to agree with each other come from the same instant — **on the event loop, which waits until every region is recognised.** [`read`](#host-ocr-read) with a list does the same off the loop.

**Signature:** `host.ocr.recognizeMany(opts: { regions: { x1, y1, x2, y2 }[], lang?: string }) -> { { text: string, words: {…}, skipped: boolean, error?: string } }[]`

Returns one entry per region, in the order given, each shaped like `host.ocr.recognize`'s result — word boxes in absolute screen coordinates (each region's own top-left is added to what the engine reported inside it), and `skipped` set per region. A region that could not be read comes back as `{ text = "", words = {}, skipped = false, error = "…" }` rather than as a hole, so `results[2]` is always the second region's answer. Inside each region table the [region form](./screen.md#region-form)'s defaults apply, but this call is stricter than `opts.region` elsewhere: a `regions` entry that is not a table raises, a missing `regions` raises, and the list ends at the first `nil`, so the regions after a hole are silently not read. One `lang` applies to every region, through the same matching as `recognize`'s; a `lang` no recogniser here reads gives every entry that language `error`, and one that is not a well-formed tag raises. Synchronous, like `recognize`: every region is recognised before the call returns, the capture and all the recognitions on the event loop.

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

What `lang` means, and what leaving it out means.

`lang` is a BCP 47 language tag — `"de"`, `"de-DE"`, `"pt-BR"`, `"zh-Hant"` — and `read` and `resolveLanguage` also take a list of them in order of preference. The host matches it against the languages the platform's recogniser reads ([`languages`](#host-ocr-languages)) and hands the engine exactly one of them, spelt as the platform spells it: `"de"` reads with `"de-DE"`, `"de-CH"` with `"de-DE"` when that is the German there is, `"en"` prefers `"en-US"`, `"zh-TW"` finds `"zh-Hant"` and `"zh-CN"` finds `"zh-Hans"`. The first tag of a list that matches anything wins. Case and `_` for `-` do not matter. The same code therefore reads German on both platforms without [`host.os.pick`](./os.md#host-os-pick).

When nothing matches, `read` answers each region `status = "failed"` with an `error` naming what was asked and what is available, and what to do about it; `recognize` and `recognizeMany` answer the same `error` in their own shape. The log says so once per module and request (once per tag for `recognize` and `recognizeMany`). A `lang` that is not a well-formed tag (`"english"`, `"de--DE"`, `""`) is a mistake in the call and raises. Tesseract's three-letter codes (`"eng"`, `"deu"`) are well-formed tags, but no platform lists a language under them, so they match nothing and are answered like any other language that is not available here: write `"en"` and `"de"`.

Leaving `lang` out means the user's language for `read`: the first of the user's own languages the recogniser reads, then English, then the first it reads. `recognize` and `recognizeMany` keep each platform's own default instead, as they always have (see below). Either way it does not mean "whatever the text is": a German Windows reads an English plug-in with the German engine, which reads English words well and a word with an umlaut in an English list less well.

```luau
-- German text, whatever the user's own language is; English where German is not installed.
host.ocr.read({ 300, 200, 620, 240 }, { lang = { "de", "en" } }, function(r)
  if r.status == "failed" then
    host.log.info(r.error)
    return
  end
  host.log.info(r.lang .. ": " .. r.text)
end)
```

### Windows

The languages are the OCR languages installed on the machine; a language whose optical character recognition component is not installed does not match, and the `error` says how to add it (Settings, Time & language, Language & region). Left out in `recognize` and `recognizeMany`, the engine is created from the **user-profile languages** (`TryCreateFromUserProfileLanguages`): the first language in the user's Windows language list that OCR supports.

The second recogniser that answers for small regions (see `recognize`) ignores `lang` altogether. It is a Latin-script model: its character set has the German umlauts but no `ß`, the whole Greek alphabet in both cases, and symbols, but no Cyrillic or East Asian script.

### macOS

The languages are what Vision reads on this macOS version at the accurate level; the fast model's own, shorter list decides only whether the last rung of the ladder may use it. Left out in `recognize` and `recognizeMany`, nothing is set and Vision's own default applies. If Vision refuses the request, that is logged once per session and the result is empty.
