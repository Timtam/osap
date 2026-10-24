---
title: "host.screen — pixels, snapshots, profiles, cells and image search"
sidebar_position: 14
toc_max_heading_level: 2
---

Reads what is on screen: the colour of a point or of several, a per-column and per-row profile of a region, a region reduced to a grid of cells and compared with stored ones, and a search for a template image within one — a PNG, or a template built in memory from bytes or from the screen itself. Each of them reads the screen as it is at the call, or a snapshot of it taken once and read by several of them.

Use it where there is no element to ask. State that exists only as colour — Kontakt reads whether its snapshot bar is open from one pixel on the camera icon, and ON:EAR classifies each of its switches from a single pixel — and anything whose position moves, which is what image search is for, since Kontakt's five instrument-editor sections are found by their captions because opening one pushes the rest down the window.

**Every touch of the screen costs a compositor frame** through the standard path on Windows, measured at about 16.7 ms whether it reads one pixel or a whole window. So `pixel` is not the cheap call it looks like: read a point once per pump and let everything that asks about it share the answer, and read several points with one [`pixels`](#host-screen-pixels). When the question is *where* something is rather than what colour a known point has, `profile` reduces one capture to a value per column and row — the way Melodyne finds a selected note by diffing two column profiles — and it does both off the event loop: in a handler it waits for them, and given a callback it waits nowhere.

**Several questions about one moment** — a menu's selected entry, its page title and the help bubble beside it — are asked of one [`snapshot`](#host-screen-snapshot): take the picture once and pass it to each read as `{ snapshot = s }`. They then read that one picture, touch the screen once between them, and cannot disagree because the screen changed between two captures ([Reading a snapshot](#reading-a-snapshot)). Given a callback, [`snapshot`](#snapshot-with-a-callback) takes that picture off the event loop and waits for nothing: at once, at a set time after a press, or once part of the screen has changed — the cursor that moved after a D-pad press, a help bubble that is up for a moment.

The matching, unlike the capture, does grow with the area. A template search across a whole plug-in window is the slow call here — measured at twelve seconds for one full-region match while twelve library overlays polled the same window every 500 ms — which is why a search is made on the image worker, which a handler waits for, and why anything on a detection poll passes `imageSearch` a callback, which waits for nothing, and re-searches a small box around the last hit before widening.

**Every call here that reads the screen or works over a region — `snapshot`, `pixel`, `pixels`, `profile`, `cells`, `matchCells`, `template`, a snapshot's `crop`, `imageSearch`, `imageSearchAll`, `save` and `saveMarked` — takes a callback as its last argument**, and then waits for nothing wherever it is called: its answer is handed to the callback on a later tick ([With a callback](#with-a-callback)). `imageSearchEach` is a callback's only. Without a callback such a call **waits in a handler** while the work is done on a thread of the host's, and only its own module waits; **where it cannot wait** it raises, and nothing is read on the event loop, the one thread that also carries speech, hotkey and key callbacks, timers and, on macOS, the event tap ([Where it waits](#where-it-waits)). Every form checks its arguments and resolves its region on the event loop — a search checks there the template paths it is given, each the first time it is named and by its PNG header alone, and leaves opening them to the image worker. The work goes to the threads below: the picture of `snapshot` and `profile` and the points of `pixel` and `pixels` to the `screen-capture` thread; `profile`'s reduction, the grid of `cells` and `matchCells`, the picture of `template{ capture }`, the copy of `crop`, the capture and the matching of the searches, and the picture, the marks, the PNG and the file of `save` and `saveMarked` to the image worker, whose matching pool does all of it but the captures. Only the answer's tables are built on the event loop, as the handler goes on or the callback runs. `pixel` and `pixels` given a snapshot read no screen, and answer at once wherever they are called — a callback on the next tick. A [`template`](#host-screen-template) built from bytes or a file checks its arguments on the event loop — the sizes and the byte count, or the PNG's header — and answers at once, wherever it is called; its picture is made on the image worker. Every other call on this page — `size`, `predicate`, `pending`, a snapshot's `release` — runs on the event loop and answers at once.

**The `screen-capture` thread is one thread, shared with [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)**: it takes the text reads' pictures and those of `snapshot` and `profile`, and reads the points of `pixel` and `pixels` — waited for in a handler or given a callback — one capture at a time, and nothing else. Which goes first: a request asked from a poll or while a module loads that has waited 500 ms; then one asked while the host dispatched a key, a hotkey, a controller event, a window coming forward or a focus change; then the rest — within each, the one due longer, and on a tie the text read. So a picture waits for at most the capture in progress and the ones ahead of it by that order, and a change wait that photographs round after round cannot keep a poll's text read waiting beyond 500 ms. Nothing on it waits for a module's input, and no input waits for it: `host.input.*` and [`host.window.focus`](./window.md#host-window-focus) act at once, so a picture asked for before a click and taken after it shows the click, and its `inputEpoch` says so — after a `post` or a `focus`, its `time` does ([`host.inputEpoch`](./timer.md#host-inputepoch)). A module acts on a picture once it is answered; the log names one that acts while a picture it asked for is still to be taken, once a session ([host.input](./input.md) says which pictures count).

**The image worker is one thread, shared by every module**, and it does the work of `cells`, `matchCells`, `template{ capture }`, a snapshot's `crop`, `imageSearch`, `imageSearchAll`, `save` and `saveMarked` — waited for in a handler or given a callback — and of `imageSearchEach`, and the reduction of every `profile` ([Where it waits](#where-it-waits), [With a callback](#with-a-callback)). It also opens the templates of every search a callback or a handler waits for: it decodes a PNG the first time a search names it and again after the file changed, and keeps it decoded meanwhile, and it makes the picture of a [`template`](#host-screen-template) built from bytes or a file — queued when the template is built, and made by a search that needs it first if that comes sooner. When it becomes free it looks at every read waiting for it (or, when none is, at the next one to come, plus those queued in the 5 ms after it), and takes those of the highest priority as one batch: a read asked while the host dispatched a key, a hotkey, a controller event, a window coming forward or a focus change — a callback's or a handler's — goes before one asked from a poll or while a module loads, so a key's read is never one more in a batch of library polls; the rest wait for the next batch, weighed again with whatever came meanwhile. It opens the batch's templates first, each PNG once however many of its searches name it — a search whose template cannot be opened is answered without a capture — then captures each distinct region of the batch once (per [source](#which-picture-a-read-sees)) — a read given a [snapshot](#reading-a-snapshot) reads that and captures nothing, and a `profile` brings its picture with it — matches or reduces every read against its frame, and only then takes the next batch. So one slow capture or match holds up the reads of every module queued behind it — a read through desktop duplication waits up to 250 ms there before it gives up (see [Which picture a read sees](#which-picture-a-read-sees)). Callbacks run in the order their reads were answered — of one priority, the order they were queued. A module may have 64 reads with a callback waiting for the worker, and a newer one past that ends its oldest ([With a callback](#with-a-callback)); a handler waits for one read at a time. So a poll keeps its own count at one by asking with a `key` and not again while [`host.screen.pending(key)`](#host-screen-pending) says it is out, as the examples below do.

**The matching runs on a pool of threads**, one per logical processor, which the image worker hands each batch to once it has captured the batch's frames: the matches, the reductions, the copies and templates cut from a picture, and a shot's marks, PNG encode and file write all run there, every read of the batch at once, and the worker waits for the pool to finish them before it takes the next batch. Only the captures and the opening of the batch's templates run on the worker itself; wherever this page says other work runs on the image worker, it runs on this pool — the picture of a `template` from bytes or a file too, when its own task makes it. On Windows the worker and the pool's threads run **below normal priority** (`THREAD_PRIORITY_BELOW_NORMAL`), set as each thread starts: while an overlay polls, the matches can keep every processor busy several times a second, and at normal priority they would compete with every other program's normal-priority threads — among them other programs' low-level keyboard hooks, which Windows calls for every key and waits for. Below normal they take what the rest of the machine leaves, so a match is as fast as before on an otherwise idle machine and waits behind other programs' work on a busy one. The application's own keyboard hook runs on a thread of its own at the highest normal priority. On macOS both run at the default priority.

**Coordinates are device pixels on Windows and points on macOS.** The application declares per-monitor DPI awareness (`PerMonitorV2`), so on Windows every coordinate on this page — and on every other page — is one physical pixel of the display; on macOS it is one point. The two numbers agree only at 100 % scaling. A region measured with a tool that is **not** DPI-aware on a Windows display scaled above 100 % is in that tool's scaled units: multiply it by the scale factor (1.5 at 150 %) before using it here. See [`host.screen.size`](#host-screen-size).

**What a call answers when it cannot.** Every call below has a **Returns** and an **Errors** section ([Reading an Errors table](./errors.md#reading-an-errors-table)). When a read gets no picture, or its work fails, the call answers an error value, `{ code, message, detail? }`, after what it answers when it could not look: `nil`, `{}` for `imageSearchAll`, `false` for a shot. **No call on this page answers a value and an error value together**: an error value always means that nothing was read. Compare `err.code` with a constant, never the message: a message may change between versions, and the code does not. The reasons many calls share are written once, under [Failure reasons](#failure-reasons). Besides the raises each entry lists, any call here can raise two more: the capability gate, when the module did not declare `screen` ([What to declare](#declare)), and the guard's `stopped by the host: …`, when the module has run too long ([the limits](../module-runtime-and-lifecycle.md#limits)).

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["screen"]
```

See [what that list is and is not](./index.md#capabilities).

## Which picture a read sees {#which-picture-a-read-sees}

Every call on this page, and `host.ocr.recognize`, reads the screen one way for the whole module, chosen in its manifest rather than per call. Nothing needs declaring for the standard way, which is what every module gets by default. A module written for an application the standard way may not read correctly — a game whose picture it sees frozen or black — can ask for the other one:

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
    if host.screen.pending("menu") then return end   -- the last picture has not come yet
    -- The point a third of the way across the client area, wherever the window is, photographed
    -- off the event loop: the poll waits for nothing, and the module's keys go on meanwhile.
    host.screen.snapshot({ region = { window = win, fraction = { 0.32, 0.31, 0.32, 0.31 } }, key = "menu" },
      function(s, err)
        -- No picture this time: under fallback = "none", a minimised window, or a limit of the
        -- requests waiting (the module's 16, the application's 64); `err` says which.
        if not s then return end
        local c = host.screen.pixel(s.x, s.y, { snapshot = s })   -- its one pixel, at once
        s:release()
        local state = (c.r > 200 and c.g < 60) and "Start" or "Options"
        if state ~= lastState then
            lastState = state
            host.speech.output(state)
        end
    end)
end)
```

**The table.** `capture` and `fallback` are its two keys. Their values are matched without regard to case, so `"Duplication"` is `"duplication"`. A value this host does not know is named in the log and read as the safe choice: `standard` for `capture`, the standard fallback for `fallback`. A key it does not know — a misspelt `captur`, or one a later host adds — is ignored, and the log names it in one line with the module (`[capture] [com.example.game] [screen] has a key 'captur' this host does not know (it knows capture and fallback), so it is ignored; check the spelling`); the module still loads and reads as if the key were not there: with no `capture` of its own, as its runtime declares (see **Inherited from a runtime** below), or else the standard way. A value that is not a string — `capture = true`, `fallback = 0`, a list — fails the manifest, and the module does not load. The format is in [the manifest](../module-package-format.md#screen-and-why-it-is-declared-per-module).

**Inherited from a runtime.** A module that depends on a `code_module` which declares `[screen]` inherits the declaration unless it declares its own. The module's own table counts only when it has a `capture`: a table with nothing but `fallback = "none"` is ignored, with a log line saying so, and the inherited declaration applies with the dependency's fallback — so a game module that wants another fallback than its runtime's writes `capture` too. After the module's own table comes the nearest declaring dependency, then the earliest in manifest order. Only a dependency that is itself a `code_module` is consulted — a plain data dependency's table is never read — and its table counts only if that dependency requires `screen` or `ocr`. The log's `[capture]` lines say which source each module ended up with, and why.

**Opened ahead of time**, on Windows: the host *starts* the opening when one of the module's [`host.window.onTrigger`](./window.md#host-window-ontrigger) triggers fires, before its first matching callback runs (the overlay runtime registers one for every module that attaches an overlay), and before the report of a window already in front that `onTrigger(matcher, { initial = true }, cb)` makes. That opening covers the **primary monitor** only: a window on another monitor has its monitor's duplication opened by its first read. It is not started while duplication is open already, backing off after a failure, waiting for an overdue read or stopped (see Windows below).

**Nothing waits for that opening.** It runs in the background, and the callback runs at once. A read on the image worker ([`cells`](#host-screen-cells), [`matchCells`](#host-screen-matchcells), [`template{ capture }`](#host-screen-template), the searches, [`save`](#host-screen-save) and [`saveMarked`](#host-screen-savemarked) in a handler or given a callback, and [`imageSearchEach`](#host-screen-imagesearcheach)) or on the `screen-capture` thread ([`host.ocr.recognize`](./ocr.md#host-ocr-recognize), and [`snapshot`](#host-screen-snapshot), [`pixel`](#host-screen-pixel), [`pixels`](#host-screen-pixels) and [`profile`](#host-screen-profile) in a handler or given a callback) waits for it, up to 250 ms; an opening that takes longer — seconds, with a cold graphics driver — leaves that read without a picture, and makes duplication overdue until the opening ends (see How long a read waits, below). In the game measurement below, creating the device took 32 ms and the first picture came 4–16 ms after opening: well within a read's wait. The reads after the opening has finished read through duplication without paying for it.

**Without a head start** — a module that never uses `onTrigger`, or one that reads before any of its triggers has fired — the module's first read starts the opening, and waits for it at most 250 ms, as every read does. A read that gets no picture that way is read the standard way, or fails under `fallback = "none"` (see Windows below).

**With `fallback = "none"`, a read that duplication cannot answer fails, and says why**: it answers an error value in its own shape — `nil, err` from `pixel`, `profile`, `imageSearch` and `template{ capture = … }`, `false, err` from `save` and `saveMarked`, and the same for every other call — whose code is the reason's, one of the [duplication errors](#duplication-errors). That is for an application where the standard picture is wrong rather than slow: saying a frozen menu item is worse than saying nothing. With the default `fallback = "standard"`, such a read is quietly read the standard way instead, and nothing in the answer says which picture it was: under load, a poll can get standard pictures between live ones (see Windows below).

Poll only while your window is in front. A [`host.timer.every`](./timer.md#host-timer-every) poll runs until it is [cancelled](./timer.md#host-timer-cancel), so gate its callback: return at once unless `host.window.active()` is your window, as the example under [`imageSearchEach`](#host-screen-imagesearcheach) does.

### Windows {#which-picture-a-read-sees-windows}

`duplication` is DXGI Desktop Duplication: it reads the picture the compositor hands the display, where the standard path reads the screen device context. It is meant for an application whose picture the standard path reads frozen or black. **Measured with such a game** (the standard path 2026-09-21, duplication 2026-09-25; full screen at 1280×1024, on an AMD Radeon integrated graphics chip): the standard path read its always-moving picture as 40 identical pictures out of 40, while desktop duplication gave 40 and 39 different pictures out of 40 in two runs — it read the game live. A whole `profile` call over the 1280×1024 client area (`axes = "columns"`: the capture and its column reduction together) took 6–7 ms through duplication, the slowest 11 ms, where the same call through the standard path had taken about 23 ms; how the 6–7 ms divide between capture and reduction was not measured. Creating the Direct3D device took 32 ms, opening the duplication under a millisecond, and the first picture came 4–16 ms after opening; after 30 s without a read the duplication was closed, and the next read opened it again without trouble. Whether duplication sees another application's frames better than the standard path has to be checked with that application.

**The first-read comparison.** After a module that reads through duplication is loaded or reloaded, its first read is also made both ways, and a `[capture]` line naming the module says whether the two pictures are the same (`the two pictures are the same`, or `the two pictures DIFFER — …` with by how much). It compares the region read — for a text read, the first of its regions that has a rectangle at the call — when that has at least 4096 pixels (64×64), otherwise the foreground window's client area when that has, otherwise the region anyway. It costs one duplication read of its own plus, when that read was answered, one standard capture of about 17 ms, and where that is paid depends on the read. A read handed to a thread — [`host.ocr.recognize`](./ocr.md#host-ocr-recognize), and [`snapshot`](#host-screen-snapshot), [`pixel`](#host-screen-pixel), [`pixels`](#host-screen-pixels) and [`profile`](#host-screen-profile) in a handler or given a callback (these on the `screen-capture` thread), and [`cells`](#host-screen-cells), [`matchCells`](#host-screen-matchcells), [`template{ capture }`](#host-screen-template), [`imageSearch`](#host-screen-imagesearch), [`imageSearchAll`](#host-screen-imagesearchall), [`save`](#host-screen-save) and [`saveMarked`](#host-screen-savemarked) in a handler or given a callback, and [`imageSearchEach`](#host-screen-imagesearcheach) (these on the image worker) — carries it to that thread, which makes it just before the read's own capture: none of it is on the event loop, its duplication read waits at most 250 ms like any read there, and the read's picture or points, and whatever that thread captures next, wait for it. One read carries it at a time. A read that ends before its picture is taken — made stale by a newer one with its key, cancelled, ended with its module, or a text read that shares the picture of the same read asked just before it — leaves it to the module's next read. No call makes it on the event loop, which reads nothing of the screen: a call made where it cannot wait raises ([Where it waits](#where-it-waits)), and leaves it to the module's next read. Until duplication has answered it once, it is tried again at every read; while duplication never answers, no comparison line appears, and the `could not answer` lines say why.

**Cost.** A read returns the most recently composed frame without waiting for the next one. Nothing captures in the background between reads: each read asks Windows, without waiting, whether a new frame has been composed since the last one, copies it on the graphics card only if there is one, and otherwise reads the copy it already holds — which is then the current screen, since no new frame means nothing was drawn. A frame that carries only a mouse-pointer move does not replace that copy. Only the part of the picture a read asks for is copied off the graphics card. On an idle desktop (the reference machine: GTX 1060, 1920×1080, a test build) it measured about 0.3–1 ms for a region up to 633×418 against the standard path's 16 ms, and 5 ms for the whole screen against 28 ms, with the same bytes from both paths. With a game in front, the game measurement above has only whole calls: a `profile` of the 1280×1024 client area, capture and reduction together, in 6–11 ms. One run on the reference machine with a full-screen Direct3D game in front measured the capture alone at about 33 ms per read at every size. That the latest frame comes back at once also means a read made right after `host.input` may still see the frame from before the input, so re-read after a short wait, as you would anyway.

**How long a read waits.** Every read through duplication is made on the image worker or on the `screen-capture` thread — `host.ocr.recognize`, and the calls of this page in a handler or given a callback; the event loop reads none, since a call made where it [cannot wait](#where-it-waits) raises. A read waits at most **250 ms** for duplication's answer, an opening included, and holds every read queued behind it on that thread meanwhile. A read whose whole wait ran out makes duplication **overdue**: from then on no read asks it until that late answer has come back, and the log says so once when that starts and once when it ends. The reads that did not ask while it was overdue are counted in the observation line but not logged one by one. So under the default fallback a poll can get standard pictures between live ones without a line in the log; under `fallback = "none"` they fail with `an earlier read has not come back yet` (`busy`).

**When it cannot answer, and when it tries again.** Windows takes duplication away at a display mode change, a switch into or out of full screen (an Alt+Tab to or from a full-screen game can be one) and a desktop switch; stops it while a UAC prompt or the lock screen shows; lets no more than four programs of a session duplicate at once; and refuses it over Remote Desktop and on some laptops with two graphics chips. **The host reopens it by itself; nothing needs switching.** A read that finds the picture lost reopens the duplication at once, within the same read — which is what a full-screen switch needs — and only a loss again straight after counts as a failure. After a failure, duplication is left alone for a while, and the reads in that time get the standard picture, or fail under `fallback = "none"` with the reason:
- the picture lost, or the secure desktop showing: 250 ms, then 500 ms, 1 s and at most 2 s while it keeps failing; the first read that is answered starts the count again;
- four other programs already recording the screen, duplication not supported here, or a pixel format it cannot read: 5 s;
- any other failure of the graphics interface (`it failed with …`): 1 s;
- opened, but no picture arrived within 50 ms: 250 ms, with the duplication kept open;
- the graphics driver reset or updated: no wait; the next read creates the device afresh.

The log names each reason once, and again only after a read has been answered in between. For a laptop with two graphics chips the reason names the remedy: Windows Settings, System, Display, Graphics, this application, Power saving.

**Three hangs within ten minutes stop it, for a while.** A read still unanswered after 2 s, or an opening still running after 10 s, counts as a hang. Three hangs within ten minutes of each other stop duplication — hangs further apart (a driver reset one day, a slow resume the next) do not add up. While it is stopped every read gets the standard picture, or fails under `fallback = "none"` with `it stopped after three reads hung within ten minutes; …` (`unavailable`). It is tried again on a fresh thread after 10 minutes; each time it stops again the wait doubles, to 20 and 40 minutes and at most an hour. The doubling is forgiven — the next stop waits 10 minutes again, said once in the log — when a fresh thread has answered 100 reads in time without a hang, or when the switch below is turned off and on again. A resume, an unlock or a connection to the session resets all of that: the next read tries duplication again at once, with the hang count, any back-off above and the doubling forgotten, and the log says so once (`desktop duplication is asked again after the system event: …`). A display or scale change does the same except that it keeps the doubling — but at most once in ten minutes, and only when there are hangs from the last ten minutes to forget or duplication has stopped (the line then ends `(after a display or scale change, at most once in 10 minutes; the stops before still lengthen the next wait)`). Otherwise a display or scale change leaves the hang count as it is, and a back-off to end on its own: a display whose link keeps dropping — a TV or receiver in standby — sends one every few seconds, and the hangs that come with it must still add up to a stop. A thread still stuck inside the graphics driver when its time comes is not replaced — the retry waits another round, said once — so at most one such thread is ever left behind. Turning the Application settings switch below off and on again tries it at once as well.

**Opening it takes time.** The first read of a session creates a Direct3D device: 32 ms in the game measurement above, about 0.2 s on the reference machine, and 3.5–4.3 s in three runs of a freshly built test binary. A read waits for the opening up to its 250 ms; one that gets no picture by then reads the standard way, or fails under `fallback = "none"`, and the reads after it while the opening goes on do too; **Opened ahead of time** above says when the host starts the opening before the first read.

**Closed when idle.** A duplication that nothing has read for 30 seconds is closed, and the next read opens it again, paying the opening and up to 50 ms for its first picture. So a gated poll lets it go while the game is in the background, and an ungated one keeps it open all the time. The Direct3D device is kept for 5 minutes without a read, then released too.

- **The mouse pointer:** duplication reports it separately and this path does not draw it in, so, as with the standard path, it is normally not in the picture. Windows documents that a pointer can also be drawn into the desktop image itself, and then it is.
- **Regions touching a rotated monitor** are read the standard way (or fail under `fallback = "none"`).
- **More than 40 million pixels:** a region that large is refused in a module that reads through duplication, with either fallback (`too_large`, `screen capture failed: the region is larger than 40 million pixels`).
- **HDR:** Windows converts the picture to 8 bits per channel for this path, so its colours may differ from the standard path's. Cut templates with `host.screen.save` from a module that declares the same `capture`.
- **"Let modules that ask for it read the screen through the graphics card"** in the Application settings tab turns duplication off for every module: a module that allows the fallback reads the standard way, and every read of a `fallback = "none"` module fails with `it is switched off in Application settings` (`unavailable`). Turning it off and on again starts duplication afresh — after it stopped for three hangs, after its thread stopped, or while it is stuck inside the graphics driver.

### macOS {#which-picture-a-read-sees-macos}

`[screen]` is accepted and ignored: a module that declares it reads the screen exactly as one that does not, and none of the [duplication errors](#duplication-errors) occurs.

Every read goes to ScreenCaptureKit's rectangle capture where this macOS has it (15.2 and later), and otherwise to the older Core Graphics capture functions where this macOS still has them; a snapshot's `via` says which answered. A ScreenCaptureKit capture that has not answered within 1.5 s is given up — that read has waited the 1.5 s, on whichever thread asked — and ScreenCaptureKit is then not asked for **30 seconds**. After that it is asked once more, a **probe**, while the reads go on to the older functions: where this macOS has them, the probe is sent alongside a read they answer and nothing waits for it; on a macOS without them the probing read waits for it, up to 1.5 s, since nothing else could answer that read. A probe answered within 1.5 s — with a picture or an error — ends the wait; one that is not doubles it, up to **ten minutes**. The Mac waking, the screen being unlocked, this user's session becoming active again, the displays waking or the display configuration changing bring the next probe forward to that moment, once per wait and not sooner than 30 seconds after the timeout that began it; the wait itself goes on until a probe is answered. Meanwhile every read goes to the older functions, and on a macOS without them fails with `screen capture failed`. The log has one `macos` line when a wait starts or grows, one when a probe is brought forward, and one when the wait ends.

## Failure reasons {#failure-reasons}

When a read gets no picture, or its work fails, the call says why: it answers an **error value**, a read-only table `{ code, message, detail? }` ([`host.errors`](./errors.md)), after what it answers when it could not look.

- `nil, err` from `pixel`, `pixels`, `snapshot`, a snapshot's `crop`, `profile`, `template`, `imageSearch`, `cells` and `matchCells`. An `imageSearch` that looked and found nothing answers `nil` alone.
- `{}, err` from `imageSearchAll`. The list is never `nil`; an empty list alone means it looked and found nothing.
- `false, err` from `save` and `saveMarked`.
- `cb(nil, err, info)` from `imageSearchEach`.
- **Given a callback**, every call above hands its callback the same two values, then `info`: `cb(nil, err, info)`, `cb({}, err, info)` for `imageSearchAll`, `cb(false, err, info)` for a shot ([With a callback](#with-a-callback)).
- `host.ocr.recognize`: a `"failed"` reading whose `error` is the error value ([its page](./ocr.md#host-ocr-recognize)).

No call on this page answers a value and an error value together: when an error value comes, nothing was read.

A module compares the value's **code**, never its message: `err.code == host.errors.UNAVAILABLE`. The message is a sentence for a person — log it, or say it where silence would leave the user guessing; `tostring(err)` and `"…" .. err` give it. In a message below, `…` stands for a number, a size, a path or the system's own words. A message may change between versions; the code does not.

Each call's **Errors** table lists every error value it can give. The reasons many calls share are written once, here, in six groups, each reason with its code. A call's table links to the capture, duplication and snapshot-source groups, and lists the reasons of the other three as rows of its own, where its module is most likely to test them. Every call a group names gives every reason of it; a reason only some of them give is a row of their own tables.

### Capture errors {#capture-errors}

A picture of the screen that could not be taken. Any call of this page that captures a region gives it: `snapshot`, `profile`, `cells`, `matchCells`, `template{ capture }`, the three searches, `save` and `saveMarked` — none of them given a snapshot — and [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) gives it too, at its entry. A region of more than 40 million pixels, in a module that reads through desktop duplication, is `too_large` with `screen capture failed: the region is larger than 40 million pixels`: a row of the searches', `save`'s and `saveMarked`'s own, since every other call holds its region to that or less before it captures — a snapshot and a profile to 40 million, `cells` and `matchCells` to 40 million, `template{ capture }` to 1,048,576, a text read to 40 million in all. The `screen-capture` thread, which takes the pictures of `snapshot`, `profile` and a text read, can also fail inside the host — `the screen capture failed inside the application; the log has the details`, and for a `snapshot` or a `profile` after its round's capture `the snapshot failed inside the application; the log has the details` — which those entries list; the image worker's calls answer their own `internal error: …` instead. `pixel` and `pixels` have their own, at their entries.

| code | when | message | platform |
|---|---|---|---|
| `failed` | The standard path could not read the region: it was empty, too large to allocate for, or the system refused the read. On macOS, every failed capture, a region wholly off the desktop included. In the build for other systems, every capture. | `screen capture failed` | both, other |

### Duplication errors {#duplication-errors}

A read that desktop duplication could not answer, in a module that declares `[screen] capture = "duplication"` with `fallback = "none"`. Windows only. Every call that reads the screen gives these, `pixel` and `pixels` included, and so does [`host.ocr.recognize`](./ocr.md#host-ocr-recognize). The message is the whole sentence `screen capture failed: desktop duplication could not answer — ` and one of the tails below; the code is the tail's. [Which picture a read sees](#which-picture-a-read-sees-windows) says what each means and when duplication is tried again. On macOS none of them occurs.

- `it is switched off in Application settings` — `unavailable`
- `it stopped after three reads hung within ten minutes; it is tried again after a while, after a resume, an unlock or a display change, or when it is switched off and on again in Application settings` — `unavailable`
- `its thread stopped on an internal error; switching it off and on again in Application settings starts it afresh` — `failed`
- `its thread could not be started` — `failed`
- `the application is closing` — `cancelled`
- `an earlier read has not come back yet` — `busy`
- `Windows does not support it here (0x…): over Remote Desktop, or on a laptop with two graphics chips where this application runs on the one that does not drive the display — Windows Settings, System, Display, Graphics, this application, Power saving` — `not_supported`
- `four other programs already record the screen — a screen recorder, a screen-sharing call, another reader` — `unavailable`
- `the secure desktop is showing (a UAC prompt or the lock screen)` — `denied`
- `the picture was lost to a display mode change, a fullscreen switch or a desktop switch` — `failed`
- `the graphics driver was reset or updated` — `failed`
- `the region touches a rotated monitor, which this version reads only the standard way` — `not_supported`
- `the region touches a monitor desktop duplication does not report` — `not_supported`
- `no picture has arrived since it was opened` — `not_ready`
- `no display output was found` — `not_supported`
- `the display delivers pixel format …, which this version cannot read` — `not_supported`
- `it failed with 0x…` — `failed`: any other failure, with its error code.

A region of more than 40 million pixels is told so before duplication is asked, with either fallback: `too_large`, `screen capture failed: the region is larger than 40 million pixels`, which only the searches, `save` and `saveMarked` can meet ([Capture errors](#capture-errors)).

### Window-form errors {#window-form-errors}

A [window region](#region-form) or window point with nothing to read this time. Any call given one gives these: `pixel`, `pixels`, `snapshot` and each region of its `change.watch`, a snapshot's `crop`, `profile`, `cells`, `matchCells`, `template{ capture }`, the three searches, `save`, `saveMarked` and each region of [`host.ocr.recognize`](./ocr.md#host-ocr-recognize). An `imageSearchEach` entry whose `within` has none answers `false` in its slot instead. Nothing is captured for them. A call's own table lists both.

| code | when | message | platform |
|---|---|---|---|
| `hidden` | The window's client area has no width or no height: a minimised window, or one not laid out yet. | `the window's client area is empty (…x…)` | both |
| `outside` | The rectangle worked out from the window table lies outside the 32-bit coordinate range. A window table from `host.window` never does. | `the region lies outside the screen's coordinate range` | both |

### Snapshot-source errors {#snapshot-source-errors}

A region that misses the snapshot a call was given as `opts.snapshot`, by the call's rule ([Reading a snapshot](#reading-a-snapshot)). Nothing is captured for it. The calls that read the part of their region inside the snapshot give this one: `profile`, the three searches, `save`, `saveMarked`, and a region of [`host.ocr.recognize`](./ocr.md#host-ocr-recognize).

| code | when | message | platform |
|---|---|---|---|
| `outside` | No part of the region is inside the snapshot. | `the region does not overlap the snapshot` | both |

The calls that need their region wholly inside the snapshot — `template{ capture = …, snapshot = … }`, `cells`, `matchCells` and a snapshot's [`crop`](#snapshot-crop) — answer `outside`, `the region is not inside the snapshot`, instead, each at its own entry. A call gives the one its rule gives, never the other. `pixel` and `pixels` answer `the point is not inside the snapshot`, at theirs.

### Request-limit errors {#request-limit-errors}

What a request of the `screen-capture` thread can be answered instead of its picture or points: `pixel`, `pixels`, `snapshot` and `profile`, waited for in a handler or given a callback. A `snapshot`'s change wait has two more limits of its own, at [its entry](#host-screen-snapshot-errors). A call's own table lists each of these.

| code | when | message | platform |
|---|---|---|---|
| `too_many` | The module had 16 requests waiting and asked for one more: its oldest is ended with this, and the new one goes ahead. | `too many snapshot requests waiting for this module (16); the oldest was ended` | both |
| `too_many` | 64 requests wait in the whole application, counted once the module's own oldest has ended: the new one is not taken, and ends none of its module's. | `too many snapshot requests waiting in the whole application (64); this one was not taken` | both |
| `replaced` | Given a callback and `opts.key`: the module asked the same call again with that key ([With a callback](#with-a-callback)). | `a newer request with the same key replaced this one` | both |
| `cancelled` | The capture thread stopped a request the event loop still waited for, which no path of the host does. Listed so that no reason is left out. | `the request was cancelled` | both |

### Image-worker errors {#image-worker-errors}

What a call of the image worker given a callback can be answered instead of its work: `cells`, `matchCells`, `template`, a snapshot's `crop`, the three searches, `save` and `saveMarked`. A handler waits for one read at a time, so these never reach it. A call's own table lists each of these.

| code | when | message | platform |
|---|---|---|---|
| `too_many` | The module had 64 of these calls with a callback waiting for their answer, together, and asked for one more: its oldest is answered with this on the next tick, and the newest goes ahead. A task not started yet is not captured or matched at all. Logged once, then counted, with a line a minute at most while it goes on. | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | With `opts.key`: the module asked the same call again with that key ([With a callback](#with-a-callback)). A snapshot's `crop` takes no key. | `a newer request with the same key replaced this one` | both |

## Where it waits {#where-it-waits}

**[`snapshot`](#host-screen-snapshot), [`pixel`](#host-screen-pixel), [`pixels`](#host-screen-pixels), [`profile`](#host-screen-profile), [`cells`](#host-screen-cells), [`matchCells`](#host-screen-matchcells), [`template{ capture }`](#host-screen-template), a snapshot's [`crop`](#snapshot-crop), [`imageSearch`](#host-screen-imagesearch), [`imageSearchAll`](#host-screen-imagesearchall), [`save`](#host-screen-save) and [`saveMarked`](#host-screen-savemarked) without a callback wait in a handler for the screen, or for the work over a region, and only their module waits meanwhile.** Given a callback they wait nowhere ([With a callback](#with-a-callback)); this section is about the calls without one. A handler is the function the host called for one of the module's events — a hotkey, a captured key, a controller event, a timer, a window trigger, a focus change, the answer of a read, an image search, a snapshot or a profile, a setting's `onChange` called for the settings dialog or for another module's `set` — and anything that function calls, a `pcall` included ([Each callback is a handler](../module-runtime-and-lifecycle.md#handlers)). There the call checks its arguments at the call, as it always has, and has its work done on a thread of the host's own, as the same call given a callback has it done: `snapshot` its picture taken on the `screen-capture` thread, a plain one; `pixel` and `pixels` their points read there; `profile` its picture taken there — none for a snapshot — and reduced on the image worker; `cells` and `matchCells` their region read and reduced, and matched, on the image worker; `template{ capture }` its region captured there, or cut from its snapshot, and made a template; `crop` its part of the snapshot copied there; the two searches their templates opened there, their region captured — or read where it lies in their snapshot — and matched; `save` and `saveMarked` their region captured there — or the part of their snapshot it covers copied — marked, encoded and written to their file. The handler stops at the call until the answer is there. Only its own module waits: its next events wait in its mailbox, in order — keys up to 256, a held key's repeats folded into one — and every other module, speech and the event tap go on. The handler then goes on with what the call returns: the handle — its `time` and `inputEpoch` those of the picture, taken on the thread — the colour or the list of colours, the profile — with its picture's `time` and `inputEpoch` — the grid or the match, the template, the crop, the hit or the list of hits, `true`, or the call's own could-not-look answer — `nil` and an error value, but `{}` and an error value for `imageSearchAll`, and `false` and an error value for a shot, a file that could not be written included. The waiting counts on no clock of the [limits](../module-runtime-and-lifecycle.md#limits). The same holds on Windows and macOS. A call that reads no screen and walks no region answers at once wherever it is called, where it cannot wait too: `pixel` and `pixels` given a snapshot, a window point with no pixel this time (a minimised window's) and an empty list; any of these calls but `snapshot` when a window region has no rectangle this time, a `profile` of an empty region or of a region outside its snapshot, `cells` or `matchCells` of a region not wholly inside theirs, a `crop` or a `template{ capture }` not wholly inside its snapshot, a search, `save` or `saveMarked` of a region with no part inside its snapshot, and a `template` from bytes or a file. Every other call on this page runs on the event loop wherever it is called, as its **Cost** says.

| Call | In a handler | Where it cannot wait |
| --- | --- | --- |
| `snapshot` | waits: its picture is taken on the `screen-capture` thread, its charge reserved at the call, as one of the module's 16 snapshot requests, a 17th ending the oldest; with 64 waiting in the whole application it answers `nil, err` (`too_many`, [Request-limit errors](#request-limit-errors)) | raises, whatever it was given (below) |
| `pixel`, `pixels` | waits: its points are read on the `screen-capture` thread, as one of the module's 16 snapshot requests with no picture and no charge; with 64 waiting in the whole application it answers `nil, err` (`too_many`). Given a snapshot: at once | raises (below). Given a snapshot: at once, as in a handler |
| `profile` | waits: its picture is taken on the `screen-capture` thread — none for a snapshot — and reduced on the image worker, as one of the module's 16 snapshot requests, its picture charged from the call until the answer; with 64 waiting in the whole application it answers `nil, err` (`too_many`) | raises (below) |
| `cells`, `matchCells` | waits: its region is read — captured through the module's source, or where it lies in its snapshot — reduced and, for `matchCells`, ranked on the image worker; the states are checked and decoded at the call | raises |
| `template{ capture }` | waits: its region is captured on the image worker, or cut from its snapshot there, and made a template; its charge is reserved at the call. With bytes or a file, or a region not wholly inside its snapshot: at once, the picture of bytes or a file then made on the image worker | raises. With bytes or a file, or a region not wholly inside its snapshot: at once, as in a handler |
| a snapshot's `crop` | waits: its part is copied on the image worker, its charge reserved at the call | raises, as `Snapshot:crop` |
| `imageSearch`, `imageSearchAll` | waits: its templates are opened, and its region captured through the module's source — or read where it lies in its snapshot — and matched, on the image worker; a template path is checked at the call, by its header, the first time it is named. A handler waits for one search at a time, which is not one of the module's 64 image searches | raises |
| `save`, `saveMarked` | waits: its region is captured on the image worker — or the part of its snapshot it covers copied — and marked, encoded and written there; a file that cannot be written is answered `false, err` after the wait | raises; nothing is written |
| every other call on this page | on the event loop, as its **Cost** says | the same |

**Where it cannot wait**, the call without a callback raises, at the line that called it — unless it touches nothing it would wait for (above), which is answered at once there too. Nothing is captured, no region is worked over and no file is written, and the event loop is never held for the screen. A `pcall` around the call returns the message. Those places are: outside every handler — a module's top level, an included file's, a function the host calls between the module's events; where Luau cannot stop the function — a metamethod, a `table.sort` comparator, a `string.gsub` function, the iterator of a `for` loop, an `xpcall` error handler; a function the host called and waits for — an arbiter's `onActivate` or `onDeactivate`, an `onChange` fired by the module's own `host.settings.set`, the top level of a file included from a handler; and a coroutine of the module's own, or one the [overlay runtime](./overlay.md#o-new) made for a hook that must not wait. The same holds on Windows and macOS, and for [`host.ocr.recognize`](./ocr.md#where-it-waits). The message names the call, the place and what to write there; `pixel`'s three:

```
host.screen.pixel cannot wait here: this runs outside every event of the module — its top level, an included file's, or a function the host calls between its events. Call it from a handler, such as host.timer.after(0, …), or pass a callback, host.screen.pixel(x, y, opts, function(colour, err) … end)
```

```
host.screen.pixel cannot wait here: this function runs inside one the host calls and waits for, or one Luau cannot suspend — an arbiter's onActivate or onDeactivate, an onChange fired by your own host.settings.set, the top level of an included file, a metamethod, a table.sort comparator, a string.gsub function, the iterator of a for loop, an xpcall error handler. Pass a callback, host.screen.pixel(x, y, opts, function(colour, err) … end), or make the call in the event's own function
```

```
host.screen.pixel cannot wait in a coroutine the module made, or one the overlay runtime made for a hook that must not read (when, present, menu tests, an identify asked on every evaluation): only the host's own coroutine for an event waits. In a hook, measure it in a handler and let the hook read what was measured (see "Where a hook runs" in docs/api/overlay.md); in a coroutine of your own, pass a callback, host.screen.pixel(x, y, opts, function(colour, err) … end), or call it from the event's own function
```

Every call's message names the call and its own callback form — `host.screen.profile cannot wait here: … pass a callback, host.screen.profile(opts, function(profile, err) … end)`, and `Snapshot:crop` for a snapshot's `crop`, with `s:crop(region, function(crop, err) … end)`. A pixel an overlay's `when` needs is a [probe](./overlay.md#o-probe)'s. The calls that can be answered at once read their arguments first, so a mistake in them raises as the mistake; `snapshot`, which always waits, raises the place's message whatever it was given.

```luau
-- A module's top level cannot wait: there a call without a callback raises, and the picture is
-- taken in a handler — here a timer's, at once — or given a callback. Needs "screen" and "timer"
-- in the manifest.
local ok, msg = pcall(host.screen.pixel, 0, 0)
host.log.info(tostring(ok) .. " " .. msg)   -- false host.screen.pixel cannot wait here: …
host.screen.pixel(0, 0, nil, function(c, err)   -- with a callback: anywhere
  host.log.info(c and ("the corner at load: " .. c.hex) or ("no corner: " .. err))
end)
host.timer.after(0, function()
  local s, err = host.screen.snapshot({ region = { 0, 0, 200, 40 } })   -- waits here
  if not s then
    host.log.info("no picture: " .. err)
    return
  end
  local corner = host.screen.pixel(0, 0, { snapshot = s })   -- of the snapshot: at once
  host.log.info("taken " .. (host.now() - s.time) .. " ms ago: " .. tostring(s) .. ", corner " .. corner.hex)
  s:release()
  local live = host.screen.pixel(0, 0)   -- of the screen: waits here
  if live then host.log.info("the corner now: " .. live.hex) end
end)
```

**A poll that waits holds its module's keys.** A `host.timer.every` or `after` handler that waits for the screen keeps the module's keys waiting meanwhile: the capture, those ahead of it on its thread, the work on the image worker and the batches ahead of it there, and the turn of the event loop that hands the answer over. A poll passes its call a callback, and keeps one request out with a `key` ([With a callback](#with-a-callback)): one that takes a picture on every turn passes `snapshot` a callback, and one that reads several things of one moment reads them from that picture in its callback (`{ snapshot = s }`). The log says so, once per module and place in a session, when a timer's handler waited while keys queued behind it:

`[com.example.game] a timer waited 63 ms for host.screen.snapshot at com.example.game/src/main.luau:41; 3 key(s) (Tab ×2, Down) waited behind it — a poll should not wait: host.screen.snapshot(opts, function(snap, err) … end)`

`[com.example.game] a timer waited 24 ms for host.screen.pixels at com.example.game/src/main.luau:30; 2 key(s) (Tab ×2) waited behind it — a poll should not wait: host.screen.pixels(points, opts, function(colours, err) … end)`

A handler that has kept its module busy for 30 seconds or more with keys waiting behind it is said once, as for a read ([Where it waits](./ocr.md#where-it-waits)).

**After a wait, the world may have moved.** While a handler waits, the user can bring another window to the front, and a click made after the answer lands there. An overlay checks with [`O:here()`](./overlay.md#o-here) before the call and [`O:stillHere`](./overlay.md#o-stillhere) after it; other code compares `host.window.active()` before and after.

**A wait turns no epoch.** The handler goes on in the dispatch that asked, so the turn of the event loop that hands it its answer leaves [`host.epoch()`](./timer.md#host-epoch) where it was: what the handler kept for the epoch before the call still holds after it, unless another event turned the epoch meanwhile.

```luau
-- A key that reads the menu's lamps from a picture it waits for, then clicks the lit entry —
-- only while the overlay is still where the key was pressed. Needs "screen" and "input"; `o` is
-- an overlay of the overlay runtime.
o:addCustomButton({ label = "Lit entry", hotkey = "Alt+L", onActivate = function(self)
  local mark = self:here()
  local s = host.screen.snapshot({ region = { 96, 90, 544, 400 } })   -- waits here
  if not s then return end
  local lamps = host.screen.pixels({ { 120, 140 }, { 120, 172 } }, { snapshot = s })
  s:release()
  if not self:stillHere(mark) then return end   -- the user moved on while it waited
  if lamps and lamps[2].r > 200 then host.input.click(120, 172) end
end })
```

**The image worker's waits** (`cells`, `matchCells`, `template{ capture }`, `crop`, the searches, `save`, `saveMarked`, and `profile`'s reduction) wait for the batch the worker is in the middle of — which may be a large match — and then for the reads of a higher priority: a key's read is a batch of its own, after the one in progress and ahead of the polls queued before it (see the top of this page). A handler waits for one read at a time, so its reads are not counted among the module's 64 image searches ([Image-worker errors](#image-worker-errors)).

### Windows {#where-it-waits-windows}

The waited picture is taken through the module's [source](#which-picture-a-read-sees) on the `screen-capture` thread, by the standard path or desktop duplication, about one compositor frame through the standard path. The waited points are read there: `pixel`'s point with `GetPixel`, `pixels`' points with the captures [`pixels`](#host-screen-pixels) plans, or through duplication as a piece of one request. Through desktop duplication that thread waits up to 250 ms for its answer; the wait holds the module, not the event loop. The image worker captures `cells`', `matchCells`', `template{ capture }`'s, the searches', `save`'s and `saveMarked`'s regions through the same source, and waits for duplication up to 250 ms likewise. The module's [first-read comparison](#which-picture-a-read-sees), when it is owed, goes with the request and is made on the thread, before its capture. Where it cannot wait the call raises, and nothing reads the screen on the event loop: the comparison is left to the module's next read.

### macOS {#where-it-waits-macos}

The waited picture is taken by ScreenCaptureKit on the `screen-capture` thread, at the display's own resolution and drawn down to points, as a snapshot's given a callback is — a `profile`'s too, so a waited profile's colours are a snapshot's. The waited points are captured there at point resolution — never at the display's own resolution, so a colour may differ a little from the same point of a snapshot; one ScreenCaptureKit capture for each rectangle `pixels` plans (one for a single `pixel`), measured at 36 to 91 ms each. The image worker captures `cells`', `matchCells`', `template{ capture }`'s, the searches', `save`'s and `saveMarked`'s regions at point resolution. Where it cannot wait the call raises, and nothing reads the screen on the event loop.

## With a callback {#with-a-callback}

**Every call that waits takes a callback as its last argument** — `snapshot(opts, cb)`, `pixel(x, y, opts, cb)` or `pixel(point, opts, cb)`, `pixels(points, opts, cb)`, `profile(opts, cb)`, `cells(opts, cb)`, `matchCells(opts, states, cb)`, `template(spec, cb)`, `s:crop(region, cb)`, `imageSearch(template, opts, cb)`, `imageSearchAll(template, opts, cb)`, `save(path, opts, cb)` and `saveMarked(path, opts, cb)` — and then returns at once, nothing, wherever it is called: at a module's top level, in a coroutine, in a hook and in a handler alike. It never waits and never holds the event loop for its work: the work is done on the thread that does it for the call without a callback ([Where it waits](#where-it-waits)), and only the answer's tables are built on the event loop, as the callback runs. A `nil` where the callback goes is no callback: the call is the one without, and nothing in this section applies to it. [`imageSearchEach`](#host-screen-imagesearcheach) always takes its callback, and [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) takes one the same way.

**The callback** runs exactly once, on the event loop, on a later turn after the answer came (every 15 ms), as a [handler](../module-runtime-and-lifecycle.md#handlers) of its module: at once while the module is free, after the events before it in its mailbox while it is busy. It runs with the priority of the dispatch that asked (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize), "Order and priority"). It is called with what the call answers in a handler — its first value, then its error value, `nil` when there is none, so the error is always the second — and then `info`, a table: `cb(value, err, info)`. `info.time` is [`host.now()`](./timer.md#host-now) when the picture the answer was read from was taken, and `info.inputEpoch` [`host.inputEpoch()`](./timer.md#host-inputepoch) once it had been: a live read's when its capture came back on its thread, a snapshot's own for a read of a snapshot and for a `crop`. Both are absent when no picture was read — nothing to read this time, a capture that failed, a limit. `snapshot`'s `info` also says how long its picture took and how many were taken ([`snapshot` with a callback](#snapshot-with-a-callback)). An answer known at the call — a snapshot's points, a template from bytes or a file, a window region with no rectangle this time — comes on the next tick all the same, never from inside the call.

**Dropped, never called**, when the module is disabled, reloaded, removed or [stopped by the guard](../module-runtime-and-lifecycle.md#limits) before its callback runs, wherever the request is then: a picture not taken yet is not taken, and the image worker skips work it has not begun — a shot it has not written yet is not written. At exit, requests still waiting are dropped. A callback that raises is reported like any other module callback error; a module whose VM runs out of memory while the answer is built in it is [stopped](../module-runtime-and-lifecycle.md#limits), as for a text read's reading.

**`opts.key`**, a non-empty string, only the callback form takes: the call without a callback raises for it (`host.screen.pixels: opts.key is only taken with a callback (the third argument)`). A newer request of the same module VM with the same key, of the same call, answers every older one still out on the next tick — also one whose answer waits in the module's mailbox, judged as it runs — with the call's own could-not-look answer and the error value `a newer request with the same key replaced this one` (`replaced`): `nil` and it, but `{}` and it for `imageSearchAll`, and `false` and it for `save` and `saveMarked`. The image worker skips one it has not begun. Keys are each call's own: an `imageSearch` with a key never replaces a `matchCells` with it, nor `pixels` a `snapshot`. [`host.screen.pending(key)`](#host-screen-pending) says whether one is out. The key goes in `opts` of every call that has one, and in the spec of `template`; a snapshot's `crop` takes none. Without a key a request is never replaced.

**What raises**, at the call: everything the call raises without a callback, a callback that is neither a function nor `nil` (`host.screen.imageSearch: the callback (third argument) must be a function, got 5`), and a `key` that is not a non-empty string. Nothing that happens at run time raises: it is answered.

**Limits.** `snapshot`, `pixel`, `pixels` and `profile` given a callback are requests of the `screen-capture` thread, under its limits: 16 per module VM and 64 in the application, the module's oldest ended or the newest refused, with the error values under [Request-limit errors](#request-limit-errors); an answer known at the call is none of them. The image worker's — `cells`, `matchCells`, `template{ capture }`, `crop`, the searches, `save`, `saveMarked` and `imageSearchEach` — may be 64 per module VM, together; one more ends the module's oldest, answered on the next tick with its call's own could-not-look answer, as a replaced one is, and the error value `too_many`, `too many image searches waiting for this module (64); the oldest was ended` ([Image-worker errors](#image-worker-errors)). A poll keeps one out at a time with its key and `pending`, as the example does.

```luau
-- A library's landmark, searched twice a second off the event loop, one search out at a time.
-- After the module was off, whose answers are dropped, the key is no longer out and the next
-- poll asks again. Needs "screen", "timer", "window" and "log" in the manifest.
local found = false
host.timer.every(500, function()
  local w = host.window.active()
  if not w or host.screen.pending("landmark") then return end
  host.screen.imageSearch("images/landmark.png",
    { region = { window = w, fraction = { 0, 0, 1, 0.2 } }, tolerance = 8, key = "landmark" },
    function(hit, err, info)
      if err then return end   -- could not look: keep what was known
      if (hit ~= nil) ~= found then
        found = hit ~= nil
        host.log.info((found and "found" or "lost") .. ", read " .. (host.now() - info.time) .. " ms ago")
      end
    end)
end)
```

### Windows {#with-a-callback-windows}

The work is the call's own, on the threads that do it without a callback, through the module's [source](#which-picture-a-read-sees). `info.time` is when the capture came back on that thread; through desktop duplication the picture is the most recently composed frame, which may be a little older.

### macOS {#with-a-callback-macos}

The work is the call's own, on the threads that do it without a callback. `info.time` is when ScreenCaptureKit's capture came back on that thread: for a pixel read, measured at 36 to 91 ms after it was asked ([`pixel`](#host-screen-pixel)).

## host.screen.pixel(x, y, opts?, cb?) {#host-screen-pixel}

**Signature:** `host.screen.pixel(x: number, y: number, opts: { snapshot: Snapshot? }?) -> (Colour?, Error?)`, or `host.screen.pixel(point: { window: Window, fraction: { number, number } }, opts: { snapshot: Snapshot? }?) -> (Colour?, Error?)`, where `Colour = { r: number, g: number, b: number, hex: string }`; with a callback `host.screen.pixel(x, y, opts: { snapshot: Snapshot?, key: string? }?, cb: (colour: Colour?, err: Error?, info: Info) -> ()) -> ()`, or `host.screen.pixel(point, opts, cb)`, where `Info = { time: number?, inputEpoch: number? }`

Reads the colour of one screen pixel: at the screen coordinates `(x, y)`, or at a point given as fractions of a window's client area. The colour has the 8-bit channels `r`, `g`, `b` (0–255) plus `hex`, an uppercase `#RRGGBB` string. When there is nothing to read this time it answers `nil` and an error value instead ([Returns](#host-screen-pixel-returns)): for the window form when the window's client area is empty, as a minimised window's is; with a snapshot when the point is not in it; for a limit of the requests waiting; and in a module that declared `fallback = "none"` when desktop duplication had no picture ([Which picture a read sees](#which-picture-a-read-sees)). Each is under [Errors](#host-screen-pixel-errors). Otherwise it always answers a colour.

**`opts.snapshot`** reads the pixel from a [snapshot](#host-screen-snapshot) instead of the screen: no screen is touched, and it costs microseconds. `opts` is read strictly: `nil`, or a table whose only key is `snapshot` — and, with a callback, `key`; anything else raises (`host.screen.pixel: opts has a key 'snap'; it takes snapshot`), as does a `snapshot` that is not a Snapshot or was released, and a `key` without a callback (`host.screen.pixel: opts.key is only taken with a callback (the argument after opts)`). The options are read before anything is answered, so a mistake in them raises even when the window form has no pixel this time.

**The window form**, `{ window = w, fraction = { fx, fy } }` (or `fraction = { x = fx, y = fy }`), reads the pixel a [window region](#region-form) starting at `fx, fy` starts with. With `W` and `H` the client area's width and height, the point is `x = client.x + clamp(floor(W * fx), 0, W - 1)` and `y = client.y + clamp(floor(H * fy), 0, H - 1)`, in IEEE doubles as the region form computes. So the point never leaves the client area: a fraction below 0 reads the first column or row, and `1` or more the last. Only `window.client` is read, with no call to the operating system — see the [Region form](#region-form) for what that means for a window table cached for the epoch. The form is read strictly and raises, naming what is wrong (`host.screen.pixel: point.fraction.y is missing; both are needed, x and y`): a key beside `window` and `fraction`, a `fraction` that is not exactly two numbers written one way, a fraction that is not a finite number, a `window` without a `client` of four whole numbers, a table that is not the window form (`{ 100, 200 }` — screen coordinates are two numbers), and a third argument after the table that is not a callback (its options come second, its callback third). `pixel(x, y)` reads its two numbers as it always has: a fraction is cut toward zero, a numeral string is accepted, and anything else raises, a number outside the 32-bit coordinate range included (`host.screen.pixel: x must be a number within the coordinate range (-2147483648 to 2147483647), got 10000000000`).

**Without a snapshot a call is a screen read.** In a [handler](#where-it-waits) it waits: the point is read on the `screen-capture` thread, and only the module waits — its keys queue behind it — while the event loop goes on. Where it cannot wait it raises, and nothing is read ([Where it waits](#where-it-waits)). **Given a callback** — after the options, `pixel(x, y, opts, cb)` or `pixel(point, opts, cb)`, `opts` `nil` when there are none — it waits nowhere: the point is read on that thread all the same, and `cb(colour, err, info)` is called on a later tick, `info` with the moment of the read; a snapshot's pixel comes on the next tick, with the snapshot's moment ([With a callback](#with-a-callback)). It is not the cheap point lookup of a reader that keeps a frame: every call reads anew, and nothing is kept between two calls. Where a poll needs several points, read them with one [`pixels`](#host-screen-pixels), or take one [`snapshot`](#host-screen-snapshot) and read every point from it; for a band of points, one [`profile`](#host-screen-profile) — each one capture, whatever it tests.

**Cost.** In a handler the module waits, from asking to the answer, for the read — one compositor frame on the standard path — the captures ahead of it on the `screen-capture` thread and the turn of the event loop that hands the colour over: about 17 to 32 ms on Windows; the event loop spends only what reading the arguments and building the colour cost, as it does given a callback, which nothing waits for. Where it cannot wait it raises at once, and costs microseconds. Given a snapshot, or a window point with no pixel this time, it answers at once and costs microseconds.

```luau
-- A module's top level cannot wait, so the pixels are read in a handler: here a hotkey's, where
-- each read waits and only this module waits. Needs "screen", "hotkey" and "window" in the manifest.
host.hotkey.register("Ctrl+Alt+Shift+P", function()
    local c, err = host.screen.pixel(100, 200)   -- waits here
    if c == nil then   -- under fallback = "none", or with 64 requests waiting in the whole application
        host.log.info("not read: " .. err)
        return
    end
    if c.hex == "#FF0000" then
        host.log.info("red pixel")
    end
    host.log.info(string.format("rgb(%d,%d,%d)", c.r, c.g, c.b))

    -- The same question about a window, wherever it is and however large: the centre of its
    -- client area's top tenth.
    local w = host.window.active()
    if w then
        local top, topErr = host.screen.pixel({ window = w, fraction = { 0.5, 0.05 } })   -- waits here
        if topErr and topErr.code == host.errors.HIDDEN then return end   -- minimised: read it later
        host.log.info(top and ("top: " .. top.hex) or ("top not read: " .. topErr))
    end
end)
```

```luau
-- A key that reads a lamp, then clicks it — only while the overlay is still where the key was
-- pressed: the pixel waits in the key's handler, and the user can move on meanwhile. Needs
-- "screen" and "input"; `o` is an overlay of the overlay runtime.
o:addCustomButton({ label = "Lamp", hotkey = "Alt+L", onActivate = function(self)
  local mark = self:here()
  local c = host.screen.pixel(120, 172)   -- waits here
  if not self:stillHere(mark) then return end   -- another window came to the front meanwhile
  if c and c.r < 100 then host.input.click(120, 172) end   -- dark: switch it on
end })
```

```luau
-- A lamp watched by a poll that waits for nothing: read with a callback, one read out at a time,
-- and said when it changes. Needs "screen", "timer" and "speech" in the manifest.
local lit = nil
host.timer.every(200, function()
  if host.screen.pending("lamp") then return end
  host.screen.pixel(120, 172, { key = "lamp" }, function(c, err)
    if not c then return end   -- not read this time; `err` says why
    if (c.r > 200) ~= lit then
      lit = c.r > 200
      host.speech.output(lit and "lamp on" or "lamp off")
    end
  end)
end)
```

### Returns {#host-screen-pixel-returns}

- A `Colour`, `{ r, g, b, hex }` — one value, and nothing after it. A point on no monitor reads black, `(0, 0, 0)`; so does, on a Mac, a point that cannot be read ([macOS](#host-screen-pixel-macos)).
- `nil, err` — there was nothing to read this time; `err.code` is one of the codes under [Errors](#host-screen-pixel-errors).
- With a callback: nothing at once; `cb(colour, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#host-screen-pixel-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | The window form, and the window's client area is empty: a minimised window, or one not laid out yet. Answered at once, wherever the call is made; with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | With `opts.snapshot`, the point is not in the snapshot. Or the window form is outside the coordinate range, which a window table from `host.window` never is. Answered at once, as above. | `the point is not inside the snapshot`, `the region lies outside the screen's coordinate range` | both |
| `too_many` | both | In a handler or with a callback: 64 requests wait in the whole application, so this one is not taken; or this was the module's oldest of 16 when it asked for one more. Answered on the next turn of the event loop, as the colour would be. | `too many snapshot requests waiting in the whole application (64); this one was not taken`, `too many snapshot requests waiting for this module (16); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked again with the same key. | `a newer request with the same key replaced this one` | both |
| `failed` | both | The host's own code failed while it read the point; the `screen-capture` thread goes on. | `the screen capture failed inside the application; the log has the details` | both |
| `cancelled` | both | The capture thread stopped the request, which no path of the host does. | `the request was cancelled` | both |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: `x` or `y` that is not a number within the 32-bit coordinate range; a window form with a key beside `window` and `fraction`, a fraction that is missing or not finite, or a window without a `client`; a third argument after the window form that is not a callback; `opts` that is neither a table nor `nil`, or has a key other than `snapshot` (and `key` with a callback); a `snapshot` that is not a Snapshot or was released; a `key` without a callback, or one that is not a non-empty string; a callback that is not a function.

**Raises where it cannot wait**: without a callback or a snapshot, a point with a pixel to read, inside a function that may not wait; see [Where it waits](#where-it-waits).

### Windows {#host-screen-pixel-windows}

**Every call without a snapshot is a screen read of its own**; nothing is kept between calls. Reads the pixel straight from the screen. What you get is what is there — unless the module declares [`[screen] capture = "duplication"`](#which-picture-a-read-sees), in which case it is read from the duplicated picture. A point that lies on no monitor reads as black (0, 0, 0), the same as a region read gives there, whichever way the module reads.

The cost is per call. The standard path is `GetPixel` on the screen, measured at about 16.7 ms each — one compositor frame — so ten points in one poll are ten frames, about 170 ms: the module's wait, on the `screen-capture` thread. Under a declared `capture = "duplication"` each call is a 1×1 read of the duplicated picture, measured at 0.30 ms on an idle desktop in a test build; still one read per point. That thread may first wait for duplication — up to 250 ms, the module's wait and not the event loop's (see [How long a read waits](#which-picture-a-read-sees)); when duplication cannot answer, the call costs that wait plus the standard `GetPixel`, or answers `nil, err` under `fallback = "none"` ([duplication errors](#duplication-errors)). The window form adds no call to the operating system: its point is worked out from the table given.

### macOS {#host-screen-pixel-macos}

In a handler and given a callback the point is captured by itself on the `screen-capture` thread, at point resolution, as `pixels` captures one: one ScreenCaptureKit capture per call, measured at 36 to 91 ms, which a handler waits out. Nothing is kept between calls: two reads of nearby points are two captures, so read them with one [`pixels`](#host-screen-pixels). A point that cannot be read is black, with one line in the log a session. How the backing pixels of a Retina display are reduced to one value per point has not been measured, so compare colours with a tolerance rather than exactly.

## host.screen.pixels(points, opts?, cb?) {#host-screen-pixels}

**Signature:** `host.screen.pixels(points: { Point }, opts: { snapshot: Snapshot? }?) -> ({ Colour }?, Error?)`, where `Point = { number, number } | { x: number, y: number } | { window: Window, fraction: { number, number } }` and `Colour` is [`pixel`](#host-screen-pixel)'s; with a callback `host.screen.pixels(points, opts: { snapshot: Snapshot?, key: string? }?, cb: (colours: { Colour }?, err: Error?, info: Info) -> ()) -> ()`, `Info` as `pixel`'s

Reads the colour at every point of `points` and returns them as one list, in the order asked — one value, and nothing after it. Without a snapshot the points are read from as few captures as they allow (below), so eight probe points close together cost what one costs. With `opts.snapshot` they are read from that [snapshot](#host-screen-snapshot), touching no screen.

**All or nothing.** When one point cannot be read, the call answers `nil` and an error value instead of a list: a capture failed, a window point's window has an empty client area, or, with a snapshot, a point is not in it ([Errors](#host-screen-pixels-errors)).

**Read strictly, at the call, and what raises.** `points` is a list — entries 1, 2, 3, … and nothing else — of at most 4096 points. A point is `{ x, y }` or `{ x = …, y = … }` in screen coordinates, whole numbers inside the 32-bit coordinate range (`10.0` is one, `10.5` is not), written one way; or the window form exactly as [`pixel`](#host-screen-pixel) reads it. `opts` is `nil` or a table whose only key is `snapshot` — and, with a callback, `key`. Anything else raises, naming the point: `host.screen.pixels: points[2].x must be a whole number within the coordinate range (-2147483648 to 2147483647), got 1.5`, `points has 4097 entries; the limit is 4096`, `opts has a key 'region'; it takes snapshot`; so does a `snapshot` that is not a Snapshot or was released. An empty list returns an empty list without reading anything.

**Cost.** In a [handler](#where-it-waits) the points are read on the `screen-capture` thread while the module waits, from asking to the answer, for those captures, the ones ahead of them on that thread and the turn of the event loop that hands the list over; the event loop spends only what reading the arguments and building the list cost. Given a callback ([With a callback](#with-a-callback)) they are read there all the same and nothing waits: `cb(colours, err, info)` comes on a later tick. Where it cannot wait it raises, and nothing is read. When the points lie in a box of at most 2,000,000 pixels (1,000 × 2,000, say), it takes one capture of that box. Points spread wider are grouped by the tile of 2048 × 976 pixels each lies in — fixed tiles, counted from the screen coordinates 0, 0 — and each group is read with one capture of the box around its own points: two points far apart are two 1×1 captures, and points spread over a whole desktop one capture for each tile they reach, none larger than 2,000,000 pixels (8 MB). Each capture is read and let go before the next is taken. A point on no monitor reads black, as it does in a region read, with nothing captured for it. With a snapshot, or for an empty list or a window point with no pixel this time, it answers at once, wherever it is called, and costs microseconds.

```luau
-- A key that says which of three menu entries is lit: one capture for all three lamps, which
-- the hotkey's handler waits for. Needs "screen", "hotkey" and "speech" in the manifest.
local LAMPS = { { 120, 140 }, { 120, 172 }, { 120, 204 } }
local NAMES = { "Start", "Options", "Quit" }
host.hotkey.register("Ctrl+Alt+Shift+M", function()
    local lamps, err = host.screen.pixels(LAMPS)   -- waits here
    if not lamps then
        host.log.info("menu not read: " .. err)
        return
    end
    for i, c in ipairs(lamps) do
        if c.r > 200 and c.b < 90 then host.speech.output(NAMES[i]) break end
    end
end)
```

### Returns {#host-screen-pixels-returns}

- A list of `Colour`s, `{ { r, g, b, hex }, … }`, one for each point in the order asked — one value, and nothing after it. A point on no monitor reads black. An empty `points` gives an empty list, alone.
- `nil, err` — one of the points could not be read, so none is answered; `err.code` is one of the codes under [Errors](#host-screen-pixels-errors).
- With a callback: nothing at once; `cb(colours, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#host-screen-pixels-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window point, and its window's client area is empty: a minimised window, or one not laid out yet. Answered at once, wherever the call is made; with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | With `opts.snapshot`, a point is not in the snapshot. Or a window point is outside the coordinate range, which a window table from `host.window` never is. Answered at once, as above. | `the point is not inside the snapshot`, `the region lies outside the screen's coordinate range` | both |
| `failed` | both | A capture the points need failed: on Windows the standard path could not read one of the rectangles it plans, and on a Mac a capture failed. In the build for other systems, every read. | `screen capture failed` | both, other |
| `too_many` | both | In a handler or with a callback: 64 requests wait in the whole application, so this one is not taken; or this was the module's oldest of 16 when it asked for one more. Answered on the next turn of the event loop, as the list would be. | `too many snapshot requests waiting in the whole application (64); this one was not taken`, `too many snapshot requests waiting for this module (16); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked again with the same key. | `a newer request with the same key replaced this one` | both |
| `failed` | both | The host's own code failed while it read the points; the `screen-capture` thread goes on. | `the screen capture failed inside the application; the log has the details` | both |
| `cancelled` | both | The capture thread stopped the request, which no path of the host does. | `the request was cancelled` | both |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: `points` that is not a list — entries 1, 2, 3, … and nothing else — or has more than 4096 entries; a point that is not `{ x, y }` or `{ x = …, y = … }` of whole numbers within the 32-bit coordinate range, written one way, nor the window form as [`pixel`](#host-screen-pixel) reads it; `opts` that is neither a table nor `nil`, or has a key other than `snapshot` (and `key` with a callback); a `snapshot` that is not a Snapshot or was released; a `key` without a callback, or one that is not a non-empty string; a callback that is not a function.

**Raises where it cannot wait**: without a callback or a snapshot, a non-empty list inside a function that may not wait; see [Where it waits](#where-it-waits).

### Windows {#host-screen-pixels-windows}

A point that lies on no monitor reads black without anything being captured or asked for it, whichever way the module reads. The other points: through the standard path the planned captures are `BitBlt`s of the screen — not `GetPixel`, which `pixel` uses — each about one compositor frame, 16.7 ms, up to about 1028×666 and about 28 ms for a whole 1920×1080 screen. So a list of close points costs one `pixel`; points spread over a whole 1920×1080 primary screen take up to two captures (it reaches two rows of tiles), and over a 3840×2160 one up to six. Under a declared `capture = "duplication"` each of them is a 1×1 piece of one duplication request, which the `screen-capture` thread waits for up to 250 ms (see [How long a read waits](#which-picture-a-read-sees)). When duplication cannot answer, the points are read the standard way, or the call answers `nil, err` under `fallback = "none"` ([duplication errors](#duplication-errors)). Coordinates are device pixels.

### macOS {#host-screen-pixels-macos}

The same plan of captures, each at point resolution like every capture on this page: every call captures afresh — in a handler and given a callback on the `screen-capture` thread, one ScreenCaptureKit capture per planned rectangle, each measured at 36 to 91 ms. Colours are the downsampled values of a Retina display, so compare them with a tolerance. Without the Screen Recording permission the colours are those of the desktop wallpaper, not `nil`.

## host.screen.snapshot(opts, cb?) {#host-screen-snapshot}

**Signature:** `host.screen.snapshot(opts: { region: Region }) -> (Snapshot?, Error?)`, where `Snapshot` is the handle below; with a callback `host.screen.snapshot(opts: { region: Region, key: string?, at: number?, change: { from: Snapshot?, watch: { Region }?, timeout: number?, tolerance: number?, minPixels: number?, settle: number? }? }, cb: (snap: Snapshot?, err: Error?, info: SnapInfo) -> ()) -> ()`, where `SnapInfo = { time: number?, inputEpoch: number?, waited: number, frames: number, changed: boolean?, settled: boolean?, rebased: boolean? }`

Takes one picture of `opts.region` now and keeps it — in a [handler](#where-it-waits) on the `screen-capture` thread, while the handler waits for it, and where it cannot wait it raises instead; given a callback on that thread too, and hands it to the callback on a later tick ([below](#snapshot-with-a-callback)), at a set time or once part of the region has changed if asked. Every call on this page that is given it as `{ snapshot = s }` — see [Reading a snapshot](#reading-a-snapshot) for which calls and how — reads that picture instead of the screen: several questions about one moment, answered from the same pixels, for one capture. Nothing is ever read from a snapshot a call was not given.

**The region** is required and read strictly, as the cells calls read theirs: corners `{ x1, y1, x2, y2 }` as whole numbers, or a window region `{ window = w, fraction = { … } }` resolved at the call (see [Region form](#region-form)). Without a callback `opts` takes `region` and nothing else; given one, also `key`, `at` and `change`. A region may cover at most 40,000,000 pixels.

It answers the handle, or `nil` and an error value when there is nothing to keep this time: a window region with an empty client area or past a limit at the window's current size, a budget with no room, a limit of the requests waiting, or a capture that failed ([Returns](#host-screen-snapshot-returns), [Errors](#host-screen-snapshot-errors)). What a window or the other modules do at run time is answered, never raised; only a mistake in the call raises.

**Cost.** One capture, through the module's [source](#which-picture-a-read-sees) — what one `profile` of the region costs without the reduction. In a handler it is taken on the `screen-capture` thread, and the handler waits for it ([Where it waits](#where-it-waits)): from asking to the answer, the capture, the captures ahead of it on that thread and the turn of the event loop that hands it over; the event loop itself spends only what checking the arguments, reserving the charge and building the handle cost. Where it cannot wait it raises at once, and nothing is captured or charged. Every read of the snapshot afterwards touches no screen. In memory it holds `w * h * 4` bytes (more on macOS, below) until it is released or collected; in a handler they are charged from the call.

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
    -- In the timer's handler: it waits for the picture, and only this module waits.
    local snap, err = host.screen.snapshot({ region = { 96, 90, 544, 400 } })
    if not snap then host.log.info("menu not read: " .. err) return end
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
- `s.inputEpoch` — [`host.inputEpoch()`](./timer.md#host-inputepoch) as it stood once the picture had been taken: for `snapshot` in a handler and [with a callback](#snapshot-with-a-callback), the value when its picture came back on the `screen-capture` thread, which can be after the call — well after it with a callback. When `host.inputEpoch()` differs from it, the host has turned it over since the picture was taken. Input the host drives itself (`host.input.*`) turns it over before it acts, so that input came after the picture and the picture predates its effect. A controller press or a window coming forward turns it over when the host delivers or notices it, a little after it happened, so its effect may already be in the picture — compare the event's `time` with `s.time`. When the two are the same, that proves nothing about whether the effect of an earlier action had been drawn yet when the picture was taken.
- `s.via` — which way it was taken: `"gdi"` (the standard Windows path), `"duplication"` (desktop duplication), `"screencapturekit"` or `"coregraphics"` (macOS). Under the default `fallback = "standard"` a module that reads through duplication can get a `"gdi"` picture; this says which one it got.
- `s.released` — `true` once it has been released.
- `s:release()` — lets the pixels go now, and gives their bytes back to the budget: [`Snapshot:release`](#snapshot-release).
- `s:crop(region, cb?) -> (Snapshot?, Error?)` — a snapshot of its own of a part of `s`, those pixels copied and charged anew: [`Snapshot:crop`](#snapshot-crop).
- `tostring(s)` — `Snapshot(640x480 at 96,120, gdi, 1.2 MiB, 35 ms old)`, or `Snapshot(released)`.

**The budget.** One module VM's snapshots may hold **128 MiB** between them, and all modules' together **512 MiB**. A snapshot is charged before its capture — `w * h * 4` bytes, five times that on macOS — and settled to what it really holds once it exists. Over either limit, this module's collector is run twice, and if the snapshot still does not fit the call answers `nil, err`, `too_large`: `this module's snapshots would hold 144.0 MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()`, or `the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. …` when it is the second. That holds for a region given as corners too: other modules' snapshots count toward the application's limit, so whether a picture fits is the moment's, not the call's, and a poll never puts an error dialog over the window it reads. For a **window region** the sentence comes after `at the window's current size `, as with the 40,000,000-pixel limit. Nothing is charged for a picture so answered. Only the calling module's collector is run, so the application's limit can be reached by other modules' snapshots, which the call cannot free. Every snapshot also steps the collector by about as many bytes as it charges, but at most about what the VM's Luau heap holds: a snapshot as large as the heap or larger costs about one collection of the heap, on the event loop. So a poll that drops its snapshots without releasing them is paced by the collector rather than stopped by the limit — but `release()` is what frees them at once. The budget also bounds one snapshot: in a module that holds no other, at most 33,554,432 pixels on Windows and, charged five times over at the call whatever the display, 6,710,886 points on macOS — fewer than the 40,000,000 a region may cover; a larger one is answered for the budget.

**Lifetime.** A snapshot belongs to the module VM that took it. It cannot be passed to another module: a legacy data export and `host.settings` refuse it. Disabling the module keeps its snapshots; reloading it frees them with the VM. A read on the image worker that was given a snapshot keeps its pixels until it is answered, even when the snapshot is released meanwhile; those bytes are not charged. A poll that keeps its last snapshot should release it when its window leaves the front.

### With a callback {#snapshot-with-a-callback}

`host.screen.snapshot(opts, cb)` takes the picture the call without one takes, off the event loop, and hands it to `cb` on a later tick, as every call on this page given a callback does ([With a callback](#with-a-callback)): at once; at a set time (`at`, [below](#taken-at-a-set-time)); or once part of the region has changed (`change`, [below](#waiting-for-the-picture-to-change)). The picture is taken on the `screen-capture` thread that also takes [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)'s (see the top of this page), through the module's [source](#which-picture-a-read-sees).

**The callback** runs exactly once, on the event loop, on the loop's next turn after the answer (every 15 ms): `cb(snap, nil, info)` with a Snapshot exactly like the one the call without a callback returns, or `cb(nil, err, info)` when there is none ([Errors](#host-screen-snapshot-errors)) — unless the module is **disabled, reloaded, removed or [stopped by the guard](../module-runtime-and-lifecycle.md#limits)** first: then it is dropped, never called, as a [`host.ocr.recognize`](./ocr.md#host-ocr-recognize)'s is, and the capture thread stops photographing for it. It runs with the priority of the dispatch that asked (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize), "Order and priority"). A callback that raises is reported like any other module callback error; a module whose VM runs out of memory while the answer — the Snapshot and `info` — is built in it is [stopped](../module-runtime-and-lifecycle.md#limits), as for a text read's reading. `info.time` and `info.inputEpoch` are the snapshot's own `time` and `inputEpoch`, absent when there is no snapshot. `info.waited` is the milliseconds from the call until the picture was taken — or until the request ended without one; `info.frames` is how many pictures were taken for it: 1 for a plain or timed one, one per round that came back for a change wait, 0 when none did — and 0 for a request answered because a newer one with its key replaced it, a limit ended or refused it, or the host's own code failed, whatever had been taken for it before. With `change`, `info` also has `changed`, `settled` and `rebased`. At exit, requests still waiting are dropped: no callback runs after the event loop has stopped. While the module is busy — one of its handlers [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read or a `snapshot` — the answer waits behind that handler and the callback runs once the module is free, in the order its events came; whether a newer request with its `key` replaced it is decided then, so one made meanwhile by an event that ran before it — the waiting handler once it went on, say — answers it with the `replaced` error value below.

**`key`.** A newer request of the same module VM with the same `key` answers every older one with that key `nil, err` (`replaced`, `a newer request with the same key replaced this one`) on the next tick — also one whose picture has been taken but not delivered yet, and one whose answer is in the same delivery as the callback that asked again. So of a burst of D-pad presses only the last one's answer is read — though a change wait's answer can show an earlier press's move ([Compare with what you last read](#waiting-for-the-picture-to-change)). Keys here are `snapshot`'s own: a `host.ocr.recognize`, a [`profile`](#host-screen-profile) or a [`pixels`](#host-screen-pixels) with the same key never replaces one, nor the other way round. Without a key a request is never replaced. [`host.screen.pending(key)`](#host-screen-pending) says whether one with a key is still out.

**Nothing cancels a request** but a newer one with its key — which is a request of its own, taken in turn — and disabling, reloading or removing the module. A callback whose answer is no longer wanted, because the window left the front meanwhile say, checks that, releases the snapshot and returns.

**Read at the call, and what raises.** `opts` is read strictly: `region` (required), `key`, `at` and `change`, and nothing else. The region is the [Region form](#region-form) read strictly, as the call without a callback reads it; a window region is resolved at the call. Raises, naming what is wrong: `opts` that is not a table (`host.screen.snapshot: opts must be a table { region = …, key?, at?, change? }, got nothing`), another key, a region missing or not exactly the Region form, corners covering more than 40,000,000 pixels — 8,294,400 (one 4K screen) with `change` —, a `key` that is not a non-empty string, an `at` that is not a number or more than 2000 ms after `host.now()`, anything in `change` [out of its range](#waiting-for-the-picture-to-change) (a `minPixels` larger than the number of pixels the wait watches is, when the region and every `watch` region are corners), and a `cb` that is neither a function nor `nil` (`host.screen.snapshot: the callback (second argument) must be a function, got 5`). Nothing a window does at run time raises: a window region whose client area is empty (`the window's client area is empty (0x0)`), that is past the limit at the window's current size, a `watch` of a window that misses the region, and a window that leaves the wait fewer watched pixels than its `minPixels`, are answered `nil, err` on the next tick, without a capture — and so is any region the budget has no room for (below).

**Limits**, answered rather than raised, so that the newest request — the press somebody is waiting for — goes ahead of its module's older ones: a module VM may have 16 requests waiting — its [`profile`](#host-screen-profile)s with a callback among them, until the image worker has answered them — of which 4 change waits; one more ends its oldest request (or oldest wait) with `too many snapshot requests waiting for this module (16); the oldest was ended` (`… change waits for this module (4) …`). The whole application may have 64 requests waiting, of which 16 change waits, counted as they stand once the module's own oldest has ended; past that the new request is answered `too many snapshot requests waiting in the whole application (64); this one was not taken` (`too many change waits in the whole application (16); this one was not started`), and a request refused so ends none of its module's. The log says so, once per module every 10 seconds, and likewise for a window region answered for the budget.

**The budget** is the call's without a callback: the request is charged at the call — `w * h * 4` bytes, five times that on macOS — and a change wait `2 * w * h * 4` more, for the two pictures it only compares with, its baseline and the one a still spell began with, which it keeps beside its newest picture as copies of the watched part, one pixel per point and without macOS's picture at the display's resolution: 3 × `w * h * 4` on Windows, 7 × on macOS. The charge is settled to what the snapshot holds when it arrives. A request answered without a picture gives it back — at once when a newer request with its key replaced it, a limit ended it or the application's limit refused it, so a burst of presses holds the newest request's charge, not one per press. Over the budget a region is answered `nil` and the error value on the next tick, as it is without a callback; so in a module that holds nothing else a change wait can cover at most 11,184,810 pixels on Windows — more than the 8,294,400 its region may — and 4,793,490 points on macOS, and a plain or timed request 33,554,432 pixels or 6,710,886 points. A `from` snapshot a change wait compares with is kept by the wait until its first round even if it is released meanwhile, uncharged; after that the wait keeps its own copy of the watched part — or, when `from` is exactly the watched part and keeps nothing beside its pixels (on Windows), `from` itself, uncharged, to the end of the wait. Two requests for the same region answered from one capture hold the same pixels and are each charged for them.

**Cost.** On the event loop: checking the arguments and the charge (with its collector step, see the budget [above](#host-screen-snapshot)), microseconds. The capture is on the `screen-capture` thread — after the [first-read comparison](#which-picture-a-read-sees), in a module that reads through desktop duplication, while duplication has not answered it yet — one at a time with the text reads' captures, in the order the top of this page describes. Requests that are due together and read through the same source share their captures. The most urgent one due goes first, by the order at the top of this page. The standard way it takes one capture, which every other due request shares whose region lies inside it or whose union with it covers at most 2,000,000 pixels — on macOS only regions on the same display —, and the rest are taken next, a capture at a time: the most urgent picture never waits behind another module's large capture. Through desktop duplication every due request goes in one request, each distinct region a piece of it. The answer is cut out of a shared capture on that thread, and a change wait keeps only its own region of it. A request is never answered from a picture taken before it was asked.

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
local last, spoken, pressedAt = nil, nil, -math.huge

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
    if host.now() - pressedAt < 1000 then return end   -- a press is being read
    if host.screen.pending("poll") then return end     -- the last one has not answered yet
    host.screen.snapshot({ region = MENU, key = "poll" }, function(snap)
        if snap and keep(snap) then readMenu(snap) end
    end)
end)

host.gamepad.on("down", function(e)
    if not gameInFront() then return end
    pressedAt = host.now()
    host.screen.snapshot({ region = MENU, key = "press",
        change = { from = last, watch = { CURSOR_COLUMN }, timeout = 400, settle = 50 } },
      function(snap, err, info)
        if not snap then return end   -- a newer press replaced this one, or nothing to read
        local again = snap.time + 150
        if keep(snap) then readMenu(snap) end
        -- Once more, a moment later; a newer press replaces this request as it did the wait.
        host.screen.snapshot({ region = MENU, key = "press", at = again }, function(later)
            if later and keep(later) then readMenu(later) end
        end)
    end)
end, { buttons = { "dpad_up", "dpad_down" } })
```

### Taken at a set time {#taken-at-a-set-time}

`at` is a time on [`host.now()`](./timer.md#host-now)'s clock, in milliseconds: the picture is not taken before it. An `at` already past is taken at once; one more than 2000 ms after `host.now()` raises. A controller event's `time` is on the same clock, so `at = e.time + 80` is 80 ms after the press itself, however late the callback runs. Nothing holds the module's input back for it, as for any request: its picture is meant to come after whatever the module does next, and acting before it is taken is not noted in the log (an `at` already past at the call counts as a plain request there: [host.input](./input.md)). With `change`, `at` is when the wait starts, and its `timeout` counts from then.

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
        host.screen.snapshot({ region = BUBBLE, at = e.time + after }, function(snap)
            if not snap then return end
            if mine == press and not read then
                local p = host.screen.profile({ axes = "rows", snapshot = snap })
                if p and p.rows.max[1] > 240 then   -- the bubble's white top edge
                    read = true
                    host.ocr.recognize(BUBBLE, { snapshot = snap }, function(r)
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
- **At the `timeout`**, counted from the call (or from `at`), the newest picture is the answer: `changed` is `true` when a change was seen, was still settling and the newest picture still differs from the baseline; `settled` is `false`. When no round came back at all, `nil` and the last capture's error value, or `timeout`, `no picture was taken before the change wait's timeout`.

The options, read at the call — each out of its range raises, naming it (`host.screen.snapshot: opts.change.settle must be a whole number from 0 to 500 (the timeout), got 600`):

| Option | Default | Meaning |
|---|---|---|
| `from` | the wait's first picture | A Snapshot to compare with; one that was released raises. |
| `watch` | the whole region | 1 to 16 regions in the [Region form](#region-form), read strictly — `{ { x1, y1, x2, y2 }, … }`, a list even of one; a single region raises and says to put it in braces. Each is cut to `region`, and a pixel two of them share counts once. One that misses `region` raises when both are corners; when a window decides it, the callback gets `nil, err` (`outside`, `a watch region does not overlap the region`). |
| `timeout` | 500 | Whole milliseconds, 1 to 2000. |
| `tolerance` | 16 | 0 to 255 per channel. |
| `minPixels` | 4 | 1 to 8,294,400, and at most the number of pixels the wait watches (each `watch` region cut to `region`, a pixel two of them share once — or the whole region): more raises when the region and every `watch` region are corners, since no picture could count as changed; when a window decides it, the callback gets `nil, err` (`outside`, `at the window's current size the change wait watches … pixels, fewer than its minPixels …`). So a `watch` of one pixel needs `minPixels = 1`. |
| `settle` | 0 | Whole milliseconds, 0 to `timeout`. |

**What counts.** Only the watched pixels: keep a blinking cursor, an animated background or a clock out of `watch`, or the wait ends on them. The mouse pointer is normally not in the picture (see [Which picture a read sees](#which-picture-a-read-sees)); a pointer the game draws itself is, and counts as a change unless it is outside `watch`.

**Compare with what you last read.** Without `from`, the baseline is the wait's first picture, taken when the thread gets to it: if the game had already redrawn by then, the change is in the baseline and the wait sees nothing until its timeout. Pass `from = last` — the snapshot the module last read — and a change that happened before the first round is seen at the first round. With a burst of presses, `last` can predate an earlier press's move as well: the wait of the second press sees the first one's move at its first round. With `settle` it answers only once the moves have stood still, which the second press's move, drawn a frame or two later, resets; a game slower to draw it than `settle` is caught by reading once more a moment after the answer, as the example above does. A wait whose baseline is its first picture — it has no `from`, or one it cannot compare with: one that does not hold every watched region, or, on Windows, one taken the other way than the module reads (a `"gdi"` snapshot in a module that reads through desktop duplication) — takes that picture when the thread gets to it, and nothing holds the module's input back meanwhile: a click the module makes right after asking can be in the baseline already, and the wait then misses what the click changed. To wait for what a click of its own changes, take a snapshot with a callback first, click in the callback, and start the wait there with `from = snap`. A wait with a `from` it can compare with has its baseline already.

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
    host.screen.snapshot({ region = BUBBLE, key = "bubble",
        change = { timeout = 600, settle = 40 } },
      function(snap, err, info)
        if not snap then return end
        local p = info.changed and host.screen.profile({ axes = "rows", snapshot = snap })
        if p and p.rows.max[1] > 240 then   -- the bubble's white top edge
            host.ocr.recognize(BUBBLE, { snapshot = snap, key = "bubble" }, function(r)
                if r.status == "text" and not r.newer then host.speech.output(r.text) end
            end)
        end
        snap:release()   -- the text read keeps its own pixels until it is answered
    end)
end, { buttons = { "dpad_up", "dpad_down", "dpad_left", "dpad_right" } })
```

### Returns {#host-screen-snapshot-returns}

- A `Snapshot` — one value, and nothing after it: the handle above.
- `nil, err` — there was nothing to keep this time; `err.code` is one of the codes under [Errors](#host-screen-snapshot-errors). Nothing is charged for it.
- With a callback: nothing at once; `cb(snap, nil, info)` or `cb(nil, err, info)` on a later tick, `info` as [above](#snapshot-with-a-callback). A change wait that saw no change still answers a picture, the newest, with `info.changed` `false`; only one none of whose rounds came back answers `nil, err`.

### Errors {#host-screen-snapshot-errors}

None of these is raised. Each comes as an error value where "arrives" says. What a window decides is answered without a capture, on the next turn of the event loop, as the picture would be.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, or a window region of a change wait's `watch`, whose window's client area is empty: a minimised window, or one not laid out yet. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. | `the region lies outside the screen's coordinate range` | both |
| `outside` | callback | A change wait whose `watch` and region, one of them resolved against a window at the call, miss each other, or leave it fewer watched pixels than its `minPixels`, so that no picture could count as changed. The same in corners only raises. | `a watch region does not overlap the region`, `at the window's current size the change wait watches … pixels, fewer than its minPixels …` | both |
| `too_large` | both | A window region past 40,000,000 pixels at the window's current size. The same corners raise. | `at the window's current size the region is …x…, … pixels; the limit is 40000000` | both |
| `too_large` | callback | A change wait's window region past 8,294,400 pixels at the window's current size. The same corners raise. | `at the window's current size the region is …x…, … pixels; the limit is 8294400` | both |
| `too_large` | both | The [budget](#the-snapshot-handle) has no room, after this module's collector ran twice: this module's 128 MiB, or the application's 512 MiB, which other modules' snapshots count toward. Answered for corners too, since the moment decides it. For a window region the message begins `at the window's current size `, which is the leading `…`. | `…this module's snapshots would hold … MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()`, `…the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()` | both |
| `too_many` | both | 64 requests wait in the whole application, so this one is not taken; or this was the module's oldest of 16 when it asked for one more. A handler's own request is ended so only by one its module asks while it waits, which only an arbiter's `onActivate` or `onDeactivate` of the module can. | `too many snapshot requests waiting in the whole application (64); this one was not taken`, `too many snapshot requests waiting for this module (16); the oldest was ended` | both |
| `too_many` | callback | A change wait: 16 change waits wait in the whole application, so this one is not started; or this was the module's oldest of 4 when it asked for one more. | `too many change waits in the whole application (16); this one was not started`, `too many change waits for this module (4); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked again with the same key ([`key`](#snapshot-with-a-callback)). | `a newer request with the same key replaced this one` | both |
| `timeout` | callback | A change wait none of whose rounds came back before its timeout. One whose captures failed answers the last capture's error instead. | `no picture was taken before the change wait's timeout` | both |
| `cancelled` | both | The capture thread stopped the request, which no path of the host does. | `the request was cancelled` | both |
| any of the [capture errors](#capture-errors) | both | The picture could not be taken. | `screen capture failed` | both, other |
| `failed` | both | The `screen-capture` thread's own code failed while it took the picture. The thread goes on. | `the screen capture failed inside the application; the log has the details` | both |
| `failed` | both | The host's own code failed for this snapshot after its round's capture. The rest of the round is answered as usual. | `the snapshot failed inside the application; the log has the details` | both |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: `opts` that is not a table (`host.screen.snapshot: opts must be a table { region = … }, got nothing`); a key other than `region` — `key`, `at` and `change` are only taken with a callback (`host.screen.snapshot: opts.key is only taken with a callback (the second argument)`); a region missing or not exactly the Region form; corners covering more than 40,000,000 pixels (`host.screen.snapshot: the region is 10000x5000, 50000000 pixels; the limit is 40000000`), or 8,294,400 with `change`. With a callback, also: a `key` that is not a non-empty string; an `at` that is not a number, or more than 2000 ms after `host.now()`; anything in `change` [out of its range](#waiting-for-the-picture-to-change), a `from` that was released, a `watch` in corners that misses a region in corners, and a `minPixels` more than the wait watches when the region and every `watch` region are corners; a callback that is neither a function nor `nil`.

**Raises where it cannot wait**: without a callback, inside a function that may not wait, whatever it was given; see [Where it waits](#where-it-waits).

### Windows {#host-screen-snapshot-windows}

The capture is `profile`'s: through the standard path the fixed compositor frame (about 16.7 ms up to about 1028×666, 28 ms for the whole 1920×1080 screen), through desktop duplication 0.3–5 ms on an idle desktop, by size, plus any wait for duplication — at most 250 ms on the `screen-capture` thread, which a handler waits out (see [How long a read waits](#which-picture-a-read-sees)). Through duplication the snapshot is the most recently composed frame, which may predate the newest redraw of a window. Coordinates and pixels are device pixels; the part of a region that leaves every monitor is black. A snapshot holds `w * h * 4` bytes.

With a callback each picture is taken so on the `screen-capture` thread: through the standard path the fixed compositor frame (about 16.7 ms up to about 1028×666, 28 ms for the whole 1920×1080 screen); through desktop duplication the most recently composed frame, 0.3–5 ms on an idle desktop, the thread waiting up to 250 ms for duplication as the image worker does (see [How long a read waits](#which-picture-a-read-sees)) and answering `nil, err` under `fallback = "none"` when it cannot ([duplication errors](#duplication-errors)). A change wait through the standard path therefore captures back to back, one compositor frame a round, for up to its timeout. Between rounds, and until an `at`, the thread sleeps, and Windows wakes it on its timer: a round or a timed picture can come up to one timer tick late (15.6 ms at Windows' default timer resolution). Through desktop duplication a round that finds no newly composed frame compares the frame it already holds. When duplication cannot answer a round and the module allows the fallback, the round's regions are read the standard way, planned as a standard round is — a region inside another's capture cut from it, unions of up to 2,000,000 pixels — one compositor frame per planned capture. Coordinates are device pixels; the part of a region that leaves every monitor is black.

### macOS {#host-screen-snapshot-macos}

The snapshot is taken at the display's own resolution and drawn down to one pixel per point, which is what every read of it reads: the downsampled values of a Retina display, so compare colours with a tolerance. The picture at the display's own resolution is kept beside them, so on a Retina display a snapshot holds up to about five times `w * h * 4` bytes, and is charged that. The part of a region off the desktop is black; a region wholly off it answers `nil, err` (`failed`, `screen capture failed`). Without the Screen Recording permission the capture does not fail: the snapshot is a picture of the wallpaper.

With a callback each picture is taken so, on the `screen-capture` thread. A round made only of change waits does not feed the log's warning about a region that keeps coming back one flat colour, which every other round does. There is no desktop duplication. Regions whose centres lie on two displays never share a capture, since two displays can differ in scale. A picture from ScreenCaptureKit and one from the older Core Graphics functions are two ways of capturing, as the two Windows paths are: a wait whose rounds switch between them mid-wait takes the new picture as its baseline, `rebased = true`. Which way a round will be taken is not known at the call, so a `from` taken the other way is found out only at the first round. A change wait is charged 28 bytes a point at the call, so one over 4,793,490 points (2560 × 1873, say) is answered for the budget (`too_large`) even in a module that holds nothing else, below the 8,294,400 its region may cover; and a plain or timed request, 20 bytes a point, one over 6,710,886 points. Without the Screen Recording permission every picture is of the wallpaper: a plain request answers one, and a change wait ends at its timeout with `changed = false`.

## Snapshot\:crop(region, cb?) {#snapshot-crop}

**Signature:** `s:crop(region: Region) -> (Snapshot?, Error?)`; with a callback `s:crop(region: Region, cb: (crop: Snapshot?, err: Error?, info: Info) -> ()) -> ()`, where `Info = { time: number?, inputEpoch: number? }`

A snapshot of its own of a part of the [snapshot](#host-screen-snapshot) `s`: those pixels copied, charged anew against the [budget](#the-snapshot-handle), with `s`'s `time`, `inputEpoch` and `via`. Keep the part a module goes on reading, and release the large picture it came from.

**The region** is read strictly, as `snapshot` reads its own: corners `{ x1, y1, x2, y2 }` as whole numbers, or a window region resolved at the call ([Region form](#region-form)). It must lie wholly inside `s`. A region larger than `s` is simply not inside it: there is no size limit of its own.

**Where it runs.** In a [handler](#where-it-waits) it waits while the image worker makes the copy, and only the module waits. Its charge is reserved at the call, and given back at once if the module is disabled meanwhile. Where it cannot wait it raises, as `Snapshot:crop`, and nothing is copied. **Given a callback**, `s:crop(region, function(crop, err, info) … end)`, the image worker makes the copy and nothing waits; `info` carries `s`'s `time` and `inputEpoch` ([With a callback](#with-a-callback)). A crop takes no key: it has no options. A crop with nothing to copy — no rectangle this time, a region not inside `s`, a region the budget has no room for — is answered at once wherever it is asked, and given a callback on the next tick.

**Cost.** It touches no screen. It costs a copy of the part's pixels — on macOS also the matching part of the kept picture at the display's own resolution, drawn anew — and the collector step every charge makes (see the budget under [The Snapshot handle](#the-snapshot-handle)).

```luau
-- Keep the title strip of a large picture, and let the rest go. In a hotkey's handler, where the
-- snapshot and the crop wait. Needs "screen", "hotkey" and "log" in the manifest.
local title = nil
host.hotkey.register("Ctrl+Alt+Shift+C", function()
  local s, err = host.screen.snapshot({ region = { 0, 0, 1280, 720 } })   -- waits here
  if not s then host.log.info("no picture: " .. err) return end
  local strip, cropErr = s:crop({ 0, 0, 1280, 40 })   -- and here, for the image worker
  s:release()
  if not strip then host.log.info("not cut: " .. cropErr) return end
  if title then title:release() end
  title = strip
  host.log.info("kept " .. tostring(title))   -- Snapshot(1280x40 at 0,0, gdi, 0.2 MiB, 3 ms old)
end)
```

### Returns {#snapshot-crop-returns}

- A `Snapshot` — one value, and nothing after it: the part's pixels, with `s`'s `time`, `inputEpoch` and `via`.
- `nil, err` — nothing was copied, and nothing is charged; `err.code` is one of the codes under [Errors](#snapshot-crop-errors).
- With a callback: nothing at once; `cb(crop, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#snapshot-crop-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. Answered at once. | `the window's client area is empty (…x…)` | both |
| `outside` | both | The region is not wholly inside `s`. Or a window region lies outside the coordinate range, which a window table from `host.window` never does. Answered at once. | `the region is not inside the snapshot`, `the region lies outside the screen's coordinate range` | both |
| `too_large` | both | The [budget](#the-snapshot-handle) has no room for the copy: this module's 128 MiB, or the application's 512 MiB, which other modules' snapshots count toward. For a window region the message begins `at the window's current size `, which is the leading `…`. Answered at once. | `…this module's snapshots would hold … MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()`, `…the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest ([Image-worker errors](#image-worker-errors)). | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `failed` | both | The copy failed inside the host, or could not be handed to the image worker because the application is closing. | `internal error: the crop could not be made` | both |

**Raises** for a mistake in the call: a `crop` not called on a Snapshot (`Snapshot:crop: call it on a Snapshot, s:crop(region), not on …`); a snapshot that was released (`Snapshot:crop: the snapshot was released`); a region missing or not exactly the Region form, read strictly; a callback that is neither a function nor `nil`.

**Raises where it cannot wait**: without a callback, a crop with something to copy, inside a function that may not wait; see [Where it waits](#where-it-waits).

### Windows {#snapshot-crop-windows}

The copy is of the snapshot's device pixels, made on the image worker's matching pool below normal priority (see the top of this page). The crop holds `w * h * 4` bytes.

### macOS {#snapshot-crop-macos}

The copy is of the snapshot's pixels, one per point, and of the matching part of the picture at the display's own resolution the snapshot keeps, drawn anew. So on a Retina display a crop holds up to about five times `w * h * 4` bytes, and is charged that at the call, as a snapshot is.

## Snapshot\:release() {#snapshot-release}

**Signature:** `s:release() -> ()`

Lets the pixels of the snapshot `s` go now, and gives their bytes back to the [budget](#the-snapshot-handle). A second call does nothing. The fields stay readable, and `s.released` is then `true`. Handing a released snapshot to a call, or cropping it, raises (`host.screen.pixel: opts.snapshot: the snapshot was released`). A read on the image worker that was given the snapshot keeps its pixels until it is answered, uncharged, so a module may release a snapshot as soon as it has handed it to every read.

Without it the pixels go only when the collector finalises the handle. It runs on the event loop, at once, wherever it is called, and costs microseconds.

```luau
-- In a handler, where the snapshot waits. Needs "screen", "hotkey" and "log" in the manifest.
host.hotkey.register("Ctrl+Alt+Shift+N", function()
  local s = host.screen.snapshot({ region = { 96, 90, 544, 400 } })   -- waits here
  if not s then return end
  local lamp = host.screen.pixel(120, 140, { snapshot = s })   -- of the snapshot: at once
  s:release()                       -- the pixels go back to the budget now
  host.log.info(tostring(s) .. ", lamp " .. (lamp and lamp.hex or "?"))   -- Snapshot(released), lamp #…
end)
```

### Returns {#snapshot-release-returns}

Nothing.

### Errors {#snapshot-release-errors}

None. It never answers an error value.

**Raises** only when it is not called on a Snapshot: `s.release()` without the colon, or on another value.

### Windows {#snapshot-release-windows}

The same on every platform: the bytes a snapshot holds go back to the module's and the application's budgets.

### macOS {#snapshot-release-macos}

The same on every platform. The picture at the display's own resolution the snapshot kept goes with it.

## Reading a snapshot {#reading-a-snapshot}

A call given `{ snapshot = s }` reads the picture `s` holds instead of the screen. It captures nothing, touches no screen, never asks desktop duplication, and never makes the [first-read comparison](#which-picture-a-read-sees). What the call does with the picture — the matching, the reduction, the PNG — costs what it always costs, where it always runs: for `profile`, the cells calls, `template`, `crop`, the searches, `save` and `saveMarked` in a handler, on the image worker while the handler waits ([Where it waits](#where-it-waits)), and given a callback on the image worker while nothing waits ([With a callback](#with-a-callback)); where they cannot wait they raise, as without a snapshot. `pixel` and `pixels` of a snapshot are answered at once wherever they are called. Given a callback, every call below answers what the table says through it, `cb(value, err, info)` on a later tick, `info` with the snapshot's `time` and `inputEpoch`.

| Call | Given as | Rule | When the region or point misses |
|---|---|---|---|
| [`pixel`](#host-screen-pixel) | `pixel(x, y, { snapshot = s })`, `pixel(point, { snapshot = s })` | inside | `nil, err` (`outside`, `the point is not inside the snapshot`) |
| [`pixels`](#host-screen-pixels) | `opts.snapshot` | inside, every point | `nil, err` (`outside`, `the point is not inside the snapshot`) |
| [`profile`](#host-screen-profile) | `opts.snapshot` | cut; `x, y, w, h` are the part read | `nil, err` (`outside`, `the region does not overlap the snapshot`) |
| [`imageSearch`](#host-screen-imagesearch), [`imageSearchAll`](#host-screen-imagesearchall) | `opts.snapshot` | cut | `nil, err`; `{}, err` (`outside`, `the region does not overlap the snapshot`) |
| [`imageSearchEach`](#host-screen-imagesearcheach) | `opts.snapshot` | cut; each `within` is cut to the part read | `cb(nil, err, info)` on a later tick (`outside`, `the region does not overlap the snapshot`) |
| [`template`](#host-screen-template) | `{ capture = region, snapshot = s }` | inside | `nil, err` (`outside`, `the region is not inside the snapshot`) |
| [`cells`](#host-screen-cells), [`matchCells`](#host-screen-matchcells) | `opts.snapshot` | inside | `nil, err` (`outside`, `the region is not inside the snapshot`) |
| [`save`](#host-screen-save), [`saveMarked`](#host-screen-savemarked) | `opts.snapshot` | cut; the PNG is the part read, and marks stay screen coordinates | `false, err` (`outside`, `the region does not overlap the snapshot`) |
| [`s:crop(region, cb?)`](#snapshot-crop) | — | inside | `nil, err` (`outside`, `the region is not inside the snapshot`) |
| [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) | `opts.snapshot` | cut, to the part the recogniser can read; the reading's `x, y, w, h` are the part read | that region's reading is `"failed"`, its `error` an error value (`outside`, `the region does not overlap the snapshot`); the call's other regions are read |
| [`snapshot` with a callback](#snapshot-with-a-callback) | `change.from` | the baseline of a change wait | a `from` that does not hold every watched region is replaced by the wait's first picture (`rebased`) |

**Cut** reads the part of the region that lies inside the snapshot, and answers `outside` when no part does. **Inside** needs the whole region or point inside the snapshot and answers `outside` otherwise: a template or a grid of cells cut to fit would be a different template or grid.

**Defaults.** A call that reads its region loosely — `profile`, the image searches and a `within`, `save`, `saveMarked`, `template`'s `capture` — takes a region left out as the whole snapshot, and a corner left out (or not a number) as the snapshot's edge, where without a snapshot they are the primary screen and its edges. The strictly read regions (the cells calls, `crop`) have no default, as without one.

**Window regions and points** are resolved against the window table they are given, as always, with no call to the operating system, and the rule applies to the rectangle that gives. To read the part of a window a snapshot holds, pass the window table the snapshot was taken with: a window that has moved since is read where that table says it was.

**What raises** is what raises without a snapshot — a mistake in a region raises with a snapshot as without one: the region is read in full before the snapshot's rule is applied to it, and it raises whether or not the snapshot holds it — plus a `snapshot` that is not a Snapshot (`host.screen.profile: opts.snapshot must be a Snapshot, got a userdata of another kind`, for a Template) and one that was released. `template` takes `snapshot` only beside `capture` and raises otherwise (`host.screen.template: snapshot belongs with capture: …`).

**On the image worker**, a read given a snapshot is answered from it in its batch without a capture and shares no capture with any other read. If its module is disabled before the answer comes, the answer is dropped, as without a snapshot.

**Text recognition** reads a snapshot through [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) `{ snapshot = s }`: nothing is photographed, and the recogniser reads the snapshot's pixels — on macOS the picture at the display's own resolution it kept, so the read is as sharp as a live one. Where the call cannot wait ([Where it waits](./ocr.md#where-it-waits)), a `snapshot` raises.

```luau
-- The page a menu is on and where its cursor is, from one picture: they cannot disagree. In a
-- hotkey's handler, where the snapshot and the match wait. Needs "screen", "window", "hotkey",
-- "resource" and "log" in the manifest.
-- pages = { predicate = "…", states = { { name = "Items", cells = "…" }, … } }, recorded with cells.
local pages = host.json.decode(host.resource.read("data/pages.json"))
host.hotkey.register("Ctrl+Alt+Shift+G", function()
    local w = host.window.active()
    local snap = w and host.screen.snapshot({ region = { window = w, fraction = { 0, 0, 1, 1 } } })
    if not snap then return end
    local page = host.screen.matchCells({
        region = { window = w, fraction = { 0.02, 0.10, 0.30, 0.16 } },
        cols = 8, rows = 2, predicate = pages.predicate, snapshot = snap,
    }, pages.states)   -- waits here, for the image worker
    local cursor = host.screen.imageSearch("images/cursor.png",
        { region = { window = w, fraction = { 0, 0.2, 0.1, 1 } }, snapshot = snap, tolerance = 8 })   -- and here
    snap:release()
    if page and cursor then host.log.info(page.name .. " page, cursor at y " .. cursor.y) end
end)
```

### Windows {#reading-a-snapshot-windows}

Reading a snapshot is the same on every platform: its pixels are those the capture delivered, in device pixels here.

### macOS {#reading-a-snapshot-macos}

Reading a snapshot is the same on every platform: its pixels are one per point, the downsampled values of a Retina display.

## host.screen.size() {#host-screen-size}

**Signature:** `host.screen.size() -> { w: number, h: number }`

Returns the primary screen dimensions in pixels as `{ w, h }`. It asks the system at every call, on the event loop, and costs microseconds.

```luau
local s = host.screen.size()
host.log.info("screen is " .. s.w .. "x" .. s.h)
```

### Returns {#host-screen-size-returns}

A new table `{ w, h }` at every call: the primary screen's width and height, in device pixels on Windows and in points on macOS. In the build for other systems, `{ w = 0, h = 0 }`.

### Errors {#host-screen-size-errors}

None. It never answers an error value. It never raises.

### Windows {#host-screen-size-windows}

**Device pixels.** The application declares per-monitor DPI awareness, so a 4K panel at 200% scaling reports 3840x2160 and every coordinate the platform takes or returns is one physical pixel. A program that is not DPI-aware — a Python script that never called `SetProcessDpiAwareness`, for one — is told the scaled size instead (1920x1080 for the same panel), so coordinates it measured are that factor too small here unless the display was at 100%.

### macOS {#host-screen-size-macos}

**Points.** The same panel reports 1920x1080 on a Retina Mac; a capture is downsampled so that one pixel of a captured image is one point, and no coordinate can address a single backing pixel.

This is the difference most likely to waste a day. Coordinates and template images calibrated on a HiDPI Windows machine are exactly **twice** the numbers a Retina Mac needs, and nothing reports it — the click simply lands half a screen away.

## host.screen.profile(opts?, cb?) {#host-screen-profile}

**Signature:** `host.screen.profile(opts: { region: Region?, axes: ("both" | "columns" | "rows")?, dark: number?, snapshot: Snapshot? }?) -> (Profile?, Error?)`, or with a callback `host.screen.profile(opts: { region: Region?, axes: ("both" | "columns" | "rows")?, dark: number?, snapshot: Snapshot?, key: string?, at: number? }?, cb: (profile: Profile?, err: Error?, info: Info) -> ()) -> ()`, where `Info = { time: number?, inputEpoch: number? }`, `Profile = { x, y, w, h, columns: Axis?, rows: Axis?, time: number, inputEpoch: number }` and `Axis = { min: number[], max: number[], dark: number[], mean: number[], r: number[], g: number[], b: number[] }`

Takes **one** capture of `opts.region` and reduces each of its columns and rows to a few statistics — the darkest pixel, the lightest, how many pixels are darker than `opts.dark`, the mean, and the mean of each colour channel. The region is corners or a window region (see [Region form](#region-form)); **omitted, it profiles the whole primary screen**. With `opts.snapshot` it profiles the part of the region inside that [snapshot](#reading-a-snapshot) instead, capturing nothing: omitted, the region is then the whole snapshot, `x`, `y`, `w`, `h` are the part profiled, a region with no part inside it answers `nil, err` (`outside`, `the region does not overlap the snapshot`), and the call is not counted in the observation line. Luminance is ITU-R BT.601. `min` and `max` are whole numbers 0–255 because they are particular pixels; the means are **fractional** — a mean over hundreds of rows moves by less than one unit when something note-sized changes inside it, and rounding would floor exactly the signal you are looking for.

`dark` is a luminance threshold, 0–255, default `128`; each axis's `dark` sequence counts the pixels of that column (or row) whose luminance is below it. For finding a **shape** it is the statistic that works where the other two do not: a mean over hundreds of rows dilutes a note-sized object to a couple of units, and a minimum saturates because a busy column already contains something black. A count is proportional to how much of the column the object covers — Melodyne finds its notes with `dark = 190`. A value outside 0–255 silently falls back to `128`, and a fraction is cut to the whole number below it.

Index `1` is the left (or top) edge of the region, so a column index maps back as `x + i - 1`. `x`, `y`, `w`, `h` are the **requested** rectangle — for a window region, the screen rectangle it resolved to: nothing is clipped to the desktop, and a region that runs off the screen comes back with black in the part that is not there. `time` is [`host.now()`](./timer.md#host-now) when the picture was taken and `inputEpoch` [`host.inputEpoch()`](./timer.md#host-inputepoch) once it had been — when the `screen-capture` thread's capture came back; on a snapshot, the snapshot's own. It answers `nil` and an error value when there is nothing to profile this time: an empty region, a window with nothing to read, a limit, a budget with no room, or a capture that failed ([Returns](#host-screen-profile-returns), [Errors](#host-screen-profile-errors)). Corners past 40,000,000 pixels, a region in neither form and an `axes` it does not know raise, before anything is captured — and so does a `key` or an `at`, which only the [callback form](#profile-with-a-callback) takes.

**In a [handler](#where-it-waits) it waits**: its picture is taken on the `screen-capture` thread as a plain [snapshot with a callback](#snapshot-with-a-callback) is — none for a snapshot — and reduced on the image worker, as [with a callback](#profile-with-a-callback), while only its module waits; the handler goes on with the profile, built on the event loop as it does. It is one of the module's snapshot requests while it waits, and its picture is charged from the call until the answer, as with a callback (**Limits and the budget**, below) — with 64 waiting in the whole application it answers `nil, err` (`too_many`). Where it cannot wait it raises, and nothing is captured or reduced ([Where it waits](#where-it-waits)). A region with nothing to read this time — a window region with no rectangle, an empty region, a region outside its snapshot — is answered at once wherever it is asked.

Use it to find something whose **position** you do not know: a vertical line is a dark column in a light band, an edge is where the mean steps, a shape's extent is where its run of changed columns begins and ends, and a differently coloured patch shows up in `r`/`g`/`b` while the luminance stays flat. For a feature much smaller than the region, prefer `min`/`max` over `mean`: a two-pixel dark line moves a column's minimum by everything and its mean by a little.

**Why this rather than many `pixel` calls:** through the standard path on Windows a screen capture costs a fixed compositor frame — measured on the reference machine at 16.6 ms for 55×27 and 16.7 ms for 1028×666 — and `host.screen.pixel` costs the same 16.7 ms *for one pixel*. Four point probes are four frames; a profile of the whole window is one. The **capture** is size-independent, but this call's reduction is not: it walks every pixel and builds seven sequences for each axis asked for — on the image worker, where only the sequences are the loop's ([With a callback](#profile-with-a-callback)) — so profile the band you need rather than the screen. `axes` skips the half you do not want.

**Cost** in a handler: on the event loop, reading the arguments and the charge, microseconds, and the tables when the answer comes; the module waits, from asking to the answer, for the capture, those ahead of it on the `screen-capture` thread, the reduction and the batch ahead of it on the image worker, and two turns of the event loop — the one that hands the picture to the image worker and the one that hands the profile over, every 15 ms. On a snapshot there is no capture, and one turn.

```luau
-- Where are the dark vertical lines in this strip? Asked from a key, whose handler waits for the
-- profile while only this module waits. Needs "screen", "hotkey", "window" and "log".
host.hotkey.register("Ctrl+Alt+Shift+L", function()
    local x, y = 100, 200
    local p = host.screen.profile({ region = { x, y, x + 400, y + 20 }, axes = "columns" })   -- waits here
    if p then
        for i, m in ipairs(p.columns.min) do
            if m < 160 then
                host.log.info("line at screen x " .. (p.x + i - 1) .. ", " .. (host.now() - p.time) .. " ms ago")
            end
        end
    end

    -- The same strip of a window, as fractions of its client area: follows the window.
    local w = host.window.active()
    if w then
        local q, err = host.screen.profile({ region = { window = w, fraction = { 0, 0.9, 1, 1 } }, axes = "columns" })
        if not q then host.log.info("not profiled: " .. err) end
    end
end)
```

### With a callback {#profile-with-a-callback}

Given a function as its second argument, the call returns at once — nothing — wherever it is made, never waits, and nothing on the event loop touches the screen or walks the pixels: the picture of `opts.region` is taken on the `screen-capture` thread as a plain [snapshot with a callback](#snapshot-with-a-callback) is, at once or at `at` (see the top of this page), through the module's [source](#which-picture-a-read-sees); with `opts.snapshot` nothing is captured. The reduction then runs on the image worker, and only the answer's Luau tables are built on the event loop, as the callback runs. Without a callback — `nil` as the second argument included — the call is the one above, which waits in a handler and where a `key` or an `at` raises, and nothing on this page about the callback applies. A poll profiles with a callback, so that it waits nowhere and its module's keys never wait for it. What every call given a callback shares is under [With a callback](#with-a-callback); what follows is `profile`'s own.

**The callback** runs exactly once, on the event loop, on the loop's next turn after the image worker answered (every 15 ms), as a [handler](../module-runtime-and-lifecycle.md#handlers) of its module: at once while the module is free, after the events before it in its mailbox while it is busy. It gets `cb(profile, nil, info)`: the table the call without a callback returns for the same picture and options — the same `x`, `y`, `w`, `h`, axes and statistics, and its `time`, [`host.now()`](./timer.md#host-now) when the picture was taken, and `inputEpoch`, [`host.inputEpoch()`](./timer.md#host-inputepoch) once it had been taken; on a snapshot both are the snapshot's own (see **How old a picture is** under [Waiting for the picture to change](#waiting-for-the-picture-to-change)) — and `info` with the same two. When there is nothing to profile it gets `cb(nil, err, info)`, `info` empty: every error value the call without a callback answers — on the next tick, for one known at the call — and `replaced` for its `key` ([Errors](#host-screen-profile-errors)). It runs with the priority of the dispatch that asked (see [`host.ocr.recognize`](./ocr.md#host-ocr-recognize), "Order and priority"). It is **dropped** — never called — when the module is disabled, reloaded, removed or [stopped by the guard](../module-runtime-and-lifecycle.md#limits) first, wherever the request is then: its picture is no longer taken, and the image worker skips its reduction if it has not begun it. At exit, requests still waiting are dropped. A callback that raises is reported like any other module callback error; a module whose VM runs out of memory while the answer's tables are built in it is [stopped](../module-runtime-and-lifecycle.md#limits), as for a text read's reading.

**`key`.** A newer `profile` with a callback of the same module VM with the same `key` answers every older one with that key `nil, err` (`replaced`, `a newer request with the same key replaced this one`) on the next tick — also one whose picture is taken and is being reduced, which the image worker then skips if it has not begun it, and one whose answer waits in the module's mailbox. Keys here are `profile`'s own: a [`snapshot`](#snapshot-with-a-callback) or a [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) with the same key never replaces one, nor the other way round. [`host.screen.pending(key)`](#host-screen-pending) is `true` while one is out. Without a key a request is never replaced.

**`at`** is a time on `host.now()`'s clock, as a snapshot's is ([Taken at a set time](#taken-at-a-set-time)): the picture is not taken before it; one already past is taken at once. A plain one — no `at` — is taken as soon as the thread can, as a plain snapshot with a callback is, and holds nothing back: a click its module makes before then may be in it, and the profile's `inputEpoch` says so; act on it in the callback.

**On a snapshot** the region is cut to the snapshot at the call, as without a callback, and the request holds the snapshot's pixels until its reduction has run: the module may release the snapshot at once, and the profile is still of it. The request charges nothing for them: they are the snapshot's charge while the module holds it, and once it is released the request holds them uncharged (see **Lifetime** under [`snapshot`](#host-screen-snapshot)).

**Limits and the budget.** From the call until the image worker has answered it — while its picture is taken and while it is reduced — a profile with a callback, and one a handler waits for, is one of its module's [snapshot requests](#snapshot-with-a-callback): 16 per module VM and 64 in the application, the oldest ended or the newest refused with the error values listed there. Its picture is charged against the [snapshot budget](#host-screen-snapshot) as a plain snapshot request's is — `w * h * 4` bytes, five times that on macOS, settled to what the picture holds — from the call until its callback is called, a wait in the module's mailbox included, and given back at once when the request is ended or dropped; the host steps no collector for it, since no handle is left behind for one to find. A profile of a snapshot is charged nothing.

**What raises**, at the call: everything that raises without a callback, a `key` and an `at` aside, and a second argument that is neither a function nor `nil` (`host.screen.profile: the callback (second argument) must be a function, got 5`), a `key` that is not a non-empty string (`host.screen.profile: opts.key must be a non-empty string, got 5`), an `at` that is not a number or is more than 2000 ms after `host.now()`, and an `at` beside `snapshot` (`host.screen.profile: opts.at belongs with a region to capture; the snapshot's picture is taken already`). A window region that is past the pixel limit at the window's current size or whose client area is empty, a region the budget has no room for, and an empty region, are answered `nil, err` on the next tick instead, without a capture.

**Cost.** On the event loop: reading the arguments and the charge, microseconds, then, when the answer comes, its tables: seven sequences per axis asked for. Measured in a debug build of the host's tests on Windows (2026-10-09), for Melodyne's note area — 910 × 522 pixels, `axes = "columns"` — the reduction took 30–35 ms, now on the image worker, and the tables 0.5–0.8 ms, which is all the event loop pays; a release build has not been measured. The capture is the `screen-capture` thread's, as a snapshot request's is — the [first-read comparison](#which-picture-a-read-sees) of a module that reads through desktop duplication there too, before it — and shares a capture with other requests due together as those do; the reduction is the image worker's, one batch at a time with every module's searches. Each answer is counted in the log's `[observe]` line for the epoch its callback runs in: `…; profile with a callback ×2: tables 1.2 ms on the event loop, reduction 30.4 ms on the image worker (1 of a snapshot)`. A profile a handler waited for is not counted there; the capture thread's slow rounds have lines of their own.

The same strip read twice a second off the event loop, one request out at a time:

```luau
-- The capture and the reduction are the host's threads'; the callback gets the profile and the
-- moment its picture was taken. Needs "screen", "timer" and "log" in the manifest.
local STRIP = { 100, 200, 500, 220 }
host.timer.every(500, function()
    if host.screen.pending("strip") then return end   -- the last one has not answered yet
    host.screen.profile({ region = STRIP, axes = "columns", key = "strip" }, function(p, err)
        if not p then
            host.log.info("not profiled: " .. err)
            return
        end
        for i, m in ipairs(p.columns.min) do
            if m < 160 then
                host.log.info(("line at screen x %d, %d ms ago"):format(p.x + i - 1, host.now() - p.time))
            end
        end
    end)
end)
```

### Returns {#host-screen-profile-returns}

- A `Profile`, `{ x, y, w, h, columns?, rows?, time, inputEpoch }` — one value, and nothing after it. `columns` and `rows` are there as `axes` asked for them.
- `nil, err` — there was nothing to profile this time; `err.code` is one of the codes under [Errors](#host-screen-profile-errors). Nothing is charged for it.
- With a callback: nothing at once; `cb(profile, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#host-screen-profile-errors}

None of these is raised. Each comes as an error value where "arrives" says. What the region decides at the call — an empty region, a window with nothing to read, a region outside its snapshot — is answered at once, wherever the call is made; given a callback, on the next tick.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. | `the window's client area is empty (…x…)` | both |
| `outside` | both | Corners with no width or no height, turned around or equal, which it has always refused before capturing; a side below none counts as none. Or a window region lies outside the coordinate range, which a window table from `host.window` never does. | `the region is empty (…x…)`, `the region lies outside the screen's coordinate range` | both |
| any of the [snapshot-source errors](#snapshot-source-errors) | both | With `opts.snapshot`, no part of the region is inside the snapshot: `outside`, `the region does not overlap the snapshot`. | `the region does not overlap the snapshot` | both |
| `too_large` | both | A window region past 40,000,000 pixels at the window's current size. The same corners raise. | `at the window's current size the region is …x…, … pixels; the limit is 40000000` | both |
| `too_large` | both | The [snapshot budget](#the-snapshot-handle), which charges the picture while it is held, has no room for it: this module's 128 MiB, or the application's 512 MiB, which other modules' snapshots count toward. For corners too. For a window region the message begins `at the window's current size `, which is the leading `…`. A profile of a snapshot is charged nothing. | `…this module's snapshots would hold … MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()`, `…the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()` | both |
| `too_many` | both | 64 requests wait in the whole application, so this one is not taken; or this was the module's oldest of 16 when it asked for one more ([Request-limit errors](#request-limit-errors)). | `too many snapshot requests waiting in the whole application (64); this one was not taken`, `too many snapshot requests waiting for this module (16); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked again with the same key. | `a newer request with the same key replaced this one` | both |
| `cancelled` | both | The capture thread stopped the request, which no path of the host does. | `the request was cancelled` | both |
| `failed` | both | The reduction failed inside the host, or the picture could not be handed to the image worker because the application is closing. The worker goes on. | `internal error: the profile could not be made` | both |
| `failed` | both | The capture came back shorter than its own size, which a capture should never do. | `the capture came back shorter than its own size` | both |
| any of the [capture errors](#capture-errors) | both | The picture could not be taken. | `screen capture failed` | both, other |
| `failed` | both | The `screen-capture` thread's own code failed while it took the picture. The thread goes on. | `the screen capture failed inside the application; the log has the details` | both |
| `failed` | both | The host's own code failed for this profile after its round's capture. The rest of the round is answered as usual. | `the snapshot failed inside the application; the log has the details` | both |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: `opts` that is neither a table nor `nil` (`host.screen.profile: opts must be a table, got …`); an `opts.region` that is neither region form; corners whose picture would be more than 40,000,000 pixels (`host.screen.profile: opts.region: the region is 10000x5000, 50000000 pixels; the limit is 40000000`); an `axes` that is not exactly `"both"`, `"columns"` or `"rows"`; an `opts.snapshot` that is not a Snapshot or was released. Without a callback, a `key` or an `at` of any value but `nil`, `key` asked about first (`host.screen.profile: opts.key is only taken with a callback (the second argument)`, and for `at` alone `host.screen.profile: opts.at is only taken with a callback (the second argument)`). With a callback, a `key` that is not a non-empty string, an `at` that is not a number or is more than 2000 ms after `host.now()`, an `at` beside `snapshot`, and a callback that is neither a function nor `nil`. A `dark` outside 0–255 does not raise: it is read as `128`.

**Raises where it cannot wait**: without a callback, inside a function that may not wait, unless there is nothing to profile this time; see [Where it waits](#where-it-waits).

### Windows {#host-screen-profile-windows}

The capture goes through the module's [source](#which-picture-a-read-sees): in a handler and with a callback on the `screen-capture` thread, as a [snapshot request](#snapshot-with-a-callback)'s does, waiting up to 250 ms for desktop duplication there. Through the standard path it is the fixed compositor frame above; through desktop duplication the capture measured 0.3–5 ms on an idle desktop, by size. The reduction comes on top, in proportion to the area — in a handler and with a callback, on the image worker. With a game in front, a whole call over a 1280×1024 client area with `axes = "columns"` — capture and reduction together — took 6–11 ms through duplication, where the same call through the standard path took about 23 ms; how much longer a wait for it takes from asking to the answer has not been measured. Add any wait for duplication (see [How long a read waits](#which-picture-a-read-sees)). Coordinates and the profile are in device pixels.

### macOS {#host-screen-profile-macos}

The capture is in points (see [`host.screen.size`](#host-screen-size)). In a handler and with a callback it is the `screen-capture` thread's, taken at the display's own resolution and drawn down to points, which it keeps beside its points until the reduction has run, as a snapshot request's does: a request is charged 20 bytes a point. The image worker's captures for searches, the nearest measured, took about 19 ms per batch in the fifth macOS session. Without the Screen Recording permission the capture does not fail: the profile is of the desktop wallpaper, not `nil`.

## host.screen.pending(key) {#host-screen-pending}

Whether a request of this module with `key` is still out: a call on this page given a callback and `opts.key` ([With a callback](#with-a-callback)) — a [`snapshot`](#snapshot-with-a-callback), a [`profile`](#host-screen-profile), an [`imageSearch`](#host-screen-imagesearch) and the rest, and an [`imageSearchEach`](#host-screen-imagesearcheach) —, asked and not answered yet.

**Signature:** `host.screen.pending(key: string) -> boolean`

- `true` from the call that asked until its callback runs: while its picture is taken, while the image worker works on it, while an answer decided without a picture — a newer request with the key, a limit, a window region with nothing to read — waits for the next tick, and while the answer waits in the module's mailbox behind the module's other events. `false` inside the request's own callback, unless another request with the key is out, and after the module was disabled, reloaded, removed or stopped by the guard, which drops its requests.
- Asks about every call at once: a `snapshot`, a `profile` and an `imageSearch` with the same key all count, though none replaces another. Asks about the module VM's own requests only: another module's with the same key are not counted. A text read's key is [`host.ocr.pending`](./ocr.md#host-ocr-pending)'s.
- Costs a look through the module's requests on the event loop, microseconds — the image worker's up to 64 of them too; it captures nothing.

It is "one request at a time" without a flag: a flag set before a request stays set when a disable drops the request and its callback never comes, while a dropped request is simply not pending. A handler that waits — for a [text read](./ocr.md#where-it-waits) — until `pending(key)` turns `false` while the answer is behind it in its own mailbox waits for good.

```luau
-- A menu's picture five times a second, never two at once. Needs "screen", "timer" and "log".
local MENU = { 96, 120, 544, 400 }
host.timer.every(200, function()
    if host.screen.pending("menu") then return end   -- the last picture has not been answered yet
    host.screen.snapshot({ region = MENU, key = "menu" }, function(snap, err)
        if not snap then
            host.log.info("no picture: " .. err)
            return
        end
        local px = host.screen.pixels({ { 120, 140 } }, { snapshot = snap })
        if px then host.log.info("lamp " .. px[1].hex) end
        snap:release()
    end)
end)
```

### Returns {#host-screen-pending-returns}

A boolean: `true` while a request of this module VM with `key` is out, as above, and `false` otherwise.

### Errors {#host-screen-pending-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `key` that is not a string (`host.screen.pending: the key must be a non-empty string, got nothing`), or is empty (`host.screen.pending: the key is empty`).

### Windows {#host-screen-pending-windows}

The same on every platform: it reads the host's own record of the module's requests.

### macOS {#host-screen-pending-macos}

The same on every platform: it reads the host's own record of the module's requests.

## host.screen.template(spec, cb?) {#host-screen-template}

**Signature:** `host.screen.template(spec: TemplateSpec) -> (Template?, Error?)` where `TemplateSpec = { name: string?, rgba: (string | buffer)?, rgb: (string | buffer)?, w: number?, h: number?, capture: Region?, file: string?, snapshot: Snapshot?, key: string? }` — `name` plus **exactly one** of `rgba`, `rgb`, `capture` or `file`, `snapshot` only beside `capture`, and `key` only with a callback — and `Template = { w: number, h: number, name: string?, count: number? }`, read-only; with a callback `host.screen.template(spec: TemplateSpec, cb: (template: Template?, err: Error?, info: Info) -> ()) -> ()`, where `Info = { time: number?, inputEpoch: number? }`

Builds a template in memory, for every search below to take wherever it takes a path. A template no longer has to be a PNG file in the module's folder: its pixels can come from bytes the module computed or read from a data file, or from the screen itself, now.

- **`rgba`** with `w` and `h`: `w * h * 4` bytes, row-major from the top-left, in R, G, B, A order. As in a PNG, **alpha 0 is a wildcard** and any other alpha is compared in full.
- **`rgb`** with `w` and `h`: `w * h * 3` bytes; every pixel is compared.
- **`capture`**: the given [region](#region-form) of the screen — corners, or a window region resolved at the call — captured now: in a [handler](#where-it-waits) on the image worker, which the handler waits for while only its module waits, and given a callback there too while nothing waits; where it cannot wait it raises. Every pixel is compared (alpha is forced opaque). It answers **`nil` and an error value** when the capture fails, and when a window region's window has nothing to capture this time: an empty client area, or one so large that the region is past the size limit below ([Errors](#host-screen-template-errors)). Those are the runtime conditions rather than mistakes, so they do not raise; a `capture` table that is neither region form raises.
- **`capture` with `snapshot = s`**: the region is cut out of that [snapshot](#reading-a-snapshot) instead, and nothing is captured — in a handler on the image worker, as a capture is. It must lie wholly inside the snapshot — a template cut from part of the region would be a different template — or the call returns `nil, err` (`outside`, `the region is not inside the snapshot`) at once, wherever it is made, and the log says nothing; a corner left out is the snapshot's edge. `snapshot` beside anything but `capture` raises (`host.screen.template: snapshot belongs with capture: …`), as does one that is not a Snapshot or was released.
- **`file`**: a PNG — the one image format the host is built with — resolved exactly as a path given to a search is (see [the path rule](./index.md#paths)). Its header is read at the call, for `w` and `h`: a file inside the module's package that is not there, or not a PNG, raises `cannot open template '…': …` there; one outside the package is answered `nil, err` instead — `not_found`, `invalid` or `failed` ([Errors](#host-screen-template-errors)), since the module's own package is what its author ships. Its pixels are decoded on the image worker, through the cache every search's paths share, and the handle shares that decode. The handle is a **snapshot**: it keeps that decode for as long as it lives, however many other paths pass through the cache, and a re-captured PNG is picked up live only by a search that is given the path. A file whose header reads and whose pixels do not decode, or that is gone by the time the picture is made, makes every search given the handle answer `nil, err` — `not_found`, `invalid` or `failed`, `cannot open template '…': …` ([`imageSearch`'s errors](#host-screen-imagesearch-errors)). That is not kept: each such search tries the file again, so a file that was half written, or held by another program, at that moment gives the handle its picture once it can be read — and from then on the handle keeps it.

**Bytes and a file are answered at once, their picture made afterwards.** The call checks what it can without converting or decoding anything — the sizes and the byte count, or the file's header — and returns the handle; its picture is made on the image worker: the bytes converted to the matcher's RGBA, or the PNG decoded, and the pixels a search compares counted. That is queued as the handle is made, ahead of any search that uses it, and a search that needs the picture before then makes it itself, so no search ever sees a template half made. Until then `count` is `nil` and `tostring(t)` says `preparing` — `Template(pack 10x36, preparing)`; after, `Template(pack 10x36, 360 compared)`, or, once the image worker or a search has tried a file that cannot be opened or whose pixels do not decode, `Template(x.png 10x36, cannot be used: cannot open template '…': …)` until a search that tries it again can. `w` and `h` are known from the call: the bytes' own, or the header's, then the decoded picture's. A `capture` is made a template before the handle is answered, its `count` known at once.

Bytes are a Luau `string` or a `buffer`, copied once. A handle built from bytes may be at most 4096 pixels on a side and 1,048,576 pixels in all (1024x1024); a `capture` given as corners raises before anything is captured when its region is larger than that, or empty, while a window region past the limit is answered `nil` and an error value (`too_large`), since the window's size decides it. `name` (at most 64 characters) comes back in every hit the template makes and in `tostring(t)`; a `file` template given none is named by its path as written. `count` is the number of pixels a search compares, `nil` while the picture is still to be made — and while a file's cannot be made, until a search that tries it again can: `tostring(t)` tells the two apart, `preparing` or `cannot be used: …`. Everything else raises, naming the field: a wrong byte count (`rgba is 1436 bytes; 10x36 RGBA needs 1440`), two sources or none, `w`/`h` given to a `capture` or a `file`, and any field this version does not know, so a misspelt `rbga` or a `tolerance` it does not take is never silently ignored. Tolerance and scales are the search's options, as for a path.

A handle is the one value returned, with nothing after it. A template belongs to the module VM that built it, like any Luau value. It cannot be passed through a legacy data export or stored in `host.settings`. One VM's handles built from bytes or a capture may hold **32 MiB** between them; over that, the collector is run and, if they still do not fit, the call answers `nil, err` (`too_large`, `this module's templates would hold … MiB; the limit is 32 MiB per module VM. Build templates once, when the module loads, rather than on every poll.`) — whatever the source, since what the module holds now decides it. Every such handle is held and charged as RGBA — 4 bytes a pixel, an `rgb` one included — plus a fixed amount per handle (the structure and 48 bytes of match order), so an RGB pack costs about four thirds of its own size. The budget is per VM: a `code_module` runtime that builds a pack while it is evaluated inside each game module's VM pays for it once per VM. `file` handles cost nothing against it. The budget is charged at the call, from `w` and `h`, before the picture is made. Build templates once, not on every poll: one from bytes or a file when the module loads, a `capture` once what it learns is on screen, in a handler or with a callback (see Windows below for a module that reads through desktop duplication). A template from bytes or a file never waits, wherever it is made.

**Given a callback** — `template(spec, function(t, err, info) … end)` — the call waits nowhere: a `capture` is made on the image worker, and its handle, or `nil` and the error value, is handed to the callback on a later tick, `info` with the moment of its picture (a snapshot's own when cut from one); any other spec is answered on the next tick, `info` empty ([With a callback](#with-a-callback)). The spec's `key` is the request's key; the call without a callback raises for one (`host.screen.template: key is only taken with a callback (the second argument)`).

**Cost.** On the event loop: for bytes, the checks and one copy of the bytes out of the VM; for a file, the path resolved and its header read — the file opened and its first few kilobytes read, nothing decoded. On the image worker: the conversion of bytes, about one pass over the pixels plus the count, and a PNG decode, which grows with the file (a 1209x987 PNG is the largest the repository ships); both happen once per handle, and a PNG once per path until the file changes. A `capture` costs its capture as below, on the image worker in a handler and given a callback.

```luau
-- A pack a converter wrote as JSON: each signature a name, a size and its RGB bytes as
-- numbers. Decoded once, when the module loads; each template keeps its name for its hits.
-- The calls return at once; the image worker converts the bytes meanwhile.
local pack = host.json.decode(host.resource.read("data/gamepack.json"))
local T = {}
for _, sig in ipairs(pack.signatures) do
  local bytes = buffer.create(#sig.rgb)
  for i, v in ipairs(sig.rgb) do buffer.writeu8(bytes, i - 1, v) end
  T[sig.name] = host.screen.template({ name = sig.name, rgb = bytes, w = sig.w, h = sig.h })
end
-- Template(start 40x12, preparing) — or "480 compared" once the worker has made it.
local first = pack.signatures[1]
if first then host.log.info(tostring(T[first.name])) end
```

```luau
-- Learn the selection marker once, in this machine's own pixels, then follow it. A capture
-- can come back nil, so a nil is not kept: the next call asks the screen again. Learned in a
-- hotkey's handler, where the capture waits. Needs "screen", "hotkey" and "log".
local marker = nil
local function learnedMarker()
  if marker == nil then
    local err
    marker, err = host.screen.template({ name = "marker", capture = { 120, 80, 132, 92 } })   -- waits here
    if marker then
      host.log.info("learned " .. tostring(marker))   -- Template(marker 12x12, 144 compared)
    else
      host.log.info("marker not learned yet: " .. err)
    end
  end
  return marker
end

host.hotkey.register("Ctrl+Alt+Shift+K", function()
  local m = learnedMarker()
  local hit = m and host.screen.imageSearch(m, { tolerance = 8 })
  if hit then host.log.info("marker at " .. hit.x .. "," .. hit.y) end
end)
```

### Returns {#host-screen-template-returns}

- A `Template` — one value, and nothing after it. From bytes or a file it comes at once, its picture still to be made: `count` is `nil` until it is, and a file whose picture cannot be made then is answered by the searches given the handle, not here.
- `nil, err` — no template this time; `err.code` is one of the codes under [Errors](#host-screen-template-errors). Nothing is charged for it.
- With a callback: nothing at once; `cb(template, nil, info)` or `cb(nil, err, info)` on a later tick — a `capture`'s once the image worker made it, any other on the next tick.

### Errors {#host-screen-template-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A `capture` given as a window region, and the window's client area is empty: a minimised window, or one not laid out yet. Answered at once. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A `capture` given as a window region lies outside the coordinate range, which a window table from `host.window` never does. Answered at once. | `the region lies outside the screen's coordinate range` | both |
| `outside` | both | A `capture` with `snapshot = s` that is not wholly inside `s` ([snapshot-source errors](#snapshot-source-errors)). Answered at once, and the log says nothing. | `the region is not inside the snapshot` | both |
| `too_large` | both | A `capture` given as a window region past a template's size limit at the window's current size: a side past 4096, or more than 1,048,576 pixels. The same corners raise. | `at the window's current size …x… is too large; no side may exceed 4096`, `at the window's current size …x… is … pixels; the limit is 1048576` | both |
| `too_large` | both | The module's templates from bytes or a capture would hold more than 32 MiB, after its collector ran; whatever the source, since what the module holds now decides it. For a `capture` given as a window region the message begins `at the window's current size `, which is the leading `…`. | `…this module's templates would hold … MiB; the limit is 32 MiB per module VM. Build templates once, when the module loads, rather than on every poll.` | both |
| `not_found` | both | A `file` outside the module's package that is not there. Inside the package it raises instead. | `cannot open template '…': …` | both |
| `invalid` | both | A `file` outside the module's package whose header is not a PNG's. | `cannot open template '…': …` | both |
| `failed` | both | A `file` outside the module's package that could not be read: held by another program, or no right to read it. | `cannot open template '…': …` | both |
| `failed` | both | A `capture` came back shorter than its own size, which a capture should never do. | `the capture came back shorter than its own size` | both |
| `failed` | both | A `capture` failed to be made a template inside the host, or could not be handed to the image worker because the application is closing. | `internal error: the template could not be made` | both |
| `too_many` | callback | A `capture` given a callback: the module had 64 of the image worker's calls with a callback waiting, and asked for one more, and this was its oldest ([Image-worker errors](#image-worker-errors)). | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With the spec's `key`: the module asked again with the same key. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | A `capture` whose picture could not be taken. | `screen capture failed` | both, other |
| any of the [duplication errors](#duplication-errors) | both | A `capture`, only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: a spec that is not a table; a field this version does not know, or a field name that is not a string; a `name` that is not a string, or longer than 64 characters; two sources or none; `rgba` or `rgb` without `w` and `h`, a side outside 1–4096, more than 1,048,576 pixels, a wrong byte count (`rgba is 1436 bytes; 10x36 RGBA needs 1440`), or bytes that are neither a string nor a buffer; `w` or `h` given to a `capture` or a `file`; a `capture` that is not a table, or neither region form, or given as corners past the size limit or empty; `snapshot` beside anything but `capture` (`host.screen.template: snapshot belongs with capture: …`), or one that is not a Snapshot or was released; a `file` that is not a string, or a file inside the module's package that is not there or not a PNG (`cannot open template '…': …`); a `key` without a callback (`host.screen.template: key is only taken with a callback (the second argument)`), or one that is not a non-empty string; a callback that is neither a function nor `nil`.

**Raises where it cannot wait**: without a callback, a `capture` with something to capture, inside a function that may not wait; see [Where it waits](#where-it-waits). Bytes and a file never wait.

### Windows {#host-screen-template-windows}

Sizes and coordinates are device pixels (see [`host.screen.size`](#host-screen-size)). A `capture` is one screen capture — through the standard path measured at a fixed ~16.7 ms whatever the region's size — on the image worker in a handler, which the module waits for along with the batch ahead of it there, and given a callback; so learn a captured template once, not on a poll. Without a `snapshot` it is always a live read of the screen, never an earlier frame; with one it is that snapshot's picture, and costs a copy of the region's pixels. It reads through the module's [source](#which-picture-a-read-sees) like every other read, and in a module that declares `capture = "duplication"` that alone does not make it the picture the module's searches will see: a `capture` made before duplication has opened — one made at load, or just after the module's window came forward — may be read the standard way, or answer `nil, err` under `fallback = "none"`, when the opening takes longer than the read waits for it, and the template keeps whichever picture it got for as long as the module keeps it. Under `fallback = "none"` a `pixel` that returned a colour was answered through duplication, so such a module learns its captured templates after one and asks again later when it gets `nil`; under the default fallback nothing tells a module which picture a read got. A game that is not DPI-aware is bitmap-stretched by the desktop compositor at any scaling other than 100% — Microsoft's own documentation says such applications "appear blurry" — so a template made at one scaling setting will not match pixel for pixel at another: give the search a `tolerance`, and one `scales` factor computed from the window's actual size rather than a ladder of guesses.

A template from bytes or a file is made on the image worker below normal priority, as its matching is (see the top of this page): on a busy machine its picture, and with it its `count`, comes later, and the first search that uses it makes it itself.

### macOS {#host-screen-template-macos}

The picture of a template from bytes or a file is made on the image worker, at the default priority, exactly as on Windows; a file's `w` and `h` are its PNG's pixels, which a search compares with a capture in points.

Captures are in **points** (see [`host.screen.size`](#host-screen-size)), so a template cut on a Windows machine at 200% scaling is twice the size of the same element on a Retina Mac; a `capture` template taken on the Mac is in the Mac's own resolution. Without the Screen Recording permission a capture does not fail — it is a picture of the desktop wallpaper — so a `capture` template made without the grant is a template of the wallpaper, and every search without the grant will find it there. The image worker's capture measured about 19 ms per batch in the fifth macOS session; a `capture` template is captured there, at point resolution, so build it once.

## host.screen.imageSearch(template, opts?, cb?) {#host-screen-imagesearch}

**Signature:** `host.screen.imageSearch(template: string | Template | {string | Template}, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot? }?) -> (Hit?, Error?)` where `Hit = { x: number, y: number, w: number, h: number, n: number, name: string? }`; with a callback `host.screen.imageSearch(template, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot?, key: string? }?, cb: (hit: Hit?, err: Error?, info: Info) -> ()) -> ()`, where `Info = { time: number?, inputEpoch: number? }`

Searches the screen for the first occurrence of `template` — an image path, a [`Template`](#host-screen-template), or a list of them — and returns its match rectangle in **screen pixels**: `x, y` is the top-left of the match and `w, h` the matched size (the template's own, or a scaled one — see `scales`). `n` is the index of the template that matched in a list, and `1` for a single template. `name` is the template's name, or the path exactly as you wrote it. It answers `nil` alone when it looked and did not find the template, and `nil` and an error value when it could not look: the region could not be captured, a template could not be opened, or a window region's window has an empty client area ([Returns](#host-screen-imagesearch-returns), [Errors](#host-screen-imagesearch-errors)). A hit is one value, with nothing after it. The search runs top to bottom, then left to right, and the first position that matches wins; there is no score.

- A path is a **PNG** — the one image format the host is built with; a `.bmp` or `.jpg` inside the module's package raises `cannot open template`, and one outside it is answered `invalid`. A relative path is resolved against the root of the module whose code makes the call (see [the path rule](./index.md#paths)); **absolute paths are also accepted**, and neither form is checked for `..`.
- `opts.region` restricts the search area: corners or a window region (see [Region form](#region-form)); default is the full primary screen. A table that is neither form raises.
- `opts.snapshot` searches the part of the region inside that [snapshot](#reading-a-snapshot) instead of the screen, capturing nothing; the region then defaults to the whole snapshot, and a hit is still in screen pixels. A region with no part inside it answers `nil, err` (`outside`, `the region does not overlap the snapshot`). The same holds for every search below.
- `opts.tolerance` is how far each colour channel may differ, 0–255, default `0` = exact. **A position matches only if every template pixel whose alpha is not 0 differs from the screen by at most `tolerance` in each of R, G and B.** A single pixel outside rejects the position. There is no score, correlation or percentage threshold, and the answer is the first position in row order, not the best one. A value outside 0–255 is read as `0`, an exact match, without a word, and a fraction is cut to the whole number below — keep it in range.
- `opts.scales` is a list of factors to try, **in the order given**, the first that matches winning; each resizes the template (bilinear) before searching. Omitted, the template is searched at its own size only. A factor that is not a positive finite number, that rounds the template to nothing, or that makes it larger than the region is skipped. Resizing blurs, so pair scales with a tolerance. A template keeps the variants it was resized to, keyed by their size — at most 16 of them and 16 MiB per template, the oldest going first — so a poll that passes the same scales resizes once; a variant larger than 16 MiB on its own is rebuilt on every call.
- **Template pixels with alpha = 0 are wildcards** — they are skipped during matching, so you can mask out irrelevant parts of the template. Any other alpha is compared in full: alpha is a mask, never a weight.
- **A list of templates** — paths and `Template` handles mixed as you like — asks one question drawn several ways: a toggle's on and off, or a glyph as two plug-in versions draw it. The region is captured **once** and the templates are tried against that one frame in list order, the whole region scanned for one before the next is tried, so when two would match, order decides and not position; the first that matches answers, `hit.n` saying which and `hit.name` its name. Separate searches would pay a capture each, and a toggle that changes mid-repaint could fall between two captures and come back as neither. Build the list in a known order and keep a parallel list of labels: `n` turns a hit back into meaning. Every path in the list is checked at the call, wherever it sits; an empty list raises (`host.screen.imageSearch: no template given; the list is empty`), and so does an entry that is neither a path nor a Template (`imageSearch: expected a path or a Template, got a number`).
- The options are read loosely, unlike a [template spec](#host-screen-template): a key the search does not know — a `threshold` carried over from another matcher, a misspelt `tolerence` — is ignored without an error, and the search runs exact. `opts` itself must be a table or nothing (`host.screen.imageSearch: opts must be a table, got …`).
- A path inside the module's package is checked the first time the module's code names it — any search, any module: the file opened and its PNG header read, nothing decoded — and the call raises `cannot open template '…': …` when it is not there or is not a PNG. After that the path is taken as it is. A path outside the package is not checked at the call: a file there that cannot be opened is the search's answer, below. The image worker decodes the file when a search first needs it and again whenever its modification time changes, so a template that calibration captures again is picked up by the next search, without a restart; meanwhile every search shares that one decode. The worker keeps the decodes of the 64 most recently used paths across the whole application, the least recently used going first: a module cycling through more templates than that has them re-read and re-decoded from disk, so it builds [`host.screen.template{ file = … }`](#host-screen-template) handles once at load instead, since a handle keeps its decode. A file gone since it was first named, one outside the package that is not there, or one whose header reads and whose pixels do not decode, is answered `nil, err` by the search — `cannot open template '…': …`, `not_found`, `invalid` or `failed` — not raised ([Errors](#host-screen-imagesearch-errors)). One template of a list that cannot be opened answers the whole search so.

**Where it runs.** In a [handler](#where-it-waits) the call waits while the image worker opens the template, captures its region — or reads its snapshot — and matches, and only its module waits meanwhile; at the call only the path is checked, the first time it is named. Where it cannot wait it raises, and nothing is opened, captured or matched ([Where it waits](#where-it-waits)).

**Given a callback** — `imageSearch(template, opts, cb)`, `opts` `nil` when there are none — it waits nowhere, wherever it is called: the image worker does the same work, a window region resolved at the call from the window table given, and `cb(hit, nil, info)` comes on a later tick — `cb(nil, nil, info)` when it looked and found nothing, `cb(nil, err, info)` when it could not look — `info` with the moment of the picture searched ([With a callback](#with-a-callback)). With `opts.snapshot` the search keeps the snapshot's pixels until it is answered, even when the snapshot is released meanwhile. A poll searches so, one search out at a time with `opts.key` and [`pending`](#host-screen-pending). The callback belongs to the module whose VM made the call — for code a `code_module` dependency runs inside your module, that is **your** module — and is dropped when that module is disabled: a poll that keeps one out with its key asks again at its first turn once the module is enabled, as the overlay runtime's landmark gate does. A VM with many polls has them all out at once — a sample-library module holds a landmark gate per library and per version of its host plug-in, each with a near-miss probe beside a miss — under the image worker's limit of 64 a module VM ([With a callback](#with-a-callback)); like any `cb(nil, err)`, a search that limit ended is "could not look", not "not there", and the overlay runtime keeps its last verdict for it and asks again at the next poll.

**Cost.** On the event loop at the call: the arguments and the region, and the first time a path is named its header — a file opened and its first few kilobytes read. On the image worker: a PNG decode the first time a path is searched for and after the file changed, the picture of a [`Template`](#host-screen-template) still to be made, the capture, and the match, which grows with the area searched and the number of scales.

**Porting from a score-based matcher** (OpenCV's `matchTemplate` with a 0.9 threshold, "95 % of pixels within X"): no such threshold carries over, because nothing here counts how many pixels agreed. Make the pixels that are allowed to differ wildcards instead — cut the template as `rgba` and give them alpha 0 — and pick the smallest `tolerance` at which the remaining pixels match on every rendering you have. For "which of these states is showing" use [`imageSearchEach`](#host-screen-imagesearcheach), which answers every template from one capture; for every position of one template use [`imageSearchAll`](#host-screen-imagesearchall).

```luau
-- On a key, whose handler waits for each search. Needs "screen", "hotkey", "window", "input" and
-- "log".
host.hotkey.register("Ctrl+Alt+Shift+P", function()
  local before = host.window.active()
  -- exact, whole-screen
  local m = host.screen.imageSearch("assets/play_button.png")   -- waits here
  -- Another window may have come forward while it waited: click only in the one the key was for.
  local now = host.window.active()
  if m and before and now and now.id == before.id then host.input.click(m.x + m.w // 2, m.y + m.h // 2) end

  -- with a search region and fuzzy match
  local hit = host.screen.imageSearch("assets/icon.png", {
    region = { x1 = 0, y1 = 0, x2 = 400, y2 = 300 },
    tolerance = 16,
  })

  -- in the left third of the active window's client area; nil alone is "not there"
  if now then
    local found, err = host.screen.imageSearch("assets/icon.png", { region = { window = now, fraction = { 0, 0, 1/3, 1 } } })
    if err and err.code ~= host.errors.HIDDEN then host.log.info("could not look: " .. err) end   -- minimised: say nothing
  end
end)
```

```luau
-- A dialog's close glyph as two versions of the plug-in draw it, looked for in one picture of
-- the window's top strip, off the event loop: the key's handler goes on at once, and `hit.n`
-- says which version matched. Needs "screen", "hotkey", "window", "input" and "log".
local VERSIONS = { "8", "7" }
host.hotkey.register("Ctrl+Alt+Shift+W", function()
  local w = host.window.active()
  if not w then return end
  host.screen.imageSearch({ "images/close-v8.png", "images/close-v7.png" },
    { region = { window = w, fraction = { 0.8, 0, 1, 0.1 } }, tolerance = 8 },
    function(hit, err)
      if err then host.log.info("could not look: " .. err) return end
      if not hit then return end   -- looked: no close glyph
      host.log.info("close glyph of version " .. VERSIONS[hit.n])
      host.input.click(hit.x + hit.w // 2, hit.y + hit.h // 2)
    end)
end)
```

### Returns {#host-screen-imagesearch-returns}

- A `Hit`, `{ x, y, w, h, n, name? }` — it looked and found the template: one value, and nothing after it.
- `nil` alone — it looked, and the template is not there.
- `nil, err` — it could not look; `err.code` is one of the codes under [Errors](#host-screen-imagesearch-errors). So test the second value to tell "not there" from "could not look": a poll keeps what it knew on `err`.
- With a callback: nothing at once; `cb(hit, nil, info)`, `cb(nil, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#host-screen-imagesearch-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. Nothing is captured. Answered at once, wherever the call is made; with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. Nothing is captured. Answered at once, as above. | `the region lies outside the screen's coordinate range` | both |
| any of the [snapshot-source errors](#snapshot-source-errors) | both | With `opts.snapshot`, no part of the region is inside the snapshot: `outside`, `the region does not overlap the snapshot`. Answered at once, as above. | `the region does not overlap the snapshot` | both |
| `not_found` | both | The image worker could not open a template's PNG because it is not there: gone since its path was first named, or outside the module's package. A `Template` built from such a file answers the same, and tries the file again at the next search. One template of a list that cannot be opened answers the whole search. | `cannot open template '…': …` | both |
| `invalid` | both | A template's PNG whose picture does not decode: cut short, not a PNG, or a format this host is not built with. | `cannot open template '…': …` | both |
| `failed` | both | A template's PNG that could not be read: held by another program, or no right to read it. | `cannot open template '…': …` | both |
| `failed` | both | A template's picture failed to be made inside the host: a `Template` built from bytes or a file, or a PNG whose decode failed. That search alone; the rest of its batch is read as usual, and the next search that names the file decodes it again. | `internal error: the template could not be made` | both |
| `failed` | both | The search's batch failed inside the host, or the search could not be handed to the image worker because the application is closing. The worker goes on. | `internal error: the search could not be made` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest ([Image-worker errors](#image-worker-errors)). It is "could not look", not "not there". | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | The region's picture could not be taken. | `screen capture failed` | both, other |
| `too_large` | both | A module that reads through desktop duplication asked for a region of more than 40 million pixels, with either fallback. | `screen capture failed: the region is larger than 40 million pixels` | Windows |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: a `template` that is neither a path, a Template nor a list of them, an empty list (`host.screen.imageSearch: no template given; the list is empty`), or an entry of one that is neither (`imageSearch: expected a path or a Template, got a number`); a path inside the module's package that is not there or not a PNG the first time it is named (`cannot open template '…': …`); `opts` that is neither a table nor `nil` (`host.screen.imageSearch: opts must be a table, got …`); an `opts.region` that is neither region form; an `opts.snapshot` that is not a Snapshot or was released; a `key` without a callback, or one that is not a non-empty string; a callback that is neither a function nor `nil`. Other keys of `opts` are ignored, and a `tolerance` outside 0–255 is read as `0`.

**Raises where it cannot wait**: without a callback, a search with something to read, inside a function that may not wait; see [Where it waits](#where-it-waits).

### Windows {#host-screen-imagesearch-windows}

The capture is one read through the module's [source](#which-picture-a-read-sees): through the standard path the fixed ~16.7 ms compositor frame whatever the region's size, through desktop duplication 0.3–5 ms on an idle desktop as measured there, plus any wait for duplication — at most 250 ms on the image worker, where a handler's search and one given a callback are made. The match follows on the image worker's matching pool — a snapshot's, which captures nothing, as well — and grows with the area searched and the number of scales. Searches in one batch of the image worker share a capture only when both the region and the source are the same, and the regions that go through desktop duplication are captured together, one request per `fallback` setting; while the worker waits for duplication it holds every read queued behind it — the image worker at the top of this page is one thread for every module. Coordinates are device pixels.

### macOS {#host-screen-imagesearch-macos}

Without the Screen Recording permission, macOS does **not** fail a capture — it hands back a picture of the desktop wallpaper. So a missing grant does not show up as an error or an empty result; it shows up as a search that never matches, or worse, matches something that was never on the plug-in. If every image search suddenly stops matching on a Mac, check the grant before the template: without it the answer is usually "not found" — `nil` alone, `cb(nil, nil, info)` given a callback — and an error value comes only for a region that cannot be captured at all (empty, too large, or wholly off the desktop) or a window region whose window had an empty client area. The capture is in points, on the image worker, each region one after the other. The image worker's capture measured about 19 ms per batch in the fifth macOS session.

## host.screen.imageSearchEach(entries, opts?, cb) {#host-screen-imagesearcheach}

**Signature:** `host.screen.imageSearchEach(entries: { string | Template | { template: string | Template, within: Region?, name: string? } }, opts: { region: Region?, tolerance: number?, scales: {number}?, snapshot: Snapshot?, key: string? }?, cb: (list: { Hit | false }?, err: Error?, info: Info) -> ()) -> ()`, `Info` as [`imageSearch`](#host-screen-imagesearch)'s

**One** capture of `opts.region` on the worker thread, and an answer for **every** entry: `cb(list, nil, info)`, where `list[i]` is entry *i*'s [hit](#host-screen-imagesearch) (with `n = i`) or `false`, and `info` has the moment of the picture. `cb(nil, err, info)` means it could not look — the region could not be captured, a template could not be opened, or a window region's window had an empty client area at the call ([Errors](#host-screen-imagesearcheach-errors)) — which a list of `false` never means. With `opts.snapshot` the one picture is that [snapshot](#reading-a-snapshot), as for `imageSearch`: the part of the region inside it is searched, and each `within` is cut to that part. A `within` left out is that whole part, as without a snapshot; a corner left out of a `within` that is given takes the snapshot's edge before the cut.

It is for the question "which of these states is showing?" — a menu whose cursor can be on one of several rows, a panel that shows one of several pages. `imageSearch` given a list answers only its first match; separate calls share a frame only by the accident of landing in the same batch of the image worker. Here one frame is the contract.

An entry is a path, a [`Template`](#host-screen-template), or a table naming one with two options: `within`, a [region](#region-form) this entry may be found in — corners, or a window region resolved at the call; it is cut to `opts.region`, and an entry whose `within` lies outside it, is smaller than its template, or is a window region whose window has an empty client area answers `false` — and `name`, which replaces the template's name in its hit. `within` is what keeps a state's template from inventing a hit elsewhere in the window, and what keeps the search cheap: matching, unlike the capture, costs in proportion to the area searched. Entries are matched in parallel. `tolerance` and `scales` apply to every entry. The callback is called, and dropped, as every callback on this page is, with `opts.key` and under the image worker's limit of 64 a module VM ([With a callback](#with-a-callback)).

**Read at the call, and what raises.** The entries, their templates and every region are read before the call returns, and a mistake raises there: an empty list (`imageSearchEach: no template given`); an entry that is not a path, a Template or a table with a `template` (`host.screen.imageSearchEach: entries[2] is a table without a template`); a template path that is not there or not a PNG, checked as [`imageSearch`](#host-screen-imagesearch) checks it — one that the worker cannot open after all is answered `cb(nil, err)` (`not_found`, `invalid` or `failed`, `cannot open template '…': …`); an `opts.region` or a `within` that is neither region form (`host.screen.imageSearchEach: entries[2].within is neither { x1, y1, x2, y2 } nor …`), or a `within` that is not a table; a `name` that is not a string; a `key` that is not a non-empty string; a `cb` that is not a function (`host.screen.imageSearchEach: the callback (third argument) must be a function, got nothing`). Otherwise an entry table is read loosely: a key other than `template`, `within` and `name` is ignored. The list ends at the first `nil`, as Luau's `ipairs` does, so the entries after a hole are not searched and have no slot in the answer.

```luau
-- Which of three menu pages is open? One capture of the game window per poll, each page's
-- template looked for only in its own strip, and never two searches in flight at once.
-- Needs "screen", "timer", "window" and "speech" in the manifest.
local GAME = { title = { contains = "My Game" } }
local PAGES = { "Items", "Magic", "Status" }
host.timer.every(250, function()
  local win = host.window.active()
  if not win or not host.window.test(GAME, win) then return end
  if host.screen.pending("pages") then return end   -- the last one has not answered yet
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
  host.screen.imageSearchEach(entries, { region = { window = win, fraction = { 0, 0, 1, 1 } }, tolerance = 12, key = "pages" },
    function(list, err)
      if list == nil then return end        -- could not look (`err` says why): keep what we knew
      for _, hit in ipairs(list) do
        if hit then host.speech.output(hit.name .. " menu") return end
      end
    end)
end)
```

### Returns {#host-screen-imagesearcheach-returns}

Nothing at once. On a later tick the callback is called once:

- `cb(list, nil, info)` — it looked: `list[i]` is entry *i*'s hit, with `n = i`, or `false` where that entry's template is not there, its `within` lies outside the region or is smaller than its template, or its `within` is a window region with nothing to read this time. A `false` is not an error.
- `cb(nil, err, info)` — it could not look, for any entry; `err.code` is one of the codes under [Errors](#host-screen-imagesearcheach-errors).

### Errors {#host-screen-imagesearcheach-errors}

None of these is raised. Each comes as an error value to the callback.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | callback | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. Nothing is captured. Answered on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | callback | A window region lies outside the coordinate range, which a window table from `host.window` never does. Nothing is captured. Answered on the next tick. | `the region lies outside the screen's coordinate range` | both |
| any of the [snapshot-source errors](#snapshot-source-errors) | callback | With `opts.snapshot`, no part of the region is inside the snapshot: `outside`, `the region does not overlap the snapshot`. Answered on the next tick. | `the region does not overlap the snapshot` | both |
| `not_found` | callback | The image worker could not open a template's PNG because it is not there: gone since its path was first named, or outside the module's package. A `Template` built from such a file answers the same, and tries the file again at the next search. One entry's template that cannot be opened answers the whole call. | `cannot open template '…': …` | both |
| `invalid` | callback | A template's PNG whose picture does not decode: cut short, not a PNG, or a format this host is not built with. | `cannot open template '…': …` | both |
| `failed` | callback | A template's PNG that could not be read: held by another program, or no right to read it. | `cannot open template '…': …` | both |
| `failed` | callback | A template's picture failed to be made inside the host: a `Template` built from bytes or a file, or a PNG whose decode failed. That search alone; the rest of its batch is read as usual, and the next search that names the file decodes it again. | `internal error: the template could not be made` | both |
| `failed` | callback | The search's batch failed inside the host, or the search could not be handed to the image worker because the application is closing. The worker goes on. | `internal error: the search could not be made` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest ([Image-worker errors](#image-worker-errors)). It is "could not look", not a list of `false`. | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | callback | The region's picture could not be taken. | `screen capture failed` | both, other |
| `too_large` | callback | A module that reads through desktop duplication asked for a region of more than 40 million pixels, with either fallback. | `screen capture failed: the region is larger than 40 million pixels` | Windows |
| any of the [duplication errors](#duplication-errors) | callback | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: an empty list (`imageSearchEach: no template given`); an entry that is not a path, a Template or a table with a `template` (`host.screen.imageSearchEach: entries[2] is a table without a template`); a template path inside the module's package that is not there or not a PNG the first time it is named (`cannot open template '…': …`); an `opts.region` or a `within` that is neither region form, or a `within` that is not a table; a `name` that is not a string; an `opts.snapshot` that is not a Snapshot or was released; a `key` that is not a non-empty string; a `cb` that is not a function (`host.screen.imageSearchEach: the callback (third argument) must be a function, got nothing`).

It never waits, so it raises nowhere for the place it is called.

### Windows {#host-screen-imagesearcheach-windows}

The capture runs on the worker thread, not on the event loop, through the module's [source](#which-picture-a-read-sees) exactly as `imageSearch`'s given a callback does; through the standard path it costs the same fixed compositor frame as any other (~16.7 ms whatever its size), and through duplication the worker waits at most 250 ms for it. The matching is spread across cores, one entry per task, on the matching pool below normal priority (see the top of this page).

### macOS {#host-screen-imagesearcheach-macos}

The same worker and the same capture as `imageSearch` given a callback. Without the Screen Recording permission the capture succeeds with a picture of the wallpaper, so the answer is **not** `nil, err` — that only means the capture itself failed (a region that is empty, too large, or wholly off the desktop) or a window region's window had an empty client area — but usually a list of `false`. Usually: a [`capture` template](#host-screen-template) that was itself made without the grant is a picture of the wallpaper, and its entry will match.

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

It runs on the event loop, costs microseconds and touches nothing on screen. A mistake raises, with the column and the text ([Errors](#host-screen-predicate-errors)).

```luau
-- A game pack's colour test, pasted from the C# reader it was recorded with, checked once.
local WARM = host.screen.predicate(
    "(red >= 80 && red*10 >= green*13 && red*10 >= blue*12) || " ..
    "(red >= 100 && green >= 45 && blue <= 150 && red >= green && green >= blue)")
host.log.info(WARM)
-- (r >= 80 and 10*r - 13*g >= 0 and 10*r - 12*b >= 0) or
--   (r >= 100 and g >= 45 and b <= 150 and r - g >= 0 and g - b >= 0)
```

### Returns {#host-screen-predicate-returns}

The predicate in its canonical form, a string. It never returns `nil`.

### Errors {#host-screen-predicate-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: anything that is not a string; and every mistake in one, a source past one of the limits above included, naming it with the column (in characters, from 1) and the text: `host.screen.predicate: the predicate: 'x' is not a channel; use r, g or b (or red, green, blue) (column 13 of "r >= 80 and x > 3")`. Among what is refused:
- an empty string
- `!=` (write `x < y or x > y`) and `not`
- a chained `80 <= r <= 120`
- a comparison that never looks at the pixel, such as `80 >= 3` or `r - r >= 0`
- upper-case channel names
- a number with a leading zero, such as `010`: C# and Luau read it as 10, C and JavaScript as octal 8, so it is refused rather than guessed
- a parenthesis left open, and parentheses nested more than 32 deep

### Windows {#host-screen-predicate-windows}

The test runs in the host on the red, green and blue bytes of each captured pixel, and the same string gives the same answer for the same colour on every platform. What can differ is the colour a capture yields for the same picture; see [`cells`](#host-screen-cells).

### macOS {#host-screen-predicate-macos}

As on Windows: the same string gives the same answer for the same colour. A Mac capture of a picture does not necessarily yield the colours a Windows capture of it yields; see [`cells`](#host-screen-cells).

## host.screen.cells(opts, cb?) {#host-screen-cells}

**Signature:** `host.screen.cells(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot? }) -> (Cells?, Error?)` where `Cells = { cells: string, x: number, y: number, w: number, h: number }`; with a callback `host.screen.cells(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot?, key: string? }, cb: (cells: Cells?, err: Error?, info: Info) -> ()) -> ()`, where `Info = { time: number?, inputEpoch: number? }`

Captures `opts.region` once and reduces it to a grid of `cols` × `rows` cells. Each cell is the share of its pixels that pass `opts.predicate` (see [`predicate`](#host-screen-predicate)), as a byte from 0 (none) to 255 (all). The answer has two parts:
- `cells`: the grid as lower-case hex, two digits a cell, row by row from the top left. Cell (row *r*, column *c*, both counted from 0) is at digits `2*(r*cols + c) + 1` and `+ 2`.
- `x, y, w, h`: the rectangle actually read, in screen coordinates.

Hex is the format [`matchCells`](#host-screen-matchcells) takes a stored state in, so this is the call that records one: read the state once while it is on screen, and put the hex in the module's data.

A grid like this is a signature, not a picture. A selected menu entry drawn in a warm colour shows as a band of high cells, and reads the same however the rest of the picture changes. The rules are those of an existing game-menu reader, so the signatures it records can be matched here as they are:
- **Blocks.** For a region `L` pixels long cut into `n` blocks, block `s` (counted from 0) starts at `start = floor(s*L/n)` and ends before `end = max(start + 1, floor((s+1)*L/n))`: the end is exclusive, and every block has at least one pixel. A block that the division would leave empty grows by one pixel to the right (for columns) or down (for rows), onto the pixel the next block starts with. Where the region is at least as long as the grid, the blocks tile it exactly and their widths differ by at most one. Where it is shorter, several cells read the same pixels — 5 pixels cut into 36 columns gives blocks 0 to 7 all pixel 0 — and a cell's `total` counts every pixel it reads, shared ones included, so each cell still has a value.
- **Value.** `round(255 * passing / total)`, with halves rounded to even: 127.5 is 128, and 42.5 is 42. `total` is the number of pixels in the cell's block, `passing` how many of them pass the predicate.
- **Alpha** is ignored.

`opts` is **strict**. The four keys `region`, `cols`, `rows` and `predicate` are required, `snapshot` may be added — and, with a callback, `key` — and anything else raises, naming what is wrong: a misspelt key, a `cols` of `10.5`, a string where a number goes (``host.screen.cells: opts.cols: invalid value: floating point `10.5`, expected a whole number``), a `snapshot` that is not a Snapshot or was released, a `key` without a callback (`host.screen.cells: opts.key is only taken with a callback (the second argument)`).
- `cols` and `rows` are whole numbers from 1 to 256, and at most 4096 cells in all.
- `region` takes either form of the [Region form](#region-form), read strictly: no default, no corner left out, no clipping. Corners may cover at most 40,000,000 pixels.
- `snapshot` reads the region where it lies in that [snapshot](#reading-a-snapshot) instead of capturing it: no screen is touched. The region must lie wholly inside the snapshot — cut to fit, every block of the grid would move.

**What happens at run time is answered, never raised**: an empty client area, a window region past the size limit at the window's current size, a capture that failed or came back a different size, a region not wholly inside its snapshot. Each is `nil` and an error value ([Errors](#host-screen-cells-errors)). So a poll never puts an error dialog over the game.

**Cost:** one screen capture and the reduction — on a snapshot, the reduction alone. In a [handler](#where-it-waits) both run on the image worker while only the module waits, for them and for the batch ahead of them there; where it cannot wait it raises. A region with nothing to read this time — an empty client area, one not inside its snapshot — is answered at once wherever it is asked. The reduction looks at each pixel once and tests every comparison of the predicate on it. On the reference machine (Core i7-8700K, release build) that measured about 8 ns a pixel with an eight-comparison predicate: 0.4 ms for 91×544 pixels, 10 ms for 1280×1024 and 17 ms for 1920×1080, on top of the capture (see below). So this is the call for recording a state or reading one once. **Given a callback** — `cells(opts, cb)` — the capture and the reduction run on the image worker while nothing waits, a window region resolved at the call, and `cb(cells, nil, info)` or `cb(nil, err, info)` comes on a later tick, `info` with the moment of the picture read ([With a callback](#with-a-callback)).

```luau
-- Record the menu's current state for a pack: the grid as hex, logged with where it was read.
-- On a key, whose handler waits for the image worker. Needs "screen", "window", "hotkey" and "log".
local WARM = host.screen.predicate("(r >= 80 and 10*r >= 13*g and 10*r >= 12*b) or " ..
    "(r >= 100 and g >= 45 and b <= 150 and r >= g and g >= b)")
host.hotkey.register("Ctrl+Alt+Shift+R", function()
    local w = host.window.active()
    if not w then return end
    local c, err = host.screen.cells({
        region = { window = w, fraction = { 0.02, 0.33, 0.09, 0.86 } },   -- x1, y1, x2, y2
        cols = 10, rows = 36, predicate = WARM,
    })   -- waits here
    if c then
        host.log.info(string.format("menu at %d,%d %dx%d: %s", c.x, c.y, c.w, c.h, c.cells))
    else
        host.log.info("menu not read: " .. err)
    end
end)
```

### Returns {#host-screen-cells-returns}

- `Cells`, `{ cells, x, y, w, h }` — one value, and nothing after it: the grid as hex and the rectangle read.
- `nil, err` — nothing was read this time; `err.code` is one of the codes under [Errors](#host-screen-cells-errors).
- With a callback: nothing at once; `cb(cells, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#host-screen-cells-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty, as a minimised game's is. Nothing is captured; answered at once, wherever the call is made, and with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. Answered at once, as above. | `the region lies outside the screen's coordinate range` | both |
| `outside` | both | With `opts.snapshot`, the region is not wholly inside the snapshot ([snapshot-source errors](#snapshot-source-errors)). Answered at once, as above. | `the region is not inside the snapshot` | both |
| `too_large` | both | A window region past 40,000,000 pixels at the window's current size. The same corners raise. Answered at once, as above. | `at the window's current size the region is …x…, … pixels; the limit is 40000000` | both |
| `failed` | both | The capture came back a different size from the region. | `the capture came back …x…, not the region's …x…` | both |
| `failed` | both | The capture came back shorter than its own size, which a capture should never do. | `the capture came back shorter than its own size` | both |
| `failed` | both | The reduction failed inside the host, or the read could not be handed to the image worker because the application is closing. The worker goes on. | `internal error: the cells could not be computed` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest ([Image-worker errors](#image-worker-errors)). | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | The region's picture could not be taken, with the cause as the message. | `screen capture failed` | both, other |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: `opts` that is not a table (`host.screen.cells: opts must be a table { region, cols, rows, predicate, snapshot? }, got …`); a key missing or misspelt, or one it does not take; a `cols` or `rows` that is not a whole number from 1 to 256 (``host.screen.cells: opts.cols: invalid value: floating point `10.5`, expected a whole number``), or more than 4096 cells in all; a `region` that is not exactly the Region form, read strictly, or corners covering more than 40,000,000 pixels; a `predicate` that does not parse, as [`predicate`](#host-screen-predicate) raises; an `opts.snapshot` that is not a Snapshot or was released; a `key` without a callback (`host.screen.cells: opts.key is only taken with a callback (the second argument)`), or one that is not a non-empty string; a callback that is neither a function nor `nil`.

**Raises where it cannot wait**: without a callback, a region with something to read, inside a function that may not wait; see [Where it waits](#where-it-waits).

### Windows {#host-screen-cells-windows}

Device pixels, never scaled: a window region resolves against the client area `host.window` reports, which is `GetClientRect` on the screen. The capture reads through the module's [source](#which-picture-a-read-sees). Through the standard path it costs about 16.7 ms whatever its size up to about 1028×666, and 28 ms for the whole 1920×1080 screen; through desktop duplication, 0.3 to 1 ms for regions up to 633×418, and 5 ms for the whole screen. What covers the region is what gets read: another window over the game, the mouse pointer when Windows draws it into the picture, and black where the region leaves every monitor.

A stored state matches only if it was recorded from the same pixels:
- the same window size, or a window region, which follows the window;
- the same display scaling: a game that is not DPI-aware is stretched by the compositor at any scaling other than 100 %, so a state recorded at one scaling is not the same bytes at another;
- the same capture source. Whether the standard path and desktop duplication give the same bytes for the same game has not been measured, so record through the source the module polls with.

### macOS {#host-screen-cells-macos}

Points, not pixels: a window region resolves against the client area `host.window` derives from the window's frame, and a capture on a Retina display is downsampled so that one pixel is one point (see [`host.screen.size`](#host-screen-size)). Cells computed from a Mac capture are not guaranteed to be the same bytes as those from a Windows capture of the same picture, so record the states on a Mac for a Mac. Without the Screen Recording permission the capture does not fail: it is a picture of the wallpaper, so the call returns cells of the wallpaper, not `nil`. In a handler and given a callback the image worker captures the region at point resolution.

## host.screen.matchCells(opts, states, cb?) {#host-screen-matchcells}

**Signature:** `host.screen.matchCells(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot? }, states: { string | { cells: string, name: string? } }) -> (CellsMatch?, Error?)`; with a callback `host.screen.matchCells(opts: { region: Region, cols: number, rows: number, predicate: string, snapshot: Snapshot?, key: string? }, states, cb: (m: CellsMatch?, err: Error?, info: Info) -> ()) -> ()`, `Info` as [`cells`](#host-screen-cells)', where `CellsMatch = { cells: string, x: number, y: number, w: number, h: number, index: number, name: string?, similarity: number, distance: number, runnerUp: number, similarities: { number } }`

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

A mistake in `opts` or in `states` raises, before anything is captured ([Errors](#host-screen-matchcells-errors)).

**Cost:** that of `cells`, plus two parts that grow with states × cells. Every call checks every state and decodes it from hex on the event loop before anything is captured — nothing is kept from one call to the next — which measured on the reference machine (release build) 0.04 ms for 10 states of 360 cells, 0.4 ms for 100, 4.4 ms for 1024, and 29 ms at the limits, 1024 states of 4096 cells: about 11 ns per cell of every state. Then the grid is compared with every state, which for 360 cells and a hundred states is microseconds. In a [handler](#where-it-waits) the capture, the reduction and the comparison run on the image worker while only the module waits, and the states are checked and decoded at the call; where it cannot wait it raises. A poll passes a callback (below), which waits for nothing — and still checks and decodes the states at the call. No form shares its picture with [`host.ocr.recognize`](./ocr.md#host-ocr-recognize), which captures on a thread of its own: detecting with a cells match and then reading the text is two pictures. Several cells matches, searches and point reads share one picture when they are given one [snapshot](#reading-a-snapshot).

```luau
-- A key that says which menu entry is selected: read once, ranked, and spoken only when sure. The
-- key's handler waits for the image worker. Needs "screen", "window", "hotkey", "resource" and "speech".
local pack = host.json.decode(host.resource.read("data/menu.json"))
-- pack = { region = { x1, y1, x2, y2 }, cols, rows, predicate, states = { { name, cells }, … } }
host.hotkey.register("Ctrl+Alt+Shift+S", function()
    local w = host.window.active()
    if not w then return end
    local m, err = host.screen.matchCells({
        region = { window = w, fraction = pack.region },
        cols = pack.cols, rows = pack.rows, predicate = pack.predicate,
    }, pack.states)   -- waits here
    if not m then
        host.speech.output("Menu not read: " .. err)
    elseif m.similarity >= 0.97 and m.similarity - m.runnerUp >= 0.01 then
        host.speech.output(m.name)
    end
end)
```

**Given a callback** — `matchCells(opts, states, cb)` — everything is still checked at the call, the states decoded and a window region resolved there from the window table given, not when the worker gets to it; the capture, the reduction and the comparison run on the image worker while nothing waits, and `cb(m, nil, info)` or `cb(nil, err, info)` comes on a later tick, `info` with the moment of the picture read ([With a callback](#with-a-callback)). Given `opts.snapshot`, the worker reduces the region where it lies in that [snapshot](#reading-a-snapshot) and captures nothing. Reads the image worker takes in one batch (see the top of this page) share one capture when they ask for the same region through the same source, whether they are cells matches or image searches, and from one module or several. Callbacks run in the order the reads were queued. A read's callback is dropped when its module is disabled before the answer comes, as every callback on this page is; a poll that keeps one read out with its `key` and [`pending`](#host-screen-pending) asks again at its first turn once the module is enabled — on the window as it is then, which may have moved, changed size or closed.

**Read, then act.** Nothing holds a module's input back for a picture it asked for — not a pending match or image search, nor a pending [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) or [snapshot](#snapshot-with-a-callback): a click made while it waits may land before its picture is taken, so its callback can describe the screen after the click. Act on a match inside its callback, or match on a snapshot taken before the click.

```luau
-- Say the selected menu entry when it changes: one read out at a time, only while the game is in
-- front. Needs "screen", "timer", "window", "resource" and "speech" in the manifest.
local pack = host.json.decode(host.resource.read("data/menu.json"))
host.screen.predicate(pack.predicate)                        -- a bad pack fails here, at load
local GAME = { title = { contains = "My Game" } }
local last = nil
host.timer.every(150, function()
    if host.screen.pending("menu") then return end            -- the last read has not answered yet
    local w = host.window.active()
    if not w or not host.window.test(GAME, w) then return end
    host.screen.matchCells({
        region = { window = w, fraction = pack.region },
        cols = pack.cols, rows = pack.rows, predicate = pack.predicate, key = "menu",
    }, pack.states, function(m, err)
        if not m then return end                              -- could not look: keep what we knew
        local name = (m.similarity >= 0.97 and m.similarity - m.runnerUp >= 0.01) and m.name or nil
        if name ~= last then
            last = name
            if name then host.speech.output(name) end
        end
    end)
end)
```

### Returns {#host-screen-matchcells-returns}

- `CellsMatch`, `{ cells, x, y, w, h, index, name?, similarity, distance, runnerUp, similarities }` — one value, and nothing after it: the winner, as above. There is always a winner; whether it is good enough is the module's to decide.
- `nil, err` — nothing was read this time, exactly where `cells` answers so; `err.code` is one of the codes under [Errors](#host-screen-matchcells-errors).
- With a callback: nothing at once; `cb(m, nil, info)` or `cb(nil, err, info)` on a later tick.

### Errors {#host-screen-matchcells-errors}

None of these is raised. Each comes as an error value where "arrives" says. They are [`cells`](#host-screen-cells-errors)' own.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty, as a minimised game's is. Nothing is captured; answered at once, wherever the call is made, and with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. Answered at once, as above. | `the region lies outside the screen's coordinate range` | both |
| `outside` | both | With `opts.snapshot`, the region is not wholly inside the snapshot ([snapshot-source errors](#snapshot-source-errors)). Answered at once, as above. | `the region is not inside the snapshot` | both |
| `too_large` | both | A window region past 40,000,000 pixels at the window's current size. The same corners raise. Answered at once, as above. | `at the window's current size the region is …x…, … pixels; the limit is 40000000` | both |
| `failed` | both | The capture came back a different size from the region. | `the capture came back …x…, not the region's …x…` | both |
| `failed` | both | The capture came back shorter than its own size, which a capture should never do. | `the capture came back shorter than its own size` | both |
| `failed` | both | The reduction or the comparison failed inside the host, or the read could not be handed to the image worker because the application is closing. The worker goes on. | `internal error: the cells could not be computed` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest ([Image-worker errors](#image-worker-errors)). | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | The region's picture could not be taken, with the cause as the message. | `screen capture failed` | both, other |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: everything [`cells`](#host-screen-cells) raises for — `opts.key` without a callback among them, `(the third argument)` here — and for these, checked before anything is captured:
- `states` that is not a list of 1 to 1024 entries, or that has other keys;
- a state that is not hex of the right length (`states[3]: 358 characters; this grid's states are 720 hex digits, two per cell`);
- a state that is not hex at all (`states are hex (720 characters), not the cells' raw bytes`);
- a state with a character that is not a hex digit, counted in characters (`states[2]: 'é' at character 5 is not a hex digit`);
- a state table with a key other than `cells` and `name`, or a `name` that is not a string.

**Raises where it cannot wait**: without a callback, a region with something to read, inside a function that may not wait; see [Where it waits](#where-it-waits). The states are checked and decoded at the call, wherever it is made.

### Windows {#host-screen-matchcells-windows}

The capture and the pixels are those of [`cells`](#host-screen-cells): device pixels, through the module's source, on the image worker, and states recorded from the same pixels match exactly. Given a callback, what a poll costs the event loop is checking its arguments and decoding its states (0.4 ms for 100 states of 360 cells, above) and resolving its region. In a module that reads through desktop duplication, the worker makes the [first-read comparison](#which-picture-a-read-sees) before its capture, until duplication has answered it once.

### macOS {#host-screen-matchcells-macos}

The capture and the pixels are those of [`cells`](#host-screen-cells): points, with states recorded on a Mac for a Mac. Without the Screen Recording permission this compares the wallpaper with the states and returns a match rather than `nil`; its `similarity` is whatever the wallpaper happens to score, so the thresholds are what keep it quiet.

## Region form {#region-form}

A region is a rectangle on screen, written in one of two forms: by its **corners**, or as **fractions of a window's client area**. **Every call that takes a region takes both**, through one reader: `profile`, `template`'s `capture`, `imageSearch`, `imageSearchEach` (its `opts.region` and each entry's `within`), `imageSearchAll`, `save`, `saveMarked`, `cells`, `matchCells`, `snapshot` and each region of its `change.watch`, a snapshot's `crop`, and each region of [`host.ocr.recognize`](./ocr.md#host-ocr-recognize). [`pixel`](#host-screen-pixel) and each point of [`pixels`](#host-screen-pixels) take a point in the same window form, `{ window = w, fraction = { x, y } }`. A region left out — `nil` — is the whole primary screen wherever a call allows leaving it out, and the whole snapshot for a call given one; the cells calls, `snapshot` and its `watch`, `crop` and `host.ocr.recognize` do not allow it.

**Corners.** A region describes an axis-aligned rectangle by its top-left and bottom-right corners and may be written in **named** or **positional** form:

- Named: `{ x1 = .., y1 = .., x2 = .., y2 = .. }`
- Positional: `{ x1, y1, x2, y2 }` (array indices `[1]`=x1, `[2]`=y1, `[3]`=x2, `[4]`=y2)

`x2` and `y2` are **exclusive**: the rectangle starts at `(x1, y1)` and is `x2 - x1` wide and `y2 - y1` high, so `{ 10, 20, 210, 120 }` covers columns 10 to 209. How strictly corners are read is each call's own, kept as it has always been: the cells calls, `snapshot` and each region of its `change.watch`, `crop`, and `host.ocr.recognize` read them strictly, every other call loosely (both below).

**Window fractions.** `{ window = w, fraction = { x1, y1, x2, y2 } }` gives a rectangle as fractions of a window's client area, and the host turns it into pixels (points on macOS) at the call.
- `window` is a window table as `host.window` returns it (a control table works too). Only its `client = { x, y, w, h }` is read, with no call to the operating system, so the region is exactly as current as the table. A table from `host.window.active()` is the one [kept for the epoch](./timer.md#host-epoch): within one epoch every call hands back the same table, and a [`host.timer.every`](./timer.md#host-timer-every) tick does not turn the epoch over, so in a poll the fractions resolve against the client rectangle the window had when the epoch last turned over. A window that has moved or changed size since then, with no event in between, is read at its old place; `host.window.find` asks the operating system afresh on every call (see [`host.window`](./window.md#table-shapes)).
- `fraction` is `{ x1, y1, x2, y2 }`, named or positional like corners, in fractions of the client area's width (`x1`, `x2`) and height (`y1`, `y2`): `{ 0, 0, 1, 1 }` is the whole client area, and `{ 0.5, 0, 1, 1 }` its right half. A reader that stores a region as `XStart, XEnd, YStart, YEnd` writes `{ XStart, YStart, XEnd, YEnd }`.
- With `W` the client area's width, the columns read start at `floor(W * x1)` and end before `ceil(W * x2)`. The start is then clamped to 0 through `W - 1`, and the end to one past the start through `W`; rows are worked out the same way from the height. So the rectangle never leaves the client area and is never empty: a fraction outside 0 to 1, or an `x2` before `x1`, still reads at least one column. The arithmetic is IEEE double precision, exactly as a C# reader computing with `double` does it. `800 * 0.035` is 28.000000000000004 there, so the end is 29.
- The numbers themselves never raise for being outside 0 to 1 or in the wrong order, as the clamps above say. What raises, in every call, is the shape: a fraction that is not a finite number (`1/0`, NaN) or not a number, a `fraction` table with a corner missing, with named and positional corners mixed or with any other key, a key beside `window` and `fraction`, and a `window` that is not a table or has no `client` with four whole numbers.
- A client area without width or height — a minimised window — is a runtime condition, answered and never raised: the call fails the way it fails when it could not look, with the error value `hidden`, `the window's client area is empty (0x0)` ([Window-form errors](#window-form-errors)). `host.ocr.recognize` answers that one region so and reads the others; an `imageSearchEach` entry whose `within` it is answers `false`.

Written as fractions, a region keeps covering the same part of the window when the window is resized, at any display scaling, and on both platforms, with nothing in the module that knows which of those it is running under.

**A table that is neither form raises**, in every call, naming the call and the argument: `{}`, and a window's `bounds`-style table `{ x = …, y = …, w = …, h = … }`, which has none of the four corners (`host.screen.profile: opts.region is neither { x1, y1, x2, y2 } nor { window = w, fraction = { x1, y1, x2, y2 } }: it has none of x1, y1, x2, y2 (its keys: h, w, x, y); a rectangle { x, y, w, h }, such as a window's bounds, is written { b.x, b.y, b.x + b.w, b.y + b.h }`). So does a region that is not a table at all, `region = "screen"` included. Only leaving the region out reads the whole primary screen.

**Strictly read corners.** The cells calls, `snapshot` and each region of its `change.watch`, a snapshot's `crop`, and `host.ocr.recognize` raise at the call for anything in corners that is not exactly right, naming it (`host.screen.cells: opts.region.x2 must be a whole number, got 10.5`):
- all four corners are required, as whole numbers (`10.0` is one, `10.5` is not), written one way (named or positional, not both), with `x2` greater than `x1` and `y2` greater than `y1`, and any other key raises — but `host.ocr.recognize` takes corners with no width or no height, empty or turned around, as a region with nothing to read: that region's reading is `"failed"`, its `error` `outside`, `the region is empty (…x…)`, a side below none counted as none, as `profile` answers them, and the call's other regions are read;
- the region has no default: `{}` has no corners and raises like any other region with a corner missing, and a region the cells calls read is not clipped to the screen, so a part off every monitor reads black.

Round a computed corner yourself, `math.floor(v + 0.5)` for the nearest pixel, or give the region as fractions of the window it belongs to.

**Loosely read corners.** Every other call reads corners as it always has: a table with at least one of `x1`, `y1`, `x2`, `y2` (or entries 1 to 4) is corners, and inside it nothing raises. Named keys take precedence over positional ones, corner by corner. A corner that is missing — or is not a number, such as a misspelt key's `nil` or a string that is not a numeral — takes its default without a word: `x1, y1` become `0`, `x2, y2` the primary screen's width and height (for a call given a snapshot, the snapshot's own edges). Other keys are ignored. A fractional coordinate is cut toward zero, like every coordinate the platform takes: a centre computed as `x + w / 2` lands on the whole pixel toward zero, which for a negative coordinate — a monitor left of or above the primary — is the opposite direction from `//`. Where the nearest pixel matters, round with `math.floor(v + 0.5)`. Corners turned around, or equal, read as an empty region — width and height below zero count as zero — which most calls answer as a read that failed rather than a mistake: `profile` with `nil, err` (`outside`, `the region is empty (0x0)`), the searches, `save` and `saveMarked` with `failed`, `screen capture failed` (given a snapshot, `outside`, `the region does not overlap the snapshot`); `template{ capture = … }` raises for it.

```luau
-- these two regions are equivalent
{ region = { x1 = 10, y1 = 20, x2 = 210, y2 = 120 } }
{ region = { 10, 20, 210, 120 } }

-- Asked from a key, whose handler waits for the screen while only this module waits. Needs
-- "screen", "hotkey", "window", "ocr" and "log".
host.hotkey.register("Ctrl+Alt+Shift+F", function()
  local w = host.window.active()
  if not w then return end

  -- a window's bounds, converted
  local b = w.bounds
  local p = host.screen.profile({ region = { b.x, b.y, b.x + b.w, b.y + b.h }, axes = "rows" })   -- waits here

  -- the lower left of the active window's client area, as fractions: follows the window
  local c = host.screen.cells({ region = { window = w, fraction = { 0, 0.4, 1/3, 1 } },
                                cols = 3, rows = 6, predicate = "r > g + b" })   -- waits here

  -- the same form in every call: a strip across the top of the client area, searched and saved
  local strip = { window = w, fraction = { 0, 0, 1, 0.1 } }
  local hit, err = host.screen.imageSearch("images/tab.png", { region = strip, tolerance = 8 })   -- waits here
  if err then host.log.info("could not look: " .. err) end
  host.screen.save("strip.png", { region = strip })   -- and here

  -- and for text: the top tenth of the client area, read off the event loop
  host.ocr.recognize({ window = w, fraction = { 0, 0, 1, 0.1 } }, function(r)
    host.log.info(r.status .. ": " .. (r.error or r.text))
  end)
end)
```

### Windows {#region-form-windows}

Fractions resolve to device pixels of the client area `GetClientRect` reports, translated to the screen.

### macOS {#region-form-macos}

Fractions resolve to points of the client area derived from the window's frame (see [the window table](./window.md#table-shapes)). The same fractions cover the same part of a window as on Windows, where corners measured on one platform would need converting for the other.

## host.screen.save(path, opts?, cb?) {#host-screen-save}

**Signature:** `host.screen.save(path: string, opts: { region: Region?, snapshot: Snapshot? }?) -> (boolean, Error?)`; with a callback `host.screen.save(path: string, opts: { region: Region?, snapshot: Snapshot?, key: string? }?, cb: (saved: boolean, err: Error?, info: Info) -> ()) -> ()`, where `Info = { time: number?, inputEpoch: number? }`

Captures `opts.region` and writes it to `path` as a PNG — corners or a window region, see [Region form](#region-form); omitted, the whole primary screen. With `opts.snapshot` it writes the part of the region inside that [snapshot](#reading-a-snapshot) instead, capturing nothing — omitted, the whole snapshot — and returns `false, err` (`outside`, `the region does not overlap the snapshot`) when no part is inside it: the picture a module's reads were answered from, for somebody to look at. PNG is the one image format the host is built with, and the format follows the extension, so `path` must end in `.png`, in upper or lower case: any other extension, or none, raises before anything is captured, whether or not the capture would have worked (`host.screen.save: 'C:\…\shot.jpg' does not end in .png; PNG is the one image format the host writes`). This is the evidence call: an author who cannot see the screen has no way to check that a coordinate lands on its button, so the picture goes to somebody who can look at it — `tools/probe` writes one shot of the window beside its text dump for exactly that, and the overlay runtime's calibration crop key (`Ctrl+Alt+Shift+T`, Command+Option+Shift+T on a Mac) uses it to cut a fresh template out of the live plug-in when an old one stopped matching. It costs one capture — about one compositor frame through the standard path on Windows (~16.7 ms) regardless of the region's size, less through desktop duplication (see [Which picture a read sees](#which-picture-a-read-sees)) — plus the PNG encode and the write, which grow with the region: in a [handler](#where-it-waits) the capture on the image worker and the encode and the write on its matching pool, which the handler waits for while only its module waits, given a callback there too while nothing waits; where it cannot wait it raises, and nothing is captured or written. The macOS round trip is unmeasured.

A relative `path` is resolved under the root of the module whose code makes the call (see [the path rule](./index.md#paths)); an absolute path (what `host.path` returns) is used as it stands, and the parent directory is created if it does not exist. Neither is checked for `..`: this writes wherever the path points. `save` and `saveMarked` are the only calls in the platform that write a file a module names, and both write PNG only. It answers `true`, or `false` and an error value when the region could not be captured, a window region's window has an empty client area, or the file could not be written ([Returns](#host-screen-save-returns), [Errors](#host-screen-save-errors)). A file that could not be written says so in its message, `screen.save '<path>': <the system's words>`, so a typo in a folder's name is not mistaken for a failed capture: `err.code` is `failed` for both. A shot a handler waits for is not written, and its folder is not made, when nobody waits for it any more once its PNG is encoded — its module disabled, reloaded, stopped by the [guard](../module-runtime-and-lifecycle.md#limits) or removed meanwhile, or its handler cancelled: a module that is gone gets no file written behind it. One whose file is already being written then is still written. The same holds for `saveMarked`.

**Given a callback** — `save(path, opts, cb)`, `opts` `nil` when there are none — the shot is taken, encoded and written on the image worker while nothing waits, and `cb(true, nil, info)` or `cb(false, err, info)` comes on a later tick, `info` with the moment of the picture written ([With a callback](#with-a-callback)) — a file that could not be written included, with the same `screen.save '<path>': <the system's words>`. A shot whose module is disabled, reloaded, stopped or removed before its file is written is not written, as above, and its callback is dropped. The same holds for `saveMarked`.

```luau
-- tools/probe: the one artefact a sighted helper can check in seconds, saved next to the numbers it
-- is supposed to confirm — in a timer's handler, which waits for the shot. Needs "screen", "window",
-- "timer" and "log".
host.timer.after(0, function()
  local win = host.window.active()
  if not win then
    host.log.info("probe: nothing is focused, so there is nothing to capture")
    return
  end
  local shot = "probe.png"
  local saved, err = host.screen.save(shot, {   -- waits here
    region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
  })
  host.log.info(string.format(
    "capture: %s (%dx%d requested)", saved and shot or ("FAILED: " .. err), win.bounds.w, win.bounds.h
  ))
end)
```

### Returns {#host-screen-save-returns}

- `true` — the file is written: one value, and nothing after it.
- `false, err` — nothing was written; `err.code` is one of the codes under [Errors](#host-screen-save-errors).
- With a callback: nothing at once; `cb(true, nil, info)` or `cb(false, err, info)` on a later tick. A shot whose module goes before its file is written gets no answer at all: the callback is dropped, and the file is not written.

### Errors {#host-screen-save-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. Nothing is captured or written; answered at once, wherever the call is made, and with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. Answered at once, as above. | `the region lies outside the screen's coordinate range` | both |
| any of the [snapshot-source errors](#snapshot-source-errors) | both | With `opts.snapshot`, no part of the region is inside the snapshot: `outside`, `the region does not overlap the snapshot`. Answered at once, as above. | `the region does not overlap the snapshot` | both |
| `failed` | both | The file could not be written: a folder that could not be made, no right to write there, a full disk. The message names the path as it was resolved and gives the system's words, so a typo in a folder's name says so here, which a failed capture never does. In a handler, once the wait is over. | `screen.save '…': …` | both |
| `failed` | both | The marking, encoding or writing failed inside the host, or the shot could not be handed to the image worker because the application is closing. | `internal error: the picture could not be saved` | both |
| `failed` | both | The capture came back shorter than its own size, which a capture should never do. | `the capture came back shorter than its own size` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest, and its file is not written ([Image-worker errors](#image-worker-errors)). | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key, before this file was written. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | The region's picture could not be taken; no file is written. | `screen capture failed` | both, other |
| `too_large` | both | A module that reads through desktop duplication asked for a region of more than 40 million pixels, with either fallback. | `screen capture failed: the region is larger than 40 million pixels` | Windows |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer; no file is written. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call, before anything is read: a `path` that is not a string; a path that does not end in `.png`, in upper or lower case (`host.screen.save: 'C:\…\shot.jpg' does not end in .png; PNG is the one image format the host writes`); `opts` that is neither a table nor `nil` (`host.screen.save: opts must be a table, got …`); an `opts.region` that is neither region form; an `opts.snapshot` that is not a Snapshot or was released; a `key` without a callback, or one that is not a non-empty string; a callback that is neither a function nor `nil`.

**Raises where it cannot wait**: without a callback, a shot with something to write, inside a function that may not wait; nothing is captured or written. See [Where it waits](#where-it-waits).

### Windows {#host-screen-save-windows}

The shot is taken through the module's [source](#which-picture-a-read-sees) like every other read: on the image worker, which waits for desktop duplication up to 250 ms. In a module that declares `capture = "duplication"`, a shot taken while duplication is still opening for longer than that — just after the module's window came forward, say — is taken the standard way, or `save` answers `false, err` under `fallback = "none"`, so a template cut from such a shot is not necessarily the kind of picture the module's searches read.

### macOS {#host-screen-save-macos}

Without the Screen Recording permission the capture does **not** fail — macOS hands back a picture of the desktop wallpaper. A saved shot is therefore the cheapest way to tell a missing grant from a wrong rectangle: if the PNG shows the wallpaper, no coordinate in the module is at fault. A template cut around the same on-screen element on a Retina Mac also comes back at point resolution, i.e. half the pixel dimensions of the same crop taken on a HiDPI Windows machine (see [`host.screen.size`](#host-screen-size)).

## host.screen.saveMarked(path, opts, cb?) {#host-screen-savemarked}

**Signature:** `host.screen.saveMarked(path: string, opts: { region: Region?, marks: { { x: number, y: number } }?, snapshot: Snapshot? }) -> (boolean, Error?)`; with a callback `host.screen.saveMarked(path: string, opts: { region: Region?, marks: { { x: number, y: number } }?, snapshot: Snapshot?, key: string? }, cb: (saved: boolean, err: Error?, info: Info) -> ()) -> ()`, `Info` as [`save`](#host-screen-save)'s

Everything `save` does — a PNG, and only a `.png` path, `true`, or `false` and an error value (`screen.saveMarked '<path>': <the system's words>` for a file it could not write), from the screen or from `opts.snapshot`, and given a callback the same callback form — plus a magenta crosshair drawn at every point in `opts.marks`, given in **screen** coordinates whichever form `opts.region` takes: a window region is resolved to its screen rectangle first (on a snapshot, to the part of it inside the snapshot), and each mark is drawn where its screen point falls in that rectangle. It puts the calibration question into one picture — is my control on its button? — instead of leaving somebody to read coordinates out of a log and find that spot in a plain screenshot by hand, which is the step that hides errors: a set of toggles in this repo sat 16 px off, on the caption row under the buttons, for as long as it existed, because the colours sampled there happened to look plausible. Magenta because these dark plug-in UIs do not use it, so ink cannot be mistaken for interface. Each crosshair is 19 px across, and the *n*th mark gets *n* dots on a row 12 px below its centre, so marks stay tellable apart without any text.

Four things bite. `opts` is **required** here (`save`'s is optional) — pass at least `{}`; without it the call raises (`host.screen.saveMarked: opts must be a table — pass at least {} — got nothing`). Mark entries are read by **name**: `{ x = 100, y = 200 }`, never positional `{ 100, 200 }`, which reads as screen `(0, 0)`: dropped when the captured region does not reach the screen origin, and otherwise drawn in the corner, which is worse — a crosshair that means nothing. A mark outside the captured region is skipped without a word, so grow the region to cover the marks the way `calibrationShot` does — Komplete Kontrol's standalone menu bar sits 10 px above the client top, and a shot clipped to the client rectangle cut every crosshair off, producing a picture with no marks at all, which reads as "nothing was marked" rather than "the marks are off-frame". And the ink lands on the very thing the mark points at, so when you also need to *look* at those pixels, take the same frame twice.

```luau
-- overlay-runtime's calibration shot (Ctrl+Alt+Shift+S): one crosshair per VISIBLE control
-- — a hidden one gets none, because marking it would claim the user can reach it. In the key's
-- handler, where each shot waits for the image worker.
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

### Returns {#host-screen-savemarked-returns}

- `true` — the file is written, with its marks: one value, and nothing after it.
- `false, err` — nothing was written; `err.code` is one of the codes under [Errors](#host-screen-savemarked-errors).
- With a callback: nothing at once; `cb(true, nil, info)` or `cb(false, err, info)` on a later tick, dropped as `save`'s is.

### Errors {#host-screen-savemarked-errors}

None of these is raised. Each comes as an error value where "arrives" says. They are [`save`](#host-screen-save-errors)'s, with the shot's own message for a file it could not write.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. Nothing is captured or written; answered at once, wherever the call is made, and with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. Answered at once, as above. | `the region lies outside the screen's coordinate range` | both |
| any of the [snapshot-source errors](#snapshot-source-errors) | both | With `opts.snapshot`, no part of the region is inside the snapshot: `outside`, `the region does not overlap the snapshot`. Answered at once, as above. | `the region does not overlap the snapshot` | both |
| `failed` | both | The file could not be written: a folder that could not be made, no right to write there, a full disk. The message names the path as it was resolved and gives the system's words, so a typo in a folder's name says so here, which a failed capture never does. In a handler, once the wait is over. | `screen.saveMarked '…': …` | both |
| `failed` | both | The marking, encoding or writing failed inside the host, or the shot could not be handed to the image worker because the application is closing. | `internal error: the picture could not be saved` | both |
| `failed` | both | The capture came back shorter than its own size, which a capture should never do. | `the capture came back shorter than its own size` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest, and its file is not written ([Image-worker errors](#image-worker-errors)). | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key, before this file was written. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | The region's picture could not be taken; no file is written. | `screen capture failed` | both, other |
| `too_large` | both | A module that reads through desktop duplication asked for a region of more than 40 million pixels, with either fallback. | `screen capture failed: the region is larger than 40 million pixels` | Windows |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer; no file is written. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call, before anything is read: what `save` raises for, with `host.screen.saveMarked` in the message; and `opts` that is not a table, `nil` included (`host.screen.saveMarked: opts must be a table — pass at least {} — got nothing`). A mark that is not a table is passed over, and a coordinate that is not a whole number is read as `0`: neither raises.

**Raises where it cannot wait**: without a callback, a shot with something to write, inside a function that may not wait; nothing is captured or written. See [Where it waits](#where-it-waits).

### Windows {#host-screen-savemarked-windows}

The capture is `save`'s: one read through the module's [source](#which-picture-a-read-sees), on the image worker, and under `fallback = "none"` a shot duplication could not answer is answered `false, err`, with no file written. Marks, region and PNG are all in device pixels, so a crosshair lands on the very pixel its screen point names at any display scaling.

### macOS {#host-screen-savemarked-macos}

The capture is `save`'s, so the wallpaper-instead-of-error case applies here too — and it is nastier with marks on it, because crosshairs drawn over a picture of the desktop look like a considered answer rather than a missing permission. Check that the shot shows the plug-in before reading anything into where the marks landed.

## host.screen.imageSearchAll(template, opts?, cb?) {#host-screen-imagesearchall}

**Signature:** `host.screen.imageSearchAll(template: string | Template, opts: { region: Region?, tolerance: number?, snapshot: Snapshot? }?) -> ({ Hit }, Error?)`; with a callback `host.screen.imageSearchAll(template, opts: { region: Region?, tolerance: number?, snapshot: Snapshot?, key: string? }?, cb: (hits: { Hit }, err: Error?, info: Info) -> ()) -> ()`, `Info` as [`imageSearch`](#host-screen-imagesearch)'s

Returns **every** match of `template` in the region, as an array of [hits](#host-screen-imagesearch) in screen pixels, rather than stopping at the first one like `imageSearch`. `template` is a path or a [`Template`](#host-screen-template), checked and opened as for [`imageSearch`](#host-screen-imagesearch) — a template the image worker cannot open is `{}, err` (`not_found`, `invalid` or `failed`, `cannot open template '…': …`); each hit carries `n = 1` and the template's `name`. `opts.region` is corners or a window region ([Region form](#region-form)), and a table that is neither raises; so does `opts` that is neither a table nor nothing (`host.screen.imageSearchAll: opts must be a table, got …`). The list is never `nil`. It is empty when there is no match, and then comes alone; when it could not look — the region could not be captured, the template could not be opened, or a window region's window has an empty client area — it is empty too and an error value comes after it, `{}, err` ([Errors](#host-screen-imagesearchall-errors)), so the two can be told apart by a caller that asks. Its use is deciding whether a template is safe to click blindly: a close-glyph template that matches twice will eventually click the wrong one, and a template that matches nowhere is a control that silently never fires. "Matched exactly once, here" is the answer you want before shipping either, which is why the overlay runtime keeps it on a calibration key rather than in any live path.

It cannot stop early, so it always pays the **full** scan of the region — and matching, unlike the capture, grows with the area: in a [handler](#where-it-waits) on the image worker's matching pool, which the handler waits for while only its module waits; where it cannot wait it raises. Given a callback — `imageSearchAll(template, opts, cb)` — the same scan runs there while nothing waits, and `cb(hits, nil, info)` or `cb({}, err, info)` comes on a later tick ([With a callback](#with-a-callback)). Two further limits are worth knowing before you read a count. It honours `tolerance` but **not** `scales`: the search runs at the template's native size only, so a control your overlay matches through a scale ladder can verify as zero hits here without anything being wrong with it. And de-duplication is per row — after a hit the scan resumes past that hit's right edge on the same row, but the next row starts again from the left, so a slack tolerance can report one button once per row it survives on.

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
local all, err = host.screen.imageSearchAll(image, { region = { window = o, fraction = { 0, 0, 1, 1 } } })
if err then host.log.info("not searched: " .. err) end
```

### Returns {#host-screen-imagesearchall-returns}

- A list of `Hit`s, `{ { x, y, w, h, n = 1, name? }, … }`, in the order the scan met them — it looked: one value, and nothing after it. An empty list alone means no match.
- `{}, err` — it could not look; `err.code` is one of the codes under [Errors](#host-screen-imagesearchall-errors). The list is never `nil`, so test the second value to tell "no match" from "could not look".
- With a callback: nothing at once; `cb(hits, nil, info)` or `cb({}, err, info)` on a later tick.

### Errors {#host-screen-imagesearchall-errors}

None of these is raised. Each comes as an error value where "arrives" says.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `hidden` | both | A window region, and the window's client area is empty: a minimised window, or one not laid out yet. Nothing is captured. Answered at once, wherever the call is made; with a callback on the next tick. | `the window's client area is empty (…x…)` | both |
| `outside` | both | A window region lies outside the coordinate range, which a window table from `host.window` never does. Nothing is captured. Answered at once, as above. | `the region lies outside the screen's coordinate range` | both |
| any of the [snapshot-source errors](#snapshot-source-errors) | both | With `opts.snapshot`, no part of the region is inside the snapshot: `outside`, `the region does not overlap the snapshot`. Answered at once, as above. | `the region does not overlap the snapshot` | both |
| `not_found` | both | The image worker could not open a template's PNG because it is not there: gone since its path was first named, or outside the module's package. A `Template` built from such a file answers the same, and tries the file again at the next search. | `cannot open template '…': …` | both |
| `invalid` | both | A template's PNG whose picture does not decode: cut short, not a PNG, or a format this host is not built with. | `cannot open template '…': …` | both |
| `failed` | both | A template's PNG that could not be read: held by another program, or no right to read it. | `cannot open template '…': …` | both |
| `failed` | both | A template's picture failed to be made inside the host: a `Template` built from bytes or a file, or a PNG whose decode failed. That search alone; the rest of its batch is read as usual, and the next search that names the file decodes it again. | `internal error: the template could not be made` | both |
| `failed` | both | The search's batch failed inside the host, or the search could not be handed to the image worker because the application is closing. The worker goes on. | `internal error: the search could not be made` | both |
| `too_many` | callback | The module had 64 of the image worker's calls with a callback waiting, and asked for one more: this was its oldest ([Image-worker errors](#image-worker-errors)). It is "could not look", not "not there". | `too many image searches waiting for this module (64); the oldest was ended` | both |
| `replaced` | callback | With `opts.key`: the module asked the same call again with the same key. | `a newer request with the same key replaced this one` | both |
| any of the [capture errors](#capture-errors) | both | The region's picture could not be taken. | `screen capture failed` | both, other |
| `too_large` | both | A module that reads through desktop duplication asked for a region of more than 40 million pixels, with either fallback. | `screen capture failed: the region is larger than 40 million pixels` | Windows |
| any of the [duplication errors](#duplication-errors) | both | Only in a module that declares `[screen] fallback = "none"`, when desktop duplication could not answer. | `screen capture failed: desktop duplication could not answer — …` | Windows |

**Raises** for a mistake in the call: a `template` that is neither a path nor a Template; a path inside the module's package that is not there or not a PNG the first time it is named (`cannot open template '…': …`); `opts` that is neither a table nor `nil` (`host.screen.imageSearchAll: opts must be a table, got …`); an `opts.region` that is neither region form; an `opts.snapshot` that is not a Snapshot or was released; a `key` without a callback, or one that is not a non-empty string; a callback that is neither a function nor `nil`.

**Raises where it cannot wait**: without a callback, a search with something to read, inside a function that may not wait; see [Where it waits](#where-it-waits).

### Windows {#host-screen-imagesearchall-windows}

The capture is one read through the module's [source](#which-picture-a-read-sees), on the image worker: the fixed ~16.7 ms frame through the standard path, less through desktop duplication plus any wait for it (at most 250 ms). The full scan follows, in device pixels, on the image worker's matching pool.

### macOS {#host-screen-imagesearchall-macos}

The capture is in points, on the image worker. Without the Screen Recording permission it does not fail: the list is the matches in a picture of the wallpaper — usually none, and no error value with it.
