---
title: "host.screen — pixels, snapshots, profiles, cells and image search"
sidebar_position: 14
toc_max_heading_level: 2
---

Reads what is on screen: the colour of a point or of several, a per-column and per-row profile of a region, a region reduced to a grid of cells and compared with stored ones, and a search for a template image within one — a PNG, or a template built in memory from bytes or from the screen itself. Each of them reads the screen as it is at the call, or a snapshot of it taken once and read by several of them.

Use it where there is no element to ask. State that exists only as colour — Kontakt reads whether its snapshot bar is open from one pixel on the camera icon, and ON:EAR classifies each of its switches from a single pixel — and anything whose position moves, which is what image search is for, since Kontakt's five instrument-editor sections are found by their captions because opening one pushes the rest down the window.

**Every touch of the screen costs a compositor frame** through the standard path on Windows, measured at about 16.7 ms whether it reads one pixel or a whole window. So `pixel` is not the cheap call it looks like: read a point once per pump and let everything that asks about it share the answer, and read several points with one [`pixels`](#host-screen-pixels). When the question is *where* something is rather than what colour a known point has, `profile` reduces one capture to a value per column and row — the way Melodyne finds a selected note by diffing two column profiles.

**Several questions about one moment** — a menu's selected entry, its page title and the help bubble beside it — are asked of one [`snapshot`](#host-screen-snapshot): take the picture once and pass it to each read as `{ snapshot = s }`. They then read that one picture, touch the screen once between them, and cannot disagree because the screen changed between two captures ([Reading a snapshot](#reading-a-snapshot)). [`snapshotAsync`](#host-screen-snapshotasync) takes that picture off the event loop: at once, at a set time after a press, or once part of the screen has changed — the cursor that moved after a D-pad press, a help bubble that is up for a moment.

The matching, unlike the capture, does grow with the area. A template search across a whole plug-in window is the slow call here — measured at twelve seconds for one full-region match while twelve library overlays polled the same window every 500 ms — which is why anything on a detection poll uses `imageSearchAsync` on its worker thread and re-searches a small box around the last hit before widening.

**Everything here except `imageSearchAsync`, `imageSearchEach`, `matchCellsAsync` and `snapshotAsync` runs on the event loop**, the one thread that also carries speech, hotkey and key callbacks, timers and, on macOS, the event tap. The first three check their arguments and resolve their region on the event loop, then capture and match on the image worker and call back on a later tick; `snapshotAsync` checks and resolves on the event loop and captures on the `screen-capture` thread.

**The `screen-capture` thread is one thread, shared with [`host.ocr.read`](./ocr.md#host-ocr-read)**: it takes the text reads' pictures and `snapshotAsync`'s, one capture at a time, and nothing else. Which goes first: a picture a module about to act waits for (below); then a request asked from a poll or while a module loads that has waited 500 ms; then one asked while the host dispatched a key, a hotkey, a controller event, a window coming forward or a focus change; then the rest — within each, the one due longer, and on a tie the text read. So a picture waits for at most the capture in progress and the ones ahead of it by that order, and a change wait that photographs round after round cannot keep a poll's text read waiting beyond 500 ms. `host.input.*` and [`host.window.focus`](./window.md#host-window-focus) wait up to 50 ms for the calling module's pending pictures on that thread — a text read's, a plain `snapshotAsync`'s, and the first of a change wait without `at` whose baseline that first picture is ([Waiting for the picture to change](#waiting-for-the-picture-to-change)) — so a picture asked for before a click is of the screen before the click.

**The image worker is one thread, shared by every module.** When it becomes free it takes every read waiting for it (or, when none is, the next one to come), plus those queued in the 5 ms after it took the first, as one batch; it captures each distinct region of the batch once (per [source](#which-picture-a-read-sees)) — a read given a [snapshot](#reading-a-snapshot) reads that and captures nothing — matches every read against its frame, and only then takes the next batch. So one slow capture or match holds up the reads of every module queued behind it — a read through desktop duplication waits up to 250 ms there before it gives up (see [Which picture a read sees](#which-picture-a-read-sees)). Callbacks run in the order the reads were queued, except a read held while its module was disabled, which is answered after the module is enabled again. Nothing limits how many reads are in flight: a poll keeps its own count at one by waiting for its callback before it asks again, as the examples below do.

**Coordinates are device pixels on Windows and points on macOS.** The application declares per-monitor DPI awareness (`PerMonitorV2`), so on Windows every coordinate on this page — and on every other page — is one physical pixel of the display; on macOS it is one point. The two numbers agree only at 100 % scaling. A region measured with a tool that is **not** DPI-aware on a Windows display scaled above 100 % is in that tool's scaled units: multiply it by the scale factor (1.5 at 150 %) before using it here. See [`host.screen.size`](#host-screen-size).

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["screen"]
```

See [what that list is and is not](./index.md#capabilities).

## Which picture a read sees {#which-picture-a-read-sees}

Every call on this page, and `host.ocr.read` / `recognize` / `recognizeMany`, reads the screen one way for the whole module, chosen in its manifest rather than per call. Nothing needs declaring for the standard way, which is what every module gets by default. A module written for an application the standard way may not read correctly — a game whose picture it sees frozen or black — can ask for the other one:

```toml
# module.toml of a game module
[capabilities]
require = ["screen", "timer", "speech", "window"]

[screen]
capture = "duplication"   # "standard" (the default) or "duplication"
fallback = "none"         # with "duplication": "standard" (the default) or "none"
```

The code does not change. With the manifest above, this poll reads through desktop duplication, and says the menu state only when it changes:

```luau
local GAME = { title = { contains = "My Game" } }
local lastState = nil
host.timer.every(100, function()
    local win = host.window.active()
    if not win or not host.window.test(GAME, win) then return end   -- only while the game is in front
    -- A point a third of the way across the client area, wherever the window is.
    local c = host.screen.pixel({ window = win, fraction = { 0.32, 0.31 } })
    if c == nil then return end   -- under fallback = "none", or a minimised window: no picture this time
    local state = (c.r > 200 and c.g < 60) and "Start" or "Options"
    if state ~= lastState then
        lastState = state
        host.speech.output(state)
    end
end)
```

**The table.** `capture` and `fallback` are its two keys. Their values are matched without regard to case, so `"Duplication"` is `"duplication"`. A value this host does not know is named in the log and read as the safe choice: `standard` for `capture`, the standard fallback for `fallback`. A key it does not know — a misspelt `captur`, or one a later host adds — is ignored, and the log names it in one line with the module (`[capture] [com.example.game] [screen] has a key 'captur' this host does not know (it knows capture and fallback), so it is ignored; check the spelling`); the module still loads and reads as if the key were not there: with no `capture` of its own, as its runtime declares (see **Inherited from a runtime** below), or else the standard way. A value that is not a string — `capture = true`, `fallback = 0`, a list — fails the manifest, and the module does not load. The format is in [the manifest](../module-package-format.md#screen-and-why-it-is-declared-per-module).

**Inherited from a runtime.** A module that depends on a `code_module` which declares `[screen]` inherits the declaration unless it declares its own. The module's own table counts only when it has a `capture`: a table with nothing but `fallback = "none"` is ignored, with a log line saying so, and the inherited declaration applies with the dependency's fallback — so a game module that wants another fallback than its runtime's writes `capture` too. After the module's own table comes the nearest declaring dependency, then the earliest in manifest order. Only a dependency that is itself a `code_module` is consulted — a plain data dependency's table is never read — and its table counts only if that dependency requires `screen` or `ocr`. The log's `[capture]` lines say which source each module ended up with, and why.

**Opened ahead of time**, on Windows: the host *starts* the opening when one of the module's [`host.window.onTrigger`](./window.md#host-window-ontrigger) triggers fires, before its first matching callback runs (the overlay runtime registers one for every module that attaches an overlay), and before the report of a window already in front that `onTrigger(matcher, { initial = true }, cb)` makes. That opening covers the **primary monitor** only: a window on another monitor has its monitor's duplication opened by its first read. It is not started while duplication is open already, backing off after a failure, waiting for an overdue read or stopped (see Windows below).

**Nothing waits for that opening.** It runs in the background, and the callback runs at once. An event-loop read made while it runs — the callback's own first read included — does not wait for it at all: it is read the standard way, or under `fallback = "none"` fails with `it is still opening`. A read on the image worker ([`imageSearchAsync`](#host-screen-imagesearchasync), [`imageSearchEach`](#host-screen-imagesearcheach), [`matchCellsAsync`](#host-screen-matchcellsasync)) or on the `screen-capture` thread ([`host.ocr.read`](./ocr.md#host-ocr-read), [`snapshotAsync`](#host-screen-snapshotasync)) waits for it, up to 250 ms; an opening that takes longer — seconds, with a cold graphics driver — leaves that read without a picture, and makes duplication overdue until the opening ends (see How long a read waits, below). So a `fallback = "none"` module makes its first detection after its window comes forward with one of those, not with a synchronous `pixel`, `profile`, `cells` or `matchCells`. In the game measurement below, creating the device took 32 ms and the first picture came 4–16 ms after opening: well within a worker read's wait, and longer than the callback takes to make its first event-loop read. The event-loop reads after the opening has finished read through duplication without paying for it.

**Without a head start** — a module that never uses `onTrigger`, or one that reads before any of its triggers has fired — the module's first read starts the opening. Of the event-loop reads made while it is still opening, the first waits for it at most 60 ms and the others not at all; a worker read waits at most 250 ms. A read that gets no picture that way is read the standard way, or fails under `fallback = "none"` (see Windows below).

**With `fallback = "none"`, a read that duplication cannot answer fails, and says why**: `nil` and a reason from `pixel`, `profile`, `imageSearch` and `template{ capture = … }`, `false` and a reason from `save` and `saveMarked`, and the same for every other call in its own shape — each shape and each reason are under [Failure reasons](#failure-reasons). That is for an application where the standard picture is wrong rather than slow: saying a frozen menu item is worse than saying nothing. With the default `fallback = "standard"`, such a read is quietly read the standard way instead, and nothing in the answer says which picture it was: under load, a poll can get standard pictures between live ones (see Windows below).

Poll only while your window is in front. A [`host.timer.every`](./timer.md#host-timer-every) poll runs until it is [cancelled](./timer.md#host-timer-cancel), so gate its callback: return at once unless `host.window.active()` is your window, as the example under [`imageSearchEach`](#host-screen-imagesearcheach) does.

### Windows

`duplication` is DXGI Desktop Duplication: it reads the picture the compositor hands the display, where the standard path reads the screen device context. It is meant for an application whose picture the standard path reads frozen or black. **Measured with such a game** (the standard path 2026-09-21, duplication 2026-09-25; full screen at 1280×1024, on an AMD Radeon integrated graphics chip): the standard path read its always-moving picture as 40 identical pictures out of 40, while desktop duplication gave 40 and 39 different pictures out of 40 in two runs — it read the game live. A whole `profile` call over the 1280×1024 client area (`axes = "columns"`: the capture and its column reduction together) took 6–7 ms through duplication, the slowest 11 ms, where the same call through the standard path had taken about 23 ms; how the 6–7 ms divide between capture and reduction was not measured. Creating the Direct3D device took 32 ms, opening the duplication under a millisecond, and the first picture came 4–16 ms after opening; after 30 s without a read the duplication was closed, and the next read opened it again without trouble. Whether duplication sees another application's frames better than the standard path has to be checked with that application.

**The first-read comparison.** After a module that reads through duplication is loaded or reloaded, its first read is also made both ways, and a `[capture]` line naming the module says whether the two pictures are the same (`the two pictures are the same`, or `the two pictures DIFFER — …` with by how much). It compares the region read when that has at least 4096 pixels (64×64), otherwise the foreground window's client area when that has, otherwise the region anyway. It runs on the event loop, inside the call that reads — also inside `imageSearchAsync`, `imageSearchEach`, `matchCellsAsync`, `snapshotAsync` and `host.ocr.read`, whose own capture happens elsewhere — and costs one duplication read of its own, which waits like any event-loop read (below), plus one standard capture of about 17 ms when that read was answered. When that first read also opens duplication, it is the comparison's read that waits for the opening. If the opening takes longer than the 60 ms an event-loop read may wait, the module's own read right after it finds that wait already spent: under `fallback = "none"` it fails with `this turn of the event loop already waited its share for it`. An opening that answers within the wait — in the game measurement above, 32 ms to create the device and 4–16 ms more for the first picture — leaves the module's read the rest of the event loop's budget (below), and it reads through duplication. Until duplication has answered it once, it is tried again at every read; while duplication never answers, no comparison line appears, and the `could not answer` lines say why.

**Cost.** A read returns the most recently composed frame without waiting for the next one. Nothing captures in the background between reads: each read asks Windows, without waiting, whether a new frame has been composed since the last one, copies it on the graphics card only if there is one, and otherwise reads the copy it already holds — which is then the current screen, since no new frame means nothing was drawn. A frame that carries only a mouse-pointer move does not replace that copy. Only the part of the picture a read asks for is copied off the graphics card. On an idle desktop (the reference machine: GTX 1060, 1920×1080, a test build) it measured about 0.3–1 ms for a region up to 633×418 against the standard path's 16 ms, and 5 ms for the whole screen against 28 ms, with the same bytes from both paths. With a game in front, the game measurement above has only whole calls: a `profile` of the 1280×1024 client area, capture and reduction together, in 6–11 ms. One run on the reference machine with a full-screen Direct3D game in front measured the capture alone at about 33 ms per read at every size. That the latest frame comes back at once also means a read made right after `host.input` may still see the frame from before the input, so re-read after a short wait, as you would anyway.

**How long a read waits.** A read made on the event loop — every call on this page except the three worker calls and `snapshotAsync`, plus `host.ocr.recognize` and `recognizeMany` — waits at most **60 ms** for duplication's answer. All event-loop reads share a budget of **60 ms of waiting in any 300 ms** (of a read that was answered, only the part beyond 5 ms counts); with less than 5 ms of it left, a read does not ask duplication at all. While duplication is opening, only the first event-loop read of that opening waits for it, within the same limits; the others do not wait — and none does when the opening was [started ahead of time](#which-picture-a-read-sees). A read on the image worker, or on the `screen-capture` thread (`host.ocr.read`, `snapshotAsync`), waits at most **250 ms**, and holds every read queued behind it on that thread meanwhile. A read whose whole wait ran out makes duplication **overdue**: from then on no read asks it until that late answer has come back, and the log says so once when that starts and once when it ends. The reads that did not ask — budget spent, still opening, overdue — are counted in the observation line but not logged one by one. So under the default fallback a poll can get standard pictures between live ones without a line in the log; under `fallback = "none"` they fail with `this turn of the event loop already waited its share for it`, `it is still opening` and `an earlier read has not come back yet`.

**When it cannot answer, and when it tries again.** Windows takes duplication away at a display mode change, a switch into or out of full screen (an Alt+Tab to or from a full-screen game can be one) and a desktop switch; stops it while a UAC prompt or the lock screen shows; lets no more than four programs of a session duplicate at once; and refuses it over Remote Desktop and on some laptops with two graphics chips. **The host reopens it by itself; nothing needs switching.** A read that finds the picture lost reopens the duplication at once, within the same read — which is what a full-screen switch needs — and only a loss again straight after counts as a failure. After a failure, duplication is left alone for a while, and the reads in that time get the standard picture, or fail under `fallback = "none"` with the reason:
- the picture lost, or the secure desktop showing: 250 ms, then 500 ms, 1 s and at most 2 s while it keeps failing; the first read that is answered starts the count again;
- four other programs already recording the screen, duplication not supported here, or a pixel format it cannot read: 5 s;
- any other failure of the graphics interface (`it failed with …`): 1 s;
- opened, but no picture arrived within 50 ms: 250 ms, with the duplication kept open;
- the graphics driver reset or updated: no wait; the next read creates the device afresh.

The log names each reason once, and again only after a read has been answered in between. For a laptop with two graphics chips the reason names the remedy: Windows Settings, System, Display, Graphics, this application, Power saving.

**Three hangs stop it.** A read still unanswered after 2 s, or an opening still running after 10 s, counts as a hang. The third hang stops duplication for the rest of the session: every read gets the standard picture, or fails under `fallback = "none"` with `it stopped after three reads hung; …`, until the Application settings switch below is turned off and on again.

**Opening it takes time.** The first read of a session creates a Direct3D device: 32 ms in the game measurement above, about 0.2 s on the reference machine, and 3.5–4.3 s in three runs of a freshly built test binary. Until it is open, reads get the standard picture or fail under `fallback = "none"`; **Opened ahead of time** above says when the host starts the opening before the first read.

**Closed when idle.** A duplication that nothing has read for 30 seconds is closed, and the next read opens it again, paying the opening and up to 50 ms for its first picture. So a gated poll lets it go while the game is in the background, and an ungated one keeps it open all the time. The Direct3D device is kept for 5 minutes without a read, then released too.

- **The mouse pointer:** duplication reports it separately and this path does not draw it in, so, as with the standard path, it is normally not in the picture. Windows documents that a pointer can also be drawn into the desktop image itself, and then it is.
- **Regions touching a rotated monitor** are read the standard way (or fail under `fallback = "none"`).
- **More than 40 million pixels:** a region that large is refused in a module that reads through duplication, with either fallback (`screen capture failed: the region is larger than 40 million pixels`).
- **HDR:** Windows converts the picture to 8 bits per channel for this path, so its colours may differ from the standard path's. Cut templates with `host.screen.save` from a module that declares the same `capture`.
- **"Let modules that ask for it read the screen through the graphics card"** in the Application settings tab turns duplication off for every module: a module that allows the fallback reads the standard way, and every read of a `fallback = "none"` module fails with `it is switched off in Application settings`. Turning it off and on again starts duplication afresh — after three hangs, after its thread stopped, or while it is stuck inside the graphics driver.

### macOS

`[screen]` is accepted and ignored: a module that declares it reads the screen exactly as one that does not, and none of the duplication reasons under [Failure reasons](#failure-reasons) occurs.

## Failure reasons {#failure-reasons}

When a read gets no picture, the call says why. The reason comes as **one value more** than the call has always returned in that case, so code that reads only the first value reads exactly what it always did; a call that looked adds nothing:
- `pixel`, `profile`, `imageSearch`, `template{ capture = … }`: `nil, reason`. An `imageSearch` that looked and found nothing returns `nil` alone.
- `imageSearchMulti`: `nil, nil, reason` — a third value after the two `nil`s that "nothing matched" returns.
- `imageSearchAll`: the empty list and the reason, `{}, reason`. The list is never `nil`; an empty list alone means it looked and found nothing.
- `save`, `saveMarked`: `false, reason`.
- `imageSearchAsync`, `imageSearchEach`: the callback is called with `nil, reason`. One that looked gets its one value as before — `nil` alone is "not found" for `imageSearchAsync`.
- `cells`, `matchCells`: `nil, reason`, and `matchCellsAsync` calls back with `nil, reason`, as they always have.
- `snapshot`, `pixels`, and a snapshot's `crop`: `nil, reason`.
- `snapshotAsync`: the callback is called with `nil, reason, info`.
- `host.ocr.read`: a `"failed"` reading whose `error` is the reason; `host.ocr.recognize` and each entry of `recognizeMany`: `{ text = "", words = {}, skipped = false, error = reason }`, for the failures [their page](./ocr.md#host-ocr-recognize) says they answer rather than raise.

A reason is a sentence for a person — log it, or say it where silence would leave the user guessing — and its wording is fixed: the host's tests hold it to the list below, so a module can also test for one, `string.find(why, "switched off", 1, true)`. `…` stands for a number or a size.

**Any read:**
- `screen capture failed` — the standard path could not read the region: it was empty, too large to allocate for, or the operating system refused the read. On macOS, every failed capture.
- `screen capture failed: the region is larger than 40 million pixels` — a module that reads through desktop duplication asked for more, with either fallback.

**Only under `fallback = "none"`**, on Windows: `screen capture failed: desktop duplication could not answer — ` followed by one of these (see [Which picture a read sees](#which-picture-a-read-sees) for what each means and when it is tried again):
- `it is switched off in Application settings`
- `it stopped after three reads hung; switching it off and on again in Application settings tries again`
- `its thread stopped on an internal error; switching it off and on again in Application settings starts it afresh`
- `its thread could not be started`
- `the application is closing`
- `it is still opening`
- `an earlier read has not come back yet`
- `this turn of the event loop already waited its share for it`
- `Windows does not support it here (…): over Remote Desktop, or on a laptop with two graphics chips where this application runs on the one that does not drive the display — Windows Settings, System, Display, Graphics, this application, Power saving`
- `four other programs already record the screen — a screen recorder, a screen-sharing call, another reader`
- `the secure desktop is showing (a UAC prompt or the lock screen)`
- `the picture was lost to a display mode change, a fullscreen switch or a desktop switch`
- `the graphics driver was reset or updated`
- `the region touches a rotated monitor, which this version reads only the standard way`
- `the region touches a monitor desktop duplication does not report`
- `no picture has arrived since it was opened`
- `no display output was found`
- `the display delivers pixel format …, which this version cannot read`
- `it failed with …` — any other failure, with its error code.

**A window region or point** (every call that takes the [window form](#region-form)):
- `the window's client area is empty (…x…)` — a minimised window, or one not laid out yet: nothing to read this time.
- `the region lies outside the screen's coordinate range` — cannot happen with a window table `host.window` returned.

**A read of a snapshot** (see [Reading a snapshot](#reading-a-snapshot)); nothing was captured for these:
- `the region does not overlap the snapshot` — a call that reads the part of its region inside the snapshot found no part inside it: `profile`, the image searches, `save`, `saveMarked`; and a region of [`host.ocr.read`](./ocr.md#host-ocr-read), as that region's reading's `error`.
- `the region is not inside the snapshot` — a call that needs its region wholly inside the snapshot: `template{ capture = …, snapshot = … }`, the cells calls, and a snapshot's `crop`.
- `the point is not inside the snapshot` — `pixel`, and `pixels` for any one of its points.

**[`snapshotAsync`](#host-screen-snapshotasync)'s own**, given to its callback:
- `a newer request with the same key replaced this one` — the same module asked again with this request's `key`.
- `too many snapshot requests waiting for this module (16); the oldest was ended` and `too many change waits for this module (4); the oldest was ended` — this was the module's oldest when it asked for one more.
- `too many snapshot requests waiting in the whole application (64); this one was not taken` and `too many change waits in the whole application (16); this one was not started` — the new request, when the application's limit is reached.
- `a watch region does not overlap the region` — a change wait whose `watch` region, resolved against a window at the call, misses its region.
- `at the window's current size the change wait watches … pixels, fewer than its minPixels …` — a change wait whose region or `watch`, resolved against a window at the call, leaves it fewer watched pixels than its `minPixels`, so that no picture could count as changed.
- `no picture was taken before the change wait's timeout` — a change wait none of whose rounds came back before it ended; one whose captures failed gives the last capture's reason instead.
- `the screen capture failed inside the application; the log has the details` and `the snapshot failed inside the application; the log has the details` — the host's own code failed; the thread goes on.
- `the request was cancelled` — the capture thread stopped a request the event loop still waited for, which no path of the host does; listed so that no reason is left out.

**One call's own:**
- `the region is empty (…x…)` — `profile`, for corners with no width or no height, which it has always refused before capturing.
- `the capture came back shorter than its own size` — `profile`, `template`, `save`, `saveMarked` and the cells calls: a capture smaller than it said it was.
- `the capture came back …x…, not the region's …x…` — the cells calls.
- `at the window's current size …x… is too large; no side may exceed 4096`, or `at the window's current size …x… is … pixels; the limit is 1048576` — `template{ capture = … }` with a window region past a template's size limit this time; given as corners, the same region raises.
- `at the window's current size the region is …x…, … pixels; the limit is 40000000` — the cells calls, `snapshot` and `snapshotAsync` with a window region past their limit.
- `at the window's current size the region is …x…, … pixels; the limit is 8294400` — a change wait of `snapshotAsync` with a window region past its limit.
- `at the window's current size this module's snapshots would hold … MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()` and `at the window's current size the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()` — `snapshot`, a snapshot's `crop` and `snapshotAsync` with a window region the [budget](#host-screen-snapshot) has no room for at the window's current size; given as corners, the same region raises.
- `the module was disabled while the read waited` — [`matchCellsAsync`](#host-screen-matchcellsasync).
- `internal error: the search could not be made` — an async search whose batch failed inside the host; `internal error: …` for a cells read.

## host.screen.pixel(x, y, opts?) {#host-screen-pixel}

**Signature:** `host.screen.pixel(x: number, y: number, opts: { snapshot: Snapshot? }?) -> (Colour?, string?)`, or `host.screen.pixel(point: { window: Window, fraction: { number, number } }, opts: { snapshot: Snapshot? }?) -> (Colour?, string?)`, where `Colour = { r: number, g: number, b: number, hex: string }`

Reads the colour of one screen pixel: at the screen coordinates `(x, y)`, or at a point given as fractions of a window's client area. Returns the 8-bit channels `r`, `g`, `b` (0–255) plus `hex`, an uppercase `#RRGGBB` string — one value, and nothing after it. When there is nothing to read it returns `nil` and a reason (see [Failure reasons](#failure-reasons)): in a module that declared `fallback = "none"` when desktop duplication had no picture ([Which picture a read sees](#which-picture-a-read-sees)), for the window form when the window's client area is empty, as a minimised window's is (`the window's client area is empty (0x0)`), and with a snapshot when the point is not in it (`the point is not inside the snapshot`). Otherwise it always returns a colour.

**`opts.snapshot`** reads the pixel from a [snapshot](#host-screen-snapshot) instead of the screen: no screen is touched, nothing is counted in the observation line, and it costs microseconds. `opts` is read strictly: `nil`, or a table whose only key is `snapshot`; anything else raises (`host.screen.pixel: opts has a key 'snap'; it takes snapshot`), as does a `snapshot` that is not a Snapshot or was released. The options are read before anything is answered, so a mistake in them raises even when the window form has no pixel this time.

**The window form**, `{ window = w, fraction = { fx, fy } }` (or `fraction = { x = fx, y = fy }`), reads the pixel a [window region](#region-form) starting at `fx, fy` starts with. With `W` and `H` the client area's width and height, the point is `x = client.x + clamp(floor(W * fx), 0, W - 1)` and `y = client.y + clamp(floor(H * fy), 0, H - 1)`, in IEEE doubles as the region form computes. So the point never leaves the client area: a fraction below 0 reads the first column or row, and `1` or more the last. Only `window.client` is read, with no call to the operating system — see the [Region form](#region-form) for what that means for a window table cached for the epoch. The form is read strictly and raises, naming what is wrong (`host.screen.pixel: point.fraction.y is missing; both are needed, x and y`): a key beside `window` and `fraction`, a `fraction` that is not exactly two numbers written one way, a fraction that is not a finite number, a `window` without a `client` of four whole numbers, a table that is not the window form (`{ 100, 200 }` — screen coordinates are two numbers), and a third argument after the table (its options come second). `pixel(x, y)` reads its two numbers as it always has: a fraction is cut toward zero, a numeral string is accepted, and anything else raises, a number outside the 32-bit coordinate range included (`host.screen.pixel: x must be a number within the coordinate range (-2147483648 to 2147483647), got 10000000000`).

**Without a snapshot a call is a screen read**, made on the event loop and counted in the log's observation line. It is not the cheap point lookup of a reader that keeps a frame; how much two nearby calls share differs by platform (see below). Where a poll needs several points, read them with one [`pixels`](#host-screen-pixels), or take one [`snapshot`](#host-screen-snapshot) and read every point from it; for a band of points, one [`profile`](#host-screen-profile) — each one capture, whatever it tests.

```luau
local c = host.screen.pixel(100, 200)
if c == nil then return end   -- only under fallback = "none"
if c.hex == "#FF0000" then
    host.log.info("red pixel")
end
host.log.info(string.format("rgb(%d,%d,%d)", c.r, c.g, c.b))

-- The same question about a window, wherever it is and however large: the centre of its
-- client area's top tenth.
local w = host.window.active()
if w then
    local top, why = host.screen.pixel({ window = w, fraction = { 0.5, 0.05 } })
    host.log.info(top and ("top: " .. top.hex) or ("top not read: " .. why))
end
```

### Windows

**Every call without a snapshot is a screen read of its own**; nothing is kept between calls. Reads the pixel straight from the screen. What you get is what is there — unless the module declares [`[screen] capture = "duplication"`](#which-picture-a-read-sees), in which case it is read from the duplicated picture. A point that lies on no monitor reads as black (0, 0, 0), the same as a region read gives there, whichever way the module reads.

The cost is per call. The standard path is `GetPixel` on the screen, measured at about 16.7 ms each — one compositor frame — so ten points in one poll are ten frames, about 170 ms of the event loop. Under a declared `capture = "duplication"` each call is a 1×1 read of the duplicated picture, measured at 0.30 ms on an idle desktop in a test build; still one read per point. Like every read on the event loop it may first wait for duplication — at most 60 ms, within the event loop's shared budget (see [How long a read waits](#which-picture-a-read-sees)) — and when duplication cannot answer, the call costs that wait plus the standard `GetPixel`, or returns `nil` and the reason under `fallback = "none"`. The window form adds no call to the operating system: its point is worked out from the table given.

### macOS

Without a snapshot, a read captures a tile of up to 256×96 points around the point (starting up to 96 points left of it and 32 above, cut to the display it lies on), at point resolution like every capture on this page, and answers the pixel from it. For the next 5 ms any other `pixel` read inside that tile is answered from the same tile without a new capture; after that, or outside it, the next read captures again. How the backing pixels of a Retina display are reduced to one value per point has not been measured, so compare colours with a tolerance rather than exactly.

## host.screen.pixels(points, opts?) {#host-screen-pixels}

**Signature:** `host.screen.pixels(points: { Point }, opts: { snapshot: Snapshot? }?) -> ({ Colour }?, string?)`, where `Point = { number, number } | { x: number, y: number } | { window: Window, fraction: { number, number } }` and `Colour` is [`pixel`](#host-screen-pixel)'s

Reads the colour at every point of `points` and returns them as one list, in the order asked — one value, and nothing after it. Without a snapshot the points are read from as few captures as they allow (below), so eight probe points close together cost what one costs. With `opts.snapshot` they are read from that [snapshot](#host-screen-snapshot), touching no screen.

**All or nothing.** When one point cannot be read, the call returns `nil` and a reason instead of a list (see [Failure reasons](#failure-reasons)): a capture failed, a window point's window has an empty client area (`the window's client area is empty (0x0)`), or, with a snapshot, a point is not in it (`the point is not inside the snapshot`).

**Read strictly, at the call, and what raises.** `points` is a list — entries 1, 2, 3, … and nothing else — of at most 4096 points. A point is `{ x, y }` or `{ x = …, y = … }` in screen coordinates, whole numbers inside the 32-bit coordinate range (`10.0` is one, `10.5` is not), written one way; or the window form exactly as [`pixel`](#host-screen-pixel) reads it. `opts` is `nil` or a table whose only key is `snapshot`. Anything else raises, naming the point: `host.screen.pixels: points[2].x must be a whole number within the coordinate range (-2147483648 to 2147483647), got 1.5`, `points has 4097 entries; the limit is 4096`, `opts has a key 'region'; it takes snapshot`; so does a `snapshot` that is not a Snapshot or was released. An empty list returns an empty list without reading anything.

**Cost.** On the event loop. Without a snapshot it is counted as one screen read in the observation line. When the points lie in a box of at most 2,000,000 pixels (1,000 × 2,000, say), it takes one capture of that box. Points spread wider are grouped by the tile of 2048 × 976 pixels each lies in — fixed tiles, counted from the screen coordinates 0, 0 — and each group is read with one capture of the box around its own points: two points far apart are two 1×1 captures, and points spread over a whole desktop one capture for each tile they reach, none larger than 2,000,000 pixels (8 MB). Each capture is read and let go before the next is taken. A point on no monitor reads black, as it does in a region read, with nothing captured for it. With a snapshot it costs microseconds.

```luau
-- Which of three menu entries is lit? One capture for all three lamps.
local LAMPS = { { 120, 140 }, { 120, 172 }, { 120, 204 } }
local NAMES = { "Start", "Options", "Quit" }
local lamps, why = host.screen.pixels(LAMPS)
if not lamps then
    host.log.info("menu not read: " .. why)
else
    for i, c in ipairs(lamps) do
        if c.r > 200 and c.b < 90 then host.speech.output(NAMES[i]) break end
    end
end
```

### Windows

A point that lies on no monitor reads black without anything being captured or asked for it, whichever way the module reads. The other points: through the standard path the planned captures are `BitBlt`s of the screen — not `GetPixel`, which `pixel` uses — each about one compositor frame, 16.7 ms, up to about 1028×666 and about 28 ms for a whole 1920×1080 screen. So a list of close points costs one `pixel`; points spread over a whole 1920×1080 primary screen take up to two captures (it reaches two rows of tiles), and over a 3840×2160 one up to six. Under a declared `capture = "duplication"` each of them is a 1×1 piece of one duplication request, which waits like any event-loop read (at most 60 ms, see [How long a read waits](#which-picture-a-read-sees)). When duplication cannot answer, the points are read the standard way, or the call returns `nil` and the reason under `fallback = "none"`. Coordinates are device pixels.

### macOS

The same plan of captures, each at point resolution like every capture on this page, without `pixel`'s tile: every call captures afresh. Colours are the downsampled values of a Retina display, so compare them with a tolerance. Without the Screen Recording permission the colours are those of the desktop wallpaper, not `nil`.

## host.screen.snapshot(opts) {#host-screen-snapshot}

**Signature:** `host.screen.snapshot(opts: { region: Region }) -> (Snapshot?, string?)`, where `Snapshot` is the handle below

Takes one picture of `opts.region` now and keeps it. Every call on this page that is given it as `{ snapshot = s }` — see [Reading a snapshot](#reading-a-snapshot) for which calls and how — reads that picture instead of the screen: several questions about one moment, answered from the same pixels, for one capture. Nothing is ever read from a snapshot a call was not given.

**The region** is required and read strictly, as the cells calls read theirs: corners `{ x1, y1, x2, y2 }` as whole numbers, or a window region `{ window = w, fraction = { … } }` resolved at the call (see [Region form](#region-form)). `opts` takes `region` and nothing else. A region may cover at most 40,000,000 pixels.

**Returns** the handle — one value — or `nil` and a reason (see [Failure reasons](#failure-reasons)) when there is nothing to keep this time: the window of a window region has an empty client area (`the window's client area is empty (0x0)`), is so large that the region covers more than 40,000,000 pixels (`at the window's current size the region is …`) or so large that the module's snapshots would pass their budget (`at the window's current size this module's snapshots would hold …`, below), or the capture failed — under `fallback = "none"`, why desktop duplication had no picture.

**Raises**, at the call: `opts` that is not a table (`host.screen.snapshot: opts must be a table { region = … }, got nothing`), a key other than `region`, a region missing or not exactly the Region form, corners covering more than 40,000,000 pixels (`host.screen.snapshot: the region is 10000x5000, 50000000 pixels; the limit is 40000000`), and a snapshot of corners that would take the module past its budget (below).

**Cost.** One capture on the event loop, through the module's [source](#which-picture-a-read-sees), counted as one screen read in the observation line — what one `profile` of the region costs without the reduction. Every read of the snapshot afterwards touches no screen. In memory it holds `w * h * 4` bytes (more on macOS, below) until it is released or collected.

```luau
-- After a press on the D-pad, ask three questions of the menu as it is 80 ms later: which entry
-- is lit, whether the Options page is showing, and whether a help bubble is up.
-- Needs "screen", "gamepad", "timer", "window" and "speech" in the manifest.
local GAME = { title = { contains = "My Game" } }
local LAMPS = { { 120, 140 }, { 120, 172 }, { 120, 204 } }
local NAMES = { "Start", "Options", "Quit" }

local function readMenu()
    local w = host.window.active()
    if not (w and host.window.test(GAME, w)) then return end
    local snap, why = host.screen.snapshot({ region = { 96, 90, 544, 400 } })
    if not snap then host.log.info("menu not read: " .. why) return end
    local lamps = host.screen.pixels(LAMPS, { snapshot = snap })
    local page = host.screen.imageSearch("images/options-title.png",
        { region = { 96, 90, 544, 118 }, snapshot = snap, tolerance = 8 })
    local bubble = host.screen.profile({ region = { 400, 140, 544, 220 }, axes = "rows", snapshot = snap })
    snap:release()   -- the pixels go now, not whenever the collector gets to them
    if not lamps then return end
    for i, c in ipairs(lamps) do
        if c.r > 200 and c.b < 90 then host.speech.output(NAMES[i]) end
    end
    if page then host.speech.output("Options page") end
    if bubble and bubble.rows.max[1] > 240 then host.speech.output("Help is showing") end
end

host.gamepad.on("down", function(e)
    host.timer.after(math.max(0, 80 - e.age), readMenu)
end, { buttons = { "dpad_up", "dpad_down" } })
```

### The Snapshot handle

A snapshot is a value of its own, like a [Template](#host-screen-template):

- `s.x`, `s.y`, `s.w`, `s.h` — the rectangle captured, in screen coordinates.
- `s.time` — [`host.now()`](./timer.md#host-now) when the capture came back.
- `s.inputEpoch` — [`host.inputEpoch()`](./timer.md#host-inputepoch) as it stood once the picture had been taken: for `snapshot` the value at the call, since nothing is acted on while the event loop captures; for [`snapshotAsync`](#host-screen-snapshotasync) the value when its picture came back on the `screen-capture` thread, which can be well after the call. When `host.inputEpoch()` differs from it, the host has turned it over since the picture was taken. Input the host drives itself (`host.input.*`) turns it over before it acts, so that input came after the picture and the picture predates its effect. A controller press or a window coming forward turns it over when the host delivers or notices it, a little after it happened, so its effect may already be in the picture — compare the event's `time` with `s.time`. When the two are the same, that proves nothing about whether the effect of an earlier action had been drawn yet when the picture was taken.
- `s.via` — which way it was taken: `"gdi"` (the standard Windows path), `"duplication"` (desktop duplication), `"screencapturekit"` or `"coregraphics"` (macOS). Under the default `fallback = "standard"` a module that reads through duplication can get a `"gdi"` picture; this says which one it got.
- `s.released` — `true` once it has been released.
- `s:release()` — lets the pixels go now, and gives their bytes back to the budget. A second call does nothing. The fields above stay readable; handing a released snapshot to a call, or cropping it, raises (`host.screen.pixel: opts.snapshot: the snapshot was released`).
- `s:crop(region) -> (Snapshot?, string?)` — a snapshot of its own of a part of `s`: those pixels copied, charged anew against the budget, with `s`'s `time`, `inputEpoch` and `via`. The region is read strictly like `snapshot`'s and must lie wholly inside `s`; otherwise `nil, "the region is not inside the snapshot"`, and a window region whose window has an empty client area gives `nil` and that reason. It raises for a released `s`, for a region that is not exactly the Region form, and for corners the budget has no room for; a window region the budget has no room for is answered `nil` and the reason, as `snapshot`'s is (below). A region larger than `s` is simply not inside it: there is no size limit of its own. It runs on the event loop and touches no screen: it costs a copy of the part's pixels — on macOS also the matching part of the kept picture at the display's own resolution, drawn anew — and the collector step every charge makes (see the budget, below).
- `tostring(s)` — `Snapshot(640x480 at 96,120, gdi, 1.2 MiB, 35 ms old)`, or `Snapshot(released)`.

**The budget.** One module VM's snapshots may hold **128 MiB** between them, and all modules' together **512 MiB**. A snapshot is charged before its capture — `w * h * 4` bytes, five times that on macOS — and settled to what it really holds once it exists. Over either limit, this module's collector is run twice, and if the snapshot still does not fit the call raises: `host.screen.snapshot: this module's snapshots would hold 144.0 MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()`, or `… the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. …` when it is the second. That is for a region given as corners. **A window region is answered instead**, `nil` and the same sentence after `at the window's current size ` — its size is the window's at run time, as with the 40,000,000-pixel limit — so a poll of a window that was maximised on a large display never puts an error dialog over it; nothing is charged for it. Only the calling module's collector is run, so the application's limit can be reached by other modules' snapshots, which the call cannot free. Every snapshot also steps the collector by about as many bytes as it charges, but at most about what the VM's Luau heap holds: a snapshot as large as the heap or larger costs about one collection of the heap, on the event loop. So a poll that drops its snapshots without releasing them is paced by the collector rather than stopped by the limit — but `release()` is what frees them at once. The budget also bounds one snapshot: in a module that holds no other, at most 33,554,432 pixels on Windows and, charged five times over at the call whatever the display, 6,710,886 points on macOS — fewer than the 40,000,000 a region may cover; a larger one raises for the budget, or as a window region is answered.

**Lifetime.** A snapshot belongs to the module VM that took it. It cannot be passed to another module: a legacy data export and `host.settings` refuse it. Disabling the module keeps its snapshots; reloading it frees them with the VM. A read on the image worker that was given a snapshot keeps its pixels until it is answered, even when the snapshot is released meanwhile; those bytes are not charged. A poll that keeps its last snapshot should release it when its window leaves the front.

### Windows

The capture is `profile`'s: through the standard path the fixed compositor frame (about 16.7 ms up to about 1028×666, 28 ms for the whole 1920×1080 screen), through desktop duplication 0.3–5 ms on an idle desktop, by size, plus any wait for duplication (at most 60 ms, see [How long a read waits](#which-picture-a-read-sees)). Through duplication the snapshot is the most recently composed frame, which may predate the newest redraw of a window. Coordinates and pixels are device pixels; the part of a region that leaves every monitor is black. A snapshot holds `w * h * 4` bytes.

### macOS

The snapshot is taken at the display's own resolution and drawn down to one pixel per point, which is what every read of it reads: the downsampled values of a Retina display, so compare colours with a tolerance. The picture at the display's own resolution is kept beside them, so on a Retina display a snapshot holds up to about five times `w * h * 4` bytes, and is charged that. The part of a region off the desktop is black; a region wholly off it answers `nil, "screen capture failed"`. Without the Screen Recording permission the capture does not fail: the snapshot is a picture of the wallpaper.

## host.screen.snapshotAsync(opts, cb) {#host-screen-snapshotasync}

**Signature:** `host.screen.snapshotAsync(opts: { region: Region, key: string?, at: number?, change: { from: Snapshot?, watch: { Region }?, timeout: number?, tolerance: number?, minPixels: number?, settle: number? }? }, cb: (snap: Snapshot?, why: string?, info: SnapInfo) -> ()) -> nil`, where `SnapInfo = { waited: number, frames: number, changed: boolean?, settled: boolean?, rebased: boolean? }`

Takes the picture [`snapshot`](#host-screen-snapshot) takes, off the event loop, and hands it to `cb` on a later tick: at once; at a set time (`at`, [below](#taken-at-a-set-time)); or once part of the region has changed (`change`, [below](#waiting-for-the-picture-to-change)). The picture is taken on the `screen-capture` thread that also takes [`host.ocr.read`](./ocr.md#host-ocr-read)'s (see the top of this page), through the module's [source](#which-picture-a-read-sees).

**The callback** runs exactly once, on the event loop, on the loop's next turn after the answer (every 15 ms): `cb(snap, nil, info)` with a Snapshot exactly like `snapshot`'s, or `cb(nil, reason, info)` when there is none (see [Failure reasons](#failure-reasons)) — unless the module is **disabled, reloaded or removed** first: then it is dropped, never called, as a [`host.ocr.read`](./ocr.md#host-ocr-read)'s is, and the capture thread stops photographing for it. It runs with the priority of the dispatch that asked (see [`host.ocr.read`](./ocr.md#host-ocr-read), "Order and priority"). A callback that raises is reported like any other module callback error. `info.waited` is the milliseconds from the call until the picture was taken — or until the request ended without one; `info.frames` is how many pictures were taken for it: 1 for a plain or timed one, one per round that came back for a change wait, 0 when none did — and 0 for a request answered because a newer one with its key replaced it, a limit ended or refused it, or the host's own code failed, whatever had been taken for it before. With `change`, `info` also has `changed`, `settled` and `rebased`. At exit, requests still waiting are dropped: no callback runs after the event loop has stopped.

**`key`.** A newer request of the same module VM with the same `key` answers every older one with that key `nil, "a newer request with the same key replaced this one"` on the next tick — also one whose picture has been taken but not delivered yet, and one whose answer is in the same delivery as the callback that asked again. So of a burst of D-pad presses only the last one's answer is read — though a change wait's answer can show an earlier press's move ([Compare with what you last read](#waiting-for-the-picture-to-change)). Keys here are `snapshotAsync`'s own: a `host.ocr.read` with the same key never replaces one, nor the other way round. Without a key a request is never replaced.

**Nothing cancels a request** but a newer one with its key — which is a request of its own, taken in turn — and disabling, reloading or removing the module. A callback whose answer is no longer wanted, because the window left the front meanwhile say, checks that, releases the snapshot and returns.

**Read at the call, and what raises.** `opts` is read strictly: `region` (required), `key`, `at` and `change`, and nothing else. The region is the [Region form](#region-form) read strictly, as `snapshot` reads it; a window region is resolved at the call. Raises, naming what is wrong: `opts` that is not a table (`host.screen.snapshotAsync: opts must be a table { region = …, key?, at?, change? }, got nothing`), another key, a region missing or not exactly the Region form, corners covering more than 40,000,000 pixels — 8,294,400 (one 4K screen) with `change` —, a `key` that is not a non-empty string, an `at` that is not a number or more than 2000 ms after `host.now()`, anything in `change` [out of its range](#waiting-for-the-picture-to-change) (a `minPixels` larger than the number of pixels the wait watches is, when the region and every `watch` region are corners), a `cb` that is not a function, and, for corners, the budget (below). Nothing a window does at run time raises: a window region whose client area is empty (`the window's client area is empty (0x0)`), that is past the limit or that the budget has no room for at the window's current size, a `watch` of a window that misses the region, and a window that leaves the wait fewer watched pixels than its `minPixels`, are answered `nil, reason` on the next tick, without a capture.

**Limits**, answered rather than raised, so that the newest request — the press somebody is waiting for — goes ahead of its module's older ones: a module VM may have 16 requests waiting, of which 4 change waits; one more ends its oldest request (or oldest wait) with `too many snapshot requests waiting for this module (16); the oldest was ended` (`… change waits for this module (4) …`). The whole application may have 64 requests waiting, of which 16 change waits, counted as they stand once the module's own oldest has ended; past that the new request is answered `too many snapshot requests waiting in the whole application (64); this one was not taken` (`too many change waits in the whole application (16); this one was not started`), and a request refused so ends none of its module's. The log says so, once per module every 10 seconds, and likewise for a window region answered for the budget.

**The budget** is `snapshot`'s: the request is charged at the call — `w * h * 4` bytes, five times that on macOS — and a change wait `2 * w * h * 4` more, for the two pictures it only compares with, its baseline and the one a still spell began with, which it keeps beside its newest picture as copies of the watched part, one pixel per point and without macOS's picture at the display's resolution: 3 × `w * h * 4` on Windows, 7 × on macOS. The charge is settled to what the snapshot holds when it arrives. A request answered without a picture gives it back — at once when a newer request with its key replaced it, a limit ended it or the application's limit refused it, so a burst of presses holds the newest request's charge, not one per press. Over the budget a region given as corners raises as `snapshot`'s does, and a window region is answered `nil` and the reason on the next tick; so in a module that holds nothing else a change wait can cover at most 11,184,810 pixels on Windows — more than the 8,294,400 its region may — and 4,793,490 points on macOS, and a plain or timed request 33,554,432 pixels or 6,710,886 points. A `from` snapshot a change wait compares with is kept by the wait until its first round even if it is released meanwhile, uncharged; after that the wait keeps its own copy of the watched part — or, when `from` is exactly the watched part and keeps nothing beside its pixels (on Windows), `from` itself, uncharged, to the end of the wait. Two requests for the same region answered from one capture hold the same pixels and are each charged for them.

**Cost.** On the event loop: checking the arguments, the charge (with its collector step, see the budget under [`snapshot`](#host-screen-snapshot)), and in a module that reads through desktop duplication, until duplication has answered once, the [first-read comparison](#which-picture-a-read-sees): microseconds otherwise. The capture is on the `screen-capture` thread, one at a time with the text reads' captures, in the order the top of this page describes. Requests that are due together and read through the same source share their captures. The most urgent one due goes first, by the order at the top of this page. The standard way it takes one capture, which every other due request shares whose region lies inside it or whose union with it covers at most 2,000,000 pixels — on macOS only regions on the same display —, and the rest are taken next, a capture at a time: a picture a module is about to click past never waits behind another module's large capture. Through desktop duplication every due request goes in one request, each distinct region a piece of it. The answer is cut out of a shared capture on that thread, and a change wait keeps only its own region of it. A request is never answered from a picture taken before it was asked.

```luau
-- A game menu read after each press on the D-pad: wait until the cursor column has changed and
-- stood still for 50 ms, read which entry is lit from that one picture, and read it once more
-- 150 ms later — of two quick presses, the second may not have been drawn yet when the first
-- one's move stood still. While nobody presses, one picture a second keeps `last` current, so
-- the next press is compared with what the menu looked like.
-- Needs "screen", "gamepad", "timer", "window" and "speech" in the manifest.
local GAME = { title = { contains = "My Game" } }
local MENU = { 96, 120, 544, 400 }
local CURSOR_COLUMN = { 100, 120, 140, 400 }
local LAMPS = { { 120, 140 }, { 120, 172 }, { 120, 204 } }
local NAMES = { "Start", "Options", "Quit" }
local last, spoken, since, pressedAt = nil, nil, nil, -math.huge

local function gameInFront()
    local w = host.window.active()
    return w ~= nil and host.window.test(GAME, w)
end

local function keep(snap)
    if last and last.time > snap.time then snap:release() return false end   -- older than what we have
    if last then last:release() end
    last = snap
    return true
end

local function readMenu(snap)
    local lamps = host.screen.pixels(LAMPS, { snapshot = snap })
    if not lamps then return end
    for i, c in ipairs(lamps) do
        if c.r > 200 and c.b < 90 then
            if i ~= spoken then spoken = i host.speech.output(NAMES[i]) end
            return
        end
    end
end

host.timer.every(1000, function()
    if not gameInFront() then
        if last then last:release() last = nil end   -- the game went to the back: let the pixels go
        return
    end
    if host.now() - pressedAt < 1000 then return end              -- a press is being read
    if since and host.now() - since < 2000 then return end        -- one in flight, unless it got lost
    since = host.now()
    host.screen.snapshotAsync({ region = MENU, key = "poll" }, function(snap)
        since = nil
        if snap and keep(snap) then readMenu(snap) end
    end)
end)

host.gamepad.on("down", function(e)
    if not gameInFront() then return end
    pressedAt = host.now()
    host.screen.snapshotAsync({ region = MENU, key = "press",
        change = { from = last, watch = { CURSOR_COLUMN }, timeout = 400, settle = 50 } },
      function(snap, why, info)
        if not snap then return end   -- a newer press replaced this one, or nothing to read
        local again = snap.time + 150
        if keep(snap) then readMenu(snap) end
        -- Once more, a moment later; a newer press replaces this request as it did the wait.
        host.screen.snapshotAsync({ region = MENU, key = "press", at = again }, function(later)
            if later and keep(later) then readMenu(later) end
        end)
    end)
end, { buttons = { "dpad_up", "dpad_down" } })
```

### Taken at a set time {#taken-at-a-set-time}

`at` is a time on [`host.now()`](./timer.md#host-now)'s clock, in milliseconds: the picture is not taken before it. An `at` already past is taken at once; one more than 2000 ms after `host.now()` raises. A controller event's `time` is on the same clock, so `at = e.time + 80` is 80 ms after the press itself, however late the callback runs. A timed request does not hold the module's input (see the top of this page): its picture is meant to come after whatever the module does next. With `change`, `at` is when the wait starts, and its `timeout` counts from then.

Several timed requests for one moment of a help bubble's life — it is up for a moment after a press, and when exactly depends on the game:

```luau
-- Four pictures of the bubble's corner, 30, 80, 150 and 250 ms after the press. The first that
-- shows the bubble is read; the rest, and any older press's, are let go.
-- Needs "screen", "ocr", "gamepad" and "speech" in the manifest.
local BUBBLE = { 400, 140, 544, 220 }
local press = 0
host.gamepad.on("down", function(e)
    press = press + 1
    local mine, read = press, false
    for _, after in ipairs({ 30, 80, 150, 250 }) do
        host.screen.snapshotAsync({ region = BUBBLE, at = e.time + after }, function(snap)
            if not snap then return end
            if mine == press and not read then
                local p = host.screen.profile({ axes = "rows", snapshot = snap })
                if p and p.rows.max[1] > 240 then   -- the bubble's white top edge
                    read = true
                    host.ocr.read(BUBBLE, { snapshot = snap }, function(r)
                        if r.status == "text" and mine == press then host.speech.output(r.text) end
                    end)
                end
            end
            snap:release()   -- the text read keeps its own pixels until it is answered
        end)
    end
end, { buttons = { "dpad_up", "dpad_down" } })
```

### Waiting for the picture to change {#waiting-for-the-picture-to-change}

With `change`, the request photographs its region round after round and compares the **watched** pixels of each picture with a **baseline**: `from`, a Snapshot the module took earlier — or, without one, the wait's own first picture. A pixel has changed when one of its channels differs by more than `tolerance`; the picture has changed when at least `minPixels` watched pixels have. Then:

- **`settle = 0`** (the default): the first picture that has changed is the answer, `changed = true, settled = true` — even when the next one no longer shows the change, and even when it is the first frame of something still being drawn, such as a bubble that fades in.
- **`settle` of some milliseconds**: the first picture that has changed begins a **still spell**, which lasts until a picture comes in which `minPixels` or more watched pixels differ by more than `tolerance` from the one that began it; that picture begins the next. Once a still spell has lasted `settle`, the newest picture is the answer, `changed = true, settled = true`: for a cursor that slides over several frames, the picture shows where it stopped, and for a bubble that fades in, the bubble fully drawn. A change that undoes itself is not a change: when a still spell comes to rest where the baseline was — a flash, a cursor that moved off and back — the wait looks on, so `changed` is never said of a picture that does not differ from the baseline. So a bubble is the answer once it has stood still for `settle`, and one that is gone again before that counts as a flash. For a help bubble that is up for a few hundred milliseconds, a `settle` of one or two of the game's frames (30–50 ms) answers it fully drawn; `settle = 0` answers the first picture that differs at all.
- **At the `timeout`**, counted from the call (or from `at`), the newest picture is the answer: `changed` is `true` when a change was seen, was still settling and the newest picture still differs from the baseline; `settled` is `false`. When no round came back at all, `nil` and the last capture's reason, or `no picture was taken before the change wait's timeout`.

The options, read at the call — each out of its range raises, naming it (`host.screen.snapshotAsync: opts.change.settle must be a whole number from 0 to 500 (the timeout), got 600`):

| Option | Default | Meaning |
|---|---|---|
| `from` | the wait's first picture | A Snapshot to compare with; one that was released raises. |
| `watch` | the whole region | 1 to 16 regions in the [Region form](#region-form), read strictly — `{ { x1, y1, x2, y2 }, … }`, a list even of one; a single region raises and says to put it in braces. Each is cut to `region`, and a pixel two of them share counts once. One that misses `region` raises when both are corners; when a window decides it, the callback gets `nil, "a watch region does not overlap the region"`. |
| `timeout` | 500 | Whole milliseconds, 1 to 2000. |
| `tolerance` | 16 | 0 to 255 per channel. |
| `minPixels` | 4 | 1 to 8,294,400, and at most the number of pixels the wait watches (each `watch` region cut to `region`, a pixel two of them share once — or the whole region): more raises when the region and every `watch` region are corners, since no picture could count as changed; when a window decides it, the callback gets `nil, "at the window's current size the change wait watches … pixels, fewer than its minPixels …"`. So a `watch` of one pixel needs `minPixels = 1`. |
| `settle` | 0 | Whole milliseconds, 0 to `timeout`. |

**What counts.** Only the watched pixels: keep a blinking cursor, an animated background or a clock out of `watch`, or the wait ends on them. The mouse pointer is normally not in the picture (see [Which picture a read sees](#which-picture-a-read-sees)); a pointer the game draws itself is, and counts as a change unless it is outside `watch`.

**Compare with what you last read.** Without `from`, the baseline is the wait's first picture, taken when the thread gets to it: if the game had already redrawn by then, the change is in the baseline and the wait sees nothing until its timeout. Pass `from = last` — the snapshot the module last read — and a change that happened before the first round is seen at the first round. With a burst of presses, `last` can predate an earlier press's move as well: the wait of the second press sees the first one's move at its first round. With `settle` it answers only once the moves have stood still, which the second press's move, drawn a frame or two later, resets; a game slower to draw it than `settle` is caught by reading once more a moment after the answer, as the example above does. Without `at`, a wait whose baseline is its first picture — it has no `from`, or one it cannot compare with: one that does not hold every watched region, or, on Windows, one taken the other way than the module reads (a `"gdi"` snapshot in a module that reads through desktop duplication) — holds the module's input like a plain request (see the top of this page) until that picture is taken, so a baseline asked for before a click is of the screen before it. A wait with a `from` it can compare with holds nothing: its baseline was taken already.

**`rebased`** is `true` when the baseline was replaced instead of compared: a picture from another way of capturing than the wait's first — desktop duplication falling back to the standard way under `fallback = "standard"`, or back — becomes the new baseline rather than counting as a change, since the two ways do not give byte-identical pictures; and a `from` taken the other way, or that does not hold every watched region, is replaced by the wait's first picture. A rebased wait can miss a change that happened before its new baseline.

**How old a picture is.** `snap.time` is when it was taken, and `snap.inputEpoch` the value of [`host.inputEpoch()`](./timer.md#host-inputepoch) once it had been taken — not at the call: a picture taken at `at`, or at a wait's last round, carries the value of that moment. When `host.inputEpoch()` has moved on since, the host has acted, or delivered a press or noticed a window change, after the picture was taken: input the host drove itself comes after the picture, while a press or a window change may have happened, and been drawn, a little before the host turned the value over — compare the event's `time` with `snap.time`. When it has not moved on, that proves nothing about whether the effect of an earlier input had been drawn — that is what a change wait is for.

**Cost.** One capture per round, and a comparison of the watched pixels that stops at the `minPixels`th difference, on the `screen-capture` thread. Rounds of one wait start no closer together than 8 ms through the standard path on Windows, 16 ms through desktop duplication and 33 ms on macOS, so a wait of 2 seconds through the standard path on Windows is at most 251 rounds, one at its start and one at its deadline among them; waits due together share their rounds' captures as other requests do.

A help bubble that is up for a moment after a press, read from the first picture that shows it standing still. Without `from`, a bubble that is already up when the wait takes its first picture is in the baseline, and the first change is then the bubble going away — so the answer is checked for the bubble before it is read (or pass the snapshot the module last read as `from`):

```luau
-- settle = 40: once the bubble has stood still for 40 ms — fully drawn, if it fades in — the
-- newest picture is the answer, even if the bubble is gone by the time the callback runs. Its
-- text is read from that picture, not from the screen, and only when the picture shows the
-- bubble.
-- Needs "screen", "ocr", "gamepad" and "speech" in the manifest.
local BUBBLE = { 400, 140, 544, 220 }
host.gamepad.on("down", function(e)
    host.screen.snapshotAsync({ region = BUBBLE, key = "bubble",
        change = { timeout = 600, settle = 40 } },
      function(snap, why, info)
        if not snap then return end
        local p = info.changed and host.screen.profile({ axes = "rows", snapshot = snap })
        if p and p.rows.max[1] > 240 then   -- the bubble's white top edge
            host.ocr.read(BUBBLE, { snapshot = snap, key = "bubble" }, function(r)
                if r.status == "text" and not r.newer then host.speech.output(r.text) end
            end)
        end
        snap:release()   -- the text read keeps its own pixels until it is answered
    end)
end, { buttons = { "dpad_up", "dpad_down", "dpad_left", "dpad_right" } })
```

### Windows

Each picture is taken as `snapshot`'s is, on the `screen-capture` thread: through the standard path the fixed compositor frame (about 16.7 ms up to about 1028×666, 28 ms for the whole 1920×1080 screen); through desktop duplication the most recently composed frame, 0.3–5 ms on an idle desktop, the thread waiting up to 250 ms for duplication as the image worker does (see [How long a read waits](#which-picture-a-read-sees)) and answering `nil` and the reason under `fallback = "none"` when it cannot. A change wait through the standard path therefore captures back to back, one compositor frame a round, for up to its timeout. Between rounds, and until an `at`, the thread sleeps, and Windows wakes it on its timer: a round or a timed picture can come up to one timer tick late (15.6 ms at Windows' default timer resolution). Through desktop duplication a round that finds no newly composed frame compares the frame it already holds. When duplication cannot answer a round and the module allows the fallback, the round's regions are read the standard way, planned as a standard round is — a region inside another's capture cut from it, unions of up to 2,000,000 pixels — one compositor frame per planned capture. Coordinates are device pixels; the part of a region that leaves every monitor is black.

### macOS

Each picture is taken as `snapshot`'s is, at the display's own resolution and kept beside its point-sized pixels, on the `screen-capture` thread. A round made only of change waits does not feed the log's warning about a region that keeps coming back one flat colour, which `snapshot`'s captures and every other round do. There is no desktop duplication. Regions whose centres lie on two displays never share a capture, since two displays can differ in scale. A picture from ScreenCaptureKit and one from the older Core Graphics functions are two ways of capturing, as the two Windows paths are: a wait whose rounds switch between them mid-wait takes the new picture as its baseline, `rebased = true`. Which way a round will be taken is not known at the call, so a `from` taken the other way is found out only at the first round; such a wait does not hold the module's input, as it would on Windows. A change wait is charged 28 bytes a point at the call, so one over 4,793,490 points (2560 × 1873, say) raises for the budget even in a module that holds nothing else — or, as a window region, is answered — below the 8,294,400 its region may cover; and a plain or timed request, 20 bytes a point, one over 6,710,886 points. Without the Screen Recording permission every picture is of the wallpaper: a plain request answers one, and a change wait ends at its timeout with `changed = false`.

## Reading a snapshot {#reading-a-snapshot}

A call given `{ snapshot = s }` reads the picture `s` holds instead of the screen. It captures nothing, touches no screen, is not counted in the observation line, never asks desktop duplication, and never makes the [first-read comparison](#which-picture-a-read-sees). What the call does with the picture — the matching, the reduction, the PNG — costs what it always costs, where it always runs.

| Call | Given as | Rule | When the region or point misses |
|---|---|---|---|
| [`pixel`](#host-screen-pixel) | `pixel(x, y, { snapshot = s })`, `pixel(point, { snapshot = s })` | inside | `nil, "the point is not inside the snapshot"` |
| [`pixels`](#host-screen-pixels) | `opts.snapshot` | inside, every point | `nil, "the point is not inside the snapshot"` |
| [`profile`](#host-screen-profile) | `opts.snapshot` | cut; `x, y, w, h` are the part read | `nil, "the region does not overlap the snapshot"` |
| [`imageSearch`](#host-screen-imagesearch), [`imageSearchMulti`](#host-screen-imagesearchmulti), [`imageSearchAll`](#host-screen-imagesearchall) | `opts.snapshot` | cut | `nil, reason`; `nil, nil, reason`; `{}, reason` |
| [`imageSearchAsync`](#host-screen-imagesearchasync), [`imageSearchEach`](#host-screen-imagesearcheach) | `opts.snapshot` | cut; each `within` is cut to the part read | `cb(nil, reason)` on a later tick |
| [`template`](#host-screen-template) | `{ capture = region, snapshot = s }` | inside | `nil, "the region is not inside the snapshot"` |
| [`cells`](#host-screen-cells), [`matchCells`](#host-screen-matchcells), [`matchCellsAsync`](#host-screen-matchcellsasync) | `opts.snapshot` | inside | `nil, reason`; `cb(nil, reason)` on a later tick |
| [`save`](#host-screen-save), [`saveMarked`](#host-screen-savemarked) | `opts.snapshot` | cut; the PNG is the part read, and marks stay screen coordinates | `false, "the region does not overlap the snapshot"` |
| `s:crop(region)` | — | inside | `nil, "the region is not inside the snapshot"` |
| [`host.ocr.read`](./ocr.md#host-ocr-read) | `opts.snapshot` | cut, to the part the recogniser can read; the reading's `x, y, w, h` are the part read | that region's reading is `"failed"`, `error = "the region does not overlap the snapshot"`; the call's other regions are read |
| [`snapshotAsync`](#host-screen-snapshotasync) | `change.from` | the baseline of a change wait | a `from` that does not hold every watched region is replaced by the wait's first picture (`rebased`) |

**Cut** reads the part of the region that lies inside the snapshot, and answers the reason when no part does. **Inside** needs the whole region or point inside the snapshot and answers the reason otherwise: a template or a grid of cells cut to fit would be a different template or grid.

**Defaults.** A call that reads its region loosely — `profile`, the image searches and a `within`, `save`, `saveMarked`, `template`'s `capture` — takes a region left out as the whole snapshot, and a corner left out (or not a number) as the snapshot's edge, where without a snapshot they are the primary screen and its edges. The strictly read regions (the cells calls, `crop`) have no default, as without one.

**Window regions and points** are resolved against the window table they are given, as always, with no call to the operating system, and the rule applies to the rectangle that gives. To read the part of a window a snapshot holds, pass the window table the snapshot was taken with: a window that has moved since is read where that table says it was.

**What raises** is what raises without a snapshot — a mistake in a region raises with a snapshot as without one: the region is read in full before the snapshot's rule is applied to it, and it raises whether or not the snapshot holds it — plus a `snapshot` that is not a Snapshot (`host.screen.profile: opts.snapshot must be a Snapshot, got a userdata of another kind`, for a Template) and one that was released. `template` takes `snapshot` only beside `capture` and raises otherwise (`host.screen.template: snapshot belongs with capture: …`).

**On the image worker**, a read given a snapshot is answered from it in its batch without a capture and shares no capture with any other read. If its module is disabled before the answer comes, a search is held and made again on the same snapshot when the module is enabled — the same picture, so the same answer — and a `matchCellsAsync` is answered `nil, "the module was disabled while the read waited"`, as without one.

**Text recognition** reads a snapshot through [`host.ocr.read`](./ocr.md#host-ocr-read) `{ snapshot = s }`: nothing is photographed, and the recogniser reads the snapshot's pixels — on macOS the picture at the display's own resolution it kept, so the read is as sharp as a live one. [`recognize`](./ocr.md#host-ocr-recognize) and `recognizeMany` take no snapshot and raise for a `snapshot` key (`host.ocr.recognize: takes no snapshot; host.ocr.read does`).

```luau
-- The page a menu is on and where its cursor is, from one picture: they cannot disagree.
-- pages = { predicate = "…", states = { { name = "Items", cells = "…" }, … } }, recorded with cells.
local pages = host.json.decode(host.resource.read("data/pages.json"))
local w = host.window.active()
local snap = w and host.screen.snapshot({ region = { window = w, fraction = { 0, 0, 1, 1 } } })
if snap then
    local page = host.screen.matchCells({
        region = { window = w, fraction = { 0.02, 0.10, 0.30, 0.16 } },
        cols = 8, rows = 2, predicate = pages.predicate, snapshot = snap,
    }, pages.states)
    local cursor = host.screen.imageSearch("images/cursor.png",
        { region = { window = w, fraction = { 0, 0.2, 0.1, 1 } }, snapshot = snap, tolerance = 8 })
    snap:release()
    if page and cursor then host.log.info(page.name .. " page, cursor at y " .. cursor.y) end
end
```

### Windows

Reading a snapshot is the same on every platform: its pixels are those the capture delivered, in device pixels here.

### macOS

Reading a snapshot is the same on every platform: its pixels are one per point, the downsampled values of a Retina display.

## host.screen.size() {#host-screen-size}

**Signature:** `host.screen.size() -> { w: number, h: number }`

Returns the primary screen dimensions in pixels as `{ w, h }`.

```luau
local s = host.screen.size()
host.log.info("screen is " .. s.w .. "x" .. s.h)
```

### Windows

**Device pixels.** The application declares per-monitor DPI awareness, so a 4K panel at 200% scaling reports 3840x2160 and every coordinate the platform takes or returns is one physical pixel. A program that is not DPI-aware — a Python script that never called `SetProcessDpiAwareness`, for one — is told the scaled size instead (1920x1080 for the same panel), so coordinates it measured are that factor too small here unless the display was at 100%.

### macOS

**Points.** The same panel reports 1920x1080 on a Retina Mac; a capture is downsampled so that one pixel of a captured image is one point, and no coordinate can address a single backing pixel.

This is the difference most likely to waste a day. Coordinates and template images calibrated on a HiDPI Windows machine are exactly **twice** the numbers a Retina Mac needs, and nothing reports it — the click simply lands half a screen away.

## host.screen.profile(opts?) {#host-screen-profile}

**Signature:** `host.screen.profile(opts: { region: Region?, axes: ("both" | "columns" | "rows")?, dark: number?, snapshot: Snapshot? }?) -> (Profile?, string?)` where `Profile = { x, y, w, h, columns: Axis?, rows: Axis? }` and `Axis = { min: number[], max: number[], dark: number[], mean: number[], r: number[], g: number[], b: number[] }`

Takes **one** capture of `opts.region` and reduces each of its columns and rows to a few statistics — the darkest pixel, the lightest, how many pixels are darker than `opts.dark`, the mean, and the mean of each colour channel. The region is corners or a window region (see [Region form](#region-form)); **omitted, it profiles the whole primary screen**. With `opts.snapshot` it profiles the part of the region inside that [snapshot](#reading-a-snapshot) instead, capturing nothing: omitted, the region is then the whole snapshot, `x`, `y`, `w`, `h` are the part profiled, a region with no part inside it answers `nil, "the region does not overlap the snapshot"`, and the call is not counted in the observation line. Luminance is ITU-R BT.601. `min` and `max` are whole numbers 0–255 because they are particular pixels; the means are **fractional** — a mean over hundreds of rows moves by less than one unit when something note-sized changes inside it, and rounding would floor exactly the signal you are looking for.

`dark` is a luminance threshold, 0–255, default `128`; each axis's `dark` sequence counts the pixels of that column (or row) whose luminance is below it. For finding a **shape** it is the statistic that works where the other two do not: a mean over hundreds of rows dilutes a note-sized object to a couple of units, and a minimum saturates because a busy column already contains something black. A count is proportional to how much of the column the object covers — Melodyne finds its notes with `dark = 190`. A value outside 0–255 silently falls back to `128`, and a fraction is cut to the whole number below it.

Index `1` is the left (or top) edge of the region, so a column index maps back as `x + i - 1`. `x`, `y`, `w`, `h` are the **requested** rectangle — for a window region, the screen rectangle it resolved to: nothing is clipped to the desktop, and a region that runs off the screen comes back with black in the part that is not there. The profile is one value, with nothing after it. Returns `nil` and a reason (see [Failure reasons](#failure-reasons)) when the region is empty (`the region is empty (0x20)`), when a window region's window has an empty client area, when the capture fails, or when the capture came back shorter than its own dimensions. Raises for an `opts.region` that is neither region form, and for an `axes` that is not exactly `"both"`, `"columns"` or `"rows"` — anything else is an error rather than a silent `"both"`.

Use it to find something whose **position** you do not know: a vertical line is a dark column in a light band, an edge is where the mean steps, a shape's extent is where its run of changed columns begins and ends, and a differently coloured patch shows up in `r`/`g`/`b` while the luminance stays flat. For a feature much smaller than the region, prefer `min`/`max` over `mean`: a two-pixel dark line moves a column's minimum by everything and its mean by a little.

**Why this rather than many `pixel` calls:** through the standard path on Windows a screen capture costs a fixed compositor frame — measured on the reference machine at 16.6 ms for 55×27 and 16.7 ms for 1028×666 — and `host.screen.pixel` costs the same 16.7 ms *for one pixel*. Four point probes are four frames; a profile of the whole window is one. The **capture** is size-independent, but this call's reduction is not: it walks every pixel and builds seven sequences for each axis asked for, on the event loop, so profile the band you need rather than the screen. `axes` skips the half you do not want.

```luau
-- Where are the dark vertical lines in this strip?
local x, y = 100, 200
local p = host.screen.profile({ region = { x, y, x + 400, y + 20 }, axes = "columns" })
if p then
    for i, m in ipairs(p.columns.min) do
        if m < 160 then
            host.log.info("line at screen x " .. (p.x + i - 1))
        end
    end
end

-- The same strip of a window, as fractions of its client area: follows the window.
local w = host.window.active()
if w then
    local q, why = host.screen.profile({ region = { window = w, fraction = { 0, 0.9, 1, 1 } }, axes = "columns" })
    if not q then host.log.info("not profiled: " .. why) end
end
```

### Windows

The capture goes through the module's [source](#which-picture-a-read-sees), on the event loop. Through the standard path it is the fixed compositor frame above; through desktop duplication the capture measured 0.3–5 ms on an idle desktop, by size. The reduction comes on top, in proportion to the area. With a game in front, a whole call over a 1280×1024 client area with `axes = "columns"` — capture and reduction together — took 6–11 ms through duplication, where the same call through the standard path took about 23 ms. Add any wait for duplication (at most 60 ms, see [How long a read waits](#which-picture-a-read-sees)). Coordinates and the profile are in device pixels.

### macOS

The capture is in points (see [`host.screen.size`](#host-screen-size)), paid on the event loop; the image worker's capture measured about 19 ms per batch in the fifth macOS session. Without the Screen Recording permission the capture does not fail: the profile is of the desktop wallpaper, not `nil`.

## host.screen.template(spec) {#host-screen-template}

**Signature:** `host.screen.template(spec: TemplateSpec) -> (Template?, string?)` where `TemplateSpec = { name: string?, rgba: (string | buffer)?, rgb: (string | buffer)?, w: number?, h: number?, capture: Region?, file: string?, snapshot: Snapshot? }` — `name` plus **exactly one** of `rgba`, `rgb`, `capture` or `file`, and `snapshot` only beside `capture` — and `Template = { w: number, h: number, name: string?, count: number }`, read-only.

Builds a template in memory, for every search below to take wherever it takes a path. A template no longer has to be a PNG file in the module's folder: its pixels can come from bytes the module computed or read from a data file, or from the screen itself, now.

- **`rgba`** with `w` and `h`: `w * h * 4` bytes, row-major from the top-left, in R, G, B, A order. As in a PNG, **alpha 0 is a wildcard** and any other alpha is compared in full.
- **`rgb`** with `w` and `h`: `w * h * 3` bytes; every pixel is compared.
- **`capture`**: the given [region](#region-form) of the screen — corners, or a window region resolved at the call — captured now. Every pixel is compared (alpha is forced opaque). Returns **`nil` and a reason** (see [Failure reasons](#failure-reasons)) when the capture fails, and when a window region's window has nothing to capture this time: an empty client area, or one so large that the region is past the size limit below. Those are the runtime conditions rather than mistakes, so the ones that do not raise; a `capture` table that is neither region form raises.
- **`capture` with `snapshot = s`**: the region is cut out of that [snapshot](#reading-a-snapshot) instead, and nothing is captured. It must lie wholly inside the snapshot — a template cut from part of the region would be a different template — or the call returns `nil, "the region is not inside the snapshot"`; a corner left out is the snapshot's edge. `snapshot` beside anything but `capture` raises (`host.screen.template: snapshot belongs with capture: …`), as does one that is not a Snapshot or was released.
- **`file`**: a PNG — the one image format the host is built with; any other file raises `cannot open template` — resolved exactly as a path given to a search is (see [the path rule](./index.md#paths)), and sharing its cached decode. The handle is a **snapshot**: it keeps that decode for as long as it lives, however many other paths pass through the cache, and a re-captured PNG is picked up live only by a search that is given the path.

Bytes are a Luau `string` or a `buffer`, copied once. A handle built from bytes may be at most 4096 pixels on a side and 1,048,576 pixels in all (1024x1024); a `capture` given as corners raises before anything is captured when its region is larger than that, or empty, while a window region past the limit is answered `nil` and a reason, since the window's size decides it. `name` (at most 64 characters) comes back in every hit the template makes and in `tostring(t)`; a `file` template given none is named by its path as written. `count` is the number of pixels a search compares. Everything else raises, naming the field: a wrong byte count (`rgba is 1436 bytes; 10x36 RGBA needs 1440`), two sources or none, `w`/`h` given to a `capture` or a `file`, and any field this version does not know, so a misspelt `rbga` or a `tolerance` it does not take is never silently ignored. Tolerance and scales are the search's options, as for a path.

A handle is the one value returned, with nothing after it. A template belongs to the module VM that built it, like any Luau value. It cannot be passed through a legacy data export or stored in `host.settings`. One VM's handles built from bytes or a capture may hold **32 MiB** between them; over that, the collector is run and, if they still do not fit, the constructor raises. Every such handle is held and charged as RGBA — 4 bytes a pixel, an `rgb` one included — plus a fixed amount per handle (the structure and 48 bytes of match order), so an RGB pack costs about four thirds of its own size. The budget is per VM: a `code_module` runtime that builds a pack while it is evaluated inside each game module's VM pays for it once per VM. `file` handles cost nothing against it. Build templates once, not on every poll: one from bytes or a file when the module loads, a `capture` once what it learns is on screen (see Windows below for a module that reads through desktop duplication).

```luau
-- A pack a converter wrote as JSON: each signature a name, a size and its RGB bytes as
-- numbers. Decoded once, when the module loads; each template keeps its name for its hits.
local pack = host.json.decode(host.resource.read("data/gamepack.json"))
local T = {}
for _, sig in ipairs(pack.signatures) do
  local bytes = buffer.create(#sig.rgb)
  for i, v in ipairs(sig.rgb) do buffer.writeu8(bytes, i - 1, v) end
  T[sig.name] = host.screen.template({ name = sig.name, rgb = bytes, w = sig.w, h = sig.h })
end
```

```luau
-- Learn the selection marker once, in this machine's own pixels, then follow it. A capture
-- can come back nil, so a nil is not kept: the next call asks the screen again.
local marker = nil
local function learnedMarker()
  if marker == nil then
    local why
    marker, why = host.screen.template({ name = "marker", capture = { 120, 80, 132, 92 } })
    if marker then
      host.log.info("learned " .. tostring(marker))   -- Template(marker 12x12, 144 compared)
    else
      host.log.info("marker not learned yet: " .. why)
    end
  end
  return marker
end

local m = learnedMarker()
local hit = m and host.screen.imageSearch(m, { tolerance = 8 })
```

### Windows

Sizes and coordinates are device pixels (see [`host.screen.size`](#host-screen-size)). A `capture` is one screen capture on the event loop — through the standard path measured at a fixed ~16.7 ms whatever the region's size — so learn a captured template once, not on a poll; it is counted in the observation log with `pixel` and `profile`. Without a `snapshot` it is always a live read of the screen, never an earlier frame; with one it is that snapshot's picture, and costs a copy of the region's pixels. It reads through the module's [source](#which-picture-a-read-sees) like every other read, and in a module that declares `capture = "duplication"` that alone does not make it the picture the module's searches will see: a `capture` made before duplication has opened — one made at load, or just after the module's window came forward — is read the standard way, or returns `nil` and the reason (`… it is still opening`) under `fallback = "none"`, and the template keeps whichever picture it got for as long as the module keeps it. Under `fallback = "none"` a `pixel` that returned a colour was answered through duplication, so such a module learns its captured templates after one and asks again later when it gets `nil`; under the default fallback nothing tells a module which picture a read got. A game that is not DPI-aware is bitmap-stretched by the desktop compositor at any scaling other than 100% — Microsoft's own documentation says such applications "appear blurry" — so a template made at one scaling setting will not match pixel for pixel at another: give the search a `tolerance`, and one `scales` factor computed from the window's actual size rather than a ladder of guesses.

### macOS

Captures are in **points** (see [`host.screen.size`](#host-screen-size)), so a template cut on a Windows machine at 200% scaling is twice the size of the same element on a Retina Mac; a `capture` template taken on the Mac is in the Mac's own resolution. Without the Screen Recording permission a capture does not fail — it is a picture of the desktop wallpaper — so a `capture` template made without the grant is a template of the wallpaper, and every search without the grant will find it there. The image worker's capture measured about 19 ms per batch in the fifth macOS session; a `capture` template pays for its capture on the event loop instead, so build it once.

## host.screen.imageSearch(template, opts?) {#host-screen-imagesearch}

**Signature:** `host.screen.imageSearch(template: string | Template, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot? }?) -> (Hit?, string?)` where `Hit = { x: number, y: number, w: number, h: number, n: number, name: string? }`

Searches the screen for the first occurrence of `template` — an image path or a [`Template`](#host-screen-template) — and returns its match rectangle in **screen pixels**: `x, y` is the top-left of the match and `w, h` the matched size (the template's own, or a scaled one — see `scales`). `n` is `1` here; it is the index into the list for the calls that take several. `name` is the template's name, or the path exactly as you wrote it. Returns `nil` alone if it looked and did not find the template, and `nil` and a reason when it could not look: the region could not be captured, or a window region's window has an empty client area (see [Failure reasons](#failure-reasons)). A hit is one value, with nothing after it. The search runs top to bottom, then left to right, and the first position that matches wins; there is no score.

- A path is a **PNG** — the one image format the host is built with; a `.bmp` or `.jpg` raises `cannot open template`. A relative path is resolved against the root of the module whose code makes the call (see [the path rule](./index.md#paths)); **absolute paths are also accepted**, and neither form is checked for `..`.
- `opts.region` restricts the search area: corners or a window region (see [Region form](#region-form)); default is the full primary screen. A table that is neither form raises.
- `opts.snapshot` searches the part of the region inside that [snapshot](#reading-a-snapshot) instead of the screen, capturing nothing; the region then defaults to the whole snapshot, and a hit is still in screen pixels. A region with no part inside it answers `nil, "the region does not overlap the snapshot"`. The same holds for every search below.
- `opts.tolerance` is how far each colour channel may differ, 0–255, default `0` = exact. **A position matches only if every template pixel whose alpha is not 0 differs from the screen by at most `tolerance` in each of R, G and B.** A single pixel outside rejects the position. There is no score, correlation or percentage threshold, and the answer is the first position in row order, not the best one. A value outside 0–255 is read as `0`, an exact match, without a word, and a fraction is cut to the whole number below — keep it in range.
- `opts.scales` is a list of factors to try, **in the order given**, the first that matches winning; each resizes the template (bilinear) before searching. Omitted, the template is searched at its own size only. A factor that is not a positive finite number, that rounds the template to nothing, or that makes it larger than the region is skipped. Resizing blurs, so pair scales with a tolerance. A template keeps the variants it was resized to, keyed by their size — at most 16 of them and 16 MiB per template, the oldest going first — so a poll that passes the same scales resizes once; a variant larger than 16 MiB on its own is rebuilt on every call.
- **Template pixels with alpha = 0 are wildcards** — they are skipped during matching, so you can mask out irrelevant parts of the template. Any other alpha is compared in full: alpha is a mask, never a weight.
- The options are read loosely, unlike a [template spec](#host-screen-template): a key the search does not know — a `threshold` carried over from another matcher, a misspelt `tolerence` — is ignored without an error, and the search runs exact.
- Raises an error if the template file cannot be opened.

**Porting from a score-based matcher** (OpenCV's `matchTemplate` with a 0.9 threshold, "95 % of pixels within X"): no such threshold carries over, because nothing here counts how many pixels agreed. Make the pixels that are allowed to differ wildcards instead — cut the template as `rgba` and give them alpha 0 — and pick the smallest `tolerance` at which the remaining pixels match on every rendering you have. For "which of these states is showing" use [`imageSearchEach`](#host-screen-imagesearcheach), which answers every template from one capture; for every position of one template use [`imageSearchAll`](#host-screen-imagesearchall).

```luau
-- exact, whole-screen
local m = host.screen.imageSearch("assets/play_button.png")
if m then host.input.click(m.x + m.w // 2, m.y + m.h // 2) end

-- with a search region and fuzzy match
local hit = host.screen.imageSearch("assets/icon.png", {
    region = { x1 = 0, y1 = 0, x2 = 400, y2 = 300 },
    tolerance = 16,
})

-- in the left third of the active window's client area; nil alone is "not there"
local w = host.window.active()
if w then
    local found, why = host.screen.imageSearch("assets/icon.png", { region = { window = w, fraction = { 0, 0, 1/3, 1 } } })
    if why then host.log.info("could not look: " .. why) end
end
```

### Windows

The capture is one read on the event loop, through the module's [source](#which-picture-a-read-sees): through the standard path the fixed ~16.7 ms compositor frame whatever the region's size, through desktop duplication 0.3–5 ms on an idle desktop as measured there, plus any wait for duplication (at most 60 ms). The match runs on the event loop too and grows with the area searched and the number of scales, so a poll uses [`imageSearchAsync`](#host-screen-imagesearchasync). Coordinates are device pixels.

### macOS

Without the Screen Recording permission, macOS does **not** fail a capture — it hands back a picture of the desktop wallpaper. So a missing grant does not show up as an error or an empty result; it shows up as a search that never matches, or worse, matches something that was never on the plug-in. If every image search suddenly stops matching on a Mac, check the grant before the template.

## host.screen.imageSearchAsync(template, opts?, cb) {#host-screen-imagesearchasync}

**Signature:** `host.screen.imageSearchAsync(template: string | Template | {string | Template}, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot? }?, cb: (hit: Hit?, why: string?) -> ()) -> nil`

Like `imageSearch`, but the region capture **and** the match both run on a worker thread; `cb` fires on a later tick. Use this for anything on a detection poll — a screen capture costs about a compositor frame, and doing it inline stalls the event loop that also carries speech and key handling.

Pass a **list of templates** — paths and [`Template`](#host-screen-template) handles mixed as you like — to try several renderings of the same thing (a dialog's close glyph as two plugin versions draw it): they are matched in order against the **same captured frame**, first hit wins, and `hit.n` reports which one matched (1-based), `hit.name` its name. Chaining separate searches instead pays a fresh capture per template. Every template is resolved before the call returns, so a bad path raises here and not on the worker, and so does an `opts.region` that is neither [region form](#region-form); an empty list raises (`imageSearchAsync: no template given`). A window region is resolved at the call, from the window table given, not when the worker gets to it. `cb(hit)` is a match; `cb(nil)` alone means it looked and found none; `cb(nil, reason)` means it could not look — the region could not be captured, or a window region's window had an empty client area at the call, which is answered on a later tick without a capture (see [Failure reasons](#failure-reasons)). With `opts.snapshot` the worker searches the part of the region inside that [snapshot](#reading-a-snapshot) and captures nothing; a region with no part inside it is answered `cb(nil, "the region does not overlap the snapshot")` on a later tick. The search keeps the snapshot's pixels until it is answered, even when the snapshot is released meanwhile.

The callback belongs to the module whose VM made the call — for code a `code_module` dependency runs inside your module, that is **your** module. If that module is **disabled** before the answer arrives, the answer is not delivered; the search is held and run again when the module is enabled, and the callback gets that fresh answer — so a poll that waits for its callback before it searches again, like the overlay runtime's landmark gate, picks up where it left off, just as a [`host.timer.every`](./timer.md#host-timer-every) poll does. A held search is made again with the rectangle its region resolved to at the call. If the module is **reloaded** or removed first, the callback is dropped, never called. A search that panics inside the host is answered with `nil, "internal error: the search could not be made"` rather than never.

```luau
host.screen.imageSearchAsync({ "images/close-v8.png", "images/close-v7.png" },
    { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
    function(hit)
        if hit then host.input.click(hit.x + hit.w // 2, hit.y + hit.h // 2) end
    end)
```

### Windows

The worker captures each search's region through the asking module's [source](#which-picture-a-read-sees). Searches in one batch share a capture only when both the region and the source are the same, and the regions that go through desktop duplication are captured together, one request per `fallback` setting. Through the standard path a capture costs the fixed ~16.7 ms frame on the worker; through duplication the worker waits at most 250 ms for it, holding every read queued behind it — the image worker at the top of this page is one thread for every module.

### macOS

The same worker, with each region captured in points, one after the other; the image worker's capture measured about 19 ms per batch in the fifth macOS session. Without the Screen Recording permission the capture succeeds with a picture of the wallpaper, so the answer is usually `cb(nil)` alone — "not found" — and `cb(nil, reason)` comes only for a region that cannot be captured at all (empty, too large, or wholly off the desktop) or a window region whose window had an empty client area; a [`capture` template](#host-screen-template) made without the grant is itself a picture of the wallpaper, and will match it.

## host.screen.imageSearchEach(entries, opts?, cb) {#host-screen-imagesearcheach}

**Signature:** `host.screen.imageSearchEach(entries: { string | Template | { template: string | Template, within: Region?, name: string? } }, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot? }?, cb: (list: { Hit | false }?, why: string?) -> ()) -> nil`

**One** capture of `opts.region` on the worker thread, and an answer for **every** entry: `cb(list)`, where `list[i]` is entry *i*'s [hit](#host-screen-imagesearch) (with `n = i`) or `false`. `cb(nil, reason)` means it could not look — the region could not be captured, or a window region's window had an empty client area at the call (see [Failure reasons](#failure-reasons)) — which a list of `false` never means. With `opts.snapshot` the one picture is that [snapshot](#reading-a-snapshot), as for `imageSearchAsync`: the part of the region inside it is searched, and each `within` is cut to that part. A `within` left out is that whole part, as without a snapshot; a corner left out of a `within` that is given takes the snapshot's edge before the cut.

It is for the question "which of these states is showing?" — a menu whose cursor can be on one of several rows, a panel that shows one of several pages. `imageSearchAsync` answers only the first match of a list; separate calls share a frame only by the accident of landing in the same batch of the image worker. Here one frame is the contract.

An entry is a path, a [`Template`](#host-screen-template), or a table naming one with two options: `within`, a [region](#region-form) this entry may be found in — corners, or a window region resolved at the call; it is cut to `opts.region`, and an entry whose `within` lies outside it, is smaller than its template, or is a window region whose window has an empty client area answers `false` — and `name`, which replaces the template's name in its hit. `within` is what keeps a state's template from inventing a hit elsewhere in the window, and what keeps the search cheap: matching, unlike the capture, costs in proportion to the area searched. Entries are matched in parallel. `tolerance` and `scales` apply to every entry. The callback follows the same ownership rule as `imageSearchAsync`'s.

**Read at the call, and what raises.** The entries, their templates and every region are read before the call returns, and a mistake raises there: an empty list (`imageSearchEach: no template given`); an entry that is not a path, a Template or a table with a `template` (`host.screen.imageSearchEach: entries[2] is a table without a template`); a template that cannot be opened; an `opts.region` or a `within` that is neither region form (`host.screen.imageSearchEach: entries[2].within is neither { x1, y1, x2, y2 } nor …`), or a `within` that is not a table; a `name` that is not a string. Otherwise an entry table is read loosely: a key other than `template`, `within` and `name` is ignored. The list ends at the first `nil`, as Luau's `ipairs` does, so the entries after a hole are not searched and have no slot in the answer.

```luau
-- Which of three menu pages is open? One capture of the game window per poll, each page's
-- template looked for only in its own strip, and never two searches in flight at once.
local GAME = { title = { contains = "My Game" } }
local PAGES = { "Items", "Magic", "Status" }
local since = nil
host.timer.every(250, function()
  local win = host.window.active()
  if not win or not host.window.test(GAME, win) then return end
  if since and host.now() - since < 1000 then return end   -- one in flight, unless it got lost
  since = host.now()
  local b = win.client
  local entries = {}
  for i, page in ipairs(PAGES) do
    entries[i] = {
      template = host.path("images/menu-" .. page:lower() .. ".png"),
      within = { b.x, b.y + 40 * i, b.x + 200, b.y + 40 * i + 40 },
      name = page,
    }
  end
  -- The whole client area, as a window region; each `within` in corners, a fixed strip.
  host.screen.imageSearchEach(entries, { region = { window = win, fraction = { 0, 0, 1, 1 } }, tolerance = 12 },
    function(list, why)
      since = nil
      if list == nil then return end        -- could not look (`why` says why): keep what we knew
      for _, hit in ipairs(list) do
        if hit then host.speech.output(hit.name .. " menu") return end
      end
    end)
end)
```

### Windows

The capture runs on the worker thread, not on the event loop, through the module's [source](#which-picture-a-read-sees) exactly as `imageSearchAsync`'s does; through the standard path it costs the same fixed compositor frame as any other (~16.7 ms whatever its size), and through duplication the worker waits at most 250 ms for it. The matching is spread across cores, one entry per task.

### macOS

The same worker and the same capture as `imageSearchAsync`. Without the Screen Recording permission the capture succeeds with a picture of the wallpaper, so the answer is **not** `nil, reason` — that only means the capture itself failed (a region that is empty, too large, or wholly off the desktop) or a window region's window had an empty client area — but usually a list of `false`. Usually: a [`capture` template](#host-screen-template) that was itself made without the grant is a picture of the wallpaper, and its entry will match.

## host.screen.predicate(expr) {#host-screen-predicate}

**Signature:** `host.screen.predicate(expr: string) -> string`

Checks a colour test for the [cells](#host-screen-cells) calls and returns it in its canonical form, or raises, naming the column, when it does not parse. The cells calls take the string itself and parse it on every call, which costs microseconds, so this call is for one thing: making a mistake in a pack's colour test fail when the module loads, not in a poll, where the error would open a dialog over the game.

A predicate is an OR of AND-groups of comparisons over the 8-bit channels of a pixel, `r`, `g` and `b` (or `red`, `green`, `blue`):

```text
expr := and { OR and }        and := atom { AND atom }        atom := cmp | "(" expr ")"
cmp  := sum REL sum           sum := ["-"] term { ("+" | "-") term }
term := INT ["*" CH] | CH ["*" INT]
CH   := r | g | b | red | green | blue        REL := >= | <= | > | < | ==
AND  := and | &&      OR := or | ||
INT  := decimal digits without a leading zero, at most 2147483647
```

`and` binds tighter than `or`, as in Luau, and parentheses group, so a condition written for C#, C or JavaScript is taken as pasted: `red*10 >= green*13 && red >= 80`. Whitespace, newlines included, may stand between any two tokens. Everything is a whole number: `r >= 1.3*g` is refused with the advice to scale both sides, `10*r >= 13*g`. A term is a number times one channel, never two channels.

The canonical form is what the host evaluates. The expression is multiplied out into alternatives. Each comparison keeps its channels on the left, in r, g, b order, with its constant on the right. `>` becomes `>=` one higher, `<` becomes `<=` one lower, and `==` becomes two comparisons. A comparison whose coefficients are all negative is turned around, so `b <= 150` stays `b <= 150`. The canonical form parses back to itself.

**Limits:**
- 8192 bytes of source, which is more than the longest canonical form (6916 bytes), so whatever this call returns is accepted again.
- Parentheses nested at most 32 deep.
- At most 8 alternatives once multiplied out.
- At most 16 comparisons in one alternative; `==` counts as two.
- Once a comparison's terms are combined, each channel's coefficient is at most ±1,000,000 and the constant at most ±1,000,000,000, which keeps every test inside 32-bit integers.

**Raises** for anything that is not a string, and for every mistake in one, with the column (in characters, from 1) and the text: `host.screen.predicate: the predicate: 'x' is not a channel; use r, g or b (or red, green, blue) (column 13 of "r >= 80 and x > 3")`. Among what is refused:
- an empty string
- `!=` (write `x < y or x > y`) and `not`
- a chained `80 <= r <= 120`
- a comparison that never looks at the pixel, such as `80 >= 3` or `r - r >= 0`
- upper-case channel names
- a number with a leading zero, such as `010`: C# and Luau read it as 10, C and JavaScript as octal 8, so it is refused rather than guessed
- a parenthesis left open, and parentheses nested more than 32 deep

It never returns `nil`. It runs on the event loop, costs microseconds and touches nothing on screen.

```luau
-- A game pack's colour test, pasted from the C# reader it was recorded with, checked once.
local WARM = host.screen.predicate(
    "(red >= 80 && red*10 >= green*13 && red*10 >= blue*12) || " ..
    "(red >= 100 && green >= 45 && blue <= 150 && red >= green && green >= blue)")
host.log.info(WARM)
-- (r >= 80 and 10*r - 13*g >= 0 and 10*r - 12*b >= 0) or
--   (r >= 100 and g >= 45 and b <= 150 and r - g >= 0 and g - b >= 0)
```

### Windows

The test runs in the host on the red, green and blue bytes of each captured pixel, and the same string gives the same answer for the same colour on every platform. What can differ is the colour a capture yields for the same picture; see [`cells`](#host-screen-cells).

### macOS

As on Windows: the same string gives the same answer for the same colour. A Mac capture of a picture does not necessarily yield the colours a Windows capture of it yields; see [`cells`](#host-screen-cells).

## host.screen.cells(opts) {#host-screen-cells}

**Signature:** `host.screen.cells(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot? }) -> (Cells?, string?)` where `Cells = { cells: string, x: number, y: number, w: number, h: number }`

Captures `opts.region` once and reduces it to a grid of `cols` × `rows` cells. Each cell is the share of its pixels that pass `opts.predicate` (see [`predicate`](#host-screen-predicate)), as a byte from 0 (none) to 255 (all). The answer has two parts:
- `cells`: the grid as lower-case hex, two digits a cell, row by row from the top left. Cell (row *r*, column *c*, both counted from 0) is at digits `2*(r*cols + c) + 1` and `+ 2`.
- `x, y, w, h`: the rectangle actually read, in screen coordinates.

Hex is the format [`matchCells`](#host-screen-matchcells) takes a stored state in, so this is the call that records one: read the state once while it is on screen, and put the hex in the module's data.

A grid like this is a signature, not a picture. A selected menu entry drawn in a warm colour shows as a band of high cells, and reads the same however the rest of the picture changes. The rules are those of an existing game-menu reader, so the signatures it records can be matched here as they are:
- **Blocks.** For a region `L` pixels long cut into `n` blocks, block `s` (counted from 0) starts at `start = floor(s*L/n)` and ends before `end = max(start + 1, floor((s+1)*L/n))`: the end is exclusive, and every block has at least one pixel. A block that the division would leave empty grows by one pixel to the right (for columns) or down (for rows), onto the pixel the next block starts with. Where the region is at least as long as the grid, the blocks tile it exactly and their widths differ by at most one. Where it is shorter, several cells read the same pixels — 5 pixels cut into 36 columns gives blocks 0 to 7 all pixel 0 — and a cell's `total` counts every pixel it reads, shared ones included, so each cell still has a value.
- **Value.** `round(255 * passing / total)`, with halves rounded to even: 127.5 is 128, and 42.5 is 42. `total` is the number of pixels in the cell's block, `passing` how many of them pass the predicate.
- **Alpha** is ignored.

`opts` is **strict**. The four keys `region`, `cols`, `rows` and `predicate` are required, `snapshot` may be added, and anything else raises, naming what is wrong: a misspelt key, a `cols` of `10.5`, a string where a number goes (``host.screen.cells: opts.cols: invalid value: floating point `10.5`, expected a whole number``), a `snapshot` that is not a Snapshot or was released.
- `cols` and `rows` are whole numbers from 1 to 256, and at most 4096 cells in all.
- `region` takes either form of the [Region form](#region-form), read strictly: no default, no corner left out, no clipping. Corners may cover at most 40,000,000 pixels.
- `snapshot` reads the region where it lies in that [snapshot](#reading-a-snapshot) instead of capturing it: no screen is touched and nothing is counted in the observation line. The region must lie wholly inside the snapshot — cut to fit, every block of the grid would move.

**Returns `nil` and a reason, and never raises**, for what happens at run time:
- the window of a `{ window, fraction }` region has an empty client area, as a minimised game does (`"the window's client area is empty (0x0)"`), or one so large that the region covers more than 40,000,000 pixels;
- the screen could not be read, with the cause as the reason — under `fallback = "none"`, why desktop duplication had no picture (see [Failure reasons](#failure-reasons));
- the capture came back a different size from the region;
- with a snapshot, the region is not wholly inside it (`"the region is not inside the snapshot"`).

So a poll never puts an error dialog over the game.

**Cost:** one screen capture and the reduction, both on the event loop, counted as one screen touch in the log's observation line, like `profile` — on a snapshot, the reduction alone. The reduction looks at each pixel once and tests every comparison of the predicate on it. On the reference machine (Core i7-8700K, release build) that measured about 8 ns a pixel with an eight-comparison predicate: 0.4 ms for 91×544 pixels, 10 ms for 1280×1024 and 17 ms for 1920×1080, on top of the capture (see below). So this is the call for recording a state or reading one once; a poll uses [`matchCellsAsync`](#host-screen-matchcellsasync).

```luau
-- Record the menu's current state for a pack: the grid as hex, logged with where it was read.
local WARM = host.screen.predicate("(r >= 80 and 10*r >= 13*g and 10*r >= 12*b) or " ..
    "(r >= 100 and g >= 45 and b <= 150 and r >= g and g >= b)")
local w = host.window.active()
if w then
    local c, why = host.screen.cells({
        region = { window = w, fraction = { 0.02, 0.33, 0.09, 0.86 } },   -- x1, y1, x2, y2
        cols = 10, rows = 36, predicate = WARM,
    })
    if c then
        host.log.info(string.format("menu at %d,%d %dx%d: %s", c.x, c.y, c.w, c.h, c.cells))
    else
        host.log.info("menu not read: " .. why)
    end
end
```

### Windows

Device pixels, never scaled: a window region resolves against the client area `host.window` reports, which is `GetClientRect` on the screen. The capture reads through the module's [source](#which-picture-a-read-sees). Through the standard path it costs about 16.7 ms whatever its size up to about 1028×666, and 28 ms for the whole 1920×1080 screen; through desktop duplication, 0.3 to 1 ms for regions up to 633×418, and 5 ms for the whole screen. What covers the region is what gets read: another window over the game, the mouse pointer when Windows draws it into the picture, and black where the region leaves every monitor.

A stored state matches only if it was recorded from the same pixels:
- the same window size, or a window region, which follows the window;
- the same display scaling: a game that is not DPI-aware is stretched by the compositor at any scaling other than 100 %, so a state recorded at one scaling is not the same bytes at another;
- the same capture source. Whether the standard path and desktop duplication give the same bytes for the same game has not been measured, so record through the source the module polls with.

### macOS

Points, not pixels: a window region resolves against the client area `host.window` derives from the window's frame, and a capture on a Retina display is downsampled so that one pixel is one point (see [`host.screen.size`](#host-screen-size)). Cells computed from a Mac capture are not guaranteed to be the same bytes as those from a Windows capture of the same picture, so record the states on a Mac for a Mac. Without the Screen Recording permission the capture does not fail: it is a picture of the wallpaper, so the call returns cells of the wallpaper, not `nil`. This call pays for its capture on the event loop.

## host.screen.matchCells(opts, states) {#host-screen-matchcells}

**Signature:** `host.screen.matchCells(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot? }, states: { string | { cells: string, name: string? } }) -> (CellsMatch?, string?)` where `CellsMatch = { cells: string, x: number, y: number, w: number, h: number, index: number, name: string?, similarity: number, distance: number, runnerUp: number, similarities: { number } }`

Reads the region exactly as [`cells`](#host-screen-cells) does, and says which of `states` it looks most like. Like `cells`, it reads the screen, or the [snapshot](#reading-a-snapshot) `opts.snapshot` gives. A state is either the hex `cells` recorded, or a table `{ cells = hex, name = "Start" }`. The hex may be in either case, and must be exactly two digits per cell of this grid.

**Similarity** is `1 - sum(|a - b|) / (cells * 255)` over the two grids: 1 when they are identical, 0 when every cell is as far apart as it can be. `distance` is the sum itself, an exact whole number.

**States with the same name are one item**: several recordings of one menu entry. A state without a name is an item of its own. An item's score is its best state's score, and the item with the best score wins; on a tie, the item that comes first in `states` wins. The answer describes the winner:
- `index`: the winning item's best state, 1-based, into `states` (on a tie, the first of them);
- `name`: that state's name, absent for a state without one;
- `similarity` and `distance`: its score;
- `runnerUp`: the best score among the other items, or `0` when there is no other item;
- `similarities`: every state's score, in the order given;
- `cells`, `x`, `y`, `w`, `h`: what `cells` returns.

The decision is the module's to make in Luau, and a menu reader's two thresholds both go there: `m.similarity >= MINIMUM and m.similarity - m.runnerUp >= MARGIN`.

**Raises** for everything [`cells`](#host-screen-cells) raises for, and for these, checked before anything is captured:
- `states` that is not a list of 1 to 1024 entries, or that has other keys;
- a state that is not hex of the right length (`states[3]: 358 characters; this grid's states are 720 hex digits, two per cell`);
- a state that is not hex at all (`states are hex (720 characters), not the cells' raw bytes`);
- a state with a character that is not a hex digit, counted in characters (`states[2]: 'é' at character 5 is not a hex digit`);
- a state table with a key other than `cells` and `name`.

**Returns `nil` and a reason** exactly where `cells` does.

**Cost:** that of `cells`, plus two parts that grow with states × cells. Every call checks every state and decodes it from hex on the event loop before anything is captured — nothing is kept from one call to the next — which measured on the reference machine (release build) 0.04 ms for 10 states of 360 cells, 0.4 ms for 100, 4.4 ms for 1024, and 29 ms at the limits, 1024 states of 4096 cells: about 11 ns per cell of every state. Then the grid is compared with every state, which for 360 cells and a hundred states is microseconds. It all runs on the event loop, so for a poll use [`matchCellsAsync`](#host-screen-matchcellsasync) — which still checks and decodes the states at the call. Neither shares its picture with [`host.ocr.read`](./ocr.md#host-ocr-read), which captures on a thread of its own: detecting with a cells match and then reading the text is two pictures. Several cells matches, searches and point reads share one picture when they are given one [snapshot](#reading-a-snapshot).

```luau
-- A key that says which menu entry is selected: read once, ranked, and spoken only when sure.
local pack = host.json.decode(host.resource.read("data/menu.json"))
-- pack = { region = { x1, y1, x2, y2 }, cols, rows, predicate, states = { { name, cells }, … } }
local w = host.window.active()
if w then
    local m, why = host.screen.matchCells({
        region = { window = w, fraction = pack.region },
        cols = pack.cols, rows = pack.rows, predicate = pack.predicate,
    }, pack.states)
    if not m then
        host.speech.output("Menu not read: " .. why)
    elseif m.similarity >= 0.97 and m.similarity - m.runnerUp >= 0.01 then
        host.speech.output(m.name)
    end
end
```

### Windows

The capture and the pixels are those of [`cells`](#host-screen-cells): device pixels, through the module's source, on the event loop, and states recorded from the same pixels match exactly.

### macOS

The capture and the pixels are those of [`cells`](#host-screen-cells): points, with states recorded on a Mac for a Mac. Without the Screen Recording permission this compares the wallpaper with the states and returns a match rather than `nil`; its `similarity` is whatever the wallpaper happens to score, so the thresholds are what keep it quiet.

## host.screen.matchCellsAsync(opts, states, cb) {#host-screen-matchcellsasync}

**Signature:** `host.screen.matchCellsAsync(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot? }, states: { string | { cells: string, name: string? } }, cb: (m: CellsMatch?, why: string?) -> ()) -> nil`

[`matchCells`](#host-screen-matchcells), with the capture, the reduction and the comparison on the image worker thread. Given `opts.snapshot`, the worker reduces the region where it lies in that [snapshot](#reading-a-snapshot) and captures nothing; a region not wholly inside it is answered `cb(nil, "the region is not inside the snapshot")` on a later tick. `cb(m, nil)` or `cb(nil, reason)` is called on a later tick, exactly once — with two exceptions: a module **reloaded or removed** before the answer comes has its callback dropped, never called, and one **disabled** has it held for as long as it stays disabled (below). Once includes a window region whose client area was empty: that answer, too, comes on a later tick, without a capture, and so does every other reason under [Failure reasons](#failure-reasons).

Everything is checked at the call, on the calling thread, so every mistake `matchCells` raises for raises here, as does a `cb` that is not a function. A window region is resolved at the call, from the window table given, not when the worker gets to it.

Reads the image worker takes in one batch (see the top of this page) share one capture when they ask for the same region through the same source, whether they are cells matches or image searches, and from one module or several; a read given a snapshot reads that and captures nothing. A [`host.ocr.read`](./ocr.md#host-ocr-read) never shares that capture: it captures on a thread of its own, so detecting with a cells match and then reading the text is two pictures, taken at two moments — unless both are given the same [snapshot](#reading-a-snapshot), which makes them one. Callbacks run in the order the reads were queued. The callback belongs to the module whose VM made the call, under [`imageSearchAsync`](#host-screen-imagesearchasync)'s rules with one difference. If that module is disabled before the answer comes, the read is held, but it is not made again when the module is enabled, as a search is: its rectangle was worked out at the call, and by then the window may have moved, changed size or closed. Instead the callback gets `nil, "the module was disabled while the read waited"` on a later tick once the module is enabled, without a capture, so a poll that waits for its callback asks again. ([`host.ocr.read`](./ocr.md#host-ocr-read) drops its callback on a disable instead.) If the module is reloaded or removed, the callback is dropped. A panic inside the reduction is answered with `nil, "internal error: …"`.

**Read, then act, is not guarded here.** A pending [`host.ocr.read`](./ocr.md#host-ocr-read), a plain [`snapshotAsync`](#host-screen-snapshotasync), or a change wait without `at` whose baseline is its own first picture makes the same module's `host.input.*` and `host.window.focus` wait, up to 50 ms, until that picture is taken; a timed `snapshotAsync` and a change wait with a `from` it can compare with do not. A pending `matchCellsAsync` does not, and neither does an image search: a click made while it waits may land before its picture is taken, so its callback can describe the screen after the click. Act on a match inside its callback, or match on a snapshot taken before the click.

```luau
-- Say the selected menu entry when it changes: one read in flight, only while the game is in front.
local pack = host.json.decode(host.resource.read("data/menu.json"))
host.screen.predicate(pack.predicate)                        -- a bad pack fails here, at load
local GAME = { title = { contains = "My Game" } }
local since, last = nil, nil
host.timer.every(150, function()
    if since and host.now() - since < 1000 then return end   -- one in flight, unless it got lost
    local w = host.window.active()
    if not w or not host.window.test(GAME, w) then return end
    since = host.now()
    host.screen.matchCellsAsync({
        region = { window = w, fraction = pack.region },
        cols = pack.cols, rows = pack.rows, predicate = pack.predicate,
    }, pack.states, function(m, why)
        since = nil
        if not m then return end                              -- could not look: keep what we knew
        local name = (m.similarity >= 0.97 and m.similarity - m.runnerUp >= 0.01) and m.name or nil
        if name ~= last then
            last = name
            if name then host.speech.output(name) end
        end
    end)
end)
```

### Windows

The capture runs on the worker thread, not the event loop, through the module's [source](#which-picture-a-read-sees), exactly as `imageSearchAsync`'s does; through the standard path it costs the same fixed compositor frame as any other. The reduction runs there too, so what a poll costs the event loop is checking its arguments and decoding its states (see [`matchCells`](#host-screen-matchcells)' cost: 0.4 ms for 100 states of 360 cells), resolving its region, and — in a module that reads through desktop duplication, until duplication has answered once — the [first-read comparison](#which-picture-a-read-sees).

### macOS

The same worker and the same capture as `imageSearchAsync`, in points. Without the Screen Recording permission the answer is a match against the wallpaper, not `nil`: `nil` and a reason only mean the region could not be read, or a window region's window had an empty client area.

## Region form {#region-form}

A region is a rectangle on screen, written in one of two forms: by its **corners**, or as **fractions of a window's client area**. **Every call that takes a region takes both**, through one reader: `profile`, `template`'s `capture`, `imageSearch`, `imageSearchAsync`, `imageSearchEach` (its `opts.region` and each entry's `within`), `imageSearchMulti`, `imageSearchAll`, `save`, `saveMarked`, `cells`, `matchCells`, `matchCellsAsync`, `snapshot` and a snapshot's `crop`, `snapshotAsync` and each region of its `change.watch`, [`host.ocr.read`](./ocr.md#host-ocr-read), [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) and each region of [`recognizeMany`](./ocr.md#host-ocr-recognizemany)'s `regions`. [`pixel`](#host-screen-pixel) and each point of [`pixels`](#host-screen-pixels) take a point in the same window form, `{ window = w, fraction = { x, y } }`. A region left out — `nil` — is the whole primary screen wherever a call allows leaving it out, and the whole snapshot for a call given one; the cells calls, `snapshot`, `snapshotAsync` and its `watch`, `crop` and `host.ocr.read` do not allow it.

**Corners.** A region describes an axis-aligned rectangle by its top-left and bottom-right corners and may be written in **named** or **positional** form:

- Named: `{ x1 = .., y1 = .., x2 = .., y2 = .. }`
- Positional: `{ x1, y1, x2, y2 }` (array indices `[1]`=x1, `[2]`=y1, `[3]`=x2, `[4]`=y2)

`x2` and `y2` are **exclusive**: the rectangle starts at `(x1, y1)` and is `x2 - x1` wide and `y2 - y1` high, so `{ 10, 20, 210, 120 }` covers columns 10 to 209. How strictly corners are read is each call's own, kept as it has always been: the cells calls, `snapshot`, `crop`, `snapshotAsync` and each region of its `change.watch`, and `host.ocr.read` read them strictly, every other call loosely (both below).

**Window fractions.** `{ window = w, fraction = { x1, y1, x2, y2 } }` gives a rectangle as fractions of a window's client area, and the host turns it into pixels (points on macOS) at the call.
- `window` is a window table as `host.window` returns it (a control table works too). Only its `client = { x, y, w, h }` is read, with no call to the operating system, so the region is exactly as current as the table. A table from `host.window.active()` is the one [kept for the epoch](./timer.md#host-epoch): within one epoch every call hands back the same table, and a [`host.timer.every`](./timer.md#host-timer-every) tick does not turn the epoch over, so in a poll the fractions resolve against the client rectangle the window had when the epoch last turned over. A window that has moved or changed size since then, with no event in between, is read at its old place; `host.window.find` asks the operating system afresh on every call (see [`host.window`](./window.md#table-shapes)).
- `fraction` is `{ x1, y1, x2, y2 }`, named or positional like corners, in fractions of the client area's width (`x1`, `x2`) and height (`y1`, `y2`): `{ 0, 0, 1, 1 }` is the whole client area, and `{ 0.5, 0, 1, 1 }` its right half. A reader that stores a region as `XStart, XEnd, YStart, YEnd` writes `{ XStart, YStart, XEnd, YEnd }`.
- With `W` the client area's width, the columns read start at `floor(W * x1)` and end before `ceil(W * x2)`. The start is then clamped to 0 through `W - 1`, and the end to one past the start through `W`; rows are worked out the same way from the height. So the rectangle never leaves the client area and is never empty: a fraction outside 0 to 1, or an `x2` before `x1`, still reads at least one column. The arithmetic is IEEE double precision, exactly as a C# reader computing with `double` does it. `800 * 0.035` is 28.000000000000004 there, so the end is 29.
- The numbers themselves never raise for being outside 0 to 1 or in the wrong order, as the clamps above say. What raises, in every call, is the shape: a fraction that is not a finite number (`1/0`, NaN) or not a number, a `fraction` table with a corner missing, with named and positional corners mixed or with any other key, a key beside `window` and `fraction`, and a `window` that is not a table or has no `client` with four whole numbers.
- A client area without width or height — a minimised window — is a runtime condition, answered and never raised: the call fails the way it fails when it could not look, with the reason `the window's client area is empty (0x0)` ([Failure reasons](#failure-reasons)). `host.ocr.read` and `recognizeMany` answer that one region so and read the others; an `imageSearchEach` entry whose `within` it is answers `false`.

Written as fractions, a region keeps covering the same part of the window when the window is resized, at any display scaling, and on both platforms, with nothing in the module that knows which of those it is running under.

**A table that is neither form raises**, in every call, naming the call and the argument: `{}`, and a window's `bounds`-style table `{ x = …, y = …, w = …, h = … }`, which has none of the four corners (`host.screen.profile: opts.region is neither { x1, y1, x2, y2 } nor { window = w, fraction = { x1, y1, x2, y2 } }: it has none of x1, y1, x2, y2 (its keys: h, w, x, y); a rectangle { x, y, w, h }, such as a window's bounds, is written { b.x, b.y, b.x + b.w, b.y + b.h }`). So does a region that is not a table at all, `region = "screen"` included. Only leaving the region out reads the whole primary screen.

**Strictly read corners.** The cells calls, `snapshot`, a snapshot's `crop`, `snapshotAsync` and each region of its `change.watch`, and `host.ocr.read` raise at the call for anything in corners that is not exactly right, naming it (`host.screen.cells: opts.region.x2 must be a whole number, got 10.5`):
- all four corners are required, as whole numbers (`10.0` is one, `10.5` is not), written one way (named or positional, not both), with `x2` greater than `x1` and `y2` greater than `y1`, and any other key raises;
- the region has no default: `{}` has no corners and raises like any other region with a corner missing, and a region the cells calls read is not clipped to the screen, so a part off every monitor reads black.

Round a computed corner yourself, `math.floor(v + 0.5)` for the nearest pixel, or give the region as fractions of the window it belongs to.

**Loosely read corners.** Every other call reads corners as it always has: a table with at least one of `x1`, `y1`, `x2`, `y2` (or entries 1 to 4) is corners, and inside it nothing raises. Named keys take precedence over positional ones, corner by corner. A corner that is missing — or is not a number, such as a misspelt key's `nil` or a string that is not a numeral — takes its default without a word: `x1, y1` become `0`, `x2, y2` the primary screen's width and height (for a call given a snapshot, the snapshot's own edges). Other keys are ignored. A fractional coordinate is cut toward zero, like every coordinate the platform takes: a centre computed as `x + w / 2` lands on the whole pixel toward zero, which for a negative coordinate — a monitor left of or above the primary — is the opposite direction from `//`. Where the nearest pixel matters, round with `math.floor(v + 0.5)`. Corners turned around, or equal, read as an empty region — width and height below zero count as zero — which most calls answer as a read that failed rather than a mistake: `profile` with `nil, "the region is empty (0x0)"`, the searches, `save` and `saveMarked` with `screen capture failed` (given a snapshot, `the region does not overlap the snapshot`), and `recognizeMany` with an `error` entry for that region; `template{ capture = … }` and `recognize` raise for it. (The two OCR calls differ on macOS, below.)

`recognizeMany` raises when `regions` is missing, and its list ends at the first `nil`, so the regions after a hole are not read.

```luau
-- these two regions are equivalent
{ region = { x1 = 10, y1 = 20, x2 = 210, y2 = 120 } }
{ region = { 10, 20, 210, 120 } }

-- a window's bounds, converted
local w = host.window.active()
if w then
  local b = w.bounds
  local p = host.screen.profile({ region = { b.x, b.y, b.x + b.w, b.y + b.h }, axes = "rows" })
end

-- the lower left of the active window's client area, as fractions: follows the window
if w then
  local c = host.screen.cells({ region = { window = w, fraction = { 0, 0.4, 1/3, 1 } },
                                cols = 3, rows = 6, predicate = "r > g + b" })
end

-- the same form in every call: a strip across the top of the client area, searched and saved
if w then
  local strip = { window = w, fraction = { 0, 0, 1, 0.1 } }
  local hit, why = host.screen.imageSearch("images/tab.png", { region = strip, tolerance = 8 })
  if why then host.log.info("could not look: " .. why) end
  host.screen.save("strip.png", { region = strip })
end

-- and for text: the top tenth of the client area, read off the event loop
if w then
  host.ocr.read({ window = w, fraction = { 0, 0, 1, 0.1 } }, function(r)
    host.log.info(r.status .. ": " .. (r.error or r.text))
  end)
end
```

### Windows

Fractions resolve to device pixels of the client area `GetClientRect` reports, translated to the screen.

### macOS

Fractions resolve to points of the client area derived from the window's frame (see [the window table](./window.md#table-shapes)). The same fractions cover the same part of a window as on Windows, where corners measured on one platform would need converting for the other.

Empty corners given to the OCR calls are not refused here: `recognize` answers `{ text = "", words = {}, skipped = false }` instead of raising, and `recognizeMany` gives that region the same entry, without an `error` — as every failed capture there does (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)).

## host.screen.save(path, opts?) {#host-screen-save}

**Signature:** `host.screen.save(path: string, opts: { region: Region?, snapshot: Snapshot? }?) -> (boolean, string?)`

Captures `opts.region` and writes it to `path` as a PNG — corners or a window region, see [Region form](#region-form); omitted, the whole primary screen. With `opts.snapshot` it writes the part of the region inside that [snapshot](#reading-a-snapshot) instead, capturing nothing — omitted, the whole snapshot — and returns `false, "the region does not overlap the snapshot"` when no part is inside it: the picture a module's reads were answered from, for somebody to look at. PNG is the one image format the host is built with, and the format follows the extension, so `path` must end in `.png`, in upper or lower case: any other extension, or none, raises before anything is captured, whether or not the capture would have worked (`host.screen.save: 'C:\…\shot.jpg' does not end in .png; PNG is the one image format the host writes`). This is the evidence call: an author who cannot see the screen has no way to check that a coordinate lands on its button, so the picture goes to somebody who can look at it — `tools/probe` writes one shot of the window beside its text dump for exactly that, and the overlay runtime's calibration crop key (`Ctrl+Alt+Shift+T`, Command+Option+Shift+T on a Mac) uses it to cut a fresh template out of the live plug-in when an old one stopped matching. It costs one capture on the event loop, so about one compositor frame through the standard path on Windows (~16.7 ms) regardless of the region's size, less through desktop duplication (see [Which picture a read sees](#which-picture-a-read-sees)), plus the PNG encode; the macOS round trip is unmeasured.

A relative `path` is resolved under the root of the module whose code makes the call (see [the path rule](./index.md#paths)); an absolute path (what `host.path` returns) is used as it stands, and the parent directory is created if it does not exist. Neither is checked for `..`: this writes wherever the path points. `save` and `saveMarked` are the only calls in the platform that write a file a module names, and both write PNG only. Returns `true` on success — one value — and `false` and a reason when the region could not be captured, or a window region's window has an empty client area (see [Failure reasons](#failure-reasons)). A path that cannot be *written* raises instead of returning `false`, so a typo in the folder name is not mistaken for a failed capture, and so does an `opts.region` that is neither region form.

```luau
-- tools/probe: the one artefact a sighted helper can check in seconds, saved next to
-- the numbers it is supposed to confirm.
local win = host.window.active()
if not win then
  host.log.info("probe: nothing is focused, so there is nothing to capture")
else
  local shot = "probe.png"
  local saved, why = host.screen.save(shot, {
    region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
  })
  host.log.info(string.format(
    "capture: %s (%dx%d requested)", saved and shot or ("FAILED: " .. why), win.bounds.w, win.bounds.h
  ))
end
```

### Windows

The shot is taken through the module's [source](#which-picture-a-read-sees) like every other read. In a module that declares `capture = "duplication"`, a shot taken before duplication has opened — at load, or just after the module's window came forward — is taken the standard way, or `save` returns `false` and the reason under `fallback = "none"`, so a template cut from such a shot is not necessarily the kind of picture the module's searches read.

### macOS

Without the Screen Recording permission the capture does **not** fail — macOS hands back a picture of the desktop wallpaper. A saved shot is therefore the cheapest way to tell a missing grant from a wrong rectangle: if the PNG shows the wallpaper, no coordinate in the module is at fault. A template cut around the same on-screen element on a Retina Mac also comes back at point resolution, i.e. half the pixel dimensions of the same crop taken on a HiDPI Windows machine (see [`host.screen.size`](#host-screen-size)).

## host.screen.saveMarked(path, opts) {#host-screen-savemarked}

**Signature:** `host.screen.saveMarked(path: string, opts: { region: Region?, marks: { { x: number, y: number } }?, snapshot: Snapshot? }) -> (boolean, string?)`

Everything `save` does — a PNG, and only a `.png` path, `true`, or `false` and a reason, from the screen or from `opts.snapshot` — plus a magenta crosshair drawn at every point in `opts.marks`, given in **screen** coordinates whichever form `opts.region` takes: a window region is resolved to its screen rectangle first (on a snapshot, to the part of it inside the snapshot), and each mark is drawn where its screen point falls in that rectangle. It puts the calibration question into one picture — is my control on its button? — instead of leaving somebody to read coordinates out of a log and find that spot in a plain screenshot by hand, which is the step that hides errors: a set of toggles in this repo sat 16 px off, on the caption row under the buttons, for as long as it existed, because the colours sampled there happened to look plausible. Magenta because these dark plug-in UIs do not use it, so ink cannot be mistaken for interface. Each crosshair is 19 px across, and the *n*th mark gets *n* dots on a row 12 px below its centre, so marks stay tellable apart without any text.

Four things bite. `opts` is **required** here (`save`'s is optional) — pass at least `{}`. Mark entries are read by **name**: `{ x = 100, y = 200 }`, never positional `{ 100, 200 }`, which reads as screen `(0, 0)`: dropped when the captured region does not reach the screen origin, and otherwise drawn in the corner, which is worse — a crosshair that means nothing. A mark outside the captured region is skipped without a word, so grow the region to cover the marks the way `calibrationShot` does — Komplete Kontrol's standalone menu bar sits 10 px above the client top, and a shot clipped to the client rectangle cut every crosshair off, producing a picture with no marks at all, which reads as "nothing was marked" rather than "the marks are off-frame". And the ink lands on the very thing the mark points at, so when you also need to *look* at those pixels, take the same frame twice.

```luau
-- overlay-runtime's calibration shot (Ctrl+Alt+Shift+S): one crosshair per VISIBLE control
-- — a hidden one gets none, because marking it would claim the user can reach it.
local o = self:origin()
local region = { o.client.x, o.client.y, o.client.x + o.bounds.w, o.client.y + o.bounds.h }
local stem = "kontakt"
local marks = {}
for _, c in ipairs(self.controls) do
  if isVisible(self, c) then
    local x, y = calibrationPoint(self, c)
    if x then marks[#marks + 1] = { x = x, y = y } end
  end
end
host.screen.saveMarked(host.path("calibration/" .. stem .. ".png"), { region = region, marks = marks })
-- The SAME frame without the ink: measuring why a slider's thumb stopped matching meant
-- reading its knob through 32 pixels of magenta. One more capture of a known region.
host.screen.saveMarked(host.path("calibration/" .. stem .. "-clean.png"), { region = region, marks = {} })
```

### Windows

The capture is `save`'s: one read on the event loop through the module's [source](#which-picture-a-read-sees), and under `fallback = "none"` a shot duplication could not answer returns `false` and the reason, with no file written. Marks, region and PNG are all in device pixels, so a crosshair lands on the very pixel its screen point names at any display scaling.

### macOS

The capture is `save`'s, so the wallpaper-instead-of-error case applies here too — and it is nastier with marks on it, because crosshairs drawn over a picture of the desktop look like a considered answer rather than a missing permission. Check that the shot shows the plug-in before reading anything into where the marks landed.

## host.screen.imageSearchAll(template, opts?) {#host-screen-imagesearchall}

**Signature:** `host.screen.imageSearchAll(template: string | Template, opts: { region: Region?, tolerance: number?, snapshot: Snapshot? }?) -> ({ Hit }, string?)`

Returns **every** match of `template` in the region, as an array of [hits](#host-screen-imagesearch) in screen pixels, rather than stopping at the first one like `imageSearch`. `template` is a path or a [`Template`](#host-screen-template); each hit carries `n = 1` and the template's `name`. `opts.region` is corners or a window region ([Region form](#region-form)), and a table that is neither raises. The list is never `nil`. It is empty when there is no match, and then comes alone; when the region could not be captured — or a window region's window has an empty client area — it is empty too and a reason comes after it, `{}, reason` (see [Failure reasons](#failure-reasons)), so the two can be told apart by a caller that asks. Its use is deciding whether a template is safe to click blindly: a close-glyph template that matches twice will eventually click the wrong one, and a template that matches nowhere is a control that silently never fires. "Matched exactly once, here" is the answer you want before shipping either, which is why the overlay runtime keeps it on a calibration key rather than in any live path.

It cannot stop early, so it always pays the **full** scan of the region, synchronously on the event loop — and matching, unlike the capture, grows with the area; there is no async form of this call. Two further limits are worth knowing before you read a count. It honours `tolerance` but **not** `scales`: the search runs at the template's native size only, so a control your overlay matches through a scale ladder can verify as zero hits here without anything being wrong with it. And de-duplication is per row — after a hit the scan resumes past that hit's right edge on the same row, but the next row starts again from the left, so a slack tolerance can report one button once per row it survives on.

```luau
-- overlay-runtime, Ctrl+Alt+Shift+V: is the focused control's template unique in the window?
local hits = host.screen.imageSearchAll(image, {
  region = { o.client.x, o.client.y, o.client.x + o.bounds.w, o.client.y + o.bounds.h },
  tolerance = 8,
})
host.log.info(string.format("%s: %d match(es) for %s", tostring(c.label), #hits, tostring(image)))
for i, h in ipairs(hits) do
  host.log.info(string.format("  %d at (%d,%d) %dx%d", i, h.x, h.y, h.w, h.h))
end

-- Zero matches, or could not look? The second value says which.
local all, why = host.screen.imageSearchAll(image, { region = { window = o, fraction = { 0, 0, 1, 1 } } })
if why then host.log.info("not searched: " .. why) end
```

### Windows

The capture is one read on the event loop through the module's [source](#which-picture-a-read-sees): the fixed ~16.7 ms frame through the standard path, less through desktop duplication plus any wait for it (at most 60 ms). The full scan that follows is on the event loop too, in device pixels.

### macOS

The capture is in points, on the event loop. Without the Screen Recording permission it does not fail: the list is the matches in a picture of the wallpaper — usually none, and no reason with it.

## host.screen.imageSearchMulti(templates, opts?) {#host-screen-imagesearchmulti}

**Signature:** `host.screen.imageSearchMulti(templates: {string | Template}, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot? }?) -> (number, Hit) | (nil, nil) | (nil, nil, string)`

Captures the region **once** and tries each template against that one frame, returning two values: the 1-based index of the first template that matched, and its [hit](#host-screen-imagesearch) in screen pixels (whose `n` is that same index). The list may mix paths and [`Template`](#host-screen-template) handles as you like. Reach for it whenever one question needs more than one template — a toggle's on/off pair, or a glyph as two plug-in versions draw it. The capture is the fixed cost (about a compositor frame on Windows, whatever its size); over a small region the extra comparisons are the cheap part, which is what makes one call beat two. The index is what turns a hit back into meaning: build the list in a known order and keep a parallel list of labels.

Sharing the frame also buys correctness, not only time: both templates are matched against the same pixels, so a toggle that changes mid-repaint cannot fall between two separate captures and come back as neither. Templates are tried in list order and the whole region is scanned for the first before the second is tried, so when both would match, order decides and not position. Decodes of paths are cached for the 64 most recently used paths across the whole application (least recently used out first, keyed by path and modification time), shared by every search: a re-captured template is picked up live, and a poll over a few templates does not re-decode PNGs. A module cycling through more templates than that re-reads and re-decodes them from disk; build [`host.screen.template{ file = … }`](#host-screen-template) handles once at load instead, since a handle keeps its decode. A template file that cannot be opened raises, as with `imageSearch`, and every template in the list is opened **before** the capture — so a bad path raises on the first call wherever it sits in the list; so does an `opts.region` that is neither [region form](#region-form). When the region could not be captured, or a window region's window has an empty client area, the call returns the same two `nil`s as "no template matched" and the reason as a **third** value, `nil, nil, reason` (see [Failure reasons](#failure-reasons)): code that reads one or two values reads what it always did, and code that asks can tell "off" from "I could not look" — for a state read like `gtoggleState`, the difference that matters. This call is synchronous — for anything on a detection poll use [`imageSearchAsync`](#host-screen-imagesearchasync), which takes a template list for the same reason and moves the capture off the event loop.

```luau
-- overlay-runtime's gtoggleState: read a graphical toggle. The region is touched ONCE,
-- not once per template, and the hit's index maps straight back to the on/off label.
local templates, labels = {}, {}
addTemplates(templates, labels, c.onImage, "on")   -- appends each path with its label
addTemplates(templates, labels, c.offImage, "off")
if #templates == 0 then return nil end
local idx = host.screen.imageSearchMulti(templates, {
  region = region,
  tolerance = c.tolerance or MATCH_TOLERANCE,
  scales = c.scales or MATCH_SCALES,
})
return idx and labels[idx] or nil

-- Ignore the index and take the second return when you only need WHERE: the runtime's
-- sliderThumb passes a one-entry list and centres on the hit.
local _, hit = host.screen.imageSearchMulti(asList(c.thumbImage), {
  region = region,
  tolerance = c.tolerance or MATCH_TOLERANCE,
  scales = c.scales or MATCH_SCALES,
})

-- "Off", or could not look? The third value says.
local n, _, why = host.screen.imageSearchMulti({ "images/on.png", "images/off.png" }, { region = region })
if why then return nil end   -- keep what we knew; `why` is the reason
```

### Windows

The capture is one read on the event loop through the module's [source](#which-picture-a-read-sees): the fixed ~16.7 ms frame through the standard path whatever the region's size, less through desktop duplication plus any wait for it (at most 60 ms). The templates are then matched on the event loop, one after the other, in device pixels.

### macOS

The capture is in points, on the event loop. Without the Screen Recording permission it does not fail — the templates are matched against a picture of the wallpaper, so the answer is usually `(nil, nil)`. The third value comes only for a region that cannot be captured at all (empty, too large, or wholly off the desktop) or a window region whose window has an empty client area.
