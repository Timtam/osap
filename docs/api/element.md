---
title: "host.element — the accessibility tree"
sidebar_position: 3
toc_max_heading_level: 2
---

Queries the accessibility tree an application publishes — UI Automation on Windows, the Accessibility API on macOS — for elements by name and control type, for a point to click, and for a dump of what a window contains.

It is the first thing to try on a new plug-in, because a tree gives names and rectangles and those survive the window being moved, resized or redrawn. Every coordinate a module writes down instead is a claim about one window size on one machine.

Two shapes account for nearly all the use of it. **Identity**, where `findAny` asks "which of these names is present" and reports *which*, so Kontakt learns the version it is attached to from the same call that recognises it at all. And **geometry**, where `locate` and `pluginLocate` hand back a point to click — ON:EAR is driven entirely off the rectangles in a `rawDump`, its five identically named preset slots picked out as the only elements sharing an exact left edge.

It is a cross-process query and it is not free: a 53-element window takes about 30 ms to walk, and took 211 ms before the backend learned to fetch a node's properties in one request, while a raw walk into a hosted plug-in has measured 60–300 ms against a pixel's 17 through the standard path. And the other program answers when its own thread gets to it: REAPER building its Load dialog did not, for a second and a half. So every call takes a callback last, and handed one, nothing waits for the answer — see [Asking without waiting](#asking-without-waiting). On Windows no call is asked on the event loop at all.

`find`, `findAny`, `pluginLocate`, `locate` and `locateVia` are asked once per [epoch](./timer.md#host-epoch): a second identical call in the same epoch gets the first answer, a `false` or `nil` included — and a "not known" (below) as not known — without asking the application again; with a callback, that answer comes on the next turn. Every module's overlays evaluate in turn on the same key, window or focus change, so one walk serves them all. The epoch turns over on OS events, on a `host.timer.after` callback coming due, on image results and on input the platform drives — but not on a [`host.timer.every`](./timer.md#host-timer-every) tick, so a poll can be handed a `find`, `findAny` or `pluginLocate` answer asked before the tick, and not on [`host.input.post`](./input.md#host-input-post), so a check repeated after a posted key in the same callback sees the answer from before it. A `locate` or `locateVia` asked in the polls' lane — from a `host.timer.every` tick, while a module loads, or from what those asked for — is put to the application every time, and its answer is kept for the calls after it in the epoch: a poll is there to see what no event announces, such as a panel that opens or closes inside a window that stays, and where a thing is, or whether it is on screen at all, is what such a change moves. An answer that comes to a callback is kept for the epoch only when the epoch and the input epoch it was asked in still stand, and only an answer the program gave: what the host says without putting the question to it — not asked, replaced, refused ([Asking without waiting](#asking-without-waiting)) — is never kept, and the next identical call asks. The other calls here are asked afresh every time.

The trap is that **not finding something and not being able to see anything are the same `false`.** A DAW-embedded Kontakt 8 is a single leaf; Kontakt's instrument editor is absent from a tree that lists its header and status bar; a probe of a REAPER FX window on macOS found 22 elements of which every one belonged to REAPER. So dump the window before building on any of this, and where a miss could mean either, decide in advance which way to fail — Kontakt's rack-view test counts "the plug-in is not answering" as yes, because a wrong no hides seven controls from somebody who cannot see that they are gone. A program that did not answer at all is told apart: that is `nil` and an error value that says why, never `false` (see below). So is a search that stopped at its bound before the end of the tree and found nothing.

**What a call answers.** Each entry's **Returns** says what every answer means, and its **Errors** lists the [error values](./errors.md#error-values) it can give, by code. The codes every question shares on Windows are listed once, as [the question errors](#question-errors), and a Mac's under [`find`'s Errors](#host-element-find-errors); the other entries name them with a link. Compare `err.code`, never the message: a message may change between versions, and the code does not. On macOS a press answers `not_supported`; the build for other systems has no tree, so every question there finds nothing and a press answers `not_supported`. Besides the mistakes each entry lists under **Raises**, two raises can meet any call here: reaching for `host.element` in a module that did not declare `element` ([What to declare](#declare)), and the host's guard stopping a callback that ran too long (`stopped by the host: …`, [the limits](../module-runtime-and-lifecycle.md#limits)). An argument of the wrong type raises with the host's conversion message, such as `bad argument #2: error converting Lua nil to String (expected string or number)`; a number with a fraction is cut toward zero where a whole number goes.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["element"]
```

The name is `element`, like the namespace. A manifest that says `"uia"` still loads — names are not checked — and the first `host.element` call then raises.

See [what that list is and is not](./index.md#capabilities).

## Asking without waiting {#asking-without-waiting}

Every call on this page but [`pending`](#host-element-pending) takes two more arguments after its own, either of which may be left out: `opts`, a table whose one option is `key`, and a callback `cb`, last. They are counted, as [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) counts its own: one value after the call's arguments is the callback when it is a function, else `opts` (a table, or `nil`) — `host.element.findAny(hwnd, names, types, cb)` — and two are `opts` and the callback, which must be a function then.

**With a callback** the call returns nothing, at once, wherever it is made, and `cb` is called with exactly what the call would have returned, on a later turn of the event loop, as a [handler](../module-runtime-and-lifecycle.md#handlers) of the module — at once while the module is free, after its earlier events while it is busy — in the lane it was asked from: a question asked from a key, a hotkey, a window or focus change is asked before a poll's. It is **dropped**, `cb` never called, when the module is disabled, reloaded, removed or stopped by the host's guard first — and a question of such a module that was not asked yet is not asked at all: a `press` or a `focusStep` it handed over is not made. Nothing here reads the screen, so an answer carries no `time` or `inputEpoch`. **Without a callback** the call returns the answer, and the event loop waits for it; the platform sections say what that wait lets through.

**Not known is not "no".** A program that does not answer is not a program without the element. When a question could not be answered — the program did not answer in time, runs with higher rights, went away while it was asked — or was not put to the program at all, every call answers `nil` and an [error value](./errors.md#error-values) that says why, `nil, err`: `find`, which otherwise answers `true` or `false`, too. A press answers `false, err`. **A dump answers what it found and, beside it, the error value that says why it is not all of the tree** — a [partial answer](./errors.md#partial-answers): the list is there *and* so is `err`, so test `err` to know whether the list is the whole tree; one that read nothing answers `{}, err`. Compare `err.code`, never the message. The codes every question can give on Windows, with their messages and when each comes, are [the question errors](#question-errors), and a Mac's are under [`find`'s Errors](#host-element-find-errors); an entry's own table adds what only it gives — the bounds of a walk, a dump's cut, a press's refusals. On macOS a question is "not known" when its application did not answer one of its reads, when the application did not answer a moment ago and is not asked, and when a walk stopped at its bound and found nothing ([macOS](#asking-without-waiting-macos)).

Nothing in the host turns a "not known" into a no, and a module that keeps what it learns must not either: ask again later. The epoch's store keeps the program's own "not known" — the bounds of a walk included — as not known until the epoch turns over; what the host says without putting the question to the program — not asked behind an earlier question, replaced, refused inside a wait, a thread that could not start, a window of this application, on a Mac an application not asked for 5 s after it did not answer — it does not keep. An overlay's kept `identify` that meets one raises rather than answer `nil`: the overlay runtime takes a raise as no verdict and never counts it, where it keeps the eighth `nil` in a row as a no ([`O:attachEmbedded`](./overlay.md#o-attachembedded)); Komplete Kontrol's module does so, and raises the same while its answer has not come.

**One question at a time.** The questions are asked one after another, so no program ever has more than one of this application's to answer: the one the event loop waits for first, then those asked from what a person did, then a poll's, each in the order asked — but never a question before one that acts on the same program — `press`, `focusStep`, `focusWithin` — and was asked before it: what it found would be from before that act. Such an act is asked as early as the most urgent question waiting behind it, so a call without a callback made right after a `press` with one waits for the press and then for its own answer, which is from after it. A question the same as one still waiting to be asked — the same call with the same arguments, from any module — is not asked twice: everyone who asked it gets the one answer, unless something acted on the screen between the two ([`host.inputEpoch`](./timer.md#host-inputepoch) turned over, or a question that acts on the same program waits between them). So is one the same as the question being asked right now, unless the input epoch turned over since that one was put, for its answer may be from before. A question that acts is always asked. When a program does not answer one question in time, the questions to it that waited behind that one are answered "not known" at once, without being put to it; a question asked after that is put to it again.

**`opts.key`** names what a question is *for*: a newer question of the same module with the same key — with a callback or without, answered from the epoch's store or asked — takes out those with that key that have not been asked yet. Their callbacks get `nil, err` with `err.code` `replaced` (a press, `false, err`; a dump, `{}, err`), and a question nobody else waits for is not asked; the one being asked is answered as it is. Nothing can replace a call without a callback while the event loop waits for it. [`host.element.pending(key)`](#host-element-pending) says whether a question with the key, asked with a callback, is out.

**What raises:** `opts` that is neither a table nor `nil`, an option other than `key`, a `key` that is not a non-empty string, a callback that is not a function — an explicit `nil` where it goes included, which is almost always a callback that is missing — and anything after the callback, `nil` included. Each message begins with the call's name: `host.element.find: opts.key must be a non-empty string, got 3`; `host.element.find: the callback is nil — pass a function to be called back with the answer, or leave the argument out to wait for it`. A question that gets no answer never raises.

```luau
-- Whether a window is a Komplete Kontrol, asked off the event loop. A yes never changes for the
-- window and is kept; a no is asked again in the next epoch, since a Komplete Kontrol whose tree
-- is not up yet answers no the instant it appears; a question the plug-in did not answer is no
-- verdict at all. Needs "element" and "window" in the manifest.
local U = host.element.type
local known = {}  -- by window id: { kk = true | false | nil, asking, askedIn }
local function kkOf(id)
  local k = known[id] or {}
  known[id] = k
  if k.kk ~= true and not k.asking and k.askedIn ~= host.epoch() then
    k.asking, k.askedIn = true, host.epoch()
    host.element.findAny(id, { "Komplete Kontrol" }, { U.Window, U.Pane }, function(i, err)
      k.asking = false
      if err then return end         -- not known: the last answer stands
      if k.kk ~= (i ~= nil) then
        k.kk = i ~= nil
        host.window.recheck()        -- an answer is no window event: look again
      end
    end)
  end
  return k.kk                        -- nil until it is known
end
```

### The errors every question can give {#question-errors}

Every call on this page but `pending` and `focusWithin` can give these on Windows, where the questions are asked on the host's accessibility thread; each entry's table links here and adds its own. `focusWithin` asks the program nothing there, so of these it gives only the host's own, which its table lists. None of them is raised. Each arrives as the call's second value without a callback and as the callback's second argument with one — beside `nil`, a press's `false` or a dump's `{}` — except where its row says it arrives one way only. A Mac's are under [`find`'s Errors](#host-element-find-errors).

| code | when | message | platform |
|---|---|---|---|
| `timeout` | The program did not answer a call within 1000 ms: it is busy, or hung. Or the question waited behind one to the same program that did not answer in time, and was not put to it; the message then begins with the earlier one's: `the program did not answer within 1000 ms to an earlier question, so this one was not asked`. Only a timeout leaves the questions behind it unasked. | `the program did not answer within 1000 ms`, `… to an earlier question, so this one was not asked` | Windows |
| `denied` | The program runs with higher rights than this application. | `the program runs with higher rights` | Windows |
| `gone` | The program went away while it was asked. | `the program is gone` | Windows |
| `replaced` | To the callback only, with `opts.key`: a newer question of the module with the same key came before this one was put to the program. It is not asked. | `a newer question with the same key was asked before this one` | Windows |
| `busy` | As the second value only, without a callback: asked while the event loop already waits for another question — from code that something sent to one of this application's windows ran. Nothing in the host asks one so. | `another accessibility question is being waited for` | Windows |
| `refused` | `hwnd` is a window of this application: a page, the module manager. It is not asked about. | `the window is this application's own, which is not asked about` | Windows |
| `failed` | The host could not start its accessibility thread, and answers every question so for the session; or the thread's code failed while it asked, and the thread goes on. The message carries the system's reason or the failure's report. | `the host could not start its accessibility thread: …`, `the host's accessibility thread failed: …` | Windows |
| `failed` | UI Automation's object could not be made on the accessibility thread, or could not build a condition of the question, so nothing could be asked. | `this system's accessibility interface could not be reached` | Windows |
| `failed` | As the second value only, without a callback: the event the host waits on could not be made, or the answer did not come back. Neither is known to happen. | `the event the host waits on for the answer could not be made`, `the answer was lost` | Windows |

### Windows {#asking-without-waiting-windows}

Every question is asked on the host's `accessibility` thread, started at the first one, and never on the event loop. The thread has no window, lives in the multithreaded COM apartment — as Microsoft recommends for a UI Automation client — and holds the automation object, whose every call into another program is bounded at 1000 ms (`IUIAutomation2`'s connection and transaction timeouts); a walk stops at the first call the program did not answer. A question that took 50 ms or more there gets a line, `[uia] findAny(Kontakt 8/Kontakt 7) of window 4589 (process 4242) took 1003 ms on the accessibility thread: not known: the program did not answer within 1000 ms`, and so do the questions answered "not known" without being asked behind it.

Without a callback the event loop waits for the answer — for the question the thread is asking at that moment, if there is one, and for a question that acts on the same program asked before it, and then for its own, which goes before every other question still waiting — in a wait that answers what other threads and programs *send* to the application's windows: a screen reader asking one of them what it holds (`WM_GETOBJECT`, and the messages the system's accessibility proxies send to read a control) is answered while the event loop waits, which it was not while the event loop asked the question itself. Nothing *posted* runs in that wait: timers, keys and every module's events wait, as they did. A wait of 50 ms or more is named in the log as before, with how much of it went by before the thread took the question: `[observe] uia.findAny(Komplete Kontrol) blocked the pump for 1459 ms`, or `… blocked the pump for 5300 ms, 5004 ms of it behind questions asked before it`. That part is another question's, often another module's, and the host's [guard](../module-runtime-and-lifecycle.md#limits) does not count it on the callback's 10 s of wall clock; the rest of the wait it counts, as it counts any slow call.

A window of this application itself — a [`host.gui.webview`](./gui.md) page, the manager — is not asked about: every call answers `nil, err`, code `refused`, message `the window is this application's own, which is not asked about` (a press, `false, err`; a dump, `{}, err`), with a callback or without. Its accessibility is answered on the event loop, in part through calls *posted* to it, which the wait above does not run: the thread would wait for the event loop and the event loop for the thread.

A window that is gone — an `hwnd` that names no window, `0` included — holds nothing, and every question about it says so without a word to any program: `find` answers `false`, a dump `{}`, the others `nil`. Only a press says why, `false, err` with `the window is gone`. A system whose automation object cannot be made, or that cannot build a question's condition, can ask nothing: every question answers `nil, err` — `failed`, `this system's accessibility interface could not be reached` — a press `false, err` and a dump `{}, err`, never "nothing found".

### macOS {#asking-without-waiting-macos}

There is no such thread yet: a question is asked on the event loop, as before, with Accessibility's own bound of 1 s on each read and the host's quarantine of an application that does not answer; a callback still gets its answer on a later turn, never inside the call. A question is "not known" as on Windows, with the codes under [`find`'s Errors](#host-element-find-errors) — `nil, err`, a dump its list so far and `err` — when:

- a read of it went unanswered: the application is busy, starting or not responding (`kAXErrorCannotComplete`), `timeout` — `the program did not answer: it is busy, starting or not responding`. What was found before it stands. The walk goes on, and the application is then left alone for 5 s;
- the application did not answer a read a moment ago, and is not asked for 5 s after it — `timeout`, `the program did not answer a moment ago, so it is not asked for 5 s`. The question is not put to it, so the epoch's store does not keep the answer;
- a walk stopped at its bound before the end of the tree and found nothing — each call's bound is at its macOS block: `too_large` for its count of elements, `timeout` for its time. A walk asks its time before every read of the application — the snapshot of each element, its children, and what the call reads of an element it found — and does not begin one past it: it stops there, at most one read, 1 s, after its time, and is told as past its time. Elements below its depth are not read, and nothing says so.

A walk that met reads the application could not complete writes one line about them as it ends: each element by its place — the child numbers from the walk's root — and what it is, each attribute not answered (read on its own, or one of an element's attributes read together whose call answered for the rest), and how many walks of its kind in that application so far met one, the walks that met none counted too. `[macos] an accessibility search walk in REAPER, under AXWindow 'FX: Track 1', met 2 read(s) the application could not complete (kAXErrorCannotComplete) at 1 element(s): at 2.1, AXGroup 'Main': AXChildren ×2; 37 element(s) read — 1 of 412 search walk(s) in REAPER so far met one`. The kinds are `search` (every search on this page), `focus` ([`focusStep`](#host-element-focusstep), [`focusWithin`](#host-element-focuswithin)), `dump` ([`dump`](#host-element-dump), [`rawDump`](#host-element-rawdump)) and `controls` ([`host.window.controls`](./window.md#host-window-controls)); up to six elements are named and the rest counted, and a title is quoted to its first 40 characters. One such line is written in 5 s at most for a kind of walk in an application: the walks of it that met such a read in between are counted in the next one, which then ends `; 3 more such walk(s) since the last line, which comes once in 5 s at most`, and their elements are named nowhere. The counts are kept for each running application — one started again begins at 0 — and hold only the walks made: a read not answered on its own also starts the 5 s above, and a walk a question would have made into the application in them is not made, so it is in neither count. Nothing else changes for it: the walk goes on past such a read, as above.

No question waits behind another here, so none is left unasked behind one. Unchecked on a Mac so far: TODO.md lists what one has to show.

## host.element.find(hwnd, name, controlType, opts?, cb?) {#host-element-find}

**Signature:** `host.element.find(hwnd: number, name: string, controlType: number, opts: { key: string? }?, cb: function?) -> boolean | (nil, Error)`, and nothing with `cb`

Returns `true` if the UI Automation subtree of the window `hwnd` contains at least one element whose Name equals `name` **and** whose ControlType equals `controlType`; otherwise `false` — and `nil, err` when the program did not answer ([Asking without waiting](#asking-without-waiting)). Primarily used for plugin/window identity checks.

Pass an **empty `name`** (`""`) to match any element of that control type (i.e. test only the control type, ignoring the Name).

Common UIA control-type ids:

| Control | id |
| --- | --- |
| Button | `50000` |
| Edit | `50004` |
| Menu | `50009` |
| MenuItem | `50011` |
| Window | `50032` |
| Group / Pane | `50033` |

```luau
-- Is this window a Kontakt instance? (a Pane named "PlogueXMLGUI" exists)
local hwnd = host.window.active().id
if host.element.find(hwnd, "PlogueXMLGUI", 50033) then
    host.log.info("Kontakt detected")
end

-- Does the window contain *any* Edit control?
if host.element.find(hwnd, "", 50004) then
    host.log.info("has at least one edit box")
end
```

### Returns {#host-element-find-returns}

- `true` — the tree of `hwnd` holds at least one element with that Name and ControlType.
- `false` — it holds none, as far as the search sees: the search reached the end of the tree, every read answered. A window that is gone is `false` too, and so is a control type with no role on macOS ([`host.element.type`](#host-element-type-macos)).
- `nil, err` — not known: the program did not answer, the question was not put to it, or the search stopped at its bound before the end of the tree. Not a no: ask again later. `err.code` is one of the codes below.
- With a callback: nothing at once; `cb(true)`, `cb(false)` or `cb(nil, err)` on a later turn of the event loop ([Asking without waiting](#asking-without-waiting)).

### Errors {#host-element-find-errors}

None of these is raised. On Windows every question gives [the question errors](#question-errors); the rows after that link are a Mac's, and every search on this page — `locate`, `findAny`, `locateVia`, `pluginLocate`, `stateProbe`, `classNavPoint` — gives them too, so those link here.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [the question errors](#question-errors) | both | The program did not answer, or the question was not put to it; each arrives as its row there says. | as there | Windows |
| `timeout` | both | A read of the search went unanswered: the application is busy, starting or not responding (`kAXErrorCannotComplete`, at once or after Accessibility's 1 s). Nothing was found before it; the application is then left alone for 5 s. | `the program did not answer: it is busy, starting or not responding` | macOS |
| `timeout` | both | The application did not answer a read a moment ago, and is not asked for 5 s after it: the question is not put to it. | `the program did not answer a moment ago, so it is not asked for 5 s` | macOS |
| `too_large` | both | The walk read 1500 elements before the end of the tree, and found nothing. | `the tree holds more than the 1500 elements a search reads` | macOS |
| `timeout` | both | The walk took longer than 300 ms before the end of the tree, and found nothing: the application answers every read, slowly. Also, without a walk, every later question to the same application in the same turn of the event loop. | `the search took longer than 300 ms` | macOS |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `name` that is neither a string nor a number; a `controlType` that is not a number in the 32-bit range; and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting). A question that gets no answer never raises.

### Windows {#host-element-find-windows}

A subtree search performed by UIA itself, with no traversal budget of ours.

The documented "is a menu open" idiom — an empty name with `host.element.type.Menu` — is exactly that: an ordinary search for any element of that control type.

### macOS {#host-element-find-macos}

A hand-rolled walk capped at **1500 nodes, depth 24 and 300 ms**, and at 256 children of any one element. The 300 ms is asked before every read, so a walk ends at most one read — Accessibility's 1 s — after it ([Asking without waiting](#asking-without-waiting-macos)). A walk that stops at its count or its time before the end of the tree and found nothing answers `nil, err` — `too_large`, `the tree holds more than the 1500 elements a search reads`, or `timeout`, `the search took longer than 300 ms` — and so does an application in the five-second not-answering quarantine, which is not asked at all (`timeout`), never `false`. A once-per-session log line says which bound a walk first met.

**A walk that ran out of time is not made again in the same turn.** Once a walk of any module's question — a search on this page, or `focusStep` and `focusWithin` — has stopped at its 300 ms in an application, every later question to that application in the same turn of the event loop is answered at once, `nil, err` with that same `timeout`, `the search took longer than 300 ms`, without a walk: seven overlays asking their identity questions in one window would otherwise pay 300 ms each, on the event loop that every captured key's delivery waits for. The next turn (about 15 ms on) asks the application afresh. A dump is walked all the same. The turn ends with one line, `accessibility: <n> question(s) of modules in one turn were answered 'too slow' without a walk — … : <n> to '<application>' (pid <pid>)`. Elements below depth 24, and children past the 256th, are not read, and nothing says so: a tree that deep can make an identity check answer `false` here while it succeeds on Windows.

The menu idiom is special-cased rather than searched: it asks the *application* whether a menu is open, walking its children and one level into its other windows, and never descends the menu bar itself. `locate` has no such special case.

## host.element.locate(hwnd, name, controlType, opts?, cb?) {#host-element-locate}

**Signature:** `host.element.locate(hwnd: number, name: string, controlType: number, opts: { key: string? }?, cb: function?) -> { x: number, y: number } | nil | (nil, Error)`, and nothing with `cb`

Finds the first UIA element in window `hwnd` matching `name` + `controlType` and returns the screen-pixel centre of its bounding rectangle as `{ x, y }`, suitable for clicking. Returns `nil` if no match, and `nil, err` when the program did not answer. As with `find`, an empty `name` matches any element of the given control type. Same control-type ids as above.

```luau
local hwnd = host.window.active().id
local pt = host.element.locate(hwnd, "Play", 50000) -- Button named "Play"
if pt then
    host.input.click(pt.x, pt.y)
end
```

### Returns {#host-element-locate-returns}

- `{ x, y }` — the screen centre of the first matching element's rectangle, whole numbers in the platform's screen coordinates.
- `nil` — no element matches, or the first match has no rectangle there (an empty one). A window that is gone is `nil` too.
- `nil, err` — not known: the program did not answer, the question was not put to it, or the search stopped at its bound ([macOS](#host-element-locate-macos)); `err.code` is under Errors.
- With a callback: nothing at once; `cb({ x, y })`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-locate-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [`find`'s errors](#host-element-find-errors) | both | The program did not answer, the question was not put to it, or (macOS) the search stopped at its bound; each arrives as it does there. | as there | both |

**Raises** for a mistake in the call, as [`find`](#host-element-find-errors) does.

### macOS {#host-element-locate-macos}

Subject to the same **1500 node / depth 24 / 300 ms** traversal budget as `host.element.find`: a search that stops at it and found nothing answers `nil, err`, as `find`'s does, never `nil` alone.

## host.element.type {#host-element-type}

**Signature:** `host.element.type: { [string]: number }`

The UIA ControlType ids by name — `host.element.type.Button` (50000), `.Edit`, `.Menu`, `.MenuItem`, `.Tab`, `.Window` (50032), `.Pane` (50033), and the rest. Pass these instead of writing the integers at call sites.

```luau
local U = host.element.type
if host.element.find(hwnd, "Kontakt 8", U.Pane) then … end
```

### Returns {#host-element-type-returns}

A table of 39 names, `Button` (50000) to `Separator` (50038), each a number. It is a plain table, made for each module's VM when the module loads, so writing to it changes only that module's copy; reading a name it does not have is `nil`, which a call then raises for as a `controlType` that is not a number.

### Windows {#host-element-type-windows}

Every constant in the table is a real UIA property condition and can match.

### macOS {#host-element-type-macos}

The ids are translated to accessibility roles, and **ten of them have no mapping at all**: `Calendar`, `StatusBar`, `Thumb`, `DataGrid`, `DataItem`, `Document`, `SplitButton`, `Header`, `HeaderItem` and `TitleBar`. A query whose control type is one of those is well-formed, can never succeed, and says so only in the log — which the module never sees.

`findAny` behaves differently again: given several types it **drops the unmapped ones and searches the rest**, returning nothing only when every type it was given was unmapped. So the same unmapped id makes `find` impossible and makes `findAny` quietly narrower.

## host.element.findAny(hwnd, names, types, opts?, cb?) {#host-element-findany}

**Signature:** `host.element.findAny(hwnd: number, names: {string}, types: {number}, opts: { key: string? }?, cb: function?) -> number | nil | (nil, Error)`, and nothing with `cb`

Answers "is any of these names present as any of these control types?" — the shape a plugin-identity check takes, since a plugin may expose its name as a Window on one host and a Pane on another. Returns the **1-based index of the matching name**, so a caller learns *which* one matched (e.g. the plugin's version) rather than asking once per candidate. `nil` if none match, and `nil, err` when the program did not answer — the search stops at the first name the program did not answer for; an empty name matches any element of the given types.

One tree traversal per name, with the types folded into the condition — where a loop of `find` calls costs one full subtree walk per name×type pair. This runs on every detection pass, per candidate control, so the difference is not academic.

```luau
local U = host.element.type
local VERSIONS = { "Kontakt 8", "Kontakt 7" }
local i = host.element.findAny(ctrl.id, VERSIONS, { U.Window, U.Pane })
if i then host.log.info("this is " .. VERSIONS[i]) end
```

### Returns {#host-element-findany-returns}

- A number — the 1-based index in `names` of the earliest name present as any of `types`. With two present, the one earlier in `names` wins.
- `nil` — none is present. An empty `names` or `types` asks nothing and is `nil`; so is a window that is gone and, on macOS, a `types` list none of whose types has a role there ([`host.element.type`](#host-element-type-macos)).
- `nil, err` — not known: on Windows the search stops at the first name the program did not answer for, and the names after it are not asked; on a Mac one walk reads every name, and it stopped at an unanswered read, at its bound, or was not put to the application. `err.code` is under Errors.
- With a callback: nothing at once; `cb(index)`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-findany-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [`find`'s errors](#host-element-find-errors) | both | The program did not answer, the question was not put to it, or (macOS) the search stopped at its bound; each arrives as it does there. | as there | both |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `names` that is not a table, or holds a value that is neither a string nor a number; a `types` that is not a table, or holds a value that is not a number in the 32-bit range; and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting). Each list is read up to its first `nil`.

## host.element.pluginLocate(hwnd, containerName, name, controlType, opts?, cb?) {#host-element-pluginlocate}

**Signature:** `host.element.pluginLocate(hwnd: number, containerName: string, name: string, controlType: number, opts: { key: string? }?, cb: function?) -> { x: number, y: number } | nil | (nil, Error)`, and nothing with `cb`

Like `locate`, but for a plugin hosted inside another application. It first finds the element that **is** the plugin — Name `containerName` (e.g. `"Kontakt 8"`) with control type Window (50032) or Pane (50033), preferring the `ni::qt::QuickWindow` class (a Qt plugin's scene root, which carries the content) over the `…QWindowIcon` window host — and then searches for the target **within** it, using the **raw** tree walker. A port of ReaHotkey's `GetPluginUIAElement` + `MainElement.FindElement(...)`.

The raw walker matters: the condition-based search (`find` / `locate`) stops at a hosted fragment's boundary, so a plugin's UI can look entirely absent to it. Note that some plugins expose no accessible content when embedded regardless — a DAW-embedded Kontakt 8 is a UIA **leaf** — in which case this returns `nil` and only coordinates remain.

```luau
local pt = host.element.pluginLocate(hwnd, "Kontakt 8", "Kontakt File Menu", 50000)
if pt then host.input.click(pt.x, pt.y) end
```

### Returns {#host-element-pluginlocate-returns}

- `{ x, y }` — the screen centre of the first target with a rectangle found inside a container, whole numbers. What a walk found stands, even when it stopped before the end of the tree.
- `nil` — no target: no container of that name (on macOS a non-empty `name` is then looked up on the whole window, below), or no container that holds one with a rectangle. A window that is gone is `nil` too.
- `nil, err` — not known: the program did not answer, the question was not put to it, or a walk stopped at its bound before the end of the tree and found nothing — the part not walked may hold the target. `err.code` is under Errors.
- With a callback: nothing at once; `cb({ x, y })`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-pluginlocate-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [`find`'s errors](#host-element-find-errors) | both | The program did not answer, the question was not put to it, or (macOS) a walk stopped at its bound; each arrives as it does there. | as there | both |
| `too_large` | both | A walk read 4000 elements before the end of the tree, and found nothing. | `the tree holds more than the 4000 elements a search reads` | Windows |
| `timeout` | both | A walk took longer than 5 s before the end of the tree, and found nothing: the program answers every call, slowly. | `the search took longer than 5 s` | Windows |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `containerName` or `name` that is neither a string nor a number; a `controlType` that is not a number in the 32-bit range; and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

### Windows {#host-element-pluginlocate-windows}

A hand-rolled walk of its own, bounded at 4000 nodes and depth 40 — deliberately larger than the ordinary search, because a hosted plug-in fragment is where the deep trees are — and at 5 s for each of its two walks (the containers, then each container's content), against a program that answers every call in time but slowly. A walk stopped at its node budget or its time before the end of the tree, that found nothing, answers `nil, err` — code `too_large`, `the tree holds more than the 4000 elements a search reads`, or `timeout`, `the search took longer than 5 s` — never `nil` alone: the part it did not walk may hold the element. What it found stands. Elements below depth 40 are not looked at, and nothing says so: that is the call's reach, not a cut.

### macOS {#host-element-pluginlocate-macos}

The general **1500 node / depth 24 / 300 ms** budget applies here too, to each walk — the containers, then each container's content — with no larger allowance for this call. A plug-in tree that this reaches on Windows can be out of reach here; a walk stopped at the budget that found nothing answers `nil, err`, as `find`'s does, so it is not read as a genuine miss.

When **no** element named `containerName` exists at all, a non-empty `name` is looked up on the whole window instead, as `locate` would. There is no fragment boundary to cross on this platform, and a plug-in hosted in a DAW need not publish an element named after itself: inside REAPER, Kontakt 8's groups hang straight off the FX window, whose title is `FX: Track 1 "Kontakt 8"`. Two cases still answer `nil`: a container that exists but does not hold the target, and an empty `name`, which on a whole DAW window would mean the DAW's own first button.

## host.element.rawDump(hwnd, opts?, cb?) {#host-element-rawdump}

**Signature:** `host.element.rawDump(hwnd: number, opts: { key: string? }?, cb: function?) -> ({ { depth: number, name: string, class: string, ctype: number, bounds: { x: number, y: number, w: number, h: number } }, … }, Error?)`, and nothing with `cb`

Diagnostic counterpart to `host.element.dump`, walking the **raw** tree instead of a condition-based search, so it crosses into hosted fragments the latter cannot see. Bounded by node budget, time and depth. Use it to find out whether a plugin exposes any accessible content at all before building on UIA. The list is never `nil`, and it can come **with an error value beside it**: what was read before the walk stopped, and why it stopped ([Returns](#host-element-rawdump-returns)).

Each node carries `bounds = { x, y, w, h }` in screen coordinates, which is usually the reason to reach for this: a rectangle measures things nothing else can — a window's own close button says how tall its title bar is, and that is the difference between an authored coordinate landing on a control and landing a title bar above it.

```luau
-- Does this plug-in publish anything at all? The probe dumps the whole tree, because on a
-- machine nobody here can see, the log is the only way to find out.
local win = host.window.active()
if not win then return end
local elements, err = host.element.rawDump(win.id)
host.log.info(string.format("%d element(s) in '%s'%s", #elements, win.title,
  err and (" — not all of the tree: " .. err) or ""))
for _, e in ipairs(elements) do
  local b = e.bounds
  host.log.info(string.format("  %sdepth=%d type=%d class='%s' name='%s' at %d,%d %dx%d",
    string.rep(" ", math.min(e.depth, 12)), e.depth, e.ctype, e.class, e.name,
    b.x, b.y, b.w, b.h))
end
```

### Returns {#host-element-rawdump-returns}

:::warning A list and an error value together
`rawDump` can answer **a list and an error value at once**: the elements read before the walk stopped, and why it stopped. That is a [partial answer](./errors.md#partial-answers). Use the list either way, and test `err` to know whether it is all of the tree. `if err then return end` throws away what was read, and `#list == 0` cannot tell a small tree from a cut one.

```luau
local list, err = host.element.rawDump(win.id)
for _, e in ipairs(list) do host.log.info(e.name) end   -- what was read, all of it or not
if err then host.log.info("only part of the tree: " .. err) end
```
:::

- `{ node }` alone — every element of the raw tree that has a Name or a class, as `{ depth, name, class, ctype, bounds }`. With no error value beside it, the list is the whole tree. It can be empty: nothing in the tree is named, or the window is gone.
- `{ node }, err` — **a partial answer**: the elements read before the walk stopped, perhaps none, and why it stopped. The program stopped answering (`timeout`, `denied`, `gone`), or the walk reached its bound — 4000 elements or 5 s on Windows, 2000 elements or 5 s on a Mac (`too_large`, `timeout`).
- `{}, err` — nothing was read, because the question was not put to the program: not asked behind an earlier one, replaced, this application's own window, an application in a Mac's quarantine, and the rest of [the question errors](#question-errors).
- With a callback: nothing at once; `cb(list)` or `cb(list, err)` on a later turn, with the same values.

### Errors {#host-element-rawdump-errors}

None of these is raised. Those that say so come **beside the list read so far**; the rest beside `{}`.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `timeout` | both | The program did not answer a call of the walk within 1000 ms. Beside the elements read before it. | `the program did not answer within 1000 ms` | Windows |
| `denied` | both | The program runs with higher rights. Beside what was read, usually nothing. | `the program runs with higher rights` | Windows |
| `gone` | both | The program went away during the walk. Beside the elements read before it. | `the program is gone` | Windows |
| `too_large` | both | The walk read 4000 elements before the end of the tree. Beside those it listed. | `the tree holds more than the 4000 elements a search reads` | Windows |
| `timeout` | both | The walk took longer than 5 s before the end of the tree. Beside the elements read until then. | `the search took longer than 5 s` | both |
| `timeout` | both | A read of the walk went unanswered: the application is busy, starting or not responding. Beside the elements read before it; the walk goes on past it. | `the program did not answer: it is busy, starting or not responding` | macOS |
| `too_large` | both | The walk read 2000 elements before the end of the tree. Beside those it listed. | `the tree holds more than the 2000 elements a dump reads` | macOS |
| `timeout` | both | The application did not answer a read a moment ago, and is not asked for 5 s after it. Beside `{}`. | `the program did not answer a moment ago, so it is not asked for 5 s` | macOS |
| the rest of [the question errors](#question-errors) | both | The question was not put to the program, or the host failed: beside `{}`. Each arrives as its row there says. | as there | Windows |

**Raises** for a mistake in the call: an `hwnd` that is not a number, and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

## host.element.dump(hwnd, opts?, cb?) {#host-element-dump}

**Signature:** `host.element.dump(hwnd: number, opts: { key: string? }?, cb: function?) -> ({ { depth: number, name: string, class: string, ctype: number, bounds: { x: number, y: number, w: number, h: number } }, … }, Error?)`, and nothing with `cb`

The condition-based counterpart to [`host.element.rawDump`](#host-element-rawdump), and the first thing to run against a window nobody here has seen: every element of `hwnd`'s subtree that carries a Name or a class key, as `{ depth, name, class, ctype, bounds }`. It is what tells you the Name/ControlType pair to key an identity check on, and the rectangles are what a coordinate can be authored against. Where this comes back empty or suspiciously thin against a plug-in hosted inside another application, that is the fragment boundary the condition-based search stops at — reach for `rawDump`, which crosses it.

`bounds` is in screen coordinates. An element the toolkit never places reports all zeros, which is a legitimate answer and not a miss. Every property is fetched across the process boundary per element, so this belongs on a diagnostic hotkey, not on a focus step. As with `rawDump`, **the list can come with an error value beside it**, saying why it is not all of the tree ([Returns](#host-element-dump-returns)).

```luau
-- Ctrl+Alt+U in tools/inspect: dump the focused window, then every Qt plug-in control in
-- it — on a machine nobody here can see, the log is the only way to learn what a tree holds.
local win = host.window.active()
local function dumpOne(label, id)
  local list, err = host.element.dump(id)
  host.log.info(string.format("UIA DUMP %s id=%d", label, id))
  for _, e in ipairs(list) do
    host.log.info(string.format("  type=%-5d class='%s' name='%s'", e.ctype, e.class, e.name))
  end
  if err then host.log.info("  (not all of the tree: " .. err .. ")") end
end
dumpOne("window '" .. win.title .. "'", win.id)
for _, c in ipairs(host.window.controls()) do
  if string.match(c.class, "Qt%d") then dumpOne("control [" .. c.class .. "]", c.id) end
end
```

### Returns {#host-element-dump-returns}

:::warning A list and an error value together
`dump` can answer **a list and an error value at once**: the elements listed, and why that is not all of the tree. That is a [partial answer](./errors.md#partial-answers). Use the list either way, and test `err` to know whether it is all of the tree.

```luau
local list, err = host.element.dump(win.id)
for _, e in ipairs(list) do host.log.info(e.name) end    -- what was listed, all of it or not
if err and err.code == host.errors.TOO_LARGE then
  host.log.info("the first 2000 elements only: " .. err)
end
```
:::

- `{ node }` alone — every element of the subtree that has a Name or a class, as `{ depth, name, class, ctype, bounds }`. With no error value beside it, the list is the whole tree. It can be empty: nothing in the tree is named, or the window is gone.
- `{ node }, err` — **a partial answer**: the elements listed, perhaps none, and why that is not all. The tree holds more than the 2000 elements a dump lists (`too_large`), a Mac's walk took longer than 5 s (`timeout`), or the program stopped answering (`timeout`, `denied`, `gone`).
- `{}, err` — nothing was read, because the question was not put to the program: not asked behind an earlier one, replaced, this application's own window, an application in a Mac's quarantine, and the rest of [the question errors](#question-errors).
- With a callback: nothing at once; `cb(list)` or `cb(list, err)` on a later turn, with the same values.

### Errors {#host-element-dump-errors}

None of these is raised. Those that say so come **beside the list read so far**; the rest beside `{}`.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `too_large` | both | The tree holds more than 2000 elements. The first 2000 are read, and those with a Name or a class listed. | `the tree holds … elements, more than the 2000 a dump lists` | Windows |
| `timeout` | both | The program did not answer a call within 1000 ms. Beside the elements listed before it. | `the program did not answer within 1000 ms` | Windows |
| `denied` | both | The program runs with higher rights. Beside what was listed, usually nothing. | `the program runs with higher rights` | Windows |
| `gone` | both | The program went away while the elements were read. Beside those listed before it. | `the program is gone` | Windows |
| `too_large` | both | The walk read 2000 elements before the end of the tree. Beside those it listed. | `the tree holds more than the 2000 elements a dump reads` | macOS |
| `timeout` | both | The walk took longer than 5 s before the end of the tree. Beside the elements read until then. | `the search took longer than 5 s` | macOS |
| `timeout` | both | A read of the walk went unanswered: the application is busy, starting or not responding. Beside the elements read before it; the walk goes on past it. | `the program did not answer: it is busy, starting or not responding` | macOS |
| `timeout` | both | The application did not answer a read a moment ago, and is not asked for 5 s after it. Beside `{}`. | `the program did not answer a moment ago, so it is not asked for 5 s` | macOS |
| the rest of [the question errors](#question-errors) | both | The question was not put to the program, or the host failed: beside `{}`. Each arrives as its row there says. | as there | Windows |

**Raises** for a mistake in the call: an `hwnd` that is not a number, and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

### Windows {#host-element-dump-windows}

One `FindAll` over the subtree, returning a **flat** list capped at the first 2000 elements: `depth` is `0` on every node here, so nesting has to be read from the rectangles rather than from the field. `rawDump` is the one that reports real depth. A tree larger than that is listed in part, and the error value beside the list says so, `too_large`: `the tree holds 2875 elements, more than the 2000 a dump lists`.

`rawDump` here walks at most 4000 elements and 5 s, depth 40, and says the same of a walk that stopped at the first two, `too_large` or `timeout`: `the tree holds more than the 4000 elements a search reads`, `the search took longer than 5 s`. Elements below depth 40 are not read, and nothing says so.

### macOS {#host-element-dump-macos}

Accessibility has only one view of the tree, so `dump` and `rawDump` are the same call and return the same list — a `dump` is never the thinner answer it can be on Windows. Bounded at **2000 nodes, depth 24 and 5 s**, and `depth` is the real nesting depth. A dump that stops at its count or its time is the list read until then with the error value beside it — `too_large`, `the tree holds more than the 2000 elements a dump reads`, or `timeout`, `the search took longer than 5 s` — and so is one that met a read its application did not answer. Elements below depth 24 are not read, and nothing says so; the dump's own line in the log says whether either bound was reached.

`class` is not a class name, because this platform has none: it is the joined `role/subrole/AXIdentifier` key, printed exactly as a module's pattern would match it, so what appears in the log can be pasted straight into the module.

## host.element.locateVia(hwnd, viaName, viaType, name, controlType, opts?, cb?) {#host-element-locatevia}

**Signature:** `host.element.locateVia(hwnd: number, viaName: string, viaType: number, name: string, controlType: number, opts: { key: string? }?, cb: function?) -> { x: number, y: number } | nil | (nil, Error)`, and nothing with `cb`

Like [`locate`](#host-element-locate), but in two levels: it first finds a container element (`viaName` + `viaType`) anywhere under `hwnd`, then searches for the target **within that container** and returns its click centre. That matters for a plug-in that publishes an identity pane and hangs its real UI off it as a nested fragment — a search that starts at the window misses what a search that starts at the container finds. An empty `name` (or `viaName`) matches any element of the given type, as elsewhere in this module. Two subtree searches, so roughly twice the cost of one `locate`.

The difference from [`pluginLocate`](#host-element-pluginlocate) is that you choose the container's control type here; `pluginLocate` accepts only a Window or Pane container, prefers the Qt scene root over the content-empty window host, and searches with the raw walker. That is the harder-edged version and the one the shipped Kontakt module uses — nothing in `modules/` calls `locateVia`. Reach for it when the container you need is neither a Window nor a Pane.

```luau
local U = host.element.type
-- Take both names from a dump first: the container is the element that publishes the
-- plug-in's identity, the target is what you want to click inside it. Shown with a Pane
-- container for familiarity; a Pane is also what `pluginLocate` would handle for you.
local fg = host.window.active()
local pt = fg and host.element.locateVia(fg.id, "Kontakt 8", U.Pane, "Kontakt File Menu", U.Button)
if pt then
  host.input.click(pt.x, pt.y)
else
  host.log.info("no 'Kontakt File Menu' inside the 'Kontakt 8' pane")
end
```

### Returns {#host-element-locatevia-returns}

- `{ x, y }` — the screen centre of the first target inside the first matching container, whole numbers.
- `nil` — no container matches, the container holds no target, or the target has no rectangle there. Only the first container is searched. A window that is gone is `nil` too.
- `nil, err` — not known: the program did not answer at either level, the question was not put to it, or (macOS) a search stopped at its bound; `err.code` is under Errors.
- With a callback: nothing at once; `cb({ x, y })`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-locatevia-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [`find`'s errors](#host-element-find-errors) | both | The program did not answer at either level, the question was not put to it, or (macOS) a search stopped at its bound; each arrives as it does there. | as there | both |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `viaName` or `name` that is neither a string nor a number; a `viaType` or `controlType` that is not a number in the 32-bit range; and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

### Windows {#host-element-locatevia-windows}

Both levels are condition-based `FindFirst` calls: the container must itself be reachable by a condition-based search from `hwnd`, and there is no raw-walker pass here — where `pluginLocate` uses the raw walker for both of its levels.

### macOS {#host-element-locatevia-macos}

Control types are translated to accessibility roles, so an unmapped type at **either** level makes the call impossible — it can never match, and says so only in the log (see [`host.element.type`](#host-element-type)). Each level gets its own **1500 node / depth 24 / 300 ms** budget, and a search that stops at it and found nothing answers `nil, err`, as `find`'s does.

## host.element.stateProbe(hwnd, containerName, name, controlType, opts?, cb?) {#host-element-stateprobe}

**Signature:** `host.element.stateProbe(hwnd: number, containerName: string, name: string, controlType: number, opts: { key: string? }?, cb: function?) -> { toggle: number?, legacyState: number, checked: boolean } | nil | (nil, Error)`, and nothing with `cb`

Asks a named element what it reports about its **own** state, rather than inferring one from its control type. It exists because "the accessibility tree has nothing to offer here" turned out to be a conclusion drawn from the type — Kontakt's status-bar toggles publish as plain Buttons — by code that had only ever fetched Name, ClassName and ControlType and so had never observed the absence of a state, only assumed it. `nil` means the element was not found at all, which stays distinguishable from "found, and it says nothing" — and `nil, err` that the program did not answer.

It is **expensive**: the same raw tree walk as `pluginLocate`, measured at 60–300 ms on the message pump against roughly 17 ms for a single pixel read through the standard path. It is the fallback for what a cheap pixel probe cannot answer, never the primary route. And a missing `toggle` is not "off" — Kontakt's Side and Info panes carry no state at all, so a module that turned that into `false` would be inventing an answer.

As with `find`, an **empty `name`** matches any element of the given type — the first the walk meets — on both platforms.

```luau
local U = host.element.type
local fg = host.window.active()
-- The state of the first check box inside the plug-in, whatever it is called.
local st = fg and host.element.stateProbe(fg.id, "Kontakt 8", "", U.CheckBox)
```

```luau
local U = host.element.type
-- Kontakt's status-bar toggles publish as plain Buttons, so the type says nothing and the
-- element has to be asked. Adapted from modules/kontakt/src/actions.luau (uiaChecked).
local KEYBOARD = "Keyboard (F3): Toggles the visibility of the Keyboard panel, which shows the virtual keyboard."
local fg = host.window.active()
local st = fg and host.element.stateProbe(fg.id, "Kontakt 8", KEYBOARD, U.Button)
if not st then return nil end                       -- not found: no answer at all
if st.toggle ~= nil then return st.toggle == 1 end  -- 0 off, 1 on
-- Found, and it carries no toggle state: `st.checked` is false for an element that never
-- sets the bit, and reporting that as "collapsed" would be inventing an answer.
return nil
```

### Returns {#host-element-stateprobe-returns}

- `{ toggle?, legacyState, checked }` — the element was found. `toggle` is its toggle state, `0` off, `1` on, `2` anything else, and absent when it has none; `legacyState` its state word; `checked` whether bit `0x10` of that word is set. What each holds differs by platform (below).
- `nil` — no such element. On Windows that includes a window with no container of that name; a window that is gone is `nil` too.
- `nil, err` — not known: the program did not answer, the question was not put to it, or a walk stopped at its bound before the end of the tree and found nothing. `err.code` is under Errors.
- With a callback: nothing at once; `cb(state)`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-stateprobe-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [`find`'s errors](#host-element-find-errors) | both | The program did not answer, the question was not put to it, or (macOS) a walk stopped at its bound; each arrives as it does there. | as there | both |
| `too_large` | both | A walk read 4000 elements before the end of the tree, and found nothing. | `the tree holds more than the 4000 elements a search reads` | Windows |
| `timeout` | both | A walk took longer than 5 s before the end of the tree, and found nothing. | `the search took longer than 5 s` | Windows |

**Raises** for a mistake in the call, as [`pluginLocate`](#host-element-pluginlocate-errors) does.

### Windows {#host-element-stateprobe-windows}

`legacyState` is a real `LegacyIAccessible` state word, so bits beyond CHECKED (`0x10`) carry information: `0x08` PRESSED, `0x04` FOCUSED, `0x100000` FOCUSABLE. Read them with care — Kontakt's Side and Info panes were seen going `0x100000` → `0x100004` across a press, and that `0x4` is the consequence of our own click rather than a panel state.

The search is confined to elements named `containerName` with control type Window or Pane (the `ni::qt::QuickWindow` class preferred over the window host), and a window with no container of that name answers `nil` without looking further. The same walks as [`pluginLocate`](#host-element-pluginlocate), with the same bounds: one stopped at its bound before the end that found nothing answers `nil, err`. An empty `name` matches any element of the type, as on macOS and in every other call here, and any other is compared exactly; until 2026-10-24 an empty one matched only an element without a name here.

### macOS {#host-element-stateprobe-macos}

There is no state word here. `toggle` comes from `AXValue` — `0` off, `1` on, `2` for anything else, absent when the element carries no value — and `legacyState` is synthesised from it: `0x10` when `toggle` is `1`, otherwise `0`. Only `checked` is meaningful; the other bits are never set, so a module that reads PRESSED or FOCUSED gets a permanent zero.

If no container of that name holds the element — none exists, or none has it — the search falls back to the whole window rather than giving up: the container is a fragment boundary on Windows, and is not one here.

## host.element.classNavPoint(hwnd, className, controlType, child, sibling, opts?, cb?) {#host-element-classnavpoint}

**Signature:** `host.element.classNavPoint(hwnd: number, className: string, controlType: number, child: number, sibling: number, opts: { key: string? }?, cb: function?) -> { x: number, y: number } | nil | (nil, Error)`, and nothing with `cb`

Finds the first element whose class contains `className` and whose control type is `controlType`, walks a fixed path from it, and returns that element's click centre. For the case where the control you want carries no name of its own but sits at a known position next to one that is identifiable: Komplete Kontrol's library-browser toggle is the previous sibling of the browser pane, Kontakt's What's-New close button is the second child of its `WhatsNewScreen`. A port of ReaHotkey's `FindElement(ClassName)` + `WalkTree(path)`, and it walks the raw tree, so it reaches into a Qt plug-in's content that a condition-based search stops short of.

`child` is 1-based with `0` meaning "do not descend"; `sibling` is signed, negative walking backwards. `nil` covers both "not found" and "found but off-screen", which makes it usable as a presence probe — KK reads the `nil` as "no browser is open" and asks nothing else. An error value as the second value says the program did not answer, which is no presence probe's answer.

```luau
local U = host.element.type
-- KK's browser-close toggle has no name of its own: it is the PREVIOUS SIBLING of the
-- browser pane. Doubles as the "is a browser open?" probe — nil means closed.
-- From modules/komplete-kontrol/src/main.luau (browserToggle).
local function browserToggle(hwnd)
  if not hwnd then return nil end
  return host.element.classNavPoint(hwnd, "FileTypeSelector", U.Tab, 0, -1)
    or host.element.classNavPoint(hwnd, "TagCloudAccordionWithBrands", U.Pane, 0, -1)
end
```

### Returns {#host-element-classnavpoint-returns}

- `{ x, y }` — the screen centre of the element the path ends on, whole numbers.
- `nil` — no element with that class and type, a step of the path that leads nowhere (no such child, a sibling past either end), or an element with no rectangle there. A window that is gone is `nil` too.
- `nil, err` — not known: the program did not answer a step, the question was not put to it, or (macOS) a search stopped at its bound. Not a presence probe's "no". `err.code` is under Errors.
- With a callback: nothing at once; `cb({ x, y })`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-classnavpoint-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [`find`'s errors](#host-element-find-errors) | both | The program did not answer, the question was not put to it, or (macOS) a search stopped at its bound; each arrives as it does there. | as there | both |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `className` that is neither a string nor a number; a `controlType`, `child` or `sibling` that is not a number in the 32-bit range; and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

### Windows {#host-element-classnavpoint-windows}

Matches on ClassName containing the substring, searched depth-first over the raw view to depth 25, and the sibling steps are raw-view siblings.

### macOS {#host-element-classnavpoint-macos}

There are no class names. The first pass matches `AXIdentifier` containing the substring — Qt fills that from `objectName`, which is where the ReaHotkey class names came from in the first place — and only if that finds nothing does a second pass try `AXDescription`, for toolkits that leave the identifier empty. An identifier match therefore always beats a description match.

Siblings are counted within the parent's own children, and a step off either end returns `nil` rather than clamping.

## host.element.focusStep(hwnd, direction, opts?, cb?) {#host-element-focusstep}

**Signature:** `host.element.focusStep(hwnd: number, direction: number, opts: { key: string? }?, cb: function?) -> { name: string, ctype: number, index: number, count: number, bounds: { x: number, y: number, w: number, h: number }? } | nil | (nil, Error)`, and nothing with `cb`

**This one writes.** It enumerates the visible, keyboard-focusable descendants of `hwnd`'s content area and *moves keyboard focus* to the next (`direction >= 0`) or previous one, wrapping at the ends, returning the element it landed on. It is the Tab pass-through for a standalone plug-in window that does not move focus on Tab by itself (Kontakt standalone), and it is the only way into such a window's native controls. Because it raises a real focus event, the screen reader announces the element — so a caller does not speak `name` over it. What the screen reader cannot announce is a name the element does not have, and a plug-in's controls often have none, being drawn rather than labelled: `name` is `""` then, or whatever the toolkit published in its place — white space, a no-break space — since the host hands the name on as it is. [`Overlay:addPassThrough`](./overlay.md#o-addpassthrough) stays quiet on a stop with a name, and reads what is drawn in `bounds` for a stop whose name has no visible character.

`bounds` is the rectangle of the element the focus landed on, in screen coordinates — the space [`host.ocr.recognize`](./ocr.md#host-ocr-recognize) regions and [`host.input.click`](./input.md#host-input-click) take on the platform: `x`, `y` its top-left corner, `w`, `h` its size. It is **absent** (`nil`) when the platform gave no rectangle for the element, an empty one (`w` or `h` not above zero) or one whose numbers are not finite; it is never a table of zeros, unlike a dump's `bounds`. The host does not cut it to the window, to the screen or to a scroll view: it is the rectangle the platform reports. For an element that reaches past its window — a list longer than its view — that reaches past the window too, and an element scrolled out of its list's view is reported where it lies, over whatever else is drawn there. A caller that reads the rectangle cuts it to the window first, as the example does. Whether the numbers are whole, and when they were read, differs by platform — see below.

Candidates are re-enumerated every step, since the tree changes shape as the user moves; and a candidate that accepts the focus request without actually taking it is skipped, verified by reading focus straight back. What the ring is scoped to differs by platform — see below. It **wraps**, so "every stop has been visited" is not something it reports — the caller counts it. `Overlay:addPassThrough` counts net steps forward since Tab went in, taking the distance between two results' 1-based `index` values while `count` holds (so stops that refused the focus and were stepped over still count — except any skipped by the very first step, since this call does not say where it started from, which can cost one repeat of the entry element), and hands Tab back once that reaches `count`. It hands the key back **without** calling this, because calling it would move the plug-in's focus again and the screen reader would name an element over the overlay's own announcement. Shift+Tab comes back out the way Tab went in: on the element Tab went in on, it returns to the pass-through stop without calling this either. A `count` that changes mid-lap means the plug-in's tree changed (a panel was shown or hidden); the lap is measured against the latest `count`, so it can end early but never go on for ever. `nil` means nothing in the scope accepted focus at all, and `nil, err` that the program did not answer. A module normally gets this through `Overlay:addPassThrough`, which asks without a callback: it decides at the key whether the key goes on.

```luau
-- One press inside the pass-through control, shortened from modules/overlay-runtime/src/main.luau
-- (Overlay:_stepPassThrough). c._net is nil until Tab goes in.
if c._net == nil and dir < 0 then return false end    -- Shift+Tab on the stop: back through the overlay
if c._leaveOn == dir then                              -- every stop visited
  c._net, c._at, c._n, c._leaveOn = nil, nil, nil, nil
  return false                                         -- hand Tab back, the plug-in's focus untouched
end
local r = host.element.focusStep(ctrl.id, dir)
if not r then return false end                         -- nothing focusable in there: let Tab carry on
local d = 1
if c._net and r.count == c._n then
  d = ((r.index - c._at) * dir) % r.count              -- stops that refused the focus count too
  if d == 0 then d = r.count end                       -- nothing else took it: a whole lap
end
c._net = c._net and (c._net + (dir >= 0 and d or -d)) or 1
c._at, c._n = r.index, r.count
c._leaveOn = (dir >= 0 and c._net >= r.count) and 1 or nil
return true
```

```luau
-- A stop with no name: read what is drawn where it is, and say it without cutting off the
-- screen reader. Needs "element", "ocr", "speech" and "window" in the manifest.
local win = host.window.active()                            -- the plug-in's window, in front
local r = win and host.element.focusStep(win.id, 1)
if r and not r.name:find("%S") and r.bounds then
  local b, wb = r.bounds, win.bounds
  -- Whole numbers covering the element (a Mac's may carry a fraction), cut to the window:
  -- what lies past it is another window's text, and a huge list would pass the read's limit.
  local x1, y1 = math.max(math.floor(b.x), wb.x), math.max(math.floor(b.y), wb.y)
  local x2 = math.min(math.ceil(b.x + b.w), wb.x + wb.w)
  local y2 = math.min(math.ceil(b.y + b.h), wb.y + wb.h)
  if x2 - x1 >= 2 and y2 - y1 >= 2 then
    host.ocr.recognize({ x1, y1, x2, y2 }, { key = "stop" }, function(reading)
      if reading.newer or reading.status ~= "text" then return end   -- Tab again, or nothing there
      host.speech.output(reading.lines[1].text, { interrupt = false })
    end)
  end
end
```

### Returns {#host-element-focusstep-returns}

- `{ name, ctype, index, count, bounds? }` — the element the focus landed on: its name as published (`""` for none), its control type, its 1-based place in the ring, how many stops the ring holds now, and its rectangle when the platform gave one.
- `nil` — nothing in the scope took the focus. Also for a window that is gone, and on macOS while the keyboard is in this application.
- `nil, err` — not known: the program did not answer, the question was not put to it, or (macOS) the walk stopped at its bound before it found a stop. The focus may have moved before the program stopped answering; read [`host.window.focusChain()`](./window.md#host-window-focuschain) before saying where it is. `err.code` is under Errors.
- With a callback: nothing at once; `cb(stop)`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-focusstep-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [the question errors](#question-errors) | both | The program did not answer, or the question was not put to it; each arrives as its row there says. | as there | Windows |
| `timeout` | both | A read of the walk went unanswered — the application is busy, starting or not responding — and nothing took the focus. | `the program did not answer: it is busy, starting or not responding` | macOS |
| `timeout` | both | The application did not answer a read a moment ago, and is not asked for 5 s after it: nothing is moved. | `the program did not answer a moment ago, so it is not asked for 5 s` | macOS |
| `too_large` | both | The walk read 800 elements before the end of the window and found no stop that took the focus. | `the window holds more than the 800 elements a focus search reads` | macOS |
| `timeout` | both | The walk took longer than 300 ms before the end of the window and found no stop that took the focus. | `the search took longer than 300 ms` | macOS |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `direction` that is not a number in the 32-bit range; and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

### Windows {#host-element-focusstep-windows}

The ring is scoped to the window's first child — ReaHotkey's content area — which drops the frame and the menu bar without a control-type blocklist. Candidates are the descendants the provider reports as keyboard-focusable and not off-screen, and focus is moved with UIA's own `SetFocus`. No traversal budget of ours applies — the whole content subtree is enumerated however large it is.

`bounds` is the landed element's `CurrentBoundingRectangle`, read once, right after the focus has landed and been read back, at the cost of one more cross-process property read per step. It has whatever the plug-in did to the element while taking the focus — a list it scrolled to the element inside its focus handling puts the element where it is now — but a scroll the plug-in animates after that is not waited for: the rectangle is the one at that read. Whole physical pixels: the application is per-monitor DPI aware, so UIA hands it the same unscaled coordinates the screen reads use.

### macOS {#host-element-focusstep-macos}

While the keyboard is in this application — one of its windows has it, it is the frontmost application, or it has asked for the front and has no answer yet — nothing is moved and the answer is `nil`, with a line in the log: where the keyboard is would have to be asked of this application itself.

The ring is the **whole window**, minus the window's own close, minimise, zoom and full-screen buttons, which are skipped by subrole along with anything under them. Not the first child, as on Windows: on macOS that is whatever the application published first, and in a standalone Kontakt 7 it is Kontakt's logo — a button with no children, which made the ring empty. The menu bar is not inside a window on this platform, so nothing else needs excluding. Focus is moved by setting `AXFocused`, and a candidate qualifies when that attribute is *settable* and the element has a non-zero rectangle. The walk is bounded at **800 nodes, depth 20 and 300 ms** — tighter than the other calls here, because it runs per keystroke. Elements below depth 20 are simply not in the ring, with nothing in the log to say so. A walk that stops at its 800 nodes or its 300 ms before the end of the window is a ring of what it found: a step on it is made, and one that found no stop answers `nil, err` — `too_large`, `the window holds more than the 800 elements a focus search reads`, or `timeout`, `the search took longer than 300 ms` — with one line in the log once per session.

`bounds` is the element's `AXPosition` and `AXSize` as the walk read them, in the same request as its name, so it costs nothing more — and it is where the element was **before** the focus moved: one that scrolls itself into view to take the focus is reported where it was. The walk does not look at scroll views, so a row below its list's viewport is in the ring, and is reported where it lies — where something else is drawn. Points, top-left of the primary display, as Accessibility reports them, which can carry a fraction; widen them to whole numbers before reading the region, as the example does. Every candidate needs a non-zero rectangle to be in the ring, so a stop landed on here has `bounds` unless Accessibility reported numbers that are not finite.

## host.element.focusWithin(hwnd, rect, opts?, cb?) {#host-element-focuswithin}

**Signature:** `host.element.focusWithin(hwnd: number, rect: { x: number, y: number, w: number, h: number }, opts: { key: string? }?, cb: function?) -> { name: string, ctype: number } | nil | (nil, Error)`, and nothing with `cb`

**This one writes.** It gives keyboard focus to the **first** keyboard-focusable element of `hwnd` whose centre lies inside `rect` (screen coordinates), and returns the element it landed on. It is the polite half of "put the keyboard back into the plug-in": focusing the *window* hands the keyboard to whatever that window last had focused — in REAPER, its FX list — and the only thing that moved it on from there used to be a click into the plug-in's panel. A plug-in that publishes its elements can simply be asked, which is what a screen reader's own navigation does, raises a real focus event, and presses nothing. `nil` means nothing inside the rectangle took focus — a plug-in that publishes no element has nothing to ask, and the caller falls back to what the platform does understand. `nil, err` means it was not known: not answered, or not asked ([Asking without waiting](#asking-without-waiting)).

Because it raises a real focus event, the screen reader announces the element; a caller that speaks afterwards should not name it again. Named after a comma, a button called 'Play View' sounded to a tester like a state the plug-in was in.

```luau
-- modules/daw-hosts/src/main.luau, the F6 shortcut on macOS: ask before clicking.
local given = host.element.focusWithin(w.id, panel)
if given then
  host.speech.output(title)                     -- the screen reader names the element itself
else
  host.input.click(panel.x + 3, panel.y + 3)   -- sforzando publishes nothing to ask
end
```

### Returns {#host-element-focuswithin-returns}

- `{ name, ctype }` — the element that took the focus: its name as published and its control type. macOS only.
- `nil` — nothing inside the rectangle took the focus. Always on Windows, where it is not implemented; on macOS also while the keyboard is in this application, and for a window that is gone.
- `nil, err` — not known. On Windows the question was not put to the program: nothing is asked of the program there, so no error of the program's comes, only the host's own. On a Mac the application did not answer a read, was not asked, or the walk stopped at its bound before a candidate took the focus. `err.code` is under Errors.
- With a callback: nothing at once; `cb(element)`, `cb(nil)` or `cb(nil, err)` on a later turn.

### Errors {#host-element-focuswithin-errors}

None of these is raised.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `timeout` | both | The question waited behind one to the same program that did not answer in time, and was not put to it; the message begins with the earlier one's. | `… to an earlier question, so this one was not asked` | Windows |
| `replaced` | callback | With `opts.key`: a newer question of the module with the same key came before this one was asked. | `a newer question with the same key was asked before this one` | Windows |
| `busy` | return | Without a callback, asked while the event loop already waits for another question. Nothing in the host asks one so. | `another accessibility question is being waited for` | Windows |
| `refused` | both | `hwnd` is a window of this application. | `the window is this application's own, which is not asked about` | Windows |
| `failed` | both | The host could not start its accessibility thread, or the thread's code failed. | `the host could not start its accessibility thread: …`, `the host's accessibility thread failed: …` | Windows |
| `failed` | return | Without a callback: the event the host waits on could not be made, or the answer did not come back. Neither is known to happen. | `the event the host waits on for the answer could not be made`, `the answer was lost` | Windows |
| `timeout` | both | A read of the walk went unanswered — the application is busy, starting or not responding — and nothing took the focus. | `the program did not answer: it is busy, starting or not responding` | macOS |
| `timeout` | both | The application did not answer a read a moment ago, and is not asked for 5 s after it: nothing is moved. | `the program did not answer a moment ago, so it is not asked for 5 s` | macOS |
| `too_large` | both | The walk read 800 elements before the end of the window and no candidate took the focus. | `the window holds more than the 800 elements a focus search reads` | macOS |
| `timeout` | both | The walk took longer than 300 ms before the end of the window and no candidate took the focus. | `the search took longer than 300 ms` | macOS |

**Raises** for a mistake in the call: an `hwnd` that is not a number; a `rect` that is not a table, or whose `x`, `y`, `w` or `h` is missing or not a number in the 32-bit range (`error converting Lua nil to i32 (expected number or string coercible to number)`); and the `opts` and callback mistakes under [Asking without waiting](#asking-without-waiting).

### Windows {#host-element-focuswithin-windows}

Not implemented — returns `nil`. Screen-reader users have OSARA's F6 there, and the plug-in host's own controls are reachable by Tab.

### macOS {#host-element-focuswithin-macos}

Walked from the window itself, not from its first child as `focusStep` is: inside REAPER's FX window Kontakt's elements *are* the window's children, with no content group to scope to. The rectangle is the scope instead. A candidate qualifies when `AXFocused` is settable and its centre is inside the rectangle; the first in tree order that actually takes the focus (read back from the system-wide focused element, not assumed) wins, so a text field near the top of a panel is what usually gets it. Same bounds as `focusStep`, and the same `nil` while the keyboard is in this application. One log line either way, naming the element and its role, or the number of candidates that accepted the question and then did not take the focus. Unverified on hardware as of 2026-09-11.

## host.element.press(hwnd, name, controlType, opts?, cb?) {#host-element-press}

**Signature:** `host.element.press(hwnd: number, name: string, controlType: number, opts: { key: string? }?, cb: ((pressed: boolean, err: Error?) -> ())?) -> (boolean, Error?)`, and nothing with `cb`

**This one acts.** It asks an element of `hwnd` to perform its own action, the one a screen reader's "activate" uses — a button is pressed, a check box toggled — and **the keyboard focus stays where it is**. The element is found as [`find`](#host-element-find) finds it: the first in the tree with that Name and ControlType, an empty `name` matching any element of the type. No pointer and no click: the window does not have to be in front or uncovered, and the window in front — a page beside the plug-in — keeps the keyboard, so nothing is announced but what the plug-in itself says about the change. It is the first thing to try in a plug-in that publishes its controls (a [`dump`](#host-element-dump) shows their names). One that publishes nothing — Kontakt in a DAW, VPS Avenger — has nothing to press, and a click is what is left ([`host.input.click`](./input.md#host-input-click), with `via` for one that leaves the window in front where it is).

**Answers** `true` when the program accepted the action — not that it did what you wanted: read the result where it shows — and `false, err` when it did not, `err.code` saying why ([Errors](#host-element-press-errors)).

**Where it waits.** Each step is a call into the other program, and a program takes its time to answer — a plug-in's accessibility runs on its DAW's own thread, and some act before they answer, a button that opens a dialog say — so each call is bounded at 1000 ms. As with every call on this page ([Asking without waiting](#asking-without-waiting)): without `cb` the event loop waits for the press; with `cb` the call returns at once, with nothing, and `cb(true)` or `cb(false, err)` runs on a later turn of the event loop, as a handler of the module, after whatever the module is busy with — and not at all when the module is disabled, reloaded, removed or stopped first, when a press still waiting to be made is not made either. A press is never shared with another, nor answered from the epoch's store: every call presses. Either way the call turns over [`host.inputEpoch()`](./timer.md#host-inputepoch), as a click does, before the press is asked, and no question to the same program asked after it is asked before it.

```luau
-- A plug-in's "Bypass" button (the name a dump shows), pressed from a page that stays in front.
-- Needs "element" and "speech" in the manifest; `plugin` is the plug-in window's table.
host.element.press(plugin.id, "Bypass", host.element.type.Button, function(pressed, err)
  if not pressed then host.speech.output("Could not press Bypass: " .. err) end
end)
```

### Returns {#host-element-press-returns}

- `true` — the program accepted the action. Not that it did what you wanted: read the result where it shows.
- `false, err` — no action was taken, or the program did not accept it; `err.code` is under Errors. After a `timeout` the program may still act on it later.
- With a callback: nothing at once; `cb(true)` or `cb(false, err)` on a later turn of the event loop.

A module written for both platforms tells the press that cannot be made here by its code, and clicks instead:

```luau
local U = host.element.type
local ok, err = host.element.press(plugin.id, "Bypass", U.Button)
if not ok and err.code == host.errors.NOT_SUPPORTED then
  local p = host.element.locate(plugin.id, "Bypass", U.Button)   -- no press here: click it
  if p then host.input.click(p.x, p.y) end
elseif not ok then
  host.log.info("Bypass not pressed: " .. err)
end
```

### Errors {#host-element-press-errors}

None of these is raised. Each comes beside `false`, as the second value or the callback's second argument.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| any of [the question errors](#question-errors) | both | As for every question: the program did not answer a step within 1000 ms, runs with higher rights or went away, the press was not put to it, or the system's automation object could not be made. Each arrives as its row there says. | as there | Windows |
| `gone` | both | No window has that id, `0` included, and nothing is asked. Or the element went away while it was pressed. | `the window is gone`, `the element is gone` | Windows |
| `not_found` | both | No element in the tree has that Name and ControlType. | `no such element` | Windows |
| `refused` | both | The element offers no Invoke, no Toggle and no default action, or the program said the action is not supported or not valid now. Or the element is not enabled. | `the element cannot be pressed`, `the element is not enabled` | Windows |
| `failed` | both | The program answered the action with another error, its code in hex. | `the program did not take the action (0x…)` | Windows |
| `not_supported` | both | This Windows cannot keep UI Automation from moving the keyboard focus (no `IUIAutomation2`, before Windows 8), so no press is made. | `this system cannot press an element without moving the keyboard focus to it` | Windows |
| `not_supported` | both | A Mac, where a press is not there yet, and the build for other systems. Nothing is asked. | `not supported on this platform yet` | macOS, other |

**Raises** for a mistake in the call, as [`find`](#host-element-find-errors) does.

### Windows {#host-element-press-windows}

On the accessibility thread, as every question ([Asking without waiting](#asking-without-waiting)). UI Automation: one `FindFirst` over the subtree, then the element's Invoke pattern (`Invoke`), else its Toggle pattern (`Toggle`), else LegacyIAccessible's default action (`DoDefaultAction`) — the last only when the element names one. UI Automation moves the keyboard focus to an element before it acts on it unless told not to, so every automation object of the host's is made with `AutoSetFocus` off (`IUIAutomation2`, Windows 8 and later); where that cannot be set, a press answers `false, err` with `not_supported`, `this system cannot press an element without moving the keyboard focus to it`, rather than move it. What follows is the program's: JUCE's buttons (6.1 and later, where the plug-in leaves accessibility on) post their click to the plug-in's own thread; the system's proxy for a standard Windows button sends it `BM_CLICK`, which Microsoft says might fail while its dialog is not the active window; a provider that moves the focus itself as it acts still does. One line in the log per press:

`[uia] pressed 'Bypass' (type 50000) in window 2755348 by Invoke, the keyboard focus left where it was`

or `[uia] could not press 'Bypass' (type 50000) in window 2755348: no such element`.

### macOS {#host-element-press-macos}

Not there yet: every press answers `false, err` — code `not_supported`, message `not supported on this platform yet` — and the callback the same. The way it would go is `AXPress`; until a Mac has measured it, a module written for both platforms clicks there.

## host.element.pending(key) {#host-element-pending}

Whether a question of this module with `key`, asked with a callback, is still out: not answered to its callback yet, nor dropped.

**Signature:** `host.element.pending(key: string) -> boolean`

- `true` from the call that asked until its callback is handed the answer, and while that answer waits in the module's mailbox behind the module's other events — a question replaced by a newer one with the same key too, until its callback has been told so. `false` inside the question's own callback (the answer has been handed over by then), after the module was disabled, reloaded or removed, or stopped by the host's guard, which drops its questions, and for a call made without a callback, which nothing can see while the event loop waits for it.
- Asks about the module's own questions only: another module's with the same key are not counted.
- Costs a look-up in a table the event loop keeps: microseconds, and nothing is asked of any program.

It is "one question at a time" without a flag of the module's own: a flag set before a question stays set when a disable drops the question and its callback never comes, while a dropped question is simply not pending.

```luau
-- A poll that asks whether a menu is up in the plug-in, with one question out at a time across
-- a disable too. Needs "element", "timer" and "speech" in the manifest; `plugin` is the
-- plug-in window's table.
local U = host.element.type
local open = false
host.timer.every(250, function()
  if host.element.pending("menu") then return end -- the last one has not answered yet
  host.element.find(plugin.id, "", U.Menu, { key = "menu" }, function(found, err)
    if err ~= nil or found == open then return end -- not known, or nothing changed
    open = found
    host.speech.output(open and "Menu open" or "Menu closed", { interrupt = false })
  end)
end)
```

### Returns {#host-element-pending-returns}

- `true` — a question of this module with `key`, asked with a callback, is out: its callback has not been handed the answer yet.
- `false` — none is: never asked, answered, dropped, or asked without a callback.

### Errors {#host-element-pending-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `key` that is not a string (`host.element.pending: the key must be a non-empty string, got nil`), and an empty one (`host.element.pending: the key is empty`).

### Windows {#host-element-pending-windows}

A question is out from the call until its callback has run, whether the accessibility thread asks it, it shares another's answer, or the epoch's store answers it at the call: every answer comes on a later turn.

### macOS {#host-element-pending-macos}

The questions are asked on the event loop at the call, and their callbacks still run on a later turn: `pending` is `true` for that turn, as on Windows.
