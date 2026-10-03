---
title: "host.ocr — text recognition"
sidebar_position: 9
toc_max_heading_level: 2
---

Recognises text in a screen region. Use it for what a plug-in or a game paints rather than publishes: a menu row, a live read-out, a field whose accessibility element disappears exactly when it is needed.

**Use [`host.ocr.read`](#host-ocr-read).** It photographs the region at the moment of the call, recognises it on a thread of its own, and hands the answer to a callback on the event loop, so the loop — which also carries speech, hotkeys, captured keys, timers and, on macOS, the event tap — never waits for a recogniser. The two older calls, [`recognize`](#host-ocr-recognize) and [`recognizeMany`](#host-ocr-recognizemany), are for code that cannot wait for a callback: they do the same work on the event loop and block it until they return.

It is the last of the three ways of reading a plug-in to reach for, and the right one more often than that suggests — an embedded Kontakt's file menu carries no accessibility information whatsoever, so its rows are read and matched by name rather than clicked at a measured offset (an offset slipped by one row once and overwrote the user's default multi), and ON:EAR drops its accessibility tree the moment the caret enters its search box.

It is also the most expensive of the three. For a tiny read-out the cost is mostly capture — a 67x13 read-out recognises in 4–6 ms while the capture under it, through the standard path on Windows, is the same fixed ~17 ms frame as any other — but recognition grows with the region and the amount of text in it: reading control labels on a focus step is estimated at 50–200 ms (a cost ranking in [the screen frame-sharing design](../screen-frame-sharing-design.md), not a recorded measurement). That is why values which have to agree with each other are read in one call: its regions are photographed together, from one picture wherever the platform can take them in one — the platform sections below say when it takes them one after another instead.

Ask for the region you actually want and no wider. The regions of one call are deliberately recognised separately, because of the second engine that rescues a lone digit on Windows: it starts at the same moment as the primary one for every region of up to 400x200 pixels, and its answer is used only when the primary one reads nothing in that region. In a merged strip the primary engine still reads the other values, so the neural answer is not used and a value the primary engine dropped stays dropped — Melodyne's first version merged its two read-outs into a single strip and lost the note name entirely.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["ocr"]
```

See [what that list is and is not](./index.md#capabilities).

## host.ocr.read(what, opts?, cb) {#host-ocr-read}

Photographs one region or several at the moment of the call, recognises them off the event loop, and calls `cb` with the answer. Given a [snapshot](./screen.md#host-screen-snapshot) the module holds, it reads the regions from that instead of photographing them.

**Signature:** `host.ocr.read(what: Region | Entry | { Region | Entry }, opts: { lang: string | { string }?, key: string?, snapshot: Snapshot? }?, cb)` → `nil`

- `Region` is the [region form](./screen.md#region-form), read strictly as it says there and by the same reader as `host.screen.cells`: corners `{ x1, y1, x2, y2 }` or `{ x1 = …, y1 = …, x2 = …, y2 = … }` in screen coordinates, `x2` and `y2` exclusive, all four of them whole numbers and not empty or turned around; or `{ window = w, fraction = { x1, y1, x2, y2 } }`, fractions of a window's client area, turned into a rectangle at the call. There is no default region: `{}` raises.
- `Entry` is `{ region = Region, name = string? }`. The name comes back on the reading, and indexes the second callback argument.
- A **list** is an array of regions and entries, up to 64. The regions of one call are photographed in one go and each is recognised on its own. Through desktop duplication that is one request, every region a piece of one frame; the standard way, it is one capture of their bounding box when that box is not wasteful, and otherwise one capture after another, all within the same job (see [Windows](#windows) and [macOS](#macos) below).
- `opts.lang` is a language tag like `"de"` or `"de-DE"`, or a list of them in order of preference; left out, the user's language. See [Recognition language](#recognition-language).
- `opts.key` names what the read is *for*, so that a newer read with the same key replaces an older one still waiting — see below.
- `opts.snapshot` reads the regions from that [snapshot](./screen.md#host-screen-snapshot) instead of photographing the screen — see **On a snapshot** below.
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

**When the picture is taken.** On a snapshot, never: the picture is the snapshot's, taken before the call, and the input barrier below does not wait for it. Otherwise at the call, normally within one screen frame: a thread that does nothing but capture takes it, so a picture does not wait behind a recognition — except that a read asked from a poll or while a module loads has its picture held back as long as the pictures waiting for recognition hold more than 256 MB (see **Limits** below). Through desktop duplication that thread waits up to 250 ms for it — while it opens, say — before the read is answered the standard way or, under `fallback = "none"`, fails (see [Windows](#windows)). The picture is the read's own: it is never shared with [`host.screen.cells`](./screen.md#host-screen-cells), [`matchCellsAsync`](./screen.md#host-screen-matchcellsasync) or an image search, which capture on the event loop or the image worker, so a state detected with one of those and a text read with this are two pictures, taken at two moments — unless both are given the same snapshot. The capture thread also takes [`host.screen.snapshotAsync`](./screen.md#host-screen-snapshotasync)'s pictures, one at a time with the reads', in the order [the top of that page](./screen.md) describes. `host.input.*` and [`host.window.focus`](./window.md#host-window-focus), called by the same module afterwards, wait — up to 50 ms — until that module's pending pictures are taken, and those pictures go before every other read's meanwhile, so **read, then act** is safe: a read followed by a click sees the screen before the click. If the picture is not taken within 50 ms — a capture already running for another read takes longer — the input goes ahead and the log says so, once per module.

**On a snapshot** (`opts.snapshot`). Nothing is photographed and no capture source is chosen, so the read makes no [first-read comparison](./screen.md#which-picture-a-read-sees): the recogniser reads the snapshot's pixels. Each region is cut to the part of the snapshot the recogniser can read — the whole snapshot on Windows, on macOS the part of it that was on the desktop — and its reading's `x`, `y`, `w`, `h` are that part. A region of which the snapshot holds nothing is not read: its reading is `"failed"` with `error = "the region does not overlap the snapshot"`, and the call's other regions are read. It goes to the recogniser at once and is recognised as a live read is, with the same keys, priorities, limits and delivery; it is never shared with another read. The part of the snapshot its regions cover counts toward the 256 MB of pictures waiting for the recogniser until it is recognised — so several reads of one snapshot count what each of them reads, not the whole snapshot each — and the read keeps the snapshot's pixels even when the snapshot is released meanwhile; those bytes are not charged against the snapshot budget. A `snapshot` that is not a Snapshot, or one that was released, raises.

```luau
-- The page title and the selected entry of a menu, from one picture: they cannot disagree.
-- Needs "ocr", "screen", "window" and "speech" in the manifest.
local w = host.window.active()
local snap = w and host.screen.snapshot({ region = { window = w, fraction = { 0, 0, 1, 1 } } })
if snap then
  host.ocr.read({
    { name = "page", region = { window = w, fraction = { 0.1, 0.02, 0.9, 0.08 } } },
    { name = "item", region = { window = w, fraction = { 0.09, 0.3, 0.41, 0.36 } } },
  }, { snapshot = snap }, function(list, byName)
    if byName.page.status == "text" and byName.item.status == "text" then
      host.speech.output(byName.page.text .. ", " .. byName.item.text)
    end
  end)
  snap:release()   -- the read keeps its own pixels until it is answered
end
```

**Delivery.** The callback runs exactly once, on the event loop, on the next turn of the loop after the recognition finished (the loop turns every 15 ms), while the module is enabled. It is **dropped** — never called — when the module is disabled, reloaded or removed first, the way a one-shot [`host.timer.after`](./timer.md#host-timer-after) is: a callback that clicked or spoke minutes later, over whatever window was in front by then, would be worse than none. Enabled again, the module simply reads again. A callback that raises is reported like any other module callback error.

**`key`, `stale` and `newer`.** A read with a key supersedes the same module's earlier reads with that key that have **not started recognising yet**; those are answered `"stale"`. A read whose recognition had already started is answered with what it read, and `newer = true` if a newer read with its key was asked for meanwhile. So a poll slower than its own period still makes progress — every recognition that starts is delivered — and code that must not speak about a focus that has moved on returns on `newer`. Without a key a read is never superseded. A key costs nothing once its reads are answered: the host remembers which read was the newest with a key only while a read with it is waiting, so a key made from a row number or from text — a new one per read — does not pile up over a session.

**Order and priority.** Answers for different keys may arrive out of the order they were asked in, because a read asked for while the host dispatches a key, a hotkey, a game-controller event (a press, a release, a stick or trigger moving, a pad connecting or disconnecting, or a [combination](./gamepad.md#host-gamepad-on), one fired after its `holdMs` included — but not the `connected` replayed for a pad that was already there, `synthetic = true`), a window coming forward, a focus change or a trigger's report of the window in front is recognised before reads asked from a [`host.timer.every`](./timer.md#host-timer-every) poll or while the module loads. A [`host.timer.after`](./timer.md#host-timer-after) callback, an image search's callback and a read's own callback run with the priority of the dispatch that asked for them, and so does a [`host.screen.snapshotAsync`](./screen.md#host-screen-snapshotasync): its pictures are taken with it on the `screen-capture` thread, and its callback runs with it — so a read asked for from that callback, of the snapshot it was handed or of the screen, goes ahead of the polls' too. A poll's read that has waited half a second goes first all the same — to be photographed and to be recognised — so a stream of key presses or controller events cannot hold it back for good. There is nothing to set: which is which is the host's to infer.

**What raises.** Only mistakes in the call, at the call: `what` not a table; a region the [region form](./screen.md#region-form)'s strict reading refuses — a corner missing (`{}` included) or not a whole number, corners empty or turned around, a mixed or unknown key, a window table without `client`, a fraction that is not a finite number; an entry without `region` or with an unknown field; more than 64 regions, or none; corners that cover more than 40 million pixels in one call together; a name used twice; an option other than `lang`, `key` and `snapshot`; a `lang` that is neither a well-formed tag nor a non-empty list of them; a `key` that is not a non-empty string; a `snapshot` that is not a Snapshot, or was released; `cb` not a function. Nothing that goes wrong routinely raises: it arrives as a reading with a `status`, so code that only looks at `.text` sees `""`. The callback never receives `nil`.

**Limits.** 64 regions and 40 million pixels per call. Corners past the pixel limit raise, as above. Window regions are counted after the corners, in the order given and at the size they resolve to at the call, because that size is the window's, not the module's: one that would take the call past the limit is answered `"failed"`, and the next may still fit, so the same call does not raise at full screen and work in a smaller window. 16 reads per module waiting or running at once: the 17th makes the oldest one without a key fail with `"too many reads waiting — put regions that belong together in one call (up to 64)"`, or fails itself when every waiting read has a key. 64 jobs waiting or running in the whole application, where a job is one picture and its recognition: reads that share one (below) count once, and the 65th distinct one fails at once. When the recogniser has answered no region for 5 seconds — on a Mac its warm-up at start counts as a job (see [macOS](#macos)) — new reads fail at once with `"the text recogniser has not answered a region for N s"` until it answers one — a long job whose regions keep coming back is not that. Two reads asking for the same thing (the same regions, language and way of capturing) before the first is photographed share one picture and one recognition, across modules too. Pictures that are taken and not yet recognised may hold 256 MB in the whole application; above that, the pictures of reads asked from a poll or while a module loads wait to be taken until recognition has caught up, and the others are taken at once.

**Cost.** On the event loop: checking the arguments and queueing, microseconds; building the reading tables when the answer comes. A read of a snapshot costs the same there — its regions are cut out of the snapshot on the recognise thread, not on the event loop. One exception on Windows: in a module that reads through [desktop duplication](./screen.md#which-picture-a-read-sees), the first read after the module is built — and each read after it until duplication has answered once — also makes that module's first-read comparison of the two ways the log reports, on the event loop and not on the capture thread: one duplication read that waits up to 60 ms, and, when that read is answered, one standard capture of about 17 ms (see [Windows](#windows)). Everything else is on the two OCR threads: the capture (on Windows a fixed ~17 ms frame through the standard path, whatever the size, and through desktop duplication what [Which picture a read sees](./screen.md#which-picture-a-read-sees) gives) and the recognition (4–6 ms for a small read-out on Windows, tens to hundreds of milliseconds for a large region; on a Mac see [macOS](#macos), measured per machine). A read that took 100 ms or more from the call to the answer gets one log line naming its regions — `x,y wxh` each, the first four and how many more — and where the time went — waiting for the capture, the capture, waiting for the recogniser, the recognition — at most one per module every 10 seconds.

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

The picture comes from the module's [capture source](./screen.md#which-picture-a-read-sees). The standard way photographs the regions' bounding box once and cuts it up when that box is no larger than four times the regions' own area (or 65 536 pixels, whichever is larger) and no larger than 8 million pixels; otherwise, or when the box comes back clipped at a screen edge, each region is photographed on its own — one capture after another on the capture thread, each a compositor frame of its own, so those regions are not from one moment. Desktop duplication takes every region as a piece of one frame in one request, and the capture thread waits up to 250 ms for its answer; with the default `fallback = "standard"` a request it does not answer is photographed the standard way instead. Under `fallback = "none"`, a picture duplication cannot give is a reading with `status = "failed"` and an `error` beginning `screen capture failed: desktop duplication could not answer`.

In a module that reads through desktop duplication, the read also makes the comparison the log's `[capture]` line reports (see [Which picture a read sees](./screen.md#which-picture-a-read-sees)) — at the call, on the event loop, not on the capture thread: a duplication read, which waits up to about 60 ms, and when it answers a standard capture of the same rectangle (a compositor frame or more) — the first of the call's regions that is not empty at the call (a window region whose window's client area is empty is passed over), or the client area of the window in front when that region is too small to tell the two apart. That happens at the module's first read after it was built, and again at each read until duplication has answered once.

Each region is then recognised exactly as `recognize` recognises it. A region of 400x200 or less is cropped to its content and enlarged; a crop that finds no content is `"blank"`, and `Windows.Media.Ocr` is not asked. For every such small region the second, neural recogniser starts at the same moment as `Windows.Media.Ocr` — before the crop, so for a `"blank"` region too — and its answer is used only when `Windows.Media.Ocr` reads nothing: the lone digit. Every one of them goes to the same recogniser thread, one thread for the whole application that takes them in the order they came, one at a time on the one model — so a region whose answer is needed may wait for one still being recognised, about 15 ms warm. A region whose answer is no longer wanted — `Windows.Media.Ocr` read it, or it was `"blank"` — is cancelled at that moment: skipped when the recogniser has not reached it, stopped before it is handed to the model when it has, and only one already inside the model runs to its end unread. So a call of 64 small regions that `Windows.Media.Ocr` reads costs the neural recogniser only the regions it reached before `Windows.Media.Ocr` answered them. `Windows.Media.Ocr` reads a small region alone — no second recogniser, so a lone digit it rejects comes back empty — while 32 regions already wait for the recogniser (one that has stopped keeping up; said once in the log), when its thread could not be started or has ended (said in the log), and once the application's exit has begun. That recogniser reads without locating, so its text's words get boxes shared out by length inside the content crop it read, marked `approx = true`. It is a Latin-script model and reads with its own character set whatever `lang` says (see [Recognition language](#recognition-language)). A larger region goes to `Windows.Media.Ocr` as captured, with no crop and so never `"blank"`.

On a snapshot, each region's pixels are copied out of the snapshot on the recognise thread and recognised exactly as a live read's are, the small-text treatment and the second recogniser included.

A new `Windows.Media.Ocr` engine is created for every region. The two OCR threads are named `screen-capture` and `ocr-recognise`; `screen-capture` also takes [`host.screen.snapshotAsync`](./screen.md#host-screen-snapshotasync)'s pictures. At exit, a job stops before its next region, the exit waits up to a second for the two threads and then up to half a second for neural recognitions still running, and no neural recognition starts after that; no callback runs by then.

### macOS

The picture is taken at the display's full backing resolution — twice the points on a Retina display — through the same capture as `recognize`: one capture of the bounding box by the same rule as on Windows, and one per region when the box would be wasteful or comes back a different size than asked. Without the Screen Recording permission macOS answers with a picture of the wallpaper rather than an error, so such a read reads the wallpaper — usually `"none"` or `"blank"` — rather than failing; a capture that could not be taken at all is `"failed"`.

On a snapshot, the recogniser reads the picture at the display's own resolution the snapshot kept beside its point-sized pixels, as it reads a live read's shared capture, so the read is as sharp as a live one; a region of the snapshot that was off the desktop is not in that picture and is cut away.

Each region then runs the same pipeline as `recognize`: Vision at the accurate level with language correction off, the content crop and the blank guard for a region of 400x200 points or less, and the retry ladder — the whole region, then enlarged, then the fast model — abandoned after 250 ms. The fast model is asked only for a language it reads. A read asked from a poll skips the enlarged and fast rungs while a read asked from a key press is waiting behind it. Exactly one language is handed to Vision: the one `lang` resolved to.

The recognise thread asks for the user-initiated quality of service. Once it has read the languages, and before its first job, it makes one Vision pass over a line of printed words the executable carries, in the language a read without `lang` is made in — the warm-up below, meant to spare the first read Vision's first pass on this thread (whether a first pass costs that much on every thread is open, see TODO.md). It makes it after the warm-up on a thread of its own has ended, and a read asked in that moment waits for both. The wait is on the hang clock, as a job is: a warm-up that has not ended after 5 seconds has new reads fail at once with `"the text recogniser has not answered a region for N s"` until it ends — a report of macOS 27 has Vision's first request hang for 27 s. The two OCR threads are named `screen-capture` and `ocr-recognise`, as on Windows, and `screen-capture` also takes `snapshotAsync`'s pictures; where the package carries the neural recogniser (below), it reads on a thread of its own, `paddle-ocr`, and loads on `paddle-warm-up`, both at the utility quality of service. At exit, a job stops before its next region and the exit waits up to a second for the two threads; a thread still inside a Vision recognition then is not waited for any longer — the process ends around it — and the log says so. Then, as on Windows, it waits up to half a second for neural recognitions still running, and none starts after that. Last, where the recogniser was loaded, its session and ONNX Runtime's environment are released, and the log says `ocr: released the neural recogniser's session and ONNX Runtime's environment before the process ends (<ms> ms)`: ONNX Runtime 1.22 aborts in the process's own teardown when its environment is still alive, which macOS would report as the application quitting unexpectedly. Should a call still be inside ONNX Runtime after another half second, nothing is released, the log says so, and the application ends without that teardown, with the status it would have had. No callback runs by then.

**What a read costs, measured** — sforzando's read-outs, regions of 40x20 to 123x23 points, through the whole pipeline with the capture, by `recognize` on the event loop, which runs the same pipeline as a read:

- **Mac mini M1**, macOS 14.5, a 1x display: 25–56 ms when another read came within 5 s before it, 115–164 ms after more than 5 s without one. A lone digit about 50 ms — probably a second pass; the slow-read line below says.
- **MacBook Air (2020) with an Intel processor**, macOS 15.8, Retina, VoiceOver running: Polyphony and Pitchbend 143–556 ms (medians about 206 and 209 ms; the two slowest, 363 and 556 ms, with a read of the recognise thread running beside them), the Instrument field 315–786 ms (median 448 ms) — probably two passes, its tight crop reading nothing; the slow-read line below says. The capture alone, through ScreenCaptureKit, 36–91 ms (median 49 ms). The Air has no Neural Engine; a read less its capture puts one accurate pass there at about 150 ms (worked out from those figures, not timed).
- **GitHub's arm64 macOS runners** (virtual Macs on M1 hosts, 3 cores; macOS 14, 15 and 26): the first read of a 200x60 pt region of menu-bar text 202–458 ms.

The first pass costs more. The warm-up over six dark bars that the application made until 2026-10 took 0.2–0.33 s on the M1, 0.3–0.85 s on the runners and 1.7–1.8 s on the Air; the warm-up over words has not been timed on a Mac yet. On the Air, the first read on the event loop and the first on the recognise thread, 79 s to 12 minutes after that warm-up, took another 0.55–2.4 s — a cost per thread, or one after a long pause; the two warm-up lines and the first-recognition lines below tell the two apart. A read with a pass of another thread beside it took 1.2 to 2 times as long on the Air (two readings). On the Windows machine — a six-core i7-8700K desktop, a debug build — the same read-outs took 22–42 ms. What has not been measured is in TODO.md, and `automation-platform ocr-bench` measures a Mac's Vision on fixed pictures (see [Building on macOS](../building-on-macos.md)).

**The warm-up and the cost lines in the log.** At start a thread of its own (`ocr-warm-up`) makes one accurate pass over that line of words, with Vision's default language; then the recognise thread, once it has read the languages and that pass has ended, makes one in the language of its reads. Each says how it went: `ocr: Vision warmed up in <ms> ms on thread ocr-warm-up and read its test line right`; `… and read <n> of its test line's <m> words right: "<read>" where it says "<printed>"` when some differ — a word misread in small print is no alarm; and `… but read NOTHING in its test line …` when Vision read none — a recogniser that answers empty on this Mac, which every read would then do too, said at the start of the session. The recognise thread's line adds how long it waited for the first (`; it waited <ms> ms for the warm-up before it`), and a line ends `; another Vision pass ran beside it` when one did. So the first times Vision's first pass in the process, and the second a first pass on another thread of a process already warm. After that, on every thread — examples; the first has the figures of the Air's first read, the second is made up:

- The first recognition in the process and the first on each thread — one line, the first kind, for a recognition that is both — with how long Vision had been idle in the process and on that thread, or that a pass of another thread was running when it began (`another Vision pass in this process was running when it began`), and `; another Vision pass ran beside it` when one ran at any moment of it: `ocr: the first recognition in this process, a 123x23 pt region on the event loop, took 1538 ms; the last Vision pass in this process ended 703.2 s before it, none ran on this thread`. Then a new slowest on a thread, from 50 ms on and 1.25 times the slowest before it.
- A recognition of 100 ms or more, part by part, at most once per region every 10 seconds on each thread: `ocr: a slow read, a 123x23 pt region on the event loop in 448 ms: capture 49 ms; tight crop 180 ms, 0 words; whole region 210 ms, 1 word, another pass beside it; the rest 9 ms` — each Vision pass with its rung (`as captured`, `tight crop`, `whole region`, `enlarged`, `fast model`, and `fast model, only compared` for the shadow's pass below), its time with making the request, its words or `refused`, `another pass beside it` when another Vision pass ran in the process at any moment of it, and `the neural recogniser beside it` when the neural recogniser did. A read's picture is said as `picture taken apart` — on the capture thread, or a snapshot's pixels; its capture is timed in the `[ocr]` line above — and its 100 ms are then the recognition's alone; a `recognizeMany` region's picture is said as `captured with the call's other regions`.
- The `[env]` block names the processor (`cpu`: its name, cores and threads, the machine's, not the current power mode's) and the revision a text request carries here (`vision`: `a new text request carries revision 3; this macOS lists revisions 1, 2, 3`).

**The neural recogniser beside Vision, only compared.** Every download CI makes carries the second recogniser Windows uses for small regions — PaddleOCR's recognition model under ONNX Runtime, in the bundle (see [Building on macOS](../building-on-macos.md); a package made without it reads with Vision alone, as before) — and it is built for macOS 13.3 and later: on an older macOS the application does not open it. It loads after Vision's warm-up has ended, on `paddle-warm-up` at the utility quality of service; the recognise thread's own warm-up pass begins at that moment too. The log says once whether it did: `ocr: the neural recogniser is ready (ONNX Runtime 1.22.0, session in <ms> ms)`, or `… is not available on this Mac (<reason>); text is read by Vision alone`. In this build **it never answers**. From the moment it is ready, every small region a read climbs the ladder for — a region of 400x200 points or less that the blank guard did not answer, through `read`, `recognize` or `recognizeMany` alike — is handed to it as well, at the moment the read has its content crop, and once the ladder has answered, its reading, if it has one by then, is compared with the answer and counted. On the recognise thread, for a language the fast model reads and a region whose ink is one line no wider than twelve times its height, one pass of the fast model over the same crop is made right after the first accurate pass, and compared the same way; never on the event loop, and not when a read asked from a key press is waiting by the end of that first pass. A read climbs the same rungs as without either and returns what they read — the same `text`, `words`, `lines` and statuses — and the ladder's 250 ms do not count that fast pass. The recognition beside it is load like any other, though: it can take a read past the 250 ms, which then skips the enlarged and fast rungs. Modules see none of it, and declare nothing for it.

What it costs: on the reading thread, copying the crop and handing it over, microseconds. The recognition runs on `paddle-ocr`, one region at a time, at the utility quality of service, below the recognise thread's and the event loop's — whether Vision is slower beside it all the same is what `ocr-bench` measures on a Mac (TODO.md) — and no read waits for it: a recognition that has not answered when the ladder has is cancelled then, and counted as late. On Windows one took 3 to 36 ms; what it takes on a Mac is not measured yet (TODO.md). The fast pass costs the recognise thread 11 to 20 ms over a small field on the CI's Intel Mac and 5 to 12 ms on its arm64 ones (`ocr-bench`, 2026-10-02).

What it says: after every 200th read it counted, and once more at exit, one line with every count — `ocr: the neural recogniser's shadow (Paddle) after 200 small reads: the first accurate pass read nothing in …` — how often the first accurate pass read nothing and the ladder went on to the whole region; what the recogniser read where that pass read text, and where it read none (the recogniser alone reading text there is a lone digit gained, or text invented on ink that is none); where the fast pass was made, how often it, the recogniser and the answer read the same, how often the fast pass and the recogniser read the same and the answer was something else, and how the answer stood where the two differed; how often the recogniser was late, not asked, or could not read the region (a panic or its thread gone, each in the log on its own); the regions that were not its to read; its time; and how many Vision passes it ran beside. With trace logging on, a line for every read in which they disagree: `ocr: shadow, paddle-alone, a 12x20 pt region at 300,200: the answer nothing, the first accurate pass nothing, no fast pass, Paddle "1" (score 0.912, 12 ms)`.

**Saving the images.** With **Save the images OCR was given** on (Application settings), a small region's capture is written beside the application as `ocr-debug-raw-<x>,<y>,<w>x<h>-<hash>.bmp` — the region in points as it was asked for, and eight hexadecimal digits of a hash of its pixels — once for each region and content, at most 8 a region, 64 regions and 32 MiB a session, with a log line for each file and one when a limit is reached. A read in which the recogniser and Vision disagree is written the same way as `ocr-shadow-<case>-<x>,<y>,<w>x<h>-<hash>.bmp`, under limits of its own of the same size — so 64 MiB a session at most between the two — and often holds the same pixels as that read's `ocr-debug-raw` file: the second name is what says the case without trace logging; `<case>` is `paddle-alone` (the ladder read nothing, the recogniser text), `ladder-alone`, `both-differ`, `paddle-differs`, `paddle-nothing`, `fast-paddle-wrong` (the fast pass and the recogniser read the same, and the answer is something else), `split-fast`, `split-paddle`, `split-other` or `split-silent` (the two read differently, and the answer is the fast pass's, the recogniser's, another or nothing), and `fast-none-paddle`, `fast-none-other` or `fast-none-silent` (the fast pass read nothing, the recogniser text). What Vision was handed keeps one name each, overwritten by the next read: `ocr-debug.bmp` (a small region's crop, enlarged and framed, or a larger region as captured), `ocr-debug-retry.bmp` (the whole region) and `ocr-debug-big.bmp` (the crop at twice the enlargement). Each file is written on the thread that read, uncompressed: four bytes a pixel of the capture, so about 45 KB for a 123x23 point region on a Retina display, and up to 1.3 MB for one of 400x200.

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

`lang` is what a read takes: a tag, a list of tags in order of preference, or `nil` for the user's language. Returns the platform's tag it resolves to — `"de"` answers `"de-DE"` where that is installed — or `nil` when nothing here reads it, or when the list is not known yet (see [`languages`](#host-ocr-languages)). Raises when `lang` is not a well-formed tag, a non-empty list of them or `nil`. Costs what `languages` does, plus the matching.

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

It reads the screen at the call and takes no snapshot: a `snapshot` key raises (`host.ocr.recognize: takes no snapshot; host.ocr.read does`), where every other key it does not know is ignored — [`read`](#host-ocr-read) reads a snapshot.

**Signature:** `host.ocr.recognize(opts: { region?: Region, lang?: string }?) -> { text: string, words: { { text: string, x: number, y: number, w: number, h: number } }, skipped: boolean, error?: string }`

`opts` is optional. The `region` is either form of the [region form](./screen.md#region-form), read by the same reader as every screen call's. Corners, `{ x1, y1, x2, y2 }` or `{ x1 = …, y1 = …, x2 = …, y2 = … }`, are read loosely, as this call always has: missing corners default to `x1=0, y1=0` and `x2/y2` = screen width/height, and `x2`/`y2` are exclusive — the capture rectangle is `(x1, y1, x2-x1, y2-y1)`. A window region, `{ window = w, fraction = { x1, y1, x2, y2 } }`, is resolved at the call from the window table's `client`; when that client area is empty (a minimised window) the call answers `{ text = "", words = {}, skipped = false, error = "the window's client area is empty (0x0)" }` instead of reading. Omitting `region` scans the whole primary display. A `region` that is neither form — `{}`, a `{ x, y, w, h }` bounds table, a value that is not a table — raises, naming it (`host.ocr.recognize: opts.region is neither …`). `lang` is a language tag and goes through the same matching as `read`'s (see [Recognition language](#recognition-language)); leaving it out keeps each platform's own default, which is not the same on the two.

**`lang` while the language list is not known yet.** The matching needs the list of languages the recogniser reads, which the recognise thread of [`read`](#host-ocr-read) looks up as the first thing it does after the application starts. A `recognize` with a `lang` waits for that list up to 50 ms, on the event loop. When it is still not known — which is the case in the first moments after start, while the modules load — the tag goes to the engine exactly as written, the way this call always handed it over, and not through the matching: a module that relies on the matching (a bare `"de"`) asks from a timer rather than at its top level, or uses `read`, which matches the language on its recognise thread, after the list is known. A `lang` given as a list of tags — the form only `read` and `resolveLanguage` take — is ignored, as if it were left out.

Synchronous: the capture and the recognition run on the event loop and block speech, hotkey and key callbacks, timers and, on macOS, the event tap until the call returns — the capture a fixed ~17 ms on Windows through the standard path, the recognition 4–6 ms for a small read-out and far more for a large region. On Windows, in a module that reads through [desktop duplication](./screen.md#which-picture-a-read-sees) the capture goes through duplication and may wait for it (at most 60 ms), and until duplication has answered once the call also makes the first-read comparison. The call has no overall time limit; the platform sections say what each recogniser bounds.

The returned table always has:
- `text` — the full recognized string for the region.
- `words` — an array; each entry is `{ text, x, y, w, h }` where `x`/`y` are the word's top-left in **absolute screen coordinates** (the region origin `x1,y1` is added back to the per-word offset), and `w`/`h` are the box size.
- `skipped` — `true` when the **blank guard answered instead of the engine**: the region was small enough to be cropped to its content, the crop found no ink, and the system recogniser was never asked (on Windows the second one has already started by then, and its answer is dropped; see the Windows section below) — so the empty `text` and `words` are the guard's answer, not a reading. `false` whenever the guard did not fire, which includes every other way of getting an empty result; those are different faults and the log names them.

A `lang` that no recogniser here reads is answered, not raised: `{ text = "", words = {}, skipped = false, error = "language: … is not available here (available: …)" }`. So is a window region whose window has an empty client area, with `error = "the window's client area is empty (0x0)"`. A `lang` that is not a well-formed tag raises. On a capture or recognition failure the call raises a Lua error **on Windows**; see the platform sections below, because macOS never raises here and a failure is indistinguishable from an empty region — unless `skipped` says so.

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

`Windows.Media.Ocr` is asked synchronously — the call waits on its `RecognizeAsync`, with no time limit — and a new engine is created for every call, and for every region of a `recognizeMany`. In a module that reads through desktop duplication the capture is a duplication read made on the event loop, which waits up to about 60 ms for it like every read there, and the call makes the comparison described under [`read`](#windows) as well.

For a region of 400x200 or less a second recogniser runs alongside the system one and its answer is used when the system engine returns nothing — which is the lone-digit case, the thing `Windows.Media.Ocr` refuses. In that case the call waits for the second recogniser to finish, with no time limit; when the system engine did answer, the call returns at once and the second recogniser finishes on its own thread, unused. At exit the application waits up to half a second for any that are still running, and logs how long it waited, or that it gave up. That fallback recognises without locating, so when it answers it sets `text` and leaves **`words` empty** here (`read` gives the same text approximate word boxes instead). It ignores `lang` and reads Latin script only (see [Recognition language](#recognition-language)).

The blank guard is that same crop step, so it exists only for a region of 400x200 or less: when the tightened crop finds no content the call returns `{ text = "", words = {}, skipped = true }` without asking the system engine. The second recogniser has already been started by then — it starts before the crop — and finishes on its own thread with its answer unused. A larger region is never checked and always reaches the system engine, so `skipped` is `false` for it whatever it contains.

### macOS

**Nothing in this path raises.** A failed capture and a refused recognition request both log and return `{ text = "", words = {} }`, with no `error`. A `pcall` guard around this call is dead code here, and a broken Screen Recording permission is indistinguishable from a genuinely empty region — the only symptom is a read-out that is permanently blank. The only `error`s are the unavailable `lang` above and a window region whose window has an empty client area (`the window's client area is empty (0x0)`).

No second engine answers here. Where the download carries the neural recogniser it reads every small region beside the ladder, for comparison only (see [`read`'s macOS section](#macos)), and its text is never returned. Small text is carried by a retry ladder — tightened crop, then the whole region, then bigger and faster — abandoned after 250 ms, and `text` and `words` always agree with each other.

All of it runs on the event loop, which on a Mac also carries the keyboard's event tap: the capture and every pass of the ladder hold the keys, the timers and the speech up until the call returns. What that costs per machine is measured under [`read`'s macOS section](#macos) — on an Intel MacBook Air a small read-out held the loop 143–786 ms, and the first one of a session 1.5 s — and the same cost lines are written for it.

A region with **nothing in it is not recognised at all**, on either platform. Two shapes count as nothing: one flat colour, and a filled panel with nothing drawn on it — a value field with no value. The second is the one that matters, because the crop finds the *well*, which used to open the retry ladder and end at the character model reading an empty box. A recogniser asked about a blank rectangle does not answer "nothing"; it answers whatever its network makes of noise, and neither a module nor the person listening can tell that from a reading.

That branch is the one `skipped` reports: `true` means Vision was not asked, and the log carries a line saying so at the same moment. Like Windows it applies only to a region of 400x200 or less — a larger one goes to Vision as captured, with no crop and so no guard. The capture failures above return empty with `skipped = false`, so the two empties this platform produces can be told apart from Lua even though neither raises.

So the same call fails in **opposite shapes**: empty `words` with real `text` on Windows for a lone digit, and empty `text` with real `words` here when the ladder runs out of budget. `read` has neither: its `words` are non-empty exactly when its `text` is.

## host.ocr.recognizeMany(opts) {#host-ocr-recognizemany}

Recognizes several regions in one call, from one capture where the platform can, so that values which have to agree come from the same instant — **on the event loop, which waits until every region is recognised.** [`read`](#host-ocr-read) with a list does the same off the loop, by its own rule for when the regions share a capture, and can read them from one snapshot. The platform sections say when this call cannot use one capture. It takes no snapshot: a `snapshot` key raises (`host.ocr.recognizeMany: takes no snapshot; host.ocr.read does`).

**Signature:** `host.ocr.recognizeMany(opts: { regions: Region[], lang?: string }) -> { { text: string, words: {…}, skipped: boolean, error?: string } }[]`

Returns one entry per region, in the order given, each shaped like `host.ocr.recognize`'s result — word boxes in absolute screen coordinates (each region's own top-left is added to what the engine reported inside it), and `skipped` set per region. A region that could not be read comes back as `{ text = "", words = {}, skipped = false, error = "…" }` rather than as a hole, so `results[2]` is always the second region's answer. Each region is either form of the [region form](./screen.md#region-form), read as `recognize` reads its `region`: corners loosely, with the region form's defaults inside the table, and a window region resolved at the call — one whose window has an empty client area comes back as that region's `error` entry (`the window's client area is empty (0x0)`) while the others are read. A `regions` entry that is neither form raises, naming it (`host.ocr.recognizeMany: opts.regions[2] is neither …`), a missing `regions` raises, and the list ends at the first `nil`, so the regions after a hole are silently not read. One `lang` applies to every region, through the same matching as `recognize`'s; a `lang` no recogniser here reads gives every entry that language `error`, and one that is not a well-formed tag raises. Synchronous, like `recognize`: every region is recognised before the call returns, the capture and all the recognitions on the event loop.

**Why:** recognition is cheap and the capture is not. Measured on the reference machine, a 67×13 read-out recognizes in 4–6 ms while the capture underneath costs a fixed ~17 ms compositor frame whatever its size — so two adjacent read-outs, read one after the other, spend two thirds of their time photographing the screen twice. A watcher polling two boxes at 120 ms measured 44 ms per tick with two calls and 27 ms with one.

The regions are **not** merged into a single wider recognition, and that is the point of the call rather than an oversight: on Windows the neural recogniser starts beside the primary one for every region of up to 400x200 pixels, and its answer is used only for a region the primary one read nothing in. A merged strip is never empty — so a value the primary recogniser dropped would stay dropped while its neighbour came through. Only the capture is shared.

```luau
local r = host.ocr.recognizeMany({ regions = {
  { x1 = 220, y1 = 61, x2 = 287, y2 = 74 },
  { x1 = 300, y1 = 61, x2 = 367, y2 = 74 },
} })
host.log.info(r[1].text .. " / " .. r[2].text)
```

### Windows

As documented: one capture of the bounding box of every region, cropped per region — falling back to one capture each, one after another, when a region is degenerate, when the box would not fit the 32-bit coordinate range (regions two billion pixels apart), or when the capture came back clipped. The box is used whatever its size, unlike `read`'s, so two regions in opposite corners of the screen are one capture of the whole screen. A module that reads through [desktop duplication](./screen.md#which-picture-a-read-sees) gets each region read separately from one frame instead, in one request, because there the cost grows with the area and the bounding box would be the expensive way round.

Under `fallback = "none"`, when duplication has no picture to give, the call does not raise: every entry read from the screen is `{ text = "", words = {}, skipped = false, error = "screen capture failed: desktop duplication could not answer — …" }`, with the reason after the dash. A window region whose window's client area was empty at the call is not read from the screen and keeps its own `error`, `the window's client area is empty (0x0)`, whatever the source. When duplication answers — or when no region of the call can go to it — a degenerate region's entry has `error = "screen capture failed"`, as under the standard source, and a region too large for duplication `"screen capture failed: …"` with the reason; when it cannot answer, every entry read from the screen, those included, carries the "could not answer" error above.

### macOS

The same, with the crop done by drawing the enclosing capture into a smaller context and letting it clip rather than by `CGImageCreateWithImageInRect`, whose rectangle is documented in the image's own coordinate space — a convention this port has no way to test.

Each region then runs the identical pipeline a single `recognize` would, including the retry ladder and the blank guard, so a value read this way is the value that call would have given — and its `skipped` is the one that call would have set.

## Recognition language {#recognition-language}

What `lang` means, and what leaving it out means.

`lang` is a BCP 47 language tag — `"de"`, `"de-DE"`, `"pt-BR"`, `"zh-Hant"` — and `read` and `resolveLanguage` also take a list of them in order of preference. The host matches it against the languages the platform's recogniser reads ([`languages`](#host-ocr-languages)) and hands the engine exactly one of them, spelt as the platform spells it: `"de"` reads with `"de-DE"`, `"de-CH"` with `"de-DE"` when that is the German there is, `"en"` prefers `"en-US"`, `"zh-TW"` finds `"zh-Hant"` and `"zh-CN"` finds `"zh-Hans"`. The first tag of a list that matches anything wins. Case and `_` for `-` do not matter. The same code therefore reads German on both platforms without [`host.os.pick`](./os.md#host-os-pick). `recognize` and `recognizeMany` take one tag and match it the same way once the list is known; in the first moments after the application starts they hand it to the engine as written (see [`recognize`](#host-ocr-recognize)).

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
